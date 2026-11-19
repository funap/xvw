use crate::core::address_map::AddressMap;
use crate::core::buffer::Buffer;
use crate::core::editor::Editor;
use crate::core::format::HexCase;
use crate::core::zlib::{ZlibMatch, decompress_zlib_at, find_zlib_streams_segmented_limited};
use crate::ui::appearance::Appearance;
use crate::ui::icon::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::table::{Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState};
use gpui_kit::component::theme::Theme;
use gpui_kit::component::{ActiveTheme as _, Disableable, Sizable, Size, WindowExt as _, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

actions!(zlib_panel, [FocusTable, ClearResults, ConfirmSelect, DismissTable]);

#[derive(Clone, PartialEq, Action)]
#[action(namespace = zlib_panel, no_json)]
struct CopyAddress {
    value: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = zlib_panel, no_json)]
struct CopyPreview {
    value: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = zlib_panel, no_json)]
struct DecompressEntry {
    index: usize,
    offset: usize,
    compressed_len: usize,
}

const CONTEXT: &str = "ZlibPanel";

/// Maximum number of zlib stream matches displayed per scan.
pub const MAX_ZLIB_RESULTS: usize = 10_000;

/// Registers keybindings for the Zlib search panel.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", DismissTable, Some("ZlibPanel")),
        KeyBinding::new("escape", DismissTable, Some("ZlibPanel > DataTable")),
        KeyBinding::new("enter", ConfirmSelect, Some("ZlibPanel > DataTable")),
        KeyBinding::new("down", FocusTable, Some("ZlibPanel > Input")),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScanSignature {
    content_state: usize,
    byte_len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ZlibCellMeta {
    compressed_label: SharedString,
    decompressed_label: SharedString,
    preview: SharedString,
    preview_char_count: usize,
}

fn build_zlib_cell_meta(results: &[ZlibMatch]) -> Arc<[ZlibCellMeta]> {
    results
        .iter()
        .map(|item| {
            let preview_cow = preview_text(&item.preview);
            let preview_char_count = preview_cow.chars().count();
            let preview = match preview_cow {
                std::borrow::Cow::Borrowed(_) => SharedString::from(item.preview.clone()),
                std::borrow::Cow::Owned(s) => SharedString::from(s),
            };
            ZlibCellMeta {
                compressed_label: SharedString::from(format!("{} B", item.compressed_len)),
                decompressed_label: SharedString::from(format!("{} B", item.decompressed_len)),
                preview,
                preview_char_count,
            }
        })
        .collect::<Vec<_>>()
        .into()
}

struct AddressLabelCache {
    results_id: u64,
    editor_id: Option<EntityId>,
    hex_case: HexCase,
    address_map: AddressMap,
    labels: Vec<Option<SharedString>>,
}

impl AddressLabelCache {
    fn new(results_id: u64, editor_id: Option<EntityId>, hex_case: HexCase, address_map: AddressMap, len: usize) -> Self {
        Self {
            results_id,
            editor_id,
            hex_case,
            address_map,
            labels: vec![None; len],
        }
    }

    fn matches(&self, results_id: u64, editor_id: Option<EntityId>, hex_case: HexCase, len: usize) -> bool {
        self.results_id == results_id && self.editor_id == editor_id && self.hex_case == hex_case && self.labels.len() == len
    }

    fn get_or_format(&mut self, index: usize, offset: usize) -> SharedString {
        if let Some(Some(cached)) = self.labels.get(index) {
            return cached.clone();
        }
        let formatted: SharedString = self.hex_case.format_prefixed_pad(self.address_map.offset_to_address(offset), 8).into();
        if let Some(slot) = self.labels.get_mut(index) {
            *slot = Some(formatted.clone());
        }
        formatted
    }
}

struct CachedZlibState {
    results_id: u64,
    results: Arc<[ZlibMatch]>,
    cell_meta: Arc<[ZlibCellMeta]>,
    is_truncated: bool,
    selected_index: Option<usize>,
    has_scanned: bool,
    scan_signature: Option<ScanSignature>,
}

/// Events emitted by [`ZlibPanel`] to coordinate editor navigation and new decompressed tabs.
#[derive(Clone)]
pub enum ZlibPanelEvent {
    NavigateTo {
        offset: usize,
        len: usize,
    },
    FocusEditor,
    OpenDecompressedDocument {
        source_path: PathBuf,
        offset: usize,
        address: usize,
        buffer: Buffer,
    },
}

impl fmt::Debug for ZlibPanelEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NavigateTo { offset, len } => f.debug_struct("NavigateTo").field("offset", offset).field("len", len).finish(),
            Self::FocusEditor => write!(f, "FocusEditor"),
            Self::OpenDecompressedDocument {
                source_path,
                offset,
                address,
                buffer,
            } => f
                .debug_struct("OpenDecompressedDocument")
                .field("source_path", source_path)
                .field("offset", offset)
                .field("address", address)
                .field("byte_len", &buffer.len())
                .finish(),
        }
    }
}

#[inline]
fn contains_ignore_case(haystack: &str, lowercase_needle: &str) -> bool {
    if lowercase_needle.is_empty() {
        return true;
    }
    if haystack.is_ascii() && lowercase_needle.is_ascii() {
        let h = haystack.as_bytes();
        let n = lowercase_needle.as_bytes();
        return h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n));
    }
    haystack.to_lowercase().contains(lowercase_needle)
}

