#!/usr/bin/env python3
"""e-doctor.py —— E lane 发射前确定性自检（自动化开发二期 P0；规则/16 §5）。

在 claude 加载上下文**之前**免费地失败：工具缺席/身份闸未配/钩子未装/上游未设/
FROZEN 件缺失，都硬退 1 让操作员在花钱前看到。只读，不修任何东西。
  python3 tools/e-doctor.py            # 全检（run-e.sh start 前置调用）
  python3 tools/e-doctor.py --selftest # 合成 PATH 夹具：缺件必红/全件必过
"""
from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

REQUIRED_TOOLS = ["tmux", "git", "python3", "node", "gh", "just", "cargo"]
REQUIRED_FILES = [
    "tools/run-e.sh", "tools/e-heartbeat.sh", "tools/e-supervisor.py",
    "tools/e-guard.py", "tools/pack-work.py", "tools/push-green.py",
    "tools/e-receipts.py", "tools/gate-wrap.sh", "tools/run-artifact.sh",
    "tools/review-debt.py", "tools/control-plane-gate.sh", "tools/control-plane.txt",
    ".claude/settings.json", ".githooks/pre-commit", ".githooks/pre-push",
    ".agents/skills/berryos-runner/SKILL.md",
]
# 远程四件（P0-3 加固）：lane 首跑会 fetch origin 后从集成分支建工作分支——
# origin 缺宪法/闸/清单即「发射先于推送」的红灯（审计 2026-11-22 楔死雷根治）。
REMOTE_MUST_HAVE = [
    ".agents/skills/berryos-runner/SKILL.md", "tools/e-guard.py",
    ".githooks/pre-commit", "tools/control-plane.txt",
    ".github/workflows/ci-light.yml",
]


