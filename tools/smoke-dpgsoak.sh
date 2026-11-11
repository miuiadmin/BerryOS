#!/usr/bin/env bash
# G4-3 C-full 缺页 soak 满配专用关卡（smoke-lockdep 同例直调 bash，不进
# CI；无 just 配方——内核路径参数化单一事实源）。
#
# 载体：门控变体卷——正装 rootfs **末 split 卷**手术在 TRAILER 前追空标记
# 文件 `dpgf.on`（newc 编码复用 tools/mkcpio.py `entry()` 单编码器纪律，无第
# 二份 newc 逻辑；TRAILER 名域 rfind 定位 = 档尾真 trailer，数据内偶合
# ASCII 不扰）→ fw_cfg 递送（X-3c 后 rootfs 越单卷帽 = split 形 rootfs0..N
# 多卷名递送，单卷形退旧名——bench.py dpg_variant/dpg_boot 同款，本脚本是
# DPGF/DPGF2 标记唯一生产面）。svcmgr `dpgf_selftest` 门 = cpio_find 命中
# dpgf.on 才跑——正装 rootfs/run/smoke/EFI 缺席 = 零扰动零新标记（rootfs
# 相 51 / 主 smoke 83 / EFI 断言 36 计数零漂）。
#
# 断言（终态 = `DPGF2 ok n=3`，帽 DPGF_CAP_S 默认 600s）：
#   ① `DPGF ok n=15` 恰 1（15× join 7 满配终态——16 槽登记恰满〔boot 1
#      + 15，demand 声明表独立池不受 EREQ 抬幅影响〕+ CONC-1 奇数子
#      spread 两核 faulter）
#   ② `DPGF2 ok n=3` 恰 1（C-4 槽回收二轮：全子 Vmar Drop 回落 1 → 再
#      spawn 再登记 Ok——「16 槽并发帽非终身帽」declare 定谳活证）
#   ③ `DPG ovfl` 极性随 DPGF_K 派生（X-5 过滤取刀 EREQ_SLOTS 8→32 起
#      默认 32）：K ≥ 15 → 恰 0（容量余量证明——15 并发 fault-阻塞臂
#      全驻留不触池满）；K < 15 → ∈ [1, DPGF_OVFL_CAP]（原 C-full 池满
#      必触达 + 有界镜像反转面）。溢出机器面（EOVERFLOW_KEY convoy
#      醒-放槽-重扫再印）已迁 ktest 确定性单测（X-5 同刀），本脸不再
#      承担其覆盖；env DPGF_OVFL_CAP 覆盖〔默认 40〕仅 K < N 路消费。
#   ④ dpg 五行行序 DPGP < DPGS < DPGN < DPGF < DPGF2 严格递增
#   ⑤ `eco: dpgf` 降级行零计（波内坏面 = fail loud 早断）；60d3bb9 起
#      `eco: dpgf t-37` 容差行（拆台撞已退子幂等面 -37，合法吸收）不计降级
#
# kernel 面无关（default / lockdep ELF 皆可跑）：LOCKDEP banner 在席时
# VIOLATION=0 机器执法（⑥，concur-1）；行序延伸 = C-3 刀另挂。宿主对端
# **echod 必在**（TCPS 相 :141 先于 dpgf 位 :188——缺席则走链卡死在终态
# 标记前）；evsclient/sktclient = 拓扑镜像惯例，**非本终态依赖**（四绿档
# 全无 EVS ok/SKT ok——懒门从未放行，concur-2 定谳）；-smp 2 必须
# （spread 失衡翻核需双核——CONC-1 真跨核面）。
#
# 用法：bash tools/smoke-dpgsoak.sh "$(bash tools/paths.sh kernel)"
# 前置：just build（内核内嵌新 svcmgr/dpgp 像）→ rootfs.cpio 在席。
# FAIL 恒留档；SMOKE_DPGSOAK_KEEP_LOG=1 成功亦留。
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

KERNEL="${1:-}"
if [ -z "$KERNEL" ] || [ ! -f "$KERNEL" ]; then
  echo "smoke-dpgsoak: 用法：$0 <内核 ELF>（just build 出）" >&2
  exit 2
fi
# 镜像面自证（smoke-lockdep:60 同款防线）：镜像须含 DPGF 串——build-user
# 未跑/旧 userbins 烘的内核直拒，不烧帽才发现终态永缺。
if ! strings "$KERNEL" | grep -q 'DPGF ok n=15'; then
  echo "smoke-dpgsoak: 镜像缺 DPGF ok n=15 串（陈旧 userbins）——先 just build" >&2
  exit 2
