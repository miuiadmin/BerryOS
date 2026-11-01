#!/usr/bin/env bash
# BerryOS S-2b 副核参与冒烟——双相（`SMP-S2实施设计草案.md` §5.1；smoke-x86
# 同款独立形态——不并入主 smoke、不进 CI：主 smoke 八跑是单核回归基线，
# 零扰动）。相 1 = 裸相（27 标记 + SMP 握手）；相 2 = rootfs 生态相
# （46 标记 + 对拍 client 对 + PAR 副核参与活证）——rootfs 全链在
# `-smp 2` 下绿 = E0「内核串行/用户并行」语义等价的端到端证据。
#
# **-smp 只进本线**：全仓唯一带 `-smp 2` 的 QEMU 入口（justfile run-smp /
# smoke-smp 本脚本）；主 run/run-el2/run-rootfs/smoke 一律单核（S-1 单核零
# 漂移硬线）。默认机型 + `-kernel` ELF 直加载（conduit 分型：HVC 只在默认
# 机型被 QEMU 拦截为 PSCI——virtualization=on 下 HVC 从 EL1 陷阱到 EL2 且
# 不再拦截，VBAR_EL2=0 → PC=0 静默挂死，内核侧 BOOT_EL 门先行跳过）。
#
# 断言面（状态谓词，非竞速）：
#   1. `SMP ok ncores=2` 恰一次**按相各自成立**（两相两次 boot 各恰一次；
#      grep -cF 全串 = 1）——副核 PSCI CPU_ON → §3.2 十项 → hb1 → 主核
#      dump → release → hb2 握手全闭环的机器断言（两心跳都是内存状态断言，
#      非时序竞速——4-5a 纪律）；反向断言 `SMP: FAIL` / `SMP: skip` 不出现。
#      S-3a probe ② 起 + `SGI ok n=` 恰一次（CPU0→CPU1 SGI_RESCHEDULE 单发
#      → 副核消费臂收讫计数的端到端机器断言；`SGI ok` 归 smp_assertions
#      不进标记数组——主 smoke 单核结构性不触发，零漂移）。
#   2. 相 1 裸相 27 标记 / 相 2 rootfs 相 46 标记全部出现（与主 smoke 同面
#      ——标记数组与 smoke.sh 运行时计数对拍，见下方机器守卫）。
#   3. switches 金丝雀 ≥3 条严格递增**按相**（任核 tick 命中 100 倍数即整行
#      持 PRINT_LOCK 打印——两核交替打印不破坏数值序列单调，§5.2）。
#   4. 相 2 专属：`ROOTFS vmo=none` 拒 + `app APPDEMO up` + EVS/SKT 宿主
#      对拍 client 完成行 + **PAR 副核参与活证**（裁决翻转 2026-10-31：E0
#      无 IPI + park 派发限频下真并行 witness 归 S-3——验收面 = PAR absent
#      行 `d1=`（副核 park 派发计数）非零；PAR ok 提前命中 = 超预期收益，
#      放行不拒）。
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
field_wait() {
  local waited=0
  while pgrep -f qemu-system >/dev/null 2>&1; do
    waited=$((waited + 1))
    [ "$waited" -gt 5 ] && { echo "smoke-smp: FAIL: QEMU 字段被占（pgrep 非空），放弃" >&2; exit 1; }
    echo "smoke-smp: QEMU 字段被占，等待 15s（第 ${waited} 轮）…" >&2
    sleep 15
  done
}

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-smp.XXXXXX")
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
    echo "smoke-smp: 串口日志保留：$LOG" >&2
    for p in "$EVS_LOG" "$SKT_LOG" "$ECHO_LOG"; do
      [ -n "$p" ] && echo "smoke-smp: 客户端日志保留：$p" >&2
    done
  else
    rm -f "$LOG" "$EVS_LOG" "$SKT_LOG" "$ECHO_LOG"
  fi
}
trap cleanup EXIT

