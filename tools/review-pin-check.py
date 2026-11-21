#!/usr/bin/env python3
"""review-pin-check.py —— 评审口径 pin 的校验器与镜头求解器（自动化开发二期 P1；规则/16 §5）。

pin = .agents/skills/berryos-runner/review-pin.toml：评审策略唯一事实源（评审者身份/
四 pass 日程/阻断级/镜头字典/条件加载/pass 帽）。本工具是它的机器面：

  python3 tools/review-pin-check.py [--pin <路径>] [--live] [--jobs <路径>]
      校验：TOML 语法 + 必填字段 + 镜头 requires 无环 + pass_cap 正整数 + 阻断级
      契约（L1/L2 必阻断、L3 必不阻断）。--live 对照 jobs.json 里 review lane 的
      model 与 pin 评审者模型（jobs 路径默认取 pin.reviewer.model_source）。
  python3 tools/review-pin-check.py lenses [--pin <路径>] [--paths <逗号分隔>]
      解答「这批改动路径该上场哪些镜头」，一行一镜（review-recv.py 消费本出口——
      pin 语义只住本文件，不双写）。

fail-direction（哪边失败是安全边）：
  - pin 缺席/语法坏/字段漂移/成环 = 红（rc 1）——评审策略不存在就不许评审跑，宁红勿糊。
  - --live 读不到 jobs.json / JSON 坏 / 找不到 lane = 只警告不红——环境缺席≠配置
    漂移（判例：CI 机器没有 ~/.hermes，红了就是假警报）；lane 在而 model 变了 = 红。

rc：0 pin 有效（--live 时含对账通过）；1 pin 无效或 live 漂移；2 用法错。

TOML 面：Python ≥3.11 用 tomllib（mkcpio/gen_abi 同依赖先例）；3.9/3.10 落到内置
MiniToml 子集（表头/字符串/整数/布尔/字符串数组/注释）——pin 是本工具自家的冻结
schema，子集够用；BERRYOS_PIN_PARSER=mini 可强制走 fallback（3.11+ 主机也能自测
该腿）。已知宽松点：整数字面量前导零（tomllib 拒、MiniToml 收），严格性由 schema
层兜底，自检的双解析器一致腿避开该面。
"""
from __future__ import annotations

import json
import os
import string
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_PIN = ROOT / ".agents" / "skills" / "berryos-runner" / "review-pin.toml"

LEVEL_KEYS = ("L1", "L2", "L3")
# 阻断级契约：L1/L2 阻断、L3 提示——机器判的就是 blocking 布尔，不是 meaning 文案。
EXPECTED_BLOCKING = {"L1": True, "L2": True, "L3": False}
REQUIRED_PASSES = ("plan", "round", "performance", "completion")
PIN_TOP_KEYS = ("reviewer", "passes", "levels", "lenses", "lens_rules", "cap")


class PinError(Exception):
    """pin 面一切「红」的载体（语法/schema）。"""


# ---------------------------------------------------------------------------
# MiniToml：TOML 子集解析器（3.9 兼容的 fallback）。为什么自己写：pin 的 schema
# 冻结在本工具，子集 = 表头 + 键值 + 字符串/整数/布尔/字符串数组 + 注释，够用且
# 不引第三方依赖；子集外的形态（浮点/内联表/多行字符串/数组表）一律红——宁红不猜。
# ---------------------------------------------------------------------------

_BARE_CHARS = set(string.ascii_letters + string.digits + "_-")


class MiniTomlError(PinError):
    pass


