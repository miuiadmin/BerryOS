#!/usr/bin/env bash
# BerryOS S-5d soak-smp 双核满载长跑通道（docs/SMP-S5实施设计草案.md §2 D1/D2/D4
# + §4「soak-smp ≥30min」）——`just soak-smp <分钟数>` 的实现（30 默认）。
#
# 与单核 tools/soak.sh 的差异面（其余全复刻）：
#   1. `-smp 2` + 双门控键：`opt/berry/soak`（soakmon 心跳 v2）+
#      `opt/berry/soakload`（S-5d D2 满载载荷 = 内核门控 EL0 churn 舰队：
#      root 家核 + 2 写者 spread→CPU1 互抢轮转——sw1 严格递增的物理来源；
#      写者恒 Runnable → CPU1 不 idle → park1 停增 = 满载谓词）。载荷零打印
#      （D2：panic SMP 安全面）；`SOAKLOAD fleet on n=3` = master 在舰队真
#      spawn 后的自证行（boot 覆盖标记）。
#   2. 心跳 v2 八字段断言（头注 3 断言带差异——单核 soak.sh 的恒零自证在此
#      换成活性/满载面）：
#        ① t 严格 +1 且从 1 起（丢拍 = 静默死）；
#        ② sw 严格递增（全局调度活性——单核 soak 同款）；
#        ③ sw0 严格递增（soakmon prio 8 每拍必切自身核——CPU0 活性）；
#        ④ sw1 双臂：严格递增（序列完整/对核活着）∧ **Δsw1 ≥ 320/拍（满载
#           谓词主判）**——2 写者互抢 tick 轮转 TCG 半率 ≈560 切换/拍；S-5d
#           B 探针实测死态残余 ≈195/拍（写者退场后 churn 线程迁入空核维持
#           秒级切换、park 亦停增——冻结断言与 park 帽双哑弹），增量塌缩 =
#           唯一可辨死签名；320 = 两态几何中点，双侧 ≥1.6x 裕量，R3 打
#           Δsw1min 观测贴帽即复审；
#        ⑤ park0/park1 ∈ [首拍值, 首拍值+256]（越窗守卫、非充分谓词：满载
#           核 park 停增，死态 park 亦停增〔B 探针实录〕——帽只抓 idle 刷穿
#           这另一族死状；满载主判在 ④）；
#        ⑥ pages 稳态窗 ∈ [第 6 拍值, +64]（S-5b 带宽口径原样）——首 5 拍
#           结算窗豁免：boot 尾 bsh 自验族瞬态（S-5d A 跑实测 FAT n=5000
#           释放波 −773 页横跨首拍基线、FATW/df/dir/cd 小波尾）在结算窗内
#           非泄漏证据；稳态基线 = 第 6 拍（≈guest t=360s，自验族收尾后）；
#        ⑦ lost 恒 0（console 环健康）。
#   3. 帽差异（满载 TCG 减速裕量）：单拍帽 240s（单核 soak 实测 ≈60s wall/
#      拍的 4x；双核三载荷 + 常驻族的 TCG 拉伸头寸）。
#   4. R3 定标行（PASS 尾打观测面非断言面：boot/首拍/均拍 wall + park Δ
#      观测 vs 帽 + pages 区间——smoke-smp R3 三数同款口径；观测贴近帽
#      70% 即按 §6 R3 预授权升帽 ×1.5 一次，再超 = R1 审计而非再升）。
#
# 注意：echod 18080 / hostfwd 18081-18083（A3-3 SKT 18083 专线在内）与
# soak/smoke/run-rootfs 同端口面
# ——**勿与任何同端口通道并行跑**（单核 soak.sh 亦然）。盘名 disk.soaksmp.img
# 专属（对抗审查 finding 10 同款零交集纪律）。30min 实跑纪律：后台执行须
# 显式 timeout ≥7200000ms（默认帽 1800000ms 恰 30min 必超——S-5 草案 §3 明文）。
# 调试：SOAK_KEEP_LOG=1 保留串口日志不删（成功也留；FAIL 恒留）。
set -u

