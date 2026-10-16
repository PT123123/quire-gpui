use anyhow::{Context, Result, bail};
use quire_core::core::database::{CellValue, PropertyKind};
use quire_core::core::database_property::{OptionId, PropertyOptions};
use quire_gpui::session::Session;
use std::{collections::BTreeSet, env, path::Path};

const DB_NAME: &str = "GPUI 旧版互通 schema";
const LEFT: &str = "源记录";
const RIGHT: &str = "目标记录";
const RELATION: &str = "相关目标";
const MIRROR: &str = "关联来源";
const TAGS: &str = "多选标签";
const SCORE: &str = "评分";
const FORMULA: &str = "计算分";
const ROLLUP: &str = "关联总分";
const TEXT: &str = "说明";
const DATE: &str = "日期";
const CHECKBOX: &str = "已确认";
const URL: &str = "网址";
const EMAIL: &str = "邮箱";
const PHONE: &str = "电话";
const SELECT: &str = "单选";
const STATUS: &str = "状态";
const FILES: &str = "文件";
const ATTACHMENT_BYTES: &[u8] = b"legacy schema attachment bytes\0\xff";
const SECOND_ATTACHMENT_BYTES: &[u8] = b"second legacy attachment bytes\x00\x01";
const SYNC_SCORE: &str = "同步分数";
const SYNC_FORMULA: &str = "同步公式";
const SYNC_SELECT: &str = "同步选项";
const SYNC_SELECT_VALUE: &str = "跨版本可选";

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let mode = args
        .get(1)
        .context("write|configure|verify|sync-seed-export|sync-verify <db> [path]")?;
    let path = Path::new(args.get(2).context("database path")?);
    match mode.as_str() {
        "write" => write(path),
        "configure" => configure(
            path,
            args.get(3).context("port")?,
            args.get(4).context("token")?,
        ),
        "verify" => verify(path),
        "sync-seed-export" => sync_seed_export(
            path,
            Path::new(args.get(3).context("snapshot output path")?),
        ),
        "sync-verify" => sync_verify(
            path,
            Path::new(args.get(3).context("snapshot response path")?),
        ),
        _ => bail!("expected write, configure, verify, sync-seed-export or sync-verify"),
    }
}

fn sync_seed_export(path: &Path, snapshot_path: &Path) -> Result<()> {
    let mut session = Session::open(path)?;
    session.set_page_title("GPUI 跨版本同步验收".into());
    let block = session.blocks()[0].id;
    session.replace(block, "GPUI → Slint → GPUI 页面数据".into(), true);
    session.create_note_with_content(
        "GPUI 同步笔记".into(),
        "旧版同步服务应完整接收这条笔记".into(),
        vec!["同步验收".into()],
    );
    let task = session.create_task("GPUI 同步任务".into());
    assert!(session.update_task_editor_tags(
        task,
        "GPUI 同步任务".into(),
        "旧版同步服务应完整接收任务备注".into(),
        None,
        vec!["同步验收".into()]
    ));
    session.create_page(None);
    session.set_page_title("GPUI 同步数据库页".into());
    let database_block = session.blocks()[0].id;
    let database = session
        .make_database(database_block, "GPUI 同步数据库".into())
        .context("create sync database")?;
    let score = session
        .add_database_property(database_block, SYNC_SCORE.into(), PropertyKind::Number)
        .context("create sync number property")?;
    let formula = session
        .add_database_property(database_block, SYNC_FORMULA.into(), PropertyKind::Formula)
        .context("create sync formula property")?;
    let select = session
        .add_database_property(database_block, SYNC_SELECT.into(), PropertyKind::Select)
        .context("create sync select property")?;
    assert!(session.set_database_formula(database_block, formula, "1 + [同步分数]")?);
    assert!(session.add_database_property_option(database_block, select, SYNC_SELECT_VALUE)?);
    let record = session
        .add_database_record(database_block)
        .context("create sync database record")?;
    let title_property = session
        .databases
        .title_property(database)
        .context("find sync database title property")?
        .id;
    assert!(session.set_database_cell(
        database_block,
        record,
        title_property,
        "Slint 跨版本数据库行"
    )?);
    assert!(session.set_database_cell(database_block, record, score, "42")?);
    assert!(session.set_database_cell(database_block, record, select, SYNC_SELECT_VALUE)?);
    let device = session.sync_self_info();
    session.persistence.force_flush()?;
    let snapshot = session.sync_export()?;
    std::fs::write(snapshot_path, snapshot.to_json())?;
    println!("SYNC_SOURCE_DEVICE={}", device.id);
    println!("SYNC_SOURCE_ROWS={}", snapshot.row_count());
    Ok(())
}

