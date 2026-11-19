#!/usr/bin/env python3
"""饥饿界离线检查器（调点调度 1.1 §6）——配置期工具，不跑 QEMU。

公式（§6.3）：
- fair 同带无更高类：B_i = (Σw / w_i − 1) × slice
- 等权 n 任务：B = (n−1) × slice
- 有更高类：B_total = B_fair + B_upper；U_upper ≥ 1−δ → unbounded
- deadline/EDF：Σ(C_i/T_i) ≤ 1 − system_reserve → 可行/不可行

用法：
  python3 tools/starvation_check.py < config.json
  python3 tools/starvation_check.py --demo   # 内置示例

输出（§6.4 证书）：JSON——bound_ns / assumptions / unbounded / formula /
inputs_hash。
"""

import hashlib
import json
import sys


def check_fair_bound(tasks: list, slice_ns: int) -> dict:
    """fair 同带：B_i = (Σw / w_i − 1) × slice。"""
    total_w = sum(t["weight"] for t in tasks)
    bounds = {}
    for t in tasks:
        w = t["weight"]
        if w <= 0:
            return {"error": f"task {t['id']} weight={w} ≤ 0"}
        b_ns = int((total_w / w - 1) * slice_ns)
        bounds[str(t["id"])] = b_ns
    formula = "fair_general" if len(set(t["weight"] for t in tasks)) > 1 else "fair_eq_weight"
    return {"bounds_ns": bounds, "formula": formula}


def check_upper_class(upper_util: float, system_reserve: float) -> dict:
    """有更高类：U_upper ≥ 1−δ → unbounded。"""
    if upper_util >= 1.0 - system_reserve:
        return {"unbounded": True, "reason": f"U_upper={upper_util:.3f} ≥ 1−δ={1.0 - system_reserve:.3f}"}
    return {"unbounded": False, "upper_util": upper_util}


def check_edf_feasibility(deadline_tasks: list, system_reserve: float) -> dict:
    """EDF：Σ(C_i/T_i) ≤ 1 − system_reserve。"""
    total_u = sum(t["budget_ns"] / t["period_ns"] for t in deadline_tasks)
    feasible = total_u <= 1.0 - system_reserve
    return {
        "total_utilization": round(total_u, 6),
        "feasible": feasible,
        "limit": round(1.0 - system_reserve, 6),
    }


def run(config: dict) -> dict:
    slice_ns = config.get("slice_ns", 100_000_000)
    system_reserve = config.get("system_reserve", 0.05)
    classes = config.get("classes", [])

    result = {"slice_ns": slice_ns, "system_reserve": system_reserve}
    assumptions = ["weights stable during measurement"]

    # 分类：fair / deadline
    fair_classes = [c for c in classes if c.get("type", "fair") == "fair"]
    dl_tasks = [c for c in classes if c.get("budget_ns") and c.get("period_ns")]

    # fair 界
    all_fair_tasks = []
    for c in fair_classes:
        all_fair_tasks.extend(c.get("tasks", []))

    if all_fair_tasks:
        fair_result = check_fair_bound(all_fair_tasks, slice_ns)
        if "error" in fair_result:
            return fair_result
        result["fair"] = fair_result
        max_bound = max(fair_result["bounds_ns"].values())
        assumptions.append("no higher class" if len(classes) <= 1 else "higher class bounded")
    else:
        max_bound = 0

    # 更高类饱和检查
    upper_utils = [c.get("utilization", 0.0) for c in classes if c.get("utilization")]
    if upper_utils:
        upper_check = check_upper_class(max(upper_utils), system_reserve)
        result["upper"] = upper_check
        if upper_check.get("unbounded"):
            result["unbounded"] = True
            result["bound_ns"] = None
            result["reason"] = upper_check["reason"]
            result["assumptions"] = assumptions
            result["formula"] = "unbounded"
            result["inputs_hash"] = hash_config(config)
            return result
        assumptions.append("upper class utilization < 1−δ")

    # EDF 可行性
    if dl_tasks:
        edf = check_edf_feasibility(dl_tasks, system_reserve)
        result["edf"] = edf
        if not edf["feasible"]:
            result["unbounded"] = True
            result["bound_ns"] = None
            result["reason"] = f"EDF infeasible: ΣU={edf['total_utilization']} > {edf['limit']}"
            result["assumptions"] = assumptions
            result["formula"] = "edf_infeasible"
            result["inputs_hash"] = hash_config(config)
            return result
        assumptions.append("EDF feasible")

    # freeze 界外（§6.3）
    assumptions.append("frozen tasks excluded (administrative, out of bound)")

    result["unbounded"] = False
    result["bound_ns"] = max_bound
    result["assumptions"] = assumptions
    result["formula"] = result.get("fair", {}).get("formula", "none")
    result["inputs_hash"] = hash_config(config)
    return result


def hash_config(config: dict) -> str:
    """输入指纹（SHA-256 截断 16——CI 比对用）。"""
    raw = json.dumps(config, sort_keys=True).encode()
    return hashlib.sha256(raw).hexdigest()[:16]


