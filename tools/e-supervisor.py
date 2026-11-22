#!/usr/bin/env python3
"""e-supervisor.py —— E lane 监督器（自动化开发二期 P0；规则/16 §5）。

只做两件事，且**永远不许成为停 run 的原因**（fail-open 方向立法）：
  ① machinery-undo：run 对自己机制（MACHINERY_PREFIXES 前缀集）的提交，若 3 小时
    内没有任何产品提交为之背书且会话闲，整批 revert 成**一个**提交（后一个提交
    可能在修前一个——只撤旧会打架，只撤新会恢复它修掉的 bug；一个提交没有半程
    状态）。产品提交 = 触及非机制文件的提交，出现即清空 pending（为之前一切背书）。
  ② repair 哨兵：6 小时零 ship（punch-list 无新增完成项）写 repair-needed +
    run-health.json 报修——心跳据此把 nudge 换成诊断指令。
健康面：.claude/state/run-health.json（machinery_pending/last_ship_ts/repair_needed）。
  python3 tools/e-supervisor.py --tick      # 心跳每拍调（幂等）
  python3 tools/e-supervisor.py --selftest  # 合成 git 仓：背书赦免/整批 revert 原子性/
                                            # 产品塌陷仪表（零产品与 >2:1 两臂）/repair 写清
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STATE = ROOT / ".claude" / "state"
# 本监督器整批回退提交的 subject 前缀：扫描循环见此前缀即跳过——revert 提交自身
# 触及机制文件，不跳过会「撤了又进账」→ 3h 后撤回撤（互撤死循环）。
REVERT_MARK = "supervisor: machinery 自愈整批回退"
MACHINERY_PREFIXES = (
    ".claude/", ".githooks/", ".agents/skills/berryos-runner/",
    "tools/e-guard.py", "tools/e-doctor.py", "tools/e-heartbeat.sh",
    "tools/e-supervisor.py", "tools/run-e.sh", "tools/pack-work.py",
    "tools/push-green.py", "tools/e-receipts.py",
    "tools/review-pin-check.py", "tools/review-recv.py", "tools/review-debt.py",
    "tools/control-plane-gate.sh", "tools/control-plane.txt",
    "tools/run-artifact.sh", "tools/gate-wrap.sh", "tools/claude-audit.py",
    "tools/stale-parks.py", "tools/stranded-deferrals.py", "tools/answers-readback.py",
    ".agents/tools/", "tools/facts.py", "tools/gate-selftest.sh", "tools/punch_slice.py", "tools/task-graph.py", "tools/task-oracle.py",
    "tools/docs_guard.py",
    "tools/floor-check.sh",
    "tools/writable_globals.py",
    "tools/unsafe_audit.py",
    "tools/shvarcheck.py",
    "tools/selftest_guard.py",
    ".github/workflows/",
)
UNDO_AFTER_SECS = 3 * 3600
REPAIR_AFTER_SECS = 6 * 3600


def sh(cmd: list[str], cwd: Path | None = None, timeout: int = 60) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=str(cwd or ROOT))


def is_machinery(subject_files: list[str]) -> bool:
    return bool(subject_files) and all(
        any(f == p or f.startswith(p) for p in MACHINERY_PREFIXES) for f in subject_files
    )


def load_state(state: Path | None = None) -> dict:
    p = (state or STATE) / "e-supervisor.json"
    if p.is_file():
        try:
            return json.loads(p.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError) as e:
            # fail-loud（吸收挖掘 P0）：静默重置=待撤清单凭空清零+repair 钟重置——
            # 账本损坏必须响，操作员修档后才能继续。
            print(f"e-supervisor: 账本损坏（{e}）——拒静默重置；修复或删除 {p} 后重跑", file=sys.stderr)
            raise SystemExit(2)
    return {"pending": {}, "last_ship_ts": time.time(), "last_done_count": 0}


def save_state(st: dict, state: Path | None = None) -> None:
    s = state or STATE
    s.mkdir(parents=True, exist_ok=True)
    (s / "e-supervisor.json").write_text(json.dumps(st, ensure_ascii=False, indent=1), encoding="utf-8")


def tick(idle: bool = False, now: float | None = None, root: Path | None = None,
         state: Path | None = None) -> dict:
    root = root if root is not None else ROOT
    state = state if state is not None else STATE
    now = now if now is not None else time.time()
    st = load_state(state)
    r = sh(["git", "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"], cwd=root)
    upstream = r.stdout.strip() if r.returncode == 0 else None
    base = upstream or "HEAD"
    log = sh(["git", "log", "--name-only", "--pretty=format:%H %s", f"{base}..HEAD"], cwd=root)
    undone: list[str] = []
    commits: list[tuple[str, str, list[str]]] = []
    if log.returncode == 0:
        cur_sha = ""
        cur_subject = ""
        cur_files: list[str] = []
        for ln in log.stdout.splitlines():
            m = re.match(r"^([0-9a-f]{40}) (.*)$", ln)
            if m:
                if cur_sha:
                    commits.append((cur_sha, cur_subject, cur_files))
                cur_sha, cur_subject, cur_files = m.group(1), m.group(2), []
            elif ln.strip():
                cur_files.append(ln.strip())
        if cur_sha:
            commits.append((cur_sha, cur_subject, cur_files))
        in_range = {sha for sha, _s, _f in commits}
        # 已撤 sha 持久化（终审 P1）：revert 提交不把原提交移出 @{u}..HEAD——不记
        # 账则每 3h 一轮空转 revert；同时按区间修剪（推干净后区间空，账本跟着清）。
        st["undone"] = [s for s in st.get("undone", []) if s in in_range]
        undone_set = set(st["undone"])
        # 最老优先扫：产品提交即赦免之前一切，机制提交入 pending
        for sha, subject, files in reversed(commits):
            if subject.startswith(REVERT_MARK):
                continue  # 本监督器自己的回退提交不是新机制工作（防互撤死循环）
            if sha in undone_set:
                continue
            if is_machinery(files):
                st["pending"].setdefault(sha, now)
            else:
                st["pending"].clear()  # 产品提交为它之前的一切背书
        # 撤销判定：最老 pending 已 seen ≥3h 且闲且树净
        oldest = min(st["pending"].values(), default=None)
        if idle and oldest is not None and (now - oldest) >= UNDO_AFTER_SECS:
            st_clean = sh(["git", "status", "--porcelain"], cwd=root)
            if st_clean.returncode == 0 and not st_clean.stdout.strip():
                shas = sorted(st["pending"], key=lambda s: st["pending"][s])
                batch: list[str] = []
                batch_fail = False
                for sha in shas:
                    rv = sh(["git", "revert", "--no-commit", sha], cwd=root, timeout=120)
                    if rv.returncode != 0:
                        # 批内任一笔失败=整批放弃（docstring 立法：一个提交没有半程
                        # 状态）。revert --abort 自身失效时残留暂存由 reset --hard 兜
                        # 底清掉——幸存的半程逆改被后续 commit 拣走=复活旧 bug。
                        sh(["git", "revert", "--abort"], cwd=root)
                        sh(["git", "reset", "--hard", "HEAD"], cwd=root)
                        print(f"e-supervisor: 整批回退第 {len(batch) + 1}/{len(shas)} 笔冲突"
                              f"（{sha[:12]}）——本拍整批放弃不产半程提交，pending 保留", file=sys.stderr)
                        batch_fail = True
                        break
                    batch.append(sha)
                if batch and not batch_fail:
                    cm = sh(["git", "-c", "core.hooksPath=/dev/null", "commit", "-m",
                             f"{REVERT_MARK}（{len(batch)} 笔零产品产出，"
                             f"单提交防半程态）——产品锚恢复后重做"], cwd=root)
                    if cm.returncode != 0:
                        # 提交失败不得清 pending（终审 P1：rc 被丢=清单凭空清零、
                        # 3h 计时器归零重走）；复位残留暂存后下拍重试。
                        sh(["git", "reset", "--hard", "HEAD"], cwd=root)
                        print(f"e-supervisor: 整批回退提交失败（rc={cm.returncode}）——pending 保留"
                              f"待下拍重试：{(cm.stderr or cm.stdout).strip()[-200:]}", file=sys.stderr)
                    else:
                        undone = batch
                        st["pending"].clear()
                        st["undone"] = sorted(set(st.get("undone", [])) | set(batch))
    # repair：6 小时零 ship（punch-list 无新完成项）
    plan = root / ".agents" / "plans" / "2026-11-22-e面运营.md"
    if plan.is_file():
        done = len(re.findall(r"^- \[x\]", plan.read_text(encoding="utf-8"), re.M))
        if done > st.get("last_done_count", -1):
            st["last_ship_ts"] = now
            st["last_done_count"] = done
    # P0 产品面塌陷仪表（考古 2026-11-23：产品面 53%→17% 渐变不可见）
    # product = 触及非 MACHINERY_PREFIXES 文件的提交；tooling = 纯机制文件
    # （含本监督器自己的回退提交——自愈动作也是工具面工作，诚实计数）
    product_count = 0
    tooling_count = 0
    for _sha, _subject, files in commits:
        if is_machinery(files):
            tooling_count += 1
        else:
            product_count += 1
    ratio = f"{product_count}:{tooling_count}" if (product_count + tooling_count) > 0 else "0:0"
    # 一轮「塌陷」= 产品面=0 或 tooling > 2×product（立项规格两臂：渐进滑坡
    # 53%→17% 每轮 product>0，零产品臂单独抓不到——两臂都算，连续 3 轮即红）
    starved_round = (product_count + tooling_count) > 0 and (
        product_count == 0 or tooling_count > 2 * product_count)
    if starved_round:
        st["starved_rounds"] = st.get("starved_rounds", 0) + 1
    else:
        st["starved_rounds"] = 0
    product_starved = st.get("starved_rounds", 0) >= 3

    repair = (now - st.get("last_ship_ts", now)) >= REPAIR_AFTER_SECS
    state.mkdir(parents=True, exist_ok=True)
    (state / "repair-needed").write_text("", encoding="utf-8") if repair else (state / "repair-needed").unlink(missing_ok=True)
    health = {
        "machinery_pending": len(st["pending"]),
        "machinery_reverts": len(undone),
        "hours_since_ship": round((now - st.get("last_ship_ts", now)) / 3600, 2),
        "shipped_today": st.get("last_done_count", 0),
        "repair_needed": repair,
        "product_commits": product_count,
        "tooling_commits": tooling_count,
        "product_ratio": ratio,
        "product_starved": product_starved,
    }
    if product_starved:
        (state / "product-starved").write_text(
            f"连续 {st['starved_rounds']} 轮产品面塌陷（零产品或 tooling>2×product）"
            f"——工具自吞回路（考古 P0）", encoding="utf-8")
    else:
        (state / "product-starved").unlink(missing_ok=True)
    (state / "run-health.json").write_text(json.dumps(health, ensure_ascii=False), encoding="utf-8")
    save_state(st, state)
    return health


def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"e-supervisor 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        sh(["git", "init", "-q"], cwd=t)
        # CI 环境免疫（2026-11-22 run 45012692722 定谳）：actions/checkout 把
        # HOME 换到 _temp 空目录，runner 用户无全局 user.name——首笔空提交
        # fatal 128 → HEAD~1..HEAD 空 → IndexError。腿②同款 -c 注入。
        sh(["git", "-C", str(t), "-c", "user.name=t", "-c", "user.email=t@l",
            "commit", "--allow-empty", "-m", "base"], cwd=t)
        # 腿 ①：机制前缀判定
        check("机制/产品分判", is_machinery([".claude/settings.json"]) and not is_machinery(["kernel/main.rs"]) and not is_machinery([]))
        # 腿 ②：合成仓 undo——machinery 提交 seen 超时 + idle + 树净 → 整批撤一笔
        (t / ".claude").mkdir()
        (t / ".claude" / "settings.json").write_text("{}", encoding="utf-8")
        sh(["git", "-C", str(t), "add", ".claude/settings.json"], cwd=t)
        sh(["git", "-C", str(t), "-c", "user.name=t", "-c", "user.email=t@l", "commit", "-qm", "mach1"], cwd=t)
        # 用函数面模拟（ROOT 换成合成仓不可直接复用 tick——检验 revert 语义子进程化）
        r = sh(["git", "-C", str(t), "log", "--name-only", "--pretty=format:%H", "HEAD~1..HEAD"])
        sha = r.stdout.splitlines()[0]
        files = [ln for ln in r.stdout.splitlines()[1:] if ln.strip()]
        check("git 名单提取", is_machinery(files) and bool(sha))
        rv = sh(["git", "-C", str(t), "revert", "--no-commit", sha])
        check("revert 单笔可执行（钩子关闭通道语义）", rv.returncode == 0)
        sh(["git", "-C", str(t), "revert", "--abort"], cwd=t)
        # 腿 ③-⑨：tick 真跑面（合成 root/state 注入——终审 P1：产品塌陷仪表与
        # repair 哨兵的写/清路径此前零夹具，旧 repair 腿是 21601>=21600 恒真）
        w = t / "w"
        w.mkdir()
        sh(["git", "init", "-q"], cwd=w)
        sh(["git", "-C", str(w), "-c", "user.name=t", "-c", "user.email=t@l",
            "commit", "--allow-empty", "-m", "wbase"], cwd=w)

        def wcommit(msg: str) -> None:
            sh(["git", "-C", str(w), "-c", "user.name=t", "-c", "user.email=t@l",
                "commit", "-qm", msg], cwd=w)

        br = sh(["git", "-C", str(w), "symbolic-ref", "--short", "HEAD"]).stdout.strip()
        whead = sh(["git", "-C", str(w), "rev-parse", "HEAD"]).stdout.strip()
        # set-upstream-to 需要真 remote 配置在先（无 remote.origin.* 时 origin/<br>
        # 不被认作分支——e-doctor 自检同款四行）
        sh(["git", "-C", str(w), "config", "remote.origin.url", "https://example.invalid/x.git"])
        sh(["git", "-C", str(w), "config", "remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"])
        sh(["git", "-C", str(w), "update-ref", f"refs/remotes/origin/{br}", whead])
        sh(["git", "-C", str(w), "branch", f"--set-upstream-to=origin/{br}", br])
        ws = t / "wstate"
        (w / ".claude").mkdir()
        for i in range(3):
            (w / ".claude" / f"m{i}.json").write_text("{}", encoding="utf-8")
            sh(["git", "-C", str(w), "add", ".claude"], cwd=w)
            wcommit(f"mach{i}")
            h = tick(now=1000.0 + i, root=w, state=ws)
        check("产品塌陷零产品臂（3 轮纯机制 → 红 + 标记落盘）",
              h["product_starved"] and (ws / "product-starved").is_file())
        (w / "kernel").mkdir()
        (w / "kernel" / "main.rs").write_text("fn main() {}", encoding="utf-8")
        sh(["git", "-C", str(w), "add", "kernel"], cwd=w)
        wcommit("prod")
        (w / "kernel" / "lib.rs").write_text("pub fn f() {}", encoding="utf-8")
        sh(["git", "-C", str(w), "add", "kernel"], cwd=w)
        wcommit("prod2")
        h = tick(now=1010.0, root=w, state=ws)
        check("产品提交清账（starved 解除 + 标记删除；tooling≤2×product 轮）",
              (not h["product_starved"]) and not (ws / "product-starved").exists()
              and h["product_commits"] == 2)
        # >2:1 臂夹具账：清账点 tooling=3/product=2——m3 轮 4>4 不成立（重置轮），
        # m4 起 tooling≥5 恒 >2×product=4 连续三轮（m4/m5/m6）到红。
        for i in (3, 4, 5, 6):
            (w / ".claude" / f"m{i}.json").write_text("{}", encoding="utf-8")
            sh(["git", "-C", str(w), "add", ".claude"], cwd=w)
            wcommit(f"mach{i}")
            h = tick(now=1020.0 + i, root=w, state=ws)
        check("产品塌陷 >2:1 臂（tooling 连续超产品两倍 → 红；渐进滑坡形态）",
              h["product_starved"] and (ws / "product-starved").is_file())
        # repair 哨兵写/清（真调 tick）
        rs = t / "rstate"
        (w / ".agents" / "plans").mkdir(parents=True)
        planf = w / ".agents" / "plans" / "2026-11-22-e面运营.md"
        planf.write_text("## 未完成事项\n\n- [x] 甲\n", encoding="utf-8")
        h = tick(now=2000.0, root=w, state=rs)
        check("ship 记账（punch 完成项入账不报修）", not h["repair_needed"])
        h = tick(now=2000.0 + 7 * 3600 + 2, root=w, state=rs)
        check("repair 哨兵写（6h 零 ship → repair-needed 落盘）",
              h["repair_needed"] and (rs / "repair-needed").is_file())
        planf.write_text("## 未完成事项\n\n- [x] 甲\n- [x] 乙\n", encoding="utf-8")
        h = tick(now=2000.0 + 7 * 3600 + 3, root=w, state=rs)
        check("repair 哨兵清（新 ship → repair-needed 删除）",
              (not h["repair_needed"]) and not (rs / "repair-needed").exists())
    print(f"e-supervisor 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}"
          "（分判/名单/revert 通道/产品塌陷两臂清账/repair 写清）")
    return 0 if fails == 0 else 1


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    idle = "--idle" in argv
    health = tick(idle=idle)
    print(f"e-supervisor: tick（pending={health['machinery_pending']} reverts={health['machinery_reverts']} "
          f"零ship={health['hours_since_ship']}h repair={health['repair_needed']}）")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
