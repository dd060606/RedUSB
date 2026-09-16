//! Block device abstraction shared by the FAT layer and the firmware storage backends.

/// Size of one logical block in bytes. Both FAT and USB mass storage use 512-byte sectors.
pub const BLOCK_SIZE: usize = 512;

/// One logical block.
pub type Block = [u8; BLOCK_SIZE];

/// Errors reported by a [`BlockDevice`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError {
    /// The requested block lies outside the device.
    OutOfRange,
    /// The underlying medium failed.
    Io,
}

/// A random-access device made of 512-byte blocks.
pub trait BlockDevice {
    /// Total number of blocks on the device.
    fn block_count(&self) -> u32;

    /// Reads block `lba` into `buf`.
    fn read_block(&mut self, lba: u32, buf: &mut Block) -> Result<(), BlockError>;

    /// Writes `buf` to block `lba`. The write may be buffered until [`BlockDevice::flush`].
    fn write_block(&mut self, lba: u32, buf: &Block) -> Result<(), BlockError>;

    /// Pushes any buffered writes to the medium.
    fn flush(&mut self) -> Result<(), BlockError> {
        Ok(())
    }
}
