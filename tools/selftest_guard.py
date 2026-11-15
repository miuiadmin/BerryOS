#!/usr/bin/env python3
"""selftest_guard.py —— tools 自检接线门禁（2026-11-12 立，判例族「有 selftest 零自动执法」根治）。

    python3 tools/selftest_guard.py            # 执法扫描（exit 0 = 过，1 = 漏接线）
    python3 tools/selftest_guard.py --selftest # 门禁自检（挂 just test-host）

背景（7 连发判例族，LOG/QUEUE 可查）：echod/evsclient、genwycheproof、
gensourceindex、fastlane_sim、gui8-smoke（连字符拼写漏网）、bcts-run.sh
（*.sh 漏扫）、shvarcheck——每件落地时自称「selftest N 面绿」，但从未接进
just test-host，零自动执法。此前的补漏方式是人工三轮扫描（*.py → *.sh →
连字符拼写），每轮都可能漏口径；本门 = 扫描口径本身机器化，新工具带
selftest 而不接线，CI 直接红。

扫描口径（自我钉板，见 selftest 第 1 面）：
  1. tools/ 下 *.py / *.sh（符号链接跳过——可能指到仓外）。
  2. 「有自检」判定 = 文本含字面量 --selftest 或 --self-test（argv 面实装，
     含 --selftest-split）。
  3. 「已接线」判定 = justfile 文本存在 `tools/<basename> --self*` 调用行
     （前缀匹配，--selftest-split / --self-test 连字符变体同吃）。
豁免表 KNOWN_EXEMPT（单源）：声明「有自检声明但非门禁形态」的工具 + 理由。
改豁免表 = 显式登记理由，不许静默（G3 探针串同哲学）。

改 tools/ 新增自检、改 justfile test-host 面必跑。
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOOLS = ROOT / "tools"
JUSTFILE = ROOT / "justfile"

# 字面量口径：argv 实装面（'#' 截行防 justfile 注释行误配）。
DECL_RE = re.compile(r"--self-?test")
# justfile 接线判定：tools/<name> 后随 --self*（变体 --selftest-split /
# --self-test 同吃）；^# 排注释行。
WIRE_RE_TMPL = r"tools/{name}\s+--self\w*"


def declares_selftest(text: str) -> bool:
    return bool(DECL_RE.search(text))


def wire_re(name: str) -> re.Pattern:
    return re.compile(WIRE_RE_TMPL.format(name=re.escape(name)))


def scan(
    tools_dir: Path = TOOLS,
    justfile: Path = JUSTFILE,
    exempt: dict | None = None,
) -> tuple[list[str], list[tuple[str, str]]]:
    """→ (漏接线清单, 豁免命中清单)。执法面，selftest 与主口共用。

    tools_dir/justfile 可注入合成仓（selftest 端到端面）；exempt 缺省 =
    KNOWN_EXEMPT 单源（selftest S6 演示豁免路径时注入合成表）。
    """
    exempt = KNOWN_EXEMPT if exempt is None else exempt
    just_text = justfile.read_text(encoding="utf-8")
    just_lines = [ln.split("#", 1)[0] for ln in just_text.splitlines()]
    unwired: list[str] = []
    exempt_hits: list[tuple[str, str]] = []
    for p in sorted(tools_dir.iterdir()):
        if p.is_symlink() or not p.is_file() or p.suffix not in (".py", ".sh"):
            continue
        try:
            text = p.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue  # 非文本件防御面
        if not declares_selftest(text):
            continue
        name = p.name
        if name in exempt:
            exempt_hits.append((name, exempt[name]))
            continue
        if not any(wire_re(name).search(ln) for ln in just_lines):
            unwired.append(name)
    return unwired, exempt_hits


# 豁免单源：文件名 → 理由。当前零豁免——7 连发后接线清零，新增豁免须写理由。
KNOWN_EXEMPT: dict[str, str] = {}


def main(argv: list[str]) -> int:
    if "--selftest" in argv:
        return selftest()
    unwired, exempt = scan()
    for name, why in sorted(exempt):
        print(f"selftest_guard: 豁免 {name}（{why}）")
    if unwired:
        for name in unwired:
            print(
                f"selftest_guard: FAIL——tools/{name} 声明 --selftest 但 justfile 未接线"
                "（判例族「有 selftest 零自动执法」；接线进 test-host 或登记豁免）",
                file=sys.stderr,
            )
        return 1
    print(
        f"selftest_guard: PASS（{sum(1 for p in TOOLS.iterdir() if p.suffix in ('.py', '.sh') and p.is_file() and not p.is_symlink())} 件扫描，"
        f"{len(exempt)} 豁免，0 漏接线）"
    )
    return 0


# ---------------------------------------------------------------- selftest
# 门禁自检纪律同 gen_abi/mkfat：检测器自检（selftest 钉板）与检测器执法
# （全仓扫描）分层——检测逻辑自身改坏（漏检/误报）须有机器咬合。
def selftest() -> int:
    """钉板面：合成夹具下逐面断言，不依赖活树当前恰好绿。"""
    import tempfile

    fails: list[str] = []
    passed = 0

    def check(label: str, cond: bool, detail: str = "") -> None:
        nonlocal passed
        if cond:
            passed += 1
        else:
            fails.append(f"{label}{('——' + detail) if detail else ''}")

    # S1 字面量口径：--selftest / --self-test / --selftest-split 三形态全认。
    for lit in ("--selftest", "--self-test", "--selftest-split"):
        check(f"S1 判定字面量 {lit}", declares_selftest(f"x {lit}"))
    check("S1 非自检文件不误报", not declares_selftest("--selfhost"))
    check("S1 普通脚本不误报", not declares_selftest("print('hello')"))

    # S2 justfile 接线口径：变体全吃（注释剔除在 scan 层 split('#')，
    # wire_re 本身是子串匹配——分层断言：S2 管子串面，S3 合成仓管注释面）。
    for flag in ("--selftest", "--self-test", "--selftest-split"):
        check(
            f"S2 接线变体 {flag}",
            wire_re("mkfat.py").search(f"    python3 tools/mkfat.py {flag}") is not None,
        )
    check(
        "S2 名字前缀不误配",
        wire_re("mkfat.py").search("    python3 tools/mkfat2.py --selftest") is None,
    )

    # S3 端到端（含红分支实证）：合成小仓 wired/loose/plain/shim 四件，
    # scan 应咬出 loose.py + shim.sh 两漏（= 判例族「落地未接线」突变复演）。
    with tempfile.TemporaryDirectory() as td:
        tdp = Path(td)
        (tdp / "tools").mkdir()
        (tdp / "tools" / "wired.py").write_text("x --selftest\n", encoding="utf-8")
        (tdp / "tools" / "loose.py").write_text("y --selftest\n", encoding="utf-8")
        (tdp / "tools" / "plain.py").write_text("z\n", encoding="utf-8")
        (tdp / "tools" / "shim.sh").write_text("w --self-test\n", encoding="utf-8")
        (tdp / "justfile").write_text(
            "test-host:\n"
            "    python3 tools/wired.py --selftest\n"
            "    # python3 tools/loose.py --selftest（注释行不算接线）\n",
            encoding="utf-8",
        )
        unwired, exempt = scan(tools_dir=tdp / "tools", justfile=tdp / "justfile")
        check("S3 咬出全部漏接线件", unwired == ["loose.py", "shim.sh"], ",".join(unwired))
        check("S3 已接线件不误报", "wired.py" not in unwired)
        check("S3 非自检件不进面", "plain.py" not in unwired)
        check("S3 合成仓零豁免", exempt == [])

    # S4 活树契约：7 判例件全部在接线集内（防 justfile 接线行被误删后门仍绿）。
    just_lines = [
        ln.split("#", 1)[0]
        for ln in JUSTFILE.read_text(encoding="utf-8").splitlines()
    ]
    for name in (
        "echod.py", "evsclient.py", "genwycheproof.py", "gensourceindex.py",
        "fastlane_sim.py", "gui8-smoke.py", "bcts-run.sh", "shvarcheck.py",
    ):
        if (TOOLS / name).exists():
            check(
                f"S4 判例件 {name} 在席且已接线",
                any(wire_re(name).search(ln) for ln in just_lines),
            )

    # S5 执法面自身：活树扫描当前应 0 漏（含豁免表零豁免契约）。
    unwired, exempt = scan()
    check("S5 活树 0 漏接线", not unwired, ",".join(unwired))
    check("S5 豁免表为空（零豁免契约）", KNOWN_EXEMPT == {})

    # S6 豁免路径端到端：合成仓豁免件不进漏接线清单但入豁免命中账（豁免
    # 语义不腐化：豁免 ≠ 隐身，主口打印豁免账）。
    with tempfile.TemporaryDirectory() as td:
        tdp = Path(td)
        (tdp / "tools").mkdir()
        (tdp / "tools" / "exc.py").write_text("e --selftest\n", encoding="utf-8")
        (tdp / "justfile").write_text("", encoding="utf-8")
        unwired, exempt = scan(
            tools_dir=tdp / "tools",
            justfile=tdp / "justfile",
            exempt={"exc.py": "合成豁免"},
        )
        check("S6 豁免件不进漏接线清单", unwired == [], ",".join(unwired))
        check("S6 豁免件入豁免命中账", exempt == [("exc.py", "合成豁免")])

    print(
        f"\nselftest_guard selftest: {'PASS' if not fails else 'FAIL'}"
        f"（{passed} passed / {len(fails)} failed）"
    )
    for f in fails:
        print(f"  FAIL: {f}")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
