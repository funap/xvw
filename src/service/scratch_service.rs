use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const APPLICATION_CONFIG_DIRECTORY: &str = "xvw";
const SCRATCHES_DIRECTORY: &str = "scratches";

/// Information about a saved scratchpad file on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScratchEntry {
    pub id: Option<usize>,
    pub filename: String,
    pub path: PathBuf,
    pub title: String,
    pub modified: Option<SystemTime>,
}

pub struct ScratchService;

impl ScratchService {
    /// Returns the directory path used to persist scratchpad files on this platform.
    pub fn scratches_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join(APPLICATION_CONFIG_DIRECTORY).join(SCRATCHES_DIRECTORY))
    }

    /// Ensures the scratchpad persistence directory exists, returning its path.
    pub fn ensure_scratches_dir() -> Result<PathBuf, std::io::Error> {
        let dir = Self::scratches_dir().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Could not determine application config directory"))?;
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Computes the default file path for a scratchpad given its numerical ID (`scratch_{id}.md`).
    pub fn scratch_file_path(id: usize) -> Option<PathBuf> {
        Self::scratches_dir().map(|dir| dir.join(format!("scratch_{id}.md")))
    }

    /// Extracts the numerical ID from a file name following the `scratch_{id}.md` convention.
    pub fn extract_id_from_filename(filename: &str) -> Option<usize> {
        let name = filename.strip_suffix(".md").or_else(|| filename.strip_suffix(".MD"))?;
        let id_str = name.strip_prefix("scratch_")?;
        id_str.parse::<usize>().ok()
    }

    /// Extracts the first Markdown heading from content, ignoring leading whitespace.
    pub fn extract_title(content: &str) -> Option<String> {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(heading) = trimmed.strip_prefix('#') {
                let title = heading.trim_start_matches('#').trim();
                if !title.is_empty() {
                    return Some(title.to_string());
                }
            }
        }
        None
    }

    /// Determines the next available scratchpad ID considering existing files on disk and open tabs.
    pub fn next_available_id(open_ids: &[usize]) -> usize {
        let mut max_id = 0;
        for &id in open_ids {
            if id > max_id {
                max_id = id;
            }
        }

        if let Some(dir) = Self::scratches_dir()
            && let Ok(entries) = fs::read_dir(dir)
        {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string()
                    && let Some(id) = Self::extract_id_from_filename(&name)
                    && id > max_id
                {
                    max_id = id;
                }
            }
        }

        max_id + 1
    }

    /// Scans the `scratches/` directory and returns all Markdown scratchpad files,
    /// sorted by modification time descending (most recently updated first).
    pub fn list_scratches() -> Vec<ScratchEntry> {
        let Some(dir) = Self::scratches_dir() else {
            return Vec::new();
        };

        if !dir.exists() {
            return Vec::new();
        }

        let Ok(entries) = fs::read_dir(&dir) else {
            return Vec::new();
        };

        let mut results = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let is_md = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
            if !is_md {
                continue;
            }

            let filename = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let id = Self::extract_id_from_filename(&filename);

            // Read content (or prefix) to extract heading title
            let content = fs::read_to_string(&path).unwrap_or_default();
            let title = Self::extract_title(&content).unwrap_or_else(|| {
                if let Some(id) = id {
                    format!("Scratchpad {}", id)
                } else {
                    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| filename.clone())
                }
            });

            let modified = entry.metadata().ok().and_then(|m| m.modified().ok());

            results.push(ScratchEntry {
                id,
                filename,
                path,
                title,
                modified,
            });
        }

        results.sort_by(|a, b| match (a.modified, b.modified) {
            (Some(ta), Some(tb)) => tb.cmp(&ta),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.filename.cmp(&b.filename),
        });

        results
    }

    /// Loads the text content of a scratchpad file.
    pub fn load_scratch(path: &Path) -> Result<String, std::io::Error> {
        fs::read_to_string(path)
    }

    /// Atomically saves content to the target path using a temporary file in the same directory.
    pub fn save_scratch_atomic(path: &Path, content: &str) -> Result<(), std::io::Error> {
        let parent = path
            .parent()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Target path has no parent directory"))?;
        fs::create_dir_all(parent)?;

        let temp_filename = format!(
            ".tmp_{}_{}_{}",
            std::process::id(),
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0),
            path.file_name().and_then(|n| n.to_str()).unwrap_or("scratch")
        );
        let temp_path = parent.join(temp_filename);

        fs::write(&temp_path, content)?;
        if let Err(err) = fs::rename(&temp_path, path) {
            let _ = fs::remove_file(&temp_path);
            return Err(err);
        }

        Ok(())
    }

    /// Deletes a scratchpad file from disk.
    pub fn delete_scratch(path: &Path) -> Result<(), std::io::Error> {
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ScratchService;

    #[test]
    fn test_extract_title_h1() {
        let content = "# My Scratchpad Title\n\nSome notes here.";
        assert_eq!(ScratchService::extract_title(content), Some("My Scratchpad Title".to_string()));
    }

    #[test]
    fn test_extract_title_h2_with_leading_spaces() {
        let content = "\n\n   ## Secondary Header\nBody text";
        assert_eq!(ScratchService::extract_title(content), Some("Secondary Header".to_string()));
    }

    #[test]
    fn test_extract_title_no_heading() {
        let content = "Just plain text without any heading.\nLine 2";
        assert_eq!(ScratchService::extract_title(content), None);
    }

    #[test]
    fn test_extract_title_empty() {
        assert_eq!(ScratchService::extract_title(""), None);
        assert_eq!(ScratchService::extract_title("   #   \n"), None);
    }

    #[test]
    fn test_extract_id_from_filename() {
        assert_eq!(ScratchService::extract_id_from_filename("scratch_1.md"), Some(1));
        assert_eq!(ScratchService::extract_id_from_filename("scratch_42.md"), Some(42));
        assert_eq!(ScratchService::extract_id_from_filename("scratch_100.MD"), Some(100));
        assert_eq!(ScratchService::extract_id_from_filename("scratch_.md"), None);
        assert_eq!(ScratchService::extract_id_from_filename("scratch_abc.md"), None);
        assert_eq!(ScratchService::extract_id_from_filename("other_file.md"), None);
        assert_eq!(ScratchService::extract_id_from_filename("scratch_1.txt"), None);
    }

    #[test]
    fn test_next_available_id_empty() {
        let id = ScratchService::next_available_id(&[]);
        assert!(id >= 1);
    }

    #[test]
    fn test_next_available_id_with_open_ids() {
        let id = ScratchService::next_available_id(&[1, 5, 3]);
        assert!(id >= 6);
    }

    #[test]
    fn test_atomic_save_and_load_scratch() {
        let temp_dir = std::env::temp_dir().join(format!("xvw_test_scratch_{}", std::process::id()));
        let file_path = temp_dir.join("scratch_999.md");

        let content = "# Test Protocol\n\nData notes 123";
        let res = ScratchService::save_scratch_atomic(&file_path, content);
        assert!(res.is_ok());

        let loaded = ScratchService::load_scratch(&file_path).unwrap();
        assert_eq!(loaded, content);

        let del_res = ScratchService::delete_scratch(&file_path);
        assert!(del_res.is_ok());
        assert!(!file_path.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
