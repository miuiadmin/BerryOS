#!/usr/bin/env bash
# BerryOS S-2b 副核参与冒烟——八相（`SMP-S2实施设计草案.md` §5.1 起，相
# 3-8 = S-3b/S-4/S-5b/S-5c 逐相增扩；smoke-x86 同款独立形态——不并入主 smoke、
# 不进 CI：主 smoke 八跑是单核回归基线，零扰动）。相 1 = 裸相（32 标记 +
# SMP 握手）；相 2 = rootfs 生态相（51 标记 + 对拍 client 对 + PAR 副核参
# 与活证）——rootfs 全链在 `-smp 2` 下绿 = E0「内核串行/用户并行」语义
# 等价的端到端证据；相 3-8 = 专项门（LB/ASIDR/TLBS/ALLOC/HT/PP——各独立
# boot，断言面见各相段注释与 `docs/规则/07-测试与冒烟.md`）。
#
# **-smp 只进本线**：全仓带 `-smp 2` 的 QEMU 入口 = 本线（justfile run-smp /
# smoke-smp 本脚本）+ S-5d soak-smp 长跑单口（tools/soak-smp.sh，30min 级
# 不进 smoke——D1 裁决独立通道）；主 run/run-el2/run-rootfs/smoke 一律单核
#（S-1 单核零漂移硬线）。默认机型 + `-kernel` ELF 直加载（conduit 分型：HVC
# 只在默认机型被 QEMU 拦截为 PSCI——virtualization=on 下 HVC 从 EL1 陷阱到
# EL2 且不再拦截，VBAR_EL2=0 → PC=0 静默挂死，内核侧 BOOT_EL 门先行跳过）。
#
# 断言面（状态谓词，非竞速）：
#   1. `SMP ok ncores=2` 恰一次**按相各自成立**（两相两次 boot 各恰一次；
#      grep -cF 全串 = 1）——副核 PSCI CPU_ON → §3.2 十项 → hb1 → 主核
#      dump → release → hb2 握手全闭环的机器断言（两心跳都是内存状态断言，
#      非时序竞速——4-5a 纪律）；反向断言 `SMP: FAIL` / `SMP: skip` 不出现。
#      S-3a probe ② 起 + `SGI ok n=` 恰一次（CPU0→CPU1 SGI_RESCHEDULE 单发
#      → 副核消费臂收讫计数的端到端机器断言；`SGI ok` 归 smp_assertions
#      不进标记数组——主 smoke 单核结构性不触发，零漂移）。S-4 4-1 起 +
#      `SGI2 ok n=` 恰一次（CPU0→CPU1 SGI_TLB_SHOOTDOWN 单发 → 副核收端臂
#      全量失效+回执计数；同归 smp_assertions——前缀 SGI2/SGI3 与 `SGI ok`
#      互不子串、断言面零碰撞，单核相 TX 恒 0 主 smoke 零漂移）。
#   2. 相 1 裸相 32 标记 / 相 2 rootfs 相 51 标记全部出现（与主 smoke 同面
#      ——标记数组与 smoke.sh 运行时计数对拍，见下方机器守卫）。
#   3. switches 金丝雀 ≥3 条严格递增**按相**（任核 tick 命中 100 倍数即整行
#      持 PRINT_LOCK 打印——两核交替打印不破坏数值序列单调，§5.2）。
#   4. 相 2 专属：`ROOTFS vmo=none` 拒 + `app APPDEMO up` + EVS/SKT 宿主
#      对拍 client 完成行 + **PAR 副核参与活证**（裁决翻转 2026-10-31：E0
#      无 IPI + park 派发限频下真并行 witness 归 S-3——验收面 = PAR absent
#      行 `d1=`（副核 park 派发计数）非零；PAR ok 提前命中 = 超预期收益，
#      放行不拒）。
#   5. G-ver-2（版本管理标准 §3/§6）相 1 裸相 banner 门（审计洞③补面：
#      本线原无 banner 断言）：形态正则 + ident 非 nogit 拒——smoke.sh
#      主路同正则同形逐字复制。
#
# 时限：双核 MTTCG 吞吐 ≈ 单核 0.61x（S-0 B 相）——一切帽 ×2 起步铁律
# （boot 120→240s / 金丝雀 180→400s / rootfs 420→840s）。**R3 首跑定标
# 义务**：脚本尾打印三数（boot 到齐墙钟 / 金丝雀行周期 / rootfs 相到齐
# 墙钟），任一数 > 帽 70% 即预授权 ×1.5 升档一次（只改一处定值——升档后
# 仍超 = R1 审计而非再升，§6 R3）。
#
# QEMU 生命周期同 smoke.sh 纪律：后台拉起 + 按记录 PID kill（多会话共享
# 本机，禁 pkill）；kill 后 wait + sleep 3（SIGTERM 后 QEMU 仍存活数秒
# ——P6 A-6 实证）。盘名专属 `disk.smp.img`（soak/B4 先例——防 mkfat
# 原地覆写兄弟线正挂载的活盘）；echod 起前 lsof 四端口让场检查
# （18080-18083——pgrep-only 场地等待盖不到「echod 已起 QEMU 未起」窗）。
set -u

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR/.."
source "$SCRIPT_DIR/paths.sh"   # 非交互 PATH 修补（同 smoke.sh）

# 内核路径 = 必传首参（just smoke-smp 经 KERNEL 注入——单一事实源，勿在本
# 文件硬编码第二份；smoke-x86 对抗审查实抓过的漂移面）。
KERNEL=${1:?用法：just smoke-smp（或显式传参：bash tools/smoke-smp.sh <内核 ELF>）}
QEMU=${QEMU:-qemu-system-aarch64}
DEADLINE_BOOT=240      # 秒：相 1 boot 标记 + SMP 握手到齐（单核 120s ×2）
DEADLINE_CANARY=400    # 秒：等 ≥3 条 switches 金丝雀（单核 180s ×2；双核
                       # MTTCG 0.61x 吞吐 + 金丝雀行周期变长）
DEADLINE_ROOTFS=840    # 秒：相 2 rootfs 相标记到齐（主 smoke 420s ×2）
DEADLINE_PAR=300       # 秒：PAR 行（ok/absent）落窗轮询——探针预算 240s
                       # （120 轮 × 2s）+ 余量；裸相窗结构性打不出（探针预算
                       # ≥ 相窗），断言只挂 rootfs 相
DEADLINE_LB=240        # 秒：相 3 `LB ok` 行轮询帽（F-A7 帽序②：锚点在标记
                       # 轮询后起算——boot 墙钟不占帽）。guest 侧分层：子收割
                       # 帽 90s（义务⑤帽序③,与 240 禁打平竞速）×2 TCG 系数
DEADLINE_ASIDR=240     # 秒：相 4 `ASIDR ok` 行轮询帽（DEADLINE_LB 同款帽序②；
                       # 任务四步全快路径——快进/取号/广播/回执等待,无收割长跑）
