use crate::core::encoding::Encoding;
use crate::core::layout::{BytesPerRow, DEFAULT_BYTES_PER_ROW, MAX_BYTES_PER_ROW, MIN_BYTES_PER_ROW};
use crate::core::radix::{ByteCategoryColors, ByteColoring, ByteGroupSize, ByteOrder, DisplayRadix};
pub use crate::core::structure::{DEFAULT_STRUCTURE_YAML_INCLUDE_OFFSETS, DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD, MAX_STRUCTURE_YAML_SHA_THRESHOLD};
use crate::core::structure::{DefinitionHistory, FileHistory, RecentFileEntry};
use crate::ui::appearance::{Appearance, MAX_FONT_SIZE, MIN_FONT_SIZE};
pub use crate::ui::views::scratchpad_view::ScratchpadMode;
use gpui_kit::App;
use gpui_kit::component::theme::{Theme, ThemeMode};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

pub const DEFAULT_LIGHT_THEME: &str = "Default Light";
pub const DEFAULT_DARK_THEME: &str = "Default Dark";

const APPLICATION_CONFIG_DIRECTORY: &str = "xvw";
const SETTINGS_FILE_NAME: &str = "settings.toml";

static SAVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static SAVE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// User-configurable application settings persisted between launches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: Appearance,
    pub light_theme: String,
    pub dark_theme: String,
    pub theme_mode: ThemeMode,
    pub default_encoding: Encoding,
    pub default_radix: DisplayRadix,
    pub default_group_size: ByteGroupSize,
    #[serde(alias = "default_byte_order")]
    pub default_endianness: ByteOrder,
    #[serde(default = "default_bytes_per_row")]
    pub bytes_per_row: usize,
    #[serde(default)]
    pub byte_coloring: ByteColoring,
    #[serde(default)]
    pub byte_category_colors: ByteCategoryColors,
    #[serde(
        default = "default_structure_yaml_sha_threshold",
        alias = "structure_yaml_sha256_threshold",
        alias = "structure_yaml_binary_sha_threshold"
    )]
    pub structure_yaml_sha_threshold: usize,
    #[serde(default = "default_structure_yaml_include_offsets", alias = "structure_yaml_include_offset")]
    pub structure_yaml_include_offsets: bool,
    pub recent_definition_paths: Vec<PathBuf>,
    pub recent_file_paths: Vec<PathBuf>,
    pub recent_files: Vec<RecentFileEntry>,
    #[serde(default)]
    pub scratchpad: ScratchpadSettings,
}

pub const DEFAULT_SPLIT_RATIO: f32 = 0.5;
pub const MIN_SPLIT_RATIO: f32 = 0.1;
pub const MAX_SPLIT_RATIO: f32 = 0.9;
#[allow(dead_code)]
pub const DEFAULT_SPLIT_FALLBACK_WIDTH: f32 = 200.0;
pub const DEFAULT_SCRATCHPAD_PANEL_WIDTH: f32 = 340.0;
pub const MIN_SCRATCHPAD_PANEL_WIDTH: f32 = 180.0;
pub const MAX_SCRATCHPAD_PANEL_WIDTH: f32 = 1600.0;

fn default_split_ratio() -> f32 {
    DEFAULT_SPLIT_RATIO
}

fn default_panel_width() -> f32 {
    DEFAULT_SCRATCHPAD_PANEL_WIDTH
}

/// Scratchpad layout and display mode settings persisted between launches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScratchpadSettings {
    #[serde(default, alias = "last_mode")]
    pub mode: ScratchpadMode,
    #[serde(default = "default_split_ratio")]
    pub split_ratio: f32,
    #[serde(default)]
    pub split_width: Option<f32>,
    #[serde(default = "default_panel_width", alias = "right_panel_width")]
    pub panel_width: f32,
    #[serde(default, alias = "last_file", alias = "last_opened_file")]
    pub last_open_file: Option<PathBuf>,
}

impl gpui_kit::Global for ScratchpadSettings {}

impl Default for ScratchpadSettings {
    fn default() -> Self {
        Self {
            mode: ScratchpadMode::Edit,
            split_ratio: DEFAULT_SPLIT_RATIO,
            split_width: None,
            panel_width: DEFAULT_SCRATCHPAD_PANEL_WIDTH,
            last_open_file: None,
        }
    }
}

