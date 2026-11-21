#!/usr/bin/env bash
# run-e.sh —— E lane 拉起器（自动化开发二期 P0；规则/16 §5）。
# 单入口：doctor → marker → tmux 起 run 会话（claude，独立 session-id，
# BERRYOS_E_RUN=1）+ 心跳会话；stop 是操作员专属（无人会话环境里拒执——
# 「the run cannot stop itself」）。一切状态在盘上（.claude/state/）。
# 用法：bash tools/run-e.sh start|stop|status|watch|launch-session
set -u
RUN_NAME="berryos-e-run"
HB_NAME="berryos-e-hb"
STATE_DIR=$(git rev-parse --show-toplevel 2>/dev/null)/.claude/state
MARKER="$STATE_DIR/e-run.marker"

RUN_PROMPT='完整读取 .agents/skills/berryos-runner/SKILL.md 并严格执行。每轮第一个动作：python3 tools/pack-work.py 拿开工包。活干完跑 python3 tools/e-guard.py fixpoint 过四门写哨兵自然停。30 分钟内一轮。'

cmd_start() {
  python3 tools/e-doctor.py || exit 1
  mkdir -p "$STATE_DIR"
  if [ -f "$MARKER" ]; then
    if tmux has-session -t "=$RUN_NAME" 2>/dev/null; then
      echo "run-e: 已在跑（marker+会话俱在）——先 stop"; exit 1
    fi
    echo "run-e: 陈旧 marker（run 会话不在——重启/tmux 全灭残态），清后继续"
    rm -f "$MARKER"
    [ -f "$STATE_DIR/e-fixpoint.sentinel" ] || true
  fi
  date +%s > "$MARKER"
  cmd_launch-session
  tmux new-session -d -s "=$HB_NAME" "bash tools/e-heartbeat.sh $RUN_NAME"
  echo "run-e: 已发射（run=$RUN_NAME hb=$HB_NAME）——bash tools/run-e.sh watch 观察"
}

cmd_launch-session() {
  mkdir -p "$STATE_DIR"
  sid_file="$STATE_DIR/e-run-session-id"
  if [ -f "$sid_file" ] && [ -n "$(cat "$sid_file")" ]; then
    sid=$(cat "$sid_file")
    BERRYOS_E_RUN=1 CLAUDE_PROJECT_DIR="$(git rev-parse --show-toplevel)" \
      tmux new-session -d -s "=$RUN_NAME" \
      "claude --resume $sid --dangerously-skip-permissions '读盘上状态继续跑：python3 tools/pack-work.py'"
  else
    sid=$(uuidgen | tr 'A-Z' 'a-z')
    printf '%s' "$sid" > "$sid_file"
    BERRYOS_E_RUN=1 CLAUDE_PROJECT_DIR="$(git rev-parse --show-toplevel)" \
      tmux new-session -d -s "=$RUN_NAME" \
      "claude --session-id $sid --dangerously-skip-permissions '$RUN_PROMPT'"
  fi
}

cmd_stop() {
  if [ "${BERRYOS_E_RUN:-0}" = "1" ]; then
    echo "run-e: 拒绝——无人会话不能停自己（the run cannot stop itself）。操作员在宿主 shell 跑本命令。" >&2
    exit 1
  fi
  rm -f "$MARKER" "$STATE_DIR/e-fixpoint.sentinel" 2>/dev/null
  tmux kill-session -t "=$RUN_NAME" 2>/dev/null
  tmux kill-session -t "=$HB_NAME" 2>/dev/null
  echo "run-e: 已停（marker/哨兵清，两会话杀）"
}

cmd_status() {
  echo "marker: $([ -f "$MARKER" ] && echo 在 || echo 无)"
  _st=0
  echo "sentinel: $([ -f "$STATE_DIR/e-fixpoint.sentinel" ] && echo 在 || echo 无)"
  echo "run 会话: $(tmux has-session -t "=$RUN_NAME" 2>/dev/null && echo 活 || echo 死)"
  echo "心跳会话: $(tmux has-session -t "=$HB_NAME" 2>/dev/null && echo 活 || echo 死)"
  if [ -f "$STATE_DIR/run-health.json" ]; then cat "$STATE_DIR/run-health.json"; fi
  return $_st
}

cmd_install_launchd() {
  ws=${1:?用法: run-e.sh install-launchd <工作区绝对路径>}
  plist="$HOME/Library/LaunchAgents/com.berryos.e-run.plist"
  cat > "$plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.berryos.e-run</string>
  <key>ProgramArguments</key><array>
    <string>/bin/bash</string><string>${ws}/tools/run-e.sh</string><string>start</string>
  </array>
  <key>WorkingDirectory</key><string>${ws}</string>
  <key>RunAtLoad</key><true/>
  <key>StandardOutPath</key><string>${ws}/out/e-run-launchd.log</string>
  <key>StandardErrorPath</key><string>${ws}/out/e-run-launchd.log</string>
</dict></plist>
PLIST
  echo "run-e: 已写 $plist——装载：launchctl load $plist（卸载用 unload；start 幂等：marker 在即拒双跑）"
}

case "${1:-}" in
  start) cmd_start ;;
  stop) cmd_stop ;;
  status) cmd_status ;;
  watch) tmux attach-session -t "=$RUN_NAME" -r 2>/dev/null || echo "run 会话不在——先 start"; ;;
  launch-session) cmd_launch-session ;;
  install-launchd) cmd_install_launchd "$2" ;;
  *) echo "usage: run-e.sh {start|stop|status|watch|launch-session|install-launchd <工作区绝对路径>}" >&2; exit 2 ;;
esac
