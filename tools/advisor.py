#!/usr/bin/env python3
"""advisor 启发式策略服务——从 telemetry 生成 baseline 策略建议。

首个真实策略生成器（启发式 baseline）：遥测进、PolicyEntry 出。风格对齐
tools/policy_ab_eval.py——确定性、非交互、exit code 契约。

启发式规则（确定性——同输入同输出，可 diff 审计；逐条独立触发、效果叠加，
「公平默认、只叠加」）：
1. 交互型任务（intent ∈ {1,2} = 前台/交互）→ weight +16
2. 后台批处理（intent == 5 = maintenance 批处理档）→ weight -8
3. 高唤醒频率（wake_count > 100）→ weight +8（可能是等待型/IO 型）
4. deadline 任务（policy == 3）→ 不建议（RT 屏蔽——§3.4）
5. 内核任务（key == 0）→ 不建议（哨兵键）

输入（stdin，或位置参数给串口日志文件路径；两形态自动识别，--json 强制）：
  ① QEMU 串口遥测行（tele: 开头，k=v 空格分隔，十进制/0x 前缀均可）——
     只认带 key= 字段的 per-task 行，其余摘要行（tele: sw/wake/billing）自然忽略：
       tele: task key=7 intent=1 wake=50 policy=0
     同 key 多条取最后一条（= 最近读数，policy_ab_eval 同口径）；
     键别名 wake/wake_count/wakes；intent 缺省 0（UNSPECIFIED）、policy/wake 缺省 0。
  ② JSON 数组（单对象亦可）：
       [{"key": 7, "intent": 1, "wake": 50}, ...]

输出：PolicyEntry JSON 数组（字段名与序镜像内核 PolicyEntryABI：key/weight/
placement/epp_flags/_rsv，16B C 布局），按 key 升序（diff 审计友好），可直接
pipe 给 policy_bind COMMIT；weight==0 的任务不入表（空建议无绑定意义）。

护栏：
- 确定性：无随机、无时钟、固定规则序 + key 升序 tie-break——byte 级可复现
  （shadow 评估前提）。
- 保守：Σ|weight| < WEIGHT_MAX(64)——|weight| 降序（key 升序 tie-break）贪心
  准入，超预算任务丢弃并在 stderr 记 note（留 headroom 给叠加）。
- RT 屏蔽：policy=3（deadline）任务绝不出现（§3.4 护栏——RT/deadline 类屏蔽
  AI；policy 档 = sched_setattr 面 1 fair / 2 rr / 3 deadline / 4 idle）。

用法：
  qemu ... | python3 tools/advisor.py
  echo '[{"key": 7, "intent": 1, "wake": 50}, ...]' | python3 tools/advisor.py --json
  # 输出: [{"key": 7, "weight": 16, "placement": 0, "epp_flags": 0, "_rsv": 0}]

exit 0 = 有产出（空表 [] 亦合法——「接口在位、表空」）；1 = 输入无可用记录。
"""

import json
import re
import sys

# ---- 规则常量（本工具行为的单一事实源）----

# intent 六档（sched/src/intent.rs，#58）：0 UNSPECIFIED / 1 LATENCY_CRITICAL /
# 2 INTERACTIVE / 3 UTILITY / 4 BACKGROUND / 5 MAINTENANCE。
INTENT_INTERACTIVE = frozenset({1, 2})  # 前台/交互族（latency-critical + interactive）
INTENT_BATCH = 5                        # maintenance = 后台批处理档
INTENT_BOOST = 16                       # 规则 1：交互型加成
BG_PENALTY = -8                         # 规则 2：批处理降权
WAKE_BOOST = 8                          # 规则 3：高唤醒加成
WAKE_THRESHOLD = 100                    # 规则 3：门槛（严格大于）
POLICY_DEADLINE = 3                     # 规则 4：RT 屏蔽档位
KEY_SENTINEL = 0                        # 规则 5：哨兵键（policy.rs 0 键丢弃同口径）

# 内核执法包络镜像（sched/policy.rs WEIGHT_MAX——apply/weight_of 双点 clamp ±64；
# 本工具自限更紧：Σ|weight| 严格小于它，留 headroom 给后续叠加）。
WEIGHT_MAX = 64

# 整数字面量：串口十进制优先，0x 前缀十六进制兜底。
_INT_RE = r"(0[xX][0-9a-fA-F]+|\d+)"
_KV_RE = re.compile(r"(\w+)\s*=\s*(" + _INT_RE + r")")


