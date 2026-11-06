#!/usr/bin/env node
/**
 * check-docs.mjs —— Curia 2.2 文档自检（纯 node 标准库、零依赖、Node ≥ 18、不联网、不执行文档内代码）。
 * 权威：Curia 2.2 规范「配套脚本规格」与「豁免清单」——编码预检 + 3 组检查（报告组号 0 至 3）、退出码 0/1/2、
 * 目录级与组级豁免、诚实条款（空集必红、根锚点缺失必红、只证明机械子集、不读 git、异常非零）。用法：
 *   node .agents/tools/check-docs.mjs [--strict] [--exclude <路径或 glob>]… [路径…]
 *   报告模式列发现不失败（0）；--strict 任一发现即 1；
 *   2 = 无法完成检查：扫描面为空、路径不存在、根锚点缺失、内部错误——两种模式都不得返回 0。
 * 锚点：工作区根 = 调用时的工作目录，该目录须自身含 `.agents/` 目录——默认调用下不满足即退出码 2，
 *   不自脚本位置、不向父目录上溯（上溯会命中 `~/.agents` 一类无关祖先）；脚本文件位置不限，一律以调用目录为基准。
 *   结构判定以根下 `.agents/` 目录为锚；显式路径的面内无 `.agents/` 子树时结构组不执行，stderr 注明、汇总行自带标注；
 *   结构锚下五个家一个都没有时同样标注（空 `.agents/` 会与完整树同形全绿）。
 * 扫描面：未给路径时取默认两根 `.agents/` 与 `docs/`（`.agents/` 是根锚点，缺失即退出码 2；`docs/` 存在才扫），
 *   默认两根与 --exclude 的相对值以调用目录（即工作区根）为基准；只收 `.md`（大小写不敏感），`.mdx`/`.markdown` 不进面。
 * 取舍：目录名 kebab-case 只查 `.agents/` 子树；日期型文件名只查 plans/research 家与 notes 活动区；
 *   组 0「编码」在文件无法按 UTF-8 解码或含 NUL 字节时报出；纯文件系统遍历，不读 git；
 *   仅 POSIX 实测：Windows 未实测、路径大小写不做归一（绝对 --exclude 按逐字前缀比较）。
 */

