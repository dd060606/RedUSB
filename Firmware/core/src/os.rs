//! Host operating system detection from USB enumeration behaviour.
//!
//! A USB device cannot ask the host what operating system it is, but the sequence
//! of `GET_DESCRIPTOR` requests a host makes while enumerating the device is
//! characteristic of the OS. The firmware feeds every such request into a
//! [`Fingerprint`], which [`Fingerprint::classify`] turns into a best-guess [`Os`].
//!
//! Reliability, in short:
//! * **Windows** is detected reliably: it is the only OS that probes for the
//!   Microsoft OS string descriptor at string index `0xEE`.
//! * **Linux vs macOS** is only a heuristic based on how many times the host reads
//!   the configuration descriptor. The thresholds ([`LINUX_MIN_CONFIG_REQUESTS`])
//!   should be confirmed against the real machines you use; the firmware logs the
//!   raw counters over defmt so you can calibrate them.
//! * Anything unrecognised stays [`Os::Unknown`], which scripts handle with `ELSE`.

/// Standard USB descriptor type codes (the high byte of a `GET_DESCRIPTOR` value).
pub mod descriptor_type {
    pub const DEVICE: u8 = 1;
    pub const CONFIGURATION: u8 = 2;
    pub const STRING: u8 = 3;
    pub const DEVICE_QUALIFIER: u8 = 6;
    pub const BOS: u8 = 15;
}

/// String descriptor index Windows requests to look for a Microsoft OS descriptor.
pub const MS_OS_STRING_INDEX: u8 = 0xEE;

/// A non-Windows host that reads the configuration descriptor at least this many
/// times is guessed to be Linux; fewer reads are guessed to be macOS. This is a
/// heuristic: calibrate it against your own hosts using the raw counters.
pub const LINUX_MIN_CONFIG_REQUESTS: u16 = 3;

/// Host operating system, guessed from USB enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Os {
    /// Not yet determined, or the enumeration pattern matched nothing known.
    #[default]
    Unknown,
    Windows,
    MacOs,
    Linux,
}

impl Os {
    /// Parses the argument of an `IF_OS` line (case-insensitive).
    pub fn from_name(name: &str) -> Option<Os> {
        let matches = |candidates: &[&str]| candidates.iter().any(|c| name.eq_ignore_ascii_case(c));
        if matches(&["WINDOWS", "WIN"]) {
            Some(Os::Windows)
        } else if matches(&["MAC", "MACOS", "OSX", "DARWIN"]) {
            Some(Os::MacOs)
        } else if matches(&["LINUX", "NIX"]) {
            Some(Os::Linux)
        } else if matches(&["UNKNOWN", "OTHER"]) {
            Some(Os::Unknown)
        } else {
            None
        }
    }

    /// Stable lower-case name, handy for logging.
    pub fn name(self) -> &'static str {
        match self {
            Os::Unknown => "unknown",
            Os::Windows => "windows",
            Os::MacOs => "macos",
            Os::Linux => "linux",
        }
    }
}

/// Counts of the descriptor requests seen during one enumeration.
///
/// The firmware calls [`Fingerprint::observe_get_descriptor`] for every
/// device-directed `GET_DESCRIPTOR` request; [`Fingerprint::classify`] reads the
/// result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Fingerprint {
    /// The host asked for the Microsoft OS string descriptor (index `0xEE`).
    pub ms_os_descriptor: bool,
    pub device_requests: u16,
    pub config_requests: u16,
    pub string_requests: u16,
    pub qualifier_requests: u16,
    pub bos_requests: u16,
    pub other_requests: u16,
}

impl Fingerprint {
    /// An empty fingerprint, before any request has been seen.
    pub const fn new() -> Self {
        Self {
            ms_os_descriptor: false,
            device_requests: 0,
            config_requests: 0,
            string_requests: 0,
            qualifier_requests: 0,
            bos_requests: 0,
            other_requests: 0,
        }
    }

