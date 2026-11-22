#!/usr/bin/env python3
"""facts.py —— 计数事实制品对拍器（工程纪律刀阶段 4；规则/16 §3）。

    python3 tools/facts.py            # 对拍 tools/facts.txt（不符 exit 1 带 diff）
    python3 tools/facts.py --update   # 刷新制品（动计数输入面的同一提交带刷）
    python3 tools/facts.py --selftest # 合成 git 仓夹具（正例/篡改/幂等三腿）

背景（impossible-os 数字自证链迁移）：一切对外声称的计数由脚本重算、
git 跟踪、零日期——数字变才有 diff，评审时 diff 即见测试债增减；数字
不再靠人口头声称。文档面三键（docs_md/design_md/stamped_designs）的
提交级执法在 check-docs --verify-facts（快照门内联）；本工具是全键面
与 CI 侧（ci-light 对拍步）。

口径声明（全部静态字面量口径，与运行期结果口径互不可比——勿混引）：
  tests_rust        git 跟踪 *.rs（排除 reference/ 与 cargo-target 系）里
                    行首 `#[test]` 的行数（宿主钉板套件静态近似面；cargo
                    result 合并口径归 规则/07）。
  smoke_bare/_rootfs/_rx/_eco   markers.sh 四数组长度（source 真值）。
  smoke_gui8        gui8-smoke.py 内含词 ok 的去重字符串字面量数（判读词
                    单源自证面——gui8 断言词住在 py，计数经此对拍）。
  bcts_floor_live   bcts-run.sh 的 TIER1_FLOOR_LIVE 值。
  docs_md           git 跟踪 .agents/ 与 docs/ 下 .md 数。
  design_md         git 跟踪 .agents/plans/<模块>/design.md 数。
  stamped_designs   其中含 `> Verified:` 戳行者的数（双戳棘轮位）。
  banner_consumers  引用 $BANNER_FORM_RE 的跟踪脚本数（3 消费脚本 + 1 检测
                    器自身 = 4 为现势真值；markers.sh 自检腿的引用计入）。

诚实条款：非 git 树或 git 不可用即 loud 红（exit 2），不静默改口径。
"""
from __future__ import annotations

import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FACTS = ROOT / "tools" / "facts.txt"

KEY_ORDER = [
    "banner_consumers", "bcts_floor_live", "design_md", "docs_md",
    "smoke_bare", "smoke_eco", "smoke_gui8", "smoke_rootfs", "smoke_rx",
    "stamped_designs", "tests_rust",
]


def git_files(root: Path, *patterns: str) -> list[str]:
    r = subprocess.run(
        ["git", "-C", str(root), "ls-files", "--", *patterns],
        capture_output=True, text=True,
    )
    if r.returncode != 0:
        print(f"facts: git ls-files 失败（{r.stderr.strip()}）——非 git 树不接受静默口径", file=sys.stderr)
        raise SystemExit(2)
    return [ln for ln in r.stdout.splitlines() if ln]


def marker_counts(root: Path) -> dict[str, int]:
    script = (
        "source tools/markers.sh; "
        "printf '%s %s %s %s\\n' \"${#MARKERS[@]}\" \"${#ROOTFS_MARKERS[@]}\" "
        "\"${#RX_MARKERS[@]}\" \"${#ECO_MARKERS[@]}\""
    )
    r = subprocess.run(["bash", "-c", script], capture_output=True, text=True, cwd=str(root))
    if r.returncode != 0:
        print(f"facts: markers.sh source 失败（{r.stderr.strip()}）", file=sys.stderr)
        raise SystemExit(2)
    bare, rootfs, rx, eco = r.stdout.split()
    return {"smoke_bare": int(bare), "smoke_rootfs": int(rootfs), "smoke_rx": int(rx), "smoke_eco": int(eco)}


