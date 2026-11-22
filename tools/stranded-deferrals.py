#!/usr/bin/env python3
"""stranded-deferrals.py —— 债网二：搁浅引用检测（自动化开发二期 P2；advisory）。

为什么：punch-list 条目常以「等某对象」挂账——「→ #<PR号>」（等评审/等合并）
「commit <hex>」（等某提交落库）。等的东西已经来了而条目还挂着，就是搁浅
（stranded deferral）：不是没人干活，是没人回头看。本工具把「引用对象到了没」
变成机器可判：

  → #<PR号>  ：gh pr view <N> --json state → state=MERGED = 已合入 = 搁浅（报）；
               OPEN/CLOSED 不报（CLOSED 未合=等待理由另有下文，人看）。
  commit <hex>：git merge-base --is-ancestor <hex> HEAD → 已是 HEAD 祖先 = 在当前
               开发线 = 搁浅（报）。判据收窄到 HEAD 而非 git branch --contains 的
               任意分支——同分支自引会全员假阳性（等的东西常就在自己分支上，等的
               是它进主线/进本线，含于 HEAD 才算「到了」）。

fail-direction：gh 不可用 → 该条跳过并注明（fail-open——环境缺席不误报搁浅；
深研公理：基础设施无答=note 不=红）。commit 对象不存在 → note（引用悬空，可能
还没推上来，同样不误报）。advisory——rc 恒 0，重开/清账由人做；用法错才 2。

用法：
  python3 tools/stranded-deferrals.py [--punch <路径>]   # 默认 .agents/plans/2026-11-22-e面运营.md
  python3 tools/stranded-deferrals.py --selftest          # 注入 JSON 夹具，零 gh/零 git/零网络
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path
from typing import Callable, Optional

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_PUNCH = ROOT / ".agents" / "plans" / "2026-11-22-e面运营.md"

# 扫全部开放条目（不限 [PARKED]——普通条目也会引用等待对象；欠账面比 park 面宽）。
ITEM_RE = re.compile(r"^(?P<line>- \[ \].*)$", re.M)
PR_REF_RE = re.compile(r"#(\d+)")
COMMIT_REF_RE = re.compile(r"\bcommit\s+([0-9a-f]{7,40})\b", re.I)

# 探针签名（依赖注入面，自检不碰真 gh/git）：
#   pr_state(n)  -> "MERGED"/"OPEN"/"CLOSED"/None（None=不可查，跳过）
#   commit_in_head(hex) -> True（HEAD 祖先）/ False（在库但未进 HEAD）/ None（对象不存在）


def real_pr_state(n: str) -> Optional[str]:
    """真模式探针：gh 缺席/失败 → None（fail-open：环境缺席不误报）。"""
    try:
        r = subprocess.run(["gh", "pr", "view", n, "--json", "state"],
                           capture_output=True, text=True, timeout=60, cwd=str(ROOT))
    except (OSError, subprocess.TimeoutExpired):
        return None
    if r.returncode != 0:
        return None
    import json
    try:
        return json.loads(r.stdout).get("state")
    except json.JSONDecodeError:
        return None


def real_commit_in_head(hexoid: str) -> Optional[bool]:
    """真模式探针：HEAD 祖先 → True；在库未进 HEAD → False；对象不存在 → None。"""
    def git(*args: str) -> subprocess.CompletedProcess:
        return subprocess.run(["git", *args], capture_output=True, text=True, cwd=str(ROOT))
    if git("merge-base", "--is-ancestor", hexoid, "HEAD").returncode == 0:
        return True
    if git("cat-file", "-e", f"{hexoid}^{{commit}}").returncode == 0:
        return False
    return None


def evaluate(text: str,
             pr_state: Callable[[str], Optional[str]],
             commit_in_head: Callable[[str], Optional[bool]]) -> tuple[list[str], list[str]]:
    """核心：punch 文本 + 注入探针 → （搁浅发现, note）。纯函数，自检打这里。"""
    findings: list[str] = []
    notes: list[str] = []
    for m in ITEM_RE.finditer(text):
        line = m.group("line")
        for n in PR_REF_RE.findall(line):
            state = pr_state(n)
            if state is None:
                notes.append(f"gh 不可查，跳过：#{n}（fail-open，不误报）——{line}")
            elif state == "MERGED":
                findings.append(f"[搁浅] 等的 #{n} 已 MERGED——等的东西来了，条目该动了：{line}")
            # OPEN/CLOSED：等待仍在进行或另有下文，不报。
        for hexoid in set(COMMIT_REF_RE.findall(line)):
            got = commit_in_head(hexoid)
            if got is None:
                notes.append(f"commit {hexoid} 对象不存在（引用悬空或未推上来，不误报）——{line}")
            elif got is True:
                findings.append(f"[搁浅] commit {hexoid} 已是 HEAD 祖先——等的东西到了：{line}")
            else:
                notes.append(f"commit {hexoid} 在库但未进 HEAD（等待仍成立）——{line}")
    return findings, notes


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    punch = DEFAULT_PUNCH
    if "--punch" in argv:
        punch = Path(argv[argv.index("--punch") + 1])
    if not punch.is_file():
        print(f"stranded-deferrals: punch 文件不在（{punch}）——无可扫面，advisory 直接过", file=sys.stderr)
        return 0
    findings, notes = evaluate(punch.read_text(encoding="utf-8"), real_pr_state, real_commit_in_head)
    for f in findings:
        print(f"stranded-deferrals: {f}")
    for n in notes:
        print(f"stranded-deferrals: {n}")
    print(f"stranded-deferrals: 扫描完成——搁浅 {len(findings)} 条 / note {len(notes)} 条"
          f"（advisory，rc 恒 0；清账由人改 punch-list）")
    return 0


def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"stranded-deferrals 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    sample = (
        "## 未完成事项\n"
        "- [ ] 等 #101 评审后收口\n"
        "- [ ] 等 #202 评审后收口\n"
        "- [ ] [PARKED→人审: 等 #303 合并] 挂账\n"
        "- [ ] 等 commit aa11bb22cc3 落库再动\n"
        "- [ ] 等 commit 00000000000 落库再动\n"
        "- [x] 已完成项不算（含 #404）\n"
        "正文提及 #505 与 commit ff11bb22cc3 不算（非条目行）\n")

    def fixture_pr(n: str) -> Optional[str]:
        return {"101": "MERGED", "202": "OPEN", "303": "MERGED"}.get(n)

    def fixture_commit(hexoid: str) -> Optional[bool]:
        return {"aa11bb22cc3": True, "00000000000": None}.get(hexoid, False)

    findings, notes = evaluate(sample, fixture_pr, fixture_commit)
    # 腿一：已合入报（#101、#303 两处；#404 不在探针表=None→跳过不算）。
    merged_hits = [f for f in findings if "#101" in f or "#303" in f]
    check("已合入报（普通条目与 park 条目都扫）", len(merged_hits) == 2)
    # 腿二：开放不报。
    check("OPEN 不报", not any("#202" in f for f in findings))
    # 腿三：gh 缺席（探针恒 None）→ 全跳过并注明。
    f_none, n_none = evaluate(sample, lambda n: None, fixture_commit)
    check("gh 缺席跳过全部 PR 引用并注明（fail-open；commit 面不受影响）",
          not any("#" in f for f in f_none) and sum(1 for x in n_none if "跳过" in x) == 3)
    # 腿四：commit 已是 HEAD 祖先 → 搁浅；对象不存在 → note。
    check("commit HEAD 祖先报搁浅", sum(1 for f in findings if "aa11bb22cc3" in f) == 1)
    check("commit 对象不存在 note 不报", any("00000000000" in x and "悬空" in x for x in notes)
          and not any("00000000000" in f for f in findings))
    # 腿五：非条目行（已完成/正文）不扫。
    check("非开放条目行不扫", not any("#505" in f or "ff11bb22cc3" in f for f in findings + notes))
    print(f"stranded-deferrals 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
