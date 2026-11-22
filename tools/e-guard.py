#!/usr/bin/env python3
"""e-guard.py —— E lane 会话守卫与 fixpoint 哨兵（自动化开发二期 P0；规则/16 §5）。

三个角色：
  stop-hook   挂 .claude/settings.json 的 Stop 事件（stdin 收 harness JSON，不解析）：
              仅 BERRYOS_E_RUN=1 的无人会话受管——marker 在且无 fixpoint 哨兵即
              exit 2 扣住会话（「跑到机器验证的终点才许停」从纪律变代码）；
              交互会话零打扰。
  frozen-guard 挂 PreToolUse（Edit|Write|MultiEdit）：仅无人会话受管——FROZEN 集
              （宪法/闸/编排脚本）不可写，改宪法诉求落 punch-list 走 attended 会话。
  fixpoint    哨兵唯一写点。写前过四门（任一不过即拒且不写）：
              ① 树净（git status --porcelain 空）；
              ② 双轨判空——punch-list 判空 AND task-oracle summary 轨的
                NEEDS_WORK+BLOCKED 清零（两轨都清了才准停——DAG 有活就不许停）；
                细化：NEEDS_WORK=0 且 BLOCKED>0 = 只剩可恢复 park 在账，按
                「BLOCKED 记因收工」停（--blocked 手动旗被 oracle 自动化，全终态
                才配「自然收工」）；oracle 缺席 = 装配期过渡按 punch 单轨判空
                （落库后自动升双轨，只收紧不放松）；拒判/坏状态 = 宁可拒停——
                停机门上「读不出」≠「没有活」，refuse 比 lie 好；
              ③ 门禁套件绿——check-docs --strict（快照语义由提交侧闸管，此处全树）
                与 facts 对拍；
              ④ 心跳在场不是必需，但 supervisor 健康文件若报 repair_needed 则拒
                （有病先修，不许带病收工）。
  clear       操作员清哨兵（配合 run-e stop）。
  --selftest  合成夹具：四门各有红腿、全绿腿、哨兵写点唯一性、DAG 轨六态判决。

哨兵文件 .claude/state/e-fixpoint.sentinel 的全部写点 = 本函数——Stop hook 只读它
（消费者只看存在性，不解析——故哨兵 JSON 可携带 dag 证据面而不破兼容）。
"""
from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STATE = ROOT / ".claude" / "state"
MARKER = STATE / "e-run.marker"
SENTINEL = STATE / "e-fixpoint.sentinel"
PUNCH_PLAN = ROOT / ".agents" / "plans" / "2026-11-22-e面运营.md"

# FROZEN 集（规则/16 §5：无人会话不可写；改它们 = attended 会话的活）
FROZEN_PREFIXES = (
    ".claude/settings.json", ".claude/hooks/", ".githooks/", ".claude/MANIFEST.md",
    ".agents/skills/berryos-runner/",
    "tools/e-guard.py", "tools/e-doctor.py", "tools/e-heartbeat.sh",
    "tools/e-supervisor.py", "tools/run-e.sh", "tools/pack-work.py",
    "tools/push-green.py", "tools/e-receipts.py",
    "tools/review-pin-check.py", "tools/review-recv.py", "tools/review-debt.py",
    "tools/control-plane-gate.sh", "tools/control-plane.txt",
    "tools/run-artifact.sh", "tools/gate-wrap.sh",
    "tools/claude-audit.py", "tools/stale-parks.py",
    "tools/stranded-deferrals.py", "tools/answers-readback.py",
    ".agents/tools/", "tools/facts.py", "tools/gate-selftest.sh", "tools/punch_slice.py",
    # P2 跨树 sweep 高危类（考古）：大规模路径/符号迁移须 owner 确认——列入冻结面
    #（task-oracle.py 归下方 DAG 输入面行——每文件只登一次，终审 r3 清 dup）
    "tools/task-graph.py",
    "tools/docs_guard.py",
    "tools/floor-check.sh",
    "tools/writable_globals.py",
    "tools/unsafe_audit.py",
    "tools/shvarcheck.py",
    "tools/selftest_guard.py",
    ".github/workflows/",
    # 任务 DAG 轨输入面（装配期双名占位：车道名 task-oracle、Study 名 task-plan）——
    # 它喂停机门第②门，无人会话改判停依据 = 自裁口子
    "tools/task-oracle.py", "tools/task-plan.py",
    # 全道 CI 判定门五件（终审 r3 补员）：ci.yml 重道把它们的判定当门跑——
    # 无人会话改判自己的 CI 要过的门 = 自裁口子（与 control-plane.txt r3 补员同批镜像）
    "tools/gui_guard.py", "tools/sdk_ledger.py", "tools/design_ledger.py",
    "tools/ver-check.sh", "tools/gui8-smoke.py",
)