fn filter_zlib_indices(results: &[ZlibMatch], address_map: &AddressMap, hex_case: HexCase, lowercase_filter: &str) -> Vec<usize> {
    if lowercase_filter.is_empty() {
        return (0..results.len()).collect();
    }

    results
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            if contains_ignore_case(&item.preview, lowercase_filter) {
                return true;
            }
            let address = address_map.offset_to_address(item.offset);
            let padded = hex_case.format_prefixed_pad(address, 8);
            if contains_ignore_case(&padded, lowercase_filter) {
                return true;
            }
            let compact = hex_case.format_prefixed(address);
            contains_ignore_case(&compact, lowercase_filter)
        })
        .map(|(index, _)| index)
        .collect()
}

fn sort_zlib_indices(results: &[ZlibMatch], indices: &mut [usize], sort_column: Option<(usize, ColumnSort)>) {
    let Some((col_ix, sort)) = sort_column else {
        indices.sort_unstable();
        return;
    };
    if sort == ColumnSort::Default {
        indices.sort_unstable();
        return;
    }

    indices.sort_by(|left, right| {
        let left_match = &results[*left];
        let right_match = &results[*right];
        let ordering = match col_ix {
            0 => left_match.offset.cmp(&right_match.offset),
            1 => left_match.compressed_len.cmp(&right_match.compressed_len),
            2 => left_match.decompressed_len.cmp(&right_match.decompressed_len),
            3 => left_match.preview.cmp(&right_match.preview),
            _ => std::cmp::Ordering::Equal,
        };
        let ordering = if sort == ColumnSort::Descending { ordering.reverse() } else { ordering };
        ordering.then_with(|| left.cmp(right))
    });
}

/// Table delegate backing [`ZlibPanel`]'s result grid.
pub struct ZlibTableDelegate {
    panel: WeakEntity<ZlibPanel>,
    results_id: u64,
    results: Arc<[ZlibMatch]>,
    cell_meta: Arc<[ZlibCellMeta]>,
    visible_indices: Vec<usize>,
    sort_column: Option<(usize, ColumnSort)>,
    editor: Option<Entity<Editor>>,
    address_cache: Option<AddressLabelCache>,
    selected_index: Option<usize>,
    focus_handle: FocusHandle,
}

impl ZlibTableDelegate {
    fn sort_indices(&mut self) {
        sort_zlib_indices(&self.results, &mut self.visible_indices, self.sort_column);
    }

    fn ensure_address_cache(&mut self, hex_case: HexCase, cx: &App) -> &mut AddressLabelCache {
        let editor_id = self.editor.as_ref().map(Entity::entity_id);
        let len = self.results.len();
        let needs_rebuild = !self
            .address_cache
            .as_ref()
            .is_some_and(|cache| cache.matches(self.results_id, editor_id, hex_case, len));
        if needs_rebuild {
            let address_map = self
                .editor
                .as_ref()
                .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.address_map.clone()))
                .unwrap_or_default();
            self.address_cache = Some(AddressLabelCache::new(self.results_id, editor_id, hex_case, address_map, len));
        }
        self.address_cache.as_mut().expect("address cache initialized")
    }

    fn cached_address_label(&mut self, index: usize, hex_case: HexCase, cx: &App) -> Option<SharedString> {
        let offset = self.results.get(index)?.offset;
        let cache = self.ensure_address_cache(hex_case, cx);
        Some(cache.get_or_format(index, offset))
    }
}

