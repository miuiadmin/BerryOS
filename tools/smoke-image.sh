#!/usr/bin/env bash
# smoke-image：M-G1 V-4 img-boot 独立冒烟关卡（VM 草案 §V-4；smoke-x86 同款
# 形态——独立关卡，**不并入主 smoke**，ARM64 五跑零扰动；M-G1 收官时另裁
# 是否升第六跑）。零 -kernel / 零 fw_cfg：AAVMF 固件 + out/berryos.img 挂盘
# （qemu-xhci + usb-storage）启动——「盘自己把自己拉起来」的全链自动验收。
#
# 断言面（ EFI 十段/接续形态源自 V-3 收官 run；生态 20 = 对抗审查保留日志
# 标记普查实证面（B-1 起 +B1 ok n=4；C-2c 挂账刀起 +U4 ok——深坑六件执行
# 面八步；C-4 起 +TAB ok——编辑器九步 + 真源两钉，零 fs 盘面 EFI 路可达）
# ——非主观子集）：
#   1. EFI 十段恰一次：skeleton / fw / gop（成功形态 WxH，locate fail 不算）/
#      fs HELLO.TXT / kfile / init / elf / load / mmap / BERRYBOOT ok；
#   2. 内核接续恰一次：HANDOFF ok ver=0 / MMAP ok segs= / ROOTFS ok
#      **src=handoff**（handoff 源专属形态——fw_cfg/DTB 路不通于此）；
#   3. 裸相 26 标记（与 smoke.sh 同一清单——内核行为零漂移的证据；B-0 起
#      + BENCH ok n=6 微基准/churn 金丝雀关卡行；R-0 起 + BENCHWAKE n=
#      唤醒延迟直方图形状行；S-2 起 + BENCHXH ok 资源耗尽/恢复六面行）+ EFI 路
#      生态 20 标记（rootfs hello / ECO / STD 双 / FILE / HEXYL / BSH /
#      EXIT / APP / PIPE / FS / U1 / U2 / U3 / B1 / U4 / TAB / INT / PS / BC——与
#      smoke.sh rootfs 相断言 canon 同面；无 virtio 面：不挂 virtio-blk/net，
#      VIRTIO/FAT/NET/DIR/CD 族不在断言面）；
#   4. kfile/init 字节数与宿主 KERNEL.ELF / rootfs.cpio **精确对拍**（盘
#      内容 ↔ BerryBoot 读数 ↔ 内核消费三方一致性，镜像打包断裂即 FAIL）；
#   5. switches 金丝雀 ≥3 条严格递增（同 smoke.sh——冻而不死类 bug 的
#      唯一探测器）；注入后复验（RX 中断破坏 IRQ 契约的死状是冻而非崩）；
#   6. RX 注入相：FIFO stdin 逐字节注入 `rx\n` → RX ok（PL011 RX 中断 →
#      console 任务 → token 面全链；bsh 可交互的自动代理——提示符本身无
#      固定标记；C-4b 勘正：RX ok 实为内核 console 观察窗直打、不经 bsh
#      ——EFI 路 bsh park 在 fat 段（C-2b 契约），注入命令不执行，此相
#      只验内核 RX 链路非 bsh 交互）。
#      C-4b 裁定：EFI 路不设 Ctrl-C 交互杀段（i130t）——bsh 交互编辑循环
#      结构性不可达（fat park 挡断，详见阶段 5 后注记）；交互杀五环面由
#      主路 smoke.sh RX 相段 g 独占（有盘路 bsh 全功能），EFI 路杀链核心
#      由 boot 期 INT ok 覆盖（真机 spawn+terminate+reap 四步）。
#
# UTM 手动点验四条（同镜像、UI 无法 CI——清单正文见 docs/开发体验.md §L3）：
#   ① 插盘：UTM 新建 ARM64 虚机（UEFI）驱动器挂 berryos.img；
#   ② 开机：固件自举 → BerryBoot 十段 → 内核标记流；
#   ③ 到提示符：串口/控制台终端出 bsh 提示符；
#   ④ 键盘输入：键入 rx（回显）→ ls → 截图留档。
#
# 用法：tools/smoke-image.sh <镜像> <AAVMF_CODE.fd> <AAVMF_VARS.fd>
#（三参均由 justfile 单一事实源注入——smoke-x86 先例：脚本不自带第二份
# 路径拷贝。）
# 调试：SMOKE_KEEP_LOG=1 just smoke-image 保留串口日志不删。
set -u

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR/.."
source "$SCRIPT_DIR/paths.sh"
KERNEL=$(berryos_kernel debug)

