use crate::core::appearance::Appearance;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::selection::Selection;
use crate::ui::icon::IconName;
use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, button::Button, button::ButtonVariants, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

pub const CONTEXT: &str = "DataInspector";
pub const EDIT_CONTEXT: &str = "InspectorEdit";

actions!(data_inspector, [CommitInspectorEdit, CancelInspectorEdit]);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", CommitInspectorEdit, Some(EDIT_CONTEXT)),
        KeyBinding::new("escape", CancelInspectorEdit, Some(EDIT_CONTEXT)),
    ]);
}

pub use crate::core::inspector::{InspectorField, format_hex_values};

pub struct DataInspector {
    pub editor: Option<Entity<Editor>>,
    pub focus_handle: FocusHandle,
    pub is_big_endian: bool,
    _editor_subscription: Option<Subscription>,
    pub editing_field: Option<InspectorField>,
    pub editing_offset: usize,
    pub original_selection: Option<Selection>,
    pub edit_input: Entity<InputState>,
    pub edit_error: Option<String>,
    _input_subscription: Option<Subscription>,
}

impl DataInspector {
    pub fn new(editor: Option<Entity<Editor>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let _editor_subscription = editor.as_ref().map(|ed| {
            cx.observe(ed, |this, ed, cx| {
                if this.editing_field.is_some() {
                    let ed_read = ed.read(cx);
                    if ed_read.cursor.offset != this.editing_offset || ed_read.is_read_only() {
                        this.editing_field = None;
                        this.original_selection = None;
                        this.edit_error = None;
                    }
                }
                cx.notify();
            })
        });

        let edit_input = cx.new(|cx| InputState::new(window, cx));
        let _input_subscription = cx.subscribe_in(&edit_input, window, |this, input, event: &input::InputEvent, window, cx| match event {
            input::InputEvent::PressEnter { .. } => {
                this.commit_edit(window, cx);
            }
            input::InputEvent::Change => {
                if let Some(field) = this.editing_field {
                    let text = input.read(cx).value().to_string();
                    match field.parse_and_serialize(&text, this.is_big_endian) {
                        Ok(_) => {
                            if this.edit_error.is_some() {
                                this.edit_error = None;
                                cx.notify();
                            }
                        }
                        Err(err) => {
                            if this.edit_error.as_deref() != Some(&err) {
                                this.edit_error = Some(err);
                                cx.notify();
                            }
                        }
                    }
                }
            }
            _ => {}
        });

        Self {
            editor,
            focus_handle,
            is_big_endian: false,
            _editor_subscription,
            editing_field: None,
            editing_offset: 0,
            original_selection: None,
            edit_input,
            edit_error: None,
            _input_subscription: Some(_input_subscription),
        }
    }

