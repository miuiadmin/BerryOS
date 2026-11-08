#!/usr/bin/env python3
"""docs_guard.py —— 入口规则架构执法器（2026-11-01 入口整改起）。

四层结构执法：CLAUDE.md（@import 指针）→ AGENTS.md（≤200 行单源）→ docs/规则/（域规则）
→ docs/雷册-判例库.md（判例全文）。用法：

    python3 tools/docs_guard.py            # 全门禁（exit 0 = 过，非 0 = 拒 + 逐条 stderr）
    python3 tools/docs_guard.py --quiet    # 只打 FAIL/WARN 行

门禁：
  G1  AGENTS.md 行数帽 ≤200。
  G2  AGENTS.md 行长：>100 字符 WARN、>120 字符 FAIL。
  G3  探针串在位（外部工具依赖的口径串，防误删）：
      AGENTS.md   6 串（清单单源 = 下方 PROBES 表，勿在此逐列——两处必漂）
      规则/01     外层包安全 API / FPSIMD
      规则/03     copy_out_u32
      规则/08     SLICE_HZ/2
      规则/09     ITARGETSR / EOIR
      规则/10     att_syntax / relocation-model=static
      规则/13     排除 8 问 / Apache-2.0
  G4  锚点可解析：docs/规则/*.md 行尾 `→ 雷册 §A…` 的每个编号（支持 §A62 / §A62-65 /
      §A38/39/47 / §A10+§A11+§A100 四形态），雷册须有对应 `### §A…` 节标题（B 同理）。
  G5  CLAUDE.md 含裸行 `@AGENTS.md`（导入语法必须裸行，反引号/围栏包裹即静默不导入）。
  G6  AGENTS.md 总字符预算：>3000 警、>3400 拒（注入预算；行长帽管不住
      「行没超但每行都塞满」的密度回归——真正该守的是字符/token，不是行数）。
  G7  CLAUDE.md 字符预算 ≤600（指针文件，防有人把正文抄回指针）。
  G8  迁出目标文件在位：docs/命令速览.md / docs/规则/README.md（AGENTS 外链
      不能指向空气；G3 只查探针串，不查文件本体）。
  G10 索引三不变量（docs/索引.md 单次读入一遍扫，2026-11-08 审计漂移机器根治）：
      ① 各区标题「…（N）」标注 == 区内 `| [..](..)` 表格行实数（有行无标注同报）；
      ② 头部「主索引：N 份文档」== Σ各区标注；③ 表行登记 .md 集 ==
      docs/ 根实存 .md 集（除索引自身；规则/ 枢纽行链指 规则/README.md 不计缺登）。
      未登 / 死链 / 标实不符 / 重复登记各报清单。G9 留空（并行刀占号风险）。
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
AGENTS = ROOT / "AGENTS.md"
CLAUDE = ROOT / "CLAUDE.md"
RULES = ROOT / "docs" / "规则"
LEI = ROOT / "docs" / "雷册-判例库.md"
MOVED_DOCS = [ROOT / "docs" / "命令速览.md", RULES / "README.md"]
INDEX = ROOT / "docs" / "索引.md"
DOCS_DIR = ROOT / "docs"
# 规则/ 目录枢纽行：链接指子目录 README，非 docs 根文档，③不计缺登。
INDEX_HUB_EXEMPT = "规则/README.md"

AGENTS_MAX_LINES = 200
AGENTS_MAX_CHARS = 3400
AGENTS_WARN_CHARS = 3000
CLAUDE_MAX_CHARS = 600
LINE_WARN = 100
LINE_FAIL = 120

# `--quiet` 只打 FAIL/WARN（docstring 承诺面；CI/本地手跑同口径）。
QUIET = "--quiet" in sys.argv[1:]


def ok(msg: str) -> None:
    if not QUIET:
        print(msg)


PROBES = {
    AGENTS: [
        "外层包安全 API",
        "SLICE_HZ/2",
        "docs/规则/",
        "docs/雷册-判例库.md",
        "docs/命令速览.md",
        "docs/规则/README.md",
    ],
    RULES / "01-内核编码.md": ["外层包安全 API", "FPSIMD"],
    RULES / "03-系统调用.md": ["copy_out_u32"],
    RULES / "08-QEMU与仿真.md": ["SLICE_HZ/2"],
    RULES / "09-中断与设备.md": ["ITARGETSR", "EOIR"],
    RULES / "10-x86移植.md": ["att_syntax", "relocation-model=static"],
    RULES / "13-工作流程与协议.md": ["排除 8 问", "Apache-2.0"],
}

ANCHOR_RE = re.compile(r"→\s*雷册\s*(§?[AB]\d[\d/+§AB-]*)\s*$")

# G10 索引解析：只认「一、二、…」编号区（维护待办「（2026-…）」不误判为计数）。
INDEX_HEAD_RE = re.compile(r"主索引：(\d+)\s*份文档")
INDEX_SEC_RE = re.compile(r"^##\s+([一二三四五六七八九十百]+、.*)$")
INDEX_CNT_RE = re.compile(r"（(\d+)(?:[，,][^）]*)?）\s*$")
INDEX_ROW_RE = re.compile(r"^\|\s*\[([^\]]*)\]\(([^)]+)\)")

fails: list[str] = []
warns: list[str] = []


def fail(msg: str) -> None:
    fails.append(msg)


def warn(msg: str) -> None:
    warns.append(msg)


def gate_g1_agents_lines() -> None:
    n = len(AGENTS.read_text(encoding="utf-8").splitlines())
    if n > AGENTS_MAX_LINES:
        fail(f"G1 AGENTS.md {n} 行 > 帽 {AGENTS_MAX_LINES}")
    else:
        ok(f"G1 ok  AGENTS.md {n}/{AGENTS_MAX_LINES} 行")


def gate_g2_line_len() -> None:
    for i, line in enumerate(AGENTS.read_text(encoding="utf-8").splitlines(), 1):
        w = len(line)
        if w > LINE_FAIL:
            fail(f"G2 AGENTS.md:{i} 行长 {w} > {LINE_FAIL} 拒")
        elif w > LINE_WARN:
            warn(f"G2 AGENTS.md:{i} 行长 {w} > {LINE_WARN} 警")


def gate_g3_probes() -> None:
    before = len(fails)
    for path, needles in PROBES.items():
        if not path.exists():
            fail(f"G3 缺文件 {path.relative_to(ROOT)}")
            continue
        text = path.read_text(encoding="utf-8")
        for needle in needles:
            if needle not in text:
                fail(f"G3 {path.relative_to(ROOT)} 探针串缺失: {needle}")
    if len(fails) == before:
        ok(f"G3 ok  探针 {sum(len(v) for v in PROBES.values())} 串在位")


def lei_ids() -> set[str]:
    return set(
        m.group(1)
        for m in re.finditer(r"^###\s+§([AB]\d+)", LEI.read_text(encoding="utf-8"), re.M)
    )


def expand_anchor_ids(anchor: str) -> list[str]:
    """§A62 / §A62-65 / §A38/39/47 / §A10+§A11+§A100 → 逐号列表。"""
    out: list[str] = []
    for piece in anchor.split("+"):
        piece = piece.strip().lstrip("§")
        m = re.match(r"^([AB])(\d+)(.*)$", piece)
        if not m:
            continue
        pfx, first, rest = m.group(1), int(m.group(2)), m.group(3)
        out.append(f"{pfx}{first}")
        rm = re.match(r"^-\s*(?:[AB])?(\d+)$", rest)
        if rm:  # 区间 A62-65
            out.extend(f"{pfx}{n}" for n in range(first + 1, int(rm.group(1)) + 1))
            continue
        for tok in rest.split("/"):  # 斜杠列表 A38/39/47
            tm = re.match(r"^\s*([AB])?(\d+)\s*$", tok)
            if tm:
                out.append(f"{tm.group(1) or pfx}{int(tm.group(2))}")
    return out


def gate_g4_anchors() -> None:
    before = len(fails)
    ids = lei_ids()
    checked = 0
    for md in sorted(RULES.glob("*.md")):
        if md.name in ("README.md", "判例索引.md"):
            continue
        for i, line in enumerate(md.read_text(encoding="utf-8").splitlines(), 1):
            m = ANCHOR_RE.search(line)
            if not m:
                continue
            checked += 1
            for aid in expand_anchor_ids(m.group(1)):
                if aid not in ids:
                    fail(f"G4 {md.name}:{i} 锚点 §{aid} 在雷册无对应节")
    if checked == 0:
        fail("G4 域文件零锚点行——瘦行节疑似整体丢失")
    elif len(fails) == before:
        ok(f"G4 ok  {checked} 锚点行全部可解析")


def gate_g6_agents_chars() -> None:
    n = len(AGENTS.read_text(encoding="utf-8"))
    if n > AGENTS_MAX_CHARS:
        fail(f"G6 AGENTS.md {n} 字符 > 帽 {AGENTS_MAX_CHARS}")
    elif n > AGENTS_WARN_CHARS:
        warn(f"G6 AGENTS.md {n} 字符 > 警线 {AGENTS_WARN_CHARS}")
    else:
        ok(f"G6 ok  AGENTS.md {n}/{AGENTS_MAX_CHARS} 字符（警线 {AGENTS_WARN_CHARS}）")


def gate_g7_claude_chars() -> None:
    if not CLAUDE.exists():
        return  # G5 已报缺文件，避免二次 traceback
    n = len(CLAUDE.read_text(encoding="utf-8"))
    if n > CLAUDE_MAX_CHARS:
        fail(f"G7 CLAUDE.md {n} 字符 > 帽 {CLAUDE_MAX_CHARS}（指针文件勿抄正文）")
    else:
        ok(f"G7 ok  CLAUDE.md {n}/{CLAUDE_MAX_CHARS} 字符")


def gate_g8_moved_docs() -> None:
    missing = [str(p.relative_to(ROOT)) for p in MOVED_DOCS if not p.exists()]
    if missing:
        fail(f"G8 迁出目标文件缺失: {', '.join(missing)}")
    else:
        ok(f"G8 ok  迁出目标 {len(MOVED_DOCS)} 件在位")


def gate_g10_index_invariants() -> None:
    """G10 索引三不变量：区标注==行实数、头部==Σ区标注、登记集==docs 根实存集。

    单次读文件一遍扫：逐行分派到「当前编号区」计行数 + 收登记目标，③豁免枢纽行。
    """
    before = len(fails)
    if not INDEX.exists():
        fail("G10 docs/索引.md 缺失——主索引是文档详表单一事实源")
        return
    lines = INDEX.read_text(encoding="utf-8").splitlines()
    sections: list[list] = []  # [区标题, 标注数 or None, 表格行实数]
    targets: list[str] = []  # 表行登记目标（枢纽行③再豁免）
    cur: list | None = None
    for line in lines:
        sm = INDEX_SEC_RE.match(line)
        if sm:
            if cur is not None:
                sections.append(cur)
            cm = INDEX_CNT_RE.search(sm.group(1))
            cur = [sm.group(1), int(cm.group(1)) if cm else None, 0]
            continue
        rm = INDEX_ROW_RE.match(line)
        if rm is not None and cur is not None:
            cur[2] += 1
            targets.append(rm.group(2))
    if cur is not None:
        sections.append(cur)
    # ① 各区标注 == 行实数（有行无标注 = 缺计数漂移，同报红）
    for title, ann, rows in sections:
        if ann is None:
            if rows:
                fail(f"G10 索引区「{title}」有 {rows} 行表格但标题缺（N）计数标注")
        elif ann != rows:
            fail(f"G10 索引区「{title}」标注 {ann} ≠ 实际表格行 {rows}")
    # ② 头部 N == Σ各区标注
    total = sum(ann for _, ann, _ in sections if ann is not None)
    hm = INDEX_HEAD_RE.search("\n".join(lines[:12]))
    if hm is None:
        fail("G10 头部「主索引：N 份文档」计数行缺失（前 12 行内未觅得）")
    elif int(hm.group(1)) != total:
        fail(f"G10 头部主索引 {hm.group(1)} 份 ≠ Σ区标注 {total}")
    # ③ 登记集 == docs 根实存 .md 集（除索引自身；规则/ 枢纽行不计缺登）
    reg = [t for t in targets if t != INDEX_HUB_EXEMPT and t.endswith(".md")]
    dup = sorted({t for t in reg if reg.count(t) > 1})
    if dup:
        fail(f"G10 索引重复登记: {', '.join(dup)}")
    actual = {p.name for p in DOCS_DIR.glob("*.md")} - {"索引.md"}
    dead = sorted(t for t in set(reg) if not (DOCS_DIR / t).exists())
    if dead:
        fail(f"G10 索引死链（登记但 docs/ 下无此文件）: {', '.join(dead)}")
    unreg = sorted(actual - set(reg))
    if unreg:
        fail(f"G10 未登文档（docs/ 根实存但索引未登记）: {', '.join(unreg)}")
    if len(fails) == before:
        ok(f"G10 ok  索引 {len(sections)} 区 {total} 行 = 头部 {hm.group(1)}，"
           f"docs 根 {len(actual)} 份 .md 全登（枢纽行豁免）")


def gate_g5_claude_import() -> None:
    if not CLAUDE.exists():
        fail("G5 CLAUDE.md 缺失——指针文件是 @import 链的绑定对")
        return
    if not any(l.strip() == "@AGENTS.md" for l in CLAUDE.read_text(encoding="utf-8").splitlines()):
        fail("G5 CLAUDE.md 无裸行 `@AGENTS.md`（导入语法必须裸行，反引号/围栏包裹即静默不导入）")
    else:
        ok("G5 ok  CLAUDE.md @import 裸行在位")


def main() -> int:
    if not AGENTS.exists():
        print("AGENTS.md 缺失", file=sys.stderr)
        return 2
    gate_g5_claude_import()
    gate_g1_agents_lines()
    gate_g2_line_len()
    gate_g3_probes()
    gate_g4_anchors()
    gate_g6_agents_chars()
    gate_g7_claude_chars()
    gate_g8_moved_docs()
    gate_g10_index_invariants()
    for w in warns:
        print(f"WARN: {w}", file=sys.stderr)
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    if not QUIET:
        print(f"docs_guard: PASS（{len(warns)} 警）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
