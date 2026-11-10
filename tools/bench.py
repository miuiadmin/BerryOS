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
- --lat（G3-4 bench-lat）：-smp 2 裸跑 + `-fw_cfg name=opt/berry/lat,string=1`
  （门控键同 rt 形）等 `LATIPC ok n=32`，非锚定收割 `BENCHLAT,lane=` 行
  （G3-4a 起 IPC 路〔内核任务对，wake_matching 直驱〕在册；SGI 路〔EL0
  用户对 spread→CPU1〕G3-4b 扩入——lane 收割面按在册路断言）。R-1 同族
  **只出表无基线**（wake→run 段补盲 = 观测面；醒因/唤醒数/负延迟严格断言
  在内核侧 L0；跨机绝对值不可比承 B-2 纪律）。行嵌矩阵流中段（prio 8 抢
  占打印）——BENCHRT 同款非锚定收割。
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
- --trace-dual（G3-4b）：`可观测性设计.md` :52「trace 开/关两套基准都过」
  过关要件的载体——--icount 同形态两臂（默认 vs `-fw_cfg opt/berry/trace=0`
  记录面关）背靠背：①关-开行差帽 TRACE_CAP（单一事实源常量——首标定
  2026-11-05 = 0.0 **结构性零**：bench 六窗前置调度器纪元，遥测钩子窗口内
  不发射；任一非零差 = 门泄漏〔报警〕或 bench 面迁纪元〔合法演进须重标定〕，
  见常量注）；②两臂各对拍自家家族基线（L2 同 ICOUNT_TOL；off 家族独立文件
  bench_baseline_icount_traceoff.txt——基线双份入库，草案 §4）；③`--update`
  刷双份（帽不过不许刷）。off 臂 done 标记 = `TRACE off ok`（BENCHWAKE 结构性
  缺席——直方图零样本，watchdog 断言面同让位）。

用法（仓库根）：
  python3 tools/bench.py             # B-0：跑一次 + 对拍基线（无基线 = 只出表）
  python3 tools/bench.py --update    # B-0：跑一次 + 刷新基线 B-0 段（B-1 段保留）
  python3 tools/bench.py --factor 5  # 放宽因子（CI 慢机场景）
  python3 tools/bench.py --el0       # B-1：rootfs 形态跑一次 + 对拍 B-1 段
  python3 tools/bench.py --el0 --update  # B-1：跑一次 + 刷新 B-1 段（B-0 段保留）
  python3 tools/bench.py --rt        # R-1：裸跑（fw_cfg 门控 rt_probe）出三档表
  python3 tools/bench.py --lat       # G3-4 bench-lat：-smp 2（fw_cfg 门控 lat 对）
                                      # wake→run 出 lane 表（IPC 路在册）
  python3 tools/bench.py --dpg       # G4-3 D/E 刀 + X-4/X-5 exec 面：dpgb / dpge+dpgx+x5 变体卷双 boot
                                      #（D 首触延迟出表 + E footprint 差分断言）
  python3 tools/bench.py --icount    # B-2：-icount 确定性跑一次 + L2 对拍
                                      #（无基线 = 只出表 + 提示 --update）
  python3 tools/bench.py --icount --update  # B-2：立/刷 icount 基线（独立单段
                                      # 文件，bench_baseline.txt 双段不动）
  python3 tools/bench.py --icount --prove   # B-2：五跑证明（漂移谱 + verdict，
                                      # 观测 only——不写基线不进门禁）
  python3 tools/bench.py --trace-dual # G3-4b：trace 开/关双跑（--icount 形态
                                      # 两臂：默认 vs opt/berry/trace=0）+ 关-
                                      # 开行差帽 TRACE_CAP + 双家族基线对拍
  python3 tools/bench.py --trace-dual --update  # 立刷双份基线（on 家族文件
                                      # bench_baseline_icount.txt + off 家族
                                      # bench_baseline_icount_traceoff.txt）

