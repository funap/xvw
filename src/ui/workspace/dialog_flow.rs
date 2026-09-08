use gpui_kit::prelude::*;
use gpui_kit::*;
use std::path::PathBuf;
use std::sync::Arc;

use super::Workspace;
use crate::actions::*;
use crate::app_state::AppState;
use crate::core::editor::Editor;
use crate::ui::pane::TabContent;
use gpui_kit::component::WindowExt;

impl Workspace {
    pub(crate) fn on_action_open_file_dialog(&mut self, _: &OpenFileDialog, window: &mut Window, cx: &mut Context<Self>) {
        let path = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select a file".into()),
        });

        let view = cx.entity();
        cx.spawn_in(window, async move |_, window| {
            if let Some(path) = path.await.ok().and_then(|r| r.ok()).flatten().and_then(|mut v| v.pop()) {
                window
                    .update(|window, cx| {
                        view.update(cx, |this, cx| {
                            this.left_panel.update(cx, |panel, cx| {
                                panel.sync_file_history(cx);
                            });
                            let action = crate::actions::OpenFile::new(path.to_string_lossy().to_string());
                            this.on_action_open_file(&action, window, cx);
                        });
                    })
                    .ok();
            }
        })
        .detach();
    }

    pub(crate) fn on_action_open_file(&mut self, action: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let file_path = action.path.clone();
        let path = std::path::PathBuf::from(&file_path);
        let path = path.canonicalize().unwrap_or(path);

        // Check if path is already open in any group with matching format
        let requested_format = action.format.unwrap_or(crate::core::format::FileFormat::Binary);
        for group in self.pane_tree.read(cx).all_groups() {
            let tabs = group
                .read(cx)
                .tabs
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let doc_format = t.content.document(cx).and_then(|d| d.read().ok().map(|doc| doc.format));
                    (i, t.path(cx), doc_format)
                })
                .collect::<Vec<_>>();
            for (idx, tab_path, tab_format) in tabs {
                let is_same_file = tab_path
                    .as_ref()
                    .is_some_and(|tab_path| tab_path.canonicalize().unwrap_or_else(|_| tab_path.clone()) == path);
                let is_matching_format = action.format.is_none()
                    || tab_format.is_none()
                    || tab_format == Some(requested_format)
                    || (requested_format == crate::core::format::FileFormat::HexOrMot && tab_format.is_some_and(|f| f.is_import()));
                if is_same_file && is_matching_format {
                    group.update(cx, |g, cx| {
                        g.activate_tab(idx, window, cx);
                    });
                    self.pane_tree.update(cx, |tree, cx| {
                        tree.set_active_group(group.read(cx).id, cx);
                    });
                    self.sync_active_editor(window, cx);
                    self.record_recent_file(path.clone(), action.format, cx);
                    cx.notify();
                    return;
                }
            }
        }

        if let Some(format) = action.format
            && format.is_import()
        {
            self.import_file_from_path(path, Some(format), window, cx);
            return;
        }

        let view = cx.entity();
        let recent_path = path.clone();
        cx.spawn_in(window, async move |_, window| {
            let document_service_opt = window.update(|_, cx| AppState::global(cx).document_service.clone()).ok();

            if let Some(document_service) = document_service_opt {
                match document_service.open_file(std::path::PathBuf::from(&file_path)).await {
                    Ok(document) => {
                        window
                            .update(|window, cx| {
                                view.update(cx, |this, cx| {
                                    this.record_recent_file(recent_path.clone(), Some(crate::core::format::FileFormat::Binary), cx);
                                    this.open_editor_panel(document, window, cx);
                                });
                            })
                            .ok();
                    }
                    Err(e) => {
                        eprintln!("Failed to open file: {:?}", e);
                        let _ = window.update(|window, cx| {
                            window.push_notification(gpui_kit::component::notification::Notification::error(format!("Failed to open file: {e}")), cx);
                        });
                    }
                }
            }
        })
        .detach();
    }

    pub(crate) fn on_action_open_diff(&mut self, action: &OpenDiff, window: &mut Window, cx: &mut Context<Self>) {
        let left_path = action.left_path.clone();
        let right_path = action.right_path.clone();

        cx.spawn_in(window, async move |this, window| {
            let app = this.update(window, |_, cx| AppState::global(cx).clone()).expect("AppState global");

            if let Some(workspace) = this.upgrade() {
                let left_result = app.document_service.open_file(std::path::PathBuf::from(left_path)).await;
                let right_result = app.document_service.open_file(std::path::PathBuf::from(right_path)).await;

                match (left_result, right_result) {
                    (Ok(left_document), Ok(right_document)) => {
                        let left_recent_path = left_document.read().ok().map(|document| document.path().to_path_buf());
                        let right_recent_path = right_document.read().ok().map(|document| document.path().to_path_buf());
                        let _ = workspace.update_in(window, |workspace_view, window, cx| {
                            if let Some(path) = left_recent_path {
                                workspace_view.record_recent_file(path, Some(crate::core::format::FileFormat::Binary), cx);
                            }
                            if let Some(path) = right_recent_path {
                                workspace_view.record_recent_file(path, Some(crate::core::format::FileFormat::Binary), cx);
                            }

                            let app = AppState::global(cx).clone();
                            let diff_result_task = app.diff_service.compute_diff(left_document.clone(), right_document.clone(), cx);

                            cx.spawn_in(window, async move |workspace, window| {
                                let diff_result = diff_result_task.await;

                                let _ = workspace.update_in(window, |workspace_view, window, cx| {
                                    use crate::ui::panels::diff_panel::DiffPanel;
                                    let diff_view = cx.new(|cx| {
                                        let mut view = DiffPanel::new(left_document.clone(), right_document.clone(), window, cx);
                                        view.set_diff_result(diff_result.clone(), cx);
                                        view
                                    });

                                    let content = TabContent::from_diff(diff_view);
                                    workspace_view.pane_tree.update(cx, |tree, cx| {
                                        tree.open_tab(content, window, cx);
                                    });
                                    workspace_view.sync_active_editor(window, cx);
                                    cx.notify();
                                });
                            })
                            .detach();
                        });
                    }
                    (Err(e), _) => {
                        eprintln!("Failed to open left diff file: {:?}", e);
                        let _ = window.update(|window, cx| {
                            window.push_notification(
                                gpui_kit::component::notification::Notification::error(format!("Failed to open diff file: {e}")),
                                cx,
                            );
                        });
                    }
                    (_, Err(e)) => {
                        eprintln!("Failed to open right diff file: {:?}", e);
                        let _ = window.update(|window, cx| {
                            window.push_notification(
                                gpui_kit::component::notification::Notification::error(format!("Failed to open diff file: {e}")),
                                cx,
                            );
                        });
                    }
                }
            }
        })
        .detach();
    }

    pub(crate) fn on_action_select_for_compare(&mut self, action: &SelectForCompare, _window: &mut Window, cx: &mut Context<Self>) {
        crate::app_state::PendingCompareState::set(Some(action.path.clone()), cx);
        self.left_panel.update(cx, |panel, cx| {
            panel.file_tree.update(cx, |file_tree, cx| {
                file_tree.pending_compare_path = Some(action.path.clone());
                cx.notify();
            });
        });
    }

    pub(crate) fn on_action_compare_with_active_file(&mut self, action: &CompareWithActiveFile, window: &mut Window, cx: &mut Context<Self>) {
        let active_path = self.active_editor(cx).and_then(|ed| {
            let doc = ed.read(cx).document.read().ok()?;
            Some(doc.path().to_path_buf())
        });
        if let Some(active_p) = active_path {
            let left_path = active_p.to_string_lossy().to_string();
            let right_path = action.path.clone();
            if left_path != right_path {
                self.on_action_open_diff(&OpenDiff { left_path, right_path }, window, cx);
            }
        }
    }

    pub(crate) fn on_action_compare_open_files(&mut self, _: &CompareOpenFiles, window: &mut Window, cx: &mut Context<Self>) {
        let mut open_paths = Vec::new();
        for group in self.pane_tree.read(cx).all_groups() {
            for tab in group.read(cx).tabs() {
                if let Some(p) = tab.path(cx)
                    && !open_paths.contains(&p)
                {
                    open_paths.push(p);
                }
            }
        }

        if open_paths.len() >= 2 {
            let active_path = self.active_editor(cx).and_then(|ed| {
                let doc = ed.read(cx).document.read().ok()?;
                Some(doc.path().to_path_buf())
            });

            let (left, right) = if let Some(active_p) = active_path {
                let other = open_paths.iter().find(|p| *p != &active_p).unwrap_or(&open_paths[0]);
                (active_p, other.clone())
            } else {
                (open_paths[0].clone(), open_paths[1].clone())
            };

            self.on_action_open_diff(
                &OpenDiff {
                    left_path: left.to_string_lossy().to_string(),
                    right_path: right.to_string_lossy().to_string(),
                },
                window,
                cx,
            );
        } else if open_paths.len() == 1 {
            let left_path = open_paths[0].clone();
            let prompt_path = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some("Select second file to compare with".into()),
            });

            let view = cx.entity();
            cx.spawn_in(window, async move |_, window| {
                if let Some(right_path) = prompt_path.await.ok().and_then(|r| r.ok()).flatten().and_then(|mut v| v.pop()) {
                    window
                        .update(|window, cx| {
                            view.update(cx, |this, cx| {
                                this.on_action_open_diff(
                                    &OpenDiff {
                                        left_path: left_path.to_string_lossy().to_string(),
                                        right_path: right_path.to_string_lossy().to_string(),
                                    },
                                    window,
                                    cx,
                                );
                            });
                        })
                        .ok();
                }
            })
            .detach();
        }
    }

    pub(crate) fn on_action_compare_visible_panes(&mut self, _: &CompareVisiblePanes, window: &mut Window, cx: &mut Context<Self>) {
        let groups = self.pane_tree.read(cx).all_groups();
        if groups.len() >= 2 {
            let g0_path = groups[0].read(cx).active_tab().and_then(|t| t.path(cx));
            let g1_path = groups[1].read(cx).active_tab().and_then(|t| t.path(cx));
            if let (Some(left), Some(right)) = (g0_path, g1_path)
                && left != right
            {
                self.on_action_open_diff(
                    &OpenDiff {
                        left_path: left.to_string_lossy().to_string(),
                        right_path: right.to_string_lossy().to_string(),
                    },
                    window,
                    cx,
                );
            }
        }
    }

    pub(crate) fn on_action_export_bookmarks(&mut self, _: &ExportBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor(cx) else { return };
        let (parent_dir, target_file_name) = {
            let doc_lock = editor.read(cx).document.read().ok();
            let parent = doc_lock
                .as_ref()
                .and_then(|d| d.path().parent().filter(|p| p.exists()).map(|p| p.to_path_buf()));
            let file_name = doc_lock.as_ref().and_then(|d| d.path().file_name().map(|n| n.to_string_lossy().into_owned()));
            (parent, file_name)
        };
        let parent_dir = parent_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        let default_name = if let Some(name) = target_file_name
            && !name.is_empty()
            && name != "Untitled"
            && name != "untitled"
        {
            format!("{name}.bookmark.yaml")
        } else {
            "bookmarks.bookmark.yaml".to_string()
        };

        let prompt_path = cx.prompt_for_new_path(&parent_dir, Some(&default_name));

        let view = cx.entity().clone();
        cx.spawn_in(window, async move |_, window| {
            if let Some(mut path) = prompt_path.await.ok().and_then(|r| r.ok()).flatten() {
                if path.extension().is_none() {
                    path.set_extension("yaml");
                }

                window
                    .update(|_, cx| {
                        view.update(cx, |this, cx| {
                            if let Some(editor) = this.active_editor(cx)
                                && let Err(e) = editor.read(cx).bookmarks().export_to_file(&path)
                            {
                                eprintln!("Failed to export bookmarks: {}", e);
                            }
                        });
                    })
                    .ok();
            }
        })
        .detach();
    }

    pub(crate) fn on_action_import_bookmarks(&mut self, _: &ImportBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        let prompt_path = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select bookmarks YAML file to import".into()),
        });

        let view = cx.entity().clone();
        cx.spawn_in(window, async move |_, window| {
            if let Some(path) = prompt_path.await.ok().and_then(|r| r.ok()).flatten().and_then(|mut v| v.pop()) {
                window
                    .update(|_, cx| {
                        view.update(cx, |this, cx| {
                            if let Some(editor) = this.active_editor(cx) {
                                let doc_path = editor.read(cx).document.read().ok().map(|d| d.path().to_path_buf());
                                editor.update(cx, |ed, cx| match ed.bookmarks_mut().import_from_file(&path) {
                                    Ok(_) => {
                                        cx.notify();
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to import bookmarks: {}", e);
                                    }
                                });
                                if let Some(ref p) = doc_path {
                                    let service = crate::app_state::AppState::global(cx).document_service.clone();
                                    service.notify_document_changed(p, cx);
                                }
                            }
                        });
                    })
                    .ok();
            }
        })
        .detach();
    }

    pub(crate) fn import_file_from_path(
        &mut self,
        path: PathBuf,
        format: Option<crate::core::format::FileFormat>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().clone();
        cx.spawn_in(window, async move |_, window| {
            let content_res = tokio::fs::read_to_string(&path).await;
            match content_res {
                Ok(content) => {
                    if format == Some(crate::core::format::FileFormat::Base64) {
                        match crate::core::format::parse_base64(&content) {
                            Ok(data) => {
                                window
                                    .update(|window, cx| {
                                        view.update(cx, |this, cx| {
                                            let canonical_path = path.canonicalize().unwrap_or_else(|_| path.clone());
                                            let detected_format = crate::core::format::FileFormat::Base64;
                                            this.record_recent_file(canonical_path.clone(), Some(detected_format), cx);
                                            let buffer = crate::core::buffer::Buffer::new(data);
                                            let doc = crate::core::document::Document::new_read_only(canonical_path, buffer).with_format(detected_format);
                                            let doc_arc = std::sync::Arc::new(std::sync::RwLock::new(doc));
                                            this.open_editor_panel(doc_arc, window, cx);
                                            window.push_notification(gpui_kit::component::notification::Notification::info("Imported Base64 successfully"), cx);
                                        });
                                    })
                                    .ok();
                            }
                            Err(e) => {
                                let _ = window.update(|window, cx| {
                                    window.push_notification(
                                        gpui_kit::component::notification::Notification::error(format!("Failed to parse Base64 file: {}", e)),
                                        cx,
                                    );
                                });
                            }
                        }
                        return;
                    }

                    let parse_result = match format {
                        Some(crate::core::format::FileFormat::IntelHex) => crate::core::hex_import::parse_intel_hex(&content),
                        Some(crate::core::format::FileFormat::MotorolaSrec) => crate::core::hex_import::parse_motorola_srec(&content),
                        _ => crate::core::hex_import::parse_hex_or_mot(&content),
                    };

                    match parse_result {
                        Ok(import_result) => {
                            window
                                .update(|window, cx| {
                                    view.update(cx, |this, cx| {
                                        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.clone());
                                        let detected_format = crate::core::format::FileFormat::from(import_result.format);
                                        this.record_recent_file(canonical_path.clone(), Some(detected_format), cx);
                                        let buffer = crate::core::buffer::Buffer::new(import_result.data);
                                        let doc = crate::core::document::Document::new_read_only(canonical_path, buffer)
                                            .with_address_map(import_result.address_map.clone())
                                            .with_format(detected_format);
                                        let doc_arc = std::sync::Arc::new(std::sync::RwLock::new(doc));
                                        this.open_editor_panel(doc_arc, window, cx);
                                        let gap_msg = if import_result.address_map.has_gaps() {
                                            format!(" ({} segments with address gaps)", import_result.address_map.segments.len())
                                        } else {
                                            String::new()
                                        };
                                        window.push_notification(
                                            gpui_kit::component::notification::Notification::info(format!(
                                                "Imported {} successfully{}",
                                                import_result.format.label(),
                                                gap_msg
                                            )),
                                            cx,
                                        );
                                    });
                                })
                                .ok();
                        }
                        Err(e) => {
                            let _ = window.update(|window, cx| {
                                window.push_notification(
                                    gpui_kit::component::notification::Notification::error(format!("Failed to parse hex/mot file: {}", e)),
                                    cx,
                                );
                            });
                        }
                    }
                }
                Err(e) => {
                    let _ = window.update(|window, cx| {
                        window.push_notification(
                            gpui_kit::component::notification::Notification::error(format!("Failed to read file: {}", e)),
                            cx,
                        );
                    });
                }
            }
        })
        .detach();
    }

    pub(crate) fn on_action_import_hex_or_mot(&mut self, _: &crate::actions::ImportHexOrMot, window: &mut Window, cx: &mut Context<Self>) {
        let prompt_path = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Motorola S-Record or Intel HEX file to import".into()),
        });

        let view = cx.entity().clone();
        cx.spawn_in(window, async move |_, window| {
            if let Some(path) = prompt_path.await.ok().and_then(|r| r.ok()).flatten().and_then(|mut v| v.pop()) {
                window
                    .update(|window, cx| {
                        view.update(cx, |this, cx| {
                            this.import_file_from_path(path, None, window, cx);
                        });
                    })
                    .ok();
            }
        })
        .detach();
    }

    pub(crate) fn on_action_import_base64(&mut self, _: &crate::actions::ImportBase64, window: &mut Window, cx: &mut Context<Self>) {
        let prompt_path = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Base64 file to import".into()),
        });

        let view = cx.entity().clone();
        cx.spawn_in(window, async move |_, window| {
            if let Some(path) = prompt_path.await.ok().and_then(|r| r.ok()).flatten().and_then(|mut v| v.pop()) {
                window
                    .update(|window, cx| {
                        view.update(cx, |this, cx| {
                            this.import_file_from_path(path, Some(crate::core::format::FileFormat::Base64), window, cx);
                        });
                    })
                    .ok();
            }
        })
        .detach();
    }

    fn export_active_document_with_format(
        &mut self,
        format_label: &'static str,
        default_ext: &'static str,
        export_fn: fn(&[u8], &crate::core::address_map::AddressMap) -> Vec<u8>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.active_editor(cx) else {
            return;
        };
        let (document, default_name, parent_dir) = {
            let editor_read = editor.read(cx);
            let document = editor_read.document.clone();
            let (default_name, parent_dir) = {
                let document_read = document.read().expect("document read lock");
                let path = document_read.path();

                let stem = path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .filter(|s| !s.is_empty() && *s != "Untitled" && *s != "untitled")
                    .unwrap_or("untitled");
                let default_name = format!("{}.{}", stem, default_ext);

                let parent_dir = path
                    .parent()
                    .filter(|p| p.exists())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")));
                (default_name, parent_dir)
            };
            (document, default_name, parent_dir)
        };

        let prompt = cx.prompt_for_new_path(&parent_dir, Some(&default_name));
        cx.spawn_in(window, async move |_, window| {
            let Some(mut path) = prompt.await.ok().and_then(|result| result.ok()).flatten() else {
                return;
            };
            if path.extension().is_none() {
                path.set_extension(default_ext);
            }

            let (contents, address_map) = {
                let doc = document.read().expect("document read lock");
                (doc.buffer.data().to_vec(), doc.address_map.clone())
            };

            let bytes = export_fn(&contents, &address_map);
            match tokio::fs::write(&path, bytes).await {
                Ok(()) => {
                    let _ = window.update(|window, cx| {
                        window.push_notification(
                            gpui_kit::component::notification::Notification::info(format!(
                                "Exported {} to {} successfully",
                                format_label,
                                path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
                            )),
                            cx,
                        );
                    });
                }
                Err(e) => {
                    let _ = window.update(|window, cx| {
                        window.push_notification(
                            gpui_kit::component::notification::Notification::error(format!("Failed to export {}: {}", format_label, e)),
                            cx,
                        );
                    });
                }
            }
        })
        .detach();
    }

    pub(crate) fn on_action_export_base64(&mut self, _: &crate::actions::ExportBase64, window: &mut Window, cx: &mut Context<Self>) {
        self.export_active_document_with_format(
            "Base64",
            "b64",
            |data, map| crate::core::format::export_base64(data, map).into_bytes(),
            window,
            cx,
        );
    }

    pub(crate) fn on_action_export_motorola_srec(&mut self, _: &crate::actions::ExportMotorolaSrec, window: &mut Window, cx: &mut Context<Self>) {
        self.export_active_document_with_format(
            "Motorola S-Record",
            "mot",
            |data, map| crate::core::hex_import::export_motorola_srec(data, map).into_bytes(),
            window,
            cx,
        );
    }

    pub(crate) fn on_action_export_intel_hex(&mut self, _: &crate::actions::ExportIntelHex, window: &mut Window, cx: &mut Context<Self>) {
        self.export_active_document_with_format(
            "Intel HEX",
            "hex",
            |data, map| crate::core::hex_import::export_intel_hex(data, map).into_bytes(),
            window,
            cx,
        );
    }

    pub(crate) fn on_action_export_raw_binary(&mut self, _: &crate::actions::ExportRawBinary, window: &mut Window, cx: &mut Context<Self>) {
        self.export_active_document_with_format(
            "Raw Binary",
            "bin",
            |data, map| crate::core::hex_import::export_raw_binary(data, map, 0x00),
            window,
            cx,
        );
    }

    pub(crate) fn on_action_load_structure_definition(&mut self, _: &LoadStructureDefinition, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_editor(cx).is_none() {
            return;
        }

        let path = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select a Kaitai Struct definition (.ksy, .yaml)".into()),
        });

        let view = cx.entity().clone();

        cx.spawn_in(window, async move |_, window| {
            if let Some(path) = path.await.ok().and_then(|r| r.ok()).flatten().and_then(|mut p| p.pop()) {
                window
                    .update(|window, cx| {
                        view.update(cx, |this, cx| {
                            this.load_structure_definition_from_path(path, window, cx);
                        });
                    })
                    .ok();
            }
        })
        .detach();
    }

    pub(crate) fn on_action_load_structure_definition_from_history(
        &mut self,
        action: &crate::actions::LoadStructureDefinitionFromHistory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_editor(cx).is_none() {
            return;
        }

        self.load_structure_definition_from_path(PathBuf::from(&action.path), window, cx);
    }

    pub(crate) fn load_structure_definition_from_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target_editor) = self.active_editor(cx) else { return };

        let view = cx.entity().clone();
        cx.spawn_in(window, async move |_, window| match std::fs::read_to_string(&path) {
            Ok(contents) => match serde_yaml::from_str::<crate::core::structure::KsyDefinition>(&contents) {
                Ok(ksy) => {
                    window
                        .update(|window, cx| {
                            view.update(cx, |this, cx| {
                                this.apply_loaded_structure_definition(target_editor, path, ksy, window, cx);
                            });
                        })
                        .ok();
                }
                Err(e) => {
                    eprintln!("Failed to parse KSY definition: {}", e);
                    let _ = window.update(|window, cx| {
                        window.push_notification(
                            gpui_kit::component::notification::Notification::error(format!("Failed to parse structure definition: {e}")),
                            cx,
                        );
                    });
                }
            },
            Err(e) => {
                eprintln!("Failed to read KSY file at {:?}: {}", path, e);
                let _ = window.update(|window, cx| {
                    window.push_notification(
                        gpui_kit::component::notification::Notification::error(format!("Failed to read structure file: {e}")),
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    pub(crate) fn apply_loaded_structure_definition(
        &mut self,
        target_editor: Entity<Editor>,
        path: PathBuf,
        ksy: crate::core::structure::KsyDefinition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ksy = Arc::new(ksy);
        let service = crate::app_state::AppState::global(cx).structure_service.clone();
        service.start_parse(&target_editor, ksy, cx);

        self.recent_definition_history.record(path);
        self.publish_recent_history(cx);
        if self.active_editor(cx).is_some_and(|editor| editor.entity_id() == target_editor.entity_id()) {
            self.left_panel.update(cx, |panel, cx| {
                panel.set_editor(Some(target_editor), cx);
                panel.set_tab(crate::ui::panels::left_panel::LeftPanelTab::Structure, cx);
            });
            self.set_left_panel_visible(true, window, cx);
        }
    }

    pub(crate) fn on_action_remove_structure_definition_from_history(
        &mut self,
        action: &crate::actions::RemoveStructureDefinitionFromHistory,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = PathBuf::from(&action.path);
        if self.recent_definition_history.remove(&path) {
            self.publish_recent_history(cx);
        }
    }

    pub(crate) fn on_action_remove_file_from_history(&mut self, action: &crate::actions::RemoveFileFromHistory, _: &mut Window, cx: &mut Context<Self>) {
        let path = PathBuf::from(&action.path);
        if self.recent_file_history.remove(&path) {
            self.publish_recent_history(cx);
        }
    }

    pub(crate) fn on_action_open_folder(&mut self, _: &OpenFolder, window: &mut Window, cx: &mut Context<Self>) {
        let path = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Select a folder".into()),
        });

        let left_panel = self.left_panel.clone();
        cx.spawn_in(window, async move |_, window| {
            if let Some(path) = path.await.ok().and_then(|r| r.ok()).flatten().and_then(|mut p| p.pop()) {
                window
                    .update(|_, cx| {
                        left_panel.update(cx, |p, cx| {
                            p.file_tree.update(cx, |ft, cx| {
                                ft.set_root_path(path, cx);
                            });
                        });
                    })
                    .ok();
            }
        })
        .detach();
    }

    pub(crate) fn on_action_close_folder(&mut self, _: &CloseFolder, _: &mut Window, cx: &mut Context<Self>) {
        self.left_panel.update(cx, |p, cx| {
            p.file_tree.update(cx, |ft, cx| {
                ft.close_folder(cx);
            });
        });
    }

    pub(crate) fn on_action_save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor(cx) else {
            return;
        };
        let (document, state_id, is_read_only, path) = {
            let editor_read = editor.read(cx);
            let document = editor_read.document.clone();
            let document_read = document.read().expect("document read lock");
            (
                document.clone(),
                document_read.history.state_id(),
                document_read.is_read_only(),
                document_read.path().to_path_buf(),
            )
        };
        if is_read_only {
            window.push_notification(
                gpui_kit::component::notification::Notification::warning("Cannot save: document is in read-only mode. Toggle read-only mode or use Save As."),
                cx,
            );
            return;
        }
        if !path.exists() {
            self.on_action_save_as(&crate::actions::SaveAs, window, cx);
            return;
        }
        let service = AppState::global(cx).document_service.clone();
        let task = service.save_document(document.clone(), cx);
        let workspace = cx.entity().clone();

        cx.spawn_in(window, async move |_, window| {
            let result = task.await;
            let _ = window.update(|window, cx| {
                workspace.update(cx, |_, cx| {
                    match result {
                        Ok(()) => {
                            let should_mark_saved = document.read().map(|document| document.history.state_id() == state_id).unwrap_or(false);
                            if should_mark_saved {
                                document.write().expect("document write lock").mark_as_saved();
                            }
                            let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
                            window.push_notification(gpui_kit::component::notification::Notification::info(format!("Saved {}", file_name)), cx);
                            editor.update(cx, |_, cx| cx.notify());
                        }
                        Err(error) => {
                            eprintln!("Failed to save document: {error}");
                            window.push_notification(
                                gpui_kit::component::notification::Notification::error(format!("Failed to save document: {error}")),
                                cx,
                            );
                        }
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    pub(crate) fn on_action_toggle_read_only(&mut self, _: &ToggleReadOnly, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor(cx) else {
            return;
        };
        let (document, is_read_only, is_dirty, state_id) = {
            let editor_read = editor.read(cx);
            let document = editor_read.document.clone();
            let document_read = document.read().expect("document read lock");
            (
                document.clone(),
                document_read.is_read_only(),
                document_read.is_dirty(),
                document_read.history.state_id(),
            )
        };

        if is_read_only {
            self.set_editor_read_only(&editor, false, cx);
            return;
        }

        if !is_dirty {
            self.set_editor_read_only(&editor, true, cx);
            return;
        }

        let prompt = window.prompt(
            PromptLevel::Warning,
            "Unsaved Changes",
            Some("Save changes before switching this file to read-only?"),
            &["Save and Make Read-only", "Cancel"],
            cx,
        );
        let workspace = cx.entity();
        let service = AppState::global(cx).document_service.clone();

        cx.spawn_in(window, async move |_, window| {
            let Ok(choice) = prompt.await else {
                return;
            };
            if choice != 0 {
                return;
            }

            let Some(save_task) = window.update(|_, cx| service.save_document(document.clone(), cx)).ok() else {
                return;
            };
            let result = save_task.await;
            let _ = window.update(|_, cx| {
                workspace.update(cx, |workspace, cx| {
                    if let Err(error) = result {
                        eprintln!("Failed to save document before making it read-only: {error}");
                        return;
                    }

                    let unchanged_since_save = document.read().map(|document| document.history.state_id() == state_id).unwrap_or(false);
                    if unchanged_since_save {
                        document.write().expect("document write lock").mark_as_saved();
                        workspace.set_editor_read_only(&editor, true, cx);
                    }
                });
            });
        })
        .detach();
    }

    pub(crate) fn set_editor_read_only(&self, editor: &Entity<Editor>, read_only: bool, cx: &mut Context<Self>) {
        let (path, changed) = editor.update(cx, |editor, _| {
            let mut document = editor.document.write().expect("document write lock");
            let changed = document.is_read_only() != read_only;
            if changed {
                document.set_read_only(read_only);
            }
            (document.path().to_path_buf(), changed)
        });

        if changed {
            let service = AppState::global(cx).document_service.clone();
            service.notify_document_changed(&path, cx);
            cx.notify();
        }
    }

    pub(crate) fn on_action_save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor(cx) else {
            return;
        };
        let (document, state_id, default_name, parent_dir, default_ext) = {
            let editor_read = editor.read(cx);
            let document = editor_read.document.clone();
            let document_read = document.read().expect("document read lock");
            let path = document_read.path();
            let default_ext = if document_read.format == crate::core::format::FileFormat::Base64 {
                "b64"
            } else {
                document_read.address_map.default_extension(path)
            };

            let default_name = if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.is_empty() || file_name == "Untitled" || file_name == "untitled" {
                    format!("untitled.{}", default_ext)
                } else if path.extension().is_none() {
                    format!("{}.{}", file_name, default_ext)
                } else {
                    file_name.to_string()
                }
            } else {
                format!("untitled.{}", default_ext)
            };

            let parent_dir = path
                .parent()
                .filter(|p| p.exists())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")));
            (document.clone(), document_read.history.state_id(), default_name, parent_dir, default_ext)
        };
        let prompt = cx.prompt_for_new_path(&parent_dir, Some(&default_name));
        let workspace = cx.entity().clone();
        let service = AppState::global(cx).document_service.clone();

        cx.spawn_in(window, async move |_, window| {
            let Some(mut path) = prompt.await.ok().and_then(|result| result.ok()).flatten() else {
                return;
            };
            if path.extension().is_none() {
                path.set_extension(default_ext);
            }
            let task = window.update(|_, cx| service.save_document_to_path(document.clone(), path.clone(), cx)).ok();
            let Some(task) = task else {
                return;
            };
            let result = task.await;
            let _ = window.update(|_, cx| {
                workspace.update(cx, |this, cx| {
                    match result {
                        Ok(()) => {
                            let mut document_write = document.write().expect("document write lock");
                            document_write.set_path(path.clone());
                            if document_write.history.state_id() == state_id {
                                document_write.mark_as_saved();
                            }
                            drop(document_write);
                            service.notify_document_changed(&path, cx);
                            for group in this.pane_tree.read(cx).all_groups() {
                                group.update(cx, |_, cx| cx.notify());
                            }
                            editor.update(cx, |_, cx| cx.notify());
                        }
                        Err(error) => {
                            eprintln!("Failed to save document as: {error}");
                        }
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    pub(crate) fn open_about_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::ui::components::about_dialog::open_about_dialog(window, cx);
    }
}
