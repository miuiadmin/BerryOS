#!/usr/bin/env python3
"""BerryOS ABI 单源生成器（`tools/abi.toml` → 三面产物）。

产物：
  1. kernel/src/abi_gen.rs              —— NR/CLOCK/CHAN_OPT/RIGHT/SIGNAL/ERR（syscall.rs/handle.rs 转发）
  2. user/berry-sys/src/abi_gen.rs  —— 同面镜像（+ INVALID_HANDLE + 私有 mod nr）
  3. docs/系统调用ABI设计.md           —— §4 编号表体 + §8.2 两段 C 定义列（marker 块替换）

用法：
  python3 tools/gen_abi.py           # 生成（写盘；文档块外的内容一字不动）
  python3 tools/gen_abi.py --check   # 逐字节比对盘上三产物 vs 期望——
                                      # 挂 just abi-check（build/smoke 前置）：
                                      # 改了 abi.toml 忘再生成 / 手改产物 → FAIL。

保真策略：产物 checked-in（盘上文件即真相，diff 可审、blame 可查）——同
elf2flat/mkcpio/size.py --assert-flip 的房子风格；不用 build.rs 运行时生成
（berry-sys 无 build.rs；rerun-if-changed 漏列 = 静默旧字节，G-eco-b 教训）。

环境要求：Python ≥3.11（stdlib tomllib；macOS CLT 自带 /usr/bin/python3 是
3.9 跑不了——交互 PATH 的 brew python 不受影响，版本不足时给出干净 FAIL）。

只覆盖**常量层**（号/位/码/宽度）；§8.3 原型层（berry_* 签名与 .h）仍手写
双镜像——v2 挂账（ABI 文档 §8 引言）。
"""

import argparse
import re
import sys

try:
    import tomllib
except ImportError:
    print("abi-gen: FAIL——需要 Python ≥3.11（stdlib tomllib）；本工具链挂"
          " just build/check/smoke 前置，请用 brew python3 或升级后重试",
          file=sys.stderr)
    sys.exit(1)

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
IDL = ROOT / "tools" / "abi.toml"
KERNEL_RS = ROOT / "kernel" / "src" / "abi_gen.rs"
USER_RS = ROOT / "user" / "berry-sys" / "src" / "abi_gen.rs"
DOC = ROOT / "docs" / "系统调用ABI设计.md"

# 每端类型映射（u32/u64 按端约定不同是** ABI 决策**，不是漂移——berry-sys 的
# CHAN_OPT/CLOCK 是 u32（参数寄存器值），内核侧按语义类型收 u64。）
KERNEL_NR_T = "usize"
KERNEL_ERR_T = "i32"
USER_CHANOPT_T = "u32"
USER_CLOCK_T = "u32"


class Fail(Exception):
    """关卡失败（--check 或校验不过）——消息直给 stderr。"""


# ---------------------------------------------------------------- 求值器

def eval_expr(expr: str) -> int:
    """IDL `expr` → 数值。支持 `1 << N` / `u64::MAX` / 整数字面（0x/十进制/_ 分隔）。"""
    e = expr.replace(" ", "")
    if e == "u64::MAX":
        return (1 << 64) - 1
    m = re.fullmatch(r"1<<(\d+)", e)
    if m:
        return 1 << int(m.group(1))
    try:
        return int(e.replace("_", ""), 0)
    except ValueError:
        raise Fail(f"abi.toml: expr 不可求值：{expr!r}")


