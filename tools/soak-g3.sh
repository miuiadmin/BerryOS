#!/usr/bin/env bash
# G3-5 激进配置常驻 soak（草案 §5：PREEMPT_RT 形激进镜像长跑模板）。
#
# 激进镜像定谳（实施刀侦察 2026-11-05）：全树 cargo feature 门唯 lockdep
# （kernel aspace/vmar/main + sched:532-582）；P-A 抢占点 / P-B preempt-on-wake
# 已随 G3-1/G3-2 落 default 面，细粒度锁 = 基线设计——**激进镜像 delta =
# lockdep feature 单项**（preempt-full/细锁非 feature 化，无可开关面）。
# 草案 §5「preempt-full + 细锁全开 + lockdep」三件中前两件已转正语义，
# 定谳记档：证据档 tools/evidence-g3-5.md + 草案 §8 G3-5 段。
#
# 模板 = soak-smp.sh 骨架全保（八断言/对拍三件套/盘名专属/端口纪律）+
# 两新增断言面：
#   ① lockdep 面（激进镜像自带）：boot COVER 加 `LOCKDEP selftest ok` +
#      终验三计数（ok 恰 1 / selftest FAIL 0 / VIOLATION 0——smoke-lockdep
#      同款执法面搬入长跑：VIOLATION fail-loud 停机由心跳帽兜，selftest
#      FAIL 非停机必须末尾计数执法）。
#   ② 尾延迟帽（§4 基线 → 候选① C 形常驻化 2026-11-05）：fw_cfg 三键
#      齐发（soak + soakload + lat）——lat 载荷常驻再触发（boot 一次
#      spawn 采样毕回睡不退；soakmon 每拍行后触发两路再采 ×32 样本/路，
#      evidence-g3-5recon.md 三形裁决 C 最优）→ BENCHLAT 每路恰 拍数+1
#      行（boot 周期 + 每拍一周期）+ LATIPC/LATSGI ok 计数同 +
#      max-over-cycles 帽（n=32 时 p99 秩 = max 秩——单 max 帽全覆盖
#      全部周期行）。boot 周期 lane=sgi 满载受扰采样（spawn 序在 soakload
#      master 后）；稳态周期行 = 全时全负载受扰时延（被测对象本体）。
#
# 帽值首跑标定（S-5d SW1_FLOOR 先例）：SOAKG3_CALIBRATE=1 跑一次 →
# 末尾定标行给全周期观测 max → 观测 ×3.0（B-0 界同款裕量）向上取整烘焙
# 默认值，附推导注。**C 形注（2026-11-05）**：现行默认帽 = 一次性形
# boot 窗标定（bank 跑 ipc 6063/sgi 20635000 ×3.0）——boot 数字不可外推
# 稳态，常驻形首跑须 CALIBRATE 重标定后烘焙。kill criteria（侦察档后继
# 刀形 4）：CALIBRATE 观测稳态 max 超银行跑帽 ×3.0 量级 = 候选① 降级线
#（候选② lockdep N 跑顶上），定标行顾问提示。他机复跑可 LAT_CAP_IPC /
# LAT_CAP_SGI 环境变量覆盖（TCG 载荷 §A118 跨机不可比——B-2 纪律）。
#
# 用法：cargo build --features lockdep && \
#       bash tools/soak-g3.sh "$(bash tools/paths.sh kernel)" [分钟]
#   （内核路径参数化单一事实源——smoke-lockdep 先例；无 just 配方：
#   justfile 为兄弟会话 WIP 占用时 script 直调同效。不进 CI——小时级
#   与 soak 家族同口径。）
#
# 端口纪律：echod 18080 + hostfwd 18081-18083——勿与 smoke / run-rootfs /
# soak / soak-smp 并行（整族端口面互斥）。盘名 disk.soakg3.img 专属
# （与 soak/smoke/soak-smp 四方零交集）。长跑后台须显式 timeout
# ≥7200000ms（30min 默认帽连坐杀先例）。SOAKG3_KEEP_LOG=1 成功亦留档。
set -u

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
cd "$SCRIPT_DIR/.."
source "$SCRIPT_DIR/paths.sh"

KERNEL="${1:-}"
MINUTES="${2:-60}"
if [ -z "$KERNEL" ] || [ ! -f "$KERNEL" ]; then
  echo "soak-g3: 用法：$0 <内核 ELF> [分钟]（feature 面——cargo build --features lockdep 出）" >&2
  exit 2
