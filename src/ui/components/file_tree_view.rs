use crate::actions::{LoadChildren, OpenDiff, OpenFile, Rename, SelectForCompare, SelectItem};
use crate::core::format::FileFormat;
use crate::core::structure::RecentFileEntry;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::ui::icon::IconName;
use autocorrect::ignorer::Ignorer;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, StyledExt as _,
    button::ButtonVariants as _,
    h_flex,
    list::ListItem,
    menu::ContextMenuExt,
    tree::{TreeItem, TreeState, tree},
    v_flex,
};
use gpui_kit::{
    App, AppContext, AsyncApp, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, MouseButton,
    ParentElement, PathPromptOptions, Render, ScrollStrategy, SharedString, StatefulInteractiveElement, Styled, WeakEntity, Window, actions, div,
    prelude::FluentBuilder as _, px, relative,
};

actions!(file_tree, [MoveUp, MoveDown, MoveTop, MoveBottom, PageUp, PageDown]);

const CONTEXT: &str = "FileTreeView";

fn path_file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, Some("FileTreeView && !Input")),
        KeyBinding::new("down", MoveDown, Some("FileTreeView && !Input")),
        KeyBinding::new("k", MoveUp, Some("FileTreeView && !Input")),
        KeyBinding::new("j", MoveDown, Some("FileTreeView && !Input")),
        KeyBinding::new("home", MoveTop, Some("FileTreeView && !Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-home", MoveTop, Some("FileTreeView && !Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-home", MoveTop, Some("FileTreeView && !Input")),
        KeyBinding::new("end", MoveBottom, Some("FileTreeView && !Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-end", MoveBottom, Some("FileTreeView && !Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-end", MoveBottom, Some("FileTreeView && !Input")),
        KeyBinding::new("pageup", PageUp, Some("FileTreeView && !Input")),
        KeyBinding::new("pagedown", PageDown, Some("FileTreeView && !Input")),
        KeyBinding::new("enter", SelectItem, Some("FileTreeView && !Input")),
    ]);
}

pub enum FileTreeViewEvent {
    OpenFile { path: PathBuf, format: Option<FileFormat> },
}

/// Tracks recent files for UI display, allowing re-sorting to be deferred
/// when opening items from the recents list itself.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecentDisplayHistory {
    displayed_entries: Vec<RecentFileEntry>,
    latest_entries: Vec<RecentFileEntry>,
    defer_reorder: bool,
}

impl RecentDisplayHistory {
    /// Creates an empty recent display history tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the currently displayed entries in their display order.
    pub fn displayed_entries(&self) -> &[RecentFileEntry] {
        &self.displayed_entries
    }

    /// Returns the currently displayed paths in their display order.
    #[allow(dead_code)]
    pub fn displayed_paths(&self) -> Vec<PathBuf> {
        self.displayed_entries.iter().map(|e| e.path.clone()).collect()
    }

    /// Returns the latest entries in MRU order.
    #[allow(dead_code)]
    pub fn latest_entries(&self) -> &[RecentFileEntry] {
        &self.latest_entries
    }

    /// Returns the latest paths in MRU order.
    #[allow(dead_code)]
    pub fn latest_paths(&self) -> Vec<PathBuf> {
        self.latest_entries.iter().map(|e| e.path.clone()).collect()
    }

    /// Returns true if re-sorting is currently deferred.
    #[allow(dead_code)]
    pub fn is_deferred(&self) -> bool {
        self.defer_reorder
    }

    /// Sets whether re-ordering should be deferred upon updates.
    pub fn set_deferred(&mut self, deferred: bool) {
        self.defer_reorder = deferred;
    }

    /// Updates the history with new entries. Returns true if the displayed list changed.
    pub fn update(&mut self, entries: &[RecentFileEntry]) -> bool {
        self.latest_entries = entries.to_vec();
        if self.defer_reorder {
            let previous_len = self.displayed_entries.len();
            self.displayed_entries.retain(|entry| entries.iter().any(|e| e.path == entry.path));
            return self.displayed_entries.len() != previous_len;
        }

        if self.displayed_entries == entries {
            return false;
        }

        self.displayed_entries = entries.to_vec();
        true
    }

