use crate::assets::Assets;
use crate::settings::{DEFAULT_DARK_THEME, DEFAULT_LIGHT_THEME, Settings};
use gpui_kit::component::theme::{Theme, ThemeConfig, ThemeMode, ThemeSet};
use gpui_kit::{App, AssetSource, SharedString, Window};
use std::collections::HashMap;
use std::rc::Rc;

/// Global registry of embedded themes bundled with the application.
#[derive(Clone, Debug, Default)]
pub struct EmbeddedThemes {
    themes: HashMap<SharedString, Rc<ThemeConfig>>,
}

impl gpui_kit::Global for EmbeddedThemes {}

impl EmbeddedThemes {
    /// Loads all theme definition files found under `themes/` in the given asset source.
    pub fn load_from_assets(assets: &impl AssetSource) -> Self {
        let mut themes = HashMap::new();
        if let Ok(paths) = assets.list("themes/") {
            for path in paths {
                if path.ends_with(".json")
                    && let Ok(Some(bytes)) = assets.load(&path)
                {
                    match serde_json::from_slice::<ThemeSet>(&bytes) {
                        Ok(theme_set) => {
                            for theme in theme_set.themes {
                                themes.insert(theme.name.clone(), Rc::new(theme));
                            }
                        }
                        Err(err) => {
                            eprintln!("Failed to parse embedded theme file '{path}': {err}");
                        }
                    }
                }
            }
        }
        Self { themes }
    }

    /// Retrieves a theme configuration by name.
    pub fn get(&self, name: &str) -> Option<Rc<ThemeConfig>> {
        self.themes.get(name).cloned()
    }

    /// Returns a sorted list of all available theme names.
    pub fn theme_names(&self) -> Vec<SharedString> {
        let mut names: Vec<_> = self.themes.keys().cloned().collect();
        names.sort();
        names
    }
}

/// Initializes embedded themes and registers the global `EmbeddedThemes` state.
pub fn init(cx: &mut App) {
    let embedded_themes = EmbeddedThemes::load_from_assets(&Assets);
    let default_light = embedded_themes.get(DEFAULT_LIGHT_THEME);
    let default_dark = embedded_themes.get(DEFAULT_DARK_THEME);
    cx.set_global(embedded_themes);

    let theme = Theme::global_mut(cx);
    if let Some(light_theme) = default_light {
        theme.light_theme = light_theme;
    }
    if let Some(dark_theme) = default_dark {
        theme.dark_theme = dark_theme;
    }
}

/// Applies the theme pair and mode configured in application settings.
pub fn apply_settings(settings: &Settings, window: Option<&mut Window>, cx: &mut App) {
    set_theme_pair(&settings.light_theme, &settings.dark_theme, settings.theme_mode, window, cx);
}

/// Sets the active light/dark theme pair and applies the active mode.
pub fn set_theme_pair(light_name: &str, dark_name: &str, mode: ThemeMode, window: Option<&mut Window>, cx: &mut App) {
    let embedded_themes = cx.global::<EmbeddedThemes>();
    let light_theme = embedded_themes.get(light_name).or_else(|| embedded_themes.get(DEFAULT_LIGHT_THEME));
    let dark_theme = embedded_themes.get(dark_name).or_else(|| embedded_themes.get(DEFAULT_DARK_THEME));

    {
        let theme = Theme::global_mut(cx);
        if let Some(light_theme) = light_theme {
            theme.light_theme = light_theme;
        }
        if let Some(dark_theme) = dark_theme {
            theme.dark_theme = dark_theme;
        }
    }

    Theme::change(mode, window, cx);
    cx.refresh_windows();
}

/// Applies an individual theme by name, activating the corresponding light or dark slot and mode.
pub fn apply_theme_by_name(name: &str, window: Option<&mut Window>, cx: &mut App) {
    let embedded_themes = cx.global::<EmbeddedThemes>();
    let Some(theme_config) = embedded_themes.get(name) else {
        return;
    };

    let mode = theme_config.mode;
    {
        let theme = Theme::global_mut(cx);
        if mode.is_dark() {
            theme.dark_theme = theme_config;
        } else {
            theme.light_theme = theme_config;
        }
    }

    Theme::change(mode, window, cx);
    cx.refresh_windows();
}

/// Returns a sorted list of all available theme names.
pub fn all_theme_names(cx: &App) -> Vec<SharedString> {
    cx.global::<EmbeddedThemes>().theme_names()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Assets;

    #[test]
    fn test_embedded_themes_load_all_presets() {
        let embedded = EmbeddedThemes::load_from_assets(&Assets);

        // Verify Ayu themes
        assert!(embedded.get("Ayu Light").is_some());
        assert!(embedded.get("Ayu Dark").is_some());

        // Verify Catppuccin themes
        assert!(embedded.get("Catppuccin Latte").is_some());
        assert!(embedded.get("Catppuccin Frappe").is_some());
        assert!(embedded.get("Catppuccin Macchiato").is_some());
        assert!(embedded.get("Catppuccin Mocha").is_some());

        // Verify Tokyo Night themes
        assert!(embedded.get("Tokyo Night").is_some());
        assert!(embedded.get("Tokyo Storm").is_some());
        assert!(embedded.get("Tokyo Moon").is_some());

        // Verify Gruvbox themes
        assert!(embedded.get("Gruvbox Light").is_some());
        assert!(embedded.get("Gruvbox Dark").is_some());

        // Verify Solarized themes
        assert!(embedded.get("Solarized Light").is_some());
        assert!(embedded.get("Solarized Dark").is_some());
    }

    #[test]
    fn test_embedded_themes_names_sorted() {
        let embedded = EmbeddedThemes::load_from_assets(&Assets);
        let names = embedded.theme_names();
        assert!(names.contains(&SharedString::from("Ayu Light")));
        assert!(names.contains(&SharedString::from("Catppuccin Mocha")));
        assert!(names.contains(&SharedString::from("Tokyo Night")));
        assert!(names.contains(&SharedString::from("Gruvbox Dark")));
        assert!(names.contains(&SharedString::from("Solarized Light")));
    }
}
