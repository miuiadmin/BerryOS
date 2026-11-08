#!/usr/bin/env python3
"""调度器统一仪表盘——QEMU 串口调度遥测一次收割 + 单屏摘要 + 基线回归检测。

把散在 ktest/main 各打印点的调度器遥测行收进一张表：PERF 三指标（验收门）、
公平性（FAIR-K）、策略表三态开销（PERF policy）、shadow divergence（A/B
判定）、唤醒延迟分位（HDRWAKE 轮询口径 / BENCHRWAKE 真睡口径——如果有）。

收割行（格式单一事实源 = kernel/src/ktest.rs、kernel/src/main.rs 打印点）：
  PERF pick avg=<N> n=<M>                                ktest test_sched_perf
  PERF freeze avg=<N> n=<M>                              同上（freeze+thaw 均摊）
  PERF billing avg=<N> n=<M>                             同上
  PERF policy base=<N> empty=<N> full=<N> delta=<N.N>%   PP-4 三态（delta=整数% 亦容）
  PERF policy divergence=<N> eval=<N>                    DV-K（policy_ab_eval 同源）
  FAIR-K spread=<N> hits=[a,b,c,d,]                      尾逗号宽容（内核逐段拼行）
  HDRWAKE ok p50=<N> p99=<N> n=<M>                       test_hdr_wake（轮询粒度）
  BENCHRWAKE p50=<N> p99=<N> n=<M>                       task_bench_rwake（真睡路径）
同一指标多次出现取最后一条（= 最近读数，policy_ab_eval/advisor 同口径）。

判定阈值（兄弟工具单源 import + 缺席兜底镜像，不另立口径）：
  - PERF 三指标：sched_perf_analyze.LIMITS（pick 5µs / freeze 5ms / billing 1µs）
    ——「验收 N/3」即此三项（sched_perf_analyze 验收同口径）；
  - divergence：policy_ab_eval.judge（<0.1 PROMOTE / >0.5 DEMOTE / 其间 HOLD）；
  - fairness spread ≤ 20、policy delta < 50%：内核断言口径（ktest.rs
    test_sched_fairness 偏差 ≤20 / PP-4 permille < 500）——行内 ok/OVER 标记，
    不计验收（数值门内核断言已在位，仪表盘只透出）。

用法：
  qemu ... | python3 tools/sched_dashboard.py
  qemu ... | python3 tools/sched_dashboard.py --save baseline.json      # 存基线
  qemu ... | python3 tools/sched_dashboard.py --baseline baseline.json # 回归检测
  python3 tools/sched_dashboard.py --demo
  python3 tools/sched_dashboard.py log.txt --json                      # 机器可读

--baseline 回归规则（更差即回归，留噪声容差）：
  - 周期/ticks 类（pick/freeze/billing、wake/rwake p50/p99）：cur > base×(1+tol)
    （tol 缺省 0.10，--tol 覆盖；base=0 时 cur>0 即回归）；
  - 百分点类（policy delta、divergence 率）与 spread：cur−base > 5.0；
  - 基线有而本次无 → 回归（缺失）；本次有而基线无 → 标「新增」不计回归。

exit 0 = 验收 3/3 且（如启用）无回归；1 = 验收有 FAIL / 有回归 / 无可用
遥测行 / 输入错误（对拍 sched_perf_analyze 口径）。
"""

import argparse
import json
import os
import re
import sys
import unicodedata

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
try:  # 阈值/解析单源复用（兄弟文件缺席时兜底镜像——值须同源同步）
    from sched_perf_analyze import (  # noqa: E402（同目录导入，见模块头注）
        FREQ_HZ, LIMITS, parse_perf_lines)
    from policy_ab_eval import (  # noqa: E402
        DEMOTE_THRESHOLD, PROMOTE_THRESHOLD, judge as _div_judge,
        parse_samples as _div_parse_samples)
