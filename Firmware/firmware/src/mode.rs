//! Device mode selection (the D8 switch) and the reboot helpers that go with it.

use embedded_hal::digital::InputPin;
use rp2040_hal::Watchdog;
use rp2040_hal::fugit::MicrosDurationU32;

/// Level read on the switch pin when the device should be in configuration mode.
/// The switch shorts the pin to ground, and the pin has a pull-up.
const CONFIG_MODE_IS_LOW: bool = true;

/// How long the switch must sit in a new position before a reboot is triggered.
const SWITCH_DEBOUNCE_MS: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum DeviceMode {
    /// USB mass storage: scripts can be uploaded, the button selects one.
    Config,
    /// USB keyboard: the button runs the selected script.
    Normal,
}

/// Reads the mode switch and detects position changes while running.
pub struct ModeSwitch<P: InputPin> {
    pin: P,
    boot_mode: DeviceMode,
    candidate: DeviceMode,
    candidate_since_ms: u64,
}

impl<P: InputPin> ModeSwitch<P> {
    pub fn new(mut pin: P, now_ms: u64) -> Self {
        let mode = Self::read_pin(&mut pin);
        Self {
            pin,
            boot_mode: mode,
            candidate: mode,
            candidate_since_ms: now_ms,
        }
    }

    fn read_pin(pin: &mut P) -> DeviceMode {
        let low = pin.is_low().unwrap_or(!CONFIG_MODE_IS_LOW);
        if low == CONFIG_MODE_IS_LOW {
            DeviceMode::Config
        } else {
            DeviceMode::Normal
        }
    }

    /// Mode sampled at start-up; the USB personality is fixed by it.
    pub fn boot_mode(&self) -> DeviceMode {
        self.boot_mode
    }

    /// Returns `true` once the switch has settled on a different mode than at boot.
    pub fn changed(&mut self, now_ms: u64) -> bool {
        let mode = Self::read_pin(&mut self.pin);
        if mode != self.candidate {
            self.candidate = mode;
            self.candidate_since_ms = now_ms;
            return false;
        }
        mode != self.boot_mode && now_ms.wrapping_sub(self.candidate_since_ms) >= SWITCH_DEBOUNCE_MS
    }
}

/// Full chip reset through the watchdog. The device re-enumerates in whatever
/// mode the switch is in.
pub fn reboot(watchdog: &mut Watchdog) -> ! {
    defmt::info!("rebooting");
    watchdog.start(MicrosDurationU32::millis(1));
    loop {
        cortex_m::asm::nop();
    }
}

/// Restarts into the ROM UF2 bootloader (the `RPI-RP2` drive).
pub fn reboot_to_bootloader() -> ! {
    rp2040_hal::rom_data::reset_to_usb_boot(0, 0);
    loop {
        cortex_m::asm::nop();
    }
}
