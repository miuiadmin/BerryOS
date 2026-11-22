#!/usr/bin/env python3
"""BerryOS Wycheproof ed25519 验签向量表生成器（fixture JSON → Rust 常量表）。

产物：
  user/berry-crypto/src/wycheproof_vectors.rs —— WYCHEPROOF_ED25519[210]
  （每条 name/pk/msg/sig/result/flags/expect_ok；**no_std 消费零解析**——
  JSON 解析留构建期，运行时只做 hex 解码，berry-crypto 零依赖纪律不破）。

单源：tools/wycheproof_ed25519_test.json —— google/wycheproof 0.9rc5
  `testvectors_v1/ed25519_test.json` 本轮直抓落仓（sha256 ea80ffeb…b74b3）。
  实测注：任务书所指 `testvectors/` 旧目录对该文件 404（ed25519 向量只在
  testvectors_v1/，2026-11-11 本轮验证）——非推断。

版权面：wycheproof = Apache-2.0。fixture 随仓分发，出处（URL）/版本
  （0.9rc5）/指纹（sha256）三注进生成产物头注——vectors.rs P0-1 头注
  「Apache-2.0 LICENSE 注随 fixture 落仓」的兑现位。

期望映射（26 号 §3-2 strict 终裁——run_wycheproof_gate 执法语义）：
  valid      → expect_ok = true
  invalid    → expect_ok = false（含 tc210 MixedOrderPublicKey 验签方程
               不过例——混阶键也有真伪签之分，两类都咬）
  acceptable → expect_ok = false —— 小阶/混阶公钥 strict 拒收。RFC 8032
               §5.1.7 cofactored 语义下这些签「可接受」，但本仓选 strict
               子群策略（26 号 §3：mixed-order 门不过不合入 = fallback
               外购触发器）；全表 acceptable 必带 Small/MixedOrderPublicKey
               旗——他旗 acceptable 出现即 FAIL 重审，不静默放宽映射。

用法：
  python3 tools/genwycheproof.py            # 生成（写盘；产物入仓）
  python3 tools/genwycheproof.py --check    # 逐字节比对盘上产物 vs 期望
                                           # （漂移即 FAIL 绝不写盘——
                                           # genfbfont --check 同款纪律）
  python3 tools/genwycheproof.py --selftest # 门禁自检：validate 拒面/
                                           # 映射钉板/在册 fixture 统计
                                           # 锚/黄金锚/确定性/产物对偶

保真策略：产物 checked-in（盘上文件即真相，diff 可审、blame 可查）——
gen_abi/genfbfont 先例；不用 build.rs 运行时生成（berry-crypto 无
build.rs，rerun-if-changed 漏列 = 静默旧字节，G-eco-b 教训）。

环境要求：Python ≥3.9（仅 stdlib json/hashlib/argparse）。
"""

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURE = ROOT / "tools" / "wycheproof_ed25519_test.json"
PROD = ROOT / "user" / "berry-crypto" / "src" / "wycheproof_vectors.rs"

# 抓取出处（生成产物头注 + LICENSE 注单一出口）。
SOURCE_URL = ("https://raw.githubusercontent.com/google/wycheproof/master/"
              "testvectors_v1/ed25519_test.json")

# strict 映射（模块头注「期望映射」的机械面；acceptable 旗白名单 = 26 号
# §3-2 小阶/混阶两族——白名单外 acceptable 一律 Fail 重审）。
RESULT_MAP = {"valid": True, "invalid": False, "failure": False, "acceptable": False}
ACCEPTABLE_FLAG_WHITELIST = {"SmallOrderPublicKey", "MixedOrderPublicKey"}
HEX_RE = re.compile(r"[0-9a-f]*")  # fullmatch 用——小写hex/可空（大写拒）

