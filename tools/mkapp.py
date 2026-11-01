#!/usr/bin/env python3
r"""mkapp——.bap 应用包打包器（app-0，`docs/app-0实施设计草案.md` §4；
SDK-1a 增量 `docs/SDK-1a实施设计草案.md` §4；ADR #43/#44：应用 = 纯用户态
协议约定，包 = cpio(newc) 容器，内核零概念）。

用法：mkapp.py <out.bap> <app-dir>        # 打包（app-dir = {manifest.toml, bin/}）
      mkapp.py --check <path.bap>         # 复验既有包，只验不写（SDK-1a §4.4）
      mkapp.py --new <dir> --id <ID>      # 应用脚手架四件（SDK-1a §4.5）
      mkapp.py --selftest                 # 门禁自检（零依赖；挂 just test-host，
                                          #          mkfat --check-fixture 同位）

包格式（app-0 §1 冻结 + SDK-1a §2.4 增）：cpio newc 恰 2 或 3 条目——
`manifest.toml` + 唯一 `bin/` 前缀条目（ELF 本体，条目名 = manifest entry）
+ `SHA256SUMS`（SDK-1a 起产包恒在；`--check` 对 2 条目历史包同样可验）。
SHA256SUMS 内容 = 恰两行「64 位小写 hex + 双空格 + 条目名」（GNU
sha256sum 文本惯例）、LF 收尾、行序 = 条目名字节序；入包排序天然
`SHA256SUMS`(0x53) < `bin/…`(0x62) < `manifest.toml`(0x6d)。进 rootfs.cpio
时整体作为单条目 `app/<ID>.bap`（'a' < 'b'，排序天然在 bin/ 之前）。

门禁（任一失败 = 非零退 + 一行理由，loud not silent）：
  1. manifest 可解析（tomllib）且结构/值域过检（镜像 berry-manifest）+
     SDK-1a 三值链执法 min ≤ target ≤ compile ≤ SDK_CURRENT（hvigor
     00303015 对应物——「不能面向未来 SDK 编译」在打包期炸响）；
  2. **规范形重序列化逐字节相等**——berry-manifest 只收 TOML 子集，
     tomllib 收全集（hex/下划线整数、单引号、内联表、多行串、注释、
     CRLF…），本道是把「tomllib 收下」收紧到「子集收下」的唯一执法点；
  3. entry == 包内 bin/ 唯一文件的完整路径（`bin/<名>`，无双重前缀）；
  4. 输出文件名 == `<id>.bap`；
  5. bin 文件头 4 字节 == \x7fELF。
另：app-dir 内容恰 {manifest.toml, bin/}，bin/ 恰一个常规文件；
manifest ≤ MANIFEST_MAX、归档 ≤ BAP_MAX。

规范形（第 2 道的比较基准，SDK-1a §3 冻结）：`[app]` 五行定序
id/version/entry/type/intent + `[compat]?`（min/target/compile，int 三元组
重发射）+ `[capabilities]` console,fs?,net?（fs/net 数组无空格形）+
`[runtime]?` stack_pages + `[security]?`（profile/key_lineage）；节间恰一
空行。旧形（五平面全缺省）输出与 v0 canonical() 逐字节相同——add-only
执法锚。空 `[runtime]` 解析器合法但非规范形 → 门禁拒（byte 不等）——
省略或显式，二选一。

编码器纪律（§4 单编码器）：cpio 编码 `from mkcpio import build_archive`
复用——禁止第二份 newc/trailer 逻辑（双编码器 = 漂移面）；条目按
name 字节排序（确定性：同输入同字节，AI 测试友好化）。`--check` 的走查
器 `_cpio_entries` 是宿主侧唯一解码器（mkcpio 只产不收、svcmgr 侧在
guest 内——对读 mkcpio.entry 编码几何，非第二编码器）。

常量对账：以下帽/词表镜像 `user/berry-manifest/src/lib.rs` 公开常量
（mkfat ↔ berry-fatfs 同款义务）——改 berry-manifest 常量须同步此处；
SDK-1a 起 SDK_CURRENT/FS_SCOPES/NET_SCOPES/VER_RE/SEC_RE 同款义务
（SDK 级单一事实源 = berry-manifest，此处镜像）。抬级流程 = 改 berry-
manifest 两常量 + 此处镜像 + `docs/应用开发指南.md`。
"""
import hashlib
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

