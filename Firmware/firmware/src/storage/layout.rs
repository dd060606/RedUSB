//! Flash memory map: 1 MiB firmware, then the ~7.1 MiB FAT volume exposed over USB,
//! 60 KiB reserved, and a 4 KiB settings sector at the top of the 8 MiB device.

use redusb_core::block::BLOCK_SIZE;

/// Total flash size of the KB2040.
pub const FLASH_SIZE: u32 = 8 * 1024 * 1024;
/// Smallest erasable unit.
pub const FLASH_SECTOR_SIZE: usize = 4096;
/// Smallest programmable unit.
pub const FLASH_PAGE_SIZE: usize = 256;

pub const VOLUME_OFFSET: u32 = 0x10_0000;
pub const VOLUME_SIZE: u32 = 0x6F_0000;
pub const VOLUME_BLOCKS: u32 = VOLUME_SIZE / BLOCK_SIZE as u32;
pub const VOLUME_LABEL: [u8; 11] = *b"REDUSB     ";

pub const SETTINGS_OFFSET: u32 = FLASH_SIZE - FLASH_SECTOR_SIZE as u32;

const _: () = {
    assert!(VOLUME_OFFSET.is_multiple_of(FLASH_SECTOR_SIZE as u32));
    assert!(VOLUME_SIZE.is_multiple_of(FLASH_SECTOR_SIZE as u32));
    assert!(VOLUME_OFFSET + VOLUME_SIZE <= SETTINGS_OFFSET);
    assert!(SETTINGS_OFFSET.is_multiple_of(FLASH_SECTOR_SIZE as u32));
    assert!(SETTINGS_OFFSET + FLASH_SECTOR_SIZE as u32 <= FLASH_SIZE);
};
