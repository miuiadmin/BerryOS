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
      AGENTS.md   外层包安全 API / SLICE_HZ/2 / docs/规则/ / docs/雷册-判例库.md
      规则/01     外层包安全 API / FPSIMD
      规则/03     copy_out_u32
      规则/08     SLICE_HZ/2
      规则/09     ITARGETSR / EOIR
      规则/10     att_syntax / relocation-model=static
      规则/13     排除 8 问 / Apache-2.0
  G4  锚点可解析：docs/规则/*.md 行尾 `→ 雷册 §A…` 的每个编号（支持 §A62 / §A62-65 /
      §A38/39/47 / §A10+§A11+§A100 四形态），雷册须有对应 `### §A…` 节标题（B 同理）。
  G5  CLAUDE.md 含裸行 `@AGENTS.md`（导入语法必须裸行，反引号/围栏包裹即静默不导入）。
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
AGENTS = ROOT / "AGENTS.md"
CLAUDE = ROOT / "CLAUDE.md"
RULES = ROOT / "docs" / "规则"
LEI = ROOT / "docs" / "雷册-判例库.md"

AGENTS_MAX_LINES = 200
LINE_WARN = 100
LINE_FAIL = 120

PROBES = {
    AGENTS: ["外层包安全 API", "SLICE_HZ/2", "docs/规则/", "docs/雷册-判例库.md"],
    RULES / "01-内核编码.md": ["外层包安全 API", "FPSIMD"],
    RULES / "03-系统调用.md": ["copy_out_u32"],
    RULES / "08-QEMU与仿真.md": ["SLICE_HZ/2"],
    RULES / "09-中断与设备.md": ["ITARGETSR", "EOIR"],
    RULES / "10-x86移植.md": ["att_syntax", "relocation-model=static"],
    RULES / "13-工作流程与协议.md": ["排除 8 问", "Apache-2.0"],
}

ANCHOR_RE = re.compile(r"→\s*雷册\s*(§?[AB]\d[\d/+§AB-]*)\s*$")

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
        print(f"G1 ok  AGENTS.md {n}/{AGENTS_MAX_LINES} 行")


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
        print(f"G3 ok  探针 {sum(len(v) for v in PROBES.values())} 串在位")


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
        print(f"G4 ok  {checked} 锚点行全部可解析")


def gate_g5_claude_import() -> None:
    if not CLAUDE.exists():
        fail("G5 CLAUDE.md 缺失——指针文件是 @import 链的绑定对")
        return
    if not any(l.strip() == "@AGENTS.md" for l in CLAUDE.read_text(encoding="utf-8").splitlines()):
        fail("G5 CLAUDE.md 无裸行 `@AGENTS.md`（导入语法必须裸行，反引号/围栏包裹即静默不导入）")
    else:
        print("G5 ok  CLAUDE.md @import 裸行在位")


def main() -> int:
    if not AGENTS.exists():
        print("AGENTS.md 缺失", file=sys.stderr)
        return 2
    gate_g5_claude_import()
    gate_g1_agents_lines()
    gate_g2_line_len()
    gate_g3_probes()
    gate_g4_anchors()
    for w in warns:
        print(f"WARN: {w}", file=sys.stderr)
    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print(f"docs_guard: PASS（{len(warns)} 警）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