# 内核路径解析收敛在 paths.sh（单一事实源，justfile/CI 同源；含非交互 PATH 修补）。
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR/.."
source "$SCRIPT_DIR/paths.sh"
KERNEL=$(berryos_kernel debug)

MINUTES=${1:-30}
case "$MINUTES" in
  ''|*[!0-9]*) echo "soak-smp: FAIL: 分钟数须为正整数，得到：$MINUTES" >&2; exit 1 ;;
esac
[ "$MINUTES" -ge 2 ] || { echo "soak-smp: FAIL: 分钟数须 ≥2（1 拍序列平凡无鉴别力——活性/满载断言须拍间差分）" >&2; exit 1; }

QEMU=${QEMU:-qemu-system-aarch64}
command -v "$QEMU" >/dev/null 2>&1 || { echo "soak-smp: FAIL: 找不到 $QEMU" >&2; exit 1; }
[ -f "$KERNEL" ] || { echo "soak-smp: FAIL: 内核未构建（先 just build）：$KERNEL" >&2; exit 1; }
[ -f rootfs.cpio ] || { echo "soak-smp: FAIL: 缺 rootfs.cpio（build-user 应产出）" >&2; exit 1; }

DEADLINE_BOOT=600   # 秒：boot 覆盖十二标记 + 门控自证行到齐。600 = 防挂死兜底
                    # 非性能门：FAT ok 基线 ≈200-206s（smoke.sh B4 归因实测）对
                    # 300s 余量 <1.5x，双核满载 TCG + 宿主争用拉伸即翻（S-5d
                    # 三场红定谳后升 600 = 基线 3x 裕量；再超 = R1 审计非再升）
PER_BEAT_CAP=240    # 秒：单拍 wall 帽（单核 soak 实测 ≈60s 的 4x——双核满载 TCG 拉伸头寸）
FIRST_BEAT_CAP=$(( DEADLINE_BOOT + 5 * PER_BEAT_CAP + 60 ))     # 首拍：boot + 潜伏五周期 + 余量
TOTAL_CAP=$(( DEADLINE_BOOT + (MINUTES + 5) * PER_BEAT_CAP + 120 ))  # 总帽（兜结构性挂死）
PARK_CAP=256        # 满载谓词帽（满载核 park 停增；载荷死 ≈10Hz 刷穿；R3 定标观测）
SW1_FLOOR=320       # Δsw1/拍地板（满载谓词主判：净态写者互抢 ≈560/拍 vs 死态
                    # 残余 ≈195/拍〔B 探针实测〕；320 = 几何中点双侧 ≥1.6x）
POLL=2

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-soaksmp.XXXXXX")
QEMU_PID=""
ECHO_PID=""
ECHO_LOG=""
EVS_PID=""
EVS_LOG=""

# A3-3：SKT socket fd 皮宿主对拍 client——bsh skt_selftest 的 accept 对端
# （hostfwd 18083 专线）。**缺席则 bsh 卡 SKT 无帽 accept → EPL/NETS/MMP/
# CLK/SIG/fat 族整族永不执行 → FAT ok 永缺**（S-5d 三场红实录定谳：9ab8a68
# A3-3 给 smoke.sh 加了 sktclient 而 soak 通道未跟进，soak.sh 停在 501d15c
# 旧对端集——本脚本首版照抄 soak.sh 一并继承缺口）。
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
  if [ -n "$EVS_PID" ] && kill -0 "$EVS_PID" 2>/dev/null; then
    kill "$EVS_PID" 2>/dev/null
    wait "$EVS_PID" 2>/dev/null
  fi
  # A3-3：sktclient 兜底收（正常流 boot 尾已收清——这里只兜中断在途实例）。
  if [ -n "$SKT_PID" ] && kill -0 "$SKT_PID" 2>/dev/null; then
    kill "$SKT_PID" 2>/dev/null
    wait "$SKT_PID" 2>/dev/null
  fi
  if [ "$KEEP" -eq 1 ]; then
    echo "soak-smp: 串口日志保留：$LOG" >&2
    [ -n "$ECHO_LOG" ] && echo "soak-smp: echod 日志保留：$ECHO_LOG" >&2
    [ -n "$EVS_LOG" ] && echo "soak-smp: evsclient 日志保留：$EVS_LOG" >&2
    [ -n "$SKT_LOG" ] && echo "soak-smp: sktclient 日志保留：$SKT_LOG" >&2
  else
    rm -f "$LOG"
    [ -n "$ECHO_LOG" ] && rm -f "$ECHO_LOG"
    [ -n "$EVS_LOG" ] && rm -f "$EVS_LOG"
    [ -n "$SKT_LOG" ] && rm -f "$SKT_LOG"
  fi
}
trap cleanup EXIT

