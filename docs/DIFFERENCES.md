# 与 Slint 版的行为差异及修正

只记录有明确用户可见差异、并有意修复或暂时保留的项目；普通组件实现差别不在此重复。

## 整理区布局与笔记新建入口（2026-10-16）

- 移除笔记平铺/列表切换和工具展开/收起按钮；复制全部与指令入口直接显示。回收站入口统一放在整理区导航栏。
- 右侧整理区取消额外的圆角外框和笔记列表底色，内容直接使用可用窗口区域。笔记列表模式下隐藏“看起来像输入框”的捕获卡片，右下角悬浮“＋”会直接创建并打开可编辑的空白笔记。
- 用户反馈原笔记平铺入口不需要、回收站位置不合适、工具折叠控件多余、笔记新建入口不直观，且右侧外框形成多层套框。
- 验收：未运行构建或桌面交互验证。

## 页面块悬停与拖动预览（2026-10-16）

- 页面块原先悬停时显示插入、拖动、颜色、复制/删除、排序和更多等快捷按钮；现在移除整排按钮，块自身保留轻微 hover 高亮，右键块打开原有上下文菜单。
- 拖动从块内容区域直接开始，拖动预览改为主题适配的半透明卡片并增加柔和阴影，接近 Notion 的淡化拖动态。GPUI 0.2.2 没有元素高斯模糊 API，因此以透明度和阴影表达虚化层次。
- 验收：未运行构建或桌面拖放验证。

## GPUI 任务详情分层（2026-10-16，用户确认方案）

- 原任务详情把优先级、重复、日期输入和所有“移至清单”按钮常驻展开，描述和子任务又各有背景块，任务标题与内容不够突出。
- 按用户确认的方案，顶部为完成勾选框、可编辑标题、菜单和关闭；日期/优先级/重复点击后只展开一个设置区，支持方向键、Enter 和 Escape。描述和子任务改为连续编辑区，当前清单与标签作为次要信息，底部固定保留“复制内容和 ID”和可展开的编号/唯一 ID/创建修改时间。删除继续走原有任务菜单与可撤销回收站路径。
- 所有修改仍通过既有编辑草稿和 core 服务保存；更改属性前提交标题、描述与子任务草稿。修正编辑子任务时同一输入实体同时出现在编辑行和新增行的问题。默认按 midnight 深色主题开发、预览和验收。
- 验收：最终 Release 构建 55.03 秒，EXE 26,343,936 字节；隔离 Release smoke 验证属性选择、草稿保留、日期校验、移动清单、宽/窄详情构建与 Escape 焦点恢复；任务 CRUD、完成、回收站及撤销重做持久化回归通过。日志位于 `.scratch/task-detail-smoke/app.stderr.log`。另用隔离样例库在 1280×800、midnight 实窗点击检查优先级展开、详细信息展开及滚动固定底栏，编号、唯一 ID 和 UTC 时间可读；修正窄任务列表的“回车创建”提示溢入详情区域。真人 IME 和跨 DPI 体验不包含在本次检查内。

## 整理区笔记与任务详情面板（2026-10-16）

- GPUI 笔记详情原先是覆盖在卡片列表上的窄浮窗，未显示笔记标题，操作只有图标且缺少明确层级；任务详情也没有把任务标题作为主要编辑入口显示出来。
- 按用户要求整理为贴齐右边缘的详情侧栏，笔记标题、标签、正文和评论分区更清楚，置顶/转待办/删除操作改为带文字的轻按钮；任务详情增加醒目的标题编辑栏，并与列表区分隔。标题继续通过现有编辑草稿和自动保存路径写回。
- 验收：Release 构建 `cargo build --release --locked --target-dir target-measure` 成功（45.70 秒）；`cargo fmt --check` 与 `git diff --check` 通过。未做真人桌面视觉/鼠标回归。

## 同步与 MCP 设置长文本折行（2026-10-16）

- Slint 参考图显示同步状态和 MCP 段落在 560px 对话框右边界被硬裁切，部分字符缺失且不显示省略号。GPUI 设置页承载动态设备名、同步诊断、日志和 MCP 令牌，长值也必须受对话框内容宽度约束。
- GPUI 为同步/MCP 两个内容列和长状态字段加 `w_full`/`min_w_0` 约束；令牌展示仅在每 12 个字符间加入零宽断行点，复制配置仍使用原始令牌。
- 验收：隔离 Release GUI smoke 注入长同步状态/监听诊断和 MCP 状态，并实际渲染同步与 MCP 设置 tab；没有做真人窗口逐像素截图对照，因此视觉行高与最终换行位置仍待 P7 对照验收。未访问真实用户库。

## 主题与侧栏过渡状态回收（2026-10-16）

- 旧版动画到时自然结束并恢复界面输入；GPUI 原先在 220ms 插值完成后仍保留透明主题遮罩和侧栏过渡状态，可能影响主窗口命中与侧栏卸载判断。
- 现在动画开始时递增序号，220ms 插值后留 20ms 渲染缓冲，再清理对应状态；快速反向切换时用线性插值计算当前侧栏宽度作为新动画起点，旧任务只在序号仍匹配时清理，不能提前移除较新的动画层。
- 理由：完成态应回归普通可交互窗口，动画临时层不应成为长期状态。
- 验收：全新 `%TEMP%` SQLite Release GUI smoke 快速反向触发主题与侧栏过渡，断言新侧栏动画起点等于旧动画当前插值宽度、旧定时器不会提前清除新状态，并在最终期限后断言主题遮罩/侧栏过渡均已清除、侧栏最终展开且主题恢复。确定性测试覆盖线性插值中点、反向终点与边界。Release build 用时 46.06 秒；未采集本次构建峰值内存，完整性能对比仍按用户要求留待功能收尾。

## 任务清单操作入口（2026-10-15）

- 旧版把清单改名、改色和删除放在每行的统一“⋯”锚定菜单里。
- GPUI 现由统一“⋯”菜单提供重命名、颜色和删除清单；颜色打开含九种非默认颜色的子页，并可返回主菜单，Escape 逐层返回或关闭。菜单与颜色子页改用 GPUI anchored 浮层并由窗口边缘自动避让；菜单取得键盘焦点后，上下键循环选择、Enter/Space 执行，颜色子页也可键盘浏览；Escape 关闭主菜单时恢复打开前焦点。改名支持 Enter/失焦提交、Escape 取消和空名称校验，改名与改色各是一条 organizer 历史记录，可 Ctrl+Z/重做并持久化。
- 点击锚定菜单或颜色子页外部会关闭菜单；Escape 在颜色子页中先回到菜单，再次关闭并恢复打开前焦点。保留差异：统一 OverlayHost 与对其他弹层的统一点击外关闭/焦点回收仍未完成。
- 验收：隔离 ScratchDir 测试验证更新、空名称拒绝、undo/redo 和 SQLite 重开；Release 窗口 smoke 覆盖菜单焦点获取/ Escape 焦点恢复、菜单/颜色子页状态、外部关闭状态迁移，并调用 Workspace 改名和选色验证撤销/重做。鼠标打开后键盘操作由该窗口路径验证；真实鼠标命中/外部点击事件尚未自动化点验。未访问真实用户库。

## 页面选项与编辑器浮层外点关闭（2026-10-15）

- 页面选项面板提供页面图标与封面操作；点击面板以外会关闭页面选项，并一并收起其打开的图标候选。图标面板、正文块颜色选择器、块操作菜单和页面搜索面板也会在外部按下时收起；处理被延迟到当前鼠标事件结束，避免目标控件被卸载后丢失该次点击。
- Calendar/Timeline 的日期轴属性选择菜单、数据库表格单元格中的 Select/Relation/Files 菜单、看板 Select/Status 选择器，以及列、筛选、分组选项面板都会在外部按下时收起；关闭筛选面板时清理未提交的筛选编辑状态。
- 各菜单仍由各自页面状态管理，没有统一 OverlayHost。设置、速记草稿、JSON 指令三个模态对话框、页面搜索和命令面板现在保存并恢复打开前的 GPUI 焦点；Ctrl+F 查找关闭时聚焦当前命中块；锚定菜单和其他弹层的焦点回收策略尚未统一，真实窗口的点击命中仍需桌面验收。
- 页面选项浮层现在在打开时取得 GPUI 焦点；Escape 先关闭已打开的图标子面板，再次 Escape 关闭浮层并恢复原焦点。点外关闭会清除临时焦点记录但保留用户点击目标的焦点；Release smoke 覆盖浮层渲染与焦点恢复。
- 验收：Release 构建与隔离 GUI smoke 通过，既有速记、托盘、MCP、窗口状态路径继续通过。smoke 未模拟新增弹层的真实指针命中；锚定/定位与真人鼠标交互仍待桌面验收。

## 正文块拖放排序（2026-10-15）

- 旧版：从块手柄发起拖动，在目标块上方或下方释放；界面在合法目标位置画强调色落点线，core 拒绝跨越父子子树边界的非法重排。
- GPUI：拖动经过目标行上半区时显示行上缘落点线并放到该行之前；下半区显示行下缘落点线并放到该行之后。文档尾部留白区仍可追加到末尾。GPUI 复用 `MoveBlockTo` 与只读合法性校验，保留子树结构、单步撤销/重做和编辑焦点。
- 保留差异：精确半区命中、嵌套子树边界的真实鼠标效果尚未手动验收；拖动尾部支持追加。
- 验收：隔离 Session 测试验证合法/非法落点、重排顺序、undo/redo 与数据库重开；未访问真实用户库。

## 编辑器块移动快捷键（2026-10-16）

