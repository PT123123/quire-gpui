# ADR-001：GPUI 依赖选型与 Windows 可行性

日期：2026-10-14（同日修订两轮：加入本机发布版实测，并撤回一条错误的选型证据）。状态：**建议待定稿**——发布版路线的编译与起窗已实测通过，输入行为仍需人手打字验证。

核验用的上游锚点：`zed-industries/zed` main @ `36682547bea1c3d4fa60c08d341cf60081853941`（2026-10-13），最新 release tag `v1.23.3`（2026-10-12）；发布版锚点：crates.io `gpui 0.2.2`（2025-10-22，源码已在 `~/.cargo/registry/src/*/gpui-0.2.2` 逐文件核对），`gpui-pre 0.3.9`/`gpui-fast 0.1.7`/`gpui-kit 0.7.3`（2026-10-12）。crates.io 查询时间 2026-10-14。

## 一、会改变路线的四个事实

> 2026-10-14 更正：本节原第 1 条"crates.io 上的 `gpui` 0.2.2 装不起来，只能走 git 依赖，`cargo add gpui` 这条路是死的"**是错的，已撤回**。我当时查的是 `gpui_platform`——那是 0.2.2 之后才从仓库里拆出的 crate，它不存在不能反证已发布的 0.2.2 缺依赖。下面是本机重跑的证据（命令见第六节实测记录）。

1. **发布版 `gpui 0.2.2` 在本机能装、能编、能弹窗。** 依赖图解析 708 包、编译 439 个 crate、dev 构建 3m46s、产物 `hello_world.exe` 起来后进程存活、窗口可见（class `Zed::Window`，516×539）。它的 9 个兄弟 crate 都以 `gpui_*` 别名发布在同版本：`gpui_collections`、`gpui_sum_tree`、`gpui_util`、`gpui_refineable`、`gpui_semantic_version`、`gpui_media`、`gpui_http_client`、`gpui_util_macros`、`gpui-macros`（逐个查过，全部 latest=0.2.2）；`gpui_platform`、`gpui_windows` 查询结果是 crate does not exist——它们属于仓库后续的拆分，与发布版无关。**所以真正的取舍不是"git vs crates.io"，而是"一年前的官方发布版 vs 当前 tag 的 git 依赖 vs 第三方周更快照"。**
2. **发布版与当前 main 是两套入口，且 0.2.2 的示例需要 edition 2024。** 0.2.2：`Application::new().run(|cx: &mut App| { cx.open_window(WindowOptions { .. }, |_, cx| cx.new(|_| X)).unwrap(); cx.activate(true); })`；main @3668254：`gpui_platform::application().run(...)`。把 0.2.2 自带的 `examples/input.rs` 放进 edition 2021 的工程会直接报 `let chains are only allowed in Rust 2024 or later`（`src/bin/input.rs:553`），改 edition 2024 后编译通过并运行出窗口。抄示例前先核对它对应哪个版本，这条对两个方向都成立。
3. **gpui-kit 是 crates.io 上的发布版组件栈，不是需要绑 git fork 的另一套框架。** `gpui-kit`/`gpui-component`/`gpui-base` 0.7.3、`gpui-pre` 0.3.9、`gpui-fast` 0.1.7 全部已发布，2026-10-12 同日发版，且按周推进（gpui-pre 0.3.7@09-28、0.3.8@10-05、0.3.9@10-12）。`gpui-pre` 的描述自述为 "gpui snapshot of zed@93d7456"。要点：**它的快照版本比官方发布版新**（0.3.9 vs 0.2.2），"选 Kit 就与上游脱钩"这个说法在发布节奏上站不住——反倒是官方 crates.io 通道已经 12 个月没发新版。仍然成立的约束只有一个：`gpui`、`gpui-pre`、`gpui-fast` 是三份不同的 crate，类型身份互不相通，**一个依赖图里只能选一套**，不能混装。
4. **gpui 没有托盘图标能力。** 已发布 crate `gpui 0.2.2` 的 `src/` 全量搜索 tray 零命中，当前 main 的 `crates/gpui*` 也是零命中——两条路线都要自建。旧版 Quire 的托盘在 `reference/slint-desktop/src/platform/tray.rs` 里是自建 Win32 实现，这部分必须照搬而不是指望框架。原生通知和原生菜单（`set_menus.rs` 示例）存在。

## 二、Windows 中文输入是本项目最大的单点风险

gpui 的 Windows IME 预编辑路径存在，且发布版里就有：`gpui 0.2.2` 的 `src/platform/windows/events.rs:102-103` 接 `WM_IME_STARTCOMPOSITION`/`WM_IME_COMPOSITION`，`657-690` 解析组合串与组合光标位置（`parse_ime_composition_string`、`retrieve_composition_cursor_position`，`1431-1454`），依赖 `windows` crate 的 `Win32_UI_Input_Ime` feature。当前 main 的同一路径在 `crates/gpui_windows/src/events.rs:752-767`。但上游有一串**未关闭**的中文输入缺陷：