fn sync_verify(path: &Path, snapshot_path: &Path) -> Result<()> {
    use quire_core::services::sync::{
        SYNC_PORT,
        engine::PeerRecord,
        model::{SNAPSHOT_VERSION, SyncSnapshot},
    };

    let text = std::fs::read_to_string(snapshot_path)?;
    let remote = SyncSnapshot::from_json(&text).map_err(anyhow::Error::msg)?;
    assert_eq!(remote.version, SNAPSHOT_VERSION);
    assert!(
        !remote.device_id.is_empty(),
        "response lost legacy device id"
    );
    assert!(
        remote
            .pages
            .iter()
            .any(|page| page.title == "GPUI 跨版本同步验收")
    );
    assert!(
        remote
            .blocks
            .iter()
            .any(|block| block.text == "GPUI → Slint → GPUI 页面数据")
    );
    assert!(remote.notes.iter().any(|note| {
        note.title == "GPUI 同步笔记"
            && note.body == "旧版同步服务应完整接收这条笔记"
            && note.tags == ["同步验收"]
    }));
    assert!(remote.tasks.iter().any(|task| {
        task.title == "GPUI 同步任务"
            && task.notes == "旧版同步服务应完整接收任务备注"
            && task.tags == ["同步验收"]
    }));
    let remote_database = remote
        .databases
        .iter()
        .find(|database| database.name == "GPUI 同步数据库")
        .context("legacy response omitted the GPUI database")?;
    assert_sync_database(remote_database)?;

    let mut session = Session::open(path)?;
    let peer = PeerRecord {
        id: remote.device_id.clone(),
        name: remote.device.clone(),
        kind: "pc".into(),
        ip: "127.0.0.1".into(),
        port: SYNC_PORT,
        paired: true,
        last_seen: 0,
        last_sync: String::new(),
    };
    let merged = session.sync_apply_remote(&remote, &[], &peer)?;
    assert!(
        session
            .pages
            .iter()
            .any(|page| page.title == "GPUI 跨版本同步验收")
    );
    assert!(
        session
            .blocks()
            .iter()
            .any(|block| block.text == "GPUI → Slint → GPUI 页面数据")
    );
    assert!(
        session
            .organizer
            .notes
            .iter()
            .any(|note| note.title == "GPUI 同步笔记")
    );
    assert!(
        session
            .organizer
            .tasks
            .iter()
            .any(|task| task.title == "GPUI 同步任务")
    );
    let applied = session.sync_export()?;
    assert_sync_database(
        applied
            .databases
            .iter()
            .find(|database| database.name == "GPUI 同步数据库")
            .context("applied snapshot omitted sync database")?,
    )?;
    session.persistence.force_flush()?;
    drop(session);

    let mut reopened = Session::open(path)?;
    assert!(
        reopened
            .organizer
            .notes
            .iter()
            .any(|note| note.title == "GPUI 同步笔记")
    );
    assert!(
        reopened
            .organizer
            .tasks
            .iter()
            .any(|task| task.title == "GPUI 同步任务")
    );
    let reopened_snapshot = reopened.sync_export()?;
    assert_sync_database(
        reopened_snapshot
            .databases
            .iter()
            .find(|database| database.name == "GPUI 同步数据库")
            .context("reopened snapshot omitted sync database")?,
    )?;
    println!(
        "SYNC_LEGACY_RESPONSE=PASS version={} rows={}",
        merged.version,
        merged.row_count()
    );
    Ok(())
}