def compute(root: Path) -> dict[str, int]:
    facts: dict[str, int] = {}
    facts.update(marker_counts(root))

    # tests_rust：静态 #[test] 行数（排除参考实现与构建产物）
    n = 0
    test_re = re.compile(r"^\s*#\[test\]\s*$")
    for f in git_files(root, "*.rs"):
        if f.startswith("reference/") or "cargo-target" in f or "/target/" in f:
            continue
        try:
            text = (root / f).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        n += sum(1 for ln in text.splitlines() if test_re.match(ln))
    facts["tests_rust"] = n

    # smoke_gui8：判读词形态字符串（含词 ok 的去重串——gui8 断言面单源自证）
    gui8 = root / "tools" / "gui8-smoke.py"
    ok_re = re.compile(r'"([^"\n]*\bok\b[^"\n]*)"')
    facts["smoke_gui8"] = len(set(ok_re.findall(gui8.read_text(encoding="utf-8", errors="replace")))) if gui8.is_file() else 0

    # bcts 地板
    bcts = root / "tools" / "bcts-run.sh"
    m = re.search(r"TIER1_FLOOR_LIVE=(\d+)", bcts.read_text(encoding="utf-8")) if bcts.is_file() else None
    facts["bcts_floor_live"] = int(m.group(1)) if m else 0

    # 文档面三键（git 跟踪面）
    mds = [f for f in git_files(root, "*.md") if f.startswith((".agents/", "docs/"))]
    facts["docs_md"] = len(mds)
    designs = [f for f in mds if re.fullmatch(r"\.agents/plans/[^/]+/design\.md", f)]
    facts["design_md"] = len(designs)
    stamp_re = re.compile(r"^>\s*Verified:", re.M)
    facts["stamped_designs"] = sum(
        1 for f in designs if stamp_re.search((root / f).read_text(encoding="utf-8", errors="replace"))
    )

    # banner 消费者
    facts["banner_consumers"] = sum(
        1 for f in git_files(root, "tools/*.sh")
        if "$BANNER_FORM_RE" in (root / f).read_text(encoding="utf-8", errors="replace")
    )
    return facts


def render(facts: dict[str, int]) -> str:
    lines = [
        "# 计数事实制品（工程纪律刀阶段 4；规则/16 §3）——tools/facts.py 重算，",
        "# 零日期：数字变才有 diff。刷新：python3 tools/facts.py --update（动计数",
        "# 输入面的同一提交带刷）；文档面三键另有 check-docs --verify-facts 提交级",
        "# 执法（快照门内联）。口径声明见 facts.py 头注——静态口径，勿与运行期",
        "# 结果口径（cargo result / smoke verdict）混引。",
    ]
    lines += [f"{k}={facts[k]}" for k in KEY_ORDER]
    return "\n".join(lines) + "\n"


