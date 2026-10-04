#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![recursion_limit = "256"]

use gpui_kit::App;
use std::path::PathBuf;

mod actions;
mod app_state;
mod assets;
mod core;
mod keybindings;
mod launch_args;
mod service;
mod settings;
mod theme;
mod ui;

use crate::assets::Assets;
pub use crate::launch_args::{CliArgs, LaunchArgs};
use ui::workspace::Workspace;

fn main() {
    init_panic_hook();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("initialize tokio runtime");
    let _guard = rt.enter();

    let launch_args = LaunchArgs::parse();

    let app = gpui_kit::application().with_assets(Assets);

    app.run(move |cx| {
        init_app(cx);
        Workspace::open_window(cx, launch_args).detach();
    });
}

/// Sets up a global panic hook with backtrace logging to a safe application data path.
fn init_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("CRITICAL PANIC: {info}\n{:?}", std::backtrace::Backtrace::force_capture());
        eprintln!("{msg}");
        let log_path = dirs::data_local_dir()
            .or_else(dirs::config_dir)
            .map(|p| p.join("xvw"))
            .and_then(|dir| {
                std::fs::create_dir_all(&dir).ok()?;
                Some(dir.join("xvw_panic.log"))
            })
            .unwrap_or_else(|| PathBuf::from("xvw_panic.log"));
        let _ = std::fs::write(log_path, msg);
    }));
}

/// Initializes application services, themes, keybindings, and UI subsystems.
fn init_app(cx: &mut App) {
    let settings = settings::Settings::load();

    app_state::AppState::init_with_settings(cx, &settings);
    gpui_kit::init(cx);
    theme::init(cx);
    theme::apply_settings(&settings, None, cx);
    settings::register_quit_handler(cx);
    keybindings::init(cx);
    ui::init(cx);
}