fail() {
  echo "soak-smp: FAIL: $1" >&2
  echo "---- 心跳表（全部已收拍）----" >&2
  grep -aoE 'SOAK t=[0-9]+ sw=[0-9]+ sw0=[0-9]+ sw1=[0-9]+ park0=[0-9]+ park1=[0-9]+ pages=[0-9]+ lost=[0-9]+' "$LOG" >&2 || true
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
  if [ -n "$SKT_LOG" ] && [ -s "$SKT_LOG" ]; then
    echo "---- sktclient 日志尾部（$SKT_LOG）----" >&2
    tail -10 "$SKT_LOG" >&2
  fi
  echo "--------------------------------" >&2
  KEEP=1   # 失败必留全日志（含 echod 现场——smoke T-c 对抗审查教训同源）
  exit 1
}

# 心跳提取：非锚定（SOAK 行嵌 ABAB 巨行中段是常态）+ -a（无效字节触发 BSD
# grep 二进制模式全文件静默零收割——smoke.sh 串口断言一律 grep -a 同款先例）。
beats_raw() {
  grep -aoE 'SOAK t=[0-9]+ sw=[0-9]+ sw0=[0-9]+ sw1=[0-9]+ park0=[0-9]+ park1=[0-9]+ pages=[0-9]+ lost=[0-9]+' "$LOG" 2>/dev/null || true
}

