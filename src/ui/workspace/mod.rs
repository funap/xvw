use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::actions::*;

use crate::ui::pane::{PaneTree, PaneTreeEvent, TabContent};
use crate::ui::panels::file_tree_panel::{FileTreePanel, FileTreePanelEvent};
use crate::ui::panels::left_panel::{LeftPanel, LeftPanelTab};
use crate::ui::views::diff_view::DiffView;
use crate::ui::views::editor_view::EditorView;

use crate::app_state::AppState;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, ByteOrder, DisplayRadix};
use gpui_kit::component::WindowExt;
use gpui_kit::component::resizable::ResizableState;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

mod action_router;
pub(crate) use action_router::reveal_in_file_explorer;
pub mod activity_bar;
mod dialog_flow;
mod render;
pub mod status_bar;
#[cfg(test)]
mod tests;
pub mod title_bar;
mod window;

pub use window::init;

pub use activity_bar::{Activity, ActivityBar, ActivityBarEvent};
pub use status_bar::{StatusBar, StatusBarEvent};
pub use title_bar::{AppTitleBar, AppTitleBarEvent};

/// Kind of palette currently supported in the workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteKind {
    QuickOpen,
    CommandPalette,
}

/// Mutually exclusive active top palette in the workspace.
#[derive(Clone)]
pub enum ActivePalette {
    QuickOpen(Entity<crate::ui::dialogs::quick_open_modal::QuickOpenModal>),
    CommandPalette(Entity<crate::ui::dialogs::command_palette_modal::CommandPaletteModal>),
}

impl ActivePalette {
    pub fn kind(&self) -> PaletteKind {
        match self {
            ActivePalette::QuickOpen(_) => PaletteKind::QuickOpen,
            ActivePalette::CommandPalette(_) => PaletteKind::CommandPalette,
        }
    }
}

/// Computes the next active palette kind given the current palette and an incoming trigger.
/// If the same palette kind is already open, it returns None to toggle it closed.
/// If a different palette kind is active, it replaces it with the incoming kind.
#[must_use]
pub fn next_active_palette_kind(current: Option<PaletteKind>, incoming: PaletteKind) -> Option<PaletteKind> {
    match current {
        Some(kind) if kind == incoming => None,
        _ => Some(incoming),
    }
}

pub const DEFAULT_LEFT_PANEL_WIDTH: f32 = 250.0;
pub const MIN_LEFT_PANEL_WIDTH: f32 = 140.0;
pub const MAX_LEFT_PANEL_WIDTH: f32 = 800.0;

pub struct Workspace {
    pub pane_tree: Entity<PaneTree>,
    pub title_bar: Entity<AppTitleBar>,
    pub status_bar: Entity<StatusBar>,
    pub left_panel: Entity<LeftPanel>,
    pub activity_bar: Entity<ActivityBar>,
    pub recent_definition_history: crate::core::structure::DefinitionHistory,
    pub recent_file_history: crate::core::recent_history::FileHistory,
    pub is_left_panel_visible: bool,
    pub left_panel_width: f32,
    pub left_panel_resizable_state: Entity<ResizableState>,
    pub scratchpad_panel: Entity<crate::ui::views::scratchpad_view::ScratchpadView>,
    pub workspace_resizable_state: Entity<ResizableState>,
    pub is_right_panel_visible: bool,
    pub active_palette: Option<ActivePalette>,
    pub untitled_count: usize,
    pub(crate) force_close: bool,
    focus_handle: FocusHandle,
    last_active_editor_id: Cell<Option<EntityId>>,
    last_active_diff_id: Cell<Option<EntityId>>,
    last_active_editor: RefCell<Option<WeakEntity<Editor>>>,
    last_active_editor_view: RefCell<Option<WeakEntity<EditorView>>>,
    last_native_menu_state: Cell<Option<crate::ui::menus::MenuEditorState>>,
    active_editor_menu_subscription: RefCell<Option<Subscription>>,
    active_diff_menu_subscription: RefCell<Option<Subscription>>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pane_tree = cx.new(|_| PaneTree::new());

        cx.subscribe_in(&pane_tree, window, |this, _, event: &PaneTreeEvent, window, cx| match event {
            PaneTreeEvent::ActiveEditorChanged => {
                this.sync_active_editor(window, cx);
            }
            PaneTreeEvent::RequestCloseTab { group_id, tab_id } => {
                this.request_close_tab(*group_id, *tab_id, window, cx);
            }
            PaneTreeEvent::RequestCloseGroup { group_id } => {
                this.request_close_group(*group_id, window, cx);
            }
            PaneTreeEvent::RequestCloseOtherTabs { group_id, keep_tab_id } => {
                this.request_close_other_tabs_in_group(*group_id, *keep_tab_id, window, cx);
            }
            PaneTreeEvent::RequestCloseTabsToRight { group_id, target_tab_id } => {
                this.request_close_tabs_to_right_in_group(*group_id, *target_tab_id, window, cx);
            }
            PaneTreeEvent::RequestCloseSavedTabs { group_id } => {
                this.request_close_saved_tabs_in_group(*group_id, window, cx);
            }
        })
        .detach();

        let workspace_weak = cx.entity().downgrade();
        let title_bar = cx.new(|cx| AppTitleBar::new(workspace_weak, window, cx));

        cx.subscribe_in(&title_bar, window, |this, _, event, window, cx| match event {
            AppTitleBarEvent::OpenSettings => {
                this.open_settings_view(window, cx);
            }
            AppTitleBarEvent::OpenAbout => {
                this.open_about_modal(window, cx);
            }
        })
        .detach();

        let file_tree = cx.new(|cx| FileTreePanel::new("FILES", window, cx));
        let left_panel = cx.new(|cx| LeftPanel::new(file_tree.clone(), window, cx));
        let activity_bar = cx.new(ActivityBar::new);

        cx.subscribe_in(&activity_bar, window, |this, _, event: &ActivityBarEvent, window, cx| match event {
            ActivityBarEvent::Select(activity) => {
                this.toggle_activity(*activity, window, cx);
            }
            ActivityBarEvent::OpenSettings => {
                this.open_settings_view(window, cx);
            }
        })
        .detach();

        cx.observe(&left_panel, |this, _, cx| {
            this.sync_activity_bar(cx);
        })
        .detach();

        let status_bar = cx.new(StatusBar::new);
        cx.subscribe_in(&status_bar, window, |this, _, event, window, cx| match event {
            StatusBarEvent::ToggleLeftPanel => {
                this.set_left_panel_visible(!this.is_left_panel_visible, window, cx);
            }
        })
        .detach();

