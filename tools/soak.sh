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
#      ⟺ soakmon 已 spawn）+ ECO/VIRTIO/SKT/FAT/NET/DNS/TCP/TCPS/EVS 十标记
#      600s 内到齐（A2-3/A3-3 起 EVS/SKT 对拍链在内）——常驻服务族缺席必须
#      loud，不能带着空心覆盖面绿跑一小时。
#      vnet 服务链尾三标记（DNS/TCP/TCPS）必收：echod 缺席/TCP 自验 trap
#      时 vnet 死在 serve() 之前，这三标记整族缺席而四标记照齐（对抗审查
#      finding 3/6——boot 期守卫罩不住它自己举例的 echod 链即此洞）；
#      echod 宿主侧启动缺席由本脚本 listening 轮询 loud（两层各管一段）。
#   3. 轮询收割心跳行（**非锚定** grep -oE：soakmon prio 8 到点抢占 prio-0
#      矩阵任务的逐字符打印流，SOAK 行必然嵌进 ABAB 巨行中段——R-1 BENCHRT
#      首跑实证同族，^ 行锚定恒漏收）；六断言逐拍（全序列重验，日志只增；
#      心跳 v2〔S-5d D4〕八字段 = t/sw/sw0/sw1/park0/park1/pages/lost）：
#        ① t 严格 +1 且从 1 起（丢拍 = 静默死）；
#        ② sw 严格递增（调度活着——「冻而不死」kill -0 活性检抓不到，此断言抓）；
#        ③ sw0 严格递增（单核 = CPU0 独跑，与 ② 同事件面——v2 per-CPU 口径）；
#        ④ sw1==0 ∧ park1==0 恒零（单核 boot 副核统计零活性自证——违例 =
#           per-CPU 计数器跨核错写/读撕面）；
#        ⑤ pages 稳态窗 ∈ [第 6 拍值, +64]（+64 帽 = 懒增长容忍量，持续上涨
#           = 真泄漏；S-5b 带宽口径原样）——首 5 拍结算窗豁免：boot 尾 bsh
#           自验族瞬态（S-5d A 跑实测 FAT n=5000 释放波 −773 页横跨首拍
#           基线、FATW/df/dir/cd 小波尾）非泄漏证据；首拍潜伏 5 周期 +
#           豁免 5 拍 = 稳态基线（第 6 拍，≈guest t=360s）钉在自验族收尾后；
#        ⑥ lost 恒 0（无输入面丢字 = console 环健康）。
#      （park0 无断言：单核 soak 常态大量 idle park，满载谓词是 soak-smp 通道
#      的断言面——双核满载核 park 停增，见 tools/soak-smp.sh。）
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

DEADLINE_BOOT=600   # 秒：boot 覆盖十标记 + 门控自证行到齐。600 = 防挂死兜底
                    # 非性能门：FAT ok 基线 ≈200-206s（smoke.sh B4 归因实测）对
                    # 300s 余量 <1.5x，宿主争用拉伸即翻（S-5d 三场红定谳后升
                    # 600 = 基线 3x 裕量；再超 = R1 审计非再升）
PER_BEAT_CAP=150    # 秒：单拍 wall 帽（本机实测 ≈60s wall 的 2.5x；历史半率机型 ≈120s 的 1.25x）
FIRST_BEAT_CAP=$(( 5 * PER_BEAT_CAP + 60 ))    # 首拍：潜伏五周期 + 余量（tq 从 boot
                 # 收拍后起算——不含 boot 分量；S-5e m7 勘误旧公式双计
                 # DEADLINE_BOOT=600：方向保守不产假绿，现口径即所言）
TOTAL_CAP=$(( (MINUTES + 5) * PER_BEAT_CAP + 120 ))  # 总帽（兜结构性挂死；同上不含 boot 分量）
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
# A3-3：SKT socket fd 皮宿主对拍 client——bsh skt_selftest 的 accept 对端
# （hostfwd 18083 专线）。**缺席则 bsh 卡 SKT 无帽 accept → EPL/NETS/MMP/
# CLK/SIG/fat 族整族永不执行 → FAT ok 永缺**（S-5d 定谳：9ab8a68 A3-3 给
# smoke.sh 加 sktclient 而 soak 通道未跟进，本脚本停在 501d15c 旧对端集
# 静默断链至今——soak 绿迹停在 S-1 即此窗）。
SKT_PID=""
SKT_LOG=""

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
  # A3-3：sktclient 兜底收（同上）。
  if [ -n "$SKT_PID" ] && kill -0 "$SKT_PID" 2>/dev/null; then
    kill "$SKT_PID" 2>/dev/null
    wait "$SKT_PID" 2>/dev/null
  fi
  if [ "$KEEP" -eq 1 ]; then
    echo "soak: 串口日志保留：$LOG" >&2
    [ -n "$ECHO_LOG" ] && echo "soak: echod 日志保留：$ECHO_LOG" >&2
    [ -n "$EVS_LOG" ] && echo "soak: evsclient 日志保留：$EVS_LOG" >&2
    [ -n "$SKT_LOG" ] && echo "soak: sktclient 日志保留：$SKT_LOG" >&2
  else
    rm -f "$LOG"
    [ -n "$ECHO_LOG" ] && rm -f "$ECHO_LOG"
    [ -n "$EVS_LOG" ] && rm -f "$EVS_LOG"
    [ -n "$SKT_LOG" ] && rm -f "$SKT_LOG"
  fi
}
trap cleanup EXIT

