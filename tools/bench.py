#!/usr/bin/env python3
"""B-0/B-1 性能基准单口：QEMU 跑 → 收割 BENCH/BENCH1 行 → 与基线对拍（宽松因子门禁）。

设计 = docs/基准与压力测试设计.md §5-6（B-0 内核微基准）+ §8（B-1 EL0 用户态
基准）。口径纪律（测试策略 §7）：
- 内核计时 = CNTVCT 原始周期数，TCG 下 ≈ 实时/2——绝对值只做**同机同 QEMU
  的相对比较**；跨机器（本地 mac vs CI ubuntu runner）绝对值不可比。
- 因此门禁 = **宽松因子**（默认 3x）：拦「数量级劣化」（算法回退/误入慢路），
  不拦小幅噪声。紧门禁（精确相等 / ±5% 级）由 -icount 确定性通道（B-2
  §13）承载，形态烘焙为 ICOUNT_TOL 常量（见下）。
- 严格确定性断言（泄漏金丝雀 = 0、标记到齐）在**内核侧/smoke 侧**执法，
  本工具只管数值对拍——「严格」与「宽松」各守各的确定性边界。

四通道（BENCH_RE/BENCH1_RE 逗号锚定互不误配；基线文件按通道分离）：
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
- --icount（B-2）：裸跑 QEMU 追加定值 `-icount shift=1,sleep=off`，等 boot
  尾 `BENCHWAKE n=`（非锚定），双面收割——六行 BENCH（行锚定）= L2 门禁面
  + BENCHWAKE 整行 = R-0 观测面（§13「依赖」段，观测不动门禁）。三条语义
  （设计 §13，防误用）：①CNTV 推进绑指令数（shift=1 虚拟时钟 = 每指令 2ns，
  折算 CNTVCT ≈0.125 cyc/instr）→「周期数」变成**指令数代理**，与 TCG 实时
  通道**基线不可比、分文件**（bench_baseline_icount.txt）；②boot 时序全变
  （tick/看门狗节奏虚拟时间化）→ 本通道**只跑 bench 收割**，不用于任何功能
  面验收；③确定性来源 = 同指令流同虚拟时钟——`sleep=off` 关实时对齐是
  **load-bearing**，删掉它虚拟时钟重新耦合宿主时钟，确定性通道失效。组合
  拒绝：--el0/--rt（通道混跑——一进程一种 QEMU 形态）、--factor（L2 形态 =
  烘焙常量 ICOUNT_TOL，可调因子会把 L2 静默降级回 L1 语义）。

用法（仓库根）：
  python3 tools/bench.py             # B-0：跑一次 + 对拍基线（无基线 = 只出表）
  python3 tools/bench.py --update    # B-0：跑一次 + 刷新基线 B-0 段（B-1 段保留）
  python3 tools/bench.py --factor 5  # 放宽因子（CI 慢机场景）
  python3 tools/bench.py --el0       # B-1：rootfs 形态跑一次 + 对拍 B-1 段
  python3 tools/bench.py --el0 --update  # B-1：跑一次 + 刷新 B-1 段（B-0 段保留）
  python3 tools/bench.py --rt        # R-1：裸跑（fw_cfg 门控 rt_probe）出三档表
  python3 tools/bench.py --icount    # B-2：-icount 确定性跑一次 + L2 对拍
                                      #（无基线 = 只出表 + 提示 --update）
  python3 tools/bench.py --icount --update  # B-2：立/刷 icount 基线（独立单段
                                      # 文件，bench_baseline.txt 双段不动）
  python3 tools/bench.py --icount --prove   # B-2：五跑证明（漂移谱 + verdict，
                                      # 观测 only——不写基线不进门禁）

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
# B-2 通道帽：与 vanilla BOOT_TIMEOUT=120 同口径、不放大。实测依据（M 系
# mac，QEMU 11.0.3，2026-10-27 首跑探针）：boot 到 BENCHWAKE 全程 ≈2s——
# sleep=off 下虚拟时钟纯指令驱动，「-icount 慢 2-4x」预期被实测反驳
# （vanilla boot 墙钟大头 = 等虚拟定时器 deadline，-icount 把等待变成指令
# 推进；icount 每-TB 开销被等待消除完全盖过）。帽 120 = ~60x 余量，CI 慢
# runner（TCG 慢 3-10x）仍有 6-20x。帽语义比 B-0 宽：done 标记 = boot 尾
# BENCHWAKE（帽盖全程矩阵），不是 BENCH ok。若未来机器慢到触帽——那是发现
# 不是参数问题（-icount 下 boot 不达是合法发现，帽必须 loud 失败而非放宽）。
BOOT_TIMEOUT_ICOUNT = 120

# B-2 定值参数（设计 §13）：shift=1 = 每指令 2ns 虚拟时钟（CNTVCT 折算
# ≈0.125 cyc/instr）；sleep=off 关实时对齐 = 确定性来源本身（load-bearing
# ——删掉它虚拟时钟重新耦合宿主时钟，通道失效，见 docstring 语义③）。
# **shift 不可参数化**：换 shift = 换量化粒度 = 基线整体作废——故定值内嵌
# 常量、不出 CLI 旗标（--factor 拒绝同因族：L2 形态必须收敛在单一事实源）。
ICOUNT_ARGS = ["-icount", "shift=1,sleep=off"]

# B-2 基线独立文件（§13 证明协议：跨机与跨模式都不可比——与 bench_baseline
# .txt 双段分离；单段裸 CSV 无前缀，独立文件即分段）。
BASELINE_ICOUNT = REPO / "tools" / "bench_baseline_icount.txt"

# L2 紧门禁形态（**单一事实源**——基线文件头注释与设计文档 §13 只做镜像
# 引用，执法点唯一在此常量；防双源漂移 = P6 A-5 基准漂移雷教训）：
#   0.0  = 六项逐值精确相等（五跑证明 verdict=exact 时合法）
#   0.05 = ±5% **双侧带** [base×0.95, base×1.05]（证明得 drift 时的降级档
#          ——确定性通道里向「好」漂移同样 = 指令流变了 = 异常，与 L1 单侧
#          只拦劣化（下方 cyc > old * factor）刻意不同）
# 证明（2026-10-27，M 系列 mac，QEMU 11.0.3）：
#   python3 tools/bench.py --icount --prove
#   → verdict=exact max_dev=0.000%（五跑六项 (iters,cyc) 元组逐值相同——
#     iters 为内核 ELF 冻结常量，元组比对顺带把收割面钉进证明，2026-10-27
#     审查起；「逐字节相同」只对 BENCHWAKE 整行比对成立——那是真字符串比对）
# 重跑得异 verdict 时：改本常量（exact↔pct5 档互换）+ `--icount --update`
# 重立基线 + 本注释同步（漂移须归因——§5 基线 diff 纪律同款）。
#
# —— 证明台账（重证记录，verdict 全 exact 则 TOL 恒 0.0）——
# [2026-10-27 B-2 立档] verdict=exact：0/29157/29127/50/29433/1695（首证）。
# [2026-10-27 K-1 重证] verdict=exact：0/168/33/50/444/1695——buddy 空闲头
#   索引线性扫→位图+摘要两级（K-1 回归修复，根因/机制/验收见
#   docs/基准与压力测试设计.md §13 归因段）：buddy 族三项坍缩为常数
#   （4k 29157→168、64k 29127→33、commit1p 29433→444，commit1p−alloc4k=276
#   与修复前 VMO 层常数逐位同——回归增量全部来自内嵌帧分配的归因再证）；
#   非 buddy 三项（cntvct 0 / handle_evt 50 / rw4k 1695）与旧基线逐值相同
#   = 指令流变化严格收敛在 buddy 路径。分配成本自此与保留前缀 k 奇偶/
#   镜像布局脱钩（「布局跨页 ±28k 翻带」结构性绝迹）。基线同日重立。
# [2026-10-27 K-1 处置刀重证] verdict=exact：0/168/33/50/444/1695——find_free_head
#   加 release 防御守卫（摘要位与非零位图字失同步时 `if word == 0 { continue; }`
#   跳该摘要字，防 tz(0)=64 静默错分配）：debug 构建该比较被 debug_assert 前置
#   证伪、死码消除（反汇编核实 panic_fmt 路径在位）→ 五跑逐值与 K-1 基线全同、
#   **基线零改动**；release codegen 独立核实（crate 级 asm：ldr 后 cbz→次摘要
#   字循环块，守卫在位）。即 debug 通道 loud（断言）/ release 通道纵深（跳字）
#   双形态成立，icount 指令流不受扰。
# [2026-10-31 S-2a 重证] verdict=exact：0/35/80/50/311/1695——KERN_LOCK BKL
#   纪元锁底座段 A（TTAS 原子序进 SVC 入口/dispatch 尾 + 五切走点舞蹈 +
#   计数原子化 A12，`docs/SMP-S2实施设计草案.md`）：非 buddy 三项（cntvct 0 /
#   handle_evt 50 / rw4k 1695）与 S-1 基线（3bf9185 立档 0/78/33/354）逐值
#   相同；buddy 族三项 78→35 / 33→80 / 354→311——三 delta 对称 ±43-47，
#   归因 = 测量窗内 ±1 次 tick 事件的相位迁移（锁指令序改变 boot 后虚拟
#   时钟相位，tick 边界相对测量窗落位翻转，窗内恰含 k 或 k+1 个完整
#   tick；非 buddy 三项窗小/相位无关不受扰）。基线同日重立。
# [2026-10-31 GUI-0 重证] verdict=exact：0/125/31/166/1662/6937——FPSIMD 全量
#   上下文 + 内核 SIMD-free 三层合同（TaskContext 112→640B、switch 无条件
#   q0..q31+FPCR/FPSR、mem*/bcmp 整数覆写，`docs/GUI里程碑规划-2026-10.md`
#   GUI-0）：两项大头 vmo_commit1p 311→1662 / vmo_rw4k 1695→6937（×4-5）＝
#   拷贝路径链 builtins NEON 版→整数 8 字节块循环的设计内代价（TCG 数量级
#   足够、确定性优先，ADR #34 修订）；handle_evt 50→166（+116）≈ 切换点
#   全量保存（16 对 stp/ldp q + mrs/msr ≈ 70 指令/切换）落入唤醒测量窗，
#   幅值与 1-2 次 switch 同量级；buddy 两项 35→125 / 80→31（+90/−49，可正
#   可负）= boot 指令流改变后 tick 相位重掷与内核镜像布局项的混叠（S-2a
#   同族形态，五跑逐值相同即确定性无损）；cntvct_pair 0→0 逐值不动（寄存器
#   直读零切换零拷贝——观测锚不动，佐证收割面无扰动）。BENCHWAKE 五跑逐字节
#   相同（wake 观测面不动）。基线同日重立。
# [2026-10-31 GUI-0 × S-2a 段 B 合并重证] verdict=exact：0/37/82/166/1574/6937
#   ——两线各验全绿后的集成树（a9a75c5 × 5a9a00f+ea53e7c）：vmo_rw4k 6937 与
#   handle_evt 166 精确等于纯 GUI-0 值 = 两改动面正交实证（段 B 锁面不进
#   vmo rw/evt 路径、GUI-0 mem* 面不进锁面）；buddy 族 37/82/1574 = 相位重掷
#   族（37≈基 35、82≈基 80、1574=纯 GUI-0 1662−88 与段 B +181 的混叠）；cntvct
#   0 观测锚不动。BENCHWAKE 五跑逐字节同（max 46250423）。墙钟双段带内零刷
#   （B-0 3/12/30/28/260/753、B-1 sys_vmo_rw4k 2019→2171 = SVC 入口锁可见
#   代价带内合规）。基线随合并树三立。
ICOUNT_TOL = 0.0

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

# B-2：BENCHWAKE 行**非锚定**收割/轮询（B-1/R-1 矩阵巨行中段教训同族）：
# watchdog 内核任务在 G1.5 ok 断言 + 稳态矩阵样本积累后才打（boot 尾），
# 行可嵌矩阵巨行中段——^ 锚定恒漏收。串内 'BENCHWAKE n=' 唯一（矩阵字符
# A-Z/a-z 无此子串）；**done 轮询/缺席诊断/整行收割三处共用 BENCHWAKE_RE
# 全形状**（值域 \d+，R-0 user.rs 打印序冻结：n/min/p50/p90/p99/max/sig/tmo
# 八字段序）——2026-10-27 三镜审查补修：首版轮询用宽前缀 `BENCHWAKE n=`
# 早退而收割要求全形状，轮询恰逢数字段撕裂半行（如 n=210 只落「2」）时
# 宽标记命中、全形状落空 → 「形状不完整」假 FAIL（真因在收割协议自身，
# 诊断文案却把人引去查 user.rs 打印序）；三处共用全形状后撕裂半行结构性
# 不可能命中早退，宽前缀 DONE_ICOUNT_RE 删除。
BENCHWAKE_RE = re.compile(
    r"BENCHWAKE n=\d+ min=\d+ p50=\d+ p90=\d+ p99=\d+ max=\d+ sig=\d+ tmo=\d+")

# B-1 串口行在基线文件里的段前缀（load/save 对称双正则——P6 A-5 基准漂移雷：
# 读写两侧必须同形，否则「双绿」退化成对陈旧基准自证）。
B1_PREFIX = "BENCH1,"


def kernel_elf() -> str:
    out = subprocess.run(
        ["bash", str(REPO / "tools" / "paths.sh"), "kernel"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    return out


def qemu_preflight() -> None:
    """起 QEMU 前跨进程串行守卫（2026-10-27 三镜审查补修——「QEMU 严格单
    实例」从进程内天然串行升格为执法点）：`pgrep -f qemu-system` 非空 =
    另一终端/CI job 的 QEMU 在场（soak/smoke 长跑正属常态）——静默并行第
    二台既违本仓串行纪律，也污染 vanilla/rt 通道的墙钟派生计时（CNTVCT
    走 TCG 实时，宿主竞争 = L1 假红面；icount 数值由虚拟时钟驱动免疫，
    纪律一体适用）。行为 = 轮询等空（2s 粒度——「等空而非并行」铁律的
    工具化）而非立即拒绝；帽 900s 后 loud SystemExit——等一小时级的 soak
    不如让人稍后再跑（无人值守 CI 下超时即失败，不静默并行）。"""
    deadline = time.time() + 900
    notified = False
    while True:
        r = subprocess.run(["pgrep", "-f", "qemu-system"],
                           capture_output=True, text=True)
        if r.returncode == 1:
            return  # pgrep rc=1 = 无匹配 = 场地空
        if r.returncode != 0:
            raise SystemExit(
                f"bench.py: pgrep 异常 rc={r.returncode}"
                f"（stderr: {r.stderr.strip()}）——串行守卫自身失效，拒不起 QEMU")
        if not notified:
            print("bench.py: 检测到在场 QEMU（pgrep -f qemu-system 非空）——"
                  "串行纪律轮询等空中（帽 900s，绝不并行第二台）", file=sys.stderr)
            notified = True
        if time.time() >= deadline:
            pids = " ".join(r.stdout.split())
            raise SystemExit(
                f"bench.py: QEMU 串行守卫等空超时（900s）——在场 pids: {pids}；"
                "长跑（just soak 等）占用期间勿并行 bench，稍后重试")
        time.sleep(2)


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

    qemu_preflight()  # 跨进程串行守卫（2026-10-27 审查补修——见其 docstring）
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
        # start_new_session 使 SIGTERM 只打 QEMU 不打本进程组）。kill 后
        # 补 wait：不等收尸则僵尸暂存进程表，下一跑 qemu_preflight 的
        # pgrep 会误报「在场 QEMU」（prove 背靠背五跑实面）。
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
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
    qemu_preflight()  # 跨进程串行守卫（2026-10-27 审查补修）
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
        # 与 run_once 同款：halt 后不退出，收完即杀（kill 后补 wait 收尸）。
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
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


def run_icount(kernel: str) -> tuple[dict[str, int], dict[str, int], str]:
    """B-2 通道：`-icount shift=1,sleep=off` 裸跑，等 boot 尾 `BENCHWAKE n=`，
    双面收割 → ({name: cyc_per_op}, {name: iters}, BENCHWAKE 整行)。

    独立函数不复用 run_once（run_rt 先例，bench.py 零回归面优先——smoke/
    bench 依赖既有两通道）：与 run_once 的差异五处——①qemu_cmd 追加定值
    ICOUNT_ARGS（boot 时序虚拟时间化，与实时通道不同形态）；②done 标记 =
    BENCHWAKE_RE **全形状**非锚定 search（轮询/缺席诊断/整行收割三处共用
    同一形状——撕裂半行结构性不可能早退，见 BENCHWAKE_RE 注；BENCHWAKE
    严格后于 `BENCH ok n=6`（watchdog 在稳态矩阵样本积累后才打，user.rs
    打印序）——等到 BENCHWAKE 六行 BENCH 必已齐，一次收割双面齐）；③双面
    收割 = 六行 BENCH **行锚定**（门禁面——bench 窗口先于矩阵起跑，行位
    干净，run_once B-0 同款）+ BENCHWAKE 整行**非锚定**（R-0 观测面）；
    ④返回三元组——iters 面（2026-10-27 三镜审查补修）：prove 的比对面
    从逐名 cyc 升 (iters,cyc) 元组（iters = 内核 ELF 冻结常量，同一会话
    内结构性不变；一并比对 = 把收割面（正则分组）也钉进证明，防「只比
    cyc 掩盖收割回退」）——门禁/立基线侧仍只用 cyc（iters 列由
    _iter_table 静态镜像写盘，两源一致性由 prove 元组比对背书）；
    ⑤qemu_preflight 跨进程串行守卫。

    -icount 下 boot 不达 BENCHWAKE = 合法发现（§13 语义② boot 时序全变）
    ——本函数只 loud 上报（SystemExit 带串口尾），绝不放宽帽硬凑绿。
    """
    qemu_cmd = [
        "qemu-system-aarch64", "-machine", "virt", "-cpu", "cortex-a72",
        "-m", "128M", "-nographic", "-kernel", kernel,
        # 定值注见 ICOUNT_ARGS（shift 不可参数化）。
        *ICOUNT_ARGS,
    ]
    with tempfile.NamedTemporaryFile(
        prefix="berryos-bench-icount.", suffix=".log", delete=False
    ) as tf:
        log_path = tf.name
    qemu_preflight()  # 跨进程串行守卫（2026-10-27 审查补修）
    t0 = time.time()
    proc = subprocess.Popen(
        qemu_cmd,
        stdin=subprocess.DEVNULL, stdout=open(log_path, "w"),
        stderr=subprocess.STDOUT, start_new_session=True,
    )
    try:
        deadline = time.time() + BOOT_TIMEOUT_ICOUNT
        while time.time() < deadline:
            if proc.poll() is not None:
                break  # QEMU 提前退出（崩溃）——带着现有日志走收割失败路径
            text = Path(log_path).read_text(encoding="utf-8", errors="replace")
            if BENCHWAKE_RE.search(text):
                break
            time.sleep(1)
        text = Path(log_path).read_text(encoding="utf-8", errors="replace")
    finally:
        # 与 run_once/run_rt 同款：halt 后不退出，收完即杀（kill 后补 wait）。
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        Path(log_path).unlink(missing_ok=True)

    if BENCHWAKE_RE.search(text) is None:
        raise SystemExit(
            f"bench.py: BENCHWAKE 行 {BOOT_TIMEOUT_ICOUNT}s 未出现（-icount 下 "
            f"boot 不达 = 合法发现，如实上报勿放宽帽——串口尾部："
            f"{' | '.join(text.splitlines()[-5:])})")
    print(f"[B-2(icount)] boot→BENCHWAKE {time.time() - t0:.1f}s wall"
          f"（帽 {BOOT_TIMEOUT_ICOUNT}s）")
    # 观测面：BENCHWAKE 整行（R-0 依赖注记——wake 延迟确定性观测，不动门禁）。
    wake_line = BENCHWAKE_RE.search(text).group(0)
    # 门禁面：六行 BENCH 行锚定收割 + BENCH ok n= 一致性校验（镜像 run_once
    # 的 B-0 分支）。BENCHWAKE 已在而 BENCH ok 缺席 = 「严格后于」时序假设
    # 被打破（user.rs 打印序变化？）——loud 不容静默降级。
    metrics: dict[str, int] = {}
    iters_seen: dict[str, int] = {}
    done_n = None
    for line in text.splitlines():
        m = BENCH_RE.match(line.strip())
        if m:
            iters_seen[m.group(1)] = int(m.group(2))
            metrics[m.group(1)] = int(m.group(3))
        d = DONE_RE.match(line.strip())
        if d:
            done_n = int(d.group(1))
    if done_n is None:
        raise SystemExit("bench.py: BENCHWAKE 已在而 BENCH ok 行缺席——时序假设"
                         "「BENCHWAKE 严格后于 BENCH ok」被打破，门禁面不可收割")
    if len(metrics) != done_n:
        raise SystemExit(
            f"bench.py: BENCH 行数 {len(metrics)} ≠ BENCH ok n={done_n}"
            "（内核侧计数与出表不一致——改 bench.rs 后同步 N_METRICS）")
    return metrics, iters_seen, wake_line


PROVE_RUNS = 5  # 五跑（§13 证明协议定值——「五跑逐值相同 → 开 L2」）


def prove_icount(kernel: str) -> None:
    """B-2 五跑证明（§13 L2 开门前置，义务在提案方）：单进程串行五次
    run_icount（每跑内建 SIGTERM+wait 纪律 + qemu_preflight 跨进程串行
    守卫——进程内五跑天然串行，进程间由 pgrep 执法，2026-10-27 审查补修），
    六项 **(iters,cyc) 元组逐值比对**出漂移谱（机器可读 CSV，公约 4）。

    比对面注记（2026-10-27 审查补修）：iters = 内核 ELF 冻结常量，同一
    证明会话内结构性不变——元组比对的价值不在多抓数值漂移，而在把**收割
    面**（正则分组/行解析）一并钉进证明，防「只比 cyc 掩盖收割回退」；
    iters 在会话内漂移 = 收割面/ELF 变化，SystemExit loud 而非记入漂移谱
    （非合法数值漂移结局）。「逐字节相同」的表述只属于 BENCHWAKE 整行
    字符串比对——六项 BENCH 是逐值（元组）比对。

    **观测 only**：不写基线、不进门禁（--prove --update 拒绝）。漂移是
    合法结局——五跑 boot 都达标记即 exit 0（run_icount 达标记失败自会
    SystemExit）；verdict 只陈述事实，门禁形态由人按 verdict 烘焙进
    ICOUNT_TOL（常量注释即证明台账，可重验）。BENCHWAKE 五跑行顺带比对
    = R-0 依赖注记的观测面（§13「依赖」段：-icount 下 wake 延迟确定性
    顺带验证）——**不动 verdict**，门禁面恒 = 六行 BENCH。
    """
    all_runs: list[dict[str, tuple[int, int]]] = []
    wakes: list[str] = []
    for i in range(1, PROVE_RUNS + 1):
        metrics, iters, wake = run_icount(kernel)
        all_runs.append({n: (iters[n], metrics[n]) for n in metrics})
        wakes.append(wake)
        print(f"bench.py: prove 跑 {i}/{PROVE_RUNS} 收割 {len(metrics)} 项")
    for i, r in enumerate(all_runs[1:], start=2):
        if set(r) != set(all_runs[0]):
            raise SystemExit(
                f"bench.py: prove 跑 {i} 基准项集合与跑 1 不同"
                f"（{sorted(set(r) ^ set(all_runs[0]))}）——收割面破碎，非数值漂移")
    exact = True
    worst = 0.0
    for name in sorted(all_runs[0]):
        pairs = [r[name] for r in all_runs]
        vals = [c for (_it, c) in pairs]
        if len(set(pairs)) != 1 and len(set(vals)) == 1:
            raise SystemExit(
                f"bench.py: prove 项 {name} 的 iters 漂移 "
                f"{sorted({it for (it, _c) in pairs})}（cyc 恒 {vals[0]}）——"
                f"iters = 内核 ELF 冻结常量，会话内漂移 = 收割面/ELF 变化，"
                f"非合法数值漂移结局")
        lo = min(vals)
        # lo=0 面（cntvct_pair 量化语义——0.125 cyc/instr 粒度下 <1 cyc/op
        # 整除落地 = 0，基线头注同款）：五值全 0 = 零漂移 dev=0；0 混非 0 =
        # 无界漂移（量化粒度级的指令流变化，比任何有限百分比都大——inf）。
        if lo == 0:
            dev = 0.0 if max(vals) == 0 else float("inf")
        else:
            dev = (max(vals) - lo) * 100.0 / lo
        exact = exact and (len(set(pairs)) == 1)
        worst = max(worst, dev)
        print(f"ICOUNT,{name},{','.join(map(str, vals))},max_dev={dev:.3f}%")
    print(f"ICOUNT verdict={'exact' if exact else 'drift'} max_dev={worst:.3f}%")
    # R-0 依赖注记的观测面（§13「依赖」段）：五跑 BENCHWAKE 整行比对。
    if len(set(wakes)) == 1:
        print(f"bench.py: BENCHWAKE 五跑逐字节相同（wake 路径确定性佐证，"
              f"观测面不动 verdict）：{wakes[0]}")
    else:
        print("bench.py: BENCHWAKE 五跑存在漂移（观测发现——上报归因，"
              "不动 verdict、不进门禁）：")
        for i, w in enumerate(wakes, start=1):
            print(f"  run{i}: {w}")
    print(f"bench.py: prove 观测 only——L2 门禁形态 = bench.py ICOUNT_TOL 单源"
          f"常量（当前 {ICOUNT_TOL}），按本 verdict 烘焙/维持")


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


def load_baseline_icount() -> dict[str, int]:
    """icount 基线单段解析：独立文件裸 CSV `name,iters,cyc`（与 bench_baseline
    .txt 的 B-0 段同形——独立文件即分段，无需前缀）。文件缺席 = 空 dict
    （对拍跳过合法——跨机首跑自立，不算过门禁）。"""
    base: dict[str, int] = {}
    if not BASELINE_ICOUNT.exists():
        return base
    for raw in BASELINE_ICOUNT.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = BENCH_RE.match(f"BENCH,{line}")
        if m:
            base[m.group(1)] = int(m.group(3))
    return base


def save_baseline_icount(metrics: dict[str, int]) -> None:
    """刷新 icount 基线**整个文件**（单段独立文件）——绝不经 save_baseline：
    其双段逻辑会误写 bench_baseline.txt（--rt 拒绝 --update 的理由同源）。
    头注释五件 = §13 语义①/跨机不可比（§2 铁律 1——非公约 5：公约 5 是
    TCG 半率单位纪律，跨机绝对值不可比的成文位置是 §2 铁律 1，2026-10-27
    审查勘误）/刷新命令/ICOUNT_TOL 镜像/cntvct_pair 量化语义（与 load 侧
    严格同形——P6 A-5 基准漂移雷：读写两侧不同形则「双绿」退化成对陈旧
    基准自证）。"""
    iters = _iter_table()  # 与 B-0 同六项（bench.rs 定值镜像——分文件不分项）
    lines = [
        "# BerryOS -icount 确定性通道基线（B-2，`python3 tools/bench.py --icount`）。",
        "# 口径 = CNTVCT 周期数在 -icount shift=1 虚拟时钟下的**指令数代理**",
        "# （每指令 2ns，折算 CNTVCT ≈0.125 cyc/instr）——与 tools/bench_baseline.txt",
        "# （TCG 实时通道）**跨模式不可比**；跨机/跨 QEMU 版本绝对值亦不可比",
        "# （§2 铁律 1）——CI 与他人 clone 首跑 `python3 tools/bench.py --icount --update` 自立",
        "# （无基线 = 只出表，不算过门禁）。",
        "# 刷新：python3 tools/bench.py --icount --update（整文件单段）。",
        "# L2 门禁形态单一事实源 = tools/bench.py 的 ICOUNT_TOL 常量（本文件只做",
        "# 镜像引用——防双源漂移）。",
        "# cntvct_pair=0 是合法值非故障：4096 次循环骨架在 0.125 cyc/instr 量化",
        "# 粒度下 <1 cyc/op 整除落地 = 0（「净成本 = 其余项 − cntvct_pair」心算",
        "# 口径仍成立——0 地板）。",
    ]
    for name, cyc in sorted(metrics.items()):
        lines.append(f"{name},{iters.get(name, 0)},{cyc}")
    BASELINE_ICOUNT.write_text("\n".join(lines) + "\n")


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
    ap.add_argument("--icount", action="store_true",
                    help="B-2 通道：-icount shift=1,sleep=off 确定性裸跑，收割六行"
                         " BENCH 做 L2 紧门禁对拍（基线 bench_baseline_icount.txt"
                         " 独立文件）+ BENCHWAKE 观测行")
    ap.add_argument("--prove", action="store_true",
                    help="五跑证明（--icount 专属：漂移谱 + verdict，观测 only"
                         "——不写基线不进门禁）")
    ap.add_argument("--kernel", default=None, help="内核 ELF 路径（默认 paths.sh 解析）")
    args = ap.parse_args()

    if args.rt and (args.update or args.el0 or args.factor is not None
                    or args.icount or args.prove):
        # RT 无基线段（设计 §10 只出表）——误触 save_baseline 会按双段前缀
        # 逻辑误写 bench_baseline.txt；--el0/--icount 同样不进 --rt 早退
        # 分支：静默忽略 = 用户要的通道/门禁凭空蒸发（--el0 --rt 实际只跑
        # RT、B-1 静默缺席且退出码 0——三镜审查实抓）。组合一律拒绝
        # （B-2 起 --icount/--prove 入拒绝集——prove 只对 --icount 立法）。
        raise SystemExit(
            "bench.py: --rt 独立通道（只出表无基线）——不与 --update/--el0/"
            "--factor/--icount/--prove 组合（--update 误写基线双段；--el0/--icount "
            "通道会被静默丢弃——一进程一种 QEMU 形态；--factor 在 RT 无门禁面；"
            "--prove 只对 --icount 立法）")

    # --- B-2 --icount 专属校验（§13 参数矩阵；拒绝消息逐组合说因，教学体
    # 同上——「不静默忽略」先例）：单跑/--update/--prove 三形态合法，
    # --el0/--rt/--factor 混入一律拒绝。
    if args.icount and args.el0:
        raise SystemExit(
            "bench.py: --icount 不与 --el0 组合——通道混跑（一进程一种 QEMU "
            "形态：icount 裸跑 vs el0 rootfs fw_cfg）；混入 mode 分支会让其中"
            "一条静默蒸发（--rt 拒绝先例同因）")
    if args.icount and args.factor is not None:
        raise SystemExit(
            "bench.py: --icount 不与 --factor 组合——L2 门禁形态 = 烘焙常量 "
            "ICOUNT_TOL（bench.py 单源：精确相等或 ±5% 双侧带），可调因子会把 "
            "L2 静默降级回 L1 单侧语义")
    if args.prove and not args.icount:
        raise SystemExit(
            "bench.py: --prove 只对 --icount 立法（§13 证明协议——L2 开门"
            "前置）；其余通道无「五跑逐字节稳定」义务，勿泛化")
    if args.prove and args.update:
        raise SystemExit(
            "bench.py: --prove 观测 only 不写基线——证明是 L2 开门前置不是立档"
            "动作；立/刷基线 = python3 tools/bench.py --icount --update")

    kernel = args.kernel or kernel_elf()
    if args.rt:
        rows = run_rt(kernel)
        print(f"[R-1(rt)] {'delta':>12} {'p50':>10} {'p99':>10} {'max':>10}")
        for delta, p50, p99, mx in rows:
            print(f"{'':<8} {delta:>12} {p50:>10} {p99:>10} {mx:>10}")
        print("bench.py: R-1 只出表不对拍（oversleep>=0 严格断言在内核侧 panic 执法"
              "（L0）；分布数值统计归统计——公约 2）")
        return
    if args.icount:
        if args.prove:
            prove_icount(kernel)
            return
        metrics, _iters, wake = run_icount(kernel)
        print(f"[B-2(icount)] {'name':<16} {'cyc/op':>10}")
        for name, cyc in sorted(metrics.items()):
            print(f"{'':<8} {name:<16} {cyc:>10}")
        print(f"[B-2(icount)] BENCHWAKE 观测行（R-0 依赖注记——观测面不进门禁）："
              f"{wake}")
        if args.update:
            save_baseline_icount(metrics)
            print(f"bench.py: icount 基线已刷新 → "
                  f"{BASELINE_ICOUNT.relative_to(REPO)}")
            return
        base = load_baseline_icount()
        if not base:
            print("bench.py: icount 基线缺席（首跑/跨机首跑）——仅出表；"
                  "`python3 tools/bench.py --icount --update` 立基线")
            return
        # L2 紧门禁：双侧带 [old×(1-tol), old×(1+tol)]——tol=0.0 退化为逐值
        # 精确相等；确定性通道双向漂移都是指令流变化，变快同样 FAIL（与 L1
        # 单侧只拦劣化刻意不同，见 ICOUNT_TOL 注）。
        bad = []
        for name, cyc in sorted(metrics.items()):
            old = base.get(name)
            if old is None:
                print(f"bench.py: 新基准项 {name}（基线缺席，不过门禁）")
                continue
            if old == 0:
                # lo=0 守卫（prove_icount 同型面——2026-10-27 三镜审查补修：
                # cntvct_pair 基线 0 在册，未来指令流变化把它量化翻到 ≥1 时
                # `(cyc-0)*100.0/0` 裸除法 ZeroDivisionError 会顶替设计好的
                # FAIL 诊断行；prove 侧首版实踩已修，门禁孪生分支同步）：
                # 0→0 = 恰等过门；0→非 0 = 量化粒度级无界漂移（inf——零混
                # 非零比任何有限百分比大，与 prove 的零语义同口径）。
                if cyc != 0:
                    bad.append(f"{name}: 0 → {cyc}（+inf%，出 L2 界 [0, 0]"
                               f"——基线 0 出非零 = 量化粒度级无界漂移，"
                               f"确定性通道双向漂移都是指令流变化）")
                continue
            lo = old * (1.0 - ICOUNT_TOL)
            hi = old * (1.0 + ICOUNT_TOL)
            if not lo <= cyc <= hi:
                dev = (cyc - old) * 100.0 / old
                bad.append(f"{name}: {old} → {cyc}（{dev:+.2f}%，出 L2 界 "
                           f"[{lo:g}, {hi:g}]——确定性通道双向漂移都是指令流"
                           f"变化，变快同样是异常）")
        missing = sorted(set(base) - set(metrics))
        if bad or missing:
            for b in bad:
                print(f"bench.py: FAIL {b}", file=sys.stderr)
            for m in missing:
                print(f"bench.py: FAIL 基准项 {m} 从输出消失"
                      "（改基准源后须 --icount --update）", file=sys.stderr)
            raise SystemExit(1)
        print(f"bench.py: L2 对拍过（ICOUNT_TOL={ICOUNT_TOL}，"
              f"{len(metrics)} 项全在界内）")
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
