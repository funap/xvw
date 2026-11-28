use super::Workspace;
use crate::actions::*;
use crate::app_state::{AppState, InsertModeState};
use crate::ui::panels::left_panel::LeftPanelTab;
use gpui_kit::component::{Root, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::*;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("shift-escape", gpui_kit::component::dock::ToggleZoom, Some("Workspace")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-k", crate::actions::LoadStructureDefinition, Some("Workspace")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-k", crate::actions::LoadStructureDefinition, Some("Workspace")),
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
        KeyBinding::new("insert", crate::actions::ToggleInsertMode, Some("Workspace && !Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-cmd-i", crate::actions::ToggleInsertMode, Some("Workspace && !Input")),
    ]);

    cx.on_action::<NewFile>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_new_file(&NewFile, window, cx);
        });
    });
    cx.on_action::<crate::actions::NewEmptyFile>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_new_empty_file(&crate::actions::NewEmptyFile, window, cx);
        });
    });
    cx.on_action::<crate::actions::ToggleRightPanel>(|action, cx| {
        let action = action.clone();
        defer_in_active_workspace(cx, move |workspace, window, cx| {
            workspace.on_action_toggle_right_panel(&action, window, cx);
        });
    });
    cx.on_action::<crate::actions::NewScratchpad>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_new_scratchpad(&crate::actions::NewScratchpad, window, cx);
        });
    });
    cx.on_action::<crate::actions::OpenScratchpadDialog>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_open_scratchpad_dialog(&crate::actions::OpenScratchpadDialog, window, cx);
        });
    });
    cx.on_action::<crate::actions::OpenScratchpadFile>(|action, cx| {
        let action = action.clone();
        defer_in_active_workspace(cx, move |workspace, window, cx| {
            workspace.on_action_open_scratchpad_file(&action, window, cx);
        });
    });
    cx.on_action::<crate::actions::RevealScratchesInExplorer>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_reveal_scratches_in_explorer(&crate::actions::RevealScratchesInExplorer, window, cx);
        });
    });
    cx.on_action::<crate::actions::ExportScratchpadAs>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_export_scratchpad_as(&crate::actions::ExportScratchpadAs, window, cx);
        });
    });
    cx.on_action::<crate::actions::DeleteCurrentScratchpad>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_delete_current_scratchpad(&crate::actions::DeleteCurrentScratchpad, window, cx);
        });
    });
    cx.on_action::<crate::actions::QuickOpen>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_quick_open(&crate::actions::QuickOpen, window, cx);
        });
    });
    cx.on_action::<crate::actions::CommandPalette>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_command_palette(&crate::actions::CommandPalette, window, cx);
        });
    });
    cx.on_action::<crate::actions::OpenScratchpadPalette>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_open_scratchpad_palette(&crate::actions::OpenScratchpadPalette, window, cx);
        });
    });
    cx.on_action::<crate::actions::ToggleScratchpadMode>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_toggle_scratchpad_mode(&crate::actions::ToggleScratchpadMode, window, cx);
        });
    });
    cx.on_action::<crate::actions::NavigateToOffset>(|action, cx| {
        let action = action.clone();
        defer_in_active_workspace(cx, move |workspace, window, cx| {
            workspace.on_action_navigate_to_offset(&action, window, cx);
        });
    });
    cx.on_action::<crate::actions::InsertActiveOffsetOnly>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_insert_active_offset_only(&crate::actions::InsertActiveOffsetOnly, window, cx);
        });
    });
    cx.on_action::<crate::actions::InsertActiveText>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_insert_active_text(&crate::actions::InsertActiveText, window, cx);
        });
    });
    cx.on_action::<crate::actions::InsertActiveHexBytes>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_insert_active_hex_bytes(&crate::actions::InsertActiveHexBytes, window, cx);
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
    cx.on_action::<CloseFolder>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_close_folder(&CloseFolder, window, cx);
        });
    });
    cx.on_action::<CloseActivePanel>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_close_active_panel(&CloseActivePanel, window, cx);
        });
    });
    cx.on_action::<OpenSettings>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_open_settings(&OpenSettings, window, cx);
        });
    });
    cx.on_action::<OpenAbout>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_open_about(&OpenAbout, window, cx);
        });
    });
    cx.on_action::<Quit>(|_, cx| {
        if cx.active_window().is_some() {
            defer_in_active_workspace(cx, |workspace, window, cx| {
                workspace.on_action_quit(&Quit, window, cx);
            });
        } else {
            cx.quit();
        }
    });
    cx.on_action::<ToggleLeftPanel>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_toggle_left_panel(&ToggleLeftPanel, window, cx);
        });
    });
    cx.on_action::<ToggleSearchPanel>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_toggle_search_panel(&ToggleSearchPanel, window, cx);
        });
    });
    cx.on_action::<SplitRight>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_split_right(&SplitRight, window, cx);
        });
    });
    cx.on_action::<SplitDown>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_split_down(&SplitDown, window, cx);
        });
    });
    cx.on_action::<ActivateNextTab>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_activate_next_tab(&ActivateNextTab, window, cx);
        });
    });
    cx.on_action::<ActivatePreviousTab>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_activate_previous_tab(&ActivatePreviousTab, window, cx);
        });
    });
    cx.on_action::<LoadStructureDefinition>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_load_structure_definition(&LoadStructureDefinition, window, cx);
        });
    });
    cx.on_action::<ToggleInlineStructureView>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_toggle_inline_structure_view(&ToggleInlineStructureView, window, cx);
        });
    });
    cx.on_action::<ExpandAllStructure>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_expand_all_structure(&ExpandAllStructure, window, cx);
        });
    });
    cx.on_action::<CollapseAllStructure>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_collapse_all_structure(&CollapseAllStructure, window, cx);
        });
    });
    cx.on_action::<CompareOpenFiles>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_compare_open_files(&CompareOpenFiles, window, cx);
        });
    });
    cx.on_action::<CompareVisiblePanes>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_compare_visible_panes(&CompareVisiblePanes, window, cx);
        });
    });
    cx.on_action::<SwapDiffFiles>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_swap_diff_files(&SwapDiffFiles, window, cx);
        });
    });
    cx.on_action::<RefreshDiff>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_refresh_diff(&RefreshDiff, window, cx);
        });
    });
    cx.on_action::<NextDifference>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_next_difference(&NextDifference, window, cx);
        });
    });
    cx.on_action::<PrevDifference>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_prev_difference(&PrevDifference, window, cx);
        });
    });
    cx.on_action::<ToggleSyncScroll>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_toggle_sync_scroll(&ToggleSyncScroll, window, cx);
        });
    });
    cx.on_action::<OpenVisualMap>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_open_visual_map(&OpenVisualMap, window, cx);
        });
    });
    cx.on_action::<ShowFilesTab>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_show_files_tab(&ShowFilesTab, window, cx);
        });
    });
    cx.on_action::<ShowSearchTab>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_show_search_tab(&ShowSearchTab, window, cx);
        });
    });
    cx.on_action::<ShowStringsTab>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_show_strings_tab(&ShowStringsTab, window, cx);
        });
    });
    cx.on_action::<ShowZlibTab>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_show_zlib_tab(&ShowZlibTab, window, cx);
        });
    });
    cx.on_action::<ShowStructureTab>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_show_structure_tab(&ShowStructureTab, window, cx);
        });
    });
    cx.on_action::<ShowInspectorTab>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_show_inspector_tab(&ShowInspectorTab, window, cx);
        });
    });
    cx.on_action::<ShowChecksumTab>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_show_checksum_tab(&ShowChecksumTab, window, cx);
        });
    });
    cx.on_action::<ShowBookmarksTab>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_show_bookmarks_tab(&ShowBookmarksTab, window, cx);
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
    cx.on_action::<ZoomIn>(|_, cx| {
        cx.update_global::<crate::core::appearance::Appearance, _>(|appearance, _| {
            appearance.font_size = (appearance.font_size + 1.0).min(crate::core::appearance::MAX_FONT_SIZE);
        });
        crate::settings::save_current(cx);
    });
    cx.on_action::<ZoomOut>(|_, cx| {
        cx.update_global::<crate::core::appearance::Appearance, _>(|appearance, _| {
            appearance.font_size = (appearance.font_size - 1.0).max(crate::core::appearance::MIN_FONT_SIZE);
        });
        crate::settings::save_current(cx);
    });
    cx.on_action::<ResetZoom>(|_, cx| {
        let default_size = crate::core::appearance::Appearance::default().font_size;
        cx.update_global::<crate::core::appearance::Appearance, _>(|appearance, _| {
            appearance.font_size = default_size;
        });
        crate::settings::save_current(cx);
    });
    cx.on_action::<NavigateBack>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_navigate_back(&NavigateBack, window, cx);
        });
    });
    cx.on_action::<NavigateForward>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_navigate_forward(&NavigateForward, window, cx);
        });
    });
    cx.on_action::<ToggleEditColumn>(|_, cx| {
        defer_in_active_workspace(cx, |workspace, window, cx| {
            workspace.on_action_toggle_edit_column(&ToggleEditColumn, window, cx);
        });
    });

    cx.activate(true);
}

