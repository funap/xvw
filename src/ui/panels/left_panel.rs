use gpui_kit::*;

use crate::core::editor::Editor;
use crate::ui::components::bookmark_panel::BookmarkPanel;
use crate::ui::components::checksum_panel::ChecksumPanel;
use crate::ui::components::data_inspector::DataInspector;
use crate::ui::components::file_tree_view::{FileTreeView, FileTreeViewEvent};
use crate::ui::components::search_panel::{SearchPanel, SearchPanelEvent};
use crate::ui::components::strings_panel::{StringsPanel, StringsPanelEvent};
use crate::ui::components::struct_tree_view::StructTreeView;
use crate::ui::panels::visual_map_panel::VisualMapPanel;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
pub enum LeftPanelTab {
    Files,
    Search,
    Strings,
    Structure,
    Inspector,
    Map,
    Checksum,
    Bookmarks,
}

pub struct LeftPanel {
    pub file_tree: Entity<FileTreeView>,
    pub search_panel: Entity<SearchPanel>,
    pub strings_panel: Entity<StringsPanel>,
    pub struct_tree: Entity<StructTreeView>,
    pub data_inspector: Entity<DataInspector>,
    pub visual_map: Entity<VisualMapPanel>,
    pub checksum_panel: Entity<ChecksumPanel>,
    pub bookmark_panel: Entity<BookmarkPanel>,
    pub active_tab: LeftPanelTab,
}

impl EventEmitter<FileTreeViewEvent> for LeftPanel {}
impl EventEmitter<SearchPanelEvent> for LeftPanel {}
impl EventEmitter<StringsPanelEvent> for LeftPanel {}

impl LeftPanel {
    pub fn new(file_tree: Entity<FileTreeView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_panel = cx.new(|cx| SearchPanel::new(None, window, cx));
        let strings_panel = cx.new(|cx| StringsPanel::new(None, window, cx));
        let struct_tree = cx.new(|cx| StructTreeView::new(None, None, cx));
        let data_inspector = cx.new(|cx| DataInspector::new(None, window, cx));
        let visual_map = cx.new(|cx| VisualMapPanel::new(None, cx));
        let checksum_panel = cx.new(|cx| ChecksumPanel::new(None, cx));
        let bookmark_panel = cx.new(|cx| BookmarkPanel::new(None, window, cx));

        cx.subscribe(&file_tree, |_, _, event: &FileTreeViewEvent, cx| match event {
            FileTreeViewEvent::OpenFile { path, format } => cx.emit(FileTreeViewEvent::OpenFile {
                path: path.clone(),
                format: *format,
            }),
        })
        .detach();

        cx.subscribe(&search_panel, |_, _, event: &SearchPanelEvent, cx| {
            cx.emit(event.clone());
        })
        .detach();

        cx.subscribe(&strings_panel, |_, _, event: &StringsPanelEvent, cx| {
            cx.emit(event.clone());
        })
        .detach();

        Self {
            file_tree,
            search_panel,
            strings_panel,
            struct_tree,
            data_inspector,
            visual_map,
            checksum_panel,
            bookmark_panel,
            active_tab: LeftPanelTab::Files,
        }
    }

    pub fn set_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        self.search_panel.update(cx, |panel, cx| {
            panel.set_editor(editor.clone(), cx);
        });
        self.strings_panel.update(cx, |panel, cx| {
            panel.set_editor(editor.clone(), cx);
        });
        self.struct_tree.update(cx, |panel, cx| {
            panel.set_editor(editor.clone(), cx);
        });
        self.data_inspector.update(cx, |panel, cx| {
            panel.set_editor(editor.clone(), cx);
        });
        self.visual_map.update(cx, |panel, cx| {
            panel.set_editor(editor.clone(), cx);
        });
        self.checksum_panel.update(cx, |panel, cx| {
            panel.set_editor(editor.clone(), cx);
        });
        self.bookmark_panel.update(cx, |panel, cx| {
            panel.set_editor(editor, cx);
        });
    }

    /// Updates the recent structure definition paths shown by the Structure panel.
    pub fn set_structure_definition_history(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        self.struct_tree.update(cx, |panel, cx| {
            panel.set_definition_history(paths, cx);
        });
    }

    /// Updates the recent binary file entries shown by the Files panel.
    pub fn set_file_history(&mut self, entries: &[crate::core::structure::RecentFileEntry], cx: &mut Context<Self>) {
        self.file_tree.update(cx, |panel, cx| {
            panel.set_recent_file_history(entries, cx);
        });
    }

    /// Synchronizes the recent file paths shown by the Files panel with latest history.
    pub fn sync_file_history(&mut self, cx: &mut Context<Self>) {
        self.file_tree.update(cx, |panel, cx| {
            panel.sync_recent_file_history(cx);
        });
    }

    pub fn set_tab(&mut self, tab: LeftPanelTab, cx: &mut Context<Self>) {
        if tab == LeftPanelTab::Files || self.active_tab == LeftPanelTab::Files {
            self.file_tree.update(cx, |panel, cx| {
                panel.sync_recent_file_history(cx);
            });
        }
        self.active_tab = tab;
        cx.notify();
    }
}

impl Render for LeftPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .min_w_0()
            .min_h_0()
            .child(match self.active_tab {
                LeftPanelTab::Files => self.file_tree.clone().into_any_element(),
                LeftPanelTab::Search => self.search_panel.clone().into_any_element(),
                LeftPanelTab::Strings => self.strings_panel.clone().into_any_element(),
                LeftPanelTab::Structure => self.struct_tree.clone().into_any_element(),
                LeftPanelTab::Inspector => self.data_inspector.clone().into_any_element(),
                LeftPanelTab::Map => self.visual_map.clone().into_any_element(),
                LeftPanelTab::Checksum => self.checksum_panel.clone().into_any_element(),
                LeftPanelTab::Bookmarks => self.bookmark_panel.clone().into_any_element(),
            })
    }
}

impl Focusable for LeftPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.active_tab {
            LeftPanelTab::Files => self.file_tree.read(cx).focus_handle(cx),
            LeftPanelTab::Search => self.search_panel.read(cx).focus_handle(cx),
            LeftPanelTab::Strings => self.strings_panel.read(cx).focus_handle(cx),
            LeftPanelTab::Structure => self.struct_tree.read(cx).focus_handle(cx),
            LeftPanelTab::Inspector => self.data_inspector.read(cx).focus_handle(cx),
            LeftPanelTab::Map => self.visual_map.read(cx).focus_handle(cx),
            LeftPanelTab::Checksum => self.checksum_panel.read(cx).focus_handle(cx),
            LeftPanelTab::Bookmarks => self.bookmark_panel.read(cx).focus_handle(cx),
        }
    }
}
