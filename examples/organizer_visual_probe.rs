//! Seed only a new, explicitly named scratch database for organizer UI review.
use anyhow::{Result, bail};
use quire_core::core::organizer::Priority;
use quire_gpui::session::Session;
use std::{net::TcpListener, path::PathBuf};

fn main() -> Result<()> {
    let path = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("provide a NEW scratch database path"),
    );
    if path.exists() {
        bail!("visual probe refuses an existing database");
    }
    let mut session = Session::open(&path)?;
    session.set_setting("gpui.quick-note-on-start", "false");
    session.set_setting("gpui.mcp.enabled", "true");
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    session.set_setting("gpui.mcp.port", listener.local_addr()?.port().to_string());
    drop(listener);
    session.set_setting("gpui.theme", "midnight");
    let pinned = session.create_note_with_content(
        "把想法留在这里".into(),
        "好的记录不一定很长。\n给想法留一点空白，也给后来的自己留一条线索。\n\n今天想把读过的书、临时想到的小事，都记在同一个地方。".into(),
        vec!["生活".into(), "随手记".into()],
    );
    session.set_note_pinned(pinned, true);
    session.create_note_reply(pinned, "下一次打开时，继续补上新的发现。".into());
    session.create_note_with_content(
        "周末，慢一点".into(),
        "去附近的公园走一走，带上相机和一本书。\n路上如果有新的想法，就随手记下来。".into(),
        vec!["周末".into()],
    );
    session.create_note_with_content(
        String::new(),
        "读到一句话：先把事情做简单，再把简单的事情做好。".into(),
        vec!["阅读".into()],
    );
    let list = session.create_task_list("工作安排".into());
    let task = session.create_task_in(list, "整理本周的阅读笔记".into());
    session.update_task_editor_tags(
        task,
        "整理本周的阅读笔记".into(),
        "留下三个最值得记住的观点，并补上自己的想法。".into(),
        Some("2026-10-16".into()),
        vec!["阅读".into()],
    );
    session.set_task_priority(task, Priority::High);
    session.add_subtask(task, "回顾划线与摘录".into());
    session.add_subtask(task, "整理成一页简短总结".into());
    let next = session.create_task_in(list, "准备下周的工作安排".into());
    session.set_task_due(next, Some("2026-10-19".into()));
    session.set_task_priority(next, Priority::Low);
    session.create_task("给书桌留一点空白".into());
    session.persistence.force_flush()?;
    println!(
        "seeded isolated organizer review database: {}",
        path.display()
    );
    Ok(())
}
