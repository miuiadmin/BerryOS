#!/usr/bin/env bash
# G3-3 lockdep 面独立冒烟（f4 复验 F10 闭合件——2026-11-05）。
#
# 缺口：selftest marker `LOCKDEP selftest ok` 此前无自动化门禁载体——
# smoke.sh/smoke-smp.sh/markers.sh 全树无 LOCKDEP 执法面；selftest FAIL
# 非停机（与 VIOLATION 的 fail-loud 停机不同），无 KEEP_LOG 的跑里 FAIL
# 行随串口档清失 = 零执法。本脚本 = feature 面最小执法载体：
#
#   快启 -smp 2 → 等 marker（帽 60s）→ 三断言：
#     ① `LOCKDEP selftest ok` 恰一次
#     ② `LOCKDEP selftest FAIL` 零次
#     ③ `LOCKDEP VIOLATION` 零次
#     ④ `DPG ok` ≥1 且行号晚于 selftest ok（C-3，G4-1 三镜：selftest 在
#       boot 窗先于 dpg::init——断言窗延到 dpg 闭环毕，执法「feature boot
#       含 dpg 锁序」不再靠借位）
#
# G4-3 B 刀起 + 相 2 rootfs 生态面：执法窗从 boot 裸链延到满拓扑生态
#（fw_cfg 多卷 rootfs + virtio-blk/net hostfwd 三口 + tablet + -smp 2，
# 拓扑镜像 smoke-smp 相 2；宿主对端三件套 echod/evsclient/sktclient 必在
# ——§A122：缺 18083 → svcmgr 卡 SKT 无帽 accept → 终态标记永缺）。终态 =
# `DPGN ok n=4`（rootfs 相末标记——它到 = 上游全链已过）；断言 = ①②③
# 复验 + dpg 四行行序（selftest < DPGP ok < DPGS ok n=4 < DPGN ok n=4
# 严格递增——键化/风暴/负序三链在仪表下依序走毕）+ `DPG ovfl` 零计
#（N=4<K=8 池永不满——B 刀 ovfl 溢出观测面只在 C-full 满载拓扑见行）。
# 执法窗边界 = boot → eco 链末标记（DPGN）即止；稳态 echo 环与
# PAR/park/偷取窗不属本面（真覆盖 = 远期可选项，CONC-1 跨核交错族
# declare 同册）。
#
# 用法：bash tools/smoke-lockdep.sh <内核 ELF>（须 feature 面——脚本先验
# marker 串在镜像内，default 面二进制直拒 loud，不烧 60s 帽；相 2 另验
# `DPGN ok n=4` 串在镜像——陈旧 userbins 烘的内核快拒，不烧 840s 帽）。
# 镜像出法：
#   just build-user && cargo build --features lockdep && \
#     bash tools/smoke-lockdep.sh "$(bash tools/paths.sh kernel)"
#（无 just 配方：justfile 为兄弟会话 WIP 占用时 script 直调同效——
# smoke-smp 先例，内核路径参数化单一事实源。cwd 无关——工具/盘/卷均
# 经 SCRIPT_DIR/ROOT 绝对解析。）
#
# 不并入主 smoke 族（ARM64 四跑零扰动纪律——主族 default 面单核；feature
# 面独立通道）。FAIL 恒留档；SMOKE_LOCKDEP_KEEP_LOG=1 成功亦留。
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

KERNEL="${1:-}"
if [ -z "$KERNEL" ] || [ ! -f "$KERNEL" ]; then
  echo "smoke-lockdep: 用法：$0 <内核 ELF>（feature 面——cargo build --features lockdep 出）" >&2
  exit 2
fi

# 镜像面自证：default 面二进制 marker 串编译消除——前置直拒（陈旧产物/
# 面错配零歧义，G-eco-b「陈旧产物静默上线」同族防线）。
if ! strings "$KERNEL" | grep -q 'LOCKDEP selftest ok'; then
  echo "smoke-lockdep: 镜像非 lockdep feature 面（marker 串零命中）——先 cargo build --features lockdep" >&2
  exit 2