退出码：0 = 过/仅出表；1 = 劣化超因子或收割不齐（可作 CI 步骤）。
"""

import argparse
import hashlib
import os
import re
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
BASELINE = REPO / "tools" / "bench_baseline.txt"
# 回执落盘目录（f4 F1/F6 补修）：icount/trace-dual/lat 家族每跑原始串口 +
# 收割面归档——此前 tempfile 每跑 unlink = 工具结构性不留档，数值主张全凭
# 转述无工件背书。out/ 已 gitignore（机器持久、不入版本库），证据档引路径。
RECEIPT_DIR = REPO / "out" / "bench-receipts"


def write_receipt(kind: str, serial_text: str, summary: str) -> Path:
    """icount/trace-dual/lat 家族回执：summary（收割面）+ 原始串口全文落
    out/bench-receipts/，stdout 同步打路径行 = 转录自身携带工件指针。"""
    RECEIPT_DIR.mkdir(parents=True, exist_ok=True)
    ts = time.strftime("%Y%m%d-%H%M%S")
    path = RECEIPT_DIR / f"{kind}-{ts}-{os.getpid()}.log"
    # K-DE2：serial 段内容指纹——回执跨树 cp 后 inode birth ≠ 文件名 ts
    # （cp 造新 inode），元数据链断时以 sha256 对拷贝件可验内容同源。
    serial_sha = hashlib.sha256(serial_text.encode("utf-8")).hexdigest()
    path.write_text(
        f"bench.py receipt kind={kind} ts={ts} pid={os.getpid()}"
        f" argv={' '.join(sys.argv)}\nserial_sha256={serial_sha}"
        f"\n--- summary ---\n{summary}"
        f"\n--- serial ---\n{serial_text}",
        encoding="utf-8",
    )
    print(f"bench.py: 回执落盘 {path.relative_to(REPO)}")
    return path
ROOTFS = REPO / "rootfs.cpio"
BOOT_TIMEOUT = 120  # 秒：boot 到 BENCH ok 的帽（与 smoke DEADLINE_BOOT 同口径）
# B-1 通道帽：rootfs 走查 ~90s + bench1 spawn/SVC 链 + 稳态矩阵尾段——与
# smoke DEADLINE_ROOTFS=300s 同口径（P6 A-6 RPC 成本模型，勿拿 syscall
# cycle 外推）。
BOOT_TIMEOUT_EL0 = 300
# R-1 通道帽（设计 §10 定帽）：boot 头 ~10-20s + rt_probe 三档
# 32×(50+200+800)ms ≈ 34s（TCG 半率）——3x 余量。
BOOT_TIMEOUT_RT = 120
# G3-4 lat 通道帽（f4 F5 勘正）：规则/08 S-1③ 冻结律「`-smp 2` 线一切时限
# 帽 ×2 起步」——本通道全脚本唯一 -smp 2 线，旧 120 未放大 = 规则抵触；
# 且 SGI 路 boot 实测 ≈10s（g3-4a 期「≈2s」为 IPC 单路旧估算，已废），
# 内核侧等位预算（¼ slice × LATSGI_POLL_MAX ≈ 100s guest 钟，TCG 半率折
# 墙钟更长）远超旧帽——宿主争用（§A128 同因）下 python 先杀 = 假超时红。
# 240 = ×2 律下限；超帽仍 loud 上报不硬凑绿。
BOOT_TIMEOUT_LAT = 240
# G4-3 D 刀帽（--dpg D 臂）：rootfs 走查 ~60-90s + dpg_bench 32 供给（TCG
# 弹性）+ 余量——300s = ~3x。
DPG_BOOT_TIMEOUT_D = 600
# G4-3 E 刀帽（--dpg E 臂）：boot ~60s + **三帧**（X-4 起 DPGE 帧后叠 DPGX
# exec 差分帧；X-5 起再叠 X5 现役 bin 差分帧：各预滚 340s + 4×130s 窗）
# + spawn/poll 余量 ≈ 42min guest 钟（TCG guest 钟 = 墙钟，§A8；宿主争用面
# §A128 只多不少）——3600s ≈ 1.4x 余量；超帽 loud 上报不硬凑绿。
DPG_BOOT_TIMEOUT_E = 3600
# X-6 页/s 直测面（仪器刀 0——X6批供提速设计-v1.0 §5）：guest 观测行
# `X6 pg=N sw=M`（pg = PAGER_SUPPLIED 累计供页〔rc==0 增〕、sw = PAGER_SWEEPS
# 同源累计圈数；十进制单行、行不带钟）——svcmgr boot 验证环各 demand bin
# （hello/hexyl/boot 尾——83 lane 侦察校准三点形；grep 不在 boot 环，仅
# x5_foot 语境）验证成功后各单发，恰 3 行；x5/dpg 相位零发射
# （kick 行与 dpgsoak 面不动）。**观测线非门标记**：不进 markers.sh /
# rootfs 51（kick 行同例——bench 侧消费）；词形契约 = X6 设计稿 §5 冻结。
# 钟 = 宿主侧轮询新字节到达时戳（分辨率 ±X6_POLL_S；X6 §5「页/s 的钟 =
# 宿主侧 marker 到达时戳」——34 背书钉进验收节）。
X6_RE = re.compile(r"^X6 pg=(\d+) sw=(\d+)\s*$", re.M)
X6_POLL_S = 0.2
# hexyl 窗（首窗 = stdhello→hexyl 差分，主装载窗）页/s 常驻回归门。None =
# 观测不动判——X-6 刀未落内核照跑照绿（零漂移）。定标法（首净窗实测、
# 不拍阈值）：② 落地首跑读 [X-6(0)] hexyl 实测率 R → 本常量写 R×0.7
# （TCG 方差带）；收刀验收另口一次性判 R ≥ 基线(~6 页/s)×3（§5；kill
# criteria <2× 撤刀）。
X6_HEXYL_PGS_FLOOR: float | None = 7.98
# 定标回执：2026-11-08 首净窗（HEAD=89703cb 硬化核，场净核验——E 臂守卫
# 通过即 boot 时零在场 QEMU）R=11.4pg/s（102 页/8.92s，sw+77；receipt
# dpg-20261108-130150-49261）×0.7。终判另裁口径：R/基线 6pg/s ≈1.9×——
# 低于 kill 线 2× 与 ② 门 ×3（18），落设计稿诚实带宽 p≈0.5 带（1.4-2×）。
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

# G3-4b trace 双跑 off 臂基线（草案 §4「基线双份入库——bench_baseline 家族
# 扩」）：与 BASELINE_ICOUNT 同形裸 CSV、同 ELF 同 bench 面，仅记录面关
#（opt/berry/trace=0）——**三态不可比**（on/off 跨配置、icount/实时跨模式、
# 跨机），独立文件即分段（load/save 与 on 家族严格同形——P6 A-5 双源漂移雷）。
BASELINE_ICOUNT_OFF = REPO / "tools" / "bench_baseline_icount_traceoff.txt"

# G3-4b trace 双跑：off 臂 done 标记（user.rs watchdog 的 BENCHWAKE 替代位
# ——记录面关时环/直方图断言对象不在场，执法同让位；含空格/括号 = 矩阵
# 字符流〔A-Z/a-z 无空格〕结构性不可伪命中；全形状正则三处共用——
# BENCHWAKE_RE 同纪律，撕裂半行不可能早退）。
TRACE_OFF_DONE_RE = re.compile(r"TRACE off ok \(record face closed\)")

# G3-4b trace 劣化帽（单一事实源——(on−off) 行差上界 = off 值的此倍率，
# 草案 §4「帽 = 关基线的 x%，x 实施刀首标定」）。**首标定（2026-11-05，
# M 系 mac，QEMU 11.x）= 0.0——全六行关/开逐值恰等**，且这是**结构性零**
# 非「量不到」：bench::run()（main.rs boot 序）前置于 berry_sched::init()
# ——bench 六窗全在调度器存在之前的串行 boot 纪元执行，遥测钩子（切换环/
# 唤醒环/直方图，经 set_switch_hook/set_wake_hook 注册后才可能发射）在
# 窗口内**结构性不可能发射**（P-1 勘误的机制面在此反向承重）。故帽收敛为
# ④' 控制列断言的**全行推广**：任一行非零差 = 二选一——①trace 门泄漏进
# 前调度器路径（gate 面 bug，L0 级报警）②bench 面迁入调度器纪元（合法
# 演进——届时遥测足迹真入窗、行分类〔控制/混叠/机制〕须重立，本帽过时
# 须按新形态重标定，注释与本常量同步）。off 臂五跑逐值相同（2026-11-05
# 证明，见 notes.md 台账）= off 家族 L2 资格同源。
TRACE_CAP = 0.0

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
# [2026-11-03 S-4 4-2 重证] verdict=exact：0/37/82/166/1574/6937——per-AS PT
#   锁 + copy 链同窗锁 + 十一号 TABLE 降档落地树：唯一漂移 frame_alloc64k
#   37→82（镜像尾新增 PT_LOCKS 64KiB BSS 挪帧分配器起点 → 64k 连扫边界位形
#   变；4k 单页 37 不变佐证 = 纯布局项非锁开销——锁面在 vmar/copy 路径不进
#   buddy 扫描）；vmo_rw4k 6937 精确不动 = TABLE 降档对 copy 链 icount 面
#   零成本实证（TABLE 位集不含 vmo_rw 的 RQ 位）。BENCHWAKE 五跑逐字节同
#   （max 46253280）。基线随 4-2 树四立。
# [2026-11-03 S-4 4-3 重证] verdict=exact：0/216/37/166/1753/6937——ASID 回卷
#   quiesce（ROLL_GEN/QUIESCED_GEN 代数对 + klock_enter 直调族改道 SVC_ALL）
#   落地树三漂移归因 = 镜像布局挪 buddy 起始拓扑（十六续同族）：alloc4k
#   37→216 与 commit1p 1574→1753 **+179≡+179 1:1 纯继承**（commit 内含一次
#   4k 分配 + 清零，分配器路径自身指令流不变）；alloc64k 82→37 恰回 4-2 前
#   值（连扫边界位形再变——反向漂移排除「envelope 真开销」假设：加锁只会变
#   慢）；非分配器面三项（cntvct_pair 0 / handle_evt 166 / vmo_rw4k 6937）
#   逐指令不动 = 直调改道不进 icount bench 面。BENCHWAKE 五跑逐字节同
#   （n=210 min=1497 p50=4096 p90=32768 p99=33554432 max=46253511）。基线
#   随 4-3 树五立。
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

# G3-4 bench-lat（wake→run 段补盲，草案 §4）。收割同 BENCHRT 族**不做行
# 锚定**：lane 行由 prio 8 sleeper/内核 waker 抢占矩阵打印流打出（首跑
# 实录：行嵌矩阵流中段）。全形状含 lane 分组（ipc|sgi 两路在册：ipc =
# 内核任务对同核直切段〔G3-4a〕、sgi = EL0 用户对跨核段〔G3-4b〕）；
# n=32 字面 = task_lat_slp/task_latsgi_wak LAT_SAMPLES 镜像（wire 面冻结
# ——内核侧 const assert 同步执法）。done 轮询/缺席诊断/整行收割三处
# 共用：DONE 按路分记（每路各自的完成标记 LATIPC/LATSGI ok——RT ok
# 同形，**须两路齐**才收）；lane 行与标记行字面互非子串。
BENCHLAT_RE = re.compile(
    r"BENCHLAT,lane=(ipc|sgi),n=(\d+),min=(\d+),p50=(\d+),p90=(\d+),p99=(\d+),max=(\d+)")
# 每路完成标记前缀（lane → 标记；新增路只扩本表与 LAT_LANES）。
LAT_DONE_MARKERS = {"ipc": "LATIPC", "sgi": "LATSGI"}
DONE_LAT_RE = re.compile(r"(LATIPC|LATSGI) ok n=(\d+)")
LAT_LANES = ("ipc", "sgi")  # 在册路（G3-4a ipc + G3-4b sgi）
LAT_N = 32            # 每路样本数（kernel LAT_SAMPLES 定值镜像）

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


# BerryOS QEMU 签名（串行守卫匹配面，2026-11-04 收窄）：aarch64 全 launcher
# （bench/run/smoke/soak/rootfs/EFI）恒带 `-machine virt`；x86 最小集无
# -machine 旗但恒带 `-kernel`。二者并集 = 本仓 QEMU 全谱；宿主他系模拟器
# （Android emulator/AVD：`qemu-system-aarch64-headless -avd …`，两签名皆
# 无）不再误伤——2026-11-04 实案：用户 Android 模拟器在场，裸 `qemu-system`
# 模式令 icount 空等 900s 超时（icount 走虚拟时钟，宿主模拟器在场本就无
# 污染面）。守卫立法面不变：只管「本仓 QEMU 单实例」纪律；墙钟通道的
# 他源宿主负载（编译/他系模拟器）从来不在守卫射程内。
QEMU_GUARD_PATTERN = r"qemu-system.*(-machine virt|-kernel)"


def qemu_preflight() -> None:
    """起 QEMU 前跨进程串行守卫（2026-10-27 三镜审查补修——「QEMU 严格单
    实例」从进程内天然串行升格为执法点）：`pgrep -f QEMU_GUARD_PATTERN`
    非空 = 另一终端/CI job 的**本仓** QEMU 在场（soak/smoke 长跑正属常态；
    Android/AVD 他系模拟器不在此面——见 QEMU_GUARD_PATTERN 注）——静默并行
    第二台既违本仓串行纪律，也污染 vanilla/rt 通道的墙钟派生计时（CNTVCT
    走 TCG 实时，宿主竞争 = L1 假红面；icount 数值由虚拟时钟驱动免疫，
    纪律一体适用）。行为 = 轮询等空（2s 粒度——「等空而非并行」铁律的
    工具化）而非立即拒绝；帽 900s 后 loud SystemExit——等一小时级的 soak
    不如让人稍后再跑（无人值守 CI 下超时即失败，不静默并行）。"""
    deadline = time.time() + 900
    notified = False
    while True:
        r = subprocess.run(["pgrep", "-f", QEMU_GUARD_PATTERN],
                           capture_output=True, text=True)
        if r.returncode == 1:
            return  # pgrep rc=1 = 无匹配 = 场地空
        if r.returncode != 0:
            raise SystemExit(
                f"bench.py: pgrep 异常 rc={r.returncode}"
                f"（stderr: {r.stderr.strip()}）——串行守卫自身失效，拒不起 QEMU")
        if not notified:
            print("bench.py: 检测到在场 QEMU（pgrep -f QEMU_GUARD_PATTERN 非空"
                  "——本仓签名，Android/AVD 不误伤）——"
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


def run_lat(kernel: str) -> dict[str, tuple[int, ...]]:
    """G3-4 bench-lat 通道：-smp 2 裸跑（+fw_cfg 具名项 opt/berry/lat 门控
    lat 任务对 spawn——IPC 路内核对 + SGI 路 EL0 舰队）等 **LAT_LANES 全路
    完成标记齐**（每路 `LATxxx ok n=32`，SGI 路舰队 spawn 较慢后到），
    非锚定收割 BENCHLAT lane 行 → {lane: (n, min, p50, p90, p99, max)}。

    run_rt 同款独立函数不复用 run_once（收割面/一致性校验不同构，零回归面
    优先）。一致性 = 恰 LAT_LANES 在册路各一行、n 逐路 == LAT_N、每路
    完成标记恰一个且 n == LAT_N（改 task_lat_* 后同步）。
    """
    qemu_cmd = [
        "qemu-system-aarch64", "-machine", "virt", "-cpu", "cortex-a72",
        "-m", "128M", "-nographic", "-kernel", kernel,
        # IPC 路 = 同核对（place 内核任务恒 CPU0）；-smp 2 = SGI 路（G3-4b）
        # 的承载形态先行钉板 + 副核 park 干扰面从首跑就在场（跨核空载真实
        # 环境，不是理想单核——G3 阶段对照口径一致性）。
        "-smp", "2",
        # 门控键同 rt 形：find_file("opt/berry/lat") 命中才 spawn；缺席 =
        # run/smoke 零扰动。
        "-fw_cfg", "name=opt/berry/lat,string=1",
    ]
    with tempfile.NamedTemporaryFile(
        prefix="berryos-bench-lat.", suffix=".log", delete=False
    ) as tf:
        log_path = tf.name
    qemu_preflight()  # 跨进程串行守卫（run_rt 同款）
    proc = subprocess.Popen(
        qemu_cmd,
        stdin=subprocess.DEVNULL, stdout=open(log_path, "w"),
        stderr=subprocess.STDOUT, start_new_session=True,
    )
    try:
        deadline = time.time() + BOOT_TIMEOUT_LAT
        while time.time() < deadline:
            if proc.poll() is not None:
                break  # QEMU 提前退出（崩溃）——带着现有日志走收割失败路径
            text = Path(log_path).read_text(encoding="utf-8", errors="replace")
            # 全路完成标记齐才收（SGI 路舰队 spawn + 32 轮 paced 握手较慢
            # 后到——恰 IPC 先齐即退 = SGI 路行缺收假绿）。
            marks = {m.group(1) for m in DONE_LAT_RE.finditer(text)}
            if all(LAT_DONE_MARKERS[l] in marks for l in LAT_LANES):
                break
            time.sleep(1)
        text = Path(log_path).read_text(encoding="utf-8", errors="replace")
    finally:
        # 与 run_rt 同款：halt 后不退出，收完即杀（kill 后补 wait 收尸）。
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        Path(log_path).unlink(missing_ok=True)

    # 完成标记按路分记（恰一个/路、n 逐路 == LAT_N；缺席诊断按缺失路点名）。
    lane_of = {v: k for k, v in LAT_DONE_MARKERS.items()}
    done_ns: dict[str, int] = {}
    for m in DONE_LAT_RE.finditer(text):
        mark, n = m.group(1), int(m.group(2))
        lane = lane_of[mark]
        if lane in done_ns:
            raise SystemExit(
                f"bench.py: {mark} ok 行重复收割（每路恰一行——改 task_lat_* 后同步）")
        done_ns[lane] = n
    missing = [l for l in LAT_LANES if l not in done_ns]
    if missing:
        marks_hint = ", ".join(f"{LAT_DONE_MARKERS[l]} ok" for l in missing)
        raise SystemExit(
            f"bench.py: {marks_hint} 行 {BOOT_TIMEOUT_LAT}s 未出现（boot 失败 / "
            f"lat 任务 panic（负延迟是 L0 执法）/ fw_cfg 门控未命中？）——"
            f"串口尾部：{' | '.join(text.splitlines()[-5:])}")
    rows: dict[str, tuple[int, ...]] = {}
    for m in BENCHLAT_RE.finditer(text):
        lane = m.group(1)
        vals = tuple(int(m.group(i)) for i in range(2, 8))
        if lane in rows:
            raise SystemExit(
                f"bench.py: BENCHLAT lane={lane} 重复收割（每路恰一行——"
                f"改 task_lat_* 后同步）")
        rows[lane] = vals
    # 一致性（run_rt 镜像形）：在册路恰齐、逐路 n==LAT_N、完成标记 n 对路相等。
    if (tuple(sorted(rows)) != tuple(sorted(LAT_LANES))
            or any(v[0] != LAT_N for v in rows.values())
            or any(done_ns[l] != LAT_N for l in LAT_LANES)):
        raise SystemExit(
            f"bench.py: BENCHLAT 收割不齐（lanes={sorted(rows)} done={done_ns} "
            f"——须恰 {LAT_LANES} 各一行、n=={LAT_N}、每路完成标记 n=={LAT_N}；"
            f"改 task_lat_* 后同步）")
    write_receipt(
        "lat", text,
        "\n".join(
            f"lane={lane} n,pmin,p50,p90,p99,pmax={','.join(map(str, vals))}"
            for lane, vals in sorted(rows.items())
        ))
    return rows


# ---- G4-3 D/E 刀通道（--dpg；调页 breakdown §7 D/E 面）----

# D 刀收割行（svcmgr dpg_bench 手排行；带参形态防子串混叠——与 DPGF/DPG ok
# 位 3 B≠F 双距互斥）。**非锚定**：串口多生产者拼行（run5 实证 PAR 字母流
# 与 DPGB 同行无换行、内核 switches= 行亦拼在字母流中）——^…$ 锚必 miss。
DPGB_RE = re.compile(r"DPGB p50=(\d+) p99=(\d+) min=(\d+) max=(\d+) n=(\d+)")
# 标签钉板（svcmgr 侧五枚 const assert 同族——§A135 镜像）：正则标签须与
# dpg_bench 手排行字面一一对应，漂移即 import 时炸响。
assert all(_lbl in DPGB_RE.pattern
           for _lbl in (" p50=", " p99=", " min=", " max=", " n=")), \
    "DPGB_RE 标签漂移（须与 svcmgr dpg_bench 手排行字面同步）"
# E 刀相位标记（svcmgr dpg_foot 6 字节行；帧序 A→B→C→E）——子串 find，
# 拼行免疫。
DPGE_MARKERS = ("DPGE A", "DPGE B", "DPGE C", "DPGE E")
# X-4 E 面 exec 差分帧标记（svcmgr dpgx_foot 行；帧序 A→B→C→E，DPGE 同形）
# ——非锚定子串 find 同上；`DPGX ready` 走私有通道不进串口，不作断言面。
DPGX_MARKERS = ("DPGX A", "DPGX B", "DPGX C", "DPGX E")
# X-5 现役 bin 差分帧标记（svcmgr x5_foot 行；帧序 A→B→C→E，DPGX 同形）
# ——hexyl/grep 停泊双胎 demand vs eager 装载差分（第三组 d''<e''）。
X5_MARKERS = ("X5 A", "X5 B", "X5 C", "X5 E")
# soakmon 心跳 pages= 字段（内核 main.rs 9 字段行 `SOAK t=… pages=<n> lost=…`；
# 60s 拍、首拍潜伏 5 拍 ≈ 300s）。**非锚定**同 DPGB_RE——内核行也会被并发
# 前缀拼行（switches= 实证）；行内原子性由单次 uart_puts 整行写保证。
SOAK_PAGES_RE = re.compile(r"SOAK .*?pages=(\d+)")


def dpg_volumes() -> list[Path]:
    """rootfs 递送卷集发现（X-3c 后 rootfs >32MiB 常态双卷，smoke.sh A1-3b
    同口径）：rootfs0..N 连续前缀在席即 split 形（手术面 = 末卷，真
    TRAILER 恒在末卷——mkcpio 切分器条目对齐保证）；缺席回落旧单卷
    rootfs.cpio。返回 = 递送序全卷（前缀卷原样 + 末卷待手术）。"""
    vols, i = [], 0
    while (REPO / f"rootfs{i}.cpio").exists():
        vols.append(REPO / f"rootfs{i}.cpio")
        i += 1
    return vols or [ROOTFS]


def dpg_variant(*gates: str) -> list[Path]:
    """门控变体卷集：末卷 TRAILER 前手术追各 `<gate>.on` 空档（smoke-
    dpgsoak 同款 newc 逻辑；TRAILER 名域 rfind 定位 = 档尾真 trailer，数据
    内偶合 ASCII 被跳过——rootfs0 卷内十余处数据偶合实证同防；X-4 起 E 臂
    三门 dpge+dpgx+x5 同卷共递；X-3c 后 rootfs 越单卷帽，手术面从 rootfs.cpio
    迁末 split 卷——门档落归档尾 = 与单卷时代同字节位）。确定性产物：同输入
    同字节。返回 = 递送序全卷（前缀卷原样引用 + 手术末卷）。"""
    sys.path.insert(0, str(REPO / "tools"))
    from mkcpio import entry

    vols = dpg_volumes()
    last = vols[-1]
    if len(vols) == 1:
        variant = REPO / f"rootfs.{'.'.join(gates)}.cpio"
    else:
        variant = REPO / f"rootfs{len(vols) - 1}.{'.'.join(gates)}.cpio"
    blob = last.read_bytes()
    name_pos = blob.rfind(b"TRAILER!!!")
    if name_pos <= 0:
        raise SystemExit("bench.py: 末卷无 TRAILER——档坏（先 just build-user）")
    out = blob[: name_pos - 110]
    for i, gate in enumerate(gates):
        # ino 避开正装递增域（双门错开 0xF0000001/2）。
        out += entry(gate.encode() + b".on", b"", 0xF0000001 + i)
    out += entry(b"TRAILER!!!", b"", 0)
    out += b"\0" * (-len(out) % 4)
    variant.write_bytes(out)
    return vols[:-1] + [variant]


def dpg_boot(kernel: str, volumes: list[Path], soak: bool, done_pat: str,
             cap: int, log_kind: str,
             x6_out: "list[tuple[float, int, int]] | None" = None) -> str:
    """D/E 共形 boot：**满配设备面**（dpgsoak 同款——vblk 预拉扫 MMIO 无盘
    即 loud 自陷 → G1.5 杀数 2>1 panic；TCPS 相先于 dpg 位 ⇒ echod 对端
    在席；evs/skt 非本终态依赖〔f4 C-full concur-2〕）+ -smp 2（家族对照
    口径一致）→ 轮询临时日志至 done_pat → SIGTERM+wait+3s 串口落定 →
    杀 echod → 返回串口全文。E 臂 soak=True 追加 opt/berry/soak 心跳门。
    卷递送（X-3c 后 split 形）：单卷名 opt/berry/rootfs 旧口径；多卷 =
    内核连续前缀发现域 opt/berry/rootfs0..9（smoke.sh A1-3b 同口径）。"""
    # 每卷越帽执法（CRIT-CF1 判例：fw_cfg 单卷越内核 ROOTFS_MAX 32MiB =
    # main.rs 静默 return None，门 miss → 烧帽且无成因行——越帽即 loud 拒入；
    # split 形逐卷查——手术只胀末卷，但前缀卷同查防手改盘外档）。
    for vol in volumes:
        vsz = vol.stat().st_size
        if vsz > 33_554_432:
            raise SystemExit(
                f"bench.py: 递送卷 {vol.name} {vsz}B 越内核 ROOTFS_MAX 32MiB "
                "帽（fw_cfg 静默拒收面）——瘦身 rootfs 后重建")
    # 端口让场（smoke/soak 族同值端口 = 天然串行协议；被占即退）。
    if subprocess.run(["lsof", "-nP", "-iTCP:18080-18083", "-sTCP:LISTEN"],
                      capture_output=True).returncode == 0:
        raise SystemExit("bench.py: 18080-18083 端口族被占（lsof）——兄弟 "
                         "smoke/soak 线在跑？让场后重试")
    # 门控串预检（smoke-dpgsoak :44 同款）：内核 ELF 缺 dpg 手排/相位串 =
    # 陈旧内核（改嵌用户 bin 未重建——§A53 两遍构建纪律）——预拒不烧帽。
    kernel_blob = Path(kernel).read_bytes()
    for sig in (b"DPGB p50=", b"DPGE A", b"DPGX A", b"X5 A"):
        if sig not in kernel_blob:
            raise SystemExit(
                f"bench.py: 内核 ELF 无 `{sig.decode()}` 串——陈旧内核（改嵌"
                "用户 bin 未重建）？重建后再跑")
    # 1MiB 盘 + LBA0 印章（dpgsoak 同款；盘名专属防覆写兄弟线活盘）。
    disk = REPO / "disk.dpg.img"
    subprocess.run([sys.executable, str(REPO / "tools" / "mkfat.py"), str(disk)],
                   check=True)
    with open(disk, "r+b") as f:
        f.write(b"BERRYOSVBLK!")
    # rootfs 递送（split 形多卷名 / 单卷旧名——dpg_volumes 同口径）。
    if len(volumes) == 1:
        fw_rootfs = ["-fw_cfg", f"name=opt/berry/rootfs,file={volumes[0].resolve()}"]
    else:
        fw_rootfs = [
            arg for i, vol in enumerate(volumes)
            for arg in ("-fw_cfg", f"name=opt/berry/rootfs{i},file={vol.resolve()}")
        ]
    qemu_cmd = [
        "qemu-system-aarch64", "-machine", "virt", "-cpu", "cortex-a72",
        "-m", "128M", "-nographic", "-kernel", kernel, "-smp", "2",
        *fw_rootfs,
        "-global", "virtio-mmio.force-legacy=false",
        "-drive", f"if=none,id=drv0,format=raw,file={disk},"
                  "cache.writeback=off",
        "-device", "virtio-blk-device,drive=drv0",
        "-netdev", "user,id=n0,hostfwd=tcp::18081-:18081,"
                   "hostfwd=tcp::18082-:18082,hostfwd=tcp::18083-:18083",
        "-device", "virtio-net-device,netdev=n0",
        "-device", "virtio-tablet-device,id=tab0",
    ]
    if soak:
        qemu_cmd += ["-fw_cfg", "name=opt/berry/soak,string=1"]
    with tempfile.NamedTemporaryFile(
        prefix=f"berryos-bench-dpg-{log_kind}.", suffix=".log", delete=False
    ) as tf:
        log_path = tf.name
    # echod 对端（宿主 18080；guest 经 10.0.2.2 撞回环，无需 hostfwd）。
    echod = subprocess.Popen(
        [sys.executable, str(REPO / "tools" / "echod.py")],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        qemu_preflight()  # 跨进程串行守卫（run_rt 同款）
    except BaseException:  # C-DE1：拒入先收 echod——preflight raise 不孤儿化对端
        echod.terminate()
        echod.wait(timeout=5)
        raise
    proc = subprocess.Popen(
        qemu_cmd,
        stdin=subprocess.DEVNULL, stdout=open(log_path, "w"),
        stderr=subprocess.STDOUT, start_new_session=True,
    )
    try:
        deadline = time.time() + cap
        prev_len = 0
        x6_end = 0
        while time.time() < deadline:
            if proc.poll() is not None:
                break  # QEMU 提前退出（崩溃）——带现有日志走收割失败路径
            text = Path(log_path).read_text(encoding="utf-8", errors="replace")
            if x6_out is not None:
                now = time.time()
                # 回看 64B 覆盖上轮半行（X6 行 <30B）；end 单调防重采。
                for m in X6_RE.finditer(text, max(0, prev_len - 64)):
                    if m.end() > x6_end:
                        x6_out.append((now, int(m.group(1)), int(m.group(2))))
                        x6_end = m.end()
                prev_len = len(text)
            if done_pat in text:
                break
            time.sleep(X6_POLL_S if x6_out is not None else 1)
    finally:
        # run_rt 同款：halt 后不退出，收完即杀（kill 后补 wait 收尸）；
        # SIGTERM 后 3s 串口落定（dpgsoak A-6 实证同款）再读终态。
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        time.sleep(3)
        text = Path(log_path).read_text(encoding="utf-8", errors="replace")
        if x6_out is not None:
            # 终读补采：done 后至 SIGTERM 落定窗内新到观测行（3s 落定窗）。
            for m in X6_RE.finditer(text, max(0, prev_len - 64)):
                if m.end() > x6_end:
                    x6_out.append((time.time(), int(m.group(1)),
                                   int(m.group(2))))
                    x6_end = m.end()
        echod.terminate()
        echod.wait(timeout=5)
    if done_pat in text:
        Path(log_path).unlink(missing_ok=True)
        return text
    # 失败保留串口全文（诊断面：S-C2 同族——烧帽档须可复核）。
    # 实耗/预算帽双报（2026-11-09 判例：外杀/崩溃提前收卷时消息只报 cap
    # 令验尸误读「跑满 3600s」——mtime 才是地面真值，消息面同步补齐）。
    elapsed = int(time.time() - (deadline - cap))
    raise SystemExit(
        f"bench.py: `{done_pat}` 未出现（实耗 {elapsed}s / 预算帽 {cap}s——提前"
        f"收卷先查 QEMU 死因：外杀/panic/崩溃；跑满帽才是真超时）"
        f"（boot 失败 / svcmgr dpg 链降级（eco 行）/ 门控卷未递 / 内核陈旧"
        f"〔改 bin 未重建〕？）"
        f"——串口全文保留：{log_path}"
        f"（尾部：{' | '.join(text.splitlines()[-5:])}）")


def run_dpg(kernel: str) -> None:
    """G4-3 D/E 刀通道（只出表不对拍——R-1/lat 同族独立通道：D 延迟 = TCG
    墙钟面无基线门禁语义；证据档 = write_receipt 落盘）。

    D 臂（dpgb.on）：单子 32 页顺序首触 × 一对一供给，faulter 侧 clock 环夹
    → `DPGB p50= p99= min= max= n=32` 收割出表。
    E 臂（dpge.on + dpgx.on + x5.on + opt/berry/soak，X-5 起三门同卷）：DPGE 帧 =
    demand 8 触 vs eager 32 页 commit 的 soakmon `pages=` 心拍差分——「标记
    前最近拍」配对 d=pB−pA / e=pC−pB（**窗在标记前** = svcmgr dpg_foot 帧
    序契约），d<e 结构断言（demand 省 ≈24 页面 + 元页）；帧序 DPGE A→B→C→E
    + 相邻标记间 ≥2 拍窗完整性。DPGX 帧（X-4 E 面 exec 实装载差分）= 同形
    帧跑 bin/dpgx 双生 spawn（demand 胎 = congruent 单 k 特形真消费者；
    eager 胎 = EagerGuard 摘钩对照，唯一变量 = 装载分支）——第二组 d'<e'
    配对 + 同套帧序/窗完整性断言。X5 帧（X-5 现役 bin 差分）= 同形帧跑
    bin/hexyl+bin/grep 停泊双胎（demand 双胎 stdin 持端停泊锚 warm 集 vs
    eager 双胎全幅 commit）——第三组 d''<e'' 配对 + 同套断言；done 标记 =
    `X5 E`（全帧最后行）。
    """
    if not ROOTFS.exists():
        raise SystemExit("bench.py: rootfs.cpio 缺席——先 just build-user")
    # 内核字节冻结（X-4 判例：双臂逐 boot，臂间主树重编 = 活臂下静默换 ELF
    # ——门控串预检只防「串缺席」不防「同串不同码」；D 臂旧码绿 + E 臂新码
    # 楔的混态跑实踩）。run 起点快照 + sha256 回执，双臂恒用快照件。
    with tempfile.NamedTemporaryFile(
        prefix="berryos-bench-dpg-kernel.", suffix=".elf", delete=False
    ) as kf:
        kf.write(Path(kernel).read_bytes())
        kernel = kf.name
    ksha = hashlib.sha256(Path(kernel).read_bytes()).hexdigest()
    print(f"bench.py: 内核冻结 {kernel} sha256={ksha[:16]}…（双臂恒用此字节）")
    # D 臂。
    text_d = dpg_boot(kernel, dpg_variant("dpgb"), False, "DPGB p50=",
                      DPG_BOOT_TIMEOUT_D, "d")
    m = DPGB_RE.search(text_d)
    if m is None:
        raise SystemExit(
            "bench.py: DPGB 行形态不符（改 svcmgr dpg_bench 后同步本正则）"
            f"——串口尾部：{' | '.join(text_d.splitlines()[-5:])}")
    p50, p99, mn, mx, n = (int(x) for x in m.groups())
    if n != 32:
        raise SystemExit(f"bench.py: DPGB n={n} ≠ 32（行生成/收割双源漂移）")
    print(f"[G4-3(D)] p50={p50}ns p99={p99}ns min={mn}ns max={mx}ns n={n}")
    # C-DE2：D 臂收割即先落一档——E 臂独立盘/独立 boot，E 败时 D 证据不陪葬。
    write_receipt("dpg-d", text_d,
                  f"D: p50={p50} p99={p99} min={mn} max={mx} n={n}")
    # E 臂。x6_events = X-6(0) 观测行宿主时戳采集（D 臂不采——E 族 = 验收面）。
    x6_events: list[tuple[float, int, int]] = []
    text_e = dpg_boot(kernel, dpg_variant("dpge", "dpgx", "x5"), True, "X5 E",
                      DPG_BOOT_TIMEOUT_E, "e", x6_out=x6_events)
    beats = [(b.start(), int(b.group(1))) for b in SOAK_PAGES_RE.finditer(text_e)]
    if not beats:
        raise SystemExit(
            "bench.py: E 臂无 SOAK 心跳行（fw_cfg opt/berry/soak 未递 / 内核"
            " soakmon 未启？）——串口尾部："
            f"{' | '.join(text_e.splitlines()[-5:])}")
    pos: dict[str, int] = {}
    for mk in DPGE_MARKERS:
        p = text_e.find(mk)
        if p < 0:
            raise SystemExit(f"bench.py: E 臂 `{mk}` 标记缺席（帧中断）")
        pos[mk] = p
    if [pos[mk] for mk in DPGE_MARKERS] != sorted(pos.values()):
        raise SystemExit(f"bench.py: E 臂 DPGE 标记行序错乱（{pos}）")
    # 标记前最近拍（配对口径单一事实源在本处 + svcmgr 帧序注释）。
    last: dict[str, int] = {}
    for mk, p in pos.items():
        prev = [v for s, v in beats if s < p]
        if not prev:
            raise SystemExit(f"bench.py: E 臂 `{mk}` 前无 SOAK 拍（预滚不足？）")
        last[mk] = prev[-1]
    # 窗完整性：相邻标记间 ≥2 拍（130s 窗 × 60s 拍契约）。
    for lo, hi in (("DPGE A", "DPGE B"), ("DPGE B", "DPGE C"), ("DPGE C", "DPGE E")):
        n_between = sum(1 for s, _ in beats if pos[lo] < s < pos[hi])
        if n_between < 2:
            raise SystemExit(
                f"bench.py: E 臂 {lo}→{hi} 窗内拍数 {n_between} < 2（窗口/心跳"
                "契约破——改 svcmgr DPGE_WINDOW_NS 或内核心跳周期须同步）")
    pa, pb, pc = last["DPGE A"], last["DPGE B"], last["DPGE C"]
    d, e = pb - pa, pc - pb
    if not (d < e):
        raise SystemExit(
            f"bench.py: E 刀结构断言败：d={d} !< e={e}（demand 8 触增量应显著"
            f"小于 eager 32 页 commit 增量；pA={pa} pB={pb} pC={pc}）")
    print(f"[G4-3(E)] pA={pa} pB={pb} pC={pc} | d(demand 8触)={d} "
          f"e(eager 32页)={e} | d<e ok（差 {e - d} 页）")
    # X-4 X 面（exec 实装载差分）：DPGX 帧同套断言——帧序/≥2 拍窗完整性/
    # 第二组 d'<e'（demand 胎 congruent 装载触集 vs eager 胎全幅 commit）。
    pos_x: dict[str, int] = {}
    for mk in DPGX_MARKERS:
        p = text_e.find(mk)
        if p < 0:
            raise SystemExit(f"bench.py: E 臂 `{mk}` 标记缺席（X 帧中断）")
        pos_x[mk] = p
    if [pos_x[mk] for mk in DPGX_MARKERS] != sorted(pos_x.values()):
        raise SystemExit(f"bench.py: E 臂 DPGX 标记行序错乱（{pos_x}）")
    if pos["DPGE E"] >= pos_x["DPGX A"]:
        raise SystemExit("bench.py: E 臂 DPGX A 未后随 DPGE E（帧叠序破）")
    last_x: dict[str, int] = {}
    for mk, p in pos_x.items():
        prev = [v for s, v in beats if s < p]
        if not prev:
            raise SystemExit(
                f"bench.py: E 臂 `{mk}` 前无 SOAK 拍（X 预滚不足？）")
        last_x[mk] = prev[-1]
    for lo, hi in (("DPGX A", "DPGX B"), ("DPGX B", "DPGX C"),
                   ("DPGX C", "DPGX E")):
        n_between = sum(1 for s, _ in beats if pos_x[lo] < s < pos_x[hi])
        if n_between < 2:
            raise SystemExit(
                f"bench.py: E 臂 {lo}→{hi} 窗内拍数 {n_between} < 2（X 帧"
                "窗口/心跳契约破）")
    xa, xb, xc = last_x["DPGX A"], last_x["DPGX B"], last_x["DPGX C"]
    dx, ex = xb - xa, xc - xb
    if not (dx < ex):
        raise SystemExit(
            f"bench.py: X-4 结构断言败：d'={dx} !< e'={ex}（demand 胎 exec"
            f" 装载触集应小于 eager 胎全幅 commit；pA'={xa} pB'={xb} pC'={xc}）")
    print(f"[X-4(X)] pA'={xa} pB'={xb} pC'={xc} | d'(demand胎exec)={dx} "
          f"e'(eager胎全幅)={ex} | d'<e' ok（差 {ex - dx} 页）")
    # X-5 面（现役 musl 多段族 exec demand 化差分）：X5 帧同套断言——帧序/
    # 叠序（X5 A 后随 DPGX E）/≥2 拍窗完整性/第三组 d''<e''（demand 双胎
    # 停泊 warm 集 vs eager 双胎全幅 commit）。
    pos_x5: dict[str, int] = {}
    for mk in X5_MARKERS:
        p = text_e.find(mk)
        if p < 0:
            raise SystemExit(f"bench.py: E 臂 `{mk}` 标记缺席（X-5 帧中断）")
        pos_x5[mk] = p
    if [pos_x5[mk] for mk in X5_MARKERS] != sorted(pos_x5.values()):
        raise SystemExit(f"bench.py: E 臂 X5 标记行序错乱（{pos_x5}）")
    if pos_x["DPGX E"] >= pos_x5["X5 A"]:
        raise SystemExit("bench.py: E 臂 X5 A 未后随 DPGX E（帧叠序破）")
    last_x5: dict[str, int] = {}
    for mk, p in pos_x5.items():
        prev = [v for s, v in beats if s < p]
        if not prev:
            raise SystemExit(
                f"bench.py: E 臂 `{mk}` 前无 SOAK 拍（X-5 预滚不足？）")
        last_x5[mk] = prev[-1]
    for lo, hi in (("X5 A", "X5 B"), ("X5 B", "X5 C"), ("X5 C", "X5 E")):
        n_between = sum(1 for s, _ in beats if pos_x5[lo] < s < pos_x5[hi])
        if n_between < 2:
            raise SystemExit(
                f"bench.py: E 臂 {lo}→{hi} 窗内拍数 {n_between} < 2（X-5 帧"
                "窗口/心跳契约破）")
    x5a, x5b, x5c = last_x5["X5 A"], last_x5["X5 B"], last_x5["X5 C"]
    dx5, ex5 = x5b - x5a, x5c - x5b
    if not (dx5 < ex5):
        raise SystemExit(
            f"bench.py: X-5 结构断言败：d''={dx5} !< e''={ex5}（demand 双胎"
            f"停泊 warm 集应小于 eager 双胎全幅 commit；pA''={x5a} "
            f"pB''={x5b} pC''={x5c}）")
    print(f"[X-5(X5)] pA''={x5a} pB''={x5b} pC''={x5c} | "
          f"d''(demand双胎停泊)={dx5} e''(eager双胎全幅)={ex5} | "
          f"d''<e'' ok（差 {ex5 - dx5} 页）")
    # X-6(0) 页/s 直测面：观测行恰 3（hello/hexyl/boot 尾各一——83 lane 发射
    # 点校准形）——相邻差
    # 分 = 各窗页数，宿主到达时戳差分 = 窗钟。无行 = 刀未落，观测面静默
    # （pre-X6 内核照跑照绿 = 零漂移构造性保证）；有行而非 3 = 词形契约破
    # loud 拒。
    x6_summary = ""
    if x6_events:
        if len(x6_events) != 3:
            raise SystemExit(
                f"bench.py: X6 观测行 {len(x6_events)} ≠ 3（词形契约破——"
                "boot 环三点各一（hello/hexyl/尾）；查 svcmgr 发射点/行形）")
        # 窗 i = 事件 i→i+1。win0 = hello→hexyl = 纯 hexyl 装载窗（两 arm 环内
        # 相邻零插花；u1 十二胎 + stdhello 均在首行前 = 黑暗基线不入窗）；
        # win1 = hexyl→尾行 = 尾窗（simbattery/vprobe/app 批拉页）。
        win_names = ("hexyl", "tail")
        wins = []
        for i, ((t0, p0, s0), (t1, p1, s1)) in enumerate(
                zip(x6_events, x6_events[1:])):
            dt = t1 - t0
            dp, ds = p1 - p0, s1 - s0
            if dp < 0 or ds < 0:
                raise SystemExit(
                    f"bench.py: X6 计数非单调（事件 {i}→{i+1}：pg {p0}→{p1} "
                    f"sw {s0}→{s1}）——收割面坏，勿读率")
            # 钟粒度下限垫分母（窗短于轮询粒度时低估率 = 下限门安全侧）。
            rate = dp / max(dt, X6_POLL_S)
            name = win_names[i] if i < len(win_names) else f"win{i}"
            wins.append((name, dp, dt, rate, ds))
            print(f"[X-6(0)] {name}: {dp} 页/{dt:.2f}s = {rate:.1f}pg/s"
                  f"（sw+{ds}）")
        hexyl_rate = wins[0][3]
        if X6_HEXYL_PGS_FLOOR is None:
            print(f"[X-6(0)] hexyl 回归门未定标（首净窗实测 {hexyl_rate:.1f}"
                  "pg/s ×0.7 → 写 X6_HEXYL_PGS_FLOOR 常量）")
        elif hexyl_rate < X6_HEXYL_PGS_FLOOR:
            raise SystemExit(
                f"bench.py: X-6 页/s 回归门败：hexyl {hexyl_rate:.1f}pg/s < "
                f"下限 {X6_HEXYL_PGS_FLOOR}（首净窗定标 ×0.7——供给链回归？"
                "查 pager 扫圈/svcmgr 供给路径）")
        x6_summary = "\n".join(
            f"X6[{n}]: pages={dp} dt={dt:.2f}s rate={r:.1f}pg/s sw+{ds}"
            for n, dp, dt, r, ds in wins)
    summary = (
        f"D: p50={p50} p99={p99} min={mn} max={mx} n={n}\n"
        f"E: pA={pa} pB={pb} pC={pc} d={d} e={e} (d<e)\n"
        f"X: pA'={xa} pB'={xb} pC'={xc} d'={dx} e'={ex} (d'<e')\n"
        f"X5: pA''={x5a} pB''={x5b} pC''={x5c} d''={dx5} e''={ex5} (d''<e'')"
        + (f"\n{x6_summary}" if x6_summary else ""))
    write_receipt("dpg", text_d + "\n===== E 臂 =====\n" + text_e, summary)


def run_icount(kernel: str, trace_off: bool = False) -> tuple[dict[str, int], dict[str, int], str]:
    """B-2 通道：`-icount shift=1,sleep=off` 裸跑，等 boot 尾 `BENCHWAKE n=`，
    双面收割 → ({name: cyc_per_op}, {name: iters}, BENCHWAKE 整行)。

    trace_off=False（默认臂）：如上。trace_off=True（G3-4b 双跑 off 臂，
    草案 §4）：追加 `-fw_cfg name=opt/berry/trace,string=0`（内核侧记录面
    关——环写/直方图让位，`TRACE off ok` 替代标记上），done 标记与观测行
    换 TRACE_OFF_DONE_RE（BENCHWAKE 在 off 臂结构性缺席——直方图零样本，
    等它 = 必超帽假红）。

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
    ⑤qemu_preflight 跨进程串行守卫；⑥trace_off 参数（off 臂 fw_cfg +
    done/观测行换面——收割协议与六行 BENCH 面两臂全同）。

    -icount 下 boot 不达 done 标记 = 合法发现（§13 语义② boot 时序全变）
    ——本函数只 loud 上报（SystemExit 带串口尾），绝不放宽帽硬凑绿。
    """
    qemu_cmd = [
        "qemu-system-aarch64", "-machine", "virt", "-cpu", "cortex-a72",
        "-m", "128M", "-nographic", "-kernel", kernel,
        # 定值注见 ICOUNT_ARGS（shift 不可参数化）。
        *ICOUNT_ARGS,
    ]
    done_re = TRACE_OFF_DONE_RE if trace_off else BENCHWAKE_RE
    if trace_off:
        # G3-4b off 臂门控键（rt/lat 同形具名门；内核 telemetry::init 读值
        # 一次定值——string=0 恰 1 字节 = 记录面关）。
        qemu_cmd += ["-fw_cfg", "name=opt/berry/trace,string=0"]
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
            if done_re.search(text):
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

    if done_re.search(text) is None:
        raise SystemExit(
            f"bench.py: done 标记（{'TRACE off ok' if trace_off else 'BENCHWAKE'}）"
            f" {BOOT_TIMEOUT_ICOUNT}s 未出现（-icount 下 boot 不达 = 合法发现，"
            f"如实上报勿放宽帽——串口尾部：{' | '.join(text.splitlines()[-5:])})")
    print(f"[B-2(icount{',off' if trace_off else ''})] boot→done 标记 "
          f"{time.time() - t0:.1f}s wall（帽 {BOOT_TIMEOUT_ICOUNT}s）")
    # 观测面：done 标记整行（on 臂 = BENCHWAKE〔R-0 依赖注记——wake 延迟确定性
    # 观测，不动门禁〕；off 臂 = TRACE off ok 替代标记行）。
    wake_line = done_re.search(text).group(0)
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
        raise SystemExit("bench.py: done 标记已在而 BENCH ok 行缺席——时序假设"
                         "「done 标记严格后于 BENCH ok」被打破，门禁面不可收割")
    if len(metrics) != done_n:
        raise SystemExit(
            f"bench.py: BENCH 行数 {len(metrics)} ≠ BENCH ok n={done_n}"
            "（内核侧计数与出表不一致——改 bench.rs 后同步 N_METRICS）")
    write_receipt(
        f"icount{'-off' if trace_off else ''}", text,
        "\n".join(f"{n} iters={iters_seen[n]} cyc={metrics[n]}"
                  for n in sorted(metrics)) + f"\n{wake_line}")
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
    prove_lines: list[str] = []  # 回执面（f4 F1）：漂移谱随 verdict 一并归档
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
        prove_lines.append(f"ICOUNT,{name},{','.join(map(str, vals))},max_dev={dev:.3f}%")
    verdict = f"ICOUNT verdict={'exact' if exact else 'drift'} max_dev={worst:.3f}%"
    print(verdict)
    prove_lines.append(verdict)
    prove_lines.append(
        f"BENCHWAKE five-run byte-identical: {len(set(wakes)) == 1}"
        f"{': ' + wakes[0] if len(set(wakes)) == 1 else ''}")
    write_receipt("icount-prove", "", "\n".join(prove_lines))
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
    头注释六件 = §13 语义①/跨机不可比（§2 铁律 1——非公约 5：公约 5 是
    TCG 半率单位纪律，跨机绝对值不可比的成文位置是 §2 铁律 1，2026-10-27
    审查勘误）/刷新命令/ICOUNT_TOL 镜像/cntvct_pair 量化语义（与 load 侧
    严格同形——P6 A-5 基准漂移雷：读写两侧不同形则「双绿」退化成对陈旧
    基准自证）/史档指针（史档与勘误记事迁 notes.md 不随 --update 重写——
    2026-11-04 m1 迁移前整文件模板重写曾实证抹史一次，沿革五十四续）。"""
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
        "# 史档/勘误记事 → tools/bench_baseline_icount.notes.md（--update 不触史，2026-11-04 m1 迁移）。",
    ]
    for name, cyc in sorted(metrics.items()):
        lines.append(f"{name},{iters.get(name, 0)},{cyc}")
    BASELINE_ICOUNT.write_text("\n".join(lines) + "\n")