impl ScratchpadSettings {
    /// Sets the global display mode.
    pub fn set_mode(&mut self, mode: ScratchpadMode) {
        self.mode = mode;
    }

    /// Sets the global split ratio and optional cached width.
    pub fn set_split_ratio(&mut self, ratio: f32, width: Option<f32>) {
        self.split_ratio = ratio.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO);
        self.split_width = width;
    }

    /// Sets the scratchpad right panel width in workspace.
    pub fn set_panel_width(&mut self, width: f32) {
        self.panel_width = width.clamp(MIN_SCRATCHPAD_PANEL_WIDTH, MAX_SCRATCHPAD_PANEL_WIDTH);
    }

    /// Sets the last opened scratchpad file path.
    pub fn set_last_open_file(&mut self, path: Option<PathBuf>) {
        self.last_open_file = path;
    }
}

fn default_bytes_per_row() -> usize {
    DEFAULT_BYTES_PER_ROW
}

fn default_structure_yaml_sha_threshold() -> usize {
    DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD
}

fn default_structure_yaml_include_offsets() -> bool {
    DEFAULT_STRUCTURE_YAML_INCLUDE_OFFSETS
}

/// Application-wide recent-path histories shared by all workspace windows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentHistoryState {
    pub definitions: DefinitionHistory,
    pub files: FileHistory,
}

impl gpui_kit::Global for RecentHistoryState {}

impl RecentHistoryState {
    /// Creates the in-memory histories from persisted settings.
    pub fn from_settings(settings: &Settings) -> Self {
        let files = if !settings.recent_files.is_empty() {
            FileHistory::from_entries(settings.recent_files.clone())
        } else {
            FileHistory::from_paths(settings.recent_file_paths.clone())
        };
        Self {
            definitions: DefinitionHistory::from_paths(settings.recent_definition_paths.clone()),
            files,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            light_theme: DEFAULT_LIGHT_THEME.to_string(),
            dark_theme: DEFAULT_DARK_THEME.to_string(),
            theme_mode: ThemeMode::Light,
            default_encoding: Encoding::default(),
            default_radix: DisplayRadix::default(),
            default_group_size: ByteGroupSize::default(),
            default_endianness: ByteOrder::default(),
            bytes_per_row: DEFAULT_BYTES_PER_ROW,
            byte_coloring: ByteColoring::default(),
            byte_category_colors: ByteCategoryColors::default(),
            structure_yaml_sha_threshold: DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD,
            structure_yaml_include_offsets: true,
            recent_definition_paths: Vec::new(),
            recent_file_paths: Vec::new(),
            recent_files: Vec::new(),
            scratchpad: ScratchpadSettings::default(),
        }
    }
}