fi
# 相 2 前置自证（B 刀）：镜像须含 DPGN 串——build-user 未跑/旧 userbins 烘
# 的内核直拒，不烧 840s 帽才发现终态永缺。
if ! strings "$KERNEL" | grep -q 'DPGN ok n=4'; then
  echo "smoke-lockdep: 镜像缺 DPGN ok n=4 串（陈旧 userbins）——先 just build-user 再 cargo build --features lockdep" >&2
  exit 2
fi

PATH="$HOME/.cargo/bin:$PATH"
QEMU="qemu-system-aarch64"
MACH="-machine virt -cpu cortex-a72 -m 128M -nographic"

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-lockdep.XXXXXX")
LOG2=""
keep_log() {
  if [ -n "${SMOKE_LOCKDEP_KEEP_LOG:-}" ] || [ "$1" = "keep" ]; then
    echo "smoke-lockdep: 串口日志保留：$LOG ${LOG2:+$LOG2}" >&2
    # 相 2 对端三件套日志随留档点名列（smoke-smp 同款）。
    for p in "${EVS_LOG:-}" "${SKT_LOG:-}" "${ECHO_LOG:-}"; do
      [ -n "$p" ] && echo "smoke-lockdep: 客户端日志保留：$p" >&2
    done
  else
    rm -f "$LOG"
    [ -n "$LOG2" ] && rm -f "$LOG2"
    rm -f "${EVS_LOG:-}" "${SKT_LOG:-}" "${ECHO_LOG:-}"
  fi
}

# 相 2 实体收尸（QEMU + 三 client——fail-fast 各出口统一兜底；相 1 QPID
# 内联收割不入列）。
P2_PIDS=""
p2_cleanup() {
  local p
  for p in $P2_PIDS; do
    kill "$p" 2>/dev/null
    wait "$p" 2>/dev/null
  done
}
trap p2_cleanup EXIT

# ============ 相 1：裸相（-smp 2 快启，原 G3-3 面） ============

"$QEMU" $MACH -smp 2 -kernel "$KERNEL" >"$LOG" 2>&1 &
QPID=$!

# 有界等待：marker 三态任一即止（ok / FAIL / VIOLATION），帽 60s（本机实测
# boot→marker ≈2s；帽对宿主争用保守 ×30）。C-3：ok 态续等到 `DPG ok`
#（dpg::init 在 selftest 后——锁序执法面须盖到 dpg 闭环毕）；FAIL/VIOLATION/
# panicked = 终态坏，早断交断言面自红。
deadline=$((SECONDS + 60))
while [ $SECONDS -lt $deadline ]; do
  if grep -qE 'LOCKDEP selftest FAIL|LOCKDEP VIOLATION|panicked' "$LOG" 2>/dev/null; then
    break
  fi
  if grep -q 'LOCKDEP selftest ok' "$LOG" 2>/dev/null && grep -q 'DPG ok' "$LOG" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$QPID" 2>/dev/null; then
    break
  fi
  sleep 0.2
done
kill "$QPID" 2>/dev/null
wait "$QPID" 2>/dev/null

OK=$(grep -c 'LOCKDEP selftest ok' "$LOG" 2>/dev/null || true)
FAIL=$(grep -c 'LOCKDEP selftest FAIL' "$LOG" 2>/dev/null || true)
VIO=$(grep -c 'LOCKDEP VIOLATION' "$LOG" 2>/dev/null || true)

rc=0
if [ "$OK" -ne 1 ]; then
  echo "smoke-lockdep: FAIL——selftest ok 计数 ${OK}（期望恰 1；0 = 未达 boot 窗或超时）" >&2
  rc=1
fi
if [ "$FAIL" -ne 0 ]; then
  echo "smoke-lockdep: FAIL——selftest FAIL 计数 ${FAIL}（六阳性三负对照有臂翻车）" >&2
  rc=1
fi
if [ "$VIO" -ne 0 ]; then
  echo "smoke-lockdep: FAIL——VIOLATION 计数 ${VIO}（真锁序违约——检测器命中）" >&2
  rc=1
