//! Configuration mode: the device is a USB drive, the button selects a script.

use crate::board::{Board, millis};
use crate::button::ButtonEvent;
use crate::led::{Color, Pattern, RgbLed};
use crate::mode;
use crate::storage::cached::CachedFlashDevice;
use crate::storage::settings::Settings;
use crate::usb::mass_storage::{Activity, UsbDrive};
use redusb_core::block::BlockDevice;
use redusb_core::catalog::ScriptCatalog;
use redusb_core::fat::FatVolume;

/// How long the LED blanks to signal a flash write.
const WRITE_FLICKER_MS: u32 = 40;

pub fn run(
    mut board: Board,
    mut storage: CachedFlashDevice,
    mut volume: Option<FatVolume>,
    mut catalog: ScriptCatalog,
) -> ! {
    let mut drive = UsbDrive::new(board.usb_bus);
    show_selection(&mut board.led, &catalog);

    loop {
        let now = millis(board.timer);

        if drive.poll(&mut storage) == Activity::Write {
            board.led.flash(Color::OFF, WRITE_FLICKER_MS, now);
        }

        // Once the host pauses writing, commit and pick up any new or removed scripts.
        if storage.tick(now) {
            rescan(&mut storage, &mut volume, &mut catalog);
            show_selection(&mut board.led, &catalog);
        }

        if board.button.poll(now) == Some(ButtonEvent::Pressed) {
            let selected = catalog.select_next();
            defmt::info!("selected script #{}", selected);
            Settings {
                selected: selected as u8,
            }
            .store(storage.flash());
            show_selection(&mut board.led, &catalog);
        }

        if board.mode_switch.changed(now) {
            let _ = storage.flush();
            mode::reboot(&mut board.watchdog);
        }

        board.led.tick(now);
    }
}

/// Palette colour of the selected script, or a slow white breathing when there is none.
fn show_selection(led: &mut RgbLed, catalog: &ScriptCatalog) {
    let pattern = if catalog.is_empty() {
        Pattern::Breathe {
            color: Color::WHITE,
            period_ms: 2000,
        }
    } else {
        Pattern::for_script(catalog.selected())
    };
    led.set_pattern(pattern);
}

/// Re-mounts the volume (the host may have reformatted it) and refreshes the catalog.
fn rescan(
    storage: &mut CachedFlashDevice,
    volume: &mut Option<FatVolume>,
    catalog: &mut ScriptCatalog,
) {
    match FatVolume::mount(storage) {
        Ok(mounted) => {
            *volume = Some(mounted);
            if let Err(err) = catalog.rescan(&mounted, storage) {
                defmt::warn!("rescan failed: {}", defmt::Debug2Format(&err));
                *catalog = ScriptCatalog::empty();
            }
        }
        Err(err) => {
            defmt::warn!("volume no longer mountable: {}", defmt::Debug2Format(&err));
            *volume = None;
            *catalog = ScriptCatalog::empty();
        }
    }
    defmt::info!(
        "{} script(s), selected #{}",
        catalog.len(),
        catalog.selected()
    );
}
