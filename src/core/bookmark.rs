use crate::core::color::RgbaColor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, HashSet};
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static BOOKMARK_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn generate_bookmark_id() -> String {
    let id = BOOKMARK_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("bm-{}", id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BookmarkColor {
    Red,
    Orange,
    #[default]
    Yellow,
    Green,
    Cyan,
    Blue,
    Purple,
    Pink,
    Custom {
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    },
}

impl BookmarkColor {
    pub const ALL_PRESETS: &'static [BookmarkColor] = &[
        BookmarkColor::Red,
        BookmarkColor::Orange,
        BookmarkColor::Yellow,
        BookmarkColor::Green,
        BookmarkColor::Cyan,
        BookmarkColor::Blue,
        BookmarkColor::Purple,
        BookmarkColor::Pink,
    ];

    /// Returns the RGBA color for this bookmark preset.
    pub fn to_rgba(self) -> RgbaColor {
        match self {
            BookmarkColor::Red => RgbaColor::from_hsla_f32(0.0, 0.75, 0.55, 0.35),
            BookmarkColor::Orange => RgbaColor::from_hsla_f32(30.0 / 360.0, 0.85, 0.55, 0.35),
            BookmarkColor::Yellow => RgbaColor::from_hsla_f32(50.0 / 360.0, 0.85, 0.50, 0.35),
            BookmarkColor::Green => RgbaColor::from_hsla_f32(120.0 / 360.0, 0.65, 0.45, 0.35),
            BookmarkColor::Cyan => RgbaColor::from_hsla_f32(180.0 / 360.0, 0.70, 0.45, 0.35),
            BookmarkColor::Blue => RgbaColor::from_hsla_f32(215.0 / 360.0, 0.75, 0.55, 0.35),
            BookmarkColor::Purple => RgbaColor::from_hsla_f32(280.0 / 360.0, 0.70, 0.55, 0.35),
            BookmarkColor::Pink => RgbaColor::from_hsla_f32(330.0 / 360.0, 0.75, 0.55, 0.35),
            BookmarkColor::Custom { r, g, b, a } => RgbaColor::new(r, g, b, a),
        }
    }

    /// Solid / opaque RGBA color for badges, icon swatches, or UI indicator dots.
    pub fn to_badge_rgba(self) -> RgbaColor {
        match self {
            BookmarkColor::Red => RgbaColor::from_hsla_f32(0.0, 0.85, 0.60, 1.0),
            BookmarkColor::Orange => RgbaColor::from_hsla_f32(30.0 / 360.0, 0.90, 0.60, 1.0),
            BookmarkColor::Yellow => RgbaColor::from_hsla_f32(50.0 / 360.0, 0.90, 0.55, 1.0),
            BookmarkColor::Green => RgbaColor::from_hsla_f32(120.0 / 360.0, 0.75, 0.50, 1.0),
            BookmarkColor::Cyan => RgbaColor::from_hsla_f32(180.0 / 360.0, 0.80, 0.50, 1.0),
            BookmarkColor::Blue => RgbaColor::from_hsla_f32(215.0 / 360.0, 0.85, 0.60, 1.0),
            BookmarkColor::Purple => RgbaColor::from_hsla_f32(280.0 / 360.0, 0.80, 0.60, 1.0),
            BookmarkColor::Pink => RgbaColor::from_hsla_f32(330.0 / 360.0, 0.85, 0.60, 1.0),
            BookmarkColor::Custom { r, g, b, .. } => RgbaColor::rgb(r, g, b),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            BookmarkColor::Red => "Red",
            BookmarkColor::Orange => "Orange",
            BookmarkColor::Yellow => "Yellow",
            BookmarkColor::Green => "Green",
            BookmarkColor::Cyan => "Cyan",
            BookmarkColor::Blue => "Blue",
            BookmarkColor::Purple => "Purple",
            BookmarkColor::Pink => "Pink",
            BookmarkColor::Custom { .. } => "Custom",
        }
    }

    pub fn from_rgba(rgba: RgbaColor) -> Self {
        let h_deg = rgba.hue();
        if (0.0..15.0).contains(&h_deg) || (345.0..=360.0).contains(&h_deg) {
            BookmarkColor::Red
        } else if (15.0..40.0).contains(&h_deg) {
            BookmarkColor::Orange
        } else if (40.0..80.0).contains(&h_deg) {
            BookmarkColor::Yellow
        } else if (80.0..150.0).contains(&h_deg) {
            BookmarkColor::Green
        } else if (150.0..200.0).contains(&h_deg) {
            BookmarkColor::Cyan
        } else if (200.0..250.0).contains(&h_deg) {
            BookmarkColor::Blue
        } else if (250.0..310.0).contains(&h_deg) {
            BookmarkColor::Purple
        } else {
            BookmarkColor::Pink
        }
    }
}

impl Serialize for BookmarkColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            BookmarkColor::Red => serializer.serialize_str("red"),
            BookmarkColor::Orange => serializer.serialize_str("orange"),
            BookmarkColor::Yellow => serializer.serialize_str("yellow"),
            BookmarkColor::Green => serializer.serialize_str("green"),
            BookmarkColor::Cyan => serializer.serialize_str("cyan"),
            BookmarkColor::Blue => serializer.serialize_str("blue"),
            BookmarkColor::Purple => serializer.serialize_str("purple"),
            BookmarkColor::Pink => serializer.serialize_str("pink"),
            BookmarkColor::Custom { r, g, b, a } => {
                if *a == 255 {
                    serializer.serialize_str(&format!("#{:02x}{:02x}{:02x}", r, g, b))
                } else {
                    serializer.serialize_str(&format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a))
                }
            }
        }
    }
}

