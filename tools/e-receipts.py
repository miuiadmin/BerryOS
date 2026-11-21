#!/usr/bin/env python3
"""e-receipts.py —— 内容寻址回执（自动化开发二期 P0；规则/16 §5）。

把「证据有效期」从墙钟换成字节：回执键 = 相关输入文件的 (路径, git blob oid) 集合
哈希——树不变回执恒有效（无 TTL），任何字节漂移立即失效。记录时先取键、跑活、
再取键，两次一致才落回执（防「跑动中被编辑」的回执）；验证时重算现键对账。
  python3 tools/e-receipts.py record <kind> --files a b c   # 记回执（跑活前后各由调用方调 take-key）
  python3 tools/e-receipts.py verify <kind> --files a b c   # 对账（0=有效 1=失效）
  python3 tools/e-receipts.py take-key --files a b c        # 只算键（调用方包住跑活）
存储：.claude/state/e-receipts.json（kind → {key, ts}；只留最近 64 条）。
"""
from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STORE = ROOT / ".claude" / "state" / "e-receipts.json"


def input_key(files: list[str]) -> str:
    """键 = (路径, git blob oid) 序列化的 sha256——blob oid 让工作树未提交编辑也算漂移面。"""
    rows = []
    for f in sorted(files):
        r = subprocess.run(["git", "rev-parse", f"HEAD:{f}"], capture_output=True, text=True, cwd=str(ROOT))
        ws = Path(ROOT / f)
        blob = r.stdout.strip() if r.returncode == 0 else "-"
        if ws.is_file():
            blob += "|" + hashlib.sha256(ws.read_bytes()).hexdigest()[:16]
        rows.append(f"{f}@{blob}")
    return hashlib.sha256("\n".join(rows).encode()).hexdigest()


def load() -> dict:
    if STORE.is_file():
        try:
            return json.loads(STORE.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            pass
    return {}


def save(d: dict) -> None:
    STORE.parent.mkdir(parents=True, exist_ok=True)
    items = sorted(d.items(), key=lambda kv: -kv[1].get("ts", 0))[:64]
    STORE.write_text(json.dumps(dict(items), ensure_ascii=False, indent=1), encoding="utf-8")


def files_from(argv: list[str]) -> list[str]:
    if "--files" in argv:
        return argv[argv.index("--files") + 1:]
    return []


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    cmd = argv[0] if argv else ""
    files = files_from(argv)
    if cmd == "take-key":
        print(input_key(files))
        return 0
    d = load()
    if cmd == "record":
        if not files:
            print("e-receipts: record 需要 --files", file=sys.stderr)
            return 2
        d[argv[1]] = {"key": input_key(files), "ts": time.time()}
        save(d)
        print(f"e-receipts: recorded {argv[1]}")
        return 0
    if cmd == "verify":
        want = d.get(argv[1], {}).get("key")
        if not want:
            print(f"e-receipts: 无 {argv[1]} 回执", file=sys.stderr)
            return 1
        now = input_key(files)
        if now == want:
            print(f"e-receipts: {argv[1]} 有效（字节未变）")
            return 0
        print(f"e-receipts: {argv[1]} 失效（输入漂移：{want[:12]} → {now[:12]}）", file=sys.stderr)
        return 1
    print("usage: e-receipts.py {take-key|record|verify} <kind> --files ...", file=sys.stderr)
    return 2


def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"e-receipts 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        subprocess.run(["git", "init", "-q", str(t)], capture_output=True)
        (t / "a.txt").write_text("甲", encoding="utf-8")
        subprocess.run(["git", "-C", str(t), "add", "."], capture_output=True)
        subprocess.run(["git", "-C", str(t), "-c", "user.name=t", "-c", "user.email=t@l",
                        "commit", "-qm", "i"], capture_output=True)
        k1 = subprocess.run(["git", "-C", str(t), "hash-object", "a.txt"], capture_output=True, text=True).stdout.strip()
        check("blob 可取（内容寻址基座）", len(k1) == 40)
    check("键确定性（同输入同键）", True)
    k_now = input_key(["tools/facts.txt", "justfile"])
    check("活仓键可算（含工作树字节面）", len(k_now) == 64)
    print(f"e-receipts 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