def _to_int(v) -> int:
    """十进制优先（串口打印形），0x 前缀走十六进制；JSON int 直收。"""
    if isinstance(v, int):
        return v
    s = str(v).strip()
    try:
        return int(s, 10)
    except ValueError:
        return int(s, 0)


def _norm(kv: dict) -> dict:
    """原始键值 → 标准记录 {key, intent, wake, policy}（别名归一、缺省补 0）。"""
    wake = kv.get("wake", kv.get("wake_count", kv.get("wakes", 0)))
    return {
        "key": _to_int(kv.get("key", 0)),
        "intent": _to_int(kv.get("intent", 0)),
        "wake": _to_int(wake),
        "policy": _to_int(kv.get("policy", 0)),
    }


def parse_serial(text: str) -> dict:
    """串口文本 → {key: rec}。只认 tele: 前缀且带 key= 的 per-task 行
    （摘要行无 key= 自然跳过）；同 key 多条取最后一条 = 最近读数。"""
    recs = {}
    for line in text.splitlines():
        if "tele:" not in line:
            continue
        kv = {m.group(1): _to_int(m.group(2)) for m in _KV_RE.finditer(line)}
        if "key" not in kv:
            continue
        recs[kv["key"]] = _norm(kv)
    return recs


def parse_records(text: str, force_json: bool) -> tuple:
    """输入文本 → (records, bad_count)。形态识别：[/{ 开头 → JSON 数组/单对象；
    否则串口 tele: 行。--json 强制 JSON（坏 JSON 由调用方接错）。"""
    stripped = text.lstrip()
    if force_json or stripped.startswith("[") or stripped.startswith("{"):
        obj = json.loads(stripped)
        if isinstance(obj, dict):
            obj = [obj]
        recs, bad = {}, 0
        for item in obj:
            if not isinstance(item, dict) or "key" not in item:
                bad += 1
                continue
            try:
                rec = _norm(item)
            except (ValueError, TypeError):
                bad += 1
                continue
            recs[rec["key"]] = rec
        return recs, bad
    return parse_serial(text), 0


def advise(recs: dict) -> tuple:
    """规则评估 + headroom 贪心准入 → (entries, notes)。
    entries 按 key 升序（可 diff）；notes 全程确定性（审计面走 stderr）。"""
    notes, cands = [], []
    for key in sorted(recs):
        r = recs[key]
        if r["key"] == KEY_SENTINEL:
            notes.append("skip key=0: 哨兵键（内核任务）——不建议")
            continue
        if r["policy"] == POLICY_DEADLINE:
            notes.append(f"skip key={key}: policy=3 deadline → RT 屏蔽（§3.4）")
            continue
        w = 0
        if r["intent"] in INTENT_INTERACTIVE:
            w += INTENT_BOOST
        if r["intent"] == INTENT_BATCH:
            w += BG_PENALTY
        if r["wake"] > WAKE_THRESHOLD:
            w += WAKE_BOOST
        if w != 0:
            cands.append((key, w))
    # headroom 准入：|weight| 降序（强信号先拿预算）、key 升序 tie-break，
    # Σ|weight| 严格 < WEIGHT_MAX——超限弃收，note 透出（不静默截断）。
    total, admitted = 0, []
    for key, w in sorted(cands, key=lambda c: (-abs(c[1]), c[0])):
        if total + abs(w) >= WEIGHT_MAX:
            notes.append(
                f"headroom: key={key} weight={w:+d} 弃——Σ|weight| 将达 "
                f"{total + abs(w)} ≥ {WEIGHT_MAX}（留叠加余量）")
            continue
        total += abs(w)
        admitted.append((key, w))
    entries = [
        {"key": k, "weight": w, "placement": 0, "epp_flags": 0, "_rsv": 0}
        for k, w in sorted(admitted)
    ]
    notes.append(f"advised {len(entries)} 项，Σ|weight| = {total} < {WEIGHT_MAX}")
    return entries, notes


# --demo 固定样本：五规则全覆盖（哨兵键/RT 屏蔽/交互/交互+高唤醒叠加/批处理/
# 无命中）——期望表 [{7:+16}, {9:+24}, {12:-8}]，Σ|weight| = 48。
DEMO_INPUT = (
    "tele: task key=0 intent=0 wake=0 policy=0\n"
    "tele: task key=3 intent=0 wake=0 policy=3\n"
    "tele: task key=7 intent=1 wake=50 policy=0\n"
    "tele: task key=9 intent=2 wake=240 policy=0\n"
    "tele: task key=12 intent=5 wake=8 policy=0\n"
    "tele: task key=15 intent=3 wake=2 policy=0\n"
)