import { existsSync, readdirSync, readFileSync, realpathSync, statSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';

/** 工作区根：调用时的工作目录，须自身含 `.agents/` 目录；否则 null（默认调用下即根锚点缺失）。不做上溯——上溯会命中 `~/.agents` 一类无关祖先。 */
const ROOT = (() => { try { return statSync(join(process.cwd(), '.agents')).isDirectory() ? process.cwd() : null; } catch { return null; } })();
const realOr = (p) => { try { return realpathSync(p); } catch { return p; } };

/** 类型枚举（机器读的一份；人读权威在规范「9 类文档」表）：类型 → 家 → 允许状态（null = 无 Status）。 */
const TYPES = {
  proposal: { home: 'plans', statuses: ['draft', 'active', 'done', 'superseded'] },
  spec: { home: 'plans', statuses: ['draft', 'active', 'done', 'superseded'] },
  plan: { home: 'plans', statuses: ['draft', 'active', 'done', 'superseded'] },
  survey: { home: 'research', statuses: ['current', 'historical', 'superseded'] },
  evidence: { home: 'research', statuses: ['current', 'historical', 'superseded'] },
  postmortem: { home: 'research', statuses: ['current', 'historical'] },
  reference: { home: 'research', statuses: ['current', 'historical', 'superseded'] },
  runbook: { home: 'skills', statuses: ['current', 'superseded'] },
  skill: { home: 'skills', statuses: null },
};

const GROUP_NAMES = { 0: '编码', 1: '头块与生命周期', 2: '家与命名', 3: '链接' };
const SKIP_DIR_NAMES = new Set(['.git', 'node_modules']);
const TYPED_HOMES = new Set(['plans', 'research', 'skills']); // 3 行头块的三个类型化家
const HOMES = ['plans', 'research', 'notes', 'skills', 'archive']; // `.agents/` 下五个家——结构组「有没有结构可判」的判据
const NOTE_STATUSES = new Set(['proposed', 'implemented', 'rejected']);
const INDEX_NAMES = new Set(['README.md', 'AGENTS.md']); // 目录说明文件不进类型化面
const MD_RE = /\.[mM][dD]$/, DATE_RE = /^\d{4}-\d{2}-\d{2}$/, DATED_NAME_RE = /^\d{4}-\d{2}-\d{2}-/; // 扩展名大小写不敏感
const KEBAB_RE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/, SCHEME_RE = /^[a-zA-Z][a-zA-Z0-9+.-]*:/, FIELD_RE = /^\s*([A-Za-z][A-Za-z0-9_-]*):\s*(.*)$/;
const NOTE_STATUS_LINE_RE = /^Status:\s*(.*)$/, PLACEHOLDER_RE = /<[^<>\n]+>/;
const EMDASH = '—', INLINE_MARK = '\uE000';
const LINK_RE = /\[[^\]]*\]\(((?:[^()\s]|\([^()\s]*\))+)(?:\s+(?:"[^"]*"|'[^']*'))?\s*\)/g; // 组 3：取链接目标（一层平衡括号，如 foo(1).md；可带 "title" / 'title'）

/** 结构锚（绝对路径）：结构判定以这些 `.agents/` 目录为锚——默认调用为根下 `.agents/`；显式路径为扫描面内找到的 `.agents/` 目录（无则不判结构）。 */
let STRUCT_ANCHORS = [];

/** 输出用路径：两侧统一 realpath（macOS /tmp → /private/tmp 符号链接下输出才不失真）；相对工作区根（= 调用目录），失败回退原值。 */
function relOf(abs) {
  let base = process.cwd(), a = abs;
  try { base = realpathSync(base); } catch {}
  try { a = realpathSync(abs); } catch {}
  const r = relative(base, a).split(sep).join('/');
  return r === '' ? '.' : r;
}

/** 读文件：剥 BOM（否则首行标题匹配失败）；bad = 含 U+FFFD 或 NUL 字节（解码损坏或非文本编码——NUL 覆盖 BOM 缺省的 UTF-16 等，判据不可信）。 */
function readLines(abs) {
  const raw = readFileSync(abs, 'utf8').replace(/^\uFEFF/, '');
  return { lines: raw.replace(/\r\n?/g, '\n').split('\n'), bad: raw.includes('\uFFFD') || raw.includes('\u0000') };
}

/** 极简 glob → RegExp：`**` 跨任意层目录（含零层），`*` 不跨目录（占位符避开替换串里的 `*`）。 */
function globToRe(pattern) {
  const re = pattern.replace(/[\\^$.|?+()[\]{}]/g, '\\$&').replace(/\*\*\//g, '\u0001').replace(/\*\*/g, '\u0002')
    .replace(/\*/g, '[^/]*').replace(/\u0001/g, '(?:.*/)?').replace(/\u0002/g, '.*');
  return new RegExp(`^${re}$`);
}

/** `--exclude` 匹配：绝对路径按前缀；相对值按工作区根（= 调用目录）的相对路径前缀或 glob（与默认两根同一锚点）。 */
function isExcluded(entry, excludes) {
  return excludes.some((e) => {
    if (isAbsolute(e)) {
      const ea = resolve(e);
      return entry.abs === ea || entry.abs.startsWith(ea + sep);
    }
    const norm = e.replace(/^\.\//, '').replace(/\/+$/, '');
    return !!norm && (entry.rel === norm || entry.rel.startsWith(norm + '/') || globToRe(norm).test(entry.rel));
  });
}

const USAGE = '用法：node .agents/tools/check-docs.mjs [--strict] [--exclude <路径或 glob>]… [路径…]';
class UsageError extends Error {}

function parseArgs(argv) {
  const opts = { strict: false, excludes: [], paths: [], help: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i], v = argv[i + 1];
    if (a === '--strict') opts.strict = true;
    else if (a === '--exclude') {
      if (!v) throw new UsageError(`${a} 缺参数`);
      i++;
      // 归一后为空（如 `./`）会静默不排除任何路径——当作用法错误报出，不静默空转
      if (!isAbsolute(v) && v.replace(/^\.\//, '').replace(/\/+$/, '') === '') throw new UsageError(`${a} 的值 \`${v}\` 归一后为空，不会排除任何路径`);
      opts.excludes.push(v);
    } else if (a === '--help' || a === '-h') opts.help = true;
    else if (a.startsWith('--')) throw new UsageError(`未知参数：${a}`);
    else opts.paths.push(a);
  }
  return opts;
}

/** 遍历扫描面收集 `.md` 文件与目录（组 2 目录名用）。目录级豁免：`.git/`、`node_modules/`、`--exclude`、含 `.agents-raw` 的目录；符号链接按真实路径去重。 */
function collect(roots, excludes) {
  const files = [], dirs = [], seen = new Set();
  const rootReals = roots.map((r) => { try { return realpathSync(r); } catch { return null; } }).filter(Boolean);
  const inFace = (real) => rootReals.some((rr) => real === rr || real.startsWith(rr + sep));
  const stack = roots.map((r) => ({ abs: r, rel: relOf(r) }));
  while (stack.length) {
    let cur = stack.pop();
    let st; try { st = statSync(cur.abs); } catch (e) { if (e.code === 'ENOENT' || e.code === 'ELOOP') continue; throw e; } // 悬空/成环符号链接跳过；其余（ENAMETOOLONG、EACCES 等）必须响，不得静默漏检
    let real; try { real = realpathSync(cur.abs); } catch { real = cur.abs; }
    if (!inFace(real) || seen.has(real)) continue;
    seen.add(real);
    if (real !== cur.abs) cur = { abs: real, rel: relOf(real) }; // 符号链接条目按真实路径定位：rel、家判定与相对链接基准同真身
    if (st.isDirectory()) {
      if (SKIP_DIR_NAMES.has(basename(cur.abs)) || existsSync(join(cur.abs, '.agents-raw'))) continue;
      if (isExcluded(cur, excludes)) continue;
      dirs.push(cur);
      for (const ent of readdirSync(cur.abs)) stack.push({ abs: join(cur.abs, ent), rel: cur.rel === '.' ? ent : `${cur.rel}/${ent}` });
    } else if (st.isFile() && MD_RE.test(cur.abs) && !isExcluded(cur, excludes)) files.push(cur);
  }
  return { files, dirs };
}

/** 掩码 fenced 代码与 HTML 注释区域（跨行）；cmask = 注释段移除后的可见文本（组 3 只判它）。 */
function analyze(lines) {
  const n = lines.length, fenced = new Array(n).fill(false), cmask = new Array(n);
  let inFence = false, fenceQuote = false, fenceChar = '', fenceLen = 0, inComment = false;
  for (let i = 0; i < n; i++) {
    const l = lines[i];
    fenced[i] = inFence;
    const fm = inComment ? null : /^\s{0,3}(?:>\s{0,3})*(`{3,}|~{3,})/.exec(l); // 引用块里的围栏也是围栏（`>` 后至多 3 空格）
    if (fm) {
      fenced[i] = true;
      const q = /^\s{0,3}>/.test(l), ch = fm[1][0], len = fm[1].length;
      if (!inFence) { inFence = true; fenceQuote = q; fenceChar = ch; fenceLen = len; }
      else if ((!q || q === fenceQuote) && ch === fenceChar && len >= fenceLen) inFence = false; // 闭合须同字符且不短于开围栏；裸围栏可闭合任意围栏；引用围栏行不闭合裸围栏（裸围栏内的 `> ``` ` 是内容）
    } else if (inFence) fenced[i] = true;
    if (inFence) { cmask[i] = l; continue; } // 围栏内不进组 3；cmask 占位保持数组完整
    let rest = inComment ? l : maskInline(l), out = ''; // 行内代码里的 `<!--` 不是注释起点（注释内容里的反引号不算代码）
    for (;;) {
      const at = inComment ? rest.indexOf('-->') : rest.indexOf('<!--');
      if (at === -1) { if (!inComment) out += rest; break; }
      if (!inComment) out += rest.slice(0, at); // 注释段本身不进判定文本，注释之外的文本照常判定
      rest = rest.slice(at + (inComment ? 3 : 4)), inComment = !inComment;
    }
    cmask[i] = out;
  }
  return { fenced, cmask };
}

/** 头块 = 首个标题后连续的 `键: 值` 行块（至第一个空行或非字段行为止）。组 1 只读它；围栏内标题不参与定位（豁免清单）。 */
function parseHeader(lines, fenced) {
  let titleIdx = -1;
  for (let i = 0; i < lines.length; i++) if (!fenced[i] && /^#{1,6}\s/.test(lines[i])) { titleIdx = i; break; }
  const fields = [];
  let i = titleIdx + 1;
  while (titleIdx !== -1 && i < lines.length && lines[i].trim() === '') i++;
  for (; titleIdx !== -1 && i < lines.length; i++) {
    const m = FIELD_RE.exec(lines[i]);
    if (!m) break;
    fields.push({ key: m[1], value: m[2].trim(), idx: i });
  }
  return { titleIdx, fields };
}

/** 定位文件/目录在结构锚下的位置（家、归档区、笔记 lifecycle）；不在任何 `.agents/` 锚下时 inAgents=false。 */
function locate(abs, isDir = false) {
  let anchor = null;
  for (const a of STRUCT_ANCHORS) {
    if ((abs === a || abs.startsWith(a + sep)) && (anchor === null || a.length > anchor.length)) anchor = a;
  }
  const rel = anchor === null ? null : relative(anchor, abs).split(sep).join('/');
  const after = rel === null || rel === '' ? [] : rel.split('/');
  const home = after.length > (isDir ? 0 : 1) ? after[0] : null;
  return { inAgents: anchor !== null, home, lifecycle: home === 'notes' ? after[1] ?? null : null,
    inArchive: home === 'archive' || (home === 'notes' && after[1] === 'archived'),
    inNotesActive: home === 'notes' && NOTE_STATUSES.has(after[1]) };
}

/** 行内代码掩码：成对反引号（同长引号串）之间的内容替换为 INLINE_MARK；未闭合的按普通文本处理。 */
function maskInline(line) {
  let out = '', i = 0;
  while (i < line.length) {
    if (line[i] === '`') {
      let j = i;
      while (j < line.length && line[j] === '`') j++;
      const close = line.indexOf('`'.repeat(j - i), j);
      if (close !== -1) { out += INLINE_MARK; i = close + (j - i); continue; }
    }
    out += line[i++];
  }
  return out;
}

function add(out, group, rel, line, msg) { out.push({ group, rel, line, msg }); }

/** 组 1：头块——三行必备、顺序固定、取值合法、字段值无 `<占位符>`。 */
function checkHeaderBlock(rel, hdr, out) {
  const anchor = hdr.titleIdx >= 0 ? hdr.titleIdx + 1 : 1;
  const typeF = hdr.fields.find((f) => f.key === 'Type');
  const statusF = hdr.fields.find((f) => f.key === 'Status');
  const updF = hdr.fields.find((f) => f.key === 'Updated');
  const def = typeF && TYPES[typeF.value];
  if (!typeF) add(out, 1, rel, anchor, '头块缺 `Type:` 行');
  if (!updF) add(out, 1, rel, anchor, '头块缺 `Updated:` 行');
  if (!statusF && !(typeF && typeF.value === 'skill')) add(out, 1, rel, anchor, '头块缺 `Status:` 行');
  if (typeF && statusF && updF && !(typeF.idx < statusF.idx && statusF.idx < updF.idx)) add(out, 1, rel, typeF.idx + 1, '头块三行顺序应为 `Type:`、`Status:`、`Updated:`');
  if (typeF && !def) add(out, 1, rel, typeF.idx + 1, `Type 值 \`${typeF.value}\` 不在 9 类内`);
  if (statusF && def && def.statuses !== null && !def.statuses.includes(statusF.value)) add(out, 1, rel, statusF.idx + 1, `Status 值 \`${statusF.value}\` 不在 \`${typeF.value}\` 的允许集（${def.statuses.join('/')}）内`);
  if (updF && !DATE_RE.test(updF.value)) add(out, 1, rel, updF.idx + 1, `Updated 值 \`${updF.value}\` 非 yyyy-mm-dd`);
  for (const f of hdr.fields) { const m = PLACEHOLDER_RE.exec(f.value); if (m) add(out, 1, rel, f.idx + 1, `字段值残留占位符 \`${m[0]}\``); }
}

/** 相对目标存在性（含 URL 解码回退）——组 3 与组 1 裸路径形态共用。 */
function targetExists(dir, target) {
  if (existsSync(resolve(dir, target))) return true;
  try { return existsSync(resolve(dir, decodeURIComponent(target))); } catch { return false; }
}

/** 组 1：`Status: superseded` 必须带 `Superseded-by:`；链接形态目标存在性归组 3，裸路径形态在此查存在性。 */
function checkSuperseded(rel, dir, hdr, out) {
  const statusF = hdr.fields.find((f) => f.key === 'Status' && f.value === 'superseded');
  if (!statusF) return;
  const supF = hdr.fields.find((f) => f.key === 'Superseded-by');
  if (!supF || supF.value === '') { add(out, 1, rel, statusF.idx + 1, '`Status: superseded` 缺 `Superseded-by:` 行'); return; }
  if (supF.value.includes('](')) return; // Markdown 链接形态：目标存在性由组 3 负责
  const p = supF.value.split('#')[0].trim();
  if (p && !targetExists(dir, p)) add(out, 1, rel, supF.idx + 1, `Superseded-by 裸路径目标不存在：${supF.value}（建议改写成相对 Markdown 链接）`);
}

/** 组 1：活动笔记头——必须在 lifecycle 目录下、标题后第一个非空行是 `Status:`、取值与目录一致、rejected 原因非空；围栏内标题不参与定位。 */
function checkNoteHeader(rel, lines, hdr, loc, out) {
  if (!loc.inNotesActive) { add(out, 1, rel, 1, '笔记不在 lifecycle 目录（proposed / implemented / rejected）下'); return; }
  const titleIdx = hdr.titleIdx; // 复用 parseHeader 的标题定位（同一围栏过滤口径，避免两处各扫一遍）
  if (titleIdx === -1) { add(out, 1, rel, 1, '笔记缺标题行（头块两行：标题与 `Status:`）'); return; }
  let statusIdx = titleIdx, statusRaw = null;
  for (let i = titleIdx + 1; i < lines.length; i++) {
    if (lines[i].trim() === '') continue;
    const m = NOTE_STATUS_LINE_RE.exec(lines[i].trim());
    statusIdx = i;
    statusRaw = m ? m[1].trim() : null;
    break;
  }
  if (statusRaw === null) { add(out, 1, rel, statusIdx + 1, '笔记头：标题后的第一个非空行不是 `Status:` 行'); return; }
  const parts = statusRaw.split(EMDASH);
  const status = parts[0].trim(), reason = parts.slice(1).join(EMDASH).trim();
  if (status !== loc.lifecycle) add(out, 1, rel, statusIdx + 1, `笔记 Status \`${status || '（空）'}\` 与目录 \`${loc.lifecycle}\` 不一致`);
  if (status === 'rejected' && reason === '') add(out, 1, rel, statusIdx + 1, 'rejected 笔记的 `—` 后原因为空');
}

/** 组 1：归档件（带类型头块的文档）必须有 `Archived: yyyy-mm-dd`；目录入口页与说明性文件不适用。 */
function checkArchivedHeader(rel, hdr, out) {
  const typeF = hdr.fields.find((f) => f.key === 'Type');
  if (!typeF) return;
  const archF = hdr.fields.find((f) => f.key === 'Archived');
  if (!archF) add(out, 1, rel, typeF.idx + 1, '归档件缺 `Archived: yyyy-mm-dd` 行');
  else if (!DATE_RE.test(archF.value)) add(out, 1, rel, archF.idx + 1, `Archived 值 \`${archF.value}\` 非 yyyy-mm-dd`);
}

/** 组 2：`.agents/` 下类型与家不符；带类型头块的文档住到 `.agents/` 之外（如 `docs/`）同样报；日期型文件名不以 `yyyy-mm-dd-` 开头。 */
function checkHomeName(rel, hdr, loc, base, isIndex, out) {
  const typeF = hdr.fields.find((f) => f.key === 'Type');
  const def = typeF && TYPES[typeF.value];
  if (def && !isIndex && loc.inAgents && loc.home !== def.home) add(out, 2, rel, typeF.idx + 1, `类型 \`${typeF.value}\` 应住 \`${def.home}/\`，实际在 \`${loc.home ?? '（不在任何家）'}\``);
  // 面外类型化文档：结构锚存在时可判（规范「docs/ 不装计划与提案」——九类的家都在 `.agents/` 下）；扫描面内无锚时不判，避免误伤外部路径巡检
  else if (def && !isIndex && !loc.inAgents && STRUCT_ANCHORS.length) add(out, 2, rel, typeF.idx + 1, `类型 \`${typeF.value}\` 的文档住在 \`.agents/\` 之外（应住 \`.agents/${def.home}/\`）——\`docs/\` 等面外根不装带类型头块的文档`);
  const dated = loc.home === 'plans' || loc.home === 'research' || loc.inNotesActive;
  if (dated && !isIndex && !DATED_NAME_RE.test(base)) add(out, 2, rel, 1, '日期型文档文件名应以 `yyyy-mm-dd-` 开头');
}

/** 组 3：相对 Markdown 链接目标不存在（锚点与外部 URL 跳过，含协议相对 `//`；扫描含归档区）。 */
function checkLinks(rel, abs, lines, a, out) {
  const dir = dirname(abs);
  for (let i = 0; i < lines.length; i++) {
    if (a.fenced[i]) continue;
    const masked = maskInline(a.cmask[i]);
    if (!masked.includes('](')) continue; // includes 守卫：无 `](` 的行跳过对超长行二次方退化的正则扫描
    for (const m of masked.matchAll(LINK_RE)) {
      let target = m[1].trim();
      if (target.startsWith('<') && target.endsWith('>')) target = target.slice(1, -1).trim(); // CommonMark 尖括号目标：剥 `<` `>` 再判存在性
      if (!target || target.startsWith('#') || target.startsWith('//') || SCHEME_RE.test(target)) continue;
      const pathPart = target.split('#')[0];
      if (pathPart && !targetExists(dir, pathPart)) add(out, 3, rel, i + 1, `相对链接目标不存在：${target}`);
    }
  }
}

/** 按豁免清单分发各组（组 0 独立预检）：归档区只受组 1 归档头与组 3 链接管辖；组 1 只读头块；围栏与行内代码中的同形文本不参与判定。 */
function checkDoc(f, out) {
  const { lines, bad } = readLines(f.abs);
  const a = analyze(lines);
  const hdr = parseHeader(lines, a.fenced);
  const loc = locate(f.abs);
  const base = basename(f.abs);
  const isIndex = INDEX_NAMES.has(base) || base === 'SKILL.md'; // 类型化文档的例外名
  if (bad) add(out, 0, f.rel, 1, '文件含无法按 UTF-8 解码的字节或 NUL 字节（疑似非 UTF-8 文本），其余判据不可信');
  if (loc.inArchive) {
    if (!isIndex) checkArchivedHeader(f.rel, hdr, out);
  } else if (loc.home === 'notes') {
    if (!isIndex) checkNoteHeader(f.rel, lines, hdr, loc, out);
  } else if (TYPED_HOMES.has(loc.home) && !isIndex) {
    checkHeaderBlock(f.rel, hdr, out);
    checkSuperseded(f.rel, dirname(f.abs), hdr, out);
  }
  if (!loc.inArchive) checkHomeName(f.rel, hdr, loc, base, isIndex, out);
  checkLinks(f.rel, f.abs, lines, a, out);
}

/** 组 2：`.agents/` 子树内目录名 kebab-case（归档区只受组 1 与组 3 管辖，不查）。返回实际检查的目录数。 */
function checkDirNames(dirs, out) {
  let n = 0;
  for (const d of dirs) {
    const loc = locate(d.abs, true);
    if (!loc.inAgents || loc.home === null || loc.inArchive) continue;
    n++;
    if (!KEBAB_RE.test(basename(d.abs))) add(out, 2, d.rel, 0, '目录名不合 kebab-case（小写字母、数字、连字符）');
  }
  return n;
}

function main(argv) {
  let opts;
  try { opts = parseArgs(argv); } catch (e) {
    if (e instanceof UsageError) { console.error(`用法错误：${e.message}\n${USAGE}`); return 2; }
    throw e;
  }
  if (opts.help) { console.log(USAGE); return 0; }
  const explicit = opts.paths.length > 0;
  let roots;
  if (explicit) {
    roots = opts.paths.map((p) => resolve(p));
    for (const [i, r] of roots.entries()) if (!existsSync(r)) { console.error(`路径不存在：${opts.paths[i]}`); return 2; }
  } else {
    if (ROOT === null) {
      console.error(`根锚点缺失：调用目录 ${process.cwd()} 自身不含 \`.agents/\` 目录；默认调用一律退出码 2，不静默降级、不向父目录上溯（先 cd 到工作区根，或显式传路径只跑编码预检与链接组）`);
      return 2;
    }
    roots = [join(ROOT, '.agents'), join(ROOT, 'docs')].filter((p) => existsSync(p));
  }
  console.log(`被检树：${process.cwd()}（${explicit ? `显式面 ${roots.length} 个路径` : '默认面 .agents/ + docs/'}）`);
  const { files, dirs } = collect(roots, opts.excludes);
  if (!files.length) { console.error(`未检查任何文件：扫描面为空（${roots.map(relOf).join('、')}）`); return 2; }
  let structNote = '';
  if (explicit) {
    STRUCT_ANCHORS = dirs.filter((d) => basename(d.abs) === '.agents').map((d) => d.abs);
    if (!STRUCT_ANCHORS.length) {
      structNote = '结构组未执行：扫描面内无 `.agents/` 子树';
      console.error('注：扫描面内没有 `.agents/` 子树，结构组不执行（只跑编码预检与链接组）');
    }
  } else {
    STRUCT_ANCHORS = [realOr(join(ROOT, '.agents'))];
  }
  // 锚存在不等于有结构可判：空 `.agents/`（部分载荷落地、文件级搬运后的形态）会与完整树同形全绿
  if (!structNote && STRUCT_ANCHORS.length && !STRUCT_ANCHORS.some((a) => HOMES.some((h) => existsSync(join(a, h))))) {
    structNote = '结构组无判定对象：`.agents/` 下无任何家目录（plans / research / notes / skills / archive）';
    console.error(`注：${structNote}——只跑编码预检与链接组`);
  }
  const out = [];
  for (const f of files) checkDoc(f, out);
  checkDirNames(dirs, out);
  out.sort((x, y) => (x.rel < y.rel ? -1 : x.rel > y.rel ? 1 : x.line - y.line || x.group - y.group));
  for (const x of out) console.log(`[组 ${x.group} · ${GROUP_NAMES[x.group]}] ${x.line > 0 ? `${x.rel}:${x.line}` : `${x.rel}/`} — ${x.msg}`);
  console.log(`检查 ${files.length} 篇 / 告警 ${out.length} 条${structNote ? `（${structNote}）` : ''}`);
  if (out.length && opts.strict) { console.error(`✗ --strict：${out.length} 条告警`); return 1; }
  return 0;
}

try {
  process.exit(main(process.argv.slice(2)));
} catch (e) {
  console.error(`内部错误：${e && e.stack ? e.stack : e}`); // 诚实条款：异常必须响，不静默返回 0
  process.exit(2);
}
