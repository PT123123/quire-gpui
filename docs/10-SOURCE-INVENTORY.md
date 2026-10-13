# Reference source inventory

Generated from the frozen working tree. Line numbers refer to reference/slint-desktop, not a future implementation.

Git HEAD: 47ef6f81d7f1ffb688e1349f459af7e6dc8eb09b

## UI files and declared components

### ui/AppWindow.slint (1132 lines)

- line 44: component BlockMenuPopup inherits PopupWindow {
- line 132: component SlashMenuPopup inherits PopupWindow {
- line 229: component DatabaseColumnsPopup inherits PopupWindow {
- line 422: component LinkDialog inherits PopupWindow {
- line 526: export component AppWindow inherits Window {

### ui/Colors.slint (263 lines)

- line 18: export struct ThemePalette {
- line 37: export global Colors {

### ui/Icons.slint (758 lines)

- line 8: export struct IconCtx {
- line 12: component IcSearch {
- line 29: component IcChevronRight {
- line 45: component IcChevronDown {
- line 61: component IcPlus {
- line 75: component IcMinus {
- line 88: component IcX {
- line 102: component IcGrip {
- line 122: component IcDotsHorizontal {
- line 137: component IcSettings {
- line 153: component IcSun {
- line 176: component IcMoon {
- line 192: component IcPanelLeft {
- line 208: component IcPage {
- line 225: component IcStar {
- line 247: component IcNote {
- line 266: component IcClock {
- line 284: component IcMinimize {
- line 297: component IcMaximize {
- line 311: component IcRestore {
- line 330: component IcTodoCheck {
- line 344: component IcExport {
- line 360: component IcImport {
- line 376: component IcChevronLeft {
- line 390: component IcPencil {
- line 406: component IcCopy {
- line 421: component IcTrash {
- line 439: component IcChevronUp {
- line 456: component IcLink {
- line 476: component IcArrowLeft {
- line 491: component IcArrowRight {
- line 506: component IcPalette {
- line 525: component IcImage {
- line 558: component IcSmile {
- line 596: component IcLock {
- line 623: component IcSend {
- line 652: component IcRefresh {
- line 692: component IcSync {
- line 712: export component Icon {

### ui/Theme.slint (55 lines)

- line 7: export global Theme {

### ui/TrayIcon.slint (44 lines)

- line 15: export component AppTray inherits SystemTrayIcon {

### ui/Types.slint (1775 lines)

- line 4: export struct SidebarNode {
- line 19: export struct TextRun {
- line 47: export struct TableCell {
- line 57: export struct ColumnItem {
- line 76: export struct ColumnBox {
- line 85: export struct TocEntry {
- line 95: export struct BacklinkRow {
- line 110: export struct DbCell {
- line 118: export struct DbRow {
- line 142: export struct DbOption {
- line 148: export struct DbColumn {
- line 157: export struct DbViewTab {
- line 173: export struct DbBoardColumn {
- line 183: export struct DbCalendarDay {
- line 192: export struct DbFormField {
- line 207: export struct DbChartPoint {
- line 215: export struct BlockRow {
- line 316: export struct DbColumnToggle {
- line 336: export struct DbPickRow {
- line 348: export struct DbFilterRow {
- line 361: export struct DbFilterOp {
- line 366: export struct CommandRow {
- line 384: export struct SyncRow {
- line 411: export struct SyncStat {
- line 421: export struct SyncLogRow {
- line 432: export struct McpLogRow {
- line 439: export struct SearchRow {
- line 446: export struct SlashRow {
- line 453: export struct MenuRow {
- line 474: export struct VersionRow {
- line 485: export struct DiffRow {
- line 500: export struct SubtaskRow {
- line 509: export struct NoteRow {
- line 534: export struct NoteDetails {
- line 548: export struct TaskListRow {
- line 560: export struct TagRow {
- line 573: export struct TaskRow {
- line 601: export struct BoardCard {
- line 617: export struct BoardColumn {
- line 627: export global UIState {

### ui/Typography.slint (98 lines)

- line 6: export global Typography {
- line 60: export global PageType {

### ui/components/AppShell.slint (173 lines)

- line 17: export component AppShell inherits Rectangle {

### ui/components/BacklinkPanel.slint (175 lines)

- line 19: export component BacklinkPanel inherits Rectangle {

### ui/components/BlockHandle.slint (70 lines)

- line 9: export component BlockHandle inherits Rectangle {

### ui/components/Button.slint (58 lines)

- line 9: export component Button inherits Rectangle {

### ui/components/ColumnItemRow.slint (633 lines)

- line 16: export component ColumnItemRow inherits Rectangle {

### ui/components/ColumnsBlock.slint (156 lines)

- line 23: export component ColumnsBlock inherits Rectangle {

### ui/components/CommandPalette.slint (204 lines)

- line 11: export component CommandPalette inherits PopupWindow {

### ui/components/ContextMenu.slint (126 lines)

- line 15: export component ContextMenu inherits PopupWindow {

### ui/components/DatabaseCell.slint (294 lines)

- line 35: export component DatabaseCell inherits Rectangle {

### ui/components/DatabaseFilterPopup.slint (538 lines)

- line 33: export component DatabaseFilterPopup inherits PopupWindow {

### ui/components/DatabaseFormulaPopup.slint (216 lines)

- line 35: export component DatabaseFormulaPopup inherits PopupWindow {

### ui/components/DatabaseGroupPopup.slint (139 lines)

- line 20: export component DatabaseGroupPopup inherits PopupWindow {

### ui/components/DatabasePickList.slint (100 lines)

- line 25: export component DatabasePickList inherits Rectangle {

### ui/components/DatabaseRelationPopup.slint (273 lines)

- line 35: export component DatabaseRelationPopup inherits PopupWindow {

### ui/components/DatabaseRollupPopup.slint (236 lines)

- line 29: export component DatabaseRollupPopup inherits PopupWindow {

### ui/components/DatabaseSwitcher.slint (172 lines)

- line 23: export component DatabaseSwitcher inherits Rectangle {

### ui/components/DatabaseView.slint (1634 lines)

- line 51: export component DatabaseView inherits Rectangle {

### ui/components/Dialog.slint (76 lines)

- line 10: export component ConfirmDialog inherits PopupWindow {

### ui/components/Editor.slint (466 lines)

- line 17: component DocumentRow inherits Rectangle {
- line 205: export component Editor inherits Rectangle {

### ui/components/EditorBlock.slint (1223 lines)

- line 16: export component EditorBlock inherits Rectangle {

### ui/components/IconButton.slint (39 lines)

- line 7: export component IconButton inherits Rectangle {

### ui/components/IconPicker.slint (126 lines)

- line 17: export component IconPicker inherits PopupWindow {

### ui/components/McpPage.slint (361 lines)

- line 28: component McpTextButton inherits Rectangle {
- line 63: component McpSwitch inherits Rectangle {
- line 92: component ConfigBox inherits Rectangle {
- line 120: component McpSectionLabel inherits Text {
- line 134: component McpLogLine inherits Rectangle {
- line 182: export component McpPage inherits Rectangle {

### ui/components/NoteCaptureCard.slint (473 lines)

- line 38: export component NoteCaptureCard inherits Rectangle {

### ui/components/OrganizerArea.slint (3346 lines)

- line 63: component OrgField inherits Text {
- line 76: component OrgDetailRow inherits HorizontalLayout {
- line 104: component OrgNavRow inherits Rectangle {
- line 290: component OrgTagColumn inherits VerticalLayout {
- line 358: component OrgFilterChip inherits Rectangle {
- line 435: component OrgSeg inherits Rectangle {
- line 479: component OrgCheck inherits Rectangle {
- line 509: component OrgMeta inherits Text {
- line 529: component OrgPill inherits Rectangle {
- line 570: component OrgPrio inherits Text {
- line 585: component OrgDue inherits Rectangle {
- line 622: component OrgTaskRow inherits Rectangle {
- line 797: component OrgQuickAdd inherits Rectangle {
- line 870: component OrgBoardCard inherits Rectangle {
- line 979: component OrgBoardGap inherits Rectangle {
- line 995: component OrgBoardColumn inherits Rectangle {
- line 1252: component OrgInput inherits Rectangle {
- line 1354: component OrgSortBox inherits Rectangle {
- line 1461: component OrgSortList inherits Rectangle {
- line 1558: component OrgEmptyState inherits VerticalLayout {
- line 1592: export component OrganizerArea inherits Rectangle {

### ui/components/OrgButton.slint (77 lines)

- line 14: export component OrgButton inherits Rectangle {

### ui/components/PageTree.slint (19 lines)

- line 10: export component PageTree inherits ListView {

### ui/components/QuickNoteWindow.slint (112 lines)

- line 35: export component QuickNoteWindow inherits Window {

### ui/components/SearchPanel.slint (219 lines)

- line 11: export component SearchPanel inherits PopupWindow {

### ui/components/SettingsDialog.slint (789 lines)

- line 25: component ThemeCard inherits Rectangle {
- line 85: component CopyableText inherits Rectangle {
- line 125: component ShortcutRow inherits Rectangle {
- line 152: component ZoomRow inherits VerticalLayout {
- line 216: component ToggleRow inherits HorizontalLayout {
- line 262: component SettingsSection inherits VerticalLayout {
- line 286: component SettingsTabs inherits Rectangle {
- line 323: export component SettingsDialog inherits PopupWindow {

### ui/components/Sidebar.slint (268 lines)

- line 12: component SidebarPinnedRow inherits Rectangle {
- line 77: export component Sidebar inherits Rectangle {

### ui/components/SidebarItem.slint (247 lines)

- line 10: export component SidebarItem inherits Rectangle {

### ui/components/SyncPage.slint (728 lines)

- line 49: component SyncButton inherits Rectangle {
- line 107: component SyncTabs inherits Rectangle {
- line 149: component SyncHeading inherits Text {
- line 163: component SyncSectionLabel inherits Text {
- line 179: component SyncOnlineDot inherits Rectangle {
- line 197: component SyncDeviceRow inherits Rectangle {
- line 273: component SyncStatRow inherits Rectangle {
- line 304: component SyncLogLine inherits Rectangle {
- line 362: export component SyncPage inherits Rectangle {

### ui/components/TableBlock.slint (361 lines)

- line 22: export component TableBlock inherits Rectangle {

### ui/components/TableEdge.slint (54 lines)

- line 11: export component TableEdge inherits Rectangle {

### ui/components/TopBar.slint (174 lines)

- line 11: component WindowControlButton inherits Rectangle {
- line 37: export component TopBar inherits Rectangle {

### ui/components/VersionPanel.slint (388 lines)

- line 29: export component VersionPanel inherits PopupWindow {

## UI callbacks

- ui/Types.slint:329: // sentence; `id` is whatever that list's callback names back (a kind index, a
- ui/Types.slint:646: callback sidebar-section-toggled(int);
- ui/Types.slint:704: callback toggle-lan-sharing();
- ui/Types.slint:707: callback toggle-mcp();
- ui/Types.slint:722: callback mcp-copy(string);
- ui/Types.slint:724: callback mcp-restart();
- ui/Types.slint:726: callback mcp-log-cleared();
- ui/Types.slint:756: callback sync-now(string);       // peer id: one pull-merge-push now
- ui/Types.slint:760: callback sync-now-all();
- ui/Types.slint:761: callback sync-forget(string);    // peer id: drop it from the table
- ui/Types.slint:768: callback sync-forget-offline();
- ui/Types.slint:769: callback sync-auto-toggled(bool);
- ui/Types.slint:784: callback sync-name-set(string);
- ui/Types.slint:785: callback sync-interval-set(int);
- ui/Types.slint:787: // re-read of the peers table (the ⟳ button). The second one is a callback
- ui/Types.slint:792: callback sync-log-cleared();
- ui/Types.slint:793: callback sync-refresh-requested();
- ui/Types.slint:961: callback version-save();
- ui/Types.slint:962: callback version-view(int);
- ui/Types.slint:963: callback version-restore(int);
- ui/Types.slint:964: callback version-delete(int);
- ui/Types.slint:967: callback version-back();
- ui/Types.slint:968: callback versions-close();
- ui/Types.slint:988: pure callback settings-match(string) -> bool;
- ui/Types.slint:1001: callback toggle-sidebar();
- ui/Types.slint:1004: callback sidebar-resized(float);
- ui/Types.slint:1005: callback toggle-theme();
- ui/Types.slint:1006: callback set-theme(string);
- ui/Types.slint:1012: callback zoom-in();
- ui/Types.slint:1013: callback zoom-out();
- ui/Types.slint:1014: callback zoom-reset();
- ui/Types.slint:1023: callback window-resized();
- ui/Types.slint:1025: // the callback is for Rust, which has one job: the moment the window goes
- ui/Types.slint:1032: callback window-maximized-changed(bool);
- ui/Types.slint:1033: callback move-block(int);
- ui/Types.slint:1036: pure callback block-drag-payload(int) -> data-transfer;
- ui/Types.slint:1037: callback block-drag-hover(data-transfer, int, bool) -> bool;
- ui/Types.slint:1038: callback block-dropped(data-transfer, int, bool);
- ui/Types.slint:1042: callback toggle-mark(int, int, int); // kind(0 bold,1 italic,2 strike,3 code,4 math), start, end
- ui/Types.slint:1043: callback indent-list(int);
- ui/Types.slint:1044: callback outdent-list(int);
- ui/Types.slint:1045: callback duplicate-block(int);
- ui/Types.slint:1046: callback link-apply();
- ui/Types.slint:1047: callback link-remove();
- ui/Types.slint:1048: callback link-cancel();
- ui/Types.slint:1049: callback close-db-notice();
- ui/Types.slint:1050: callback find-start();
- ui/Types.slint:1051: callback find-step(int);
- ui/Types.slint:1052: callback find-close();
- ui/Types.slint:1053: callback title-commit(string);
- ui/Types.slint:1054: callback title-cancel();
- ui/Types.slint:1059: callback lock-nudge();
- ui/Types.slint:1063: callback empty-page-started();
- ui/Types.slint:1067: // fires it; the callback lives here so both shells keep one UIState.
- ui/Types.slint:1068: callback insert-at-end-requested();
- ui/Types.slint:1069: callback open-link(string);
- ui/Types.slint:1070: callback palette-query-changed();
- ui/Types.slint:1071: callback palette-open-requested();
- ui/Types.slint:1072: callback command-invoked();
- ui/Types.slint:1073: callback palette-move(int);
- ui/Types.slint:1074: callback todo-toggled(int);
- ui/Types.slint:1076: callback toggle-fold(int);
- ui/Types.slint:1078: // downsampled raster. A callback and not a model field, so a picture only
- ui/Types.slint:1082: pure callback image-for(int) -> image;
- ui/Types.slint:1086: pure callback image-aspect(int) -> float;
- ui/Types.slint:1091: pure callback attachment-size(int) -> string;
- ui/Types.slint:1093: // turned into the Unicode a row shows. A callback and not a model field
- ui/Types.slint:1096: pure callback math-render(string) -> string;
- ui/Types.slint:1101: pure callback embed-label(string) -> string;
- ui/Types.slint:1102: pure callback embed-url(string) -> string;
- ui/Types.slint:1112: pure callback code-layer(string, string, length, length, int) -> string;
- ui/Types.slint:1118: callback attachment-opened(int);
- ui/Types.slint:1119: callback attachment-saved(int);
- ui/Types.slint:1120: callback block-activate(int);
- ui/Types.slint:1124: callback toc-jump(int);
- ui/Types.slint:1130: callback table-cell-move(int, int); // cell id, delta (+1 / -1)
- ui/Types.slint:1131: callback cell-changed(); // a container item's live text moved: re-arm the commit
- ui/Types.slint:1132: callback table-row-added(int);
- ui/Types.slint:1133: callback table-row-removed(int);
- ui/Types.slint:1134: callback table-column-added(int);
- ui/Types.slint:1135: callback table-column-removed(int);
- ui/Types.slint:1140: callback column-item-move(int, int); // item id, delta (+1 / -1)
- ui/Types.slint:1141: callback column-added(int);
- ui/Types.slint:1142: callback column-removed(int);
- ui/Types.slint:1145: callback column-fill(int); // box (Column block) id
- ui/Types.slint:1154: callback db-cell-activated(int, int, int);   // block, record, property: focus the one editor
- ui/Types.slint:1155: callback db-cell-changed();                  // the live cell's text moved: re-arm the commit
- ui/Types.slint:1160: callback db-cell-closed();
- ui/Types.slint:1161: callback db-cell-toggled(int, int, int, bool); // block, record, property, what the cell now shows
- ui/Types.slint:1162: callback db-option-picked(int, int, int, string, string); // block, record, property, option id, current
- ui/Types.slint:1163: callback db-row-added(int);                  // block: append a row
- ui/Types.slint:1164: callback db-row-removed(int, int);           // block, record
- ui/Types.slint:1165: callback db-column-resized(int, int, int);   // block, property, width in permille (0 = auto)
- ui/Types.slint:1166: callback db-columns-opened(int, length, length); // block, viewport-x, viewport-y
- ui/Types.slint:1167: callback db-column-toggled(int, int);        // block, property
- ui/Types.slint:1168: callback db-columns-closed();
- ui/Types.slint:1175: callback db-filter-opened(int, length, length); // block, viewport-x, viewport-y
- ui/Types.slint:1176: callback db-filter-closed();
- ui/Types.slint:1177: callback db-filter-panel-set(int);              // panel state (0 rules · 1 columns · 2 ops · 3 options)
- ui/Types.slint:1178: callback db-filter-match-set(int, bool);        // block, match-any
- ui/Types.slint:1179: callback db-filter-add-rule(int);               // block: switch the panel to the column chooser
- ui/Types.slint:1180: callback db-filter-column-picked(int, int);     // block, property: append an unfilled clause
- ui/Types.slint:1181: callback db-filter-row-removed(int, int);       // block, clause index
- ui/Types.slint:1182: callback db-filter-op-menu(int, int);           // block, clause index: push that kind's comparisons
- ui/Types.slint:1183: callback db-filter-op-picked(int, int, int);    // block, clause index, op
- ui/Types.slint:1184: callback db-filter-text-set(int, int, string);  // block, clause index, value (Enter)
- ui/Types.slint:1185: callback db-filter-flag-set(int, int, bool);    // block, clause index, checked
- ui/Types.slint:1186: callback db-filter-option-menu(int, int);       // block, clause index: push the column's options
- ui/Types.slint:1187: callback db-filter-option-picked(int, int, string); // block, clause index, option id
- ui/Types.slint:1188: callback db-filter-not-toggled(int, int);       // block, clause index
- ui/Types.slint:1189: callback db-filter-cleared(int);                // block
- ui/Types.slint:1193: callback db-sort-cycled(int, int);              // block, property
- ui/Types.slint:1196: callback db-group-opened(int, length, length);  // block, viewport-x, viewport-y
- ui/Types.slint:1197: callback db-group-closed();
- ui/Types.slint:1198: callback db-group-picked(int, int);             // block, property (-1 = none)
- ui/Types.slint:1201: callback db-view-picked(int, int);           // block, view id
- ui/Types.slint:1205: callback db-viewport(int, length);           // block, top-in-viewport
- ui/Types.slint:1206: callback db-open-record(int, int);           // block, record: open the row's page (D5)
- ui/Types.slint:1211: callback db-view-added(int, int);            // block, layout index
- ui/Types.slint:1213: callback db-cal-month(int, int);             // block, delta (+1 / -1)
- ui/Types.slint:1216: callback db-gallery-shaped(int, int);        // block, cards per row
- ui/Types.slint:1219: callback db-form-text(int, int, string);     // block, property, draft text
- ui/Types.slint:1220: callback db-form-submitted(int);             // block
- ui/Types.slint:1221: callback db-form-cleared(int);               // block
- ui/Types.slint:1227: callback db-search-opened(int);              // block
- ui/Types.slint:1228: callback db-search-text-set(int, string);    // block, needle (per keystroke)
- ui/Types.slint:1229: callback db-search-closed(int);              // block
- ui/Types.slint:1233: callback db-chart-kind(int, int);            // block, ChartKind index
- ui/Types.slint:1236: callback db-template-saved(int, int);        // block, record
- ui/Types.slint:1247: callback db-formula-opened(int, int, int);   // block, record (sample row), property
- ui/Types.slint:1248: callback db-formula-text-changed(string);    // the draft: parse + evaluate per keystroke
- ui/Types.slint:1249: callback db-formula-accepted();              // commit (one change, one undo step)
- ui/Types.slint:1250: callback db-formula-closed();                // dismissed (outside click / Escape)
- ui/Types.slint:1272: callback db-kind-opened(int, int);        // block, property (-1 = "a new column")
- ui/Types.slint:1273: callback db-kind-back();                  // the menu's own "back to the columns"
- ui/Types.slint:1274: callback db-kind-picked(int, int);        // block, PropertyKind index
- ui/Types.slint:1285: callback db-relation-opened(int, int, int); // block, record, property
- ui/Types.slint:1286: callback db-relation-closed();
- ui/Types.slint:1287: callback db-relation-panel-set(int);        // 0 rows · 1 databases · 2 mirrors
- ui/Types.slint:1288: callback db-relation-text-set(string);      // the needle, per keystroke
- ui/Types.slint:1289: callback db-relation-database-picked(int);  // DatabaseId (panel 1)
- ui/Types.slint:1290: callback db-relation-mirror-picked(int);    // PropertyId, -1 = one-way (panel 2)
- ui/Types.slint:1291: callback db-relation-toggled(int);          // RecordId (panel 0)
- ui/Types.slint:1306: callback db-rollup-opened(int, int);        // block, property
- ui/Types.slint:1307: callback db-rollup-closed();
- ui/Types.slint:1308: callback db-rollup-panel-set(int);          // 0 summary · 1 relation · 2 column · 3 aggregate
- ui/Types.slint:1309: callback db-rollup-relation-picked(int);    // PropertyId of this database's relation
- ui/Types.slint:1310: callback db-rollup-column-picked(int);      // PropertyId of the related database
- ui/Types.slint:1311: callback db-rollup-aggregate-picked(int);   // Aggregate index
- ui/Types.slint:1323: callback rich-paste(int) -> bool;
- ui/Types.slint:1327: callback open-data-folder();
- ui/Types.slint:1328: callback backup-now();
- ui/Types.slint:1330: callback reclaim-attachments();
- ui/Types.slint:1334: pure callback page-drag-payload(int) -> data-transfer;
- ui/Types.slint:1335: callback page-drag-hover(data-transfer, int) -> bool;
- ui/Types.slint:1336: callback page-dropped(data-transfer, int);
- ui/Types.slint:1338: callback editing-changed(length, length, length);
- ui/Types.slint:1339: callback enter-at-caret(int);
- ui/Types.slint:1340: callback backspace-at-start();
- ui/Types.slint:1341: callback focus-move(int);
- ui/Types.slint:1342: callback cancel-editing();
- ui/Types.slint:1343: callback undo-requested();
- ui/Types.slint:1344: callback redo-requested();
- ui/Types.slint:1345: callback save-requested();
- ui/Types.slint:1348: callback nav-back-requested();
- ui/Types.slint:1349: callback nav-forward-requested();
- ui/Types.slint:1350: callback slash-move(int);
- ui/Types.slint:1351: callback slash-apply-selected();
- ui/Types.slint:1352: callback slash-close();
- ui/Types.slint:1360: callback mention-typed(string, length, length, length);
- ui/Types.slint:1374: callback backlink-jump(int); // the referring block's id
- ui/Types.slint:1375: callback backlink-toggle(); // fold / unfold the window
- ui/Types.slint:1376: callback block-menu-opened(int, length, length); // id, handle viewport-y, content-x
- ui/Types.slint:1377: callback block-menu-action(int);
- ui/Types.slint:1380: callback block-plus(int, length, length);
- ui/Types.slint:1382: callback node-clicked(int);
- ui/Types.slint:1383: callback node-expand-toggled(int);
- ui/Types.slint:1388: callback node-context(int, length);
- ui/Types.slint:1389: callback page-create-requested();
- ui/Types.slint:1390: callback rename-committed(int, string);
- ui/Types.slint:1391: callback rename-cancelled();
- ui/Types.slint:1392: callback menu-action(int);
- ui/Types.slint:1394: callback icon-picked(string);
- ui/Types.slint:1395: callback search-open-requested();
- ui/Types.slint:1396: callback search-query-changed();
- ui/Types.slint:1397: callback search-move(int);
- ui/Types.slint:1398: callback search-invoked();
- ui/Types.slint:1400: callback search-open-hit(int, int);
- ui/Types.slint:1401: callback dialog-confirmed();
- ui/Types.slint:1402: callback dialog-cancelled();
- ui/Types.slint:1403: callback settings-open-requested();
- ui/Types.slint:1404: callback settings-close-requested();
- ui/Types.slint:1556: callback org-open-requested(int); // 0 笔记, 1 任务
- ui/Types.slint:1557: callback org-close-requested(); // back to the document area
- ui/Types.slint:1563: callback org-note-selected(int);
- ui/Types.slint:1567: callback org-note-closed();
- ui/Types.slint:1568: callback org-note-title-set(int, string);
- ui/Types.slint:1569: callback org-note-body-set(int, string);
- ui/Types.slint:1570: callback org-note-pin-toggled(int);
- ui/Types.slint:1571: callback org-note-tags-set(int, string);
- ui/Types.slint:1572: callback org-note-deleted(int);
- ui/Types.slint:1575: // does. One callback for both doors because they are one question — the ⋯ is
- ui/Types.slint:1577: callback org-note-menu-requested(int, length, length);
- ui/Types.slint:1580: callback org-undo-delete();
- ui/Types.slint:1582: callback org-note-to-task(int);
- ui/Types.slint:1585: callback org-reply-submitted();
- ui/Types.slint:1587: callback org-tag-picked(string);
- ui/Types.slint:1589: // One callback for both directions, because which one it is is decided by the
- ui/Types.slint:1591: callback org-tag-excluded(string);
- ui/Types.slint:1594: callback org-filters-cleared();
- ui/Types.slint:1607: callback org-bin-toggled();
- ui/Types.slint:1610: callback org-bin-restored(int);
- ui/Types.slint:1611: callback org-bin-purged(int);
- ui/Types.slint:1613: callback org-bin-emptied();
- ui/Types.slint:1618: callback org-notes-copy-all();
- ui/Types.slint:1628: callback org-command-opened(int);
- ui/Types.slint:1629: callback org-command-run();
- ui/Types.slint:1633: callback org-command-example();
- ui/Types.slint:1634: callback org-command-closed();
- ui/Types.slint:1654: callback org-capture-opened();
- ui/Types.slint:1655: callback org-capture-sent();
- ui/Types.slint:1656: callback org-capture-closed();
- ui/Types.slint:1660: // `changed` hook fires this callback — whose handler walks the same door the ＋
- ui/Types.slint:1662: callback org-quick-note-requested();
- ui/Types.slint:1670: callback org-capture-draft-changed();
- ui/Types.slint:1673: callback org-tag-suggest-picked(string);
- ui/Types.slint:1679: callback org-auto-input-toggled(bool);
- ui/Types.slint:1692: callback hotkey-alt-n-toggled(bool);
- ui/Types.slint:1693: callback hotkey-alt-m-toggled(bool);
- ui/Types.slint:1695: callback org-task-create();
- ui/Types.slint:1696: callback org-task-selected(int);
- ui/Types.slint:1697: callback org-task-title-set(int, string);
- ui/Types.slint:1698: callback org-task-notes-set(int, string);
- ui/Types.slint:1699: callback org-task-done-toggled(int);
- ui/Types.slint:1700: callback org-task-priority-set(int, int);
- ui/Types.slint:1701: callback org-task-due-set(int, string);
- ui/Types.slint:1702: callback org-task-repeat-set(int, int);
- ui/Types.slint:1703: callback org-task-tags-set(int, string);
- ui/Types.slint:1704: callback org-task-list-set(int, int); // task, list id (-1 = 收集箱)
- ui/Types.slint:1705: callback org-task-deleted(int);
- ui/Types.slint:1708: callback org-task-menu-requested(int, length, length);
- ui/Types.slint:1712: // not need its own callback: the menu action carries the id.
- ui/Types.slint:1713: callback org-select-entered(int);
- ui/Types.slint:1715: callback org-select-stopped();
- ui/Types.slint:1716: // 全选 / 取消全选 — one callback, because which of the two it is is decided by
- ui/Types.slint:1718: callback org-select-all-toggled();
- ui/Types.slint:1720: callback org-selection-verbed(int);
- ui/Types.slint:1723: callback org-quick-add-submitted();
- ui/Types.slint:1726: callback org-task-moved(int, int);
- ui/Types.slint:1727: callback org-task-add-to(int, string);
- ui/Types.slint:1730: pure callback org-card-payload(int) -> data-transfer;
- ui/Types.slint:1731: pure callback org-card-hover(data-transfer, int) -> bool;
- ui/Types.slint:1732: callback org-card-dropped(data-transfer, int);
- ui/Types.slint:1747: callback org-mode-picked(int);
- ui/Types.slint:1748: callback org-show-done-toggled();
- ui/Types.slint:1749: callback org-subtask-added(int);
- ui/Types.slint:1750: callback org-subtask-edit(int, int); // task, subtask: start editing that line's title
- ui/Types.slint:1751: callback org-subtask-title-set(int, int, string);
- ui/Types.slint:1752: callback org-subtask-done-toggled(int, int, bool);
- ui/Types.slint:1753: callback org-subtask-deleted(int, int);
- ui/Types.slint:1757: callback org-list-created();
- ui/Types.slint:1760: callback org-list-menu-requested(int, length, length);
- ui/Types.slint:1762: callback org-list-name-set(int, string);
- ui/Types.slint:1764: callback org-view-picked(int);
- ui/Types.slint:1765: callback org-list-picked(int);
- ui/Types.slint:1766: callback org-sort-picked(int);
- ui/Types.slint:1767: // The 笔记 half's sort, the three choices above. Its own callback because it
- ui/Types.slint:1770: callback org-note-sort-picked(int);
- ui/Types.slint:1771: callback org-query-changed();
- ui/Types.slint:1773: callback org-field-edited(int);

## Scene routing and seeds (candidate index)

- src/app/controller.rs:9401: pub fn apply_scene(ui: &AppWindow, state: &Rc<AppState>, scene: &str) {
- src/app/controller.rs:9441: "sidebar-toggle" => {
- src/app/controller.rs:9450: "theme-fade" => {
- src/app/controller.rs:9459: "mcp-toggle" => {
- src/app/controller.rs:9480: "board-drag" => {
- src/app/controller.rs:9643: fn apply_scene_body(ui: &AppWindow, state: &Rc<AppState>, scene: &str) {
- src/app/controller.rs:9646: "dark" => g.set_theme("midnight".into()),
- src/app/controller.rs:9721: "sync" | "sync-stats" | "sync-log" => {
- src/app/controller.rs:9801: "tasks-list-menu" | "tasks-list-color" => {
- src/app/controller.rs:9813: "tasks-list-rename" => {
- src/app/controller.rs:9823: "undo-bar" => {
- src/app/controller.rs:9841: "bin" | "tasks-bin" => {
- src/app/controller.rs:9859: "notes-menu" => {
- src/app/controller.rs:9872: "notes-capture" | "notes-capture-empty" | "notes-capture-suggest" => {
- src/app/controller.rs:9895: "notes-select" | "tasks-select" => {
- src/app/controller.rs:9926: "notes-filter" => {
- src/app/controller.rs:9950: "notes-commands" => {
- src/app/controller.rs:9965: "dark-notes-filter" => {
- src/app/controller.rs:9968: "dark-notes-commands" => {
- src/app/controller.rs:9971: "dark-notes-select" => {
- src/app/controller.rs:9974: "dark-tasks-select" => {
- src/app/controller.rs:9977: "dark-notes" => {
- src/app/controller.rs:9980: "dark-notes-reply" => {
- src/app/controller.rs:9983: "dark-notes-menu" => {
- src/app/controller.rs:9986: "dark-notes-capture" => {
- src/app/controller.rs:9989: "dark-notes-capture-empty" => {
- src/app/controller.rs:9992: "dark-notes-capture-suggest" => {
- src/app/controller.rs:10000: "dark-notes-sort" => {
- src/app/controller.rs:10003: "dark-notes-empty" => {
- src/app/controller.rs:10009: "dark-sync" => {
- src/app/controller.rs:10012: "dark-sync-stats" => {
- src/app/controller.rs:10015: "dark-sync-log" => {
- src/app/controller.rs:10018: "dark-tasks-list-menu" => {
- src/app/controller.rs:10021: "dark-tasks-list-color" => {
- src/app/controller.rs:10024: "dark-bin" => {
- src/app/controller.rs:10027: "dark-tasks-bin" => {
- src/app/controller.rs:10030: "dark-tasks" => {
- src/app/controller.rs:10033: "dark-notes-detail" => {
- src/app/controller.rs:10036: "dark-notes-info" => {
- src/app/controller.rs:10039: "dark-tasks-detail" => {
- src/app/controller.rs:10042: "dark-tasks-board" => {
- src/app/controller.rs:10045: "dark-undo-bar" => {
- src/app/controller.rs:10048: "palette" | "search" | "search-notes" | "menu" | "dialog" | "settings" => {
- src/app/controller.rs:10051: "slash" => {
- src/app/controller.rs:10066: "block-menu" => {}
- src/app/controller.rs:10067: "link" => apply_scene_overlay(ui, state, "link-dlg"),
- src/app/controller.rs:10068: "dark-slash" => {
- src/app/controller.rs:10071: "dark-find" => {
- src/app/controller.rs:10074: "dark-marks" => {
- src/app/controller.rs:10077: "dark-code-hl" => {
- src/app/controller.rs:10080: "dark-link" => {
- src/app/controller.rs:10084: "dark-block-menu" => {
- src/app/controller.rs:10088: "dark-block-colors" => {
- src/app/controller.rs:10091: "dark-synced" => {
- src/app/controller.rs:10094: "dark-synced-source-gone" => {
- src/app/controller.rs:10097: "dark-move-to-tall" => {
- src/app/controller.rs:10101: "dark-title-edit" => {
- src/app/controller.rs:10104: "dark-mention" => {
- src/app/controller.rs:10107: "dark-date" => {
- src/app/controller.rs:10110: "dark-backlinks" => {
- src/app/controller.rs:10113: "dark-dangling" => {
- src/app/controller.rs:10116: "dark-database-table" => {
- src/app/controller.rs:10119: "dark-database-filter" => {
- src/app/controller.rs:10122: "dark-database-formula" => {
- src/app/controller.rs:10125: "dark-database-board" => {
- src/app/controller.rs:10128: "dark-database-list" => {
- src/app/controller.rs:10131: "dark-database-calendar" => {
- src/app/controller.rs:10134: "dark-database-gallery" => {
- src/app/controller.rs:10137: "dark-database-timeline" => {
- src/app/controller.rs:10140: "dark-database-form" => {
- src/app/controller.rs:10143: "dark-database-chart" => {
- src/app/controller.rs:10146: "dark-database-search" => {
- src/app/controller.rs:10149: "dark-database-linked" => {
- src/app/controller.rs:10152: "dark-database-template" => {
- src/app/controller.rs:10158: "dark-database-kinds" => {
- src/app/controller.rs:10161: "dark-database-relation" => {
- src/app/controller.rs:10164: "dark-database-rollup" => {
- src/app/controller.rs:10167: "title-edit" => {
- src/app/controller.rs:10171: "recovered" => {
- src/app/controller.rs:10180: "page-block" => {
- src/app/controller.rs:10203: "link-block" => {
- src/app/controller.rs:10231: "rename" => {
- src/app/controller.rs:10241: "mention" | "date" => {
- src/app/controller.rs:10269: "dangling" => {
- src/app/controller.rs:10284: "backlinks" | "backlinks-open" => {
- src/app/controller.rs:10289: "backlinks-small" => {
- src/app/controller.rs:10295: "database-table" => {
- src/app/controller.rs:10304: "database-filter" => {
- src/app/controller.rs:10317: "database-formula" => {
- src/app/controller.rs:10326: "database-board" => {
- src/app/controller.rs:10329: "database-list" => {
- src/app/controller.rs:10332: "database-calendar" => {
- src/app/controller.rs:10335: "database-gallery" => {
- src/app/controller.rs:10338: "database-timeline" => {
- src/app/controller.rs:10341: "database-form" => {
- src/app/controller.rs:10349: "database-chart" => {
- src/app/controller.rs:10358: "database-search" => {
- src/app/controller.rs:10369: "database-linked" => {
- src/app/controller.rs:10377: "database-template" => {
- src/app/controller.rs:10387: "database-kinds" => {
- src/app/controller.rs:10390: "database-relation" => {
- src/app/controller.rs:10393: "database-rollup" => {
- src/app/controller.rs:10400: "synced" => {
- src/app/controller.rs:10403: "synced-source-gone" => {
- src/app/controller.rs:10407: "empty" => open(&g, state, 113),
- src/app/controller.rs:10408: "nest" => {
- src/app/controller.rs:10426: "toggle" | "toggle-fold" => {
- src/app/controller.rs:10454: "image" | "image-half" => {
- src/app/controller.rs:10487: "file" => {
- src/app/controller.rs:10513: "table" | "table-edit" | "table-marks" => {
- src/app/controller.rs:10578: "columns" | "columns-3" | "columns-marks" => {
- src/app/controller.rs:10666: "math" => {
- src/app/controller.rs:10688: "math-inline" => {
- src/app/controller.rs:10723: "toc" => {
- src/app/controller.rs:10743: "embed" => {
- src/app/controller.rs:10766: "embed-empty" => {
- src/app/controller.rs:10787: "code-hl" => {
- src/app/controller.rs:10811: "find" => {
- src/app/controller.rs:10828: "find-grid" | "find-cols" => {
- src/app/controller.rs:10843: "find-callout" => {
- src/app/controller.rs:10869: "style-serif" | "style-mono" | "style-small" | "style-full" | "style-tight" => {
- src/app/controller.rs:10872: "style-serif" => state.set_page_font(page, crate::core::PageFont::Serif),
- src/app/controller.rs:10873: "style-mono" => state.set_page_font(page, crate::core::PageFont::Mono),
- src/app/controller.rs:10874: "style-small" => state.toggle_page_small_text(page),
- src/app/controller.rs:10875: "style-full" => state.toggle_page_full_width(page),
- src/app/controller.rs:10885: "dark-style-serif" => {
- src/app/controller.rs:10892: "page-icon" => {
- src/app/controller.rs:10895: "dark-page-icon" => {
- src/app/controller.rs:10904: "page-cover" | "page-cover-white" | "page-cover-icon" => {
- src/app/controller.rs:10917: "dark-page-cover" => {
- src/app/controller.rs:10920: "dark-page-cover-white" => {
- src/app/controller.rs:10930: "page-lock" => {
- src/app/controller.rs:10933: "page-lock-menu" => {
- src/app/controller.rs:10942: "page-lock-block-menu" => {
- src/app/controller.rs:10958: "dark-page-lock" => {
- src/app/controller.rs:10966: "page-templates" => {
- src/app/controller.rs:10979: "page-template-pick" => {
- src/app/controller.rs:10992: "slash-template" => {
- src/app/controller.rs:11000: "dark-page-templates" => {
- src/app/controller.rs:11005: "page-versions" => {
- src/app/controller.rs:11008: "page-versions-diff" => {
- src/app/controller.rs:11057: "dark-page-versions" => {
- src/app/controller.rs:11060: "marks" => {
- src/app/controller.rs:11102: "marks-wrap" => {
- src/app/controller.rs:11146: "edit" => {
- src/app/controller.rs:11161: "block-colors" => {
- src/app/controller.rs:11236: pub fn apply_scene_overlay(ui: &AppWindow, state: &Rc<AppState>, scene: &str) {
- src/app/controller.rs:11239: "palette" => g.set_palette_open(true),
- src/app/controller.rs:11242: "palette-nav" => {
- src/app/controller.rs:11247: "search" => {
- src/app/controller.rs:11250: "search-notes" => {
- src/app/controller.rs:11256: "menu" => {
- src/app/controller.rs:11264: "sidebar-menu" => {
- src/app/controller.rs:11276: "tasks-list-menu" => {
- src/app/controller.rs:11286: "notes-menu" => {
- src/app/controller.rs:11298: "tasks-list-color" => {
- src/app/controller.rs:11308: "page-move-to" => {
- src/app/controller.rs:11318: "page-style" => {
- src/app/controller.rs:11330: "page-icon" => {
- src/app/controller.rs:11337: "icon-picker" => {
- src/app/controller.rs:11344: "dialog" => {
- src/app/controller.rs:11350: "settings" => g.set_settings_open(true),
- src/app/controller.rs:11351: "dark-slash" => apply_scene_overlay(ui, state, "slash"),
- src/app/controller.rs:11352: "slash" => {
- src/app/controller.rs:11360: "block-menu" => {
- src/app/controller.rs:11375: "plus" => {
- src/app/controller.rs:11386: "move-to" => {
- src/app/controller.rs:11408: "move-to-tall" => {
- src/app/controller.rs:11429: "text-color" => {
- src/app/controller.rs:11443: "bg-color" => {
- src/app/controller.rs:11457: "link-dlg" => {
- src/app/controller.rs:11468: "database-kinds" => {
- src/app/controller.rs:11473: "database-relation" => {
- src/app/controller.rs:11478: "database-rollup" => {
- src/app/controller.rs:11483: "dark-database-kinds" => apply_scene_overlay(ui, state, "database-kinds"),
- src/app/controller.rs:11484: "dark-database-relation" => apply_scene_overlay(ui, state, "database-relation"),
- src/app/controller.rs:11485: "dark-database-rollup" => apply_scene_overlay(ui, state, "database-rollup"),
- src/app/controller.rs:11488: "dark-tasks-list-menu" => apply_scene_overlay(ui, state, "tasks-list-menu"),
- src/app/controller.rs:11489: "dark-tasks-list-color" => apply_scene_overlay(ui, state, "tasks-list-color"),
- src/app/controller.rs:11490: "dark-notes-menu" => apply_scene_overlay(ui, state, "notes-menu"),

This index is a discovery aid. Inspect full branches, runtime preconditions and controller registration; it is not a completeness proof.