fi
# ④（C-3）：DPG ok ≥1 且行号晚于 selftest ok 行（执法窗盖到 dpg 闭环）。
DPG=$(grep -c 'DPG ok' "$LOG" 2>/dev/null || true)
SF_LINE=$(grep -n 'LOCKDEP selftest ok' "$LOG" 2>/dev/null | head -1 | cut -d: -f1)
DPG_LINE=$(grep -n 'DPG ok' "$LOG" 2>/dev/null | head -1 | cut -d: -f1)
if [ "$DPG" -lt 1 ]; then
  echo "smoke-lockdep: FAIL——DPG ok 计数 ${DPG}（C-3：断言窗已延至 dpg 闭环后，0 = dpg 链未毕/超时）" >&2
  rc=1
elif [ -z "$SF_LINE" ] || [ -z "$DPG_LINE" ] || [ "$DPG_LINE" -le "$SF_LINE" ]; then
  echo "smoke-lockdep: FAIL——DPG ok 行号($DPG_LINE) 不晚于 selftest ok 行号($SF_LINE)（C-3 行序断言）" >&2
  rc=1
fi

if [ "$rc" -ne 0 ]; then
  keep_log keep
  exit "$rc"
fi
echo "smoke-lockdep: 相 1 PASS（selftest ok 恰 1 + FAIL 0 + VIOLATION 0 + DPG ok 行序对，-smp 2 快启）"

# ============ 相 2：rootfs 生态面（G4-3 B 刀） ============

# rootfs 卷前置检查（smoke-smp 相 2 同款：缺产物 loud 快败，不烧 840s 帽）。
[ -f "$ROOT/rootfs.cpio" ] || {
  echo "smoke-lockdep: FAIL——缺 rootfs.cpio（相 2 需 build-user 产物——先 just build-user）" >&2
  keep_log keep
  exit 1
}
# fw_cfg 多卷发现环（smoke-smp/smoke.sh 同款：rootfs0..9 连续前缀拼接，
# 缺席回落旧名单单卷）。
FWCFG=""
vi=0
while [ -f "$ROOT/rootfs$vi.cpio" ]; do
  FWCFG="$FWCFG -fw_cfg name=opt/berry/rootfs$vi,file=$ROOT/rootfs$vi.cpio"
  vi=$((vi+1))
done
[ -n "$FWCFG" ] || FWCFG="-fw_cfg name=opt/berry/rootfs,file=$ROOT/rootfs.cpio"

# 盘名专属 disk.lockdep.img + LBA0 印章（smoke-smp 同款：防覆写兄弟线活盘）。
DISK="$ROOT/disk.lockdep.img"
python3 "$SCRIPT_DIR/mkfat.py" "$DISK" || {
  echo "smoke-lockdep: FAIL——mkfat 造盘失败（${DISK}）" >&2
  keep_log keep
  exit 1
}
printf 'BERRYOSVBLK!' | dd of="$DISK" conv=notrunc status=none

# 端口让场（18080-18083 兄弟线在用即退——与主 smoke/smoke-smp/soak 同值
# 端口族 = 两线天然串行协议）。
if lsof -nP -iTCP:18080-18083 -sTCP:LISTEN >/dev/null 2>&1; then
  echo "smoke-lockdep: FAIL——18080-18083 端口族被占（lsof）——兄弟 smoke/soak 线在跑？让场后重试" >&2
  keep_log keep
  exit 1
fi

# QEMU 字段让行（smoke-smp 同款签名——本仓 QEMU 签名 pgrep，不抢跑兄弟线）。
waited=0
while pgrep -f 'qemu-system.*(-machine virt|-kernel)' >/dev/null 2>&1; do
  waited=$((waited + 1))
  [ "$waited" -gt 5 ] && { echo "smoke-lockdep: FAIL——QEMU 字段被占，放弃" >&2; keep_log keep; exit 1; }
  echo "smoke-lockdep: QEMU 字段被占，等待 15s（第 ${waited} 轮）…" >&2
  sleep 15
done

