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
    let norm_a = normalize_path(path_a);
    let norm_b = normalize_path(path_b);
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
