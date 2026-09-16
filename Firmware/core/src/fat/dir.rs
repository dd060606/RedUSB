//! Root directory entries.

use heapless::String;

const ATTR_READ_ONLY: u8 = 0x01;
const ATTR_HIDDEN: u8 = 0x02;
const ATTR_SYSTEM: u8 = 0x04;
const ATTR_VOLUME_ID: u8 = 0x08;
const ATTR_DIRECTORY: u8 = 0x10;
#[cfg(test)]
const ATTR_ARCHIVE: u8 = 0x20;
const ATTR_LONG_NAME: u8 = ATTR_READ_ONLY | ATTR_HIDDEN | ATTR_SYSTEM | ATTR_VOLUME_ID;

/// Size of one directory entry on disk.
pub const DIR_ENTRY_SIZE: usize = 32;

/// An 8.3 file name as stored on disk: space padded, upper case, no dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ShortName(pub [u8; 11]);

impl ShortName {
    /// Base name without padding.
    pub fn base(&self) -> &[u8] {
        trim_spaces(&self.0[..8])
    }

    /// Extension without padding.
    pub fn extension(&self) -> &[u8] {
        trim_spaces(&self.0[8..])
    }

    /// Case-insensitive extension comparison, `ext` given without the dot.
    pub fn has_extension(&self, ext: &[u8]) -> bool {
        self.extension().eq_ignore_ascii_case(ext)
    }

    /// Human readable `NAME.EXT` form.
    pub fn display(&self) -> String<12> {
        let mut out = String::new();
        for &b in self.base() {
            let _ = out.push(b as char);
        }
        if !self.extension().is_empty() {
            let _ = out.push('.');
            for &b in self.extension() {
                let _ = out.push(b as char);
            }
        }
        out
    }
}

fn trim_spaces(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().rposition(|&b| b != b' ').map_or(0, |i| i + 1);
    &bytes[..end]
}

/// A regular file found in the root directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirEntry {
    pub name: ShortName,
    pub first_cluster: u32,
    pub size: u32,
}

/// Outcome of decoding one raw directory slot.
pub(crate) enum RawEntry {
    /// A regular file.
    File(DirEntry),
    /// A slot to skip: deleted, long-name fragment, label or directory.
    Skip,
    /// The end-of-directory marker; nothing follows.
    End,
}

pub(crate) fn decode_entry(raw: &[u8]) -> RawEntry {
    debug_assert!(raw.len() >= DIR_ENTRY_SIZE);
    match raw[0] {
        0x00 => return RawEntry::End,
        0xE5 => return RawEntry::Skip,
        _ => {}
    }
    let attr = raw[11];
    if attr & ATTR_LONG_NAME == ATTR_LONG_NAME || attr & (ATTR_VOLUME_ID | ATTR_DIRECTORY) != 0 {
        return RawEntry::Skip;
    }
    let mut name = [0u8; 11];
    name.copy_from_slice(&raw[0..11]);
    // 0x05 escapes a leading 0xE5 in a real name.
    if name[0] == 0x05 {
        name[0] = 0xE5;
    }
    RawEntry::File(DirEntry {
        name: ShortName(name),
        first_cluster: u16::from_le_bytes([raw[26], raw[27]]) as u32,
        size: u32::from_le_bytes([raw[28], raw[29], raw[30], raw[31]]),
    })
}

/// Builds the raw 32 bytes of a volume label entry.
pub(crate) fn volume_label_entry(label: &[u8; 11]) -> [u8; DIR_ENTRY_SIZE] {
    let mut raw = [0u8; DIR_ENTRY_SIZE];
    raw[..11].copy_from_slice(label);
    raw[11] = ATTR_VOLUME_ID;
    raw
}

/// Builds the raw 32 bytes of a regular file entry.
#[cfg(test)]
pub(crate) fn file_entry(name: &ShortName, first_cluster: u32, size: u32) -> [u8; DIR_ENTRY_SIZE] {
    let mut raw = [0u8; DIR_ENTRY_SIZE];
    raw[..11].copy_from_slice(&name.0);
    raw[11] = ATTR_ARCHIVE;
    raw[26..28].copy_from_slice(&(first_cluster as u16).to_le_bytes());
    raw[28..32].copy_from_slice(&size.to_le_bytes());
    raw
}

/// Converts a `NAME.EXT` string into a padded short name, upper-casing ASCII.
pub fn short_name_from_str(name: &str) -> Option<ShortName> {
    let mut out = [b' '; 11];
    let (base, ext) = match name.rsplit_once('.') {
        Some((b, e)) => (b, e),
        None => (name, ""),
    };
    if base.is_empty() || base.len() > 8 || ext.len() > 3 {
        return None;
    }
    for (dst, src) in out[..8].iter_mut().zip(base.bytes()) {
        *dst = src.to_ascii_uppercase();
    }
    for (dst, src) in out[8..].iter_mut().zip(ext.bytes()) {
        *dst = src.to_ascii_uppercase();
    }
    Some(ShortName(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_name_round_trip() {
        let name = short_name_from_str("hello.txt").unwrap();
        assert_eq!(&name.0, b"HELLO   TXT");
        assert_eq!(name.display().as_str(), "HELLO.TXT");
        assert!(name.has_extension(b"txt"));
        assert!(!name.has_extension(b"bin"));
    }

    #[test]
    fn short_name_without_extension() {
        let name = short_name_from_str("README").unwrap();
        assert_eq!(name.display().as_str(), "README");
        assert!(short_name_from_str("waytoolongname.txt").is_none());
    }

    #[test]
    fn decode_skips_special_entries() {
        let mut raw = [0u8; 32];
        assert!(matches!(decode_entry(&raw), RawEntry::End));
        raw[0] = 0xE5;
        assert!(matches!(decode_entry(&raw), RawEntry::Skip));
        raw[0] = b'A';
        raw[11] = ATTR_LONG_NAME;
        assert!(matches!(decode_entry(&raw), RawEntry::Skip));
        raw[11] = ATTR_DIRECTORY;
        assert!(matches!(decode_entry(&raw), RawEntry::Skip));
        raw[11] = ATTR_ARCHIVE;
        assert!(matches!(decode_entry(&raw), RawEntry::File(_)));
    }
}
