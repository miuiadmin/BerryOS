#!/usr/bin/env python3
"""svc_registry_check —— SVC 落类注册表三方对拍（W3-Q67 执法件）。

单源纪律：号→锁类的唯一事实源是 `berry-svcface` 的 `REGISTRY`（43 项
`(nr, class)` 对）。`kernel/src/syscall.rs install()` 的 43 行 `reg()`
字面量与 `arch/aarch64/src/kern_lock.rs` 的六类常量 re-export 都是消费面
——漂移（改一处忘另一处）在编译期无咬合：reg() 吃的是 NR_* 常量与
SVC_CLASS_* 常量，值相同名字对不上也照编。本脚本按文本解析三方对拍：

  1. `kernel/src/abi_gen.rs` 的 `pub const NR_*: usize = <val>`（含
     syscall.rs 内联 `const NR_*`）——号值域；
  2. `kernel/src/syscall.rs install()` 的 `reg(NR_X | 0xNN, fn, SVC_CLASS_Y)`
     ——45 行期望表；
  3. `svcface/src/lib.rs` 的 `REGISTRY: &[(u8, u8)]`——实存表。

判据（任一失配退出 1）：
  - install() 行数 == REGISTRY 项数 == 45；
  - 逐号 (nr 值, 类常量名) 全等（序不敏感；install 侧 NR_* 先经 1 解析成值）；
  - REGISTRY 无重复号；
  - install()/REGISTRY 引用的 SVC_CLASS_* 常量名都在 svcface lib.rs 定义。

用法：`python3 tools/svc_registry_check.py [--check]`（无参数重产模式
保留给未来扩展——当前只有判序面，行为与 --check 一致）。
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXPECT_ENTRIES = 45


def parse_nr_consts() -> dict:
    """NR_* 号常量值域：abi_gen.rs 全量 + syscall.rs 内联 const。"""
    nrs: dict[str, int] = {}
    for rel in ("kernel/src/abi_gen.rs", "kernel/src/syscall.rs"):
        text = (ROOT / rel).read_text(encoding="utf-8")
        for m in re.finditer(
            r"^\s*(?:pub )?const (NR_\w+):\s*usize\s*=\s*(0x[0-9A-Fa-f]+|\d+)\s*;",
            text,
            re.M,
        ):
            nrs[m.group(1)] = int(m.group(2), 0)
    return nrs


def parse_install() -> list:
    """install() 的 reg() 行 → [(nr_value, SVC_CLASS_*, 原始号token)]。"""
    text = (ROOT / "kernel/src/syscall.rs").read_text(encoding="utf-8")
    out = []
    for m in re.finditer(
        r"reg\(\s*(NR_\w+|0x[0-9A-Fa-f]+)\s*,\s*\w+\s*,\s*(SVC_CLASS_\w+)\s*\)",
        text,
    ):
        tok = m.group(1)
        out.append((tok, m.group(2)))
    return out


def parse_registry() -> list:
    """svcface REGISTRY 字面量表 → [(nr_value, SVC_CLASS_*)]。"""
    text = (ROOT / "svcface/src/lib.rs").read_text(encoding="utf-8")
    m = re.search(r"pub const REGISTRY[^=]*=\s*&\[(.*?)\]\s*;", text, re.S)
    if not m:
        raise SystemExit("svc_registry_check: REGISTRY 块未找到（svcface/src/lib.rs）")
    return [
        (int(a, 16), b)
        for a, b in re.findall(r"\(\s*(0x[0-9A-Fa-f]+)\s*,\s*(SVC_CLASS_\w+)\s*\)", m.group(1))
    ]


def defined_class_consts() -> set:
    text = (ROOT / "svcface/src/lib.rs").read_text(encoding="utf-8")
    return set(re.findall(r"pub const (SVC_CLASS_\w+):\s*u8", text))


def main() -> int:
    nrs = parse_nr_consts()
    install_tok = parse_install()
    registry = parse_registry()
    defined = defined_class_consts()
    fails: list[str] = []

    def want(n: int, got: int, msg: str):
        if got != n:
            fails.append(f"{msg}: 期望 {n} 实得 {got}")

    want(EXPECT_ENTRIES, len(install_tok), "install() reg() 行数")
    want(EXPECT_ENTRIES, len(registry), "REGISTRY 项数")

    install = []
    for tok, cls in install_tok:
        if tok.startswith("0x"):
            val = int(tok, 0)
        elif tok in nrs:
            val = nrs[tok]
        else:
            fails.append(f"install() 号 token {tok} 未在 NR 常量域解析到")
            continue
        install.append((val, cls))

    inst_map = dict(install)
    if len(inst_map) != len(install):
        fails.append("install() 存在重复号")
    reg_map = dict(registry)
    if len(reg_map) != len(registry):
        fails.append("REGISTRY 存在重复号")

    for val in sorted(set(inst_map) | set(reg_map)):
        a, b = inst_map.get(val), reg_map.get(val)
        if a != b:
            fails.append(f"nr={val:#04x} 失配: install={a} REGISTRY={b}")

    for name in sorted({c for _, c in install} | {c for _, c in registry}):
        if name not in defined:
            fails.append(f"类常量 {name} 未在 svcface lib.rs 定义")

    if fails:
        print("svc_registry_check: FAIL")
        for f in fails:
            print("  -", f)
        return 1
    print(f"svc_registry_check: PASS（{len(registry)} 号三方对拍零失配——"
          f"install() ↔ REGISTRY ↔ NR 常量域）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
