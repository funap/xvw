use crate::assets::Assets;
use crate::settings::{DEFAULT_DARK_THEME, DEFAULT_LIGHT_THEME, Settings};
use gpui_kit::component::theme::{Theme, ThemeConfig, ThemeMode, ThemeRegistry};
use gpui_kit::{Anchor, App, AssetSource, SharedString, Window, px};
use std::collections::HashMap;
use std::rc::Rc;

/// Loads all theme definition files found under `themes/` in the given asset source into a `ThemeRegistry`.
#[cfg(test)]
pub fn load_themes_from_assets(assets: &impl AssetSource) -> ThemeRegistry {
    let mut registry = ThemeRegistry::default();
    load_themes_into_registry(&mut registry, assets);
    registry
}

/// Loads all theme definition files found under `themes/` in the given asset source into an existing `ThemeRegistry`.
pub fn load_themes_into_registry(registry: &mut ThemeRegistry, assets: &impl AssetSource) {
    if let Ok(paths) = assets.list("themes/") {
        for path in paths {
            if path.ends_with(".json")
                && let Ok(Some(bytes)) = assets.load(&path)
                && let Ok(content) = std::str::from_utf8(&bytes)
                && let Err(err) = registry.load_themes_from_str(content)
            {
                eprintln!("Failed to parse embedded theme file '{path}': {err}");
            }
        }
    }
}

/// Returns a list of all available theme names, grouping Light themes first followed by Dark themes.
/// Within each group, the default theme comes first, followed by alphabetical order.
pub fn theme_names(themes: &HashMap<SharedString, Rc<ThemeConfig>>) -> Vec<SharedString> {
    let mut light_names: Vec<_> = Vec::new();
    let mut dark_names: Vec<_> = Vec::new();

    for (name, config) in themes {
        if name.as_ref() == DEFAULT_LIGHT_THEME || name.as_ref() == DEFAULT_DARK_THEME {
            continue;
        }
        if config.mode.is_dark() {
            dark_names.push(name.clone());
        } else {
            light_names.push(name.clone());
        }
    }

    light_names.sort();
    dark_names.sort();

    let mut result = Vec::with_capacity(themes.len());
    if themes.contains_key(DEFAULT_LIGHT_THEME) {
        result.push(SharedString::from(DEFAULT_LIGHT_THEME));
    }
    result.extend(light_names);

    if themes.contains_key(DEFAULT_DARK_THEME) {
        result.push(SharedString::from(DEFAULT_DARK_THEME));
    }
    result.extend(dark_names);

    result
}

/// Initializes embedded themes and registers them directly into the global `ThemeRegistry`.
pub fn init(cx: &mut App) {
    load_themes_into_registry(ThemeRegistry::global_mut(cx), &Assets);
    let default_light = ThemeRegistry::global(cx).themes().get(DEFAULT_LIGHT_THEME).cloned();
    let default_dark = ThemeRegistry::global(cx).themes().get(DEFAULT_DARK_THEME).cloned();

    Theme::update(cx, |theme| {
        if let Some(light_theme) = default_light {
            theme.light_theme = light_theme;
        }
        if let Some(dark_theme) = default_dark {
            theme.dark_theme = dark_theme;
        }
        theme.notification.placement = Anchor::BottomRight;
        theme.notification.margins.bottom = px(32.0);
    });
}

/// Applies the theme pair and mode configured in application settings.
pub fn apply_settings(settings: &Settings, window: Option<&mut Window>, cx: &mut App) {
    set_theme_pair(&settings.light_theme, &settings.dark_theme, settings.theme_mode, window, cx);
}

/// Sets the active light/dark theme pair and applies the active mode.
pub fn set_theme_pair(light_name: &str, dark_name: &str, mode: ThemeMode, window: Option<&mut Window>, cx: &mut App) {
    let registry = ThemeRegistry::global(cx);
    let light_theme = registry
        .themes()
        .get(light_name)
        .cloned()
        .or_else(|| registry.themes().get(DEFAULT_LIGHT_THEME).cloned());
    let dark_theme = registry
        .themes()
        .get(dark_name)
        .cloned()
        .or_else(|| registry.themes().get(DEFAULT_DARK_THEME).cloned());

    Theme::update(cx, |theme| {
        if let Some(light_theme) = light_theme {
            theme.light_theme = light_theme;
        }
        if let Some(dark_theme) = dark_theme {
            theme.dark_theme = dark_theme;
        }
        theme.mode = mode;
        theme.notification.placement = Anchor::BottomRight;
        theme.notification.margins.bottom = px(32.0);
    });

    Theme::change(mode, window, cx);
}

/// Applies an individual theme by name, activating the corresponding light or dark slot and mode.
pub fn apply_theme_by_name(name: &str, window: Option<&mut Window>, cx: &mut App) {
    let Some(theme_config) = ThemeRegistry::global(cx).themes().get(name).cloned() else {
        return;
    };

    let mode = theme_config.mode;
    Theme::update(cx, |theme| {
        if mode.is_dark() {
            theme.dark_theme = theme_config;
        } else {
            theme.light_theme = theme_config;
        }
        theme.mode = mode;
        theme.notification.placement = Anchor::BottomRight;
        theme.notification.margins.bottom = px(32.0);
    });

    Theme::change(mode, window, cx);
}