# 黄金锚（selftest 钉板位——fixture 升级若漂移即红，逼有意识复核）。
GOLDEN = {
    "sha256": "ea80ffebac085e21cb9d3e903e7fa2b448d96702da5b31f061d0b83b430b74b3",
    "counts": {"valid": 117, "acceptable": 29, "invalid": 64},
    "max": {"msg": 1023, "sig": 96},
    "tc1_pk_head": "7d4d0e7f6153a69b6242b522abbee685",
    "tc1_sig_head": "d4fbdb52bfa726b44d1786a8c0d171c3",
    "tc1_sig_tail": "ebaa9d311c4007",
    "first_acceptable_tc": 152,  # SmallOrderPublicKey（pk 0100…0000 = 1-torsion）
    "empty_sig_tc": 30,          # TruncatedSignature「empty signature」
    "wide_sig_tc": 34,           # 96B（SignatureWithGarbage 尾垃圾）
    "max_msg_tc": 83,            # 1023B 消息
    "invalid_mixed_tc": 210,     # invalid + MixedOrderPublicKey（方程不过例）
}


class Fail(Exception):
    """关卡失败（validate/--check/自检不过）——消息直给 stderr。"""


# ---------------------------------------------------------------- 装载 + 校验

def load() -> dict:
    try:
        text = FIXTURE.read_text("utf-8")
    except OSError as e:
        raise Fail(f"读 fixture 失败：{FIXTURE}（{e}）")
    try:
        d = json.loads(text)
    except json.JSONDecodeError as e:
        raise Fail(f"fixture JSON 语法错：{e}")
    return validate(d, text)


def validate(d: dict, text: str) -> dict:
    """装载校验 + 归一化 → {rows, stats, source, sha256}（rows 保 JSON 原序）。

    两源核（数值纪律）：①numberOfTests（JSON 自报）== 枚举数；②flags ⊆
    notes 键（fixture 自洽）。统计（valid/acceptable/invalid 计数、msg/sig
    峰值）由数据活算——产物缓冲帽常量从这来，不硬抄。"""
    if d.get("algorithm") != "EDDSA":
        raise Fail(f"fixture: algorithm={d.get('algorithm')!r} != 'EDDSA'")
    notes = d.get("notes", {})
    rows = []
    source_versions = set()
    for g in d.get("testGroups", []):
        if g.get("type") != "EddsaVerify":
            raise Fail(f"fixture: testGroups type={g.get('type')!r} != 'EddsaVerify'")
        pk = g.get("publicKey", {}).get("pk", "")
        if g.get("publicKey", {}).get("curve") != "edwards25519":
            raise Fail(f"fixture: curve={g.get('publicKey', {}).get('curve')!r}"
                       " != 'edwards25519'")
        src = g.get("source", {})
        source_versions.add(f"{src.get('name', '?')} {src.get('version', '?')}")
        for t in g.get("tests", []):
            rows.append(_validate_one(t, pk, notes))
    if not rows:
        raise Fail("fixture: 零向量（testGroups/tests 空）")
    declared = d.get("numberOfTests")
    if declared != len(rows):
        raise Fail(f"fixture: numberOfTests={declared} != 枚举 {len(rows)}"
                   "（两源核破——fixture 截断/损坏）")
    tcids = [r["tc_id"] for r in rows]
    if len(set(tcids)) != len(tcids):
        raise Fail("fixture: tcId 重复")
    counts = {k: sum(1 for r in rows if r["result_class"] == k)
              for k in ("valid", "acceptable", "invalid")}
    if sum(counts.values()) != len(rows):
        raise Fail("fixture: result 三类分账 != 总数（映射表漏类）")
    # 混源如实记：ed25519 文件三源混编（google-wycheproof 0.9rc5 主源 +
    # cpu/eddsazerosigngen + sodium_compat boundary-regressions 组）——
    # 逐组记名入产物常量表，不折叠冒充单源。
    return {
        "rows": rows,
        "sources": sorted(source_versions),
        "sha256": hashlib.sha256(text.encode("utf-8")).hexdigest(),
        "stats": {
            "total": len(rows),
            **counts,
            "msg_max": max(len(r["msg"]) // 2 for r in rows),
            "sig_max": max(len(r["sig"]) // 2 for r in rows),
            # 边角锚 next(..., None)：合成 fixture 可无此类条目；真件由
            # selftest 黄金锚等值咬（None != int 即红）。
            "first_acceptable_tc": next((r["tc_id"] for r in rows
                                         if r["result_class"] == "acceptable"), None),
            "empty_sig_tc": next((r["tc_id"] for r in rows if r["sig"] == ""), None),
            "wide_sig_tc": next((r["tc_id"] for r in rows
                                 if len(r["sig"]) // 2 == max(len(x["sig"]) // 2
                                                               for x in rows)), None),
            "max_msg_tc": next((r["tc_id"] for r in rows
                                if len(r["msg"]) // 2 == max(len(x["msg"]) // 2
                                                             for x in rows)), None),
            "invalid_mixed_tc": next((r["tc_id"] for r in rows
                                      if r["result_class"] == "invalid"
                                      and "MixedOrderPublicKey" in r["flags"]), None),
        },
    }


