#!/usr/bin/env python3
"""review-recv.py —— 评审报告协议行回执验收器（自动化开发二期 P1；规则/16 §5）。

评审报告的机器可判契约（报告 = 自然语言正文 + 协议尾）：
  - 每个上场镜头一行：LENS <镜头名> <pass|L1|L2|L3>
  - 末行收口：HIGHEST <级别>（全部 pass 时为 pass；必须等于各镜头最高级）
协议尾必须连续收在报告末尾（机器只读尾，散落在正文的协议行=拒收）；级别词汇表
钉死 pass|L1|L2|L3，镜头集合在 --pin 时对照 pin 求解（经 review-pin-check.py
lenses 子命令——pin 语义单源住那，本工具不双写）。

子命令：
  parse <报告> [--pin <pin.toml>] [--paths <逗号分隔>] [--state-dir <d>]
      只判不落盘。
  envelope <报告> [同上]
      完整才落 <state-dir>/<指纹>.json（含输入指纹、报告全文、解析结果、路径
      口径）——不完整不落回执（fail closed：半截叙述不当凭据）。
  verify <报告> [--pin <pin.toml>] [--paths <逗号分隔>] [--state-dir <d>]
      对账既有回执：重算输入指纹找回执；找不到 = 从未验收或输入面漂移。
输入指纹 = sha256(报告字节 + 策略字节(pin 用了才计) + 路径口径)——字节不变回执
恒有效（无 TTL），任何漂移立即失效（e-receipts 同哲学）。

fail-direction：rc=3（报告不完整/不自洽，评审必须重出协议尾）与 rc=1（verify
无回执/漂移、pin 面红）都是拒收边；rc=2 是用法/IO 错。默认 state 目录
.claude/state/review-recv/。

rc：0 完整/对账一致；1 对账红或 pin 面红；2 用法/IO；3 报告不完整或不自洽。
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PIN_CHECK = Path(__file__).resolve().with_name("review-pin-check.py")
DEFAULT_STATE_DIR = ROOT / ".claude" / "state" / "review-recv"

VERDICT_RANK = {"pass": 0, "L3": 1, "L2": 2, "L1": 3}  # 数字越大越重
_RE_LENS = re.compile(r"^LENS\s+(\S+)\s+(\S+)$")
_RE_HIGHEST = re.compile(r"^HIGHEST\s+(\S+)$")
_RE_LENS_ANY = re.compile(r"^LENS(\s|$)")
_RE_HIGHEST_ANY = re.compile(r"^HIGHEST(\s|$)")
_RE_NAME = re.compile(r"^[A-Za-z0-9_][A-Za-z0-9_-]*$")


def parse_report(text: str):
    """返回 (parsed, problems)。problems 空 = 报告完整且自洽。

    parsed 恒含尽力解析出的 lenses/highest（供 --pin 对账与漂移报告），
    problems 非空时 parsed 不作凭据用。
    """
    problems: list = []
    lenses: dict = {}
    order: list = []
    highest = None
    lines = [ln.strip() for ln in text.splitlines() if ln.strip()]
    if not lines:
        return None, ["报告为空"]

    # ① 末行必须是 HIGHEST（缺收口行是「缺 HIGHEST」腿）
    m = _RE_HIGHEST.match(lines[-1])
    if not m:
        problems.append("末行不是 HIGHEST <级别>（缺收口行）")
    else:
        highest = m.group(1)
        if highest not in VERDICT_RANK:
            problems.append(f"HIGHEST 级别未知：{highest!r}（词汇表钉死 pass|L1|L2|L3）")
            highest = None

    # ② 从末行向上收连续 LENS 行（协议尾），至少一条（「缺镜头行」腿）
    idx = len(lines) - 2
    while idx >= 0 and _RE_LENS.match(lines[idx]):
        name, level = _RE_LENS.match(lines[idx]).groups()
        if level not in VERDICT_RANK:
            problems.append(f"LENS 行级别未知：{name} {level!r}（词汇表钉死 pass|L1|L2|L3）")
        elif not _RE_NAME.match(name):
            problems.append(f"镜头名不合法：{name!r}")
        elif name in lenses:
            problems.append(f"镜头 {name} 重复出现（自相矛盾面）")
        else:
            lenses[name] = level
            order.append(name)
        idx -= 1
    tail_len = (len(lines) - 2 - idx) + 1  # LENS 行数 + HIGHEST 行
    if not order and not any("级别未知" in p for p in problems):
        problems.append("缺镜头行（协议尾至少一行 LENS <镜头名> <级别>）")

    # ③ 协议行散落在正文 = 拒收（机器只读尾，散落即漏读面）
    for ln in lines[: len(lines) - tail_len]:
        if _RE_LENS_ANY.match(ln) or _RE_HIGHEST_ANY.match(ln):
            problems.append(f"协议行散落在正文（必须连续收尾）：{ln[:60]!r}")

    # ④ HIGHEST 与镜头自洽（仅当两边都完整）
    if highest and lenses and not any("级别未知" in p for p in problems):
        top = max(VERDICT_RANK[v] for v in lenses.values())
        if VERDICT_RANK[highest] != top:
            problems.append(f"HIGHEST {highest} 与镜头最高级不自洽（应为 "
                            f"{[k for k, v in VERDICT_RANK.items() if v == top][0]}）")

    parsed = {"lenses": lenses, "lens_order": list(reversed(order)), "highest": highest}
    return parsed, problems


def expected_lenses(pin_path: Path, paths: list):
    """经 pin-check lenses 子命令求期望镜头集（pin 语义单源不双写）。"""
    cmd = [sys.executable, str(PIN_CHECK), "lenses", "--pin", str(pin_path)]
    if paths:
        cmd += ["--paths", ",".join(paths)]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        return None, f"pin-check lenses 红（rc={r.returncode}）：{r.stderr.strip()[:200]}"
    return set(r.stdout.split()), None


def fingerprint(report_bytes: bytes, pin_bytes, paths: list) -> str:
    h = hashlib.sha256()
    h.update(report_bytes)
    h.update(b"\x00pin:")
    h.update(pin_bytes if pin_bytes is not None else b"-")
    h.update(b"\x00paths:\n")
    h.update("\n".join(paths).encode("utf-8"))
    return h.hexdigest()


def read_report(path: Path):
    try:
        raw = path.read_bytes()
        text = raw.decode("utf-8")
    except (OSError, UnicodeDecodeError) as e:
        print(f"review-recv: 报告读不到/非 UTF-8（{path}: {e}）", file=sys.stderr)
        raise SystemExit(2)
    return raw, text


def adjudicate(text: str, pin_path, paths: list):
    """parse 语义：返回 (rc, parsed, problems)。pin 在席时对账镜头集合。"""
    parsed, problems = parse_report(text)
    if pin_path is not None:
        exp, perr = expected_lenses(pin_path, paths)
        if perr is not None:
            print(f"review-recv: {perr}", file=sys.stderr)
            return 1, parsed, problems
        if parsed and parsed["lenses"]:
            missing = sorted(exp - set(parsed["lenses"]))
            extra = sorted(set(parsed["lenses"]) - exp)
            if missing:
                problems.append("缺镜头行（pin 判该上场而报告没有）：" + ", ".join(missing))
            if extra:
                problems.append("多余镜头行（pin 判不该上场）：" + ", ".join(extra))
    if problems:
        for p in problems:
            print(f"review-recv: 报告不完整——{p}", file=sys.stderr)
        return 3, parsed, problems
    return 0, parsed, problems


def cmd_envelope(report: Path, pin_path, paths: list, state_dir: Path) -> int:
    raw, text = read_report(report)
    rc, parsed, _ = adjudicate(text, pin_path, paths)
    if rc != 0:
        print("review-recv: 不落回执（fail closed）", file=sys.stderr)
        return rc
    pin_bytes = None
    if pin_path is not None:
        try:
            pin_bytes = pin_path.read_bytes()
        except OSError as e:
            print(f"review-recv: pin 读不到（{e}）——不落回执", file=sys.stderr)
            return 1
    fp = fingerprint(raw, pin_bytes, paths)
    env = {
        "v": 1,
        "fingerprint": fp,
        "ts": time.time(),
        "report_path": str(report),
        "report_sha256": hashlib.sha256(raw).hexdigest(),
        "report_bytes_n": len(raw),
        "pin_path": str(pin_path) if pin_path else None,
        "pin_sha256": hashlib.sha256(pin_bytes).hexdigest() if pin_bytes else None,
        "paths": paths,
        "lens_order": parsed["lens_order"],
        "lenses": parsed["lenses"],
        "highest": parsed["highest"],
        "report": text,
    }
    state_dir.mkdir(parents=True, exist_ok=True)
    out = state_dir / f"{fp}.json"
    # 原子写：验收器与 verify 并发时半写文件会被当回执读。
    tmp = out.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(env, ensure_ascii=False, indent=1), encoding="utf-8")
    os.replace(tmp, out)
    print(f"review-recv: 回执已落 {out}（指纹 {fp[:12]}，HIGHEST {parsed['highest']}）")
    return 0


def cmd_verify(report: Path, pin_path, paths: list, state_dir: Path) -> int:
    raw, text = read_report(report)
    rc, parsed, problems = adjudicate(text, pin_path, paths)
    if rc == 3:
        return 3
    if rc == 1:
        return 1
    pin_bytes = None
    if pin_path is not None:
        try:
            pin_bytes = pin_path.read_bytes()
        except OSError as e:
            print(f"review-recv: pin 读不到（{e}）", file=sys.stderr)
            return 1
    fp = fingerprint(raw, pin_bytes, paths)
    env_path = state_dir / f"{fp}.json"
    if not env_path.is_file():
        print(f"review-recv: 无回执（指纹 {fp[:12]}）——从未验收或输入面漂移"
              "（报告/pin/路径口径任一变动指纹即变）", file=sys.stderr)
        return 1
    try:
        env = json.loads(env_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as e:
        print(f"review-recv: 回执文件坏（{e}）——按无回执处理，重跑 envelope", file=sys.stderr)
        return 1
    if env.get("lenses") != parsed["lenses"] or env.get("highest") != parsed["highest"] \
            or env.get("report_sha256") != hashlib.sha256(raw).hexdigest():
        print("review-recv: 对账红——回执内容与现输入不一致", file=sys.stderr)
        return 1
    print(f"review-recv: 对账一致（指纹 {fp[:12]}，ts {env.get('ts')}，HIGHEST {parsed['highest']}）")
    return 0


# ---------------------------------------------------------------------------
# selftest：合成报告 + 合成 pin 夹具，零网络零真仓写。
# ---------------------------------------------------------------------------

_FX_PIN = """
[reviewer]
lane = "review-lane-x"
model = "model-x"
model_source = "jobs.json"