impl TableDelegate for ZlibTableDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        4
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.visible_indices.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        let sort = match self.sort_column {
            Some((ix, s)) if ix == col_ix => s,
            _ => ColumnSort::Default,
        };

        match col_ix {
            0 => Column::new("address", "Address").width(px(88.0)).min_width(px(50.0)).resizable(true).sort(sort),
            1 => Column::new("compressed", "Compressed")
                .width(px(76.0))
                .min_width(px(48.0))
                .resizable(true)
                .sort(sort),
            2 => Column::new("decompressed", "Decompressed")
                .width(px(88.0))
                .min_width(px(48.0))
                .resizable(true)
                .sort(sort),
            3 => Column::new("preview", "Preview")
                .width(px(180.0))
                .min_width(px(80.0))
                .resizable(true)
                .sort(sort),
            _ => Column::default(),
        }
    }

    fn perform_sort(&mut self, col_ix: usize, sort: ColumnSort, _window: &mut Window, cx: &mut Context<TableState<Self>>) {
        let sort_col = if sort == ColumnSort::Default { None } else { Some((col_ix, sort)) };
        self.sort_column = sort_col;
        self.sort_indices();
        if let Some(panel) = self.panel.upgrade() {
            cx.defer(move |cx| {
                panel.update(cx, |panel, cx| {
                    panel.sort_column = sort_col;
                    panel.sync_selection_to_filter(cx);
                    cx.notify();
                });
            });
        }
    }

    fn fit_column(&mut self, col_ix: usize, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> Option<Pixels> {
        let max_len = match col_ix {
            0 => {
                let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
                let mut max_chars = 0usize;
                for row_pos in 0..self.visible_indices.len().min(128) {
                    let index = self.visible_indices[row_pos];
                    if let Some(label) = self.cached_address_label(index, hex_case, cx) {
                        max_chars = max_chars.max(label.chars().count());
                    }
                }
                max_chars
            }
            1 => self
                .visible_indices
                .iter()
                .take(128)
                .filter_map(|&index| self.cell_meta.get(index))
                .map(|meta| meta.compressed_label.len())
                .max()
                .unwrap_or(0),
            2 => self
                .visible_indices
                .iter()
                .take(128)
                .filter_map(|&index| self.cell_meta.get(index))
                .map(|meta| meta.decompressed_label.len())
                .max()
                .unwrap_or(0),
            3 => self
                .visible_indices
                .iter()
                .take(128)
                .filter_map(|&index| self.cell_meta.get(index))
                .map(|meta| meta.preview_char_count)
                .max()
                .unwrap_or(0),
            _ => 0,
        };

        Some(px(max_len as f32 * 7.5 + 24.0))
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(&index) = self.visible_indices.get(row_ix) else {
            return menu;
        };
        let Some(item) = self.results.get(index) else {
            return menu;
        };
        let offset = item.offset;
        let compressed_len = item.compressed_len;
        let preview_value = item.preview.clone();
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let offset_value = self
            .cached_address_label(index, hex_case, cx)
            .map(|label| label.to_string())
            .unwrap_or_default();

        menu.action_context(self.focus_handle.clone())
            .menu_with_icon(
                "Decompress to New Document",
                IconName::ExternalLink,
                Box::new(DecompressEntry { index, offset, compressed_len }),
            )
            .separator()
            .menu_with_icon("Copy Address", IconName::Hash, Box::new(CopyAddress { value: offset_value }))
            .menu_with_icon("Copy Preview", IconName::Binary, Box::new(CopyPreview { value: preview_value }))
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        if col_ix > 3 {
            return div().into_any_element();
        }
        let Some(&index) = self.visible_indices.get(row_ix) else {
            return div().into_any_element();
        };
        if index >= self.results.len() {
            return div().into_any_element();
        }

        let theme = cx.theme();
        let font_family = cx.global::<Appearance>().font_family.clone();
        let is_selected = self.selected_index == Some(index);
        let colors = zlib_row_colors(is_selected, false, theme);

        let (color, text): (Hsla, SharedString) = match col_ix {
            0 => {
                let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
                let Some(addr) = self.cached_address_label(index, hex_case, cx) else {
                    return div().into_any_element();
                };
                (colors.address_color, addr)
            }
            1 => {
                let Some(meta) = self.cell_meta.get(index) else {
                    return div().into_any_element();
                };
                (colors.size_color, meta.compressed_label.clone())
            }
            2 => {
                let Some(meta) = self.cell_meta.get(index) else {
                    return div().into_any_element();
                };
                (colors.size_color, meta.decompressed_label.clone())
            }
            3 => {
                let Some(meta) = self.cell_meta.get(index) else {
                    return div().into_any_element();
                };
                (colors.value_color, meta.preview.clone())
            }
            _ => return div().into_any_element(),
        };

        div().text_xs().font_family(font_family).text_color(color).child(text).into_any_element()
    }
}

/// Sidebar panel for finding embedded zlib streams and opening decompressed payloads in new tabs.
pub struct ZlibPanel {
    pub editor: Option<Entity<Editor>>,
    pub focus_handle: FocusHandle,
    pub table_state: Entity<TableState<ZlibTableDelegate>>,
    pub filter_input: Entity<InputState>,
    pub is_scanning: bool,
    pub is_decompressing: bool,
    pub results: Arc<[ZlibMatch]>,
    cell_meta: Arc<[ZlibCellMeta]>,
    results_id: u64,
    next_results_id: u64,
    pub is_truncated: bool,
    pub selected_index: Option<usize>,
    pub has_scanned: bool,
    pub scan_task: Option<Task<()>>,
    pub decompress_task: Option<Task<()>>,
    pub sort_column: Option<(usize, ColumnSort)>,
    scan_generation: usize,
    decompress_generation: usize,
    scan_signature: Option<ScanSignature>,
    cached_states: HashMap<EntityId, CachedZlibState>,
    _table_subscription: Option<Subscription>,
    _filter_subscription: Option<Subscription>,
    _editor_subscription: Option<Subscription>,
}

impl EventEmitter<ZlibPanelEvent> for ZlibPanel {}

