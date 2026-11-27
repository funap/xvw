use std::path::{Path, PathBuf};

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
    split_frontmatter_and_body(content).map(|(yaml, _)| yaml)
}

/// Extracts the linked `target_file` path from YAML frontmatter in markdown content.
pub fn extract_target_file(content: &str) -> Option<PathBuf> {
    let yaml = extract_frontmatter_yaml(content)?;
    let val: serde_yaml::Value = serde_yaml::from_str(yaml).ok()?;
    let target = val.get("target_file")?.as_str()?.trim();
    if target.is_empty() { None } else { Some(PathBuf::from(target)) }
}

/// Extracts the first Markdown heading from content, ignoring YAML frontmatter.
pub fn extract_title(content: &str) -> Option<String> {
    let body = if let Some((_yaml, body)) = split_frontmatter_and_body(content) {
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

/// Returns true if two file paths lexically refer to the same target file.
pub fn is_same_target_file(path_a: &Path, path_b: &Path) -> bool {
    if path_a.as_os_str().is_empty() || path_b.as_os_str().is_empty() {
        return false;
    }
    if path_a == path_b {
        return true;
    }
    let norm_a = normalize_path(path_a);
    let norm_b = normalize_path(path_b);
    if norm_a == norm_b {
        return true;
    }
    if norm_a.is_relative() && !norm_a.as_os_str().is_empty() && norm_a != Path::new(".") && norm_b.ends_with(&norm_a) {
        return true;
    }
    if norm_b.is_relative() && !norm_b.as_os_str().is_empty() && norm_b != Path::new(".") && norm_a.ends_with(&norm_b) {
        return true;
    }
    false
}

fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;

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

/// Display mode for the Markdown scratchpad editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScratchpadMode {
    #[default]
    Edit,
    Split,
    Preview,
}

impl ScratchpadMode {
    /// Cycles to the next scratchpad mode (Edit -> Split -> Preview -> Edit).
    pub fn next(self) -> Self {
        match self {
            Self::Edit => Self::Split,
            Self::Split => Self::Preview,
            Self::Preview => Self::Edit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_same_target_file_lexical_normalization() {
        assert!(is_same_target_file(
            Path::new("/foo/baz/../bar/firmware.bin"),
            Path::new("/foo/bar/firmware.bin")
        ));
        assert!(is_same_target_file(Path::new("./bar/firmware.bin"), Path::new("/foo/bar/firmware.bin")));
        assert!(!is_same_target_file(Path::new("."), Path::new("/foo/bar/firmware.bin")));
        assert!(!is_same_target_file(Path::new("a/.."), Path::new("/foo/bar/firmware.bin")));
        assert!(!is_same_target_file(Path::new(""), Path::new("/foo/bar/firmware.bin")));
    }

    #[test]
    fn test_split_frontmatter_and_body() {
        let content = "---\ntarget_file: /path/to/firmware.bin\n---\n# Firmware Bootloader Notes\n\nNotes here.";
        let (yaml, body) = split_frontmatter_and_body(content).expect("must split frontmatter");
        assert_eq!(yaml, "target_file: /path/to/firmware.bin\n");
        assert_eq!(body, "# Firmware Bootloader Notes\n\nNotes here.");

        // CRLF
        let crlf = "---\r\ntarget_file: C:\\firmware.bin\r\n---\r\n# Notes\r\n";
        let (yaml_crlf, body_crlf) = split_frontmatter_and_body(crlf).expect("must split CRLF frontmatter");
        assert_eq!(yaml_crlf, "target_file: C:\\firmware.bin\r\n");
        assert_eq!(body_crlf, "# Notes\r\n");

        // Horizontal rules in body after frontmatter
        let hr = "---\ntarget_file: a.bin\n---\n# Heading\n---\nMore notes";
        let (yaml_hr, body_hr) = split_frontmatter_and_body(hr).expect("must split with HR");
        assert_eq!(yaml_hr, "target_file: a.bin\n");
        assert_eq!(body_hr, "# Heading\n---\nMore notes");

        // No frontmatter
        assert_eq!(split_frontmatter_and_body("# Plain markdown"), None);
        assert_eq!(split_frontmatter_and_body("---\nUnclosed frontmatter"), None);
        assert_eq!(split_frontmatter_and_body(""), None);

        // Indented delimiters should not be treated as frontmatter
        assert_eq!(split_frontmatter_and_body("   ---\ntarget_file: a.bin\n---\n# Notes"), None);
        assert_eq!(split_frontmatter_and_body("---\ntarget_file: a.bin\n   ---\n# Notes"), None);
    }

    #[test]
    fn test_scratchpad_mode_default_and_variants() {
        assert_eq!(ScratchpadMode::default(), ScratchpadMode::Edit);
        assert_ne!(ScratchpadMode::Split, ScratchpadMode::Edit);
        assert_ne!(ScratchpadMode::Split, ScratchpadMode::Preview);
        assert_ne!(ScratchpadMode::Edit, ScratchpadMode::Preview);
    }

    #[test]
    fn test_scratchpad_mode_cycle() {
        assert_eq!(ScratchpadMode::Edit.next(), ScratchpadMode::Split);
        assert_eq!(ScratchpadMode::Split.next(), ScratchpadMode::Preview);
        assert_eq!(ScratchpadMode::Preview.next(), ScratchpadMode::Edit);
    }

    #[test]
    fn test_scratchpad_mode_serde() {
        assert_eq!(serde_json::to_string(&ScratchpadMode::Edit).unwrap(), "\"edit\"");
        assert_eq!(serde_json::to_string(&ScratchpadMode::Split).unwrap(), "\"split\"");
        assert_eq!(serde_json::to_string(&ScratchpadMode::Preview).unwrap(), "\"preview\"");

        assert_eq!(serde_json::from_str::<ScratchpadMode>("\"edit\"").unwrap(), ScratchpadMode::Edit);
        assert_eq!(serde_json::from_str::<ScratchpadMode>("\"split\"").unwrap(), ScratchpadMode::Split);
        assert_eq!(serde_json::from_str::<ScratchpadMode>("\"preview\"").unwrap(), ScratchpadMode::Preview);
    }
}
