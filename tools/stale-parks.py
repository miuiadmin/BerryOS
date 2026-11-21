#!/usr/bin/env python3
"""stale-parks.py —— 债网一：punch-list 停车理由失效检测（自动化开发二期 P2；advisory）。

为什么：park 语义（berryos-runner SKILL「park 语义」节）防的是 silently
defer-and-forget，但条目 park 上去后没人回头看——「等推送」的东西早推完了、
park 还挂着，就成了死账。本工具把「停车理由是否还成立」变成机器可判的形态：

  「等推送」「领先 origin」→ 查 @{u}..HEAD：空 = 等的东西已落地 = 理由失效可重开；
  「等 CI」→ 只查 gh 在场、不查结论（结论归 push-green 的三态判决面，本工具不
    重复判 CI）——gh 缺席 = 本环境永远等不到 CI 可见性，报失效提请人裁（换环境
    或改理由），不是自动重开；
  其余理由（等人审/等裁决……）机器不可判，不报——宁可漏报不误报：advisory 面
    漏一条只是多挂一天，误报一条会骗人重开不该开的。

fail-direction：rc 恒 0（有发现也 0，advisory——输出清单给人看，重开动作永远
是人做的）；只有用法错才 2。环境缺席（无 upstream / punch 文件不在）= note
不 = 红（深研公理：基础设施无答不归责）。

用法：
  python3 tools/stale-parks.py [--punch <路径>]   # 默认 .agents/plans/2026-11-22-e面运营.md
  python3 tools/stale-parks.py --selftest          # 合成夹具，零 git/零网络
"""
from __future__ import annotations

import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Callable, Optional

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_PUNCH = ROOT / ".agents" / "plans" / "2026-11-22-e面运营.md"

# park 条目形态（宪法钦定）：`- [ ] [PARKED→人审: <一句理由>]`。全文件扫而不只扫
# 「未完成事项」节——park 是欠账，搬节不算清账（e-guard 的 open-items oracle 只看
# 该节，但那是「还有没有活」的判据；本工具是「挂账还成立吗」的判据，面更宽）。
PARKED_RE = re.compile(r"^(?P<line>-\s\[ \]\s*\[PARKED→人审:\s*(?P<reason>[^\]]+)\].*)$", re.M)

# 机器可判的理由形态（命中词 → 形态名）。「等CI」无空格形态一并收。
PUSH_FORMS = ("等推送", "领先 origin", "领先origin")
CI_FORMS = ("等 CI", "等CI")


