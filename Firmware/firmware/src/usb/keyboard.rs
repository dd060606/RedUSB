//! USB HID boot keyboard used to play macros.

use super::fingerprint::OsFingerprint;
use super::{build_device, select_keyboard_profile};
use crate::board::millis;
use redusb_core::os::Os;
use redusb_core::script::{Delay, KeyStroke, KeyboardSink, SinkError};
use rp2040_hal::Timer;
use rp2040_hal::usb::UsbBus;
use usb_device::UsbError;
use usb_device::bus::UsbBusAllocator;
use usb_device::device::{UsbDevice, UsbDeviceState};
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};
use usbd_hid::hid_class::{
    HIDClass, HidClassSettings, HidCountryCode, HidProtocol, HidSubClass, ProtocolModeConfig,
};

/// Interval requested from the host between interrupt IN polls.
const POLL_INTERVAL_MS: u8 = 1;
/// Base duration a key stays pressed and a release lasts before the next key.
pub const KEY_HOLD_MS: u32 = 8;
/// Maximum random jitter added on top of KEY_HOLD_MS per press/release (ms).
const JITTER_MAX_MS: u32 = 10;
/// Give up on a report if the host has not fetched the previous one by then.
const SEND_TIMEOUT_MS: u64 = 1000;

pub struct UsbKeyboard {
    hid: HIDClass<'static, UsbBus>,
    fingerprint: OsFingerprint,
    device: UsbDevice<'static, UsbBus>,
    timer: Timer,
    /// Xorshift32 PRNG state for per-keystroke timing jitter, seeded from boot entropy.
    prng_state: u32,
}

/// Advances a 32-bit xorshift PRNG and returns the new state.
fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

impl UsbKeyboard {
    pub fn new(alloc: &'static UsbBusAllocator<UsbBus>, timer: Timer, release: u16) -> Self {
        let settings = HidClassSettings {
            subclass: HidSubClass::Boot,
            protocol: HidProtocol::Keyboard,
            config: ProtocolModeConfig::ForceBoot,
            locale: HidCountryCode::NotSupported,
        };
        let hid = HIDClass::new_ep_in_with_settings(
            alloc,
            KeyboardReport::desc(),
            POLL_INTERVAL_MS,
            settings,
        );
        let profile = select_keyboard_profile(release);
        defmt::info!(
            "usb keyboard: {=str} {=str} release {=u16:#06x}",
            profile.manufacturer,
            profile.product,
            release
        );
        let device = build_device(
            alloc,
            profile.vid,
            profile.pid,
            profile.manufacturer,
            profile.product,
            None, // most commodity keyboards ship without a serial-number descriptor
            false,
            release,
        );
        // Seed the PRNG from boot entropy; multiplicative mix avoids a near-zero state.
        let prng_state = (release as u32).wrapping_mul(0x9E37_79B9) ^ 0xDEAD_CAFE;
        Self {
            hid,
            fingerprint: OsFingerprint::new(),
            device,
            timer,
            prng_state,
        }
    }

    /// Services the bus. Call it continuously.
    ///
    /// The fingerprint class is polled first so it observes every enumeration
    /// request before the HID class or the standard handler consumes it.
    pub fn poll(&mut self) -> bool {
        self.device
            .poll(&mut [&mut self.fingerprint, &mut self.hid])
    }

    pub fn is_configured(&self) -> bool {
        self.device.state() == UsbDeviceState::Configured
    }

    /// The host OS guessed from enumeration (see [`redusb_core::os`]).
    pub fn host_os(&self) -> Os {
        self.fingerprint.host_os()
    }

    /// Raw enumeration counters, for logging and heuristic calibration.
    pub fn fingerprint(&self) -> redusb_core::os::Fingerprint {
        self.fingerprint.fingerprint()
    }

    /// Queues a report, servicing the bus until the endpoint accepts it.
    fn send(&mut self, report: &KeyboardReport) -> Result<(), SinkError> {
        let deadline = millis(self.timer) + SEND_TIMEOUT_MS;
        loop {
            self.poll();
            match self.hid.push_input(report) {
                Ok(_) => return Ok(()),
                Err(UsbError::WouldBlock) => {}
                Err(_) => return Err(SinkError::Timeout),
            }
            if millis(self.timer) > deadline {
                return Err(SinkError::Timeout);
            }
        }
    }
}

impl KeyboardSink for UsbKeyboard {
    fn press(&mut self, stroke: KeyStroke) -> Result<(), SinkError> {
        let report = KeyboardReport {
            modifier: stroke.modifiers,
            reserved: 0,
            leds: 0,
            keycodes: [stroke.usage, 0, 0, 0, 0, 0],
        };
        self.send(&report)?;
        let jitter = xorshift32(&mut self.prng_state) % (JITTER_MAX_MS + 1);
        self.delay_ms(KEY_HOLD_MS + jitter);
        Ok(())
    }

    fn release(&mut self) -> Result<(), SinkError> {
        self.send(&KeyboardReport::default())?;
        let jitter = xorshift32(&mut self.prng_state) % (JITTER_MAX_MS + 1);
        self.delay_ms(KEY_HOLD_MS + jitter);
        Ok(())
    }
}

impl Delay for UsbKeyboard {
    /// Busy-waits while keeping the USB device responsive.
    fn delay_ms(&mut self, ms: u32) {
        let end = millis(self.timer) + ms as u64;
        while millis(self.timer) < end {
            self.poll();
        }
    }
}
