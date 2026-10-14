# 与 Slint 版的行为差异及修正

只记录有明确用户可见差异、并有意修复或暂时保留的项目；普通组件实现差别不在此重复。

## 速记窗口尺寸、置顶与候选列表布局（2026-10-15）

- 旧版：`ui/components/QuickNoteWindow.slint` 以 560×340 打开、最小 360×220、无边框且始终置顶；标签候选列表最多 148px，文本区保持至少 56px。
- 修正前 GPUI：以 440×270 打开，Windows `WindowKind::PopUp` 仅创建 tool window，并不设置 topmost；编辑区最小 120px，候选展开时会挤压整张卡片。
- 新行为：默认尺寸调整为 560×340；Windows 创建后调用 `SetWindowPos(HWND_TOPMOST, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)`，macOS 继续使用 GPUI PopUp 窗口层级；输入区最小高度降为 56px，在候选最多 148px 时为标题、提示与按钮保留布局空间。补齐可拖动标题区、关闭按钮、快捷键提示和草稿字数。
- 理由：速记是 Alt+N 从其他应用唤起的短暂输入面板，必须保持可见，且候选弹出时不能遮掉提交操作。
- 验收：Windows dev `cargo check -j 8` 通过；Windows release `cargo build --release -j 8` 通过；release 程序在全新临时库运行 `--smoke-test`，窗口成功启动并保存后退出。topmost 与候选展开布局尚待真人桌面验收。