def _validate_one(t: dict, pk: str, notes: dict) -> dict:
    """单条校验：tcId int / hex 小写偶长 / pk 恰 32B / result 映射 /
    flags ⊆ notes / acceptable 旗白名单（strict 重审执法位）。"""
    tc = t.get("tcId")
    if not isinstance(tc, int) or isinstance(tc, bool):
        raise Fail(f"fixture: tcId 非整数：{tc!r}")
    # pk 来自 testGroup.publicKey（组级）；msg/sig 来自 test（条级）。
    if HEX_RE.fullmatch(pk) is None or len(pk) != 64:
        raise Fail(f"tc{tc}: pk 非小写 hex 或长 ≠64（32B）：{pk!r}")
    for field in ("msg", "sig"):
        v = t.get(field)
        if not isinstance(v, str) or HEX_RE.fullmatch(v) is None:
            raise Fail(f"tc{tc}: {field} 非小写 hex（大写/非字符串拒）：{v!r}")
        if len(v) % 2 != 0:
            raise Fail(f"tc{tc}: {field} 奇数 hex 长度")
    result = t.get("result")
    if result not in RESULT_MAP:
        raise Fail(f"tc{tc}: result={result!r} 无 strict 映射"
                   f"（在册：{sorted(RESULT_MAP)}）")
    flags = t.get("flags")
    if not isinstance(flags, list) or not flags or any(not isinstance(f, str) for f in flags):
        raise Fail(f"tc{tc}: flags 非非空字符串数组：{flags!r}")
    unknown = [f for f in flags if f not in notes]
    if unknown:
        raise Fail(f"tc{tc}: flags {unknown} 不在 fixture notes 键集"
                   "（fixture 自洽破）")
    if result == "acceptable" and not set(flags) <= ACCEPTABLE_FLAG_WHITELIST:
        raise Fail(f"tc{tc}: acceptable 带白名单外旗 {flags}——strict 映射"
                   "（acceptable→拒）只对小阶/混阶公键终裁过，新旗型须"
                   "重审（26 号 §3-2），不静默放宽")
    return {
        "tc_id": tc,
        "comment": t.get("comment", ""),
        "pk": pk,
        "msg": t["msg"],
        "sig": t["sig"],
        "result": result,
        "flags": ",".join(flags),
        "expect_ok": RESULT_MAP[result],
        # failure 别名归 invalid 类记账（ed25519 文件在册用 invalid；别名
        # 收容是防御面——wycheproof 其他套件有 failure 拼法）。
        "result_class": "invalid" if result in ("invalid", "failure") else result,
    }


# ---------------------------------------------------------------- Rust 生成

GEN_HEADER = "//! Wycheproof ed25519 验签向量表"


def esc(s: str) -> str:
    """comment → Rust 字符串字面量转义（引号/反斜杠；在册全 ASCII 无需
    转义，防御面保留——fixture 升级可能带入）。"""
    return s.replace("\\", "\\\\").replace('"', '\\"')