def c_value(expr: str) -> str:
    """IDL `expr` → C 宏值（§8.2 列）。与 eval_expr 同吃空格归一形式——非规范
    拼写（`1<<40`）不得原样透传：裸进 C 是对 int 移位 = UB、`u64 :: MAX` 更是
    非法 C（对抗审查修：eval/c_value 两面必须同一归一，否则校验过、产物坏）。"""
    e = expr.replace(" ", "")
    if e == "u64::MAX":
        return "(-1LL)"
    m = re.fullmatch(r"1<<(\d+)", e)
    if m:
        return f"(1ULL << {m.group(1)})"
    if re.fullmatch(r"(?:0[xX][0-9A-Fa-f]+|\d+)", e):
        return e
    raise Fail(f"abi.toml: right.expr 无 C 宏映射（只收 `1 << N` / `u64::MAX` / 整数字面）：{expr!r}")


# ---------------------------------------------------------------- 装载 + 校验

def load() -> dict:
    try:
        d = tomllib.loads(IDL.read_text("utf-8"))
    except tomllib.TOMLDecodeError as e:
        raise Fail(f"abi.toml 语法错：{e}")
    except OSError as e:
        raise Fail(f"读单源文件失败：{IDL}（{e}）")
    for key in ("meta", "types"):
        if key not in d:
            raise Fail(f"abi.toml: 缺 [{key}] 段")
    for key in ("clock", "option", "right", "signal", "errno", "group"):
        if key not in d:
            raise Fail(f"abi.toml: 缺 [[{key}]] 段")
        if not d[key]:
            raise Fail(f"abi.toml: [[{key}]] 段为空（清空段请连段一起删——"
                       "空表无内容可产且会让 C 列宽计算裸崩）")
    meta = d["meta"]
    rows = [s for g in d["group"] for s in g["syscall"]]
    v1 = [s for s in rows if s["stage"] == "v1"]
    impl = [s for s in v1 if s.get("implemented", True)]
    if len({s["num"] for s in rows}) != len(rows):
        raise Fail("abi.toml: syscall num 重复")
    if len(v1) != meta["nr_total"]:
        raise Fail(f"abi.toml: meta.nr_total={meta['nr_total']} != v1 行数 {len(v1)}")
    if len(impl) != meta["implemented"]:
        raise Fail(f"abi.toml: meta.implemented={meta['implemented']} != 实际 {len(impl)}")
    for sec in ("clock", "option", "right", "signal", "errno"):
        names = [e["name"] for e in d[sec]]
        if len(set(names)) != len(names):
            raise Fail(f"abi.toml: [[{sec}]] name 重复")
        vals = [eval_expr(e["expr"]) for e in d[sec]]
        if len(set(vals)) != len(vals):
            raise Fail(f"abi.toml: [[{sec}]] expr 值重复")
    return d


def v1_rows(d: dict) -> list:
    return [s for g in d["group"] for s in g["syscall"] if s["stage"] == "v1"]


# ---------------------------------------------------------------- Rust 生成

def pick_desc(e: dict) -> str:
    """berry-sys 侧文档注释：优先 `desc_user`——两端注释历史上不同文案的条目
    （CLOCK_MONOTONIC/RIGHTS_SAME）按「迁移 = 逐字搬运」保真，不强行统一。"""
    return e.get("desc_user") or e.get("desc", "")


def push_doc(L: list, text: str):
    """desc/note → `///` 文档行（空串零行）。"""
    if not text:
        return
    for ln in text.split("\n"):
        L.append(("/// " + ln).rstrip())


GEN_HEADER = [
    "//! ABI 常量层——**生成文件，勿手改**。",
    "//! 单源 = `tools/abi.toml`；再生 = `just abi-gen`；",
    "//! 守卫 = `just abi-check`（`tools/gen_abi.py --check`，挂 build/smoke 前置）。",
]