# ---- SDK-1a 常量（对账锚点同上；SDK 级单一事实源 = berry-manifest）----
SDK_CURRENT = (0, 1, 0)
SDK_CURRENT_STR = "0.1.0"
FS_SCOPES = ("rootfs",)
NET_SCOPES = ("client",)
VER_RE = re.compile(r"^(0|[1-9][0-9]{0,2})\.(0|[1-9][0-9]{0,2})\.(0|[1-9][0-9]{0,2})$")
SEC_RE = re.compile(r"^[a-z0-9-]{1,16}$")

ID_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*")
APP_KEYS = ("id", "version", "entry", "type", "intent")

# SHA256SUMS 行形：64 位小写 hex + 恰两空格 + 条目名（GNU sha256sum 文本惯例）
SUMS_LINE_RE = re.compile(rb"^([0-9a-f]{64})  (.+)$")


class Reject(Exception):
    """门禁拒绝（一行理由，非零退出）。"""


def _ver(v: tuple) -> str:
    """int 三元组 → `x.y.z`（canonical 重发射/诊断行用——文法禁前导零 ⇒
    「收下 → 重发射」恒等归一，§3.1 要点①）。"""
    return ".".join(str(x) for x in v)


class M:
    """过检后的 manifest 视图（规范形重序列化的数据源）。"""

    def __init__(self, app, caps, runtime, compat, sec):
        self.id = app["id"]
        self.version = app["version"]
        self.entry = app["entry"]
        self.console = caps["console"]
        self.has_runtime = runtime is not None
        self.stack_pages = runtime["stack_pages"] if runtime else 8
        self.intent = app["intent"]
        # SDK-1a（§4.1）：compat = None | (min, target, compile) int 三元组
        # 的三元组；fs/net = None | str（词表门已保证单元素数组——此处取
        # 元素，canonical 按 `["str"]` 无空格形重发射）；sec = None | (p, k)。
        self.compat = compat
        fs_val = caps.get("fs")
        self.fs = fs_val[0] if fs_val else None
        net_val = caps.get("net")
        self.net = net_val[0] if net_val else None
        self.sec = sec

    def canonical(self) -> str:
        # SDK-1a §3.1 定序：[app] → [compat]? → [capabilities] → [runtime]?
        # → [security]?；节间恰一空行；旧形（全缺省）输出与 v0 逐字节同。
        out = (
            "[app]\n"
            f'id = "{self.id}"\n'
            f'version = "{self.version}"\n'
            f'entry = "{self.entry}"\n'
            'type = "app"\n'
            f'intent = "{self.intent}"\n'
        )
        if self.compat:
            mn, tg, cp = self.compat
            out += "\n[compat]\n" f'min = "{_ver(mn)}"\ntarget = "{_ver(tg)}"\ncompile = "{_ver(cp)}"\n'
        out += f"\n[capabilities]\nconsole = {'true' if self.console else 'false'}\n"
        if self.fs:
            out += f'fs = ["{self.fs}"]\n'
        if self.net:
            out += f'net = ["{self.net}"]\n'
        if self.has_runtime:
            out += f"\n[runtime]\nstack_pages = {self.stack_pages}\n"
        if self.sec:
            p, k = self.sec
            out += f'\n[security]\nprofile = "{p}"\nkey_lineage = "{k}"\n'
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
    """manifest 文本 → 过检视图；门禁第 1+2 道（结构/值域 + 三值链 + 规范形）。"""
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

    unknown = set(doc) - {"app", "capabilities", "runtime", "compat", "security"}
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

    # SDK-1a：[compat] 三值域 + 链执法（§4.2）。isinstance 门 = 节级形态
    # 硬ening（tomllib 把 `compat = 5` 解成 int，set(int) 会 TypeError——
    # 崩栈不是门禁，与 [app]/[capabilities] 的 isinstance 门同口径）。
    vs = None
    compat = doc.get("compat")
    if compat is not None:
        if not isinstance(compat, dict) or set(compat) != {"min", "target", "compile"}:
            raise Reject("manifest: [compat] 应为 min/target/compile 三键齐备")
        vs = []
        for k in ("min", "target", "compile"):
            v = compat[k]
            if not isinstance(v, str) or not VER_RE.fullmatch(v):
                raise Reject("manifest: compat 值应为 x.y.z（每段 1-3 位十进制、无前导零）")
            vs.append(tuple(int(g) for g in VER_RE.fullmatch(v).groups()))
        if not vs[0] <= vs[1] <= vs[2]:
            raise Reject("manifest: compat 序不合（需 min ≤ target ≤ compile）")  # hvigor 00303015 对应物
        if vs[2] > SDK_CURRENT:
            raise Reject(f"manifest: compile {_ver(vs[2])} 超工具链当前级 {SDK_CURRENT_STR}")

    caps = doc["capabilities"]
    if not {"console"} <= set(caps) or set(caps) - {"console", "fs", "net"}:
        raise Reject("manifest: [capabilities] 键集应为 console(+fs/net)")
    if type(caps["console"]) is not bool:
        raise Reject("manifest: console 应为 bool")
    for key, vocab in (("fs", FS_SCOPES), ("net", NET_SCOPES)):
        if key in caps:
            v = caps[key]
            if not (isinstance(v, list) and len(v) == 1 and isinstance(v[0], str)
                    and v[0] in vocab):
                raise Reject(f"manifest: {key} 应为单元素数组，词表 {vocab}")

    runtime = doc.get("runtime")
    if runtime is not None:
        if not isinstance(runtime, dict) or set(runtime) != {"stack_pages"}:
            raise Reject("manifest: [runtime] 仅 stack_pages 一键")
        sp = runtime["stack_pages"]
        if type(sp) is not int:  # bool 是 int 子类，type() 才拦得住
            raise Reject("manifest: stack_pages 应为整数")
        if not STACK_PAGES_MIN <= sp <= STACK_PAGES_MAX:
            raise Reject(
                f"manifest: stack_pages 越界"
                f" [{STACK_PAGES_MIN},{STACK_PAGES_MAX}]"
            )

    # SDK-1a：[security] 预留节形态执法（值域 1..=16B [a-z0-9-]，齐备或齐缺）
    sec_t = None
    sec = doc.get("security")
    if sec is not None:
        if not isinstance(sec, dict) or set(sec) != {"profile", "key_lineage"}:
            raise Reject("manifest: [security] 应为 profile/key_lineage 两键齐备")
        for k in ("profile", "key_lineage"):
            if not isinstance(sec[k], str) or not SEC_RE.fullmatch(sec[k]):
                raise Reject("manifest: security 值应为 1..16B 的 [a-z0-9-]")
        sec_t = (sec["profile"], sec["key_lineage"])

    m = M(app, caps, runtime, tuple(vs) if vs is not None else None, sec_t)
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

    payload_items = sorted(
        [
            (b"manifest.toml", raw),
            (m.entry.encode("ascii"), elf),
        ]
    )
    # SDK-1a §4.3：第三条目 SHA256SUMS——payload 两行的 sha256 清单
    # （双空格、LF 收尾、行序 = 条目名字节序）。连同自身入包再排序：
    # 'S'(0x53) < 'b'(0x62) < 'm'(0x6d)，§2.4 条目定序由排序自动满足。
    sums = "".join(
        f"{hashlib.sha256(data).hexdigest()}  {name.decode('ascii')}\n"
        for name, data in payload_items
    ).encode("ascii")
    items = sorted(payload_items + [(b"SHA256SUMS", sums)])
    blob = build_archive(items)
    if len(blob) > BAP_MAX:
        raise Reject(f"包体 {len(blob)}B 超 BAP_MAX {BAP_MAX}")
    with open(out_path, "wb") as f:
        f.write(blob)
    # 产包摘要行（宿主侧可见性，无消费面）；SDK-1a 尾随增量在场才打
    line = (
        f"mkapp: {out_path} {len(blob)}B {m.id} v{m.version} "
        f"console={'true' if m.console else 'false'} "
        f"stack={m.stack_pages} intent={m.intent}"
    )
    if m.compat is not None:
        line += f" compat={_ver(m.compat[0])}..{_ver(m.compat[2])}"
    if m.fs:
        line += f" fs={m.fs}"
    if m.net:
        line += f" net={m.net}"
    print(line)