impl Settings {
    /// Loads settings from the platform configuration directory.
    ///
    /// Missing or invalid files are ignored and replaced with the defaults so
    /// a damaged preferences file cannot prevent the application from starting.
    pub fn load() -> Self {
        let Some(path) = settings_path() else {
            return Self::default();
        };

        match Self::load_from(&path) {
            Ok(settings) => settings,
            Err(SettingsError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => {
                eprintln!("Failed to load settings from {}: {error}", path.display());
                Self::default()
            }
        }
    }

    /// Loads and validates settings from an explicit file path.
    pub fn load_from(path: &Path) -> Result<Self, SettingsError> {
        let contents = fs::read_to_string(path)?;
        let settings: Self = toml::from_str(&contents)?;
        Ok(settings.sanitized())
    }

    /// Creates a settings snapshot from the application's current globals.
    pub fn from_app(cx: &App) -> Self {
        let recent_history = cx.global::<RecentHistoryState>();
        let theme = Theme::global(cx);
        Self {
            appearance: cx.global::<Appearance>().clone(),
            light_theme: theme.light_theme.name.to_string(),
            dark_theme: theme.dark_theme.name.to_string(),
            theme_mode: theme.mode,
            default_encoding: *cx.global::<Encoding>(),
            default_radix: *cx.global::<DisplayRadix>(),
            default_group_size: *cx.global::<ByteGroupSize>(),
            default_endianness: *cx.global::<ByteOrder>(),
            bytes_per_row: cx.global::<BytesPerRow>().0,
            byte_coloring: *cx.global::<ByteColoring>(),
            byte_category_colors: *cx.global::<ByteCategoryColors>(),
            structure_yaml_sha_threshold: cx.global::<crate::core::structure::StructureYamlShaThreshold>().0,
            structure_yaml_include_offsets: cx.global::<crate::core::structure::StructureYamlIncludeOffsets>().0,
            recent_definition_paths: recent_history.definitions.paths().to_vec(),
            recent_file_paths: recent_history.files.paths(),
            recent_files: recent_history.files.entries().to_vec(),
            scratchpad: cx.try_global::<ScratchpadSettings>().cloned().unwrap_or_default(),
        }
    }

    /// Saves settings to an explicit file path.
    pub fn save_to(&self, path: &Path) -> Result<(), SettingsError> {
        let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty());
        if let Some(parent) = parent {
            fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(&self.clone().sanitized())?;
        fs::write(path, contents)?;
        Ok(())
    }

    /// Schedules a background save of these settings.
    ///
    /// Saving is serialized and superseded snapshots are skipped, so typing in
    /// a settings input cannot leave an older value on disk after a newer save.
    pub fn save_async(&self, cx: &App) {
        let Some(path) = settings_path() else {
            return;
        };

        let generation = SAVE_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
        let settings = self.clone();
        cx.background_executor()
            .spawn(async move {
                let lock = SAVE_LOCK.get_or_init(|| Mutex::new(()));
                let _guard = lock.lock().expect("settings save lock");
                if SAVE_GENERATION.load(Ordering::Acquire) != generation {
                    return;
                }

                if let Err(error) = settings.save_to(&path) {
                    eprintln!("Failed to save settings to {}: {error}", path.display());
                }
            })
            .detach();
    }

    fn sanitized(mut self) -> Self {
        let defaults = Appearance::default();
        if self.appearance.font_family.trim().is_empty() {
            self.appearance.font_family = defaults.font_family;
        }
        if !self.appearance.font_size.is_finite() || !(MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&self.appearance.font_size) {
            self.appearance.font_size = defaults.font_size;
        }
        if self.light_theme.trim().is_empty() {
            self.light_theme = DEFAULT_LIGHT_THEME.to_string();
        }
        if self.dark_theme.trim().is_empty() {
            self.dark_theme = DEFAULT_DARK_THEME.to_string();
        }
        if !(MIN_BYTES_PER_ROW..=MAX_BYTES_PER_ROW).contains(&self.bytes_per_row) {
            self.bytes_per_row = DEFAULT_BYTES_PER_ROW;
        }
        if self.byte_coloring == ByteColoring::Custom || self.byte_category_colors != ByteCategoryColors::default() {
            self.byte_coloring = ByteColoring::from_colors(self.byte_category_colors);
        } else if let Some(preset_colors) = self.byte_coloring.colors() {
            self.byte_category_colors = preset_colors;
        }
        if self.structure_yaml_sha_threshold > MAX_STRUCTURE_YAML_SHA_THRESHOLD {
            self.structure_yaml_sha_threshold = DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD;
        }

        self.recent_definition_paths = DefinitionHistory::from_paths(self.recent_definition_paths).paths().to_vec();
        let history = if !self.recent_files.is_empty() {
            FileHistory::from_entries(self.recent_files)
        } else {
            FileHistory::from_paths(self.recent_file_paths)
        };
        self.recent_files = history.entries().to_vec();
        self.recent_file_paths = history.paths();
        if !self.scratchpad.split_ratio.is_finite() || !(MIN_SPLIT_RATIO..=MAX_SPLIT_RATIO).contains(&self.scratchpad.split_ratio) {
            self.scratchpad.split_ratio = DEFAULT_SPLIT_RATIO;
        }
        if let Some(w) = self.scratchpad.split_width
            && (!w.is_finite() || w < 20.0 || w > 10000.0)
        {
            self.scratchpad.split_width = None;
        }
        if !self.scratchpad.panel_width.is_finite() || !(MIN_SCRATCHPAD_PANEL_WIDTH..=MAX_SCRATCHPAD_PANEL_WIDTH).contains(&self.scratchpad.panel_width) {
            self.scratchpad.panel_width = DEFAULT_SCRATCHPAD_PANEL_WIDTH;
        }
        self
    }
}

/// Returns the path used to persist user settings on this platform.
pub fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|directory| directory.join(APPLICATION_CONFIG_DIRECTORY).join(SETTINGS_FILE_NAME))
}