fn assert_sync_database(database: &quire_core::services::sync::model::SDatabase) -> Result<()> {
    let title = database
        .properties
        .iter()
        .find(|property| property.kind == "title")
        .context("sync database lost its title property")?;
    let score = database
        .properties
        .iter()
        .find(|property| property.name == SYNC_SCORE)
        .context("legacy sync response omitted number property")?;
    let formula = database
        .properties
        .iter()
        .find(|property| property.name == SYNC_FORMULA)
        .context("legacy sync response omitted formula property")?;
    let select = database
        .properties
        .iter()
        .find(|property| property.name == SYNC_SELECT)
        .context("legacy sync response omitted select property")?;
    assert_eq!(score.kind, "number");
    assert_eq!(formula.kind, "formula");
    assert_eq!(
        quire_core::core::database_formula::config_formula(&formula.config).as_deref(),
        Some("1 + [同步分数]")
    );
    assert_eq!(select.kind, "select");
    let options = PropertyOptions::from_config(&select.config);
    let option = options
        .iter()
        .find(|option| option.name == SYNC_SELECT_VALUE)
        .context("legacy sync response omitted select option")?;
    let record = database
        .records
        .iter()
        .find(|record| {
            database.values.iter().any(|value| {
                value.record == record.id
                    && value.property == title.id
                    && value.text.as_deref() == Some("Slint 跨版本数据库行")
            })
        })
        .context("legacy sync response omitted the database row")?;
    assert!(database.values.iter().any(|value| {
        value.record == record.id && value.property == score.id && value.num == Some(42.0)
    }));
    assert!(database.values.iter().any(|value| {
        value.record == record.id
            && value.property == select.id
            && value.text.as_deref() == Some(&option.id.0.to_string())
    }));
    Ok(())
}

fn configure(path: &Path, port: &str, token: &str) -> Result<()> {
    let mut s = Session::open(path)?;
    s.set_setting("mcp.port", port);
    s.set_setting("mcp.token", token);
    s.set_setting("mcp.enable", "true");
    s.persistence.force_flush()?;
    println!("configured isolated legacy MCP endpoint on port {port}");
    Ok(())
}

fn write(path: &Path) -> Result<()> {
    let attachment_source = path.with_file_name("compat-attachment.bin");
    let second_attachment_source = path.with_file_name("compat-attachment-2.bin");
    std::fs::write(&attachment_source, ATTACHMENT_BYTES)?;
    std::fs::write(&second_attachment_source, SECOND_ATTACHMENT_BYTES)?;
    let mut s = Session::open(path)?;
    let block = s.blocks()[0].id;
    let db = s
        .make_database(block, DB_NAME.into())
        .context("create database")?;
    let relation = s
        .add_database_property(block, RELATION.into(), PropertyKind::Relation)
        .context("relation")?;
    let mirror = s
        .add_database_property(block, MIRROR.into(), PropertyKind::Relation)
        .context("mirror")?;
    let tags = s
        .add_database_property(block, TAGS.into(), PropertyKind::MultiSelect)
        .context("tags")?;
    let score = s
        .add_database_property(block, SCORE.into(), PropertyKind::Number)
        .context("score")?;
    let formula = s
        .add_database_property(block, FORMULA.into(), PropertyKind::Formula)
        .context("formula")?;
    let rollup = s
        .add_database_property(block, ROLLUP.into(), PropertyKind::Rollup)
        .context("rollup")?;
    let text = s
        .add_database_property(block, TEXT.into(), PropertyKind::Text)
        .context("text")?;
    let date = s
        .add_database_property(block, DATE.into(), PropertyKind::Date)
        .context("date")?;
    let checkbox = s
        .add_database_property(block, CHECKBOX.into(), PropertyKind::Checkbox)
        .context("checkbox")?;
    let url = s
        .add_database_property(block, URL.into(), PropertyKind::Url)
        .context("url")?;
    let email = s
        .add_database_property(block, EMAIL.into(), PropertyKind::Email)
        .context("email")?;
    let phone = s
        .add_database_property(block, PHONE.into(), PropertyKind::Phone)
        .context("phone")?;
    let select = s
        .add_database_property(block, SELECT.into(), PropertyKind::Select)
        .context("select")?;
    let status = s
        .add_database_property(block, STATUS.into(), PropertyKind::Status)
        .context("status")?;
    let files = s
        .add_database_property(block, FILES.into(), PropertyKind::Files)
        .context("files")?;
    assert!(s.set_database_relation_target(block, relation, Some(db))?);
    assert!(s.set_database_relation_target(block, mirror, Some(db))?);
    assert!(s.set_database_relation_mirror(block, relation, mirror)?);
    assert!(s.set_database_formula(block, formula, "1 + [评分]")?);
    assert!(s.set_database_rollup(
        block,
        rollup,
        Some(relation),
        Some(score),
        quire_core::core::database_rollup::Aggregate::Sum,
    )?);
    assert!(s.add_database_property_option(block, tags, "Alpha")?);
    assert!(s.add_database_property_option(block, tags, "Beta")?);
    assert!(s.add_database_property_option(block, select, "重点")?);
    assert!(s.add_database_property_option(block, status, "进行中")?);
    let left = s.add_database_record(block).context("left record")?;
    let right = s.add_database_record(block).context("right record")?;
    s.set_database_cell(
        block,
        left,
        s.databases.title_property(db).context("title")?.id,
        LEFT,
    )?;
    s.set_database_cell(block, left, score, "12")?;
    s.set_database_cell(block, left, text, "保留的普通文本")?;
    s.set_database_cell(block, left, date, "2026-10-16")?;
    s.set_database_cell(block, left, checkbox, "true")?;
    s.set_database_cell(block, left, url, "https://example.com/quire")?;
    s.set_database_cell(block, left, email, "quire@example.com")?;
    s.set_database_cell(block, left, phone, "+1 555 0100")?;
    s.set_database_cell(block, left, select, "重点")?;
    s.set_database_cell(block, left, status, "进行中")?;
    assert!(s.import_database_files(
        block,
        left,
        files,
        &[attachment_source, second_attachment_source]
    )?);
    s.set_database_cell(block, right, score, "7")?;
    s.set_database_cell(
        block,
        right,
        s.databases.title_property(db).context("title")?.id,
        RIGHT,
    )?;
    assert!(s.set_database_relation_cell(block, left, relation, &[right])?);
    assert!(s.set_database_cell_many(block, left, tags, &["Alpha".into(), "Beta".into()])?);
    println!(
        "written database={db} relation={relation} mirror={mirror} tags={tags} left={left} right={right}"
    );
    Ok(())
}

