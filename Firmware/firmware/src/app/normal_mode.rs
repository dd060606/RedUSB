//! Normal mode: the device is a keyboard, the button plays the selected script.

use super::DEFAULT_LAYOUT;
use crate::board::{Board, millis};
use crate::button::ButtonEvent;
use crate::led::{Color, Pattern};
use crate::mode;
use crate::storage::cached::CachedFlashDevice;
use crate::usb::keyboard::UsbKeyboard;
use redusb_core::catalog::ScriptCatalog;
use redusb_core::fat::{FatError, FatVolume, FileReader};
use redusb_core::script::{ByteSource, Interpreter};

/// Red flash after a failed or missing script.
const ERROR_FLASH_MS: u32 = 1500;

pub fn run(
    mut board: Board,
    mut storage: CachedFlashDevice,
    volume: Option<FatVolume>,
    catalog: ScriptCatalog,
) -> ! {
    let mut keyboard = UsbKeyboard::new(board.usb_bus, board.timer);
    let mut interpreter = Interpreter::new(DEFAULT_LAYOUT);
    let idle = idle_pattern(&catalog);
    board.led.set_pattern(idle);

    loop {
        let now = millis(board.timer);
        keyboard.poll();

        if board.button.poll(now) == Some(ButtonEvent::Pressed) {
            match (volume, catalog.current()) {
                (Some(volume), Some(entry)) if keyboard.is_configured() => {
                    // Guess the host OS from how it enumerated us, for IF_OS blocks.
                    let os = keyboard.host_os();
                    let fp = keyboard.fingerprint();
                    defmt::info!(
                        "running {}, host OS guessed as {} (ms_os={}, dev={}, cfg={}, str={})",
                        entry.name.display().as_str(),
                        os.name(),
                        fp.ms_os_descriptor,
                        fp.device_requests,
                        fp.config_requests,
                        fp.string_requests
                    );
                    interpreter.set_host_os(os);
                    // The interpreter blocks, so show a steady "busy" colour meanwhile.
                    board.led.set_pattern(Pattern::Solid(Color::WHITE));
                    board.led.tick(now);
                    let mut source = FileSource {
                        reader: volume.open(entry),
                        volume,
                        storage: &mut storage,
                    };
                    match interpreter.run(&mut source, &mut keyboard) {
                        Ok(()) => defmt::info!("script finished"),
                        Err(err) => {
                            defmt::warn!(
                                "script failed at line {}: {}",
                                err.line,
                                defmt::Debug2Format(&err.kind)
                            );
                            board
                                .led
                                .flash(Color::RED, ERROR_FLASH_MS, millis(board.timer));
                        }
                    }
                    board.led.set_pattern(idle);
                }
                _ => {
                    defmt::warn!("no script to run");
                    board.led.flash(Color::RED, ERROR_FLASH_MS, now);
                }
            }
        }

        if board.mode_switch.changed(now) {
            mode::reboot(&mut board.watchdog);
        }

        board.led.tick(now);
    }
}

/// Dimmed palette colour of the selected script, or a faint red pulse without one.
fn idle_pattern(catalog: &ScriptCatalog) -> Pattern {
    if catalog.is_empty() {
        return Pattern::Breathe {
            color: Color::RED.scaled(1, 2),
            period_ms: 3000,
        };
    }
    match Pattern::for_script(catalog.selected()) {
        Pattern::Solid(color) => Pattern::Solid(color.scaled(1, 4)),
        Pattern::Blink { color, period_ms } => Pattern::Blink {
            color: color.scaled(1, 4),
            period_ms,
        },
        other => other,
    }
}

/// Adapts a FAT file to the interpreter's byte stream.
struct FileSource<'a> {
    reader: FileReader,
    volume: FatVolume,
    storage: &'a mut CachedFlashDevice,
}

impl ByteSource for FileSource<'_> {
    type Error = FatError;

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, FatError> {
        self.reader.read(&self.volume, self.storage, buf)
    }
}
