// 从冻结快照生成 docs/PARITY.csv 与 docs/15-SCENE-CATALOG.md。
// 只读 reference/slint-desktop，不修改旧版；重跑会覆盖这两个输出。
// 手填的实现状态/验证状态/差异/人工语义不要写在本文件输出里，写 docs/PARITY-SEMANTICS.csv（按回调名索引），这里负责合并。
// 用法：node tools/extract-parity.mjs
import fs from 'fs';
import path from 'path';

const ROOT = path.resolve(import.meta.dirname, '..');
const REF = path.join(ROOT, 'reference', 'slint-desktop');
const lines = (f) => fs.readFileSync(f, 'utf8').split(/\r?\n/);

// 1. UIState 的 callback 与 property 声明
const types = lines(path.join(REF, 'ui', 'Types.slint'));
const callbacks = [];
const props = [];
types.forEach((s, i) => {
  const n = i + 1;
  let m = s.match(/^\s*callback\s+([A-Za-z0-9_-]+)\s*\((.*)/);
  if (m) callbacks.push({ name: m[1], args: m[2].replace(/\).*$/, '').trim(), decl: n });
  m = s.match(/^\s*(in-out|in|out)\s+property\s*<([^>]+)>\s*([A-Za-z0-9_-]+)(\s*:\s*(.*?))?\s*;/);
  if (m) props.push({ dir: m[1], type: m[2], name: m[3], dflt: (m[5] || '').trim(), decl: n });
});

