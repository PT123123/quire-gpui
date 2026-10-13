# 数据兼容与 Windows 集成

## core 与存储

旧版依赖 quire-core Git rev `db0e95f`，复刻初始保持相同。Document/命令/Changes、SQLite migrations、FTS、附件、persistence、备份/恢复、organizer 和 sync 业务从 core 复用。新 UI 不自建第二套 schema 或搜索实现。

旧版 README 与阶段记录中的防抖间隔存在历史差异，实施时读当前 controller/state 调用点确定实际值。输入 burst 分组、存储 debounce、组织器草稿 debounce 是三件事，不混成一个全局 timer。

保存失败保留 dirty queue 和用户输入，呈现错误；不能以成功 toast 掩盖失败。Ctrl+S、切库、退出按现有服务语义刷新。退出失败允许用户保留会话或重试，不能静默丢数据。

未知 schema/kind、新版协议拒绝、安全恢复路径按 core 行为。加载未知数据不能默认清库重新种 demo。新程序首次启动只在明确的测试/新库中生成种子。

## 开发与切换

开发应用使用独立目录与显式 `--db` 测试库；建议 App ID 和 binary 使用 Quire GPUI/quire-gpui，最终品牌切换另定。不要启动时自动找到旧库并写入。备份 SQLite 时使用一致性备份服务，不能在旧版打开 WAL 库时仅复制主 .db 文件。

同库不允许两版并发写入。跨版验证用关闭后的完整库副本或 SQLite backup API，包含附件树。记录迁移前 schema、记录数、稳定 ID、关键字段和附件哈希；测试旧→新→旧往返。不能声称“schema 一样就兼容”，还要测设置、页面 metadata、organizer、数据库属性和附件引用。

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