def _parse_minitoml(text: str, src: str = "<minitoml>") -> dict:
    root: dict = {}
    cur = root
    declared_tables: set = set()
    i, n = 0, len(text)

    def err(msg: str, pos: int) -> MiniTomlError:
        line = text.count("\n", 0, pos) + 1
        return MiniTomlError(f"{src}: 行 {line}: {msg}（MiniToml 子集外）")

    def skip_ws(newlines: bool) -> None:
        nonlocal i
        while i < n:
            c = text[i]
            if c in " \t" or (newlines and c in "\r\n"):
                i += 1
            elif c == "#":
                while i < n and text[i] != "\n":
                    i += 1
            else:
                break

    def parse_string() -> str:
        nonlocal i
        if i >= n or text[i] != '"':
            raise err("期待字符串", i)
        i += 1
        out = []
        while i < n:
            c = text[i]
            if c == '"':
                i += 1
                return "".join(out)
            if c == "\n":
                raise err("字符串内裸换行（子集禁多行字符串）", i)
            if c == "\\":
                i += 1
                if i >= n:
                    raise err("转义截断", i)
                e = text[i]
                mapped = {'"': '"', "\\": "\\", "n": "\n", "t": "\t", "r": "\r"}.get(e)
                if mapped is None:
                    raise err(f"不支持的转义 \\{e}", i)
                out.append(mapped)
                i += 1
                continue
            out.append(c)
            i += 1
        raise err("字符串未闭合", i)

    def parse_key() -> str:
        nonlocal i
        skip_ws(False)
        if i < n and text[i] == '"':
            return parse_string()
        j = i
        while j < n and text[j] in _BARE_CHARS:
            j += 1
        if j == i:
            raise err("期待键名（裸键或带引号键）", i)
        k = text[i:j]
        i = j
        return k

    def parse_array() -> list:
        nonlocal i
        i += 1  # 消费 '['
        items = []
        while True:
            skip_ws(True)
            if i >= n:
                raise err("数组未闭合", i)
            if text[i] == "]":
                i += 1
                return items
            if text[i] != '"':
                raise err("数组元素只许字符串（子集）", i)
            items.append(parse_string())
            skip_ws(True)
            if i < n and text[i] == ",":
                i += 1
                continue
            if i < n and text[i] == "]":
                i += 1
                return items
            raise err("数组元素后期待 , 或 ]", i)

    def parse_value():
        nonlocal i
        if i >= n:
            raise err("值缺席", i)
        c = text[i]
        if c == '"':
            return parse_string()
        if text.startswith("true", i):
            i += 4
            return True
        if text.startswith("false", i):
            i += 5
            return False
        if c == "[":
            return parse_array()
        if c == "-" or c.isdigit():
            j = i + 1 if c == "-" else i
            while j < n and text[j].isdigit():
                j += 1
            tok = text[i:j]
            if tok in ("", "-"):
                raise err("数值残缺", i)
            if j < n and text[j] in ".eExXoObB":
                raise err("数值带浮点/进制标记——不在子集", j)
            i = j
            return int(tok)
        raise err(f"不支持的字面量 {c!r}", i)

    def expect_eol() -> None:
        nonlocal i
        skip_ws(False)
        if i < n and text[i] not in "\r\n":
            raise err("行尾只许注释", i)

    while True:
        skip_ws(True)
        if i >= n:
            return root
        if text[i] == "[":
            if text.startswith("[[", i):
                raise err("数组表 [[...]] 不在子集", i)
            i += 1
            parts = []
            while True:
                parts.append(parse_key())
                skip_ws(False)
                if i < n and text[i] == ".":
                    i += 1
                    continue
                break
            if i >= n or text[i] != "]":
                raise err("表头未闭合", i)
            i += 1
            expect_eol()
            key = tuple(parts)
            if key in declared_tables:
                raise err(f"表头 {'.'.join(parts)} 重复声明", i)
            declared_tables.add(key)
            cur = root
            for p in parts:
                nxt = cur.get(p)
                if nxt is None:
                    nxt = {}
                    cur[p] = nxt
                if not isinstance(nxt, dict):
                    raise err(f"表头冲突：{p} 已是值", i)
                cur = nxt
            continue
        key = parse_key()
        skip_ws(False)
        if i >= n or text[i] != "=":
            raise err("期待 =", i)
        i += 1
        skip_ws(False)
        if key in cur:
            raise err(f"键 {key} 在同表内重复", i)
        cur[key] = parse_value()
        expect_eol()


