//! GPUI 0.2.2 EntityInputHandler integration, based on its Apache-2.0 input example.
//! Unlike the single-line example, all geometry here comes from wrapped text.
use gpui::{prelude::*, *};
use quire_core::core::{
    highlight::{self, Kind as HighlightKind},
    types::{Lang, Mark, MarkKind},
};
use quire_gpui::buffer::{Buffer, to_utf16};
use std::{cell::RefCell, ops::Range, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Enter,
        Escape,
        IndentList,
        OutdentList,
        ToggleBold,
        ToggleItalic,
        ToggleStrike,
        ToggleInlineCode,
        RemoveBlock
    ]
);

#[derive(Clone)]
pub enum InputEvent {
    Edited { text: String, boundary: bool },
    Split(usize),
    Merge,
    Navigate(i32),
    NavigateHorizontal(i32),
    Focused,
    Blurred,
    Indent,
    Outdent,
    ToggleMark(MarkKind),
    RemoveBlock,
    DismissSlash,
    ActivateLink(String),
}

pub struct TextInput {
    pub buffer: Buffer,
    pub marks: Vec<Mark>,
    pub placeholder: Option<String>,
    pub focus: FocusHandle,
    layout: Option<TextLayout>,
    bounds: Option<Bounds<Pixels>>,
    selecting: bool,
    pub font_size: f32,
    pub text_color: Option<Hsla>,
    pub code_lang: Lang,
    pub code_colors: [Hsla; 5],
}
impl EventEmitter<InputEvent> for TextInput {}
impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TextInput {
    pub fn new(
        text: String,
        marks: Vec<Mark>,
        font_size: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        cx.on_focus_out(&focus, window, |this, _, _, cx| {
            // Abandon uncommitted composition on blur; never save a pinyin preview.
            this.buffer.cancel();
            this.selecting = false;
            cx.emit(InputEvent::Blurred);
            cx.notify();
        })
        .detach();
        Self {
            buffer: Buffer::new(text),
            marks,
            placeholder: None,
            focus,
            layout: None,
            bounds: None,
            selecting: false,
            font_size,
            text_color: None,
            code_lang: Lang::Plain,
            code_colors: [
                rgb(0xad7fb8).into(),
                rgb(0x9b9a97).into(),
                rgb(0x6ba584).into(),
                rgb(0xe28d50).into(),
                rgb(0x5b9bc4).into(),
            ],
        }
    }
    pub fn reset(&mut self, text: String, marks: Vec<Mark>, caret: usize, cx: &mut Context<Self>) {
        self.buffer = Buffer::new(text);
        self.marks = marks;
        self.buffer.move_to(caret);
        self.layout = None;
        cx.notify();
    }
    fn changed(&mut self, boundary: bool, cx: &mut Context<Self>) {
        if !self.buffer.composing() {
            cx.emit(InputEvent::Edited {
                text: self.buffer.text.clone(),
                boundary,
            });
        }
        cx.notify();
    }
    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.composing() {
            return;
        }
        if self.buffer.selection.is_empty() && self.buffer.caret() == 0 {
            cx.emit(InputEvent::Merge);
            return;
        }
        if self.buffer.selection.is_empty() {
            self.buffer.select_to(self.buffer.previous());
        }
        self.replace_text_in_range(None, "", window, cx);
    }
    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.composing() {
            return;
        }
        if self.buffer.selection.is_empty() {
            self.buffer.select_to(self.buffer.next());
        }
        self.replace_text_in_range(None, "", window, cx);
    }
    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.composing() {
            return;
        }
        if self.buffer.selection.is_empty() && self.buffer.caret() == 0 {
            cx.emit(InputEvent::NavigateHorizontal(-1));
            return;
        }
        let p = if self.buffer.selection.is_empty() {
            self.buffer.previous()
        } else {
            self.buffer.selection.start
        };
        self.buffer.move_to(p);
        cx.notify();
    }
    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.composing() {
            return;
        }
        if self.buffer.selection.is_empty() && self.buffer.caret() == self.buffer.text.len() {
            cx.emit(InputEvent::NavigateHorizontal(1));
            return;
        }
        let p = if self.buffer.selection.is_empty() {
            self.buffer.next()
        } else {
            self.buffer.selection.end
        };
        self.buffer.move_to(p);
        cx.notify();
    }
    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.composing() {
            self.buffer.select_to(self.buffer.previous());
            cx.notify();
        }
    }
    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.composing() {
            self.buffer.select_to(self.buffer.next());
            cx.notify();
        }
    }
    fn vertical(&mut self, direction: i32, select: bool, cx: &mut Context<Self>) {
        if self.buffer.composing() {
            return;
        }
        if let Some(layout) = &self.layout {
            let pos = layout.position(self.buffer.caret());
            let target = point(pos.x, pos.y + layout.line_height * direction as f32);
            if !select && (target.y < px(0.) || target.y >= layout.height()) {
                cx.emit(InputEvent::Navigate(direction));
                return;
            }
            let byte = layout.index(target).min(self.buffer.text.len());
            if select {
                self.buffer.select_to(byte);
            } else {
                self.buffer.move_to(byte);
            }
            cx.notify();
        }
    }
    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(-1, false, cx);
    }
    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(1, false, cx);
    }
    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(-1, true, cx);
    }
    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.vertical(1, true, cx);
    }
    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.composing() {
            self.buffer.move_to(0);
            self.buffer.select_to(self.buffer.text.len());
            cx.notify();
        }
    }
    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.composing() {
            self.buffer.move_to(0);
            cx.notify();
        }
    }
    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.composing() {
            self.buffer.move_to(self.buffer.text.len());
            cx.notify();
        }
    }
    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.composing() {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.buffer
                .replace(None, &text.replace("\r\n", "\n").replace('\r', "\n"));
            self.changed(true, cx);
            let _ = window;
        }
    }
    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.selection.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.buffer.text[self.buffer.selection.clone()].into(),
            ));
        }
    }
    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.composing() {
            return;
        }
        self.copy(&Copy, window, cx);
        if !self.buffer.selection.is_empty() {
            self.buffer.replace(None, "");
            self.changed(true, cx);
        }
    }
    fn enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.composing() {
            return;
        }
        if !self.buffer.selection.is_empty() {
            self.buffer.replace(None, "");
            self.changed(true, cx);
        }
        cx.emit(InputEvent::Split(self.buffer.caret()));
    }
    fn escape(&mut self, _: &Escape, _: &mut Window, cx: &mut Context<Self>) {
        let composing = self.buffer.composing();
        self.buffer.cancel();
        if !composing {
            cx.emit(InputEvent::DismissSlash);
        }
        cx.notify();
    }
    fn indent_list(&mut self, _: &IndentList, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.composing() {
            cx.emit(InputEvent::Indent);
        }
    }
    fn outdent_list(&mut self, _: &OutdentList, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.composing() {
            cx.emit(InputEvent::Outdent);
        }
    }
    fn toggle_mark(&mut self, kind: MarkKind, cx: &mut Context<Self>) {
        if self.buffer.composing() || self.buffer.selection.is_empty() {
            return;
        }
        cx.emit(InputEvent::ToggleMark(kind));
    }
    fn toggle_bold(&mut self, _: &ToggleBold, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_mark(MarkKind::Bold, cx);
    }
    fn toggle_italic(&mut self, _: &ToggleItalic, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_mark(MarkKind::Italic, cx);
    }
    fn toggle_strike(&mut self, _: &ToggleStrike, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_mark(MarkKind::Strike, cx);
    }
    fn toggle_inline_code(&mut self, _: &ToggleInlineCode, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_mark(MarkKind::Code, cx);
    }
    fn remove_block(&mut self, _: &RemoveBlock, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buffer.composing() {
            cx.emit(InputEvent::RemoveBlock);
        }
    }
    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.buffer.cancel();
        window.focus(&self.focus);
        if let (Some(layout), Some(bounds)) = (&self.layout, self.bounds) {
            let byte = layout
                .index(event.position - bounds.origin)
                .min(self.buffer.text.len());
            if event.modifiers.control
                && let Some(link) = self.marks.iter().find(|mark| {
                    matches!(mark.kind, MarkKind::Link | MarkKind::Mention)
                        && mark.start <= byte
                        && byte < mark.end
                })
            {
                cx.emit(InputEvent::ActivateLink(link.url.clone()));
                cx.notify();
                return;
            }
            self.selecting = true;
            if event.modifiers.shift {
                self.buffer.select_to(byte);
            } else {
                self.buffer.move_to(byte);
            }
        }
        cx.emit(InputEvent::Focused);
        cx.notify();
    }
    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }
    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting
            && let (Some(layout), Some(bounds)) = (&self.layout, self.bounds)
        {
            let byte = layout
                .index(event.position - bounds.origin)
                .min(self.buffer.text.len());
            self.buffer.select_to(byte);
            cx.notify();
        }
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.buffer.utf8_range(&range);
        *actual = Some(self.buffer.utf16_range(&range));
        Some(self.buffer.text[range].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.buffer.utf16_range(&self.buffer.selection),
            reversed: self.buffer.reversed,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.buffer.marked().map(|r| self.buffer.utf16_range(&r))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.buffer.composing() {
            self.buffer.unmark();
            self.changed(true, cx);
        }
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let composition = self.buffer.composing();
        self.buffer.replace(range, text);
        self.changed(composition, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.mark(range, text, selected);
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let range = self.buffer.utf8_range(&range);
        let start = layout.position(range.start);
        let end = layout.position(range.end);
        // Use the actual text element origin, not the enclosing input's padded bounds.
        let origin = self.bounds?.origin + start;
        let width = if start.y == end.y {
            (end.x - start.x).max(px(1.))
        } else {
            px(1.)
        };
        Some(Bounds::new(origin, size(width, layout.line_height)))
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let index = self
            .layout
            .as_ref()?
            .index(point - self.bounds?.origin)
            .min(self.buffer.text.len());
        Some(to_utf16(&self.buffer.text, index))
    }
}