fail() {
  echo "soak: FAIL: $1" >&2
  echo "---- 心跳表（全部已收拍）----" >&2
  grep -aoE 'SOAK t=[0-9]+ sw=[0-9]+ sw0=[0-9]+ sw1=[0-9]+ park0=[0-9]+ park1=[0-9]+ pages=[0-9]+ lost=[0-9]+' "$LOG" >&2 || true
  echo "---- 串口日志尾部（全文保留：${LOG}）----" >&2
  tail -40 "$LOG" >&2
  if [ -n "$ECHO_LOG" ] && [ -s "$ECHO_LOG" ]; then
    echo "---- echod 日志尾部（${ECHO_LOG}）----" >&2
    tail -10 "$ECHO_LOG" >&2
  fi
  if [ -n "$EVS_LOG" ] && [ -s "$EVS_LOG" ]; then
    echo "---- evsclient 日志尾部（${EVS_LOG}）----" >&2
    tail -10 "$EVS_LOG" >&2
  fi
  if [ -n "$SKT_LOG" ] && [ -s "$SKT_LOG" ]; then
    echo "---- sktclient 日志尾部（${SKT_LOG}）----" >&2
    tail -10 "$SKT_LOG" >&2
  fi
  echo "--------------------------------" >&2
  KEEP=1   # 失败必留全日志（含 echod 现场——smoke T-c 对抗审查教训同源）
  exit 1
}

# 心跳提取：非锚定（SOAK 行嵌 ABAB 巨行中段是常态，见头注 3）。-a = 串口
# 日志混入任一无效 UTF-8/NUL 字节即触发 BSD grep 二进制模式全文件静默零收割
# （smoke.sh 串口日志断言一律 grep -a 同款先例，对抗审查 finding 8）。
beats_raw() {
  grep -aoE 'SOAK t=[0-9]+ sw=[0-9]+ sw0=[0-9]+ sw1=[0-9]+ park0=[0-9]+ park1=[0-9]+ pages=[0-9]+ lost=[0-9]+' "$LOG" 2>/dev/null || true
}