def load_pin_text(text: str, src: str) -> dict:
    """tomllib 优先（≥3.11），MiniToml 兜底（3.9/3.10 或强制 env 钮）。"""
    if os.environ.get("BERRYOS_PIN_PARSER", "").lower() != "mini":
        try:
            import tomllib  # Python ≥3.11
        except ModuleNotFoundError:
            pass
        else:
            try:
                return tomllib.loads(text)
            except Exception as e:  # tomllib.TOMLDecodeError
                raise PinError(f"{src}: TOML 语法红（tomllib）：{e}")
    return _parse_minitoml(text, src)


# ---------------------------------------------------------------------------
# schema 校验
# ---------------------------------------------------------------------------

def _is_ne_str(x) -> bool:
    return isinstance(x, str) and x.strip() != ""


def _is_int(x) -> bool:
    # bool 是 int 子类——pass_cap = true 必须红，不许蒙混成 1。
    return isinstance(x, int) and not isinstance(x, bool)


def _lens_cycle(lenses: dict):
    """镜头 requires 成环检测（DFS 三色）。返回环路径列表或 None。"""
    color = {k: 0 for k in lenses if isinstance(lenses[k], dict)}
    stack: list = []

    def dfs(k):
        color[k] = 1
        stack.append(k)
        for dep in lenses[k].get("requires", []):
            if dep not in color:
                continue  # 指向不存在的镜头由必填腿另案报
            if color[dep] == 1:
                return stack[stack.index(dep):] + [dep]
            if color[dep] == 0:
                found = dfs(dep)
                if found:
                    return found
        color[k] = 2
        stack.pop()
        return None

    for k in list(color):
        if color[k] == 0:
            found = dfs(k)
            if found:
                return found
    return None


