use gpui_kit::{App, KeyBinding};

#[cfg(target_os = "macos")]
macro_rules! primary_key {
    ($key:literal) => {
        concat!("cmd-", $key)
    };
}
#[cfg(not(target_os = "macos"))]
macro_rules! primary_key {
    ($key:literal) => {
        concat!("ctrl-", $key)
    };
}

#[cfg(target_os = "macos")]
macro_rules! alt_primary_key {
    ($key:literal) => {
        concat!("alt-cmd-", $key)
    };
}
#[cfg(not(target_os = "macos"))]
macro_rules! alt_primary_key {
    ($key:literal) => {
        concat!("alt-ctrl-", $key)
    };
}

/// Registers global keybindings for window, tab, and document actions.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        // File / Folder dialogs
        KeyBinding::new(primary_key!("n"), crate::actions::NewFile, None),
        KeyBinding::new(primary_key!("o"), crate::actions::OpenFileDialog, None),
        KeyBinding::new(primary_key!("shift-o"), crate::actions::OpenFolder, None),
        // Save
        KeyBinding::new(primary_key!("s"), crate::actions::Save, None),
        // Panels & Views
        KeyBinding::new(primary_key!("b"), crate::actions::ToggleLeftPanel, None),
        KeyBinding::new(primary_key!("shift-f"), crate::actions::ToggleSearchPanel, None),
        // Tab switching
        KeyBinding::new("ctrl-tab", crate::actions::ActivateNextTab, None),
        KeyBinding::new("ctrl-shift-tab", crate::actions::ActivatePreviousTab, None),
        KeyBinding::new(alt_primary_key!("right"), crate::actions::ActivateNextTab, None),
        KeyBinding::new(alt_primary_key!("left"), crate::actions::ActivatePreviousTab, None),
        // Direct Tab Selection (1..9)
        KeyBinding::new(primary_key!("1"), crate::actions::ActivateTab { index: 1 }, None),
        KeyBinding::new(primary_key!("2"), crate::actions::ActivateTab { index: 2 }, None),
        KeyBinding::new(primary_key!("3"), crate::actions::ActivateTab { index: 3 }, None),
        KeyBinding::new(primary_key!("4"), crate::actions::ActivateTab { index: 4 }, None),
        KeyBinding::new(primary_key!("5"), crate::actions::ActivateTab { index: 5 }, None),
        KeyBinding::new(primary_key!("6"), crate::actions::ActivateTab { index: 6 }, None),
        KeyBinding::new(primary_key!("7"), crate::actions::ActivateTab { index: 7 }, None),
        KeyBinding::new(primary_key!("8"), crate::actions::ActivateTab { index: 8 }, None),
        KeyBinding::new(primary_key!("9"), crate::actions::ActivateTab { index: 9 }, None),
        // Close & Quit
        KeyBinding::new(primary_key!("w"), crate::actions::CloseActivePanel, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-f4", crate::actions::CloseActivePanel, None),
        KeyBinding::new(primary_key!("q"), crate::actions::Quit, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-f4", crate::actions::Quit, None),
        // Settings
        KeyBinding::new(primary_key!(","), crate::actions::OpenSettings, None),
        // Compare / Diff
        KeyBinding::new(alt_primary_key!("d"), crate::actions::CompareOpenFiles, None),
        // Standard text input shortcuts on non-macOS platforms
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-home", gpui_kit::component::input::MoveToStart, Some("Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-end", gpui_kit::component::input::MoveToEnd, Some("Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-home", gpui_kit::component::input::SelectToStart, Some("Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-end", gpui_kit::component::input::SelectToEnd, Some("Input")),
    ]);
}
