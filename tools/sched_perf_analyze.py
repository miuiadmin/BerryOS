#!/usr/bin/env python3
"""调度 1.1 性能分析工具——从 QEMU 串口 PERF 行对照验收标准。

用法：
  qemu ... | python3 tools/sched_perf_analyze.py
  python3 tools/sched_perf_analyze.py --demo
  python3 tools/sched_perf_analyze.py --selftest   # 门禁自检（a-3 U4 纪律）

验收标准（调点调度-v1.1.md）：
- pick < 5µs（§10-R6 kill 线 @1k 任务）
- freeze/thaw < 5ms（§8.3-3 性能门）
- billing < 1µs（§8.1-M1.1-4 记账开销 <1% × 100µs slice）
"""

import re
import sys

# QEMU virt CNTFRQ
FREQ_HZ = 62_500_000

# 验收阈值（纳秒）
LIMITS = {
    "pick": 5_000,    # 5µs
    "freeze": 5_000_000,  # 5ms
    "billing": 1_000,    # 1µs
}


def parse_perf_lines(text: str) -> list:
    """解析 PERF 行 → [(op, avg_cycles, count)]。"""
    results = []
    for m in re.finditer(r"PERF (\w+) avg=(\d+) n=(\d+)", text):
        op = m.group(1)
        cycles = int(m.group(2))
        count = int(m.group(3))
        results.append((op, cycles, count))
    return results


def analyze(entries: list) -> list:
    """对照验收标准。"""
    report = []
    for op, cycles, count in entries:
        ns = cycles * 1_000_000_000 // FREQ_HZ
        limit = LIMITS.get(op)
        if limit is None:
            report.append({"op": op, "avg_cycles": cycles, "avg_ns": ns,
                           "limit_ns": None, "pass": None, "note": "no limit"})
        else:
            ok = ns < limit
            margin = limit // max(ns, 1)
            report.append({"op": op, "avg_cycles": cycles, "avg_ns": ns,
                           "limit_ns": limit, "pass": ok,
                           "note": f"{margin}x margin"})
    return report


