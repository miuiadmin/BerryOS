#!/usr/bin/env python3
r"""mkapp——.bap 应用包打包器（app-0，`docs/app-0实施设计草案.md` §4；
ADR #43/#44：应用 = 纯用户态协议约定，包 = cpio(newc) 容器，内核零概念）。

用法：mkapp.py <out.bap> <app-dir>        # 打包（app-dir = {manifest.toml, bin/}
      mkapp.py --selftest                 # 门禁自检（零依赖；挂 just test-host，
                                          #          mkfat --check-fixture 同位）

包格式（§1 冻结）：cpio newc 恰两条目——`manifest.toml` + 唯一 `bin/`
前缀条目（ELF 本体，条目名 = manifest entry）；进 rootfs.cpio 时整体作
为单条目 `app/<ID>.bap`（'a' < 'b'，排序天然在 bin/ 之前）。

门禁五道（任一失败 = 非零退 + 一行理由，loud not silent）：
  1. manifest 可解析（tomllib）且结构/值域过检（镜像 berry-manifest）；
  2. **规范形重序列化逐字节相等**——berry-manifest 只收 TOML 子集，
     tomllib 收全集（hex/下划线整数、单引号、内联表、多行串、注释、
     CRLF…），本道是把「tomllib 收下」收紧到「子集收下」的唯一执法点；
  3. entry == 包内 bin/ 唯一文件的完整路径（`bin/<名>`，无双重前缀）；
  4. 输出文件名 == `<id>.bap`；
  5. bin 文件头 4 字节 == \x7fELF。
另：app-dir 内容恰 {manifest.toml, bin/}，bin/ 恰一个常规文件；
manifest ≤ MANIFEST_MAX、归档 ≤ BAP_MAX。

规范形（第 2 道的比较基准，§2 冻结）：`[app]` 五行定序
id/version/entry/type/intent + `[capabilities]` console + 可选
`[runtime]` stack_pages（出现则必须显式给值）。空 `[runtime]` 解析器
合法但非规范形 → 门禁拒（byte 不等）——省略或显式，二选一。

编码器纪律（§4 单编码器）：cpio 编码 `from mkcpio import build_archive`
复用——禁止第二份 newc/trailer 逻辑（双编码器 = 漂移面）；条目按
name 字节排序（确定性：同输入同字节，AI 测试友好化）。

常量对账：以下帽/词表镜像 `user/berry-manifest/src/lib.rs` 公开常量
（mkfat ↔ berry-fatfs 同款义务）——改 berry-manifest 常量须同步此处。
"""
import os
import re
import sys

try:
    import tomllib
except ModuleNotFoundError:  # CLT python3=3.9 无 tomllib（gen_abi 同款门）
    sys.stderr.write("mkapp: 需要 Python >= 3.11（tomllib）\n")
    sys.exit(1)

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mkcpio import build_archive  # noqa: E402（单编码器纪律，见模块头注）

# ---- 帽与词表（对账锚点：user/berry-manifest/src/lib.rs）----
MANIFEST_MAX = 1024
BAP_MAX = 4 * 1024 * 1024
ID_MAX = 24
VERSION_MAX = 16
ENTRY_MAX = 32
STACK_PAGES_MIN = 1
STACK_PAGES_MAX = 64
INTENTS = (
    "unspecified",
    "latency_critical",
    "interactive",
    "utility",
    "background",
    "maintenance",
)

ID_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*")
APP_KEYS = ("id", "version", "entry", "type", "intent")


class Reject(Exception):
    """门禁拒绝（一行理由，非零退出）。"""


class M:
    """过检后的 manifest 视图（规范形重序列化的数据源）。"""

    def __init__(self, app, caps, runtime):
        self.id = app["id"]
        self.version = app["version"]
        self.entry = app["entry"]
        self.console = caps["console"]
        self.has_runtime = runtime is not None
        self.stack_pages = runtime["stack_pages"] if runtime else 8
        self.intent = app["intent"]

    def canonical(self) -> str:
        out = (
            "[app]\n"
            f'id = "{self.id}"\n'
            f'version = "{self.version}"\n'
            f'entry = "{self.entry}"\n'
            'type = "app"\n'
            f'intent = "{self.intent}"\n'
            f"\n[capabilities]\nconsole = {'true' if self.console else 'false'}\n"
        )
        if self.has_runtime:
            out += f"\n[runtime]\nstack_pages = {self.stack_pages}\n"
        return out


