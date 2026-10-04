use crate::service::{DiffService, DocumentService, SearchService, StructureService};
use gpui_kit::{App, BorrowAppContext, Global};

/// Application-wide editing mode shared by every open document view.
#[derive(Clone, Copy, Debug, Default)]
pub struct InsertModeState {
    pub enabled: bool,
}

impl Global for InsertModeState {}

impl InsertModeState {
    /// Returns whether the application is currently in Insert Mode.
    pub fn is_enabled(cx: &App) -> bool {
        cx.global::<Self>().enabled
    }

    /// Toggles Insert Mode and returns its new state.
    pub fn toggle(cx: &mut App) -> bool {
        let mut enabled = false;
        cx.update_global::<Self, _>(|state, _| {
            state.enabled = !state.enabled;
            enabled = state.enabled;
        });
        enabled
    }
}

/// Global state tracking the currently selected file path for comparison.
#[derive(Clone, Debug, Default)]
pub struct PendingCompareState {
    pub path: Option<String>,
}

impl Global for PendingCompareState {}

impl PendingCompareState {
    pub fn path(cx: &App) -> Option<String> {
        cx.try_global::<Self>().and_then(|s| s.path.clone())
    }

    pub fn set(path: Option<String>, cx: &mut App) {
        if cx.has_global::<Self>() {
            cx.update_global::<Self, _>(|state, _| {
                state.path = path;
            });
        } else {
            cx.set_global(Self { path });
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct AppState {
    pub document_service: DocumentService,
    pub structure_service: StructureService,
    pub search_service: SearchService,
    pub diff_service: DiffService,
}

impl Global for AppState {}

impl AppState {
    #[allow(dead_code)]
    pub fn init(cx: &mut App) {
        let settings = crate::settings::Settings::load();
        Self::init_with_settings(cx, &settings);
    }

    /// Initializes core application services and registers settings-driven globals.
    pub fn init_with_settings(cx: &mut App, settings: &crate::settings::Settings) {
        let state = Self {
            document_service: DocumentService::new(),
            structure_service: StructureService::new(),
            search_service: SearchService::new(),
            diff_service: DiffService::new(),
        };
        cx.set_global::<AppState>(state);
        cx.set_global(InsertModeState::default());
        cx.set_global(PendingCompareState::default());

        cx.set_global(settings.appearance.clone());
        cx.set_global(settings.default_encoding);
        cx.set_global(settings.default_radix);
        cx.set_global(settings.default_group_size);
        cx.set_global(settings.default_endianness);
        cx.set_global(crate::core::layout::BytesPerRow(settings.bytes_per_row));
        cx.set_global(crate::core::structure::StructureYamlShaThreshold(settings.structure_yaml_sha_threshold));
        cx.set_global(crate::core::structure::StructureYamlIncludeOffsets(settings.structure_yaml_include_offsets));
        cx.set_global(crate::settings::RecentHistoryState::from_settings(settings));
    }

    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    #[allow(dead_code)]
    pub fn global_mut(cx: &mut App) -> &mut Self {
        cx.global_mut::<Self>()
    }
}