def gen_kernel_rs(d: dict) -> str:
    meta = d["meta"]
    rows = v1_rows(d)
    unimpl = [s for s in rows if not s.get("implemented", True)]
    L = list(GEN_HEADER)
    L.append("//! 消费：`syscall.rs` / `handle.rs` 经 `pub use` 转发，既有路径")
    L.append("//! （`crate::syscall::NR_*`、`crate::handle::RIGHT_*` 等）零改动。")
    L.append("")
    L.append("use crate::handle::Rights;")
    L.append("")
    L.append("// ---- syscall 号（ABI §4 编号表；"
             f"v1 {meta['nr_total']} 号在册、{meta['implemented']} 已实现）----")
    if unimpl:
        names = "、".join(f"0x{s['num']:02X} {s['name']}" for s in unimpl)
        L.append(f"// 在册未实现：{names}（派发回落 -ENOTSUP，永不 panic）")
    for s in rows:
        push_doc(L, s.get("note", ""))
        L.append(f"pub const NR_{s['name'].upper()}: {KERNEL_NR_T} = 0x{s['num']:02X};")
    L.append("")
    L.append("// ---- clock id（ABI §8.3 `berry_clock_id_t`）----")
    for c in d["clock"]:
        push_doc(L, c.get("desc", ""))
        L.append(f"pub const CLOCK_{c['name']}: u64 = {c['expr']};")
    L.append("")
    L.append("// ---- options 位（ABI §8「options 纪律」：未知位一律 -EINVAL）----")
    for o in d["option"]:
        push_doc(L, o.get("desc", ""))
        L.append(f"pub const {o['name']}: u64 = {o['expr']};")
    L.append("")
    L.append("// ---- rights 位（`能力模型设计.md` §2；位表冻结，未消费位全量在册）----")
    for r in d["right"]:
        push_doc(L, r.get("desc", ""))
        cn = r.get("rust") or f"RIGHT_{r['name']}"
        L.append(f"pub const {cn}: Rights = {r['expr']};")
    L.append("")
    L.append("// ---- wait 信号位（4-2 位空间：低 32 内核派生 + 高 32 用户）----")
    for s in d["signal"]:
        push_doc(L, s.get("desc", ""))
        L.append(f"pub const SIGNAL_{s['name']}: u64 = {s['expr']};")
    L.append("")
    L.append("// ---- 错误码（posix-ish 负数；ABI §8.2，`berry_status_t` = i32）----")
    for e in d["errno"]:
        if not e.get("rust", True):
            continue
        push_doc(L, e.get("desc", ""))
        L.append(f"pub const ERR_{e['name']}: {KERNEL_ERR_T} = {e['expr']};")
    L.append("")
    L.append("// ---- 宽度冻结断言（abi.toml [types]；G2 冻结面的机器执法）----")
    t = d["types"]
    L.append(f"const _: () = assert!(core::mem::size_of::<crate::handle::Handle>() == {t['handle']});")
    L.append(f"const _: () = assert!(core::mem::size_of::<crate::handle::Rights>() == {t['rights']});")
    L.append("")
    return "\n".join(L)


