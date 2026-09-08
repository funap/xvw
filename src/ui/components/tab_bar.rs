use crate::ui::icon::IconName;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::path::PathBuf;

use crate::actions::{ActivateTab, CloseActivePanel};

#[allow(dead_code)]
pub struct TabItemInfo {
    pub id: usize,
    pub title: String,
    pub is_dirty: bool,
    pub is_read_only: bool,
    pub is_active: bool,
    #[allow(dead_code)]
    pub path: Option<PathBuf>,
}

#[allow(dead_code)]
pub fn render_zed_tab_bar(tabs: &[TabItemInfo], _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let theme = cx.theme();
    let tab_bar_bg = theme.tab_bar;

    div()
        .id("zed-tab-bar")
        .flex()
        .flex_row()
        .items_center()
        .w_full()
        .h(px(34.0))
        .bg(tab_bar_bg)
        .border_b_1()
        .border_color(theme.border)
        .overflow_x_scroll()
        .children(tabs.iter().enumerate().map(|(idx, tab)| {
            let tab_id = tab.id;
            let is_active = tab.is_active;
            let is_dirty = tab.is_dirty;
            let is_read_only = tab.is_read_only;
            let title = tab.title.clone();
            let one_based_index = idx + 1;

            div()
                .id(ElementId::NamedInteger("zed-tab".into(), tab_id as u64))
                .relative()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .h_full()
                .min_w(px(100.0))
                .max_w(px(220.0))
                .cursor_pointer()
                .border_r_1()
                .border_color(theme.border)
                .when(is_active, |s| {
                    s.bg(theme.background)
                        .text_color(theme.foreground)
                        .child(div().absolute().top_0().left_0().right_0().h(px(2.0)).bg(theme.primary))
                })
                .when(!is_active, |s| {
                    s.bg(tab_bar_bg)
                        .text_color(theme.muted_foreground)
                        .hover(|style| style.bg(theme.accent.opacity(0.12)))
                })
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.dispatch_action(Box::new(ActivateTab { index: one_based_index }), cx);
                })
                .on_mouse_down(MouseButton::Middle, move |_, window, cx| {
                    window.dispatch_action(Box::new(CloseActivePanel), cx);
                })
                .context_menu({
                    let can_close_others = tabs.len() > 1;
                    let can_close_right = idx + 1 < tabs.len();
                    let has_saved = tabs.iter().any(|t| !t.is_dirty);
                    let tab_path = tab.path.clone();
                    let active_tab_path = tabs.iter().find(|t| t.is_active).and_then(|t| t.path.clone());
                    let pending_compare_path = crate::app_state::PendingCompareState::path(cx);
                    let other_open_tabs: Vec<(String, String)> = tabs
                        .iter()
                        .filter_map(|t| {
                            if t.id != tab_id
                                && let Some(p) = &t.path
                                && t.path != tab.path
                            {
                                let p_str = p.to_string_lossy().to_string();
                                return Some((t.title.clone(), p_str));
                            }
                            None
                        })
                        .collect();
                    move |menu, window, cx| {
                        let mut menu = menu
                            .menu_with_icon("Close", IconName::Close, Box::new(crate::actions::CloseActivePanel))
                            .menu_with_icon_and_disabled("Close Others", IconName::Close, Box::new(crate::actions::CloseOtherTabs), !can_close_others)
                            .menu_with_icon_and_disabled(
                                "Close to the Right",
                                IconName::ChevronRight,
                                Box::new(crate::actions::CloseTabsToRight),
                                !can_close_right,
                            )
                            .menu_with_icon_and_disabled("Close Saved", IconName::Check, Box::new(crate::actions::CloseSavedTabs), !has_saved)
                            .menu_with_icon("Close All", IconName::Close, Box::new(crate::actions::CloseAllTabs))
                            .separator()
                            .menu_with_icon("Split Right", IconName::PanelRight, Box::new(crate::actions::SplitRight))
                            .menu_with_icon("Split Down", IconName::PanelBottom, Box::new(crate::actions::SplitDown));

                        if let Some(current_path) = &tab_path {
                            let current_path_str = current_path.to_string_lossy().to_string();
                            menu = menu.separator();

                            menu = menu.menu_with_icon(
                                "Select for Compare",
                                IconName::GitCompare,
                                Box::new(crate::actions::SelectForCompare {
                                    path: current_path_str.clone(),
                                }),
                            );

                            if let Some(pending_path) = &pending_compare_path
                                && pending_path != &current_path_str
                            {
                                let pending_name = std::path::Path::new(pending_path)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "Selected".to_string());
                                menu = menu.menu_with_icon(
                                    format!("Compare with '{}'", pending_name),
                                    IconName::GitCompare,
                                    Box::new(crate::actions::OpenDiff {
                                        left_path: pending_path.clone(),
                                        right_path: current_path_str.clone(),
                                    }),
                                );
                            }

                            if !is_active
                                && let Some(active_p) = &active_tab_path
                                && active_p != current_path
                            {
                                menu = menu.menu_with_icon(
                                    "Compare with Active File",
                                    IconName::GitCompare,
                                    Box::new(crate::actions::OpenDiff {
                                        left_path: active_p.to_string_lossy().to_string(),
                                        right_path: current_path_str.clone(),
                                    }),
                                );
                            }

                            if other_open_tabs.len() == 1 {
                                let (other_title, other_path) = &other_open_tabs[0];
                                let is_already_shown = pending_compare_path.as_deref() == Some(other_path.as_str())
                                    || (!is_active
                                        && active_tab_path.as_ref().map(|p| p.to_string_lossy().to_string()).as_deref() == Some(other_path.as_str()));
                                if !is_already_shown {
                                    menu = menu.menu_with_icon(
                                        format!("Compare with '{}'", other_title),
                                        IconName::GitCompare,
                                        Box::new(crate::actions::OpenDiff {
                                            left_path: current_path_str.clone(),
                                            right_path: other_path.clone(),
                                        }),
                                    );
                                }
                            } else if other_open_tabs.len() > 1 {
                                let current_path_str_clone = current_path_str.clone();
                                let other_tabs_clone = other_open_tabs.clone();
                                menu = menu.submenu("Compare with...", window, cx, move |menu, _window, _cx| {
                                    let mut sub = menu;
                                    for (title, path) in &other_tabs_clone {
                                        sub = sub.menu_with_icon(
                                            title.clone(),
                                            IconName::GitCompare,
                                            Box::new(crate::actions::OpenDiff {
                                                left_path: current_path_str_clone.clone(),
                                                right_path: path.clone(),
                                            }),
                                        );
                                    }
                                    sub
                                });
                            }
                        }

                        if tab_path.is_some() {
                            menu = menu
                                .separator()
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
                                );
                        }
                        menu
                    }
                })
                .child(
                    Icon::new(if is_read_only { IconName::PenOff } else { IconName::File })
                        .size(px(14.0))
                        .text_color(if is_active { theme.primary } else { theme.muted_foreground }),
                )
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_sm()
                        .when(is_active, |s| s.font_weight(FontWeight::MEDIUM))
                        .child(title),
                )
                .child(
                    div()
                        .id(ElementId::NamedInteger("tab-close-area".into(), tab_id as u64))
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(18.0))
                        .h(px(18.0))
                        .rounded_sm()
                        .hover(|style| style.bg(theme.accent.opacity(0.2)).text_color(theme.accent_foreground))
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            window.dispatch_action(Box::new(ActivateTab { index: one_based_index }), cx);
                            window.dispatch_action(Box::new(CloseActivePanel), cx);
                        })
                        .child(if is_dirty && !is_active {
                            div().w(px(6.0)).h(px(6.0)).rounded_full().bg(theme.primary).into_any_element()
                        } else {
                            Icon::new(IconName::Close).size(px(12.0)).text_color(theme.muted_foreground).into_any_element()
                        }),
                )
        }))
}
