// 从冻结快照抽取「具名尺寸属性 / 动画 / 颜色字面量 / 字号」四类台账，生成 docs/12-DESIGN-CONSTANTS.md。
// 用法：node tools/extract-design-constants.mjs
import fs from 'fs';
import path from 'path';

const ROOT = path.resolve(import.meta.dirname, '..');
const REF = path.join(ROOT, 'reference', 'slint-desktop', 'ui');
const files = [];
const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).forEach((e) => {
  const p = path.join(d, e.name);
  if (e.isDirectory()) walk(p); else if (e.name.endsWith('.slint')) files.push(p);
});
walk(REF);
files.sort();

const named = [];
const anims = [];
const colors = new Map();
const fontSizes = new Map();
const commentAt = (lines, i) => {
  for (let k = i - 1; k >= Math.max(0, i - 3); k--) {
    const t = lines[k].trim();
    if (t.startsWith('//')) return t.replace(/^\/\/+\s*/, '');
    if (t) return '';
  }
  return '';
};

for (const f of files) {
  const rel = path.relative(REF, f).replace(/\\/g, '/');
  const lines = fs.readFileSync(f, 'utf8').split(/\r?\n/);
  lines.forEach((s, i) => {
    const at = `${rel}:${i + 1}`;
    let m = s.match(/^\s*(?:in-out|in|out|transient)?\s*property\s*<(length|int|float|color|string|bool)>\s+([a-z0-9-]+)\s*(?::\s*(.+?))?\s*;/);
    if (m && m[1] !== 'bool' && m[1] !== 'string' && m[4] !== 'true') {
      named.push({ at, type: m[1], name: m[2], value: (m[3] || '').trim(), note: commentAt(lines, i) });
    }
    if (/^\s*animate\s+/.test(s)) {
      const body = lines.slice(i, i + 6).join(' ');
      const dur = (body.match(/duration:\s*([^,;}\n]+)/) || [, ''])[1].trim();
      const ease = (body.match(/easing:\s*([^,;}\n]+)/) || [, ''])[1].trim();
      anims.push({ at, decl: s.trim(), dur, ease });
    }
    for (const c of s.matchAll(/#[0-9a-fA-F]{6}[0-9a-fA-F]{2}(?![0-9a-fA-F])/g)) {
      colors.set(c[0].toLowerCase(), (colors.get(c[0].toLowerCase()) || 0) + 1);
    }
    for (const c of s.matchAll(/font-size:\s*([0-9.]+)px/g)) {
      fontSizes.set(c[1], (fontSizes.get(c[1]) || 0) + 1);
    }
  });
}

const byFile = new Map();
for (const n of named) byFile.set(n.at.split(':')[0], [...(byFile.get(n.at.split(':')[0]) || []), n]);

const out = [];
out.push('# 设计常量台账（自动抽取）');
out.push('');
out.push(`生成命令：\`node tools/extract-design-constants.mjs\`。来源：\`reference/slint-desktop/ui\` 共 ${files.length} 个 .slint 文件。`);
out.push('');
out.push('这份表是 docs/03、docs/04 的数值底账：新实现每落一个尺寸/时长，应能在这里找到对应行；');
out.push('这里没有的数值说明它内联在组件里，需要按 `file:line` 回读原文，不要凭印象补。');
out.push('派生表达式（mix/lighten/darken）保留 Slint 原文，移植时要核对比例方向与色彩空间，见 docs/04。');
out.push('');
out.push(`## 1. 具名尺寸/颜色属性（${named.length} 条）`);
out.push('');
out.push('| 文件 | 属性 | 类型 | 默认值 | 邻近注释 |');
out.push('| --- | --- | --- | --- | --- |');
for (const [file, list] of byFile) {
  for (const n of list) out.push(`| \`${n.at}\` | ${n.name} | ${n.type} | \`${n.value}\` | ${n.note.slice(0, 90)} |`);
}
out.push('');
out.push(`## 2. 动画台账（${anims.length} 处 animate 块）`);
out.push('');
out.push('| 位置 | 声明 | duration | easing |');
out.push('| --- | --- | --- | --- |');
for (const a of anims) out.push(`| \`${a.at}\` | \`${a.decl}\` | ${a.dur || '（继承/见下文）'} | ${a.ease || ''} |`);
out.push('');
out.push(`## 3. 8 位色值字面量（RRGGBBAA，共 ${colors.size} 个不同值）`);
out.push('');
out.push('| 值 | 出现次数 |');
out.push('| --- | --- |');
for (const [c, n] of [...colors].sort((a, b) => b[1] - a[1])) out.push(`| \`${c}\` | ${n} |`);
out.push('');
out.push(`## 4. font-size 字面量（共 ${fontSizes.size} 档）`);
out.push('');
out.push('| px | 出现次数 |');
out.push('| --- | --- |');
for (const [c, n] of [...fontSizes].sort((a, b) => parseFloat(b[0]) - parseFloat(a[0]))) out.push(`| ${c} | ${n} |`);
out.push('');

fs.writeFileSync(path.join(ROOT, 'docs', '12-DESIGN-CONSTANTS.md'), out.join('\n'));
console.log(`files=${files.length} named=${named.length} anims=${anims.length} colors=${colors.size} fontSizes=${fontSizes.size}`);
