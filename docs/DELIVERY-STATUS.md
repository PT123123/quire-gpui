# 交付状态

## 第一轮 2026-10-14（02:31）：交接包骨架

- 新建独立 quire-gpui 目录，未修改原 Slint/Android/core 项目。
- README、AGENTS、P0–P7 实施计划，以及产品/架构/逐页界面/设计系统/编辑器/功能追踪/平台数据/验收/AI 交接说明。
- 冻结当前 Slint 工作区 113 个参考文件，包含未提交修改；完整列表、commit、dirty 列表、时间与 SHA-256 见 REFERENCE-MANIFEST.json。
- 抽取 12 套完整基础 palette 和设计 token 表达式，核验其中 4 套渐变。
- 生成组件、UI callback、场景分支索引，供后续逐项建立功能矩阵。

## 第二轮 2026-10-14（03:05）：规格补深 + 事实核验

复核过的（不是转述，是本轮重算/重开文件）：

- 113 个参考文件 SHA-256 全量重算，与 manifest **逐条一致**，无缺失无漂移。
- manifest 的 dirty 列表与 `../quire-desktop` 当前 `git status` 的 10 个文件逐行一致，HEAD 仍是 `47ef6f8`。
- 本地 `../quire-core` 的 HEAD 恰为旧版 pin 的 `db0e95f`，工作树干净。
- 12 套 palette、4 套渐变的端点值与 `ui/Colors.slint` 逐行对得上；`design-tokens.json` 的 12 个 id 顺序与主题名一致。

新增产物：

- `docs/PARITY.md` + `PARITY.csv`（246 callback 逐行带入口/注册点/首语句/场景）+ `PARITY-STATE.csv`（261 property）+ `PARITY-UNWIRED.txt`。
- `docs/11-CORE-API.md`：core 在 `db0e95f` 的完整 API 面，含"文本单位是 UTF-8 字节偏移""块类型在 core 是字符串、0..24 数字是桌面端自造""撤销归 core（每页栈 CAP=100）""MCP 不在 core"等承重结论，逐条带 file:line。
- `docs/12-DESIGN-CONSTANTS.md`：349 条具名尺寸属性、62 处 animate、8 个 8 位色值、5 档字号的台账。
- `docs/ADR-001-GPUI-STACK.md`：GPUI/gpui-kit 一手事实（含 crates.io 0.2.2 装不起来、入口 API 不兼容、5 个未关闭中文 IME issue、无托盘、渐变坐标语义）+ 本机工具链缺口 + 7 条 P0 探针。
- `docs/13-DECISIONS-NEEDED.md`：九件必须用户拍板的事，含代价与建议。
- `docs/14-REFERENCE-SHOTS.md` + `reference/shots/`：312 张离屏参考图（30.6MB）与可核对的采集身份。
- `docs/15-SCENE-CATALOG.md`：188 个截图场景目录。
- `tools/extract-parity.mjs`、`tools/extract-design-constants.mjs`、`tools/capture-reference-shots.ps1`：三个可重跑的生成/采集脚本。

本轮更正的错误：`docs/08-VALIDATION.md` 原写"场景会创建真实测试数据、只对 scratch 库运行"，实际 `quire-shot` 用 `SqliteRepository::in_memory()` 且不接受 `--db`；同时补了"`--w 1280` 是物理像素、不能当 DPI 验收证据"这条坑。

收尾补采与自我更正：`docs/14` 第 2 条最初写"`settings` 与 `sync` 在 **7 个主题**下同图、原因**未解释**"，重算 manifest 后是 **12 个主题全部**同图，原因也已定位（这批场景没有一个把 `settings-tab` 从 0 移开，逐条 file:line 见该节）。据此给采集脚本加了 `-Mode Tabs`，补采 16 张设置对话框逐 tab 图（296 → 312 张），并从新图里读出一条真实裁切：同步页状态文字与 MCP 页段落在 560px 对话框右边界被硬切、无省略号，1600×900 对照证明切点在对话框而非窗口。

## 2026-10-14 决策落定（用户）

`docs/13-DECISIONS-NEEDED.md` 已回填拍板表：core 不动（缺口另议最小修改）、官方 gpui 与 gpui-kit 都按输入效果/维护成本/组件能力评估、IME 出问题先定位修复再换栈（自绘不等于解决 IME）、独立数据目录并保留旧版、312 张参考图冻结为 `baseline-2026-10-14`、明确缺陷要修并逐处写 `docs/DIFFERENCES.md`。仍开放三条：上游锚点细节、MCP 页面级工具、内存门槛拆法——都不阻塞第一竖切。

下一轮目标改为**能运行、能微软拼音输入、能保存一页并重启读回**的程序（AGENTS.md「第一竖切」、PLAN.md「下一轮目标」），并明确停止扩充交接文档。

## 仍然没有做的事（不要误认已完成）

尚未创建 Cargo 应用、未选定最终依赖组合、未编译、未运行任何 GPUI 代码；未测中文 IME、未测视觉/性能/数据往返/同步/Windows 集成。功能矩阵是**入口骨架**，语义列空白，未与 core 的 50 个 Command 和 MCP 的 10 个工具交叉。参考图缺：MCP 页运行态（只有"未开启/无令牌"空态）、其余 10 套主题的逐 tab 图、独立速记窗口、hover/pressed/focus/disabled 态、窄窗口、真窗口 GPU 对照、动效中间帧。

旧版源码中的截图工具与场景已保存，可由后续 AI 在独立测试环境生成参考图。源码和 token 是可追溯的参考，不等于新程序已有同样行为。

参考目录只为交接，不能作为可编译的旧版完整仓库使用：发布资源、运行数据、二进制等未复制。实际运行旧版请使用相邻 quire-desktop 并遵守测试库要求。

如果旧版继续变化，保留本次快照，新增单独版本参考与差异记录。freeze-reference.ps1 遇到已有快照会拒绝覆盖。本轮 312 张图建议按 `docs/13-DECISIONS-NEEDED.md` 第 5 条冻结为 `baseline-2026-10-14`。