fn verify(path: &Path) -> Result<()> {
    let s = Session::open(path)?;
    let db = s
        .databases
        .databases
        .iter()
        .find(|d| d.name == DB_NAME)
        .context("database lost")?;
    let find = |name: &str| {
        s.databases
            .properties
            .iter()
            .find(|p| p.db == db.id && p.name == name)
            .with_context(|| format!("property lost: {name}"))
    };
    let relation = find(RELATION)?;
    let mirror = find(MIRROR)?;
    let tags = find(TAGS)?;
    let formula = find(FORMULA)?;
    let score = find(SCORE)?;
    let rollup = find(ROLLUP)?;
    let text = find(TEXT)?;
    let date = find(DATE)?;
    let checkbox = find(CHECKBOX)?;
    let url = find(URL)?;
    let email = find(EMAIL)?;
    let phone = find(PHONE)?;
    let select = find(SELECT)?;
    let status = find(STATUS)?;
    let files = find(FILES)?;
    let relation_config = quire_core::core::database_relation::config_relation(&relation.config);
    let mirror_config = quire_core::core::database_relation::config_relation(&mirror.config);
    assert_eq!(relation_config.target, Some(db.id));
    assert_eq!(relation_config.mirror, Some(mirror.id));
    assert_eq!(mirror_config.target, Some(db.id));
    assert_eq!(mirror_config.mirror, Some(relation.id));
    assert_eq!(
        quire_core::core::database_formula::config_formula(&formula.config).as_deref(),
        Some("1 + [评分]")
    );
    let rollup_config = quire_core::core::database_rollup::config_rollup(&rollup.config);
    assert_eq!(rollup_config.relation, Some(relation.id));
    assert_eq!(rollup_config.column, Some(score.id));
    assert_eq!(
        rollup_config.aggregate,
        quire_core::core::database_rollup::Aggregate::Sum
    );
    let rows = s.repo.records_of(db.id)?;
    let left = rows
        .iter()
        .find(|r| s.repo.record_title(r.id).ok().flatten().as_deref() == Some(LEFT))
        .context("left row lost")?;
    let right = rows
        .iter()
        .find(|r| s.repo.record_title(r.id).ok().flatten().as_deref() == Some(RIGHT))
        .context("right row lost")?;
    assert!(
        matches!(s.repo.cell(left.id, score.id)?, CellValue::Number(value) if value == 12.0),
        "left numeric source value changed"
    );
    assert!(
        matches!(s.repo.cell(right.id, score.id)?, CellValue::Number(value) if value == 7.0),
        "right numeric source value changed"
    );
    assert_eq!(
        s.repo.cell(left.id, relation.id)?,
        CellValue::Items(vec![right.id.0.to_string()])
    );
    assert_eq!(
        s.repo.cell(right.id, mirror.id)?,
        CellValue::Items(vec![left.id.0.to_string()])
    );
    for (property, expected) in [
        (text.id, CellValue::Text("保留的普通文本".into())),
        (date.id, CellValue::Text("2026-10-16".into())),
        (checkbox.id, CellValue::Flag(true)),
        (url.id, CellValue::Text("https://example.com/quire".into())),
        (email.id, CellValue::Text("quire@example.com".into())),
        (phone.id, CellValue::Text("+1 555 0100".into())),
    ] {
        assert_eq!(
            s.repo.cell(left.id, property)?,
            expected,
            "property {property}"
        );
    }
    for (property, expected_name) in [(select, "重点"), (status, "进行中")] {
        let options = PropertyOptions::from_config(&property.config);
        let value = s.repo.cell(left.id, property.id)?;
        let CellValue::Text(option_id) = value else {
            panic!("option value was not stored as text: {value:?}");
        };
        let option_id = option_id.parse::<u64>().context("invalid option id")?;
        assert_eq!(
            options
                .get(OptionId(option_id))
                .map(|option| option.name.as_str()),
            Some(expected_name),
            "option value changed for {}",
            property.name
        );
    }
    let file_ids = match s.repo.cell(left.id, files.id)? {
        CellValue::Items(ids) if ids.len() == 2 => ids,
        value => bail!("unexpected file cell value: {value:?}"),
    };
    for (id, expected_bytes) in file_ids
        .iter()
        .zip([ATTACHMENT_BYTES, SECOND_ATTACHMENT_BYTES])
    {
        let attachment_id = id.parse::<u64>().context("invalid attachment id")?;
        let stored_path = s
            .attachment_path(quire_core::core::types::AttachmentId(attachment_id))
            .context("attachment path missing")?;
        assert_eq!(std::fs::read(stored_path)?, expected_bytes);
    }
    let options = PropertyOptions::from_config(&tags.config);
    let expected_tags = BTreeSet::from(["Alpha".to_owned(), "Beta".to_owned()]);
    assert_eq!(
        options
            .iter()
            .map(|option| option.name.clone())
            .collect::<BTreeSet<_>>(),
        expected_tags,
        "multi-select options changed"
    );
    match s.repo.cell(left.id, tags.id)? {
        CellValue::Items(items) => {
            let names = items
                .iter()
                .map(|id| {
                    let id = id.parse::<u64>().context("invalid option id")?;
                    options
                        .get(OptionId(id))
                        .map(|option| option.name.clone())
                        .context("multi-select option reference was lost")
                })
                .collect::<Result<BTreeSet<_>>>()?;
            assert_eq!(names, expected_tags, "multi-select cell values changed");
        }
        value => panic!("unexpected multi-select value: {value:?}"),
    }
    assert!(
        s.organizer
            .notes
            .iter()
            .any(|note| note.title == "旧版 MCP 写入验收"
                && note.body == "Slint → GPUI 隔离库往返通过"
                && note.tags == ["互通验收"]),
        "legacy MCP note did not survive GPUI reopen"
    );
    assert!(
        s.organizer.tasks.iter().any(|task| {
            task.title == "旧版 MCP 任务恢复验收"
                && task.notes == "Slint → GPUI 隔离任务回收站往返通过"
                && task.tags == ["互通验收"]
                && !task.done
                && task.deleted_at.is_none()
        }),
        "legacy MCP task did not survive GPUI reopen and restore"
    );
    println!(
        "verified database={} paired relation, formula and rollup config, multi-select and common typed cells, ordered attachment set bytes, and old-version MCP note/task restore",
        db.id
    );
    Ok(())
}