def wrap_entry_comment(tc: int, flags: str, comment: str, result: str,
                       expect_ok: bool) -> list:
    """条目注释 → 多行 //（行长帽 ≤100：wycheproof 长注（如 tc42 位翻
    转说明 161 字符）按词折行续对齐——comment 全文保真不截断）。"""
    verdict = "过" if expect_ok else "拒"
    head, tail = f"    // tc{tc} [{flags}]", f"→ {result}（须{verdict}）"
    if not comment:
        return [f"{head} {tail}"]
    words = esc(comment).split(" ")
    lines = []
    # 行宽预算：总帽 100 − tail（→ result（须X）随 result 长短不一）− 收尾
    # 引号/空格 3——acceptable 类 tail 最长（17 字符）时预算 80。
    budget = 97 - len(tail) - 3
    cur = f'{head} "{words[0]}'
    for w in words[1:]:
        if len(cur) + 1 + len(w) > budget:
            lines.append(cur)
            cur = f"    //     {w}"
        else:
            cur += f" {w}"
    lines.append(f'{cur}" {tail}')
    return lines


def rust_hex(field: str, hexstr: str, indent: str = "        ") -> str:
    """hex 串 → Rust 字面量；>64 hex 续行（反斜杠续 + 对齐引号列——
    vectors.rs 手写表同款形态；行长 ≤ ~80 不触 100 警戒）。"""
    if len(hexstr) <= 64:
        return f'{indent}{field}: "{hexstr}",'
    chunks = [hexstr[i:i + 64] for i in range(0, len(hexstr), 64)]
    cont = " " * (len(indent) + len(field) + 3)  # `sig: "` 后对齐
    lines = [f'{indent}{field}: "{chunks[0]}\\']
    lines += [f"{cont}{c}\\" for c in chunks[1:-1]]
    lines.append(f'{cont}{chunks[-1]}",')
    return "\n".join(lines)


