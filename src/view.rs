use crate::input::{InputEvent, TextInput};
use gpui::{prelude::*, *};
use quire_core::core::{
    command::Command,
    types::{BlockId, BlockKind},
};
use quire_gpui::session::Session;
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

actions!(workspace, [Save, Undo, Redo]);

fn ordered_page_tree(
    pages: &[quire_core::core::types::Page],
    expand_all: bool,
) -> Vec<(quire_core::core::types::Page, usize)> {
    fn append(
        pages: &[quire_core::core::types::Page],
        parent: Option<quire_core::core::types::PageId>,
        depth: usize,
        expand_all: bool,
        visited: &mut HashSet<quire_core::core::types::PageId>,
        out: &mut Vec<(quire_core::core::types::Page, usize)>,
    ) {
        let mut children = pages
            .iter()
            .filter(|page| page.parent == parent && !page.template)
            .cloned()
            .collect::<Vec<_>>();
        children.sort_by_key(|page| page.order);
        for page in children {
            if !visited.insert(page.id) {
                continue;
            }
            let id = page.id;
            let expanded = page.expanded;
            out.push((page, depth));
            if expanded || expand_all {
                append(pages, Some(id), depth + 1, expand_all, visited, out);
            }
        }
    }
    let mut out = Vec::new();
    append(pages, None, 0, expand_all, &mut HashSet::new(), &mut out);
    out
}

fn slash_items() -> &'static [(&'static str, &'static str, BlockKind)] {
    &[
        ("文本", "text paragraph", BlockKind::Paragraph),
        ("标题 1", "h1 heading", BlockKind::Heading1),
        ("标题 2", "h2 heading", BlockKind::Heading2),
        ("标题 3", "h3 heading", BlockKind::Heading3),
        ("项目列表", "bullet list", BlockKind::Bullet),
        ("编号列表", "numbered list", BlockKind::Numbered),
        ("待办", "todo task", BlockKind::Todo),
        ("折叠块", "toggle", BlockKind::Toggle),
        ("引用", "quote", BlockKind::Quote),
        ("代码", "code", BlockKind::Code),
        ("提示块", "callout", BlockKind::Callout),
        ("分隔线", "divider", BlockKind::Divider),
        ("表格", "table grid", BlockKind::Table),
        ("分栏", "columns layout", BlockKind::Columns),
    ]
}

#[derive(Clone, serde::Deserialize)]
struct Palette {
    id: String,
    bg: String,
    grad2: String,
    fg: String,
    #[serde(rename = "bg-elev")]
    elevated: String,
    #[serde(rename = "fg-muted")]
    muted: String,
    border: String,
    accent: String,
}
impl Palette {
    fn load(id: &str) -> Self {
        let palettes: Vec<Self> = serde_json::from_str(include_str!("palettes.json"))
            .expect("checked-in design palettes");
        palettes
            .iter()
            .find(|p| p.id == id)
            .unwrap_or(&palettes[0])
            .clone()
    }
    fn color(value: &str) -> Hsla {
        rgb(u32::from_str_radix(value.trim_start_matches('#'), 16).expect("palette hex")).into()
    }
}

pub struct Workspace {
    session: Session,
    title_input: Entity<TextInput>,
    title_subscription: Option<Subscription>,
    search_input: Entity<TextInput>,
    search_subscription: Option<Subscription>,
    search_query: String,
    inputs: BTreeMap<u64, Entity<TextInput>>,
    subscriptions: Vec<Subscription>,
    palette: Palette,
    theme_picker_open: bool,
    icon_picker_open: bool,
    confirm_delete_page: Option<quire_core::core::types::PageId>,
    slash_menu: Option<BlockId>,
    status: String,
    error: bool,
    active: Option<BlockId>,
    save_task: Option<Task<()>>,
    flush_lock: Arc<Mutex<()>>,
    closing: bool,
    smoke: bool,
}

