use crate::core::appearance::Appearance;
use crate::core::editor::Editor;
use crate::core::radix::DisplayRadix;
use crate::core::structure::{ParseResult, ParsedField, format_parse_result_as_text, format_parse_result_as_yaml};
use crate::ui::components::data_table::{TableColumn, VirtualTable, VirtualTableState};
use crate::ui::icon::IconName;
use crate::ui::style::{format_size_friendly, format_with_commas};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, StyledExt as _, WindowExt as _, button::ButtonVariants as _, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

actions!(
    struct_tree,
    [MoveUp, MoveDown, MoveTop, MoveBottom, PageUp, PageDown, ToggleExpand, Expand, Collapse]
);

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = struct_tree)]
pub struct CopyFieldValue {
    pub value: String,
}

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = struct_tree)]
pub struct CopyFieldName {
    pub name: String,
}

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = struct_tree)]
pub struct CopyFieldOffset {
    pub offset: String,
}

const CONTEXT: &str = "StructTreeView";
const TREE_INDICATOR_WIDTH: f32 = 12.0;
const TREE_INDENT_WIDTH: f32 = 14.0;
const COLUMN_GAP: f32 = 4.0;
const STRUCTURE_ROW_HEIGHT: f32 = 28.0;
const FIELD_COLUMN_WIDTH: f32 = 150.0;
const ADDRESS_COLUMN_WIDTH: f32 = 78.0;
const TYPE_COLUMN_WIDTH: f32 = 108.0;
const SIZE_COLUMN_WIDTH: f32 = 58.0;
const AUTOFIT_HORIZONTAL_PADDING: f32 = 16.0;
const AUTOFIT_MIN_WIDTH: f32 = 52.0;
const AUTOFIT_MAX_WIDTH: f32 = 360.0;
const AUTOFIT_CHAR_WIDTH: f32 = 7.2;
const AUTOFIT_WIDE_CHAR_WIDTH: f32 = 13.0;
const AUTOFIT_MAX_TEXT_CHARS: usize = 128;

fn default_structure_columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("field", "Field", px(FIELD_COLUMN_WIDTH)).min_width(px(60.0)).resizable(true),
        TableColumn::new("address", "Address", px(ADDRESS_COLUMN_WIDTH))
            .min_width(px(50.0))
            .resizable(true)
            .visible(true),
        TableColumn::new("type", "Type", px(TYPE_COLUMN_WIDTH))
            .min_width(px(50.0))
            .resizable(true)
            .visible(true),
        TableColumn::new("size", "Size", px(SIZE_COLUMN_WIDTH))
            .min_width(px(40.0))
            .resizable(true)
            .visible(true),
        TableColumn::new("value", "Value", px(180.0)).min_width(px(60.0)).resizable(true),
    ]
}

fn path_from_string_file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".to_string())
}

