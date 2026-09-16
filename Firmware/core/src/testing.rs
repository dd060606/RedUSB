//! Test helpers: an in-memory block device.

use crate::block::{BLOCK_SIZE, Block, BlockDevice, BlockError};
use std::vec;
use std::vec::Vec;

/// RAM-backed block device used by the host tests.
pub struct MemBlockDevice {
    data: Vec<u8>,
    pub flushes: usize,
}

impl MemBlockDevice {
    pub fn new(block_count: u32) -> Self {
        Self {
            data: vec![0xFF; block_count as usize * BLOCK_SIZE],
            flushes: 0,
        }
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

impl BlockDevice for MemBlockDevice {
    fn block_count(&self) -> u32 {
        (self.data.len() / BLOCK_SIZE) as u32
    }

    fn read_block(&mut self, lba: u32, buf: &mut Block) -> Result<(), BlockError> {
        let start = lba as usize * BLOCK_SIZE;
        let src = self
            .data
            .get(start..start + BLOCK_SIZE)
            .ok_or(BlockError::OutOfRange)?;
        buf.copy_from_slice(src);
        Ok(())
    }

    fn write_block(&mut self, lba: u32, buf: &Block) -> Result<(), BlockError> {
        let start = lba as usize * BLOCK_SIZE;
        let dst = self
            .data
            .get_mut(start..start + BLOCK_SIZE)
            .ok_or(BlockError::OutOfRange)?;
        dst.copy_from_slice(buf);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        self.flushes += 1;
        Ok(())
    }
}