impl<'de> Deserialize<'de> for BookmarkColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ColorVisitor;

        impl<'de> serde::de::Visitor<'de> for ColorVisitor {
            type Value = BookmarkColor;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a color string (e.g. 'red', 'blue', '#ff0000') or an RGBA object")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let lower = v.trim().to_lowercase();
                match lower.as_str() {
                    "red" => Ok(BookmarkColor::Red),
                    "orange" => Ok(BookmarkColor::Orange),
                    "yellow" => Ok(BookmarkColor::Yellow),
                    "green" => Ok(BookmarkColor::Green),
                    "cyan" => Ok(BookmarkColor::Cyan),
                    "blue" => Ok(BookmarkColor::Blue),
                    "purple" => Ok(BookmarkColor::Purple),
                    "pink" => Ok(BookmarkColor::Pink),
                    hex if hex.starts_with('#') => {
                        let hex_body = &hex[1..];
                        match hex_body.len() {
                            3 => {
                                let r = u8::from_str_radix(&hex_body[0..1].repeat(2), 16).map_err(E::custom)?;
                                let g = u8::from_str_radix(&hex_body[1..2].repeat(2), 16).map_err(E::custom)?;
                                let b = u8::from_str_radix(&hex_body[2..3].repeat(2), 16).map_err(E::custom)?;
                                Ok(BookmarkColor::Custom { r, g, b, a: 255 })
                            }
                            6 => {
                                let r = u8::from_str_radix(&hex_body[0..2], 16).map_err(E::custom)?;
                                let g = u8::from_str_radix(&hex_body[2..4], 16).map_err(E::custom)?;
                                let b = u8::from_str_radix(&hex_body[4..6], 16).map_err(E::custom)?;
                                Ok(BookmarkColor::Custom { r, g, b, a: 255 })
                            }
                            8 => {
                                let r = u8::from_str_radix(&hex_body[0..2], 16).map_err(E::custom)?;
                                let g = u8::from_str_radix(&hex_body[2..4], 16).map_err(E::custom)?;
                                let b = u8::from_str_radix(&hex_body[4..6], 16).map_err(E::custom)?;
                                let a = u8::from_str_radix(&hex_body[6..8], 16).map_err(E::custom)?;
                                Ok(BookmarkColor::Custom { r, g, b, a })
                            }
                            _ => Err(E::custom(format!("Invalid hex color format: {}", hex))),
                        }
                    }
                    _ => Err(E::custom(format!("Unknown bookmark color: {}", lower))),
                }
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                let mut r = None;
                let mut g = None;
                let mut b = None;
                let mut a = Some(255u8);

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "r" => r = Some(map.next_value()?),
                        "g" => g = Some(map.next_value()?),
                        "b" => b = Some(map.next_value()?),
                        "a" => a = Some(map.next_value()?),
                        _ => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }

                let r = r.ok_or_else(|| serde::de::Error::missing_field("r"))?;
                let g = g.ok_or_else(|| serde::de::Error::missing_field("g"))?;
                let b = b.ok_or_else(|| serde::de::Error::missing_field("b"))?;
                let a = a.unwrap_or(255);

                Ok(BookmarkColor::Custom { r, g, b, a })
            }
        }

        deserializer.deserialize_any(ColorVisitor)
    }
}

