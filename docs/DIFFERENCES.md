# 与 Slint 版的行为差异及修正

只记录有明确用户可见差异、并有意修复或暂时保留的项目；普通组件实现差别不在此重复。

## 速记窗口尺寸、置顶与候选列表布局（2026-10-15）

- 旧版：`ui/components/QuickNoteWindow.slint` 以 560×340 打开、最小 360×220、无边框且始终置顶；标签候选列表最多 148px，文本区保持至少 56px。
- 修正前 GPUI：以 440×270 打开，Windows `WindowKind::PopUp` 仅创建 tool window，并不设置 topmost；编辑区最小 120px，候选展开时会挤压整张卡片。
- 新行为：默认尺寸调整为 560×340；Windows 创建后调用 `SetWindowPos(HWND_TOPMOST, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)`，macOS 继续使用 GPUI PopUp 窗口层级；输入区最小高度降为 56px，在候选最多 148px 时为标题、提示与按钮保留布局空间。补齐可拖动标题区、关闭按钮、快捷键提示和草稿字数。
- 理由：速记是 Alt+N 从其他应用唤起的短暂输入面板，必须保持可见，且候选弹出时不能遮掉提交操作。
- 验收：Windows dev `cargo check -j 8` 通过；Windows release `cargo build --release -j 8` 通过；release 程序在全新临时库运行 `--smoke-test`，窗口成功启动并保存后退出。topmost 与候选展开布局尚待真人桌面验收。

## 速记入口的 GPUI 实体重入修复（2026-10-15）

- 旧版：速记入口在编辑器/托盘/全局快捷键触发后打开独立窗口，候选由当前整理区标签提供。
- GPUI 缺陷：打开速记时，GPUI 正在更新 Workspace；新窗口的同步构造闭包又读取 Workspace 生成标签候选，违反 GPUI 实体不可重入约束，导致速记窗口触发 panic。
- 新行为：在调用 `open_window` 前，从当前 Session 读取并生成候选列表，再将值传给新窗口；新窗口构造不再重入读取 Workspace。编辑仍订阅速记输入事件，提交回到唯一 Workspace Session。
- 理由：恢复旧版已有的速记入口行为，并避免托盘/快捷键触发时崩溃。
- 验收：release 隔离库 smoke 真正创建速记窗口、经 TextInput 编辑事件写入正文、走窗口提交路径、验证标签和重开读回；`cargo test` 与 Release 构建通过。真人托盘点击/Alt+N 仍待桌面交互验收。