DEADLINE_TLBS=240      # 秒：相 5 `TLBS ok` 行轮询帽（帽序②同前；guest 侧
                       # 分层：warm 栅障 8s + canary 收割 8s 双帽（TCG 半速
                       # ≈ 16s 墙钟 ×2）+ spawn/风暴余量——两帽之和远小于本帽）
DEADLINE_STORM=240     # 秒：相 6 `ALLOC ok` / 相 7 `HT ok` / 相 8 `PP ok`
                       # 行轮询帽（S-5b/c，帽序②同前；guest 侧 budget 8s×2
                       # TCG + spawn 余量——64 轮 × 5 人 churn / 320 轮
                       # ping-pong 对打均在 8s budget 内对账）
LB_TICK_TH=24          # 相 3 c1 下限（F-A5 定标 run10：4 线程 K=2 全改道 rq1
                       # 形态实测 c1=77 ≈ 每 16s 边 ~40 tick 量级;24 ≈ 1/3——
                       # 区分「副核得货死（c1=0）」与「活」,余量防轮转波动）
CLIENT_CMP_CAP=60      # 秒：对拍 client 收割帽（guest 标记已到后的透传余量；
                       # 主 smoke 30s ×2——§5.1「evs/skt 对拍 30→60s」）
# evsclient 帽 env 覆盖（§5.1：TOTAL 480→960「client 帽 ≥ 相帽 + 60s」/
# ATTEMPT 45→90「内帽 60 < 尝试帽」帽序——vnet 内帽整族 ×2 后 45 倒挂）。
export BERRY_EVS_TOTAL_CAP_S=960
export BERRY_EVS_ATTEMPT_CAP_S=90

command -v "$QEMU" >/dev/null 2>&1 || { echo "smoke-smp: FAIL: 找不到 $QEMU" >&2; exit 1; }
[ -f "$KERNEL" ] || { echo "smoke-smp: FAIL: 内核未构建（先 just build）：$KERNEL" >&2; exit 1; }

# QEMU 严格串行（多会话共享本机）：起跑前等字段空（占位 → 15s 重试，至多
# 5 轮后 loud 退——不抢跑、不 pkill 别人的 run）。两相 QEMU 起前各查一次。
# 匹配面 = 本仓 QEMU 签名（同 bench.py QEMU_GUARD_PATTERN，2026-11-04 收窄：
# aarch64 恒 -machine virt / x86·EFI 恒 -kernel；宿主 Android/AVD 模拟器两签名
# 皆无，不误伤——裸 qemu-system 会把用户模拟器在场误判成字段被占）。
field_wait() {
  local waited=0
  while pgrep -f 'qemu-system.*(-machine virt|-kernel)' >/dev/null 2>&1; do
    waited=$((waited + 1))
    [ "$waited" -gt 5 ] && { echo "smoke-smp: FAIL: QEMU 字段被占（本仓签名 pgrep 非空），放弃" >&2; exit 1; }
    echo "smoke-smp: QEMU 字段被占，等待 15s（第 ${waited} 轮）…" >&2
    sleep 15
  done
}

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-smp.XXXXXX")
# per-boot 串口轮转（smoke.sh 同款刀法，f4 复验 minor C 移植）：每 QEMU
# boot 一份日志，$LOG 恒指当前 boot。此前八相顺序截断同文件——后相抹前相
# 现场（SMOKE_KEEP_LOG 也只余末相）；本刀起各相 launch 前 rotate_log 换档，
# SMOKE_KEEP_LOG=1 时八 boot 各留档、失败全尸逐相保全。
LOGS=("$LOG")
rotate_log() {
  LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-smp.XXXXXX")
  LOGS+=("$LOG")
}
EVS_LOG=""; SKT_LOG=""; ECHO_LOG=""
QEMU_PID=""; EVS_PID=""; SKT_PID=""; ECHO_PID=""

cleanup() {
  local p
  for p in "$QEMU_PID" "$SKT_PID" "$EVS_PID" "$ECHO_PID"; do
    [ -n "$p" ] || continue
    kill "$p" 2>/dev/null
    wait "$p" 2>/dev/null
  done
  # SIGTERM 后 QEMU 仍存活数秒并继续刷串口（P6 A-6 实证）——断言前日志须已
  # 落定，kill+wait 后再 sleep 3 收尾。
  [ -n "$QEMU_PID" ] && sleep 3
  if [ -n "${SMOKE_KEEP_LOG:-}" ]; then
    for lg in "${LOGS[@]}"; do
      echo "smoke-smp: 串口日志保留：$lg" >&2
    done
    for p in "$EVS_LOG" "$SKT_LOG" "$ECHO_LOG"; do
      [ -n "$p" ] && echo "smoke-smp: 客户端日志保留：$p" >&2
    done
  else
    rm -f "${LOGS[@]}"
    rm -f "$EVS_LOG" "$SKT_LOG" "$ECHO_LOG"
  fi
}
trap cleanup EXIT

fail() {
  echo "smoke-smp: FAIL: $1" >&2
  # 失败即自动留全尸（2026-11-01 相 3 时敏 flake 归因教训：tail -40 截掉
  # PC/FAR/SPSR 行 + cleanup 删原件 = 现场不可逆丢失，差分压测被迫盲跑）。
  # 成功路径不受影响（照旧即弃；SMOKE_KEEP_LOG 仍管全量保留）。
  # per-boot 轮转起（f4 复验 minor C）全相留档：逐 boot 各存一份（旧形态
  # 只存当相——前相现场随截断不可逆丢失）。
  local keep i=1 ts
  ts=$(date +%Y%m%d-%H%M%S)
  mkdir -p out 2>/dev/null
  for lg in "${LOGS[@]}"; do
    keep="out/smoke-smp-fail.$ts.p${i}.log"
    cp "$lg" "$keep" 2>/dev/null && echo "smoke-smp: 失败全尸已存：$keep" >&2
    i=$((i+1))
  done
  echo "---- 串口日志尾部（${LOG}）----" >&2
  tail -40 "$LOG" >&2
  # 客户端死因随失败即打印不留文件（smoke.sh 同款纪律）。
  for p in "echod:$ECHO_LOG" "evsclient:$EVS_LOG" "sktclient:$SKT_LOG"; do
    local lg="${p#*:}"
    if [ -n "$lg" ] && [ -s "$lg" ]; then
      echo "---- ${p%%:*} 日志尾部（${lg}）----" >&2
      tail -10 "$lg" >&2
    fi
  done
  echo "--------------------------------" >&2
  exit 1
}