fn default_bookmark_color() -> BookmarkColor {
    BookmarkColor::Yellow
}

fn deserialize_offset_or_hex<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    struct OffsetVisitor;

    impl<'de> serde::de::Visitor<'de> for OffsetVisitor {
        type Value = usize;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an integer or hex string (e.g. 1024 or '0x400')")
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v as usize)
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if v < 0 { Err(E::custom("offset cannot be negative")) } else { Ok(v as usize) }
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let trimmed = v.trim();
            if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                usize::from_str_radix(hex, 16).map_err(E::custom)
            } else {
                trimmed.parse::<usize>().map_err(E::custom)
            }
        }
    }

    deserializer.deserialize_any(OffsetVisitor)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookmarkItem {
    #[serde(default = "generate_bookmark_id", skip_serializing)]
    pub id: String,
    #[serde(deserialize_with = "deserialize_offset_or_hex")]
    pub offset: usize,
    #[serde(deserialize_with = "deserialize_offset_or_hex")]
    pub size: usize,
    #[serde(default = "default_bookmark_color")]
    pub color: BookmarkColor,
    #[serde(default)]
    pub comment: String,
}

impl BookmarkItem {
    pub fn new(offset: usize, size: usize, color: BookmarkColor, comment: impl Into<String>) -> Self {
        Self {
            id: generate_bookmark_id(),
            offset,
            size,
            color,
            comment: comment.into(),
        }
    }

    pub fn range(&self) -> Range<usize> {
        self.offset..self.offset.saturating_add(self.size)
    }

    pub fn rgba_color(&self) -> RgbaColor {
        self.color.to_rgba()
    }

    #[allow(dead_code)]
    pub fn format_offset(&self) -> String {
        format!("0x{:08X}", self.offset)
    }

    #[allow(dead_code)]
    pub fn format_size(&self) -> String {
        if self.size == 1 {
            "1 byte".to_string()
        } else if self.size < 1024 {
            format!("{} bytes", self.size)
        } else if self.size < 1024 * 1024 {
            format!("{:.1} KB", self.size as f64 / 1024.0)
        } else {
            format!("{:.2} MB", self.size as f64 / (1024.0 * 1024.0))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    pub bookmarks: Vec<BookmarkItem>,
}

fn default_version() -> u32 {
    1
}

impl BookmarkFile {
    pub fn from_yaml(yaml: &str) -> anyhow::Result<Vec<BookmarkItem>> {
        let mut items = if let Ok(file) = serde_yaml::from_str::<BookmarkFile>(yaml) {
            file.bookmarks
        } else if let Ok(items) = serde_yaml::from_str::<Vec<BookmarkItem>>(yaml) {
            items
        } else {
            let err = serde_yaml::from_str::<BookmarkFile>(yaml).unwrap_err();
            anyhow::bail!("Failed to parse bookmarks YAML: {}", err)
        };

        // Guarantee distinct, fresh unique runtime IDs for all loaded items
        for item in &mut items {
            item.id = generate_bookmark_id();
        }
        Ok(items)
    }

    pub fn to_yaml(bookmarks: &[BookmarkItem], file_path: Option<&Path>) -> anyhow::Result<String> {
        let file = BookmarkFile {
            version: 1,
            file_path: file_path.map(|p| p.to_string_lossy().to_string()),
            bookmarks: bookmarks.to_vec(),
        };
        Ok(serde_yaml::to_string(&file)?)
    }
}

/// Summary details for a folded region created by hidden bookmarks or gaps.
#[derive(Debug, Clone, PartialEq)]
pub struct FoldedBookmarkSummary {
    pub start_offset: usize,
    pub end_offset: usize,
    pub size: usize,
    pub color: BookmarkColor,
    pub comment: String,
    pub bookmark_ids: Vec<String>,
    pub is_unbookmarked: bool,
}

/// Encapsulates bookmark entries, visibility filters, interval calculations, and serialization.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookmarkStore {
    pub items: Vec<BookmarkItem>,
    pub hidden_colors: HashSet<BookmarkColor>,
    pub hidden_ids: HashSet<String>,
    pub hide_unbookmarked: bool,
}

impl BookmarkStore {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[allow(dead_code)]
    pub fn items(&self) -> &[BookmarkItem] {
        &self.items
    }

