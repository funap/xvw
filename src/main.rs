#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gpui_kit::{App, KeyBinding};
use std::path::PathBuf;

mod actions;
mod app_state;
mod assets;
mod core;
mod service;
mod settings;
mod theme;
mod ui;

use crate::assets::Assets;
use ui::workspace::Workspace;

#[derive(Default, Clone, Debug)]
pub struct CliArgs {
    pub files_to_open: Vec<PathBuf>,
    pub folder_to_open: Option<PathBuf>,
    pub ksy_to_load: Option<PathBuf>,
    pub diff: Option<(PathBuf, PathBuf)>,
    pub panel: Option<String>,
    pub sidebar: Option<bool>,
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("initialize tokio runtime");
    let _guard = rt.enter();

    let cli_args = parse_cli_args();

    let app = gpui_kit::application().with_assets(Assets);

    app.run(move |cx| {
        init_app_state(cx);
        setup_menus(cx);
        setup_keybindings(cx);

        Workspace::open_window(cx, cli_args).detach();
    });
}

/// Parses command line arguments to determine files, folder, and options to open at launch.
fn parse_cli_args() -> CliArgs {
    let mut args = CliArgs::default();
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;

    while i < raw_args.len() {
        match raw_args[i].as_str() {
            "--ksy" => {
                if i + 1 < raw_args.len() {
                    args.ksy_to_load = Some(PathBuf::from(&raw_args[i + 1]));
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--diff" => {
                if i + 2 < raw_args.len() {
                    args.diff = Some((PathBuf::from(&raw_args[i + 1]), PathBuf::from(&raw_args[i + 2])));
                    i += 3;
                } else {
                    i += 1;
                }
            }
            "--folder" => {
                if i + 1 < raw_args.len() {
                    args.folder_to_open = Some(PathBuf::from(&raw_args[i + 1]));
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--panel" => {
                if i + 1 < raw_args.len() {
                    args.panel = Some(raw_args[i + 1].clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--no-sidebar" => {
                args.sidebar = Some(false);
                i += 1;
            }
            "--sidebar" => {
                args.sidebar = Some(true);
                i += 1;
            }
            other => {
                let path = PathBuf::from(other);
                if path.is_file() {
                    args.files_to_open.push(path);
                } else if path.is_dir() {
                    args.folder_to_open = Some(path);
                } else {
                    // Check if it's a valid path relative or absolute
                    args.files_to_open.push(path);
                }
                i += 1;
            }
        }
    }

    args
}

/// Initializes core application state, themes, and UI components.
fn init_app_state(cx: &mut App) {
    let settings = settings::Settings::load();

    app_state::AppState::init(cx);
    cx.set_global(settings.appearance.clone());
    cx.set_global(settings.default_encoding);
    cx.set_global(crate::core::layout::BytesPerRow(settings.bytes_per_row));
    cx.set_global(settings::RecentHistoryState::from_settings(&settings));

    gpui_kit::init(cx);
    theme::init(cx);
    theme::apply_settings(&settings, None, cx);
    settings::register_quit_handler(cx);
    ui::workspace::init(cx);
    ui::components::new_file_modal::init(cx);
    ui::components::fill_selection_modal::init(cx);
    ui::components::data_table::init(cx);
    ui::components::file_tree_view::init(cx);
    ui::components::goto_offset_bar::init(cx);
    ui::components::search_bar::init(cx);
    ui::components::search_panel::init(cx);
    ui::components::strings_panel::init(cx);
    ui::components::struct_tree_view::init(cx);
    ui::components::bookmark_panel::init(cx);
    ui::components::data_inspector::init(cx);
    ui::components::title_bar::init(cx);
    ui::panels::editor_panel::init(cx);
    ui::panels::diff_panel::init(cx);
}

/// Registers the application top menu bar items.
fn setup_menus(cx: &mut App) {
    cx.set_menus(crate::ui::menus::application_menus().iter().map(|menu| menu.to_gpui_menu()));
}

/// Registers global keybindings for window and document actions.
fn setup_keybindings(cx: &mut App) {
    cx.bind_keys([
        // File / Folder dialogs
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-n", crate::actions::NewFile, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-n", crate::actions::NewFile, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-o", crate::actions::OpenFileDialog, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-o", crate::actions::OpenFileDialog, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-o", crate::actions::OpenFolder, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-o", crate::actions::OpenFolder, None),
        // Save
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-s", crate::actions::Save, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-s", crate::actions::Save, None),
        // Panels & Views
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-b", crate::actions::ToggleLeftPanel, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-b", crate::actions::ToggleLeftPanel, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-f", crate::actions::ToggleSearchPanel, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-f", crate::actions::ToggleSearchPanel, None),
        // Tab switching
        KeyBinding::new("ctrl-tab", crate::actions::ActivateNextTab, None),
        KeyBinding::new("ctrl-shift-tab", crate::actions::ActivatePreviousTab, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-cmd-right", crate::actions::ActivateNextTab, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-ctrl-right", crate::actions::ActivateNextTab, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-cmd-left", crate::actions::ActivatePreviousTab, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-ctrl-left", crate::actions::ActivatePreviousTab, None),
        // Direct Tab Selection (1..9)
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-1", crate::actions::ActivateTab { index: 1 }, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-1", crate::actions::ActivateTab { index: 1 }, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-2", crate::actions::ActivateTab { index: 2 }, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-2", crate::actions::ActivateTab { index: 2 }, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-3", crate::actions::ActivateTab { index: 3 }, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-3", crate::actions::ActivateTab { index: 3 }, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-4", crate::actions::ActivateTab { index: 4 }, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-4", crate::actions::ActivateTab { index: 4 }, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-5", crate::actions::ActivateTab { index: 5 }, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-5", crate::actions::ActivateTab { index: 5 }, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-6", crate::actions::ActivateTab { index: 6 }, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-6", crate::actions::ActivateTab { index: 6 }, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-7", crate::actions::ActivateTab { index: 7 }, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-7", crate::actions::ActivateTab { index: 7 }, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-8", crate::actions::ActivateTab { index: 8 }, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-8", crate::actions::ActivateTab { index: 8 }, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-9", crate::actions::ActivateTab { index: 9 }, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-9", crate::actions::ActivateTab { index: 9 }, None),
        // Close & Quit
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-w", crate::actions::CloseActivePanel, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-w", crate::actions::CloseActivePanel, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-f4", crate::actions::CloseActivePanel, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-q", crate::actions::Quit, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-q", crate::actions::Quit, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-f4", crate::actions::Quit, None),
        // Settings
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-,", crate::actions::OpenSettings, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-,", crate::actions::OpenSettings, None),
        // Compare / Diff
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-cmd-d", crate::actions::CompareOpenFiles, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-ctrl-d", crate::actions::CompareOpenFiles, None),
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