# 标记断言：DEMO ok / TAB ok 特判（smoke.sh 同款——AIDEMO ok 子串混叠 P5
# 实抓；TAGTAB ok 前邻 G 排除 C-4 实抓，真标记前驱可为任意大写矩阵字母
# 故只排 G）；其余裸 grep -qF。
assert_marker() {
  if [ "$1" = "DEMO ok" ]; then
    grep -qE '(^|[^I])DEMO ok' "$LOG"
  elif [ "$1" = "TAB ok" ]; then
    grep -qE '(^|[^G])TAB ok' "$LOG"
  else
    grep -qF "$1" "$LOG"
  fi
}

# 标记单源：双数组与 smoke.sh 共用 tools/markers.sh（S-5 后测试基建收尾
# 刀落——原「复制 + awk 计数对拍」§5.1 最低限度方案退役，漂移面构造性
# 消失；防假绿非空兜底随单源文件内置）。元素约束（不含 `)`/引号）见
# markers.sh 头注。
source "$SCRIPT_DIR/markers.sh"

# SMP 按相断言：SMP ok 恰一次（重复 = 副核路径重入；缺失已由标记轮询覆盖）
# + 副核报告行在席（mpidr/ctx 回显/per-core 寄存器——超时 dump 的诊断源；
# 哨兵 's' 紧邻其前 = UART 单写者纪律的可观测面）。
smp_assertions() {
  local label="$1" n
  n=$(grep -cF "SMP ok ncores=2" "$LOG" || true)
  [ "$n" -eq 1 ] || fail "$label SMP 标记非恰一次（n=${n}）：SMP ok ncores=2"
  grep -q "SMP: sec up" "$LOG" || fail "$label 缺副核报告行 SMP: sec up"
  # S-3a probe ②：SGI 收讫标记恰一次（超时面已由 `SMP: FAIL` 反向快败覆盖）。
  n=$(grep -cF "SGI ok n=" "$LOG" || true)
  [ "$n" -eq 1 ] || fail "$label SGI ok 非恰一次（n=${n}）——S-3a SGI_RESCHEDULE 收讫链断（发/投/收讫计数三面查）"
  # S-4 4-1 probe ④：TLB shootdown 回执标记恰一次（超时面同由 `SMP: FAIL`
  # 反向快败覆盖——`SMP: FAIL sgi2 timeout` 同前缀）。
  n=$(grep -cF "SGI2 ok n=" "$LOG" || true)
  [ "$n" -eq 1 ] || fail "$label SGI2 ok 非恰一次（n=${n}）——S-4 4-1 SGI #2 回执链断（banked 使能/发送/投递/收端全量失效/回执计数四面查）"
}

# 标记轮询公共体：QEMU 活着 + SMP: FAIL/skip 反向快败 + （可选）SMP ok
# 首现 + 数组逐项到齐；超时 FAIL 带缺失清单。
poll_markers() {
  local dl="$1" label="$2" with_smp="$3"; shift 3
  local -a ms=("$@")
  local t0 missing m
  t0=$(date +%s)
  while :; do
    kill -0 "$QEMU_PID" 2>/dev/null \
      || fail "QEMU 提前退出（${label}）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
    grep -q "SMP: FAIL" "$LOG" && fail "SMP 握手自报 FAIL（${label}）：$(grep 'SMP: FAIL' "$LOG" | tail -1)"
    grep -q "SMP: skip" "$LOG" && fail "SMP 被跳过（${label}——错机型/错旗？）：$(grep 'SMP: skip' "$LOG" | tail -1)"
    missing=""
    if [ "$with_smp" = "1" ]; then
      grep -qF "SMP ok ncores=2" "$LOG" || missing="SMP ok ncores=2"
    fi
    for m in "${ms[@]}"; do
      assert_marker "$m" || missing="${missing:+$missing }$m"
    done
    [ -z "$missing" ] && break
    [ $(( $(date +%s) - t0 )) -gt "$dl" ] && fail "$label 标记 ${dl}s 未到齐，缺：$missing"
    sleep 1
  done
}