- 旧版声明了 `Ctrl+Shift+↑/↓` 块移动操作，但普通块先匹配无修饰键的上下导航分支，后面的修饰键移动分支不可达；GPUI 此前只有块悬停箭头和拖放。
- GPUI 现在让该快捷键优先路由到现有 `MoveBlock` 操作，沿用页面锁定检查、core 撤销历史和当前块焦点恢复；输入法组合期间忽略快捷键，避免打断未确认的文字。这样修复了旧快捷键在普通块上被导航分支截获的问题。
- 验收：同一 core 块移动 Session 覆盖顺序、撤销/重做与数据库重开；`cargo test --locked -j 8`、`cargo check --locked -j 8`、Release build 与隔离 GUI smoke 通过。快捷键真人按键验收尚未做。未访问真实用户库。

## 整理区回收站彻底删除与清空（2026-10-15）

- GPUI 修正前：笔记和任务可以进入回收站、恢复，但没有逐项彻底删除或清空当前类型回收站的入口。
- 新行为：回收站每行显示恢复和彻底删除；列表标题操作可清空当前笔记或任务回收站。core DeleteNote/DeleteTask 经 organizer 历史执行，单项删除和整批清空都可 Ctrl+Z 恢复；清空作为单个批次撤销项。
- 理由：补齐旧版回收站的两个必要管理操作，同时避免误删后无法恢复。
- 验收：隔离库测试覆盖笔记/任务单项删除、整批清空、撤销/重做与重开持久化；隔离 Windows Release smoke 实际调用 Workspace 操作路径并验证完整行为，未访问真实用户库。

## LAN 出站同步的 socket 超时（2026-10-15）

- 旧路径：GPUI 把 `SyncWith` 交给锁定的 quire-core transport；虽然调用方为 snapshot/附件传入 60–120 秒 timeout，core 建连后仍把读写 idle timeout 固定为 30 秒。
- 新路径：保留 core Engine 的 UDP 发现、入站 HTTP server、同步 job、snapshot wire format 与 merge；GPUI 自有出站 worker 按每个操作把 5/60/120 秒配置用于 TCP connect/read/write，维持相同 pull → merge → push 协议。
- 理由：大型 snapshot 或慢速附件链路不能被一个更短的硬编码 idle timeout 截断；不修改共享 quire-core revision，也不改变 Slint 端。
- 验收：单测检查 120 秒 timeout 同时设置到 socket 读写，并以 loopback 假 peer 执行完整 info/snapshot/attachment/push 往返；双 Session 集成测试、74 项全量测试、Release build 与隔离 GUI smoke 通过。真实设备网络仍待验收。

## 页面历史导航与 HTML 外部打开（2026-10-15）

- GPUI 原先没有页面历史后退/前进或选择本地 HTML 并交由浏览器打开的入口。
- 新行为：Alt+←/→ 在最近 50 个页面访问之间切换；Ctrl+K 可筛选“后退”“前进”和“在浏览器中打开 HTML…”。无效/已删除历史项会跳过；系统无法启动默认文件处理程序时状态栏显示错误。HTML 选择器只接受 `.html`/`.htm`。
- 理由：恢复旧版已有的导航与 HTML 预览动作；复用操作系统默认处理程序，不把文件内容导入 Quire。
- 验收：纯历史栈测试覆盖连续重复项与 50 项上限；编译、全量测试与 Release smoke 验证记录见 DELIVERY-STATUS。本地文件选择器及不同浏览器注册情况未在自动 smoke 中实际点击验证。

## 页内查找的虚拟列表远端命中（2026-10-15）

- GPUI 修正前：查找结果计数扫描整页可编辑块；导航仅在命中块的 TextInput 已挂载时选区，因此视口外结果显示在计数中但不会滚动或选中。搜索还会把折叠子树中的可编辑文本计入结果。
- 新行为：仅在当前页结构上可见且可编辑的块中搜索；导航按稳定 BlockId 对应的列表索引滚动，重复等待直到虚拟行编辑器挂载，再选中 UTF-8 命中范围并聚焦。
- 理由：计数和可导航结果必须一致；虚拟化只延迟编辑器创建，不应让整页后半段无法查找。
- 验收：隔离 Release 窗口在 1000 个合成块中查找屏幕外第 500 条，验证目标行被挂载并选中完整文本范围；`cargo test`、`cargo check` 与格式检查通过。未读取真实用户库。

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

## 整理区 JSON 指令批次（2026-10-16）

- 旧版：笔记和任务页签均可打开“指令…”面板，粘贴 AI 返回的 `{"operations":[...]}`；逐项报告成功/失败，整个成功子集经一个 `exec_all` 历史项提交，因此一次 Ctrl+Z 撤销整个批次。
- GPUI：已提供同类入口、页签专属示例复制、多行 JSON 输入、操作说明、逐项执行计数、错误保留与关闭后焦点恢复。Session 通过 core organizer 操作持久化笔记/任务的创建、编辑、标签、评论、回收站及任务完成/移动/优先级/截止日期/子任务。
- 当前差异：GPUI 与旧版一样保留成功子集、逐项报告失败；Session 暂存各 core-backed 操作的 apply/revert changes，最后合成一条 organizer undo entry。无效操作不会变更；一次 Ctrl+Z 撤销本批所有成功操作，Ctrl+Shift+Z 重做整批。
- 验收：隔离 SQLite Session 测试覆盖 note 多操作、task 创建及子任务/状态组合的一次撤销和重做，以及失败计数与重开读回；Release GUI smoke 挂载输入浮层、验证错误 JSON 保留原稿，再运行创建笔记的 JSON 并确认关闭。真人鼠标/键盘操作仍待用户桌面反馈。未访问真实用户库。

## 整理区笔记与任务行菜单（2026-10-16）

- 旧版笔记行提供“⋯”菜单并支持长按；GPUI 笔记行/卡片现在提供“⋯”和鼠标右键两个入口。菜单包括打开详情、复制正文/标题及唯一 ID、置顶、转任务、移入回收站。GPUI 用右键作为桌面长按以外的入口；回收站继续显示恢复和彻底删除按钮。
- 旧版任务行菜单提供打开、复制、完成状态和回收站操作；GPUI 任务行现有相同菜单，并保留原有勾选快捷入口。回收站/多选时菜单关闭，沿用原来的恢复/彻底删除与批量操作。
- 验收：隔离 Release smoke 挂载两种菜单并调用笔记置顶、任务完成动作；`cargo test --locked` 与 Release build 通过。菜单真实鼠标坐标、右键输入和外点关闭仍待真人桌面验收。未访问真实用户库。

## 页面树右键菜单键盘操作与焦点恢复（2026-10-16）

- GPUI 页面树右键菜单此前仅支持指针选择，关闭后没有明确恢复打开前焦点的路径。
- 现在菜单打开时记录并接管键盘焦点；上下键在十个页面操作间循环选择，Enter/Space 执行当前操作，Escape 关闭并恢复原焦点。页面操作继续复用现有 Workspace/core 命令路径。
- 验收：边界/环绕由菜单选择器单测覆盖；全新 `%TEMP%` 库 Release smoke 通过页面菜单打开、焦点接管、渲染与 Escape 焦点恢复的窗口路径。此 smoke 没有模拟真人右键坐标或物理键盘输入；这些桌面命中仍待候选验收。未访问真实用户库。

## 固定侧栏入口菜单键盘操作与焦点恢复（2026-10-16）

- GPUI 固定侧栏工作区、笔记、任务、搜索和设置入口此前的右键菜单只支持指针选择，没有可见键盘选择状态或关闭后的焦点恢复路径。
- 菜单打开后现在接管焦点；上下键循环高亮当前入口可用的操作，Enter/Space 执行，Escape 关闭并恢复原焦点。点击菜单外关闭时保留用户点击目标的焦点。
- 验收：全新 `%TEMP%` 库 Release smoke 覆盖设置入口菜单打开、焦点接管、渲染和 Escape 恢复。自动 smoke 不合成物理右键或按键事件，真实桌面命中仍需用户反馈；未访问真实用户库。

## 整理区笔记与任务行菜单键盘操作（2026-10-16）

- 整理区笔记/任务行的“⋯”和右键菜单原先只有鼠标操作；键盘焦点没有进入菜单，Escape 也没有恢复路径。
- 现在打开菜单会接管焦点；上下键循环选择当前菜单动作，Enter/Space 执行，Escape 关闭并恢复打开前焦点。普通笔记/任务菜单和回收站“恢复/彻底删除”菜单均按各自的动作顺序导航；菜单外点击继续聚焦点击目标。
- 验收：隔离 Release smoke 覆盖笔记置顶、任务完成、菜单焦点接管与 Escape 恢复。自动 smoke 没有合成物理右键/按键；真实桌面命中仍待用户反馈。未访问真实用户库。

## 编辑器块操作菜单键盘操作（2026-10-16）

- 编辑器块“⋮”菜单此前只支持指针操作，菜单和转换/图片宽度/移动页面子页没有键盘选择或焦点恢复路径。
- 现在菜单取得 GPUI 焦点；上下键循环高亮当前菜单项，Enter/Space 执行，Escape 从子页返回主菜单，再按 Escape 关闭并还原打开前焦点。复制、转换、颜色、删除、粘贴、图片宽度和移动页面沿用既有操作。
- 验收：隔离 Release smoke 覆盖块菜单打开、挂载渲染与 Escape 焦点恢复；自动 smoke 没有合成真实物理按键或鼠标，方向键/动作需候选桌面验收。未访问真实用户库。

## 编辑器块颜色选择器键盘操作（2026-10-16）

- 块文字/背景颜色色板此前只能用鼠标点选，弹出色板时没有焦点目标或关闭时的焦点恢复。
- 色板现在接管焦点并标记当前键盘色块；方向键循环移动，Enter/Space 应用颜色，Escape 关闭并恢复原焦点。菜单外点击关闭仍将焦点留在点击目标。
- 验收：隔离 Release smoke 覆盖色板打开、渲染和 Escape 焦点恢复；自动 smoke 未合成物理键盘输入，颜色选择的 OS 输入路径仍可由用户实际使用后反馈。未访问真实用户库。

