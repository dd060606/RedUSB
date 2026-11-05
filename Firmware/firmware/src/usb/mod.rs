//! USB personalities: a HID keyboard (normal mode) and a mass-storage drive (config
//! mode). The keyboard rotates its full USB identity (VID, PID, strings) from a pool
//! of verified real keyboards each boot to avoid presenting a custom-tool fingerprint.

pub mod fingerprint;
pub mod keyboard;
pub mod mass_storage;

use rp2040_hal::usb::UsbBus;
use usb_device::bus::UsbBusAllocator;
use usb_device::device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid};

/// USB identity used by the mass-storage config personality (fixed, never rotated).
pub const VID: u16 = 0x1209;
pub const PID_STORAGE: u16 = 0x0002;

/// Fixed USB release (bcdDevice) for the mass-storage personality.
pub const STORAGE_RELEASE: u16 = 0x0100;

/// Current draw declared to the host, in mA.
const MAX_POWER_MA: usize = 100;

/// USB identity of one real commercial keyboard used to blend in with the target.
pub struct KeyboardProfile {
    pub vid: u16,
    pub pid: u16,
    pub manufacturer: &'static str,
    pub product: &'static str,
}

/// Pool of verified wired HID keyboards.
const KEYBOARD_PROFILES: &[KeyboardProfile] = &[
    // Logitech K120 (most common office keyboard worldwide)
    KeyboardProfile { vid: 0x046D, pid: 0xC31C, manufacturer: "Logitech", product: "USB Keyboard" },
    // Logitech Classic Keyboard 200
    KeyboardProfile { vid: 0x046D, pid: 0xC315, manufacturer: "Logitech", product: "Classic Keyboard 200" },
    // Logitech HID-Compliant Keyboard
    KeyboardProfile { vid: 0x046D, pid: 0xC316, manufacturer: "Logitech", product: "HID-Compliant Keyboard" },
    // Logitech Wave Corded Keyboard
    KeyboardProfile { vid: 0x046D, pid: 0xC317, manufacturer: "Logitech", product: "Wave Corded Keyboard" },
    // Dell KB216 Wired Keyboard
    KeyboardProfile { vid: 0x413C, pid: 0x2113, manufacturer: "Dell Inc.", product: "KB216 Wired Keyboard" },
    // Dell KB212-B Quiet Key Keyboard
    KeyboardProfile { vid: 0x413C, pid: 0x2107, manufacturer: "Dell Inc.", product: "KB212-B Quiet Key Keyboard" },
    // Microsoft Wired Keyboard 600
    KeyboardProfile { vid: 0x045E, pid: 0x0750, manufacturer: "Microsoft", product: "Wired Keyboard 600" },
    // Microsoft Wired Keyboard 400
    KeyboardProfile { vid: 0x045E, pid: 0x0752, manufacturer: "Microsoft", product: "Wired Keyboard 400" },
];

/// Returns the keyboard profile for this boot.
pub fn select_keyboard_profile(entropy: u16) -> &'static KeyboardProfile {
    &KEYBOARD_PROFILES[(entropy as usize) % KEYBOARD_PROFILES.len()]
}

/// Builds the device once every class has been allocated on `alloc`. `release` is the
/// USB release number (bcdDevice) reported to the host.
pub fn build_device(
    alloc: &'static UsbBusAllocator<UsbBus>,
    vid: u16,
    pid: u16,
    manufacturer: &'static str,
    product: &'static str,
    serial: Option<&'static str>,
    with_iads: bool,
    release: u16,
) -> UsbDevice<'static, UsbBus> {
    let mut strings = StringDescriptors::default()
        .manufacturer(manufacturer)
        .product(product);
    if let Some(sn) = serial {
        strings = strings.serial_number(sn);
    }
    let mut builder = UsbDeviceBuilder::new(alloc, UsbVidPid(vid, pid))
        .strings(&[strings])
        .expect("usb strings")
        .max_packet_size_0(64)
        .expect("usb ep0 size")
        .max_power(MAX_POWER_MA)
        .expect("usb max power")
        .device_release(release);
    if with_iads {
        builder = builder.composite_with_iads();
    }
    builder.build()
}