def check(root: Path, path_override: str | None = None) -> tuple[int, list[str]]:
    fails: list[str] = []
    env_path = path_override or os.environ.get("PATH", "")
    for t in REQUIRED_TOOLS:
        if shutil.which(t, path=env_path) is None:
            fails.append(f"工具缺席：{t}")
    if shutil.which("claude", path=env_override_user(env_path)) is None:
        fails.append("工具缺席：claude（常在 ~/.local/bin——非登录 shell 的 PATH 会缺）")
    for f in REQUIRED_FILES:
        if not (root / f).is_file():
            fails.append(f"必备件缺席：{f}")
    r = subprocess.run(["git", "-C", str(root), "config", "--get", "berryos.bot-email"],
                       capture_output=True, text=True)
    if r.returncode != 0 or not r.stdout.strip():
        fails.append("身份闸未配：git config berryos.bot-email <bot 邮箱>（pre-push bot 面依赖）")
    r = subprocess.run(["git", "-C", str(root), "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
                       capture_output=True, text=True)
    if r.returncode != 0:
        fails.append("上游未设：git push -u origin <分支>")
    else:
        upstream = r.stdout.strip()
        for f in REMOTE_MUST_HAVE:
            rr = subprocess.run(["git", "-C", str(root), "cat-file", "-e", f"{upstream}:{f}"],
                                capture_output=True)
            if rr.returncode != 0:
                fails.append(f"远程缺纪律件：{upstream} 无 {f}——先推纪律线再发射（punch-list「纪律刀推送」条；lane 旧基无宪法/闸/触发面即楔死）")
    r = subprocess.run(["git", "-C", str(root), "config", "--get", "core.hooksPath"],
                       capture_output=True, text=True)
    if "githooks" not in (r.stdout or ""):
        fails.append("钩子未装：bash .githooks/install.sh（快照门/身份闸都不在位）")
    return len(fails), fails


def env_override_user(path: str) -> str:
    home = os.path.expanduser("~/.local/bin")
    return f"{home}:{path}"


def selftest() -> int:
    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        # 腿 1：空 PATH + 空树 → 必红（多腿齐触发）
        n, fails = check(t, path_override="/nonexistent")
        ok1 = n >= 2 and any("工具缺席" in f for f in fails) and any("必备件缺席" in f for f in fails)
        print(f"e-doctor 自检: {'腿绿' if ok1 else 'FAIL'} 缺件必红（{n} 项）")
        # 腿 2：合成全配仓 → 必过（不依赖主仓现场——主仓不设 bot 身份是正确形态）
        g = t / "provisioned"
        g.mkdir()
        subprocess.run(["git", "init", "-q", str(g)], capture_output=True)
        (g / ".claude").mkdir()
        (g / ".claude" / "settings.json").write_text("{}", encoding="utf-8")
        (g / ".githooks").mkdir()
        for f in ("pre-commit", "pre-push"):
            (g / ".githooks" / f).write_text("#!/bin/sh\n", encoding="utf-8")
        for f in REQUIRED_FILES:
            p = g / f
            p.parent.mkdir(parents=True, exist_ok=True)
            if not p.exists():
                p.write_text("x", encoding="utf-8")
        for f in REMOTE_MUST_HAVE:
            p = g / f
            p.parent.mkdir(parents=True, exist_ok=True)
            if not p.exists():
                p.write_text("x", encoding="utf-8")
        subprocess.run(["git", "-C", str(g), "add", "-A", ":!fakehome"], capture_output=True)
        for cmd in (
            ["config", "berryos.bot-email", "bot@x"],
            ["config", "core.hooksPath", ".githooks"],
        ):
            subprocess.run(["git", "-C", str(g), *cmd], capture_output=True)
        subprocess.run(["git", "-C", str(g), "commit", "--allow-empty", "-qm", "i"],
                       capture_output=True, env={"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@l",
                                                 "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@l",
                                                 "PATH": os.environ["PATH"], "HOME": os.environ.get("HOME", "")})
        br = subprocess.run(["git", "-C", str(g), "rev-parse", "--abbrev-ref", "HEAD"],
                            capture_output=True, text=True).stdout.strip()
        head = subprocess.run(["git", "-C", str(g), "rev-parse", "HEAD"],
                              capture_output=True, text=True).stdout.strip()
        subprocess.run(["git", "-C", str(g), "update-ref", f"refs/remotes/origin/{br}", head], capture_output=True)
        subprocess.run(["git", "-C", str(g), "config", "remote.origin.url", "https://example.invalid/x.git"], capture_output=True)
        subprocess.run(["git", "-C", str(g), "config", "remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"], capture_output=True)
        subprocess.run(["git", "-C", str(g), "config", f"branch.{br}.remote", "origin"], capture_output=True)
        subprocess.run(["git", "-C", str(g), "config", f"branch.{br}.merge", f"refs/heads/{br}"], capture_output=True)
        # 封闭化（审计 P0）：腿 2 不再用真 PATH——CI runner 无 tmux/claude/gh
        # 时此处必红（selftest_guard --run 接线面）。stub PATH 让本腿验的是
        # check() 的逻辑而非宿主舰队。
        stub_bin = t / "bin"
        stub_bin.mkdir()
        for tool in ("tmux", "gh", "claude", "just", "cargo"):
            (stub_bin / tool).write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            (stub_bin / tool).chmod(0o755)
        (t / "fakehome").mkdir()
        env2 = {**os.environ, "HOME": str(t / "fakehome")}
        orig_environ = os.environ.copy()
        os.environ.clear(); os.environ.update(env2)
        try:
            n2, fails2 = check(g, path_override=f"{stub_bin}:{os.environ.get('PATH', '')}")
        finally:
            os.environ.clear(); os.environ.update(orig_environ)
        ok2 = n2 == 0
        print(f"e-doctor 自检: {'腿绿' if ok2 else 'FAIL'} 合成全配仓必过（{'；'.join(fails2[:3])}）")
        rc = 0 if (ok1 and ok2) else 1
        print(f"e-doctor 自检：{'PASS' if rc == 0 else 'FAIL'}（缺件红 / 全配过 两腿）")
        return rc


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    n, fails = check(ROOT)
    if n:
        for f in fails:
            print(f"e-doctor FAIL: {f}", file=sys.stderr)
        print(f"e-doctor: FAIL（{n} 项）——发射前先修上列项（RUNBOOK §E lane 新建）", file=sys.stderr)
        return 1
    print("e-doctor: PASS（工具/必备件/身份闸/上游/钩子全在位——可以发射）")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