impl ZlibPanel {
    /// Creates a new [`ZlibPanel`] optionally bound to an initial [`Editor`].
    pub fn new(editor: Option<Entity<Editor>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let view = cx.entity().downgrade();
        let delegate = ZlibTableDelegate {
            panel: view,
            results_id: 0,
            results: Arc::default(),
            cell_meta: Arc::default(),
            visible_indices: Vec::new(),
            sort_column: None,
            editor: None,
            address_cache: None,
            selected_index: None,
            focus_handle: focus_handle.clone(),
        };
        let table_state = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .row_selectable(true)
                .col_selectable(false)
                .col_resizable(true)
                .col_movable(false)
        });

        let table_subscription = cx.subscribe_in(&table_state, window, |this, _, event: &TableEvent, window, cx| match event {
            TableEvent::SelectRow(row_ix) => {
                if let Some(index) = this.resolve_visible_row_index(*row_ix, cx)
                    && this.selected_index != Some(index)
                {
                    this.select_item_at_row(index, Some(*row_ix), true, cx);
                }
            }
            TableEvent::DoubleClickedRow(row_ix) => {
                if let Some(index) = this.resolve_visible_row_index(*row_ix, cx) {
                    this.select_item_at_row(index, Some(*row_ix), false, cx);
                    cx.emit(ZlibPanelEvent::FocusEditor);
                    window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
                }
            }
            _ => {}
        });

        let filter_input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by address or preview..."));

        let filter_subscription = cx.subscribe_in(&filter_input, window, |this, _, event: &input::InputEvent, _window, cx| {
            if let input::InputEvent::Change = event {
                this.sync_selection_to_filter(cx);
                cx.notify();
            }
        });

        let mut this = Self {
            editor: None,
            focus_handle,
            table_state,
            filter_input,
            is_scanning: false,
            is_decompressing: false,
            results: Arc::default(),
            cell_meta: Arc::default(),
            results_id: 0,
            next_results_id: 1,
            is_truncated: false,
            selected_index: None,
            has_scanned: false,
            scan_task: None,
            decompress_task: None,
            scan_generation: 0,
            decompress_generation: 0,
            scan_signature: None,
            cached_states: HashMap::new(),
            sort_column: None,
            _table_subscription: Some(table_subscription),
            _filter_subscription: Some(filter_subscription),
            _editor_subscription: None,
        };

        this.set_editor(editor, cx);
        this
    }

    /// Rebinds the panel to `editor`, caching previous results per editor instance.
    pub fn set_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        let current_editor_id = self.editor.as_ref().map(Entity::entity_id);
        let next_editor_id = editor.as_ref().map(Entity::entity_id);
        if current_editor_id == next_editor_id {
            cx.notify();
            return;
        }

        self.cache_current_state();
        self._editor_subscription = None;
        self.invalidate_scan();
        self.editor = editor.clone();
        self.scan_signature = self.current_signature(cx);
        self.sync_selection_to_filter(cx);

        if let Some(ed) = &editor {
            self._editor_subscription = Some(cx.observe(ed, |this, _, cx| {
                this.invalidate_if_content_changed(cx);
            }));
        }
        if let Some(editor_id) = next_editor_id {
            self.restore_cached_state(editor_id, cx);
        }
        cx.notify();
    }

    fn alloc_results_id(&mut self) -> u64 {
        let id = self.next_results_id;
        self.next_results_id = self.next_results_id.wrapping_add(1).max(1);
        id
    }

    fn cache_current_state(&mut self) {
        let Some(editor) = &self.editor else {
            return;
        };

        self.cached_states.insert(
            editor.entity_id(),
            CachedZlibState {
                results_id: self.results_id,
                results: std::mem::take(&mut self.results),
                cell_meta: std::mem::take(&mut self.cell_meta),
                is_truncated: self.is_truncated,
                selected_index: self.selected_index,
                has_scanned: self.has_scanned,
                scan_signature: self.scan_signature,
            },
        );
    }

    fn restore_cached_state(&mut self, editor_id: EntityId, cx: &mut Context<Self>) {
        let Some(cached) = self.cached_states.remove(&editor_id) else {
            return;
        };

        let is_current_scan = cached.has_scanned && cached.scan_signature == self.scan_signature;
        if !is_current_scan && cached.has_scanned {
            return;
        }

        self.results_id = cached.results_id;
        self.results = cached.results;
        self.cell_meta = cached.cell_meta;
        self.is_truncated = cached.is_truncated;
        self.selected_index = cached.selected_index;
        self.has_scanned = cached.has_scanned;
        self.sync_selection_to_filter(cx);
    }

    fn current_signature(&self, cx: &App) -> Option<ScanSignature> {
        let editor = self.editor.as_ref()?.read(cx);
        let document = editor.document.read().expect("document read lock");
        Some(ScanSignature {
            content_state: document.history.state_id(),
            byte_len: document.buffer.len(),
        })
    }

    fn invalidate_if_content_changed(&mut self, cx: &mut Context<Self>) {
        let signature = self.current_signature(cx);
        if signature == self.scan_signature {
            return;
        }
        self.invalidate_scan();
        self.scan_signature = signature;
        self.sync_selection_to_filter(cx);
        cx.notify();
    }

    fn invalidate_scan(&mut self) {
        self.scan_generation = self.scan_generation.wrapping_add(1);
        self.scan_task = None;
        self.is_scanning = false;
        self.decompress_generation = self.decompress_generation.wrapping_add(1);
        self.decompress_task = None;
        self.is_decompressing = false;
        self.results_id = self.alloc_results_id();
        self.results = Arc::default();
        self.cell_meta = Arc::default();
        self.is_truncated = false;
        self.selected_index = None;
        self.has_scanned = false;
    }

    /// Scans the active document on a background thread for embedded zlib streams.
    pub fn trigger_scan(&mut self, cx: &mut Context<Self>) {
        self.invalidate_scan();
        self.sync_selection_to_filter(cx);
        let generation = self.scan_generation;
        self.scan_signature = self.current_signature(cx);

        let Some(editor_entity) = &self.editor else {
            self.is_scanning = false;
            cx.notify();
            return;
        };

        let (buffer_data, segment_ranges) = {
            let editor = editor_entity.read(cx);
            let document = editor.document.read().expect("document read lock");
            (document.buffer.clone(), document.address_map.segment_ranges())
        };

        self.is_scanning = true;
        let task = cx.spawn(async move |this, cx| {
            let (results, cell_meta, is_truncated) = cx
                .background_executor()
                .spawn(async move {
                    let (results, is_truncated) = find_zlib_streams_segmented_limited(buffer_data.data(), &segment_ranges, MAX_ZLIB_RESULTS);
                    let cell_meta = build_zlib_cell_meta(&results);
                    (Arc::from(results), cell_meta, is_truncated)
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if this.scan_generation != generation {
                        return;
                    }

                    this.scan_task = None;
                    this.is_scanning = false;
                    this.is_truncated = is_truncated;
                    this.results_id = this.alloc_results_id();
                    this.results = results;
                    this.cell_meta = cell_meta;
                    this.has_scanned = true;
                    this.sync_selection_to_filter(cx);
                    cx.notify();
                });
            }
        });

        self.scan_task = Some(task);
        cx.notify();
    }

    /// Decompresses the currently selected zlib result on a background thread and emits
    /// [`ZlibPanelEvent::OpenDecompressedDocument`].
    pub fn decompress_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.selected_index {
            self.decompress_index(index, window, cx);
        }
    }

    fn decompress_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_scanning || self.is_decompressing {
            return;
        }
        let Some(editor_entity) = self.editor.clone() else {
            return;
        };
        let Some(item) = self.results.get(index).cloned() else {
            return;
        };

        let (buffer_data, source_path, address) = {
            let editor = editor_entity.read(cx);
            let document = editor.document.read().expect("document read lock");
            (
                document.buffer.clone(),
                document.path().to_path_buf(),
                document.address_map.offset_to_address(item.offset),
            )
        };

        self.decompress_generation = self.decompress_generation.wrapping_add(1);
        let generation = self.decompress_generation;
        self.is_decompressing = true;

        let bg_task = cx
            .background_executor()
            .spawn(async move { decompress_zlib_at(buffer_data.data(), item.offset, Some(item.compressed_len)) });

        let task = cx.spawn_in(window, async move |this, window| {
            let result = bg_task.await;
            let _ = window.update(|window, cx| {
                if let Some(this) = this.upgrade() {
                    this.update(cx, |this, cx| {
                        if this.decompress_generation != generation {
                            return;
                        }
                        this.decompress_task = None;
                        this.is_decompressing = false;
                        match result {
                            Ok(bytes) => {
                                cx.emit(ZlibPanelEvent::OpenDecompressedDocument {
                                    source_path,
                                    offset: item.offset,
                                    address,
                                    buffer: Buffer::new(bytes),
                                });
                            }
                            Err(err) => {
                                window.push_notification(Notification::error(format!("Failed to decompress zlib stream: {err}")), cx);
                            }
                        }
                        cx.notify();
                    });
                }
            });
        });

        self.decompress_task = Some(task);
        cx.notify();
    }

    fn filter_query(&self, cx: &App) -> String {
        self.filter_input.read(cx).value().trim().to_lowercase()
    }

    fn filtered_indices(&self, cx: &App) -> Vec<usize> {
        let filter = self.filter_query(cx);
        if filter.is_empty() {
            return (0..self.results.len()).collect();
        }

        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let address_map = self
            .editor
            .as_ref()
            .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.address_map.clone()))
            .unwrap_or_default();
        filter_zlib_indices(&self.results, &address_map, hex_case, &filter)
    }

    fn sorted_indices(&self, cx: &App) -> Vec<usize> {
        let mut indices = self.filtered_indices(cx);
        if self.sort_column.is_some_and(|(_, sort)| sort != ColumnSort::Default) {
            sort_zlib_indices(&self.results, &mut indices, self.sort_column);
        }
        indices
    }

    fn resolve_visible_row_index(&self, row_ix: usize, cx: &App) -> Option<usize> {
        self.table_state.read(cx).delegate().visible_indices.get(row_ix).copied()
    }

    fn result_summary(&self) -> String {
        if self.is_scanning {
            "Searching...".to_string()
        } else if self.is_truncated {
            format!("{}+ streams found (showing first {} streams)", MAX_ZLIB_RESULTS, MAX_ZLIB_RESULTS)
        } else {
            let label = if self.results.len() == 1 { "stream" } else { "streams" };
            format!("{} {label} found", self.results.len())
        }
    }

    fn sync_selection_to_filter(&mut self, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices(cx);
        if let Some(index) = self.selected_index {
            if !visible_indices.contains(&index) {
                self.selected_index = visible_indices.first().copied();
            }
        } else {
            self.selected_index = visible_indices.first().copied();
        }

        let row_ix = self.selected_index.and_then(|idx| visible_indices.iter().position(|&i| i == idx));
        let results_id = self.results_id;
        let results = Arc::clone(&self.results);
        let cell_meta = Arc::clone(&self.cell_meta);
        let editor = self.editor.clone();
        let sort_column = self.sort_column;
        let selected_index = self.selected_index;

        self.table_state.update(cx, |table, cx| {
            let delegate = table.delegate_mut();
            let editor_changed = delegate.editor.as_ref().map(Entity::entity_id) != editor.as_ref().map(Entity::entity_id);
            if delegate.results_id != results_id || editor_changed {
                delegate.address_cache = None;
            }
            delegate.results_id = results_id;
            delegate.results = results;
            delegate.cell_meta = cell_meta;
            delegate.visible_indices = visible_indices;
            delegate.sort_column = sort_column;
            delegate.editor = editor;
            delegate.selected_index = selected_index;
            table.refresh(cx);
            if let Some(row_ix) = row_ix {
                table.set_selected_row(row_ix, cx);
            } else {
                table.clear_selection(cx);
            }
        });
    }

    fn select_item_at_row(&mut self, index: usize, known_row_ix: Option<usize>, from_table_selection: bool, cx: &mut Context<Self>) {
        if let Some(item) = self.results.get(index) {
            let selection_changed = self.selected_index != Some(index);
            self.selected_index = Some(index);
            let offset = item.offset;
            let len = item.compressed_len;

            self.table_state.update(cx, |table, cx| {
                let delegate = table.delegate_mut();
                if delegate.selected_index != Some(index) {
                    delegate.selected_index = Some(index);
                }
                if !from_table_selection {
                    let row_ix = known_row_ix
                        .filter(|&row| delegate.visible_indices.get(row) == Some(&index))
                        .or_else(|| delegate.visible_indices.iter().position(|&i| i == index));
                    if let Some(row_ix) = row_ix
                        && table.selected_row() != Some(row_ix)
                    {
                        table.set_selected_row(row_ix, cx);
                    }
                }
            });

            if let Some(ed) = &self.editor {
                ed.update(cx, |editor, cx| {
                    let prev_cursor = editor.cursor.offset;
                    let prev_selection = editor.selection_range();
                    if len > 0 {
                        editor.set_selection_range(offset..offset.saturating_add(len));
                    } else {
                        editor.set_cursor_offset(offset);
                    }
                    if editor.cursor.offset != prev_cursor || editor.selection_range() != prev_selection {
                        cx.notify();
                    }
                });
            }
            cx.emit(ZlibPanelEvent::NavigateTo { offset, len });
            if selection_changed && !from_table_selection {
                cx.notify();
            }
        } else if !from_table_selection {
            cx.notify();
        }
    }

    fn copy_address(&mut self, action: &CopyAddress, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        window.push_notification(Notification::info(format!("Copied address {}", action.value)), cx);
    }

    fn copy_preview(&mut self, action: &CopyPreview, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        window.push_notification(Notification::info("Copied preview"), cx);
    }

    fn decompress_entry(&mut self, action: &DecompressEntry, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.results.get(action.index) else {
            return;
        };
        if item.offset != action.offset || item.compressed_len != action.compressed_len {
            return;
        }
        self.select_item_at_row(action.index, None, false, cx);
        self.decompress_index(action.index, window, cx);
    }

    fn focus_table(&mut self, _: &FocusTable, window: &mut Window, cx: &mut Context<Self>) {
        if !self.table_state.read(cx).delegate().visible_indices.is_empty() {
            let focus_handle = self.table_state.read(cx).focus_handle(cx);
            focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    fn confirm_select(&mut self, _: &ConfirmSelect, window: &mut Window, cx: &mut Context<Self>) {
        let target = {
            let table = self.table_state.read(cx);
            let visible_indices = &table.delegate().visible_indices;
            self.selected_index
                .and_then(|index| visible_indices.iter().position(|&i| i == index).map(|row| (index, row)))
                .or_else(|| visible_indices.first().copied().map(|index| (index, 0)))
        };
        if let Some((index, row_ix)) = target {
            self.select_item_at_row(index, Some(row_ix), false, cx);
        }
        cx.emit(ZlibPanelEvent::FocusEditor);
        window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
    }

    fn dismiss_table(&mut self, _: &DismissTable, window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ZlibPanelEvent::FocusEditor);
        window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
    }

    fn clear_results(&mut self, _: &ClearResults, _: &mut Window, cx: &mut Context<Self>) {
        self.invalidate_scan();
        self.sync_selection_to_filter(cx);
        cx.notify();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ZlibRowColors {
    pub bg_color: Hsla,
    pub border_color: Hsla,
    pub hover_bg: Hsla,
    pub address_color: Hsla,
    pub size_color: Hsla,
    pub value_color: Hsla,
}

pub(crate) fn zlib_row_colors(is_selected: bool, is_table_focused: bool, theme: &Theme) -> ZlibRowColors {
    if is_selected {
        if is_table_focused {
            ZlibRowColors {
                bg_color: theme.selection,
                border_color: theme.accent,
                hover_bg: theme.selection,
                address_color: theme.accent_foreground,
                size_color: theme.accent_foreground,
                value_color: theme.accent_foreground,
            }
        } else {
            ZlibRowColors {
                bg_color: theme.muted,
                border_color: theme.border,
                hover_bg: theme.muted,
                address_color: theme.foreground,
                size_color: theme.muted_foreground,
                value_color: theme.foreground,
            }
        }
    } else {
        ZlibRowColors {
            bg_color: theme.sidebar,
            border_color: theme.table_row_border,
            hover_bg: theme.table_hover,
            address_color: theme.foreground,
            size_color: theme.muted_foreground,
            value_color: theme.foreground,
        }
    }
}

impl Render for ZlibPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let is_input_focused = self.filter_input.read(cx).focus_handle(cx).is_focused(window);
        let is_table_focused = self.table_state.read(cx).focus_handle(cx).is_focused(window);
        let is_focused = self.focus_handle.is_focused(window) || is_input_focused || is_table_focused;
        let has_editor = self.editor.is_some();
        let can_decompress_selected = has_editor && self.selected_index.is_some() && !self.is_scanning && !self.is_decompressing;

        let actions = Button::new("clear-zlib")
            .ghost()
            .icon(IconName::Eraser)
            .with_size(Size::XSmall)
            .tooltip("Clear results")
            .disabled(!self.has_scanned && !self.is_scanning)
            .on_click(cx.listener(|this, _, _window, cx| {
                this.invalidate_scan();
                this.sync_selection_to_filter(cx);
                cx.notify();
            }));
        let header = crate::ui::panels::panel_header("ZLIB", is_focused, theme, None, Some(actions.into_any_element()));

        let scan_controls = h_flex().p_2().gap_2().border_b_1().border_color(theme.border).items_center().child(
            div().flex_1().child(
                Button::new("search-zlib")
                    .label(if self.is_scanning { "Searching..." } else { "Search" })
                    .icon(IconName::Search)
                    .primary()
                    .with_size(Size::Small)
                    .w_full()
                    .disabled(self.is_scanning || !has_editor)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.trigger_scan(cx);
                    })),
            ),
        );

        let result_summary = div()
            .px_2()
            .py_1()
            .bg(theme.secondary)
            .border_b_1()
            .border_color(theme.border)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(self.result_summary());

        let filter_controls = h_flex()
            .p_2()
            .gap_1p5()
            .items_center()
            .border_b_1()
            .border_color(theme.border)
            .child(div().flex_1().child(Input::new(&self.filter_input)))
            .child(
                Button::new("clear-zlib-filter")
                    .ghost()
                    .icon(IconName::Close)
                    .with_size(Size::XSmall)
                    .tooltip("Clear filter")
                    .disabled(self.filter_query(cx).is_empty())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.filter_input.update(cx, |input, cx| {
                            input.set_value(String::new(), window, cx);
                        });
                        this.sync_selection_to_filter(cx);
                        cx.notify();
                    })),
            );

        let body = if self.is_scanning {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Searching...")
                .into_any_element()
        } else if self.results.is_empty() {
            let message = if self.has_scanned {
                "No zlib streams found"
            } else if has_editor {
                "Click Search to find zlib streams"
            } else {
                "Open a file to search zlib streams"
            };
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(message)
                .into_any_element()
        } else {
            div()
                .flex_1()
                .overflow_hidden()
                .child(DataTable::new(&self.table_state).bordered(false))
                .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        this.dismiss_table(&DismissTable, window, cx);
                        cx.stop_propagation();
                    }
                }))
                .into_any_element()
        };

        let show_footer = !self.is_scanning && !self.results.is_empty();
        let footer = h_flex()
            .p_2()
            .gap_2()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .items_center()
            .child(
                div().flex_1().child(
                    Button::new("open-decompressed-zlib")
                        .label(if self.is_decompressing { "Decompressing..." } else { "Open Decompressed" })
                        .icon(IconName::ExternalLink)
                        .primary()
                        .with_size(Size::Small)
                        .w_full()
                        .tooltip("Decompress selected zlib stream into a new document tab")
                        .disabled(!can_decompress_selected)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.decompress_selected(window, cx);
                        })),
                ),
            );

        crate::ui::panels::panel_container(is_focused, theme)
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::focus_table))
            .on_action(cx.listener(Self::confirm_select))
            .on_action(cx.listener(Self::dismiss_table))
            .on_action(cx.listener(|this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                this.dismiss_table(&DismissTable, window, cx);
            }))
            .on_action(cx.listener(Self::copy_address))
            .on_action(cx.listener(Self::copy_preview))
            .on_action(cx.listener(Self::decompress_entry))
            .on_action(cx.listener(Self::clear_results))
            .child(header)
            .child(scan_controls)
            .when(self.has_scanned || self.is_scanning, |this| this.child(result_summary).child(filter_controls))
            .child(body)
            .when(show_footer, |this| this.child(footer))
    }
}