def git_state(git: Callable[[list[str]], "subprocess.CompletedProcess[str]"]) -> Optional[dict]:
    """真模式探针：返回 {upstream: bool, ahead: int}；git 不可用返回 None（note 面）。"""
    try:
        r = git(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
    except Exception:
        return None
    upstream = r.returncode == 0 and bool(r.stdout.strip())
    ahead = 0
    if upstream:
        r = git(["rev-list", "--count", "@{u}..HEAD"])
        if r.returncode == 0 and r.stdout.strip().isdigit():
            ahead = int(r.stdout.strip())
    return {"upstream": upstream, "ahead": ahead}


def gh_present(which: Callable[[str], Optional[str]] = shutil.which) -> bool:
    """「等 CI」腿只查 gh 在场不查结论——不跑 gh 任何网络命令。"""
    return which("gh") is not None


def evaluate(text: str, state: Optional[dict], gh: bool) -> list[str]:
    """核心：punch 文本 + 注入的 git/gh 状态 → 发现清单（str 列表）。纯函数，自检打这里。"""
    findings: list[str] = []
    for m in PARKED_RE.finditer(text):
        reason = m.group("reason").strip()
        line = m.group("line")
        if any(f in reason for f in PUSH_FORMS):
            if state is None:
                findings.append(f"[note] {reason} —— git 状态不可查（环境缺席不归责，人核）")
            elif not state.get("upstream"):
                findings.append(f"[note] {reason} —— 无 upstream，机器不可判（人核）")
            elif state["ahead"] == 0:
                findings.append(f"[失效可重开] {reason} —— @{{u}}..HEAD 已空，等的东西落地了：{line}")
            # ahead > 0：理由仍成立，不报（漏报优于误报）。
        elif any(f in reason for f in CI_FORMS):
            if not gh:
                findings.append(
                    f"[失效-需人裁] {reason} —— gh 缺席，本环境永远等不到 CI 可见性"
                    f"（重开或换环境再审）：{line}")
            # gh 在场：不判结论（push-green 的面），不报。
        # 其余形态机器不可判，不报。
    return findings


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    punch = DEFAULT_PUNCH
    if "--punch" in argv:
        punch = Path(argv[argv.index("--punch") + 1])
    if not punch.is_file():
        print(f"stale-parks: punch 文件不在（{punch}）——无可扫面，advisory 直接过", file=sys.stderr)
        return 0
    text = punch.read_text(encoding="utf-8")

    def git(argv2: list[str]) -> "subprocess.CompletedProcess[str]":
        return subprocess.run(["git", *argv2], capture_output=True, text=True, cwd=str(ROOT))

    state = git_state(git)
    findings = evaluate(text, state, gh_present())
    for f in findings:
        print(f"stale-parks: {f}")
    n_stale = sum(1 for f in findings if f.startswith("[失效"))
    print(f"stale-parks: 扫描完成——失效 {n_stale} 条 / note "
          f"{len(findings) - n_stale} 条（advisory，rc 恒 0；重开由人改 punch-list）")
    return 0


def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"stale-parks 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    # 抽取腿：parked 计入、普通开放/已完成/行内提及 PARKED 的非条目行不计。
    sample = (
        "## 未完成事项\n"
        "- [ ] 普通开放\n"
        "- [ ] [PARKED→人审: 等推送，领先 origin 若干] 挂账A\n"
        "- [ ] [PARKED→人审: 等 CI 绿] 挂账B\n"
        "- [ ] [PARKED→人审: 等人审裁决] 挂账C\n"
        "- [x] [PARKED→人审: 等推送] 已勾掉的不算\n"
        "正文里提到 [PARKED→人审: 等推送] 不算（非行首条目）\n")
    found = evaluate(sample, {"upstream": True, "ahead": 3}, gh=True)
    check("抽取（ahead=3 时三条机器形态都不误报失效）", len([f for f in found if f.startswith("[失效")]) == 0)
    # 腿一：等推送 + ahead=0 → 失效。
    found = evaluate(sample, {"upstream": True, "ahead": 0}, gh=True)
    stale = [f for f in found if f.startswith("[失效")]
    check("等推送 + @{u}..HEAD 空 → 报失效可重开", len(stale) == 1 and "挂账A" in stale[0])
    # 腿二：等推送 + ahead>0 → 不报失效（仍成立）。
    found_ahead = evaluate(sample, {"upstream": True, "ahead": 3}, gh=True)
    check("等推送 + 仍有未推提交 → 不报", not any("挂账A" in f and f.startswith("[失效") for f in found_ahead))
    # 腿三：等 CI + gh 缺席 → 报失效-需人裁；gh 在场 → 不报。
    found_no_gh = evaluate(sample, {"upstream": True, "ahead": 9}, gh=False)
    check("等 CI + gh 缺席 → 报失效-需人裁", sum(1 for f in found_no_gh if "挂账B" in f and f.startswith("[失效")) == 1)
    check("等 CI + gh 在场 → 不判结论不报", not any("挂账B" in f and f.startswith("[失效") for f in found))
    # 腿四：git 探针缺席 / 无 upstream → note 不红（fail-open）。
    check("git 不可用 → note 不误报失效", all(not f.startswith("[失效") for f in evaluate(sample, None, gh=True)))
    check("无 upstream → note 不误报失效",
          all(not f.startswith("[失效") for f in evaluate(sample, {"upstream": False, "ahead": 0}, gh=True)))
    print(f"stale-parks 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