    /// Resets the deferred state and synchronizes displayed entries with latest entries.
    /// Returns true if the displayed list changed.
    pub fn sync(&mut self) -> bool {
        self.defer_reorder = false;
        if self.displayed_entries != self.latest_entries {
            self.displayed_entries = self.latest_entries.clone();
            true
        } else {
            false
        }
    }
}

pub struct FileTreeView {
    tree_state: Entity<TreeState>,
    selected_item: Option<TreeItem>,
    selected_items: Vec<TreeItem>,
    _title: SharedString,
    focus_handle: FocusHandle,
    root_path: Option<PathBuf>,
    loaded_paths: HashSet<String>,
    items: Vec<TreeItem>,
    recent_history: RecentDisplayHistory,
    pub pending_compare_path: Option<String>,
}

fn build_file_items(ignorer: &Ignorer, root: &PathBuf, path: &PathBuf) -> Vec<TreeItem> {
    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let relative_path = path.strip_prefix(root).unwrap_or(&path);
            if ignorer.is_ignored(&relative_path.to_string_lossy()) || relative_path.ends_with(".git") {
                continue;
            }
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown").to_string();
            let id = path.to_string_lossy().to_string();
            if path.is_dir() {
                items.push(TreeItem::new(id, file_name).child(TreeItem::new("loading", "Loading...")));
            } else {
                items.push(TreeItem::new(id, file_name));
            }
        }
    }
    items.sort_by(|a, b| b.is_folder().cmp(&a.is_folder()).then(a.label.cmp(&b.label)));
    items
}

fn update_item_children(items: &mut [TreeItem], target_id: &str, children: Vec<TreeItem>) -> bool {
    let mut pending = vec![items];
    let mut replacement = Some(children);

    while let Some(items) = pending.pop() {
        for item in items.iter_mut() {
            if item.id == target_id {
                item.children = replacement.take().expect("tree child replacement must be available");
                return true;
            }
            if item.is_folder() {
                pending.push(item.children.as_mut_slice());
            }
        }
    }

    false
}

impl FileTreeView {
    pub fn new(title: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tree_state = cx.new(|cx| TreeState::new(cx));
        let focus_handle = cx.focus_handle();

        cx.on_focus_in(&focus_handle, window, |this, _, cx| {
            cx.notify();
            this.clear_tree_selection(cx);
        })
        .detach();
        cx.on_focus_out(&focus_handle, window, |this, _, _, cx| {
            this.clear_tree_selection(cx);
            cx.notify();
        })
        .detach();

        Self {
            tree_state: tree_state.clone(),
            selected_item: None,
            selected_items: Vec::new(),
            _title: title.into(),
            focus_handle,
            root_path: None,
            loaded_paths: HashSet::new(),
            items: Vec::new(),
            recent_history: RecentDisplayHistory::new(),
            pending_compare_path: None,
        }
    }

    /// Updates the recent binary file entries displayed in the empty state.
    pub fn set_recent_file_history(&mut self, entries: &[RecentFileEntry], cx: &mut Context<Self>) {
        if self.recent_history.update(entries) {
            cx.notify();
        }
    }

    /// Synchronizes the displayed recent file paths with the latest history.
    pub fn sync_recent_file_history(&mut self, cx: &mut Context<Self>) {
        if self.recent_history.sync() {
            cx.notify();
        }
    }

