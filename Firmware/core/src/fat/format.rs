//! Volume formatter used to initialise (or repair) the flash-backed FAT volume.

use super::FatError;
use super::bpb::{Bpb, FatType};
use super::dir::volume_label_entry;
use crate::block::{BLOCK_SIZE, Block, BlockDevice};

/// Media descriptor for fixed / removable disks.
const MEDIA_DESCRIPTOR: u8 = 0xF8;

/// Geometry requested from [`format_volume`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatParams {
    /// Total number of 512-byte sectors in the volume.
    pub total_sectors: u32,
    /// Sectors per allocation unit, a power of two.
    pub sectors_per_cluster: u8,
    /// Sectors before the first FAT (at least 1, the boot sector).
    pub reserved_sectors: u16,
    /// Number of 32-byte root directory slots.
    pub root_entries: u16,
    /// Space padded upper-case volume label.
    pub label: [u8; 11],
    /// The data region start is padded up to a multiple of this many sectors.
    pub align_sectors: u32,
    /// Volume serial number written in the boot sector.
    pub serial: u32,
}

impl FormatParams {
    /// Geometry tuned for a NOR flash with 4 KiB erase sectors: 4 KiB clusters and a
    /// data region aligned on 4 KiB so each cluster maps to exactly one flash sector.
    pub const fn for_flash(total_sectors: u32, label: [u8; 11]) -> Self {
        Self {
            total_sectors,
            sectors_per_cluster: 8,
            reserved_sectors: 8,
            root_entries: 512,
            label,
            align_sectors: 8,
            serial: 0x5245_4455,
        }
    }
}

/// Derived layout for a set of [`FormatParams`].
struct Layout {
    fat_type: FatType,
    sectors_per_fat: u32,
    root_dir_sectors: u32,
}

fn compute_layout(p: &FormatParams) -> Result<Layout, FatError> {
    let root_dir_sectors = (p.root_entries as u32 * 32).div_ceil(BLOCK_SIZE as u32);
    let fixed = p.reserved_sectors as u32 + root_dir_sectors;
    if p.total_sectors <= fixed + p.sectors_per_cluster as u32 {
        return Err(FatError::DeviceTooSmall);
    }
    // Size the FATs from an upper-bound cluster guess, iterating in case the FAT type flipped.
    let mut fat_type = FatType::Fat12;
    let mut sectors_per_fat = 0u32;
    for _ in 0..3 {
        let data_sectors = p.total_sectors - fixed - 2 * sectors_per_fat;
        let clusters = data_sectors / p.sectors_per_cluster as u32;
        let new_type = if clusters < 4085 {
            FatType::Fat12
        } else if clusters < 65525 {
            FatType::Fat16
        } else {
            return Err(FatError::Unsupported);
        };
        let fat_bytes = match new_type {
            FatType::Fat12 => ((clusters + 2) * 3).div_ceil(2),
            FatType::Fat16 => (clusters + 2) * 2,
        };
        let mut fat_sectors = fat_bytes.div_ceil(BLOCK_SIZE as u32);
        // Pad the FATs so the data region starts on an aligned sector.
        if p.align_sectors > 1 {
            while !(fixed + 2 * fat_sectors).is_multiple_of(p.align_sectors) {
                fat_sectors += 1;
            }
        }
        let stable = new_type == fat_type && fat_sectors == sectors_per_fat;
        fat_type = new_type;
        sectors_per_fat = fat_sectors;
        if stable {
            break;
        }
    }
    Ok(Layout {
        fat_type,
        sectors_per_fat,
        root_dir_sectors,
    })
}

