use crate::input::{InputEvent, TextInput};
use gpui::{AnimationExt, prelude::*, *};
use quire_core::core::{
    command::Command,
    database::ViewId,
    organizer::{OrganizerCatalog, Priority, Repeat, Task as OrganizerTask},
    types::{Block, BlockId, BlockKind, ColorKind, Lang},
};
use quire_gpui::session::Session;
use quire_gpui::ui_scale::px;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Range,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Copy)]
enum SidebarGlyph {
    Search,
    Settings,
    Note,
    TodoCheck,
    ChevronDown,
}

fn sidebar_glyph(glyph: SidebarGlyph, color: Hsla, size: Pixels) -> impl IntoElement {
    canvas(
        move |bounds, _, _| {
            let origin = bounds.origin;
            let size = bounds.size;
            let sx = size.width / 24.;
            let sy = size.height / 24.;
            let point_at = |x: f32, y: f32| point(origin.x + sx * x, origin.y + sy * y);
            let mut path = PathBuilder::stroke(px(1.8));
            match glyph {
                SidebarGlyph::Search => {
                    path.move_to(point_at(11., 4.));
                    path.arc_to(
                        point(sx * 7., sy * 7.),
                        px(0.),
                        false,
                        true,
                        point_at(11., 18.),
                    );
                    path.arc_to(
                        point(sx * 7., sy * 7.),
                        px(0.),
                        false,
                        true,
                        point_at(11., 4.),
                    );
                    path.move_to(point_at(16., 16.));
                    path.line_to(point_at(20., 20.));
                }
                SidebarGlyph::Settings => {
                    path.move_to(point_at(4., 8.));
                    path.line_to(point_at(20., 8.));
                    path.move_to(point_at(9., 5.));
                    path.line_to(point_at(9., 11.));
                    path.move_to(point_at(4., 16.));
                    path.line_to(point_at(20., 16.));
                    path.move_to(point_at(17., 13.));
                    path.line_to(point_at(17., 19.));
                }
                SidebarGlyph::Note => {
                    path.move_to(point_at(5., 3.));
                    path.line_to(point_at(15., 3.));
                    path.line_to(point_at(19., 7.));
                    path.line_to(point_at(19., 21.));
                    path.line_to(point_at(5., 21.));
                    path.close();
                    path.move_to(point_at(15., 3.));
                    path.line_to(point_at(15., 7.));
                    path.line_to(point_at(19., 7.));
                    path.move_to(point_at(8.5, 12.));
                    path.line_to(point_at(15.5, 12.));
                    path.move_to(point_at(8.5, 16.));
                    path.line_to(point_at(15.5, 16.));
                }
                SidebarGlyph::TodoCheck => {
                    path = PathBuilder::stroke(px(2.6));
                    path.move_to(point_at(4.5, 12.5));
                    path.line_to(point_at(9.5, 17.));
                    path.line_to(point_at(19.5, 6.5));
                }
                SidebarGlyph::ChevronDown => {
                    path.move_to(point_at(5., 9.5));
                    path.line_to(point_at(12., 16.5));
                    path.line_to(point_at(19., 9.5));
                }
            }
            path.build().expect("static sidebar glyph path")
        },
        move |bounds, path, window, _| {
            let _ = bounds;
            window.paint_path(path, color);
        },
    )
    .size(size)
}

actions!(
    workspace,
    [
        Save,
        Undo,
        Redo,
        Find,
        FindNext,
        FindPrevious,
        SearchPages,
        ZoomIn,
        ZoomOut,
        ZoomReset
    ]
);

fn find_matches_in(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() || query.len() > text.len() {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    let mut next_allowed = 0;
    for (start, _) in text.char_indices() {
        if start < next_allowed {
            continue;
        }
        let Some(end) = start.checked_add(query.len()) else {
            continue;
        };
        if end <= text.len()
            && text.is_char_boundary(end)
            && text[start..end].eq_ignore_ascii_case(query)
        {
            ranges.push(start..end);
            next_allowed = end;
        }
    }
    ranges
}

fn priority_label(priority: Priority) -> &'static str {
    match priority {
        Priority::None => "无",
        Priority::Low => "低",
        Priority::Medium => "中",
        Priority::High => "高",
    }
}

fn repeat_label(repeat: Repeat) -> &'static str {
    match repeat {
        Repeat::None => "不重复",
        Repeat::Daily => "每天",
        Repeat::Weekdays => "每个工作日",
        Repeat::Weekly => "每周",
        Repeat::Monthly => "每月",
    }
}

fn parse_organizer_tags(input: &str) -> Vec<String> {
    input
        .split([',', '，'])
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .fold(Vec::<String>::new(), |mut tags, tag| {
            if !tags.iter().any(|existing| existing == &tag) {
                tags.push(tag);
            }
            tags
        })
}

fn task_matches_smart_view(
    catalog: &OrganizerCatalog,
    task: &OrganizerTask,
    view: i32,
    today: &str,
    week: &str,
) -> bool {
    match view {
        0 => catalog.list(task.list).is_none(),
        1 => task.due.as_deref() == Some(today),
        2 => matches!(task.due.as_deref(), Some(due) if due > today && due <= week),
        4 => task.done,
        _ => true,
    }
}

fn toc_headings(blocks: &[Block]) -> Vec<(BlockId, u8, String)> {
    blocks
        .iter()
        .filter_map(|block| {
            let level = block.kind.heading_level()?;
            let mut parent = block.parent;
            let mut hidden = false;
            let mut guard = 0;
            while let Some(parent_id) = parent {
                let Some(ancestor) = blocks.iter().find(|candidate| candidate.id == parent_id)
                else {
                    break;
                };
                if ancestor.folded || ancestor.kind == BlockKind::TableCell {
                    hidden = true;
                    break;
                }
                parent = ancestor.parent;
                guard += 1;
                if guard >= 32 {
                    hidden = true;
                    break;
                }
            }
            (!hidden).then(|| {
                (
                    block.id,
                    level,
                    if block.text.is_empty() {
                        "无标题".to_owned()
                    } else {
                        block.text.clone()
                    },
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod view_tests {
    use super::{find_matches_in, parse_organizer_tags, toc_headings};
    use quire_core::core::types::{Block, BlockId, BlockKind, ColorKind, Lang, OrderKey, PageId};

    #[test]
    fn organizer_tag_entry_trims_deduplicates_and_accepts_fullwidth_commas() {
        assert_eq!(
            parse_organizer_tags(" 设计,复刻， 设计 ,,桌面端 "),
            ["设计", "复刻", "桌面端"]
        );
    }

    fn block(id: u64, kind: BlockKind, text: &str, parent: Option<BlockId>, folded: bool) -> Block {
        Block {
            id: BlockId(id),
            page: PageId(1),
            parent,
            order: OrderKey(id),
            kind,
            text: text.into(),
            checked: false,
            marks: Vec::new(),
            color: ColorKind::Default,
            background: ColorKind::Default,
            page_ref: None,
            folded,
            attachment: None,
            img_percent: 100,
            columns: 0,
            lang: Lang::Plain,
            db_ref: None,
            sync_ref: None,
        }
    }

    #[test]
    fn find_matches_are_non_overlapping_case_insensitive_and_utf8_aligned() {
        assert_eq!(
            find_matches_in("Alpha α alpha Alpha", "alpha"),
            [0..5, 9..14, 15..20]
        );
        assert_eq!(find_matches_in("标题标题", "标题"), [0..6, 6..12]);
        assert!(find_matches_in("anything", "").is_empty());
    }

    #[test]
    fn toc_lists_headings_and_omits_folded_subtrees() {
        let blocks = [
            block(1, BlockKind::Toggle, "Section", None, true),
            block(
                2,
                BlockKind::Heading2,
                "Hidden heading",
                Some(BlockId(1)),
                false,
            ),
            block(3, BlockKind::Heading1, "Visible heading", None, false),
        ];
        assert_eq!(
            toc_headings(&blocks),
            vec![(BlockId(3), 1, "Visible heading".into())]
        );
    }
}

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
        ("数学公式", "math latex formula", BlockKind::Math),
        ("链接卡片", "embed link card", BlockKind::Embed),
        ("同步块", "synced block mirror", BlockKind::Synced),
        ("页面", "page embed child page", BlockKind::Page),
        ("链接到页面", "link page reference", BlockKind::Link),
        ("提示块", "callout", BlockKind::Callout),
        ("分隔线", "divider", BlockKind::Divider),
        ("表格", "table grid", BlockKind::Table),
        (
            "数据库",
            "database records properties table board calendar",
            BlockKind::Database,
        ),
        ("分栏", "columns layout", BlockKind::Columns),
        ("目录", "toc table of contents", BlockKind::Toc),
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
    #[serde(rename = "bg-elev2")]
    elevated2: String,
    #[serde(rename = "fg-muted")]
    muted: String,
    border: String,
    accent: String,
    light: bool,
}
impl Palette {
    fn load(id: &str) -> Self {
        let palettes: Vec<Self> = serde_json::from_str(include_str!("palettes.json"))
            .expect("checked-in design palettes");
        let id = if id == "system" {
            system_palette_id()
        } else {
            id
        };
        palettes
            .iter()
            .find(|p| p.id == id)
            .unwrap_or(&palettes[0])
            .clone()
    }
    fn color(value: &str) -> Hsla {
        rgb(u32::from_str_radix(value.trim_start_matches('#'), 16).expect("palette hex")).into()
    }
    fn block_text(&self, slot: i32) -> Hsla {
        let dark = !self.light;
        let values = if dark {
            [
                "#e7e7ea", "#9b9a97", "#c3996b", "#e28d50", "#dcae3f", "#6ba584", "#5b9bc4",
                "#ad7fb8", "#d3709f", "#e06d66",
            ]
        } else {
            [
                "#1f2328", "#787774", "#9f6b53", "#bd6408", "#a87718", "#448361", "#337ea9",
                "#9065b0", "#c14c8a", "#d44c47",
            ]
        };
        Self::color(values[slot.clamp(0, 9) as usize])
    }
    fn block_bg(&self, slot: i32) -> Hsla {
        if slot <= 0 {
            return rgb(0x00000000).into();
        }
        let dark = !self.light;
        let values = if dark {
            [
                "#00000000",
                "#2f2f2f",
                "#2e2420",
                "#38291c",
                "#372c14",
                "#1e2c21",
                "#1d272e",
                "#2a2233",
                "#322028",
                "#37201f",
            ]
        } else {
            [
                "#00000000",
                "#f1f1ef",
                "#f4eeee",
                "#faebdd",
                "#fbf3db",
                "#edf3ec",
                "#e7f3f8",
                "#f6f3f9",
                "#faf1f5",
                "#fdebec",
            ]
        };
        Self::color(values[slot.clamp(0, 9) as usize])
    }
}

fn system_palette_id() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                "/v",
                "AppsUseLightTheme",
            ])
            .output();
        if output.is_ok_and(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .last()
                .is_some_and(|value| value == "0x1")
        }) {
            return "light";
        }
        "midnight"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "midnight"
    }
}

pub struct Workspace {
    session: Session,
    title_input: Entity<TextInput>,
    title_subscription: Option<Subscription>,
    title_edit_original: Option<String>,
    search_input: Entity<TextInput>,
    find_input: Entity<TextInput>,
    link_target_input: Entity<TextInput>,
    note_title_input: Entity<TextInput>,
    note_body_input: Entity<TextInput>,
    task_title_input: Entity<TextInput>,
    task_due_input: Entity<TextInput>,
    organizer_tags_input: Entity<TextInput>,
    subtask_title_input: Entity<TextInput>,
    database_cell_input: Entity<TextInput>,
    search_subscription: Option<Subscription>,
    find_subscription: Option<Subscription>,
    note_title_subscription: Option<Subscription>,
    note_body_subscription: Option<Subscription>,
    task_title_subscription: Option<Subscription>,
    task_due_subscription: Option<Subscription>,
    organizer_tags_subscription: Option<Subscription>,
    subtask_title_subscription: Option<Subscription>,
    database_cell_subscription: Option<Subscription>,
    search_query: String,
    find_query: String,
    find_matches: Vec<(BlockId, Range<usize>)>,
    find_index: Option<usize>,
    find_open: bool,
    search_open: bool,
    search_selected: usize,
    backlinks_open: bool,
    hovered_block: Option<BlockId>,
    hovered_page: Option<quire_core::core::types::PageId>,
    hovered_note: Option<quire_core::core::organizer::NoteId>,
    hovered_task: Option<quire_core::core::organizer::TaskId>,
    inputs: BTreeMap<u64, Entity<TextInput>>,
    selected_database_views: HashMap<u64, ViewId>,
    active_database_cell: Option<(u64, u64, u64, u64)>,
    subscriptions: Vec<Subscription>,
    palette: Palette,
    transition_palette: Option<Palette>,
    theme_choice: String,
    ui_zoom: f32,
    theme_picker_open: bool,
    settings_tab: usize,
    icon_picker_open: bool,
    format_bar_open: bool,
    page_options_open: bool,
    link_editor_open: bool,
    color_picker: Option<(BlockId, bool)>,
    confirm_delete_page: Option<quire_core::core::types::PageId>,
    slash_menu: Option<BlockId>,
    slash_selection: usize,
    mention_menu: Option<(BlockId, usize, String)>,
    mention_selection: usize,
    organizer_open: bool,
    sidebar_open: bool,
    sidebar_transition: Option<(u64, f32, f32)>,
    sidebar_transition_seq: u64,
    sidebar_width: f32,
    sidebar_resizing: bool,
    sidebar_drag_offset: f32,
    organizer_tasks: bool,
    organizer_trash: bool,
    organizer_tag: Option<String>,
    selected_task_list: quire_core::core::organizer::ListId,
    selected_task_smart_view: Option<i32>,
    show_task_done: bool,
    task_sort: i32,
    note_sort_recent: bool,
    task_multi_select: bool,
    selected_note: Option<quire_core::core::organizer::NoteId>,
    selected_task: Option<quire_core::core::organizer::TaskId>,
    selected_task_ids: HashSet<quire_core::core::organizer::TaskId>,
    editing_subtask: Option<(quire_core::core::organizer::TaskId, u64)>,
    note_title_draft: String,
    note_body_draft: String,
    task_title_draft: String,
    task_due_draft: String,
    organizer_tags_draft: String,
    subtask_title_draft: String,
    task_due_error: bool,
    status: String,
    error: bool,
    active: Option<BlockId>,
    save_task: Option<gpui::Task<()>>,
    flush_lock: Arc<Mutex<()>>,
    closing: bool,
    smoke: bool,
    normal_window_bounds: Bounds<Pixels>,
}