def selftest() -> int:
    """门禁自检（a-3 U4 纪律同款）——PERF 行解析与验收对照的纯函数面钉板。

    本工具是调度 1.1 验收标准（调点调度-v1.1.md：pick <5µs / freeze·thaw
    <5ms / billing <1µs）的唯一宿主侧判官，此前只有 --demo 一次性展示，
    门禁自身坏（FREQ 常量漂移 / 阈值严格性松成闭区间 / regex 误收噪声行
    / exit-code 契约漂移）= 性能验收静默劣化（红线松了没人知道）。覆盖：
    - parse_perf_lines：三 op 正常收 / 噪声免疫（PERFx 前缀、缺 op 双空格、
      尾部残渣容忍）/ 保持出现序
    - analyze 手算向量：62.5MHz 下 ns=cycles×16 恰为整数——pick 312cyc
      =4992ns PASS、313=5008ns FAIL（5µs 严格边界）；freeze 312500cyc
      恰等 5ms 被拒（严格 <，闭区间陷阱）；billing 62/63 跨 1µs 界
    - margin 注记（16x/15x/1x=恰达阈值的降级面、0x=超阈）与 no-limit
      op 的 pass=None 三值逻辑（未知 op 不连坐 FAIL）
    - exit-code 契约：--demo 绿=0 / 无 PERF 行=1 / 有 FAIL 面=1 /
      未知 op 单独在场=0（no limit 不拦）
    - 确定性：同输入两次 byte 级一致（基线对拍前提）
    """
    import subprocess

    failures = []

    def check(name, cond, detail=""):
        if cond:
            print(f"  ok {name}")
        else:
            failures.append(f"{name}: {detail}")
            print(f"  FAIL {name}: {detail}")

    # --- 0. 规格常量钉板（验收阈值/FREQ 的单一事实源，漂移即咬） ---
    check("LIMITS pinned", LIMITS == {"pick": 5_000, "freeze": 5_000_000,
          "billing": 1_000}, f"{LIMITS}")
    check("FREQ pinned", FREQ_HZ == 62_500_000, f"{FREQ_HZ}")

    # --- 1. parse_perf_lines：正常收 + 噪声免疫 + 保序 ---
    text = ("PERF pick avg=26 n=1000\n"
            "noise PERFx pick avg=1 n=1\n"       # PERFx ≠ PERF → 不收
            "PERF  avg=5 n=5\n"                  # 缺 op（双空格）→ 不收
            "PERF billing avg=1 n=1000 tail\n"   # 尾部残渣容忍
            "PERF freeze avg=15 n=200\n")
    entries = parse_perf_lines(text)
    check("parse happy+order", entries == [("pick", 26, 1000),
          ("billing", 1, 1000), ("freeze", 15, 200)], f"{entries}")
    check("parse noise immune", parse_perf_lines("PERF avg=5 n=5\n") == [])
    check("parse empty", parse_perf_lines("boot ok\n") == [])
    check("parse keeps duplicates",
          parse_perf_lines("PERF pick avg=1 n=1\nPERF pick avg=2 n=2\n")
          == [("pick", 1, 1), ("pick", 2, 2)])

    # --- 2. analyze 手算向量：62.5MHz → ns = cycles×16（恰整数）---
    # pick 界：5000ns/16 = 312.5 cyc → 312=4992ns PASS、313=5008ns FAIL
    rep = analyze([("pick", 312, 1000)])
    check("pick 312cyc=4992ns pass", rep[0]["avg_ns"] == 4992
          and rep[0]["pass"] is True and rep[0]["note"] == "1x margin",
          f"{rep}")
    rep = analyze([("pick", 313, 1000)])
    check("pick 313cyc=5008ns fail", rep[0]["avg_ns"] == 5008
          and rep[0]["pass"] is False and rep[0]["note"] == "0x margin",
          f"{rep}")
    # freeze 界：5e6ns/16 = 312500 cyc 恰等 → 严格 < 拒（闭区间陷阱）
    rep = analyze([("freeze", 312500, 10)])
    check("freeze exact-5ms rejected", rep[0]["avg_ns"] == 5_000_000
          and rep[0]["pass"] is False and rep[0]["note"] == "1x margin",
          f"{rep}")
    rep = analyze([("freeze", 312499, 10)])
    check("freeze 312499cyc pass", rep[0]["pass"] is True, f"{rep}")
    # billing 界：1000ns/16 = 62.5 cyc → 62=992ns PASS、63=1008ns FAIL
    rep = analyze([("billing", 62, 1), ("billing", 63, 1)])
    check("billing 62/63 boundary",
          rep[0]["avg_ns"] == 992 and rep[0]["pass"] is True
          and rep[1]["avg_ns"] == 1008 and rep[1]["pass"] is False, f"{rep}")
    # 超阈 margin=0x；limit_ns/cycles 原样进报告
    rep = analyze([("billing", 200, 7)])
    check("over-limit 0x margin + fields", rep[0]["note"] == "0x margin"
          and rep[0]["limit_ns"] == 1000 and rep[0]["avg_cycles"] == 200,
          f"{rep}")
    # 未知 op：三值逻辑 pass=None，note=no limit（不连坐 FAIL）
    rep = analyze([("switch", 999_999, 5)])
    check("unknown op no-limit", rep[0]["pass"] is None
          and rep[0]["note"] == "no limit" and rep[0]["limit_ns"] is None,
          f"{rep}")

    # --- 3. exit-code 契约 + 端到端子进程真跑 ---
    p = subprocess.run([sys.executable, __file__, "--demo"],
                       capture_output=True, text=True)
    check("e2e --demo exit 0", p.returncode == 0
          and "PASS: 3/3 criteria met" in p.stdout, f"{p.stdout!r}")

    p = subprocess.run([sys.executable, __file__],
                       input="no perf lines here\n",
                       capture_output=True, text=True)
    check("e2e no-lines exit 1", p.returncode == 1
          and "no PERF lines" in p.stderr, f"rc={p.returncode}")

    p = subprocess.run([sys.executable, __file__],
                       input="PERF pick avg=313 n=1000\n",
                       capture_output=True, text=True)
    check("e2e failing line exit 1", p.returncode == 1
          and "0/1 criteria met" in p.stdout, f"rc={p.returncode}")

    p = subprocess.run([sys.executable, __file__],
                       input="PERF switch avg=999999 n=5\n",
                       capture_output=True, text=True)
    check("e2e unknown-op-only exit 0", p.returncode == 0
          and "0/1 criteria met" in p.stdout,
          f"rc={p.returncode} {p.stdout!r}")

    # --- 4. 确定性：同输入两次 byte 级一致（基线对拍前提） ---
    p1 = subprocess.run([sys.executable, __file__, "--demo"],
                        capture_output=True, text=True)
    p2 = subprocess.run([sys.executable, __file__, "--demo"],
                        capture_output=True, text=True)
    check("deterministic byte-equal",
          p1.stdout == p2.stdout and p1.returncode == p2.returncode)

    print(f"\nsched_perf_analyze selftest: "
          f"{'PASS' if not failures else 'FAIL'} ({len(failures)} failures)")
    return 0 if not failures else 1


def main():
    if "--selftest" in sys.argv:
        sys.exit(selftest())
    if "--demo" in sys.argv:
        text = "PERF pick avg=26 n=1000\nPERF freeze avg=15 n=200\nPERF billing avg=1 n=1000\n"
    else:
        text = sys.stdin.read()

    entries = parse_perf_lines(text)
    if not entries:
        print("ERROR: no PERF lines found", file=sys.stderr)
        sys.exit(1)

    report = analyze(entries)
    all_pass = all(r["pass"] is not False for r in report)

    import json
    print(json.dumps(report, indent=2))
    print(f"\n{'PASS' if all_pass else 'FAIL'}: "
          f"{sum(1 for r in report if r['pass'])}/{len(report)} criteria met")
    sys.exit(0 if all_pass else 1)


if __name__ == "__main__":
    main()