        let (handles, bookmark_panel, struct_tree) = {
            let left_read = left_panel.read(cx);
            (
                [
                    file_tree.read(cx).focus_handle(cx),
                    left_read.search_panel.read(cx).focus_handle(cx),
                    left_read.strings_panel.read(cx).focus_handle(cx),
                    left_read.zlib_panel.read(cx).focus_handle(cx),
                    left_read.struct_tree.read(cx).focus_handle(cx),
                    left_read.data_inspector.read(cx).focus_handle(cx),
                    left_read.visual_map.read(cx).focus_handle(cx),
                    left_read.checksum_panel.read(cx).focus_handle(cx),
                    left_read.checksum_panel.read(cx).compare_input.read(cx).focus_handle(cx),
                    left_read.bookmark_panel.read(cx).focus_handle(cx),
                ],
                left_read.bookmark_panel.clone(),
                left_read.struct_tree.clone(),
            )
        };

        for handle in handles {
            cx.on_focus_in(&handle, window, |this, _, cx| {
                this.on_focus_changed(cx);
                cx.notify();
            })
            .detach();
        }

        cx.subscribe_in(
            &bookmark_panel,
            window,
            |this, _, event: &crate::ui::panels::bookmark_panel::BookmarkPanelEvent, _window, cx| match event {
                crate::ui::panels::bookmark_panel::BookmarkPanelEvent::Export => {
                    this.on_action_export_bookmarks(&crate::actions::ExportBookmarks, _window, cx);
                }
                crate::ui::panels::bookmark_panel::BookmarkPanelEvent::Import => {
                    this.on_action_import_bookmarks(&crate::actions::ImportBookmarks, _window, cx);
                }
                crate::ui::panels::bookmark_panel::BookmarkPanelEvent::NavigateTo { offset, size } => {
                    if let Some(editor_view) = this.effective_editor_view(cx) {
                        editor_view.update(cx, |view, cx| {
                            let len = (*size).max(1);
                            view.scroll_to_range_if_needed(*offset..offset.saturating_add(len), cx);
                        });
                    }
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &struct_tree,
            window,
            |this, _, event: &crate::ui::panels::struct_tree_panel::StructTreePanelEvent, window, cx| match event {
                crate::ui::panels::struct_tree_panel::StructTreePanelEvent::NavigateTo { offset, size } => {
                    if let Some(editor_view) = this.effective_editor_view(cx) {
                        editor_view.update(cx, |view, cx| {
                            let len = (*size).max(1);
                            view.scroll_to_range_if_needed(*offset..offset.saturating_add(len), cx);
                        });
                    }
                }
                crate::ui::panels::struct_tree_panel::StructTreePanelEvent::FocusEditor => {
                    this.restore_editor_focus(window, cx);
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &left_panel,
            window,
            |this, _, event: &crate::ui::panels::search_panel::SearchPanelEvent, window, cx| match event {
                crate::ui::panels::search_panel::SearchPanelEvent::NavigateTo { offset, len } => {
                    if let Some(editor_view) = this.effective_editor_view(cx) {
                        editor_view.update(cx, |view, cx| {
                            let match_len = (*len).max(1);
                            view.scroll_to_range_if_needed(*offset..offset.saturating_add(match_len), cx);
                        });
                    }
                }
                crate::ui::panels::search_panel::SearchPanelEvent::FocusEditor => {
                    this.restore_editor_focus(window, cx);
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &left_panel,
            window,
            |this, _, event: &crate::ui::panels::strings_panel::StringsPanelEvent, window, cx| match event {
                crate::ui::panels::strings_panel::StringsPanelEvent::NavigateTo { offset, len } => {
                    if let Some(editor_view) = this.effective_editor_view(cx) {
                        editor_view.update(cx, |view, cx| {
                            let match_len = (*len).max(1);
                            view.scroll_to_range_if_needed(*offset..offset.saturating_add(match_len), cx);
                        });
                    }
                }
                crate::ui::panels::strings_panel::StringsPanelEvent::FocusEditor => {
                    this.restore_editor_focus(window, cx);
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &left_panel,
            window,
            |this, _, event: &crate::ui::panels::zlib_panel::ZlibPanelEvent, window, cx| match event {
                crate::ui::panels::zlib_panel::ZlibPanelEvent::NavigateTo { offset, len } => {
                    if let Some(editor_view) = this.effective_editor_view(cx) {
                        editor_view.update(cx, |view, cx| {
                            let match_len = (*len).max(1);
                            view.scroll_to_range_if_needed(*offset..offset.saturating_add(match_len), cx);
                        });
                    }
                }
                crate::ui::panels::zlib_panel::ZlibPanelEvent::FocusEditor => {
                    this.restore_editor_focus(window, cx);
                }
                crate::ui::panels::zlib_panel::ZlibPanelEvent::OpenDecompressedDocument { source_path, address, buffer } => {
                    this.open_decompressed_zlib_document(source_path, *address, buffer.clone(), window, cx);
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &left_panel,
            window,
            |this, _, event: &crate::ui::panels::visual_map_panel::VisualMapPanelEvent, _window, cx| match event {
                crate::ui::panels::visual_map_panel::VisualMapPanelEvent::NavigateTo { offset } => {
                    if let Some(editor_view) = this.effective_editor_view(cx) {
                        editor_view.update(cx, |view, cx| {
                            view.scroll_to_byte_if_needed(*offset, cx);
                        });
                    }
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &left_panel,
            window,
            |this, _, event: &crate::ui::panels::checksum_panel::ChecksumPanelEvent, window, cx| match event {
                crate::ui::panels::checksum_panel::ChecksumPanelEvent::NavigateTo { offset, len } => {
                    if let Some(editor_view) = this.effective_editor_view(cx) {
                        editor_view.update(cx, |view, cx| {
                            let match_len = (*len).max(1);
                            view.scroll_to_range_if_needed(*offset..offset.saturating_add(match_len), cx);
                        });
                    }
                }
                crate::ui::panels::checksum_panel::ChecksumPanelEvent::FocusEditor => {
                    this.restore_editor_focus(window, cx);
                }
            },
        )
        .detach();

        cx.subscribe(&left_panel, |_, _, event: &FileTreePanelEvent, cx| match event {
            FileTreePanelEvent::OpenFile { path, format, record_recent } => {
                let action = if *record_recent {
                    crate::actions::OpenFile::with_format(path.to_string_lossy().to_string(), *format)
                } else {
                    crate::actions::OpenFile::without_recording(path.to_string_lossy().to_string(), *format)
                };
                cx.dispatch_action(&action);
            }
        })
        .detach();

        let recent_history = cx.global::<crate::settings::RecentHistoryState>().clone();
        let recent_definition_paths = recent_history.definitions.paths().to_vec();
        let recent_file_entries = recent_history.files.entries().to_vec();
        let scratchpad_panel = cx.new(|cx| {
            let scratchpad_settings = cx.try_global::<crate::settings::ScratchpadSettings>().cloned().unwrap_or_default();
            let target_entry = if let Some(last_path) = &scratchpad_settings.last_open_file {
                if last_path.exists() {
                    let id = crate::service::ScratchService::extract_id_from_filename(last_path.file_name().and_then(|n| n.to_str()).unwrap_or(""));
                    Some((id.unwrap_or(1), last_path.clone()))
                } else {
                    None
                }
            } else {
                None
            };

            let (id, path, content) = if let Some((id, path)) = target_entry {
                let content = crate::service::ScratchService::load_scratch(&path)
                    .unwrap_or_else(|_| crate::ui::views::scratchpad_view::DEFAULT_SCRATCHPAD_CONTENT.to_string());
                (id, path, content)
            } else {
                let recent = crate::service::ScratchService::list_scratches();
                if let Some(entry) = recent.first() {
                    let content = crate::service::ScratchService::load_scratch(&entry.path)
                        .unwrap_or_else(|_| crate::ui::views::scratchpad_view::DEFAULT_SCRATCHPAD_CONTENT.to_string());
                    let id = entry.id.unwrap_or(1);
                    (id, entry.path.clone(), content)
                } else {
                    let id = 1;
                    let path = crate::service::ScratchService::scratch_file_path(id).unwrap_or_else(|| PathBuf::from(format!("scratch_{id}.md")));
                    let content = crate::ui::views::scratchpad_view::DEFAULT_SCRATCHPAD_CONTENT.to_string();
                    (id, path, content)
                }
            };

            crate::ui::views::scratchpad_view::ScratchpadView::new_with_file(id, path, content, window, cx)
        });

        let left_panel_resizable_state = cx.new(|_| ResizableState::default());
        let workspace_resizable_state = cx.new(|_| ResizableState::default());

        cx.observe_global::<crate::app_state::InsertModeState>(|this, cx| {
            this.sync_native_menus(cx);
        })
        .detach();

        let workspace = Self {
            pane_tree,
            title_bar,
            status_bar,
            left_panel,
            activity_bar,
            recent_definition_history: recent_history.definitions,
            recent_file_history: recent_history.files,
            is_left_panel_visible: true,
            left_panel_width: DEFAULT_LEFT_PANEL_WIDTH,
            left_panel_resizable_state,
            scratchpad_panel,
            workspace_resizable_state,
            is_right_panel_visible: false,
            active_palette: None,
            untitled_count: 0,
            force_close: false,
            focus_handle: cx.focus_handle(),
            last_active_editor_id: Cell::new(None),
            last_active_diff_id: Cell::new(None),
            last_active_editor: RefCell::new(None),
            last_active_editor_view: RefCell::new(None),
            last_native_menu_state: Cell::new(None),
            active_editor_menu_subscription: RefCell::new(None),
            active_diff_menu_subscription: RefCell::new(None),
        };

        workspace.left_panel.update(cx, |panel, cx| {
            panel.set_structure_definition_history(&recent_definition_paths, cx);
            panel.set_file_history(&recent_file_entries, cx);
        });
        workspace.sync_native_menus(cx);

        workspace
    }

    pub fn active_editor(&self, cx: &App) -> Option<Entity<Editor>> {
        self.pane_tree.read(cx).active_editor(cx)
    }

    pub fn active_editor_view(&self, cx: &App) -> Option<Entity<EditorView>> {
        self.pane_tree.read(cx).active_tab_as::<Entity<EditorView>>(cx)
    }

    /// Returns the currently active `DiffView` tab in the active pane group, if any.
    pub fn active_diff_view(&self, cx: &App) -> Option<Entity<DiffView>> {
        self.pane_tree.read(cx).active_tab_as::<Entity<DiffView>>(cx)
    }

    /// Resolves the initial encoding for a newly opened diff tab, preferring any open editor
    /// for either compared file before falling back to the global default encoding.
    pub(crate) fn initial_diff_encoding(
        &self,
        left_document: &Arc<RwLock<crate::core::document::Document>>,
        right_document: &Arc<RwLock<crate::core::document::Document>>,
        cx: &App,
    ) -> Encoding {
        let normalize = |path: &Path| -> PathBuf {
            if path.as_os_str().is_empty() {
                PathBuf::new()
            } else {
                path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
            }
        };

        let left_path = left_document.read().map(|d| normalize(d.path())).unwrap_or_default();
        let right_path = right_document.read().map(|d| normalize(d.path())).unwrap_or_default();
        let default_encoding = *cx.global::<Encoding>();

        let active_entry = self.active_editor(cx).and_then(|ed| {
            let editor = ed.read(cx);
            let path = editor.document.read().ok().map(|d| normalize(d.path()))?;
            Some((path, editor.options.encoding))
        });

        let mut open_entries: Vec<(PathBuf, Encoding)> = Vec::new();
        for group in self.pane_tree.read(cx).all_groups() {
            for tab in &group.read(cx).tabs {
                if let Some(ed) = tab.content.editor(cx) {
                    let editor = ed.read(cx);
                    if let Ok(doc) = editor.document.read() {
                        open_entries.push((normalize(doc.path()), editor.options.encoding));
                    }
                }
            }
        }

        let open_refs: Vec<(&Path, Encoding)> = open_entries.iter().map(|(p, enc)| (p.as_path(), *enc)).collect();
        resolve_initial_diff_encoding(
            default_encoding,
            &left_path,
            &right_path,
            active_entry.as_ref().map(|(p, enc)| (p.as_path(), *enc)),
            &open_refs,
        )
    }

    /// Evaluates the current editor state for menu and command enablement.
    pub fn menu_editor_state(&self, cx: &App) -> crate::ui::menus::MenuEditorState {
        use crate::ui::menus::MenuEditorState;

        let has_scratch = self.active_scratchpad(cx).is_some();
        let active_diff = self.active_diff_view(cx);
        let has_diff = active_diff.is_some();
        let is_insert_mode = crate::app_state::InsertModeState::is_enabled(cx);

        let (has_tab, can_close_others, can_close_right, has_saved) = if let Some(group) = self.pane_tree.read(cx).active_group(cx) {
            let g = group.read(cx);
            let has_tab = !g.tabs.is_empty();
            let can_close_others = g.tabs.len() > 1;
            let active_ix = g.active_index;
            let can_close_right = has_tab && active_ix + 1 < g.tabs.len();
            let has_saved = g.tabs.iter().any(|t| !t.is_dirty(cx));
            (has_tab, can_close_others, can_close_right, has_saved)
        } else {
            (false, false, false, false)
        };

        if let Some(editor) = self.active_editor(cx) {
            let ed = editor.read(cx);
            let is_ro = ed.is_read_only();
            let can_u = !is_ro && ed.can_undo();
            let can_r = !is_ro && ed.can_redo();
            let has_sel = ed.has_selection();
            let can_copy = if let Some(editor_view) = self.active_editor_view(cx) {
                editor_view.read(cx).hex_view().read(cx).copy_range(cx).is_some()
            } else {
                has_sel
            };
            MenuEditorState {
                has_doc: true,
                has_tab,
                has_diff,
                is_read_only: is_ro,
                is_insert_mode,
                can_undo: can_u,
                can_redo: can_r,
                has_selection: has_sel,
                can_copy,
                can_close_others,
                can_close_right,
                has_saved,
                has_scratch,
                encoding: Some(ed.options.encoding),
                radix: Some(ed.options.radix),
                group_size: Some(ed.options.group_size),
                byte_order: Some(if ed.options.is_big_endian {
                    crate::core::radix::ByteOrder::BigEndian
                } else {
                    crate::core::radix::ByteOrder::LittleEndian
                }),
                inline_structure_view: ed.structure.show_inline_structure_view,
                hide_unbookmarked: ed.bookmarks().is_hide_unbookmarked(),
            }
        } else {
            MenuEditorState {
                has_tab,
                has_diff,
                is_read_only: true,
                is_insert_mode,
                can_close_others,
                can_close_right,
                has_saved,
                has_scratch,
                encoding: active_diff.map(|diff| diff.read(cx).encoding()),
                ..Default::default()
            }
        }
    }

    /// Synchronizes the native OS menu bar whenever the derived `MenuEditorState` changes.
    pub(crate) fn sync_native_menus(&self, cx: &App) {
        let state = self.menu_editor_state(cx);
        if self.last_native_menu_state.get() == Some(state) {
            return;
        }
        self.last_native_menu_state.set(Some(state));
        crate::ui::menus::update_application_menus_with_state(&state, cx);
    }

    /// Finds any open EditorView across all pane groups, preferring the active group.
    pub fn any_editor_view(&self, cx: &App) -> Option<Entity<EditorView>> {
        if let Some(view) = self.active_editor_view(cx) {
            return Some(view);
        }
        for group in self.pane_tree.read(cx).all_groups() {
            if let Some(view) = group.read(cx).active_tab_as::<Entity<EditorView>>() {
                return Some(view);
            }
        }
        for group in self.pane_tree.read(cx).all_groups() {
            for tab in &group.read(cx).tabs {
                if let Some(view) = tab.content.downcast::<Entity<EditorView>>() {
                    return Some(view.clone());
                }
            }
        }
        None
    }

    /// Finds and optionally switches to an open EditorView across all pane groups.
    pub fn find_or_activate_editor_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<EditorView>> {
        if let Some(view) = self.active_editor_view(cx) {
            return Some(view);
        }
        for group in self.pane_tree.read(cx).all_groups() {
            if let Some(view) = group.read(cx).active_tab_as::<Entity<EditorView>>() {
                return Some(view);
            }
        }
        let mut target = None;
        for group in self.pane_tree.read(cx).all_groups() {
            let tabs = &group.read(cx).tabs;
            for (idx, tab) in tabs.iter().enumerate() {
                if let Some(view) = tab.content.downcast::<Entity<EditorView>>() {
                    target = Some((group.clone(), idx, view.clone()));
                    break;
                }
            }
            if target.is_some() {
                break;
            }
        }

        if let Some((group, idx, view)) = target {
            group.update(cx, |g, cx| {
                g.activate_tab(idx, window, cx);
            });
            self.sync_active_editor(window, cx);
            cx.notify();
            Some(view)
        } else {
            None
        }
    }

    /// Finds and activates an open EditorView matching `target_path` across all pane groups.
    pub fn find_or_activate_editor_view_for_path(
        &mut self,
        target_path: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<EditorView>> {
        use crate::service::ScratchService;

        let effective_target = target_path.canonicalize().unwrap_or_else(|_| target_path.to_path_buf());

        // 1. Check if the currently active editor view already matches target_path
        if let Some(view) = self.active_editor_view(cx) {
            let matches = view
                .read(cx)
                .editor()
                .read(cx)
                .document
                .read()
                .ok()
                .is_some_and(|doc| ScratchService::is_same_target_file(doc.path(), &effective_target));
            if matches {
                return Some(view);
            }
        }

        // 2. Search all groups and tabs
        let mut target = None;
        let all_groups = self.pane_tree.read(cx).all_groups();
        for group in all_groups {
            let group_ref = group.read(cx);
            for (idx, tab) in group_ref.tabs.iter().enumerate() {
                let matches = tab
                    .path(cx)
                    .is_some_and(|tab_path| ScratchService::is_same_target_file(&tab_path, &effective_target));
                if matches && let Some(view) = tab.content.downcast::<Entity<EditorView>>() {
                    target = Some((group.clone(), idx, view));
                    break;
                }
            }
            if target.is_some() {
                break;
            }
        }

        if let Some((group, idx, view)) = target {
            let group_id = group.read(cx).id;
            group.update(cx, |g, cx| {
                g.activate_tab(idx, window, cx);
            });
            self.pane_tree.update(cx, |tree, cx| {
                tree.set_active_group(group_id, cx);
            });
            self.sync_active_editor(window, cx);
            cx.notify();
            Some(view)
        } else {
            None
        }
    }

    pub fn active_scratchpad(&self, cx: &App) -> Option<Entity<crate::ui::views::scratchpad_view::ScratchpadView>> {
        if self.is_right_panel_visible {
            return Some(self.scratchpad_panel.clone());
        }
        self.pane_tree
            .read(cx)
            .active_tab_as::<Entity<crate::ui::views::scratchpad_view::ScratchpadView>>(cx)
    }

    /// Finds any open ScratchpadView across all pane groups, preferring the active group.
    pub fn any_scratchpad(&self, cx: &App) -> Option<Entity<crate::ui::views::scratchpad_view::ScratchpadView>> {
        if let Some(sp) = self.active_scratchpad(cx) {
            return Some(sp);
        }
        for group in self.pane_tree.read(cx).all_groups() {
            if let Some(sp) = group.read(cx).active_tab_as::<Entity<crate::ui::views::scratchpad_view::ScratchpadView>>() {
                return Some(sp);
            }
        }
        for group in self.pane_tree.read(cx).all_groups() {
            for tab in &group.read(cx).tabs {
                if let Some(sp) = tab.content.downcast::<Entity<crate::ui::views::scratchpad_view::ScratchpadView>>() {
                    return Some(sp);
                }
            }
        }
        Some(self.scratchpad_panel.clone())
    }

    #[allow(dead_code)]
    pub fn all_open_scratchpads(&self, cx: &App) -> Vec<Entity<crate::ui::views::scratchpad_view::ScratchpadView>> {
        let mut scratches = Vec::new();
        if self.is_right_panel_visible {
            scratches.push(self.scratchpad_panel.clone());
        }
        for group in self.pane_tree.read(cx).all_groups() {
            for tab in &group.read(cx).tabs {
                if let Some(sp) = tab.content.downcast::<Entity<crate::ui::views::scratchpad_view::ScratchpadView>>() {
                    scratches.push(sp);
                }
            }
        }
        if scratches.is_empty() {
            scratches.push(self.scratchpad_panel.clone());
        }
        scratches
    }

    /// Checks whether the specified Editor is still open in any pane group.
    pub fn is_editor_open(&self, editor: &Entity<Editor>, cx: &App) -> bool {
        for group in self.pane_tree.read(cx).all_groups() {
            for tab in &group.read(cx).tabs {
                if let Some(tab_editor) = tab.content.editor(cx)
                    && tab_editor.entity_id() == editor.entity_id()
                {
                    return true;
                }
            }
        }
        false
    }

    /// Checks whether the specified EditorView is still open in any pane group.
    pub fn is_editor_view_open(&self, view: &Entity<EditorView>, cx: &App) -> bool {
        for group in self.pane_tree.read(cx).all_groups() {
            for tab in &group.read(cx).tabs {
                if let Some(tab_view) = tab.content.downcast::<Entity<EditorView>>()
                    && tab_view.entity_id() == view.entity_id()
                {
                    return true;
                }
            }
        }
        false
    }

    /// Returns the first open Editor in the workspace pane tree, if any.
    pub fn any_open_editor(&self, cx: &App) -> Option<Entity<Editor>> {
        for group in self.pane_tree.read(cx).all_groups() {
            for tab in &group.read(cx).tabs {
                if let Some(tab_editor) = tab.content.editor(cx) {
                    return Some(tab_editor);
                }
            }
        }
        None
    }

    /// Returns the first open EditorView in the workspace pane tree, if any.
    pub fn any_open_editor_view(&self, cx: &App) -> Option<Entity<EditorView>> {
        for group in self.pane_tree.read(cx).all_groups() {
            for tab in &group.read(cx).tabs {
                if let Some(tab_view) = tab.content.downcast::<Entity<EditorView>>() {
                    return Some(tab_view);
                }
            }
        }
        None
    }

    /// Returns the effective Editor for panel displays and inspection.
    /// Falls back to the last active editor if the current active tab has no editor (e.g. Scratchpad).
    pub fn effective_editor(&self, cx: &App) -> Option<Entity<Editor>> {
        let current = self.active_editor(cx);
        let last = self.last_active_editor.borrow().as_ref().and_then(|w| w.upgrade());
        resolve_effective_item(current, last, |editor| self.is_editor_open(editor, cx), || self.any_open_editor(cx))
    }

    /// Returns the effective EditorView for navigation from side panels.
    /// Falls back to the last active editor view if the current active tab has no editor view (e.g. Scratchpad).
    pub fn effective_editor_view(&self, cx: &App) -> Option<Entity<EditorView>> {
        let current = self.active_editor_view(cx);
        let last = self.last_active_editor_view.borrow().as_ref().and_then(|w| w.upgrade());
        resolve_effective_item(current, last, |view| self.is_editor_view_open(view, cx), || self.any_open_editor_view(cx))
    }

    /// Returns the file path of the currently active or effective editor document, if any.
    pub fn active_file_path(&self, cx: &App) -> Option<PathBuf> {
        let editor = self.effective_editor(cx)?;
        let doc = editor.read(cx).document.read().ok()?;
        let path = doc.path();
        if path.as_os_str().is_empty() {
            None
        } else {
            Some(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
        }
    }

    /// Returns the file name of the currently active or effective editor document, if any.
    ///
    /// Maintained primarily for display labels and test assertions; internal routing uses [`Self::active_file_path`].
    #[allow(dead_code)]
    pub fn active_file_name(&self, cx: &App) -> Option<String> {
        let path = self.active_file_path(cx)?;
        path.file_name()
            .and_then(|n| n.to_str())
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.to_string())
    }

    /// Updates the scratchpad panel to link to the active file in its frontmatter
    /// if the scratchpad has not yet been edited or saved to disk.
    pub(crate) fn update_scratchpad_for_active_file_if_untouched(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.scratchpad_panel.read(cx).is_untouched()
            && let Some(target_path) = self.active_file_path(cx)
        {
            let id = self.scratchpad_panel.read(cx).id();
            let path = self.scratchpad_panel.read(cx).file_path().to_path_buf();
            let content = crate::ui::views::scratchpad_view::default_content_for_target(Some(&target_path), None);
            self.scratchpad_panel.update(cx, |sp, cx| {
                sp.load_file(id, path, content, window, cx);
            });
        }
    }
}

/// Resolves the initial text encoding for a newly opened diff view.
///
/// Prefers the active editor's encoding if it matches either compared file,
/// then any open editor matching the left file, then the right file, and
/// finally falls back to the workspace default encoding.
pub fn resolve_initial_diff_encoding(
    default_encoding: Encoding,
    left_path: &Path,
    right_path: &Path,
    active_editor: Option<(&Path, Encoding)>,
    open_editors: &[(&Path, Encoding)],
) -> Encoding {
    let matches_non_empty = |candidate: &Path, target: &Path| !candidate.as_os_str().is_empty() && candidate == target;

    if let Some((path, enc)) = active_editor
        && (matches_non_empty(path, left_path) || matches_non_empty(path, right_path))
    {
        return enc;
    }
    if let Some((_, enc)) = open_editors.iter().find(|(path, _)| matches_non_empty(path, left_path)) {
        return *enc;
    }
    if let Some((_, enc)) = open_editors.iter().find(|(path, _)| matches_non_empty(path, right_path)) {
        return *enc;
    }
    default_encoding
}

/// Resolves the effective target item (e.g. Editor or EditorView) for panel inspection and navigation.
///
/// If `current_active` is present, it takes priority.
/// When `current_active` is absent (such as when focus is on a Scratchpad),
/// it falls back to `last_active` if it is still open, or otherwise to any open item.
pub fn resolve_effective_item<T: Clone>(
    current_active: Option<T>,
    last_active: Option<T>,
    is_open: impl Fn(&T) -> bool,
    fallback_any: impl FnOnce() -> Option<T>,
) -> Option<T> {
    if let Some(active) = current_active {
        return Some(active);
    }
    if let Some(last) = last_active
        && is_open(&last)
    {
        return Some(last);
    }
    fallback_any()
}

impl Workspace {
    pub(crate) fn publish_recent_history(&mut self, cx: &mut Context<Self>) {
        let definition_paths = self.recent_definition_history.paths().to_vec();
        let file_entries = self.recent_file_history.entries().to_vec();
        self.left_panel.update(cx, |panel, cx| {
            panel.set_structure_definition_history(&definition_paths, cx);
            panel.set_file_history(&file_entries, cx);
        });

        let definitions = self.recent_definition_history.clone();
        let files = self.recent_file_history.clone();
        cx.update_global::<crate::settings::RecentHistoryState, _>(|state, _| {
            state.definitions = definitions;
            state.files = files;
        });
        crate::settings::save_current(cx);
    }

    pub(crate) fn record_recent_file(&mut self, path: PathBuf, format: Option<crate::core::format::FileFormat>, cx: &mut Context<Self>) {
        self.recent_file_history.record(path, format);
        self.publish_recent_history(cx);
    }

    pub(crate) fn sync_active_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        let direct_editor = self.active_editor(cx);
        let direct_view = self.active_editor_view(cx);

        if let Some(ref editor) = direct_editor {
            self.last_active_editor.replace(Some(editor.downgrade()));
        }
        if let Some(ref view) = direct_view {
            self.last_active_editor_view.replace(Some(view.downgrade()));
        }

        let effective_editor = self.effective_editor(cx);
        let active_diff_view = self.active_diff_view(cx);
        let pane_tree_is_empty = self.pane_tree.read(cx).is_empty();

        // A split can emit several state events while its new group is being
        // assembled. Only the active editor/diff entity affects these subscribers,
        // so avoid rebuilding every side-panel subscription for duplicate
        // notifications.
        let effective_editor_id = effective_editor.as_ref().map(Entity::entity_id);
        let active_diff_id = active_diff_view.as_ref().map(Entity::entity_id);
        let editor_changed = self.last_active_editor_id.get() != effective_editor_id;
        let diff_changed = self.last_active_diff_id.get() != active_diff_id;
        if !editor_changed && !diff_changed {
            self.sync_native_menus(cx);
            if pane_tree_is_empty {
                self.focus_handle.focus(window, cx);
            }
            return;
        }
        self.last_active_editor_id.set(effective_editor_id);
        self.last_active_diff_id.set(active_diff_id);

        if editor_changed {
            let sub = effective_editor
                .as_ref()
                .map(|editor| cx.observe(editor, |this, _, cx| this.sync_native_menus(cx)));
            self.active_editor_menu_subscription.replace(sub);
        }
        if diff_changed {
            let sub = active_diff_view.as_ref().map(|diff| cx.observe(diff, |this, _, cx| this.sync_native_menus(cx)));
            self.active_diff_menu_subscription.replace(sub);
        }

        self.status_bar.update(cx, |status_bar, cx| {
            if editor_changed {
                status_bar.set_active_editor(effective_editor.clone(), cx);
            }
            if diff_changed {
                status_bar.set_active_diff_view(active_diff_view, cx);
            }
        });
        if editor_changed {
            self.left_panel.update(cx, |panel, cx| {
                panel.set_editor(effective_editor, cx);
            });
        }
        self.on_focus_changed(cx);
        self.sync_native_menus(cx);

        if pane_tree_is_empty {
            self.focus_handle.focus(window, cx);
        }
    }

    pub(crate) fn set_left_panel_visible(&mut self, visible: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_left_panel_visible != visible {
            self.is_left_panel_visible = visible;

            self.workspace_resizable_state.update(cx, |state, _| {
                state.clear();
            });
            self.left_panel_resizable_state.update(cx, |state, _| {
                state.clear();
            });

            if visible {
                let focus_handle = self.left_panel.read(cx).focus_handle(cx);
                focus_handle.focus(window, cx);
            } else {
                self.restore_editor_focus(window, cx);
            }

            self.sync_activity_bar(cx);
            cx.notify();
        }
    }

    pub(crate) fn set_right_panel_visible(&mut self, visible: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_right_panel_visible != visible {
            self.is_right_panel_visible = visible;

            self.workspace_resizable_state.update(cx, |state, _| {
                state.clear();
            });

            if visible {
                self.scratchpad_panel.update(cx, |sp, cx| {
                    sp.focus(window, cx);
                });
            } else {
                self.restore_editor_focus(window, cx);
            }

            self.sync_native_menus(cx);
            cx.notify();
        }
    }

    pub(crate) fn open_editor_view(
        &mut self,
        document: Arc<RwLock<crate::core::document::Document>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EditorView> {
        let default_encoding = *cx.global::<Encoding>();
        let default_radix = *cx.global::<DisplayRadix>();
        let default_group_size = *cx.global::<ByteGroupSize>();
        let default_byte_order = *cx.global::<ByteOrder>();
        let bytes_per_row = cx.global::<crate::core::layout::BytesPerRow>().0;
        let editor = cx.new(|_| {
            let mut editor = Editor::new(document);
            editor.set_encoding(default_encoding);
            editor.set_radix(default_radix);
            editor.set_group_size(default_group_size);
            editor.set_is_big_endian(default_byte_order.is_big_endian());
            editor.set_bytes_per_row(bytes_per_row);
            editor
        });

        let editor_view = cx.new(|cx| EditorView::new(editor, window, cx));
        let content = TabContent::new(editor_view.clone());

        self.pane_tree.update(cx, |tree, cx| {
            tree.open_tab(content, window, cx);
        });

        self.sync_active_editor(window, cx);
        self.update_scratchpad_for_active_file_if_untouched(window, cx);
        cx.notify();
        editor_view
    }

    pub(crate) fn on_action_new_file(&mut self, _: &NewFile, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = cx.entity().clone();
        crate::ui::dialogs::new_file_modal::open_new_file_dialog(window, cx, move |size, fill_byte, window, cx| {
            workspace.update(cx, |this, cx| {
                this.create_new_file(size, fill_byte, window, cx);
            });
        });
    }

    pub(crate) fn on_action_new_empty_file(&mut self, _: &crate::actions::NewEmptyFile, window: &mut Window, cx: &mut Context<Self>) {
        self.create_new_file(0, 0, window, cx);
    }

    fn create_new_file(&mut self, size: usize, fill_byte: u8, window: &mut Window, cx: &mut Context<Self>) {
        self.untitled_count += 1;
        let title = format!("Untitled-{}.bin", self.untitled_count);
        let path = std::path::PathBuf::from(title);
        let data = vec![fill_byte; size];
        let buffer = crate::core::buffer::Buffer::new(data);
        let document = Arc::new(RwLock::new(crate::core::document::Document::new(path, buffer)));

        self.open_editor_view(document, window, cx);
        cx.notify();
    }

    pub(crate) fn open_decompressed_zlib_document(
        &mut self,
        source_path: &std::path::Path,
        address: usize,
        buffer: crate::core::buffer::Buffer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let byte_len = buffer.len();
        let open_paths = self.collect_open_tab_paths(cx);
        let hex_case = cx.try_global::<crate::core::format::HexCase>().copied().unwrap_or_default();
        let path = AppState::global(cx)
            .zlib_service
            .resolve_unique_decompressed_path(source_path, address, hex_case, &open_paths);
        let filename = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "zlib.bin".into());

        let document = Arc::new(RwLock::new(crate::core::document::Document::new(path, buffer)));
        self.open_editor_view(document, window, cx);
        self.restore_editor_focus(window, cx);
        window.push_notification(crate::ui::notification::info(format!("Opened {filename} ({byte_len} B)")), cx);
    }

    fn collect_open_tab_paths(&self, cx: &App) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for group in self.pane_tree.read(cx).all_groups() {
            for tab in &group.read(cx).tabs {
                if let Some(tab_path) = tab.path(cx) {
                    paths.push(tab_path);
                }
            }
        }
        paths
    }

    pub(crate) fn on_action_fill_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active_editor) = self.active_editor(cx) else {
            return;
        };
        let (range_opt, is_read_only) = {
            let ed = active_editor.read(cx);
            (ed.selection_range(), ed.is_read_only())
        };
        if is_read_only {
            return;
        }
        let Some(range) = range_opt else {
            return;
        };
        if range.is_empty() {
            return;
        }

        let workspace = cx.entity().clone();
        let editor_entity = active_editor.clone();
        let target_range = range.clone();
        crate::ui::dialogs::fill_selection_modal::open_fill_selection_dialog(range, window, cx, move |pattern, window, cx| {
            workspace.update(cx, |this, cx| {
                this.execute_fill_selection(&editor_entity, target_range.clone(), &pattern, window, cx);
            });
        });
    }

    pub(crate) fn restore_editor_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor_view) = self.active_editor_view(cx) {
            editor_view.update(cx, |view, cx| {
                view.hex_view().read(cx).focus_handle(cx).focus(window, cx);
            });
        } else if let Some(diff_view) = self.active_diff_view(cx) {
            let handle = diff_view.read(cx).active_side_focus_handle(cx);
            handle.focus(window, cx);
        } else if let Some(scratchpad) = self
            .pane_tree
            .read(cx)
            .active_tab_as::<Entity<crate::ui::views::scratchpad_view::ScratchpadView>>(cx)
        {
            scratchpad.update(cx, |sp, cx| {
                sp.focus(window, cx);
            });
        } else if let Some(editor_view) = self.effective_editor_view(cx) {
            editor_view.update(cx, |view, cx| {
                view.hex_view().read(cx).focus_handle(cx).focus(window, cx);
            });
        } else if let Some(scratchpad) = self.active_scratchpad(cx) {
            scratchpad.update(cx, |sp, cx| {
                sp.focus(window, cx);
            });
        } else {
            self.focus_handle.focus(window, cx);
        }
    }

    /// Closes any active palette and restores focus to the active editor or scratchpad.
    pub(crate) fn close_active_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_palette.take().is_some() {
            self.restore_editor_focus(window, cx);
            cx.notify();
        }
    }

    /// Dismisses active palette without restoring editor focus (used before transitioning to an action/dialog).
    pub(crate) fn dismiss_active_palette(&mut self, cx: &mut Context<Self>) {
        if self.active_palette.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn open_quick_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }

        let current = self.active_palette.as_ref().map(ActivePalette::kind);
        if next_active_palette_kind(current, PaletteKind::QuickOpen).is_none() {
            self.close_active_palette(window, cx);
            return;
        }

        self.active_palette = None;

        use crate::ui::dialogs::quick_open_modal::{QuickOpenCandidate, QuickOpenEvent, QuickOpenModal, collect_workspace_files};

        let mut candidates = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();

        let workspace_root = self.left_panel.read(cx).file_tree.read(cx).root_path().cloned();

        // 1. Recent Files
        for entry in self.recent_file_history.entries() {
            let path = &entry.path;
            if !path.exists() {
                continue;
            }
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if !seen_paths.insert(canonical.clone()) {
                continue;
            }
            seen_paths.insert(path.clone());

            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());

            let detail = if let Some(ref root) = workspace_root {
                if let Ok(rel) = path.strip_prefix(root) {
                    rel.to_string_lossy().to_string()
                } else {
                    path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
                }
            } else {
                path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
            };

            candidates.push(QuickOpenCandidate::RecentFile {
                path: path.clone(),
                file_name,
                detail,
                format: entry.format,
            });
        }

        // 2. Scratchpad Notes (in-memory)
        for scratch in crate::service::ScratchService::list_scratches() {
            candidates.push(QuickOpenCandidate::Note(scratch));
        }

        let is_loading_files = workspace_root.is_some();
        let modal = cx.new(|cx| QuickOpenModal::new(candidates, is_loading_files, window, cx));
        cx.subscribe_in(&modal, window, |this, _, event: &QuickOpenEvent, window, cx| match event {
            QuickOpenEvent::OpenFile { path, format } => {
                let path = path.clone();
                let format = *format;
                this.dismiss_active_palette(cx);
                let action = crate::actions::OpenFile::with_format(path.to_string_lossy().to_string(), format);
                this.on_action_open_file(&action, window, cx);
            }
            QuickOpenEvent::OpenNote(path) => {
                let path = path.clone();
                this.dismiss_active_palette(cx);
                this.open_scratchpad_file(path, window, cx);
            }
            QuickOpenEvent::NewNote(custom_title) => {
                let custom_title = custom_title.clone();
                this.dismiss_active_palette(cx);
                this.create_new_scratchpad(custom_title.as_deref(), window, cx);
            }
            QuickOpenEvent::Cancel => {
                this.close_active_palette(window, cx);
            }
        })
        .detach();

        modal.update(cx, |m, cx| {
            m.focus(window, cx);
        });

        self.active_palette = Some(ActivePalette::QuickOpen(modal.clone()));
        cx.notify();

        // 3. Asynchronously scan workspace files in a background thread to prevent UI micro-freeze
        if let Some(root) = workspace_root {
            let modal_weak = modal.downgrade();
            let seen_recent = seen_paths;
            cx.spawn_in(window, async move |_, window| {
                let files = tokio::task::spawn_blocking(move || collect_workspace_files(&root, 3000))
                    .await
                    .unwrap_or_default();

                let mut ws_candidates = Vec::with_capacity(files.len());
                for (path, rel_path, file_name) in files {
                    if !seen_recent.contains(&path) {
                        ws_candidates.push(QuickOpenCandidate::WorkspaceFile {
                            path,
                            file_name,
                            relative_path: rel_path,
                        });
                    }
                }

                let _ = window.update(|_, cx| {
                    if let Some(modal) = modal_weak.upgrade() {
                        modal.update(cx, |m, cx| {
                            m.append_candidates(ws_candidates, cx);
                        });
                    }
                });
            })
            .detach();
        }
    }

    pub(crate) fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }

        let current = self.active_palette.as_ref().map(ActivePalette::kind);
        if next_active_palette_kind(current, PaletteKind::CommandPalette).is_none() {
            self.close_active_palette(window, cx);
            return;
        }

        self.active_palette = None;

        use crate::ui::dialogs::command_palette_modal::{CommandPaletteEvent, CommandPaletteModal};

        let editor_state = self.menu_editor_state(cx);
        let menus = crate::ui::menus::application_menus();
        let commands = crate::ui::menus::collect_palette_commands(&menus);

        let modal = cx.new(|cx| CommandPaletteModal::new(commands, editor_state, window, cx));

        cx.subscribe_in(&modal, window, |this, _, event: &CommandPaletteEvent, window, cx| match event {
            CommandPaletteEvent::Execute(action_fn) => {
                this.close_active_palette(window, cx);
                let action = (action_fn)();
                window.dispatch_action(action, cx);
            }
            CommandPaletteEvent::Cancel => {
                this.close_active_palette(window, cx);
            }
        })
        .detach();

        modal.update(cx, |m, cx| {
            m.focus(window, cx);
        });

        self.active_palette = Some(ActivePalette::CommandPalette(modal));
        cx.notify();
    }

    fn execute_fill_selection(
        &mut self,
        editor: &Entity<Editor>,
        range: std::ops::Range<usize>,
        pattern: &crate::core::fill::FillPattern,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let len = range.len();
        let bytes = pattern.generate(len);
        let start = range.start;
        let end = range.end;

        editor.update(cx, |ed, cx| {
            if ed.replace_range(start..end, bytes) {
                ed.set_selection_range(start..end);
                cx.notify();
            }
        });

        self.sync_active_editor(window, cx);
        cx.notify();
    }

    pub(crate) fn on_action_quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if self.force_close {
            cx.quit();
            return;
        }

        let all_tabs: Vec<crate::ui::pane::TabItem> = self.pane_tree.read(cx).all_groups().iter().flat_map(|g| g.read(cx).tabs.clone()).collect();

        self.confirm_close_tabs(&all_tabs, window, cx, |workspace, _window, cx| {
            workspace.force_close = true;
            cx.quit();
        });
    }
    fn on_focus_changed(&self, cx: &mut Context<Self>) {
        self.left_panel.update(cx, |panel, cx| {
            panel.file_tree.update(cx, |_, cx| cx.notify());
            panel.strings_panel.update(cx, |_, cx| cx.notify());
            panel.zlib_panel.update(cx, |_, cx| cx.notify());
            panel.struct_tree.update(cx, |_, cx| cx.notify());
            panel.data_inspector.update(cx, |_, cx| cx.notify());
            panel.visual_map.update(cx, |_, cx| cx.notify());
            panel.checksum_panel.update(cx, |_, cx| cx.notify());
        });

        for group in self.pane_tree.read(cx).all_groups() {
            group.update(cx, |_, cx| cx.notify());
        }
    }