def gen_user_rs(d: dict) -> str:
    rows = v1_rows(d)
    L = list(GEN_HEADER)
    L.append("//! 与内核侧 `kernel/src/abi_gen.rs` 同源生成（两端 u32/u64 类型约定见")
    L.append("//! `tools/gen_abi.py` 头注——那是 ABI 决策不是漂移）。")
    L.append("")
    L.append("use crate::{Handle, Rights, Status, Time};")
    L.append("")
    L.append("/// 无效 handle（表槽 0 恒空；`berry_handle_t` = 0）。")
    L.append(f"pub const INVALID_HANDLE: Handle = {d['types']['invalid_handle']};")
    L.append("")
    L.append("// ---- rights 位（12 位；权限在 handle 不在对象）----")
    for r in d["right"]:
        push_doc(L, pick_desc(r))
        cn = r.get("rust") or f"RIGHT_{r['name']}"
        L.append(f"pub const {cn}: Rights = {r['expr']};")
    L.append("")
    L.append("// ---- wait 信号位（4-2 位空间：低 32 内核派生 + 高 32 用户）----")
    for s in d["signal"]:
        push_doc(L, pick_desc(s))
        L.append(f"pub const SIGNAL_{s['name']}: u64 = {s['expr']};")
    L.append("")
    L.append("// ---- options 位 / clock id ----")
    for o in d["option"]:
        push_doc(L, pick_desc(o))
        L.append(f"pub const {o['name']}: {USER_CHANOPT_T} = {o['expr']};")
    for c in d["clock"]:
        push_doc(L, pick_desc(c))
        L.append(f"pub const CLOCK_{c['name']}: {USER_CLOCK_T} = {c['expr']};")
    L.append("")
    L.append("// ---- 错误码（G2 冻结面；`berry_status_t` = i32，0 = Ok）----")
    for e in d["errno"]:
        if not e.get("rust", True):
            continue
        push_doc(L, pick_desc(e))
        L.append(f"pub const BERRY_ERR_{e['name']}: Status = {e['expr']};")
    L.append("")
    L.append("// ---- syscall 号（v1 在册；私有——用户程序不写编号，只经本 crate 的")
    L.append("// `berry_*` 封 `svc`，§8.3；未实现号 wrapper 照发、内核回落 -ENOTSUP）----")
    L.append("pub(crate) mod nr {")
    for s in rows:
        if s.get("note"):
            L.append("    /// " + s["note"].split("\n")[0])
        L.append(f"    pub(crate) const {s['name'].upper()}: usize = 0x{s['num']:02X};")
    L.append("}")
    L.append("")
    L.append("// ---- 宽度冻结断言（abi.toml [types]；G2 冻结面的机器执法）----")
    t = d["types"]
    for cn, key in (("Handle", "handle"), ("Rights", "rights"), ("Status", "status"), ("Time", "time")):
        L.append(f"const _: () = assert!(core::mem::size_of::<{cn}>() == {t[key]});")
    L.append("")
    return "\n".join(L)


# ---------------------------------------------------------------- 文档生成

# §4 表头 + 分隔行——HTML 注释 marker 独占一行是 GFM 的 HTML block、会按
# 「另一块级结构开始处」断表：marker 对只能**包整表**放表外，故表头也归生成
# 物（对抗审查 [3]/[8] 修——marker 夹在分隔行与表体间 = 整表渲染塌成竖线文本）。
DOC_TABLE_HEADER = [
    "| 号 | syscall | 说明 | 阶段 |",
    "|----|---------|------|------|",
]


def doc_syscall_table(d: dict) -> str:
    L = list(DOC_TABLE_HEADER)
    for g in d["group"]:
        L.append(f"| {g['title']} ||||")
        for s in g["syscall"]:
            L.append(f"| 0x{s['num']:02X} | `{s['name']}` | {s['desc']} | {s['stage']} |")
    return "\n".join(L)


def c_defines(entries: list) -> str:
    """entries = [(c_name, value, c_note)]；名字列对齐最长+2、值列对齐最长+2（带
    注释的行——无注释行不补尾随空白）。"""
    w = max(len(n) for n, _, _ in entries) + 2
    vw = max(len(v) for _, v, _ in entries) + 2
    L = []
    for n, v, note in entries:
        line = f"#define {n:<{w}}{v:<{vw}}" if note else f"#define {n:<{w}}{v}"
        if note:
            line += f"/* {note} */"
        L.append(line)
    return "\n".join(L)


def doc_rights_c(d: dict) -> str:
    entries = []
    for r in d["right"]:
        cn = r.get("c") or f"BERRY_RIGHT_{r['name']}"
        entries.append((cn, c_value(r["expr"]), r.get("c_note", "")))
    return c_defines(entries)


def doc_errno_c(d: dict) -> str:
    entries = []
    for e in d["errno"]:
        cn = e.get("c") or (f"BERRY_ERR_{e['name']}" if e["name"] != "OK" else "BERRY_OK")
        v = e["expr"] if e["expr"] == "0" else f"({e['expr']})"
        entries.append((cn, v, e.get("c_note", "")))
    return c_defines(entries)


