//! RedUSB firmware entry point.
//!
//! Boot flow:
//! 1. Bring up clocks, pins, PWM, timer and the USB bus (`board`).
//! 2. Read the mode switch. In configuration mode, a button held at power-up
//!    reboots into the RP2040 UF2 bootloader.
//! 3. Mount the FAT volume kept in flash (formatting it in configuration mode if
//!    it is blank), load the persisted script selection and build the catalog.
//! 4. Hand over to the mode-specific superloop, which never returns.

#![no_std]
#![no_main]

mod app;
mod board;
mod button;
mod led;
mod mode;
mod storage;
mod usb;

use defmt_rtt as _;
use panic_probe as _;

use board::Board;
use mode::DeviceMode;
use redusb_core::block::BlockDevice;
use redusb_core::catalog::ScriptCatalog;
use redusb_core::fat::{FatVolume, FormatParams, format_volume};
use storage::cached::CachedFlashDevice;
use storage::flash::Flash;
use storage::layout::{VOLUME_BLOCKS, VOLUME_LABEL};
use storage::settings::Settings;

/// Second stage bootloader for the Winbond W25Qxx flash fitted on the KB2040.
#[unsafe(link_section = ".boot2")]
#[used]
pub static BOOT2_FIRMWARE: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

#[rp2040_hal::entry]
fn main() -> ! {
    let mut board = Board::take();
    let mode = board.mode_switch.boot_mode();
    defmt::info!("RedUSB starting in {} mode", mode);

    if mode == DeviceMode::Config && board.button.is_pressed_raw() {
        defmt::info!("button held at power-up, rebooting to bootloader");
        mode::reboot_to_bootloader();
    }

    let mut storage = CachedFlashDevice::new(Flash::take());
    let settings = Settings::load(storage.flash());
    defmt::info!("settings: {}", settings);

    let volume = mount_or_format(&mut storage, mode);
    let mut catalog = match volume {
        Some(volume) => ScriptCatalog::scan(&volume, &mut storage).unwrap_or_else(|err| {
            defmt::warn!("directory scan failed: {}", defmt::Debug2Format(&err));
            ScriptCatalog::empty()
        }),
        None => ScriptCatalog::empty(),
    };
    catalog.select(settings.selected as usize);
    defmt::info!(
        "{} script(s), selected #{}",
        catalog.len(),
        catalog.selected()
    );

    match mode {
        DeviceMode::Config => app::config_mode::run(board, storage, volume, catalog),
        DeviceMode::Normal => app::normal_mode::run(board, storage, volume, catalog),
    }
}

/// Mounts the flash volume. In configuration mode a blank or damaged volume is
/// re-formatted; in normal mode it is simply reported as absent.
fn mount_or_format(storage: &mut CachedFlashDevice, mode: DeviceMode) -> Option<FatVolume> {
    match FatVolume::mount(storage) {
        Ok(volume) => Some(volume),
        Err(err) if mode == DeviceMode::Config => {
            defmt::warn!(
                "volume unusable ({}), formatting",
                defmt::Debug2Format(&err)
            );
            let params = FormatParams::for_flash(VOLUME_BLOCKS, VOLUME_LABEL);
            if let Err(err) = format_volume(storage, &params).and_then(|_| Ok(storage.flush()?)) {
                defmt::error!("format failed: {}", defmt::Debug2Format(&err));
                return None;
            }
            FatVolume::mount(storage).ok()
        }
        Err(err) => {
            defmt::warn!("volume unusable: {}", defmt::Debug2Format(&err));
            None
        }
    }
}
