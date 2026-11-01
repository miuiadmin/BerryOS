#!/usr/bin/env bash
# BerryOS S-1 soak 长跑通道体（docs/基准与压力测试设计.md §11）——`just soak <分钟数>` 的实现。
#
# 做五件事：
#   1. 复刻 run-rootfs 全量 QEMU 环境（-fw_cfg rootfs + virtio-blk/net + echod
#      TCP 回显对端 + 盘线 cache.writeback=off），另递门控键
#      `-fw_cfg name=opt/berry/soak,string=1`——内核侧 soakmon 仅该具名项在位
#      才 spawn（R-1 opt/berry/rt 同款先例）；stdin=/dev/null（bsh 空转读循环，
#      lost 恒 0 断言才有意义）。全量不自建最小面：soak 的被测面 = 稳态矩阵 +
#      常驻服务主体（svcmgr/fsmgr/vblk/vnet），砍设备 = 覆盖面静默缩水。
#   2. boot 覆盖守卫：SOAK gate on（门控自证——内核门控块内打印，行在
#      ⟺ soakmon 已 spawn）+ ECO/VIRTIO/FAT/NET/DNS/TCP/TCPS 七标记 300s
#      内到齐——常驻服务族缺席必须 loud，不能带着空心覆盖面绿跑一小时。
#      vnet 服务链尾三标记（DNS/TCP/TCPS）必收：echod 缺席/TCP 自验 trap
#      时 vnet 死在 serve() 之前，这三标记整族缺席而四标记照齐（对抗审查
#      finding 3/6——boot 期守卫罩不住它自己举例的 echod 链即此洞）；
#      echod 宿主侧启动缺席由本脚本 listening 轮询 loud（两层各管一段）。
#   3. 轮询收割心跳行（**非锚定** grep -oE：soakmon prio 8 到点抢占 prio-0
#      矩阵任务的逐字符打印流，SOAK 行必然嵌进 ABAB 巨行中段——R-1 BENCHRT
#      首跑实证同族，^ 行锚定恒漏收）；四断言逐拍（全序列重验，日志只增）：
#        ① t 严格 +1 且从 1 起（丢拍 = 静默死）；
#        ② sw 严格递增（调度活着——「冻而不死」kill -0 活性检抓不到，此断言抓）；
#        ③ pages ∈ [首拍值, 首拍值+64]（+64 帽 = 懒增长容忍量，持续上涨 = 真泄漏；
#           首拍基线由内核侧首拍潜伏 5 周期保证落在 boot 尾部之后——无潜伏则
#           boot 尾部帧回收瞬态会把 pages 打下穿首拍 = 假 FAIL；3 周期在本机
#           实测 ≈182s wall 恰贴 walk 尾部末段，宿主负载拉伸尾部即失保证，
#           故 5 周期 ≈300-600s wall 对 300s 最坏链 ≥1.6x 裕量）；
#        ④ lost 恒 0（无输入面丢字 = console 环健康）。
#   4. 帽：首拍 = 300（boot）+ 5×150（潜伏五周期）+ 60 余量；此后单拍 150s
#      （60s vct 心跳本机实测 ≈60s wall（CNTVCT ≈1.0×），历史半率机型
#      ≈120s——按半率规划 150s = 1.25x 兜抖动，实测余量 2.5x）；总帽
#      300 + (N+5)×150 + 120（兜结构性挂死）。
#   5. QEMU 全程活性检（kill -0：panic/halt 提前退出即 FAIL）；收尾
#      kill + wait + sleep 3（P6 A-6：QEMU 由 SIGTERM 终止后仍存活数秒并继续
#      刷盘——盘取证/复用前必等真退，设计 §11 明文）；FAIL 留全串口日志。
#
# 注意：echod 18080 宿主端口 guest 侧定值不可参数化——**勿与 smoke/run-rootfs
# 并行跑**（端口冲突 → 本脚本 listening 轮询 loud fail）。盘名 disk.soak.img
# 专属（对抗审查 finding 10）——与 smoke/run-rootfs 的 disk.img 零交集，
# 并行窗口内盘互毁面已闭（端口面仍在，警告保留）。A2-3 起再加 18082
#（EVS client）同款端口面——evsclient 不做 listening 自证（无监听面），
# 冲突死状 = client 重试环打满 240s 总帽 FAIL（boot 尾收口处 loud）。
# 调试：SOAK_KEEP_LOG=1 保留串口日志不删（成功也留；FAIL 恒留）。
set -u