| 议题 | 内容 | 状态 |
| --- | --- | --- |
| #59882 | 中文 IME 间歇性失效（编辑器/agent/commit） | open, S2 |
| #59636 | TSF `GetTextExt` 只返回 caret rect → 候选窗定位不准 | open, S2 |
| #59193 | 中文 IME 下文本纵向跳动 | open, S2 |
| #64850 | 候选窗遮挡文本 | open, S3 |
| #65453 | 无预编辑下划线 | open |
| #64389 | `TextInput` 只存在于 `crates/gpui/examples/input.rs` 示例里，各应用各自复制；`replace_and_mark_text_in_range` 会放错 IME 组合选区 | open。发布版 0.2.2 同样如此：`src/` 无 `TextInput`，只有 `examples/input.rs:33` 的示例实现（已核）。Kit 是否自带等价输入组件**未核**，选 Kit 前要先查 `gpui-component 0.7.3` |

窗口管理另有 #58521（拖动/缩放严重 bug，open）、#61638（退出时最大化位置错乱）、#60140（全屏挡住任务栏）。

含义：旧版 Slint 客户端的中文输入是**当前可用**的基线。GPUI 若复现上述任一问题，P1 闸门直接不过，而改用 GPUI 的核心理由恰恰是编辑体验。但闸门按严重度分档，不设成"七条探针全绿才开始写代码"的前置循环（见 docs/16 与 AGENTS.md 第一竖切）：探针 1 与"能在输入窗里打中文"先跑，富文本折行、万块列表、完整托盘随后续竖切验证。丢字/重复上屏/改错范围/无法选词/输入崩溃=严重失败，阻塞；候选框持续脱离 caret 或遮挡操作也要修；单条上游 issue 存在或一次轻微视觉差异不自动否决整条路线。

## 三、渐变与几何

- 有原生 API：`linear_gradient(angle: f32, from: impl Into<LinearColorStop>, to: impl Into<LinearColorStop>) -> Background`，支持 `.color_space(Oklab|Srgb)`。发布版里就有：`gpui 0.2.2` 的 `src/color.rs:765`（`ColorSpace::Oklab` 在 `:674`），示例 `examples/gradient.rs`。
- 但 HLSL 着色器的插值参数取自**该绘制四边形自身的包围盒**，不是窗口几何：0.2.2 的 `src/platform/windows/shaders.hlsl:331 gradient_color(..., Bounds bounds, ...)` 之后按 `bounds.size` 展开短边算方向投影（main 的 `crates/gpui_windows/src/shaders.hlsl` 同构）。所以"整窗一条连续 ramp、不随文档滚动"必须靠把根元素铺满窗口来实现，滚动容器内部不能再各画一条。这与 docs/04 的要求一致，属于可实现但需实测确认。
- **框架自带的 element 比原先写的多**（这是对本节旧结论的更正）。0.2.2 的 `src/elements/` 直接导出：`list.rs`（`List`/`ListState`/`ListScrollEvent`/`ListMeasuringBehavior`，即可变高虚拟化列表）、`uniform_list.rs`（`uniform_list()`/`UniformListScrollHandle`）、`anchored.rs`（`anchored()`/`AnchoredState`，锚定弹层）、`canvas.rs`、`animation.rs`、`img`/`svg`/`text`；`examples/` 另带 `data_table.rs`、`uniform_list.rs`、`input.rs`、`scrollable.rs`。缺的是 `TextInput`（见第二节 #64389）和托盘，不是"列表和弹层全部自研"。
- 每显示器 DPI：`resources/windows/gpui.manifest.xml` 声明 `dpiAwareness = PerMonitorV2`，且 default features 含 `windows-manifest`，`build.rs::embed_resource()` 会把 `gpui.rc` + manifest 用 `embed_resource::compile(...).manifest_required()` 打进二进制——不是只有仓库里的资源文件。与旧版要求同档。
- **release 构建已在本机验证通过**：`cargo build --release -j 8 --target-dir target-release` 用时 6m13s，dev 与 release 都能构建。Windows SDK 的 `fxc.exe` 位于默认 Kits 路径，版本 `10.0.26100.7705`；release 产物目录包含 GPUI 生成的 HLSL 字节码头文件和嵌入的 Windows manifest/资源。还未启动 release exe 做运行时验收。
- 无边框/自绘标题栏：0.2.2 的 Windows 侧有 `titlebar.appears_transparent`（`window.rs:380`，默认 true → `hide_title_bar`）并在 `events.rs:705/767/884` 参与非客户端区计算与命中测试；`zoom()`/`is_maximized()`/`minimize()`/`toggle_fullscreen()`/`set_window_placement()` 都在（`window.rs:302-310, 523, 778-800`）。注意 `window.rs:270-274` 那段剥 `WS_CAPTION` 是**全屏路径**，不是通用无边框路径，别照抄。Snap 是否走原生 Win32 仍属推断，探针 6 要实测。

