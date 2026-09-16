//! 512-byte block device over the flash volume, with a one-sector write-back cache.
//!
//! USB hosts write 512-byte blocks but the flash erases 4 KiB at a time. Writes are
//! collected in a RAM copy of the affected flash sector and committed when the host
//! moves on to another sector, when the bus goes idle, or on an explicit flush.

use super::flash::Flash;
use super::layout::{FLASH_SECTOR_SIZE, VOLUME_BLOCKS, VOLUME_OFFSET};
use redusb_core::block::{BLOCK_SIZE, Block, BlockDevice, BlockError};

const BLOCKS_PER_SECTOR: u32 = (FLASH_SECTOR_SIZE / BLOCK_SIZE) as u32;

/// Commit pending writes after this much time without a new write.
pub const IDLE_FLUSH_MS: u64 = 150;

pub struct CachedFlashDevice {
    flash: Flash,
    /// Flash offset of the cached sector, if any.
    cached: Option<u32>,
    dirty: bool,
    buf: [u8; FLASH_SECTOR_SIZE],
    now_ms: u64,
    last_write_ms: u64,
}

impl CachedFlashDevice {
    pub fn new(flash: Flash) -> Self {
        Self {
            flash,
            cached: None,
            dirty: false,
            buf: [0xFF; FLASH_SECTOR_SIZE],
            now_ms: 0,
            last_write_ms: 0,
        }
    }

    /// Direct access to the flash, for data living outside the volume.
    pub fn flash(&mut self) -> &mut Flash {
        &mut self.flash
    }

    /// Advances the device clock and commits the cache once writes have paused.
    /// Returns `true` when a commit happened.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        self.now_ms = now_ms;
        if self.dirty && now_ms.wrapping_sub(self.last_write_ms) >= IDLE_FLUSH_MS {
            self.commit();
            return true;
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

    /// Makes the sector containing `lba` the cached one, committing the previous one.
    fn load(&mut self, lba: u32) {
        let offset = Self::sector_offset(lba);
        if self.cached == Some(offset) {
            return;
        }
        self.commit();
        self.flash.read(offset, &mut self.buf);
        self.cached = Some(offset);
    }

    /// Writes the cached sector back if it changed.
    fn commit(&mut self) {
        if !self.dirty {
            return;
        }
        if let Some(offset) = self.cached {
            // Hosts often rewrite identical FAT/directory sectors; skip those erases.
            if self.flash.view(offset, FLASH_SECTOR_SIZE) != self.buf {
                defmt::debug!("flash: program sector @ {=u32:#x}", offset);
                self.flash.erase_and_program_sector(offset, &self.buf);
            }
        }
        self.dirty = false;
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
        if self.dirty && self.cached == Some(sector) {
            let start = (offset - sector) as usize;
            buf.copy_from_slice(&self.buf[start..start + BLOCK_SIZE]);
        } else {
            self.flash.read(offset, buf);
        }
        Ok(())
    }

    fn write_block(&mut self, lba: u32, buf: &Block) -> Result<(), BlockError> {
        Self::check(lba)?;
        self.load(lba);
        let start = ((lba % BLOCKS_PER_SECTOR) * BLOCK_SIZE as u32) as usize;
        self.buf[start..start + BLOCK_SIZE].copy_from_slice(buf);
        self.dirty = true;
        self.last_write_ms = self.now_ms;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        self.commit();
        Ok(())
    }
}