## 数据库筛选子面板键盘操作（2026-10-16）

- 数据库筛选的属性、比较运算符和选项子面板此前只能用鼠标选择，打开时也没有明确键盘焦点与返回路径。
- 筛选面板现在接管焦点；子面板支持方向键循环高亮、Enter/Space 选择，Escape 返回规则面板，再次 Escape 关闭并恢复原焦点。菜单外点击关闭会保留目标控件的焦点；规则变更仍走原有 core-backed 数据库视图持久化操作。
- 验收：隔离 Release smoke 覆盖筛选面板打开、渲染和 Escape 焦点恢复；自动 smoke 未合成物理方向键或点击，真实桌面输入仍可由用户后续反馈。未访问真实用户库。

## 数据库分组选择器键盘操作（2026-10-16）

- 数据库视图的分组属性选择器此前只能用鼠标选择，打开后焦点也没有转交给选择器。
- 选择器现在接管焦点；上下键在“无分组”和支持分组的属性间循环，Enter/Space 应用选择，Escape 关闭并恢复原焦点。切换到筛选或列面板时先正确关闭分组弹层并还原其焦点。
- 验收：隔离 Release smoke 覆盖分组选择器打开、渲染和 Escape 焦点恢复；自动 smoke 未合成物理方向键或鼠标，真人操作可由用户后续反馈。未访问真实用户库。

## Embed 卡片地址编辑

- 旧版 Embed 是链接卡片，地址存放在块正文中；点击卡片进入同一正文输入，标题和可打开地址由当前输入派生。
- GPUI 现在允许 Embed 块编辑地址：点卡片聚焦地址输入，编辑时保持卡片高度；离开输入按正常文本编辑路径防抖保存并进入撤销历史。标题与地址显示、允许的 http/https/mailto 外部打开行为不变。
- 验收：隔离 Session 测试验证地址修改、撤销/重做、flush 与 SQLite 重开；Release GUI smoke 确认 Embed 输入实体挂载并能取得焦点。卡片真人鼠标命中与中文输入体验尚待桌面回归；未访问真实用户库。

## 数学块编辑

- 旧版数学块在阅读状态显示公式预览，进入编辑后可直接编辑 LaTeX 源文。
- GPUI 现在允许点击公式卡片切换到同一块正文的输入框，未编辑时继续由 core 数学子集生成 Unicode 预览；编辑输入沿用常规块的撤销/重做与持久化路径。
- 验收：隔离 Session 测试覆盖源码修改、撤销/重做、flush 与 SQLite 重开；Release GUI smoke 验证预览与聚焦输入可先后渲染。实际鼠标命中及 IME 输入仍待桌面回归。
# 托盘版本提示与整理区笔记卡片（2026-10-16）

- GPUI 托盘图标此前只显示固定的 `Quire`；笔记卡片悬停时同时显示置顶、删除和菜单，且卡片标题没有固定的单行布局，窄宽下会被操作区挤压。
- 托盘提示现在使用 `quire-{版本号}(gpui)`。普通笔记卡片显示单行省略标题，悬停只显示“⋯”菜单；置顶继续显示在元数据行，回收站只显示恢复与彻底删除。
- 验收：源码对照 Slint 整理区卡片的标题/菜单和回收站动作；`cargo fmt --check` 与 `git diff --check` 已运行。未做桌面鼠标回归或构建验证。
# 任务平铺看板高度约束（2026-10-16）

- 旧行为：任务看板列采用 `h_full()`，但横向滚动容器没有明确高度；GPUI 无法稳定解析列及卡片列表的可用高度，平铺视图中任务卡片可能不可见。
- 新行为：横向滚动容器占满任务内容区并设置 `min_h_0()`，让列和卡片滚动区域有确定的高度约束。
- 理由：修复用户反馈的任务平铺视图没有正常显示任务。
- 验收：未运行构建或桌面验证；布局修复仍需在应用中确认。

## 文档编辑焦点与侧边栏选中态（2026-10-16）

- 旧行为：整理区打开时，“笔记”或“任务”侧边栏项保持选中，即使键盘焦点已经进入文档编辑器。
- 新行为：文档块输入获得焦点后，侧边栏不再显示“笔记/任务”的选中态；焦点离开文档后按整理区当前页恢复。
- 理由：同一时刻只应有一个区域呈现为当前焦点。
- 验收：未运行构建或桌面验证；需在应用中确认文档输入焦点与侧边栏选中态切换。

## 深色渐变与整理区卡片层次（2026-10-16）

- 旧行为：窗口根背景直接使用渐变画笔，渲染时可能被压平成首个颜色；任务列表行透明，难以和页面背景区分。
- 新行为：用全窗口背景图层绘制 `bg` 到 `grad2` 的渐变；笔记卡片保留抬高底色，任务列表行使用抬高底色和完整边框，悬停切换到第二层底色。
- 理由：让四套渐变主题在整个窗口可见，并使笔记、任务卡片从背景中清楚分离。
- 验收：`cargo fmt --check`、`cargo check --locked` 通过；未做桌面视觉回归。

## 笔记列表时间流与概览侧栏（2026-11-05）

- 旧行为：笔记列表沿用居中卡片流，窄内容列之外没有笔记概览；标签过滤留在整理区左侧导航。
- 新行为：笔记列表改为横向时间行，时间/操作区固定在左侧、正文摘要占据主体；桌面宽窗口右侧显示近 7 天数量、标签总数和可点击的标签/排除操作。笔记列表态隐藏整理区内部导航，让工作区主侧栏与笔记流直接衔接。
- 理由：按用户提供的笔记界面参考图调整布局层级与信息密度。
- 验收：`cargo fmt -- src/view.rs` 与 `cargo check --locked -j 8` 通过；未做截图像素对照或真实鼠标回归。运行窗口仍使用隔离的 `%LOCALAPPDATA%\\QuireGPUI\\quire.db`。

## GPUI 版本显示名（2026-11-05）

- 旧名：`quire-0.1.0(gpui)`。
- 新名：`quire-gpui-0.1.0`，统一用于主窗口、速记窗口、托盘提示、Windows 文件描述/产品名和安装器版本名。
- 验收：Release EXE 文件属性读回 `ProductName=quire-gpui-0.1.0`、`FileDescription=quire-gpui-0.1.0`；运行窗口标题读回相同值。

## 新建笔记与发送提示（2026-11-05）

- 笔记页浮动“＋”现在打开新建笔记浮层；浮层采用更宽的编辑区、标题说明、标签快捷提示和独立发送按钮。提交笔记或速记后显示 2.6 秒成功 toast，取消仍丢弃草稿。
- 版本升至 `0.1.1`，主窗/速记窗/托盘/Windows 产品元数据继续使用 `quire-gpui-0.1.1` 格式。
- 验收：`cargo fmt --all -- --check`、`cargo check --locked -j 8`、`cargo build --release --locked -j 8` 通过；Release EXE 读回 ProductName/FileDescription `quire-gpui-0.1.1`、FileVersion `0.1.1.0`。未运行自动化测试或真人桌面视觉验收。

## 笔记操作入口、多选与手动排序（2026-11-05）

- 旧行为：笔记卡片和详情面板显示复制、置顶、转任务、删除等按钮；多选仅由工具栏按钮进入；笔记不能拖动调整顺序。
- 新行为：卡片与详情操作收进右键菜单；右键菜单可进入多选，之后右键笔记切换选中状态；笔记卡片可拖放排序，顺序记入 GPUI 设置。编辑仍使用标题/正文/标签输入的自动保存流程。
- 理由：减少卡片上的操作按钮，并让多选和整理动作可由右键完成。
- 验收：`cargo check` 通过；未做桌面鼠标、拖放或中文输入回归。

## 笔记详情交互与界面修正（2026-11-05）

- 笔记列表首击只选中，再次点击同一条才打开详情；关闭详情保留选中项。详情占据主工作区，隐藏列表列，避免显示成侧栏。
- 新建笔记提交后不再自动预选，确保下一次点击仍遵循首击选中规则；“已发送”提示改为高对比深绿底白字并延长到 4 秒。
- 任务列表新增右下角浮动“＋”按钮；移除标题栏窗口控制左侧的搜索按钮。
- 版本升至 `0.1.2`，应用显示名使用 `quire-gpui-0.1.2`。
- 验收：`cargo fmt --all`、`cargo check --locked -j 8`、`cargo test --locked -j 8`、`cargo build --release --locked -j 8` 通过；使用隔离临时数据库完成真实窗口验收，确认 toast、首击/二击行为、全宽详情、任务加号和标题栏布局。

## 页面悬停操作与版本更新（2026-11-05）

- 页面树条目悬停时不再显示快捷操作按钮，页面管理操作继续通过右键菜单访问。
- Cargo patch 版本升至 `0.1.3`；窗口和 Windows 产品标识格式为 `quire-gpui-0.1.3`。
- 验收：格式检查、编译检查和 Release 构建通过；确认页面行仍保留右键菜单事件。

## 回车输入与提示收敛（2026-11-05）

- Enter 创建新段后立即补齐输入事件订阅，再设置焦点，确保新段的连续输入正常到达工作区。
- 热键占用静默降级；同步设置页隐藏非致命监听冲突警告。
- 版本升至 `0.1.4`，应用标识使用 `quire-gpui-0.1.4`。
- 验收：编辑器键盘事件单测与 `cargo check --offline -j 8` 通过；桌面真人验收被用户的 Escape 中断。

## 编辑器输入补全：代码块多行、快捷键与鼠标选区（2026-11-06）

