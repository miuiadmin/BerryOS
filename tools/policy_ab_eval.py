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


def selftest():
    """门禁自检（a-3 U4 纪律同款）——A/B 升级判定链纯函数面钉板。

    本工具此前只在 S6 策略服务 A/B 升级工作流（策略服务用户指南 §3.2/§4③）
    里被一次性消费，门禁自身坏（阈值边界漂移/packed 拆包错位/载体行漏认/
    --delta 首尾差错序）= 升级判定静默劣化（该 PROMOTE 的 HOLD、该 DEMOTE
    的放行）。覆盖：
    - judge 三区间手算向量 + 严格边界（< 0.1 PROMOTE / > 0.5 DEMOTE /
      恰等阈值 = HOLD）+ evals==0 诚实 HOLD
    - unpack 位布局（bit 0-31 分子 / bit 32-63 分母，镜像内核 p3）
    - parse_samples 载体行族（PERF policy / STATUS / packed 0x / 键别名
      divergence_count / shadow_eval）+ 噪声行免疫 + 多样本取最后
    - parse_json 键别名与 packed 单键代拆
    - exit-code 契约（0 = 有判定 / 1 = 无样本）与 stdout 单行 JSON 形状
    - 端到端 --demo 固定样本（divergence=3 eval=120 → rate 0.025 → PROMOTE）
    - 子进程真跑（stdin pipe → 判定 JSON），非只测纯函数
    """
    import subprocess

    failures = []

    def check(name, cond, detail=""):
        if cond:
            print(f"  ok {name}")
        else:
            failures.append(f"{name}: {detail}")
            print(f"  FAIL {name}: {detail}")

    P, D = PROMOTE_THRESHOLD, DEMOTE_THRESHOLD

    # --- 1. judge 三区间手算向量（指南 §3.2 阈值语义） ---
    r = judge(3, 120, P, D)
    check("deep promote 3/120", r == {
        "divergence_rate": 0.025, "recommendation": "PROMOTE", "threshold": P},
        f"{r}")
    r = judge(11, 12, P, D)
    check("deep demote 11/12", r == {
        "divergence_rate": 0.9167, "recommendation": "DEMOTE", "threshold": D},
        f"{r}")
    r = judge(3, 12, P, D)
    check("hold midband 3/12", r == {
        "divergence_rate": 0.25, "recommendation": "HOLD", "threshold": None},
        f"{r}")

    # --- 2. 阈值严格边界（文档「严格小于/严格大于」语义） ---
    r = judge(1, 10, P, D)   # rate == 0.1 恰等 promote 线 → 不 PROMOTE
    check("rate==promote is HOLD", r["recommendation"] == "HOLD", f"{r}")
    r = judge(5, 10, P, D)   # rate == 0.5 恰等 demote 线 → 不 DEMOTE
    check("rate==demote is HOLD", r["recommendation"] == "HOLD", f"{r}")
    r = judge(0, 99, P, D)   # 零分歧 → 稳 PROMOTE（转正面）
    check("zero divergence PROMOTE", r["recommendation"] == "PROMOTE", f"{r}")

    # --- 3. evals==0 诚实面：无样本不装死也不误判 ---
    r = judge(0, 0, P, D)
    check("evals==0 honest HOLD", r == {
        "divergence_rate": None, "recommendation": "HOLD", "threshold": None},
        f"{r}")

    # --- 4. 阈值覆盖（--promote/--demote CLI 面） ---
    r = judge(2, 10, 0.25, D)  # 自定 promote=0.25：0.2 < 0.25 → PROMOTE
    check("custom promote threshold", r["recommendation"] == "PROMOTE"
          and r["threshold"] == 0.25, f"{r}")

    # --- 5. unpack 位布局（内核 op=2 p3 u64 镜像，syscall.rs packed 同源） ---
    d, e = unpack(0x000000780000000C)  # 高 32 位 120、低 32 位 12
    check("packed bit layout", (d, e) == (12, 120), f"{(d, e)}")
    d, e = unpack(0x0000000C0000000B)  # demo 样本：11/12
    check("packed demo sample", (d, e) == (11, 12), f"{(d, e)}")

    # --- 6. parse_samples 载体行族 + 噪声免疫 + 多样本取最后 ---
    text = ("random boot noise line\n"
            "PERF policy base=26 empty=15 full=40 delta=8%\n"
            "tele: task key=7 intent=1 wake=50 policy=0\n"   # 非载体行
            "STATUS divergence=11 eval=12\n"
            "PERF policy divergence=3 eval=120\n")
    s = parse_samples(text)
    check("carrier lines parsed", s == [(11, 12), (3, 120)], f"{s}")
    check("noise lines ignored", len(s) == 2, f"{s}")
    # 键别名宽容：divergence_count/shadow_eval/shadow_eval_count。
    s = parse_samples("STATUS divergence_count=4 shadow_eval=40\n"
                      "PERF policy divergence_count=1 shadow_eval_count=8\n")
    check("key aliases + last sample", s == [(4, 40), (1, 8)], f"{s}")
    # packed 行（0x 前缀十六进制）。
    s = parse_samples("STATUS packed=0x000000780000000C\n")
    check("packed serial line", s == [(12, 120)], f"{s}")

    # --- 7. parse_json 键别名与 packed 单键 ---
    check("json plain keys", parse_json({"divergence": 3, "eval": 120}) == (3, 120))
    check("json alias keys", parse_json(
        {"divergence_count": 4, "shadow_eval_count": 40}) == (4, 40))
    check("json packed key", parse_json({"packed": "0x0000000C0000000B"}) == (11, 12))
    check("json packed decimal", parse_json({"packed": "51539607563"}) == (11, 12))
    try:
        parse_json({"eval": 5})
        check("json missing key raises", False, "no exception")
    except ValueError:
        check("json missing key raises", True)

    # --- 8. 子进程端到端：stdin pipe → 单行 JSON + exit 0 ---
    p = subprocess.run([sys.executable, __file__],
                       input='{"divergence": 3, "eval": 120}',
                       capture_output=True, text=True)
    ok = p.returncode == 0
    lines = p.stdout.strip().splitlines()
    try:
        obj = json.loads(lines[-1])
        ok = ok and obj == {"divergence_rate": 0.025,
                            "recommendation": "PROMOTE", "threshold": 0.1}
    except (ValueError, IndexError):
        ok = False
    check("e2e stdin JSON verdict", ok, f"rc={p.returncode} out={p.stdout!r}")

    # 串口文本输入（真实消费形：qemu-run.log 直 pipe）。
    p = subprocess.run([sys.executable, __file__],
                       input="PERF policy divergence=11 eval=12\n",
                       capture_output=True, text=True)
    ok = p.returncode == 0 and '"DEMOTE"' in p.stdout
    check("e2e serial line DEMOTE", ok, f"rc={p.returncode} out={p.stdout!r}")

    # --delta 窗口化：首尾差 (3−1, 120−40) = (2, 80) → rate 0.025 PROMOTE。
    p = subprocess.run([sys.executable, __file__, "--delta"],
                       input="STATUS divergence=1 eval=40\n"
                             "PERF policy divergence=3 eval=120\n",
                       capture_output=True, text=True)
    ok = p.returncode == 0 and '"PROMOTE"' in p.stdout and "0.025" in p.stdout
    check("e2e --delta first-last window", ok,
          f"rc={p.returncode} out={p.stdout!r}")

    # exit 1 契约：无载体行的输入（对拍 sched_perf_analyze 口径）。
    p = subprocess.run([sys.executable, __file__],
                       input="nothing here\n",
                       capture_output=True, text=True)
    check("no-sample exit 1", p.returncode == 1 and "no PERF policy" in p.stderr,
          f"rc={p.returncode}")

    # --- 9. --demo 固定样本端到端（文档 §3.2 首例） ---
    p = subprocess.run([sys.executable, __file__, "--demo"],
                       capture_output=True, text=True)
    ok = p.returncode == 0 and '"divergence_rate": 0.025' in p.stdout \
        and '"PROMOTE"' in p.stdout
    check("--demo e2e PROMOTE", ok, f"rc={p.returncode} out={p.stdout!r}")

    # --- 10. 确定性：同输入两次 byte 级一致（shadow 评估前提） ---
    p1 = subprocess.run([sys.executable, __file__, "--demo"],
                        capture_output=True, text=True)
    check("deterministic byte-equal", p1.stdout == p.stdout,
          f"{p.stdout!r} vs {p1.stdout!r}")

    print(f"\npolicy_ab_eval selftest: {'PASS' if not failures else 'FAIL'}"
          f" ({len(failures)} failures)")
    return 0 if not failures else 1


def main():
    args = sys.argv[1:]
    if "--selftest" in args:
        sys.exit(selftest())
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