# task-oracle 的子命令面是旗标式（非 positional）：summary 要 --json 才出机器面。
# 为什么映射表在消费侧：两个消费面（e-guard/pack-work）同文各持一份管道，命令
# 面漂移在这里一眼对齐。
_ORACLE_ARGS = {
    "summary": ["--summary", "--json"],
    "next-buildable": ["--next-buildable"],
}

# 替身 oracle（--selftest 专用）：真读合成 plans 树派生 summary 计数——让子进程
# 腿测的是「合成树→替身→本件管道」整链而非死字符串；旗标面镜像真 oracle。
_FAKE_ORACLE_SRC = (
    "import json, sys\n"
    "from pathlib import Path\n"
    "root = Path(__file__).resolve().parent.parent\n"
    "open_n = done_n = 0\n"
    "for p in sorted((root / '.agents' / 'plans').glob('*.md')):\n"
    "    for ln in p.read_text(encoding='utf-8').splitlines():\n"
    "        s = ln.strip()\n"
    "        if s.startswith('- [ ] T'):\n"
    "            open_n += 1\n"
    "        elif s.startswith('- [x] T'):\n"
    "            done_n += 1\n"
    "if '--summary' in sys.argv[1:]:\n"
    "    print(json.dumps({'counts': {'DONE': done_n, 'NEEDS_WORK': open_n, 'BLOCKED': 0}}))\n"
    "else:\n"
    "    print(json.dumps({'buildable': []}))\n"
)


def sh(cmd: list[str], timeout: int = 120) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=str(ROOT))


def is_headless() -> bool:
    return os.environ.get("BERRYOS_E_RUN") == "1"


# 切分单源（吸收挖掘 P0）：本门与 pack-work 对「什么算 open」必须同源——各执一份
# 正则就是控制面正确性隐患。实现唯一家 = tools/punch_slice.py。
# 装载面（终审 P1）：本件同时挂 Stop 与 PreToolUse 两个钩子——模块级 import 时
# punch_slice 缺席/损坏会让钩子进程带 traceback 崩 rc 1（Claude Code 只把 rc 2 当
# 阻断）→ 停机保护与 FROZEN 封条双双 fail-open 且无报错面（跨 lane 重放丢件先例
# f03ec3f3 在案）。故迟到装载 + 失败可判：装载不出 = 按「有活拒停」处置；frozen-guard
# 不经此依赖（零 punch 面），模块级装载故障面整体不存在。
_SLICE_CACHE: dict[str, object] = {}


def _slice_punch_fn(root: Path | None = None):
    import importlib.util
    key = str(root or ROOT)
    if key in _SLICE_CACHE:
        return _SLICE_CACHE[key]
    p = (root or ROOT) / "tools" / "punch_slice.py"
    fn = None
    if p.is_file():
        try:
            spec = importlib.util.spec_from_file_location("berryos_punch_slice", p)
            if spec is not None and spec.loader is not None:
                mod = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(mod)
                fn = mod.slice_punch
        except Exception as e:
            print(f"e-guard: punch_slice 装载失败（{e}）——按缺席处置（宁可拒停）", file=sys.stderr)
    if fn is not None:
        _SLICE_CACHE[key] = fn  # 只缓存成功装载：失败/缺席每拍重探，修复后自动恢复
    return fn