    fn load_root(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(|view: WeakEntity<FileTreeView>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let ignorer = Ignorer::new(&path.to_string_lossy());
                let items = build_file_items(&ignorer, &path, &path);

                view.update(&mut cx, |this, cx| {
                    this.items = items.clone();
                    this.tree_state.update(cx, |tree, cx| {
                        tree.set_items(items, cx);
                    });
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn load_children(&mut self, item_id: &str, cx: &mut Context<Self>) {
        if self.loaded_paths.contains(item_id) {
            return;
        }

        let path = PathBuf::from(item_id);
        if path.is_dir() {
            let item_id_clone = item_id.to_string();
            let root_path = self.root_path.clone();

            cx.spawn(|view: WeakEntity<FileTreeView>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    if let Some(root_path) = root_path {
                        let ignorer = Ignorer::new(&root_path.to_string_lossy());
                        let children = build_file_items(&ignorer, &root_path, &PathBuf::from(&item_id_clone));

                        view.update(&mut cx, |this, cx| {
                            if update_item_children(&mut this.items, &item_id_clone, children) {
                                this.tree_state.update(cx, |state, cx| {
                                    state.set_items(this.items.clone(), cx);
                                });
                            }
                        })
                        .ok();
                    }
                }
            })
            .detach();

            self.loaded_paths.insert(item_id.to_string());
        }
    }

    fn on_action_select_item(&mut self, _: &SelectItem, _: &mut Window, cx: &mut Context<Self>) {
        let item = self
            .selected_item
            .clone()
            .or_else(|| self.tree_state.read(cx).selected_entry().map(|entry| entry.item().clone()));

        if let Some(item) = item {
            self.selected_item = Some(item.clone());
            self.selected_items = vec![item.clone()];
            self.clear_tree_selection(cx);

            if !item.is_folder() {
                cx.emit(FileTreeViewEvent::OpenFile {
                    path: PathBuf::from(item.id.to_string()),
                    format: None,
                });
            }
            cx.notify();
        }
    }

    fn on_action_rename(&mut self, _: &Rename, _: &mut Window, cx: &mut Context<Self>) {
        let item = self
            .selected_item
            .clone()
            .or_else(|| self.tree_state.read(cx).selected_entry().map(|entry| entry.item().clone()));

        if let Some(item) = item {
            println!("Renaming item: {} ({})", item.label, item.id);
        }
    }

    pub fn prompt_open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Select a folder".into()),
        });

