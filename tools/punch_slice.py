#!/usr/bin/env python3
"""punch_slice.py —— punch-list 切分单源（section_slice 吸收；吸收缺口挖掘 P0）。

e-guard.py 的 open_punch_items 与 pack-work.py 的 split_punch 是同一份语义的
两份私有正则——「什么算 open」在门（fixpoint 判空）与包（开工项列表）各执
一词是 E lane 控制面正确性隐患（审计 r3/吸收挖掘双报）。本件是唯一实现：
节定位（## 未完成事项 → 下一 ## 或文末）、行分类（open / parked / done）。
  python3 tools/punch_slice.py [--punch <路径>]   # 打印 JSON 切分结果
  python3 tools/punch_slice.py --renumber-guard   # 提交门接线口：HEAD 版 vs 暂存版
                                                   # 重排检（pre-commit 调；见函数注）
  python3 tools/punch_slice.py --selftest
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_PUNCH = ROOT / ".agents" / "plans" / "2026-11-22-e面运营.md"

OPEN_RE = re.compile(r"^- \[ \]\s*(.*)$")
DONE_RE = re.compile(r"^- \[[xX]\]\s*(.*)$")
PARK_TOKEN = "[PARKED"
# 行级 punch 条目（状态 + 正文）；T 编号从正文头提取（`T12·说明` 新语法）
ROW_RE = re.compile(r"^- \[( |x|X)\]\s*(.*)$")
TID_RE = re.compile(r"^(T\d+)")
# park 令牌归一化：owner 解 park（删令牌改回普通 [ ]）是文档化合法流——指纹
# 对比前剥掉令牌，防合法解 park 被误判成删行重建。
PARK_STRIP_RE = re.compile(r"\[PARKED→[^\]]*\]\s*")


def _norm_body(body: str) -> str:
    return PARK_STRIP_RE.sub("", body).strip()


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
    """P2 加固（考古 2026-11-23；2026-11-23 终审接线+升格）：punch 条目 ID 只增
    不挪——删了 T3 又新建 T3 = 重编号。返回违规列表（空=合法）。

    三层判据（终审 P1：旧集合差实现检不出同 ID 重建，且对无 T 编号的现行
    punch 格式恒空判）：
      ① 任一 T 编号在旧文在场、新文缺席 = 删条目（已勾/open 一视——编号只增）；
      ② 已勾/park 的 T 条目在新文仍已勾/park 但正文变了 = 删后重建绕编号闸
        （合法重开=新文状态翻回 open，不触发本判）；
      ③ 已勾/park 的无编号条目按归一化整行指纹 fallback（剥 park 令牌）：
        旧正文在新文任何行都不在场 = 删行。
    """
    def rows(text: str) -> list[tuple[bool, str | None, str]]:
        out = []
        for ln in text.splitlines():
            m = ROW_RE.match(ln)
            if m:
                raw = m.group(2).strip()
                tid = TID_RE.match(raw)
                # protected 判据用原始行（归一化会剥掉 park 令牌）
                out.append((m.group(1) != " " or PARK_TOKEN in raw,
                            tid.group(1) if tid else None, _norm_body(raw)))
        return out

    old_rows = rows(old_text)
    new_ids: dict[str, tuple[bool, str]] = {}
    new_bodies: set[str] = set()
    for protected, tid, body in rows(new_text):
        new_bodies.add(body)
        if tid:
            new_ids[tid] = (protected, body)
    violations: list[str] = []
    for protected, tid, body in old_rows:
        if tid:
            if tid not in new_ids:
                violations.append(f"条目 {tid} 被删除——编号只增不挪（已勾/park 行永不删，Curia 永不删除），重编号用新 ID")
            elif protected and new_ids[tid][0] and new_ids[tid][1] != body:
                violations.append(f"条目 {tid} 已勾/park 正文被改写（疑似删后重建绕编号闸）——旧：「{body}」新：「{new_ids[tid][1]}」")
        elif protected and body not in new_bodies:
            violations.append(f"已勾/park 无编号行被删（整行指纹 fallback）——「{body[:60]}」")
    return violations


def renumber_guard(root: Path | None = None) -> int:
    """提交门接线口（pre-commit 调，终审 P1 补接线——原 validate_no_renumber 零
    调用方）：对拍 HEAD 版与暂存版（git index）的 punch 载体。判暂存面不判工作
    树（快照语义，与文档门同律——兄弟会话半成品不在此拦）。

    退出码：0=合法/首增（HEAD 无载体无可比）/本次未 staged 载体；1=违规拒提交；
    2=载体被整删（HEAD 在场而暂存缺席）。
    """
    root = root if root is not None else ROOT
    rel = DEFAULT_PUNCH.relative_to(ROOT).as_posix()
    old = subprocess.run(["git", "show", f"HEAD:{rel}"], capture_output=True, text=True, cwd=str(root))
    if old.returncode != 0:
        return 0  # HEAD 无载体（首笔新增）——无旧文可比
    new = subprocess.run(["git", "show", f":{rel}"], capture_output=True, text=True, cwd=str(root))
    if new.returncode != 0:
        print(f"punch_slice: 载体被删（HEAD 有 {rel} 而暂存区无）——punch 载体永不删除；归档走翻状态流程", file=sys.stderr)
        return 2
    violations = validate_no_renumber(old.stdout, new.stdout)
    for v in violations:
        print(f"punch_slice 重排闸: 拒——{v}", file=sys.stderr)
    return 1 if violations else 0


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
    ok3 = len(v) == 0  # 同 ID 在=合法；已勾翻回 open（重开）也合法
    v2 = validate_no_renumber("- [ ] T1 甲\n- [x] T2 乙\n", "- [ ] T1 甲\n")  # T2 被删
    ok4 = len(v2) == 1 and "T2" in v2[0]
    # 终审 P1 补腿：删 T3 重建 T3（内容不同）——集合差检不出，ID→内容映射才抓得住
    v5 = validate_no_renumber("- [x] T3 甲交付\n", "- [x] T3 乙顶替\n")
    ok5 = len(v5) == 1 and "T3" in v5[0]
    # 无编号已勾行整行指纹 fallback（现行 punch 无 T 编号——旧实现恒空判）
    v6 = validate_no_renumber("- [x] 已完成甲\n- [ ] 乙开放\n", "- [ ] 乙开放\n")
    ok6 = len(v6) == 1 and "甲" in v6[0]
    # owner 解 park（删令牌改回普通 [ ]）是文档化合法流——不得误判
    v7 = validate_no_renumber("- [ ] [PARKED→人审: 等裁决] 乙挂账\n", "- [ ] 乙挂账\n")
    ok7 = len(v7) == 0
    print(f"punch_slice 自检: {'腿绿' if ok3 and ok4 else 'FAIL'} 重编号检（同 ID 合法/删 ID 红）")
    print(f"punch_slice 自检: {'腿绿' if ok5 else 'FAIL'} 同 ID 重建检（删 T3 换内容重建=红）")
    print(f"punch_slice 自检: {'腿绿' if ok6 and ok7 else 'FAIL'} 无编号指纹 fallback（删已勾行红/解 park 合法）")
    rc = 0 if (ok1 and ok2 and ok3 and ok4 and ok5 and ok6 and ok7) else 1
    print(f"punch_slice 自检：{'PASS' if rc == 0 else 'FAIL'}")
    return rc


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    if "--renumber-guard" in argv:
        return renumber_guard()
    path = Path(argv[argv.index("--punch") + 1]) if "--punch" in argv else None
    print(json.dumps(slice_punch(load(path)), ensure_ascii=False, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