impl Workspace {
    pub fn new(
        session: Session,
        theme: &str,
        smoke: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let title_input =
            cx.new(|cx| TextInput::new(session.page.title.clone(), Vec::new(), 38., window, cx));
        let search_input = cx.new(|cx| TextInput::new(String::new(), Vec::new(), 12., window, cx));
        search_input.update(cx, |input, _| {
            input.placeholder = Some("搜索页面和正文".into());
        });
        let mut this = Self {
            session,
            title_input,
            title_subscription: None,
            search_input,
            search_subscription: None,
            search_query: String::new(),
            inputs: BTreeMap::new(),
            subscriptions: Vec::new(),
            palette: Palette::load(theme),
            theme_picker_open: false,
            icon_picker_open: false,
            confirm_delete_page: None,
            slash_menu: None,
            status: "已保存".into(),
            error: false,
            active: None,
            save_task: None,
            flush_lock: Arc::new(Mutex::new(())),
            closing: false,
            smoke,
        };
        let title_input = this.title_input.clone();
        this.title_subscription =
            Some(
                cx.subscribe_in(&title_input, window, |this, _, event, window, cx| {
                    if let InputEvent::Edited { text, boundary } = event {
                        if this.session.page.locked {
                            let title = this.session.page.title.clone();
                            this.title_input.update(cx, |input, cx| {
                                input.reset(title.clone(), Vec::new(), title.len(), cx)
                            });
                        } else {
                            this.session.set_page_title(text.clone());
                            this.schedule_save(*boundary, window, cx);
                        }
                    }
                }),
            );
        let search_input = this.search_input.clone();
        this.search_subscription =
            Some(
                cx.subscribe_in(&search_input, window, |this, _, event, _, cx| {
                    if let InputEvent::Edited { text, .. } = event {
                        this.search_query = text.clone();
                        cx.notify();
                    }
                }),
            );
        this.sync_inputs(window, cx);
        let persistence = this.session.persistence.clone();
        let lock = this.flush_lock.clone();
        cx.on_app_quit(move |_, cx| {
            let persistence = persistence.clone();
            let lock = lock.clone();
            cx.background_executor().spawn(async move {
                let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                if let Err(error) = persistence.force_flush() {
                    eprintln!("shutdown save failed: {error}");
                }
            })
        })
        .detach();
        if smoke {
            cx.spawn_in(window, async move |weak, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(1000))
                    .await;
                let _ = weak.update_in(cx, |this, window, cx| this.smoke_input(window, cx));
                cx.background_executor()
                    .timer(Duration::from_millis(800))
                    .await;
                let _ = weak.update_in(cx, |this, window, cx| {
                    // Read the resulting core state after the emitted input events have run.
                    let texts = this
                        .session
                        .blocks()
                        .iter()
                        .map(|b| b.text.as_str())
                        .collect::<Vec<_>>();
                    if texts != ["剥离心智模型 🦀", "重启后仍然存在"] {
                        panic!("UI input/core smoke mismatch: {texts:?}");
                    }
                    eprintln!(
                        "SMOKE: input callbacks -> core document OK; not OS IME verification"
                    );
                    this.request_close(window, cx);
                });
            })
            .detach();
        }
        this
    }
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.subscriptions.clear();
        let blocks = self.session.blocks().to_vec();
        self.inputs.retain(|id, _| {
            blocks
                .iter()
                .any(|b| b.id.0 == *id && self.session.editable(b.id))
        });
        for block in blocks {
            if !self.session.editable(block.id) {
                continue;
            }
            let font = match block.kind {
                BlockKind::Heading1 => 28.,
                BlockKind::Heading2 => 22.,
                BlockKind::Heading3 => 18.,
                _ if self.session.page.small_text => 13.,
                _ => 15.5,
            };
            let input = if let Some(input) = self.inputs.get(&block.id.0) {
                input.clone()
            } else {
                let text = block.text.clone();
                let marks = block.marks.clone();
                let input = cx.new(|cx| TextInput::new(text, marks, font, window, cx));
                self.inputs.insert(block.id.0, input.clone());
                input
            };
            input.update(cx, |input, cx| {
                input.font_size = font;
                if input.buffer.committed() != block.text {
                    input.reset(block.text, block.marks.clone(), input.buffer.caret(), cx);
                } else if input.marks != block.marks {
                    input.marks = block.marks.clone();
                    cx.notify();
                }
            });
            let id = block.id;
            self.subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this, _, event, window, cx| this.input_event(id, event, window, cx),
            ));
        }
        cx.notify();
    }
    fn input_event(
        &mut self,
        id: BlockId,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.closing {
            return;
        }
        match event {
            InputEvent::Edited { text, boundary } => {
                let shortcut = match text.as_str() {
                    "# " => Some(BlockKind::Heading1),
                    "## " => Some(BlockKind::Heading2),
                    "### " => Some(BlockKind::Heading3),
                    "- " | "* " => Some(BlockKind::Bullet),
                    "1. " => Some(BlockKind::Numbered),
                    "[] " => Some(BlockKind::Todo),
                    "> " => Some(BlockKind::Quote),
                    "``` " => Some(BlockKind::Code),
                    _ => None,
                };
                self.active = Some(id);
                if let Some(kind) = shortcut {
                    self.session.convert_prefix(id, String::new(), kind);
                    self.sync_inputs(window, cx);
                    self.restore_focus(window, cx);
                } else {
                    self.session.replace(id, text.clone(), *boundary);
                }
                self.slash_menu = (text.starts_with('/')
                    && !text[1..].chars().any(char::is_whitespace))
                .then_some(id);
                if let (Some(block), Some(input)) = (
                    self.session.document.block(id),
                    self.inputs.get(&id.0).cloned(),
                ) {
                    let marks = block.marks.clone();
                    input.update(cx, |input, cx| {
                        input.marks = marks;
                        cx.notify();
                    });
                }
                self.schedule_save(false, window, cx);
            }
            InputEvent::Split(caret) => {
                if self
                    .session
                    .document
                    .block(id)
                    .is_some_and(|block| block.kind == BlockKind::TableCell)
                {
                    if let Some(index) = self.session.document.index_of(self.session.page.id, id)
                        && let Some(next) =
                            self.session.blocks()[index + 1..].iter().find(|block| {
                                block.kind == BlockKind::TableCell
                                    && block.parent
                                        == self
                                            .session
                                            .document
                                            .block(id)
                                            .and_then(|block| block.parent)
                            })
                    {
                        self.focus(next.id, 0, window, cx);
                    }
                    return;
                }
                if self.slash_menu == Some(id) {
                    let query = self
                        .session
                        .document
                        .block(id)
                        .and_then(|block| block.text.strip_prefix('/'))
                        .unwrap_or("")
                        .to_lowercase();
                    if let Some((_, _, kind)) = slash_items().iter().find(|(label, keywords, _)| {
                        query.is_empty()
                            || label.to_lowercase().contains(&query)
                            || keywords.contains(&query)
                    }) {
                        self.select_slash_kind(id, *kind, window, cx);
                        return;
                    }
                }
                let index = self
                    .session
                    .document
                    .index_of(self.session.page.id, id)
                    .unwrap_or(0);
                self.session
                    .execute(Command::SplitBlock { id, caret: *caret });
                let target = self.session.blocks().get(index + 1).map(|b| b.id);
                self.sync_inputs(window, cx);
                if let Some(target) = target {
                    self.focus(target, 0, window, cx);
                }
                self.schedule_save(false, window, cx);
            }
            InputEvent::Merge => {
                let index = self
                    .session
                    .document
                    .index_of(self.session.page.id, id)
                    .unwrap_or(0);
                // Retain the last empty paragraph as a reachable editing surface.
                if index == 0 {
                    return;
                }
                let previous = self.session.blocks()[index - 1].clone();
                if !self.session.editable(previous.id) {
                    return;
                }
                self.session.execute(Command::MergeBackward { id });
                self.sync_inputs(window, cx);
                self.focus(previous.id, previous.text.len(), window, cx);
                self.schedule_save(false, window, cx);
            }
            InputEvent::Navigate(direction) => {
                if let Some(index) = self.session.document.index_of(self.session.page.id, id) {
                    let next = index as i64 + *direction as i64;
                    if next >= 0
                        && let Some(block) = self.session.blocks().get(next as usize)
                    {
                        let target = block.id;
                        let caret = if *direction < 0 { block.text.len() } else { 0 };
                        self.focus(target, caret, window, cx);
                    }
                }
            }
            InputEvent::Focused => {
                self.session.end_typing();
                self.active = Some(id);
                cx.notify();
            }
            InputEvent::Indent => {
                self.session.execute(Command::IndentList { id });
                self.sync_inputs(window, cx);
                self.schedule_save(false, window, cx);
            }
            InputEvent::Outdent => {
                self.session.execute(Command::OutdentList { id });
                self.sync_inputs(window, cx);
                self.schedule_save(false, window, cx);
            }
            InputEvent::ToggleMark(kind) => {
                let Some(input) = self.inputs.get(&id.0) else {
                    return;
                };
                let selection = input.read(cx).buffer.selection.clone();
                if selection.is_empty() {
                    return;
                }
                self.session.execute(Command::ToggleMark {
                    id,
                    start: selection.start,
                    end: selection.end,
                    kind: *kind,
                    url: String::new(),
                    date: None,
                });
                self.sync_inputs(window, cx);
                self.restore_focus(window, cx);
                self.schedule_save(false, window, cx);
            }
            InputEvent::RemoveBlock => self.delete_block(id, window, cx),
            InputEvent::DismissSlash => {
                if self.slash_menu.take().is_some() {
                    cx.notify();
                }
            }
        }
    }
    fn change_block_kind(&mut self, kind: BlockKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.composing(cx) || self.closing || self.session.page.locked {
            return;
        }
        let target = self
            .active
            .or_else(|| self.session.blocks().first().map(|b| b.id));
        if let Some(id) = target {
            self.session.execute(Command::SetBlockType { id, kind });
            self.sync_inputs(window, cx);
            self.active = Some(self.first_child_editor(id, kind).unwrap_or(id));
            self.restore_focus(window, cx);
            self.schedule_save(false, window, cx);
        }
    }
    fn add_table_row(
        &mut self,
        id: BlockId,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        self.session.execute(Command::TableAddRow { id, row });
        self.sync_inputs(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn add_table_column(
        &mut self,
        id: BlockId,
        col: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        self.session.execute(Command::TableAddColumn { id, col });
        self.sync_inputs(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn add_columns_column(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.page.locked {
            return;
        }
        self.session.execute(Command::ColumnsAddColumn { id });
        self.sync_inputs(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn remove_columns_column(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.page.locked {
            return;
        }
        self.session.execute(Command::ColumnsDeleteColumn { id });
        self.sync_inputs(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn add_column_block(
        &mut self,
        column_id: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        self.session
            .execute(Command::ColumnsAddBlock { id: column_id });
        let next = self
            .session
            .blocks()
            .iter()
            .filter(|block| block.parent == Some(column_id) && block.kind == BlockKind::Paragraph)
            .max_by_key(|block| block.order)
            .map(|block| block.id);
        self.sync_inputs(window, cx);
        if let Some(id) = next {
            self.active = Some(id);
            self.focus(id, 0, window, cx);
        }
        self.schedule_save(false, window, cx);
    }
    fn first_child_editor(&self, id: BlockId, kind: BlockKind) -> Option<BlockId> {
        match kind {
            BlockKind::Table => self
                .session
                .blocks()
                .iter()
                .find(|block| block.parent == Some(id) && block.kind == BlockKind::TableCell)
                .map(|block| block.id),
            BlockKind::Columns => {
                let column = self
                    .session
                    .blocks()
                    .iter()
                    .find(|block| block.parent == Some(id) && block.kind == BlockKind::Column)?
                    .id;
                self.session
                    .blocks()
                    .iter()
                    .find(|block| block.parent == Some(column) && self.session.editable(block.id))
                    .map(|block| block.id)
            }
            _ => None,
        }
    }
    fn select_slash_kind(
        &mut self,
        id: BlockId,
        kind: BlockKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.composing(cx) || self.session.page.locked {
            return;
        }
        self.slash_menu = None;
        self.session.convert_prefix(id, String::new(), kind);
        self.sync_inputs(window, cx);
        self.active = Some(self.first_child_editor(id, kind).unwrap_or(id));
        self.restore_focus(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn toggle_todo(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing || self.session.page.locked {
            return;
        }
        self.session.execute(Command::ToggleTodoChecked { id });
        self.schedule_save(false, window, cx);
    }
    fn toggle_block_folded(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(block) = self.session.document.block(id) else {
            return;
        };
        self.session.set_block_folded(id, !block.folded);
        self.schedule_save(false, window, cx);
    }
    fn delete_block(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing || self.session.page.locked {
            return;
        }
        let Some(index) = self.session.document.index_of(self.session.page.id, id) else {
            return;
        };
        if self.session.blocks().len() <= 1 {
            self.session.replace(id, String::new(), true);
            self.sync_inputs(window, cx);
            self.focus(id, 0, window, cx);
            self.schedule_save(false, window, cx);
            return;
        }
        self.session.execute(Command::DeleteBlock { id });
        self.sync_inputs(window, cx);
        let index = index.min(self.session.blocks().len() - 1);
        if let Some(target) = self.session.blocks().get(index).map(|block| block.id) {
            self.focus(target, 0, window, cx);
        }
        self.schedule_save(false, window, cx);
    }
    fn toggle_selected_mark(
        &mut self,
        kind: quire_core::core::types::MarkKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.composing(cx) || self.closing {
            return;
        }
        let Some(id) = self.active else {
            return;
        };
        let Some(input) = self.inputs.get(&id.0) else {
            return;
        };
        let selection = input.read(cx).buffer.selection.clone();
        if selection.is_empty() {
            return;
        }
        self.session.execute(Command::ToggleMark {
            id,
            start: selection.start,
            end: selection.end,
            kind,
            url: String::new(),
            date: None,
        });
        self.sync_inputs(window, cx);
        self.restore_focus(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn select_theme(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = Palette::load(id);
        self.theme_picker_open = false;
        self.session
            .set_setting("gpui.theme", self.palette.id.clone());
        self.schedule_save(true, window, cx);
    }
    fn toggle_theme_picker(&mut self, cx: &mut Context<Self>) {
        self.theme_picker_open = !self.theme_picker_open;
        self.icon_picker_open = false;
        cx.notify();
    }
    fn toggle_icon_picker(&mut self, cx: &mut Context<Self>) {
        self.icon_picker_open = !self.icon_picker_open;
        self.theme_picker_open = false;
        cx.notify();
    }
    fn select_page_icon(&mut self, icon: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.composing(cx) || self.session.page.icon == icon {
            self.icon_picker_open = false;
            cx.notify();
            return;
        }
        let id = self.session.page.id;
        self.session.page.icon = icon.to_owned();
        if let Some(page) = self.session.pages.iter_mut().find(|page| page.id == id) {
            page.icon = icon.to_owned();
        }
        self.session
            .persistence
            .record(vec![quire_core::core::persistence::Change::PageIconSet {
                id,
                icon: icon.to_owned(),
            }]);
        self.icon_picker_open = false;
        self.schedule_save(false, window, cx);
    }
    fn toggle_full_width(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let full_width = !self.session.page.full_width;
        let small_text = self.session.page.small_text;
        self.session.set_page_layout(full_width, small_text);
        self.schedule_save(false, window, cx);
    }
    fn toggle_small_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let full_width = self.session.page.full_width;
        let small_text = !self.session.page.small_text;
        self.session.set_page_layout(full_width, small_text);
        self.sync_inputs(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn cycle_page_font(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use quire_core::core::types::PageFont;
        let next = match self.session.page.font {
            PageFont::Default => PageFont::Serif,
            PageFont::Serif => PageFont::Mono,
            PageFont::Mono => PageFont::Default,
        };
        self.session.set_page_font(next);
        self.schedule_save(false, window, cx);
    }
    fn toggle_page_lock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composing(cx) {
            return;
        }
        self.session.set_page_locked(!self.session.page.locked);
        self.session.end_typing();
        self.sync_inputs(window, cx);
        self.schedule_save(true, window, cx);
    }
    fn activate_page(
        &mut self,
        id: quire_core::core::types::PageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if id == self.session.page.id || self.closing || self.composing(cx) {
            return;
        }
        if self.session.select_page(id) {
            self.session.visit_page(id);
            self.active = None;
            let title = self.session.page.title.clone();
            self.title_input.update(cx, |input, cx| {
                let len = title.len();
                input.reset(title, Vec::new(), len, cx);
            });
            self.sync_inputs(window, cx);
            self.focus_first(window, cx);
            self.schedule_save(false, window, cx);
        }
    }
    fn toggle_page_expanded(
        &mut self,
        id: quire_core::core::types::PageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = self.session.pages.iter_mut().find(|page| page.id == id) else {
            return;
        };
        page.expanded = !page.expanded;
        let expanded = page.expanded;
        if self.session.page.id == id {
            self.session.page.expanded = expanded;
        }
        self.session.persistence.record(vec![
            quire_core::core::persistence::Change::PageExpandedSet { id, expanded },
        ]);
        self.schedule_save(false, window, cx);
    }
    fn toggle_favorite(
        &mut self,
        id: quire_core::core::types::PageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = self.session.pages.iter_mut().find(|page| page.id == id) else {
            return;
        };
        page.favorite = !page.favorite;
        let favorite = page.favorite;
        if self.session.page.id == id {
            self.session.page.favorite = favorite;
        }
        self.session.persistence.record(vec![
            quire_core::core::persistence::Change::PageFavoriteSet { id, favorite },
        ]);
        self.schedule_save(false, window, cx);
    }
    fn move_page(
        &mut self,
        id: quire_core::core::types::PageId,
        delta: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.move_page(id, delta) {
            cx.notify();
            self.schedule_save(false, window, cx);
        }
    }
    fn indent_page(
        &mut self,
        id: quire_core::core::types::PageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.indent_page(id) {
            cx.notify();
            self.schedule_save(false, window, cx);
        }
    }
    fn outdent_page(
        &mut self,
        id: quire_core::core::types::PageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.outdent_page(id) {
            cx.notify();
            self.schedule_save(false, window, cx);
        }
    }
    fn request_delete_page(&mut self, id: quire_core::core::types::PageId, cx: &mut Context<Self>) {
        self.confirm_delete_page = Some(id);
        cx.notify();
    }
    fn cancel_delete_page(&mut self, cx: &mut Context<Self>) {
        self.confirm_delete_page = None;
        cx.notify();
    }
    fn delete_page(
        &mut self,
        id: quire_core::core::types::PageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.composing(cx) {
            return;
        }
        if self.session.delete_page(id) {
            self.confirm_delete_page = None;
            self.active = None;
            let title = self.session.page.title.clone();
            self.title_input.update(cx, |input, cx| {
                let len = title.len();
                input.reset(title, Vec::new(), len, cx);
            });
            self.sync_inputs(window, cx);
            self.focus_first(window, cx);
            self.schedule_save(true, window, cx);
        }
    }
    fn add_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing || self.session.page.locked {
            return;
        }
        self.session.create_page(None);
        let id = self.session.page.id;
        self.session.visit_page(id);
        self.active = None;
        self.sync_inputs(window, cx);
        self.focus_first(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn add_child_page(
        &mut self,
        parent: quire_core::core::types::PageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.closing || self.session.page.locked {
            return;
        }
        self.session.create_page(Some(parent));
        let id = self.session.page.id;
        self.session.visit_page(id);
        self.active = None;
        self.sync_inputs(window, cx);
        self.focus_first(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn focus(&mut self, id: BlockId, caret: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.inputs.get(&id.0) {
            input.update(cx, |input, cx| {
                input.buffer.move_to(caret);
                window.focus(&input.focus);
                cx.notify();
            });
            self.active = Some(id);
        }
    }
    pub fn focus_first(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self
            .session
            .blocks()
            .iter()
            .find(|b| self.inputs.contains_key(&b.id.0))
            .map(|b| b.id)
        {
            self.focus(id, 0, window, cx);
        }
    }
    fn composing(&self, cx: &App) -> bool {
        self.title_input.read(cx).buffer.composing()
            || self.search_input.read(cx).buffer.composing()
            || self.inputs.values().any(|i| i.read(cx).buffer.composing())
    }
    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        self.session.end_typing();
        self.schedule_save(true, window, cx);
    }
    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if self.composing(cx) {
            return;
        }
        self.session.undo();
        self.sync_inputs(window, cx);
        self.restore_focus(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        if self.composing(cx) {
            return;
        }
        self.session.redo();
        self.sync_inputs(window, cx);
        self.restore_focus(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn restore_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.active.filter(|id| self.inputs.contains_key(&id.0)) {
            let caret = self.inputs[&id.0].read(cx).buffer.caret();
            self.focus(id, caret, window, cx);
        } else {
            self.focus_first(window, cx);
        }
    }
    fn schedule_save(&mut self, immediate: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.status = if immediate {
            "正在保存…"
        } else {
            "未保存"
        }
        .into();
        self.error = false;
        cx.notify();
        let persistence = self.session.persistence.clone();
        let lock = self.flush_lock.clone();
        let delay = if immediate {
            Duration::ZERO
        } else {
            Duration::from_millis(350)
        };
        self.save_task = Some(cx.spawn_in(window, async move |weak, cx| {
            cx.background_executor().timer(delay).await;
            let result = cx
                .background_executor()
                .spawn(async move {
                    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                    persistence.force_flush()
                })
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                match result {
                    Ok(()) => {
                        this.status = "已保存".into();
                        this.error = false;
                        if this.closing {
                            eprintln!("saved {}", this.session.path.display());
                            window.remove_window();
                            return;
                        }
                    }
                    Err(error) => {
                        this.status = format!("保存失败：{error}；请 Ctrl+S 重试");
                        this.error = true;
                        this.closing = false;
                    }
                }
                cx.notify();
            });
        }));
    }
    pub fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        for input in self.inputs.values() {
            input.update(cx, |i, cx| {
                i.buffer.cancel();
                cx.notify();
            });
        }
        self.session.end_typing();
        self.closing = true;
        self.schedule_save(true, window, cx);
    }
    fn smoke_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = self
            .inputs
            .values()
            .next()
            .expect("empty seed input")
            .clone();
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "boli", Some(4..4), window, cx);
            assert_eq!(input.buffer.committed(), "");
            input.replace_text_in_range(None, "剥离心智模型 🦀", window, cx);
        });
        // Events are applied after this update returns, so subsequent edits run on later ticks.
        cx.spawn_in(window, async move |weak, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                let id = this.session.blocks()[0].id;
                this.input_event(id, &InputEvent::Split("剥离心智模型 🦀".len()), window, cx);
                let second = this.session.blocks()[1].id;
                this.inputs[&second.0].update(cx, |input, cx| {
                    input.replace_text_in_range(None, "重启后仍然存在", window, cx);
                });
            });
        })
        .detach();
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette.clone();
        let text = Palette::color(&p.fg);
        let muted = Palette::color(&p.muted);
        let accent = Palette::color(&p.accent);
        let theme_options = if self.theme_picker_open {
            let palettes: Vec<Palette> =
                serde_json::from_str(include_str!("palettes.json")).expect("checked-in palettes");
            palettes
                .into_iter()
                .map(|palette| {
                    let id = palette.id.clone();
                    let label = id.clone();
                    let selected = id == self.palette.id;
                    div()
                        .id(SharedString::from(format!("theme-option-{id}")))
                        .w_full()
                        .h(px(28.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(px(5.))
                        .cursor_pointer()
                        .when(selected, |row| row.bg(Palette::color(&p.bg).opacity(0.35)))
                        .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.5)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_theme(&id, window, cx)
                        }))
                        .child(
                            div().size(px(16.)).rounded(px(4.)).bg(linear_gradient(
                                135.,
                                linear_color_stop(Palette::color(&palette.bg), 0.),
                                linear_color_stop(Palette::color(&palette.grad2), 1.),
                            )
                            .color_space(ColorSpace::Srgb)),
                        )
                        .child(format!("{}{}", label, if selected { "  ✓" } else { "" }))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let icon_options = if self.icon_picker_open {
            [
                "", "📄", "📝", "📚", "💡", "🎯", "✅", "🧠", "🌱", "🔥", "📌", "⭐", "🚀", "🗂️",
                "🏠",
            ]
            .into_iter()
            .map(|icon| {
                let selected = self.session.page.icon == icon;
                let label = if icon.is_empty() {
                    "清除图标".to_owned()
                } else {
                    format!("{icon}  {icon}")
                };
                div()
                    .id(SharedString::from(format!("page-icon-{icon}")))
                    .w_full()
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .cursor_pointer()
                    .when(selected, |row| row.bg(Palette::color(&p.bg).opacity(0.35)))
                    .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.5)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_page_icon(icon, window, cx)
                    }))
                    .child(label)
            })
            .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let query = self.search_query.trim().to_lowercase();
        let page_rows = ordered_page_tree(&self.session.pages, !query.is_empty())
            .into_iter()
            .filter(|(page, _)| {
                query.is_empty()
                    || page.title.to_lowercase().contains(&query)
                    || self
                        .session
                        .document
                        .page_blocks(page.id)
                        .iter()
                        .any(|block| block.text.to_lowercase().contains(&query))
            })
            .map(|(page, depth)| {
                let selected = page.id == self.session.page.id;
                let page_id = page.id;
                let has_children = self
                    .session
                    .pages
                    .iter()
                    .any(|child| child.parent == Some(page_id));
                let disclosure = if has_children {
                    if page.expanded { "▾" } else { "▸" }
                } else {
                    " "
                };
                div()
                    .id(SharedString::from(format!("page-{}", page.id.0)))
                    .h(px(30.))
                    .pl(px(8. + depth as f32 * 12.))
                    .pr(px(8.))
                    .flex()
                    .items_center()
                    .rounded(px(5.))
                    .when(selected, |row| row.bg(rgba(0xffffff14)).text_color(accent))
                    .when(!selected, |row| row.text_color(muted))
                    .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.24)))
                    .child(
                        div()
                            .id(SharedString::from(format!("expand-page-{}", page_id.0)))
                            .w(px(14.))
                            .flex_shrink_0()
                            .when(has_children, |button| {
                                button.cursor_pointer().on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.toggle_page_expanded(page_id, window, cx)
                                    },
                                ))
                            })
                            .child(disclosure),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("add-child-page-{}", page_id.0)))
                            .px(px(3.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_child_page(page_id, window, cx)
                            }))
                            .child("＋"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("select-page-{}", page_id.0)))
                            .flex_1()
                            .min_w_0()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.activate_page(page_id, window, cx)
                            }))
                            .child(format!(
                                "{}  {}",
                                if page.icon.is_empty() {
                                    "▤"
                                } else {
                                    &page.icon
                                },
                                page.title
                            )),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("move-page-up-{}", page_id.0)))
                            .px(px(2.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.move_page(page_id, -1, window, cx)
                            }))
                            .child("↑"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("move-page-down-{}", page_id.0)))
                            .px(px(2.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.move_page(page_id, 1, window, cx)
                            }))
                            .child("↓"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("indent-page-{}", page_id.0)))
                            .px(px(2.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.indent_page(page_id, window, cx)
                            }))
                            .child("↳"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("outdent-page-{}", page_id.0)))
                            .px(px(2.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.outdent_page(page_id, window, cx)
                            }))
                            .child("↰"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("favorite-page-{}", page_id.0)))
                            .px(px(3.))
                            .cursor_pointer()
                            .text_color(if page.favorite { accent } else { muted })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.toggle_favorite(page_id, window, cx)
                            }))
                            .child(if page.favorite { "★" } else { "☆" }),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("delete-page-{}", page_id.0)))
                            .px(px(3.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(rgb(0xe06c75)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.request_delete_page(page_id, cx)
                            }))
                            .child("×"),
                    )
            })
            .collect::<Vec<_>>();
        let favorite_rows = self
            .session
            .pages
            .iter()
            .filter(|page| {
                page.favorite
                    && !page.template
                    && (query.is_empty()
                        || page.title.to_lowercase().contains(&query)
                        || self
                            .session
                            .document
                            .page_blocks(page.id)
                            .iter()
                            .any(|block| block.text.to_lowercase().contains(&query)))
            })
            .map(|page| {
                let page_id = page.id;
                let selected = page_id == self.session.page.id;
                div()
                    .id(SharedString::from(format!("favorite-nav-{}", page_id.0)))
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .when(selected, |row| row.bg(rgba(0xffffff14)).text_color(accent))
                    .when(!selected, |row| row.text_color(muted))
                    .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.24)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_page(page_id, window, cx)
                    }))
                    .child("★")
                    .child(page.title.clone())
            })
            .collect::<Vec<_>>();
        let recent_rows = self
            .session
            .recent_pages()
            .into_iter()
            .filter(|id| *id != self.session.page.id)
            .filter_map(|id| {
                self.session
                    .pages
                    .iter()
                    .find(|page| page.id == id)
                    .map(|page| (id, page.title.clone()))
            })
            .filter(|(_, title)| query.is_empty() || title.to_lowercase().contains(&query))
            .map(|(page_id, title)| {
                div()
                    .id(SharedString::from(format!("recent-nav-{}", page_id.0)))
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .text_color(muted)
                    .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.24)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_page(page_id, window, cx)
                    }))
                    .child("◷")
                    .child(title)
            })
            .collect::<Vec<_>>();
        let delete_confirmation =
            self.confirm_delete_page.and_then(|id| {
                self.session
                    .pages
                    .iter()
                    .find(|page| page.id == id)
                    .map(|page| {
                        let title = page.title.clone();
                        div()
                            .w_full()
                            .px(px(8.))
                            .py(px(8.))
                            .rounded(px(6.))
                            .bg(Palette::color(&p.bg).opacity(0.55))
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .child(format!("删除「{title}」及其所有子页面？"))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .id("confirm-delete-page")
                                            .cursor_pointer()
                                            .text_color(rgb(0xe06c75))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.delete_page(id, window, cx)
                                            }))
                                            .child("删除页面树"),
                                    )
                                    .child(
                                        div()
                                            .id("cancel-delete-page")
                                            .cursor_pointer()
                                            .text_color(muted)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cancel_delete_page(cx)
                                            }))
                                            .child("取消"),
                                    ),
                            )
                    })
            });
        let kind_buttons =
            [
                ("段落", BlockKind::Paragraph),
                ("H1", BlockKind::Heading1),
                ("H2", BlockKind::Heading2),
                ("H3", BlockKind::Heading3),
                ("• 列表", BlockKind::Bullet),
                ("1. 列表", BlockKind::Numbered),
                ("☐ 待办", BlockKind::Todo),
                ("折叠块", BlockKind::Toggle),
                ("引用", BlockKind::Quote),
                ("代码", BlockKind::Code),
                ("提示", BlockKind::Callout),
                ("分隔线", BlockKind::Divider),
                ("表格", BlockKind::Table),
                ("分栏", BlockKind::Columns),
            ]
            .into_iter()
            .map(|(label, kind)| {
                div()
                    .id(SharedString::from(format!("kind-{kind:?}")))
                    .px(px(9.))
                    .py(px(5.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .text_color(muted)
                    .hover(|s| s.bg(Palette::color(&p.elevated)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.change_block_kind(kind, window, cx)
                    }))
                    .child(label)
            })
            .collect::<Vec<_>>();
        let mark_buttons = [
            ("B", quire_core::core::types::MarkKind::Bold),
            ("I", quire_core::core::types::MarkKind::Italic),
            ("S", quire_core::core::types::MarkKind::Strike),
            ("</>", quire_core::core::types::MarkKind::Code),
        ]
        .into_iter()
        .map(|(label, kind)| {
            div()
                .id(SharedString::from(format!("mark-{kind:?}")))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(5.))
                .cursor_pointer()
                .text_color(Palette::color(&p.accent))
                .hover(|s| s.bg(Palette::color(&p.elevated)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.toggle_selected_mark(kind, window, cx)
                }))
                .child(label)
        })
        .collect::<Vec<_>>();
        let rows = self
            .session
            .blocks()
            .iter()
            .filter(|block| {
                if matches!(block.kind, BlockKind::TableCell | BlockKind::Column) {
                    return false;
                }
                let mut parent = block.parent;
                while let Some(parent_id) = parent {
                    let Some(parent_block) = self
                        .session
                        .blocks()
                        .iter()
                        .find(|candidate| candidate.id == parent_id)
                    else {
                        break;
                    };
                    if matches!(parent_block.kind, BlockKind::Column | BlockKind::TableCell) {
                        return false;
                    }
                    if parent_block.folded {
                        return false;
                    }
                    parent = parent_block.parent;
                }
                true
            })
            .enumerate()
            .map(|(index, block)| {
                let block_index = self
                    .session
                    .blocks()
                    .iter()
                    .position(|candidate| candidate.id == block.id)
                    .unwrap_or(index);
                let prefix = match block.kind {
                    BlockKind::Bullet => "•".to_owned(),
                    BlockKind::Numbered => {
                        let count = self.session.blocks()[..block_index]
                            .iter()
                            .rev()
                            .take_while(|previous| {
                                previous.kind == BlockKind::Numbered
                                    && previous.parent == block.parent
                            })
                            .count()
                            + 1;
                        format!("{count}.")
                    }
                    BlockKind::Todo => {
                        if block.checked {
                            "☑".to_owned()
                        } else {
                            "☐".to_owned()
                        }
                    }
                    BlockKind::Toggle => if block.folded { "▸" } else { "▾" }.to_owned(),
                    _ => String::new(),
                };
                let input = self.inputs.get(&block.id.0).cloned();
                let block_id = block.id;
                let is_todo = block.kind == BlockKind::Todo;
                let is_toggle = block.kind == BlockKind::Toggle;
                let kind = block.kind;
                let slash_query = block.text.strip_prefix('/').unwrap_or("").to_lowercase();
                let slash_options = if self.slash_menu == Some(block_id) {
                    slash_items()
                        .iter()
                        .filter(|(label, keywords, _)| {
                            slash_query.is_empty()
                                || label.to_lowercase().contains(&slash_query)
                                || keywords.contains(slash_query.as_str())
                        })
                        .map(|(label, keywords, item_kind)| {
                            let item_kind = *item_kind;
                            div()
                                .id(SharedString::from(format!(
                                    "slash-option-{}-{item_kind:?}",
                                    block_id.0
                                )))
                                .w_full()
                                .h(px(28.))
                                .px(px(9.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .rounded(px(4.))
                                .cursor_pointer()
                                .hover(|option| option.bg(Palette::color(&p.elevated)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_slash_kind(block_id, item_kind, window, cx)
                                }))
                                .child(*label)
                                .child(div().text_size(px(10.)).text_color(muted).child(*keywords))
                        })
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let table_cells = if kind == BlockKind::Table {
                    self.session
                        .blocks()
                        .iter()
                        .filter(|cell| {
                            cell.parent == Some(block_id) && cell.kind == BlockKind::TableCell
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let table_columns = usize::from(block.columns.max(1));
                let table_row_count = table_cells.len().div_ceil(table_columns);
                let table_rows = table_cells
                    .chunks(table_columns)
                    .map(|cells| {
                        div().w_full().flex().children(cells.iter().map(|cell| {
                            let cell_id = cell.id;
                            let input = self.inputs.get(&cell_id.0).cloned();
                            div()
                                .flex_1()
                                .min_w_0()
                                .min_h(px(34.))
                                .border_1()
                                .border_color(Palette::color(&p.border))
                                .px(px(6.))
                                .py(px(5.))
                                .child(
                                    div()
                                        .w_full()
                                        .min_h(px(22.))
                                        .when_some(
                                            input.filter(|_| !self.closing),
                                            |view, input| view.child(input),
                                        )
                                        .when(
                                            self.closing || !self.inputs.contains_key(&cell_id.0),
                                            |view| view.child(cell.text.clone()),
                                        ),
                                )
                        }))
                    })
                    .collect::<Vec<_>>();
                let table_actions = if kind == BlockKind::Table {
                    vec![
                        div()
                            .id(SharedString::from(format!("table-add-row-{}", block_id.0)))
                            .px(px(7.))
                            .py(px(3.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_table_row(block_id, table_row_count, window, cx)
                            }))
                            .child("＋ 行"),
                        div()
                            .id(SharedString::from(format!(
                                "table-add-column-{}",
                                block_id.0
                            )))
                            .px(px(7.))
                            .py(px(3.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_table_column(block_id, table_columns, window, cx)
                            }))
                            .child("＋ 列"),
                    ]
                } else {
                    Vec::new()
                };
                let columns = if kind == BlockKind::Columns {
                    self.session
                        .blocks()
                        .iter()
                        .filter(|column| {
                            column.parent == Some(block_id) && column.kind == BlockKind::Column
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let column_views = columns
                    .iter()
                    .map(|column| {
                        let column_id = column.id;
                        let lines = self
                            .session
                            .blocks()
                            .iter()
                            .filter(|line| line.parent == Some(column_id))
                            .map(|line| {
                                let line_id = line.id;
                                let input = self.inputs.get(&line_id.0).cloned();
                                div()
                                    .w_full()
                                    .min_h(px(30.))
                                    .py(px(4.))
                                    .when_some(input.filter(|_| !self.closing), |row, input| {
                                        row.child(input)
                                    })
                                    .when(
                                        self.closing || !self.inputs.contains_key(&line_id.0),
                                        |row| row.child(line.text.clone()),
                                    )
                            })
                            .collect::<Vec<_>>();
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h(px(80.))
                            .px(px(9.))
                            .py(px(7.))
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .border_1()
                            .border_color(Palette::color(&p.border))
                            .children(lines)
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "columns-add-block-{}",
                                        column_id.0
                                    )))
                                    .mt(px(5.))
                                    .cursor_pointer()
                                    .text_color(muted)
                                    .hover(|button| button.text_color(accent))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.add_column_block(column_id, window, cx)
                                    }))
                                    .child("＋ 添加内容"),
                            )
                    })
                    .collect::<Vec<_>>();
                let columns_count = columns.len();
                let column_actions = vec![
                    div()
                        .id(SharedString::from(format!("columns-add-{}", block_id.0)))
                        .px(px(7.))
                        .py(px(3.))
                        .cursor_pointer()
                        .text_color(muted)
                        .hover(|button| button.text_color(accent))
                        .when(columns_count < 3, |button| {
                            button.on_click(cx.listener(move |this, _, window, cx| {
                                this.add_columns_column(block_id, window, cx)
                            }))
                        })
                        .child("＋ 分栏"),
                    div()
                        .id(SharedString::from(format!("columns-remove-{}", block_id.0)))
                        .px(px(7.))
                        .py(px(3.))
                        .cursor_pointer()
                        .text_color(muted)
                        .hover(|button| button.text_color(accent))
                        .when(columns_count > 2, |button| {
                            button.on_click(cx.listener(move |this, _, window, cx| {
                                this.remove_columns_column(block_id, window, cx)
                            }))
                        })
                        .child("− 分栏"),
                ];
                div()
                    .id(("block", block.id.0))
                    .w_full()
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .py(px(4.))
                    .when(block.parent.is_some(), |row| row.ml(px(18.)))
                    .when(kind == BlockKind::Code, |row| {
                        row.bg(Palette::color(&p.elevated))
                            .rounded(px(5.))
                            .px(px(8.))
                    })
                    .when(kind == BlockKind::Quote, |row| {
                        row.border_l_2()
                            .border_color(Palette::color(&p.border))
                            .pl(px(12.))
                    })
                    .when(kind == BlockKind::Callout, |row| {
                        row.bg(Palette::color(&p.elevated))
                            .rounded(px(5.))
                            .px(px(10.))
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("todo-{block_id:?}")))
                            .w(px(18.))
                            .flex_shrink_0()
                            .text_color(muted)
                            .when(is_todo || is_toggle, |prefix| {
                                prefix.cursor_pointer().on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        if is_todo {
                                            this.toggle_todo(block_id, window, cx)
                                        } else {
                                            this.toggle_block_folded(block_id, window, cx)
                                        }
                                    },
                                ))
                            })
                            .child(prefix),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .when(kind == BlockKind::Table, |view| {
                                view.flex_col()
                                    .children(table_rows)
                                    .child(div().flex().gap(px(6.)).children(table_actions))
                            })
                            .when(kind == BlockKind::Columns, |view| {
                                view.flex_col()
                                    .gap(px(7.))
                                    .child(div().w_full().flex().gap(px(8.)).children(column_views))
                                    .child(div().flex().gap(px(6.)).children(column_actions))
                            })
                            .when(kind == BlockKind::Divider, |view| {
                                view.py(px(8.))
                                    .child(div().w_full().h(px(1.)).bg(Palette::color(&p.border)))
                            })
                            .when(
                                kind != BlockKind::Divider
                                    && kind != BlockKind::Table
                                    && kind != BlockKind::Columns,
                                |view| {
                                    view.font_family(if kind == BlockKind::Code {
                                        "Cascadia Code"
                                    } else {
                                        "Segoe UI"
                                    })
                                    .when_some(input.filter(|_| !self.closing), |view, input| {
                                        view.child(input)
                                    })
                                    .when(
                                        self.closing || !self.inputs.contains_key(&block.id.0),
                                        |view| view.child(block.text.clone()),
                                    )
                                },
                            )
                            .children(slash_options),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("delete-block-{block_id:?}")))
                            .px(px(4.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.delete_block(block_id, window, cx)
                            }))
                            .child("×"),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .key_context("Quire")
            .size_full()
            .flex()
            .flex_col()
            .font_family(match self.session.page.font {
                quire_core::core::types::PageFont::Default => "Segoe UI",
                quire_core::core::types::PageFont::Serif => "Georgia",
                quire_core::core::types::PageFont::Mono => "Cascadia Code",
            })
            .text_size(px(if self.session.page.small_text {
                12.
            } else {
                13.
            }))
            .text_color(text)
            .bg(linear_gradient(
                180.,
                linear_color_stop(Palette::color(&p.bg), 0.),
                linear_color_stop(Palette::color(&p.grad2), 1.),
            )
            .color_space(ColorSpace::Srgb))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .child(
                div()
                    .h(px(40.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(16.))
                    .border_b_1()
                    .border_color(Palette::color(&p.border))
                    .child(format!("工作区  ›  {}", self.session.page.title))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .id("page-icon-picker")
                                    .px(px(7.))
                                    .py(px(4.))
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .text_color(Palette::color(&p.accent))
                                    .hover(|s| s.bg(Palette::color(&p.elevated)))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_icon_picker(cx)),
                                    )
                                    .child(format!(
                                        "图标 · {}",
                                        if self.session.page.icon.is_empty() {
                                            "选择"
                                        } else {
                                            &self.session.page.icon
                                        }
                                    )),
                            )
                            .child(
                                div()
                                    .id("page-lock")
                                    .px(px(7.))
                                    .py(px(4.))
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(Palette::color(&p.elevated)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_page_lock(window, cx)
                                    }))
                                    .child(if self.session.page.locked {
                                        "解锁"
                                    } else {
                                        "锁定"
                                    }),
                            )
                            .child(
                                div()
                                    .id("page-width")
                                    .px(px(7.))
                                    .py(px(4.))
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(Palette::color(&p.elevated)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_full_width(window, cx)
                                    }))
                                    .child(if self.session.page.full_width {
                                        "居中"
                                    } else {
                                        "全宽"
                                    }),
                            )
                            .child(
                                div()
                                    .id("page-small-text")
                                    .px(px(7.))
                                    .py(px(4.))
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(Palette::color(&p.elevated)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_small_text(window, cx)
                                    }))
                                    .child(if self.session.page.small_text {
                                        "标准字"
                                    } else {
                                        "小字号"
                                    }),
                            )
                            .child(
                                div()
                                    .id("page-font")
                                    .px(px(7.))
                                    .py(px(4.))
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(Palette::color(&p.elevated)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.cycle_page_font(window, cx)
                                    }))
                                    .child(format!("字体 · {}", self.session.page.font.label())),
                            )
                            .child(
                                div()
                                    .id("theme-picker-toggle")
                                    .px(px(9.))
                                    .py(px(4.))
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .text_color(Palette::color(&p.accent))
                                    .hover(|s| s.bg(Palette::color(&p.elevated)))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_theme_picker(cx)),
                                    )
                                    .child(format!("主题 · {}  ▾", p.id)),
                            )
                            .child(
                                div()
                                    .id("save")
                                    .px(px(12.))
                                    .py(px(4.))
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(Palette::color(&p.elevated)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.save(&Save, window, cx)
                                    }))
                                    .child("保存  Ctrl+S"),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(36.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(12.))
                    .border_b_1()
                    .border_color(Palette::color(&p.border))
                    .children(kind_buttons)
                    .child(div().w(px(1.)).h(px(18.)).bg(Palette::color(&p.border)))
                    .children(mark_buttons),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .w(px(240.))
                            .flex_shrink_0()
                            .h_full()
                            .bg(Palette::color(&p.elevated))
                            .px(px(12.))
                            .py(px(16.))
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child("Quire"))
                            .children(theme_options)
                            .children(icon_options)
                            .child(
                                div()
                                    .h(px(30.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.))
                                    .bg(Palette::color(&p.bg))
                                    .text_color(muted)
                                    .child(self.search_input.clone()),
                            )
                            .when_some(delete_confirmation, |sidebar, confirmation| {
                                sidebar.child(confirmation)
                            })
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .mt(px(16.))
                                    .child("工作区"),
                            )
                            .child(
                                div()
                                    .id("page-list-scroll")
                                    .flex_1()
                                    .h_full()
                                    .overflow_y_scroll()
                                    .when(!recent_rows.is_empty(), |list| {
                                        list.child(
                                            div()
                                                .h(px(24.))
                                                .px(px(8.))
                                                .flex()
                                                .items_center()
                                                .text_size(px(11.))
                                                .text_color(muted)
                                                .child("最近"),
                                        )
                                        .children(recent_rows)
                                    })
                                    .when(!favorite_rows.is_empty(), |list| {
                                        list.child(
                                            div()
                                                .h(px(24.))
                                                .px(px(8.))
                                                .flex()
                                                .items_center()
                                                .text_size(px(11.))
                                                .text_color(muted)
                                                .child("收藏"),
                                        )
                                        .children(favorite_rows)
                                    })
                                    .children(page_rows)
                                    .child(
                                        div()
                                            .id("add-page")
                                            .h(px(30.))
                                            .px(px(8.))
                                            .flex()
                                            .items_center()
                                            .rounded(px(5.))
                                            .cursor_pointer()
                                            .text_color(muted)
                                            .hover(|row| {
                                                row.bg(Palette::color(&p.bg).opacity(0.24))
                                            })
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.add_page(window, cx)
                                            }))
                                            .child("＋  新建页面"),
                                    ),
                            )
                            .child(
                                div()
                                    .h(px(28.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.))
                                    .bg(rgba(0xffffff14))
                                    .text_color(accent)
                                    .child(self.session.page.title.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .child("GPUI · 开发预览"),
                            ),
                    )
                    .child(
                        div()
                            .id("document-scroll")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_y_scroll()
                            .px(px(48.))
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(if self.session.page.full_width {
                                        1120.
                                    } else {
                                        760.
                                    }))
                                    .mx_auto()
                                    .pt(px(64.))
                                    .pb(px(128.))
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_size(px(38.))
                                            .font_weight(FontWeight::BOLD)
                                            .mb(px(16.))
                                            .child(self.title_input.clone()),
                                    )
                                    .when(self.session.page.locked, |d| {
                                        d.child(
                                            div()
                                                .text_color(muted)
                                                .mb(px(12.))
                                                .child("此页面已锁定"),
                                        )
                                    })
                                    .children(rows),
                            ),
                    ),
            )
            .child(
                div()
                    .min_h(px(30.))
                    .flex_shrink_0()
                    .px(px(16.))
                    .py(px(6.))
                    .border_t_1()
                    .border_color(Palette::color(&p.border))
                    .text_size(px(11.))
                    .text_color(if self.error {
                        rgb(0xe5534b).into()
                    } else {
                        muted
                    })
                    .child(format!(
                        "{}{}  ·  {}",
                        self.status,
                        if self.smoke {
                            " · 自动回放（非 IME 验收）"
                        } else {
                            ""
                        },
                        self.session.path.display()
                    )),
            )
    }
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-s", Save, Some("Quire")),
        KeyBinding::new("ctrl-z", Undo, Some("Quire")),
        KeyBinding::new("ctrl-shift-z", Redo, Some("Quire")),
        KeyBinding::new("ctrl-y", Redo, Some("Quire")),
    ]);
}