# 宿主对端三件套（§A122 教训面：缺席 = svcmgr 卡 SKT 无帽 accept，终态标记
# 永缺）。echod 即起（guest 出网回声对端）；evsclient 即起；sktclient 懒
# 启动（guest `EVS ok` 后才 exec——SYN 重试流不与 EVS 帧同环排队，smoke-smp
# A3-3 同款）。
LOG2=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-lockdep.XXXXXX")
EVS_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-evsclient.XXXXXX")
SKT_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-sktclient.XXXXXX")
ECHO_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-echod.XXXXXX")
# evsclient 帽 env 覆盖（§5.1 帽序不变量「client 帽 ≥ 相帽 + 60s」：相 2
# 帽 840s > evsclient 默认 TOTAL 480s 倒挂——慢宿主下健康跑被自家对端转
# §A122 形假挂死〔EVS 缺 → DPGN 永缺 → 烧满 840s 红且死因误指〕；TOTAL
# 480→960 / ATTEMPT 45→90，smoke-smp:85-86 同款。echod/sktclient 无帽面
# 不涉〔sktclient SYN 重试流 + echod 无 TOTAL〕。verdict CONC-B2-1。）
export BERRY_EVS_TOTAL_CAP_S=960
export BERRY_EVS_ATTEMPT_CAP_S=90
python3 "$SCRIPT_DIR/evsclient.py" 18082 EVS > "$EVS_LOG" 2>&1 & EVS_PID=$!
( until grep -q 'EVS ok' "$LOG2" 2>/dev/null; do sleep 2; done
  exec python3 "$SCRIPT_DIR/evsclient.py" 18083 SKT ) > "$SKT_LOG" 2>&1 & SKT_PID=$!
python3 "$SCRIPT_DIR/echod.py" > "$ECHO_LOG" 2>&1 & ECHO_PID=$!
P2_PIDS="$EVS_PID $SKT_PID $ECHO_PID"
te=$(date +%s)
until grep -q 'listening on' "$ECHO_LOG" 2>/dev/null; do
  kill -0 "$ECHO_PID" 2>/dev/null || {
    echo "smoke-lockdep: FAIL——echod 启动失败（18080 被占？）：$(cat "$ECHO_LOG")" >&2
    keep_log keep
    exit 1
  }
  [ $(( $(date +%s) - te )) -gt 5 ] && {
    echo "smoke-lockdep: FAIL——echod 5s 未 listening" >&2
    keep_log keep
    exit 1
  }
  sleep 0.2
done

echo "smoke-lockdep: QEMU 启动（-smp 2，相 2 rootfs 生态面：fw_cfg rootfs 卷 + virtio-blk + virtio-net hostfwd 三口 + tablet）"
t2=$(date +%s)
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  $FWCFG \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file="$DISK",cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082,hostfwd=tcp::18083-:18083 -device virtio-net-device,netdev=n0 \
  -device virtio-tablet-device,id=tab0 \
  < /dev/null > "$LOG2" 2>&1 &
Q2=$!
P2_PIDS="$Q2 $P2_PIDS"

# 终态轮询：`DPGN ok n=4`（帽 840s = smoke-smp 相 2 同值——lockdep 仪表化
# 开销另盖；fail-fast 面早断真断链，帽只兜挂死）。
dl2=$((SECONDS + 840))
while :; do
  grep -qE 'LOCKDEP selftest FAIL|LOCKDEP VIOLATION|panicked' "$LOG2" 2>/dev/null && break
  grep -q 'DPGN ok n=4' "$LOG2" 2>/dev/null && break
  kill -0 "$Q2" 2>/dev/null || break
  [ $SECONDS -ge $dl2 ] && break
  sleep 1
done
kill "$Q2" 2>/dev/null
wait "$Q2" 2>/dev/null
sleep 3   # SIGTERM 后串口落定（P6 A-6 实证同款）
# 相 2 墙钟（R3/qemu-tcg 判例对照面——lockdep 插桩膨胀 vs 840s 帽的回执
# 数字；QEMU 起 → DPGN 终态收割毕）。
echo "smoke-lockdep: 相 2 墙钟 $(( $(date +%s) - t2 ))s（QEMU 起→终态收割；帽 840s）"
kill "$EVS_PID" "$SKT_PID" "$ECHO_PID" 2>/dev/null
wait "$EVS_PID" "$SKT_PID" "$ECHO_PID" 2>/dev/null
P2_PIDS=""

