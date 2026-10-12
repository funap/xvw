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
    pub target_file: Option<PathBuf>,
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

    /// Splits markdown content into YAML frontmatter and document body if a frontmatter
    /// block is present at the beginning of `content`.
    pub fn split_frontmatter_and_body(content: &str) -> Option<(&str, &str)> {
        let text = content.strip_prefix('\u{feff}').unwrap_or(content);
        let mut lines = text.split_inclusive('\n');
        let first_line = lines.next()?;
        let delimiter_first = first_line.trim_end_matches(['\r', '\n']);
        if delimiter_first != "---" {
            return None;
        }

        let mut current_offset = first_line.len();
        for line in lines {
            let delimiter = line.trim_end_matches(['\r', '\n']);
            if delimiter == "---" || delimiter == "..." {
                let yaml = &text[first_line.len()..current_offset];
                let body_offset = current_offset + line.len();
                let body = &text[body_offset..];
                return Some((yaml, body));
            }
            current_offset += line.len();
        }
        None
    }

    /// Extracts the raw YAML string within markdown frontmatter delimiters, if present.
    pub fn extract_frontmatter_yaml(content: &str) -> Option<&str> {
        Self::split_frontmatter_and_body(content).map(|(yaml, _)| yaml)
    }

    /// Extracts the linked `target_file` path from YAML frontmatter in markdown content.
    pub fn extract_target_file(content: &str) -> Option<PathBuf> {
        let yaml = Self::extract_frontmatter_yaml(content)?;
        let val: serde_yaml::Value = serde_yaml::from_str(yaml).ok()?;
        let target = val.get("target_file")?.as_str()?.trim();
        if target.is_empty() { None } else { Some(PathBuf::from(target)) }
    }

    /// Extracts the first Markdown heading from content, ignoring YAML frontmatter.
    pub fn extract_title(content: &str) -> Option<String> {
        let body = if let Some((_yaml, body)) = Self::split_frontmatter_and_body(content) {
            body
        } else {
            content
        };
        for line in body.lines() {
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

    /// Generates markdown content with YAML frontmatter linking to `target_file`,
    /// or simple heading markdown if `target_file` is None.
    pub fn format_scratch_content(target_file: Option<&Path>, custom_title: Option<&str>) -> String {
        let heading = if let Some(title) = custom_title.map(str::trim).filter(|s| !s.is_empty()) {
            title
        } else {
            target_file
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("Scratchpad")
        };

        if let Some(target) = target_file.filter(|p| !p.as_os_str().is_empty()) {
            #[derive(serde::Serialize)]
            struct Frontmatter<'a> {
                target_file: &'a Path,
            }
            let yaml = serde_yaml::to_string(&Frontmatter { target_file: target }).unwrap_or_else(|_| format!("target_file: \"{}\"\n", target.display()));
            format!("---\n{yaml}---\n# {heading}\n")
        } else {
            format!("# {heading}\n")
        }
    }

    /// Returns true if two file paths refer to the same target file.
    pub fn is_same_target_file(path_a: &Path, path_b: &Path) -> bool {
        if path_a.as_os_str().is_empty() || path_b.as_os_str().is_empty() {
            return false;
        }
        if path_a == path_b {
            return true;
        }
        if let (Ok(canon_a), Ok(canon_b)) = (path_a.canonicalize(), path_b.canonicalize())
            && canon_a == canon_b
        {
            return true;
        }
        let norm_a = Self::normalize_path(path_a);
        let norm_b = Self::normalize_path(path_b);
        if norm_a == norm_b {
            return true;
        }
        if norm_a.is_relative() && norm_b.ends_with(&norm_a) {
            return true;
        }
        if norm_b.is_relative() && norm_a.ends_with(&norm_b) {
            return true;
        }
        false
    }

    fn normalize_path(path: &Path) -> PathBuf {
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::CurDir => {}
                _ => normalized.push(component.as_os_str()),
            }
        }
        normalized
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
            let target_file = Self::extract_target_file(&content);

            results.push(ScratchEntry {
                id,
                filename,
                path,
                title,
                modified,
                target_file,
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

    #[test]
    fn test_sequential_scratch_creation_and_title_handling() {
        let temp_dir = std::env::temp_dir().join(format!("xvw_test_seq_scratch_{}", std::process::id()));
        let file1 = temp_dir.join("scratch_1.md");
        let file2 = temp_dir.join("scratch_2.md");

        // Both have the same default title "# Scratchpad"
        let default_content = "# Scratchpad\n\nNotes";
        assert!(ScratchService::save_scratch_atomic(&file1, default_content).is_ok());
        assert!(ScratchService::save_scratch_atomic(&file2, default_content).is_ok());

        assert_eq!(ScratchService::extract_title(default_content), Some("Scratchpad".to_string()));
        assert_eq!(ScratchService::extract_id_from_filename("scratch_1.md"), Some(1));
        assert_eq!(ScratchService::extract_id_from_filename("scratch_2.md"), Some(2));

        // When open_ids has [1, 2], next_available_id should be at least 3
        let next_id = ScratchService::next_available_id(&[1, 2]);
        assert!(next_id >= 3);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_split_frontmatter_and_body() {
        let content = "---\ntarget_file: /path/to/firmware.bin\n---\n# Firmware Bootloader Notes\n\nNotes here.";
        let (yaml, body) = ScratchService::split_frontmatter_and_body(content).expect("must split frontmatter");
        assert_eq!(yaml, "target_file: /path/to/firmware.bin\n");
        assert_eq!(body, "# Firmware Bootloader Notes\n\nNotes here.");

        // CRLF
        let crlf = "---\r\ntarget_file: C:\\firmware.bin\r\n---\r\n# Notes\r\n";
        let (yaml_crlf, body_crlf) = ScratchService::split_frontmatter_and_body(crlf).expect("must split CRLF frontmatter");
        assert_eq!(yaml_crlf, "target_file: C:\\firmware.bin\r\n");
        assert_eq!(body_crlf, "# Notes\r\n");

        // Horizontal rules in body after frontmatter
        let hr = "---\ntarget_file: a.bin\n---\n# Heading\n---\nMore notes";
        let (yaml_hr, body_hr) = ScratchService::split_frontmatter_and_body(hr).expect("must split with HR");
        assert_eq!(yaml_hr, "target_file: a.bin\n");
        assert_eq!(body_hr, "# Heading\n---\nMore notes");

        // No frontmatter
        assert_eq!(ScratchService::split_frontmatter_and_body("# Plain markdown"), None);
        assert_eq!(ScratchService::split_frontmatter_and_body("---\nUnclosed frontmatter"), None);
        assert_eq!(ScratchService::split_frontmatter_and_body(""), None);

        // Indented delimiters should not be treated as frontmatter
        assert_eq!(ScratchService::split_frontmatter_and_body("   ---\ntarget_file: a.bin\n---\n# Notes"), None);
        assert_eq!(ScratchService::split_frontmatter_and_body("---\ntarget_file: a.bin\n   ---\n# Notes"), None);
    }

    #[test]
    fn test_extract_target_file() {
        use std::path::PathBuf;

        let content = "---\ntarget_file: /path/to/firmware.bin\n---\n# Notes";
        assert_eq!(ScratchService::extract_target_file(content), Some(PathBuf::from("/path/to/firmware.bin")));

        let with_sha = "---\ntarget_file: /path/to/firmware.bin\nsha256: e3b0c44298\n---\n# Notes";
        assert_eq!(ScratchService::extract_target_file(with_sha), Some(PathBuf::from("/path/to/firmware.bin")));

        let quoted = "---\ntarget_file: \"/path with spaces/firmware.bin\"\n---\n# Notes";
        assert_eq!(
            ScratchService::extract_target_file(quoted),
            Some(PathBuf::from("/path with spaces/firmware.bin"))
        );

        let empty_target = "---\ntarget_file: \"\"\n---\n# Notes";
        assert_eq!(ScratchService::extract_target_file(empty_target), None);

        let no_target = "---\nother_prop: 123\n---\n# Notes";
        assert_eq!(ScratchService::extract_target_file(no_target), None);

        let plain = "# Notes without frontmatter";
        assert_eq!(ScratchService::extract_target_file(plain), None);
    }

    #[test]
    fn test_extract_title_with_frontmatter_comment() {
        let content = "---\ntarget_file: /path/to/firmware.bin\n# This is a YAML comment\n---\n# Actual Title\n";
        assert_eq!(ScratchService::extract_title(content), Some("Actual Title".to_string()));
    }

    #[test]
    fn test_format_scratch_content() {
        use std::path::Path;

        let formatted_with_target = ScratchService::format_scratch_content(Some(Path::new("/path/to/firmware.bin")), Some("Firmware Bootloader Notes"));
        assert_eq!(
            formatted_with_target,
            "---\ntarget_file: /path/to/firmware.bin\n---\n# Firmware Bootloader Notes\n"
        );

        let formatted_default_title = ScratchService::format_scratch_content(Some(Path::new("/path/to/firmware.bin")), None);
        assert_eq!(formatted_default_title, "---\ntarget_file: /path/to/firmware.bin\n---\n# firmware.bin\n");

        let formatted_no_target = ScratchService::format_scratch_content(None, Some("Custom Title"));
        assert_eq!(formatted_no_target, "# Custom Title\n");

        let formatted_default = ScratchService::format_scratch_content(None, None);
        assert_eq!(formatted_default, "# Scratchpad\n");

        // Special characters in path (e.g., hash, spaces) must round-trip correctly
        let special_path = Path::new("/path/with space/#1/firmware.bin");
        let formatted_special = ScratchService::format_scratch_content(Some(special_path), None);
        let extracted_special = ScratchService::extract_target_file(&formatted_special);
        assert_eq!(extracted_special, Some(special_path.to_path_buf()));
    }

    #[test]
    fn test_is_same_target_file() {
        use std::path::Path;

        assert!(ScratchService::is_same_target_file(
            Path::new("/foo/bar/firmware.bin"),
            Path::new("/foo/bar/firmware.bin")
        ));
        assert!(ScratchService::is_same_target_file(
            Path::new("firmware.bin"),
            Path::new("/foo/bar/firmware.bin")
        ));
        assert!(ScratchService::is_same_target_file(
            Path::new("./bar/firmware.bin"),
            Path::new("/foo/bar/firmware.bin")
        ));
        assert!(!ScratchService::is_same_target_file(
            Path::new("/foo/bar/firmware.bin"),
            Path::new("/foo/bar/other.bin")
        ));
        assert!(!ScratchService::is_same_target_file(Path::new("other.bin"), Path::new("/foo/bar/firmware.bin")));

        // Empty paths must return false
        assert!(!ScratchService::is_same_target_file(Path::new(""), Path::new("/foo/bar/firmware.bin")));
        assert!(!ScratchService::is_same_target_file(Path::new("/foo/bar/firmware.bin"), Path::new("")));
        assert!(!ScratchService::is_same_target_file(Path::new(""), Path::new("")));
    }
}
