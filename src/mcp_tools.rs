use crate::mcp::ToolDef;
use quire_core::core::{
    organizer::{ListId, NoteId, Priority, TaskId},
    types::{BlockId, PageId},
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
        "search_pages" => {
            let query = required_string(args, "query")?.trim();
            if query.is_empty() {
                return Err("搜索词不能为空".into());
            }
            let needle = query.to_lowercase();
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(20)
                .clamp(1, 100) as usize;
            let mut matches = Vec::new();
            for page in session.pages.iter().filter(|page| !page.template) {
                let title_match = page.title.to_lowercase().contains(&needle);
                let blocks = session
                    .document
                    .page_blocks(page.id)
                    .iter()
                    .filter(|block| block.text.to_lowercase().contains(&needle))
                    .map(|block| {
                        json!({
                            "block_id": block.id.0,
                            "kind": block.kind.as_str(),
                            "text": block.text.chars().take(240).collect::<String>()
                        })
                    })
                    .collect::<Vec<_>>();
                if title_match || !blocks.is_empty() {
                    matches.push(json!({
                        "page_id": page.id.0,
                        "title": page.title,
                        "title_match": title_match,
                        "blocks": blocks
                    }));
                    if matches.len() >= limit {
                        break;
                    }
                }
            }
            Ok(json!({"query": query, "pages": matches}))
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
            let parent = if args.get("parent_id").is_some() {
                Some(PageId(integer(args, "parent_id")?))
            } else {
                None
            };
            if let Some(parent_id) = parent
                && !session
                    .pages
                    .iter()
                    .any(|page| page.id == parent_id && !page.template)
            {
                return Err(format!("找不到父页面 {}", parent_id.0));
            }
            let current = session.page.id;
            let id = session.import_markdown_under(title.to_owned(), body, parent);
            let _ = session.select_page(current);
            Ok(json!({"ok": true, "page_id": id.0, "parent_id": parent.map(|id| id.0)}))
        }
        "update_page" => {
            let id = PageId(integer(args, "page_id")?);
            let title = args.get("title").and_then(Value::as_str);
            let body = args.get("body").and_then(Value::as_str);
            if title.is_none() && body.is_none() {
                return Err("至少提供 title 或 body".into());
            }
            if title.is_some_and(|title| title.trim().is_empty()) {
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
            if let Some(body) = body {
                session
                    .replace_page_markdown(id, body)
                    .map_err(|error| error.to_string())?;
            }
            if let Some(title) = title {
                if !session.select_page(id) {
                    return Err(format!("无法打开页面 {}", id.0));
                }
                session.set_page_title(title.trim().to_owned());
                let _ = session.select_page(current);
            }
            let new_title = session
                .pages
                .iter()
                .find(|page| page.id == id)
                .map(|page| page.title.as_str())
                .unwrap_or_default();
            Ok(
                json!({"ok": true, "page_id": id.0, "title": new_title, "body_updated": body.is_some()}),
            )
        }
        "update_block_text" => {
            let page_id = PageId(integer(args, "page_id")?);
            let block_id = BlockId(integer(args, "block_id")?);
            let text = required_string(args, "text")?.to_owned();
            session
                .replace_plain_page_block_text(page_id, block_id, text.clone())
                .map_err(|error| error.to_string())?;
            Ok(json!({"ok": true, "page_id": page_id.0, "block_id": block_id.0, "text": text}))
        }
        "list_notes" => {
            let include_trashed = args
                .get("include_trashed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let rows = session.organizer.notes.iter().filter(|n| {
                (include_trashed || !n.is_trashed()) && !session.note_delete_is_pending(n.id)
            }).map(|n| json!({
                "id": n.id.0, "note_id": n.id.0, "title": n.title, "body": n.body, "tags": n.tags,
                "pinned": n.pinned, "trashed": n.is_trashed(), "created": n.created, "edited": n.edited,
                "ref_note": n.ref_note.map(|parent| parent.0)
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
            Ok(json!({"ok": true, "id": id.0, "note_id": id.0}))
        }
        "update_note" => {
            let id = NoteId(integer(args, "note_id")?);
            if session.note_delete_is_pending(id) {
                return Err(format!("笔记 {} 正在等待撤销删除", id.0));
            }
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
            if session.note_delete_is_pending(id) {
                return Err(format!("笔记 {} 正在等待撤销删除", id.0));
            }
            let note = session
                .organizer
                .note(id)
                .ok_or_else(|| format!("找不到笔记 {}", id.0))?;
            let trashed = name == "trash_note";
            if note.is_trashed() == trashed {
                return Err(if trashed {
                    format!("笔记 {} 不存在（或已在回收站）", id.0)
                } else {
                    format!("笔记 {} 不在回收站", id.0)
                });
            }
            if !session.set_note_trashed(id, trashed) {
                return Err(format!("更新笔记 {} 的回收站状态失败", id.0));
            }
            Ok(json!({"ok": true, "note_id": id.0, "trashed": trashed}))
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
                .filter(|t| {
                    (include_trashed || !t.is_trashed()) && !session.task_delete_is_pending(t.id)
                })
                .map(|t| {
                    json!({
                        "id": t.id.0, "task_id": t.id.0, "list": t.list.0, "list_id": t.list.0, "title": t.title, "notes": t.notes,
                        "due": t.due, "priority": t.priority.as_str(), "done": t.done,
                        "tags": t.tags, "trashed": t.is_trashed(),
                        "subtasks": t.subtasks.iter().map(|item| json!({
                            "id": item.id, "title": item.title, "done": item.done
                        })).collect::<Vec<_>>()
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
            let list_id = match args.get("list_id") {
                None => ListId::INBOX,
                Some(value) => {
                    let raw = value
                        .as_i64()
                        .ok_or_else(|| "list_id 必须是非负整数".to_owned())?;
                    let raw =
                        u64::try_from(raw).map_err(|_| "list_id 必须是非负整数".to_owned())?;
                    ListId(raw)
                }
            };
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
            Ok(json!({"ok": true, "id": id.0, "task_id": id.0}))
        }
        "update_task" => {
            let id = TaskId(integer(args, "task_id")?);
            if session.task_delete_is_pending(id) {
                return Err(format!("任务 {} 正在等待撤销删除", id.0));
            }
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
            if session.task_delete_is_pending(id) {
                return Err(format!("任务 {} 正在等待撤销删除", id.0));
            }
            let task = session
                .organizer
                .task(id)
                .ok_or_else(|| format!("找不到任务 {}", id.0))?;
            let trashed = name == "trash_task";
            if task.is_trashed() == trashed {
                return Err(if trashed {
                    format!("任务 {} 不存在（或已在回收站）", id.0)
                } else {
                    format!("任务 {} 不在回收站", id.0)
                });
            }
            if !session.set_task_trashed(id, trashed) {
                return Err(format!("更新任务 {} 的回收站状态失败", id.0));
            }
            Ok(json!({"ok": true, "task_id": id.0, "trashed": trashed}))
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
            name: "search_pages",
            description: "按页面标题或块正文不区分大小写搜索普通页面，返回命中块与截断文本；limit 默认 20，最多 100 页。",
            input_schema: schema(
                json!({ "query": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": 100 } }),
                &["query"],
            ),
        },
        ToolDef {
            name: "create_page",
            description: "新建页面，可用 Markdown 正文初始化内容；可选 parent_id 在普通页面下创建子页面。",
            input_schema: schema(
                json!({ "title": { "type": "string" }, "body": { "type": "string" }, "parent_id": { "type": "integer", "minimum": 1 } }),
                &["title"],
            ),
        },
        ToolDef {
            name: "update_page",
            description: "修改普通页面标题和/或以 Markdown 全量替换正文；锁定页面或含无法无损往返的复杂块/样式时拒绝正文替换。",
            input_schema: schema(
                json!({ "page_id": { "type": "integer" }, "title": { "type": "string" }, "body": { "type": "string", "description": "Markdown 正文，会全量替换该页内容；仅支持可无损往返的简单块页面" } }),
                &["page_id"],
            ),
        },
        ToolDef {
            name: "update_block_text",
            description: "替换页面中的单个纯文本块，保留其块类型和位置；有行内格式、子块、引用、附件或特殊块类型时拒绝修改。",
            input_schema: schema(
                json!({ "page_id": { "type": "integer" }, "block_id": { "type": "integer" }, "text": { "type": "string" } }),
                &["page_id", "block_id", "text"],
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
            description: "列出整理器的任务（含回收站的，用 include_trashed 控制）。返回清单 id、标题、备注、截止日、优先级、完成状态及子任务 id/标题/完成状态。",
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
                    "list_id": { "type": "integer", "minimum": 0, "description": "清单 id；缺省为收集箱" }
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
        assert_eq!(created["id"], id);
        let note = session.organizer.note(NoteId(id)).unwrap();
        assert_eq!(note.tags, ["工作", "项目"]);
        let reply = session
            .create_note_reply(NoteId(id), "MCP 可见评论".into())
            .unwrap();
        let listed = execute(&mut session, "list_notes", &json!({})).unwrap();
        assert_eq!(listed["notes"][0]["id"], id);
        assert_eq!(listed["notes"][0]["note_id"], id);
        let listed_reply = listed["notes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|note| note["id"] == reply.0)
            .unwrap();
        assert_eq!(listed_reply["ref_note"], id);
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
        let remaining = execute(&mut session, "list_notes", &json!({})).unwrap();
        assert_eq!(remaining["notes"].as_array().unwrap().len(), 1);
        assert_eq!(remaining["notes"][0]["id"], reply.0);
        assert_eq!(remaining["notes"][0]["ref_note"], id);
        execute(&mut session, "restore_note", &json!({"note_id": id})).unwrap();
        assert!(!session.organizer.note(NoteId(id)).unwrap().is_trashed());
    }

    #[test]
    fn trash_restore_tools_reject_noop_state_transitions() {
        let (_scratch, mut session) = session();
        let note_id = session.create_note();
        assert!(execute(&mut session, "restore_note", &json!({"note_id": note_id.0})).is_err());
        execute(&mut session, "trash_note", &json!({"note_id": note_id.0})).unwrap();
        assert!(execute(&mut session, "trash_note", &json!({"note_id": note_id.0})).is_err());
        assert!(session.organizer.note(note_id).unwrap().is_trashed());
        execute(&mut session, "restore_note", &json!({"note_id": note_id.0})).unwrap();
        assert!(execute(&mut session, "restore_note", &json!({"note_id": note_id.0})).is_err());
        assert!(!session.organizer.note(note_id).unwrap().is_trashed());

        let task_id = session.create_task("状态迁移".into());
        assert!(execute(&mut session, "restore_task", &json!({"task_id": task_id.0})).is_err());
        execute(&mut session, "trash_task", &json!({"task_id": task_id.0})).unwrap();
        assert!(execute(&mut session, "trash_task", &json!({"task_id": task_id.0})).is_err());
        assert!(session.organizer.task(task_id).unwrap().is_trashed());
        execute(&mut session, "restore_task", &json!({"task_id": task_id.0})).unwrap();
        assert!(execute(&mut session, "restore_task", &json!({"task_id": task_id.0})).is_err());
        assert!(!session.organizer.task(task_id).unwrap().is_trashed());
    }

    #[test]
    fn organizer_pending_deletes_are_hidden_and_cannot_be_mutated() {
        let (_scratch, mut session) = session();
        let note_id = session.create_note();
        session.defer_trash_notes(&[note_id], "pending".into());
        assert_eq!(
            execute(&mut session, "list_notes", &json!({})).unwrap()["notes"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert!(
            execute(
                &mut session,
                "update_note",
                &json!({"note_id": note_id.0, "title":"race"})
            )
            .unwrap_err()
            .contains("等待撤销删除")
        );

        let task_id = session.create_task("pending task".into());
        session.defer_trash_tasks(&[task_id], "pending".into());
        assert_eq!(
            execute(&mut session, "list_tasks", &json!({})).unwrap()["tasks"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert!(
            execute(
                &mut session,
                "update_task",
                &json!({"task_id": task_id.0, "title":"race"})
            )
            .unwrap_err()
            .contains("等待撤销删除")
        );
    }

    #[test]
    fn list_tasks_preserves_subtask_details_for_mcp_clients() {
        let (_scratch, mut session) = session();
        let created = execute(
            &mut session,
            "create_task",
            &json!({"title": "Release", "notes": "ship", "priority": "high"}),
        )
        .unwrap();
        assert_eq!(created["id"], created["task_id"]);
        let task_id = TaskId(created["task_id"].as_u64().unwrap());
        assert!(session.add_subtask(task_id, "Build installer".into()));
        let subtask_id = session.organizer.task(task_id).unwrap().subtasks[0].id;
        assert!(session.set_subtask_done(task_id, subtask_id, true));

        let listed = execute(&mut session, "list_tasks", &json!({})).unwrap();
        let task = listed["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|task| task["task_id"] == task_id.0)
            .unwrap();
        assert_eq!(task["id"], task_id.0);
        assert_eq!(task["list"], task["list_id"]);
        assert_eq!(task["subtasks"][0]["id"], subtask_id);
        assert_eq!(task["subtasks"][0]["title"], "Build installer");
        assert_eq!(task["subtasks"][0]["done"], true);
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
            &json!({"page_id": id, "title":"Renamed page", "body":"# Revised\n\nUpdated body"}),
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
        assert!(session.select_page(PageId(id)));
        session.undo();
        assert!(
            session
                .document
                .page_blocks(PageId(id))
                .iter()
                .any(|block| block.text == "Page body")
        );
        session.redo();
        assert!(
            session
                .document
                .page_blocks(PageId(id))
                .iter()
                .any(|block| block.text == "Updated body")
        );
        assert!(session.select_page(current));
        let search = execute(
            &mut session,
            "search_pages",
            &json!({"query":"Updated body"}),
        )
        .unwrap();
        assert_eq!(search["pages"][0]["page_id"], id);
        assert_eq!(search["pages"][0]["blocks"][0]["text"], "Updated body");
        let body_block = session
            .document
            .page_blocks(PageId(id))
            .iter()
            .find(|block| block.text == "Updated body")
            .unwrap()
            .id
            .0;
        execute(
            &mut session,
            "update_block_text",
            &json!({"page_id": id, "block_id": body_block, "text": "Edited one block"}),
        )
        .unwrap();
        assert_eq!(session.page.id, current);
        assert!(
            session
                .document
                .page_blocks(PageId(id))
                .iter()
                .any(|block| block.id == BlockId(body_block) && block.text == "Edited one block")
        );
        assert!(session.select_page(PageId(id)));
        session.undo();
        assert!(
            session
                .document
                .page_blocks(PageId(id))
                .iter()
                .any(|block| block.id == BlockId(body_block) && block.text == "Updated body")
        );
        session.redo();
        assert!(
            session
                .document
                .page_blocks(PageId(id))
                .iter()
                .any(|block| block.id == BlockId(body_block) && block.text == "Edited one block")
        );
        assert!(session.select_page(current));
        let complex = session.import_markdown("Complex page".into(), "Safe paragraph");
        assert!(session.select_page(complex));
        session.execute(quire_core::core::command::Command::AppendBlock {
            kind: quire_core::core::types::BlockKind::Callout,
            text: "Important callout".into(),
        });
        let complex_before = session.document.page_blocks(complex).to_vec();
        let callout_id = complex_before
            .iter()
            .find(|block| block.kind == quire_core::core::types::BlockKind::Callout)
            .unwrap()
            .id
            .0;
        assert!(
            execute(
                &mut session,
                "update_block_text",
                &json!({"page_id":complex.0, "block_id":callout_id, "text":"changed"})
            )
            .unwrap_err()
            .contains("拒绝直接替换")
        );
        assert!(session.select_page(current));
        let rejected = execute(
            &mut session,
            "update_page",
            &json!({"page_id": complex.0, "body":"replacement"}),
        );
        assert!(rejected.unwrap_err().contains("拒绝整页替换"));
        assert_eq!(session.document.page_blocks(complex), complex_before);
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
                .any(|block| block.text == "Edited one block")
        );
    }

    #[test]
    fn create_page_can_add_a_child_without_changing_selection_and_rejects_invalid_parent() {
        let (_scratch, mut session) = session();
        let current = session.page.id;
        let parent = execute(
            &mut session,
            "create_page",
            &json!({"title":"Parent", "body":"parent body"}),
        )
        .unwrap()["page_id"]
            .as_u64()
            .unwrap();
        let child = execute(
            &mut session,
            "create_page",
            &json!({"title":"Child", "body":"child body", "parent_id":parent}),
        )
        .unwrap();
        let child_id = child["page_id"].as_u64().unwrap();
        assert_eq!(child["parent_id"], parent);
        assert_eq!(session.page.id, current);
        assert_eq!(
            session
                .pages
                .iter()
                .find(|page| page.id.0 == child_id)
                .and_then(|page| page.parent)
                .map(|id| id.0),
            Some(parent)
        );
        assert_eq!(
            session.document.page_blocks(PageId(child_id))[0].text,
            "child body"
        );
        assert!(
            execute(
                &mut session,
                "create_page",
                &json!({"title":"Orphan", "parent_id":u64::MAX}),
            )
            .unwrap_err()
            .contains("找不到父页面")
        );
        assert_eq!(session.page.id, current);
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

    #[test]
    fn create_task_rejects_invalid_explicit_list_id_without_falling_back_to_inbox() {
        let (_scratch, mut session) = session();
        let initial_task_count = session.organizer.tasks.len();

        for list_id in [json!(-1), json!("not-a-list")] {
            let result = execute(
                &mut session,
                "create_task",
                &json!({"title":"must not be created", "list_id":list_id}),
            );
            assert!(result.is_err());
            assert_eq!(session.organizer.tasks.len(), initial_task_count);
        }

        let created = execute(
            &mut session,
            "create_task",
            &json!({"title":"inbox default"}),
        )
        .unwrap();
        let task = session
            .organizer
            .task(TaskId(created["task_id"].as_u64().unwrap()))
            .unwrap();
        assert_eq!(task.list, ListId::INBOX);
    }
}
