use crate::mcp::ToolDef;
use quire_core::core::{
    organizer::{ListId, NoteId, Priority, TaskId},
    types::PageId,
};
use quire_gpui::session::Session;
use serde_json::{Value, json};

fn tags(value: Option<&Value>) -> Option<Vec<String>> {
    value.and_then(Value::as_str).map(|value| {
        let mut out = Vec::new();
        for tag in value
            .split([',', '，'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !out.iter().any(|existing| existing == tag) {
                out.push(tag.to_owned());
            }
        }
        out
    })
}

fn integer(args: &Value, key: &str) -> Result<u64, String> {
    args.get(key)
        .and_then(Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or_else(|| format!("参数 {key} 必须是正整数"))
}

fn required_string<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("缺少字符串参数 {key}"))
}

fn split_tags(value: &str) -> Vec<String> {
    tags(Some(&Value::String(value.to_owned()))).unwrap_or_default()
}

/// Execute MCP organizer tools against the application's one live session.
/// The caller runs this only on GPUI's UI thread, then flushes persistence.
pub fn execute(session: &mut Session, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "list_pages" => {
            let pages = session
                .pages
                .iter()
                .filter(|page| !page.template)
                .map(|page| {
                    json!({
                        "page_id": page.id.0,
                        "title": page.title,
                        "parent_id": page.parent.map(|parent| parent.0),
                        "locked": page.locked,
                        "favorite": page.favorite
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({"pages": pages}))
        }
        "read_page" => {
            let id = PageId(integer(args, "page_id")?);
            let page = session
                .pages
                .iter()
                .find(|page| page.id == id && !page.template)
                .ok_or_else(|| format!("找不到页面 {}", id.0))?;
            let blocks = session
                .document
                .page_blocks(id)
                .iter()
                .map(|block| {
                    json!({
                        "block_id": block.id.0,
                        "parent_id": block.parent.map(|parent| parent.0),
                        "kind": block.kind.as_str(),
                        "text": block.text,
                        "checked": block.checked,
                        "page_ref": block.page_ref.map(|page| page.0),
                        "folded": block.folded
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "page_id": page.id.0,
                "title": page.title,
                "parent_id": page.parent.map(|parent| parent.0),
                "locked": page.locked,
                "blocks": blocks
            }))
        }
        "create_page" => {
            let title = required_string(args, "title")?.trim();
            if title.is_empty() {
                return Err("页面标题不能为空".into());
            }
            let body = args.get("body").and_then(Value::as_str).unwrap_or("");
            let current = session.page.id;
            let id = session.import_markdown(title.to_owned(), body);
            let _ = session.select_page(current);
            Ok(json!({"ok": true, "page_id": id.0}))
        }
        "update_page" => {
            let id = PageId(integer(args, "page_id")?);
            let title = required_string(args, "title")?.trim();
            if title.is_empty() {
                return Err("页面标题不能为空".into());
            }
            let page = session
                .pages
                .iter()
                .find(|page| page.id == id && !page.template)
                .ok_or_else(|| format!("找不到页面 {}", id.0))?;
            if page.locked {
                return Err(format!("页面 {} 已锁定", id.0));
            }
            let current = session.page.id;
            if !session.select_page(id) {
                return Err(format!("无法打开页面 {}", id.0));
            }
            session.set_page_title(title.to_owned());
            let _ = session.select_page(current);
            Ok(json!({"ok": true, "page_id": id.0, "title": title}))
        }
        "list_notes" => {
            let include_trashed = args
                .get("include_trashed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let rows = session.organizer.notes.iter().filter(|n| include_trashed || !n.is_trashed()).map(|n| json!({
                "note_id": n.id.0, "title": n.title, "body": n.body, "tags": n.tags,
                "pinned": n.pinned, "trashed": n.is_trashed(), "created": n.created, "edited": n.edited
            })).collect::<Vec<_>>();
            Ok(json!({"notes": rows}))
        }
        "create_note" => {
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let body = args
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let id = session.create_note();
            session.update_note_editor(
                id,
                title,
                body,
                split_tags(args.get("tags").and_then(Value::as_str).unwrap_or("")),
            );
            Ok(json!({"ok": true, "note_id": id.0}))
        }
        "update_note" => {
            let id = NoteId(integer(args, "note_id")?);
            let old = session
                .organizer
                .note(id)
                .cloned()
                .ok_or_else(|| format!("找不到笔记 {}", id.0))?;
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(&old.title)
                .to_owned();
            let body = args
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or(&old.body)
                .to_owned();
            let new_tags = tags(args.get("tags")).unwrap_or(old.tags);
            session.update_note_editor(id, title, body, new_tags);
            Ok(json!({"ok": true, "note_id": id.0}))
        }
        "trash_note" | "restore_note" => {
            let id = NoteId(integer(args, "note_id")?);
            if session.organizer.note(id).is_none() {
                return Err(format!("找不到笔记 {}", id.0));
            }
            session.set_note_trashed(id, name == "trash_note");
            Ok(json!({"ok": true, "note_id": id.0, "trashed": name == "trash_note"}))
        }
        "list_tasks" => {
            let include_trashed = args
                .get("include_trashed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let rows = session
                .organizer
                .tasks
                .iter()
                .filter(|t| include_trashed || !t.is_trashed())
                .map(|t| {
                    json!({
                        "task_id": t.id.0, "list_id": t.list.0, "title": t.title, "notes": t.notes,
                        "due": t.due, "priority": t.priority.as_str(), "done": t.done,
                        "tags": t.tags, "trashed": t.is_trashed()
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({"tasks": rows}))
        }
        "create_task" => {
            let title = required_string(args, "title")?.to_owned();
            let priority = args
                .get("priority")
                .and_then(Value::as_str)
                .map(|value| {
                    Priority::try_from_str(value)
                        .ok_or_else(|| "priority 必须是 none/low/medium/high".to_owned())
                })
                .transpose()?;
            let list_id = args
                .get("list_id")
                .and_then(Value::as_u64)
                .map(ListId)
                .unwrap_or(ListId::INBOX);
            if list_id != ListId::INBOX && session.organizer.list(list_id).is_none() {
                return Err(format!("找不到清單 {}", list_id.0));
            }
            let notes = args
                .get("notes")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let due = args
                .get("due")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .map(str::to_owned);
            if due
                .as_deref()
                .is_some_and(|date| !quire_core::core::date::is_iso_date(date))
            {
                return Err("任务日期无效（应为 YYYY-MM-DD）".into());
            }
            let task_tags = split_tags(args.get("tags").and_then(Value::as_str).unwrap_or(""));
            let id = session.create_task_in(list_id, title);
            let old = session
                .organizer
                .task(id)
                .cloned()
                .ok_or_else(|| "建立任务失败".to_owned())?;
            session.update_task_editor_tags(id, old.title, notes, due, task_tags);
            if let Some(priority) = priority {
                session.set_task_priority(id, priority);
            }
            Ok(json!({"ok": true, "task_id": id.0}))
        }
        "update_task" => {
            let id = TaskId(integer(args, "task_id")?);
            let old = session
                .organizer
                .task(id)
                .cloned()
                .ok_or_else(|| format!("找不到任务 {}", id.0))?;
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(&old.title)
                .to_owned();
            let notes = args
                .get("notes")
                .and_then(Value::as_str)
                .unwrap_or(&old.notes)
                .to_owned();
            let due = args
                .get("due")
                .and_then(Value::as_str)
                .map(|v| (!v.is_empty()).then(|| v.to_owned()))
                .unwrap_or(old.due.clone());
            let task_tags = tags(args.get("tags")).unwrap_or(old.tags.clone());
            let priority = args
                .get("priority")
                .and_then(Value::as_str)
                .map(|value| {
                    Priority::try_from_str(value)
                        .ok_or_else(|| "priority 必须是 none/low/medium/high".to_owned())
                })
                .transpose()?;
            if due
                .as_deref()
                .is_some_and(|date| !quire_core::core::date::is_iso_date(date))
            {
                return Err("任务更新失败或日期无效（应为 YYYY-MM-DD）".into());
            }
            session.update_task_editor_tags(id, title, notes, due, task_tags);
            if let Some(priority) = priority {
                session.set_task_priority(id, priority);
            }
            if let Some(done) = args.get("done").and_then(Value::as_bool) {
                session.set_task_done(id, done);
            }
            Ok(json!({"ok": true, "task_id": id.0}))
        }
        "trash_task" | "restore_task" => {
            let id = TaskId(integer(args, "task_id")?);
            if session.organizer.task(id).is_none() {
                return Err(format!("找不到任务 {}", id.0));
            }
            session.set_task_trashed(id, name == "trash_task");
            Ok(json!({"ok": true, "task_id": id.0, "trashed": name == "trash_task"}))
        }
        _ => Err(format!("未知工具：{name}")),
    }
}

#[derive(Debug)]
pub enum McpJob {
    Call {
        name: String,
        args: Value,
        reply: std::sync::mpsc::Sender<Result<Value, String>>,
    },
}

pub fn call_sender(tx: std::sync::mpsc::Sender<crate::platform::ShellEvent>) -> crate::mcp::CallFn {
    std::sync::Arc::new(move |name, args| {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        tx.send(crate::platform::ShellEvent::McpCall(McpJob::Call {
            name: name.to_owned(),
            args,
            reply: reply_tx,
        }))
        .map_err(|_| "MCP 请求队列已关闭（应用正在退出）".to_owned())?;
        reply_rx
            .recv_timeout(std::time::Duration::from_secs(120))
            .map_err(|_| "MCP 工具调用超时".to_owned())?
    })
}

fn schema(props: Value, required: &[&str]) -> Value {
    let mut v = json!({ "type": "object", "properties": props });
    if !required.is_empty() {
        v["required"] = json!(required);
    }
    v
}

/// 全部工具。名字用英文（Agent 的调用面），描述里写清中文语境。
pub fn tool_defs() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "list_pages",
            description: "列出工作区中的普通页面及标题、父页面、锁定和收藏状态；模板不包含在内。",
            input_schema: schema(json!({}), &[]),
        },
        ToolDef {
            name: "read_page",
            description: "读取页面标题和有序内容块。页面/块引用以 id 返回，页面模板不会列出。",
            input_schema: schema(json!({ "page_id": { "type": "integer" } }), &["page_id"]),
        },
        ToolDef {
            name: "create_page",
            description: "新建顶层页面，可用 Markdown 正文初始化内容。",
            input_schema: schema(
                json!({ "title": { "type": "string" }, "body": { "type": "string" } }),
                &["title"],
            ),
        },
        ToolDef {
            name: "update_page",
            description: "修改普通页面标题；锁定页面拒绝编辑。",
            input_schema: schema(
                json!({ "page_id": { "type": "integer" }, "title": { "type": "string" } }),
                &["page_id", "title"],
            ),
        },
        ToolDef {
            name: "list_notes",
            description: "列出整理器的笔记（含在回收站里的，用 include_trashed 控制）。",
            input_schema: schema(
                json!({ "include_trashed": { "type": "boolean", "description": "默认 false，只列未删除的" } }),
                &[],
            ),
        },
        ToolDef {
            name: "create_note",
            description: "新建一条笔记。tags 用逗号分隔。",
            input_schema: schema(
                json!({
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "tags": { "type": "string", "description": "逗号分隔，如 \"工作,项目\"" }
                }),
                &["title"],
            ),
        },
        ToolDef {
            name: "update_note",
            description: "修改一条笔记的标题/正文/标签；只改给出的字段。",
            input_schema: schema(
                json!({
                    "note_id": { "type": "integer" },
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "tags": { "type": "string" }
                }),
                &["note_id"],
            ),
        },
        ToolDef {
            name: "trash_note",
            description: "把一条笔记移到回收站（可恢复；不彻底删除）。",
            input_schema: schema(json!({ "note_id": { "type": "integer" } }), &["note_id"]),
        },
        ToolDef {
            name: "restore_note",
            description: "把一条笔记从回收站恢复。",
            input_schema: schema(json!({ "note_id": { "type": "integer" } }), &["note_id"]),
        },
        ToolDef {
            name: "list_tasks",
            description: "列出整理器的任务（含回收站的，用 include_trashed 控制）。返回清单 id、标题、备注、截止日、优先级、完成状态。",
            input_schema: schema(json!({ "include_trashed": { "type": "boolean" } }), &[]),
        },
        ToolDef {
            name: "create_task",
            description: "新建一个任务（默认进收集箱）。priority ∈ none/low/medium/high，due 是 YYYY-MM-DD，tags 逗号分隔。",
            input_schema: schema(
                json!({
                    "title": { "type": "string" },
                    "notes": { "type": "string" },
                    "due": { "type": "string" },
                    "priority": { "type": "string", "enum": ["none", "low", "medium", "high"] },
                    "tags": { "type": "string" },
                    "list_id": { "type": "integer", "description": "清单 id；缺省为收集箱" }
                }),
                &["title"],
            ),
        },
        ToolDef {
            name: "update_task",
            description: "修改一个任务的标题/备注/截止日/优先级/完成状态/标签；只改给出的字段。",
            input_schema: schema(
                json!({
                    "task_id": { "type": "integer" },
                    "title": { "type": "string" },
                    "notes": { "type": "string" },
                    "due": { "type": "string" },
                    "priority": { "type": "string", "enum": ["none", "low", "medium", "high"] },
                    "done": { "type": "boolean" },
                    "tags": { "type": "string" }
                }),
                &["task_id"],
            ),
        },
        ToolDef {
            name: "trash_task",
            description: "把一个任务移到回收站（可恢复；不彻底删除）。",
            input_schema: schema(json!({ "task_id": { "type": "integer" } }), &["task_id"]),
        },
        ToolDef {
            name: "restore_task",
            description: "把一个任务从回收站恢复。",
            input_schema: schema(json!({ "task_id": { "type": "integer" } }), &["task_id"]),
        },
    ]
}

/// 「给 Agent 的提示词」：用户贴进 AI 对话的一段话，说明 Quire 的接口能做什么、
/// 有哪些工具、参数怎么填、守什么约定。工具清单和参数都从 tool_defs() 生成 —
/// 接口加了新工具或改了参数，这段话不会和实际能力脱节。刻意不含令牌：这段文字
/// 会被贴到远端助手里，连接配置（含令牌）留在页面上另一处复制。
pub fn agent_prompt(port: u16) -> String {
    let mut s = String::from(
        "Quire 是运行在这台电脑上的笔记与任务应用，通过 MCP 接口开放下面这些工具。\n\
         需要读取或修改页面、记录笔记或整理任务时，请直接调用这些工具，不要改动 Quire 的数据文件。\n\n\
         可用工具：\n",
    );
    for t in tool_defs() {
        s.push_str(&format!("- {}：{}\n", t.name, t.description));
        // 参数行从工具自己的 input_schema 拆：必填在前，可选项带类型和枚举值。
        let empty = serde_json::Value::Null;
        let props = t.input_schema.get("properties").unwrap_or(&empty);
        let required: Vec<&str> = t
            .input_schema
            .get("required")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let mut parts: Vec<String> = Vec::new();
        for key in &required {
            if let Some(p) = props.get(key) {
                let ty = p
                    .get("enum")
                    .map(|_| "枚举")
                    .unwrap_or_else(|| p.get("type").and_then(Value::as_str).unwrap_or("值"));
                parts.push(format!("{key}({ty})"));
            }
        }
        if let Some(obj) = props.as_object() {
            let optional: Vec<String> = obj
                .iter()
                .filter(|(k, _)| !required.contains(&k.as_str()))
                .map(|(k, p)| {
                    let mut d = format!(
                        "{k}({})",
                        p.get("enum").map(|_| "枚举").unwrap_or_else(|| {
                            p.get("type").and_then(Value::as_str).unwrap_or("值")
                        })
                    );
                    if let Some(ev) = p.get("enum").and_then(Value::as_array) {
                        let vals: Vec<&str> = ev.iter().filter_map(Value::as_str).collect();
                        d.push_str(&format!("：{}", vals.join("|")));
                    }
                    d
                })
                .collect();
            if !optional.is_empty() {
                parts.push(format!("可选 {}", optional.join("、")));
            }
        }
        if !parts.is_empty() {
            s.push_str(&format!("  参数：{}\n", parts.join("；")));
        }
    }
    s.push_str(&format!(
        "\n使用约定：\n\
         - 修改或删除前，先用 list_notes / list_tasks 查到对应的 note_id / task_id。\n\
         - trash_* 只把条目移入回收站，可用 restore_* 恢复；接口不提供彻底删除。\n\
         - tags 用逗号分隔多个标签，支持「项目/子标签」这样的层级写法。\n\
         - update_* 只修改给出的字段，没给出的字段保持原样。\n\
         - HTTP 服务在 http://127.0.0.1:{port}；stdio 与 HTTP 的连接配置（含令牌）在 Quire 设置 → MCP 页复制。\n\
         - 客户端不走 MCP 配置时，也可以按 JSON-RPC 2.0 直接调：每次 POST 一条消息，\n\
         \x20 请求形如 {{\"method\":\"tools/call\",\"params\":{{\"name\":\"create_note\",\"arguments\":{{\"title\":\"示例\"}}}}}}。\n"
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> (quire_core::testing::ScratchDir, Session) {
        let scratch = quire_core::testing::ScratchDir::new("gpui-mcp-tools");
        let session = Session::open(&scratch.path().join("test.db")).expect("session opens");
        (scratch, session)
    }

    #[test]
    fn note_tools_create_update_trash_restore_and_list() {
        let (_scratch, mut session) = session();
        let created = execute(
            &mut session,
            "create_note",
            &json!({
                "title": "MCP note", "body": "hello", "tags": "工作，项目,工作"
            }),
        )
        .unwrap();
        let id = created["note_id"].as_u64().unwrap();
        let note = session.organizer.note(NoteId(id)).unwrap();
        assert_eq!(note.tags, ["工作", "项目"]);
        execute(
            &mut session,
            "update_note",
            &json!({"note_id": id, "body": "updated"}),
        )
        .unwrap();
        assert_eq!(
            session.organizer.note(NoteId(id)).unwrap().title,
            "MCP note"
        );
        execute(&mut session, "trash_note", &json!({"note_id": id})).unwrap();
        assert_eq!(
            execute(&mut session, "list_notes", &json!({})).unwrap()["notes"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        execute(&mut session, "restore_note", &json!({"note_id": id})).unwrap();
        assert!(!session.organizer.note(NoteId(id)).unwrap().is_trashed());
    }

    #[test]
    fn page_tools_list_read_create_rename_and_reopen_without_changing_selection() {
        let (scratch, mut session) = session();
        let current = session.page.id;
        let created = execute(
            &mut session,
            "create_page",
            &json!({"title":"MCP page", "body":"# Heading\n\nPage body"}),
        )
        .unwrap();
        let id = created["page_id"].as_u64().unwrap();
        assert_eq!(session.page.id, current);
        let listed = execute(&mut session, "list_pages", &json!({})).unwrap();
        assert!(
            listed["pages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|page| { page["page_id"] == id && page["title"] == "MCP page" })
        );
        let read = execute(&mut session, "read_page", &json!({"page_id": id})).unwrap();
        assert_eq!(read["blocks"][0]["text"], "Heading");
        assert!(
            read["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|block| { block["text"] == "Page body" })
        );
        execute(
            &mut session,
            "update_page",
            &json!({"page_id": id, "title":"Renamed page"}),
        )
        .unwrap();
        assert_eq!(session.page.id, current);
        assert_eq!(
            session
                .pages
                .iter()
                .find(|page| page.id.0 == id)
                .unwrap()
                .title,
            "Renamed page"
        );
        session.persistence.force_flush().unwrap();
        drop(session);
        let reopened = Session::open(&scratch.path().join("test.db")).unwrap();
        assert_eq!(
            reopened
                .pages
                .iter()
                .find(|page| page.id.0 == id)
                .unwrap()
                .title,
            "Renamed page"
        );
        assert!(
            reopened
                .document
                .page_blocks(PageId(id))
                .iter()
                .any(|block| block.text == "Page body")
        );
    }

    #[test]
    fn page_update_rejects_locked_pages_without_partial_changes() {
        let (_scratch, mut session) = session();
        let page = session.page.id;
        let original_title = session.page.title.clone();
        session.set_page_locked(true);
        let result = execute(
            &mut session,
            "update_page",
            &json!({"page_id": page.0, "title":"Should not apply"}),
        );
        assert!(result.unwrap_err().contains("已锁定"));
        assert_eq!(session.page.title, original_title);
    }

    #[test]
    fn task_tools_validate_before_mutating_and_preserve_unspecified_fields() {
        let (_scratch, mut session) = session();
        let initial_task_count = session.organizer.tasks.len();
        let invalid = execute(
            &mut session,
            "create_task",
            &json!({"title":"bad", "due":"tomorrow"}),
        );
        assert!(invalid.is_err());
        assert_eq!(session.organizer.tasks.len(), initial_task_count);
        let created = execute(&mut session, "create_task", &json!({
            "title":"Ship", "notes":"keep this", "due":"2026-10-20", "priority":"high", "tags":"a,b"
        })).unwrap();
        let id = created["task_id"].as_u64().unwrap();
        execute(
            &mut session,
            "update_task",
            &json!({"task_id": id, "done": true}),
        )
        .unwrap();
        let task = session.organizer.task(TaskId(id)).unwrap();
        assert_eq!(task.notes, "keep this");
        assert_eq!(task.due.as_deref(), Some("2026-10-20"));
        assert!(task.done);
        assert_eq!(task.priority, Priority::High);
        let rejected = execute(
            &mut session,
            "update_task",
            &json!({
                "task_id": id, "title": "must not partially apply", "priority": "urgent"
            }),
        );
        assert!(rejected.is_err());
        assert_eq!(session.organizer.task(TaskId(id)).unwrap().title, "Ship");
    }
}