fi
# 镜像面自证（smoke-lockdep 同款）：default 面二进制 marker 串编译消除——
# 前置直拒，不烧 60min 银行跑才在 lockdep 断言面红。
if ! strings "$KERNEL" | command grep -q 'LOCKDEP selftest ok'; then
  echo "soak-g3: 镜像非 lockdep feature 面（marker 串零命中）——先 cargo build --features lockdep" >&2
  exit 2
fi

case "$MINUTES" in
  ''|*[!0-9]*) echo "soak-g3: FAIL: 分钟数须为正整数，得到：$MINUTES" >&2; exit 1 ;;
esac
[ "$MINUTES" -ge 2 ] || { echo "soak-g3: FAIL: 分钟数须 ≥2（1 拍序列平凡无鉴别力——活性/满载断言须拍间差分）" >&2; exit 1; }

QEMU=${QEMU:-qemu-system-aarch64}
command -v "$QEMU" >/dev/null 2>&1 || { echo "soak-g3: FAIL: 找不到 $QEMU" >&2; exit 1; }
[ -f rootfs.cpio ] || { echo "soak-g3: FAIL: 缺 rootfs.cpio（build-user 应产出）" >&2; exit 1; }

DEADLINE_BOOT=600   # 秒：boot 覆盖标记到齐（soak-smp 同值：FAT ok 基线
                    # ≈200-206s 的 3x 裕量；lockdep face 构建慢径不改变
                    # guest 侧 boot 时序面——自证行全在 boot 窗早期）
PER_BEAT_CAP=240    # 秒：单拍 wall 帽（soak-smp 同值：单核实测 ≈60s 的 4x）
FIRST_BEAT_CAP=$(( 5 * PER_BEAT_CAP + 60 ))    # 首拍：潜伏五周期 + 余量
TOTAL_CAP=$(( (MINUTES + 5) * PER_BEAT_CAP + 120 ))  # 总帽（兜结构性挂死）
PARK_CAP=256        # 满载谓词帽（soak-smp 同值）
SW1_FLOOR=320       # Δsw1/拍地板（soak-smp 同值：双写者在席 ≥57% 口径）
POLL=2

# 尾延迟帽（§4 基线进断言面）。默认值 = 标定首跑观测 ×3.0（B-0 界同款
# 裕量，S-5d SW1_FLOOR 先例）：lockdep 面 + soakload boot 窗满载受扰观测
# ipc max=6063 / sgi max=20635000（对 §4 净态基线 5000/15234750 增量
# ×1.21/×1.35 = lockdep 开销 + 满载受扰的合成分解面，详 evidence-g3-5.md）。
# 稳态帽（2026-11-05 C 形 12 拍 CALIBRATE PASS 定标：稳态 max-over-cycles
# ipc=8187 / sgi=20728250，×3.0 烘入——稳态 ≈ boot 同量级，soakload 干扰
# 未抬尾；boot 窗数字 18189/61905000 降为银行跑参照线 BANK_CAP_*）。
# 环境变量可覆盖（TCG 载荷 §A118 跨机不可比——B-2 纪律）。
LAT_CAP_IPC=${LAT_CAP_IPC:-24561}
LAT_CAP_SGI=${LAT_CAP_SGI:-62184750}
# 银行跑帽（2026-11-05 bank PASS 60/60 拍，一次性形 boot 窗标定）——
# kill criteria 参照线（CALIBRATE 观测稳态 max 超其 ×3.0 = 候选① 降级；
# f4 F4 修起执法分支同样机器评估），不随 LAT_CAP_* 环境变量覆盖漂移。
# 出处注记（f4 F16）：定标数字溯源 pilot 首跑 stdout 回执
# （out/bench-receipts/soakg3-pilot-stdout-20261105.log——该卷 PASS 行
# LAT_OBS 仍默认值，出自未提交早期修订；数字经 bank 跑独立复核在案沿用）。
BANK_CAP_IPC=18189
BANK_CAP_SGI=61905000
CALIBRATE=0
[ -n "${SOAKG3_CALIBRATE:-}" ] && CALIBRATE=1

LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-soakg3.XXXXXX")
QEMU_PID=""
ECHO_PID=""
ECHO_LOG=""
EVS_PID=""
EVS_LOG=""
SKT_PID=""
SKT_LOG=""

