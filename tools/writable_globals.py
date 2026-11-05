#!/usr/bin/env python3
"""可写全局清单（借 Ginux `writable-globals.tsv` 门禁，2026-11）。

扫描 kernel/ mm/ sched/ ipc/ arch/ 的 `static mut`（以及 static + UnsafeCell
形态），维护 `tools/writable_globals.txt` 的精确清单：

  path::symbol<TAB>group

键不含行号（行长漂移不破门）；新增/删除可写全局必须同刀改清单，否则
`--check` 红。`--update` 只补缺失/删陈旧，保留人工 group；新条目默认给
auto group，需要 owner 语义时手工改 group。

用法:
  python3 tools/writable_globals.py --list     # 当前清单（含 auto group）
  python3 tools/writable_globals.py --check    # 与清单精确对拍（CI/just check）
  python3 tools/writable_globals.py --update   # 生成/同步清单（保留已有 group）

口径:
  - 扫描面 = kernel/ mm/ sched/ ipc/ arch/ 的 `.rs`；
  - 排除 kernel/src/ktest.rs / kernel/src/bench.rs（测试/基准面另账）；
  - 只登记声明点，不把 `&'static mut` 局部/函数参数误算为全局；
  - 注释/字符串先经 unsafe_audit.mask_rust 掩码，asm 字符串不误报。
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from unsafe_audit import mask_rust  # noqa: E402（复用注释/字符串掩码）

REPO = Path(__file__).resolve().parent.parent
MANIFEST = REPO / "tools" / "writable_globals.txt"
SCAN_DIRS = ("kernel", "mm", "sched", "ipc", "arch")
TEST_FILES = {"kernel/src/ktest.rs", "kernel/src/bench.rs"}
GROUPS = ("boot", "arch", "page-table", "scheduler", "ipc", "telemetry", "other")

# 声明点：static mut NAME；static NAME: ...UnsafeCell...
RE_STATIC_MUT = re.compile(
    r"^[ \t]*(?:pub(?:\([^)]*\))?\s+)?static\s+mut\s+([A-Za-z_][A-Za-z0-9_]*)",
    re.MULTILINE,
)
RE_STATIC_CELL = re.compile(
    r"^[ \t]*(?:pub(?:\([^)]*\))?\s+)?static\s+([A-Za-z_][A-Za-z0-9_]*)"
    r"\s*:\s*[^;=]*UnsafeCell",
    re.MULTILINE,
)


def auto_group(rel: str) -> str:
    """粗分 owner 组（首版自动分类；人工可改 group）。"""
    if rel.endswith(("mmu.rs", "mm.rs", "aspace.rs")):
        return "page-table"
    if rel.startswith("sched/") or rel.endswith(("smp.rs", "kern_lock.rs", "timer.rs")):
        return "scheduler"
    if rel.endswith("telemetry.rs"):
        return "telemetry"
    if rel.startswith("ipc/") or "/ipc/" in rel:
        return "ipc"
    if rel.endswith("main.rs") or rel.endswith("syscall.rs"):
        return "boot"
    if rel.startswith("arch/"):
        return "arch"
    return "other"


def scan() -> dict[str, str]:
    """返回 {path::symbol: auto_group}，按 key 排序。"""
    found: dict[str, str] = {}
    for root in SCAN_DIRS:
        for p in sorted((REPO / root).rglob("*.rs")):
            rel = p.relative_to(REPO).as_posix()
            if rel in TEST_FILES:
                continue
            text = mask_rust(p.read_text(encoding="utf-8"))
            names = set(m.group(1) for m in RE_STATIC_MUT.finditer(text))
            names.update(m.group(1) for m in RE_STATIC_CELL.finditer(text))
            for name in names:
                found[f"{rel}::{name}"] = auto_group(rel)
    return dict(sorted(found.items()))


def parse_manifest() -> dict[str, str]:
    if not MANIFEST.exists():
        return {}
    out: dict[str, str] = {}
    for lineno, line in enumerate(MANIFEST.read_text(encoding="utf-8").splitlines(), 1):
        raw = line.strip()
        if not raw or raw.startswith("#"):
            continue
        key, sep, group = raw.partition("\t")
        key, group = key.strip(), group.strip()
        if not sep or not key or "::" not in key:
            raise SystemExit(f"{MANIFEST}:{lineno}: 期望 path::symbol<TAB>group，got {line!r}")
        if key in out:
            raise SystemExit(f"{MANIFEST}:{lineno}: 重复键 {key}")
        if group not in GROUPS:
            raise SystemExit(f"{MANIFEST}:{lineno}: group {group!r} 不在 {GROUPS}")
        out[key] = group
    return out


def write_manifest(current: dict[str, str], old: dict[str, str]) -> None:
    lines = [
        "# 可写全局清单（Ginux writable-globals 借法；路径::符号<TAB>owner-group）",
        "# 新 static mut 必须入册；--update 只补缺失/删陈旧，保留人工 group。",
        "# 排除测试/基准：kernel/src/ktest.rs、kernel/src/bench.rs。",
        f"# groups: {', '.join(GROUPS)}",
        "",
    ]
    for key in sorted(current):
        lines.append(f"{key}\t{old.get(key, current[key])}")
    MANIFEST.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    args = set(sys.argv[1:])
    if len(args) != 1 or not args & {"--list", "--check", "--update"}:
        print(__doc__, file=sys.stderr)
        return 2
    current = scan()
    if "--list" in args:
        for key, group in current.items():
            print(f"{key}\t{group}")
        print(f"# {len(current)} writable globals", file=sys.stderr)
        return 0
    old = parse_manifest()
    if "--update" in args:
        write_manifest(current, old)
        print(f"writable_globals: updated {MANIFEST.relative_to(REPO)} ({len(current)} entries)")
        return 0
    missing = sorted(set(current) - set(old))
    stale = sorted(set(old) - set(current))
    if missing or stale:
        for k in missing:
            print(f"MISSING {k}\t{current[k]}", file=sys.stderr)
        for k in stale:
            print(f"STALE   {k}\t{old[k]}", file=sys.stderr)
        print(
            "writable_globals: FAIL（新增/删除可写全局未入册；"
            "确认 owner 后跑 python3 tools/writable_globals.py --update）",
            file=sys.stderr,
        )
        return 1
    print(f"writable_globals: ok ({len(current)} entries)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