// 2. controller.rs 的注册点、处理函数首行注释与其中的 core/state 调用
const ctrl = lines(path.join(REF, 'src', 'app', 'controller.rs'));
const snake = (n) => n.replace(/([a-z0-9])([A-Z])/g, '$1_$2').replace(/-/g, '_').toLowerCase();
const regLine = new Map();
ctrl.forEach((s, i) => {
  const m = s.match(/\.on_([a-z0-9_]+)\s*\(/);
  if (m && !regLine.has(m[1])) regLine.set(m[1], i + 1);
});
const handler = (name) => {
  const key = snake(name);
  const at = regLine.get(key);
  if (!at) return { at: 0, note: '', calls: '', firstStmt: '' };
  let note = '';
  for (let i = at - 2; i >= Math.max(0, at - 6); i--) {
    const t = ctrl[i].trim();
    if (t.startsWith('//')) note = t.replace(/^\/\/+\s*/, '') + (note ? ' ' + note : '');
    else if (t) break;
  }
  const body = ctrl.slice(at - 1, Math.min(ctrl.length, at + 30)).join(' ');
  const firstStmt = (body.match(/\{\s*(?:\/\/[^\n]*\n\s*)*([^\n]{8,120}?);/) || [, ''])[1].trim().replace(/\s+/g, ' ').slice(0, 110);
  const skip = new Set(['clone', 'borrow', 'borrow_mut', 'into', 'into_iter', 'unwrap', 'unwrap_or', 'unwrap_or_default', 'expect', 'map', 'and_then', 'some', 'ok', 'is_empty', 'len']);
  const stateCalls = [...new Set([...body.matchAll(/\bstate\.([a-z_0-9]+)\(/g)].map((m) => m[1]))].filter((x) => !skip.has(x)).map((x) => `state.${x}`);
  const coreCalls = [...new Set([...body.matchAll(/\b(?:quire_core::|core::)([A-Za-z0-9_:]+)/g)].map((m) => m[1]))].map((x) => `core::${x}`);
  const repoCalls = [...new Set([...body.matchAll(/\b(?:repo|repository)\??\.(save[a-z_0-9]*|[a-z_0-9]*commit[a-z_0-9]*|[a-z_0-9]*flush[a-z_0-9]*)\(/g)].map((m) => `repo.${m[1]}`))];
  const calls = [...new Set([...stateCalls, ...coreCalls, ...repoCalls])].slice(0, 8).join(' | ');
  return { at, note, calls, firstStmt };
};

// 3. UI 调用点：入口在哪个组件的哪一行
const uiFiles = [];
const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).forEach((e) => {
  const p = path.join(d, e.name);
  if (e.isDirectory()) walk(p); else if (e.name.endsWith('.slint')) uiFiles.push(p);
});
walk(path.join(REF, 'ui'));
const sites = new Map();
for (const f of uiFiles) {
  const rel = path.relative(REF, f).replace(/\\/g, '/');
  lines(f).forEach((s, i) => {
    for (const m of s.matchAll(/UIState\.([a-z0-9_-]+)\(/g)) {
      if (!sites.has(m[1])) sites.set(m[1], []);
      sites.get(m[1]).push(`${rel}:${i + 1}`);
    }
  });
}

// 4. 区域判定：优先取第一个调用点所在组件
const AREA = [
  [/Editor\.slint|EditorBlock\.slint|BlockHandle\.slint/, '编辑器'],
  [/Database/, '数据库'],
  [/OrganizerArea|ColumnItemRow|OrgButton|NoteCaptureCard/, '整理区'],
  [/QuickNoteWindow/, '速记窗口'],
  [/Sidebar|PageTree|AppShell|TopBar/, '侧栏与导航'],
  [/SettingsDialog/, '设置'],
  [/SyncPage/, '同步'],
  [/McpPage/, 'MCP'],
  [/SearchPanel|CommandPalette/, '搜索与命令面板'],
  [/VersionPanel/, '历史版本'],
  [/IconPicker/, '图标选择'],
  [/BacklinkPanel/, '反向链接'],
  [/TableBlock|ColumnsBlock|TableEdge/, '内容块'],
  [/ContextMenu|Dialog/, '通用弹层'],
  [/AppWindow/, '主窗口与弹层'],
];
const areaOf = (name) => {
  const s = sites.get(name) || [];
  for (const re of [/^/]) void re;
  for (const [re, a] of AREA) if (s.some((x) => re.test(x))) return a;
  if (!s.length) return '未接线';
  return '其它';
};

const csv = (v) => `"${String(v).replace(/"/g, '""')}"`;
const head = ['id', '回调名', '区域', '入口组件与行', '声明', '签名', 'controller注册行', '处理说明', '处理首语句', '调用的core/state', '关联场景', '实现状态', '验证状态', '差异', '人工语义'];
const kw = {
  编辑器: ['edit', 'marks', 'slash', 'block-menu', 'block-colors', 'code-hl', 'find', 'toggle', 'style-', 'nest', 'synced', 'toc', 'embed', 'math', 'table', 'columns', 'image', 'file', 'title-edit', 'link', 'mention', 'backlinks', 'page-icon', 'page-cover', 'page-lock', 'page-templates', 'page-versions', 'dangling', 'recovered', 'page-block'],
  数据库: ['database'],
  整理区: ['notes', 'tasks', 'bin', 'undo-bar'],
  速记窗口: ['quick', 'note-capture'],
  同步: ['sync'],
  MCP: ['mcp'],
  设置: ['settings'],
  侧栏与导航: ['sidebar', 'rename', 'move-to', 'menu'],
  搜索与命令面板: ['palette', 'search'],
  其它: [],
};
// 场景只从 apply_scene / apply_scene_body / apply_scene_overlay 三个函数体内取，
// 否则 controller 里其它 match 分支（主题 id、OS 判定）会被误当成场景。
const fnStart = (re) => ctrl.findIndex((l) => re.test(l)) + 1;
const sBase = fnStart(/^pub fn apply_scene\(/);
const sBody = fnStart(/^fn apply_scene_body\(/);
const sOverlay = fnStart(/^pub fn apply_scene_overlay\(/);
const region = (a, b) => ctrl.slice(a - 1, b - 1);
const grab = (ls) => [...new Set(ls.flatMap((s) => [
  ...[...s.matchAll(/^\s*"([a-z0-9_-]+)"\s*(?:=>|\|)/g)].map((m) => m[1]),
  ...[...s.matchAll(/(?:scene|s)\s*(?:==|\.starts_with\()\s*"([a-z0-9_-]+)"/g)].map((m) => m[1]),
]))];
const overlaySet = new Set(grab(region(sOverlay, ctrl.length)));
const scenes = [...new Set([...grab(region(sBase, sBody)), ...grab(region(sBody, sOverlay)), ...overlaySet])].sort();

const stop = new Set(['set', 'get', 'on', 'ui', 'id', 'to', 'of', 'and', 'the', 'toggle', 'open', 'close', 'copy', 'clear']);
const sceneHits = (name, keys) => {
  const toks = name.split(/[-_]/).filter((t) => t.length >= 3 && !stop.has(t));
  const scored = scenes.map((s) => {
    const st = s.split(/[-_]/);
    return { s, n: toks.filter((t) => st.includes(t)).length };
  }).filter((x) => x.n > 0).sort((a, b) => b.n - a.n || a.s.length - b.s.length);
  const strong = scored.filter((x) => x.n >= 2).slice(0, 6).map((x) => x.s);
  if (strong.length) return strong.join(' ');
  if (scored.length) return scored.slice(0, 4).map((x) => x.s).join(' ');
  return scenes.filter((x) => keys.some((k) => x.includes(k))).slice(0, 4).map((x) => `~${x}`).join(' ');
};

const rows = callbacks.map((c, i) => {
  const h = handler(c.name);
  const s = (sites.get(c.name) || []).slice(0, 4).join(' ; ');
  const a = areaOf(c.name);
  const keys = kw[a] || [];
  const rel = sceneHits(c.name, keys);
  return [
    `F${String(i + 1).padStart(3, '0')}`, c.name, a, s || '（无 UI 调用点）',
    `ui/Types.slint:${c.decl}`, c.args, h.at ? `src/app/controller.rs:${h.at}` : '未注册',
    h.note, h.firstStmt, h.calls, rel, '未开始', '未验证', '', '',
  ];
});

// 人工语义与进度走 sidecar：docs/PARITY.csv 是生成产物，重跑必然覆盖，
// 所以手填的列只存在 docs/PARITY-SEMANTICS.csv（按稳定的「回调名」索引），由这里合并进来。
// sidecar 里留空的格子不覆盖生成值——空即「没人工结论」，不是「人工结论是空」。
const SEM = path.join(ROOT, 'docs', 'PARITY-SEMANTICS.csv');
const split = (l) => {
  const out = [];
  let cur = '', q = false;
  for (let i = 0; i < l.length; i++) {
    const ch = l[i];
    if (q) { if (ch === '"' && l[i + 1] === '"') { cur += '"'; i++; } else if (ch === '"') q = false; else cur += ch; }
    else if (ch === '"') q = true;
    else if (ch === ',') { out.push(cur); cur = ''; }
    else cur += ch;
  }
  out.push(cur);
  return out;
};
let sem = new Map();
if (fs.existsSync(SEM)) {
  const txt = fs.readFileSync(SEM, 'utf8').replace(/^/, '').split(/\r?\n/).filter((l) => l.trim());
  const sh = split(txt[0]).map((x) => x.trim());
  const col = (n) => sh.indexOf(n);
  const at = ['回调名', '实现状态', '验证状态', '差异', '人工语义'].map(col);
  if (at.some((x) => x < 0)) throw new Error(`docs/PARITY-SEMANTICS.csv 缺少列，需要 ${['回调名', '实现状态', '验证状态', '差异', '人工语义'].join('/')}`);
  for (const l of txt.slice(1)) {
    const c = split(l);
    const name = (c[at[0]] || '').trim();
    if (!name) continue;
    sem.set(name, { impl: (c[at[1]] || '').trim(), verify: (c[at[2]] || '').trim(), diff: (c[at[3]] || '').trim(), note: (c[at[4]] || '').trim() });
  }
}
let merged = 0, orphan = 0;
const byName = new Map();
rows.forEach((r) => { if (!byName.has(r[1])) byName.set(r[1], []); byName.get(r[1]).push(r); });
for (const [name, v] of sem) {
  const rs = byName.get(name);
  if (!rs) { orphan++; continue; }
  for (const r of rs) {
    if (v.impl) r[11] = v.impl;
    if (v.verify) r[12] = v.verify;
    if (v.diff) r[13] = v.diff;
    if (v.note) r[14] = v.note;
  }
  merged++;
}

rows.sort((x, y) => (x[2] + x[0]).localeCompare(y[2] + y[0]));
// BOM + CRLF：这两份 CSV 是给 Excel/WPS 里逐行填状态用的，没 BOM 中文会花屏。
const BOM = String.fromCharCode(0xfeff);
const out = BOM + [head, ...rows].map((r) => r.map(csv).join(',')).join('\r\n') + '\r\n';
fs.writeFileSync(path.join(ROOT, 'docs', 'PARITY.csv'), out);

// 属性投影面单独成表：新 UI 每格状态都要有归属
const phead = ['名称', '方向', '类型', '默认值', '声明'];
fs.writeFileSync(
  path.join(ROOT, 'docs', 'PARITY-STATE.csv'),
  BOM + [phead, ...props.map((p) => [p.name, p.dir, p.type, p.dflt, `ui/Types.slint:${p.decl}`].map(csv).join(','))].join('\r\n') + '\r\n',
);

// 场景目录
// 若参考图已采集，用哈希把「渲染结果与默认页完全相同」的场景标出来：
// quire-shot 对无法识别的场景名不报错，只会静默渲染默认页，光看目录发现不了。
const shotHash = new Map();
const manPath = path.join(ROOT, 'reference', 'shots', 'manifest.tsv');
if (fs.existsSync(manPath)) {
  for (const l of fs.readFileSync(manPath, 'utf8').replace(/^/, '').split(/\r?\n/).slice(1)) {
    if (!l.trim()) continue;
    const c = l.split('\t');
    shotHash.set(c[0], c[6]);
  }
}
const noEffect = shotHash.get('t-light__baseline');
const sceneLine = (s) => {
  const at = ctrl.findIndex((l) => new RegExp(`"${s.replace(/-/g, '\\-')}"\\s*(?:=>|\\||==)`).test(l));
  return at >= 0 ? `src/app/controller.rs:${at + 1}` : '';
};
fs.writeFileSync(
  path.join(ROOT, 'docs', '15-SCENE-CATALOG.md'),
  `# 旧版截图场景目录\n\n自动生成自 reference/slint-desktop/src/app/controller.rs 的三个函数体：\n\`apply_scene\`（${sBase}）、\`apply_scene_body\`（${sBody}）、\`apply_scene_overlay\`（${sOverlay}）。\n共 ${scenes.length} 个场景名。「叠加」表示该场景在首帧渲染后还会画弹层，单次 \`--scene\` 采集即可见。\n「与默认页同图」由 ${noEffect ? 'reference/shots/manifest.tsv 的 SHA-256 比对得出' : '未采集参考图，未比对'}：这类名字要么只是 \`--motion\` 规格，要么其静态分支不改变像素，拿它当 \`--scene\` 采不到想要的画面。\n\n| 场景 | 定义位置 | 叠加 | 暗色变体 | 与默认页同图 |\n| --- | --- | --- | --- | --- |\n` +
  scenes.map((s) => `| \`${s}\` | ${sceneLine(s)} | ${overlaySet.has(s) ? '是' : ''} | ${s.startsWith('dark-') ? '本身就是' : scenes.includes(`dark-${s}`) ? `dark-${s}` : ''} | ${noEffect && shotHash.get(`s-${s}`) === noEffect ? '是' : ''} |`).join('\n') + '\n',
);

const unwired = rows.filter((r) => r[3].startsWith('（无')).map((r) => `${r[0]} ${r[1]} (${r[4]})`);
const unreg = rows.filter((r) => r[6] === '未注册').map((r) => `${r[0]} ${r[1]} (${r[4]})`);
fs.writeFileSync(path.join(ROOT, 'docs', 'PARITY-UNWIRED.txt'), [...unwired, '', ...unreg].join('\r\n') + '\r\n');
console.log(`callbacks=${callbacks.length} properties=${props.length} scenes=${scenes.length}`);
console.log(`人工语义 sidecar：${fs.existsSync(SEM) ? `docs/PARITY-SEMANTICS.csv 合并 ${merged} 个回调` + (orphan ? `，${orphan} 条回调名在生成结果里找不到（拼错或已删除）` : '') : 'docs/PARITY-SEMANTICS.csv 不存在，实现/验证列全部为默认值'}`);
console.log(`无 UI 调用点=${unwired}  controller 未注册=${unreg}`);
const byArea = {};
rows.forEach((r) => (byArea[r[2]] = (byArea[r[2]] || 0) + 1));
console.log(Object.entries(byArea).sort((a, b) => b[1] - a[1]).map(([k, v]) => `${k}:${v}`).join('  '));