def load_baseline_icount_off() -> dict[str, int]:
    """icount off 臂基线解析：与 load_baseline_icount 严格同形（裸 CSV
    `name,iters,cyc`；独立文件即分段）——P6 A-5 双源漂移雷：读侧与 save
    侧同形。文件缺席 = 空 dict（首跑自立）。"""
    base: dict[str, int] = {}
    if not BASELINE_ICOUNT_OFF.exists():
        return base
    for raw in BASELINE_ICOUNT_OFF.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = BENCH_RE.match(f"BENCH,{line}")
        if m:
            base[m.group(1)] = int(m.group(3))
    return base


def save_baseline_icount_off(metrics: dict[str, int]) -> None:
    """刷新 off 臂基线整个文件（单段独立文件；**绝不经 save_baseline_icount**
    ——那是 on 家族文件，两文件数据源不同臂不能互写）。头注释镜像 on 家族
    六件 + 关臂语义/结构性零足迹定谳（TRACE_CAP 注释为单一事实源，此处只
    镜像）。"""
    iters = _iter_table()  # 与 on 家族同六项（同一 bench 面，分文件不分项）
    lines = [
        "# BerryOS -icount 确定性通道 **trace 记录面关** 臂基线（G3-4b 双跑，",
        "# `python3 tools/bench.py --trace-dual`；fw_cfg opt/berry/trace=0）。",
        "# 口径 = CNTVCT 周期数在 -icount shift=1 虚拟时钟下的指令数代理",
        "# （每指令 2ns，折算 ≈0.125 cyc/instr）——与 tools/bench_baseline.txt",
        "# （实时）跨模式、与 bench_baseline_icount.txt（on 臂）**跨配置**、",
        "# 跨机三态皆不可比（§2 铁律 1）。",
        "# 结构性零足迹：bench 六窗前置调度器纪元，遥测钩子窗口内不发射——",
        "# off 臂值 = on 臂值的镜像（关-开差恒 0，TRACE_CAP=0.0 执法）；本文件",
        "# 存在的意义 = off 宇宙指令流的独立漂移绊线（门面回退/记录面让位",
        "# 过宽都会在此先红）。",
        "# 刷新：python3 tools/bench.py --trace-dual --update（on 家族文件随同刷）。",
        "# 史档/勘误记事 → tools/bench_baseline_icount.notes.md（on/off 家族共册）。",
    ]
    for name, cyc in sorted(metrics.items()):
        lines.append(f"{name},{iters.get(name, 0)},{cyc}")
    BASELINE_ICOUNT_OFF.write_text("\n".join(lines) + "\n")


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