# 内核路径解析收敛在 paths.sh（单一事实源，justfile/CI 同源；含非交互 PATH 修补）。
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR/.."
source "$SCRIPT_DIR/paths.sh"
KERNEL=$(berryos_kernel debug)

MINUTES=${1:-60}
case "$MINUTES" in
  ''|*[!0-9]*) echo "soak: FAIL: 分钟数须为正整数，得到：$MINUTES" >&2; exit 1 ;;
esac
[ "$MINUTES" -ge 1 ] || { echo "soak: FAIL: 分钟数须 ≥1（1 拍序列平凡无鉴别力，短验建议 2 起）" >&2; exit 1; }

QEMU=${QEMU:-qemu-system-aarch64}
command -v "$QEMU" >/dev/null 2>&1 || { echo "soak: FAIL: 找不到 $QEMU" >&2; exit 1; }
[ -f "$KERNEL" ] || { echo "soak: FAIL: 内核未构建（先 just build）：$KERNEL" >&2; exit 1; }
[ -f rootfs.cpio ] || { echo "soak: FAIL: 缺 rootfs.cpio（build-user 应产出）" >&2; exit 1; }

DEADLINE_BOOT=300   # 秒：boot 覆盖七标记 + 门控自证行到齐（smoke DEADLINE_ROOTFS 同源口径）
PER_BEAT_CAP=150    # 秒：单拍 wall 帽（本机实测 ≈60s wall 的 2.5x；历史半率机型 ≈120s 的 1.25x）
FIRST_BEAT_CAP=$(( DEADLINE_BOOT + 5 * PER_BEAT_CAP + 60 ))     # 首拍：boot + 潜伏五周期 + 余量
TOTAL_CAP=$(( DEADLINE_BOOT + (MINUTES + 5) * PER_BEAT_CAP + 120 ))  # 总帽（兜结构性挂死）
POLL=2

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-soak.XXXXXX")
QEMU_PID=""
ECHO_PID=""
ECHO_LOG=""
# A2-3：EVS echo server 宿主对拍 client——soak 带 netdev = bsh evs_selftest
# 必跑（netd_h()≠0），client 缺席则 bsh 卡 accept-wait 120s 后 bail = 交互
# 链断 + EVS ok 缺席覆盖守卫 FAIL（echod 必抄同款：三行 shell 换 bsh 活链）。
EVS_PID=""
EVS_LOG=""

KEEP=0
[ -n "${SOAK_KEEP_LOG:-}" ] && KEEP=1

cleanup() {
  if [ -n "$QEMU_PID" ] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null
    wait "$QEMU_PID" 2>/dev/null
  fi
  # P6 A-6 纪律：kill 后 wait 真退 + 再 sleep 3（SIGTERM 后仍存活数秒刷盘）。
  sleep 3
  if [ -n "$ECHO_PID" ] && kill -0 "$ECHO_PID" 2>/dev/null; then
    kill "$ECHO_PID" 2>/dev/null
    wait "$ECHO_PID" 2>/dev/null
  fi
  # A2-3：evsclient 兜底收（正常流 boot 尾已收清——这里只兜中断在途实例）。
  if [ -n "$EVS_PID" ] && kill -0 "$EVS_PID" 2>/dev/null; then
    kill "$EVS_PID" 2>/dev/null
    wait "$EVS_PID" 2>/dev/null
  fi
  if [ "$KEEP" -eq 1 ]; then
    echo "soak: 串口日志保留：$LOG" >&2
    [ -n "$ECHO_LOG" ] && echo "soak: echod 日志保留：$ECHO_LOG" >&2
    [ -n "$EVS_LOG" ] && echo "soak: evsclient 日志保留：$EVS_LOG" >&2
  else
    rm -f "$LOG"
    [ -n "$ECHO_LOG" ] && rm -f "$ECHO_LOG"
    [ -n "$EVS_LOG" ] && rm -f "$EVS_LOG"
  fi
}
trap cleanup EXIT