[passes]
plan = "p1"
round = "p2"
performance = "p3"
completion = "p4"

[levels.L1]
meaning = "m1"
blocking = true
[levels.L2]
meaning = "m2"
blocking = true
[levels.L3]
meaning = "m3"
blocking = false

[lenses.alpha]
focus = "f-alpha"
requires = []

[lenses.beta]
focus = "f-beta"
requires = ["alpha"]

[lenses.gamma]
focus = "f-gamma"
requires = ["beta"]

[lens_rules]
always = ["alpha", "beta"]

[lens_rules.load]
gui = ["gamma"]

[cap]
pass_cap = 33
"""

_R_OK = "评审正文一行。\n结论一段。\nLENS alpha L1\nLENS beta pass\nHIGHEST L1\n"
_R_NO_HIGHEST = "正文。\nLENS alpha pass\nLENS beta pass\n"
_R_NO_LENS = "正文。\nHIGHEST pass\n"
_R_UNKNOWN = "正文。\nLENS alpha L4\nLENS beta pass\nHIGHEST L1\n"
_R_INCONSISTENT = "正文。\nLENS alpha pass\nLENS beta pass\nHIGHEST L2\n"
_R_SCATTERED = "LENS alpha pass\n中间隔了正文。\nLENS beta pass\nHIGHEST pass\n"
_R_DUP = "正文。\nLENS alpha pass\nLENS alpha pass\nHIGHEST pass\n"
_R_WITH_GAMMA = "正文。\nLENS alpha pass\nLENS beta pass\nLENS gamma L3\nHIGHEST L3\n"


def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"review-recv 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    def run(args: list, env_extra: dict = None):
        env = {**os.environ, **(env_extra or {})}
        return subprocess.run([sys.executable, str(Path(__file__).resolve()), *args],
                              capture_output=True, text=True, env=env)

    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        pin = t / "fx-pin.toml"
        pin.write_text(_FX_PIN, encoding="utf-8")
        state = t / "review-recv"

        def rep(name: str, text: str) -> Path:
            p = t / name
            p.write_text(text, encoding="utf-8")
            return p

        # ① 完整过（无 pin：纯结构契约）
        r = run(["parse", str(rep("ok.txt", _R_OK))])
        check("完整报告过 rc0", r.returncode == 0)
        # ② 缺 HIGHEST rc3
        r = run(["parse", str(rep("noh.txt", _R_NO_HIGHEST))])
        check("缺 HIGHEST rc3", r.returncode == 3 and "缺收口行" in r.stderr)
        # ③ 缺镜头行 rc3
        r = run(["parse", str(rep("nol.txt", _R_NO_LENS))])
        check("缺镜头行 rc3", r.returncode == 3 and "缺镜头行" in r.stderr)
        # ④ 未知级别 rc3
        r = run(["parse", str(rep("unk.txt", _R_UNKNOWN))])
        check("未知级别 rc3", r.returncode == 3 and "级别未知" in r.stderr)
        # ⑤ HIGHEST 不自洽 rc3
        r = run(["parse", str(rep("inc.txt", _R_INCONSISTENT))])
        check("HIGHEST 不自洽 rc3", r.returncode == 3 and "不自洽" in r.stderr)
        # ⑥ 协议行散落 rc3
        r = run(["parse", str(rep("sct.txt", _R_SCATTERED))])
        check("协议行散落正文 rc3", r.returncode == 3 and "散落" in r.stderr)
        # ⑦ 重复镜头 rc3
        r = run(["parse", str(rep("dup.txt", _R_DUP))])
        check("重复镜头行 rc3", r.returncode == 3 and "重复" in r.stderr)
        # ⑧ pin 对账：齐镜过 / 缺条件镜红 / 多余镜红
        r = run(["parse", str(rep("ok2.txt", _R_OK)), "--pin", str(pin), "--paths", "kernel/a.rs"])
        check("pin 对账齐镜过 rc0", r.returncode == 0)
        r = run(["parse", str(rep("ok3.txt", _R_OK)), "--pin", str(pin), "--paths", "gui/x.rs"])
        check("pin 判缺条件镜（gui 该上 gamma）rc3", r.returncode == 3 and "gamma" in r.stderr)
        r = run(["parse", str(rep("wg.txt", _R_WITH_GAMMA)), "--pin", str(pin), "--paths", "kernel/a.rs"])
        check("pin 判多余镜（kernel 不上 gamma）rc3", r.returncode == 3 and "gamma" in r.stderr)
        r = run(["parse", str(rep("wg2.txt", _R_WITH_GAMMA)), "--pin", str(pin), "--paths", "gui/x.rs"])
        check("pin 对账条件镜齐 rc0", r.returncode == 0)
        # ⑨ envelope：落盘内容齐 + 幂等指纹
        p_ok = rep("env.txt", _R_OK)
        r = run(["envelope", str(p_ok), "--pin", str(pin), "--paths", "kernel/a.rs", "--state-dir", str(state)])
        fp_file = None
        if state.is_dir():
            fps = list(state.glob("*.json"))
            fp_file = fps[0] if fps else None
        ok_env = r.returncode == 0 and fp_file is not None
        if ok_env:
            env = json.loads(fp_file.read_text(encoding="utf-8"))
            ok_env = (len(env["fingerprint"]) == 64 and env["report"] == _R_OK
                      and env["lenses"] == {"alpha": "L1", "beta": "pass"}
                      and env["highest"] == "L1" and env["paths"] == ["kernel/a.rs"])
        check("envelope 落盘内容齐（指纹/全文/解析/口径）", ok_env)
        # ⑩ verify 对账一致 / 漂移红 / 无回执红
        r = run(["verify", str(p_ok), "--pin", str(pin), "--paths", "kernel/a.rs", "--state-dir", str(state)])
        check("verify 同输入对账一致 rc0", r.returncode == 0)
        p_drift = rep("drift.txt", _R_OK.replace("评审正文一行。", "评审正文改了一字。"))
        r = run(["verify", str(p_drift), "--pin", str(pin), "--paths", "kernel/a.rs", "--state-dir", str(state)])
        check("verify 报告漂移 rc1", r.returncode == 1 and "无回执" in r.stderr)
        r = run(["verify", str(rep("fresh.txt", _R_WITH_GAMMA)), "--pin", str(pin),
                 "--paths", "gui/x.rs", "--state-dir", str(state)])
        check("verify 从未验收 rc1", r.returncode == 1)
        # ⑪ 不完整不落回执（fail closed）
        state2 = t / "review-recv2"
        r = run(["envelope", str(rep("bad.txt", _R_NO_HIGHEST)), "--state-dir", str(state2)])
        check("不完整报告不落回执 rc3", r.returncode == 3 and not state2.exists())
        # ⑫ pin 坏（缺字段）→ envelope 拒收 rc1 且不落盘
        bad_pin = t / "bad-pin.toml"
        bad_pin.write_text(_FX_PIN.replace('model = "model-x"\n', ""), encoding="utf-8")
        state3 = t / "review-recv3"
        r = run(["envelope", str(p_ok), "--pin", str(bad_pin), "--state-dir", str(state3)])
        check("pin 面红拒收回执 rc1", r.returncode == 1 and not state3.exists())
        # ⑬ MiniToml 强制腿：pin-check 走 fallback，envelope 照常
        state4 = t / "review-recv4"
        r = run(["envelope", str(rep("mini.txt", _R_OK)), "--pin", str(pin),
                 "--paths", "kernel/a.rs", "--state-dir", str(state4)],
                env_extra={"BERRYOS_PIN_PARSER": "mini"})
        check("pin 解析 fallback 腿（MiniToml）rc0", r.returncode == 0)
        # ⑭ 用法错 rc2
        r = run(["bogus-cmd"])
        check("用法错 rc2", r.returncode == 2)

    print(f"review-recv 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}"
          "（完整/缺 HIGHEST/缺镜头/未知级别/不自洽/散落/重复/pin 对账四腿/envelope/verify 三腿/fail-closed/fallback）")
    return 0 if fails == 0 else 1


def _usage() -> int:
    print("usage: review-recv.py {parse|envelope|verify} <报告> [--pin <pin.toml>]"
          " [--paths <逗号分隔>] [--state-dir <目录>]  |  --selftest", file=sys.stderr)
    return 2


def main(argv: list) -> int:
    if "--selftest" in argv:
        return selftest()
    if not argv or argv[0] not in ("parse", "envelope", "verify"):
        return _usage()
    cmd = argv.pop(0)
    pos: list = []
    pin_path = None
    paths: list = []
    state_dir = DEFAULT_STATE_DIR
    while argv:
        a = argv.pop(0)
        if a == "--pin" and argv:
            pin_path = Path(argv.pop(0))
        elif a == "--paths" and argv:
            paths = [p.strip() for p in argv.pop(0).split(",") if p.strip()]
        elif a == "--state-dir" and argv:
            state_dir = Path(argv.pop(0))
        elif a.startswith("--"):
            return _usage()
        else:
            pos.append(a)
    if len(pos) != 1:
        return _usage()
    report = Path(pos[0])
    if cmd == "parse":
        raw, text = read_report(report)
        rc, parsed, _ = adjudicate(text, pin_path, paths)
        if rc == 0:
            print(f"review-recv: 完整（{len(parsed['lenses'])} 镜，HIGHEST {parsed['highest']}）")
        return rc
    if cmd == "envelope":
        return cmd_envelope(report, pin_path, paths, state_dir)
    return cmd_verify(report, pin_path, paths, state_dir)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
