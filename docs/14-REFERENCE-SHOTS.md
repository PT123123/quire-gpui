# 旧版参考图采集记录

日期：2026-10-14 02:46–02:58 采集主题矩阵与场景全扫；03:12–03:17 补采设置对话框 tab。产物：`reference/shots/` 共 **312 张 PNG，30.6MB**（274 个不同 SHA-256），清单在 `reference/shots/manifest.tsv`，312 行全部 `OK`。

本文的 `controller.rs:NNN`、`quire_shot.rs:NNN`、`ui/...`、`docs/SETTINGS_UI.md` 等行号一律指**冻结快照** `reference/slint-desktop/`（约定见 docs/10 第 3 行），快照与旧版工作树已逐个哈希核对一致。

## 采集身份（可核对）

| 项 | 值 |
| --- | --- |
| 源码 | `../quire-desktop` HEAD `47ef6f81d7f1ffb688e1349f459af7e6dc8eb09b` + 10 个未提交修改（与 `REFERENCE-MANIFEST.json` 的 dirty 列表逐行一致） |
| 构建命令 | `cargo build --features software --bin quire-shot --target-dir target-shot`（耗时 2m34s，`Finished dev profile`） |
| 二进制 | `quire-desktop/target-shot/debug/quire-shot.exe`，84,945,920 字节，mtime 2026-10-14 02:46:37，SHA-256 `4292896d226a21633a8bbc84fe8f2c6816fa7feaafa1a1e40f6613bb3733e41c` |
| 渲染器 | Slint 软件光栅（`HeadlessPlatform` + `MinimalSoftwareWindow`），**不创建窗口、不用 GPU**；图内 renderer 字段被强制写成 `Software · headless` |
| 尺寸 | 1280×800。注意 `quire_shot.rs` 用的是 `PhysicalSize::new(w, h)`，所以这是**物理像素**，与本机 DPI 缩放无关 |
| 机器 | Windows 10.0.26200 x64 |
| 采集脚本 | `tools/capture-reference-shots.ps1`（本轮新增） |

复采命令（先重建二进制，否则采到的是旧版行为）：

```powershell
cd ..\quire-desktop
cargo build --features software --bin quire-shot --target-dir target-shot
cd ..\quire-gpui
powershell -NoProfile -ExecutionPolicy Bypass -File tools\capture-reference-shots.ps1 `
  -Shot ..\quire-desktop\target-shot\debug\quire-shot.exe -Mode Both
# 只补新场景用 -Mode Matrix / -Mode Sweep / -Mode Tabs；已存在的 PNG 默认跳过，-Force 覆盖
```

`-Mode Tabs` 用 `--click` 把设置对话框点到指定 tab（坐标是 1280×800 下从网格图量出来的逻辑像素，见下面第 2 条），默认只采 `light` 与 `midnight` 两套：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\capture-reference-shots.ps1 `
  -Shot ..\quire-desktop\target-shot\debug\quire-shot.exe -Mode Tabs `
  -TabThemes light,midnight -Force
```

## 数据安全（已验证，不是推断）

- `quire-shot` 只在 `--scene` 名包含 `backlinks` 或 `database` 时开库，且开的是 **`SqliteRepository::in_memory()`**（`src/bin/quire_shot.rs` 第 369-380 行）；其余场景 `repo = None`，走内存 demo 会话。
- 因此本文件此前在 `docs/08-VALIDATION.md` 里"场景会创建真实测试数据"的说法对 `quire-shot` 是**错的**，已在该文件更正。真实写库的是带 `--db` 的正常启动，本采集未使用。
- 采集前后 `quire-desktop/appdata/` 全部文件 mtime 未变（最新仍是 2026-09-30 17:45），可复核。
- 副作用一条：进程启动时会尝试绑定同步端口 5878/5879，本机当时被占用，每张图都打印 `os error 10048` 后继续渲染。截图内容不受影响，但说明**无头截图仍会拉起 LAN 服务代码路径**，实现 GPUI 版时不要以为 headless 就等于无网络。

## 命名与清单

