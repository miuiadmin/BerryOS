#!/usr/bin/env python3
"""pack-work.py —— E lane 开工包拼装器（自动化开发二期 P0；规则/16 §5）。

lane 醒来第一个动作跑本件（零模型成本），拿到一份 JSON 开工包，把 token 花在
干活而非重导状态——对应 impossible-os 的 advance-work + section-pack：
  punch_list   活跃计划「未完成事项」节的开放条目（park 单列，带理由）
  dag          task-oracle next-buildable 面：当前可建造节点前 5 个（ref=节
               地址、deliverable=交付物）——DAG 活优先于 punch-list，punch-list
               是兜底（盲取 punch 首项可能拿到依赖未齐的条目，DAG 面只给依赖
               已齐的）；oracle 缺席/故障时降级记 error（fail-open，不停跑）
  facts        tools/facts.txt 全键（计数自证面）
  git          分支/领先上游笔数/脏文件数
  gates        门禁三件的快速现状（存在性而非真跑——真跑归提交流）
  stamps       已双戳的 design.md 数（棘轮位）
  hint         下一动作建议（DAG 有活=指名 DAG 首项；否则 punch 第一开放项；
               全 park=问 fixpoint）
只读。python3 tools/pack-work.py --selftest 用合成夹具验拼包纯函数（含注入式
oracle 面与合成 plans 树替身子进程腿）。

task-oracle 消费契约（tools/task-oracle.py 已落库的现行 CLI；分类语义唯一家
=task-oracle——本件只做进程与 JSON 管道，判定不许在此再造第二份，学
punch_slice 单源律）：
  python3 tools/task-oracle.py --next-buildable
      → stdout JSON {"status": WORK|BLOCKED|DONE|EMPTY, "counts": {…},
                     "buildable": [{"ref", "file", "n", "line", "heading"}, …],
                     "waiting": […], "graph": {…}}
  rc：0=判成（含诚实的 DONE/BLOCKED/EMPTY 终态）；1=目标文档不在；2=拒判或
  用法错（状态载不入，宁拒不猜）；其余/超时/坏 JSON=规划故障。
  消费侧 fail-open（设计律「规划故障不停跑」）：任何非 ok 都降级回 punch 单轨，
  原因记进 dag.status/dag.detail——绝不假装 DAG 清零，也绝不挡开工。
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PUNCH_PLAN = ROOT / ".agents" / "plans" / "2026-11-22-e面运营.md"

OPEN_RE = re.compile(r"^- \[ \]\s*(.*)$")
PARK_TOKEN = "[PARKED"

# 车道约定：开工包只带前 5 个可建造节点。切片在消费侧做——不依赖 oracle 的
# --limit 旗标面，oracle 只要给全量列表本件就能自取所需。
BUILDABLE_CAP = 5

# task-oracle 的子命令面是旗标式（非 positional）：next-buildable 独旗；summary
# 要 --json 才出机器面（无 --json 是给人读的逐文档行）。为什么映射表在消费侧：
# 两个消费面（pack-work/e-guard）同文各持一份管道，命令面漂移在这里一眼对齐。
_ORACLE_ARGS = {
    "next-buildable": ["--next-buildable"],
    "summary": ["--summary", "--json"],
}

# 替身 oracle（--selftest 专用）：真读合成 plans 树里「- [ ] TNN·」条目派生
# buildable——让子进程腿测的是「合成树→替身→本件管道」整链而非死字符串；
# 旗标面镜像真 oracle（--next-buildable）。
_FAKE_ORACLE_SRC = (
    "import json, sys\n"
    "from pathlib import Path\n"
    "root = Path(__file__).resolve().parent.parent\n"
    "items = []\n"
    "if '--next-buildable' in sys.argv[1:]:\n"
    "    for p in sorted((root / '.agents' / 'plans').glob('*.md')):\n"
    "        for ln in p.read_text(encoding='utf-8').splitlines():\n"
    "            s = ln.strip()\n"
    "            if s.startswith('- [ ] T'):\n"
    "                head, _, rest = s[6:].partition('：')\n"
    "                items.append({'ref': p.stem + ' ' + head, 'deliverable': rest.strip()})\n"
    "    print(json.dumps({'buildable': items}, ensure_ascii=False))\n"
)


def split_punch(text: str) -> tuple[list[str], list[str]]:
    m = re.search(r"## 未完成事项.*?\n(.*?)(?=\n## |\Z)", text, re.S)
    open_items, parked = [], []
    if not m:
        return open_items, parked
    for ln in m.group(1).splitlines():
        mm = OPEN_RE.match(ln)
        if not mm:
            continue
        (parked if PARK_TOKEN in mm.group(1) else open_items).append(mm.group(1).strip())
    return open_items, parked


def sh(cmd: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=60, cwd=str(ROOT))


def oracle_face(subcmd: str, script: Path | None = None, root: Path | None = None) -> dict:
    """task-oracle 子进程面 → 状态信封 {status, data, detail}（管道工，非语义）。

    status 四态：ok=答了（data=stdout JSON）；absent=件不在；refused=rc 2 拒判
    或用法错（状态载不入——oracle 自己的 fail-closed 面）；error=其余非零/超时/
    坏 JSON。为什么「缺席」要单独成态：装配/搬场期 oracle 不在是常态而非事故，
    消费面必须能区分「还没有这件」（过渡，fail-open）与「这件坏了」（记 error
    待修）。
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


