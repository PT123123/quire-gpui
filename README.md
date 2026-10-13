# Quire GPUI — 桌面端完整复刻交接包

创建日期：2026-10-14。当前状态：**只完成规格、计划与参考快照，尚未创建 GPUI 应用或验证其运行效果**。

目标：用 Rust + GPUI 重写 Quire 桌面 UI，复用现有 `quire-core`，尽量完整复刻当前 `quire-desktop` 的界面、12 套主题（包括 4 套渐变）、交互、数据行为与 Windows 桌面集成。在复刻成立的基础上，改善富文本折行、焦点和编辑一致性。

用户授权的本轮范围是创建文件夹和详细交接文档。后续实现 AI 根据用户后续指令开展编码；不要把本交接包视为已经实现、已经通过测试或已经决定弃用旧版。

## 阅读顺序

1. `AGENTS.md`：执行规则、完成定义与边界。
2. `PLAN.md`：阶段、任务、阻塞门槛、交付物（P0 已标注本轮完成度）。
3. `docs/13-DECISIONS-NEEDED.md`：**2026-10-14 已拍板一轮**，看开头的拍板表；仍开放三条（上游锚点细节、MCP 页面工具、内存门槛）。
4. `docs/ADR-001-GPUI-STACK.md`：GPUI 一手事实核验 + 本机实测记录（探针 1 已在发布版 `gpui 0.2.2` 上通过：能装、能编、能起带 IME 的输入窗）、三条依赖路线 A1/A2/B、真实工具链、7 条 P0 探针。
5. `docs/16-REVIEW-AND-NEXT-STEPS.md`：另一位 AI 对上述产物的复核。**它是建议，不是用户批准**；与 `docs/13` 拍板表冲突时以用户结论为准。逐条处理结果记在 `docs/DELIVERY-STATUS.md` 的"对 docs/16 的处理"一节——其中六条已核实并落到正文，最重要的是 ADR-001 那条"crates.io 装不起来、只能走 git 依赖"的证据**混淆了两个源码版本，已撤回**（他抓对了，我错了）。
6. `docs/01-PRODUCT-SCOPE.md`：功能范围与复刻原则。
7. `docs/02-ARCHITECTURE.md`：GPUI 架构、core 边界和状态流。
8. `docs/11-CORE-API.md`：`quire-core@db0e95f` 的 API 面、文本单位、撤销归属、不许重实现的清单。
9. `docs/03-VISUAL-REPLICATION.md`：逐页界面及交互规格。
10. `docs/04-THEMES-AND-DESIGN.md`：主题、渐变、排版、动效。
11. `docs/12-DESIGN-CONSTANTS.md`：349 条尺寸属性、62 处动画、色值与字号台账。
12. `docs/05-EDITOR.md`：编辑器、IME、富文本、虚拟化。
13. `docs/06-FEATURE-PARITY.md` 与 `docs/PARITY.md`：矩阵方法与当前骨架（246 callback / 261 property）。
14. `docs/07-DATA-AND-PLATFORM.md`：数据兼容、同步、托盘和多窗口。
15. `docs/08-VALIDATION.md`：视觉、交互、数据、性能验收。
16. `docs/14-REFERENCE-SHOTS.md` 与 `reference/shots/`：312 张基线参考图与采集身份。
17. `docs/09-HANDOFF.md`：交给实现 AI 的直接任务说明。
18. `docs/REFERENCE-MANIFEST.json`、`docs/10-SOURCE-INVENTORY.md`、`docs/15-SCENE-CATALOG.md`：参考版本、文件哈希、组件/回调/场景索引。

## 可重跑的生成脚本

```text
node tools/extract-parity.mjs            # PARITY.csv（合并 PARITY-SEMANTICS.csv 的人工列）/ PARITY-STATE.csv / PARITY-UNWIRED.txt / 15-SCENE-CATALOG.md
node tools/extract-design-constants.mjs  # 12-DESIGN-CONSTANTS.md
powershell -File tools/capture-reference-shots.ps1 -Shot ..\quire-desktop\target-shot\debug\quire-shot.exe -Mode Both
powershell -File tools/capture-reference-shots.ps1 -Shot ..\quire-desktop\target-shot\debug\quire-shot.exe -Mode Tabs
```

三个脚本只读 `reference/slint-desktop`，输出可复现；旧版源码变了先跑 `tools/freeze-reference.ps1` 生成新快照再重跑。`-Mode Tabs` 是设置对话框逐 tab 补采（靠 `--click`，坐标写死在默认参数里，见 docs/14）。

## 项目关系

- `../quire-desktop`：当前可运行的 Slint 桌面版，行为和视觉参考。
- `../quire-core`：本地核心代码，可用于阅读；实际桌面依赖以旧版 Cargo.toml 的 Git rev 为准。
- `../quire-compose`：Android 端参考，不在此次重写范围内。
- `reference/slint-desktop`：创建本交接包时的旧版源码及选定文档快照；不是新项目实现。
- `reference/shots`：312 张离屏基线参考图（12 主题 × 8 页面 + 188 场景全扫 + 设置对话框逐 tab）与 `manifest.tsv`；采集身份与缺口见 `docs/14-REFERENCE-SHOTS.md`。

参考快照不包含用户数据库、附件、配置、target、安装包和运行日志。源码中可能有示例路径或测试字符串，不能把这些当作当前用户数据位置。

## 核心约定

先验证最难的编辑器，再完整迁移。不能先画一个好看的壳，最后才发现中文输入和选区不可靠。

复刻是默认任务，不是只借鉴风格。侧栏、编辑区、整理区、数据库视图、设置、同步、MCP、速记、历史版本和各类弹层都要纳入范围；不能只完成文档页后宣布完工。

独立程序名建议 `quire-gpui`，独立开发数据目录。正式兼容切换前始终用测试库副本验证，不与旧版同时写同一个库。

## 给后续 AI 的起始提示

交接资料已经够用，**这一轮的产出是一个跑起来的程序**：一个窗口、一个页面、能用微软拼音连续中文输入、能保存进测试库并重启读回（AGENTS.md「第一竖切」与 PLAN.md「下一轮目标」）。

先读 `AGENTS.md`（含 2026-10-14 已拍板的决策）、`PLAN.md`、`docs/13-DECISIONS-NEEDED.md` 的拍板表、`docs/ADR-001-GPUI-STACK.md` 与 `docs/09-HANDOFF.md`。ADR-001 第 6 节的探针 1（最小窗口）已在发布版 `gpui 0.2.2` 上跑通并记了实测记录，其余探针随竖切跑，不要攒成开工前置门槛。依赖选型三条路都允许走（A1 发布版 / A2 git 钉 tag / B Kit），比较口径是输入效果、维护成本、组件能力。core 保持不动；数据目录独立且默认可写，**绝不自动发现或迁移旧版真实库**，要碰旧库只能由用户显式 `--db` 指向测试副本。严格复刻 `reference/slint-desktop` 的界面与 12 主题 4 渐变（基线 `reference/shots`，已冻结为 `baseline-2026-10-14`）。中文输入出问题先定位修复，别急着自己绘文本层——自绘不解决系统 IME。明确缺陷要修（如同步/MCP 页文字裁切）并逐处写 `docs/DIFFERENCES.md`。功能清单以 `docs/PARITY.csv` 为骨架，人工语义与进度写 `docs/PARITY-SEMANTICS.csv`（生成器合并，重跑不覆盖），实现到哪一行补哪一行，不要为了补文档而补文档。逐项记录已实现/已验证/未验证/阻塞，编译通过和截图都不算交互验收。