    #[allow(dead_code)]
    pub fn items_mut(&mut self) -> &mut Vec<BookmarkItem> {
        &mut self.items
    }

    pub fn snapshot(&self) -> Vec<BookmarkItem> {
        self.items.clone()
    }

    pub fn by_id(&self, id: &str) -> Option<&BookmarkItem> {
        self.items.iter().find(|b| b.id == id)
    }

    pub fn by_id_cloned(&self, id: &str) -> Option<BookmarkItem> {
        self.by_id(id).cloned()
    }

    pub fn add(&mut self, item: BookmarkItem, total_size: usize) -> String {
        if item.size == 0 {
            return String::new();
        }
        let clamped_offset = item.offset.min(total_size);
        let clamped_size = item.size.min(total_size.saturating_sub(clamped_offset));
        if clamped_size == 0 {
            return String::new();
        }

        let mut item = item;
        item.offset = clamped_offset;
        item.size = clamped_size;

        if let Some(existing) = self.items.iter_mut().find(|h| h.offset == item.offset && h.size == item.size) {
            existing.color = item.color;
            if !item.comment.is_empty() {
                existing.comment = item.comment;
            }
            return existing.id.clone();
        }

        if item.id.is_empty() || self.items.iter().any(|h| h.id == item.id) {
            item.id = generate_bookmark_id();
        }

        let id = item.id.clone();
        self.items.push(item);
        self.items.sort_by_key(|h| (h.offset, h.size));
        id
    }

    pub fn add_custom(&mut self, range: Range<usize>, color: RgbaColor, total_size: usize) {
        if range.is_empty() {
            return;
        }
        let clamped_start = range.start.min(total_size);
        let clamped_end = range.end.min(total_size);
        if clamped_start >= clamped_end {
            return;
        }
        let new_range = clamped_start..clamped_end;
        let hl_color = BookmarkColor::from_rgba(color);

        let mut updated = Vec::with_capacity(self.items.len() + 2);
        for h in self.items.drain(..) {
            let h_range = h.range();
            if h_range.end <= new_range.start || h_range.start >= new_range.end {
                updated.push(h);
            } else {
                if h_range.start < new_range.start {
                    let mut left = h.clone();
                    left.id = generate_bookmark_id();
                    left.size = new_range.start - h_range.start;
                    updated.push(left);
                }
                if h_range.end > new_range.end {
                    let mut right = h.clone();
                    right.id = generate_bookmark_id();
                    right.offset = new_range.end;
                    right.size = h_range.end - new_range.end;
                    updated.push(right);
                }
            }
        }
        updated.push(BookmarkItem::new(new_range.start, new_range.len(), hl_color, ""));
        updated.sort_by_key(|h| (h.offset, h.size));
        self.items = updated;
    }

    pub fn update_comment(&mut self, id: &str, comment: impl Into<String>) -> bool {
        if let Some(item) = self.items.iter_mut().find(|h| h.id == id) {
            item.comment = comment.into();
            true
        } else {
            false
        }
    }

    pub fn update_color(&mut self, id: &str, color: BookmarkColor) -> bool {
        if let Some(item) = self.items.iter_mut().find(|h| h.id == id) {
            item.color = color;
            true
        } else {
            false
        }
    }

