#!/usr/bin/env python3
"""task-graph.py —— 任务 DAG 图构建器+校验器（plans/ 实施顺序表 → 节点+边）。

载体语法（设计出处：任务 DAG 件 Study 设计，本件是图的唯一构建实现）：
  plan 文档「## 实施顺序」节下的五列表（中英列名按位混认）：
      | 状态 | 任务 | 交付物 | 依赖 | 验收 |
      （| Done | Section | Deliverable | Needs | Items |；可选第 6 列 | 需 |）
  状态闭集：[x]=done、[/]=已交付欠审（needs_work，评审恒可跑）、[ ]=open、
      [PARKED→人审: 理由]=parked（表行与 punch 行同形）。
  任务列=本文档编号节 §N（一行一节；节点地址）。依赖列闭集语法：
      §N（本文档节）、T##（本文档 punch 条目）、[文字](相对链接) §N/T##
      （跨文档，目标相对本文档解析）、§A-§B 区间展开、-- 无依赖。
      散文 token 静默跳过（防假阳性洪泛）；长得像引用的（TODO-/§/路径形/
      .md 尾）→ 未决红（假阴性与误报的分界——宁点名不吞）。
  punch 条目带机读 ID 的新语法 `- [ ] T12·说明……` 全文可作依赖目标，行内
      `（先 T07）`=依赖注记；无 ID 的老 punch 行不进图（切分归 punch_slice.py
      唯一实现，本件只认带 ID 新语法，不另造一份切分语义）。

节点 ID = plan 文件相对仓根路径 + 「#§N」/「#T##」（稳定地址——重排文件内
条目不漂移引用）。判定单源：本件 classify/状态判定是图/门/包三消费面共用
实现（punch_slice 的「切分单源」律升级为「判定单源」），消费面 import 本件，
不许抄语义。

子命令与退出码（0/1/2 三态，全仓惯例）：
  parse   解析语料打印图概要（--json 机读）   0=正常 2=表畸形/撞名/载入失败
  check   环/悬空/自依赖/孤儿/未决/闭集检     0=全绿 1=有红发现 2=载入失败
  next    可建造集+等待链+停车摘要            0=正常（空集也是诚实答案）2=拒判
  dot     Graphviz DOT（advisory 面）          同 parse
  cache   写 JSON 缓存（输入指纹自证）         0=写成 2=写失败
  --selftest 合成 plans 树自检（好图/环/悬空/自依赖/next 集/缓存回路 等 17 腿）

图接缝（task-oracle 契约）：模块级 ready_rows(root) → [(文件相对路径, 节号字
符串)]，给 task-oracle --next-buildable 当结构可跑面（图管结构、oracle 管双戳
分类，交集=可建）；异常上抛由 oracle 捕获 → fail-open（规划故障不停跑）。本件
为被 spec_from_file_location 手动装载而写成零魔法朴素类（py3.14 dataclass 在
未注册 sys.modules 的装载下崩——自检「oracle 接缝」腿实测在案）。

血教训（原型 impossible-os todo-graph 的 BerryOS 收窄移植）：
  - 状态载不入即拒判 rc 2，绝不答「队列空」——空数组/截断缓存会让一切分类
    DONE，静默完成未完成的工作，refuse 比 lie 好；
  - 共享缓存每跑必陈 → 读路径默认现场重建（秒级、确定），缓存只作外部消费
    面快照；读它（--from-cache）必过 schema 验证+输入指纹对拍，坏/陈皆 rc 2；
  - 做完的工作不约束任何人：环检跑在等待缩面上（双端皆 open/progress，parked
    不入缩面——unpark 后自然复检）；悬空/未决只对 open 依赖方报告（done/parked
    留历史依赖不追——Curia 规则 4 的机器执法面：翻状态必须同批修入链，否则
    死边红）；
  - 歧义目标拒判不猜第一个（INDEX.md 同名教训）：撞节号/撞 T 号=构建硬错。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
import traceback
from pathlib import Path

try:  # POSIX 排它锁；非 POSIX 平台降级为仅靠 tmp+rename 原子性（并发写最后赢者恒完整）
    import fcntl
except ImportError:  # pragma: no cover
    fcntl = None

TOOL_VERSION = 1
SCHEMA_VERSION = 1
EXIT_OK, EXIT_RED, EXIT_REFUSE = 0, 1, 2

# --- 载体语法常量（闭集——改这里就是改契约，须同批改 selftest 夹具） -----------

IO_TITLES = ("实施顺序", "implementation order")  # 标题前缀匹配（中英）
COL_SYNONYMS = [  # 五列按位同义集（混用中英列名按位成立）
    {"状态", "done", "status"},
    {"任务", "section", "task"},
    {"交付物", "deliverable"},
    {"依赖", "needs", "depends", "dependency"},
    {"验收", "items", "acceptance", "accept"},
]
WANTS_SYNS = {"需", "wants"}
WANTS_CLOSED = {"", "--", "attended", "owner-decision", "首跑", "场依赖"}  # 需列值域
NODE_STATES = {"done", "progress", "open", "parked"}
EDGE_STATES = {"satisfied", "waiting", "parked", "dangling", "non_node", "unresolved", "self"}
NEEDS_NONE = {"", "-", "--", "——", "—"}  # 无依赖的合法写法（em-dash 容错）
PARK_MARK = "[PARKED"
EXCLUDED_STATUS = {"superseded", "historical"}  # 规则 9：过时文档不当现行依据

HEADING_RE = re.compile(r"^(#{1,6})\s+(.+?)\s*$")
FENCE_RE = re.compile(r"^\s*(```|~~~)")
SEP_CELL_RE = re.compile(r"^:?-{1,}:?$")
PUNCH_T_RE = re.compile(r"^-\s*\[([ xX/])\]\s*(.*)$")
T_ID_RE = re.compile(r"(?<![0-9T])T(\d+)·")  # 中点 · 是 ID 与说明的分界（设计语法）
XIAN_NOTE_RE = re.compile(r"（先([^）]*)）")  # 「（先 T07）」依赖注记（全角括号）
NUM_HEAD_RE = re.compile(r"^([0-9]{1,3}|[一二三四五六七八九十]{1,3})[.、．](?!\d)\s*(.*)$")
_CN = {"一": 1, "二": 2, "三": 3, "四": 4, "五": 5, "六": 6, "七": 7, "八": 8, "九": 9}

# 依赖格 tokenizer：链接 | §N(区间) | T## | 分隔 | 其他。顺序即优先级——
# 链接优先保证 [T12 说明](x.md) 的标签不被 T\d 误切。
_DEP_TOKEN_RE = re.compile(
    r"(?P<link>\[[^\[\]]*\]\([^()\s]*\))"
    r"|(?P<sec>§\s*(\d+)(?:\s*[-–—至]\s*§?\s*(\d+))?)"
    r"|(?P<item>T\s*(\d+))"
    r"|(?P<sep>[\s,，;；]+)"
    r"|(?P<other>[^\s,，;；]+)"
)
_STRIP_TOK = "（）();；:：.。,，、`"  # 散文标点不进判定（原型的尾标点剥离律）


def _cn_num(s: str):
    """中文节号 → int（一~九十九）。为什么封顶 99：编号节超过 99 的 plan 不存在；
    超范围返回 None 让该标题不按编号节收（歧义拒判，不猜）。"""
    if not s:
        return None
    if s == "十":
        return 10
    if "十" in s:
        head, _, tail = s.partition("十")
        if head and head not in _CN:
            return None
        if tail and tail not in _CN:
            return None
        return (1 if not head else _CN[head]) * 10 + (0 if not tail else _CN[tail])
    return _CN.get(s)


class RefuseError(Exception):
    """拒判（fail-closed）：载入/缓存面失败——绝不说「队列空」。"""


class FormatError(Exception):
    """载体畸形（构建硬错）：表头漂移/列数错/状态闭集外/撞节号。"""

    def __init__(self, rel: str, line: int, msg: str):
        super().__init__(f"{rel}:{line}: {msg}")
        self.rel, self.line, self.msg = rel, line, msg


# --- 状态判定单源 -------------------------------------------------------------


def classify_marker(marker: str, tail: str) -> tuple[str, str]:
    """checkbox 旗 + 行余文 → (state, park_reason)。表行与 punch 行共用本函数
    （判定单源——消费面不许再抄一份）。闭集外抛 FormatError 由调用方定位。"""
    if PARK_MARK in tail:
        m = re.search(r"\[PARKED→([^]]*)\]", tail)
        return "parked", (m.group(1).strip() if m else tail.strip())
    if marker in ("x", "X"):
        return "done", ""
    if marker == "/":
        return "progress", ""  # 已交付欠审：needs_work，但评审恒可跑（不被依赖阻塞）
    return "open", ""


# --- 载体解析 -----------------------------------------------------------------


class Row:
    """实施顺序表的一行。为什么不用 dataclass：消费面（task-oracle）用
    spec_from_file_location+exec_module 手动装载本件，py3.14 的 dataclass
    会查 sys.modules[模块名].__dict__——未注册即 AttributeError；朴素类
    对任何装载器零魔法（自检腿「oracle 接缝」实测此坑）。"""

    def __init__(self, num, state, park_reason, deliverable, needs_raw, wants, accept, line):
        self.num = num
        self.state = state
        self.park_reason = park_reason
        self.deliverable = deliverable
        self.needs_raw = needs_raw
        self.wants = wants
        self.accept = accept
        self.line = line


class TItem:
    def __init__(self, tid, num, state, park_reason, label, line):
        self.tid = tid  # 原书写形（T07 与 T7 是同一 int 的两种写法——匹配按 int，ID 按原书写）
        self.num = num
        self.state = state
        self.park_reason = park_reason
        self.label = label
        self.line = line


class Doc:
    def __init__(self, rel, path):
        self.rel = rel
        self.path = path
        self.excluded = ""  # 非空=翻状态跳过（规则 9：旧文不当现行依据）
        self.rows = []
        self.items = []
        self.headings = {}  # 编号节 N → 标题
        self.dup_headings = []
        self.notes = []


def _fence_mask(lines: list) -> list:
    """围栏遮蔽：``` 围起的行对一切扫描不可见。为什么：示例表/示例 punch 行
    会制造第二个假扫描面（原型实测：夹具里的表曾顶掉真表）。"""
    mask, inf = [], False
    for ln in lines:
        if FENCE_RE.match(ln):
            mask.append(True)
            inf = not inf
            continue
        mask.append(inf)
    return mask


def _head_status(text: str) -> str:
    """头块翻状态检测（规则 9 机器面）：superseded/historical/Archived → 排除。"""
    for ln in text.splitlines()[:60]:
        if HEADING_RE.match(ln):
            break  # 头块止于首个标题
        m = re.match(r"^Archived:", ln)
        if m:
            return "archived"
        m = re.match(r"^Status:\s*(\S+)", ln)
        if m and m.group(1).strip().lower() in EXCLUDED_STATUS:
            return m.group(1).strip().lower()
    return ""


def _norm_cell(c: str) -> str:
    return c.strip().strip("`").strip().lower()