# ---- SDK-1a §4.4：--check（既有 .bap 复验，只验不写）----


def _cpio_entries(blob: bytes) -> list[tuple[bytes, bytes]]:
    """cpio(newc) 走查器（--check 专用；mkcpio 只产不收、svcmgr 侧在
    guest 内——宿主侧无第二实现可借，本函数对读 mkcpio.entry 的编码
    几何：magic 070701 + 13 × hex8，filesize = 域[6]、namesize = 域[11]，
    名起 off+110 不含 NUL，头+名垫 4、数据垫 4，TRAILER!!! 终止）。
    越界/坏魔数 = Reject（敌意包 loud，不静默截断）。"""
    out: list[tuple[bytes, bytes]] = []
    off = 0
    while True:
        if off + 110 > len(blob):
            raise Reject(f"bap: cpio 头越界 @ {off}")
        if blob[off : off + 6] != b"070701":
            raise Reject(f"bap: cpio 坏魔数 @ {off}")
        try:
            fields = [
                int(blob[off + 6 + i * 8 : off + 6 + (i + 1) * 8], 16)
                for i in range(13)
            ]
        except ValueError:
            raise Reject(f"bap: cpio 坏字段 @ {off}")
        filesize, namesize = fields[6], fields[11]
        if namesize < 1:
            raise Reject(f"bap: cpio 名长非法 @ {off}")
        name_off = off + 110
        if name_off + namesize > len(blob):
            raise Reject(f"bap: cpio 名越界 @ {off}")
        name = blob[name_off : name_off + namesize - 1]  # 剥尾 NUL
        data_off = (name_off + namesize + 3) & ~3  # 头+名垫 4
        if data_off + filesize > len(blob):
            raise Reject(f"bap: cpio 数据越界 @ {off}")
        if name == b"TRAILER!!!":
            break
        out.append((name, blob[data_off : data_off + filesize]))
        off = (data_off + filesize + 3) & ~3  # 数据垫 4
    return out