impl Workspace {
    pub fn new(
        session: Session,
        theme: &str,
        smoke: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme_choice = session.setting("gpui.theme").unwrap_or(theme).to_owned();
        let ui_zoom = session
            .setting("gpui.ui-scale")
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| (0.5..=3.0).contains(value))
            .unwrap_or(1.0);
        let sidebar_width = session
            .setting("gpui.sidebar-width")
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite())
            .unwrap_or(260.)
            .clamp(200., 480.);
        quire_gpui::ui_scale::set(ui_zoom);
        let title_input =
            cx.new(|cx| TextInput::new(session.page.title.clone(), Vec::new(), 38., window, cx));
        title_input.update(cx, |input, _| input.font_weight = FontWeight::BOLD);
        let search_input = cx.new(|cx| TextInput::new(String::new(), Vec::new(), 12., window, cx));
        search_input.update(cx, |input, _| {
            input.placeholder = Some("搜索页面…".into());
        });
        let find_input = cx.new(|cx| TextInput::new(String::new(), Vec::new(), 12., window, cx));
        find_input.update(cx, |input, _| {
            input.placeholder = Some("查找当前页面".into());
        });
        let link_target_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 12., window, cx));
        link_target_input.update(cx, |input, _| {
            input.placeholder = Some("https://… 或 quire://page/ID".into())
        });
        let note_title_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 24., window, cx));
        note_title_input.update(cx, |input, _| input.placeholder = Some("笔记标题".into()));
        let note_body_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 15., window, cx));
        note_body_input.update(cx, |input, _| input.placeholder = Some("写点什么…".into()));
        let task_title_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 13., window, cx));
        task_title_input.update(cx, |input, _| {
            input.placeholder = Some("添加任务到「收集箱」…".into())
        });
        let task_due_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 12., window, cx));
        task_due_input.update(cx, |input, _| input.placeholder = Some("YYYY-MM-DD".into()));
        let organizer_tags_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 12., window, cx));
        organizer_tags_input.update(cx, |input, _| {
            input.placeholder = Some("标签，以逗号分隔".into())
        });
        let subtask_title_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 12., window, cx));
        subtask_title_input.update(cx, |input, _| {
            input.placeholder = Some("添加子任务…".into())
        });
        let database_cell_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 12., window, cx));
        let input_fg = Palette::color(&Palette::load(theme).fg);
        for input in [
            title_input.clone(),
            search_input.clone(),
            find_input.clone(),
            link_target_input.clone(),
            note_title_input.clone(),
            note_body_input.clone(),
            task_title_input.clone(),
            task_due_input.clone(),
            organizer_tags_input.clone(),
            subtask_title_input.clone(),
            database_cell_input.clone(),
        ] {
            input.update(cx, |input, _| input.text_color = Some(input_fg));
        }
        let mut this = Self {
            session,
            title_input,
            title_subscription: None,
            title_edit_original: None,
            search_input,
            find_input,
            link_target_input,
            note_title_input,
            note_body_input,
            task_title_input,
            task_due_input,
            organizer_tags_input,
            subtask_title_input,
            database_cell_input,
            search_subscription: None,
            find_subscription: None,
            note_title_subscription: None,
            note_body_subscription: None,
            task_title_subscription: None,
            task_due_subscription: None,
            organizer_tags_subscription: None,
            subtask_title_subscription: None,
            database_cell_subscription: None,
            search_query: String::new(),
            find_query: String::new(),
            find_matches: Vec::new(),
            find_index: None,
            find_open: false,
            search_open: false,
            search_selected: 0,
            backlinks_open: false,
            hovered_block: None,
            hovered_page: None,
            hovered_note: None,
            hovered_task: None,
            inputs: BTreeMap::new(),
            selected_database_views: HashMap::new(),
            active_database_cell: None,
            subscriptions: Vec::new(),
            palette: Palette::load(&theme_choice),
            transition_palette: None,
            theme_choice,
            ui_zoom,
            theme_picker_open: false,
            settings_tab: 0,
            icon_picker_open: false,
            format_bar_open: false,
            page_options_open: false,
            link_editor_open: false,
            color_picker: None,
            confirm_delete_page: None,
            slash_menu: None,
            slash_selection: 0,
            mention_menu: None,
            mention_selection: 0,
            organizer_open: false,
            sidebar_open: true,
            sidebar_transition: None,
            sidebar_transition_seq: 0,
            sidebar_width,
            sidebar_resizing: false,
            sidebar_drag_offset: 0.,
            organizer_tasks: false,
            organizer_trash: false,
            organizer_tag: None,
            selected_task_list: quire_core::core::organizer::ListId::INBOX,
            selected_task_smart_view: Some(0),
            show_task_done: false,
            task_sort: 0,
            note_sort_recent: true,
            task_multi_select: false,
            selected_note: None,
            selected_task: None,
            selected_task_ids: HashSet::new(),
            editing_subtask: None,
            note_title_draft: String::new(),
            note_body_draft: String::new(),
            task_title_draft: String::new(),
            task_due_draft: String::new(),
            organizer_tags_draft: String::new(),
            subtask_title_draft: String::new(),
            task_due_error: false,
            status: "已保存".into(),
            error: false,
            active: None,
            save_task: None,
            flush_lock: Arc::new(Mutex::new(())),
            closing: false,
            smoke,
            normal_window_bounds: window.bounds(),
        };
        let bounds_subscription = cx.observe_window_bounds(window, |this, window, _| {
            if !window.is_maximized() && !window.is_fullscreen() {
                this.normal_window_bounds = window.bounds();
            }
        });
        this.subscriptions.push(bounds_subscription);
        let note_title_input = this.note_title_input.clone();
        this.note_title_subscription = Some(cx.subscribe_in(
            &note_title_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.note_title_draft = text.clone();
                    cx.notify();
                }
                InputEvent::Blurred => this.commit_note_draft(window, cx),
                InputEvent::Split(_) => window.focus(&this.note_body_input.read(cx).focus),
                _ => {}
            },
        ));
        let note_body_input = this.note_body_input.clone();
        this.note_body_subscription = Some(cx.subscribe_in(
            &note_body_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.note_body_draft = text.clone();
                    cx.notify();
                }
                InputEvent::Blurred => this.commit_note_draft(window, cx),
                InputEvent::Split(_) => this.note_body_input.update(cx, |input, cx| {
                    input.insert_newline(cx);
                }),
                _ => {}
            },
        ));
        let task_title_input = this.task_title_input.clone();
        this.task_title_subscription = Some(cx.subscribe_in(
            &task_title_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.task_title_draft = text.clone();
                    cx.notify();
                }
                InputEvent::Split(_) => this.create_task_from_draft(window, cx),
                _ => {}
            },
        ));
        let task_due_input = this.task_due_input.clone();
        this.task_due_subscription = Some(cx.subscribe_in(
            &task_due_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.task_due_draft = text.clone();
                    cx.notify();
                }
                InputEvent::Blurred => this.commit_note_draft(window, cx),
                _ => {}
            },
        ));
        let organizer_tags_input = this.organizer_tags_input.clone();
        this.organizer_tags_subscription = Some(cx.subscribe_in(
            &organizer_tags_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.organizer_tags_draft = text.clone();
                    cx.notify();
                }
                InputEvent::Blurred => this.commit_note_draft(window, cx),
                _ => {}
            },
        ));
        let subtask_title_input = this.subtask_title_input.clone();
        this.subtask_title_subscription = Some(cx.subscribe_in(
            &subtask_title_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.subtask_title_draft = text.clone();
                    cx.notify();
                }
                InputEvent::Blurred => this.commit_subtask_draft(window, cx),
                InputEvent::Split(_) => {
                    if this.editing_subtask.is_some() {
                        this.commit_subtask_draft(window, cx)
                    } else {
                        this.add_subtask_from_draft(window, cx)
                    }
                }
                _ => {}
            },
        ));
        let database_cell_input = this.database_cell_input.clone();
        this.database_cell_subscription = Some(cx.subscribe_in(
            &database_cell_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { .. } => cx.notify(),
                InputEvent::Split(_) => {
                    this.commit_database_cell(true, window, cx);
                    if this.active_database_cell.is_none() {
                        this.focus_first(window, cx);
                    }
                }
                InputEvent::Blurred => this.commit_database_cell(false, window, cx),
                InputEvent::DismissSlash => {
                    this.active_database_cell = None;
                    this.focus_first(window, cx);
                    cx.notify();
                }
                _ => {}
            },
        ));
        let title_input = this.title_input.clone();
        this.title_subscription = Some(cx.subscribe_in(
            &title_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Focused => {
                    if this.title_edit_original.is_none() {
                        this.title_edit_original = Some(this.session.page.title.clone());
                    }
                }
                InputEvent::Edited { text, boundary } => {
                    if this.session.page.locked {
                        let title = this.session.page.title.clone();
                        this.title_input.update(cx, |input, cx| {
                            input.reset(title.clone(), Vec::new(), title.len(), cx)
                        });
                    } else {
                        if this.title_edit_original.is_none() {
                            this.title_edit_original = Some(this.session.page.title.clone());
                        }
                        this.session.set_page_title(text.clone());
                        this.schedule_save(*boundary, window, cx);
                    }
                }
                InputEvent::DismissSlash => {
                    if let Some(title) = this.title_edit_original.take() {
                        this.session.set_page_title(title.clone());
                        this.title_input.update(cx, |input, cx| {
                            let len = title.len();
                            input.reset(title, Vec::new(), len, cx);
                        });
                        this.schedule_save(true, window, cx);
                    }
                }
                InputEvent::Split(_) => {
                    this.title_edit_original = None;
                    this.focus_first(window, cx);
                }
                InputEvent::Blurred => this.title_edit_original = None,
                _ => {}
            },
        ));
        let search_input = this.search_input.clone();
        this.search_subscription = Some(cx.subscribe_in(
            &search_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.search_query = text.clone();
                    this.search_selected = 0;
                    cx.notify();
                }
                InputEvent::Navigate(direction) => {
                    let count = this.search_result_ids().len();
                    if count > 0 {
                        this.search_selected = (this.search_selected as i32 + direction)
                            .rem_euclid(count as i32)
                            as usize;
                        cx.notify();
                    }
                }
                InputEvent::Split(_) => this.open_selected_search_result(window, cx),
                InputEvent::DismissSlash => {
                    if this.theme_picker_open {
                        this.close_settings(window, cx);
                    } else {
                        this.close_search(window, cx);
                    }
                }
                _ => {}
            },
        ));
        let find_input = this.find_input.clone();
        this.find_subscription = Some(cx.subscribe_in(
            &find_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.find_query = text.clone();
                    this.find_index = None;
                    this.refresh_find_matches();
                    cx.notify();
                }
                InputEvent::DismissSlash => this.close_find(window, cx),
                _ => {}
            },
        ));
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
            let weight = if matches!(
                block.kind,
                BlockKind::Heading1 | BlockKind::Heading2 | BlockKind::Heading3
            ) {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            };
            let input = if let Some(input) = self.inputs.get(&block.id.0) {
                input.clone()
            } else {
                let text = block.text.clone();
                let marks = block.marks.clone();
                let input = cx.new(|cx| {
                    let mut input = TextInput::new(text, marks, font, window, cx);
                    input.font_weight = weight;
                    input
                });
                self.inputs.insert(block.id.0, input.clone());
                input
            };
            let block_text_color = (block.color != ColorKind::Default)
                .then(|| self.palette.block_text(block.color.slot()));
            let code_colors = [
                self.palette.block_text(7),
                self.palette.block_text(1),
                self.palette.block_text(5),
                self.palette.block_text(3),
                self.palette.block_text(6),
            ];
            input.update(cx, |input, cx| {
                input.font_size = font;
                input.font_weight = weight;
                input.text_color =
                    Some(block_text_color.unwrap_or_else(|| Palette::color(&self.palette.fg)));
                input.code_lang = if block.kind == BlockKind::Code {
                    block.lang
                } else {
                    Lang::Plain
                };
                input.code_colors = code_colors;
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
                self.slash_selection = 0;
                self.mention_menu = self.inputs.get(&id.0).and_then(|input| {
                    let input = input.read(cx);
                    let caret = input.buffer.caret();
                    if input.buffer.composing() || caret != text.len() {
                        return None;
                    }
                    quire_core::core::reference::trigger_at(text)
                        .map(|at| (id, at, text[at + 1..caret].to_lowercase()))
                });
                self.mention_selection = 0;
                if self.find_open {
                    self.find_index = None;
                    self.refresh_find_matches();
                }
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
                if let Some((mention_id, at, _)) = self
                    .mention_menu
                    .clone()
                    .filter(|(mention_id, _, _)| *mention_id == id)
                {
                    let matches = self.mention_matches();
                    if let Some(page) = matches
                        .get(self.mention_selection.min(matches.len().saturating_sub(1)))
                        .cloned()
                    {
                        self.select_mention(mention_id, at, page, window, cx);
                        return;
                    }
                }
                if self
                    .session
                    .document
                    .block(id)
                    .is_some_and(|block| block.kind == BlockKind::TableCell)
                {
                    self.navigate_table_cell(id, 1, window, cx);
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
                    let matches = slash_items()
                        .iter()
                        .filter(|(label, keywords, _)| {
                            query.is_empty()
                                || label.to_lowercase().contains(&query)
                                || keywords.contains(&query)
                        })
                        .collect::<Vec<_>>();
                    if let Some((_, _, kind)) =
                        matches.get(self.slash_selection.min(matches.len().saturating_sub(1)))
                    {
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
                if self
                    .mention_menu
                    .as_ref()
                    .is_some_and(|(mention_id, _, _)| *mention_id == id)
                {
                    let count = self.mention_matches().len();
                    if count > 0 {
                        self.mention_selection = (self.mention_selection as i64 + *direction as i64)
                            .rem_euclid(count as i64)
                            as usize;
                        cx.notify();
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
                    let count = slash_items()
                        .iter()
                        .filter(|(label, keywords, _)| {
                            query.is_empty()
                                || label.to_lowercase().contains(&query)
                                || keywords.contains(&query)
                        })
                        .count();
                    if count > 0 {
                        self.slash_selection = (self.slash_selection as i64 + *direction as i64)
                            .rem_euclid(count as i64)
                            as usize;
                        cx.notify();
                    }
                    return;
                }
                if let Some(table_id) = self
                    .session
                    .document
                    .block(id)
                    .filter(|block| block.kind == BlockKind::TableCell)
                    .and_then(|block| block.parent)
                {
                    let columns = self
                        .session
                        .document
                        .block(table_id)
                        .map_or(1, |table| i32::from(table.columns.max(1)));
                    self.navigate_table_cell(id, columns * *direction, window, cx);
                    return;
                }
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
            InputEvent::NavigateHorizontal(direction) => {
                if self
                    .session
                    .document
                    .block(id)
                    .is_some_and(|block| block.kind == BlockKind::TableCell)
                {
                    self.navigate_table_cell(id, *direction, window, cx);
                } else if let Some(index) = self.session.document.index_of(self.session.page.id, id)
                {
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
            InputEvent::Blurred => {}
            InputEvent::Indent => {
                if self
                    .session
                    .document
                    .block(id)
                    .is_some_and(|block| block.kind == BlockKind::TableCell)
                {
                    self.navigate_table_cell(id, 1, window, cx);
                } else {
                    self.session.execute(Command::IndentList { id });
                    self.sync_inputs(window, cx);
                    self.schedule_save(false, window, cx);
                }
            }
            InputEvent::Outdent => {
                if self
                    .session
                    .document
                    .block(id)
                    .is_some_and(|block| block.kind == BlockKind::TableCell)
                {
                    self.navigate_table_cell(id, -1, window, cx);
                } else {
                    self.session.execute(Command::OutdentList { id });
                    self.sync_inputs(window, cx);
                    self.schedule_save(false, window, cx);
                }
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
                if self.find_open {
                    self.close_find(window, cx);
                }
                let slash_closed = self.slash_menu.take().is_some();
                let mention_closed = self.mention_menu.take().is_some();
                if slash_closed || mention_closed {
                    cx.notify();
                }
            }
            InputEvent::ActivateLink(url) => {
                if let Some(page_id) = url
                    .strip_prefix("quire://page/")
                    .and_then(|id| id.parse::<u64>().ok())
                {
                    self.activate_page(quire_core::core::types::PageId(page_id), window, cx);
                } else if url.starts_with("https://")
                    || url.starts_with("http://")
                    || url.starts_with("mailto:")
                {
                    cx.open_url(url);
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
            if kind == BlockKind::Database {
                self.session.make_database(id, "新数据库".into());
                self.sync_inputs(window, cx);
                self.active = Some(id);
                self.schedule_save(false, window, cx);
                cx.notify();
                return;
            }
            self.session.execute(Command::SetBlockType { id, kind });
            self.sync_inputs(window, cx);
            self.active = Some(self.first_child_editor(id, kind).unwrap_or(id));
            self.restore_focus(window, cx);
            self.schedule_save(false, window, cx);
        }
    }
    fn set_code_lang(
        &mut self,
        id: BlockId,
        lang: Lang,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        self.session.execute(Command::SetCodeLang { id, lang });
        self.sync_inputs(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn set_image_width(
        &mut self,
        id: BlockId,
        percent: u16,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        self.session.execute(Command::SetImageWidth { id, percent });
        self.schedule_save(false, window, cx);
    }
    fn pick_attachment(&mut self, image: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.page.locked || self.closing {
            return;
        }
        let mut picker = rfd::FileDialog::new();
        if image {
            picker = picker.add_filter("图片", &["png", "jpg", "jpeg", "gif", "bmp"]);
        }
        let Some(path) = picker.pick_file() else {
            return;
        };
        let after = self
            .active
            .filter(|id| self.session.blocks().iter().any(|block| block.id == *id))
            .or_else(|| self.session.blocks().last().map(|block| block.id));
        let Some(after) = after else {
            return;
        };
        match self.session.import_attachment(after, &path, image) {
            Ok(id) => {
                self.active = Some(id);
                self.sync_inputs(window, cx);
                self.status = "附件已添加，正在保存…".into();
                self.schedule_save(false, window, cx);
            }
            Err(error) => {
                self.status = format!("导入附件失败：{error}");
                self.error = true;
                cx.notify();
            }
        }
    }
    fn pick_page_cover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.page.locked || self.closing {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title("设置页面封面")
            .add_filter("图片", &["png", "jpg", "jpeg", "gif", "bmp"])
            .pick_file()
        else {
            return;
        };
        match self.session.set_page_cover_from(&path) {
            Ok(()) => {
                self.status = "封面已更新，正在保存…".into();
                self.schedule_save(false, window, cx);
            }
            Err(error) => {
                self.status = format!("设置封面失败：{error}");
                self.error = true;
                cx.notify();
            }
        }
    }
    fn clear_page_cover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session.clear_page_cover();
        self.schedule_save(false, window, cx);
    }
    fn save_attachment(
        &mut self,
        id: quire_core::core::types::AttachmentId,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self.session.attachment_save_name(id) else {
            return;
        };
        let Some(target) = rfd::FileDialog::new().set_file_name(name).save_file() else {
            return;
        };
        match self.session.export_attachment(id, &target) {
            Ok(()) => {
                self.status = format!("已保存附件：{}", target.display());
                self.error = false;
                cx.notify();
            }
            Err(error) => {
                self.status = format!("另存附件失败：{error}");
                self.error = true;
                cx.notify();
            }
        }
    }
    fn export_markdown(&mut self, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        let title = self
            .session
            .page
            .title
            .chars()
            .map(|ch| if r#"<>:\"/|?*"#.contains(ch) { '_' } else { ch })
            .collect::<String>();
        let file_name = format!(
            "{}.md",
            if title.trim().is_empty() {
                "未命名"
            } else {
                title.trim()
            }
        );
        let Some(path) = rfd::FileDialog::new()
            .set_title("导出 Markdown")
            .set_file_name(file_name)
            .add_filter("Markdown", &["md"])
            .save_file()
        else {
            return;
        };
        let markdown = quire_core::services::export_service::export_page(self.session.blocks());
        match std::fs::write(&path, markdown) {
            Ok(()) => {
                self.status = format!("已导出 Markdown：{}", path.display());
                self.error = false;
            }
            Err(error) => {
                self.status = format!("导出 Markdown 失败：{error}");
                self.error = true;
            }
        }
        cx.notify();
    }
    fn import_markdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title("导入 Markdown 为新页面")
            .add_filter("Markdown", &["md", "markdown"])
            .pick_file()
        else {
            return;
        };
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                self.status = format!("读取 Markdown 失败：{error}");
                self.error = true;
                cx.notify();
                return;
            }
        };
        let title = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().trim().to_owned())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| "已导入".into());
        let page = self.session.import_markdown(title.clone(), &source);
        self.session.visit_page(page);
        self.find_index = None;
        self.refresh_find_matches();
        self.active = None;
        self.title_edit_original = None;
        let page_title = self.session.page.title.clone();
        self.title_input.update(cx, |input, cx| {
            let len = page_title.len();
            input.reset(page_title, Vec::new(), len, cx);
        });
        self.sync_inputs(window, cx);
        self.focus_first(window, cx);
        self.status = format!("已导入 Markdown：{title}");
        self.error = false;
        self.schedule_save(false, window, cx);
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
    fn delete_table_row(
        &mut self,
        id: BlockId,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        self.session.execute(Command::TableDeleteRow { id, row });
        self.sync_inputs(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn delete_table_column(
        &mut self,
        id: BlockId,
        col: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        self.session.execute(Command::TableDeleteColumn { id, col });
        self.sync_inputs(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn navigate_table_cell(
        &mut self,
        id: BlockId,
        delta: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(table_id) = self
            .session
            .document
            .block(id)
            .filter(|block| block.kind == BlockKind::TableCell)
            .and_then(|block| block.parent)
        else {
            return;
        };
        let mut cells = self
            .session
            .blocks()
            .iter()
            .filter(|block| block.parent == Some(table_id) && block.kind == BlockKind::TableCell)
            .collect::<Vec<_>>();
        cells.sort_by_key(|block| block.order);
        let Some(index) = cells.iter().position(|block| block.id == id) else {
            return;
        };
        let next_index = index as i64 + delta as i64;
        if next_index >= 0
            && let Some(next) = cells.get(next_index as usize)
        {
            let next_id = next.id;
            let caret = if delta < 0 { next.text.len() } else { 0 };
            self.focus(next_id, caret, window, cx);
        } else if delta > 0 {
            let columns = self
                .session
                .document
                .block(table_id)
                .map_or(1, |table| usize::from(table.columns.max(1)));
            let row = cells.len().div_ceil(columns);
            self.session
                .execute(Command::TableAddRow { id: table_id, row });
            self.sync_inputs(window, cx);
            let target_column = if delta == columns as i32 {
                index % columns
            } else {
                0
            };
            let mut cells = self
                .session
                .blocks()
                .iter()
                .filter(|block| {
                    block.parent == Some(table_id) && block.kind == BlockKind::TableCell
                })
                .collect::<Vec<_>>();
            cells.sort_by_key(|block| block.order);
            if let Some(target) = cells.get(cells.len().saturating_sub(columns) + target_column) {
                let target = target.id;
                self.focus(target, 0, window, cx);
            }
            self.schedule_save(false, window, cx);
        }
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
        if kind == BlockKind::Database {
            self.session.replace(id, String::new(), false);
            self.session.make_database(id, "新数据库".into());
        } else {
            self.session.convert_prefix(id, String::new(), kind);
        }
        self.sync_inputs(window, cx);
        self.active = Some(self.first_child_editor(id, kind).unwrap_or(id));
        self.restore_focus(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn mention_matches(&self) -> Vec<quire_core::core::types::Page> {
        let Some((_, _, query)) = &self.mention_menu else {
            return Vec::new();
        };
        self.session
            .pages
            .iter()
            .filter(|page| {
                !page.template && (query.is_empty() || page.title.to_lowercase().contains(query))
            })
            .take(20)
            .cloned()
            .collect()
    }
    fn select_mention(
        &mut self,
        id: BlockId,
        at: usize,
        page: quire_core::core::types::Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.composing(cx) || self.session.page.locked {
            return;
        }
        self.mention_menu = None;
        let caret = at + page.title.len();
        self.session.execute(Command::InsertReference {
            id,
            at,
            label: page.title,
            kind: quire_core::core::types::MarkKind::Mention,
            url: quire_core::core::page_uri(page.id),
            date: None,
        });
        self.sync_inputs(window, cx);
        self.focus(id, caret, window, cx);
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
    fn duplicate_block(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing || self.session.page.locked || self.composing(cx) {
            return;
        }
        let Some(kind) = self.session.document.block(id).map(|block| block.kind) else {
            return;
        };
        let duplicate = self.session.duplicate_block(id);
        self.sync_inputs(window, cx);
        if let Some(duplicate) = duplicate {
            self.active = Some(
                self.first_child_editor(duplicate, kind)
                    .unwrap_or(duplicate),
            );
            self.restore_focus(window, cx);
        }
        self.status = "块副本已创建，正在保存…".into();
        self.schedule_save(false, window, cx);
    }
    fn move_block(&mut self, id: BlockId, delta: i32, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing || self.session.page.locked {
            return;
        }
        let caret = self
            .inputs
            .get(&id.0)
            .map(|input| input.read(cx).buffer.caret());
        self.session.execute(Command::MoveBlock { id, delta });
        self.sync_inputs(window, cx);
        if let Some(caret) = caret {
            self.focus(id, caret, window, cx);
        }
        self.schedule_save(false, window, cx);
    }
    fn toggle_color_picker(&mut self, id: BlockId, background: bool, cx: &mut Context<Self>) {
        self.color_picker = if self.color_picker == Some((id, background)) {
            None
        } else {
            Some((id, background))
        };
        cx.notify();
    }
    fn set_block_color(
        &mut self,
        id: BlockId,
        background: bool,
        slot: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        let Some(block) = self.session.blocks().iter().find(|block| block.id == id) else {
            return;
        };
        let color = if background {
            block.color
        } else {
            ColorKind::from_slot(slot).unwrap_or(ColorKind::Default)
        };
        let bg = if background {
            ColorKind::from_slot(slot).unwrap_or(ColorKind::Default)
        } else {
            block.background
        };
        self.session.execute(Command::SetBlockColor {
            id,
            color,
            background: bg,
        });
        self.color_picker = None;
        self.sync_inputs(window, cx);
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
    fn toggle_link_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.link_editor_open = !self.link_editor_open;
        if self.link_editor_open {
            self.link_target_input.update(cx, |input, input_cx| {
                input.reset(String::new(), Vec::new(), 0, input_cx)
            });
            let input = self.link_target_input.clone();
            input.update(cx, |input, _| window.focus(&input.focus));
        }
        cx.notify();
    }
    fn apply_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            self.status = "先选中要添加链接的文字".into();
            cx.notify();
            return;
        }
        let url = self
            .link_target_input
            .read(cx)
            .buffer
            .committed()
            .trim()
            .to_owned();
        if !(url.starts_with("https://")
            || url.starts_with("http://")
            || url.starts_with("mailto:")
            || url.starts_with("quire://page/"))
        {
            self.status = "链接需使用 http、https、mailto 或 quire://page/ID".into();
            cx.notify();
            return;
        }
        self.session
            .set_link(id, selection.start, selection.end, url);
        self.link_editor_open = false;
        self.sync_inputs(window, cx);
        self.focus(id, selection.end, window, cx);
        self.schedule_save(false, window, cx);
    }
    fn select_theme(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.transition_palette = Some(self.palette.clone());
        self.palette = Palette::load(id);
        self.theme_choice = id.to_owned();
        let fg = Palette::color(&self.palette.fg);
        for input in [
            self.title_input.clone(),
            self.search_input.clone(),
            self.find_input.clone(),
            self.link_target_input.clone(),
            self.note_title_input.clone(),
            self.note_body_input.clone(),
            self.task_title_input.clone(),
            self.task_due_input.clone(),
            self.organizer_tags_input.clone(),
            self.subtask_title_input.clone(),
            self.database_cell_input.clone(),
        ] {
            input.update(cx, |input, _| input.text_color = Some(fg));
        }
        self.sync_inputs(window, cx);
        self.theme_picker_open = false;
        self.search_query.clear();
        self.search_input.update(cx, |input, cx| {
            input.placeholder = Some("搜索页面…".into());
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.session
            .set_setting("gpui.theme", self.theme_choice.clone());
        self.schedule_save(true, window, cx);
    }
    fn set_ui_zoom(&mut self, zoom: f32, window: &mut Window, cx: &mut Context<Self>) {
        let zoom = zoom.clamp(0.5, 3.0);
        if (zoom - self.ui_zoom).abs() < f32::EPSILON {
            return;
        }
        self.ui_zoom = zoom;
        quire_gpui::ui_scale::set(zoom);
        self.session
            .set_setting("gpui.ui-scale", format!("{zoom:.2}"));
        let mut inputs = vec![
            self.title_input.clone(),
            self.search_input.clone(),
            self.find_input.clone(),
            self.link_target_input.clone(),
            self.note_title_input.clone(),
            self.note_body_input.clone(),
            self.task_title_input.clone(),
            self.task_due_input.clone(),
            self.organizer_tags_input.clone(),
            self.subtask_title_input.clone(),
            self.database_cell_input.clone(),
        ];
        inputs.extend(self.inputs.values().cloned());
        for input in inputs {
            input.update(cx, |_, cx| cx.notify());
        }
        self.schedule_save(true, window, cx);
        cx.notify();
    }
    fn begin_database_cell_edit(
        &mut self,
        block: u64,
        db: u64,
        record: u64,
        property: u64,
        mut value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        if self
            .session
            .databases
            .properties_of(quire_core::core::database::DatabaseId(db))
            .find(|definition| definition.id.0 == property)
            .is_some_and(|definition| {
                !matches!(
                    definition.kind,
                    quire_core::core::database::PropertyKind::Select
                        | quire_core::core::database::PropertyKind::Status
                )
            })
        {
            if let Ok(stored) = self.session.repo.cell(
                quire_core::core::database::RecordId(record),
                quire_core::core::database::PropertyId(property),
            ) {
                value = stored.display();
            }
        }
        self.active_database_cell = Some((block, db, record, property));
        let caret = value.len();
        self.database_cell_input
            .update(cx, |input, cx| input.reset(value, Vec::new(), caret, cx));
        let input = self.database_cell_input.read(cx);
        window.focus(&input.focus);
        cx.notify();
    }
    fn commit_database_cell(
        &mut self,
        keep_editing_on_error: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((block, _db, record, property)) = self.active_database_cell else {
            return;
        };
        let value = self
            .database_cell_input
            .read(cx)
            .buffer
            .committed()
            .to_owned();
        match self.session.set_database_cell(
            BlockId(block),
            quire_core::core::database::RecordId(record),
            quire_core::core::database::PropertyId(property),
            &value,
        ) {
            Ok(_) => {
                self.active_database_cell = None;
                self.schedule_save(true, window, cx);
            }
            Err(error) => {
                self.status = error.to_string();
                self.error = true;
                if !keep_editing_on_error {
                    self.active_database_cell = None;
                }
                cx.notify();
            }
        }
    }
    fn zoom_in(&mut self, _: &ZoomIn, window: &mut Window, cx: &mut Context<Self>) {
        self.set_ui_zoom(quire_gpui::ui_scale::step(self.ui_zoom, 1), window, cx);
    }
    fn zoom_out(&mut self, _: &ZoomOut, window: &mut Window, cx: &mut Context<Self>) {
        self.set_ui_zoom(quire_gpui::ui_scale::step(self.ui_zoom, -1), window, cx);
    }
    fn zoom_reset(&mut self, _: &ZoomReset, window: &mut Window, cx: &mut Context<Self>) {
        self.set_ui_zoom(1., window, cx);
    }
    fn commit_note_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tags = parse_organizer_tags(&self.organizer_tags_draft);
        let changed = if let Some(id) = self.selected_note {
            self.session.update_note_editor(
                id,
                self.note_title_draft.clone(),
                self.note_body_draft.clone(),
                tags,
            )
        } else if let Some(id) = self.selected_task {
            let due = self.task_due_draft.trim();
            if !due.is_empty() && !quire_core::core::date::is_iso_date(due) {
                self.task_due_error = true;
                let previous_due = self
                    .session
                    .organizer
                    .task(id)
                    .and_then(|task| task.due.clone());
                self.session.update_task_editor_tags(
                    id,
                    self.note_title_draft.clone(),
                    self.note_body_draft.clone(),
                    previous_due,
                    tags,
                )
            } else {
                self.task_due_error = false;
                self.session.update_task_editor_tags(
                    id,
                    self.note_title_draft.clone(),
                    self.note_body_draft.clone(),
                    (!due.is_empty()).then(|| due.to_owned()),
                    tags,
                )
            }
        } else {
            false
        };
        if changed {
            self.schedule_save(true, window, cx);
        }
    }
    fn reload_selected_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_note {
            self.selected_note = None;
            if self.session.organizer.note(id).is_some() {
                self.select_note(id, window, cx);
                return;
            }
        } else if let Some(id) = self.selected_task {
            self.selected_task = None;
            if self.session.organizer.task(id).is_some() {
                self.select_task(id, window, cx);
                return;
            }
        } else {
            return;
        }
        self.note_title_draft.clear();
        self.note_body_draft.clear();
        self.note_title_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.note_body_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        cx.notify();
    }
    fn close_selected_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_note.is_none() {
            return;
        }
        self.commit_note_draft(window, cx);
        self.selected_note = None;
        self.note_title_draft.clear();
        self.note_body_draft.clear();
        self.organizer_tags_draft.clear();
        self.note_title_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.note_body_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.organizer_tags_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        cx.notify();
    }
    fn convert_note_to_task(
        &mut self,
        id: quire_core::core::organizer::NoteId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_note_draft(window, cx);
        let Some(note) = self.session.organizer.note(id).cloned() else {
            return;
        };
        let title = if note.title.trim().is_empty() {
            note.body
                .lines()
                .map(|line| {
                    line.trim()
                        .trim_start_matches(['#', '>', '-', '*', '+'])
                        .trim()
                })
                .find(|line| !line.is_empty())
                .unwrap_or("新任务")
                .chars()
                .take(50)
                .collect::<String>()
        } else {
            note.title.trim().chars().take(50).collect::<String>()
        };
        let task_id = self
            .session
            .create_task_in(quire_core::core::organizer::ListId::INBOX, title.clone());
        self.session.update_task_editor_tags(
            task_id,
            title.clone(),
            note.body.clone(),
            None,
            note.tags.clone(),
        );
        self.session.set_note_trashed(id, true);
        self.selected_note = None;
        self.selected_task = Some(task_id);
        self.organizer_tasks = true;
        self.note_title_draft.clear();
        self.note_body_draft.clear();
        self.schedule_save(true, window, cx);
        cx.notify();
    }
    fn select_note(
        &mut self,
        id: quire_core::core::organizer::NoteId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_note == Some(id) && self.selected_task.is_none() {
            return;
        }
        self.commit_note_draft(window, cx);
        let Some(note) = self.session.organizer.note(id).cloned() else {
            return;
        };
        self.selected_task = None;
        self.selected_note = Some(id);
        self.task_due_draft.clear();
        self.organizer_tags_draft = note.tags.join(", ");
        let tags = self.organizer_tags_draft.clone();
        self.organizer_tags_input.update(cx, |input, cx| {
            input.reset(tags.clone(), Vec::new(), tags.len(), cx);
        });
        self.task_due_error = false;
        self.task_due_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.note_title_draft = note.title.clone();
        self.note_body_draft = note.body.clone();
        self.note_title_input.update(cx, |input, cx| {
            let len = note.title.len();
            input.reset(note.title, Vec::new(), len, cx);
        });
        self.note_body_input.update(cx, |input, cx| {
            let len = note.body.len();
            input.reset(note.body, Vec::new(), len, cx);
        });
        cx.notify();
    }
    fn select_task(
        &mut self,
        id: quire_core::core::organizer::TaskId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_task == Some(id) && self.selected_note.is_none() {
            return;
        }
        self.commit_note_draft(window, cx);
        let Some(task) = self.session.organizer.task(id).cloned() else {
            return;
        };
        self.selected_note = None;
        self.selected_task = Some(id);
        self.subtask_title_draft.clear();
        self.subtask_title_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.task_due_draft = task.due.clone().unwrap_or_default();
        self.organizer_tags_draft = task.tags.join(", ");
        let tags = self.organizer_tags_draft.clone();
        self.organizer_tags_input.update(cx, |input, cx| {
            input.reset(tags.clone(), Vec::new(), tags.len(), cx);
        });
        self.task_due_error = false;
        let due_draft = self.task_due_draft.clone();
        let due_len = due_draft.len();
        self.task_due_input.update(cx, |input, cx| {
            input.reset(due_draft, Vec::new(), due_len, cx);
        });
        self.note_title_draft = task.title.clone();
        self.note_body_draft = task.notes.clone();
        self.note_title_input.update(cx, |input, cx| {
            let len = task.title.len();
            input.reset(task.title, Vec::new(), len, cx);
        });
        self.note_body_input.update(cx, |input, cx| {
            let len = task.notes.len();
            input.reset(task.notes, Vec::new(), len, cx);
        });
        cx.notify();
    }
    fn create_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_note_draft(window, cx);
        let id = self.session.create_note();
        self.organizer_open = true;
        self.selected_note = None;
        self.select_note(id, window, cx);
        self.schedule_save(true, window, cx);
    }
    fn cycle_task_priority(
        &mut self,
        id: quire_core::core::organizer::TaskId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_note_draft(window, cx);
        let Some(task) = self.session.organizer.task(id) else {
            return;
        };
        let next = Priority::from_slot((task.priority.slot() + 1) % Priority::ALL.len() as i32);
        if self.session.set_task_priority(id, next) {
            self.schedule_save(true, window, cx);
            cx.notify();
        }
    }
    fn cycle_task_repeat(
        &mut self,
        id: quire_core::core::organizer::TaskId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task) = self.session.organizer.task(id) else {
            return;
        };
        let next = Repeat::from_slot((task.repeat.slot() + 1) % Repeat::ALL.len() as i32);
        if self.session.set_task_repeat(id, next) {
            self.schedule_save(true, window, cx);
            cx.notify();
        }
    }
    fn add_subtask_from_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_subtask.is_some() {
            self.commit_subtask_draft(window, cx);
            return;
        }
        let Some(id) = self.selected_task else { return };
        let title = self.subtask_title_draft.trim().to_owned();
        if title.is_empty() || self.organizer_trash {
            return;
        }
        if self.session.add_subtask(id, title) {
            self.subtask_title_draft.clear();
            self.subtask_title_input.update(cx, |input, cx| {
                input.reset(String::new(), Vec::new(), 0, cx);
            });
            self.schedule_save(true, window, cx);
            cx.notify();
        }
    }
    fn start_edit_subtask(
        &mut self,
        task: quire_core::core::organizer::TaskId,
        subtask: u64,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.organizer_trash {
            return;
        }
        self.commit_note_draft(window, cx);
        self.editing_subtask = Some((task, subtask));
        self.subtask_title_draft = title.clone();
        self.subtask_title_input.update(cx, |input, cx| {
            input.reset(title.clone(), Vec::new(), title.len(), cx);
        });
        window.focus(&self.subtask_title_input.read(cx).focus);
    }
    fn commit_subtask_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((task, subtask)) = self.editing_subtask.take() else {
            return;
        };
        let title = self.subtask_title_draft.trim().to_owned();
        if !title.is_empty() && self.session.set_subtask_title(task, subtask, title) {
            self.schedule_save(true, window, cx);
        }
        self.subtask_title_draft.clear();
        self.subtask_title_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        cx.notify();
    }
    fn toggle_subtask_done(
        &mut self,
        task: quire_core::core::organizer::TaskId,
        subtask: u64,
        done: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.organizer_trash && self.session.set_subtask_done(task, subtask, !done) {
            self.schedule_save(true, window, cx);
            cx.notify();
        }
    }
    fn delete_subtask(
        &mut self,
        task: quire_core::core::organizer::TaskId,
        subtask: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.organizer_trash && self.session.delete_subtask(task, subtask) {
            self.schedule_save(true, window, cx);
            cx.notify();
        }
    }
    fn create_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_note_draft(window, cx);
        let id = self
            .session
            .create_task_in(self.selected_task_list, "新任务".into());
        self.organizer_open = true;
        self.organizer_tasks = true;
        self.selected_note = None;
        self.selected_task = None;
        self.select_task(id, window, cx);
        self.schedule_save(true, window, cx);
    }
    fn create_task_from_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.task_title_draft.trim().to_owned();
        if title.is_empty() || self.organizer_trash {
            return;
        }
        self.commit_note_draft(window, cx);
        let id = self.session.create_task_in(self.selected_task_list, title);
        self.task_title_draft.clear();
        self.task_title_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.selected_task = None;
        self.select_task(id, window, cx);
        self.schedule_save(true, window, cx);
        cx.notify();
    }
    fn toggle_organizer_tab(&mut self, tasks: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.organizer_tasks == tasks {
            let placeholder = if tasks {
                "搜索任务…"
            } else {
                "搜索笔记…"
            };
            self.search_input.update(cx, |input, cx| {
                input.placeholder = Some(placeholder.into());
                input.reset(String::new(), Vec::new(), 0, cx);
            });
            self.search_query.clear();
            return;
        }
        self.commit_note_draft(window, cx);
        self.organizer_tasks = tasks;
        self.search_query.clear();
        self.search_input.update(cx, |input, cx| {
            input.placeholder = Some(
                if tasks {
                    "搜索任务…"
                } else {
                    "搜索笔记…"
                }
                .into(),
            );
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.selected_task_ids.clear();
        self.selected_note = None;
        self.selected_task = None;
        self.reset_organizer_item_inputs(cx);
        cx.notify();
    }
    fn toggle_task_done(
        &mut self,
        id: quire_core::core::organizer::TaskId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task) = self.session.organizer.task(id) else {
            return;
        };
        if self.session.set_task_done(id, !task.done) {
            self.schedule_save(true, window, cx);
        }
    }
    fn select_task_list(
        &mut self,
        list: quire_core::core::organizer::ListId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_task_list == list {
            if self.selected_task_smart_view.is_none() {
                return;
            }
        }
        self.commit_note_draft(window, cx);
        self.selected_task_ids.clear();
        self.selected_task_smart_view = None;
        self.selected_task_list = list;
        self.selected_task = None;
        self.reset_organizer_item_inputs(cx);
    }
    fn select_task_smart_view(&mut self, view: i32, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_task_smart_view == Some(view) {
            return;
        }
        self.commit_note_draft(window, cx);
        self.selected_task_ids.clear();
        self.selected_task_smart_view = Some(view);
        self.selected_task_list = quire_core::core::organizer::ListId::INBOX;
        self.selected_task = None;
        self.reset_organizer_item_inputs(cx);
    }
    fn task_matches_smart_view(&self, task: &OrganizerTask, view: i32) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let today = quire_core::core::date::today_iso();
        let week = quire_core::core::date::to_iso(now.div_euclid(86_400) + 7);
        task_matches_smart_view(&self.session.organizer, task, view, &today, &week)
    }
    fn reset_organizer_item_inputs(&mut self, cx: &mut Context<Self>) {
        self.note_title_draft.clear();
        self.note_body_draft.clear();
        self.task_due_draft.clear();
        self.organizer_tags_draft.clear();
        self.task_due_error = false;
        self.note_title_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.note_body_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.task_due_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.organizer_tags_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        cx.notify();
    }
    fn toggle_show_task_done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_note_draft(window, cx);
        self.show_task_done = !self.show_task_done;
        self.selected_task = None;
        self.selected_task_ids.clear();
        self.reset_organizer_item_inputs(cx);
    }
    fn cycle_task_sort(&mut self, cx: &mut Context<Self>) {
        self.task_sort = (self.task_sort + 1) % 3;
        cx.notify();
    }
    fn toggle_task_multi_select(
        &mut self,
        id: quire_core::core::organizer::TaskId,
        cx: &mut Context<Self>,
    ) {
        if !self.selected_task_ids.remove(&id) {
            self.selected_task_ids.insert(id);
        }
        cx.notify();
    }
    fn bulk_complete_tasks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_task_ids.iter().copied().collect::<Vec<_>>();
        if self.session.set_tasks_done(&ids, true) {
            self.selected_task_ids.clear();
            self.status = format!("已完成 {} 项任务（Ctrl+Z 可撤销）", ids.len());
            self.schedule_save(true, window, cx);
            cx.notify();
        }
    }
    fn bulk_trash_tasks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_task_ids.iter().copied().collect::<Vec<_>>();
        if self.session.set_tasks_trashed(&ids, true) {
            self.selected_task_ids.clear();
            self.selected_task = None;
            self.reset_organizer_item_inputs(cx);
            self.status = format!("已移入回收站 {} 项任务（Ctrl+Z 可撤销）", ids.len());
            self.schedule_save(true, window, cx);
        }
    }
    fn create_task_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let number = self.session.organizer.lists.len() + 1;
        let id = self.session.create_task_list(format!("清单 {number}"));
        self.selected_task_list = id;
        self.selected_task_smart_view = None;
        self.selected_task = None;
        self.schedule_save(true, window, cx);
    }
    fn delete_task_list(
        &mut self,
        list: quire_core::core::organizer::ListId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(count) = self.session.delete_task_list(list) else {
            return;
        };
        if self.selected_task_list == list {
            self.selected_task_list = quire_core::core::organizer::ListId::INBOX;
        }
        self.status = format!("清单已删除，{count} 项任务已移至收集箱（Ctrl+Z 可撤销）");
        self.schedule_save(true, window, cx);
    }
    fn move_task_to_list(
        &mut self,
        task: quire_core::core::organizer::TaskId,
        list: quire_core::core::organizer::ListId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_note_draft(window, cx);
        if self.session.set_task_list(task, list) {
            self.selected_task_list = list;
            self.selected_task_smart_view = None;
            self.schedule_save(true, window, cx);
            cx.notify();
        }
    }
    fn trash_task(
        &mut self,
        id: quire_core::core::organizer::TaskId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_note_draft(window, cx);
        if self.session.set_task_trashed(id, true) {
            if self.selected_task == Some(id) {
                self.selected_task = None;
                self.note_title_draft.clear();
                self.note_body_draft.clear();
                self.note_title_input.update(cx, |input, cx| {
                    input.reset(String::new(), Vec::new(), 0, cx);
                });
                self.note_body_input.update(cx, |input, cx| {
                    input.reset(String::new(), Vec::new(), 0, cx);
                });
            }
            self.schedule_save(true, window, cx);
        }
    }
    fn toggle_note_pin(
        &mut self,
        id: quire_core::core::organizer::NoteId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(note) = self.session.organizer.note(id) else {
            return;
        };
        if self.session.set_note_pinned(id, !note.pinned) {
            self.schedule_save(true, window, cx);
        }
    }
    fn trash_note(
        &mut self,
        id: quire_core::core::organizer::NoteId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_note_draft(window, cx);
        if self.session.set_note_trashed(id, true) {
            if self.selected_note == Some(id) {
                self.selected_note = None;
                self.note_title_draft.clear();
                self.note_body_draft.clear();
                self.note_title_input.update(cx, |input, cx| {
                    input.reset(String::new(), Vec::new(), 0, cx);
                });
                self.note_body_input.update(cx, |input, cx| {
                    input.reset(String::new(), Vec::new(), 0, cx);
                });
            }
            self.schedule_save(true, window, cx);
        }
    }
    fn toggle_organizer_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_note_draft(window, cx);
        self.organizer_trash = !self.organizer_trash;
        self.selected_task_ids.clear();
        self.selected_note = None;
        self.selected_task = None;
        self.reset_organizer_item_inputs(cx);
    }
    fn toggle_organizer_tag(&mut self, tag: String, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_note_draft(window, cx);
        self.organizer_tag = if tag.is_empty() {
            None
        } else {
            (self.organizer_tag.as_deref() != Some(tag.as_str())).then_some(tag)
        };
        self.selected_task_ids.clear();
        self.selected_note = None;
        self.selected_task = None;
        self.reset_organizer_item_inputs(cx);
    }
    fn restore_note(
        &mut self,
        id: quire_core::core::organizer::NoteId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.set_note_trashed(id, false) {
            self.status = "笔记已恢复".into();
            self.schedule_save(true, window, cx);
        }
    }
    fn restore_task(
        &mut self,
        id: quire_core::core::organizer::TaskId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.set_task_trashed(id, false) {
            self.status = "任务已恢复".into();
            self.schedule_save(true, window, cx);
        }
    }
    fn toggle_organizer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_note_draft(window, cx);
        self.organizer_open = !self.organizer_open;
        if self.organizer_open {
            self.search_query.clear();
            let placeholder = if self.organizer_tasks {
                "搜索任务…"
            } else {
                "搜索笔记…"
            };
            self.search_input.update(cx, |input, cx| {
                input.placeholder = Some(placeholder.into());
                input.reset(String::new(), Vec::new(), 0, cx);
            });
        }
        if !self.organizer_open {
            self.restore_focus(window, cx);
        }
        cx.notify();
    }
    fn toggle_theme_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.theme_picker_open {
            self.close_settings(window, cx);
            return;
        }
        self.theme_picker_open = true;
        self.settings_tab = 0;
        self.search_query.clear();
        self.search_input.update(cx, |input, cx| {
            input.placeholder = Some("搜索设置…".into());
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.icon_picker_open = false;
        window.focus(&self.search_input.read(cx).focus);
        cx.notify();
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.theme_picker_open = false;
        self.search_query.clear();
        self.search_selected = 0;
        let placeholder = if self.organizer_open {
            if self.organizer_tasks {
                "搜索任务…"
            } else {
                "搜索笔记…"
            }
        } else {
            "搜索页面…"
        };
        self.search_input.update(cx, |input, cx| {
            input.placeholder = Some(placeholder.into());
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.restore_focus(window, cx);
        cx.notify();
    }
    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        let from = if self.sidebar_open {
            self.sidebar_width
        } else {
            0.
        };
        self.sidebar_open = !self.sidebar_open;
        let to = if self.sidebar_open {
            self.sidebar_width
        } else {
            0.
        };
        self.sidebar_transition_seq = self.sidebar_transition_seq.wrapping_add(1);
        self.sidebar_transition = Some((self.sidebar_transition_seq, from, to));
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
            self.mention_menu = None;
            self.find_index = None;
            self.refresh_find_matches();
            self.active = None;
            self.title_edit_original = None;
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
    fn search_result_ids(&self) -> Vec<quire_core::core::types::PageId> {
        let query = self.search_query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        self.session
            .pages
            .iter()
            .filter(|page| {
                !page.template
                    && (page.title.to_lowercase().contains(&query)
                        || self
                            .session
                            .document
                            .page_blocks(page.id)
                            .iter()
                            .any(|block| block.text.to_lowercase().contains(&query)))
            })
            .take(8)
            .map(|page| page.id)
            .collect()
    }
    fn open_search(&mut self, _: &SearchPages, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = true;
        self.search_query.clear();
        self.search_selected = 0;
        self.search_input.update(cx, |input, cx| {
            input.placeholder = Some("搜索页面…".into());
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        cx.notify();
        window.focus(&self.search_input.read(cx).focus);
    }
    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = false;
        self.search_query.clear();
        self.search_selected = 0;
        let placeholder = if self.theme_picker_open {
            "搜索设置…"
        } else if self.organizer_open {
            if self.organizer_tasks {
                "搜索任务…"
            } else {
                "搜索笔记…"
            }
        } else {
            "搜索页面…"
        };
        self.search_input.update(cx, |input, cx| {
            input.placeholder = Some(placeholder.into());
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        if let Some(id) = self.active
            && let Some(input) = self.inputs.get(&id.0)
        {
            window.focus(&input.read(cx).focus);
        }
        cx.notify();
    }
    fn open_selected_search_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.search_result_ids().get(self.search_selected).copied();
        if let Some(page_id) = result {
            self.activate_page(page_id, window, cx);
        }
        self.close_search(window, cx);
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
    fn duplicate_page(
        &mut self,
        id: quire_core::core::types::PageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.closing || self.composing(cx) {
            return;
        }
        let Some(duplicate) = self.session.duplicate_page(id) else {
            return;
        };
        self.session.visit_page(duplicate);
        self.find_index = None;
        self.refresh_find_matches();
        self.active = None;
        self.title_edit_original = None;
        let title = self.session.page.title.clone();
        self.title_input.update(cx, |input, cx| {
            let len = title.len();
            input.reset(title, Vec::new(), len, cx);
        });
        self.sync_inputs(window, cx);
        self.focus_first(window, cx);
        self.status = "页面副本已创建，正在保存…".into();
        self.schedule_save(false, window, cx);
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
            self.find_index = None;
            self.refresh_find_matches();
            self.title_edit_original = None;
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
        self.find_index = None;
        self.refresh_find_matches();
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
        self.find_index = None;
        self.refresh_find_matches();
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
            || self.find_input.read(cx).buffer.composing()
            || self.inputs.values().any(|i| i.read(cx).buffer.composing())
    }
    fn refresh_find_matches(&mut self) {
        self.find_matches = if self.find_query.is_empty() {
            Vec::new()
        } else {
            self.session
                .blocks()
                .iter()
                .filter(|block| self.inputs.contains_key(&block.id.0))
                .flat_map(|block| {
                    find_matches_in(&block.text, &self.find_query)
                        .into_iter()
                        .map(|range| (block.id, range))
                })
                .collect()
        };
    }
    fn open_find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.active.and_then(|id| {
            self.inputs.get(&id.0).and_then(|input| {
                let input = input.read(cx);
                (!input.buffer.selection.is_empty())
                    .then(|| input.buffer.text[input.buffer.selection.clone()].to_owned())
            })
        });
        self.find_open = true;
        if let Some(selected) = selected.filter(|text| !text.is_empty()) {
            self.find_query = selected.clone();
            self.find_input.update(cx, |input, cx| {
                let len = selected.len();
                input.reset(selected, Vec::new(), len, cx);
            });
        }
        self.find_index = None;
        self.refresh_find_matches();
        cx.notify();
        window.focus(&self.find_input.read(cx).focus);
    }
    fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find_open = false;
        if let Some(id) = self.active
            && let Some(input) = self.inputs.get(&id.0)
        {
            let focus = input.read(cx).focus.clone();
            window.focus(&focus);
        }
        cx.notify();
    }
    fn move_find(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find_open {
            return;
        }
        self.refresh_find_matches();
        let count = self.find_matches.len();
        if count == 0 {
            self.find_index = None;
            cx.notify();
            return;
        }
        let target = self.find_index.map_or_else(
            || if step > 0 { 0 } else { count - 1 },
            |index| (index as isize + step).rem_euclid(count as isize) as usize,
        );
        self.find_index = Some(target);
        let (id, range) = self.find_matches[target].clone();
        if let Some(input) = self.inputs.get(&id.0) {
            let focus = input.read(cx).focus.clone();
            input.update(cx, |input, cx| {
                input.buffer.selection = range;
                input.buffer.reversed = false;
                cx.notify();
            });
            self.active = Some(id);
            window.focus(&focus);
        }
        cx.notify();
    }
    fn find_next_action(&mut self, _: &FindNext, window: &mut Window, cx: &mut Context<Self>) {
        self.move_find(1, window, cx);
    }
    fn find_previous_action(
        &mut self,
        _: &FindPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_find(-1, window, cx);
    }
    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_note_draft(window, cx);
        self.session.end_typing();
        self.schedule_save(true, window, cx);
    }
    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if self.composing(cx) {
            return;
        }
        if self.organizer_open {
            self.commit_note_draft(window, cx);
            if self.session.undo_organizer() {
                self.reload_selected_note(window, cx);
            }
            self.schedule_save(false, window, cx);
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
        if self.organizer_open {
            self.commit_note_draft(window, cx);
            if self.session.redo_organizer() {
                self.reload_selected_note(window, cx);
            }
            self.schedule_save(false, window, cx);
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
        if !window.is_maximized() && !window.is_fullscreen() {
            self.normal_window_bounds = window.bounds();
        }
        let bounds = self.normal_window_bounds;
        for (key, value) in [
            ("window.x", f32::from(bounds.origin.x)),
            ("window.y", f32::from(bounds.origin.y)),
            ("window.w", f32::from(bounds.size.width)),
            ("window.h", f32::from(bounds.size.height)),
        ] {
            self.session.set_setting(key, value.round().to_string());
        }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar_transition = self.sidebar_transition;
        let p = self.palette.clone();
        let database_width = (window.bounds().size.width - px(356.))
            .min(px(760.))
            .max(px(200.));
        let text = Palette::color(&p.fg);
        let muted = Palette::color(&p.muted);
        let accent = Palette::color(&p.accent);
        let backlinks_count = self
            .session
            .repo
            .reference_count(self.session.page.id)
            .unwrap_or(0);
        let backlink_rows = if self.backlinks_open {
            self.session
                .repo
                .references(self.session.page.id, 500)
                .unwrap_or_default()
                .into_iter()
                .map(|reference| {
                    let page_id = reference.page;
                    let block_id = reference.block;
                    let page_title = self
                        .session
                        .pages
                        .iter()
                        .find(|page| page.id == page_id)
                        .map(|page| page.title.clone())
                        .unwrap_or_else(|| "已删除页面".into());
                    let preview = if reference.text.trim().is_empty() {
                        "页面引用".to_owned()
                    } else {
                        reference.text
                    };
                    div()
                        .id(SharedString::from(format!(
                            "backlink-{page_id:?}-{block_id:?}"
                        )))
                        .w_full()
                        .px(px(9.))
                        .py(px(6.))
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.28)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.activate_page(page_id, window, cx);
                            this.focus(block_id, 0, window, cx);
                            this.backlinks_open = false;
                            cx.notify();
                        }))
                        .child(div().text_size(px(11.)).text_color(muted).child(page_title))
                        .child(div().text_color(text).child(preview))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let backlinks_empty = backlink_rows.is_empty();
        let sync_sources = self
            .session
            .pages
            .iter()
            .filter(|page| !page.template)
            .flat_map(|page| {
                let page_id = page.id;
                let title = page.title.clone();
                self.session
                    .document
                    .page_blocks(page_id)
                    .iter()
                    .filter(|block| {
                        matches!(
                            block.kind,
                            BlockKind::Paragraph
                                | BlockKind::Heading1
                                | BlockKind::Heading2
                                | BlockKind::Heading3
                                | BlockKind::Bullet
                                | BlockKind::Numbered
                                | BlockKind::Todo
                                | BlockKind::Quote
                                | BlockKind::Code
                                | BlockKind::Callout
                                | BlockKind::Toggle
                        ) && !block.text.trim().is_empty()
                    })
                    .map(move |block| (page_id, title.clone(), block.id, block.text.clone()))
            })
            .take(20)
            .collect::<Vec<_>>();
        let page_candidates = self
            .session
            .pages
            .iter()
            .filter(|page| page.id != self.session.page.id && !page.template)
            .map(|page| (page.id, page.title.clone()))
            .collect::<Vec<_>>();
        let cover_path = self
            .session
            .page
            .cover
            .and_then(|cover| self.session.attachment_display_path(cover));
        let theme_options = if self.theme_picker_open {
            let palettes: Vec<Palette> =
                serde_json::from_str(include_str!("palettes.json")).expect("checked-in palettes");
            let mut system_palette = Palette::load("system");
            system_palette.id = "system".into();
            let palettes = std::iter::once(system_palette)
                .chain(palettes)
                .collect::<Vec<_>>();
            palettes
                .into_iter()
                .filter(|palette| {
                    let query = self.search_query.trim().to_lowercase();
                    query.is_empty()
                        || palette.id.to_lowercase().contains(&query)
                        || match palette.id.as_str() {
                            "system" => "跟随系统".contains(&query),
                            "midnight" => "暗夜蓝".contains(&query),
                            "graphite" => "石墨灰".contains(&query),
                            "violet" => "紫罗兰".contains(&query),
                            "emerald" => "森林绿".contains(&query),
                            "amber" => "琥珀暖".contains(&query),
                            "ocean" => "海洋青".contains(&query),
                            "rose" => "珊瑚红".contains(&query),
                            "light" => "明亮".contains(&query),
                            "jade" => "翡翠绿".contains(&query),
                            "deepblue" => "深空蓝".contains(&query),
                            "twilight" => "暮光紫".contains(&query),
                            "crimson" => "荣艳红".contains(&query),
                            _ => false,
                        }
                })
                .map(|palette| {
                    let id = palette.id.clone();
                    let label = match id.as_str() {
                        "system" => "跟随系统",
                        "midnight" => "🌙 暗夜蓝",
                        "graphite" => "🪨 石墨灰",
                        "violet" => "💜 紫罗兰",
                        "emerald" => "🌲 森林绿",
                        "amber" => "🔥 琥珀暖",
                        "ocean" => "🌊 海洋青",
                        "rose" => "🌹 珊瑚红",
                        "light" => "☀️ 明亮",
                        "jade" => "💎 翡翠绿",
                        "deepblue" => "🌌 深空蓝",
                        "twilight" => "🌆 暮光紫",
                        "crimson" => "🌺 荣艳红",
                        _ => id.as_str(),
                    }
                    .to_owned();
                    let selected = id == self.theme_choice;
                    div()
                        .id(SharedString::from(format!("theme-option-{id}")))
                        .w(px(110.))
                        .h(px(62.))
                        .p(px(7.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .border_1()
                        .when(selected, |card| card.border_2())
                        .border_color(if selected {
                            accent
                        } else {
                            Palette::color(&p.border)
                        })
                        .cursor_pointer()
                        .bg(linear_gradient(
                            135.,
                            linear_color_stop(Palette::color(&palette.bg), 0.),
                            linear_color_stop(Palette::color(&palette.grad2), 1.),
                        )
                        .color_space(ColorSpace::Srgb))
                        .hover(|row| row.border_color(accent))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_theme(&id, window, cx)
                        }))
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(Palette::color(&palette.fg))
                                .child(format!("{}{}", label, if selected { "  ✓" } else { "" })),
                        )
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let settings_overlay = self.theme_picker_open.then(|| {
            div()
                .id("settings-overlay")
                .absolute()
                .top(px(0.))
                .right(px(0.))
                .bottom(px(0.))
                .left(px(0.))
                .flex()
                .items_center()
                .justify_center()
                .bg(Hsla::from(rgb(0x000000)).opacity(0x59 as f32 / 255.))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_settings(window, cx)
                }))
                .child(
                    div()
                        .id("settings-dialog")
                        .w(px(560.))
                        .max_w(window.bounds().size.width - px(24.))
                        .h(px(612.))
                        .max_h(window.bounds().size.height - px(24.))
                        .overflow_y_scroll()
                        .p(px(16.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(Palette::color(&p.border))
                        .bg(Palette::color(&p.elevated2))
                        .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                        .shadow(vec![BoxShadow {
                            color: Hsla::from(rgb(0x000000)).opacity(0xa0 as f32 / 255.),
                            offset: point(px(2.), px(3.)),
                            blur_radius: px(0.),
                            spread_radius: px(-2.),
                        }])
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(
                            div()
                                .text_size(px(18.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("设置"),
                        )
                        .child(
                            div()
                                .id("settings-search")
                                .w_full()
                                .h(px(32.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .rounded(px(5.))
                                .border_1()
                                .border_color(Palette::color(&p.border))
                                .bg(Palette::color(&p.bg).opacity(0.25))
                                .child(sidebar_glyph(SidebarGlyph::Search, muted, px(14.)))
                                .child(div().flex_1().min_w_0().child(self.search_input.clone())),
                        )
                        .child(
                            div()
                                .id("settings-tabs")
                                .w_full()
                                .h(px(32.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .rounded(px(5.))
                                .border_1()
                                .border_color(Palette::color(&p.border))
                                .children(
                                    ["外观", "通用", "同步", "MCP", "快捷键", "关于"]
                                        .into_iter()
                                        .enumerate()
                                        .map(|(index, tab)| {
                                            div()
                                                .id(SharedString::from(format!("settings-tab-{index}")))
                                                .flex_1()
                                                .py(px(6.))
                                                .cursor_pointer()
                                                .text_size(px(12.))
                                                .text_center()
                                                .text_color(if self.settings_tab == index {
                                                    text
                                                } else {
                                                    muted
                                                })
                                                .when(self.settings_tab == index, |item| {
                                                    item.bg(Palette::color(&p.bg).opacity(0.4))
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.settings_tab = index;
                                                    cx.notify();
                                                }))
                                                .child(tab)
                                        }),
                                ),
                        )
                        .child(div().h(px(1.)).bg(Palette::color(&p.border)))
                        .when(self.settings_tab == 0, |dialog| dialog.child(
                            div()
                                .text_size(px(12.))
                                .text_color(muted)
                                .child("外观 · 渐变主题"),
                        ).child(
                            div()
                                .id("settings-theme-grid")
                                .w_full()
                                .flex()
                                .flex_wrap()
                                .gap(px(8.))
                                .children(theme_options),
                        ).child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child("界面缩放")
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(12.))
                                        .child(
                                            div()
                                                .id("settings-zoom-out")
                                                .w(px(22.))
                                                .h(px(24.))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor_pointer()
                                                .text_color(muted)
                                                .hover(|button| button.text_color(text))
                                                .when(self.ui_zoom > 0.5, |button| {
                                                    button.on_click(cx.listener(|this, _, window, cx| {
                                                        this.set_ui_zoom(quire_gpui::ui_scale::step(this.ui_zoom, -1), window, cx)
                                                    }))
                                                })
                                                .child("−"),
                                        )
                                        .child(format!("{:.0}%", self.ui_zoom * 100.))
                                        .child(
                                            div()
                                                .id("settings-zoom-in")
                                                .w(px(22.))
                                                .h(px(24.))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor_pointer()
                                                .text_color(muted)
                                                .hover(|button| button.text_color(text))
                                                .when(self.ui_zoom < 3.0, |button| {
                                                    button.on_click(cx.listener(|this, _, window, cx| {
                                                        this.set_ui_zoom(quire_gpui::ui_scale::step(this.ui_zoom, 1), window, cx)
                                                    }))
                                                })
                                                .child("＋"),
                                        )
                                        .child(
                                            div()
                                                .id("settings-zoom-reset")
                                                .px(px(10.))
                                                .py(px(5.))
                                                .rounded(px(4.))
                                                .border_1()
                                                .border_color(Palette::color(&p.border))
                                                .cursor_pointer()
                                                .text_color(muted)
                                                .hover(|button| button.text_color(text))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.set_ui_zoom(1., window, cx)
                                                }))
                                                .child("复位"),
                                        ),
                                ),
                        ).child(
                            div()
                                .text_size(px(10.))
                                .text_color(muted)
                                .child("整个界面一起放大或缩小 · Ctrl+= / Ctrl+- 步进，Ctrl+0 复位"),
                        ))
                        .when(self.settings_tab == 1, |dialog| dialog.child(
                            div().flex().flex_col().gap(px(14.)).child(
                                div().text_size(px(12.)).text_color(muted).child("通用 · 当前工作区"),
                            ).child(
                                div().flex().justify_between().items_center().child("启动时自动弹出新建笔记输入框").child("○"),
                            ).child(
                                div().flex().justify_between().items_center().child("全局 Alt+N 速记（不打开主界面）").child("○"),
                            ).child(
                                div().flex().justify_between().items_center().child("全局 Alt+M 显示主界面").child("○"),
                            ).child(
                                div().text_size(px(11.)).text_color(muted).child("快捷键、同步服务和数据目录选项将在对应模块接入后启用。"),
                            ),
                        ))
                        .when(self.settings_tab == 2, |dialog| dialog.child(
                            div().flex().flex_col().gap(px(12.)).child(
                                div().flex().items_center().justify_between().child(div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).child("局域网同步")).child(div().text_size(px(11.)).text_color(muted).child("GPUI 版本尚未连接同步服务")),
                            ).child(
                                div().h(px(1.)).bg(Palette::color(&p.border)),
                            ).child(
                                div().text_size(px(12.)).text_color(muted).child("本机设备、在线状态、同步进度与日志将在同步传输层接入后显示。"),
                            ).child(
                                div().flex().gap(px(8.)).children(["设备", "统计", "日志"].into_iter().map(|label| div().px(px(10.)).py(px(6.)).rounded(px(5.)).border_1().border_color(Palette::color(&p.border)).text_size(px(12.)).child(label))),
                            ).child(
                                div().flex_1().flex().items_center().justify_center().text_color(muted).child("尚无同步会话"),
                            ),
                        ))
                        .when(self.settings_tab == 3, |dialog| dialog.child(
                            div().flex().flex_col().gap(px(12.)).child(
                                div().flex().items_center().gap(px(10.)).child(div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).child("MCP")).child(div().text_size(px(11.)).text_color(muted).child("尚未启用")),
                            ).child(
                                div().h(px(1.)).bg(Palette::color(&p.border)),
                            ).child(
                                div().font_weight(FontWeight::SEMIBOLD).text_size(px(12.)).child("客户端配置"),
                            ).child(
                                div().text_size(px(12.)).text_color(muted).child("GPUI 版本暂未启动 MCP 服务。服务接入后可在此查看客户端配置、权限说明和调用记录。"),
                            ).child(
                                div().h(px(1.)).bg(Palette::color(&p.border)),
                            ).child(
                                div().font_weight(FontWeight::SEMIBOLD).text_size(px(12.)).child("调用日志 · 0 条记录"),
                            ).child(
                                div().flex_1().flex().items_center().justify_center().text_color(muted).child("尚无调用记录"),
                            ),
                        ))
                        .when(self.settings_tab == 4, |dialog| dialog.child(
                            div().flex().flex_col().gap(px(12.)).child(
                                div().text_size(px(12.)).text_color(muted).child("快捷键 · 编辑器"),
                            ).children([
                                ("保存页面", "Ctrl+S"),
                                ("撤销 / 重做", "Ctrl+Z / Ctrl+Shift+Z"),
                                ("查找页面", "Ctrl+P"),
                                ("页面内查找", "Ctrl+F"),
                                ("界面缩放 / 复位", "Ctrl+= / Ctrl+- / Ctrl+0"),
                                ("命令面板", "Ctrl+K"),
                                ("新建块", "Enter"),
                            ].into_iter().map(|(label, key)| div().flex().justify_between().items_center().border_b_1().border_color(Palette::color(&p.border)).py(px(7.)).child(label).child(div().px(px(7.)).py(px(3.)).rounded(px(4.)).bg(Palette::color(&p.bg).opacity(0.3)).text_size(px(11.)).child(key)))),
                        ))
                        .when(self.settings_tab == 5, |dialog| dialog.child(
                            div().flex().flex_col().gap(px(12.)).child(
                                div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).child("Quire GPUI"),
                            ).child(
                                div().text_size(px(12.)).text_color(muted).child(format!("版本 {}", env!("CARGO_PKG_VERSION"))),
                            ).child(
                                div().text_size(px(12.)).text_color(muted).child("界面：GPUI · 数据：本地 SQLite"),
                            ).child(
                                div().text_size(px(12.)).text_color(muted).child("使用独立于 Slint 版的 Quire GPUI 数据目录。"),
                            ),
                        ))
                        .child(div().flex_1())
                        .child(
                            div().flex().justify_end().child(
                                div()
                                    .id("settings-done")
                                    .px(px(14.))
                                    .py(px(7.))
                                    .rounded(px(5.))
                                    .border_1()
                                    .border_color(Palette::color(&p.border))
                                    .cursor_pointer()
                                    .hover(|button| button.bg(Palette::color(&p.bg).opacity(0.35)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_settings(window, cx);
                                    }))
                                    .child("完成"),
                            ),
                        ),
                )
        });
        let icon_options = if self.icon_picker_open {
            div()
                .id("page-icon-popover")
                .w(px(224.))
                .p(px(7.))
                .rounded(px(8.))
                .border_1()
                .border_color(Palette::color(&p.border))
                .bg(Palette::color(&p.elevated))
                .child(
                    div()
                        .id("page-icon-grid")
                        .w_full()
                        .max_h(px(228.))
                        .overflow_y_scroll()
                        .flex()
                        .flex_wrap()
                        .gap(px(3.))
                        .children(
                            std::iter::once("")
                                .chain(quire_core::core::icon::PICKER.iter().copied())
                                .map(|icon| {
                                    let selected = self.session.page.icon == icon;
                                    div()
                                        .id(SharedString::from(format!("page-icon-{icon}")))
                                        .size(px(25.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(5.))
                                        .cursor_pointer()
                                        .when(selected, |row| {
                                            row.bg(Palette::color(&p.bg).opacity(0.35))
                                        })
                                        .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.5)))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.select_page_icon(icon, window, cx)
                                        }))
                                        .child(if icon.is_empty() { "×" } else { icon })
                                }),
                        ),
                )
        } else {
            div().id("page-icon-popover")
        };
        let query = self.search_query.trim().to_lowercase();
        let page_rows = ordered_page_tree(&self.session.pages, false)
            .into_iter()
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
                let page_actions = div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "page-actions-format-{}",
                                page_id.0
                            )))
                            .px(px(3.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(accent))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.format_bar_open = !this.format_bar_open;
                                cx.notify();
                            }))
                            .child("Aa"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "page-options-toggle-{}",
                                page_id.0
                            )))
                            .px(px(3.))
                            .cursor_pointer()
                            .text_color(if self.page_options_open {
                                accent
                            } else {
                                muted
                            })
                            .hover(|button| button.text_color(accent))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.page_options_open = !this.page_options_open;
                                cx.notify();
                            }))
                            .child("···"),
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
                            .id(SharedString::from(format!("duplicate-page-{}", page_id.0)))
                            .px(px(3.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.duplicate_page(page_id, window, cx)
                            }))
                            .child("⧉"),
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
                    );
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
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered {
                            if this.hovered_page != Some(page_id) {
                                this.hovered_page = Some(page_id);
                                cx.notify();
                            }
                        } else if this.hovered_page == Some(page_id) {
                            this.hovered_page = None;
                            cx.notify();
                        }
                    }))
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
                    .when(self.hovered_page == Some(page_id), |row| {
                        row.child(page_actions)
                    })
            })
            .collect::<Vec<_>>();
        let favorite_rows = self
            .session
            .pages
            .iter()
            .filter(|page| page.favorite && !page.template)
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
        let search_result_rows = self
            .search_result_ids()
            .into_iter()
            .enumerate()
            .filter_map(|(index, page_id)| {
                self.session
                    .pages
                    .iter()
                    .find(|page| page.id == page_id)
                    .map(|page| {
                        let title = page.title.clone();
                        let preview = self
                            .session
                            .document
                            .page_blocks(page_id)
                            .iter()
                            .find(|block| block.text.to_lowercase().contains(&query))
                            .map(|block| block.text.clone())
                            .unwrap_or_else(|| "页面标题匹配".into());
                        div()
                            .id(SharedString::from(format!("search-result-{}", page_id.0)))
                            .w_full()
                            .min_h(px(42.))
                            .px(px(9.))
                            .py(px(5.))
                            .rounded(px(5.))
                            .flex()
                            .flex_col()
                            .justify_center()
                            .cursor_pointer()
                            .when(index == self.search_selected, |row| {
                                row.bg(Palette::color(&p.bg).opacity(0.45))
                            })
                            .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.45)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.activate_page(page_id, window, cx);
                                this.close_search(window, cx);
                            }))
                            .child(title)
                            .child(div().text_size(px(10.)).text_color(muted).child(preview))
                    })
            })
            .collect::<Vec<_>>();
        let search_overlay = div()
            .id("page-search-overlay")
            .absolute()
            .top(px(96.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("page-search-panel")
                    .w(px(620.))
                    .max_w_full()
                    .p(px(10.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(Palette::color(&p.border))
                    .bg(Palette::color(&p.elevated))
                    .child(
                        div()
                            .h(px(38.))
                            .flex()
                            .items_center()
                            .gap(px(9.))
                            .px(px(4.))
                            .border_b_1()
                            .border_color(Palette::color(&p.border))
                            .text_color(muted)
                            .child("⌕")
                            .child(div().flex_1().min_w_0().child(self.search_input.clone())),
                    )
                    .when(query.is_empty(), |panel| {
                        panel.child(
                            div()
                                .h(px(52.))
                                .px(px(2.))
                                .flex()
                                .items_center()
                                .text_color(muted)
                                .child("输入以搜索标题和内容。"),
                        )
                    })
                    .when(
                        !query.is_empty() && search_result_rows.is_empty(),
                        |panel| {
                            panel.child(
                                div()
                                    .h(px(52.))
                                    .px(px(2.))
                                    .flex()
                                    .items_center()
                                    .text_color(muted)
                                    .child("没有匹配的页面。"),
                            )
                        },
                    )
                    .when(!search_result_rows.is_empty(), |panel| {
                        panel.child(
                            div()
                                .id("search-result-list")
                                .max_h(px(300.))
                                .overflow_y_scroll()
                                .py(px(4.))
                                .flex()
                                .flex_col()
                                .children(search_result_rows),
                        )
                    })
                    .child(
                        div()
                            .h(px(28.))
                            .flex()
                            .items_end()
                            .text_size(px(10.))
                            .text_color(muted)
                            .child("↑↓ 选择 · Enter 打开 · Esc 关闭"),
                    ),
            );
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
                ("数学公式", BlockKind::Math),
                ("链接卡片", BlockKind::Embed),
                ("同步块", BlockKind::Synced),
                ("页面", BlockKind::Page),
                ("页面链接", BlockKind::Link),
                ("提示", BlockKind::Callout),
                ("分隔线", BlockKind::Divider),
                ("表格", BlockKind::Table),
                ("数据库", BlockKind::Database),
                ("分栏", BlockKind::Columns),
                ("目录", BlockKind::Toc),
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
        let attachment_buttons = [("图片", true), ("文件", false)]
            .into_iter()
            .map(|(label, image)| {
                div()
                    .id(if image { "insert-image" } else { "insert-file" })
                    .px(px(8.))
                    .py(px(5.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .text_color(if self.session.page.locked {
                        muted
                    } else {
                        accent
                    })
                    .hover(|button| button.bg(Palette::color(&p.elevated)))
                    .when(!self.session.page.locked, |button| {
                        button.on_click(cx.listener(move |this, _, window, cx| {
                            this.pick_attachment(image, window, cx)
                        }))
                    })
                    .child(label)
            })
            .collect::<Vec<_>>();
        let link_editor = if self.link_editor_open {
            div()
                .id("link-editor")
                .w_full()
                .h(px(34.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(8.))
                .border_b_1()
                .border_color(Palette::color(&p.border))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(self.link_target_input.clone()),
                )
                .child(
                    div()
                        .id("apply-link")
                        .px(px(9.))
                        .py(px(4.))
                        .rounded(px(4.))
                        .cursor_pointer()
                        .text_color(accent)
                        .hover(|button| button.bg(Palette::color(&p.elevated)))
                        .on_click(cx.listener(|this, _, window, cx| this.apply_link(window, cx)))
                        .child("应用链接"),
                )
        } else {
            div().id("link-editor-closed")
        };
        let find_bar = if self.find_open {
            let count = self.find_matches.len();
            let position = self
                .find_index
                .map(|index| format!("{} / {count}", index + 1))
                .unwrap_or_else(|| {
                    if count == 0 {
                        "无结果".into()
                    } else {
                        format!("{count} 个结果")
                    }
                });
            div()
                .id("find-bar")
                .w_full()
                .h(px(38.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(8.))
                .border_b_1()
                .border_color(Palette::color(&p.border))
                .child(div().flex_1().min_w_0().child(self.find_input.clone()))
                .child(div().text_size(px(11.)).text_color(muted).child(position))
                .child(
                    div()
                        .id("find-previous")
                        .px(px(6.))
                        .cursor_pointer()
                        .hover(|button| button.bg(Palette::color(&p.elevated)))
                        .on_click(cx.listener(|this, _, window, cx| this.move_find(-1, window, cx)))
                        .child("↑"),
                )
                .child(
                    div()
                        .id("find-next")
                        .px(px(6.))
                        .cursor_pointer()
                        .hover(|button| button.bg(Palette::color(&p.elevated)))
                        .on_click(cx.listener(|this, _, window, cx| this.move_find(1, window, cx)))
                        .child("↓"),
                )
                .child(
                    div()
                        .id("find-close")
                        .px(px(6.))
                        .cursor_pointer()
                        .text_color(muted)
                        .hover(|button| button.bg(Palette::color(&p.elevated)))
                        .on_click(cx.listener(|this, _, window, cx| this.close_find(window, cx)))
                        .child("×"),
                )
        } else {
            div().id("find-bar-closed")
        };
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
                let database_table = if kind == BlockKind::Database {
                    block.db_ref.and_then(|db_id| {
                        self.session.databases.database(db_id)?;
                        let title = self.session.databases.title_property(db_id)?.id;
                        let columns = self
                            .session
                            .databases
                            .properties_of(db_id)
                            .filter(|property| !property.kind.is_title())
                            .cloned()
                            .collect::<Vec<_>>();
                        let views = self
                            .session
                            .databases
                            .views_of(db_id)
                            .cloned()
                            .collect::<Vec<_>>();
                        let selected_view = self
                            .selected_database_views
                            .get(&db_id.0)
                            .copied()
                            .filter(|view_id| views.iter().any(|view| view.id == *view_id))
                            .or_else(|| views.first().map(|view| view.id));
                        let total = self.session.repo.record_count(db_id).ok()?;
                        Some((
                            db_id,
                            title,
                            columns,
                            views,
                            selected_view,
                            total,
                            self.session.repo.clone(),
                        ))
                    })
                } else {
                    None
                };
                let toc_entries = if kind == BlockKind::Toc {
                    toc_headings(self.session.blocks())
                        .into_iter()
                        .map(|(heading_id, level, label)| {
                            div()
                                .id(SharedString::from(format!(
                                    "toc-entry-{block_id:?}-{heading_id:?}"
                                )))
                                .h(px(25.))
                                .pl(px(6. + (level.saturating_sub(1) as f32 * 14.)))
                                .pr(px(6.))
                                .flex()
                                .items_center()
                                .rounded(px(4.))
                                .cursor_pointer()
                                .text_color(text)
                                .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.34)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.focus(heading_id, 0, window, cx)
                                }))
                                .child(label)
                        })
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let sync_candidates = if kind == BlockKind::Synced && block.sync_ref.is_none() {
                    sync_sources
                        .iter()
                        .filter(|(_, _, source_id, _)| *source_id != block_id)
                        .map(|(_, page_title, source_id, source_text)| {
                            let source_id = *source_id;
                            div()
                                .id(SharedString::from(format!(
                                    "sync-source-{block_id:?}-{source_id:?}"
                                )))
                                .w_full()
                                .px(px(8.))
                                .py(px(5.))
                                .rounded(px(4.))
                                .cursor_pointer()
                                .text_color(muted)
                                .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.3)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if this.session.set_sync_source(block_id, Some(source_id)) {
                                        this.sync_inputs(window, cx);
                                        this.schedule_save(false, window, cx);
                                    }
                                }))
                                .child(format!("{page_title} · {source_text}"))
                        })
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let page_reference_candidates = if matches!(kind, BlockKind::Page | BlockKind::Link)
                    && block.page_ref.is_none()
                {
                    page_candidates
                        .iter()
                        .map(|(target_page, title)| {
                            let target_page = *target_page;
                            div()
                                .id(SharedString::from(format!(
                                    "page-reference-target-{block_id:?}-{target_page:?}"
                                )))
                                .w_full()
                                .px(px(8.))
                                .py(px(5.))
                                .rounded(px(4.))
                                .cursor_pointer()
                                .text_color(muted)
                                .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.3)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if this.session.set_page_reference(block_id, Some(target_page))
                                    {
                                        this.sync_inputs(window, cx);
                                        this.schedule_save(false, window, cx);
                                    }
                                }))
                                .child(title.clone())
                        })
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let image_path = if kind == BlockKind::Image {
                    block
                        .attachment
                        .and_then(|attachment| self.session.attachment_display_path(attachment))
                } else {
                    None
                };
                let image_missing = kind == BlockKind::Image && image_path.is_none();
                let image_width_controls = if kind == BlockKind::Image {
                    [50u16, 75, 100]
                        .into_iter()
                        .map(|percent| {
                            let selected = block.img_percent == percent;
                            div()
                                .id(SharedString::from(format!(
                                    "image-width-{block_id:?}-{percent}"
                                )))
                                .px(px(6.))
                                .py(px(2.))
                                .rounded(px(3.))
                                .cursor_pointer()
                                .text_size(px(10.))
                                .text_color(if selected { accent } else { muted })
                                .when(selected, |chip| chip.bg(Palette::color(&p.bg).opacity(0.4)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.set_image_width(block_id, percent, window, cx)
                                }))
                                .child(format!("{percent}%"))
                        })
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let file_path = if kind == BlockKind::File {
                    block
                        .attachment
                        .and_then(|attachment| self.session.attachment_path(attachment))
                } else {
                    None
                };
                let file_attachment = block.attachment;
                let code_lang_controls = if kind == BlockKind::Code {
                    Lang::ALL
                        .into_iter()
                        .map(|lang| {
                            let selected = block.lang == lang;
                            div()
                                .id(SharedString::from(format!(
                                    "code-lang-{block_id:?}-{}",
                                    lang.as_str()
                                )))
                                .px(px(5.))
                                .py(px(2.))
                                .rounded(px(3.))
                                .cursor_pointer()
                                .text_size(px(10.))
                                .text_color(if selected { accent } else { muted })
                                .when(selected, |chip| chip.bg(Palette::color(&p.bg).opacity(0.4)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.set_code_lang(block_id, lang, window, cx)
                                }))
                                .child(lang.label())
                        })
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                let move_allowed = !matches!(kind, BlockKind::Table | BlockKind::Columns)
                    && !self
                        .session
                        .blocks()
                        .iter()
                        .any(|child| child.parent == Some(block_id));
                let mention_options = if let Some((_, at, _)) = self
                    .mention_menu
                    .clone()
                    .filter(|(mention_id, _, _)| *mention_id == block_id)
                {
                    let matches = self.mention_matches();
                    if matches.is_empty() {
                        vec![
                            div()
                                .id(SharedString::from(format!("mention-empty-{block_id:?}")))
                                .w_full()
                                .h(px(28.))
                                .px(px(9.))
                                .flex()
                                .items_center()
                                .text_color(muted)
                                .child("没有匹配页面"),
                        ]
                    } else {
                        matches
                            .into_iter()
                            .enumerate()
                            .map(|(option_index, page)| {
                                let page_id = page.id;
                                let title = page.title.clone();
                                div()
                                    .id(SharedString::from(format!(
                                        "mention-option-{block_id:?}-{page_id:?}"
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
                                    .when(option_index == self.mention_selection, |option| {
                                        option.bg(Palette::color(&p.elevated))
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select_mention(block_id, at, page.clone(), window, cx)
                                    }))
                                    .child(title)
                                    .child(div().text_size(px(10.)).text_color(muted).child("页面"))
                            })
                            .collect::<Vec<_>>()
                    }
                } else {
                    Vec::new()
                };
                let slash_query = block.text.strip_prefix('/').unwrap_or("").to_lowercase();
                let slash_options = if self.slash_menu == Some(block_id) {
                    slash_items()
                        .iter()
                        .filter(|(label, keywords, _)| {
                            slash_query.is_empty()
                                || label.to_lowercase().contains(&slash_query)
                                || keywords.contains(slash_query.as_str())
                        })
                        .enumerate()
                        .map(|(option_index, (label, keywords, item_kind))| {
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
                                .when(option_index == self.slash_selection, |option| {
                                    option.bg(Palette::color(&p.elevated))
                                })
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
                        div()
                            .id(SharedString::from(format!(
                                "table-delete-row-{}",
                                block_id.0
                            )))
                            .px(px(7.))
                            .py(px(3.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(accent))
                            .when(table_row_count > 1, |button| {
                                button.on_click(cx.listener(move |this, _, window, cx| {
                                    this.delete_table_row(block_id, table_row_count - 1, window, cx)
                                }))
                            })
                            .child("− 行"),
                        div()
                            .id(SharedString::from(format!(
                                "table-delete-column-{}",
                                block_id.0
                            )))
                            .px(px(7.))
                            .py(px(3.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(accent))
                            .when(table_columns > 1, |button| {
                                button.on_click(cx.listener(move |this, _, window, cx| {
                                    this.delete_table_column(
                                        block_id,
                                        table_columns - 1,
                                        window,
                                        cx,
                                    )
                                }))
                            })
                            .child("− 列"),
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
                let color_picker_view =
                    self.color_picker
                        .filter(|(id, _)| *id == block_id)
                        .map(|(_, background)| {
                            div()
                                .w_full()
                                .flex()
                                .flex_wrap()
                                .gap(px(5.))
                                .pt(px(5.))
                                .children(ColorKind::ALL.into_iter().enumerate().map(
                                    |(slot, _)| {
                                        let slot = slot as i32;
                                        let swatch = if background {
                                            self.palette.block_bg(slot)
                                        } else {
                                            self.palette.block_text(slot)
                                        };
                                        div()
                                            .id(SharedString::from(format!(
                                                "block-color-{block_id:?}-{background}-{slot}"
                                            )))
                                            .size(px(19.))
                                            .rounded(px(4.))
                                            .border_1()
                                            .border_color(Palette::color(&p.border))
                                            .bg(swatch)
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.set_block_color(
                                                    block_id, background, slot, window, cx,
                                                )
                                            }))
                                    },
                                ))
                        });
                div()
                    .id(("block", block.id.0))
                    .w_full()
                    .flex()
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered {
                            if this.hovered_block != Some(block_id) {
                                this.hovered_block = Some(block_id);
                                cx.notify();
                            }
                        } else if this.hovered_block == Some(block_id) {
                            this.hovered_block = None;
                            cx.notify();
                        }
                    }))
                    .items_start()
                    .gap(px(8.))
                    .py(px(4.))
                    .when(block.parent.is_some(), |row| row.ml(px(18.)))
                    .when(block.background != ColorKind::Default, |row| {
                        row.bg(self.palette.block_bg(block.background.slot()))
                            .rounded(px(4.))
                    })
                    .when(kind == BlockKind::Code, |row| {
                        row.when(block.background == ColorKind::Default, |row| {
                            row.bg(Palette::color(&p.elevated))
                        })
                        .rounded(px(5.))
                        .px(px(8.))
                    })
                    .when(kind == BlockKind::Quote, |row| {
                        row.border_l_2()
                            .border_color(Palette::color(&p.border))
                            .pl(px(12.))
                    })
                    .when(kind == BlockKind::Callout, |row| {
                        row.when(block.background == ColorKind::Default, |row| {
                            row.bg(Palette::color(&p.elevated))
                        })
                        .rounded(px(5.))
                        .px(px(10.))
                    })
                    .when(!prefix.is_empty(), |row| {
                        row.child(
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
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .w_full()
                            .flex()
                            .flex_col()
                            .when(kind == BlockKind::Table, |view| {
                                view.flex_col()
                                    .children(table_rows)
                                    .child(div().flex().gap(px(6.)).children(table_actions))
                            })
                            .when(kind == BlockKind::Database, |view| {
                                if let Some((db_id, title, columns, views, selected_view, total, repo)) =
                                    database_table
                                {
                                    let db_border = Palette::color(&p.border);
                                    let db_hover = Palette::color(&p.bg).opacity(0.22);
                                    let db_checkbox_fg = Palette::color(&p.bg);
                                    let query_columns = columns.clone();
                                    let cell_title_property = title;
                                                            let cell_block = block_id.0;
                                    let header = div()
                                        .w(database_width)
                                        .flex()
                                        .h(px(34.))
                                        .border_b_1()
                                        .border_color(Palette::color(&p.border))
                                        .child(
                                            div()
                                                .w(px(220.))
                                                .flex_1()
                                                .min_w(px(220.))
                                                .px(px(10.))
                                                .flex()
                                                .items_center()
                                                .text_color(muted)
                                                .text_size(px(11.))
                                                .child("名称"),
                                        )
                                        .children(columns.iter().map(|property| {
                                            div()
                                                .w(px(160.))
                                                .flex_1()
                                                .min_w(px(160.))
                                                .px(px(10.))
                                                .flex()
                                                .items_center()
                                                .text_color(muted)
                                                .text_size(px(11.))
                                                .child(property.name.clone())
                                        }));
                                    let rows = uniform_list(
                                        SharedString::from(format!("database-rows-{}", db_id.0)),
                                        total,
                                        cx.processor(
                                        move |_this, range: Range<usize>, _window, _cx| {
                                                let request =
                                                    quire_core::core::database::RowRequest::new(
                                                        db_id,
                                                        title,
                                                        &query_columns,
                                                    );
                                                let slice = quire_core::core::database::RowWindow {
                                                    start: range.start,
                                                    end: range.end,
                                                };
                                                repo.window_rows(&request, slice)
                                                    .unwrap_or_default()
                                                    .into_iter()
                                                    .map(|row| {
                                                    let cells = std::iter::once(row.title)
                                                            .chain(row.cells)
                                                            .enumerate()
                                                        .map(|(index, value)| {
                                                            let property_id = if index == 0 {
                                                                cell_title_property
                                                            } else {
                                                                query_columns[index - 1].id
                                                            };
                                                            let kind = if index == 0 {
                                                                quire_core::core::database::PropertyKind::Title
                                                            } else {
                                                                query_columns[index - 1].kind
                                                            };
                                                            let editable = !_this.session.page.locked
                                                                && !kind.is_computed()
                                                                && !kind.is_derived()
                                                                && !matches!(
                                                                    kind,
                                                                    quire_core::core::database::PropertyKind::MultiSelect
                                                                        | quire_core::core::database::PropertyKind::Files
                                                                        | quire_core::core::database::PropertyKind::Relation
                                                                );
                                                            let cell_key =
                                                                (cell_block, db_id.0, row.record, property_id.0);
                                                            let editing = _this.active_database_cell == Some(cell_key);
                                                            let input = _this.database_cell_input.clone();
                                                            let displayed = if value.is_empty() {
                                                                "—".to_owned()
                                                            } else {
                                                                value.clone()
                                                            };
                                                            let checked = matches!(
                                                                value.as_str(),
                                                                "Yes" | "true" | "True"
                                                            );
                                                            let checkbox = div()
                                                                .w(px(16.))
                                                                .h(px(16.))
                                                                .rounded(px(4.))
                                                                .border_1()
                                                                .border_color(if checked {
                                                                    accent
                                                                } else {
                                                                    muted
                                                                })
                                                                .when(checked, |box_| {
                                                                    box_.bg(accent).flex().items_center().justify_center().child(
                                                                        div()
                                                                            .text_size(px(10.))
                                                                            .text_color(db_checkbox_fg)
                                                                            .child("✓"),
                                                                    )
                                                                });
                                                            let raw_value = value.clone();
                                                            let mut cell = div()
                                                                    .id(SharedString::from(
                                                                        format!(
                                                                            "db-cell-{}-{}-{index}",
                                                                            db_id.0, row.record
                                                                        ),
                                                                    ))
                                                                    .w(px(if index == 0 {
                                                                        220.
                                                                    } else {
                                                                        160.
                                                                    }))
                                                                    .flex_1()
                                                                    .min_w(px(if index == 0 {
                                                                        220.
                                                                    } else {
                                                                        160.
                                                                    }))
                                                                    .h(px(36.))
                                                                    .px(px(10.))
                                                                    .flex()
                                                                    .items_center()
                                                                    .border_r_1()
                                                                    .border_color(db_border)
                                                                    .text_size(px(12.))
                                                                    .text_color(text)
                                                                    .when(editing, |cell| {
                                                                        cell.child(input.clone())
                                                                    })
                                                                    .when(
                                                                        !editing
                                                                            && kind == quire_core::core::database::PropertyKind::Checkbox,
                                                                        |cell| cell.child(checkbox),
                                                                    )
                                                                    .when(
                                                                        !editing
                                                                            && kind != quire_core::core::database::PropertyKind::Checkbox,
                                                                        |cell| {
                                                                        cell.child(displayed.clone())
                                                                        },
                                                                    );
                                                            if editable && !editing {
                                                                cell = cell.cursor_pointer().hover(|cell| cell.bg(db_hover));
                                                                if kind
                                                                    == quire_core::core::database::PropertyKind::Checkbox
                                                                {
                                                                    cell = cell.text_color(accent).on_click(_cx.listener(
                                                                        move |this, _, window, cx| {
                                                                            let next = if checked { "false" } else { "true" };
                                                                            match this.session.set_database_cell(
                                                                                BlockId(cell_block),
                                                                                quire_core::core::database::RecordId(row.record),
                                                                                quire_core::core::database::PropertyId(property_id.0),
                                                                                next,
                                                                            ) {
                                                                                Ok(_) => this.schedule_save(true, window, cx),
                                                                                Err(error) => {
                                                                                    this.status = error.to_string();
                                                                                    this.error = true;
                                                                                    cx.notify();
                                                                                }
                                                                            }
                                                                        },
                                                                    ));
                                                                } else {
                                                                    cell = cell.on_click(_cx.listener(
                                                                        move |this, _, window, cx| {
                                                                            this.begin_database_cell_edit(
                                                                                cell_block,
                                                                                db_id.0,
                                                                                row.record,
                                                                                property_id.0,
                                                                                raw_value.clone(),
                                                                                window,
                                                                                cx,
                                                                            );
                                                                        },
                                                                    ));
                                                                }
                                                            }
                                                            cell
                                                            })
                                                            .collect::<Vec<_>>();
                                                        div()
                                                            .id(SharedString::from(format!(
                                                                "db-row-{}-{}",
                                                                db_id.0, row.record
                                                            )))
                                                            .flex()
                                                            .w_full()
                                                            .h(px(36.))
                                                            .border_b_1()
                                                            .border_color(db_border)
                                                            .children(cells)
                                                    })
                                                    .collect::<Vec<_>>()
                                            },
                                        ),
                                    )
                                    .h(px((total.min(8).max(1) as f32 * 36.).max(40.)))
                                    .w(database_width)
                                    .max_h(px(288.))
                                    .flex_1()
                                    .min_h(px(40.));
                                    view.flex_col()
                                        .w(database_width)
                                        .rounded(px(7.))
                                        .border_1()
                                        .border_color(Palette::color(&p.border))
                                        .bg(Palette::color(&p.elevated))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .px(px(10.))
                                                .py(px(7.))
                                                .border_b_1()
                                                .border_color(Palette::color(&p.border))
                                                .child(
                                                    div()
                                                .flex()
                                                .items_center()
                                                .gap(px(5.))
                                                .children(views.iter().map(|view| {
                                                    let view_id = view.id;
                                                    let selected = Some(view_id) == selected_view;
                                                    div()
                                                        .id(SharedString::from(format!("db-view-{}-{}", db_id.0, view_id.0)))
                                                        .px(px(7.))
                                                        .py(px(3.))
                                                        .rounded(px(4.))
                                                        .cursor_pointer()
                                                        .text_size(px(11.))
                                                        .text_color(if selected { Palette::color(&p.fg) } else { muted })
                                                        .when(selected, |tab| tab.bg(Palette::color(&p.accent).opacity(0.82)))
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.selected_database_views.insert(db_id.0, view_id);
                                                            cx.notify();
                                                        }))
                                                        .child(view.name.clone())
                                                }))
                                                .child(
                                                    div()
                                                        .id(SharedString::from(format!("db-add-view-{}", db_id.0)))
                                                        .px(px(5.))
                                                        .py(px(3.))
                                                        .rounded(px(4.))
                                                        .cursor_pointer()
                                                        .text_color(muted)
                                                        .hover(|button| button.bg(Palette::color(&p.border)))
                                                        .on_click(cx.listener(move |this, _, window, cx| {
                                                            if let Some(view) = this.session.add_database_view(block_id) {
                                                                this.selected_database_views.insert(db_id.0, view);
                                                                this.schedule_save(false, window, cx);
                                                                cx.notify();
                                                            }
                                                        }))
                                                        .child("＋"),
                                                ),
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(8.))
                                                        .text_size(px(11.))
                                                        .text_color(muted)
                                                        .child(format!("{total} 行"))
                                                        .child(div().flex().gap(px(4.)).child(
                                                            div()
                                                                .id(SharedString::from(format!(
                                                                    "db-add-record-{}",
                                                                    block_id.0
                                                                )))
                                                                .px(px(8.))
                                                                .py(px(4.))
                                                                .rounded(px(4.))
                                                                .cursor_pointer()
                                                                .text_color(accent)
                                                                .hover(|button| {
                                                                    button.bg(Palette::color(&p.bg)
                                                                        .opacity(0.3))
                                                                })
                                                                .on_click(cx.listener(
                                                                    move |this, _, window, cx| {
                                                                        if this
                                                                            .session
                                                                            .add_database_record(
                                                                                block_id,
                                                                            )
                                                                            .is_some()
                                                                        {
                                                                            this.schedule_save(
                                                                                false, window, cx,
                                                                            );
                                                                            cx.notify();
                                                                        }
                                                                    },
                                                                ))
                                                                .child("＋ 新建行"),
                                                        ).child(
                                                            div()
                                                                .id(SharedString::from(format!("db-add-property-{}", block_id.0)))
                                                                .px(px(8.))
                                                                .py(px(4.))
                                                                .rounded(px(4.))
                                                                .cursor_pointer()
                                                                .text_color(accent)
                                                                .hover(|button| button.bg(Palette::color(&p.bg).opacity(0.3)))
                                                                .on_click(cx.listener(move |this, _, window, cx| {
                                                                    if this.session.add_database_property(block_id, "文本".into(), quire_core::core::database::PropertyKind::Text).is_some() {
                                                                        this.schedule_save(false, window, cx);
                                                                        cx.notify();
                                                                    }
                                                                }))
                                                                .child("＋ 列"),
                                                        )),
                                                ),
                                        )
                                        .child(header)
                                        .child(rows)
                                } else {
                                    view.rounded(px(7.))
                                        .border_1()
                                        .border_color(Palette::color(&p.border))
                                        .px(px(12.))
                                        .py(px(10.))
                                        .text_color(muted)
                                        .child("数据库已删除或无法读取")
                                }
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
                            .when(kind == BlockKind::Toc, |view| {
                                if toc_entries.is_empty() {
                                    view.flex_col()
                                        .gap(px(3.))
                                        .rounded(px(5.))
                                        .px(px(8.))
                                        .py(px(7.))
                                        .bg(Palette::color(&p.elevated))
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(muted)
                                                .child("目录"),
                                        )
                                        .child(
                                            div()
                                                .py(px(3.))
                                                .text_color(muted)
                                                .child("此页面还没有标题"),
                                        )
                                } else {
                                    view.flex_col()
                                        .gap(px(2.))
                                        .rounded(px(5.))
                                        .px(px(8.))
                                        .py(px(7.))
                                        .bg(Palette::color(&p.elevated))
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(muted)
                                                .child("目录"),
                                        )
                                        .children(toc_entries)
                                }
                            })
                            .when(kind == BlockKind::Math, |view| {
                                view.rounded(px(5.))
                                    .px(px(10.))
                                    .py(px(8.))
                                    .bg(Palette::color(&p.elevated))
                                    .child(
                                        div()
                                            .font_family("Cambria Math")
                                            .text_size(px(20.))
                                            .text_color(self.palette.block_text(block.color.slot()))
                                            .child(quire_core::core::math::to_unicode(&block.text)),
                                    )
                            })
                            .when(kind == BlockKind::Synced, |view| {
                                if let Some(source) = block
                                    .sync_ref
                                    .and_then(|id| self.session.document.block(id))
                                {
                                    view.rounded(px(5.))
                                        .px(px(10.))
                                        .py(px(8.))
                                        .border_l_2()
                                        .border_color(Palette::color(&p.accent))
                                        .bg(Palette::color(&p.elevated))
                                        .child(
                                            div()
                                                .text_color(
                                                    self.palette.block_text(block.color.slot()),
                                                )
                                                .child(source.text.clone()),
                                        )
                                        .child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "replace-sync-source-{block_id:?}"
                                                )))
                                                .w(px(92.))
                                                .px(px(8.))
                                                .py(px(4.))
                                                .rounded(px(4.))
                                                .cursor_pointer()
                                                .text_size(px(11.))
                                                .text_color(accent)
                                                .hover(|button| {
                                                    button.bg(Palette::color(&p.border))
                                                })
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        if this
                                                            .session
                                                            .set_sync_source(block_id, None)
                                                        {
                                                            this.sync_inputs(window, cx);
                                                            this.schedule_save(false, window, cx);
                                                        }
                                                    },
                                                ))
                                                .child("更换来源"),
                                        )
                                } else {
                                    view.flex_col()
                                        .gap(px(2.))
                                        .rounded(px(5.))
                                        .px(px(8.))
                                        .py(px(7.))
                                        .bg(Palette::color(&p.elevated))
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(muted)
                                                .child("选择要同步的内容块"),
                                        )
                                        .children(sync_candidates)
                                }
                            })
                            .when(matches!(kind, BlockKind::Page | BlockKind::Link), |view| {
                                if let Some(target_id) = block.page_ref {
                                    let target_title = self
                                        .session
                                        .pages
                                        .iter()
                                        .find(|page| page.id == target_id)
                                        .map(|page| page.title.clone())
                                        .unwrap_or_else(|| "目标页面已删除".into());
                                    view.flex_col()
                                        .gap(px(5.))
                                        .rounded(px(6.))
                                        .border_1()
                                        .border_color(Palette::color(&p.border))
                                        .px(px(10.))
                                        .py(px(8.))
                                        .bg(Palette::color(&p.elevated))
                                        .child(
                                            div()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(target_title),
                                        )
                                        .when(
                                            self.session
                                                .pages
                                                .iter()
                                                .any(|page| page.id == target_id),
                                            |card| {
                                                card.child(
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "open-page-reference-{block_id:?}"
                                                        )))
                                                        .w(px(92.))
                                                        .px(px(8.))
                                                        .py(px(4.))
                                                        .rounded(px(4.))
                                                        .cursor_pointer()
                                                        .text_color(accent)
                                                        .hover(|button| {
                                                            button.bg(Palette::color(&p.border))
                                                        })
                                                        .on_click(cx.listener(
                                                            move |this, _, window, cx| {
                                                                this.activate_page(
                                                                    target_id, window, cx,
                                                                );
                                                            },
                                                        ))
                                                        .child("打开页面"),
                                                )
                                            },
                                        )
                                        .child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "replace-page-reference-{block_id:?}"
                                                )))
                                                .w(px(92.))
                                                .px(px(8.))
                                                .py(px(4.))
                                                .rounded(px(4.))
                                                .cursor_pointer()
                                                .text_size(px(11.))
                                                .text_color(muted)
                                                .hover(|button| {
                                                    button.bg(Palette::color(&p.border))
                                                })
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        if this
                                                            .session
                                                            .set_page_reference(block_id, None)
                                                        {
                                                            this.sync_inputs(window, cx);
                                                            this.schedule_save(false, window, cx);
                                                        }
                                                    },
                                                ))
                                                .child("更改目标"),
                                        )
                                } else {
                                    view.flex_col()
                                        .gap(px(2.))
                                        .rounded(px(5.))
                                        .px(px(8.))
                                        .py(px(7.))
                                        .bg(Palette::color(&p.elevated))
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(muted)
                                                .child("选择目标页面"),
                                        )
                                        .children(page_reference_candidates)
                                }
                            })
                            .when(kind == BlockKind::Embed, |view| {
                                let url = block.text.clone();
                                let title = quire_core::core::embed::describe(&url);
                                let openable = quire_core::core::embed::is_openable(&url);
                                view.flex_col()
                                    .gap(px(5.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(Palette::color(&p.border))
                                    .px(px(10.))
                                    .py(px(8.))
                                    .bg(Palette::color(&p.elevated))
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(muted)
                                            .child(url.clone()),
                                    )
                                    .when(openable, |card| {
                                        card.child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "open-embed-{block_id:?}"
                                                )))
                                                .w(px(140.))
                                                .px(px(8.))
                                                .py(px(4.))
                                                .rounded(px(4.))
                                                .cursor_pointer()
                                                .text_color(accent)
                                                .hover(|button| {
                                                    button.bg(Palette::color(&p.border))
                                                })
                                                .on_click(cx.listener(move |_, _, _, cx| {
                                                    cx.open_url(
                                                        &quire_core::core::embed::with_scheme(&url),
                                                    )
                                                }))
                                                .child("在浏览器中打开"),
                                        )
                                    })
                            })
                            .when_some(image_path, |view, path| {
                                let width =
                                    700. * f32::from(block.img_percent.clamp(10, 100)) / 100.;
                                view.flex_col()
                                    .gap(px(4.))
                                    .child(img(path).w(px(width)).max_h(px(520.)).rounded(px(5.)))
                                    .child(div().flex().gap(px(3.)).children(image_width_controls))
                            })
                            .when(image_missing, |view| {
                                view.child(div().text_color(muted).child("图片附件不可用"))
                            })
                            .when(kind == BlockKind::File, |view| {
                                let path = file_path.clone();
                                view.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(format!("📎 {}", block.text))
                                        .when_some(path, |row, path| {
                                            row.child(
                                                div()
                                                    .id(SharedString::from(format!(
                                                        "open-file-{block_id:?}"
                                                    )))
                                                    .px(px(7.))
                                                    .py(px(3.))
                                                    .rounded(px(4.))
                                                    .cursor_pointer()
                                                    .text_color(accent)
                                                    .hover(|button| {
                                                        button.bg(Palette::color(&p.elevated))
                                                    })
                                                    .on_click(cx.listener(move |_, _, _, cx| {
                                                        if let Ok(uri) =
                                                            url::Url::from_file_path(&path)
                                                        {
                                                            cx.open_url(uri.as_str());
                                                        }
                                                    }))
                                                    .child("打开文件"),
                                            )
                                            .when_some(
                                                file_attachment,
                                                |row, attachment| {
                                                    row.child(
                                                        div()
                                                            .id(SharedString::from(format!(
                                                                "save-file-{block_id:?}"
                                                            )))
                                                            .px(px(7.))
                                                            .py(px(3.))
                                                            .rounded(px(4.))
                                                            .cursor_pointer()
                                                            .text_color(accent)
                                                            .hover(|button| {
                                                                button
                                                                    .bg(Palette::color(&p.elevated))
                                                            })
                                                            .on_click(cx.listener(
                                                                move |this, _, _, cx| {
                                                                    this.save_attachment(
                                                                        attachment, cx,
                                                                    )
                                                                },
                                                            ))
                                                            .child("另存为"),
                                                    )
                                                },
                                            )
                                        }),
                                )
                            })
                            .when(kind == BlockKind::Code, |view| {
                                view.child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap(px(3.))
                                        .mb(px(4.))
                                        .children(code_lang_controls),
                                )
                            })
                            .when(
                                kind != BlockKind::Divider
                                    && kind != BlockKind::Table
                                    && kind != BlockKind::Columns
                                    && kind != BlockKind::Toc
                                    && kind != BlockKind::Synced
                                    && kind != BlockKind::Page
                                    && kind != BlockKind::Link
                                    && kind != BlockKind::Image
                                    && kind != BlockKind::File,
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
                            .children(slash_options)
                            .children(mention_options),
                    )
                    .when(self.hovered_block == Some(block_id), |row| {
                        row.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "block-text-color-{block_id:?}"
                                        )))
                                        .px(px(3.))
                                        .cursor_pointer()
                                        .text_color(self.palette.block_text(block.color.slot()))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.toggle_color_picker(block_id, false, cx)
                                        }))
                                        .child("A"),
                                )
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "block-background-color-{block_id:?}"
                                        )))
                                        .px(px(3.))
                                        .cursor_pointer()
                                        .text_color(muted)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.toggle_color_picker(block_id, true, cx)
                                        }))
                                        .child("▧"),
                                )
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "duplicate-block-{block_id:?}"
                                        )))
                                        .px(px(2.))
                                        .cursor_pointer()
                                        .text_color(muted)
                                        .hover(|button| button.text_color(accent))
                                        .when(!self.session.page.locked, |button| {
                                            button.on_click(cx.listener(
                                                move |this, _, window, cx| {
                                                    this.duplicate_block(block_id, window, cx)
                                                },
                                            ))
                                        })
                                        .child("⧉"),
                                )
                                .when(move_allowed, |actions| {
                                    actions
                                        .child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "move-block-up-{block_id:?}"
                                                )))
                                                .px(px(2.))
                                                .cursor_pointer()
                                                .text_color(muted)
                                                .hover(|button| button.text_color(accent))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.move_block(block_id, -1, window, cx)
                                                    },
                                                ))
                                                .child("↑"),
                                        )
                                        .child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "move-block-down-{block_id:?}"
                                                )))
                                                .px(px(2.))
                                                .cursor_pointer()
                                                .text_color(muted)
                                                .hover(|button| button.text_color(accent))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.move_block(block_id, 1, window, cx)
                                                    },
                                                ))
                                                .child("↓"),
                                        )
                                })
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "delete-block-{block_id:?}"
                                        )))
                                        .px(px(4.))
                                        .cursor_pointer()
                                        .text_color(muted)
                                        .hover(|button| button.text_color(accent))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.delete_block(block_id, window, cx)
                                        }))
                                        .child("×"),
                                ),
                        )
                    })
                    .when_some(color_picker_view, |row, picker| row.child(picker))
            })
            .collect::<Vec<_>>();
        let mut notes = (if self.organizer_trash {
            self.session.organizer.trashed_notes().collect::<Vec<_>>()
        } else {
            self.session.organizer.live_notes().collect::<Vec<_>>()
        })
        .into_iter()
        .cloned()
        .filter(|note| {
            self.organizer_tag
                .as_ref()
                .is_none_or(|tag| note.tags.contains(tag))
                && (self.search_query.trim().is_empty()
                    || note
                        .title
                        .to_lowercase()
                        .contains(&self.search_query.to_lowercase())
                    || note
                        .body
                        .to_lowercase()
                        .contains(&self.search_query.to_lowercase()))
        })
        .collect::<Vec<_>>();
        let note_count = notes.len();
        notes.sort_by_key(|note| {
            std::cmp::Reverse((
                note.pinned,
                if self.note_sort_recent {
                    note.created
                } else {
                    note.edited
                },
            ))
        });
        let note_rows = notes
            .into_iter()
            .map(|note| {
                let id = note.id;
                let selected = self.selected_note == Some(id);
                let pinned = note.pinned;
                let edited_age = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                let edited_age = edited_age.saturating_sub(note.edited).max(0);
                let edited_label = if edited_age < 60 {
                    "刚刚".to_owned()
                } else if edited_age < 3600 {
                    format!("{} 分钟前", edited_age / 60)
                } else if edited_age < 86400 {
                    format!("{} 小时前", edited_age / 3600)
                } else {
                    format!("{} 天前", edited_age / 86400)
                };
                let preview = note
                    .body
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .take(3)
                    .collect::<Vec<_>>()
                    .join("\n")
                    .chars()
                    .take(150)
                    .collect::<String>();
                let tags = note.tags.clone();
                let title = if note.title.trim().is_empty() {
                    "无标题".to_owned()
                } else {
                    note.title
                };
                div()
                    .id(SharedString::from(format!("organizer-note-{id:?}")))
                    .w_full()
                    .px(px(14.))
                    .py(px(8.))
                    .rounded(px(9.))
                    .border_1()
                    .border_color(if selected {
                        accent
                    } else {
                        Palette::color(&p.border)
                    })
                    .bg(Palette::color(&p.elevated2))
                    .cursor_pointer()
                    .text_color(if selected { accent } else { text })
                    .hover(|row| row.border_color(accent).bg(Palette::color(&p.elevated)))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.select_note(id, window, cx)),
                    )
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered {
                            this.hovered_note = Some(id);
                        } else if this.hovered_note == Some(id) {
                            this.hovered_note = None;
                        }
                        cx.notify();
                    }))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_size(px(11.))
                            .text_color(muted)
                            .child(edited_label)
                            .when(pinned && !self.organizer_trash, |meta| {
                                meta.child(div().text_color(accent).child("★"))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .when(self.hovered_note == Some(id), |header| {
                                header
                                    .child(
                                        div()
                                            .id(SharedString::from(format!("organizer-pin-{id:?}")))
                                            .px(px(3.))
                                            .text_color(if pinned { accent } else { muted })
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.toggle_note_pin(id, window, cx)
                                            }))
                                            .when(!self.organizer_trash, |button| {
                                                button.child(if pinned { "★" } else { "☆" })
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "organizer-trash-{id:?}"
                                            )))
                                            .px(px(3.))
                                            .text_color(muted)
                                            .cursor_pointer()
                                            .hover(|button| button.text_color(rgb(0xe5534b)))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                if this.organizer_trash {
                                                    this.restore_note(id, window, cx)
                                                } else {
                                                    this.trash_note(id, window, cx)
                                                }
                                            }))
                                            .child(if self.organizer_trash { "↩" } else { "×" }),
                                    )
                            }),
                    )
                    .when(!preview.is_empty(), |card| {
                        card.child(
                            div()
                                .text_size(px(14.))
                                .line_height(px(18.))
                                .text_color(muted)
                                .max_h(px(54.))
                                .overflow_hidden()
                                .child(preview),
                        )
                    })
                    .when(!tags.is_empty(), |card| {
                        card.child(div().flex().flex_wrap().gap(px(4.)).children(
                            tags.into_iter().map(|tag| {
                                div()
                                    .px(px(7.))
                                    .py(px(2.))
                                    .rounded(px(10.))
                                    .border_1()
                                    .border_color(Palette::color(&p.border))
                                    .text_size(px(10.))
                                    .text_color(muted)
                                    .child(format!("#{tag}"))
                            }),
                        ))
                    })
            })
            .collect::<Vec<_>>();
        let mut tasks = (if self.organizer_trash {
            self.session.organizer.trashed_tasks().collect::<Vec<_>>()
        } else {
            self.session.organizer.live_tasks().collect::<Vec<_>>()
        })
        .into_iter()
        .filter(|task| {
            if self.organizer_trash {
                return self
                    .organizer_tag
                    .as_ref()
                    .is_none_or(|tag| task.tags.contains(tag))
                    && (self.search_query.trim().is_empty()
                        || task
                            .title
                            .to_lowercase()
                            .contains(&self.search_query.to_lowercase())
                        || task
                            .notes
                            .to_lowercase()
                            .contains(&self.search_query.to_lowercase()));
            }
            let in_scope = self
                .selected_task_smart_view
                .map_or(task.list == self.selected_task_list, |view| {
                    self.task_matches_smart_view(task, view)
                });
            in_scope
                && self
                    .organizer_tag
                    .as_ref()
                    .is_none_or(|tag| task.tags.contains(tag))
                && (self.search_query.trim().is_empty()
                    || task
                        .title
                        .to_lowercase()
                        .contains(&self.search_query.to_lowercase())
                    || task
                        .notes
                        .to_lowercase()
                        .contains(&self.search_query.to_lowercase()))
                && (self.selected_task_smart_view == Some(4) && task.done
                    || self.selected_task_smart_view != Some(4)
                        && (!task.done || self.show_task_done))
        })
        .cloned()
        .collect::<Vec<_>>();
        let task_count = tasks.len();
        let task_view_title = if self.organizer_trash {
            "回收站".to_owned()
        } else if let Some(view) = self.selected_task_smart_view {
            match view {
                0 => "收集箱".to_owned(),
                1 => "今天".to_owned(),
                2 => "最近 7 天".to_owned(),
                3 => "全部".to_owned(),
                4 => "已完成".to_owned(),
                _ => "任务".to_owned(),
            }
        } else {
            self.session
                .organizer
                .list(self.selected_task_list)
                .map(|list| list.name.clone())
                .unwrap_or_else(|| "收集箱".to_owned())
        };
        let completed_task_count = self
            .session
            .organizer
            .live_tasks()
            .filter(|task| task.done)
            .count();
        match self.task_sort {
            1 => tasks.sort_by(|a, b| {
                a.due
                    .is_none()
                    .cmp(&b.due.is_none())
                    .then_with(|| a.due.cmp(&b.due))
                    .then_with(|| a.ord.cmp(&b.ord))
            }),
            2 => tasks.sort_by(|a, b| {
                b.priority
                    .slot()
                    .cmp(&a.priority.slot())
                    .then_with(|| a.ord.cmp(&b.ord))
            }),
            _ => tasks.sort_by_key(|task| task.ord),
        }
        let today = quire_core::core::date::today_iso();
        let task_rows = tasks
            .into_iter()
            .map(|task| {
                let id = task.id;
                let done = task.done;
                let selected = self.selected_task == Some(id);
                let task_tags = task.tags.clone();
                let subtask_total = task.subtasks.len();
                let subtask_done = task.subtasks.iter().filter(|item| item.done).count();
                let due_label = task.due.as_deref().map(|due| {
                    if due == today {
                        "今天".to_owned()
                    } else {
                        due.to_owned()
                    }
                });
                let title = if task.title.trim().is_empty() {
                    "无标题任务".to_owned()
                } else {
                    task.title
                };
                div()
                    .id(SharedString::from(format!("organizer-task-{id:?}")))
                    .w_full()
                    .px(px(9.))
                    .py(px(8.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .text_color(if selected { accent } else { text })
                    .when(selected, |row| row.bg(Palette::color(&p.bg).opacity(0.34)))
                    .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.28)))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.select_task(id, window, cx)),
                    )
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered {
                            this.hovered_task = Some(id);
                        } else if this.hovered_task == Some(id) {
                            this.hovered_task = None;
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .when(!self.organizer_trash && self.task_multi_select, |row| {
                                row.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "organizer-task-select-{id:?}"
                                        )))
                                        .w(px(17.))
                                        .h(px(17.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(3.))
                                        .border_1()
                                        .border_color(if self.selected_task_ids.contains(&id) {
                                            accent
                                        } else {
                                            muted
                                        })
                                        .text_color(accent)
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.toggle_task_multi_select(id, cx)
                                        }))
                                        .child(if self.selected_task_ids.contains(&id) {
                                            "✓"
                                        } else {
                                            ""
                                        }),
                                )
                            })
                            .child(
                                div()
                                    .id(SharedString::from(format!("organizer-task-done-{id:?}")))
                                    .w(px(18.))
                                    .h(px(18.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(9.))
                                    .border_1()
                                    .border_color(if done { accent } else { muted })
                                    .text_color(accent)
                                    .cursor_pointer()
                                    .when(!self.organizer_trash, |button| {
                                        button
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.toggle_task_done(id, window, cx)
                                            }))
                                            .child(if done { "✓" } else { "" })
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_color(if done { muted } else { text })
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .when(task.priority != Priority::None, |row| {
                                row.child(div().text_size(px(11.)).text_color(accent).child("◆"))
                            })
                            .when_some(due_label, |row, due| {
                                row.child(
                                    div()
                                        .px(px(6.))
                                        .py(px(2.))
                                        .rounded(px(9.))
                                        .border_1()
                                        .border_color(Palette::color(&p.border))
                                        .text_size(px(10.))
                                        .text_color(muted)
                                        .child(due),
                                )
                            })
                            .child(
                                div()
                                    .id(SharedString::from(format!("organizer-task-trash-{id:?}")))
                                    .px(px(3.))
                                    .text_color(muted)
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if this.organizer_trash {
                                            this.restore_task(id, window, cx)
                                        } else {
                                            this.trash_task(id, window, cx)
                                        }
                                    }))
                                    .when(
                                        self.organizer_trash || self.hovered_task == Some(id),
                                        |button| {
                                            button.child(if self.organizer_trash {
                                                "↩"
                                            } else {
                                                "×"
                                            })
                                        },
                                    ),
                            ),
                    )
                    .when(!task_tags.is_empty() || subtask_total > 0, |row| {
                        row.child(
                            div()
                                .pl(px(if self.task_multi_select { 23. } else { 24. }))
                                .flex()
                                .items_center()
                                .gap(px(5.))
                                .text_size(px(10.))
                                .text_color(muted)
                                .children(task_tags.into_iter().map(|tag| {
                                    div()
                                        .px(px(6.))
                                        .py(px(2.))
                                        .rounded(px(8.))
                                        .bg(Palette::color(&p.elevated).opacity(0.55))
                                        .child(format!("#{tag}"))
                                }))
                                .when(subtask_total > 0, |meta| {
                                    meta.child(format!("✓ {subtask_done}/{subtask_total}"))
                                }),
                        )
                    })
            })
            .collect::<Vec<_>>();
        let task_summary = self.selected_task.and_then(|id| {
            self.session.organizer.task(id).map(|task| {
                let list_name = self
                    .session
                    .organizer
                    .list(task.list)
                    .map(|list| list.name.clone())
                    .unwrap_or_else(|| "收集箱".into());
                (
                    id,
                    task.done,
                    priority_label(task.priority),
                    repeat_label(task.repeat),
                    task.due.clone(),
                    task.list,
                    list_name,
                    task.subtasks.clone(),
                )
            })
        });
        let mut task_lists = self.session.organizer.lists.clone();
        task_lists.sort_by_key(|list| list.ord);
        let now_days = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            / 86_400;
        let week = quire_core::core::date::to_iso(now_days + 7);
        let task_smart_view_buttons = [
            (0, "收集箱"),
            (1, "今天"),
            (2, "最近 7 天"),
            (3, "全部"),
            (4, "已完成"),
        ]
        .into_iter()
        .map(|(view, label)| {
            let count = self
                .session
                .organizer
                .live_tasks()
                .filter(|task| {
                    task_matches_smart_view(&self.session.organizer, task, view, &today, &week)
                        && (view == 4 && task.done
                            || view != 4 && (!task.done || self.show_task_done))
                })
                .count();
            div()
                .id(SharedString::from(format!("organizer-smart-view-{view}")))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(5.))
                .cursor_pointer()
                .text_color(if self.selected_task_smart_view == Some(view) {
                    accent
                } else {
                    muted
                })
                .when(self.selected_task_smart_view == Some(view), |chip| {
                    chip.bg(Palette::color(&p.elevated))
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_task_smart_view(view, window, cx)
                }))
                .child(format!("{label} · {count}"))
        })
        .collect::<Vec<_>>();
        let task_list_buttons = task_lists
            .into_iter()
            .map(|list| {
                let id = list.id;
                let count = self
                    .session
                    .organizer
                    .live_tasks()
                    .filter(|task| task.list == id)
                    .count();
                div()
                    .id(SharedString::from(format!("organizer-list-row-{id:?}")))
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .child(
                        div()
                            .id(SharedString::from(format!("organizer-list-{id:?}")))
                            .flex_1()
                            .min_w_0()
                            .px(px(8.))
                            .py(px(5.))
                            .rounded(px(5.))
                            .cursor_pointer()
                            .text_color(
                                if self.selected_task_smart_view.is_none()
                                    && self.selected_task_list == id
                                {
                                    accent
                                } else {
                                    muted
                                },
                            )
                            .when(
                                self.selected_task_smart_view.is_none()
                                    && self.selected_task_list == id,
                                |chip| chip.bg(Palette::color(&p.elevated)),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_task_list(id, window, cx)
                            }))
                            .child(format!("{} · {count}", list.name)),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("organizer-list-delete-{id:?}")))
                            .px(px(5.))
                            .py(px(4.))
                            .rounded(px(4.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.text_color(rgb(0xe5534b)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.delete_task_list(id, window, cx)
                            }))
                            .child("×"),
                    )
            })
            .collect::<Vec<_>>();
        let mut organizer_tags = Vec::<String>::new();
        if self.organizer_tasks {
            let tasks = if self.organizer_trash {
                self.session.organizer.trashed_tasks().collect::<Vec<_>>()
            } else {
                self.session.organizer.live_tasks().collect::<Vec<_>>()
            };
            for task in tasks {
                for tag in &task.tags {
                    if !organizer_tags.contains(tag) {
                        organizer_tags.push(tag.clone());
                    }
                }
            }
        } else {
            let notes = if self.organizer_trash {
                self.session.organizer.trashed_notes().collect::<Vec<_>>()
            } else {
                self.session.organizer.live_notes().collect::<Vec<_>>()
            };
            for note in notes {
                for tag in &note.tags {
                    if !organizer_tags.contains(tag) {
                        organizer_tags.push(tag.clone());
                    }
                }
            }
        }
        organizer_tags.sort();
        let organizer_tag_buttons = organizer_tags
            .into_iter()
            .map(|tag| {
                let selected = self.organizer_tag.as_deref() == Some(tag.as_str());
                let label = tag.clone();
                div()
                    .id(SharedString::from(format!("organizer-tag-{tag}")))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(10.))
                    .cursor_pointer()
                    .text_color(if selected { accent } else { muted })
                    .when(selected, |chip| chip.bg(Palette::color(&p.elevated)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.toggle_organizer_tag(label.clone(), window, cx)
                    }))
                    .child(tag)
            })
            .collect::<Vec<_>>();
        let organizer_view = div()
            .id("organizer-main")
            .flex()
            .flex_1()
            .min_h_0()
            .px(px(14.))
            .py(px(14.))
            .child(
                div()
                    .id("organizer-card")
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(Palette::color(&p.border))
                    .bg(Palette::color(&p.elevated))
                    .relative()
                    .overflow_hidden()
                    .child(
                        div()
                    .id("organizer-note-list")
                    .flex_1()
                    .min_w_0()
                    .flex_shrink_0()
                    .when(self.organizer_tasks, |column| {
                        // Reset flex-1's zero basis so the task list retains its
                        // intended 400px reference width inside the three-column card.
                        column.flex_initial().w(px(434.)).flex_shrink_0()
                    })
                    .h_full()
                    .border_r_1()
                    .border_color(Palette::color(&p.border))
                    .px(px(20.))
                    .py(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(if self.organizer_tasks {
                                format!("{task_view_title}  {task_count} 项待办")
                            } else {
                                format!("全部笔记 · {note_count} 条")
                            }))
                            .child(
                                div()
                                    .id("organizer-create-note")
                                    .px(px(8.))
                                    .py(px(4.))
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .text_color(accent)
                                    .hover(|button| button.bg(Palette::color(&p.elevated)))
                                    .when(!self.organizer_trash, |button| {
                                        button.on_click(cx.listener(|this, _, window, cx| {
                                            if this.organizer_tasks {
                                                this.create_task(window, cx);
                                            } else {
                                                this.create_note(window, cx);
                                            }
                                        }))
                                    })
                                    .child(if self.organizer_trash {
                                        "回收站"
                                    } else if self.organizer_tasks {
                                        "＋"
                                    } else {
                                        ""
                                    }),
                            )
                            .when(self.organizer_tasks && !self.organizer_trash, |row| {
                                row.child(
                                    div()
                                        .id("organizer-create-task-list")
                                        .px(px(7.))
                                        .py(px(4.))
                                        .rounded(px(4.))
                                        .cursor_pointer()
                                        .text_color(muted)
                                        .hover(|button| button.bg(Palette::color(&p.elevated)))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.create_task_list(window, cx)
                                        }))
                                        .child("＋清单"),
                                )
                                .child(
                                    div()
                                        .id("organizer-task-multiselect-toggle")
                                        .px(px(7.))
                                        .py(px(4.))
                                        .rounded(px(4.))
                                        .cursor_pointer()
                                        .text_color(if self.task_multi_select { accent } else { muted })
                                        .hover(|button| button.bg(Palette::color(&p.elevated)))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.task_multi_select = !this.task_multi_select;
                                            if !this.task_multi_select {
                                                this.selected_task_ids.clear();
                                            }
                                            cx.notify();
                                        }))
                                        .child(if self.task_multi_select { "取消多选" } else { "多选" }),
                                )
                            }),
                    )
                    .when(!self.organizer_tasks, |list| {
                        list.child(
                            div()
                                .id("organizer-note-toolbar")
                                .w_full()
                                .h(px(32.))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .id("organizer-note-search")
                                        .w(px(240.))
                                        .h(px(32.))
                                        .px(px(8.))
                                        .rounded(px(5.))
                                        .bg(Palette::color(&p.bg).opacity(0.25))
                                        .child(self.search_input.clone()),
                                )
                                .child(div().flex_1())
                                .child(
                                    div()
                                        .id("organizer-note-sort-toggle")
                                        .px(px(8.))
                                        .py(px(6.))
                                        .rounded(px(5.))
                                        .cursor_pointer()
                                        .text_color(muted)
                                        .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.25)))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.note_sort_recent = !this.note_sort_recent;
                                            cx.notify();
                                        }))
                                        .child(if self.note_sort_recent {
                                            "最近创建  ⌄"
                                        } else {
                                            "最近更新  ⌄"
                                        }),
                                )
                                .child(
                                    div()
                                        .id("organizer-note-trash-shortcut")
                                        .px(px(8.))
                                        .py(px(6.))
                                        .rounded(px(5.))
                                        .border_1()
                                        .border_color(Palette::color(&p.border))
                                        .cursor_pointer()
                                        .text_color(muted)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.toggle_organizer_trash(window, cx)
                                        }))
                                        .child(if self.organizer_trash { "返回笔记" } else { "回收站" }),
                                ),
                        )
                    })
                    .when(self.organizer_tasks && !self.organizer_trash, |list| {
                        list.child(
                            div()
                                .id("organizer-task-quick-add")
                                .w_full()
                                .h(px(36.))
                                .px(px(8.))
                                .rounded(px(5.))
                                .border_1()
                                .border_color(Palette::color(&p.border))
                                .bg(Palette::color(&p.bg).opacity(0.2))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(div().flex_1().child(self.task_title_input.clone()))
                                .child(div().text_size(px(10.)).text_color(muted).child("回车创建")),
                        )
                    })
                    .when(self.organizer_tasks && !self.organizer_trash, |list| {
                        list.child(
                            div()
                                .id("organizer-task-sort-toggle")
                                .px(px(8.))
                                .py(px(5.))
                                .rounded(px(5.))
                                .cursor_pointer()
                                .text_color(muted)
                                .hover(|row| row.bg(Palette::color(&p.elevated)))
                                .on_click(cx.listener(|this, _, _, cx| this.cycle_task_sort(cx)))
                                .child(match self.task_sort {
                                    1 => "排序：截止日期",
                                    2 => "排序：优先级",
                                    _ => "排序：手动顺序",
                                }),
                        )
                        .when(!self.selected_task_ids.is_empty(), |list| {
                            list.child(
                                div()
                                    .flex()
                                    .gap(px(5.))
                                    .child(
                                        div()
                                            .id("organizer-bulk-complete")
                                            .flex_1()
                                            .px(px(6.))
                                            .py(px(5.))
                                            .rounded(px(4.))
                                            .cursor_pointer()
                                            .text_color(accent)
                                            .hover(|button| button.bg(Palette::color(&p.elevated)))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.bulk_complete_tasks(window, cx)
                                            }))
                                            .child(format!(
                                                "完成 {} 项",
                                                self.selected_task_ids.len()
                                            )),
                                    )
                                    .child(
                                        div()
                                            .id("organizer-bulk-trash")
                                            .px(px(7.))
                                            .py(px(5.))
                                            .rounded(px(4.))
                                            .cursor_pointer()
                                            .text_color(muted)
                                            .hover(|button| button.text_color(rgb(0xe5534b)))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.bulk_trash_tasks(window, cx)
                                            }))
                                            .child("移入回收站"),
                                    ),
                            )
                        })
                    })
                    .child(
                        div()
                            .id("organizer-notes-scroll")
                            .flex_1()
                            .h_full()
                            .overflow_y_scroll()
                            .when(self.organizer_tasks, |list| list.children(task_rows))
                            .when(!self.organizer_tasks, |list| list.children(note_rows)),
                    )
                    .when(self.organizer_tasks && !self.organizer_trash, |list| {
                        list.child(
                            div()
                                .id("organizer-task-footer")
                                .h(px(30.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .border_t_1()
                                .border_color(Palette::color(&p.border))
                                .text_size(px(11.))
                                .text_color(muted)
                                .when(self.selected_task_smart_view != Some(4), |footer| {
                                    footer.child(
                                        div()
                                            .id("organizer-show-done-toggle")
                                            .px(px(8.))
                                            .py(px(4.))
                                            .rounded(px(4.))
                                            .cursor_pointer()
                                            .hover(|row| row.bg(Palette::color(&p.elevated)))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.toggle_show_task_done(window, cx)
                                            }))
                                            .child(if self.show_task_done {
                                                "☑ 显示已完成"
                                            } else {
                                                "□ 显示已完成"
                                            }),
                                    )
                                })
                                .child(format!(
                                    "已完成 {completed_task_count} / {}",
                                    self.session.organizer.live_tasks().count()
                                )),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .px(px(20.))
                    .py(px(20.))
                    .when(!self.organizer_tasks || self.selected_note.is_none(), |pane| {
                        pane.flex_none()
                            .w(px(0.))
                            .px(px(0.))
                            .py(px(0.))
                            .overflow_hidden()
                    })
                    .when(
                        self.selected_task.is_some(),
                        |editor| {
                            editor
                                .w_full()
                                .flex()
                                .flex_col()
                                .gap(px(18.))
                                .when_some(
                                    task_summary,
                                    |pane, (id, done, priority, repeat, due, list_id, list_name, subtasks)| {
                                        pane.child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(10.))
                                                .text_color(muted)
                                                .child(format!(
                                                    "{} · {} · 优先级 {} · 截止 {}",
                                                    if done { "已完成" } else { "待完成" },
                                                    list_name,
                                                    priority,
                                                    due.as_deref().unwrap_or("未设置")
                                                ))
                                                .child(
                                                    div()
                                                        .id("organizer-task-priority-cycle")
                                                        .px(px(7.))
                                                        .py(px(3.))
                                                        .rounded(px(4.))
                                                        .cursor_pointer()
                                                        .text_color(accent)
                                                        .hover(|button| {
                                                            button.bg(Palette::color(&p.elevated))
                                                        })
                                                        .on_click(cx.listener(
                                                            move |this, _, window, cx| {
                                                                this.cycle_task_priority(
                                                                    id, window, cx,
                                                                )
                                                            },
                                                        ))
                                                        .child("更改优先级"),
                                                )
                                                .child(
                                                    div()
                                                        .id("organizer-task-repeat-cycle")
                                                        .px(px(7.))
                                                        .py(px(3.))
                                                        .rounded(px(4.))
                                                        .cursor_pointer()
                                                        .text_color(accent)
                                                        .hover(|button| {
                                                            button.bg(Palette::color(&p.elevated))
                                                        })
                                                        .on_click(cx.listener(
                                                            move |this, _, window, cx| {
                                                                this.cycle_task_repeat(
                                                                    id, window, cx,
                                                                )
                                                            },
                                                        ))
                                                        .child(format!("重复：{repeat}")),
                                                )
                                                .child(
                                                    div()
                                                        .id("organizer-task-toggle-done")
                                                        .px(px(8.))
                                                        .py(px(4.))
                                                        .rounded(px(4.))
                                                        .cursor_pointer()
                                                        .text_color(accent)
                                                        .hover(|button| {
                                                            button.bg(Palette::color(&p.elevated))
                                                        })
                                                        .on_click(cx.listener(
                                                            move |this, _, window, cx| {
                                                                this.toggle_task_done(
                                                                    id, window, cx,
                                                                )
                                                            },
                                                        ))
                                                        .child(if done {
                                                            "重新打开"
                                                        } else {
                                                            "标记完成"
                                                        }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.))
                                                .text_color(muted)
                                                .child("截止日期")
                                                .child(
                                                    div()
                                                        .id("organizer-task-due-input")
                                                        .w(px(150.))
                                                        .h(px(30.))
                                                        .rounded(px(4.))
                                                        .border_1()
                                                        .border_color(if self.task_due_error {
                                                            Hsla::from(rgb(0xe5534b))
                                                        } else {
                                                            Palette::color(&p.border)
                                                        })
                                                        .px(px(6.))
                                                        .child(self.task_due_input.clone()),
                                                )
                                                .when(self.task_due_error, |row| {
                                                    row.child(div().text_color(rgb(0xe5534b)).child("请输入 YYYY-MM-DD 日期"))
                                                }),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(5.))
                                                .text_color(muted)
                                                .child("移至")
                                                .children(
                                                    std::iter::once((
                                                        quire_core::core::organizer::ListId::INBOX,
                                                        "收集箱".to_owned(),
                                                    ))
                                                    .chain(
                                                        self.session.organizer.lists.iter().map(
                                                            |list| (list.id, list.name.clone()),
                                                        ),
                                                    )
                                                    .filter(|(target, _)| *target != list_id)
                                                    .map(|(target, label)| {
                                                        div()
                                                            .id(SharedString::from(format!(
                                                                "organizer-move-task-{}-{target:?}",
                                                                id.0
                                                            )))
                                                            .px(px(7.))
                                                            .py(px(3.))
                                                            .rounded(px(4.))
                                                            .cursor_pointer()
                                                            .text_color(accent)
                                                            .hover(|button| {
                                                                button
                                                                    .bg(Palette::color(&p.elevated))
                                                            })
                                                            .on_click(cx.listener(
                                                                move |this, _, window, cx| {
                                                                    this.move_task_to_list(
                                                                        id, target, window, cx,
                                                                    )
                                                                },
                                                            ))
                                                            .child(label)
                                                    })
                                                    .collect::<Vec<_>>(),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(4.))
                                                .child(
                                                    div()
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(muted)
                                                        .child(format!(
                                                            "子任务 · {}/{}",
                                                            subtasks.iter().filter(|item| item.done).count(),
                                                            subtasks.len()
                                                        )),
                                                )
                                                .children(subtasks.into_iter().map(|subtask| {
                                                    let subtask_id = subtask.id;
                                                    let done = subtask.done;
                                                    let title = subtask.title;
                                                    let edit_title = title.clone();
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(6.))
                                                        .child(
                                                            div()
                                                                .id(SharedString::from(format!("organizer-subtask-done-{id:?}-{subtask_id}")))
                                                                .w(px(18.))
                                                                .h(px(18.))
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .rounded(px(9.))
                                                                .border_1()
                                                                .border_color(if done { accent } else { muted })
                                                                .cursor_pointer()
                                                                .when(!self.organizer_trash, |control| control.on_click(cx.listener(move |this, _, window, cx| {
                                                                    this.toggle_subtask_done(id, subtask_id, done, window, cx)
                                                                })))
                                                                .child(if done { "✓" } else { "" }),
                                                        )
                                                        .when(self.editing_subtask == Some((id, subtask_id)), |row| {
                                                            row.child(div().flex_1().h(px(28.)).child(self.subtask_title_input.clone()))
                                                        })
                                                        .when(self.editing_subtask != Some((id, subtask_id)), |row| {
                                                            row.child(
                                                                div()
                                                                    .id(SharedString::from(format!("organizer-subtask-title-{id:?}-{subtask_id}")))
                                                                    .flex_1()
                                                                    .text_color(if done { muted } else { text })
                                                                    .cursor_pointer()
                                                                    .on_click(cx.listener(move |this, _, window, cx| {
                                                                        this.start_edit_subtask(id, subtask_id, edit_title.clone(), window, cx)
                                                                    }))
                                                                    .child(title),
                                                            )
                                                        })
                                                        .when(!self.organizer_trash, |row| row.child(
                                                            div()
                                                                .id(SharedString::from(format!("organizer-subtask-delete-{id:?}-{subtask_id}")))
                                                                .px(px(4.))
                                                                .cursor_pointer()
                                                                .text_color(muted)
                                                                .on_click(cx.listener(move |this, _, window, cx| this.delete_subtask(id, subtask_id, window, cx)))
                                                                .child("×")
                                                        ))
                                                }))
                                                .when(!self.organizer_trash, |list| list.child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(6.))
                                                        .child(div().flex_1().h(px(30.)).rounded(px(4.)).border_1().border_color(Palette::color(&p.border)).px(px(6.)).child(self.subtask_title_input.clone()))
                                                        .child(div().id("organizer-subtask-add").px(px(8.)).py(px(4.)).rounded(px(4.)).cursor_pointer().text_color(accent).on_click(cx.listener(|this, _, window, cx| this.add_subtask_from_draft(window, cx))).child("添加"))
                                                )),
                                        )
                                    },
                                )
                                .child(
                                    div()
                                        .text_size(px(30.))
                                        .font_weight(FontWeight::BOLD)
                                        .child(self.note_title_input.clone()),
                                )
                                .child(
                                    div()
                                        .id("organizer-note-body")
                                        .flex_1()
                                        .min_h(px(300.))
                                        .child(self.note_body_input.clone()),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .text_color(muted)
                                        .child("标签")
                                        .child(
                                            div()
                                                .id("organizer-tags-input")
                                                .flex_1()
                                                .h(px(32.))
                                                .rounded(px(5.))
                                                .border_1()
                                                .border_color(Palette::color(&p.border))
                                                .px(px(8.))
                                                .child(self.organizer_tags_input.clone()),
                                        ),
                                )
                        },
                    )
                    .when(
                        self.selected_task.is_none(),
                        |empty| {
                            empty
                                .w_full()
                                .text_color(muted)
                                .child(div().text_size(px(14.)).child(if self.organizer_tasks {
                                    "选择一项任务，或新建任务开始安排。"
                                } else {
                                    "选择一篇笔记，或新建笔记开始记录。"
                                }))
                        },
                    ),
                    )
                    .child(
                        div()
                            .id("organizer-navigation")
                            .w(px(200.))
                            .flex_shrink_0()
                            .h_full()
                            .border_l_1()
                            .border_color(Palette::color(&p.border))
                            .px(px(10.))
                            .py(px(10.))
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div().flex().gap(px(6.)).child(
                                    div().id("organizer-tab-notes").flex_1().px(px(9.)).py(px(6.)).rounded(px(5.)).cursor_pointer().text_color(if !self.organizer_tasks { accent } else { muted }).when(!self.organizer_tasks, |tab| tab.bg(Palette::color(&p.elevated))).on_click(cx.listener(|this, _, window, cx| this.toggle_organizer_tab(false, window, cx))).child("笔记")
                                ).child(
                                    div().id("organizer-tab-tasks").flex_1().px(px(9.)).py(px(6.)).rounded(px(5.)).cursor_pointer().text_color(if self.organizer_tasks { accent } else { muted }).when(self.organizer_tasks, |tab| tab.bg(Palette::color(&p.elevated))).on_click(cx.listener(|this, _, window, cx| this.toggle_organizer_tab(true, window, cx))).child("任务")
                                ),
                            )
                            .when(self.organizer_tasks, |nav| {
                                nav.child(
                                    div()
                                        .id("organizer-task-search")
                                        .w_full()
                                        .h(px(32.))
                                        .px(px(8.))
                                        .rounded(px(5.))
                                        .bg(Palette::color(&p.bg).opacity(0.25))
                                        .child(self.search_input.clone()),
                                )
                            })
                            .when(!self.organizer_tasks, |nav| nav.child(
                                div()
                                    .id("organizer-all-notes")
                                    .px(px(8.))
                                    .py(px(6.))
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .text_color(if self.organizer_tag.is_none() { accent } else { muted })
                                    .when(self.organizer_tag.is_none(), |row| row.bg(Palette::color(&p.elevated)))
                                    .on_click(cx.listener(|this, _, window, cx| this.toggle_organizer_tag(String::new(), window, cx)))
                                    .child(format!("全部笔记 · {note_count}"))
                            ))
                            .when(self.organizer_tasks && !self.organizer_trash, |nav| nav.child(
                                div().flex().flex_col().gap(px(3.))
                                    .child(div().text_size(px(11.)).text_color(muted).mb(px(2.)).child("视图"))
                                    .children(task_smart_view_buttons)
                                    .child(div().text_size(px(11.)).text_color(muted).mt(px(7.)).mb(px(2.)).child("清单"))
                                    .child(div().id("organizer-task-list-selector").flex().flex_col().gap(px(3.)).overflow_y_scroll().children(task_list_buttons))
                            ))
                            .child(div().text_size(px(11.)).text_color(muted).mt(px(6.)).child("标签"))
                            .when(!organizer_tag_buttons.is_empty(), |nav| nav.child(
                                div().id("organizer-tag-filter-list").flex().flex_col().gap(px(3.)).children(organizer_tag_buttons)
                            ))
                            .child(div().flex_1())
                            .child(
                                div().id("organizer-trash-toggle").px(px(8.)).py(px(7.)).rounded(px(5.)).cursor_pointer().text_color(if self.organizer_trash { accent } else { muted }).when(self.organizer_trash, |row| row.bg(Palette::color(&p.elevated))).hover(|row| row.bg(Palette::color(&p.elevated))).on_click(cx.listener(|this, _, window, cx| this.toggle_organizer_trash(window, cx))).child(format!("回收站 · {}", self.session.organizer.trashed_notes().count() + self.session.organizer.trashed_tasks().count()))
                            ),
                    )
                    .when(!self.organizer_tasks && !self.organizer_trash, |card| {
                        card.child(
                            div()
                                .id("organizer-create-note-floating")
                                .absolute()
                                .bottom(px(20.))
                                .right(px(220.))
                                .w(px(56.))
                                .h(px(56.))
                                .rounded(px(28.))
                                .bg(accent)
                                .text_color(rgb(0xffffff))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.create_note(window, cx)
                                }))
                                .child("＋"),
                        )
                    })
                    .when(self.selected_note.is_some() && !self.organizer_tasks, |card| {
                        card.child(
                            div()
                                .id("organizer-note-detail-overlay")
                                .absolute()
                                .top(px(16.))
                                .bottom(px(16.))
                                .right(px(14.))
                                .w(px(420.))
                                .max_w(px(520.))
                                .rounded(px(14.))
                                .border_1()
                                .border_color(Palette::color(&p.border))
                                .bg(Palette::color(&p.elevated2))
                                .shadow_lg()
                                .px(px(16.))
                                .py(px(14.))
                                .flex()
                                .flex_col()
                                .gap(px(12.))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(div().flex_1().text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child(self.note_title_input.clone()))
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(4.))
                                                .children(self.selected_note.map(|id| {
                                                    let pinned = self.session.organizer.note(id).is_some_and(|note| note.pinned);
                                                    vec![
                                                        div().id("organizer-note-detail-pin").px(px(6.)).py(px(4.)).rounded(px(5.)).cursor_pointer().text_color(if pinned { accent } else { muted }).hover(|button| button.bg(Palette::color(&p.elevated))).on_click(cx.listener(move |this, _, window, cx| this.toggle_note_pin(id, window, cx))).child(if pinned { "★" } else { "☆" }),
                                                        div().id("organizer-note-detail-to-task").px(px(6.)).py(px(4.)).rounded(px(5.)).cursor_pointer().text_color(muted).hover(|button| button.text_color(accent)).on_click(cx.listener(move |this, _, window, cx| this.convert_note_to_task(id, window, cx))).child("✓"),
                                                        div().id("organizer-note-detail-trash").px(px(6.)).py(px(4.)).rounded(px(5.)).cursor_pointer().text_color(muted).hover(|button| button.text_color(rgb(0xe5534b))).on_click(cx.listener(move |this, _, window, cx| this.trash_note(id, window, cx))).child("▣"),
                                                    ]
                                                }).unwrap_or_default())
                                                .child(
                                                    div()
                                                        .id("organizer-note-detail-close")
                                                        .px(px(8.))
                                                        .py(px(4.))
                                                        .rounded(px(5.))
                                                        .cursor_pointer()
                                                        .text_color(muted)
                                                        .hover(|button| button.bg(Palette::color(&p.elevated)))
                                                        .on_click(cx.listener(|this, _, window, cx| this.close_selected_note(window, cx)))
                                                        .child("×"),
                                                ),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .text_color(muted)
                                        .child("标签")
                                        .child(
                                            div()
                                                .flex_1()
                                                .h(px(30.))
                                                .rounded(px(5.))
                                                .border_1()
                                                .border_color(Palette::color(&p.border))
                                                .px(px(8.))
                                                .child(self.organizer_tags_input.clone()),
                                        ),
                                )
                                .child(div().id("organizer-note-body-scroll").flex_1().min_h_0().overflow_y_scroll().child(self.note_body_input.clone())),
                        )
                    }),
                );
        let page_content = if self.organizer_open {
            organizer_view
        } else {
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
                        .pt(px(72.))
                        .pb(px(128.))
                        .flex()
                        .flex_col()
                        .when_some(cover_path, |page, path| {
                            page.child(
                                img(path)
                                    .w_full()
                                    .h(px(180.))
                                    .object_fit(ObjectFit::Cover)
                                    .rounded(px(8.))
                                    .mb(px(16.)),
                            )
                        })
                        .child(
                            div()
                                .text_size(px(38.))
                                .font_weight(FontWeight::BOLD)
                                .mb(px(16.))
                                .child(self.title_input.clone()),
                        )
                        .when(self.session.page.locked, |d| {
                            d.child(div().text_color(muted).mb(px(12.)).child("此页面已锁定"))
                        })
                        .when(self.backlinks_open, |d| {
                            d.child(
                                div()
                                    .id("page-backlinks-panel")
                                    .w_full()
                                    .max_h(px(220.))
                                    .overflow_y_scroll()
                                    .mb(px(12.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(Palette::color(&p.border))
                                    .bg(Palette::color(&p.elevated))
                                    .px(px(8.))
                                    .py(px(6.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .px(px(9.))
                                            .py(px(4.))
                                            .text_size(px(11.))
                                            .text_color(muted)
                                            .child(format!("引用此页面的内容 · {backlinks_count}")),
                                    )
                                    .when(backlinks_empty, |panel| {
                                        panel.child(
                                            div()
                                                .px(px(9.))
                                                .py(px(7.))
                                                .text_color(muted)
                                                .child("暂无反向链接"),
                                        )
                                    })
                                    .children(backlink_rows),
                            )
                        })
                        .children(rows),
                )
        };
        div()
            .key_context("Quire")
            .size_full()
            .relative()
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
            .on_action(cx.listener(Self::open_find))
            .on_action(cx.listener(Self::find_next_action))
            .on_action(cx.listener(Self::find_previous_action))
            .on_action(cx.listener(Self::open_search))
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .on_action(cx.listener(Self::zoom_reset))
            .when_some(self.transition_palette.clone(), |root, old_palette| {
                root.child(
                    div()
                        .id("theme-transition-overlay")
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .left_0()
                        .bg(linear_gradient(
                            180.,
                            linear_color_stop(Palette::color(&old_palette.bg), 0.),
                            linear_color_stop(Palette::color(&old_palette.grad2), 1.),
                        )
                        .color_space(ColorSpace::Srgb))
                        .with_animation(
                            "theme-fade",
                            Animation::new(Duration::from_millis(220)),
                            |overlay, progress| overlay.opacity(1. - progress),
                        ),
                )
            })
            .child(
                div()
                    .h(px(40.))
                    .flex_shrink_0()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .px(px(8.))
                    .border_b_1()
                    .border_color(Palette::color(&p.border))
                    .flex()
                    .items_center()
                    // The reference titlebar's rail-colored strip joins the
                    // sidebar into one continuous surface.
                    .child({
                        let rail = div()
                            .id("sidebar-rail")
                            .absolute()
                            .left_0()
                            .top_0()
                            .h(px(40.))
                            .bg(Palette::color(&p.elevated));
                        if let Some((key, from, to)) = sidebar_transition {
                            rail.w(px(from))
                                .with_animation(
                                    ("sidebar-rail", key),
                                    Animation::new(Duration::from_millis(220)),
                                    move |rail, progress| rail.w(px(from + (to - from) * progress)),
                                )
                                .into_any_element()
                        } else {
                            rail.w(px(if self.sidebar_open {
                                self.sidebar_width
                            } else {
                                0.
                            }))
                            .into_any_element()
                        }
                    })
                    .child(
                        div()
                            .id("sidebar-toggle")
                            .w(px(30.))
                            .h(px(30.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.bg(Palette::color(&p.elevated)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_sidebar(cx);
                            }))
                            .child("◫"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .px(px(6.))
                            .gap(px(7.))
                            .text_color(muted)
                            .window_control_area(WindowControlArea::Drag)
                            .child("工作区")
                            .child("›")
                            .child(self.session.page.title.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .id("sidebar-search-toggle")
                                    .px(px(7.))
                                    .py(px(4.))
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .text_color(muted)
                                    .hover(|button| button.bg(Palette::color(&p.elevated)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_search(&SearchPages, window, cx);
                                    }))
                                    .child("⌕"),
                            )
                            .child(div().w(px(1.)).h(px(18.)).bg(Palette::color(&p.border)))
                            .child(
                                div()
                                    .id("window-minimize")
                                    .w(px(38.))
                                    .h(px(40.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(muted)
                                    .window_control_area(WindowControlArea::Min)
                                    .child("−"),
                            )
                            .child(
                                div()
                                    .id("window-maximize")
                                    .w(px(38.))
                                    .h(px(40.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(muted)
                                    .window_control_area(WindowControlArea::Max)
                                    .child("□"),
                            )
                            .child(
                                div()
                                    .id("window-close")
                                    .w(px(38.))
                                    .h(px(40.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(muted)
                                    .hover(|button| {
                                        button.bg(rgb(0xc42b1c)).text_color(rgb(0xffffff))
                                    })
                                    .window_control_area(WindowControlArea::Close)
                                    .child("×"),
                            ),
                    )
                    .when(self.page_options_open, |header| {
                        header.child(
                            div()
                                .id("page-options-popover")
                                .absolute()
                                .top(px(40.))
                                .right(px(142.))
                                .w(px(720.))
                                .flex_wrap()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .p(px(8.))
                                .rounded(px(8.))
                                .border_1()
                                .border_color(Palette::color(&p.border))
                                .bg(Palette::color(&p.elevated))
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
                                            cx.listener(|this, _, _, cx| {
                                                this.toggle_icon_picker(cx)
                                            }),
                                        )
                                        .child(format!(
                                            "图标 · {}",
                                            if self.session.page.icon.is_empty() {
                                                "选择"
                                            } else {
                                                &self.session.page.icon
                                            }
                                        ))
                                        .when(self.icon_picker_open, |button| {
                                            button.child(
                                                anchored()
                                                    .anchor(Corner::BottomLeft)
                                                    .offset(point(px(0.), px(5.)))
                                                    .snap_to_window()
                                                    .child(icon_options),
                                            )
                                        }),
                                )
                                .child(
                                    div()
                                        .id("page-cover-picker")
                                        .px(px(7.))
                                        .py(px(4.))
                                        .rounded(px(5.))
                                        .cursor_pointer()
                                        .text_color(Palette::color(&p.accent))
                                        .hover(|s| s.bg(Palette::color(&p.elevated)))
                                        .when(!self.session.page.locked, |button| {
                                            button.on_click(cx.listener(|this, _, window, cx| {
                                                this.pick_page_cover(window, cx)
                                            }))
                                        })
                                        .child(if self.session.page.cover.is_some() {
                                            "更换封面"
                                        } else {
                                            "设置封面"
                                        }),
                                )
                                .when(self.session.page.cover.is_some(), |bar| {
                                    bar.child(
                                        div()
                                            .id("page-cover-clear")
                                            .px(px(6.))
                                            .py(px(4.))
                                            .rounded(px(5.))
                                            .cursor_pointer()
                                            .text_color(muted)
                                            .hover(|s| s.bg(Palette::color(&p.elevated)))
                                            .when(!self.session.page.locked, |button| {
                                                button.on_click(cx.listener(
                                                    |this, _, window, cx| {
                                                        this.clear_page_cover(window, cx)
                                                    },
                                                ))
                                            })
                                            .child("清除"),
                                    )
                                })
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
                                        .id("page-backlinks-toggle")
                                        .px(px(7.))
                                        .py(px(4.))
                                        .rounded(px(5.))
                                        .cursor_pointer()
                                        .text_color(if self.backlinks_open {
                                            accent
                                        } else {
                                            muted
                                        })
                                        .hover(|s| s.bg(Palette::color(&p.elevated)))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.backlinks_open = !this.backlinks_open;
                                            cx.notify();
                                        }))
                                        .child(format!("反向链接 · {backlinks_count}")),
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
                                        .child(format!(
                                            "字体 · {}",
                                            self.session.page.font.label()
                                        )),
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
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.toggle_theme_picker(window, cx)
                                        }))
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
                        )
                    }),
            )
            .when(!self.organizer_open && self.format_bar_open, |root| {
                root.child(
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
                        .children(mark_buttons)
                        .child(
                            div()
                                .id("link-editor-toggle")
                                .px(px(8.))
                                .py(px(5.))
                                .rounded(px(5.))
                                .cursor_pointer()
                                .text_color(Palette::color(&p.accent))
                                .hover(|button| button.bg(Palette::color(&p.elevated)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.toggle_link_editor(window, cx)
                                }))
                                .child("链接"),
                        )
                        .children(attachment_buttons)
                        .child(
                            div()
                                .id("page-import-markdown")
                                .px(px(8.))
                                .py(px(5.))
                                .rounded(px(5.))
                                .cursor_pointer()
                                .text_color(Palette::color(&p.accent))
                                .hover(|button| button.bg(Palette::color(&p.elevated)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.import_markdown(window, cx)
                                }))
                                .child("导入 Markdown"),
                        )
                        .child(
                            div()
                                .id("page-export-markdown")
                                .px(px(8.))
                                .py(px(5.))
                                .rounded(px(5.))
                                .cursor_pointer()
                                .text_color(Palette::color(&p.accent))
                                .hover(|button| button.bg(Palette::color(&p.elevated)))
                                .on_click(cx.listener(|this, _, _, cx| this.export_markdown(cx)))
                                .child("导出 Markdown"),
                        ),
                )
            })
            .child(link_editor)
            .child(find_bar)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(self.sidebar_open || sidebar_transition.is_some(), |body| {
                        let sidebar = div()
                            .id("sidebar-panel")
                            .relative()
                            .w(px(self.sidebar_width))
                            .flex_shrink_0()
                            .h_full()
                            .overflow_hidden()
                            .bg(Palette::color(&p.elevated))
                            .px(px(8.))
                            .py(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(0.))
                            .child(
                                div()
                                    .id("sidebar-workspace-header")
                                    .h(px(36.))
                                    .px(px(4.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .size(px(20.))
                                            .rounded(px(6.))
                                            .bg(accent)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_color(Palette::color(&p.bg))
                                            .font_weight(FontWeight::BOLD)
                                            .child("Q"),
                                    )
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child("工作区"))
                                    .child(div().flex_1())
                                    .child(sidebar_glyph(
                                        SidebarGlyph::ChevronDown,
                                        muted,
                                        px(16.),
                                    )),
                            )
                            .child(
                                div()
                                    .id("sidebar-organizer-notes")
                                    .h(px(28.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .text_color(if self.organizer_open && !self.organizer_tasks {
                                        text
                                    } else {
                                        Palette::color(&p.fg).opacity(0.78)
                                    })
                                    .when(self.organizer_open && !self.organizer_tasks, |row| {
                                        row.bg(Palette::color(&p.elevated2).opacity(0.55))
                                    })
                                    .hover(|row| row.bg(Palette::color(&p.elevated2).opacity(0.45)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if !this.organizer_open {
                                            this.toggle_organizer(window, cx);
                                        }
                                        this.toggle_organizer_tab(false, window, cx);
                                    }))
                                    .child(
                                        div()
                                            .w(px(16.))
                                            .h(px(16.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(sidebar_glyph(
                                                SidebarGlyph::Note,
                                                if self.organizer_open && !self.organizer_tasks {
                                                    text
                                                } else {
                                                    muted
                                                },
                                                px(16.),
                                            )),
                                    )
                                    .child(div().text_size(px(13.)).child("笔记")),
                            )
                            .child(
                                div()
                                    .id("sidebar-organizer-tasks")
                                    .h(px(28.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .text_color(if self.organizer_open && self.organizer_tasks {
                                        text
                                    } else {
                                        Palette::color(&p.fg).opacity(0.78)
                                    })
                                    .when(self.organizer_open && self.organizer_tasks, |row| {
                                        row.bg(Palette::color(&p.elevated2).opacity(0.55))
                                    })
                                    .hover(|row| row.bg(Palette::color(&p.elevated2).opacity(0.45)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if !this.organizer_open {
                                            this.toggle_organizer(window, cx);
                                        }
                                        this.toggle_organizer_tab(true, window, cx);
                                    }))
                                    .child(
                                        div()
                                            .w(px(16.))
                                            .h(px(16.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(sidebar_glyph(
                                                SidebarGlyph::TodoCheck,
                                                if self.organizer_open && self.organizer_tasks {
                                                    text
                                                } else {
                                                    muted
                                                },
                                                px(16.),
                                            )),
                                    )
                                    .child(div().text_size(px(13.)).child("任务")),
                            )
                            .child(
                                div()
                                    .id("sidebar-search")
                                    .h(px(28.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .text_color(muted)
                                    .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.3)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_search(&SearchPages, window, cx);
                                    }))
                                    .child(
                                        div()
                                            .w(px(16.))
                                            .h(px(16.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(sidebar_glyph(
                                                SidebarGlyph::Search,
                                                muted,
                                                px(16.),
                                            )),
                                    )
                                    .child(div().text_size(px(13.)).child("搜索"))
                                    .child(div().flex_1())
                                    .child(
                                        div().text_size(px(11.)).text_color(muted).child("Ctrl P"),
                                    ),
                            )
                            .child(
                                div()
                                    .id("sidebar-settings")
                                    .h(px(28.))
                                    .px(px(8.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .text_color(muted)
                                    .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.3)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_theme_picker(window, cx);
                                    }))
                                    .child(
                                        div()
                                            .w(px(16.))
                                            .h(px(16.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(sidebar_glyph(
                                                SidebarGlyph::Settings,
                                                muted,
                                                px(16.),
                                            )),
                                    )
                                    .child(div().text_size(px(13.)).child("设置")),
                            )
                            .when_some(delete_confirmation, |sidebar, confirmation| {
                                sidebar.child(confirmation)
                            })
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .mt(px(10.))
                                    .child("工作区"),
                            )
                            .child(
                                div()
                                    .id("page-list-scroll")
                                    .flex_1()
                                    .h_full()
                                    .overflow_y_scroll()
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
                            .child(div().h(px(8.)))
                            .child(div().h(px(1.)).bg(Palette::color(&p.border)))
                            .child(
                                div()
                                    .h(px(22.))
                                    .px(px(28.))
                                    .flex()
                                    .items_center()
                                    .text_size(px(11.))
                                    .text_color(muted)
                                    .child(env!("CARGO_PKG_VERSION")),
                            )
                            .child(
                                div()
                                    .id("sidebar-resize-handle")
                                    .absolute()
                                    .top_0()
                                    .right_0()
                                    .bottom_0()
                                    .w(px(6.))
                                    .cursor(CursorStyle::ResizeLeftRight)
                                    .hover(|handle| {
                                        handle.bg(Palette::color(&p.border).opacity(0.8))
                                    })
                                    .when(self.sidebar_resizing, |handle| {
                                        handle.bg(Palette::color(&p.border).opacity(0.8))
                                    })
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                            let zoom = quire_gpui::ui_scale::get();
                                            this.sidebar_drag_offset = this.sidebar_width
                                                - f32::from(event.position.x) / zoom;
                                            this.sidebar_resizing = true;
                                            cx.notify();
                                            let workspace = cx.entity();
                                            window.on_mouse_event({
                                                let workspace = workspace.clone();
                                                move |event: &MouseMoveEvent, _, _, cx| {
                                                    if !event.dragging() {
                                                        return;
                                                    }
                                                    workspace.update(cx, |this, cx| {
                                                        if this.sidebar_resizing {
                                                            let zoom = quire_gpui::ui_scale::get();
                                                            this.sidebar_width =
                                                                (f32::from(event.position.x)
                                                                    / zoom
                                                                    + this.sidebar_drag_offset)
                                                                    .clamp(200., 480.);
                                                            cx.notify();
                                                        }
                                                    });
                                                }
                                            });
                                            window.on_mouse_event({
                                                move |_: &MouseUpEvent, _, window, cx| {
                                                    workspace.update(cx, |this, cx| {
                                                        if this.sidebar_resizing {
                                                            this.sidebar_resizing = false;
                                                            this.session.set_setting(
                                                                "gpui.sidebar-width",
                                                                format!(
                                                                    "{:.1}",
                                                                    this.sidebar_width
                                                                ),
                                                            );
                                                            this.schedule_save(true, window, cx);
                                                            cx.notify();
                                                        }
                                                    });
                                                }
                                            });
                                        }),
                                    ),
                            );
                        if let Some((key, from, to)) = sidebar_transition {
                            body.child(sidebar.with_animation(
                                ("sidebar-panel", key),
                                Animation::new(Duration::from_millis(220)),
                                move |sidebar, progress| {
                                    sidebar.w(px(from + (to - from) * progress))
                                },
                            ))
                        } else {
                            body.child(sidebar)
                        }
                    })
                    .child(page_content),
            )
            .when(!self.organizer_open, |root| {
                root.child(
                    div()
                        .absolute()
                        .bottom(px(7.))
                        .right(px(12.))
                        .text_size(px(11.))
                        .text_color(if self.error {
                            rgb(0xe5534b).into()
                        } else {
                            muted
                        })
                        .child(if self.status != "已保存" {
                            self.status.clone()
                        } else {
                            format!(
                                "{} words · {} chars",
                                self.session
                                    .blocks()
                                    .iter()
                                    .map(|block| block.text.split_whitespace().count())
                                    .sum::<usize>(),
                                self.session
                                    .blocks()
                                    .iter()
                                    .map(|block| block.text.chars().count())
                                    .sum::<usize>()
                            )
                        }),
                )
            })
            .when(self.search_open, |root| root.child(search_overlay))
            .when_some(settings_overlay, |root, overlay| root.child(overlay))
    }
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-s", Save, Some("Quire")),
        KeyBinding::new("ctrl-z", Undo, Some("Quire")),
        KeyBinding::new("ctrl-shift-z", Redo, Some("Quire")),
        KeyBinding::new("ctrl-y", Redo, Some("Quire")),
        KeyBinding::new("ctrl-f", Find, Some("Quire")),
        KeyBinding::new("ctrl-g", FindNext, Some("Quire")),
        KeyBinding::new("ctrl-shift-g", FindPrevious, Some("Quire")),
        KeyBinding::new("ctrl-p", SearchPages, Some("Quire")),
        KeyBinding::new("ctrl-=", ZoomIn, Some("Quire")),
        KeyBinding::new("ctrl--", ZoomOut, Some("Quire")),
        KeyBinding::new("ctrl-0", ZoomReset, Some("Quire")),
    ]);
}