def validate_pin(pin: dict):
    """返回 (errors, warnings)：errors 非空 = pin 红。"""
    errs: list = []
    warns: list = []

    reviewer = pin.get("reviewer")
    if not isinstance(reviewer, dict):
        errs.append("缺 [reviewer] 表（评审者身份）")
        reviewer = {}
    for k in ("lane", "model", "model_source"):
        if not _is_ne_str(reviewer.get(k)):
            errs.append(f"reviewer.{k} 缺席或非非空字符串")

    passes = pin.get("passes")
    if not isinstance(passes, dict):
        errs.append("缺 [passes] 表（四 pass 日程）")
        passes = {}
    for k in REQUIRED_PASSES:
        if not _is_ne_str(passes.get(k)):
            errs.append(f"passes.{k} 缺席或空（四 pass 各一句职责）")
    for k in passes:
        if k not in REQUIRED_PASSES:
            warns.append(f"passes.{k} 不在四 pass 日程内（机器不读，向前兼容）")

    levels = pin.get("levels")
    if not isinstance(levels, dict):
        errs.append("缺 [levels] 表（阻断级定义）")
        levels = {}
    for k in LEVEL_KEYS:
        lv = levels.get(k)
        if not isinstance(lv, dict):
            errs.append(f"levels.{k} 缺席")
            continue
        if not _is_ne_str(lv.get("meaning")):
            errs.append(f"levels.{k}.meaning 缺席或空")
        b = lv.get("blocking")
        if not isinstance(b, bool):
            errs.append(f"levels.{k}.blocking 缺席或非布尔")
        elif b != EXPECTED_BLOCKING[k]:
            errs.append(f"levels.{k}.blocking={b} 违反阻断级契约（应为 {EXPECTED_BLOCKING[k]}）")
    for k in levels:
        if k not in LEVEL_KEYS:
            errs.append(f"levels.{k} 越出 L1/L2/L3（级别词汇面钉死在工具侧）")

    lenses = pin.get("lenses")
    if not isinstance(lenses, dict) or not lenses:
        errs.append("缺 [lenses.*] 镜头字典（至少一镜）")
        lenses = lenses if isinstance(lenses, dict) else {}
    for name, body in lenses.items():
        if not isinstance(body, dict):
            errs.append(f"lenses.{name} 不是表")
            continue
        if not _is_ne_str(body.get("focus")):
            errs.append(f"lenses.{name}.focus 缺席或空")
        req = body.get("requires", [])
        if not isinstance(req, list) or not all(_is_ne_str(x) for x in req):
            errs.append(f"lenses.{name}.requires 须为字符串数组")
    for name, body in lenses.items():
        if isinstance(body, dict):
            for dep in body.get("requires", []):
                if isinstance(dep, str) and dep not in lenses:
                    errs.append(f"lenses.{name}.requires 指向不存在的镜头 {dep}")
    cyc = _lens_cycle(lenses)
    if cyc:
        errs.append("镜头 requires 成环：" + " -> ".join(cyc))

    rules = pin.get("lens_rules")
    if not isinstance(rules, dict):
        errs.append("缺 [lens_rules]（always + 条件加载）")
        rules = {}
    always = rules.get("always")
    if not isinstance(always, list) or not always or not all(_is_ne_str(x) for x in always):
        errs.append("lens_rules.always 须为非空字符串数组")
        always = []
    else:
        if len(set(always)) != len(always):
            errs.append("lens_rules.always 有重复镜头")
        for x in always:
            if x not in lenses:
                errs.append(f"lens_rules.always 引用不存在的镜头 {x}")
    load = rules.get("load", {})
    if not isinstance(load, dict):
        errs.append("lens_rules.load 须为表（路径前缀 → 附加镜头）")
        load = {}
    for prefix, adds in load.items():
        if not _is_ne_str(prefix):
            errs.append(f"lens_rules.load 键 {prefix!r} 不是非空路径前缀")
        if not isinstance(adds, list) or not all(_is_ne_str(x) for x in adds):
            errs.append(f"lens_rules.load[{prefix!r}] 须为字符串数组")
            continue
        if len(set(adds)) != len(adds):
            errs.append(f"lens_rules.load[{prefix!r}] 有重复镜头")
        for x in adds:
            if x not in lenses:
                errs.append(f"lens_rules.load[{prefix!r}] 引用不存在的镜头 {x}")
            elif x in always:
                errs.append(f"镜头 {x} 同时在 always 与条件加载（恒上镜不该条件加载）")
    for name in lenses:
        in_load = any(
            isinstance(adds, list) and name in adds for adds in load.values()
        )
        if name not in always and not in_load:
            warns.append(f"镜头 {name} 既不在 always 也没被任何前缀加载（策略孤儿，机器不读）")

    cap = pin.get("cap")
    if not isinstance(cap, dict):
        errs.append("缺 [cap] 表（pass 帽）")
        cap = {}
    pc = cap.get("pass_cap")
    if not _is_int(pc):
        errs.append("cap.pass_cap 缺席或非整数")
    elif not (0 < pc <= 1000):
        errs.append(f"cap.pass_cap={pc} 越界（须 0 < 值 ≤ 1000——帽写成 33000 多半是笔误）")

    for k in pin:
        if k not in PIN_TOP_KEYS:
            warns.append(f"顶层键 {k} 机器不读（向前兼容）")
    return errs, warns


def parse_paths(csv: str) -> list:
    return [p.strip() for p in csv.split(",") if p.strip()]


def prefix_hit(path: str, prefix: str) -> bool:
    """组件边界前缀匹配：path == prefix 或 path 以 prefix/ 开头（仓相对路径口径）。"""
    return path == prefix or path.startswith(prefix + "/")


def active_lenses(pin: dict, paths: list) -> list:
    """这批路径该上场的镜头（保持 always 顺序 + 条件镜追加序）。"""
    out = list(pin["lens_rules"]["always"])
    for prefix, adds in pin["lens_rules"].get("load", {}).items():
        if any(prefix_hit(p, prefix) for p in paths):
            for lens in adds:
                if lens not in out:
                    out.append(lens)
    return out