def _verify_sums(sums: bytes, payload: list[tuple[bytes, bytes]]) -> None:
    """SHA256SUMS 复验（§4.4 步 4）：LF 收尾、行数 = 条目数、双空格行形、
    行序 = 条目名字节序、逐文件重算比对；任一不符 = 拒。"""
    if not sums.endswith(b"\n"):
        raise Reject("bap: SHA256SUMS 应 LF 收尾")
    lines = sums[:-1].split(b"\n")
    expect = sorted(payload)
    if len(lines) != len(expect):
        raise Reject(f"bap: SHA256SUMS 行数 {len(lines)} != 条目数 {len(expect)}")
    for line, (name, data) in zip(lines, expect):
        mo = SUMS_LINE_RE.fullmatch(line)
        if not mo or mo.group(2) != name:
            raise Reject(f"bap: SHA256SUMS 行形/行序不符 {line!r}")
        if mo.group(1).decode("ascii") != hashlib.sha256(data).hexdigest():
            raise Reject(f"bap: SHA256SUMS 摘要不符 {name!r}")


def check_bap(path: str) -> None:
    """--check 五步（§4.4）：形状 → manifest 全量门禁 → entry 对表 →
    SHA256SUMS 复验（在场）→ ELF 魔数；通过打一行 OK。"""
    with open(path, "rb") as f:
        blob = f.read()
    if len(blob) > BAP_MAX:
        raise Reject(f"bap: 包体 {len(blob)}B 超 BAP_MAX {BAP_MAX}")
    entries = _cpio_entries(blob)
    names = [n for n, _ in entries]
    if len(names) != len(set(names)):
        raise Reject(f"bap: 条目重名 {sorted(set(names))}")
    if b"manifest.toml" not in names:
        raise Reject("bap: 缺 manifest.toml 条目")
    bin_entries = [n for n in names if n.startswith(b"bin/")]
    others = set(names) - {b"manifest.toml", b"SHA256SUMS"} - set(bin_entries)
    if len(entries) not in (2, 3) or len(bin_entries) != 1 or others:
        raise Reject(
            f"bap: 应恰 2 或 3 条目（manifest.toml + 唯一 bin/ + SHA256SUMS?），"
            f"得 {sorted(names)}"
        )
    ent = dict(entries)

    raw = ent[b"manifest.toml"]
    if len(raw) > MANIFEST_MAX:
        raise Reject(f"manifest.toml 超 MANIFEST_MAX {MANIFEST_MAX}B")
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError:
        raise Reject("manifest.toml 非 ASCII")
    m = check_manifest_text(text)  # 全量门禁复跑（含三值链/词表/规范形）
    if m.entry.encode("ascii") != bin_entries[0]:
        raise Reject(
            f"manifest entry {m.entry!r} != 包内条目 {bin_entries[0].decode('ascii', 'replace')!r}"
        )

    elf = ent[bin_entries[0]]
    if elf[:4] != b"\x7fELF":
        raise Reject(f"bap: 非 ELF（头 4 字节 {elf[:4]!r}）")

    digests = "n/a"
    if b"SHA256SUMS" in ent:
        _verify_sums(
            ent[b"SHA256SUMS"],
            [(n, d) for n, d in entries if n != b"SHA256SUMS"],
        )
        digests = "ok"
    print(f"mkapp: {path} OK {len(blob)}B entries={len(entries)} digests={digests}")