/// Saves the current application settings in the background.
pub fn save_current(cx: &App) {
    Settings::from_app(cx).save_async(cx);
}

/// Registers a final synchronous save so the latest edit is retained when the
/// application quits before a background save has completed.
pub fn register_quit_handler(cx: &App) {
    cx.on_app_quit(|cx| {
        let settings = Settings::from_app(cx);
        async move {
            let Some(path) = settings_path() else {
                return;
            };

            let lock = SAVE_LOCK.get_or_init(|| Mutex::new(()));
            let _guard = lock.lock().expect("settings save lock");
            if let Err(error) = settings.save_to(&path) {
                eprintln!("Failed to save settings to {}: {error}", path.display());
            }
        }
    })
    .detach();
}

/// Errors returned while reading or writing the settings file.
#[derive(Debug)]
pub enum SettingsError {
    Io(std::io::Error),
    Deserialize(toml::de::Error),
    Serialize(toml::ser::Error),
}

impl Display for SettingsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Deserialize(error) => write!(formatter, "invalid TOML: {error}"),
            Self::Serialize(error) => write!(formatter, "serialization error: {error}"),
        }
    }
}

impl Error for SettingsError {}

impl From<std::io::Error> for SettingsError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<toml::de::Error> for SettingsError {
    fn from(error: toml::de::Error) -> Self {
        Self::Deserialize(error)
    }
}

impl From<toml::ser::Error> for SettingsError {
    fn from(error: toml::ser::Error) -> Self {
        Self::Serialize(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestSettingsFile {
        path: PathBuf,
    }

    impl TestSettingsFile {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock must be after the Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("xvw-settings-{label}-{}-{nonce}.toml", std::process::id()));
            Self { path }
        }
    }

    impl Drop for TestSettingsFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    #[test]
    fn settings_round_trip() {
        use crate::core::radix::CategoryColor;

        let file = TestSettingsFile::new("round-trip");
        let settings = Settings {
            appearance: Appearance {
                font_family: "Fira Code".into(),
                font_size: 18.0,
            },
            light_theme: "Solarized Light".into(),
            dark_theme: "Tokyo Night".into(),
            theme_mode: ThemeMode::Dark,
            default_encoding: Encoding::Utf16Le,
            default_radix: DisplayRadix::Binary,
            default_group_size: ByteGroupSize::Four,
            default_endianness: ByteOrder::BigEndian,
            bytes_per_row: 24,
            byte_coloring: ByteColoring::Custom,
            byte_category_colors: ByteCategoryColors {
                null: CategoryColor::Muted,
                ascii_printable: CategoryColor::Default,
                ascii_whitespace: CategoryColor::Green,
                ascii_control: CategoryColor::Red,
                non_ascii: CategoryColor::Yellow,
                max: CategoryColor::Magenta,
                mixed: CategoryColor::Cyan,
            },
            structure_yaml_sha_threshold: 64,
            structure_yaml_include_offsets: false,
            recent_definition_paths: vec![PathBuf::from("definition.ksy")],
            recent_file_paths: vec![PathBuf::from("binary.bin")],
            recent_files: vec![RecentFileEntry::new(PathBuf::from("binary.bin"), None)],
            scratchpad: ScratchpadSettings {
                mode: ScratchpadMode::Split,
                split_ratio: 0.6,
                split_width: Some(240.0),
                panel_width: 380.0,
                last_open_file: Some(PathBuf::from("/tmp/saved_scratch.md")),
            },
        };

        settings.save_to(&file.path).expect("save settings");

        assert_eq!(Settings::load_from(&file.path).expect("load settings"), settings);
    }

    #[test]
    fn legacy_recent_file_paths_migrated() {
        let file = TestSettingsFile::new("legacy");
        fs::write(&file.path, "recent_file_paths = [\"legacy1.bin\", \"legacy2.hex\"]\n").expect("write settings");

        let settings = Settings::load_from(&file.path).expect("load settings");
        assert_eq!(settings.recent_files.len(), 2);
        assert_eq!(settings.recent_files[0].path, PathBuf::from("legacy1.bin"));
        assert_eq!(settings.recent_files[0].format, None);
        assert_eq!(settings.recent_files[1].path, PathBuf::from("legacy2.hex"));
        assert_eq!(settings.recent_files[1].format, None);
    }

