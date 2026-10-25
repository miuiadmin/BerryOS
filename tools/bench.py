#!/usr/bin/env python3
"""B-0 性能基准单口：QEMU 裸跑 → 收割 BENCH 行 → 与基线对拍（宽松因子门禁）。

设计 = docs/基准与压力测试设计.md §5-6。口径纪律（测试策略 §7）：
- 内核计时 = CNTVCT 原始周期数，TCG 下 ≈ 实时/2——绝对值只做**同机同 QEMU
  的相对比较**；跨机器（本地 mac vs CI ubuntu runner）绝对值不可比。
- 因此门禁 = **宽松因子**（默认 3x）：拦「数量级劣化」（算法回退/误入慢路），
  不拦小幅噪声。紧门禁（±20% 级）留给 -icount 确定性通道（B-2）。
- 严格确定性断言（泄漏金丝雀 = 0、标记到齐）在**内核侧/smoke 侧**执法，
  本工具只管数值对拍——「严格」与「宽松」各守各的确定性边界。

用法（仓库根）：
  python3 tools/bench.py             # 跑一次 + 对拍基线（无基线 = 只出表）
  python3 tools/bench.py --update    # 跑一次 + 刷新基线（tools/bench_baseline.txt）
  python3 tools/bench.py --factor 5  # 放宽因子（CI 慢机场景）

退出码：0 = 过/仅出表；1 = 劣化超因子或收割不齐（可作 CI 步骤）。
"""

import argparse
import re
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
BASELINE = REPO / "tools" / "bench_baseline.txt"
BOOT_TIMEOUT = 120  # 秒：boot 到 BENCH ok 的帽（与 smoke DEADLINE_BOOT 同口径）

BENCH_RE = re.compile(r"^BENCH,([a-z0-9_]+),(\d+),(\d+)$")
DONE_RE = re.compile(r"^BENCH ok n=(\d+)$")


def kernel_elf() -> str:
    out = subprocess.run(
        ["bash", str(REPO / "tools" / "paths.sh"), "kernel"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    return out


def run_once(kernel: str) -> dict[str, int]:
    """QEMU 裸跑到 BENCH ok 行，收割 {name: cyc_per_op}。

    串口落临时文件 + 轮询（smoke.sh 同款）——直接 readline 会在 boot 静默
    期无限阻塞，deadline 形同虚设。
    """
    with tempfile.NamedTemporaryFile(
        prefix="berryos-bench.", suffix=".log", delete=False
    ) as tf:
        log_path = tf.name
    proc = subprocess.Popen(
        [
            "qemu-system-aarch64", "-machine", "virt", "-cpu", "cortex-a72",
            "-m", "128M", "-nographic", "-kernel", kernel,
        ],
        stdin=subprocess.DEVNULL, stdout=open(log_path, "w"),
        stderr=subprocess.STDOUT, start_new_session=True,
    )
    try:
        deadline = time.time() + BOOT_TIMEOUT
        while time.time() < deadline:
            if proc.poll() is not None:
                break  # QEMU 提前退出（崩溃）——带着现有日志走收割失败路径
            text = Path(log_path).read_text()
            if DONE_RE.search(text):
                break
            time.sleep(1)
        text = Path(log_path).read_text()
    finally:
        # QEMU halt 后不退出（wfe 死循环）——收完即杀（与 smoke 同款；
        # start_new_session 使 SIGTERM 只打 QEMU 不打本进程组）。
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
        Path(log_path).unlink(missing_ok=True)

    metrics: dict[str, int] = {}
    done_n = None
    for line in text.splitlines():
        m = BENCH_RE.match(line.strip())
        if m:
            metrics[m.group(1)] = int(m.group(3))
        d = DONE_RE.match(line.strip())
        if d:
            done_n = int(d.group(1))
    if done_n is None:
        raise SystemExit(
            f"bench.py: BENCH ok 行 {BOOT_TIMEOUT}s 未出现（boot 失败？）——"
            f"串口尾部：{' | '.join(text.splitlines()[-5:])}")
    if len(metrics) != done_n:
        raise SystemExit(f"bench.py: BENCH 行数 {len(metrics)} ≠ BENCH ok n={done_n}"
                         "（内核侧计数与出表不一致——改 bench.rs 后同步 N_METRICS）")
    return metrics


def load_baseline() -> dict[str, int]:
    if not BASELINE.exists():
        return {}
    out: dict[str, int] = {}
    for line in BASELINE.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        m = BENCH_RE.match(f"BENCH,{line}")
        if m:
            out[m.group(1)] = int(m.group(3))
    return out


def save_baseline(metrics: dict[str, int]) -> None:
    lines = [
        "# BerryOS B-0 内核微基准基线（机器可读：name,iters,cyc_per_op）。",
        "# 口径 = CNTVCT 原始周期数（TCG ≈ 实时/2）——只做同机同 QEMU 相对比较；",
        "# 刷新：python3 tools/bench.py --update（改内核后跑一次，diff 随源入册）。",
    ]
    iters = {m: n for m, n in _iter_table().items()}
    for name, cyc in sorted(metrics.items()):
        lines.append(f"{name},{iters.get(name, 0)},{cyc}")
    BASELINE.write_text("\n".join(lines) + "\n")


def _iter_table() -> dict[str, int]:
    """基准项 → 迭代数（与 bench.rs 定值镜像——漂移只影响基线可读性，对拍按名）。"""
    return {
        "cntvct_pair": 4096,
        "frame_alloc4k": 1024,
        "frame_alloc64k": 256,
        "vmo_commit1p": 256,
        "vmo_rw4k": 128,
        "handle_evt": 1024,
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--update", action="store_true", help="刷新基线文件")
    ap.add_argument("--factor", type=float, default=3.0,
                    help="劣化因子门禁（默认 3.0——拦数量级劣化不拦噪声）")
    ap.add_argument("--kernel", default=None, help="内核 ELF 路径（默认 paths.sh 解析）")
    args = ap.parse_args()

    kernel = args.kernel or kernel_elf()
    metrics = run_once(kernel)

    print(f"{'name':<16} {'cyc/op':>10}")
    for name, cyc in sorted(metrics.items()):
        print(f"{name:<16} {cyc:>10}")

    base = load_baseline()
    if args.update:
        save_baseline(metrics)
        print(f"bench.py: 基线已刷新 → {BASELINE.relative_to(REPO)}")
        return

    if not base:
        print("bench.py: 无基线（首跑）——仅出表；`python3 tools/bench.py --update` 立基线")
        return

    bad = []
    for name, cyc in sorted(metrics.items()):
        old = base.get(name)
        if old is None:
            print(f"bench.py: 新基准项 {name}（基线缺席，不过门禁）")
            continue
        if cyc > old * args.factor:
            bad.append(f"{name}: {old} → {cyc}（超基线 ×{cyc / old:.1f} > {args.factor}）")
    missing = sorted(set(base) - set(metrics))
    if bad or missing:
        for b in bad:
            print(f"bench.py: FAIL {b}", file=sys.stderr)
        for m in missing:
            print(f"bench.py: FAIL 基准项 {m} 从内核输出消失（改 bench.rs 后须 --update）",
                  file=sys.stderr)
        raise SystemExit(1)
    print(f"bench.py: 对拍过（因子 ×{args.factor}，{len(metrics)} 项全在界内）")


if __name__ == "__main__":
    main()
