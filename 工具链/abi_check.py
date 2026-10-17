#!/usr/bin/env python3
"""BerryOS ABI 镜像断言 —— 内核侧 / berry-sys / ABI 文档三方对账（挂账项的临时关卡）。

背景（ADR/CLAUDE.md 挂账）：「下次任何 ABI 变更前先补 IDL 单源生成 + CI 结构体
布局断言」——Plan B v0.5（0x33/0x36 落地）即一次 ABI 变更，在单源生成到位前先上
**镜像检查**：同一事实的三份手写副本（内核常量、berry-sys 镜像、ABI 文档编号表）
逐值对账，任何一侧漂移即 FAIL（exit 1）。挂进 smoke.sh（--assert-flip 之后、QEMU
之前——便宜且早响）。单源生成落地后本脚本退役。

对账面（v1 全集，双方都须完整镜像——缺项即 FAIL，不只比值）：
  1. syscall 号：内核 syscall.rs `NR_*`（pub）↔ berry-sys 私有 `mod nr` 常量
     （名 = NR_ 去前缀）↔ ABI 文档 §4 编号表（hex + snake_case 名）；
  2. rights / 信号位：内核 handle.rs `RIGHT_*`/`RIGHTS_SAME`/`SIGNAL_*` ↔
     berry-sys 同名 `pub const`；
  3. errno：内核 handle.rs `ERR_*` ↔ berry-sys `BERRY_ERR_*`（加前缀）；
  4. clock id：内核 syscall.rs `CLOCK_MONOTONIC`（私有 const）↔ berry-sys 同名。

值求值器只认 v1 常量语法（十/十六进制、负数、`1 << N`、`u64::MAX`、`_` 分隔），
出现别的表达式 = 假设失效 → FAIL（宁可误杀不静默跳过）。
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
KERNEL_SYSCALL = ROOT / "内核" / "src" / "syscall.rs"
KERNEL_HANDLE = ROOT / "内核" / "src" / "handle.rs"
BERRY_SYS = ROOT / "用户态" / "berry-sys" / "src" / "lib.rs"
ABI_DOC = ROOT / "文档" / "系统调用ABI设计.md"

# 常量声明：可选 pub / pub(super)，类型段，值表达式（止于分号）。
CONST_RE = re.compile(
    r"(?:pub(?:\(\w+\))?\s+)?const\s+([A-Z][A-Z0-9_]*)\s*:\s*[\w:]+\s*=\s*([^;]+);"
)


def parse_consts(text: str, name_filter):
    """返回 {名: 值表达式字符串}——name_filter 做前缀/全名筛选。"""
    out = {}
    for m in CONST_RE.finditer(text):
        name, expr = m.group(1), m.group(2).strip()
        if name_filter(name):
            if name in out:
                raise AssertionError(f"重复常量声明：{name}")
            out[name] = expr
    return out


def eval_const(name: str, expr: str) -> int:
    """求值 v1 常量语法；不认的式子抛错（FAIL 而非静默）。"""
    e = expr.replace(" ", "")
    if e == "u64::MAX":
        return (1 << 64) - 1
    m = re.fullmatch(r"1<<(\d+)", e)
    if m:
        return 1 << int(m.group(1))
    e2 = e.replace("_", "")
    try:
        return int(e2, 0)  # 0x.. 十六进制 / 十进制 / 负号均收
    except ValueError:
        raise AssertionError(f"{name}: 不认的常量表达式「{expr}」（须扩求值器）")


def collect():
    ks = KERNEL_SYSCALL.read_text(encoding="utf-8")
    kh = KERNEL_HANDLE.read_text(encoding="utf-8")
    bs = BERRY_SYS.read_text(encoding="utf-8")

    kernel_nr = parse_consts(ks, lambda n: n.startswith("NR_"))
    # berry-sys 私有 mod nr：整文件正则里 pub(super) 已覆盖；按 mod nr 区间截取。
    nr_mod = re.search(r"\bmod\s+nr\s*\{(.*?)\n\}", bs, re.S)
    if not nr_mod:
        raise AssertionError("berry-sys: 找不到 `mod nr` 块")
    user_nr = parse_consts(nr_mod.group(1), lambda n: True)

    # 顶层扫描前挖掉 mod nr 块（其内 NR 号常量与顶层前缀组无关，防 CLOCK_READ
    # 之类撞进 CLOCK_ 组）。
    bs_top = bs.replace(nr_mod.group(0), "")

    kernel_rights = parse_consts(kh, lambda n: n.startswith(("RIGHT_", "RIGHTS_")))
    user_rights = parse_consts(bs_top, lambda n: n.startswith(("RIGHT_", "RIGHTS_")))
    kernel_sig = parse_consts(kh, lambda n: n.startswith("SIGNAL_"))
    user_sig = parse_consts(bs_top, lambda n: n.startswith("SIGNAL_"))
    kernel_err = parse_consts(kh, lambda n: n.startswith("ERR_"))
    user_err = parse_consts(bs_top, lambda n: n.startswith("BERRY_ERR_"))

    # clock id：内核侧是私有 const（无 pub），parse_consts 的可选 pub 已兼容。
    kernel_clock = parse_consts(ks, lambda n: n.startswith("CLOCK_"))
    user_clock = parse_consts(bs_top, lambda n: n.startswith("CLOCK_"))

    return {
        "NR": (kernel_nr, user_nr),
        "RIGHT/SIGNAL": (kernel_rights | kernel_sig, user_rights | user_sig),
        "ERR": (kernel_err, user_err),
        "CLOCK": (kernel_clock, user_clock),
    }


def norm_nr(name: str) -> str:
    return name[len("NR_"):] if name.startswith("NR_") else name


def norm_err(name: str) -> str:
    """两侧 errno 名归一到裸词干：内核 ERR_X / berry-sys BERRY_ERR_X → X。"""
    if name.startswith("BERRY_ERR_"):
        return name[len("BERRY_ERR_"):]
    if name.startswith("ERR_"):
        return name[len("ERR_"):]
    return name


def main() -> int:
    problems = []

    groups = collect()

    # —— 1. 内核 ↔ berry-sys 双向对账 ——
    totals = {}
    for group, (kern, user) in groups.items():
        if group == "NR":
            kn = {norm_nr(n): (n, e) for n, e in kern.items()}
            un = {n: (n, e) for n, e in user.items()}
        elif group == "ERR":
            kn = {norm_err(n): (n, e) for n, e in kern.items()}
            un = {norm_err(n): (n, e) for n, e in user.items()}
        else:
            kn = {n: (n, e) for n, e in kern.items()}
            un = {n: (n, e) for n, e in user.items()}
        for base, (kname, kexpr) in kn.items():
            if base not in un:
                problems.append(f"{group}: berry-sys 缺 {base}（内核 {kname}={kexpr}）")
                continue
            kv = eval_const(kname, kexpr)
            uv = eval_const(un[base][0], un[base][1])
            if kv != uv:
                problems.append(f"{group}: {base} 值不一致：内核 {kname}={kv:#x} vs berry-sys {un[base][0]}={uv:#x}")
        for base, (uname, uexpr) in un.items():
            if base not in kn:
                problems.append(f"{group}: 内核缺 {base}（berry-sys {uname}={uexpr}）")
        totals[group] = (len(kn), len(un))

    # —— 2. ABI 文档编号表对账（双向：内核 NR 号在册 + 行内名 = snake_case；
    #         文档 v1 行必须有内核 NR 在册——只做正向会静默放过幽灵/忘删行）——
    # 行形 = | 0xNN | `name` | 描述… | vN |（版本列收尾；描述列不含裸 `|`）。
    doc_rows = {}
    doc_v1 = {}
    for m in re.finditer(
        r"^\|\s*(0x[0-9A-Fa-f]{2})\s*\|\s*`([a-z_0-9]+)`\s*\|(.*?)\|\s*(v\d+)\s*\|\s*$",
        ABI_DOC.read_text(encoding="utf-8"), re.M,
    ):
        num = int(m.group(1), 16)
        doc_rows[num] = m.group(2)
        if m.group(4) == "v1":
            doc_v1[num] = m.group(2)
    kernel_nr = groups["NR"][0]
    for name, expr in kernel_nr.items():
        v = eval_const(name, expr)
        want = name[len("NR_"):].lower()
        if v not in doc_rows:
            problems.append(f"DOC: 编号表缺 0x{v:02X}（内核 {name}）")
        elif doc_rows[v] != want:
            problems.append(f"DOC: 0x{v:02X} 行名 {doc_rows[v]!r} ≠ 内核 {want!r}（{name}）")
    kern_nr_values = {eval_const(n, e) for n, e in kernel_nr.items()}
    for num, name in sorted(doc_v1.items()):
        if num not in kern_nr_values:
            problems.append(f"DOC: v1 行 0x{num:02X}（{name}）无内核 NR_* 在册（幽灵行/废弃号忘删？）")

    if problems:
        print("abi-check: FAIL")
        for p in problems:
            print(f"  - {p}")
        print(f"  （对账面：{totals}；文档编号表 {len(doc_rows)} 行）")
        return 1

    n_nr = totals["NR"][0]
    print(f"abi-check: PASS（NR {n_nr} 号 × rights/signal "
          f"{totals['RIGHT/SIGNAL'][0]} × errno {totals['ERR'][0]} "
          f"× clock {totals['CLOCK'][0]}，内核 ↔ berry-sys ↔ 文档三方一致）")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except AssertionError as e:
        print(f"abi-check: FAIL: {e}")
        sys.exit(1)