fi
# rootfs 卷集解析（split 形优先——X-3c 后 rootfs 越单卷帽，正装 rootfs.cpio
# 34.5MB > 32MiB fw_cfg 单卷帽 = 先天不可递；手术面迁末 split 卷 = bench.py
# dpg_variant 同款，门档落归档尾与单卷时代同字节位。缺产物 loud 快败
# （smoke-lockdep 相 2 同款）。
VOL_COUNT=0
if [ -f "$ROOT/rootfs0.cpio" ]; then
  while [ -f "$ROOT/rootfs${VOL_COUNT}.cpio" ]; do VOL_COUNT=$((VOL_COUNT + 1)); done
elif [ -f "$ROOT/rootfs.cpio" ]; then
  VOL_COUNT=1
fi
[ "$VOL_COUNT" -ge 1 ] || {
  echo "smoke-dpgsoak: FAIL——缺 rootfs 卷（rootfs.cpio / rootfs0.cpio.. 均无——先 just build）" >&2
  exit 1
}
LAST_IDX=$((VOL_COUNT - 1))
if [ "$VOL_COUNT" -eq 1 ]; then
  LAST_VOL="$ROOT/rootfs.cpio"
  VARIANT="$ROOT/rootfs.dpgf.cpio"
else
  LAST_VOL="$ROOT/rootfs${LAST_IDX}.cpio"
  VARIANT="$ROOT/rootfs${LAST_IDX}.dpgf.cpio"
fi

PATH="$HOME/.cargo/bin:$PATH"
QEMU="qemu-system-aarch64"
MACH="-machine virt -cpu cortex-a72 -m 128M -nographic"
OVFL_CAP="${DPGF_OVFL_CAP:-40}"
# 内核 EREQ 缺页池槽数（dpg.rs EREQ_SLOTS 单源镜像——X-5 过滤取刀 8→32
# 起默认 32；env 覆盖供旧内核对跑/二分）。断言③极性由此派生（K≥15 期
# 望 ovfl 恰 0 = 容量余量证明；K<15 走原 C-full 池满面）。
DPGF_K="${DPGF_K:-32}"
DL_S="${DPGF_CAP_S:-600}"

# 门控变体卷：末卷手术 + dpgf.on（确定性产物：同输入同字节；bench.py
# dpg_variant 同款逻辑——split 形手术 rootfs<末>.cpio，单卷形手术 rootfs.cpio）。
python3 - "$LAST_VOL" "$VARIANT" "$SCRIPT_DIR" <<'PY' || exit 1
import sys
sys.path.insert(0, sys.argv[3])
from mkcpio import entry

src, dst = sys.argv[1], sys.argv[2]
blob = open(src, "rb").read()
# TRAILER!!! 名域唯一档尾（newc 头恰 110B = magic 6 + 13×hex8；rfind 取
# 最后一次 = 真 trailer——数据内偶合 ASCII 被跳过）。
name_pos = blob.rfind(b"TRAILER!!!")
assert name_pos > 0, f"{src} 无 TRAILER——档坏"
out = blob[: name_pos - 110]
out += entry(b"dpgf.on", b"", 0xF0000001)  # ino 避开正装递增域；字段余同定值
out += entry(b"TRAILER!!!", b"", 0)
out += b"\0" * (-len(out) % 4)
open(dst, "wb").write(out)
print(f"smoke-dpgsoak: 变体卷 {dst} {len(out)}B（末卷 {src} {len(blob)}B + dpgf.on 门）")
PY
# 递送集逐卷越帽执法（CRIT-CF1）：fw_cfg 单卷越内核 ROOTFS_MAX 32MiB =
# 静默 return None（无成因行）→ 门 miss → 烧满 DPGF_CAP_S 且诊断看不到
# 死因——split 形前缀卷同查防手改盘外档（bench.py dpg_boot 同款）。
i=0
while [ "$i" -lt "$VOL_COUNT" ]; do
  if [ "$i" -eq "$LAST_IDX" ]; then CHK="$VARIANT"; else CHK="$ROOT/rootfs${i}.cpio"; fi
  VSZ=$(wc -c < "$CHK" | tr -d ' ')
  [ "$VSZ" -le 33554432 ] || {
    echo "smoke-dpgsoak: FAIL——递送卷 $CHK ${VSZ}B 越帽 32MiB ROOTFS_MAX（内核 fw_cfg 静默拒收→门 miss 烧帽）——瘦身 rootfs 后重建" >&2
    exit 1
  }
  i=$((i + 1))
