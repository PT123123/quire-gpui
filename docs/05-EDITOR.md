# 编辑器与输入规格

## 目标

沿用 core 的块树与命令语义，构建显示和编辑一致的块编辑器。旧版是静态 Text/marked runs 与焦点 TextInput 切换；新实现可以调整结构，但不得让切换造成字形、行高、折行和光标位置跳动。

P1 必须决定并验证文本方案：组件库输入控件是否支持所需 styled text、composition、选区和可变高度；若不足，使用 GPUI 文本排版/输入接口实现编辑 element。先证明混合样式、CJK、emoji 和长链接成立，再扩展所有块类型。

## 唯一几何来源

同一 layout 结果驱动字形、换行、行高、背景/删除线、链接命中、光标、选区、查找命中、IME anchor 和滚动定位。不能用普通文本测高，另画粗体/mono 文本，最后把更高的实际内容裁掉。

排版缓存至少按 block ID、文本/marks revision、内容宽度、字号/字体、DPI/字体环境和相关 style 失效；颜色主题切换尽可能只重绘。链接、粗体中文、连续无空格文本、组合字符与 emoji 不能用固定字符宽估算。

## 字符位置与输入法

明确区分 UTF-8 byte offset、平台 UTF-16 offset、grapheme 边界、排版 glyph cluster 与 UI selection。core 使用什么单位以实际接口为准，适配层集中转换。不得将 Rust 字节索引直接送到平台文本范围，也不能把 emoji 半个代理项作为光标位置。

CompositionState 保存预编辑范围/文本/选区与原始替换范围。组合中的 Enter、方向键、Backspace 优先由 IME 消费，不触发拆块、菜单选择或普通撤销。候选提交只写一次；取消不残留；替换选区不能重复删除。拼音预编辑不得被 persistence 当成最终正文保存。

实际测试微软拼音，条件允许再测第三方中文 IME：连续输入、多次候选翻页、数字候选、空格、标点、中英文切换、输入中切块/点外部/关闭菜单/切窗口；明确 commit/cancel 策略。DPI 和多屏验证候选框在当前 caret 附近，无比例或窗口偏移。

## 编辑操作

- 单击定位、拖动选择、Shift 扩选、Home/End/上下跨视觉行，目标横坐标按排版维护。
- Enter 拆块、Backspace-at-start 合并、空列表退出、Tab/Shift+Tab 嵌套、待办切换。
- Ctrl+B/I/E/Shift+X 样式；Ctrl+L 链接；Ctrl+D 复制块；Ctrl+S 强制保存；Undo/Redo 复用 core 语义。
- Markdown 前缀转换、slash 转换与清理前缀合并成一次 undo。
- 内部/外部链接、页面引用、同步块内容 ID、锁定页只读。
- 富剪贴板/块复制/多行粘贴遵守当前格式；无法识别内容回退纯文本，并保留 Unicode。

跨块 selection 等能力先核对旧版实际支持程度；至少不能退化已有能力。新扩展单列后续项，不能用尚未完成的扩展阻止现有功能验收。

## Undo 与草稿

输入 burst 合并规则参考旧版 controller/state，不照搬早期文档中的固定 debounce 数字。普通字符、composition 提交、粘贴、结构变化、格式修改、页面切换如何切事务需明确。core 历史与组件自身 undo 不得同时生效造成“双重撤销”。

批量动作一步撤销。selection restoration 与文档恢复同步；undo 后不将光标留在已经删除的 ID。编辑与 organizer 的撤销上下文隔离。

## 虚拟化

只为可见块与适量 overscan 创建昂贵元素；活动输入状态放在持久 editor session，不随行回收丢失。以稳定 block ID 标识，过滤/折叠/拖动/插入后不依赖旧索引。

可变高度与 prefix sum/等价索引支持 scroll-to-block；测高更新维护滚动锚点，前面某块变高不能把阅读位置抛走。宽度/字体变化重新排版，程序滚动与用户滚动避免来回纠正。页头、封面、尾部、backlink 也计入布局。

表格子单元、分栏内部块、折叠内容不能重复投影为顶层行。同步块显示的 row ID 与编辑 source ID 分开，防止改错对象。拖放保持父子约束，落点按实时 viewport 转换，在 auto-scroll 时持续更新。

## 全块类型

扫描 core kind 定义与 EditorBlock.slint 分支，至少包含段落、h1/h2/h3、bullet、numbered、todo、quote、code、divider、callout、page、link、toggle、image、file、table/cell、columns/column、math、TOC、embed、database、synced。不把 kind 数字凭空重排；未知 kind 显示安全占位并保留原数据。

代码高亮、语言菜单、数学显示、TOC 跳转、图片缩放/放大、附件打开/另存为、表格 Tab 扩行、分栏增删等以现有实现为准。

## 必须通过的案例

中文+英文+emoji 混合选区修改；跨行粗体和 mono；长链接/连续中文折行；composition 后 undo 一次；拆合块后保存重启；锁定页快捷键不写；焦点块滚出后滚回；查找滚动后选区正确；删除/恢复含附件或容器块；速记与主编辑器焦点切换不串输入。
