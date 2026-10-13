# quire-core API 面（GPUI 壳要调用的全部东西）

来源：本地 `../quire-core`，HEAD = `db0e95fc7710f4fd6010617cadc2492325e79d75`（与旧版 `Cargo.toml:51` 的 pin 一致，工作树干净）。行号指 `quire-core/src`。本文中标记「已复核」的条目由本次交接逐条开文件确认过原文，其余为按同一批源码整理，实现前仍应按行号回读。

规模：`src` 共 37437 行、四个顶层模块（`lib.rs:4-16`）。`Cargo.toml` **没有任何 `[features]`**，依赖只有 `image 0.25.10`（default-features=false，png/jpeg/bmp/gif）、`rusqlite 0.40.2`（bundled）、`serde 1`、`serde_json 1`。头文件规则：core 内不得出现 Slint、文件对话框、剪贴板或任何平台 API。

## 1. 模块地图

- `core/`：21 个 pub 模块（`core/mod.rs:4-37`），re-export 见 `core/mod.rs:39-48`。命令、文档、历史、类型、数据库域、organizer、persistence、数学、高亮、embed、reference、icon、template、diff、date。
- `storage/`：11 个模块，re-export `OpenReport, Database, SqliteRepository`（`storage/mod.rs:31-33`）。红线（`storage/mod.rs:9-14`）：只有 `database_store` 执行 SQL，`database_query` 只拼查询文本与绑定参数。
- `services/`：11 个模块（`services/mod.rs:3-13`）：attachment_store、export/import/find/search、persistence、settings_store、lan_server、lan_client、logging、sync。
- `testing.rs`：scratch 目录守卫，不是 `#[cfg(test)]`，因为无头截图工具要用。

最大文件：`command.rs` 4855、`database_store.rs` 4655、`database_view.rs` 2234、`database_property.rs` 1788、`merge.rs` 1652、`database.rs` 1602、`migrations.rs` 1340。

## 2. 标识与排序（已复核）

**所有 ID 都是整型 newtype，不是字符串**：`pub struct X(pub u64)`（`core/types.rs:15-40` 宏 + `core/database.rs:253-262`、`core/organizer.rs:63-68`）。`PageId/BlockId/AttachmentId/DatabaseId/PropertyId/RecordId/ViewId/NoteId/TaskId/ListId/OptionId`，各有 `as_u64()`。

- `PageId(u64::MAX)` = `ORGANIZER_STACK`（`types.rs:57`，理由 42-56）：整理区复用同一套撤销栈。
- `ListId(0)` = 收件箱哨兵，`is_inbox()`（`organizer.rs:81,85`）。
- `OrderKey`（`types.rs:61-88`）：u64 分数索引，`FIRST=1<<32`、`STRIDE=1<<16`、`between()` 取中点，耗尽时整页重排；落库时符号位翻转成 i64（`storage/database.rs:42-48`）。
- **重排是内部行为**：`Document::renumber_page`（`document.rs:103`）不发 Change，理由见 96-98。GPUI 侧不能假设"顺序变化一定带来 Change"。

## 3. 块类型：core 用字符串，桌面用 int（已复核，最容易踩）

- `BlockKind` 有 **25 个变体，core 里没有数字**，落库是短稳定字符串（`types.rs:90` 注释 + `as_str` 223-251）：`paragraph, heading_1, heading_2, heading_3, bullet, numbered, todo, quote, code, divider, callout, page, link`（`LinkToPage` 序列化成 `"link"`）`, toggle, image, file, table, table_cell, columns, column, math, toc, embed, synced, database`。解析入口 `try_from_str`（`types.rs:258`），展示顺序数组 `ALL`（184-210）。
- 0..24 那套数字是**桌面端自造的 Slint 菜单 id**：`quire-desktop/src/app/state.rs:5974` 的 `kind_from_int`/`kind_to_int`（1=Heading1…6=Todo，`state.rs:14497` `pub const BLOCK_TODO: i32 = 6`），slash 菜单数组由它驱动（`state.rs:5854-5893`）。core 从不见到这些数字。
- 结论：GPUI 的新编辑器可以自行决定内部编号，**但**必须保证任何持久化路径不写这套 int；同时 `ColorKind::slot`、`PageFont::slot`、`PropertyKind::index`、`ViewLayout::index`、`FilterOp::index`、`Priority/Repeat::slot`、`highlight::Kind::from_int` 是**跨端承重的数字**（core 或已存数据里有），不许重排。

