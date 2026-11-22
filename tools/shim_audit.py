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

  --selftest
      门禁自检（合成 nm 行向量钉解析/分类/覆盖语义，不依赖外部 nm——
      host 确定性；mkfat --selftest 同位纪律）。

依赖 llvm-nm（macOS /usr/bin/nm 即是）。"""
import subprocess
import sys
from pathlib import Path

GLOBALS_DEF = set("ABCDGRSTWI")


def run_nm(path: str) -> str:
    return subprocess.run(["nm", "--", path], capture_output=True, text=True).stdout


# ---- 纯解析面（--selftest 钉板位；rlib_surface/elf_surface 的单源）----


def acc_archive_nm(lines, defined: set, undef: dict) -> None:
    """nm 归档输出逐行累加进 defined/undef。

    行形三态：归档成员头（尾冒号，跳过）/ 三段 `addr TYPE sym` /
    两段 `TYPE sym`（未定义符号无地址）。分类语义：
    U=强未定义（后行覆盖）；w/v=弱未定义（setdefault——强已在则不降级，
    后行 U 仍可升强）；GLOBALS_DEF=全局定义（覆盖未定义）；其余
    （小写本地符号 t/b/d 等、N 调试段）与 shim 面无关，忽略。
    """
    for line in lines:
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


def net_surface(defined: set, undef: dict) -> dict:
    """「未定义 − 已定义」净面：归档自带定义即满足，不再要求 shim。"""
    return {s: k for s, k in undef.items() if s not in defined}


def format_rlib_report(n_archives: int, defined: set, out: dict) -> str:
    lines = [f"# archives={n_archives} defined={len(defined)}"]
    for kind, label in (("U", "STRONG(shim 必须定义)"), ("w", "WEAK(缺省 0 合法)")):
        syms = sorted(s for s, k in out.items() if k == kind)
        lines.append(f"# {label}: {len(syms)}")
        lines.extend(syms)
    return "\n".join(lines)


def split_elf_undef(lines):
    """最终 ELF 的 nm 输出 → (强未定义, 弱未定义)。

    只收两段形 `TYPE sym`（无地址=未定义）；带地址的三段形是已定义
    符号，与本面无关。v 同按弱处理（与 acc_archive_nm 口径一致）。
    """
    strong: list[str] = []
    weak: list[str] = []
    for line in lines:
        parts = line.split()
        if len(parts) == 2 and parts[0] in ("U", "w", "v"):
            (strong if parts[0] == "U" else weak).append(parts[1])
    return strong, weak


def format_elf_report(strong: list, weak: list) -> str:
    lines = [f"# STRONG undefined: {len(strong)}"]
    lines.extend(sorted(strong))
    lines.append(f"# WEAK undefined: {len(weak)}")
    lines.extend(sorted(weak))
    return "\n".join(lines)


def rlib_surface(libdir: Path) -> None:
    defined: set[str] = set()
    undef: dict[str, str] = {}
    targets = sorted(libdir.glob("*.rlib")) + [libdir / "self-contained" / "libunwind.a"]
    for arc in targets:
        if not arc.exists():
            continue
        acc_archive_nm(run_nm(str(arc)).splitlines(), defined, undef)
    print(format_rlib_report(sum(1 for t in targets if t.exists()), defined,
                             net_surface(defined, undef)))


def elf_surface(elf: Path) -> None:
    strong, weak = split_elf_undef(run_nm(str(elf)).splitlines())
    print(format_elf_report(strong, weak))


# ---- selftest：合成 nm 行向量钉解析/分类/覆盖语义（不依赖外部 nm）----


def selftest() -> int:
    """门禁自检（a-3 U4 纪律同款）——shim 符号面审计的纯解析面钉板。

    本工具是 G-eco-d libc shim 路线的关键裁决器（上界=全可 shim 的
    存活证据，d-3c hexyl 终验的数据源），此前只在 G-eco 一次性人工跑：
    分类面坏（GLOBALS_DEF 漏字母=全局定义漏计虚高上界；U/w 覆盖序翻
    面=强弱错档误导 shim 策略；定义覆盖失效=已满足符号重复要求）=
    路线裁决静默劣化。覆盖：
    - GLOBALS_DEF 常量钉板（漂移即咬，红分支实证位）
    - acc_archive_nm：三段/两段/成员头/空行四行形 + U 覆盖 w（后行升强）
      + w 不覆盖 U（setdefault 保强）+ 小写 t 本地符号不计定义 + N 调试忽略
    - net_surface：定义覆盖未定义；未覆盖弱保留 w 档
    - format_rlib_report：计数头 + 双档标签 + 字典序排序
    - split_elf_undef：两段收/三段（已定义带地址）不收 + v 归弱
    - 全链合成归档向量：成员头+定义+强弱未定义 → net → 报告逐行对拍
    - exit-code 契约：错参 rc=1 + usage 上 stderr；--selftest rc=0
    - 确定性：同输入两次 byte 级一致
    """
    import subprocess as sp

    failures = []

    def check(name, cond, detail=""):
        if cond:
            print(f"  ok {name}")
        else:
            failures.append(f"{name}: {detail}")
            print(f"  FAIL {name}: {detail}")

    # --- 0. 常量钉板（shim 上界的分类口径——漏字母即虚高上界） ---
    check("GLOBALS_DEF pinned", "".join(sorted(GLOBALS_DEF)) == "ABCDGIRSTW",
          f"{''.join(sorted(GLOBALS_DEF))}")

    # --- 1. acc_archive_nm 行形四态 ---
    d, u = set(), {}
    acc_archive_nm(["hello.rlib:core-1.o:", "", "junk-no-fields"], d, u)
    check("member header / blank skipped", not d and not u, f"{d} {u}")

    acc_archive_nm(["0000000000000000 T _start"], d, u)
    check("3-field form (addr TYPE sym)", "_start" in d, f"{sorted(d)}")

    acc_archive_nm(["U memcpy"], d, u)
    check("2-field form (TYPE sym)", u.get("memcpy") == "U", f"{u}")

    # --- 2. U/w 覆盖序（强弱档不被观察序骗） ---
    d2, u2 = set(), {}
    acc_archive_nm(["w foo", "U foo"], d2, u2)  # 后行 U 升强
    check("U upgrades prior w", u2["foo"] == "U", f"{u2}")
    d2b, u2b = set(), {}
    acc_archive_nm(["U foo", "w foo", "v foo"], d2b, u2b)  # setdefault 保强
    check("w/v never downgrade U", u2b["foo"] == "U", f"{u2b}")

    # --- 3. 分类口径：本地/调试不计定义 ---
    d3, u3 = set(), {}
    acc_archive_nm(["0000000000000000 t local_fn", "0000000000000000 N debug_sym",
                    "U local_fn"], d3, u3)
    check("lowercase t not defined (local)", "local_fn" not in d3 and
          u3.get("local_fn") == "U", f"{sorted(d3)} {u3}")
    check("N debug ignored", "debug_sym" not in d3, f"{sorted(d3)}")

    # --- 4. net_surface：定义覆盖 ---
    out = net_surface({"_start", "selfdef"}, {"memcpy": "U", "__cxa_finalize": "w",
                                              "selfdef": "U"})
    check("defined covers undefined", out == {"memcpy": "U",
          "__cxa_finalize": "w"}, f"{out}")

    # --- 5. format_rlib_report 头行/双档/排序 ---
    rep = format_rlib_report(2, {"b_sym", "a_sym"}, {"zeta": "U", "alpha": "U",
                                                     "wonly": "w"})
    lines = rep.splitlines()
    check("report header counts", lines[0] == "# archives=2 defined=2", lines[0])
    check("report strong tier sorted", lines[1] == "# STRONG(shim 必须定义): 2"
          and lines[2:4] == ["alpha", "zeta"], f"{lines[:4]}")
    check("report weak tier", lines[4] == "# WEAK(缺省 0 合法): 1" and
          lines[5] == "wonly", f"{lines[4:6]}")

    # --- 6. split_elf_undef：两段收/三段不收/v 归弱 ---
    s6, w6 = split_elf_undef(["U memcpy", "                 w __cxa_finalize",
                              "v __emutls", "0000000000000000 T _start",
                              "0000000000000000 t local"])
    check("elf strong 2-field only", s6 == ["memcpy"], f"{s6}")
    check("elf weak w+v", w6 == ["__cxa_finalize", "__emutls"], f"{w6}")

    # --- 7. 全链合成归档向量（手算对拍） ---
    d7, u7 = set(), {}
    acc_archive_nm([
        "std-abc123.rlib:std-1.o:",
        "0000000000000000 T __rust_alloc",
        "                 U memcpy",
        "                 w __cxa_finalize",
        "std-abc123.rlib:std-2.o:",
        "0000000000000000 D __rust_no_panic",
        "                 U __rust_alloc",   # 跨成员自满足
        "                 U abort",
    ], d7, u7)
    out7 = net_surface(d7, u7)
    check("full-chain net exact", out7 == {"memcpy": "U", "abort": "U",
          "__cxa_finalize": "w"}, f"{out7}")
    check("full-chain report deterministic",
          format_rlib_report(1, d7, out7) == format_rlib_report(1, d7, out7))

    # --- 8. exit-code 契约（子进程真跑；不测 --selftest 自身 rc——自引用
    #     即递归，rc=0 契约由本函数返回值本身兑现） ---
    p = sp.run([sys.executable, __file__, "bogus-mode", "x"],
               capture_output=True, text=True)
    check("bogus mode exit 1 + usage on stderr",
          p.returncode == 1 and "rlib-surface" in p.stderr, f"rc={p.returncode}")
    p0 = sp.run([sys.executable, __file__], capture_output=True, text=True)
    check("no-args exit 1 + usage", p0.returncode == 1 and
          "elf-surface" in p0.stderr, f"rc={p0.returncode}")
    p3 = sp.run([sys.executable, __file__, "--selftest", "extra"],
                capture_output=True, text=True)
    check("--selftest exact-arg guard (extra arg = usage, not silent run)",
          p3.returncode == 1, f"rc={p3.returncode}")

    print(f"\nshim_audit selftest: {'PASS' if not failures else 'FAIL'}"
          f"{' x' + str(len(failures)) if failures else ''}")
    return 0 if not failures else 1


def main() -> None:
    if len(sys.argv) == 2 and sys.argv[1] == "--selftest":
        sys.exit(selftest())
    if len(sys.argv) != 3 or sys.argv[1] not in ("rlib-surface", "elf-surface"):
        sys.exit(__doc__)
    if sys.argv[1] == "rlib-surface":
        rlib_surface(Path(sys.argv[2]))
    else:
        elf_surface(Path(sys.argv[2]))


if __name__ == "__main__":
    main()
