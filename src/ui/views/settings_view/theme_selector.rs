use crate::ui::icon::IconName;
use gpui_kit::component::{
    ActiveTheme, Icon, IndexPath,
    searchable_list::{SearchableListDelegate, SearchableListItem},
    select::SelectState,
};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, SharedString, WeakEntity, Window, div, px};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemePreviewState {
    origin: Option<SharedString>,
    current_index: Option<usize>,
    confirmed: bool,
    open: bool,
}

impl ThemePreviewState {
    /// Returns whether the theme dropdown popup is currently open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Sets whether the theme dropdown popup is open.
    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// Records the current theme as origin if not already previewing.
    pub fn start_preview(&mut self, current_theme: SharedString) {
        if self.origin.is_none() {
            self.origin = Some(current_theme);
            self.confirmed = false;
        }
    }

    /// Sets the current preview index (e.g. from mouse hover).
    pub fn set_hover_index(&mut self, index: usize) {
        self.current_index = Some(index);
    }

    /// Steps selection index up or down across `total_items`, matching PopupMenu keyboard behavior.
    /// Returns the newly selected index.
    #[allow(dead_code)]
    pub fn step_index(&mut self, step_down: bool, total_items: usize) -> Option<usize> {
        if total_items == 0 {
            return None;
        }
        let next_idx = match self.current_index {
            None => {
                if step_down {
                    0
                } else {
                    total_items - 1
                }
            }
            Some(idx) => {
                if step_down {
                    if idx + 1 >= total_items { 0 } else { idx + 1 }
                } else {
                    if idx == 0 { total_items - 1 } else { idx - 1 }
                }
            }
        };
        self.current_index = Some(next_idx);
        Some(next_idx)
    }

    /// Marks the selected theme as confirmed and clears origin so dismissal will not revert.
    pub fn confirm(&mut self) {
        self.confirmed = true;
        self.origin = None;
        self.current_index = None;
        self.open = false;
    }

    /// Cancels preview and returns the original theme to restore, if not confirmed.
    pub fn cancel(&mut self) -> Option<SharedString> {
        self.current_index = None;
        self.open = false;
        let is_confirmed = self.confirmed;
        self.confirmed = false;
        if !is_confirmed {
            self.origin.take()
        } else {
            self.origin = None;
            None
        }
    }

    /// Returns the original saved theme name if actively previewing, or None.
    pub fn origin(&self) -> Option<&SharedString> {
        self.origin.as_ref()
    }

    /// Returns the currently previewed index, if any.
    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }
}

/// Calculates the next index when stepping up or down across a list of items of length `total`,
/// starting from `current_index`. Stepping down advances with wraparound; stepping up retreats with wraparound.
pub fn cycle_theme_index(current_index: usize, step_down: bool, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    if step_down {
        if current_index + 1 >= total { 0 } else { current_index + 1 }
    } else {
        if current_index == 0 { total - 1 } else { current_index - 1 }
    }
}

#[derive(Clone)]
pub struct ThemeListDelegate {
    themes: Vec<SharedString>,
    preview_state: Rc<RefCell<ThemePreviewState>>,
    select_handle: Rc<RefCell<Option<WeakEntity<SelectState<Self>>>>>,
}

impl ThemeListDelegate {
    pub fn new(themes: Vec<SharedString>, preview_state: Rc<RefCell<ThemePreviewState>>) -> Self {
        Self {
            themes,
            preview_state,
            select_handle: Rc::new(RefCell::new(None)),
        }
    }

    pub fn with_select_handle(mut self, select_handle: Rc<RefCell<Option<WeakEntity<SelectState<Self>>>>>) -> Self {
        self.select_handle = select_handle;
        self
    }

    /// Checks whether an item should display the checkmark.
    /// When actively previewing, the checkmark stays firmly on the original saved theme.
    pub fn check_item(&self, item: &SharedString, current_selection: &[(IndexPath, SharedString)]) -> bool {
        if let Some(origin) = self.preview_state.borrow().origin() {
            item == origin
        } else {
            current_selection.iter().any(|(_, selected_item)| selected_item.value() == item.value())
        }
    }
}

impl SearchableListDelegate for ThemeListDelegate {
    type Item = SharedString;

    fn items_count(&self, _section: usize) -> usize {
        self.themes.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&Self::Item> {
        self.themes.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        Self::Item: SearchableListItem<Value = V>,
        V: PartialEq,
    {
        self.themes.iter().position(|item| item.value() == value).map(IndexPath::new)
    }

    fn is_item_checked(&self, _ix: IndexPath, item: &Self::Item, current_selection: &[(IndexPath, Self::Item)], _cx: &App) -> bool {
        self.check_item(item, current_selection)
    }

    fn render_item(&self, ix: IndexPath, item: &Self::Item, checked: bool, _window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let name = item.clone();
        let preview_state = self.preview_state.clone();
        let select_handle = self.select_handle.clone();
        {
            let mut state = preview_state.borrow_mut();
            state.set_open(true);
            if checked && state.current_index().is_none() {
                state.start_preview(cx.theme().theme_name().clone());
                state.set_hover_index(ix.row);
            }
        }
        Some(
            div()
                .id(SharedString::from(format!("theme-item-{}", name)))
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().truncate().child(name.clone()))
                .when(checked, |this| this.child(Icon::new(IconName::Check).size(px(14.0)).flex_shrink_0()))
                .on_hover(move |hovered, window, cx| {
                    if *hovered {
                        let weak = select_handle.borrow().clone();
                        if let Some(weak) = weak
                            && let Some(theme_select) = weak.upgrade()
                        {
                            theme_select.update(cx, |select, cx| {
                                if select.selected_index(cx) != Some(ix) {
                                    select.set_selected_index(Some(ix), window, cx);
                                }
                            });
                        }

                        let (need_apply, theme_name) = {
                            let mut state = preview_state.borrow_mut();
                            state.set_hover_index(ix.row);
                            if cx.theme().theme_name() != name.as_ref() {
                                state.start_preview(cx.theme().theme_name().clone());
                                (true, Some(name.clone()))
                            } else {
                                (false, None)
                            }
                        };
                        if need_apply && let Some(theme_name) = theme_name {
                            crate::theme::apply_theme_by_name(&theme_name, Some(window), cx);
                        }
                    }
                })
                .into_any_element(),
        )
    }
}
