//! The two application superloops, one per device mode.

pub mod config_mode;
pub mod normal_mode;

use redusb_core::script::Layout;

/// Layout assumed by `STRING` lines until a script says `LAYOUT FR` / `LAYOUT US`.
pub const DEFAULT_LAYOUT: Layout = Layout::Fr;