- 旧行为：GPUI 代码块按 Enter 会拆成两个代码块，无法键入多行代码（旧版编辑器 `single-line: block.kind != 8`，代码块是唯一多行块）；Ctrl+L 打开链接编辑器与 Ctrl+M 切换行内数学标记两个旧版快捷键缺失；双击无法选词、三击无法全选块；代码块内 Tab/Shift+Tab 无效果。
- 新行为：代码块 Enter 经普通输入路径插入换行（斜杠/提及菜单打开时仍优先解析），Up/Down 仍按行导航并可越过块边界；新增 `ctrl-l`（打开链接编辑器，行为同工具栏"链接"按钮）与 `ctrl-m`（切换 Math 标记，需非空选区）；双击按 UAX#29 选词（词内点击选词、词边界点击优先闭合词、点在多字节字素如 emoji 内部选中该字素、非词字符选中单个字素），三击选中整块文本；代码块 Tab 写入两个空格、Shift+Tab 删除光标前至多两个空格（不越过行首），均走普通编辑与撤销路径。
- 理由：对齐旧版已声明的多行代码行为并补齐文档编辑的常规键盘/鼠标能力。
- 验收：新增单测覆盖词选区边界（含 CJK 单字成词、emoji 字素回退、文本末尾闭合词）、ctrl-m/ctrl-l 按键分发、多行 Tab/Shift+Tab 与单行 Tab 事件分流、双击/三击选区、`enter_splits_block` 类型判定；`cargo fmt --all -- --check`、`cargo check --all-targets --locked` 与全量 `cargo test --locked -j 8`（123 项）通过。真实鼠标双击与中文输入体验仍待桌面真人回归；未访问真实用户库。

## 单实例交接与速记窗口行为（2026-11-06）

- 旧行为：GPUI 版重复启动同一数据库会直接弹"此库已被另一个 Quire GPUI 实例打开"错误框，正在运行的实例（可能藏在托盘）毫无反应；速记窗口以 Windows 原生 TOPMOST 置顶，切到其他应用时仍悬浮在最上层；速记窗口和笔记页"新建笔记"浮层在 Esc/×/Alt+F4 关闭后直接丢弃已输入草稿，重开从零开始。
- 新行为：启动时按数据库路径哈希创建命名管道 `\.\pipe\quire-gpui-show-<hash>`，第二次启动检测到同库实例即通过管道发送 show 并静默退出，运行中的实例收到 ShowMain 事件恢复并激活主窗口（与托盘"显示主界面"同一路径）；不同数据库（隔离 smoke、并行工作区）各用各的管道互不干扰。速记窗口不再置顶，按普通窗口层级处理；速记窗口 Esc/×/Alt+F4 与笔记浮层 Esc/×关闭时把未发送草稿（含速记未落库的 pending note id）缓存进工作区，再次打开优先恢复缓存草稿（覆盖标签预填），提交成功后清空缓存。
- 理由：对齐旧版 Slint 的单实例 pipe 交接行为；按用户要求速记窗口不应常驻最上层，且误关不应丢字。
- 验收：新增单测覆盖管道名按库稳定且互异、真实命名管道 loopback（监听线程→客户端写入→ShowMain 信号）与无监听时的 false 返回、草稿缓存仅保留非空白文本；`cargo fmt --all -- --check`、`cargo check --all-targets --locked` 与全量 `cargo test --locked -j 8`（127 项）通过。真实桌面双开、托盘唤回与 Alt+F4 场景待真人验收；未访问真实用户库。

## 文档排序模式：右键移动入口改为专门排序（2026-11-06）

- 旧行为：块右键菜单带"移动到页面…"子菜单逐块移动；正文拖拽与文本选择混在同一手势里，没有块选择概念，批量移动无从谈起。
- 新行为：右键菜单的移动入口换成"排序…"，进入整页**排序模式**——顶部出现模式条（拖动块调整顺序 · 点击块选中 · 已选 N 块 · 全选 / 移动到… / 完成），排序模式下块行整体可拖（复用既有 EditorBlockDrag 与落点线）、点击变为选中/取消选中（输入不再抢焦点），选中块经"移动到…"页面列表批量移动到目标页；Escape 或"完成"退出，切换页面自动退出。批量移动走新增的 `Session::move_blocks_to_page`：跳过已被祖先覆盖的选择、任一规划失败整体放弃、全部移动合并为**单条可撤销历史**并一次落盘。
- 理由：按用户要求把文档内移动统一为排序模式 + 单选/多选批量移动，右键菜单不再承担移动职责。
- 验收：新增 Session 单测（父子同选只移动父、单步 undo/redo、flush 重开读回目标页）与菜单动作表单测（无移动项、以"排序…"收尾、键盘索引一致）；`cargo fmt --all -- --check`、`cargo check --all-targets --locked` 与全量 `cargo test --locked -j 8`（130 项）通过。真人拖拽与批量移动待用户在场时验收（启动会触发防火墙弹窗，按用户要求未运行）。

## 整理区工具条按钮只剩空壳（2026-11-06）

- 旧行为：`toolbar_chip(label, color, active, palette)` 只设样式、从不把 `label` 放进 Div，笔记/任务工具条的 11 处调用点也都不再补 `.child(...)`，于是"复制全部""指令…""最新创建 ⌄""多选""＋清单""清空回收站"等按钮全部绘成约 20×10 的空描边胶囊，只剩边框和悬停底色，读不出任何动作。
- 新行为：`toolbar_chip` 末尾追加 `.child(label)`，胶囊按文字宽度撑开，11 处调用点一次修复；文字仍走 `text_size(px(11.))` 与传入色，active 态的描边/底色不变。
- 理由：该 helper 的注释写明"每个工具条动作都走这里"，丢掉入参是纯缺陷，不是与旧版的设计差异；旧版 Slint 对应位置始终显示按钮文字。
- 验收：新增 `view::view_tests::toolbar_chip_lays_out_the_label_it_is_given`，在同一窗口里并排渲染"复制全部"与空标签两颗胶囊，用 `debug_bounds` 读实际布局宽度并要求带字的那颗 >1.5×空壳（实测 47.0 vs 20.0；修复前两者同为 20.0，门直接红）。隔离库实拍对照：同一笔记页修复前工具条为四颗空胶囊，修复后四颗分别显示"复制全部/指令…/最新创建/多选"。`cargo fmt --all -- --check`、`cargo check --all-targets --locked` 与全量 `cargo test --locked -j 8`（132 项）通过；真人鼠标点击这些按钮的验收仍待用户在场时补。

## 侧边栏与整理区分隔条拖拽：按下即崩、只跟一帧（2026-11-06）

- 旧行为：三处宽度拖拽（侧边栏右边缘 6px 手柄、任务页列表与详情之间的分隔条、数据库表格列头分隔线）都在 `on_mouse_down` 事件处理器里调用 `window.on_mouse_event` 注册全局 move/up 监听。gpui 的 `Window::on_mouse_event` 内部断言"只能在 paint 期间调用"，从事件处理器调用在 debug 构建里直接 panic——真机在侧栏边缘按下左键，进程当场退出（release 构建不 panic，但 `Frame::finish` 不保留 paint 之外注册的监听器，监听器只活一帧，第一次 move 之后手柄就不再跟手，光标一旦离开 6px 也彻底失联）。
- 新行为：按下只做"上膛"（置 `sidebar_resizing` / `organizer_list_resizing` / `database_column_drag`），`render` 在存在上膛拖拽时把一层铺满视口的透明 `resize-overlay` 追加为根节点最后一个子节点，`on_mouse_move` / `on_mouse_up` / `on_mouse_up_out` 全挂在这层自己的元素监听上——`div` 每帧 paint 会重新注册自己的监听器，所以整条拖拽链跨帧存活，光标跑出 6px、跑出窗口、在任意位置松手都能收尾。move 与 up 共用 `update_resize_drag` / `finish_resize_drag`：侧栏 200–480 clamp 后写 `gpui.sidebar-width`；任务分隔条 240–(整理区宽−380) clamp 后写 `gpui.organizer.task-list-width`（笔记态那支写 `gpui.organizer.note-detail-width`，钳的是详情列宽）；数据库列宽按千分比（60–1060）写回视图定义，失败落状态条"数据库列宽保存失败：…"并置 error。三处都 `schedule_save(true)`。启动读取 `gpui.sidebar-width` 也补了同一 200–480 clamp，脏值不再撑破布局。
- 理由：不是设计差异，是用错了注册通道。gpui 里唯一能跨帧存活的监听器是元素自己 paint 时注册的那批；6px 手柄本身抓不住整段拖拽，必须有一层覆盖全视口的元素替它接管 move 与 release。
- 验收：新增三道门（隔离库，`gpui::test` + `debug_bounds`/`simulate_mouse_*`）。`sidebar_resize_handle_keeps_tracking_the_cursor`：按下手柄后连投 40/80/120px 三次 move，要求每一步宽度都比上一步大 >1px（只活一帧的监听器会让第一步动、后两步原地不动，留下 `[300.0, 300.0, 300.0]` 这种台阶），最终宽度 = 起始 + 120（实测 380.0 vs 260.0+120），松手后 `gpui.sidebar-width` 读回 `380.0`；把 `resize_drag_active` 改成恒 `false` 的变异体立刻红在 `each move must widen the panel: [260.0, 260.0, 260.0]`。`organizer_list_handle_keeps_tracking_the_cursor`：先 `create_task` 让任务详情打开（分隔条只在"详情打开 + 双栏 + 非看板"时渲染），同一手法拖 120px，`organizer_task_list_width` 320.0→440.0 且 `gpui.organizer.task-list-width` 读回 `440.0`；两条变异分别红在"each move must widen the task list: [320.0, 320.0, 320.0]"与"releasing the divider must persist the dragged width: left None, right Some("440.0")"。`sidebar_resize_handle_tracks_on_the_task_page`：切到任务页后侧栏手柄仍能逐步变宽（侧栏是共享壳层，笔记页/任务页共用同一手柄）。真机（debug 构建、默认库、PID 104136）用 PostMessage 投 OS 级左键：修复前同一坐标按下即 panic 退出；修复后按下 x=257 手柄高亮出现在截屏 x=508–519（该窗口 192 DPI，截屏为输入坐标 2×，说明命中的正是手柄），连投三次按住左键的 move 把面板右边界从 503→589→669→749 推过去，宽度增量与光标输入增量都是 123（1:1 跟手），松手 `gpui.sidebar-width` 读回 `383.0`，再原路拖回读回 `260.0`；进程全程存活。`cargo fmt --all -- --check`、`cargo check --all-targets --locked` 与全量 `cargo test --locked -j 8`（135 项）通过。笔记页没有列表/详情分隔条：`organizer_single_pane = note_detail_open || …` 使笔记态永远是单栏，手柄条件不可满足，本轮按"不扩布局"处理，只把笔记分支的钳位留在共用函数里。真人手拖待用户在场时验收；未访问真实用户库以外路径。