except ImportError:  # pragma: no cover —— 兜底镜像
    FREQ_HZ = 62_500_000
    LIMITS = {"pick": 5_000, "freeze": 5_000_000, "billing": 1_000}
    PROMOTE_THRESHOLD, DEMOTE_THRESHOLD = 0.1, 0.5
    _FB = re.compile(r"PERF (\w+) avg=(\d+) n=(\d+)")

    def parse_perf_lines(text):
        return [(m.group(1), int(m.group(2)), int(m.group(3)))
                for m in _FB.finditer(text)]

    def _div_judge(div, evals, promote, demote):
        if evals <= 0:
            return {"recommendation": "HOLD"}
        rate = round(div / evals, 4)
        rec = ("PROMOTE" if rate < promote
               else "DEMOTE" if rate > demote else "HOLD")
        return {"divergence_rate": rate, "recommendation": rec}

    def _div_parse_samples(text):
        out = []
        for line in text.splitlines():
            if "PERF policy" not in line and "STATUS" not in line:
                continue
            d = re.search(r"\bdivergence(?:_count)?\s*=\s*(\d+)", line)
            e = re.search(r"(?:\bshadow_)?eval(?:uation)?(?:_count)?\s*=\s*(\d+)", line)
            if d and e:
                out.append((int(d.group(1)), int(e.group(1))))
        return out

# ---- 内核断言口径（ktest.rs）——行内标记，不计验收 ----
FAIR_SPREAD_LIMIT = 20          # test_sched_fairness：等权 4 任务 100 轮偏差 ≤20
POLICY_DELTA_LIMIT_PCT = 50.0   # PP-4：三态 avg 差 <50%（permille < 500）

# ---- 基线回归容差 ----
REL_TOL_DEFAULT = 0.10          # 周期/ticks 类相对容差（TCG 时基噪声缓冲）
PP_TOL = 5.0                    # 百分点/spread 类绝对容差

# 整数/小数字面量（delta 千分比拆印形 `42.8%` 与整数形 `8%` 双容）。
_PCT = r"(\d+(?:\.\d+)?)"
_POLICY_RE = re.compile(
    r"PERF policy base=(\d+) empty=(\d+) full=(\d+)(?: delta=" + _PCT + r"%)?")
_FAIR_RE = re.compile(r"FAIR-K spread=(\d+) hits=\[([\d,]*)\]")  # 尾逗号宽容
_HDRWAKE_RE = re.compile(r"HDRWAKE ok p50=(\d+) p99=(\d+) n=(\d+)")
_RWAKE_RE = re.compile(r"BENCHRWAKE p50=(\d+) p99=(\d+) n=(\d+)")

# 基线对比指标表：(标签, metrics 内取值路径, 容差类, 单位)。
#   rel = 相对容差（周期/ticks）/ pp = 绝对百分点容差（率与 spread）。
_SPECS = [
    ("pick_next", ("pick", "cycles"), "rel", "cyc"),
    ("freeze/thaw", ("freeze", "cycles"), "rel", "cyc"),
    ("billing", ("billing", "cycles"), "rel", "cyc"),
    ("fair spread", ("fairness", "spread"), "pp", ""),
    ("policy delta", ("policy", "delta_pct"), "pp", "%"),
    ("divergence", ("divergence", "rate_pct"), "pp", "%"),
    ("wake p50", ("wake", "p50"), "rel", "ticks"),
    ("wake p99", ("wake", "p99"), "rel", "ticks"),
    ("rwake p50", ("rwake", "p50"), "rel", "ticks"),
    ("rwake p99", ("rwake", "p99"), "rel", "ticks"),
]

MIN_INNER = 38                  # 盒内最小显示宽度（示例报告同量级）


# ---- 解析 ----

def _last_match(rx, text):
    """全量匹配取最后一条（= 最近读数）。"""
    last = None
    for m in rx.finditer(text):
        last = m
    return last