fn build_boot_sector(p: &FormatParams, layout: &Layout) -> Block {
    let mut s = [0u8; BLOCK_SIZE];
    s[0..3].copy_from_slice(&[0xEB, 0x3C, 0x90]);
    s[3..11].copy_from_slice(b"REDUSB  ");
    s[11..13].copy_from_slice(&(BLOCK_SIZE as u16).to_le_bytes());
    s[13] = p.sectors_per_cluster;
    s[14..16].copy_from_slice(&p.reserved_sectors.to_le_bytes());
    s[16] = 2;
    s[17..19].copy_from_slice(&p.root_entries.to_le_bytes());
    if p.total_sectors < 0x1_0000 {
        s[19..21].copy_from_slice(&(p.total_sectors as u16).to_le_bytes());
    } else {
        s[32..36].copy_from_slice(&p.total_sectors.to_le_bytes());
    }
    s[21] = MEDIA_DESCRIPTOR;
    s[22..24].copy_from_slice(&(layout.sectors_per_fat as u16).to_le_bytes());
    s[24..26].copy_from_slice(&32u16.to_le_bytes());
    s[26..28].copy_from_slice(&2u16.to_le_bytes());
    s[36] = 0x80;
    s[38] = 0x29;
    s[39..43].copy_from_slice(&p.serial.to_le_bytes());
    s[43..54].copy_from_slice(&p.label);
    s[54..62].copy_from_slice(match layout.fat_type {
        FatType::Fat12 => b"FAT12   ",
        FatType::Fat16 => b"FAT16   ",
    });
    s[510] = 0x55;
    s[511] = 0xAA;
    s
}

/// Writes a fresh, empty FAT12/16 volume onto `dev` and returns its parsed geometry.
///
/// Only the boot sector, the FATs and the root directory are written; the data
/// region is left untouched. The caller is responsible for flushing the device.
pub fn format_volume<D: BlockDevice>(dev: &mut D, p: &FormatParams) -> Result<Bpb, FatError> {
    if dev.block_count() < p.total_sectors {
        return Err(FatError::DeviceTooSmall);
    }
    let layout = compute_layout(p)?;
    let boot = build_boot_sector(p, &layout);
    dev.write_block(0, &boot)?;

    let zero = [0u8; BLOCK_SIZE];
    for lba in 1..p.reserved_sectors as u32 {
        dev.write_block(lba, &zero)?;
    }

    let mut fat_head = [0u8; BLOCK_SIZE];
    fat_head[0] = MEDIA_DESCRIPTOR;
    fat_head[1] = 0xFF;
    fat_head[2] = 0xFF;
    if layout.fat_type == FatType::Fat16 {
        fat_head[3] = 0xFF;
    }
    let first_fat = p.reserved_sectors as u32;
    for copy in 0..2 {
        let start = first_fat + copy * layout.sectors_per_fat;
        dev.write_block(start, &fat_head)?;
        for lba in start + 1..start + layout.sectors_per_fat {
            dev.write_block(lba, &zero)?;
        }
    }

    let first_root = first_fat + 2 * layout.sectors_per_fat;
    let mut root_head = [0u8; BLOCK_SIZE];
    root_head[..32].copy_from_slice(&volume_label_entry(&p.label));
    dev.write_block(first_root, &root_head)?;
    for lba in first_root + 1..first_root + layout.root_dir_sectors {
        dev.write_block(lba, &zero)?;
    }

    Bpb::parse(&boot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MemBlockDevice;

    #[test]
    fn flash_geometry_is_fat12_and_aligned() {
        let mut dev = MemBlockDevice::new(14208);
        let params = FormatParams::for_flash(14208, *b"REDUSB     ");
        let bpb = format_volume(&mut dev, &params).unwrap();
        assert_eq!(bpb.fat_type, FatType::Fat12);
        assert_eq!(bpb.sectors_per_fat, 8);
        assert_eq!(bpb.first_data_sector, 56);
        assert_eq!(bpb.first_data_sector % 8, 0);
        assert_eq!(bpb.cluster_count, (14208 - 56) / 8);
    }

    #[test]
    fn one_sector_clusters_give_fat16() {
        let mut dev = MemBlockDevice::new(14208);
        let mut params = FormatParams::for_flash(14208, *b"NO NAME    ");
        params.sectors_per_cluster = 1;
        let bpb = format_volume(&mut dev, &params).unwrap();
        assert_eq!(bpb.fat_type, FatType::Fat16);
        assert!(bpb.cluster_count >= 4085);
        // The FAT must hold every cluster entry.
        assert!(bpb.sectors_per_fat as u32 * 256 >= bpb.cluster_count + 2);
    }

    #[test]
    fn rejects_small_devices() {
        let mut dev = MemBlockDevice::new(16);
        let params = FormatParams::for_flash(14208, *b"REDUSB     ");
        assert_eq!(
            format_volume(&mut dev, &params).unwrap_err(),
            FatError::DeviceTooSmall
        );
    }
}
