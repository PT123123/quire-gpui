//! Core-only session. UI previews and IME text do not enter this module.
use anyhow::{Context, Result, bail};
use quire_core::{
    core::{
        command::{self, Command},
        document::{Document, Entry},
        history::History,
        persistence::{Change, Repository},
        types::{Block, BlockId, BlockKind, Mark, OrderKey, Page, PageFont, PageId},
    },
    services::persistence::PersistenceService,
    storage::SqliteRepository,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

pub struct Session {
    pub path: PathBuf,
    pub pages: Vec<Page>,
    settings: BTreeMap<String, String>,
    pub page: Page,
    pub document: Document,
    pub persistence: Arc<PersistenceService>,
    pub repo: Arc<SqliteRepository>,
    history: History,
    typing: Option<(BlockId, Entry, Instant)>,
    // Prevent two GPUI instances allocating IDs against the same stale document.
    _instance_lock: std::fs::File,
}

fn snap_left(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    text.grapheme_indices(true)
        .take_while(|(start, _)| *start <= byte)
        .last()
        .map_or(0, |(start, _)| start)
}

fn split_marks(marks: &[Mark], at: usize) -> (Vec<Mark>, Vec<Mark>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for mark in marks {
        if mark.start < at {
            let mut part = mark.clone();
            part.end = part.end.min(at);
            if part.start < part.end {
                left.push(part);
            }
        }
        if mark.end > at {
            let mut part = mark.clone();
            part.start = part.start.saturating_sub(at);
            part.end -= at;
            if part.start < part.end {
                right.push(part);
            }
        }
    }
    (left, right)
}

fn adjust_marks(old: &str, new: &str, marks: &[Mark]) -> Vec<Mark> {
    let prefix = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    let old_rest = &old[prefix..];
    let new_rest = &new[prefix..];
    let suffix = old_rest
        .chars()
        .rev()
        .zip(new_rest.chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    let mut adjusted = Vec::new();
    for mark in marks {
        if mark.end <= prefix {
            adjusted.push(mark.clone());
        } else if mark.start >= old_end {
            let mut shifted = mark.clone();
            shifted.start = new_end + (mark.start - old_end);
            shifted.end = new_end + (mark.end - old_end);
            adjusted.push(shifted);
        } else {
            if mark.start < prefix {
                let mut left = mark.clone();
                left.end = prefix;
                if left.start < left.end {
                    adjusted.push(left);
                }
            }
            if mark.end > old_end {
                let mut right = mark.clone();
                right.start = new_end;
                right.end = new_end + (mark.end - old_end);
                if right.start < right.end {
                    adjusted.push(right);
                }
            }
        }
    }
    adjusted
}

impl Session {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut lock_path = path.as_os_str().to_os_string();
        lock_path.push(".gpui.lock");
        let instance_lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        instance_lock
            .try_lock()
            .context("此库已被另一个 Quire GPUI 实例打开")?;
        let repo =
            Arc::new(SqliteRepository::open(path).context("无法打开库；不会覆盖或重建已有文件")?);
        let loaded = repo.load()?;
        let mut pages = loaded.pages.clone();
        pages.sort_by_key(|p| p.order);
        let selected = pages
            .iter()
            .find(|p| !p.template && p.parent.is_none())
            .cloned()
            .or_else(|| pages.iter().find(|p| !p.template).cloned());
        if selected.is_none() && !pages.is_empty() {
            bail!("库中只有模板，本轮原型不会自动修改该库");
        }
        let page = selected.unwrap_or(Page {
            id: PageId(1),
            title: "未命名".into(),
            parent: None,
            order: OrderKey(1 << 32),
            favorite: false,
            expanded: true,
            font: PageFont::Default,
            full_width: false,
            small_text: false,
            icon: String::new(),
            cover: None,
            locked: false,
            template: false,
        });
        let mut document = Document::new(1);
        for p in &pages {
            document.set_page_blocks(
                p.id,
                loaded
                    .blocks
                    .iter()
                    .filter(|b| b.page == p.id)
                    .cloned()
                    .collect(),
            );
        }
        let persistence = Arc::new(PersistenceService::with_default_clock(repo.clone()));
        if pages.is_empty() {
            persistence.record(vec![Change::PageCreated(page.clone())]);
        }
        let mut session = Self {
            path: path.into(),
            pages,
            settings: loaded.settings,
            page,
            document,
            persistence,
            repo,
            history: History::default(),
            typing: None,
            _instance_lock: instance_lock,
        };
        if session.blocks().is_empty() && !session.page.locked {
            let command = Command::AppendBlock {
                kind: BlockKind::Paragraph,
                text: String::new(),
            };
            if let Some(entry) = command::plan(&mut session.document, session.page.id, command) {
                session.document.apply(&entry.apply);
                session.persistence.record(entry.apply);
            }
        }
        // Startup runs before the event loop, not in render or a keystroke handler.
        session.persistence.force_flush()?;
        Ok(session)
    }
    pub fn blocks(&self) -> &[Block] {
        self.document.page_blocks(self.page.id)
    }
    pub fn select_page(&mut self, id: PageId) -> bool {
        self.end_typing();
        let Some(page) = self
            .pages
            .iter()
            .find(|p| p.id == id && !p.template)
            .cloned()
        else {
            return false;
        };
        self.page = page;
        self.ensure_page_has_block();
        true
    }
    pub fn create_page(&mut self, parent: Option<PageId>) -> PageId {
        self.end_typing();
        let parent = parent.filter(|id| {
            self.pages
                .iter()
                .any(|page| page.id == *id && !page.template)
        });
        let id = PageId(self.pages.iter().map(|p| p.id.0).max().unwrap_or(0) + 1);
        let last_order = self
            .pages
            .iter()
            .filter(|p| p.parent == parent && !p.template)
            .map(|p| p.order)
            .max();
        let page = Page {
            id,
            title: "未命名".into(),
            parent,
            order: OrderKey::between(last_order, None).unwrap_or(OrderKey::FIRST),
            favorite: false,
            expanded: true,
            font: PageFont::Default,
            full_width: false,
            small_text: false,
            icon: String::new(),
            cover: None,
            locked: false,
            template: false,
        };
        self.persistence
            .record(vec![Change::PageCreated(page.clone())]);
        self.document.set_page_blocks(id, Vec::new());
        self.pages.push(page.clone());
        self.page = page;
        self.ensure_page_has_block();
        id
    }
    pub fn set_page_title(&mut self, title: String) {
        if self.page.locked || self.page.title == title {
            return;
        }
        let id = self.page.id;
        self.page.title = title.clone();
        if let Some(page) = self.pages.iter_mut().find(|page| page.id == id) {
            page.title = title.clone();
        }
        self.persistence
            .record(vec![Change::PageTitleSet { id, title }]);
    }
    pub fn setting(&self, key: &str) -> Option<&str> {
        self.settings.get(key).map(String::as_str)
    }
    pub fn set_setting(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        self.settings.insert(key.clone(), value.clone());
        self.persistence
            .record(vec![Change::SettingSet { key, value }]);
    }
    pub fn set_page_layout(&mut self, full_width: bool, small_text: bool) {
        if self.page.locked
            || (self.page.full_width == full_width && self.page.small_text == small_text)
        {
            return;
        }
        self.page.full_width = full_width;
        self.page.small_text = small_text;
        if let Some(page) = self.pages.iter_mut().find(|page| page.id == self.page.id) {
            page.full_width = full_width;
            page.small_text = small_text;
        }
        self.persistence.record(vec![Change::PageLayoutSet {
            id: self.page.id,
            full_width,
            small_text,
        }]);
    }
    pub fn set_page_font(&mut self, font: PageFont) {
        if self.page.locked || self.page.font == font {
            return;
        }
        self.page.font = font;
        if let Some(page) = self.pages.iter_mut().find(|page| page.id == self.page.id) {
            page.font = font;
        }
        self.persistence.record(vec![Change::PageFontSet {
            id: self.page.id,
            font,
        }]);
    }
    pub fn set_page_locked(&mut self, locked: bool) {
        if self.page.locked == locked {
            return;
        }
        self.page.locked = locked;
        if let Some(page) = self.pages.iter_mut().find(|page| page.id == self.page.id) {
            page.locked = locked;
        }
        self.persistence.record(vec![Change::PageLockedSet {
            id: self.page.id,
            locked,
        }]);
    }
    pub fn move_page(&mut self, id: PageId, delta: i32) -> bool {
        let Some(target) = self
            .pages
            .iter()
            .find(|page| page.id == id && !page.template)
            .cloned()
        else {
            return false;
        };
        let mut siblings = self
            .pages
            .iter()
            .filter(|page| page.parent == target.parent && !page.template)
            .map(|page| page.id)
            .collect::<Vec<_>>();
        siblings.sort_by_key(|sibling| {
            self.pages
                .iter()
                .find(|page| page.id == *sibling)
                .map(|page| page.order)
        });
        let Some(current) = siblings.iter().position(|sibling| *sibling == id) else {
            return false;
        };
        let Some(destination) = current.checked_add_signed(delta as isize) else {
            return false;
        };
        if destination >= siblings.len() {
            return false;
        }
        siblings.remove(current);
        siblings.insert(destination, id);
        let mut changes = Vec::new();
        for (index, sibling) in siblings.into_iter().enumerate() {
            let order = OrderKey(OrderKey::FIRST.0 + index as u64 * OrderKey::STRIDE);
            if let Some(page) = self.pages.iter_mut().find(|page| page.id == sibling) {
                if page.order != order {
                    page.order = order;
                    changes.push(Change::PageMoved {
                        id: sibling,
                        parent: page.parent,
                        order,
                    });
                }
            }
        }
        self.pages.sort_by_key(|page| (page.parent, page.order));
        if changes.is_empty() {
            return false;
        }
        self.persistence.record(changes);
        true
    }
    pub fn delete_page(&mut self, id: PageId) -> bool {
        let Some(target) = self
            .pages
            .iter()
            .find(|page| page.id == id && !page.template)
            .cloned()
        else {
            return false;
        };
        let mut subtree = vec![id];
        let mut cursor = 0;
        while cursor < subtree.len() {
            let parent = subtree[cursor];
            for child in self
                .pages
                .iter()
                .filter(|page| page.parent == Some(parent) && !page.template)
            {
                if !subtree.contains(&child.id) {
                    subtree.push(child.id);
                }
            }
            cursor += 1;
        }
        let remaining = self
            .pages
            .iter()
            .filter(|page| !page.template && !subtree.contains(&page.id))
            .count();
        if remaining == 0 {
            return false;
        }

        self.end_typing();
        self.persistence.record(vec![Change::PageDeleted { id }]);
        for page_id in &subtree {
            self.document.drop_page(*page_id);
        }
        self.pages.retain(|page| !subtree.contains(&page.id));

        if subtree.contains(&self.page.id) {
            self.page = target
                .parent
                .filter(|parent| !subtree.contains(parent))
                .and_then(|parent| self.pages.iter().find(|page| page.id == parent).cloned())
                .or_else(|| {
                    self.pages
                        .iter()
                        .filter(|page| !page.template)
                        .min_by_key(|page| page.order)
                        .cloned()
                })
                .expect("at least one non-template page remains");
            self.ensure_page_has_block();
        }
        true
    }
    fn ensure_page_has_block(&mut self) {
        if self.blocks().is_empty() && !self.page.locked {
            let command = Command::AppendBlock {
                kind: BlockKind::Paragraph,
                text: String::new(),
            };
            if let Some(entry) = command::plan(&mut self.document, self.page.id, command) {
                self.document.apply(&entry.apply);
                self.persistence.record(entry.apply);
            }
        }
    }
    pub fn editable(&self, id: BlockId) -> bool {
        !self.page.locked
            && self.document.block(id).is_some_and(|b| {
                let simple_kind = matches!(
                    b.kind,
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
                );
                let list_child = b.parent.is_some_and(|parent| {
                    matches!(
                        b.kind,
                        BlockKind::Bullet | BlockKind::Numbered | BlockKind::Todo
                    ) && self.document.block(parent).is_some_and(|parent| {
                        matches!(
                            parent.kind,
                            BlockKind::Bullet | BlockKind::Numbered | BlockKind::Todo
                        )
                    })
                });
                simple_kind && (b.parent.is_none() || list_child)
            })
    }
    pub fn end_typing(&mut self) {
        if let Some((_, entry, _)) = self.typing.take() {
            self.history.push(self.page.id, entry);
        }
    }
    pub fn replace(&mut self, id: BlockId, text: String, boundary: bool) {
        if !self.editable(id) {
            return;
        }
        let now = Instant::now();
        if boundary
            || self.typing.as_ref().is_some_and(|(other, _, last)| {
                *other != id || now.duration_since(*last) > Duration::from_millis(600)
            })
        {
            self.end_typing();
        }
        let Some(mut entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::ReplaceText {
                id,
                text: text.clone(),
            },
        ) else {
            return;
        };
        if let Some(previous) = self.document.block(id) {
            let marks = adjust_marks(&previous.text, &text, &previous.marks);
            if marks != previous.marks {
                entry.apply.push(Change::BlockMarksSet { id, marks });
                entry.revert.insert(
                    0,
                    Change::BlockMarksSet {
                        id,
                        marks: previous.marks.clone(),
                    },
                );
            }
        }
        self.document.apply(&entry.apply);
        self.persistence.record(entry.apply.clone());
        if let Some((_, active, last)) = &mut self.typing {
            active.apply = entry.apply;
            *last = now;
        } else {
            self.typing = Some((id, entry, now));
        }
        if boundary {
            self.end_typing();
        }
    }
    pub fn convert_prefix(&mut self, id: BlockId, text: String, kind: BlockKind) {
        self.replace(id, text, false);
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SetBlockType { id, kind },
        ) else {
            return;
        };
        self.document.apply(&entry.apply);
        self.persistence.record(entry.apply.clone());
        if let Some((typing_id, typing, _)) = &mut self.typing
            && *typing_id == id
        {
            typing.apply.extend(entry.apply);
            let mut revert = entry.revert;
            revert.append(&mut typing.revert);
            typing.revert = revert;
        } else {
            self.history.push(self.page.id, entry);
        }
    }
    pub fn execute(&mut self, command: Command) {
        if self.page.locked {
            return;
        }
        self.end_typing();
        match command {
            Command::SplitBlock { id, caret } => {
                self.split(id, caret);
                return;
            }
            Command::MergeBackward { id } => {
                self.merge(id);
                return;
            }
            command => {
                if let Some(changes) =
                    command::exec(&mut self.document, &mut self.history, self.page.id, command)
                {
                    self.persistence.record(changes);
                }
            }
        }
    }
    fn apply_entry(&mut self, entry: Entry) {
        self.document.apply(&entry.apply);
        self.persistence.record(entry.apply.clone());
        self.history.push(self.page.id, entry);
    }
    fn split(&mut self, id: BlockId, caret: usize) {
        let Some(old) = self.document.block(id).cloned() else {
            return;
        };
        let Some(mut entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SplitBlock { id, caret },
        ) else {
            return;
        };
        let split_at = snap_left(&old.text, caret);
        let (left, right) = split_marks(&old.marks, split_at);
        let Some(inserted) = entry.apply.iter_mut().find_map(|change| match change {
            Change::BlockInserted(block) => Some(block),
            _ => None,
        }) else {
            return;
        };
        inserted.marks = right;
        entry.apply.push(Change::BlockMarksSet { id, marks: left });
        entry.revert.push(Change::BlockMarksSet {
            id,
            marks: old.marks,
        });
        self.apply_entry(entry);
    }
    fn merge(&mut self, id: BlockId) {
        let Some(index) = self.document.index_of(self.page.id, id) else {
            return;
        };
        let Some(current) = self.document.page_blocks(self.page.id).get(index).cloned() else {
            return;
        };
        let previous = index
            .checked_sub(1)
            .and_then(|i| self.document.page_blocks(self.page.id).get(i))
            .cloned();
        let Some(mut entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::MergeBackward { id },
        ) else {
            return;
        };
        if let Some(previous) = previous {
            let mut marks = previous.marks.clone();
            marks.extend(current.marks.iter().cloned().map(|mut mark| {
                mark.start += previous.text.len();
                mark.end += previous.text.len();
                mark
            }));
            entry.apply.push(Change::BlockMarksSet {
                id: previous.id,
                marks,
            });
            entry.revert.push(Change::BlockMarksSet {
                id: previous.id,
                marks: previous.marks,
            });
        }
        self.apply_entry(entry);
    }
    pub fn undo(&mut self) {
        self.end_typing();
        if let Some(changes) = command::undo(&mut self.document, &mut self.history, self.page.id) {
            self.persistence.record(changes);
        }
    }
    pub fn redo(&mut self) {
        self.end_typing();
        if let Some(changes) = command::redo(&mut self.document, &mut self.history, self.page.id) {
            self.persistence.record(changes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_typing_split_merge_undo_and_reopen_use_real_core_and_sqlite() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-roundtrip");
        let path = scratch.path().join("test.db");
        {
            let mut s = Session::open(&path)?;
            let id = s.blocks()[0].id;
            s.replace(id, "中文🦀".into(), false);
            s.replace(id, "中文🦀第一段".into(), false);
            s.undo();
            assert_eq!(s.blocks()[0].text, "");
            s.redo();
            assert_eq!(s.blocks()[0].text, "中文🦀第一段");
            s.execute(Command::SplitBlock {
                id,
                caret: "中文🦀".len(),
            });
            assert_eq!(s.blocks().len(), 2);
            let second = s.blocks()[1].id;
            s.execute(Command::MergeBackward { id: second });
            assert_eq!(s.blocks()[0].text, "中文🦀第一段");
            s.undo();
            assert_eq!(s.blocks().len(), 2);
            s.persistence.force_flush()?;
        }
        let s = Session::open(&path)?;
        assert_eq!(s.blocks()[0].text, "中文🦀");
        assert_eq!(s.blocks()[1].text, "第一段");
        assert_ne!(s.blocks()[0].id, s.blocks()[1].id);
        Ok(())
    }
}