def _check_str(v, what: str, maxlen: int) -> None:
    # parse_string 镜像：可打印 ASCII 0x20..=0x7E、无反斜杠、无内嵌引号
    if not isinstance(v, str):
        raise Reject(f"manifest: {what} 应为字符串")
    if not 1 <= len(v) <= maxlen:
        raise Reject(f"manifest: {what} 空/超长（1..={maxlen}）")
    for ch in v:
        if not (0x20 <= ord(ch) <= 0x7E) or ch in ('\\', '"'):
            raise Reject(f"manifest: {what} 非法字符 {ch!r}")


def check_manifest_text(text: str) -> M:
    """manifest 文本 → 过检视图；门禁第 1+2 道（结构/值域 + 规范形）。"""
    # 字节面前置（berry-manifest 同款：LF only、拒 tab——tomllib 对两者
    # 都比我们宽容，先拒免得把锅甩给规范形比较）
    if "\r" in text:
        raise Reject("manifest: CR（CRLF）拒收——LF only")
    if "\t" in text:
        raise Reject("manifest: tab 拒收")
    try:
        doc = tomllib.loads(text)
    except tomllib.TOMLDecodeError as e:
        raise Reject(f"manifest: TOML 解析失败： {e}")

    unknown = set(doc) - {"app", "capabilities", "runtime"}
    if unknown:
        raise Reject(f"manifest: 未知节 {sorted(unknown)}")
    for need in ("app", "capabilities"):
        if not isinstance(doc.get(need), dict):
            raise Reject(f"manifest: 缺 [{need}]")

    app = doc["app"]
    if set(app) != set(APP_KEYS):
        raise Reject(f"manifest: [app] 键集应为五键 {sorted(APP_KEYS)}")
    _check_str(app["id"], "id", ID_MAX)
    if not ID_RE.fullmatch(app["id"]):
        raise Reject("manifest: id 字符集 [A-Za-z0-9._-]、首字符字母数字")
    _check_str(app["version"], "version", VERSION_MAX)
    _check_str(app["entry"], "entry", ENTRY_MAX)
    if not app["entry"].startswith("bin/") or len(app["entry"]) <= len("bin/"):
        raise Reject("manifest: entry 应为 bin/ 前缀完整路径")
    if app["type"] != "app":
        raise Reject('manifest: type 仅 "app"')
    if app["intent"] not in INTENTS:
        raise Reject(f"manifest: intent 应为六词之一 {INTENTS}")

    caps = doc["capabilities"]
    if set(caps) != {"console"}:
        raise Reject("manifest: [capabilities] 仅 console 一键")
    if type(caps["console"]) is not bool:
        raise Reject("manifest: console 应为 bool")

    runtime = doc.get("runtime")
    if runtime is not None:
        if set(runtime) != {"stack_pages"}:
            raise Reject("manifest: [runtime] 仅 stack_pages 一键")
        sp = runtime["stack_pages"]
        if type(sp) is not int:  # bool 是 int 子类，type() 才拦得住
            raise Reject("manifest: stack_pages 应为整数")
        if not STACK_PAGES_MIN <= sp <= STACK_PAGES_MAX:
            raise Reject(
                f"manifest: stack_pages 越界"
                f" [{STACK_PAGES_MIN},{STACK_PAGES_MAX}]"
            )

    m = M(app, caps, runtime)
    if text != m.canonical():
        raise Reject(
            "manifest: 非规范形（hex/下划线整数、单引号、内联表、"
            "多行串、注释、多余空白等 tomllib 收而门禁不收）"
        )
    return m


