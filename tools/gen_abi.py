#!/usr/bin/env python3
"""BerryOS ABI 单源生成器（`tools/abi.toml` → 五面产物）。

产物：
  1. kernel/src/abi_gen.rs              —— NR/CLOCK/CHAN_OPT/RIGHT/SIGNAL/ERR（syscall.rs/handle.rs 转发）
  2. user/berry-sys/src/abi_gen.rs  —— 同面镜像（+ INVALID_HANDLE + 私有 mod nr）
  3. docs/系统调用ABI设计-v1.0.md    —— §4 编号表体 + §8.2 三段 C 定义列（marker 块替换）
  4. tools/abi.syz                      —— syzkaller syscall 描述骨架（SEC-01 §4.5 件 25；未过
                                          syz-sysgen 编译验证——头注如实声明，详见 gen_syz）
  5. arch/aarch64/src/wake_gen.rs   —— 唤醒源枚举常量 WAKE_SRC_*（刀 1：trap 臂源映射/
                                          cell 初值/内核 telemetry per-source 记账同源；用户侧
                                          berry-power 刀落地时从同 [[wakesrc]] 再生用户面常量）

用法：
  python3 tools/gen_abi.py           # 生成（写盘；文档块外的内容一字不动）
  python3 tools/gen_abi.py --check   # 逐字节比对盘上四产物 vs 期望——
                                      # 挂 just abi-check（build/smoke 前置）：
                                      # 改了 abi.toml 忘再生成 / 手改产物 → FAIL；
                                      # + G-ver-1（版本管理标准 §3）：[platform]
                                      # level 现值 < git HEAD 值 → FAIL（无 git /
                                      # HEAD 无该段 = 豁免注记不红）。
  python3 tools/gen_abi.py --selftest  # 门禁自检（挂 just test-host 前置）：
                                      # 求值器/C 映射/号段/四元组/行筛选/
                                      # syz 投影/marker 替换/装载校验钉板 +
                                      # 在册五产物逐字节 oracle（a-3 U4 纪律
                                      # 同款；--check 只拦漂移不拦逻辑改坏）。

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
ARCH_RS = ROOT / "arch" / "aarch64" / "src" / "wake_gen.rs"
DOC = ROOT / "docs" / "系统调用ABI设计-v1.0.md"
SYZ = ROOT / "tools" / "abi.syz"

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


def next_free_wakesrc(wv: dict) -> int | None:
    """号段下一个空闲号（0x01–0x7E 最小未占；NONE/UNKNOWN 两语义锚永不入
    空闲集）。报错文案与 wake_gen 头注共用——空闲段活算而非硬抄区间，
    再占一号不需回头改文案（sim-S2 审查裁决：0x07=POWER_KEY 后三处
    『预留段 0x07–0x7E』旧文全数漂移，根因即硬编码区间）。"""
    return next((v for v in range(1, 0x7F) if v not in wv.values()), None)


# ---------------------------------------------------------------- 装载 + 校验

def load() -> dict:
    try:
        d = tomllib.loads(IDL.read_text("utf-8"))
    except tomllib.TOMLDecodeError as e:
        raise Fail(f"abi.toml 语法错：{e}")
    except OSError as e:
        raise Fail(f"读单源文件失败：{IDL}（{e}）")
    return validate(d)


def validate(d: dict) -> dict:
    """单源校验（--selftest 钉板位；load = 读盘 + 本函数）。"""
    for key in ("meta", "platform", "types"):
        if key not in d:
            raise Fail(f"abi.toml: 缺 [{key}] 段")
    for key in ("clock", "option", "right", "signal", "errno", "group", "wakesrc"):
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
    # 预算四元组（刀 1 前置 #8，PM-17 终审 §4 裁 B 机制）：nr_ceiling 在册 +
    # nr_defined ≤ 帽执法；口径唯一收口进两端 abi_gen 头注——消灭 35/39/38
    # 三口径打架的文档面（「稳定性靠执法工具不靠文档承诺」，ADR #55）。
    for key in ("nr_total", "implemented", "nr_ceiling"):
        if key not in meta:
            raise Fail(f"abi.toml: [meta] 缺 {key}（四元组执法面）")
    # [platform] level：单调整数 API level 轨道（VER-01 提案第 4 条）——
    # ≥1 整数（bool 是 int 子类，显式拒——TOML `level = true` 不是 1）；
    # 「只增不减、不兼容变更 +1、兼容新增不 +1」是流程纪律（改值 = 不兼容
    # ABI 变更，评审执法），单源只钉形状。
    lvl = d["platform"].get("level")
    if not isinstance(lvl, int) or isinstance(lvl, bool) or lvl < 1:
        raise Fail("abi.toml: [platform] level 应为 ≥1 的整数（单调整数轨道："
                   "只增不减、不兼容 ABI 变更 +1、兼容新增不 +1）")
    _, nr_defined, nr_registered, _ = four_tuple(d)
    if nr_defined > meta["nr_ceiling"]:
        raise Fail(f"abi.toml: nr_defined={nr_defined} > nr_ceiling={meta['nr_ceiling']}"
                   "（ADR #11 号面帽——回收/合并后再加号）")
    if nr_registered > nr_defined:
        raise Fail(f"abi.toml: nr_registered={nr_registered} > nr_defined={nr_defined}（口径反常）")
    for sec in ("clock", "option", "right", "signal", "errno", "wakesrc"):
        names = [e["name"] for e in d[sec]]
        if len(set(names)) != len(names):
            raise Fail(f"abi.toml: [[{sec}]] name 重复")
        if sec == "option":
            # options 位是 per-syscall 位图（scope 域声明归属 syscall）——
            # 值唯一性按 scope 执法；缺 scope 视同全局命名空间
            pairs = [(e.get("scope", ""), eval_expr(e["expr"])) for e in d[sec]]
            if len(set(pairs)) != len(pairs):
                raise Fail("abi.toml: [[option]] 同 scope 下 expr 值重复")
        else:
            vals = [eval_expr(e["expr"]) for e in d[sec]]
            if len(set(vals)) != len(vals):
                raise Fail(f"abi.toml: [[{sec}]] expr 值重复")
    # [[wakesrc]] 值域纪律：u8 且 ≤ 0x7F；NONE=0x00 / UNKNOWN=0x7F 两语义锚
    # 必在册（cell 哨兵与未知臂映射的编译期前提——缺任一 = 消费面悬空）。
    wv = {w["name"]: eval_expr(w["expr"]) for w in d["wakesrc"]}
    for name, val in wv.items():
        if not 0 <= val <= 0x7F:
            free = next_free_wakesrc(wv)
            hint = f"下一位空闲 0x{free:02X}" if free is not None else "号段已满"
            raise Fail(f"abi.toml: wakesrc {name}={val} 出值域（u8 且 ≤ 0x7F——"
                       f"0x7F 即 UNKNOWN 锚，{hint}；占用以在册为准）")
    for anchor, want in (("NONE", 0x00), ("UNKNOWN", 0x7F)):
        if wv.get(anchor) != want:
            raise Fail(f"abi.toml: wakesrc 语义锚 {anchor}=0x{want:02X} 缺失或变值"
                       "（add-only 纪律——哨兵/未知锚不得改值）")
    return d


def v1_rows(d: dict) -> list:
    return [s for g in d["group"] for s in g["syscall"] if s["stage"] == "v1"]


def all_rows(d: dict) -> list:
    """所有在册号（含 1.1/v2/v3）——文档/ syz 全量表用；常量只发 v1+1.1。"""
    return [s for g in d["group"] for s in g["syscall"]]


def const_rows(d: dict) -> list:
    """常量层镜像 v1 + 1.1（v2/v3 未到号，不提前发常量）。"""
    return [s for g in d["group"] for s in g["syscall"] if s["stage"] in ("v1", "1.1")]


def four_tuple(d: dict) -> tuple:
    """预算四元组（刀 1 前置 #8，PM-17 终审 §4）：(nr_total, nr_defined,
    nr_registered, nr_ceiling)。nr_total = v1 在册行数；nr_defined = v1+1.1
    常量层在册（abi_gen `pub const NR_` 计数面）；nr_registered = 其中
    implemented 行数（= 已挂派发号数——在册未实现号恒 -ENOTSUP 回落不挂
    reg）；nr_ceiling = ADR #11 号面帽。load() 执法 nr_defined ≤ 帽。"""
    defined = const_rows(d)
    registered = [s for s in defined if s.get("implemented", True)]
    return (d["meta"]["nr_total"], len(defined), len(registered), d["meta"]["nr_ceiling"])


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
    rows = const_rows(d)
    v1 = v1_rows(d)
    unimpl = [s for s in v1 if not s.get("implemented", True)]
    extra = [s for s in rows if s["stage"] != "v1"]
    L = list(GEN_HEADER)
    L.append("//! 消费：`syscall.rs` / `handle.rs` 经 `pub use` 转发，既有路径")
    L.append("//! （`crate::syscall::NR_*`、`crate::handle::RIGHT_*` 等）零改动。")
    nt, nd, nr, nc = four_tuple(d)
    L.append(f"//! 预算四元组（生成器执法，ADR #11 帽）：nr_total={nt}（v1 在册）/")
    L.append(f"//! nr_defined={nd}（v1+1.1 常量层）/ nr_registered={nr}（已挂派发）/")
    L.append(f"//! nr_ceiling={nc}（nr_defined ≤ 帽 = `tools/gen_abi.py` load 执法）。")
    L.append("")
    L.append("use crate::handle::Rights;")
    L.append("")
    # [platform] level（VER-01 提案第 4 条）：两端同源同值；内核侧暂无消费者
    # （装载期门在用户态 svcmgr），照在册未实现号先例豁免 dead_code。
    L.append("// ---- 平台 API level（abi.toml [platform]；VER-01 提案第 4 条：单调整数")
    L.append("// level 轨道——只增不减、不兼容 ABI 变更 +1、兼容新增不 +1。内核侧")
    L.append("// 在册无消费者（装载期门在 svcmgr），照在册未实现号先例豁免 dead_code）----")
    L.append("/// svcmgr 装载期与 .bap `[compat] min_level` 比较的平台级基线")
    L.append("///（用户侧同名常量同源同值，两端在册防漂移）。")
    L.append("#[allow(dead_code)]")
    L.append(f"pub const BERRY_PLATFORM_LEVEL: u32 = {d['platform']['level']};")
    L.append("")
    L.append("// ---- syscall 号（ABI §4 编号表；"
             f"v1 {meta['nr_total']} 号在册、{meta['implemented']} 已实现）----")
    if unimpl:
        names = "、".join(f"0x{s['num']:02X} {s['name']}" for s in unimpl)
        L.append(f"// 在册未实现：{names}（派发回落 -ENOTSUP，永不 panic）")
    # 1.1 行只在真未实现时列（S1/S3-b 翻真后全实现——stage 全集 ≠ 未实现集，
    # 陈旧「全列」标签曾与 nr_registered=已挂派发自相矛盾，2026-11 X-3b 收口）。
    extra_unimpl = [s for s in extra if not s.get("implemented", True)]
    if extra_unimpl:
        names = "、".join(f"0x{s['num']:02X} {s['name']}" for s in extra_unimpl)
        L.append(f"// 1.1/S1 在册未实现：{names}（派发回落 -ENOTSUP，永不 panic）")
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
    rows = const_rows(d)
    nt, nd, nr, nc = four_tuple(d)
    L = list(GEN_HEADER)
    L.append("//! 与内核侧 `kernel/src/abi_gen.rs` 同源生成（两端 u32/u64 类型约定见")
    L.append("//! `tools/gen_abi.py` 头注——那是 ABI 决策不是漂移）。")
    L.append(f"//! 预算四元组（生成器执法，ADR #11 帽）：nr_total={nt} / nr_defined={nd} /")
    L.append(f"//! nr_registered={nr} / nr_ceiling={nc}（口径与内核侧头注同源同值）。")
    L.append("")
    L.append("use crate::{Handle, Rights, Status, Time};")
    L.append("")
    L.append("/// 无效 handle（表槽 0 恒空；`berry_handle_t` = 0）。")
    L.append(f"pub const INVALID_HANDLE: Handle = {d['types']['invalid_handle']};")
    L.append("")
    # [platform] level（VER-01 提案第 4 条）：svcmgr 装载期消费（min_level 门）。
    L.append("// ---- 平台 API level（abi.toml [platform]；与内核侧同源同值）----")
    L.append("/// 单调整数 level 轨道（VER-01 提案第 4 条：只增不减、不兼容 ABI 变更")
    L.append("/// +1、兼容新增不 +1——Android API level 形而更简）；svcmgr 装载期与")
    L.append("/// .bap `[compat] min_level` 比较的平台级基线。")
    L.append(f"pub const BERRY_PLATFORM_LEVEL: u32 = {d['platform']['level']};")
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
    L.append("// ---- syscall 号（v1/1.1 在册；私有——用户程序不写编号，只经本 crate 的")
    L.append("// `berry_*` 封 `svc`，§8.3；在册未实现号常量照发、无 wrapper（原型层")
    L.append("// v2 挂账——dead_code 豁免随生成器），内核回落 -ENOTSUP）----")
    L.append("pub(crate) mod nr {")
    for s in rows:
        if s.get("note"):
            # 多行 note 全文逐行（DOC-6 修：旧「取首行」把 0x21 §5-⑤ 整段
            # 截尾——berry-sys 读者只见「…睡 pager」断句）。
            for nl in s["note"].split("\n"):
                L.append("    /// " + nl)
        if not s.get("implemented", True):
            L.append("    // 在册未实现：无 berry_* wrapper 消费（§8.3 原型层 v2 挂账）——豁免 dead_code。")
            L.append("    #[allow(dead_code)]")
        L.append(f"    pub(crate) const {s['name'].upper()}: usize = 0x{s['num']:02X};")
    L.append("}")
    L.append("")
    L.append("// ---- 宽度冻结断言（abi.toml [types]；G2 冻结面的机器执法）----")
    t = d["types"]
    for cn, key in (("Handle", "handle"), ("Rights", "rights"), ("Status", "status"), ("Time", "time")):
        L.append(f"const _: () = assert!(core::mem::size_of::<{cn}>() == {t[key]});")
    L.append("")
    return "\n".join(L)