`Block` 字段（`types.rs:513-580`）：`id, page, parent, order, kind, text, checked, marks, color, background, page_ref, folded, attachment, img_percent, columns, lang, db_ref, sync_ref`。
`Mark`（459-508）：`start, end, kind, url, date`；`MarkKind` 8 个：Bold/Italic/Strike/Code/Link/Math/Mention/Date。
`Page`（656-695）：`id, title, parent, order, favorite, expanded, font, full_width, small_text, icon(emoji), cover, locked, template`；`full_width=1 / small_text=2` 位掩码落库（`storage/repository.rs:46-47`）。
`Attachment`（587-600）：`file`/`thumb` 存的是附件目录里的文件名，不是 blob。

## 4. 文本单位：UTF-8 字节偏移（已复核，IME 适配层的契约）

四处独立证据一致：`types.rs:407-408`（Mark 文档："byte offsets into the block's UTF-8 text (char boundaries — the caret model guarantees it)"）、`services/find_service.rs:18-25`、`storage/migrations.rs:139`、`services/import_service.rs:42`；`core/command.rs:66-67` 声明 `ToggleMark` 的 start/end 是字节偏移，光标吸附到字符边界的辅助在 `command.rs:745`。

含义：GPUI 内部若用 UTF-16 或 grapheme 索引，**必须在一个适配层里集中转换**，`ReplaceText`、`SplitBlock.caret`、`ToggleMark`、`InsertReference.at`、`reference::trigger_at` 全部只认 UTF-8 字节。这正是 docs/05 要求的转换层，现在有了确定的目标单位。

## 5. 变更路径与撤销（已复核）

单一 mutation 路径（core ADR-0012）：`Command → plan() → Entry{apply, revert} → Document::apply + History::push + Repository::apply`。

- `Command` 约 50 个变体（`core/command.rs:21-463`）。关键约束（`command.rs:140-149`）：**plan 是 Document 的纯函数，新 ID 与旧值由调用方先分配/捕获后装进命令里**——这是 sync 与批量操作可重放的前提。
- 入口：`plan`（921）、`exec`（2501）、`exec_all`（2513）。`exec_all` 把一批命令压成**一条**历史条目，且都按 pre-state 规划。批量撤销一步靠它。
- `Change` 约 70 个变体（`core/persistence.rs:24-277`），**只按名字追加、没有序号编码**（文档 88-95、204-206）：新壳匹配变体必须按名，不许按索引。
- 撤销归 core：`core/history.rs`，`CAP = 100` 每页栈（已复核 `history.rs:10`），`undo/redo` 返回 `Vec<Change>` 由壳重放；`referenced_attachments`（56-68）供附件回收。整理区用同一栈，键为 `ORGANIZER_STACK`。壳只负责按键与喂回。

服务层（不走 Command 的）：附件 `AttachmentStore::{for_db, import_file, import_bytes, import_any_file, display_path, stored_path, remove, save_name, export_to}`（`services/attachment_store.rs`，`MAX_EDGE=1280`，按内容嗅探格式，落盘 `<db父目录>/attachments/<id>.<ext>` + `<id>.cache.png`）；导出 `export_page_full`（`export_service.rs:109`）；导入 `parse_markdown` / `import_markdown(src, page, alloc)`（`import_service.rs:51,130`，同样由调用方分配 ID）；设置 `SettingsStore::settings_changes`（`settings_store.rs:137`，diff→Change）。模板 `core/template.rs:33` 内置 5 个。

## 6. 存储与生命周期

- `Repository` trait（`core/persistence.rs:324-334`）：`load` / `apply(&[Change])`（唯一写路径，一个事务）/ `replace_all`。契约 322-323：实现必须可从任意线程使用（防抖 flush 可能离开 UI 线程）。`StorageError::{Open, Sql, Corrupt}`（301-308）。
- `SqliteRepository::{open, open_with_report, open_at, in_memory, snapshot, search, references, reference_count, load_attachments}`（`storage/repository.rs:67-175`）。**`search` 故意不在 trait 上**（core ADR-0014）。
- 连接：单个 `Mutex<Connection>`（`storage/database.rs:17`），WAL + `synchronous=FULL` + `foreign_keys=ON` + busy_timeout 5s（理由 87-107），每次 open 跑 `PRAGMA integrity_check`（116）。
- 迁移：`CURRENT_VERSION: i32 = 30`（`storage/migrations.rs:53`，已复核），记在 `PRAGMA user_version`；`REPLAYABLE = [(2, ["search_pages","search_blocks"])]`（1125，理由：user_version 不是回执）；`repair_and_verify` 每次 open 都跑（1151）；**比 30 新的文件直接拒绝打开**（`ensure_current` 1180-1187 返回 Corrupt）。`check_schema` 的 18 张表清单在 1214-1237。
- 备份：`KEEP=5`、`MAX_AGE=7d`、`.bak<N>` 轮转、`VACUUM INTO` 快照、`open_with_recovery`（`storage/backup.rs:222`）完整性失败时回滚到最新可读备份。每页版本：`versions/p<page>-<created>.db`，`MAX_PER_PAGE=20`，`save()` 返回 `None` 表示同秒冲突需重试（`storage/versions.rs:151`）。
- 防抖持久化：`services/persistence.rs`，`DEFAULT_DEBOUNCE_MS = 300`（已复核 77）、`DEFAULT_SNAPSHOT_INTERVAL_MS = 600_000`（84）；`record/record_all/has_pending/pending_len/next_deadline_ms/flush_if_due/force_flush/take_last_error`。**`next_deadline_ms` 就是给壳的定时器间隔**，别自己拍脑袋定 tick。
- 数据位置：`storage/data_location.rs`，`APP_DIR_NAME="Quire"`、`DB_NAME="quire.db"`、`LEGACY_DIR="appdata"`；`LaunchOptions::from_args` 解析 `--db`/`--db=`/`--portable`（137-156），`decide`（166）里 **`--db` 压倒一切**；`migrate_legacy`（267）会带走 db + .bak 家族 + sidecar + 日志家族。新壳的独立数据目录要走同一套策略，别自己拼路径。