KEEP=0
[ -n "${SOAKG3_KEEP_LOG:-}" ] && KEEP=1

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
  if [ -n "$SKT_PID" ] && kill -0 "$SKT_PID" 2>/dev/null; then
    kill "$SKT_PID" 2>/dev/null
    wait "$SKT_PID" 2>/dev/null
  fi
  if [ "$KEEP" -eq 1 ]; then
    echo "soak-g3: 串口日志保留：$LOG" >&2
    [ -n "$ECHO_LOG" ] && echo "soak-g3: echod 日志保留：$ECHO_LOG" >&2
    [ -n "$EVS_LOG" ] && echo "soak-g3: evsclient 日志保留：$EVS_LOG" >&2
    [ -n "$SKT_LOG" ] && echo "soak-g3: sktclient 日志保留：$SKT_LOG" >&2
  else
    rm -f "$LOG"
    [ -n "$ECHO_LOG" ] && rm -f "$ECHO_LOG"
    [ -n "$EVS_LOG" ] && rm -f "$EVS_LOG"
    [ -n "$SKT_LOG" ] && rm -f "$SKT_LOG"
  fi
}
trap cleanup EXIT

fail() {
  echo "soak-g3: FAIL: $1" >&2
  echo "---- 心跳表（全部已收拍）----" >&2
  grep -aoE 'SOAK t=[0-9]+ sw=[0-9]+ sw0=[0-9]+ sw1=[0-9]+ park0=[0-9]+ park1=[0-9]+ pages=[0-9]+ lost=[0-9]+ churn=[0-9]+' "$LOG" >&2 || true
  echo "---- lockdep 计数 ----" >&2
  echo "selftest ok=$(grep -ac 'LOCKDEP selftest ok' "$LOG" 2>/dev/null || true) selftest FAIL=$(grep -ac 'LOCKDEP selftest FAIL' "$LOG" 2>/dev/null || true) VIOLATION=$(grep -ac 'LOCKDEP VIOLATION' "$LOG" 2>/dev/null || true)" >&2
  echo "---- BENCHLAT 收割（全部）----" >&2
  grep -aoE 'BENCHLAT,lane=(ipc|sgi),n=32,min=[0-9]+,p50=[0-9]+,p90=[0-9]+,p99=[0-9]+,max=[0-9]+' "$LOG" >&2 || true
  echo "---- 串口日志尾部（全文保留：${LOG}）----" >&2
  tail -40 "$LOG" >&2
  for f in "$ECHO_LOG" "$EVS_LOG" "$SKT_LOG"; do
    if [ -n "$f" ] && [ -s "$f" ]; then
      echo "---- $(basename "$f") 尾部（${f}）----" >&2
      tail -10 "$f" >&2
    fi
  done
  echo "--------------------------------" >&2
  KEEP=1   # 失败必留全日志
  exit 1
}

# 心跳提取：非锚定 + -a（无效字节触发 BSD grep 二进制模式静默零收割——
# smoke.sh 串口断言一律 grep -a 同款先例）。
beats_raw() {
  grep -aoE 'SOAK t=[0-9]+ sw=[0-9]+ sw0=[0-9]+ sw1=[0-9]+ park0=[0-9]+ park1=[0-9]+ pages=[0-9]+ lost=[0-9]+ churn=[0-9]+' "$LOG" 2>/dev/null || true
}

