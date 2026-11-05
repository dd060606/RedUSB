//! Persisted settings as an append-only log in the last flash sector: each record is
//! one 256-byte page written to the next blank page, so the sector erases once per 16 saves.

use super::flash::Flash;
use super::layout::{FLASH_PAGE_SIZE, FLASH_SECTOR_SIZE, SETTINGS_OFFSET};

const MAGIC: [u8; 4] = *b"RUSB";
const RECORDS_PER_SECTOR: u32 = (FLASH_SECTOR_SIZE / FLASH_PAGE_SIZE) as u32;
/// Bytes of a record that carry data; the rest of the page stays erased.
const RECORD_LEN: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, defmt::Format)]
pub struct Settings {
    /// Index of the selected script in the sorted catalog.
    pub selected: u8,
}

impl Settings {
    fn encode(&self) -> [u8; RECORD_LEN] {
        let mut rec = [0u8; RECORD_LEN];
        rec[..4].copy_from_slice(&MAGIC);
        rec[4] = self.selected;
        rec[5] = !self.selected;
        rec
    }

    fn decode(rec: &[u8]) -> Option<Self> {
        if rec[..4] != MAGIC || rec[4] != !rec[5] {
            return None;
        }
        Some(Self { selected: rec[4] })
    }

    fn page_offset(slot: u32) -> u32 {
        SETTINGS_OFFSET + slot * FLASH_PAGE_SIZE as u32
    }

    fn is_blank(flash: &Flash, slot: u32) -> bool {
        flash
            .view(Self::page_offset(slot), RECORD_LEN)
            .iter()
            .all(|&b| b == 0xFF)
    }

    /// Returns the most recent valid record, or defaults.
    pub fn load(flash: &Flash) -> Self {
        let mut latest = Self::default();
        for slot in 0..RECORDS_PER_SECTOR {
            if Self::is_blank(flash, slot) {
                break;
            }
            if let Some(settings) = Self::decode(flash.view(Self::page_offset(slot), RECORD_LEN)) {
                latest = settings;
            }
        }
        latest
    }

    /// Appends a record, erasing the sector when it is full. Skips the write when
    /// the stored value is already current.
    pub fn store(&self, flash: &mut Flash) {
        if Self::load(flash) == *self {
            return;
        }
        let mut page = [0xFFu8; FLASH_PAGE_SIZE];
        page[..RECORD_LEN].copy_from_slice(&self.encode());
        match (0..RECORDS_PER_SECTOR).find(|&slot| Self::is_blank(flash, slot)) {
            Some(slot) => flash.program_page(Self::page_offset(slot), &page),
            None => {
                let mut sector = [0xFFu8; FLASH_SECTOR_SIZE];
                sector[..FLASH_PAGE_SIZE].copy_from_slice(&page);
                flash.erase_and_program_sector(SETTINGS_OFFSET, &sector);
            }
        }
        defmt::info!("settings saved: {}", self);
    }
}
