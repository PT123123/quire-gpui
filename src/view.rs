use crate::input::{InputEvent, TextInput};
use gpui::{AnimationExt, prelude::*, *};
use quire_core::core::{
    command::Command,
    database::ViewId,
    organizer::{OrganizerCatalog, Priority, Repeat, Task as OrganizerTask},
    types::{Block, BlockId, BlockKind, ColorKind, Lang, Mark, MarkKind},
};
use quire_gpui::session::Session;
use quire_gpui::ui_scale::px;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    ops::Range,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Copy)]
struct DatabaseColumnDrag {
    block: BlockId,
    db: quire_core::core::database::DatabaseId,
    view: ViewId,
    property: quire_core::core::database::PropertyId,
    start_x: f32,
    start_width: f32,
    grid_width: f32,
    preview_width: f32,
}

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
        QuickNote,
        OpenCommandPalette,
        NewPage,
        Undo,
        Redo,
        Find,
        FindNext,
        FindPrevious,
        SearchPages,
        NavigateBack,
        NavigateForward,
        ZoomIn,
        ZoomOut,
        ZoomReset
    ]
);

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockMenuMode {
    Actions,
    Convert,
    MoveToPage,
}

#[derive(Clone, Copy)]
enum PaletteCommand {
    NewPage,
    SearchPages,
    ToggleSidebar,
    ToggleTheme,
    Settings,
    OpenNotes,
    OpenTasks,
    RenamePage,
    DuplicatePage,
    DeletePage,
    ExportMarkdown,
    ImportMarkdown,
    CopyMarkdown,
    SaveCurrentAsTemplate,
    ImportTemplate,
    InsertTemplate(quire_core::core::types::PageId),
    NewPageFromTemplate(quire_core::core::types::PageId),
    DeleteTemplate(quire_core::core::types::PageId),
    ExportTemplateMarkdown(quire_core::core::types::PageId),
    NavigateBack,
    NavigateForward,
    OpenHtml,
    OpenPage(quire_core::core::types::PageId),
}

struct PaletteItem {
    name: String,
    hint: &'static str,
    section: &'static str,
    icon: &'static str,
    action: PaletteCommand,
}

fn palette_matches(query: &str, name: &str, section: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let name = name.to_lowercase();
    let section = section.to_lowercase();
    query
        .split_whitespace()
        .all(|term| name.contains(term) || section.contains(term))
}

fn push_history<T: Copy + PartialEq>(history: &mut Vec<T>, value: T) {
    const LIMIT: usize = 50;
    if history.last() != Some(&value) {
        history.push(value);
        if history.len() > LIMIT {
            history.remove(0);
        }
    }
}

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

fn database_kind_label(kind: quire_core::core::database::PropertyKind) -> &'static str {
    use quire_core::core::database::PropertyKind as Kind;
    match kind {
        Kind::Title => "标题",
        Kind::Text => "文本",
        Kind::Number => "数字",
        Kind::Select => "单选",
        Kind::MultiSelect => "多选",
        Kind::Status => "状态",
        Kind::Date => "日期",
        Kind::Checkbox => "复选框",
        Kind::Url => "网址",
        Kind::Email => "邮箱",
        Kind::Phone => "电话",
        Kind::Files => "文件",
        Kind::CreatedTime => "创建时间",
        Kind::LastEditedTime => "最后编辑",
        Kind::Formula => "公式",
        Kind::Rollup => "汇总",
        Kind::Relation => "关联",
    }
}

fn database_filter_op_label(
    op: quire_core::core::database_view::FilterOp,
    kind: quire_core::core::database::PropertyKind,
) -> &'static str {
    use quire_core::core::database::PropertyKind as Kind;
    use quire_core::core::database_view::FilterOp as Op;
    match op {
        Op::Contains if matches!(kind, Kind::MultiSelect | Kind::Files) => "包含项",
        Op::Contains => "包含",
        Op::Eq => "是",
        Op::Ne => "不是",
        Op::Gt if matches!(kind, Kind::Date | Kind::CreatedTime | Kind::LastEditedTime) => "晚于",
        Op::Gt => ">",
        Op::Gte if matches!(kind, Kind::Date | Kind::CreatedTime | Kind::LastEditedTime) => {
            "不早于"
        }
        Op::Gte => "≥",
        Op::Lt if matches!(kind, Kind::Date | Kind::CreatedTime | Kind::LastEditedTime) => "早于",
        Op::Lt => "<",
        Op::Lte if matches!(kind, Kind::Date | Kind::CreatedTime | Kind::LastEditedTime) => {
            "不晚于"
        }
        Op::Lte => "≤",
        Op::AnyOf if matches!(kind, Kind::MultiSelect | Kind::Files) => "包含任一",
        Op::AnyOf => "属于",
        Op::IsEmpty => "为空",
        Op::IsNotEmpty => "不为空",
    }
}

fn database_filter_value_from_text(
    kind: quire_core::core::database::PropertyKind,
    text: &str,
) -> Option<quire_core::core::database_view::FilterValue> {
    use quire_core::core::database::PropertyKind as Kind;
    use quire_core::core::database_view::FilterValue;
    match kind {
        Kind::Number => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(FilterValue::Number),
        Kind::Date | Kind::CreatedTime | Kind::LastEditedTime => {
            quire_core::core::database_view::is_stored_date(text)
                .then(|| FilterValue::Text(text.to_owned()))
        }
        _ => Some(FilterValue::Text(text.to_owned())),
    }
}

fn order_database_groups(
    mut groups: Vec<(quire_core::core::database_view::GroupKey, usize)>,
    config: &str,
) -> Vec<(quire_core::core::database_view::GroupKey, usize)> {
    use quire_core::core::database_view::GroupKey;
    let options = quire_core::core::database_property::PropertyOptions::from_config(config);
    let rank = |key: &GroupKey| -> (u64, String) {
        match key {
            GroupKey::Unchecked => (0, String::new()),
            GroupKey::Checked => (1, String::new()),
            GroupKey::Option(id) => match options
                .iter()
                .position(|option| option.id.to_string() == *id)
            {
                Some(index) => (2 + index as u64, String::new()),
                None => (u64::MAX / 2, id.clone()),
            },
            GroupKey::Empty => (u64::MAX, String::new()),
        }
    };
    groups.sort_by(|(left, _), (right, _)| rank(left).cmp(&rank(right)));
    groups
}

fn database_group_label(key: &quire_core::core::database_view::GroupKey, config: &str) -> String {
    use quire_core::core::database_view::GroupKey;
    match key {
        GroupKey::Empty => "无值".into(),
        GroupKey::Unchecked => "未勾选".into(),
        GroupKey::Checked => "已勾选".into(),
        GroupKey::Option(id) => {
            quire_core::core::database_property::PropertyOptions::from_config(config)
                .iter()
                .find(|option| option.id.to_string() == *id)
                .map(|option| option.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| id.clone())
        }
    }
}

fn database_group_identity(key: &quire_core::core::database_view::GroupKey) -> String {
    use quire_core::core::database_view::GroupKey;
    match key {
        GroupKey::Empty => "empty".into(),
        GroupKey::Unchecked => "unchecked".into(),
        GroupKey::Checked => "checked".into(),
        GroupKey::Option(id) => format!("option:{id}"),
    }
}

fn evaluate_database_formula(
    property: quire_core::core::database::PropertyId,
    properties: &[quire_core::core::database::Property],
    repo: &quire_core::storage::SqliteRepository,
    record: quire_core::core::database::RecordId,
    depth: u32,
) -> Result<quire_core::core::database_formula::Val, quire_core::core::database_formula::FormulaError>
{
    use quire_core::core::database::PropertyKind;
    use quire_core::core::database_formula::{FormulaError, Program, Val};
    let definition = properties
        .iter()
        .find(|candidate| candidate.id == property)
        .ok_or_else(|| FormulaError::Eval("formula property no longer exists".into()))?;
    let source =
        quire_core::core::database_formula::config_formula(&definition.config).unwrap_or_default();
    if source.is_empty() {
        return Ok(Val::Empty);
    }
    let program = Program::parse(&source, |name| {
        properties
            .iter()
            .find(|candidate| candidate.name == name)
            .map(|candidate| candidate.id)
    })?;
    program.eval_at(depth, &mut |dependency, next_depth| {
        let definition = properties
            .iter()
            .find(|candidate| candidate.id == dependency)
            .ok_or_else(|| FormulaError::Eval("formula dependency no longer exists".into()))?;
        if definition.kind == PropertyKind::Formula {
            evaluate_database_formula(dependency, properties, repo, record, next_depth)
        } else {
            let value = repo
                .cell(record, dependency)
                .map_err(|error| FormulaError::Eval(error.to_string()))?;
            Ok(quire_core::core::database_formula::val_of(
                definition.kind,
                &value,
            ))
        }
    })
}

fn evaluate_database_rollup(
    property: quire_core::core::database::PropertyId,
    properties: &[quire_core::core::database::Property],
    repo: &quire_core::storage::SqliteRepository,
    record: quire_core::core::database::RecordId,
) -> Result<quire_core::core::database_formula::Val, quire_core::core::database_formula::FormulaError>
{
    use quire_core::core::database::{CellValue, PropertyKind};
    use quire_core::core::database_formula::{FormulaError, Val};
    let definition = properties
        .iter()
        .find(|candidate| candidate.id == property)
        .ok_or_else(|| FormulaError::Eval("rollup column no longer exists".into()))?;
    let config = quire_core::core::database_rollup::config_rollup(&definition.config);
    let Some(relation_id) = config.relation else {
        return Ok(Val::Empty);
    };
    let relation = properties
        .iter()
        .find(|candidate| candidate.id == relation_id)
        .ok_or_else(|| FormulaError::Eval("rollup relation no longer exists".into()))?;
    let targets = match repo
        .cell(record, relation_id)
        .map_err(|error| FormulaError::Eval(error.to_string()))?
    {
        CellValue::Items(ids) => ids,
        _ => Vec::new(),
    };
    let mut values = Vec::new();
    if config.aggregate.reads_column() {
        let Some(column_id) = config.column else {
            return Ok(Val::Empty);
        };
        let column = properties
            .iter()
            .find(|candidate| candidate.id == column_id)
            .ok_or_else(|| FormulaError::Eval("rollup target column no longer exists".into()))?;
        if column.kind.is_computed() || column.kind == PropertyKind::Relation {
            return Err(FormulaError::Eval(
                "rollup target column is not aggregatable".into(),
            ));
        }
        for target in &targets {
            let Ok(target_id) = target.parse::<u64>() else {
                continue;
            };
            let value = repo
                .cell(quire_core::core::database::RecordId(target_id), column_id)
                .map_err(|error| FormulaError::Eval(error.to_string()))?;
            if value != CellValue::Empty {
                values.push(quire_core::core::database_formula::val_of(
                    column.kind,
                    &value,
                ));
            }
        }
    }
    let _ = relation;
    quire_core::core::database_rollup::fold(config.aggregate, &values, targets.len())
        .map_err(|error| FormulaError::Eval(error.to_string()))
}

