//! USB mass-storage drive (SCSI over Bulk-Only Transport) exposing the flash volume.
//! `usbd-storage` handles the transport; this module answers SCSI commands against the
//! [`CachedFlashDevice`], keeping per-command progress in [`Handler`] across polls for large transfers.

use super::{PID_STORAGE, STORAGE_RELEASE, VID, build_device};
use crate::storage::cached::CachedFlashDevice;
use redusb_core::block::{BLOCK_SIZE, Block, BlockDevice};
use rp2040_hal::usb::UsbBus;
use static_cell::StaticCell;
use usb_device::bus::UsbBusAllocator;
use usb_device::device::UsbDevice;
use usbd_storage::subclass::Command;
use usbd_storage::subclass::scsi::{Scsi, ScsiCommand};
use usbd_storage::transport::TransportError;
use usbd_storage::transport::bbb::{BulkOnly, BulkOnlyError};

const USB_PACKET_SIZE: u16 = 64;
/// Transport buffer; one flash sector so large transfers need few round trips.
const IO_BUFFER_SIZE: usize = 4096;

// SCSI opcodes usbd-storage does not decode but hosts routinely send.
const OP_START_STOP_UNIT: u8 = 0x1B;
const OP_PREVENT_ALLOW_MEDIUM_REMOVAL: u8 = 0x1E;
const OP_SYNCHRONIZE_CACHE: u8 = 0x35;

type ScsiClass = Scsi<BulkOnly<'static, UsbBus, &'static mut [u8]>>;
type ScsiCmd<'a> = Command<'a, ScsiCommand, ScsiClass>;
type TransportResult<T = ()> = Result<T, TransportError<BulkOnlyError>>;

/// Standard INQUIRY response: direct-access, removable, SPC-2 style.
const INQUIRY_RESPONSE: [u8; 36] = *b"\x00\x80\x02\x02\x1F\x00\x00\x00RedUSB  Macro Drive     1.0 ";

/// What the drive did during the last poll, for the activity indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Idle,
    Read,
    Write,
}

/// Fixed-format sense data reported on the next REQUEST SENSE.
#[derive(Debug, Clone, Copy)]
struct Sense {
    key: u8,
    asc: u8,
    ascq: u8,
}

impl Sense {
    const NONE: Sense = Sense {
        key: 0x00,
        asc: 0x00,
        ascq: 0x00,
    };
    const INVALID_COMMAND: Sense = Sense {
        key: 0x05,
        asc: 0x20,
        ascq: 0x00,
    };
    const LBA_OUT_OF_RANGE: Sense = Sense {
        key: 0x05,
        asc: 0x21,
        ascq: 0x00,
    };
    const MEDIUM_ERROR: Sense = Sense {
        key: 0x03,
        asc: 0x11,
        ascq: 0x00,
    };

    fn encode(self) -> [u8; 18] {
        let mut data = [0u8; 18];
        data[0] = 0x70;
        data[2] = self.key;
        data[7] = 10;
        data[12] = self.asc;
        data[13] = self.ascq;
        data
    }
}

/// Per-command progress plus the last sense.
struct Handler {
    sense: Sense,
    /// Bytes already moved in the current READ/WRITE data phase.
    transferred: usize,
    /// Block staging buffer and how much of it is filled (WRITE) or loaded (READ).
    block: Block,
    fill: usize,
    loaded_lba: Option<u32>,
    activity: Activity,
}

impl Handler {
    fn new() -> Self {
        Self {
            sense: Sense::NONE,
            transferred: 0,
            block: [0; BLOCK_SIZE],
            fill: 0,
            loaded_lba: None,
            activity: Activity::Idle,
        }
    }

    fn reset_transfer(&mut self) {
        self.transferred = 0;
        self.fill = 0;
        self.loaded_lba = None;
    }

    /// Answers a command whose whole response fits in the transport buffer.
    fn reply(&mut self, mut cmd: ScsiCmd<'_>, data: &[u8]) -> TransportResult {
        let written = cmd.write_data(data)?;
        cmd.pass(written as u32);
        Ok(())
    }

    fn reject(&mut self, cmd: ScsiCmd<'_>, sense: Sense) {
        self.sense = sense;
        cmd.fail(0);
    }

