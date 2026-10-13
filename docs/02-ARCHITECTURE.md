# 架构与依赖选择

## 依赖原则

GPUI 是底层 UI 框架，不自动附带完整应用架构或 Notion 编辑器。可以评估 GPUI Kit/其组件层来提供输入框、虚拟列表、弹层、表格等基础能力；组件的默认外观不能覆盖 Quire 设计系统。

选型时重新检查官方资料，锁定能在 Windows 上编译和运行的兼容组合。不要把多个 GPUI fork、不同 Git commit 与组件库依赖混装。记录包来源、版本、commit、Rust 最低版本和许可证；建议提交 Cargo.lock。现有 core pin 为 `db0e95f`，完整依赖表达见 reference Cargo.toml；不得仅因本地 core 更新就默默换 pin。

官方参考（需实现时重新核对）：https://gpui.rs/ 、https://gpui.rs/examples/ 、https://github.com/zed-industries/zed 、https://gpui-kit.com/docs/ 。

## 建议目录（不是已生成实现）

```text
src/
  main.rs                 参数、日志、初始化、事件循环
  app/                    AppState、typed actions、会话、投影、任务协调
  design/                 palette、semantic tokens、typography、icons、motion
  views/                  shell、sidebar、editor、organizer、settings、search
  editor/                 text layout、selection、input、viewport、drag、blocks
  database/               properties、cells、views、overlays
  components/             基础按钮、菜单、dialog、theme card、empty state
  platform/               Windows 窗口、托盘、热键、剪贴板、显示器
  testing/                场景种子、截图入口、输入回放、测量
tests/                    编辑、持久化、状态/平台集成测试
```

早期用单 crate 即可，不为了形式把模块拆成多个 crate；core 保持独立依赖。

## 状态划分

AppState 持有 core 会话、工作区导航、设置、后台任务与窗口注册表。DocumentSession 持有页面文档、编辑命令历史和当前事务边界。EditorState 持有焦点块、selection、composition、滚动锚点、拖动和搜索命中。OrganizerState 持有笔记/任务草稿、选中 ID 集、排序/过滤、待删除批次和区域撤销栈。OverlayState 持有弹层栈、锚点和返回焦点。

保存的设置与短暂 UI 状态分开：theme ID、侧栏宽度等按旧版持久化；hover、菜单坐标、临时 composition 不能落库。每个字段只设一个权威来源。视图可以拥有局部交互状态，但跨组件共享的数据不能复制成多份真相。

## 数据流

用户输入 → typed action/编辑事务 → core command/service → Changes/历史 → 更新受影响投影 → GPUI 通知 → 渲染。存储经 persistence service 防抖并批量提交。

UI 不直接写 SQL，不在 render 中修改业务状态、发起网络请求、解析整份文档或编码图片。不在每个字符输入时全页重建全部块；可见范围投影、缓存失效和后台结果应用应可定位。

搜索/IO/导入导出/图片解码/同步在后台执行。任务返回携带会话/页面/查询 generation，旧查询不能覆盖新结果；窗口关闭后的回调不得访问已销毁实体。core 如果要求单线程访问，就由拥有者串行执行命令，不强行把非 Send 对象跨线程传递。

## 窗口和弹层

主窗口与速记窗口共享 AppState、主题和同一笔记提交服务；分别拥有焦点/窗口几何。用 QuickNoteSession 显式管理草稿与承载窗口，避免两份输入同时修改。

OverlayHost 负责 modal 与 anchored popup 两类；建立明确关闭次序和焦点恢复。不要把每个行内菜单注册成独立系统窗口，除非实际平台行为要求如此。滚动后重新定位或关闭锚定弹层，不得留在失效坐标。

## 兼容边界

core::Document/命令/Changes/Repository 等接口需要以实际依赖源码核对。旧版 state.rs/controller.rs 同时混合业务调用和 Slint 投影，只提取与 UI 无关的逻辑，不整体复制并继续依赖 Slint。

新架构 ADR 至少包括：依赖组合；文本编辑与排版策略；虚拟化/滚动锚点；输入和 undo 分组；多窗口共享状态；截图测试方法；数据库路径/应用标识。