def _header_verdict(cells: list) -> str:
    """表头判定：full=五列（或五+需）按位全中；near=至少 3 位命中（漂移硬错）；
    None=别的表（静默忽略——实施顺序节里允许放说明性小表）。"""
    hits = 0
    for i, syn in enumerate(COL_SYNONYMS):
        if i < len(cells) and _norm_cell(cells[i]) in syn:
            hits += 1
    wants_ok = len(cells) == 6 and _norm_cell(cells[5]) in WANTS_SYNS
    if hits == 5 and (len(cells) == 5 or wants_ok):
        return "full"
    if hits >= 3:
        return "near"
    return None


def _is_io_title(title: str) -> bool:
    t = title.strip().lower()
    return any(t.startswith(x) for x in IO_TITLES)


def parse_doc(path: Path, rel: str) -> Doc:
    """单文档解析：翻状态 → 实施顺序表行 → 编号节 → 带 ID punch 条目。"""
    text = path.read_text(encoding="utf-8")
    doc = Doc(rel=rel, path=path)
    st = _head_status(text)
    if st:
        doc.excluded = st
        return doc  # 冷历史不进图；其内容腐烂也不许阻塞现行语料（跳过正文解析）
    lines = text.splitlines()
    mask = _fence_mask(lines)
    in_io = False
    table_buf: list = []  # (line_no, text)
    seen_rows: set = set()
    seen_items: dict = {}  # int → tid 原书写（撞号硬错）

    def flush_table() -> None:
        if not table_buf:
            return
        buf, table_buf[:] = list(table_buf), []
        start = buf[0][0]
        cells0 = [c.strip() for c in buf[0][1].strip().strip("|").split("|")]
        if len(buf) < 2 or not all(SEP_CELL_RE.match(c) for c in [x.strip() for x in buf[1][1].strip().strip("|").split("|")] if c):
            doc.notes.append(f"{rel}:{start}: 管线表缺分隔行——按非表忽略")
            return
        verdict = _header_verdict(cells0)
        if verdict is None:
            return  # 说明性小表：不是实施顺序表
        if verdict == "near":
            raise FormatError(rel, start, f"表头漂移（{cells0}）——五列闭集：状态/任务/交付物/依赖/验收（第 6 列可选「需」）")
        ncols = len(cells0)
        for ln_no, ln in buf[2:]:
            cells = [c.strip() for c in ln.strip().strip("|").split("|")]
            if len(cells) != ncols:
                raise FormatError(rel, ln_no, f"列数 {len(cells)}≠{ncols}——防雷列规①：单元格禁竖线（写 / 代替）")
            marker_m = re.match(r"^\[([ xX/])\]", cells[0])
            if cells[0].startswith(PARK_MARK):  # 表行裸 park 形（与 punch 同形合法）
                state, park_reason = "parked", ""
                pm = re.search(r"\[PARKED→([^]]*)\]", cells[0])
                park_reason = pm.group(1).strip() if pm else cells[0].strip()
            elif not marker_m:
                raise FormatError(rel, ln_no, f"状态格 {cells[0]!r} 不在闭集 [x]/[/]/[ ]/[PARKED→…]")
            else:
                state, park_reason = classify_marker(marker_m.group(1), cells[0][marker_m.end():])
            sec_m = re.match(r"^§(\d+)$", cells[1])
            if not sec_m:
                raise FormatError(rel, ln_no, f"任务格 {cells[1]!r} 须为本文档节号 §N（一行一节）")
            num = int(sec_m.group(1))
            if num in seen_rows:
                raise FormatError(rel, ln_no, f"撞节号：§{num} 已有表行（ref names two sections——歧义拒判）")
            seen_rows.add(num)
            doc.rows.append(Row(num=num, state=state, park_reason=park_reason,
                                deliverable=cells[2], needs_raw=cells[3].strip().strip("`").strip(),
                                wants=(cells[5] if ncols == 6 else ""), accept=cells[4] if ncols > 4 else "", line=ln_no))

    for i, ln in enumerate(lines, start=1):
        if not mask[i - 1]:
            hm = HEADING_RE.match(ln)
            if hm:
                lvl, title = len(hm.group(1)), hm.group(2).strip()
                flush_table()
                if lvl <= 2:  # ## 级开关窗；### 以下留在窗内（小节含说明表不顶窗）
                    if not in_io and _is_io_title(title):
                        in_io = True
                    elif in_io:
                        in_io = False
                nm = NUM_HEAD_RE.match(title)
                if nm:
                    n = int(nm.group(1)) if nm.group(1).isdigit() else _cn_num(nm.group(1))
                    if n is not None:
                        if n in doc.headings:
                            doc.dup_headings.append(n)  # 撞号不猜第一个——check 点名
                        else:
                            doc.headings[n] = title
                continue
            if in_io:
                s = ln.strip()
                if s.startswith("｜"):
                    raise FormatError(rel, i, "全角竖线行——会被 Markdown 吞掉（整行不可见），防雷列规①：写 / 代替")
                if s.startswith("|"):
                    table_buf.append((i, ln))
                    continue
                if not s:
                    flush_table()
                    continue
        pm = PUNCH_T_RE.match(ln)
        if pm and not mask[i - 1]:
            rest = pm.group(2)
            tm = T_ID_RE.search(rest)
            if tm:
                state, park_reason = classify_marker(pm.group(1), rest)
                tid, num = tm.group(0)[:-1], int(tm.group(1))
                if num in seen_items:
                    raise FormatError(rel, i, f"撞 T 号：T{tm.group(1)} 已有条目（歧义拒判）")
                seen_items[num] = tid
                label = rest[tm.end():].strip()
                doc.items.append(TItem(tid=tid, num=num, state=state, park_reason=park_reason,
                                       label=label, line=i))
    flush_table()
    return doc


def load_corpus(root: Path, plans: Path) -> dict:
    """收集 plans/ 全树 .md。为什么目录缺席是 rc 2 而空目录不是：--plans 拼错
    若静默当空语料，「队列空」就是谎言；真空目录是诚实的空。"""
    if not plans.is_dir():
        raise RefuseError(f"plans 目录不存在：{plans}（默认 <root>/.agents/plans）")
    docs: dict = {}
    for p in sorted(plans.rglob("*.md")):
        rel_p = p.relative_to(plans)
        if any(part.startswith(".") for part in rel_p.parts):
            continue  # 隐藏目录（.git 等）不是语料
        try:
            rel = p.relative_to(root).as_posix()  # 同侧未解析对未解析——相对部即稳定地址
        except ValueError:
            try:
                rel = p.resolve().relative_to(root.resolve()).as_posix()  # 双侧同解析（/var 与 /private/var 前缀陷阱）
            except ValueError:
                rel = p.as_posix()
        try:
            docs[rel] = parse_doc(p, rel)
        except FormatError:
            raise
        except (OSError, UnicodeDecodeError) as e:
            raise RefuseError(f"{rel}: 读不了（{e}）——状态载不入即拒判，不静默跳过")
    return docs