## 视图状态不落盘：宽度记住了，开合与折叠归零（2026-11-06）

- 旧行为：侧栏宽度拖完写 `gpui.sidebar-width` 并落盘，但同一块界面上的其余选择全是内存态——`sidebar_open`（◫ 按钮、命令面板"显示/隐藏侧边栏"）、收藏/最近分组折叠、任务页"平铺/列表"看板开关，重启一律回到"侧栏打开 + 两组折叠 + 列表"。任务页另两个宽度设置 `gpui.organizer.task-list-width` / `note-detail-width` 启动读取只 `parse().ok()`，没像 `gpui.sidebar-width` 那样过滤非有限值：库里存进 `NaN`/`inf` 时 `clamp` 对 NaN 不生效，宽度带着 NaN 进布局，下一次拖拽的分隔条偏移再由它算出来并把 `NaN` 写回设置，越拖越坏。收藏/最近两个可点标题带 `cursor_pointer` 却没有任何 hover 底色，同列的收藏/最近页面行和"＋ 新建页面"都有。进回收站会把 `organizer_board_mode` 直接清零，退出回收站也不还原，用户"平铺"的选择就此丢失。
- 新行为：四个新键 `gpui.sidebar-open`、`gpui.sidebar.favorites-collapsed`、`gpui.sidebar.recents-collapsed`、`gpui.organizer.board-mode`，值沿用 `gpui.quick-note-on-start` 的 `"true"/"false"` 写法；`toggle_sidebar`、`toggle_organizer_board_mode` 加 `window` 入参以便走 `set_setting` + `schedule_save(true)`（与主题、宽度同一条保存路径，状态条同样显示"正在保存…"），收藏/最近的标题点击抽成 `toggle_favorites_collapsed` / `toggle_recents_collapsed`。启动读回统一走新增的 `bool_setting`：只认 `true`/`false`，`yes`/`1`/`TRUE` 这类脏值回默认而不是猜。两处宽度读取补 `.filter(is_finite)`（布局渲染期本来还会按视口再钳一次，这里只挡非有限值，不再另设上限）。回收站改为进入时隐藏看板列但不覆写设置，退出时按存储值还原——设置与字段不再互相说谎。收藏/最近标题补 `rounded(5)` 与 `hover` 洗色 `p.bg @0.35`，与其下方页面行同一档，不是新发明的浅色。
- 理由：与宽度拖拽同源。用户在一块壳层上做的选择应当活过重启，否则"记住了宽度、忘了开合"是自相矛盾的持久化；回收站那段是持久化的前提，否则字段和设置会出现一次没人选过的翻转。
- 验收：新增四道门。`the_view_toggles_record_their_choice_in_the_settings`：连点四处后要求设置表读出 `[Some("false"), Some("false"), Some("false"), Some("true")]`；把 `toggle_sidebar` 的键名改成 `gpui.sidebar-open-x` 立刻红在该门，把 `toggle_recents_collapsed` 的键名同样改掉则红在第三项 `[Some("false"), Some("false"), None, Some("true")]`。`a_reopened_workspace_restores_the_saved_view_choices`：用裸 `Session` 写四键 + `force_flush()` + `drop`（数据库是独占锁，释放后第二个工作区才按新进程的方式读文件），要求重建出的字段是 `(false, false, false, true)`；把启动字段退回硬编码 `sidebar_open: true` 红在该门，把 `favorites_collapsed` 退回 `true` 也单独红在该门（`(false, true, false, true)`）。`a_garbage_view_setting_reopens_with_the_default`：库里塞 `-1 / yes / 1 / TRUE / NaN / inf` 六行脏值，要求侧栏宽被钳回 200（不是丢弃）、任务列表宽落回默认 320、笔记详情宽回 `None`、三个布尔全回默认；删掉 task-list 的 `.filter(is_finite)` 后该门红在 NaN。`the_task_board_view_resumes_after_a_trip_through_the_trash`：平铺→进回收站（列必须收起来）→出回收站（平铺必须回来）；把退出分支还原成旧的无条件 `false` 红在该门。真机（debug 构建、`--db` 指向 `%TEMP%\quire-stateslice\quire.db` 的隔离库，三臂各起一次进程、PrintWindow flag=2 截 1280×800）：库里没有这些键时侧栏右边界稳定落在截屏 x=518（260 设计像素 × 该窗口 2× 采集，50 行采样里 40 行读到 518，另 10 行读到 958/956/782，那是居中正文列的边缘）；只写 `gpui.sidebar-open=false` 再启动，50 行扫描里有 40 行左半区已无边界带（读回 `-1`），余下 10 行是 704–740 与 522 两组正文列边缘；再写 `sidebar-open=true` + `sidebar-width=420.0` 启动，边界移到 x=838（≈420×2），说明宽度与开合走的是同一套启动读取。`cargo fmt --all -- --check`、`cargo check --all-targets --locked`（沿用此前 2 条 dead-code 告警）与全量 `cargo test --locked -j 8`（139 项）通过。真实用户库只读校验：本轮未写入，`settings` 表仍只有 `gpui.sidebar-width=260.0` 等原有行，四个新键缺席=下次启动仍是当前出厂表现。真人点侧栏 ◫、折叠收藏/最近、切平铺后重启的回归待用户在场时验收；`organizer_note_board_mode`（笔记平铺）在产线仍没有任何入口，只在 `--smoke-test` 里被直接赋值，本轮未加入口、未持久化。

## 块悬停导轨、页面面包屑与替换的整页撤销（2026-11-06）