rc2=0
OK2=$(grep -c 'LOCKDEP selftest ok' "$LOG2" 2>/dev/null || true)
FL2=$(grep -c 'LOCKDEP selftest FAIL' "$LOG2" 2>/dev/null || true)
VI2=$(grep -c 'LOCKDEP VIOLATION' "$LOG2" 2>/dev/null || true)
if [ "$OK2" -ne 1 ]; then
  echo "smoke-lockdep: FAIL——相 2 selftest ok 计数 ${OK2}（0 = boot 未达/QEMU 早退/超时）" >&2
  rc2=1
fi
if [ "$FL2" -ne 0 ]; then
  echo "smoke-lockdep: FAIL——相 2 selftest FAIL 计数 $FL2" >&2
  rc2=1
fi
if [ "$VI2" -ne 0 ]; then
  echo "smoke-lockdep: FAIL——相 2 VIOLATION 计数 ${VI2}（生态载荷下真锁序违约）" >&2
  rc2=1
fi
# dpg 四行行序（B 刀执法面：selftest → DPGP → DPGS → DPGN 严格递增行号；
# 空 = 对应链未毕）。
l_sf=$(grep -n 'LOCKDEP selftest ok' "$LOG2" 2>/dev/null | head -1 | cut -d: -f1)
l_p=$(grep -nF 'DPGP ok' "$LOG2" 2>/dev/null | head -1 | cut -d: -f1)
l_s=$(grep -nF 'DPGS ok n=4' "$LOG2" 2>/dev/null | head -1 | cut -d: -f1)
l_n=$(grep -nF 'DPGN ok n=4' "$LOG2" 2>/dev/null | head -1 | cut -d: -f1)
if [ -z "$l_sf" ] || [ -z "$l_p" ] || [ -z "$l_s" ] || [ -z "$l_n" ] \
   || [ "$l_sf" -ge "$l_p" ] || [ "$l_p" -ge "$l_s" ] || [ "$l_s" -ge "$l_n" ]; then
  echo "smoke-lockdep: FAIL——dpg 四行行序断言（sf=$l_sf < DPGP=$l_p < DPGS=$l_s < DPGN=$l_n 须全非空严格递增）" >&2
  rc2=1
fi
# ovfl 溢出观测面零计（N=4<K=8：池永不满——ovfl 行在 = 槽释放链断/键化泄漏旁证）。
OV=$(grep -c 'DPG ovfl' "$LOG2" 2>/dev/null || true)
if [ "$OV" -ne 0 ]; then
  echo "smoke-lockdep: FAIL——DPG ovfl 计数 ${OV}（N=4<K=8 拓扑池满不该发生——槽释放链查）" >&2
  rc2=1
fi

if [ "$rc2" -eq 0 ]; then
  echo "smoke-lockdep: PASS（双相：相 1 裸相 selftest/DPG ok 行序 + 相 2 rootfs 满拓扑 DPGN 终态 + dpg 四行行序 + ovfl 零计 + FAIL/VIOLATION 0×2 boot）"
  keep_log drop
else
  echo "---- 相 2 串口日志尾部（${LOG2}）----" >&2
  tail -40 "$LOG2" >&2
  # 客户端死因随失败即打印不留文件（smoke-smp 同款纪律——§A122 第一手
  # 死因就在三件套日志里）。
  for p in "echod:${ECHO_LOG:-}" "evsclient:${EVS_LOG:-}" "sktclient:${SKT_LOG:-}"; do
    lg="${p#*:}"
    if [ -n "$lg" ] && [ -s "$lg" ]; then
      echo "---- ${p%%:*} 日志尾部（${lg}）----" >&2
      tail -10 "$lg" >&2
    fi
  done
  keep_log keep
fi
exit "$rc2"