# ---- SDK-1a §4.5：--new（应用脚手架四件——不含 bin/ 内容的两步流）----

SCAFFOLD_MANIFEST = (
    "[app]\n"
    'id = "{app_id}"\n'
    'version = "0.1.0"\n'
    'entry = "bin/{slug}"\n'
    'type = "app"\n'
    'intent = "utility"\n'
    "\n[compat]\n"
    'min = "0.1.0"\n'
    'target = "0.1.0"\n'
    'compile = "0.1.0"\n'
    "\n[capabilities]\n"
    "console = true\n"
    "\n[runtime]\n"
    "stack_pages = 8\n"
)

SCAFFOLD_MAIN = """\
//! {app_id} —— BerryOS 应用（SDK-1a 脚手架；契约见 docs/应用开发指南.md）。
use std::process::ExitCode;

// 链接种子（stdhello/appdemo 同款）：无引用则 berry-libc 的 crt0（含
// `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() -> ExitCode {{
    // argv0 契约：spawn name = manifest entry（app-0 §5）。
    assert_eq!(
        std::env::args().next().as_deref(),
        Some("bin/{slug}"),
        "argv0 should be manifest entry"
    );
    println!("app {app_id} up");
    ExitCode::from(0)
}}
"""

SCAFFOLD_CARGO = """\
[package]
name = "{slug}"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
berry-libc = {{ path = "../../berry-libc" }}

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
"""

