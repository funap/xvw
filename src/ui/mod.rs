pub mod appearance;
pub mod color;
pub mod components;
pub mod dialogs;
pub mod icon;
pub mod menus;
pub mod pane;
pub mod panels;
pub mod views;
pub mod workspace;

impl gpui_kit::Global for crate::core::layout::BytesPerRow {}
impl gpui_kit::Global for crate::core::encoding::Encoding {}
