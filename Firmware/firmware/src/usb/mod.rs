//! USB personalities: a HID keyboard in normal mode, a mass-storage drive in
//! configuration mode. Both share the vendor ID and strings; the product ID
//! differs so hosts never reuse a cached descriptor from the other mode.

pub mod fingerprint;
pub mod keyboard;
pub mod mass_storage;

use rp2040_hal::usb::UsbBus;
use usb_device::bus::UsbBusAllocator;
use usb_device::device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid};

/// pid.codes vendor ID. The product IDs below are the pid.codes *test* IDs and
/// must be replaced before the device is distributed.
pub const VID: u16 = 0x1209;
pub const PID_KEYBOARD: u16 = 0x0001;
pub const PID_STORAGE: u16 = 0x0002;

const MANUFACTURER: &str = "RedUSB";
const SERIAL_NUMBER: &str = "REDUSB-0001";
/// Current draw declared to the host, in mA.
const MAX_POWER_MA: usize = 100;

/// Builds the device once every class has been allocated on `alloc`.
pub fn build_device(
    alloc: &'static UsbBusAllocator<UsbBus>,
    pid: u16,
    product: &'static str,
    with_iads: bool,
) -> UsbDevice<'static, UsbBus> {
    let strings = StringDescriptors::default()
        .manufacturer(MANUFACTURER)
        .product(product)
        .serial_number(SERIAL_NUMBER);
    let mut builder = UsbDeviceBuilder::new(alloc, UsbVidPid(VID, pid))
        .strings(&[strings])
        .expect("usb strings")
        .max_packet_size_0(64)
        .expect("usb ep0 size")
        .max_power(MAX_POWER_MA)
        .expect("usb max power");
    if with_iads {
        builder = builder.composite_with_iads();
    }
    builder.build()
}