## 四、工具链成本（本机实测，2026-10-14）

旧版这张表把 Zed **完整仓库**的要求当成独立 gpui 应用的要求，结论过宽。独立最小应用（`gpui="=0.2.2"`，dev profile）在本机的实测：

| 项 | 独立 gpui 应用实测 | 判定 |
| --- | --- | --- |
| Rust | rustc 1.98.0 / cargo 1.98.0，**没有**装 1.98.1 | 够用。1.98.1 那个 pin 是 zed 仓库的 `rust-toolchain.toml`，独立工程不受它约束 |
| Edition | 必须 **2024**：0.2.2 自带示例用了 let-chains，2021 下编译失败 | 新工程直接 `edition = "2024"` |
| CMake / wasmtime | 本次 439 个 crate 的构建日志里零命中 | 与独立 gpui 应用无关，别预先安装 |
| Spectre-mitigated libs | `VC/Tools/MSVC/*/Lib/Spectre` 不存在（find 结果 0 个目录），构建照样通过 | **不是门**，撤回"需在 VS Installer 勾选" |
| Windows SDK / MSVC | 10.0.26100 + MSVC 14.44.35207 | 满足 |
| fxc.exe | dev 不需要；release 已用默认路径 `C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\fxc.exe` 编译 HLSL | 已通过，版本 10.0.26100.7705 |
| 依赖体量 | `Cargo.lock` **709** 个包（对比旧版 Slint 工程 619） | 首次 `cargo build` 3m46s，后续增量 9s |

## 五、决策选项

三条，都可在本机开跑，前置研究不必再做：

- **A1. 官方发布版 `gpui =0.2.2`（crates.io，零 git）。** 本机已证明能装、能编、能弹出带 IME 的输入窗（第六节实测记录）。`list`/`uniform_list`/`anchored`/`canvas` 自带，PerMonitorV2 manifest 自动嵌入。代价：**停在 2025-10-22，比上游晚约 12 个月**，那之后修的 IME/窗口缺陷不在里面，`TextInput` 仍要从示例复制。
- **A2. 官方当前版（git 钉 `zed@v1.23.3`）。** 拿到最新的 `gpui_platform`/`gpui_windows` 与上游修复；代价是引入 git 依赖，且 `gpui_platform` 等兄弟 crate 未发布，锁文件必须钉 SHA。**git 依赖能否解析本轮未测**，别当已知。
- **B. Kit 栈（`gpui-kit`/`gpui-component` 0.7.3 + `gpui-pre` 0.3.9 + `gpui-fast` 0.1.7，全 crates.io，零 git）。** 组件现成，且 `gpui-pre` 按周从上游打快照，**发布节奏比官方 crates.io 通道快**——"选 Kit 就与上游脱钩"这个说法在版本新旧上是反的。代价：运行时分三种 crate，不能与官方 `gpui` 混装；Longbridge 的快照与 patch 要按其 release 的 Cargo.toml/lock 逐个核；`gpui-fast` 的渲染优化改了哪些行为是未知数。

约束只有一条，跟选哪个无关：一个依赖图里只允许一套 gpui 类型来源。无论选哪条，托盘、全局热键、DIB 剪贴板、多屏位置夹紧都从旧版 `reference/slint-desktop/src/platform/` 移植，不要指望框架（三条路线都没有托盘）。

建议：**A1 直接承担第一竖切**（零 git、已证明能运行，最快拿到"能中文输入、能保存一页"的判断依据），同时把同一组探针在 B 的隔离工程里跑第二份记录做对比；A2 只在 A1 暴露出"缺某个上游修复且本地补不了"时才升级。

用户决定（2026-10-14）：**A 与 B 都要真评估，不预先排斥 Kit**，比较口径是输入效果、维护成本、组件能力三项，且要写成可对比的结论（同一组探针在两条栈上的实测记录），不是印象。中文输入若失败，**先定位并修复，再考虑换栈**；"自绘文本"不算修复手段，因为预编辑、候选窗定位与提交/取消仍然走系统 IME。

## 六、P0 探针清单（每条都要有可复制命令与判据）

