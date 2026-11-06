# 数据兼容与 Windows 集成

## core 与存储

旧版依赖 quire-core Git rev `1d6a357`。2026-11-06 起 GPUI 锁同一 rev（此前是 `b73af89`，两者的 `migrations.rs` 相同、`CURRENT_VERSION` 都是 30）。Document/命令/Changes、SQLite migrations、FTS、附件、persistence、备份/恢复、organizer 和 sync 业务从 core 复用。新 UI 不自建第二套 schema 或搜索实现。

旧版 README 与阶段记录中的防抖间隔存在历史差异，实施时读当前 controller/state 调用点确定实际值。输入 burst 分组、存储 debounce、组织器草稿 debounce 是三件事，不混成一个全局 timer。

保存失败保留 dirty queue 和用户输入，呈现错误；不能以成功 toast 掩盖失败。Ctrl+S、切库、退出按现有服务语义刷新。退出失败允许用户保留会话或重试，不能静默丢数据。

未知 schema/kind、新版协议拒绝、安全恢复路径按 core 行为。加载未知数据不能默认清库重新种 demo。新程序首次启动只在明确的测试/新库中生成种子。

## 开发与切换

2026-11-06 的用户决定：**两版共用一套数据后端**。GPUI 的默认库不再是 `%LOCALAPPDATA%\QuireGPUI\`，而是旧版一直在用的 `%APPDATA%\Quire\quire.db`；`--db` 仍然优先于默认值，压测/迁移/破坏性测试照旧用显式 `--db` 指向 `%TEMP%` 副本，不碰真实库。开发构建与安装标识（AppUserModelID、安装目录、exe 名）仍然独立。

同库不允许两版并发写入，这条现在有了机器门：GPUI 打开库时先独占试开 `<db>-shm`（Windows `share_mode(0)`）。SQLite 只在有连接附着时持有该文件，试开失败即说明另一版正在写，GPUI 拒绝启动并说明要先退出另一个；残留的 `-shm`（上一进程没干净退出）能被独占打开，所以这道门不会把库锁死。门只覆盖 GPUI 这一侧——旧版代码未改，**旧版在 GPUI 常驻时被拉起不会被拦住**，对称的那 5 行要加在 quire-desktop，等授权。

`--import-library PATH` 是一次性的并库命令：以真实会话打开源库，`sync_export` 出一份快照，再走局域网同步那套三方合并 `sync_apply_remote`（无 shadow）并进当前库，打印每项前后计数后退出，不开窗、不弹窗。合并只新增：模板页不进快照（`sync_export` 就排除了它），两侧同 id 的行保留本侧，本侧独有的行不会被”对端没有”判成删除。内置模板种子也改成按预设逐个查标题——旧版库自带 5 份同名模板页，GPUI 只补缺的，不再往用户库塞第二套。

备份 SQLite 时使用一致性备份服务，不能在任一版打开 WAL 库时仅复制主 `.db` 文件。跨版验证用关闭后的完整库副本（主文件 + `-wal` + `-shm` + 附件 + 配置），记录迁移前 schema、记录数、稳定 ID、关键字段和附件哈希；测旧→新→旧往返：GPUI 写入后的副本必须由 `quire-slint --db 副本 --dump-state` 读出同样的页数，反向亦然。不能声称”schema 一样就兼容”，还要测设置、页面 metadata、organizer、数据库属性和附件引用。

## 同步与 Android

UI 替换不应改变 snapshot version、设备 ID、wire format 或合并策略。新程序必须复用现有 core sync，使用真实测试设备/进程做 create/update/delete、离线重连和冲突双向验证。不要每次启动重建 device ID。

协议不兼容明确显示原因；不能半理解后写库。不触碰 Android 项目 pin，除非另有明确变更需求。不能把两个 UI 同时打开的截图当成同步测试证据。

## 平台模块

审阅 reference/src/platform：hotkeys、tray、monitors、quit、dib；reference/src/app/tray 与 main 也包含协调逻辑。纯 Windows 调用可按需复用，Slint event loop/window handle 部分改为 GPUI 适配。

热键：Alt+N 默认开启、打开独立速记；Alt+M 默认关闭、显示主窗（以当前设置代码核对）。注册失败显示冲突或实际 OS 错误，配置切换同步注销，退出释放。

托盘：点击/菜单显示主窗、速记及退出等以实际实现为准；窗口关闭与进程退出分开；退出一定刷新。隐藏主窗不应留下持续刷新；速记关闭/隐藏不退出主应用。

多屏：窗口位置夹紧到可用工作区，断开屏幕后仍可找回；速记置顶、无任务栏项、拖动与焦点请求实际测试。不要把 HWND hack 原封不动复制到不同窗口生命周期。

剪贴板：Unicode 文本、富文本和图片 DIB 等格式按旧版支持；处理像素 stride/alpha/色序、尺寸上限，不能假设全部 BGRA 图像紧密连续。打开文件和 URL 走系统安全接口，路径不拼成 shell 命令。

## MCP

reference/src/mcp.rs、app/mcp_tools.rs、McpPage.slint 和 Types 包含当前行为。保留工具业务、权限开关、状态和启停语义；工具修改之后走统一状态刷新路径，不让 UI 显示旧投影。manifest 标记了这些文件的未提交状态，不能仅从旧 commit 重建而丢掉最新改动。