        let view = cx.entity().clone();
        cx.spawn_in(window, async move |_, window| {
            if let Some(path) = path.await.ok().and_then(|r| r.ok()).flatten().and_then(|mut p| p.pop()) {
                window
                    .update(|_, cx| {
                        view.update(cx, |this, cx| {
                            this.set_root_path(path, cx);
                        });
                    })
                    .ok();
            }
        })
        .detach();
    }

    pub fn close_folder(&mut self, cx: &mut Context<Self>) {
        self.sync_recent_file_history(cx);
        self.root_path = None;
        self.loaded_paths.clear();
        self.items.clear();
        self.selected_item = None;
        self.selected_items.clear();
        self.clear_tree_selection(cx);
        self.tree_state.update(cx, |state, cx| {
            state.set_items(vec![], cx);
        });
        cx.notify();
    }

    pub fn set_root_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.root_path = Some(path.clone());
        self.loaded_paths.clear();
        self.loaded_paths.insert(path.to_string_lossy().to_string());
        self.selected_item = None;
        self.selected_items.clear();
        self.clear_tree_selection(cx);
        self.load_root(path, cx);
        cx.notify();
    }

    fn on_action_set_file_tree_folder(&mut self, action: &crate::actions::SetFileTreeFolder, _: &mut Window, cx: &mut Context<Self>) {
        let path = PathBuf::from(&action.path);
        self.set_root_path(path, cx);
    }

    fn on_action_load_children(&mut self, action: &LoadChildren, _: &mut Window, cx: &mut Context<Self>) {
        self.load_children(&action.path, cx);
    }

    fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(-1, cx);
    }

    fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_cursor(1, cx);
    }

    fn move_top(&mut self, _: &MoveTop, _: &mut Window, cx: &mut Context<Self>) {
        let visible_items = self.visible_items();
        if visible_items.is_empty() {
            return;
        }
        let item = visible_items[0].clone();
        self.selected_item = Some(item.clone());
        self.selected_items = vec![item];
        self.tree_state.update(cx, |state, _| {
            state.scroll_to_item(0, ScrollStrategy::Top);
        });
        self.clear_tree_selection(cx);
        cx.notify();
    }

    fn move_bottom(&mut self, _: &MoveBottom, _: &mut Window, cx: &mut Context<Self>) {
        let visible_items = self.visible_items();
        if visible_items.is_empty() {
            return;
        }
        let last_index = visible_items.len() - 1;
        let item = visible_items[last_index].clone();
        self.selected_item = Some(item.clone());
        self.selected_items = vec![item];
        self.tree_state.update(cx, |state, _| {
            state.scroll_to_item(last_index, ScrollStrategy::Bottom);
        });
        self.clear_tree_selection(cx);
        cx.notify();
    }

    fn page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
        let visible_items = self.visible_items();
        if visible_items.is_empty() {
            return;
        }
        let current_index = self
            .selected_item
            .as_ref()
            .and_then(|selected| visible_items.iter().position(|item| item.id == selected.id))
            .unwrap_or(0);
        let next_index = current_index.saturating_sub(10);
        let item = visible_items[next_index].clone();
        self.selected_item = Some(item.clone());
        self.selected_items = vec![item];
        self.tree_state.update(cx, |state, _| {
            state.scroll_to_item(next_index, ScrollStrategy::Top);
        });
        self.clear_tree_selection(cx);
        cx.notify();
    }

    fn page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
        let visible_items = self.visible_items();
        if visible_items.is_empty() {
            return;
        }
        let current_index = self
            .selected_item
            .as_ref()
            .and_then(|selected| visible_items.iter().position(|item| item.id == selected.id))
            .unwrap_or(0);
        let last_index = visible_items.len() - 1;
        let next_index = (current_index + 10).min(last_index);
        let item = visible_items[next_index].clone();
        self.selected_item = Some(item.clone());
        self.selected_items = vec![item];
        self.tree_state.update(cx, |state, _| {
            state.scroll_to_item(next_index, ScrollStrategy::Bottom);
        });
        self.clear_tree_selection(cx);
        cx.notify();
    }

    fn move_cursor(&mut self, direction: i8, cx: &mut Context<Self>) {
        let visible_items = self.visible_items();
        if visible_items.is_empty() {
            return;
        }

        let current_index = self
            .selected_item
            .as_ref()
            .and_then(|selected| visible_items.iter().position(|item| item.id == selected.id));
        let last_index = visible_items.len() - 1;
        let next_index = match (current_index, direction) {
            (Some(index), -1) => index.saturating_sub(1),
            (Some(index), 1) => (index + 1).min(last_index),
            (Some(index), _) => index,
            (None, _) => 0,
        };
        let item = visible_items[next_index].clone();

        self.selected_item = Some(item.clone());
        self.selected_items = vec![item];
        self.tree_state.update(cx, |state, _| {
            let strategy = if direction < 0 { ScrollStrategy::Top } else { ScrollStrategy::Bottom };
            state.scroll_to_item(next_index, strategy);
        });
        self.clear_tree_selection(cx);
        cx.notify();
    }

    fn visible_items(&self) -> Vec<TreeItem> {
        let mut visible_items = Vec::new();
        Self::collect_visible_items(&self.items, &mut visible_items);
        visible_items
    }

    fn collect_visible_items(items: &[TreeItem], visible_items: &mut Vec<TreeItem>) {
        let mut pending: Vec<&TreeItem> = items.iter().rev().collect();
        while let Some(item) = pending.pop() {
            visible_items.push(item.clone());
            if item.is_expanded() {
                pending.extend(item.children.iter().rev());
            }
        }
    }

    fn clear_tree_selection(&mut self, cx: &mut Context<Self>) {
        if self.tree_state.read(cx).selected_index().is_some() {
            self.tree_state.update(cx, |state, cx| {
                state.set_selected_index(None, cx);
            });
        }
    }

    fn toggle_selection(&mut self, item: TreeItem, cx: &mut Context<Self>) {
        if let Some(pos) = self.selected_items.iter().position(|i| i.id == item.id) {
            self.selected_items.remove(pos);
        } else {
            self.selected_items.push(item);
        }
        cx.notify();
    }
}