# 八断言（soak-smp 逐字同款——骨架复用合同：本脚本不改测量面语义）。
validate_beats() {
  beats_raw | sed -E 's/^SOAK t=//; s/ sw=/ /; s/ sw0=/ /; s/ sw1=/ /; s/ park0=/ /; s/ park1=/ /; s/ pages=/ /; s/ lost=/ /; s/ churn=/ /' | awk '
    {
      t=$1; sw=$2; s0=$3; s1=$4; q0=$5; q1=$6; pg=$7; lo=$8; ch=$9
      if (NR == 1) {
        if (t != 1) { printf "首拍 t=%d != 1\n", t; bad=1; exit 1 }
        pmin = pg; pmax = pg; pq0 = q0; pq1 = q1
      } else {
        if (t != pt + 1) { printf "t=%d 非严格 +1（前 t=%d）——丢拍/静默死\n", t, pt; bad=1; exit 1 }
        if (sw <= psw) { printf "t=%d sw=%d 非严格递增（前 sw=%d）——调度冻死\n", t, sw, psw; bad=1; exit 1 }
        if (s0 <= ps0) { printf "t=%d sw0=%d 非严格递增（前 sw0=%d）——CPU0 冻死/心跳断\n", t, s0, ps0; bad=1; exit 1 }
        if (s1 <= ps1) { printf "t=%d sw1=%d 非严格递增（前 sw1=%d）——CPU1 冻死\n", t, s1, ps1; bad=1; exit 1 }
        else if (s1 - ps1 < '"$SW1_FLOOR"') { printf "t=%d Δsw1=%d 塌缩 < %d/拍（前 sw1=%d）——载荷活性主判破（④口径=双写者在席 ≥57%）\n", t, s1 - ps1, '"$SW1_FLOOR"', ps1; bad=1; exit 1 }
        if (mind1 == "" || s1 - ps1 < mind1) mind1 = s1 - ps1
        if (ch <= pch) { printf "t=%d churn=%d 非严格递增（前 %d）——载荷业务进度冻死（S-5e M1 端到端面）\n", t, ch, pch; bad=1; exit 1 }
      }
      if (q0 < pq0 || q0 > pq0 + '"$PARK_CAP"') { printf "t=%d park0=%d 越窗 [基线 %d, %d]——CPU0 非满载（载荷死/常驻阻塞面）\n", t, q0, pq0, pq0 + '"$PARK_CAP"'; bad=1; exit 1 }
      if (q1 < pq1 || q1 > pq1 + '"$PARK_CAP"') { printf "t=%d park1=%d 越窗 [基线 %d, %d]——CPU1 非满载（写者死→idle park 环刷穿帽）\n", t, q1, pq1, pq1 + '"$PARK_CAP"'; bad=1; exit 1 }
      if (NR == 6) pbase = pg
      if (NR > 5 && (pg < pbase || pg > pbase + 64)) { printf "t=%d pages=%d 越窗 [稳态基线 %d, %d]——泄漏/回收异常\n", t, pg, pbase, pbase + 64; bad=1; exit 1 }
      if (lo != 0) { printf "t=%d lost=%d 非 0——console 环丢字\n", t, lo; bad=1; exit 1 }
      if (pg < pmin) pmin = pg
      if (pg > pmax) pmax = pg
      if (q0 > mq0) mq0 = q0
      if (q1 > mq1) mq1 = q1
      pt=t; psw=sw; ps0=s0; ps1=s1; pch=ch; n=NR
    }
    END { if (bad) exit 1; printf "n=%d last_t=%d last_sw=%d last_sw0=%d last_sw1=%d parkD0=%d parkD1=%d Δsw1min=%d pages=%d..%d churn=%d", n, pt, psw, ps0, ps1, mq0-pq0, mq1-pq1, mind1+0, pmin, pmax, pch+0 }'
}