- 旧行为：（1）文档块没有任何鼠标入口——两块之间插入只能把光标移到块尾敲斜杠，移动/删除只能右键开块菜单，指针扫过整页时正文左侧那条 42px 的槽一直是空的。（2）子页面打开后正文上方只有页面标题，看不出它挂在哪个页面之下，也没有回到父页的入口，父链只能去侧栏页面树里展开找。（3）Ctrl+F 查找栏早已存在（上一处/下一处/结果计数），但"替换"这一行是否参与布局无人管；`replace_all` 逐块调 `Session::replace(boundary = true)`，N 个块就往撤销栈压 N 条 `Entry`，而状态行同时写的是"Ctrl+Z 可撤销"——按一次 Ctrl+Z 只交还最后一个块，页面停在半替换态。（4）动作消息与保存态抢同一行：`schedule_save` 函数第一行就无条件把 `self.status` 覆成"未保存"（`immediate` 时"正在保存…"），350ms 后保存完成再覆成"已保存"，而渲染期只有 `status == "已保存"` 时才显示字数统计。本轮逐点数过：全库 11 处"（Ctrl+Z 可撤销）"消息里 10 处写在 `schedule_save` 之前——块粘贴 `src/view.rs:9206`、块移动 `9449`、任务完成 `13010`、任务移动 `13040`、结构块粘贴 `13299`、清单改名 `13320`、清单改色 `13350`、清单删除 `13371`、彻底删除 `13621`/`13643`，这些消息在屏幕上根本来不及出现；只有版本恢复 `14038` 后面不接保存，消息留得住。
- 新行为：（1）悬停导轨。指针所在行的行左缘外挂 `+`（12px）与 `⋮⋮`（11px），定位是 `absolute().left(px(-42.)).top(px(2.))`，不是行的 flex 兄弟；实测悬停前后同一行的 `origin.x` 与 `size.width` 完全相等，正文可读列宽不因 hover 变化。`+` 走 `insert_block_after_and_open_menu`（在该块后插一块，斜杠菜单开在**新块**上而不是被点的行），`⋮⋮` 走 `toggle_block_context_menu`（同一按钮第二次点击收起）。离开时导轨不立刻卸载：`document_rail_exit` 记下那一行，用 `Animation::new(MOTION_HOVER = 120ms).with_easing(rail_easing)` 淡出，`MOTION_HOVER_SETTLE = 140ms` 后才释放；淡出那一层只画裸字形，没有 `.id()`、没有监听器、没有 `cursor_pointer`，因此 gpui 不给它生成 hitbox——在它正上方连点两下，块数、斜杠菜单、块菜单全部不变。`rail_easing` 起步慢收尾也慢（`rail_easing(0.25) < 0.25`、`rail_easing(0.75) > 0.75`）且全程单调，不是线性。跨行移动时 enter 与 leave 两个事件按 paint 顺序到达、谁先谁后不定，`track_document_hover` 因此只处理"与自己那一行同名"的离开，绝不清掉别一行刚声明的 hover；释放计时器再带一个 `document_rail_exit_seq` 序号，旧淡出的定时器不许撤走更新的那一层。（2）页面面包屑。新增 `page_breadcrumb(pages, page)`，沿 `parent` 走到根、root-first 输出、不含当前页自身，`seen` 环检测 + 16 级封顶，父页悬空则回空轨道；渲染在标题上方一行（`.mb(px(10.))`），每段是 `breadcrumb-{PageId}` 可点元素，点击 `activate_page` 过去。轨道为空时整条元素不渲染、不占布局：实测根页正文行 `origin.y` 与"进子页 → 点面包屑回父页 → 回根"之后的读数精确相等。（3）替换。查找条的第二行（替换输入框 + 替换此处/全部替换）只在 `find_replace_open` 为真时进布局，展开时条高变大、其下正文整体下移，收起后条高与正文行 `y` 都回到展开前的值。`replace_all` 改为先起草每个块的新文本、再经新增的 `Session::replace_many(edits)` 一次写入：整页只压一条 `Entry { apply, revert }`、persistence 只记一次，marks 依新文本重排后并进同一条 revert 的头部，一次 Ctrl+Z 交还整页；`replace_current` 仍走单块 `replace`，一步撤销一处。（4）替换的两条成功消息改走浮条：`show_toast` 增加 `detail` 入参（原先第二行硬编码"笔记已保存到列表"，两个调用点同批补上该文案），替换写"已替换 N 处（M 个块）/ Ctrl+Z 可撤销"，4 秒自动收、`toast_generation` 守卫防旧计时器抹掉新条；`status` 那行交还给保存态。上面第（4）点的其余 10 处消息本轮未迁——它要改 10 个入口并先定"消息与保存态谁占这条线"的规矩，记在下面未决项里。
- 理由：导轨做成覆盖层而不是加一列 flex 兄弟，是因为后者会在每次 hover 时收窄可读正文列、让整页文字重排；淡出层必须"看得见但点不着"，否则用户在一个淡出周期内点下去的意图会被一层已经不属于它的透明控件吃掉。整页替换的撤销粒度不是新设计，而是那句状态文案已经许下的承诺——做不到就该改文案，这里选了改实现。
- 未决：`schedule_save` 与动作消息共用一个槽，是壳层设计缺陷而非本竖切引入；本轮只把自己新增的两条消息挪到浮条，其余 10 处（粘贴/移动/删除/清单改名改色删除/任务完成移动/彻底删除）仍会在写入的同一帧被"正在保存…/未保存"覆盖。要修得先定规则：要么这些消息一律走浮条（浮条需要通用化，现在它的样式与 `organizer-sent-toast` 这个 id 都还是"已发送"专用），要么状态行分两个槽（消息槽 + 保存态槽）。两种都超出"补界面功能与细节"的范围，等用户排期。
- 验收：新增 14 道门（隔离库 `gpui::test`，`cargo test --release --bin quire-gpui` 现有 93 项全绿，`--lib` 61 项全绿；全量 debug `cargo test --locked -j 8` 156 项 = 61 库 + 93 二进制 + 2 集成，全部 `test result: ok`）。`the_block_rail_follows_the_pointer_without_moving_the_text`：投真实 OS 级 mouse move，要求 `document_hover == Some(该行)`、rail 两项都在文字左缘之外且 `+` 不与 `⋮⋮` 重叠、同行 `origin.x/width` 悬停前后相等、未悬停行没有导轨；跨行后旧行的淡出层在 `MOTION_HOVER_SETTLE` 之后必须释放（这一步要显式 `advance_clock`：`run_until_parked` 从不推进假时钟，计时器会永远悬着——这是本轮踩到的夹具坑，写进夹具说明）。`a_leaving_rows_rail_fades_without_becoming_a_click_target`：淡出层必须仍在布局里（否则无可淡出之物）、仍在 gutter 内、与当前行的活动导轨并存，点它两下不得加块、不得开斜杠菜单或块菜单。`an_older_fade_cannot_cut_a_newer_one_short`：淡出跑到一半把指针挪回上一行，再走 60ms，要求旧的定时器不许释放新的那一层（`Some(second)`），到自己截止点才松手。`the_rail_insert_button_adds_one_block_and_opens_the_slash_menu`、`the_rail_handle_button_opens_the_block_menu_for_its_row`：真实点击量出来的行为。`the_replace_row_joins_the_layout_only_when_it_is_asked_for`：条高与正文行 `y` 的四段对照（闭合→展开查询行→展开替换行→收起→关条）。`replace_all_writes_the_page_as_one_undo_step`：3 处/2 块、一次撤销整页还原、浮条文案与 4 秒后自动收、同时 `status` 已回到"已保存"。`replace_current_writes_one_match_and_gives_it_back_on_undo`、`a_locked_page_answers_a_replace_attempt_out_loud`、`the_breadcrumb_walks_the_page_chain_and_back`、`page_breadcrumb_lists_ancestors_root_first_and_survives_a_broken_chain`（含环与悬空父页）、`apply_replacements_splices_every_match_highest_first`（多字节正反两向 + 一条"按低偏移先 splice 结果必然不同"的 control 腿）、`the_rail_fade_eases_rather_than_switches`（含线性曲线 control 腿）。八条源码变异逐条红在预期用例名上，且每条都先跑过逐字节相同的控制副本：去掉 `document_rail_exit_seq` 守卫→红在"the older timer may not release a fade that has nine frames left"；把合并 `Entry` 的 `revert` 换成空表→红在"a single undo has to give the whole page back"；替换行无条件渲染→红在"the replace row stays out of the layout until the user asks for it"；淡出层不挂载→红在"a fading rail must stay mounted"；面包屑把当前页也列进去→同时红在纯函数门与视觉门的"根页无轨道"；轨道条无条件占位→红在"根页无轨道"；插入按钮不再开斜杠菜单→红在"opens the slash menu on the block it just made"；把手点击传 `BlockId(0)`→红在"the handle opens the actions menu on its own row"。变异全部用 `cp` 还原并比对 sha256（还原当场：`src/view.rs` `a0f2288d…`、`src/session.rs` `4ee2ad9b…` 与变异前一致；此后又跑了一次 `cargo fmt --all`，它只改了 `src/view.rs`，所以现场重算的 view.rs 哈希已是 `5749de98…`，`4ee2ad9b…` 仍是 session.rs 的当前值）；fmt 之后 `cargo test --release --lib`/`--bin` 与全量 debug 各重跑一遍仍为 61/93/1/1 全绿，release 二进制重编后 `FileVersion` 仍读回 `0.1.18.0`（26,573,824 B、16:56:32）。0.1.18 运行时复测（release、1600×900、warmup 2s、8s@250ms、隔离库）：空壳两臂 WS 末值 119.5/119.2MB、峰值 146.9/146.6MB、Private 末值 90.3/89.9MB、静止 CPU 1.73%/2.30%，对 0.1.17 基线（120.4/106.8、150.2/136.6、91.2/83.3、2.5%/1.34%）无退化；10k 块新种库首臂读到 178.6MB/13.62% 高于基线（164.2/207.1、7.61%），补测一臂全新种库 160.1MB/207.0MB 峰值/139.7MB Private/2.88%、另两臂复用同一种子库 164.5/163.8MB、峰值 207.4/206.7MB、CPU 1.92%/1.54%，即 5 臂里 4 臂落回 0.1.17 基线带内，178MB/13.6% 那次未复现，判为刚写完 10k 块与四份备份后的落盘尾巴，不作为退化结论；hover 路径在静止场景不参与（无鼠标移动即无 `notify`）。真人鼠标在正文左缘扫动、按导轨插入与拖动、点面包屑回父页的实机验收待用户在场时补；本轮未读写真实用户库、未安装、未发布、未提交。

## 文档输入实体从整页常驻改为按视口封顶（2026-11-06）

- 旧行为：`sync_inputs` 与滚动处理器为打开页的**每一块**建并保留一个 `TextInput` 实体（`inputs: BTreeMap` + `input_subscriptions: HashMap`）。原注释解释这是故意的：视口外的块若在滚动回来时于 render（paint）回合重建，`cx.subscribe_in` 在渲染期注册永不触发，键入与回车会静默丢失——所以宁可整页常驻。0.1.14 已释放这些离屏实体的排版缓存，但实体本身、FocusHandle 与订阅槽仍全数常驻。
- 新行为：`inputs`/`input_subscriptions` 封顶到 `document_visible_range ± 120`（新增 `viewport_input_ids`）∪ 活动块 ∪ IME 组合中的块 ∪ 视口内表格单元格；滚动处理器同样按该窗口 retain，滚出即释放实体与订阅。离屏行需要渲染时由 paint 的 `ensure_block_input` 重建实体（渲染不需要订阅）；`focus`/`focus_selection` 在事件回合调用新增的 `ensure_input_subscribed`，给即将聚焦的块补齐实体 + 订阅——事件/异步回合注册的订阅才生效，这正是原来绕不开"整页常驻"的点，现改为"聚焦即补"。`±120` 是端选择：连续滚动的新行落在窗口内、且整页短于一屏加余量时行为与旧版逐块等价。随之删除唯一调用点已迁移的 `document_visible_block_ids`。
- 理由：闲置内存的归因（见 `docs/08-VALIDATION.md`「闲置内存现状」与 `benchmarks/results/idle-memory-2026-11-06/` 的探针文本）显示空白页 83MB 是渲染地板、整库约 30MB 属 quire-core 外部依赖，本层唯一能动的是这 1.08MB 文本被 10002 个实体放大成的约 18MB。实体数由视口而非页大小决定后，10k 块页 Private 末值从 130–160MB 降到 109–115MB（与"只建 80 实体"的 cap 理论下限 111–115MB 吻合），峰值从 153–169 降到 133MB，空白页 83.0→83.1/83.4 持平（小页零回归）。
- 未验证：真人滚轮连续滚动后在"刚滚入的行"上立即键入、以及组合期间滚动，未做真机验收；新门只覆盖事件路径。`±120` 未被规格钉死。版本仍 0.1.18（未自动 bump）。
- 验收：新增 `focusing_a_far_block_recreates_its_input_and_still_takes_text`——播种 200 块，断言第 199 块改前不在常驻窗口、常驻数远小于 200，`focus` 后经真实编辑事件写回模型为 `"typed"`；移除 `ensure_input_subscribed` 的变异体令该门红在 `"row 199"` ≠ `"typed"`。全量 debug `cargo test --locked -j 8` 157 项（61 库 + 94 二进制 + 2 集成）全绿；`cargo fmt --all` 后重编，release `FileVersion 0.1.18.0`、`view.rs` sha256 `262901feb4f021e6…`、exe 26,577,408B。未读写真实用户库、未安装、未发布、未提交。

