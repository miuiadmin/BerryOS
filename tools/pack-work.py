#!/usr/bin/env python3
"""pack-work.py —— E lane 开工包拼装器（自动化开发二期 P0；规则/16 §5）。

lane 醒来第一个动作跑本件（零模型成本），拿到一份 JSON 开工包，把 token 花在
干活而非重导状态——对应 impossible-os 的 advance-work + section-pack：
  punch_list   活跃计划「未完成事项」节的开放条目（park 单列，带理由）
  facts        tools/facts.txt 全键（计数自证面）
  git          分支/领先上游笔数/脏文件数
  gates        门禁三件的快速现状（存在性而非真跑——真跑归提交流）
  stamps       已双戳的 design.md 数（棘轮位）
  hint         下一动作建议（有 open 项=干第一项；全 park=问 fixpoint）
只读。python3 tools/pack-work.py --selftest 用合成夹具验拼包纯函数。
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


def build_pack(root: Path = ROOT, punch_text: str | None = None) -> dict:
    open_items, parked = split_punch(punch_text if punch_text is not None else
                                     (PUNCH_PLAN.read_text(encoding="utf-8") if PUNCH_PLAN.is_file() else ""))
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
    hint = ("干 punch-list 第一开放项：验证过门再 commit（同批刷 facts）"
            if open_items else
            "无开放项——先核对 [PARKED] 是否已解，再跑 python3 tools/e-guard.py fixpoint")
    return {"punch_list": {"open": open_items, "parked": parked}, "facts": facts,
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
    rc = 0 if (ok and ok2) else 1
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