def live_check(pin: dict, jobs_path: Path):
    """返回 (rc, msg)。漂移红；环境缺席（文件缺/坏/lane 缺席）只警告。"""
    lane = pin.get("reviewer", {}).get("lane", "")
    want = pin.get("reviewer", {}).get("model", "")
    try:
        raw = jobs_path.read_text(encoding="utf-8")
    except OSError as e:
        return 0, f"warning: jobs.json 读不到（{jobs_path}: {e}）——环境缺席只警告，不判漂移"
    try:
        jobs = json.loads(raw).get("jobs", [])
    except json.JSONDecodeError as e:
        return 0, f"warning: jobs.json 非法 JSON（{e}）——环境坏只警告，不判漂移"
    for job in jobs:
        if isinstance(job, dict) and job.get("name") == lane:
            got = job.get("model")
            if got == want:
                return 0, f"live 对账一致：lane={lane} model={want}"
            return 1, (f"live 漂移红：lane={lane} 实际 model={got!r}，pin 记 {want!r}"
                       "——评审者被换而口径没跟上；先显式改 pin（可审提交）再跑")
    return 0, f"warning: jobs.json 里没有 lane={lane}——环境缺席只警告"


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def _usage() -> int:
    print("usage: review-pin-check.py [check] [--pin <p>] [--live] [--jobs <p>]\n"
          "       review-pin-check.py lenses [--pin <p>] [--paths <逗号分隔>]\n"
          "       review-pin-check.py --selftest", file=sys.stderr)
    return 2


def _read_pin(pin_path: Path):
    try:
        text = pin_path.read_text(encoding="utf-8")
    except OSError as e:
        raise PinError(f"pin 读不到（{pin_path}: {e}）——评审策略缺席=红")
    return load_pin_text(text, str(pin_path))


def run_check(argv: list) -> int:
    cmd = "check"
    if argv and argv[0] in ("check", "lenses"):
        cmd = argv.pop(0)
    pin_path = DEFAULT_PIN
    jobs_path = None
    live = False
    paths_csv = ""
    while argv:
        a = argv.pop(0)
        if a == "--pin" and argv:
            pin_path = Path(argv.pop(0))
        elif a == "--jobs" and argv:
            jobs_path = Path(argv.pop(0))
        elif a == "--paths" and argv:
            paths_csv = argv.pop(0)
        elif a == "--live":
            live = True
        else:
            return _usage()
    try:
        pin = _read_pin(pin_path)
    except PinError as e:
        print(f"review-pin-check: {e}", file=sys.stderr)
        return 1
    errs, warns = validate_pin(pin)
    for w in warns:
        print(f"review-pin-check: {w}", file=sys.stderr)
    if errs:
        for e in errs:
            print(f"review-pin-check: pin 红——{e}", file=sys.stderr)
        return 1
    if cmd == "lenses":
        for name in active_lenses(pin, parse_paths(paths_csv)):
            print(name)
        return 0
    print(f"review-pin-check: pin 绿（{pin_path}）")
    if live:
        jp = jobs_path or Path(pin["reviewer"]["model_source"]).expanduser()
        rc, msg = live_check(pin, jp)
        if rc == 0 and msg.startswith("warning:"):
            print(f"review-pin-check: {msg}", file=sys.stderr)
        elif rc == 0:
            print(f"review-pin-check: {msg}")
        else:
            print(f"review-pin-check: {msg}", file=sys.stderr)
        return rc
    return 0


# ---------------------------------------------------------------------------
# selftest：合成夹具 + 真 pin 只读腿。零网络、零真仓写。
# ---------------------------------------------------------------------------

_GOOD_PIN = """
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

[lens_rules]
always = ["alpha", "beta"]

[lens_rules.load]
gui = ["gamma"]

[lenses.gamma]
focus = "f-gamma"
requires = ["beta"]

[cap]
pass_cap = 33
"""


def _mutate_good(**replacements) -> str:
    """对好夹具做定点替换：键为「旧文本子串」，值为新文本；缺省删除该子串。"""
    text = _GOOD_PIN
    for old, new in replacements.items():
        text = text.replace(old, new)
    return text


