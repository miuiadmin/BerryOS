#!/usr/bin/env node
/**
 * check-docs.mjs —— Curia 2.4 文档自检（纯 node 标准库、零依赖、Node ≥ 18、不联网、不执行文档内代码）。
 * 权威：Curia 2.4 规范「配套脚本规格」与「豁免清单」——编码预检 + 3 组检查（报告组号 0 至 3）、退出码 0/1/2、
 * 目录级与组级豁免、诚实条款（空集必红、根锚点缺失必红、只证明机械子集、不读 git、异常非零）。用法：
 *   node .agents/tools/check-docs.mjs [--strict] [--exclude <路径或 glob>]… [路径…]
 *   报告模式列发现不失败（0）；--strict 任一发现即 1；
 *   2 = 无法完成检查：扫描面为空、路径不存在、根锚点缺失、内部错误——两种模式都不得返回 0。
 * 锚点：工作区根 = 调用时的工作目录，该目录须自身含 `.agents/` 或 `docs/` 目录——两者皆无时默认调用退出码 2，
 *   不自脚本位置、不向父目录上溯（上溯会命中 `~/.agents` 一类无关祖先）；脚本文件位置不限，一律以调用目录为基准。
 *   结构判定以根下 `.agents/` 目录为锚；显式路径的面内无 `.agents/` 子树时结构组不执行，stderr 注明、汇总行自带标注；
 *   结构锚下五个家一个都没有时同样标注（空 `.agents/` 会与完整树同形全绿）。只有 `docs/` 时走 docs-only 模式：
 *   不跑 `.agents/` 结构组，仍跑编码预检、docs 标准头块与链接组。
 * 扫描面：未给路径时取默认两根 `.agents/` 与 `docs/`（两者存在才扫，至少一个存在即合法根），
 *   默认两根与 --exclude 的相对值以调用目录（即工作区根）为基准；只收 `.md`（大小写不敏感），`.mdx`/`.markdown` 不进面。
 * 取舍：目录名 kebab-case 只查 `.agents/` 子树；日期型文件名只查 plans/research 家与 notes 活动区；
 *   `docs/` 标准只查 `Version` 格式与文件名版本一致性，`docs/` 自由知识命名不查；
 *   组 0「编码」在文件无法按 UTF-8 解码或含 NUL 字节时报出；纯文件系统遍历，不读 git；
 *   仅 POSIX 实测：Windows 未实测、路径大小写不做归一（绝对 --exclude 按逐字前缀比较）。
 */

import { existsSync, readdirSync, readFileSync, realpathSync, statSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';

/** 工作区根：调用时的工作目录，须自身含 `.agents/` 或 `docs/` 目录；否则 null（默认调用下即根锚点缺失）。不做上溯——上溯会命中 `~/.agents` 一类无关祖先。 */
const ROOT = (() => {
  const cwd = process.cwd();
  for (const name of ['.agents', 'docs']) {
    try { if (statSync(join(cwd, name)).isDirectory()) return cwd; } catch {}
  }
  return null;
})();
const realOr = (p) => { try { return realpathSync(p); } catch { return p; } };
const isDir = (p) => { try { return statSync(p).isDirectory(); } catch { return false; } };

/** 类型枚举（机器读的一份；人读权威在规范「10 类文档」表）：类型 → 家 → 允许状态（null = 无 Status）。 */
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
  standard: { home: 'docs', statuses: ['current', 'historical', 'superseded'] },
};

/** 现行态集（取代链终点判据用）：该类型的 Status 落在集合内才算「现行依据」，取代链必须终结在这里。`spec` 的 `done` 仍是权威（规范「状态与退役」）；`plan`/`proposal` 的 `done` 是历史；`research` 三类的 `historical` 是时效过去。 */
const CURRENT = {
  proposal: ['draft', 'active'], spec: ['draft', 'active', 'done'], plan: ['draft', 'active'],
  survey: ['current'], evidence: ['current'], postmortem: ['current'], reference: ['current'], runbook: ['current'],
  standard: ['current'],
};