    pub fn update_range(&mut self, id: &str, offset: usize, size: usize, total_size: usize) -> bool {
        if size == 0 {
            return false;
        }
        let clamped_offset = offset.min(total_size);
        let clamped_size = size.min(total_size.saturating_sub(clamped_offset));
        if clamped_size == 0 {
            return false;
        }

        if let Some(item) = self.items.iter_mut().find(|h| h.id == id) {
            item.offset = clamped_offset;
            item.size = clamped_size;
            self.items.sort_by_key(|h| (h.offset, h.size));
            true
        } else {
            false
        }
    }

    pub fn remove_by_id(&mut self, id: &str) -> bool {
        let initial_len = self.items.len();
        self.items.retain(|h| h.id != id);
        self.items.len() < initial_len
    }

    pub fn remove_by_index(&mut self, index: usize) -> Option<BookmarkItem> {
        if index < self.items.len() { Some(self.items.remove(index)) } else { None }
    }

    pub fn clear_custom(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        let mut updated = Vec::with_capacity(self.items.len() + 2);
        for h in self.items.drain(..) {
            let h_range = h.range();
            if h_range.end <= range.start || h_range.start >= range.end {
                updated.push(h);
            } else {
                if h_range.start < range.start {
                    let mut left = h.clone();
                    left.id = generate_bookmark_id();
                    left.size = range.start - h_range.start;
                    updated.push(left);
                }
                if h_range.end > range.end {
                    let mut right = h.clone();
                    right.id = generate_bookmark_id();
                    right.offset = range.end;
                    right.size = h_range.end - range.end;
                    updated.push(right);
                }
            }
        }
        self.items = updated;
        self.items.sort_by_key(|h| (h.offset, h.size));
    }

    pub fn clear_all(&mut self) {
        self.items.clear();
    }

    pub fn custom_bookmarks_for_rendering(&self) -> Vec<(Range<usize>, RgbaColor)> {
        self.items.iter().map(|h| (h.range(), h.rgba_color())).collect()
    }

    pub fn to_yaml(&self, doc_path: Option<&Path>) -> anyhow::Result<String> {
        BookmarkFile::to_yaml(&self.items, doc_path)
    }

    pub fn import_items(&mut self, items: Vec<BookmarkItem>, total_size: usize) -> usize {
        let count = items.len();
        for item in items {
            self.add(item, total_size);
        }
        count
    }

    pub fn is_color_hidden(&self, color: BookmarkColor) -> bool {
        if self.hidden_colors.contains(&color) {
            return true;
        }
        if self.hidden_ids.is_empty() {
            return false;
        }
        let mut count_total = 0;
        let mut count_hidden = 0;
        for b in &self.items {
            if b.color == color {
                count_total += 1;
                if self.hidden_ids.contains(&b.id) {
                    count_hidden += 1;
                }
            }
        }
        count_total > 0 && count_total == count_hidden
    }

    pub fn is_id_hidden(&self, id: &str) -> bool {
        self.hidden_ids.contains(id)
    }

    pub fn is_item_hidden(&self, item: &BookmarkItem) -> bool {
        self.hidden_colors.contains(&item.color) || self.hidden_ids.contains(&item.id)
    }

    pub fn toggle_color(&mut self, color: BookmarkColor) {
        if self.is_color_hidden(color) {
            self.show_color(color);
        } else {
            self.hide_color(color);
        }
    }

    pub fn show_color(&mut self, color: BookmarkColor) {
        self.hidden_colors.remove(&color);
        for b in &self.items {
            if b.color == color {
                self.hidden_ids.remove(&b.id);
            }
        }
    }

    pub fn hide_color(&mut self, color: BookmarkColor) {
        self.hidden_colors.insert(color);
        for b in &self.items {
            if b.color == color {
                self.hidden_ids.remove(&b.id);
            }
        }
    }