- `t-<theme>__<scene>.png`：主题矩阵，12 个 theme id × 8 个页面 + 每个主题一张 baseline（无 `--scene`）。页面为 `edit, page-cover, marks, notes, tasks, database-table, settings, sync`。
- `s-<scene>.png`：场景全扫，188 个场景名各一张，不带 `--theme`（用场景自带配色）。
- `t-<theme>__tab-<tab>.png`：设置对话框逐 tab，mode 列为 `tab`，共 16 张（`light`、`midnight` × `appearance, general, sync, mcp, keys, about, sync-stats, sync-log`）。`appearance` 两张与对应主题的 `__settings` 同图，是预期结果（点默认 tab 不改变状态），脚本报的 `NO-CHANGE` 就是它。
- `manifest.tsv` 列：`name / mode / scene / theme / size / bytes / sha256 / status`，312 行全部 `OK`。

## 重复哈希读出的五条事实

312 张图共 274 个不同 SHA-256。重复不是错误，但每组都在说话：

1. `t-midnight__baseline` 与 `s-dark` 同图；`t-midnight__<scene>` 与 `s-dark-<scene>` 逐对同图（notes/page-cover/marks/tasks/database-table/settings 全部命中）。**结论：`dark-X` 场景等价于 `X` + midnight 主题**，采主题矩阵时用 `--theme` 就够，不必再采 `dark-*`。
2. `t-<theme>__settings` 与 `t-<theme>__sync` **12 个主题全部逐对同图**（`t-light__settings`/`s-settings`/`s-sync`/`s-sync-log`/`s-sync-stats` 也全是同一张 `28F571DDDB6C…`）。已解释，原因是这批场景没有一个把设置对话框从默认 tab 移开：
   - `settings` 场景只做 `g.set_settings_open(true)`（controller.rs:11350）。
   - `sync | sync-stats | sync-log` 分支同样只 `set_settings_open(true)`（controller.rs:9723），它们设的是 **`sync_tab`**（9725/9728，同步页内部的 设备/统计/日志 子 tab），从不设 `settings-tab`。
   - `settings-tab` 默认值为 0（`ui/Types.slint:979`），而 0 = 外观（`docs/SETTINGS_UI.md:23`：0 外观 / 1 通用 / 2 同步 / 3 MCP / 4 快捷键 / 5 关于）。同步页内容挂在 `UIState.settings-tab == 2` 的分支下（`ui/components/SettingsDialog.slint:658`）。
   - 已用肉眼复核 `t-jade__settings.png`：对话框落在 **外观** tab，13 张主题卡（跟随系统 + 12 套，各自画自己的渐变，jade 带青色选中边框），下面是界面缩放行与「完成」按钮。**同步种子数据一个像素都没出现。**
   - **结论：`settings` 与 `sync` 这 24 张只是设置对话框的外观 tab**，旧版没有任何静态场景能拍到同步页。已用 `-Mode Tabs` 补采 16 张逐 tab 图（见上面命名一节）；`--click`/`--hover` 接受**逻辑坐标**（`quire_shot.rs:297-301` 用 `LogicalPosition`），而 `--w/--h` 是物理像素，离屏 scale factor 为 1 时两者数值相同。1280×800 下 tab 条在 y=205，六个 tab 的 x 依次是 418 / 500 / 585 / 669 / 753 / 845。
