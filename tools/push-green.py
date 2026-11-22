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
import os
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


LIGHT_PATHS_RE = None  # 惰性编译见 is_light_only()


def diff_is_light_only(files: list[str]) -> bool:
    """混合轮假绿闸（审计 r2）：diff 全落轻面（docs/.agents/md/tools/deny）才允许 CI-light-only 绿判 green。"""
    import re as _re
    global LIGHT_PATHS_RE
    if LIGHT_PATHS_RE is None:
        LIGHT_PATHS_RE = _re.compile(r"^(?:docs/|\.agents/|tools/).+|.+\.md$|^deny\.toml$")
    return bool(files) and all(LIGHT_PATHS_RE.match(f) for f in files)


def list_runs(branch: str, created_after: str | None = None, head_sha: str | None = None) -> list[dict]:
    cmd = ["gh", "run", "list", "--branch", branch, "--limit", "10",
           "--json", "databaseId,status,conclusion,createdAt,workflowName,headSha,event"]
    r = sh(cmd, timeout=60)
    if r.returncode != 0:
        return []
    runs = json.loads(r.stdout or "[]")
    if created_after:
        runs = [x for x in runs if (x.get("createdAt") or "") > created_after]
    if head_sha:
        # 精确 supersession（审计 r2）：按提交哈希过滤——免疫时钟偏斜与陈旧 run
        runs = [x for x in runs if (x.get("headSha") or "") == head_sha]
    return runs


def ensure_pr(branch: str) -> dict:
    """PR 先建后等（审计 r2 P0）：pull_request opened/synchronize 是产品改动在 wip/auto-*
    上的唯一重道触发面——落地后、等绿前建好 PR。幂等：已有 open PR 即返回。"""
    r = sh(["gh", "pr", "view", branch, "--json", "number,url"], timeout=60)
    if r.returncode == 0 and r.stdout.strip().startswith("{"):
        try:
            return json.loads(r.stdout)
        except json.JSONDecodeError:
            pass
    r = sh(["gh", "pr", "create", "--head", branch, "--base", "wip/sched-1.1",
            "--label", "hermes-auto", "--title", f"auto: {branch}",
            "--body", "E lane 自动提交——评审与 A: 决策行见后续补记。"], timeout=120)
    if r.returncode != 0:
        return {"error": (r.stderr or r.stdout or "")[-200:]}
    return {"url": r.stdout.strip()}


def wait_green(branch: str, created_after: str | None = None,
               poll: int = POLL_SECS, start_timeout: int = START_TIMEOUT,
               total_timeout: int = TOTAL_TIMEOUT, sleeper=time.sleep, clock=time.time,
               head_sha: str | None = None, light_only: bool = True) -> dict:
    """三态判决：green / red / note（无答）。

    supersession 双保险：created_after（兼容旧调用）+ head_sha 精确过滤（免疫时钟偏斜）。
    道校验（审计 r2）：非轻面 diff 上 CI-light-only 绿不判 green——须重道 run 或按触发面出 note。
    """
    t0 = clock()
    while True:
        runs = list_runs(branch, created_after, head_sha)
        done = [x for x in runs if x.get("status") == "completed"]
        if done:
            latest = max(done, key=lambda x: x.get("createdAt") or "")
            concl = latest.get("conclusion")
            names = sorted({x.get("workflowName") or "?" for x in done})
            if concl == "success":
                light_only_green = all("light" in (n or "").lower() for n in names)
                if not light_only and light_only_green:
                    # 混合/产品 diff 只有轻道绿——等重道（PR 面）；超时兜 note
                    if (clock() - t0) >= total_timeout:
                        return {"verdict": GITHUB_UNANSWERED, "why": "产品面无重道 run（触发面缺）", "workflows": names}
                else:
                    return {"verdict": "green", "run": latest["databaseId"], "workflows": names}
            if concl in ("failure", "cancelled"):
                # failure/cancelled 都是弱证据：有更新的未完 run 在场就不定谳（审计 r2：扩到 failure）
                newer_pending = [x for x in runs if x.get("status") != "completed"]
                if newer_pending:
                    pass
                else:
                    return {"verdict": "red", "run": latest["databaseId"], "conclusion": concl, "workflows": names}
        else:
            started = bool(runs)
            if not started and (clock() - t0) >= start_timeout:
                return {"verdict": GITHUB_UNANSWERED, "why": "45 分钟无相关 run 启动（触发面可能为空——PR 已建否？）"}
        if (clock() - t0) >= total_timeout:
            return {"verdict": GITHUB_UNANSWERED, "why": "3 小时未出结论"}
        sleeper(poll)


USAGE_PG = """用法：push-green.py [--branch <名>] [--poll <秒>] [--start-timeout <秒>] [--total-timeout <秒>] [--dry-run]
  PR 先建后等（审计 r2）：落地后先确保 open PR（hermes-auto，base wip/sched-1.1）再等绿——
  pull_request opened/synchronize 是产品改动在 wip/auto-* 上的唯一重道触发面。
  超时可经 BERRYOS_PG_POLL/PG_START_TIMEOUT/PG_TOTAL_TIMEOUT env 兜底。退出：0 绿/1 推失败/2 红/3 note。"""