    pub fn show_only_color(&mut self, target_color: BookmarkColor) {
        let all_colors: Vec<BookmarkColor> = self.items.iter().map(|b| b.color).collect();
        self.hidden_colors.clear();
        for c in all_colors {
            if c != target_color {
                self.hidden_colors.insert(c);
            }
        }
        self.hidden_ids.clear();
    }

    pub fn show_all(&mut self) {
        self.hidden_colors.clear();
        self.hidden_ids.clear();
    }

    pub fn hide_all(&mut self) {
        let all_colors: Vec<BookmarkColor> = self.items.iter().map(|b| b.color).collect();
        for c in all_colors {
            self.hidden_colors.insert(c);
        }
        self.hidden_ids.clear();
    }

    pub fn toggle_item_visibility(&mut self, id: &str) {
        let target_item = self.items.iter().find(|b| b.id == id).cloned();

        if let Some(target) = target_item {
            if self.hidden_colors.contains(&target.color) {
                self.hidden_colors.remove(&target.color);
                for other_bm in &self.items {
                    if other_bm.color == target.color && other_bm.id != id {
                        self.hidden_ids.insert(other_bm.id.clone());
                    }
                }
                self.hidden_ids.remove(id);
            } else if self.hidden_ids.contains(id) {
                self.hidden_ids.remove(id);
            } else {
                self.hidden_ids.insert(id.to_string());
            }
        } else if self.hidden_ids.contains(id) {
            self.hidden_ids.remove(id);
        } else {
            self.hidden_ids.insert(id.to_string());
        }
    }

    pub fn unfold_at(&mut self, offset: usize, folded_regions: &BTreeMap<usize, usize>) -> bool {
        let found = folded_regions.iter().find(|&(&start, &end)| offset >= start && offset < end);
        if let Some((&start, &end)) = found {
            let mut colors_to_decompose = HashSet::new();
            let mut ids_to_unhide = Vec::new();

            for it in &self.items {
                if it.offset < end && it.offset.saturating_add(it.size) > start {
                    colors_to_decompose.insert(it.color);
                    ids_to_unhide.push(it.id.clone());
                }
            }

            let mut changed = false;
            for &color in &colors_to_decompose {
                if self.hidden_colors.contains(&color) {
                    self.hidden_colors.remove(&color);
                    for other_bm in &self.items {
                        if other_bm.color == color {
                            let other_start = other_bm.offset;
                            let other_end = other_bm.offset.saturating_add(other_bm.size);
                            if !(other_start < end && other_end > start) {
                                self.hidden_ids.insert(other_bm.id.clone());
                            }
                        }
                    }
                    changed = true;
                }
            }

            for id in ids_to_unhide {
                if self.hidden_ids.remove(&id) {
                    changed = true;
                }
            }

            if self.hide_unbookmarked && colors_to_decompose.is_empty() {
                self.hide_unbookmarked = false;
                changed = true;
            }

            changed
        } else {
            false
        }
    }

    pub fn is_hide_unbookmarked(&self) -> bool {
        self.hide_unbookmarked
    }

    pub fn toggle_hide_unbookmarked(&mut self) {
        self.hide_unbookmarked = !self.hide_unbookmarked;
    }

    pub fn set_hide_unbookmarked(&mut self, hide: bool) {
        self.hide_unbookmarked = hide;
    }