done

# 递送卷 fw_cfg 参数集（多卷名 = 内核连续前缀发现域 opt/berry/rootfs0..9，
# smoke.sh A1-3b / bench.py dpg_boot 同口径；单卷形退旧名 opt/berry/rootfs）。
FW_ARGS=()
if [ "$VOL_COUNT" -eq 1 ]; then
  FW_ARGS=(-fw_cfg "name=opt/berry/rootfs,file=$VARIANT")
else
  i=0
  while [ "$i" -lt "$VOL_COUNT" ]; do
    if [ "$i" -eq "$LAST_IDX" ]; then DELIVER="$VARIANT"; else DELIVER="$ROOT/rootfs${i}.cpio"; fi
    FW_ARGS+=(-fw_cfg "name=opt/berry/rootfs${i},file=$DELIVER")
    i=$((i + 1))
  done
fi

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-smoke-dpgsoak.XXXXXX")
EVS_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-evsclient.XXXXXX")
SKT_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-sktclient.XXXXXX")
ECHO_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-echod.XXXXXX")
keep_log() {
  if [ -n "${SMOKE_DPGSOAK_KEEP_LOG:-}" ] || [ "$1" = "keep" ]; then
    echo "smoke-dpgsoak: 串口日志保留：$LOG" >&2
    echo "smoke-dpgsoak: 变体卷保留：$VARIANT" >&2
    for p in "$EVS_LOG" "$SKT_LOG" "$ECHO_LOG"; do
      echo "smoke-dpgsoak: 客户端日志保留：$p" >&2
    done
  else
    rm -f "$LOG" "$EVS_LOG" "$SKT_LOG" "$ECHO_LOG" "$VARIANT"
  fi
}
P2_PIDS=""
p2_cleanup() {
  local p
  for p in $P2_PIDS; do
    kill "$p" 2>/dev/null
    wait "$p" 2>/dev/null
  done
}
trap p2_cleanup EXIT

# 盘名专属 disk.dpgf.img + LBA0 印章（smoke-lockdep 同款：防覆写兄弟线活盘）。
DISK="$ROOT/disk.dpgf.img"
python3 "$SCRIPT_DIR/mkfat.py" "$DISK" || {
  echo "smoke-dpgsoak: FAIL——mkfat 造盘失败（${DISK}）" >&2
  keep_log keep
  exit 1
}
printf 'BERRYOSVBLK!' | dd of="$DISK" conv=notrunc status=none

# 端口让场（18080-18083 兄弟线在用即退——smoke/soak 族同值端口 = 天然
# 串行协议）。
if lsof -nP -iTCP:18080-18083 -sTCP:LISTEN >/dev/null 2>&1; then
  echo "smoke-dpgsoak: FAIL——18080-18083 端口族被占（lsof）——兄弟 smoke/soak 线在跑？让场后重试" >&2
  keep_log keep
  exit 1
fi
# QEMU 字段让行（smoke-lockdep 同款签名——不抢跑兄弟线）。
waited=0
while pgrep -f 'qemu-system.*(-machine virt|-kernel)' >/dev/null 2>&1; do
  waited=$((waited + 1))
  [ "$waited" -gt 5 ] && { echo "smoke-dpgsoak: FAIL——QEMU 字段被占，放弃" >&2; keep_log keep; exit 1; }
  echo "smoke-dpgsoak: QEMU 字段被占，等待 15s（第 ${waited} 轮）…" >&2
  sleep 15
done

# 宿主对端三件套（§A122 面；sktclient 懒启动 = guest `EVS ok` 后才 exec
# ——smoke-lockdep 相 2 同款）。
# evsclient 帽 env 覆盖（S-C5：由 DL_S 派生 = 帽序「client 帽 ≥ 相帽 +
# 360s」恒成立；concur-2 定谳 = 断言族无 EVS 面，此帽纯卫生无执法受害者）。
export BERRY_EVS_TOTAL_CAP_S=$((DL_S + 360))
export BERRY_EVS_ATTEMPT_CAP_S=90
python3 "$SCRIPT_DIR/evsclient.py" 18082 EVS > "$EVS_LOG" 2>&1 & EVS_PID=$!
( until grep -q 'EVS ok' "$LOG" 2>/dev/null; do sleep 2; done
  exec python3 "$SCRIPT_DIR/evsclient.py" 18083 SKT ) > "$SKT_LOG" 2>&1 & SKT_PID=$!