fn preview_text(text: &str) -> std::borrow::Cow<'_, str> {
    const PREVIEW_LIMIT: usize = 256;
    if text.len() <= PREVIEW_LIMIT {
        return std::borrow::Cow::Borrowed(text);
    }
    if let Some((end_byte, _)) = text.char_indices().nth(PREVIEW_LIMIT) {
        let mut preview = String::with_capacity(end_byte + '…'.len_utf8());
        preview.push_str(&text[..end_byte]);
        preview.push('…');
        std::borrow::Cow::Owned(preview)
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

impl Focusable for ZlibPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AddressLabelCache, AddressMap, ColumnSort, HexCase, Theme, ZlibMatch, ZlibPanelEvent, build_zlib_cell_meta, filter_zlib_indices, sort_zlib_indices,
        zlib_row_colors,
    };
    use crate::core::address_map::MemorySegment;
    use gpui_kit::{Hsla, KeyBindingContextPredicate, KeyContext, Rgba};

    #[test]
    fn test_zlib_row_colors_states() {
        let theme = Theme::default();

        let unselected = zlib_row_colors(false, false, &theme);
        assert_eq!(unselected.bg_color, theme.sidebar);
        assert_eq!(unselected.border_color, theme.table_row_border);
        assert_eq!(unselected.address_color, theme.foreground);
        assert_eq!(unselected.size_color, theme.muted_foreground);
        assert_eq!(unselected.value_color, theme.foreground);

        let selected_focused = zlib_row_colors(true, true, &theme);
        assert_eq!(selected_focused.bg_color, theme.selection);
        assert_eq!(selected_focused.border_color, theme.accent);
        assert_eq!(selected_focused.address_color, theme.accent_foreground);
        assert_eq!(selected_focused.size_color, theme.accent_foreground);
        assert_eq!(selected_focused.value_color, theme.accent_foreground);

        let selected_unfocused = zlib_row_colors(true, false, &theme);
        assert_eq!(selected_unfocused.bg_color, theme.muted);
        assert_eq!(selected_unfocused.border_color, theme.border);
        assert_eq!(selected_unfocused.address_color, theme.foreground);
        assert_eq!(selected_unfocused.size_color, theme.muted_foreground);
        assert_eq!(selected_unfocused.value_color, theme.foreground);
    }

    #[test]
    fn test_zlib_address_contrast_across_all_embedded_themes() {
        use crate::assets::Assets;
        use crate::theme::{load_themes_from_assets, theme_names};

        let registry = load_themes_from_assets(&Assets);
        let names = theme_names(registry.themes());
        assert!(!names.is_empty());

        fn channel_to_linear(c: f32) -> f32 {
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }

        fn rel_luminance(c: Hsla) -> f32 {
            let rgba = Rgba::from(c);
            0.2126 * channel_to_linear(rgba.r) + 0.7152 * channel_to_linear(rgba.g) + 0.0722 * channel_to_linear(rgba.b)
        }

        fn contrast_ratio(c1: Hsla, c2: Hsla) -> f32 {
            let l1 = rel_luminance(c1);
            let l2 = rel_luminance(c2);
            let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
            (lighter + 0.05) / (darker + 0.05)
        }

        for name in names {
            let config = registry.themes().get(&name).unwrap();
            let mut theme = Theme::default();
            theme.apply_config(config);

            let colors = zlib_row_colors(false, false, &theme);
            let ratio = contrast_ratio(colors.address_color, colors.bg_color);
            assert!(
                ratio >= 4.5,
                "Theme '{}' failed WCAG AA contrast for address: ratio was {:.2}:1 (expected >= 4.5:1)",
                name,
                ratio
            );
        }
    }

    #[test]
    fn test_sort_zlib_indices_all_columns() {
        let results = vec![
            ZlibMatch {
                offset: 0x30,
                compressed_len: 30,
                decompressed_len: 100,
                header: [0x78, 0x9C],
                preview: "bbb".into(),
            },
            ZlibMatch {
                offset: 0x10,
                compressed_len: 80,
                decompressed_len: 50,
                header: [0x78, 0x9C],
                preview: "ccc".into(),
            },
            ZlibMatch {
                offset: 0x20,
                compressed_len: 50,
                decompressed_len: 200,
                header: [0x78, 0x9C],
                preview: "aaa".into(),
            },
        ];

        let mut indices = vec![0, 1, 2];
        sort_zlib_indices(&results, &mut indices, Some((0, ColumnSort::Ascending)));
        assert_eq!(indices, vec![1, 2, 0]);

        sort_zlib_indices(&results, &mut indices, Some((1, ColumnSort::Descending)));
        assert_eq!(indices, vec![1, 2, 0]);

        sort_zlib_indices(&results, &mut indices, Some((2, ColumnSort::Descending)));
        assert_eq!(indices, vec![2, 0, 1]);

        sort_zlib_indices(&results, &mut indices, Some((3, ColumnSort::Ascending)));
        assert_eq!(indices, vec![2, 0, 1]);

        sort_zlib_indices(&results, &mut indices, Some((3, ColumnSort::Default)));
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_filter_zlib_indices_by_preview_and_address() {
        let results = vec![
            ZlibMatch {
                offset: 0x10,
                compressed_len: 24,
                decompressed_len: 40,
                header: [0x78, 0x9C],
                preview: "PNG image chunk".into(),
            },
            ZlibMatch {
                offset: 0x40,
                compressed_len: 18,
                decompressed_len: 20,
                header: [0x78, 0x9C],
                preview: "89 50 4E 47".into(),
            },
        ];
        let map = AddressMap::from_segments(vec![MemorySegment {
            buffer_offset: 0,
            address: 0x1000,
            length: 0x100,
        }]);

        assert_eq!(filter_zlib_indices(&results, &map, HexCase::Lower, ""), vec![0, 1]);
        assert_eq!(filter_zlib_indices(&results, &map, HexCase::Lower, "chunk"), vec![0]);
        assert_eq!(filter_zlib_indices(&results, &map, HexCase::Lower, "png"), vec![0]);
        assert_eq!(filter_zlib_indices(&results, &map, HexCase::Lower, "0x1040"), vec![1]);
        assert_eq!(filter_zlib_indices(&results, &map, HexCase::Upper, "00001010"), vec![0]);
    }

    #[test]
    fn test_build_zlib_cell_meta_and_address_cache() {
        let results = [ZlibMatch {
            offset: 0x1A,
            compressed_len: 42,
            decompressed_len: 128,
            header: [0x78, 0x9C],
            preview: "hello zlib".into(),
        }];
        let meta = build_zlib_cell_meta(&results);
        assert_eq!(meta.len(), 1);
        assert_eq!(meta[0].compressed_label.as_ref(), "42 B");
        assert_eq!(meta[0].decompressed_label.as_ref(), "128 B");
        assert_eq!(meta[0].preview.as_ref(), "hello zlib");
        assert_eq!(meta[0].preview_char_count, 10);

        let map = AddressMap::from_segments(vec![MemorySegment {
            buffer_offset: 0,
            address: 0x0800_0000,
            length: 64,
        }]);
        let mut cache = AddressLabelCache::new(1, None, HexCase::Upper, map, 1);
        assert_eq!(cache.get_or_format(0, 0x1A).as_ref(), "0x0800001A");
        assert!(cache.matches(1, None, HexCase::Upper, 1));
        assert!(!cache.matches(1, None, HexCase::Lower, 1));
    }

    #[test]
    fn test_zlib_panel_key_context_predicate() {
        let event = ZlibPanelEvent::FocusEditor;
        assert!(matches!(event, ZlibPanelEvent::FocusEditor));

        let parent_ctx = KeyContext::try_from("ZlibPanel").unwrap();
        let child_ctx = KeyContext::try_from("DataTable").unwrap();
        let contexts = vec![parent_ctx, child_ctx];

        let pred_desc = KeyBindingContextPredicate::parse("ZlibPanel > DataTable").unwrap();
        assert!(pred_desc.eval(&contexts));
        assert_eq!(pred_desc.depth_of(&contexts), Some(2));
    }
}
