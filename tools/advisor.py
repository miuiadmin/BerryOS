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


def main() -> int:
    args = sys.argv[1:]
    if "-h" in args or "--help" in args:
        print(__doc__)
        return 0
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
