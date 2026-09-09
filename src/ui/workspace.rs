use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::actions::*;

use crate::ui::components::activity_bar::{Activity, ActivityBar, ActivityBarEvent};
use crate::ui::components::file_tree_view::{FileTreeView, FileTreeViewEvent};
use crate::ui::components::title_bar::AppTitleBar;
use crate::ui::pane::{PaneTree, PaneTreeEvent, TabContent};
use crate::ui::panels::editor_panel::EditorPanel;
use crate::ui::panels::left_panel::{LeftPanel, LeftPanelTab};

use crate::app_state::{AppState, InsertModeState};
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::ui::components::status_bar::StatusBar;
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::component::{Root, WindowExt, v_flex};
use std::cell::Cell;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

mod action_router;
mod dialog_flow;
#[cfg(test)]
mod tests;

pub struct Workspace {
    pub pane_tree: Entity<PaneTree>,
    pub title_bar: Entity<AppTitleBar>,
    pub status_bar: Entity<StatusBar>,
    pub left_panel: Entity<LeftPanel>,
    pub activity_bar: Entity<ActivityBar>,
    pub recent_definition_history: crate::core::structure::DefinitionHistory,
    pub recent_file_history: crate::core::structure::FileHistory,
    pub is_left_panel_visible: bool,
    pub new_file_modal: Option<Entity<crate::ui::components::new_file_modal::NewFileModal>>,
    pub fill_selection_modal: Option<Entity<crate::ui::components::fill_selection_modal::FillSelectionModal>>,
    pub untitled_count: usize,
    pub(crate) force_close: bool,
    focus_handle: FocusHandle,
    last_active_editor_id: Cell<Option<EntityId>>,
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("shift-escape", gpui_kit::component::dock::ToggleZoom, Some("Workspace")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-s", crate::actions::Save, Some("Workspace")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-s", crate::actions::Save, Some("Workspace")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-s", crate::actions::LoadStructureDefinition, Some("Workspace")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-s", crate::actions::LoadStructureDefinition, Some("Workspace")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-v", crate::actions::ToggleInlineStructureView, Some("Workspace")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-v", crate::actions::ToggleInlineStructureView, Some("Workspace")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-\\", crate::actions::SplitRight, Some("Workspace")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-\\", crate::actions::SplitRight, Some("Workspace")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-d", crate::actions::SplitDown, Some("Workspace")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-d", crate::actions::SplitDown, Some("Workspace")),
        KeyBinding::new("insert", crate::actions::ToggleInsertMode, Some("Workspace")),
    ]);

    cx.on_action::<NewFile>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_new_file(&NewFile, window, cx);
        });
    });
    cx.on_action::<crate::actions::FillSelection>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_fill_selection(window, cx);
        });
    });
    cx.on_action::<OpenFile>(|action, cx| {
        let action = action.clone();
        defer_in_active_workspace(cx, move |workspace, window, cx| {
            workspace.on_action_open_file(&action, window, cx);
        });
    });
    cx.on_action::<OpenFileDialog>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_open_file_dialog(&OpenFileDialog, window, cx);
        });
    });
    cx.on_action::<crate::actions::ImportHexOrMot>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_import_hex_or_mot(&crate::actions::ImportHexOrMot, window, cx);
        });
    });
    cx.on_action::<crate::actions::ImportBase64>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_import_base64(&crate::actions::ImportBase64, window, cx);
        });
    });
    cx.on_action::<crate::actions::ExportBase64>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_export_base64(&crate::actions::ExportBase64, window, cx);
        });
    });
    cx.on_action::<crate::actions::ExportMotorolaSrec>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_export_motorola_srec(&crate::actions::ExportMotorolaSrec, window, cx);
        });
    });
    cx.on_action::<crate::actions::ExportIntelHex>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_export_intel_hex(&crate::actions::ExportIntelHex, window, cx);
        });
    });
    cx.on_action::<crate::actions::ExportRawBinary>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_export_raw_binary(&crate::actions::ExportRawBinary, window, cx);
        });
    });
    cx.on_action::<crate::actions::ExportBookmarks>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_export_bookmarks(&crate::actions::ExportBookmarks, window, cx);
        });
    });
    cx.on_action::<crate::actions::ImportBookmarks>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_import_bookmarks(&crate::actions::ImportBookmarks, window, cx);
        });
    });
    cx.on_action::<Save>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_save(&Save, window, cx);
        });
    });
    cx.on_action::<SaveAs>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_save_as(&SaveAs, window, cx);
        });
    });
    cx.on_action::<OpenFolder>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_open_folder(&OpenFolder, window, cx);
        });
    });
    cx.on_action::<LoadStructureDefinition>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_load_structure_definition(&LoadStructureDefinition, window, cx);
        });
    });
    cx.on_action::<crate::actions::LoadStructureDefinitionFromHistory>(|action, cx| {
        let action = action.clone();
        defer_in_active_workspace(cx, move |workspace, window, cx| {
            workspace.on_action_load_structure_definition_from_history(&action, window, cx);
        });
    });
    cx.on_action::<crate::actions::RemoveStructureDefinitionFromHistory>(|action, cx| {
        let action = action.clone();
        defer_in_active_workspace(cx, move |workspace, window, cx| {
            workspace.on_action_remove_structure_definition_from_history(&action, window, cx);
        });
    });
    cx.on_action::<crate::actions::RemoveFileFromHistory>(|action, cx| {
        let action = action.clone();
        defer_in_active_workspace(cx, move |workspace, window, cx| {
            workspace.on_action_remove_file_from_history(&action, window, cx);
        });
    });
    cx.on_action::<ToggleInsertMode>(|_, cx| {
        InsertModeState::toggle(cx);
    });

    cx.activate(true);
}