def parse_telemetry(text: str) -> dict:
    """串口文本 → 遥测字典（键缺席 = 该行未出现）。"""
    m = {}
    for op, cyc, n in parse_perf_lines(text):  # 后行覆盖前行 = 取最后
        m[op] = {"cycles": cyc, "n": n}
    pm = _last_match(_POLICY_RE, text)
    if pm:
        m["policy"] = {
            "base": int(pm.group(1)), "empty": int(pm.group(2)),
            "full": int(pm.group(3)),
            "delta_pct": float(pm.group(4)) if pm.group(4) is not None else None,
        }
    fm = _last_match(_FAIR_RE, text)
    if fm:  # hits=[25,25,25,25,] —— 内核逐段拼行带尾逗号，split 后滤空
        m["fairness"] = {
            "spread": int(fm.group(1)),
            "hits": [int(x) for x in fm.group(2).split(",") if x.strip()],
        }
    ds = _div_parse_samples(text)  # policy_ab_eval 同源（含 packed/别名宽容）
    if ds:
        d, e = ds[-1]
        m["divergence"] = {
            "div": d, "eval": e,
            "rate_pct": round(d / e * 100, 1) if e > 0 else None,
        }
    hm = _last_match(_HDRWAKE_RE, text)
    if hm:
        m["wake"] = {"p50": int(hm.group(1)), "p99": int(hm.group(2)),
                     "n": int(hm.group(3))}
    rm = _last_match(_RWAKE_RE, text)
    if rm:
        m["rwake"] = {"p50": int(rm.group(1)), "p99": int(rm.group(2)),
                      "n": int(rm.group(3))}
    return m


# ---- 格式化 ----

def fmt_ns(ns: int) -> str:
    """纳秒人类可读形：<1µs 保整数 ns，之上 µs/ms/s 两位小数。"""
    if ns < 1_000:
        return f"{ns}ns"
    if ns < 1_000_000:
        return f"{ns / 1_000:.2f}µs"
    if ns < 1_000_000_000:
        return f"{ns / 1_000_000:.2f}ms"
    return f"{ns / 1_000_000_000:.3f}s"


def _dw(s: str) -> int:
    """显示宽度：East Asian W/F 计 2（CJK 对齐用），其余 1。"""
    return sum(2 if unicodedata.east_asian_width(c) in ("W", "F") else 1
               for c in s)


def _fit(s: str, width: int) -> str:
    """右侧补空格到显示宽度（盒框对齐的唯一点）。"""
    return s + " " * max(width - _dw(s), 0)


def render_box(title: str, groups: list) -> str:
    """组化盒表：组间 ├─┤ 分隔，行内左列/右列对齐。"""
    flat = [row for g in groups for row in g]
    leftw = max(_dw(left) for left, _ in flat) + 2
    inner = max([leftw + _dw(right) for _, right in flat]
                + [_dw(title), MIN_INNER])
    bar = "─" * (inner + 2)
    out = ["┌" + bar + "┐", "│ " + _fit(title, inner) + " │"]
    for g in groups:
        out.append("├" + bar + "┤")
        for left, right in g:
            out.append("│ " + _fit(left, leftw) + _fit(right, inner - leftw)
                       + " │")
    out.append("└" + bar + "┘")
    return "\n".join(out)


