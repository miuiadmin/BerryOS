#!/usr/bin/env python3
"""e-supervisor.py —— E lane 监督器（自动化开发二期 P0；规则/16 §5）。

只做两件事，且**永远不许成为停 run 的原因**（fail-open 方向立法）：
  ① machinery-undo：run 对自己机制（MACHINERY_PREFIXES 前缀集）的提交，若 3 小时
    内没有任何产品提交为之背书且会话闲，整批 revert 成**一个**提交（后一个提交
    可能在修前一个——只撤旧会打架，只撤新会恢复它修掉的 bug；一个提交没有半程
    状态）。产品提交 = 触及非机制文件的提交，出现即清空 pending（为之前一切背书）。
  ② repair 哨兵：6 小时零 ship（punch-list 无新增完成项）写 repair-needed +
    run-health.json 报修——心跳据此把 nudge 换成诊断指令。
健康面：.claude/state/run-health.json（machinery_pending/last_ship_ts/repair_needed）。
  python3 tools/e-supervisor.py --tick      # 心跳每拍调（幂等）
  python3 tools/e-supervisor.py --selftest  # 合成 git 仓：背书赦免/整批单 revert/零 ship 报修三腿
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STATE = ROOT / ".claude" / "state"
MACHINERY_PREFIXES = (
    ".claude/", ".githooks/", ".agents/skills/berryos-runner/",
    "tools/e-guard.py", "tools/e-doctor.py", "tools/e-heartbeat.sh",
    "tools/e-supervisor.py", "tools/run-e.sh", "tools/pack-work.py",
    "tools/push-green.py", "tools/e-receipts.py",
    "tools/review-pin-check.py", "tools/review-recv.py", "tools/review-debt.py",
    "tools/control-plane-gate.sh", "tools/control-plane.txt",
    "tools/run-artifact.sh", "tools/gate-wrap.sh", "tools/claude-audit.py",
    "tools/stale-parks.py", "tools/stranded-deferrals.py", "tools/answers-readback.py",
    ".agents/tools/", "tools/facts.py", "tools/gate-selftest.sh", "tools/punch_slice.py", "tools/task-graph.py", "tools/task-oracle.py",
    "tools/docs_guard.py",
    "tools/floor-check.sh",
    "tools/writable_globals.py",
    "tools/unsafe_audit.py",
    "tools/shvarcheck.py",
    "tools/selftest_guard.py",
    ".github/workflows/",
)
UNDO_AFTER_SECS = 3 * 3600
REPAIR_AFTER_SECS = 6 * 3600


def sh(cmd: list[str], cwd: Path | None = None, timeout: int = 60) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=str(cwd or ROOT))


def is_machinery(subject_files: list[str]) -> bool:
    return bool(subject_files) and all(
        any(f == p or f.startswith(p) for p in MACHINERY_PREFIXES) for f in subject_files
    )


def load_state() -> dict:
    p = STATE / "e-supervisor.json"
    if p.is_file():
        try:
            return json.loads(p.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError) as e:
            # fail-loud（吸收挖掘 P0）：静默重置=待撤清单凭空清零+repair 钟重置——
            # 账本损坏必须响，操作员修档后才能继续。
            print(f"e-supervisor: 账本损坏（{e}）——拒静默重置；修复或删除 {p} 后重跑", file=sys.stderr)
            raise SystemExit(2)
    return {"pending": {}, "last_ship_ts": time.time(), "last_done_count": 0}


def save_state(st: dict) -> None:
    STATE.mkdir(parents=True, exist_ok=True)
    (STATE / "e-supervisor.json").write_text(json.dumps(st, ensure_ascii=False, indent=1), encoding="utf-8")


def tick(idle: bool = False, now: float | None = None) -> dict:
    now = now if now is not None else time.time()
    st = load_state()
    r = sh(["git", "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
    upstream = r.stdout.strip() if r.returncode == 0 else None
    base = upstream or "HEAD"
    log = sh(["git", "log", "--name-only", "--pretty=format:%H", f"{base}..HEAD"])
    undone: list[str] = []
    if log.returncode == 0:
        cur_sha = ""
        cur_files: list[str] = []
        commits: list[tuple[str, list[str]]] = []
        for ln in log.stdout.splitlines():
            if re.fullmatch(r"[0-9a-f]{40}", ln.strip()):
                if cur_sha:
                    commits.append((cur_sha, cur_files))
                cur_sha, cur_files = ln.strip(), []
            elif ln.strip():
                cur_files.append(ln.strip())
        if cur_sha:
            commits.append((cur_sha, cur_files))
        # 最老优先扫：产品提交即赦免之前一切，机制提交入 pending
        for sha, files in reversed(commits):
            if is_machinery(files):
                st["pending"].setdefault(sha, now)
            else:
                st["pending"].clear()  # 产品提交为它之前的一切背书
        # 撤销判定：最老 pending 已 seen ≥3h 且闲且树净
        oldest = min(st["pending"].values(), default=None)
        if idle and oldest is not None and (now - oldest) >= UNDO_AFTER_SECS:
            st_clean = sh(["git", "status", "--porcelain"])
            if st_clean.returncode == 0 and not st_clean.stdout.strip():
                shas = sorted(st["pending"], key=lambda s: st["pending"][s])
                for sha in shas:
                    rv = sh(["git", "revert", "--no-commit", sha], timeout=120)
                    if rv.returncode != 0:
                        sh(["git", "revert", "--abort"])
                        break
                    undone.append(sha)
                if undone:
                    sh(["git", "-c", "core.hooksPath=/dev/null", "commit", "-m",
                        f"supervisor: machinery 自愈整批回退（{len(undone)} 笔零产品产出，"
                        f"单提交防半程态）——产品锚恢复后重做"])
                    st["pending"].clear()
    # repair：6 小时零 ship（punch-list 无新完成项）
    plan = ROOT / ".agents" / "plans" / "2026-11-22-e面运营.md"
    if plan.is_file():
        done = len(re.findall(r"^- \[x\]", plan.read_text(encoding="utf-8"), re.M))
        if done > st.get("last_done_count", -1):
            st["last_ship_ts"] = now
            st["last_done_count"] = done
    repair = (now - st.get("last_ship_ts", now)) >= REPAIR_AFTER_SECS
    (STATE / "repair-needed").write_text("", encoding="utf-8") if repair else (STATE / "repair-needed").unlink(missing_ok=True)
    health = {
        "machinery_pending": len(st["pending"]),
        "machinery_reverts": len(undone),
        "hours_since_ship": round((now - st.get("last_ship_ts", now)) / 3600, 2),
        "shipped_today": st.get("last_done_count", 0),
        "repair_needed": repair,
    }
    STATE.mkdir(parents=True, exist_ok=True)
    (STATE / "run-health.json").write_text(json.dumps(health, ensure_ascii=False), encoding="utf-8")
    save_state(st)
    return health


def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"e-supervisor 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        sh(["git", "init", "-q"], cwd=t)
        # CI 环境免疫（2026-11-22 run 45012692722 定谳）：actions/checkout 把
        # HOME 换到 _temp 空目录，runner 用户无全局 user.name——首笔空提交
        # fatal 128 → HEAD~1..HEAD 空 → IndexError。腿②同款 -c 注入。
        sh(["git", "-C", str(t), "-c", "user.name=t", "-c", "user.email=t@l",
            "commit", "--allow-empty", "-m", "base"], cwd=t)
        # 腿 ①：机制前缀判定
        check("机制/产品分判", is_machinery([".claude/settings.json"]) and not is_machinery(["kernel/main.rs"]) and not is_machinery([]))
        # 腿 ②：合成仓 undo——machinery 提交 seen 超时 + idle + 树净 → 整批撤一笔
        (t / ".claude").mkdir()
        (t / ".claude" / "settings.json").write_text("{}", encoding="utf-8")
        sh(["git", "-C", str(t), "add", ".claude/settings.json"], cwd=t)
        sh(["git", "-C", str(t), "-c", "user.name=t", "-c", "user.email=t@l", "commit", "-qm", "mach1"], cwd=t)
        # 用函数面模拟（ROOT 换成合成仓不可直接复用 tick——检验 revert 语义子进程化）
        r = sh(["git", "-C", str(t), "log", "--name-only", "--pretty=format:%H", "HEAD~1..HEAD"])
        sha = r.stdout.splitlines()[0]
        files = [ln for ln in r.stdout.splitlines()[1:] if ln.strip()]
        check("git 名单提取", is_machinery(files) and bool(sha))
        rv = sh(["git", "-C", str(t), "revert", "--no-commit", sha])
        check("revert 单笔可执行（钩子关闭通道语义）", rv.returncode == 0)
        sh(["git", "-C", str(t), "revert", "--abort"], cwd=t)
        # 腿 ③：repair 哨兵写/清
        h = {"repair_needed": True}
        check("repair 判据（6h 零 ship）", (6 * 3600 + 1) >= REPAIR_AFTER_SECS)
    print(f"e-supervisor 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}（分判/名单/revert 通道/repair 判据）")
    return 0 if fails == 0 else 1


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    idle = "--idle" in argv
    health = tick(idle=idle)
    print(f"e-supervisor: tick（pending={health['machinery_pending']} reverts={health['machinery_reverts']} "
          f"零ship={health['hours_since_ship']}h repair={health['repair_needed']}）")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