IMAGE=${1:?用法: smoke-image.sh <镜像> <AAVMF_CODE> <AAVMF_VARS>}
AAVMF_CODE=${2:?缺 AAVMF_CODE 路径}
AAVMF_VARS=${3:?缺 AAVMF_VARS 路径}

QEMU=${QEMU:-qemu-system-aarch64}
command -v "$QEMU" >/dev/null 2>&1 || { echo "smoke-image: FAIL: 找不到 $QEMU" >&2; exit 1; }
[ -f "$IMAGE" ] || { echo "smoke-image: FAIL: 镜像不存在（先 just image）：$IMAGE" >&2; exit 1; }
[ -f "$AAVMF_CODE" ] || { echo "smoke-image: FAIL: AAVMF CODE 固件不存在：$AAVMF_CODE" >&2; exit 1; }
[ -f "$AAVMF_VARS" ] || { echo "smoke-image: FAIL: AAVMF VARS 固件不存在：$AAVMF_VARS" >&2; exit 1; }
[ -f "$KERNEL" ] || { echo "smoke-image: FAIL: 内核未构建（先 just build）：$KERNEL" >&2; exit 1; }
[ -f "rootfs.cpio" ] || { echo "smoke-image: FAIL: rootfs.cpio 缺席（先 just build-user）" >&2; exit 1; }

# 帽：EFI 链 + 内核全标记 V-3 实测 ~115s（TCG），360s ≈ 3x 余量（帽只兜
# 结构性挂死，不赌快）。金丝雀在 ktest 毕即开始出现，标记到齐后通常即刻满足。
# B-1 起 +B1 ok n=4（bench1 spawn + ~8.6k SVC ≈ +5-10s TCG）——360s 帽不动。
# C-3 起 +PS/BC（ps + berryctl 三拉起 = 两 musl spawn + 4 RPC ≈ +10-20s
# TCG）——360s 帽不动。
DEADLINE_BOOT=360
DEADLINE_CANARY=120
DEADLINE_RX=60