SCAFFOLD_README = """\
# {app_id} —— BerryOS 应用

脚手架由 `mkapp.py --new` 产出（SDK-1a）；契约详见 `docs/应用开发指南.md`。

## 构建（咒语参数勿动——justfile build-user 同链）

本工程须位于 `<ROOT>/user/bins/{slug}/`（Cargo.toml 的 berry-libc 相对路径
按此位置钉死 `../../berry-libc`；脚手架到别处 = 手改该 path）。

```bash
ROOT=$(cd ../../.. && pwd)   # <ROOT>/user/bins/{slug} → checkout 根
cd "$ROOT/user"
CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \\
    cargo build --release --manifest-path bins/{slug}/Cargo.toml \\
    --target aarch64-unknown-linux-musl
```

参数逐项为什么：见指南 §4（musl target + berry-ld 剥 musl crt + 4K
max-page）。产物：`user/cargo-target/aarch64-unknown-linux-musl/release/{slug}`。

## 打包四步

1. 组 staging：`mkdir -p /tmp/{slug}-stage/bin`；
2. 放件：`cp manifest.toml /tmp/{slug}-stage/` 且
   `cp <产物> /tmp/{slug}-stage/bin/`（bin/ 恰一个文件，名 = entry 去
   `bin/` 前缀；SHA256SUMS 是 mkapp 自动生成的，**不要手放**）；
3. 打包：`python3 $ROOT/tools/mkapp.py /tmp/{app_id}.bap /tmp/{slug}-stage`；
4. 复验：`python3 $ROOT/tools/mkapp.py --check /tmp/{app_id}.bap`。

## 契约三件（速览）

- **argv0**：spawn 名 = manifest `entry`（`bin/{slug}`）——main 里断言钉着；
- **EOF 先行**：退出走 `ExitCode::from(0)` 真退出（libc exit 族先发零长 EOF）；
- **退出码**：appd reap 收 0 才计入 `APP ok` 谓词；panic → abort(134) → loud。

## in-tree 接线（进 rootfs）

进 rootfs.cpio 需 justfile build-user 挂账位（SDK-1a §10 O7——脚手架刀不
接线）：构建产物 + staging 四步后，把 `{app_id}.bap` 以 `app/{app_id}.bap`
条目加进 build-user 的 mkcpio 调用（appdemo/appquiet 同款两行）。
"""