def _l2_bad(tag: str, metrics: dict[str, int], base: dict[str, int]) -> list[str]:
    """L2 对拍（--icount 门禁的家族镜像，供 --trace-dual 两臂复用）：双侧带
    [old×(1−tol), old×(1+tol)]（tol=ICOUNT_TOL，确定性通道双向漂移都是指令流
    变化）；0 基线行 0→0 恰等过、0→非零 = 无界漂移（prove 同口径零语义）；
    缺席行记 FAIL（改基准源后须 --update）。既有 --icount 内联门禁块不动
    （零回归面），本 helper 只服务双跑。"""
    bad: list[str] = []
    for name, cyc in sorted(metrics.items()):
        old = base.get(name)
        if old is None:
            print(f"bench.py: [{tag}] 新基准项 {name}（基线缺席，不过门禁）")
            continue
        if old == 0:
            if cyc != 0:
                bad.append(f"[{tag}] {name}: 0 → {cyc}（+inf%，出 L2 界 [0, 0]"
                           "——基线 0 出非零 = 量化粒度级无界漂移）")
            continue
        lo, hi = old * (1.0 - ICOUNT_TOL), old * (1.0 + ICOUNT_TOL)
        if not lo <= cyc <= hi:
            dev = (cyc - old) * 100.0 / old
            bad.append(f"[{tag}] {name}: {old} → {cyc}（{dev:+.2f}%，出 L2 界 "
                       f"[{lo:g}, {hi:g}]——确定性通道双向漂移都是指令流变化）")
    for m in sorted(set(base) - set(metrics)):
        bad.append(f"[{tag}] 基准项 {m} 从输出消失（改基准源后须 --update）")
    return bad