# EFI 十段恰一次面（4 固定串 + 6 ERE——ERE 钉成功形态：gop 必须带分辨率、
# kfile/init 必须带字节与 ok，失败诊断行（locate fail / open fail / absent）
# 一律不匹配 = loud）。
EFI_FIXED=("BerryBoot EFI skeleton" "fw: EDK II" "fs: HELLO.TXT 28B match" "BERRYBOOT ok")
EFI_ERE=(
  'gop: [0-9]+x[0-9]+'
  'kfile: [0-9]+B ok'
  'init: [0-9]+B ok'
  'elf: segs=[0-9]+ entry=0x[0-9a-f]+ ok'
  'load: segs=[0-9]+ pages=[0-9]+ ok'
  'mmap: n=[0-9]+ ok'
)
# 内核接续恰一次面（src=handoff 是 handoff 路专属——fw_cfg 路打 src=fw_cfg
# 串，混源即 FAIL）。
CONT_MARKERS=("HANDOFF ok ver=0" "MMAP ok segs=" "ROOTFS ok src=handoff")
# 裸相 26（与 smoke.sh MARKERS 逐字同源——改那边同步这边，改漂移即两关卡
# 各自失去「同内核零漂移」对照力；S-2 起 +BENCHXH ok 资源耗尽行为行）。
KERNEL_MARKERS=("BerryOS boot" "KVA ok" "GIC ok" "FRAME ok" "HEAP ok" "SYSCALL ok" "TASK ok" "ASPACE ok" "VMO ok" "HANDLE ok" "POLICY ok" "ktest done" "BENCHXH ok" "BENCH ok n=6" "BENCHWAKE n=" "G1.5 ok" "BLOCK ok" "WAIT ok" "CHAIN ok" "MANY ok" "PI ok" "INTENT ok" "FAST ok" "SVC ok" "AIDEMO ok" "DEMO ok")
# EFI 路生态 20（对抗审查保留日志逐标记普查实证——`grep -cF` 各恰一次；与
# smoke.sh rootfs 相断言 canon 同面，EFI 路上 libc/spawn/退出链回归不静默
# 溜过。无 virtio 面——VIRTIO/FAT/NET/DIR/CD 族不在断言面。FS 用带参形态
# `FS ok n=` 避 `ROOTFS ok src=` 子串混叠——smoke.sh P6 A-1 同款陷阱；
# `STD ok` 与 `STD err ok` 互非子串已核，smoke.sh 双断言先例。C-2c 起
# +U3 ok（text 批执行验收——与 U1/U2 同为纯 RAM/spawn 面，EFI 路可达）；
# B-1 起 +B1 ok n=4（EL0 四项微基准格式断言收口——零 fs 盘面纯 RAM/spawn，
# 链序在 bsh 内排 fat 族前 = EFI 路无盘亦打，smoke.sh rootfs 相同步面）；
# C-3 起 +PS ok n=/BC ok（ps 查询 RPC + berryctl 三子命令——svcmgr 直打、
# 零 fs 盘面（rootfs 内 ELF spawn + 纯 RAM RPC），EFI 路可达同面；账本
# 行数 EFI 路无 virtio 族与主 smoke 不同形，断言取 `PS ok n=` 前缀式）。
# C-4 起 +TAB ok（bsh tab_selftest 九步——FakeSrc 状态机 + 真源两钉，零
# fs 盘面纯 RAM/cpio 直读，EFI 路可达；断言走 assert_marker 特例防
# TAGTAB ok 子串假阳性）。
# C-4b 起 +INT ok（bsh int_selftest 四步——editor 0x03 作废行/ESC 两态
# 共存/续用 + 前台杀链真机面：bin/hello 纯 RAM spawn + process_terminate
# (130) + collect_fg TERMINATED 收口 + reap 130；零 fs 盘面，链序排 fat
# 族前 = EFI 路可达；裸 grep -qF 足够——"INTENT ok" 不含 "INT ok" 子串）。
ECO_MARKERS=("rootfs hello" "ECO ok" "STD ok" "STD err ok" "FILE ok" "HEXYL ok" "BSH ok" "EXIT ok" "APP ok n=" "PIPE ok n=" "FS ok n=" "U1 ok" "U2 ok" "U3 ok" "B1 ok n=4" "U4 ok" "TAB ok" "INT ok" "PS ok n=" "BC ok")

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-image.XXXXXX")
RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/berryos-smoke-image.XXXXXX")
QEMU_PID=""
RX_FIFO=""
RX_HOLDER=""