python3 "$SCRIPT_DIR/echod.py" > "$ECHO_LOG" 2>&1 & ECHO_PID=$!
P2_PIDS="$EVS_PID $SKT_PID $ECHO_PID"
te=$(date +%s)
until grep -q 'listening on' "$ECHO_LOG" 2>/dev/null; do
  kill -0 "$ECHO_PID" 2>/dev/null || {
    echo "smoke-dpgsoak: FAIL——echod 启动失败（18080 被占？）：$(cat "$ECHO_LOG")" >&2
    keep_log keep
    exit 1
  }
  [ $(( $(date +%s) - te )) -gt 5 ] && {
    echo "smoke-dpgsoak: FAIL——echod 5s 未 listening" >&2
    keep_log keep
    exit 1
  }
  sleep 0.2
done

echo "smoke-dpgsoak: QEMU 启动（-smp 2，门控变体卷 + virtio-blk/net hostfwd 三口 + tablet）"
t0=$(date +%s)
"$QEMU" $MACH -smp 2 -kernel "$KERNEL" \
  "${FW_ARGS[@]}" \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file="$DISK",cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082,hostfwd=tcp::18083-:18083 -device virtio-net-device,netdev=n0 \
  -device virtio-tablet-device,id=tab0 \
  < /dev/null > "$LOG" 2>&1 &
QPID=$!
P2_PIDS="$QPID $P2_PIDS"

# 终态轮询：`DPGF2 ok n=3`（C-4 二轮末标记——它到 = 波 1 + 槽回收全毕）；
# fail-fast = eco dpgf 真降级 / panic 族早断真断链（t-37 容差行排除——
# 60d3bb9 起拆台 -37 合法吸收，不掐轮询），帽只兜挂死。
deadline=$((SECONDS + DL_S))
while :; do
  { grep -E 'eco: dpgf|panicked|PANIC' "$LOG" 2>/dev/null || true; } | grep -vF 't-37' | grep -q . && break
  grep -q 'DPGF2 ok n=3' "$LOG" 2>/dev/null && break
  kill -0 "$QPID" 2>/dev/null || break
  [ $SECONDS -ge $deadline ] && break
  sleep 1
done
kill "$QPID" 2>/dev/null
wait "$QPID" 2>/dev/null
sleep 3   # SIGTERM 后串口落定（P6 A-6 实证同款）
echo "smoke-dpgsoak: 墙钟 $(( $(date +%s) - t0 ))s（QEMU 起→DPGF2 终态收割；帽 ${DL_S}s）" | tee -a "$LOG"
kill "$EVS_PID" "$SKT_PID" "$ECHO_PID" 2>/dev/null
wait "$EVS_PID" "$SKT_PID" "$ECHO_PID" 2>/dev/null
P2_PIDS=""

rc=0
# ① DPGF 恰 1。
N1=$(grep -c 'DPGF ok n=15' "$LOG" 2>/dev/null || true)
if [ "$N1" -ne 1 ]; then
  echo "smoke-dpgsoak: FAIL——DPGF ok n=15 计数 ${N1}（期望恰 1；0 = 波 1 未毕/超时/降级）" >&2
  rc=1
fi
# ② DPGF2 恰 1。
N2=$(grep -c 'DPGF2 ok n=3' "$LOG" 2>/dev/null || true)
if [ "$N2" -ne 1 ]; then
  echo "smoke-dpgsoak: FAIL——DPGF2 ok n=3 计数 ${N2}（期望恰 1；0 = C-4 二轮未毕——槽回收面查）" >&2
  rc=1
fi
# ③ ovfl 极性随 K 派生：K ≥ N=15 → 期望恰 0（容量余量——15 并发臂全
#    驻留不触池满，非零 = EREQ_SLOTS 抬幅/过滤取回归）；K < N → 原面
#    [1, OVFL_CAP]（池满必触达 + 有界；溢出机器面本体已迁 ktest 同刀）。
OV=$(grep -c 'DPG ovfl' "$LOG" 2>/dev/null || true)
if [ "$DPGF_K" -ge 15 ]; then
  if [ "$OV" -ne 0 ]; then
    echo "smoke-dpgsoak: FAIL——DPG ovfl 计数 ${OV}（K=$DPGF_K ≥ 15 容量余量面期望 0——非零 = 池容量/过滤取回归，查 EREQ_SLOTS）" >&2
    rc=1
  fi
elif [ "$OV" -lt 1 ]; then
  echo "smoke-dpgsoak: FAIL——DPG ovfl 计数 ${OV}（15>K=$DPGF_K 池满必触达——零计 = 溢出观测面断）" >&2
  rc=1