# ---------------------------------------------------------------- 唤醒源生成（第五产物）

def gen_arch_rs(d: dict) -> str:
    """arch/aarch64/src/wake_gen.rs —— [[wakesrc]] → WAKE_SRC_* u8 常量。

    消费面：`wake.rs` per-CPU cell 初值（KERNEL_BOOT/NONE）、`trap.rs` IRQ 臂
    IAR→源映射、内核 telemetry per-source 记账（kernel 经
    `berry_hal_aarch64::wake_gen` 同名直引——arch 与 kernel 同值同源，
    漂移被 --check 逐字节拦）。用户侧常量随 berry-power 刀（0x84
    telemetry_open 快照面）从同 toml 段再生——本刀只冻形状不实现。"""
    L = list(GEN_HEADER)
    L.append("//! 唤醒源枚举（`tools/abi.toml` [[wakesrc]]；add-only——新源只追加，")
    wv = {w["name"]: eval_expr(w["expr"]) for w in d["wakesrc"]}
    free = next_free_wakesrc(wv)
    hint = f"下一位空闲 0x{free:02X}" if free is not None else "号段已满"
    L.append(f"//! 不改值/不改义/不复用退役值，值域 u8 ≤ 0x7F，{hint}，占用以在册为准）。")
    L.append("//! 消费：`wake.rs`（per-CPU cell 初值）+ `trap.rs`（IRQ 臂 IAR→源")
    L.append("//! 映射）；内核 telemetry per-source 记账同名常量直引。语义协议详注 →")
    L.append("//! research/pm-notes/刀1-前置两问-裁决.md §5（PM-17 终审问①裁 A1）。")
    L.append("")
    L.append("// ---- 唤醒源（哨兵/锚 = NONE 0x00 与 UNKNOWN 0x7F，add-only）----")
    for w in d["wakesrc"]:
        push_doc(L, w.get("desc", ""))
        L.append(f"pub const WAKE_SRC_{w['name']}: u8 = {w['expr']};")
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


