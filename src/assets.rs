use gpui_kit::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;

/// Embed application assets containing Lucide SVG icons and theme definitions.
#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "icons/**/*.svg"]
#[include = "themes/**/*.json"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        if let Some(f) = Self::get(path) {
            return Ok(Some(f.data));
        }

        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths: Vec<SharedString> = Self::iter().filter_map(|p| p.starts_with(path).then(|| p.into())).collect();
        if let Ok(component_paths) = gpui_kit::assets::Assets.list(path) {
            paths.extend(component_paths);
            paths.sort();
            paths.dedup();
        }
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assets_load_sort_icons() {
        let assets = Assets;
        let asc = assets.load("icons/sort-ascending.svg").expect("load asc").expect("asc exists");
        assert!(!asc.is_empty());
        assert!(std::str::from_utf8(&asc).unwrap().contains("<svg"));

        let desc = assets.load("icons/sort-descending.svg").expect("load desc").expect("desc exists");
        assert!(!desc.is_empty());
        assert!(std::str::from_utf8(&desc).unwrap().contains("<svg"));

        let default_sort = assets.load("icons/chevrons-up-down.svg").expect("load chevrons").expect("chevrons exist");
        assert!(!default_sort.is_empty());
        assert!(std::str::from_utf8(&default_sort).unwrap().contains("<svg"));
    }
}