fail() {
  echo "soak: FAIL: $1" >&2
  echo "---- 心跳表（全部已收拍）----" >&2
  grep -aoE 'SOAK t=[0-9]+ sw=[0-9]+ pages=[0-9]+ lost=[0-9]+' "$LOG" >&2 || true
  echo "---- 串口日志尾部（全文保留：$LOG）----" >&2
  tail -40 "$LOG" >&2
  if [ -n "$ECHO_LOG" ] && [ -s "$ECHO_LOG" ]; then
    echo "---- echod 日志尾部（$ECHO_LOG）----" >&2
    tail -10 "$ECHO_LOG" >&2
  fi
  if [ -n "$EVS_LOG" ] && [ -s "$EVS_LOG" ]; then
    echo "---- evsclient 日志尾部（$EVS_LOG）----" >&2
    tail -10 "$EVS_LOG" >&2
  fi
  echo "--------------------------------" >&2
  KEEP=1   # 失败必留全日志（含 echod 现场——smoke T-c 对抗审查教训同源）
  exit 1
}

# 心跳提取：非锚定（SOAK 行嵌 ABAB 巨行中段是常态，见头注 3）。-a = 串口
# 日志混入任一无效 UTF-8/NUL 字节即触发 BSD grep 二进制模式全文件静默零收割
# （smoke.sh 串口日志断言一律 grep -a 同款先例，对抗审查 finding 8）。
beats_raw() {
  grep -aoE 'SOAK t=[0-9]+ sw=[0-9]+ pages=[0-9]+ lost=[0-9]+' "$LOG" 2>/dev/null || true
}

# 四断言（输入行 "t sw pages lost"）：破即打印诊断行退 1；全过末行打印摘要。
# awk 陷阱实录：action 里裸 `exit` 退出码恒 0——`|| fail` 永不触发、违规只能
# 等总帽超时才 FAIL（诊断误导）。必须 `exit 1` + END 里 `if (bad) exit 1`
# 双保险（END 会照跑，别让它的正常返回覆盖挂起的 1）。
validate_beats() {
  beats_raw | sed -E 's/^SOAK t=//; s/ sw=/ /; s/ pages=/ /; s/ lost=/ /' | awk '
    {
      t=$1; sw=$2; pg=$3; lo=$4
      if (NR == 1) {
        if (t != 1) { printf "首拍 t=%d != 1\n", t; bad=1; exit 1 }
        base = pg; pmin = pg; pmax = pg
      } else {
        if (t != pt + 1) { printf "t=%d 非严格 +1（前 t=%d）——丢拍/静默死\n", t, pt; bad=1; exit 1 }
        if (sw <= psw) { printf "t=%d sw=%d 非严格递增（前 sw=%d）——调度冻死\n", t, sw, psw; bad=1; exit 1 }
      }
      if (pg < base || pg > base + 64) { printf "t=%d pages=%d 越窗 [基线 %d, %d]——泄漏/回收异常\n", t, pg, base, base + 64; bad=1; exit 1 }
      if (lo != 0) { printf "t=%d lost=%d 非 0——console 环丢字\n", t, lo; bad=1; exit 1 }
      if (pg < pmin) pmin = pg
      if (pg > pmax) pmax = pg
      pt=t; psw=sw; n=NR
    }
    # 摘要 pages= 打观测区间 [min,max]（非两次基线——拍间波动被抹平会误导
    # 事后读摘要的排障/回归对比，对抗审查 finding 5；下界恒=基线是断言③
    # 执法结果，max 反映窗口内真实高水位）。
    END { if (bad) exit 1; printf "n=%d last_t=%d last_sw=%d pages=%d..%d", n, pt, psw, pmin, pmax }'
}

