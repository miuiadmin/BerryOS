#!/usr/bin/env python3
"""B-0/B-1 性能基准单口：QEMU 跑 → 收割 BENCH/BENCH1 行 → 与基线对拍（宽松因子门禁）。

设计 = docs/基准与压力测试设计.md §5-6（B-0 内核微基准）+ §8（B-1 EL0 用户态
基准）。口径纪律（测试策略 §7）：
- 内核计时 = CNTVCT 原始周期数，TCG 下 ≈ 实时/2——绝对值只做**同机同 QEMU
  的相对比较**；跨机器（本地 mac vs CI ubuntu runner）绝对值不可比。
- 因此门禁 = **宽松因子**（默认 3x）：拦「数量级劣化」（算法回退/误入慢路），
  不拦小幅噪声。紧门禁（±20% 级）留给 -icount 确定性通道（B-2）。
- 严格确定性断言（泄漏金丝雀 = 0、标记到齐）在**内核侧/smoke 侧**执法，
  本工具只管数值对拍——「严格」与「宽松」各守各的确定性边界。

两通道（BENCH_RE/BENCH1_RE 逗号锚定互不误配；基线文件双段行前缀区分）：
- 默认（B-0）：QEMU 裸跑收割内核 BENCH 行（boot 窗口六项）。
- --el0（B-1）：run-rootfs 形态 QEMU（-fw_cfg rootfs，无盘无网无 echod），
  等 bsh 链打到 `B1 ok n=4` 后收割 bsh relay 的 BENCH1 四行。裸相的内核
  BENCH 行也在串口上，但不属本通道收割面（段隔离）。
- --rt（R-1）：裸跑 QEMU + `-fw_cfg name=opt/berry/rt,string=1`（门控键——
  内核侧 rt_probe 仅在该具名项在位时 spawn；缺席 = run/smoke 零扰动，公约 7）
  等 `RT ok n=96`，非锚定收割 BENCHRT 三行。**只出表无基线**（设计 §10 无
  「门禁与基线」段；oversleep≥0 严格断言在内核侧 panic 执法 = L0，分布数值
  统计归统计——故 --rt 为独立通道：--update/--el0/--factor 组合一律显式
  拒绝，不静默忽略）。

用法（仓库根）：
  python3 tools/bench.py             # B-0：跑一次 + 对拍基线（无基线 = 只出表）
  python3 tools/bench.py --update    # B-0：跑一次 + 刷新基线 B-0 段（B-1 段保留）
  python3 tools/bench.py --factor 5  # 放宽因子（CI 慢机场景）
  python3 tools/bench.py --el0       # B-1：rootfs 形态跑一次 + 对拍 B-1 段
  python3 tools/bench.py --el0 --update  # B-1：跑一次 + 刷新 B-1 段（B-0 段保留）
  python3 tools/bench.py --rt        # R-1：裸跑（fw_cfg 门控 rt_probe）出三档表

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
ROOTFS = REPO / "rootfs.cpio"
BOOT_TIMEOUT = 120  # 秒：boot 到 BENCH ok 的帽（与 smoke DEADLINE_BOOT 同口径）
# B-1 通道帽：rootfs 走查 ~90s + bench1 spawn/SVC 链 + 稳态矩阵尾段——与
# smoke DEADLINE_ROOTFS=300s 同口径（P6 A-6 RPC 成本模型，勿拿 syscall
# cycle 外推）。
BOOT_TIMEOUT_EL0 = 300
# R-1 通道帽（设计 §10 定帽）：boot 头 ~10-20s + rt_probe 三档
# 32×(50+200+800)ms ≈ 34s（TCG 半率）——3x 余量。
BOOT_TIMEOUT_RT = 120

BENCH_RE = re.compile(r"^BENCH,([a-z0-9_]+),(\d+),(\d+)$")
# MULTILINE：done 正则要喂给轮询早退的 search(全串口日志)——无 MULTILINE 时
# ^ 只匹配字符串起点（首行 '1BerryOS boot'），标记行永不命中 → 每次跑满全帽
# （结果仍正确——收割走 splitlines 逐行 match——但「标记到齐即收」失效）。
DONE_RE = re.compile(r"^BENCH ok n=(\d+)$", re.MULTILINE)
# B-1 侧**不做行锚定**（首跑实踩）：bsh relay 的 BENCH1/B1 ok 行落在稳态
# 矩阵的 40KB 巨行中段（实测前缀 'ssss…BBBvPPP…d'）——^ 锚定逐行 match 恒
# 漏收、B1 ok 行首干净与否是竞速；两 pattern 均串内唯一子串（矩阵字符
# A-Z/a-z 无逗号数字序列、诊断行无 'B1 ok'/'BENCH1,' 子串），finditer/
# search 全文收割即正确（elf2pe rsym 雷同族教训：合成样本须复刻真实串口
# 行位形态，行锚定只对 B-0 内核侧裸行成立）。
BENCH1_RE = re.compile(r"BENCH1,([a-z0-9_]+),(\d+),(\d+)")
DONE1_RE = re.compile(r"B1 ok n=(\d+)")

# R-1（rt_probe oversleep）。收割同样**不做行锚定**（B-1 教训同族）：rt_probe
# prio 8 醒来会抢占 prio 0 矩阵任务的逐字符打印流——BENCHRT 行可落在矩阵
# 40KB 巨行中段，^ 锚定恒漏收。串内无伪命中面：矩阵字符 A-Z/a-z 无逗号数字
# 序列；`RT ok n=` 与 `BENCH ok n=` 字面互非子串（BENCHRT 行含 "RT," 不含
# "RT ok"）。n=32 字面 = task_rt_probe RT_SAMPLES 镜像（wire 面冻结——内核侧
# const assert 同步执法）。
BENCHRT_RE = re.compile(r"BENCHRT,delta=(\d+),n=32,p50=(\d+),p99=(\d+),max=(\d+)")
DONE_RT_RE = re.compile(r"RT ok n=(\d+)")
RT_TIERS = 3   # 档数（slice/4、slice、4·slice——kernel deltas 定值镜像）
RT_N = 32      # 每档样本数（kernel RT_SAMPLES 定值镜像）

# B-1 串口行在基线文件里的段前缀（load/save 对称双正则——P6 A-5 基准漂移雷：
# 读写两侧必须同形，否则「双绿」退化成对陈旧基准自证）。
B1_PREFIX = "BENCH1,"


def kernel_elf() -> str:
    out = subprocess.run(
        ["bash", str(REPO / "tools" / "paths.sh"), "kernel"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    return out


def run_once(kernel: str, el0: bool = False) -> dict[str, int]:
    """QEMU 跑到收尾标记，收割 {name: cyc_per_op}。

    el0=False：裸跑到 BENCH ok（B-0 内核六项）。
    el0=True：run-rootfs 形态（-fw_cfg rootfs 递送；无盘无网——B-1 无网络
    面，echod 免挂）跑到 `B1 ok n=4`（bsh selftest 链尾），收割 bsh relay
    的 BENCH1 行。

    串口落临时文件 + 轮询（smoke.sh 同款）——直接 readline 会在 boot 静默
    期无限阻塞，deadline 形同虚设。
    """
    qemu_cmd = [
        "qemu-system-aarch64", "-machine", "virt", "-cpu", "cortex-a72",
        "-m", "128M", "-nographic", "-kernel", kernel,
    ]
    if el0:
        if not ROOTFS.exists():
            raise SystemExit("bench.py: rootfs.cpio 缺席——先 just build-user")
        qemu_cmd += [
            "-fw_cfg", f"name=opt/berry/rootfs,file={ROOTFS.resolve()}",
        ]
    bench_re, done_re = (BENCH1_RE, DONE1_RE) if el0 else (BENCH_RE, DONE_RE)
    timeout = BOOT_TIMEOUT_EL0 if el0 else BOOT_TIMEOUT

    with tempfile.NamedTemporaryFile(
        prefix="berryos-bench.", suffix=".log", delete=False
    ) as tf:
        log_path = tf.name
    proc = subprocess.Popen(
        qemu_cmd,
        stdin=subprocess.DEVNULL, stdout=open(log_path, "w"),
        stderr=subprocess.STDOUT, start_new_session=True,
    )
    try:
        deadline = time.time() + timeout
        while time.time() < deadline:
            if proc.poll() is not None:
                break  # QEMU 提前退出（崩溃）——带着现有日志走收割失败路径
            # errors=replace：串口可能含跨 console 分片撕裂的 UTF-8 序列
            # （hexyl 显示区 '×'=0xC3 0x97 被斩在中间——B-1 首跑实踩；
            # B-0 裸跑无此面但同读点同治）。BENCH1/BENCH 收割面纯 ASCII 无扰。
            text = Path(log_path).read_text(encoding="utf-8", errors="replace")
            if done_re.search(text):
                break
            time.sleep(1)
        text = Path(log_path).read_text(encoding="utf-8", errors="replace")
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
    if el0:
        # B-1：矩阵行位污染 → 全文子串收割（见 BENCH1_RE 注）；B-0 侧维持
        # 逐行 match 裸行锚定（内核 bench 段先于矩阵起跑，行位干净——零漂移）。
        for m in BENCH1_RE.finditer(text):
            metrics[m.group(1)] = int(m.group(3))
        d = DONE1_RE.search(text)
        if d:
            done_n = int(d.group(1))
    else:
        for line in text.splitlines():
            m = bench_re.match(line.strip())
            if m:
                metrics[m.group(1)] = int(m.group(3))
            d = done_re.match(line.strip())
            if d:
                done_n = int(d.group(1))
    tag = "B1 ok" if el0 else "BENCH ok"
    if done_n is None:
        raise SystemExit(
            f"bench.py: {tag} 行 {timeout}s 未出现（boot 失败？）——"
            f"串口尾部：{' | '.join(text.splitlines()[-5:])}")
    if len(metrics) != done_n:
        raise SystemExit(f"bench.py: BENCH1 行数 {len(metrics)} ≠ B1 ok n={done_n}"
                         "（bsh relay 面与内核/程序出表不一致——改 bench1 后同步）"
                         if el0 else
                         f"bench.py: BENCH 行数 {len(metrics)} ≠ BENCH ok n={done_n}"
                         "（内核侧计数与出表不一致——改 bench.rs 后同步 N_METRICS）")
    return metrics


def run_rt(kernel: str) -> list[tuple[int, int, int, int]]:
    """R-1 通道：裸跑 QEMU（+fw_cfg 具名项 opt/berry/rt 门控 rt_probe spawn）
    等 `RT ok n=96`，非锚定收割三行 BENCHRT → [(delta, p50, p99, max)]。

    独立函数不复用 run_once：收割面（按档元组表 vs 按名 dict）与一致性校验
    （档数/单调 vs 行数==n）都不同构，混入 mode 分支反而扰动 B-0/B-1 既有
    两通道（smoke/bench 依赖它们——零回归面优先）。
    """
    qemu_cmd = [
        "qemu-system-aarch64", "-machine", "virt", "-cpu", "cortex-a72",
        "-m", "128M", "-nographic", "-kernel", kernel,
        # 门控键：内核侧 find_file("opt/berry/rt") 命中才 spawn rt_probe；
        # string backing 也登记 FILE_DIR（QEMU 规范行为），内容永不被读。
        "-fw_cfg", "name=opt/berry/rt,string=1",
    ]
    with tempfile.NamedTemporaryFile(
        prefix="berryos-bench-rt.", suffix=".log", delete=False
    ) as tf:
        log_path = tf.name
    proc = subprocess.Popen(
        qemu_cmd,
        stdin=subprocess.DEVNULL, stdout=open(log_path, "w"),
        stderr=subprocess.STDOUT, start_new_session=True,
    )
    try:
        deadline = time.time() + BOOT_TIMEOUT_RT
        while time.time() < deadline:
            if proc.poll() is not None:
                break  # QEMU 提前退出（崩溃）——带着现有日志走收割失败路径
            text = Path(log_path).read_text(encoding="utf-8", errors="replace")
            if DONE_RT_RE.search(text):
                break
            time.sleep(1)
        text = Path(log_path).read_text(encoding="utf-8", errors="replace")
    finally:
        # 与 run_once 同款：halt 后不退出，收完即杀。
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
        Path(log_path).unlink(missing_ok=True)

    d = DONE_RT_RE.search(text)
    if d is None:
        raise SystemExit(
            f"bench.py: RT ok 行 {BOOT_TIMEOUT_RT}s 未出现（boot 失败 / rt_probe "
            f"panic（负 oversleep 是 L0 执法）/ fw_cfg 门控未命中？）——"
            f"串口尾部：{' | '.join(text.splitlines()[-5:])}")
    done_n = int(d.group(1))
    rows = [(int(m.group(1)), int(m.group(2)), int(m.group(3)), int(m.group(4)))
            for m in BENCHRT_RE.finditer(text)]
    deltas = [r[0] for r in rows]
    # 一致性（镜像 run_once 的 len≠n 检查）：恰 3 档、n 合计 3×32、delta 单调
    # 不重（slice/4 < slice < 4·slice；不硬编 62.5MHz 折算值——绝对值跨机型
    # 不可比，形状断言已够）。
    if (len(rows) != RT_TIERS or done_n != RT_TIERS * RT_N
            or deltas != sorted(deltas) or len(set(deltas)) != RT_TIERS):
        raise SystemExit(
            f"bench.py: BENCHRT 收割不齐（rows={len(rows)} done_n={done_n} "
            f"deltas={deltas}——须恰 {RT_TIERS} 行、n 合计 {RT_TIERS * RT_N}、"
            f"delta 单调不重；改 task_rt_probe 后同步）")
    return rows


def load_baseline() -> tuple[dict[str, int], dict[str, int]]:
    """双段解析：返回 (B-0 段, B-1 段)。

    文件内 B-0 行 = 裸 CSV `name,iters,cyc`；B-1 行带完整前缀
    `BENCH1,name,iters,cyc`（P6 A-5 基准漂移雷——读侧与 save 侧严格同形）。
    段缺席 = 空 dict（对拍跳过合法——首跑立档归验收阶段）。
    """
    b0: dict[str, int] = {}
    b1: dict[str, int] = {}
    if not BASELINE.exists():
        return b0, b1
    for raw in BASELINE.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m1 = BENCH1_RE.match(line)
        if m1:
            b1[m1.group(1)] = int(m1.group(3))
            continue
        m = BENCH_RE.match(f"BENCH,{line}")
        if m:
            b0[m.group(1)] = int(m.group(3))
    return b0, b1


def save_baseline(el0: bool, metrics: dict[str, int]) -> None:
    """刷新基线**单段**，另一段原样保留（load 回读往返——--update 任一模式
    都不 wipe 另一段的数值历史）。"""
    b0_keep, b1_keep = load_baseline()
    if el0:
        b1_keep = metrics
    else:
        b0_keep = metrics
    lines = [
        "# BerryOS 性能基准基线（机器可读，双段行前缀区分）：",
        "#   B-0 段 = 裸 CSV `name,iters,cyc`（内核微基准，bench.py 默认模式）；",
        "#   B-1 段 = `BENCH1,name,iters,cyc`（EL0 用户态基准，bench.py --el0）。",
        "# 口径 = CNTVCT 原始周期数（TCG ≈ 实时/2）——只做同机同 QEMU 相对比较；",
        "# 刷新：python3 tools/bench.py [--el0] --update（只动对应段，另一段保留）。",
    ]
    iters0 = _iter_table()
    for name, cyc in sorted(b0_keep.items()):
        lines.append(f"{name},{iters0.get(name, 0)},{cyc}")
    if b1_keep:
        lines.append("")
        lines.append("# —— B-1 段（BENCH1 前缀）——")
    iters1 = _iter_table_el0()
    for name, cyc in sorted(b1_keep.items()):
        lines.append(f"{B1_PREFIX}{name},{iters1.get(name, 0)},{cyc}")
    BASELINE.write_text("\n".join(lines) + "\n")


def _iter_table() -> dict[str, int]:
    """B-0 基准项 → 迭代数（与 bench.rs 定值镜像——漂移只影响基线可读性，对拍按名）。"""
    return {
        "cntvct_pair": 4096,
        "frame_alloc4k": 1024,
        "frame_alloc64k": 256,
        "vmo_commit1p": 256,
        "vmo_rw4k": 128,
        "handle_evt": 1024,
    }


def _iter_table_el0() -> dict[str, int]:
    """B-1 基准项 → 迭代数（与 bench1 main.rs 定值镜像——三镜之一，
    bsh bench1_selftest 字面常量是第二镜）。"""
    return {
        "sys_null": 4096,
        "sys_vmo_rw4k": 128,
        "ipc_enqueue8": 128,
        "evt_signal": 1024,
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--update", action="store_true", help="刷新基线文件（本模式对应段）")
    ap.add_argument("--factor", type=float, default=None,
                    help="劣化因子门禁（默认 3.0——拦数量级劣化不拦噪声）")
    ap.add_argument("--el0", action="store_true",
                    help="B-1 通道：run-rootfs 形态收割 BENCH1 四行（默认 B-0 裸跑）")
    ap.add_argument("--rt", action="store_true",
                    help="R-1 通道：裸跑（fw_cfg 门控 rt_probe）收割 BENCHRT 三行"
                         "（只出表无基线——独立通道，--update/--el0/--factor 组合拒绝）")
    ap.add_argument("--kernel", default=None, help="内核 ELF 路径（默认 paths.sh 解析）")
    args = ap.parse_args()

    if args.rt and (args.update or args.el0 or args.factor is not None):
        # RT 无基线段（设计 §10 只出表）——误触 save_baseline 会按双段前缀
        # 逻辑误写 bench_baseline.txt；--el0/--factor 同样不进 --rt 早退
        # 分支：静默忽略 = 用户要的通道/门禁凭空蒸发（--el0 --rt 实际只跑
        # RT、B-1 静默缺席且退出码 0——三镜审查实抓）。组合一律拒绝。
        raise SystemExit(
            "bench.py: --rt 独立通道（只出表无基线）——不与 --update/--el0/"
            "--factor 组合（--update 误写基线双段；--el0 通道会被静默丢弃；"
            "--factor 在 RT 无门禁面）")

    kernel = args.kernel or kernel_elf()
    if args.rt:
        rows = run_rt(kernel)
        print(f"[R-1(rt)] {'delta':>12} {'p50':>10} {'p99':>10} {'max':>10}")
        for delta, p50, p99, mx in rows:
            print(f"{'':<8} {delta:>12} {p50:>10} {p99:>10} {mx:>10}")
        print("bench.py: R-1 只出表不对拍（oversleep>=0 严格断言在内核侧 panic 执法"
              "（L0）；分布数值统计归统计——公约 2）")
        return
    # --factor 默认 None（非 3.0）：default 值会淹没「显式传参」信号——上方
    # --rt 组合检测靠 None 区分「用户真传了 --factor」。
    factor = 3.0 if args.factor is None else args.factor
    metrics = run_once(kernel, el0=args.el0)

    tag = "B-1(el0)" if args.el0 else "B-0(kernel)"
    print(f"[{tag}] {'name':<16} {'cyc/op':>10}")
    for name, cyc in sorted(metrics.items()):
        print(f"{'':<8} {name:<16} {cyc:>10}")

    b0_base, b1_base = load_baseline()
    base = b1_base if args.el0 else b0_base
    if args.update:
        save_baseline(args.el0, metrics)
        print(f"bench.py: 基线 {tag} 段已刷新 → {BASELINE.relative_to(REPO)}")
        return

    if not base:
        print(f"bench.py: {tag} 段无基线（首跑）——仅出表；"
              f"`python3 tools/bench.py{' --el0' if args.el0 else ''} --update` 立基线")
        return

    bad = []
    for name, cyc in sorted(metrics.items()):
        old = base.get(name)
        if old is None:
            print(f"bench.py: 新基准项 {name}（基线缺席，不过门禁）")
            continue
        if cyc > old * factor:
            bad.append(f"{name}: {old} → {cyc}（超基线 ×{cyc / old:.1f} > {factor}）")
    missing = sorted(set(base) - set(metrics))
    if bad or missing:
        for b in bad:
            print(f"bench.py: FAIL {b}", file=sys.stderr)
        for m in missing:
            print(f"bench.py: FAIL 基准项 {m} 从输出消失（改基准源后须 --update）",
                  file=sys.stderr)
        raise SystemExit(1)
    print(f"bench.py: 对拍过（{tag} 因子 ×{factor}，{len(metrics)} 项全在界内）")


if __name__ == "__main__":
    main()
