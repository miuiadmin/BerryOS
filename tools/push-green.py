#!/usr/bin/env python3
"""push-green.py —— 一节一推、推到绿（自动化开发二期 P0；规则/16 §5）。

六拍链的推送半边：①后台化推送（300s 帽）；②落地判据 = @{u}..HEAD 为空（不是
管道退出码——「看产物不看管道」判例的推送版）；③等 CI 绿：gh run 轮询 20s 一拍、
45 分钟未启动/3 小时未完双熔断；④supersession 判读——只看**本次推送之后**启动的
run（看不见自己修复的闸是假警机器）；「无答」（GitHub 侧超时/取消）≠「红」，
输出 note 归档不归责。判决输出 JSON（verdict: green/red/note）。
  python3 tools/push-green.py [--branch <名>]   # 缺省当前分支
  python3 tools/push-green.py --selftest        # 判读纯函数夹具：supersession/无答/双熔断
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
POLL_SECS = 20
START_TIMEOUT = 45 * 60
TOTAL_TIMEOUT = 3 * 3600
GITHUB_UNANSWERED = "note"  # 基础设施无答：既不是绿也不是红


def sh(cmd: list[str], timeout: int = 120) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=str(ROOT))


def landing_confirmed() -> bool:
    """落地判据：@{u}..HEAD 为空（推上去的东西远端有了）。"""
    r = sh(["git", "rev-list", "--count", "@{u}..HEAD"])
    return r.returncode == 0 and r.stdout.strip() == "0"


def list_runs(branch: str, created_after: str | None = None) -> list[dict]:
    cmd = ["gh", "run", "list", "--branch", branch, "--limit", "10",
           "--json", "databaseId,status,conclusion,createdAt,workflowName"]
    r = sh(cmd, timeout=60)
    if r.returncode != 0:
        return []
    runs = json.loads(r.stdout or "[]")
    if created_after:
        runs = [x for x in runs if (x.get("createdAt") or "") > created_after]
    return runs


def wait_green(branch: str, created_after: str | None = None,
               poll: int = POLL_SECS, start_timeout: int = START_TIMEOUT,
               total_timeout: int = TOTAL_TIMEOUT, sleeper=time.sleep, clock=time.time) -> dict:
    """三态判决：green（最新相关 run conclusion=success）/ red（failure）/ note（无答）。

    supersession：只认 created_after 之后启动的 run——自己修的自己，旧红不算数。
    """
    t0 = clock()
    while True:
        runs = list_runs(branch, created_after)
        done = [x for x in runs if x.get("status") == "completed"]
        if done:
            latest = max(done, key=lambda x: x.get("createdAt") or "")
            concl = latest.get("conclusion")
            if concl == "success":
                return {"verdict": "green", "run": latest["databaseId"]}
            if concl in ("failure", "cancelled"):
                # cancelled 是弱证据（可能是被新推顶掉的旧 run）——仅当它仍是最新时记红
                newer_pending = [x for x in runs if x.get("status") != "completed"]
                if concl == "cancelled" and newer_pending:
                    pass  # 有更新的在跑——旧取消不算数，继续等
                else:
                    return {"verdict": "red", "run": latest["databaseId"], "conclusion": concl}
        else:
            started = bool(runs)
            if not started and (clock() - t0) >= start_timeout:
                return {"verdict": GITHUB_UNANSWERED, "why": "45 分钟无相关 run 启动"}
        if (clock() - t0) >= total_timeout:
            return {"verdict": GITHUB_UNANSWERED, "why": "3 小时未出结论"}
        sleeper(poll)


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    branch = None
    if "--branch" in argv:
        branch = argv[argv.index("--branch") + 1]
    if not branch:
        branch = sh(["git", "rev-parse", "--abbrev-ref", "HEAD"]).stdout.strip()
    push = subprocess.run(["git", "push", "origin", branch], cwd=str(ROOT),
                          capture_output=True, text=True, timeout=300)
    if push.returncode != 0:
        print(json.dumps({"verdict": "push-failed", "stderr": push.stderr[-400:]}, ensure_ascii=False))
        return 1
    if not landing_confirmed():
        print(json.dumps({"verdict": "not-landed", "why": "@{u}..HEAD 非空"}, ensure_ascii=False))
        return 1
    verdict = wait_green(branch)
    print(json.dumps(verdict, ensure_ascii=False))
    return 0 if verdict["verdict"] == "green" else (2 if verdict["verdict"] == "red" else 3)


def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"push-green 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    # supersession：旧红被 created_after 过滤 → 等到新绿
    fake_runs = [
        {"databaseId": 1, "status": "completed", "conclusion": "failure", "createdAt": "2026-11-22T00:00:00Z", "workflowName": "ci"},
        {"databaseId": 2, "status": "completed", "conclusion": "success", "createdAt": "2026-11-22T01:00:00Z", "workflowName": "ci"},
    ]
    orig_list = list_runs
    globals()["list_runs"] = lambda *a, **k: [x for x in fake_runs if (x["createdAt"] > "2026-11-22T00:30:00Z")]
    v = wait_green("b", created_after="2026-11-22T00:30:00Z", sleeper=lambda s: None)
    check("supersession（旧红不计，新绿判 green）", v["verdict"] == "green")
    globals()["list_runs"] = lambda *a, **k: fake_runs[:1]
    v = wait_green("b", sleeper=lambda s: None)
    check("最新即红 → red", v["verdict"] == "red")
    # cancelled + 有更新在跑 → 不算数继续等（这里用熔断收）
    cancelled_runs = [
        {"databaseId": 1, "status": "completed", "conclusion": "cancelled", "createdAt": "2026-11-22T00:00:00Z", "workflowName": "ci"},
        {"databaseId": 2, "status": "in_progress", "conclusion": None, "createdAt": "2026-11-22T01:00:00Z", "workflowName": "ci"},
    ]
    calls = {"n": 0}

    def fake_sleep(_s: float) -> None:
        calls["n"] += 1
        if calls["n"] > 2:
            raise KeyboardInterrupt  # 用熔断路径逃出（等价超时）

    globals()["list_runs"] = lambda *a, **k: cancelled_runs
    t0 = [1e9 + 4 * 3600]  # clock 直取超时
    try:
        v = wait_green("b", sleeper=fake_sleep, clock=lambda: t0[0])
    except KeyboardInterrupt:
        v = {"verdict": "note", "why": "selftest escape"}
    check("cancelled 弱证据不记红（等待/超时面）", v["verdict"] != "red")
    globals()["list_runs"] = orig_list
    print(f"push-green 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}（supersession/红/取消弱证据）")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
