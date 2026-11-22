#!/usr/bin/env python3
"""claude-audit.py —— .claude 配置面登记审计（自动化开发二期 P2 治理四件；五向对拍）。

为什么：.claude/ 是 agent 的「神经接线」——hooks 改一行、settings 多一个文件，
行为就变了，而 git 对它的默许度极高（新文件静默入库、hook 事件增删无对账）。
2026-11-22 深研差距表 #8：配置面漂移必须靠登记+对拍，不靠记性。登记单源 =
.claude/MANIFEST.md（本审计的唯一事实对照面）。

五向对拍：
  ① MANIFEST 行集（`- `路径` — 用途` 行）
  ② git ls-files .claude 跟踪集
  ③ 磁盘 .claude 实存（除运行时面 state/、commands/、goals/、worktrees/、
     scheduled_tasks.lock——.gitignore 已排除，非登记面）
  ④ settings.json 引用的命令文件在盘（悬空引用=接线断头）
  ⑤ settings.json hook 事件覆盖面 == MANIFEST 该行 [hooks: ...] 标记

fail-direction（default-conservative）：未登记新跟踪文件=红；登记悬空=红；
hook 事件与描述不一致=红；MANIFEST 缺席=红（登记面自身没了就是最大漂移）；
磁盘多出的未跟踪文件=note 不红（多半是 gitignored 本地面，人看）。
git 不可用=红（审计门在提交/巡检路径跑，无对拍能力=拦）。

退出码：0=对拍齐；1=漂移（红）；2=用法错。
用法：python3 tools/claude-audit.py [--root <目录>]   # --root 供合成树自检
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Optional

ROOT = Path(__file__).resolve().parent.parent

# 运行时/本地面（.gitignore 对齐）：不入对拍——审计只看登记/跟踪面。
RUNTIME_EXCLUDE = {"state", "commands", "goals", "worktrees", "scheduled_tasks.lock"}

ENTRY_RE = re.compile(r"^-\s+`(?P<path>[^`]+)`", re.M)
HOOKS_MARK_RE = re.compile(r"\[hooks:\s*(?P<events>[^\]]+)\]")
SCRIPT_SUFFIXES = (".py", ".sh", ".mjs", ".js")


def parse_manifest(text: str) -> dict[str, Optional[set]]:
    """MANIFEST 文本 → {路径: hooks 事件集或 None}（None=该行无 [hooks: ...] 标记）。"""
    out: dict[str, Optional[set]] = {}
    for m in ENTRY_RE.finditer(text):
        rest = text[m.end():text.find("\n", m.end()) if text.find("\n", m.end()) != -1 else len(text)]
        hm = HOOKS_MARK_RE.search(rest)
        out[m.group("path")] = ({e.strip() for e in hm.group("events").split(",") if e.strip()}
                                if hm else None)
    return out


def disk_set(root: Path) -> set:
    """磁盘 .claude 实存（文件+符号链接，去运行时面）——skills 是符号链接，别漏。"""
    d = root / ".claude"
    if not d.is_dir():
        return set()
    out: set = set()
    for entry in sorted(d.iterdir()):
        if entry.name in RUNTIME_EXCLUDE:
            continue
        if entry.is_file() or entry.is_symlink():
            out.add(f".claude/{entry.name}")
    return out


def settings_refs(root: Path) -> tuple[Optional[set], list[str], Optional[str]]:
    """解析 settings.json → (hook 事件集, 引用的仓内脚本路径清单, 错误消息)。"""
    p = root / ".claude" / "settings.json"
    if not p.is_file():
        return None, [], None
    try:
        cfg = json.loads(p.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as e:
        return None, [], f"settings.json 解析失败：{e}"
    events: set = set()
    refs: list[str] = []
    hooks = cfg.get("hooks") or {}
    if isinstance(hooks, dict):
        for event, groups in hooks.items():
            events.add(event)
            if not isinstance(groups, list):
                continue
            for g in groups:
                for h in g.get("hooks", []) if isinstance(g, dict) else []:
                    cmd = h.get("command", "") if isinstance(h, dict) else ""
                    for tok in cmd.split():
                        # 启发式抽路径形 token：含 /、脚本后缀、非旗标——绝对路径与
                        # 仓外命令（system 命令无 /）不在此面。
                        if "/" in tok and tok.endswith(SCRIPT_SUFFIXES) and not tok.startswith("-"):
                            refs.append(tok)
    return events, refs, None


def audit(root: Path, tracked: Optional[set]) -> tuple[int, list[str]]:
    """核心：合成树/真仓同路（tracked 注入 None 时真跑 git ls-files）。返回 (rc, 行)。"""
    reds: list[str] = []
    notes: list[str] = []
    manifest_p = root / ".claude" / "MANIFEST.md"
    if not manifest_p.is_file():
        return 1, ["claude-audit: 红——MANIFEST 缺席（登记面自身没了是最大漂移，default-conservative）"]
    registered = parse_manifest(manifest_p.read_text(encoding="utf-8"))
    if not registered:
        reds.append("claude-audit: 红——MANIFEST 无登记行（空登记面对非空跟踪面=漂移）")

    if tracked is None:
        r = subprocess.run(["git", "ls-files", ".claude"], capture_output=True, text=True, cwd=str(root))
        if r.returncode != 0:
            reds.append("claude-audit: 红——git ls-files 不可用（审计门无对拍能力=拦，不静默放行）")
            tracked_git: set = set()
        else:
            tracked_git = {ln for ln in r.stdout.splitlines() if ln.strip()}
        tracked = tracked_git

    # ①×② 登记/跟踪互拍。
    for p in sorted(set(registered) - tracked):
        reds.append(f"claude-audit: 红——登记悬空：{p} 列入 MANIFEST 但 git 未跟踪（或已删）")
    for p in sorted(tracked - set(registered)):
        reds.append(f"claude-audit: 红——未登记：{p} 被跟踪但 MANIFEST 无行（default-conservative）")

    # ③ 磁盘实存：跟踪面缺席=红；未跟踪在场=note（多半 gitignored 本地面）。
    disk = disk_set(root)
    for p in sorted(tracked - disk):
        reds.append(f"claude-audit: 红——已跟踪但磁盘缺席：{p}")
    for p in sorted(disk - tracked - set(registered)):
        notes.append(f"claude-audit: note——未跟踪在场：{p}（多半是 gitignored 本地面，人看）")

    # ④⑤ settings.json 接线面（只在它被跟踪时才拍——未跟踪已由上面红）。
    if ".claude/settings.json" in tracked:
        events, refs, err = settings_refs(root)
        if err:
            reds.append(f"claude-audit: 红——{err}")
        else:
            for ref in refs:
                if not (root / ref).exists():
                    reds.append(f"claude-audit: 红——settings 引用悬空：{ref} 不在盘（接线断头）")
            mark = registered.get(".claude/settings.json")
            if mark is None:
                reds.append("claude-audit: 红——settings.json 行缺 [hooks: ...] 标记（⑤ 无对账面）")
            elif mark != events:
                reds.append(f"claude-audit: 红——hook 事件不一致：MANIFEST 标记 {sorted(mark)} "
                            f"≠ settings 实际 {sorted(events)}")
    lines = reds + notes
    if not reds:
        lines.append(f"claude-audit: 对拍齐——登记 {len(registered)} / 跟踪 {len(tracked)} / "
                     f"磁盘 {len(disk)}，hook 事件与引用面一致")
    return (1 if reds else 0), lines


def build_tree(td: Path, *, register: list[str], settings_body: Optional[str],
               extra_tracked_file: Optional[str] = None) -> None:
    """合成树构造器（自检腿共用）。register 形如 "路径" 或 "路径|mark"——带 |mark 的
    settings.json 行附 [hooks: PreToolUse,Stop] 标记（腿四用无标记形态验 ⑤ 红腿）。"""
    d = td / ".claude"
    d.mkdir(parents=True, exist_ok=True)
    lines = ["# 合成 MANIFEST（自检腿）", ""]
    for path in register:
        p, _, flag = path.partition("|")
        mark = " [hooks: PreToolUse,Stop]" if flag == "mark" else ""
        lines.append(f"- `{p}` — 合成用途{mark}")
    (d / "MANIFEST.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    if settings_body is not None:
        (d / "settings.json").write_text(settings_body, encoding="utf-8")
    (d / "skills").symlink_to("../.agents/skills", target_is_directory=True)
    tools = td / "tools"
    tools.mkdir(exist_ok=True)
    (tools / "e-guard.py").write_text("# 合成占位\n", encoding="utf-8")
    if extra_tracked_file:
        target = td / extra_tracked_file
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("x\n", encoding="utf-8")


SETTINGS_TMPL = """{
  "hooks": {
    "Stop": [
      { "hooks": [ { "type": "command", "command": "python3 tools/CMDNAME stop-hook", "timeout": 20 } ] }
    ],
    "PreToolUse": [
      { "matcher": "Edit|Write|MultiEdit",
        "hooks": [ { "type": "command", "command": "python3 tools/CMDNAME frozen-guard", "timeout": 20 } ] }
    ]
  }
}
"""


def settings_text(cmd: str) -> str:
    """合成 settings.json——用 replace 不用 format（JSON 花括号与 format 语法相撞）。"""
    return SETTINGS_TMPL.replace("CMDNAME", cmd)


def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"claude-audit 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    def run_audit(td: Path, tracked: list[str]) -> tuple[int, list[str]]:
        rc, lines = audit(td, tracked=set(tracked))
        return rc, lines

    base_register = [".claude/settings.json|mark", ".claude/skills", ".claude/MANIFEST.md"]
    base_tracked = [".claude/settings.json", ".claude/skills", ".claude/MANIFEST.md"]
    settings_ok = settings_text("e-guard.py")

    with tempfile.TemporaryDirectory() as td_s:
        td = Path(td_s)
        # 腿一：未登记红——extra.md 被跟踪但 MANIFEST 无行。
        build_tree(td, register=base_register, settings_body=settings_ok,
                   extra_tracked_file=".claude/extra.md")
        rc, lines = run_audit(td, base_tracked + [".claude/extra.md"])
        check("未登记新文件红（default-conservative）", rc == 1 and any("未登记" in l and "extra.md" in l for l in lines))
    with tempfile.TemporaryDirectory() as td_s:
        td = Path(td_s)
        # 腿二：登记齐过——三向一致、引用在盘、hooks 标记与实际事件集相等。
        build_tree(td, register=base_register, settings_body=settings_ok)
        rc, lines = run_audit(td, base_tracked)
        check("登记齐过（五向对拍全绿）", rc == 0 and any("对拍齐" in l for l in lines))
    with tempfile.TemporaryDirectory() as td_s:
        td = Path(td_s)
        # 腿三：settings 引用悬空红。
        build_tree(td, register=base_register, settings_body=settings_text("nope.py"))
        rc, lines = run_audit(td, base_tracked)
        check("settings 引用悬空红", rc == 1 and any("引用悬空" in l and "nope.py" in l for l in lines))
    with tempfile.TemporaryDirectory() as td_s:
        td = Path(td_s)
        # 腿四：hooks 标记不一致红（settings.json 行不带 |mark → 无 [hooks: ...] 标记）。
        no_mark = [".claude/settings.json"] + [p for p in base_register if p != ".claude/settings.json|mark"]
        build_tree(td, register=no_mark, settings_body=settings_ok)
        rc, lines = run_audit(td, base_tracked)
        check("hook 事件与 MANIFEST 标记不一致红", rc == 1 and any("事件不一致" in l or "缺 [hooks" in l for l in lines))
    print(f"claude-audit 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}")
    return 0 if fails == 0 else 1


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    root = ROOT
    if "--root" in argv:
        root = Path(argv[argv.index("--root") + 1]).resolve()
    rc, lines = audit(root, None)
    for l in lines:
        print(l)
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