def parse_buildable(data) -> list[dict]:
    """next-buildable 应答 → [{ref, deliverable}]（截前 5 个）。

    条目缺 ref 整条弃——没有节地址的节点指不了路。deliverable 来源优先级：
    deliverable/delivery/task/title 键 > heading——真 oracle 现行给的是节标题
    （heading），交付物列的机器面还没立法，标题是诚实的最近似描述，不硬造。
    """
    seq = None
    if isinstance(data, list):
        seq = data
    elif isinstance(data, dict):
        for k in ("buildable", "next", "items", "nodes"):
            v = data.get(k)
            if isinstance(v, list):
                seq = v
                break
    out: list[dict] = []
    for it in seq or []:
        if not isinstance(it, dict):
            continue
        ref = next((str(it[k]) for k in ("ref", "addr", "node", "id") if it.get(k)), "")
        if not ref:
            continue
        deliver = next((str(it[k]) for k in ("deliverable", "delivery", "task", "title", "heading")
                        if it.get(k)), "")
        out.append({"ref": ref, "deliverable": deliver})
    return out[:BUILDABLE_CAP]


def dag_field(face) -> dict:
    """开工包 DAG 面。face=(subcmd)->状态信封——注入点是给 --selftest 的。"""
    r = face("next-buildable")
    if r["status"] == "ok":
        return {"present": True, "buildable": parse_buildable(r.get("data")),
                "status": "ok", "detail": ""}
    return {"present": False, "buildable": [], "status": r["status"],
            "detail": r.get("detail") or r["status"]}


def build_pack(root: Path = ROOT, punch_text: str | None = None, oracle=None) -> dict:
    """oracle：task-oracle 消费面注入点（(subcmd) -> 状态信封）；None=真子进程面。"""
    open_items, parked = split_punch(punch_text if punch_text is not None else
                                     (PUNCH_PLAN.read_text(encoding="utf-8") if PUNCH_PLAN.is_file() else ""))
    dag = dag_field(oracle or oracle_face)
    facts = {}
    fp = root / "tools" / "facts.txt"
    if fp.is_file():
        for ln in fp.read_text(encoding="utf-8").splitlines():
            m = re.fullmatch(r"([a-z0-9_]+)=(.+)", ln)
            if m:
                facts[m.group(1)] = m.group(2)
    br = sh(["git", "rev-parse", "--abbrev-ref", "HEAD"]).stdout.strip() or "?"
    ahead = len(sh(["git", "rev-list", "--count", "@{u}..HEAD"]).stdout.split()) and \
        sh(["git", "rev-list", "--count", "@{u}..HEAD"]).stdout.strip() or "0"
    dirty = len(sh(["git", "status", "--porcelain"]).stdout.splitlines())
    stamped = 0
    for d in (root / ".agents" / "plans").glob("*/design.md"):
        if re.search(r"^>\s*Verified:", d.read_text(encoding="utf-8"), re.M):
            stamped += 1
    # hint 三级优先（车道律：有 DAG 活时优先干 DAG 活，punch-list 是兜底）：
    # DAG 可建造首项 > punch-list 第一开放项 > 问 fixpoint。
    if dag["present"] and dag["buildable"]:
        b0 = dag["buildable"][0]
        hint = (f"干 DAG 首项 {b0['ref']}：{b0['deliverable'] or '交付物见计划文档'}"
                "——验证过门再 commit（同批刷 facts）")
    elif open_items:
        hint = "干 punch-list 第一开放项：验证过门再 commit（同批刷 facts）"
    else:
        hint = "无开放项——先核对 [PARKED] 是否已解，再跑 python3 tools/e-guard.py fixpoint"
        if dag.get("status") in ("error", "refused"):
            # 降级可见性：punch 也空而 DAG 读不出时，操作者得知道为什么不是双轨收口
            hint += f"（DAG 轨降级 {dag['status']}：{dag.get('detail') or ''}——修 task-oracle 后双轨收口）"
    return {"punch_list": {"open": open_items, "parked": parked}, "dag": dag, "facts": facts,
            "git": {"branch": br, "ahead": ahead, "dirty_files": dirty},
            "stamped_designs": stamped, "hint": hint}


