//! Raw QSPI flash access. Reads go through the memory-mapped XIP window; erase and
//! program run from RAM with interrupts disabled, since the flash can't serve XIP then.

use super::layout::{FLASH_PAGE_SIZE, FLASH_SECTOR_SIZE, FLASH_SIZE};
use portable_atomic::{AtomicBool, Ordering};

/// Base address of the execute-in-place flash window.
const XIP_BASE: usize = 0x1000_0000;

/// Use the copy of the second-stage bootloader to re-enter fast XIP mode after a
/// write. Correct whenever the firmware itself was booted through boot2.
const USE_BOOT2: bool = true;

static TAKEN: AtomicBool = AtomicBool::new(false);

/// Exclusive handle on the flash. Only one exists.
pub struct Flash {
    _private: (),
}

impl Flash {
    /// Claims the flash. Panics if called twice.
    pub fn take() -> Self {
        assert!(!TAKEN.swap(true, Ordering::SeqCst), "flash already taken");
        Self { _private: () }
    }

    /// Memory-mapped view of `len` bytes at `offset`.
    pub fn view(&self, offset: u32, len: usize) -> &[u8] {
        assert!(
            offset as usize + len <= FLASH_SIZE as usize,
            "flash read out of range"
        );
        // SAFETY: XIP maps the whole flash read-only, the range is checked, and the borrow ties to `&self`.
        unsafe { core::slice::from_raw_parts((XIP_BASE + offset as usize) as *const u8, len) }
    }

    /// Copies `buf.len()` bytes from `offset` into `buf`.
    pub fn read(&self, offset: u32, buf: &mut [u8]) {
        buf.copy_from_slice(self.view(offset, buf.len()));
    }

    /// Erases the 4 KiB sector at `offset` and programs `data` into it.
    pub fn erase_and_program_sector(&mut self, offset: u32, data: &[u8; FLASH_SECTOR_SIZE]) {
        assert!(
            offset.is_multiple_of(FLASH_SECTOR_SIZE as u32),
            "sector offset misaligned"
        );
        assert!(
            offset + FLASH_SECTOR_SIZE as u32 <= FLASH_SIZE,
            "sector out of range"
        );
        cortex_m::interrupt::free(|_| {
            // SAFETY: interrupts off, single core running, `data` in RAM, range aligned and in flash.
            unsafe { rp2040_flash::flash::flash_range_erase_and_program(offset, data, USE_BOOT2) }
        });
    }

    /// Programs one 256-byte page. The page must have been erased beforehand.
    pub fn program_page(&mut self, offset: u32, data: &[u8; FLASH_PAGE_SIZE]) {
        assert!(
            offset.is_multiple_of(FLASH_PAGE_SIZE as u32),
            "page offset misaligned"
        );
        assert!(
            offset + FLASH_PAGE_SIZE as u32 <= FLASH_SIZE,
            "page out of range"
        );
        cortex_m::interrupt::free(|_| {
            // SAFETY: see `erase_and_program_sector`.
            unsafe { rp2040_flash::flash::flash_range_program(offset, data, USE_BOOT2) }
        });
    }
}