## 7. 搜索、页内查找、反向链接

- FTS5 两张表 `search_pages`/`search_blocks`，**rowid 就是领域 ID**；中文按**逐字符**切分（不是 trigram，core ADR-0014），`is_split_char` 范围在 `storage/search_index.rs:26-36`。查询构造 `build_match`（82-96）：单词 → `bareword*`，多词 → `"a b"*`。索引维护与数据同事务（105-160），`rebuild`（179）是重放式迁移。
- 返回：`SearchRequest{query, page, limit}`（210）、`Match{page, page_title, is_title, block, text, score}`（234），**score 是 bm25，越小越好**；`DEFAULT_LIMIT=20`；查询会排除模板页并先 4 倍过采（250-266）。
- 壳侧必须走 `SearchService::search_async`（`services/search_service.rs:68`，线程名 `quire-search`，返回 `PendingSearch`，`poll`/`wait`），文件头规则：UI 线程永远不等 SQLite。
- 页内 Ctrl+F 是另一套：`FindSession`（`services/find_service.rs:41`），字节寻址、不重叠、ASCII 大小写不敏感、`next` 会回绕、`position` 从 1 开始。
- 反向链接是查询不是存储：`references_of(conn, page, limit)`（`storage/backlinks.rs:52`）。

## 8. 同步与线程模型

- 端口：`SYNC_PORT=5878`、`DISCOVERY_PORT=5879`（`services/sync/mod.rs:36,38`）；局域网只读分享 `5877`（`services/lan_server.rs:20`）；MCP `5880` 在桌面侧。
- 线上格式：`SyncSnapshot{version, device_id, device, pages, blocks, attachments, databases, notes, tasks, lists}`（`sync/model.rs:60-82`），`SNAPSHOT_VERSION = 4`（已复核 57），`to_json/from_json` **要求版本精确相等**，缺字段故意不做 serde default（73-81）以便旧对端大声失败。
- 合并：`merge(local, shadow, remote, peer, &mut MergeCtx) -> MergeOutcome`（`sync/merge.rs:452`）。`MergeCtx` 携带 **10 个分配闭包**（67-92）——ID 分配由壳提供，与本地 Command 同一规则。整理区按 uuid + `rev` 字符串仲裁（34-44），`rev = "{millis:013}-{device}"`（`organizer.rs:106`）。冲突=保留本地并记日志。
- 线程结论：core 自身没有 `Rc`（全库仅测试里出现），`Repository: Send+Sync`，flush 可跨线程。**但** `sync/mod.rs:24-26` 明文规定：任何读写 workspace 的 Job 必须经 channel 回到 UI 线程的定时器上执行，后台线程只搬字节和 JSON。`engine::start(self_info, jobs) -> Sender<Cmd>`（194）会拉起 4 条后台线程（HTTP server、announcer、listener、worker）。设备 ID 由壳持久化在设置行里（`sync/engine.rs:23-24`）。
- 因此 GPUI 壳必须重建：Job 泵（旧版是 250ms UI 定时器）、设备 ID 持久化、MergeCtx 的 10 个闭包。这三件 core 帮不了。

## 9. MCP 在桌面侧，不在 core（已复核：core 全库 grep "mcp" 零命中）