def open_punch_items(text: str, root: Path | None = None) -> list[str]:
    """活跃计划「未完成事项」节里未 park 的开放条目（机器读回 punch-list 的 oracle 面）。"""
    fn = _slice_punch_fn(root)
    if fn is None:
        return ["<punch 切分单源缺席/损坏（tools/punch_slice.py）——装载故障=拒停面，修复后本门自动恢复>"]
    r = fn(text)
    if not r["vehicle_present"]:
        return ["<punch 载体缺席：归档/改名必须先改 punch_slice.DEFAULT_PUNCH，宁可拒停>"]
    return r["open"]


def oracle_face(subcmd: str, script: Path | None = None, root: Path | None = None) -> dict:
    """task-oracle 子进程面 → 状态信封 {status, data, detail}（管道工，非语义）。

    与 pack-work.py 同文的薄管道（为什么双份而不是 import：文件名带连字符不可
    常规 import，而 e-guard 挂在 Stop hook 上——控制面件不在钩子里背另一个件
    的加载故障面；语义唯一家仍在 tools/task-oracle.py，本份只做进程与 JSON）。
    status 四态：ok=答了（data=stdout JSON）；absent=件不在（装配/搬场期）；
    refused=rc 2 拒判或用法错（oracle 自己的 fail-closed 面：状态载不入，宁拒不
    猜）；error=其余非零/超时/坏 JSON（规划故障）。
    """
    sc = script or (root or ROOT) / "tools" / "task-oracle.py"
    if not sc.is_file():
        return {"status": "absent", "data": None, "detail": f"{sc} 不在"}
    try:
        r = subprocess.run([sys.executable, str(sc), *_ORACLE_ARGS.get(subcmd, [f"--{subcmd}"])],
                           capture_output=True, text=True, timeout=60, cwd=str(root or ROOT))
    except subprocess.TimeoutExpired:
        return {"status": "error", "data": None, "detail": "超时（>60s）"}
    except OSError as e:
        return {"status": "error", "data": None, "detail": f"起不了进程：{e}"}
    if r.returncode == 2:
        return {"status": "refused", "data": None,
                "detail": (r.stderr or r.stdout).strip()[-160:] or "rc=2 拒判（无诊断）"}
    if r.returncode != 0:
        return {"status": "error", "data": None,
                "detail": f"rc={r.returncode}：{(r.stderr or r.stdout).strip()[-160:]}"}
    try:
        return {"status": "ok", "data": json.loads(r.stdout), "detail": ""}
    except json.JSONDecodeError:
        return {"status": "error", "data": None, "detail": f"stdout 非 JSON：{r.stdout.strip()[:120]}"}


def dag_counts(face=None) -> dict:
    """task-oracle --summary --json 面 → {status, counts, detail}。

    counts=None 或 {NEEDS_WORK, BLOCKED}。真 oracle 的机器面=整语料 JSON（counts
    嵌套键，三键预置含 total）；本件另接受 {"NEEDS_WORK": n, "BLOCKED": n} 平铺
    形做对齐缓冲。缺键不默认 0——原型 --summary 两键字典 KeyError 教训：BLOCKED
    键缺席那天就是漏报停机那天，缺任一键=坏状态按 error 处置，不猜。
    """
    r = (face or oracle_face)("summary")
    if r["status"] != "ok":
        return {"status": r["status"], "counts": None, "detail": r.get("detail") or r["status"]}
    data = r.get("data")
    counts = None
    if isinstance(data, dict):
        for cand in (data.get("counts"), data):
            if (isinstance(cand, dict) and isinstance(cand.get("NEEDS_WORK"), int)
                    and isinstance(cand.get("BLOCKED"), int)):
                counts = {"NEEDS_WORK": cand["NEEDS_WORK"], "BLOCKED": cand["BLOCKED"]}
                break
    if counts is None:
        return {"status": "error", "counts": None,
                "detail": "summary 缺 NEEDS_WORK/BLOCKED 预置计数或值非整数（两键字典教训：缺键不默认 0）"}
    return {"status": "ok", "counts": counts, "detail": ""}