    pub fn computed_folded_regions(&self, total_size: usize) -> BTreeMap<usize, usize> {
        if total_size == 0 {
            return BTreeMap::new();
        }

        let is_hide_unbookmarked = self.hide_unbookmarked;
        let hidden_colors = &self.hidden_colors;
        let hidden_ids = &self.hidden_ids;

        let mut bookmarked_ranges = Vec::new();
        let mut hidden_ranges = Vec::new();

        for item in &self.items {
            if item.size > 0 {
                let start = item.offset.min(total_size);
                let end = item.offset.saturating_add(item.size).min(total_size);
                if start < end {
                    bookmarked_ranges.push((start, end));
                    if hidden_colors.contains(&item.color) || hidden_ids.contains(&item.id) {
                        hidden_ranges.push((start, end));
                    }
                }
            }
        }

        let mut folds = BTreeMap::new();

        // 1. Hidden bookmark ranges become folds
        if !hidden_ranges.is_empty() {
            hidden_ranges.sort_unstable_by_key(|&(s, e)| (s, e));
            let mut cur_start = hidden_ranges[0].0;
            let mut cur_end = hidden_ranges[0].1;
            for &(s, e) in &hidden_ranges[1..] {
                if s < cur_end {
                    cur_end = cur_end.max(e);
                } else {
                    folds.insert(cur_start, cur_end);
                    cur_start = s;
                    cur_end = e;
                }
            }
            folds.insert(cur_start, cur_end);
        }

        // 2. If hide_unbookmarked is enabled, unbookmarked gaps also become folds
        if is_hide_unbookmarked {
            if bookmarked_ranges.is_empty() {
                folds.insert(0, total_size);
            } else {
                bookmarked_ranges.sort_unstable_by_key(|&(s, e)| (s, e));
                let mut merged_bm = Vec::new();
                let mut cur_start = bookmarked_ranges[0].0;
                let mut cur_end = bookmarked_ranges[0].1;
                for &(s, e) in &bookmarked_ranges[1..] {
                    if s <= cur_end {
                        cur_end = cur_end.max(e);
                    } else {
                        merged_bm.push((cur_start, cur_end));
                        cur_start = s;
                        cur_end = e;
                    }
                }
                merged_bm.push((cur_start, cur_end));

                let mut cursor = 0;
                for (bm_s, bm_e) in merged_bm {
                    if bm_s > cursor {
                        folds.insert(cursor, bm_s);
                    }
                    cursor = bm_e;
                }
                if cursor < total_size {
                    folds.insert(cursor, total_size);
                }
            }
        }

        folds
    }

    pub fn fold_bookmark_summary_at(&self, offset: usize, total_size: usize) -> Option<FoldedBookmarkSummary> {
        let folded = self.computed_folded_regions(total_size);
        let fold_end = folded.get(&offset).copied()?;

        let mut matched_items = Vec::new();
        for item in &self.items {
            if (self.hidden_colors.contains(&item.color) || self.hidden_ids.contains(&item.id))
                && item.offset < fold_end
                && item.offset.saturating_add(item.size) > offset
            {
                matched_items.push(item);
            }
        }

        let is_unbookmarked = matched_items.is_empty();
        let primary = matched_items.first().copied();
        let color = primary.map(|it| it.color).unwrap_or_default();
        let comment = primary
            .map(|it| it.comment.clone())
            .unwrap_or_else(|| if is_unbookmarked { "Unbookmarked".to_string() } else { String::new() });
        let bookmark_ids = matched_items.iter().map(|it| it.id.clone()).collect();

        Some(FoldedBookmarkSummary {
            start_offset: offset,
            end_offset: fold_end,
            size: fold_end.saturating_sub(offset),
            color,
            comment,
            bookmark_ids,
            is_unbookmarked,
        })
    }

