#!/usr/bin/env python3
"""punch_slice.py —— punch-list 切分单源（section_slice 吸收；吸收缺口挖掘 P0）。

e-guard.py 的 open_punch_items 与 pack-work.py 的 split_punch 是同一份语义的
两份私有正则——「什么算 open」在门（fixpoint 判空）与包（开工项列表）各执
一词是 E lane 控制面正确性隐患（审计 r3/吸收挖掘双报）。本件是唯一实现：
节定位（## 未完成事项 → 下一 ## 或文末）、行分类（open / parked / done）。
  python3 tools/punch_slice.py [--punch <路径>]   # 打印 JSON 切分结果
  python3 tools/punch_slice.py --selftest
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_PUNCH = ROOT / ".agents" / "plans" / "2026-11-22-e面运营.md"

OPEN_RE = re.compile(r"^- \[ \]\s*(.*)$")
DONE_RE = re.compile(r"^- \[[xX]\]\s*(.*)$")
PARK_TOKEN = "[PARKED"


def slice_punch(text: str) -> dict:
    """→ {open: [...], parked: [...], done: [...], vehicle_present: bool}

    open = 未勾且不带 PARK 令牌；parked = 未勾但带 [PARKED→...]；done = 已勾。
    出节截止：只扫「## 未完成事项」节到下一个 ## 或文末。
    """
    m = re.search(r"## 未完成事项.*?\n(.*?)(?=\n## |\Z)", text, re.S)
    if not m:
        return {"open": [], "parked": [], "done": [], "vehicle_present": False}
    open_items: list[str] = []
    parked: list[str] = []
    done: list[str] = []
    for ln in m.group(1).splitlines():
        om = OPEN_RE.match(ln)
        if om:
            (parked if PARK_TOKEN in om.group(1) else open_items).append(om.group(1).strip())
        else:
            dm = DONE_RE.match(ln)
            if dm:
                done.append(dm.group(1).strip())
    return {"open": open_items, "parked": parked, "done": done, "vehicle_present": True}


def load(path: Path | None = None) -> str:
    p = path or DEFAULT_PUNCH
    return p.read_text(encoding="utf-8") if p.is_file() else ""


def validate_no_renumber(old_text: str, new_text: str) -> list[str]:
    """P2 加固（考古 2026-11-23）：punch 条目 ID 只增不挪——删了 T3 又新建 T3 = 重编号。
    返回违规列表（空=合法）。已勾/已 park 行永不删（Curia 永不删除）。"""
    old_ids = re.findall(r"- \[.\] (T\d+)", old_text)
    new_ids = re.findall(r"- \[.\] (T\d+)", new_text)
    # 旧文本有而新文本没有的 ID = 被删了
    deleted = set(old_ids) - set(new_ids)
    return [f"条目 {tid} 被删除——已勾/park 行永不删（Curia 永不删除），重编号用新 ID" for tid in sorted(deleted)]


def selftest() -> int:
    sample = (
        "## 未完成事项（punch-list）\n\n"
        "- [ ] 甲开放项\n"
        "- [ ] [PARKED→人审: 等裁决] 乙挂账\n"
        "- [x] 丙完成\n"
        "- [ ] [PARKED→人审: 等首跑] 丁远期\n"
        "\n## 验收标准\n"
        "- [ ] 别节的开放项（不得计入）\n"
    )
    r = slice_punch(sample)
    ok1 = r["open"] == ["甲开放项"] and len(r["parked"]) == 2 and r["done"] == ["丙完成"] and r["vehicle_present"]
    print(f"punch_slice 自检: {'腿绿' if ok1 else 'FAIL'} 切分（open/park/done/出节截止）")
    r2 = slice_punch("# 无该节的文档\n")
    ok2 = r2 == {"open": [], "parked": [], "done": [], "vehicle_present": False}
    print(f"punch_slice 自检: {'腿绿' if ok2 else 'FAIL'} 载体缺席判 vehicle_present=false")
    # P2 重编号检腿
    v = validate_no_renumber("- [ ] T1 甲\n- [x] T2 乙\n", "- [ ] T1 甲\n- [ ] T2 乙重开\n")
    ok3 = len(v) == 0  # 同 ID 在=合法
    v2 = validate_no_renumber("- [ ] T1 甲\n- [x] T2 乙\n", "- [ ] T1 甲\n")  # T2 被删
    ok4 = len(v2) == 1 and "T2" in v2[0]
    print(f"punch_slice 自检: {'腿绿' if ok3 and ok4 else 'FAIL'} 重编号检（同 ID 合法/删 ID 红）")
    rc = 0 if (ok1 and ok2 and ok3 and ok4) else 1
    print(f"punch_slice 自检：{'PASS' if rc == 0 else 'FAIL'}")
    return rc


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    path = Path(argv[argv.index("--punch") + 1]) if "--punch" in argv else None
    print(json.dumps(slice_punch(load(path)), ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
