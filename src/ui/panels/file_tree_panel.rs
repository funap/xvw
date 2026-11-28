use crate::actions::{LoadChildren, OpenDiff, OpenFile, SelectForCompare, SelectItem};
use crate::core::format::FileFormat;
use crate::core::recent_history::RecentFileEntry;
use crate::ui::icon::IconName;
use crate::ui::notification;
use autocorrect::ignorer::Ignorer;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, Size, StyledExt as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    list::ListItem,
    menu::ContextMenuExt,
    tooltip::Tooltip,
    tree::{TreeItem, TreeState, tree},
    v_flex,
};
use gpui_kit::{
    Action, App, AppContext, AsyncApp, ClickEvent, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyBinding, MouseButton, ParentElement, PathPromptOptions, Render, ScrollStrategy, SharedString, StatefulInteractiveElement, Styled, WeakEntity, Window,
    actions, div, prelude::FluentBuilder as _, px, relative,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

actions!(
    file_tree_panel,
    [
        MoveUp,
        MoveDown,
        MoveTop,
        MoveBottom,
        PageUp,
        PageDown,
        ExpandItem,
        CollapseItem,
        Dismiss,
        ClearPendingCompare
    ]
);

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = file_tree_panel)]
struct CopyTreeItemPath {
    path: String,
}

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = file_tree_panel)]
struct CopyTreeItemName {
    name: String,
}

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = file_tree_panel)]
struct RevealTreeItem {
    path: String,
}

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = file_tree_panel)]
struct RefreshTreeFolder {
    path: String,
}

const CONTEXT: &str = "FileTreePanel";

fn path_file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some("FileTreePanel && !Input")),
        KeyBinding::new("up", MoveUp, Some("FileTreePanel && !Input")),
        KeyBinding::new("down", MoveDown, Some("FileTreePanel && !Input")),
        KeyBinding::new("k", MoveUp, Some("FileTreePanel && !Input")),
        KeyBinding::new("j", MoveDown, Some("FileTreePanel && !Input")),
        KeyBinding::new("right", ExpandItem, Some("FileTreePanel && !Input")),
        KeyBinding::new("l", ExpandItem, Some("FileTreePanel && !Input")),
        KeyBinding::new("left", CollapseItem, Some("FileTreePanel && !Input")),
        KeyBinding::new("h", CollapseItem, Some("FileTreePanel && !Input")),
        KeyBinding::new("home", MoveTop, Some("FileTreePanel && !Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-home", MoveTop, Some("FileTreePanel && !Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-home", MoveTop, Some("FileTreePanel && !Input")),
        KeyBinding::new("end", MoveBottom, Some("FileTreePanel && !Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-end", MoveBottom, Some("FileTreePanel && !Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-end", MoveBottom, Some("FileTreePanel && !Input")),
        KeyBinding::new("pageup", PageUp, Some("FileTreePanel && !Input")),
        KeyBinding::new("pagedown", PageDown, Some("FileTreePanel && !Input")),
        KeyBinding::new("enter", SelectItem, Some("FileTreePanel && !Input")),
    ]);
}

pub enum FileTreePanelEvent {
    OpenFile {
        path: PathBuf,
        format: Option<FileFormat>,
        record_recent: bool,
    },
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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

pub struct FileTreePanel {
    tree_state: Entity<TreeState>,
    selected_item: Option<TreeItem>,
    selected_items: Vec<TreeItem>,
    _title: SharedString,
    focus_handle: FocusHandle,
    root_path: Option<PathBuf>,
    loaded_paths: HashSet<String>,
    items: Vec<TreeItem>,
    load_generation: u64,
    recent_history: RecentDisplayHistory,
    pub pending_compare_path: Option<String>,
}

#[derive(Clone, Debug)]
struct ScannedFileEntry {
    id: String,
    label: String,
    is_dir: bool,
}

fn scan_directory_entries(ignorer: &Ignorer, root: &Path, path: &Path) -> Vec<ScannedFileEntry> {
    let mut entries_out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let relative_path = path.strip_prefix(root).unwrap_or(&path);
            if ignorer.is_ignored(&relative_path.to_string_lossy()) || relative_path.ends_with(".git") {
                continue;
            }
            let label = path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown").to_string();
            let id = path.to_string_lossy().to_string();
            let is_dir = path.is_dir();
            entries_out.push(ScannedFileEntry { id, label, is_dir });
        }
    }
    entries_out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.label.cmp(&b.label)));
    entries_out
}