def build(out_path: str, app_dir: str) -> None:
    mf_path = os.path.join(app_dir, "manifest.toml")
    bin_dir = os.path.join(app_dir, "bin")
    if not os.path.isfile(mf_path) or not os.path.isdir(bin_dir):
        raise Reject(f"{app_dir}: 应含 manifest.toml 与 bin/")
    extra = set(os.listdir(app_dir)) - {"manifest.toml", "bin"}
    if extra:
        raise Reject(f"{app_dir}: 未知内容 {sorted(extra)}")
    bins = os.listdir(bin_dir)
    if len(bins) != 1:
        raise Reject(f"{bin_dir}: 应恰一个文件， 得 {len(bins)}")
    bin_name = bins[0]
    bin_path = os.path.join(bin_dir, bin_name)
    if not os.path.isfile(bin_path):
        raise Reject(f"{bin_path}: 应为常规文件")

    with open(mf_path, "rb") as f:
        raw = f.read()
    if len(raw) > MANIFEST_MAX:
        raise Reject(f"manifest.toml 超 MANIFEST_MAX {MANIFEST_MAX}B")
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError:
        raise Reject("manifest.toml 非 ASCII")
    m = check_manifest_text(text)

    # 门禁 3：entry == bin/ 唯一文件的完整包内路径
    if m.entry != f"bin/{bin_name}":
        raise Reject(f"manifest entry {m.entry!r} != bin/{bin_name!r}")
    # 门禁 4：输出名 == <id>.bap
    if os.path.basename(out_path) != f"{m.id}.bap":
        raise Reject(f"输出名应为 {m.id}.bap")
    # 门禁 5：ELF 魔数
    with open(bin_path, "rb") as f:
        elf = f.read()
    if elf[:4] != b"\x7fELF":
        raise Reject(f"{bin_path}: 非 ELF（头 4 字节 {elf[:4]!r}）")

    items = sorted(
        [
            (b"manifest.toml", raw),
            (m.entry.encode("ascii"), elf),
        ]
    )
    blob = build_archive(items)
    if len(blob) > BAP_MAX:
        raise Reject(f"包体 {len(blob)}B 超 BAP_MAX {BAP_MAX}")
    with open(out_path, "wb") as f:
        f.write(blob)
    print(
        f"mkapp: {out_path} {len(blob)}B {m.id} v{m.version} "
        f"console={'true' if m.console else 'false'} "
        f"stack={m.stack_pages} intent={m.intent}"
    )


BASE = (
    "[app]\n"
    'id = "APPDEMO"\n'
    'version = "0.1.0"\n'
    'entry = "bin/appdemo"\n'
    'type = "app"\n'
    'intent = "utility"\n'
    "\n[capabilities]\nconsole = true\n"
)


def selftest() -> int:
    """门禁自检（§2）：负样本 = tomllib 收下而门禁必须拒的形态——证明
    第 2 道规范形比较真的在收紧 tomllib 的超集；正对照 = 规范形必须过
    （防空转：check 恒拒也满足「负样本全拒」）。"""
    neg = [
        ("dotted-key", BASE.replace('id = "APPDEMO"', "id.x = \"APPDEMO\"")),
        ("hex-int", BASE + "\n[runtime]\nstack_pages = 0x8\n"),
        ("underscore-int", BASE + "\n[runtime]\nstack_pages = 1_6\n"),
        ("crlf", BASE.replace("\n", "\r\n")),
        (
            "escape-in-string",
            BASE.replace('version = "0.1.0"', 'version = "0.1\\n0"'),
        ),
        ("inline-table", "runtime = { stack_pages = 8 }\n\n" + BASE),
        (
            "multiline-string",
            BASE.replace('id = "APPDEMO"', 'id = """APPDEMO"""'),
        ),
        ("single-quotes", BASE.replace('id = "APPDEMO"', "id = 'APPDEMO'")),
        ("trailing-comment", BASE.replace("console = true", "console = true # y")),
    ]
    bad = 0
    for label, text in neg:
        try:
            check_manifest_text(text)
        except Reject:
            print(f"  selftest 拒（预期） : {label}")
            continue
        print(f"  selftest FAIL: {label} 被门禁放行")
        bad += 1
    pos = [
        ("canonical", BASE),
        ("canonical-runtime", BASE + "\n[runtime]\nstack_pages = 8\n"),
    ]
    for label, text in pos:
        try:
            m = check_manifest_text(text)
            print(f"  selftest 过（预期） : {label} id={m.id}")
        except Reject as e:
            print(f"  selftest FAIL: {label} 被门禁误拒： {e}")
            bad += 1
    if bad:
        print(f"mkapp selftest: FAIL x{bad}")
        return 1
    print("mkapp selftest: ok")
    return 0


def main() -> int:
    args = sys.argv[1:]
    if args == ["--selftest"]:
        return selftest()
    if len(args) != 2:
        sys.stderr.write(__doc__)
        return 2
    try:
        build(args[0], args[1])
    except Reject as e:
        sys.stderr.write(f"mkapp: {e}\n")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