## 侧栏搜索行从顶部移到最近组之后（2026-11-06）

- 旧行为：搜索行常驻侧栏顶部，夹在"Q 工作区"切换器与"笔记/任务"之间（`sidebar-panel` 的直接子级），带 `Ctrl P` 徽标。这个位置是 0.1.14「侧栏 Notion 式重排」落下来的，`docs/DELIVERY-STATUS.md` 0.1.14 条把它记成用户要求；本轮用户看图后明确这不是他要的位置。
- 新行为：搜索行移进 `page-list-scroll`，排在收藏/最近组之后、"工作区"分组行（`workspace-root-drop-target`）之前，行高 28、上间距 `mt(6)`，随页面树一起滚动，不再常驻顶部。点击仍走 `open_search(&SearchPages)`（与 Ctrl+P 同一个居中浮层），右键仍是固定菜单 index 3，行为一字未改，改的只是它在布局树里的位置。为让门能读坐标，给 `sidebar-organizer-notes`、`sidebar-organizer-tasks` 补了 `debug_selector`（`sidebar-search` 原本只有 `id`）。
- 理由：用户在本轮给出的三个去处里选了"侧栏保留但往下放"，顶部留给切换器与笔记/任务。代价写在脸上：搜索行进入滚动区后，页面树很长时它会被推出视野，鼠标入口不再常驻；Ctrl+P 与浮层本身不受影响。
- 验收：新增门 `the_search_row_lays_out_under_the_recents_group`——三页播种让最近组非空，断言 `笔记 < 任务 < 最近 < 搜索 < 工作区分组` 这五个真实 `origin.y` 的次序，再按布局上报的坐标点一次搜索行、断言 `search_open` 为真（证明搬进滚动区没把命中区搬丢）。变异自证：把该行整块搬回侧栏顶部（旧位置）后，该门红在预期断言上，报出 `recents 160, search 76, header 190`；还原后 `src/view.rs` sha256 与变异前一致（`2f723c7ea15f…`），门回绿。全量 debug `cargo test --locked -j 8` 158 项（61 库 + 95 二进制 + 2 集成）全绿，`cargo fmt --all -- --check`、`cargo check --all-targets` 通过。实拍：release 构建在隔离 `%TEMP%` 库、1280×800 真窗口 PrintWindow 截图，侧栏自上而下为 工作区 / 笔记 / 任务 / 搜索(Ctrl P) / 工作区分组 / 页面 / ＋新建页面，底部设置——该夹具库没有最近条目，所以搜索行紧贴任务下方 6px；真实库有最近组时它落在最近之后。
- 未验证：真实库里最近组展开、页面树很长时搜索行被滚出视野后鼠标是否还够得着（未做真机滚动验收）。已随 0.1.19 出包、装机并在用户真实库上实拍确认（`docs/DELIVERY-STATUS.md` 0.1.19 条）。
- 测量诚实性另记一条：本轮第一次全量 `cargo test --locked -j 8` 报 1 项失败，但当时的过滤管道把失败名洗掉了、没抓到；随后 5 次运行（含 bin 套件连续三次）全绿，无法复现。首要嫌疑是同步端口占用（测试日志里的 `os error 10048`）撞上上一轮运行的端口还没释放，未定位，不当作已解决。

## 两版共用一套数据后端与一次性导入（2026-11-06）

- 旧行为：GPUI 默认库在 `%LOCALAPPDATA%\QuireGPUI\quire.db`，与旧版的 `%APPDATA%\Quire\quire.db` 各一套；`AGENTS.md` 把「数据目录独立、绝不自动发现或迁移旧版真实库」写成硬规则，`--db` 只允许指向测试副本，两版对比靠手动导副本。用户 2026-11-06 改判：以后 slint 和 gpui 共用一套数据后端，原 slint 数据可以备份。
- 新行为（五件事，缺一不可成一套）：
  1. **默认库就是旧版那颗**。`shared_library_path`（`src/main.rs:30`）取 `%APPDATA%\Quire\quire.db`，`--db` 仍然优先（`src/main.rs:139`）。门 `the_default_library_is_the_one_the_legacy_desktop_already_uses`（`src/main.rs:555`）钉死这个字符串、断言路径里不含 `QuireGPUI`、且 `APPDATA` 缺失时回落到临时目录而不是当前目录。安装标识照旧分开（独立 AppUserModelID、独立安装目录、独立 exe），卸载只撤程序。
  2. **一次性合并命令 `--import-library PATH`**（解析 `src/main.rs:54`，互斥与前置校验 `src/main.rs:126-138`，报告 `src/main.rs:187-201`，实现 `src/session.rs:4402`）。它开真 Session、把源库整库并进目标库、打印 JSON（source/target 路径与 pages/blocks/notes/tasks/attachments 各自的 before/after，加 `imported_pages` 标题清单）后退出，不开窗口、不进事件循环。拒绝三种输入：源不存在、源就是本次要写的目标、源库此刻还有另一个连接挂着。
  3. **并发写门 `ensure_no_other_quire_holds_it`（`src/session.rs:119`）**。Windows 上以 `share_mode(0)` 独占试开 `<db>-shm`：SQLite 只在有连接 attach 到 WAL 期间持有那颗文件，所以这一步不需要旧版配合就能问出「对面是不是正在写」。`-shm` 无人持有（上次非正常退出的残留）时独占打开成功、库照常打开——这条检查不可能把自己锁死。非 Windows 直接返回 Ok。**它只覆盖 GPUI 一侧**：旧版代码未改，"GPUI 常驻时再开旧版"仍靠人工，`docs/RELEASE-AND-ROLLBACK.md` 与本文件都写明这一点。
  4. **合并分配器按两侧水位播种**（`src/session.rs:4016`，`let floor = |local, remote| local.max(remote)`）。merge 对被撞 id 的行是「renumber 到分配器的下一个值」，而另一批对端行原样保留自己的整数；分配器若只看本机水位，就会把一条被撞的行塞进另一条对端行正占着的 id，`SqliteRepository::apply` 撞 UNIQUE 并整批回滚。变异 M1（floor 只看 local）红在 `import_library_adds_the_other_librarys_pages_and_keeps_its_own`，报的正是 UNIQUE 约束。
  5. **内置模板按预设逐个播种**（`src/session.rs:897`，判据在 `924`）。旧版用它自己的设置键 `builtin-templates-seeded` 在同一颗库里已经播过 5 条（id 114-118），GPUI 原来只看 `gpui.templates-seeded-v1`，第一次打开共用库就会在用户页面旁边再摆一份重复模板。现在按 `page.template && page.title == preset.name` 逐条问，缺哪条补哪条。
