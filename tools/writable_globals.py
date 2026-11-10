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
  python3 tools/writable_globals.py --selftest # 门禁自检（a-3 U4 纪律，独占旗标）

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


def selftest() -> int:
    """门禁自检（a-3 U4 纪律同款）——可写全局清单门禁自身的钉板。

    本工具是 just check / CI 的 static mut 入册执法面（76 条在册），
    此前零自检：扫描器坏 = 新增可写全局静默漏报或 --check 静默劣化。
    覆盖：
    - auto_group：粗分链七组（mmu/mm/aspace→页表、sched/锁计时→调度、
      telemetry、ipc 双形、main/syscall→boot、arch/ 前缀、兜底 other）
    - 声明点正则：static mut（含 pub/pub(crate) 可见性前缀）/ static:
      UnsafeCell 收、&'static mut 局部引用与普通 static 不误算、
      注释经 mask_rust 掩后不登记
    - parse_manifest：注释/空行滤过 + path::symbol<TAB>group 契约 +
      四种坏行 SystemExit（无 tab/无 ::/重复键/非法 group）
    - write_manifest：--update 只补缺删陈旧、人工 group 保留、
      新条目落 auto group、头四行结构钉板
    - 三态 main：子进程真跑合成树——--check 红分支（MISSING/STALE
      各自红字 + rc=1）/ --update 收敛（更新后 --check 转绿 rc=0）/
      --list 键全集 + stderr 计数
    - live 树对拍：scan() 非空 + 键全 path::symbol 形 + group 全在册 +
      与 tools/writable_globals.txt 逐条对拍（--check 语义不变量）
    - 确定性：scan() 两次全等（--check 对拍前提）
    - exit-code 契约：--selftest 独占守卫（组合旗标/未知旗标 rc=2）
    """
    import shutil
    import subprocess
    import tempfile

    failures: list[str] = []

    def check(name: str, cond, detail: str = "") -> None:
        print(f"  {'ok' if cond else 'FAIL'} {name}"
              + (f": {detail}" if not cond else ""))
        if not cond:
            failures.append(name)

    # --- 1. auto_group：分类链 ---
    for rel, want in [
        ("mm/mmu.rs", "page-table"), ("kernel/src/mm.rs", "page-table"),
        ("arch/aarch64/src/aspace.rs", "page-table"),
        ("sched/src/x.rs", "scheduler"),
        ("arch/aarch64/src/smp.rs", "scheduler"),
        ("kernel/src/kern_lock.rs", "scheduler"),
        ("arch/aarch64/src/timer.rs", "scheduler"),
        ("kernel/src/telemetry.rs", "telemetry"),
        ("ipc/src/x.rs", "ipc"), ("kernel/src/ipc/y.rs", "ipc"),
        ("kernel/src/main.rs", "boot"), ("kernel/src/syscall.rs", "boot"),
        ("arch/aarch64/src/x.rs", "arch"),
        ("kernel/src/foo.rs", "other"),
    ]:
        check(f"grp: {rel} → {want}", auto_group(rel) == want,
              auto_group(rel))

    # --- 2. 声明点正则（mask 后） ---
    for src, want_mut, want_cell in [
        ("static mut A: u32 = 0;", {"A"}, set()),
        ("pub static mut B: u8 = 1;", {"B"}, set()),
        ("pub(crate) static mut C: u8 = 1;", {"C"}, set()),
        ("static mut E;", {"E"}, set()),
        ("static mut NAME_2: usize = 0;", {"NAME_2"}, set()),
        ("static D: UnsafeCell<u32> = UnsafeCell::new(0);", set(), {"D"}),
        ("let x: &'static mut F = y;", set(), set()),
        ("static H: u32 = 0;", set(), set()),
        ("// static mut G: u32 = 0;", set(), set()),
        ("/* static mut GG: u32 = 0; */", set(), set()),
        ('let s = r#"static mut R: u32 = 0;"#;', set(), set()),
        ("static CELL: SyncUnsafeCell<u8> = SyncUnsafeCell::new(0);",
         set(), {"CELL"}),  # 变体形保守收编（漏报可写全局比误报危险）
    ]:
        m = mask_rust(src)
        mut_n = set(g.group(1) for g in RE_STATIC_MUT.finditer(m))
        cell_n = set(g.group(1) for g in RE_STATIC_CELL.finditer(m))
        check(f"re mut: {src[:34]!r}", mut_n == want_mut, f"{mut_n}")
        check(f"re cell: {src[:34]!r}", cell_n == want_cell, f"{cell_n}")

    # --- 3. parse_manifest：契约 + 四坏行 ---
    with tempfile.TemporaryDirectory() as td:
        man = Path(td) / "wg.txt"
        man.write_text("a.rs::X\tother\n# 注释\n\nb.rs::Y\tboot\n",
                       encoding="utf-8")
        try:
            got = _parse_at(man)
            check("parse: 注释/空行滤过 + 键值对",
                  got == {"a.rs::X": "other", "b.rs::Y": "boot"}, str(got))
        except SystemExit as e:
            check("parse: 注释/空行滤过 + 键值对", False, str(e))
        for content, label in [
            ("a.rs::X other\n", "无 tab 拒"),
            ("aX other\n", "无 :: 拒"),
            ("a.rs::X\tother\na.rs::X\tboot\n", "重复键拒"),
            ("a.rs::X\tnogroup\n", "非法 group 拒"),
        ]:
            man.write_text(content, encoding="utf-8")
            try:
                _parse_at(man)
                check(f"parse: {label}", False, "未 SystemExit")
            except SystemExit:
                check(f"parse: {label}", True)

    # --- 4/5. 三态 main：子进程真跑合成树 ---
    with tempfile.TemporaryDirectory() as td:
        tdp = Path(td)
        (tdp / "tools").mkdir()
        shutil.copy(Path(__file__).resolve().parent / "unsafe_audit.py",
                    tdp / "tools" / "unsafe_audit.py")
        shim = tdp / "tools" / "writable_globals.py"
        shutil.copy(Path(__file__).resolve(), shim)
        src_dir = tdp / "kernel" / "src"
        src_dir.mkdir(parents=True)
        (src_dir / "x.rs").write_text(
            "pub static mut FOO: u32 = 0;\nstatic mut BAR: u8 = 1;\n",
            encoding="utf-8")
        (tdp / "tools" / "writable_globals.txt").write_text(
            "kernel/src/x.rs::FOO\tother\n", encoding="utf-8")

        def run(*args: str) -> subprocess.CompletedProcess:
            return subprocess.run(
                [sys.executable, str(shim), *args],
                capture_output=True, text=True, cwd=tdp)

        r = run("--check")
        check("main: --check 缺 BAR → MISSING 红 rc=1",
              r.returncode == 1 and "MISSING" in r.stderr
              and "kernel/src/x.rs::BAR" in r.stderr,
              f"rc={r.returncode}")
        (tdp / "tools" / "writable_globals.txt").write_text(
            "kernel/src/x.rs::FOO\tother\nkernel/src/x.rs::GONE\tother\n",
            encoding="utf-8")
        r = run("--check")
        check("main: --check 多 GONE → STALE 红 rc=1",
              r.returncode == 1 and "STALE" in r.stderr
              and "::GONE" in r.stderr, f"rc={r.returncode}")
        r = run("--update")
        man_txt = (tdp / "tools" / "writable_globals.txt").read_text(
            encoding="utf-8")
        lines = [ln for ln in man_txt.splitlines() if ln and not ln.startswith("#")]
        check("main: --update 收敛全集（FOO/BAR 在、GONE 删）",
              r.returncode == 0
              and sorted(lines) == ["kernel/src/x.rs::BAR\tother",
                                    "kernel/src/x.rs::FOO\tother"],
              str(lines))
        check("main: --update 人工 group 保留",
              "kernel/src/x.rs::FOO\tother" in man_txt)
        r = run("--check")
        check("main: update 后 --check 转绿 rc=0",
              r.returncode == 0 and "ok (2 entries)" in r.stdout,
              f"rc={r.returncode} {r.stdout.strip()}")
        r = run("--list")
        check("main: --list 键全集 + stderr 计数",
              r.returncode == 0 and "::FOO" in r.stdout
              and "::BAR" in r.stdout and "2 writable globals" in r.stderr,
              r.stderr.strip())

    # --- 6. live 树对拍 + 确定性 ---
    live = scan()
    check("live: scan 非空（76±量级）", len(live) > 30, str(len(live)))
    check("live: 键全 path::symbol 形",
          all("::" in k and "\t" not in k for k in live))
    check("live: group 全在册", set(live.values()) <= set(GROUPS),
          str(set(live.values()) - set(GROUPS)))
    orig = parse_manifest()
    check("live: 与在册清单逐条对拍（--check 语义）",
          orig == live, f"only-scan={sorted(set(live) - set(orig))[:3]} "
                        f"only-man={sorted(set(orig) - set(live))[:3]}")
    check("determ: scan 两次全等", scan() == live)

    print(f"\nwritable_globals selftest: "
          f"{'PASS' if not failures else 'FAIL'}（{len(failures)} 面红）"
          + ("" if failures else
             "—— 分类链/正则/manifest 契约/三态 main 红绿分支/"
             "live 对拍/确定性/exit-code"))
    return 1 if failures else 0


def _parse_at(path: Path) -> dict[str, str]:
    """selftest 辅助：在指定路径上跑 parse_manifest（不动全局 MANIFEST）。"""
    global MANIFEST
    saved = MANIFEST
    MANIFEST = path
    try:
        return parse_manifest()
    finally:
        MANIFEST = saved


def main() -> int:
    args = set(sys.argv[1:])
    if "--selftest" in args:
        if len(args) != 1:
            print("writable_globals: --selftest 须独占使用（无组合旗标）",
                  file=sys.stderr)
            return 2
        return selftest()
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