3. `s-dark-sync-log` 与 `s-dark-sync-stats` 等于同一张图是上一条的**同一个根因**，不是「无数据时不改变像素」：子 tab 挂在看不见的页面上。补采时同步页的三个子状态靠 `--scene sync-stats|sync-log` 播种 + 一次点击 同步 tab 得到（`t-light__tab-sync-stats.png`、`t-light__tab-sync-log.png`：日志页可见「同步日志 6 条记录」与两行记录）。
4. `t-light__baseline` 与 `s-board-drag`、`s-mcp-toggle`、`s-sidebar-toggle`、`s-theme-fade` 同图。这 4 个名字是 **`--motion` 动画规格，不是静态场景**（`docs/15-SCENE-CATALOG.md` 的「与默认页同图」列已用哈希把它们标出）。拿它们当 `--scene` 用会静默得到默认页——`quire-shot` 对无法识别的场景名不报错。所以脚本内置了 NO-EFFECT 检查：某主题的 baseline 与场景图字节相同即报警。
5. **补采的 tab 图暴露出一处真实裁切**：`t-light__tab-sync.png` 里本机名称行的状态文字（`端口 5878 无法监听 — 本机收不到其他设备的同步：{e}`，controller.rs:249 拼出）在对话框右边界被**硬切成半个「通」字，没有省略号**；`t-light__tab-mcp.png` 的段落同样把换行点画到了可见区之外。已放大逐像素复核，并排除了"被窗口右边界切掉"这种解释：同场景在 **1600×900** 重采（对话框随窗口居中，tab 条移到 y=253、同步 tab x=741），断点仍然落在对话框右边界（≈1080）而不是窗口边界，边界外是完整的文档背景。
   - 对话框宽度是常量 `width: 560px`（SettingsDialog.slint:327），而这段 Text 明明写了 `horizontal-stretch: 1; overflow: elide`（SyncPage.slint:499-506）却没出省略号 → 溢出发生在对话框**内部**：内容列被撑得比 560 宽，整列被 PopupWindow 表面裁掉。具体是哪一行把列撑宽未逐层求证，实现时按源码自己定，不复现就按 docs/01 的规矩写进 `DIFFERENCES.md`。
   - 这条同时说明：**逐 tab 的参考图右侧是不完整的**，不能拿 `t-light__tab-sync.png` 的右边界当"同步页就这么宽"的证据。

## 动效参考（未采，配方已给）

静态图不含中间帧。旧版有现成的动画采集：

```powershell
..\quire-desktop\target-shot\debug\quire-shot.exe --motion sidebar-toggle --scene default `
  --gif clip.gif --frames 20 --frame-ms 40 --gif-w 560 --seq-dir seq-sidebar
```

`--motion` 手工推进 Slint 时钟（`CLOCK`，见 `quire_shot.rs` 注释），第 N 帧就是动画第 N×frame-ms 的真实插值，不是截图连拍。已知的 motion 规格：`sidebar-toggle`、`theme-fade`、`mcp-toggle`、`board-drag`。P2 阶段应据此采一套 GIF/帧序列作为动效对照。

## 覆盖缺口（明确未采到）

- **设置对话框 tab 只采了 2 套主题**：`light` 与 `midnight`；另外 10 套主题的 6 个 tab 未拍（`-TabThemes` 传全量即可补齐，代价是一次运行 80 张）。外观 tab 有全 12 套（`t-<theme>__settings`）。
- **MCP 页的运行态**：`t-<theme>__tab-mcp.png` 拍到的是"未开启/无令牌"的空态（提示词框是空的，调用日志 0 条）。有令牌、有调用记录的状态没有场景能种出来，只有 `mcp-toggle` 这个 motion；要参考得先补场景或走真实进程。
- **同步页右侧内容**：见上面第 5 条，逐 tab 图的右边界被对话框裁切，`淘汰离线设备` 右侧与状态句尾部不在图内。
- **独立速记窗口**：`QuickNoteWindow.slint` 是另一个窗口，离屏截图只渲染 `AppWindow`，未覆盖。
- **hover/pressed/focus/disabled 态**：`quire-shot` 支持 `--hover x,y` 与 `--click x,y`、`--key <k>`，本轮只在 tab 条上用了 `--click`，未系统性采集其余控件的交互态。
- **窄窗口与多 DPI**：主题矩阵与场景全扫全是 1280×800 物理像素（第 5 条的对照实验用过一次 1600×900，未入库）；`docs/08` 要求的 900×700、1920×1080 与 125%/150%/200% 未采。软件渲染离屏图不受系统 DPI 影响，**不能**当作 DPI 验收证据，那必须在真窗口上测。
- **真窗口 FemtoVG 对照**：本轮全是软件光栅，字体栅格与默认 GPU 路径不同。`docs/08` 已说明只能用于布局与 token 参考。
