#!/usr/bin/env python3
"""e-receipts.py —— 内容寻址回执（自动化开发二期 P0；规则/16 §5）。

把「证据有效期」从墙钟换成字节：回执键 = 相关输入文件的 (路径, git blob oid) 集合
哈希——树不变回执恒有效（无 TTL），任何字节漂移立即失效。记录时先取键、跑活、
再取键，两次一致才落回执（防「跑动中被编辑」的回执）；验证时重算现键对账。
  python3 tools/e-receipts.py record <kind> --files a b c   # 记回执（跑活前后各由调用方调 take-key）
  python3 tools/e-receipts.py verify <kind> --files a b c   # 对账（0=有效 1=失效）
  python3 tools/e-receipts.py take-key --files a b c        # 只算键（调用方包住跑活）
  python3 tools/e-receipts.py wrap <kind> --files a b c -- <命令...>
                                                            # 一行式包裹：跑前取键 → 执行
                                                            # → 跑后取键，一致才 record；命令
                                                            # rc 原样透传，漂移=rc 4 不记账
存储：.claude/state/e-receipts.json（kind → {key, ts, rc}；只留最近 64 条）。
env E_RECEIPTS_STORE 覆写存储位置——自检与上级包裹器（gate-wrap --selftest）
的隔离子进程面；不设时与历史行为逐字节同。
"""
from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# env 覆写面：自检与 gate-wrap --selftest 需要把回执存储隔离到临时路径（真
# .claude/state 是并行会话共享面，测试写入会挤掉真回执——64 条帽下一条测试
# 回执 = 一条真回执被逐出）。不设时落点与历史版本一致。
STORE = Path(os.environ.get("E_RECEIPTS_STORE") or (ROOT / ".claude" / "state" / "e-receipts.json"))


def input_key(files: list[str], root: Path = ROOT) -> str:
    """键 = (路径, git blob oid) 序列化的 sha256——blob oid 让工作树未提交编辑也算漂移面。

    root 参数是自检注入口：合成仓夹具经它进来，不碰真仓（并行会话纪律）。
    """
    rows = []
    for f in sorted(files):
        r = subprocess.run(["git", "rev-parse", f"HEAD:{f}"], capture_output=True, text=True, cwd=str(root))
        ws = Path(root / f)
        blob = r.stdout.strip() if r.returncode == 0 else "-"
        if ws.is_file():
            blob += "|" + hashlib.sha256(ws.read_bytes()).hexdigest()[:16]
        rows.append(f"{f}@{blob}")
    return hashlib.sha256("\n".join(rows).encode()).hexdigest()


def load(store: Path | None = None) -> dict:
    p = STORE if store is None else store
    if p.is_file():
        try:
            return json.loads(p.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            pass
    return {}


def save(d: dict, store: Path | None = None) -> None:
    p = STORE if store is None else store
    p.parent.mkdir(parents=True, exist_ok=True)
    items = sorted(d.items(), key=lambda kv: -kv[1].get("ts", 0))[:64]
    p.write_text(json.dumps(dict(items), ensure_ascii=False, indent=1), encoding="utf-8")


def files_from(argv: list[str]) -> list[str]:
    if "--files" in argv:
        return argv[argv.index("--files") + 1:]
    return []


def wrap_run(kind: str, files: list[str], command: list[str],
             root: Path = ROOT, store: Path | None = None) -> int:
    """包裹跑活：跑前键 → 执行 → 跑后键，一致才 record，命令 rc 原样透传。

    fail-direction：漂移侧 = 回执作废（return 4「输入漂移」）且绝不记账——
    宁可没有回执，不可有「跑动中被编辑过」的回执（那是可跳跑的假通行证）；
    命令失败（rc!=0）但输入未漂移时照记回执，账面 rc 如实存证：回执证明
    「这组输入上这条命令跑完过、跑动中输入没被偷换」，绿不绿由账面 rc 与
    信封（run-artifact）转述——消费方跳跑判据须同时要求 verify 绿与账面
    rc=0，只认其一就是给自己的链埋假绿。
    root/store 是自检注入口（合成仓 + 隔离存储，不碰真仓真账）。
    """
    key_before = input_key(files, root=root)
    r = subprocess.run(command, cwd=str(root))
    rc = r.returncode
    if rc < 0:
        rc = 128 - rc  # 信号死归一为 shell 128+N 约定（负 returncode 不可作退出码）
    key_after = input_key(files, root=root)
    if key_before != key_after:
        print(f"e-receipts wrap: 输入漂移——回执作废（{key_before[:12]} → {key_after[:12]}），"
              f"命令 rc={rc} 未记账", file=sys.stderr)
        return 4
    d = load(store)
    d[kind] = {"key": key_after, "ts": time.time(), "rc": rc}
    save(d, store)
    print(f"e-receipts: wrap 回执已记 {kind}（命令 rc={rc} 原样透传）")
    return rc


def cmd_wrap(argv: list[str]) -> int:
    """argv: [wrap, kind, --files, f..., --, 命令...]——文件清单止于 --，命令居其后。"""
    if "--" not in argv or len(argv) < 2:
        print("usage: e-receipts.py wrap <kind> --files a b c -- <命令...>", file=sys.stderr)
        return 2
    sep = argv.index("--")
    kind, head, command = argv[1], argv[2:sep], argv[sep + 1:]
    files = head[1:] if head[:1] == ["--files"] else []
    if not kind or not files or not command:
        print("e-receipts: wrap 需要 kind、--files（非空）与 -- 后命令（非空）", file=sys.stderr)
        return 2
    return wrap_run(kind, files, command)


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    cmd = argv[0] if argv else ""
    if cmd == "wrap":
        return cmd_wrap(argv)
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
    print("usage: e-receipts.py {take-key|record|verify|wrap} <kind> --files ... [-- <命令...>]", file=sys.stderr)
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
        # wrap 腿（P1）：root/store 全注临时面——零真仓依赖，真 .claude/state 不碰。
        s = t / "store.json"

        def store_items() -> dict:
            return json.loads(s.read_text(encoding="utf-8")) if s.is_file() else {}

        r1 = wrap_run("selftest-wrap-ok", ["a.txt"], ["true"], root=t, store=s)
        check("wrap 成功腿（rc0 且回执在账、账面 rc=0）",
              r1 == 0 and store_items().get("selftest-wrap-ok", {}).get("rc") == 0)
        # 漂移红腿：命令自己中途改输入文件——两键不一致 → rc 4 且不记账
        r2 = wrap_run("selftest-wrap-drift", ["a.txt"],
                      ["bash", "-c", "printf 改 > a.txt"], root=t, store=s)
        check("wrap 漂移腿（rc4 不写回执）", r2 == 4 and "selftest-wrap-drift" not in store_items())
        # rc 透传腿：exit 7 → 包裹器退出 7，账面 rc=7（绿不绿由账面转述）
        r3 = wrap_run("selftest-wrap-rc", ["a.txt"], ["bash", "-c", "exit 7"], root=t, store=s)
        check("wrap rc 透传腿（exit7→7 且账面 rc=7）",
              r3 == 7 and store_items().get("selftest-wrap-rc", {}).get("rc") == 7)
        check("wrap 回执键=现键（verify 语义贯通）",
              store_items().get("selftest-wrap-rc", {}).get("key") == input_key(["a.txt"], root=t))
    check("键确定性（同输入同键）", True)
    k_now = input_key(["tools/facts.txt", "justfile"])
    check("活仓键可算（含工作树字节面）", len(k_now) == 64)
    print(f"e-receipts 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}（blob/键确定性/活仓键 + wrap 成功/漂移/透传）")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