def gen_rs(m: dict) -> str:
    rows, s, srcs, sha = m["rows"], m["stats"], m["sources"], m["sha256"]
    n_accept_flags = {}
    for r in rows:
        if r["result_class"] == "acceptable":
            for f in r["flags"].split(","):
                n_accept_flags[f] = n_accept_flags.get(f, 0) + 1
    accept_note = " + ".join(f"{v} {k}" for k, v in sorted(n_accept_flags.items()))
    L = [
        f"{GEN_HEADER}——**生成文件，勿手改**。",
        "//! 单源 = `tools/wycheproof_ed25519_test.json`（**Apache-2.0**，",
        "//! LICENSE 注即此段：出处/版本/指纹三注齐——抓取自",
        f"//! {SOURCE_URL}；",
        f"//! fixture sha256 = `{sha}`",
        f"//! （两源核：JSON numberOfTests={s['total']} == 枚举数——生成器 load 执法）。",
        "//! 再生 = `python3 tools/genwycheproof.py`；守卫 = `--check` 逐字节",
        "//! 比对（gen_abi/genfbfont 产物入仓同款纪律）。",
        "//!",
        "//! 期望映射（26 号 §3-2 strict 终裁——`vectors::run_wycheproof_gate`",
        "//! 执法语义）：valid→true；invalid→false；**acceptable→false**——",
        "//! 小阶/混阶公钥 strict 拒收（RFC 8032 §5.1.7 cofactored 语义下可",
        "//! 接受，本仓选 strict 子群策略：mixed-order 门不过不合入 = 26 号",
        "//! §3-3 fallback 外购触发器）。生成器对白名单外 acceptable 旗 FAIL",
        "//! 重审，不静默放宽。",
        "//!",
        f"//! 统计：total {s['total']} = valid {s['valid']} + acceptable"
        f" {s['acceptable']} + invalid {s['invalid']}",
        f"//! （acceptable = {accept_note}）；expect_ok=true 恰 {s['valid']}。",
        f"//! msg 峰 {s['msg_max']}B（tc{s['max_msg_tc']}）/ sig 峰 {s['sig_max']}B"
        f"（tc{s['wide_sig_tc']}，含空 sig tc{s['empty_sig_tc']}）——",
        "//! 解码缓冲帽见 WY_MSG_MAX/WY_SIG_MAX（生成器从数据活算）。",
        "",
        "/// 单条 Wycheproof ed25519 验签向量（hex 存表零解析——运行时只",
        "/// hex 解码；`name` = \"tc\" + JSON tcId，失败记账/诊断索引）。",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
        "pub struct WycheproofVector {",
        "    /// 向量名（\"tc<N>\"——N = fixture tcId，全表唯一）。",
        "    pub name: &'static str,",
        "    /// 公钥（64 hex = 32B，全表恰满；小阶/混阶键原样入表——拒收",
        "    /// 语义由 expect_ok 执法，表不预筛）。",
        "    pub pk: &'static str,",
        "    /// 消息（hex，可空串 = 空消息；偶长 ≤ WY_MSG_MAX×2）。",
        "    pub msg: &'static str,",
        "    /// 签名（hex，**可 ≠64B**——截断/带垃圾/压缩编码例原样入表，",
        "    /// strict verify 须拒；偶长 ≤ WY_SIG_MAX×2）。",
        "    pub sig: &'static str,",
        "    /// fixture 原始判定（\"valid\"/\"acceptable\"/\"invalid\"——不折叠，",
        "    /// acceptable 与 invalid 的拒因不同层：前者策略拒、后者方程拒）。",
        "    pub result: &'static str,",
        "    /// 旗（逗号 join：SmallOrderPublicKey/MixedOrderPublicKey/",
        "    /// SignatureMalleability 等——fail 记账入报告，诊断面）。",
        "    pub flags: &'static str,",
        "    /// 门期望（strict 映射产物：valid=true，invalid/acceptable=false）。",
        "    pub expect_ok: bool,",
        "}",
        "",
        f"/// fixture 指纹（sha256——重抓漂移即变，`--check`+selftest 双咬）。",
        f'pub const WY_FIXTURE_SHA256: &str = "{sha}";',
        "/// 向量源（fixture testGroups[].source 逐组去重——三源混编如实记：",
        "/// google-wycheproof 主源 + 边界回归生成器组，见模块头注）。",
        "pub const WY_SOURCES: &[&str] = &[",
        *[f'    "{x}",' for x in srcs],
        "];",
        f"/// 向量总数（== fixture numberOfTests，两源核）。",
        f"pub const WY_TOTAL: usize = {s['total']};",
        f"/// valid 数（expect_ok=true 数——strict 门期望通过数）。",
        f"pub const WY_VALID: usize = {s['valid']};",
        f"/// acceptable 数（strict 策略拒——小阶/混阶公钥拒收执法面）。",
        f"pub const WY_ACCEPTABLE: usize = {s['acceptable']};",
        f"/// invalid 数（验签方程不过——必须拒）。",
        f"pub const WY_INVALID: usize = {s['invalid']};",
        f"/// 消息解码缓冲帽（全表最长 = tc{s['max_msg_tc']} {s['msg_max']}B；",
        "/// 与 RFC 批 MSG_BUF_MAX=1023 同值是数据巧合，非单源）。",
        f"pub const WY_MSG_MAX: usize = {s['msg_max']};",
        f"/// 签名解码缓冲帽（全表最长 = tc{s['wide_sig_tc']} {s['sig_max']}B",
        f"/// 带垃圾尾；最短 tc{s['empty_sig_tc']} = 0B 空 sig）。",
        f"pub const WY_SIG_MAX: usize = {s['sig_max']};",
        "",
        f"/// Wycheproof ed25519 全向量（tc1..=tc{s['total']}，fixture 原序。",
        "/// 消费 = `vectors::run_wycheproof_gate_forced`（verify 回调批跑）。",
        "pub const WYCHEPROOF_ED25519: &[WycheproofVector] = &[",
    ]
    for r in rows:
        L.extend(wrap_entry_comment(r["tc_id"], r["flags"], r["comment"],
                                    r["result"], r["expect_ok"]))
        L.append("    WycheproofVector {")
        L.append(f'        name: "tc{r["tc_id"]}",')
        L.append(rust_hex("pk", r["pk"]))
        L.append(rust_hex("msg", r["msg"]))
        L.append(rust_hex("sig", r["sig"]))
        L.append(f'        result: "{r["result"]}",')
        L.append(f'        flags: "{r["flags"]}",')
        L.append(f"        expect_ok: {'true' if r['expect_ok'] else 'false'},")
        L.append("    },")
    L.append("];")
    L.append("")
    return "\n".join(L)