# 尾延迟帽执法 + 定标观测（G3-5 新增面②；候选① C 形常驻升级 2026-11-05）：
# BENCHLAT 每路恰 beats+1 行（boot 周期 1 + 每拍后触发 1——周期行与拍行
# 一一对应；零行 = 载荷断链；少行 = 触发漏发/竞速超 far；多行 = 自由跑
# 〔清位破〕——恰 N+1 断言全形执法）。max-over-cycles 对帽（n=32 时 p99
# 秩 = max 秩，p99==max 恒成立——单 max 帽覆盖全部周期行含 boot 行）。
# CALIBRATE 模式跳帽只出观测（含 kill criteria 顾问行——稳态 max 超银行
# 跑帽 ×3.0 量级 = 候选① 降级线，evidence-g3-5recon.md 后继刀形 4）。
# 用法：lat_face <拍数>（末拍周期行结算等待在调用面完成）。
lat_face() {
  local beats="$1"
  local want=$(( beats + 1 ))
  local lines_ipc lines_sgi bad=0
  lines_ipc=$(grep -aoE 'BENCHLAT,lane=ipc,n=32,[^ ]+' "$LOG" 2>/dev/null || true)
  lines_sgi=$(grep -aoE 'BENCHLAT,lane=sgi,n=32,[^ ]+' "$LOG" 2>/dev/null || true)
  local n_ipc n_sgi
  n_ipc=$(printf '%s\n' "$lines_ipc" | command grep -c . || true)
  n_sgi=$(printf '%s\n' "$lines_sgi" | command grep -c . || true)
  [ "${n_ipc:-0}" -eq "$want" ] || { echo "lat 面：lane=ipc BENCHLAT 行数 ${n_ipc:-0} ≠ ${want}（boot+每拍一周期——断链/漏触发/自由跑形破）" >&2; bad=1; }
  [ "${n_sgi:-0}" -eq "$want" ] || { echo "lat 面：lane=sgi BENCHLAT 行数 ${n_sgi:-0} ≠ ${want}（boot+每拍一周期——断链/漏触发/自由跑形破）" >&2; bad=1; }
  # 全周期行取 max（单帽覆盖 boot + 稳态全周期）。
  local m_ipc m_sgi p99_ipc p99_sgi
  m_ipc=$(printf '%s\n' "$lines_ipc" | awk -F'max=' '{print $2}' | command grep -oE '^[0-9]+' | sort -n | tail -1 || echo "")
  m_sgi=$(printf '%s\n' "$lines_sgi" | awk -F'max=' '{print $2}' | command grep -oE '^[0-9]+' | sort -n | tail -1 || echo "")
  p99_ipc=$(printf '%s\n' "$lines_ipc" | awk -F'p99=' '{print $2}' | command grep -oE '^[0-9]+' | sort -n | tail -1 || echo "")
  p99_sgi=$(printf '%s\n' "$lines_sgi" | awk -F'p99=' '{print $2}' | command grep -oE '^[0-9]+' | sort -n | tail -1 || echo "")
  [ -n "$m_ipc" ] && [ -n "$m_sgi" ] || { echo "lat 面：BENCHLAT 字段解析失败（ipc max='${m_ipc}' sgi max='${m_sgi}'）" >&2; bad=1; }
  [ "$bad" -eq 1 ] && return 1
  if [ "$CALIBRATE" -eq 1 ]; then
    # 定标行（S-5d SW1_FLOOR 先例同形）：全周期观测 max ×3.0 向上取整烘焙。
    local kil=""
    [ "$m_ipc" -gt $(( BANK_CAP_IPC * 3 )) ] && kil="${kil} ipc"
    [ "$m_sgi" -gt $(( BANK_CAP_SGI * 3 )) ] && kil="${kil} sgi"
    if [ -n "$kil" ]; then
      echo "soak-g3: lat kill criteria 破线（${kil} 稳态 max 超银行跑帽 ×3.0）——候选① 降级，候选②（lockdep N 跑）顶上（evidence-g3-5recon.md 后继刀形 4）" >&2
    fi
    echo "soak-g3: lat 定标（${beats}+1 周期行）：ipc p99max=${p99_ipc} max=${m_ipc}（帽建议 ×3.0 → $(( m_ipc * 3 ))）/ sgi p99max=${p99_sgi} max=${m_sgi}（帽建议 ×3.0 → $(( m_sgi * 3 ))）——烘焙 LAT_CAP_IPC/LAT_CAP_SGI 后复跑执法"
    LAT_OBS="CALIBRATE 观测 ${beats}+1 周期（ipc p99max=${p99_ipc} max=${m_ipc} / sgi p99max=${p99_sgi} max=${m_sgi}——帽未执法）"
    return 0
  fi
  if [ "$m_ipc" -gt "$LAT_CAP_IPC" ]; then
    echo "lat 面：lane=ipc max-over-cycles=${m_ipc} > 帽 ${LAT_CAP_IPC}（稳态尾延迟回归——lockdep 开销/满载受扰超标）" >&2; bad=1
  fi
  if [ "$m_sgi" -gt "$LAT_CAP_SGI" ]; then
    echo "lat 面：lane=sgi max-over-cycles=${m_sgi} > 帽 ${LAT_CAP_SGI}（稳态尾延迟回归——lockdep 开销/满载受扰超标）" >&2; bad=1
  fi
  # kill criteria 时延肢执法（f4 F4 修，2026-11-05）：稳态 max 超银行跑帽
  # ×3.0 = 候选① 降级线——执法跑同样机器评估（原仅 CALIBRATE 顾问提示，
  # LAT_CAP_* 被环境变量抬高后降级线成盲区；pages 肢两模式均 fail-loud，
  # 此修补对称）。
  local kil=""
  [ "$m_ipc" -gt $(( BANK_CAP_IPC * 3 )) ] && kil="${kil}ipc "
  [ "$m_sgi" -gt $(( BANK_CAP_SGI * 3 )) ] && kil="${kil}sgi "
  if [ -n "$kil" ]; then
    echo "soak-g3: lat kill criteria 破线（${kil% } 稳态 max 超银行跑帽 ×3.0：ipc ${m_ipc} / sgi ${m_sgi} vs ${BANK_CAP_IPC}×3 / ${BANK_CAP_SGI}×3）——候选① 降级线触发" >&2
    bad=1
  fi
  LAT_OBS="ipc p99max=${p99_ipc} max=${m_ipc} ≤ ${LAT_CAP_IPC}（${beats}+1 周期恰形）/ sgi p99max=${p99_sgi} max=${m_sgi} ≤ ${LAT_CAP_SGI}（kill 线 ${BANK_CAP_IPC}×3/${BANK_CAP_SGI}×3 内）"
  [ "$bad" -eq 1 ] && return 1
  return 0
}