    fn handle(&mut self, cmd: ScsiCmd<'_>, storage: &mut CachedFlashDevice) -> TransportResult {
        let kind = cmd.kind;
        match kind {
            ScsiCommand::TestUnitReady => {
                cmd.pass(0);
                Ok(())
            }
            ScsiCommand::Inquiry { .. } => self.reply(cmd, &INQUIRY_RESPONSE),
            ScsiCommand::RequestSense { .. } => {
                let data = self.sense.encode();
                self.sense = Sense::NONE;
                self.reply(cmd, &data)
            }
            ScsiCommand::ModeSense6 { .. } => self.reply(cmd, &[3, 0, 0, 0]),
            ScsiCommand::ModeSense10 { .. } => self.reply(cmd, &[0, 6, 0, 0, 0, 0, 0, 0]),
            ScsiCommand::ReadCapacity10 => {
                let mut data = [0u8; 8];
                data[..4].copy_from_slice(&(storage.block_count() - 1).to_be_bytes());
                data[4..].copy_from_slice(&(BLOCK_SIZE as u32).to_be_bytes());
                self.reply(cmd, &data)
            }
            ScsiCommand::ReadCapacity16 { .. } => {
                let mut data = [0u8; 32];
                data[..8].copy_from_slice(&((storage.block_count() - 1) as u64).to_be_bytes());
                data[8..12].copy_from_slice(&(BLOCK_SIZE as u32).to_be_bytes());
                self.reply(cmd, &data)
            }
            ScsiCommand::ReadFormatCapacities { .. } => {
                let mut data = [0u8; 12];
                data[3] = 8;
                data[4..8].copy_from_slice(&storage.block_count().to_be_bytes());
                data[8] = 0x02;
                data[9..12].copy_from_slice(&(BLOCK_SIZE as u32).to_be_bytes()[1..]);
                self.reply(cmd, &data)
            }
            ScsiCommand::Read { lba, len } => self.read(cmd, storage, lba, len as u32),
            ScsiCommand::Write { lba, len } => self.write(cmd, storage, lba, len as u32),
            ScsiCommand::Unknown { cmd: opcode } => match opcode {
                OP_PREVENT_ALLOW_MEDIUM_REMOVAL => {
                    cmd.pass(0);
                    Ok(())
                }
                OP_START_STOP_UNIT | OP_SYNCHRONIZE_CACHE => {
                    if storage.flush().is_err() {
                        self.reject(cmd, Sense::MEDIUM_ERROR);
                    } else {
                        cmd.pass(0);
                    }
                    Ok(())
                }
                _ => {
                    defmt::debug!("scsi: unsupported opcode {=u8:#x}", opcode);
                    self.reject(cmd, Sense::INVALID_COMMAND);
                    Ok(())
                }
            },
            _ => {
                self.reject(cmd, Sense::INVALID_COMMAND);
                Ok(())
            }
        }
    }

    fn read(
        &mut self,
        mut cmd: ScsiCmd<'_>,
        storage: &mut CachedFlashDevice,
        lba: u32,
        len: u32,
    ) -> TransportResult {
        if len == 0 {
            cmd.pass(0);
            return Ok(());
        }
        if lba.saturating_add(len) > storage.block_count() {
            self.reject(cmd, Sense::LBA_OUT_OF_RANGE);
            return Ok(());
        }
        let total = len as usize * BLOCK_SIZE;
        while self.transferred < total {
            let block_lba = lba + (self.transferred / BLOCK_SIZE) as u32;
            let in_block = self.transferred % BLOCK_SIZE;
            if self.loaded_lba != Some(block_lba) {
                if storage.read_block(block_lba, &mut self.block).is_err() {
                    self.sense = Sense::MEDIUM_ERROR;
                    cmd.fail(self.transferred as u32);
                    self.reset_transfer();
                    return Ok(());
                }
                self.loaded_lba = Some(block_lba);
            }
            let written = cmd.write_data(&self.block[in_block..])?;
            if written == 0 {
                // Transport buffer full; resume on the next poll.
                break;
            }
            self.transferred += written;
        }
        self.activity = Activity::Read;
        if self.transferred >= total {
            cmd.pass(total as u32);
            self.reset_transfer();
        }
        Ok(())
    }

    fn write(
        &mut self,
        mut cmd: ScsiCmd<'_>,
        storage: &mut CachedFlashDevice,
        lba: u32,
        len: u32,
    ) -> TransportResult {
        if len == 0 {
            cmd.pass(0);
            return Ok(());
        }
        if lba.saturating_add(len) > storage.block_count() {
            self.reject(cmd, Sense::LBA_OUT_OF_RANGE);
            return Ok(());
        }
        let total = len as usize * BLOCK_SIZE;
        loop {
            let received = cmd.read_data(&mut self.block[self.fill..])?;
            if received == 0 {
                break;
            }
            self.fill += received;
            if self.fill == BLOCK_SIZE {
                let block_lba = lba + (self.transferred / BLOCK_SIZE) as u32;
                if storage.write_block(block_lba, &self.block).is_err() {
                    self.sense = Sense::MEDIUM_ERROR;
                    cmd.fail(self.transferred as u32);
                    self.reset_transfer();
                    return Ok(());
                }
                self.transferred += BLOCK_SIZE;
                self.fill = 0;
            }
        }
        self.activity = Activity::Write;
        if self.transferred >= total {
            cmd.pass(total as u32);
            self.reset_transfer();
        }
        Ok(())
    }
}

/// The mass-storage USB device.
pub struct UsbDrive {
    scsi: ScsiClass,
    device: UsbDevice<'static, UsbBus>,
    handler: Handler,
}

impl UsbDrive {
    pub fn new(alloc: &'static UsbBusAllocator<UsbBus>) -> Self {
        static IO_BUFFER: StaticCell<[u8; IO_BUFFER_SIZE]> = StaticCell::new();
        let buffer: &'static mut [u8] = IO_BUFFER.init([0; IO_BUFFER_SIZE]);
        let scsi = Scsi::new(alloc, USB_PACKET_SIZE, 0, buffer).expect("scsi class");
        let device = build_device(alloc, VID, PID_STORAGE, "RedUSB", "RedUSB Config Drive", Some("REDUSB-0001"), true, STORAGE_RELEASE);
        Self {
            scsi,
            device,
            handler: Handler::new(),
        }
    }

    /// Services the bus and any pending SCSI command. Call it continuously.
    pub fn poll(&mut self, storage: &mut CachedFlashDevice) -> Activity {
        let Self {
            scsi,
            device,
            handler,
        } = self;
        device.poll(&mut [scsi]);
        handler.activity = Activity::Idle;
        if let Err(err) = scsi.poll_command(|cmd| handler.handle(cmd, storage)) {
            defmt::debug!("scsi: transport error {}", defmt::Debug2Format(&err));
        }
        handler.activity
    }
}
