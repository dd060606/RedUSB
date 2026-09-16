//! BIOS Parameter Block parsing and derived volume geometry.

use super::FatError;
use crate::block::{BLOCK_SIZE, Block};

/// FAT flavour, decided purely by the number of data clusters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatType {
    Fat12,
    Fat16,
}

/// Parsed boot sector fields plus the derived sector layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bpb {
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub fat_count: u8,
    pub root_entries: u16,
    pub total_sectors: u32,
    pub sectors_per_fat: u16,
    pub fat_type: FatType,
    pub root_dir_sectors: u32,
    pub first_fat_sector: u32,
    pub first_root_sector: u32,
    pub first_data_sector: u32,
    pub cluster_count: u32,
}

const BOOT_SIGNATURE: [u8; 2] = [0x55, 0xAA];

impl Bpb {
    /// Parses and validates a boot sector.
    pub fn parse(sector: &Block) -> Result<Self, FatError> {
        if sector[510..512] != BOOT_SIGNATURE {
            return Err(FatError::InvalidBootSector);
        }
        let bytes_per_sector = u16::from_le_bytes([sector[11], sector[12]]);
        let sectors_per_cluster = sector[13];
        let reserved_sectors = u16::from_le_bytes([sector[14], sector[15]]);
        let fat_count = sector[16];
        let root_entries = u16::from_le_bytes([sector[17], sector[18]]);
        let total_sectors_16 = u16::from_le_bytes([sector[19], sector[20]]);
        let sectors_per_fat = u16::from_le_bytes([sector[22], sector[23]]);
        let total_sectors_32 = u32::from_le_bytes([sector[32], sector[33], sector[34], sector[35]]);

        if bytes_per_sector as usize != BLOCK_SIZE
            || sectors_per_cluster == 0
            || !sectors_per_cluster.is_power_of_two()
            || reserved_sectors == 0
            || fat_count == 0
        {
            return Err(FatError::InvalidBootSector);
        }
        // A zero 16-bit FAT size means FAT32, which is out of scope.
        if sectors_per_fat == 0 || root_entries == 0 {
            return Err(FatError::Unsupported);
        }
        let total_sectors = if total_sectors_16 != 0 {
            total_sectors_16 as u32
        } else {
            total_sectors_32
        };
        if total_sectors == 0 {
            return Err(FatError::InvalidBootSector);
        }

        let root_dir_sectors = (root_entries as u32 * 32).div_ceil(BLOCK_SIZE as u32);
        let first_fat_sector = reserved_sectors as u32;
        let first_root_sector = first_fat_sector + fat_count as u32 * sectors_per_fat as u32;
        let first_data_sector = first_root_sector + root_dir_sectors;
        if first_data_sector >= total_sectors {
            return Err(FatError::InvalidBootSector);
        }
        let cluster_count = (total_sectors - first_data_sector) / sectors_per_cluster as u32;
        let fat_type = match cluster_count {
            0..=4084 => FatType::Fat12,
            4085..=65524 => FatType::Fat16,
            _ => return Err(FatError::Unsupported),
        };

        Ok(Self {
            sectors_per_cluster,
            reserved_sectors,
            fat_count,
            root_entries,
            total_sectors,
            sectors_per_fat,
            fat_type,
            root_dir_sectors,
            first_fat_sector,
            first_root_sector,
            first_data_sector,
            cluster_count,
        })
    }

    /// First sector of a data cluster (clusters start at 2).
    pub fn cluster_to_sector(&self, cluster: u32) -> u32 {
        self.first_data_sector + (cluster - 2) * self.sectors_per_cluster as u32
    }

    /// Whether `cluster` addresses a real data cluster.
    pub fn is_valid_cluster(&self, cluster: u32) -> bool {
        cluster >= 2 && cluster < self.cluster_count + 2
    }
}