# --- 依赖格 tokenizer ---------------------------------------------------------


def _refish(tok: str) -> bool:
    """长得像引用的散文 token：保留给校验器点名（假阴性与误报的分界）。
    纯散文（等 icount 收尾）静默跳过——防雷列规③：散文依赖写正文不进格。"""
    t = tok.strip(_STRIP_TOK)
    if not t:
        return False
    return bool(re.match(r"^(?:TODO-|§|D\d|T\d|\.{1,2}/|\d{1,2}-)", t)) or t.endswith(".md")


def parse_dep_cell(cell: str):
    """依赖格 → (groups, near_misses)。group={target, sections, items}：
    target=None 即本文档；链接换车，§N/T## 搭当前车（「T08 §8, §9」一组不散架）；
    区间 §1-§3 展开；空 section/item 的显式链接=文档级判定。"""
    groups, misses = [], []
    if cell.strip().strip("`").strip() in NEEDS_NONE:
        return groups, misses
    cur_t, secs, items, opened = None, [], [], False

    def close():
        nonlocal cur_t, secs, items, opened
        if opened and (secs or items or cur_t is not None):
            groups.append({"target": cur_t, "sections": list(dict.fromkeys(secs)),
                           "items": list(dict.fromkeys(items))})
        cur_t, secs, items, opened = None, [], [], False

    for m in _DEP_TOKEN_RE.finditer(cell):
        if m.group("link"):
            close()
            raw = m.group("link")
            inner = re.match(r"^\[[^\[\]]*\]\(([^()]*)\)$", raw).group(1)
            path_part, _, frag = inner.partition("#")
            cur_t = path_part if path_part else None  # 纯锚链接(#§2)=本文档
            opened = True
            fm = re.match(r"^§?(\d+)$", frag)
            if fm:
                secs.append(int(fm.group(1)))
            else:
                fm2 = re.match(r"^T(\d+)$", frag, re.I)
                if fm2:
                    items.append(int(fm2.group(1)))
            # 未识别 fragment=html 锚（如 #背景）——不是任务引用，忽略
        elif m.group("sec"):
            if not opened:
                opened = True  # 裸 § 开隐式本文档组
            a, b = int(m.group(3)), (int(m.group(4)) if m.group(4) else None)
            if b is None:
                secs.append(a)
            elif b < a:
                secs.append(a)  # 倒序区间降级为单节（原型同款容错，语法面不变）
            else:
                secs.extend(range(a, b + 1))
        elif m.group("item"):
            if not opened:
                opened = True
            items.append(int(m.group(6)))  # 组号：link=1｜sec=2(3,4 数字)｜item=5(6 数字)
        elif m.group("sep"):
            continue
        else:
            tok = m.group("other").strip(_STRIP_TOK)
            if tok and _refish(tok):
                misses.append(tok)
    close()
    return groups, misses


# --- 图构建 -------------------------------------------------------------------


class Node:
    def __init__(self, id, doc_rel, kind, num, state, deliverable="", label="",
                 wants="", park_reason="", line=0, src=None):
        self.id = id
        self.doc_rel = doc_rel
        self.kind = kind  # sec | item
        self.num = num
        self.state = state
        self.deliverable = deliverable
        self.label = label
        self.wants = wants
        self.park_reason = park_reason
        self.line = line
        self.src = src


class Edge:
    def __init__(self, src, dst, target, state, detail=""):
        self.src = src
        self.dst = dst  # None=非节点目标（文档级/未决 token——dot 画幻影）
        self.target = target  # 人读目标描述（链接原文/§N/T##）
        self.state = state
        self.detail = detail


class Finding:
    def __init__(self, cls, rel, line, node, detail):
        self.cls = cls
        self.rel = rel
        self.line = line
        self.node = node
        self.detail = detail

    def render(self) -> str:
        at = f"{self.rel}:{self.line}" if self.line else self.rel
        return f"[{self.cls}] {at}  {self.node}  {self.detail}"


def _doc_state(doc: Doc) -> str:
    """文档级三态（triage 四的图面）：全 done 才 done；任一 needs_work 即 open；
    余者（done+parked）=parked；无条目=noncarrier。"""
    nodes = list(doc.rows) + list(doc.items)
    if not nodes:
        return "noncarrier"
    if any(n.state in ("open", "progress") for n in nodes):
        return "open"
    if any(n.state == "parked" for n in nodes):
        return "parked"
    return "done"


