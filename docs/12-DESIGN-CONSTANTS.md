# 设计常量台账（自动抽取）

生成命令：`node tools/extract-design-constants.mjs`。来源：`reference/slint-desktop/ui` 共 44 个 .slint 文件。

这份表是 docs/03、docs/04 的数值底账：新实现每落一个尺寸/时长，应能在这里找到对应行；
这里没有的数值说明它内联在组件里，需要按 `file:line` 回读原文，不要凭印象补。
派生表达式（mix/lighten/darken）保留 Slint 原文，移植时要核对比例方向与色彩空间，见 docs/04。

## 1. 具名尺寸/颜色属性（349 条）

| 文件 | 属性 | 类型 | 默认值 | 邻近注释 |
| --- | --- | --- | --- | --- |
| `AppWindow.slint:1109` | aspect | float | `UIState.image-aspect(UIState.preview-attachment)` |  |
| `AppWindow.slint:1110` | box-w | length | `root.width - 2 * Theme.spacing-xl` |  |
| `AppWindow.slint:1111` | box-h | length | `root.height - 2 * Theme.spacing-xl` |  |
| `Colors.slint:111` | theme-index | int | `Colors.index-of(Theme.theme)` |  |
| `Colors.slint:115` | background | color | `Colors.p.bg` | ---- backgrounds & surfaces ---- |
| `Colors.slint:121` | sidebar | color | `Colors.p.bg-elev` |  |
| `Colors.slint:122` | surface | color | `Colors.p.bg-elev` |  |
| `Colors.slint:123` | surface-hover | color | `Colors.p.bg-elev2` |  |
| `Colors.slint:124` | surface-selected | color | `Colors.p.bg-elev2.mix(Colors.p.accent, 0.22)` |  |
| `Colors.slint:125` | surface-overlay | color | `Colors.p.bg-elev2` |  |
| `Colors.slint:142` | cover-scrim | color | `#0000009e` | is the picture, not the palette. |
| `Colors.slint:147` | text-primary | color | `Colors.p.fg` | (`applyThemeColors`'s `fgSoft`): nothing here is a new colour. |
| `Colors.slint:148` | text-secondary | color | `Colors.p.fg.mix(Colors.p.fg-muted, 0.55)` |  |
| `Colors.slint:149` | text-muted | color | `Colors.p.fg-muted` |  |
| `Colors.slint:150` | text-on-accent | color | `#ffffff` |  |
| `Colors.slint:153` | border | color | `Colors.p.border` | ---- lines & fills ---- |
| `Colors.slint:154` | border-strong | color | `Colors.p.border.mix(Colors.p.fg, 0.14)` |  |
| `Colors.slint:155` | divider | color | `Colors.p.border` |  |
| `Colors.slint:163` | accent | color | `Colors.p.accent` | primary tier legible on the tint by construction. |
| `Colors.slint:164` | accent-soft | color | `Colors.p.accent.mix(Colors.p.bg, 0.84)` |  |
| `Colors.slint:168` | danger | color | `Colors.p.danger` |  |
| `Colors.slint:169` | success | color | `Colors.p.ok` |  |
| `Colors.slint:170` | selection | color | `Colors.p.accent.mix(Colors.p.bg, 0.50)` |  |
| `Colors.slint:189` | find-hit | color | `Theme.dark ? #3d3413 : #ffe9a8` | which is the same in every one of them. |
| `Colors.slint:190` | find-hit-border | color | `Theme.dark ? #a87718 : #bd6408` |  |
| `Colors.slint:195` | hover-fill | color | `Theme.dark ? #ffffff14 : #0000000a` | the theme: a 4% wash of the ink is 4% of the ink whatever the accent is. |
| `Colors.slint:196` | pressed-fill | color | `Theme.dark ? #ffffff20 : #00000014` |  |
| `Colors.slint:197` | focus-ring | color | `Colors.accent` |  |
| `Colors.slint:200` | shadow | color | `Theme.dark ? #000000a0 : #00000026` | ---- shadows (used sparingly: popups & palette only) ---- |
| `Colors.slint:201` | scrim | color | `#00000059` |  |
| `Colors.slint:204` | callout-background | color | `Colors.accent-soft` | ---- callout block ---- |
| `Icons.slint:14` | size | length | `16px` |  |
| `Icons.slint:31` | size | length | `16px` |  |
| `Icons.slint:47` | size | length | `16px` |  |
| `Icons.slint:63` | size | length | `16px` |  |
| `Icons.slint:77` | size | length | `16px` |  |
| `Icons.slint:90` | size | length | `16px` |  |
| `Icons.slint:104` | size | length | `16px` |  |
| `Icons.slint:124` | size | length | `16px` |  |
| `Icons.slint:139` | size | length | `16px` |  |
| `Icons.slint:155` | size | length | `16px` |  |
| `Icons.slint:178` | size | length | `16px` |  |
| `Icons.slint:194` | size | length | `16px` |  |
| `Icons.slint:210` | size | length | `16px` |  |
| `Icons.slint:227` | size | length | `16px` |  |
| `Icons.slint:249` | size | length | `16px` |  |
| `Icons.slint:268` | size | length | `16px` |  |
| `Icons.slint:286` | size | length | `16px` |  |
| `Icons.slint:299` | size | length | `16px` |  |
| `Icons.slint:313` | size | length | `16px` |  |
| `Icons.slint:332` | size | length | `16px` |  |
| `Icons.slint:346` | size | length | `16px` |  |
| `Icons.slint:362` | size | length | `16px` |  |
| `Icons.slint:378` | size | length | `16px` |  |
| `Icons.slint:392` | size | length | `16px` |  |
| `Icons.slint:408` | size | length | `16px` |  |
| `Icons.slint:423` | size | length | `16px` |  |
| `Icons.slint:441` | size | length | `16px` |  |
| `Icons.slint:458` | size | length | `16px` |  |
| `Icons.slint:478` | size | length | `16px` |  |
| `Icons.slint:493` | size | length | `16px` |  |
| `Icons.slint:508` | size | length | `16px` |  |
| `Icons.slint:527` | size | length | `16px` |  |
| `Icons.slint:560` | size | length | `16px` |  |
| `Icons.slint:598` | size | length | `16px` |  |
| `Icons.slint:625` | size | length | `16px` |  |
| `Icons.slint:654` | size | length | `16px` |  |
| `Icons.slint:694` | size | length | `16px` |  |
| `Icons.slint:714` | size | length | `16px` |  |
| `Icons.slint:716` | stroke-width | length | `1.8px` |  |
| `Theme.slint:25` | spacing-xxs | length | `2px` | ---- spacing scale (the only allowed step ladder) ---- |
| `Theme.slint:26` | spacing-xs | length | `4px` |  |
| `Theme.slint:27` | spacing-sm | length | `8px` |  |
| `Theme.slint:28` | spacing-md | length | `12px` |  |
| `Theme.slint:29` | spacing-lg | length | `16px` |  |
| `Theme.slint:30` | spacing-xl | length | `24px` |  |
| `Theme.slint:31` | spacing-xxl | length | `32px` |  |
| `Theme.slint:32` | spacing-hero | length | `64px` |  |
| `Theme.slint:35` | radius-xs | length | `3px` | ---- corner radius ---- |
| `Theme.slint:36` | radius-sm | length | `5px` |  |
| `Theme.slint:37` | radius-md | length | `8px` |  |
| `Theme.slint:38` | radius-lg | length | `12px` |  |
| `Theme.slint:39` | radius-xl | length | `16px` |  |
| `Theme.slint:42` | titlebar-height | length | `40px` | ---- structural metrics ---- |
| `Theme.slint:46` | sidebar-width | length | `UIState.sidebar-width * 1px` | also the fallback state.rs reads when the settings row is absent. |
| `Theme.slint:47` | editor-max-width | length | `760px` |  |
| `Theme.slint:48` | editor-side-padding | length | `96px` |  |
| `Types.slint:638` | sidebar-width | float | `260` | rubber band, not a drag. |
| `Types.slint:647` | sidebar-selected-id | int | `0` |  |
| `Types.slint:648` | renaming-id | int | `-1` |  |
| `Types.slint:649` | tree-viewport-y | length | `0px` |  |
| `Types.slint:654` | editor-scroll-y | length | `0px` |  |
| `Types.slint:682` | slash-focus | int | `0` |  |
| `Types.slint:684` | slash-x | length | `300px` |  |
| `Types.slint:685` | slash-y | length | `200px` |  |
| `Types.slint:689` | block-menu-open-id | int | `-1` | block-menu-open-id (>= 0 open) — no separate bool to keep in sync |
| `Types.slint:691` | block-menu-x | length | `300px` |  |
| `Types.slint:692` | block-menu-y | length | `100px` |  |
| `Types.slint:697` | link-start | int | `0` |  |
| `Types.slint:698` | link-end | int | `0` |  |
| `Types.slint:738` | sync-tab | int | `0` | ever true, which is the same rule the notes' own 笔记/任务 tabs follow. |
| `Types.slint:747` | sync-log-total | int | `0` | stored log reaches the screen and the count is Rust's. |
| `Types.slint:767` | sync-offline-count | int | `0` | comes back resumes where it left off instead of re-deciding every row. |
| `Types.slint:781` | sync-interval | int | `10` | read as "60 秒" on any frame drawn first. |
| `Types.slint:805` | page-font | int | `0` | them from these three and nothing else. |
| `Types.slint:816` | page-cover-id | int | `0` | attachment costs the raster cache once. |
| `Types.slint:828` | find-target-block | int | `-1` | when it (re)creates, driven by find-sel-gen |
| `Types.slint:829` | pending-sel-start | int | `-1` |  |
| `Types.slint:830` | pending-sel-end | int | `-1` |  |
| `Types.slint:831` | find-sel-gen | int | `0` |  |
| `Types.slint:838` | editing-id | int | `-1` | input consumes on init (then resets to -1). |
| `Types.slint:840` | pending-caret | int | `-1` |  |
| `Types.slint:847` | db-editing-record | int | `-1` | have two live inputs, whatever order the two were set in. |
| `Types.slint:848` | db-editing-property | int | `-1` |  |
| `Types.slint:853` | db-editing-block | int | `-1` | block travels with the other two rather than being guessed at. |
| `Types.slint:857` | db-picking-record | int | `-1` | popup: one popup per realized row is not a popup, it is a list. |
| `Types.slint:858` | db-picking-property | int | `-1` |  |
| `Types.slint:860` | db-menu-record | int | `-1` | A row's own menu (delete this row), keyed the same way. |
| `Types.slint:864` | editor-viewport-h | length | `720px` | every database on it sees the same one. |
| `Types.slint:869` | db-columns-block | int | `-1` |  |
| `Types.slint:871` | db-columns-x | length | `300px` |  |
| `Types.slint:872` | db-columns-y | length | `200px` |  |
| `Types.slint:881` | db-filter-block | int | `-1` |  |
| `Types.slint:882` | db-filter-x | length | `300px` |  |
| `Types.slint:883` | db-filter-y | length | `200px` |  |
| `Types.slint:887` | db-filter-panel | int | `0` |  |
| `Types.slint:888` | db-filter-edit-row | int | `-1` |  |
| `Types.slint:894` | db-group-block | int | `-1` |  |
| `Types.slint:895` | db-group-current | int | `-1` |  |
| `Types.slint:896` | db-group-x | length | `300px` |  |
| `Types.slint:897` | db-group-y | length | `200px` |  |
| `Types.slint:906` | db-search-block | int | `-1` |  |
| `Types.slint:912` | palette-focus | int | `0` |  |
| `Types.slint:914` | command-selected-id | int | `-1` |  |
| `Types.slint:919` | search-focus | int | `0` |  |
| `Types.slint:920` | search-selected-id | int | `-1` |  |
| `Types.slint:925` | menu-node-id | int | `-1` |  |
| `Types.slint:926` | menu-x | length | `240px` |  |
| `Types.slint:927` | menu-y | length | `100px` |  |
| `Types.slint:935` | icon-picker-page | int | `-1` |  |
| `Types.slint:936` | icon-picker-x | length | `240px` |  |
| `Types.slint:937` | icon-picker-y | length | `100px` |  |
| `Types.slint:947` | versions-page | int | `-1` |  |
| `Types.slint:959` | versions-selected | int | `-1` | which row's version the comparison is showing; -1 = the list |
| `Types.slint:979` | settings-tab | int | `0` | follow. A tab is a *state*, so no five booleans. |
| `Types.slint:991` | preview-attachment | int | `0` | 0 = closed. Clicking a picture opens it; clicking the scrim closes it. |
| `Types.slint:994` | window-w | length | `1280px` | window geometry mirror (for centering/anchoring popups from Rust) |
| `Types.slint:995` | window-h | length | `800px` |  |
| `Types.slint:1018` | zoom-level | float | `1.0` | instead of owning it. 1.0 = 100%. |
| `Types.slint:1041` | drop-line-row | int | `-1` | means "below the last block", -1 hidden |
| `Types.slint:1117` | code-advance | length | `0px` | every layer of a block — which is why it lives here and not per row. |
| `Types.slint:1252` | db-formula-block | int | `-1` |  |
| `Types.slint:1253` | db-formula-property | int | `-1` |  |
| `Types.slint:1254` | db-formula-record | int | `-1` |  |
| `Types.slint:1275` | db-columns-panel | int | `0` |  |
| `Types.slint:1276` | db-columns-property | int | `-1` |  |
| `Types.slint:1293` | db-relation-block | int | `-1` |  |
| `Types.slint:1294` | db-relation-record | int | `-1` |  |
| `Types.slint:1295` | db-relation-property | int | `-1` |  |
| `Types.slint:1296` | db-relation-panel | int | `0` |  |
| `Types.slint:1313` | db-rollup-block | int | `-1` |  |
| `Types.slint:1314` | db-rollup-property | int | `-1` |  |
| `Types.slint:1315` | db-rollup-panel | int | `0` |  |
| `Types.slint:1337` | page-drop-target-id | int | `-1` |  |
| `Types.slint:1366` | backlink-total | int | `0` |  |
| `Types.slint:1417` | org-tab | int | `0` |  |
| `Types.slint:1418` | org-view | int | `0` |  |
| `Types.slint:1419` | org-list | int | `-1` |  |
| `Types.slint:1421` | org-sort | int | `0` |  |
| `Types.slint:1429` | org-note-sort | int | `0` | with the tab is a control nobody can read. |
| `Types.slint:1439` | org-mode | int | `0` | 1 平铺. Session state like everything else here (ADR-0073). |
| `Types.slint:1452` | org-done-count | int | `0` |  |
| `Types.slint:1453` | org-total-count | int | `0` |  |
| `Types.slint:1461` | org-drop-list | int | `-2` | drag-reorder teaches the same lesson: a delegate never parses a payload). |
| `Types.slint:1495` | org-selected-note | int | `-1` |  |
| `Types.slint:1502` | org-note-open | int | `-1` | projected from it, so `id >= 0` here is the page's own gate. |
| `Types.slint:1503` | org-selected-task | int | `-1` |  |
| `Types.slint:1511` | org-selection-count | int | `0` |  |
| `Types.slint:1513` | org-new-list-color | int | `0` | The list a new task lands in, and the draft of a list name being typed. |
| `Types.slint:1546` | org-list-renaming | int | `-1` | one for the other would put a rename box on whichever page shared the number. |
| `Types.slint:1551` | org-field | int | `0` | purpose: it describes a keystroke in flight, not a document. |
| `Types.slint:1552` | org-field-id | int | `-1` |  |
| `Types.slint:1553` | org-field-sub | int | `-1` |  |
| `Types.slint:1602` | org-bin-note-count | int | `0` | What each half holds, for the nav row's count; the row prints the tab's own. |
| `Types.slint:1603` | org-bin-task-count | int | `0` |  |
| `Types.slint:1606` | org-note-count | int | `0` | *bin's* rows while the bin is open, so a nav row reading it would be wrong. |
| `Types.slint:1627` | org-cmd-tab | int | `0` |  |
| `Types.slint:1647` | org-capture-caret | int | `0` | it wrote, the same shape the find flow's `pending-sel-*` pair has. |
| `Types.slint:1663` | org-quick-note-tick | int | `0` |  |
| `Types.slint:1738` | org-settle-task | int | `-1` | the only thing in the area that has one. |
| `Types.slint:1739` | org-settle-alpha | float | `0.0` |  |
| `Types.slint:1745` | org-drop-y | length | `-100000px` | read it live in another sub-tree. |
| `Typography.slint:19` | size-caption | length | `11px` | page chrome |
| `Typography.slint:20` | size-ui | length | `13px` |  |
| `Typography.slint:21` | size-ui-lg | length | `14px` |  |
| `Typography.slint:22` | size-icon-sidebar | length | `13px` |  |
| `Typography.slint:23` | size-icon-picker | length | `19px` |  |
| `Typography.slint:29` | size-cover-height | length | `168px` | the band is not merely a strip above it. |
| `Typography.slint:30` | size-cover-overlap | length | `68px` |  |
| `Typography.slint:33` | size-body | length | `15.5px` | document |
| `Typography.slint:34` | line-body | float | `1.55` |  |
| `Typography.slint:35` | size-h3 | length | `18px` |  |
| `Typography.slint:36` | line-h3 | float | `1.5` |  |
| `Typography.slint:37` | size-h2 | length | `22px` |  |
| `Typography.slint:38` | line-h2 | float | `1.36` |  |
| `Typography.slint:39` | size-h1 | length | `28px` |  |
| `Typography.slint:40` | line-h1 | float | `1.36` |  |
| `Typography.slint:41` | size-page-title | length | `38px` |  |
| `Typography.slint:42` | size-code | length | `13.5px` |  |
| `Typography.slint:43` | line-code | float | `1.48` |  |
| `Typography.slint:45` | weight-regular | int | `400` |  |
| `Typography.slint:46` | weight-medium | int | `500` |  |
| `Typography.slint:47` | weight-semibold | int | `600` |  |
| `Typography.slint:48` | weight-bold | int | `700` |  |
| `Typography.slint:63` | scale | float | `UIState.page-small-text ? 0.87 : 1` | factor — 13.5/15.5, which is the ratio Notion's own two tiers use. |
| `Typography.slint:81` | size-body | length | `Typography.size-body * scale` |  |
| `Typography.slint:82` | size-h3 | length | `Typography.size-h3 * scale` |  |
| `Typography.slint:83` | size-h2 | length | `Typography.size-h2 * scale` |  |
| `Typography.slint:84` | size-h1 | length | `Typography.size-h1 * scale` |  |
| `Typography.slint:85` | size-page-title | length | `Typography.size-page-title * scale` |  |
| `Typography.slint:86` | size-code | length | `Typography.size-code * scale` |  |
| `Typography.slint:89` | size-page-icon | length | `46px * scale` | small text shrinks it along with everything else on the page. |
| `Typography.slint:93` | line-body | float | `Typography.line-body` | gets a proportionally smaller box without touching these. |
| `Typography.slint:94` | line-h3 | float | `Typography.line-h3` |  |
| `Typography.slint:95` | line-h2 | float | `Typography.line-h2` |  |
| `Typography.slint:96` | line-h1 | float | `Typography.line-h1` |  |
| `Typography.slint:97` | line-code | float | `Typography.line-code` |  |
| `components/BacklinkPanel.slint:20` | content-x | length | `` |  |
| `components/BacklinkPanel.slint:21` | content-width | length | `` |  |
| `components/BacklinkPanel.slint:33` | line-h | length | `PageType.size-body * PageType.line-body + 4px` |  |
| `components/BacklinkPanel.slint:34` | title-h | length | `Typography.size-caption * 1.6 + 2px` |  |
| `components/BlockHandle.slint:11` | block-id | int | `-1` |  |
| `components/ColumnItemRow.slint:18` | box-width | length | `` |  |
| `components/ColumnItemRow.slint:21` | row-y | length | `` | edited, which for an item means its own y inside a row inside the list |
| `components/ColumnItemRow.slint:22` | content-x | length | `` |  |
| `components/ColumnItemRow.slint:26` | pointer | int | `` | here would put one binding per item on the layout's property. |
| `components/ColumnItemRow.slint:55` | text-color | color | `Colors.block-text(item.color)` |  |
| `components/ColumnItemRow.slint:56` | bg-color | color | `Colors.block-bg(item.bg)` |  |
| `components/ColumnItemRow.slint:68` | pic-aspect | float | `is-image ? UIState.image-aspect(item.attachment) : 0` |  |
| `components/ColumnItemRow.slint:69` | pic-width | length | `box-width` |  |
| `components/ColumnItemRow.slint:70` | pic-height | length | `root.pic-width * root.pic-aspect` |  |
| `components/ColumnItemRow.slint:337` | particle-glyph | length | `particle ? 12px : 0px` |  |
| `components/ColumnItemRow.slint:338` | particle-pad | length | `particle ? 4px : 0px` |  |
| `components/ColumnsBlock.slint:25` | layout-width | length | `` |  |
| `components/ColumnsBlock.slint:27` | line-y | length | `` | the editor row's own geometry, for the slash menu an item can open |
| `components/ColumnsBlock.slint:28` | line-x | length | `` |  |
| `components/ColumnsBlock.slint:37` | pointer-item | int | `-1` | would read false on every item-to-item move; each item claims by id. |
| `components/ColumnsBlock.slint:43` | boxes | int | `block.column-boxes.length` | which keeps every index below in range without a guard. |
| `components/ColumnsBlock.slint:44` | gap | length | `Theme.spacing-lg` |  |
| `components/ColumnsBlock.slint:46` | each-width | length | `(layout-width - (boxes - 1) * gap) / boxes` | even halves/thirds of what is left after the gaps |
| `components/ColumnsBlock.slint:48` | strip-height | length | `hovered || inner-hover ? 22px : 0px` | the toolbar collapses to nothing (and its buttons stop hit-testing) |
| `components/DatabaseCell.slint:38` | record | int | `` |  |
| `components/DatabaseCell.slint:39` | block | int | `` |  |
| `components/DatabaseCell.slint:71` | pad | length | `Theme.spacing-sm` |  |
| `components/DatabaseFilterPopup.slint:48` | block | int | `UIState.db-filter-block` | the panel's own state, read once per expression |
| `components/DatabasePickList.slint:29` | row-height | length | `30px` | chooser list is one line and tighter. |
| `components/DatabaseSwitcher.slint:25` | block | int | `` |  |
| `components/DatabaseView.slint:53` | content-width | length | `` |  |
| `components/DatabaseView.slint:54` | row-y | length | `` |  |
| `components/DatabaseView.slint:72` | layout | int | `root.block.db-layout-index` | number cannot drift away from a display string because it is not one. |
| `components/DatabaseView.slint:98` | top-in-view | length | `root.row-y - UIState.editor-scroll-y` | *only* piece of layout this slice lets Rust depend on. |
| `components/DatabaseView.slint:107` | body-top | length | `root.block.db-header-height` | zero-height and always clickable. |
| `components/DatabaseView.slint:108` | body-h | length | `Math.max(root.block.db-body-height, 0px)` |  |
| `components/DatabaseView.slint:109` | empty-h | length | `34px` |  |
| `components/DatabaseView.slint:118` | slot-w | length | `22px` | the rows, which is what keeps their columns aligned. |
| `components/DatabaseView.slint:119` | grid-w | length | `Math.max(root.content-width - root.slot-w, 40px)` |  |
| `components/DatabaseView.slint:120` | drag-col | int | `-1` |  |
| `components/DatabaseView.slint:121` | drag-dx | length | `0px` |  |
| `components/DatabaseView.slint:124` | drag-last-x | length | `0px` | 改用 TouchArea.pressed-x 手动求差；只是让编译通过，拖拽语义不变。 |
| `components/DatabaseView.slint:706` | day-w | length | `root.grid-w / Math.max(root.block.db-tl-days, 1)` |  |
| `components/DatabaseView.slint:707` | bar-x | length | `(row.tl-from - root.block.db-tl-start) * self.day-w` |  |
| `components/DatabaseView.slint:1189` | card-w | length | `root.grid-w / Math.max(root.block.db-gallery-per-row, 1)` |  |
| `components/DatabaseView.slint:1413` | chrome-h | length | `30px` | then the plot, then the 24px label strip |
| `components/DatabaseView.slint:1414` | label-h | length | `24px` |  |
| `components/DatabaseView.slint:1415` | plot-w | length | `root.grid-w` |  |
| `components/DatabaseView.slint:1418` | point-count | int | `root.block.db-chart-points.length` |  |
| `components/Editor.slint:19` | depth | int | `` |  |
| `components/Editor.slint:20` | index | int | `` |  |
| `components/Editor.slint:201` | content-x | length | `` |  |
| `components/Editor.slint:202` | content-width | length | `` |  |
| `components/EditorBlock.slint:18` | depth | int | `` |  |
| `components/EditorBlock.slint:19` | index | int | `` |  |
| `components/EditorBlock.slint:20` | row-y | length | `` |  |
| `components/EditorBlock.slint:21` | content-width | length | `` |  |
| `components/EditorBlock.slint:22` | content-x | length | `` |  |
| `components/EditorBlock.slint:25` | list-indent | length | `(is-list ? root.depth * 24px : 0px)` |  |
| `components/EditorBlock.slint:64` | content-id | int | `root.is-synced ? block.sync-source : block.id` | anyway is worse than refusing. |
| `components/EditorBlock.slint:68` | sync-gutter | length | `root.is-synced ? 22px : 0px` | while the glyph is the only part of this row that is its own. |
| `components/EditorBlock.slint:73` | pic-aspect | float | `is-image ? UIState.image-aspect(block.attachment) : 0` | row falls back to naming it. |
| `components/EditorBlock.slint:76` | pic-height | length | `root.pic-width * pic-aspect` |  |
| `components/EditorBlock.slint:88` | embed-inset | length | `Theme.spacing-md + 19px + Theme.spacing-sm` |  |
| `components/EditorBlock.slint:104` | fold-gutter | length | `block.kind == 13 ? 22px : 0px` | has no children yet, so the text does not jump as the section grows |
| `components/EditorBlock.slint:107` | block-text-color | color | `Colors.block-text(block.color)` | block color pair (⋮⋮ Text color / Background color; ADR-0023) |
| `components/EditorBlock.slint:108` | block-bg-color | color | `Colors.block-bg(block.bg)` |  |
| `components/EditorBlock.slint:292` | runs-y | length | `root.is-callout ? top-margin + Theme.spacing-sm : top-margin` |  |
| `components/EditorBlock.slint:325` | particle-glyph | length | `particle ? 12px : 0px` |  |
| `components/EditorBlock.slint:326` | particle-pad | length | `particle ? 4px : 0px` |  |
| `components/EditorBlock.slint:694` | code-width | length | `input-width` | cannot reflow under the caret. |
| `components/EditorBlock.slint:702` | char-advance | length | `UIState.code-advance` | answer — Slint still wraps the text, and every layer shares that. |
| `components/IconButton.slint:9` | size | length | `28px` |  |
| `components/IconPicker.slint:21` | columns | int | `8` | grid's real height rather than a guess that clips the last row. |
| `components/IconPicker.slint:22` | cell | length | `30px` |  |
| `components/NoteCaptureCard.slint:56` | caret | int | `0` |  |
| `components/NoteCaptureCard.slint:68` | radius | length | `Theme.radius-lg` | rounds its corners would paint them with whatever Windows leaves behind. |
| `components/NoteCaptureCard.slint:78` | gutter | length | `16px` |  |
| `components/NoteCaptureCard.slint:79` | body-size | length | `Typography.size-ui` |  |
| `components/NoteCaptureCard.slint:92` | suggest-index | int | `0` | a hidden tray over a `#` still being typed is a tray that lied. |
| `components/NoteCaptureCard.slint:100` | chars | int | `root.draft.character-count` | three bytes a word and the number on screen is a word count. |
| `components/OrgButton.slint:22` | color-slot | int | `-1` |  |
| `components/OrgButton.slint:27` | font-size | length | `Typography.size-ui` |  |
| `components/OrganizerArea.slint:106` | count | int | `-1` |  |
| `components/OrganizerArea.slint:107` | color-slot | int | `-1` |  |
| `components/OrganizerArea.slint:436` | selected | int | `0` |  |
| `components/OrganizerArea.slint:481` | box-size | length | `16px` |  |
| `components/OrganizerArea.slint:531` | tint | color | `Colors.text-secondary` |  |
| `components/OrganizerArea.slint:571` | slot | int | `0` |  |
| `components/OrganizerArea.slint:1015` | gap-h | length | `34px` | a promise, not a preview. |
| `components/OrganizerArea.slint:1101` | my-top | length | `self.absolute-position.y` |  |
| `components/OrganizerArea.slint:1102` | my-mid | length | `my-top + board-card.preferred-height / 2` |  |
| `components/OrganizerArea.slint:1103` | my-foot | length | `my-top + board-card.preferred-height` |  |
| `components/OrganizerArea.slint:1255` | font-size | length | `Typography.size-ui` |  |
| `components/OrganizerArea.slint:1258` | hint-height | length | `28px` | `Rectangle`, and overriding it is an error. |
| `components/OrganizerArea.slint:1268` | field-width | length | `170px` |  |
| `components/OrganizerArea.slint:1356` | selected | int | `0` |  |
| `components/OrganizerArea.slint:1375` | list-x | length | `0px` | child cannot ask for. |
| `components/OrganizerArea.slint:1376` | list-y | length | `0px` |  |
| `components/OrganizerArea.slint:1463` | selected | int | `0` |  |
| `components/OrganizerArea.slint:1629` | note-sort-x | length | `0px` | different sub-trees and neither is the other's parent. |
| `components/OrganizerArea.slint:1630` | note-sort-y | length | `0px` |  |
| `components/QuickNoteWindow.slint:73` | caret | int | `0` |  |
| `components/SettingsDialog.slint:28` | ink | color | `` |  |
| `components/SettingsDialog.slint:87` | font-size | length | `Typography.size-ui` |  |
| `components/SettingsDialog.slint:88` | ink | color | `Colors.text-primary` |  |
| `components/SettingsDialog.slint:287` | selected | int | `` |  |
| `components/Sidebar.slint:19` | node-id | int | `-1` | for. Negative, like every other row that is not a page. |
| `components/Sidebar.slint:248` | grab-offset | length | `` |  |
| `components/SidebarItem.slint:12` | index | int | `` |  |
| `components/SyncPage.slint:55` | font-size | length | `Typography.size-caption` |  |
| `components/SyncPage.slint:108` | selected | int | `0` |  |
| `components/TableBlock.slint:24` | grid-width | length | `` |  |
| `components/TableBlock.slint:29` | anchor-y | length | `0px` | popup a cell is allowed to raise. |
| `components/TableBlock.slint:30` | anchor-x | length | `0px` |  |
| `components/TableBlock.slint:42` | pointer-cell | int | `-1` | its own id, and only lets go when nobody has claimed it since. |
| `components/TableBlock.slint:46` | cols | int | `Math.max(block.columns, 1)` |  |
| `components/TableBlock.slint:48` | rows | int | `block.table-cells.length / cols` | Rust projects whole rows only, so this divides exactly |
| `components/TableBlock.slint:49` | pad | length | `Theme.spacing-sm` |  |
| `components/TableBlock.slint:53` | strip-height | length | `hovered || inner-hover ? 22px : 0px` | the toolbar collapses to nothing (and its buttons stop hit-testing) |
| `components/TableBlock.slint:137` | particle-glyph | length | `particle ? 12px : 0px` |  |
| `components/TableBlock.slint:138` | particle-pad | length | `particle ? 4px : 0px` |  |