# 七断言（输入行 "t sw sw0 sw1 park0 park1 pages lost"；头注 2 逐条）。
# awk 陷阱实录（soak.sh 同款）：裸 `exit` 退出码恒 0——必须 `exit 1` +
# END `if (bad) exit 1` 双保险。返回行 = R3 定标观测串（park0/park1 Δ +
# pages 区间——PASS 行消费）。
validate_beats() {
  beats_raw | sed -E 's/^SOAK t=//; s/ sw=/ /; s/ sw0=/ /; s/ sw1=/ /; s/ park0=/ /; s/ park1=/ /; s/ pages=/ /; s/ lost=/ /' | awk '
    {
      t=$1; sw=$2; s0=$3; s1=$4; q0=$5; q1=$6; pg=$7; lo=$8
      if (NR == 1) {
        if (t != 1) { printf "首拍 t=%d != 1\n", t; bad=1; exit 1 }
        pmin = pg; pmax = pg; pq0 = q0; pq1 = q1
      } else {
        if (t != pt + 1) { printf "t=%d 非严格 +1（前 t=%d）——丢拍/静默死\n", t, pt; bad=1; exit 1 }
        if (sw <= psw) { printf "t=%d sw=%d 非严格递增（前 sw=%d）——调度冻死\n", t, sw, psw; bad=1; exit 1 }
        if (s0 <= ps0) { printf "t=%d sw0=%d 非严格递增（前 sw0=%d）——CPU0 冻死/心跳断\n", t, s0, ps0; bad=1; exit 1 }
        if (s1 <= ps1) { printf "t=%d sw1=%d 非严格递增（前 sw1=%d）——CPU1 冻死\n", t, s1, ps1; bad=1; exit 1 }
        else if (s1 - ps1 < '"$SW1_FLOOR"') { printf "t=%d Δsw1=%d 塌缩 < %d/拍（前 sw1=%d）——CPU1 满载谓词破（载荷死/写者退场：净态互抢 ≈560/拍 vs 死态残余 ≈195/拍，B 探针实测）\n", t, s1 - ps1, '"$SW1_FLOOR"', ps1; bad=1; exit 1 }
        if (mind1 == "" || s1 - ps1 < mind1) mind1 = s1 - ps1
      }
      if (q0 < pq0 || q0 > pq0 + '"$PARK_CAP"') { printf "t=%d park0=%d 越窗 [基线 %d, %d]——CPU0 非满载（载荷死/常驻阻塞面）\n", t, q0, pq0, pq0 + '"$PARK_CAP"'; bad=1; exit 1 }
      if (q1 < pq1 || q1 > pq1 + '"$PARK_CAP"') { printf "t=%d park1=%d 越窗 [基线 %d, %d]——CPU1 非满载（写者死→idle park 环刷穿帽）\n", t, q1, pq1, pq1 + '"$PARK_CAP"'; bad=1; exit 1 }
      # pages：首 5 拍结算窗豁免（boot 尾 bsh 自验族瞬态——S-5d A 跑实测
      # FAT n=5000 释放波 −773 页击穿首拍基线窗）；第 6 拍起稳态窗执法。
      if (NR == 6) pbase = pg
      if (NR > 5 && (pg < pbase || pg > pbase + 64)) { printf "t=%d pages=%d 越窗 [稳态基线 %d, %d]——泄漏/回收异常\n", t, pg, pbase, pbase + 64; bad=1; exit 1 }
      if (lo != 0) { printf "t=%d lost=%d 非 0——console 环丢字\n", t, lo; bad=1; exit 1 }
      if (pg < pmin) pmin = pg
      if (pg > pmax) pmax = pg
      if (q0 > mq0) mq0 = q0
      if (q1 > mq1) mq1 = q1
      pt=t; psw=sw; ps0=s0; ps1=s1; n=NR
    }
    END { if (bad) exit 1; printf "n=%d last_t=%d last_sw=%d last_sw0=%d last_sw1=%d parkD0=%d parkD1=%d Δsw1min=%d pages=%d..%d", n, pt, psw, ps0, ps1, mq0-pq0, mq1-pq1, mind1+0, pmin, pmax }'
}

# —— 造盘（run-rootfs/soak 同款确定性 FAT12 卷 + LBA0 印章；每次重造 = 跨跑
# 零残留）。盘名 disk.soaksmp.img 专属：与 soak 的 disk.soak.img / smoke 的
# disk.img 三方零交集（并行窗口盘互毁面归零，对抗审查 finding 10）。
python3 tools/mkfat.py disk.soaksmp.img || { echo "soak-smp: FAIL: mkfat 造盘失败" >&2; exit 1; }
printf 'BERRYOSVBLK!' | dd of=disk.soaksmp.img conv=notrunc status=none

# —— echod：vnet TCP 自验对端（soak.sh 同款必抄——缺席则 DNS/TCP 常驻族
# 整族缺席 = 覆盖面静默缩水）。
ECHO_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-echod.XXXXXX")
python3 tools/echod.py > "$ECHO_LOG" 2>&1 &
ECHO_PID=$!
te=$(date +%s)
while ! grep -q 'listening on' "$ECHO_LOG" 2>/dev/null; do
  kill -0 "$ECHO_PID" 2>/dev/null \
    || fail "echod 启动失败（18080 被占？勿与 soak/smoke/run-rootfs 并行）：$(cat "$ECHO_LOG")"
  [ $(( $(date +%s) - te )) -gt 5 ] && fail "echod 5s 未 listening"
  sleep 0.2
done

# —— A2-3 evsclient：bsh EVS echo server 对拍对端（soak.sh 同款必抄）。
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