# —— 造盘（run-rootfs 同款确定性 FAT12 卷 + LBA0 印章；每次重造 = 跨跑零残留）。
# 盘名 disk.soak.img 专属：smoke.sh/justfile run-rootfs 共写 repo 根 disk.img——
# 并行窗口（smoke 头两相 echod 未起、端口空闲）下同名盘互毁 FAT 卷，专属名
# 归零交集（对抗审查 finding 10）。
python3 tools/mkfat.py disk.soak.img || { echo "soak: FAIL: mkfat 造盘失败" >&2; exit 1; }
printf 'BERRYOSVBLK!' | dd of=disk.soak.img conv=notrunc status=none

# —— echod：vnet TCP 自验对端（必抄，侦察 Q4 裁决）——缺席则 guest connect
# 被拒 → 握手失败 → vnet trap 自陷死在 serve() 之前 → DNS/TCP 常驻服务族
# 整族缺席（覆盖面静默缩水）+ svcmgr 侧两回 60s vct 应答帽白等（TCG ≈4min
# wall）。三行 shell 换全常驻族在席。
ECHO_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-echod.XXXXXX")
python3 tools/echod.py > "$ECHO_LOG" 2>&1 &
ECHO_PID=$!
te=$(date +%s)
while ! grep -q 'listening on' "$ECHO_LOG" 2>/dev/null; do
  kill -0 "$ECHO_PID" 2>/dev/null \
    || fail "echod 启动失败（18080 被占？勿与 smoke/run-rootfs 并行）：$(cat "$ECHO_LOG")"
  [ $(( $(date +%s) - te )) -gt 5 ] && fail "echod 5s 未 listening"
  sleep 0.2
done

# —— A2-3 evsclient：bsh EVS echo server 对拍对端（必抄 echod 模式）——
# 起在 QEMU 前（client 重试环吃 hostfwd 未就绪态，240s 总帽内必达）；boot
# 覆盖标记到齐后收口（30s 帽只盖透传余量），成功后收进程清槽——此后
# 1 小时长跑与 client 零纠缠。
EVS_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-evsclient.XXXXXX")
python3 tools/evsclient.py > "$EVS_LOG" 2>&1 &
EVS_PID=$!

# —— QEMU（run-rootfs 全量形态 + soak 门控键；stdin /dev/null）——
echo "soak: QEMU 启动（$MINUTES 拍；首拍帽 ${FIRST_BEAT_CAP}s / 单拍帽 ${PER_BEAT_CAP}s / 总帽 ${TOTAL_CAP}s）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio \
  -fw_cfg name=opt/berry/soak,string=1 \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.soak.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082 -device virtio-net-device,netdev=n0 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# —— 阶段 1：boot 覆盖七标记 + 门控自证（常驻服务主体在席守卫，头注 2）——