## 2. 动画台账（62 处 animate 块）

| 位置 | 声明 | duration | easing |
| --- | --- | --- | --- |
| `AppWindow.slint:72` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `AppWindow.slint:175` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `AppWindow.slint:267` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/BlockHandle.slint:20` | `animate opacity { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/Button.slint:29` | `animate background { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/ColumnItemRow.slint:223` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/ContextMenu.slint:57` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/DatabaseGroupPopup.slint:52` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/DatabaseGroupPopup.slint:94` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/DatabasePickList.slint:43` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/DatabaseSwitcher.slint:43` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/EditorBlock.slint:209` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/EditorBlock.slint:623` | `animate background { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/EditorBlock.slint:651` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/EditorBlock.slint:949` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/EditorBlock.slint:1119` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/EditorBlock.slint:1141` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/IconButton.slint:22` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/IconPicker.slint:64` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/IconPicker.slint:105` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/McpPage.slint:39` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/McpPage.slint:71` | `animate background { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/McpPage.slint:79` | `animate x { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/NoteCaptureCard.slint:231` | `animate border-color { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/NoteCaptureCard.slint:240` | `animate border-color { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/NoteCaptureCard.slint:358` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/NoteCaptureCard.slint:449` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/NoteCaptureCard.slint:450` | `animate border-color { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/NoteCaptureCard.slint:453` | `animate opacity { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrgButton.slint:39` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:145` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:388` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:458` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:641` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:642` | `animate border-color { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:881` | `animate opacity { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/OrganizerArea.slint:888` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:889` | `animate border-color { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:1038` | `animate background { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/OrganizerArea.slint:1039` | `animate border-color { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/OrganizerArea.slint:1128` | `animate height, opacity { duration: 150ms; easing: ease-out; }` | 150ms | ease-out |
| `components/OrganizerArea.slint:1135` | `animate height, opacity { duration: 150ms; easing: ease-out; }` | 150ms | ease-out |
| `components/OrganizerArea.slint:1148` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:1300` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:1403` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:1521` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:2080` | `animate border-color { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/OrganizerArea.slint:2346` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/SettingsDialog.slint:239` | `animate background { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/SettingsDialog.slint:253` | `animate x { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/SettingsDialog.slint:304` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/Sidebar.slint:32` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/Sidebar.slint:83` | `animate width { duration: UIState.sidebar-resizing ? 0ms : Theme.duration-normal; }` | UIState.sidebar-resizing ? 0ms : Theme.duration-normal |  |
| `components/SidebarItem.slint:53` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/SyncPage.slint:79` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/SyncPage.slint:127` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/SyncPage.slint:208` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/SyncPage.slint:595` | `animate background { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/SyncPage.slint:603` | `animate x { duration: Theme.duration-fast; }` | Theme.duration-fast |  |
| `components/TableEdge.slint:26` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/TopBar.slint:19` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |
| `components/VersionPanel.slint:227` | `animate background { duration: Theme.duration-instant; }` | Theme.duration-instant |  |

## 3. 8 位色值字面量（RRGGBBAA，共 8 个不同值）

| 值 | 出现次数 |
| --- | --- |
| `#0000009e` | 1 |
| `#ffffff14` | 1 |
| `#0000000a` | 1 |
| `#ffffff20` | 1 |
| `#00000014` | 1 |
| `#000000a0` | 1 |
| `#00000026` | 1 |
| `#00000059` | 1 |

## 4. font-size 字面量（共 5 档）

| px | 出现次数 |
| --- | --- |
| 42 | 1 |
| 22 | 1 |
| 12 | 1 |
| 11 | 2 |
| 9 | 2 |