DOC_BLOCKS = {
    "syscalls": doc_syscall_table,
    "rights-c": doc_rights_c,
    "errno-c": doc_errno_c,
}


def markers(key: str) -> tuple:
    """marker 对语法按落点分流：`-c` 后缀 = ```c 围栏内 → C 注释（HTML 注释
    在 fence 里逐字透传、渲染成可见的垃圾行）；markdown 语境 → HTML 注释
    （渲染不可见，但独占一行会断 GFM 表格——syscalls 对必须包整表）。"""
    if key.endswith("-c"):
        return (f"/* @gen:{key} begin */", f"/* @gen:{key} end */")
    return (f"<!-- @gen:{key} begin -->", f"<!-- @gen:{key} end -->")


def patch_doc(text: str, d: dict) -> str:
    for key, fn in DOC_BLOCKS.items():
        b, en = markers(key)
        nb, ne = text.count(b), text.count(en)
        if nb != 1 or ne != 1:
            raise Fail(f"文档 @gen:{key} 标记须恰一对（现 begin×{nb} / end×{ne}）——"
                       "重复对会留一块永远不被替换的陈旧内容，且 --check 比的是"
                       "「再生 vs 盘上」照样恒绿")
        i = text.index(b) + len(b)
        j = text.index(en)
        if j <= i:
            raise Fail(f"文档 @gen:{key} end 标记在 begin 之前（或重叠）——拒绝拼出重复段")
        text = text[:i] + "\n" + fn(d) + "\n" + text[j:]
    # 计数行交叉断言：§8.3 的 v1 计数 = meta.nr_total。
    m = re.search(r"v1 计数 = (\d+)", text)
    if not m:
        raise Fail("文档缺「v1 计数 = N」行（§8.3 计数断言锚点）")
    if int(m.group(1)) != d["meta"]["nr_total"]:
        raise Fail(f"文档 v1 计数 = {m.group(1)} != abi.toml nr_total = {d['meta']['nr_total']}")
    return text


# ---------------------------------------------------------------- 主流程

def build_all(d: dict) -> tuple:
    try:
        doc_text = DOC.read_text("utf-8")
    except OSError as e:
        raise Fail(f"读 ABI 文档失败：{DOC}（{e}）")
    return (gen_kernel_rs(d), gen_user_rs(d), patch_doc(doc_text, d))


def main() -> int:
    ap = argparse.ArgumentParser(description="BerryOS ABI 单源生成/守卫")
    ap.add_argument("--check", action="store_true",
                    help="不写盘：逐字节比对盘上三产物 vs 期望，漂移即 FAIL")
    args = ap.parse_args()
    try:
        d = load()
        k, u, doc = build_all(d)
        if args.check:
            targets = [(KERNEL_RS, k), (USER_RS, u), (DOC, doc)]
            bad = []
            for path, want in targets:
                got = path.read_text("utf-8") if path.exists() else None
                if got != want:
                    bad.append(path)
            if bad:
                for p in bad:
                    print(f"abi-check: FAIL——{p.relative_to(ROOT)} 与 abi.toml 期望不符"
                          f"（改了 abi.toml 跑 `just abi-gen`；手改了产物请改回）")
                return 1
            n = len(v1_rows(d))
            print(f"abi-check: PASS（NR v1 {n} 号在册——内核 ↔ berry-sys ↔ 文档三面单源一致）")
            return 0
        KERNEL_RS.write_text(k, "utf-8")
        USER_RS.write_text(u, "utf-8")
        DOC.write_text(doc, "utf-8")
        print(f"abi-gen: 写 {KERNEL_RS.relative_to(ROOT)} / {USER_RS.relative_to(ROOT)}"
              f" / 文档三 marker 块——提交时产物 diff 一并入册")
        return 0
    except Fail as e:
        print(f"abi-gen: FAIL——{e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