def metric_rows(m: dict) -> tuple:
    """遥测字典 → (指标行 [(label, value, 状态标记)], PERF 三指标通过位)。"""
    rows, trio = [], []
    for key, label in (("pick", "pick_next:"), ("freeze", "freeze/thaw:"),
                       ("billing", "billing:")):
        e = m.get(key)
        lim = LIMITS.get(key)
        if e:
            ns = e["cycles"] * 1_000_000_000 // FREQ_HZ
            unit = "cycles" if e["cycles"] != 1 else "cycle "  # 对齐括号列
            value = f"{e['cycles']} {unit:6} ({fmt_ns(ns)})"
            ok = (ns < lim) if lim is not None else None
        else:
            value, ok = "—", False
        trio.append(bool(ok))
        rows.append((label, value, "" if ok is None
                     else ("PASS" if ok else "FAIL")))

    f = m.get("fairness")
    if f:
        ok = f["spread"] <= FAIR_SPREAD_LIMIT
        rows.append(("fairness:",
                     f"spread={f['spread']} [{','.join(map(str, f['hits']))}]",
                     "ok" if ok else "OVER"))
    else:
        rows.append(("fairness:", "—", ""))

    p = m.get("policy")
    if p and p["delta_pct"] is not None:
        ok = p["delta_pct"] < POLICY_DELTA_LIMIT_PCT
        rows.append(("policy delta:",
                     f"{p['delta_pct']:g}% (base={p['base']})",
                     "ok" if ok else "OVER"))
    elif p:
        rows.append(("policy delta:", f"(base={p['base']})，delta 未打印", ""))
    else:
        rows.append(("policy delta:", "—", ""))

    d = m.get("divergence")
    if d:
        verdict = _div_judge(d["div"], d["eval"],
                             PROMOTE_THRESHOLD, DEMOTE_THRESHOLD)
        rate = (f"{d['rate_pct']:.1f}%" if d["rate_pct"] is not None else "—")
        rows.append(("divergence:", f"{d['div']}/{d['eval']} ({rate})",
                     verdict["recommendation"]))
    else:
        rows.append(("divergence:", "—", ""))

    for src, labels in (("wake", ("wake p50:", "wake p99:")),
                        ("rwake", ("rwake p50:", "rwake p99:"))):
        e = m.get(src)
        if not e:  # rwake 可选行——有才出（「如果有」）
            if src == "wake":
                rows.extend((lbl, "—", "") for lbl in labels)
            continue
        for lbl, k in zip(labels, ("p50", "p99")):
            ticks = e[k]
            rows.append((lbl,
                         f"{ticks} ticks "
                         f"({fmt_ns(ticks * 1_000_000_000 // FREQ_HZ)})",
                         f"n={e['n']}"))
    return rows, trio


# ---- 基线对比 ----

def _get(m: dict, path: tuple):
    """路径取值；任一级缺席/None → None（缺席不算 0）。"""
    cur = m
    for k in path:
        if not isinstance(cur, dict) or cur.get(k) is None:
            return None
        cur = cur[k]
    return float(cur)


def compare_baseline(m: dict, base: dict, tol: float) -> tuple:
    """本次 vs 基线 → (对比行 [(label, value, 标记)], 回归指标名列表)。"""
    rows, regs = [], []
    for label, path, kind, unit in _SPECS:
        cv, bv = _get(m, path), _get(base, path)
        if bv is None and cv is None:
            continue
        if bv is None:  # 本次新采到的指标
            rows.append((f"{label}:", f"{cv:g} {unit}".strip(), "新增"))
            continue
        if cv is None:  # 基线有、本次缺——视为回归（数据面断链）
            rows.append((f"{label}:", f"基线 {bv:g}，本次缺失", "回归"))
            regs.append(label)
            continue
        if kind == "rel":
            reg = cv > bv * (1 + tol) if bv > 0 else cv > 0
            better = cv < bv * (1 - tol) if bv > 0 else cv < bv
        else:
            reg = (cv - bv) > PP_TOL
            better = (bv - cv) > PP_TOL
        pct = f"({(cv - bv) / bv * 100:+.1f}%)" if bv > 0 else "(+∞%)"
        mark = "回归" if reg else ("改善" if better else "稳")
        if reg:
            regs.append(label)
        mid = f"{bv:g} → {cv:g}"
        if unit == "%":  # 百分号贴数值，无空格
            mid += "%"
        elif unit:
            mid += f" {unit}"
        rows.append((f"{label}:", f"{mid} {pct}", mark))
    return rows, regs


# ---- 主流程 ----