    #[test]
    fn recent_files_with_format_round_trip() {
        let file = TestSettingsFile::new("format-round-trip");
        let settings = Settings {
            recent_files: vec![
                RecentFileEntry::new(PathBuf::from("firmware.hex"), Some(crate::core::format::FileFormat::IntelHex)),
                RecentFileEntry::new(PathBuf::from("firmware.srec"), Some(crate::core::format::FileFormat::MotorolaSrec)),
                RecentFileEntry::new(PathBuf::from("data.bin"), Some(crate::core::format::FileFormat::Binary)),
            ],
            ..Settings::default()
        }
        .sanitized();

        settings.save_to(&file.path).expect("save settings");
        let loaded = Settings::load_from(&file.path).expect("load settings");
        assert_eq!(loaded.recent_files, settings.recent_files);
        assert_eq!(loaded.recent_file_paths, settings.recent_file_paths);
    }

    #[test]
    fn missing_values_use_defaults() {
        let file = TestSettingsFile::new("defaults");
        fs::write(&file.path, "[appearance]\nfont_family = \"Fira Code\"\n").expect("write settings");

        let settings = Settings::load_from(&file.path).expect("load settings");

        assert_eq!(settings.appearance.font_family, "Fira Code");
        assert_eq!(settings.appearance.font_size, Appearance::default().font_size);
        assert_eq!(DEFAULT_LIGHT_THEME, "Default Light");
        assert_eq!(DEFAULT_DARK_THEME, "Default Dark");
        assert_eq!(settings.light_theme, DEFAULT_LIGHT_THEME);
        assert_eq!(settings.dark_theme, DEFAULT_DARK_THEME);
        assert_eq!(settings.theme_mode, ThemeMode::Light);
        assert_eq!(settings.default_encoding, Encoding::default());
        assert_eq!(settings.default_radix, DisplayRadix::default());
        assert_eq!(settings.default_group_size, ByteGroupSize::default());
        assert_eq!(settings.default_endianness, ByteOrder::default());
        assert_eq!(settings.bytes_per_row, DEFAULT_BYTES_PER_ROW);
        assert_eq!(settings.structure_yaml_sha_threshold, DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD);
        assert!(settings.structure_yaml_include_offsets);
    }

    #[test]
    fn invalid_appearance_values_are_replaced_with_defaults() {
        let file = TestSettingsFile::new("sanitize");
        fs::write(&file.path, "[appearance]\nfont_family = \"\"\nfont_size = 0.0\n").expect("write settings");

        let settings = Settings::load_from(&file.path).expect("load settings");

        assert_eq!(settings.appearance, Appearance::default());
        assert_eq!(settings.light_theme, DEFAULT_LIGHT_THEME);
        assert_eq!(settings.dark_theme, DEFAULT_DARK_THEME);
        assert_eq!(settings.bytes_per_row, DEFAULT_BYTES_PER_ROW);
    }

    #[test]
    fn invalid_bytes_per_row_is_replaced_with_defaults() {
        let file = TestSettingsFile::new("sanitize-bytes-zero");
        fs::write(&file.path, "bytes_per_row = 0\n").expect("write settings");

        let settings = Settings::load_from(&file.path).expect("load settings");
        assert_eq!(settings.bytes_per_row, DEFAULT_BYTES_PER_ROW);

        let file_high = TestSettingsFile::new("sanitize-bytes-high");
        fs::write(&file_high.path, "bytes_per_row = 100\n").expect("write settings");

        let settings_high = Settings::load_from(&file_high.path).expect("load settings");
        assert_eq!(settings_high.bytes_per_row, DEFAULT_BYTES_PER_ROW);
    }

    #[test]
    fn default_byte_order_alias_supported() {
        let file = TestSettingsFile::new("byte-order-alias");
        fs::write(&file.path, "default_byte_order = \"BigEndian\"\n").expect("write settings");

        let settings = Settings::load_from(&file.path).expect("load settings");
        assert_eq!(settings.default_endianness, ByteOrder::BigEndian);
    }

