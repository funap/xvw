use crate::core::bookmark::{BookmarkFile, BookmarkItem};
use std::fs;
use std::path::Path;

/// Service for bookmark persistence and file I/O operations.
pub struct BookmarkService;

impl BookmarkService {
    /// Saves bookmarks to a YAML file on disk.
    pub fn export_to_file(path: &Path, bookmarks: &[BookmarkItem], doc_path: Option<&Path>) -> anyhow::Result<()> {
        let yaml = BookmarkFile::to_yaml(bookmarks, doc_path)?;
        fs::write(path, yaml)?;
        Ok(())
    }

    /// Loads bookmarks from a YAML file on disk.
    pub fn import_from_file(path: &Path) -> anyhow::Result<Vec<BookmarkItem>> {
        let content = fs::read_to_string(path)?;
        BookmarkFile::from_yaml(&content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::bookmark::BookmarkColor;

    #[test]
    fn test_bookmark_service_round_trip() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_bm_service_roundtrip.bookmark.yaml");

        let items = vec![
            BookmarkItem::new(10, 4, BookmarkColor::Green, "Header"),
            BookmarkItem::new(100, 20, BookmarkColor::Blue, "Payload"),
        ];

        BookmarkService::export_to_file(&path, &items, Some(Path::new("test.bin"))).expect("export");
        let loaded = BookmarkService::import_from_file(&path).expect("import");

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].offset, 10);
        assert_eq!(loaded[0].size, 4);
        assert_eq!(loaded[0].comment, "Header");
        assert_eq!(loaded[1].offset, 100);

        let _ = fs::remove_file(path);
    }
}
