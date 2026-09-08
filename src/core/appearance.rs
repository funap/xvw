use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub font_family: String,
    pub font_size: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        let font_family = if cfg!(target_os = "macos") {
            "Menlo"
        } else if cfg!(target_os = "windows") {
            "Consolas"
        } else {
            "DejaVu Sans Mono"
        };

        Self {
            font_family: font_family.into(),
            font_size: 14.0,
        }
    }
}

impl gpui_kit::Global for Appearance {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_appearance_default() {
        let appearance = Appearance::default();
        assert_eq!(appearance.font_size, 14.0);

        #[cfg(target_os = "macos")]
        assert_eq!(appearance.font_family, "Menlo");

        #[cfg(target_os = "windows")]
        assert_eq!(appearance.font_family, "Consolas");

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        assert_eq!(appearance.font_family, "DejaVu Sans Mono");
    }
}
