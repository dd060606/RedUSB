//! Minimal read-only FAT12/FAT16 support: mount, root-directory listing, sequential
//! file reads and a formatter for the flash volume. Long file names are skipped in
//! favour of the 8.3 alias the host writes alongside them.

mod bpb;
mod dir;
mod format;
mod volume;

pub use bpb::{Bpb, FatType};
pub use dir::{DirEntry, ShortName, short_name_from_str};
pub use format::{FormatParams, format_volume};
pub use volume::{FatVolume, FileReader};

#[cfg(test)]
pub(crate) use volume::test_writer;

use crate::block::BlockError;

/// Errors reported by the FAT layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatError {
    /// The block device failed.
    Block(BlockError),
    /// The boot sector is missing or malformed.
    InvalidBootSector,
    /// The volume is FAT32 or uses a geometry this crate does not handle.
    Unsupported,
    /// A cluster chain points outside the data area or is corrupt.
    CorruptChain,
    /// The device is smaller than the requested format geometry.
    DeviceTooSmall,
}

impl From<BlockError> for FatError {
    fn from(err: BlockError) -> Self {
        FatError::Block(err)
    }
}
