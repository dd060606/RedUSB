//! The list of macro scripts found on the volume and which one is selected.

use crate::block::BlockDevice;
use crate::fat::{DirEntry, FatError, FatVolume, ShortName};
use heapless::Vec;

/// Maximum number of scripts tracked. Extra files are ignored.
pub const MAX_SCRIPTS: usize = 16;

/// Extension (without dot) of files treated as scripts.
pub const SCRIPT_EXTENSION: &[u8] = b"TXT";

/// Sorted, bounded list of script files with a current selection.
#[derive(Debug, Default)]
pub struct ScriptCatalog {
    entries: Vec<DirEntry, MAX_SCRIPTS>,
    selected: usize,
}

impl ScriptCatalog {
    /// A catalog with no scripts.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Lists the `.TXT` files of the root directory, sorted by 8.3 name.
    pub fn scan<D: BlockDevice>(vol: &FatVolume, dev: &mut D) -> Result<Self, FatError> {
        let mut catalog = Self::empty();
        catalog.fill(vol, dev)?;
        Ok(catalog)
    }

    /// Re-reads the directory, keeping the same script selected when it still exists.
    pub fn rescan<D: BlockDevice>(&mut self, vol: &FatVolume, dev: &mut D) -> Result<(), FatError> {
        let previous = self.current().map(|e| e.name);
        self.entries.clear();
        self.fill(vol, dev)?;
        match previous.and_then(|name| self.position_of(&name)) {
            Some(index) => self.selected = index,
            None => self.select(self.selected),
        }
        Ok(())
    }

    fn fill<D: BlockDevice>(&mut self, vol: &FatVolume, dev: &mut D) -> Result<(), FatError> {
        let entries = &mut self.entries;
        vol.for_each_root_file(dev, |entry| {
            if entry.name.has_extension(SCRIPT_EXTENSION) {
                insert_sorted(entries, *entry);
            }
            true
        })?;
        self.select(self.selected);
        Ok(())
    }

    fn position_of(&self, name: &ShortName) -> Option<usize> {
        self.entries.iter().position(|e| &e.name == name)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[DirEntry] {
        &self.entries
    }

    /// Index of the selected script (0 when the catalog is empty).
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Selects `index`, clamped to the available scripts.
    pub fn select(&mut self, index: usize) {
        self.selected = match self.entries.len() {
            0 => 0,
            n => index.min(n - 1),
        };
    }

    /// Moves the selection to the next script, wrapping around.
    pub fn select_next(&mut self) -> usize {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1) % self.entries.len();
        }
        self.selected
    }

    /// The selected script, if any.
    pub fn current(&self) -> Option<&DirEntry> {
        self.entries.get(self.selected)
    }
}

/// Inserts keeping the list sorted by name; when full, the largest name is dropped.
fn insert_sorted(entries: &mut Vec<DirEntry, MAX_SCRIPTS>, entry: DirEntry) {
    let pos = entries
        .iter()
        .position(|e| e.name > entry.name)
        .unwrap_or(entries.len());
    if entries.is_full() {
        if pos == entries.len() {
            return;
        }
        entries.pop();
    }
    // Cannot fail: a slot was freed above if needed.
    let _ = entries.insert(pos, entry);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fat::test_writer::write_file;
    use crate::fat::{FormatParams, format_volume};
    use crate::testing::MemBlockDevice;
    use std::format;

    fn volume() -> (MemBlockDevice, FatVolume) {
        let mut dev = MemBlockDevice::new(14208);
        format_volume(&mut dev, &FormatParams::for_flash(14208, *b"REDUSB     ")).unwrap();
        let vol = FatVolume::mount(&mut dev).unwrap();
        (dev, vol)
    }

    #[test]
    fn scan_filters_and_sorts() {
        let (mut dev, vol) = volume();
        write_file(&mut dev, &vol, "zeta.txt", b"z");
        write_file(&mut dev, &vol, "notes.md", b"m");
        write_file(&mut dev, &vol, "alpha.TXT", b"a");
        let catalog = ScriptCatalog::scan(&vol, &mut dev).unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog.entries()[0].name.display().as_str(), "ALPHA.TXT");
        assert_eq!(catalog.entries()[1].name.display().as_str(), "ZETA.TXT");
    }

    #[test]
    fn selection_wraps_and_clamps() {
        let (mut dev, vol) = volume();
        write_file(&mut dev, &vol, "a.txt", b"");
        write_file(&mut dev, &vol, "b.txt", b"");
        write_file(&mut dev, &vol, "c.txt", b"");
        let mut catalog = ScriptCatalog::scan(&vol, &mut dev).unwrap();
        assert_eq!(catalog.selected(), 0);
        assert_eq!(catalog.select_next(), 1);
        assert_eq!(catalog.select_next(), 2);
        assert_eq!(catalog.select_next(), 0);
        catalog.select(99);
        assert_eq!(catalog.selected(), 2);
        assert_eq!(catalog.current().unwrap().name.display().as_str(), "C.TXT");
    }

    #[test]
    fn empty_catalog_is_inert() {
        let (mut dev, vol) = volume();
        let mut catalog = ScriptCatalog::scan(&vol, &mut dev).unwrap();
        assert!(catalog.is_empty());
        assert_eq!(catalog.select_next(), 0);
        assert!(catalog.current().is_none());
    }

    #[test]
    fn rescan_keeps_selection_by_name() {
        let (mut dev, vol) = volume();
        write_file(&mut dev, &vol, "b.txt", b"");
        write_file(&mut dev, &vol, "c.txt", b"");
        let mut catalog = ScriptCatalog::scan(&vol, &mut dev).unwrap();
        catalog.select(1);
        // A new file sorting before the selection shifts its index.
        write_file(&mut dev, &vol, "a.txt", b"");
        catalog.rescan(&vol, &mut dev).unwrap();
        assert_eq!(catalog.selected(), 2);
        assert_eq!(catalog.current().unwrap().name.display().as_str(), "C.TXT");
    }

    #[test]
    fn overflow_keeps_first_names() {
        let (mut dev, vol) = volume();
        for i in (0..20).rev() {
            write_file(&mut dev, &vol, &format!("s{i:02}.txt"), b"");
        }
        let catalog = ScriptCatalog::scan(&vol, &mut dev).unwrap();
        assert_eq!(catalog.len(), MAX_SCRIPTS);
        assert_eq!(catalog.entries()[0].name.display().as_str(), "S00.TXT");
        assert_eq!(catalog.entries()[15].name.display().as_str(), "S15.TXT");
    }
}