#[derive(Clone)]
struct TextLayout {
    // (absolute UTF-8 start, y, shaped physical paragraph)
    lines: Vec<(usize, Pixels, WrappedLine)>,
    line_height: Pixels,
    width: Pixels,
}
impl TextLayout {
    fn shape(
        text: SharedString,
        marks: &[Mark],
        marked: Option<Range<usize>>,
        width: Pixels,
        font_size: Pixels,
        dim: bool,
        text_color: Option<Hsla>,
        code_lang: Lang,
        code_colors: [Hsla; 5],
        window: &Window,
    ) -> Self {
        let style = window.text_style();
        let run = TextRun {
            len: text.len(),
            font: style.font(),
            color: if dim {
                text_color.unwrap_or(style.color).opacity(0.56)
            } else {
                text_color.unwrap_or(style.color)
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let syntax_ranges = if code_lang == Lang::Plain {
            Vec::new()
        } else {
            let layers = (1..=5)
                .map(|kind| {
                    highlight::layer(
                        text.as_ref(),
                        code_lang,
                        1.0e9,
                        1.0,
                        HighlightKind::from_int(kind),
                    )
                })
                .map(|layer| layer.chars().collect::<Vec<_>>())
                .collect::<Vec<_>>();
            let mut ranges = Vec::new();
            let mut byte = 0;
            let mut active: Option<(usize, usize)> = None;
            for (index, ch) in text.chars().enumerate() {
                let kind = if ch.is_whitespace() {
                    None
                } else {
                    layers.iter().position(|layer| {
                        layer
                            .get(index)
                            .is_some_and(|painted| *painted != '\u{00a0}')
                    })
                };
                if kind != active.map(|(_, active_kind)| active_kind) {
                    if let Some((start, active_kind)) = active.take() {
                        ranges.push((start, byte, active_kind));
                    }
                    active = kind.map(|kind| (byte, kind));
                }
                byte += ch.len_utf8();
            }
            if let Some((start, kind)) = active {
                ranges.push((start, byte, kind));
            }
            ranges
        };
        let mut boundaries = vec![0, text.len()];
        for mark in marks {
            if mark.start < mark.end && mark.end <= text.len() {
                boundaries.extend([mark.start, mark.end]);
            }
        }
        for (start, end, _) in &syntax_ranges {
            boundaries.extend([*start, *end]);
        }
        if let Some(mark) = &marked {
            boundaries.extend([mark.start, mark.end]);
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        let mut runs = boundaries
            .windows(2)
            .filter_map(|span| {
                let (start, end) = (span[0], span[1]);
                if start == end {
                    return None;
                }
                let mut styled = run.clone();
                styled.len = end - start;
                if let Some((_, _, kind)) = syntax_ranges
                    .iter()
                    .find(|(from, to, _)| *from <= start && *to >= end)
                {
                    styled.color = code_colors[*kind];
                }
                for mark in marks
                    .iter()
                    .filter(|mark| mark.start <= start && mark.end >= end)
                {
                    match mark.kind {
                        MarkKind::Bold => styled.font.weight = FontWeight::BOLD,
                        MarkKind::Italic => styled.font.style = FontStyle::Italic,
                        MarkKind::Strike => {
                            styled.strikethrough = Some(StrikethroughStyle {
                                thickness: px(1.),
                                color: Some(style.color),
                            });
                        }
                        MarkKind::Code => {
                            styled.font.family = "Cascadia Code".into();
                            styled.background_color = Some(rgba(0x88888824).into());
                        }
                        MarkKind::Link | MarkKind::Mention | MarkKind::Date => {
                            styled.underline = Some(UnderlineStyle {
                                thickness: px(1.),
                                color: Some(style.color),
                                wavy: false,
                            });
                        }
                        MarkKind::Math => styled.font.style = FontStyle::Italic,
                    }
                }
                if marked
                    .as_ref()
                    .is_some_and(|m| m.start <= start && m.end >= end)
                {
                    styled.underline = Some(UnderlineStyle {
                        thickness: px(1.),
                        color: Some(style.color),
                        wavy: true,
                    });
                }
                Some(styled)
            })
            .collect::<Vec<_>>();
        if runs.is_empty() {
            runs.push(run);
        }
        let shaped = window
            .text_system()
            .shape_text(text, font_size, &runs, Some(width.max(px(1.))), None)
            .expect("valid platform text layout");
        let line_height = font_size * 1.55;
        let mut byte = 0;
        let mut y = px(0.);
        let lines = shaped
            .into_iter()
            .map(|line| {
                let start = byte;
                let top = y;
                byte += line.len() + 1;
                y += line.size(line_height).height;
                (start, top, line)
            })
            .collect();
        Self {
            lines,
            line_height,
            width,
        }
    }
    fn height(&self) -> Pixels {
        self.lines.last().map_or(self.line_height, |(_, y, l)| {
            *y + l.size(self.line_height).height
        })
    }
    fn position(&self, byte: usize) -> Point<Pixels> {
        for (start, y, line) in &self.lines {
            if byte <= start + line.len() {
                let p = line
                    .position_for_index(byte.saturating_sub(*start), self.line_height)
                    .unwrap_or_default();
                return point(p.x, *y + p.y);
            }
        }
        point(px(0.), self.height() - self.line_height)
    }
    fn index(&self, position: Point<Pixels>) -> usize {
        for (start, y, line) in &self.lines {
            if position.y < *y + line.size(self.line_height).height {
                let local = point(position.x.max(px(0.)), (position.y - *y).max(px(0.)));
                return *start
                    + line
                        .closest_index_for_position(local, self.line_height)
                        .unwrap_or_else(|i| i);
            }
        }
        self.lines.last().map_or(0, |(s, _, l)| s + l.len())
    }
}

struct TextElement {
    input: Entity<TextInput>,
}
impl IntoElement for TextElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for TextElement {
    type RequestLayoutState = Rc<RefCell<Option<TextLayout>>>;
    type PrepaintState = TextLayout;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        let dim = input.buffer.text.is_empty() && input.placeholder.is_some();
        let text: SharedString = input
            .placeholder
            .as_ref()
            .filter(|_| dim)
            .cloned()
            .unwrap_or_else(|| input.buffer.text.clone())
            .into();
        let marks = input.marks.clone();
        let marked = input.buffer.marked();
        let font_size = px(input.font_size);
        let text_color = input.text_color;
        let code_lang = input.code_lang;
        let code_colors = input.code_colors;
        let state = Rc::new(RefCell::new(None));
        let measured = state.clone();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let id = window.request_measured_layout(style, move |known, available, window, _| {
            let width = known.width.unwrap_or_else(|| match available.width {
                AvailableSpace::Definite(w) => w,
                _ => px(760.),
            });
            let layout = TextLayout::shape(
                text.clone(),
                &marks,
                marked.clone(),
                width,
                font_size,
                dim,
                text_color,
                code_lang,
                code_colors,
                window,
            );
            let size = size(width, layout.height());
            *measured.borrow_mut() = Some(layout);
            size
        });
        (id, state)
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> TextLayout {
        let input = self.input.read(cx);
        state
            .borrow_mut()
            .take()
            .filter(|l| l.width == bounds.size.width)
            .unwrap_or_else(|| {
                let dim = input.buffer.text.is_empty() && input.placeholder.is_some();
                let text = input
                    .placeholder
                    .as_ref()
                    .filter(|_| dim)
                    .cloned()
                    .unwrap_or_else(|| input.buffer.text.clone());
                TextLayout::shape(
                    text.into(),
                    &input.marks,
                    input.buffer.marked(),
                    bounds.size.width,
                    px(input.font_size),
                    dim,
                    input.text_color,
                    input.code_lang,
                    input.code_colors,
                    window,
                )
            })
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        layout: &mut TextLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        let input = self.input.read(cx);
        let focus = input.focus.clone();
        let buffer = input.buffer.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if focus.is_focused(window) {
            // Per-grapheme quads follow the same shaping result as the glyphs.
            for (i, grapheme) in buffer.text.grapheme_indices(true) {
                if i >= buffer.selection.end || i + grapheme.len() <= buffer.selection.start {
                    continue;
                }
                let p = layout.position(i);
                let q = layout.position(i + grapheme.len());
                let width = if q.y == p.y {
                    (q.x - p.x).max(px(2.))
                } else {
                    (layout.width - p.x).max(px(2.))
                };
                window.paint_quad(fill(
                    Bounds::new(bounds.origin + p, size(width, layout.line_height)),
                    rgba(0x4c8bf550),
                ));
            }
        }
        for (_, y, line) in &layout.lines {
            if let Err(error) = line.paint(
                bounds.origin + point(px(0.), *y),
                layout.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            ) {
                eprintln!("text paint: {error}");
            }
        }
        if buffer.text.is_empty() && focus.is_focused(window) {
            // Placeholder uses ordinary GPUI text elsewhere; empty layout still has caret geometry.
        }
        if focus.is_focused(window) && buffer.selection.is_empty() {
            window.paint_quad(fill(
                Bounds::new(
                    bounds.origin + layout.position(buffer.caret()),
                    size(px(1.5), layout.line_height),
                ),
                rgb(0x4c8bf5),
            ));
        }
        self.input.update(cx, |input, _| {
            input.layout = Some(layout.clone());
            input.bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .min_h(px(self.font_size * 1.55))
            .key_context("QuireInput")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::escape))
            .on_action(cx.listener(Self::indent_list))
            .on_action(cx.listener(Self::outdent_list))
            .on_action(cx.listener(Self::toggle_bold))
            .on_action(cx.listener(Self::toggle_italic))
            .on_action(cx.listener(Self::toggle_strike))
            .on_action(cx.listener(Self::toggle_inline_code))
            .on_action(cx.listener(Self::remove_block))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .child(TextElement { input: cx.entity() })
    }
}

pub fn bind_keys(cx: &mut App) {
    let context = Some("QuireInput");
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("left", Left, context),
        KeyBinding::new("right", Right, context),
        KeyBinding::new("up", Up, context),
        KeyBinding::new("down", Down, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("shift-up", SelectUp, context),
        KeyBinding::new("shift-down", SelectDown, context),
        KeyBinding::new("ctrl-a", SelectAll, context),
        KeyBinding::new("ctrl-v", Paste, context),
        KeyBinding::new("ctrl-c", Copy, context),
        KeyBinding::new("ctrl-x", Cut, context),
        KeyBinding::new("home", Home, context),
        KeyBinding::new("end", End, context),
        KeyBinding::new("enter", Enter, context),
        KeyBinding::new("escape", Escape, context),
        KeyBinding::new("tab", IndentList, context),
        KeyBinding::new("shift-tab", OutdentList, context),
        KeyBinding::new("ctrl-b", ToggleBold, context),
        KeyBinding::new("ctrl-i", ToggleItalic, context),
        KeyBinding::new("ctrl-shift-x", ToggleStrike, context),
        KeyBinding::new("ctrl-e", ToggleInlineCode, context),
        KeyBinding::new("ctrl-shift-backspace", RemoveBlock, context),
    ]);
}