# --demo 固定样本：八类行全覆盖 + 噪声行（验收 3/3 PASS、divergence 恰坐
# HOLD 线 50.0%、BENCHRWAKE 在场演示「如果有」行）。
DEMO_INPUT = (
    "[    0.000000] BerryOS boot: kmain enter\n"
    "PP-K ok\n"
    "PERF policy base=7 empty=5 full=9 delta=42.8%\n"
    "PERF pick avg=20 n=1000\n"
    "PERF freeze avg=5 n=200\n"
    "PERF billing avg=1 n=1000\n"
    "PERF-K ok\n"
    "FAIR-K spread=0 hits=[25,25,25,25,]\n"
    "PERF policy divergence=6 eval=12\n"
    "HDRWAKE ok p50=63 p99=125 n=128\n"
    "BENCHRWAKE p50=625000 p99=689000 n=64\n"
    "smoke: all markers present\n"
)


def main() -> int:
    ap = argparse.ArgumentParser(
        description="BerryOS 调度器统一仪表盘（遥测收割 + 验收 + 回归检测）")
    ap.add_argument("input", nargs="?", help="串口日志文件（缺省 stdin）")
    ap.add_argument("--demo", action="store_true", help="内置样例数据演示")
    ap.add_argument("--baseline", metavar="FILE",
                    help="基线 JSON（--save 产物）——回归检测")
    ap.add_argument("--save", metavar="FILE", help="把本次遥测存为基线 JSON")
    ap.add_argument("--json", action="store_true",
                    help="输出机器可读 JSON（不出表）")
    ap.add_argument("--tol", type=float, default=REL_TOL_DEFAULT,
                    help=f"周期/ticks 类回归相对容差（缺省 {REL_TOL_DEFAULT}）")
    args = ap.parse_args()

    if args.demo:
        print(DEMO_INPUT, file=sys.stderr, end="")
        text = DEMO_INPUT
    elif args.input:
        try:
            with open(args.input, "r", errors="replace") as f:
                text = f.read()
        except OSError as ex:
            print(f"ERROR: 输入文件不可读: {ex}", file=sys.stderr)
            return 1
    else:
        text = sys.stdin.read()

    m = parse_telemetry(text)
    if not m:
        print("ERROR: 无可用调度遥测行（PERF/FAIR-K/HDRWAKE/BENCHRWAKE）",
              file=sys.stderr)
        return 1

    if args.save:
        try:
            with open(args.save, "w", encoding="utf-8") as f:
                json.dump(m, f, ensure_ascii=False, indent=2)
                f.write("\n")
        except OSError as ex:
            print(f"ERROR: 基线写不出: {ex}", file=sys.stderr)
            return 1
        print(f"saved: {args.save}", file=sys.stderr)

    if args.json:
        print(json.dumps(m, ensure_ascii=False, indent=2))
        return 0

    rows, trio = metric_rows(m)
    passed, total = sum(trio), len(trio)
    acc_ok = passed == total

    groups = [[(f"{label:<15}{value}", tag) for label, value, tag in rows]]
    regs = []
    if args.baseline:
        try:
            with open(args.baseline, "r", encoding="utf-8") as f:
                base = json.load(f)
        except (OSError, ValueError) as ex:
            print(f"ERROR: 基线文件不可读: {ex}", file=sys.stderr)
            return 1
        brows, regs = compare_baseline(m, base, args.tol)
        g = [(f"基线对比: {os.path.basename(args.baseline)}", "")]
        g += [(f"{label:<15}{value}", tag) for label, value, tag in brows]
        g.append(("回归: " + (", ".join(regs) if regs else "无"),
                  "REG" if regs else "OK"))
        groups.append(g)

    groups.append([(f"验收: {passed}/{total}", "PASS" if acc_ok else "FAIL")])
    print(render_box("BerryOS 调度器仪表盘", groups))

    return 0 if (acc_ok and not regs) else 1


if __name__ == "__main__":
    sys.exit(main())
