# 旧版截图场景目录

自动生成自 reference/slint-desktop/src/app/controller.rs 的三个函数体：
`apply_scene`（9401）、`apply_scene_body`（9643）、`apply_scene_overlay`（11236）。
共 188 个场景名。「叠加」表示该场景在首帧渲染后还会画弹层，单次 `--scene` 采集即可见。
「与默认页同图」由 reference/shots/manifest.tsv 的 SHA-256 比对得出：这类名字要么只是 `--motion` 规格，要么其静态分支不改变像素，拿它当 `--scene` 采不到想要的画面。

| 场景 | 定义位置 | 叠加 | 暗色变体 | 与默认页同图 |
| --- | --- | --- | --- | --- |
| `backlinks` | src/app/controller.rs:10284 |  | dark-backlinks |  |
| `backlinks-open` | src/app/controller.rs:10284 |  |  |  |
| `backlinks-small` | src/app/controller.rs:10289 |  |  |  |
| `bg-color` | src/app/controller.rs:11443 | 是 |  |  |
| `bin` | src/app/controller.rs:9841 |  | dark-bin |  |
| `block-colors` | src/app/controller.rs:11161 |  | dark-block-colors |  |
| `block-menu` | src/app/controller.rs:10066 | 是 | dark-block-menu |  |
| `board-drag` | src/app/controller.rs:9480 |  |  | 是 |
| `code-hl` | src/app/controller.rs:10787 |  | dark-code-hl |  |
| `columns` | src/app/controller.rs:10578 |  |  |  |
| `columns-3` | src/app/controller.rs:10578 |  |  |  |
| `columns-marks` | src/app/controller.rs:10578 |  |  |  |
| `dangling` | src/app/controller.rs:10269 |  | dark-dangling |  |
| `dark` | src/app/controller.rs:9646 |  |  |  |
| `dark-backlinks` | src/app/controller.rs:10110 |  | 本身就是 |  |
| `dark-bin` | src/app/controller.rs:10024 |  | 本身就是 |  |
| `dark-block-colors` | src/app/controller.rs:10088 |  | 本身就是 |  |
| `dark-block-menu` | src/app/controller.rs:10084 |  | 本身就是 |  |
| `dark-code-hl` | src/app/controller.rs:10077 |  | 本身就是 |  |
| `dark-dangling` | src/app/controller.rs:10113 |  | 本身就是 |  |
| `dark-database-board` | src/app/controller.rs:10125 |  | 本身就是 |  |
| `dark-database-calendar` | src/app/controller.rs:10131 |  | 本身就是 |  |
| `dark-database-chart` | src/app/controller.rs:10143 |  | 本身就是 |  |
| `dark-database-filter` | src/app/controller.rs:10119 |  | 本身就是 |  |
| `dark-database-form` | src/app/controller.rs:10140 |  | 本身就是 |  |
| `dark-database-formula` | src/app/controller.rs:10122 |  | 本身就是 |  |
| `dark-database-gallery` | src/app/controller.rs:10134 |  | 本身就是 |  |
| `dark-database-kinds` | src/app/controller.rs:10158 | 是 | 本身就是 |  |
| `dark-database-linked` | src/app/controller.rs:10149 |  | 本身就是 |  |
| `dark-database-list` | src/app/controller.rs:10128 |  | 本身就是 |  |
| `dark-database-relation` | src/app/controller.rs:10161 | 是 | 本身就是 |  |
| `dark-database-rollup` | src/app/controller.rs:10164 | 是 | 本身就是 |  |
| `dark-database-search` | src/app/controller.rs:10146 |  | 本身就是 |  |
| `dark-database-table` | src/app/controller.rs:10116 |  | 本身就是 |  |
| `dark-database-template` | src/app/controller.rs:10152 |  | 本身就是 |  |
| `dark-database-timeline` | src/app/controller.rs:10137 |  | 本身就是 |  |
| `dark-date` | src/app/controller.rs:10107 |  | 本身就是 |  |
| `dark-find` | src/app/controller.rs:10071 |  | 本身就是 |  |
| `dark-link` | src/app/controller.rs:10080 |  | 本身就是 |  |
| `dark-marks` | src/app/controller.rs:10074 |  | 本身就是 |  |
| `dark-mention` | src/app/controller.rs:10104 |  | 本身就是 |  |
| `dark-move-to-tall` | src/app/controller.rs:10097 |  | 本身就是 |  |
| `dark-notes` | src/app/controller.rs:9977 |  | 本身就是 |  |
| `dark-notes-capture` | src/app/controller.rs:9986 |  | 本身就是 |  |
| `dark-notes-capture-empty` | src/app/controller.rs:9989 |  | 本身就是 |  |
| `dark-notes-capture-suggest` | src/app/controller.rs:9992 |  | 本身就是 |  |
| `dark-notes-commands` | src/app/controller.rs:9968 |  | 本身就是 |  |
| `dark-notes-detail` | src/app/controller.rs:10033 |  | 本身就是 |  |
| `dark-notes-empty` | src/app/controller.rs:10003 |  | 本身就是 |  |
| `dark-notes-filter` | src/app/controller.rs:9965 |  | 本身就是 |  |
| `dark-notes-info` | src/app/controller.rs:10036 |  | 本身就是 |  |
| `dark-notes-menu` | src/app/controller.rs:9983 | 是 | 本身就是 |  |
| `dark-notes-reply` | src/app/controller.rs:9980 |  | 本身就是 |  |
| `dark-notes-select` | src/app/controller.rs:9971 |  | 本身就是 |  |
| `dark-notes-sort` | src/app/controller.rs:10000 |  | 本身就是 |  |
| `dark-page-cover` | src/app/controller.rs:10917 |  | 本身就是 |  |
| `dark-page-cover-white` | src/app/controller.rs:10920 |  | 本身就是 |  |
| `dark-page-icon` | src/app/controller.rs:10895 |  | 本身就是 |  |
| `dark-page-lock` | src/app/controller.rs:10958 |  | 本身就是 |  |
| `dark-page-templates` | src/app/controller.rs:11000 |  | 本身就是 |  |
| `dark-page-versions` | src/app/controller.rs:11057 |  | 本身就是 |  |
| `dark-slash` | src/app/controller.rs:10068 | 是 | 本身就是 |  |
| `dark-style-serif` | src/app/controller.rs:10885 |  | 本身就是 |  |
| `dark-sync` | src/app/controller.rs:10009 |  | 本身就是 |  |
| `dark-sync-log` | src/app/controller.rs:10015 |  | 本身就是 |  |
| `dark-sync-stats` | src/app/controller.rs:10012 |  | 本身就是 |  |
| `dark-synced` | src/app/controller.rs:10091 |  | 本身就是 |  |
| `dark-synced-source-gone` | src/app/controller.rs:10094 |  | 本身就是 |  |
| `dark-tasks` | src/app/controller.rs:10030 |  | 本身就是 |  |
| `dark-tasks-bin` | src/app/controller.rs:10027 |  | 本身就是 |  |
| `dark-tasks-board` | src/app/controller.rs:10042 |  | 本身就是 |  |
| `dark-tasks-detail` | src/app/controller.rs:10039 |  | 本身就是 |  |
| `dark-tasks-list-color` | src/app/controller.rs:10021 | 是 | 本身就是 |  |
| `dark-tasks-list-menu` | src/app/controller.rs:10018 | 是 | 本身就是 |  |
| `dark-tasks-select` | src/app/controller.rs:9974 |  | 本身就是 |  |
| `dark-title-edit` | src/app/controller.rs:10101 |  | 本身就是 |  |
| `dark-undo-bar` | src/app/controller.rs:10045 |  | 本身就是 |  |
| `database-board` | src/app/controller.rs:10326 |  | dark-database-board |  |
| `database-calendar` | src/app/controller.rs:10332 |  | dark-database-calendar |  |
| `database-chart` | src/app/controller.rs:10349 |  | dark-database-chart |  |
| `database-filter` | src/app/controller.rs:10304 |  | dark-database-filter |  |
| `database-form` | src/app/controller.rs:10341 |  | dark-database-form |  |
| `database-formula` | src/app/controller.rs:10317 |  | dark-database-formula |  |
| `database-gallery` | src/app/controller.rs:10335 |  | dark-database-gallery |  |
| `database-kinds` | src/app/controller.rs:10387 | 是 | dark-database-kinds |  |
| `database-linked` | src/app/controller.rs:10369 |  | dark-database-linked |  |
| `database-list` | src/app/controller.rs:10329 |  | dark-database-list |  |
| `database-relation` | src/app/controller.rs:10390 | 是 | dark-database-relation |  |
| `database-rollup` | src/app/controller.rs:10393 | 是 | dark-database-rollup |  |
| `database-search` | src/app/controller.rs:10358 |  | dark-database-search |  |
| `database-table` | src/app/controller.rs:10295 |  | dark-database-table |  |
| `database-template` | src/app/controller.rs:10377 |  | dark-database-template |  |
| `database-timeline` | src/app/controller.rs:10338 |  | dark-database-timeline |  |
| `dialog` | src/app/controller.rs:10048 | 是 |  |  |
| `edit` | src/app/controller.rs:11146 |  |  |  |
| `embed` | src/app/controller.rs:10743 |  |  |  |
| `embed-empty` | src/app/controller.rs:10766 |  |  |  |
| `empty` | src/app/controller.rs:10407 |  |  |  |
| `file` | src/app/controller.rs:10487 |  |  |  |
| `find` | src/app/controller.rs:10811 |  | dark-find |  |
| `find-callout` | src/app/controller.rs:10843 |  |  |  |
| `find-grid` | src/app/controller.rs:10828 |  |  |  |
| `icon-picker` | src/app/controller.rs:11337 | 是 |  |  |
| `image` | src/app/controller.rs:10454 |  |  |  |
| `link` | src/app/controller.rs:10067 |  | dark-link |  |
| `link-block` | src/app/controller.rs:10203 |  |  |  |
| `link-dlg` | src/app/controller.rs:11457 | 是 |  |  |
| `marks` | src/app/controller.rs:11060 |  | dark-marks |  |
| `marks-wrap` | src/app/controller.rs:11102 |  |  |  |
| `math` | src/app/controller.rs:10666 |  |  |  |
| `math-inline` | src/app/controller.rs:10688 |  |  |  |
| `mcp-toggle` | src/app/controller.rs:9459 |  |  | 是 |
| `mention` | src/app/controller.rs:10241 |  | dark-mention |  |
| `menu` | src/app/controller.rs:10048 | 是 |  |  |
| `move-to` | src/app/controller.rs:11386 | 是 |  |  |
| `move-to-tall` | src/app/controller.rs:11408 | 是 | dark-move-to-tall |  |
| `nest` | src/app/controller.rs:10408 |  |  |  |
| `notes` | src/app/controller.rs:9653 |  | dark-notes |  |
| `notes-capture` | src/app/controller.rs:9872 |  | dark-notes-capture |  |
| `notes-capture-suggest` | src/app/controller.rs:9872 |  | dark-notes-capture-suggest |  |
| `notes-commands` | src/app/controller.rs:9950 |  | dark-notes-commands |  |
| `notes-detail` | src/app/controller.rs:9653 |  | dark-notes-detail |  |
| `notes-empty` | src/app/controller.rs:9654 |  | dark-notes-empty |  |
| `notes-filter` | src/app/controller.rs:9926 |  | dark-notes-filter |  |
| `notes-info` | src/app/controller.rs:9653 |  | dark-notes-info |  |
| `notes-menu` | src/app/controller.rs:9859 | 是 | dark-notes-menu |  |
| `notes-reply` |  |  | dark-notes-reply |  |
| `notes-search` | src/app/controller.rs:9653 |  |  |  |
| `notes-select` | src/app/controller.rs:9895 |  | dark-notes-select |  |
| `notes-sort` | src/app/controller.rs:9654 |  | dark-notes-sort |  |
| `page-block` | src/app/controller.rs:10180 |  |  |  |
| `page-cover` | src/app/controller.rs:10904 |  | dark-page-cover |  |
| `page-cover-icon` | src/app/controller.rs:10904 |  |  |  |
| `page-cover-white` | src/app/controller.rs:10904 |  | dark-page-cover-white |  |
| `page-icon` | src/app/controller.rs:10892 | 是 | dark-page-icon |  |
| `page-lock` | src/app/controller.rs:10930 |  | dark-page-lock |  |
| `page-lock-block-menu` | src/app/controller.rs:10942 |  |  |  |
| `page-lock-menu` | src/app/controller.rs:10933 |  |  |  |
| `page-move-to` | src/app/controller.rs:11308 | 是 |  |  |
| `page-style` | src/app/controller.rs:11318 | 是 |  |  |
| `page-template-pick` | src/app/controller.rs:10979 |  |  |  |
| `page-templates` | src/app/controller.rs:10966 |  | dark-page-templates |  |
| `page-versions` | src/app/controller.rs:11005 |  | dark-page-versions |  |
| `page-versions-diff` | src/app/controller.rs:11008 |  |  |  |
| `palette` | src/app/controller.rs:10048 | 是 |  |  |
| `palette-nav` | src/app/controller.rs:11242 | 是 |  |  |
| `plus` | src/app/controller.rs:11375 | 是 |  |  |
| `recovered` | src/app/controller.rs:10171 |  |  |  |
| `rename` | src/app/controller.rs:10231 |  |  |  |
| `search` | src/app/controller.rs:10048 | 是 |  |  |
| `search-notes` | src/app/controller.rs:10048 | 是 |  |  |
| `settings` | src/app/controller.rs:10048 | 是 |  |  |
| `sidebar-menu` | src/app/controller.rs:11264 | 是 |  |  |
| `sidebar-toggle` | src/app/controller.rs:9441 |  |  | 是 |
| `slash` | src/app/controller.rs:10051 | 是 | dark-slash |  |
| `slash-template` | src/app/controller.rs:10992 |  |  |  |
| `style-full` | src/app/controller.rs:10869 |  |  |  |
| `style-mono` | src/app/controller.rs:10869 |  |  |  |
| `style-serif` | src/app/controller.rs:10869 |  | dark-style-serif |  |
| `style-small` | src/app/controller.rs:10869 |  |  |  |
| `sync` | src/app/controller.rs:9721 |  | dark-sync |  |
| `sync-log` | src/app/controller.rs:9721 |  | dark-sync-log |  |
| `sync-stats` | src/app/controller.rs:9721 |  | dark-sync-stats |  |
| `synced` | src/app/controller.rs:10400 |  | dark-synced |  |
| `synced-source-gone` | src/app/controller.rs:10403 |  | dark-synced-source-gone |  |
| `table` | src/app/controller.rs:10513 |  |  |  |
| `table-edit` | src/app/controller.rs:10513 |  |  |  |
| `table-marks` | src/app/controller.rs:10513 |  |  |  |
| `tasks` | src/app/controller.rs:9739 |  | dark-tasks |  |
| `tasks-bin` | src/app/controller.rs:9841 |  | dark-tasks-bin |  |
| `tasks-board` |  |  | dark-tasks-board |  |
| `tasks-detail` | src/app/controller.rs:9739 |  | dark-tasks-detail |  |
| `tasks-done` | src/app/controller.rs:9740 |  |  |  |
| `tasks-drag` | src/app/controller.rs:9740 |  |  |  |
| `tasks-drop` | src/app/controller.rs:9740 |  |  |  |
| `tasks-list` | src/app/controller.rs:9739 |  |  |  |
| `tasks-list-color` | src/app/controller.rs:9801 | 是 | dark-tasks-list-color |  |
| `tasks-list-menu` | src/app/controller.rs:9801 | 是 | dark-tasks-list-menu |  |
| `tasks-list-rename` | src/app/controller.rs:9813 |  |  |  |
| `tasks-overdue` | src/app/controller.rs:9739 |  |  |  |
| `tasks-select` | src/app/controller.rs:9895 |  | dark-tasks-select |  |
| `text-color` | src/app/controller.rs:11429 | 是 |  |  |
| `theme-fade` | src/app/controller.rs:9450 |  |  | 是 |
| `title-edit` | src/app/controller.rs:10167 |  | dark-title-edit |  |
| `toc` | src/app/controller.rs:10723 |  |  |  |
| `toggle` | src/app/controller.rs:10426 |  |  |  |
| `toggle-fold` | src/app/controller.rs:10426 |  |  |  |
| `undo-bar` | src/app/controller.rs:9823 |  | dark-undo-bar |  |