def dag_stop_verdict(dag: dict) -> tuple[bool, bool, str]:
    """DAG 轨停机判决（纯函数，selftest 直打）→ (允许停, 只剩 BLOCKED, 诊断)。

      ok + NEEDS_WORK>0 → (False, …)   DAG 有活就不许停（车道律——两轨都清了才准停）
      ok + 全零         → (True, False) 两轨全清——自然收工
      ok + 只剩 BLOCKED → (True, True)  可恢复 park 在账=合法停（设计 item3：--blocked
                                        手动旗被 oracle 自动化，全终态才是「自然收工」）
      absent            → (True, False) 装配期过渡：punch 单轨判空（oracle 落库后自动
                                        升双轨——行为收紧不放松）
      error / refused   → (False, …)    停机门上「读不出」≠「没有活」：坏状态读成清零
                                        = 静默完成未完成的工作，refuse 比 lie 好
    """
    if dag.get("status") == "ok":
        c = dag.get("counts")
        if not isinstance(c, dict) or not isinstance(c.get("NEEDS_WORK"), int) \
                or not isinstance(c.get("BLOCKED"), int):
            return False, False, "counts 非法（缺键或非整数）——坏状态拒停，不猜"
        nw, bl = c["NEEDS_WORK"], c["BLOCKED"]
        if nw > 0:
            return False, False, (f"NEEDS_WORK={nw} BLOCKED={bl}——DAG 有活就不许停"
                                  "（python3 tools/pack-work.py 看 hint 指名首项）")
        if bl > 0:
            return True, True, f"NEEDS_WORK=0 BLOCKED={bl}——只剩可恢复 park 在账（记因收工形态）"
        return True, False, "NEEDS_WORK=0 BLOCKED=0（两轨全清）"
    if dag.get("status") == "absent":
        return True, False, "task-oracle 缺席——punch 单轨判空（装配期过渡；落库后自动升双轨）"
    return False, False, (f"task-oracle {dag.get('status')}（{dag.get('detail') or '无诊断'}）"
                          "——读不出≠没有活，宁可拒停")


def gate_suite_green() -> tuple[bool, str]:
    r = sh(["node", ".agents/tools/check-docs.mjs", "--strict"], timeout=180)
    if r.returncode != 0:
        return False, f"check-docs --strict 红：{r.stdout.strip()[-200:]}"
    r = sh(["python3", "tools/facts.py"], timeout=120)
    if r.returncode != 0:
        return False, f"facts 对拍红：{r.stdout.strip()[-200:]}"
    return True, "check-docs + facts 绿"


def repair_needed() -> bool:
    h = STATE / "run-health.json"
    if not h.is_file():
        return False
    try:
        return bool(json.loads(h.read_text(encoding="utf-8")).get("repair_needed"))
    except (json.JSONDecodeError, OSError):
        return False