class Graph:
    def __init__(self, corpus: dict, root: Path, plans: Path):
        self.root, self.plans, self.corpus = root, plans, corpus
        self.nodes: dict = {}
        self.idx: dict = {}  # (rel, kind, num) → Node
        self.edges: list = []
        self._edge_keys: set = set()
        for rel, doc in sorted(corpus.items()):
            if doc.excluded:
                continue
            for r in doc.rows:
                n = Node(id=f"{rel}#§{r.num}", doc_rel=rel, kind="sec", num=r.num, state=r.state,
                         deliverable=r.deliverable, wants=r.wants, park_reason=r.park_reason,
                         line=r.line, src=doc)
                self.nodes[n.id] = n
                self.idx[(rel, "sec", r.num)] = n
            for it in doc.items:
                n = Node(id=f"{rel}#{it.tid}", doc_rel=rel, kind="item", num=it.num, state=it.state,
                         label=it.label, park_reason=it.park_reason, line=it.line, src=doc)
                self.nodes[n.id] = n
                self.idx[(rel, "item", it.num)] = n
        self._assemble()

    # -- 边判定（八态的 BerryOS 收窄六态+self） --

    def _resolve_target_rel(self, doc: Doc, raw: str):
        """链接目标 → (rel 或 None=语料外, 磁盘存在, 绝对路径)。相对本文档解析；
        解析回源文档自身=别名（归 self 车辆）。"""
        p = (doc.path.parent / raw).resolve()
        exists = p.is_file()
        try:
            rel = p.relative_to(self.root).as_posix()
            if rel.startswith("../"):
                return None, exists, p
            return rel, exists, p
        except ValueError:
            return None, exists, p

    def _target_edge_state(self, doc: Doc, rel: str, exists: bool, p: Path, kind: str, num: int, src: Node):
        """单目标判定。返回 (dst_id, state, detail)。"""
        tdoc = self.corpus.get(rel)
        if tdoc is None or tdoc.excluded:
            if tdoc is not None and tdoc.excluded:
                return None, "dangling", f"目标文档已 {tdoc.excluded}（规则 4：翻状态须同批修入链）"
            if rel is None:
                if not exists:
                    return None, "dangling", f"目标文档不存在：{p}"
                return None, "non_node", f"目标在 plans 语料外（{p}）——非任务载体"
            if not exists:
                return None, "dangling", f"目标文档不在语料：{rel}"
            return None, "non_node", f"{rel} 无实施顺序表/带 ID 条目——非任务载体"
        key = (rel, kind, num)
        if key not in self.idx:
            if kind == "sec" and not tdoc.rows:
                return None, "dangling", f"{rel} 无实施顺序表（引用节 §{num} 无处落）"
            what = f"§{num}" if kind == "sec" else f"T{num:02d}"
            return None, "dangling", f"目标 {what} 不在 {rel} 的条目集"
        dn = self.idx[key]
        if dn.id == src.id:
            return dn.id, "self", "自依赖（执行序写进了节号格？）"
        if dn.state == "done":
            return dn.id, "satisfied", ""
        if dn.state == "parked":
            return dn.id, "parked", dn.park_reason
        return dn.id, "waiting", ""

    def _add_edge(self, src: Node, dst: str, target: str, state: str, detail: str) -> None:
        k = (src.id, dst, target)
        if k in self._edge_keys:  # 双层去重：组内 [6,6] 与跨组同目标一视同仁
            return
        self._edge_keys.add(k)
        self.edges.append(Edge(src=src.id, dst=dst, target=target, state=state, detail=detail))

    def _assemble(self) -> None:
        for nid, n in sorted(self.nodes.items()):
            doc = n.src
            groups: list = []
            misses: list = []
            if n.kind == "sec":
                row = next(r for r in doc.rows if r.num == n.num)
                g, ms = parse_dep_cell(row.needs_raw)
                groups, misses = g, ms
            else:
                it = next(x for x in doc.items if x.num == n.num)
                raw_line = it.label + (f" [PARKED→{it.park_reason}]" if it.park_reason else "")
                for xm in XIAN_NOTE_RE.finditer(raw_line):
                    g, ms = parse_dep_cell(xm.group(1))
                    groups.extend(g)  # 多个「（先 …）」注记全部收
                    misses.extend(ms)
            for tok in misses:
                self._add_edge(n, None, tok, "unresolved", f"疑似引用未解析（闭集：§N / T## / [链接](相对路径)）")
            for g in groups:
                if g["target"] is None and not g["sections"] and not g["items"]:
                    continue  # 空隐式组（纯散文格）不产边
                if g["target"] is None:
                    tdoc = doc
                else:
                    rel, exists, p = self._resolve_target_rel(doc, g["target"])
                    tdoc = self.corpus.get(rel) if rel else None
                for s in g["sections"]:
                    if tdoc is doc:
                        dst, state, detail = self._target_edge_state(doc, doc.rel, True, doc.path, "sec", s, n)
                        self._add_edge(n, dst, f"§{s}", state, detail)
                    else:
                        dst, state, detail = self._target_edge_state(doc, rel, exists, p, "sec", s, n)
                        self._add_edge(n, dst, f"{g['target']} §{s}", state, detail)
                for t in g["items"]:
                    if tdoc is doc:
                        dst, state, detail = self._target_edge_state(doc, doc.rel, True, doc.path, "item", t, n)
                        self._add_edge(n, dst, f"T{t:02d}", state, detail)
                    else:
                        dst, state, detail = self._target_edge_state(doc, rel, exists, p, "item", t, n)
                        self._add_edge(n, dst, f"{g['target']} T{t:02d}", state, detail)
                if not g["sections"] and not g["items"]:  # 显式链接无节=文档级判定
                    if tdoc is doc:
                        self._add_edge(n, None, g["target"], "self", "文档级自指（依赖整个本文档=悖论）")
                        continue
                    if tdoc is not None and tdoc.excluded:
                        self._add_edge(n, None, g["target"], "dangling",
                                       f"目标文档已 {tdoc.excluded}（规则 4：翻状态须同批修入链）")
                        continue
                    if tdoc is None:
                        if rel is None:
                            if exists:
                                self._add_edge(n, None, g["target"], "non_node", f"目标在 plans 语料外（{p}）——非任务载体")
                            else:
                                self._add_edge(n, None, g["target"], "dangling", f"目标文档不存在：{p}")
                        elif not exists:
                            self._add_edge(n, None, g["target"], "dangling", f"目标文档不在语料：{rel}")
                        else:
                            self._add_edge(n, None, g["target"], "non_node", f"{rel} 无实施顺序表/带 ID 条目——非任务载体")
                        continue
                    ds = _doc_state(tdoc)
                    if ds == "done":
                        self._add_edge(n, None, g["target"], "satisfied", "")
                    elif ds == "open":
                        self._add_edge(n, None, g["target"], "waiting", "等目标文档全量收口")
                    elif ds == "parked":
                        self._add_edge(n, None, g["target"], "parked", "目标文档余量皆 park")
                    else:
                        self._add_edge(n, None, g["target"], "non_node", f"{rel} 非任务载体")

    # -- 检查面 --

    def check(self) -> list:
        findings: list = []
        by_src: dict = {}
        for e in self.edges:
            by_src.setdefault(e.src, []).append(e)
        # ① 自依赖：无状态豁免——执行序进节号格是格式事故，done 也点名
        for e in self.edges:
            if e.state == "self":
                n = self.nodes[e.src]
                findings.append(Finding("自依赖", n.doc_rel, n.line, e.src, e.detail))
        # ② 环：等待缩面（双端皆 open/progress；parked 不入——unpark 后自然复检）
        adj: dict = {}
        for e in self.edges:
            if e.dst and e.state == "waiting":
                a, b = self.nodes.get(e.src), self.nodes.get(e.dst)
                if a and b and a.state in ("open", "progress") and b.state in ("open", "progress"):
                    adj.setdefault(a.id, []).append(b.id)
        for comp in _sccs(adj):
            if len(comp) > 1:
                first = self.nodes[comp[0]]
                findings.append(Finding("环", first.doc_rel, first.line, " ↔ ".join(sorted(comp)),
                                        "多节点强连通——成员永不可建；自环另案（见自依赖）"))
        # ③ 死因三族：只对 open 依赖方报（done/parked 留历史依赖不追——规则 4 的
        #    机器执法面：翻状态不同批修入链=死边红）
        for e in self.edges:
            if e.state in ("dangling", "non_node", "unresolved"):
                n = self.nodes[e.src]
                if n.state in ("open", "progress"):
                    findings.append(Finding({"dangling": "悬空", "non_node": "非载体", "unresolved": "未决"}[e.state],
                                            n.doc_rel, n.line, e.src, f"← {e.target}：{e.detail}"))
        # ④ 孤儿：表行 ↔ 正文编号节互锚（原型 orphan-io-row 语义）——表是节号
        #    地址簿，行无所指=地址悬空；节无所行=工作面未纳管
        for rel, doc in sorted(self.corpus.items()):
            if doc.excluded or not doc.rows:
                continue
            for n in doc.dup_headings:
                findings.append(Finding("节号撞名", rel, 0, f"{rel}#§{n}", "正文编号节撞号——歧义拒判（不猜第一个）"))
            row_ns = {r.num for r in doc.rows}
            for r in doc.rows:
                if r.num not in doc.headings:
                    findings.append(Finding("孤儿", rel, r.line, f"{rel}#§{r.num}",
                                            f"表行 §{r.num} 无对应正文编号节（## {r.num}. 标题）——节点地址悬空"))
            for hn, htitle in sorted(doc.headings.items()):
                if hn not in row_ns:
                    findings.append(Finding("孤儿", rel, 0, f"{rel}#§{hn}",
                                            f"正文编号节 {hn}「{htitle}」无表行——工作面未纳管"))
        # ⑤ 需列闭集（可选列的值域执法）
        for rel, doc in sorted(self.corpus.items()):
            if doc.excluded:
                continue
            for r in doc.rows:
                if r.wants and r.wants not in WANTS_CLOSED:
                    findings.append(Finding("闭集", rel, r.line, f"{rel}#§{r.num}",
                                            f"需列值 {r.wants!r} 不在闭集 {sorted(WANTS_CLOSED - {'', '--'})}"))
        return findings

    # -- 可建造面 --

    def next_items(self) -> dict:
        by_src: dict = {}
        for e in self.edges:
            by_src.setdefault(e.src, []).append(e)
        ready, waiting, parked = [], [], []
        for nid, n in sorted(self.nodes.items()):
            if n.state == "done":
                continue  # done 不进任何面——清单只回答「还有什么可做/在等什么」
            if n.state == "parked":
                parked.append({"id": nid, "reason": n.park_reason})
                continue
            bad = [e for e in by_src.get(nid, []) if e.state != "satisfied"]
            if n.state == "progress":
                # 已交付欠审：评审不被依赖阻塞故恒可跑（triage 五）
                ready.append({"id": nid, "kind": n.kind, "state": n.state,
                              "label": (n.deliverable or n.label), "note": "只欠评审"})
            elif not bad:
                ready.append({"id": nid, "kind": n.kind, "state": n.state,
                              "label": (n.deliverable or n.label), "note": ""})
            else:
                waiting.append({"id": nid,
                                "blockers": [{"id": e.dst or e.target, "state": e.state} for e in bad]})
        counts = {
            "nodes": len(self.nodes),
            "done": sum(1 for n in self.nodes.values() if n.state == "done"),
            "progress": sum(1 for n in self.nodes.values() if n.state == "progress"),
            "open": sum(1 for n in self.nodes.values() if n.state == "open"),
            "parked": len(parked),
            "edges": len(self.edges),
        }
        return {"ready": ready, "waiting": waiting, "parked": parked, "counts": counts}

    def topo_order(self) -> list:
        """堆顶 Kahn +（载体, 行号）字典序 tiebreak——给规划消费面的全序视图。"""
        import heapq
        indeg: dict = {nid: 0 for nid in self.nodes}
        adj: dict = {nid: [] for nid in self.nodes}
        for e in self.edges:
            if e.dst and e.dst in self.nodes and e.state == "waiting":
                adj[e.src].append(e.dst)
                indeg[e.dst] += 1
        key = lambda nid: (self.nodes[nid].doc_rel, self.nodes[nid].line, nid)
        h = [(key(nid), nid) for nid, d in indeg.items() if d == 0]
        heapq.heapify(h)
        out, seen = [], set()
        while h:
            _, nid = heapq.heappop(h)
            if nid in seen:
                continue
            seen.add(nid)
            out.append(nid)
            for nxt in adj[nid]:
                indeg[nxt] -= 1
                if indeg[nxt] == 0:
                    heapq.heappush(h, (key(nxt), nxt))
        for nid in self.nodes:  # 环上节点（入度永不清零）按位次补尾——全序不缺员
            if nid not in seen:
                out.append(nid)
        return out

    # -- 渲染面 --

    def to_dot(self) -> str:
        colors = {"done": "lightgrey", "progress": "lightblue", "open": "white", "parked": "khaki"}
        out = ["digraph task_graph {", '  rankdir=LR; node [shape=box style="filled" fontname="Helvetica"];']
        for nid, n in sorted(self.nodes.items()):
            label = (n.deliverable or n.label or nid).replace('"', "'")
            if len(label) > 48:
                label = label[:48] + "…"
            out.append(f'  "{nid}" [label="{nid}\\n{label}", fillcolor={colors[n.state]}];')
        phantom: set = set()
        for e in self.edges:
            style, color = {
                "satisfied": ("solid", "grey"), "waiting": ("solid", "black"),
                "parked": ("dashed", "darkgoldenrod"), "self": ("solid", "red"),
                "dangling": ("dotted", "red"), "non_node": ("dotted", "red"), "unresolved": ("dotted", "red"),
            }[e.state]
            dst = e.dst
            if not dst:
                dst = f"?{e.target}"
                if dst not in phantom:
                    phantom.add(dst)
                    out.append(f'  "{dst}" [label="{e.target}", shape=ellipse, color=red, style=dashed];')
            out.append(f'  "{e.src}" -> "{dst}" [style={style}, color={color}];')
        out.append("}")
        return "\n".join(out)

    def summary(self) -> dict:
        ni = self.next_items()
        return {"docs": len(self.corpus),
                "excluded": sum(1 for d in self.corpus.values() if d.excluded),
                "tables": sum(1 for d in self.corpus.values() if d.rows),
                "rows": sum(len(d.rows) for d in self.corpus.values() if not d.excluded),
                "items": sum(len(d.items) for d in self.corpus.values() if not d.excluded),
                "edges": len(self.edges), "node_counts": ni["counts"]}

    def to_payload(self) -> dict:
        """缓存负载：图自证所需全量（节点+边+文档锚面）。"""
        return {
            "schema_version": SCHEMA_VERSION, "tool_version": TOOL_VERSION,
            "plans_rel": self.plans.resolve().relative_to(self.root).as_posix()
            if self.plans.resolve().is_relative_to(self.root) else self.plans.as_posix(),
            "nodes": [{"id": n.id, "doc": n.doc_rel, "kind": n.kind, "num": n.num, "state": n.state,
                       "deliverable": n.deliverable, "label": n.label, "wants": n.wants,
                       "park_reason": n.park_reason, "line": n.line}
                      for _, n in sorted(self.nodes.items())],
            "edges": [{"src": e.src, "dst": e.dst, "target": e.target, "state": e.state, "detail": e.detail}
                      for e in sorted(self.edges, key=lambda x: (x.src, x.dst or "", x.target))],
            "docs": {rel: {"excluded": d.excluded,
                           "headings": {str(k): v for k, v in sorted(d.headings.items())},
                           "dup_headings": d.dup_headings,
                           "has_table": bool(d.rows)}
                     for rel, d in sorted(self.corpus.items())},
            "summary": self.summary(),
        }