# 六断言（输入行 "t sw sw0 sw1 park0 park1 pages lost"）：破即打印诊断行退 1；
# 全过末行打印摘要。awk 陷阱实录：action 里裸 `exit` 退出码恒 0——`|| fail`
# 永不触发、违规只能等总帽超时才 FAIL（诊断误导）。必须 `exit 1` + END 里
# `if (bad) exit 1` 双保险（END 会照跑，别让它的正常返回覆盖挂起的 1）。
validate_beats() {
  beats_raw | sed -E 's/^SOAK t=//; s/ sw=/ /; s/ sw0=/ /; s/ sw1=/ /; s/ park0=/ /; s/ park1=/ /; s/ pages=/ /; s/ lost=/ /' | awk '
    {
      t=$1; sw=$2; s0=$3; s1=$4; q0=$5; q1=$6; pg=$7; lo=$8
      if (NR == 1) {
        if (t != 1) { printf "首拍 t=%d != 1\n", t; bad=1; exit 1 }
        pmin = pg; pmax = pg
      } else {
        if (t != pt + 1) { printf "t=%d 非严格 +1（前 t=%d）——丢拍/静默死\n", t, pt; bad=1; exit 1 }
        if (sw <= psw) { printf "t=%d sw=%d 非严格递增（前 sw=%d）——调度冻死\n", t, sw, psw; bad=1; exit 1 }
        if (s0 <= ps0) { printf "t=%d sw0=%d 非严格递增（前 sw0=%d）——CPU0 per-CPU 口径冻死\n", t, s0, ps0; bad=1; exit 1 }
      }
      if (s1 != 0 || q1 != 0) { printf "t=%d 副核统计非零（sw1=%d park1=%d）——单核 boot 恒零自证破\n", t, s1, q1; bad=1; exit 1 }
      # pages：首 5 拍结算窗豁免（boot 尾 bsh 自验族瞬态——S-5d A 跑实测
      # FAT n=5000 释放波 −773 页击穿首拍基线窗）；第 6 拍起稳态窗执法。
      if (NR == 6) pbase = pg
      if (NR > 5 && (pg < pbase || pg > pbase + 64)) { printf "t=%d pages=%d 越窗 [稳态基线 %d, %d]——泄漏/回收异常\n", t, pg, pbase, pbase + 64; bad=1; exit 1 }
      if (lo != 0) { printf "t=%d lost=%d 非 0——console 环丢字\n", t, lo; bad=1; exit 1 }
      if (pg < pmin) pmin = pg
      if (pg > pmax) pmax = pg
      pt=t; psw=sw; ps0=s0; n=NR
    }
    # 摘要 pages= 打观测区间 [min,max]（非两次基线——拍间波动被抹平会误导
    # 事后读摘要的排障/回归对比，对抗审查 finding 5；min 含首 5 拍结算豁免
    # 窗（自验族瞬态下穿不算 FAIL、观测保留），max 反映真实高水位）。
    END { if (bad) exit 1; printf "n=%d last_t=%d last_sw=%d last_sw0=%d pages=%d..%d", n, pt, psw, ps0, pmin, pmax }'
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

# —— A3-3 sktclient：bsh skt_selftest accept 对端（smoke.sh 同款懒启动 =
# guest `EVS ok` 后才起——QEMU 前起则 18083 SYN 重试流在 boot 全程与 18082
# 帧同 RX 环排队拖迟 EVS 握手；watcher 子壳 exec 成 python，SKT_PID 直杀
# 两形态皆中）。
SKT_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-sktclient.XXXXXX")
( until grep -q 'EVS ok' "$LOG" 2>/dev/null; do sleep 2; done
  exec python3 tools/evsclient.py 18083 SKT ) > "$SKT_LOG" 2>&1 &
SKT_PID=$!

# —— QEMU（run-rootfs 全量形态 + soak 门控键；stdin /dev/null）——
echo "soak: QEMU 启动（$MINUTES 拍；首拍帽 ${FIRST_BEAT_CAP}s / 单拍帽 ${PER_BEAT_CAP}s / 总帽 ${TOTAL_CAP}s）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio \
  -fw_cfg name=opt/berry/soak,string=1 \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.soak.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082,hostfwd=tcp::18083-:18083 -device virtio-net-device,netdev=n0 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# —— 阶段 1：boot 覆盖十标记 + 门控自证（常驻服务主体在席守卫，头注 2）——
# DNS/TCP/TCPS 三标记是 vnet 转常驻服务链的活性终点：echod 中途死/TCP 自验
# trap 时整族缺席而 ECO/VIRTIO/FAT/NET 照齐——四标记守卫罩不住自己举例的
# 失败链（对抗审查 finding 3/6）；'SOAK gate on' = 内核门控块内打印（行在
# ⟺ soakmon 已 spawn），键名漂移在此 loud 而非等首拍帽超时。A2-3 起
# + EVS ok n=2（bsh 侧 netd 会话消费链终点——client 缺席/对拍失配在此
# loud，而非等 bsh 120s accept-wait bail 后交互链静默断）。A3-3 起
# + SKT ok n=（sktclient 对拍链终点——缺席即永缺且 fat 族整族断，S-5d
# 静默断链实录后入守卫）。
t0=$(date +%s)
COVER=("SOAK gate on" "ECO ok" "VIRTIO ok" "SKT ok n=" "FAT ok n=" "NET ok ping=" "DNS ok addr=" "TCP ok echo=" "TCPS ok echo=" "EVS ok n=2")
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 提前退出（boot 期崩溃？）"
  missing=""
  for m in "${COVER[@]}"; do
    grep -aqF "$m" "$LOG" || missing="${missing:+$missing }$m"
  done
  [ -z "$missing" ] && break
  [ $(( $(date +%s) - t0 )) -gt "$DEADLINE_BOOT" ] \
    && fail "boot 覆盖标记 ${DEADLINE_BOOT}s 未到齐，缺：${missing}（常驻服务族缺席/门控键漂移 = 覆盖面缩水或 soakmon 未 spawn，勿空心绿跑）"
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

# A3-3：收 sktclient 对拍结果（guest SKT ok 已在覆盖面到齐——30s 帽只盖
# 透传余量，smoke.sh 同款）。
tc=$(date +%s)
while ! grep -q 'SKT client ok n=2' "$SKT_LOG" 2>/dev/null; do
  kill -0 "$SKT_PID" 2>/dev/null \
    || fail "sktclient 提前退出：$(tail -5 "$SKT_LOG" | tr '\n' ' ')"
  [ $(( $(date +%s) - tc )) -gt 30 ] \
    && fail "SKT client 30s 未对拍完成（guest SKT ok 已到——查 SKT_LOG 重试诊断）"
  sleep 1
done
kill "$SKT_PID" 2>/dev/null
wait "$SKT_PID" 2>/dev/null
rm -f "$SKT_LOG"
SKT_PID=""
SKT_LOG=""
echo "soak: SKT client 对拍完成（fd 皮 server 段两连接 64B/24B 逐字节）"

# —— 阶段 2：心跳六断言轮询（首拍潜伏 5 周期 ≈300-600s wall 是内核侧设计，非挂死）——
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
    summary=$(validate_beats) || fail "六断言破：$summary"
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
echo "soak: PASS ${MINUTES}/${MINUTES} 拍六断言全绿（${summary}；pages 稳态帽 +64，首 5 拍结算豁免——pmin..pmax 含豁免窗观测）"
echo "soak: 全程 ${elapsed}s（总帽 ${TOTAL_CAP}s）"