# —— 造盘（确定性 FAT12 卷 + LBA0 印章；每次重造 = 跨跑零残留）。盘名
# disk.soakg3.img 专属：与 soak/soak-smp/smoke 四方零交集（并行窗口盘
# 互毁面归零）。
python3 tools/mkfat.py disk.soakg3.img || { echo "soak-g3: FAIL: mkfat 造盘失败" >&2; exit 1; }
printf 'BERRYOSVBLK!' | dd of=disk.soakg3.img conv=notrunc status=none

# —— echod：vnet TCP 自验对端（soak 家族必抄——缺席则 DNS/TCP 常驻族
# 整族缺席 = 覆盖面静默缩水）。
ECHO_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-echod.XXXXXX")
python3 tools/echod.py > "$ECHO_LOG" 2>&1 &
ECHO_PID=$!
te=$(date +%s)
while ! command grep -q 'listening on' "$ECHO_LOG" 2>/dev/null; do
  kill -0 "$ECHO_PID" 2>/dev/null \
    || fail "echod 启动失败（18080 被占？勿与 soak/smoke/run-rootfs 并行）：$(cat "$ECHO_LOG")"
  [ $(( $(date +%s) - te )) -gt 5 ] && fail "echod 5s 未 listening"
  sleep 0.2
done

# —— evsclient：bsh EVS echo server 对拍对端（soak 家族必抄）。
EVS_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-evsclient.XXXXXX")
python3 tools/evsclient.py > "$EVS_LOG" 2>&1 &
EVS_PID=$!

# —— sktclient：bsh skt_selftest accept 对拍对端（懒启动 = guest `EVS ok`
# 后才起——QEMU 前起则 18083 SYN 重试流拖迟 EVS 握手）。
SKT_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-sktclient.XXXXXX")
( until command grep -q 'EVS ok' "$LOG" 2>/dev/null; do sleep 2; done
  exec python3 tools/evsclient.py 18083 SKT ) > "$SKT_LOG" 2>&1 &
SKT_PID=$!

# —— QEMU（soak-smp 全量形态 + 三门控键：soak 心跳 + soakload 满载舰队
# + lat 常驻再触发〔C 形——BENCHLAT 恰 拍数+1 周期行执法，§4 基线已
# 升格；旧「一次性采样」注已正（f4 F9）〕；stdin /dev/null）——
echo "soak-g3: QEMU 启动（-smp 2 + lockdep feature 镜像 + soak/soakload/lat 三门控；$MINUTES 拍；首拍帽 ${FIRST_BEAT_CAP}s / 单拍帽 ${PER_BEAT_CAP}s / park 帽 +${PARK_CAP} / 总帽 ${TOTAL_CAP}s$( [ "$CALIBRATE" -eq 1 ] && echo '；CALIBRATE 模式——lat 帽只观测不执法' ))"
"$QEMU" -machine virt -cpu cortex-a72 -m 128M -nographic -smp 2 -kernel "$KERNEL" \
  -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio \
  -fw_cfg name=opt/berry/soak,string=1 \
  -fw_cfg name=opt/berry/soakload,string=1 \
  -fw_cfg name=opt/berry/lat,string=1 \
  -global virtio-mmio.force-legacy=false \
  -drive if=none,id=drv0,format=raw,file=disk.soakg3.img,cache.writeback=off \
  -device virtio-blk-device,drive=drv0 \
  -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082,hostfwd=tcp::18083-:18083 -device virtio-net-device,netdev=n0 \
  < /dev/null > "$LOG" 2>&1 &
QEMU_PID=$!

# —— 阶段 1：boot 覆盖十五标记（soak-smp 十二 + lockdep/lat 面三件：
# `LOCKDEP selftest ok`〔G3-3 六阳性三负对照自证——face 缺席即静默非绿〕
# + `LATSGI gate on`〔lat 门键命中自证——键名漂移则 lat 面必假绿〕
# + `LAT trig armed`〔C 形触发装配自证 2026-11-05——soak∧lat 双门且触发
# Event 落静态；缺席 = 常驻面静默退化一次性，恰 N+1 行断言红〕）——
t0=$(date +%s)
COVER=("SMP ok ncores=2" "SOAK gate on" "LAT trig armed" "SOAKLOAD fleet on n=3" "LOCKDEP selftest ok" "LATSGI gate on" "ECO ok" "VIRTIO ok" "SKT ok n=" "FAT ok n=" "NET ok ping=" "DNS ok addr=" "TCP ok echo=" "TCPS ok echo=" "EVS ok n=2")
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null || fail "QEMU 提前退出（boot 期崩溃？）"
  missing=""
  for m in "${COVER[@]}"; do
    command grep -aqF "$m" "$LOG" || missing="${missing:+$missing }$m"
  done
  [ -z "$missing" ] && break
  [ $(( $(date +%s) - t0 )) -gt "$DEADLINE_BOOT" ] \
    && fail "boot 覆盖标记 ${DEADLINE_BOOT}s 未到齐，缺：${missing}（门控键漂移/载荷 master 未跑 = 断言必假绿，勿空心跑）"
  sleep 2
