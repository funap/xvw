use super::ZlibPanel;
use super::model::{AddressLabelCache, ZlibResultRow, sort_zlib_indices, zlib_cell_colors};
use crate::core::appearance::Appearance;
use crate::core::editor::Editor;
use crate::core::format::HexCase;
use crate::ui::icon::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::*;
use std::sync::Arc;

#[derive(Clone, PartialEq, Action)]
#[action(namespace = zlib_panel, no_json)]
pub(super) struct CopyAddress {
    pub value: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = zlib_panel, no_json)]
pub(super) struct CopyPreview {
    pub value: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = zlib_panel, no_json)]
pub(super) struct DecompressEntry {
    pub index: usize,
    pub offset: usize,
    pub compressed_len: usize,
}

/// Table delegate backing [`ZlibPanel`]'s result grid.
pub struct ZlibTableDelegate {
    panel: WeakEntity<ZlibPanel>,
    results_id: u64,
    rows: Arc<[ZlibResultRow]>,
    visible_indices: Vec<usize>,
    sort_column: Option<(usize, ColumnSort)>,
    editor: Option<Entity<Editor>>,
    address_cache: Option<AddressLabelCache>,
    selected_index: Option<usize>,
    focus_handle: FocusHandle,
}

impl ZlibTableDelegate {
    pub(super) fn new(panel: WeakEntity<ZlibPanel>, focus_handle: FocusHandle) -> Self {
        Self {
            panel,
            results_id: 0,
            rows: Arc::default(),
            visible_indices: Vec::new(),
            sort_column: None,
            editor: None,
            address_cache: None,
            selected_index: None,
            focus_handle,
        }
    }

    pub(super) fn visible_indices(&self) -> &[usize] {
        &self.visible_indices
    }

    pub(super) fn set_selected_index(&mut self, index: Option<usize>) {
        self.selected_index = index;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn sync_state(
        &mut self,
        results_id: u64,
        rows: Arc<[ZlibResultRow]>,
        visible_indices: Vec<usize>,
        sort_column: Option<(usize, ColumnSort)>,
        editor: Option<Entity<Editor>>,
        selected_index: Option<usize>,
    ) {
        let editor_changed = self.editor.as_ref().map(Entity::entity_id) != editor.as_ref().map(Entity::entity_id);
        if self.results_id != results_id || editor_changed {
            self.address_cache = None;
        }
        self.results_id = results_id;
        self.rows = rows;
        self.visible_indices = visible_indices;
        self.sort_column = sort_column;
        self.editor = editor;
        self.selected_index = selected_index;
    }

    fn ensure_address_cache(&mut self, hex_case: HexCase, cx: &App) -> &mut AddressLabelCache {
        let editor_id = self.editor.as_ref().map(Entity::entity_id);
        let len = self.rows.len();
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
        let offset = self.rows.get(index)?.offset;
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
        sort_zlib_indices(&self.rows, &mut self.visible_indices, self.sort_column);

        let selected_row = self.selected_index.and_then(|idx| self.visible_indices.iter().position(|&i| i == idx));
        if let Some(panel) = self.panel.upgrade() {
            cx.defer(move |cx| {
                panel.update(cx, |panel, cx| {
                    panel.on_table_sorted(sort_col, selected_row, cx);
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
                .filter_map(|&index| self.rows.get(index))
                .map(|row| row.compressed_label.len())
                .max()
                .unwrap_or(0),
            2 => self
                .visible_indices
                .iter()
                .take(128)
                .filter_map(|&index| self.rows.get(index))
                .map(|row| row.decompressed_label.len())
                .max()
                .unwrap_or(0),
            3 => self
                .visible_indices
                .iter()
                .take(128)
                .filter_map(|&index| self.rows.get(index))
                .map(|row| row.preview_char_count)
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
        let Some(row) = self.rows.get(index) else {
            return menu;
        };
        let offset = row.offset;
        let compressed_len = row.compressed_len;
        let preview_value = row.preview.to_string();
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
        if index >= self.rows.len() {
            return div().into_any_element();
        }

        let theme = cx.theme();
        let font_family = cx.global::<Appearance>().font_family.clone();
        let colors = zlib_cell_colors(theme);

        let (color, text): (Hsla, SharedString) = match col_ix {
            0 => {
                let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
                let Some(addr) = self.cached_address_label(index, hex_case, cx) else {
                    return div().into_any_element();
                };
                (colors.address_color, addr)
            }
            1 => {
                let Some(row) = self.rows.get(index) else {
                    return div().into_any_element();
                };
                (colors.size_color, row.compressed_label.clone())
            }
            2 => {
                let Some(row) = self.rows.get(index) else {
                    return div().into_any_element();
                };
                (colors.size_color, row.decompressed_label.clone())
            }
            3 => {
                let Some(row) = self.rows.get(index) else {
                    return div().into_any_element();
                };
                (colors.preview_color, row.preview.clone())
            }
            _ => return div().into_any_element(),
        };

        div().text_xs().font_family(font_family).text_color(color).child(text).into_any_element()
    }
}