def load_expected(path: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    for ln in path.read_text(encoding="utf-8").splitlines():
        m = re.fullmatch(r"([a-z0-9_]+)=(.+)", ln)
        if m:
            out[m.group(1)] = m.group(2)
    return out


def facts_check(root: Path, artifact: Path) -> int:
    if not artifact.is_file():
        print(f"facts: 制品缺席（{artifact}）——先 python3 tools/facts.py --update", file=sys.stderr)
        return 1
    got = compute(root)
    exp = load_expected(artifact)
    bad = 0
    for k in KEY_ORDER:
        if k not in exp:
            print(f"facts: 制品缺键 {k}", file=sys.stderr)
            bad += 1
        elif str(got[k]) != exp[k]:
            print(f"facts: {k} 漂移：制品 {exp[k]} ≠ 实况 {got[k]}", file=sys.stderr)
            bad += 1
    if bad:
        print(f"facts: FAIL（{bad} 键漂移）——刷新：python3 tools/facts.py --update（同批提交）", file=sys.stderr)
        return 1
    print(f"facts: PASS（{len(KEY_ORDER)} 键一致）")
    return 0


def selftest() -> int:
    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        r = subprocess.run(["git", "-C", str(t), "init", "-q"], capture_output=True)
        if r.returncode != 0:
            print("facts 自检 FAIL: git init 失败", file=sys.stderr)
            return 1
        (t / "tools").mkdir()
        (t / "tools" / "markers.sh").write_text(
            'MARKERS=("a ok" "b ok")\nROOTFS_MARKERS=("r ok")\nRX_MARKERS=("x ok")\nECO_MARKERS=("e ok" "e2 ok")\n',
            encoding="utf-8",
        )
        (t / "tools" / "bcts-run.sh").write_text("TIER1_FLOOR_LIVE=91\n", encoding="utf-8")
        (t / "tools" / "smoke-a.sh").write_text("grep -Eq \"$BANNER_FORM_RE\" l\n", encoding="utf-8")
        (t / "tools" / "gui8-smoke.py").write_text('S1 = "CLICK ok"\nS2 = "CLICK ok"\nS3 = "LAYOUT ok n="\nOTHER = 3\n', encoding="utf-8")
        (t / "src").mkdir()
        (t / "src" / "x.rs").write_text("#[test]\nfn a() {}\n\n#[test]\nfn b() {}\n", encoding="utf-8")
        (t / ".agents" / "plans" / "mod1").mkdir(parents=True)
        (t / ".agents" / "plans" / "mod1" / "design.md").write_text(
            "# mod1 总体设计\n\nType: spec\nStatus: active\nUpdated: 2026-11-21\nVersion: 1.0.0\n\n> Verified: 2026-11-21 · abcdef123456 · 1/1 · tests -\n",
            encoding="utf-8",
        )
        (t / ".agents" / "plans" / "mod2").mkdir(parents=True)
        (t / ".agents" / "plans" / "mod2" / "design.md").write_text(
            "# mod2 总体设计\n\nType: spec\nStatus: active\nUpdated: 2026-11-21\nVersion: 1.0.0\n", encoding="utf-8",
        )
        (t / "docs").mkdir()
        (t / "docs" / "free.md").write_text("# 自由\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(t), "add", "-A"], capture_output=True)
        subprocess.run(
            ["git", "-C", str(t), "-c", "user.name=f", "-c", "user.email=f@l", "commit", "-qm", "i"],
            capture_output=True,
        )
        got = compute(t)
        expect = {
            "smoke_bare": 2, "smoke_rootfs": 1, "smoke_rx": 1, "smoke_eco": 2,
            "tests_rust": 2, "smoke_gui8": 2, "bcts_floor_live": 91,
            "docs_md": 3, "design_md": 2, "stamped_designs": 1, "banner_consumers": 1,
        }
        if got != expect:
            print(f"facts 自检 FAIL: 合成仓计数不符\n  实得 {got}\n  期望 {expect}", file=sys.stderr)
            return 1
        # 篡改腿：制品与实况漂移必红
        art = t / "facts.txt"
        art.write_text("docs_md=99\nsmoke_bare=2\n", encoding="utf-8")
        if facts_check(t, art) == 0:
            print("facts 自检 FAIL: 篡改制品未红", file=sys.stderr)
            return 1
        # 幂等腿：--update 语义 = 写出的制品与实况一致
        art.write_text(render(got), encoding="utf-8")
        if facts_check(t, art) != 0:
            print("facts 自检 FAIL: 新鲜制品未过对拍", file=sys.stderr)
            return 1
        print("facts 自检：PASS（合成仓计数 11 键 / 篡改红 / 幂等三腿全触发）")
        return 0


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    if "--update" in argv:
        got = compute(ROOT)
        FACTS.write_text(render(got), encoding="utf-8")
        print(f"facts: 制品已刷新（{len(KEY_ORDER)} 键 → {FACTS.relative_to(ROOT)}）")
        return 0
    return facts_check(ROOT, FACTS)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
