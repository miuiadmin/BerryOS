#!/usr/bin/env python3
"""review-debt.py —— 评审欠账记账面（自动化开发二期 P1；规则/16 §5）。

评审 lane 宕机/跳过节不是把评审丢掉而是欠着：欠账五件套 = 记账(record) →
禁推(check 供推送闸消费，有欠账 rc=1) → 恢复后最旧优先补审(list 首条) →
清账(clear) → 推送闸接线（push 面归主会话，接线命令见交付说明）。

  python3 tools/review-debt.py record <节标识> [--note <一句>] [--base <分支>] [--state <文件>]
  python3 tools/review-debt.py list    [--state <文件>]
  python3 tools/review-debt.py clear <id> [--state <文件>]
  python3 tools/review-debt.py check   [--state <文件>]
存储：.claude/state/review-debt.json —— {"next_id":N,"debts":[{id,section,ts,base,note}]}。
原子写（tmp + os.replace）：推送闸的 check 与 record 并发时，半写 JSON 会把账本
打成不可解析，等于账面凭空清零（假绿）。

fail-direction（哪边失败是安全边）：
  - 账本坏（非法 JSON）= rc2 loud，不许把坏账本当无账重置——清零 = 假绿 = 安全边破。
  - 账本缺席 = 无账（合法首跑）。
  - base 探测失败（非 git 树/分离 HEAD）记 "unknown" 也照记——记账不许因环境
    缺席而丢账；欠账的真相在「有一笔」，不在 base 精确。
  - check 有欠账 = rc1（保守边：有疑即禁推）。

rc：0 正常；1 check 有欠账（推送闸消费）；2 用法错/账本坏/清不存在的账。
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import time
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_STATE = ROOT / ".claude" / "state" / "review-debt.json"


class DebtStateError(Exception):
    """账本坏——loud 红，不静默重置。"""


def load(state_path: Path) -> dict:
    if not state_path.is_file():
        return {"next_id": 1, "debts": []}
    try:
        d = json.loads(state_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as e:
        raise DebtStateError(f"账本坏（{state_path}: {e}）——不许静默重置，人工修后再跑")
    if not isinstance(d, dict) or not isinstance(d.get("debts"), list) \
            or not isinstance(d.get("next_id"), int):
        raise DebtStateError(f"账本结构不对（{state_path}）——须为 {{next_id, debts}}")
    return d


def save(state_path: Path, d: dict) -> None:
    state_path.parent.mkdir(parents=True, exist_ok=True)
    tmp = state_path.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(d, ensure_ascii=False, indent=1), encoding="utf-8")
    os.replace(tmp, state_path)


def detect_base() -> str:
    """调用方 cwd 的当前分支；探测失败记 unknown（丢 base 不丢账）。"""
    r = subprocess.run(["git", "rev-parse", "--abbrev-ref", "HEAD"],
                       capture_output=True, text=True)
    branch = r.stdout.strip() if r.returncode == 0 else ""
    if not branch or branch == "HEAD":  # 分离 HEAD 也算探测失败
        print("review-debt: warning: base 探测失败（非 git 树或分离 HEAD），记 unknown", file=sys.stderr)
        return "unknown"
    return branch


def _fmt(d: dict) -> str:
    ts = datetime.fromtimestamp(d["ts"]).isoformat(timespec="seconds")
    line = f"#{d['id']}\t{ts}\tbase={d['base']}\t{d['section']}"
    if d.get("note"):
        line += f"\tnote={d['note']}"
    return line


def ordered_debts(d: dict) -> list:
    return sorted(d["debts"], key=lambda x: (x.get("ts", 0), x.get("id", 0)))


def _usage() -> int:
    print("usage: review-debt.py record <节标识> [--note <一句>] [--base <分支>] [--state <文件>]\n"
          "       review-debt.py list [--state <文件>]\n"
          "       review-debt.py clear <id> [--state <文件>]\n"
          "       review-debt.py check [--state <文件>]\n"
          "       review-debt.py --selftest", file=sys.stderr)
    return 2


def main(argv: list) -> int:
    if "--selftest" in argv:
        return selftest()
    if not argv:
        return _usage()
    cmd = argv.pop(0)
    state_path = DEFAULT_STATE
    base = None
    note = ""
    pos: list = []
    while argv:
        a = argv.pop(0)
        if a == "--state" and argv:
            state_path = Path(argv.pop(0))
        elif a == "--base" and argv:
            base = argv.pop(0)
        elif a == "--note" and argv:
            note = argv.pop(0)
        elif a.startswith("--"):
            return _usage()
        else:
            pos.append(a)
    try:
        d = load(state_path)
    except DebtStateError as e:
        print(f"review-debt: {e}", file=sys.stderr)
        return 2

    if cmd == "record":
        if len(pos) != 1 or not pos[0].strip():
            print("review-debt: record 需要非空 <节标识>", file=sys.stderr)
            return 2
        base_v = base if base is not None else detect_base()
        d["debts"].append({"id": d["next_id"], "section": pos[0].strip(),
                           "ts": time.time(), "base": base_v, "note": note})
        d["next_id"] += 1
        save(state_path, d)
        print(f"review-debt: 已记 #{d['next_id'] - 1}（{pos[0].strip()}，base={base_v}）——禁推至清账")
        return 0

    if cmd == "list":
        rows = ordered_debts(d)
        if not rows:
            print("无评审欠账")
        for row in rows:
            print(_fmt(row))
        return 0

    if cmd == "clear":
        if len(pos) != 1 or not pos[0].lstrip("#").isdigit():
            print("review-debt: clear 需要 <id>（数字）", file=sys.stderr)
            return 2
        cid = int(pos[0].lstrip("#"))
        before = len(d["debts"])
        d["debts"] = [x for x in d["debts"] if x.get("id") != cid]
        if len(d["debts"]) == before:
            # 清不存在的账不许静默成功：可能是 id 记错或已被并发会话清掉——
            # 前者要人看，后者 list 即对账，都该 loud。
            print(f"review-debt: #{cid} 不在账上（未动账面；先 list 对账）", file=sys.stderr)
            return 2
        save(state_path, d)
        print(f"review-debt: #{cid} 已清（剩 {len(d['debts'])} 笔）")
        return 0

    if cmd == "check":
        rows = ordered_debts(d)
        if not rows:
            print("无评审欠账")
            return 0
        for row in rows:
            print(_fmt(row))
        first = rows[0]
        print(f"review-debt: check 红——{len(rows)} 笔评审欠账在案，禁推；"
              f"恢复后最旧优先补审：#{first['id']} {first['section']}", file=sys.stderr)
        return 1

    return _usage()


# ---------------------------------------------------------------------------
# selftest：合成态（--state 指到临时目录）+ 合成 git 仓夹具。零网络零真仓写。
# ---------------------------------------------------------------------------

def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"review-debt 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    def run(args: list, cwd=None):
        return subprocess.run([sys.executable, str(Path(__file__).resolve()), *args],
                              capture_output=True, text=True, cwd=cwd)

    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        state = t / "review-debt.json"

        # ① 记两笔（显式 base）
        r = run(["record", "节甲", "--base", "wip/sched-1.1", "--state", str(state)])
        check("record 甲 rc0", r.returncode == 0)
        r = run(["record", "节乙", "--base", "wip/sched-1.1", "--state", str(state)])
        check("record 乙 rc0", r.returncode == 0)
        # ② list 最旧优先
        r = run(["list", "--state", str(state)])
        out = r.stdout
        check("list 两笔最旧优先", r.returncode == 0 and "#1" in out and "#2" in out
              and out.index("节甲") < out.index("节乙"))
        # ③ check 有账 rc1（含最旧提示）
        r = run(["check", "--state", str(state)])
        check("check 有欠账 rc1", r.returncode == 1 and "最旧优先" in r.stderr and "节甲" in r.stdout)
        # ④ clear 一笔 + 清不存在 id 红且账面不变
        r = run(["clear", "1", "--state", str(state)])
        check("clear 1 rc0", r.returncode == 0)
        before = state.read_bytes()
        r = run(["clear", "999", "--state", str(state)])
        check("清不存在 id rc2 且账面不变", r.returncode == 2 and state.read_bytes() == before)
        r = run(["list", "--state", str(state)])
        check("清后 list 只剩乙", "节乙" in r.stdout and "节甲" not in r.stdout)
        # ⑤ 清空后 check rc0
        r = run(["clear", "#2", "--state", str(state)])
        r2 = run(["check", "--state", str(state)])
        check("清空后 check rc0", r.returncode == 0 and r2.returncode == 0 and "无评审欠账" in r2.stdout)
        # ⑥ id 单调不回退（next_id 不因清账回卷）
        r = run(["record", "节丙", "--base", "b", "--state", str(state)])
        r2 = run(["list", "--state", str(state)])
        check("id 单调（清账后新账取 #3）", r.returncode == 0 and "#3" in r2.stdout)
        # ⑦ base 自动探测：合成 git 仓夹具（先提交再切分支，防分离/未生 HEAD）
        repo = t / "repo"
        repo.mkdir()
        subprocess.run(["git", "-C", str(repo), "init", "-q"], capture_output=True)
        (repo / "f.txt").write_text("x", encoding="utf-8")
        subprocess.run(["git", "-C", str(repo), "add", "."], capture_output=True)
        subprocess.run(["git", "-C", str(repo), "-c", "user.name=t", "-c", "user.email=t@t",
                        "commit", "-qm", "i"], capture_output=True)
        subprocess.run(["git", "-C", str(repo), "checkout", "-qB", "wip/auto-test"], capture_output=True)
        st2 = t / "repo-debt.json"
        r = run(["record", "节丁", "--state", str(st2)], cwd=str(repo))
        check("base 自动探测（cwd 分支）", r.returncode == 0 and "base=wip/auto-test" in r.stdout)
        # ⑧ 非 git 目录探测失败记 unknown 不丢账
        nongit = t / "nogit"
        nongit.mkdir()
        st3 = t / "nogit-debt.json"
        r = run(["record", "节戊", "--state", str(st3)], cwd=str(nongit))
        check("base 探测失败记 unknown 照记账", r.returncode == 0 and "base=unknown" in r.stdout)
        # ⑨ 腐败账本 loud 红（check/list/record 都拒）且不覆盖坏文件
        bad = t / "bad-debt.json"
        bad.write_text("{不是 json", encoding="utf-8")
        snap = bad.read_bytes()
        r = run(["check", "--state", str(bad)])
        r2 = run(["record", "节己", "--state", str(bad)])
        check("腐败账本 loud 红 rc2 且不动文件",
              r.returncode == 2 and r2.returncode == 2 and bad.read_bytes() == snap)
        # ⑩ 空 section rc2
        r = run(["record", "  ", "--state", str(state)])
        check("空节标识 rc2", r.returncode == 2)
        # ⑪ 用法错 rc2
        r = run(["bogus", "--state", str(state)])
        check("用法错 rc2", r.returncode == 2)

    print(f"review-debt 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}"
          "（记/列/清/查四腿 + 单调 + base 探测 + 腐败账本 fail-loud）")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