fn database_property_kind_choices() -> [quire_core::core::database::PropertyKind; 15] {
    use quire_core::core::database::PropertyKind as Kind;
    [
        Kind::Text,
        Kind::Number,
        Kind::Select,
        Kind::Status,
        Kind::Date,
        Kind::Checkbox,
        Kind::Url,
        Kind::Email,
        Kind::Phone,
        Kind::CreatedTime,
        Kind::LastEditedTime,
        Kind::Formula,
        Kind::MultiSelect,
        Kind::Relation,
        Kind::Rollup,
    ]
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

fn organizer_tag_matches_path(candidate: &str, path: &str) -> bool {
    path.is_empty() || candidate == path || candidate.starts_with(&format!("{path}/"))
}

/// Direct children at the current level of a hierarchical organizer tag tree.
/// A note/task is counted once under each ancestor even when several of its tags
/// share that ancestor, matching the reference organizer's subtree counts.
fn organizer_tag_tree_rows(items_tags: &[Vec<String>], path: &str) -> Vec<(String, usize, bool)> {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for tags in items_tags {
        let mut prefixes = std::collections::BTreeSet::new();
        for tag in tags {
            let mut prefix = String::new();
            for segment in tag.split('/').filter(|segment| !segment.is_empty()) {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(segment);
                prefixes.insert(prefix.clone());
            }
        }
        for prefix in prefixes {
            *counts.entry(prefix).or_default() += 1;
        }
    }
    let paths = counts
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut rows = counts
        .into_iter()
        .filter_map(|(full, count)| {
            let parent = full
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .unwrap_or("");
            (parent == path).then(|| {
                let has_children = paths
                    .iter()
                    .any(|candidate| candidate.starts_with(&format!("{full}/")));
                (full, count, has_children)
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    rows.into_iter()
        .map(|(path, count, has_children)| {
            let name = path.rsplit('/').next().unwrap_or_default().to_owned();
            (name, count, has_children)
        })
        .collect()
}

fn organizer_tags_exclude(tags: &[String], excluded: &BTreeSet<String>) -> bool {
    excluded
        .iter()
        .any(|path| tags.iter().any(|tag| organizer_tag_matches_path(tag, path)))
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

fn parse_mcp_port(value: &str) -> Option<u16> {
    let port = value.trim().parse::<u16>().ok()?;
    (port != 0).then_some(port)
}

fn sync_targets(
    peers: Vec<quire_core::services::sync::engine::PeerRecord>,
    now: u64,
    pending: &HashSet<String>,
) -> Vec<quire_core::services::sync::engine::PeerRecord> {
    peers
        .into_iter()
        .filter(|peer| now.saturating_sub(peer.last_seen) <= 60 && !pending.contains(&peer.id))
        .collect()
}

#[cfg(test)]
mod view_tests {
    use super::{
        capture_tag_replacement, capture_tags, database_filter_value_from_text,
        database_group_identity, database_group_label, database_property_kind_choices,
        evaluate_database_formula, evaluate_database_rollup, find_matches_in, link_target_range,
        order_database_groups, organizer_tag_matches_path, organizer_tag_tree_rows,
        organizer_tags_exclude, palette_matches, parse_mcp_port, parse_organizer_tags,
        push_history, sync_targets, toc_headings,
    };
    use quire_core::core::database::PropertyKind;
    use quire_core::core::database_view::FilterValue;
    use quire_core::core::types::{
        Block, BlockId, BlockKind, ColorKind, Lang, Mark, MarkKind, OrderKey, PageId,
    };

    #[test]
    fn organizer_tag_entry_trims_deduplicates_and_accepts_fullwidth_commas() {
        assert_eq!(
            parse_organizer_tags(" 设计,复刻， 设计 ,,桌面端 "),
            ["设计", "复刻", "桌面端"]
        );
    }

    #[test]
    fn organizer_tag_filters_follow_hierarchy_and_keep_sibling_names_distinct() {
        assert!(organizer_tag_matches_path("项目/工作", "项目"));
        assert!(organizer_tag_matches_path("项目", "项目"));
        assert!(!organizer_tag_matches_path("项目2/工作", "项目"));
        assert!(organizer_tags_exclude(
            &["项目/工作".into(), "读书".into()],
            &["项目".into()].into()
        ));
        assert!(!organizer_tags_exclude(
            &["项目2/工作".into()],
            &["项目".into()].into()
        ));
    }

    #[test]
    fn organizer_tag_tree_shows_direct_children_with_unique_subtree_counts() {
        let tags = [
            vec!["项目/工作/GPUI".to_owned(), "项目/工作/Slint".to_owned()],
            vec!["项目/个人".to_owned()],
            vec!["项目/个人".to_owned()],
            vec!["读书".to_owned()],
        ];
        assert_eq!(
            organizer_tag_tree_rows(&tags, ""),
            [("项目".to_owned(), 3, true), ("读书".to_owned(), 1, false)]
        );
        assert_eq!(
            organizer_tag_tree_rows(&tags, "项目"),
            [("个人".to_owned(), 2, false), ("工作".to_owned(), 1, true)]
        );
        assert_eq!(
            organizer_tag_tree_rows(&tags, "项目/工作"),
            [
                ("GPUI".to_owned(), 1, false),
                ("Slint".to_owned(), 1, false)
            ]
        );
    }

    #[test]
    fn quick_capture_extracts_reference_hash_tags_without_changing_body() {
        assert_eq!(
            capture_tags("记一下 #工作/项目，#工作/项目 #2026 #计划。"),
            ["工作/项目", "计划"]
        );
        assert!(capture_tags("普通文字 # 不是标签").is_empty());
    }

    #[test]
    fn quick_capture_suggestion_replaces_the_current_utf8_tag_token() {
        let draft = "记下 #工后 继续";
        let caret = "记下 #工".len();
        assert_eq!(
            capture_tag_replacement(draft, caret, "项目/工作"),
            Some(("记下 #项目/工作 继续".into(), "记下 #项目/工作 ".len()))
        );
        assert_eq!(
            capture_tag_replacement("记下 #工，继续", "记下 #工".len(), "项目/工作"),
            Some(("记下 #项目/工作，继续".into(), "记下 #项目/工作".len()))
        );
        assert_eq!(capture_tag_replacement("没有标签", 4, "项目"), None);
    }

    #[test]
    fn mcp_port_accepts_only_nonzero_u16_values() {
        assert_eq!(parse_mcp_port(" 5880 "), Some(5880));
        assert_eq!(parse_mcp_port("65535"), Some(65535));
        assert_eq!(parse_mcp_port("0"), None);
        assert_eq!(parse_mcp_port("65536"), None);
        assert_eq!(parse_mcp_port("http"), None);
    }

    #[test]
    fn sync_target_selection_includes_fresh_boundary_and_skips_stale_or_in_flight_peers() {
        use quire_core::services::sync::engine::PeerRecord;

        let now = 1_000;
        let peer = |id: &str, age| PeerRecord {
            id: id.into(),
            name: id.into(),
            kind: "windows".into(),
            ip: "127.0.0.1".into(),
            port: 5878,
            paired: true,
            last_seen: now - age,
            last_sync: String::new(),
        };
        let peers = vec![peer("boundary", 60), peer("stale", 61), peer("pending", 1)];
        let selected = sync_targets(peers, now, &["pending".to_owned()].into());
        assert_eq!(
            selected
                .iter()
                .map(|peer| peer.id.as_str())
                .collect::<Vec<_>>(),
            ["boundary"]
        );
    }

    #[test]
    fn command_palette_filters_names_and_sections_without_losing_chinese_matches() {
        assert!(palette_matches(" 设 置 ", "设置", "导航"));
        assert!(palette_matches("跳转", "项目计划", "跳转到页面"));
        assert!(!palette_matches("附件", "设置", "导航"));
        assert!(palette_matches("", "任意命令", "界面"));
    }

    #[test]
    fn page_history_suppresses_consecutive_duplicates_and_keeps_latest_fifty() {
        let mut history = Vec::new();
        push_history(&mut history, 1_u8);
        push_history(&mut history, 1_u8);
        for page in 2..=60 {
            push_history(&mut history, page);
        }
        assert_eq!(history.len(), 50);
        assert_eq!(history.first(), Some(&11));
        assert_eq!(history.last(), Some(&60));
    }

    #[test]
    fn database_group_order_follows_schema_options_and_places_empty_last() {
        use quire_core::core::database_view::GroupKey;

        let groups = vec![
            (GroupKey::Empty, 1),
            (GroupKey::Option("99".into()), 2),
            (GroupKey::Option("7".into()), 3),
            (GroupKey::Option("3".into()), 4),
        ];
        let ordered = order_database_groups(
            groups,
            r#"{"options":[{"id":3,"name":"第一个"},{"id":7,"name":"第二个"}]}"#,
        );
        assert_eq!(
            ordered.iter().map(|(key, _)| key).collect::<Vec<_>>(),
            [
                &GroupKey::Option("3".into()),
                &GroupKey::Option("7".into()),
                &GroupKey::Option("99".into()),
                &GroupKey::Empty,
            ]
        );
        assert_eq!(database_group_label(&GroupKey::Empty, ""), "无值");
        assert_eq!(database_group_identity(&GroupKey::Empty), "empty");
        assert_eq!(
            database_group_identity(&GroupKey::Option("7".into())),
            "option:7"
        );
        assert_eq!(
            database_group_label(
                &GroupKey::Option("7".into()),
                r#"{"options":[{"id":7,"name":"第二个"}]}"#
            ),
            "第二个"
        );
    }

    #[test]
    fn database_type_picker_includes_read_only_time_properties() {
        use quire_core::core::database::PropertyKind as Kind;
        let kinds = database_property_kind_choices();
        assert!(kinds.contains(&Kind::CreatedTime));
        assert!(kinds.contains(&Kind::LastEditedTime));
        assert!(!kinds.contains(&Kind::Title));
    }

    #[test]
    fn database_formula_evaluates_visible_rows_and_reports_nested_values() {
        let scratch = quire_core::testing::ScratchDir::new("gpui-formula-cell");
        let path = scratch.path().join("test.db");
        let mut session = quire_gpui::session::Session::open(&path).expect("session opens");
        let block = session.blocks()[0].id;
        let db = session
            .make_database(block, "公式测试".into())
            .expect("database created");
        let amount = session
            .add_database_property(
                block,
                "数量".into(),
                quire_core::core::database::PropertyKind::Text,
            )
            .expect("amount property created");
        assert!(
            session
                .set_database_property_kind(
                    block,
                    amount,
                    quire_core::core::database::PropertyKind::Number,
                )
                .expect("number type set")
        );
        let total = session
            .add_database_property(
                block,
                "总计".into(),
                quire_core::core::database::PropertyKind::Text,
            )
            .expect("formula property created");
        assert!(
            session
                .set_database_property_kind(
                    block,
                    total,
                    quire_core::core::database::PropertyKind::Formula,
                )
                .expect("formula type set")
        );
        let record = session.add_database_record(block).expect("record created");
        assert!(
            session
                .set_database_cell(block, record, amount, "6")
                .expect("number stored")
        );
        assert!(
            session
                .set_database_formula(block, total, "[数量] * 2")
                .expect("formula stored")
        );
        let properties = session
            .databases
            .properties_of(db)
            .cloned()
            .collect::<Vec<_>>();
        let value = evaluate_database_formula(total, &properties, &session.repo, record, 0)
            .expect("formula evaluates");
        assert_eq!(quire_core::core::database_formula::display(&value), "12");
    }

    #[test]
    fn database_rollup_uses_related_rows_and_typed_values() {
        use quire_core::core::database::{PropertyKind, RecordId};
        use quire_core::core::database_rollup::Aggregate;

        let scratch = quire_core::testing::ScratchDir::new("gpui-rollup-cell");
        let path = scratch.path().join("test.db");
        let mut session = quire_gpui::session::Session::open(&path).expect("session opens");
        let block = session.blocks()[0].id;
        let db = session
            .make_database(block, "汇总测试".into())
            .expect("database created");
        let relation = session
            .add_database_property(block, "相关记录".into(), PropertyKind::Relation)
            .expect("relation created");
        assert!(
            session
                .set_database_relation_target(block, relation, Some(db))
                .expect("target configured")
        );
        let amount = session
            .add_database_property(block, "金额".into(), PropertyKind::Number)
            .expect("amount created");
        let rollup = session
            .add_database_property(block, "金额合计".into(), PropertyKind::Rollup)
            .expect("rollup created");
        let source = session.add_database_record(block).expect("source created");
        let target = session.add_database_record(block).expect("target created");
        session
            .set_database_cell(block, target, amount, "12.5")
            .expect("amount written");
        session
            .set_database_relation_cell(block, source, relation, &[target])
            .expect("relation written");
        session
            .set_database_rollup(block, rollup, Some(relation), Some(amount), Aggregate::Sum)
            .expect("rollup configured");
        let properties = session.databases.properties.clone();
        let value =
            evaluate_database_rollup(rollup, &properties, &session.repo, RecordId(source.0))
                .expect("rollup evaluates");
        assert_eq!(quire_core::core::database_formula::display(&value), "12.5");
        session.undo();
        assert_eq!(
            quire_core::core::database_rollup::config_rollup(
                &session
                    .databases
                    .properties
                    .iter()
                    .find(|property| property.id == rollup)
                    .unwrap()
                    .config
            )
            .aggregate,
            Aggregate::None
        );
        session.redo();
        drop(session);
        let reopened = quire_gpui::session::Session::open(&path).expect("database reopens");
        assert_eq!(
            quire_core::core::database_rollup::config_rollup(
                &reopened
                    .databases
                    .properties
                    .iter()
                    .find(|property| property.id == rollup)
                    .unwrap()
                    .config
            )
            .aggregate,
            Aggregate::Sum
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
    fn link_target_uses_selection_or_link_at_caret_and_ignores_invalid_marks() {
        let marks = [
            Mark {
                start: 0,
                end: 4,
                kind: MarkKind::Bold,
                url: String::new(),
                date: None,
            },
            Mark {
                start: 5,
                end: 10,
                kind: MarkKind::Link,
                url: "https://example.test".into(),
                date: None,
            },
            Mark {
                start: 10,
                end: 14,
                kind: MarkKind::Link,
                url: "https://past-end.test".into(),
                date: None,
            },
        ];
        assert_eq!(link_target_range(&(1..3), 8, 14, &marks), Some((1, 3)));
        assert_eq!(link_target_range(&(0..0), 7, 14, &marks), Some((5, 10)));
        assert_eq!(link_target_range(&(0..0), 12, 12, &marks), None);
        assert_eq!(link_target_range(&(0..0), 4, 14, &marks), None);
    }

    #[test]
    fn database_filter_values_validate_numeric_and_date_inputs_without_rewriting_text() {
        assert_eq!(
            database_filter_value_from_text(PropertyKind::Number, " 12.5 "),
            Some(FilterValue::Number(12.5))
        );
        assert_eq!(
            database_filter_value_from_text(PropertyKind::Number, "NaN"),
            None
        );
        assert_eq!(
            database_filter_value_from_text(PropertyKind::Date, "2026-10-15"),
            Some(FilterValue::Text("2026-10-15".into()))
        );
        assert_eq!(
            database_filter_value_from_text(PropertyKind::Date, "15/10/2026"),
            None
        );
        assert_eq!(
            database_filter_value_from_text(PropertyKind::Text, " a  b "),
            Some(FilterValue::Text(" a  b ".into()))
        );
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

fn link_target_range(
    selection: &Range<usize>,
    caret: usize,
    text_len: usize,
    marks: &[Mark],
) -> Option<(usize, usize)> {
    if !selection.is_empty() {
        return Some((selection.start, selection.end));
    }
    marks
        .iter()
        .find(|mark| {
            mark.kind == MarkKind::Link
                && mark.start < mark.end
                && mark.end <= text_len
                && mark.start <= caret
                && caret <= mark.end
        })
        .map(|mark| (mark.start, mark.end))
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
    page_history_back: Vec<quire_core::core::types::PageId>,
    page_history_forward: Vec<quire_core::core::types::PageId>,
    title_input: Entity<TextInput>,
    title_subscription: Option<Subscription>,
    title_edit_original: Option<String>,
    sync_name_input: Entity<TextInput>,
    sync_name_subscription: Option<Subscription>,
    sync_name_draft: String,
    sync_name_original: String,
    search_input: Entity<TextInput>,
    find_input: Entity<TextInput>,
    link_target_input: Entity<TextInput>,
    note_title_input: Entity<TextInput>,
    note_body_input: Entity<TextInput>,
    task_title_input: Entity<TextInput>,
    task_list_name_input: Entity<TextInput>,
    task_due_input: Entity<TextInput>,
    organizer_tags_input: Entity<TextInput>,
    subtask_title_input: Entity<TextInput>,
    database_cell_input: Entity<TextInput>,
    database_formula_input: Entity<TextInput>,
    database_option_input: Entity<TextInput>,
    database_search_input: Entity<TextInput>,
    database_relation_search_input: Entity<TextInput>,
    database_property_name_input: Entity<TextInput>,
    search_subscription: Option<Subscription>,
    database_search_subscription: Option<Subscription>,
    database_relation_search_subscription: Option<Subscription>,
    database_property_name_subscription: Option<Subscription>,
    find_subscription: Option<Subscription>,
    note_title_subscription: Option<Subscription>,
    note_body_subscription: Option<Subscription>,
    task_title_subscription: Option<Subscription>,
    task_list_name_subscription: Option<Subscription>,
    task_due_subscription: Option<Subscription>,
    organizer_tags_subscription: Option<Subscription>,
    subtask_title_subscription: Option<Subscription>,
    database_cell_subscription: Option<Subscription>,
    database_formula_subscription: Option<Subscription>,
    database_option_subscription: Option<Subscription>,
    search_query: String,
    find_query: String,
    find_matches: Vec<(BlockId, Range<usize>)>,
    find_index: Option<usize>,
    find_open: bool,
    search_open: bool,
    search_selected: usize,
    command_palette_open: bool,
    command_palette_query: String,
    command_palette_selected: usize,
    backlinks_open: bool,
    hovered_block: Option<BlockId>,
    block_menu_open: Option<(BlockId, BlockMenuMode)>,
    hovered_page: Option<quire_core::core::types::PageId>,
    hovered_note: Option<quire_core::core::organizer::NoteId>,
    hovered_task: Option<quire_core::core::organizer::TaskId>,
    inputs: BTreeMap<u64, Entity<TextInput>>,
    input_subscriptions: HashMap<u64, Subscription>,
    document_list: ListState,
    document_list_ids: Vec<BlockId>,
    document_visible_range: Range<usize>,
    selected_database_views: HashMap<u64, ViewId>,
    database_columns_open: Option<(u64, u64)>,
    database_filter_open: Option<(u64, u64)>,
    database_group_open: Option<(u64, u64)>,
    database_collapsed_groups: HashSet<(u64, u64, String)>,
    database_filter_panel: usize,
    database_filter_edit_row: Option<usize>,
    database_filter_value_input: Entity<TextInput>,
    database_filter_value_active: Option<(u64, u64, u64, usize)>,
    database_property_kind_open: Option<(u64, u64)>,
    database_column_drag: Option<DatabaseColumnDrag>,
    database_search_open: Option<u64>,
    database_search_text: String,
    database_relation_search_text: String,
    selected_database_record: Option<(u64, u64, u64)>,
    active_database_cell: Option<(u64, u64, u64, u64)>,
    active_database_formula: Option<(u64, u64, u64)>,
    active_database_option: Option<(u64, u64, u64)>,
    active_database_choice: Option<(u64, u64, u64, u64)>,
    active_database_relation: Option<(u64, u64, u64, u64)>,
    active_database_property_edit: Option<(u64, u64, u64)>,
    active_database_view_edit: Option<(u64, u64, u64)>,
    database_property_name_draft: String,
    database_header_click_seq: u64,
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
    organizer_excluded_tags: BTreeSet<String>,
    selected_task_list: quire_core::core::organizer::ListId,
    task_list_editing: Option<quire_core::core::organizer::ListId>,
    task_list_menu_open: Option<quire_core::core::organizer::ListId>,
    task_list_color_open: Option<quire_core::core::organizer::ListId>,
    task_list_name_draft: String,
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
    organizer_debounce_seq: u64,
    task_title_draft: String,
    task_due_draft: String,
    organizer_tags_draft: String,
    subtask_title_draft: String,
    task_due_error: bool,
    status: String,
    error: bool,
    startup_notice: Option<String>,
    active: Option<BlockId>,
    save_task: Option<gpui::Task<()>>,
    flush_lock: Arc<Mutex<()>>,
    closing: bool,
    normal_window_bounds: Bounds<Pixels>,
    mcp_event_tx: std::sync::mpsc::Sender<crate::platform::ShellEvent>,
    mcp_server: Option<crate::mcp::McpServerHandle>,
    mcp_port: u16,
    mcp_port_input: Entity<TextInput>,
    mcp_port_subscription: Option<Subscription>,
    mcp_port_draft: String,
    mcp_token: String,
    mcp_status: String,
    tray_available: bool,
    quick_note_window: Option<WindowHandle<QuickNoteWindow>>,
    sync_cmd_tx: std::sync::mpsc::Sender<quire_core::services::sync::engine::Cmd>,
    sync_status: String,
    sync_listener_warning: Option<String>,
    sync_last_auto: std::time::Instant,
    sync_pending: HashSet<String>,
    restart_on_quit: Arc<std::sync::atomic::AtomicBool>,
    set_hotkey: Arc<dyn Fn(u8, bool) -> Result<(), String> + Send + Sync>,
    hotkey_notice: String,
}

impl Workspace {
    pub fn new(
        mut session: Session,
        theme: &str,
        smoke: bool,
        tray_available: bool,
        sync_listener_warning: Option<String>,
        shell_events: std::sync::mpsc::Receiver<crate::platform::ShellEvent>,
        platform_services: crate::platform::Services,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let startup_notice = session.startup_notice.take();
        if smoke && startup_notice.is_some() {
            eprintln!("SMOKE: Workspace mounted the database recovery warning banner");
        }
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
        let task_list_name_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 12., window, cx));
        task_list_name_input.update(cx, |input, _| input.placeholder = Some("清单名称".into()));
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
        let database_formula_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 12., window, cx));
        database_formula_input.update(cx, |input, _| {
            input.placeholder = Some("例如：[数量] * [单价]".into())
        });
        let database_option_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 11., window, cx));
        database_option_input.update(cx, |input, _| {
            input.placeholder = Some("新选项名，按 Enter 添加".into())
        });
        let database_search_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 12., window, cx));
        let database_relation_search_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 11., window, cx));
        let database_filter_value_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 12., window, cx));
        database_filter_value_input.update(cx, |input, _| {
            input.placeholder = Some("输入筛选值后按 Enter".into());
        });
        let database_property_name_input =
            cx.new(|cx| TextInput::new(String::new(), Vec::new(), 11., window, cx));
        let sync_self = session.sync_self_info();
        let sync_name_input =
            cx.new(|cx| TextInput::new(sync_self.name.clone(), Vec::new(), 11., window, cx));
        sync_name_input.update(cx, |input, _| {
            input.placeholder = Some("此设备在局域网中的名称".into());
        });
        let sync_name_draft = sync_self.name.clone();
        let sync_name_original = sync_name_draft.clone();
        database_search_input.update(cx, |input, _| {
            input.placeholder = Some("搜索此数据库…".into())
        });
        database_relation_search_input.update(cx, |input, _| {
            input.placeholder = Some("搜索目标记录…".into())
        });
        let input_fg = Palette::color(&Palette::load(theme).fg);
        for input in [
            title_input.clone(),
            search_input.clone(),
            find_input.clone(),
            link_target_input.clone(),
            sync_name_input.clone(),
            note_title_input.clone(),
            note_body_input.clone(),
            task_title_input.clone(),
            task_list_name_input.clone(),
            task_due_input.clone(),
            organizer_tags_input.clone(),
            subtask_title_input.clone(),
            database_cell_input.clone(),
            database_formula_input.clone(),
            database_option_input.clone(),
            database_search_input.clone(),
            database_relation_search_input.clone(),
            database_property_name_input.clone(),
        ] {
            input.update(cx, |input, _| input.text_color = Some(input_fg));
        }
        let mcp_event_tx = platform_services.event_tx.clone();
        let set_hotkey = platform_services.set_hotkey.clone();
        let hotkey_alt_n = session.setting("gpui.hotkey.alt-n").unwrap_or("true") == "true";
        let hotkey_alt_m = session.setting("gpui.hotkey.alt-m").unwrap_or("false") == "true";
        let mut hotkey_startup_warnings = Vec::new();
        for (id, enabled) in [
            (crate::platform::HOTKEY_ALT_N, hotkey_alt_n),
            (crate::platform::HOTKEY_ALT_M, hotkey_alt_m),
        ] {
            if let Err(error) = set_hotkey(id, enabled) {
                eprintln!("Quire GPUI: persisted global hotkey {id} could not be applied: {error}");
                let label = if id == crate::platform::HOTKEY_ALT_N {
                    "Alt+N"
                } else {
                    "Alt+M"
                };
                hotkey_startup_warnings.push(format!("{label} 未能注册：{error}"));
            }
        }
        let hotkey_startup_notice = if hotkey_startup_warnings.is_empty() {
            "Alt+N 默认开启；Alt+M 默认关闭".to_owned()
        } else {
            hotkey_startup_warnings.join("；")
        };
        let mcp_port = session
            .setting("gpui.mcp.port")
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|port| *port > 0)
            .unwrap_or(crate::mcp::DEFAULT_PORT);
        let mcp_port_draft = mcp_port.to_string();
        let mcp_port_input =
            cx.new(|cx| TextInput::new(mcp_port_draft.clone(), Vec::new(), 12., window, cx));
        mcp_port_input.update(cx, |input, _| {
            input.placeholder = Some("5880".into());
            input.text_color = Some(input_fg);
        });
        let mcp_token = match session.setting("gpui.mcp.token") {
            Some(token) => token.to_owned(),
            None => {
                let token = crate::mcp::mint_token();
                session.set_setting("gpui.mcp.token", token.clone());
                token
            }
        };
        let mcp_enabled = session.setting("gpui.mcp.enabled") == Some("true");
        let (sync_job_tx, sync_job_rx) = std::sync::mpsc::channel();
        let sync_cmd_tx = quire_core::services::sync::engine::Engine::start(sync_self, sync_job_tx);
        let mut this = Self {
            session,
            page_history_back: Vec::new(),
            page_history_forward: Vec::new(),
            title_input,
            title_subscription: None,
            title_edit_original: None,
            sync_name_input,
            sync_name_subscription: None,
            sync_name_draft,
            sync_name_original,
            search_input,
            find_input,
            link_target_input,
            note_title_input,
            note_body_input,
            task_title_input,
            task_list_name_input,
            task_due_input,
            organizer_tags_input,
            subtask_title_input,
            database_cell_input,
            database_formula_input,
            database_option_input,
            database_search_input,
            database_relation_search_input,
            database_property_name_input,
            search_subscription: None,
            database_search_subscription: None,
            database_relation_search_subscription: None,
            database_property_name_subscription: None,
            find_subscription: None,
            note_title_subscription: None,
            note_body_subscription: None,
            task_title_subscription: None,
            task_list_name_subscription: None,
            task_due_subscription: None,
            organizer_tags_subscription: None,
            subtask_title_subscription: None,
            database_cell_subscription: None,
            database_formula_subscription: None,
            database_option_subscription: None,
            search_query: String::new(),
            find_query: String::new(),
            find_matches: Vec::new(),
            find_index: None,
            find_open: false,
            search_open: false,
            search_selected: 0,
            command_palette_open: false,
            command_palette_query: String::new(),
            command_palette_selected: 0,
            backlinks_open: false,
            hovered_block: None,
            block_menu_open: None,
            hovered_page: None,
            hovered_note: None,
            hovered_task: None,
            inputs: BTreeMap::new(),
            input_subscriptions: HashMap::new(),
            document_list: ListState::new(2, ListAlignment::Top, px(240.)),
            document_list_ids: Vec::new(),
            document_visible_range: 0..2,
            selected_database_views: HashMap::new(),
            database_columns_open: None,
            database_filter_open: None,
            database_group_open: None,
            database_collapsed_groups: HashSet::new(),
            database_property_kind_open: None,
            database_column_drag: None,
            database_search_open: None,
            database_search_text: String::new(),
            database_relation_search_text: String::new(),
            database_filter_panel: 0,
            database_filter_edit_row: None,
            database_filter_value_input,
            database_filter_value_active: None,
            selected_database_record: None,
            active_database_cell: None,
            active_database_formula: None,
            active_database_option: None,
            active_database_choice: None,
            active_database_relation: None,
            active_database_property_edit: None,
            active_database_view_edit: None,
            database_property_name_draft: String::new(),
            database_header_click_seq: 0,
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
            organizer_excluded_tags: BTreeSet::new(),
            selected_task_list: quire_core::core::organizer::ListId::INBOX,
            task_list_editing: None,
            task_list_menu_open: None,
            task_list_color_open: None,
            task_list_name_draft: String::new(),
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
            organizer_debounce_seq: 0,
            task_title_draft: String::new(),
            task_due_draft: String::new(),
            organizer_tags_draft: String::new(),
            subtask_title_draft: String::new(),
            task_due_error: false,
            status: "已保存".into(),
            error: false,
            startup_notice,
            active: None,
            save_task: None,
            flush_lock: Arc::new(Mutex::new(())),
            closing: false,
            normal_window_bounds: window.bounds(),
            mcp_event_tx,
            mcp_server: None,
            mcp_port,
            mcp_port_input,
            mcp_port_subscription: None,
            mcp_port_draft,
            mcp_token,
            mcp_status: if mcp_enabled {
                "MCP 已启用"
            } else {
                "尚未启用"
            }
            .into(),
            tray_available,
            quick_note_window: None,
            sync_cmd_tx,
            sync_status: sync_listener_warning
                .as_deref()
                .unwrap_or("正在搜索局域网设备")
                .to_owned(),
            sync_listener_warning,
            sync_last_auto: std::time::Instant::now(),
            sync_pending: HashSet::new(),
            restart_on_quit: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            set_hotkey,
            hotkey_notice: hotkey_startup_notice,
        };
        if mcp_enabled {
            this.start_mcp();
        }
        let bounds_subscription = cx.observe_window_bounds(window, |this, window, _| {
            if !window.is_maximized() && !window.is_fullscreen() {
                this.normal_window_bounds = window.bounds();
            }
        });
        this.subscriptions.push(bounds_subscription);
        let link_target_input = this.link_target_input.clone();
        this.subscriptions.push(cx.subscribe_in(
            &link_target_input,
            window,
            |this, _, event, window, cx| {
                if matches!(event, InputEvent::DismissSlash) && this.link_editor_open {
                    this.link_editor_open = false;
                    this.restore_focus(window, cx);
                    cx.notify();
                }
            },
        ));
        let database_filter_value_input = this.database_filter_value_input.clone();
        this.subscriptions.push(cx.subscribe_in(
            &database_filter_value_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { .. } => cx.notify(),
                InputEvent::Split(_) => this.commit_database_filter_value(window, cx),
                InputEvent::DismissSlash => {
                    this.database_filter_value_active = None;
                    this.restore_focus(window, cx);
                    cx.notify();
                }
                _ => {}
            },
        ));
        let sync_name_input = this.sync_name_input.clone();
        this.sync_name_subscription = Some(cx.subscribe_in(
            &sync_name_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.sync_name_draft = text.clone();
                    cx.notify();
                }
                InputEvent::Split(_) => this.apply_sync_name(window, cx),
                _ => {}
            },
        ));
        let mcp_port_input = this.mcp_port_input.clone();
        this.mcp_port_subscription = Some(cx.subscribe_in(
            &mcp_port_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.mcp_port_draft = text.clone();
                    cx.notify();
                }
                InputEvent::Split(_) => this.apply_mcp_port(window, cx),
                _ => {}
            },
        ));
        let note_title_input = this.note_title_input.clone();
        this.note_title_subscription = Some(cx.subscribe_in(
            &note_title_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.note_title_draft = text.clone();
                    this.arm_organizer_draft_debounce(window, cx);
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
                    this.arm_organizer_draft_debounce(window, cx);
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
        let task_list_name_input = this.task_list_name_input.clone();
        this.task_list_name_subscription = Some(cx.subscribe_in(
            &task_list_name_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.task_list_name_draft = text.clone();
                    cx.notify();
                }
                InputEvent::Split(_) | InputEvent::Blurred => {
                    this.commit_task_list_rename(window, cx)
                }
                InputEvent::DismissSlash => this.cancel_task_list_rename(cx),
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
                    this.arm_organizer_draft_debounce(window, cx);
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
                    this.arm_organizer_draft_debounce(window, cx);
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
        let database_formula_input = this.database_formula_input.clone();
        this.database_formula_subscription = Some(cx.subscribe_in(
            &database_formula_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { .. } => cx.notify(),
                InputEvent::Split(_) => this.commit_database_formula(true, window, cx),
                InputEvent::DismissSlash => {
                    this.active_database_formula = None;
                    this.focus_first(window, cx);
                    cx.notify();
                }
                _ => {}
            },
        ));
        let database_option_input = this.database_option_input.clone();
        this.database_option_subscription = Some(cx.subscribe_in(
            &database_option_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { .. } => cx.notify(),
                InputEvent::Split(_) => this.commit_database_option(window, cx),
                InputEvent::DismissSlash => {
                    this.active_database_option = None;
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
                    if this.command_palette_open {
                        this.command_palette_query = text.clone();
                        this.command_palette_selected = 0;
                    } else {
                        this.search_query = text.clone();
                        this.search_selected = 0;
                    }
                    cx.notify();
                }
                InputEvent::Navigate(direction) => {
                    if this.command_palette_open {
                        let count = this.palette_items().len().min(8);
                        if count > 0 {
                            this.command_palette_selected = (this.command_palette_selected as i32
                                + direction)
                                .clamp(0, count as i32 - 1)
                                as usize;
                            cx.notify();
                        }
                        return;
                    }
                    let count = this.search_result_ids().len();
                    if count > 0 {
                        this.search_selected = (this.search_selected as i32 + direction)
                            .rem_euclid(count as i32)
                            as usize;
                        cx.notify();
                    }
                }
                InputEvent::Split(_) => {
                    if this.command_palette_open {
                        this.invoke_palette_command(window, cx);
                    } else {
                        this.open_selected_search_result(window, cx);
                    }
                }
                InputEvent::DismissSlash => {
                    if this.command_palette_open {
                        this.close_command_palette(window, cx);
                    } else if this.theme_picker_open {
                        this.close_settings(window, cx);
                    } else {
                        this.close_search(window, cx);
                    }
                }
                _ => {}
            },
        ));
        let database_search_input = this.database_search_input.clone();
        this.database_search_subscription = Some(cx.subscribe_in(
            &database_search_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.database_search_text = text.clone();
                    cx.notify();
                }
                InputEvent::DismissSlash => this.close_database_search(window, cx),
                _ => {}
            },
        ));
        let database_relation_search_input = this.database_relation_search_input.clone();
        this.database_relation_search_subscription = Some(cx.subscribe_in(
            &database_relation_search_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.database_relation_search_text = text.clone();
                    cx.notify();
                }
                InputEvent::DismissSlash => {
                    if let Some(key) = this.active_database_relation {
                        this.toggle_database_relation_picker(key, window, cx);
                    }
                }
                _ => {}
            },
        ));
        let database_property_name_input = this.database_property_name_input.clone();
        this.database_property_name_subscription = Some(cx.subscribe_in(
            &database_property_name_input,
            window,
            |this, _, event, window, cx| match event {
                InputEvent::Edited { text, .. } => {
                    this.database_property_name_draft = text.clone();
                    cx.notify();
                }
                InputEvent::Split(_) => {
                    if this.active_database_view_edit.is_some() {
                        this.commit_database_view_name(true, window, cx)
                    } else {
                        this.commit_database_property_name(true, window, cx)
                    }
                }
                InputEvent::Blurred => {
                    if this.active_database_view_edit.is_some() {
                        this.commit_database_view_name(false, window, cx)
                    } else {
                        this.commit_database_property_name(false, window, cx)
                    }
                }
                InputEvent::DismissSlash => this.cancel_database_property_name(window, cx),
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
        if smoke && this.startup_notice.is_some() {
            let weak = cx.entity().downgrade();
            cx.spawn_in(window, async move |_, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(700))
                    .await;
                let _ = weak.update_in(cx, |this, window, cx| {
                    if let Some(quick_note) = this.quick_note_window.take() {
                        let _ = quick_note.update(cx, |_, popup, _| popup.remove_window());
                    }
                    eprintln!("SMOKE: recovery warning remained mounted in the GPUI workspace");
                    window.remove_window();
                });
            })
            .detach();
        } else if smoke {
            let mcp_port = this.mcp_port;
            let mcp_token = this.mcp_token.clone();
            let (mcp_result_tx, mcp_result_rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = mcp_result_tx.send(smoke_mcp_create_note(mcp_port, &mcp_token));
            });
            cx.spawn_in(window, async move |weak, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                let mut mcp_response = None;
                for _ in 0..125 {
                    if let Ok(response) = mcp_result_rx.try_recv() {
                        mcp_response = Some(response);
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(40))
                        .await;
                }
                let mcp_response = mcp_response
                    .expect("MCP smoke request should finish within five seconds")
                    .expect("MCP smoke request should receive an HTTP response");
                assert!(
                    mcp_response.starts_with("HTTP/1.1 200"),
                    "MCP smoke should receive success: {mcp_response}"
                );
                let _ = weak.update_in(cx, |this, _, _| {
                    let note = this
                        .session
                        .organizer
                        .live_notes()
                        .find(|note| note.body == "由 MCP 桌面窗口回归创建")
                        .expect("MCP request should reach the live Session");
                    assert_eq!(note.tags, ["mcp-smoke", "桌面回归"]);
                    eprintln!("SMOKE: persisted MCP startup, HTTP auth, UI dispatch and note write passed");
                });
                let _ = weak.update_in(cx, |this, window, cx| {
                    let smoke_list = this.session.create_task_list("smoke 清单".into());
                    this.toggle_task_list_menu(smoke_list, cx);
                    assert_eq!(this.task_list_menu_open, Some(smoke_list));
                    this.toggle_task_list_color_picker(smoke_list, cx);
                    assert_eq!(this.task_list_color_open, Some(smoke_list));
                    this.task_list_color_open = None;
                    this.start_task_list_rename(smoke_list, window, cx);
                    this.task_list_name_draft = "桌面回归清单".into();
                    this.task_list_name_input.update(cx, |input, cx| {
                        let name = "桌面回归清单";
                        input.reset(name.into(), Vec::new(), name.len(), cx)
                    });
                    this.commit_task_list_rename(window, cx);
                    assert_eq!(
                        this.session.organizer.list(smoke_list).unwrap().name,
                        "桌面回归清单"
                    );
                    this.set_task_list_color(smoke_list, ColorKind::Purple, window, cx);
                    assert_eq!(
                        this.session.organizer.list(smoke_list).unwrap().color,
                        ColorKind::Purple
                    );
                    assert!(this.session.undo_organizer());
                    assert_ne!(
                        this.session.organizer.list(smoke_list).unwrap().color,
                        ColorKind::Purple
                    );
                    assert!(this.session.redo_organizer());
                    assert!(this.session.undo_organizer());
                    assert!(this.session.undo_organizer());
                    assert_ne!(
                        this.session.organizer.list(smoke_list).unwrap().name,
                        "桌面回归清单"
                    );
                    assert!(this.session.redo_organizer());
                    assert!(this.session.redo_organizer());
                    assert!(this.session.delete_task_list(smoke_list).is_some());
                    this.selected_task_list = quire_core::core::organizer::ListId::INBOX;
                    eprintln!("SMOKE: organizer list menu, color submenu, rename and undo/redo passed");

                    this.organizer_trash = true;
                    this.organizer_tasks = false;
                    let first = this.session.create_note();
                    assert!(this.session.set_note_trashed(first, true));
                    this.purge_note(first, window, cx);
                    assert!(this.session.organizer.note(first).is_none());
                    assert!(this.session.undo_organizer());
                    assert!(this.session.organizer.note(first).unwrap().is_trashed());
                    assert!(this.session.redo_organizer());
                    assert!(this.session.organizer.note(first).is_none());
                    assert!(this.session.undo_organizer());
                    for _ in 0..2 {
                        let id = this.session.create_note();
                        assert!(this.session.set_note_trashed(id, true));
                    }
                    this.empty_organizer_bin(window, cx);
                    assert!(this.session.organizer.trashed_notes().next().is_none());
                    assert!(this.session.undo_organizer());
                    assert_eq!(this.session.organizer.trashed_notes().count(), 3);
                    assert!(this.session.redo_organizer());
                    assert!(this.session.organizer.trashed_notes().next().is_none());

                    this.organizer_tasks = true;
                    for title in ["回收站任务 A", "回收站任务 B"] {
                        let id = this.session.create_task(title.into());
                        assert!(this.session.set_task_trashed(id, true));
                    }
                    this.empty_organizer_bin(window, cx);
                    assert!(this.session.organizer.trashed_tasks().next().is_none());
                    assert!(this.session.undo_organizer());
                    assert_eq!(this.session.organizer.trashed_tasks().count(), 2);
                    assert!(this.session.redo_organizer());
                    assert!(this.session.organizer.trashed_tasks().next().is_none());
                    this.organizer_trash = false;
                    this.organizer_tasks = false;
                    this.schedule_save(true, window, cx);
                    eprintln!("SMOKE: organizer single purge, batch empty and undo/redo passed");
                });
                let _ = weak.update_in(cx, |this, window, cx| {
                    this.open_command_palette(window, cx);
                    this.search_input.update(cx, |input, cx| {
                        input.replace_text_in_range(None, "设置", window, cx);
                    });
                });
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                let _ = weak.update_in(cx, |this, window, cx| {
                    assert!(this.command_palette_open);
                    assert_eq!(this.command_palette_query, "设置");
                    assert_eq!(this.palette_items().len(), 1);
                    this.invoke_palette_command(window, cx);
                    assert!(this.theme_picker_open);
                    this.close_settings(window, cx);
                    eprintln!("SMOKE: Ctrl+K command palette filtering and settings action passed");
                    this.open_command_palette(window, cx);
                    this.search_input.update(cx, |input, cx| {
                        input.replace_text_in_range(
                            None,
                            "插入模板：Meeting notes",
                            window,
                            cx,
                        );
                    });
                });
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                let _ = weak.update_in(cx, |this, window, cx| {
                    assert_eq!(this.command_palette_query, "插入模板：Meeting notes");
                    assert_eq!(this.palette_items().len(), 1);
                    let original_block_count = this.session.blocks().len();
                    this.invoke_palette_command(window, cx);
                    assert!(this.session.blocks().len() > original_block_count);
                    this.sync_inputs(window, cx);
                    this.session.undo();
                    this.sync_inputs(window, cx);
                    assert_eq!(this.session.blocks().len(), original_block_count);
                    this.session.redo();
                    this.sync_inputs(window, cx);
                    assert!(this.session.blocks().len() > original_block_count);
                    this.session.undo();
                    this.sync_inputs(window, cx);
                    assert_eq!(this.session.blocks().len(), original_block_count);
                    eprintln!("SMOKE: template palette insertion and one-step undo/redo passed");
                    let original_page = this.session.page.id;
                    this.add_page(window, cx);
                    let created_page = this.session.page.id;
                    assert_ne!(created_page, original_page);
                    this.navigate_page_history(false, window, cx);
                    assert_eq!(this.session.page.id, original_page);
                    this.navigate_page_history(true, window, cx);
                    assert_eq!(this.session.page.id, created_page);
                    this.navigate_page_history(false, window, cx);
                    assert_eq!(this.session.page.id, original_page);
                    this.add_page(window, cx);
                    let branch_page = this.session.page.id;
                    this.navigate_page_history(true, window, cx);
                    assert_eq!(this.session.page.id, branch_page);
                    this.navigate_page_history(false, window, cx);
                    assert_eq!(this.session.page.id, original_page);
                    eprintln!("SMOKE: page history back/forward and branch reset passed");
                    crate::platform::set_main_window_maximized(window, true);
                });
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                let _ = weak.update_in(cx, |_, window, _| {
                    assert!(window.is_maximized(), "native maximize control must maximize the main window");
                    crate::platform::set_main_window_maximized(window, false);
                });
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                let _ = weak.update_in(cx, |_, window, _| {
                    assert!(!window.is_maximized(), "native restore control must return to windowed mode");
                    eprintln!("SMOKE: maximize and windowed restore passed");
                });
                cx.background_executor()
                    .timer(Duration::from_millis(150))
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
                    if texts.get(0).copied() != Some("剥离心智模型 🦀")
                        || texts.get(1).copied() != Some("重启后仍然存在")
                    {
                        panic!("UI input/core smoke mismatch: {texts:?}");
                    }
                    let blocks = this.session.blocks();
                    let find_target = blocks[blocks.len() / 2].clone();
                    this.open_find(&Find, window, cx);
                    this.find_query = find_target.text.clone();
                    this.find_input.update(cx, |input, cx| {
                        let len = find_target.text.len();
                        input.reset(find_target.text.clone(), Vec::new(), len, cx);
                    });
                    this.refresh_find_matches();
                    assert_eq!(this.find_matches.len(), 1);
                    this.move_find(1, window, cx);
                    let find_target_id = find_target.id;
                    let find_target_range = 0..find_target.text.len();
                    cx.spawn_in(window, async move |weak, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(300))
                            .await;
                        let _ = weak.update_in(cx, |this, window, cx| {
                            assert_eq!(this.active, Some(find_target_id));
                            assert_eq!(
                                this.inputs
                                    .get(&find_target_id.0)
                                    .expect("find should mount an offscreen block editor")
                                    .read(cx)
                                    .buffer
                                    .selection,
                                find_target_range,
                                "find should select the UTF-8 range in an offscreen block"
                            );
                            this.close_find(window, cx);
                            eprintln!("SMOKE: Ctrl+F match selection and offscreen virtual-list navigation passed");
                        });
                        let _ = weak.update_in(cx, |this, window, cx| {
                    let note_text = "速记冒烟回归 #gpui-smoke";
                    if !this.save_quick_note(note_text.to_owned(), cx) {
                        panic!("quick note smoke save failed");
                    }
                    let saved_note = this
                        .session
                        .organizer
                        .live_notes()
                        .find(|note| note.body == note_text)
                        .expect("quick note must be in the core session");
                    if saved_note.tags != ["gpui-smoke"] {
                        panic!("quick note tag extraction mismatch: {:?}", saved_note.tags);
                    }
                    let last_block = this
                        .session
                        .blocks()
                        .last()
                        .expect("smoke page must have a block")
                        .id;
                    this.focus(last_block, 0, window, cx);
                    eprintln!(
                        "SMOKE scroll requested: offset={:?}, viewport={:?}",
                        this.document_list.logical_scroll_top(),
                        this.document_list.viewport_bounds()
                    );
                    cx.spawn_in(window, async move |weak, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(800))
                            .await;
                        let _ = weak.update_in(cx, |this, window, cx| {
                            assert!(
                                this.document_list.viewport_bounds().size.height > px(0.),
                                "document virtual list must have a nonzero viewport"
                            );
                            let mounted = this.inputs.contains_key(&last_block.0);
                            if !mounted {
                                let last_index = this
                                    .document_list_ids
                                    .iter()
                                    .position(|id| *id == last_block);
                                eprintln!(
                                    "SMOKE virtual scroll diagnostic: mounted={mounted}, last_index={last_index:?}, list_count={}, visible={:?}, input_count={}, active={:?}, offset={:?}, viewport={:?}",
                                    this.document_list.item_count(),
                                    this.document_visible_range,
                                    this.inputs.len(),
                                    this.active,
                                    this.document_list.logical_scroll_top(),
                                    this.document_list.viewport_bounds()
                                );
                                panic!("scrolling to the last block must mount its editor");
                            }
                            eprintln!(
                                "SMOKE: virtual scroll-to-last passed; dispatching the platform quick-note event"
                            );
                            assert!(
                                this.quick_note_window.is_some(),
                                "the persisted quick-note-on-start preference must open the popup"
                            );
                            let windows_before = cx.windows().len();
                            this.mcp_event_tx
                                .send(crate::platform::ShellEvent::QuickNote)
                                .expect("quick-note event pump should still be available");
                            let quick_note_text = "速记窗口冒烟回归 #gpui-window-smoke".to_owned();
                            cx.spawn_in(window, async move |weak, cx| {
                                cx.background_executor()
                                    .timer(Duration::from_millis(200))
                                    .await;
                                let note_text = quick_note_text.clone();
                                let _ = weak.update_in(cx, |this, _, cx| {
                                    let handle = this
                                        .quick_note_window
                                        .expect("quick-note window should open in smoke mode");
                                    assert_eq!(
                                        cx.windows().len(),
                                        windows_before,
                                        "repeated quick-note activation must reuse the startup popup"
                                    );
                                    handle
                                        .update(cx, |note, popup, cx| {
                                            note.input.update(cx, |input, cx| {
                                                input.replace_text_in_range(
                                                    None, &note_text, popup, cx,
                                                );
                                            });
                                        })
                                        .expect("quick-note window should remain available");
                                });
                                cx.background_executor()
                                    .timer(Duration::from_millis(200))
                                    .await;
                                let handle = weak
                                    .update_in(cx, |this, _, _| this.quick_note_window)
                                    .ok()
                                    .flatten()
                                    .expect("quick-note window should remain open");
                                let _ = handle
                                    .update(cx, |note, popup, cx| note.submit(popup, cx));
                                cx.background_executor()
                                    .timer(Duration::from_millis(200))
                                    .await;
                                let _ = weak.update_in(cx, |this, window, cx| {
                                    let saved_note = this
                                        .session
                                        .organizer
                                        .live_notes()
                                        .find(|note| note.body == quick_note_text)
                                        .expect("quick-note window must save through the session");
                                    assert_eq!(saved_note.tags, ["gpui-window-smoke"]);
                                    this.session.persistence.force_flush().unwrap();
                                    eprintln!(
                                        "SMOKE: quick-note window input, tag extraction, submit and save passed; testing tray hide/show"
                                    );
                                    this.hide_to_tray(window, cx);
                                    let show_event = this.mcp_event_tx.clone();
                                    cx.spawn_in(window, async move |weak, cx| {
                                        cx.background_executor()
                                            .timer(Duration::from_millis(200))
                                            .await;
                                        show_event
                                            .send(crate::platform::ShellEvent::ShowMain)
                                            .expect("main-window event pump should still be available");
                                        cx.background_executor()
                                            .timer(Duration::from_millis(200))
                                            .await;
                                        let _ = weak.update_in(cx, |this, window, cx| {
                                            assert!(
                                                crate::platform::main_window_visible(window),
                                                "tray ShowMain event must restore the hidden main window"
                                            );
                                            eprintln!("SMOKE: tray hide/show event path passed");
                                            this.request_close(window, cx);
                                        });
                                    })
                                    .detach();
                                });
                            })
                            .detach();
                        });
                    })
                    .detach();
                        });
                    })
                    .detach();
                });
            })
            .detach();
        }
        let platform_handles = Arc::new(Mutex::new(Some(platform_services.handles)));
        let shutdown_handles = platform_handles.clone();
        let restart_on_quit = this.restart_on_quit.clone();
        let restart_executable = std::env::current_exe().ok();
        let restart_arguments: Vec<_> = std::env::args_os().skip(1).collect();
        cx.on_app_quit(move |_, _| {
            let handles = shutdown_handles.clone();
            let restart_on_quit = restart_on_quit.clone();
            let restart_executable = restart_executable.clone();
            let restart_arguments = restart_arguments.clone();
            async move {
                drop(
                    handles
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .take(),
                );
                if restart_on_quit.load(std::sync::atomic::Ordering::Acquire) {
                    if let Some(executable) = restart_executable {
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(700));
                            if let Err(error) = std::process::Command::new(executable)
                                .args(restart_arguments)
                                .spawn()
                            {
                                eprintln!("Quire GPUI restart failed: {error}");
                            }
                        });
                    } else {
                        eprintln!("Quire GPUI restart failed: executable path unavailable");
                    }
                }
            }
        })
        .detach();
        let window_id = window.window_handle().window_id();
        cx.spawn(async move |_, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                loop {
                    let event = match shell_events.try_recv() {
                        Ok(event) => event,
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                    };
                    if matches!(&event, crate::platform::ShellEvent::Quit) {
                        let _ = cx.update(|app| app.quit());
                        return;
                    }
                    let _ = cx.update(|app| {
                        WindowHandle::<Workspace>::new(window_id).update(app, |this, window, cx| {
                            this.handle_shell_event(event, window, cx)
                        })
                    });
                }
                loop {
                    let job = match sync_job_rx.try_recv() {
                        Ok(job) => job,
                        Err(std::sync::mpsc::TryRecvError::Empty)
                        | Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                    };
                    let _ = cx.update(|app| {
                        WindowHandle::<Workspace>::new(window_id).update(app, |this, window, cx| {
                            this.handle_sync_job(job, window, cx)
                        })
                    });
                }
                let _ = cx.update(|app| {
                    WindowHandle::<Workspace>::new(window_id)
                        .update(app, |this, window, cx| this.sync_auto_tick(window, cx))
                });
            }
        })
        .detach();
        if this
            .session
            .setting("gpui.quick-note-on-start")
            .unwrap_or("false")
            == "true"
        {
            this.open_quick_note(cx);
        }
        this
    }
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let visible_blocks = self.visible_document_blocks();
        let ids = visible_blocks.iter().map(|(_, id)| *id).collect::<Vec<_>>();
        self.sync_document_list_ids(&ids);
        let mut keep = self.document_visible_block_ids();
        if let Some(active) = self.active {
            keep.insert(active.0);
        }
        self.inputs.retain(|id, input| {
            keep.contains(id)
                || input.read(cx).buffer.composing()
                    && self
                        .session
                        .blocks()
                        .iter()
                        .any(|block| block.id.0 == *id && self.session.editable(block.id))
        });
        self.input_subscriptions
            .retain(|id, _| self.inputs.contains_key(id));
        for (_, id) in visible_blocks {
            if let Some(block) = self.session.document.block(id).cloned()
                && self.inputs.contains_key(&id.0)
            {
                self.ensure_block_input(&block, window, cx);
            }
        }
        cx.notify();
    }
    fn visible_document_blocks(&self) -> Vec<(usize, BlockId)> {
        self.session
            .blocks()
            .iter()
            .enumerate()
            .filter_map(|(index, block)| {
                if matches!(block.kind, BlockKind::TableCell | BlockKind::Column) {
                    return None;
                }
                let mut parent = block.parent;
                while let Some(parent_id) = parent {
                    let Some(parent_block) = self.session.document.block(parent_id) else {
                        break;
                    };
                    if matches!(parent_block.kind, BlockKind::Column | BlockKind::TableCell)
                        || parent_block.folded
                    {
                        return None;
                    }
                    parent = parent_block.parent;
                }
                Some((index, block.id))
            })
            .collect()
    }
    fn sync_document_list_ids(&mut self, ids: &[BlockId]) {
        let expected_count = self.document_list_ids.len() + 2;
        if self.document_list.item_count() != expected_count {
            self.document_list.reset(expected_count);
        }
        if self.document_list_ids == ids {
            return;
        }
        let prefix = self
            .document_list_ids
            .iter()
            .zip(ids)
            .take_while(|(old, new)| old == new)
            .count();
        let suffix = self.document_list_ids[prefix..]
            .iter()
            .rev()
            .zip(ids[prefix..].iter().rev())
            .take_while(|(old, new)| old == new)
            .count();
        let old_end = self.document_list_ids.len() - suffix + 1;
        let new_count = ids.len() - prefix - suffix;
        self.document_list.splice(prefix + 1..old_end, new_count);
        self.document_list_ids = ids.to_vec();
        self.inputs.retain(|id, _| {
            ids.iter().any(|block| block.0 == *id)
                || self.active.is_some_and(|active| active.0 == *id)
        });
        self.input_subscriptions
            .retain(|id, _| self.inputs.contains_key(id));
    }
    fn document_visible_block_ids(&self) -> HashSet<u64> {
        let start = self.document_visible_range.start.saturating_sub(1);
        let end = self
            .document_visible_range
            .end
            .saturating_sub(1)
            .min(self.document_list_ids.len());
        self.document_list_ids
            .get(start..end.max(start))
            .unwrap_or_default()
            .iter()
            .map(|id| id.0)
            .collect()
    }
    fn ensure_block_input(
        &mut self,
        block: &Block,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<TextInput>> {
        if !self.session.editable(block.id) {
            return None;
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
            let id = block.id;
            let subscription =
                cx.subscribe_in(&input, window, move |this, _, event, window, cx| {
                    this.input_event(id, event, window, cx)
                });
            self.input_subscriptions.insert(block.id.0, subscription);
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
                input.reset(
                    block.text.clone(),
                    block.marks.clone(),
                    input.buffer.caret(),
                    cx,
                );
            } else if input.marks != block.marks {
                input.marks = block.marks.clone();
                cx.notify();
            }
        });
        Some(input)
    }
    fn invalidate_document_block(&self, id: BlockId) {
        if let Some(index) = self
            .document_list_ids
            .iter()
            .position(|candidate| *candidate == id)
        {
            self.document_list.splice(index + 1..index + 2, 1);
        }
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
                    self.invalidate_document_block(id);
                    self.sync_inputs(window, cx);
                    self.restore_focus(window, cx);
                } else {
                    self.session.replace(id, text.clone(), *boundary);
                    self.invalidate_document_block(id);
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
                self.invalidate_document_block(id);
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
                self.invalidate_document_block(id);
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
                self.set_active_block(id, cx);
                if self
                    .database_filter_open
                    .is_some_and(|(filter_block, _)| filter_block != id.0)
                {
                    self.database_filter_open = None;
                    self.database_filter_value_active = None;
                }
                if self
                    .database_group_open
                    .is_some_and(|(group_block, _)| group_block != id.0)
                {
                    self.database_group_open = None;
                }
                if self
                    .block_menu_open
                    .is_some_and(|(menu_block, _)| menu_block != id)
                {
                    self.block_menu_open = None;
                }
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
                let block_menu_closed = self.block_menu_open.take().is_some();
                let filter_menu_closed = self.database_filter_open.take().is_some();
                let group_menu_closed = self.database_group_open.take().is_some();
                self.database_filter_value_active = None;
                if self.find_open {
                    self.close_find(window, cx);
                }
                let slash_closed = self.slash_menu.take().is_some();
                let mention_closed = self.mention_menu.take().is_some();
                if block_menu_closed
                    || filter_menu_closed
                    || group_menu_closed
                    || slash_closed
                    || mention_closed
                {
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
    fn pick_database_file(
        &mut self,
        block: u64,
        record: u64,
        property: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked || self.closing {
            return;
        }
        let Some(paths) = rfd::FileDialog::new()
            .set_title("添加数据库文件")
            .pick_files()
        else {
            return;
        };
        let count = paths.len();
        match self.session.import_database_files(
            BlockId(block),
            quire_core::core::database::RecordId(record),
            quire_core::core::database::PropertyId(property),
            &paths,
        ) {
            Ok(true) => {
                self.status = format!("已添加 {count} 个文件，正在保存…");
                self.schedule_save(true, window, cx);
            }
            Ok(false) => {}
            Err(error) => {
                self.status = format!("添加文件失败：{error}");
                self.error = true;
                cx.notify();
            }
        }
    }
    fn remove_database_file(
        &mut self,
        block: u64,
        record: u64,
        property: u64,
        attachment: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids = match self.session.repo.cell(
            quire_core::core::database::RecordId(record),
            quire_core::core::database::PropertyId(property),
        ) {
            Ok(quire_core::core::database::CellValue::Items(ids)) => ids,
            _ => return,
        };
        let remaining = ids
            .into_iter()
            .filter(|id| id != &attachment.to_string())
            .collect::<Vec<_>>();
        match self.session.set_database_cell_many(
            BlockId(block),
            quire_core::core::database::RecordId(record),
            quire_core::core::database::PropertyId(property),
            &remaining,
        ) {
            Ok(_) => {
                self.schedule_save(true, window, cx);
                cx.notify();
            }
            Err(error) => {
                self.status = format!("移除附件失败：{error}");
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
    fn open_html_in_browser(&mut self, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title("在浏览器中打开 HTML")
            .add_filter("HTML", &["html", "htm"])
            .pick_file()
        else {
            return;
        };
        match crate::platform::open_with_default(&path) {
            Ok(()) => {
                self.status = format!("已在默认浏览器中打开：{}", path.display());
                self.error = false;
            }
            Err(error) => {
                self.status = format!("打开 HTML 失败：{error}");
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
    fn move_block_to_page(
        &mut self,
        id: BlockId,
        target: quire_core::core::types::PageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.closing || self.session.page.locked {
            return;
        }
        if self.composing(cx) {
            self.status = "请先完成输入法组合输入，再移动此块".into();
            self.error = true;
            cx.notify();
            return;
        }
        if self.session.move_block_to_page(id, target) {
            self.block_menu_open = None;
            self.sync_inputs(window, cx);
            self.focus_first(window, cx);
            let title = self
                .session
                .pages
                .iter()
                .find(|page| page.id == target)
                .map(|page| page.title.as_str())
                .unwrap_or("目标页面");
            self.status = format!("块已移动到「{title}」，可撤销");
            self.error = false;
            self.schedule_save(false, window, cx);
        }
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
        if self.link_editor_open {
            self.link_editor_open = false;
            self.restore_focus(window, cx);
            cx.notify();
            return;
        }
        let selection = self.active.and_then(|id| {
            let input = self.inputs.get(&id.0)?.read(cx);
            (!input.buffer.selection.is_empty()).then(|| {
                let range = input.buffer.selection.clone();
                let existing = input.marks.iter().find(|mark| {
                    mark.kind == quire_core::core::types::MarkKind::Link
                        && mark.start <= range.start
                        && mark.end >= range.end
                });
                existing.map(|mark| mark.url.clone()).unwrap_or_default()
            })
        });
        if selection.is_none() {
            self.status = "先选中要添加或修改链接的文字".into();
            cx.notify();
            return;
        }
        let url = selection.expect("checked above");
        self.link_target_input.update(cx, |input, input_cx| {
            let len = url.len();
            input.reset(url, Vec::new(), len, input_cx);
            window.focus(&input.focus);
        });
        self.link_editor_open = true;
        cx.notify();
    }
    fn apply_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composing(cx) || self.closing {
            return;
        }
        let Some(id) = self.active else {
            return;
        };
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
        let Some(input) = self.inputs.get(&id.0) else {
            return;
        };
        let selection = input.read(cx).buffer.selection.clone();
        let target = {
            let input = input.read(cx);
            link_target_range(
                &selection,
                input.buffer.caret(),
                input.buffer.text.len(),
                &input.marks,
            )
        };
        let Some((start, end)) = target else {
            self.status = "先选中要添加链接的文字，或将光标放在链接文字上".into();
            cx.notify();
            return;
        };
        self.session.set_link(id, start, end, url);
        self.link_editor_open = false;
        self.sync_inputs(window, cx);
        self.focus(id, end, window, cx);
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
            self.database_formula_input.clone(),
            self.database_option_input.clone(),
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
            self.database_formula_input.clone(),
            self.database_option_input.clone(),
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
        let definition = self
            .session
            .databases
            .properties_of(quire_core::core::database::DatabaseId(db))
            .find(|definition| definition.id.0 == property)
            .cloned();
        if definition.as_ref().is_some_and(|definition| {
            !matches!(
                definition.kind,
                quire_core::core::database::PropertyKind::Select
                    | quire_core::core::database::PropertyKind::Status
                    | quire_core::core::database::PropertyKind::MultiSelect
                    | quire_core::core::database::PropertyKind::Relation
            )
        }) {
            if let Ok(stored) = self.session.repo.cell(
                quire_core::core::database::RecordId(record),
                quire_core::core::database::PropertyId(property),
            ) {
                value = stored.display();
            }
        } else if let Some(definition) = definition {
            if definition.kind == quire_core::core::database::PropertyKind::MultiSelect
                && let Ok(quire_core::core::database::CellValue::Items(ids)) =
                    self.session.repo.cell(
                        quire_core::core::database::RecordId(record),
                        quire_core::core::database::PropertyId(property),
                    )
            {
                let options = quire_core::core::database_property::PropertyOptions::from_config(
                    &definition.config,
                );
                value = ids
                    .iter()
                    .map(|id| {
                        options
                            .iter()
                            .find(|option| option.id.to_string() == *id)
                            .map(|option| option.name.as_str())
                            .unwrap_or(id)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
            } else if definition.kind == quire_core::core::database::PropertyKind::Relation
                && let Ok(quire_core::core::database::CellValue::Items(ids)) =
                    self.session.repo.cell(
                        quire_core::core::database::RecordId(record),
                        quire_core::core::database::PropertyId(property),
                    )
            {
                value = ids.join(", ");
            }
        }
        self.active_database_cell = Some((block, db, record, property));
        let caret = value.len();
        self.database_cell_input.update(cx, |input, cx| {
            input.reset(value, Vec::new(), caret, cx);
            input.placeholder = Some("多个选项用逗号分隔".into());
        });
        let input = self.database_cell_input.read(cx);
        window.focus(&input.focus);
        cx.notify();
    }
    fn begin_database_formula_edit(
        &mut self,
        block: u64,
        db: u64,
        property: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        let expression = self
            .session
            .databases
            .properties_of(quire_core::core::database::DatabaseId(db))
            .find(|candidate| candidate.id.0 == property)
            .filter(|candidate| candidate.kind == quire_core::core::database::PropertyKind::Formula)
            .and_then(|candidate| {
                quire_core::core::database_formula::config_formula(&candidate.config)
            })
            .unwrap_or_default();
        self.active_database_formula = Some((block, db, property));
        self.database_formula_input.update(cx, |input, cx| {
            let caret = expression.len();
            input.reset(expression, Vec::new(), caret, cx);
        });
        window.focus(&self.database_formula_input.read(cx).focus);
        cx.notify();
    }
    fn commit_database_formula(
        &mut self,
        restore_focus_after_commit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((block, _db, property)) = self.active_database_formula else {
            return;
        };
        let expression = self
            .database_formula_input
            .read(cx)
            .buffer
            .committed()
            .to_owned();
        match self.session.set_database_formula(
            BlockId(block),
            quire_core::core::database::PropertyId(property),
            &expression,
        ) {
            Ok(_) => {
                self.active_database_formula = None;
                self.error = false;
                self.status = "公式已保存".into();
                self.schedule_save(true, window, cx);
                if restore_focus_after_commit {
                    self.focus_first(window, cx);
                }
                cx.notify();
            }
            Err(error) => {
                self.status = format!("公式未保存：{error}");
                self.error = true;
                cx.notify();
            }
        }
    }
    fn begin_database_option_edit(
        &mut self,
        block: u64,
        db: u64,
        property: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        self.active_database_option = Some((block, db, property));
        self.database_option_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
            input.placeholder = Some("新选项名，按 Enter 添加".into());
        });
        window.focus(&self.database_option_input.read(cx).focus);
        cx.notify();
    }
    fn commit_database_option(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((block, _db, property)) = self.active_database_option else {
            return;
        };
        let name = self
            .database_option_input
            .read(cx)
            .buffer
            .committed()
            .trim()
            .to_owned();
        if name.is_empty() {
            return;
        }
        match self.session.add_database_property_option(
            BlockId(block),
            quire_core::core::database::PropertyId(property),
            &name,
        ) {
            Ok(true) => {
                self.database_option_input.update(cx, |input, cx| {
                    input.reset(String::new(), Vec::new(), 0, cx);
                    input.placeholder = Some("新选项名，按 Enter 添加".into());
                });
                self.status = format!("已添加选项：{name}");
                self.error = false;
                self.schedule_save(true, window, cx);
                cx.notify();
            }
            Ok(false) => {
                self.status = "选项已存在或名称为空".into();
                self.error = false;
                cx.notify();
            }
            Err(error) => {
                self.status = format!("添加选项失败：{error}");
                self.error = true;
                cx.notify();
            }
        }
    }
    fn begin_database_property_name_edit(
        &mut self,
        block: u64,
        db: u64,
        property: u64,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        self.active_database_view_edit = None;
        self.active_database_property_edit = Some((block, db, property));
        self.database_property_name_draft = name.clone();
        let caret = name.len();
        self.database_property_name_input
            .update(cx, |input, cx| input.reset(name, Vec::new(), caret, cx));
        window.focus(&self.database_property_name_input.read(cx).focus);
        cx.notify();
    }
    fn cancel_database_property_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_database_property_edit.take().is_some()
            || self.active_database_view_edit.take().is_some()
        {
            self.database_property_name_draft.clear();
            self.focus_first(window, cx);
            cx.notify();
        }
    }
    fn begin_database_view_name_edit(
        &mut self,
        block: u64,
        db: u64,
        view: u64,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        self.active_database_property_edit = None;
        self.active_database_view_edit = Some((block, db, view));
        self.database_property_name_draft = name.clone();
        let caret = name.len();
        self.database_property_name_input
            .update(cx, |input, cx| input.reset(name, Vec::new(), caret, cx));
        window.focus(&self.database_property_name_input.read(cx).focus);
        cx.notify();
    }
    fn commit_database_property_name(
        &mut self,
        restore_focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_database_view_edit.is_some() {
            self.commit_database_view_name(restore_focus, window, cx);
            return;
        }
        let Some((block, _db, property)) = self.active_database_property_edit else {
            return;
        };
        let name = self
            .database_property_name_input
            .read(cx)
            .buffer
            .committed()
            .to_owned();
        if name.trim().is_empty() {
            if restore_focus {
                self.status = "属性名称不能为空".into();
                self.error = true;
                window.focus(&self.database_property_name_input.read(cx).focus);
            } else {
                self.active_database_property_edit = None;
                self.database_property_name_draft.clear();
            }
            cx.notify();
            return;
        }
        self.active_database_property_edit = None;
        match self.session.rename_database_property(
            BlockId(block),
            quire_core::core::database::PropertyId(property),
            &name,
        ) {
            Ok(true) => self.schedule_save(true, window, cx),
            Ok(false) => {}
            Err(error) => {
                self.status = error.to_string();
                self.error = true;
            }
        }
        self.database_property_name_draft.clear();
        if restore_focus {
            self.focus_first(window, cx);
        }
        cx.notify();
    }
    fn commit_database_view_name(
        &mut self,
        restore_focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((block, _db, view)) = self.active_database_view_edit else {
            return;
        };
        let name = self
            .database_property_name_input
            .read(cx)
            .buffer
            .committed()
            .to_owned();
        if name.trim().is_empty() {
            if restore_focus {
                self.status = "视图名称不能为空".into();
                self.error = true;
                window.focus(&self.database_property_name_input.read(cx).focus);
            } else {
                self.active_database_view_edit = None;
                self.database_property_name_draft.clear();
            }
            cx.notify();
            return;
        }
        self.active_database_view_edit = None;
        match self
            .session
            .rename_database_view(BlockId(block), ViewId(view), &name)
        {
            Ok(true) => {
                self.error = false;
                self.schedule_save(true, window, cx);
            }
            Ok(false) => {}
            Err(error) => {
                self.status = error.to_string();
                self.error = true;
            }
        }
        self.database_property_name_draft.clear();
        if restore_focus {
            self.focus_first(window, cx);
        }
        cx.notify();
    }
    fn schedule_database_header_sort(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        property: quire_core::core::database::PropertyId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.database_header_click_seq = self.database_header_click_seq.wrapping_add(1);
        let sequence = self.database_header_click_seq;
        cx.spawn_in(window, async move |weak, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                if this.database_header_click_seq == sequence {
                    this.toggle_database_sort(block, db, view_id, property, window, cx);
                }
            });
        })
        .detach();
    }
    fn toggle_database_sort(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        property: quire_core::core::database::PropertyId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        let Some(view) = self
            .session
            .databases
            .views
            .iter()
            .find(|view| view.id == view_id && view.db == db)
        else {
            return;
        };
        let mut definition =
            quire_core::core::database_view::ViewDefinition::parse(&view.definition);
        let mut sorts = definition.rules(db, &self.session.databases).sorts;
        match sorts.iter().position(|sort| sort.property == property) {
            Some(index) if !sorts[index].descending => sorts[index].descending = true,
            Some(index) => {
                sorts.remove(index);
            }
            None => {
                let Some(column) = self
                    .session
                    .databases
                    .properties_of(db)
                    .find(|column| column.id == property)
                else {
                    return;
                };
                if let Some(sort) = quire_core::core::database::SortSpec::of(column, false) {
                    sorts.push(sort);
                } else {
                    return;
                }
            }
        }
        definition.set_sorts(&sorts);
        match self
            .session
            .set_database_view_definition(block, view_id, definition.to_text())
        {
            Ok(true) => self.schedule_save(true, window, cx),
            Ok(false) => {}
            Err(error) => {
                self.status = format!("数据库排序保存失败：{error}");
                self.error = true;
                cx.notify();
            }
        }
    }
    fn edit_database_filter(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        edit: impl FnOnce(&mut quire_core::core::database_view::FlatFilter),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        let Some(view) = self
            .session
            .databases
            .views
            .iter()
            .find(|view| view.id == view_id && view.db == db)
        else {
            return;
        };
        let mut definition =
            quire_core::core::database_view::ViewDefinition::parse(&view.definition);
        let rules = definition.rules(db, &self.session.databases);
        let Some(mut flat) =
            quire_core::core::database_view::FlatFilter::from_tree(rules.filter.as_ref())
        else {
            self.status = "此视图含嵌套筛选规则，当前无法安全编辑".into();
            self.error = false;
            cx.notify();
            return;
        };
        edit(&mut flat);
        let tree = flat.to_tree();
        definition.set_filter(Some(&tree));
        match self
            .session
            .set_database_view_definition(block, view_id, definition.to_text())
        {
            Ok(true) => {
                self.error = false;
                self.schedule_save(true, window, cx);
                cx.notify();
            }
            Ok(false) => cx.notify(),
            Err(error) => {
                self.status = format!("数据库筛选保存失败：{error}");
                self.error = true;
                cx.notify();
            }
        }
    }
    fn set_database_group(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        property: Option<quire_core::core::database::PropertyId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        if let Some(property) = property {
            let Some(column) = self
                .session
                .databases
                .properties_of(db)
                .find(|column| column.id == property)
            else {
                return;
            };
            if !quire_core::core::database_view::GroupSpec::admits(column.kind) {
                return;
            }
        }
        let Some(view) = self
            .session
            .databases
            .views
            .iter()
            .find(|view| view.id == view_id && view.db == db)
        else {
            return;
        };
        let mut definition =
            quire_core::core::database_view::ViewDefinition::parse(&view.definition);
        definition.set_group(property);
        match self
            .session
            .set_database_view_definition(block, view_id, definition.to_text())
        {
            Ok(true) => {
                self.database_group_open = None;
                self.database_collapsed_groups
                    .retain(|(collapsed_block, collapsed_view, _)| {
                        *collapsed_block != block.0 || *collapsed_view != view_id.0
                    });
                self.schedule_save(true, window, cx);
                cx.notify();
            }
            Ok(false) => {
                self.database_group_open = None;
                cx.notify();
            }
            Err(error) => {
                self.status = format!("数据库分组保存失败：{error}");
                self.error = true;
                cx.notify();
            }
        }
    }
    fn set_database_filter_match(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        any: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_database_filter(block, db, view_id, |flat| flat.any = any, window, cx);
    }
    fn add_database_filter_clause(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        property: quire_core::core::database::PropertyId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(column) = self
            .session
            .databases
            .properties_of(db)
            .find(|column| column.id == property)
            .cloned()
        else {
            return;
        };
        let Some(op) = quire_core::core::database_view::FilterOp::ops_for(column.kind)
            .first()
            .copied()
        else {
            return;
        };
        let mut index = 0;
        self.edit_database_filter(
            block,
            db,
            view_id,
            |flat| {
                index = flat.clauses.len();
                flat.clauses
                    .push(quire_core::core::database_view::FlatClause {
                        clause: quire_core::core::database_view::FilterClause {
                            property,
                            kind: column.kind,
                            op,
                            value: quire_core::core::database_view::FilterValue::Missing,
                        },
                        invert: false,
                    });
            },
            window,
            cx,
        );
        self.database_filter_edit_row = Some(index);
        self.database_filter_panel = 0;
    }
    fn remove_database_filter_clause(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_database_filter(
            block,
            db,
            view_id,
            |flat| {
                if row < flat.clauses.len() {
                    flat.clauses.remove(row);
                }
            },
            window,
            cx,
        );
        self.database_filter_edit_row = None;
    }
    fn set_database_filter_op(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        row: usize,
        op: quire_core::core::database_view::FilterOp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_database_filter(
            block,
            db,
            view_id,
            |flat| {
                if let Some(clause) = flat.clauses.get_mut(row)
                    && quire_core::core::database_view::FilterOp::ops_for(clause.clause.kind)
                        .contains(&op)
                {
                    clause.clause.op = op;
                    clause.clause.value = quire_core::core::database_view::FilterValue::Missing;
                }
            },
            window,
            cx,
        );
        self.database_filter_edit_row = Some(row);
        self.database_filter_panel = 0;
    }
    fn toggle_database_filter_not(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_database_filter(
            block,
            db,
            view_id,
            |flat| {
                if let Some(clause) = flat.clauses.get_mut(row) {
                    clause.invert = !clause.invert;
                }
            },
            window,
            cx,
        );
    }
    fn clear_database_filter(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_database_filter(block, db, view_id, |flat| flat.clauses.clear(), window, cx);
        self.database_filter_edit_row = None;
    }
    fn toggle_database_filter_option(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        row: usize,
        option: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_database_filter(
            block,
            db,
            view_id,
            move |flat| {
                let Some(clause) = flat.clauses.get_mut(row) else {
                    return;
                };
                if clause.clause.op == quire_core::core::database_view::FilterOp::AnyOf {
                    let mut items = match &clause.clause.value {
                        quire_core::core::database_view::FilterValue::Any(items) => items.clone(),
                        quire_core::core::database_view::FilterValue::Text(item) => {
                            vec![item.clone()]
                        }
                        _ => Vec::new(),
                    };
                    if let Some(at) = items.iter().position(|item| *item == option) {
                        items.remove(at);
                    } else {
                        items.push(option);
                    }
                    clause.clause.value = if items.is_empty() {
                        quire_core::core::database_view::FilterValue::Missing
                    } else {
                        quire_core::core::database_view::FilterValue::Any(items)
                    };
                } else {
                    clause.clause.value = match &clause.clause.value {
                        quire_core::core::database_view::FilterValue::Text(item)
                            if *item == option =>
                        {
                            quire_core::core::database_view::FilterValue::Missing
                        }
                        _ => quire_core::core::database_view::FilterValue::Text(option),
                    };
                }
            },
            window,
            cx,
        );
        self.database_filter_panel = 0;
    }
    fn set_database_filter_flag(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        row: usize,
        checked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_database_filter(
            block,
            db,
            view_id,
            |flat| {
                if let Some(clause) = flat.clauses.get_mut(row) {
                    clause.clause.value =
                        quire_core::core::database_view::FilterValue::Flag(checked);
                }
            },
            window,
            cx,
        );
    }
    fn begin_database_filter_value(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self
            .session
            .databases
            .views
            .iter()
            .find(|view| view.id == view_id && view.db == db)
        else {
            return;
        };
        let rules = quire_core::core::database_view::ViewDefinition::parse(&view.definition)
            .rules(db, &self.session.databases);
        let Some(clause) =
            quire_core::core::database_view::FlatFilter::from_tree(rules.filter.as_ref())
                .and_then(|flat| flat.clauses.get(row).map(|flat| flat.clause.clone()))
        else {
            return;
        };
        let text = match clause.value {
            quire_core::core::database_view::FilterValue::Missing => String::new(),
            quire_core::core::database_view::FilterValue::Text(text) => text,
            quire_core::core::database_view::FilterValue::Number(number) => number.to_string(),
            quire_core::core::database_view::FilterValue::Flag(flag) => flag.to_string(),
            quire_core::core::database_view::FilterValue::Any(values) => values.join(","),
        };
        self.database_filter_value_active = Some((block.0, db.0, view_id.0, row));
        self.database_filter_value_input.update(cx, |input, cx| {
            let len = text.len();
            input.reset(text, Vec::new(), len, cx);
            window.focus(&input.focus);
        });
        cx.notify();
    }
    fn commit_database_filter_value(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((block, db, view, row)) = self.database_filter_value_active else {
            return;
        };
        let text = self
            .database_filter_value_input
            .read(cx)
            .buffer
            .committed()
            .to_owned();
        let db = quire_core::core::database::DatabaseId(db);
        let view_id = ViewId(view);
        let current_view = self
            .session
            .databases
            .views
            .iter()
            .find(|candidate| candidate.id == view_id && candidate.db == db);
        let kind = current_view.and_then(|current_view| {
            let rules =
                quire_core::core::database_view::ViewDefinition::parse(&current_view.definition)
                    .rules(db, &self.session.databases);
            quire_core::core::database_view::FlatFilter::from_tree(rules.filter.as_ref())
                .and_then(|flat| flat.clauses.get(row).map(|clause| clause.clause.kind))
        });
        let Some(value) = kind.and_then(|kind| database_filter_value_from_text(kind, &text)) else {
            self.status = "筛选值无效；数字需为有限数值，日期请使用 YYYY-MM-DD".into();
            self.error = true;
            cx.notify();
            return;
        };
        self.edit_database_filter(
            BlockId(block),
            db,
            view_id,
            move |flat| {
                let Some(clause) = flat.clauses.get_mut(row) else {
                    return;
                };
                clause.clause.value = value;
            },
            window,
            cx,
        );
        self.database_filter_value_active = None;
        if let Some(input) = self.inputs.values().next() {
            let focus = input.read(cx).focus.clone();
            window.focus(&focus);
        }
    }
    fn toggle_database_column(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view_id: ViewId,
        property: quire_core::core::database::PropertyId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked {
            return;
        }
        let Some(view) = self
            .session
            .databases
            .views
            .iter()
            .find(|view| view.id == view_id && view.db == db)
        else {
            return;
        };
        let all = quire_core::core::database_view::all_columns(&self.session.databases, db);
        let mut definition =
            quire_core::core::database_view::ViewDefinition::parse(&view.definition);
        if definition.shows(property, &all) {
            definition.hide(property, &all);
        } else {
            definition.show(property, &all);
        }
        match self
            .session
            .set_database_view_definition(block, view_id, definition.to_text())
        {
            Ok(true) => self.schedule_save(true, window, cx),
            Ok(false) => {}
            Err(error) => {
                self.status = format!("数据库列设置保存失败：{error}");
                self.error = true;
                cx.notify();
            }
        }
    }
    fn set_database_property_kind(
        &mut self,
        block: BlockId,
        property: quire_core::core::database::PropertyId,
        kind: quire_core::core::database::PropertyKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .session
            .set_database_property_kind(block, property, kind)
        {
            Ok(true) => {
                self.status = "列类型已更改；已有值不会自动转换".into();
                self.error = false;
                self.schedule_save(true, window, cx);
                if matches!(
                    kind,
                    quire_core::core::database::PropertyKind::Formula
                        | quire_core::core::database::PropertyKind::Select
                        | quire_core::core::database::PropertyKind::Status
                        | quire_core::core::database::PropertyKind::MultiSelect
                        | quire_core::core::database::PropertyKind::Relation
                        | quire_core::core::database::PropertyKind::Rollup
                ) {
                    if let Some(db) = self
                        .session
                        .databases
                        .properties
                        .iter()
                        .find(|candidate| candidate.id == property)
                        .map(|candidate| candidate.db)
                    {
                        self.database_property_kind_open = Some((db.0, property.0));
                        if kind == quire_core::core::database::PropertyKind::Formula {
                            self.begin_database_formula_edit(block.0, db.0, property.0, window, cx);
                        } else if !matches!(
                            kind,
                            quire_core::core::database::PropertyKind::Relation
                                | quire_core::core::database::PropertyKind::Rollup
                        ) {
                            self.begin_database_option_edit(block.0, db.0, property.0, window, cx);
                        }
                    }
                } else {
                    self.database_property_kind_open = None;
                }
            }
            Ok(false) => {
                if matches!(
                    kind,
                    quire_core::core::database::PropertyKind::Formula
                        | quire_core::core::database::PropertyKind::Select
                        | quire_core::core::database::PropertyKind::Status
                        | quire_core::core::database::PropertyKind::MultiSelect
                        | quire_core::core::database::PropertyKind::Relation
                        | quire_core::core::database::PropertyKind::Rollup
                ) {
                    if let Some(db) = self
                        .session
                        .databases
                        .properties
                        .iter()
                        .find(|candidate| candidate.id == property)
                        .map(|candidate| candidate.db)
                    {
                        self.database_property_kind_open = Some((db.0, property.0));
                        if kind == quire_core::core::database::PropertyKind::Formula {
                            self.begin_database_formula_edit(block.0, db.0, property.0, window, cx);
                        } else if !matches!(
                            kind,
                            quire_core::core::database::PropertyKind::Relation
                                | quire_core::core::database::PropertyKind::Rollup
                        ) {
                            self.begin_database_option_edit(block.0, db.0, property.0, window, cx);
                        }
                    }
                } else {
                    self.database_property_kind_open = None;
                }
            }
            Err(error) => {
                self.status = error.to_string();
                self.error = true;
                self.database_property_kind_open = None;
            }
        }
        cx.notify();
    }
    fn open_database_search(&mut self, db: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.database_search_open = Some(db);
        self.database_search_text.clear();
        self.database_search_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
            input.placeholder = Some("搜索此数据库…".into());
        });
        window.focus(&self.database_search_input.read(cx).focus);
        cx.notify();
    }
    fn toggle_database_relation_picker(
        &mut self,
        key: (u64, u64, u64, u64),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_database_relation == Some(key) {
            self.active_database_relation = None;
            self.database_relation_search_text.clear();
            self.restore_focus(window, cx);
        } else {
            self.active_database_relation = Some(key);
            self.database_relation_search_text.clear();
            self.database_relation_search_input.update(cx, |input, cx| {
                input.reset(String::new(), Vec::new(), 0, cx);
                input.placeholder = Some("搜索目标记录…".into());
            });
            window.focus(&self.database_relation_search_input.read(cx).focus);
        }
        cx.notify();
    }
    fn close_database_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.database_search_open.take().is_none() {
            return;
        }
        self.database_search_text.clear();
        self.database_search_input.update(cx, |input, cx| {
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        self.restore_focus(window, cx);
        cx.notify();
    }
    fn begin_database_column_drag(
        &mut self,
        block: BlockId,
        db: quire_core::core::database::DatabaseId,
        view: ViewId,
        property: quire_core::core::database::PropertyId,
        width: f32,
        grid_width: f32,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.page.locked || grid_width <= 0.0 {
            return;
        }
        let zoom = quire_gpui::ui_scale::get();
        self.database_column_drag = Some(DatabaseColumnDrag {
            block,
            db,
            view,
            property,
            start_x: f32::from(event.position.x) / zoom,
            start_width: width,
            grid_width,
            preview_width: width,
        });
        cx.stop_propagation();
        cx.notify();

        let workspace = cx.entity();
        window.on_mouse_event({
            let workspace = workspace.clone();
            move |event: &MouseMoveEvent, _, _, cx| {
                if !event.dragging() {
                    return;
                }
                workspace.update(cx, |this, cx| {
                    if let Some(drag) = &mut this.database_column_drag {
                        let zoom = quire_gpui::ui_scale::get();
                        let x = f32::from(event.position.x) / zoom;
                        drag.preview_width = (drag.start_width + x - drag.start_x)
                            .clamp(drag.grid_width * 0.06, drag.grid_width * 1.06);
                        cx.notify();
                    }
                });
            }
        });
        window.on_mouse_event(move |event: &MouseUpEvent, _, window, cx| {
            workspace.update(cx, |this, cx| {
                let Some(mut drag) = this.database_column_drag.take() else {
                    return;
                };
                let zoom = quire_gpui::ui_scale::get();
                let x = f32::from(event.position.x) / zoom;
                drag.preview_width = (drag.start_width + x - drag.start_x)
                    .clamp(drag.grid_width * 0.06, drag.grid_width * 1.06);
                let permille = ((drag.preview_width * 1000.0 / drag.grid_width).round() as u16)
                    .clamp(60, 1060);
                if let Some(view) = this
                    .session
                    .databases
                    .views
                    .iter()
                    .find(|view| view.id == drag.view && view.db == drag.db)
                {
                    let mut definition =
                        quire_core::core::database_view::ViewDefinition::parse(&view.definition);
                    definition.set_width(drag.property, permille);
                    match this.session.set_database_view_definition(
                        drag.block,
                        drag.view,
                        definition.to_text(),
                    ) {
                        Ok(true) => this.schedule_save(true, window, cx),
                        Ok(false) => {}
                        Err(error) => {
                            this.status = format!("数据库列宽保存失败：{error}");
                            this.error = true;
                        }
                    }
                }
                cx.notify();
            });
        });
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
        let property_id = quire_core::core::database::PropertyId(property);
        let result = if self.session.databases.properties.iter().any(|definition| {
            definition.id == property_id
                && definition.kind == quire_core::core::database::PropertyKind::MultiSelect
        }) {
            let items = value
                .split([',', '，'])
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            self.session.set_database_cell_many(
                BlockId(block),
                quire_core::core::database::RecordId(record),
                property_id,
                &items,
            )
        } else if self.session.databases.properties.iter().any(|definition| {
            definition.id == property_id
                && definition.kind == quire_core::core::database::PropertyKind::Relation
        }) {
            let targets = value
                .split([',', '，'])
                .map(str::trim)
                .filter_map(|value| value.parse::<u64>().ok())
                .map(quire_core::core::database::RecordId)
                .collect::<Vec<_>>();
            self.session.set_database_relation_cell(
                BlockId(block),
                quire_core::core::database::RecordId(record),
                property_id,
                &targets,
            )
        } else {
            self.session.set_database_cell(
                BlockId(block),
                quire_core::core::database::RecordId(record),
                property_id,
                &value,
            )
        };
        match result {
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
    fn set_database_choice(
        &mut self,
        block: u64,
        record: u64,
        property: u64,
        value: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.session.set_database_property_option_cell(
            BlockId(block),
            quire_core::core::database::RecordId(record),
            quire_core::core::database::PropertyId(property),
            value.and_then(|value| value.parse::<u64>().ok()),
        ) {
            Ok(_) => {
                self.active_database_choice = None;
                self.selected_database_record = Some((
                    block,
                    self.session
                        .document
                        .block(BlockId(block))
                        .and_then(|block| block.db_ref)
                        .map_or(0, |db| db.0),
                    record,
                ));
                self.schedule_save(true, window, cx);
                cx.notify();
            }
            Err(error) => {
                self.status = error.to_string();
                self.error = true;
                cx.notify();
            }
        }
    }
    fn toggle_database_relation_target(
        &mut self,
        block: u64,
        record: u64,
        property: u64,
        target: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let record = quire_core::core::database::RecordId(record);
        let property = quire_core::core::database::PropertyId(property);
        let mut ids = match self.session.repo.cell(record, property) {
            Ok(quire_core::core::database::CellValue::Items(ids)) => ids
                .into_iter()
                .filter_map(|id| id.parse::<u64>().ok())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        if let Some(index) = ids.iter().position(|id| *id == target) {
            ids.remove(index);
        } else {
            ids.push(target);
        }
        let ids = ids
            .into_iter()
            .map(quire_core::core::database::RecordId)
            .collect::<Vec<_>>();
        match self
            .session
            .set_database_relation_cell(BlockId(block), record, property, &ids)
        {
            Ok(_) => {
                self.schedule_save(true, window, cx);
                cx.notify();
            }
            Err(error) => {
                self.status = error.to_string();
                self.error = true;
                cx.notify();
            }
        }
    }
    fn clear_database_relation(
        &mut self,
        block: u64,
        record: u64,
        property: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.session.set_database_relation_cell(
            BlockId(block),
            quire_core::core::database::RecordId(record),
            quire_core::core::database::PropertyId(property),
            &[],
        ) {
            Ok(_) => {
                self.schedule_save(true, window, cx);
                cx.notify();
            }
            Err(error) => {
                self.status = error.to_string();
                self.error = true;
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
    fn arm_organizer_draft_debounce(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.organizer_debounce_seq = self.organizer_debounce_seq.saturating_add(1);
        let token = self.organizer_debounce_seq;
        cx.spawn_in(window, async move |weak, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                if this.organizer_debounce_seq == token {
                    this.commit_note_draft(window, cx);
                }
            });
        })
        .detach();
    }
    fn commit_note_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Explicit commits (blur, selection changes, delete, close) invalidate
        // the pending keystroke timer so it cannot write against a new selection.
        self.organizer_debounce_seq = self.organizer_debounce_seq.saturating_add(1);
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
        self.defer_organizer_delete(
            vec![id.0],
            false,
            format!("笔记「{title}」已转为待办"),
            window,
            cx,
        );
        self.selected_note = None;
        self.selected_task = Some(task_id);
        self.organizer_tasks = true;
        self.note_title_draft.clear();
        self.note_body_draft.clear();
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
        self.task_list_menu_open = None;
        self.task_list_color_open = None;
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
        self.task_list_menu_open = None;
        self.task_list_color_open = None;
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
        self.task_list_menu_open = None;
        self.task_list_color_open = None;
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
        if ids.is_empty() {
            return;
        }
        self.selected_task_ids.clear();
        self.defer_organizer_delete(
            ids.iter().map(|id| id.0).collect(),
            true,
            format!("已移入回收站：{} 项任务", ids.len()),
            window,
            cx,
        );
    }
    fn create_task_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let number = self.session.organizer.lists.len() + 1;
        let id = self.session.create_task_list(format!("清单 {number}"));
        self.selected_task_list = id;
        self.selected_task_smart_view = None;
        self.selected_task = None;
        self.schedule_save(true, window, cx);
    }
    fn start_task_list_rename(
        &mut self,
        list: quire_core::core::organizer::ListId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .session
            .organizer
            .list(list)
            .map(|list| list.name.clone())
        else {
            return;
        };
        self.task_list_menu_open = None;
        self.task_list_color_open = None;
        self.task_list_editing = Some(list);
        self.task_list_name_draft = name.clone();
        self.task_list_name_input.update(cx, |input, cx| {
            input.reset(name.clone(), Vec::new(), name.len(), cx)
        });
        window.focus(&self.task_list_name_input.read(cx).focus);
        cx.notify();
    }
    fn cancel_task_list_rename(&mut self, cx: &mut Context<Self>) {
        self.task_list_editing = None;
        self.task_list_name_draft.clear();
        cx.notify();
    }
    fn toggle_task_list_menu(
        &mut self,
        id: quire_core::core::organizer::ListId,
        cx: &mut Context<Self>,
    ) {
        self.task_list_menu_open = (self.task_list_menu_open != Some(id)).then_some(id);
        self.task_list_color_open = None;
        cx.notify();
    }
    fn task_list_menu_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key.as_str() != "escape"
            || event.keystroke.modifiers.modified()
            || self.task_list_menu_open.is_none()
        {
            return;
        }
        if self.task_list_color_open.take().is_none() {
            self.task_list_menu_open = None;
        }
        cx.stop_propagation();
        cx.notify();
    }
    fn commit_task_list_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.task_list_editing.take() else {
            return;
        };
        self.task_list_menu_open = None;
        self.task_list_color_open = None;
        let Some(list) = self.session.organizer.list(id) else {
            return;
        };
        let name = self.task_list_name_draft.trim().to_owned();
        if name.is_empty() {
            self.status = "清单名称不能为空".into();
            self.error = true;
            cx.notify();
            return;
        }
        if self.session.update_task_list(id, name, list.color) {
            self.status = "清单名称已更新（Ctrl+Z 可撤销）".into();
            self.error = false;
            self.schedule_save(true, window, cx);
        }
        cx.notify();
    }
    fn toggle_task_list_color_picker(
        &mut self,
        id: quire_core::core::organizer::ListId,
        cx: &mut Context<Self>,
    ) {
        self.task_list_color_open = (self.task_list_color_open != Some(id)).then_some(id);
        cx.notify();
    }
    fn set_task_list_color(
        &mut self,
        id: quire_core::core::organizer::ListId,
        color: ColorKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(list) = self.session.organizer.list(id) else {
            return;
        };
        let name = list.name.clone();
        self.task_list_menu_open = None;
        self.task_list_color_open = None;
        if self.session.update_task_list(id, name, color) {
            self.status = "清单颜色已更新（Ctrl+Z 可撤销）".into();
            self.error = false;
            self.schedule_save(true, window, cx);
        }
        cx.notify();
    }
    fn delete_task_list(
        &mut self,
        list: quire_core::core::organizer::ListId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.task_list_menu_open = None;
        self.task_list_color_open = None;
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
        let Some(task) = self.session.organizer.task(id) else {
            return;
        };
        let message = format!("已移入回收站：任务「{}」", task.title);
        self.defer_organizer_delete(vec![id.0], true, message, window, cx);
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
        let Some(note) = self.session.organizer.note(id) else {
            return;
        };
        let title = if note.title.trim().is_empty() {
            "无标题"
        } else {
            note.title.as_str()
        };
        self.defer_organizer_delete(
            vec![id.0],
            false,
            format!("已移入回收站：笔记「{title}」"),
            window,
            cx,
        );
    }
    fn defer_organizer_delete(
        &mut self,
        ids: Vec<u64>,
        is_task: bool,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ids.is_empty() {
            return;
        }
        self.commit_note_draft(window, cx);
        let (token, committed_previous) = if is_task {
            let task_ids = ids
                .iter()
                .copied()
                .map(quire_core::core::organizer::TaskId)
                .collect::<Vec<_>>();
            self.session.defer_trash_tasks(&task_ids, message.clone())
        } else {
            let note_ids = ids
                .iter()
                .copied()
                .map(quire_core::core::organizer::NoteId)
                .collect::<Vec<_>>();
            self.session.defer_trash_notes(&note_ids, message.clone())
        };
        if committed_previous {
            self.schedule_save(true, window, cx);
        }
        if is_task {
            if self
                .selected_task
                .is_some_and(|selected| ids.contains(&selected.0))
            {
                self.selected_task = None;
                self.reset_organizer_item_inputs(cx);
            }
            self.selected_task_ids
                .retain(|selected| !ids.contains(&selected.0));
        } else if self
            .selected_note
            .is_some_and(|selected| ids.contains(&selected.0))
        {
            self.selected_note = None;
            self.note_title_draft.clear();
            self.note_body_draft.clear();
            self.organizer_tags_draft.clear();
            self.reset_organizer_item_inputs(cx);
        }
        cx.spawn_in(window, async move |weak, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(3_000))
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                if this.session.commit_pending_organizer_delete(token) {
                    this.schedule_save(true, window, cx);
                } else {
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn undo_organizer_delete(&mut self, cx: &mut Context<Self>) {
        if self.session.undo_pending_organizer_delete() {
            cx.notify();
        }
    }
    fn toggle_organizer_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_note_draft(window, cx);
        self.task_list_menu_open = None;
        self.task_list_color_open = None;
        self.organizer_trash = !self.organizer_trash;
        self.selected_task_ids.clear();
        self.selected_note = None;
        self.selected_task = None;
        self.reset_organizer_item_inputs(cx);
    }
    fn toggle_organizer_tag(&mut self, tag: String, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_note_draft(window, cx);
        // A path cannot be both included and excluded. Selecting it removes
        // the exclusion, matching the reference organizer's filter rule.
        self.organizer_excluded_tags.remove(&tag);
        self.organizer_tag = if tag.is_empty() {
            None
        } else {
            (self.organizer_tag.as_deref() != Some(tag.as_str())).then_some(tag)
        };
        if self.organizer_tag.is_some() {
            self.organizer_trash = false;
        }
        self.selected_task_ids.clear();
        self.selected_note = None;
        self.selected_task = None;
        self.reset_organizer_item_inputs(cx);
    }
    fn toggle_organizer_tag_exclusion(
        &mut self,
        tag: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_note_draft(window, cx);
        if !self.organizer_excluded_tags.insert(tag.clone()) {
            self.organizer_excluded_tags.remove(&tag);
        } else if self.organizer_tag.as_deref() == Some(tag.as_str()) {
            self.organizer_tag = None;
        }
        self.selected_task_ids.clear();
        self.selected_note = None;
        self.selected_task = None;
        self.reset_organizer_item_inputs(cx);
    }
    fn clear_organizer_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_note_draft(window, cx);
        self.organizer_tag = None;
        self.organizer_excluded_tags.clear();
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
    fn purge_note(
        &mut self,
        id: quire_core::core::organizer::NoteId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_note_draft(window, cx);
        let Some(title) = self
            .session
            .organizer
            .note(id)
            .map(|note| note.title.clone())
        else {
            return;
        };
        if self.session.purge_organizer_note(id) {
            if self.selected_note == Some(id) {
                self.selected_note = None;
                self.reset_organizer_item_inputs(cx);
            }
            self.status = format!("已彻底删除「{title}」— Ctrl+Z 可撤销");
            self.schedule_save(true, window, cx);
        }
    }
    fn purge_task(
        &mut self,
        id: quire_core::core::organizer::TaskId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(title) = self
            .session
            .organizer
            .task(id)
            .map(|task| task.title.clone())
        else {
            return;
        };
        if self.session.purge_organizer_task(id) {
            if self.selected_task == Some(id) {
                self.selected_task = None;
            }
            self.status = format!("已彻底删除「{title}」— Ctrl+Z 可撤销");
            self.schedule_save(true, window, cx);
        }
    }
    fn empty_organizer_bin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_note_draft(window, cx);
        let count = self.session.empty_organizer_bin(self.organizer_tasks);
        self.selected_note = None;
        self.selected_task = None;
        self.selected_task_ids.clear();
        self.reset_organizer_item_inputs(cx);
        self.status = if count == 0 {
            "回收站已经是空的".into()
        } else {
            format!("已清空回收站，{count} 项 — Ctrl+Z 可撤销")
        };
        if count > 0 {
            self.schedule_save(true, window, cx);
        } else {
            cx.notify();
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
        if self
            .session
            .pages
            .iter()
            .any(|page| page.id == id && !page.template)
        {
            Self::push_page_history(&mut self.page_history_back, self.session.page.id);
            self.page_history_forward.clear();
        }
        self.activate_page_without_history(id, window, cx);
    }
    fn push_page_history(
        history: &mut Vec<quire_core::core::types::PageId>,
        page: quire_core::core::types::PageId,
    ) {
        push_history(history, page);
    }
    fn navigate_page_history(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.closing || self.composing(cx) {
            return;
        }
        let target = {
            let history = if forward {
                &mut self.page_history_forward
            } else {
                &mut self.page_history_back
            };
            loop {
                let Some(candidate) = history.pop() else {
                    break None;
                };
                if self
                    .session
                    .pages
                    .iter()
                    .any(|page| page.id == candidate && !page.template)
                {
                    break Some(candidate);
                }
            }
        };
        let Some(target) = target else { return };
        let current = self.session.page.id;
        let opposite = if forward {
            &mut self.page_history_back
        } else {
            &mut self.page_history_forward
        };
        Self::push_page_history(opposite, current);
        self.activate_page_without_history(target, window, cx);
    }
    fn activate_page_without_history(
        &mut self,
        id: quire_core::core::types::PageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    fn palette_items(&self) -> Vec<PaletteItem> {
        let mut items = vec![
            PaletteItem {
                name: "新建页面".into(),
                hint: "Ctrl+N",
                section: "编辑",
                icon: "+",
                action: PaletteCommand::NewPage,
            },
            PaletteItem {
                name: "搜索页面…".into(),
                hint: "Ctrl+P",
                section: "导航",
                icon: "⌕",
                action: PaletteCommand::SearchPages,
            },
            PaletteItem {
                name: "切换侧边栏".into(),
                hint: "",
                section: "界面",
                icon: "▤",
                action: PaletteCommand::ToggleSidebar,
            },
            PaletteItem {
                name: "切换深色模式".into(),
                hint: "",
                section: "界面",
                icon: "◐",
                action: PaletteCommand::ToggleTheme,
            },
            PaletteItem {
                name: "设置".into(),
                hint: "",
                section: "导航",
                icon: "⚙",
                action: PaletteCommand::Settings,
            },
            PaletteItem {
                name: "打开笔记".into(),
                hint: "",
                section: "导航",
                icon: "▧",
                action: PaletteCommand::OpenNotes,
            },
            PaletteItem {
                name: "打开任务".into(),
                hint: "",
                section: "导航",
                icon: "✓",
                action: PaletteCommand::OpenTasks,
            },
            PaletteItem {
                name: "重命名页面".into(),
                hint: "F2",
                section: "页面",
                icon: "✎",
                action: PaletteCommand::RenamePage,
            },
            PaletteItem {
                name: "复制页面".into(),
                hint: "",
                section: "页面",
                icon: "⧉",
                action: PaletteCommand::DuplicatePage,
            },
            PaletteItem {
                name: "删除页面".into(),
                hint: "",
                section: "页面",
                icon: "⌫",
                action: PaletteCommand::DeletePage,
            },
            PaletteItem {
                name: "导出页面为 Markdown…".into(),
                hint: "",
                section: "页面",
                icon: "⇧",
                action: PaletteCommand::ExportMarkdown,
            },
            PaletteItem {
                name: "导入 Markdown…".into(),
                hint: "",
                section: "页面",
                icon: "⇩",
                action: PaletteCommand::ImportMarkdown,
            },
            PaletteItem {
                name: "复制页面为 Markdown".into(),
                hint: "",
                section: "页面",
                icon: "⧉",
                action: PaletteCommand::CopyMarkdown,
            },
            PaletteItem {
                name: "将当前页面保存为模板".into(),
                hint: "",
                section: "模板",
                icon: "▧",
                action: PaletteCommand::SaveCurrentAsTemplate,
            },
            PaletteItem {
                name: "从 Markdown 导入模板…".into(),
                hint: "",
                section: "模板",
                icon: "⇧",
                action: PaletteCommand::ImportTemplate,
            },
            PaletteItem {
                name: "后退".into(),
                hint: "Alt+←",
                section: "导航",
                icon: "←",
                action: PaletteCommand::NavigateBack,
            },
            PaletteItem {
                name: "前进".into(),
                hint: "Alt+→",
                section: "导航",
                icon: "→",
                action: PaletteCommand::NavigateForward,
            },
            PaletteItem {
                name: "在浏览器中打开 HTML…".into(),
                hint: "",
                section: "页面",
                icon: "↗",
                action: PaletteCommand::OpenHtml,
            },
        ];
        let mut pages = self
            .session
            .pages
            .iter()
            .filter(|page| !page.template)
            .collect::<Vec<_>>();
        pages.sort_by_key(|page| (page.parent, page.order));
        items.extend(pages.into_iter().map(|page| PaletteItem {
            name: page.title.clone(),
            hint: "",
            section: "跳转到页面",
            icon: "▤",
            action: PaletteCommand::OpenPage(page.id),
        }));
        for template in self.session.templates() {
            items.push(PaletteItem {
                name: format!("插入模板：{}", template.title),
                hint: "",
                section: "模板",
                icon: "⇩",
                action: PaletteCommand::InsertTemplate(template.id),
            });
            items.push(PaletteItem {
                name: format!("从模板新建页面：{}", template.title),
                hint: "",
                section: "模板",
                icon: "+",
                action: PaletteCommand::NewPageFromTemplate(template.id),
            });
            items.push(PaletteItem {
                name: format!("删除模板：{}", template.title),
                hint: "",
                section: "模板",
                icon: "×",
                action: PaletteCommand::DeleteTemplate(template.id),
            });
            items.push(PaletteItem {
                name: format!("导出模板为 Markdown：{}", template.title),
                hint: "",
                section: "模板",
                icon: "⇧",
                action: PaletteCommand::ExportTemplateMarkdown(template.id),
            });
        }
        items
            .into_iter()
            .filter(|item| palette_matches(&self.command_palette_query, &item.name, item.section))
            .collect()
    }
    fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_open {
            self.search_open = false;
        }
        self.theme_picker_open = false;
        self.icon_picker_open = false;
        self.command_palette_open = true;
        self.command_palette_query.clear();
        self.command_palette_selected = 0;
        self.search_query.clear();
        self.search_input.update(cx, |input, cx| {
            input.placeholder = Some("输入命令…".into());
            input.reset(String::new(), Vec::new(), 0, cx);
        });
        window.focus(&self.search_input.read(cx).focus);
        cx.notify();
    }
    fn close_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.command_palette_open = false;
        self.command_palette_query.clear();
        self.command_palette_selected = 0;
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
    fn invoke_palette_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = self
            .palette_items()
            .get(self.command_palette_selected)
            .map(|item| item.action)
        else {
            return;
        };
        self.close_command_palette(window, cx);
        match action {
            PaletteCommand::NewPage => self.add_page(window, cx),
            PaletteCommand::SearchPages => self.open_search(&SearchPages, window, cx),
            PaletteCommand::ToggleSidebar => self.toggle_sidebar(cx),
            PaletteCommand::ToggleTheme => {
                let next = if self.palette.id == "light" {
                    "midnight"
                } else {
                    "light"
                };
                self.select_theme(next, window, cx);
            }
            PaletteCommand::Settings => {
                self.settings_tab = 0;
                self.toggle_theme_picker(window, cx);
            }
            PaletteCommand::OpenNotes | PaletteCommand::OpenTasks => {
                self.commit_note_draft(window, cx);
                self.organizer_open = true;
                self.organizer_tasks = matches!(action, PaletteCommand::OpenTasks);
                self.selected_note = None;
                self.selected_task = None;
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
                cx.notify();
            }
            PaletteCommand::RenamePage => {
                if !self.session.page.locked {
                    window.focus(&self.title_input.read(cx).focus);
                }
            }
            PaletteCommand::DuplicatePage => self.duplicate_page(self.session.page.id, window, cx),
            PaletteCommand::DeletePage => {
                self.request_delete_page(self.session.page.id, cx);
            }
            PaletteCommand::ExportMarkdown => self.export_markdown(cx),
            PaletteCommand::ImportMarkdown => self.import_markdown(window, cx),
            PaletteCommand::CopyMarkdown => {
                let markdown =
                    quire_core::services::export_service::export_page(self.session.blocks());
                cx.write_to_clipboard(ClipboardItem::new_string(markdown));
                self.status = "页面 Markdown 已复制".into();
                self.error = false;
                cx.notify();
            }
            PaletteCommand::SaveCurrentAsTemplate => {
                let title = format!("{} 模板", self.session.page.title);
                if self.session.save_page_as_template(title).is_some() {
                    self.status = "当前页面已保存为模板".into();
                    self.error = false;
                    self.schedule_save(false, window, cx);
                } else {
                    self.status = "当前页面无法保存为模板（可能已锁定）".into();
                    self.error = true;
                    cx.notify();
                }
            }
            PaletteCommand::ImportTemplate => {
                if self.closing {
                    return;
                }
                let Some(path) = rfd::FileDialog::new()
                    .set_title("从 Markdown 导入模板")
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
                    .unwrap_or_else(|| "已导入模板".into());
                if self
                    .session
                    .import_markdown_as_template(title.clone(), &source)
                    .is_some()
                {
                    self.status = format!("已导入模板：{title}");
                    self.error = false;
                    self.schedule_save(false, window, cx);
                }
            }
            PaletteCommand::InsertTemplate(template_id) => {
                let anchor = self
                    .active
                    .or_else(|| self.session.blocks().first().map(|block| block.id));
                if let Some(first) = self.session.insert_template(template_id, anchor) {
                    self.sync_inputs(window, cx);
                    self.active = Some(first);
                    self.restore_focus(window, cx);
                    self.status = "模板已插入，可撤销".into();
                    self.error = false;
                    self.schedule_save(false, window, cx);
                } else {
                    self.status = "无法在当前位置插入该模板".into();
                    self.error = true;
                    cx.notify();
                }
            }
            PaletteCommand::NewPageFromTemplate(template_id) => {
                let previous = self.session.page.id;
                if let Some(id) = self.session.create_page_from_template(template_id, None) {
                    Self::push_page_history(&mut self.page_history_back, previous);
                    self.page_history_forward.clear();
                    self.session.visit_page(id);
                    self.find_index = None;
                    self.refresh_find_matches();
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
            PaletteCommand::DeleteTemplate(template_id) => {
                if self.session.delete_template(template_id) {
                    self.status = "模板已删除".into();
                    self.error = false;
                    self.schedule_save(false, window, cx);
                }
            }
            PaletteCommand::ExportTemplateMarkdown(template_id) => {
                if self.closing {
                    return;
                }
                let Some(template) = self
                    .session
                    .pages
                    .iter()
                    .find(|page| page.id == template_id && page.template)
                else {
                    return;
                };
                let title = template
                    .title
                    .chars()
                    .map(|ch| if r#"<>:\"/|?*"#.contains(ch) { '_' } else { ch })
                    .collect::<String>();
                let file_name = format!("{}.md", title.trim());
                let Some(path) = rfd::FileDialog::new()
                    .set_title("导出模板为 Markdown")
                    .set_file_name(file_name)
                    .add_filter("Markdown", &["md"])
                    .save_file()
                else {
                    return;
                };
                let markdown = quire_core::services::export_service::export_page(
                    self.session.document.page_blocks(template_id),
                );
                match std::fs::write(&path, markdown) {
                    Ok(()) => {
                        self.status = format!("已导出模板：{}", path.display());
                        self.error = false;
                    }
                    Err(error) => {
                        self.status = format!("导出模板失败：{error}");
                        self.error = true;
                    }
                }
                cx.notify();
            }
            PaletteCommand::NavigateBack => self.navigate_page_history(false, window, cx),
            PaletteCommand::NavigateForward => self.navigate_page_history(true, window, cx),
            PaletteCommand::OpenHtml => self.open_html_in_browser(cx),
            PaletteCommand::OpenPage(page_id) => self.activate_page(page_id, window, cx),
        }
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
    fn navigate_back_action(
        &mut self,
        _: &NavigateBack,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_page_history(false, window, cx);
    }
    fn navigate_forward_action(
        &mut self,
        _: &NavigateForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_page_history(true, window, cx);
    }
    fn new_page_action(&mut self, _: &NewPage, window: &mut Window, cx: &mut Context<Self>) {
        self.add_page(window, cx);
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
        let previous_page = self.session.page.id;
        let Some(duplicate) = self.session.duplicate_page(id) else {
            return;
        };
        Self::push_page_history(&mut self.page_history_back, previous_page);
        self.page_history_forward.clear();
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
        let previous = self.session.page.id;
        self.session.create_page(None);
        let id = self.session.page.id;
        Self::push_page_history(&mut self.page_history_back, previous);
        self.page_history_forward.clear();
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
        let previous = self.session.page.id;
        self.session.create_page(Some(parent));
        let id = self.session.page.id;
        Self::push_page_history(&mut self.page_history_back, previous);
        self.page_history_forward.clear();
        self.session.visit_page(id);
        self.find_index = None;
        self.refresh_find_matches();
        self.active = None;
        self.sync_inputs(window, cx);
        self.focus_first(window, cx);
        self.schedule_save(false, window, cx);
    }
    fn focus(&mut self, id: BlockId, caret: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.inputs.get(&id.0).cloned() {
            self.set_active_block(id, cx);
            input.update(cx, |input, cx| {
                input.buffer.move_to(caret);
                window.focus(&input.focus);
                cx.notify();
            });
        } else if let Some(index) = self
            .document_list_ids
            .iter()
            .position(|candidate| *candidate == id)
        {
            self.set_active_block(id, cx);
            self.document_list.scroll_to(ListOffset {
                item_ix: index + 1,
                offset_in_item: px(0.),
            });
            cx.notify();
            let entity = cx.entity();
            window.on_next_frame(move |window, app| {
                let _ = entity.update(app, |this, cx| {
                    if let Some(input) = this.inputs.get(&id.0).cloned() {
                        if let Some(index) = this
                            .document_list_ids
                            .iter()
                            .position(|candidate| *candidate == id)
                        {
                            this.document_list.splice_focusable(
                                index + 1..index + 2,
                                [Some(input.read(cx).focus.clone())],
                            );
                        }
                        input.update(cx, |input, cx| {
                            input.buffer.move_to(caret);
                            window.focus(&input.focus);
                            cx.notify();
                        });
                    }
                });
            });
        }
    }
    fn focus_selection(
        &mut self,
        id: BlockId,
        selection: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(input) = self.inputs.get(&id.0).cloned() {
            self.set_active_block(id, cx);
            input.update(cx, |input, cx| {
                input.buffer.selection = selection;
                input.buffer.reversed = false;
                window.focus(&input.focus);
                cx.notify();
            });
            return;
        }
        let Some(index) = self
            .document_list_ids
            .iter()
            .position(|candidate| *candidate == id)
        else {
            return;
        };
        self.set_active_block(id, cx);
        self.document_list.scroll_to(ListOffset {
            item_ix: index + 1,
            offset_in_item: px(0.),
        });
        cx.notify();
        cx.spawn_in(window, async move |weak, cx| {
            // A virtual row may need more than one frame to mount after a long
            // scroll. Keep the request alive until that editor exists instead
            // of silently dropping the find selection on the first frame.
            for _ in 0..100 {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let mounted = weak
                    .update_in(cx, |this, window, cx| {
                        if let Some(input) = this.inputs.get(&id.0).cloned() {
                            if let Some(index) = this
                                .document_list_ids
                                .iter()
                                .position(|candidate| *candidate == id)
                            {
                                this.document_list.splice_focusable(
                                    index + 1..index + 2,
                                    [Some(input.read(cx).focus.clone())],
                                );
                            }
                            input.update(cx, |input, cx| {
                                input.buffer.selection = selection.clone();
                                input.buffer.reversed = false;
                                window.focus(&input.focus);
                                cx.notify();
                            });
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if mounted {
                    break;
                }
            }
        })
        .detach();
    }
    fn set_active_block(&mut self, id: BlockId, cx: &mut Context<Self>) {
        let previous = self.active.replace(id);
        if previous == Some(id) {
            return;
        }
        if let Some(previous) = previous
            && let Some(index) = self
                .document_list_ids
                .iter()
                .position(|candidate| *candidate == previous)
        {
            self.document_list
                .splice_focusable(index + 1..index + 2, [None]);
        }
        if let Some(index) = self
            .document_list_ids
            .iter()
            .position(|candidate| *candidate == id)
            && let Some(input) = self.inputs.get(&id.0)
        {
            self.document_list
                .splice_focusable(index + 1..index + 2, [Some(input.read(cx).focus.clone())]);
        }
    }
    pub fn focus_first(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.document_list_ids.iter().copied().find(|id| {
            self.session
                .document
                .block(*id)
                .is_some_and(|block| self.session.editable(block.id))
        }) {
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
            self.visible_document_blocks()
                .into_iter()
                .filter_map(|(_, id)| {
                    let block = self.session.document.block(id)?;
                    self.session.editable(id).then_some(block)
                })
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
        self.focus_selection(id, range, window, cx);
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
    fn quick_note(&mut self, _: &QuickNote, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_quick_note(cx);
    }
    fn open_palette_action(
        &mut self,
        _: &OpenCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_command_palette(window, cx);
    }
    fn open_quick_note(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.quick_note_window {
            if handle
                .update(cx, |note, window, cx| {
                    window.activate_window();
                    window.focus(&note.input.read(cx).focus);
                })
                .is_ok()
            {
                return;
            }
            self.quick_note_window = None;
        }
        let owner = cx.entity();
        let theme = self.palette.clone();
        let draft = self
            .organizer_tag
            .as_ref()
            .map(|tag| format!("#{tag} "))
            .unwrap_or_default();
        let suggestions = self.session.organizer_tag_suggestions(&draft);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(560.), px(340.)), cx)),
            titlebar: None,
            window_min_size: Some(size(px(360.), px(220.))),
            kind: WindowKind::PopUp,
            ..Default::default()
        };
        let opened = cx.open_window(options, move |window, cx| {
            cx.new(|cx| QuickNoteWindow::new(owner, theme, draft, suggestions, window, cx))
        });
        match opened {
            Ok(handle) => self.quick_note_window = Some(handle),
            Err(error) => {
                self.status = format!("无法打开速记窗口：{error}");
                self.error = true;
                cx.notify();
            }
        }
    }
    fn handle_shell_event(
        &mut self,
        event: crate::platform::ShellEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            crate::platform::ShellEvent::ShowMain => {
                crate::platform::set_main_window_visible(window, true);
                window.activate_window();
            }
            crate::platform::ShellEvent::QuickNote => self.open_quick_note(cx),
            crate::platform::ShellEvent::Quit => cx.quit(),
            crate::platform::ShellEvent::McpCall(job) => {
                let crate::mcp_tools::McpJob::Call { name, args, reply } = job;
                let targets_active_page =
                    matches!(name.as_str(), "update_page" | "update_block_text")
                        && args.get("page_id").and_then(serde_json::Value::as_u64)
                            == Some(self.session.page.id.0);
                let execution = if targets_active_page && self.composing(cx) {
                    Err("当前页面正处于输入法组合状态，请完成输入后重试".to_owned())
                } else {
                    crate::mcp_tools::execute(&mut self.session, &name, &args)
                };
                if execution.is_ok() && targets_active_page {
                    let title = self.session.page.title.clone();
                    self.title_input.update(cx, |input, cx| {
                        input.reset(title.clone(), Vec::new(), title.len(), cx);
                    });
                    self.sync_inputs(window, cx);
                    self.focus_first(window, cx);
                }
                let result = execution.and_then(|value| {
                    self.session
                        .persistence
                        .force_flush()
                        .map_err(|error| error.to_string())?;
                    Ok(value)
                });
                // Do not copy note/task contents into the persistent call log.
                let message = match &result {
                    Ok(_) => "调用成功".to_owned(),
                    Err(error) => error.clone(),
                };
                self.push_mcp_log(&name, result.is_ok(), message);
                let _ = reply.send(result);
                cx.notify();
            }
        }
    }
    fn handle_sync_job(
        &mut self,
        job: quire_core::services::sync::engine::Job,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use quire_core::services::sync::engine::Job;
        match job {
            Job::ExportSnapshot { reply } => match self.session.sync_export() {
                Ok(snapshot) => {
                    let _ = reply.send(snapshot);
                }
                Err(error) => {
                    self.sync_status = format!("同步快照失败：{error}");
                }
            },
            Job::ListLocalAttachments { reply } => {
                let _ = reply.send(self.session.attachments.keys().copied().collect());
            }
            Job::AttachmentBytes { id, reply } => {
                let bytes = self
                    .session
                    .attachment_display_path(quire_core::core::types::AttachmentId(id))
                    .or_else(|| {
                        self.session
                            .attachment_path(quire_core::core::types::AttachmentId(id))
                    })
                    .and_then(|path| std::fs::read(path).ok());
                if let Some(bytes) = bytes {
                    let _ = reply.send(bytes);
                } else {
                    self.sync_status = format!("缺少附件 {id}，本轮同步中止");
                }
            }
            Job::ApplyRemote {
                peer,
                snapshot,
                bytes,
                reply,
            } => {
                let known = self
                    .session
                    .sync_peers()
                    .into_iter()
                    .find(|known| known.id == peer.id);
                let Some(mut peer) = known else {
                    let error = "拒绝从本机尚未发现的设备接收同步".to_owned();
                    self.session.sync_log_push("未知设备", false, &error);
                    self.sync_status = error.clone();
                    let _ = reply.send(Err(error));
                    cx.notify();
                    return;
                };
                if !snapshot.device.is_empty() {
                    peer.name = snapshot.device.clone();
                }
                let result = self
                    .session
                    .sync_apply_remote(&snapshot, &bytes, &peer)
                    .map_err(|error| error.to_string());
                match &result {
                    Ok(_) => {
                        self.session.sync_note_device(
                            &peer.id, &peer.name, &peer.kind, &peer.ip, peer.port,
                        );
                        self.sync_status = format!("已与 {} 同步", peer.name);
                        self.title_input.update(cx, |input, cx| {
                            let title = self.session.page.title.clone();
                            input.reset(title.clone(), Vec::new(), title.len(), cx);
                        });
                        self.sync_inputs(window, cx);
                    }
                    Err(error) => {
                        self.sync_status = format!("与 {} 同步失败：{error}", peer.name);
                    }
                }
                let _ = reply.send(result);
            }
            Job::InboundPair { device, ip, reply } => {
                self.session.sync_note_device(
                    &device.id,
                    &device.name,
                    &device.kind,
                    &ip,
                    device.port,
                );
                self.session
                    .sync_log_push(&device.name, true, "设备已加入局域网同步");
                self.sync_status = format!("{} 已加入同步", device.name);
                let _ = reply.send(true);
            }
            Job::Discovered { device, ip } => {
                self.session.sync_note_device(
                    &device.id,
                    &device.name,
                    &device.kind,
                    &ip,
                    device.port,
                );
                self.sync_status = format!("发现设备：{}", device.name);
            }
            Job::SyncDone {
                peer_id,
                ok,
                message,
            } => {
                self.sync_pending.remove(&peer_id);
                self.session.sync_mark_done(&peer_id, ok, &message);
                self.sync_status = message;
            }
            Job::Paired { device, ip } => {
                self.session.sync_note_device(
                    &device.id,
                    &device.name,
                    &device.kind,
                    &ip,
                    device.port,
                );
                self.sync_status = format!("已连接 {}", device.name);
            }
            Job::AutoTick => {}
        }
        cx.notify();
    }
    fn sync_now_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = quire_core::services::sync::engine::now_unix();
        let peers = sync_targets(self.session.sync_peers(), now, &self.sync_pending);
        if peers.is_empty() {
            self.sync_status = if self.sync_pending.is_empty() {
                "暂未发现在线设备".into()
            } else {
                "当前同步仍在进行中".into()
            };
        } else {
            self.sync_status = format!("正在同步 {} 台设备…", peers.len());
            for peer in peers {
                self.sync_pending.insert(peer.id.clone());
                if self
                    .sync_cmd_tx
                    .send(quire_core::services::sync::engine::Cmd::SyncWith(
                        peer.clone(),
                    ))
                    .is_err()
                {
                    self.sync_pending.remove(&peer.id);
                    self.sync_status = "同步引擎已停止，请重启 Quire 后重试".into();
                }
            }
        }
        self.schedule_save(false, window, cx);
        cx.notify();
    }
    fn apply_sync_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.sync_name_draft.trim().to_owned();
        if name.is_empty() {
            self.sync_status = "设备名称不能为空".into();
            cx.notify();
            return;
        }
        if self.session.setting("sync.device-name") == Some(name.as_str()) {
            self.sync_status = "设备名称未更改".into();
            cx.notify();
            return;
        }
        if !self.session.sync_set_device_name(&name) {
            self.sync_status = "设备名称未更改".into();
            cx.notify();
            return;
        }
        self.restart_on_quit
            .store(true, std::sync::atomic::Ordering::Release);
        self.sync_status = format!("设备名称已设为「{name}」；保存后将重启以更新局域网广播");
        self.request_close(window, cx);
    }
    fn toggle_global_hotkey(
        &mut self,
        id: u8,
        setting: &'static str,
        default: bool,
        label: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let enabled = self
            .session
            .setting(setting)
            .map(|value| value == "true")
            .unwrap_or(default);
        let next = !enabled;
        match (self.set_hotkey)(id, next) {
            Ok(()) => {
                self.session.set_setting(setting, next.to_string());
                self.hotkey_notice = format!("{label}已{}", if next { "开启" } else { "关闭" });
                self.schedule_save(false, window, cx);
            }
            Err(error) => {
                self.hotkey_notice = format!("{label}设置失败：{error}");
                cx.notify();
            }
        }
    }
    fn toggle_quick_note_on_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = self
            .session
            .setting("gpui.quick-note-on-start")
            .unwrap_or("false")
            == "true";
        let next = !enabled;
        self.session
            .set_setting("gpui.quick-note-on-start", next.to_string());
        self.hotkey_notice = format!(
            "启动时自动弹出速记{}",
            if next { "已开启" } else { "已关闭" }
        );
        self.schedule_save(false, window, cx);
    }
    fn sync_auto_tick(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let (automatic, interval) = self.session.sync_config();
        if !automatic || self.sync_last_auto.elapsed().as_secs() < interval {
            return;
        }
        self.sync_last_auto = std::time::Instant::now();
        let now = quire_core::services::sync::engine::now_unix();
        let peers = sync_targets(self.session.sync_peers(), now, &self.sync_pending);
        let mut queued = 0usize;
        let mut changed = false;
        for peer in peers {
            self.sync_pending.insert(peer.id.clone());
            if self
                .sync_cmd_tx
                .send(quire_core::services::sync::engine::Cmd::SyncWith(
                    peer.clone(),
                ))
                .is_ok()
            {
                queued += 1;
                changed = true;
            } else {
                self.sync_pending.remove(&peer.id);
                self.sync_status = "同步引擎已停止，请重启 Quire 后重试".into();
                changed = true;
            }
        }
        if queued > 0 {
            self.sync_status = format!("正在自动同步 {queued} 台设备…");
        }
        if changed {
            cx.notify();
        }
    }
    fn start_mcp(&mut self) {
        let call = crate::mcp_tools::call_sender(self.mcp_event_tx.clone());
        match crate::mcp::McpServer::new(
            self.mcp_port,
            self.mcp_token.clone(),
            crate::mcp_tools::tool_defs(),
            call,
        )
        .spawn()
        {
            Ok(server) => {
                if let Some(dir) = self.session.path.parent() {
                    if let Err(error) =
                        crate::mcp::write_pointer(dir, self.mcp_port, &self.mcp_token)
                    {
                        self.mcp_status = format!("已启动，但写入连接配置失败：{error}");
                    } else {
                        self.mcp_status =
                            format!("已启用 · http://127.0.0.1:{}/mcp", self.mcp_port);
                    }
                }
                self.mcp_server = Some(server);
                self.session.set_setting("gpui.mcp.enabled", "true");
                self.session
                    .set_setting("gpui.mcp.port", self.mcp_port.to_string());
                self.session
                    .set_setting("gpui.mcp.token", self.mcp_token.clone());
            }
            Err(error) => {
                self.mcp_status = format!("启动失败：{error}");
                self.session.set_setting("gpui.mcp.enabled", "false");
            }
        }
    }
    fn stop_mcp(&mut self) {
        self.mcp_server.take();
        if let Some(dir) = self.session.path.parent() {
            let _ = std::fs::remove_file(crate::mcp::pointer_path(dir));
        }
        self.session.set_setting("gpui.mcp.enabled", "false");
        self.mcp_status = "尚未启用".into();
    }
    fn toggle_mcp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mcp_server.is_some() {
            self.stop_mcp();
        } else {
            self.start_mcp();
        }
        self.schedule_save(true, window, cx);
        cx.notify();
    }
    fn restart_mcp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_mcp();
        self.start_mcp();
        self.schedule_save(true, window, cx);
        cx.notify();
    }
    fn apply_mcp_port(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let port = match parse_mcp_port(&self.mcp_port_draft) {
            Some(port) => port,
            None => {
                self.mcp_status = "端口必须是 1–65535 之间的整数".into();
                cx.notify();
                return;
            }
        };
        let was_enabled = self.mcp_server.is_some();
        if port == self.mcp_port {
            self.mcp_port_draft = port.to_string();
            self.mcp_status = if was_enabled {
                "端口未更改；MCP 服务继续运行".into()
            } else {
                format!("端口仍为 {port}；服务尚未启用")
            };
            cx.notify();
            return;
        }
        if was_enabled {
            let call = crate::mcp_tools::call_sender(self.mcp_event_tx.clone());
            let candidate = crate::mcp::McpServer::new(
                port,
                self.mcp_token.clone(),
                crate::mcp_tools::tool_defs(),
                call,
            )
            .spawn();
            match candidate {
                Ok(server) => {
                    let old_server = self.mcp_server.replace(server);
                    self.mcp_port = port;
                    self.mcp_port_draft = port.to_string();
                    self.session.set_setting("gpui.mcp.enabled", "true");
                    self.session.set_setting("gpui.mcp.port", port.to_string());
                    self.session
                        .set_setting("gpui.mcp.token", self.mcp_token.clone());
                    self.mcp_status = if let Some(dir) = self.session.path.parent() {
                        match crate::mcp::write_pointer(dir, port, &self.mcp_token) {
                            Ok(()) => format!("已启用 · http://127.0.0.1:{port}/mcp"),
                            Err(error) => {
                                let _ = std::fs::remove_file(crate::mcp::pointer_path(dir));
                                format!("服务已切换，但写入连接配置失败：{error}")
                            }
                        }
                    } else {
                        format!("已启用 · http://127.0.0.1:{port}/mcp")
                    };
                    drop(old_server);
                }
                Err(error) => {
                    self.mcp_status = format!(
                        "新端口 {port} 启动失败；原服务仍运行在 {}：{error}",
                        self.mcp_port
                    );
                    cx.notify();
                    return;
                }
            }
        } else {
            self.mcp_port = port;
            self.mcp_port_draft = port.to_string();
            self.session.set_setting("gpui.mcp.port", port.to_string());
            self.mcp_status = format!("端口已设为 {port}；服务尚未启用");
        }
        self.mcp_port_input.update(cx, |input, cx| {
            input.reset(port.to_string(), Vec::new(), port.to_string().len(), cx);
        });
        self.schedule_save(true, window, cx);
        cx.notify();
    }
    fn push_mcp_log(&mut self, tool: &str, ok: bool, message: String) {
        let mut log: Vec<serde_json::Value> = self
            .session
            .setting("gpui.mcp.log")
            .and_then(|value| serde_json::from_str(value).ok())
            .unwrap_or_default();
        log.push(serde_json::json!({
            "at": crate::mcp::now_label(), "tool": tool, "ok": ok, "message": message
        }));
        if log.len() > 50 {
            log.drain(..log.len() - 50);
        }
        self.session.set_setting(
            "gpui.mcp.log",
            serde_json::to_string(&log).unwrap_or_default(),
        );
    }
    pub fn hide_to_tray(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.commit_note_draft(window, cx);
        self.session.end_typing();
        self.schedule_save(true, window, cx);
        crate::platform::set_main_window_visible(window, false);
    }
    fn save_quick_note(&mut self, text: String, cx: &mut Context<Self>) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        let tags = capture_tags(text);
        let id = self.session.create_note();
        self.session
            .update_note_editor(id, String::new(), text.to_owned(), tags);
        if let Err(error) = self.session.persistence.force_flush() {
            self.status = format!("速记保存失败：{error}");
            self.error = true;
            cx.notify();
            return false;
        } else {
            self.status = "速记已保存".into();
            self.error = false;
        }
        cx.notify();
        true
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
                        if this
                            .restart_on_quit
                            .swap(false, std::sync::atomic::Ordering::AcqRel)
                        {
                            let previous = this.sync_name_original.clone();
                            this.session
                                .set_setting("sync.device-name", previous.clone());
                            this.sync_name_draft = previous.clone();
                            this.sync_name_input.update(cx, |input, cx| {
                                input.reset(previous.clone(), Vec::new(), previous.len(), cx);
                            });
                        }
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
                let second_block = this.session.blocks()[1].clone();
                let second = this
                    .ensure_block_input(&second_block, window, cx)
                    .expect("split block must remain editable");
                second.update(cx, |input, cx| {
                    input.replace_text_in_range(None, "重启后仍然存在", window, cx);
                });
            });
        })
        .detach();
    }
}

