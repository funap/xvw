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

        // Verify Default themes
        assert!(embedded.get("Default Light").is_some());
        assert!(embedded.get("Default Dark").is_some());

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

        // Verify Adventure themes
        assert!(embedded.get("Adventure").is_some());
        assert!(embedded.get("Adventure Time").is_some());

        // Verify Alduin and Asciinema
        assert!(embedded.get("Alduin").is_some());
        assert!(embedded.get("Asciinema").is_some());

        // Verify Aurora
        assert!(embedded.get("Aurora Light").is_some());

        // Verify Everforest themes
        assert!(embedded.get("Everforest Light").is_some());
        assert!(embedded.get("Everforest Dark").is_some());

        // Verify Fahrenheit
        assert!(embedded.get("Fahrenheit").is_some());

        // Verify Flexoki themes
        assert!(embedded.get("Flexoki Light").is_some());
        assert!(embedded.get("Flexoki Dark").is_some());

        // Verify Harper, Hybrid, Jellybeans, Kibble
        assert!(embedded.get("Harper").is_some());
        assert!(embedded.get("Hybrid Light").is_some());
        assert!(embedded.get("Hybrid Dark").is_some());
        assert!(embedded.get("Jellybeans").is_some());
        assert!(embedded.get("Kibble").is_some());

        // Verify macOS Classic themes
        assert!(embedded.get("macOS Classic Light").is_some());
        assert!(embedded.get("macOS Classic Dark").is_some());

        // Verify Mellifluous themes
        assert!(embedded.get("Mellifluous Light").is_some());
        assert!(embedded.get("Mellifluous Dark").is_some());

        // Verify Molokai themes
        assert!(embedded.get("Molokai Light").is_some());
        assert!(embedded.get("Molokai Dark").is_some());

        // Verify Spaceduck and Twilight
        assert!(embedded.get("Spaceduck").is_some());
        assert!(embedded.get("Twilight").is_some());

        assert_eq!(embedded.theme_names().len(), 38);
    }

    #[test]
    fn test_embedded_themes_names_sorted() {
        let embedded = EmbeddedThemes::load_from_assets(&Assets);
        let names = embedded.theme_names();
        assert_eq!(names.len(), 38);
        assert!(names.contains(&SharedString::from("Adventure")));
        assert!(names.contains(&SharedString::from("Ayu Light")));
        assert!(names.contains(&SharedString::from("Catppuccin Mocha")));
        assert!(names.contains(&SharedString::from("Default Light")));
        assert!(names.contains(&SharedString::from("Everforest Dark")));
        assert!(names.contains(&SharedString::from("Gruvbox Dark")));
        assert!(names.contains(&SharedString::from("macOS Classic Light")));
        assert!(names.contains(&SharedString::from("Solarized Light")));
        assert!(names.contains(&SharedString::from("Tokyo Night")));
        assert!(names.contains(&SharedString::from("Twilight")));
    }
}