- 语义（为什么合并只增不删）：`import_library` 走的是 LAN 同步那条现成引擎——`from.sync_export()` → `sync_apply_remote(..., shadow = None)`。core `merge.rs:365` 那条 `None if l.is_some() => Pick::Local` 保证影子缺席时「对端没有这一行」永远不读成删除，所以一次好心导入不会抹掉本机数据。模板页不走这条线：`sync_export` 按 `!page.template` 过滤（`src/session.rs:3908`、`3926`），导入的只有正文页。
- 依赖：两版的 `quire-core` 一并钉到 `1d6a357`（GPUI 原来在 `b73af89`，旧版已在 `1d6a357`）。这一版带来的正是共用后端要用到的三条：无影子时不把缺失读成删除、`BlockInserted` 批量 apply、`search_index::upsert_block` 跟上 `BlockMovedToPage`。两端 schema 同为 `CURRENT_VERSION = 30`、`migrations.rs` 逐字节一致。
- 理由：把导入写在同步引擎之上，而不是另写一套"复制行"的代码，是为了让 `block_children`、FTS 行、organizer 的 uuid 和附件指针由同一份已经过双向往返验证的实现负责；第二套实现只会带来第二种错法。默认路径改成旧版那颗而不是加一个"共享模式"开关，是因为用户要的是"以后都共用"，一个需要每次记得传的开关等于没有。
- 已知代价：两版现在共用同一目录里的 `mcp.json`（它描述的是当前活着的那个进程的端口），以及 `<db>.gpui.lock` 会落在旧版数据目录里；旧版原有的 `quire.db.bak1..5` 是合并前快照，而 GPUI 的损坏恢复按同名滚动备份，必须先把旧版那五份改名（`.premerge-20261106`，已执行），否则一次回滚就把这次合并抹掉。
- 两处本轮自纠，都按「先怀疑自己的验证脚本」处理：① 导入门第一次红（left 4 / right 3）不是实现错，是我的夹具算错——两侧库都带一颗 freshly seeded 的「未命名」页，merge 会把两边一致的行收敛，所以"页数增量"不是不变量；改成逐个标题到达 + 报告自身算术一致。② 合并后出现的重复模板页我一开始误判成导入缺陷，写了一个"导入跳过模板"过滤器，它的新门当场把真因照出来：`sync_export` 本来就不导出模板，重复来自 GPUI 的模板播种，而 `--dump-state` 绕过 Session 所以看不见播种。过滤器已完整回退，真修复落在 `seed_builtin_templates`。
- 未验证 / 未落地：旧版侧的对称并发写门（5 行，改 `quire-desktop`）等用户点头；`--import-library` 的断点续跑与冲突清单未做；真实共用库上的一轮已在 2026-11-06 夜间跑完（见下面「真实库切换实跑」），但合并后的长期往返（用户在两版之间来回写几天）还没开始。
- 验收：本轮新增/改写 6 道门（导入只增不删、organizer 两边都在、自导入与缺失源拒绝、`-shm` 探针、默认库路径、模板按预设播种），源码变异 M1-M6 逐条先跑逐字节相同的控制副本再改一处，红在预期用例名上（M1 UNIQUE、M2 organizer 计数、M3 自导入、M4 探针恒过、M5 默认路径、M6 模板重复）。副本上的端到端实跑：旧版库 ← GPUI 库，页数 8→11、块 289→295、笔记 111→113、任务 15→17、模板行 114-118 未动；合并结果由本轮新构建的 `quire-slint.exe --db <副本> --dump-state` 读出 `pages=16`，即旧版读得懂 GPUI 写进去的东西。全量测试数字见 `docs/DELIVERY-STATUS.md` 0.1.20 条。
- 取证过程有一条副作用必须交代：为核对旧版库的行数，我用了 platform-tools 的 `sqlite3` 以只读方式查询**真实** `%APPDATA%\Quire\quire.db`。SQLite 的只读连接仍会把 WAL checkpoint 进主库并删掉 `-wal`/`-shm`（主库 3,325,952 B，`integrity_check` ok，289 块、145 条笔记行）。没有丢数据，但「只读查询不动 sidecar」这个默认预期是错的，以后对真实库做计数要先想到这一条。同样地，本轮的两份日期备份（`%LOCALAPPDATA%\QuireBackups\pre-share-2026-11-06\{slint,gpui}`）是在两版还开着的时候复制的，不满足 `docs/RELEASE-AND-ROLLBACK.md` 第 14 行的静默要求；真实切换用的是重做的静默份 `pre-share-2026-11-06-quiet`，那两份早期副本只作为本轮实验的起点留档。
- 真实库切换实跑（2026-11-06 23:20-23:31，装机版 0.1.20）：前置是 `tasklist` 里没有任何 Quire 进程，静默备份 `pre-share-2026-11-06-quiet\{slint,gpui}` 与被复制的真实库逐字节相同（旧版主库 3,325,952 B `6a361d1769e139a9…`；GPUI 主库 `da63a52bdf62b3fc…` + `-wal` 1,458,512 B `eecbbcc57b7605c7…`），并核过备份里那颗 `-wal` 没有未落盘的帧——同一副本「db+wal」与「只有 db」两种读法都是 13 页 / 289 块 / 145 笔记行。合并前读数：GPUI Session 口径 8 正文页 + 5 条模板行（id 114-118，旧版用它自己的 `builtin-templates-seeded` 早已播好）+ 289 块 + 111 条在用笔记，旧版 `--dump-state` 同一颗库报 `pages=13`（含模板的原始行数，两个口径都对得上）；GPUI 试验库 8 页 / 54 块 / 2 笔记。装机：`Quire-GPUI-0.1.20-windows-x64-setup.exe` 7,860,330 B `39e8de79f0b109ad…` 静默装完，`%LOCALAPPDATA%\Programs\Quire GPUI\quire-gpui.exe` 26,581,504 B、`FileVersion 0.1.20.0`、sha256 `639E97A2BAC09AFA…` 与 `target/release` 那颗逐字节相同，HKCU 卸载项 `version=0.1.20`。合并：`quire-gpui.exe --import-library %LOCALAPPDATA%\QuireGPUI\quire.db`（不带 `--db`，目标即默认共用库）exit 0，JSON 报 pages 8→11、blocks 289→295、notes 111→113、tasks 15→17、attachments 0→0，`imported_pages` 是三条「未命名」，在共用库落成 id 1、7、8。合并后：`pragma integrity_check` = ok，pages 表 16 行 = 11 正文 + 5 模板，模板行 id 与合并前完全相同（真实库上确认了「播种按预设逐个问」没有摆出第二份模板），本次写进共用库设置表的唯一非合并键是 `gpui.templates-seeded-v1=true`；旧版 `--dump-state` 同一颗库改报 `pages=16`，与合并前那句是同一条打印，+3 正好是导入的三页。并发写门实拍：旧版实例（pid 113032）持着 `-shm` 时启动装机版 GPUI，GPUI 不开主窗口，弹模态框，标题「Quire GPUI — 无法启动」，正文原句「旧版 Quire 桌面端正打开着这个库：%APPDATA%\Quire\quire.db（另一个程序正在使用此文件，进程无法访问。 (os error 32)）两个版本不能同时写一套数据，先退出另一个再打开。」（框里那条绝对路径按本仓库惯例写成 `%APPDATA%` 形式，不落本机用户名）。旧版接 `--quit` 优雅退出后 GPUI 正常打开。
- 终态与回退点：GPUI 0.1.20 常驻共用库（窗口标题 `quire-gpui-0.1.20`，真实 3.3MB 库上 30 秒空闲 WS 122MB / Private 99MB / 387 句柄 / 41 线程——这是用户实际数据上的第一个数，与隔离种子库的基线不同口径）；旧版的滚动备份 `bak1/bak2/bak3` 已由合并后的库重新生成，合并前五份快照改名为 `quire.db.bakN.premerge-20261106` 原地留着；`%LOCALAPPDATA%\QuireGPUI\` 整目录未删也未动，它就是 GPUI 侧的合并前快照。回退按 `docs/RELEASE-AND-ROLLBACK.md`：退出 GPUI → 用静默备份整目录还原 → 开旧版。

## 跨端整理区断言改用唯一 ID（2026-11-06）

- 旧行为：`tests/sync_loopback.rs` 的双节点回环门在 A、B 各自建笔记/任务，交换快照后用**发起方的整型 id**去断言接收方那一侧的行存在（`b.organizer.note(note_a)`、`a.organizer.task(task_b)`）。它一直是绿的，绿的原因是运气：接收方那条 collections 的水位是 0，`merge_organizer` 给来行的"本机新 id"恰好落在 1，与发起方的 id 相同。
- 新行为：跨端断言先按 uuid 把行找出来、再用**该设备自己的** id 说话（新增 `note_id_by_uuid` / `task_id_by_uuid`，`tests/sync_loopback.rs:14`、`:24`），并加一条"一个 uuid 在这台设备上只有一行"的计数断言与一条 uuid 回读。B 侧后续的编辑与重开读回全部改用 `note_on_b`。
- 理由：这不是迁就实现，是回读规格的结果。`quire-desktop/docs/SPEC.md` §四十一写得明白：organizer 的 notes/tasks 按 **唯一 ID** 合并，「插入时给一个本机的**新** id」，而 core `Note::uuid` 的文档注释直接说 `id` 是「a per-device watermark a merge may renumber... not a name a device can hand to another one」。也就是说旧断言测的是规格禁止的东西。引入分配器 floor（上一条竖切）之后，来行 id 不再是发起方 id，这条一直潜伏的错法第一次照出来——红的是门，不是产品。
- 验收：`cargo test --test sync_loopback` 绿；控制副本（逐字节相同）绿；变异 M7（`sync_organizer_changes` 不再为到达的笔记发 `NoteAdded`）红在 `tests\sync_loopback.rs:323`「A's note did not arrive on B」，变异 M8（同样对任务不发 `TaskAdded`）红在 `:379`「B's task did not arrive on A」。两条都先跑过控制，且 M7 的红证明新断言比原来那句 `assert!(...is_some())` 更会说话：旧的标题级断言（`merged_b.notes` 里有没有这条笔记）在两个变异下仍然是绿的，因为它读的是 merge 的**输出**而不是这台设备**落了**什么。

## 侧栏折叠门的点击断言补上几何取证（2026-11-06）

- 现象（一条仍未定位的红，按失败记录留档）：`view::view_tests::collapsing_a_sidebar_group_takes_its_rows_out_of_the_layout` 在全量 `cargo test --locked -j 8` 里红过一次，报 `the click landing on the reported row bounds must open that page`，`left: PageId(8) / right: PageId(7)`——点收藏行打开的是下面那行。同一条红在 0.1.18 那轮也出现过一次（本文件上一条竖切的"测量诚实性"记的正是它），两次都不复现。
- 已排除的两个嫌疑：① 单独串行连跑 3 次、bin 套件并行连跑 5 次、release 串行全量 1 次，共 9 次全绿；② 按上一条的猜测把 TCP+UDP 5878 用另一个进程占住（测试日志里那句 `os error 10048` 就是它），再连跑 6 次仍全绿——同步端口占用与该红无关，上一条竖切写下的这条嫌疑**被本轮实测否掉**。
- 本轮改动：给该门补两处取证，不改它的任何断言方向。① 点击前重新读一次该行的 bounds，若与首次读到的 `origin.x/origin.y/height` 不一致，就报"树在两次快照之间动了"而不是把责任推给命中区；② 最终那条 `assert_eq!` 的失败信息现在把 `favorite-nav-*` / `recent-nav-*` / 两个分组头 / `workspace-root-drop-target` 各自的 `(y, x, height)` 全部打出来，并打出实际点击点。下一次再红，报的就是机制而不是症状。
- 变异自证：C1 控制副本（逐字节相同）绿；M9 把该行的 `on_click` 改成打开 `PageId(0)` 后，门红在预期的最终断言上，新信息当场打出 `favorite-nav-7 Some((156.0, 8.0, 28.0))`、`sidebar-recents-toggle Some((184.0, 8.0, 24.0))`、点击点 `(28.0, 170.0)`——即 170 确实落在 156..184 那一条里，失败信息与行位置对得上。
- 未证的一条，明说：新增的"bounds 前后一致"那句是夹具完整性检查，不是行为门——本轮构造不出能让它单独红的单点源码变异（两次读之间没有任何事件），所以它只在未来真的出现"树动了"时提供分辨力。该门的原始红仍未定位，不当作已解决。