elif [ "$OV" -gt "$OVFL_CAP" ]; then
  echo "smoke-dpgsoak: FAIL——DPG ovfl 计数 $OV 超帽 ${OVFL_CAP}（convoy 重印失稳？env DPGF_OVFL_CAP 校准）" >&2
  rc=1
fi
# ④ 五行行序（boot 生态序：DPGP → DPGS → DPGN → DPGF → DPGF2 严格递增）。
l_p=$(grep -nF 'DPGP ok' "$LOG" 2>/dev/null | head -1 | cut -d: -f1)
l_s=$(grep -nF 'DPGS ok n=4' "$LOG" 2>/dev/null | head -1 | cut -d: -f1)
l_n=$(grep -nF 'DPGN ok n=4' "$LOG" 2>/dev/null | head -1 | cut -d: -f1)
l_f=$(grep -nF 'DPGF ok n=15' "$LOG" 2>/dev/null | head -1 | cut -d: -f1)
l_f2=$(grep -nF 'DPGF2 ok n=3' "$LOG" 2>/dev/null | head -1 | cut -d: -f1)
if [ -z "$l_p" ] || [ -z "$l_s" ] || [ -z "$l_n" ] || [ -z "$l_f" ] || [ -z "$l_f2" ] \
   || [ "$l_p" -ge "$l_s" ] || [ "$l_s" -ge "$l_n" ] || [ "$l_n" -ge "$l_f" ] || [ "$l_f" -ge "$l_f2" ]; then
  echo "smoke-dpgsoak: FAIL——dpg 五行行序断言（DPGP=$l_p < DPGS=$l_s < DPGN=$l_n < DPGF=$l_f < DPGF2=$l_f2 须全非空严格递增）" >&2
  rc=1
fi
# ⑤ eco dpgf 真降级零计（t-37 容差行分账不计）+ panic 族零计。
ECO=$(grep 'eco: dpgf' "$LOG" 2>/dev/null | grep -vF 't-37' | wc -l | tr -d ' ')
T37=$(grep -cF 'eco: dpgf t-37' "$LOG" 2>/dev/null || true)
if [ "$ECO" -ne 0 ]; then
  echo "smoke-dpgsoak: FAIL——eco dpgf 降级行 $ECO 条（波内坏面早断——tail 看相位）" >&2
  rc=1
fi
PN=$(grep -cE 'panicked|PANIC' "$LOG" 2>/dev/null || true)
if [ "$PN" -ne 0 ]; then
  echo "smoke-dpgsoak: FAIL——panic 族 $PN 条" >&2
  rc=1
fi

# ⑥ lockdep 面 VIOLATION 零执法（concur-1）：banner 在席才断言（default
# 面 ELF 无 LOCKDEP 自测——缺席不断言，C-3 行序延伸仍另挂）。
if grep -q 'LOCKDEP selftest ok' "$LOG" 2>/dev/null; then
  LV=$(grep -c 'VIOLATION' "$LOG" 2>/dev/null || true)
  if [ "$LV" -ne 0 ]; then
    echo "smoke-dpgsoak: FAIL——lockdep VIOLATION $LV 条（banner 在席须 0——头注头号不变量机器执法）" >&2
    rc=1
  fi
fi

if [ "$rc" -eq 0 ]; then
  echo "smoke-dpgsoak: PASS（DPGF n=15 满配 + DPGF2 n=3 槽回收 + ovfl $OV ∈ [1,$OVFL_CAP] + 五行行序 + 降级/panic 零 + t-37 容差 ${T37}）" | tee -a "$LOG"
  keep_log drop
else
  echo "smoke-dpgsoak: FAIL（判词见上/stderr；串口 + 三 client 档随 keep 留存）" >> "$LOG"
  echo "---- 串口日志尾部（${LOG}）----" >&2
  tail -40 "$LOG" >&2
  # ovfl 观测行全量随失败打印（首跑标定面：重印分布 = 帽值依据）。
  echo "---- DPG ovfl 行分布 ----" >&2
  grep -n 'DPG ovfl' "$LOG" 2>/dev/null | head -50 >&2
  # 客户端死因随失败即打印不留文件（§A122 第一手死因）。
  for p in "echod:$ECHO_LOG" "evsclient:$EVS_LOG" "sktclient:$SKT_LOG"; do
    lg="${p#*:}"
    if [ -s "$lg" ]; then
      echo "---- ${p%%:*} 日志尾部（${lg}）----" >&2
      tail -10 "$lg" >&2
    fi
  done
  keep_log keep
fi
exit "$rc"