def selftest() -> int:
    fails = 0

    def check(name: str, cond: bool) -> None:
        nonlocal fails
        print(f"review-pin-check 自检: {'腿绿' if cond else 'FAIL'} {name}")
        fails += 0 if cond else 1

    def run(args: list, env_extra: dict = None):
        env = {**os.environ, **(env_extra or {})}
        return subprocess.run([sys.executable, str(Path(__file__).resolve()), *args],
                              capture_output=True, text=True, env=env)

    py = Path(__file__).resolve()
    with tempfile.TemporaryDirectory() as td:
        t = Path(td)
        good = t / "good.toml"
        good.write_text(_GOOD_PIN, encoding="utf-8")

        # ① 好腿
        r = run(["check", "--pin", str(good)])
        check("好 pin 过（rc0）", r.returncode == 0)
        # ② 缺字段红：去掉 reviewer.model 行
        bad = t / "no-model.toml"
        bad.write_text(_GOOD_PIN.replace('model = "model-x"\n', ""), encoding="utf-8")
        r = run(["check", "--pin", str(bad)])
        check("缺字段红（reviewer.model）", r.returncode == 1 and "reviewer.model" in r.stderr)
        # ③ 坏类型红：pass_cap 写成字符串
        bad.write_text(_GOOD_PIN.replace("pass_cap = 33", 'pass_cap = "33"'), encoding="utf-8")
        r = run(["check", "--pin", str(bad)])
        check("坏类型红（pass_cap 字符串）", r.returncode == 1 and "pass_cap" in r.stderr)
        # ④ 正整数红：pass_cap = 0
        bad.write_text(_GOOD_PIN.replace("pass_cap = 33", "pass_cap = 0"), encoding="utf-8")
        r = run(["check", "--pin", str(bad)])
        check("pass_cap=0 红（须正整数）", r.returncode == 1 and "pass_cap" in r.stderr)
        # ⑤ 成环红：alpha → gamma → alpha
        cyc = _GOOD_PIN.replace("requires = []", 'requires = ["gamma"]', 1)
        cyc = cyc.replace('requires = ["beta"]', 'requires = ["alpha"]', 1)
        bad.write_text(cyc, encoding="utf-8")
        r = run(["check", "--pin", str(bad)])
        check("镜头成环红", r.returncode == 1 and "成环" in r.stderr)
        # ⑥ requires 指空镜头红
        bad.write_text(_GOOD_PIN.replace('requires = ["alpha"]', 'requires = ["ghost"]'), encoding="utf-8")
        r = run(["check", "--pin", str(bad)])
        check("requires 指向不存在镜头红", r.returncode == 1 and "ghost" in r.stderr)
        # ⑦ 阻断级契约红：L3 写成阻断
        bad.write_text(_GOOD_PIN.replace('meaning = "m3"\nblocking = false', 'meaning = "m3"\nblocking = true'), encoding="utf-8")
        r = run(["check", "--pin", str(bad)])
        check("L3 blocking=true 红（阻断级契约）", r.returncode == 1 and "契约" in r.stderr)
        # ⑧ always 引用不存在镜头红
        bad.write_text(_GOOD_PIN.replace('always = ["alpha", "beta"]', 'always = ["alpha", "ghost"]'), encoding="utf-8")
        r = run(["check", "--pin", str(bad)])
        check("always 引用空镜头红", r.returncode == 1)
        # ⑨ 恒上镜同时条件加载红
        bad.write_text(_GOOD_PIN.replace('gui = ["gamma"]', 'gui = ["beta"]'), encoding="utf-8")
        r = run(["check", "--pin", str(bad)])
        check("恒上镜条件加载矛盾红", r.returncode == 1 and "不该条件加载" in r.stderr)
        # ⑩ 坏语法红（字符串没闭合）
        bad.write_text(_GOOD_PIN.replace('model = "model-x"', 'model = "model-x'), encoding="utf-8")
        r = run(["check", "--pin", str(bad)])
        check("TOML 语法坏红", r.returncode == 1)
        # ⑪ lenses 求解：条件加载两腿
        r = run(["lenses", "--pin", str(good), "--paths", "kernel/a.rs,docs/x.md"])
        check("lenses 求解（不触 gui=基础镜）", r.returncode == 0 and r.stdout.split() == ["alpha", "beta"])
        r = run(["lenses", "--pin", str(good), "--paths", "gui/main.rs"])
        check("lenses 求解（触 gui=追加 gamma）", r.returncode == 0 and r.stdout.split() == ["alpha", "beta", "gamma"])
        # ⑫ --live 四腿：对账一致/漂移红/文件缺警告/lane 缺警告
        jobs = t / "jobs.json"
        jobs.write_text(json.dumps({"jobs": [{"name": "review-lane-x", "model": "model-x"}]}), encoding="utf-8")
        r = run(["check", "--pin", str(good), "--live", "--jobs", str(jobs)])
        check("live 对账一致 rc0", r.returncode == 0)
        jobs.write_text(json.dumps({"jobs": [{"name": "review-lane-x", "model": "model-other"}]}), encoding="utf-8")
        r = run(["check", "--pin", str(good), "--live", "--jobs", str(jobs)])
        check("live 模型漂移 rc1", r.returncode == 1 and "漂移" in r.stderr)
        r = run(["check", "--pin", str(good), "--live", "--jobs", str(t / "absent.json")])
        check("live 文件缺席只警告 rc0", r.returncode == 0 and "只警告" in r.stderr)
        jobs.write_text(json.dumps({"jobs": [{"name": "another-lane", "model": "model-x"}]}), encoding="utf-8")
        r = run(["check", "--pin", str(good), "--live", "--jobs", str(jobs)])
        check("live lane 缺席只警告 rc0", r.returncode == 0 and "只警告" in r.stderr)
        # ⑬ 双解析器一致（tomllib 在席才可比）：好夹具 + 真 pin
        try:
            import tomllib
            has_tomllib = True
        except ModuleNotFoundError:
            has_tomllib = False
        if has_tomllib:
            same = tomllib.loads(_GOOD_PIN) == _parse_minitoml(_GOOD_PIN)
            real_text = DEFAULT_PIN.read_text(encoding="utf-8")
            same_real = tomllib.loads(real_text) == _parse_minitoml(real_text)
            check("MiniToml 与 tomllib 同解（夹具/真 pin）", same and same_real)
        else:
            check("MiniToml 可用（无 tomllib 环境）", _parse_minitoml(_GOOD_PIN)["cap"]["pass_cap"] == 33)
        # ⑭ fallback 端到端：强制 MiniToml 也过真 pin
        r = run(["check"], env_extra={"BERRYOS_PIN_PARSER": "mini"})
        check("fallback 解析真 pin rc0（BERRYOS_PIN_PARSER=mini）", r.returncode == 0)
        # ⑮ 真 pin（默认路径）只读腿
        r = run(["check"])
        check("真 pin 校验 rc0", r.returncode == 0)
        r = run(["lenses", "--paths", "kernel/a.rs,user/berry-ui/b.rs"])
        check("真 pin 条件求解（kernel+GUI 路径全镜）",
              r.returncode == 0 and set(r.stdout.split()) ==
              {"adversarial", "integration", "consistency", "test-coverage", "perf", "concurrency", "design"})
        # ⑯ 用法错 rc2
        r = run(["--bogus"])
        check("用法错 rc2", r.returncode == 2)

    print(f"review-pin-check 自检：{'PASS' if fails == 0 else f'FAIL（{fails} 腿红）'}"
          "（好/缺字段/坏类型/非正整数/成环/空引用/契约矛盾/语法/求解/live 四腿/双解析器/真 pin）")
    return 0 if fails == 0 else 1


def main(argv: list) -> int:
    if "--selftest" in argv:
        return selftest()
    return run_check(argv)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
