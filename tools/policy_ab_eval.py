#!/usr/bin/env python3
"""策略 A/B 评估工具——shadow divergence 率对照 PROMOTE/DEMOTE 阈值。

shadow 对账口径（sched/policy.rs「Shadow divergence 对账」节）：
  divergence 率 = divergence_count / shadow_eval_count
  = 对账期内「shadow 会做的」≠「active 做了的」决策频率（每 pick 记分母，
  weight/EPP 任一差异记分子；promote/demote 轨道切换自动清零——读数即
  当前对账期）。

输入（stdin，两形态自动识别）：
  ① JSON 计数对：
       echo '{"divergence": 3, "eval": 120}' | python3 tools/policy_ab_eval.py
     键别名：divergence/divergence_count、eval/shadow_eval/shadow_eval_count；
     或单键 "packed"（内核 op=2 p3 原始 u64——本工具代为拆包）。
  ② QEMU 串口文本（内核 DV-K 遥测行 / STATUS 查询结果行）：
       PERF policy divergence=11 eval=12
       STATUS divergence=11 eval=12
       STATUS packed=0x0000000C0000000B   （bit 0-31 = 分子、bit 32-63 = 分母）
     多样本取**最后一条**（= 最近对账期读数）；--delta 取首尾差
     （窗口化评估：rate = (div2-div1)/(eval2-eval1)，轨道未切换时更准）。

判定阈值（A/B 评估判据，可 --promote/--demote 覆盖）：
  rate <  0.1 → PROMOTE（shadow 与 active 决策高度一致，可转正）
  rate >  0.5 → DEMOTE  （分歧过大，shadow 表不可信，回观察态）
  其间        → HOLD   （证据不足，继续累积对账期样本）

输出：单行 JSON {"divergence_rate", "recommendation", "threshold"}；
threshold = 触发判定所用阈值（HOLD = null）。exit 0 = 有判定；
1 = 输入无可用样本（对拍 sched_perf_analyze 口径）。
"""

import json
import re
import sys

# 判定阈值：PROMOTE 线（严格小于）/ DEMOTE 线（严格大于）。
PROMOTE_THRESHOLD = 0.1
DEMOTE_THRESHOLD = 0.5

# 整数字面量：串口十进制优先，0x 前缀十六进制兜底（packed 面）。
_INT_RE = r"(0[xX][0-9a-fA-F]+|\d+)"

# 计数对行：divergence=/eval= 同行出现（键别名宽容——shadow_eval/divergence_count）。
_DIV_RE = re.compile(r"\bdivergence(?:_count)?\s*=\s*" + _INT_RE)
_EVAL_RE = re.compile(r"(?:\bshadow_)?eval(?:uation)?(?:_count)?\s*=\s*" + _INT_RE)

# packed 单值行：内核 op=2 p3 原始 u64（bit 0-31 = 分子、bit 32-63 = 分母）。
_PACKED_RE = re.compile(r"\bpacked\s*=\s*" + _INT_RE)


def _to_int(s: str) -> int:
    """十进制优先（串口打印形），0x 前缀走十六进制。"""
    try:
        return int(s, 10)
    except ValueError:
        return int(s, 0)


def unpack(packed: int) -> tuple:
    """内核 p3 位布局拆包（镜像 syscall.rs policy_divergence_packed）：
    bit 0-31 = divergence_count、bit 32-63 = shadow_eval_count。"""
    return packed & 0xFFFF_FFFF, (packed >> 32) & 0xFFFF_FFFF


def parse_samples(text: str) -> list:
    """串口文本 → [(divergence, eval)]（保留时序，多样本供 --delta）。"""
    samples = []
    for line in text.splitlines():
        if "PERF policy" not in line and "STATUS" not in line:
            continue  # 只认两类载体行（PERF policy 遥测 / STATUS 查询结果）
        m = _PACKED_RE.search(line)
        if m:
            samples.append(unpack(_to_int(m.group(1))))
            continue
        d = _DIV_RE.search(line)
        e = _EVAL_RE.search(line)
        if d and e:
            samples.append((_to_int(d.group(1)), _to_int(e.group(1))))
    return samples


def parse_json(obj: dict) -> tuple:
    """JSON 计数对 → (divergence, eval)；packed 单键代拆。"""
    if "packed" in obj:
        return unpack(_to_int(str(obj["packed"])))
    d = obj.get("divergence", obj.get("divergence_count"))
    e = obj.get("eval", obj.get("shadow_eval", obj.get("shadow_eval_count")))
    if d is None or e is None:
        raise ValueError("JSON 缺 divergence/eval 键")
    return int(d), int(e)


def judge(div: int, evals: int, promote: float, demote: float) -> dict:
    """计数对 → 判定。evals == 0 = 对账期无样本：率不可算，HOLD（诚实透出）。"""
    if evals <= 0:
        return {"divergence_rate": None, "recommendation": "HOLD", "threshold": None}
    rate = round(div / evals, 4)
    if rate < promote:
        return {"divergence_rate": rate, "recommendation": "PROMOTE", "threshold": promote}
    if rate > demote:
        return {"divergence_rate": rate, "recommendation": "DEMOTE", "threshold": demote}
    return {"divergence_rate": rate, "recommendation": "HOLD", "threshold": None}


def main():
    args = sys.argv[1:]
    demo = "--demo" in args
    delta = "--delta" in args
    promote = PROMOTE_THRESHOLD
    demote = DEMOTE_THRESHOLD
    for i, a in enumerate(args):  # 阈值覆盖（--promote 0.05 / --demote 0.4）
        if a == "--promote" and i + 1 < len(args):
            promote = float(args[i + 1])
        if a == "--demote" and i + 1 < len(args):
            demote = float(args[i + 1])

    if demo:
        text = ("PERF policy base=26 empty=15 full=40 delta=8%\n"
                "STATUS packed=0x0000000000000030\n"
                "PERF policy divergence=3 eval=120\n")
        print(text, file=sys.stderr, end="")
    else:
        text = sys.stdin.read()

    # 形态识别：JSON 对象 → 单样本；否则按串口载体行解析。
    sample = None
    stripped = text.lstrip()
    if stripped.startswith("{"):
        try:
            sample = parse_json(json.loads(stripped))
        except (ValueError, json.JSONDecodeError) as ex:
            print(f"ERROR: bad JSON input: {ex}", file=sys.stderr)
            sys.exit(1)
    else:
        samples = parse_samples(text)
        if not samples:
            print("ERROR: no PERF policy / STATUS samples found", file=sys.stderr)
            sys.exit(1)
        if delta and len(samples) >= 2:  # 窗口化：首尾差（轨道未切换时更准）
            first, last = samples[0], samples[-1]
            sample = (last[0] - first[0], last[1] - first[1])
        else:
            sample = samples[-1]  # 缺省取最后一条 = 最近对账期读数

    verdict = judge(sample[0], sample[1], promote, demote)
    if verdict["divergence_rate"] is None:
        print("note: shadow_eval_count == 0（对账期无样本）→ HOLD", file=sys.stderr)
    print(json.dumps(verdict))


if __name__ == "__main__":
    main()