    #[test]
    fn structure_yaml_sha_threshold_alias_supported() {
        let file = TestSettingsFile::new("yaml-sha-alias");
        fs::write(&file.path, "structure_yaml_sha256_threshold = 48\n").expect("write settings");

        let settings = Settings::load_from(&file.path).expect("load settings");
        assert_eq!(settings.structure_yaml_sha_threshold, 48);
    }

    #[test]
    fn invalid_structure_yaml_sha_threshold_is_replaced_with_defaults() {
        let file = TestSettingsFile::new("sanitize-yaml-sha-high");
        fs::write(&file.path, "structure_yaml_sha_threshold = 20000000\n").expect("write settings");

        let settings = Settings::load_from(&file.path).expect("load settings");
        assert_eq!(settings.structure_yaml_sha_threshold, DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD);
    }

    #[test]
    fn structure_yaml_include_offsets_setting_supported() {
        let file = TestSettingsFile::new("yaml-include-offsets");
        fs::write(&file.path, "structure_yaml_include_offsets = false\n").expect("write settings");

        let settings = Settings::load_from(&file.path).expect("load settings");
        assert!(!settings.structure_yaml_include_offsets);
    }

    #[test]
    fn scratchpad_settings_toml_round_trip() {
        let file = TestSettingsFile::new("scratchpad-round-trip");
        let toml_str = r#"
[scratchpad]
mode = "split"
split_ratio = 0.65
split_width = 220.0
panel_width = 420.0
last_open_file = "/tmp/test_scratch.md"
"#;
        fs::write(&file.path, toml_str).expect("write settings");

        let settings = Settings::load_from(&file.path).expect("load settings");
        assert_eq!(settings.scratchpad.mode, ScratchpadMode::Split);
        assert!((settings.scratchpad.split_ratio - 0.65).abs() < 1e-5);
        assert_eq!(settings.scratchpad.split_width, Some(220.0));
        assert_eq!(settings.scratchpad.panel_width, 420.0);
        assert_eq!(settings.scratchpad.last_open_file, Some(PathBuf::from("/tmp/test_scratch.md")));
    }

    #[test]
    fn scratchpad_settings_legacy_alias_and_mutation() {
        let file = TestSettingsFile::new("scratchpad-alias");
        let toml_str = r#"
[scratchpad]
last_mode = "preview"
right_panel_width = 380.0
last_opened_file = "/tmp/old_scratch.md"
"#;
        fs::write(&file.path, toml_str).expect("write settings");
        let settings = Settings::load_from(&file.path).expect("load settings");
        assert_eq!(settings.scratchpad.mode, ScratchpadMode::Preview);
        assert_eq!(settings.scratchpad.panel_width, 380.0);
        assert_eq!(settings.scratchpad.last_open_file, Some(PathBuf::from("/tmp/old_scratch.md")));

        let mut sp = ScratchpadSettings::default();
        assert_eq!(sp.mode, ScratchpadMode::Edit);
        assert_eq!(sp.split_ratio, DEFAULT_SPLIT_RATIO);
        assert_eq!(sp.split_width, None);
        assert_eq!(sp.panel_width, DEFAULT_SCRATCHPAD_PANEL_WIDTH);
        assert_eq!(sp.last_open_file, None);

        sp.set_mode(ScratchpadMode::Split);
        assert_eq!(sp.mode, ScratchpadMode::Split);

        sp.set_split_ratio(0.72, Some(250.0));
        assert!((sp.split_ratio - 0.72).abs() < 1e-5);
        assert_eq!(sp.split_width, Some(250.0));

        sp.set_panel_width(500.0);
        assert_eq!(sp.panel_width, 500.0);

        sp.set_last_open_file(Some(PathBuf::from("/tmp/another.md")));
        assert_eq!(sp.last_open_file, Some(PathBuf::from("/tmp/another.md")));
    }

