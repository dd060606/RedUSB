//! On-chip flash storage: raw access, the cached block device backing the FAT
//! volume, and the persisted settings.

pub mod cached;
pub mod flash;
pub mod layout;
pub mod settings;