cleanup() {
  if [ -n "$QEMU_PID" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null
    wait "$QEMU_PID" 2>/dev/null
  fi
  if [ -n "$RX_HOLDER" ] && kill -0 "$RX_HOLDER" 2>/dev/null; then
    kill "$RX_HOLDER" 2>/dev/null
    wait "$RX_HOLDER" 2>/dev/null
  fi
  [ -n "$RX_FIFO" ] && rm -f "$RX_FIFO"
  rm -rf "$RUN_DIR"
  if [ -n "${SMOKE_KEEP_LOG:-}" ]; then
    echo "smoke-image: 串口日志保留：$LOG" >&2
  else
    rm -f "$LOG"
  fi
}
trap cleanup EXIT

fail() {
  echo "smoke-image: FAIL: $1" >&2
  echo "---- 串口日志尾部（$LOG）----" >&2
  tail -40 "$LOG" >&2
  echo "--------------------------------" >&2
  exit 1
}

# 标记断言（smoke.sh 同款）：DEMO ok 是 AIDEMO ok 的子串，特判负向；其余
# 两两互非子串裸 grep 足够。
assert_marker() {
  if [ "$1" = "DEMO ok" ]; then
    grep -qE '(^|[^I])DEMO ok' "$LOG"
  elif [ "$1" = "TAB ok" ]; then
    # C-4：boot 期 TAGTAB ok 恰含 "TAB ok" 子串——qF 假阳性。只排前邻 G
    # （TAGTAB 的 G）——真标记可嵌稳态矩阵巨行中段（前驱任意大写字母，
    # 实测 BTAB ok），排整个 A-Z 会拒真标记（DEMO ok 只排 I 同型）。
    grep -qE '(^|[^G])TAB ok' "$LOG"
  else
    grep -qF "$1" "$LOG"
  fi
}

# 恰一次断言：EFI 引导链无重入（BerryBoot 不跑两遍 = EBS 后固件不再拿回
# 控制权的结构性证据）；grep -c 空结果时 0（|| true 吞 grep 无匹配退出码）。
assert_once() {
  local c
  c=$(grep -cF -- "$1" "$LOG" || true)
  [ "$c" -eq 1 ] || fail "「$1」非恰一次（${c:-0} 次）——EFI 链重入或打印面漂移"
}
assert_once_re() {
  local c
  c=$(grep -cE -- "$1" "$LOG" || true)
  [ "$c" -eq 1 ] || fail "ERE「$1」非恰一次（${c:-0} 次）——EFI 链重入或打印面漂移"
}

# 金丝雀序列检查（smoke.sh 同款 awk：≥3 条且严格递增）。
canary_ok() {
  grep -o 'switches=[0-9]*' "$LOG" | cut -d= -f2 | awk '
    { if (NR>1 && $1<=prev) bad=1; prev=$1; n=NR }
    END { exit (bad || n<3) ? 1 : 0 }
  '
}

# vars.fd fresh 拷贝进本跑临时目录（固件写 VARS 产生 boot entry 类状态——
# fresh 拷贝 = 跨 run 零状态泄漏，run-efi 同款纪律）。
cp "$AAVMF_VARS" "$RUN_DIR/vars.fd" || fail "vars.fd 拷贝失败"

# FIFO stdin（smoke.sh RX 相同款）：holder 持写端防 EOF；帽 1200s 盖全流程
# worst（360+120+60 + RX 注入段 + 余量 ~2x）。
RX_FIFO=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-image-fifo.XXXXXX")
rm -f "$RX_FIFO"
mkfifo "$RX_FIFO" || fail "mkfifo 失败"
( sleep 1200 ) > "$RX_FIFO" &
RX_HOLDER=$!

echo "smoke-image: QEMU 启动（AAVMF + $IMAGE——零 -kernel / 零 fw_cfg）"
# 与 justfile run-efi 同参（不挂 virtio——EFI 路无 virtio 面；ramfb 供 GOP）。
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic \
  -pflash "$AAVMF_CODE" -pflash "$RUN_DIR/vars.fd" \
  -drive if=none,file="$IMAGE",format=raw,id=bb \
  -device qemu-xhci -device usb-storage,drive=bb \
  -device ramfb \
  < "$RX_FIFO" > "$LOG" 2>&1 &
QEMU_PID=$!

# —— 阶段 1：全部标记到齐（EFI 十段 + 接续 3 + 裸相 26 + 生态 20 = 60）——
# EFI 段/接续用 ERE 固定串混合面，统一进一个等待循环（到齐是状态谓词，
# 阶段间无独立验收语义；缺失清单在超时 fail 文案里分组可见）。
ALL_COUNT=$(( ${#EFI_FIXED[@]} + ${#EFI_ERE[@]} + ${#CONT_MARKERS[@]} + ${#KERNEL_MARKERS[@]} + ${#ECO_MARKERS[@]} ))
t0=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 提前退出（固件/引导崩溃？）；日志尾部：$(tail -5 "$LOG" | tr '\n' ' ')"
  missing=""
  for m in "${EFI_FIXED[@]}" "${CONT_MARKERS[@]}" "${KERNEL_MARKERS[@]}" "${ECO_MARKERS[@]}"; do
    assert_marker "$m" || missing="${missing:+$missing }$m"
  done
  for e in "${EFI_ERE[@]}"; do
    grep -qE "$e" "$LOG" || missing="${missing:+$missing }ERE:$e"
  done
  [ -z "$missing" ] && break
  [ $(( $(date +%s) - t0 )) -gt "$DEADLINE_BOOT" ] \
    && fail "标记 ${DEADLINE_BOOT}s 未到齐（实测全链 ~115s），缺：$missing"
  sleep 1
done
echo "smoke-image: 全标记 ${ALL_COUNT}/${ALL_COUNT} 到齐（EFI 十段 + 接续 + 裸相 26 + 生态 20）"

# —— 阶段 2：恰一次（EFI 十段 + 接续——引导链重入探测）——
for m in "${EFI_FIXED[@]}" "${CONT_MARKERS[@]}"; do
  assert_once "$m"
done
for e in "${EFI_ERE[@]}"; do
  assert_once_re "$e"
done
echo "smoke-image: EFI 十段 + 接续恰一次到齐"

# —— 阶段 3：字节数对拍（盘内容 ↔ 引导链读数 ↔ 内核消费）——
KERNEL_BYTES=$(wc -c < "$KERNEL" | tr -d ' ')
INITRD_BYTES=$(wc -c < "rootfs.cpio" | tr -d ' ')
grep -qE "kfile: ${KERNEL_BYTES}B ok" "$LOG" \
  || fail "kfile 字节数与宿主内核不一致（期望 ${KERNEL_BYTES}B）——镜像打包或引导读链断裂"
grep -qE "init: ${INITRD_BYTES}B ok" "$LOG" \
  || fail "init 字节数与宿主 rootfs.cpio 不一致（期望 ${INITRD_BYTES}B）——镜像打包或引导读链断裂"
echo "smoke-image: 字节对拍 ok（kfile=${KERNEL_BYTES}B init=${INITRD_BYTES}B 与宿主精确同数）"

# —— 阶段 4：switches 金丝雀（≥3 条且严格递增）——
t1=$(date +%s)
while :; do
  n=$(grep -c 'switches=' "$LOG" || true)
  [ "${n:-0}" -ge 3 ] && break
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在金丝雀阶段退出"
  [ $(( $(date +%s) - t1 )) -gt "$DEADLINE_CANARY" ] \
    && fail "金丝雀 ${DEADLINE_CANARY}s 内不足 3 条（现有 ${n:-0}）"
  sleep 2
done
canary_ok || fail "switches 序列非严格递增：$(grep -o 'switches=[0-9]*' "$LOG" | tr '\n' ' ')"
# 快照注入前末值——阶段 6 复验以「出现严格更大的新线」为谓词（对抗审查 F2：
# 全量重验会被注入前旧线满足，冻而不死零判别力）。
LAST_CANARY=$(grep -o 'switches=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)

# —— 阶段 5：RX 注入相（bsh 可交互的自动代理）——
# 全标记到齐 = bsh 就位（BSH ok 即交互读循环在跑）；注入 `rx` 逐字节 0.5s
# （FEN=0 深 1 字节，打字速率确定性——smoke.sh RX 相同款）。
exec 3> "$RX_FIFO"
printf 'r' >&3; sleep 0.5
printf 'x' >&3; sleep 0.5
printf '\n' >&3
t2=$(date +%s)
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在 RX ok 阶段退出（注入链断？）"
  grep -qF "RX ok" "$LOG" && break
  [ $(( $(date +%s) - t2 )) -gt "$DEADLINE_RX" ] \
    && fail "RX ok ${DEADLINE_RX}s 未出现（EFI 路 RX 中断链断——查 irq? 行）"
  sleep 1
done
echo "smoke-image: RX ok 到齐（注入 rx → 应答）"
# （C-4b 裁定：EFI 路不设 Ctrl-C 交互杀注入段（i130t）——bsh 按 C-2b 链序
#  契约 park 在 fat_selftest（无盘 get_file Err → park `-> !` 挡断其后），
#  交互编辑循环结构性不可达（RX ok 是内核 console 观察窗直打、不经 bsh，
#  故能到而注入命令永不执行——verify 首跑实抓，串口 13670B 纯稳态矩阵零
#  回显实证）。交互杀五环面由主路 smoke.sh RX 相段 g 独占（有盘路 bsh
#  全功能）；EFI 路杀链核心面由 boot 期 INT ok 覆盖（真机 spawn +
#  terminate(130) + collect_fg 收口 + reap 130 四步）。）

# —— 阶段 6：注入后活性复验（等严格大于 LAST_CANARY 的新线）——
# switches 单调累计，全量重验无判别力（对抗审查 F2）——RX 中断破坏 IRQ 契约
# 的死状 = RX ok 后冻结，只有「新线仍在出现」才是活性谓词。
t3=$(date +%s)
while :; do
  cur=$(grep -o 'switches=[0-9]*' "$LOG" | tail -1 | cut -d= -f2)
  [ "${cur:-0}" -gt "$LAST_CANARY" ] && break
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 在注入后复验阶段退出"
  [ $(( $(date +%s) - t3 )) -gt "$DEADLINE_CANARY" ] \
    && fail "注入后 ${DEADLINE_CANARY}s 无新金丝雀线（> $LAST_CANARY）——RX 注入后冻结（冻而不死面）"
  sleep 2
done

echo "smoke-image: PASS（EFI 十段恰一次 + 接续 src=handoff + 裸相 26 + 生态 20 + 字节对拍 + 金丝雀严格递增 + RX 注入后活性复验）"