fn smoke_mcp_create_note(port: u16, token: &str) -> Result<String, String> {
    let discover = smoke_mcp_post(
        port,
        token,
        r#"{"jsonrpc":"2.0","id":0,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{},"io.modelcontextprotocol/clientInfo":{"name":"quire-gpui-smoke","version":"0.1.0"}}}}"#,
    )?;
    if !discover.contains("\"2026-07-28\"") {
        return Err(format!(
            "MCP server discovery did not advertise 2026-07-28: {discover}"
        ));
    }
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "create_note",
            "arguments": {
                "title": "MCP 窗口 smoke",
                "body": "由 MCP 桌面窗口回归创建",
                "tags": "mcp-smoke,桌面回归"
            },
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": {},
                "io.modelcontextprotocol/clientInfo": {
                    "name": "quire-gpui-smoke",
                    "version": "0.1.0"
                }
            }
        }
    })
    .to_string();
    smoke_mcp_post(port, token, &body)
}

fn smoke_mcp_post(port: u16, token: &str, body: &str) -> Result<String, String> {
    use std::io::{Read, Write};

    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))
        .map_err(|error| format!("connect to MCP smoke server: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("set MCP smoke timeout: {error}"))?;
    let metadata_headers = crate::mcp::transport_http_metadata_headers(body)?;
    write!(
        stream,
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\n{metadata_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|error| format!("send MCP smoke request: {error}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("read MCP smoke response: {error}"))?;
    Ok(response)
}

fn capture_tags(text: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for token in text.split([' ', '\n', '\t', '，', ',', '、']) {
        let Some(tag) = token.strip_prefix('#') else {
            continue;
        };
        let tag = tag.trim_end_matches([
            '.', '。', '!', '！', '?', '？', ')', '）', ':', '：', ';', '；',
        ]);
        if !tag.is_empty()
            && !tag.chars().all(|ch| ch.is_ascii_digit())
            && !tags.iter().any(|known| known == tag)
        {
            tags.push(tag.to_owned());
        }
    }
    tags
}

fn capture_tag_replacement(draft: &str, caret: usize, tag: &str) -> Option<(String, usize)> {
    let is_boundary = |ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                ',' | '，'
                    | '、'
                    | '.'
                    | '。'
                    | '!'
                    | '！'
                    | '?'
                    | '？'
                    | ')'
                    | '）'
                    | ']'
                    | '】'
                    | '}'
                    | '：'
                    | ':'
                    | ';'
                    | '；'
                    | '»'
                    | '」'
                    | '』'
            )
    };
    let caret = caret.min(draft.len());
    if !draft.is_char_boundary(caret) {
        return None;
    }
    let hash = draft[..caret].rfind('#')?;
    let typed = &draft[hash + 1..caret];
    if typed.chars().any(is_boundary) {
        return None;
    }
    let tail = &draft[caret..];
    let token_end = caret + tail.find(is_boundary).unwrap_or(tail.len());
    let mut next = String::with_capacity(draft.len() + tag.len());
    next.push_str(&draft[..hash]);
    next.push('#');
    next.push_str(tag);
    let mut suffix_start = token_end;
    match draft[token_end..].chars().next() {
        Some(separator) if separator.is_whitespace() => {
            next.push(separator);
            suffix_start += separator.len_utf8();
        }
        Some(_) => {}
        None => next.push(' '),
    }
    let new_caret = next.len();
    next.push_str(&draft[suffix_start..]);
    Some((next, new_caret))
}

impl Workspace {
    fn render_document_header(
        &mut self,
        cx: &mut Context<Self>,
        text: Hsla,
        muted: Hsla,
        p: &Palette,
    ) -> AnyElement {
        let page_id = self.session.page.id;
        let backlinks_count = self.session.repo.reference_count(page_id).unwrap_or(0);
        let backlink_rows = if self.backlinks_open {
            self.session
                .repo
                .references(page_id, 500)
                .unwrap_or_default()
                .into_iter()
                .map(|reference| {
                    let source_page = reference.page;
                    let block_id = reference.block;
                    let page_title = self
                        .session
                        .pages
                        .iter()
                        .find(|page| page.id == source_page)
                        .map(|page| page.title.clone())
                        .unwrap_or_else(|| "已删除页面".into());
                    let preview = if reference.text.trim().is_empty() {
                        "页面引用".to_owned()
                    } else {
                        reference.text
                    };
                    div()
                        .id(SharedString::from(format!(
                            "backlink-{source_page:?}-{block_id:?}"
                        )))
                        .w_full()
                        .px(px(9.))
                        .py(px(6.))
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.28)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.activate_page(source_page, window, cx);
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
        let cover_path = self
            .session
            .page
            .cover
            .and_then(|cover| self.session.attachment_display_path(cover));
        div()
            .w_full()
            .max_w(px(if self.session.page.full_width {
                1120.
            } else {
                760.
            }))
            .mx_auto()
            .pt(px(72.))
            .pb(px(16.))
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
            .into_any_element()
    }

    fn render_document_block(
        &mut self,
        index: usize,
        block: &Block,
        window: &mut Window,
        cx: &mut Context<Self>,
        text: Hsla,
        muted: Hsla,
        accent: Hsla,
        elevated: Hsla,
        p: &Palette,
        database_width: Pixels,
        sync_sources: &[(quire_core::core::types::PageId, String, BlockId, String)],
        page_candidates: &[(quire_core::core::types::PageId, String)],
    ) -> AnyElement {
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
                        previous.kind == BlockKind::Numbered && previous.parent == block.parent
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
        let input = self.ensure_block_input(block, window, cx);
        let block_id = block.id;
        let is_todo = block.kind == BlockKind::Todo;
        let is_toggle = block.kind == BlockKind::Toggle;
        let kind = block.kind;
        let database_table = if kind == BlockKind::Database {
            block.db_ref.and_then(|db_id| {
                self.session.databases.database(db_id)?;
                let title = self.session.databases.title_property(db_id)?.id;
                let all_columns = self
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
                let selected_view_key = format!("gpui.database-view.{}", db_id.0);
                let remembered_view = self
                    .session
                    .setting(&selected_view_key)
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(ViewId);
                let selected_view = self
                    .selected_database_views
                    .get(&db_id.0)
                    .copied()
                    .filter(|view_id| views.iter().any(|view| view.id == *view_id))
                    .or_else(|| {
                        remembered_view
                            .filter(|view_id| views.iter().any(|view| view.id == *view_id))
                    })
                    .or_else(|| views.first().map(|view| view.id));
                let active_view = views.iter().find(|view| Some(view.id) == selected_view)?;
                let columns = quire_core::core::database_view::view_columns(
                    &self.session.databases,
                    db_id,
                    active_view,
                )
                .into_iter()
                .filter(|property| !property.kind.is_title())
                .collect::<Vec<_>>();
                let definition =
                    quire_core::core::database_view::ViewDefinition::parse(&active_view.definition);
                let grid_width = f32::from(database_width) / quire_gpui::ui_scale::get();
                let column_ids = std::iter::once(title)
                    .chain(columns.iter().map(|property| property.id))
                    .collect::<Vec<_>>();
                let fixed_width = column_ids
                    .iter()
                    .map(|property| definition.width(*property))
                    .filter(|width| *width != 0)
                    .map(|width| grid_width * f32::from(width) / 1000.0)
                    .sum::<f32>();
                let auto_count = column_ids
                    .iter()
                    .filter(|property| definition.width(**property) == 0)
                    .count();
                let auto_width = if auto_count == 0 {
                    0.0
                } else {
                    ((grid_width - fixed_width).max(0.0) / auto_count as f32).max(grid_width * 0.06)
                };
                let mut column_widths = column_ids
                    .iter()
                    .map(|property| {
                        let width = definition.width(*property);
                        if width == 0 {
                            auto_width
                        } else {
                            grid_width * f32::from(width) / 1000.0
                        }
                    })
                    .collect::<Vec<_>>();
                if let Some(drag) = self
                    .database_column_drag
                    .filter(|drag| drag.db == db_id && drag.view == active_view.id)
                {
                    if let Some(index) = column_ids
                        .iter()
                        .position(|property| *property == drag.property)
                    {
                        column_widths[index] = drag.preview_width;
                    }
                }
                let rules = definition.rules(db_id, &self.session.databases);
                let row_search = (self.database_search_open == Some(db_id.0)
                    && !self.database_search_text.trim().is_empty())
                .then(|| self.database_search_text.clone());
                let repo = self.session.repo.clone();
                let total = repo
                    .filtered_count(&quire_core::core::database::RowRequest {
                        db: db_id,
                        title,
                        columns: &columns,
                        sorts: &rules.sorts,
                        filter: rules.filter.as_ref(),
                        search: row_search.as_deref(),
                    })
                    .ok()?;
                let group_data = rules.group.and_then(|spec| {
                    let config = self
                        .session
                        .databases
                        .properties_of(db_id)
                        .find(|property| property.id == spec.property)?
                        .config
                        .clone();
                    let request = quire_core::core::database::RowRequest {
                        db: db_id,
                        title,
                        columns: &columns,
                        sorts: &rules.sorts,
                        filter: rules.filter.as_ref(),
                        search: row_search.as_deref(),
                    };
                    let groups = repo.group_counts(&request, &spec).ok()?;
                    Some((spec, order_database_groups(groups, &config), config))
                });
                let list_total = group_data.as_ref().map_or(total, |(_, groups, _)| {
                    let view_id = selected_view.expect("database table has selected view");
                    groups
                        .iter()
                        .map(|(key, count)| {
                            1 + if self.database_collapsed_groups.contains(&(
                                block_id.0,
                                view_id.0,
                                database_group_identity(key),
                            )) {
                                0
                            } else {
                                *count
                            }
                        })
                        .sum()
                });
                Some((
                    db_id,
                    title,
                    columns,
                    all_columns,
                    column_widths,
                    row_search,
                    views,
                    selected_view,
                    rules.sorts,
                    rules.filter,
                    group_data,
                    total,
                    list_total,
                    repo,
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
        let page_reference_candidates =
            if matches!(kind, BlockKind::Page | BlockKind::Link) && block.page_ref.is_none() {
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
                                if this.session.set_page_reference(block_id, Some(target_page)) {
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
                .filter(|cell| cell.parent == Some(block_id) && cell.kind == BlockKind::TableCell)
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
                                .when_some(input.filter(|_| !self.closing), |view, input| {
                                    view.child(input)
                                })
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
                            this.delete_table_column(block_id, table_columns - 1, window, cx)
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
                        .children(ColorKind::ALL.into_iter().enumerate().map(|(slot, _)| {
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
                                    this.set_block_color(block_id, background, slot, window, cx)
                                }))
                        }))
                });
        let block_context_popup =
            self.block_menu_open
                .filter(|(id, _)| *id == block_id)
                .map(|(_, mode)| {
                    let rows = if mode == BlockMenuMode::Convert {
                        slash_items()
                            .iter()
                            .filter(|(_, _, target)| *target != kind)
                            .map(|(label, _, target)| {
                                let target = *target;
                                div()
                                    .id(SharedString::from(format!(
                                        "block-menu-kind-{block_id:?}-{target:?}"
                                    )))
                                    .w_full()
                                    .h(px(28.))
                                    .px(px(9.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .text_size(px(12.))
                                    .text_color(text)
                                    .hover(|item| item.bg(Palette::color(&p.bg).opacity(0.3)))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.block_menu_open = None;
                                        this.select_slash_kind(block_id, target, window, cx);
                                    }))
                                    .child(*label)
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>()
                    } else if mode == BlockMenuMode::MoveToPage {
                        self.session
                            .pages
                            .iter()
                            .filter(|page| {
                                !page.template && page.id != self.session.page.id && !page.locked
                            })
                            .map(|page| {
                                let target = page.id;
                                div()
                                    .id(SharedString::from(format!(
                                        "block-menu-move-to-{}-{block_id:?}",
                                        target.0
                                    )))
                                    .w_full()
                                    .h(px(28.))
                                    .px(px(9.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .text_size(px(12.))
                                    .text_color(text)
                                    .hover(|item| item.bg(Palette::color(&p.bg).opacity(0.3)))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.block_menu_open = None;
                                        this.move_block_to_page(block_id, target, window, cx);
                                    }))
                                    .child(page.title.clone())
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>()
                    } else {
                        [
                            ("转换为…", 0),
                            ("文字颜色", 1),
                            ("背景颜色", 2),
                            ("复制块", 3),
                            ("删除", 4),
                            ("移动到页面…", 5),
                        ]
                        .into_iter()
                        .map(|(label, action)| {
                            div()
                                .id(SharedString::from(format!(
                                    "block-menu-action-{block_id:?}-{action}"
                                )))
                                .w_full()
                                .h(px(28.))
                                .px(px(9.))
                                .flex()
                                .items_center()
                                .rounded(px(4.))
                                .cursor_pointer()
                                .text_size(px(12.))
                                .text_color(if action == 4 {
                                    Hsla::from(rgb(0xe5534b))
                                } else {
                                    text
                                })
                                .hover(|item| item.bg(Palette::color(&p.bg).opacity(0.3)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    match action {
                                        0 => {
                                            this.block_menu_open =
                                                Some((block_id, BlockMenuMode::Convert))
                                        }
                                        1 => {
                                            this.block_menu_open = None;
                                            this.toggle_color_picker(block_id, false, cx);
                                        }
                                        2 => {
                                            this.block_menu_open = None;
                                            this.toggle_color_picker(block_id, true, cx);
                                        }
                                        3 => {
                                            this.block_menu_open = None;
                                            this.duplicate_block(block_id, window, cx);
                                        }
                                        4 => {
                                            this.block_menu_open = None;
                                            this.delete_block(block_id, window, cx);
                                        }
                                        5 => {
                                            this.block_menu_open =
                                                Some((block_id, BlockMenuMode::MoveToPage))
                                        }
                                        _ => {}
                                    }
                                    cx.notify();
                                }))
                                .child(label.to_owned())
                                .into_any_element()
                        })
                        .collect::<Vec<_>>()
                    };
                    let mut rows = rows;
                    if mode == BlockMenuMode::MoveToPage && rows.is_empty() {
                        rows.push(
                            div()
                                .w_full()
                                .h(px(28.))
                                .px(px(9.))
                                .flex()
                                .items_center()
                                .text_size(px(11.))
                                .text_color(muted)
                                .child("没有可移动到的页面")
                                .into_any_element(),
                        );
                    }
                    if mode != BlockMenuMode::Actions {
                        rows.insert(
                            0,
                            div()
                                .id(SharedString::from(format!("block-menu-back-{block_id:?}")))
                                .w_full()
                                .h(px(28.))
                                .px(px(9.))
                                .flex()
                                .items_center()
                                .rounded(px(4.))
                                .cursor_pointer()
                                .text_size(px(12.))
                                .text_color(muted)
                                .hover(|item| item.bg(Palette::color(&p.bg).opacity(0.3)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.block_menu_open = Some((block_id, BlockMenuMode::Actions));
                                    cx.notify();
                                }))
                                .child("‹  返回")
                                .into_any_element(),
                        );
                    }
                    div()
                        .id(SharedString::from(format!("block-menu-{block_id:?}")))
                        .absolute()
                        .top(px(32.))
                        .right(px(0.))
                        .w(px(184.))
                        .max_h(px(320.))
                        .overflow_y_scroll()
                        .p(px(4.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .rounded(px(7.))
                        .border_1()
                        .border_color(Palette::color(&p.border))
                        .bg(Palette::color(&p.elevated))
                        .children(rows)
                });
        div()
                    .id(("block", block.id.0))
                    .w_full()
                    .relative()
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
                                if let Some((db_id, title, columns, all_columns, column_widths, row_search, views, selected_view, view_sorts, view_filter, group_data, total, list_total, repo)) =
                                    database_table
                                {
                                    let db_border = Palette::color(&p.border);
                                    let db_hover = Palette::color(&p.bg).opacity(0.22);
                                    let db_checkbox_fg = Palette::color(&p.bg);
                                    let query_columns = columns.clone();
                                    let row_column_widths = column_widths.clone();
                                    let row_search = row_search.clone();
                                    let cell_title_property = title;
                                    let cell_block = block_id.0;
                                    let mut header_columns = vec![(title, "名称".to_owned(), true)];
                                    header_columns.extend(
                                        columns
                                            .iter()
                                            .map(|property| (property.id, property.name.clone(), false)),
                                    );
                                    let header = div()
                                        .w(database_width)
                                        .flex()
                                        .h(px(34.))
                                        .border_b_1()
                                        .border_color(Palette::color(&p.border))
                                        .children(header_columns.into_iter().enumerate().map(
                                            |(index, (property, name, _is_title))| {
                                                let width = column_widths[index];
                                                let is_title = property == title;
                                                let editing_name = self.active_database_property_edit
                                                    == Some((block_id.0, db_id.0, property.0));
                                                let indicator = view_sorts
                                                    .iter()
                                                    .position(|sort| sort.property == property)
                                                    .map(|index| {
                                                        if view_sorts[index].descending {
                                                            format!(" ↓{}", index + 1)
                                                        } else {
                                                            format!(" ↑{}", index + 1)
                                                        }
                                                    })
                                                    .unwrap_or_default();
                                                let mut cell = div()
                                                    .id(SharedString::from(format!(
                                                        "db-header-{}-{}",
                                                        db_id.0, property.0
                                                    )))
                                                    .w(px(width))
                                                    .flex_shrink_0()
                                                    .min_w(px(width))
                                                    .h_full()
                                                    .px(px(10.))
                                                    .flex()
                                                    .items_center()
                                                    .text_color(muted)
                                                    .text_size(px(11.))
                                                    .child(
                                                        if editing_name {
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .child(self.database_property_name_input.clone())
                                                                .into_any_element()
                                                        } else {
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .child(name.clone())
                                                                .into_any_element()
                                                        },
                                                    )
                                                    .child(indicator);
                                                if let Some(view_id) = selected_view
                                                    .filter(|_| !self.session.page.locked)
                                                {
                                                    cell = cell
                                                        .cursor_pointer()
                                                        .hover(|cell| {
                                                            cell.bg(db_hover).text_color(text)
                                                        })
                                                        .on_click(cx.listener(
                                                            move |this, event: &ClickEvent, window, cx| {
                                                                if event.click_count() >= 2 && !is_title {
                                                                    this.database_header_click_seq = this.database_header_click_seq.wrapping_add(1);
                                                                    this.begin_database_property_name_edit(
                                                                        block_id.0,
                                                                        db_id.0,
                                                                        property.0,
                                                                        name.clone(),
                                                                        window,
                                                                        cx,
                                                                    );
                                                                } else if event.click_count() == 1 {
                                                                    this.schedule_database_header_sort(
                                                                        block_id,
                                                                        db_id,
                                                                        view_id,
                                                                        property,
                                                                        window,
                                                                        cx,
                                                                    );
                                                                }
                                                            },
                                                        ));
                                                }
                                                if let Some(view_id) = selected_view
                                                    .filter(|_| !self.session.page.locked)
                                                {
                                                    let drag_block = block_id;
                                                    let drag_db = db_id;
                                                    let drag_property = property;
                                                    let drag_grid = f32::from(database_width) / quire_gpui::ui_scale::get();
                                                    cell = cell.child(
                                                        div()
                                                            .id(SharedString::from(format!(
                                                                "db-resize-{}-{}",
                                                                db_id.0, property.0
                                                            )))
                                                            .w(px(6.))
                                                            .h_full()
                                                            .flex_shrink_0()
                                                            .cursor(CursorStyle::ResizeLeftRight)
                                                            .hover(|handle| {
                                                                handle.bg(accent.opacity(0.55))
                                                            })
                                                            .on_mouse_down(
                                                                MouseButton::Left,
                                                                cx.listener(
                                                                    move |this,
                                                                          event: &MouseDownEvent,
                                                                          window,
                                                                          cx| {
                                                                        this.begin_database_column_drag(
                                                                            drag_block,
                                                                            drag_db,
                                                                            view_id,
                                                                            drag_property,
                                                                            width,
                                                                            drag_grid,
                                                                            event,
                                                                            window,
                                                                            cx,
                                                                        );
                                                                    },
                                                                ),
                                                            )
                                                            .on_click(cx.listener(
                                                                |_, _, _, cx| cx.stop_propagation(),
                                                            )),
                                                    );
                                                }
                                                cell
                                            },
                                        ));
                                    let active_view_id = selected_view
                                        .expect("database table has selected view");
                                    let active_layout = views
                                        .iter()
                                        .find(|view| view.id == active_view_id)
                                        .map_or(
                                            quire_core::core::database::ViewLayout::Table,
                                            |view| view.layout,
                                        );
                                    let list_layout =
                                        active_layout == quire_core::core::database::ViewLayout::List;
                                    let row_sorts = view_sorts.clone();
                                    let row_filter = view_filter.clone();
                                    let row_group_data = group_data.clone();
                                    let collapsed_groups = self.database_collapsed_groups.clone();
                                    let formula_properties = all_columns.clone();
                                    let all_database_properties = self.session.databases.properties.clone();
                                    let database_attachments = self.session.attachments.clone();
                                    let rows = uniform_list(
                                        SharedString::from(format!("database-rows-{}", db_id.0)),
                                        list_total,
                                        cx.processor(
                                        move |_this, range: Range<usize>, _window, _cx| {
                                                let request = quire_core::core::database::RowRequest {
                                                    db: db_id,
                                                    title,
                                                    columns: &query_columns,
                                                    sorts: &row_sorts,
                                                    filter: row_filter.as_ref(),
                                                    search: row_search.as_deref(),
                                                };
                                                let mut entries: Vec<(usize, AnyElement)> = Vec::new();
                                                let render_row = |row: quire_core::core::database::RowView| {
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
                                                                && !kind.is_derived();
                                                            let cell_key =
                                                                (cell_block, db_id.0, row.record, property_id.0);
                                                            let editing = _this.active_database_cell == Some(cell_key);
                                                            let choice_open = _this.active_database_choice == Some(cell_key);
                                                            let relation_open = _this.active_database_relation == Some(cell_key);
                                                            let input = _this.database_cell_input.clone();
                                                            let display_value = if kind == quire_core::core::database::PropertyKind::Formula {
                                                                evaluate_database_formula(
                                                                    property_id,
                                                                    &formula_properties,
                                                                    &repo,
                                                                    quire_core::core::database::RecordId(row.record),
                                                                    0,
                                                                )
                                                                .map(|value| {
                                                                    quire_core::core::database_formula::display(&value)
                                                                })
                                                                .unwrap_or_else(|_| {
                                                                    quire_core::core::database_formula::FORMULA_ERROR_PAINT.into()
                                                                })
                                                            } else if kind == quire_core::core::database::PropertyKind::Rollup {
                                                                evaluate_database_rollup(property_id, &all_database_properties, &repo, quire_core::core::database::RecordId(row.record))
                                                                    .map(|value| quire_core::core::database_formula::display(&value))
                                                                    .unwrap_or_else(|_| quire_core::core::database_formula::FORMULA_ERROR_PAINT.into())
                                                            } else if kind == quire_core::core::database::PropertyKind::Files {
                                                                match repo.cell(quire_core::core::database::RecordId(row.record), property_id) {
                                                                    Ok(quire_core::core::database::CellValue::Items(ids)) => ids.iter().map(|id| id.parse::<u64>().ok().and_then(|id| database_attachments.get(&id).map(|attachment| attachment.name.clone())).unwrap_or_else(|| "附件不可用".into())).collect::<Vec<_>>().join(", "),
                                                                    _ => String::new(),
                                                                }
                                                            } else {
                                                                value.clone()
                                                            };
                                                            let display_value = if kind == quire_core::core::database::PropertyKind::Relation {
                                                                let relation = formula_properties.iter().find(|property| property.id == property_id).map(|property| quire_core::core::database_relation::config_relation(&property.config));
                                                                if let (Some(_relation), Ok(quire_core::core::database::CellValue::Items(ids))) = (relation, _this.session.repo.cell(quire_core::core::database::RecordId(row.record), property_id)) {
                                                                    let record_ids = ids.iter().filter_map(|id| id.parse::<u64>().ok()).map(quire_core::core::database::RecordId).collect::<Vec<_>>();
                                                                    let titles = _this.session.repo.record_titles(&record_ids).unwrap_or_default();
                                                                    quire_core::core::database_relation::paint_targets(&ids, |id| id.parse::<u64>().ok().and_then(|id| titles.get(&id).cloned()))
                                                                } else if relation.is_some_and(|relation| relation.target.is_none()) {
                                                                    "未配置目标数据库".into()
                                                                } else {
                                                                    display_value
                                                                }
                                                            } else { display_value };
                                                            let displayed = if display_value.is_empty() {
                                                                "—".to_owned()
                                                            } else {
                                                                display_value
                                                            };
                                                            let displayed = if list_layout && index > 0 {
                                                                format!(
                                                                    "{}: {displayed}",
                                                                    query_columns[index - 1].name
                                                                )
                                                            } else {
                                                                displayed
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
                                                            let width = if list_layout {
                                                                if index == 0 { 240. } else { 190. }
                                                            } else {
                                                                row_column_widths[index]
                                                            };
                                                            let mut cell = div()
                                                                    .id(SharedString::from(
                                                                        format!(
                                                                            "db-cell-{}-{}-{index}",
                                                                            db_id.0, row.record
                                                                        ),
                                                                    ))
                                                                    .w(px(width))
                                                                    .flex_shrink_0()
                                                                    .min_w(px(width))
                                                                    .h(px(36.))
                                                                    .px(px(10.))
                                                                    .flex()
                                                                    .items_center()
                                                                    .when(
                                                                        !list_layout,
                                                                        |cell| cell.border_r_1(),
                                                                    )
                                                                    .border_color(db_border)
                                                                    .text_size(px(12.))
                                                                    .text_color(text)
                                                                    .relative()
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
                                                                                this.selected_database_record = Some((
                                                                                    cell_block,
                                                                                    db_id.0,
                                                                                    row.record,
                                                                                ));
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
                                                                                this.selected_database_record = Some((
                                                                                    cell_block,
                                                                                    db_id.0,
                                                                                    row.record,
                                                                                ));
                                                                            if kind == quire_core::core::database::PropertyKind::Files {
                                                                                this.active_database_relation = if this.active_database_relation == Some((cell_block, db_id.0, row.record, property_id.0)) { None } else { Some((cell_block, db_id.0, row.record, property_id.0)) };
                                                                                cx.notify();
                                                                            } else if matches!(kind, quire_core::core::database::PropertyKind::Select | quire_core::core::database::PropertyKind::Status) {
                                                                                this.active_database_choice = if this.active_database_choice == Some((cell_block, db_id.0, row.record, property_id.0)) { None } else { Some((cell_block, db_id.0, row.record, property_id.0)) };
                                                                                this.active_database_cell = None;
                                                                                this.selected_database_record = Some((cell_block, db_id.0, row.record));
                                                                                cx.notify();
                                                                            } else if kind == quire_core::core::database::PropertyKind::Relation {
                                                                                this.active_database_cell = None;
                                                                                this.toggle_database_relation_picker((cell_block, db_id.0, row.record, property_id.0), window, cx);
                                                                            } else {
                                                                            this.begin_database_cell_edit(
                                                                                cell_block,
                                                                                db_id.0,
                                                                                row.record,
                                                                                property_id.0,
                                                                                raw_value.clone(),
                                                                                window,
                                                                                cx,
                                                                            );
                                                                            }
                                                                        },
                                                                    ));
                                                                }
                                                            }
                                                            if choice_open {
                                                                if let Some(property) = _this.session.databases.properties.iter().find(|property| property.id == quire_core::core::database::PropertyId(property_id.0)) {
                                                                    let options = quire_core::core::database_property::PropertyOptions::from_config(&property.config);
                                                                    let selected = _this.session.repo.cell(quire_core::core::database::RecordId(row.record), property.id).ok().map(|value| value.display()).unwrap_or_default();
                                                                    let mut menu = div().id(SharedString::from(format!("db-choice-menu-{}-{}", row.record, property_id.0))).absolute().top(px(33.)).left(px(0.)).min_w(px(140.)).max_h(px(220.)).overflow_y_scroll().flex_col().gap(px(2.)).p(px(5.)).rounded(px(6.)).border_1().border_color(db_border).bg(elevated).shadow_lg();
                                                                    menu = menu.child(div().id(SharedString::from(format!("db-choice-clear-{}-{}", row.record, property_id.0))).px(px(7.)).py(px(5.)).rounded(px(4.)).text_size(px(11.)).text_color(muted).hover(|item| item.bg(db_hover)).cursor_pointer().on_click(_cx.listener(move |this, _, window, cx| this.set_database_choice(cell_block, row.record, property_id.0, None, window, cx))).child("清除"));
                                                                    menu = menu.children(options.iter().map(|option| {
                                                                        let option_id = option.id.0.to_string();
                                                                        let selected_here = selected == option.id.to_string();
                                                                        div().id(SharedString::from(format!("db-choice-{}-{}-{}", row.record, property_id.0, option.id.0))).px(px(7.)).py(px(5.)).rounded(px(4.)).text_size(px(11.)).text_color(text).when(selected_here, |item| item.bg(accent.opacity(0.28))).hover(|item| item.bg(db_hover)).cursor_pointer().on_click(_cx.listener(move |this, _, window, cx| this.set_database_choice(cell_block, row.record, property_id.0, Some(option_id.clone()), window, cx))).child(option.name.clone())
                                                                    }));
                                                                    cell = cell.child(menu);
                                                                }
                                                            }
                                                            if relation_open {
                                                                if let Some(property) = _this.session.databases.properties.iter().find(|property| property.id == quire_core::core::database::PropertyId(property_id.0)) {
                                                                    let relation = quire_core::core::database_relation::config_relation(&property.config);
                                                                    if let Some(target_db) = relation.target {
                                                                        let selected_ids = match _this.session.repo.cell(quire_core::core::database::RecordId(row.record), property.id) { Ok(quire_core::core::database::CellValue::Items(ids)) => ids, _ => Vec::new() };
                                                                        let query = _this.database_relation_search_text.trim().to_owned();
                                                                        let candidates = _this.session.repo.records_named(target_db, &query, 40).unwrap_or_default();
                                                                        let mut menu = div().id(SharedString::from(format!("db-relation-menu-{}-{}", row.record, property_id.0))).absolute().top(px(33.)).left(px(0.)).min_w(px(170.)).max_h(px(240.)).overflow_y_scroll().flex_col().gap(px(2.)).p(px(5.)).rounded(px(6.)).border_1().border_color(db_border).bg(elevated).shadow_lg();
                                                                        menu = menu.child(div().text_size(px(10.)).text_color(muted).px(px(6.)).py(px(3.)).child(format!("{} · 搜索结果最多 40 条", _this.session.databases.database(target_db).map(|db| db.name.as_str()).unwrap_or("目标数据库"))));
                                                                        menu = menu.child(div().w(px(218.)).h(px(28.)).px(px(5.)).rounded(px(4.)).border_1().border_color(db_border).child(_this.database_relation_search_input.clone()));
                                                                        menu = menu.child(div().id(SharedString::from(format!("db-relation-clear-{}-{property_id}", row.record))).px(px(7.)).py(px(5.)).rounded(px(4.)).text_size(px(11.)).text_color(muted).hover(|item| item.bg(db_hover)).cursor_pointer().on_click(_cx.listener(move |this, _, window, cx| this.clear_database_relation(cell_block, row.record, property_id.0, window, cx))).child("清除全部关联"));
                                                                        if candidates.is_empty() { menu = menu.child(div().px(px(7.)).py(px(5.)).text_size(px(11.)).text_color(muted).child(if query.is_empty() { "目标数据库没有记录" } else { "没有匹配的记录" })); }
                                                                        menu = menu.children(candidates.into_iter().map(|(target, title)| {
                                                                            let selected = selected_ids.iter().any(|id| id == &target.to_string());
                                                                            div().id(SharedString::from(format!("db-relation-choice-{}-{target}", property_id.0))).px(px(7.)).py(px(5.)).rounded(px(4.)).text_size(px(11.)).text_color(text).when(selected, |item| item.bg(accent.opacity(0.28))).hover(|item| item.bg(db_hover)).cursor_pointer().on_click(_cx.listener(move |this, _, window, cx| this.toggle_database_relation_target(cell_block, row.record, property_id.0, target, window, cx))).child(format!("{} {}", if selected { "☑" } else { "□" }, if title.is_empty() { "（无标题）".to_owned() } else { title }))
                                                                        }));
                                                                        cell = cell.child(menu);
                                                                    }
                                                                }
                                                            }
                                                            if relation_open && kind == quire_core::core::database::PropertyKind::Files {
                                                                let ids = match _this.session.repo.cell(quire_core::core::database::RecordId(row.record), quire_core::core::database::PropertyId(property_id.0)) { Ok(quire_core::core::database::CellValue::Items(ids)) => ids, _ => Vec::new() };
                                                                let mut menu = div().id(SharedString::from(format!("db-files-menu-{}-{}", row.record, property_id.0))).absolute().top(px(33.)).left(px(0.)).min_w(px(180.)).max_h(px(240.)).overflow_y_scroll().flex_col().gap(px(2.)).p(px(5.)).rounded(px(6.)).border_1().border_color(db_border).bg(elevated).shadow_lg();
                                                                menu = menu.child(div().id(SharedString::from(format!("db-files-add-{}-{}", row.record, property_id.0))).px(px(7.)).py(px(5.)).rounded(px(4.)).text_size(px(11.)).text_color(accent).hover(|item| item.bg(db_hover)).cursor_pointer().on_click(_cx.listener(move |this, _, window, cx| this.pick_database_file(cell_block, row.record, property_id.0, window, cx))).child("＋ 添加文件…"));
                                                                for file_id in ids.iter().filter_map(|id| id.parse::<u64>().ok()) {
                                                                    let name = database_attachments.get(&file_id).map(|attachment| attachment.name.clone()).unwrap_or_else(|| "附件不可用".into());
                                                                    menu = menu.child(div().id(SharedString::from(format!("db-files-item-{property_id}-{}", file_id))).flex().items_center().gap(px(5.)).px(px(5.)).py(px(4.)).text_size(px(11.)).text_color(text)
                                                                        .child(div().flex_1().min_w_0().child(name))
                                                                        .child(div().id(SharedString::from(format!("db-files-open-{property_id}-{file_id}"))).px(px(5.)).rounded(px(3.)).text_color(accent).hover(|item| item.bg(db_hover)).cursor_pointer().on_click(_cx.listener(move |this, _, _, cx| {
                                                                            if let Some(path) = this.session.attachment_path(quire_core::core::types::AttachmentId(file_id)).and_then(|path| url::Url::from_file_path(path).ok()) { cx.open_url(path.as_str()); }
                                                                        })).child("打开"))
                                                                        .child(div().id(SharedString::from(format!("db-files-save-{property_id}-{file_id}"))).px(px(5.)).rounded(px(3.)).text_color(muted).hover(|item| item.bg(db_hover)).cursor_pointer().on_click(_cx.listener(move |this, _, _, cx| this.save_attachment(quire_core::core::types::AttachmentId(file_id), cx))).child("另存"))
                                                                        .child(div().id(SharedString::from(format!("db-files-remove-{property_id}-{file_id}"))).px(px(5.)).rounded(px(3.)).text_color(muted).hover(|item| item.bg(db_hover)).cursor_pointer().on_click(_cx.listener(move |this, _, window, cx| this.remove_database_file(cell_block, row.record, property_id.0, file_id, window, cx))).child("移除")));
                                                                }
                                                                cell = cell.child(menu);
                                                            }
                                                            cell
                                                            })
                                                            .collect::<Vec<_>>();
                                                        let rendered_row = div()
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
                                                            .into_any_element();
                                                        rendered_row
                                                };
                                                if let Some((spec, groups, config)) = &row_group_data {
                                                    let counts = groups
                                                        .iter()
                                                        .map(|(key, count)| {
                                                            if collapsed_groups.contains(&(
                                                                block_id.0,
                                                                active_view_id.0,
                                                                database_group_identity(key),
                                                            )) {
                                                                0
                                                            } else {
                                                                *count
                                                            }
                                                        })
                                                        .collect::<Vec<_>>();
                                                    let window = quire_core::core::database::RowWindow {
                                                        start: range.start,
                                                        end: range.end,
                                                    };
                                                    let layout = quire_core::core::database_view::group_window(&counts, window);
                                                    for (group_index, entry) in layout.headers {
                                                        let (key, count) = &groups[group_index];
                                                        let label = database_group_label(key, config);
                                                        let identity = database_group_identity(key);
                                                        let collapsed = collapsed_groups.contains(&(
                                                            block_id.0,
                                                            active_view_id.0,
                                                            identity.clone(),
                                                        ));
                                                        let collapsed_key = (
                                                            block_id.0,
                                                            active_view_id.0,
                                                            identity,
                                                        );
                                                        let header = div()
                                                            .id(SharedString::from(format!("db-group-{}-{group_index}", db_id.0)))
                                                            .w_full()
                                                            .h(px(36.))
                                                            .px(px(10.))
                                                            .flex()
                                                            .items_center()
                                                            .justify_between()
                                                            .border_b_1()
                                                            .border_color(db_border)
                                                            .bg(db_hover)
                                                            .text_size(px(11.))
                                                            .text_color(text)
                                                            .cursor_pointer()
                                                            .hover(|header| header.bg(db_hover.opacity(0.75)))
                                                            .on_click(_cx.listener(move |this, _, _, cx| {
                                                                if !this.database_collapsed_groups.remove(&collapsed_key) {
                                                                    this.database_collapsed_groups.insert(collapsed_key.clone());
                                                                }
                                                                cx.notify();
                                                            }))
                                                            .child(format!("{}  {label}", if collapsed { "›" } else { "⌄" }))
                                                            .child(format!("{count} 行"))
                                                            .into_any_element();
                                                        entries.push((entry, header));
                                                    }
                                                    for slice in layout.rows {
                                                        let (key, _) = &groups[slice.group];
                                                        let rows = repo.window_rows_in_group(
                                                            &request,
                                                            spec,
                                                            key,
                                                            slice.skip,
                                                            slice.len,
                                                        ).unwrap_or_default();
                                                        for (offset, row) in rows.into_iter().enumerate() {
                                                            entries.push((slice.at + offset, render_row(row)));
                                                        }
                                                    }
                                                } else {
                                                    let slice = quire_core::core::database::RowWindow {
                                                        start: range.start,
                                                        end: range.end,
                                                    };
                                                    for (offset, row) in repo.window_rows(&request, slice).unwrap_or_default().into_iter().enumerate() {
                                                        entries.push((range.start + offset, render_row(row)));
                                                    }
                                                }
                                                entries.sort_by_key(|(entry, _)| *entry);
                                                entries.into_iter().map(|(_, row)| row).collect::<Vec<_>>()
                                            },
                                        ),
                                    )
                                    .h(px((list_total.min(8).max(1) as f32 * 36.).max(40.)))
                                    .w(database_width)
                                    .max_h(px(288.))
                                    .flex_1()
                                    .min_h(px(40.));
                                    let columns_open = selected_view.is_some_and(|view_id| {
                                        self.database_columns_open
                                            == Some((block_id.0, view_id.0))
                                    });
                                    let group_open = selected_view.is_some_and(|view_id| {
                                        self.database_group_open == Some((block_id.0, view_id.0))
                                    });
                                    let filter_open = selected_view.is_some_and(|view_id| {
                                        self.database_filter_open == Some((block_id.0, view_id.0))
                                    });
                                    let filter_columns = self
                                        .session
                                        .databases
                                        .properties_of(db_id)
                                        .cloned()
                                        .collect::<Vec<_>>();
                                    let grouped_property = group_data
                                        .as_ref()
                                        .map(|(spec, _, _)| spec.property);
                                    let group_picker = group_open.then(|| {
                                        let mut popup = div()
                                            .w_full()
                                            .px(px(8.))
                                            .py(px(6.))
                                            .flex()
                                            .flex_wrap()
                                            .gap(px(5.))
                                            .border_b_1()
                                            .border_color(db_border);
                                        let selected = grouped_property.is_none();
                                        popup = popup.child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "db-group-none-{}",
                                                    block_id.0
                                                )))
                                                .px(px(8.))
                                                .py(px(5.))
                                                .rounded(px(4.))
                                                .text_size(px(11.))
                                                .text_color(if selected { accent } else { muted })
                                                .when(!self.session.page.locked, |item| {
                                                    item.cursor_pointer()
                                                        .hover(|item| item.bg(db_hover))
                                                })
                                                .when(!self.session.page.locked, |item| {
                                                    item.on_click(cx.listener(move |this, _, window, cx| {
                                                        this.set_database_group(
                                                            block_id,
                                                            db_id,
                                                            active_view_id,
                                                            None,
                                                            window,
                                                            cx,
                                                        );
                                                    }))
                                                })
                                                .child("无分组"),
                                        );
                                        popup = popup.children(filter_columns.iter().filter(|property| {
                                            quire_core::core::database_view::GroupSpec::admits(property.kind)
                                        }).map(|property| {
                                            let property_id = property.id;
                                            let label = format!("{} · {}", property.name, database_kind_label(property.kind));
                                            let selected = grouped_property == Some(property_id);
                                            let mut item = div()
                                                .id(SharedString::from(format!(
                                                    "db-group-property-{}-{}",
                                                    db_id.0, property_id.0
                                                )))
                                                .px(px(8.))
                                                .py(px(5.))
                                                .rounded(px(4.))
                                                .text_size(px(11.))
                                                .text_color(if selected { accent } else { text })
                                                .child(label);
                                            if !self.session.page.locked {
                                                item = item.cursor_pointer()
                                                    .hover(|item| item.bg(db_hover))
                                                    .on_click(cx.listener(move |this, _, window, cx| {
                                                        this.set_database_group(
                                                            block_id,
                                                            db_id,
                                                            active_view_id,
                                                            Some(property_id),
                                                            window,
                                                            cx,
                                                        );
                                                    }));
                                            }
                                            item
                                        }));
                                        popup
                                    });
                                    let flat_filter = quire_core::core::database_view::FlatFilter::from_tree(view_filter.as_ref());
                                    let filter_picker = filter_open.then(|| {
                                        let mut popup = div()
                                            .id(SharedString::from(format!(
                                                "db-filter-panel-{}-{}",
                                                block_id.0, active_view_id.0
                                            )))
                                            .w_full()
                                            .max_h(px(360.))
                                            .overflow_y_scroll()
                                            .px(px(8.))
                                            .py(px(6.))
                                            .flex()
                                            .flex_col()
                                            .gap(px(5.))
                                            .border_b_1()
                                            .border_color(db_border);
                                        if let Some(flat) = flat_filter.clone() {
                                            match self.database_filter_panel {
                                                0 => {
                                                    let any = flat.any;
                                                    popup = popup.child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap(px(5.))
                                                            .text_size(px(11.))
                                                            .text_color(muted)
                                                            .child("匹配")
                                                            .children([(false, "全部"), (true, "任一")].into_iter().map(|(match_any, label)| {
                                                                div()
                                                                    .id(SharedString::from(format!("db-filter-match-{}-{}", block_id.0, match_any)))
                                                                    .px(px(8.))
                                                                    .py(px(4.))
                                                                    .rounded(px(4.))
                                                                    .text_color(if any == match_any { accent } else { muted })
                                                                    .when(!self.session.page.locked, |button| button.cursor_pointer().hover(|button| button.bg(db_hover)))
                                                                    .on_click(cx.listener(move |this, _, window, cx| this.set_database_filter_match(block_id, db_id, active_view_id, match_any, window, cx)))
                                                                    .child(label)
                                                            })),
                                                    );
                                                    for (index, flat_clause) in flat.clauses.iter().enumerate() {
                                                        let clause = &flat_clause.clause;
                                                        let row_property = self.session.databases.properties_of(db_id).find(|property| property.id == clause.property);
                                                        let name = row_property.map(|property| property.name.as_str()).unwrap_or("已删除的列");
                                                        let op_name = database_filter_op_label(clause.op, clause.kind);
                                                        let editing = self.database_filter_value_active == Some((block_id.0, db_id.0, active_view_id.0, index));
                                                        let value_row = if matches!(clause.op, quire_core::core::database_view::FilterOp::IsEmpty | quire_core::core::database_view::FilterOp::IsNotEmpty) {
                                                            div().text_size(px(11.)).text_color(muted).child("无需值").into_any_element()
                                                        } else if clause.kind == quire_core::core::database::PropertyKind::Checkbox {
                                                            let checked = matches!(&clause.value, quire_core::core::database_view::FilterValue::Flag(true));
                                                            let label = match &clause.value {
                                                                quire_core::core::database_view::FilterValue::Flag(true) => "☑ 已勾选",
                                                                quire_core::core::database_view::FilterValue::Flag(false) => "□ 未勾选",
                                                                _ => "选择…",
                                                            };
                                                            div()
                                                                .id(SharedString::from(format!("db-filter-flag-{}-{}", block_id.0, index)))
                                                                .px(px(8.))
                                                                .py(px(4.))
                                                                .rounded(px(4.))
                                                                .text_size(px(11.))
                                                                .text_color(text)
                                                                .cursor_pointer()
                                                                .hover(|item| item.bg(db_hover))
                                                                .on_click(cx.listener(move |this, _, window, cx| this.set_database_filter_flag(block_id, db_id, active_view_id, index, !checked, window, cx)))
                                                                .child(label)
                                                                .into_any_element()
                                                        } else if matches!(clause.kind, quire_core::core::database::PropertyKind::Select | quire_core::core::database::PropertyKind::Status | quire_core::core::database::PropertyKind::MultiSelect | quire_core::core::database::PropertyKind::Files) {
                                                            let label = row_property.map(|property| {
                                                                let options = quire_core::core::database_property::PropertyOptions::from_config(&property.config);
                                                                match &clause.value {
                                                                    quire_core::core::database_view::FilterValue::Text(id) => options.iter().find(|option| option.id.to_string() == *id).map(|option| option.name.clone()).unwrap_or_else(|| id.clone()),
                                                                    quire_core::core::database_view::FilterValue::Any(ids) => ids.iter().map(|id| options.iter().find(|option| option.id.to_string() == *id).map(|option| option.name.as_str()).unwrap_or(id)).collect::<Vec<_>>().join(", "),
                                                                    _ => String::new(),
                                                                }
                                                            }).unwrap_or_default();
                                                            div()
                                                                .id(SharedString::from(format!("db-filter-option-value-{}-{}", block_id.0, index)))
                                                                .px(px(8.))
                                                                .py(px(4.))
                                                                .rounded(px(4.))
                                                                .text_size(px(11.))
                                                                .text_color(if label.is_empty() { muted } else { text })
                                                                .cursor_pointer()
                                                                .hover(|item| item.bg(db_hover))
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    this.database_filter_edit_row = Some(index);
                                                                    this.database_filter_panel = 3;
                                                                    cx.notify();
                                                                }))
                                                                .child(if label.is_empty() { "选择值…".to_owned() } else { label })
                                                                .into_any_element()
                                                        } else if editing {
                                                            div().w(px(250.)).h(px(26.)).child(self.database_filter_value_input.clone()).into_any_element()
                                                        } else {
                                                            let value = clause.value.display();
                                                            div()
                                                                .id(SharedString::from(format!("db-filter-value-{}-{}", block_id.0, index)))
                                                                .min_w(px(90.))
                                                                .px(px(8.))
                                                                .py(px(4.))
                                                                .rounded(px(4.))
                                                                .text_size(px(11.))
                                                                .text_color(if value.is_empty() { muted } else { text })
                                                                .cursor_pointer()
                                                                .hover(|item| item.bg(db_hover))
                                                                .on_click(cx.listener(move |this, _, window, cx| this.begin_database_filter_value(block_id, db_id, active_view_id, index, window, cx)))
                                                                .child(if value.is_empty() { "输入值…".to_owned() } else { value })
                                                                .into_any_element()
                                                        };
                                                        popup = popup.child(
                                                            div()
                                                                .w_full()
                                                                .px(px(6.))
                                                                .py(px(5.))
                                                                .flex()
                                                                .flex_col()
                                                                .gap(px(4.))
                                                                .rounded(px(5.))
                                                                .border_1()
                                                                .border_color(db_border)
                                                                .child(
                                                                    div()
                                                                        .flex()
                                                                        .items_center()
                                                                        .gap(px(4.))
                                                                        .child(div().flex_1().text_size(px(11.)).text_color(text).child(name.to_owned()))
                                                                        .child(div().id(SharedString::from(format!("db-filter-op-open-{}-{}", block_id.0, index))).px(px(7.)).py(px(4.)).rounded(px(4.)).text_size(px(11.)).text_color(muted).cursor_pointer().hover(|button| button.bg(db_hover)).on_click(cx.listener(move |this, _, _, cx| { this.database_filter_edit_row = Some(index); this.database_filter_panel = 2; cx.notify(); })).child(op_name))
                                                                        .child(div().id(SharedString::from(format!("db-filter-not-{}-{}", block_id.0, index))).px(px(6.)).py(px(3.)).rounded(px(4.)).text_size(px(11.)).text_color(if flat_clause.invert { accent } else { muted }).cursor_pointer().hover(|button| button.bg(db_hover)).on_click(cx.listener(move |this, _, window, cx| this.toggle_database_filter_not(block_id, db_id, active_view_id, index, window, cx))).child("¬"))
                                                                        .child(div().id(SharedString::from(format!("db-filter-remove-{}-{}", block_id.0, index))).px(px(6.)).py(px(3.)).rounded(px(4.)).text_size(px(11.)).text_color(muted).cursor_pointer().hover(|button| button.bg(db_hover)).on_click(cx.listener(move |this, _, window, cx| this.remove_database_filter_clause(block_id, db_id, active_view_id, index, window, cx))).child("×")),
                                                                )
                                                                .child(value_row),
                                                        );
                                                    }
                                                    popup = popup.child(
                                                        div()
                                                            .flex()
                                                            .gap(px(5.))
                                                            .children([
                                                                div().id("db-filter-add").px(px(8.)).py(px(5.)).rounded(px(4.)).text_size(px(11.)).text_color(text).cursor_pointer().hover(|button| button.bg(db_hover)).on_click(cx.listener(|this, _, _, cx| { this.database_filter_panel = 1; cx.notify(); })).child("＋ 添加规则"),
                                                                div().id("db-filter-clear").px(px(8.)).py(px(5.)).rounded(px(4.)).text_size(px(11.)).text_color(muted).cursor_pointer().hover(|button| button.bg(db_hover)).on_click(cx.listener(move |this, _, window, cx| this.clear_database_filter(block_id, db_id, active_view_id, window, cx))).child("全部清空"),
                                                            ]),
                                                    );
                                                }
                                                1 => {
                                                    popup = popup.child(div().text_size(px(11.)).text_color(muted).child("‹ 选择属性"));
                                                    popup = popup.children(filter_columns.iter().filter(|property| !quire_core::core::database_view::FilterOp::ops_for(property.kind).is_empty()).map(|property| {
                                                        let property_id = property.id;
                                                        let name = property.name.clone();
                                                        div().id(SharedString::from(format!("db-filter-column-{}-{}", db_id.0, property_id.0))).w_full().px(px(8.)).py(px(5.)).rounded(px(4.)).flex().justify_between().text_size(px(11.)).text_color(text).cursor_pointer().hover(|item| item.bg(db_hover)).on_click(cx.listener(move |this, _, window, cx| { this.add_database_filter_clause(block_id, db_id, active_view_id, property_id, window, cx); })).child(name).child(database_kind_label(property.kind))
                                                    }));
                                                }
                                                2 => {
                                                    popup = popup.child(div().text_size(px(11.)).text_color(muted).child("‹ 选择比较方式"));
                                                    if let Some(row) = self.database_filter_edit_row
                                                        && let Some(clause) = flat.clauses.get(row)
                                                    {
                                                        let kind = clause.clause.kind;
                                                        popup = popup.children(quire_core::core::database_view::FilterOp::ops_for(kind).iter().copied().map(|op| {
                                                            let label = database_filter_op_label(op, kind);
                                                            div().id(SharedString::from(format!("db-filter-op-{}-{}-{}", block_id.0, row, op.as_str()))).w_full().px(px(8.)).py(px(5.)).rounded(px(4.)).text_size(px(11.)).text_color(text).cursor_pointer().hover(|item| item.bg(db_hover)).on_click(cx.listener(move |this, _, window, cx| this.set_database_filter_op(block_id, db_id, active_view_id, row, op, window, cx))).child(label)
                                                        }));
                                                    }
                                                }
                                                3 => {
                                                    popup = popup.child(div().text_size(px(11.)).text_color(muted).child("‹ 选择值"));
                                                    if let Some(row) = self.database_filter_edit_row
                                                        && let Some(clause) = flat.clauses.get(row)
                                                        && let Some(property) = self.session.databases.properties_of(db_id).find(|property| property.id == clause.clause.property)
                                                    {
                                                        let selected = match &clause.clause.value {
                                                            quire_core::core::database_view::FilterValue::Text(id) => vec![id.clone()],
                                                            quire_core::core::database_view::FilterValue::Any(ids) => ids.clone(),
                                                            _ => Vec::new(),
                                                        };
                                                        let options = quire_core::core::database_property::PropertyOptions::from_config(&property.config);
                                                        popup = popup.children(options.iter().map(|option| {
                                                            let id = option.id.to_string();
                                                            let label = option.name.clone();
                                                            let checked = selected.contains(&id);
                                                            div().id(SharedString::from(format!("db-filter-option-{}-{}-{}", db_id.0, row, id))).w_full().px(px(8.)).py(px(5.)).rounded(px(4.)).text_size(px(11.)).text_color(if checked { accent } else { text }).cursor_pointer().hover(|item| item.bg(db_hover)).on_click(cx.listener(move |this, _, window, cx| this.toggle_database_filter_option(block_id, db_id, active_view_id, row, id.clone(), window, cx))).child(format!("{}{}", if checked { "☑ " } else { "□ " }, label))
                                                        }));
                                                    }
                                                }
                                                _ => {}
                                            }
                                        } else {
                                            popup = popup.child(div().text_size(px(11.)).text_color(muted).child("当前筛选包含嵌套规则，GPUI 保留筛选结果但暂不允许编辑。"));
                                        }
                                        popup
                                    });
                                    let column_picker = columns_open.then(|| {
                                        let mut picker = div()
                                            .w_full()
                                            .px(px(8.))
                                            .py(px(6.))
                                            .flex()
                                            .flex_wrap()
                                            .gap(px(5.))
                                            .border_b_1()
                                            .border_color(db_border)
                                            .children(all_columns.iter().map(|property| {
                                                let property_id = property.id;
                                                let property_kind = property.kind;
                                                let visible = columns
                                                    .iter()
                                                    .any(|column| column.id == property_id);
                                                let kind_open = self.database_property_kind_open
                                                    == Some((db_id.0, property_id.0));
                                                let mut item = div()
                                                    .id(SharedString::from(format!(
                                                        "db-column-toggle-{}-{}",
                                                        db_id.0, property_id.0
                                                    )))
                                                    .px(px(7.))
                                                    .py(px(4.))
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(5.))
                                                    .rounded(px(4.))
                                                    .text_size(px(11.))
                                                    .text_color(if visible { text } else { muted })
                                                    .child(if visible { "☑" } else { "□" })
                                                    .child(property.name.clone());
                                                if !self.session.page.locked {
                                                    let kind_label = database_kind_label(property_kind);
                                                    item = item.child(
                                                        div()
                                                            .id(SharedString::from(format!(
                                                                "db-property-kind-{}-{}",
                                                                db_id.0, property_id.0
                                                            )))
                                                            .px(px(5.))
                                                            .py(px(2.))
                                                            .rounded(px(3.))
                                                            .text_color(if kind_open { accent } else { muted })
                                                            .child(kind_label)
                                                            .cursor_pointer()
                                                            .hover(|button| button.bg(db_hover))
                                                            .on_click(cx.listener(
                                                                move |this, _, window, cx| {
                                                                    cx.stop_propagation();
                                                                    let key = (db_id.0, property_id.0);
                                                                    this.database_property_kind_open =
                                                                        (this.database_property_kind_open != Some(key))
                                                                            .then_some(key);
                                                                    if this.database_property_kind_open == Some(key) {
                                                                        if property_kind == quire_core::core::database::PropertyKind::Formula {
                                                                            this.begin_database_formula_edit(block_id.0, db_id.0, property_id.0, window, cx);
                                                                        } else if matches!(property_kind, quire_core::core::database::PropertyKind::Select | quire_core::core::database::PropertyKind::Status | quire_core::core::database::PropertyKind::MultiSelect) {
                                                                            this.begin_database_option_edit(block_id.0, db_id.0, property_id.0, window, cx);
                                                                        }
                                                                    } else if this.active_database_formula
                                                                        == Some((block_id.0, db_id.0, property_id.0))
                                                                    {
                                                                        this.active_database_formula = None;
                                                                    } else if this.active_database_option == Some((block_id.0, db_id.0, property_id.0)) {
                                                                        this.active_database_option = None;
                                                                    } else if this.active_database_relation.is_some_and(|(active_block, active_db, _, active_property)| active_block == block_id.0 && active_db == db_id.0 && active_property == property_id.0) {
                                                                        this.active_database_relation = None;
                                                                    }
                                                                    cx.notify();
                                                                },
                                                            )),
                                                    );
                                                    if let Some(view_id) = selected_view {
                                                        item = item
                                                            .cursor_pointer()
                                                            .hover(|item| {
                                                                item.bg(db_hover)
                                                            })
                                                            .on_click(cx.listener(
                                                                move |this, _, window, cx| {
                                                                    this.toggle_database_column(
                                                                        block_id,
                                                                        db_id,
                                                                        view_id,
                                                                        property_id,
                                                                        window,
                                                                        cx,
                                                                    );
                                                                },
                                                            ));
                                                    }
                                                }
                                                item
                                            }));
                                        if let Some((kind_db, property_id)) =
                                            self.database_property_kind_open
                                                .filter(|(kind_db, _)| *kind_db == db_id.0)
                                        {
                                            if let Some(property) = all_columns
                                                .iter()
                                                .find(|property| property.id.0 == property_id)
                                            {
                                                let property_kind = property.kind;
                                                let name = property.name.clone();
                                                picker = picker.child(
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "db-property-kind-options-{kind_db}-{property_id}"
                                                        )))
                                                        .w_full()
                                                        .px(px(5.))
                                                        .py(px(5.))
                                                        .flex()
                                                        .flex_wrap()
                                                        .items_center()
                                                        .gap(px(5.))
                                                        .border_t_1()
                                                        .border_color(db_border)
                                                        .child(format!("{} 类型：", name))
                                                        .children(database_property_kind_choices().into_iter().map(|kind| {
                                                            let selected = kind == property_kind;
                                                            div()
                                                                .id(SharedString::from(format!(
                                                                    "db-kind-option-{}-{}-{}",
                                                                    db_id.0,
                                                                    property_id,
                                                                    kind.as_str()
                                                                )))
                                                                .px(px(7.))
                                                                .py(px(4.))
                                                                .rounded(px(4.))
                                                                .text_size(px(11.))
                                                                .text_color(if selected { Palette::color(&p.fg) } else { muted })
                                                                .when(selected, |button| button.bg(accent.opacity(0.45)))
                                                                .when(!selected && !self.session.page.locked, |button| {
                                                                    button.cursor_pointer().hover(|button| button.bg(db_hover))
                                                                })
                                                                .on_click(cx.listener(move |this, _, window, cx| {
                                                                    this.set_database_property_kind(
                                                                        block_id,
                                                                        quire_core::core::database::PropertyId(property_id),
                                                                        kind,
                                                                        window,
                                                                        cx,
                                                                    );
                                                                }))
                                                                .child(database_kind_label(kind))
                                                        })),
                                                );
                                                if property_kind
                                                    == quire_core::core::database::PropertyKind::Formula
                                                {
                                                    let editing = self.active_database_formula
                                                        == Some((block_id.0, db_id.0, property_id));
                                                    let mut formula_editor = div()
                                                        .id(SharedString::from(format!(
                                                            "db-formula-editor-{}-{property_id}",
                                                            db_id.0
                                                        )))
                                                        .w_full()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(6.))
                                                        .border_t_1()
                                                        .border_color(db_border);
                                                    if editing {
                                                        formula_editor = formula_editor
                                                            .child(div().flex_1().child(self.database_formula_input.clone()))
                                                            .child(
                                                                div()
                                                                    .id(SharedString::from(format!(
                                                                        "db-formula-save-{}-{property_id}",
                                                                        db_id.0
                                                                    )))
                                                                    .px(px(8.))
                                                                    .py(px(5.))
                                                                    .rounded(px(4.))
                                                                    .text_size(px(11.))
                                                                    .text_color(accent)
                                                                    .cursor_pointer()
                                                                    .hover(|button| button.bg(db_hover))
                                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                                        this.commit_database_formula(true, window, cx)
                                                                    }))
                                                                    .child("保存公式"),
                                                            );
                                                    } else {
                                                        formula_editor = formula_editor
                                                            .text_size(px(11.))
                                                            .text_color(muted)
                                                            .child("点击公式类型可编辑表达式");
                                                    }
                                                    picker = picker.child(formula_editor);
                                                }
                                                if matches!(property_kind, quire_core::core::database::PropertyKind::Number | quire_core::core::database::PropertyKind::Date | quire_core::core::database::PropertyKind::CreatedTime | quire_core::core::database::PropertyKind::LastEditedTime) {
                                                    let formats: &[(&str, &str)] = match property_kind {
                                                        quire_core::core::database::PropertyKind::Number => &[("plain", "数字"), ("integer", "整数"), ("percent", "百分比")],
                                                        _ => &[("date", "日期"), ("datetime", "日期与时间")],
                                                    };
                                                    let selected_format = quire_core::core::database_property::json::Json::parse(&property.config).ok().and_then(|config| config.get("format").and_then(quire_core::core::database_property::json::Json::as_str).map(str::to_owned)).unwrap_or_else(|| if matches!(property_kind, quire_core::core::database::PropertyKind::CreatedTime | quire_core::core::database::PropertyKind::LastEditedTime) { "datetime".into() } else if property_kind == quire_core::core::database::PropertyKind::Number { "plain".into() } else { "date".into() });
                                                    picker = picker.child(div().id(SharedString::from(format!("db-format-editor-{}-{property_id}", db_id.0))).w_full().flex().items_center().flex_wrap().gap(px(5.)).border_t_1().border_color(db_border).px(px(5.)).py(px(5.)).child(div().text_size(px(11.)).text_color(muted).child("显示格式：")).children(formats.iter().map(|(format, label)| {
                                                        let selected = selected_format == *format;
                                                        let format = (*format).to_owned();
                                                        div().id(SharedString::from(format!("db-format-{}-{}", property_id, format))).px(px(7.)).py(px(4.)).rounded(px(4.)).text_size(px(11.)).text_color(if selected { text } else { muted }).when(selected, |item| item.bg(accent.opacity(0.32))).when(!selected && !self.session.page.locked, |item| item.cursor_pointer().hover(|item| item.bg(db_hover))).on_click(cx.listener(move |this, _, window, cx| {
                                                            match this.session.set_database_property_format(block_id, quire_core::core::database::PropertyId(property_id), &format) {
                                                                Ok(_) => { this.schedule_save(true, window, cx); cx.notify(); }
                                                                Err(error) => { this.status = error.to_string(); this.error = true; cx.notify(); }
                                                            }
                                                        })).child(*label)
                                                    })));
                                                }
                                                if property_kind == quire_core::core::database::PropertyKind::Relation {
                                                    let relation = quire_core::core::database_relation::config_relation(&property.config);
                                                    let databases = self.session.databases.databases.clone();
                                                    picker = picker.child(div().id(SharedString::from(format!("db-relation-config-{}-{property_id}", db_id.0))).w_full().flex().items_center().flex_wrap().gap(px(5.)).border_t_1().border_color(db_border).px(px(5.)).py(px(5.)).child(div().text_size(px(11.)).text_color(muted).child("关联到：")).children(databases.into_iter().map(|database| {
                                                        let selected = relation.target == Some(database.id);
                                                        let target = database.id;
                                                        div().id(SharedString::from(format!("db-relation-target-{}-{}", property_id, target.0))).px(px(7.)).py(px(4.)).rounded(px(4.)).text_size(px(11.)).text_color(if selected { text } else { muted }).when(selected, |item| item.bg(accent.opacity(0.32))).when(!selected && !self.session.page.locked, |item| item.cursor_pointer().hover(|item| item.bg(db_hover))).on_click(cx.listener(move |this, _, window, cx| {
                                                            match this.session.set_database_relation_target(block_id, quire_core::core::database::PropertyId(property_id), Some(target)) {
                                                                Ok(_) => { this.schedule_save(true, window, cx); cx.notify(); }
                                                                Err(error) => { this.status = error.to_string(); this.error = true; cx.notify(); }
                                                            }
                                                        })).child(database.name)
                                                    })));
                                                    if let Some(target) = relation.target {
                                                        let candidates = self.session.databases.properties_of(target).filter(|candidate| candidate.kind == quire_core::core::database::PropertyKind::Relation).cloned().collect::<Vec<_>>();
                                                        let current_mirror = relation.mirror;
                                                        picker = picker.child(div().id(SharedString::from(format!("db-relation-mirror-{}-{property_id}", db_id.0))).w_full().flex().items_center().flex_wrap().gap(px(5.)).border_t_1().border_color(db_border).px(px(5.)).py(px(5.)).child(div().text_size(px(11.)).text_color(muted).child("双向列：")).children(candidates.into_iter().map(|candidate| {
                                                            let selected = current_mirror == Some(candidate.id);
                                                            let mirror = candidate.id;
                                                            div().id(SharedString::from(format!("db-relation-mirror-choice-{property_id}-{}", mirror.0))).px(px(7.)).py(px(4.)).rounded(px(4.)).text_size(px(11.)).text_color(if selected { text } else { muted }).when(selected, |item| item.bg(accent.opacity(0.32))).when(!selected && !self.session.page.locked, |item| item.cursor_pointer().hover(|item| item.bg(db_hover))).on_click(cx.listener(move |this, _, window, cx| {
                                                                match this.session.set_database_relation_mirror(block_id, quire_core::core::database::PropertyId(property_id), mirror) {
                                                                    Ok(_) => { this.schedule_save(true, window, cx); cx.notify(); }
                                                                    Err(error) => { this.status = error.to_string(); this.error = true; cx.notify(); }
                                                                }
                                                            })).child(candidate.name)
                                                        })));
                                                    }
                                                }
                                                if property_kind == quire_core::core::database::PropertyKind::Rollup {
                                                    use quire_core::core::database_rollup::Aggregate;
                                                    let config = quire_core::core::database_rollup::config_rollup(&property.config);
                                                    let relations = self.session.databases.properties_of(db_id).filter(|candidate| candidate.kind == quire_core::core::database::PropertyKind::Relation).cloned().collect::<Vec<_>>();
                                                    let target_db = config.relation.and_then(|relation| self.session.databases.properties.iter().find(|candidate| candidate.id == relation)).and_then(|relation| quire_core::core::database_relation::config_relation(&relation.config).target);
                                                    let target_columns = target_db.map(|target| self.session.databases.properties_of(target).filter(|candidate| !candidate.kind.is_computed() && candidate.kind != quire_core::core::database::PropertyKind::Relation).cloned().collect::<Vec<_>>()).unwrap_or_default();
                                                    let mut editor = div().id(SharedString::from(format!("db-rollup-editor-{}-{property_id}", db_id.0))).w_full().flex_col().gap(px(5.)).border_t_1().border_color(db_border).px(px(5.)).py(px(5.));
                                                    editor = editor.child(div().flex().items_center().flex_wrap().gap(px(5.)).child(div().text_size(px(11.)).text_color(muted).child("关联列：")).children(relations.into_iter().map(|relation| {
                                                        let selected = config.relation == Some(relation.id);
                                                        let relation_id = relation.id;
                                                        div().id(SharedString::from(format!("db-rollup-relation-{}-{}", property_id, relation_id.0))).px(px(7.)).py(px(4.)).rounded(px(4.)).text_size(px(11.)).text_color(if selected { text } else { muted }).when(selected, |item| item.bg(accent.opacity(0.32))).when(!self.session.page.locked, |item| item.cursor_pointer().hover(|item| item.bg(db_hover))).on_click(cx.listener(move |this, _, window, cx| {
                                                            match this.session.set_database_rollup(block_id, quire_core::core::database::PropertyId(property_id), Some(relation_id), None, Aggregate::None) {
                                                                Ok(_) => { this.schedule_save(true, window, cx); cx.notify(); }
                                                                Err(error) => { this.status = error.to_string(); this.error = true; cx.notify(); }
                                                            }
                                                        })).child(relation.name)
                                                    })));
                                                    if config.relation.is_some() {
                                                        editor = editor.child(div().flex().items_center().flex_wrap().gap(px(5.)).child(div().text_size(px(11.)).text_color(muted).child("目标列：")).children(target_columns.into_iter().map(|column| {
                                                            let selected = config.column == Some(column.id);
                                                            let column_id = column.id;
                                                            let relation_id = config.relation;
                                                            let aggregate = config.aggregate;
                                                            div().id(SharedString::from(format!("db-rollup-column-{}-{}", property_id, column_id.0))).px(px(7.)).py(px(4.)).rounded(px(4.)).text_size(px(11.)).text_color(if selected { text } else { muted }).when(selected, |item| item.bg(accent.opacity(0.32))).when(!self.session.page.locked, |item| item.cursor_pointer().hover(|item| item.bg(db_hover))).on_click(cx.listener(move |this, _, window, cx| {
                                                                match this.session.set_database_rollup(block_id, quire_core::core::database::PropertyId(property_id), relation_id, Some(column_id), aggregate) {
                                                                    Ok(_) => { this.schedule_save(true, window, cx); cx.notify(); }
                                                                    Err(error) => { this.status = error.to_string(); this.error = true; cx.notify(); }
                                                                }
                                                            })).child(column.name)
                                                        })));
                                                        editor = editor.child(div().flex().items_center().flex_wrap().gap(px(5.)).child(div().text_size(px(11.)).text_color(muted).child("计算：")).children(Aggregate::ALL.into_iter().map(|aggregate| {
                                                            let selected = config.aggregate == aggregate;
                                                            let relation_id = config.relation;
                                                            let column_id = config.column;
                                                            div().id(SharedString::from(format!("db-rollup-aggregate-{}-{}", property_id, aggregate.as_str()))).px(px(7.)).py(px(4.)).rounded(px(4.)).text_size(px(11.)).text_color(if selected { text } else { muted }).when(selected, |item| item.bg(accent.opacity(0.32))).when(!self.session.page.locked, |item| item.cursor_pointer().hover(|item| item.bg(db_hover))).on_click(cx.listener(move |this, _, window, cx| {
                                                                match this.session.set_database_rollup(block_id, quire_core::core::database::PropertyId(property_id), relation_id, column_id, aggregate) {
                                                                    Ok(_) => { this.schedule_save(true, window, cx); cx.notify(); }
                                                                    Err(error) => { this.status = error.to_string(); this.error = true; cx.notify(); }
                                                                }
                                                            })).child(aggregate.label())
                                                        })));
                                                    }
                                                    picker = picker.child(editor);
                                                }
                                                if matches!(property_kind, quire_core::core::database::PropertyKind::Select | quire_core::core::database::PropertyKind::Status | quire_core::core::database::PropertyKind::MultiSelect) {
                                                    let options = quire_core::core::database_property::PropertyOptions::from_config(&property.config);
                                                    let editing = self.active_database_option == Some((block_id.0, db_id.0, property_id));
                                                    let mut option_editor = div()
                                                        .id(SharedString::from(format!("db-option-editor-{}-{property_id}", db_id.0)))
                                                        .w_full()
                                                        .flex_col()
                                                        .gap(px(5.))
                                                        .border_t_1()
                                                        .border_color(db_border)
                                                        .px(px(5.))
                                                        .py(px(5.));
                                                    if options.is_empty() {
                                                        option_editor = option_editor.child(div().text_size(px(11.)).text_color(muted).child("还没有选项"));
                                                    } else {
                                                        option_editor = option_editor.child(div().flex().flex_wrap().gap(px(5.)).children(options.iter().map(|option| {
                                                            div().px(px(7.)).py(px(3.)).rounded(px(4.)).bg(db_hover).text_size(px(11.)).text_color(text).child(option.name.clone())
                                                        })));
                                                    }
                                                    if editing && !self.session.page.locked {
                                                        option_editor = option_editor.child(div().flex().items_center().gap(px(5.)).child(div().flex_1().child(self.database_option_input.clone())).child(
                                                            div().id(SharedString::from(format!("db-option-add-{}-{property_id}", db_id.0))).px(px(8.)).py(px(5.)).rounded(px(4.)).text_size(px(11.)).text_color(accent).cursor_pointer().hover(|button| button.bg(db_hover)).on_click(cx.listener(|this, _, window, cx| this.commit_database_option(window, cx))).child("添加")
                                                        ));
                                                    } else if !self.session.page.locked {
                                                        option_editor = option_editor.child(div().text_size(px(11.)).text_color(muted).child("点击类型标签添加选项"));
                                                    }
                                                    picker = picker.child(option_editor);
                                                }
                                            }
                                        }
                                        picker
                                    });
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
                                                    let name = view.name.clone();
                                                    let edit_name = name.clone();
                                                    let selected = Some(view_id) == selected_view;
                                                    let editing_name = self.active_database_view_edit
                                                        == Some((block_id.0, db_id.0, view_id.0));
                                                    div()
                                                        .id(SharedString::from(format!("db-view-{}-{}", db_id.0, view_id.0)))
                                                        .px(px(7.))
                                                        .py(px(3.))
                                                        .rounded(px(4.))
                                                        .cursor_pointer()
                                                        .text_size(px(11.))
                                                        .text_color(if selected { Palette::color(&p.fg) } else { muted })
                                                        .when(selected, |tab| tab.bg(Palette::color(&p.accent).opacity(0.82)))
                                                        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                                                            if event.click_count() >= 2 {
                                                                this.begin_database_view_name_edit(
                                                                    block_id.0,
                                                                    db_id.0,
                                                                    view_id.0,
                                                                    edit_name.clone(),
                                                                    window,
                                                                    cx,
                                                                );
                                                            } else if event.click_count() == 1 {
                                                                this.selected_database_views.insert(db_id.0, view_id);
                                                                this.session.set_setting(
                                                                    format!("gpui.database-view.{}", db_id.0),
                                                                    view_id.0.to_string(),
                                                                );
                                                                this.schedule_save(false, window, cx);
                                                            }
                                                        }))
                                                        .child(if editing_name {
                                                            div()
                                                                .min_w(px(50.))
                                                                .max_w(px(130.))
                                                                .child(self.database_property_name_input.clone())
                                                                .into_any_element()
                                                        } else {
                                                            div()
                                                                .child(name.clone())
                                                                .into_any_element()
                                                        })
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
                                                                    this.session.set_setting(
                                                                        format!("gpui.database-view.{}", db_id.0),
                                                                        view.0.to_string(),
                                                                    );
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
                                                        .child({
                                                            if self.database_search_open == Some(db_id.0) {
                                                                div()
                                                                    .flex()
                                                                    .items_center()
                                                                    .gap(px(4.))
                                                                    .child(
                                                                        div()
                                                                            .w(px(148.))
                                                                            .h(px(26.))
                                                                            .px(px(6.))
                                                                            .rounded(px(4.))
                                                                            .border_1()
                                                                            .border_color(db_border)
                                                                            .bg(Palette::color(&p.bg).opacity(0.28))
                                                                            .child(self.database_search_input.clone()),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .id(SharedString::from(format!(
                                                                                "db-search-close-{}",
                                                                                db_id.0
                                                                            )))
                                                                            .px(px(5.))
                                                                            .cursor_pointer()
                                                                            .text_color(muted)
                                                                            .hover(|button| {
                                                                                button.bg(db_hover)
                                                                            })
                                                                            .on_click(cx.listener(
                                                                                |this, _, window, cx| {
                                                                                    this.close_database_search(window, cx)
                                                                                },
                                                                            ))
                                                                            .child("×"),
                                                                    )
                                                                    .into_any_element()
                                                            } else {
                                                                div()
                                                                    .id(SharedString::from(format!(
                                                                        "db-search-open-{}",
                                                                        db_id.0
                                                                    )))
                                                                    .px(px(7.))
                                                                    .py(px(4.))
                                                                    .rounded(px(4.))
                                                                    .cursor_pointer()
                                                                    .text_color(muted)
                                                                    .hover(|button| button.bg(db_hover))
                                                                    .on_click(cx.listener(
                                                                        move |this, _, window, cx| {
                                                                            this.open_database_search(
                                                                                db_id.0,
                                                                                window,
                                                                                cx,
                                                                            );
                                                                        },
                                                                    ))
                                                                    .child("搜索")
                                                                    .into_any_element()
                                                            }
                                                        })
                                                        .child(div().flex().gap(px(4.)).child({
                                                            let mut button = div()
                                                                .id(SharedString::from(format!(
                                                                    "db-filter-toggle-{}",
                                                                    block_id.0
                                                                )))
                                                                .px(px(8.))
                                                                .py(px(4.))
                                                                .rounded(px(4.))
                                                                .text_color(if filter_open { accent } else { muted })
                                                                .hover(|button| button.bg(db_hover))
                                                                .child(format!(
                                                                    "筛选 {}",
                                                                    view_filter.as_ref().map_or(0, |filter| filter.clause_count())
                                                                ));
                                                            if !self.session.page.locked {
                                                                button = button.cursor_pointer().on_click(cx.listener(
                                                                    move |this, _, _, cx| {
                                                                        let key = selected_view.map(|view| (block_id.0, view.0));
                                                                        this.database_filter_open = key.filter(|key| {
                                                                            this.database_filter_open != Some(*key)
                                                                        });
                                                                        if this.database_filter_open.is_some() {
                                                                            this.database_filter_panel = 0;
                                                                            this.database_filter_edit_row = None;
                                                                            this.database_filter_value_active = None;
                                                                        }
                                                                        this.database_columns_open = None;
                                                                        this.database_group_open = None;
                                                                        cx.notify();
                                                                    },
                                                                ));
                                                            }
                                                            button
                                                        }).child({
                                                            let is_table = !list_layout;
                                                            let mut button = div()
                                                                .id(SharedString::from(format!(
                                                                    "db-layout-toggle-{}",
                                                                    block_id.0
                                                                )))
                                                                .px(px(8.))
                                                                .py(px(4.))
                                                                .rounded(px(4.))
                                                                .text_color(muted)
                                                                .hover(|button| button.bg(db_hover))
                                                                .child(if is_table { "表格" } else { "列表" });
                                                            if !self.session.page.locked {
                                                                button = button.cursor_pointer().on_click(cx.listener(
                                                                    move |this, _, window, cx| {
                                                                        let layout = if is_table {
                                                                            quire_core::core::database::ViewLayout::List
                                                                        } else {
                                                                            quire_core::core::database::ViewLayout::Table
                                                                        };
                                                                        match this.session.set_database_view_layout(
                                                                            block_id,
                                                                            active_view_id,
                                                                            layout,
                                                                        ) {
                                                                            Ok(true) => {
                                                                                this.schedule_save(true, window, cx);
                                                                                cx.notify();
                                                                            }
                                                                            Ok(false) => {}
                                                                            Err(error) => {
                                                                                this.status = format!("视图布局保存失败：{error}");
                                                                                this.error = true;
                                                                                cx.notify();
                                                                            }
                                                                        }
                                                                    },
                                                                ));
                                                            }
                                                            button
                                                        }).child({
                                                            let group_name = grouped_property
                                                                .and_then(|property_id| {
                                                                    self.session.databases.properties_of(db_id)
                                                                        .find(|property| property.id == property_id)
                                                                        .map(|property| property.name.clone())
                                                                });
                                                            let mut button = div()
                                                                .id(SharedString::from(format!(
                                                                    "db-group-toggle-{}",
                                                                    block_id.0
                                                                )))
                                                                .px(px(8.))
                                                                .py(px(4.))
                                                                .rounded(px(4.))
                                                                .text_color(if group_open { accent } else { muted })
                                                                .hover(|button| button.bg(db_hover))
                                                                .child(group_name.map_or_else(
                                                                    || "分组".to_owned(),
                                                                    |name| format!("分组 · {name}"),
                                                                ));
                                                            if !self.session.page.locked {
                                                                button = button.cursor_pointer().on_click(cx.listener(
                                                                    move |this, _, _, cx| {
                                                                        let key = selected_view.map(|view| (block_id.0, view.0));
                                                                        this.database_group_open = key.filter(|key| this.database_group_open != Some(*key));
                                                                        if this.database_group_open.is_some() {
                                                                            this.database_filter_open = None;
                                                                            this.database_columns_open = None;
                                                                        }
                                                                        cx.notify();
                                                                    },
                                                                ));
                                                            }
                                                            button
                                                        }).child({
                                                            let mut button = div()
                                                                .id(SharedString::from(format!(
                                                                    "db-columns-toggle-{}",
                                                                    block_id.0
                                                                )))
                                                                .px(px(8.))
                                                                .py(px(4.))
                                                                .rounded(px(4.))
                                                                .text_color(if columns_open {
                                                                    accent
                                                                } else {
                                                                    muted
                                                                })
                                                                .hover(|button| {
                                                                    button.bg(
                                                                        Palette::color(&p.bg)
                                                                            .opacity(0.3),
                                                                    )
                                                                })
                                                                .child(format!(
                                                                    "列 {}/{}",
                                                                    columns.len() + 1,
                                                                    all_columns.len() + 1
                                                                ));
                                                            if !self.session.page.locked {
                                                                button = button
                                                                    .cursor_pointer()
                                                                    .on_click(cx.listener(
                                                                        move |this, _, _, cx| {
                                                                            let key = selected_view
                                                                                .map(|view| {
                                                                                    (block_id.0, view.0)
                                                                                });
                                                                            this.database_columns_open =
                                                                                key.filter(|key| {
                                                                                    this.database_columns_open
                                                                                        != Some(*key)
                                                                                });
                                                                            if this.database_columns_open.is_some() {
                                                                                this.database_group_open = None;
                                                                            }
                                                                            this.database_property_kind_open = None;
                                                                            cx.notify();
                                                                        },
                                                                    ));
                                                            }
                                                            button
                                                        }).child(
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
                                                                            .is_some_and(|record| {
                                                                                this.selected_database_record = Some((
                                                                                    block_id.0,
                                                                                    db_id.0,
                                                                                    record.0,
                                                                                ));
                                                                                true
                                                                            })
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
                                                            {
                                                                let selected = self.selected_database_record.is_some_and(
                                                                    |(block, db, _)| {
                                                                        block == block_id.0 && db == db_id.0
                                                                    },
                                                                );
                                                                let mut button = div()
                                                                    .id(SharedString::from(format!(
                                                                        "db-delete-row-{}",
                                                                        block_id.0
                                                                    )))
                                                                    .px(px(8.))
                                                                    .py(px(4.))
                                                                    .rounded(px(4.))
                                                                    .text_color(if selected { p.block_text(9) } else { muted })
                                                                    .hover(|button| button.bg(Palette::color(&p.bg).opacity(0.3)))
                                                                    .child("删除选中行");
                                                                if selected && !self.session.page.locked {
                                                                    button = button.cursor_pointer().on_click(cx.listener(
                                                                        move |this, _, window, cx| {
                                                                            let Some((_, _, record)) = this.selected_database_record.filter(
                                                                                |(block, db, _)| *block == block_id.0 && *db == db_id.0,
                                                                            ) else {
                                                                                return;
                                                                            };
                                                                            match this.session.delete_database_record(
                                                                                block_id,
                                                                                quire_core::core::database::RecordId(record),
                                                                            ) {
                                                                                Ok(true) => {
                                                                                    this.selected_database_record = None;
                                                                                    this.schedule_save(true, window, cx);
                                                                                }
                                                                                Ok(false) => {}
                                                                                Err(error) => {
                                                                                    this.status = error.to_string();
                                                                                    this.error = true;
                                                                                }
                                                                            }
                                                                            cx.notify();
                                                                        },
                                                                    ));
                                                                }
                                                                button
                                                            },
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
                                        .when_some(column_picker, |view, picker| {
                                            view.child(picker)
                                        })
                                        .when_some(group_picker, |view, picker| {
                                            view.child(picker)
                                        })
                                        .when_some(filter_picker, |view, picker| {
                                            view.child(picker)
                                        })
                                        .when(!list_layout, |view| view.child(header))
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
                                )
                                .when(!self.session.page.locked, |actions| {
                                    actions.child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "block-more-{block_id:?}"
                                            )))
                                            .px(px(3.))
                                            .cursor_pointer()
                                            .text_color(muted)
                                            .hover(|button| button.text_color(accent))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.block_menu_open = if this
                                                    .block_menu_open
                                                    .is_some_and(|(open_id, _)| open_id == block_id)
                                                {
                                                    None
                                                } else {
                                                    Some((block_id, BlockMenuMode::Actions))
                                                };
                                                cx.notify();
                                            }))
                                            .child("⋮"),
                                    )
                                }),
                        )
                    })
                    .when_some(color_picker_view, |row, picker| row.child(picker))
                    .when_some(block_context_popup, |row, popup| row.child(popup))
            .into_any_element()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar_transition = self.sidebar_transition;
        let p = self.palette.clone();
        let maximized = window.is_maximized();
        let database_width = (window.bounds().size.width - px(356.))
            .min(px(760.))
            .max(px(200.));
        let text = Palette::color(&p.fg);
        let muted = Palette::color(&p.muted);
        let accent = Palette::color(&p.accent);
        let elevated = Palette::color(&p.elevated);
        let mcp_enabled = self.mcp_server.is_some();
        let mcp_status = self.mcp_status.clone();
        let mcp_endpoint = format!("http://127.0.0.1:{}/mcp", self.mcp_port);
        let mcp_token = self.mcp_token.clone();
        let hotkey_alt_n = self.session.setting("gpui.hotkey.alt-n").unwrap_or("true") == "true";
        let hotkey_alt_m = self.session.setting("gpui.hotkey.alt-m").unwrap_or("false") == "true";
        let quick_note_on_start = self
            .session
            .setting("gpui.quick-note-on-start")
            .unwrap_or("false")
            == "true";
        let hotkey_notice = self.hotkey_notice.clone();
        let tray_available = self.tray_available;
        let mcp_logs: Vec<serde_json::Value> = self
            .session
            .setting("gpui.mcp.log")
            .and_then(|value| serde_json::from_str(value).ok())
            .unwrap_or_default();
        let sync_peers = self.session.sync_peers();
        let sync_logs = self.session.sync_log();
        let (sync_auto, sync_interval) = self.session.sync_config();
        let sync_offline_count = sync_peers
            .iter()
            .filter(|peer| {
                quire_core::services::sync::engine::now_unix().saturating_sub(peer.last_seen) > 15
            })
            .count();
        let sync_status = self.sync_status.clone();
        let sync_listener_warning = self.sync_listener_warning.clone();
        let sync_device_name = self
            .session
            .setting("sync.device-name")
            .unwrap_or("Quire")
            .to_owned();
        let backlinks_count = self
            .session
            .repo
            .reference_count(self.session.page.id)
            .unwrap_or(0);
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
                                div().flex().justify_between().items_center()
                                    .child("启动时自动弹出新建笔记输入框")
                                    .child(div().id("quick-note-on-start").px(px(8.)).py(px(4.)).rounded(px(4.))
                                        .border_1().border_color(Palette::color(&p.border)).cursor_pointer()
                                        .text_color(if quick_note_on_start { accent } else { muted })
                                        .on_click(cx.listener(|this, _, window, cx| this.toggle_quick_note_on_start(window, cx)))
                                        .child(if quick_note_on_start { "● 已开启" } else { "○ 已关闭" })),
                            ).child(
                                div().flex().justify_between().items_center()
                                    .child("全局 Alt+N 速记（不打开主界面）")
                                    .child(div().id("hotkey-alt-n").px(px(8.)).py(px(4.)).rounded(px(4.))
                                        .border_1().border_color(Palette::color(&p.border)).cursor_pointer()
                                        .text_color(if hotkey_alt_n { accent } else { muted })
                                        .on_click(cx.listener(|this, _, window, cx| this.toggle_global_hotkey(
                                            crate::platform::HOTKEY_ALT_N, "gpui.hotkey.alt-n", true, "Alt+N", window, cx
                                        )))
                                        .child(if hotkey_alt_n { "● 已开启" } else { "○ 已关闭" })),
                            ).child(
                                div().flex().justify_between().items_center()
                                    .child("全局 Alt+M 显示主界面")
                                    .child(div().id("hotkey-alt-m").px(px(8.)).py(px(4.)).rounded(px(4.))
                                        .border_1().border_color(Palette::color(&p.border)).cursor_pointer()
                                        .text_color(if hotkey_alt_m { accent } else { muted })
                                        .on_click(cx.listener(|this, _, window, cx| this.toggle_global_hotkey(
                                            crate::platform::HOTKEY_ALT_M, "gpui.hotkey.alt-m", false, "Alt+M", window, cx
                                        )))
                                        .child(if hotkey_alt_m { "● 已开启" } else { "○ 已关闭" })),
                            ).child(
                                div().text_size(px(11.)).text_color(muted).child(hotkey_notice),
                            ).when(!tray_available, |panel| panel.child(
                                div().text_size(px(10.)).text_color(rgb(0xd9a441))
                                    .child("系统托盘当前不可用；关闭窗口会保存并退出 Quire。"),
                            )),
                        ))
                        .when(self.settings_tab == 2, |dialog| dialog.child(
                            div().flex().flex_col().gap(px(8.))
                                .child(div().flex().items_center().justify_between()
                                    .child(div().flex().flex_col().gap(px(2.))
                                        .child(div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).child("局域网同步"))
                                        .child(div().text_size(px(10.)).text_color(muted).child(format!("本机：{sync_device_name}"))))
                                    .child(div().id("sync-now-all").px(px(10.)).py(px(6.)).rounded(px(5.))
                                        .bg(Palette::color(&p.accent)).text_color(Palette::color(&p.fg)).cursor_pointer().text_size(px(11.))
                                        .on_click(cx.listener(|this, _, window, cx| this.sync_now_all(window, cx)))
                                        .child("立即同步")))
                                .child(div().text_size(px(10.)).text_color(muted).child(sync_status.clone()))
                                .when_some(sync_listener_warning, |panel, warning| panel.child(
                                    div().text_size(px(10.)).text_color(rgb(0xd9a441)).child(warning),
                                ))
                                .child(div().flex().items_center().gap(px(6.))
                                    .child(div().h(px(27.)).flex_1().min_w_0().px(px(7.)).rounded(px(4.))
                                        .border_1().border_color(Palette::color(&p.border))
                                        .child(self.sync_name_input.clone()))
                                    .child(div().id("sync-name-apply").px(px(8.)).py(px(5.)).rounded(px(4.))
                                        .bg(Palette::color(&p.accent)).text_color(Palette::color(&p.fg)).cursor_pointer().text_size(px(9.))
                                        .on_click(cx.listener(|this, _, window, cx| this.apply_sync_name(window, cx)))
                                        .child("保存并重启")))
                                .child(div().text_size(px(9.)).text_color(muted).child("设备名称应用后重启 Quire，新的名称才会广播到局域网。"))
                                .child(div().flex().items_center().gap(px(5.))
                                    .child(div().text_size(px(10.)).child(if sync_auto { "自动同步" } else { "手动同步" }))
                                    .child(div().id("sync-auto-toggle").px(px(7.)).py(px(4.)).rounded(px(4.))
                                        .border_1().border_color(Palette::color(&p.border)).cursor_pointer().text_size(px(9.))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            let (enabled, _) = this.session.sync_config();
                                            this.session.sync_set_auto(!enabled);
                                            this.sync_last_auto = std::time::Instant::now();
                                            this.sync_status = if enabled { "自动同步已关闭" } else { "自动同步已开启" }.into();
                                            this.schedule_save(true, window, cx);
                                            cx.notify();
                                        }))
                                        .child(if sync_auto { "关闭" } else { "开启" }))
                                    .children([10u64, 30, 60].into_iter().map(|seconds|
                                        div().id(SharedString::from(format!("sync-interval-{seconds}"))).px(px(6.)).py(px(4.)).rounded(px(4.))
                                            .border_1().border_color(Palette::color(&p.border)).cursor_pointer().text_size(px(9.))
                                            .when(sync_interval == seconds, |chip| chip.bg(Palette::color(&p.accent)).text_color(Palette::color(&p.fg)))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.session.sync_set_interval(seconds);
                                                this.schedule_save(true, window, cx);
                                                cx.notify();
                                            }))
                                            .child(format!("{seconds}s")))))
                                .child(div().h(px(1.)).bg(Palette::color(&p.border)))
                                .child(div().flex().items_center().justify_between()
                                    .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).child(format!("设备 · {}", sync_peers.len())))
                                    .child(div().id("sync-refresh").px(px(6.)).py(px(3.))
                                        .border_1().border_color(Palette::color(&p.border)).cursor_pointer().text_size(px(9.))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            const STALE_AFTER: u64 = 30 * 24 * 60 * 60;
                                            let stale = this.session.sync_prune_stale_peers(STALE_AFTER);
                                            if stale.is_empty() {
                                                this.sync_status = "设备列表已刷新；正在监听局域网广播".into();
                                            } else {
                                                let names = stale.iter().map(|peer| peer.name.as_str()).collect::<Vec<_>>().join("、");
                                                let message = format!("刷新列表并清理 {} 台长期离线设备：{}", stale.len(), names);
                                                this.session.sync_log_push("本机", true, &message);
                                                this.sync_status = message;
                                                this.schedule_save(true, window, cx);
                                            }
                                            cx.notify();
                                        }))
                                        .child("刷新列表"))
                                    .child(div().id("sync-forget-offline").px(px(6.)).py(px(3.))
                                        .border_1().border_color(Palette::color(&p.border)).cursor_pointer().text_size(px(9.))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            let retired = this.session.sync_retire_offline_peers(15);
                                            if retired.is_empty() {
                                                this.sync_status = "没有离线设备需要清理".into();
                                            } else {
                                                let names = retired.iter().map(|peer| peer.name.as_str()).collect::<Vec<_>>().join("、");
                                                let message = format!("已移除 {} 台离线设备：{}", retired.len(), names);
                                                this.session.sync_log_push("本机", true, &message);
                                                this.sync_status = message;
                                                this.schedule_save(true, window, cx);
                                            }
                                            cx.notify();
                                        }))
                                        .child(format!("清理离线 · {sync_offline_count}"))))
                                .children(sync_peers.iter().map(|peer| {
                                    let peer_for_sync = peer.clone();
                                    let peer_key = peer.id.clone();
                                    let id_for_forget = peer.id.clone();
                                    let peer_id = peer.id.clone();
                                    let peer_name = peer.name.clone();
                                    let last_sync = if peer.last_sync.is_empty() { "尚未同步".to_owned() } else { peer.last_sync.clone() };
                                    let online = quire_core::services::sync::engine::now_unix().saturating_sub(peer.last_seen) <= 15;
                                    div().flex().items_center().justify_between().gap(px(6.)).px(px(7.)).py(px(5.))
                                        .rounded(px(5.)).border_1().border_color(Palette::color(&p.border))
                                        .child(div().flex().flex_col().gap(px(2.))
                                            .child(div().text_size(px(11.)).child(format!("{}  {}", if online { "●" } else { "○" }, peer.name)))
                                            .child(div().text_size(px(9.)).text_color(muted).child(format!("{}:{} · {}", peer.ip, peer.port, last_sync))))
                                        .child(div().flex().gap(px(5.))
                                            .child(div().id(SharedString::from(format!("sync-peer-{peer_id}"))).px(px(7.)).py(px(4.)).rounded(px(4.))
                                                .bg(Palette::color(&p.accent)).text_color(Palette::color(&p.fg)).cursor_pointer().text_size(px(9.))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    if !this.sync_pending.insert(peer_key.clone()) {
                                                        this.sync_status = "该设备的同步仍在进行中".into();
                                                        cx.notify();
                                                        return;
                                                    }
                                                    if this.sync_cmd_tx.send(quire_core::services::sync::engine::Cmd::SyncWith(peer_for_sync.clone())).is_ok() {
                                                        this.sync_status = format!("正在与 {peer_name} 同步…");
                                                    } else {
                                                        this.sync_pending.remove(&peer_key);
                                                        this.sync_status = "同步引擎已停止，请重启 Quire 后重试".into();
                                                    }
                                                    cx.notify();
                                                }))
                                                .child("同步"))
                                            .child(div().id(SharedString::from(format!("forget-peer-{peer_id}"))).px(px(7.)).py(px(4.))
                                                .border_1().border_color(Palette::color(&p.border)).cursor_pointer().text_size(px(9.))
                                                .on_click(cx.listener(move |this, _, window, cx| {
                                                    this.session.sync_forget_peer(&id_for_forget);
                                                    this.schedule_save(true, window, cx);
                                                    cx.notify();
                                                }))
                                                .child("忘记")))
                                }))
                                .when(sync_peers.is_empty(), |panel| panel.child(div().py(px(8.)).text_size(px(10.)).text_color(muted).child("正在搜索设备；请确认设备位于同一局域网并已启动 Quire。")))
                                .child(div().h(px(1.)).bg(Palette::color(&p.border)))
                                .child(div().flex().items_center().justify_between()
                                    .child(div().text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).child(format!("同步日志 · {} 条", sync_logs.len())))
                                    .child(div().id("sync-clear-log").px(px(6.)).py(px(3.))
                                        .border_1().border_color(Palette::color(&p.border)).cursor_pointer().text_size(px(9.))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.session.sync_log_clear();
                                            this.schedule_save(true, window, cx);
                                            cx.notify();
                                        }))
                                        .child("清空")))
                                .children(sync_logs.iter().rev().take(3).map(|line| div().text_size(px(9.)).text_color(muted).child(format!("{} · {} · {}", line.at, line.peer, line.message))))
                        ))
                        .when(self.settings_tab == 3, |dialog| dialog.child(
                            div().flex().flex_col().gap(px(10.))
                                .child(div().flex().items_center().justify_between()
                                    .child(div().flex().items_center().gap(px(10.))
                                        .child(div().text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).child("MCP"))
                                        .child(div().text_size(px(11.)).text_color(muted).child(mcp_status.clone())))
                                    .child(div().flex().items_center().gap(px(5.))
                                        .when(mcp_enabled, |row| row.child(div().id("mcp-restart").px(px(8.)).py(px(5.)).rounded(px(5.))
                                            .border_1().border_color(Palette::color(&p.border)).cursor_pointer().text_size(px(10.))
                                            .on_click(cx.listener(|this, _, window, cx| this.restart_mcp(window, cx)))
                                            .child("重启")))
                                        .child(div().id("mcp-toggle").px(px(11.)).py(px(6.)).rounded(px(5.))
                                            .bg(Palette::color(&p.accent)).text_color(Palette::color(&p.fg))
                                            .cursor_pointer().text_size(px(11.))
                                            .on_click(cx.listener(|this, _, window, cx| this.toggle_mcp(window, cx)))
                                            .child(if mcp_enabled { "关闭服务" } else { "启用服务" }))))
                                .child(div().h(px(1.)).bg(Palette::color(&p.border)))
                                .child(div().font_weight(FontWeight::SEMIBOLD).text_size(px(12.)).child("客户端连接信息"))
                                .child(div().flex().items_center().gap(px(6.))
                                    .child(div().text_size(px(11.)).text_color(muted).child("本机端口"))
                                    .child(div().w(px(90.)).h(px(28.)).px(px(7.)).rounded(px(4.))
                                        .border_1().border_color(Palette::color(&p.border))
                                        .child(self.mcp_port_input.clone()))
                                    .child(div().id("mcp-port-apply").px(px(8.)).py(px(5.)).rounded(px(4.))
                                        .border_1().border_color(Palette::color(&p.border)).cursor_pointer().text_size(px(10.))
                                        .on_click(cx.listener(|this, _, window, cx| this.apply_mcp_port(window, cx)))
                                        .child(if mcp_enabled { "应用并重启" } else { "应用端口" })))
                                .child(div().text_size(px(11.)).text_color(muted).child(format!("Endpoint  {mcp_endpoint}")))
                                .child(div().text_size(px(11.)).text_color(muted).child(format!("Authorization: Bearer {mcp_token}")))
                                .child(div().flex().gap(px(8.))
                                    .child(div().id("mcp-copy-config").px(px(9.)).py(px(5.)).rounded(px(4.))
                                        .border_1().border_color(Palette::color(&p.border)).cursor_pointer().text_size(px(10.))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            let config = serde_json::json!({
                                                "url": format!("http://127.0.0.1:{}/mcp", this.mcp_port),
                                                "headers": {"Authorization": format!("Bearer {}", this.mcp_token)}
                                            }).to_string();
                                            cx.write_to_clipboard(ClipboardItem::new_string(config));
                                            this.status = "MCP 连接配置已复制".into();
                                            cx.notify();
                                        }))
                                        .child("复制连接配置"))
                                    .child(div().id("mcp-copy-stdio-config").px(px(9.)).py(px(5.)).rounded(px(4.))
                                        .border_1().border_color(Palette::color(&p.border)).cursor_pointer().text_size(px(10.))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            let executable = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("quire-gpui"));
                                            let config = serde_json::json!({
                                                "mcpServers": {
                                                    "quire": {
                                                        "command": executable,
                                                        "args": ["--stdio", "--db", this.session.path]
                                                    }
                                                }
                                            }).to_string();
                                            cx.write_to_clipboard(ClipboardItem::new_string(config));
                                            this.status = "MCP stdio 客户端配置已复制".into();
                                            cx.notify();
                                        }))
                                        .child("复制 stdio 配置"))
                                    .child(div().id("mcp-copy-prompt").px(px(9.)).py(px(5.)).rounded(px(4.))
                                        .border_1().border_color(Palette::color(&p.border)).cursor_pointer().text_size(px(10.))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(crate::mcp_tools::agent_prompt(this.mcp_port)));
                                            this.status = "MCP Agent 提示词已复制".into();
                                            cx.notify();
                                        }))
                                        .child("复制 Agent 提示词")))
                                .child(div().text_size(px(10.)).text_color(muted).child("服务仅监听本机 127.0.0.1；工具可以读取和修改笔记/任务，删除只进入回收站。令牌保存在当前库目录的 mcp.json。"))
                                .child(div().h(px(1.)).bg(Palette::color(&p.border)))
                                .child(div().flex().items_center().justify_between()
                                    .child(div().font_weight(FontWeight::SEMIBOLD).text_size(px(12.)).child(format!("调用日志 · {} 条记录", mcp_logs.len())))
                                    .child(div().id("mcp-clear-log").px(px(6.)).py(px(3.))
                                        .border_1().border_color(Palette::color(&p.border)).cursor_pointer().text_size(px(9.))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.session.set_setting("gpui.mcp.log", "[]");
                                            this.schedule_save(true, window, cx);
                                            cx.notify();
                                        }))
                                        .child("清空")))
                                .children(mcp_logs.iter().rev().take(6).map(|entry| {
                                    let ok = entry.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false);
                                    let tool = entry.get("tool").and_then(serde_json::Value::as_str).unwrap_or("tool");
                                    let message = entry.get("message").and_then(serde_json::Value::as_str).unwrap_or("");
                                    div().text_size(px(10.)).text_color(muted)
                                        .child(format!("{}  {} · {}", if ok { "✓" } else { "!" }, tool, message.chars().take(72).collect::<String>()))
                                }))
                                .when(mcp_logs.is_empty(), |panel| panel.child(div().text_size(px(10.)).text_color(muted).child("尚无调用记录"))),
                        ))
                        .when(self.settings_tab == 4, |dialog| dialog.child(
                            div().flex().flex_col().gap(px(12.)).child(
                                div().text_size(px(12.)).text_color(muted).child("快捷键 · 编辑器"),
                            ).children([
                                ("保存页面", "Ctrl+S"),
                                ("新建页面", "Ctrl+N"),
                                ("撤销 / 重做", "Ctrl+Z / Ctrl+Shift+Z"),
                                ("查找页面", "Ctrl+P"),
                                ("页面内查找", "Ctrl+F"),
                                ("界面缩放 / 复位", "Ctrl+= / Ctrl+- / Ctrl+0"),
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
        let command_items = self.palette_items();
        let command_rows = command_items
            .iter()
            .take(8)
            .enumerate()
            .map(|(index, item)| {
                div()
                    .id(SharedString::from(format!("command-palette-row-{index}")))
                    .w_full()
                    .h(px(36.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .when(index == self.command_palette_selected, |row| {
                        row.bg(Palette::color(&p.bg).opacity(0.48))
                    })
                    .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.48)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.command_palette_selected = index;
                        this.invoke_palette_command(window, cx);
                    }))
                    .child(
                        div()
                            .w(px(18.))
                            .text_center()
                            .text_color(muted)
                            .child(item.icon),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .text_color(text)
                            .child(item.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(muted)
                            .child(item.section),
                    )
                    .when(!item.hint.is_empty(), |row| {
                        row.child(
                            div()
                                .text_size(px(10.))
                                .font_family("monospace")
                                .text_color(muted)
                                .child(item.hint),
                        )
                    })
            })
            .collect::<Vec<_>>();
        let command_overlay = div()
            .id("command-palette-overlay")
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .flex()
            .justify_center()
            .items_start()
            .bg(Hsla::from(rgb(0x000000)).opacity(0.18))
            .on_click(cx.listener(|this, _, window, cx| this.close_command_palette(window, cx)))
            .child(
                div()
                    .id("command-palette-panel")
                    .mt(px(96.))
                    .w(px(560.))
                    .max_w(window.bounds().size.width - px(24.))
                    .p(px(8.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(Palette::color(&p.border))
                    .bg(Palette::color(&p.elevated))
                    .shadow_lg()
                    .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(40.))
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(9.))
                            .border_b_1()
                            .border_color(Palette::color(&p.border))
                            .text_color(muted)
                            .child("⌕")
                            .child(div().flex_1().min_w_0().child(self.search_input.clone())),
                    )
                    .child(
                        div()
                            .id("command-palette-list")
                            .max_h(px(288.))
                            .overflow_y_scroll()
                            .py(px(4.))
                            .flex()
                            .flex_col()
                            .children(command_rows),
                    )
                    .when(command_items.is_empty(), |panel| {
                        panel.child(
                            div()
                                .h(px(42.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .text_size(px(12.))
                                .text_color(muted)
                                .child("没有匹配的命令或页面。"),
                        )
                    })
                    .child(
                        div()
                            .h(px(26.))
                            .px(px(4.))
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
        let rows = self.visible_document_blocks();
        let mut notes = (if self.organizer_trash {
            self.session.organizer.trashed_notes().collect::<Vec<_>>()
        } else {
            self.session.organizer.live_notes().collect::<Vec<_>>()
        })
        .into_iter()
        .cloned()
        .filter(|note| {
            (self.organizer_trash || !self.session.note_delete_is_pending(note.id))
                && self.organizer_tag.as_ref().is_none_or(|path| {
                    note.tags
                        .iter()
                        .any(|tag| organizer_tag_matches_path(tag, path))
                })
                && !organizer_tags_exclude(&note.tags, &self.organizer_excluded_tags)
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
                            .when(
                                self.organizer_trash || self.hovered_note == Some(id),
                                |header| {
                                    header
                                        .child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "organizer-pin-{id:?}"
                                                )))
                                                .px(px(3.))
                                                .text_color(if pinned { accent } else { muted })
                                                .cursor_pointer()
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.toggle_note_pin(id, window, cx)
                                                    },
                                                ))
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
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        if this.organizer_trash {
                                                            this.restore_note(id, window, cx)
                                                        } else {
                                                            this.trash_note(id, window, cx)
                                                        }
                                                    },
                                                ))
                                                .child(if self.organizer_trash {
                                                    "↩"
                                                } else {
                                                    "×"
                                                }),
                                        )
                                        .when(self.organizer_trash, |header| {
                                            header.child(
                                                div()
                                                    .id(SharedString::from(format!(
                                                        "organizer-note-purge-{id:?}"
                                                    )))
                                                    .px(px(3.))
                                                    .text_color(Hsla::from(rgb(0xe5534b)))
                                                    .cursor_pointer()
                                                    .hover(|button| {
                                                        button.text_color(rgb(0xff8179))
                                                    })
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.purge_note(id, window, cx)
                                                        },
                                                    ))
                                                    .child("×"),
                                            )
                                        })
                                },
                            ),
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
            if !self.organizer_trash && self.session.task_delete_is_pending(task.id) {
                return false;
            }
            if self.organizer_trash {
                return self.organizer_tag.as_ref().is_none_or(|path| {
                    task.tags
                        .iter()
                        .any(|tag| organizer_tag_matches_path(tag, path))
                }) && !organizer_tags_exclude(&task.tags, &self.organizer_excluded_tags)
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
                && self.organizer_tag.as_ref().is_none_or(|path| {
                    task.tags
                        .iter()
                        .any(|tag| organizer_tag_matches_path(tag, path))
                })
                && !organizer_tags_exclude(&task.tags, &self.organizer_excluded_tags)
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
            .filter(|task| {
                !self.session.task_delete_is_pending(task.id)
                    && task.done
                    && self.organizer_tag.as_ref().is_none_or(|path| {
                        task.tags
                            .iter()
                            .any(|tag| organizer_tag_matches_path(tag, path))
                    })
                    && !organizer_tags_exclude(&task.tags, &self.organizer_excluded_tags)
            })
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
                            )
                            .when(self.organizer_trash, |row| {
                                row.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "organizer-task-purge-{id:?}"
                                        )))
                                        .px(px(3.))
                                        .text_color(Hsla::from(rgb(0xe5534b)))
                                        .cursor_pointer()
                                        .hover(|button| button.text_color(rgb(0xff8179)))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.purge_task(id, window, cx)
                                        }))
                                        .child("×"),
                                )
                            }),
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
                .filter(|task| !self.session.task_delete_is_pending(task.id))
                .filter(|task| {
                    task_matches_smart_view(&self.session.organizer, task, view, &today, &week)
                        && self.organizer_tag.as_ref().is_none_or(|path| {
                            task.tags
                                .iter()
                                .any(|tag| organizer_tag_matches_path(tag, path))
                        })
                        && !organizer_tags_exclude(&task.tags, &self.organizer_excluded_tags)
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
                let list_name = list.name.clone();
                let list_color = list.color;
                let editing = self.task_list_editing == Some(id);
                let menu_open = self.task_list_menu_open == Some(id);
                let color_menu_open = self.task_list_color_open == Some(id);
                let count = self
                    .session
                    .organizer
                    .live_tasks()
                    .filter(|task| {
                        !self.session.task_delete_is_pending(task.id)
                            && task.list == id
                            && self.organizer_tag.as_ref().is_none_or(|path| {
                                task.tags
                                    .iter()
                                    .any(|tag| organizer_tag_matches_path(tag, path))
                            })
                            && !organizer_tags_exclude(&task.tags, &self.organizer_excluded_tags)
                    })
                    .count();
                let row = div()
                    .id(SharedString::from(format!("organizer-list-row-{id:?}")))
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .child(
                        div()
                            .id(SharedString::from(format!("organizer-list-color-{id:?}")))
                            .w(px(20.))
                            .h(px(25.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.))
                            .text_color(
                                Palette::load(&self.theme_choice).block_text(list_color.slot()),
                            )
                            .child("●"),
                    )
                    .when(editing, |row| {
                        row.flex_1().child(
                            div()
                                .id(SharedString::from(format!("organizer-list-rename-{id:?}")))
                                .flex_1()
                                .h(px(28.))
                                .px(px(5.))
                                .rounded(px(4.))
                                .bg(Palette::color(&p.bg).opacity(0.25))
                                .child(self.task_list_name_input.clone()),
                        )
                    })
                    .when(!editing, |row| {
                        row.child(
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
                                .child(format!("{} · {count}", list_name)),
                        )
                    })
                    .when(!editing, |row| {
                        row.child(
                            div()
                                .id(SharedString::from(format!(
                                    "organizer-list-menu-button-{id:?}"
                                )))
                                .w(px(24.))
                                .h(px(25.))
                                .px(px(8.))
                                .rounded(px(5.))
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(if menu_open { accent } else { muted })
                                .hover(|button| button.text_color(accent))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_task_list_menu(id, cx)
                                }))
                                .child("⋯"),
                        )
                    });
                div()
                    .id(SharedString::from(format!(
                        "organizer-list-container-{id:?}"
                    )))
                    .flex()
                    .flex_col()
                    .child(row)
                    .when(menu_open && !color_menu_open, |container| {
                        container.child(
                            div()
                                .id(SharedString::from(format!("organizer-list-menu-{id:?}")))
                                .ml(px(22.))
                                .mb(px(4.))
                                .p(px(4.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(Palette::color(&p.border))
                                .bg(Palette::color(&p.elevated))
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "organizer-list-menu-rename-{id:?}"
                                        )))
                                        .w_full()
                                        .h(px(27.))
                                        .px(px(8.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(4.))
                                        .cursor_pointer()
                                        .text_size(px(12.))
                                        .text_color(text)
                                        .hover(|item| item.bg(Palette::color(&p.bg).opacity(0.35)))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.start_task_list_rename(id, window, cx)
                                        }))
                                        .child("重命名"),
                                )
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "organizer-list-menu-color-{id:?}"
                                        )))
                                        .w_full()
                                        .h(px(27.))
                                        .px(px(8.))
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .rounded(px(4.))
                                        .cursor_pointer()
                                        .text_size(px(12.))
                                        .text_color(text)
                                        .hover(|item| item.bg(Palette::color(&p.bg).opacity(0.35)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.toggle_task_list_color_picker(id, cx)
                                        }))
                                        .child("颜色")
                                        .child(
                                            div()
                                                .text_color(
                                                    Palette::load(&self.theme_choice)
                                                        .block_text(list_color.slot()),
                                                )
                                                .child("●  ›"),
                                        ),
                                )
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "organizer-list-menu-delete-{id:?}"
                                        )))
                                        .w_full()
                                        .h(px(27.))
                                        .px(px(8.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(4.))
                                        .cursor_pointer()
                                        .text_size(px(12.))
                                        .text_color(Hsla::from(rgb(0xe5534b)))
                                        .hover(|item| item.bg(Palette::color(&p.bg).opacity(0.35)))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.delete_task_list(id, window, cx)
                                        }))
                                        .child("删除清单"),
                                ),
                        )
                    })
                    .when(menu_open && color_menu_open, |container| {
                        container.child(
                            div()
                                .id(SharedString::from(format!("organizer-list-colors-{id:?}")))
                                .ml(px(22.))
                                .mb(px(4.))
                                .p(px(4.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(Palette::color(&p.border))
                                .bg(Palette::color(&p.elevated))
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "organizer-list-menu-color-back-{id:?}"
                                        )))
                                        .w_full()
                                        .h(px(27.))
                                        .px(px(8.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(4.))
                                        .cursor_pointer()
                                        .text_size(px(12.))
                                        .text_color(muted)
                                        .hover(|item| item.bg(Palette::color(&p.bg).opacity(0.35)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.task_list_color_open = None;
                                            cx.notify();
                                        }))
                                        .child("‹ 返回"),
                                )
                                .children(
                                    ColorKind::ALL
                                        .into_iter()
                                        .filter(|color| *color != ColorKind::Default)
                                        .map(|color| {
                                            let color_id = id;
                                            let selected = color == list_color;
                                            let label = match color {
                                                ColorKind::Gray => "灰色",
                                                ColorKind::Brown => "棕色",
                                                ColorKind::Orange => "橙色",
                                                ColorKind::Yellow => "黄色",
                                                ColorKind::Green => "绿色",
                                                ColorKind::Blue => "蓝色",
                                                ColorKind::Purple => "紫色",
                                                ColorKind::Pink => "粉色",
                                                ColorKind::Red => "红色",
                                                ColorKind::Default => "默认",
                                            };
                                            div()
                                                .id(SharedString::from(format!(
                                                    "organizer-list-color-{id:?}-{}",
                                                    color.slot()
                                                )))
                                                .w_full()
                                                .h(px(25.))
                                                .px(px(8.))
                                                .flex()
                                                .items_center()
                                                .gap(px(8.))
                                                .rounded(px(4.))
                                                .cursor_pointer()
                                                .text_size(px(12.))
                                                .text_color(text)
                                                .hover(|item| {
                                                    item.bg(Palette::color(&p.bg).opacity(0.35))
                                                })
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.set_task_list_color(
                                                            color_id, color, window, cx,
                                                        )
                                                    },
                                                ))
                                                .child(
                                                    div()
                                                        .w(px(10.))
                                                        .h(px(10.))
                                                        .rounded(px(5.))
                                                        .bg(Palette::load(&self.theme_choice)
                                                            .block_text(color.slot())),
                                                )
                                                .child(label)
                                                .when(selected, |item| {
                                                    item.child(div().flex_1()).child("✓")
                                                })
                                        }),
                                ),
                        )
                    })
            })
            .collect::<Vec<_>>();
        let mut organizer_item_tags = Vec::<Vec<String>>::new();
        if self.organizer_tasks {
            let tasks = if self.organizer_trash {
                self.session.organizer.trashed_tasks().collect::<Vec<_>>()
            } else {
                self.session
                    .organizer
                    .live_tasks()
                    .filter(|task| !self.session.task_delete_is_pending(task.id))
                    .collect::<Vec<_>>()
            };
            for task in tasks {
                organizer_item_tags.push(task.tags.clone());
            }
        } else {
            let notes = if self.organizer_trash {
                self.session.organizer.trashed_notes().collect::<Vec<_>>()
            } else {
                self.session
                    .organizer
                    .live_notes()
                    .filter(|note| !self.session.note_delete_is_pending(note.id))
                    .collect::<Vec<_>>()
            };
            for note in notes {
                organizer_item_tags.push(note.tags.clone());
            }
        }
        let organizer_tag_path = self.organizer_tag.as_deref().unwrap_or("");
        let organizer_tag_tree = organizer_tag_tree_rows(&organizer_item_tags, organizer_tag_path);
        let organizer_excluded_label = (!self.organizer_excluded_tags.is_empty()).then(|| {
            format!(
                "排除 {}",
                self.organizer_excluded_tags
                    .iter()
                    .map(|tag| format!("⊘#{tag}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        });
        let organizer_tag_buttons = organizer_tag_tree
            .into_iter()
            .map(|(tag, count, has_children)| {
                let tag_path = if organizer_tag_path.is_empty() {
                    tag.clone()
                } else {
                    format!("{organizer_tag_path}/{tag}")
                };
                let excluded = self.organizer_excluded_tags.contains(&tag_path);
                let label = tag_path.clone();
                let exclude_label = tag_path.clone();
                div()
                    .id(SharedString::from(format!("organizer-tag-{tag_path}")))
                    .w_full()
                    .h(px(25.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(px(5.))
                    .child(
                        div()
                            .id(SharedString::from(format!("organizer-tag-pick-{tag_path}")))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .text_color(if excluded {
                                Hsla::from(rgb(0xe5534b))
                            } else {
                                muted
                            })
                            .hover(|row| row.text_color(text))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.toggle_organizer_tag(label.clone(), window, cx)
                            }))
                            .child(format!("{tag}  ·  {count}")),
                    )
                    .when(has_children, |row| {
                        let child_path = tag_path.clone();
                        row.child(
                            div()
                                .id(SharedString::from(format!(
                                    "organizer-tag-expand-{child_path}"
                                )))
                                .w(px(22.))
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .text_color(muted)
                                .hover(|button| button.text_color(accent))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.toggle_organizer_tag(child_path.clone(), window, cx)
                                }))
                                .child("›"),
                        )
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "organizer-tag-exclude-{exclude_label}"
                            )))
                            .w(px(21.))
                            .h(px(21.))
                            .mr(px(2.))
                            .rounded(px(11.))
                            .border_1()
                            .border_color(if excluded {
                                Hsla::from(rgb(0xe5534b))
                            } else {
                                Palette::color(&p.border)
                            })
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_size(px(13.))
                            .text_color(if excluded {
                                Hsla::from(rgb(0xe5534b))
                            } else {
                                muted
                            })
                            .hover(|button| button.border_color(Hsla::from(rgb(0xe5534b))))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.toggle_organizer_tag_exclusion(
                                    exclude_label.clone(),
                                    window,
                                    cx,
                                )
                            }))
                            .child("⊖"),
                    )
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
                            } else if self.organizer_trash {
                                format!("回收站 · {note_count} 项")
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
                                    .when(self.organizer_trash, |button| {
                                        button
                                            .text_color(Hsla::from(rgb(0xe5534b)))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.empty_organizer_bin(window, cx)
                                            }))
                                    })
                                    .child(if self.organizer_trash {
                                        "清空回收站"
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
                                .when_some(
                                    self.session
                                        .pending_organizer_delete()
                                        .map(|pending| pending.message.clone()),
                                    |list, message| {
                                        list.child(
                                            div()
                                                .id("organizer-delete-undo")
                                                .w_full()
                                                .px(px(10.))
                                                .py(px(8.))
                                                .rounded(px(6.))
                                                .border_1()
                                                .border_color(Palette::color(&p.border))
                                                .bg(Palette::color(&p.elevated))
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .child(div().flex_1().min_w_0().text_size(px(11.)).child(message))
                                                .child(
                                                    div()
                                                        .id("organizer-delete-undo-button")
                                                        .px(px(9.))
                                                        .py(px(4.))
                                                        .rounded(px(4.))
                                                        .cursor_pointer()
                                                        .text_color(accent)
                                                        .hover(|button| button.bg(Palette::color(&p.bg).opacity(0.35)))
                                                        .on_click(cx.listener(|this, _, _, cx| this.undo_organizer_delete(cx)))
                                                        .child("撤销"),
                                                ),
                                        )
                                    },
                                )
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
                                    self.session
                                        .organizer
                                        .live_tasks()
                                        .filter(|task| !self.session.task_delete_is_pending(task.id))
                                        .count()
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
                                    .text_color(if self.organizer_tag.is_none() && self.organizer_excluded_tags.is_empty() { accent } else { muted })
                                    .when(self.organizer_tag.is_none() && self.organizer_excluded_tags.is_empty(), |row| row.bg(Palette::color(&p.elevated)))
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
                            .child(div().text_size(px(11.)).text_color(muted).mt(px(6.)).child(if organizer_tag_path.is_empty() { "标签".to_owned() } else { organizer_tag_path.replace('/', " / ") }))
                            .when(self.organizer_tag.is_some(), |nav| {
                                let parent_path = organizer_tag_path.rsplit_once('/').map(|(parent, _)| parent.to_owned()).unwrap_or_default();
                                nav.child(
                                    div()
                                        .id("organizer-tag-parent")
                                        .px(px(8.))
                                        .py(px(6.))
                                        .rounded(px(5.))
                                        .cursor_pointer()
                                        .text_color(muted)
                                        .hover(|row| row.text_color(accent).bg(Palette::color(&p.bg).opacity(0.3)))
                                        .on_click(cx.listener(move |this, _, window, cx| this.toggle_organizer_tag(parent_path.clone(), window, cx)))
                                        .child("↑ 上级"),
                                )
                            })
                            .when_some(organizer_excluded_label, |nav, label| {
                                nav.child(
                                    div()
                                        .id("organizer-excluded-tags")
                                        .px(px(8.))
                                        .py(px(4.))
                                        .text_size(px(10.))
                                        .text_color(Hsla::from(rgb(0xe5534b)))
                                        .child(label),
                                )
                            })
                            .when(
                                self.organizer_tag.is_some()
                                    || !self.organizer_excluded_tags.is_empty(),
                                |nav| {
                                    nav.child(
                                        div()
                                            .id("organizer-clear-filters")
                                            .px(px(8.))
                                            .py(px(6.))
                                            .rounded(px(5.))
                                            .cursor_pointer()
                                            .text_color(accent)
                                            .hover(|row| row.bg(Palette::color(&p.bg).opacity(0.3)))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.clear_organizer_filters(window, cx)
                                            }))
                                            .child("清除筛选"),
                                    )
                                },
                            )
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
        let document_blocks = rows;
        let document_ids = document_blocks
            .iter()
            .map(|(_, id)| *id)
            .collect::<Vec<_>>();
        self.sync_document_list_ids(&document_ids);
        let scroll_weak = cx.entity().downgrade();
        self.document_list.set_scroll_handler(move |event, _, app| {
            let visible_range = event.visible_range.clone();
            let _ = scroll_weak.update(app, |this, cx| {
                this.document_visible_range = visible_range;
                let mut keep = this.document_visible_block_ids();
                if let Some(active) = this.active {
                    keep.insert(active.0);
                }
                this.inputs
                    .retain(|id, input| keep.contains(id) || input.read(cx).buffer.composing());
                this.input_subscriptions
                    .retain(|id, _| this.inputs.contains_key(id));
            });
        });
        let row_weak = cx.entity().downgrade();
        let row_palette = p.clone();
        let row_text = text;
        let row_muted = muted;
        let row_accent = accent;
        let row_elevated = elevated;
        let row_width = if self.session.page.full_width {
            1120.
        } else {
            760.
        };
        let document_rows = list(
            self.document_list.clone(),
            move |list_index, window, app| {
                if list_index == 0 {
                    let weak = row_weak.clone();
                    let palette = row_palette.clone();
                    return weak
                        .update(app, |this, cx| {
                            this.render_document_header(cx, row_text, row_muted, &palette)
                        })
                        .unwrap_or_else(|_| div().into_any_element());
                }
                if list_index > document_ids.len() {
                    return div().h(px(128.)).into_any_element();
                }
                let (index, id) = document_blocks[list_index - 1];
                let weak = row_weak.clone();
                let palette = row_palette.clone();
                let sync_sources = &sync_sources;
                let page_candidates = &page_candidates;
                weak.update(app, |this, cx| {
                    let Some(block) = this.session.document.block(id).cloned() else {
                        return div().into_any_element();
                    };
                    div()
                        .w_full()
                        .max_w(px(row_width))
                        .mx_auto()
                        .child(this.render_document_block(
                            index,
                            &block,
                            window,
                            cx,
                            row_text,
                            row_muted,
                            row_accent,
                            row_elevated,
                            &palette,
                            database_width,
                            sync_sources,
                            page_candidates,
                        ))
                        .into_any_element()
                })
                .unwrap_or_else(|_| div().into_any_element())
            },
        );
        let page_content = if self.organizer_open {
            organizer_view
        } else {
            div()
                .id("document-scroll")
                .flex_1()
                .min_w_0()
                .min_h_0()
                .h_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .id("document-list-scroll")
                        .flex_1()
                        .min_h_0()
                        .min_w_0()
                        .px(px(48.))
                        .flex()
                        .flex_col()
                        .child(document_rows.flex_1().min_h_0()),
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
            .on_action(cx.listener(Self::quick_note))
            .on_action(cx.listener(Self::open_palette_action))
            .on_action(cx.listener(Self::new_page_action))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::open_find))
            .on_action(cx.listener(Self::find_next_action))
            .on_action(cx.listener(Self::find_previous_action))
            .on_action(cx.listener(Self::open_search))
            .on_action(cx.listener(Self::navigate_back_action))
            .on_action(cx.listener(Self::navigate_forward_action))
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .on_action(cx.listener(Self::zoom_reset))
            .capture_key_down(cx.listener(Self::task_list_menu_key_down))
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
                                    .cursor_pointer()
                                    .hover(|button| button.bg(Palette::color(&p.elevated)))
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
                                    .cursor_pointer()
                                    .hover(|button| button.bg(Palette::color(&p.elevated)))
                                    .window_control_area(WindowControlArea::Max)
                                    .child({
                                        let icon = div()
                                            .relative()
                                            .w(px(14.))
                                            .h(px(14.))
                                            .flex()
                                            .items_center()
                                            .justify_center();
                                        if maximized {
                                            icon.child(
                                                div()
                                                    .absolute()
                                                    .top(px(0.))
                                                    .right(px(0.))
                                                    .w(px(10.))
                                                    .h(px(10.))
                                                    .border_1()
                                                    .border_color(muted),
                                            )
                                            .child(
                                                div()
                                                    .absolute()
                                                    .left(px(0.))
                                                    .bottom(px(0.))
                                                    .w(px(10.))
                                                    .h(px(10.))
                                                    .bg(Palette::color(&p.elevated))
                                                    .border_1()
                                                    .border_color(muted),
                                            )
                                        } else {
                                            icon.child(
                                                div()
                                                    .w(px(11.))
                                                    .h(px(11.))
                                                    .border_1()
                                                    .border_color(muted),
                                            )
                                        }
                                    }),
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
            .when(self.command_palette_open, |root| {
                root.child(command_overlay)
            })
            .when_some(settings_overlay, |root, overlay| root.child(overlay))
            .when_some(self.startup_notice.clone(), |root, notice| {
                root.child(
                    div()
                        .id("startup-storage-notice")
                        .absolute()
                        .top(px(46.))
                        .left(px(276.))
                        .right(px(12.))
                        .flex()
                        .items_start()
                        .gap(px(10.))
                        .p(px(10.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(rgb(0xd99b38))
                        .bg(rgb(0x4b351b))
                        .shadow_lg()
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(12.))
                                .text_color(rgb(0xffe1a6))
                                .child(notice),
                        )
                        .child(
                            div()
                                .id("dismiss-startup-storage-notice")
                                .px(px(7.))
                                .py(px(2.))
                                .rounded(px(4.))
                                .text_size(px(12.))
                                .text_color(rgb(0xffe1a6))
                                .cursor_pointer()
                                .hover(|button| button.bg(rgb(0x765322)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.startup_notice = None;
                                    cx.notify();
                                }))
                                .child("关闭"),
                        ),
                )
            })
    }
}

