//! Passive USB class that guesses the host OS from enumeration requests. It adds no
//! interface or endpoint and never accepts a transfer, only recording each device
//! `GET_DESCRIPTOR`; placed first in the class list it sees every request first.
//! See [`redusb_core::os`] for how the [`Fingerprint`] becomes an [`Os`].

use redusb_core::os::{Fingerprint, Os};
use rp2040_hal::usb::UsbBus;
use usb_device::class::{ControlIn, UsbClass};
use usb_device::control::{Recipient, Request, RequestType};

#[derive(Default)]
pub struct OsFingerprint {
    fingerprint: Fingerprint,
}

impl OsFingerprint {
    pub fn new() -> Self {
        Self::default()
    }

    /// The raw request counters, for logging and calibration.
    pub fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// The current best guess of the host OS.
    pub fn host_os(&self) -> Os {
        self.fingerprint.classify()
    }
}

impl UsbClass<UsbBus> for OsFingerprint {
    fn control_in(&mut self, xfer: ControlIn<UsbBus>) {
        let req = xfer.request();
        if req.request_type == RequestType::Standard
            && req.recipient == Recipient::Device
            && req.request == Request::GET_DESCRIPTOR
        {
            let (desc_type, desc_index) = req.descriptor_type_index();
            self.fingerprint
                .observe_get_descriptor(desc_type, desc_index);
        }
        // Deliberately neither accept nor reject: the standard handler answers it.
    }

    fn reset(&mut self) {
        // Each enumeration attempt starts a fresh fingerprint.
        self.fingerprint = Fingerprint::new();
    }
}