def selftest() -> int:
    """门禁自检（a-3 U4 纪律同款）——bench 对拍器自身的钉板（纯 host 零 QEMU）。

    本工具是 just bench / bench-rt / bench-icount 六通道 QEMU 门禁的宿主
    收割与对拍面，此前零自检：收割正则坏 = 基准行静默漏收（假超时红）或
    误收（空心绿）；基线读写两侧不同形 = 「双绿」对陈旧基线自证（P6 A-5
    基准漂移雷）。覆盖：
    - 收割正则族：B-0 行锚定 vs B-1/RT/LAT/WAKE 全文子串（矩阵巨行中段
      教训族）；DONE_RE MULTILINE 中位命中；BENCHWAKE 撕裂半行结构性
      不命中（轮询/收割三处共用全形状——2026-10-27 三镜审查补修面）
    - 双段基线 roundtrip：B-0 裸 CSV / B-1 BENCH1 前缀 / --update 单段
      刷新另一段保留 / 文件缺席双空 dict（首跑自立合法）
    - icount 双文件：on/off 头契约（ICOUNT_TOL / TRACE_CAP 镜像行）+
      回读等 + off 写不触 on（两文件数据源不同臂不能互写）
    - _l2_bad L2 带：等值过 / 0→0 恰等过 / 0→非零无界红 / 双侧漂移红 /
      项消失红 / 新项跳过 / tol 参数化双侧带语义
    - 三镜迭代定值 live 对读：bench.rs 六项字面 ↔ _iter_table；bench1
      四 const ↔ _iter_table_el0；bsh bench1_selftest WANT ↔ 同表
      （「改值三处同步」纪律的宿主执法面）
    - RT/LAT wire 镜像：RT_SAMPLES/LAT_SAMPLES ↔ RT_N/LAT_N、deltas
      档数 ↔ RT_TIERS、RT_TIERS×RT_N=96（`RT ok n=96` 字面账）
    - write_receipt 回执：落盘 + serial_sha256 内容指纹（K-DE2）
    - 组合拒绝 exit-code 契约：六通道互斥族子进程真跑（rc!=0 + 逐组
      说因消息在 stderr）；--selftest 独占守卫（组合旗标 rc=2）
    """
    import shutil

    failures: list[str] = []
    _SELFTEST_TOTAL = [0]

    def check(name: str, cond, detail: str = "") -> None:
        _SELFTEST_TOTAL[0] += 1
        print(f"  {'ok' if cond else 'FAIL'} {name}"
              + (f": {detail}" if not cond else ""))
        if not cond:
            failures.append(name)

    here = Path(__file__).resolve()

    # --- 1. B-0 收割正则：行锚定（内核 bench 段行位干净的裸行语义） ---
    m = BENCH_RE.match("BENCH,frame_alloc4k,1024,1234")
    check("re b0: 全形行命中（name/iters/cyc 三组）",
          m is not None and m.group(1) == "frame_alloc4k"
          and m.group(2) == "1024" and m.group(3) == "1234")
    check("re b0: 矩阵巨行中段不命中（行锚定语义——B-1 面由子串正则收）",
          BENCH_RE.match("ssssBENCH,frame_alloc4k,1024,1234PPP") is None)
    check("re b0: 多余字段拒", BENCH_RE.match("BENCH,x,1,2,3") is None)
    check("re b0: 大写名拒（字符域 [a-z0-9_]）",
          BENCH_RE.match("BENCH,X,1,2") is None)

    # --- 2. DONE_RE：MULTILINE 中位命中 + 撕裂半行不命中 ---
    d = DONE_RE.search("matrix line\nBENCH ok n=6\nmore output")
    check("re done: 多行文本中位命中（MULTILINE——无它 ^ 只匹配串首）",
          d is not None and d.group(1) == "6")
    check("re done: 撕裂半行不命中", DONE_RE.search("BENCH ok n=") is None)

    # --- 3. B-1：全文子串收割（bsh relay 行嵌稳态矩阵巨行中段教训） ---
    giant = "s" * 40 + "BENCH1,sys_null,4096,77" + "P" * 40
    hits = list(BENCH1_RE.finditer(giant))
    check("re b1: 巨行中段子串收割（不 ^ 锚定）",
          len(hits) == 1 and hits[0].group(1) == "sys_null"
          and hits[0].group(3) == "77")
    check("re b1: done 标记子串命中（B1 ok 同嵌巨行）",
          DONE1_RE.search("ssB1 ok n=4BB") is not None)

    # --- 4. R-1：BENCHRT 全形状 + n=32 字面 wire 冻结 ---
    mrt = BENCHRT_RE.search("xxBENCHRT,delta=200,n=32,p50=1,p99=2,max=3yy")
    check("re rt: 全形状命中（嵌流子串）",
          mrt is not None and mrt.group(1) == "200" and mrt.group(2) == "1")
    check("re rt: n=31 拒（RT_SAMPLES 镜像字面冻结）",
          BENCHRT_RE.search("BENCHRT,delta=200,n=31,p50=1,p99=2,max=3") is None)
    drt = DONE_RT_RE.search("RT ok n=96")
    check("re rt: done 标记命中", drt is not None and drt.group(1) == "96")

    # --- 5. G3-4 lat：lane 双路 + 标记表一致性 ---
    mlat = BENCHLAT_RE.search(
        "xxBENCHLAT,lane=sgi,n=32,min=1,p50=2,p90=3,p99=4,max=5yy")
    check("re lat: 全形状命中（lane 分组）",
          mlat is not None and mlat.group(1) == "sgi" and mlat.group(2) == "32")
    check("re lat: 未在册 lane 拒（ipc|sgi 白名单）",
          BENCHLAT_RE.search(
              "BENCHLAT,lane=xyz,n=32,min=1,p50=2,p90=3,p99=4,max=5") is None)
    check("re lat: LAT_DONE_MARKERS 键 ⊇ LAT_LANES（新增路只扩两表）",
          set(LAT_DONE_MARKERS) >= set(LAT_LANES))
    dlat1 = DONE_LAT_RE.search("LATIPC ok n=32")
    dlat2 = DONE_LAT_RE.search("LATSGI ok n=32")
    check("re lat: 双路完成标记各命中",
          dlat1 is not None and dlat2 is not None)

    # --- 6. BENCHWAKE：全八字段形状——撕裂半行结构性不命中 ---
    wake = ("BENCHWAKE n=210 min=1497 p50=4096 p90=32768 p99=33554432 "
            "max=46253511 sig=0 tmo=0")
    check("re wake: 八字段全形状命中",
          BENCHWAKE_RE.search(wake) is not None)
    check("re wake: 撕裂半行不命中（三处共用全形状——宽前缀假早退雷）",
          BENCHWAKE_RE.search("BENCHWAKE n=21") is None)
    check("re wake: 缺字段拒",
          BENCHWAKE_RE.search("BENCHWAKE n=210 min=1497 p50=4096") is None)

    # --- 7/8/9. 基线 roundtrip + icount 双文件 + _l2_bad 带（tmp 隔离） ---
    global BASELINE, BASELINE_ICOUNT, BASELINE_ICOUNT_OFF, RECEIPT_DIR, ICOUNT_TOL
    with tempfile.TemporaryDirectory(prefix="berryos-bench-selftest.") as td:
        tdp = Path(td)
        saved = (BASELINE, BASELINE_ICOUNT, BASELINE_ICOUNT_OFF,
                 RECEIPT_DIR, ICOUNT_TOL)
        # 回执目录必须在 REPO 下（write_receipt 打印 relative_to(REPO)）；
        # out/ 已 gitignore，测后整目录删除。
        RECEIPT_DIR = REPO / "out" / "bench-receipts-selftest"
        BASELINE = tdp / "bench_baseline.txt"
        BASELINE_ICOUNT = tdp / "bench_baseline_icount.txt"
        BASELINE_ICOUNT_OFF = tdp / "bench_baseline_icount_traceoff.txt"
        try:
            b0a, b1a = load_baseline()
            check("base: 文件缺席 → 双空 dict（首跑自立合法）",
                  not b0a and not b1a)
            save_baseline(False, {"cntvct_pair": 0, "frame_alloc4k": 1234})
            txt = BASELINE.read_text(encoding="utf-8")
            check("base: B-0 段裸 CSV 行带 iter 表（读写同形）",
                  "cntvct_pair,4096,0" in txt
                  and "frame_alloc4k,1024,1234" in txt)
            save_baseline(True, {"sys_null": 456})
            txt = BASELINE.read_text(encoding="utf-8")
            check("base: B-1 段 BENCH1 前缀", "BENCH1,sys_null,4096,456" in txt)
            b0b, b1b = load_baseline()
            check("base: 双段 roundtrip 回读等",
                  b0b == {"cntvct_pair": 0, "frame_alloc4k": 1234}
                  and b1b == {"sys_null": 456})
            save_baseline(False, {"frame_alloc4k": 999})
            b0c, b1c = load_baseline()
            check("base: --update 单段刷新另一段保留（不 wipe 数值历史）",
                  b0c == {"frame_alloc4k": 999} and b1c == {"sys_null": 456})

            save_baseline_icount({"cntvct_pair": 0, "handle_evt": 166})
            ictxt = BASELINE_ICOUNT.read_text(encoding="utf-8")
            check("icount: 头含 ICOUNT_TOL 单源镜像行（防双源漂移）",
                  "ICOUNT_TOL" in ictxt)
            check("icount: 回读等",
                  load_baseline_icount()
                  == {"cntvct_pair": 0, "handle_evt": 166})
            save_baseline_icount_off({"handle_evt": 166})
            offtxt = BASELINE_ICOUNT_OFF.read_text(encoding="utf-8")
            check("icount-off: 头含 TRACE_CAP 镜像行", "TRACE_CAP" in offtxt)
            check("icount-off: off 写不触 on（两文件不同臂）",
                  load_baseline_icount()
                  == {"cntvct_pair": 0, "handle_evt": 166})

            rp = write_receipt("selftest", "serial-text", "summary-line")
            rtxt = rp.read_text(encoding="utf-8")
            want_sha = hashlib.sha256(
                "serial-text".encode("utf-8")).hexdigest()
            check("receipt: 落盘 + serial_sha256 内容指纹（K-DE2）",
                  rp.exists() and f"serial_sha256={want_sha}" in rtxt
                  and "kind=selftest" in rtxt and "serial-text" in rtxt)

            # --- _l2_bad L2 带（tol=0 精确相等 / 参数化双侧） ---
            base_l2 = {"a": 100, "z": 0}
            check("l2: 等值过", _l2_bad("t", {"a": 100, "z": 0}, base_l2) == [])
            check("l2: 新基准项跳过（基线缺席不过门禁）",
                  _l2_bad("t", {"a": 100, "z": 0, "n": 5}, base_l2) == [])
            check("l2: +1 漂移红（ICOUNT_TOL=0 精确相等形态）",
                  len(_l2_bad("t", {"a": 101, "z": 0}, base_l2)) == 1)
            check("l2: 0→非零 = 无界漂移红",
                  any("无界" in b
                      for b in _l2_bad("t", {"a": 100, "z": 1}, base_l2)))
            check("l2: 基准项消失红",
                  any("消失" in b for b in _l2_bad("t", {"a": 100}, base_l2)))
            ICOUNT_TOL = 0.05
            check("l2: tol=0.05 带内过（+4%）",
                  _l2_bad("t", {"a": 104, "z": 0}, base_l2) == [])
            check("l2: tol=0.05 带外红（+6%）",
                  len(_l2_bad("t", {"a": 106, "z": 0}, base_l2)) == 1)
            check("l2: tol=0.05 下侧带外红（−6%，双向漂移都是指令流变化）",
                  len(_l2_bad("t", {"a": 94, "z": 0}, base_l2)) == 1)
        finally:
            (BASELINE, BASELINE_ICOUNT, BASELINE_ICOUNT_OFF, RECEIPT_DIR,
             ICOUNT_TOL) = saved
            shutil.rmtree(RECEIPT_DIR, ignore_errors=True)

    # --- 10. 三镜迭代定值 live 对读（「改值三处同步」的宿主执法面） ---
    bench_rs = REPO / "kernel" / "src" / "bench.rs"
    bench1_rs = REPO / "user" / "bins" / "bench1" / "src" / "main.rs"
    bsh_rs = REPO / "user" / "bins" / "bsh" / "src" / "main.rs"
    main_rs = REPO / "kernel" / "src" / "main.rs"
    mirrors = [bench_rs, bench1_rs, bsh_rs, main_rs]
    for p in mirrors:
        check(f"mirror: {p.relative_to(REPO)} 在席", p.exists())
    if all(p.exists() for p in mirrors):
        got: dict[str, int] = {}
        for nm, it in re.findall(r'bench\(b"([a-z0-9_]+)", (\d+),',
                                 bench_rs.read_text(encoding="utf-8")):
            got[nm] = int(it)
        check("mirror: bench.rs 六项字面 ↔ _iter_table（B-0 镜）",
              got == _iter_table(),
              f"rs={sorted(got.items())} py={sorted(_iter_table().items())}")
        got1: dict[str, int] = {}
        # bench1 的 const 名去 ITERS 尾后小写 ≠ 表键（VMO_RW4K→vmo_rw4k 而
        # 键 = sys_vmo_rw4k——bench1 里 sys_ 前缀被 const 名吞进 SYS_ 段位），
        # 故映射显式逐项（四项闭集，加项时三镜同步纪律自会咬）。
        for cn, it in re.findall(r'const ([A-Z0-9_]+)_ITERS: u64 = (\d+);',
                                 bench1_rs.read_text(encoding="utf-8")):
            key = {
                "SYS_NULL": "sys_null",
                "VMO_RW4K": "sys_vmo_rw4k",
                "IPC_ENQUEUE8": "ipc_enqueue8",
                "EVT_SIGNAL": "evt_signal",
            }.get(cn, cn.lower())
            got1[key] = int(it)
        check("mirror: bench1 四 const ↔ _iter_table_el0（B-1 镜）",
              got1 == _iter_table_el0(),
              f"rs={sorted(got1.items())} py={sorted(_iter_table_el0().items())}")
        bsh_text = bsh_rs.read_text(encoding="utf-8")
        seg = bsh_text.split("const WANT: [(&str, &str); 4]", 1)
        want: dict[str, int] = {}
        if len(seg) == 2:
            for nm, it in re.findall(r'\("([a-z0-9_]+)", "(\d+)"\)',
                                     seg[1].split("];", 1)[0]):
                want[nm] = int(it)
        check("mirror: bsh bench1_selftest WANT ↔ _iter_table_el0（第三镜）",
              want == _iter_table_el0(),
              f"bsh={sorted(want.items())} py={sorted(_iter_table_el0().items())}")
        main_text = main_rs.read_text(encoding="utf-8")
        rt_m = re.search(r'const RT_SAMPLES: usize = (\d+);', main_text)
        lat_m = re.search(r'const LAT_SAMPLES: usize = (\d+);', main_text)
        dl_m = re.search(r'let deltas = \[([^\]]*)\];', main_text)
        check("mirror: RT_SAMPLES ↔ RT_N",
              rt_m is not None and int(rt_m.group(1)) == RT_N)
        check("mirror: deltas 档数 ↔ RT_TIERS",
              dl_m is not None
              and len([e for e in dl_m.group(1).split(",") if e.strip()])
              == RT_TIERS)
        check("mirror: RT ok n=96 字面账（RT_TIERS×RT_N）",
              RT_TIERS * RT_N == 96)
        check("mirror: LAT_SAMPLES ↔ LAT_N",
              lat_m is not None and int(lat_m.group(1)) == LAT_N)

    # --- 11. 组合拒绝 exit-code 契约（子进程真跑；参数验证先于 kernel_elf
    #     解析 = 零 QEMU 零构建，纯参数面红绿） ---
    combos = [
        (["--rt", "--el0"], "--rt 独立通道"),
        (["--rt", "--update"], "--rt 独立通道"),
        (["--lat", "--factor", "3"], "--lat 独立通道"),
        (["--icount", "--el0"], "--icount 不与 --el0 组合"),
        (["--icount", "--factor", "3"], "--icount 不与 --factor 组合"),
        (["--prove"], "--prove 只对 --icount"),
        (["--icount", "--prove", "--update"], "观测 only"),
        (["--trace-dual", "--icount"], "--trace-dual 不与"),
        (["--dpg", "--rt"], "--dpg 独立通道"),
    ]
    for argv, needle in combos:
        r = subprocess.run([sys.executable, str(here), *argv],
                           capture_output=True, text=True, timeout=60)
        check(f"reject: {' '.join(argv)} → rc!=0 且说因在 stderr",
              r.returncode != 0 and needle in r.stderr,
              f"rc={r.returncode} stderr={r.stderr.strip()[:100]}")
    r = subprocess.run([sys.executable, str(here), "--selftest", "--rt"],
                       capture_output=True, text=True, timeout=60)
    check("guard: --selftest 独占（组合旗标 rc=2）", r.returncode == 2,
          f"rc={r.returncode}")

    total = _SELFTEST_TOTAL[0]
    if failures:
        print(f"\nbench.py selftest: FAIL（{len(failures)} 面红 / 共 {total} 面）")
    else:
        print(f"\nbench.py selftest: PASS（0 面红 / 共 {total} 面）")
    return 1 if failures else 0


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--selftest", action="store_true",
                    help="门禁自检（a-3 U4 纪律，独占旗标——纯 host 零 QEMU）")
    ap.add_argument("--update", action="store_true", help="刷新基线文件（本模式对应段）")
    ap.add_argument("--factor", type=float, default=None,
                    help="劣化因子门禁（默认 3.0——拦数量级劣化不拦噪声）")
    ap.add_argument("--el0", action="store_true",
                    help="B-1 通道：run-rootfs 形态收割 BENCH1 四行（默认 B-0 裸跑）")
    ap.add_argument("--rt", action="store_true",
                    help="R-1 通道：裸跑（fw_cfg 门控 rt_probe）收割 BENCHRT 三行"
                         "（只出表无基线——独立通道，--update/--el0/--factor 组合拒绝）")
    ap.add_argument("--lat", action="store_true",
                    help="G3-4 bench-lat 通道：-smp 2（fw_cfg 门控 lat 任务对）收割"
                         " BENCHLAT lane 行（只出表无基线——R-1 同族独立通道）")
    ap.add_argument("--icount", action="store_true",
                    help="B-2 通道：-icount shift=1,sleep=off 确定性裸跑，收割六行"
                         " BENCH 做 L2 紧门禁对拍（基线 bench_baseline_icount.txt"
                         " 独立文件）+ BENCHWAKE 观测行")
    ap.add_argument("--trace-dual", action="store_true",
                    help="G3-4b trace 开/关双跑：--icount 同形态两臂（默认 vs "
                         "opt/berry/trace=0）+ 关-开行差帽 TRACE_CAP（首标定"
                         "=0.0 结构性零）+ 两臂各对拍自家基线（--update 刷双份）")
    ap.add_argument("--prove", action="store_true",
                    help="五跑证明（--icount 专属：漂移谱 + verdict，观测 only"
                         "——不写基线不进门禁）")
    ap.add_argument("--dpg", action="store_true",
                    help="G4-3 D/E 通道：dpgb/dpge 门控变体卷双 boot——D 首触"
                         "延迟 p50/p99 出表 + E demand/eager footprint 差分"
                         "（d<e 结构断言；R-1 同族只出表无基线，回执落盘）")
    ap.add_argument("--kernel", default=None, help="内核 ELF 路径（默认 paths.sh 解析）")
    args = ap.parse_args()

    # --selftest 独占守卫（writable_globals/unsafe_audit 同款）：组合任何
    # 通道旗标 = 语义未定义（自检不跑 QEMU，通道参数无处消费），rc=2 区分
    # 于运行失败（rc=1）。
    channel_flags = [args.update, args.el0, args.rt, args.lat, args.icount,
                     args.prove, args.dpg, args.trace_dual,
                     args.factor is not None, args.kernel is not None]
    if args.selftest:
        if any(channel_flags):
            ap.error("--selftest 独占——不与任何通道/基线旗标组合（rc=2）")
        raise SystemExit(selftest())

    if args.rt and (args.update or args.el0 or args.factor is not None
                    or args.icount or args.prove or args.lat or args.trace_dual):
        # RT 无基线段（设计 §10 只出表）——误触 save_baseline 会按双段前缀
        # 逻辑误写 bench_baseline.txt；--el0/--icount 同样不进 --rt 早退
        # 分支：静默忽略 = 用户要的通道/门禁凭空蒸发（--el0 --rt 实际只跑
        # RT、B-1 静默缺席且退出码 0——三镜审查实抓）。组合一律拒绝
        # （B-2 起 --icount/--prove 入拒绝集——prove 只对 --icount 立法；
        # --lat 入拒绝集同因——一进程一种 QEMU 形态；--trace-dual 入拒绝集
        # 同因——双跑独占两臂 QEMU 形态）。
        raise SystemExit(
            "bench.py: --rt 独立通道（只出表无基线）——不与 --update/--el0/"
            "--factor/--icount/--prove/--lat/--trace-dual 组合（--update 误写"
            "基线双段；--el0/--icount/--lat/--trace-dual 通道会被静默丢弃"
            "——一进程一种 QEMU 形态；--factor 在 RT 无门禁面；--prove 只对 "
            "--icount 立法）")

    # --- G3-4 --lat 专属校验（--rt 同款独立通道纪律）---
    if args.lat and (args.update or args.el0 or args.factor is not None
                     or args.icount or args.prove or args.trace_dual):
        raise SystemExit(
            "bench.py: --lat 独立通道（R-1 同族只出表无基线）——不与 --update/"
            "--el0/--factor/--icount/--prove/--trace-dual 组合（--rt 已在上互斥）")

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
    # --- G3-4b --trace-dual 专属校验（icount 家族双臂形态；--update/--kernel
    # 外不与任何通道组合——双跑独占两臂 QEMU 形态，混入任一通道即静默蒸发）。
    if args.trace_dual and (args.el0 or args.factor is not None
                            or args.icount or args.prove):
        raise SystemExit(
            "bench.py: --trace-dual 不与 --el0/--factor/--icount/--prove 组合"
            "（双跑独占两臂 QEMU 形态——icount 家族双臂自含；--factor 会把 L2"
            " 形态降级回可调因子；--prove 只对单臂 --icount 立法）")

    # --- G4-3 --dpg 专属校验（R-1/lat 同族独立通道纪律）：双臂 boot 独占
    # 两 QEMU 形态 + 无基线段——与任一别通道组合 = 其中一条静默蒸发，一律
    # 拒绝（逐组合说因，教学体同上）。
    if args.dpg and (args.update or args.el0 or args.factor is not None
                     or args.rt or args.lat or args.icount or args.prove
                     or args.trace_dual):
        raise SystemExit(
            "bench.py: --dpg 独立通道（R-1 同族只出表无基线）——不与 --update/"
            "--el0/--factor/--rt/--lat/--icount/--prove/--trace-dual 组合"
            "（一进程一种 QEMU 形态 + 本通道双臂 boot 自含独占；D 延迟 = TCG"
            "墙钟面无基线门禁语义，证据档 = 回执落盘；E 断言 d<e 内建于通道）")

    kernel = args.kernel or kernel_elf()
    if args.dpg:
        run_dpg(kernel)
        return
    if args.lat:
        rows = run_lat(kernel)
        print(f"[G3-4(lat)] {'lane':<5} {'n':>4} {'min':>10} {'p50':>10} "
              f"{'p90':>10} {'p99':>10} {'max':>10}")
        for lane in LAT_LANES:
            n, mn, p50, p90, p99, mx = rows[lane]
            print(f"{'':<8} {lane:<5} {n:>4} {mn:>10} {p50:>10} "
                  f"{p90:>10} {p99:>10} {mx:>10}")
        print("bench.py: bench-lat 只出表不对拍（醒因/唤醒数/负延迟严格断言在"
              "内核侧 L0；wake→run 分布数值统计归统计——R-1 公约 2 同族；G3 "
              "阶段对照数值承 tools/evidence 留档）")
        return
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
    if args.trace_dual:
        metrics_on, _it_on, wake = run_icount(kernel)
        metrics_off, _it_off, mark = run_icount(kernel, trace_off=True)
        if set(metrics_on) != set(metrics_off):
            raise SystemExit(
                f"bench.py: 双臂基准项集合不同"
                f"（{sorted(set(metrics_on) ^ set(metrics_off))}）——同 ELF 同 "
                "bench 面两臂必同集，收割面破碎")
        print(f"[G3-4(trace-dual)] {'name':<16} {'off':>10} {'on':>10} {'delta':>8}")
        bad: list[str] = []
        for name in sorted(metrics_on):
            off, on = metrics_off[name], metrics_on[name]
            delta = on - off
            print(f"{'':<8} {name:<16} {off:>10} {on:>10} {delta:>+8}")
            # 帽 = 双侧（L2 教义：确定性通道双向漂移都是指令流变化——on 臂
            # 「变快」同样异常）；off=0 面 = 零混非零即无界漂移（prove 零语义
            # 同口径）。
            if off == 0:
                if on != 0:
                    bad.append(f"{name}: off=0 → on={on}（off 臂 0 出非零——"
                               "量化粒度级无界漂移）")
            elif abs(delta) > off * TRACE_CAP:
                bad.append(f"{name}: off={off} → on={on}（δ={delta:+}，出帽 "
                           f"|δ| ≤ off×TRACE_CAP={off * TRACE_CAP:g}——trace 门"
                           "泄漏进前调度器路径〔gate 面 bug，报警〕，或 bench 面"
                           "已迁入调度器纪元〔合法演进——须重标定 TRACE_CAP，"
                           "见其注释〕")
        print(f"[G3-4(trace-dual)] on 臂观测行：{wake}")
        print(f"[G3-4(trace-dual)] off 臂替代标记行：{mark}")
        # 双跑汇总回执（f4 F1）：两臂数值 + 帽 verdict + 违约单归档（两臂
        # 原始串口在各自 icount/icount-off 回执；update 刷新行随 stdout）。
        write_receipt(
            "trace-dual", "",
            f"on-wake-line: {wake}\noff-mark-line: {mark}\n"
            + "\n".join(
                f"{name} off={metrics_off[name]} on={metrics_on[name]}"
                f" delta={metrics_on[name] - metrics_off[name]:+}"
                for name in sorted(metrics_on))
            + f"\ncap: TRACE_CAP={TRACE_CAP} bad={len(bad)}"
            + ("\n" + "\n".join(f"BAD {b}" for b in bad) if bad else ""))
        if args.update:
            if bad:
                # 帽不过不许刷基线（--icount 单臂无此交点——单臂 update 只是
                # 数值记录；双跑的 update 是**门禁面**：把违约数值写进基线 =
                # 把报警器拆了）。
                for b in bad:
                    print(f"bench.py: FAIL {b}", file=sys.stderr)
                raise SystemExit(1)
            save_baseline_icount(metrics_on)
            save_baseline_icount_off(metrics_off)
            print(f"bench.py: 双份基线已刷新 → "
                  f"{BASELINE_ICOUNT.relative_to(REPO)} + "
                  f"{BASELINE_ICOUNT_OFF.relative_to(REPO)}")
            return
        if bad:
            for b in bad:
                print(f"bench.py: FAIL {b}", file=sys.stderr)
            raise SystemExit(1)
        print(f"bench.py: trace 双跑关-开差过帽（TRACE_CAP={TRACE_CAP}，"
              f"{len(metrics_on)} 行全在界内）")
        base_on = load_baseline_icount()
        base_off = load_baseline_icount_off()
        if not base_on or not base_off:
            missing = []
            if not base_on:
                missing.append(str(BASELINE_ICOUNT.relative_to(REPO)))
            if not base_off:
                missing.append(str(BASELINE_ICOUNT_OFF.relative_to(REPO)))
            print(f"bench.py: 基线缺席（{'、'.join(missing)}）——仅出表+帽；"
                  "`python3 tools/bench.py --trace-dual --update` 立双份")
            return
        l2 = _l2_bad("on", metrics_on, base_on) + _l2_bad("off", metrics_off, base_off)
        if l2:
            for b in l2:
                print(f"bench.py: FAIL {b}", file=sys.stderr)
            raise SystemExit(1)
        print(f"bench.py: 双臂 L2 对拍过（ICOUNT_TOL={ICOUNT_TOL}，on/off "
              f"两家族 {len(base_on) + len(base_off)} 项全在界内）")
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