struct QuickNoteWindow {
    owner: Entity<Workspace>,
    input: Entity<TextInput>,
    draft: String,
    suggestions: Vec<String>,
    selected_suggestion: usize,
    suggestions_hidden: bool,
    palette: Palette,
    subscription: Option<Subscription>,
}

impl QuickNoteWindow {
    fn new(
        owner: Entity<Workspace>,
        palette: Palette,
        draft: String,
        suggestions: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        crate::platform::set_window_always_on_top(window);
        let input = cx.new(|cx| TextInput::new(draft.clone(), Vec::new(), 15., window, cx));
        input.update(cx, |input, _| {
            input.placeholder = Some("写下速记…（#标签会加入笔记标签）".into())
        });
        input.update(cx, |input, _| input.buffer.move_to(draft.len()));
        let mut this = Self {
            owner,
            input: input.clone(),
            draft,
            suggestions,
            selected_suggestion: 0,
            suggestions_hidden: false,
            palette,
            subscription: None,
        };
        this.subscription = Some(cx.subscribe(&input, |this, entity, event, cx| match event {
            InputEvent::Edited { text, .. } => {
                this.draft = text.clone();
                let caret = entity.read(cx).buffer.caret();
                this.refresh_suggestions(caret, cx);
            }
            InputEvent::Split(_) => {
                if let Some(tag) = this.current_suggestion() {
                    this.pick_suggestion(tag, cx);
                } else {
                    entity.update(cx, |input, cx| input.insert_newline(cx));
                }
            }
            InputEvent::Indent => {
                if let Some(tag) = this.current_suggestion() {
                    this.pick_suggestion(tag, cx);
                }
            }
            _ => {}
        }));
        window.focus(&input.read(cx).focus);
        this
    }
    fn suggestions_visible(&self) -> bool {
        !self.suggestions_hidden && !self.suggestions.is_empty()
    }
    fn current_suggestion(&self) -> Option<String> {
        self.suggestions_visible()
            .then(|| self.suggestions.get(self.selected_suggestion).cloned())
            .flatten()
    }
    fn refresh_suggestions(&mut self, caret: usize, cx: &mut Context<Self>) {
        let prefix = self.draft.get(..caret).unwrap_or(&self.draft).to_owned();
        self.suggestions = self
            .owner
            .read(cx)
            .session
            .organizer_tag_suggestions(&prefix);
        self.selected_suggestion = 0;
        self.suggestions_hidden = false;
        cx.notify();
    }
    fn pick_suggestion(&mut self, tag: String, cx: &mut Context<Self>) {
        let caret = self.input.read(cx).buffer.caret();
        let Some((draft, caret)) = capture_tag_replacement(&self.draft, caret, &tag) else {
            return;
        };
        self.draft = draft.clone();
        self.input.update(cx, |input, cx| {
            input.reset(draft.clone(), Vec::new(), caret, cx);
        });
        self.refresh_suggestions(caret, cx);
    }
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.read(cx).buffer.composing() {
            return;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if self.suggestions_visible() && !modifiers.modified() {
            match key {
                "up" | "down" => {
                    let count = self.suggestions.len();
                    if key == "up" {
                        self.selected_suggestion = (self.selected_suggestion + count - 1) % count;
                    } else {
                        self.selected_suggestion = (self.selected_suggestion + 1) % count;
                    }
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                "enter" | "tab" => {
                    if let Some(tag) = self.current_suggestion() {
                        self.pick_suggestion(tag, cx);
                        window.focus(&self.input.read(cx).focus);
                    }
                    cx.stop_propagation();
                    return;
                }
                "escape" => {
                    self.suggestions_hidden = true;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }
        if key == "escape" && !modifiers.modified() {
            cx.stop_propagation();
            self.cancel(window, cx);
        }
    }
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.draft.trim().is_empty() {
            return;
        }
        let text = self.draft.clone();
        let saved = self
            .owner
            .update(cx, |workspace, cx| workspace.save_quick_note(text, cx));
        if saved {
            window.remove_window();
        }
    }
    fn save_action(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        self.submit(window, cx);
    }
    fn cancel(&mut self, window: &mut Window, _: &mut Context<Self>) {
        window.remove_window();
    }
}

impl Render for QuickNoteWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette.clone();
        let foreground = Palette::color(&palette.fg);
        let muted = Palette::color(&palette.muted);
        let accent = Palette::color(&palette.accent);
        let suggestions = self.suggestions.clone();
        let selected_suggestion = self.selected_suggestion;
        let suggestions_visible = self.suggestions_visible();
        div()
            .key_context("QuireQuickNote")
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.key_down(event, window, cx)
            }))
            .size_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(18.))
            .text_color(foreground)
            .bg(linear_gradient(
                180.,
                linear_color_stop(Palette::color(&palette.bg), 0.),
                linear_color_stop(Palette::color(&palette.grad2), 1.),
            )
            .color_space(ColorSpace::Srgb))
            .child(
                div()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .window_control_area(WindowControlArea::Drag)
                    .child(div().w(px(4.)).h(px(14.)).rounded(px(2.)).bg(accent))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(14.))
                            .font_weight(FontWeight::BOLD)
                            .child("速记"),
                    )
                    .child(
                        div()
                            .id("quick-note-close")
                            .w(px(24.))
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .cursor_pointer()
                            .text_color(muted)
                            .hover(|button| button.bg(Palette::color(&palette.elevated)))
                            .on_click(cx.listener(|this, _, window, cx| this.cancel(window, cx)))
                            .child("×"),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(muted)
                    .child("提示：# 输入标签，Ctrl+Enter 或 Alt+S 提交，Esc 取消"),
            )
            .child(
                div()
                    .id("quick-note-input")
                    .flex_1()
                    .min_h(px(56.))
                    .p(px(10.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(Palette::color(&palette.border))
                    .bg(Palette::color(&palette.elevated))
                    .child(self.input.clone()),
            )
            .when(suggestions_visible, |root| {
                root.child(
                    div()
                        .id("quick-note-suggestions")
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .max_h(px(148.))
                        .overflow_y_scroll()
                        .p(px(4.))
                        .rounded(px(7.))
                        .border_1()
                        .border_color(Palette::color(&palette.border))
                        .bg(Palette::color(&palette.bg))
                        .children(suggestions.into_iter().enumerate().map(|(index, tag)| {
                            let selected = index == selected_suggestion;
                            let selected_tag = tag.clone();
                            div()
                                .id(SharedString::from(format!("quick-note-suggestion-{index}")))
                                .h(px(26.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .rounded(px(4.))
                                .cursor_pointer()
                                .when(selected, |row| {
                                    row.bg(accent).text_color(Palette::color(&palette.bg))
                                })
                                .when(!selected, |row| {
                                    row.text_color(foreground)
                                        .hover(|row| row.bg(Palette::color(&palette.elevated)))
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.pick_suggestion(selected_tag.clone(), cx);
                                    window.focus(&this.input.read(cx).focus);
                                }))
                                .child(format!("#{tag}"))
                        })),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_size(px(11.)).text_color(muted).child(
                        if self.draft.trim().is_empty() {
                            "空草稿".to_owned()
                        } else {
                            format!("{} 字", self.draft.chars().count())
                        },
                    ))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                div()
                                    .id("quick-note-cancel")
                                    .px(px(12.))
                                    .py(px(7.))
                                    .rounded(px(6.))
                                    .text_color(muted)
                                    .cursor_pointer()
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.cancel(window, cx)),
                                    )
                                    .child("取消"),
                            )
                            .child(
                                div()
                                    .id("quick-note-submit")
                                    .px(px(14.))
                                    .py(px(7.))
                                    .rounded(px(6.))
                                    .bg(accent)
                                    .text_color(Palette::color(&palette.bg))
                                    .cursor_pointer()
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.submit(window, cx)),
                                    )
                                    .child("保存速记"),
                            ),
                    ),
            )
            .on_action(cx.listener(Self::save_action))
    }
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-s", Save, Some("Quire")),
        KeyBinding::new("ctrl-enter", Save, Some("QuireQuickNote")),
        KeyBinding::new("alt-s", Save, Some("QuireQuickNote")),
        KeyBinding::new("alt-n", QuickNote, Some("Quire")),
        KeyBinding::new("ctrl-k", OpenCommandPalette, Some("Quire")),
        KeyBinding::new("ctrl-n", NewPage, Some("Quire")),
        KeyBinding::new("ctrl-z", Undo, Some("Quire")),
        KeyBinding::new("ctrl-shift-z", Redo, Some("Quire")),
        KeyBinding::new("ctrl-y", Redo, Some("Quire")),
        KeyBinding::new("ctrl-f", Find, Some("Quire")),
        KeyBinding::new("ctrl-g", FindNext, Some("Quire")),
        KeyBinding::new("ctrl-shift-g", FindPrevious, Some("Quire")),
        KeyBinding::new("ctrl-p", SearchPages, Some("Quire")),
        KeyBinding::new("alt-left", NavigateBack, Some("Quire")),
        KeyBinding::new("alt-right", NavigateForward, Some("Quire")),
        KeyBinding::new("ctrl-=", ZoomIn, Some("Quire")),
        KeyBinding::new("ctrl--", ZoomOut, Some("Quire")),
        KeyBinding::new("ctrl-0", ZoomReset, Some("Quire")),
    ]);
}
