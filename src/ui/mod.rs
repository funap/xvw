pub mod appearance;
pub mod color;
pub mod components;
pub mod dialogs;
pub mod icon;
pub mod menus;
pub mod notification;
pub mod pane;
pub mod panels;
pub mod views;
pub mod workspace;

impl gpui_kit::Global for crate::core::layout::BytesPerRow {}
impl gpui_kit::Global for crate::core::encoding::Encoding {}
impl gpui_kit::Global for crate::core::radix::DisplayRadix {}
impl gpui_kit::Global for crate::core::radix::ByteGroupSize {}
impl gpui_kit::Global for crate::core::radix::ByteOrder {}
impl gpui_kit::Global for crate::core::structure::StructureYamlShaThreshold {}
impl gpui_kit::Global for crate::core::structure::StructureYamlIncludeOffsets {}

/// Initializes all UI panels, dialogs, views, and menus.
pub fn init(cx: &mut gpui_kit::App) {
    workspace::init(cx);
    workspace::title_bar::init(cx);
    dialogs::new_file_modal::init(cx);
    dialogs::fill_selection_modal::init(cx);
    components::data_table::init(cx);
    panels::file_tree_view::init(cx);
    panels::search_panel::init(cx);
    panels::strings_panel::init(cx);
    panels::struct_tree_view::init(cx);
    panels::bookmark_panel::init(cx);
    panels::data_inspector::init(cx);
    views::editor_view::init(cx);
    views::diff_view::init(cx);

    // Register top application menu bar
    cx.set_menus(menus::application_menus().iter().map(|menu| menu.to_gpui_menu()));
}