done
BOOT_WALL=$(( $(date +%s) - t0 ))
echo "soak-g3: boot 覆盖标记 ${#COVER[@]}/${#COVER[@]} 到齐（含 LOCKDEP selftest + SOAKLOAD fleet + LATSGI gate + LAT trig armed 四面自证；双核 + 常驻族 + churn 舰队在席）"

# —— 收 evsclient 对拍结果（30s 帽只盖透传余量）——
tc=$(date +%s)
while ! command grep -q 'EVS client ok n=2' "$EVS_LOG" 2>/dev/null; do
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
echo "soak-g3: EVS client 对拍完成（两连接 64B/24B 逐字节）"

# —— 收 sktclient 对拍结果（guest SKT ok 已在覆盖面到齐——30s 帽只盖
# 透传余量）。
tc=$(date +%s)
while ! command grep -q 'SKT client ok n=2' "$SKT_LOG" 2>/dev/null; do
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
echo "soak-g3: SKT client 对拍完成（fd 皮 server 段两连接 64B/24B 逐字节）"

# —— 阶段 2：心跳八断言轮询（首拍潜伏 5 周期 ≈300-1200s wall 是内核侧
# 设计，非挂死）——G1.5 kill 基线扣除语义 = soak-smp 同款（boot 期 rogue
# 矩阵预期杀；阶段 2 起新增即 FAIL；churn 冻结红互补成满覆盖）。
KILL_BASE=$(command grep -ac 'G1.5: kill user task' "$LOG" 2>/dev/null || true)
KILL_BASE=${KILL_BASE:-0}
echo "soak-g3: boot 期 G1.5 kill 基线 ${KILL_BASE}（rogue 矩阵预期杀；阶段 2 起新增即 FAIL）"
tq=$(date +%s)
first_beat_wall=""
last_n=0
last_beat_wall=$tq
summary="(无拍)"
LAT_OBS="(lat 面待终验)"
while :; do
  kill -0 "$QEMU_PID" 2>/dev/null \
    || fail "QEMU 中途退出（崩溃/panic？——心跳表与日志尾部见上）"
  KILL_NOW=$(command grep -ac 'G1.5: kill user task' "$LOG" 2>/dev/null || true)
  KILL_NOW=${KILL_NOW:-0}
  [ "$KILL_NOW" -gt "$KILL_BASE" ] \
    && fail "串口新增 G1.5: kill user task（基线 ${KILL_BASE} → ${KILL_NOW}）——用户线程被杀（舰队缩水，S-5e M1 封口）"
  now=$(date +%s)
  [ $(( now - tq )) -gt "$TOTAL_CAP" ] && fail "总帽 ${TOTAL_CAP}s 破（已收 ${last_n}/${MINUTES} 拍）"
  n=$(beats_raw | wc -l | tr -d ' ')
  n=${n:-0}
  if [ "$n" -gt 0 ]; then
    summary=$(validate_beats) || fail "八断言破：$summary"
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
[ "$mean_beat" -ge 40 ] || fail "均拍 ${mean_beat}s < 40s——拍周期塌缩（deadline 减半形 bug 全绿假象，S-5e ① 封口）"