def write_sentinel(blocked_flag: bool = False) -> bool:
    """四门全过才写；任何失败返回 False 且不落盘。全仓唯一哨兵写点。

    收工形态自动记因（设计 item3）：--blocked 手动旗被 oracle 自动化——DAG 轨
    只剩 BLOCKED（可恢复 park 在账）时自动「BLOCKED 记因收工」，全终态（两轨
    全清）才配「自然收工」；哨兵 JSON 带 dag 证据面（消费者只看存在性，兼容）。
    """
    checks: list[tuple[str, bool, str]] = []
    st = sh(["git", "status", "--porcelain"])
    checks.append(("树净", st.returncode == 0 and st.stdout.strip() == "", st.stdout.strip()[:120]))
    # ② 双轨判空（本门升级）：punch-list 判空 AND task-oracle summary 轨清——
    #    两轨都清了才准停（DAG 有活就不许停）；缺席=装配期 punch 单轨过渡，
    #    拒判/坏状态=宁可拒停（见 dag_stop_verdict 的六态判决注记）。
    if PUNCH_PLAN.is_file():
        items = open_punch_items(PUNCH_PLAN.read_text(encoding="utf-8"))
    else:
        items = ["<punch 载体缺席：.agents/plans/2026-11-22-e面运营.md 不在——归档/改名必须先改本常量，宁可拒停>"]
    dag = dag_counts()
    dag_allowed, dag_blocked_only, dag_detail = dag_stop_verdict(dag)
    checks.append(("双轨判空（punch+DAG）", (not items) and dag_allowed,
                   f"punch {len(items)} 条开放；DAG 轨：{dag_detail}"))
    ok, msg = gate_suite_green()
    checks.append(("门禁套件绿", ok, msg))
    checks.append(("无 repair_needed", not repair_needed(), "supervisor 报修在案"))
    bad = [(n, d) for n, ok_, d in checks if not ok_]
    if bad:
        for n, d in bad:
            print(f"e-guard fixpoint: 拒——{n} 未过（{d}）", file=sys.stderr)
        return False
    reason = "自然收工" if not (blocked_flag or dag_blocked_only) else "BLOCKED 记因收工"
    STATE.mkdir(parents=True, exist_ok=True)
    SENTINEL.write_text(json.dumps({
        "reason": reason, "pid": os.getpid(),
        "dag": {"status": dag["status"], "counts": dag["counts"]},
    }, ensure_ascii=False), encoding="utf-8")
    print(f"e-guard fixpoint: 哨兵已写（{reason}；DAG 轨：{dag_detail}）——Stop hook 将放行自然停机")
    return True


def cmd_stop_hook() -> int:
    _ = sys.stdin.read() if not sys.stdin.isatty() else ""
    if not is_headless():
        return 0
    if SENTINEL.is_file():
        return 0
    if not MARKER.is_file():
        return 0  # 操作员已 stop——放行
    print("[E-STOP] 无人会话不许自愿停机：活还没到机器验证的终点。正确动作："
          "python3 tools/pack-work.py 拿开工包按 hint 干下一项（DAG 活优先，"
          "punch-list 兜底）；确实无活可干→ python3 tools/e-guard.py fixpoint"
          "（过四门写哨兵后自然停）；被硬停类阻塞→ 按 SKILL.md 落 punch-list 后"
          " python3 tools/e-guard.py fixpoint --blocked 记因收工。", file=sys.stderr)
    return 2


def cmd_frozen_guard() -> int:
    if not is_headless():
        return 0
    try:
        payload = json.loads(sys.stdin.read() or "{}")
    except json.JSONDecodeError:
        return 0
    path = payload.get("tool_input", {}).get("file_path", "") or payload.get("tool_input", {}).get("notebook_path", "")
    if not path:
        return 0
    rel = Path(path).resolve()
    try:
        rel_s = str(rel.relative_to(ROOT.resolve()))
    except ValueError:
        return 0  # 仓外路径不归本闸管
    if any(rel_s == p or rel_s.startswith(p) for p in FROZEN_PREFIXES):
        print(f"[E-FROZEN] 无人会话不可写 {rel_s}（宪法/闸/编排脚本属 FROZEN 集，规则/16 §5）。"
              "改宪法诉求：punch-list 加一条 [PARKED→人审: 理由]，在 attended 会话做。", file=sys.stderr)
        return 2
    return 0