def _sccs(adj: dict) -> list:
    """迭代 Tarjan SCC（栈式免递归深度限制——语料大时递归会爆栈）。"""
    index, low, onstk, stack, counter, result = {}, {}, set(), [], [0], []
    for start in sorted(adj):
        if start in index:
            continue
        index[start] = low[start] = counter[0]
        counter[0] += 1
        stack.append(start)
        onstk.add(start)
        work = [(start, iter(sorted(adj[start])))]
        while work:
            v, it = work[-1]
            pushed = False
            for w in it:
                if w not in index:
                    index[w] = low[w] = counter[0]
                    counter[0] += 1
                    stack.append(w)
                    onstk.add(w)
                    work.append((w, iter(sorted(adj.get(w, [])))))
                    pushed = True
                    break
                if w in onstk:
                    low[v] = min(low[v], index[w])
            if not pushed:
                work.pop()
                if work:
                    pv = work[-1][0]
                    low[pv] = min(low[pv], low[v])
                if low[v] == index[v]:
                    comp = []
                    while True:
                        w = stack.pop()
                        onstk.discard(w)
                        comp.append(w)
                        if w == v:
                            break
                    result.append(comp)
    return result


# --- 缓存面（写原子 + 读必验证） ----------------------------------------------


def fingerprint(root: Path, plans: Path) -> str:
    """输入指纹：语料全文件内容哈希 + schema/tool 版本。为什么把版本编进指纹：
    语义变了旧缓存即使输入未变也陈——版本盐让旧件自动拒读。"""
    h = hashlib.sha256()
    h.update(f"{SCHEMA_VERSION}|{TOOL_VERSION}|".encode())
    if plans.is_dir():
        for p in sorted(plans.rglob("*.md")):
            if any(part.startswith(".") for part in p.relative_to(plans).parts):
                continue
            try:
                rel = p.resolve().relative_to(root).as_posix()
            except (OSError, ValueError):
                rel = p.as_posix()
            h.update(rel.encode())
            h.update(b"\0")
            h.update(hashlib.sha256(p.read_bytes()).digest())
    return h.hexdigest()


def cache_path_for(root: Path, override: str = "") -> Path:
    return Path(override) if override else root / ".claude" / "state" / "task-graph.json"


def write_cache(path: Path, payload: dict, fp: str) -> None:
    """原子写：tmp 同目录 + fsync + rename；POSIX 下加 flock 防并发交错。
    为什么不裸写：截断件被读路径当真相 = 一切分类 DONE 的静默谎言。"""
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = dict(payload)
    payload["fingerprint"] = fp
    lock = None
    try:
        lock = open(str(path) + ".lock", "w")
        if fcntl is not None:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
        fd, tmp = tempfile.mkstemp(dir=str(path.parent), prefix=".task-graph-", suffix=".json")
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as f:
                json.dump(payload, f, ensure_ascii=False, indent=1)
                f.flush()
                os.fsync(f.fileno())
            os.replace(tmp, path)
            try:
                dfd = os.open(str(path.parent), os.O_RDONLY)
                try:
                    os.fsync(dfd)
                finally:
                    os.close(dfd)
            except OSError:
                pass  # 目录 fsync 是加保险不是正确性前提（rename 已原子）
        finally:
            if os.path.exists(tmp):
                os.unlink(tmp)
    finally:
        if lock is not None:
            lock.close()


def load_cache(path: Path, root: Path, plans: Path) -> dict:
    """读必验证：JSON 完整性 + schema 形状 + 输入指纹对拍。任何不合 → 拒判
    （rc 2 由调用方落地）——绝不让坏件静默当空图/全 done 图。"""
    try:
        raw = path.read_text(encoding="utf-8")
        payload = json.loads(raw)
    except (OSError, ValueError) as e:
        raise RefuseError(f"缓存读不了/坏 JSON（{path}：{e}）——重跑 cache 子命令重建")
    if not isinstance(payload, dict):
        raise RefuseError("缓存形状坏（顶层非对象）——重跑 cache 子命令重建")
    for k in ("schema_version", "tool_version", "fingerprint", "nodes", "edges", "docs", "summary"):
        if k not in payload:
            raise RefuseError(f"缓存缺键 {k}——schema 不合，重跑 cache 子命令重建")
    if payload["schema_version"] != SCHEMA_VERSION or payload["tool_version"] != TOOL_VERSION:
        raise RefuseError(f"缓存 schema/tool 版本不合（{payload['schema_version']}/{payload['tool_version']}"
                          f" ≠ {SCHEMA_VERSION}/{TOOL_VERSION}）——重跑 cache 子命令重建")
    ids = set()
    for n in payload["nodes"]:
        if not isinstance(n, dict) or not all(k in n for k in ("id", "doc", "kind", "num", "state")):
            raise RefuseError("缓存节点形状坏——重跑 cache 子命令重建")
        if n["state"] not in NODE_STATES:
            raise RefuseError(f"缓存节点状态 {n['state']!r} 不在闭集——重跑 cache 子命令重建")
        if n["id"] in ids:
            raise RefuseError(f"缓存撞节点 id {n['id']}——重跑 cache 子命令重建")
        ids.add(n["id"])
    for e in payload["edges"]:
        if not isinstance(e, dict) or not all(k in e for k in ("src", "dst", "target", "state")):
            raise RefuseError("缓存边形状坏——重跑 cache 子命令重建")
        if e["state"] not in EDGE_STATES:
            raise RefuseError(f"缓存边状态 {e['state']!r} 不在闭集——重跑 cache 子命令重建")
        if e["state"] != "unresolved" and e["src"] not in ids:
            raise RefuseError(f"缓存边引用未知源 {e['src']}——重跑 cache 子命令重建")
    fp_now = fingerprint(root, plans)
    if payload["fingerprint"] != fp_now:
        raise RefuseError("缓存陈（输入已变：文件增删改都会翻指纹）——重跑 cache 子命令重建")
    return payload