def demo():
    """内置示例（§6.2 同形输入）。"""
    return {
        "slice_ns": 100_000_000,
        "classes": [
            {
                "name": "fair",
                "weight": 100,
                "tasks": [
                    {"id": 1, "weight": 100},
                    {"id": 2, "weight": 100},
                    {"id": 3, "weight": 200},
                ],
            }
        ],
        "system_reserve": 0.05,
    }


def selftest():
    """门禁自检（a-3 U4 纪律同款）——饥饿界证书产链的纯函数面钉板。

    此前只在 X-6 bench 对比轮里被一次性消费，门禁自身坏（界公式漂移/
    unbounded 误判/证书字段塌）= 饥饿证据静默劣化。覆盖：
    - fair 等权界 (n−1)×slice 与一般权界 (Σw/w_i −1)×slice 手算向量
    - 权比不变性（同倍缩放界不变）与 slice 线性
    - unbounded 两通道（上层饱和 / EDF 不可行）+ formula/exit-code 语义
    - 证书结构钉板（bound_ns/assumptions/formula/inputs_hash/JSON 序列化）
    - 拒面：非正权 / 空 classes / 非法 JSON
    """
    import subprocess

    failures = []

    def check(name, cond, detail=""):
        if cond:
            print(f"  ok {name}")
        else:
            failures.append(f"{name}: {detail}")
            print(f"  FAIL {name}: {detail}")

    # --- 1. fair 等权界：(n−1)×slice 手算向量 ---
    # n=4 等权 w=100，slice=100ms → B=(4−1)×100ms=300ms，人人相同。
    r = run({"slice_ns": 100_000_000, "classes": [
        {"name": "fair", "tasks": [
            {"id": i, "weight": 100} for i in range(1, 5)]}]})
    check("eq-weight bound (n-1)*slice",
          r["unbounded"] is False and r["bound_ns"] == 300_000_000
          and r["formula"] == "fair_eq_weight"
          and set(r["fair"]["bounds_ns"].values()) == {300_000_000},
          f"bound_ns={r.get('bound_ns')}")

    # n=2 等权 → B=slice（最坏等一轮）。
    r2 = run({"slice_ns": 50_000_000, "classes": [
        {"name": "fair", "tasks": [{"id": 7, "weight": 100}, {"id": 8, "weight": 100}]}]})
    check("eq-weight n=2 bound = slice",
          r2["bound_ns"] == 50_000_000, f"bound_ns={r2.get('bound_ns')}")

    # --- 2. 一般权界手算向量（demo 同形） ---
    # w = {1:100, 2:100, 3:200}，Σw=400，slice=100ms：
    # B_1 = (400/100 −1)×100ms = 300ms；B_3 = (400/200 −1)×100ms = 100ms。
    r3 = run(demo())
    b = r3["fair"]["bounds_ns"]
    check("general weights hand-computed",
          b["1"] == 300_000_000 and b["2"] == 300_000_000 and b["3"] == 100_000_000
          and r3["formula"] == "fair_general" and r3["unbounded"] is False,
          f"bounds={b}")

    # --- 3. 权比不变性 + slice 线性 ---
    # 全体权重 ×k 界不变（比例量）；slice ×2 界 ×2（线性量）。
    base = {"slice_ns": 100_000_000, "classes": [{"name": "f", "tasks": [
        {"id": "a", "weight": 3}, {"id": "b", "weight": 7}]}]}
    scaled = {"slice_ns": 100_000_000, "classes": [{"name": "f", "tasks": [
        {"id": "a", "weight": 30}, {"id": "b", "weight": 70}]}]}
    rb, rs = run(base), run(scaled)
    check("weight-scale invariance",
          rb["fair"]["bounds_ns"] == rs["fair"]["bounds_ns"],
          f"{rb['fair']['bounds_ns']} vs {rs['fair']['bounds_ns']}")
    doubled = json.loads(json.dumps(base))
    doubled["slice_ns"] = 200_000_000
    rd = run(doubled)
    check("slice linearity",
          rd["bound_ns"] == 2 * rb["bound_ns"],
          f"{rd['bound_ns']} vs {2 * rb['bound_ns']}")

    # --- 4. unbounded：上层饱和 ---
    # U_upper=0.96 ≥ 1−δ=0.95 → unbounded，formula=unbounded。
    ru = run({"classes": [{"name": "rt", "type": "rt", "utilization": 0.96}]})
    check("upper saturation unbounded",
          ru["unbounded"] is True and ru["bound_ns"] is None
          and ru["formula"] == "unbounded" and "U_upper" in ru["reason"],
          f"result={ru}")

    # 边界同侧：U_upper=0.949 < 0.95 → 不 unbounded（严格 < 才放行）。
    ru2 = run({"classes": [{"name": "rt", "type": "rt", "utilization": 0.949}]})
    check("upper just-below saturation bounded",
          ru2.get("unbounded") is False and ru2.get("bound_ns") == 0,
          f"result={ru2}")
    # 恰压线：U_upper == 1−δ（同式同浮点）→ unbounded（≥ 语义钉板，
    # 防 >= 退化成 > 的边界漂移）。
    ru3 = run({"system_reserve": 0.05,
               "classes": [{"name": "rt", "type": "rt",
                            "utilization": 1.0 - 0.05}]})
    check("upper exact-boundary saturation unbounded",
          ru3.get("unbounded") is True and ru3.get("formula") == "unbounded",
          f"result={ru3}")

    # --- 5. unbounded：EDF 不可行 ---
    # 两 deadline 类 C/T = 0.5+0.5 = 1.0 > 1−δ=0.95 → edf_infeasible。
    re = run({"classes": [
        {"name": "dl1", "type": "deadline",
         "budget_ns": 500_000, "period_ns": 1_000_000},
        {"name": "dl2", "type": "deadline",
         "budget_ns": 500_000, "period_ns": 1_000_000}]})
    check("edf infeasible flags unbounded",
          re["unbounded"] is True and re["formula"] == "edf_infeasible"
          and re["edf"]["total_utilization"] == 1.0 and re["edf"]["feasible"] is False,
          f"result={re}")
    # 刚好压线：ΣU=0.95 == 1−δ → feasible（≤ 语义钉板）。
    re2 = run({"classes": [{"name": "dl", "type": "deadline",
                            "budget_ns": 950_000, "period_ns": 1_000_000}]})
    check("edf boundary ΣU == limit feasible",
          re2["unbounded"] is False and re2["edf"]["feasible"] is True,
          f"result={re2}")

    # --- 6. 证书结构钉板 ---
    # demo 证书必须携带全字段且可 JSON 往返（smoke/CI 消费面）。
    cert = run(demo())
    for field in ("slice_ns", "system_reserve", "fair", "bound_ns",
                  "assumptions", "unbounded", "formula", "inputs_hash"):
        check(f"cert field {field}", field in cert, f"absent in {sorted(cert)}")
    check("assumptions non-empty & includes freeze note",
          len(cert["assumptions"]) >= 3
          and any("frozen" in a for a in cert["assumptions"]),
          f"{cert['assumptions']}")
    rt = json.loads(json.dumps(cert))
    check("cert JSON round-trip", rt["inputs_hash"] == cert["inputs_hash"],
          "hash mismatch after round-trip")

    # --- 7. inputs_hash 契约 ---
    # sort_keys 稳定指纹：键序无关同哈希；内容变则哈希变。
    h1 = hash_config(demo())
    h2 = hash_config({k: demo()[k] for k in reversed(list(demo()))})
    check("inputs_hash key-order invariant", h1 == h2, f"{h1} vs {h2}")
    d = demo(); d["slice_ns"] += 1
    check("inputs_hash content-sensitive", hash_config(d) != h1, "hash unchanged")

    # --- 8. 拒面 ---
    # 非正权：证书带 error（上层分支返回）。
    bad = run({"classes": [{"name": "f", "tasks": [
        {"id": 1, "weight": 0}, {"id": 2, "weight": 100}]}]})
    check("reject non-positive weight", "error" in bad and "weight" in bad["error"],
          f"result={bad}")
    neg = run({"classes": [{"name": "f", "tasks": [
        {"id": 1, "weight": -5}, {"id": 2, "weight": 100}]}]})
    check("reject negative weight", "error" in neg, f"result={neg}")
    # 空 classes：合法退路 bound_ns=0 / formula=none（不 crash 不误报）。
    empty = run({"classes": []})
    check("empty classes degenerate cert",
          empty["unbounded"] is False and empty["bound_ns"] == 0
          and empty["formula"] == "none" and empty["inputs_hash"],
          f"result={empty}")
    # 非法 JSON stdin：exit 1 + stderr error（main 入口语义，经管道真跑）。
    p = subprocess.run([sys.executable, __file__], input="{not json",
                       capture_output=True, text=True)
    check("malformed JSON exit 1 + stderr",
          p.returncode == 1 and "JSON parse" in p.stderr, f"rc={p.returncode}")
    # unbounded 证书 exit 2 / 正常 exit 0（main 退出码契约）。
    p = subprocess.run([sys.executable, __file__, "--demo"],
                       capture_output=True, text=True)
    check("--demo exit 0", p.returncode == 0 and '"unbounded": false' in p.stdout,
          f"rc={p.returncode}")
    p = subprocess.run([sys.executable, __file__], input=json.dumps(
        {"classes": [{"name": "rt", "type": "rt", "utilization": 0.99}]}),
        capture_output=True, text=True)
    check("unbounded exit 2", p.returncode == 2, f"rc={p.returncode}")

    print(f"\nstarvation_check selftest: {'PASS' if not failures else 'FAIL'}"
          f" ({len(failures)} failures)")
    return 0 if not failures else 1


def main():
    if "--selftest" in sys.argv:
        sys.exit(selftest())
    if "--demo" in sys.argv:
        config = demo()
    else:
        try:
            config = json.load(sys.stdin)
        except json.JSONDecodeError as e:
            print(json.dumps({"error": f"JSON parse: {e}"}), file=sys.stderr)
            sys.exit(1)

    cert = run(config)
    print(json.dumps(cert, indent=2))
    sys.exit(0 if not cert.get("unbounded", False) else 2)


if __name__ == "__main__":
    main()