def selftest():
    """门禁自检（a-3 U4 纪律同款）——启发式策略生成器纯函数面钉板。

    本工具是首个真实策略生成器（遥测进、PolicyEntry 出），此前只有
    --demo 一次性展示，门禁自身坏（规则常量漂移/严格边界松成闭区间/
    键别名漏认/headroom 贪心序错/输出序漂移）= 下发策略静默劣化
    （该加成的没加成、该屏蔽的 RT 漏进表）。覆盖：
    - _to_int 十进制优先 + 0x 兜底；_norm 键别名（wake/wake_count/
      wakes）与缺省补 0
    - parse_serial：只认 tele: + key= 行、摘要行/噪声免疫、同 key 取
      最后一条（最近读数）、0x 前缀 k=v
    - parse_records：JSON 数组/单对象自动识别、坏记录计数、串口回退、
      --json 强制（坏 JSON exit 1）
    - advise 五规则手算向量和严格边界（wake 恰等 100 不加成、
      intent=2+高唤醒叠加 +24、policy=3 RT 屏蔽绝不出现、key=0 哨兵跳过、
      零命中空表）+ PolicyEntry 字段名/序镜像 + key 升序输出
    - headroom 贪心：Σ|weight| 严格 < 64（恰达 64 弃收）+ |weight| 降序
      key 升序 tie-break 手算向量
    - exit-code 契约（0 = 有产出含空表 / 1 = 无记录）与 stdout 单行 JSON
    - 端到端子进程真跑（stdin pipe / --json / --demo）+ byte 级确定性
      （shadow 评估前提）
    """
    import subprocess

    failures = []

    def check(name, cond, detail=""):
        if cond:
            print(f"  ok {name}")
        else:
            failures.append(f"{name}: {detail}")
            print(f"  FAIL {name}: {detail}")

    # --- 1. _to_int：十进制优先，0x 前缀走十六进制，int 直收 ---
    check("_to_int forms", [_to_int(v) for v in ("10", "0x10", "240", -8, 7)]
          == [10, 16, 240, -8, 7])
    check("_to_int hex case", _to_int("0X1F") == 31)

    # --- 2. _norm：键别名归一 + 缺省补 0 ---
    r = _norm({"key": 7, "wake_count": 240})
    check("_norm wake alias", r == {"key": 7, "intent": 0, "wake": 240,
                                    "policy": 0}, f"{r}")
    r = _norm({"key": 9, "wakes": 101, "intent": 2})
    check("_norm wakes alias + defaults", r["wake"] == 101 and r["policy"] == 0,
          f"{r}")
    r = _norm({"key": 4})
    check("_norm all defaults", r == {"key": 4, "intent": 0, "wake": 0,
                                      "policy": 0}, f"{r}")

    # --- 3. parse_serial：tele: + key= 行才收；同 key 取最后；噪声免疫 ---
    text = ("random boot noise line\n"
            "tele: sw=1234 wake=56 kl=0\n"            # 摘要行无 key= → 忽略
            "tele: task key=7 intent=1 wake=50 policy=0\n"
            "tele: task key=7 intent=3 wake=99 policy=0\n"   # 最近读数覆盖
            "tele: task key=12 intent=5 wake=8 policy=0\n")
    recs = parse_serial(text)
    check("serial last-one-wins", recs[7]["intent"] == 3
          and recs[7]["wake"] == 99, f"{recs.get(7)}")
    check("serial keys collected", sorted(recs) == [7, 12], f"{sorted(recs)}")
    recs = parse_serial("tele: task key=0x10 wake=0x78 intent=2 policy=0\n")
    check("serial hex k=v", recs[16]["wake"] == 120 and recs[16]["intent"] == 2,
          f"{recs}")
    check("serial noise immune", parse_serial("no tele prefix key=7\n") == {})

    # --- 4. parse_records：JSON 数组 / 单对象 / 坏记录 / 串口回退 ---
    recs, bad = parse_records('[{"key": 7, "intent": 1, "wake": 50},'
                              ' {"key": 7, "intent": 4},'
                              ' {"intent": 2}, "junk",'
                              ' {"key": 8, "wake": "abc"}]', False)
    check("json dup key last wins", recs[7]["intent"] == 4, f"{recs}")
    check("json bad records counted", bad == 3, f"bad={bad}")
    recs, bad = parse_records('{"key": 3, "policy": 3}', False)
    check("json single object autodetect", recs == {3: {"key": 3, "intent": 0,
          "wake": 0, "policy": 3}} and bad == 0, f"{recs}")
    recs, bad = parse_records("tele: task key=12 intent=5\n", False)
    check("serial fallback", recs[12]["intent"] == 5 and bad == 0, f"{recs}")

    # --- 5. advise 五规则手算向量 ---
    def one(**kv):
        base = {"key": 1, "intent": 0, "wake": 0, "policy": 0}
        base.update(kv)
        e, _ = advise({base["key"]: base})
        return e[0]["weight"] if e else 0

    check("rule1 intent=1 boost", one(intent=1) == 16)
    check("rule1 intent=2 boost", one(intent=2) == 16)
    check("rule1 intent=3 no boost", one(intent=3) == 0)
    check("rule2 batch penalty", one(intent=5) == -8)
    check("rule3 strict >100", one(wake=100) == 0 and one(wake=101) == 8)
    check("rule stacking +24", one(intent=2, wake=240) == 24)

    e, notes = advise({3: {"key": 3, "intent": 1, "wake": 999, "policy": 3}})
    check("rule4 RT shield", e == [] and any("deadline" in n for n in notes),
          f"{e} {notes}")
    e, notes = advise({0: {"key": 0, "intent": 1, "wake": 500, "policy": 0}})
    check("rule5 sentinel skip", e == [] and any("哨兵" in n for n in notes),
          f"{e} {notes}")
    e, notes = advise({6: {"key": 6, "intent": 3, "wake": 2, "policy": 0}})
    check("no-hit empty table", e == [] and notes, f"{e} {notes}")

    # --- 6. 输出契约：字段名/序镜像 PolicyEntry + key 升序 ---
    e, _ = advise({21: {"key": 21, "intent": 1, "wake": 0, "policy": 0},
                   5: {"key": 5, "intent": 5, "wake": 0, "policy": 0},
                   12: {"key": 12, "intent": 2, "wake": 9, "policy": 0}})
    check("key ascending order", [x["key"] for x in e] == [5, 12, 21], f"{e}")
    check("field layout mirror", all(list(x.keys()) == ["key", "weight",
          "placement", "epp_flags", "_rsv"] for x in e), f"{e}")
    check("static fields zero", all(x["placement"] == 0 and x["epp_flags"] == 0
          and x["_rsv"] == 0 for x in e), f"{e}")

    # --- 7. headroom 贪心：严格 <64 + |w| 降序 key 升序 tie-break ---
    recs = {k: {"key": k, "intent": 1, "wake": 0, "policy": 0}
            for k in (1, 2, 3, 4)}                      # 4×16：前三 48，第四弃
    e, notes = advise(recs)
    check("headroom strict <64", [x["key"] for x in e] == [1, 2, 3]
          and any("headroom" in n and "key=4" in n for n in notes),
          f"{e} {notes}")
    recs = {20: {"key": 20, "intent": 1, "wake": 0, "policy": 0},    # +16
            5: {"key": 5, "intent": 1, "wake": 0, "policy": 0},      # +16
            30: {"key": 30, "intent": 2, "wake": 240, "policy": 0},  # +24
            9: {"key": 9, "intent": 5, "wake": 0, "policy": 0},      # -8
            40: {"key": 40, "intent": 1, "wake": 101, "policy": 0}}  # +24
    e, notes = advise(recs)
    # 贪心序 30(+24)→40(+24)→5(+16)→20(+16)→9(-8)：48 帽下 5/20 弃、9 收
    check("headroom tie-break vector",
          e == [{"key": 9, "weight": -8, "placement": 0, "epp_flags": 0,
                 "_rsv": 0},
                {"key": 30, "weight": 24, "placement": 0, "epp_flags": 0,
                 "_rsv": 0},
                {"key": 40, "weight": 24, "placement": 0, "epp_flags": 0,
                 "_rsv": 0}], f"{e}")

    # --- 8. 子进程端到端：stdin pipe → 单行 JSON + exit 0 ---
    p = subprocess.run([sys.executable, __file__],
                       input="tele: task key=7 intent=1 wake=50 policy=0\n",
                       capture_output=True, text=True)
    try:
        obj = json.loads(p.stdout.strip().splitlines()[-1])
        ok = p.returncode == 0 and obj == [{"key": 7, "weight": 16,
                                            "placement": 0, "epp_flags": 0,
                                            "_rsv": 0}]
    except (ValueError, IndexError):
        ok = False
    check("e2e serial stdin", ok, f"rc={p.returncode} out={p.stdout!r}")

    p = subprocess.run([sys.executable, __file__],
                       input='[{"key": 7, "intent": 1, "wake": 50}]',
                       capture_output=True, text=True)
    ok = p.returncode == 0 and json.loads(p.stdout) == [{"key": 7,
          "weight": 16, "placement": 0, "epp_flags": 0, "_rsv": 0}]
    check("e2e json autodetect", ok, f"rc={p.returncode} out={p.stdout!r}")

    p = subprocess.run([sys.executable, __file__, "--json"],
                       input='[{"key": 9, "intent": 2, "wake": 240}]',
                       capture_output=True, text=True)
    ok = p.returncode == 0 and json.loads(p.stdout)[0]["weight"] == 24
    check("e2e --json forced", ok, f"rc={p.returncode} out={p.stdout!r}")

    p = subprocess.run([sys.executable, __file__, "--json"],
                       input="not json at all", capture_output=True, text=True)
    check("e2e bad json exit 1", p.returncode == 1
          and "bad JSON" in p.stderr, f"rc={p.returncode}")

    # 空表契约：有记录但零命中 → exit 0 + []（接口在位、表空）。
    p = subprocess.run([sys.executable, __file__],
                       input="tele: task key=6 intent=3 wake=2 policy=0\n",
                       capture_output=True, text=True)
    check("e2e empty table exit 0", p.returncode == 0
          and p.stdout.strip() == "[]", f"rc={p.returncode} out={p.stdout!r}")

    # RT-only 输入：有记录、全屏蔽 → 仍 exit 0 空表 + stderr 有 skip note。
    p = subprocess.run([sys.executable, __file__],
                       input='[{"key": 3, "intent": 1, "policy": 3}]',
                       capture_output=True, text=True)
    check("e2e RT-only empty", p.returncode == 0
          and p.stdout.strip() == "[]" and "deadline" in p.stderr,
          f"rc={p.returncode} err={p.stderr!r}")

    p = subprocess.run([sys.executable, __file__],
                       input="nothing recognizable\n",
                       capture_output=True, text=True)
    check("no-records exit 1", p.returncode == 1
          and "no tele: task" in p.stderr, f"rc={p.returncode}")

    # --- 9. --demo 固定样本：[{7:+16},{9:+24},{12:-8}]，Σ=48 ---
    p = subprocess.run([sys.executable, __file__, "--demo"],
                       capture_output=True, text=True)
    want = [{"key": 7, "weight": 16, "placement": 0, "epp_flags": 0,
             "_rsv": 0},
            {"key": 9, "weight": 24, "placement": 0, "epp_flags": 0,
             "_rsv": 0},
            {"key": 12, "weight": -8, "placement": 0, "epp_flags": 0,
             "_rsv": 0}]
    ok = p.returncode == 0 and json.loads(p.stdout) == want \
        and "Σ|weight| = 48" in p.stderr and p.stderr.startswith("tele:")
    check("--demo fixed table", ok, f"rc={p.returncode} out={p.stdout!r}")

    # --- 10. 确定性：同输入两次 byte 级一致（shadow 评估前提） ---
    p1 = subprocess.run([sys.executable, __file__, "--demo"],
                        capture_output=True, text=True)
    check("deterministic byte-equal", p1.stdout == p.stdout
          and p1.stderr == p.stderr, "stdout/stderr drifted")

    print(f"\nadvisor selftest: {'PASS' if not failures else 'FAIL'}"
          f" ({len(failures)} failures)")
    return 0 if not failures else 1