# DNS/TCP/TCPS 三标记是 vnet 转常驻服务链的活性终点：echod 中途死/TCP 自验
# trap 时整族缺席而 ECO/VIRTIO/FAT/NET 照齐——四标记守卫罩不住自己举例的
# 失败链（对抗审查 finding 3/6）；'SOAK gate on' = 内核门控块内打印（行在
# ⟺ soakmon 已 spawn），键名漂移在此 loud 而非等首拍帽超时。A2-3 起
# + EVS ok n=2（bsh 侧 netd 会话消费链终点——client 缺席/对拍失配在此
# loud，而非等 bsh 120s accept-wait bail 后交互链静默断）。
t0=$(date +%s)
COVER=("SOAK gate on" "ECO ok" "VIRTIO ok" "FAT ok n=" "NET ok ping=" "DNS ok addr=" "TCP ok echo=" "TCPS ok echo=" "EVS ok n=2")
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 提前退出（boot 期崩溃？）"
  missing=""
  for m in "${COVER[@]}"; do
    grep -aqF "$m" "$LOG" || missing="${missing:+$missing }$m"
  done
  [ -z "$missing" ] && break
  [ $(( $(date +%s) - t0 )) -gt "$DEADLINE_BOOT" ] \
    && fail "boot 覆盖标记 ${DEADLINE_BOOT}s 未到齐，缺：$missing（常驻服务族缺席/门控键漂移 = 覆盖面缩水或 soakmon 未 spawn，勿空心绿跑）"
  sleep 2
done
echo "soak: boot 覆盖标记 ${#COVER[@]}/${#COVER[@]} 到齐（含 SOAK gate on 门控自证；svcmgr/fsmgr/vblk/vnet 常驻族在席）"

# A2-3：收 evsclient 对拍结果（guest EVS ok 已在覆盖面到齐——30s 帽只盖
# 透传余量；对拍失配/重试环打满 = loud，不留到 1 小时后才见）。
tc=$(date +%s)
while ! grep -q 'EVS client ok n=2' "$EVS_LOG" 2>/dev/null; do
  kill -0 "$EVS_PID" 2>/dev/null \
    || fail "evsclient 提前退出：$(tail -5 "$EVS_LOG" | tr '\n' ' ')"
  [ $(( $(date +%s) - tc )) -gt 30 ] \
    && fail "EVS client 30s 未对拍完成（guest EVS ok 已到——查 EVS_LOG 重试诊断）"
  sleep 1
done
kill "$EVS_PID" 2>/dev/null
wait "$EVS_PID" 2>/dev/null
rm -f "$EVS_LOG"
EVS_PID=""
EVS_LOG=""
echo "soak: EVS client 对拍完成（两连接 64B/24B 逐字节）"

# —— 阶段 2：心跳四断言轮询（首拍潜伏 5 周期 ≈300-600s wall 是内核侧设计，非挂死）——
tq=$(date +%s)
last_n=0
last_beat_wall=$tq
summary="(无拍)"
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 中途退出（崩溃/panic？——心跳表与日志尾部见上）"
  now=$(date +%s)
  [ $(( now - tq )) -gt "$TOTAL_CAP" ] && fail "总帽 ${TOTAL_CAP}s 破（已收 ${last_n}/${MINUTES} 拍）"
  n=$(beats_raw | wc -l | tr -d ' ')
  n=${n:-0}
  if [ "$n" -gt 0 ]; then
    summary=$(validate_beats) || fail "四断言破：$summary"
  fi
  if [ "$n" -ge "$MINUTES" ]; then
    break
  fi
  if [ "$n" -eq 0 ]; then
    [ $(( now - tq )) -gt "$FIRST_BEAT_CAP" ] \
      && fail "首拍 ${FIRST_BEAT_CAP}s 未到（boot 失败 / fw_cfg 门控未命中 / 撕行漏拍？）"
  else
    if [ "$n" -gt "$last_n" ]; then
      last_n=$n
      last_beat_wall=$now
    fi
    [ $(( now - last_beat_wall )) -gt "$PER_BEAT_CAP" ] \
      && fail "第 $(( last_n + 1 )) 拍缺席超单拍帽 ${PER_BEAT_CAP}s（调度冻死/丢拍——冻而不死 kill -0 抓不到，此断言抓）"
  fi
  sleep "$POLL"
done

elapsed=$(( $(date +%s) - tq ))
echo "soak: PASS ${MINUTES}/${MINUTES} 拍四断言全绿（$summary）"
echo "soak: 全程 ${elapsed}s（总帽 ${TOTAL_CAP}s）"