    pub fn set_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        if self.editing_field.is_some() {
            self.cancel_edit_internal(cx);
        }
        self._editor_subscription = None;
        self.editor = editor.clone();
        if let Some(ed) = &editor {
            self._editor_subscription = Some(cx.observe(ed, |this, ed, cx| {
                if this.editing_field.is_some() {
                    let ed_read = ed.read(cx);
                    if ed_read.cursor.offset != this.editing_offset || ed_read.is_read_only() {
                        this.editing_field = None;
                        this.original_selection = None;
                        this.edit_error = None;
                    }
                }
                cx.notify();
            }));
        }
        cx.notify();
    }

    /// Selects `byte_len` bytes starting from current cursor_offset in the active editor.
    pub fn select_bytes(&mut self, byte_len: usize, cx: &mut Context<Self>) {
        if self.editing_field.is_some() {
            self.cancel_edit_internal(cx);
        }
        let Some(ed) = &self.editor else { return };
        ed.update(cx, |editor, cx| {
            let cursor_offset = editor.cursor.offset;
            let total = editor.total_size();
            let end = (cursor_offset + byte_len).min(total);
            editor.set_selection(cursor_offset, end);
            cx.notify();
        });
    }

    pub fn start_editing(&mut self, field: InspectorField, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ed) = self.editor.clone() else { return };
        let (cursor_offset, is_read_only, bytes) = {
            let reader = ed.read(cx);
            if reader.is_read_only() {
                return;
            }
            let bytes = reader.read_bytes_at_cursor(field.byte_len());
            if bytes.len() < field.byte_len() {
                return;
            }
            (reader.cursor.offset, reader.is_read_only(), bytes)
        };

        if is_read_only {
            return;
        }

        if self.editing_field.is_some() {
            self.cancel_edit_internal(cx);
        }

        let orig_sel = ed.read(cx).selection();
        self.original_selection = Some(orig_sel);
        self.editing_field = Some(field);
        self.editing_offset = cursor_offset;
        self.edit_error = None;

        ed.update(cx, |editor, cx| {
            editor.set_selection(cursor_offset, cursor_offset + field.byte_len());
            cx.notify();
        });

        let initial_val = field.current_input_value(&bytes, self.is_big_endian);
        self.edit_input.update(cx, |input, cx| {
            input.set_value(initial_val, window, cx);
            input.focus(window, cx);
        });

        cx.notify();
    }

    pub fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.editing_field else { return };
        let Some(ed) = &self.editor else { return };

        if ed.read(cx).is_read_only() {
            self.cancel_edit(window, cx);
            return;
        }

        let input_text = self.edit_input.read(cx).value().to_string();
        let parse_result = field.parse_and_serialize(&input_text, self.is_big_endian);

        let replacement = match parse_result {
            Ok(bytes) => {
                assert_eq!(bytes.len(), field.byte_len(), "replacement length must match field byte length");
                bytes
            }
            Err(err) => {
                self.edit_error = Some(err);
                cx.notify();
                return;
            }
        };

        let cursor_offset = self.editing_offset;
        let byte_len = field.byte_len();
        let orig_sel = self.original_selection.take();

        let changed = ed.update(cx, |editor, cx| {
            if let Some(orig) = orig_sel {
                editor.set_selection(orig.anchor(), orig.active());
            }
            let success = editor.replace_range_with_cursor(cursor_offset..cursor_offset + byte_len, replacement, cursor_offset);
            if success {
                cx.notify();
            }
            success
        });

        if changed {
            let path = ed.read(cx).document.read().ok().map(|d| d.path().to_path_buf());
            if let Some(path) = path {
                let service = crate::app_state::AppState::global(cx).document_service.clone();
                service.notify_document_changed(&path, cx);
            }
        }

        self.editing_field = None;
        self.edit_error = None;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn cancel_edit_internal(&mut self, cx: &mut Context<Self>) {
        if let (Some(ed), Some(orig)) = (&self.editor, self.original_selection.take()) {
            ed.update(cx, |editor, cx| {
                editor.set_selection(orig.anchor(), orig.active());
                cx.notify();
            });
        }
        self.editing_field = None;
        self.original_selection = None;
        self.edit_error = None;
    }

    pub fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_edit_internal(cx);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    #[allow(clippy::too_many_arguments)]
    fn render_row(
        &self,
        field: Option<InspectorField>,
        fallback_byte_len: usize,
        label: &'static str,
        value: String,
        font_family: &str,
        view: &Entity<Self>,
        window: &mut Window,
        theme: &gpui_kit::component::Theme,
        is_read_only: bool,
    ) -> impl IntoElement {
        let label = field.map_or(label, |f| f.label());
        let byte_len = field.map_or(fallback_byte_len, |f| f.byte_len());
        let val_for_click = value.clone();
        let is_editing = field.is_some() && self.editing_field == field;
        let can_copy = value != "--" && value != "-" && !value.is_empty();

        let label_el = div()
            .flex_shrink_0()
            .w(px(100.0))
            .text_xs()
            .whitespace_nowrap()
            .text_color(theme.muted_foreground)
            .child(label);

        let value_color = if value == "--" { theme.muted_foreground } else { theme.foreground };

        let row_group = SharedString::from(format!("inspector-row-{}", label));

        let value_el = if is_editing {
            let has_error = self.edit_error.is_some();
            h_flex()
                .key_context(EDIT_CONTEXT)
                .on_action(window.listener_for(view, |this, _: &CommitInspectorEdit, window, cx| {
                    this.commit_edit(window, cx);
                }))
                .on_action(window.listener_for(view, |this, _: &CancelInspectorEdit, window, cx| {
                    this.cancel_edit(window, cx);
                }))
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .when(has_error, |el| el.border_1().border_color(red()).rounded_sm())
                        .child(Input::new(&self.edit_input).with_size(gpui_kit::component::Size::XSmall)),
                )
                .child(
                    Button::new("commit-inspector-edit")
                        .icon(IconName::Check)
                        .ghost()
                        .with_size(gpui_kit::component::Size::XSmall)
                        .tooltip("Save (Enter)")
                        .on_click(window.listener_for(view, |this, _, window, cx| {
                            this.commit_edit(window, cx);
                        })),
                )
                .child(
                    Button::new("cancel-inspector-edit")
                        .icon(IconName::Close)
                        .ghost()
                        .with_size(gpui_kit::component::Size::XSmall)
                        .tooltip("Cancel (Esc)")
                        .on_click(window.listener_for(view, |this, _, window, cx| {
                            this.cancel_edit(window, cx);
                        })),
                )
                .into_any_element()
        } else {
            let mut action_buttons = h_flex().items_center().gap_0p5().flex_shrink_0();

            if let Some(f) = field
                && can_copy
            {
                let mut edit_btn = Button::new(SharedString::from(format!("inspector-edit-{}", label)))
                    .ghost()
                    .icon(IconName::PenLine)
                    .with_size(gpui_kit::component::Size::XSmall);

                if is_read_only {
                    edit_btn = edit_btn.disabled(true).tooltip("Cannot edit in read-only mode");
                } else {
                    edit_btn = edit_btn.tooltip("Edit Value").on_click(window.listener_for(view, move |this, _, window, cx| {
                        this.start_editing(f, window, cx);
                    }));
                }
                action_buttons = action_buttons.child(edit_btn);
            }

            if can_copy {
                let copy_val = val_for_click.clone();
                action_buttons = action_buttons.child(
                    Button::new(SharedString::from(format!("inspector-copy-{}", label)))
                        .ghost()
                        .icon(IconName::Copy)
                        .with_size(gpui_kit::component::Size::XSmall)
                        .tooltip("Copy Value")
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy_val.clone()));
                        }),
                );
            }

            let normal_fade = theme.background;
            let hover_fade = theme.background.blend(theme.muted.opacity(0.4));

            let value_text_el = div()
                .relative()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .flex_1()
                        .text_right()
                        .font_family(font_family.to_string())
                        .text_xs()
                        .whitespace_nowrap()
                        .text_color(value_color)
                        .child(value),
                )
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left_0()
                        .w(px(24.0))
                        .bg(linear_gradient(
                            90.0,
                            linear_color_stop(normal_fade, 0.0),
                            linear_color_stop(normal_fade.opacity(0.0), 1.0),
                        ))
                        .group_hover(row_group.clone(), move |style| {
                            style.bg(linear_gradient(
                                90.0,
                                linear_color_stop(hover_fade, 0.0),
                                linear_color_stop(hover_fade.opacity(0.0), 1.0),
                            ))
                        }),
                );

            h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .justify_end()
                .gap_1()
                .child(value_text_el)
                .child(action_buttons)
                .into_any_element()
        };

        let mut row_content = h_flex()
            .id(label)
            .group(row_group)
            .w_full()
            .justify_between()
            .items_center()
            .gap_2()
            .py_1()
            .pl_3()
            .pr_1()
            .rounded_sm();

        if !is_editing {
            row_content = row_content.cursor_pointer().hover(|style| style.bg(theme.muted.opacity(0.4)));

            row_content = row_content.on_mouse_down(
                MouseButton::Left,
                window.listener_for(view, move |this, _, _window, cx| {
                    if byte_len > 0 && val_for_click != "--" && val_for_click != "-" {
                        this.select_bytes(byte_len, cx);
                    }
                }),
            );
        }

        let row_content = row_content.child(label_el).child(value_el);

        if is_editing && let Some(err) = &self.edit_error {
            v_flex()
                .w_full()
                .child(row_content)
                .child(div().px_3().pb_1().text_xs().text_color(red()).child(err.clone()))
                .into_any_element()
        } else {
            row_content.into_any_element()
        }
    }

    fn render_section_header(&self, label: &'static str, theme: &gpui_kit::component::Theme) -> impl IntoElement {
        crate::ui::panels::panel_section_header(label, theme)
    }

    fn format_unix_time(&self, timestamp: i64) -> String {
        if !(0..=253402300799).contains(&timestamp) {
            // up to year 9999
            return "Out of range".to_string();
        }

        let seconds = timestamp;
        let day_clock = seconds % 86400;
        let mut days_since_epoch = seconds / 86400;

        let hour = day_clock / 3600;
        let minute = (day_clock % 3600) / 60;
        let second = day_clock % 60;

        let mut year = 1970;
        loop {
            let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
            let year_days = if is_leap { 366 } else { 365 };
            if days_since_epoch < year_days {
                break;
            }
            days_since_epoch -= year_days;
            year += 1;
        }

        let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let month_days = if is_leap {
            [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        } else {
            [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        };

        let mut month = 1;
        for &days in &month_days {
            if days_since_epoch < days {
                break;
            }
            days_since_epoch -= days;
            month += 1;
        }

        let day = days_since_epoch + 1;
        format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", year, month, day, hour, minute, second)
    }
}

impl Render for DataInspector {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let active_editor = self.editor.as_ref();

        let bytes_at_cursor = if let Some(editor) = active_editor {
            editor.read(cx).read_bytes_at_cursor(8)
        } else {
            Vec::new()
        };

        let is_read_only = active_editor.is_none_or(|ed| ed.read(cx).is_read_only());
        let is_big_endian = self.is_big_endian;

        let (hex8_val, hex16_val, hex32_val, hex64_val) = format_hex_values(&bytes_at_cursor, is_big_endian);

        // Data conversion logic
        let mut i8_val = "--".to_string();
        let mut u8_val = "--".to_string();
        let mut i16_val = "--".to_string();
        let mut u16_val = "--".to_string();
        let mut i32_val = "--".to_string();
        let mut u32_val = "--".to_string();
        let mut i64_val = "--".to_string();
        let mut u64_val = "--".to_string();
        let mut f32_val = "--".to_string();
        let mut f64_val = "--".to_string();
        let mut unix_time_32 = "--".to_string();
        let mut unix_time_64 = "--".to_string();
        let mut ascii_val = "--".to_string();
        let mut utf8_val = "--".to_string();
        let mut utf16_val = "--".to_string();

        if !bytes_at_cursor.is_empty() {
            let b = bytes_at_cursor[0];
            i8_val = format!("{}", b as i8);
            u8_val = format!("{}", b);

            let ch = b as char;
            if ch.is_ascii_graphic() || ch == ' ' {
                ascii_val = format!("'{}'", ch);
            } else {
                ascii_val = ".".to_string();
            }
        }

        if bytes_at_cursor.len() >= 2 {
            let arr: [u8; 2] = bytes_at_cursor[0..2].try_into().expect("2-byte slice");
            let i16_val_raw = if is_big_endian { i16::from_be_bytes(arr) } else { i16::from_le_bytes(arr) };
            let u16_val_raw = if is_big_endian { u16::from_be_bytes(arr) } else { u16::from_le_bytes(arr) };
            i16_val = format!("{}", i16_val_raw);
            u16_val = format!("{}", u16_val_raw);

            let ch = u16_val_raw;
            if let Some(c) = char::from_u32(ch as u32) {
                if !c.is_control() {
                    utf16_val = format!("'{}'", c);
                } else {
                    utf16_val = ".".to_string();
                }
            } else {
                utf16_val = ".".to_string();
            }
        }

        if bytes_at_cursor.len() >= 4 {
            let arr: [u8; 4] = bytes_at_cursor[0..4].try_into().expect("4-byte slice");
            let i32_val_raw = if is_big_endian { i32::from_be_bytes(arr) } else { i32::from_le_bytes(arr) };
            let u32_val_raw = if is_big_endian { u32::from_be_bytes(arr) } else { u32::from_le_bytes(arr) };
            let f32_val_raw = if is_big_endian { f32::from_be_bytes(arr) } else { f32::from_le_bytes(arr) };
            i32_val = format!("{}", i32_val_raw);
            u32_val = format!("{}", u32_val_raw);
            f32_val = format!("{:.6}", f32_val_raw);
            unix_time_32 = self.format_unix_time(i32_val_raw as i64);
        }

        if bytes_at_cursor.len() >= 8 {
            let arr: [u8; 8] = bytes_at_cursor[0..8].try_into().expect("8-byte slice");
            let i64_val_raw = if is_big_endian { i64::from_be_bytes(arr) } else { i64::from_le_bytes(arr) };
            let u64_val_raw = if is_big_endian { u64::from_be_bytes(arr) } else { u64::from_le_bytes(arr) };
            let f64_val_raw = if is_big_endian { f64::from_be_bytes(arr) } else { f64::from_le_bytes(arr) };
            i64_val = format!("{}", i64_val_raw);
            u64_val = format!("{}", u64_val_raw);
            f64_val = format!("{:.6}", f64_val_raw);
            unix_time_64 = self.format_unix_time(i64_val_raw);
        }

        if !bytes_at_cursor.is_empty() {
            let first_byte = bytes_at_cursor[0];
            let expected_len = if first_byte & 0x80 == 0 {
                1
            } else if first_byte & 0xE0 == 0xC0 {
                2
            } else if first_byte & 0xF0 == 0xE0 {
                3
            } else if first_byte & 0xF8 == 0xF0 {
                4
            } else {
                0
            };

            let mut decoded = false;
            if expected_len > 0
                && expected_len <= bytes_at_cursor.len()
                && let Ok(s) = std::str::from_utf8(&bytes_at_cursor[0..expected_len])
                && let Some(c) = s.chars().next()
            {
                if !c.is_control() {
                    utf8_val = format!("'{}'", c);
                } else {
                    utf8_val = ".".to_string();
                }
                decoded = true;
            }
            if !decoded {
                utf8_val = ".".to_string();
            }
        }

        let (current_encoding, current_enc_val) = if let Some(ed) = &self.editor {
            let ed = ed.read(cx);
            let enc = ed.options.encoding;
            if !matches!(enc, Encoding::Ascii | Encoding::Utf8 | Encoding::Utf16Le | Encoding::Utf16Be) {
                let mut val = ".".to_string();
                if !bytes_at_cursor.is_empty()
                    && let Some((c, _)) = enc.decode_char_at(&bytes_at_cursor, 0)
                {
                    val = format!("'{}'", c);
                }
                (Some(enc.label()), Some(val))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        let is_focused = self.focus_handle.is_focused(window);

        let endian_controls = h_flex()
            .items_center()
            .gap_1()
            .child(
                Button::new("endian_little")
                    .label("LE")
                    .ghost()
                    .selected(!is_big_endian)
                    .with_size(gpui_kit::component::Size::XSmall)
                    .tooltip("Little Endian")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.is_big_endian {
                            if this.editing_field.is_some() {
                                this.cancel_edit_internal(cx);
                            }
                            this.is_big_endian = false;
                            cx.notify();
                        }
                    })),
            )
            .child(
                Button::new("endian_big")
                    .label("BE")
                    .ghost()
                    .selected(is_big_endian)
                    .with_size(gpui_kit::component::Size::XSmall)
                    .tooltip("Big Endian")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if !this.is_big_endian {
                            if this.editing_field.is_some() {
                                this.cancel_edit_internal(cx);
                            }
                            this.is_big_endian = true;
                            cx.notify();
                        }
                    })),
            );

        let view = cx.entity().clone();

        let font_family = cx.global::<Appearance>().font_family.clone();
        let header_title = if is_read_only { "DATA INSPECTOR (READ ONLY)" } else { "DATA INSPECTOR" };
        let header = crate::ui::panels::panel_header(header_title, is_focused, theme, None, Some(endian_controls.into_any_element()));
        let container = crate::ui::panels::panel_container(is_focused, theme);

        container
            .id("data-inspector")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if this.editing_field.is_none() {
                        this.focus_handle.focus(window, cx);
                    }
                }),
            )
            .child(header)
            .child(
                v_flex()
                    .size_full()
                    .min_w_0()
                    .overflow_y_scrollbar()
                    .py_2()
                    .pl_2()
                    .pr_1()
                    .child(self.render_section_header("HEXADECIMAL", theme))
                    .child(self.render_row(
                        Some(InspectorField::Hex8),
                        1,
                        "Hex (1 byte)",
                        hex8_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_row(
                        Some(InspectorField::Hex16),
                        2,
                        "Hex (2 bytes)",
                        hex16_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_row(
                        Some(InspectorField::Hex32),
                        4,
                        "Hex (4 bytes)",
                        hex32_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_row(
                        Some(InspectorField::Hex64),
                        8,
                        "Hex (8 bytes)",
                        hex64_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_section_header("INTEGERS", theme))
                    .child(self.render_row(Some(InspectorField::Int8), 1, "Int8", i8_val, &font_family, &view, window, theme, is_read_only))
                    .child(self.render_row(
                        Some(InspectorField::UInt8),
                        1,
                        "UInt8",
                        u8_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_row(
                        Some(InspectorField::Int16),
                        2,
                        "Int16",
                        i16_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_row(
                        Some(InspectorField::UInt16),
                        2,
                        "UInt16",
                        u16_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_row(
                        Some(InspectorField::Int32),
                        4,
                        "Int32",
                        i32_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_row(
                        Some(InspectorField::UInt32),
                        4,
                        "UInt32",
                        u32_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_row(
                        Some(InspectorField::Int64),
                        8,
                        "Int64",
                        i64_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_row(
                        Some(InspectorField::UInt64),
                        8,
                        "UInt64",
                        u64_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_section_header("FLOATS", theme))
                    .child(self.render_row(
                        Some(InspectorField::Float32),
                        4,
                        "Float32",
                        f32_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_row(
                        Some(InspectorField::Float64),
                        8,
                        "Float64",
                        f64_val,
                        &font_family,
                        &view,
                        window,
                        theme,
                        is_read_only,
                    ))
                    .child(self.render_section_header("TIME", theme))
                    .child(self.render_row(None, 4, "UnixTime32", unix_time_32, &font_family, &view, window, theme, is_read_only))
                    .child(self.render_row(None, 8, "UnixTime64", unix_time_64, &font_family, &view, window, theme, is_read_only))
                    .child(self.render_section_header("TEXT", theme))
                    .child(self.render_row(None, 1, "ASCII", ascii_val, &font_family, &view, window, theme, is_read_only))
                    .child(self.render_row(None, 1, "UTF-8", utf8_val, &font_family, &view, window, theme, is_read_only))
                    .child(self.render_row(None, 2, "UTF-16", utf16_val, &font_family, &view, window, theme, is_read_only))
                    .when_some(current_encoding.zip(current_enc_val), |parent, (enc_label, enc_val)| {
                        parent.child(self.render_row(None, 1, enc_label, enc_val, &font_family, &view, window, theme, is_read_only))
                    }),
            )
    }
}

impl Focusable for DataInspector {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::InspectorField;

    #[test]
    fn test_inspector_field_reexport() {
        assert_eq!(InspectorField::Hex8.byte_len(), 1);
        assert_eq!(InspectorField::ALL.len(), 14);
    }

    #[test]
    fn test_selection_range_clamping() {
        let cursor_offset = 10;
        let total = 15;
        let byte_len = 8;
        let end = (cursor_offset + byte_len).min(total);
        assert_eq!(end, 15);

        let byte_len = 2;
        let end = (cursor_offset + byte_len).min(total);
        assert_eq!(end, 12);
    }
}