    fn sync_activity_bar(&self, cx: &mut Context<Self>) {
        let is_visible = self.is_left_panel_visible;
        let active_tab = self.left_panel.update(cx, |panel, cx| {
            panel.set_panel_visible(is_visible, cx);
            panel.active_tab
        });
        self.activity_bar.update(cx, |activity_bar, cx| {
            if is_visible {
                match active_tab {
                    LeftPanelTab::Files => activity_bar.set_activity(Some(Activity::Files), cx),
                    LeftPanelTab::Search => activity_bar.set_activity(Some(Activity::Search), cx),
                    LeftPanelTab::Strings => activity_bar.set_activity(Some(Activity::Strings), cx),
                    LeftPanelTab::Zlib => activity_bar.set_activity(Some(Activity::Zlib), cx),
                    LeftPanelTab::Structure => activity_bar.set_activity(Some(Activity::Structure), cx),
                    LeftPanelTab::Inspector => activity_bar.set_activity(Some(Activity::Inspector), cx),
                    LeftPanelTab::Map => activity_bar.set_activity(Some(Activity::Map), cx),
                    LeftPanelTab::Checksum => activity_bar.set_activity(Some(Activity::Checksum), cx),
                    LeftPanelTab::Bookmarks => activity_bar.set_activity(Some(Activity::Bookmarks), cx),
                }
            } else {
                activity_bar.set_activity(None, cx);
            }
        });
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
