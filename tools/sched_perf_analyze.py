#!/usr/bin/env python3
"""调度 1.1 性能分析工具——从 QEMU 串口 PERF 行对照验收标准。

用法：
  qemu ... | python3 tools/sched_perf_analyze.py
  python3 tools/sched_perf_analyze.py --demo

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


def main():
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
