//! Normal mode: the device is a keyboard; the selected script auto-runs once the host
//! has configured it, and replays on every button press.

use super::DEFAULT_LAYOUT;
use crate::board::{Board, millis};
use crate::button::ButtonEvent;
use crate::led::{Color, Pattern};
use crate::mode;
use crate::storage::cached::CachedFlashDevice;
use crate::usb::keyboard::UsbKeyboard;
use redusb_core::catalog::ScriptCatalog;
use redusb_core::fat::{FatError, FatVolume, FileReader};
use redusb_core::script::{ByteSource, Interpreter, RunErrorKind};

/// Red flash after a failed or missing script.
const ERROR_FLASH_MS: u32 = 1500;

/// Time the host must keep us configured before the script auto-runs, so its HID
/// driver is bound and the first keystrokes are not missed. Generous because the
/// per-boot USB release number makes the host treat us as a new device and re-run
/// its device setup on every connection.
const AUTORUN_SETTLE_MS: u64 = 1000;

pub fn run(
    mut board: Board,
    mut storage: CachedFlashDevice,
    volume: Option<FatVolume>,
    catalog: ScriptCatalog,
) -> ! {
    let mut keyboard = UsbKeyboard::new(board.usb_bus, board.timer, board.boot_entropy);
    let mut interpreter = Interpreter::new(DEFAULT_LAYOUT);
    let idle = idle_pattern(&catalog);
    board.led.set_pattern(idle);

    // Auto-run the selected script once the host has kept us configured long enough to
    // bind its HID driver; the button still re-runs on demand. A replug power-cycles the
    // bus-powered board, so every connection re-arms the auto-run from a fresh boot.
    let mut configured_since: Option<u64> = None;
    let mut autorun_done = false;

    loop {
        let now = millis(board.timer);
        keyboard.poll();

        let autorun = if autorun_done {
            false
        } else if keyboard.is_configured() {
            now.wrapping_sub(*configured_since.get_or_insert(now)) >= AUTORUN_SETTLE_MS
        } else {
            configured_since = None;
            false
        };
        let pressed = board.button.poll(now) == Some(ButtonEvent::Pressed);

        if autorun || pressed {
            match (volume, catalog.current()) {
                (Some(volume), Some(entry)) if keyboard.is_configured() => {
                    autorun_done = true;
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
                    interpreter.set_fingerprint(fp);
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
                                .flash(error_color(&err.kind), ERROR_FLASH_MS, millis(board.timer));
                        }
                    }
                    board.led.set_pattern(idle);
                }
                _ => {
                    // Latch a scriptless auto-run so it does not retry every loop.
                    autorun_done |= autorun;
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

/// Colour of the error flash:
/// * magenta — a keystroke timed out (the host never fetched the HID report).
/// * orange  — a script line could not be parsed.
/// * yellow  — the script file could not be read from flash.
/// * cyan    — malformed `IF_OS` / `ELSE` / `END_IF` structure.
fn error_color<E>(kind: &RunErrorKind<E>) -> Color {
    match kind {
        RunErrorKind::Sink(_) => Color::MAGENTA,
        RunErrorKind::Parse(_) => Color::ORANGE,
        RunErrorKind::Line(_) => Color::YELLOW,
        RunErrorKind::Control(_) => Color::CYAN,
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
