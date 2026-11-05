//! 512-byte block device over the flash volume with a small multi-sector write-back
//! cache. Flash erases 4 KiB at a time, so writes are buffered and committed only when
//! the bus goes idle or on flush. Caching several sectors keeps a whole small-file write
//! (data + FAT + directory) in RAM, so no blocking erase stalls the USB stack mid-transfer.

use super::flash::Flash;
use super::layout::{FLASH_SECTOR_SIZE, VOLUME_BLOCKS, VOLUME_OFFSET};
use redusb_core::block::{BLOCK_SIZE, Block, BlockDevice, BlockError};

const BLOCKS_PER_SECTOR: u32 = (FLASH_SECTOR_SIZE / BLOCK_SIZE) as u32;

/// Commit pending writes after this much time without a new write.
pub const IDLE_FLUSH_MS: u64 = 150;

/// Sectors held in RAM at once. Four covers a small write's data sector, both FAT
/// copies and the root directory, so an ordinary upload never evicts mid-transfer.
const CACHE_SECTORS: usize = 4;

/// One cached 4 KiB flash sector.
struct Slot {
    /// Flash offset of the sector held here, or `None` when the slot is free.
    offset: Option<u32>,
    dirty: bool,
    buf: [u8; FLASH_SECTOR_SIZE],
}

impl Slot {
    fn empty() -> Self {
        Self {
            offset: None,
            dirty: false,
            buf: [0xFF; FLASH_SECTOR_SIZE],
        }
    }
}

pub struct CachedFlashDevice {
    flash: Flash,
    slots: [Slot; CACHE_SECTORS],
    /// Round-robin victim used only when every slot is already dirty.
    next_victim: usize,
    now_ms: u64,
    last_write_ms: u64,
    dirty: bool,
}

impl CachedFlashDevice {
    pub fn new(flash: Flash) -> Self {
        Self {
            flash,
            slots: core::array::from_fn(|_| Slot::empty()),
            next_victim: 0,
            now_ms: 0,
            last_write_ms: 0,
            dirty: false,
        }
    }

    /// Direct access to the flash, for data living outside the volume.
    pub fn flash(&mut self) -> &mut Flash {
        &mut self.flash
    }

    /// Advances the device clock and commits every cached sector once writes have
    /// paused. Returns `true` when at least one sector was programmed.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        self.now_ms = now_ms;
        if self.dirty && now_ms.wrapping_sub(self.last_write_ms) >= IDLE_FLUSH_MS {
            return self.commit_all();
        }
        false
    }

    fn sector_offset(lba: u32) -> u32 {
        let block_offset = VOLUME_OFFSET + lba * BLOCK_SIZE as u32;
        block_offset - block_offset % FLASH_SECTOR_SIZE as u32
    }

    fn check(lba: u32) -> Result<(), BlockError> {
        if lba < VOLUME_BLOCKS {
            Ok(())
        } else {
            Err(BlockError::OutOfRange)
        }
    }

    /// Slot index currently holding `offset`, if any.
    fn find(&self, offset: u32) -> Option<usize> {
        self.slots.iter().position(|s| s.offset == Some(offset))
    }

    /// Returns a slot holding `offset`, loading it from flash when it is not cached.
    /// A load may have to commit and reuse another slot, but only when every slot is
    /// already dirty (a write larger than the cache) — never for an ordinary upload.
    fn slot_for(&mut self, offset: u32) -> usize {
        if let Some(i) = self.find(offset) {
            return i;
        }
        let i = self.pick_slot();
        self.flash.read(offset, &mut self.slots[i].buf);
        self.slots[i].offset = Some(offset);
        self.slots[i].dirty = false;
        i
    }

    /// Picks a slot to reuse: a free one, then a clean one, otherwise the round-robin
    /// victim, which is committed first.
    fn pick_slot(&mut self) -> usize {
        if let Some(i) = self.slots.iter().position(|s| s.offset.is_none()) {
            return i;
        }
        if let Some(i) = self.slots.iter().position(|s| !s.dirty) {
            return i;
        }
        let i = self.next_victim;
        self.next_victim = (self.next_victim + 1) % CACHE_SECTORS;
        self.commit_slot(i);
        i
    }

    /// Writes one slot back if it changed. Returns whether an erase happened.
    fn commit_slot(&mut self, i: usize) -> bool {
        if !self.slots[i].dirty {
            return false;
        }
        let mut programmed = false;
        if let Some(offset) = self.slots[i].offset {
            // Hosts often rewrite identical FAT/directory sectors; skip those erases.
            if self.flash.view(offset, FLASH_SECTOR_SIZE) != self.slots[i].buf {
                defmt::debug!("flash: program sector @ {=u32:#x}", offset);
                self.flash
                    .erase_and_program_sector(offset, &self.slots[i].buf);
                programmed = true;
            }
        }
        self.slots[i].dirty = false;
        programmed
    }

    /// Writes every dirty slot back. Returns whether any erase happened.
    fn commit_all(&mut self) -> bool {
        let mut programmed = false;
        for i in 0..CACHE_SECTORS {
            programmed |= self.commit_slot(i);
        }
        self.dirty = false;
        programmed
    }
}

impl BlockDevice for CachedFlashDevice {
    fn block_count(&self) -> u32 {
        VOLUME_BLOCKS
    }

    fn read_block(&mut self, lba: u32, buf: &mut Block) -> Result<(), BlockError> {
        Self::check(lba)?;
        let offset = VOLUME_OFFSET + lba * BLOCK_SIZE as u32;
        let sector = Self::sector_offset(lba);
        match self.find(sector) {
            Some(i) if self.slots[i].dirty => {
                let start = (offset - sector) as usize;
                buf.copy_from_slice(&self.slots[i].buf[start..start + BLOCK_SIZE]);
            }
            _ => self.flash.read(offset, buf),
        }
        Ok(())
    }

    fn write_block(&mut self, lba: u32, buf: &Block) -> Result<(), BlockError> {
        Self::check(lba)?;
        let sector = Self::sector_offset(lba);
        let i = self.slot_for(sector);
        let start = ((lba % BLOCKS_PER_SECTOR) * BLOCK_SIZE as u32) as usize;
        self.slots[i].buf[start..start + BLOCK_SIZE].copy_from_slice(buf);
        self.slots[i].dirty = true;
        self.dirty = true;
        self.last_write_ms = self.now_ms;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        self.commit_all();
        Ok(())
    }
}