def selftest() -> int:
    sample = "## 未完成事项\n\n- [ ] 甲项\n- [ ] [PARKED→人审: 等Owner] 乙项\n- [x] 丙项\n\n## 其他\n- [ ] 别节\n"
    o, p = split_punch(sample)
    ok = (o == ["甲项"] and len(p) == 1 and "乙项" in p[0])
    print(f"pack-work 自检: {'腿绿' if ok else 'FAIL'} punch 切分（open/park/出节）")
    pack = build_pack(punch_text=sample)
    ok2 = pack["punch_list"]["open"] == ["甲项"] and "hint" in pack and "facts" in pack
    print(f"pack-work 自检: {'腿绿' if ok2 else 'FAIL'} 开工包形状（含 hint/facts）")
    # DAG 面腿 1（真树集成）：真 oracle 在场——验信封完整性与 hint 一致性
    # （有 buildable → 指名首项；无 → punch 兜底）；不钉死语料状态（节数会动）。
    dg = pack["dag"]
    ok2b = (isinstance(dg["present"], bool) and isinstance(dg["buildable"], list)
            and len(dg["buildable"]) <= BUILDABLE_CAP
            and all(isinstance(b.get("ref"), str) and b["ref"] for b in dg["buildable"])
            and ((dg["present"] and dg["buildable"] and dg["buildable"][0]["ref"] in pack["hint"])
                 or ("第一开放项" in pack["hint"])))
    print(f"pack-work 自检: {'腿绿' if ok2b else 'FAIL'} 真树 DAG 面（真 oracle 信封完整 + hint 指名一致）")
    # DAG 面腿 2（缺席注入）：oracle 不在=搬场/卸载过渡态 → fail-open 降级，既有面不动
    def face_absent(sub: str) -> dict:
        return oracle_face(sub, script=Path("/nonexistent/task-oracle.py"))

    pack_a = build_pack(punch_text=sample, oracle=face_absent)
    ok3 = (not pack_a["dag"]["present"]) and pack_a["dag"]["buildable"] == [] and "第一开放项" in pack_a["hint"]
    print(f"pack-work 自检: {'腿绿' if ok3 else 'FAIL'} DAG 缺席降级（fail-open 回 punch 兜底）")

    # DAG 面腿 2（注入）：ok 应答——前 5 切片 + hint 指名 DAG 首项（优先于 punch）
    def face_ok(sub: str) -> dict:
        if sub != "next-buildable":
            return {"status": "ok", "data": {}, "detail": ""}
        buildable = [{"ref": "合成计划 T01", "deliverable": "甲交付"},
                     {"ref": "合成计划 T02", "deliverable": "乙交付"}] + \
                    [{"ref": f"合成计划 T{n:02d}", "deliverable": f"丁交付{n}"} for n in range(3, 9)]
        return {"status": "ok", "data": {"buildable": buildable}, "detail": ""}

    pack2 = build_pack(punch_text=sample, oracle=face_ok)
    d = pack2["dag"]
    ok4 = (d["present"] and len(d["buildable"]) == 5 and d["buildable"][0]["ref"] == "合成计划 T01"
           and "合成计划 T01" in pack2["hint"] and "甲交付" in pack2["hint"])
    print(f"pack-work 自检: {'腿绿' if ok4 else 'FAIL'} DAG 注入面（前 5 切片 + hint 指名首项优先于 punch）")

    # DAG 面腿 3（边界）：oracle 在但无可建造 → punch 兜底；故障 → 降级记因不挡开工
    pack3 = build_pack(punch_text=sample,
                       oracle=lambda sub: {"status": "ok", "data": {"buildable": []}, "detail": ""})
    ok5 = pack3["dag"]["present"] and pack3["dag"]["buildable"] == [] and "第一开放项" in pack3["hint"]
    pack4 = build_pack(punch_text=sample,
                       oracle=lambda sub: {"status": "error", "data": None, "detail": "炸了"})
    ok6 = (not pack4["dag"]["present"]) and pack4["dag"]["status"] == "error" \
        and "第一开放项" in pack4["hint"]
    print(f"pack-work 自检: {'腿绿' if ok5 and ok6 else 'FAIL'} DAG 空集回 punch 兜底 / 故障降级记因")

    # DAG 面腿 4（子进程）：合成 plans 树 + 替身 oracle——验真管道（spawn→stdout JSON→切片）
    # 与 rc=2 拒判信封（refused 不混进 error——两态的消费语义不同：拒停门上前者更硬）。
    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        (t / ".agents" / "plans").mkdir(parents=True)
        (t / ".agents" / "plans" / "2099-01-01-合成.md").write_text(
            "## 未完成事项\n\n- [ ] T01·甲：交付甲\n- [ ] T02·乙：交付乙\n- [x] T00·丙：已毕\n",
            encoding="utf-8")
        (t / "tools").mkdir()
        (t / "tools" / "task-oracle.py").write_text(_FAKE_ORACLE_SRC, encoding="utf-8")
        r = oracle_face("next-buildable", script=t / "tools" / "task-oracle.py", root=t)
        b = parse_buildable(r["data"]) if r["status"] == "ok" else []
        ok7 = (r["status"] == "ok"
               and [x["ref"] for x in b] == ["2099-01-01-合成 T01·甲", "2099-01-01-合成 T02·乙"]
               and b[0]["deliverable"] == "交付甲")
        (t / "tools" / "task-oracle.py").write_text(
            "import sys; sys.stderr.write('状态载不入'); sys.exit(2)\n", encoding="utf-8")
        r2 = oracle_face("next-buildable", script=t / "tools" / "task-oracle.py", root=t)
        ok7 = ok7 and r2["status"] == "refused" and "状态载不入" in r2["detail"]
    print(f"pack-work 自检: {'腿绿' if ok7 else 'FAIL'} 合成 plans 树子进程腿（替身 oracle 管道 + rc=2 拒判信封）")
    rc = 0 if all((ok, ok2, ok2b, ok3, ok4, ok5, ok6, ok7)) else 1
    print(f"pack-work 自检：{'PASS' if rc == 0 else 'FAIL'}")
    return rc


def debt_sweep() -> dict:
    """三张 advisory 债网（stale-parks/stranded-deferrals/answers-readback）——醒来即见死账清单。

    rc 恒 0（advisory 立法）；单件超时 30s、失败记 error 不致命（fail-open：债网缺席不挡开工）。
    """
    out = {}
    for name in ("stale-parks", "stranded-deferrals", "answers-readback"):
        try:
            r = subprocess.run([sys.executable, "-I", f"tools/{name}.py"],
                               capture_output=True, text=True, timeout=30, cwd=str(ROOT))
            lines = [ln for ln in (r.stdout or "").splitlines() if ln.strip()]
            out[name] = lines[:10] if lines else []
        except Exception as e:  # noqa: BLE001 —— advisory 面宽捕，缺席记档不致命
            out[name] = [f"error: {e}"]
    return out


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    pack = build_pack()
    if "--sweep" in argv:
        pack["debt_sweep"] = debt_sweep()
    print(json.dumps(pack, ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