# —— QEMU（soak 全量形态 + -smp 2 + 双门控键；stdin /dev/null）——
echo "soak-smp: QEMU 启动（-smp 2 + soak/soakload 双门控；$MINUTES 拍；首拍帽 ${FIRST_BEAT_CAP}s / 单拍帽 ${PER_BEAT_CAP}s / park 帽 +${PARK_CAP} / 总帽 ${TOTAL_CAP}s）"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio \
  -fw_cfg name=opt/berry/soak,string=1 \
  -fw_cfg name=opt/berry/soakload,string=1 \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.soaksmp.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082,hostfwd=tcp::18083-:18083 -device virtio-net-device,netdev=n0 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# —— 阶段 1：boot 覆盖十二标记（soak 十标记 + SMP ok + SOAKLOAD fleet on
# 满载载荷自证——行在 ⟺ 舰队已 spawn 且 root churn 已在打；SKT ok n= =
# A3-3 对拍链终点，sktclient 缺席即永缺——S-5d 静默断链实录后入守卫）——
t0=$(date +%s)
COVER=("SMP ok ncores=2" "SOAK gate on" "SOAKLOAD fleet on n=3" "ECO ok" "VIRTIO ok" "SKT ok n=" "FAT ok n=" "NET ok ping=" "DNS ok addr=" "TCP ok echo=" "TCPS ok echo=" "EVS ok n=2")
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 提前退出（boot 期崩溃？）"
  missing=""
  for m in "${COVER[@]}"; do
    grep -aqF "$m" "$LOG" || missing="${missing:+$missing }$m"
  done
  [ -z "$missing" ] && break
  [ $(( $(date +%s) - t0 )) -gt "$DEADLINE_BOOT" ] \
    && fail "boot 覆盖标记 ${DEADLINE_BOOT}s 未到齐，缺：$missing（soakload 门控键漂移/载荷 master 未跑 = 满载断言必假绿，勿空心跑）"
  sleep 2
done
BOOT_WALL=$(( $(date +%s) - t0 ))
echo "soak-smp: boot 覆盖标记 ${#COVER[@]}/${#COVER[@]} 到齐（含 SOAKLOAD fleet on 满载载荷自证；双核 + 常驻族 + churn 舰队在席）"

# —— A2-3：收 evsclient 对拍结果（soak.sh 同款；30s 帽只盖透传余量）——
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
echo "soak-smp: EVS client 对拍完成（两连接 64B/24B 逐字节）"

# —— A3-3：收 sktclient 对拍结果（guest SKT ok 已在覆盖面到齐——30s 帽只盖
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
echo "soak-smp: SKT client 对拍完成（fd 皮 server 段两连接 64B/24B 逐字节）"

# —— 阶段 2：心跳七断言轮询（首拍潜伏 5 周期 ≈300-1200s wall 是内核侧
# 设计，非挂死——soak.sh 同款）——
tq=$(date +%s)
first_beat_wall=""
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
    summary=$(validate_beats) || fail "七断言破：$summary"
    if [ -z "$first_beat_wall" ]; then
      first_beat_wall=$(( now - tq ))
    fi
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
mean_beat=$(( (elapsed - first_beat_wall) / (MINUTES - 1) ))
echo "soak-smp: PASS ${MINUTES}/${MINUTES} 拍七断言全绿（$summary）"
# R3 定标行（观测面非断言面；任一数 > 帽 70% 即预授权 ×1.5 一次——smoke-smp
# R3 三数同款 §6 纪律）。
echo "soak-smp: R3 定标：boot=${BOOT_WALL}s（帽 ${DEADLINE_BOOT}s）/ 首拍=${first_beat_wall}s（帽 ${FIRST_BEAT_CAP}s）/ 均拍=${mean_beat}s（帽 ${PER_BEAT_CAP}s）/$summary（park 帽 +${PARK_CAP} / Δsw1 地板 ${SW1_FLOOR}/拍 / pages 稳态帽 +64，首 5 拍结算豁免——pmin..pmax 含豁免窗观测）"
echo "soak-smp: 全程 ${elapsed}s（总帽 ${TOTAL_CAP}s）"