def _env_int(name: str, default: int) -> int:
    try:
        return int(os.environ.get(name, "") or default)
    except ValueError:
        return default


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    # 严格 argv（审计 r2 P2：--help 曾触发对当前分支的真实推送）
    known = {"--branch", "--poll", "--start-timeout", "--total-timeout", "--dry-run"}
    for a in argv:
        if a.startswith("--") and a not in known:
            print(USAGE_PG, file=sys.stderr)
            return 2
    def arg_of(name: str, default: str) -> str:
        return argv[argv.index(name) + 1] if name in argv and argv.index(name) + 1 < len(argv) else default
    if any(a in argv for a in ("--help", "-h")):
        print(USAGE_PG)
        return 0
    branch = arg_of("--branch", "") or sh(["git", "rev-parse", "--abbrev-ref", "HEAD"]).stdout.strip()
    poll = int(arg_of("--poll", str(_env_int("BERRYOS_PG_POLL", POLL_SECS))))
    st_timeout = int(arg_of("--start-timeout", str(_env_int("BERRYOS_PG_START_TIMEOUT", START_TIMEOUT))))
    tot_timeout = int(arg_of("--total-timeout", str(_env_int("BERRYOS_PG_TOTAL_TIMEOUT", TOTAL_TIMEOUT))))
    debt = sh(["python3", "tools/review-debt.py", "check"])
    if debt.returncode != 0:
        print(json.dumps({"verdict": "review-debt", "why": "评审欠账在册——恢复后最旧优先补审再推（规则/16 §5）", "detail": debt.stdout.strip()[-200:]}, ensure_ascii=False))
        return 1
    if "--dry-run" in argv:
        print(json.dumps({"verdict": "dry-run", "branch": branch, "poll": poll, "start_timeout": st_timeout, "total_timeout": tot_timeout}, ensure_ascii=False))
        return 0
    import shutil as _sh
    if not _sh.which("gh"):
        print(json.dumps({"verdict": GITHUB_UNANSWERED, "why": "gh 不在场——等绿链无消费端"}, ensure_ascii=False))
        return 3
    diff_files = [ln for ln in sh(["git", "diff", "--name-only", "@{u}..HEAD"]).stdout.splitlines() if ln.strip()]
    light_only = diff_is_light_only(diff_files)
    push = subprocess.run(["git", "push", "origin", branch], cwd=str(ROOT),
                          capture_output=True, text=True, timeout=300)
    if push.returncode != 0:
        print(json.dumps({"verdict": "push-failed", "stderr": push.stderr[-400:]}, ensure_ascii=False))
        return 1
    if not landing_confirmed():
        print(json.dumps({"verdict": "not-landed", "why": "@{u}..HEAD 非空"}, ensure_ascii=False))
        return 1
    head_sha = sh(["git", "rev-parse", "HEAD"]).stdout.strip()
    pr = ensure_pr(branch)
    if "error" in pr:
        print(json.dumps({"verdict": GITHUB_UNANSWERED, "why": "PR 建立失败——产品改动将无重道触发面", "detail": pr["error"]}, ensure_ascii=False))
        return 3
    verdict = wait_green(branch, head_sha=head_sha, light_only=light_only,
                         poll=poll, start_timeout=st_timeout, total_timeout=tot_timeout)
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
    # headSha 精确过滤腿（审计 r2）
    sha_runs = [
        {"databaseId": 9, "status": "completed", "conclusion": "failure", "createdAt": "2026-11-22T05:00:00Z", "workflowName": "ci", "headSha": "oldsha", "event": "push"},
        {"databaseId": 10, "status": "completed", "conclusion": "success", "createdAt": "2026-11-22T06:00:00Z", "workflowName": "ci", "headSha": "newsha", "event": "push"},
    ]
    globals()["list_runs"] = lambda *a, **k: [x for x in sha_runs if x["headSha"] == "newsha"]
    v = wait_green("b", head_sha="newsha", sleeper=lambda s: None)
    check("headSha 精确 supersession（旧 sha 红不计）", v["verdict"] == "green")
    # 轻道假绿堵腿（审计 r2）：非轻面 diff + 仅 CI-light 绿 → 不判 green（超时面出 note）
    light_runs = [{"databaseId": 11, "status": "completed", "conclusion": "success", "createdAt": "2026-11-22T07:00:00Z", "workflowName": "CI-light", "headSha": "h2", "event": "push"}]
    calls2 = {"n": 0}
    def sleep2(_s):
        calls2["n"] += 1
        if calls2["n"] > 1:
            raise KeyboardInterrupt
    t0 = [1e10]
    globals()["list_runs"] = lambda *a, **k: light_runs
    try:
        v = wait_green("b", head_sha="h2", light_only=False, sleeper=sleep2, clock=lambda: t0[0])
    except KeyboardInterrupt:
        v = {"verdict": "escape"}
    check("混合 diff 轻道绿不判 green（等重道/超时 note）", v["verdict"] != "green")
    # 轻面分类腿
    check("diff 分类（tools=轻 / kernel=重）", diff_is_light_only(["tools/a.py", "docs/b.md"]) and not diff_is_light_only(["kernel/main.rs"]))
    globals()["list_runs"] = orig_list
    print(f"push-green 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}（supersession/红/取消弱证据/headSha/轻道假绿/分类）")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