impl Render for FileTreeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_empty = self.root_path.is_none();
        let is_focused = self.focus_handle.is_focused(window);
        let theme = cx.theme();

        let header_actions = if !is_empty {
            Some(
                gpui_kit::component::button::Button::new("close-folder")
                    .ghost()
                    .icon(IconName::Eraser)
                    .with_size(gpui_kit::component::Size::XSmall)
                    .tooltip("Close Folder")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_folder(cx);
                    }))
                    .into_any_element(),
            )
        } else {
            Some(
                gpui_kit::component::button::Button::new("open-folder-header")
                    .ghost()
                    .icon(IconName::FolderOpen)
                    .with_size(gpui_kit::component::Size::XSmall)
                    .tooltip("Open Folder")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.prompt_open_folder(window, cx);
                    }))
                    .into_any_element(),
            )
        };

        let header = crate::ui::style::panel_header("FILES", is_focused, theme, None, header_actions);

        let container = crate::ui::style::panel_container(is_focused, theme);

        container
            .id("file-tree-view")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.focus_handle.focus(window, cx);
                }),
            )
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_top))
            .on_action(cx.listener(Self::move_bottom))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::on_action_rename))
            .on_action(cx.listener(Self::on_action_select_item))
            .on_action(cx.listener(Self::on_action_set_file_tree_folder))
            .on_action(cx.listener(Self::on_action_load_children))
            .child(header)
            .child(div().flex_1().min_h_0().w_full().overflow_hidden().child(if is_empty {
                let open_btn = gpui_kit::component::button::Button::new("open-folder-btn")
                    .label("Open Folder")
                    .primary()
                    .with_size(gpui_kit::component::Size::Small)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.prompt_open_folder(window, cx);
                    }))
                    .into_any_element();

                let mut empty_actions = v_flex().w_full().items_center().child(open_btn);
                if !self.recent_history.displayed_entries().is_empty() {
                    let recent_items = self
                        .recent_history
                        .displayed_entries()
                        .iter()
                        .enumerate()
                        .map(|(index, entry)| {
                            let path = entry.path.to_string_lossy().into_owned();
                            let label = path_file_name(Path::new(&path));
                            let open_path = entry.path.clone();
                            let open_format = entry.format;
                            let remove_path = path.clone();
                            let tooltip_text = match entry.format {
                                Some(fmt) if fmt.is_import() => format!("{path} ({})", fmt.label()),
                                _ => path.clone(),
                            };

                            let mut item_content = h_flex()
                                .id(("recent-file-item", index))
                                .flex_1()
                                .min_w_0()
                                .h_5()
                                .items_center()
                                .gap_1()
                                .px_1()
                                .cursor_pointer()
                                .tooltip(move |_window, cx| cx.new(|_| gpui_kit::component::tooltip::Tooltip::new(tooltip_text.clone())).into())
                                .hover(|style| style.bg(theme.muted.opacity(0.4)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.recent_history.set_deferred(true);
                                    this.focus_handle.focus(window, cx);
                                    cx.emit(FileTreeViewEvent::OpenFile {
                                        path: open_path.clone(),
                                        format: open_format,
                                    });
                                }))
                                .child(Icon::new(IconName::File).with_size(gpui_kit::component::Size::XSmall))
                                .child(div().flex_1().min_w_0().text_xs().truncate().whitespace_nowrap().child(label));

                            if let Some(format) = entry.format
                                && format.is_import()
                            {
                                item_content = item_content.child(
                                    div()
                                        .px_1()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(theme.muted_foreground)
                                        .child(format.badge_text()),
                                );
                            }

                            h_flex()
                                .w_full()
                                .items_center()
                                .gap_1()
                                .child(item_content)
                                .child(
                                    gpui_kit::component::button::Button::new(SharedString::from(format!("remove-recent-file-{index}")))
                                        .ghost()
                                        .icon(IconName::Close)
                                        .with_size(gpui_kit::component::Size::XSmall)
                                        .tooltip("Remove from recents")
                                        .on_click(cx.listener(move |_, _, window, cx| {
                                            window.dispatch_action(Box::new(crate::actions::RemoveFileFromHistory { path: remove_path.clone() }), cx);
                                        })),
                                )
                                .into_any_element()
                        })
                        .collect::<Vec<_>>();

                    empty_actions = empty_actions.child(
                        h_flex().w_full().justify_start().child(
                            v_flex()
                                .w(relative(0.95))
                                .mt_4()
                                .pt_3()
                                .border_t_1()
                                .border_color(theme.border.opacity(0.6))
                                .items_start()
                                .gap_1()
                                .child(div().px_1().text_xs().font_semibold().text_color(theme.muted_foreground).child("Recents"))
                                .children(recent_items),
                        ),
                    );
                }

                crate::ui::style::panel_empty_state(
                    IconName::FolderOpen,
                    "No Folder Opened",
                    Some("Open a directory to explore files"),
                    Some(empty_actions.into_any_element()),
                    theme,
                )
                .into_any_element()
            } else {
                let view = cx.entity().clone();
                tree(&self.tree_state, {
                    let selected_ids: HashSet<_> = self.selected_items.iter().map(|i| i.id.clone()).collect();
                    let focus_handle = self.focus_handle.clone();
                    let loaded_paths = self.loaded_paths.clone();
                    move |ix, entry, _selected, window, cx| {
                        let item = entry.item();
                        let icon = if !entry.is_folder() {
                            IconName::File
                        } else if entry.is_expanded() {
                            IconName::FolderOpen
                        } else {
                            IconName::Folder
                        };

                        let is_multi_selected = selected_ids.contains(&item.id);
                        let is_focused = focus_handle.is_focused(window);

                        if entry.is_expanded() && entry.is_folder() && !loaded_paths.contains(&item.id.to_string()) {
                            let item_id = item.id.to_string();
                            window.dispatch_action(Box::new(crate::actions::LoadChildren { path: item_id }), cx);
                        }

                        let selection_bg = if is_focused { cx.theme().selection } else { cx.theme().muted };

                        ListItem::new(ix)
                            .selected(false)
                            .when(is_multi_selected, |this| this.bg(selection_bg))
                            .w_full()
                            .rounded(cx.theme().radius)
                            .px_3()
                            .pl(px(16.) * entry.depth() + px(12.))
                            .child(h_flex().gap_2().child(icon).child(item.label.clone()).size_full().context_menu({
                                let view = view.clone();
                                let item_id = item.id.clone();
                                let is_folder = item.is_folder();
                                move |menu, window, cx| {
                                    let (can_compare, left_path, right_path, pending_compare) = view.update(cx, |this, cx| {
                                        let can_compare = this.selected_items.len() == 2 && this.selected_items.iter().all(|item| !item.is_folder());
                                        let (lp, rp) = if can_compare {
                                            (Some(this.selected_items[0].id.to_string()), Some(this.selected_items[1].id.to_string()))
                                        } else {
                                            (None, None)
                                        };
                                        let pending = crate::app_state::PendingCompareState::path(cx).or_else(|| this.pending_compare_path.clone());
                                        (can_compare, lp, rp, pending)
                                    });

                                    let mut menu = menu.menu_with_icon("Open", IconName::FolderOpen, Box::new(OpenFile::new(item_id.to_string())));

                                    if !is_folder {
                                        let open_path = item_id.to_string();
                                        let open_path_b64 = open_path.clone();
                                        menu = menu.submenu("Import", window, cx, move |menu, _window, _cx| {
                                            menu.menu(
                                                "Motorola S-Record / Intel HEX",
                                                Box::new(OpenFile::with_format(open_path.clone(), Some(crate::core::format::FileFormat::HexOrMot))),
                                            )
                                            .menu(
                                                "Base64",
                                                Box::new(OpenFile::with_format(open_path_b64.clone(), Some(crate::core::format::FileFormat::Base64))),
                                            )
                                        });

                                        menu = menu.separator();
                                        if let Some(ref pending_path) = pending_compare
                                            && pending_path != &item_id.to_string()
                                        {
                                            let pending_file_name = std::path::Path::new(pending_path)
                                                .file_name()
                                                .map(|n| n.to_string_lossy().to_string())
                                                .unwrap_or_else(|| "Selected".to_string());
                                            menu = menu.menu_with_icon(
                                                format!("Compare with '{}'", pending_file_name),
                                                IconName::GitCompare,
                                                Box::new(OpenDiff {
                                                    left_path: pending_path.clone(),
                                                    right_path: item_id.to_string(),
                                                }),
                                            );
                                        }

                                        menu = menu.menu_with_icon(
                                            "Select for Compare",
                                            IconName::GitCompare,
                                            Box::new(SelectForCompare { path: item_id.to_string() }),
                                        );

                                        if can_compare {
                                            menu = menu.menu_with_icon(
                                                "Compare Selected Files",
                                                IconName::GitCompare,
                                                Box::new(OpenDiff {
                                                    left_path: left_path.unwrap_or_default(),
                                                    right_path: right_path.unwrap_or_default(),
                                                }),
                                            );
                                        }
                                    }

                                    menu.separator()
                                        .menu_with_icon("Copy Path", IconName::Copy, Box::new(crate::actions::CopyPath))
                                        .menu_with_icon("Copy File Name", IconName::FileText, Box::new(crate::actions::CopyFileName))
                                        .menu_with_icon(
                                            if cfg!(target_os = "macos") {
                                                "Reveal in Finder"
                                            } else {
                                                "Reveal in File Explorer"
                                            },
                                            IconName::FolderSearch,
                                            Box::new(crate::actions::RevealInExplorer),
                                        )
                                        .separator()
                                        .menu_with_icon("Rename", IconName::PenLine, Box::new(Rename))
                                }
                            }))
                            .on_click(window.listener_for(&view, {
                                let item = item.clone();
                                let focus_handle = focus_handle.clone();
                                move |this, event: &ClickEvent, window, cx| {
                                    focus_handle.focus(window, cx);
                                    if event.modifiers().control || event.modifiers().platform {
                                        this.toggle_selection(item.clone(), cx);
                                    } else {
                                        this.selected_items = vec![item.clone()];
                                        this.selected_item = Some(item.clone());
                                    }

                                    if !item.is_folder() && this.selected_items.len() == 1 {
                                        cx.emit(FileTreeViewEvent::OpenFile {
                                            path: PathBuf::from(item.id.to_string()),
                                            format: None,
                                        });
                                    }
                                    this.clear_tree_selection(cx);
                                    cx.notify();
                                }
                            }))
                    }
                })
                .into_any_element()
            }))
    }
}