# ---------------------------------------------------------------- 主流程

def build() -> str:
    return gen_rs(load())


def main() -> int:
    ap = argparse.ArgumentParser(description="BerryOS Wycheproof ed25519 向量表生成/守卫")
    ap.add_argument("--check", action="store_true",
                    help="不写盘：逐字节比对盘上产物 vs 期望（漂移即 FAIL）")
    ap.add_argument("--selftest", action="store_true",
                    help="门禁自检：validate 拒面/映射钉板/在册统计锚/黄金锚/确定性/产物对偶")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    try:
        text = build()
    except Fail as e:
        print(f"wycheproof-gen: FAIL——{e}", file=sys.stderr)
        return 1
    if args.check:
        got = PROD.read_text("utf-8") if PROD.exists() else None
        if got != text:
            print(f"wycheproof-check: FAIL——{PROD.relative_to(ROOT)} 与 fixture 重产不符"
                  "（重抓/换版 fixture 后须重跑 `python3 tools/genwycheproof.py`；"
                  "手改产物请改回）", file=sys.stderr)
            return 1
        n = text.count("\n    WycheproofVector {")
        print(f"wycheproof-check: PASS（{n} 向量 / "
              f"{len(json.loads(FIXTURE.read_text('utf-8'))['testGroups'])} 组"
              "——fixture ↔ 产物逐字节一致）")
        return 0
    try:
        PROD.parent.mkdir(parents=True, exist_ok=True)
        PROD.write_text(text, "utf-8")
    except OSError as e:
        print(f"wycheproof-gen: FAIL——写产物失败：{PROD}（{e}）", file=sys.stderr)
        return 1
    print(f"wycheproof-gen: 写 {PROD.relative_to(ROOT)}"
          f"（{text.count(chr(10) + '    WycheproofVector {')} 向量）")
    return 0


# ---------------------------------------------------------------- selftest

