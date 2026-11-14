use crate::core::format::FileFormat;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The number of structure definitions kept in the recent history.
pub const MAX_RECENT_DEFINITIONS: usize = 5;
/// The number of binary files kept in the recent history.
pub const MAX_RECENT_FILES: usize = MAX_RECENT_DEFINITIONS;

/// Stores paths in most-recently-used order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentPathHistory {
    paths: Vec<PathBuf>,
}

impl RecentPathHistory {
    /// Creates a history from paths ordered newest to oldest.
    pub fn from_paths(paths: Vec<PathBuf>) -> Self {
        let mut history = Self::default();
        for path in paths.into_iter().rev() {
            history.record(path);
        }
        history
    }

    /// Records a path, moving an existing entry to the front.
    pub fn record(&mut self, path: PathBuf) {
        let path = normalize_path(path);
        self.paths.retain(|entry| entry != &path);
        self.paths.insert(0, path);
        self.paths.truncate(MAX_RECENT_DEFINITIONS);
    }

    /// Removes a path and reports whether an entry was removed.
    pub fn remove(&mut self, path: &std::path::Path) -> bool {
        let path = normalize_path(path.to_path_buf());
        let old_len = self.paths.len();
        self.paths.retain(|entry| entry != &path);
        self.paths.len() != old_len
    }

    /// Returns paths from newest to oldest.
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

/// Stores the most recently used Kaitai structure definition paths.
pub type DefinitionHistory = RecentPathHistory;

/// An entry representing a recently opened or imported file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentFileEntry {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<FileFormat>,
}

impl RecentFileEntry {
    pub fn new(path: PathBuf, format: Option<FileFormat>) -> Self {
        Self {
            path: normalize_path(path),
            format,
        }
    }
}

impl From<PathBuf> for RecentFileEntry {
    fn from(path: PathBuf) -> Self {
        Self::new(path, None)
    }
}

/// Stores the most recently opened binary or imported files and their formats.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileHistory {
    entries: Vec<RecentFileEntry>,
}

impl FileHistory {
    /// Creates a history from entries ordered newest to oldest.
    pub fn from_entries(entries: Vec<RecentFileEntry>) -> Self {
        let mut history = Self::default();
        for entry in entries.into_iter().rev() {
            history.record(entry.path, entry.format);
        }
        history
    }

    /// Creates a history from paths ordered newest to oldest with default format.
    pub fn from_paths(paths: Vec<PathBuf>) -> Self {
        let mut history = Self::default();
        for path in paths.into_iter().rev() {
            history.record(path, None);
        }
        history
    }

    /// Records a path and its open format, moving an existing entry to the front.
    pub fn record(&mut self, path: PathBuf, format: Option<FileFormat>) {
        let path = normalize_path(path);
        self.entries.retain(|entry| entry.path != path);
        self.entries.insert(0, RecentFileEntry::new(path, format));
        self.entries.truncate(MAX_RECENT_FILES);
    }

    /// Removes a path and reports whether an entry was removed.
    pub fn remove(&mut self, path: &std::path::Path) -> bool {
        let path = normalize_path(path.to_path_buf());
        let old_len = self.entries.len();
        self.entries.retain(|entry| entry.path != path);
        self.entries.len() != old_len
    }

    /// Returns entries from newest to oldest.
    pub fn entries(&self) -> &[RecentFileEntry] {
        &self.entries
    }

    /// Returns paths from newest to oldest.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.entries.iter().map(|e| e.path.clone()).collect()
    }
}

/// Lexically normalizes `.` and `..` components without blocking filesystem I/O.
fn normalize_path(path: PathBuf) -> PathBuf {
    use std::path::Component;

    if path.as_os_str().is_empty() {
        return path;
    }

    let mut normalized: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.last() {
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                Some(Component::ParentDir) | None => normalized.push(component),
                Some(Component::CurDir) => unreachable!(),
            },
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => normalized.push(component),
        }
    }

    if normalized.is_empty() {
        PathBuf::from(".")
    } else {
        normalized.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_five_unique_paths_in_most_recent_order() {
        let mut history = DefinitionHistory::default();
        for index in 0..=MAX_RECENT_DEFINITIONS {
            history.record(PathBuf::from(format!("definition-{index}.ksy")));
        }

        assert_eq!(history.paths().len(), MAX_RECENT_DEFINITIONS);
        assert_eq!(history.paths().first(), Some(&PathBuf::from("definition-5.ksy")));
        assert!(!history.paths().contains(&PathBuf::from("definition-0.ksy")));
    }

    #[test]
    fn recording_an_existing_path_moves_it_to_the_front() {
        let mut history = DefinitionHistory::default();
        history.record(PathBuf::from("older.ksy"));
        history.record(PathBuf::from("newer.ksy"));
        history.record(PathBuf::from("older.ksy"));

        assert_eq!(history.paths(), &[PathBuf::from("older.ksy"), PathBuf::from("newer.ksy")]);
    }

    #[test]
    fn file_history_records_and_updates_format() {
        let mut history = FileHistory::default();
        history.record(PathBuf::from("file.hex"), Some(FileFormat::IntelHex));
        assert_eq!(history.entries().len(), 1);
        assert_eq!(history.entries()[0].format, Some(FileFormat::IntelHex));

        history.record(PathBuf::from("file.srec"), Some(FileFormat::MotorolaSrec));
        assert_eq!(history.entries().len(), 2);
        assert_eq!(history.entries()[0].path, PathBuf::from("file.srec"));
        assert_eq!(history.entries()[0].format, Some(FileFormat::MotorolaSrec));

        // Updating file.hex moves to front and updates format
        history.record(PathBuf::from("file.hex"), Some(FileFormat::Binary));
        assert_eq!(history.entries().len(), 2);
        assert_eq!(history.entries()[0].path, PathBuf::from("file.hex"));
        assert_eq!(history.entries()[0].format, Some(FileFormat::Binary));
    }

    #[test]
    fn removes_a_path_from_history() {
        let mut history = FileHistory::from_paths(vec![PathBuf::from("new.bin"), PathBuf::from("old.bin")]);

        assert!(history.remove(std::path::Path::new("new.bin")));
        assert_eq!(history.paths(), &[PathBuf::from("old.bin")]);
        assert!(!history.remove(std::path::Path::new("missing.bin")));
    }

    #[test]
    fn normalizes_dot_segments_lexically_without_filesystem_io() {
        let mut history = DefinitionHistory::default();
        history.record(PathBuf::from("/tmp/dir/./sub/../schema.ksy"));
        assert_eq!(history.paths(), &[PathBuf::from("/tmp/dir/schema.ksy")]);
        assert!(history.remove(std::path::Path::new("/tmp/dir/schema.ksy")));
        assert!(history.paths().is_empty());

        let mut files = FileHistory::default();
        files.record(PathBuf::from("./nested/../sample.bin"), Some(FileFormat::Binary));
        assert_eq!(files.paths(), &[PathBuf::from("sample.bin")]);
    }
}