def graph_from_payload(payload: dict, root: Path, plans: Path) -> Graph:
    """缓存 → 图（不回读语料）。headings/排除态都在负载里——check 的孤儿腿
    离线可跑，这就是缓存要存 docs 锚面的原因。"""
    g = Graph.__new__(Graph)
    g.root, g.plans, g.corpus = root, plans, {}
    g.nodes, g.idx, g.edges, g._edge_keys = {}, {}, [], set()
    for rel, d in payload["docs"].items():
        doc = Doc(rel=rel, path=Path(root, rel))
        doc.excluded = d.get("excluded", "")
        doc.headings = {int(k): v for k, v in d.get("headings", {}).items()}
        doc.dup_headings = list(d.get("dup_headings", []))
        g.corpus[rel] = doc
    for n in payload["nodes"]:
        node = Node(id=n["id"], doc_rel=n["doc"], kind=n["kind"], num=n["num"], state=n["state"],
                    deliverable=n.get("deliverable", ""), label=n.get("label", ""),
                    wants=n.get("wants", ""), park_reason=n.get("park_reason", ""), line=n.get("line", 0),
                    src=g.corpus.get(n["doc"]))
        g.nodes[node.id] = node
        g.idx[(n["doc"], n["kind"], n["num"])] = node
        if node.kind == "sec":  # 孤儿腿要行号集合：由节点回填伪行（状态/行号足用）
            doc = g.corpus.get(n["doc"])
            if doc is not None:
                doc.rows.append(Row(num=node.num, state=node.state, park_reason="",
                                    deliverable=node.deliverable, needs_raw="", wants=node.wants,
                                    accept="", line=node.line))
    for e in payload["edges"]:
        g.edges.append(Edge(src=e["src"], dst=e["dst"], target=e["target"],
                            state=e["state"], detail=e.get("detail", "")))
    return g


# --- 汇报渲染 -----------------------------------------------------------------


def _print_next(ni: dict, as_json: bool) -> None:
    if as_json:
        print(json.dumps(ni, ensure_ascii=False, indent=1))
        return
    c = ni["counts"]
    print(f"可建造 READY（{len(ni['ready'])}）：")
    for r in ni["ready"]:
        note = f"（{r['note']}）" if r["note"] else ""
        print(f"  {r['id']}  {r['label']}{note}")
    print(f"等待 WAITING（{len(ni['waiting'])}）：")
    for w in ni["waiting"]:
        bl = ", ".join(f"{b['id']}({b['state']})" for b in w["blockers"])
        print(f"  {w['id']}  ← {bl}")
    print(f"停车 PARKED（{len(ni['parked'])}）：")
    for p in ni["parked"]:
        print(f"  {p['id']}  [PARKED→人审: {p['reason']}]")
    print(f"计数：节点 {c['nodes']}（done {c['done']}/progress {c['progress']}/open {c['open']}/parked {c['parked']}）· 边 {c['edges']}")
    if c["nodes"] == 0:
        print("语料无任务节点（plans/ 下无实施顺序表、无带 ID punch 条目）——空集是诚实答案，非拒判")


def _print_parse(g: Graph, as_json: bool) -> None:
    if as_json:
        print(json.dumps(g.to_payload(), ensure_ascii=False, indent=1))
        return
    s = g.summary()
    print(f"语料 {s['docs']} 篇（翻状态跳过 {s['excluded']}）· 表 {s['tables']} · 行 {s['rows']} · "
          f"带 ID 条目 {s['items']} · 边 {s['edges']}")
    for rel, d in sorted(g.corpus.items()):
        if d.excluded:
            print(f"  跳过 {rel}（{d.excluded}）")
        for note in d.notes:
            print(f"  注 {note}")


# --- CLI ----------------------------------------------------------------------


def _build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="task-graph.py", description="任务 DAG 图构建器+校验器（exit 0/1/2：正常/红发现/拒判）")
    sub = p.add_subparsers(dest="cmd", required=True)
    for name in ("parse", "check", "next", "dot", "cache"):
        sp = sub.add_parser(name)
        sp.add_argument("-C", "--root", default=".", help="仓根（默认 cwd）")
        sp.add_argument("--plans", default="", help="plans 目录（默认 <root>/.agents/plans）")
        sp.add_argument("--json", action="store_true", help="机读输出")
        sp.add_argument("--from-cache", action="store_true", help="读缓存代语料解析（坏/陈 → rc 2 拒判）")
        sp.add_argument("--cache-path", default="", help="缓存件路径（默认 <root>/.claude/state/task-graph.json）")
    return p


def run(argv: list) -> int:
    if "--selftest" in argv:
        return selftest()
    args = _build_parser().parse_args(argv)
    root = Path(args.root).resolve()
    plans = Path(args.plans).resolve() if args.plans else root / ".agents" / "plans"
    try:
        if args.from_cache:
            payload = load_cache(cache_path_for(root, args.cache_path), root, plans)
            graph = graph_from_payload(payload, root, plans)
        else:
            graph = Graph(load_corpus(root, plans), root, plans)
    except RefuseError as e:
        print(f"task-graph: 拒判——{e}", file=sys.stderr)
        return EXIT_REFUSE
    except FormatError as e:
        print(f"task-graph: 载体畸形——{e}", file=sys.stderr)
        return EXIT_REFUSE
    if args.cmd == "parse":
        _print_parse(graph, args.json)
        return EXIT_OK
    if args.cmd == "check":
        findings = graph.check()
        s = graph.summary()
        if args.json:  # --json 独占 stdout——机读消费面不与人读行混流
            print(json.dumps({"findings": [f.__dict__ for f in findings], "summary": s},
                             ensure_ascii=False, indent=1))
            return EXIT_RED if findings else EXIT_OK
        for f in findings:
            print(f.render())
        print(f"检查：节点 {s['node_counts']['nodes']} · 边 {s['edges']} · 红 {len(findings)}"
              f"（done {s['node_counts']['done']}/progress {s['node_counts']['progress']}"
              f"/open {s['node_counts']['open']}/parked {s['node_counts']['parked']}）")
        return EXIT_RED if findings else EXIT_OK
    if args.cmd == "next":
        _print_next(graph.next_items(), args.json)
        return EXIT_OK
    if args.cmd == "dot":
        print(graph.to_dot())
        return EXIT_OK
    if args.cmd == "cache":
        fp = fingerprint(root, plans)
        payload = graph.to_payload()
        payload["fingerprint"] = fp  # 与落盘件同形（--json 消费面看到的即文件内容）
        path = cache_path_for(root, args.cache_path)
        try:
            write_cache(path, payload, fp)
        except OSError as e:
            print(f"task-graph: 缓写失败——{e}", file=sys.stderr)
            return EXIT_REFUSE
        if args.json:
            print(json.dumps(payload, ensure_ascii=False, indent=1))
        else:
            print(f"缓存写成 {path}（指纹 {fp[:12]}… · 节点 {len(payload['nodes'])} · 边 {len(payload['edges'])}）")
        return EXIT_OK
    return EXIT_REFUSE


# --- 图接缝（task-oracle 契约：ready_rows(root) → [(文件相对路径, 节号字符串)]） ---


def ready_rows(root) -> list:
    """给 task-oracle --next-buildable 的结构可跑面（判定单源分工：本件管图结构，
    oracle 管双戳分类，交集=可建）。只含表行节——oracle 的节点面是编号节，T 条目
    是 punch 队列面不交集。异常向上抛：oracle 捕获后 fail-open（规划故障不停跑）。

    为什么语料零表行时拒判而不是答空集：图面尚未采用时「无可建」是结构缺席冒充
    结构答案——拒判让 oracle 落回 fail-open（一切 NEEDS_WORK 可跑），首表落库
    即自然接管；真有表而全等待时返回 []（那是诚实的结构答案）。"""
    root = Path(root).resolve()  # 归一侧：调用面给未解析路径（/var 前缀）时相对部才不漂
    plans = root / ".agents" / "plans"
    graph = Graph(load_corpus(root, plans), root, plans)
    if not any(n.kind == "sec" for n in graph.nodes.values()):
        raise RefuseError("语料无实施顺序表行——图面未采用，交 oracle 层 fail-open")
    out = []
    for r in graph.next_items()["ready"]:
        n = graph.nodes[r["id"]]
        if n.kind == "sec":
            out.append((n.doc_rel, str(n.num)))
    return out


# --- 自检（合成 plans 树；好图/环/悬空/自依赖/next 集/缓存回路 等 17 腿） --------


GOOD_MAIN = """# 渲染刀设计

Type: spec
Status: active
Version: 1

## 1. 骨架

骨架正文。

## 2. 纹理

## 3. 合成

## 实施顺序

| 状态 | 任务 | 交付物 | 依赖 | 验收 |
| :-: | :-: | --- | :-: | --- |
| [x] | §1 | 骨架代码 | -- | just test-host |
| [ ] | §2 | 纹理上传 | §1 | just test-host |
| [ ] | §3 | 合成路径 | §1, §2 | just image |
| [/] | §4 | 调试开关 | -- | just test-host |

## 4. 调试开关
"""

