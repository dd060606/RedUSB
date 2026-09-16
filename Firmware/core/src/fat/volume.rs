//! Mounted volume: root directory iteration and sequential file reading.

use super::FatError;
use super::bpb::{Bpb, FatType};
use super::dir::{DIR_ENTRY_SIZE, DirEntry, RawEntry, decode_entry};
use crate::block::{BLOCK_SIZE, Block, BlockDevice};

/// A mounted FAT12/16 volume. Holds only the geometry, so it is cheap to copy.
#[derive(Debug, Clone, Copy)]
pub struct FatVolume {
    bpb: Bpb,
}

/// One-block cache in front of the FAT, so chain walks do not re-read the same sector.
struct FatCache {
    lba: u32,
    block: Block,
}

impl FatCache {
    fn new() -> Self {
        Self {
            lba: u32::MAX,
            block: [0; BLOCK_SIZE],
        }
    }

    fn byte<D: BlockDevice>(
        &mut self,
        dev: &mut D,
        first_fat: u32,
        offset: u32,
    ) -> Result<u8, FatError> {
        let lba = first_fat + offset / BLOCK_SIZE as u32;
        if lba != self.lba {
            dev.read_block(lba, &mut self.block)?;
            self.lba = lba;
        }
        Ok(self.block[offset as usize % BLOCK_SIZE])
    }
}

impl FatVolume {
    /// Reads and validates the boot sector.
    pub fn mount<D: BlockDevice>(dev: &mut D) -> Result<Self, FatError> {
        let mut boot = [0u8; BLOCK_SIZE];
        dev.read_block(0, &mut boot)?;
        let bpb = Bpb::parse(&boot)?;
        if bpb.total_sectors > dev.block_count() {
            return Err(FatError::InvalidBootSector);
        }
        Ok(Self { bpb })
    }

    /// Volume geometry.
    pub fn bpb(&self) -> &Bpb {
        &self.bpb
    }