    pub fn adjust_after_edit(&mut self, start: usize, old_len: usize, new_len: usize, shift: impl Fn(usize) -> usize) {
        let old_end = start.saturating_add(old_len);
        for item in self.items.iter_mut() {
            let item_start = item.offset;
            let item_end = item.offset.saturating_add(item.size);
            if old_len == 0 {
                if item_start >= start {
                    item.offset = item_start.saturating_add(new_len);
                } else if item_end > start {
                    item.size = item.size.saturating_add(new_len);
                }
                continue;
            }

            if item_end <= start {
                continue;
            }
            if item_start >= old_end {
                item.offset = shift(item_start);
                continue;
            }

            let prefix = item_end.min(start).saturating_sub(item_start);
            let suffix = item_end.saturating_sub(old_end.max(item_start));
            item.offset = item_start.min(start);
            item.size = prefix.saturating_add(new_len).saturating_add(suffix);
        }
        self.items.retain(|item| item.size > 0);
        self.items.sort_by_key(|item| (item.offset, item.size));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bookmark_color_presets_and_names() {
        for color in BookmarkColor::ALL_PRESETS {
            let rgba = color.to_rgba();
            assert!(rgba.a > 0);
            let badge = color.to_badge_rgba();
            assert_eq!(badge.a, 255);
            assert!(!color.name().is_empty());
        }
    }

    #[test]
    fn test_bookmark_color_serde() {
        let yaml = serde_yaml::to_string(&BookmarkColor::Red).unwrap();
        assert_eq!(yaml.trim(), "red");
        let color: BookmarkColor = serde_yaml::from_str("blue").unwrap();
        assert_eq!(color, BookmarkColor::Blue);

        let hex_color: BookmarkColor = serde_yaml::from_str("\"#112233\"").unwrap();
        assert_eq!(
            hex_color,
            BookmarkColor::Custom {
                r: 0x11,
                g: 0x22,
                b: 0x33,
                a: 255
            }
        );

        let rgba_color: BookmarkColor = serde_yaml::from_str("r: 10\ng: 20\nb: 30\na: 128\n").unwrap();
        assert_eq!(rgba_color, BookmarkColor::Custom { r: 10, g: 20, b: 30, a: 128 });
    }

    #[test]
    fn test_bookmark_item_serde_with_hex_offsets() {
        let yaml = r#"
- offset: "0x0010"
  size: "0x20"
  color: green
  comment: Header section
- offset: 100
  size: 4
  color: pink
  comment: Magic number
"#;

        let items = BookmarkFile::from_yaml(yaml).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].offset, 16);
        assert_eq!(items[0].size, 32);
        assert_eq!(items[0].color, BookmarkColor::Green);
        assert_eq!(items[0].comment, "Header section");

        assert_eq!(items[1].offset, 100);
        assert_eq!(items[1].size, 4);
        assert_eq!(items[1].color, BookmarkColor::Pink);
        assert_eq!(items[1].comment, "Magic number");
    }

    #[test]
    fn test_bookmark_yaml_roundtrip() {
        let items = vec![
            BookmarkItem::new(0, 16, BookmarkColor::Red, "File header"),
            BookmarkItem::new(64, 4, BookmarkColor::Cyan, "Checksum"),
        ];

        let yaml = BookmarkFile::to_yaml(&items, Some(Path::new("test.bin"))).unwrap();
        let loaded = BookmarkFile::from_yaml(&yaml).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].offset, 0);
        assert_eq!(loaded[0].size, 16);
        assert_eq!(loaded[0].color, BookmarkColor::Red);
        assert_eq!(loaded[0].comment, "File header");
        assert_eq!(loaded[1].offset, 64);
        assert_eq!(loaded[1].size, 4);
        assert_eq!(loaded[1].color, BookmarkColor::Cyan);
        assert_eq!(loaded[1].comment, "Checksum");
    }

    #[test]
    fn test_bookmark_file_yaml_roundtrip() {
        let items = vec![
            BookmarkItem::new(1024, 256, BookmarkColor::Yellow, "Data block"),
            BookmarkItem::new(2048, 128, BookmarkColor::Purple, "Signature"),
        ];

        let yaml = BookmarkFile::to_yaml(&items, Some(Path::new("firmware.bin"))).unwrap();
        let loaded = BookmarkFile::from_yaml(&yaml).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].offset, 1024);
        assert_eq!(loaded[0].size, 256);
        assert_eq!(loaded[0].color, BookmarkColor::Yellow);
        assert_eq!(loaded[0].comment, "Data block");
        assert_eq!(loaded[1].offset, 2048);
        assert_eq!(loaded[1].size, 128);
        assert_eq!(loaded[1].color, BookmarkColor::Purple);
        assert_eq!(loaded[1].comment, "Signature");
    }

    #[test]
    fn test_bookmark_item_formatting() {
        let item = BookmarkItem::new(0x1234, 16, BookmarkColor::Green, "Test");
        assert_eq!(item.format_offset(), "0x00001234");
        assert_eq!(item.format_size(), "16 bytes");

        let single = BookmarkItem::new(0, 1, BookmarkColor::Blue, "");
        assert_eq!(single.format_size(), "1 byte");

        let kb = BookmarkItem::new(0, 2048, BookmarkColor::Orange, "");
        assert_eq!(kb.format_size(), "2.0 KB");
    }
}
