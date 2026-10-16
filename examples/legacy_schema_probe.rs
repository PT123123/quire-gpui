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

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let mode = args.get(1).context("write|configure|verify <db>")?;
    let path = Path::new(args.get(2).context("database path")?);
    match mode.as_str() {
        "write" => write(path),
        "configure" => configure(
            path,
            args.get(3).context("port")?,
            args.get(4).context("token")?,
        ),
        "verify" => verify(path),
        _ => bail!("expected write or verify"),
    }
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
    assert!(s.set_database_relation_target(block, relation, Some(db))?);
    assert!(s.set_database_relation_target(block, mirror, Some(db))?);
    assert!(s.set_database_relation_mirror(block, relation, mirror)?);
    assert!(s.add_database_property_option(block, tags, "Alpha")?);
    assert!(s.add_database_property_option(block, tags, "Beta")?);
    let left = s.add_database_record(block).context("left record")?;
    let right = s.add_database_record(block).context("right record")?;
    s.set_database_cell(
        block,
        left,
        s.databases.title_property(db).context("title")?.id,
        LEFT,
    )?;
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
    let relation_config = quire_core::core::database_relation::config_relation(&relation.config);
    let mirror_config = quire_core::core::database_relation::config_relation(&mirror.config);
    assert_eq!(relation_config.target, Some(db.id));
    assert_eq!(relation_config.mirror, Some(mirror.id));
    assert_eq!(mirror_config.target, Some(db.id));
    assert_eq!(mirror_config.mirror, Some(relation.id));
    let rows = s.repo.records_of(db.id)?;
    let left = rows
        .iter()
        .find(|r| s.repo.record_title(r.id).ok().flatten().as_deref() == Some(LEFT))
        .context("left row lost")?;
    let right = rows
        .iter()
        .find(|r| s.repo.record_title(r.id).ok().flatten().as_deref() == Some(RIGHT))
        .context("right row lost")?;
    assert_eq!(
        s.repo.cell(left.id, relation.id)?,
        CellValue::Items(vec![right.id.0.to_string()])
    );
    assert_eq!(
        s.repo.cell(right.id, mirror.id)?,
        CellValue::Items(vec![left.id.0.to_string()])
    );
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
    println!(
        "verified database={} paired relation config/cells, multi-select options/cell values, and old-version MCP note",
        db.id
    );
    Ok(())
}