    /// Calls `visit` for every regular file of the root directory, in on-disk order,
    /// until it returns `false`.
    pub fn for_each_root_file<D, F>(&self, dev: &mut D, mut visit: F) -> Result<(), FatError>
    where
        D: BlockDevice,
        F: FnMut(&DirEntry) -> bool,
    {
        let mut block = [0u8; BLOCK_SIZE];
        for i in 0..self.bpb.root_dir_sectors {
            dev.read_block(self.bpb.first_root_sector + i, &mut block)?;
            for raw in block.chunks_exact(DIR_ENTRY_SIZE) {
                match decode_entry(raw) {
                    RawEntry::End => return Ok(()),
                    RawEntry::Skip => {}
                    RawEntry::File(entry) => {
                        if !visit(&entry) {
                            return Ok(());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Returns the cluster following `cluster`, or `None` at the end of the chain.
    fn next_cluster<D: BlockDevice>(
        &self,
        dev: &mut D,
        cache: &mut FatCache,
        cluster: u32,
    ) -> Result<Option<u32>, FatError> {
        let first_fat = self.bpb.first_fat_sector;
        let (value, end_of_chain, bad) = match self.bpb.fat_type {
            FatType::Fat12 => {
                let offset = cluster + cluster / 2;
                let lo = cache.byte(dev, first_fat, offset)? as u32;
                let hi = cache.byte(dev, first_fat, offset + 1)? as u32;
                let packed = lo | (hi << 8);
                let value = if cluster & 1 == 1 {
                    packed >> 4
                } else {
                    packed & 0x0FFF
                };
                (value, 0x0FF8, 0x0FF7)
            }
            FatType::Fat16 => {
                let offset = cluster * 2;
                let lo = cache.byte(dev, first_fat, offset)? as u32;
                let hi = cache.byte(dev, first_fat, offset + 1)? as u32;
                (lo | (hi << 8), 0xFFF8, 0xFFF7)
            }
        };
        if value >= end_of_chain {
            return Ok(None);
        }
        if value == bad || !self.bpb.is_valid_cluster(value) {
            return Err(FatError::CorruptChain);
        }
        Ok(Some(value))
    }

    /// Opens a file for sequential reading.
    pub fn open(&self, entry: &DirEntry) -> FileReader {
        FileReader {
            cluster: entry.first_cluster,
            sector_in_cluster: 0,
            offset_in_block: 0,
            loaded: false,
            remaining: entry.size,
            block: [0; BLOCK_SIZE],
            fat: FatCache::new(),
        }
    }
}

/// Sequential reader over a file's cluster chain.
///
/// Holds two 512-byte buffers (current data block and a FAT block cache).
pub struct FileReader {
    cluster: u32,
    sector_in_cluster: u8,
    offset_in_block: usize,
    loaded: bool,
    remaining: u32,
    block: Block,
    fat: FatCache,
}

impl FileReader {
    /// Bytes left to read.
    pub fn remaining(&self) -> u32 {
        self.remaining
    }

    /// Reads up to `buf.len()` bytes, returning 0 at end of file.
    pub fn read<D: BlockDevice>(
        &mut self,
        vol: &FatVolume,
        dev: &mut D,
        buf: &mut [u8],
    ) -> Result<usize, FatError> {
        let bpb = vol.bpb();
        let mut done = 0;
        while done < buf.len() && self.remaining > 0 {
            if !self.loaded {
                if !bpb.is_valid_cluster(self.cluster) {
                    return Err(FatError::CorruptChain);
                }
                let lba = bpb.cluster_to_sector(self.cluster) + self.sector_in_cluster as u32;
                dev.read_block(lba, &mut self.block)?;
                self.loaded = true;
                self.offset_in_block = 0;
            }
            let available = BLOCK_SIZE - self.offset_in_block;
            let wanted = buf.len() - done;
            let n = available.min(wanted).min(self.remaining as usize);
            buf[done..done + n]
                .copy_from_slice(&self.block[self.offset_in_block..self.offset_in_block + n]);
            self.offset_in_block += n;
            self.remaining -= n as u32;
            done += n;

            if self.offset_in_block == BLOCK_SIZE {
                self.advance(vol, dev)?;
            }
        }
        Ok(done)
    }

    /// Moves past the block just consumed, following the FAT chain at a cluster end.
    fn advance<D: BlockDevice>(&mut self, vol: &FatVolume, dev: &mut D) -> Result<(), FatError> {
        self.loaded = false;
        self.sector_in_cluster += 1;
        if self.sector_in_cluster == vol.bpb().sectors_per_cluster {
            self.sector_in_cluster = 0;
            if self.remaining > 0 {
                self.cluster = vol
                    .next_cluster(dev, &mut self.fat, self.cluster)?
                    .ok_or(FatError::CorruptChain)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test_writer {
    //! A tiny FAT writer, enough to build fixtures for the reader tests.

    use super::*;
    use crate::fat::dir::{ShortName, file_entry, short_name_from_str};
    use crate::testing::MemBlockDevice;

    fn fat_get(bytes: &[u8], bpb: &Bpb, cluster: u32) -> u32 {
        let base = bpb.first_fat_sector as usize * BLOCK_SIZE;
        match bpb.fat_type {
            FatType::Fat12 => {
                let off = base + (cluster + cluster / 2) as usize;
                let packed = bytes[off] as u32 | ((bytes[off + 1] as u32) << 8);
                if cluster & 1 == 1 {
                    packed >> 4
                } else {
                    packed & 0xFFF
                }
            }
            FatType::Fat16 => {
                let off = base + cluster as usize * 2;
                bytes[off] as u32 | ((bytes[off + 1] as u32) << 8)
            }
        }
    }

    fn fat_set(bytes: &mut [u8], bpb: &Bpb, cluster: u32, value: u32) {
        for copy in 0..bpb.fat_count as usize {
            let base =
                (bpb.first_fat_sector as usize + copy * bpb.sectors_per_fat as usize) * BLOCK_SIZE;
            match bpb.fat_type {
                FatType::Fat12 => {
                    let off = base + (cluster + cluster / 2) as usize;
                    let mut packed = bytes[off] as u32 | ((bytes[off + 1] as u32) << 8);
                    if cluster & 1 == 1 {
                        packed = (packed & 0x000F) | ((value & 0xFFF) << 4);
                    } else {
                        packed = (packed & 0xF000) | (value & 0xFFF);
                    }
                    bytes[off] = packed as u8;
                    bytes[off + 1] = (packed >> 8) as u8;
                }
                FatType::Fat16 => {
                    let off = base + cluster as usize * 2;
                    bytes[off] = value as u8;
                    bytes[off + 1] = (value >> 8) as u8;
                }
            }
        }
    }

    fn add_root_slot(bytes: &mut [u8], bpb: &Bpb, raw: &[u8; DIR_ENTRY_SIZE]) {
        let start = bpb.first_root_sector as usize * BLOCK_SIZE;
        let end = start + bpb.root_dir_sectors as usize * BLOCK_SIZE;
        for off in (start..end).step_by(DIR_ENTRY_SIZE) {
            if bytes[off] == 0x00 || bytes[off] == 0xE5 {
                bytes[off..off + DIR_ENTRY_SIZE].copy_from_slice(raw);
                return;
            }
        }
        panic!("root directory full");
    }

    /// Writes `data` as a root file, allocating the first free clusters.
    pub fn write_file(dev: &mut MemBlockDevice, vol: &FatVolume, name: &str, data: &[u8]) {
        let bpb = *vol.bpb();
        let short = short_name_from_str(name).expect("valid 8.3 name");
        let cluster_bytes = bpb.sectors_per_cluster as usize * BLOCK_SIZE;
        let needed = data.len().div_ceil(cluster_bytes);
        let bytes = dev.bytes_mut();

        let mut chain = std::vec::Vec::new();
        let mut cluster = 2;
        while chain.len() < needed {
            assert!(bpb.is_valid_cluster(cluster), "volume full");
            if fat_get(bytes, &bpb, cluster) == 0 {
                chain.push(cluster);
            }
            cluster += 1;
        }
        let eoc = match bpb.fat_type {
            FatType::Fat12 => 0xFFF,
            FatType::Fat16 => 0xFFFF,
        };
        for (i, &c) in chain.iter().enumerate() {
            let next = chain.get(i + 1).copied().unwrap_or(eoc);
            fat_set(bytes, &bpb, c, next);
            let start = bpb.cluster_to_sector(c) as usize * BLOCK_SIZE;
            let src = &data[i * cluster_bytes..data.len().min((i + 1) * cluster_bytes)];
            bytes[start..start + src.len()].copy_from_slice(src);
        }
        let first = chain.first().copied().unwrap_or(0);
        add_root_slot(bytes, &bpb, &file_entry(&short, first, data.len() as u32));
    }

    /// Inserts a long-file-name fragment slot, which readers must skip.
    pub fn add_lfn_slot(dev: &mut MemBlockDevice, vol: &FatVolume) {
        let mut raw = [0u8; DIR_ENTRY_SIZE];
        raw[0] = 0x41;
        raw[11] = 0x0F;
        add_root_slot(dev.bytes_mut(), vol.bpb(), &raw);
    }

    pub fn name(s: &str) -> ShortName {
        short_name_from_str(s).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::test_writer::{add_lfn_slot, name, write_file};
    use super::*;
    use crate::fat::format::{FormatParams, format_volume};
    use crate::testing::MemBlockDevice;
    use std::vec::Vec;

    fn fixture(sectors_per_cluster: u8) -> (MemBlockDevice, FatVolume) {
        let mut dev = MemBlockDevice::new(14208);
        let mut params = FormatParams::for_flash(14208, *b"REDUSB     ");
        params.sectors_per_cluster = sectors_per_cluster;
        format_volume(&mut dev, &params).unwrap();
        let vol = FatVolume::mount(&mut dev).unwrap();
        (dev, vol)
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 % 251) as u8).collect()
    }

    fn read_all(
        vol: &FatVolume,
        dev: &mut MemBlockDevice,
        entry: &DirEntry,
        chunk: usize,
    ) -> Vec<u8> {
        let mut reader = vol.open(entry);
        let mut out = Vec::new();
        let mut buf = std::vec![0u8; chunk];
        loop {
            let n = reader.read(vol, dev, &mut buf).unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        assert_eq!(reader.remaining(), 0);
        out
    }

    fn list(vol: &FatVolume, dev: &mut MemBlockDevice) -> Vec<DirEntry> {
        let mut entries = Vec::new();
        vol.for_each_root_file(dev, |e| {
            entries.push(*e);
            true
        })
        .unwrap();
        entries
    }

    #[test]
    fn mount_rejects_blank_device() {
        let mut dev = MemBlockDevice::new(64);
        assert_eq!(
            FatVolume::mount(&mut dev).unwrap_err(),
            FatError::InvalidBootSector
        );
    }

    #[test]
    fn empty_volume_lists_nothing() {
        let (mut dev, vol) = fixture(8);
        assert!(list(&vol, &mut dev).is_empty());
    }

    #[test]
    fn reads_multi_cluster_file_on_fat12() {
        let (mut dev, vol) = fixture(8);
        assert_eq!(vol.bpb().fat_type, FatType::Fat12);
        let small = b"REM hello\nSTRING hi\n";
        let big = pattern(9000);
        write_file(&mut dev, &vol, "01_hello.txt", small);
        add_lfn_slot(&mut dev, &vol);
        write_file(&mut dev, &vol, "02_big.txt", &big);

        let entries = list(&vol, &mut dev);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, name("01_HELLO.TXT"));
        assert_eq!(entries[1].name, name("02_BIG.TXT"));
        assert_eq!(entries[1].size, 9000);

        assert_eq!(read_all(&vol, &mut dev, &entries[0], 512), small);
        for chunk in [1, 7, 100, 512, 4096] {
            assert_eq!(read_all(&vol, &mut dev, &entries[1], chunk), big);
        }
    }

    #[test]
    fn reads_multi_cluster_file_on_fat16() {
        let (mut dev, vol) = fixture(1);
        assert_eq!(vol.bpb().fat_type, FatType::Fat16);
        let data = pattern(3000);
        write_file(&mut dev, &vol, "a.txt", &data);
        let entries = list(&vol, &mut dev);
        assert_eq!(entries.len(), 1);
        assert_eq!(read_all(&vol, &mut dev, &entries[0], 333), data);
    }

    #[test]
    fn zero_length_file_reads_nothing() {
        let (mut dev, vol) = fixture(8);
        write_file(&mut dev, &vol, "empty.txt", b"");
        let entries = list(&vol, &mut dev);
        assert_eq!(entries[0].first_cluster, 0);
        assert!(read_all(&vol, &mut dev, &entries[0], 64).is_empty());
    }

    #[test]
    fn truncated_chain_is_reported() {
        let (mut dev, vol) = fixture(8);
        write_file(&mut dev, &vol, "big.txt", &pattern(9000));
        let entries = list(&vol, &mut dev);
        // Claim the file is longer than its chain.
        let mut entry = entries[0];
        entry.size = 20000;
        let mut reader = vol.open(&entry);
        let mut buf = [0u8; 512];
        let mut result = Ok(0);
        for _ in 0..64 {
            result = reader.read(&vol, &mut dev, &mut buf);
            if result.is_err() {
                break;
            }
        }
        assert_eq!(result.unwrap_err(), FatError::CorruptChain);
    }
}