fn defer_in_active_workspace(cx: &mut App, handler: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static) {
    let Some(window) = cx.active_window() else {
        return;
    };

    cx.defer(move |cx| {
        let _ = cx.update_window(window, |root_view, window, cx| {
            let Ok(root) = root_view.downcast::<Root>() else {
                return;
            };
            let Ok(workspace) = root.read(cx).view().clone().downcast::<Workspace>() else {
                return;
            };

            workspace.update(cx, |workspace, cx| {
                handler(workspace, window, cx);
            });
        });
    });
}

impl Workspace {
    fn new_local(cx: &mut App) -> Task<anyhow::Result<(WindowHandle<Root>, Entity<Workspace>)>> {
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

            let (window_handle, workspace) = cx.update(|cx| gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Self::new(window, cx))))?;

            let window: WindowHandle<Root> = window_handle
                .downcast::<Root>()
                .ok_or_else(|| anyhow::anyhow!("failed to downcast Root window handle"))?;

            window
                .update(cx, |_root, window, cx| {
                    window.activate_window();
                    window.set_window_title("XVW");
                    cx.on_release(|_, cx| {
                        cx.quit();
                    })
                    .detach();

                    let ws = workspace.clone();
                    window.on_window_should_close(cx, move |window, cx| {
                        let (force_close, all_tabs) = {
                            let ws_ref = ws.read(cx);
                            let all_tabs: Vec<crate::ui::pane::TabItem> =
                                ws_ref.pane_tree.read(cx).all_groups().iter().flat_map(|g| g.read(cx).tabs.clone()).collect();
                            (ws_ref.force_close, all_tabs)
                        };

                        let dirty_docs = crate::ui::workspace::dialog_flow::collect_dirty_documents(&all_tabs, cx);
                        if force_close || dirty_docs.is_empty() {
                            true
                        } else {
                            ws.update(cx, |workspace, cx| {
                                workspace.confirm_close_tabs(&all_tabs, window, cx, |workspace, _window, cx| {
                                    workspace.force_close = true;
                                    cx.quit();
                                });
                            });
                            false
                        }
                    });
                })
                .map_err(|e| anyhow::anyhow!("failed to update window: {e}"))?;

            Ok((window, workspace))
        })
    }

    /// Opens a new workspace window with the specified files and folder.
    /// This is the main public API for creating workspace windows.
    pub fn open_window(cx: &mut App, args: crate::CliArgs) -> Task<()> {
        let task = Self::new_local(cx);
        cx.spawn(async move |cx| match task.await {
            Ok((window, workspace)) => {
                let _ = window.update(cx, |_root, window, cx| {
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
                                                this.open_editor_view(document, window, cx);
                                            });
                                        });
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to open file {:?}: {:?}", file_path, e);
                                        let _ = window.update(|window, cx| {
                                            window.push_notification(crate::ui::notification::error(format!("Failed to open file: {e}")), cx);
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
                                    "zlib" => Some(LeftPanelTab::Zlib),
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
                });
            }
            Err(e) => {
                eprintln!("Failed to open workspace window: {e:?}");
            }
        })
    }
}