# 金丝雀公共体：≥3 条严格递增（awk 体沿主 smoke）+ R3 定标数②（行周期
# 均值 = 首→末墙钟 / 值跨度——观测面非断言面）。
canary_check() {
  local label="$1" dl="$2"
  local t_first="" v_first=0 v_last=0 n tc
  tc=$(date +%s)
  while :; do
    n=$(grep -c 'switches=' "$LOG" || true)
    if [ "${n:-0}" -ge 1 ] && [ -z "$t_first" ]; then
      t_first=$(date +%s)
      v_first=$(grep -o 'switches=[0-9]*' "$LOG" | head -1 | cut -d= -f2)
    fi
    [ "${n:-0}" -ge 3 ] && break
    kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在金丝雀阶段退出（${label}）"
    [ $(( $(date +%s) - tc )) -gt "$dl" ] \
      && fail "$label 金丝雀 ${dl}s 内不足 3 条（现有 ${n:-0}）"
    sleep 2
  done
  grep -o 'switches=[0-9]*' "$LOG" | cut -d= -f2 | awk '
    { if (NR>1 && $1<=prev) bad=1; prev=$1; n=NR }
    END { exit (bad || n<3) ? 1 : 0 }
  ' || fail "$label switches 序列非严格递增：$(grep -o 'switches=[0-9]*' "$LOG" | tr '\n' ' ')"
  v_last=$(grep -o 'switches=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)
  if [ -n "$t_first" ] && [ "$v_last" -gt "$v_first" ]; then
    CAN_PERIOD_S=$(( ( $(date +%s) - t_first ) / (v_last - v_first) ))
    echo "smoke-smp: $label 金丝雀行周期 ≈ ${CAN_PERIOD_S}s/行（值 ${v_first}→${v_last}）"
  fi
}

# —— 对拍 client 对 + echod（smoke.sh 同款生命周期，帽随 §5.1 ×2）——
evs_client_start() {
  EVS_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-evsclient.XXXXXX")
  python3 tools/evsclient.py 18082 EVS > "$EVS_LOG" 2>&1 &
  EVS_PID=$!
}
evs_client_wait() {
  local label="$1" tc
  tc=$(date +%s)
  while ! grep -q 'EVS client ok n=2' "$EVS_LOG" 2>/dev/null; do
    kill -0 "$EVS_PID" 2>/dev/null \
      || fail "evsclient 提前退出（${label}）：$(tail -5 "$EVS_LOG" | tr '\n' ' ')"
    [ $(( $(date +%s) - tc )) -gt "$CLIENT_CMP_CAP" ] \
      && fail "$label EVS client ${CLIENT_CMP_CAP}s 未对拍完成（guest EVS ok 已到——查 EVS_LOG 重试诊断）"
    sleep 1
  done
  kill "$EVS_PID" 2>/dev/null
  wait "$EVS_PID" 2>/dev/null
  rm -f "$EVS_LOG"
  EVS_PID=""
  EVS_LOG=""
  echo "smoke-smp: $label EVS client 对拍完成（两连接 64B/24B 逐字节）"
}
# sktclient 懒启动（smoke.sh A3-3 首跑实抓形态）：watcher 等 guest `EVS ok`
# 后才 exec——QEMU 前起的 18083 SYN 重试流与 18082 帧同 RX 环排队，拖慢
# EVS 握手可见性；watcher 子壳 exec 成 python：SKT_PID 直杀两形态皆中。
skt_client_start() {
  SKT_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-sktclient.XXXXXX")
  ( until grep -q 'EVS ok' "$LOG" 2>/dev/null; do sleep 2; done
    exec python3 tools/evsclient.py 18083 SKT ) > "$SKT_LOG" 2>&1 &
  SKT_PID=$!
}
skt_client_wait() {
  local label="$1" tc
  tc=$(date +%s)
  while ! grep -q 'SKT client ok n=2' "$SKT_LOG" 2>/dev/null; do
    kill -0 "$SKT_PID" 2>/dev/null \
      || fail "sktclient 提前退出（${label}）：$(tail -5 "$SKT_LOG" | tr '\n' ' ')"
    [ $(( $(date +%s) - tc )) -gt "$CLIENT_CMP_CAP" ] \
      && fail "$label SKT client ${CLIENT_CMP_CAP}s 未对拍完成（guest SKT ok 已到——查 SKT_LOG 重试诊断）"
    sleep 1
  done
  kill "$SKT_PID" 2>/dev/null
  wait "$SKT_PID" 2>/dev/null
  rm -f "$SKT_LOG"
  SKT_PID=""
  SKT_LOG=""
  echo "smoke-smp: $label SKT client 对拍完成（fd 皮 server 段两连接 64B/24B 逐字节）"
}
echod_start() {
  ECHO_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-echod.XXXXXX")
  python3 tools/echod.py > "$ECHO_LOG" 2>&1 &
  ECHO_PID=$!
  local te
  te=$(date +%s)
  while ! grep -q 'listening on' "$ECHO_LOG" 2>/dev/null; do
    kill -0 "$ECHO_PID" 2>/dev/null \
      || fail "echod 启动失败（18080 被占？）：$(cat "$ECHO_LOG")"
    [ $(( $(date +%s) - te )) -gt 5 ] && fail "echod 5s 未 listening"
    sleep 0.2
  done
}
echod_stop() {
  [ -n "$ECHO_PID" ] || return 0
  kill "$ECHO_PID" 2>/dev/null
  wait "$ECHO_PID" 2>/dev/null
  rm -f "$ECHO_LOG"
  ECHO_PID=""
  ECHO_LOG=""
}

# ============ 相 1：裸相（32 标记 + SMP 握手 + 金丝雀） ============

field_wait
echo "smoke-smp: QEMU 启动（${KERNEL}，-smp 2，相 1 裸相）"
t_boot0=$(date +%s)
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

poll_markers "$DEADLINE_BOOT" "裸相" 1 "${MARKERS[@]}"
# G-ver-2（版本管理标准 §3）：boot banner 形态断言——smoke.sh 主路同正则
# 同形逐字复制（审计洞③：SMP 线裸相原无 banner 门）：首匹配行须为
# `BerryOS boot <semver> (<ident>[ rustc<ver> dbg|rel])`。上方 poll_markers
# 的 MARKERS[0] "BerryOS boot" 子串断言原样保留——本行只加形态边不松子串
# 边；置轮询到齐后 = banner 行必已在席，断言零竞态。挂相 1 一处即足：八
# 相同一 $KERNEL 同一 banner，逐相重断言零增量鉴别力。
grep -m1 'BerryOS boot' "$LOG" | grep -Eq 'BerryOS boot [0-9]+\.[0-9]+\.[0-9]+ \([A-Za-z0-9.-]+( rustc[A-Za-z0-9.?]+ (dbg|rel))?\)' \
  || fail "boot banner 形态缺：首匹配行应为 'BerryOS boot <semver> (<ident>[ rustc<ver> dbg|rel])'（版本管理标准 §3 G-ver-2）；实得：$(grep -m1 'BerryOS boot' "$LOG" || echo '（日志无 BerryOS boot 行）')"
# 标准 §6 执法化（审计洞②：原纯人律无机器门）：ident 非 nogit 拒——nogit
# 标识产物不得对外发布（无归因能力的构建仅限本地调试），冒烟门是发布链
# 前闸，nogit 构建过门即违规。门 = 首 banner 行含 nogit 即红（行内唯一可
# 含该串的段 = ident——semver/rustc/profile 段均数字与固定词）；与
# smoke.sh 主路同形同正则。
grep -m1 'BerryOS boot' "$LOG" | grep -q 'nogit' \
  && fail "boot banner ident 为 nogit：nogit 标识产物不得对外发布/进发布链（版本管理标准 §6——无归因能力的构建仅限本地调试）；实得：$(grep -m1 'BerryOS boot' "$LOG")"
SMP_BOOT_WALL=$(( $(date +%s) - t_boot0 ))
smp_assertions "相 1"
echo "smoke-smp: 相 1 SMP 握手 + boot 标记 ${#MARKERS[@]}/${#MARKERS[@]} 到齐（墙钟 ${SMP_BOOT_WALL}s）"

canary_check "相 1" "$DEADLINE_CANARY"

# S-3c c-2c KL 争用观测（非门禁）：每相末条金丝雀 kl=（C20 收缩前后对照基线）。
# 门控相（4-8）常缺 = 设计内：canary 仅在相内全局切换过 SOAK_PRINT_EVERY=100
# 倍数时打行——门控压测相总切换 <100 无行可取（S-5f 相 8 留档串口全程
# 0 行 canary 实证）；观测面非门禁，缺 ≠ 断链。
kl_last=$(grep -o ' kl=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)
echo "smoke-smp: 相 1 KL 争用观测（非门禁）：kl=${kl_last:-缺(切换<100)}"

kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
QEMU_PID=""

# ============ 相 2：rootfs 生态相（50 标记 + 对拍 client + PAR 活证） ============

# rootfs 卷前置检查（§5.1 随迁件②：justfile smoke-smp 已挂 build-user 依赖；
# 手跑裸脚本时缺产物 = loud 快败，不烧 840s DEADLINE 才红）。
[ -f rootfs.cpio ] || fail "缺 rootfs.cpio（build-user 应产出——先 just build-user / just smoke-smp）"
# fw_cfg 多卷发现环（§5.1 随迁件①，smoke.sh 同款：rootfs0..9 连续前缀拼接，
# 缺席回落旧名单单卷——mkcpio 主路恒走 --split 32 多卷产链，rootfs0.cpio
# 已达单卷帽 32MiB 九成，硬编码单卷名随时踩墙）。
FWCFG=""
vi=0
while [ -f "rootfs$vi.cpio" ]; do
  FWCFG="$FWCFG -fw_cfg name=opt/berry/rootfs$vi,file=rootfs$vi.cpio"
  vi=$((vi+1))
done
[ -n "$FWCFG" ] || FWCFG="-fw_cfg name=opt/berry/rootfs,file=rootfs.cpio"

# 盘名专属 disk.smp.img（§5.1 盘/端口行：防覆写兄弟线活盘；每跑重造 = 跨
# run 零残留）。LBA0 印章 = vblk LBA0 对拍锚（smoke.sh 同款）。
python3 tools/mkfat.py disk.smp.img || fail "mkfat 造盘失败（disk.smp.img）"
printf 'BERRYOSVBLK!' | dd of=disk.smp.img conv=notrunc status=none

# lsof 四端口让场（§5.1：18080-18083 任一 LISTEN = 兄弟线正用端口族——
# pgrep-only 场地等待盖不到「echod 已起 QEMU 未起」窗；端口与主 smoke 同值
# → 两线天然串行协议）。
if lsof -nP -iTCP:18080-18083 -sTCP:LISTEN >/dev/null 2>&1; then
  fail "18080-18083 端口族被占（lsof）——兄弟 smoke/soak 线在跑？两线串行协议，让场后重试"
fi
echod_start

rotate_log   # 日志分相换档（smoke.sh 同款 per-boot 轮转——后相不再抹前相现场）
evs_client_start
skt_client_start

field_wait
echo "smoke-smp: QEMU 启动（-smp 2，相 2 rootfs 相：fw_cfg rootfs 卷 + virtio-blk disk.smp.img + virtio-net hostfwd 三口）"
t_rootfs0=$(date +%s)
# GUI-4 段②：+virtio-tablet（id=tab0，无 QMP——INPUTP 注入往返不属本线；
# tablet 在席 = INPUT ok 照打，数组同步后断言恰好覆盖）。
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  $FWCFG \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.smp.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082,hostfwd=tcp::18083-:18083 -device virtio-net-device,netdev=n0 \
  -device virtio-tablet-device,id=tab0 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# with_smp=1：SMP ok 首现与标记同轮询——握手静默缺失（无 FAIL 行）时快败，
# 不让「单核照跑生态链」的 run 烧满 840s 才被 smp_assertions 抓到。
poll_markers "$DEADLINE_ROOTFS" "rootfs 相" 1 "${ROOTFS_MARKERS[@]}"
ROOTFS_WALL=$(( $(date +%s) - t_rootfs0 ))

smp_assertions "相 2"

# vmo 行须非降级形态（=none 即 seam 断：内核没把 rootfs 包进 VMO）。
grep -q 'ROOTFS vmo=none' "$LOG" \
  && fail "ROOTFS vmo=none（rootfs 在但 VMO 交付缺席——入机侧 seam 断）"
# app-0 a-2 证据层：APPDEMO（console=true）子自证输出（能力面断言随迁）。
grep -q 'app APPDEMO up' "$LOG" \
  || fail "app APPDEMO up 缺失（console=true 能力面断——appdemo 未见自证输出；查 eco: app 行）"

evs_client_wait "rootfs 相"
skt_client_wait "rootfs 相"

# —— PAR 副核参与活证（裁决翻转 2026-10-31：真并行 witness 归 S-3）——
# 探针 240s 预算（120 轮 × 2s）必落 rootfs 相窗内（裸相窗同长结构性打不出
# ——fix6 首跑实证）；absent 行诊断尾 `d1=` = 副核 park 派发计数，非零 =
# 参与活证（不要求满速——满速/真并行归 S-3 IPI 面）。PAR ok 提前命中 =
# witness 超预期收益，放行（其机器断言归 S-3 harness）。
tc=$(date +%s)
while ! grep -qE 'PAR (ok|absent)' "$LOG"; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 PAR 轮询阶段退出"
  [ $(( $(date +%s) - tc )) -gt "$DEADLINE_PAR" ] \
    && fail "PAR 行 ${DEADLINE_PAR}s 未出（探针 240s 预算应已落窗——查 boot 期 PAR gate on 是否在席）"
  sleep 2
done
if grep -q 'PAR ok c0=' "$LOG"; then
  echo "smoke-smp: PAR ok 提前命中（witness）——$(grep 'PAR ok c0=' "$LOG" | tail -1)"
else
  d1=$(grep 'PAR absent' "$LOG" | tail -1 | grep -o ' d1=[0-9]*' | cut -d= -f2)
  [ -n "$d1" ] && [ "$d1" -gt 0 ] \
    || fail "PAR absent 行 d1=${d1:-缺}（副核 park 派发零活证——d1=0 = 副核从未摘入任务，S-2b 参与面破损）"
  echo "smoke-smp: PAR absent d1=${d1}（副核参与活证 ≥1 派发；真并行 witness 归 S-3）"
fi

canary_check "相 2" "$DEADLINE_CANARY"

# S-3c c-2c KL 争用观测（非门禁）：每相末条金丝雀 kl=（C20 收缩前后对照基线）。
# 门控相（4-8）常缺 = 设计内：canary 仅在相内全局切换过 SOAK_PRINT_EVERY=100
# 倍数时打行——门控压测相总切换 <100 无行可取（S-5f 相 8 留档串口全程
# 0 行 canary 实证）；观测面非门禁，缺 ≠ 断链。
kl_last=$(grep -o ' kl=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)
echo "smoke-smp: 相 2 KL 争用观测（非门禁）：kl=${kl_last:-缺(切换<100)}"

kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
QEMU_PID=""
echod_stop

# ============ 相 3：LB 均衡验收（S-3b b-2 义务④；gated bare 第三 boot） ============
# fw_cfg `opt/berry/lb` 门控（RT/SOAK/PAR 同款机制——kernel boot 期 find_file
# 在位才 spawn lb_master + 注入 set_lb_match 判别 + 打 `LB gate on` 自证行）。
# 机制语义（子案 §9.6 + 9.2 裁决二）：lb_master 真 svc 链造 **单进程五任务**
# ——root 经 process_start（spread=false 恒家核,保进程 Alive/started）+ 4
# 计算线程经 thread_start（spread=true → place K=2 深度失衡**直路路由** rq1
# + SGI 醒 park 环；偷取为第二路径）→ wait_many（Thread TERMINATED）收割
# → `LB ok c0=Δ c1=Δ st1=Δ n=4`。直路与偷取同格合格（裁决二——路由形态
# 与断言解耦），tick 采样按 thread_key 归因（root 键天然排除）；两路径全坏
# 时 4 子单核墙钟串行跑完 n=4 仍凑齐——真断言在 c1 ≥ TH（副核得货,c0 为
# 观测面：K=2 按各环实深改道,bare boot rq0 常驻噪声 → 全落 rq1 时 c0 恒 0,
# run10 定谳）。

field_wait
rotate_log   # F-A12 跨 boot 混日志防线 + per-boot 轮转：相 3 换档
echo "smoke-smp: QEMU 启动（-smp 2，相 3 LB 相：fw_cfg lb gate + bare）"
t_lb0=$(date +%s)
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/lb,string=1 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# F-A7 帽序②：DEADLINE_LB 锚点在标记轮询**后**起算（boot 时间不占帽）。
tc=$(date +%s)
while ! grep -q 'LB ok c0=' "$LOG"; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 LB 轮询阶段退出"
  [ $(( $(date +%s) - tc )) -gt "$DEADLINE_LB" ] \
    && fail "LB ok ${DEADLINE_LB}s 未出（查 boot 期 LB gate on 是否在席——absent 即一票 FAIL，F-A4）"
  sleep 2
done
LB_WALL=$(( $(date +%s) - t_lb0 ))

# F-A5 数值复验（run10 定谳形态）：宿主 grep -E 提 c0/c1/st1/n——**c0==0 且
# c1 ≥ TH 且 n==4**（裁决二「断言与路由形态解耦」：K=2 深度路由按各环实深
# 改道,bare boot rq0 常驻字母噪声 → 4 计算线程全落空队 rq1,c0 结构性 0——
# 升硬门（路由形态漂移金丝雀,零漂声称执法化）;c1 ≥ TH 才证副核得货（直路/
# 偷取同格）,n=4 单独放行 = 零成本假绿;st1 依裁决二维持诊断位不进门禁。
# LB 行格式三面钉同值）。
lb_line=$(grep 'LB ok c0=' "$LOG" | tail -1)
echo "smoke-smp: ${lb_line}"
lb_c0=$(echo "$lb_line" | grep -o 'c0=[0-9]*' | cut -d= -f2)
lb_c1=$(echo "$lb_line" | grep -o 'c1=[0-9]*' | cut -d= -f2)
lb_n=$(echo "$lb_line" | grep -o 'n=[0-9]*' | cut -d= -f2)
[ -n "$lb_c0" ] && [ "$lb_c0" -eq 0 ] \
  || fail "LB c0=${lb_c0:-缺} ≠ 0（裸 boot rq0 路由形态漂移——结构性零金丝雀失守,F-A4）"
[ -n "$lb_c1" ] && [ "$lb_c1" -ge "$LB_TICK_TH" ] \
  || fail "LB c1=${lb_c1:-缺} < TH=${LB_TICK_TH}（副核得货活证缺失——直路/偷取两路径同格破损（裁决二）,n=4 凑齐不救,F-A4）"
[ -n "$lb_n" ] && [ "$lb_n" -eq 4 ] \
  || fail "LB n=${lb_n:-缺} ≠ 4（收割计数漂移——LB_CHILD_N 三面钉同值）"

# S-3c c-2c KL 争用观测（非门禁）：每相末条金丝雀 kl=（C20 收缩前后对照基线）。
# 门控相（4-8）常缺 = 设计内：canary 仅在相内全局切换过 SOAK_PRINT_EVERY=100
# 倍数时打行——门控压测相总切换 <100 无行可取（S-5f 相 8 留档串口全程
# 0 行 canary 实证）；观测面非门禁，缺 ≠ 断链。
kl_last=$(grep -o ' kl=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)
echo "smoke-smp: 相 3 KL 争用观测（非门禁）：kl=${kl_last:-缺(切换<100)}"

kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
QEMU_PID=""

# ============ 相 4：ASID 回卷 quiesce 验收（S-4 4-3；gated bare 第四 boot） ============
# fw_cfg `opt/berry/asidroll` 门控（lb 同款机制）：boot 期双门过（smp_active
# + 具名项在席）→ spawn task_asidroll（①force_near_roll 快进 ②alloc 真回卷
# ——ASID 锁内代数 bump + 锁外武装（本核 vmalle1is + SGI #2 广播置闩）
# ③显式 roll_quiesce_check 驱动（IRQ 开零锁语境内联等待——内核任务无
# dispatch 尾等待点的等价面）④断言代数/回执前进）→ `ASIDR ok n=`（n = 全核
# 回执增量,≥1 = 对核已为本轮回卷做过全量失效——回卷 quiesce 回执链活证）。
# 断言面 = gate 自证行 + ok 行在席 + n≥1（bare boot 单任务单发,无相 1..3
# 恰一次复杂度）。锁外等待纪律（§3.2）与对飞面记录 → tlbshoot.rs 模块注。

field_wait
rotate_log   # F-A12 跨 boot 混日志防线 + per-boot 轮转：相 4 换档
echo "smoke-smp: QEMU 启动（-smp 2，相 4 ASID 回卷相：fw_cfg asidroll gate + bare）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/asidroll,string=1 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# F-A7 帽序②同款：锚点在标记轮询后起算。
tc=$(date +%s)
while ! grep -q 'ASIDR ok n=' "$LOG"; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 ASIDR 轮询阶段退出"
  [ $(( $(date +%s) - tc )) -gt "$DEADLINE_ASIDR" ] \
    && fail "ASIDR ok ${DEADLINE_ASIDR}s 未出（查 boot 期 ASIDR gate on 是否在席——absent 即门未过,guest 断言炸则有 panic 行）"
  sleep 2
done
grep -q 'ASIDR gate on' "$LOG" \
  || fail "ASIDR gate on 自证行缺失（门未过——查 fw_cfg 注入与 smp_active 面）"
asidr_line=$(grep 'ASIDR ok n=' "$LOG" | tail -1)
echo "smoke-smp: ${asidr_line}"
asidr_n=$(echo "$asidr_line" | grep -o 'n=[0-9]*' | cut -d= -f2)
[ -n "$asidr_n" ] && [ "$asidr_n" -ge 1 ] \
  || fail "ASIDR n=${asidr_n:-缺} < 1（全核回执零前进——回卷 quiesce 回执链断：查 SGI #2 面/对飞记录）"

# S-3c c-2c KL 争用观测（非门禁）：每相末条金丝雀 kl=（C20 收缩前后对照基线）。
# 门控相（4-8）常缺 = 设计内：canary 仅在相内全局切换过 SOAK_PRINT_EVERY=100
# 倍数时打行——门控压测相总切换 <100 无行可取（S-5f 相 8 留档串口全程
# 0 行 canary 实证）；观测面非门禁，缺 ≠ 断链。
kl_last=$(grep -o ' kl=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)
echo "smoke-smp: 相 4 KL 争用观测（非门禁）：kl=${kl_last:-缺(切换<100)}"

kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
QEMU_PID=""

# ============ 相 5：TLB shootdown canary 压测（S-4 4-4；gated bare 第五 boot） ============
# fw_cfg `opt/berry/tlbs` 门控（lb/asidroll 同款机制）：boot 期双门过 →
# spawn task_tlbs——canary 五步 × 双轮（SMP-S4 草案 §2 4-4 冻结规格
# 「decommit 与 Drop 双构造」）：每轮①预热（root 家核 + 4 写者
# spread→CPU1 warm str 旧 VA 驻留 TLB + probe fault 汇报）②warm 栅障
# （Δprobe == 5 + 跨核断言）③unmap page0 + 还帧（PT 锁 + 本核 aside1 +
# SGI #2 广播——SVC dispatch 尾等回执,svc 返回即全核失效毕；round A =
# vmo_decommit（VMO 活）/ round B = handle_close→Drop（与生产 teardown
# 同机制））④复用（新 VMO 吸收还帧,不回 map 同 VA）⑤放行 flag → 写
# 旧 VA 期望 fault（Δcanary 恰 == 5 + Δhits[1][0]≥1 = 对核 stale 已被
# 广播打掉）。SVC 风暴面 = 写者轮询环每迭代一次 clock_read（对核满载荷
# 中发 shootdown）。变异标定（去 tlbi 证 canary 真 红）= 一次性验收步,
# 不入常驻 smoke（结论入判例兼 §5.1 答案）。
# 断言面 = gate 自证行 + ok 行在席 + n==10（2 轮 × 5 参与者,
# 与 main.rs TLBS_PARTICIPANTS 三面钉同值）。

field_wait
rotate_log   # F-A12 跨 boot 混日志防线 + per-boot 轮转：相 5 换档
echo "smoke-smp: QEMU 启动（-smp 2，相 5 TLB canary 相：fw_cfg tlbs gate + bare）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/tlbs,string=1 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# F-A7 帽序②同款：锚点在 QEMU 起后起算。
tc=$(date +%s)
while ! grep -q 'TLBS ok n=' "$LOG"; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 TLBS 轮询阶段退出"
  [ $(( $(date +%s) - tc )) -gt "$DEADLINE_TLBS" ] \
    && fail "TLBS ok ${DEADLINE_TLBS}s 未出（查 boot 期 TLBS gate on 是否在席——absent 即门未过,guest 断言炸则有 panic 行）"
  sleep 2
done
grep -q 'TLBS gate on' "$LOG" \
  || fail "TLBS gate on 自证行缺失（门未过——查 fw_cfg 注入与 smp_active 面）"
tlbs_line=$(grep 'TLBS ok n=' "$LOG" | tail -1)
echo "smoke-smp: ${tlbs_line}"
tlbs_n=$(echo "$tlbs_line" | grep -o 'n=[0-9]*' | cut -d= -f2)
[ -n "$tlbs_n" ] && [ "$tlbs_n" -eq 10 ] \
  || fail "TLBS n=${tlbs_n:-缺} != 10（双轮 canary 命中不齐 = 有写入成功 = stale 残留 = shootdown 断链：查 SGI #2 广播/收端 vmalle1/unmap/decommit/Drop 路径——变异标定记录见判例）"

# S-3c c-2c KL 争用观测（非门禁）：每相末条金丝雀 kl=（C20 收缩前后对照基线）。
# 门控相（4-8）常缺 = 设计内：canary 仅在相内全局切换过 SOAK_PRINT_EVERY=100
# 倍数时打行——门控压测相总切换 <100 无行可取（S-5f 相 8 留档串口全程
# 0 行 canary 实证）；观测面非门禁，缺 ≠ 断链。
kl_last=$(grep -o ' kl=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)
echo "smoke-smp: 相 5 KL 争用观测（非门禁）：kl=${kl_last:-缺(切换<100)}"

kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
QEMU_PID=""

# R3 首跑定标三数（观测面非断言面；任一数 > 帽 70% 即预授权 ×1.5 一次）。
echo "smoke-smp: R3 定标三数：boot=${SMP_BOOT_WALL}s（帽 ${DEADLINE_BOOT}s）/ 金丝雀≈${CAN_PERIOD_S:-?}s/行 / rootfs=${ROOTFS_WALL}s（帽 ${DEADLINE_ROOTFS}s）"

# ============ 相 6：ALLOC churn 压测（S-5b；gated bare 第六 boot） ============
# fw_cfg `opt/berry/alloc` 门控（前相同款机制）：boot 期双门过 → spawn
# task_alloc——双核 EL0 分配器 churn：root + 4 写者（spread→CPU1）×
# 64 轮 vmo create/commit/write/close（模板 __user_allocst，SP 自推导
# 窗口 + go-flag 基线语义）。验收 = err 全 0 + 完成轮数全满 + 水位恒等
# （allocated_pages() 终 == 基线——churn 帧全还）+ sw0/sw1 均严格增
# （双核参与证）。断言面 = gate 自证行 + ok 行在席 + n==320（64 轮 ×
# 5 参与者，与 main.rs STORM_ROUNDS × STORM_PARTICIPANTS / 模板 movz
# #64 三面钉同值）。

field_wait
rotate_log   # F-A12 跨 boot 混日志防线 + per-boot 轮转：相 6 换档
echo "smoke-smp: QEMU 启动（-smp 2，相 6 ALLOC churn 相：fw_cfg alloc gate + bare）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/alloc,string=1 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

tc=$(date +%s)   # F-A7 帽序②同款：锚点在 QEMU 起后起算
while ! grep -q 'ALLOC ok n=' "$LOG"; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 ALLOC 轮询阶段退出"
  [ $(( $(date +%s) - tc )) -gt "$DEADLINE_STORM" ] \
    && fail "ALLOC ok ${DEADLINE_STORM}s 未出（查 ALLOC gate on 是否在席——absent 即门未过；水位恒等/双核 sw 断言炸则有 panic 行）"
  sleep 2
done
grep -q 'ALLOC gate on' "$LOG" \
  || fail "ALLOC gate on 自证行缺失（门未过——查 fw_cfg 注入与 smp_active 面）"
alloc_line=$(grep 'ALLOC ok n=' "$LOG" | tail -1)
echo "smoke-smp: ${alloc_line}"
alloc_n=$(echo "$alloc_line" | grep -o 'n=[0-9]*' | cut -d= -f2)
[ -n "$alloc_n" ] && [ "$alloc_n" -eq 320 ] \
  || fail "ALLOC n=${alloc_n:-缺} != 320（churn 轮数不齐 = err 早断或超时——guest assert 行带 errs/dones/水位三元定位）"

# S-3c c-2c KL 争用观测（非门禁）：每相末条金丝雀 kl=（C20 收缩前后对照基线）。
# 门控相（4-8）常缺 = 设计内：canary 仅在相内全局切换过 SOAK_PRINT_EVERY=100
# 倍数时打行——门控压测相总切换 <100 无行可取（S-5f 相 8 留档串口全程
# 0 行 canary 实证）；观测面非门禁，缺 ≠ 断链。
kl_last=$(grep -o ' kl=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)
echo "smoke-smp: 相 6 KL 争用观测（非门禁）：kl=${kl_last:-缺(切换<100)}"

kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
QEMU_PID=""

# ============ 相 7：HT 风暴压测（S-5b；gated bare 第七 boot） ============
# fw_cfg `opt/berry/ht` 门控（相 6 同款）：spawn task_ht——同 fleet 形态
# 换 event 风暴载荷（每轮 event_create→object_signal→handle_close，模板
# __user_htst）。被打的共享结构 = 子进程 handle 表（单进程五线程跨两核
# 共用一表，SVC_CLASS_SQ_RQ_TABLE 锁竞争面）。静默点双走查（S-5a
# integrity_check 契约）：风暴前 + join 后均 Ok(0) + used_count==0
# （残留态/UAF = 竞态真病灶面）。断言面同相 6（n==320 三面钉）。

field_wait
rotate_log   # F-A12 同款：相 7 换档
echo "smoke-smp: QEMU 启动（-smp 2，相 7 HT 风暴相：fw_cfg ht gate + bare）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/ht,string=1 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

tc=$(date +%s)
while ! grep -q 'HT ok n=' "$LOG"; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 HT 轮询阶段退出"
  [ $(( $(date +%s) - tc )) -gt "$DEADLINE_STORM" ] \
    && fail "HT ok ${DEADLINE_STORM}s 未出（查 HT gate on 是否在席；风暴前后走查炸则有 panic 行——残留态/UAF 面）"
  sleep 2
done
grep -q 'HT gate on' "$LOG" \
  || fail "HT gate on 自证行缺失（门未过——查 fw_cfg 注入与 smp_active 面）"
ht_line=$(grep 'HT ok n=' "$LOG" | tail -1)
echo "smoke-smp: ${ht_line}"
ht_n=$(echo "$ht_line" | grep -o 'n=[0-9]*' | cut -d= -f2)
[ -n "$ht_n" ] && [ "$ht_n" -eq 320 ] \
  || fail "HT n=${ht_n:-缺} != 320（风暴轮数不齐——guest assert 行带 errs/dones 定位）"

# S-3c c-2c KL 争用观测（非门禁）：每相末条金丝雀 kl=（C20 收缩前后对照基线）。
# 门控相（4-8）常缺 = 设计内：canary 仅在相内全局切换过 SOAK_PRINT_EVERY=100
# 倍数时打行——门控压测相总切换 <100 无行可取（S-5f 相 8 留档串口全程
# 0 行 canary 实证）；观测面非门禁，缺 ≠ 断链。
kl_last=$(grep -o ' kl=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)
echo "smoke-smp: 相 7 KL 争用观测（非门禁）：kl=${kl_last:-缺(切换<100)}"

kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
QEMU_PID=""

# ============ 相 8：PP ping-pong 压测（S-5c；gated bare 第八 boot） ============
# fw_cfg `opt/berry/pp` 门控（相 6/7 同款）：spawn task_pp——单进程双线程
# 对打（root A 家核 + B 写者 spread→CPU1，模板 __user_pp arg 分流）。A 建
# channel 两端点、端点 1 经 data 窗口中继 B 窗，N=320 轮严格交替
# send/recv——双向序号逐轮验（零丢失/零乱序/零重复对账）。无 go-flag/
# 二次放行：断言面 = 窗口槽值（master 持 data VMO handle——drain 洗不掉）
# 。断言面 = a==b==320（三面钉：模板 movz #0x140 / PP_ROUNDS / 本行）。

field_wait
rotate_log   # F-A12 同款：相 8 换档
echo "smoke-smp: QEMU 启动（-smp 2，相 8 PP ping-pong 相：fw_cfg pp gate + bare）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/pp,string=1 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

tc=$(date +%s)
while ! grep -q 'PP ok a=' "$LOG"; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 PP 轮询阶段退出"
  [ $(( $(date +%s) - tc )) -gt "$DEADLINE_STORM" ] \
    && fail "PP ok ${DEADLINE_STORM}s 未出（查 PP gate on 是否在席；序号对账炸则有 errs 早断/panic 行）"
  sleep 2
done
grep -q 'PP gate on' "$LOG" \
  || fail "PP gate on 自证行缺失（门未过——查 fw_cfg 注入与 smp_active 面）"
pp_line=$(grep 'PP ok a=' "$LOG" | tail -1)
echo "smoke-smp: ${pp_line}"
pp_a=$(echo "$pp_line" | grep -o 'a=[0-9]*' | cut -d= -f2)
pp_b=$(echo "$pp_line" | grep -o 'b=[0-9]*' | cut -d= -f2)
{ [ -n "$pp_a" ] && [ "$pp_a" -eq 320 ] && [ -n "$pp_b" ] && [ "$pp_b" -eq 320 ]; } \
  || fail "PP a=${pp_a:-缺} b=${pp_b:-缺} != 320/320（对打轮数不齐——guest assert 行带 errs/单面缺轮定位）"

# S-3c c-2c KL 争用观测（非门禁）：每相末条金丝雀 kl=（C20 收缩前后对照基线）。
# 门控相（4-8）常缺 = 设计内：canary 仅在相内全局切换过 SOAK_PRINT_EVERY=100
# 倍数时打行——门控压测相总切换 <100 无行可取（S-5f 相 8 留档串口全程
# 0 行 canary 实证）；观测面非门禁，缺 ≠ 断链。
kl_last=$(grep -o ' kl=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)
echo "smoke-smp: 相 8 KL 争用观测（非门禁）：kl=${kl_last:-缺(切换<100)}"

kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
QEMU_PID=""

echo "smoke-smp: PASS（八相：相 1/2 boot 执法面 = SMP ok ncores=2 各恰一次 + SGI ok 各恰一次（S-3a IPI 收讫链）+ SGI2 ok 各恰一次（S-4 TLB shootdown 回执链）+ banner 形态 + ident 非 nogit（G-ver-2 §3/§6，相 1——smoke.sh 同正则）+ 裸相 ${#MARKERS[@]} + rootfs 相 ${#ROOTFS_MARKERS[@]} 标记 + PAR d1 活证 + 金丝雀严格递增；相 3 专项门 = LB c0=0/c1≥${LB_TICK_TH}/n=4 三硬门（S-3b 副核得货活证）；相 4 专项门 = ASIDR gate on + n≥1（S-4 4-3 回卷 quiesce 回执链）；相 5 专项门 = TLBS gate on + n=10（S-4 4-4 canary 压测双轮 decommit/Drop——stale TLB 被 SGI #2 广播打掉的十命中活证）；相 6 专项门 = ALLOC gate on + n=320（S-5b 双核 EL0 分配器 churn——水位恒等 + 双核 sw 参与证）；相 7 专项门 = HT gate on + n=320（S-5b 双核 handle 表风暴——静默点双走查 Ok(0)）；相 8 专项门 = PP gate on + a=320/b=320（S-5c 双核 channel ping-pong——单进程双线程对打 + 序号零丢失对账）——恰一次/金丝雀执法不跨相 3-8 boot）"