def meta_platform_level(d: dict) -> int:
    """[platform] level 单值读口（syz 注释/文档 C 列共用；validate 已钉形状）。"""
    return d["platform"]["level"]


def doc_platform_c(d: dict) -> str:
    """§8.2 平台 API level C 行（VER-01 提案第 4 条——单调整数轨道单源出口）。"""
    return c_defines([
        ("BERRY_PLATFORM_LEVEL", str(meta_platform_level(d)),
         "平台 API level（单调整数轨道：只增不减；不兼容 ABI 变更 +1；兼容新增不 +1）"),
    ])


def doc_errno_c(d: dict) -> str:
    entries = []
    for e in d["errno"]:
        cn = e.get("c") or (f"BERRY_ERR_{e['name']}" if e["name"] != "OK" else "BERRY_OK")
        v = e["expr"] if e["expr"] == "0" else f"({e['expr']})"
        entries.append((cn, v, e.get("c_note", "")))
    return c_defines(entries)


DOC_BLOCKS = {
    "syscalls": doc_syscall_table,
    "platform-c": doc_platform_c,
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


# ---------------------------------------------------------------- syz 生成

# syzkaller 描述骨架（SEC-01 §4.5 件 25 后半）：常量层单源的 syscall fuzz 面投影。
# abi.toml **无逐 syscall 参数表**（§8.3 原型层 v2 挂账）——不臆造参数：各入口给
# 6 个全宽 int64 占位参（a1..a6 ↔ ARM64 SVC x1..x6，x0 = 号）。常量以字面量注释
# 随附——syz const 引用需 syz-extract 的 const 数据库，骨架阶段字面量最稳。
# 本机无 syz-sysgen：产物**未过编译验证**，头注如实声明（骨架，非可用描述）。
SYZ_HEADER = [
    "# BerryOS syscall 描述骨架（syzkaller）——**生成文件，勿手改**。",
    "# 单源 = `tools/abi.toml`；再生 = `python3 tools/gen_abi.py`（`just abi-gen`）；",
    "# 守卫 = `just abi-check`（`tools/gen_abi.py --check` 逐字节比对，挂 build/smoke 前置）。",
    "#",
    "# **骨架——未过 syz-sysgen 编译验证**；形态对表 syzkaller",
    "# docs/syscall_descriptions.md 保守子集（type/resource 定义、syscall 行、注释）。",
    "# abi.toml 只覆盖常量层（号/位/码/宽度），无逐 syscall 参数表——本骨架不臆造",
    "# 参数：各入口按 ARM64 SVC 惯例（x0 = 号、x1..x6 = 参，a1..a6 一一对应）给",
    "# 6 个全宽 int64 占位参；参数面待 §8.3 原型层（v2 挂账）补，届时 handle 参 →",
    "# berry_handle、数值参按宽度收窄、指针/缓冲参 → ptr[in/out] 保守映射。",
    "# 常量以字面量注释随附——不用 syz const 引用（需 syz-extract 的 const 数据库）。",
]

# §4 编号表全量在册都出入口行（v2/v3 + 在册未实现 v1 号内核回落 -ENOTSUP——
# 注释如实标 stage；fuzz 侧选录是接入真实 syzkaller 时的下游裁剪，不是单源的事）。
SYZ_ARGS = ", ".join(f"a{i} int64" for i in range(1, 7))


def syz_int(width: int, signed: bool) -> str:
    """[types] 宽度 → syz 整型名；非 1/2/4/8 宽 FAIL（单源执法——不静默落 int64）。"""
    if width not in (1, 2, 4, 8):
        raise Fail(f"abi.toml: [types] 宽度 {width} 无 syz 整型映射（1/2/4/8）")
    base = {1: "int8", 2: "int16", 4: "int32", 8: "int64"}[width]
    return base if signed else "u" + base


def syz_val(expr: str) -> str:
    """常量注释值列：`expr (求值)`——字面拼写与求值同形（如 0 / -9）不重复。"""
    v = eval_expr(expr)
    shown = hex(v) if v >= 0 else str(v)
    norm = expr.replace(" ", "").replace("_", "")
    if norm in (shown, str(v)):
        return expr
    return f"{expr}  ({shown})"


def syz_stage_mark(s: dict) -> str:
    if s["stage"] != "v1":
        if not s.get("implemented", True):
            return f"（{s['stage']}·未实现）"
        return f"（{s['stage']}·已实现）"
    if not s.get("implemented", True):
        return "（v1·在册未实现）"
    return "（v1）"


def gen_syz(d: dict) -> str:
    t = d["types"]
    meta = d["meta"]
    L = list(SYZ_HEADER)
    L.append("")
    L.append("# ---- 资源/类型定义（abi.toml [types] 宽度冻结面；handle 参未来映射到 berry_handle）----")
    L.append(f"# 无效 handle：BERRY_HANDLE_INVALID = {t['invalid_handle']}（表槽 0 恒空）")
    L.append(f"resource berry_handle[{syz_int(t['handle'], False)}]")
    L.append(f"type berry_rights {syz_int(t['rights'], False)}")
    L.append(f"type berry_status {syz_int(t['status'], True)}")
    L.append(f"type berry_time {syz_int(t['time'], False)}")
    L.append("")
    L.append("# ---- 常量注释（abi.toml 单源字面量；非 syz const 引用）----")
    L.append("# 平台 API level（[platform]；单调整数轨道——只增不减、不兼容 ABI 变更 +1）:")
    L.append(f"#   BERRY_PLATFORM_LEVEL = {meta_platform_level(d)}")
    L.append("# clock id:")
    for c in d["clock"]:
        L.append(f"#   CLOCK_{c['name']} = {syz_val(c['expr'])}")
    L.append("# options 位（未知位一律 -EINVAL；scope = 归属 syscall——同值"
             "异 scope 不冲突，DOC-6 修：旧裸值两条 1 << 0 无注记）:")
    for o in d["option"]:
        L.append(f"#   {o['name']} = {syz_val(o['expr'])}（scope: {o.get('scope', '全局')}）")
    L.append("# rights 位（berry_rights；权限在 handle 不在对象）:")
    for r in d["right"]:
        cn = r.get("c") or f"BERRY_RIGHT_{r['name']}"
        L.append(f"#   {cn} = {syz_val(r['expr'])}")
    L.append("# wait 信号位（低 32 内核派生 + 高 32 用户）:")
    for sg in d["signal"]:
        L.append(f"#   SIGNAL_{sg['name']} = {syz_val(sg['expr'])}")
    L.append("# 错误码（berry_status_t = i32；posix-ish 负数）:")
    for e in d["errno"]:
        cn = e.get("c") or (f"BERRY_ERR_{e['name']}" if e["name"] != "OK" else "BERRY_OK")
        L.append(f"#   {cn} = {syz_val(e['expr'])}")
    L.append("")
    L.append(f"# ---- syscall 入口（§4 编号表全量在册；v1 {meta['nr_total']} 号、"
             f"已实现 {meta['implemented']}；")
    L.append("#      a1..a6 ↔ ARM64 SVC x1..x6 全宽占位参——参数面待 §8.3 原型层（v2 挂账）补）----")
    for g in d["group"]:
        L.append("")
        L.append(f"# {g['title']}")
        for s in g["syscall"]:
            _d = s['desc'].split(chr(10))[0]
            L.append(f"# 0x{s['num']:02X} {s['name']}{syz_stage_mark(s)}——{_d}")
            for cont in s["desc"].split("\n")[1:]:
                L.append(f"#   {cont}")
            if s.get("note"):
                nl = s["note"].split("\n")
                L.append(f"#   note: {nl[0]}")
                for cont in nl[1:]:
                    L.append(f"#   {cont}")
            L.append(f"{s['name']}({SYZ_ARGS})")
    L.append("")
    return "\n".join(L)


# ---------------------------------------------------------------- selftest

def selftest() -> int:
    """门禁自检（挂 just test-host 前置；mkfat/genfbfont --selftest 同位纪律）。

    --check 只拦「盘上产物 vs 再生期望」漂移——生成器逻辑本身改坏（如
    eval_expr 吞了负号、c_value 透传裸 1<<40、four_tuple 口径换了）且两侧
    同步再生成时照样绿。selftest 用合成向量钉住纯函数语义 + 在册单源跑
    全链（validate → 五产物）对真实盘上文件做逐字节 oracle——逻辑改坏
    即咬，无需再生成。
    """
    failures = []
    passed = 0

    def check(name, cond, detail=""):
        nonlocal passed
        if cond:
            passed += 1
        else:
            failures.append((name, detail))

    def expect_fail(name, fn, want_sub):
        try:
            fn()
        except Fail as e:
            check(name, want_sub in str(e), f"msg={str(e)!r} 不含 {want_sub!r}")
        else:
            check(name, False, "未抛 Fail")

    # --- 1. 求值器 eval_expr：四语法族 + 拒面 ---
    check("eval u64::MAX", eval_expr("u64::MAX") == (1 << 64) - 1)
    check("eval 1 << N（带空格归一）", eval_expr("1 << 40") == 1 << 40)
    check("eval 0x/十进制/_ 分隔", eval_expr("0xFFFF_FFFF_0000_0000") == 0xFFFF_FFFF_0000_0000
          and eval_expr("-110") == -110 and eval_expr(" -110 ") == -110)
    expect_fail("eval 拒不可求值", lambda: eval_expr("1 <<< 4"), "不可求值")
    expect_fail("eval 拒表达式注入", lambda: eval_expr("__import__('os')"), "不可求值")

    # --- 2. C 宏映射 c_value：u64::MAX/-1LL、1<<N/1ULL、裸整直通、
    #     非规范拼写拒（裸进 C 是 int 移位 UB——对抗审查修的口径） ---
    check("c_value u64::MAX → (-1LL)", c_value("u64::MAX") == "(-1LL)")
    check("c_value 1 << 40 → (1ULL << 40)", c_value("1 << 40") == "(1ULL << 40)")
    check("c_value 裸整直通", c_value("12") == "12" and c_value("0x10") == "0x10")
    expect_fail("c_value 拒负数（rights 位全正值域）", lambda: c_value("-9"), "C 宏映射")
    check("c_value 归一后收 1<<40（非规范拼写不透传——产 ULL 形）",
          c_value("1<<40") == "(1ULL << 40)")
    check("c_value 归一后收 u64 :: MAX", c_value("u64 :: MAX") == "(-1LL)")
    expect_fail("c_value 拒 2 << 3（只收 1<<N）", lambda: c_value("2 << 3"), "C 宏映射")

    # --- 3. 唤醒源号段 next_free_wakesrc：0x01–0x7E 最小未占；
    #     NONE/UNKNOWN 锚不入空闲集（占用以在册为准） ---
    w_full = {f"W{i}": v for i, v in enumerate(range(0x00, 0x7F))}
    check("next_free 号段占满 → None", next_free_wakesrc(
        {**w_full, "UNKNOWN": 0x7F}) is None)
    w_gap = {"NONE": 0x00, "KERNEL_BOOT": 0x01, "TIMER": 0x02, "UNKNOWN": 0x7F}
    check("next_free 跳锚取最小未占 0x03", next_free_wakesrc(w_gap) == 0x03)
    w_mid = {"NONE": 0x00, "A": 0x01, "C": 0x03, "UNKNOWN": 0x7F}
    check("next_free 中段空洞 0x02", next_free_wakesrc(w_mid) == 0x02)

    # --- 4. 装载校验 validate：合成最小 toml 逐拒面咬 ---
    base_toml = """
[meta]
stage = "v1"
nr_total = 1
implemented = 1
nr_ceiling = 45
[platform]
level = 1
[types]
handle = 4
rights = 8
status = 4
time = 8
invalid_handle = 0
[[wakesrc]]
name = "NONE"
expr = "0x00"
[[wakesrc]]
name = "UNKNOWN"
expr = "0x7F"
[[clock]]
name = "MONOTONIC"
expr = "0"
[[option]]
name = "OPT_A"
scope = "channel_create"
expr = "1 << 0"
[[right]]
name = "READ"
expr = "1 << 0"
[[signal]]
name = "READABLE"
expr = "1 << 0"
[[errno]]
name = "OK"
expr = "0"
[[group]]
title = "**t**"
[[group.syscall]]
num = 0x01
name = "handle_close"
desc = "d"
stage = "v1"
"""
    import tomllib as _tl
    base = _tl.loads(base_toml)

    d = validate(_tl.loads(base_toml))
    check("validate 最小合成单源过", d["meta"]["nr_total"] == 1)

    def mut(**kw):
        obj = _tl.loads(base_toml)
        for k, v in kw.items():
            obj[k] = v
        return obj

    expect_fail("validate 拒缺 [meta]", lambda: validate(
        {k: v for k, v in base.items() if k != "meta"}), "缺 [meta]")
    # [platform] level（VER-01 提案第 4 条）：缺段/非正整数/bool 全拒
    expect_fail("validate 拒缺 [platform]", lambda: validate(
        {k: v for k, v in base.items() if k != "platform"}), "缺 [platform]")
    expect_fail("validate 拒 level=0", lambda: validate(
        mut(platform={"level": 0})), "≥1 的整数")
    expect_fail("validate 拒 level=bool（int 子类显式拒）", lambda: validate(
        mut(platform={"level": True})), "≥1 的整数")
    expect_fail("validate 拒空 [[clock]]", lambda: validate(
        mut(clock=[])), "段为空")
    expect_fail("validate 拒 num 重复", lambda: validate(mut(group=[{
        "title": "t", "syscall": [
            {"num": 0x01, "name": "a", "desc": "d", "stage": "v1"},
            {"num": 0x01, "name": "b", "desc": "d", "stage": "v1"}]}] + base["group"][1:])),
        "num 重复")
    expect_fail("validate 拒 nr_total 账错", lambda: validate(
        mut(meta={**base["meta"], "nr_total": 2})), "nr_total=2")
    expect_fail("validate 拒 implemented 账错", lambda: validate(
        mut(meta={**base["meta"], "implemented": 0})), "implemented=0")
    expect_fail("validate 拒超号面帽", lambda: validate(
        mut(meta={**base["meta"], "nr_ceiling": 0})), "nr_ceiling=0")
    expect_fail("validate 拒 right 值重复", lambda: validate(mut(right=[
        {"name": "READ", "expr": "1 << 0"}, {"name": "WRITE", "expr": "1 << 0"}])),
        "right]] expr 值重复")
    # option 同 scope 同值咬、异 scope 同值放行（per-syscall 位图口径）
    expect_fail("validate 拒同 scope option 同值", lambda: validate(mut(option=[
        {"name": "OPT_A", "scope": "channel_create", "expr": "1 << 0"},
        {"name": "OPT_B", "scope": "channel_create", "expr": "1 << 0"}])),
        "同 scope 下 expr 值重复")
    check("validate 放异 scope option 同值", (lambda: (
        validate(mut(option=[
            {"name": "OPT_A", "scope": "channel_create", "expr": "1 << 0"},
            {"name": "OPT_B", "scope": "vmo_create", "expr": "1 << 0"}])), True)[1])())
    expect_fail("validate 拒 wakesrc 出值域", lambda: validate(mut(wakesrc=[
        {"name": "NONE", "expr": "0x00"}, {"name": "X", "expr": "0x80"},
        {"name": "UNKNOWN", "expr": "0x7F"}])), "出值域")
    expect_fail("validate 拒 UNKNOWN 锚变值", lambda: validate(mut(wakesrc=[
        {"name": "NONE", "expr": "0x00"}, {"name": "UNKNOWN", "expr": "0x7E"}])),
        "语义锚 UNKNOWN")
    expect_fail("validate 拒 name 重复", lambda: validate(mut(right=[
        {"name": "READ", "expr": "1 << 0"}, {"name": "READ", "expr": "1 << 1"}])),
        "name 重复")

    # --- 5. 行筛选三分口径：v1_rows/const_rows/all_rows（v1+1.1 常量层 /
    #     全量表 / v1 在册） ---
    synth = {"group": [{"title": "g", "syscall": [
        {"num": 0x01, "name": "a", "desc": "d", "stage": "v1"},
        {"num": 0x02, "name": "b", "desc": "d", "stage": "1.1"},
        {"num": 0x03, "name": "c", "desc": "d", "stage": "v2"},
    ]}]}
    check("v1_rows 只收 v1", [s["name"] for s in v1_rows(synth)] == ["a"])
    check("const_rows 收 v1+1.1", [s["name"] for s in const_rows(synth)] == ["a", "b"])
    check("all_rows 收全量", [s["name"] for s in all_rows(synth)] == ["a", "b", "c"])

    # --- 6. 四元组 four_tuple：在册/常量层/已挂派发/帽 四口径（PM-17 §4） ---
    ft = {"meta": {"nr_total": 1, "nr_ceiling": 45}, "group": [{"syscall": [
        {"num": 0x01, "name": "a", "stage": "v1"},
        {"num": 0x02, "name": "b", "stage": "1.1"},
        {"num": 0x03, "name": "c", "stage": "1.1", "implemented": False},
        {"num": 0x04, "name": "d", "stage": "v1", "implemented": False},
    ]}]}
    check("four_tuple 四口径", four_tuple(ft) == (1, 4, 2, 45),
          f"got {four_tuple(ft)}")

    # --- 7. syz 投影：syz_int 宽度映射 + 拒面、syz_val 同形不重复、
    #     stage 标记四态 ---
    check("syz_int 映射", syz_int(4, True) == "int32" and syz_int(8, False) == "uint64")
    expect_fail("syz_int 拒 3 字节宽", lambda: syz_int(3, True), "无 syz 整型映射")
    check("syz_val 同形直通", syz_val("0") == "0" and syz_val("-9") == "-9")
    check("syz_val 异形附求值", syz_val("1 << 40") == "1 << 40  (0x10000000000)")
    check("syz_val hex 异形附 hex", syz_val("0xFFFF_FFFF_0000_0000") ==
          "0xFFFF_FFFF_0000_0000  (0xffffffff00000000)")
    check("syz_stage_mark 四态", [
        syz_stage_mark({"stage": "v1"}),
        syz_stage_mark({"stage": "v1", "implemented": False}),
        syz_stage_mark({"stage": "1.1"}),
        syz_stage_mark({"stage": "1.1", "implemented": False}),
    ] == ["（v1）", "（v1·在册未实现）", "（1.1·已实现）", "（1.1·未实现）"])

    # --- 8. marker 分流 + patch_doc 唯一性执法：C fence 内 C 注释、
    #     markdown HTML 注释；重复对/交叉序/缺计数锚拒 ---
    check("markers markdown 语境", markers("syscalls") == (
        "<!-- @gen:syscalls begin -->", "<!-- @gen:syscalls end -->"))
    check("markers -c fence 语境", markers("rights-c") == (
        "/* @gen:rights-c begin */", "/* @gen:rights-c end */"))
    doc_ok = ("# T\n\n<!-- @gen:syscalls begin -->\nOLD\n<!-- @gen:syscalls end -->\n\n"
              "```c\n/* @gen:platform-c begin */\nP\n/* @gen:platform-c end */\n```\n\n"
              "```c\n/* @gen:rights-c begin */\nX\n/* @gen:rights-c end */\n```\n\n"
              "```c\n/* @gen:errno-c begin */\nY\n/* @gen:errno-c end */\n```\n\n"
              "v1 计数 = 1\n")
    patched = patch_doc(doc_ok, base)
    check("patch_doc 四块全替换", "OLD" not in patched and "\nP\n" not in patched
          and "\nX\n" not in patched and "\nY\n" not in patched and "v1 计数 = 1" in patched)
    check("patch_doc 表头契约在席", DOC_TABLE_HEADER[0] in patched
          and DOC_TABLE_HEADER[1] in patched)
    check("patch_doc platform C 行在席", "#define BERRY_PLATFORM_LEVEL" in patched)
    expect_fail("patch_doc 拒重复 marker 对", lambda: patch_doc(
        doc_ok.replace("<!-- @gen:syscalls begin -->",
                       "<!-- @gen:syscalls begin -->\n<!-- @gen:syscalls begin -->"),
        base), "恰一对")
    expect_fail("patch_doc 拒计数锚漂移", lambda: patch_doc(
        doc_ok.replace("v1 计数 = 1", "v1 计数 = 2"), base), "nr_total")
    expect_fail("patch_doc 拒缺计数锚", lambda: patch_doc(
        doc_ok.replace("v1 计数 = 1", "无关行"), base), "计数断言锚点")

    # --- 9. C 定义列 c_defines：对齐数学（名列宽 = 最长+2）+ 尾随空白纪律
    #      （无注释行不补尾空白） ---
    aligned = c_defines([("A", "1", "note"), ("LONGER_NAME", "22", "")])
    lines = aligned.split("\n")
    check("c_defines 名列对齐（A 补到 13 宽 = 12 空格）",
          lines[0].startswith("#define A" + " " * 12 + "1")
          and lines[1] == "#define LONGER_NAME  22")

    # --- 9.5 G-ver-1（版本管理标准 §3）：level 单调裁决钉板（降号红负向钉） ---
    check("G-ver-1 升号过（1→2）", level_verdict(2, 1) is None)
    check("G-ver-1 平号过（1→1）", level_verdict(1, 1) is None)
    check("G-ver-1 豁免不红（old=None）", level_verdict(1, None) is None)
    down = level_verdict(1, 2)
    check("G-ver-1 降号红（2→1；消息含两值与标准 §3 引用）",
          down is not None and "现值 1" in down and "HEAD 值 2" in down
          and "§3" in down)
    head_lvl, head_note = head_platform_level()
    check("G-ver-1 HEAD 基线口径（豁免注记 或 ≥1 整值同形注记）",
          (head_lvl is None and "豁免" in head_note)
          or (isinstance(head_lvl, int) and not isinstance(head_lvl, bool)
              and head_lvl >= 1 and head_note == f"HEAD level={head_lvl}"))

    # --- 10. exit-code 契约（子进程真跑；不测 --selftest 自身——自引用
    #      会无限递归；extra-arg 守卫 = exact-arg 契约） ---
    import subprocess as sp
    p1 = sp.run([sys.executable, __file__, "--check"], capture_output=True, text=True)
    check("--check rc=0 且 PASS 行", p1.returncode == 0 and "abi-check: PASS" in p1.stdout)
    p2 = sp.run([sys.executable, __file__, "--bogus"], capture_output=True, text=True)
    check("--bogus rc=2 usage（argparse 契约）", p2.returncode == 2 and "usage:" in p2.stderr)
    p3 = sp.run([sys.executable, __file__, "--selftest", "extra"],
                capture_output=True, text=True)
    check("--selftest exact-arg guard（extra arg = usage 拒，不静默跑）",
          p3.returncode != 0 and "usage:" in p3.stderr)

    # --- 11. 在册单源全链 oracle：validate → 五产物 vs 盘上逐字节 ---
    real = load()
    k, u, doc, syz, arch = build_all(real)
    targets = [(KERNEL_RS, k), (USER_RS, u), (DOC, doc), (SYZ, syz), (ARCH_RS, arch)]
    bad = [str(p.relative_to(ROOT)) for p, want in targets
           if not p.exists() or p.read_text("utf-8") != want]
    check("在册五产物逐字节 oracle", not bad, f"漂移: {bad}")
    nt, nd, nr, nc = four_tuple(real)
    check("在册四元组口径", (nt, nd, nr, nc) == (37, 41, 40, 45),
          f"got {(nt, nd, nr, nc)}")
    check("在册 wakesrc 空闲段活算", next_free_wakesrc(
        {w["name"]: eval_expr(w["expr"]) for w in real["wakesrc"]}) == 0x08)

    # 确定性：全链再跑一遍逐字节同值（无隐状态/时间漂移）。
    k2, u2, doc2, syz2, arch2 = build_all(load())
    check("全链确定性（两跑逐字节同值）",
          (k, u, doc, syz, arch) == (k2, u2, doc2, syz2, arch2))

    print(f"\ngen_abi selftest: {'PASS' if not failures else 'FAIL'}"
          f"（{passed} passed, {len(failures)} failed）")
    for name, detail in failures:
        print(f"  FAIL: {name}" + (f"——{detail}" if detail else ""))
    return 1 if failures else 0


# ---------------------------------------------------- G-ver-1（版本管理标准 §3）
# level 单调执法（挂 --check = just abi-check，build/smoke 前置顺带）：
# 现值 < git HEAD 值即 FAIL。豁免（注记不红，与 build.rs 的 nogit 标识同
# 哲学——缺证据不冒充通过）：无 git / HEAD abi.toml 无 [platform] level
# （level 轨道首入轨前的历史提交）。基线取 HEAD 而非「工作树某旧值」：
# abi-check 的执法时点 = 提交前，HEAD 即「上一个已入库的对外承诺」。

def head_platform_level() -> tuple:
    """git HEAD `tools/abi.toml` 的 [platform] level（G-ver-1 比较基线）。

    返回 (level, note)：level=None = 豁免、note 为豁免原因；否则
    (≥1 整数, "HEAD level=<n>")。HEAD 文件缺段/不可解析一律豁免——
    本门只裁「能确证旧值的提交」上的回退，不在证据缺席时冒充执法。"""
    import subprocess as sp
    try:
        p = sp.run(["git", "-C", str(ROOT), "show", "HEAD:tools/abi.toml"],
                   capture_output=True, text=True, encoding="utf-8",
                   errors="replace")
    except OSError:
        return None, "豁免（无 git 可用）"
    if p.returncode != 0:
        return None, "豁免（非 git 仓或无 HEAD 提交）"
    try:
        old = tomllib.loads(p.stdout)
    except tomllib.TOMLDecodeError:
        return None, "豁免（HEAD abi.toml 不可解析）"
    lvl = old.get("platform", {}).get("level")
    if not isinstance(lvl, int) or isinstance(lvl, bool):
        return None, "豁免（HEAD abi.toml 无 [platform] level）"
    return lvl, f"HEAD level={lvl}"


def level_verdict(cur: int, old) -> str | None:
    """G-ver-1 单调裁决（--selftest 钉板位）：old=None 豁免 → None；
    cur < old → FAIL 消息（含两值与标准 §3 引用）；cur ≥ old → None。"""
    if old is None:
        return None
    if cur < old:
        return (f"G-ver-1（版本管理标准 §3）level 单调性破坏：现值 {cur} <"
                f" HEAD 值 {old}——level 只增不减（回退请 revert tools/abi.toml"
                " 或走不兼容变更评审 +1 纪律）")
    return None


# ---------------------------------------------------------------- 主流程

def build_all(d: dict) -> tuple:
    try:
        doc_text = DOC.read_text("utf-8")
    except OSError as e:
        raise Fail(f"读 ABI 文档失败：{DOC}（{e}）")
    return (gen_kernel_rs(d), gen_user_rs(d), patch_doc(doc_text, d), gen_syz(d),
            gen_arch_rs(d))


def main() -> int:
    # --selftest 前置守卫（不经 argparse）：exact-arg 契约——extra arg 落
    # argparse usage 拒（rc=2），不静默跑半套。
    if len(sys.argv) == 2 and sys.argv[1] == "--selftest":
        return selftest()
    ap = argparse.ArgumentParser(description="BerryOS ABI 单源生成/守卫")
    ap.add_argument("--check", action="store_true",
                    help="不写盘：逐字节比对盘上产物 vs 期望（漂移即 FAIL）"
                         " + G-ver-1 level 单调（现值 < git HEAD 即 FAIL）")
    ap.add_argument("--selftest", action="store_true",
                    help="门禁自检：纯函数钉板 + 在册五产物逐字节 oracle（exit 0=PASS）")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    try:
        d = load()
        k, u, doc, syz, arch = build_all(d)
        if args.check:
            targets = [(KERNEL_RS, k), (USER_RS, u), (DOC, doc), (SYZ, syz), (ARCH_RS, arch)]
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
            # G-ver-1（版本管理标准 §3）：[platform] level 单调——现值 <
            # git HEAD 值即 FAIL；无 git / HEAD 无该段 = 豁免注记不红。
            cur_lvl = d["platform"]["level"]
            old_lvl, lvl_note = head_platform_level()
            verdict = level_verdict(cur_lvl, old_lvl)
            if verdict is not None:
                print(f"abi-check: FAIL——{verdict}")
                return 1
            n = len(v1_rows(d))
            print(f"abi-check: G-ver-1 level 单调 {lvl_note}，现 level={cur_lvl}")
            print(f"abi-check: PASS（NR v1 {n} 号在册——内核 ↔ berry-sys ↔ 文档 ↔ syz ↔"
                  " arch-wake 五面单源一致）")
            return 0
        KERNEL_RS.write_text(k, "utf-8")
        USER_RS.write_text(u, "utf-8")
        DOC.write_text(doc, "utf-8")
        SYZ.write_text(syz, "utf-8")
        ARCH_RS.write_text(arch, "utf-8")
        print(f"abi-gen: 写 {KERNEL_RS.relative_to(ROOT)} / {USER_RS.relative_to(ROOT)}"
              f" / 文档四 marker 块 / {SYZ.relative_to(ROOT)} /"
              f" {ARCH_RS.relative_to(ROOT)}——提交时产物 diff 一并入册")
        return 0
    except Fail as e:
        print(f"abi-gen: FAIL——{e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