def do_new(app_dir: str, app_id: str) -> None:
    """--new：脚手架四件（§4.5）。basename(dir) == ID.lower()（id 大写
    惯例对齐 demo 双 app）；目录已存在且非空 = 拒（防覆写）。**不含
    bin/ 内容**——产物是「不可打包的源码工程」，构建 ELF 放入 bin/ 后
    才可打包（空 bin/ 会被 build 的「恰一个文件」门禁拒，两步流有意）。"""
    slug = os.path.basename(os.path.normpath(app_dir))
    if slug != app_id.lower():
        raise Reject(
            f"--new: basename({app_dir!r}) = {slug!r} != --id {app_id!r} 的小写形"
        )
    _check_str(app_id, "id", ID_MAX)
    if not ID_RE.fullmatch(app_id):
        raise Reject("manifest: id 字符集 [A-Za-z0-9._-]、首字符字母数字")
    # slug 直接成为 Cargo.toml 包名——cargo 包名词法拒 '.'（ID_RE 的宽松面
    # 是 schema 既有承诺，打包主路径不拦；--new 产出必须可构建，故此收紧）。
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]*", slug):
        raise Reject(
            f"--new: slug {slug!r} 不是合法 cargo 包名（含点号等 cargo 拒收字符；"
            "id 勿含 '.'，请改用 '-' 或 '_'）"
        )
    if os.path.isfile(app_dir):
        raise Reject(f"--new: {app_dir} 是文件")
    if os.path.isdir(app_dir) and os.listdir(app_dir):
        raise Reject(f"--new: {app_dir} 已存在且非空（防覆写）")
    os.makedirs(os.path.join(app_dir, "src"), exist_ok=True)
    files = {
        "manifest.toml": SCAFFOLD_MANIFEST.format(app_id=app_id, slug=slug),
        "src/main.rs": SCAFFOLD_MAIN.format(app_id=app_id, slug=slug),
        "Cargo.toml": SCAFFOLD_CARGO.format(slug=slug),
        "README.md": SCAFFOLD_README.format(app_id=app_id, slug=slug),
    }
    for rel, text in files.items():
        with open(os.path.join(app_dir, rel), "w", encoding="utf-8") as f:
            f.write(text)
    # 自产自证：脚手架 manifest 必须过全量门禁（防模板漂移——§4.5 尾约）
    check_manifest_text(files["manifest.toml"])
    print(f"mkapp: {app_dir} 脚手架四件就绪（{app_id}；构建步骤见 {app_dir}/README.md）")


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
    """门禁自检（§2 + SDK-1a §4.6）：负样本 = tomllib 收下而门禁必须拒
    的形态——证明第 2 道规范形比较真的在收紧 tomllib 的超集（SDK-1a 起
    负面另含三值链/词表/security 的第 1 道拒面）；正对照 = 规范形必须过
    （防空转：check 恒拒也满足「负样本全拒」）——BASE 字节零漂移 =
    add-only 执法样本，F 形 = §3.2 字节模板七形态中五款 + roundtrip。"""
    f0 = (
        "[app]\n"
        'id = "X"\n'
        'version = "0.1.0"\n'
        'entry = "bin/x"\n'
        'type = "app"\n'
        'intent = "utility"\n'
        "\n[capabilities]\nconsole = true\n"
    )
    f1 = f0.replace(
        "\n[capabilities]",
        '\n[compat]\nmin = "0.1.0"\ntarget = "0.1.0"\ncompile = "0.1.0"\n\n[capabilities]',
    )
    f2 = f0.replace("console = true\n", 'console = true\nfs = ["rootfs"]\n')
    f3 = f0.replace("console = true\n", 'console = true\nnet = ["client"]\n')
    f4 = f0 + '\n[security]\nprofile = "ed25519-main"\nkey_lineage = "root-2026"\n'
    f6 = (
        "[app]\n"
        'id = "KITCHEN"\n'
        'version = "0.1.0"\n'
        'entry = "bin/kitchen"\n'
        'type = "app"\n'
        'intent = "utility"\n'
        "\n[compat]\n"
        'min = "0.1.0"\n'
        'target = "0.1.0"\n'
        'compile = "0.1.0"\n'
        "\n[capabilities]\n"
        "console = true\n"
        'fs = ["rootfs"]\n'
        'net = ["client"]\n'
        "\n[runtime]\n"
        "stack_pages = 8\n"
        "\n[security]\n"
        'profile = "ed25519-main"\n'
        'key_lineage = "root-2026"\n'
    )
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
        # ---- SDK-1a 新增拒面（§4.6 清单）----
        ("compat-missing-key", f0 + '\n[compat]\nmin = "0.1.0"\ntarget = "0.1.0"\n'),
        # 链执法两样本 = f1（[compat] 在规范位）值变异——非 f0 尾拼节：尾拼
        # 节独立违反规范形定序，链门删了仍有门禁 2 拒 = 假绿（对抗审查实抓，
        # 词表样本同款手法：值变异在规范位上，门删 → canonical 重发射与输入
        # 逐字节恒等 → 样本被放行 → selftest FAIL，样本钉得住门）。
        ("compat-order", f1.replace('min = "0.1.0"', 'min = "0.2.0"')),
        ("compile-above-sdk", f1.replace('target = "0.1.0"', 'target = "0.2.0"').replace('compile = "0.1.0"', 'compile = "0.2.0"')),
        ("ver-leading-zero", f0 + '\n[compat]\nmin = "01.0.0"\ntarget = "0.1.0"\ncompile = "0.1.0"\n'),
        ("ver-two-segs", f0 + '\n[compat]\nmin = "1.2"\ntarget = "0.1.0"\ncompile = "0.1.0"\n'),
        ("ver-four-segs", f0 + '\n[compat]\nmin = "1.2.3.4"\ntarget = "0.1.0"\ncompile = "0.1.0"\n'),
        ("fs-vocab-tmpfs", f2.replace('["rootfs"]', '["tmpfs"]')),
        ("fs-vocab-appdata", f2.replace('["rootfs"]', '["$appdata"]')),
        ("fs-not-array", f0.replace("console = true\n", 'console = true\nfs = "rootfs"\n')),
        ("net-vocab-server", f3.replace('["client"]', '["server"]')),
        ("sec-missing-key", f4.replace('\nkey_lineage = "root-2026"\n', "\n")),
        ("sec-value-space", f0 + '\n[security]\nprofile = "a b"\nkey_lineage = "x"\n'),
        ("sec-value-17b", f0 + '\n[security]\nprofile = "' + "a" * 17 + '"\nkey_lineage = "x"\n'),
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
        ("canonical", BASE),  # v0 旧形字节零漂移锚（add-only 执法样本）
        ("canonical-runtime", BASE + "\n[runtime]\nstack_pages = 8\n"),
        ("f1-compat", f1),
        ("f2-fs", f2),
        ("f3-net", f3),
        ("f4-security", f4),
        ("f6-kitchen-sink", f6),
    ]
    for label, text in pos:
        try:
            m = check_manifest_text(text)
            print(f"  selftest 过（预期） : {label} id={m.id}")
        except Reject as e:
            print(f"  selftest FAIL: {label} 被门禁误拒： {e}")
            bad += 1
    # roundtrip：--new → 伪 ELF 入 bin/ → build → --check 全链（§4.5 尾约）
    # + 旧形 2 条目 .bap 过 --check（SDK-1a 前历史形状兼容）。
    import tempfile

    with tempfile.TemporaryDirectory() as td:
        try:
            app_dir = os.path.join(td, "myapp")
            do_new(app_dir, "MYAPP")
            # staging 组装（指南 §5.1 四步——scaffold 目录本身含 src/Cargo/
            # README，不是 staging；build 只收恰 {manifest.toml, bin/}）
            stage = os.path.join(td, "myapp-stage")
            os.makedirs(os.path.join(stage, "bin"))
            with open(os.path.join(app_dir, "manifest.toml"), "r") as f:
                mf_text = f.read()
            with open(os.path.join(stage, "manifest.toml"), "w") as f:
                f.write(mf_text)
            with open(os.path.join(stage, "bin", "myapp"), "wb") as f:
                f.write(b"\x7fELF")  # 4 字节伪桩（门禁 5 只看魔数）
            out = os.path.join(td, "MYAPP.bap")
            build(out, stage)
            check_bap(out)
            print("  selftest 过（预期） : new-build-check-roundtrip id=MYAPP")
        except (Reject, OSError) as e:
            print(f"  selftest FAIL: new-build-check-roundtrip： {e}")
            bad += 1
        try:
            legacy = build_archive(
                sorted(
                    [
                        (b"manifest.toml", BASE.encode("ascii")),
                        (b"bin/appdemo", b"\x7fELF-stub"),
                    ]
                )
            )
            legacy_path = os.path.join(td, "LEGACY.bap")
            with open(legacy_path, "wb") as f:
                f.write(legacy)
            check_bap(legacy_path)  # digests=n/a 路径（2 条目历史包）
            print("  selftest 过（预期） : legacy-2entry-check digests=n/a")
        except (Reject, OSError) as e:
            print(f"  selftest FAIL: legacy-2entry-check： {e}")
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
    if len(args) == 2 and args[0] == "--check":
        try:
            check_bap(args[1])
        except Reject as e:
            sys.stderr.write(f"mkapp: {e}\n")
            return 1
        return 0
    if len(args) == 4 and args[0] == "--new" and args[2] == "--id":
        try:
            do_new(args[1], args[3])
        except Reject as e:
            sys.stderr.write(f"mkapp: {e}\n")
            return 1
        return 0
    if len(args) == 2 and not args[0].startswith("-"):
        try:
            build(args[0], args[1])
        except Reject as e:
            sys.stderr.write(f"mkapp: {e}\n")
            return 1
        return 0
    sys.stderr.write(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