fn defer_in_active_workspace(cx: &mut App, handler: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static) {
    let Some(window) = cx.active_window() else {
        return;
    };

    cx.defer(move |cx| {
        let Some(window) = window.downcast::<Root>() else {
            return;
        };

        let _ = window.update(cx, |root, window, cx| {
            let Ok(workspace) = root.view().clone().downcast::<Workspace>() else {
                return;
            };

            workspace.update(cx, |workspace, cx| {
                handler(workspace, window, cx);
            });
        });
    });
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pane_tree = cx.new(|_| PaneTree::new());

        cx.subscribe_in(&pane_tree, window, |this, _, event: &PaneTreeEvent, window, cx| match event {
            PaneTreeEvent::ActiveEditorChanged => {
                this.sync_active_editor(window, cx);
            }
        })
        .detach();

        let workspace_weak = cx.entity().downgrade();
        let title_bar = cx.new(|cx| AppTitleBar::new(workspace_weak, window, cx));

        cx.subscribe_in(&title_bar, window, |this, _, event, window, cx| match event {
            crate::ui::components::title_bar::AppTitleBarEvent::OpenSettings => {
                this.open_settings_panel(window, cx);
            }
            crate::ui::components::title_bar::AppTitleBarEvent::OpenAbout => {
                this.open_about_dialog(window, cx);
            }
        })
        .detach();

        let file_tree = cx.new(|cx| FileTreeView::new("FILES", window, cx));
        let left_panel = cx.new(|cx| LeftPanel::new(file_tree.clone(), window, cx));
        let activity_bar = cx.new(ActivityBar::new);

        cx.subscribe_in(&activity_bar, window, |this, _, event: &ActivityBarEvent, window, cx| match event {
            ActivityBarEvent::Select(activity) => {
                this.select_activity(*activity, window, cx);
            }
            ActivityBarEvent::OpenSettings => {
                this.open_settings_panel(window, cx);
            }
        })
        .detach();

        cx.observe(&left_panel, |this, _, cx| {
            this.sync_activity_bar(cx);
        })
        .detach();

        let status_bar = cx.new(StatusBar::new);
        cx.subscribe_in(&status_bar, window, |this, _, event, window, cx| match event {
            crate::ui::components::status_bar::StatusBarEvent::ToggleLeftPanel => {
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
                    left_read.struct_tree.read(cx).focus_handle(cx),
                    left_read.data_inspector.read(cx).focus_handle(cx),
                    left_read.visual_map.read(cx).focus_handle(cx),
                    left_read.checksum_panel.read(cx).focus_handle(cx),
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
            |this, _, event: &crate::ui::components::bookmark_panel::BookmarkPanelEvent, _window, cx| match event {
                crate::ui::components::bookmark_panel::BookmarkPanelEvent::Export => {
                    this.on_action_export_bookmarks(&crate::actions::ExportBookmarks, _window, cx);
                }
                crate::ui::components::bookmark_panel::BookmarkPanelEvent::Import => {
                    this.on_action_import_bookmarks(&crate::actions::ImportBookmarks, _window, cx);
                }
                crate::ui::components::bookmark_panel::BookmarkPanelEvent::NavigateTo { offset, size } => {
                    if let Some(editor_panel) = this.active_editor_panel(cx) {
                        editor_panel.update(cx, |panel, cx| {
                            let len = (*size).max(1);
                            panel.scroll_to_range_if_needed(*offset..offset.saturating_add(len), cx);
                        });
                    }
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &struct_tree,
            window,
            |this, _, event: &crate::ui::components::struct_tree_view::StructTreeViewEvent, _window, cx| match event {
                crate::ui::components::struct_tree_view::StructTreeViewEvent::NavigateTo { offset, size } => {
                    if let Some(editor_panel) = this.active_editor_panel(cx) {
                        editor_panel.update(cx, |panel, cx| {
                            let len = (*size).max(1);
                            panel.scroll_to_range_if_needed(*offset..offset.saturating_add(len), cx);
                        });
                    }
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &left_panel,
            window,
            |this, _, event: &crate::ui::components::search_panel::SearchPanelEvent, window, cx| match event {
                crate::ui::components::search_panel::SearchPanelEvent::NavigateTo { offset, len } => {
                    if let Some(editor_panel) = this.active_editor_panel(cx) {
                        editor_panel.update(cx, |panel, cx| {
                            let match_len = (*len).max(1);
                            panel.scroll_to_range_if_needed(*offset..offset.saturating_add(match_len), cx);
                        });
                    }
                }
                crate::ui::components::search_panel::SearchPanelEvent::FocusEditor => {
                    if let Some(editor_panel) = this.active_editor_panel(cx) {
                        editor_panel.update(cx, |panel, cx| {
                            panel.hex_view().read(cx).focus_handle(cx).focus(window, cx);
                        });
                    }
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &left_panel,
            window,
            |this, _, event: &crate::ui::components::strings_panel::StringsPanelEvent, window, cx| match event {
                crate::ui::components::strings_panel::StringsPanelEvent::NavigateTo { offset, len } => {
                    if let Some(editor_panel) = this.active_editor_panel(cx) {
                        editor_panel.update(cx, |panel, cx| {
                            let match_len = (*len).max(1);
                            panel.scroll_to_range_if_needed(*offset..offset.saturating_add(match_len), cx);
                        });
                    }
                }
                crate::ui::components::strings_panel::StringsPanelEvent::FocusEditor => {
                    if let Some(editor_panel) = this.active_editor_panel(cx) {
                        editor_panel.update(cx, |panel, cx| {
                            panel.hex_view().read(cx).focus_handle(cx).focus(window, cx);
                        });
                    }
                }
            },
        )
        .detach();

        cx.subscribe(&left_panel, |_, _, event: &FileTreeViewEvent, cx| match event {
            FileTreeViewEvent::OpenFile { path, format } => {
                cx.dispatch_action(&crate::actions::OpenFile::with_format(path.to_string_lossy().to_string(), *format));
            }
        })
        .detach();

        let recent_history = cx.global::<crate::settings::RecentHistoryState>().clone();
        let recent_definition_paths = recent_history.definitions.paths().to_vec();
        let recent_file_entries = recent_history.files.entries().to_vec();
        let workspace = Self {
            pane_tree,
            title_bar,
            status_bar,
            left_panel,
            activity_bar,
            recent_definition_history: recent_history.definitions,
            recent_file_history: recent_history.files,
            is_left_panel_visible: true,
            new_file_modal: None,
            fill_selection_modal: None,
            untitled_count: 0,
            force_close: false,
            focus_handle: cx.focus_handle(),
            last_active_editor_id: Cell::new(None),
        };

        workspace.left_panel.update(cx, |panel, cx| {
            panel.set_structure_definition_history(&recent_definition_paths, cx);
            panel.set_file_history(&recent_file_entries, cx);
        });

        workspace
    }

    pub fn active_editor(&self, cx: &App) -> Option<Entity<Editor>> {
        self.pane_tree.read(cx).active_editor(cx)
    }

    pub fn active_editor_panel(&self, cx: &App) -> Option<Entity<EditorPanel>> {
        self.pane_tree.read(cx).active_editor_panel(cx)
    }

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
        let active_editor = self.active_editor(cx);
        let pane_tree_is_empty = self.pane_tree.read(cx).is_empty();

        // A split can emit several state events while its new group is being
        // assembled. Only the active editor entity affects these subscribers,
        // so avoid rebuilding every side-panel subscription for duplicate
        // notifications.
        let active_editor_id = active_editor.as_ref().map(Entity::entity_id);
        if self.last_active_editor_id.get() == active_editor_id {
            if pane_tree_is_empty {
                self.focus_handle.focus(window, cx);
            }
            return;
        }
        self.last_active_editor_id.set(active_editor_id);

        self.status_bar.update(cx, |status_bar, cx| {
            status_bar.set_active_editor(active_editor.clone(), cx);
        });
        self.left_panel.update(cx, |panel, cx| {
            panel.set_editor(active_editor, cx);
        });
        self.on_focus_changed(cx);

        if pane_tree_is_empty {
            self.focus_handle.focus(window, cx);
        }
    }

    pub(crate) fn set_left_panel_visible(&mut self, visible: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.is_left_panel_visible = visible;

        if visible {
            self.left_panel.update(cx, |panel, cx| {
                panel.sync_file_history(cx);
            });
            let focus_handle = self.left_panel.read(cx).focus_handle(cx);
            focus_handle.focus(window, cx);
        } else {
            self.focus_handle.focus(window, cx);
        }

        self.sync_activity_bar(cx);
        cx.notify();
    }

    fn observe_notification(&mut self, notification: &Entity<gpui_kit::component::notification::NotificationList>, cx: &mut Context<Self>) {
        cx.observe(notification, |_, _, cx| {
            cx.notify();
        })
        .detach();
    }

    fn new_local(cx: &mut App) -> Task<anyhow::Result<WindowHandle<Root>>> {
        let mut window_size = size(px(1600.0), px(1200.0));
        if let Some(display) = cx.primary_display() {
            let display_size = display.bounds().size;
            window_size.width = window_size.width.min(display_size.width * 0.85);
            window_size.height = window_size.height.min(display_size.height * 0.85);
        }

        let window_bounds = Bounds::centered(None, window_size, cx);

        cx.spawn(async move |cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                #[cfg(not(target_os = "linux"))]
                titlebar: Some(gpui_kit::component::TitleBar::title_bar_options()),
                window_min_size: Some(Size {
                    width: px(640.),
                    height: px(480.),
                }),
                #[cfg(target_os = "linux")]
                window_background: WindowBackgroundAppearance::Transparent,
                #[cfg(target_os = "linux")]
                window_decorations: Some(WindowDecorations::Client),
                kind: WindowKind::Normal,
                ..Default::default()
            };

            let window = cx.open_window(options, |window, cx| {
                let view = cx.new(|cx| Self::new(window, cx));
                let root = cx.new(|cx| Root::new(view.clone(), window, cx));
                let notification = root.read(cx).notification.clone();
                view.update(cx, |workspace, cx| {
                    workspace.observe_notification(&notification, cx);
                });
                root
            })?;

            window
                .update(cx, |root, window, cx| {
                    window.activate_window();
                    window.set_window_title("XVW");
                    cx.on_release(|_, cx| {
                        cx.quit();
                    })
                    .detach();

                    if let Ok(workspace) = root.view().clone().downcast::<Workspace>() {
                        window.on_window_should_close(cx, move |window, cx| {
                            let (force_close, all_tabs) = {
                                let ws = workspace.read(cx);
                                let all_tabs: Vec<crate::ui::pane::TabItem> =
                                    ws.pane_tree.read(cx).all_groups().iter().flat_map(|g| g.read(cx).tabs.clone()).collect();
                                (ws.force_close, all_tabs)
                            };

                            let dirty_docs = crate::ui::workspace::dialog_flow::collect_dirty_documents(&all_tabs, cx);
                            if force_close || dirty_docs.is_empty() {
                                true
                            } else {
                                workspace.update(cx, |workspace, cx| {
                                    workspace.confirm_close_tabs(&all_tabs, window, cx, |workspace, _window, cx| {
                                        workspace.force_close = true;
                                        cx.quit();
                                    });
                                });
                                false
                            }
                        });
                    }
                })
                .expect("failed to update window");

            Ok(window)
        })
    }

    pub(crate) fn open_editor_panel(&mut self, document: Arc<RwLock<crate::core::document::Document>>, window: &mut Window, cx: &mut Context<Self>) {
        let default_encoding = *cx.global::<Encoding>();
        let bytes_per_row = cx.global::<crate::core::layout::BytesPerRow>().0;
        let editor = cx.new(|_| {
            let mut editor = Editor::new(document);
            editor.set_encoding(default_encoding);
            editor.set_bytes_per_row(bytes_per_row);
            editor
        });

        let editor_panel = cx.new(|cx| EditorPanel::new(editor, window, cx));
        let content = TabContent::from_editor(editor_panel);

        self.pane_tree.update(cx, |tree, cx| {
            tree.open_tab(content, window, cx);
        });

        self.sync_active_editor(window, cx);
        cx.notify();
    }

    pub(crate) fn on_action_new_file(&mut self, _: &NewFile, window: &mut Window, cx: &mut Context<Self>) {
        use crate::ui::components::new_file_modal::{NewFileModal, NewFileModalEvent};

        let modal = cx.new(|cx| NewFileModal::new(window, cx));
        cx.subscribe_in(&modal, window, |this, _, event: &NewFileModalEvent, window, cx| match event {
            NewFileModalEvent::Create { size, fill_byte } => {
                this.create_new_file(*size, *fill_byte, window, cx);
            }
            NewFileModalEvent::Cancel => {
                this.close_new_file_modal(window, cx);
            }
        })
        .detach();

        modal.update(cx, |m, cx| {
            m.focus(window, cx);
        });

        self.new_file_modal = Some(modal);
        cx.notify();
    }

    fn close_new_file_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_file_modal = None;
        self.sync_active_editor(window, cx);
        cx.notify();
    }

    fn create_new_file(&mut self, size: usize, fill_byte: u8, window: &mut Window, cx: &mut Context<Self>) {
        self.new_file_modal = None;
        self.untitled_count += 1;
        let title = format!("Untitled-{}.bin", self.untitled_count);
        let path = std::path::PathBuf::from(title);
        let data = vec![fill_byte; size];
        let buffer = crate::core::buffer::Buffer::new(data);
        let document = Arc::new(RwLock::new(crate::core::document::Document::new(path, buffer)));

        self.open_editor_panel(document, window, cx);
        cx.notify();
    }

    pub(crate) fn on_action_fill_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::ui::components::fill_selection_modal::{FillSelectionModal, FillSelectionModalEvent};

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

        let modal = cx.new(|cx| FillSelectionModal::new(range.clone(), window, cx));
        let editor_entity = active_editor.clone();
        let target_range = range.clone();
        cx.subscribe_in(&modal, window, move |this, _, event: &FillSelectionModalEvent, window, cx| match event {
            FillSelectionModalEvent::Fill(pattern) => {
                this.execute_fill_selection(&editor_entity, target_range.clone(), pattern, window, cx);
            }
            FillSelectionModalEvent::Cancel => {
                this.close_fill_selection_modal(window, cx);
            }
        })
        .detach();

        modal.update(cx, |m, cx| {
            m.focus(window, cx);
        });

        self.fill_selection_modal = Some(modal);
        cx.notify();
    }

    fn close_fill_selection_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fill_selection_modal = None;
        self.sync_active_editor(window, cx);
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
        self.fill_selection_modal = None;
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

    /// Opens a new workspace window with the specified files and folder.
    /// This is the main public API for creating workspace windows.
    pub fn open_window(cx: &mut App, args: crate::CliArgs) -> Task<()> {
        let task = Self::new_local(cx);
        cx.spawn(async move |cx| {
            if let Ok(window) = task.await {
                let _ = window.update(cx, |root, window, cx| {
                    if let Ok(workspace) = root.view().clone().downcast::<Workspace>() {
                        workspace.update(cx, |workspace, cx| {
                            if let Some(folder_path) = args.folder_to_open.clone() {
                                workspace.left_panel.update(cx, |p, cx| {
                                    p.file_tree.update(cx, |ft, cx| {
                                        ft.set_root_path(folder_path, cx);
                                    });
                                });
                            }
                            if let Some((left_path, right_path)) = args.diff.clone() {
                                workspace.on_action_open_diff(
                                    &crate::actions::OpenDiff {
                                        left_path: left_path.to_string_lossy().to_string(),
                                        right_path: right_path.to_string_lossy().to_string(),
                                    },
                                    window,
                                    cx,
                                );
                            }
                        });

                        let view = workspace.clone();
                        let files = args.files_to_open.clone();
                        let ksy_to_load = args.ksy_to_load.clone();
                        let panel_name = args.panel.clone();

                        cx.spawn_in(window, async move |_, window| {
                            let document_service_opt = window.update(|_, cx| AppState::global(cx).document_service.clone()).ok();
                            if let Some(document_service) = document_service_opt {
                                for file_path in files {
                                    let recent_path = file_path.canonicalize().unwrap_or_else(|_| file_path.clone());
                                    match document_service.open_file(file_path.clone()).await {
                                        Ok(document) => {
                                            let _ = window.update(|window, cx| {
                                                view.update(cx, |this, cx| {
                                                    this.record_recent_file(recent_path.clone(), Some(crate::core::format::FileFormat::Binary), cx);
                                                    this.open_editor_panel(document, window, cx);
                                                });
                                            });
                                        }
                                        Err(e) => {
                                            eprintln!("Failed to open file {:?}: {:?}", file_path, e);
                                            let _ = window.update(|window, cx| {
                                                window.push_notification(
                                                    gpui_kit::component::notification::Notification::error(format!("Failed to open file: {e}")),
                                                    cx,
                                                );
                                            });
                                        }
                                    }
                                }

                                if let Some(ksy_path) = ksy_to_load {
                                    let _ = window.update(|window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.load_structure_definition_from_path(ksy_path, window, cx);
                                        });
                                    });
                                }

                                if let Some(panel_name) = panel_name {
                                    let tab = match panel_name.to_lowercase().as_str() {
                                        "files" => Some(LeftPanelTab::Files),
                                        "search" => Some(LeftPanelTab::Search),
                                        "strings" => Some(LeftPanelTab::Strings),
                                        "structure" => Some(LeftPanelTab::Structure),
                                        "inspector" => Some(LeftPanelTab::Inspector),
                                        "map" | "visual_map" => Some(LeftPanelTab::Map),
                                        "checksum" => Some(LeftPanelTab::Checksum),
                                        "bookmarks" => Some(LeftPanelTab::Bookmarks),
                                        _ => None,
                                    };
                                    if let Some(tab) = tab {
                                        let _ = window.update(|window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.left_panel.update(cx, |p, cx| {
                                                    p.set_tab(tab, cx);
                                                });
                                                this.set_left_panel_visible(true, window, cx);
                                            });
                                        });
                                    }
                                }

                                if args.sidebar == Some(false) || (args.diff.is_some() && args.sidebar != Some(true)) {
                                    let _ = window.update(|window, cx| {
                                        view.update(cx, |this, cx| {
                                            this.set_left_panel_visible(false, window, cx);
                                        });
                                    });
                                }
                            }
                        })
                        .detach();
                    }
                });
            }
        })
    }

    fn on_focus_changed(&self, cx: &mut Context<Self>) {
        self.left_panel.update(cx, |panel, cx| {
            panel.file_tree.update(cx, |_, cx| cx.notify());
            panel.strings_panel.update(cx, |_, cx| cx.notify());
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
        let active_tab = self.left_panel.read(cx).active_tab;
        self.activity_bar.update(cx, |activity_bar, cx| {
            if is_visible {
                match active_tab {
                    LeftPanelTab::Files => activity_bar.set_activity(Some(Activity::Files), cx),
                    LeftPanelTab::Search => activity_bar.set_activity(Some(Activity::Search), cx),
                    LeftPanelTab::Strings => activity_bar.set_activity(Some(Activity::Strings), cx),
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

    fn render_bottom_right_notifications(window: &mut Window, cx: &mut App) -> Option<impl IntoElement> {
        let root = window.root::<Root>()??;
        let items = root.read(cx).notification.read(cx).notifications();
        if items.is_empty() {
            return None;
        }
        let items = items.into_iter().rev().take(10).rev();

        Some(
            div()
                .absolute()
                .bottom(px(32.0))
                .right(px(16.0))
                .child(v_flex().id("notification-list-bottom-right").gap_3().children(items)),
        )
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("workspace")
            .on_action(cx.listener(Self::on_action_new_file))
            .on_action(cx.listener(Self::on_action_open_file))
            .on_action(cx.listener(Self::on_action_save))
            .on_action(cx.listener(Self::on_action_save_as))
            .on_action(cx.listener(Self::on_action_toggle_read_only))
            .on_action(cx.listener(Self::on_action_close_active_panel))
            .on_action(cx.listener(Self::on_action_open_file_dialog))
            .on_action(cx.listener(Self::on_action_quit))
            .on_action(cx.listener(Self::on_action_select_all))
            .on_action(cx.listener(Self::on_action_go_to_beginning))
            .on_action(cx.listener(Self::on_action_go_to_end))
            .on_action(cx.listener(Self::on_action_toggle_goto_address))
            .on_action(cx.listener(Self::on_action_toggle_search))
            .on_action(cx.listener(Self::on_action_toggle_search_panel))
            .on_action(cx.listener(Self::on_action_search_next))
            .on_action(cx.listener(Self::on_action_search_prev))
            .on_action(cx.listener(Self::on_action_copy))
            .on_action(cx.listener(Self::on_action_copy_as_hexdump))
            .on_action(cx.listener(Self::on_action_copy_as_cpp_array))
            .on_action(cx.listener(Self::on_action_copy_as_hex_stream))
            .on_action(cx.listener(Self::on_action_copy_as_hex_spaces))
            .on_action(cx.listener(Self::on_action_copy_as_printable_text))
            .on_action(cx.listener(Self::on_action_copy_as_base64))
            .on_action(cx.listener(Self::on_action_copy_as_escaped_string))
            .on_action(cx.listener(Self::on_action_copy_as_binary))
            .on_action(cx.listener(Self::on_action_copy_as_rust_array))
            .on_action(cx.listener(Self::on_action_copy_as_json_array))
            .on_action(cx.listener(Self::on_action_bookmark_red))
            .on_action(cx.listener(Self::on_action_bookmark_orange))
            .on_action(cx.listener(Self::on_action_bookmark_yellow))
            .on_action(cx.listener(Self::on_action_bookmark_green))
            .on_action(cx.listener(Self::on_action_bookmark_cyan))
            .on_action(cx.listener(Self::on_action_bookmark_blue))
            .on_action(cx.listener(Self::on_action_bookmark_purple))
            .on_action(cx.listener(Self::on_action_bookmark_pink))
            .on_action(cx.listener(Self::on_action_clear_bookmark))
            .on_action(cx.listener(Self::on_action_clear_all_bookmarks))
            .on_action(cx.listener(Self::on_action_add_custom_break))
            .on_action(cx.listener(Self::on_action_remove_custom_break_backward))
            .on_action(cx.listener(Self::on_action_remove_custom_break_forward))
            .on_action(cx.listener(Self::on_action_join_line))
            .on_action(cx.listener(Self::on_action_clear_all_custom_breaks))
            .on_action(cx.listener(Self::on_action_show_all_bookmarks))
            .on_action(cx.listener(Self::on_action_hide_all_bookmarks))
            .on_action(cx.listener(Self::on_action_toggle_hide_unbookmarked))
            .on_action(cx.listener(Self::on_action_unfold_bookmark_at_cursor))
            .on_action(cx.listener(Self::on_action_set_encoding))
            .on_action(cx.listener(Self::on_action_set_encoding_ascii))
            .on_action(cx.listener(Self::on_action_set_encoding_utf8))
            .on_action(cx.listener(Self::on_action_set_encoding_utf16le))
            .on_action(cx.listener(Self::on_action_set_encoding_utf16be))
            .on_action(cx.listener(Self::on_action_set_radix_hex))
            .on_action(cx.listener(Self::on_action_set_radix_dec))
            .on_action(cx.listener(Self::on_action_set_radix_oct))
            .on_action(cx.listener(Self::on_action_set_radix_bin))
            .on_action(cx.listener(Self::on_action_set_group_size_1))
            .on_action(cx.listener(Self::on_action_set_group_size_2))
            .on_action(cx.listener(Self::on_action_set_group_size_4))
            .on_action(cx.listener(Self::on_action_set_group_size_8))
            .on_action(cx.listener(Self::on_action_set_byte_order_le))
            .on_action(cx.listener(Self::on_action_set_byte_order_be))
            .on_action(cx.listener(Self::on_action_toggle_byte_order))
            .on_action(cx.listener(Self::on_action_open_diff))
            .on_action(cx.listener(Self::on_action_select_for_compare))
            .on_action(cx.listener(Self::on_action_compare_with_active_file))
            .on_action(cx.listener(Self::on_action_compare_open_files))
            .on_action(cx.listener(Self::on_action_compare_visible_panes))
            .on_action(cx.listener(Self::on_action_toggle_left_panel))
            .on_action(cx.listener(Self::on_action_open_settings))
            .on_action(cx.listener(Self::on_action_open_about))
            .on_action(cx.listener(Self::on_action_open_visual_map))
            .on_action(cx.listener(Self::on_action_show_files_tab))
            .on_action(cx.listener(Self::on_action_show_strings_tab))
            .on_action(cx.listener(Self::on_action_show_structure_tab))
            .on_action(cx.listener(Self::on_action_show_checksum_tab))
            .on_action(cx.listener(Self::on_action_show_bookmarks_tab))
            .on_action(cx.listener(Self::on_action_export_bookmarks))
            .on_action(cx.listener(Self::on_action_import_bookmarks))
            .on_action(cx.listener(Self::on_action_import_hex_or_mot))
            .on_action(cx.listener(Self::on_action_import_base64))
            .on_action(cx.listener(Self::on_action_export_base64))
            .on_action(cx.listener(Self::on_action_export_motorola_srec))
            .on_action(cx.listener(Self::on_action_export_intel_hex))
            .on_action(cx.listener(Self::on_action_export_raw_binary))
            .on_action(cx.listener(Self::on_action_load_structure_definition))
            .on_action(cx.listener(Self::on_action_load_structure_definition_from_history))
            .on_action(cx.listener(Self::on_action_remove_structure_definition_from_history))
            .on_action(cx.listener(Self::on_action_remove_file_from_history))
            .on_action(cx.listener(Self::on_action_clear_structure_definition))
            .on_action(cx.listener(Self::on_action_toggle_inline_structure_view))
            .on_action(cx.listener(Self::on_action_open_folder))
            .on_action(cx.listener(Self::on_action_close_folder))
            .on_action(cx.listener(Self::on_action_activate_next_tab))
            .on_action(cx.listener(Self::on_action_activate_previous_tab))
            .on_action(cx.listener(Self::on_action_activate_tab))
            .on_action(cx.listener(Self::on_action_close_other_tabs))
            .on_action(cx.listener(Self::on_action_close_tabs_to_right))
            .on_action(cx.listener(Self::on_action_close_saved_tabs))
            .on_action(cx.listener(Self::on_action_close_all_tabs))
            .on_action(cx.listener(Self::on_action_copy_path))
            .on_action(cx.listener(Self::on_action_copy_file_name))
            .on_action(cx.listener(Self::on_action_reveal_in_explorer))
            .on_action(cx.listener(Self::on_action_split_right))
            .on_action(cx.listener(Self::on_action_split_down))
            .on_drop(cx.listener(move |this, external_paths: &ExternalPaths, window, cx| {
                for path in external_paths.paths() {
                    if path.is_file() {
                        this.left_panel.update(cx, |panel, cx| {
                            panel.sync_file_history(cx);
                        });
                        let action = crate::actions::OpenFile::new(path.to_string_lossy().to_string());
                        this.on_action_open_file(&action, window, cx);
                    } else if path.is_dir() {
                        this.left_panel.update(cx, |p, cx| {
                            p.set_tab(crate::ui::panels::left_panel::LeftPanelTab::Files, cx);
                            p.file_tree.update(cx, |ft, cx| {
                                ft.set_root_path(path.clone(), cx);
                            });
                        });
                        this.set_left_panel_visible(true, window, cx);
                    }
                }
                cx.notify();
            }))
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(self.title_bar.clone())
            .child(
                div()
                    .track_focus(&self.focus_handle)
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(self.activity_bar.clone())
                    .child(
                        div().flex().flex_col().flex_1().size_full().min_w_0().min_h_0().overflow_hidden().child(
                            h_resizable("workspace-h-resize")
                                .child(
                                    resizable_panel()
                                        .visible(self.is_left_panel_visible)
                                        .size(px(250.))
                                        .child(div().size_full().min_w_0().min_h_0().overflow_hidden().child(self.left_panel.clone())),
                                )
                                .child(
                                    resizable_panel().child(div().relative().size_full().min_w_0().min_h_0().overflow_hidden().child(self.pane_tree.clone())),
                                ),
                        ),
                    ),
            )
            .child(self.status_bar.clone())
            .when_some(self.new_file_modal.clone(), |el, modal| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x00000080))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .child(modal),
                )
            })
            .when_some(self.fill_selection_modal.clone(), |el, modal| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x00000080))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .child(modal),
                )
            })
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Self::render_bottom_right_notifications(window, cx))
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