# —— 阶段 3：G3-5 新增两断言面终验 ——
# lockdep 面（三计数——smoke-lockdep 执法面搬入长跑收口）。
LD_OK=$(command grep -ac 'LOCKDEP selftest ok' "$LOG" 2>/dev/null || true); LD_OK=${LD_OK:-0}
LD_FAIL=$(command grep -ac 'LOCKDEP selftest FAIL' "$LOG" 2>/dev/null || true); LD_FAIL=${LD_FAIL:-0}
LD_VIO=$(command grep -ac 'LOCKDEP VIOLATION' "$LOG" 2>/dev/null || true); LD_VIO=${LD_VIO:-0}
[ "$LD_OK" -eq 1 ] || fail "lockdep 面：selftest ok 计数 ${LD_OK}（期望恰 1；boot COVER 见过行后又被覆写 = 环丢字面）"
[ "$LD_FAIL" -eq 0 ] || fail "lockdep 面：selftest FAIL 计数 ${LD_FAIL}（六阳性三负对照有臂翻车）"
[ "$LD_VIO" -eq 0 ] || fail "lockdep 面：VIOLATION 计数 ${LD_VIO}（真锁序违约——长跑暴露）"
# lat 面（C 形常驻：LATIPC/LATSGI ok 计数 = 拍数+1——boot 周期 + 每拍一
# 周期）。末拍周期行在末拍行后数秒落（周期秒级 vs 拍 60s）——先结算等待
#（45s 帽 < 拍距余量 ≈58s：等得到末周期行、等不到下拍），落齐即收 QEMU
# 冻结日志再终验（后续拍不再进账——计数面闭形）。
N_FINAL=$(beats_raw | wc -l | tr -d ' ')
ts=$(date +%s)
while :; do
  LAT_OK_IPC=$(command grep -ac 'LATIPC ok n=32' "$LOG" 2>/dev/null || true); LAT_OK_IPC=${LAT_OK_IPC:-0}
  LAT_OK_SGI=$(command grep -ac 'LATSGI ok n=32' "$LOG" 2>/dev/null || true); LAT_OK_SGI=${LAT_OK_SGI:-0}
  [ "$LAT_OK_IPC" -ge $(( N_FINAL + 1 )) ] && [ "$LAT_OK_SGI" -ge $(( N_FINAL + 1 )) ] && break
  kill -0 "$QEMU_PID" 2>/dev/null || fail "lat 结算期 QEMU 退出（周期内崩溃/panic？——心跳表与日志尾部见上）"
  [ $(( $(date +%s) - ts )) -gt 45 ] \
    && fail "lat 面：末拍周期行 45s 未落齐（LATIPC ${LAT_OK_IPC}/LATSGI ${LAT_OK_SGI} ≠ $(( N_FINAL + 1 ))——触发断链/周期挂死）"
  sleep 2
done
kill "$QEMU_PID" 2>/dev/null; wait "$QEMU_PID" 2>/dev/null; QEMU_PID=""
LAT_OK_IPC=$(command grep -ac 'LATIPC ok n=32' "$LOG" 2>/dev/null || true); LAT_OK_IPC=${LAT_OK_IPC:-0}
LAT_OK_SGI=$(command grep -ac 'LATSGI ok n=32' "$LOG" 2>/dev/null || true); LAT_OK_SGI=${LAT_OK_SGI:-0}
[ "$LAT_OK_IPC" -eq $(( N_FINAL + 1 )) ] || fail "lat 面：LATIPC ok n=32 计数 ${LAT_OK_IPC} ≠ $(( N_FINAL + 1 ))（boot+每拍一周期——断链/漏触发/自由跑）"
[ "$LAT_OK_SGI" -eq $(( N_FINAL + 1 )) ] || fail "lat 面：LATSGI ok n=32 计数 ${LAT_OK_SGI} ≠ $(( N_FINAL + 1 ))（boot+每拍一周期——断链/漏触发/自由跑）"
lat_face "$N_FINAL" || fail "尾延迟帽面破（收割与帽判定见上）"

# S-5e m6 同款措辞边界：pages 稳态窗真比较最早第 7 拍。
if [ "$MINUTES" -ge 7 ]; then
  echo "soak-g3: PASS ${MINUTES}/${MINUTES} 拍八断言全绿 + lockdep 三计数绿（ok 1/FAIL 0/VIOLATION 0）+ lat ${LAT_OBS}"
else
  echo "soak-g3: PASS ${MINUTES}/${MINUTES} 拍七断言全绿 + pages 稳态窗未及执法（≥7 拍才真比较，n=${MINUTES} 平凡非绿）+ lockdep 三计数绿 + lat ${LAT_OBS}"
fi
echo "soak-g3: R3 定标：boot=${BOOT_WALL}s（帽 ${DEADLINE_BOOT}s）/ 首拍=${first_beat_wall}s（帽 ${FIRST_BEAT_CAP}s）/ 均拍=${mean_beat}s（帽 ${PER_BEAT_CAP}s，下界 40）/${summary}（park 帽 +${PARK_CAP} / Δsw1 地板 ${SW1_FLOOR}/拍 / pages 稳态帽 +64 首拍豁免 / churn 严格递增）"
echo "soak-g3: 全程 ${elapsed}s（总帽 ${TOTAL_CAP}s）"