    /// Records one `GET_DESCRIPTOR` request from its type and index.
    pub fn observe_get_descriptor(&mut self, desc_type: u8, desc_index: u8) {
        match desc_type {
            descriptor_type::DEVICE => {
                self.device_requests = self.device_requests.saturating_add(1)
            }
            descriptor_type::CONFIGURATION => {
                self.config_requests = self.config_requests.saturating_add(1)
            }
            descriptor_type::STRING => {
                self.string_requests = self.string_requests.saturating_add(1);
                if desc_index == MS_OS_STRING_INDEX {
                    self.ms_os_descriptor = true;
                }
            }
            descriptor_type::DEVICE_QUALIFIER => {
                self.qualifier_requests = self.qualifier_requests.saturating_add(1)
            }
            descriptor_type::BOS => self.bos_requests = self.bos_requests.saturating_add(1),
            _ => self.other_requests = self.other_requests.saturating_add(1),
        }
    }

    /// Whether nothing has been observed yet.
    pub fn is_empty(&self) -> bool {
        *self == Fingerprint::new()
    }

    /// Best guess of the host OS.
    pub fn classify(&self) -> Os {
        // Reliable: only Windows probes for the Microsoft OS descriptor.
        if self.ms_os_descriptor {
            return Os::Windows;
        }
        // Nothing meaningful seen yet.
        if self.device_requests == 0 && self.config_requests == 0 {
            return Os::Unknown;
        }
        // Heuristic split of the remaining (non-Windows) hosts. See the module docs.
        if self.config_requests >= LINUX_MIN_CONFIG_REQUESTS {
            Os::Linux
        } else if self.config_requests >= 1 {
            Os::MacOs
        } else {
            Os::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_names_parse() {
        assert_eq!(Os::from_name("windows"), Some(Os::Windows));
        assert_eq!(Os::from_name("WIN"), Some(Os::Windows));
        assert_eq!(Os::from_name("mac"), Some(Os::MacOs));
        assert_eq!(Os::from_name("OSX"), Some(Os::MacOs));
        assert_eq!(Os::from_name("Linux"), Some(Os::Linux));
        assert_eq!(Os::from_name("other"), Some(Os::Unknown));
        assert_eq!(Os::from_name("beos"), None);
    }

    #[test]
    fn ms_os_descriptor_means_windows() {
        let mut fp = Fingerprint::new();
        fp.observe_get_descriptor(descriptor_type::STRING, MS_OS_STRING_INDEX);
        assert!(fp.ms_os_descriptor);
        assert_eq!(fp.classify(), Os::Windows);
    }

    #[test]
    fn ordinary_string_reads_are_not_ms_os() {
        let mut fp = Fingerprint::new();
        fp.observe_get_descriptor(descriptor_type::STRING, 0x02);
        assert!(!fp.ms_os_descriptor);
        assert_eq!(fp.string_requests, 1);
    }

    #[test]
    fn empty_is_unknown() {
        assert!(Fingerprint::new().is_empty());
        assert_eq!(Fingerprint::new().classify(), Os::Unknown);
    }

    #[test]
    fn counters_route_by_type() {
        let mut fp = Fingerprint::new();
        fp.observe_get_descriptor(descriptor_type::DEVICE, 0);
        fp.observe_get_descriptor(descriptor_type::CONFIGURATION, 0);
        fp.observe_get_descriptor(descriptor_type::CONFIGURATION, 0);
        fp.observe_get_descriptor(descriptor_type::DEVICE_QUALIFIER, 0);
        fp.observe_get_descriptor(descriptor_type::BOS, 0);
        fp.observe_get_descriptor(99, 0);
        assert_eq!(fp.device_requests, 1);
        assert_eq!(fp.config_requests, 2);
        assert_eq!(fp.qualifier_requests, 1);
        assert_eq!(fp.bos_requests, 1);
        assert_eq!(fp.other_requests, 1);
        assert!(!fp.is_empty());
    }

    // Current heuristic thresholds for non-Windows hosts. These pin the documented
    // behaviour; adjust them (and this test) after calibrating on real hardware.
    #[test]
    fn non_windows_config_read_heuristic() {
        let with_config = |n: u16| {
            let mut fp = Fingerprint::new();
            fp.device_requests = 1;
            fp.config_requests = n;
            fp.classify()
        };
        assert_eq!(with_config(1), Os::MacOs);
        assert_eq!(with_config(2), Os::MacOs);
        assert_eq!(with_config(LINUX_MIN_CONFIG_REQUESTS), Os::Linux);
        assert_eq!(with_config(LINUX_MIN_CONFIG_REQUESTS + 2), Os::Linux);
    }
}