fn scanned_entries_to_tree_items(entries: Vec<ScannedFileEntry>) -> Vec<TreeItem> {
    entries
        .into_iter()
        .map(|entry| {
            if entry.is_dir {
                TreeItem::new(entry.id, entry.label).child(TreeItem::new("loading", "Loading..."))
            } else {
                TreeItem::new(entry.id, entry.label)
            }
        })
        .collect()
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

/// Sets the expanded state on the tree item matching `target_id`. Returns `true` if found.
fn set_item_expanded(items: &mut [TreeItem], target_id: &str, expanded: bool) -> bool {
    let mut pending = vec![items];
    while let Some(slice) = pending.pop() {
        for item in slice.iter_mut() {
            if item.id == target_id {
                let placeholder = TreeItem::new("", "");
                let taken = std::mem::replace(item, placeholder);
                *item = taken.expanded(expanded);
                return true;
            }
            if item.is_folder() {
                pending.push(item.children.as_mut_slice());
            }
        }
    }
    false
}

/// Finds the parent item of `target_id` within `items`, if any.
fn find_parent_item(items: &[TreeItem], target_id: &str) -> Option<TreeItem> {
    let mut stack: Vec<(&TreeItem, Option<&TreeItem>)> = items.iter().rev().map(|i| (i, None)).collect();
    while let Some((item, parent)) = stack.pop() {
        if item.id == target_id {
            return parent.cloned();
        }
        if item.is_expanded() {
            for child in item.children.iter().rev() {
                stack.push((child, Some(item)));
            }
        }
    }
    None
}

impl FileTreePanel {
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
            tree_state,
            selected_item: None,
            selected_items: Vec::new(),
            _title: title.into(),
            focus_handle,
            root_path: None,
            loaded_paths: HashSet::new(),
            items: Vec::new(),
            load_generation: 0,
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

    /// Returns the currently opened folder root path, if any.
    pub fn root_path(&self) -> Option<&PathBuf> {
        self.root_path.as_ref()
    }

    /// Reloads the currently opened folder tree from disk.
    pub fn refresh_root(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.root_path.clone() {
            self.loaded_paths.clear();
            self.loaded_paths.insert(path.to_string_lossy().to_string());
            self.load_root(path, cx);
        }
    }

    fn load_root(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.load_generation = self.load_generation.wrapping_add(1);
        let generation = self.load_generation;
        let bg_task = cx.background_executor().spawn(async move {
            let ignorer = Ignorer::new(&path.to_string_lossy());
            scan_directory_entries(&ignorer, &path, &path)
        });

        cx.spawn(move |view: WeakEntity<FileTreePanel>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let scanned = bg_task.await;
                view.update(&mut cx, |this, cx| {
                    if this.load_generation != generation {
                        return;
                    }
                    let items = scanned_entries_to_tree_items(scanned);
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

    fn load_children(&mut self, item_id: &str, force: bool, cx: &mut Context<Self>) {
        if !force && self.loaded_paths.contains(item_id) {
            return;
        }

        let path = PathBuf::from(item_id);
        if path.is_dir() {
            let item_id_clone = item_id.to_string();
            let Some(root_path) = self.root_path.clone() else {
                return;
            };
            let generation = self.load_generation;
            let dir_path = path.clone();
            let bg_task = cx.background_executor().spawn(async move {
                let ignorer = Ignorer::new(&root_path.to_string_lossy());
                scan_directory_entries(&ignorer, &root_path, &dir_path)
            });

            self.loaded_paths.insert(item_id.to_string());

            cx.spawn(move |view: WeakEntity<FileTreePanel>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    let scanned = bg_task.await;
                    view.update(&mut cx, |this, cx| {
                        if this.load_generation != generation {
                            return;
                        }
                        let children = scanned_entries_to_tree_items(scanned);
                        if update_item_children(&mut this.items, &item_id_clone, children) {
                            this.tree_state.update(cx, |state, cx| {
                                state.set_items(this.items.clone(), cx);
                            });
                            cx.notify();
                        }
                    })
                    .ok();
                }
            })
            .detach();
        }
    }

    fn set_folder_expanded(&mut self, item_id: &str, expanded: bool, cx: &mut Context<Self>) {
        if set_item_expanded(&mut self.items, item_id, expanded) {
            if expanded {
                self.load_children(item_id, false, cx);
            }
            self.tree_state.update(cx, |state, cx| {
                state.set_items(self.items.clone(), cx);
            });
            self.clear_tree_selection(cx);
            cx.notify();
        }
    }

    fn on_action_select_item(&mut self, _: &SelectItem, _: &mut Window, cx: &mut Context<Self>) {
        let item = self
            .selected_item
            .clone()
            .or_else(|| self.tree_state.read(cx).selected_entry().map(|entry| entry.item().clone()));

        if let Some(item) = item {
            if item.is_folder() {
                let next_expanded = !item.is_expanded();
                let item_id = item.id.to_string();
                self.set_folder_expanded(&item_id, next_expanded, cx);
                if let Some(updated) = self.visible_items().into_iter().find(|v| v.id == item.id) {
                    self.selected_item = Some(updated.clone());
                    self.selected_items = vec![updated];
                }
                cx.notify();
                return;
            }

            self.selected_item = Some(item.clone());
            self.selected_items = vec![item.clone()];
            self.clear_tree_selection(cx);

            cx.emit(FileTreePanelEvent::OpenFile {
                path: PathBuf::from(item.id.to_string()),
                format: None,
                record_recent: true,
            });
            cx.notify();
        }
    }

    fn expand_item(&mut self, _: &ExpandItem, _: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.selected_item.clone() else {
            return;
        };
        if !item.is_folder() {
            return;
        }
        if !item.is_expanded() {
            let item_id = item.id.to_string();
            self.set_folder_expanded(&item_id, true, cx);
            if let Some(updated) = self.visible_items().into_iter().find(|v| v.id == item.id) {
                self.selected_item = Some(updated.clone());
                self.selected_items = vec![updated];
            }
            cx.notify();
        } else if let Some(first_child) = item.children.first().cloned() {
            let visible = self.visible_items();
            if let Some(idx) = visible.iter().position(|v| v.id == first_child.id) {
                self.selected_item = Some(first_child.clone());
                self.selected_items = vec![first_child];
                self.tree_state.update(cx, |state, _| {
                    state.scroll_to_item(idx, ScrollStrategy::Bottom);
                });
                self.clear_tree_selection(cx);
                cx.notify();
            }
        }
    }

    fn collapse_item(&mut self, _: &CollapseItem, _: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.selected_item.clone() else {
            return;
        };
        if item.is_folder() && item.is_expanded() {
            let item_id = item.id.to_string();
            self.set_folder_expanded(&item_id, false, cx);
            if let Some(updated) = self.visible_items().into_iter().find(|v| v.id == item.id) {
                self.selected_item = Some(updated.clone());
                self.selected_items = vec![updated];
            }
            cx.notify();
        } else if let Some(parent) = find_parent_item(&self.items, &item.id) {
            let visible = self.visible_items();
            if let Some(idx) = visible.iter().position(|v| v.id == parent.id) {
                self.selected_item = Some(parent.clone());
                self.selected_items = vec![parent];
                self.tree_state.update(cx, |state, _| {
                    state.scroll_to_item(idx, ScrollStrategy::Top);
                });
                self.clear_tree_selection(cx);
                cx.notify();
            }
        }
    }

    fn dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
    }

    fn on_copy_tree_item_path(&mut self, action: &CopyTreeItemPath, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.path.clone()));
        window.push_notification(notification::info("Copied path"), cx);
    }

    fn on_copy_tree_item_name(&mut self, action: &CopyTreeItemName, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.name.clone()));
        window.push_notification(notification::info(format!("Copied {}", action.name)), cx);
    }

    fn on_reveal_tree_item(&mut self, action: &RevealTreeItem, _: &mut Window, _: &mut Context<Self>) {
        crate::ui::workspace::reveal_in_file_explorer(Path::new(&action.path));
    }

    fn on_refresh_tree_folder(&mut self, action: &RefreshTreeFolder, _: &mut Window, cx: &mut Context<Self>) {
        self.load_children(&action.path, true, cx);
    }

    fn on_clear_pending_compare(&mut self, _: &ClearPendingCompare, _: &mut Window, cx: &mut Context<Self>) {
        self.pending_compare_path = None;
        crate::app_state::PendingCompareState::set(None, cx);
        cx.notify();
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
        self.load_generation = self.load_generation.wrapping_add(1);
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
        self.load_children(&action.path, false, cx);
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

impl Render for FileTreePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_empty = self.root_path.is_none();
        let is_focused = self.focus_handle.is_focused(window);
        let theme = cx.theme();
        let pending_compare = crate::app_state::PendingCompareState::path(cx).or_else(|| self.pending_compare_path.clone());

        let header_actions = if !is_empty {
            Some(
                h_flex()
                    .items_center()
                    .gap_0p5()
                    .child(
                        Button::new("refresh-folder")
                            .ghost()
                            .icon(IconName::Redo)
                            .with_size(Size::XSmall)
                            .tooltip("Refresh Folder")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_root(cx);
                            })),
                    )
                    .child(
                        Button::new("open-folder-switch")
                            .ghost()
                            .icon(IconName::FolderOpen)
                            .with_size(Size::XSmall)
                            .tooltip("Open Different Folder")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.prompt_open_folder(window, cx);
                            })),
                    )
                    .child(
                        Button::new("close-folder")
                            .ghost()
                            .icon(IconName::Close)
                            .with_size(Size::XSmall)
                            .tooltip("Close Folder")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_folder(cx);
                            })),
                    )
                    .into_any_element(),
            )
        } else {
            Some(
                Button::new("open-folder-header")
                    .ghost()
                    .icon(IconName::FolderOpen)
                    .with_size(Size::XSmall)
                    .tooltip("Open Folder")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.prompt_open_folder(window, cx);
                    }))
                    .into_any_element(),
            )
        };

        let header = crate::ui::panels::panel_header("FILES", is_focused, theme, None, header_actions);

        let root_strip = self.root_path.as_ref().map(|root| {
            let folder_name = path_file_name(root);
            let full_path = root.display().to_string();
            h_flex()
                .id("file-tree-root-strip")
                .w_full()
                .flex_shrink_0()
                .items_center()
                .gap_1p5()
                .px_3()
                .py_1()
                .bg(theme.secondary.opacity(0.45))
                .border_b_1()
                .border_color(theme.border)
                .tooltip(move |_window, cx| cx.new(|_| Tooltip::new(full_path.clone())).into())
                .child(Icon::new(IconName::FolderOpen).size_3p5().flex_shrink_0().text_color(theme.muted_foreground))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .whitespace_nowrap()
                        .text_xs()
                        .font_semibold()
                        .text_color(theme.foreground)
                        .child(folder_name),
                )
        });

        let compare_banner = pending_compare.as_ref().map(|path_str| {
            let file_name = path_file_name(Path::new(path_str));
            let full_tooltip = format!("Compare base: {path_str}");
            h_flex()
                .id("pending-compare-banner")
                .w_full()
                .flex_shrink_0()
                .items_center()
                .gap_1p5()
                .px_2p5()
                .py_1()
                .bg(theme.primary.opacity(0.12))
                .border_t_1()
                .border_color(theme.border)
                .tooltip(move |_window, cx| cx.new(|_| Tooltip::new(full_tooltip.clone())).into())
                .child(Icon::new(IconName::GitCompare).size_3().flex_shrink_0().text_color(theme.primary))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .whitespace_nowrap()
                        .text_xs()
                        .text_color(theme.foreground)
                        .child(format!("Compare: {file_name}")),
                )
                .child(
                    Button::new("clear-pending-compare-btn")
                        .ghost()
                        .icon(IconName::Close)
                        .with_size(Size::XSmall)
                        .tooltip("Clear selected compare file")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.on_clear_pending_compare(&ClearPendingCompare, window, cx);
                        })),
                )
        });

        let container = crate::ui::panels::panel_container(is_focused, theme);

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
            .on_action(cx.listener(Self::expand_item))
            .on_action(cx.listener(Self::collapse_item))
            .on_action(cx.listener(Self::dismiss))
            .on_action(cx.listener(Self::on_action_select_item))
            .on_action(cx.listener(Self::on_action_set_file_tree_folder))
            .on_action(cx.listener(Self::on_action_load_children))
            .on_action(cx.listener(Self::on_copy_tree_item_path))
            .on_action(cx.listener(Self::on_copy_tree_item_name))
            .on_action(cx.listener(Self::on_reveal_tree_item))
            .on_action(cx.listener(Self::on_refresh_tree_folder))
            .on_action(cx.listener(Self::on_clear_pending_compare))
            .child(header)
            .when_some(root_strip, |el, strip| el.child(strip))
            .child(div().flex_1().min_h_0().w_full().overflow_hidden().child(if is_empty {
                let open_btn = Button::new("open-folder-btn")
                    .label("Open Folder")
                    .primary()
                    .with_size(Size::Small)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.prompt_open_folder(window, cx);
                    }))
                    .into_any_element();

                let mut empty_actions = v_flex().w_full().min_h_0().items_center().child(open_btn);
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
                                .rounded_sm()
                                .cursor_pointer()
                                .tooltip(move |_window, cx| cx.new(|_| Tooltip::new(tooltip_text.clone())).into())
                                .hover(|style| style.bg(theme.list_hover))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.focus_handle.focus(window, cx);
                                    cx.emit(FileTreePanelEvent::OpenFile {
                                        path: open_path.clone(),
                                        format: open_format,
                                        record_recent: false,
                                    });
                                }))
                                .child(Icon::new(IconName::File).with_size(Size::XSmall).flex_shrink_0())
                                .child(div().flex_1().min_w_0().text_xs().truncate().whitespace_nowrap().child(label));

                            if let Some(format) = entry.format
                                && format.is_import()
                            {
                                item_content = item_content.child(
                                    div()
                                        .px_1()
                                        .flex_shrink_0()
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
                                    Button::new(SharedString::from(format!("remove-recent-file-{index}")))
                                        .ghost()
                                        .icon(IconName::Close)
                                        .with_size(Size::XSmall)
                                        .tooltip("Remove from recents")
                                        .on_click(cx.listener(move |_, _, window, cx| {
                                            window.dispatch_action(Box::new(crate::actions::RemoveFileFromHistory { path: remove_path.clone() }), cx);
                                        })),
                                )
                                .into_any_element()
                        })
                        .collect::<Vec<_>>();

                    empty_actions = empty_actions.child(
                        h_flex().w_full().min_h_0().justify_start().child(
                            v_flex()
                                .id("recent-files-scroll")
                                .w(relative(0.95))
                                .max_h(px(360.0))
                                .overflow_y_scroll()
                                .mt_4()
                                .pt_3()
                                .border_t_1()
                                .border_color(theme.border)
                                .items_start()
                                .gap_1()
                                .child(div().px_1().text_xs().font_semibold().text_color(theme.muted_foreground).child("Recents"))
                                .children(recent_items),
                        ),
                    );
                }

                crate::ui::panels::panel_empty_state(
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
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .size_full()
                                    .min_w_0()
                                    .child(Icon::new(icon).size_3p5().flex_shrink_0())
                                    .child(div().flex_1().min_w_0().truncate().whitespace_nowrap().child(item.label.clone()))
                                    .context_menu({
                                        let view = view.clone();
                                        let item_id = item.id.clone();
                                        let item_label = item.label.clone();
                                        let is_folder = item.is_folder();
                                        let context_focus = focus_handle.clone();
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

                                            let item_path_str = item_id.to_string();
                                            let item_name_str = item_label.to_string();
                                            let mut menu = menu.action_context(context_focus.clone());

                                            if is_folder {
                                                menu = menu
                                                    .menu_with_icon(
                                                        "Open as Root Folder",
                                                        IconName::FolderOpen,
                                                        Box::new(crate::actions::SetFileTreeFolder { path: item_path_str.clone() }),
                                                    )
                                                    .menu_with_icon(
                                                        "Refresh Folder",
                                                        IconName::Redo,
                                                        Box::new(RefreshTreeFolder { path: item_path_str.clone() }),
                                                    );
                                            } else {
                                                let open_path = item_path_str.clone();
                                                let open_path_b64 = open_path.clone();
                                                menu = menu
                                                    .menu_with_icon("Open", IconName::FolderOpen, Box::new(OpenFile::new(item_path_str.clone())))
                                                    .submenu("Import", window, cx, move |menu, _window, _cx| {
                                                        menu.menu(
                                                            "Motorola S-Record / Intel HEX",
                                                            Box::new(OpenFile::with_format(open_path.clone(), Some(crate::core::format::FileFormat::HexOrMot))),
                                                        )
                                                        .menu(
                                                            "Base64",
                                                            Box::new(OpenFile::with_format(
                                                                open_path_b64.clone(),
                                                                Some(crate::core::format::FileFormat::Base64),
                                                            )),
                                                        )
                                                    })
                                                    .separator();

                                                if let Some(ref pending_path) = pending_compare
                                                    && pending_path != &item_path_str
                                                {
                                                    let pending_file_name = path_file_name(Path::new(pending_path));
                                                    menu = menu.menu_with_icon(
                                                        format!("Compare with '{pending_file_name}'"),
                                                        IconName::GitCompare,
                                                        Box::new(OpenDiff {
                                                            left_path: pending_path.clone(),
                                                            right_path: item_path_str.clone(),
                                                        }),
                                                    );
                                                }

                                                menu = menu.menu_with_icon(
                                                    "Select for Compare",
                                                    IconName::GitCompare,
                                                    Box::new(SelectForCompare { path: item_path_str.clone() }),
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
                                                .menu_with_icon("Copy Path", IconName::Copy, Box::new(CopyTreeItemPath { path: item_path_str.clone() }))
                                                .menu_with_icon(
                                                    if is_folder { "Copy Folder Name" } else { "Copy File Name" },
                                                    IconName::FileText,
                                                    Box::new(CopyTreeItemName { name: item_name_str }),
                                                )
                                                .menu_with_icon(
                                                    if cfg!(target_os = "macos") {
                                                        "Reveal in Finder"
                                                    } else {
                                                        "Reveal in File Explorer"
                                                    },
                                                    IconName::FolderSearch,
                                                    Box::new(RevealTreeItem { path: item_path_str }),
                                                )
                                        }
                                    }),
                            )
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
                                        cx.emit(FileTreePanelEvent::OpenFile {
                                            path: PathBuf::from(item.id.to_string()),
                                            format: None,
                                            record_recent: true,
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
            .when_some(compare_banner, |el, banner| el.child(banner))
    }
}

impl EventEmitter<FileTreePanelEvent> for FileTreePanel {}

impl Focusable for FileTreePanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FileTreePanelState {
    pub root_path: Option<PathBuf>,
}

impl FileTreePanelState {
    #[allow(dead_code)]
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("serialize FileTreePanelState")
    }

    #[allow(dead_code)]
    pub fn from_value(value: serde_json::Value) -> Option<Self> {
        serde_json::from_value(value).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{RecentDisplayHistory, find_parent_item, set_item_expanded};
    use crate::core::format::FileFormat;
    use crate::core::recent_history::RecentFileEntry;
    use gpui_kit::component::tree::TreeItem;
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

    #[test]
    fn test_file_tree_expand_and_find_parent() {
        let mut items = vec![
            TreeItem::new("/dir", "dir").child(TreeItem::new("/dir/a.bin", "a.bin")),
            TreeItem::new("/root.bin", "root.bin"),
        ];

        assert!(!items[0].is_expanded());
        assert!(find_parent_item(&items, "/dir/a.bin").is_none());

        assert!(set_item_expanded(&mut items, "/dir", true));
        assert!(items[0].is_expanded());

        let parent = find_parent_item(&items, "/dir/a.bin");
        assert_eq!(parent.map(|p| p.id.to_string()), Some("/dir".to_string()));
        assert!(find_parent_item(&items, "/root.bin").is_none());
        assert!(!set_item_expanded(&mut items, "/missing", true));
    }

    #[test]
    fn test_file_tree_panel_event_open_file_record_recent_flag() {
        use super::FileTreePanelEvent;

        let recents_event = FileTreePanelEvent::OpenFile {
            path: PathBuf::from("recent.bin"),
            format: None,
            record_recent: false,
        };
        match recents_event {
            FileTreePanelEvent::OpenFile { record_recent, .. } => assert!(!record_recent),
        }

        let tree_event = FileTreePanelEvent::OpenFile {
            path: PathBuf::from("tree.bin"),
            format: None,
            record_recent: true,
        };
        match tree_event {
            FileTreePanelEvent::OpenFile { record_recent, .. } => assert!(record_recent),
        }
    }
}
