use crate::core::bookmark::{BookmarkFile, BookmarkItem};
use std::fs;
use std::path::Path;

/// Service for bookmark persistence and file I/O operations.
#[derive(Debug, Clone, Default)]
pub struct BookmarkService;

impl BookmarkService {
    /// Creates a new `BookmarkService` instance.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }

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
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempFileGuard(PathBuf);

    impl Drop for TempFileGuard {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn temporary_path(label: &str) -> TempFileGuard {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("xvw-bookmark-service-{label}-{}-{nonce}.yaml", std::process::id()));
        TempFileGuard(path)
    }

    #[test]
    fn test_bookmark_service_round_trip() {
        let guard = temporary_path("roundtrip");
        let path = &guard.0;

        let items = vec![
            BookmarkItem::new(10, 4, BookmarkColor::Green, "Header"),
            BookmarkItem::new(100, 20, BookmarkColor::Blue, "Payload"),
        ];

        BookmarkService::export_to_file(path, &items, Some(Path::new("test.bin"))).expect("export");
        let loaded = BookmarkService::import_from_file(path).expect("import");

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].offset, 10);
        assert_eq!(loaded[0].size, 4);
        assert_eq!(loaded[0].comment, "Header");
        assert_eq!(loaded[1].offset, 100);
    }

    #[test]
    fn test_bookmark_service_new() {
        let service = BookmarkService::new();
        let cloned = service.clone();
        assert_eq!(format!("{service:?}"), "BookmarkService");
        let _ = cloned;
    }
}