GOOD_OPS = """# 运营

Type: plan
Status: active

## 未完成事项（punch-list）

- [x] T01·地基：先打地基
- [ ] T02·对接：与渲染刀对接（先 T01）——验收：自测
- [ ] [PARKED→人审: 等裁决] T03·远期：等 owner 定夺

## 1. 桥接层

## 实施顺序

| 状态 | 任务 | 交付物 | 依赖 | 验收 |
| :-: | :-: | --- | :-: | --- |
| [ ] | §1 | 桥接层 | [渲染刀](mod/design.md) §1, [渲染刀](mod/design.md) | just test-host |
"""


def _w(root: Path, rel: str, text: str) -> None:
    p = root / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text, encoding="utf-8")


def _good(root: Path) -> None:
    _w(root, ".agents/plans/mod/design.md", GOOD_MAIN)
    _w(root, ".agents/plans/ops.md", GOOD_OPS)


def _invoke(argv: list):
    """进程内跑 run()（快、可断言退出码）；输出截获——断言失败时回放定位。"""
    import contextlib
    import io as _io
    out, err = _io.StringIO(), _io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        rc = run(list(argv))
    return rc, out.getvalue(), err.getvalue()


def selftest() -> int:  # noqa: C901 —— 自检腿多即长；每腿独立 try 便于点名
    import shutil
    import subprocess
    import tempfile

    results = []
    tmpdirs = []

    def leg(name: str, fn) -> None:
        try:
            ok, detail = fn()
        except Exception:  # noqa: BLE001 —— 自检腿自身炸=腿红，带栈定位
            ok, detail = False, traceback.format_exc(limit=3)
        results.append((name, ok, detail))
        print(f"task-graph 自检: {'腿绿' if ok else 'FAIL'} {name}" + ("" if ok else f"\n{detail}"))

    def mk():
        d = tempfile.mkdtemp(prefix="task-graph-selftest-")
        tmpdirs.append(d)
        return d

    def l_good():
        d = mk()
        r = Path(d)
        _good(r)
        rc, out, err = _invoke(["check", "--root", d])
        return rc == 0 and "红 0" in out, f"rc={rc} out={out!r} err={err!r}"

    def l_next():
        d = mk()
        r = Path(d)
        _good(r)
        rc, out, _ = _invoke(["next", "--root", d, "--json"])
        ni = json.loads(out)
        ready = {x["id"] for x in ni["ready"]}
        want = {".agents/plans/mod/design.md#§2", ".agents/plans/mod/design.md#§4", ".agents/plans/ops.md#T02"}
        wait = {x["id"]: x["blockers"] for x in ni["waiting"]}
        ok = (rc == 0 and ready == want
              and ".agents/plans/mod/design.md#§3" in wait
              and ".agents/plans/ops.md#§1" in wait
              and any(b["state"] == "waiting" for b in wait[".agents/plans/mod/design.md#§3"])
              and {p["id"] for p in ni["parked"]} == {".agents/plans/ops.md#T03"})
        return ok, f"ready={ready} want={want} waiting={list(wait)}"

    def _one_doc(body: str, name: str = "t.md"):
        d = mk()
        _w(Path(d), f".agents/plans/{name}", body)
        return Path(d)

    def l_cycle():
        r = _one_doc("## 1. 甲\n\n## 2. 乙\n\n## 实施顺序\n\n"
                     "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
                     "| [ ] | §1 | a | §2 | x |\n| [ ] | §2 | b | §1 | x |\n")
        rc, out, _ = _invoke(["check", "--root", str(r)])
        return rc == 1 and "[环]" in out, f"rc={rc} out={out!r}"

    def l_dangling():
        r = _one_doc("## 1. 甲\n\n## 实施顺序\n\n"
                     "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
                     "| [ ] | §1 | a | §9 | x |\n")
        rc, out, _ = _invoke(["check", "--root", str(r)])
        return rc == 1 and "[悬空]" in out, f"rc={rc} out={out!r}"

    def l_selfdep():
        r = _one_doc("## 1. 甲\n\n## 实施顺序\n\n"
                     "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
                     "| [ ] | §1 | a | §1 | x |\n")
        rc, out, _ = _invoke(["check", "--root", str(r)])
        return rc == 1 and "[自依赖]" in out, f"rc={rc} out={out!r}"

    def l_orphan():
        r1 = _one_doc("## 1. 甲\n\n## 实施顺序\n\n"
                      "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
                      "| [ ] | §5 | a | -- | x |\n")
        rc1, out1, _ = _invoke(["check", "--root", str(r1)])
        r2 = _one_doc("## 1. 甲\n\n## 2. 乙\n\n## 实施顺序\n\n"
                      "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
                      "| [ ] | §1 | a | -- | x |\n")
        rc2, out2, _ = _invoke(["check", "--root", str(r2)])
        return (rc1 == 1 and "无对应正文编号节" in out1
                and rc2 == 1 and "无表行" in out2), f"rc1={rc1}/{out1!r} rc2={rc2}/{out2!r}"

    def l_unresolved():
        r1 = _one_doc("## 1. 甲\n\n## 实施顺序\n\n"
                      "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
                      "| [ ] | §1 | a | TODO-45 | x |\n")
        rc1, out1, _ = _invoke(["check", "--root", str(r1)])
        r2 = _one_doc("## 1. 甲\n\n## 实施顺序\n\n"
                      "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
                      "| [ ] | §1 | a | 等 icount 收尾 | x |\n")
        rc2, out2, _ = _invoke(["check", "--root", str(r2)])
        return (rc1 == 1 and "[未决]" in out1 and "TODO-45" in out1
                and rc2 == 0), f"rc1={rc1}/{out1!r} rc2={rc2}/{out2!r}"

    def l_park():
        r = _one_doc("## 1. 甲\n\n## 未完成事项\n\n"
                     "- [ ] [PARKED→人审: 等裁决] T01·远期\n\n"
                     "## 实施顺序\n\n"
                     "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
                     "| [ ] | §1 | a | T01 | x |\n")
        rc, out, _ = _invoke(["next", "--root", str(r), "--json"])
        ni = json.loads(out)
        ok = rc == 0 and not ni["ready"] and ni["waiting"] and any(
            b["state"] == "parked" for b in ni["waiting"][0]["blockers"])
        rc2, out2, _ = _invoke(["check", "--root", str(r)])
        return ok and rc2 == 0, f"next={ni} check_rc={rc2}/{out2!r}"

    def l_doclevel():
        d = mk()
        r = Path(d)
        _w(r, ".agents/plans/c.md", "## 1. 旧刀\n\n## 实施顺序\n\n"
           "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
           "| [x] | §1 | 旧 | -- | x |\n")
        _w(r, ".agents/plans/g.md", "# 无表白文档\n")
        _w(r, ".agents/plans/e.md", "## 1. 用家\n\n## 实施顺序\n\n"
           "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
           "| [ ] | §1 | e | [C](c.md) | x |\n")
        _w(r, ".agents/plans/f.md", "## 1. 用家\n\n## 实施顺序\n\n"
           "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
           "| [ ] | §1 | f | [G](g.md) | x |\n")
        rc, out, _ = _invoke(["next", "--root", d, "--json"])
        ni = json.loads(out)
        ready = {x["id"] for x in ni["ready"]}
        rc2, out2, _ = _invoke(["check", "--root", d])
        return (".agents/plans/e.md#§1" in ready
                and rc2 == 1 and "[非载体]" in out2), f"ready={ready} rc2={rc2}/{out2!r}"

    def l_superseded():
        d = mk()
        r = Path(d)
        _w(r, ".agents/plans/h.md", "Status: superseded\n\n# 旧\n\n## 1. 旧节\n\n## 实施顺序\n\n"
           "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
           "| [x] | §1 | 旧 | -- | x |\n")
        _w(r, ".agents/plans/i.md", "## 1. 新\n\n## 实施顺序\n\n"
           "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
           "| [ ] | §1 | 新 | [H](h.md) §1 | x |\n")
        rc, out, _ = _invoke(["check", "--root", d])
        return rc == 1 and "[悬空]" in out and "superseded" in out, f"rc={rc} out={out!r}"

    def l_dup():
        r = _one_doc("## 1. 甲\n\n## 实施顺序\n\n"
                     "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
                     "| [ ] | §1 | a | -- | x |\n| [x] | §1 | 又一个 | -- | x |\n")
        rc1, _, err1 = _invoke(["check", "--root", str(r)])
        r2 = _one_doc("## 未完成事项\n\n- [ ] T07·甲\n- [x] T07·乙\n")
        rc2, _, err2 = _invoke(["check", "--root", str(r2)])
        return (rc1 == 2 and "撞节号" in err1 and rc2 == 2 and "撞 T 号" in err2), f"rc1={rc1}/{err1!r} rc2={rc2}/{err2!r}"

    def l_malformed():
        base_hdr = "| 状态 | 任务 | 交付物 | 依赖 |\n| :-: | :-: | --- | :-: |\n"
        r1 = _one_doc("## 1. 甲\n\n## 实施顺序\n\n" + base_hdr + "| [ ] | §1 | a | -- |\n")
        rc1, _, err1 = _invoke(["check", "--root", str(r1)])
        r2 = _one_doc("## 1. 甲\n\n## 实施顺序\n\n"
                      "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
                      "| [ ] | §1 | a | -- | x | 多一格 |\n")
        rc2, _, err2 = _invoke(["check", "--root", str(r2)])
        r3 = _one_doc("## 1. 甲\n\n## 实施顺序\n\n"
                      "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
                      "| [?] | §1 | a | -- | x |\n")
        rc3, _, err3 = _invoke(["check", "--root", str(r3)])
        r4 = _one_doc("## 1. 甲\n\n## 实施顺序\n\n｜ [ ] ｜ §1 ｜ a ｜ -- ｜ x ｜\n")
        rc4, _, err4 = _invoke(["check", "--root", str(r4)])
        return (rc1 == 2 and "表头漂移" in err1 and rc2 == 2 and "列数" in err2
                and rc3 == 2 and "闭集" in err3 and rc4 == 2 and "全角竖线" in err4), \
               f"{rc1}/{err1!r} {rc2}/{err2!r} {rc3}/{err3!r} {rc4}/{err4!r}"

    def l_range_dedup():
        d = mk()
        r = Path(d)
        _w(r, ".agents/plans/mod/design.md", GOOD_MAIN)
        _w(r, ".agents/plans/r.md", "## 1. a\n\n## 2. b\n\n## 3. c\n\n## 实施顺序\n\n"
           "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
           "| [ ] | §1 | d | §2-§3, §2 | x |\n"
           "| [ ] | §2 | e | [渲染刀](mod/design.md) §1, §2 | x |\n"
           "| [ ] | §3 | f | -- | x |\n")
        rc, out, _ = _invoke(["parse", "--root", d, "--json"])
        pl = json.loads(out)
        es = {(e["src"], e["dst"], e["state"]) for e in pl["edges"]}
        ok = (rc == 0
              and (".agents/plans/r.md#§1", ".agents/plans/r.md#§2", "waiting") in es
              and (".agents/plans/r.md#§1", ".agents/plans/r.md#§3", "waiting") in es
              and (".agents/plans/r.md#§2", ".agents/plans/mod/design.md#§1", "satisfied") in es
              and (".agents/plans/r.md#§2", ".agents/plans/mod/design.md#§2", "waiting") in es
              and len([e for e in pl["edges"] if e["src"] == ".agents/plans/r.md#§1"]) == 2)
        return ok, f"edges={sorted(es)}"

    def l_cache():
        d = mk()
        r = Path(d)
        _good(r)
        rc, out, _ = _invoke(["cache", "--root", d])
        cpath = r / ".claude" / "state" / "task-graph.json"
        rc1, out1, _ = _invoke(["next", "--root", d, "--from-cache", "--json"])
        ni1 = json.loads(out1)
        _w(r, ".agents/plans/ops.md", GOOD_OPS.replace("T02·对接", "T02·对接2"))
        rc2, _, err2 = _invoke(["next", "--root", d, "--from-cache"])
        _w(r, ".agents/plans/ops.md", GOOD_OPS)
        cpath.write_text("{trunc", encoding="utf-8")
        rc3, _, err3 = _invoke(["next", "--root", d, "--from-cache"])
        rc0, _, _ = _invoke(["cache", "--root", d])
        full = json.loads(cpath.read_text())
        full["schema_version"] = 999
        cpath.write_text(json.dumps(full), encoding="utf-8")
        rc4, _, err4 = _invoke(["next", "--root", d, "--from-cache"])
        return (rc == 0 and cpath.is_file()
                and rc1 == 0 and any(x["id"] == ".agents/plans/ops.md#T02" for x in ni1["ready"])
                and rc2 == 2 and "陈" in err2
                and rc3 == 2 and "坏 JSON" in err3
                and rc4 == 2 and "版本不合" in err4), \
               f"rc={rc} rc1={rc1} rc2={rc2}/{err2!r} rc3={rc3}/{err3!r} rc4={rc4}/{err4!r}"

    def l_subprocess():
        d = mk()
        r = Path(d)
        _good(r)
        script = Path(__file__).resolve()
        a = subprocess.run([sys.executable, "-I", str(script), "check", "--root", d],
                           capture_output=True, text=True)
        dc = mk()
        _w(Path(dc), ".agents/plans/c.md", "## 1. 甲\n\n## 实施顺序\n\n"
           "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
           "| [ ] | §1 | a | §1 | x |\n")
        b = subprocess.run([sys.executable, "-I", str(script), "check", "--root", dc],
                           capture_output=True, text=True)
        c = subprocess.run([sys.executable, "-I", str(script), "check", "--root", dc, "--plans", "/nonexistent-xyz"],
                           capture_output=True, text=True)
        h = subprocess.run([sys.executable, "-I", str(script), "--help"], capture_output=True, text=True)
        return (a.returncode == 0 and b.returncode == 1 and c.returncode == 2 and h.returncode == 0), \
               f"a={a.returncode} b={b.returncode} c={c.returncode}/{c.stderr!r} h={h.returncode}"

    def l_fence():
        d = mk()
        r = Path(d)
        body = (
            "## 1. 甲\n\n## 3.1 子节不算编号节\n\n## 实施顺序\n\n"
            "```\n"
            "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
            "| [ ] | §1 | 假 | -- | x |\n| [x] | §1 | 撞 | -- | x |\n"
            "```\n\n"
            "| 状态 | 任务 | 交付物 | 依赖 | 验收 |\n| :-: | :-: | --- | :-: | --- |\n"
            "| [ ] | §1 | 真 | -- | x |\n\n"
            "- [ ] 围栏外的普通 punch 行（无 ID 不进图）\n"
        )
        _w(r, ".agents/plans/f.md", body)
        rc, out, err = _invoke(["check", "--root", d])
        return rc == 0 and "红 0" in out, f"rc={rc} out={out!r} err={err!r}"

    def l_seam():
        d = mk()
        r = Path(d)
        _good(r)
        rows = ready_rows(r)
        want = sorted({(".agents/plans/mod/design.md", "2"), (".agents/plans/mod/design.md", "4")})
        d2 = mk()
        _w(Path(d2), ".agents/plans/x.md", "# 无表白文档\n")
        refused = False
        try:
            ready_rows(Path(d2))
        except RefuseError:
            refused = True
        return (sorted(rows) == want and refused
                and all(isinstance(a, str) and isinstance(b, str) for a, b in rows)), \
               f"rows={sorted(rows)} want={want} refused={refused}"

    leg("好图过（多文档+跨文档+T 条目）", l_good)
    leg("next 集正确（ready/waiting/parked 三面）", l_next)
    leg("环红（SCC 点名成员）", l_cycle)
    leg("悬空红（目标行缺席）", l_dangling)
    leg("自依赖红", l_selfdep)
    leg("孤儿双向（行无节/节无行）", l_orphan)
    leg("未决 token（TODO- 红；纯散文不红）", l_unresolved)
    leg("park 语义（依赖 parked=等待非 ready；check 不红）", l_park)
    leg("文档级三态（全 done 满足/非载体红）", l_doclevel)
    leg("superseded 目标=死边红（规则 4 执法）", l_superseded)
    leg("撞名硬错（§ 重复/T 重复 → rc 2）", l_dup)
    leg("表畸形硬错（漂移头/列数/状态闭集/全角竖线）", l_malformed)
    leg("区间展开+去重+搭前车（边集精确对拍）", l_range_dedup)
    leg("缓存回路（写读一致/陈拒/坏拒/版本拒）", l_cache)
    leg("子进程端到端（0/1/2/--help 退出码）", l_subprocess)
    leg("围栏遮蔽（假表不计；3.1 非编号节）", l_fence)
    leg("oracle 接缝（ready_rows 对形/零表拒判）", l_seam)

    fails = [n for n, ok, _ in results if not ok]
    print(f"task-graph 自检：{'PASS' if not fails else f'FAIL（{len(fails)} 腿红：{fails}）'}（{len(results)} 腿）")
    for d in tmpdirs:  # 自检合成树即弃——不留 /tmp 垃圾（/tmp 整洗扬别家 target dir 的教训）
        shutil.rmtree(d, ignore_errors=True)
    return EXIT_OK if not fails else EXIT_RED


def main() -> int:
    return run(sys.argv[1:])


if __name__ == "__main__":
    sys.exit(main())