1. 最小窗口能否编译运行：候选依赖 `cargo build` 通过并弹出窗口（A1 用 `gpui="=0.2.2"`，A2 用钉 `zed@v1.23.3` 的 git 依赖，B 用 `gpui-kit`）。判据：进程存活 ≥5s，无 panic，窗口可见且有句柄。
2. 微软拼音连续输入：在示例 `input.rs` 的 `TextInput` 里输入"剥离心智模型"并翻页选词。判据：候选框落在 caret 附近（截图量坐标）、无重复上屏、无纵向跳动（对照 #59193/#59636 是否复现）。
3. 富文本折行：同一段中英混排 + 粗体 + mono + emoji + 长链接，可变字号，125%/150% DPI。判据：折行位置与基线在编辑态与静态态一致。
4. 根级渐变：`linear_gradient` 铺满窗口 + 内部滚动容器。判据：滚动时渐变不移动、页面标题与正文交界不断带（取顶/中/底三点像素）。
5. 虚拟列表：10000 个可变高条目。判据：滚动 p95 帧时间、内存 Working Set 与旧版同数据对比（旧版 0.1.26 基线：10000 块约 124.1MB Working Set）。
6. 无边框窗口 + 40px 自绘顶栏：拖动、双击最大化、Snap 到左半屏、跨屏 DPI。判据：与 #58521 描述的 bug 不复现。
7. 托盘 + Alt+N/Alt+M：移植旧版 Win32 实现进 gpui 事件循环。判据：主窗隐藏后进程仍在、热键可唤起、退出会 flush。

任何一条失败都要在本文追加"实测结果"小节：复现步骤、上游 issue 编号、尝试过的绕行、是否推翻选型。不许以"GPUI 限制"一句结案。

### 实测记录（2026-10-14，探针 1）

路线 A1（`gpui =0.2.2`，crates.io）**通过**。可复制命令（scratch 工程在 `%TEMP%/gpui-022-probe`，与正式工程无关，只是证据）：

```bash
cargo new probe --bin && cd probe
# Cargo.toml: edition="2024"，dependencies.gpui="=0.2.2", unicode-segmentation="1.10"
cp ~/.cargo/registry/src/*/gpui-0.2.2/examples/{input,hello_world}.rs src/bin/
cargo generate-lockfile            # Locking 708 packages
cargo build -j 8 --bin input --bin hello_world   # 439 crates，dev 3m46s；增量 9s
```

判据实测（PowerShell + user32，`run-probe2.ps1`）：

| exe | 存活 6s | hwnd | IsWindowVisible | 窗口尺寸 | class | Working Set |
| --- | --- | --- | --- | --- | --- | --- |
| `hello_world.exe` | 是 | 6359518 | True | 516×539 | `Zed::Window` | 96.9MB |
| `input.exe`（0.2.2 的 TextInput 示例） | 是 | 1051038 | True | 316×339 | `Zed::Window` | 89.8MB |

两个 exe 都无 panic，用 `Stop-Process -Id <pid>` 按 PID 收尾。**这台机器没装 Spectre-mitigated libs（`VC/Tools/MSVC/*/Lib/Spectre` find 结果 0 个目录），构建照样通过**——旧版第四节据此设的硬阻塞不成立。

探针 2 的**机器部分到此为止**：能起带 IME 处理路径的输入窗已证。微软拼音实际输入"剥离心智模型"、翻页选词、候选框落点、撤销后是否丢字，**必须由用户在真实桌面上手打**——本项目不允许用脚本模拟按键或截屏来代替，那测不到 TSF 行为。上面这两个 exe 就是给人手打的现成夹具。

## 七、明确未证实（不得当作事实引用）

- Windows 上 gpui 的实际拖动/缩放/IME 表现（只有 issue 列表，没有本机复现记录；起窗已证，**输入行为与 Snap 行为未证**）。
- A2 的 git 依赖能否在本机解析、`gpui_platform` 等未发布兄弟 crate 走 git path 依赖是否成立。
- B（Kit）那三个 crate 的实际快照 commit、patch 面，以及 `gpui-component` 是否自带等价 `TextInput`。
- release exe 启动、窗口行为和最终安装包运行时表现（release 编译及 HLSL 编译已过，应用尚未启动）。
- 输入到显示延迟、启动耗时。gpui 侧的内存数字目前只有"空示例 90MB 级 Working Set（dev）"，不能当作应用基线。
- App 是否强制主线程（未找到明文规定，只有 `background_executor`/`foreground_executor` API）。
- Zed 编辑器的块渲染能否复用：`crates/editor` 等未发布，跨 crate 内部路径依赖属未知领域。
- 上游"pre-1.0、经常 breaking"（`crates/gpui/README.md` @3668254 原文），因此**走 A2 必须钉 tag/SHA，禁止 `branch = "main"`**。