/// Returns a list of all available theme names, grouping Light themes first followed by Dark themes.
pub fn all_theme_names(cx: &App) -> Vec<SharedString> {
    theme_names(ThemeRegistry::global(cx).themes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Assets;

    #[test]
    fn test_embedded_themes_load_all_presets() {
        let registry = load_themes_from_assets(&Assets);

        // Verify Default themes
        assert!(registry.themes().get("Default Light").is_some());
        assert!(registry.themes().get("Default Dark").is_some());

        // Verify Ayu themes
        assert!(registry.themes().get("Ayu Light").is_some());
        assert!(registry.themes().get("Ayu Dark").is_some());

        // Verify Catppuccin themes
        assert!(registry.themes().get("Catppuccin Latte").is_some());
        assert!(registry.themes().get("Catppuccin Frappe").is_some());
        assert!(registry.themes().get("Catppuccin Macchiato").is_some());
        assert!(registry.themes().get("Catppuccin Mocha").is_some());

        // Verify Tokyo Night themes
        assert!(registry.themes().get("Tokyo Night").is_some());
        assert!(registry.themes().get("Tokyo Storm").is_some());
        assert!(registry.themes().get("Tokyo Moon").is_some());

        // Verify Gruvbox themes
        assert!(registry.themes().get("Gruvbox Light").is_some());
        assert!(registry.themes().get("Gruvbox Dark").is_some());

        // Verify Solarized themes
        assert!(registry.themes().get("Solarized Light").is_some());
        assert!(registry.themes().get("Solarized Dark").is_some());

        // Verify Adventure themes
        assert!(registry.themes().get("Adventure").is_some());
        assert!(registry.themes().get("Adventure Time").is_some());

        // Verify Alduin and Asciinema
        assert!(registry.themes().get("Alduin").is_some());
        assert!(registry.themes().get("Asciinema").is_some());

        // Verify Aurora
        assert!(registry.themes().get("Aurora Light").is_some());

        // Verify Everforest themes
        assert!(registry.themes().get("Everforest Light").is_some());
        assert!(registry.themes().get("Everforest Dark").is_some());

        // Verify Fahrenheit
        assert!(registry.themes().get("Fahrenheit").is_some());

        // Verify Flexoki themes
        assert!(registry.themes().get("Flexoki Light").is_some());
        assert!(registry.themes().get("Flexoki Dark").is_some());

        // Verify Harper, Hybrid, Jellybeans, Kibble
        assert!(registry.themes().get("Harper").is_some());
        assert!(registry.themes().get("Hybrid Light").is_some());
        assert!(registry.themes().get("Hybrid Dark").is_some());
        assert!(registry.themes().get("Jellybeans").is_some());
        assert!(registry.themes().get("Kibble").is_some());

        // Verify macOS Classic themes
        assert!(registry.themes().get("macOS Classic Light").is_some());
        assert!(registry.themes().get("macOS Classic Dark").is_some());

        // Verify Mellifluous themes
        assert!(registry.themes().get("Mellifluous Light").is_some());
        assert!(registry.themes().get("Mellifluous Dark").is_some());

        // Verify Molokai themes
        assert!(registry.themes().get("Molokai Light").is_some());
        assert!(registry.themes().get("Molokai Dark").is_some());

        // Verify Spaceduck and Twilight
        assert!(registry.themes().get("Spaceduck").is_some());
        assert!(registry.themes().get("Twilight").is_some());

        assert_eq!(theme_names(registry.themes()).len(), 38);
    }

    #[test]
    fn test_embedded_themes_names_sorted() {
        let registry = load_themes_from_assets(&Assets);
        let names = theme_names(registry.themes());
        assert_eq!(names.len(), 38);

        // First theme must be Default Light
        assert_eq!(names[0], SharedString::from("Default Light"));

        // Find the boundary where dark themes start
        let dark_index = names
            .iter()
            .position(|name| registry.themes().get(name).unwrap().mode.is_dark())
            .expect("must contain dark themes");

        // First dark theme must be Default Dark
        assert_eq!(names[dark_index], SharedString::from("Default Dark"));

        // All themes before dark_index must be light
        for name in &names[..dark_index] {
            assert!(
                !registry.themes().get(name).unwrap().mode.is_dark(),
                "theme {name} in light section should not be dark"
            );
        }

        // All themes from dark_index onward must be dark
        for name in &names[dark_index..] {
            assert!(
                registry.themes().get(name).unwrap().mode.is_dark(),
                "theme {name} in dark section should be dark"
            );
        }

        // Light themes after Default Light must be sorted alphabetically
        assert!(names[1..dark_index].windows(2).all(|w| w[0] <= w[1]));

        // Dark themes after Default Dark must be sorted alphabetically
        assert!(names[dark_index + 1..].windows(2).all(|w| w[0] <= w[1]));

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
