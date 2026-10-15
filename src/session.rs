//! Core-only session. UI previews and IME text do not enter this module.
use anyhow::{Context, Result, bail};
use quire_core::{
    core::{
        command::{self, Command},
        database::{
            DatabaseCatalog, DatabaseDraft, DatabaseId, PropertyId, RecordId, View, ViewId,
            ViewLayout,
        },
        document::{Document, Entry},
        history::History,
        organizer::{
            self, ListId, Note, NoteId, OrganizerCatalog, Priority, Repeat, Subtask, Task, TaskId,
            TaskList,
        },
        persistence::{Change, Repository},
        types::{
            Attachment, AttachmentId, Block, BlockId, BlockKind, ColorKind, Mark, ORGANIZER_STACK,
            OrderKey, Page, PageFont, PageId,
        },
    },
    services::attachment_store::AttachmentStore,
    services::persistence::PersistenceService,
    storage::{SqliteRepository, backup::OpenReport},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingOrganizerDelete {
    pub token: u64,
    pub ids: Vec<u64>,
    pub is_task: bool,
    pub message: String,
}

pub struct Session {
    pub path: PathBuf,
    pub startup_notice: Option<String>,
    pub pages: Vec<Page>,
    settings: BTreeMap<String, String>,
    pub page: Page,
    pub document: Document,
    pub persistence: Arc<PersistenceService>,
    pub repo: Arc<SqliteRepository>,
    pub attachments: BTreeMap<u64, Attachment>,
    pub organizer: OrganizerCatalog,
    pub databases: DatabaseCatalog,
    attachment_store: AttachmentStore,
    next_attachment_id: u64,
    history: History,
    typing: Option<(BlockId, Entry, Instant)>,
    pending_organizer_delete: Option<PendingOrganizerDelete>,
    next_organizer_delete_token: u64,
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

fn page_depth(
    page: &quire_core::services::sync::model::SPage,
    pages: &[quire_core::services::sync::model::SPage],
) -> usize {
    let mut depth = 0;
    let mut parent = page.parent;
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = parent {
        if !seen.insert(id) {
            break;
        }
        depth += 1;
        parent = pages
            .iter()
            .find(|candidate| candidate.id == id)
            .and_then(|candidate| candidate.parent);
    }
    depth
}

fn block_depth(
    block: &quire_core::services::sync::model::SBlock,
    blocks: &[quire_core::services::sync::model::SBlock],
) -> usize {
    let mut depth = 0;
    let mut parent = block.parent;
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = parent {
        if !seen.insert(id) {
            break;
        }
        depth += 1;
        parent = blocks
            .iter()
            .find(|candidate| candidate.id == id)
            .and_then(|candidate| candidate.parent);
    }
    depth
}

fn normalize_page_tree(pages: &mut [quire_core::services::sync::model::SPage]) {
    let original = pages.to_vec();
    for page in pages {
        let mut parent = page.parent;
        let mut seen = std::collections::HashSet::new();
        seen.insert(page.id);
        let mut valid = true;
        while let Some(id) = parent {
            if !seen.insert(id) {
                valid = false;
                break;
            }
            parent = original
                .iter()
                .find(|candidate| candidate.id == id)
                .and_then(|candidate| candidate.parent);
            if !original.iter().any(|candidate| candidate.id == id) {
                valid = false;
                break;
            }
        }
        if !valid {
            page.parent = None;
        }
    }
}

fn normalize_block_tree(blocks: &mut [quire_core::services::sync::model::SBlock]) {
    let original = blocks.to_vec();
    for block in blocks {
        let mut parent = block.parent;
        let mut seen = std::collections::HashSet::new();
        seen.insert(block.id);
        let mut valid = true;
        while let Some(id) = parent {
            if !seen.insert(id) {
                valid = false;
                break;
            }
            parent = original
                .iter()
                .find(|candidate| candidate.id == id)
                .and_then(|candidate| candidate.parent);
            if !original.iter().any(|candidate| candidate.id == id) {
                valid = false;
                break;
            }
        }
        if !valid {
            block.parent = None;
        }
    }
}

fn sync_block_diff(
    before: &quire_core::services::sync::model::SBlock,
    after: &quire_core::services::sync::model::SBlock,
    changes: &mut Vec<quire_core::core::persistence::Change>,
) {
    use quire_core::core::{
        persistence::Change,
        types::{BlockId, PageId},
    };
    let id = BlockId(after.id);
    if before.page != after.page {
        changes.push(Change::BlockMovedToPage {
            id,
            page: PageId(after.page),
            parent: after.parent.map(BlockId),
            order: quire_core::core::OrderKey(after.ord),
        });
    } else if before.parent != after.parent || before.ord != after.ord {
        changes.push(Change::BlockMoved {
            id,
            parent: after.parent.map(BlockId),
            order: quire_core::core::OrderKey(after.ord),
        });
    }
    if before.text != after.text {
        changes.push(Change::BlockTextSet {
            id,
            text: after.text.clone(),
        });
    }
    if before.kind != after.kind {
        changes.push(Change::BlockKindSet {
            id,
            kind: after.to_core().kind,
        });
    }
    if before.checked != after.checked {
        changes.push(Change::BlockCheckedSet {
            id,
            checked: after.checked,
        });
    }
    if before.folded != after.folded {
        changes.push(Change::BlockFoldedSet {
            id,
            folded: after.folded,
        });
    }
    if before.marks != after.marks {
        changes.push(Change::BlockMarksSet {
            id,
            marks: after.to_core().marks,
        });
    }
    if before.color != after.color || before.background != after.background {
        changes.push(Change::BlockColorSet {
            id,
            color: after.to_core().color,
            background: after.to_core().background,
        });
    }
    if before.page_ref != after.page_ref {
        changes.push(Change::BlockRefSet {
            id,
            page: after.page_ref.map(PageId),
        });
    }
    if before.sync_ref != after.sync_ref {
        changes.push(Change::BlockSyncSet {
            id,
            source: after.sync_ref.map(BlockId),
        });
    }
    if before.attachment != after.attachment {
        changes.push(Change::BlockAttachmentSet {
            id,
            attachment: after.attachment.map(quire_core::core::types::AttachmentId),
        });
    }
    if before.img_percent != after.img_percent {
        changes.push(Change::BlockImageWidthSet {
            id,
            percent: after.img_percent,
        });
    }
    if before.columns != after.columns {
        changes.push(Change::BlockColumnsSet {
            id,
            columns: after.columns,
        });
    }
    if before.lang != after.lang {
        changes.push(Change::BlockLangSet {
            id,
            lang: after.to_core().lang,
        });
    }
    if before.db_ref != after.db_ref {
        changes.push(Change::BlockDbRefSet {
            id,
            db: after.db_ref.map(quire_core::core::database::DatabaseId),
        });
    }
}

fn sync_database_changes(
    local: &[quire_core::services::sync::model::SDatabase],
    merged: &[quire_core::services::sync::model::SDatabase],
    changes: &mut Vec<quire_core::core::persistence::Change>,
) {
    use quire_core::core::{
        database::{CellValue, DatabaseId, PropertyId, RecordId, ViewId},
        persistence::Change,
    };
    for db in merged {
        let db_id = DatabaseId(db.id);
        let Some(before_db) = local.iter().find(|row| row.id == db.id) else {
            changes.push(Change::DatabaseCreated(db.to_core_entity()));
            changes.extend(
                db.properties
                    .iter()
                    .map(|property| Change::PropertyAdded(property.to_core())),
            );
            changes.extend(
                db.views
                    .iter()
                    .map(|view| Change::ViewAdded(view.to_core())),
            );
            continue;
        };
        if before_db.name != db.name {
            changes.push(Change::DatabaseRenamed {
                id: db_id,
                name: db.name.clone(),
            });
        }
        if before_db.template != db.template {
            changes.push(Change::DatabaseTemplateSet {
                id: db_id,
                template: db.template.clone(),
            });
        }
        for property in &db.properties {
            match before_db
                .properties
                .iter()
                .find(|row| row.id == property.id)
            {
                None => changes.push(Change::PropertyAdded(property.to_core())),
                Some(before) => {
                    let id = PropertyId(property.id);
                    if before.name != property.name {
                        changes.push(Change::PropertyRenamed {
                            id,
                            name: property.name.clone(),
                        });
                    }
                    if before.kind != property.kind {
                        changes.push(Change::PropertyKindSet {
                            id,
                            kind: property.to_core().kind,
                        });
                    }
                    if before.config != property.config {
                        changes.push(Change::PropertyConfigSet {
                            id,
                            config: property.config.clone(),
                        });
                    }
                    if before.ord != property.ord {
                        changes.push(Change::PropertyOrdSet {
                            id,
                            ord: quire_core::core::OrderKey(property.ord),
                        });
                    }
                }
            }
        }
        for view in &db.views {
            match before_db.views.iter().find(|row| row.id == view.id) {
                None => changes.push(Change::ViewAdded(view.to_core())),
                Some(before) => {
                    let id = ViewId(view.id);
                    if before.name != view.name {
                        changes.push(Change::ViewRenamed {
                            id,
                            name: view.name.clone(),
                        });
                    }
                    if before.layout != view.layout {
                        changes.push(Change::ViewLayoutSet {
                            id,
                            layout: view.to_core().layout,
                        });
                    }
                    if before.definition != view.definition {
                        changes.push(Change::ViewDefinitionSet {
                            id,
                            definition: view.definition.clone(),
                        });
                    }
                    if before.ord != view.ord {
                        changes.push(Change::ViewOrdSet {
                            id,
                            ord: quire_core::core::OrderKey(view.ord),
                        });
                    }
                }
            }
        }
    }
    let local_records: Vec<_> = local.iter().flat_map(|db| db.records.iter()).collect();
    let local_values: std::collections::HashMap<_, _> = local
        .iter()
        .flat_map(|db| {
            db.values
                .iter()
                .map(|value| ((value.record, value.property), value))
        })
        .collect();
    let merged_record_ids: std::collections::HashSet<_> = merged
        .iter()
        .flat_map(|db| db.records.iter().map(|row| row.id))
        .collect();
    let merged_value_ids: std::collections::HashSet<_> = merged
        .iter()
        .flat_map(|db| db.values.iter().map(|value| (value.record, value.property)))
        .collect();
    for db in merged {
        for record in &db.records {
            let id = RecordId(record.id);
            match local_records.iter().find(|row| row.id == record.id) {
                None => changes.push(Change::RecordCreated(record.to_core_record())),
                Some(before) => {
                    if before.ord != record.ord {
                        changes.push(Change::RecordOrdSet {
                            id,
                            ord: quire_core::core::OrderKey(record.ord),
                        });
                    }
                    if before.page != record.page {
                        changes.push(Change::RecordPageSet {
                            id,
                            page: record.page.map(quire_core::core::types::PageId),
                        });
                    }
                }
            }
        }
        for value in &db.values {
            if local_values
                .get(&(value.record, value.property))
                .is_none_or(|before| *before != value)
            {
                changes.push(Change::CellSet {
                    record: RecordId(value.record),
                    property: PropertyId(value.property),
                    value: value.to_core(),
                });
            }
        }
    }
    for (key, _) in &local_values {
        if !merged_value_ids.contains(key) {
            changes.push(Change::CellSet {
                record: RecordId(key.0),
                property: PropertyId(key.1),
                value: CellValue::Empty,
            });
        }
    }
    for record in &local_records {
        if !merged_record_ids.contains(&record.id) {
            changes.push(Change::RecordDeleted {
                id: RecordId(record.id),
            });
        }
    }
    for db in local {
        for property in &db.properties {
            if !merged.iter().any(|row| {
                row.properties
                    .iter()
                    .any(|candidate| candidate.id == property.id)
            }) {
                changes.push(Change::PropertyDeleted {
                    id: PropertyId(property.id),
                });
            }
        }
        for view in &db.views {
            if !merged
                .iter()
                .any(|row| row.views.iter().any(|candidate| candidate.id == view.id))
            {
                changes.push(Change::ViewDeleted {
                    id: ViewId(view.id),
                });
            }
        }
    }
    let merged_db_ids: std::collections::HashSet<_> = merged.iter().map(|db| db.id).collect();
    for db in local {
        if !merged_db_ids.contains(&db.id) {
            changes.push(Change::DatabaseDeleted {
                id: DatabaseId(db.id),
            });
        }
    }
}

fn sync_organizer_changes(
    local: &quire_core::services::sync::model::SyncSnapshot,
    merged: &quire_core::services::sync::model::SyncSnapshot,
    changes: &mut Vec<quire_core::core::persistence::Change>,
) {
    use quire_core::core::{
        organizer::{ListId, NoteId, TaskId},
        persistence::Change,
    };
    for note in &merged.notes {
        let row = note.to_core();
        match local.notes.iter().find(|before| before.id == note.id) {
            None => changes.push(Change::NoteAdded(row)),
            Some(before) if before != note => changes.push(Change::NoteUpdated(row)),
            _ => {}
        }
    }
    let notes: std::collections::HashSet<_> = merged.notes.iter().map(|row| row.id).collect();
    for note in &local.notes {
        if !notes.contains(&note.id) {
            changes.push(Change::NoteDeleted {
                id: NoteId(note.id),
            });
        }
    }
    for list in &merged.lists {
        let row = list.to_core();
        match local.lists.iter().find(|before| before.id == list.id) {
            None => changes.push(Change::TaskListAdded(row)),
            Some(before) if before != list => changes.push(Change::TaskListUpdated(row)),
            _ => {}
        }
    }
    for task in &merged.tasks {
        let row = task.to_core();
        match local.tasks.iter().find(|before| before.id == task.id) {
            None => changes.push(Change::TaskAdded(row)),
            Some(before) if before != task => changes.push(Change::TaskUpdated(row)),
            _ => {}
        }
    }
    let tasks: std::collections::HashSet<_> = merged.tasks.iter().map(|row| row.id).collect();
    for task in &local.tasks {
        if !tasks.contains(&task.id) {
            changes.push(Change::TaskDeleted {
                id: TaskId(task.id),
            });
        }
    }
    let lists: std::collections::HashSet<_> = merged.lists.iter().map(|row| row.id).collect();
    for list in &local.lists {
        if !lists.contains(&list.id) {
            changes.push(Change::TaskListDeleted {
                id: ListId(list.id),
            });
        }
    }
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
        let (repo, open_report) = SqliteRepository::open_with_report(path)
            .context("无法打开库；不会覆盖或重建已有文件")?;
        let repo = Arc::new(repo);
        let loaded = repo.load()?;
        let organizer = repo.load_organizer()?;
        let databases = repo.load_databases()?;
        let attachments = repo
            .load_attachments()?
            .into_iter()
            .map(|attachment| (attachment.id.as_u64(), attachment))
            .collect::<BTreeMap<_, _>>();
        let next_attachment_id = attachments
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(1);
        let attachment_store = AttachmentStore::for_db(Some(path));
        let mut pages = loaded.pages.clone();
        pages.sort_by_key(|p| p.order);
        let selected = pages
            .iter()
            .find(|p| !p.template && p.parent.is_none())
            .cloned()
            .or_else(|| pages.iter().find(|p| !p.template).cloned());
        let create_initial_page = selected.is_none();
        let page = selected.unwrap_or(Page {
            id: PageId(pages.iter().map(|page| page.id.0).max().unwrap_or(0) + 1),
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
        if create_initial_page {
            // The synthetic first page is persisted below; it also belongs in
            // the live workspace tree so later imports and page creation do
            // not reuse its id or leave it out of navigation.
            pages.push(page.clone());
        }
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
        if create_initial_page {
            persistence.record(vec![Change::PageCreated(page.clone())]);
        }
        let mut session = Self {
            path: path.into(),
            startup_notice: open_report_notice(&open_report, repo.path().unwrap_or(path)),
            pages,
            settings: loaded.settings,
            page,
            document,
            persistence,
            repo,
            attachments,
            organizer,
            databases,
            attachment_store,
            next_attachment_id,
            history: History::default(),
            typing: None,
            pending_organizer_delete: None,
            next_organizer_delete_token: 0,
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
        session.seed_builtin_templates();
        // Startup runs before the event loop, not in render or a keystroke handler.
        session.persistence.force_flush()?;
        Ok(session)
    }

    fn seed_builtin_templates(&mut self) {
        const SEEDED_KEY: &str = "gpui.templates-seeded-v1";
        if self.setting(SEEDED_KEY) == Some("true") {
            return;
        }
        let mut next_page = self
            .pages
            .iter()
            .map(|page| page.id.0)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut next_order = self
            .pages
            .iter()
            .filter(|page| page.template)
            .map(|page| page.order)
            .max();
        let mut changes = Vec::new();
        for preset in quire_core::core::template::PRESETS {
            let order = OrderKey::between(next_order, None).unwrap_or(OrderKey::FIRST);
            next_order = Some(order);
            let page = Page {
                id: PageId(next_page),
                title: preset.name.into(),
                parent: None,
                order,
                favorite: false,
                expanded: false,
                font: PageFont::Default,
                full_width: false,
                small_text: false,
                icon: String::new(),
                cover: None,
                locked: false,
                template: true,
            };
            next_page = next_page.saturating_add(1);
            let imported = {
                let mut allocate = || self.document.alloc_block_id();
                quire_core::services::import_service::import_markdown(
                    preset.markdown,
                    &page,
                    &mut allocate,
                )
            };
            self.document.apply(&imported);
            changes.extend(imported);
            self.pages.push(page);
        }
        self.pages.sort_by_key(|page| (page.parent, page.order));
        self.settings.insert(SEEDED_KEY.into(), "true".into());
        changes.push(Change::SettingSet {
            key: SEEDED_KEY.into(),
            value: "true".into(),
        });
        self.persistence.record(changes);
    }
    pub fn blocks(&self) -> &[Block] {
        self.document.page_blocks(self.page.id)
    }
    pub fn make_database(&mut self, block: BlockId, name: String) -> Option<DatabaseId> {
        if self.page.locked {
            return None;
        }
        let db = DatabaseId(
            self.repo
                .max_id(quire_core::storage::database_store::DbTable::Databases)
                .ok()?
                .saturating_add(1),
        );
        let property = PropertyId(
            self.repo
                .max_id(quire_core::storage::database_store::DbTable::Properties)
                .ok()?
                .saturating_add(1),
        );
        let view = ViewId(
            self.repo
                .max_id(quire_core::storage::database_store::DbTable::Views)
                .ok()?
                .saturating_add(1),
        );
        let draft = DatabaseDraft::new(db, property, view, name);
        let database = draft.database.clone();
        let title = draft.title.clone();
        let first_view = draft.view.clone();
        let entry = command::plan(
            &mut self.document,
            self.page.id,
            Command::MakeDatabase { id: block, draft },
        )?;
        self.apply_entry(entry);
        self.databases.databases.push(database);
        self.databases.properties.push(title);
        self.databases.views.push(first_view);
        let _ = self.persistence.force_flush();
        Some(db)
    }
    pub fn add_database_record(
        &mut self,
        block: BlockId,
    ) -> Option<quire_core::core::database::RecordId> {
        if self.page.locked {
            return None;
        }
        let db = self.document.block(block)?.db_ref?;
        let id = quire_core::core::database::RecordId(
            self.repo
                .max_id(quire_core::storage::database_store::DbTable::Records)
                .ok()?
                .saturating_add(1),
        );
        let last = self.repo.last_record_ord(db).ok().flatten();
        let ord = OrderKey::between(last, None)?;
        let entry = command::plan(
            &mut self.document,
            self.page.id,
            Command::AddDatabaseRecord {
                block,
                record: id,
                ord,
            },
        )?;
        self.apply_entry(entry);
        let _ = self.persistence.force_flush();
        Some(id)
    }
    pub fn delete_database_record(&mut self, block: BlockId, record: RecordId) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(db) = self.document.block(block).and_then(|block| block.db_ref) else {
            return Ok(false);
        };
        let Some(row) = self.repo.record(record)? else {
            return Ok(false);
        };
        if row.db != db {
            return Ok(false);
        }
        if row.page.is_some() {
            bail!("该记录已关联页面，请从侧栏删除页面");
        }
        let values = self.repo.record_values(record)?;
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::DeleteDatabaseRecord {
                block,
                record: row,
                values,
                page: None,
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        self.persistence.force_flush()?;
        Ok(true)
    }
    pub fn add_database_property(
        &mut self,
        block: BlockId,
        name: String,
        kind: quire_core::core::database::PropertyKind,
    ) -> Option<PropertyId> {
        if self.page.locked {
            return None;
        }
        let db = self.document.block(block)?.db_ref?;
        let id = PropertyId(
            self.repo
                .max_id(quire_core::storage::database_store::DbTable::Properties)
                .ok()?
                .saturating_add(1),
        );
        let last = self
            .databases
            .properties_of(db)
            .map(|property| property.ord)
            .max();
        let ord = OrderKey::between(last, None)?;
        let property = quire_core::core::database::Property {
            id,
            db,
            name,
            kind,
            config: String::new(),
            ord,
        };
        let entry = command::plan(
            &mut self.document,
            self.page.id,
            Command::AddDatabaseProperty {
                block,
                property: property.clone(),
            },
        )?;
        self.apply_entry(entry);
        self.databases.properties.push(property);
        let _ = self.persistence.force_flush();
        Some(id)
    }
    pub fn rename_database_property(
        &mut self,
        block: BlockId,
        property: PropertyId,
        name: &str,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(db) = self.document.block(block).and_then(|block| block.db_ref) else {
            return Ok(false);
        };
        let Some(current) = self
            .databases
            .properties
            .iter()
            .find(|candidate| candidate.id == property && candidate.db == db)
            .cloned()
        else {
            return Ok(false);
        };
        let name = name.trim();
        if name.is_empty() {
            bail!("属性名称不能为空");
        }
        if current.name == name {
            return Ok(false);
        }
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::RenameDatabaseProperty {
                block,
                property,
                from: current.name,
                to: name.to_owned(),
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        if let Some(property) = self
            .databases
            .properties
            .iter_mut()
            .find(|candidate| candidate.id == property)
        {
            property.name = name.to_owned();
        }
        self.persistence.force_flush()?;
        Ok(true)
    }
    pub fn set_database_property_kind(
        &mut self,
        block: BlockId,
        property: PropertyId,
        kind: quire_core::core::database::PropertyKind,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(db) = self.document.block(block).and_then(|block| block.db_ref) else {
            return Ok(false);
        };
        let Some(current) = self
            .databases
            .properties
            .iter()
            .find(|candidate| candidate.id == property && candidate.db == db)
            .cloned()
        else {
            bail!("该列不属于此数据库");
        };
        if current.kind.is_title() {
            bail!("数据库的标题列决定每一行的名称");
        }
        if current.kind == kind {
            return Ok(false);
        }
        if current.kind == quire_core::core::database::PropertyKind::Relation {
            let relation = quire_core::core::database_relation::config_relation(&current.config);
            if relation.mirror.is_some()
                || self.databases.properties.iter().any(|candidate| {
                    candidate.kind == quire_core::core::database::PropertyKind::Relation
                        && quire_core::core::database_relation::config_relation(&candidate.config)
                            .mirror
                            == Some(property)
                })
            {
                bail!("此列被双向关系引用，请先清除反向指针");
            }
        }
        if self.databases.properties.iter().any(|candidate| {
            if candidate.kind != quire_core::core::database::PropertyKind::Rollup {
                return false;
            }
            let config = quire_core::core::database_rollup::config_rollup(&candidate.config);
            config.relation == Some(property) || config.column == Some(property)
        }) {
            bail!("有汇总列通过此列计算，请先移除该引用");
        }
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SetDatabasePropertyKind {
                block,
                property,
                from: current.kind,
                to: kind,
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        if let Some(property) = self
            .databases
            .properties
            .iter_mut()
            .find(|candidate| candidate.id == property)
        {
            property.kind = kind;
        }
        self.persistence.force_flush()?;
        Ok(true)
    }
    pub fn set_database_property_format(
        &mut self,
        block: BlockId,
        property: PropertyId,
        format: &str,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(current) = self.session_database_property(block, property).cloned() else {
            return Ok(false);
        };
        if !matches!(
            (current.kind, format),
            (
                quire_core::core::database::PropertyKind::Number,
                "plain" | "integer" | "percent"
            ) | (
                quire_core::core::database::PropertyKind::Date,
                "date" | "datetime"
            ) | (
                quire_core::core::database::PropertyKind::CreatedTime
                    | quire_core::core::database::PropertyKind::LastEditedTime,
                "date" | "datetime"
            )
        ) {
            bail!("该格式不适用于此列类型");
        }
        let mut config = quire_core::core::database_property::json::Json::parse(&current.config)
            .unwrap_or_else(
                |_| quire_core::core::database_property::json::Json::Object(Vec::new()),
            );
        let fields = match &mut config {
            quire_core::core::database_property::json::Json::Object(fields) => fields,
            _ => bail!("列配置不是对象"),
        };
        let value = quire_core::core::database_property::json::Json::Text(format.to_owned());
        if let Some((_, old)) = fields.iter_mut().find(|(key, _)| key == "format") {
            if *old == value {
                return Ok(false);
            }
            *old = value;
        } else {
            fields.push(("format".into(), value));
        }
        let config = config.to_text();
        self.apply_entry(Entry {
            apply: vec![Change::PropertyConfigSet {
                id: property,
                config: config.clone(),
            }],
            revert: vec![Change::PropertyConfigSet {
                id: property,
                config: current.config,
            }],
        });
        if let Some(definition) = self
            .databases
            .properties
            .iter_mut()
            .find(|definition| definition.id == property)
        {
            definition.config = config;
        }
        self.persistence.force_flush()?;
        Ok(true)
    }
    pub fn set_database_formula(
        &mut self,
        block: BlockId,
        property: PropertyId,
        expression: &str,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(db) = self.document.block(block).and_then(|block| block.db_ref) else {
            return Ok(false);
        };
        let properties = self
            .databases
            .properties_of(db)
            .cloned()
            .collect::<Vec<_>>();
        let Some(current) = properties.iter().find(|candidate| candidate.id == property) else {
            bail!("该列不属于此数据库");
        };
        if current.kind != quire_core::core::database::PropertyKind::Formula {
            bail!("公式只能配置在公式列上");
        }
        let resolve = |name: &str| {
            properties
                .iter()
                .find(|candidate| candidate.name == name)
                .map(|candidate| candidate.id)
        };
        let program = if expression.trim().is_empty() {
            None
        } else {
            Some(
                quire_core::core::database_formula::Program::parse(expression, resolve)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?,
            )
        };
        if let Some(program) = &program {
            let would_cycle = quire_core::core::database_formula::would_cycle(
                property,
                program.deps(),
                |dependency| {
                    let dependency = properties
                        .iter()
                        .find(|candidate| candidate.id == dependency)?;
                    if dependency.kind != quire_core::core::database::PropertyKind::Formula {
                        return None;
                    }
                    let source =
                        quire_core::core::database_formula::config_formula(&dependency.config)?;
                    quire_core::core::database_formula::Program::parse(&source, resolve)
                        .ok()
                        .map(|program| program.deps().clone())
                },
            );
            if would_cycle {
                bail!("公式引用会形成循环");
            }
        }
        let from = current.config.clone();
        let to = quire_core::core::database_formula::config_set_formula(&from, expression);
        if from == to {
            return Ok(false);
        }
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SetDatabaseFormula {
                block,
                property,
                from,
                to: to.clone(),
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        if let Some(property) = self
            .databases
            .properties
            .iter_mut()
            .find(|candidate| candidate.id == property)
        {
            property.config = to;
        }
        self.persistence.force_flush()?;
        self.refresh_databases();
        Ok(true)
    }
    pub fn add_database_view(&mut self, block: BlockId) -> Option<ViewId> {
        if self.page.locked {
            return None;
        }
        let db = self.document.block(block)?.db_ref?;
        let id = ViewId(
            self.repo
                .max_id(quire_core::storage::database_store::DbTable::Views)
                .ok()?
                .saturating_add(1),
        );
        let last = self.databases.views_of(db).map(|view| view.ord).max()?;
        let ord = OrderKey::between(Some(last), None)?;
        let number = self.databases.views_of(db).count() + 1;
        let view = View {
            id,
            db,
            name: format!("表格 {number}"),
            layout: ViewLayout::Table,
            definition: String::new(),
            ord,
        };
        let entry = command::plan(
            &mut self.document,
            self.page.id,
            Command::AddDatabaseView {
                block,
                view: view.clone(),
            },
        )?;
        self.apply_entry(entry);
        self.databases.views.push(view);
        let _ = self.persistence.force_flush();
        Some(id)
    }
    pub fn rename_database_view(
        &mut self,
        block: BlockId,
        view_id: ViewId,
        name: &str,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(db) = self.document.block(block).and_then(|block| block.db_ref) else {
            return Ok(false);
        };
        let Some(current) = self
            .databases
            .views
            .iter()
            .find(|view| view.id == view_id && view.db == db)
            .cloned()
        else {
            bail!("该视图不属于此数据库");
        };
        let name = name.trim();
        if name.is_empty() {
            bail!("视图名称不能为空");
        }
        if current.name == name {
            return Ok(false);
        }
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::RenameDatabaseView {
                block,
                view: view_id,
                from: current.name,
                to: name.to_owned(),
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        if let Some(view) = self
            .databases
            .views
            .iter_mut()
            .find(|view| view.id == view_id)
        {
            view.name = name.to_owned();
        }
        self.persistence.force_flush()?;
        Ok(true)
    }
    pub fn set_database_view_definition(
        &mut self,
        block: BlockId,
        view_id: ViewId,
        definition: String,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(view) = self
            .databases
            .views
            .iter()
            .find(|view| view.id == view_id)
            .cloned()
        else {
            return Ok(false);
        };
        if view.definition == definition {
            return Ok(false);
        }
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SetDatabaseViewDefinition {
                block,
                view: view_id,
                from: view.definition,
                to: definition.clone(),
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        if let Some(view) = self
            .databases
            .views
            .iter_mut()
            .find(|view| view.id == view_id)
        {
            view.definition = definition;
        }
        Ok(true)
    }
    pub fn set_database_view_layout(
        &mut self,
        block: BlockId,
        view_id: ViewId,
        layout: ViewLayout,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(db) = self.document.block(block).and_then(|block| block.db_ref) else {
            return Ok(false);
        };
        let Some(current) = self
            .databases
            .views
            .iter()
            .find(|view| view.id == view_id && view.db == db)
            .cloned()
        else {
            bail!("该视图不属于此数据库");
        };
        if current.layout == layout {
            return Ok(false);
        }
        self.apply_entry(Entry {
            apply: vec![Change::ViewLayoutSet {
                id: view_id,
                layout,
            }],
            revert: vec![Change::ViewLayoutSet {
                id: view_id,
                layout: current.layout,
            }],
        });
        if let Some(view) = self
            .databases
            .views
            .iter_mut()
            .find(|view| view.id == view_id)
        {
            view.layout = layout;
        }
        self.persistence.force_flush()?;
        Ok(true)
    }
    pub fn add_database_property_option(
        &mut self,
        block: BlockId,
        property: PropertyId,
        name: &str,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(db) = self.document.block(block).and_then(|block| block.db_ref) else {
            return Ok(false);
        };
        let Some(current) = self
            .databases
            .properties
            .iter()
            .find(|candidate| candidate.id == property && candidate.db == db)
            .cloned()
        else {
            bail!("该列不属于此数据库");
        };
        if !matches!(
            current.kind,
            quire_core::core::database::PropertyKind::Select
                | quire_core::core::database::PropertyKind::Status
                | quire_core::core::database::PropertyKind::MultiSelect
        ) {
            bail!("此列类型不支持选项");
        }
        let name = name.trim();
        if name.is_empty() {
            return Ok(false);
        }
        let mut options =
            quire_core::core::database_property::PropertyOptions::from_config(&current.config);
        let before_len = options.len();
        options.option_named(name);
        if options.len() == before_len {
            return Ok(false);
        }
        let config = options.to_config();
        self.apply_entry(Entry {
            apply: vec![Change::PropertyConfigSet {
                id: property,
                config: config.clone(),
            }],
            revert: vec![Change::PropertyConfigSet {
                id: property,
                config: current.config,
            }],
        });
        if let Some(property) = self
            .databases
            .properties
            .iter_mut()
            .find(|candidate| candidate.id == property)
        {
            property.config = config;
        }
        self.persistence.force_flush()?;
        Ok(true)
    }
    pub fn set_database_property_option_cell(
        &mut self,
        block: BlockId,
        record: RecordId,
        property: PropertyId,
        option_id: Option<u64>,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(definition) = self.session_database_property(block, property).cloned() else {
            return Ok(false);
        };
        if !matches!(
            definition.kind,
            quire_core::core::database::PropertyKind::Select
                | quire_core::core::database::PropertyKind::Status
        ) {
            bail!("此列不是单选或状态类型");
        }
        let to = match option_id {
            Some(id) => {
                let options = quire_core::core::database_property::PropertyOptions::from_config(
                    &definition.config,
                );
                let option = options
                    .get(quire_core::core::database_property::OptionId(id))
                    .ok_or_else(|| anyhow::anyhow!("该选项不存在"))?;
                quire_core::core::database::CellValue::Text(option.id.to_string())
            }
            None => quire_core::core::database::CellValue::Empty,
        };
        let from = self.repo.cell(record, property)?;
        if from == to {
            return Ok(false);
        }
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SetDatabaseCell {
                block,
                record,
                property,
                from,
                to,
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        self.persistence.force_flush()?;
        self.refresh_databases();
        Ok(true)
    }
    pub fn set_database_relation_target(
        &mut self,
        block: BlockId,
        property: PropertyId,
        target: Option<DatabaseId>,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(current) = self.session_database_property(block, property).cloned() else {
            return Ok(false);
        };
        if current.kind != quire_core::core::database::PropertyKind::Relation {
            bail!("此列不是关联类型");
        }
        if let Some(target) = target.filter(|target| {
            !self
                .databases
                .databases
                .iter()
                .any(|database| database.id == *target)
        }) {
            bail!("目标数据库不存在：{target}");
        }
        let old = quire_core::core::database_relation::config_relation(&current.config);
        if old.mirror.is_some() {
            bail!("该关联含双向配置，不能从单向设置器修改");
        }
        if old.target.is_some() && old.target != target {
            bail!("关联目标配置后不可直接更换；请新建关联列");
        }
        let to =
            quire_core::core::database_relation::config_set_relation(&current.config, target, None);
        if current.config == to {
            return Ok(false);
        }
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SetRelationConfig {
                block,
                property,
                from: current.config.clone(),
                to: to.clone(),
                mirrors: Vec::new(),
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        if let Some(definition) = self
            .databases
            .properties
            .iter_mut()
            .find(|definition| definition.id == property)
        {
            definition.config = to;
        }
        self.persistence.force_flush()?;
        Ok(true)
    }
    pub fn set_database_relation_mirror(
        &mut self,
        block: BlockId,
        property: PropertyId,
        mirror: PropertyId,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let current = self
            .session_database_property(block, property)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("关联列不存在"))?;
        if current.kind != quire_core::core::database::PropertyKind::Relation {
            bail!("此列不是关联类型");
        }
        let forward = quire_core::core::database_relation::config_relation(&current.config);
        let target = forward
            .target
            .ok_or_else(|| anyhow::anyhow!("请先配置目标数据库"))?;
        let candidate = self
            .databases
            .properties
            .iter()
            .find(|p| p.id == mirror)
            .ok_or_else(|| anyhow::anyhow!("反向关联列不存在"))?;
        let back = quire_core::core::database_relation::config_relation(&candidate.config);
        let facts = quire_core::core::database_relation::ColumnFacts {
            kind: candidate.kind,
            db: candidate.db,
            target: back.target,
            mirror: back.mirror,
        };
        quire_core::core::database_relation::check_pair(
            property, current.db, target, mirror, &facts,
        )
        .map_err(|reason| anyhow::anyhow!(reason.message()))?;
        let from = current.config.clone();
        let to = quire_core::core::database_relation::config_set_relation(
            &from,
            Some(target),
            Some(mirror),
        );
        let mirror_to = quire_core::core::database_relation::config_set_relation(
            &candidate.config,
            Some(current.db),
            Some(property),
        );
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SetRelationConfig {
                block,
                property,
                from,
                to: to.clone(),
                mirrors: vec![(mirror, candidate.config.clone(), mirror_to.clone())],
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        for definition in &mut self.databases.properties {
            if definition.id == property {
                definition.config = to.clone();
            }
            if definition.id == mirror {
                definition.config = mirror_to.clone();
            }
        }
        self.persistence.force_flush()?;
        self.refresh_databases();
        Ok(true)
    }
    pub fn set_database_relation_cell(
        &mut self,
        block: BlockId,
        record: RecordId,
        property: PropertyId,
        targets: &[RecordId],
    ) -> Result<bool> {
        use quire_core::core::database::CellValue;
        if self.page.locked {
            return Ok(false);
        }
        let Some(definition) = self.session_database_property(block, property).cloned() else {
            return Ok(false);
        };
        if definition.kind != quire_core::core::database::PropertyKind::Relation {
            bail!("此列不是关联类型");
        }
        let config = quire_core::core::database_relation::config_relation(&definition.config);
        let target_db = config
            .target
            .ok_or_else(|| anyhow::anyhow!("请先配置目标数据库"))?;
        let source = self
            .repo
            .record(record)?
            .ok_or_else(|| anyhow::anyhow!("当前记录不存在"))?;
        if source.db != definition.db {
            bail!("当前记录不属于关联列所在数据库");
        }
        let mut ids = Vec::with_capacity(targets.len());
        for target in targets {
            let row = self
                .repo
                .record(*target)?
                .ok_or_else(|| anyhow::anyhow!("目标记录不存在"))?;
            if row.db != target_db {
                bail!("目标记录不属于所选数据库");
            }
            ids.push(target.0.to_string());
        }
        ids.sort();
        ids.dedup();
        let from = self.repo.cell(record, property)?;
        let to = if ids.is_empty() {
            CellValue::Empty
        } else {
            CellValue::Items(ids)
        };
        if from == to {
            return Ok(false);
        }
        let mut mirrors = Vec::new();
        if let Some(mirror) = config.mirror {
            let mirror_definition = self
                .databases
                .properties
                .iter()
                .find(|candidate| candidate.id == mirror)
                .ok_or_else(|| anyhow::anyhow!("反向关联列不存在"))?;
            let mut changed = std::collections::BTreeSet::new();
            if let CellValue::Items(old_ids) = &from {
                for id in old_ids {
                    if let Ok(id) = id.parse::<u64>() {
                        changed.insert(RecordId(id));
                    }
                }
            }
            changed.extend(targets.iter().copied());
            for target in changed {
                let before = self.repo.cell(target, mirror)?;
                let mut ids = match &before {
                    CellValue::Items(ids) => ids.clone(),
                    _ => Vec::new(),
                };
                let source_id = record.0.to_string();
                if targets.contains(&target) {
                    if !ids.contains(&source_id) {
                        ids.push(source_id);
                    }
                } else {
                    ids.retain(|id| id != &source_id);
                }
                ids.sort();
                ids.dedup();
                let after = if ids.is_empty() {
                    CellValue::Empty
                } else {
                    CellValue::Items(ids)
                };
                if before != after {
                    mirrors.push((target, mirror_definition.id, before, after));
                }
            }
        }
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SetRelation {
                block,
                record,
                property,
                from,
                to,
                mirrors,
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        self.persistence.force_flush()?;
        self.refresh_databases();
        Ok(true)
    }
    pub fn set_database_rollup(
        &mut self,
        block: BlockId,
        property: PropertyId,
        relation: Option<PropertyId>,
        column: Option<PropertyId>,
        aggregate: quire_core::core::database_rollup::Aggregate,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(current) = self.session_database_property(block, property).cloned() else {
            return Ok(false);
        };
        if current.kind != quire_core::core::database::PropertyKind::Rollup {
            bail!("此列不是汇总类型");
        }
        let relation_property = relation.and_then(|id| {
            self.databases
                .properties
                .iter()
                .find(|candidate| candidate.id == id)
        });
        if relation_property.is_some_and(|relation| relation.db != current.db) {
            bail!("汇总所用关联列必须属于当前数据库");
        }
        let relation_config = relation_property
            .map(|relation| quire_core::core::database_relation::config_relation(&relation.config));
        let target_db = relation_config.and_then(|config| config.target);
        let column_property = column.and_then(|id| {
            self.databases
                .properties
                .iter()
                .find(|candidate| candidate.id == id)
        });
        if aggregate.reads_column() && column.is_none() {
            bail!("此聚合方式需要选择目标列");
        }
        let facts = quire_core::core::database_rollup::RollupFacts {
            relation_kind: relation_property
                .map_or(quire_core::core::database::PropertyKind::Text, |property| {
                    property.kind
                }),
            relation_target: target_db,
            column_kind: column_property
                .map_or(quire_core::core::database::PropertyKind::Text, |property| {
                    property.kind
                }),
            column_is_in_target: match (target_db, column_property) {
                (Some(target), Some(property)) => property.db == target,
                (_, None) => !aggregate.reads_column(),
                _ => false,
            },
        };
        quire_core::core::database_rollup::check_config(&facts)
            .map_err(|error| anyhow::anyhow!(error.message()))?;
        let from = current.config.clone();
        let to = quire_core::core::database_rollup::config_set_rollup(
            &from, relation, column, aggregate,
        );
        if from == to {
            return Ok(false);
        }
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SetDatabaseRollup {
                block,
                property,
                from,
                to: to.clone(),
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        if let Some(definition) = self
            .databases
            .properties
            .iter_mut()
            .find(|definition| definition.id == property)
        {
            definition.config = to;
        }
        self.persistence.force_flush()?;
        Ok(true)
    }
    pub fn set_database_cell_many(
        &mut self,
        block: BlockId,
        record: RecordId,
        property: PropertyId,
        inputs: &[String],
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(definition) = self.session_database_property(block, property).cloned() else {
            return Ok(false);
        };
        let from = self.repo.cell(record, property)?;
        let to = quire_core::core::database_property::parse_many(
            definition.kind,
            &definition.config,
            inputs,
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        if from == to {
            return Ok(false);
        }
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SetDatabaseCell {
                block,
                record,
                property,
                from,
                to,
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        self.persistence.force_flush()?;
        self.refresh_databases();
        Ok(true)
    }
    pub fn import_database_file(
        &mut self,
        block: BlockId,
        record: RecordId,
        property: PropertyId,
        path: &Path,
    ) -> Result<bool> {
        self.import_database_files(block, record, property, &[path.to_path_buf()])
    }
    pub fn import_database_files(
        &mut self,
        block: BlockId,
        record: RecordId,
        property: PropertyId,
        paths: &[PathBuf],
    ) -> Result<bool> {
        use quire_core::core::database::{CellValue, PropertyKind};
        if self.page.locked {
            return Ok(false);
        }
        if paths.is_empty() {
            return Ok(false);
        }
        let definition = self
            .session_database_property(block, property)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("文件列不存在"))?;
        if definition.kind != PropertyKind::Files {
            bail!("此列不是文件类型");
        }
        let row = self
            .repo
            .record(record)?
            .ok_or_else(|| anyhow::anyhow!("数据库记录不存在"))?;
        if row.db != definition.db {
            bail!("记录不属于此文件列所在数据库");
        }
        let from = self.repo.cell(record, property)?;
        let mut ids = match &from {
            CellValue::Items(ids) => ids.clone(),
            _ => Vec::new(),
        };
        let mut imported = Vec::with_capacity(paths.len());
        for path in paths {
            let id = AttachmentId(self.next_attachment_id);
            self.next_attachment_id = self.next_attachment_id.saturating_add(1);
            let attachment = self
                .attachment_store
                .import_any_file(id, path)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            ids.push(id.as_u64().to_string());
            imported.push(attachment);
        }
        let to = quire_core::core::database_property::parse_many(
            PropertyKind::Files,
            &definition.config,
            &ids,
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let Some(mut entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SetDatabaseCell {
                block,
                record,
                property,
                from,
                to,
            },
        ) else {
            bail!("无法写入文件列");
        };
        for attachment in imported.into_iter().rev() {
            self.attachments
                .insert(attachment.id.as_u64(), attachment.clone());
            entry.apply.insert(0, Change::AttachmentAdded(attachment));
        }
        self.apply_entry(entry);
        self.persistence.force_flush()?;
        self.refresh_databases();
        Ok(true)
    }
    pub fn set_database_cell(
        &mut self,
        block: BlockId,
        record: RecordId,
        property: PropertyId,
        input: &str,
    ) -> Result<bool> {
        if self.page.locked {
            return Ok(false);
        }
        let Some(definition) = self.session_database_property(block, property).cloned() else {
            return Ok(false);
        };
        let from = self.repo.cell(record, property)?;
        let to = quire_core::core::database_property::parse_one(
            definition.kind,
            &definition.config,
            input,
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        if from == to {
            return Ok(false);
        }
        let Some(entry) = command::plan(
            &mut self.document,
            self.page.id,
            Command::SetDatabaseCell {
                block,
                record,
                property,
                from,
                to,
            },
        ) else {
            return Ok(false);
        };
        self.apply_entry(entry);
        self.persistence.force_flush()?;
        self.refresh_databases();
        Ok(true)
    }
    fn session_database_property(
        &self,
        block: BlockId,
        property: PropertyId,
    ) -> Option<&quire_core::core::database::Property> {
        let db = self.document.block(block)?.db_ref?;
        self.databases
            .properties_of(db)
            .find(|candidate| candidate.id == property)
    }
    pub fn refresh_databases(&mut self) {
        if let Ok(catalog) = self.repo.load_databases() {
            self.databases = catalog;
        }
    }
    pub fn import_attachment(
        &mut self,
        after: BlockId,
        path: &Path,
        image: bool,
    ) -> Result<BlockId> {
        let id = AttachmentId(self.next_attachment_id);
        self.next_attachment_id = self.next_attachment_id.saturating_add(1);
        let attachment = if image {
            self.attachment_store
                .import_file(id, path)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?
        } else {
            self.attachment_store
                .import_any_file(id, path)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?
        };
        self.attachments.insert(id.as_u64(), attachment.clone());
        self.execute(if image {
            Command::InsertImage {
                id: after,
                attachment,
            }
        } else {
            Command::InsertFile {
                id: after,
                attachment,
            }
        });
        self.blocks()
            .iter()
            .find(|block| block.attachment == Some(id))
            .map(|block| block.id)
            .ok_or_else(|| anyhow::anyhow!("core rejected the attachment insertion"))
    }
    pub fn attachment_display_path(&self, id: AttachmentId) -> Option<PathBuf> {
        self.attachments
            .get(&id.as_u64())
            .map(|attachment| self.attachment_store.display_path(attachment))
    }
    pub fn attachment_path(&self, id: AttachmentId) -> Option<PathBuf> {
        self.attachments
            .get(&id.as_u64())
            .map(|attachment| self.attachment_store.stored_path(attachment))
    }
    pub fn attachment_save_name(&self, id: AttachmentId) -> Option<String> {
        self.attachments
            .get(&id.as_u64())
            .map(|attachment| self.attachment_store.save_name(attachment))
    }
    pub fn export_attachment(&self, id: AttachmentId, target: &Path) -> Result<()> {
        let attachment = self
            .attachments
            .get(&id.as_u64())
            .ok_or_else(|| anyhow::anyhow!("附件元数据不存在"))?;
        self.attachment_store
            .export_to(attachment, target)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
    pub fn set_page_cover_from(&mut self, path: &Path) -> Result<()> {
        if self.page.locked {
            bail!("此页面已锁定");
        }
        let id = AttachmentId(self.next_attachment_id);
        self.next_attachment_id = self.next_attachment_id.saturating_add(1);
        let attachment = self
            .attachment_store
            .import_file(id, path)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        self.attachments.insert(id.as_u64(), attachment.clone());
        self.page.cover = Some(id);
        if let Some(page) = self.pages.iter_mut().find(|page| page.id == self.page.id) {
            page.cover = Some(id);
        }
        self.persistence.record(vec![
            Change::AttachmentAdded(attachment),
            Change::PageCoverSet {
                id: self.page.id,
                cover: Some(id),
            },
        ]);
        Ok(())
    }
    pub fn clear_page_cover(&mut self) {
        if self.page.locked || self.page.cover.is_none() {
            return;
        }
        self.page.cover = None;
        if let Some(page) = self.pages.iter_mut().find(|page| page.id == self.page.id) {
            page.cover = None;
        }
        self.persistence.record(vec![Change::PageCoverSet {
            id: self.page.id,
            cover: None,
        }]);
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
    pub fn import_markdown(&mut self, title: String, source: &str) -> PageId {
        self.import_markdown_under(title, source, None)
    }
    pub fn import_markdown_under(
        &mut self,
        title: String,
        source: &str,
        parent: Option<PageId>,
    ) -> PageId {
        self.end_typing();
        let parent = parent.filter(|id| {
            self.pages
                .iter()
                .any(|page| page.id == *id && !page.template)
        });
        let id = PageId(self.pages.iter().map(|page| page.id.0).max().unwrap_or(0) + 1);
        let last_order = self
            .pages
            .iter()
            .filter(|page| page.parent == parent && !page.template)
            .map(|page| page.order)
            .max();
        let page = Page {
            id,
            title,
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
        let changes = {
            let mut allocate = || self.document.alloc_block_id();
            quire_core::services::import_service::import_markdown(source, &page, &mut allocate)
        };
        self.document.apply(&changes);
        self.persistence.record(changes);
        self.pages.push(page.clone());
        self.pages.sort_by_key(|page| (page.parent, page.order));
        self.page = page;
        self.ensure_page_has_block();
        id
    }
    pub fn replace_page_markdown(&mut self, id: PageId, source: &str) -> Result<()> {
        self.end_typing();
        let page = self
            .pages
            .iter()
            .find(|page| page.id == id && !page.template)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("找不到页面 {}", id.0))?;
        if page.locked {
            bail!("页面 {} 已锁定", id.0);
        }
        let previous = self.document.page_blocks(id).to_vec();
        let exported = quire_core::services::export_service::export_page(&previous);
        let parsed = quire_core::services::import_service::parse_markdown(&exported);
        let empty_seed = previous.len() == 1
            && previous[0].kind == BlockKind::Paragraph
            && previous[0].text.is_empty();
        let markdown_round_trips = (parsed.len() == previous.len()
            && parsed.iter().zip(&previous).all(|(parsed, block)| {
                parsed.kind == block.kind
                    && parsed.text == block.text
                    && parsed.checked == block.checked
                    && parsed.marks == block.marks
                    && parsed.lang == block.lang
            }))
            || (parsed.is_empty() && empty_seed);
        let safe_blocks = previous.iter().all(|block| {
            block.parent.is_none()
                && matches!(
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
                        | BlockKind::Divider
                        | BlockKind::Math
                )
                && block.color == ColorKind::Default
                && block.background == ColorKind::Default
                && !block.folded
                && block.attachment.is_none()
                && block.page_ref.is_none()
                && block.sync_ref.is_none()
                && block.db_ref.is_none()
        });
        if !safe_blocks || !markdown_round_trips {
            bail!("页面含 Markdown 无法完整往返的块或样式；为避免丢失内容，拒绝整页替换");
        }
        let imported = {
            let mut allocate = || self.document.alloc_block_id();
            quire_core::services::import_service::import_markdown(source, &page, &mut allocate)
        };
        let inserted = imported
            .into_iter()
            .filter_map(|change| match change {
                Change::BlockInserted(block) => Some(block),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut apply = previous
            .iter()
            .filter(|block| block.parent.is_none())
            .map(|block| Change::BlockDeleted { id: block.id })
            .collect::<Vec<_>>();
        apply.extend(inserted.iter().cloned().map(Change::BlockInserted));
        if apply.is_empty() {
            return Ok(());
        }
        let mut revert = inserted
            .iter()
            .filter(|block| block.parent.is_none())
            .map(|block| Change::BlockDeleted { id: block.id })
            .collect::<Vec<_>>();
        revert.extend(previous.into_iter().map(Change::BlockInserted));
        self.document.apply(&apply);
        self.persistence.record(apply.clone());
        self.history.push(id, Entry { apply, revert });
        Ok(())
    }
    /// Replace one plain text block without round-tripping the whole page
    /// through Markdown. Rich text, child blocks and non-text entities are
    /// rejected so the operation cannot discard structure or formatting.
    pub fn replace_plain_page_block_text(
        &mut self,
        page_id: PageId,
        block_id: BlockId,
        text: String,
    ) -> Result<()> {
        let page = self
            .pages
            .iter()
            .find(|page| page.id == page_id && !page.template)
            .ok_or_else(|| anyhow::anyhow!("找不到页面 {}", page_id.0))?;
        if page.locked {
            bail!("页面 {} 已锁定", page_id.0);
        }
        let blocks = self.document.page_blocks(page_id);
        let block = blocks
            .iter()
            .find(|block| block.id == block_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("页面 {} 中找不到块 {}", page_id.0, block_id.0))?;
        if !matches!(
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
        ) || !block.marks.is_empty()
            || block.color != ColorKind::Default
            || block.background != ColorKind::Default
            || block.page_ref.is_some()
            || block.attachment.is_some()
            || block.db_ref.is_some()
            || block.sync_ref.is_some()
            || block.folded
            || blocks
                .iter()
                .any(|candidate| candidate.parent == Some(block_id))
        {
            bail!("该块包含格式、引用或子结构；为保护内容，拒绝直接替换");
        }

        let current_page = self.page.id;
        if !self.select_page(page_id) {
            bail!("无法打开页面 {}", page_id.0);
        }
        self.execute(Command::ReplaceText { id: block_id, text });
        let _ = self.select_page(current_page);
        Ok(())
    }
    pub fn duplicate_page(&mut self, id: PageId) -> Option<PageId> {
        self.end_typing();
        let original = self.pages.iter().find(|page| page.id == id)?.clone();
        if original.template {
            return None;
        }
        let new_id = PageId(
            self.pages
                .iter()
                .map(|page| page.id.0)
                .max()?
                .checked_add(1)?,
        );
        let siblings = self
            .pages
            .iter()
            .filter(|page| page.parent == original.parent && !page.template)
            .collect::<Vec<_>>();
        let next_order = siblings
            .iter()
            .position(|page| page.id == original.id)
            .and_then(|index| siblings.get(index + 1).map(|page| page.order));
        let order = OrderKey::between(Some(original.order), next_order).or_else(|| {
            siblings
                .iter()
                .map(|page| page.order)
                .max()
                .and_then(|last| OrderKey::between(Some(last), None))
        })?;
        let mut duplicate = original.clone();
        duplicate.id = new_id;
        duplicate.title = format!("{} 副本", original.title);
        duplicate.order = order;
        duplicate.favorite = false;
        duplicate.expanded = true;
        duplicate.locked = false;

        let source_blocks = self.document.page_blocks(id).to_vec();
        let mut id_map = BTreeMap::new();
        let mut copies = Vec::with_capacity(source_blocks.len());
        for block in source_blocks {
            let fresh_id = self.document.alloc_block_id();
            id_map.insert(block.id, fresh_id);
            let mut copy = block;
            copy.id = fresh_id;
            copy.page = new_id;
            copies.push(copy);
        }
        for block in &mut copies {
            if let Some(parent) = block.parent {
                block.parent = id_map.get(&parent).copied();
            }
        }

        let mut changes = Vec::with_capacity(copies.len() + 2);
        changes.push(Change::PageCreated(duplicate.clone()));
        changes.extend(copies.iter().cloned().map(Change::BlockInserted));
        if let Some(parent) = duplicate.parent
            && let Some(parent_page) = self.pages.iter_mut().find(|page| page.id == parent)
            && !parent_page.expanded
        {
            parent_page.expanded = true;
            changes.push(Change::PageExpandedSet {
                id: parent,
                expanded: true,
            });
        }
        self.document.apply(&changes);
        self.persistence.record(changes);
        self.pages.push(duplicate.clone());
        self.pages.sort_by_key(|page| (page.parent, page.order));
        self.page = duplicate;
        self.ensure_page_has_block();
        Some(new_id)
    }

    pub fn templates(&self) -> Vec<Page> {
        let mut templates = self
            .pages
            .iter()
            .filter(|page| page.template)
            .cloned()
            .collect::<Vec<_>>();
        templates.sort_by_key(|page| page.order);
        templates
    }

    pub fn save_page_as_template(&mut self, title: String) -> Option<PageId> {
        if self.page.locked || title.trim().is_empty() {
            return None;
        }
        self.end_typing();
        let id = PageId(
            self.pages
                .iter()
                .map(|page| page.id.0)
                .max()?
                .checked_add(1)?,
        );
        let order = OrderKey::between(
            self.pages
                .iter()
                .filter(|page| page.template)
                .map(|page| page.order)
                .max(),
            None,
        )?;
        let page = Page {
            id,
            title: title.trim().to_owned(),
            parent: None,
            order,
            favorite: false,
            expanded: false,
            font: PageFont::Default,
            full_width: false,
            small_text: false,
            icon: String::new(),
            cover: None,
            locked: false,
            template: true,
        };
        let source = self.document.page_blocks(self.page.id).to_vec();
        let mut id_map = BTreeMap::new();
        let mut copies = Vec::with_capacity(source.len());
        for block in source {
            let source_id = block.id;
            let mut copy = block;
            copy.id = self.document.alloc_block_id();
            copy.page = id;
            id_map.insert(source_id, copy.id);
            copies.push(copy);
        }
        for block in &mut copies {
            if let Some(parent) = block.parent {
                block.parent = id_map.get(&parent).copied();
            }
        }
        let mut changes = vec![Change::PageCreated(page.clone())];
        changes.extend(copies.iter().cloned().map(Change::BlockInserted));
        self.document.apply(&changes);
        self.persistence.record(changes);
        self.pages.push(page);
        self.pages.sort_by_key(|page| (page.parent, page.order));
        Some(id)
    }

    pub fn import_markdown_as_template(&mut self, title: String, source: &str) -> Option<PageId> {
        let title = title.trim();
        if title.is_empty() {
            return None;
        }
        self.end_typing();
        let id = PageId(
            self.pages
                .iter()
                .map(|page| page.id.0)
                .max()?
                .checked_add(1)?,
        );
        let order = OrderKey::between(
            self.pages
                .iter()
                .filter(|page| page.template)
                .map(|page| page.order)
                .max(),
            None,
        )?;
        let page = Page {
            id,
            title: title.to_owned(),
            parent: None,
            order,
            favorite: false,
            expanded: false,
            font: PageFont::Default,
            full_width: false,
            small_text: false,
            icon: String::new(),
            cover: None,
            locked: false,
            template: true,
        };
        let changes = {
            let mut allocate = || self.document.alloc_block_id();
            quire_core::services::import_service::import_markdown(source, &page, &mut allocate)
        };
        self.document.apply(&changes);
        self.persistence.record(changes);
        self.pages.push(page);
        self.pages.sort_by_key(|page| (page.parent, page.order));
        Some(id)
    }

    pub fn create_page_from_template(
        &mut self,
        template_id: PageId,
        title: Option<String>,
    ) -> Option<PageId> {
        self.end_typing();
        let source_page = self
            .pages
            .iter()
            .find(|page| page.id == template_id && page.template)?
            .clone();
        let id = PageId(
            self.pages
                .iter()
                .map(|page| page.id.0)
                .max()?
                .checked_add(1)?,
        );
        let order = OrderKey::between(
            self.pages
                .iter()
                .filter(|page| !page.template && page.parent.is_none())
                .map(|page| page.order)
                .max(),
            None,
        )?;
        let mut page = source_page;
        page.id = id;
        page.title = title
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(page.title);
        page.parent = None;
        page.order = order;
        page.favorite = false;
        page.expanded = true;
        page.locked = false;
        page.template = false;
        let source = self.document.page_blocks(template_id).to_vec();
        let mut id_map = BTreeMap::new();
        let mut copies = Vec::with_capacity(source.len());
        for block in source {
            let source_id = block.id;
            let mut copy = block;
            copy.id = self.document.alloc_block_id();
            copy.page = id;
            id_map.insert(source_id, copy.id);
            copies.push(copy);
        }
        for block in &mut copies {
            if let Some(parent) = block.parent {
                block.parent = id_map.get(&parent).copied();
            }
        }
        let mut changes = vec![Change::PageCreated(page.clone())];
        changes.extend(copies.iter().cloned().map(Change::BlockInserted));
        self.document.apply(&changes);
        self.persistence.record(changes);
        self.pages.push(page.clone());
        self.pages.sort_by_key(|page| (page.parent, page.order));
        self.page = page;
        self.ensure_page_has_block();
        Some(id)
    }

    pub fn insert_template(
        &mut self,
        template_id: PageId,
        anchor: Option<BlockId>,
    ) -> Option<BlockId> {
        if self.page.locked {
            return None;
        }
        let template = self
            .pages
            .iter()
            .find(|page| page.id == template_id && page.template)?;
        let blocks = self.document.page_blocks(template.id).to_vec();
        if blocks.is_empty() {
            return None;
        }
        let anchor = anchor.filter(|id| {
            self.document
                .block(*id)
                .is_some_and(|block| block.page == self.page.id)
        });
        let replace_blank = anchor.is_some_and(|id| {
            self.document.block(id).is_some_and(|block| {
                block.parent.is_none()
                    && block.kind == BlockKind::Paragraph
                    && block.text.is_empty()
                    && self
                        .document
                        .page_blocks(self.page.id)
                        .iter()
                        .all(|candidate| candidate.parent != Some(id))
            })
        });
        self.end_typing();
        let mut commands = vec![Command::InsertForest { anchor, blocks }];
        if replace_blank {
            commands.push(Command::DeleteBlock { id: anchor? });
        }
        let changes = command::exec_all(
            &mut self.document,
            &mut self.history,
            self.page.id,
            commands,
        )?;
        let first = changes.iter().find_map(|change| match change {
            Change::BlockInserted(block) if block.parent.is_none() => Some(block.id),
            _ => None,
        });
        self.persistence.record(changes);
        first
    }

    pub fn delete_template(&mut self, template_id: PageId) -> bool {
        if !self
            .pages
            .iter()
            .any(|page| page.id == template_id && page.template)
        {
            return false;
        }
        self.end_typing();
        self.document.drop_page(template_id);
        self.pages.retain(|page| page.id != template_id);
        self.persistence
            .record(vec![Change::PageDeleted { id: template_id }]);
        true
    }

    pub fn duplicate_block(&mut self, id: BlockId) -> Option<BlockId> {
        if self.page.locked {
            return None;
        }
        self.end_typing();
        let kind = self.document.block(id)?.kind;
        let source = self.blocks().to_vec();
        let mut subtree = HashSet::from([id]);
        loop {
            let descendants = source
                .iter()
                .filter(|block| block.parent.is_some_and(|parent| subtree.contains(&parent)))
                .map(|block| block.id)
                .filter(|block| !subtree.contains(block))
                .collect::<Vec<_>>();
            if descendants.is_empty() {
                break;
            }
            subtree.extend(descendants);
        }
        let forest = source
            .into_iter()
            .filter(|block| subtree.contains(&block.id))
            .collect::<Vec<_>>();
        let existing = self
            .blocks()
            .iter()
            .map(|block| block.id)
            .collect::<HashSet<_>>();
        let command = if forest.len() > 1 && !matches!(kind, BlockKind::Table | BlockKind::Columns)
        {
            Command::InsertForest {
                anchor: Some(id),
                blocks: forest,
            }
        } else {
            Command::DuplicateBlock { id }
        };
        self.execute(command);
        self.blocks()
            .iter()
            .find(|block| !existing.contains(&block.id))
            .map(|block| block.id)
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
    pub fn recent_pages(&self) -> Vec<PageId> {
        self.setting("gpui.recent-pages")
            .unwrap_or_default()
            .split(',')
            .filter_map(|id| id.parse::<u64>().ok().map(PageId))
            .filter(|id| {
                self.pages
                    .iter()
                    .any(|page| page.id == *id && !page.template)
            })
            .take(8)
            .collect()
    }
    pub fn visit_page(&mut self, id: PageId) {
        if !self
            .pages
            .iter()
            .any(|page| page.id == id && !page.template)
        {
            return;
        }
        let mut recent = self
            .recent_pages()
            .into_iter()
            .filter(|page_id| *page_id != id)
            .collect::<Vec<_>>();
        recent.insert(0, id);
        recent.truncate(8);
        self.set_setting(
            "gpui.recent-pages",
            recent
                .iter()
                .map(|page_id| page_id.0.to_string())
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    pub fn set_setting(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        self.settings.insert(key.clone(), value.clone());
        self.persistence
            .record(vec![Change::SettingSet { key, value }]);
    }
    pub fn sync_self_info(&mut self) -> quire_core::services::sync::engine::DeviceInfo {
        use quire_core::services::sync::{SYNC_PORT, engine::DeviceInfo};
        let mut id = self
            .setting("sync.device-id")
            .unwrap_or_default()
            .to_owned();
        if id.is_empty() {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default();
            id = format!("{}-{nonce:x}-{:x}", DeviceInfo::kind(), std::process::id());
            self.set_setting("sync.device-id", id.clone());
        }
        let mut name = self
            .setting("sync.device-name")
            .unwrap_or_default()
            .to_owned();
        if name.is_empty() {
            name = DeviceInfo::default_name(&id);
            self.set_setting("sync.device-name", name.clone());
        }
        DeviceInfo {
            id,
            name,
            kind: DeviceInfo::kind(),
            port: SYNC_PORT,
        }
    }

    pub fn sync_peers(&self) -> Vec<quire_core::services::sync::engine::PeerRecord> {
        self.setting("sync.peers")
            .and_then(|value| serde_json::from_str(value).ok())
            .unwrap_or_default()
    }

    pub fn sync_note_device(&mut self, id: &str, name: &str, kind: &str, ip: &str, port: u16) {
        let now = quire_core::services::sync::engine::now_unix();
        let mut peers = self.sync_peers();
        if let Some(peer) = peers.iter_mut().find(|peer| peer.id == id) {
            peer.name = name.to_owned();
            peer.kind = kind.to_owned();
            peer.ip = ip.to_owned();
            if port != 0 {
                peer.port = port;
            }
            peer.paired = true;
            peer.last_seen = now;
        } else {
            peers.push(quire_core::services::sync::engine::PeerRecord {
                id: id.to_owned(),
                name: name.to_owned(),
                kind: kind.to_owned(),
                ip: ip.to_owned(),
                port: if port == 0 {
                    quire_core::services::sync::SYNC_PORT
                } else {
                    port
                },
                paired: true,
                last_seen: now,
                last_sync: String::new(),
            });
        }
        self.set_setting(
            "sync.peers",
            serde_json::to_string(&peers).unwrap_or_default(),
        );
    }

    pub fn sync_mark_done(&mut self, peer_id: &str, ok: bool, message: &str) {
        let mut peers = self.sync_peers();
        let name = if let Some(peer) = peers.iter_mut().find(|peer| peer.id == peer_id) {
            if ok {
                peer.last_sync = quire_core::services::sync::engine::now_rfc3339();
            }
            peer.name.clone()
        } else {
            peer_id.to_owned()
        };
        self.set_setting(
            "sync.peers",
            serde_json::to_string(&peers).unwrap_or_default(),
        );
        self.sync_log_push(&name, ok, message);
    }

    pub fn sync_log(&self) -> Vec<quire_core::services::sync::engine::LogLine> {
        self.setting("sync.log")
            .and_then(|value| serde_json::from_str(value).ok())
            .unwrap_or_default()
    }

    pub fn sync_log_push(&mut self, peer: &str, ok: bool, message: &str) {
        let mut log = self.sync_log();
        log.push(quire_core::services::sync::engine::LogLine {
            at: quire_core::services::sync::engine::now_rfc3339(),
            peer: peer.to_owned(),
            ok,
            message: message.to_owned(),
        });
        if log.len() > 50 {
            log.drain(..log.len() - 50);
        }
        self.set_setting("sync.log", serde_json::to_string(&log).unwrap_or_default());
    }

    pub fn sync_forget_peer(&mut self, id: &str) {
        let mut peers = self.sync_peers();
        peers.retain(|peer| peer.id != id);
        self.set_setting(
            "sync.peers",
            serde_json::to_string(&peers).unwrap_or_default(),
        );
    }

    pub fn sync_config(&self) -> (bool, u64) {
        let automatic = self
            .setting("sync.auto")
            .map(|value| value != "0")
            .unwrap_or(true);
        let interval = self
            .setting("sync.interval")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(10)
            .max(10);
        (automatic, interval)
    }

    pub fn sync_set_auto(&mut self, automatic: bool) {
        self.set_setting("sync.auto", if automatic { "1" } else { "0" });
    }

    pub fn sync_set_interval(&mut self, seconds: u64) {
        self.set_setting("sync.interval", seconds.max(10).to_string());
    }

    pub fn sync_set_device_name(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.setting("sync.device-name") == Some(name) {
            return false;
        }
        self.set_setting("sync.device-name", name);
        true
    }

    pub fn sync_retire_offline_peers(
        &mut self,
        online_window_secs: u64,
    ) -> Vec<quire_core::services::sync::engine::PeerRecord> {
        let now = quire_core::services::sync::engine::now_unix();
        let mut peers = self.sync_peers();
        let mut retired = Vec::new();
        peers.retain(|peer| {
            if now.saturating_sub(peer.last_seen) >= online_window_secs {
                retired.push(peer.clone());
                false
            } else {
                true
            }
        });
        if !retired.is_empty() {
            self.set_setting(
                "sync.peers",
                serde_json::to_string(&peers).unwrap_or_default(),
            );
        }
        retired
    }

    pub fn sync_prune_stale_peers(
        &mut self,
        stale_after_secs: u64,
    ) -> Vec<quire_core::services::sync::engine::PeerRecord> {
        let now = quire_core::services::sync::engine::now_unix();
        let mut peers = self.sync_peers();
        let mut stale = Vec::new();
        peers.retain(|peer| {
            if now.saturating_sub(peer.last_seen) >= stale_after_secs {
                stale.push(peer.clone());
                false
            } else {
                true
            }
        });
        if !stale.is_empty() {
            self.set_setting(
                "sync.peers",
                serde_json::to_string(&peers).unwrap_or_default(),
            );
        }
        stale
    }

    pub fn sync_log_clear(&mut self) {
        self.set_setting("sync.log", "[]");
    }

    /// Build a complete network-sync snapshot from this session. A partial
    /// database/attachment read is an error: peers interpret omitted rows as
    /// deletions, so returning a shortened snapshot would corrupt the merge.
    pub fn sync_export(
        &mut self,
    ) -> anyhow::Result<quire_core::services::sync::model::SyncSnapshot> {
        use quire_core::{
            core::database::{DatabaseId, PropertyId},
            services::sync::model::{
                self, SAttachment, SBlock, SNote, SPage, SRecord, STask, STaskList, SValue,
                SyncSnapshot,
            },
        };
        let me = self.sync_self_info();
        self.persistence.force_flush()?;
        let mut snapshot = SyncSnapshot::default();
        snapshot.device_id = me.id;
        snapshot.device = me.name;
        let shared_pages = self
            .pages
            .iter()
            .filter(|page| !page.template)
            .collect::<Vec<_>>();
        let shared_page_ids = shared_pages
            .iter()
            .map(|page| page.id)
            .collect::<HashSet<_>>();
        snapshot.pages = shared_pages.into_iter().map(SPage::from).collect();
        let shared_blocks = self
            .document
            .all_blocks()
            .filter(|block| shared_page_ids.contains(&block.page))
            .collect::<Vec<_>>();
        let shared_attachments = shared_blocks
            .iter()
            .filter_map(|block| block.attachment)
            .chain(
                self.pages
                    .iter()
                    .filter(|page| !page.template)
                    .filter_map(|page| page.cover),
            )
            .collect::<HashSet<_>>();
        snapshot.blocks = shared_blocks.into_iter().map(SBlock::from).collect();
        snapshot.attachments = self
            .attachments
            .values()
            .filter(|attachment| shared_attachments.contains(&attachment.id))
            .map(SAttachment::from)
            .collect();
        snapshot.databases = model::catalog_schema(&self.databases);
        for database in &mut snapshot.databases {
            let records = self.repo.records_of(DatabaseId(database.id))?;
            let record_ids: Vec<_> = records.iter().map(|record| record.id).collect();
            database.records = records
                .iter()
                .map(|record| SRecord {
                    id: record.id.0,
                    db: record.db.0,
                    page: record.page.map(|page| page.0),
                    ord: record.ord.0,
                })
                .collect();
            for property in &database.properties {
                let kind = quire_core::core::database::PropertyKind::try_from_str(&property.kind);
                if kind
                    .map(|kind| kind.is_computed() || kind.is_derived())
                    .unwrap_or(true)
                {
                    continue;
                }
                let values = self.repo.values_of(&record_ids, PropertyId(property.id))?;
                for (record, value) in values {
                    database.values.push(SValue::from_core(
                        quire_core::core::database::RecordId(record),
                        PropertyId(property.id),
                        &value,
                    ));
                }
            }
        }
        snapshot.notes = self.organizer.notes.iter().map(SNote::from).collect();
        snapshot.tasks = self.organizer.tasks.iter().map(STask::from).collect();
        snapshot.lists = self.organizer.lists.iter().map(STaskList::from).collect();
        Ok(snapshot)
    }

    pub fn sync_apply_remote(
        &mut self,
        remote: &quire_core::services::sync::model::SyncSnapshot,
        bytes: &[(u64, Vec<u8>)],
        peer: &quire_core::services::sync::engine::PeerRecord,
    ) -> anyhow::Result<quire_core::services::sync::model::SyncSnapshot> {
        use quire_core::{
            core::{
                persistence::Change,
                types::{AttachmentId, BlockId, PageId},
            },
            services::sync::model::{SBlock, SyncSnapshot},
        };
        let local = self.sync_export()?;
        let shadow = self
            .setting(&format!("sync.shadow.{}", peer.id))
            .and_then(|value| SyncSnapshot::from_json(value).ok());

        macro_rules! allocator {
            ($name:ident, $last:expr) => {
                let mut $name = $last.saturating_add(1).max(1);
                let mut $name = || {
                    let id = $name;
                    $name = $name.saturating_add(1);
                    id
                };
            };
        }
        allocator!(
            next_page,
            self.pages.iter().map(|row| row.id.0).max().unwrap_or(0)
        );
        allocator!(
            next_block,
            self.document
                .all_blocks()
                .map(|row| row.id.0)
                .max()
                .unwrap_or(0)
        );
        allocator!(
            next_attachment,
            local
                .attachments
                .iter()
                .map(|row| row.id)
                .max()
                .unwrap_or(0)
        );
        allocator!(
            next_db,
            local.databases.iter().map(|row| row.id).max().unwrap_or(0)
        );
        allocator!(
            next_property,
            local
                .databases
                .iter()
                .flat_map(|db| db.properties.iter().map(|row| row.id))
                .max()
                .unwrap_or(0)
        );
        allocator!(
            next_record,
            local
                .databases
                .iter()
                .flat_map(|db| db.records.iter().map(|row| row.id))
                .max()
                .unwrap_or(0)
        );
        allocator!(
            next_view,
            local
                .databases
                .iter()
                .flat_map(|db| db.views.iter().map(|row| row.id))
                .max()
                .unwrap_or(0)
        );
        allocator!(
            next_note,
            local.notes.iter().map(|row| row.id).max().unwrap_or(0)
        );
        allocator!(
            next_task,
            local.tasks.iter().map(|row| row.id).max().unwrap_or(0)
        );
        allocator!(
            next_list,
            local.lists.iter().map(|row| row.id).max().unwrap_or(0)
        );
        let mut uuid = || quire_core::core::organizer::new_uuid();
        let mut ctx = quire_core::services::sync::merge::MergeCtx {
            next_page: &mut next_page,
            next_block: &mut next_block,
            next_attachment: &mut next_attachment,
            next_db: &mut next_db,
            next_property: &mut next_property,
            next_record: &mut next_record,
            next_view: &mut next_view,
            next_note: &mut next_note,
            next_task: &mut next_task,
            next_list: &mut next_list,
            new_uuid: &mut uuid,
        };
        let outcome = quire_core::services::sync::merge::merge(
            &local,
            shadow.as_ref(),
            remote,
            &peer.name,
            &mut ctx,
        );
        let attachment_remap = outcome.attachment_remap;
        let mut merged = outcome.merged;
        drop(ctx);

        // Never apply an invalid page/block forest. Network snapshots are
        // untrusted; missing parents and cycles are repaired to the root before
        // the storage tree or in-memory document can be touched.
        normalize_page_tree(&mut merged.pages);
        normalize_block_tree(&mut merged.blocks);

        let bytes: std::collections::HashMap<u64, &[u8]> = bytes
            .iter()
            .map(|(id, data)| {
                let target = attachment_remap
                    .iter()
                    .find(|(remote_id, _)| remote_id == id)
                    .map(|(_, local_id)| *local_id)
                    .unwrap_or(*id);
                (target, data.as_slice())
            })
            .collect();
        let mut changes = Vec::<Change>::new();

        // Attachments are materialized before rows point to them. A missing
        // payload fails the round instead of silently persisting a broken file.
        for row in &merged.attachments {
            if self.attachments.contains_key(&row.id) {
                continue;
            }
            let data = bytes
                .get(&row.id)
                .ok_or_else(|| anyhow::anyhow!("对端快照缺少附件 {} 的文件", row.id))?;
            let id = AttachmentId(row.id);
            let stored = if row.mime.starts_with("image/") {
                self.attachment_store.import_bytes(id, &row.name, data).ok()
            } else {
                let temp = std::env::temp_dir().join(format!(
                    "quire-sync-{}-{}-{}.bin",
                    std::process::id(),
                    row.id,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|value| value.as_nanos())
                        .unwrap_or_default()
                ));
                std::fs::write(&temp, data)?;
                let imported = self.attachment_store.import_any_file(id, &temp).ok();
                let _ = std::fs::remove_file(&temp);
                imported
            }
            .ok_or_else(|| anyhow::anyhow!("无法写入同步附件 {}", row.id))?;
            self.attachments.insert(row.id, stored.clone());
            changes.push(Change::AttachmentAdded(stored));
        }

        let local_pages: std::collections::HashMap<u64, _> = self
            .pages
            .iter()
            .filter(|page| !page.template)
            .map(|page| (page.id.0, page.clone()))
            .collect();
        let merged_page_ids: std::collections::HashSet<u64> =
            merged.pages.iter().map(|page| page.id).collect();
        let mut deleted_pages: Vec<_> = local_pages
            .keys()
            .copied()
            .filter(|id| !merged_page_ids.contains(id))
            .collect();
        deleted_pages.sort_unstable();
        let mut page_deletions = Vec::new();
        for id in deleted_pages.iter().copied().filter(|id| {
            local_pages
                .get(id)
                .and_then(|page| page.parent)
                .is_none_or(|parent| merged_page_ids.contains(&parent.0))
        }) {
            page_deletions.push(Change::PageDeleted { id: PageId(id) });
        }
        let mut ordered_pages = merged.pages.clone();
        ordered_pages.sort_by_key(|page| page_depth(page, &merged.pages));
        for wire in &ordered_pages {
            let page = wire.to_core();
            match local_pages.get(&wire.id) {
                None => changes.push(Change::PageCreated(page)),
                Some(before) => {
                    let id = before.id;
                    if before.title != page.title {
                        changes.push(Change::PageTitleSet {
                            id,
                            title: page.title.clone(),
                        });
                    }
                    if before.parent != page.parent || before.order != page.order {
                        changes.push(Change::PageMoved {
                            id,
                            parent: page.parent,
                            order: page.order,
                        });
                    }
                    if before.favorite != page.favorite {
                        changes.push(Change::PageFavoriteSet {
                            id,
                            favorite: page.favorite,
                        });
                    }
                    if before.expanded != page.expanded {
                        changes.push(Change::PageExpandedSet {
                            id,
                            expanded: page.expanded,
                        });
                    }
                    if before.font != page.font {
                        changes.push(Change::PageFontSet {
                            id,
                            font: page.font,
                        });
                    }
                    if before.full_width != page.full_width || before.small_text != page.small_text
                    {
                        changes.push(Change::PageLayoutSet {
                            id,
                            full_width: page.full_width,
                            small_text: page.small_text,
                        });
                    }
                    if before.icon != page.icon {
                        changes.push(Change::PageIconSet {
                            id,
                            icon: page.icon.clone(),
                        });
                    }
                    if before.cover != page.cover {
                        changes.push(Change::PageCoverSet {
                            id,
                            cover: page.cover,
                        });
                    }
                    if before.locked != page.locked {
                        changes.push(Change::PageLockedSet {
                            id,
                            locked: page.locked,
                        });
                    }
                }
            }
        }

        let local_blocks: std::collections::HashMap<u64, SBlock> = self
            .document
            .all_blocks()
            .filter(|block| {
                self.pages
                    .iter()
                    .any(|page| page.id == block.page && !page.template)
            })
            .map(|block| (block.id.0, SBlock::from(block)))
            .collect();
        let merged_block_ids: std::collections::HashSet<u64> =
            merged.blocks.iter().map(|block| block.id).collect();
        let mut ordered_blocks = merged.blocks.clone();
        ordered_blocks.sort_by_key(|block| block_depth(block, &merged.blocks));
        for wire in &ordered_blocks {
            match local_blocks.get(&wire.id) {
                None => changes.push(Change::BlockInserted(wire.to_core())),
                Some(before) => sync_block_diff(before, wire, &mut changes),
            }
        }
        let mut deleted_blocks: Vec<_> = local_blocks
            .keys()
            .copied()
            .filter(|id| !merged_block_ids.contains(id))
            .collect();
        deleted_blocks.sort_unstable();
        for id in deleted_blocks.iter().copied().filter(|id| {
            local_blocks
                .get(id)
                .is_some_and(|block| merged_page_ids.contains(&block.page))
                && local_blocks
                    .get(id)
                    .and_then(|block| block.parent)
                    .is_none_or(|parent| merged_block_ids.contains(&parent))
        }) {
            changes.push(Change::BlockDeleted { id: BlockId(id) });
        }
        changes.extend(page_deletions);

        sync_database_changes(&local.databases, &merged.databases, &mut changes);
        sync_organizer_changes(&local, &merged, &mut changes);

        // The document, catalogs, and persistence queue must observe the same
        // ordered batch. Page rows are maintained by Session; block operations
        // are applied by Document; the two catalogs have dedicated reducers.
        for page in &ordered_pages {
            if !local_pages.contains_key(&page.id) {
                self.document.set_page_blocks(PageId(page.id), Vec::new());
            }
        }
        self.document.apply(&changes);
        for id in &deleted_pages {
            self.document.drop_page(PageId(*id));
        }
        let template_pages = self
            .pages
            .iter()
            .filter(|page| page.template)
            .cloned()
            .collect::<Vec<_>>();
        self.pages = ordered_pages
            .into_iter()
            .map(|page| page.to_core())
            .chain(template_pages)
            .collect();
        self.pages.sort_by_key(|page| (page.parent, page.order));
        self.apply_database_changes(&changes);
        self.apply_organizer_changes(&changes);
        self.persistence.record(changes);
        self.persistence.force_flush()?;
        self.databases = self.repo.load_databases()?;
        if let Some(page) = self
            .pages
            .iter()
            .find(|page| page.id == self.page.id)
            .cloned()
        {
            self.page = page;
        } else if let Some(page) = self.pages.iter().find(|page| !page.template).cloned() {
            self.page = page;
        }
        self.ensure_page_has_block();

        let me = self.sync_self_info();
        merged.device_id = me.id;
        merged.device = me.name;
        self.set_setting(format!("sync.shadow.{}", peer.id), merged.to_json());
        self.persistence.force_flush()?;
        Ok(merged)
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

    pub fn set_sync_source(&mut self, id: BlockId, source: Option<BlockId>) -> bool {
        if self.page.locked {
            return false;
        }
        let Some(block) = self.document.block(id) else {
            return false;
        };
        if block.page != self.page.id || block.kind != BlockKind::Synced {
            return false;
        }
        if source
            .is_some_and(|source_id| source_id == id || self.document.block(source_id).is_none())
        {
            return false;
        }
        let previous = block.sync_ref;
        let previous_text = block.text.clone();
        if previous == source {
            return true;
        }
        self.end_typing();
        let mut apply = Vec::new();
        let mut revert = Vec::new();
        if !previous_text.is_empty() {
            apply.push(Change::BlockTextSet {
                id,
                text: String::new(),
            });
            revert.push(Change::BlockTextSet {
                id,
                text: previous_text,
            });
        }
        apply.push(Change::BlockSyncSet { id, source });
        revert.insert(
            0,
            Change::BlockSyncSet {
                id,
                source: previous,
            },
        );
        self.apply_entry(Entry { apply, revert });
        true
    }

    pub fn set_page_reference(&mut self, id: BlockId, target: Option<PageId>) -> bool {
        if self.page.locked {
            return false;
        }
        let Some(block) = self.document.block(id) else {
            return false;
        };
        if block.page != self.page.id || !matches!(block.kind, BlockKind::Page | BlockKind::Link) {
            return false;
        }
        if target.is_some_and(|page_id| {
            !self
                .pages
                .iter()
                .any(|page| page.id == page_id && !page.template)
        }) {
            return false;
        }
        let previous = block.page_ref;
        let previous_text = block.text.clone();
        if previous == target {
            return true;
        }
        self.end_typing();
        let mut apply = Vec::new();
        let mut revert = Vec::new();
        if !previous_text.is_empty() {
            apply.push(Change::BlockTextSet {
                id,
                text: String::new(),
            });
            revert.push(Change::BlockTextSet {
                id,
                text: previous_text,
            });
        }
        apply.push(Change::BlockRefSet { id, page: target });
        revert.insert(0, Change::BlockRefSet { id, page: previous });
        self.apply_entry(Entry { apply, revert });
        true
    }
    pub fn create_note(&mut self) -> NoteId {
        let id = NoteId(
            self.organizer
                .notes
                .iter()
                .map(|note| note.id.0)
                .max()
                .unwrap_or(0)
                .saturating_add(1)
                .max(1),
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let millis = now.as_millis().min(i64::MAX as u128) as i64;
        let seconds = now.as_secs().min(i64::MAX as u64) as i64;
        let note = Note {
            id,
            uuid: organizer::new_uuid(),
            title: "新建笔记".into(),
            body: String::new(),
            pinned: false,
            tags: Vec::new(),
            created: seconds,
            edited: seconds,
            ref_note: None,
            deleted_at: None,
            rev: organizer::rev(millis, "quire-gpui"),
        };
        let Some(entry) = command::plan(
            &mut self.document,
            ORGANIZER_STACK,
            Command::CreateNote { note },
        ) else {
            return id;
        };
        self.apply_organizer_entry(entry);
        id
    }
    pub fn update_note_content(&mut self, id: NoteId, title: String, body: String) -> bool {
        let Some(tags) = self.organizer.note(id).map(|note| note.tags.clone()) else {
            return false;
        };
        self.update_note_editor(id, title, body, tags)
    }
    pub fn update_note_editor(
        &mut self,
        id: NoteId,
        title: String,
        body: String,
        tags: Vec<String>,
    ) -> bool {
        let Some(before) = self
            .organizer
            .notes
            .iter()
            .find(|note| note.id == id)
            .cloned()
        else {
            return false;
        };
        if before.title == title && before.body == body && before.tags == tags {
            return false;
        }
        let mut after = before.clone();
        after.title = title;
        after.body = body;
        after.tags = tags;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        after.edited = now.as_secs().min(i64::MAX as u64) as i64;
        after.rev = organizer::rev(now.as_millis().min(i64::MAX as u128) as i64, "quire-gpui");
        let Some(entry) = command::plan(
            &mut self.document,
            ORGANIZER_STACK,
            Command::UpdateNote { id, before, after },
        ) else {
            return false;
        };
        self.apply_organizer_entry(entry);
        true
    }
    pub fn set_note_pinned(&mut self, id: NoteId, pinned: bool) -> bool {
        self.update_note_metadata(id, |note| note.pinned = pinned)
    }
    pub fn set_note_tags(&mut self, id: NoteId, tags: Vec<String>) -> bool {
        self.update_note_metadata(id, |note| note.tags = tags)
    }

    pub fn organizer_tag_suggestions(&self, draft: &str) -> Vec<String> {
        let Some(hash) = draft.rfind('#') else {
            return Vec::new();
        };
        let typed = &draft[hash + 1..];
        if typed.contains([' ', '\n', '\t']) {
            return Vec::new();
        }
        let prefix = typed.to_lowercase();
        let tags: BTreeSet<String> = self
            .organizer
            .live_notes()
            .filter(|note| !self.note_delete_is_pending(note.id))
            .map(|note| &note.tags)
            .chain(
                self.organizer
                    .live_tasks()
                    .filter(|task| !self.task_delete_is_pending(task.id))
                    .map(|task| &task.tags),
            )
            .flat_map(|tags| tags.iter().cloned())
            .collect();
        let mut suggestions = tags
            .into_iter()
            .filter(|tag| {
                if prefix.is_empty() {
                    return true;
                }
                let last = tag.rsplit('/').next().unwrap_or_default();
                tag.to_lowercase().starts_with(&prefix) || last.to_lowercase().starts_with(&prefix)
            })
            .collect::<Vec<_>>();
        suggestions.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
        suggestions.truncate(5);
        suggestions
    }
    pub fn set_note_trashed(&mut self, id: NoteId, trashed: bool) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let deleted_at = trashed.then(|| now.as_secs().min(i64::MAX as u64) as i64);
        self.update_note_metadata(id, |note| note.deleted_at = deleted_at)
    }
    pub fn set_notes_trashed(&mut self, ids: &[NoteId], trashed: bool) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let millis = now.as_millis().min(i64::MAX as u128) as i64;
        let deleted_at = trashed.then(|| now.as_secs().min(i64::MAX as u64) as i64);
        let commands = ids
            .iter()
            .filter_map(|id| {
                let before = self.organizer.note(*id)?.clone();
                let mut after = before.clone();
                after.deleted_at = deleted_at;
                after.rev = organizer::rev(millis, "quire-gpui");
                Some(Command::UpdateNote {
                    id: *id,
                    before,
                    after,
                })
            })
            .collect::<Vec<_>>();
        self.apply_organizer_batch(commands)
    }
    pub fn pending_organizer_delete(&self) -> Option<&PendingOrganizerDelete> {
        self.pending_organizer_delete.as_ref()
    }
    pub fn note_delete_is_pending(&self, id: NoteId) -> bool {
        self.pending_organizer_delete
            .as_ref()
            .is_some_and(|pending| !pending.is_task && pending.ids.contains(&id.0))
    }
    pub fn task_delete_is_pending(&self, id: TaskId) -> bool {
        self.pending_organizer_delete
            .as_ref()
            .is_some_and(|pending| pending.is_task && pending.ids.contains(&id.0))
    }
    pub fn defer_trash_notes(&mut self, ids: &[NoteId], message: String) -> (u64, bool) {
        let committed_previous = self.commit_pending_organizer_delete_if_any();
        self.next_organizer_delete_token = self.next_organizer_delete_token.saturating_add(1);
        let token = self.next_organizer_delete_token;
        self.pending_organizer_delete = Some(PendingOrganizerDelete {
            token,
            ids: ids.iter().map(|id| id.0).collect(),
            is_task: false,
            message,
        });
        (token, committed_previous)
    }
    pub fn defer_trash_tasks(&mut self, ids: &[TaskId], message: String) -> (u64, bool) {
        let committed_previous = self.commit_pending_organizer_delete_if_any();
        self.next_organizer_delete_token = self.next_organizer_delete_token.saturating_add(1);
        let token = self.next_organizer_delete_token;
        self.pending_organizer_delete = Some(PendingOrganizerDelete {
            token,
            ids: ids.iter().map(|id| id.0).collect(),
            is_task: true,
            message,
        });
        (token, committed_previous)
    }
    pub fn undo_pending_organizer_delete(&mut self) -> bool {
        self.pending_organizer_delete.take().is_some()
    }
    pub fn commit_pending_organizer_delete(&mut self, token: u64) -> bool {
        let Some(pending) = self
            .pending_organizer_delete
            .as_ref()
            .filter(|pending| pending.token == token)
            .cloned()
        else {
            return false;
        };
        self.pending_organizer_delete = None;
        if pending.is_task {
            let ids = pending.ids.into_iter().map(TaskId).collect::<Vec<_>>();
            self.set_tasks_trashed(&ids, true)
        } else {
            let ids = pending.ids.into_iter().map(NoteId).collect::<Vec<_>>();
            self.set_notes_trashed(&ids, true)
        }
    }
    fn commit_pending_organizer_delete_if_any(&mut self) -> bool {
        self.pending_organizer_delete
            .as_ref()
            .map(|pending| pending.token)
            .is_some_and(|token| self.commit_pending_organizer_delete(token))
    }
    fn update_note_metadata(&mut self, id: NoteId, update: impl FnOnce(&mut Note)) -> bool {
        let Some(before) = self.organizer.note(id).cloned() else {
            return false;
        };
        let mut after = before.clone();
        update(&mut after);
        if before == after {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        after.rev = organizer::rev(now.as_millis().min(i64::MAX as u128) as i64, "quire-gpui");
        let Some(entry) = command::plan(
            &mut self.document,
            ORGANIZER_STACK,
            Command::UpdateNote { id, before, after },
        ) else {
            return false;
        };
        self.apply_organizer_entry(entry);
        true
    }
    pub fn create_task(&mut self, title: String) -> TaskId {
        self.create_task_in(ListId::INBOX, title)
    }
    pub fn create_task_in(&mut self, list: ListId, title: String) -> TaskId {
        let id = TaskId(
            self.organizer
                .tasks
                .iter()
                .map(|task| task.id.0)
                .max()
                .unwrap_or(0)
                .saturating_add(1)
                .max(1),
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let millis = now.as_millis().min(i64::MAX as u128) as i64;
        let seconds = now.as_secs().min(i64::MAX as u64) as i64;
        let last_order = self.organizer.tasks_in(list).map(|task| task.ord).max();
        let task = Task {
            id,
            uuid: organizer::new_uuid(),
            list,
            title,
            notes: String::new(),
            priority: Priority::None,
            due: None,
            repeat: Repeat::None,
            done: false,
            completed_at: None,
            tags: Vec::new(),
            subtasks: Vec::<Subtask>::new(),
            created: seconds,
            edited: seconds,
            ord: OrderKey::between(last_order, None).unwrap_or(OrderKey(
                last_order.unwrap_or(OrderKey::FIRST).0.saturating_add(1),
            )),
            deleted_at: None,
            rev: organizer::rev(millis, "quire-gpui"),
        };
        if let Some(entry) = command::plan(
            &mut self.document,
            ORGANIZER_STACK,
            Command::CreateTask { task },
        ) {
            self.apply_organizer_entry(entry);
        }
        id
    }
    pub fn create_task_list(&mut self, name: String) -> ListId {
        let id = ListId(
            self.organizer
                .lists
                .iter()
                .map(|list| list.id.0)
                .max()
                .unwrap_or(0)
                .saturating_add(1)
                .max(1),
        );
        let previous = self.organizer.lists.iter().map(|list| list.ord).max();
        let list = TaskList {
            id,
            name,
            color: ColorKind::ALL[id.0 as usize % ColorKind::ALL.len()],
            ord: OrderKey::between(previous, None).unwrap_or(OrderKey(
                previous.unwrap_or(OrderKey::FIRST).0.saturating_add(1),
            )),
        };
        if let Some(entry) = command::plan(
            &mut self.document,
            ORGANIZER_STACK,
            Command::CreateTaskList { list },
        ) {
            self.apply_organizer_entry(entry);
        }
        id
    }
    pub fn update_task_list(&mut self, id: ListId, name: String, color: ColorKind) -> bool {
        let Some(before) = self.organizer.list(id).cloned() else {
            return false;
        };
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let mut after = before.clone();
        after.name = name.to_owned();
        after.color = color;
        if before == after {
            return false;
        }
        let Some(entry) = command::plan(
            &mut self.document,
            ORGANIZER_STACK,
            Command::UpdateTaskList { id, before, after },
        ) else {
            return false;
        };
        self.apply_organizer_entry(entry);
        true
    }
    pub fn set_task_list(&mut self, id: TaskId, list: ListId) -> bool {
        self.update_task(id, |task| task.list = list)
    }
    pub fn delete_task_list(&mut self, id: ListId) -> Option<usize> {
        let list = self.organizer.list(id)?.clone();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let seconds = now.as_secs().min(i64::MAX as u64) as i64;
        let millis = now.as_millis().min(i64::MAX as u128) as i64;
        let mut tasks = self.organizer.tasks_in(id).cloned().collect::<Vec<_>>();
        tasks.sort_by_key(|task| task.ord);
        let mut previous = self
            .organizer
            .tasks_in(ListId::INBOX)
            .map(|task| task.ord)
            .max();
        let moved = tasks
            .into_iter()
            .map(|before| {
                let mut after = before.clone();
                after.list = ListId::INBOX;
                after.edited = seconds;
                after.rev = organizer::rev(millis, "quire-gpui");
                after.ord = OrderKey::between(previous, None).unwrap_or(OrderKey(
                    previous.unwrap_or(OrderKey::FIRST).0.saturating_add(1),
                ));
                previous = Some(after.ord);
                (before, after)
            })
            .collect::<Vec<_>>();
        let count = moved.len();
        let entry = command::plan(
            &mut self.document,
            ORGANIZER_STACK,
            Command::DeleteTaskList { list, moved },
        )?;
        self.apply_organizer_entry(entry);
        Some(count)
    }
    pub fn update_task_content(&mut self, id: TaskId, title: String, notes: String) -> bool {
        let Some(due) = self.organizer.task(id).and_then(|task| task.due.clone()) else {
            return self.update_task_editor(id, title, notes, None);
        };
        self.update_task_editor(id, title, notes, Some(due))
    }
    pub fn update_task_editor(
        &mut self,
        id: TaskId,
        title: String,
        notes: String,
        due: Option<String>,
    ) -> bool {
        let Some(tags) = self.organizer.task(id).map(|task| task.tags.clone()) else {
            return false;
        };
        self.update_task_editor_tags(id, title, notes, due, tags)
    }
    pub fn update_task_editor_tags(
        &mut self,
        id: TaskId,
        title: String,
        notes: String,
        due: Option<String>,
        tags: Vec<String>,
    ) -> bool {
        if due
            .as_deref()
            .is_some_and(|date| !quire_core::core::date::is_iso_date(date))
        {
            return false;
        }
        self.update_task(id, |task| {
            task.title = title;
            task.notes = notes;
            task.due = due;
            task.tags = tags;
        })
    }
    pub fn set_task_done(&mut self, id: TaskId, done: bool) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .min(i64::MAX as u64) as i64;
        self.update_task(id, |task| {
            task.done = done;
            task.completed_at = done.then_some(now);
        })
    }
    pub fn set_task_priority(&mut self, id: TaskId, priority: Priority) -> bool {
        self.update_task(id, |task| task.priority = priority)
    }
    pub fn set_task_repeat(&mut self, id: TaskId, repeat: organizer::Repeat) -> bool {
        self.update_task(id, |task| task.repeat = repeat)
    }
    pub fn set_task_tags(&mut self, id: TaskId, tags: Vec<String>) -> bool {
        self.update_task(id, |task| task.tags = tags)
    }
    pub fn add_subtask(&mut self, id: TaskId, title: String) -> bool {
        let next_id = self
            .organizer
            .task(id)
            .map(|task| task.subtasks.iter().map(|item| item.id).max().unwrap_or(0) + 1);
        let Some(next_id) = next_id else { return false };
        self.update_task(id, |task| {
            task.subtasks.push(Subtask {
                id: next_id,
                title,
                done: false,
            });
        })
    }
    pub fn set_subtask_done(&mut self, id: TaskId, subtask_id: u64, done: bool) -> bool {
        self.update_task(id, |task| {
            if let Some(subtask) = task.subtasks.iter_mut().find(|item| item.id == subtask_id) {
                subtask.done = done;
            }
        })
    }
    pub fn delete_subtask(&mut self, id: TaskId, subtask_id: u64) -> bool {
        self.update_task(id, |task| {
            task.subtasks.retain(|item| item.id != subtask_id);
        })
    }
    pub fn set_subtask_title(&mut self, id: TaskId, subtask_id: u64, title: String) -> bool {
        self.update_task(id, |task| {
            if let Some(subtask) = task.subtasks.iter_mut().find(|item| item.id == subtask_id) {
                subtask.title = title;
            }
        })
    }
    pub fn set_tasks_done(&mut self, ids: &[TaskId], done: bool) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let millis = now.as_millis().min(i64::MAX as u128) as i64;
        let seconds = now.as_secs().min(i64::MAX as u64) as i64;
        let commands = ids
            .iter()
            .filter_map(|id| {
                let before = self.organizer.task(*id)?.clone();
                let mut after = before.clone();
                after.done = done;
                after.completed_at = done.then_some(seconds);
                after.edited = seconds;
                after.rev = organizer::rev(millis, "quire-gpui");
                Some(Command::UpdateTask {
                    id: *id,
                    before,
                    after,
                })
            })
            .collect::<Vec<_>>();
        self.apply_organizer_batch(commands)
    }
    pub fn set_tasks_trashed(&mut self, ids: &[TaskId], trashed: bool) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let millis = now.as_millis().min(i64::MAX as u128) as i64;
        let seconds = now.as_secs().min(i64::MAX as u64) as i64;
        let commands = ids
            .iter()
            .filter_map(|id| {
                let before = self.organizer.task(*id)?.clone();
                let mut after = before.clone();
                after.deleted_at = trashed.then_some(seconds);
                after.rev = organizer::rev(millis, "quire-gpui");
                Some(Command::UpdateTask {
                    id: *id,
                    before,
                    after,
                })
            })
            .collect::<Vec<_>>();
        self.apply_organizer_batch(commands)
    }
    pub fn purge_organizer_note(&mut self, id: NoteId) -> bool {
        self.commit_pending_organizer_delete_if_any();
        let Some(note) = self
            .organizer
            .note(id)
            .filter(|note| note.is_trashed())
            .cloned()
        else {
            return false;
        };
        let Some(entry) = command::plan(
            &mut self.document,
            ORGANIZER_STACK,
            Command::DeleteNote { note },
        ) else {
            return false;
        };
        self.apply_organizer_entry(entry);
        true
    }
    pub fn purge_organizer_task(&mut self, id: TaskId) -> bool {
        self.commit_pending_organizer_delete_if_any();
        let Some(task) = self
            .organizer
            .task(id)
            .filter(|task| task.is_trashed())
            .cloned()
        else {
            return false;
        };
        let Some(entry) = command::plan(
            &mut self.document,
            ORGANIZER_STACK,
            Command::DeleteTask { task },
        ) else {
            return false;
        };
        self.apply_organizer_entry(entry);
        true
    }
    pub fn empty_organizer_bin(&mut self, is_task: bool) -> usize {
        self.commit_pending_organizer_delete_if_any();
        let commands = if is_task {
            self.organizer
                .trashed_tasks()
                .cloned()
                .map(|task| Command::DeleteTask { task })
                .collect()
        } else {
            self.organizer
                .trashed_notes()
                .cloned()
                .map(|note| Command::DeleteNote { note })
                .collect()
        };
        let Some(changes) = command::exec_all(
            &mut self.document,
            &mut self.history,
            ORGANIZER_STACK,
            commands,
        ) else {
            return 0;
        };
        let count = changes
            .iter()
            .filter(|change| {
                matches!(
                    change,
                    Change::NoteDeleted { .. } | Change::TaskDeleted { .. }
                )
            })
            .count();
        self.apply_organizer_changes(&changes);
        self.persistence.record(changes);
        count
    }
    fn apply_organizer_batch(&mut self, commands: Vec<Command>) -> bool {
        let Some(changes) = command::exec_all(
            &mut self.document,
            &mut self.history,
            ORGANIZER_STACK,
            commands,
        ) else {
            return false;
        };
        self.apply_organizer_changes(&changes);
        self.persistence.record(changes);
        true
    }
    pub fn set_task_due(&mut self, id: TaskId, due: Option<String>) -> bool {
        if due
            .as_deref()
            .is_some_and(|date| !quire_core::core::date::is_iso_date(date))
        {
            return false;
        }
        self.update_task(id, |task| task.due = due)
    }
    pub fn set_task_trashed(&mut self, id: TaskId, trashed: bool) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .min(i64::MAX as u64) as i64;
        self.update_task(id, |task| {
            task.deleted_at = trashed.then_some(now);
        })
    }
    fn update_task(&mut self, id: TaskId, update: impl FnOnce(&mut Task)) -> bool {
        let Some(before) = self.organizer.task(id).cloned() else {
            return false;
        };
        let mut after = before.clone();
        update(&mut after);
        if before == after {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        after.edited = now.as_secs().min(i64::MAX as u64) as i64;
        after.rev = organizer::rev(now.as_millis().min(i64::MAX as u128) as i64, "quire-gpui");
        let Some(entry) = command::plan(
            &mut self.document,
            ORGANIZER_STACK,
            Command::UpdateTask { id, before, after },
        ) else {
            return false;
        };
        self.apply_organizer_entry(entry);
        true
    }
    fn apply_organizer_entry(&mut self, entry: Entry) {
        self.document.apply(&entry.apply);
        self.apply_organizer_changes(&entry.apply);
        self.persistence.record(entry.apply.clone());
        self.history.push(ORGANIZER_STACK, entry);
    }
    fn apply_organizer_changes(&mut self, changes: &[Change]) {
        for change in changes {
            match change {
                Change::NoteAdded(note) => self.organizer.notes.push(note.clone()),
                Change::NoteUpdated(note) => {
                    if let Some(existing) =
                        self.organizer.notes.iter_mut().find(|n| n.id == note.id)
                    {
                        *existing = note.clone();
                    }
                }
                Change::NoteDeleted { id } => self.organizer.notes.retain(|note| note.id != *id),
                Change::TaskAdded(task) => self.organizer.tasks.push(task.clone()),
                Change::TaskUpdated(task) => {
                    if let Some(existing) =
                        self.organizer.tasks.iter_mut().find(|t| t.id == task.id)
                    {
                        *existing = task.clone();
                    }
                }
                Change::TaskDeleted { id } => self.organizer.tasks.retain(|task| task.id != *id),
                Change::TaskListAdded(list) => self.organizer.lists.push(list.clone()),
                Change::TaskListUpdated(list) => {
                    if let Some(existing) =
                        self.organizer.lists.iter_mut().find(|l| l.id == list.id)
                    {
                        *existing = list.clone();
                    }
                }
                Change::TaskListDeleted { id } => {
                    self.organizer.lists.retain(|list| list.id != *id)
                }
                _ => {}
            }
        }
    }
    pub fn undo_organizer(&mut self) -> bool {
        let Some(entry) = self.history.undo(ORGANIZER_STACK) else {
            return false;
        };
        self.document.apply(&entry.revert);
        self.apply_organizer_changes(&entry.revert);
        self.persistence.record(entry.revert);
        true
    }
    pub fn redo_organizer(&mut self) -> bool {
        let Some(entry) = self.history.redo(ORGANIZER_STACK) else {
            return false;
        };
        self.document.apply(&entry.apply);
        self.apply_organizer_changes(&entry.apply);
        self.persistence.record(entry.apply);
        true
    }
    pub fn set_block_folded(&mut self, id: BlockId, folded: bool) {
        let Some(block) = self.document.block(id) else {
            return;
        };
        if block.folded == folded {
            return;
        }
        let change = Change::BlockFoldedSet { id, folded };
        self.document.apply(std::slice::from_ref(&change));
        self.persistence.record(vec![change]);
    }
    pub fn set_link(&mut self, id: BlockId, start: usize, end: usize, url: String) {
        if self.page.locked {
            return;
        }
        self.end_typing();
        let Some(block) = self.document.block(id).cloned() else {
            return;
        };
        let (start, end) = (
            start.min(end).min(block.text.len()),
            start.max(end).min(block.text.len()),
        );
        if start == end || !block.text.is_char_boundary(start) || !block.text.is_char_boundary(end)
        {
            return;
        }
        let mut marks = Vec::new();
        for mark in &block.marks {
            if mark.kind != quire_core::core::types::MarkKind::Link
                || mark.end <= start
                || mark.start >= end
            {
                marks.push(mark.clone());
                continue;
            }
            if mark.start < start {
                let mut left = mark.clone();
                left.end = start;
                marks.push(left);
            }
            if mark.end > end {
                let mut right = mark.clone();
                right.start = end;
                marks.push(right);
            }
        }
        marks.push(Mark {
            start,
            end,
            kind: quire_core::core::types::MarkKind::Link,
            url,
            date: None,
        });
        marks.sort_by_key(|mark| (mark.start, mark.end));
        if marks == block.marks {
            return;
        }
        let entry = Entry {
            apply: vec![Change::BlockMarksSet { id, marks }],
            revert: vec![Change::BlockMarksSet {
                id,
                marks: block.marks,
            }],
        };
        self.document.apply(&entry.apply);
        self.persistence.record(entry.apply.clone());
        self.history.push(self.page.id, entry);
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

    /// Move a block and its entire core-managed subtree to another ordinary,
    /// writable page. The core command records one undoable cross-page change.
    pub fn move_block_to_page(&mut self, id: BlockId, target: PageId) -> bool {
        if self.page.locked || target == self.page.id {
            return false;
        }
        let Some(destination) = self
            .pages
            .iter()
            .find(|page| page.id == target && !page.template)
        else {
            return false;
        };
        if destination.locked
            || !self
                .document
                .block(id)
                .is_some_and(|block| block.page == self.page.id)
        {
            return false;
        }
        self.end_typing();
        let source = self.page.id;
        let Some(mut entry) = command::plan(
            &mut self.document,
            source,
            Command::MoveBlockToPage { id, page: target },
        ) else {
            return false;
        };
        // The core command appends the moved root after the target's last
        // top-level block and preserves descendant order keys. Rebase the
        // moved subtree after every target block so flat display ordering
        // cannot interleave a destination child between the new root/child.
        let last_target_order = self
            .document
            .page_blocks(target)
            .iter()
            .map(|block| block.order)
            .max();
        let Some(mut next_order) = OrderKey::between(last_target_order, None) else {
            return false;
        };
        let moved_count = entry
            .apply
            .iter()
            .filter(
                |change| matches!(change, Change::BlockMovedToPage { page, .. } if *page == target),
            )
            .count();
        let mut moved_index = 0;
        for change in &mut entry.apply {
            if let Change::BlockMovedToPage { page, order, .. } = change
                && *page == target
            {
                *order = next_order;
                moved_index += 1;
                if moved_index < moved_count {
                    let Some(order) = OrderKey::between(Some(next_order), None) else {
                        return false;
                    };
                    next_order = order;
                }
            }
        }
        self.document.apply(&entry.apply);
        self.persistence.record(entry.apply.clone());
        self.history.push(source, entry);
        true
    }
    pub fn indent_page(&mut self, id: PageId) -> bool {
        let Some(page) = self
            .pages
            .iter()
            .find(|page| page.id == id && !page.template)
        else {
            return false;
        };
        let mut siblings = self
            .pages
            .iter()
            .filter(|candidate| candidate.parent == page.parent && !candidate.template)
            .collect::<Vec<_>>();
        siblings.sort_by_key(|candidate| candidate.order);
        let Some(index) = siblings.iter().position(|candidate| candidate.id == id) else {
            return false;
        };
        let Some(previous) = index.checked_sub(1).and_then(|index| siblings.get(index)) else {
            return false;
        };
        self.reparent_page(id, Some(previous.id), None)
    }
    pub fn outdent_page(&mut self, id: PageId) -> bool {
        let Some(page) = self
            .pages
            .iter()
            .find(|page| page.id == id && !page.template)
        else {
            return false;
        };
        let Some(parent_id) = page.parent else {
            return false;
        };
        let Some(parent) = self
            .pages
            .iter()
            .find(|candidate| candidate.id == parent_id)
        else {
            return false;
        };
        let destination_parent = parent.parent;
        let mut siblings = self
            .pages
            .iter()
            .filter(|candidate| {
                candidate.parent == destination_parent && candidate.id != id && !candidate.template
            })
            .collect::<Vec<_>>();
        siblings.sort_by_key(|candidate| candidate.order);
        let insertion = siblings
            .iter()
            .position(|candidate| candidate.id == parent_id)
            .map_or(siblings.len(), |index| index + 1);
        self.reparent_page(id, destination_parent, Some(insertion))
    }
    fn reparent_page(
        &mut self,
        id: PageId,
        parent: Option<PageId>,
        insertion: Option<usize>,
    ) -> bool {
        let Some(target) = self
            .pages
            .iter()
            .find(|page| page.id == id && !page.template)
            .cloned()
        else {
            return false;
        };
        if parent == Some(id)
            || target.parent == parent
            || parent.is_some_and(|parent_id| {
                !self
                    .pages
                    .iter()
                    .any(|page| page.id == parent_id && !page.template)
            })
        {
            return false;
        }
        let mut ancestor = parent;
        let mut visited = HashSet::new();
        while let Some(ancestor_id) = ancestor {
            if ancestor_id == id || !visited.insert(ancestor_id) {
                return false;
            }
            ancestor = self
                .pages
                .iter()
                .find(|page| page.id == ancestor_id)
                .and_then(|page| page.parent);
        }

        let mut siblings = self
            .pages
            .iter()
            .filter(|page| page.parent == parent && page.id != id && !page.template)
            .map(|page| page.id)
            .collect::<Vec<_>>();
        siblings.sort_by_key(|sibling| {
            self.pages
                .iter()
                .find(|page| page.id == *sibling)
                .map(|page| page.order)
        });
        let index = insertion.unwrap_or(siblings.len()).min(siblings.len());
        siblings.insert(index, id);
        let mut changes = Vec::new();
        for (index, sibling) in siblings.into_iter().enumerate() {
            let order = OrderKey(OrderKey::FIRST.0 + index as u64 * OrderKey::STRIDE);
            let Some(page) = self.pages.iter_mut().find(|page| page.id == sibling) else {
                continue;
            };
            let next_parent = parent;
            if page.parent != next_parent || page.order != order {
                page.parent = next_parent;
                page.order = order;
                changes.push(Change::PageMoved {
                    id: sibling,
                    parent: next_parent,
                    order,
                });
            }
        }
        if let Some(parent_id) = parent
            && let Some(parent_page) = self.pages.iter_mut().find(|page| page.id == parent_id)
            && !parent_page.expanded
        {
            parent_page.expanded = true;
            changes.push(Change::PageExpandedSet {
                id: parent_id,
                expanded: true,
            });
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
        let recent = self
            .recent_pages()
            .iter()
            .map(|page_id| page_id.0.to_string())
            .collect::<Vec<_>>()
            .join(",");
        if self.setting("gpui.recent-pages") != Some(recent.as_str()) {
            self.set_setting("gpui.recent-pages", recent);
        }

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
            self.visit_page(self.page.id);
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
                        | BlockKind::Toggle
                );
                let valid_child = b.parent.is_some_and(|parent| {
                    self.document.block(parent).is_some_and(|parent| {
                        parent.kind == BlockKind::Toggle
                            || parent.kind == BlockKind::Column
                            || (matches!(
                                b.kind,
                                BlockKind::Bullet | BlockKind::Numbered | BlockKind::Todo
                            ) && matches!(
                                parent.kind,
                                BlockKind::Bullet | BlockKind::Numbered | BlockKind::Todo
                            ))
                    })
                });
                let table_cell = b.kind == BlockKind::TableCell
                    && b.parent.is_some_and(|parent| {
                        self.document
                            .block(parent)
                            .is_some_and(|parent| parent.kind == BlockKind::Table)
                    });
                table_cell || (simple_kind && (b.parent.is_none() || valid_child))
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
            let database_change = changes.iter().any(is_database_change);
            self.apply_database_changes(&changes);
            self.persistence.record(changes);
            if database_change {
                let _ = self.persistence.force_flush();
            }
        }
    }
    pub fn redo(&mut self) {
        self.end_typing();
        if let Some(changes) = command::redo(&mut self.document, &mut self.history, self.page.id) {
            let database_change = changes.iter().any(is_database_change);
            self.apply_database_changes(&changes);
            self.persistence.record(changes);
            if database_change {
                let _ = self.persistence.force_flush();
            }
        }
    }
    fn apply_database_changes(&mut self, changes: &[Change]) {
        for change in changes {
            match change {
                Change::DatabaseCreated(database) => {
                    if !self
                        .databases
                        .databases
                        .iter()
                        .any(|item| item.id == database.id)
                    {
                        self.databases.databases.push(database.clone());
                    }
                }
                Change::DatabaseDeleted { id } => {
                    self.databases
                        .databases
                        .retain(|database| database.id != *id);
                    self.databases
                        .properties
                        .retain(|property| property.db != *id);
                    self.databases.views.retain(|view| view.db != *id);
                }
                Change::PropertyAdded(property) => self.databases.properties.push(property.clone()),
                Change::PropertyDeleted { id } => self
                    .databases
                    .properties
                    .retain(|property| property.id != *id),
                Change::PropertyRenamed { id, name } => {
                    if let Some(property) = self
                        .databases
                        .properties
                        .iter_mut()
                        .find(|property| property.id == *id)
                    {
                        property.name = name.clone();
                    }
                }
                Change::PropertyKindSet { id, kind } => {
                    if let Some(property) = self
                        .databases
                        .properties
                        .iter_mut()
                        .find(|property| property.id == *id)
                    {
                        property.kind = *kind;
                    }
                }
                Change::PropertyOrdSet { id, ord } => {
                    if let Some(property) = self
                        .databases
                        .properties
                        .iter_mut()
                        .find(|property| property.id == *id)
                    {
                        property.ord = *ord;
                    }
                }
                Change::PropertyConfigSet { id, config } => {
                    if let Some(property) = self
                        .databases
                        .properties
                        .iter_mut()
                        .find(|property| property.id == *id)
                    {
                        property.config = config.clone();
                    }
                }
                Change::ViewAdded(view) => self.databases.views.push(view.clone()),
                Change::ViewDeleted { id } => self.databases.views.retain(|view| view.id != *id),
                Change::ViewRenamed { id, name } => {
                    if let Some(view) = self.databases.views.iter_mut().find(|view| view.id == *id)
                    {
                        view.name = name.clone();
                    }
                }
                Change::ViewLayoutSet { id, layout } => {
                    if let Some(view) = self.databases.views.iter_mut().find(|view| view.id == *id)
                    {
                        view.layout = *layout;
                    }
                }
                Change::ViewDefinitionSet { id, definition } => {
                    if let Some(view) = self.databases.views.iter_mut().find(|view| view.id == *id)
                    {
                        view.definition = definition.clone();
                    }
                }
                Change::ViewOrdSet { id, ord } => {
                    if let Some(view) = self.databases.views.iter_mut().find(|view| view.id == *id)
                    {
                        view.ord = *ord;
                    }
                }
                Change::DatabaseRenamed { id, name } => {
                    if let Some(database) = self
                        .databases
                        .databases
                        .iter_mut()
                        .find(|database| database.id == *id)
                    {
                        database.name = name.clone();
                    }
                }
                Change::DatabaseTemplateSet { id, template } => {
                    if let Some(database) = self
                        .databases
                        .databases
                        .iter_mut()
                        .find(|database| database.id == *id)
                    {
                        database.template = template.clone();
                    }
                }
                _ => {}
            }
        }
    }
}

fn open_report_notice(report: &OpenReport, current_path: &Path) -> Option<String> {
    let mut notices = Vec::new();
    if let Some(from) = &report.recovered_from {
        notices.push(format!(
            "数据库无法读取，已从备份恢复（{}）。最近一次备份后的修改可能丢失；损坏文件已保留为 .corrupt。",
            from.display()
        ));
    }
    if report.backup_failed {
        notices.push("无法写入本次启动备份；请检查数据库目录的磁盘空间和写入权限。".into());
    }
    if let Some(from) = &report.migrated_from {
        notices.push(format!(
            "数据库已从旧位置迁移：{}。当前数据位置为 {}。",
            from.display(),
            current_path.display()
        ));
    }
    (!notices.is_empty()).then(|| notices.join(" "))
}

fn is_database_change(change: &Change) -> bool {
    matches!(
        change,
        Change::DatabaseCreated(_)
            | Change::DatabaseRenamed { .. }
            | Change::DatabaseDeleted { .. }
            | Change::DatabaseTemplateSet { .. }
            | Change::BlockDbRefSet { .. }
            | Change::RecordCreated(_)
            | Change::RecordOrdSet { .. }
            | Change::RecordPageSet { .. }
            | Change::RecordDeleted { .. }
            | Change::CellSet { .. }
            | Change::PropertyAdded(_)
            | Change::PropertyDeleted { .. }
            | Change::PropertyRenamed { .. }
            | Change::PropertyKindSet { .. }
            | Change::PropertyOrdSet { .. }
            | Change::PropertyConfigSet { .. }
            | Change::ViewAdded(_)
            | Change::ViewDeleted { .. }
            | Change::ViewRenamed { .. }
            | Change::ViewLayoutSet { .. }
            | Change::ViewDefinitionSet { .. }
            | Change::ViewOrdSet { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use quire_core::core::database::CellValue;

    #[test]
    fn startup_notice_surfaces_recovery_backup_and_migration_facts() {
        let path = PathBuf::from(r"C:\QuireGPUI\quire.db");
        assert_eq!(open_report_notice(&OpenReport::default(), &path), None);

        let report = OpenReport {
            recovered_from: Some(PathBuf::from(r"C:\QuireGPUI\quire.db.bak1")),
            migrated_from: Some(PathBuf::from(r"C:\old\appdata")),
            backup_failed: true,
        };
        let notice = open_report_notice(&report, &path).unwrap();
        assert!(notice.contains("quire.db.bak1"));
        assert!(notice.contains("修改可能丢失"));
        assert!(notice.contains("启动备份"));
        assert!(notice.contains(r"C:\old\appdata"));
        assert!(notice.contains(r"C:\QuireGPUI\quire.db"));
    }

    #[test]
    fn session_recovers_an_isolated_corrupt_database_and_exposes_notice() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-startup-recovery");
        let path = scratch.path().join("recover.db");
        {
            let mut session = Session::open(&path)?;
            session.set_page_title("Recovered content".into());
            session.persistence.force_flush()?;
        }

        let backup = quire_core::storage::backup::slot(&path, 1);
        std::fs::copy(&path, &backup)?;
        std::fs::write(&path, b"this is not a sqlite database")?;
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = path.as_os_str().to_os_string();
            sidecar.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(sidecar));
        }

        let recovered = Session::open(&path)?;
        let notice = recovered.startup_notice.as_deref().unwrap();
        assert!(notice.contains("数据库无法读取，已从备份恢复"));
        assert!(notice.contains("修改可能丢失"));
        assert_eq!(recovered.page.title, "Recovered content");
        assert!(path.with_extension("db.corrupt").exists());
        Ok(())
    }

    #[test]
    fn sync_preferences_and_peer_cleanup_are_persisted() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-sync-preferences");
        let path = scratch.path().join("sync.db");
        let mut session = Session::open(&path)?;
        assert_eq!(session.sync_config(), (true, 10));
        let default_name = session.sync_self_info().name;
        assert!(!session.sync_set_device_name("  "));
        assert!(session.sync_set_device_name("  Study PC  "));
        assert!(!session.sync_set_device_name("Study PC"));
        session.sync_set_auto(false);
        session.sync_set_interval(5);
        assert_eq!(session.sync_config(), (false, 10));

        let now = quire_core::services::sync::engine::now_unix();
        let peers = vec![
            quire_core::services::sync::engine::PeerRecord {
                id: "online".into(),
                name: "Online".into(),
                kind: "windows".into(),
                ip: "127.0.0.1".into(),
                port: quire_core::services::sync::SYNC_PORT,
                paired: true,
                last_seen: now,
                last_sync: String::new(),
            },
            quire_core::services::sync::engine::PeerRecord {
                id: "offline".into(),
                name: "Offline".into(),
                kind: "windows".into(),
                ip: "127.0.0.2".into(),
                port: quire_core::services::sync::SYNC_PORT,
                paired: true,
                last_seen: now.saturating_sub(3600),
                last_sync: String::new(),
            },
            quire_core::services::sync::engine::PeerRecord {
                id: "stale".into(),
                name: "Stale".into(),
                kind: "windows".into(),
                ip: "127.0.0.3".into(),
                port: quire_core::services::sync::SYNC_PORT,
                paired: true,
                last_seen: now.saturating_sub(31 * 24 * 3600),
                last_sync: String::new(),
            },
        ];
        session.set_setting("sync.peers", serde_json::to_string(&peers)?);
        session.sync_log_push("Offline", false, "connection timed out");
        assert_eq!(session.sync_log().len(), 1);
        let stale = session.sync_prune_stale_peers(30 * 24 * 3600);
        assert_eq!(
            stale
                .iter()
                .map(|peer| peer.id.as_str())
                .collect::<Vec<_>>(),
            ["stale"]
        );
        let removed = session.sync_retire_offline_peers(15);
        assert_eq!(
            removed
                .iter()
                .map(|peer| peer.id.as_str())
                .collect::<Vec<_>>(),
            ["offline"]
        );
        assert_eq!(
            session
                .sync_peers()
                .iter()
                .map(|peer| peer.id.as_str())
                .collect::<Vec<_>>(),
            ["online"]
        );
        session.sync_log_clear();
        assert!(session.sync_log().is_empty());
        session.persistence.force_flush()?;
        drop(session);

        let mut reopened = Session::open(&path)?;
        assert_eq!(reopened.sync_config(), (false, 10));
        assert_eq!(reopened.sync_self_info().name, "Study PC");
        assert_ne!(reopened.sync_self_info().name, default_name);
        assert_eq!(reopened.sync_peers().len(), 1);
        assert!(reopened.sync_log().is_empty());
        Ok(())
    }

    #[test]
    fn sync_export_carries_pages_blocks_database_rows_and_organizer() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-sync-export");
        let mut session = Session::open(&scratch.path().join("sync.db"))?;
        let note = session.create_note();
        session.update_note_editor(note, "sync note".into(), "body".into(), vec![]);
        let task = session.create_task("sync task".into());
        let block = session.blocks()[0].id;
        let database = session
            .make_database(block, "Sync database".into())
            .unwrap();
        let record = session.add_database_record(block).unwrap();
        let snapshot = session.sync_export()?;
        assert!(!snapshot.device_id.is_empty());
        assert_eq!(
            snapshot.pages.len(),
            session.pages.iter().filter(|page| !page.template).count()
        );
        assert_eq!(
            session.templates().len(),
            quire_core::core::template::PRESETS.len()
        );
        assert!(snapshot.blocks.iter().any(|row| row.id == block.0));
        let database = snapshot
            .databases
            .iter()
            .find(|row| row.id == database.0)
            .unwrap();
        assert!(database.records.iter().any(|row| row.id == record.0));
        assert!(snapshot.notes.iter().any(|row| row.id == note.0));
        assert!(snapshot.tasks.iter().any(|row| row.id == task.0));
        Ok(())
    }

    #[test]
    fn built_in_templates_seed_once_and_insert_as_one_undoable_copy() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-templates");
        let path = scratch.path().join("templates.db");
        let deleted_template;
        {
            let mut session = Session::open(&path)?;
            let templates = session.templates();
            assert_eq!(templates.len(), quire_core::core::template::PRESETS.len());
            let meeting = templates
                .iter()
                .find(|page| page.title == "Meeting notes")
                .unwrap();
            let source_blocks = session.document.page_blocks(meeting.id).to_vec();
            let empty = session.blocks()[0].id;
            let inserted = session.insert_template(meeting.id, Some(empty)).unwrap();
            assert_eq!(session.blocks().len(), source_blocks.len());
            assert_eq!(session.blocks()[0].id, inserted);
            assert_eq!(session.blocks()[0].text, source_blocks[0].text);
            assert!(
                session
                    .blocks()
                    .iter()
                    .all(|block| block.id != source_blocks[0].id)
            );
            session.undo();
            assert_eq!(session.blocks().len(), 1);
            assert_eq!(session.blocks()[0].id, empty);
            assert_eq!(session.blocks()[0].text, "");
            session.redo();
            assert_eq!(session.blocks().len(), source_blocks.len());

            let custom_id = session
                .save_page_as_template("Custom draft".into())
                .unwrap();
            let custom = session
                .pages
                .iter()
                .find(|page| page.id == custom_id)
                .unwrap();
            assert!(custom.template);
            assert_eq!(custom.title, "Custom draft");
            assert_eq!(
                session.document.page_blocks(custom_id)[0].text,
                session.blocks()[0].text
            );
            assert_ne!(session.document.page_blocks(custom_id)[0].id, inserted);

            let imported_id = session
                .import_markdown_as_template("Imported markdown".into(), "## Imported\n\n- ")
                .unwrap();
            assert!(session.pages.iter().any(|page| {
                page.id == imported_id && page.template && page.title == "Imported markdown"
            }));
            assert!(
                session
                    .document
                    .page_blocks(imported_id)
                    .iter()
                    .any(|block| block.kind == BlockKind::Bullet && block.text.is_empty())
            );

            let new_page = session
                .create_page_from_template(custom_id, Some("From custom".into()))
                .unwrap();
            assert!(!session.page.template);
            assert_eq!(session.page.id, new_page);
            assert_eq!(session.page.title, "From custom");
            assert_eq!(
                session.blocks()[0].text,
                session.document.page_blocks(custom_id)[0].text
            );
            deleted_template = templates[0].id;
            session.delete_template(deleted_template);
            session.persistence.force_flush()?;
        }
        let reopened = Session::open(&path)?;
        assert_eq!(
            reopened.templates().len(),
            quire_core::core::template::PRESETS.len() + 1
        );
        assert!(
            reopened
                .templates()
                .iter()
                .all(|page| page.id != deleted_template)
        );
        assert!(
            reopened
                .pages
                .iter()
                .any(|page| page.title == "From custom")
        );
        Ok(())
    }

    #[test]
    fn sync_merge_applies_remote_organizer_changes_and_persists_shadow() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-sync-merge");
        let path = scratch.path().join("sync.db");
        let mut session = Session::open(&path)?;
        let template_ids = session
            .templates()
            .iter()
            .map(|page| page.id)
            .collect::<HashSet<_>>();
        let note_id = session.create_note();
        session.update_note_editor(note_id, "Local title".into(), "body".into(), vec![]);
        let mut remote = session.sync_export()?;
        remote.device_id = "remote-device".into();
        remote.device = "Remote".into();
        let remote_note = remote
            .notes
            .iter_mut()
            .find(|note| note.id == note_id.0)
            .unwrap();
        remote_note.title = "Remote title".into();
        remote_note.edited += 1;
        remote_note.rev = "9999999999999-remote-device".into();
        let mut extra_page = remote.pages[0].clone();
        extra_page.id = 999;
        extra_page.title = "Remote page".into();
        extra_page.parent = None;
        remote.pages.push(extra_page);
        let mut extra_block = remote.blocks[0].clone();
        extra_block.id = 999;
        extra_block.page = 999;
        extra_block.parent = None;
        extra_block.text = "remote page content".into();
        remote.blocks.push(extra_block);
        let peer = quire_core::services::sync::engine::PeerRecord {
            id: remote.device_id.clone(),
            name: remote.device.clone(),
            kind: "windows".into(),
            ip: "127.0.0.1".into(),
            port: quire_core::services::sync::SYNC_PORT,
            paired: true,
            last_seen: 0,
            last_sync: String::new(),
        };
        let answer = session.sync_apply_remote(&remote, &[], &peer)?;
        assert!(template_ids.iter().all(|id| {
            session
                .pages
                .iter()
                .any(|page| page.id == *id && page.template)
        }));
        assert_eq!(
            session.organizer.note(note_id).unwrap().title,
            "Remote title"
        );
        assert!(
            session
                .pages
                .iter()
                .any(|page| page.id.0 == 999 && page.title == "Remote page")
        );
        assert!(
            session
                .document
                .page_blocks(quire_core::core::types::PageId(999))
                .iter()
                .any(|block| block.text == "remote page content")
        );
        assert_eq!(answer.device_id, session.sync_self_info().id);
        assert!(session.setting("sync.shadow.remote-device").is_some());
        drop(session);
        let reopened = Session::open(&path)?;
        assert_eq!(
            reopened.organizer.note(note_id).unwrap().title,
            "Remote title"
        );
        assert!(reopened.pages.iter().any(|page| page.id.0 == 999));
        assert!(template_ids.iter().all(|id| {
            reopened
                .pages
                .iter()
                .any(|page| page.id == *id && page.template)
        }));
        Ok(())
    }

    #[test]
    fn undoing_a_formula_config_flushes_before_reopening() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-formula-undo-flush");
        let path = scratch.path().join("test.db");
        let formula_id;
        {
            let mut session = Session::open(&path)?;
            let block = session.blocks()[0].id;
            session
                .make_database(block, "公式撤销".into())
                .expect("database created");
            formula_id = session
                .add_database_property(
                    block,
                    "计算".into(),
                    quire_core::core::database::PropertyKind::Text,
                )
                .expect("property created");
            assert!(session.set_database_property_kind(
                block,
                formula_id,
                quire_core::core::database::PropertyKind::Formula,
            )?);
            assert!(session.set_database_formula(block, formula_id, "1 + 2")?);
            session.undo();
            assert!(
                quire_core::core::database_formula::config_formula(
                    &session
                        .databases
                        .properties
                        .iter()
                        .find(|property| property.id == formula_id)
                        .unwrap()
                        .config
                )
                .is_none()
            );
        }
        let reopened = Session::open(&path)?;
        let property = reopened
            .databases
            .properties
            .iter()
            .find(|property| property.id == formula_id)
            .unwrap();
        assert!(quire_core::core::database_formula::config_formula(&property.config).is_none());
        Ok(())
    }

    #[test]
    fn database_view_layout_is_undoable_and_persists() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-database-view-layout");
        let path = scratch.path().join("test.db");
        let (block, db, view_id);
        {
            let mut session = Session::open(&path)?;
            block = session.blocks()[0].id;
            db = session
                .make_database(block, "布局测试".into())
                .expect("database created");
            view_id = session.databases.views_of(db).next().unwrap().id;
            assert!(session.set_database_view_layout(block, view_id, ViewLayout::List,)?);
            assert_eq!(
                session.databases.views_of(db).next().unwrap().layout,
                ViewLayout::List
            );
            session.undo();
            assert_eq!(
                session.databases.views_of(db).next().unwrap().layout,
                ViewLayout::Table
            );
            session.redo();
        }
        let reopened = Session::open(&path)?;
        assert_eq!(
            reopened.databases.views_of(db).next().unwrap().layout,
            ViewLayout::List
        );
        Ok(())
    }

    #[test]
    fn multiselect_options_and_values_are_undoable_and_persist() -> Result<()> {
        use quire_core::core::database::{CellValue, PropertyKind};

        let scratch = quire_core::testing::ScratchDir::new("gpui-database-multiselect");
        let path = scratch.path().join("test.db");
        let (block, db, property, record);
        {
            let mut session = Session::open(&path)?;
            block = session.blocks()[0].id;
            db = session
                .make_database(block, "多选测试".into())
                .expect("database created");
            property = session
                .add_database_property(block, "标签".into(), PropertyKind::MultiSelect)
                .expect("property created");
            assert!(session.add_database_property_option(block, property, "进行中")?);
            assert!(session.add_database_property_option(block, property, "已完成")?);
            assert!(!session.add_database_property_option(block, property, "进行中")?);
            record = session.add_database_record(block).expect("record created");

            assert!(session.set_database_cell_many(
                block,
                record,
                property,
                &["进行中".into(), "已完成".into()]
            )?);
            let expected = quire_core::core::database_property::PropertyOptions::from_config(
                &session
                    .databases
                    .properties
                    .iter()
                    .find(|definition| definition.id == property)
                    .unwrap()
                    .config,
            );
            assert_eq!(
                session.repo.cell(record, property)?,
                CellValue::Items(
                    expected
                        .iter()
                        .map(|option| option.id.to_string())
                        .collect()
                )
            );
            session.undo();
            assert_eq!(session.repo.cell(record, property)?, CellValue::Empty);
            session.redo();
        }

        let reopened = Session::open(&path)?;
        let definition = reopened
            .databases
            .properties
            .iter()
            .find(|definition| definition.id == property)
            .unwrap();
        assert_eq!(definition.db, db);
        assert_eq!(definition.kind, PropertyKind::MultiSelect);
        assert_eq!(
            quire_core::core::database_property::PropertyOptions::from_config(&definition.config)
                .iter()
                .map(|option| option.name.as_str())
                .collect::<Vec<_>>(),
            vec!["进行中", "已完成"]
        );
        let values =
            quire_core::core::database_property::PropertyOptions::from_config(&definition.config);
        assert_eq!(
            reopened.repo.cell(record, property)?,
            CellValue::Items(values.iter().map(|option| option.id.to_string()).collect())
        );
        Ok(())
    }

    #[test]
    fn select_option_cell_writes_and_undoes_option_id() -> Result<()> {
        use quire_core::core::database::PropertyKind;

        let scratch = quire_core::testing::ScratchDir::new("gpui-database-select-option");
        let path = scratch.path().join("test.db");
        let mut session = Session::open(&path)?;
        let block = session.blocks()[0].id;
        let db = session
            .make_database(block, "单选测试".into())
            .expect("database created");
        let property = session
            .add_database_property(block, "状态".into(), PropertyKind::Select)
            .expect("property created");
        session.add_database_property_option(block, property, "进行中")?;
        let record = session.add_database_record(block).expect("record created");
        let option_id = quire_core::core::database_property::PropertyOptions::from_config(
            &session
                .databases
                .properties
                .iter()
                .find(|definition| definition.id == property)
                .unwrap()
                .config,
        )
        .iter()
        .next()
        .unwrap()
        .id
        .0;
        assert!(session.set_database_property_option_cell(
            block,
            record,
            property,
            Some(option_id)
        )?);
        assert_eq!(
            session.repo.cell(record, property)?,
            quire_core::core::database::CellValue::Text(option_id.to_string())
        );
        session.undo();
        assert_eq!(
            session.repo.cell(record, property)?,
            quire_core::core::database::CellValue::Empty
        );
        assert!(session.set_database_property_option_cell(
            block,
            record,
            property,
            Some(option_id)
        )?);
        assert_eq!(session.databases.database(db).unwrap().id, db);
        Ok(())
    }

    #[test]
    fn database_property_format_is_undoable_and_persists() -> Result<()> {
        use quire_core::core::database::PropertyKind;

        let scratch = quire_core::testing::ScratchDir::new("gpui-database-format");
        let path = scratch.path().join("test.db");
        let (block, property);
        {
            let mut session = Session::open(&path)?;
            block = session.blocks()[0].id;
            session
                .make_database(block, "格式测试".into())
                .expect("database created");
            property = session
                .add_database_property(block, "完成度".into(), PropertyKind::Number)
                .expect("property created");
            assert!(session.set_database_property_format(block, property, "percent")?);
            assert_eq!(
                quire_core::core::database_property::NumberFormat::from_config(
                    &session
                        .databases
                        .properties
                        .iter()
                        .find(|candidate| candidate.id == property)
                        .unwrap()
                        .config
                ),
                quire_core::core::database_property::NumberFormat::Percent
            );
            session.undo();
            assert_eq!(
                quire_core::core::database_property::NumberFormat::from_config(
                    &session
                        .databases
                        .properties
                        .iter()
                        .find(|candidate| candidate.id == property)
                        .unwrap()
                        .config
                ),
                quire_core::core::database_property::NumberFormat::Plain
            );
            session.redo();
        }
        let mut reopened = Session::open(&path)?;
        let definition = reopened
            .databases
            .properties
            .iter()
            .find(|candidate| candidate.id == property)
            .unwrap();
        assert_eq!(
            quire_core::core::database_property::NumberFormat::from_config(&definition.config),
            quire_core::core::database_property::NumberFormat::Percent
        );
        assert!(
            reopened
                .set_database_property_format(block, property, "date")
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn one_way_relation_config_and_cells_are_undoable_and_persist() -> Result<()> {
        use quire_core::core::database::{CellValue, PropertyKind};

        let scratch = quire_core::testing::ScratchDir::new("gpui-database-relation");
        let path = scratch.path().join("test.db");
        let (block, db, property, source, target);
        {
            let mut session = Session::open(&path)?;
            block = session.blocks()[0].id;
            db = session
                .make_database(block, "关系测试".into())
                .expect("database created");
            property = session
                .add_database_property(block, "相关记录".into(), PropertyKind::Relation)
                .expect("property created");
            assert!(session.set_database_relation_target(block, property, Some(db))?);
            source = session.add_database_record(block).expect("source created");
            target = session.add_database_record(block).expect("target created");
            assert!(session.set_database_relation_cell(block, source, property, &[target])?);
            assert_eq!(
                session.repo.cell(source, property)?,
                CellValue::Items(vec![target.0.to_string()])
            );
            session.undo();
            assert_eq!(session.repo.cell(source, property)?, CellValue::Empty);
            session.redo();
            assert!(session.set_database_relation_cell(block, source, property, &[])?);
            assert_eq!(session.repo.cell(source, property)?, CellValue::Empty);
            session.undo();
        }
        let reopened = Session::open(&path)?;
        let definition = reopened
            .databases
            .properties
            .iter()
            .find(|definition| definition.id == property)
            .unwrap();
        assert_eq!(
            quire_core::core::database_relation::config_relation(&definition.config).target,
            Some(db)
        );
        assert_eq!(
            reopened.repo.cell(source, property)?,
            CellValue::Items(vec![target.0.to_string()])
        );
        Ok(())
    }

    #[test]
    fn paired_relation_updates_both_cells_and_undoes_atomically() -> Result<()> {
        use quire_core::core::database::{CellValue, PropertyKind};
        let scratch = quire_core::testing::ScratchDir::new("gpui-paired-relation");
        let path = scratch.path().join("test.db");
        let (block, db, forward, reverse, left, right);
        {
            let mut session = Session::open(&path)?;
            block = session.blocks()[0].id;
            db = session
                .make_database(block, "双向关系".into())
                .expect("database");
            forward = session
                .add_database_property(block, "父项".into(), PropertyKind::Relation)
                .expect("forward");
            reverse = session
                .add_database_property(block, "子项".into(), PropertyKind::Relation)
                .expect("reverse");
            session.set_database_relation_target(block, forward, Some(db))?;
            session.set_database_relation_target(block, reverse, Some(db))?;
            assert!(session.set_database_relation_mirror(block, forward, reverse)?);
            left = session.add_database_record(block).expect("left");
            right = session.add_database_record(block).expect("right");
            assert!(session.set_database_relation_cell(block, left, forward, &[right])?);
            assert_eq!(
                session.repo.cell(right, reverse)?,
                CellValue::Items(vec![left.0.to_string()])
            );
            session.undo();
            assert_eq!(session.repo.cell(left, forward)?, CellValue::Empty);
            assert_eq!(session.repo.cell(right, reverse)?, CellValue::Empty);
            session.redo();
            assert!(session.set_database_relation_cell(block, left, forward, &[])?);
            assert_eq!(session.repo.cell(right, reverse)?, CellValue::Empty);
            session.undo();
        }
        let reopened = Session::open(&path)?;
        let config = quire_core::core::database_relation::config_relation(
            &reopened
                .databases
                .properties
                .iter()
                .find(|p| p.id == forward)
                .unwrap()
                .config,
        );
        assert_eq!(config.mirror, Some(reverse));
        assert_eq!(
            reopened.repo.cell(right, reverse)?,
            CellValue::Items(vec![left.0.to_string()])
        );
        Ok(())
    }

    #[test]
    fn database_file_import_is_undoable_and_survives_reopen() -> Result<()> {
        use quire_core::core::database::{CellValue, PropertyKind};
        let scratch = quire_core::testing::ScratchDir::new("gpui-database-file");
        let path = scratch.path().join("test.db");
        let source_file = scratch.path().join("readme.txt");
        let second_file = scratch.path().join("notes.pdf");
        std::fs::write(&source_file, "file column attachment")?;
        std::fs::write(&second_file, "second attachment")?;
        let (block, property, record, attachment_ids);
        {
            let mut session = Session::open(&path)?;
            block = session.blocks()[0].id;
            session
                .make_database(block, "文件列".into())
                .expect("database");
            property = session
                .add_database_property(block, "附件".into(), PropertyKind::Files)
                .expect("files property");
            record = session.add_database_record(block).expect("record");
            assert!(session.import_database_files(
                block,
                record,
                property,
                &[source_file.clone(), second_file.clone()]
            )?);
            let ids = match session.repo.cell(record, property)? {
                CellValue::Items(ids) => ids,
                value => panic!("unexpected value: {value:?}"),
            };
            attachment_ids = ids
                .iter()
                .map(|id| id.parse::<u64>().expect("attachment id"))
                .collect::<Vec<_>>();
            assert_eq!(attachment_ids.len(), 2);
            assert_eq!(session.attachments[&attachment_ids[0]].name, "readme.txt");
            assert_eq!(session.attachments[&attachment_ids[1]].name, "notes.pdf");
            session.undo();
            assert_eq!(session.repo.cell(record, property)?, CellValue::Empty);
            session.redo();
            assert!(session.set_database_cell_many(
                block,
                record,
                property,
                &[attachment_ids[1].to_string()]
            )?);
            assert_eq!(
                session.repo.cell(record, property)?,
                CellValue::Items(vec![attachment_ids[1].to_string()])
            );
            session.undo();
            assert!(session.set_database_cell_many(block, record, property, &[])?);
            assert_eq!(session.repo.cell(record, property)?, CellValue::Empty);
            session.undo();
        }
        let reopened = Session::open(&path)?;
        assert_eq!(
            reopened.repo.cell(record, property)?,
            CellValue::Items(attachment_ids.iter().map(u64::to_string).collect())
        );
        for (id, name) in attachment_ids.iter().zip(["readme.txt", "notes.pdf"]) {
            assert_eq!(reopened.attachments[id].name, name);
            assert!(
                reopened
                    .attachment_path(quire_core::core::types::AttachmentId(*id))
                    .unwrap()
                    .exists()
            );
        }
        Ok(())
    }

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

    #[test]
    fn database_block_and_rows_use_core_history_and_reopen_from_sqlite() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-database-table");
        let path = scratch.path().join("test.db");
        {
            let mut session = Session::open(&path)?;
            let block = session.blocks()[0].id;
            let db = session
                .make_database(block, "项目库".into())
                .expect("database created");
            assert_eq!(session.databases.database(db).unwrap().name, "项目库");
            let title = session.databases.title_property(db).unwrap().id;
            assert!(
                session
                    .databases
                    .properties_of(db)
                    .any(|property| property.id == title)
            );
            assert_eq!(session.databases.views_of(db).count(), 1);
            let status_property = session
                .add_database_property(
                    block,
                    "状态".into(),
                    quire_core::core::database::PropertyKind::Text,
                )
                .expect("property created");
            assert!(session.rename_database_property(block, status_property, "进度")?);
            assert_eq!(
                session
                    .databases
                    .properties
                    .iter()
                    .find(|property| property.id == status_property)
                    .unwrap()
                    .name,
                "进度"
            );
            session.undo();
            assert_eq!(
                session
                    .databases
                    .properties
                    .iter()
                    .find(|property| property.id == status_property)
                    .unwrap()
                    .name,
                "状态"
            );
            session.redo();
            let done_property = session
                .add_database_property(
                    block,
                    "完成".into(),
                    quire_core::core::database::PropertyKind::Checkbox,
                )
                .expect("checkbox property created");
            let formula_property = session
                .add_database_property(
                    block,
                    "计算".into(),
                    quire_core::core::database::PropertyKind::Text,
                )
                .expect("formula property created");
            assert!(session.set_database_property_kind(
                block,
                formula_property,
                quire_core::core::database::PropertyKind::Formula,
            )?);
            assert!(
                session
                    .set_database_formula(block, formula_property, "1 +")
                    .is_err()
            );
            assert!(
                session
                    .set_database_formula(block, formula_property, "[计算]")
                    .is_err()
            );
            assert!(session.set_database_formula(block, formula_property, "1 + 2")?);
            session.undo();
            assert!(
                quire_core::core::database_formula::config_formula(
                    &session
                        .databases
                        .properties
                        .iter()
                        .find(|property| property.id == formula_property)
                        .unwrap()
                        .config
                )
                .is_none()
            );
            session.redo();
            let view_id = session.add_database_view(block).expect("view created");
            assert_eq!(session.databases.views_of(db).count(), 2);
            session.undo();
            assert_eq!(session.databases.views_of(db).count(), 1);
            session.redo();
            assert_eq!(session.databases.views_of(db).count(), 2);
            assert!(session.rename_database_view(block, view_id, "任务清单")?);
            assert_eq!(
                session
                    .databases
                    .views
                    .iter()
                    .find(|view| view.id == view_id)
                    .unwrap()
                    .name,
                "任务清单"
            );
            session.undo();
            assert_eq!(
                session
                    .databases
                    .views
                    .iter()
                    .find(|view| view.id == view_id)
                    .unwrap()
                    .name,
                "表格 2"
            );
            session.redo();
            let mut definition = quire_core::core::database_view::ViewDefinition::new();
            definition.set_sorts(&[quire_core::core::database::SortSpec::of(
                session.databases.title_property(db).unwrap(),
                false,
            )
            .unwrap()]);
            let sorted_definition = definition.to_text();
            assert!(session.set_database_view_definition(
                block,
                view_id,
                sorted_definition.clone()
            )?);
            session.set_setting(
                format!("gpui.database-view.{}", db.0),
                view_id.0.to_string(),
            );
            session.undo();
            assert!(
                session
                    .databases
                    .views
                    .iter()
                    .find(|view| view.id == view_id)
                    .unwrap()
                    .definition
                    .is_empty()
            );
            session.redo();
            assert_eq!(
                session
                    .databases
                    .views
                    .iter()
                    .find(|view| view.id == view_id)
                    .unwrap()
                    .definition,
                sorted_definition
            );
            let all_column_ids =
                quire_core::core::database_view::all_columns(&session.databases, db);
            let mut definition =
                quire_core::core::database_view::ViewDefinition::parse(&sorted_definition);
            definition.hide(done_property, &all_column_ids);
            let hidden_done_definition = definition.to_text();
            assert!(session.set_database_view_definition(
                block,
                view_id,
                hidden_done_definition.clone()
            )?);
            session.undo();
            assert!(
                quire_core::core::database_view::view_columns(
                    &session.databases,
                    db,
                    session
                        .databases
                        .views
                        .iter()
                        .find(|view| view.id == view_id)
                        .unwrap()
                )
                .iter()
                .any(|column| column.id == done_property)
            );
            session.redo();
            assert_eq!(
                session
                    .databases
                    .views
                    .iter()
                    .find(|view| view.id == view_id)
                    .unwrap()
                    .definition,
                hidden_done_definition
            );
            let record = session.add_database_record(block).expect("record created");
            assert!(session.set_database_cell(block, record, title, "进行中")?);
            assert!(session.set_database_cell(block, record, status_property, "12")?);
            assert_eq!(
                session.repo.cell(record, status_property)?,
                CellValue::Text("12".into())
            );
            assert!(session.set_database_property_kind(
                block,
                status_property,
                quire_core::core::database::PropertyKind::Number,
            )?);
            assert_eq!(
                session.repo.cell(record, status_property)?,
                CellValue::Empty,
                "text is not presented as a number until the user re-enters it"
            );
            session.undo();
            assert_eq!(
                session
                    .databases
                    .properties
                    .iter()
                    .find(|property| property.id == status_property)
                    .unwrap()
                    .kind,
                quire_core::core::database::PropertyKind::Text
            );
            assert_eq!(
                session.repo.cell(record, status_property)?,
                CellValue::Text("12".into()),
                "undo must reveal the untouched value"
            );
            session.redo();
            assert!(session.set_database_cell(block, record, done_property, "true")?);
            assert_eq!(
                session.repo.cell(record, title)?,
                CellValue::Text("进行中".into())
            );
            assert_eq!(
                session.repo.cell(record, status_property)?,
                CellValue::Empty
            );
            assert_eq!(
                session.repo.cell(record, done_property)?,
                CellValue::Flag(true)
            );
            session.undo();
            assert_eq!(session.repo.cell(record, done_property)?, CellValue::Empty);
            session.redo();
            let second_record = session
                .add_database_record(block)
                .expect("second record created");
            assert!(session.set_database_cell(block, second_record, title, "Alpha")?);
            session.persistence.force_flush()?;
            assert_eq!(session.repo.record_count(db)?, 2);
            assert_eq!(session.repo.record(record)?.unwrap().db, db);
            let active_view = session
                .databases
                .views
                .iter()
                .find(|view| view.id == view_id)
                .unwrap();
            let columns =
                quire_core::core::database_view::view_columns(&session.databases, db, active_view)
                    .into_iter()
                    .filter(|property| !property.kind.is_title())
                    .collect::<Vec<_>>();
            let rules =
                quire_core::core::database_view::ViewDefinition::parse(&active_view.definition)
                    .rules(db, &session.databases);
            let rows = session.repo.window_rows(
                &quire_core::core::database::RowRequest {
                    db,
                    title,
                    columns: &columns,
                    sorts: &rules.sorts,
                    filter: rules.filter.as_ref(),
                    search: None,
                },
                quire_core::core::database::RowWindow { start: 0, end: 20 },
            )?;
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].cells.len(), 2);
            assert_eq!(rows[0].title, "Alpha");
            assert_eq!(rows[1].title, "进行中");
            assert_eq!(rows[1].cells[0], "");
            assert_eq!(rows[0].cells[1], "");
            assert_eq!(rows[1].cells[1], "");
            assert!(!columns.iter().any(|column| column.id == done_property));
            assert_eq!(
                session
                    .repo
                    .filtered_count(&quire_core::core::database::RowRequest {
                        db,
                        title,
                        columns: &columns,
                        sorts: &rules.sorts,
                        filter: rules.filter.as_ref(),
                        search: Some("Alpha"),
                    })?,
                1
            );
            assert!(session.delete_database_record(block, second_record)?);
            assert_eq!(session.repo.record_count(db)?, 1);
            session.undo();
            assert_eq!(session.repo.record_count(db)?, 2);
            session.redo();
            assert_eq!(session.repo.record_count(db)?, 1);
            session.undo();
            session.persistence.force_flush()?;
            assert_eq!(session.repo.record_count(db)?, 2);
        }
        let mut reopened = Session::open(&path)?;
        let block = reopened
            .blocks()
            .iter()
            .find(|block| block.kind == BlockKind::Database)
            .unwrap();
        let db = block.db_ref.unwrap();
        assert_eq!(reopened.databases.database(db).unwrap().name, "项目库");
        assert_eq!(
            reopened.setting(&format!("gpui.database-view.{}", db.0)),
            Some("2")
        );
        assert!(
            reopened
                .databases
                .properties_of(db)
                .any(|property| property.name == "进度")
        );
        assert_eq!(
            reopened
                .databases
                .properties_of(db)
                .find(|property| property.name == "进度")
                .unwrap()
                .kind,
            quire_core::core::database::PropertyKind::Number
        );
        assert_eq!(reopened.databases.views_of(db).count(), 2);
        assert!(
            reopened
                .databases
                .views_of(db)
                .any(|view| view.name == "任务清单")
        );
        assert_eq!(reopened.repo.record_count(db)?, 2);
        assert!(
            reopened
                .databases
                .views_of(db)
                .any(|view| view.definition.contains("sorts"))
        );
        let active_view = reopened
            .databases
            .views_of(db)
            .find(|view| view.definition.contains("sorts"))
            .unwrap();
        assert!(
            !quire_core::core::database_view::view_columns(&reopened.databases, db, active_view)
                .iter()
                .any(|column| column.name == "完成")
        );
        let record = reopened.repo.records_of(db)?[0].id;
        let title = reopened.databases.title_property(db).unwrap().id;
        let status = reopened
            .databases
            .properties_of(db)
            .find(|property| property.name == "进度")
            .unwrap()
            .id;
        let done = reopened
            .databases
            .properties_of(db)
            .find(|property| property.name == "完成")
            .unwrap()
            .id;
        assert_eq!(
            reopened.repo.cell(record, title)?,
            CellValue::Text("进行中".into())
        );
        assert_eq!(reopened.repo.cell(record, status)?, CellValue::Empty);
        assert!(reopened.set_database_property_kind(
            block.id,
            status,
            quire_core::core::database::PropertyKind::Text,
        )?);
        assert_eq!(
            reopened.repo.cell(record, status)?,
            CellValue::Text("12".into()),
            "restoring the prior type after reopening must reveal the stored value"
        );
        assert_eq!(reopened.repo.cell(record, done)?, CellValue::Flag(true));
        Ok(())
    }

    #[test]
    fn replacing_link_target_preserves_outer_ranges_and_undoes_as_one_entry() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-link-replace");
        let path = scratch.path().join("test.db");
        let mut session = Session::open(&path)?;
        let id = session.blocks()[0].id;
        session.replace(id, "alpha beta gamma".into(), false);
        session.set_link(id, 0, 16, "https://old.example".into());

        session.set_link(id, 6, 10, "https://new.example".into());
        let links = session.document.block(id).unwrap().marks.clone();
        assert_eq!(links.len(), 3);
        assert_eq!((links[0].start, links[0].end), (0, 6));
        assert_eq!(links[0].url, "https://old.example");
        assert_eq!((links[1].start, links[1].end), (6, 10));
        assert_eq!(links[1].url, "https://new.example");
        assert_eq!((links[2].start, links[2].end), (10, 16));
        assert_eq!(links[2].url, "https://old.example");

        session.undo();
        let links = &session.document.block(id).unwrap().marks;
        assert_eq!(links.len(), 1);
        assert_eq!((links[0].start, links[0].end), (0, 16));
        assert_eq!(links[0].url, "https://old.example");
        session.redo();
        let links = &session.document.block(id).unwrap().marks;
        assert_eq!(links.len(), 3);
        assert_eq!(links[1].url, "https://new.example");
        Ok(())
    }

    #[test]
    fn markdown_import_creates_a_new_persisted_page() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-markdown-import");
        let path = scratch.path().join("test.db");
        let imported_page;
        {
            let mut session = Session::open(&path)?;
            let original_page = session.page.id;
            imported_page = session.import_markdown(
                "Imported notes".into(),
                "# Heading\n\n- first item\n- second item\n\n[site](https://example.com)\n",
            );
            assert_ne!(original_page, imported_page);
            assert_eq!(session.page.id, imported_page);
            assert_eq!(session.page.title, "Imported notes");
            let blocks = session.blocks();
            assert_eq!(blocks.len(), 4);
            assert_eq!(blocks[0].kind, BlockKind::Heading1);
            assert_eq!(blocks[1].kind, BlockKind::Bullet);
            assert_eq!(blocks[2].kind, BlockKind::Bullet);
            assert_eq!(blocks[3].marks[0].url, "https://example.com");
            session.persistence.force_flush()?;
        }
        let reopened = Session::open(&path)?;
        assert!(
            reopened
                .pages
                .iter()
                .any(|page| { page.id == imported_page && page.title == "Imported notes" })
        );
        let imported = reopened.document.page_blocks(imported_page);
        assert_eq!(imported.len(), 4);
        assert_eq!(imported[0].text, "Heading");
        assert_eq!(imported[3].marks[0].url, "https://example.com");
        Ok(())
    }

    #[test]
    fn backlink_projection_finds_page_blocks_after_flush_and_reopen() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-backlinks");
        let path = scratch.path().join("test.db");
        let (target_page, source_page, source_block);
        {
            let mut session = Session::open(&path)?;
            target_page = session.page.id;
            source_page = session.create_page(None);
            source_block = session.blocks()[0].id;
            session.execute(Command::SetBlockType {
                id: source_block,
                kind: BlockKind::Page,
            });
            session.document.apply(&[Change::BlockRefSet {
                id: source_block,
                page: Some(target_page),
            }]);
            session.persistence.record(vec![Change::BlockRefSet {
                id: source_block,
                page: Some(target_page),
            }]);
            session.persistence.force_flush()?;
            let refs = session.repo.references(target_page, 20)?;
            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].page, source_page);
            assert_eq!(refs[0].block, source_block);
            assert!(refs[0].block_level);
            assert_eq!(session.repo.reference_count(target_page)?, 1);
        }
        let reopened = Session::open(&path)?;
        let refs = reopened.repo.references(target_page, 20)?;
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].page, source_page);
        assert_eq!(refs[0].block, source_block);
        Ok(())
    }

    #[test]
    fn synced_block_follows_source_and_undoes_its_reference() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-synced-block");
        let path = scratch.path().join("test.db");
        let mut session = Session::open(&path)?;
        let source = session.blocks()[0].id;
        session.execute(Command::ReplaceText {
            id: source,
            text: "live source".into(),
        });
        session.create_page(None);
        let synced = session.blocks()[0].id;
        session.execute(Command::SetBlockType {
            id: synced,
            kind: BlockKind::Synced,
        });
        assert!(session.set_sync_source(synced, Some(source)));
        assert_eq!(
            session.document.block(synced).unwrap().sync_ref,
            Some(source)
        );
        assert!(session.document.block(synced).unwrap().text.is_empty());
        session.undo();
        assert_eq!(session.document.block(synced).unwrap().sync_ref, None);
        session.redo();
        session.persistence.force_flush()?;
        drop(session);
        let reopened = Session::open(&path)?;
        assert_eq!(
            reopened.document.block(synced).unwrap().sync_ref,
            Some(source)
        );
        assert_eq!(reopened.document.block(source).unwrap().text, "live source");
        Ok(())
    }

    #[test]
    fn page_reference_clears_old_text_and_undoes_target_change() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-page-reference");
        let path = scratch.path().join("test.db");
        let (source_page, target_page, block_id);
        {
            let mut session = Session::open(&path)?;
            source_page = session.page.id;
            target_page = session.create_page(None);
            block_id = session.blocks()[0].id;
            session.replace(block_id, "old block label".into(), true);
            session.execute(Command::SetBlockType {
                id: block_id,
                kind: BlockKind::Link,
            });
            assert!(session.set_page_reference(block_id, Some(source_page)));
            assert_eq!(
                session.document.block(block_id).unwrap().page_ref,
                Some(source_page)
            );
            assert!(session.document.block(block_id).unwrap().text.is_empty());
            session.undo();
            assert_eq!(session.document.block(block_id).unwrap().page_ref, None);
            assert_eq!(
                session.document.block(block_id).unwrap().text,
                "old block label"
            );
            session.redo();
            session.persistence.force_flush()?;
        }
        let reopened = Session::open(&path)?;
        assert_eq!(
            reopened.document.block(block_id).unwrap().page_ref,
            Some(source_page)
        );
        assert!(reopened.pages.iter().any(|page| page.id == target_page));
        Ok(())
    }

    #[test]
    fn page_mention_is_an_atomic_undoable_reference() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-page-mention");
        let path = scratch.path().join("test.db");
        let (target_page, source_page, block_id);
        {
            let mut session = Session::open(&path)?;
            target_page = session.page.id;
            session.create_page(None);
            source_page = session.page.id;
            block_id = session.blocks()[0].id;
            session.execute(Command::ReplaceText {
                id: block_id,
                text: "see @未".into(),
            });
            session.execute(Command::InsertReference {
                id: block_id,
                at: 4,
                label: "未命名".into(),
                kind: quire_core::core::types::MarkKind::Mention,
                url: quire_core::core::page_uri(target_page),
                date: None,
            });
            let block = session.document.block(block_id).unwrap();
            assert_eq!(block.page, source_page);
            assert_eq!(block.text, "see 未命名");
            assert_eq!(block.marks.len(), 1);
            assert_eq!(
                block.marks[0].kind,
                quire_core::core::types::MarkKind::Mention
            );
            assert_eq!(block.marks[0].url, quire_core::core::page_uri(target_page));
            session.undo();
            assert_eq!(session.document.block(block_id).unwrap().text, "see @未");
            assert!(session.document.block(block_id).unwrap().marks.is_empty());
            session.redo();
            session.persistence.force_flush()?;
        }
        let reopened = Session::open(&path)?;
        let mention = &reopened.document.block(block_id).unwrap().marks[0];
        assert_eq!(mention.kind, quire_core::core::types::MarkKind::Mention);
        assert_eq!(mention.url, quire_core::core::page_uri(target_page));
        Ok(())
    }

    #[test]
    fn organizer_capture_suggestions_match_full_or_leaf_tags_and_cap_results() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-capture-tags");
        let mut session = Session::open(&scratch.path().join("capture.db"))?;
        let note = session.create_note();
        session.update_note_editor(
            note,
            "tag source".into(),
            String::new(),
            vec![
                "设计/复刻".into(),
                "工程/复刻".into(),
                "复刻".into(),
                "项目".into(),
                "计划".into(),
                "窗口".into(),
                "主题".into(),
            ],
        );
        let task = session.create_task("task source".into());
        session.set_task_tags(task, vec!["设计".into()]);

        assert_eq!(
            session.organizer_tag_suggestions("记录 #复"),
            ["复刻", "工程/复刻", "设计/复刻"]
        );
        assert_eq!(
            session.organizer_tag_suggestions("记录 #设计"),
            ["设计", "设计/复刻"]
        );
        assert_eq!(session.organizer_tag_suggestions("记录 #").len(), 5);
        assert!(
            session
                .organizer_tag_suggestions("记录 #复 别的")
                .is_empty()
        );
        assert!(session.organizer_tag_suggestions("没有标签").is_empty());
        Ok(())
    }

    #[test]
    fn organizer_delete_waits_for_undo_window_and_superseding_delete_commits_previous() -> Result<()>
    {
        let scratch = quire_core::testing::ScratchDir::new("gpui-organizer-deferred-delete");
        let mut session = Session::open(&scratch.path().join("test.db"))?;
        let first = session.create_note();
        let second = session.create_note();
        let (first_token, committed) = session.defer_trash_notes(&[first], "delete first".into());
        assert!(!committed);
        assert!(session.note_delete_is_pending(first));
        assert!(!session.organizer.note(first).unwrap().is_trashed());

        let (second_token, committed) =
            session.defer_trash_notes(&[second], "delete second".into());
        assert!(committed);
        assert!(session.organizer.note(first).unwrap().is_trashed());
        assert!(session.note_delete_is_pending(second));
        assert!(!session.commit_pending_organizer_delete(first_token));
        assert!(session.undo_pending_organizer_delete());
        assert!(!session.organizer.note(second).unwrap().is_trashed());

        let (final_token, committed) = session.defer_trash_notes(&[second], "delete second".into());
        assert!(!committed);
        assert!(session.commit_pending_organizer_delete(final_token));
        assert!(!session.commit_pending_organizer_delete(second_token));
        assert!(session.organizer.note(second).unwrap().is_trashed());
        assert!(session.undo_organizer());
        assert!(!session.organizer.note(second).unwrap().is_trashed());

        let task = session.create_task("临时任务".into());
        let (task_token, committed) = session.defer_trash_tasks(&[task], "delete task".into());
        assert!(!committed);
        assert!(session.task_delete_is_pending(task));
        assert!(!session.organizer.task(task).unwrap().is_trashed());
        assert!(session.commit_pending_organizer_delete(task_token));
        assert!(session.organizer.task(task).unwrap().is_trashed());
        assert!(session.undo_organizer());
        assert!(!session.organizer.task(task).unwrap().is_trashed());
        Ok(())
    }

    #[test]
    fn organizer_note_crud_undo_and_reopen_use_core_changes() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-organizer-note");
        let path = scratch.path().join("test.db");
        let note_id;
        {
            let mut session = Session::open(&path)?;
            note_id = session.create_note();
            assert_eq!(session.organizer.note(note_id).unwrap().title, "新建笔记");
            assert!(session.update_note_content(
                note_id,
                "项目记录".into(),
                "第一行\n第二行".into(),
            ));
            let note = session.organizer.note(note_id).unwrap();
            assert_eq!(note.title, "项目记录");
            assert_eq!(note.body, "第一行\n第二行");
            assert!(session.undo_organizer());
            assert_eq!(session.organizer.note(note_id).unwrap().title, "新建笔记");
            assert!(session.redo_organizer());
            assert!(session.update_note_editor(
                note_id,
                "项目记录".into(),
                "第一行\n第二行".into(),
                vec!["设计".into(), "复刻".into()],
            ));
            assert_eq!(
                session.organizer.note(note_id).unwrap().tags,
                ["设计", "复刻"]
            );
            assert!(session.undo_organizer());
            assert!(session.organizer.note(note_id).unwrap().tags.is_empty());
            assert!(session.redo_organizer());
            assert!(session.set_note_pinned(note_id, true));
            assert!(session.organizer.note(note_id).unwrap().pinned);
            assert!(session.set_note_trashed(note_id, true));
            assert!(session.organizer.note(note_id).unwrap().is_trashed());
            assert!(session.undo_organizer());
            assert!(!session.organizer.note(note_id).unwrap().is_trashed());
            assert!(session.redo_organizer());
            session.persistence.force_flush()?;
        }
        let reopened = Session::open(&path)?;
        let note = reopened.organizer.note(note_id).unwrap();
        assert_eq!(note.title, "项目记录");
        assert_eq!(note.body, "第一行\n第二行");
        assert_eq!(note.tags, ["设计", "复刻"]);
        assert!(!note.uuid.is_empty());
        assert!(note.pinned);
        assert!(note.is_trashed());
        Ok(())
    }

    #[test]
    fn organizer_task_crud_completion_and_trash_use_core_history() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-organizer-task");
        let path = scratch.path().join("test.db");
        let task_id;
        let bulk_task_id;
        let list_id;
        {
            let mut session = Session::open(&path)?;
            task_id = session.create_task("发布 GPUI 版".into());
            bulk_task_id = session.create_task("检查安装包".into());
            assert!(session.set_tasks_done(&[task_id, bulk_task_id], true));
            assert!(session.organizer.task(task_id).unwrap().done);
            assert!(session.organizer.task(bulk_task_id).unwrap().done);
            assert!(session.undo_organizer());
            assert!(!session.organizer.task(task_id).unwrap().done);
            assert!(!session.organizer.task(bulk_task_id).unwrap().done);
            assert!(session.set_tasks_trashed(&[task_id, bulk_task_id], true));
            assert!(session.organizer.task(task_id).unwrap().is_trashed());
            assert!(session.organizer.task(bulk_task_id).unwrap().is_trashed());
            assert!(session.undo_organizer());
            assert!(!session.organizer.task(task_id).unwrap().is_trashed());
            assert!(!session.organizer.task(bulk_task_id).unwrap().is_trashed());
            assert_eq!(session.organizer.task(task_id).unwrap().list, ListId::INBOX);
            assert!(session.update_task_content(
                task_id,
                "发布候选版".into(),
                "检查安装包和回退说明".into(),
            ));
            assert!(session.set_task_priority(task_id, Priority::High));
            assert!(session.set_task_repeat(task_id, Repeat::Weekly));
            assert!(session.set_task_tags(task_id, vec!["发布".into(), "桌面端".into()]));
            assert!(session.undo_organizer());
            assert!(session.organizer.task(task_id).unwrap().tags.is_empty());
            assert!(session.redo_organizer());
            assert!(session.add_subtask(task_id, "核对安装包".into()));
            let subtask_id = session.organizer.task(task_id).unwrap().subtasks[0].id;
            assert!(session.set_subtask_done(task_id, subtask_id, true));
            assert!(session.undo_organizer());
            assert!(!session.organizer.task(task_id).unwrap().subtasks[0].done);
            assert!(session.redo_organizer());
            assert!(session.set_subtask_title(task_id, subtask_id, "核验发布包".into()));
            assert!(session.undo_organizer());
            assert_eq!(
                session.organizer.task(task_id).unwrap().subtasks[0].title,
                "核对安装包"
            );
            assert!(session.redo_organizer());
            assert!(session.set_task_due(task_id, Some("2026-11-02".into())));
            assert!(!session.set_task_due(task_id, Some("下周".into())));
            assert!(session.update_task_editor(
                task_id,
                "发布候选版".into(),
                "检查安装包和回退说明".into(),
                Some("2026-11-03".into()),
            ));
            assert!(session.set_task_done(task_id, true));
            assert!(
                session
                    .organizer
                    .task(task_id)
                    .unwrap()
                    .completed_at
                    .is_some()
            );
            assert!(session.undo_organizer());
            assert!(!session.organizer.task(task_id).unwrap().done);
            assert!(session.set_task_trashed(task_id, true));
            assert!(session.organizer.task(task_id).unwrap().is_trashed());
            assert!(session.undo_organizer());
            assert!(!session.organizer.task(task_id).unwrap().is_trashed());
            list_id = session.create_task_list("工作".into());
            let original_color = session.organizer.list(list_id).unwrap().color;
            assert!(session.update_task_list(list_id, "  项目发布  ".into(), ColorKind::Blue));
            assert_eq!(session.organizer.list(list_id).unwrap().name, "项目发布");
            assert_eq!(
                session.organizer.list(list_id).unwrap().color,
                ColorKind::Blue
            );
            assert!(session.undo_organizer());
            assert_eq!(session.organizer.list(list_id).unwrap().name, "工作");
            assert_eq!(
                session.organizer.list(list_id).unwrap().color,
                original_color
            );
            assert!(session.redo_organizer());
            assert!(!session.update_task_list(list_id, "   ".into(), ColorKind::Red));
            assert!(session.set_task_list(task_id, list_id));
            assert_eq!(session.organizer.task(task_id).unwrap().list, list_id);
            assert!(session.undo_organizer());
            assert_eq!(session.organizer.task(task_id).unwrap().list, ListId::INBOX);
            assert!(session.redo_organizer());
            assert_eq!(session.delete_task_list(list_id), Some(1));
            assert!(session.organizer.list(list_id).is_none());
            assert_eq!(session.organizer.task(task_id).unwrap().list, ListId::INBOX);
            assert!(session.undo_organizer());
            assert_eq!(session.organizer.list(list_id).unwrap().name, "项目发布");
            assert_eq!(
                session.organizer.list(list_id).unwrap().color,
                ColorKind::Blue
            );
            assert_eq!(session.organizer.task(task_id).unwrap().list, list_id);
            session.persistence.force_flush()?;
        }
        let reopened = Session::open(&path)?;
        let task = reopened.organizer.task(task_id).unwrap();
        assert_eq!(task.title, "发布候选版");
        assert_eq!(task.notes, "检查安装包和回退说明");
        assert_eq!(task.priority, Priority::High);
        assert_eq!(task.repeat, Repeat::Weekly);
        assert_eq!(task.tags, ["发布", "桌面端"]);
        assert_eq!(task.subtasks.len(), 1);
        assert!(task.subtasks[0].done);
        assert_eq!(task.subtasks[0].title, "核验发布包");
        assert_eq!(task.due.as_deref(), Some("2026-11-03"));
        assert!(!task.done);
        assert!(!task.is_trashed());
        assert_eq!(task.list, list_id);
        assert_eq!(reopened.organizer.list(list_id).unwrap().name, "项目发布");
        assert_eq!(
            reopened.organizer.list(list_id).unwrap().color,
            ColorKind::Blue
        );
        Ok(())
    }

    #[test]
    fn page_duplication_remaps_nested_blocks_and_preserves_content() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-page-duplicate");
        let path = scratch.path().join("test.db");
        let source_page;
        let duplicate_page;
        {
            let mut session = Session::open(&path)?;
            source_page = session.page.id;
            let root_id = session.blocks()[0].id;
            session.execute(Command::SetBlockType {
                id: root_id,
                kind: BlockKind::Toggle,
            });
            session.replace(root_id, "Root title".into(), false);
            session.set_link(root_id, 0, 4, "https://example.com".into());
            let mut child = session.document.block(root_id).unwrap().clone();
            child.id = session.document.alloc_block_id();
            child.page = source_page;
            child.parent = Some(root_id);
            child.order = OrderKey::between(Some(child.order), None).unwrap();
            child.kind = BlockKind::Paragraph;
            child.text = "Nested child".into();
            child.marks.clear();
            session
                .document
                .apply(&[Change::BlockInserted(child.clone())]);
            session
                .persistence
                .record(vec![Change::BlockInserted(child)]);

            duplicate_page = session.duplicate_page(source_page).unwrap();
            assert_ne!(source_page, duplicate_page);
            assert_eq!(session.page.title, "未命名 副本");
            let copies = session.blocks();
            assert_eq!(copies.len(), 2);
            assert_eq!(copies[0].text, "Root title");
            assert_eq!(copies[0].kind, BlockKind::Toggle);
            assert_eq!(copies[0].marks[0].url, "https://example.com");
            assert_ne!(copies[0].id, root_id);
            assert_eq!(copies[1].text, "Nested child");
            assert_eq!(copies[1].parent, Some(copies[0].id));
            assert_ne!(copies[1].parent, Some(root_id));
            session.persistence.force_flush()?;
        }
        let reopened = Session::open(&path)?;
        let copies = reopened.document.page_blocks(duplicate_page);
        assert_eq!(copies.len(), 2);
        assert_eq!(copies[1].parent, Some(copies[0].id));
        assert_eq!(copies[0].marks[0].url, "https://example.com");
        Ok(())
    }

    #[test]
    fn nested_block_duplicate_copies_its_subtree_as_one_undo_step() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-block-duplicate");
        let path = scratch.path().join("test.db");
        let mut session = Session::open(&path)?;
        let root_id = session.blocks()[0].id;
        session.execute(Command::SetBlockType {
            id: root_id,
            kind: BlockKind::Toggle,
        });
        session.replace(root_id, "Toggle".into(), false);
        let mut child = session.document.block(root_id).unwrap().clone();
        child.id = session.document.alloc_block_id();
        child.parent = Some(root_id);
        child.order = OrderKey::between(Some(child.order), None).unwrap();
        child.kind = BlockKind::Paragraph;
        child.text = "Nested".into();
        child.marks.clear();
        let child_id = child.id;
        session
            .document
            .apply(&[Change::BlockInserted(child.clone())]);
        session
            .persistence
            .record(vec![Change::BlockInserted(child)]);

        let copy_id = session.duplicate_block(root_id).unwrap();
        let copied_root = session.document.block(copy_id).unwrap();
        assert_eq!(copied_root.kind, BlockKind::Toggle);
        assert!(!copied_root.folded);
        let copied_child = session
            .blocks()
            .iter()
            .find(|block| block.parent == Some(copy_id))
            .unwrap();
        assert_eq!(copied_child.text, "Nested");
        assert_ne!(
            copied_child.id,
            session.document.block(child_id).unwrap().id
        );

        session.undo();
        assert!(session.document.block(copy_id).is_none());
        assert_eq!(session.blocks().len(), 2);
        session.redo();
        assert_eq!(session.blocks().len(), 4);
        assert_eq!(
            session
                .blocks()
                .iter()
                .find(|block| block.parent == Some(copy_id))
                .unwrap()
                .text,
            "Nested"
        );
        Ok(())
    }

    #[test]
    fn cross_page_block_move_is_atomic_undoable_and_persistent() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-block-move-page");
        let path = scratch.path().join("test.db");
        let source_page;
        let target_page;
        let locked_page;
        let block_id;
        let child_id;
        {
            let mut session = Session::open(&path)?;
            source_page = session.page.id;
            block_id = session.blocks()[0].id;
            session.execute(Command::SetBlockType {
                id: block_id,
                kind: BlockKind::Toggle,
            });
            session.replace(block_id, "Move me".into(), false);
            let mut child = session.document.block(block_id).unwrap().clone();
            child.id = session.document.alloc_block_id();
            child.parent = Some(block_id);
            child.order = OrderKey::between(Some(child.order), None).unwrap();
            child.kind = BlockKind::Paragraph;
            child.text = "Nested content".into();
            child.marks.clear();
            child_id = child.id;
            session
                .document
                .apply(&[Change::BlockInserted(child.clone())]);
            session
                .persistence
                .record(vec![Change::BlockInserted(child)]);
            locked_page = session.import_markdown("Read-only destination".into(), "Locked target");
            session.set_page_locked(true);
            assert!(session.select_page(source_page));
            assert!(!session.move_block_to_page(block_id, locked_page));
            assert_eq!(session.document.block(block_id).unwrap().page, source_page);
            target_page = session.import_markdown("Destination".into(), "Existing target text");
            assert!(session.select_page(source_page));
            assert!(session.move_block_to_page(block_id, target_page));
            assert_eq!(session.document.block(block_id).unwrap().page, target_page);
            assert_eq!(session.document.block(child_id).unwrap().page, target_page);
            assert_eq!(
                session.document.block(child_id).unwrap().parent,
                Some(block_id)
            );
            assert!(session.document.page_blocks(source_page).is_empty());

            session.undo();
            assert_eq!(session.document.block(block_id).unwrap().page, source_page);
            assert_eq!(session.document.block(child_id).unwrap().page, source_page);
            assert_eq!(session.document.page_blocks(source_page)[0].text, "Move me");
            assert_eq!(session.document.page_blocks(target_page).len(), 1);

            session.redo();
            assert_eq!(session.document.block(block_id).unwrap().page, target_page);
            assert_eq!(session.document.block(child_id).unwrap().page, target_page);
            assert_eq!(
                session.document.block(child_id).unwrap().parent,
                Some(block_id)
            );
            let target_blocks = session.document.page_blocks(target_page);
            assert_eq!(target_blocks.len(), 3);
            assert_eq!(target_blocks[1].id, block_id);
            assert_eq!(target_blocks[2].id, child_id);
            assert!(session.select_page(source_page));
            session.persistence.force_flush()?;
        }

        let reopened = Session::open(&path)?;
        assert_eq!(reopened.document.block(block_id).unwrap().page, target_page);
        assert_eq!(reopened.document.block(child_id).unwrap().page, target_page);
        assert_eq!(
            reopened.document.block(child_id).unwrap().parent,
            Some(block_id)
        );
        assert_eq!(reopened.document.page_blocks(source_page).len(), 1);
        assert!(
            reopened
                .document
                .page_blocks(target_page)
                .iter()
                .any(|block| block.id == block_id && block.text == "Move me")
        );
        Ok(())
    }

    #[test]
    fn organizer_bin_purge_and_empty_are_undoable_and_persist() -> Result<()> {
        let scratch = quire_core::testing::ScratchDir::new("gpui-organizer-bin-purge");
        let path = scratch.path().join("test.db");
        let (note_a, note_b, task_a, task_b);
        {
            let mut session = Session::open(&path)?;
            note_a = session.create_note();
            note_b = session.create_note();
            task_a = session.create_task("第一项".into());
            task_b = session.create_task("第二项".into());
            assert!(session.set_note_trashed(note_a, true));
            assert!(session.set_note_trashed(note_b, true));
            assert!(session.set_task_trashed(task_a, true));
            assert!(session.set_task_trashed(task_b, true));

            assert!(session.purge_organizer_note(note_a));
            assert!(session.organizer.note(note_a).is_none());
            assert!(session.undo_organizer());
            assert!(session.organizer.note(note_a).unwrap().is_trashed());
            assert!(session.redo_organizer());
            assert!(session.organizer.note(note_a).is_none());
            assert!(session.undo_organizer());

            assert_eq!(session.empty_organizer_bin(false), 2);
            assert!(session.organizer.trashed_notes().next().is_none());
            assert_eq!(session.organizer.trashed_tasks().count(), 2);
            assert!(session.undo_organizer());
            assert_eq!(session.organizer.trashed_notes().count(), 2);

            assert!(session.purge_organizer_task(task_a));
            assert!(session.organizer.task(task_a).is_none());
            assert!(session.undo_organizer());
            assert!(session.organizer.task(task_a).unwrap().is_trashed());
            assert!(session.redo_organizer());
            assert!(session.organizer.task(task_a).is_none());
            assert!(session.undo_organizer());

            assert_eq!(session.empty_organizer_bin(true), 2);
            assert!(session.organizer.trashed_tasks().next().is_none());
            assert!(session.undo_organizer());
            assert_eq!(session.organizer.trashed_tasks().count(), 2);
            assert_eq!(session.empty_organizer_bin(true), 2);
            assert!(session.organizer.trashed_tasks().next().is_none());
            session.persistence.force_flush()?;
        }
        let reopened = Session::open(&path)?;
        assert_eq!(reopened.organizer.trashed_notes().count(), 2);
        assert!(reopened.organizer.trashed_tasks().next().is_none());
        assert!(reopened.organizer.task(task_a).is_none());
        assert!(reopened.organizer.task(task_b).is_none());
        Ok(())
    }
}