const GROUP_NAMES = { 0: '编码', 1: '头块与生命周期', 2: '家与命名', 3: '链接' };
const SKIP_DIR_NAMES = new Set(['.git', 'node_modules']);
const TYPED_HOMES = new Set(['plans', 'research', 'skills']); // `.agents/` 下 3 行头块的三个家；docs 标准在 checkDoc 的 docs 分支单独判
const HOMES = ['plans', 'research', 'notes', 'skills', 'archive']; // `.agents/` 下五个家——结构组「有没有结构可判」的判据
const NOTE_STATUSES = new Set(['proposed', 'implemented', 'rejected']);
const INDEX_NAMES = new Set(['README.md', 'AGENTS.md']); // 目录说明文件不进类型化面
const MD_RE = /\.[mM][dD]$/, DATE_RE = /^\d{4}-\d{2}-\d{2}$/, DATED_NAME_RE = /^\d{4}-\d{2}-\d{2}-/; // 扩展名大小写不敏感
const CJK_DIR_RE = /^[\u4e00-\u9fa5][\u4e00-\u9fa50-9]*$/; // 目录名另一种合法形态：纯中文（可续数字，不含空格与标点）
const KEBAB_RE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/, SCHEME_RE = /^[a-zA-Z][a-zA-Z0-9+.-]*:/, FIELD_RE = /^\s*([A-Za-z][A-Za-z0-9_-]*):\s*(.*)$/;
const SEMVER_RE = /^\d+\.\d+\.\d+$/; // 模块设计文档的 `Version:`
const NOTE_STATUS_LINE_RE = /^Status:\s*(.*)$/, PLACEHOLDER_RE = /<[^<>\n]+>/;
const EMDASH = '—', INLINE_MARK = '\uE000';
const LINK_RE = /\[[^\]]*\]\(((?:[^()\s]|\([^()\s]*\))+)(?:\s+(?:"[^"]*"|'[^']*'))?\s*\)/g; // 组 3：取链接目标（一层平衡括号，如 foo(1).md；可带 "title" / 'title'）

/** 结构锚（绝对路径）：结构判定以这些 `.agents/` 目录为锚——默认调用为根下 `.agents/`；显式路径为扫描面内找到的 `.agents/` 目录（无则不判结构）。 */
let STRUCT_ANCHORS = [];

/** docs 锚（绝对路径）：`docs/` 自由知识与标准的判定基准——默认调用为根下 `docs/`；显式路径为扫描面内找到的 `docs/` 目录。 */
let DOCS_ANCHORS = [];

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

/** 定位文件/目录在结构锚下的位置（家、归档区、笔记 lifecycle；docs 根与 docs 归档区）；不在任何锚下时 inAgents/inDocs 均为 false。 */
function locate(abs, isDir = false) {
  let anchor = null;
  for (const a of STRUCT_ANCHORS) {
    if ((abs === a || abs.startsWith(a + sep)) && (anchor === null || a.length > anchor.length)) anchor = a;
  }
  const rel = anchor === null ? null : relative(anchor, abs).split(sep).join('/');
  const after = rel === null || rel === '' ? [] : rel.split('/');
  const home = after.length > (isDir ? 0 : 1) ? after[0] : null;
  let docsAnchor = null;
  for (const a of DOCS_ANCHORS) {
    if ((abs === a || abs.startsWith(a + sep)) && (docsAnchor === null || a.length > docsAnchor.length)) docsAnchor = a;
  }
  const docsRel = docsAnchor === null ? null : relative(docsAnchor, abs).split(sep).join('/');
  const inDocs = docsAnchor !== null;
  const inDocsArchive = inDocs && (docsRel === 'archive' || docsRel.startsWith('archive/'));
  return { inAgents: anchor !== null, home, lifecycle: home === 'notes' ? after[1] ?? null : null,
    inArchive: home === 'archive' || (home === 'notes' && after[1] === 'archived') || inDocsArchive,
    inNotesActive: home === 'notes' && NOTE_STATUSES.has(after[1]), inDocs, docsRel, inDocsArchive };
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
  if (typeF && !def) add(out, 1, rel, typeF.idx + 1, `Type 值 \`${typeF.value}\` 不在 10 类内`);
  if (statusF && def && def.statuses !== null && !def.statuses.includes(statusF.value)) add(out, 1, rel, statusF.idx + 1, `Status 值 \`${statusF.value}\` 不在 \`${typeF.value}\` 的允许集（${def.statuses.join('/')}）内`);
  if (updF && !DATE_RE.test(updF.value)) add(out, 1, rel, updF.idx + 1, `Updated 值 \`${updF.value}\` 非 yyyy-mm-dd`);
  for (const f of hdr.fields) { const m = PLACEHOLDER_RE.exec(f.value); if (m) add(out, 1, rel, f.idx + 1, `字段值残留占位符 \`${m[0]}\``); }
}

/** 相对目标存在性（含 URL 解码回退）——组 3 与组 1 裸路径形态共用。 */
function targetExists(dir, target) {
  if (existsSync(resolve(dir, target))) return true;
  try { return existsSync(resolve(dir, decodeURIComponent(target))); } catch { return false; }
}

/** 从 `Superseded-by` 字段值取目标路径：Markdown 链接形态或裸路径；去锚点。取不到时返回空串。 */
function supersededTarget(value) {
  const linkRe = new RegExp(LINK_RE.source, '');
  const m = linkRe.exec(value);
  const raw = m ? m[1] : value;
  return raw.split('#')[0].trim();
}

/** 读一份文件的类型化头块（只读需要的那几个字段）；读不出或二进制化时返回 null。 */
function readTypedHeader(abs) {
  let text; try { text = readFileSync(abs, 'utf8'); } catch { return null; }
  text = text.replace(/^\uFEFF/, '');
  const lines = text.split(/\r\n|\r|\n/);
  if (lines.some((l) => l.includes('\u0000'))) return null;
  const { fenced } = analyze(lines);
  const hdr = parseHeader(lines, fenced);
  const get = (k) => hdr.fields.find((f) => f.key === k);
  const typeF = get('Type'), statusF = get('Status'), supF = get('Superseded-by');
  return { type: typeF ? typeF.value : null, status: statusF ? statusF.value : null,
    archived: Boolean(get('Archived')), sup: supF ? supF.value : null };
}

/** 解析相对链接目标为绝对路径（含 URL 解码回退）；不存在时 null。 */
function resolveTargetPath(dir, target) {
  const p = resolve(dir, target);
  if (existsSync(p)) return p;
  try { const q = resolve(dir, decodeURIComponent(target)); return existsSync(q) ? q : null; } catch { return null; }
}

/** 沿取代链走到终点，返回终点文件名；链断、成环、解析不到，或终点不是类型化/现行态/未归档的文档时返回 null（报错归 checkSupersededChain，这里只回答「现行版本是谁」）。 */
function resolveChainTerminal(abs) {
  let curAbs = resolve(abs); const seen = new Set([curAbs]);
  for (let hop = 0; hop < 16; hop++) {
    const doc = readTypedHeader(curAbs);
    if (!doc) return null;
    if (doc.status !== 'superseded') {
      // 只有类型化、现行态、未归档的终点才算「现行版本」；否则让调用方说「链未解析到现行版本」
      if (!doc.type || !TYPES[doc.type]) return null;
      if (locate(curAbs).inArchive || doc.archived) return null;
      const allowed = CURRENT[doc.type] || [];
      return allowed.includes(doc.status) ? basename(curAbs) : null;
    }
    if (!doc.sup) return null;
    const target = supersededTarget(doc.sup);
    const next = target ? resolveTargetPath(dirname(curAbs), target) : null;
    if (!next || seen.has(next)) return null;
    seen.add(next); curAbs = next;
  }
  return null;
}

/** 组 1：`Status: superseded` 必须带 `Superseded-by:`；链接形态目标存在性归组 3，裸路径形态在此查存在性。 */
function checkSuperseded(rel, abs, hdr, out) {
  const statusF = hdr.fields.find((f) => f.key === 'Status' && f.value === 'superseded');
  if (!statusF) return;
  const supF = hdr.fields.find((f) => f.key === 'Superseded-by');
  if (!supF || supF.value === '') { add(out, 1, rel, statusF.idx + 1, '`Status: superseded` 缺 `Superseded-by:` 行'); return; }
  if (!supF.value.includes('](')) { // Markdown 链接形态：目标存在性由组 3 负责
    const p = supF.value.split('#')[0].trim();
    if (p && !targetExists(dirname(abs), p)) { add(out, 1, rel, supF.idx + 1, `Superseded-by 裸路径目标不存在：${supF.value}（建议改写成相对 Markdown 链接）`); return; }
  }
  checkSupersededChain(rel, abs, supF, out);
}

/** 组 1：取代链必须终结在类型化、未再被取代、未归档的文档上——「任何一篇旧文档都能机械解析到现行版本」。 */
function checkSupersededChain(rel, abs0, supF, out) {
  const chain = [basename(abs0)];
  const seen = new Set([resolve(abs0)]);
  let curDir = dirname(abs0), value = supF.value, hop = 0;
  while (hop++ < 16) {
    const target = supersededTarget(value);
    if (!target) return; // 空值等形态问题由缺行判据负责
    const abs = resolveTargetPath(curDir, target);
    if (!abs) return; // 目标不存在：组 3 或裸路径存在性已报
    const label = basename(abs);
    chain.push(label);
    if (seen.has(abs)) { add(out, 1, rel, supF.idx + 1, `Superseded-by 取代链成环：${chain.join(' → ')}`); return; }
    seen.add(abs);
    const doc = readTypedHeader(abs);
    if (!doc) { add(out, 1, rel, supF.idx + 1, `Superseded-by 取代链第 ${hop} 跳指向无法读取的文件：${target}`); return; }
    if (!doc.type || !TYPES[doc.type]) { add(out, 1, rel, supF.idx + 1, `Superseded-by 取代链未终结在类型化文档：${label} 无 Type 头块——被取代的指导要有类型化的现行版本`); return; }
    if (locate(abs).inArchive || doc.archived) { add(out, 1, rel, supF.idx + 1, `Superseded-by 取代链终结于归档件 ${label}：归档件是冷历史，不能充当现行版本`); return; }
    if (doc.status !== 'superseded') { // 终点：必须落在现行态
      const allowed = CURRENT[doc.type] || [];
      if (!allowed.includes(doc.status)) add(out, 1, rel, supF.idx + 1, `Superseded-by 取代链终点不是现行态：${label} 的 Status 是 ${doc.status || '（空）'}，现行态为 ${allowed.join('/') || '（无）'}`);
      return;
    }
    if (!doc.sup) { add(out, 1, rel, supF.idx + 1, `Superseded-by 取代链在 ${label} 断开：它是 superseded 却缺 Superseded-by:`); return; }
    curDir = dirname(abs); value = doc.sup;
  }
  add(out, 1, rel, supF.idx + 1, `Superseded-by 取代链超过 16 跳仍未终结：${chain.join(' → ')}`);
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

/** 组 1/2：模块设计文档的固定形态——`plans/<模块>/design.md`，Type 为 spec、带 `Version` semver、标题含模块标识。 */
function checkModuleDesign(rel, hdr, lines, out) {
  const parts = rel.split('/'), base = parts[parts.length - 1];
  if (base !== 'design.md') return;
  // rel 相对工作区根：合法形态恰为 `.agents/plans/<模块>/design.md`
  const underAnchor = parts[0] === '.agents' ? parts.slice(1) : parts;
  if (underAnchor.length !== 3 || underAnchor[0] !== 'plans') { add(out, 2, rel, 1, '`design.md` 是模块设计文档的固定名，应放在 `.agents/plans/<模块>/design.md`'); return; }
  const slug = underAnchor[1];
  const typeF = hdr.fields.find((f) => f.key === 'Type');
  const verF = hdr.fields.find((f) => f.key === 'Version');
  const anchor = hdr.titleIdx >= 0 ? hdr.titleIdx + 1 : 1;
  if (!typeF || typeF.value !== 'spec') add(out, 1, rel, anchor, '模块设计文档的 `Type:` 应为 `spec`');
  if (!verF) add(out, 1, rel, anchor, '模块设计文档缺 `Version:` 行（形如 1.4.2，记这份设计自己的版本）');
  else if (!SEMVER_RE.test(verF.value)) add(out, 1, rel, verF.idx + 1, `Version 值 \`${verF.value}\` 非 MAJOR.MINOR.PATCH`);
  const title = hdr.titleIdx >= 0 ? lines[hdr.titleIdx] : '';
  if (!title.includes(slug)) add(out, 1, rel, hdr.titleIdx >= 0 ? hdr.titleIdx + 1 : 1, `标题应含模块标识 \`${slug}\`——模块设计文档的名字就是这个模块`);
}

/** 组 1：docs 标准——必须有 `Version:` semver；文件名带版本时须与头块一致（文件名是人读标签，头块是权威）。 */
function checkStandard(rel, hdr, out) {
  const typeF = hdr.fields.find((f) => f.key === 'Type');
  if (!typeF || typeF.value !== 'standard') return;
  const verF = hdr.fields.find((f) => f.key === 'Version');
  const anchor = hdr.titleIdx >= 0 ? hdr.titleIdx + 1 : 1;
  if (!verF) add(out, 1, rel, anchor, '标准缺 `Version:` 行（形如 1.2.0，记这份标准自己的契约版本）');
  else if (!SEMVER_RE.test(verF.value)) add(out, 1, rel, verF.idx + 1, `Version 值 \`${verF.value}\` 非 MAJOR.MINOR.PATCH`);
  else {
    const m = /v(\d+(?:\.\d+){0,2})/i.exec(basename(rel));
    if (m && !(verF.value === m[1] || verF.value.startsWith(`${m[1]}.`))) {
      add(out, 1, rel, verF.idx + 1, `文件名版本 \`v${m[1]}\` 与头块 Version \`${verF.value}\` 不一致`);
    }
  }
}

/** 组 2：按类型表判家——`.agents/` 下类型与家不符、`docs/` 下非 standard、面外带类型头块的文档。日期型文件名不以 `yyyy-mm-dd-` 开头也报。 */
function checkHomeName(rel, hdr, loc, base, isIndex, out, exemptDated = false) {
  const typeF = hdr.fields.find((f) => f.key === 'Type');
  const def = typeF && TYPES[typeF.value];
  if (def && !isIndex) {
    if (loc.inAgents) {
      if (def.home === 'docs') {
        add(out, 2, rel, typeF.idx + 1, `类型 \`${typeF.value}\` 应住 \`docs/\`，实际在 \`.agents/${loc.home ?? '（不在任何家）'}\``);
      } else if (loc.home !== def.home) {
        add(out, 2, rel, typeF.idx + 1, `类型 \`${typeF.value}\` 应住 \`${def.home}/\`，实际在 \`${loc.home ?? '（不在任何家）'}\``);
      }
    } else if (loc.inDocs && def.home !== 'docs') {
      add(out, 2, rel, typeF.idx + 1, `类型 \`${typeF.value}\` 应住 \`.agents/${def.home}/\`，实际在 \`docs/\``);
    } else if (!loc.inAgents && !loc.inDocs && STRUCT_ANCHORS.length) {
      // 面外类型化文档：扫描面内有 .agents/ 结构锚时可判；无锚时不判，避免误伤外部路径巡检
      const expected = def.home === 'docs' ? '`docs/`' : `\`.agents/${def.home}/\``;
      add(out, 2, rel, typeF.idx + 1, `类型 \`${typeF.value}\` 的文档住在受管根之外（应住 ${expected}）`);
    }
  }
  const dated = loc.home === 'plans' || loc.home === 'research' || loc.inNotesActive;
  if (dated && !isIndex && !exemptDated && !DATED_NAME_RE.test(base)) add(out, 2, rel, 1, '日期型文档文件名应以 `yyyy-mm-dd-` 开头');
}

/** 组 3：相对 Markdown 链接目标不存在（锚点与外部 URL 跳过，含协议相对 `//`；扫描含归档区）。 */
function checkLinks(rel, abs, lines, a, out, hdr, loc) {
  const dir = dirname(abs);
  const typeF = hdr ? hdr.fields.find((f) => f.key === 'Type') : null;
  const statusF = hdr ? hdr.fields.find((f) => f.key === 'Status') : null;
  // 「现行依据」只可能是 plans/、skills/ 与 docs/ 下 current 的 standard：research/ 是某一时点的快照，引用旧文本属正常；笔记、归档与 docs 自由知识同理
  const referrerCurrent = Boolean(typeF) && (!statusF || statusF.value !== 'superseded')
    && Boolean(loc) && !loc.inArchive && !loc.inNotesActive
    && (loc.home === 'plans' || loc.home === 'skills'
      || (loc.inDocs && typeF.value === 'standard' && statusF && statusF.value === 'current'));
  for (let i = 0; i < lines.length; i++) {
    if (a.fenced[i]) continue;
    const masked = maskInline(a.cmask[i]);
    if (!masked.includes('](')) continue; // includes 守卫：无 `](` 的行跳过对超长行二次方退化的正则扫描
    for (const m of masked.matchAll(LINK_RE)) {
      let target = m[1].trim();
      if (target.startsWith('<') && target.endsWith('>')) target = target.slice(1, -1).trim(); // CommonMark 尖括号目标：剥 `<` `>` 再判存在性
      if (!target || target.startsWith('#') || target.startsWith('//') || SCHEME_RE.test(target)) continue;
      const pathPart = target.split('#')[0];
      if (pathPart && !targetExists(dir, pathPart)) { add(out, 3, rel, i + 1, `相对链接目标不存在：${target}`); continue; }
      if (!referrerCurrent || !pathPart || !MD_RE.test(pathPart)) continue;
      const tAbs = resolveTargetPath(dir, pathPart);
      const tDoc = tAbs ? readTypedHeader(tAbs) : null;
      if (tDoc && tDoc.status === 'superseded') {
        const term = resolveChainTerminal(tAbs);
        add(out, 3, rel, i + 1, `引用了已被取代的文档 ${basename(tAbs)}${term ? `，现行版本是 ${term}` : '，且取代链未解析到现行版本'}——现行依据应指向取代者，历史说明写进笔记`);
      }
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
    checkSuperseded(f.rel, f.abs, hdr, out);
    checkModuleDesign(f.rel, hdr, lines, out);
  } else if (loc.inDocs && hdr.fields.some((f) => f.key === 'Type') && !isIndex) {
    // docs/ 自由知识无头块；一旦出现 `Type:` 就按类型化文档判——standard 是唯一合法类型，其余由组 2 报家归属
    checkHeaderBlock(f.rel, hdr, out);
    checkSuperseded(f.rel, f.abs, hdr, out);
    checkStandard(f.rel, hdr, out);
    checkModuleDesign(f.rel, hdr, lines, out); // `design.md` 仍是模块设计文档的固定名，docs 下出现同样报
  }
  const relParts = f.rel.split('/'), underAnchorLen = relParts[0] === '.agents' ? relParts.length - 1 : relParts.length;
  const moduleDesign = base === 'design.md' && underAnchorLen === 3 && relParts.includes('plans'); // 免日期前缀（固定名，见规范「模块设计文档」）
  if (!loc.inArchive) checkHomeName(f.rel, hdr, loc, base, isIndex, out, moduleDesign);
  checkLinks(f.rel, f.abs, lines, a, out, hdr, loc);
}

/** 组 2：`.agents/` 子树内目录名——kebab-case 或纯中文名，二者取一（归档区只受组 1 与组 3 管辖，不查）。返回实际检查的目录数。 */
function checkDirNames(dirs, out) {
  let n = 0;
  for (const d of dirs) {
    const loc = locate(d.abs, true);
    if (!loc.inAgents || loc.home === null || loc.inArchive) continue;
    n++;
    const name = basename(d.abs);
    if (!KEBAB_RE.test(name) && !CJK_DIR_RE.test(name)) add(out, 2, d.rel, 0, '目录名应为 kebab-case（小写字母、数字、连字符）或纯中文名（中文开头，可续数字，不含空格与标点）');
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
      console.error(`根锚点缺失：调用目录 ${process.cwd()} 自身既不含 \`.agents/\` 也不含 \`docs/\` 目录；默认调用一律退出码 2，不静默降级、不向父目录上溯（先 cd 到工作区根，或显式传路径只跑编码预检与链接组）`);
      return 2;
    }
    roots = [join(ROOT, '.agents'), join(ROOT, 'docs')].filter(isDir);
  }
  const defaultFace = roots.map((p) => `${basename(p)}/`).join(' + ');
  console.log(`被检树：${process.cwd()}（${explicit ? `显式面 ${roots.length} 个路径` : `默认面 ${defaultFace}`}）`);
  const { files, dirs } = collect(roots, opts.excludes);
  if (!files.length) { console.error(`未检查任何文件：扫描面为空（${roots.map(relOf).join('、')}）`); return 2; }
  let structNote = '';
  if (explicit) {
    STRUCT_ANCHORS = dirs.filter((d) => basename(d.abs) === '.agents').map((d) => d.abs);
    DOCS_ANCHORS = dirs.filter((d) => basename(d.abs) === 'docs').map((d) => d.abs);
    if (!STRUCT_ANCHORS.length) {
      structNote = '结构组未执行：扫描面内无 `.agents/` 子树';
      console.error('注：扫描面内没有 `.agents/` 子树，结构组不执行（只跑编码预检与链接组；扫描面内有 `docs/` 时另跑标准头块）');
    }
  } else {
    STRUCT_ANCHORS = isDir(join(ROOT, '.agents')) ? [realOr(join(ROOT, '.agents'))] : [];
    DOCS_ANCHORS = isDir(join(ROOT, 'docs')) ? [realOr(join(ROOT, 'docs'))] : [];
    if (!STRUCT_ANCHORS.length) {
      structNote = '结构组未执行：仅 docs/ 根（无 `.agents/` 结构锚）';
      console.error(`注：${structNote}——只跑编码预检、docs 标准头块与链接组`);
    }
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