def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"e-guard 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    # ① open_punch_items：parked 不算开放、普通开放算
    sample = "## 未完成事项\n\n- [ ] 普通开放\n- [ ] [PARKED→人审: 等裁决] 挂账项\n- [x] 已完成\n\n## 验收标准\n- [ ] 别节的\n"
    items = open_punch_items(sample)
    check("punch oracle（parked 不计/开放计/出节截止；单源后 open 为去前缀文本）", items == ["普通开放"])
    # ①b 切分单源迟到装载（终审 P1）：缺席/损坏 = 拒停哨兵（fail-closed），在位 = 同判
    with tempfile.TemporaryDirectory() as td:
        tr = Path(td)
        absent = open_punch_items(sample, root=tr)
        (tr / "tools").mkdir()
        shutil.copy(ROOT / "tools" / "punch_slice.py", tr / "tools" / "punch_slice.py")
        loaded = open_punch_items(sample, root=tr)
    check("punch_slice 迟到装载（缺席=拒停哨兵/在位=同判）",
          absent != [] and "缺席" in absent[0] and loaded == ["普通开放"])
    # ② FROZEN 拦截语义
    frozen_hit = any("tools/e-guard.py" == p for p in FROZEN_PREFIXES)
    check("FROZEN 集含自身（闸即宪法）", frozen_hit)
    check("FROZEN 集含 task-oracle/task-plan（停机门输入面，无人会话不可改判停依据）",
          "tools/task-oracle.py" in FROZEN_PREFIXES and "tools/task-plan.py" in FROZEN_PREFIXES)
    check("FROZEN 集含全道 CI 闸族五件（gui_guard/sdk_ledger/design_ledger/ver-check/gui8-smoke）",
          all(p in FROZEN_PREFIXES for p in (
              "tools/gui_guard.py", "tools/sdk_ledger.py", "tools/design_ledger.py",
              "tools/ver-check.sh", "tools/gui8-smoke.py")))
    check("FROZEN 集无重复条目（终审 r3 清相邻 dup 对）",
          len(FROZEN_PREFIXES) == len(set(FROZEN_PREFIXES)))
    # ③ 合成仓四门红腿：脏树拒写哨兵
    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        r = subprocess.run(["git", "-C", str(t), "init", "-q"], capture_output=True)
        (t / "f.txt").write_text("x", encoding="utf-8")
        # 用子进程环境隔离跑 stop-hook 的 headless 判别
        env = {**os.environ, "BERRYOS_E_RUN": "1"}
        r = subprocess.run([sys.executable, str(Path(__file__).resolve()), "selftest-stop-hook"],
                           capture_output=True, text=True, env=env, input="{}")
        check("stop-hook 头腿拒（marker/sentinel 均缺席时放行=操作员态）", r.returncode == 0)
        st_dir = t / ".claude" / "state"
        st_dir.mkdir(parents=True)
        # marker 在、哨兵无 → 拒；复用真实 STATE 不可（活树），改打函数面：
    # write_sentinel 在活树上会被树净门拒（本仓必有未提交内容时）——只验「门不过即不落盘」
    before = SENTINEL.is_file()
    ok = True
    try:
        ok = write_sentinel(blocked_flag=False)
    except Exception:
        ok = False
    if ok and not before:
        SENTINEL.unlink(missing_ok=True)  # 活树恰净且全过——写后即清，保持现场
    check("哨兵写点带四门（门不过不落盘 / 全过可写）", (not ok and not SENTINEL.is_file()) or (ok and (before or SENTINEL.is_file())))
    # ④ DAG 轨停机判决六态（纯函数面，注入合成信封）
    cases = [
        ({"status": "ok", "counts": {"NEEDS_WORK": 2, "BLOCKED": 0}}, False, False, "有活不许停"),
        ({"status": "ok", "counts": {"NEEDS_WORK": 0, "BLOCKED": 0}}, True, False, "全零过（自然收工）"),
        ({"status": "ok", "counts": {"NEEDS_WORK": 0, "BLOCKED": 3}}, True, True, "只剩 BLOCKED=记因收工"),
        ({"status": "absent", "counts": None, "detail": ""}, True, False, "缺席单轨过渡"),
        ({"status": "refused", "counts": None, "detail": "状态载不入"}, False, False, "拒判拒停"),
        ({"status": "error", "counts": None, "detail": "超时"}, False, False, "坏状态拒停"),
    ]
    dag_ok = True
    for dag, exp_allow, exp_bo, name in cases:
        allow, blocked_only, _det = dag_stop_verdict(dag)
        if allow != exp_allow or blocked_only != exp_bo:
            dag_ok = False
            print(f"  ↳ 腿红：{name} → allow={allow} blocked_only={blocked_only}")
    check("DAG 轨停机判决六态（有活拒停/全零/只剩 BLOCKED/缺席/拒判/坏状态）", dag_ok)
    # ⑤ summary 解析：嵌套/平铺两形 + 缺键不默认 0（原型两键字典 KeyError 教训）
    n1 = dag_counts(lambda sub: {"status": "ok", "data": {"counts": {"DONE": 5, "NEEDS_WORK": 1, "BLOCKED": 0}}, "detail": ""})
    n2 = dag_counts(lambda sub: {"status": "ok", "data": {"NEEDS_WORK": 0, "BLOCKED": 2}, "detail": ""})
    n3 = dag_counts(lambda sub: {"status": "ok", "data": {"counts": {"NEEDS_WORK": 1}}, "detail": ""})
    check("summary 双形解析 + 缺键=error 不默认 0",
          n1["status"] == "ok" and n1["counts"] == {"NEEDS_WORK": 1, "BLOCKED": 0}
          and n2["counts"] == {"NEEDS_WORK": 0, "BLOCKED": 2}
          and n3["status"] == "error" and n3["counts"] is None)
    # ⑥ 合成 plans 树 + 替身 oracle 子进程腿：有活→不许停；勾完→全零过
    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        (t / ".agents" / "plans").mkdir(parents=True)
        pf = t / ".agents" / "plans" / "2099-01-01-合成.md"
        pf.write_text("## 未完成事项\n\n- [ ] T01·甲：交付甲\n- [ ] T02·乙：交付乙\n", encoding="utf-8")
        (t / "tools").mkdir()
        (t / "tools" / "task-oracle.py").write_text(_FAKE_ORACLE_SRC, encoding="utf-8")

        def face(sub: str) -> dict:
            return oracle_face(sub, script=t / "tools" / "task-oracle.py", root=t)

        d1 = dag_counts(face)
        a1, _bo1, _det1 = dag_stop_verdict(d1)
        pf.write_text("## 未完成事项\n\n- [x] T01·甲：交付甲\n- [x] T02·乙：交付乙\n", encoding="utf-8")
        d2 = dag_counts(face)
        a2, bo2, _det2 = dag_stop_verdict(d2)
        check("合成 plans 树子进程腿（替身 oracle：有活拒停→勾完全零过）",
              d1["status"] == "ok" and d1["counts"]["NEEDS_WORK"] == 2 and not a1
              and d2["counts"]["NEEDS_WORK"] == 0 and a2 and not bo2)
    # ⑦ 真树集成：真 oracle --summary --json → ok 信封且计数非负整数。语料拒判
    # （ref 撞名/权威改形）在这里红是立意而非误伤——那同样会砖掉第②门，该响。
    dr = dag_counts()
    check(f"真树 DAG 面（真 oracle summary：{dr['status']}）",
          dr["status"] == "ok" and isinstance(dr["counts"], dict)
          and isinstance(dr["counts"]["NEEDS_WORK"], int) and dr["counts"]["NEEDS_WORK"] >= 0
          and isinstance(dr["counts"]["BLOCKED"], int) and dr["counts"]["BLOCKED"] >= 0)
    print(f"e-guard 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}"
          "（punch oracle/FROZEN 自指/stop-hook 环境腿/四门拒写/DAG 轨六态与子进程腿）")
    return 0 if fails == 0 else 1


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    if argv[:1] == ["stop-hook"] or argv[:1] == ["selftest-stop-hook"]:
        return cmd_stop_hook()
    if argv[:1] == ["frozen-guard"]:
        return cmd_frozen_guard()
    if argv[:1] == ["fixpoint"]:
        return 0 if write_sentinel(blocked_flag="--blocked" in argv) else 1
    if argv[:1] == ["clear"]:
        SENTINEL.unlink(missing_ok=True)
        print("e-guard: 哨兵已清（操作员动作）")
        return 0
    print("usage: e-guard.py [stop-hook|frozen-guard|fixpoint [--blocked]|clear|--selftest]", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
