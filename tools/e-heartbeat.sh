#!/usr/bin/env bash
# e-heartbeat.sh —— E lane 心跳判活（自动化开发二期 P0；规则/16 §5）。
# 300 秒一拍：marker 没了 / fixpoint 哨兵在 → 干完了，退出码 10 收心跳。
# 判活信号 = 本会话自己的 transcript mtime（旁边有人操作的会话写东西不代表
# 这个 run 活着）；闲 ≥45 分钟且无附着客户端 → 送一句最小 nudge；会话死 →
# 300×2^n 退避（封顶 1h）用 --resume 接回自己的 session-id。每拍顺带以
# 2400s 帽调 supervisor。心跳自身永远不主动杀 run（fail-open 方向立法）。
# 用法：bash tools/e-heartbeat.sh <run-tmux-会话名>（run-e.sh start 拉起）。
set -u
RUN_NAME=${1:?用法: e-heartbeat.sh <run tmux 会话名>}
TICK_SECS=300
IDLE_SECS=2700
MAX_BACKOFF=3600
deaths=0
wait_until=0
last_restart=0

# tmux 目标一律 =前缀精确匹配（前缀撞名会让死 run 看着活——impossible-os 判例）
run_alive() { tmux has-session -t "=$RUN_NAME" 2>/dev/null; }
send_line() { tmux send-keys -t "=$RUN_NAME" "$1" Enter; }

# 本 run 的 transcript：~/.claude/projects/<转义路径>/<session-id>.jsonl
run_transcript() {
  root=$(git rev-parse --show-toplevel 2>/dev/null) || return 1
  sid=$(cat "$root/.claude/state/e-run-session-id" 2>/dev/null) || return 1
  esc=$(printf '%s' "$root" | sed 's/[.\/]/-/g')
  t="$HOME/.claude/projects/$esc/$sid.jsonl"
  [ -f "$t" ] && printf '%s' "$t" && return 0
  printf '%s' "$root/.claude/state/e-run-session-id"  # 退避面：sid 文件自身 mtime
}

while :; do
  sleep "$TICK_SECS"
  root=$(git rev-parse --show-toplevel 2>/dev/null) || exit 12
  marker="$root/.claude/state/e-run.marker"
  sentinel="$root/.claude/state/e-fixpoint.sentinel"
  [ -f "$marker" ] || { echo "e-heartbeat: marker 没了（操作员 stop）——收心跳"; exit 10; }
  [ -f "$sentinel" ] && { echo "e-heartbeat: fixpoint 哨兵在——干完了，收心跳"; exit 10; }

  now=$(date +%s)
  if run_alive; then
    tf=$(run_transcript) || tf=""
    idle=999999
    if [ -n "$tf" ] && [ -f "$tf" ]; then
      idle=$(( now - $(stat -f %m "$tf" 2>/dev/null || stat -c %Y "$tf" 2>/dev/null || echo "$now") ))
    fi
    # 只读观察者不算人（审计修正：原 grep -vc 匹配不到 tmux 输出——恒计入 attached 压制 nudge）
    attached=$(tmux list-clients -t "=$RUN_NAME" -F '#{client_readonly}' 2>/dev/null | grep -cx 0 || true)
    if [ "$idle" -ge "$IDLE_SECS" ] && [ "$attached" -eq 0 ]; then
      # 闲且无人附着才传 --idle（machinery 整批回退的活路径；2400s 帽=supervisor 自身不许拖死心跳）。
      # 输出落 last 态日志不丢弃（终审 P1：旧 >/dev/null 把回退失败/冲突诊断全吞——
      # 整批放弃与提交失败只写 stderr，丢弃即无报警面）；|| true 维持——心跳自身永不因 supervisor 死。
      timeout 2400 python3 "$root/tools/e-supervisor.py" --tick --idle >"$root/.claude/state/supervisor-last.log" 2>&1 || true
    else
      timeout 2400 python3 "$root/tools/e-supervisor.py" --tick >"$root/.claude/state/supervisor-last.log" 2>&1 || true
    fi
    # （同上条件，变量未变——supervisor 调用与 nudge 同一闲态分支，审计 r2 合并）
    if [ "$idle" -ge "$IDLE_SECS" ] && [ "$attached" -eq 0 ]; then
      if [ -f "$root/.claude/state/repair-needed" ]; then
        send_line "repair 模式：从 git log、门禁输出与 smoke 证物找 6 小时零 ship 的原因，在拥有它的层修，或记 punch-list 后换下一项"
      elif [ -f "$root/.claude/state/product-starved" ]; then
        send_line "product-starved 模式：连续 3 轮产品面塌陷（零产品或 tooling>2x product，详见 .claude/state/run-health.json 与 supervisor-last.log）——优先捡 punch-list/DAG 里触及产品代码的任务，工具面自我服务停一停"
      else
        send_line "Continue the run."
      fi
    fi
    if [ $(( now - last_restart )) -ge 3600 ]; then deaths=0; wait_until=0; fi
  else
    echo "e-heartbeat: run 会话不在（第 $(( deaths + 1 )) 次）"
    if [ "$now" -lt "$wait_until" ]; then continue; fi
    deaths=$(( deaths + 1 ))
    backoff=$(( 300 * ( 2 ** ( deaths < 5 ? deaths : 5 ) ) ))
    [ "$backoff" -gt "$MAX_BACKOFF" ] && backoff=$MAX_BACKOFF
    wait_until=$(( now + backoff ))
    last_restart=$now
    echo "e-heartbeat: ${backoff}s 后 resume（退避 300×2^n 封顶 1h）"
    (
      cd "$root" || exit 1
      sid=$(cat .claude/state/e-run-session-id 2>/dev/null)
      if [ -n "$sid" ]; then
        BERRYOS_E_RUN=1 CLAUDE_PROJECT_DIR="$root" tmux new-session -d -s "$RUN_NAME" \
          "claude --resume $sid --dangerously-skip-permissions '读盘上状态继续跑：git status、punch-list、tools/pack-work.py 的开工包'"
      else
        BERRYOS_E_RUN=1 CLAUDE_PROJECT_DIR="$root" tmux new-session -d -s "$RUN_NAME" \
          "bash tools/run-e.sh launch-session"
      fi
    ) || echo "e-heartbeat: resume 失败——下拍重试"
  fi
done