fail() {
  echo "smoke-smp: FAIL: $1" >&2
  echo "---- 串口日志尾部（$LOG）----" >&2
  tail -40 "$LOG" >&2
  # 客户端死因随失败即打印不留文件（smoke.sh 同款纪律）。
  for p in "echod:$ECHO_LOG" "evsclient:$EVS_LOG" "sktclient:$SKT_LOG"; do
    local lg="${p#*:}"
    if [ -n "$lg" ] && [ -s "$lg" ]; then
      echo "---- ${p%%:*} 日志尾部（$lg）----" >&2
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

MARKERS=("BerryOS boot" "KVA ok" "GIC ok" "FRAME ok" "HEAP ok" "SYSCALL ok" "TASK ok" "FP ok" "ASPACE ok" "VMO ok" "HANDLE ok" "POLICY ok" "ktest done" "BENCHXH ok" "BENCH ok n=6" "BENCHWAKE n=" "G1.5 ok" "BLOCK ok" "WAIT ok" "CHAIN ok" "MANY ok" "PI ok" "INTENT ok" "FAST ok" "SVC ok" "AIDEMO ok" "DEMO ok")

# rootfs 相 46 标记——**复制自 smoke.sh**（§5.1 最低限度方案：markers.sh
# 抽取留后刀；漂移面由下方运行时计数对拍机器守卫盖住——主 smoke 加标记
# 而本数组漏跟 = 立即 FAIL 防假绿，P6 A-5「基准重产义务要有机器守卫」族）。
ROOTFS_MARKERS=("ROOTFS ok src=" "ROOTFS vmo=" "rootfs hello" "ECO ok" "STD ok" "STD err ok" "FILE ok" \
  "HEXYL ok" "BSH ok" "EXIT ok" "PIPE ok" "FS ok n=" "VIRTIO ok" "VBLK ok" "BLKD ok n=" "FAT ok n=" "FATW ok n=" "NET ok ping=" "DNS ok addr=" "TCP ok echo=" "LSN ok n=7" "TCPS ok echo=" "APP ok n=" "DIR ok n=" "CD ok" "U1 ok" "U2 ok" "U3 ok" "B1 ok n=4" "DF ok" "U4 ok" "TAB ok" "INT ok" "POLL ok" "EVS ok n=2" "SKT ok n=3" "EPL ok n=8" "NETS ok n=2" "MMP ok n=9" "CLK ok n=6" "SIG ok n=6" "PS ok n=" "BC ok" "M3DEMO ok n=5" "THR ok" "INPUT ok")

# 机器守卫：两数组与 smoke.sh 原文计数对拍（元素 = 引号串——**元素内不得
# 含 `)`/引号**，加标记时守住此约束）。awk 状态机在起始行内即判 `)` 收口
# ——sed 区间 /a/,/b/ 的结束模式不在起始行测试（单行数组会越界吃到下一行
# 引号串，守卫干跑实抓的多计假阳性）。
cross_check_markers() {
  local src="$SCRIPT_DIR/smoke.sh"
  local n1 n2
  n1=$(awk '/^MARKERS=\(/{f=1} f{n+=gsub(/"/,"x"); if (/\)/) exit} END{print int(n/2)}' "$src")
  n2=$(awk '/^ROOTFS_MARKERS=\(/{f=1} f{n+=gsub(/"/,"x"); if (/\)/) exit} END{print int(n/2)}' "$src")
  [ "$n1" -eq "${#MARKERS[@]}" ] \
    || fail "裸相 MARKERS 与 smoke.sh 漂移（本脚本 ${#MARKERS[@]} vs smoke.sh ${n1:-缺}）——主 smoke 改标记须同步本数组"
  [ "$n2" -eq "${#ROOTFS_MARKERS[@]}" ] \
    || fail "ROOTFS_MARKERS 与 smoke.sh 漂移（本脚本 ${#ROOTFS_MARKERS[@]} vs smoke.sh ${n2:-缺}）——主 smoke 加标记须同步本数组（§5.1 单一事实源纪律）"
}
cross_check_markers

# SMP 按相断言：SMP ok 恰一次（重复 = 副核路径重入；缺失已由标记轮询覆盖）
# + 副核报告行在席（mpidr/ctx 回显/per-core 寄存器——超时 dump 的诊断源；
# 哨兵 's' 紧邻其前 = UART 单写者纪律的可观测面）。
smp_assertions() {
  local label="$1" n
  n=$(grep -cF "SMP ok ncores=2" "$LOG" || true)
  [ "$n" -eq 1 ] || fail "$label SMP 标记非恰一次（n=$n）：SMP ok ncores=2"
  grep -q "SMP: sec up" "$LOG" || fail "$label 缺副核报告行 SMP: sec up"
  # S-3a probe ②：SGI 收讫标记恰一次（超时面已由 `SMP: FAIL` 反向快败覆盖）。
  n=$(grep -cF "SGI ok n=" "$LOG" || true)
  [ "$n" -eq 1 ] || fail "$label SGI ok 非恰一次（n=$n）——S-3a SGI_RESCHEDULE 收讫链断（发/投/收讫计数三面查）"
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
      || fail "QEMU 提前退出（$label）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
    grep -q "SMP: FAIL" "$LOG" && fail "SMP 握手自报 FAIL（$label）：$(grep 'SMP: FAIL' "$LOG" | tail -1)"
    grep -q "SMP: skip" "$LOG" && fail "SMP 被跳过（$label——错机型/错旗？）：$(grep 'SMP: skip' "$LOG" | tail -1)"
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
    kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在金丝雀阶段退出（$label）"
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
    echo "smoke-smp: $label 金丝雀行周期 ≈ ${CAN_PERIOD_S}s/行（值 $v_first→$v_last）"
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
      || fail "evsclient 提前退出（$label）：$(tail -5 "$EVS_LOG" | tr '\n' ' ')"
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
      || fail "sktclient 提前退出（$label）：$(tail -5 "$SKT_LOG" | tr '\n' ' ')"
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

# ============ 相 1：裸相（27 标记 + SMP 握手 + 金丝雀） ============

field_wait
echo "smoke-smp: QEMU 启动（$KERNEL，-smp 2，相 1 裸相）"
t_boot0=$(date +%s)
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

poll_markers "$DEADLINE_BOOT" "裸相" 1 "${MARKERS[@]}"
SMP_BOOT_WALL=$(( $(date +%s) - t_boot0 ))
smp_assertions "相 1"
echo "smoke-smp: 相 1 SMP 握手 + boot 标记 ${#MARKERS[@]}/${#MARKERS[@]} 到齐（墙钟 ${SMP_BOOT_WALL}s）"

canary_check "相 1" "$DEADLINE_CANARY"

kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
QEMU_PID=""

# ============ 相 2：rootfs 生态相（46 标记 + 对拍 client + PAR 活证） ============

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

: > "$LOG"   # 日志分相（smoke.sh 先例：复用同一日志文件，cleanup 只删一份）
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
  echo "smoke-smp: PAR absent d1=$d1（副核参与活证 ≥1 派发；真并行 witness 归 S-3）"
fi

canary_check "相 2" "$DEADLINE_CANARY"

kill "$QEMU_PID" 2>/dev/null
wait "$QEMU_PID" 2>/dev/null
QEMU_PID=""
echod_stop

# R3 首跑定标三数（观测面非断言面；任一数 > 帽 70% 即预授权 ×1.5 一次）。
echo "smoke-smp: R3 定标三数：boot=${SMP_BOOT_WALL}s（帽 ${DEADLINE_BOOT}s）/ 金丝雀≈${CAN_PERIOD_S:-?}s/行 / rootfs=${ROOTFS_WALL}s（帽 ${DEADLINE_ROOTFS}s）"
echo "smoke-smp: PASS（两相：SMP ok ncores=2 各恰一次 + SGI ok 各恰一次（S-3a IPI 收讫链）+ 裸相 ${#MARKERS[@]} + rootfs 相 ${#ROOTFS_MARKERS[@]} 标记 + PAR d1 活证 + 金丝雀严格递增）"