fn default_yaml_file_name(definition_id: &str) -> String {
    let mut name: String = definition_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name == "." || name == ".." {
        name = "structure".to_string();
    }
    if !name.ends_with(".yaml") && !name.ends_with(".yml") {
        name.push_str(".yaml");
    }
    name
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, Some("StructTreeView && !Input")),
        KeyBinding::new("down", MoveDown, Some("StructTreeView && !Input")),
        KeyBinding::new("k", MoveUp, Some("StructTreeView && !Input")),
        KeyBinding::new("j", MoveDown, Some("StructTreeView && !Input")),
        KeyBinding::new("home", MoveTop, Some("StructTreeView && !Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-home", MoveTop, Some("StructTreeView && !Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-home", MoveTop, Some("StructTreeView && !Input")),
        KeyBinding::new("end", MoveBottom, Some("StructTreeView && !Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-end", MoveBottom, Some("StructTreeView && !Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-end", MoveBottom, Some("StructTreeView && !Input")),
        KeyBinding::new("pageup", PageUp, Some("StructTreeView && !Input")),
        KeyBinding::new("pagedown", PageDown, Some("StructTreeView && !Input")),
        KeyBinding::new("space", ToggleExpand, Some("StructTreeView && !Input")),
        KeyBinding::new("enter", ToggleExpand, Some("StructTreeView && !Input")),
        KeyBinding::new("right", Expand, Some("StructTreeView && !Input")),
        KeyBinding::new("left", Collapse, Some("StructTreeView && !Input")),
        KeyBinding::new("l", Expand, Some("StructTreeView && !Input")),
        KeyBinding::new("h", Collapse, Some("StructTreeView && !Input")),
    ]);
}

#[allow(dead_code)]
pub enum StructTreeViewEvent {
    NavigateTo { offset: usize, size: usize },
}

pub struct StructTreeView {
    pub parse_result: Option<std::sync::Arc<ParseResult>>,
    pub flattened_fields: Vec<FlattenedField>,
    pub expanded_paths: HashSet<Vec<usize>>,
    pub editor: Option<Entity<Editor>>,
    pub table_state: VirtualTableState,
    pub focus_handle: FocusHandle,
    pub selected_index: Option<usize>,
    pub recent_definition_paths: Vec<PathBuf>,
    last_container_width: Pixels,
    value_radix: DisplayRadix,
    last_parse_id: Option<(String, usize, usize, usize, usize, bool)>,
    last_selection_cursor: Option<usize>,
    last_selection_scan_len: usize,
    export_status: StructureExportStatus,
    export_request_id: u64,
    _editor_subscription: Option<Subscription>,
}

#[derive(Clone, Default)]
enum StructureExportStatus {
    #[default]
    Idle,
    Copying,
    Exporting,
    Success(String),
    Error(String),
}

#[derive(Clone)]
pub struct FlattenedField {
    pub path: Vec<usize>,
    pub offset: usize,
    pub size: usize,
    pub depth: usize,
    pub has_children: bool,
    pub is_collapsed: bool,
}

impl EventEmitter<StructTreeViewEvent> for StructTreeView {}

impl StructTreeView {
    pub fn new(
        parse_result: Option<std::sync::Arc<crate::core::structure::types::ParseResult>>,
        editor: Option<Entity<Editor>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let expanded_paths = HashSet::new();
        let mut flattened = Vec::new();
        if let Some(ref res) = parse_result {
            Self::flatten_fields(res.fields.iter(), 0, &[], &expanded_paths, &mut flattened, 0);
        }
        let table_focus_handle = cx.focus_handle();
        let table_state = VirtualTableState::new("struct-tree-table", default_structure_columns(), table_focus_handle);
        let focus_handle = cx.focus_handle();

        let mut this = Self {
            parse_result,
            flattened_fields: flattened,
            expanded_paths,
            editor: editor.clone(),
            table_state,
            focus_handle,
            selected_index: None,
            recent_definition_paths: Vec::new(),
            last_container_width: px(300.0),
            value_radix: DisplayRadix::Decimal,
            last_parse_id: None,
            last_selection_cursor: None,
            last_selection_scan_len: 0,
            export_status: StructureExportStatus::default(),
            export_request_id: 0,
            _editor_subscription: None,
        };

        if let Some(ed) = editor {
            this._editor_subscription = Some(cx.observe(&ed, |this, editor, cx| {
                this.sync_fields(&editor, cx);
            }));
            this.sync_fields(&ed, cx);
        }

        this
    }

    pub fn set_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        let same_editor = match (&self.editor, &editor) {
            (Some(current), Some(next)) => current.entity_id() == next.entity_id(),
            (None, None) => true,
            _ => false,
        };
        if same_editor {
            return;
        }

        self._editor_subscription = None;
        self.editor = editor.clone();

        if let Some(ed) = &editor {
            self._editor_subscription = Some(cx.observe(ed, |this, editor, cx| {
                this.sync_fields(&editor, cx);
            }));
            self.sync_fields(ed, cx);
        } else {
            self.last_parse_id = None;
            self.set_parse_result(None, cx);
        }
        cx.notify();
    }

    /// Updates the recent structure definition paths displayed in the empty state.
    pub fn set_definition_history(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        if self.recent_definition_paths == paths {
            return;
        }

        self.recent_definition_paths = paths.to_vec();
        cx.notify();
    }

    fn sync_fields(&mut self, editor: &Entity<Editor>, cx: &mut Context<Self>) {
        let (current_parse_id, cursor_offset) = {
            let editor_lock = editor.read(cx);
            let doc_version = editor_lock.document.read().ok().map(|d| d.history.version()).unwrap_or(0);
            let parse_id = editor_lock.parse_result().map(|r| {
                (
                    r.definition_id.clone(),
                    r.total_parsed_bytes,
                    r.fields.len(),
                    doc_version,
                    editor_lock.structure.generation,
                    editor_lock.structure.is_parsing,
                )
            });
            (parse_id, editor_lock.cursor.offset)
        };

        if current_parse_id != self.last_parse_id {
            let parse_res = editor.read(cx).parse_result();
            let can_append = match (&self.last_parse_id, &current_parse_id, &self.parse_result) {
                (Some(previous), Some(current), Some(existing)) => {
                    previous.3 == current.3 && previous.4 == current.4 && current.2 >= previous.2 && existing.fields.len() == previous.2
                }
                _ => false,
            };
            if can_append {
                let previous_field_count = self.last_parse_id.as_ref().map(|parse_id| parse_id.2).unwrap_or(0);
                let old = std::mem::replace(&mut self.parse_result, parse_res);
                if let Some(old) = old {
                    Self::release_parse_result_in_background(old, cx);
                }
                if let Some(ref res) = self.parse_result
                    && current_parse_id.as_ref().map(|parse_id| parse_id.2).unwrap_or(0) > previous_field_count
                {
                    Self::flatten_fields(
                        res.fields.iter_from(previous_field_count),
                        0,
                        &[],
                        &self.expanded_paths,
                        &mut self.flattened_fields,
                        previous_field_count,
                    );
                }
                cx.notify();
            } else {
                self.set_parse_result(parse_res, cx);
            }
            self.last_parse_id = current_parse_id;
        }

        self.sync_selected_index_from_cursor(cursor_offset, cx);
    }

    fn sync_selected_index_from_cursor(&mut self, cursor_offset: usize, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            self.last_selection_cursor = Some(cursor_offset);
            self.last_selection_scan_len = 0;
            return;
        }

        // If current selected item already covers cursor_offset, keep it
        if let Some(curr_idx) = self.selected_index
            && let Some(field) = self.flattened_fields.get(curr_idx)
        {
            let end = field.offset + field.size;
            let matches = if field.size > 0 {
                cursor_offset >= field.offset && cursor_offset < end
            } else {
                cursor_offset == field.offset
            };
            if matches {
                self.last_selection_cursor = Some(cursor_offset);
                self.last_selection_scan_len = self.flattened_fields.len();
                return;
            }
        }

        let cursor_changed = self.last_selection_cursor != Some(cursor_offset);
        let scan_start = if !cursor_changed && self.selected_index.is_none() && self.last_selection_scan_len <= self.flattened_fields.len() {
            self.last_selection_scan_len
        } else {
            0
        };
        if scan_start == self.flattened_fields.len() {
            return;
        }

        let mut best_match: Option<(usize, bool, usize, usize)> = None; // (index, is_leaf, depth, size)
        for (i, field) in self.flattened_fields.iter().enumerate().skip(scan_start) {
            let end = field.offset + field.size;
            let matches = if field.size > 0 {
                cursor_offset >= field.offset && cursor_offset < end
            } else {
                cursor_offset == field.offset
            };

            if matches {
                let is_leaf = !field.has_children;
                match best_match {
                    None => {
                        best_match = Some((i, is_leaf, field.depth, field.size));
                    }
                    Some((_, best_is_leaf, best_depth, best_size)) => {
                        // Prefer leaf nodes over container nodes, then deeper nodes, then smaller size
                        if (is_leaf && !best_is_leaf)
                            || (is_leaf == best_is_leaf && field.depth > best_depth)
                            || (is_leaf == best_is_leaf && field.depth == best_depth && field.size < best_size)
                        {
                            best_match = Some((i, is_leaf, field.depth, field.size));
                        }
                    }
                }
            }
        }

        self.last_selection_cursor = Some(cursor_offset);
        self.last_selection_scan_len = self.flattened_fields.len();
        if let Some((idx, _, _, _)) = best_match
            && self.selected_index != Some(idx)
        {
            self.selected_index = Some(idx);
            self.table_state.scroll_to_row(idx, ScrollStrategy::Top);
            cx.notify();
        } else if best_match.is_none() {
            self.selected_index = None;
        }
    }

    pub fn set_parse_result(&mut self, parse_result: Option<std::sync::Arc<crate::core::structure::types::ParseResult>>, cx: &mut Context<Self>) {
        let old_expanded_paths = std::mem::take(&mut self.expanded_paths);
        let old_flattened_fields = std::mem::take(&mut self.flattened_fields);
        let old_parse_result = std::mem::replace(&mut self.parse_result, parse_result);
        self.export_status = StructureExportStatus::default();
        self.export_request_id = self.export_request_id.wrapping_add(1);

        // Replacing a completed parse can release a very large tree and a
        // large flattened panel cache. Keep the UI thread responsible only
        // for the cheap state swap; the old snapshot and panel caches are
        // owned by this detached background task until they are fully
        // released.
        if old_parse_result.is_some() || !old_flattened_fields.is_empty() || !old_expanded_paths.is_empty() {
            cx.background_executor()
                .spawn(async move {
                    drop((old_parse_result, old_flattened_fields, old_expanded_paths));
                })
                .detach();
        }
        self.rebuild_flattened();
        self.selected_index = None;
        self.last_selection_cursor = None;
        self.last_selection_scan_len = 0;
        self.table_state.scroll_to_row(0, ScrollStrategy::Top);
        cx.notify();
    }

    fn release_parse_result_in_background(result: std::sync::Arc<crate::core::structure::types::ParseResult>, cx: &mut Context<Self>) {
        cx.background_executor()
            .spawn(async move {
                drop(result);
            })
            .detach();
    }

    fn can_export_result(&self) -> bool {
        self.parse_result.as_ref().is_some_and(|result| !result.fields.is_empty())
    }

    fn export_is_busy(&self) -> bool {
        matches!(self.export_status, StructureExportStatus::Copying | StructureExportStatus::Exporting)
    }

    fn copy_structure_result(&mut self, _: &crate::actions::CopyStructureResult, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_is_busy() || !self.can_export_result() {
            return;
        }

        let Some(parse_result) = self.parse_result.clone() else {
            return;
        };
        let request_id = self.export_request_id.wrapping_add(1);
        self.export_request_id = request_id;
        self.export_status = StructureExportStatus::Copying;
        cx.notify();

        let field_count = parse_result.fields.len();
        let task = cx.background_executor().spawn(async move { format_parse_result_as_text(&parse_result) });
        let view = cx.entity().clone();
        cx.spawn_in(window, async move |_, window| {
            let text = task.await;
            let _ = window.update(|window, cx| {
                let applied = view.update(cx, |this, cx| {
                    if this.export_request_id != request_id {
                        return false;
                    }
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    this.export_status = StructureExportStatus::Success(format!("Copied {field_count} root fields to clipboard"));
                    cx.notify();
                    true
                });
                if applied {
                    window.push_notification(
                        gpui_kit::component::notification::Notification::success("Structure analysis copied to clipboard"),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    fn copy_field_value(&mut self, action: &CopyFieldValue, _window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
    }

    fn copy_field_name(&mut self, action: &CopyFieldName, _window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.name.clone()));
    }

    fn copy_field_offset(&mut self, action: &CopyFieldOffset, _window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.offset.clone()));
    }

    fn export_structure_yaml(&mut self, _: &crate::actions::ExportStructureYaml, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_is_busy() || !self.can_export_result() {
            return;
        }

        let Some(parse_result) = self.parse_result.clone() else {
            return;
        };
        let (parent_dir, target_file_name) = self
            .editor
            .as_ref()
            .and_then(|ed| {
                let doc = ed.read(cx).document.read().ok()?;
                let path = doc.path();
                let dir = path.parent().filter(|p| p.exists()).map(|p| p.to_path_buf());
                let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                Some((dir, file_name))
            })
            .unwrap_or((None, None));

        let parent_dir = parent_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        let default_file_name = if let Some(name) = target_file_name
            && !name.is_empty()
            && name != "Untitled"
            && name != "untitled"
        {
            format!("{name}.{}.yaml", parse_result.definition_id)
        } else {
            default_yaml_file_name(&parse_result.definition_id)
        };

        let prompt = cx.prompt_for_new_path(&parent_dir, Some(&default_file_name));

        let request_id = self.export_request_id.wrapping_add(1);
        self.export_request_id = request_id;
        self.export_status = StructureExportStatus::Exporting;
        cx.notify();

        let executor = cx.background_executor().clone();
        let view = cx.entity().clone();
        cx.spawn_in(window, async move |_, window| {
            let selected_path = prompt.await.ok().and_then(|result| result.ok()).flatten();
            let Some(mut path) = selected_path else {
                let _ = window.update(|_, cx| {
                    view.update(cx, |this, cx| {
                        if this.export_request_id == request_id {
                            this.export_status = StructureExportStatus::Idle;
                            cx.notify();
                        }
                    });
                });
                return;
            };

            let export_task = executor.spawn(async move {
                if path.extension().is_none() {
                    path.set_extension("yaml");
                }

                let yaml = format_parse_result_as_yaml(&parse_result).map_err(|error| error.to_string())?;
                std::fs::write(&path, yaml).map_err(|error| format!("{}: {error}", path.display()))?;
                Ok::<PathBuf, String>(path)
            });
            let result = export_task.await;

            let _ = window.update(|window, cx| {
                let applied = view.update(cx, |this, cx| {
                    if this.export_request_id != request_id {
                        return false;
                    }
                    match &result {
                        Ok(path) => {
                            this.export_status = StructureExportStatus::Success(format!("Exported YAML to {}", path.display()));
                        }
                        Err(error) => {
                            this.export_status = StructureExportStatus::Error(format!("YAML export failed: {error}"));
                        }
                    }
                    cx.notify();
                    true
                });

                if applied {
                    match &result {
                        Ok(path) => window.push_notification(
                            gpui_kit::component::notification::Notification::success(format!("Structure YAML exported to {}", path.display())),
                            cx,
                        ),
                        Err(error) => window.push_notification(gpui_kit::component::notification::Notification::error(error.clone()), cx),
                    }
                }
            });
        })
        .detach();
    }

    fn on_action_copy_structure_result(&mut self, action: &crate::actions::CopyStructureResult, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_structure_result(action, window, cx);
    }

    fn on_action_export_structure_yaml(&mut self, action: &crate::actions::ExportStructureYaml, window: &mut Window, cx: &mut Context<Self>) {
        self.export_structure_yaml(action, window, cx);
    }

    fn rebuild_flattened(&mut self) {
        let mut flattened = Vec::new();
        if let Some(ref res) = self.parse_result {
            Self::flatten_fields(res.fields.iter(), 0, &[], &self.expanded_paths, &mut flattened, 0);
        }
        self.flattened_fields = flattened;
        self.last_selection_cursor = None;
        self.last_selection_scan_len = 0;
    }

    fn flatten_fields<'a, I>(
        fields: I,
        depth: usize,
        parent_path: &[usize],
        expanded_paths: &HashSet<Vec<usize>>,
        results: &mut Vec<FlattenedField>,
        index_offset: usize,
    ) where
        I: IntoIterator<Item = &'a ParsedField>,
    {
        struct Frame<'a, I> {
            root: Option<I>,
            children: Option<std::slice::Iter<'a, ParsedField>>,
            depth: usize,
            parent_path: Vec<usize>,
            index_offset: usize,
            next_index: usize,
        }

        let mut frames = vec![Frame {
            root: Some(fields.into_iter()),
            children: None,
            depth,
            parent_path: parent_path.to_vec(),
            index_offset,
            next_index: 0,
        }];

        while !frames.is_empty() {
            let Some((relative_idx, field, frame_depth, frame_index_offset, parent_path)) = ({
                let frame = frames.last_mut().expect("flattening stack must not be empty");
                let next = if let Some(root) = frame.root.as_mut() {
                    root.next().map(|field| (frame.next_index, field))
                } else if let Some(children) = frame.children.as_mut() {
                    children.next().map(|field| (frame.next_index, field))
                } else {
                    None
                };

                next.map(|(relative_idx, field)| {
                    frame.next_index += 1;
                    (relative_idx, field, frame.depth, frame.index_offset, frame.parent_path.clone())
                })
            }) else {
                frames.pop();
                continue;
            };

            let idx = relative_idx + frame_index_offset;
            let mut current_path = parent_path;
            current_path.push(idx);

            let has_children = !field.children.is_empty();
            let is_expanded = expanded_paths.contains(&current_path);

            results.push(FlattenedField {
                path: current_path.clone(),
                offset: field.offset,
                size: field.size,
                depth: frame_depth,
                has_children,
                is_collapsed: !is_expanded,
            });

            if has_children && is_expanded {
                frames.push(Frame {
                    root: None,
                    children: Some(field.children.iter()),
                    depth: frame_depth + 1,
                    parent_path: current_path,
                    index_offset: 0,
                    next_index: 0,
                });
            }
        }
    }

    pub fn field_at_path<'a>(&'a self, path: &[usize]) -> Option<&'a ParsedField> {
        let res = self.parse_result.as_ref()?;
        let (&first, rest) = path.split_first()?;
        let mut target_field = res.fields.get(first)?;
        for &idx in rest {
            target_field = target_field.children.get(idx)?;
        }
        Some(target_field)
    }

    #[allow(dead_code)]
    pub fn get_field_at_path<'a>(&'a self, path: &[usize]) -> Option<&'a ParsedField> {
        self.field_at_path(path)
    }

    fn find_first_leaf_offset(field: &ParsedField) -> usize {
        let mut current = field;
        while let Some(first_child) = current.children.first() {
            current = first_child;
        }
        current.offset
    }

    fn select_item(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx >= self.flattened_fields.len() {
            return;
        }

        self.selected_index = Some(idx);
        self.table_state.scroll_to_row(idx, ScrollStrategy::Top);

        let item = &self.flattened_fields[idx];
        let offset = if item.size == 0 && item.has_children {
            if let Some(parsed_field) = self.field_at_path(&item.path) {
                Self::find_first_leaf_offset(parsed_field)
            } else {
                item.offset
            }
        } else {
            item.offset
        };

        let mut nav_offset = offset;
        if let Some(editor) = &self.editor {
            editor.update(cx, |editor, cx| {
                let total = editor.total_size();
                if total > 0 && item.size > 0 {
                    let start = item.offset.min(total.saturating_sub(1));
                    let end = (item.offset + item.size.saturating_sub(1)).min(total.saturating_sub(1));
                    editor.set_selection_range(start..end.saturating_add(1));
                    editor.cursor.offset = start;
                    nav_offset = start;
                } else {
                    let clamped = offset.min(total);
                    editor.set_cursor_offset_exact(clamped);
                    nav_offset = clamped;
                }
                cx.notify();
            });
        }

        cx.emit(StructTreeViewEvent::NavigateTo {
            offset: nav_offset,
            size: item.size,
        });
        cx.notify();
    }

    fn toggle_collapse_at(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(field) = self.flattened_fields.get(idx) {
            if !field.has_children {
                return;
            }

            let path = field.path.clone();
            if self.expanded_paths.contains(&path) {
                self.expanded_paths.remove(&path);
            } else {
                self.expanded_paths.insert(path);
            }

            self.rebuild_flattened();

            // Maintain selection index if possible by path
            if idx < self.flattened_fields.len() {
                self.selected_index = Some(idx);
            } else if !self.flattened_fields.is_empty() {
                self.selected_index = Some(self.flattened_fields.len() - 1);
            }

            cx.notify();
        }
    }

    fn move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            return;
        }

        let next_idx = match self.selected_index {
            Some(idx) => idx.saturating_sub(1),
            None => 0,
        };

        self.select_item(next_idx, cx);
    }

    fn move_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            return;
        }

        let max_idx = self.flattened_fields.len() - 1;
        let next_idx = match self.selected_index {
            Some(idx) => (idx + 1).min(max_idx),
            None => 0,
        };

        self.select_item(next_idx, cx);
    }

    fn move_top(&mut self, _: &MoveTop, _window: &mut Window, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            return;
        }
        self.select_item(0, cx);
    }

    fn move_bottom(&mut self, _: &MoveBottom, _window: &mut Window, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            return;
        }
        let last_idx = self.flattened_fields.len().saturating_sub(1);
        self.select_item(last_idx, cx);
    }

    fn page_up(&mut self, _: &PageUp, _window: &mut Window, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            return;
        }
        let current = self.selected_index.unwrap_or(0);
        let next_idx = current.saturating_sub(10);
        self.select_item(next_idx, cx);
    }

    fn page_down(&mut self, _: &PageDown, _window: &mut Window, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            return;
        }
        let current = self.selected_index.unwrap_or(0);
        let last_idx = self.flattened_fields.len().saturating_sub(1);
        let next_idx = (current + 10).min(last_idx);
        self.select_item(next_idx, cx);
    }

    fn toggle_expand(&mut self, _: &ToggleExpand, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(idx) = self.selected_index {
            self.toggle_collapse_at(idx, cx);
        }
    }

    fn expand(&mut self, _: &Expand, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(idx) = self.selected_index
            && let Some(field) = self.flattened_fields.get(idx)
            && field.has_children
            && field.is_collapsed
        {
            self.toggle_collapse_at(idx, cx);
        }
    }

    fn collapse(&mut self, _: &Collapse, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(idx) = self.selected_index
            && let Some(field) = self.flattened_fields.get(idx)
        {
            if field.has_children && !field.is_collapsed {
                self.toggle_collapse_at(idx, cx);
            } else if field.depth > 0 {
                // Move to parent field
                let target_depth = field.depth - 1;
                for p_idx in (0..idx).rev() {
                    if self.flattened_fields[p_idx].depth == target_depth {
                        self.select_item(p_idx, cx);
                        break;
                    }
                }
            }
        }
    }

    pub fn collapse_all(&mut self, cx: &mut Context<Self>) {
        self.expanded_paths.clear();
        self.rebuild_flattened();
        self.selected_index = self.selected_index.map(|idx| idx.min(self.flattened_fields.len().saturating_sub(1)));
        cx.notify();
    }

    pub fn expand_all(&mut self, cx: &mut Context<Self>) {
        if let Some(ref res) = self.parse_result {
            fn collect_branches<'a>(fields: impl IntoIterator<Item = &'a ParsedField>, parent_path: Vec<usize>, out: &mut HashSet<Vec<usize>>) {
                for (idx, field) in fields.into_iter().enumerate() {
                    let mut path = parent_path.clone();
                    path.push(idx);
                    if !field.children.is_empty() {
                        out.insert(path.clone());
                        collect_branches(field.children.iter(), path, out);
                    }
                }
            }
            collect_branches(res.fields.iter(), Vec::new(), &mut self.expanded_paths);
        }
        self.rebuild_flattened();
        cx.notify();
    }

    fn on_action_expand_all(&mut self, _: &crate::actions::ExpandAllStructure, _window: &mut Window, cx: &mut Context<Self>) {
        self.expand_all(cx);
    }

    fn on_action_collapse_all(&mut self, _: &crate::actions::CollapseAllStructure, _window: &mut Window, cx: &mut Context<Self>) {
        self.collapse_all(cx);
    }

    fn on_action_toggle_structure_address_column(&mut self, _: &crate::actions::ToggleStructureAddressColumn, _window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.table_state.column(1).map(|c| c.visible).unwrap_or(true);
        self.table_state.set_column_visible(1, !visible);
        cx.notify();
    }

    fn on_action_toggle_structure_type_column(&mut self, _: &crate::actions::ToggleStructureTypeColumn, _window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.table_state.column(2).map(|c| c.visible).unwrap_or(false);
        self.table_state.set_column_visible(2, !visible);
        cx.notify();
    }

    fn on_action_toggle_structure_size_column(&mut self, _: &crate::actions::ToggleStructureSizeColumn, _window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.table_state.column(3).map(|c| c.visible).unwrap_or(false);
        self.table_state.set_column_visible(3, !visible);
        cx.notify();
    }

    fn on_action_toggle_structure_value_column(&mut self, _: &crate::actions::ToggleStructureValueColumn, _window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.table_state.column(4).map(|c| c.visible).unwrap_or(true);
        self.table_state.set_column_visible(4, !visible);
        cx.notify();
    }

    fn set_value_radix(&mut self, radix: DisplayRadix, cx: &mut Context<Self>) {
        if self.value_radix == radix {
            return;
        }

        self.value_radix = radix;
        cx.notify();
    }

    fn on_action_set_value_radix_hex(&mut self, _: &crate::actions::SetRadixHex, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_value_radix(DisplayRadix::Hexadecimal, cx);
    }

    fn on_action_set_value_radix_dec(&mut self, _: &crate::actions::SetRadixDec, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_value_radix(DisplayRadix::Decimal, cx);
    }

    fn on_action_set_value_radix_oct(&mut self, _: &crate::actions::SetRadixOct, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_value_radix(DisplayRadix::Octal, cx);
    }

    fn on_action_set_value_radix_bin(&mut self, _: &crate::actions::SetRadixBin, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_value_radix(DisplayRadix::Binary, cx);
    }

    fn estimated_text_width(text: &str) -> f32 {
        text.chars()
            .take(AUTOFIT_MAX_TEXT_CHARS)
            .map(|character| if character.is_ascii() { AUTOFIT_CHAR_WIDTH } else { AUTOFIT_WIDE_CHAR_WIDTH })
            .sum()
    }

    fn value_text(parsed_field: &ParsedField, radix: DisplayRadix) -> String {
        if parsed_field.is_struct() {
            format!(
                "{} child{}",
                parsed_field.children.len(),
                if parsed_field.children.len() == 1 { "" } else { "ren" }
            )
        } else if let Some(label) = &parsed_field.enum_label {
            format!("{} ({})", parsed_field.value.format_with_radix(radix), label)
        } else {
            parsed_field.value.format_with_radix(radix)
        }
    }

    fn auto_fit_column(&mut self, col_ix: usize, cx: &mut Context<Self>) {
        self.table_state.end_resize();
        let col = self.table_state.column(col_ix);
        let header = col.map(|c| c.name.to_string()).unwrap_or_default();
        let mut max_width = Self::estimated_text_width(&header);
        if col_ix == 0 {
            max_width += TREE_INDICATOR_WIDTH + COLUMN_GAP;
        }

        for field in self.flattened_fields.iter().take(128) {
            let Some(parsed_field) = self.field_at_path(&field.path) else {
                continue;
            };

            let text = match col_ix {
                0 => parsed_field.id.clone(),
                1 => format!("0x{:X}", parsed_field.offset),
                2 => {
                    if parsed_field.field_type.is_empty() {
                        if field.has_children { "struct".to_string() } else { "value".to_string() }
                    } else {
                        parsed_field.field_type.clone()
                    }
                }
                3 => format!("{} B", format_with_commas(field.size)),
                4 => Self::value_text(parsed_field, self.value_radix),
                _ => String::new(),
            };
            let mut text_width = Self::estimated_text_width(&text);
            if col_ix == 0 {
                text_width += field.depth as f32 * TREE_INDENT_WIDTH + TREE_INDICATOR_WIDTH + COLUMN_GAP;
            }
            max_width = max_width.max(text_width);
        }

        let width = (max_width + AUTOFIT_HORIZONTAL_PADDING).clamp(AUTOFIT_MIN_WIDTH, AUTOFIT_MAX_WIDTH);
        self.table_state.set_column_width(col_ix, px(width));
        cx.notify();
    }

    #[allow(clippy::too_many_arguments)]
    fn render_list_item(
        ix: usize,
        field: &FlattenedField,
        parsed_field: &ParsedField,
        is_selected: bool,
        is_focused: bool,
        visible_cols: &[(usize, Pixels)],
        total_visible_width: Pixels,
        scroll_offset_x: Pixels,
        value_radix: DisplayRadix,
        view: Entity<Self>,
        focus_handle: &FocusHandle,
        window: &mut Window,
        cx: &App,
    ) -> AnyElement {
        let theme = cx.theme();
        let font_family = cx.global::<Appearance>().font_family.clone();
        let bg_color = if is_selected {
            if is_focused { theme.selection } else { theme.muted_foreground.opacity(0.3) }
        } else {
            hsla(0.0, 0.0, 0.0, 0.0)
        };
        let hover_color = if is_selected {
            theme.selection.opacity(0.82)
        } else {
            theme.muted.opacity(0.42)
        };

        let focus_handle_left = focus_handle.clone();
        let focus_handle_right = focus_handle.clone();
        let id = SharedString::from(parsed_field.id.clone());
        let type_name = if parsed_field.field_type.is_empty() {
            if field.has_children { "struct" } else { "value" }
        } else {
            parsed_field.field_type.as_str()
        };
        let type_label = SharedString::from(type_name.to_string());
        let value = SharedString::from(Self::value_text(parsed_field, value_radix));
        let chevron_symbol = if field.has_children {
            if field.is_collapsed { "▶" } else { "▼" }
        } else {
            " "
        };

        let field_offset = parsed_field.offset;

        h_flex()
            .id(ix)
            .items_center()
            .w_full()
            .h(px(STRUCTURE_ROW_HEIGHT))
            .flex_shrink_0()
            .overflow_hidden()
            .bg(bg_color)
            .hover(|style| style.bg(hover_color))
            .on_mouse_down(
                MouseButton::Left,
                window.listener_for(&view, move |this, _event: &MouseDownEvent, window, cx| {
                    focus_handle_left.focus(window, cx);
                    this.select_item(ix, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                window.listener_for(&view, move |this, _event: &MouseDownEvent, window, cx| {
                    focus_handle_right.focus(window, cx);
                    this.select_item(ix, cx);
                }),
            )
            .child(
                h_flex()
                    .w(total_visible_width)
                    .h_full()
                    .ml(-scroll_offset_x)
                    .children(visible_cols.iter().enumerate().map(|(vis_ix, &(col_ix, width))| {
                        let is_first = vis_ix == 0;
                        let cell_content = match col_ix {
                            0 => h_flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    h_flex()
                                        .flex_shrink_0()
                                        .w(px(TREE_INDENT_WIDTH * field.depth as f32 + TREE_INDICATOR_WIDTH))
                                        .h(px(TREE_INDICATOR_WIDTH))
                                        .items_center()
                                        .justify_end()
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .w(px(TREE_INDICATOR_WIDTH))
                                                .h(px(TREE_INDICATOR_WIDTH))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .cursor_pointer()
                                                .child(chevron_symbol)
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    window.listener_for(&view, move |this, _event: &MouseDownEvent, _window, cx| {
                                                        this.toggle_collapse_at(ix, cx);
                                                    }),
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .whitespace_nowrap()
                                        .text_sm()
                                        .font_medium()
                                        .text_color(theme.foreground)
                                        .child(id.clone()),
                                )
                                .into_any_element(),
                            1 => div()
                                .text_xs()
                                .font_family(font_family.clone())
                                .text_color(if is_selected { theme.accent_foreground } else { theme.muted_foreground })
                                .child(format!("0x{:X}", field_offset))
                                .into_any_element(),
                            2 => div()
                                .rounded_sm()
                                .bg(theme.accent.opacity(0.14))
                                .text_xs()
                                .font_family(font_family.clone())
                                .text_color(theme.accent_foreground)
                                .child(type_label.clone())
                                .into_any_element(),
                            3 => div()
                                .text_xs()
                                .font_family(font_family.clone())
                                .text_color(if is_selected { theme.accent_foreground } else { theme.muted_foreground })
                                .child(format!("{} B", format_with_commas(field.size)))
                                .into_any_element(),
                            4 => div().text_xs().text_color(theme.muted_foreground).child(value.clone()).into_any_element(),
                            _ => div().into_any_element(),
                        };
                        VirtualTable::render_data_cell(col_ix, width, is_first, theme.border.opacity(0.35), cell_content)
                    })),
            )
            .into_any_element()
    }
}

impl Render for StructTreeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let table_overlay = VirtualTable::render_table_overlay(&self.table_state, cx, |this| &mut this.table_state);

        let view = cx.entity().clone();
        let is_parsing = self.editor.as_ref().is_some_and(|ed| ed.read(cx).structure.is_parsing);
        let is_empty = self.flattened_fields.is_empty() && self.parse_result.as_ref().is_none_or(|result| result.fields.is_empty());
        let show_parsing_placeholder = is_parsing && self.flattened_fields.is_empty();
        let is_focused = self.focus_handle.is_focused(window);
        let theme = cx.theme();
        let has_active_editor = self.editor.is_some();
        let parse_progress = self.editor.as_ref().and_then(|editor| {
            let editor = editor.read(cx);
            if !editor.structure.is_parsing && !editor.structure.is_finalizing {
                return None;
            }

            let total_bytes = if editor.structure.total_size > 0 {
                editor.structure.total_size
            } else {
                editor.total_size()
            };
            let progress = if total_bytes > 0 {
                (editor.structure.progress_offset as f32 / total_bytes as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            Some((progress, editor.structure.progress_offset, total_bytes, editor.structure.is_finalizing))
        });

        let has_structure = self.parse_result.as_ref().is_some_and(|result| !result.fields.is_empty()) || !self.flattened_fields.is_empty();
        let export_is_busy = self.export_is_busy();
        let definition_badge = self.parse_result.as_ref().map(|result| {
            crate::ui::style::panel_badge(result.definition_id.clone(), theme)
                .max_w(px(110.0))
                .truncate()
                .into_any_element()
        });

        let header_actions = if has_structure {
            Some(
                gpui_kit::component::button::Button::new("clear-structure-btn")
                    .ghost()
                    .icon(IconName::Eraser)
                    .with_size(gpui_kit::component::Size::XSmall)
                    .disabled(export_is_busy)
                    .tooltip("Clear Structure Definition")
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(crate::actions::ClearStructureDefinition), cx);
                    }))
                    .into_any_element(),
            )
        } else if has_active_editor {
            Some(
                gpui_kit::component::button::Button::new("load-structure-header-btn")
                    .ghost()
                    .icon(IconName::FolderOpen)
                    .with_size(gpui_kit::component::Size::XSmall)
                    .tooltip(if cfg!(target_os = "macos") {
                        "Load Structure Definition (cmd-shift-s)"
                    } else {
                        "Load Structure Definition (ctrl-shift-s)"
                    })
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(crate::actions::LoadStructureDefinition), cx);
                    }))
                    .into_any_element(),
            )
        } else {
            None
        };

        let header = crate::ui::style::panel_header("STRUCTURE", is_focused, theme, definition_badge, header_actions);

        let structure_toolbar = if has_structure {
            Some(
                v_flex()
                    .w_full()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(theme.border.opacity(0.7))
                    .child(
                        h_flex()
                            .w_full()
                            .justify_end()
                            .items_center()
                            .gap_1()
                            .child(
                                gpui_kit::component::button::Button::new("expand-all-struct-btn")
                                    .ghost()
                                    .icon(IconName::ListTree)
                                    .with_size(gpui_kit::component::Size::XSmall)
                                    .tooltip("Expand all fields")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.expand_all(cx);
                                    })),
                            )
                            .child(
                                gpui_kit::component::button::Button::new("collapse-all-struct-btn")
                                    .ghost()
                                    .icon(IconName::Minimize)
                                    .with_size(gpui_kit::component::Size::XSmall)
                                    .tooltip("Collapse all fields")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.collapse_all(cx);
                                    })),
                            ),
                    )
                    .into_any_element(),
            )
        } else {
            None
        };

        let export_feedback = match &self.export_status {
            StructureExportStatus::Idle => None,
            StructureExportStatus::Copying => Some((IconName::LoaderCircle, "Preparing structure text...".to_string(), theme.accent)),
            StructureExportStatus::Exporting => Some((IconName::LoaderCircle, "Preparing YAML export...".to_string(), theme.accent)),
            StructureExportStatus::Success(message) => Some((IconName::Check, message.clone(), theme.green)),
            StructureExportStatus::Error(message) => Some((IconName::TriangleAlert, message.clone(), theme.red)),
        };

        crate::ui::style::panel_container(is_focused, theme)
            .id("struct-tree-view")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.table_state.resizing_column.is_some() {
                    this.table_state.update_resize(event.position.x);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.table_state.resizing_column.is_some() {
                        this.table_state.end_resize();
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.table_state.resizing_column.is_some() {
                        this.table_state.end_resize();
                        cx.notify();
                    }
                }),
            )
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_top))
            .on_action(cx.listener(Self::move_bottom))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::toggle_expand))
            .on_action(cx.listener(Self::expand))
            .on_action(cx.listener(Self::collapse))
            .on_action(cx.listener(Self::on_action_expand_all))
            .on_action(cx.listener(Self::on_action_collapse_all))
            .on_action(cx.listener(Self::on_action_toggle_structure_address_column))
            .on_action(cx.listener(Self::on_action_toggle_structure_type_column))
            .on_action(cx.listener(Self::on_action_toggle_structure_size_column))
            .on_action(cx.listener(Self::on_action_toggle_structure_value_column))
            .on_action(cx.listener(Self::on_action_set_value_radix_hex))
            .on_action(cx.listener(Self::on_action_set_value_radix_dec))
            .on_action(cx.listener(Self::on_action_set_value_radix_oct))
            .on_action(cx.listener(Self::on_action_set_value_radix_bin))
            .on_action(cx.listener(Self::copy_field_value))
            .on_action(cx.listener(Self::copy_field_name))
            .on_action(cx.listener(Self::copy_field_offset))
            .on_action(cx.listener(Self::on_action_copy_structure_result))
            .on_action(cx.listener(Self::on_action_export_structure_yaml))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.focus_handle.focus(window, cx);
                }),
            )
            .child(header)
            .when_some(structure_toolbar, |el, toolbar| el.child(toolbar))
            .when_some(export_feedback, |el, (icon, message, color)| {
                el.child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_1()
                        .px_3()
                        .py_1()
                        .bg(color.opacity(0.08))
                        .text_xs()
                        .text_color(color)
                        .child(Icon::new(icon).size(px(13.0)))
                        .child(div().flex_1().truncate().child(message)),
                )
            })
            .when_some(parse_progress, |el, (progress, parsed_bytes, total_bytes, is_finalizing)| {
                el.child(
                    div()
                        .id("structure-parse-progress")
                        .h(px(3.0))
                        .w_full()
                        .overflow_hidden()
                        .bg(theme.border.opacity(0.35))
                        .child(div().h_full().w(relative(progress)).bg(if is_finalizing { theme.yellow } else { theme.accent })),
                )
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(if is_finalizing {
                            "Finalizing structure index..."
                        } else {
                            "Analyzing binary data..."
                        })
                        .child(format!("{} / {}", format_size_friendly(parsed_bytes), format_size_friendly(total_bytes))),
                )
            })
            .child(div().flex_1().min_h_0().w_full().overflow_hidden().child(if show_parsing_placeholder {
                crate::ui::style::panel_empty_state(
                    IconName::LoaderCircle,
                    "Parsing Structure...",
                    Some("Analyzing binary data with Kaitai Struct..."),
                    None,
                    theme,
                )
                .into_any_element()
            } else if is_empty {
                let recent_section = if !self.recent_definition_paths.is_empty() {
                    let recent_buttons = self
                        .recent_definition_paths
                        .iter()
                        .enumerate()
                        .map(|(index, path)| {
                            let path = path.to_string_lossy().into_owned();
                            let label = path_from_string_file_name(&path);
                            let load_path = path.clone();
                            let remove_path = path.clone();

                            h_flex()
                                .w_full()
                                .items_center()
                                .gap_1()
                                .child(
                                    h_flex()
                                        .id(("recent-definition-item", index))
                                        .flex_1()
                                        .min_w_0()
                                        .h_5()
                                        .items_center()
                                        .gap_1()
                                        .px_1()
                                        .cursor_pointer()
                                        .when(!has_active_editor, |style| style.opacity(0.5))
                                        .hover(|style| style.bg(theme.muted.opacity(0.4)))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            if has_active_editor {
                                                this.focus_handle.focus(window, cx);
                                                window.dispatch_action(
                                                    Box::new(crate::actions::LoadStructureDefinitionFromHistory { path: load_path.clone() }),
                                                    cx,
                                                );
                                            }
                                        }))
                                        .child(Icon::new(IconName::FileCode).with_size(gpui_kit::component::Size::XSmall))
                                        .child(div().flex_1().min_w_0().text_xs().truncate().whitespace_nowrap().child(label)),
                                )
                                .child(
                                    gpui_kit::component::button::Button::new(SharedString::from(format!("remove-recent-definition-{index}")))
                                        .ghost()
                                        .icon(IconName::Close)
                                        .with_size(gpui_kit::component::Size::XSmall)
                                        .tooltip("Remove from recents")
                                        .on_click(cx.listener(move |_, _, window, cx| {
                                            window.dispatch_action(
                                                Box::new(crate::actions::RemoveStructureDefinitionFromHistory { path: remove_path.clone() }),
                                                cx,
                                            );
                                        })),
                                )
                                .into_any_element()
                        })
                        .collect::<Vec<_>>();

                    Some(
                        h_flex()
                            .w_full()
                            .justify_start()
                            .child(
                                v_flex()
                                    .w(relative(0.95))
                                    .mt_4()
                                    .pt_3()
                                    .border_t_1()
                                    .border_color(theme.border.opacity(0.6))
                                    .items_start()
                                    .gap_1()
                                    .child(div().px_1().text_xs().font_semibold().text_color(theme.muted_foreground).child("Recents"))
                                    .children(recent_buttons),
                            )
                            .into_any_element(),
                    )
                } else {
                    None
                };

                if !has_active_editor {
                    crate::ui::style::panel_empty_state(
                        IconName::File,
                        "No File Open",
                        Some("Open a binary file before loading a structure definition"),
                        recent_section,
                        theme,
                    )
                    .into_any_element()
                } else {
                    let load_btn = gpui_kit::component::button::Button::new("load-ksy-btn")
                        .label("Load Definition...")
                        .primary()
                        .with_size(gpui_kit::component::Size::Small)
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(crate::actions::LoadStructureDefinition), cx);
                        }))
                        .into_any_element();

                    let mut load_actions = v_flex().w_full().items_center().child(load_btn);
                    if let Some(recent_section) = recent_section {
                        load_actions = load_actions.child(recent_section);
                    }

                    crate::ui::style::panel_empty_state(
                        IconName::Braces,
                        "No Structure Loaded",
                        Some("Open a Kaitai Struct (.ksy) YAML file to inspect binary fields"),
                        Some(load_actions.into_any_element()),
                        theme,
                    )
                    .into_any_element()
                }
            } else {
                let focus_handle = self.focus_handle.clone();
                let value_radix = self.value_radix;
                let total_visible_width = self.table_state.total_visible_width();
                let scroll_offset_x = self.table_state.scroll_offset_x;
                let is_address_visible = self.table_state.column(1).map(|c| c.visible).unwrap_or(true);
                let is_type_visible = self.table_state.column(2).map(|c| c.visible).unwrap_or(false);
                let is_size_visible = self.table_state.column(3).map(|c| c.visible).unwrap_or(false);
                let is_value_visible = self.table_state.column(4).map(|c| c.visible).unwrap_or(true);

                let column_header = VirtualTable::render_header_row(
                    &self.table_state,
                    "structure-column-header",
                    theme,
                    cx,
                    Some(|col_ix, col: &TableColumn| {
                        if col_ix == 0 {
                            Some(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(div().flex_shrink_0().w(px(TREE_INDICATOR_WIDTH)).h(px(TREE_INDICATOR_WIDTH)))
                                    .child(div().flex_1().min_w_0().truncate().whitespace_nowrap().child(col.name.clone()))
                                    .into_any_element(),
                            )
                        } else {
                            None
                        }
                    }),
                    Self::auto_fit_column,
                    None::<fn(&mut Self, usize, &mut Context<Self>)>,
                    |this| &mut this.table_state,
                )
                .context_menu({
                    let context_focus = self.focus_handle.clone();
                    let value_radix = self.value_radix;
                    move |menu, window, cx| {
                        menu.action_context(context_focus.clone())
                            .label("Visible columns")
                            .separator()
                            .menu_with_check("Address", is_address_visible, Box::new(crate::actions::ToggleStructureAddressColumn))
                            .menu_with_check("Type", is_type_visible, Box::new(crate::actions::ToggleStructureTypeColumn))
                            .menu_with_check("Size", is_size_visible, Box::new(crate::actions::ToggleStructureSizeColumn))
                            .menu_with_check("Value", is_value_visible, Box::new(crate::actions::ToggleStructureValueColumn))
                            .separator()
                            .submenu("Value format", window, cx, move |menu, _window, _cx| {
                                menu.menu_with_check(
                                    "Hexadecimal (16)",
                                    value_radix == DisplayRadix::Hexadecimal,
                                    Box::new(crate::actions::SetRadixHex),
                                )
                                .menu_with_check("Decimal (10)", value_radix == DisplayRadix::Decimal, Box::new(crate::actions::SetRadixDec))
                                .menu_with_check("Octal (8)", value_radix == DisplayRadix::Octal, Box::new(crate::actions::SetRadixOct))
                                .menu_with_check(
                                    "Binary (2)",
                                    value_radix == DisplayRadix::Binary,
                                    Box::new(crate::actions::SetRadixBin),
                                )
                            })
                    }
                });

                let context_view = view.clone();
                let context_focus = focus_handle.clone();
                let visible_cols: Vec<(usize, Pixels)> = self.table_state.visible_columns().map(|(ix, col)| (ix, col.width)).collect();

                let tree_list = uniform_list("struct-tree-list", self.flattened_fields.len(), move |range, window, cx| {
                    let this = view.read(cx);
                    range
                        .map(|ix| {
                            if let Some(field) = this.flattened_fields.get(ix) {
                                let is_selected = this.selected_index == Some(ix);
                                if let Some(parsed_field) = this.field_at_path(&field.path) {
                                    Self::render_list_item(
                                        ix,
                                        field,
                                        parsed_field,
                                        is_selected,
                                        is_focused,
                                        &visible_cols,
                                        total_visible_width,
                                        scroll_offset_x,
                                        value_radix,
                                        view.clone(),
                                        &focus_handle,
                                        window,
                                        cx,
                                    )
                                } else {
                                    div().into_any_element()
                                }
                            } else {
                                div().into_any_element()
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .track_scroll(&self.table_state.vertical_scroll_handle)
                .size_full();

                let horizontal_scrollbar = VirtualTable::render_horizontal_scrollbar(&self.table_state, self.last_container_width, theme);
                let vertical_scrollbar = VirtualTable::render_vertical_scrollbar(&self.table_state, theme);

                v_flex()
                    .id("struct-table-container")
                    .size_full()
                    .overflow_hidden()
                    .relative()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        if this.table_state.resizing_column.is_some() {
                            this.table_state.update_resize(event.position.x);
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                            if this.table_state.resizing_column.is_some() {
                                this.table_state.end_resize();
                                cx.notify();
                            }
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                            if this.table_state.resizing_column.is_some() {
                                this.table_state.end_resize();
                                cx.notify();
                            }
                        }),
                    )
                    .child(table_overlay)
                    .child(column_header)
                    .child(
                        div()
                            .id("struct-results-container")
                            .flex_1()
                            .overflow_hidden()
                            .relative()
                            .child(tree_list)
                            .child(vertical_scrollbar)
                            .children(horizontal_scrollbar),
                    )
                    .context_menu(move |menu, window, cx| {
                        let selected_field = {
                            let this = context_view.read(cx);
                            this.selected_index.and_then(|ix| {
                                let field = this.flattened_fields.get(ix)?;
                                let parsed_field = this.field_at_path(&field.path)?;
                                Some((
                                    parsed_field.id.clone(),
                                    parsed_field.offset,
                                    Self::value_text(parsed_field, this.value_radix),
                                    this.value_radix,
                                ))
                            })
                        };
                        let Some((field_id, field_offset, field_value, value_radix)) = selected_field else {
                            return menu;
                        };

                        let offset_hex = format!("0x{:08X}", field_offset);
                        menu.action_context(context_focus.clone())
                            .menu_with_icon(
                                format!("Go to Offset ({})", offset_hex),
                                IconName::ChevronRight,
                                Box::new(crate::actions::GoToBeginning),
                            )
                            .separator()
                            .menu_with_icon(
                                format!("Copy Value ({})", field_value),
                                IconName::Copy,
                                Box::new(CopyFieldValue { value: field_value }),
                            )
                            .menu_with_icon(
                                format!("Copy Field Name ({})", field_id),
                                IconName::Copy,
                                Box::new(CopyFieldName { name: field_id }),
                            )
                            .menu_with_icon(
                                format!("Copy Offset ({})", offset_hex),
                                IconName::Hash,
                                Box::new(CopyFieldOffset { offset: offset_hex }),
                            )
                            .separator()
                            .submenu("Value format", window, cx, move |menu, _window, _cx| {
                                menu.menu_with_check(
                                    "Hexadecimal (16)",
                                    value_radix == DisplayRadix::Hexadecimal,
                                    Box::new(crate::actions::SetRadixHex),
                                )
                                .menu_with_check("Decimal (10)", value_radix == DisplayRadix::Decimal, Box::new(crate::actions::SetRadixDec))
                                .menu_with_check("Octal (8)", value_radix == DisplayRadix::Octal, Box::new(crate::actions::SetRadixOct))
                                .menu_with_check(
                                    "Binary (2)",
                                    value_radix == DisplayRadix::Binary,
                                    Box::new(crate::actions::SetRadixBin),
                                )
                            })
                            .separator()
                            .menu_with_icon("Copy Structure Analysis", IconName::Copy, Box::new(crate::actions::CopyStructureResult))
                            .menu_with_icon(
                                "Export Structure as YAML",
                                IconName::HardDriveDownload,
                                Box::new(crate::actions::ExportStructureYaml),
                            )
                            .separator()
                            .menu_with_icon("Expand All", IconName::ChevronDown, Box::new(crate::actions::ExpandAllStructure))
                            .menu_with_icon("Collapse All", IconName::ChevronUp, Box::new(crate::actions::CollapseAllStructure))
                    })
                    .into_any_element()
            }))
    }
}

impl Focusable for StructTreeView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