- 服务端 `quire-desktop/src/mcp.rs`：只绑 127.0.0.1（166），令牌存在设置行 `mcp.token`，`DEFAULT_PORT=5880`（36），协议版本协商 `["2024-11-05","2025-03-26","2025-06-18"]`（40），指针文件 `{port,token}`（281/288），开关设置 `mcp.enable` 默认 true。
- stdio 桥 `quire.exe mcp --port N --db path --token t`（`run_stdio_bridge` 320）**代理到运行中的实例而不是直开库**，原因写在文件头第 17 行：ID 水位在应用内存里，第二个进程会撞号。这条约束对任何 GPUI 外部工具同样成立。
- 工具目录 10 个，只有整理区：`list/create/update/trash/restore` × `note/task`（`app/mcp_tools.rs:108`）。**页面完全不在协议里**；删除只做 tombstone；写入走 `AppState::org_* → exec_org → record`，所以 agent 的修改和用户敲键盘进同一条撤销栈。8 个写工具列在 `WRITES`（346）。调用经 `McpJob::Call` + 250ms 泵，超时 120s，日志留在设置行 `mcp.log`（尾部 50 行）。
- 新壳要重写的是 server/桥/工具定义，数据操作全是现成 core API。

## 10. 不许在壳里重实现（core 已有且带隐藏语义）

schema 与迁移、Command→Change→Repository 写路径、撤销栈、FTS 索引、同步协议与 rev 格式、附件目录布局与命名、备份/版本家族、日期格式（只有 `YYYY-MM-DD`，`core/date.rs`）、以及第 3 节列出的承重数字。

## 11. core 已经提供的"与渲染无关"投影（直接复用，别重算）

- 文档：`Document::{page_blocks, all_blocks, index_of}`、`can_move_block_to`（`command.rs:908`）、`math::to_unicode`（`core/math.rs:21`，只在绘制时展开，从不入库）、`highlight::layer(text, lang, frame, advance, kind)`（`core/highlight.rs:97`，语法高亮按颜色叠加一层文本）、`embed::{describe, is_openable, with_scheme, PROVIDERS}`（`core/embed.rs`，无网络）、`reference::{PAGE_SCHEME="quire://page/", page_uri, page_of, trigger_at}`（`core/reference.rs:23-66`，@ 触发点返回**字节偏移**）、`icon::{PICKER[96], PER_ROW=8, initial, slot}`、`diff::compare`（`core/diff.rs:67`，版本对比的行数据）、`template::PRESETS`。
- 数据库：窗口化 `window()` / `RealizedRows::scroll_to(total, geometry, scroll_y, fetch)`（`core/database.rs:123,177`，`DEFAULT_OVERSCAN=8`，测试证明 10000 行只实例化 31 行）、`TableView::{height, row_y}` + `ROW_HEIGHT=32.0`/`HEADER_HEIGHT=64.0`（`core/database_view.rs:462,467`）、`TableRowView/TableCellView`（`editable`/`painted`/`checked` 已算好）、`ViewDefinition` 解析与 pass-through 编辑、列宽千分比（`WIDTH_UNIT=1000`/`MIN=60`/`AUTO=0`）、`group_window`、日历辅助（1763-1972）、图表路径 `chart_line_path`/`chart_pie_paths`（2140/2168，矢量路径数据）、`database_property::{parse_one, parse_many, paint}`（选项色与数字格式）、公式引擎含预算（`FORMULA_MAX_TOKENS=2048`、`MAX_DEPTH=32`、`MAX_STEPS=10000`、`RESULT_MAX=65536`、`ERROR_PAINT="Error"`、`would_cycle`）、relation `check_pair`、rollup `check_config/fold/paint`。**过滤/排序/分组全在 SQL 里执行**（`database_query` 拼、`database_store` 跑），壳侧不许做 UI 端过滤。
- 整理区：`OrganizerCatalog` 全量加载（与数据库的窗口化策略相反，理由 `organizer.rs:444-452`），`note/task/list/tasks_in/live_notes/trashed_notes/live_tasks/trashed_tasks`。

## 12. 桌面侧目前自己扛着、GPUI 壳必须重建的能力（core 缺口）

1. 块类型 int 与 slash 菜单数组（`state.rs:5974`、`5854-5893`）。
2. **中文文案层**：core 的 `label()` 是英文，中文词表在 `state.rs:14992` 附近（注释原文："core 的 label() 是英文，这些才是给用户看的词"），月份中文标签在 `state.rs:15067`。新壳要么搬这套词表，要么把 i18n 提进 core（需 ADR）。
3. 同步/MCP 的会话胶水与 Job 泵（第 8、9 节）。
4. Markdown 快捷输入 → Command 的识别（`controller.rs:1359-1383`、`9214-9216`），core 只收结果命令。
5. 打开页面时的 todo 勾选修补（`controller.rs:3054`，与 `exec_on_open_page` 分开的一条路径；`state.rs:3408-3409` 合成 `ToggleTodoChecked`）。
6. MCP 的页面级工具目前**根本不存在**，这是壳的策略选择而非 core 限制。
