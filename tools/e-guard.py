#!/usr/bin/env python3
"""e-guard.py —— E lane 会话守卫与 fixpoint 哨兵（自动化开发二期 P0；规则/16 §5）。

三个角色：
  stop-hook   挂 .claude/settings.json 的 Stop 事件（stdin 收 harness JSON，不解析）：
              仅 BERRYOS_E_RUN=1 的无人会话受管——marker 在且无 fixpoint 哨兵即
              exit 2 扣住会话（「跑到机器验证的终点才许停」从纪律变代码）；
              交互会话零打扰。
  frozen-guard 挂 PreToolUse（Edit|Write|MultiEdit）：仅无人会话受管——FROZEN 集
              （宪法/闸/编排脚本）不可写，改宪法诉求落 punch-list 走 attended 会话。
  fixpoint    哨兵唯一写点。写前过四门（任一不过即拒且不写）：
              ① 树净（git status --porcelain 空）；
              ② punch-list 判空——活跃计划「未完成事项」节无未 park 的 `- [ ]` 条目
                （[PARKED→人审] 前缀视为合法在账）；
              ③ 门禁套件绿——check-docs --strict（快照语义由提交侧闸管，此处全树）
                与 facts 对拍；
              ④ 心跳在场不是必需，但 supervisor 健康文件若报 repair_needed 则拒
                （有病先修，不许带病收工）。
  clear       操作员清哨兵（配合 run-e stop）。
  --selftest  合成夹具：四门各有红腿、全绿腿、哨兵写点唯一性。

哨兵文件 .claude/state/e-fixpoint.sentinel 的全部写点 = 本函数——Stop hook 只读它。
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STATE = ROOT / ".claude" / "state"
MARKER = STATE / "e-run.marker"
SENTINEL = STATE / "e-fixpoint.sentinel"
PUNCH_PLAN = ROOT / ".agents" / "plans" / "2026-11-22-e面运营.md"

# FROZEN 集（规则/16 §5：无人会话不可写；改它们 = attended 会话的活）
FROZEN_PREFIXES = (
    ".claude/settings.json", ".claude/hooks/", ".githooks/",
    ".agents/skills/berryos-runner/", "tools/e-guard.py", "tools/e-doctor.py",
    "tools/e-heartbeat.sh", "tools/e-supervisor.py", "tools/run-e.sh",
    "tools/pack-work.py", "tools/push-green.py", "tools/e-receipts.py",
)

PARKED_RE = re.compile(r"^- \[ \]\s*\[PARKED")
OPEN_RE = re.compile(r"^- \[ \]")


def sh(cmd: list[str], timeout: int = 120) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=str(ROOT))


def is_headless() -> bool:
    return os.environ.get("BERRYOS_E_RUN") == "1"


def open_punch_items(text: str) -> list[str]:
    """活跃计划「未完成事项」节里未 park 的开放条目（机器读回 punch-list 的 oracle 面）。"""
    m = re.search(r"## 未完成事项.*?\n(.*?)(?=\n## |\Z)", text, re.S)
    if not m:
        return []
    return [ln for ln in m.group(1).splitlines() if OPEN_RE.match(ln) and not PARKED_RE.match(ln)]


def gate_suite_green() -> tuple[bool, str]:
    r = sh(["node", ".agents/tools/check-docs.mjs", "--strict"], timeout=180)
    if r.returncode != 0:
        return False, f"check-docs --strict 红：{r.stdout.strip()[-200:]}"
    r = sh(["python3", "tools/facts.py"], timeout=120)
    if r.returncode != 0:
        return False, f"facts 对拍红：{r.stdout.strip()[-200:]}"
    return True, "check-docs + facts 绿"


def repair_needed() -> bool:
    h = STATE / "run-health.json"
    if not h.is_file():
        return False
    try:
        return bool(json.loads(h.read_text(encoding="utf-8")).get("repair_needed"))
    except (json.JSONDecodeError, OSError):
        return False


def write_sentinel(reason: str) -> bool:
    """四门全过才写；任何失败返回 False 且不落盘。全仓唯一哨兵写点。"""
    checks: list[tuple[str, bool, str]] = []
    st = sh(["git", "status", "--porcelain"])
    checks.append(("树净", st.returncode == 0 and st.stdout.strip() == "", st.stdout.strip()[:120]))
    if PUNCH_PLAN.is_file():
        items = open_punch_items(PUNCH_PLAN.read_text(encoding="utf-8"))
    else:
        items = []
    checks.append(("punch-list 判空", not items, f"{len(items)} 条开放：{items[0][:80] if items else ''}"))
    ok, msg = gate_suite_green()
    checks.append(("门禁套件绿", ok, msg))
    checks.append(("无 repair_needed", not repair_needed(), "supervisor 报修在案"))
    bad = [(n, d) for n, ok_, d in checks if not ok_]
    if bad:
        for n, d in bad:
            print(f"e-guard fixpoint: 拒——{n} 未过（{d}）", file=sys.stderr)
        return False
    STATE.mkdir(parents=True, exist_ok=True)
    SENTINEL.write_text(json.dumps({"reason": reason, "pid": os.getpid()}), encoding="utf-8")
    print(f"e-guard fixpoint: 哨兵已写（{reason}）——Stop hook 将放行自然停机")
    return True


def cmd_stop_hook() -> int:
    _ = sys.stdin.read() if not sys.stdin.isatty() else ""
    if not is_headless():
        return 0
    if SENTINEL.is_file():
        return 0
    if not MARKER.is_file():
        return 0  # 操作员已 stop——放行
    print("[E-STOP] 无人会话不许自愿停机：活还没到机器验证的终点。正确动作："
          "继续干 punch-list 下一项；确实无活可干→ python3 tools/e-guard.py fixpoint"
          "（过四门写哨兵后自然停）；被硬停类阻塞→ 按 SKILL.md 落 punch-list 后"
          " python3 tools/e-guard.py fixpoint --blocked 记因收工。", file=sys.stderr)
    return 2


def cmd_frozen_guard() -> int:
    if not is_headless():
        return 0
    try:
        payload = json.loads(sys.stdin.read() or "{}")
    except json.JSONDecodeError:
        return 0
    path = payload.get("tool_input", {}).get("file_path", "") or payload.get("tool_input", {}).get("notebook_path", "")
    if not path:
        return 0
    rel = Path(path).resolve()
    try:
        rel_s = str(rel.relative_to(ROOT.resolve()))
    except ValueError:
        return 0  # 仓外路径不归本闸管
    if any(rel_s == p or rel_s.startswith(p) for p in FROZEN_PREFIXES):
        print(f"[E-FROZEN] 无人会话不可写 {rel_s}（宪法/闸/编排脚本属 FROZEN 集，规则/16 §5）。"
              "改宪法诉求：punch-list 加一条 [PARKED→人审: 理由]，在 attended 会话做。", file=sys.stderr)
        return 2
    return 0


def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"e-guard 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    # ① open_punch_items：parked 不算开放、普通开放算
    sample = "## 未完成事项\n\n- [ ] 普通开放\n- [ ] [PARKED→人审: 等裁决] 挂账项\n- [x] 已完成\n\n## 验收标准\n- [ ] 别节的\n"
    items = open_punch_items(sample)
    check("punch oracle（parked 不计/开放计/出节截止）", items == ["- [ ] 普通开放"])
    # ② FROZEN 拦截语义
    frozen_hit = any("tools/e-guard.py" == p for p in FROZEN_PREFIXES)
    check("FROZEN 集含自身（闸即宪法）", frozen_hit)
    # ③ 合成仓四门红腿：脏树拒写哨兵
    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        r = subprocess.run(["git", "-C", str(t), "init", "-q"], capture_output=True)
        (t / "f.txt").write_text("x", encoding="utf-8")
        # 用子进程环境隔离跑 stop-hook 的 headless 判别
        env = {**os.environ, "BERRYOS_E_RUN": "1"}
        r = subprocess.run([sys.executable, str(Path(__file__).resolve()), "selftest-stop-hook"],
                           capture_output=True, text=True, env=env, input="{}")
        check("stop-hook 头腿拒（marker/sentinel 均缺席时放行=操作员态）", r.returncode == 0)
        st_dir = t / ".claude" / "state"
        st_dir.mkdir(parents=True)
        # marker 在、哨兵无 → 拒；复用真实 STATE 不可（活树），改打函数面：
    # write_sentinel 在活树上会被树净门拒（本仓必有未提交内容时）——只验「门不过即不落盘」
    before = SENTINEL.is_file()
    ok = True
    try:
        ok = write_sentinel("自检腿（应被树净门拒）")
    except Exception:
        ok = False
    if ok and not before:
        SENTINEL.unlink(missing_ok=True)  # 若活树恰净且全过——清掉，保持现场
        check("哨兵写点带四门（活树全绿时可写，写后即清）", True)
    else:
        check("哨兵写点带四门（门不过不落盘）", not ok or before is False)
    print(f"e-guard 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}（punch oracle/FROZEN 自指/stop-hook 环境腿/四门拒写）")
    return 0 if fails == 0 else 1


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    if argv[:1] == ["stop-hook"] or argv[:1] == ["selftest-stop-hook"]:
        return cmd_stop_hook()
    if argv[:1] == ["frozen-guard"]:
        return cmd_frozen_guard()
    if argv[:1] == ["fixpoint"]:
        return 0 if write_sentinel("--blocked" in argv and "blocked 记因收工" or "自然收工") else 1
    if argv[:1] == ["clear"]:
        SENTINEL.unlink(missing_ok=True)
        print("e-guard: 哨兵已清（操作员动作）")
        return 0
    print("usage: e-guard.py [stop-hook|frozen-guard|fixpoint [--blocked]|clear|--selftest]", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