impl EventEmitter<FileTreeViewEvent> for FileTreeView {}

impl Focusable for FileTreeView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FileTreeViewState {
    pub root_path: Option<PathBuf>,
}

impl FileTreeViewState {
    #[allow(dead_code)]
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("serialize FileTreeViewState")
    }

    #[allow(dead_code)]
    pub fn from_value(value: serde_json::Value) -> Option<Self> {
        serde_json::from_value(value).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::RecentDisplayHistory;
    use crate::core::format::FileFormat;
    use crate::core::structure::RecentFileEntry;
    use std::path::PathBuf;

    #[test]
    fn test_recent_display_history_initial_state() {
        let history = RecentDisplayHistory::new();
        assert!(history.displayed_entries().is_empty());
        assert!(history.latest_entries().is_empty());
        assert!(!history.is_deferred());
    }

    #[test]
    fn test_recent_display_history_immediate_update_when_not_deferred() {
        let mut history = RecentDisplayHistory::new();
        let entries = vec![
            RecentFileEntry::new(PathBuf::from("a.bin"), None),
            RecentFileEntry::new(PathBuf::from("b.hex"), Some(FileFormat::IntelHex)),
            RecentFileEntry::new(PathBuf::from("c.srec"), Some(FileFormat::MotorolaSrec)),
        ];

        assert!(history.update(&entries));
        assert_eq!(history.displayed_entries(), &entries);
        assert_eq!(history.latest_entries(), &entries);

        // Same entries should return false (no change)
        assert!(!history.update(&entries));
    }

    #[test]
    fn test_recent_display_history_deferred_reorder_preserves_display_order() {
        let mut history = RecentDisplayHistory::new();
        let initial_entries = vec![
            RecentFileEntry::new(PathBuf::from("a.bin"), None),
            RecentFileEntry::new(PathBuf::from("b.bin"), None),
            RecentFileEntry::new(PathBuf::from("c.hex"), Some(FileFormat::IntelHex)),
        ];
        history.update(&initial_entries);

        // Simulate user clicking "c.hex" from recents
        history.set_deferred(true);
        assert!(history.is_deferred());

        // Background history moves "c.hex" to front
        let updated_entries = vec![
            RecentFileEntry::new(PathBuf::from("c.hex"), Some(FileFormat::IntelHex)),
            RecentFileEntry::new(PathBuf::from("a.bin"), None),
            RecentFileEntry::new(PathBuf::from("b.bin"), None),
        ];
        assert!(!history.update(&updated_entries));

        // latest_entries updated to new MRU order, but displayed_entries keeps original order
        assert_eq!(history.latest_entries(), &updated_entries);
        assert_eq!(history.displayed_entries(), &initial_entries);

        // Simulate user clicking "b.bin" from recents
        let updated_entries_2 = vec![
            RecentFileEntry::new(PathBuf::from("b.bin"), None),
            RecentFileEntry::new(PathBuf::from("c.hex"), Some(FileFormat::IntelHex)),
            RecentFileEntry::new(PathBuf::from("a.bin"), None),
        ];
        assert!(!history.update(&updated_entries_2));
        assert_eq!(history.latest_entries(), &updated_entries_2);
        assert_eq!(history.displayed_entries(), &initial_entries);
    }

    #[test]
    fn test_recent_display_history_retains_on_removal_while_deferred() {
        let mut history = RecentDisplayHistory::new();
        let initial_entries = vec![
            RecentFileEntry::new(PathBuf::from("a.bin"), None),
            RecentFileEntry::new(PathBuf::from("b.bin"), None),
            RecentFileEntry::new(PathBuf::from("c.bin"), None),
        ];
        history.update(&initial_entries);

        history.set_deferred(true);

        // Remove "b.bin" from history
        let entries_after_remove = vec![
            RecentFileEntry::new(PathBuf::from("a.bin"), None),
            RecentFileEntry::new(PathBuf::from("c.bin"), None),
        ];
        assert!(history.update(&entries_after_remove));

        assert_eq!(history.latest_entries(), &entries_after_remove);
        // "b.bin" removed, order of remaining items preserved
        assert_eq!(
            history.displayed_entries(),
            &[
                RecentFileEntry::new(PathBuf::from("a.bin"), None),
                RecentFileEntry::new(PathBuf::from("c.bin"), None),
            ]
        );
    }

    #[test]
    fn test_recent_display_history_sync_flushes_latest_order() {
        let mut history = RecentDisplayHistory::new();
        let initial_entries = vec![
            RecentFileEntry::new(PathBuf::from("a.bin"), None),
            RecentFileEntry::new(PathBuf::from("b.bin"), None),
            RecentFileEntry::new(PathBuf::from("c.hex"), Some(FileFormat::IntelHex)),
        ];
        history.update(&initial_entries);

        history.set_deferred(true);
        let mru_entries = vec![
            RecentFileEntry::new(PathBuf::from("c.hex"), Some(FileFormat::IntelHex)),
            RecentFileEntry::new(PathBuf::from("a.bin"), None),
            RecentFileEntry::new(PathBuf::from("b.bin"), None),
        ];
        history.update(&mru_entries);
        assert_eq!(history.displayed_entries(), &initial_entries);

        // Sync when FILES panel is reopened
        assert!(history.sync());
        assert!(!history.is_deferred());
        assert_eq!(history.displayed_entries(), &mru_entries);

        // Second sync does nothing
        assert!(!history.sync());

        // Subsequent update when not deferred updates immediately
        let new_entries = vec![
            RecentFileEntry::new(PathBuf::from("d.bin"), None),
            RecentFileEntry::new(PathBuf::from("c.hex"), Some(FileFormat::IntelHex)),
            RecentFileEntry::new(PathBuf::from("a.bin"), None),
            RecentFileEntry::new(PathBuf::from("b.bin"), None),
        ];
        assert!(history.update(&new_entries));
        assert_eq!(history.displayed_entries(), &new_entries);
    }

    #[test]
    fn test_file_tree_navigation_indices() {
        let count = 15;
        let move_cursor = |curr: Option<usize>, dir: i8| match (curr, dir) {
            (Some(index), -1) => index.saturating_sub(1),
            (Some(index), 1) => (index + 1).min(count - 1),
            (Some(index), _) => index,
            (None, _) => 0,
        };
        assert_eq!(move_cursor(Some(5), -1), 4);
        assert_eq!(move_cursor(Some(0), -1), 0);
        assert_eq!(move_cursor(Some(5), 1), 6);
        assert_eq!(move_cursor(Some(14), 1), 14);

        let page_up = |curr: usize| curr.saturating_sub(10);
        let page_down = |curr: usize| (curr + 10).min(count - 1);
        assert_eq!(page_up(12), 2);
        assert_eq!(page_up(4), 0);
        assert_eq!(page_down(2), 12);
        assert_eq!(page_down(10), 14);
    }
}