def selftest() -> int:
    """门禁自检（a-3 U4 纪律）：--check 只拦「盘上 vs 重产」漂移，生成器
    逻辑改坏且同步重产时照样绿——本处用合成 fixture 钉纯函数语义 + 在册
    fixture 统计/黄金锚 + 确定性 + 产物对偶，逻辑改坏即咬。"""
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

    # --- 1. validate 拒面（合成最小 fixture 逐面咬）---
    def synth(**over):
        base = {
            "algorithm": "EDDSA",
            "numberOfTests": 2,
            "notes": {"Valid": {}, "SmallOrderPublicKey": {}},
            "testGroups": [{
                "type": "EddsaVerify",
                "source": {"name": "google-wycheproof", "version": "0.9rc5"},
                "publicKey": {"curve": "edwards25519", "pk": "00" * 32},
                "tests": [
                    {"tcId": 1, "comment": "ok", "flags": ["Valid"],
                     "msg": "", "sig": "11" * 64, "result": "valid"},
                    {"tcId": 2, "comment": "small", "flags": ["SmallOrderPublicKey"],
                     "msg": "ab", "sig": "22" * 64, "result": "acceptable"},
                ],
            }],
        }
        for k, v in over.items():
            base[k] = v
        return base

    ok = validate(synth(), json.dumps(synth()))
    check("validate 最小合成过 + strict 映射",
          [r["expect_ok"] for r in ok["rows"]] == [True, False]
          and ok["stats"]["total"] == 2)
    expect_fail("validate 拒 algorithm 异", lambda: validate(
        synth(algorithm="ECDSA"), "{}"), "EDDSA")
    expect_fail("validate 拒计数漂移", lambda: validate(
        synth(numberOfTests=3), "{}"), "两源核破")
    g = synth()["testGroups"][0]
    expect_fail("validate 拒大写 hex", lambda: validate(
        synth(testGroups=[{**g, "tests": [{**g["tests"][0], "sig": "AB" * 64}]}]),
        "{}"), "非小写 hex")
    expect_fail("validate 拒奇长 hex", lambda: validate(
        synth(testGroups=[{**g, "tests": [{**g["tests"][0], "msg": "abc"}]}]),
        "{}"), "奇数")
    expect_fail("validate 拒组级 pk 大写", lambda: validate(
        synth(testGroups=[{**g, "publicKey": {"curve": "edwards25519", "pk": "0A" * 32}}]),
        "{}"), "非小写 hex 或长 ≠64")
    expect_fail("validate 拒 result 无映射", lambda: validate(
        synth(testGroups=[{**g, "tests": [{**g["tests"][0], "result": "bogus"}]}]),
        "{}"), "无 strict 映射")
    expect_fail("validate 拒白名单外 acceptable 旗", lambda: validate(
        synth(notes={**synth()["notes"], "SignatureMalleability": {}},
              testGroups=[{**g, "tests": [
                  {**g["tests"][1], "flags": ["SignatureMalleability"]}]}]),
        "{}"), "重审")
    expect_fail("validate 拒 notes 外旗", lambda: validate(
        synth(testGroups=[{**g, "tests": [
            {**g["tests"][0], "flags": ["NoSuchFlag"]}]}]), "{}"), "notes 键集")
    expect_fail("validate 拒 tcId 重复", lambda: validate(
        synth(testGroups=[{**g, "tests": [
            {**g["tests"][0]}, {**g["tests"][0]}]}], numberOfTests=2), "{}"), "tcId 重复")
    expect_fail("validate 拒 pk 长 ≠64 hex", lambda: validate(
        synth(testGroups=[{**g, "publicKey": {"curve": "edwards25519", "pk": "00" * 31}}]),
        "{}"), "长 ≠64")
    # failure 别名归 invalid 类（防御面——ed25519 在册用 invalid）
    al = validate(synth(numberOfTests=1, testGroups=[{**g, "tests": [
        {**g["tests"][0], "result": "failure"}]}]), "{}")
    check("failure 别名 → invalid 类拒", al["rows"][0]["result_class"] == "invalid"
          and al["rows"][0]["expect_ok"] is False)

    # --- 2. rust_hex 形态：短直通 / 长续行对齐（行长帽）---
    one = rust_hex("sig", "ab" * 32)
    check("rust_hex ≤64 hex 单行", one == '        sig: "' + "ab" * 32 + '",')
    long_hex = "cd" * 48  # 96 hex → 两段
    two = rust_hex("sig", long_hex)
    lines = two.split("\n")
    check("rust_hex >64 hex 首行反斜杠续行", lines[0].endswith("cd\\"))
    check("rust_hex 续行拼接无损",
          "".join(l.replace("\\", "").replace('",', "").split('"')[-1].strip()
                  for l in lines) == long_hex)
    check("rust_hex 行长 ≤100", all(len(l) <= 100 for l in lines))

    # --- 3. esc 转义：引号/反斜杠（在册无、防御面）---
    check("esc 引号+反斜杠", esc('a"b\\c') == 'a\\"b\\\\c')

    # --- 4. 在册 fixture：统计锚/黄金锚/确定性（两源核位）---
    m = load()
    s = m["stats"]
    check("fixture sha256 锚", m["sha256"] == GOLDEN["sha256"],
          f"got {m['sha256']}")
    check("fixture 计数锚 117/29/64",
          (s["total"], s["valid"], s["acceptable"], s["invalid"])
          == (210, GOLDEN["counts"]["valid"], GOLDEN["counts"]["acceptable"],
              GOLDEN["counts"]["invalid"]),
          f"got {s}")
    check("fixture 峰值锚 msg 1023 / sig 96",
          (s["msg_max"], s["sig_max"]) == (GOLDEN["max"]["msg"], GOLDEN["max"]["sig"]),
          f"got {s['msg_max']}/{s['sig_max']}")
    check("fixture 边角锚（首 acceptable/空 sig/宽 sig/峰 msg/invalid 混阶）",
          (s["first_acceptable_tc"], s["empty_sig_tc"], s["wide_sig_tc"],
           s["max_msg_tc"], s["invalid_mixed_tc"])
          == (GOLDEN["first_acceptable_tc"], GOLDEN["empty_sig_tc"],
              GOLDEN["wide_sig_tc"], GOLDEN["max_msg_tc"], GOLDEN["invalid_mixed_tc"]),
          f"got {s}")
    rows = {f'tc{r["tc_id"]}': r for r in m["rows"]}
    check("tc1 黄金锚（pk/sig 首尾——本轮直抓两源核）",
          rows["tc1"]["pk"].startswith(GOLDEN["tc1_pk_head"])
          and rows["tc1"]["sig"].startswith(GOLDEN["tc1_sig_head"])
          and rows["tc1"]["sig"].endswith(GOLDEN["tc1_sig_tail"]))
    check("tc1 期望过 / tc152 small-order 期望拒",
          rows["tc1"]["expect_ok"] and not rows["tc152"]["expect_ok"]
          and rows["tc152"]["flags"] == "SmallOrderPublicKey")
    check("tc30 空 sig / tc210 混阶 invalid 拒",
          rows["tc30"]["sig"] == "" and rows["tc210"]["result_class"] == "invalid"
          and rows["tc210"]["flags"] == "MixedOrderPublicKey")

    # --- 5. 生成确定性 + 产物契约行 ---
    text1, text2 = gen_rs(m), gen_rs(load())
    check("全链确定性（两跑逐字节同值）", text1 == text2)
    # 计数只数条目行（4 空格缩进）——struct 定义行无缩进不入数。
    n_vec = text1.count("\n    WycheproofVector {")
    check("产物向量数 == 统计总数", n_vec == s["total"], f"{n_vec} != {s['total']}")
    for frag in (f"pub const WY_TOTAL: usize = {s['total']};",
                 f"pub const WY_MSG_MAX: usize = {s['msg_max']};",
                 f"pub const WY_SIG_MAX: usize = {s['sig_max']};",
                 "pub const WYCHEPROOF_ED25519: &[WycheproofVector] = &[",
                 f'pub const WY_FIXTURE_SHA256: &str = "{m["sha256"]}";',
                 "Apache-2.0"):
        check(f"产物契约行在席：{frag[:44]}…", frag in text1)
    check("产物 tc152 注释含旗（comment 含 flags 纪律）",
          "// tc152 [SmallOrderPublicKey]" in text1)
    check("产物 tc30 空 sig 原样入表（截断拒面）",
          "// tc30 [TruncatedSignature]" in text1
          and text1.count('sig: "",') >= 1)

    # --- 6. 产物对偶（盘上文件 vs 重产——漂移即咬）+ exit-code 契约 ---
    import subprocess as sp
    got = PROD.read_text("utf-8") if PROD.exists() else None
    check("在册产物逐字节对偶", got == text1,
          "漂移或缺失（重产: python3 tools/genwycheproof.py）"
          if got != text1 else "")
    p1 = sp.run([sys.executable, __file__, "--bogus"], capture_output=True, text=True)
    check("--bogus rc=2 usage（argparse 契约）", p1.returncode == 2 and "usage:" in p1.stderr)
    p2 = sp.run([sys.executable, __file__, "--check"], capture_output=True, text=True)
    check("--check rc=0 且 PASS 行（产物已落仓前提）",
          p2.returncode == 0 and "wycheproof-check: PASS" in p2.stdout)

    print(f"\ngenwycheproof selftest: {'PASS' if not failures else 'FAIL'}"
          f"（{passed} passed, {len(failures)} failed）")
    for name, detail in failures:
        print(f"  FAIL: {name}" + (f"——{detail}" if detail else ""))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