    #[test]
    fn scratchpad_invalid_values_sanitized() {
        let file = TestSettingsFile::new("scratchpad-sanitize");
        let toml_str = r#"
[scratchpad]
split_ratio = 999.0
split_width = -100.0
panel_width = 99999.0
"#;
        fs::write(&file.path, toml_str).expect("write settings");

        let settings = Settings::load_from(&file.path).expect("load settings");
        assert_eq!(settings.scratchpad.split_ratio, DEFAULT_SPLIT_RATIO);
        assert_eq!(settings.scratchpad.split_width, None);
        assert_eq!(settings.scratchpad.panel_width, DEFAULT_SCRATCHPAD_PANEL_WIDTH);
    }

    #[test]
    fn byte_coloring_setting_round_trip() {
        let file = TestSettingsFile::new("byte-coloring");
        fs::write(&file.path, "byte_coloring = \"uniform\"\n").expect("write settings");

        let settings = Settings::load_from(&file.path).expect("load settings");
        assert_eq!(settings.byte_coloring, ByteColoring::Uniform);
        assert_eq!(settings.byte_category_colors, ByteColoring::Uniform.colors().unwrap());

        let file_padding = TestSettingsFile::new("byte-coloring-padding");
        fs::write(&file_padding.path, "byte_coloring = \"dim_padding\"\n").expect("write settings");

        let settings_padding = Settings::load_from(&file_padding.path).expect("load settings");
        assert_eq!(settings_padding.byte_coloring, ByteColoring::DimPadding);
        assert_eq!(settings_padding.byte_category_colors, ByteColoring::DimPadding.colors().unwrap());

        let file_category = TestSettingsFile::new("byte-coloring-category");
        fs::write(&file_category.path, "byte_coloring = \"category\"\n").expect("write settings");

        let settings_category = Settings::load_from(&file_category.path).expect("load settings");
        assert_eq!(settings_category.byte_coloring, ByteColoring::Category);
        assert_eq!(settings_category.byte_category_colors, ByteColoring::Category.colors().unwrap());

        let file_dim_zero = TestSettingsFile::new("byte-coloring-dim-zero");
        fs::write(&file_dim_zero.path, "byte_coloring = \"dim_zero\"\n").expect("write settings");

        let settings_dim_zero = Settings::load_from(&file_dim_zero.path).expect("load settings");
        assert_eq!(settings_dim_zero.byte_coloring, ByteColoring::DimZero);
        assert_eq!(settings_dim_zero.byte_category_colors, ByteColoring::DimZero.colors().unwrap());

        let file_default = TestSettingsFile::new("byte-coloring-default");
        fs::write(&file_default.path, "").expect("write settings");

        let settings_default = Settings::load_from(&file_default.path).expect("load settings");
        assert_eq!(settings_default.byte_coloring, ByteColoring::Uniform);
        assert_eq!(settings_default.byte_category_colors, ByteColoring::Uniform.colors().unwrap());
    }

    #[test]
    fn byte_category_colors_setting_round_trip() {
        use crate::core::radix::CategoryColor;

        let file = TestSettingsFile::new("category-colors");
        let toml_str = r#"
[byte_category_colors]
null = "muted"
ascii_printable = "default"
ascii_whitespace = "green"
ascii_control = "red"
non_ascii = "yellow"
max = "magenta"
mixed = "cyan"
"#;
        fs::write(&file.path, toml_str).expect("write settings");

        let settings = Settings::load_from(&file.path).expect("load settings");
        assert_eq!(settings.byte_coloring, ByteColoring::Custom);
        assert_eq!(settings.byte_category_colors.null, CategoryColor::Muted);
        assert_eq!(settings.byte_category_colors.ascii_printable, CategoryColor::Default);
        assert_eq!(settings.byte_category_colors.ascii_whitespace, CategoryColor::Green);
        assert_eq!(settings.byte_category_colors.ascii_control, CategoryColor::Red);
        assert_eq!(settings.byte_category_colors.non_ascii, CategoryColor::Yellow);
        assert_eq!(settings.byte_category_colors.max, CategoryColor::Magenta);
        assert_eq!(settings.byte_category_colors.mixed, CategoryColor::Cyan);

        let file_default = TestSettingsFile::new("category-colors-default");
        fs::write(&file_default.path, "").expect("write settings");

        let settings_default = Settings::load_from(&file_default.path).expect("load settings");
        assert_eq!(settings_default.byte_coloring, ByteColoring::Uniform);
        assert_eq!(settings_default.byte_category_colors, ByteCategoryColors::default());
    }
}
