# 功能矩阵使用说明

生成命令：`node tools/extract-parity.mjs`（只读 `reference/slint-desktop`，重跑覆盖输出）。**人工填的语义与进度不要写在 `docs/PARITY.csv` 里**——它是生成产物，重跑必然覆盖；写 `docs/PARITY-SEMANTICS.csv`，由生成器按回调名合并进 PARITY.csv。

| 文件 | 内容 | 规模 |
| --- | --- | --- |
| `docs/PARITY.csv` | 每个 UIState callback 一行：入口、签名、controller 注册点、首条语句、调用的 core/state、关联场景、实现/验证状态、差异、人工语义（后四列来自 sidecar） | 246 行 |
| `docs/PARITY-SEMANTICS.csv` | **人工维护**，按 `回调名` 索引：`实现状态,验证状态,差异,人工语义` | 每完成一段实现，就补对应回调；GPUI 同步/MCP 当前已逐项标出已实现与未实现项 |
| `docs/PARITY-STATE.csv` | 每个 UIState property 一行：方向、类型、默认值、声明位置 | 261 行 |
| `docs/PARITY-UNWIRED.txt` | 无调用点/无注册的 callback 清单 | 14 + 8 |
| `docs/15-SCENE-CATALOG.md` | 截图场景目录（含「叠加」「暗色变体」两列） | 188 个场景 |

CSV 用 UTF-8 BOM + CRLF，Excel/WPS 可直接打开。`实现状态` 与 `验证状态` 是**两列**，不许合并：按钮存在≠动作落地，动作落地≠验收通过。

sidecar 的两条规则：**留空=没有人工结论**，不会覆盖生成列的默认值（`未开始`/`未验证`），所以要改就得写满那一格；**回调名拼错会被点名**，生成时打印"合并 N 个回调，M 条回调名在生成结果里找不到"，M>0 就是白填了。重跑幂等已验：去掉新增列后，246 行生成列与上一版逐字节相同。

## 区域分布（246 个 callback）

整理区 58 · 数据库 56 · 编辑器 35 · 主窗口与弹层 18 · 侧栏与导航 14 · 未接线 14 · 设置 13 · 内容块 10 · 同步 8 · 历史版本 6 · 搜索与命令面板 5 · 通用弹层 3 · MCP 3 · 反向链接 2 · 图标选择 1

## 三次覆盖审计的当前数字

1. `ui/` 44 个 .slint、246 个 callback、261 个 property、188 个截图场景、62 处 animate、349 条具名尺寸属性（见 `docs/12-DESIGN-CONSTANTS.md`）。
2. `controller.rs` 有 288 个 `on_*` 注册点，覆盖 238 个 callback。
3. 差集已逐条开文件核实，见下节。

## 14 个「只有声明」的 callback（已复核，不是漏扫）

`grep -rn <名字> ui/ src/` 全仓只命中 `Types.slint` 的声明行：

- 有声明、有注册、无 UI 调用点（6 个）：`insert-at-end-requested`（Types.slint:1068）、`db-columns-closed`（1168）、`db-filter-closed`（1176）、`db-group-closed`（1197）、`db-rollup-closed`（1307）、`org-close-requested`（1557）。
- 声明、注册、调用点三者皆无（8 个）：`org-note-title-set`（1568）、`org-note-body-set`（1569）、`org-note-tags-set`（1571）、`org-task-title-set`（1697）、`org-task-notes-set`（1698）、`org-task-tags-set`（1703）、`org-task-moved`（1726）、`org-subtask-title-set`（1751）。

**这不是可以砍掉的功能，是已经换了一条实现路径**，两条都已核实：

- 整理区草稿走双向 property：`org-note-title-draft`（Types.slint:1531）、`org-task-title-draft`（1534），controller 在 7332-7337 直接 set/clear。所以 `org-*-set` 那 8 个 callback 是早期设计的残留。
- 数据库弹层关闭走 property + `changed` 回调：`db-filter-open`（Types.slint:880）在 `AppWindow.slint:568` 与 UIState 双向绑定，566-674 的 `changed db-filter-open` 负责显示与自动关闭；`db-*-closed` callback 从未被触发。

给新壳的含义：**别照着这 14 个名字写 14 个空处理器**。要复刻的是"草稿不提前写库""弹层外点关闭不穿透"这两个行为，载体由新架构自己定。

## 当前覆盖与剩余验收

截至 2026-10-16，`PARITY-SEMANTICS.csv` 有 246 个唯一回调键；实现状态、验证状态和行为差异 246/246 已填写，人工语义也为 246/246 填写。重复键现在会让生成器报错，避免同一回调悄悄采用旧行。**语义齐全不代表功能已验收**：目前 37 项为已验证、209 项仍未验证；桌面 UI 逐项操作、参考图/主题对照和系统级场景仍是 P7 未关闭项。

余下工作是对每个场景建立可复查证据：核对 handler 与 `.slint` 触发点、旧版快捷键设置、写入/持久化/undo 行为，并为自动 smoke 或真人桌面步骤登记结果。按区域推进，不把“代码存在”或“cargo test 通过”代替用户可见交互验收。

另有两个来源需继续与 callback 矩阵交叉：GPUI `src/mcp_tools.rs` 的 16 个 MCP 工具（agent 入口，UI 上没有 callback；旧版基线为整理区 10 个工具），以及 core 侧 50 个 `Command` 变体的 GPUI 接线。Command 已逐项核对到实现入口或 Session 封装；仍需随新增行为持续回归。复杂页面 MCP 编辑会安全拒绝，不能把它描述为覆盖所有块类型。

## 状态取值

`未开始 / 实现中 / 已实现 / 已验证 / 阻塞 / 经用户确认延期`。只有跑过 `docs/08-VALIDATION.md` 对应证据才允许写「已验证」；编译通过和截图好看都不算。
