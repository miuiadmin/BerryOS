#!/usr/bin/env python3
"""G-eco-d libc shim 符号面审计（草案附录 A 的工具面，AI 测试友好化：
确定性、可复跑、机器可读）。

两种模式:

  rlib-surface <target-libdir>
      上界面: target libdir 下全部 rlib + self-contained/libunwind.a 的
      「未定义 − 已定义」全局符号 = 任何 std 程序可能要 libc 提供的全集
      （shim 按「全定义」目标即永不因符号缺失拒链）。

  elf-surface <elf>
      程序面: 最终 ELF 的未定义非弱符号（链接成功的产物上应为空;
      剥 -lc 链接的探针产物上 = 该程序真实需要的 libc 面）。

依赖 llvm-nm（macOS /usr/bin/nm 即是）。"""
import subprocess
import sys
from pathlib import Path

GLOBALS_DEF = set("ABCDGRSTWI")


def run_nm(path: str) -> str:
    return subprocess.run(["nm", "--", path], capture_output=True, text=True).stdout


def rlib_surface(libdir: Path) -> None:
    defined: set[str] = set()
    undef: dict[str, str] = {}
    targets = sorted(libdir.glob("*.rlib")) + [libdir / "self-contained" / "libunwind.a"]
    for arc in targets:
        if not arc.exists():
            continue
        for line in run_nm(str(arc)).splitlines():
            if line.endswith(":"):  # 归档成员头
                continue
            parts = line.split()
            if len(parts) == 3:
                t, sym = parts[1], parts[2]
            elif len(parts) == 2:
                t, sym = parts[0], parts[1]
            else:
                continue
            if t == "U":
                undef[sym] = "U"
            elif t in ("w", "v"):
                undef.setdefault(sym, "w")
            elif t in GLOBALS_DEF:
                defined.add(sym)
    out = {s: k for s, k in undef.items() if s not in defined}
    print(f"# archives={sum(1 for t in targets if t.exists())} defined={len(defined)}")
    for kind, label in (("U", "STRONG(shim 必须定义)"), ("w", "WEAK(缺省 0 合法)")):
        syms = sorted(s for s, k in out.items() if k == kind)
        print(f"# {label}: {len(syms)}")
        for s in syms:
            print(s)


def elf_surface(elf: Path) -> None:
    weak: list[str] = []
    strong: list[str] = []
    for line in run_nm(str(elf)).splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[0] in ("U", "w", "v"):
            (strong if parts[0] == "U" else weak).append(parts[1])
    print(f"# STRONG undefined: {len(strong)}")
    for s in sorted(strong):
        print(s)
    print(f"# WEAK undefined: {len(weak)}")
    for s in sorted(weak):
        print(s)


def main() -> None:
    if len(sys.argv) != 3 or sys.argv[1] not in ("rlib-surface", "elf-surface"):
        sys.exit(__doc__)
    if sys.argv[1] == "rlib-surface":
        rlib_surface(Path(sys.argv[2]))
    else:
        elf_surface(Path(sys.argv[2]))


if __name__ == "__main__":
    main()
