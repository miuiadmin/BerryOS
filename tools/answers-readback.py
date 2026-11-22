#!/usr/bin/env python3
"""answers-readback.py —— 决策账本机器读回（自动化开发二期 P2；advisory）。

为什么：runner 宪法（berryos-runner SKILL「decide-then-report」）要求 PR body 落
`A: (run decision, YYYY-MM-DD) <选择>. Reversal: <反转代价一句>` 决策行——但答案
落了盘没人读回来：owner 已在 merged PR 里裁决过的主题，punch-list 里同主题的
[PARKED→人审] 条目还挂着等审。本工具把「裁决已落」变成机器可判：

  抽取：body 里全部 `A: (run decision, 日期) 选择. Reversal: 代价` 行 → 已裁决集；
  对照：punch-list 的 [PARKED→人审: ...] 条目文本与裁决文本共享 ≥4 字连续片段
        （同主题词）→ 报「裁决已落可重开」；
  统计：采纳/否决计数——来源 body 含「否决/回滚」字样计否决（4-gram 取
        [CJK/字母/数字] 连续段并去空白：格式前缀不参与匹配，标点与全半角空格
        不制造假共享——「等推送」这类通用短语只有 3 字，不够 4 字门槛，不误命中）。

fail-direction：advisory——rc 恒 0（清单给人看，重开由人做）；gh 不可用 → note
后 rc 0（环境缺席不归责）；用法错才 2。宁可漏报不误报：匹配门槛 4 字连续段，
共享「run decision」类格式词不可能出现（已被行格式剥掉）。

用法：
  python3 tools/answers-readback.py [--fixture <JSON文件>] [--punch <路径>]
    # 无 --fixture 时真跑：gh pr list --label hermes-auto --state merged
    #   --json number,title,body（只在主模式；--selftest 零网络）
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Optional

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_PUNCH = ROOT / ".agents" / "plans" / "2026-11-22-e面运营.md"
GH_CMD = ["gh", "pr", "list", "--label", "hermes-auto", "--state", "merged",
          "--json", "number,title,body"]

PARKED_RE = re.compile(r"^(?P<line>-\s\[ \]\s*\[PARKED→人审:\s*(?P<reason>[^\]]+)\].*)$", re.M)
DECISION_RE = re.compile(
    r"^A: \(run decision,\s*(?P<date>[^)]+)\)\s*(?P<choice>.+?)\.\s*Reversal:\s*(?P<rev>.+)$", re.M)
REJECT_WORDS = ("否决", "回滚")
RUN_CH = re.compile(r"[一-鿿A-Za-z0-9]+")


def normalize(s: str) -> str:
    """匹配面归一：去全部空白 + ASCII 小写——全半角空格与大小写不制造/破坏共享段。"""
    return "".join(s.split()).lower()


def fourgrams(s: str) -> set[str]:
    """归一文本的 4 字连续段集合（只取 CJK/字母/数字段——标点不参与共享判定）。"""
    grams: set[str] = set()
    for run in RUN_CH.findall(s):
        low = run.lower()
        for i in range(len(low) - 3):
            grams.add(low[i:i + 4])
    return grams


def extract_decisions(prs: list[dict]) -> list[dict]:
    """PR JSON 列表 → 决策行列表（带来源 body 供否决计数、带 title 参与匹配）。"""
    out: list[dict] = []
    for pr in prs:
        body = pr.get("body") or ""
        for m in DECISION_RE.finditer(body):
            out.append({
                "number": pr.get("number"),
                "title": pr.get("title") or "",
                "choice": m.group("choice").strip(),
                "rev": m.group("rev").strip(),
                "rejected": any(w in body for w in REJECT_WORDS),
                # 匹配面：题目带主题词，选择是裁决本体——两者并集参与 4-gram 对照。
                "match_text": normalize(f"{pr.get('title') or ''} {m.group('choice')}"),
            })
    return out


def match_parks(punch_text: str, decisions: list[dict]) -> list[tuple[str, dict]]:
    """punch 文本 × 已裁决集 → （park 条目行, 命中的决策）列表。"""
    hits: list[tuple[str, dict]] = []
    for pm in PARKED_RE.finditer(punch_text):
        line = pm.group("line")
        grams = fourgrams(line)
        for d in decisions:
            if any(g in d["match_text"] for g in grams):
                hits.append((line, d))
                break  # 一条 park 命中一次即够，报最前（本工具不排序裁决优先级）
    return hits


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    punch = DEFAULT_PUNCH
    if "--punch" in argv:
        punch = Path(argv[argv.index("--punch") + 1])
    prs: Optional[list[dict]] = None
    if "--fixture" in argv:
        try:
            prs = json.loads(Path(argv[argv.index("--fixture") + 1]).read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as e:
            print(f"answers-readback: 用法错——fixture 读不进（{e}）", file=sys.stderr)
            return 2
    else:
        try:
            r = subprocess.run(GH_CMD, capture_output=True, text=True, timeout=120, cwd=str(ROOT))
        except (OSError, subprocess.TimeoutExpired):
            r = None
        if r is None or r.returncode != 0:
            print("answers-readback: gh 不可用/失败——决策面缺席，advisory 放行（fail-open，rc 0）", file=sys.stderr)
            prs = []
        else:
            try:
                prs = json.loads(r.stdout)
            except json.JSONDecodeError:
                print("answers-readback: gh 输出非 JSON——决策面缺席，advisory 放行", file=sys.stderr)
                prs = []
    if not punch.is_file():
        print(f"answers-readback: punch 文件不在（{punch}）——无对照面，advisory 直接过", file=sys.stderr)
        return 0

    decisions = extract_decisions(prs)
    hits = match_parks(punch.read_text(encoding="utf-8"), decisions)
    n_rej = sum(1 for d in decisions if d["rejected"])
    for line, d in hits:
        tag = "否决" if d["rejected"] else "采纳"
        print(f"answers-readback: [裁决已落可重开|{tag}] PR#{d['number']} "
              f"「{d['choice']}」 已裁决，park 可重开：{line}")
    print(f"answers-readback: 决策行 {len(decisions)} 条（采纳 {len(decisions) - n_rej} / 否决 {n_rej}）；"
          f"park 命中 {len(hits)} 条（advisory，rc 恒 0；重开由人改 punch-list）")
    return 0


def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"answers-readback 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    fixture = [{
        "number": 31, "title": "输入楔死修复：stale-reset 自愈",
        "body": ("任务：修输入楔死。改动：四面修复。\n"
                 "A: (run decision, 2026-11-20) 采纳 stale-reset 自愈方案，回滚帽两次. Reversal: 撤回自愈保留观测\n"
                 "A: (run decision, 2026-11-21) 采纳 tally 观测面. Reversal: 删 tally 探针\n"),
    }, {
        "number": 32, "title": "无关主题",
        "body": "A: (run decision, 2026-11-21) 采纳图标直进方案. Reversal: 还原脚本\n",
    }]
    # 腿一：抽取——两条 body 共 3 条决策行，格式前缀与 Reversal 段不混入 choice。
    ds = extract_decisions(fixture)
    check("抽取（3 条决策行，choice 干净）",
          len(ds) == 3 and ds[0]["choice"].startswith("采纳 stale-reset")
          and ds[1]["choice"] == "采纳 tally 观测面")
    punch = (
        "## 未完成事项\n"
        "- [ ] [PARKED→人审: 等输入楔死修复裁决] 挂账A\n"      # 与 #31 title 共享「输入楔死修复」
        "- [ ] [PARKED→人审: 等tally观测面定案] 挂账B\n"        # 与 #31 choice 共享「tally观测面」（去空白后命中）
        "- [ ] [PARKED→人审: 等钱包件设计裁决] 挂账C\n"         # 无共享主题词
        "- [ ] 普通开放条目不算\n")
    hits = match_parks(punch, ds)
    # 腿二：匹配——A/B 命中、C 不命中；普通条目不进对照面。
    hit_lines = [h[0] for h in hits]
    check("匹配（同主题 ≥4 字命中，无主题不命中，非 park 不进）",
          sum(1 for l in hit_lines if "挂账A" in l) == 1
          and sum(1 for l in hit_lines if "挂账B" in l) == 1
          and not any("挂账C" in l or "普通开放" in l for l in hit_lines))
    # 腿三：统计——#31 body 含「回滚」→ 其 2 条决策计否决；#32 计采纳。
    n_rej = sum(1 for d in ds if d["rejected"])
    check("统计（含否决/回滚字样的 body 计否决）", n_rej == 2 and len(ds) - n_rej == 1)
    print(f"answers-readback 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