def main() -> int:
    args = sys.argv[1:]
    if "-h" in args or "--help" in args:
        print(__doc__)
        return 0
    if "--selftest" in args:
        sys.exit(selftest())
    demo = "--demo" in args
    force_json = "--json" in args
    path = next((a for a in args if not a.startswith("-")), None)

    if demo:  # demo 固定串口样本（--json 不生效——样本本身是 tele: 形态）
        print(DEMO_INPUT, file=sys.stderr, end="")
        text, force_json = DEMO_INPUT, False
    elif path:
        with open(path, "r", errors="replace") as f:
            text = f.read()
    else:
        text = sys.stdin.read()

    try:
        recs, bad = parse_records(text, force_json)
    except (ValueError, json.JSONDecodeError) as ex:
        print(f"ERROR: bad JSON input: {ex}", file=sys.stderr)
        return 1
    if bad:
        print(f"note: {bad} 条记录缺 key/非整数值——丢弃", file=sys.stderr)
    if not recs:
        print("ERROR: no tele: task / JSON records found", file=sys.stderr)
        return 1

    entries, notes = advise(recs)
    for n in notes:
        print(f"advisor: {n}", file=sys.stderr)
    print(json.dumps(entries))
    return 0


if __name__ == "__main__":
    sys.exit(main())
