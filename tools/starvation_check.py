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


def main():
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
