#!/usr/bin/env python3
r"""mkapp——.bap 应用包打包器（app-0，`docs/app-0实施设计草案.md` §4；
SDK-1a 增量 `docs/SDK-1a实施设计草案.md` §4；GUI-7 增量见本头注
「GUI-7」段与「门禁」第 1 道；ADR #43/#44：应用 = 纯用户态协议约定，
包 = cpio(newc) 容器，内核零概念）。

用法：mkapp.py <out.bap> <app-dir>        # 打包（app-dir = {manifest.toml, bin/, icons/?, views/?}）
      mkapp.py --check <path.bap>         # 复验既有包，只验不写（SDK-1a §4.4）
      mkapp.py --json <out.bap> <app-dir> # 同打包·机读单行 JSON 信封（SDK-2 §二；
                                          #   亦作 --check/--new 前缀旗标，拒因 exit 1）
      mkapp.py --new <dir> --id <ID>      # 应用脚手架四件（SDK-1a §4.5）
      mkapp.py --selftest                 # 门禁自检（零依赖；挂 just test-host，
                                          #          mkfat --check-fixture 同位）

签名面（P1 集成刀，SDK-2 分发签名链——24 号 §4.3 半刀②③；可选旗标，
无旗标时行为与旧版逐字节同——向后兼容）：
      mkapp.py --sign-key <sk> <out.bap> <app-dir>
                                          # 打包后自动调 berry-sign sign 产
                                          #   <out>.bap.sig（.sig 信封 v1；
                                          #   manifest 须含 [security] 三键——
                                          #   key_lineage 是信封回声源）
      mkapp.py --verify-key <pk> --check <path.bap>
                                          # check 后验既有 <path>.bap.sig
                                          #   （.sig 缺失 = 拒——防剥离护栏）
  berry-sign 二进制发现序：env BERRY_SIGN → 构建产物候选
  （<repo>/target-cargo 与 /tmp/target-p1 下 aarch64-apple-darwin/
  {debug,release}/berry-sign）。验收线四件（正样例 + 篡改/断链/换钥
  三负样例）主归属 = `berry-sign selftest`（宿主最短路径，零依赖纪律
  不破——mkapp selftest 保持纯 Python 门禁）。

包格式（app-0 §1 冻结 + SDK-1a §2.4 增 + GUI-7 扩）：cpio newc 条目 =
`manifest.toml` + 唯一 `bin/` 前缀条目（ELF 本体，条目名 = manifest entry）
+ `SHA256SUMS`（SDK-1a 起产包恒在；`--check` 对 2 条目历史包同样可验）
+ `icons/<名>.byi` × 0..ICON_FILES_MAX + `views/<名>` × 0..VIEWS_FILES_MAX
（GUI-7）。SHA256SUMS 内容 = 每条 payload 一行「64 位小写 hex + 双空格 +
条目名」（GNU sha256sum 文本惯例）、LF 收尾、行序 = 条目名字节序；入包
排序天然 `SHA256SUMS`(0x53) < `bin/…`(0x62) < `icons/…`(0x69) <
`manifest.toml`(0x6d) < `views/…`(0x76)。进 rootfs.cpio 时整体作为单条目
`app/<ID>.bap`（'a' < 'b'，排序天然在 bin/ 之前）。

GUI-7 图标/视图管道（裁决 B——staging 源是 PNG，产物是 BYI1）：
  - staging `icons/` 恰收 `.png` 源（扁平、常规文件、≤ICON_FILES_MAX 个），
    mkapp 逐个编码为 `icons/<stem>.byi`（BYI1 格式，宿主侧唯一生产者 =
    本文件 `png_to_byi`）后入包——**源不入包、产物不落 staging**；
  - staging `views/` 文件原样入包（berryui DSL 定形于 SDK-2，v1 opaque，
    mkapp 只验名形/帽，不解释内容）；
  - 交叉门（build 与 --check 双跑，`_ui_cross_check` 单源）：manifest 未
    声明 ui.icon ⇒ 包内 icons/ 条目须为零；声明 ⇒ 所指条目必须在场
    （额外条目 ≤ 帽 tolerated——@2x 备用图标等）；views 对称同律；
  - BYI1 产面帽 = 单边 ≤ICON_DIM_MAX 且 w*h ≤ICON_PIXELS_MAX——比
    berry-gfx decode 面严（decode 无单边帽）：**产面恒为解码面子集**；
    build 编码后即 `_byi_validate` 自验，`--check` 对包内 .byi 全量重走
    （同一走查器双跑）。

门禁（任一失败 = 非零退 + 一行理由，loud not silent）：
  1. manifest 可解析（tomllib）且结构/值域过检（镜像 berry-manifest）+
     SDK-1a 三值链执法 min ≤ target ≤ compile ≤ SDK_CURRENT（hvigor
     00303015 对应物——「不能面向未来 SDK 编译」在打包期炸响）+
     GUI-7 [ui] 三键独立可选值域（icon = `icons/` 前缀 `.byi` 后缀产品名、
     views = `views/` 前缀路径、label = 1..=LABEL_MAX——帽/前缀镜像
     berry-manifest，`.byi` 后缀是 mkapp 产面收紧项）+ 同意面先落件二
     [agent-actions]（键 = kebab-case 动作名、值 = 三档词
     routine|sensitive|destructive——镜像 berry-manifest RiskTier）+
     ADR #82 V3 [security] 三键齐备（profile/key_lineage/integrator——
     值域 SEC_RE 同域，「两键齐备或齐缺」不变量三键化重立法）+
     V6 [blob] 载体节声明校验（在场则五字段 license/origin/sha256/
     size/usage 必全——缺一 loud 拒，镜像 berry-manifest；值只校验
     形态，内容对拍随 berry-assemble/V13）；
  2. **规范形重序列化逐字节相等**——berry-manifest 只收 TOML 子集，
     tomllib 收全集（hex/下划线整数、单引号、内联表、多行串、注释、
     CRLF…），本道是把「tomllib 收下」收紧到「子集收下」的唯一执法点；
  3. entry == 包内 bin/ 唯一文件的完整路径（`bin/<名>`，无双重前缀）；
  4. 输出文件名 == `<id>.bap`；
  5. bin 文件头 4 字节 == \x7fELF。
另：app-dir 内容恰 {manifest.toml, bin/, icons/?, views/?}，bin/ 恰一个
常规文件，icons/ 恰 `.png` 常规文件（扁平），views/ 恰非空常规文件；
manifest ≤ MANIFEST_MAX、归档 ≤ BAP_MAX。

--json 信封（SDK-2 §二 拒因三元组双读，2026-11-08 刀7；不带旗标时人读
输出/退出码逐字节同现状）：恒 argv[1] 前缀（`--json <out.bap> <app-dir>` |
`--json --check <path.bap>` | `--json --new <dir> --id <ID>`；--selftest 不收
--json）。成功 {"tool":"mkapp","op":"build|check|new","ok":true,"result":
{…摘要行镜像…}}；拒因 ok=false + error 舱 {code,where,line,expected,got,
fix,message}——code=域.键.因 点分小写（负样例八类见 selftest --json 腿）；
where=节.键路径，line 仅语法层透传 tomllib 位点、其余域用自带位点（cpio
偏移/文件名）；--json 模式 OSError 兜底 code="io"；用法错双模式同形
（stderr __doc__ + exit 2）。

规范形（第 2 道的比较基准，SDK-1a §3 冻结 + GUI-7 尾增 + 同意面先落件二
尾增）：`[app]` 五行定序 id/version/entry/type/intent + `[compat]?`
（min/target/compile，int 三元组重发射；VER-01 提案第 4 条起可选尾增
`min_level` u32 整数行——未声明不发射）+ `[capabilities]`
console,fs?,net?（fs/net 数组无空格形）+ `[runtime]?` stack_pages +
`[security]?`（profile/key_lineage/integrator——ADR #82 V3 三键）+
`[ui]?`（GUI-7——icon/label/views 键定序、三键独立可选）+
`[agent-actions]?`（同意面先落件二——kebab-case 键按文档序保序重发射）+
`[blob]?`（ADR #82 V6 前置——五字段键定序 license/origin/sha256/size/
usage，居末）；节间恰一空行。旧形（五平面全缺省）输出与 v0 canonical()
逐字节相同。空 `[runtime]` / 空 `[ui]` / 空 `[agent-actions]` / 空
`[blob]` 解析器合法但非规范形 → 门禁拒（byte 不等）——省略或显式，
二选一。

编码器纪律（§4 单编码器）：cpio 编码 `from mkcpio import build_archive`
复用——禁止第二份 newc/trailer 逻辑（双编码器 = 漂移面）；条目按
name 字节排序（确定性：同输入同字节，AI 测试友好化）。`--check` 的走查
器 `_cpio_entries` 是宿主侧唯一解码器（mkcpio 只产不收、svcmgr 侧在
guest 内——对读 mkcpio.entry 编码几何，非第二编码器）。PNG→BYI1 同律：
`png_to_byi` 是宿主侧唯一 BYI1 生产者（berry-gfx decode 在 crate/guest
侧——wire 格式对读，非第二编码器）。

常量对账：以下帽/词表镜像 `user/berry-manifest/src/lib.rs` 公开常量
（mkfat ↔ berry-fatfs 同款义务）——改 berry-manifest 常量须同步此处；
SDK-1a 起 SDK_CURRENT/FS_SCOPES/NET_SCOPES/VER_RE/SEC_RE 同款义务
（SDK 级单一事实源 = berry-manifest，此处镜像）。GUI-7 起 UI_PATH_MAX/
LABEL_MAX/ICON_PREFIX/VIEWS_PREFIX/ICON_FILES_MAX/VIEWS_FILES_MAX
同款义务；ICON_DIM_MAX/ICON_PIXELS_MAX 对齐 berry-gfx decode 面
（产面子集执法）。抬级流程 = 改 berry-manifest 两常量 + 此处镜像 +
`docs/应用开发指南.md`。
"""
import hashlib
import json
import os
import re
import struct
import subprocess
import sys
import zlib

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

# ---- GUI-7 常量（对账锚点：UI_PATH_MAX/LABEL_MAX/两前缀/两条目帽镜像
# berry-manifest；DIM/PIXELS 对齐 berry-gfx decode 面——产面子集执法）----
UI_PATH_MAX = 32
LABEL_MAX = 32
ICON_PREFIX = "icons/"
VIEWS_PREFIX = "views/"
ICON_FILES_MAX = 4      # icons/ 条目帽（含声明条目；mkapp ↔ appd 双侧镜像）
VIEWS_FILES_MAX = 4     # views/ 条目帽（同上）
ICON_DIM_MAX = 1024     # 单边帽（decode 面无单边帽，产面收紧）
ICON_PIXELS_MAX = 65536  # w*h 帽（= berry-gfx decode 同值；RLE count 是 u16——超 65535 的 run 编码期拆分）

# ---- 同意面先落件二常量（docs/AI同意面设计-v1.0.md §5；对账锚点：
# ACTION_NAME_MAX/AGENT_ACTIONS_MAX/RISK_TIERS 镜像 berry-manifest
# RiskTier 词表与帽——双侧同步义务同上）----
ACTION_NAME_MAX = 24    # 动作名帽（kebab-case：首字符小写字母、[a-z0-9-]）
AGENT_ACTIONS_MAX = 16  # 条目数帽（berry-manifest 零分配定长槽的镜像）
RISK_TIERS = ("routine", "sensitive", "destructive")
ACTION_KEY_RE = re.compile(r"[a-z][a-z0-9-]{0,23}")

# ---- ADR #82 厂商定制预埋常量（V3/V6；对账锚点：SEC_KEYS 三键集与
# BLOB_FIELD_MAX/BLOB_KEYS 五字段集镜像 berry-manifest——双侧同步义务
# 同上）----
SEC_KEYS = ("profile", "key_lineage", "integrator")  # V3：三键齐备或齐缺
BLOB_FIELD_MAX = 64     # [blob] 单字段帽（V6——形态校验 only，值执法随 berry-assemble/V13）
BLOB_KEYS = ("license", "origin", "sha256", "size", "usage")  # V6：最低披露五字段（09 号 §4 族）

ID_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*")
APP_KEYS = ("id", "version", "entry", "type", "intent")

# SHA256SUMS 行形：64 位小写 hex + 恰两空格 + 条目名（GNU sha256sum 文本惯例）
SUMS_LINE_RE = re.compile(rb"^([0-9a-f]{64})  (.+)$")


class Reject(Exception):
    """门禁拒绝（一行理由，非零退出）。

    --json 模式附结构化拒因（SDK-2 §二 三元组扩展面，人读文案零改）：
    code=域.键.因 点分小写稳定名；where=节.键路径；line 仅语法层透传
    tomllib 位点（其余域 None——非 manifest 域用自带位点进 where/got）；
    expected/got/fix 供消费端（含 agent）按舱分支渲染。全部缺省 None：
    旧调用点零 kwargs 兼容。"""

    def __init__(self, message: str, *, code=None, where=None, line=None,
                 expected=None, got=None, fix=None):
        super().__init__(message)
        self.code = code
        self.where = where
        self.line = line
        self.expected = expected
        self.got = got
        self.fix = fix


def _ver(v: tuple) -> str:
    """int 三元组 → `x.y.z`（canonical 重发射/诊断行用——文法禁前导零 ⇒
    「收下 → 重发射」恒等归一，§3.1 要点①）。"""
    return ".".join(str(x) for x in v)


class M:
    """过检后的 manifest 视图（规范形重序列化的数据源）。"""

    def __init__(self, app, caps, runtime, compat, min_level, sec, ui, aa, blob):
        self.id = app["id"]
        self.version = app["version"]
        self.entry = app["entry"]
        self.console = caps["console"]
        self.has_runtime = runtime is not None
        self.stack_pages = runtime["stack_pages"] if runtime else 8
        self.intent = app["intent"]
        # SDK-1a（§4.1）：compat = None | (min, target, compile) int 三元组
        # 的三元组；fs/net = None | str（词表门已保证单元素数组——此处取
        # 元素，canonical 按 `["str"]` 无空格形重发射）；sec = None |
        # (p, k, i)（ADR #82 V3 起三键）。
        self.compat = compat
        # VER-01 提案第 4 条：min_level = None | u32（[compat] 独立可选、
        # 缺省 0——None = 未声明不发射；appd 装载期与 BERRY_PLATFORM_LEVEL 比）。
        self.min_level = min_level
        fs_val = caps.get("fs")
        self.fs = fs_val[0] if fs_val else None
        net_val = caps.get("net")
        self.net = net_val[0] if net_val else None
        self.sec = sec
        # GUI-7：ui = None | (icon, label, views)——三槽各自 None | str
        # （独立可选；空 [ui] 节在门禁 2 规范形比较处拒，到不了这里非空）。
        self.ui = ui
        # 同意面先落件二：aa = None | [(动作名, 档词), …]（文档序——tomllib
        # dict 保序；空 [agent-actions] 节同理在门禁 2 处拒）。
        self.agent_actions = aa
        # ADR #82 V6 前置：blob = None | (license, origin, sha256, size,
        # usage) 五元组（BLOB_KEYS 定序；空节同理在门禁 2 处拒）。
        self.blob = blob

    def canonical(self) -> str:
        # SDK-1a §3.1 定序 + GUI-7/同意面先落件二尾增：[app] → [compat]?
        # → [capabilities] → [runtime]? → [security]? → [ui]? →
        # [agent-actions]?；节间恰一空行；旧形（全缺省）输出与 v0 逐字节
        # 同（尾增居末 = 既有形态字节零漂移）。
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
            # VER-01 提案第 4 条：min_level 尾增（未声明不发射——旧形字节零漂移）
            if self.min_level is not None:
                out += f"min_level = {self.min_level}\n"
        out += f"\n[capabilities]\nconsole = {'true' if self.console else 'false'}\n"
        if self.fs:
            out += f'fs = ["{self.fs}"]\n'
        if self.net:
            out += f'net = ["{self.net}"]\n'
        if self.has_runtime:
            out += f"\n[runtime]\nstack_pages = {self.stack_pages}\n"
        if self.sec:
            p, k, g = self.sec
            out += f'\n[security]\nprofile = "{p}"\nkey_lineage = "{k}"\nintegrator = "{g}"\n'
        if self.ui:
            icon, label, views = self.ui
            out += "\n[ui]\n"
            if icon:
                out += f'icon = "{icon}"\n'
            if label:
                out += f'label = "{label}"\n'
            if views:
                out += f'views = "{views}"\n'
        # 同意面先落件二居末（F0–F7 字节零漂移）；键按文档序保序重发射。
        if self.agent_actions:
            out += "\n[agent-actions]\n"
            out += "".join(f'{k} = "{v}"\n' for k, v in self.agent_actions)
        # ADR #82 V6：[blob] 尾增居末（F0–F8 字节零漂移）；五字段键定序
        # license/origin/sha256/size/usage（BLOB_KEYS 单源）。
        if self.blob:
            lic, org, sha, sz, usg = self.blob
            out += (
                f'\n[blob]\nlicense = "{lic}"\norigin = "{org}"\n'
                f'sha256 = "{sha}"\nsize = "{sz}"\nusage = "{usg}"\n'
            )
        return out


def _check_str(v, what: str, maxlen: int, key: str = "") -> None:
    # parse_string 镜像：可打印 ASCII 0x20..=0x7E、无反斜杠、无内嵌引号；
    # key = --json 拒因尾段（code=manifest.<key>.type/.length/.charset、
    # where=<key>）；缺省空串 = 旧点兼容（现役调用点全传）。
    if not isinstance(v, str):
        raise Reject(f"manifest: {what} 应为字符串",
                     code=f"manifest.{key}.type", where=key)
    if not 1 <= len(v) <= maxlen:
        raise Reject(f"manifest: {what} 空/超长（1..={maxlen}）",
                     code=f"manifest.{key}.length", where=key,
                     expected=f"1..={maxlen}", got=len(v))
    for ch in v:
        if not (0x20 <= ord(ch) <= 0x7E) or ch in ('\\', '"'):
            raise Reject(f"manifest: {what} 非法字符 {ch!r}",
                         code=f"manifest.{key}.charset", where=key)


def _toml_line(e: Exception):
    # tomllib 报错文案含「at line N」——透传给 --json error.line（仅语法层）
    mo = re.search(r"at line (\d+)", str(e))
    return int(mo.group(1)) if mo else None


def check_manifest_text(text: str) -> M:
    """manifest 文本 → 过检视图；门禁第 1+2 道（结构/值域 + 三值链 + 规范形）。"""
    # 字节面前置（berry-manifest 同款：LF only、拒 tab——tomllib 对两者
    # 都比我们宽容，先拒免得把锅甩给规范形比较）
    if "\r" in text:
        raise Reject("manifest: CR（CRLF）拒收——LF only",
                     code="manifest.syntax.crlf", where="manifest.toml",
                     fix="编辑器行尾改存 LF")
    if "\t" in text:
        raise Reject("manifest: tab 拒收",
                     code="manifest.syntax.tab", where="manifest.toml",
                     fix="tab 改空格/删除")
    try:
        doc = tomllib.loads(text)
    except tomllib.TOMLDecodeError as e:
        raise Reject(f"manifest: TOML 解析失败： {e}",
                     code="manifest.syntax.toml", where="manifest.toml",
                     line=_toml_line(e))

    unknown = set(doc) - {
        "app", "capabilities", "runtime", "compat", "security", "ui", "agent-actions", "blob",
    }
    if unknown:
        raise Reject(f"manifest: 未知节 {sorted(unknown)}",
                     code="manifest.section.unknown", where="manifest.toml",
                     expected=["app", "capabilities", "runtime", "compat",
                               "security", "ui", "agent-actions", "blob"],
                     got=sorted(unknown))
    for need in ("app", "capabilities"):
        if not isinstance(doc.get(need), dict):
            raise Reject(f"manifest: 缺 [{need}]",
                         code="manifest.section.missing", where=need,
                         expected=f"[{need}] 节")

    app = doc["app"]
    if set(app) != set(APP_KEYS):
        raise Reject(f"manifest: [app] 键集应为五键 {sorted(APP_KEYS)}",
                     code="manifest.app.keys", where="app",
                     expected=sorted(APP_KEYS), got=sorted(app))
    _check_str(app["id"], "id", ID_MAX, key="app.id")
    if not ID_RE.fullmatch(app["id"]):
        raise Reject("manifest: id 字符集 [A-Za-z0-9._-]、首字符字母数字",
                     code="manifest.id.charset", where="app.id",
                     expected="[A-Za-z0-9._-]、首字符字母数字")
    _check_str(app["version"], "version", VERSION_MAX, key="app.version")
    _check_str(app["entry"], "entry", ENTRY_MAX, key="app.entry")
    if not app["entry"].startswith("bin/") or len(app["entry"]) <= len("bin/"):
        raise Reject("manifest: entry 应为 bin/ 前缀完整路径",
                     code="manifest.entry.prefix", where="app.entry",
                     expected="bin/<name>")
    if app["type"] != "app":
        # ADR #82 V1：**产面不同步**——berry-manifest 词表已收 "driver"
        # （收格式不消费，svcmgr 收集槽 type!=app 过滤），但 [hw] 节立法
        # （驱动模型 v2 刀 B）前无合法 driver 产出路径，本门维持仅产
        # "app"（11 号 §2-V1/W4）。
        raise Reject('manifest: type 仅 "app"',
                     code="manifest.type.value", where="app.type",
                     expected="app", got=app["type"])
    if app["intent"] not in INTENTS:
        raise Reject(f"manifest: intent 应为六词之一 {INTENTS}",
                     code="manifest.intent.vocab", where="app.intent",
                     expected=list(INTENTS), got=app["intent"])

    # SDK-1a：[compat] 三值域 + 链执法（§4.2）+ VER-01 提案第 4 条可选
    # min_level（u32 整数、独立可选、缺省 0——不参三值齐备约束）。
    # isinstance 门 = 节级形态硬ening（tomllib 把 `compat = 5` 解成 int，
    # set(int) 会 TypeError——崩栈不是门禁，与 [app]/[capabilities] 的
    # isinstance 门同口径）。
    vs = None
    compat = doc.get("compat")
    ml = None
    if compat is not None:
        if (not isinstance(compat, dict)
                or not {"min", "target", "compile"} <= set(compat)
                or set(compat) - {"min", "target", "compile", "min_level"}):
            raise Reject("manifest: [compat] 应为 min/target/compile 三键齐备（+可选 min_level）",
                         code="manifest.compat.keys", where="compat",
                         expected=["min", "target", "compile", "min_level?"])
        vs = []
        for k in ("min", "target", "compile"):
            v = compat[k]
            if not isinstance(v, str) or not VER_RE.fullmatch(v):
                raise Reject("manifest: compat 值应为 x.y.z（每段 1-3 位十进制、无前导零）",
                             code="manifest.compat.value", where=f"compat.{k}",
                             expected="x.y.z")
            vs.append(tuple(int(g) for g in VER_RE.fullmatch(v).groups()))
        if not vs[0] <= vs[1] <= vs[2]:
            raise Reject("manifest: compat 序不合（需 min ≤ target ≤ compile）",  # hvigor 00303015 对应物
                         code="manifest.compat.order", where="compat",
                         expected="min ≤ target ≤ compile",
                         got=f"min={_ver(vs[0])} target={_ver(vs[1])} "
                             f"compile={_ver(vs[2])}")
        if vs[2] > SDK_CURRENT:
            raise Reject(f"manifest: compile {_ver(vs[2])} 超工具链当前级 {SDK_CURRENT_STR}",
                         code="manifest.compat.ceiling", where="compat.compile",
                         expected=f"≤{SDK_CURRENT_STR}", got=_ver(vs[2]))
        # VER-01 提案第 4 条：min_level（u32 整数；装载期检查在 appd 与
        # BERRY_PLATFORM_LEVEL 比——打包期不做「未来级」闸，只钉形状）。
        ml = compat.get("min_level")
        if ml is not None:
            if type(ml) is not int:  # bool 是 int 子类，type() 才拦得住
                raise Reject("manifest: min_level 应为整数",
                             code="manifest.compat.min_level.type",
                             where="compat.min_level", expected="int")
            if not 0 <= ml <= 0xFFFFFFFF:
                raise Reject("manifest: min_level 越界 [0,4294967295]",
                             code="manifest.compat.min_level.range",
                             where="compat.min_level",
                             expected="[0,4294967295]", got=ml)

    caps = doc["capabilities"]
    if not {"console"} <= set(caps) or set(caps) - {"console", "fs", "net"}:
        raise Reject("manifest: [capabilities] 键集应为 console(+fs/net)",
                     code="manifest.capabilities.keys", where="capabilities",
                     expected="console（+fs/net）")
    if type(caps["console"]) is not bool:
        raise Reject("manifest: console 应为 bool",
                     code="manifest.capabilities.console",
                     where="capabilities.console", expected="bool")
    for key, vocab in (("fs", FS_SCOPES), ("net", NET_SCOPES)):
        if key in caps:
            v = caps[key]
            if not (isinstance(v, list) and len(v) == 1 and isinstance(v[0], str)
                    and v[0] in vocab):
                raise Reject(f"manifest: {key} 应为单元素数组，词表 {vocab}",
                             code=f"manifest.{key}.vocab",
                             where=f"capabilities.{key}",
                             expected=list(vocab))

    runtime = doc.get("runtime")
    if runtime is not None:
        if not isinstance(runtime, dict) or set(runtime) != {"stack_pages"}:
            raise Reject("manifest: [runtime] 仅 stack_pages 一键",
                         code="manifest.runtime.keys", where="runtime",
                         expected=["stack_pages"])
        sp = runtime["stack_pages"]
        if type(sp) is not int:  # bool 是 int 子类，type() 才拦得住
            raise Reject("manifest: stack_pages 应为整数",
                         code="manifest.runtime.type",
                         where="runtime.stack_pages", expected="int")
        if not STACK_PAGES_MIN <= sp <= STACK_PAGES_MAX:
            raise Reject(
                f"manifest: stack_pages 越界"
                f" [{STACK_PAGES_MIN},{STACK_PAGES_MAX}]",
                code="manifest.runtime.range", where="runtime.stack_pages",
                expected=f"[{STACK_PAGES_MIN},{STACK_PAGES_MAX}]", got=sp,
            )

    # SDK-1a：[security] 预留节形态执法（值域 1..=16B [a-z0-9-]）；ADR #82
    # V3 起三键齐备或齐缺（integrator = 系统集成者身份 lineage，消费随
    # #77——SEC_RE 同域，berry-manifest set_sec 同臂镜像）。
    sec_t = None
    sec = doc.get("security")
    if sec is not None:
        if not isinstance(sec, dict) or set(sec) != set(SEC_KEYS):
            raise Reject("manifest: [security] 应为 profile/key_lineage/integrator 三键齐备",
                         code="manifest.security.keys", where="security",
                         expected=list(SEC_KEYS))
        for k in SEC_KEYS:
            if not isinstance(sec[k], str) or not SEC_RE.fullmatch(sec[k]):
                raise Reject("manifest: security 值应为 1..16B 的 [a-z0-9-]",
                             code="manifest.security.vocab",
                             where=f"security.{k}", expected="1..16B [a-z0-9-]")
        sec_t = (sec["profile"], sec["key_lineage"], sec["integrator"])

    # GUI-7：[ui] 三键独立可选（无齐备约束——空节解析合法，但 canonical
    # 不发射空节 → 门禁 2 byte 不等拒；「省略或显式」同 [runtime] 先例）。
    # 值域镜像 berry-manifest set_ui_path/label 臂 + 产面收紧 .byi 后缀
    # （staging 源名是 .png，产品名是 .byi——防声明确指了不存在的产物）。
    ui_t = None
    ui = doc.get("ui")
    if ui is not None:
        if not isinstance(ui, dict) or set(ui) - {"icon", "label", "views"}:
            raise Reject("manifest: [ui] 键集应为 icon/label/views（独立可选）",
                         code="manifest.ui.keys", where="ui",
                         expected=["icon", "label", "views"])
        icon_v = ui.get("icon")
        if icon_v is not None:
            _check_str(icon_v, "ui.icon", UI_PATH_MAX, key="ui.icon")
            if (
                not icon_v.startswith(ICON_PREFIX)
                or len(icon_v) <= len(ICON_PREFIX)
                or not icon_v.endswith(".byi")
            ):
                raise Reject(
                    f"manifest: ui.icon 应为 {ICON_PREFIX}前缀 + `.byi` 后缀产品名",
                    code="manifest.ui.icon", where="ui.icon",
                    expected=f"{ICON_PREFIX}<名>.byi",
                )
        label_v = ui.get("label")
        if label_v is not None:
            _check_str(label_v, "ui.label", LABEL_MAX, key="ui.label")
        views_v = ui.get("views")
        if views_v is not None:
            _check_str(views_v, "ui.views", UI_PATH_MAX, key="ui.views")
            if not views_v.startswith(VIEWS_PREFIX) or len(views_v) <= len(VIEWS_PREFIX):
                raise Reject(f"manifest: ui.views 应为 {VIEWS_PREFIX}前缀路径",
                             code="manifest.ui.views", where="ui.views",
                             expected=f"{VIEWS_PREFIX}<名>")
        if icon_v or label_v or views_v:
            ui_t = (icon_v, label_v, views_v)

    # 同意面先落件二（docs/AI同意面设计-v1.0.md §5）：[agent-actions] 动作
    # 风险档声明——键 = kebab-case 动作名（首字符小写字母、[a-z0-9-]、
    # ≤ACTION_NAME_MAX），值 = 三档词闭集（镜像 berry-manifest RiskTier
    # 词表）；v1 收格式不消费。空节 = canonical 不发射空节 → 门禁 2 byte
    # 不等拒（空 [ui] 同款「省略或显式」二选一）。
    aa_t = None
    aa = doc.get("agent-actions")
    if aa is not None:
        if not isinstance(aa, dict):
            raise Reject("manifest: [agent-actions] 应为节（键值表）",
                         code="manifest.agent-actions.type", where="agent-actions",
                         expected="节（键值表）")
        if len(aa) > AGENT_ACTIONS_MAX:
            raise Reject(f"manifest: [agent-actions] 条目越帽 {AGENT_ACTIONS_MAX}",
                         code="manifest.agent-actions.cap", where="agent-actions",
                         expected=f"≤{AGENT_ACTIONS_MAX}", got=len(aa))
        for k, v in aa.items():
            if not ACTION_KEY_RE.fullmatch(k):
                raise Reject(
                    "manifest: [agent-actions] 键应为 kebab-case 动作名"
                    f"（首字符小写字母、[a-z0-9-]、≤{ACTION_NAME_MAX}B）",
                    code="manifest.agent-actions.key", where="agent-actions",
                    expected="kebab-case（首字符小写字母、[a-z0-9-]）", got=k,
                )
            if v not in RISK_TIERS:
                raise Reject(f"manifest: [agent-actions] 值应为三档之一 {RISK_TIERS}",
                             code="manifest.agent-actions.tier",
                             where=f"agent-actions.{k}",
                             expected=list(RISK_TIERS), got=v)
        # 文档序保序（tomllib dict 保序）——canonical 同序重发射
        aa_t = list(aa.items())

    # ADR #82 V6 前置（声明校验半）：[blob] 载体节在场则五字段必全（缺一
    # loud 拒——声明完整性）；值只校验形态（1..=BLOB_FIELD_MAX 可打印
    # ASCII 串，_check_str 镜像 parse_string），内容对拍随 berry-assemble
    # （V13）——provenance 只在组装层存在（11 号 §2-V6）。空节 = canonical
    # 不发射空节 → 门禁 2 byte 不等拒（空 [ui] 同款「省略或显式」二选一）。
    blob_t = None
    bl = doc.get("blob")
    if bl is not None:
        if not isinstance(bl, dict):
            raise Reject("manifest: [blob] 应为节（键值表）",
                         code="manifest.blob.type", where="blob",
                         expected="节（键值表）")
        if set(bl) != set(BLOB_KEYS):
            raise Reject(f"manifest: [blob] 应为五字段齐备 {list(BLOB_KEYS)}"
                         "（缺一拒——声明完整性）",
                         code="manifest.blob.keys", where="blob",
                         expected=list(BLOB_KEYS),
                         got=(sorted(set(bl) - set(BLOB_KEYS))
                              or sorted(set(BLOB_KEYS) - set(bl))))
        for k in BLOB_KEYS:
            _check_str(bl[k], f"blob.{k}", BLOB_FIELD_MAX, key=f"blob.{k}")
        blob_t = tuple(bl[k] for k in BLOB_KEYS)

    m = M(app, caps, runtime, tuple(vs) if vs is not None else None, ml, sec_t, ui_t, aa_t, blob_t)
    if text != m.canonical():
        raise Reject(
            "manifest: 非规范形（hex/下划线整数、单引号、内联表、"
            "多行串、注释、多余空白等 tomllib 收而门禁不收）",
            code="manifest.canonical", where="manifest.toml",
            fix="以门禁规范形重发射（canonical 键序/定序/双引号基础串/LF/无注释）",
        )
    return m


# ---- GUI-7：PNG → BYI1 编码器（宿主侧唯一 BYI1 生产者）----
# BYI1 wire 格式（对读 user/berry-gfx/src/icon.rs decode，权威在 crate 侧）：
# magic "BYI1" + version u8=1 + flags u8=0 + width LE u16 + height LE u16 +
# RLE 段 ×N（count LE u16 ≥1 + pixel LE u32）；像素 = (a<<24)|(r<<16)|(g<<8)|b。
# 约束：Σcount == w*h；count ≤ 65535（超长 run 编码期拆分——256×256 纯色
# = [65535, 1] 两段）；产面另加单边 ≤ ICON_DIM_MAX（decode 面无此帽）。


def _png_chunks(png: bytes, name: str = "icon") -> list[tuple[bytes, bytes]]:
    """PNG 走查：签名 + 逐 chunk CRC 复验（敌意/损坏文件 loud）。"""
    if png[:8] != b"\x89PNG\r\n\x1a\n":
        raise Reject("icon: 非 PNG（签名不符）",
                     code="icon.png.signature", where=name, expected="PNG 签名")
    out = []
    off = 8
    while off < len(png):
        if off + 8 > len(png):
            raise Reject("icon: PNG chunk 头越界",
                         code="icon.png.chunk", where=name, got=f"@{off}")
        (length,) = struct.unpack(">I", png[off : off + 4])
        tag = png[off + 4 : off + 8]
        if off + 12 + length > len(png):
            raise Reject(f"icon: PNG chunk {tag!r} 越界",
                         code="icon.png.chunk", where=name,
                         got=f"{tag.decode('ascii', 'replace')}@{off}")
        body = png[off + 8 : off + 8 + length]
        (crc,) = struct.unpack(">I", png[off + 8 + length : off + 12 + length])
        if zlib.crc32(tag + body) & 0xFFFFFFFF != crc:
            raise Reject(f"icon: PNG chunk {tag!r} CRC 不符",
                         code="icon.png.crc", where=name,
                         got=f"{tag.decode('ascii', 'replace')}@{off}")
        out.append((tag, body))
        off += 12 + length
        if tag == b"IEND":
            break
    if off != len(png):
        raise Reject("icon: PNG IEND 后拖尾字节",
                     code="icon.png.trailing", where=name)
    if not out or out[0][0] != b"IHDR":
        raise Reject("icon: PNG 首 chunk 非 IHDR",
                     code="icon.png.ihdr", where=name)
    return out


def _paeth(a: int, b: int, c: int) -> int:
    # PNG 规范 Paeth 预测子（RFC 2083）：|p-a| 最小取 a，次 |p-b|，次 |p-c|。
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    if pb <= pc:
        return b
    return c


def png_to_byi(png: bytes, src_name: str) -> bytes:
    """PNG（depth 8、RGB/RGBA、非隔行、≤产面帽）→ BYI1 字节。

    收面 = 产面帽收紧的子集：depth≠8 / 彩色类型 ∉ {2, 6} / 隔行 / tRNS /
    单边 >1024 / w*h >65536 / CRC 坏 / IDAT 长度不符 / 未知 filter 一律拒
    （staging 放的是开发者可控源，产面从严 = 包内产物恒过 decode；tRNS
    是透明色键语义——静默丢弃 = 半透明像素丢成不透明，故 loud 拒）。
    """
    chunks = _png_chunks(png, src_name)
    ihdr, idat = chunks[0][1], b""
    if len(ihdr) != 13:
        raise Reject(f"icon: {src_name} IHDR 长 {len(ihdr)} ≠ 13",
                     code="icon.encode.ihdr", where=src_name,
                     expected=13, got=len(ihdr))
    w, h, depth, color, comp, filt, interlace = struct.unpack(">IIBBBBB", ihdr)
    # 应用规范 §3.3 图标源规格【v1 立法】：桌面图标源建议 108×108、有效
    # 内容居中、四周 ≥18px 裁切裕量（launcher 162px 目标几何采样 + 系统蒙
    # 版前提）。忠告面不拒（现役 32×32 demo 资产先于规格）——过小源在目标
    # 几何放大即糊，loud 提示开发者换源。
    if w < 108 or h < 108:
        print(f"mkapp: warn icon {src_name} {w}x{h} < 108 源规格"
              "（规范 §3.3：建议 108×108 居中 + ≥18px 裕量，过小源放大即糊）")
    if depth != 8 or color not in (2, 6):
        raise Reject(
            f"icon: {src_name} 应为 8-bit RGB/RGBA（得 depth={depth} color={color}；"
            "调色板/灰度/16-bit 请重导出）",
            code="icon.encode.color", where=src_name,
            expected="8-bit RGB/RGBA", got=f"depth={depth} color={color}",
        )
    if comp != 0 or filt != 0 or interlace != 0:
        raise Reject(f"icon: {src_name} 非基本压缩/过滤/隔行形态（interlace={interlace}）",
                     code="icon.encode.form", where=src_name,
                     got=f"interlace={interlace}")
    if not (1 <= w <= ICON_DIM_MAX and 1 <= h <= ICON_DIM_MAX and w * h <= ICON_PIXELS_MAX):
        raise Reject(
            f"icon: {src_name} 尺寸 {w}x{h} 越产面帽"
            f"（单边≤{ICON_DIM_MAX} 且 w*h≤{ICON_PIXELS_MAX}）",
            code="icon.encode.dim", where=src_name,
            expected=f"单边≤{ICON_DIM_MAX} 且 w*h≤{ICON_PIXELS_MAX}",
            got=f"{w}x{h}",
        )
    for tag, body in chunks[1:]:
        if tag == b"IDAT":
            idat += body  # 多 IDAT 拼接后一次解压（PNG 规范允许分段）
        elif tag == b"IEND":
            break
        elif tag == b"tRNS":
            raise Reject(f"icon: {src_name} 带 tRNS（透明色键）——请导出 RGBA",
                         code="icon.encode.trns", where=src_name,
                         fix="导出 RGBA（弃透明色键）")
        elif tag[:1].isupper():  # 大写首字母 = 关键 chunk——不认识就是拒
            raise Reject(f"icon: {src_name} 未知关键 chunk {tag!r}",
                         code="icon.encode.chunk", where=src_name,
                         got=tag.decode("ascii", "replace"))
        # 其余小写首字母 ancillary chunk（tEXt/gAMA…）跳过——不改像素语义
    bpp = 3 if color == 2 else 4
    try:
        raw = zlib.decompress(idat)
    except zlib.error as e:
        raise Reject(f"icon: {src_name} IDAT 解压失败： {e}",
                     code="icon.encode.idat", where=src_name, got=str(e))
    if len(raw) != h * (1 + w * bpp):
        raise Reject(f"icon: {src_name} 解压长 {len(raw)} ≠ 期望 {h * (1 + w * bpp)}",
                     code="icon.encode.idat.len", where=src_name,
                     expected=h * (1 + w * bpp), got=len(raw))
    # 逐行 unfilter（filters 0-4；首行 b/c 退化为 0）
    stride = w * bpp
    rows: list[bytearray] = []
    prev = bytearray(stride)
    pos = 0
    for _y in range(h):
        f = raw[pos]
        pos += 1
        if f > 4:
            raise Reject(f"icon: {src_name} 未知 filter 类型 {f}",
                         code="icon.encode.filter", where=src_name, got=f)
        line = bytearray(raw[pos : pos + stride])
        pos += stride
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if f == 1:
                line[i] = (line[i] + a) & 0xFF
            elif f == 2:
                line[i] = (line[i] + b) & 0xFF
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 0xFF
            elif f == 4:
                line[i] = (line[i] + _paeth(a, b, c)) & 0xFF
        rows.append(line)
        prev = line
    # 行主序 RLE（u32 = ARGB8888；RGB 源 alpha 恒 255）；run > 65535 拆分
    pixels: list[int] = []
    for line in rows:
        for i in range(0, stride, bpp):
            r, g, b = line[i], line[i + 1], line[i + 2]
            a = line[i + 3] if bpp == 4 else 255
            pixels.append((a << 24) | (r << 16) | (g << 8) | b)
    segs: list[tuple[int, int]] = []
    run_px, run_n = pixels[0], 1
    for px in pixels[1:]:
        if px == run_px and run_n < 65535:
            run_n += 1
            continue
        segs.append((run_n, run_px))
        run_px, run_n = px, 1
    segs.append((run_n, run_px))
    out = b"BYI1" + bytes((1, 0)) + struct.pack("<HH", w, h)
    for count, px in segs:
        out += struct.pack("<HI", count, px)
    return out


def _byi_validate(data: bytes, name: bytes) -> None:
    """--check 复验：包内 .byi 全量重走（header + RLE 段逐段 + Σcount ==
    w*h + 字节恰耗尽；约束面对齐 berry-gfx decode + 产面帽）。"""
    label = name.decode("ascii", "replace")
    if len(data) < 10 or data[:4] != b"BYI1":
        raise Reject(f"bap: {label} 非 BYI1（魔数不符）",
                     code="bap.byi.magic", where=label, expected="BYI1")
    if data[4] != 1 or data[5] != 0:
        raise Reject(f"bap: {label} version/flags 非 1/0",
                     code="bap.byi.version", where=label, expected="1/0")
    w, h = struct.unpack("<HH", data[6:10])
    if not (1 <= w <= ICON_DIM_MAX and 1 <= h <= ICON_DIM_MAX and w * h <= ICON_PIXELS_MAX):
        raise Reject(f"bap: {label} 尺寸 {w}x{h} 越产面帽",
                     code="bap.byi.dim", where=label,
                     expected=f"单边≤{ICON_DIM_MAX} 且 w*h≤{ICON_PIXELS_MAX}",
                     got=f"{w}x{h}")
    off, total = 10, 0
    while off < len(data):
        if off + 6 > len(data):
            raise Reject(f"bap: {label} RLE 段截断",
                         code="bap.byi.trunc", where=label, got=f"@{off}")
        count, _px = struct.unpack("<HI", data[off : off + 6])
        off += 6
        if count < 1:
            raise Reject(f"bap: {label} RLE 零长段",
                         code="bap.byi.zero", where=label, got=f"@{off - 6}")
        total += count
        if total > w * h:
            raise Reject(f"bap: {label} RLE 像素溢出",
                         code="bap.byi.overflow", where=label, got=f"{total} > {w * h}")
    if total != w * h:
        raise Reject(f"bap: {label} RLE Σcount {total} != w*h {w * h}",
                     code="bap.byi.sum", where=label,
                     expected=w * h, got=total)


def _ui_cross_check(m: M, icon_names: list[bytes], view_names: list[bytes]) -> None:
    """GUI-7 交叉门（build 与 --check 双跑，单源）：manifest 声明 ↔ 包内
    icons//views/ 条目互指。未声明 ⇒ 零条目；声明 ⇒ 在场；条目总数 ≤ 帽。"""
    if len(icon_names) > ICON_FILES_MAX:
        raise Reject(f"bap: icons/ 条目 {len(icon_names)} 越帽 {ICON_FILES_MAX}",
                     code="bap.ui.cross", where="icons/",
                     expected=f"≤{ICON_FILES_MAX}", got=len(icon_names))
    if len(view_names) > VIEWS_FILES_MAX:
        raise Reject(f"bap: views/ 条目 {len(view_names)} 越帽 {VIEWS_FILES_MAX}",
                     code="bap.ui.cross", where="views/",
                     expected=f"≤{VIEWS_FILES_MAX}", got=len(view_names))
    want_icon = m.ui[0].encode("ascii") if m.ui and m.ui[0] else None
    if want_icon is None:
        if icon_names:
            raise Reject(f"bap: 未声明 ui.icon 而包含 icons/ 条目 {sorted(icon_names)}",
                         code="bap.ui.cross", where="ui.icon",
                         fix="补 [ui] icon 声明或移除 icons/ 源")
    elif want_icon not in icon_names:
        raise Reject(f"bap: manifest ui.icon 不在包内（{want_icon.decode('ascii')}）",
                     code="bap.ui.cross", where="ui.icon",
                     got=want_icon.decode("ascii"),
                     fix="staging 放置同名 .png 源或删该声明")
    want_views = m.ui[2].encode("ascii") if m.ui and m.ui[2] else None
    if want_views is None:
        if view_names:
            raise Reject(f"bap: 未声明 ui.views 而包含 views/ 条目 {sorted(view_names)}",
                         code="bap.ui.cross", where="ui.views",
                         fix="补 [ui] views 声明或移除 views/ 文件")
    elif want_views not in view_names:
        raise Reject(f"bap: manifest ui.views 不在包内（{want_views.decode('ascii')}）",
                     code="bap.ui.cross", where="ui.views",
                     got=want_views.decode("ascii"),
                     fix="staging 放置同名文件或删该声明")


def _mk_png(w: int, h: int, color: int = 6, depth: int = 8, interlace: int = 0,
            raw: bytes | None = None,
            extra_chunks: list[tuple[bytes, bytes]] | None = None) -> bytes:
    """selftest 专用 PNG 工厂（最小合法件 + 可调形态面；raw 缺省 = 全零
    像素行；extra_chunks 插在 IHDR 后——tRNS 等 ancillary 负样本用）。
    非生产件——生产方向是 png_to_byi 收任意合规 PNG。"""
    if raw is None:
        bpp = 4 if color == 6 else 3
        raw = b"".join(b"\x00" + bytes(w * bpp) for _ in range(h))

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data)) + tag + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", w, h, depth, color, 0, 0, interlace)
    mid = b"".join(chunk(tag, data) for tag, data in (extra_chunks or []))
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + mid
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def build(out_path: str, app_dir: str, *, emit: bool = True) -> dict:
    """打包（门禁 1-5 全过才落盘）；emit=False 供 --json 模式静默取
    result 摘要（人读行逐字节同现状——selftest 钉板执法）。"""
    mf_path = os.path.join(app_dir, "manifest.toml")
    bin_dir = os.path.join(app_dir, "bin")
    if not os.path.isfile(mf_path) or not os.path.isdir(bin_dir):
        raise Reject(f"{app_dir}: 应含 manifest.toml 与 bin/",
                     code="stage.shape", where=app_dir,
                     expected="{manifest.toml, bin/}")
    extra = set(os.listdir(app_dir)) - {"manifest.toml", "bin", "icons", "views"}
    if extra:
        raise Reject(f"{app_dir}: 未知内容 {sorted(extra)}",
                     code="stage.unknown", where=app_dir, got=sorted(extra))
    bins = os.listdir(bin_dir)
    if len(bins) != 1:
        raise Reject(f"{bin_dir}: 应恰一个文件， 得 {len(bins)}",
                     code="stage.bin.count", where="bin/", expected=1, got=len(bins))
    bin_name = bins[0]
    bin_path = os.path.join(bin_dir, bin_name)
    if not os.path.isfile(bin_path):
        raise Reject(f"{bin_path}: 应为常规文件",
                     code="stage.bin.file", where=f"bin/{bin_name}")

    # GUI-7：staging icons/ 恰 .png 源（扁平常规文件，编码为 .byi 入包）；
    # views/ 恰非空常规文件（原样入包，DSL 定形 SDK-2，v1 opaque）。
    icon_dir = os.path.join(app_dir, "icons")
    view_dir = os.path.join(app_dir, "views")
    for d, cap, kind in (
        (icon_dir, ICON_FILES_MAX, "icons/"),
        (view_dir, VIEWS_FILES_MAX, "views/"),
    ):
        if os.path.isdir(d) and len(os.listdir(d)) > cap:
            raise Reject(f"{d}: 条目越帽 {cap}",
                         code="stage.cap", where=kind,
                         expected=f"≤{cap}", got=len(os.listdir(d)))
    icon_items: list[tuple[bytes, bytes]] = []
    if os.path.isdir(icon_dir):
        for name in sorted(os.listdir(icon_dir)):
            p = os.path.join(icon_dir, name)
            if not os.path.isfile(p):
                raise Reject(f"{icon_dir}: 应为扁平常规文件（得 {name}）",
                             code="stage.icons.file", where=f"icons/{name}")
            if not name.endswith(".png"):
                raise Reject(f"{icon_dir}: 恰收 .png 源（得 {name}）",
                             code="stage.icons.png", where=f"icons/{name}",
                             expected=".png 源")
            stem = name[: -len(".png")]
            entry = f"{ICON_PREFIX}{stem}.byi"
            if not stem:
                raise Reject(f"{icon_dir}: 空 stem（得 {name}）",
                             code="stage.icons.stem", where=f"icons/{name}")
            try:
                entry_b = entry.encode("ascii")
            except UnicodeEncodeError:
                raise Reject(f"{icon_dir}: 条目名 {entry} 须 ASCII",
                             code="stage.icons.name", where=f"icons/{name}")
            if len(entry_b) > UI_PATH_MAX:
                raise Reject(f"{icon_dir}: 产物名 {entry} 超长（{len(entry_b)}B > {UI_PATH_MAX}）",
                             code="stage.icons.len", where=f"icons/{name}",
                             expected=f"≤{UI_PATH_MAX}B", got=len(entry_b))
            with open(p, "rb") as f:
                byi = png_to_byi(f.read(), name)
            _byi_validate(byi, entry_b)  # 编码后再按解码面全量自验（产面子集）
            icon_items.append((entry_b, byi))
    view_items: list[tuple[bytes, bytes]] = []
    if os.path.isdir(view_dir):
        for name in sorted(os.listdir(view_dir)):
            p = os.path.join(view_dir, name)
            if not os.path.isfile(p):
                raise Reject(f"{view_dir}: 应为扁平常规文件（得 {name}）",
                             code="stage.views.file", where=f"views/{name}")
            entry = f"{VIEWS_PREFIX}{name}"
            try:
                entry_b = entry.encode("ascii")
            except UnicodeEncodeError:
                raise Reject(f"{view_dir}: 条目名 {entry} 须 ASCII",
                             code="stage.views.name", where=f"views/{name}")
            if len(entry_b) > UI_PATH_MAX or len(entry_b) <= len(VIEWS_PREFIX):
                raise Reject(f"{view_dir}: 条目名 {entry} 空/超长（≤{UI_PATH_MAX}）",
                             code="stage.views.len", where=f"views/{name}",
                             expected=f"≤{UI_PATH_MAX}B", got=len(entry_b))
            with open(p, "rb") as f:
                data = f.read()
            if not data:
                raise Reject(f"{p}: 空文件（views/ 须非空——占位至少一行注释）",
                             code="stage.views.empty", where=f"views/{name}",
                             fix="占位至少一行注释")
            view_items.append((entry_b, data))

    with open(mf_path, "rb") as f:
        raw = f.read()
    if len(raw) > MANIFEST_MAX:
        raise Reject(f"manifest.toml 超 MANIFEST_MAX {MANIFEST_MAX}B",
                     code="manifest.size", where="manifest.toml",
                     expected=f"≤{MANIFEST_MAX}B", got=len(raw))
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError:
        raise Reject("manifest.toml 非 ASCII",
                     code="manifest.encoding", where="manifest.toml",
                     expected="ASCII")
    m = check_manifest_text(text)

    # 门禁 3：entry == bin/ 唯一文件的完整包内路径
    if m.entry != f"bin/{bin_name}":
        raise Reject(f"manifest entry {m.entry!r} != bin/{bin_name!r}",
                     code="entry.mismatch", where="app.entry",
                     expected=f"bin/{bin_name}", got=m.entry)
    # GUI-7 交叉门（--check 同源复跑）
    _ui_cross_check(m, [n for n, _ in icon_items], [n for n, _ in view_items])
    # 门禁 4：输出名 == <id>.bap
    if os.path.basename(out_path) != f"{m.id}.bap":
        raise Reject(f"输出名应为 {m.id}.bap",
                     code="output.name", where=os.path.basename(out_path),
                     expected=f"{m.id}.bap")
    # 门禁 5：ELF 魔数
    with open(bin_path, "rb") as f:
        elf = f.read()
    if elf[:4] != b"\x7fELF":
        raise Reject(f"{bin_path}: 非 ELF（头 4 字节 {elf[:4]!r}）",
                     code="bin.elf", where=f"bin/{bin_name}",
                     expected="7f454c46", got=elf[:4].hex())

    payload_items = sorted(
        [
            (b"manifest.toml", raw),
            (m.entry.encode("ascii"), elf),
        ]
        + icon_items
        + view_items
    )
    # SDK-1a §4.3：SHA256SUMS——每 payload 条目一行 sha256 清单（双空格、
    # LF 收尾、行序 = 条目名字节序）。连同自身入包再排序：
    # 'S'(0x53) < 'b'(0x62) < 'i'(0x69) < 'm'(0x6d) < 'v'(0x76)，
    # §2.4 条目定序由排序自动满足（GUI-7 icons//views/ 并入同律）。
    sums = "".join(
        f"{hashlib.sha256(data).hexdigest()}  {name.decode('ascii')}\n"
        for name, data in payload_items
    ).encode("ascii")
    items = sorted(payload_items + [(b"SHA256SUMS", sums)])
    blob = build_archive(items)
    if len(blob) > BAP_MAX:
        raise Reject(f"包体 {len(blob)}B 超 BAP_MAX {BAP_MAX}",
                     code="bap.size", expected=f"≤{BAP_MAX}B", got=len(blob))
    with open(out_path, "wb") as f:
        f.write(blob)
    # 产包摘要行（宿主侧可见性，无消费面）；SDK-1a 尾随增量在场才打
    line = (
        f"mkapp: {out_path} {len(blob)}B {m.id} v{m.version} "
        f"console={'true' if m.console else 'false'} "
        f"stack={m.stack_pages} intent={m.intent}"
    )
    result = {
        "out": out_path, "bytes": len(blob), "id": m.id, "version": m.version,
        "console": bool(m.console), "stack_pages": m.stack_pages,
        "intent": m.intent,
    }
    if m.compat is not None:
        line += f" compat={_ver(m.compat[0])}..{_ver(m.compat[2])}"
        result["compat"] = f"{_ver(m.compat[0])}..{_ver(m.compat[2])}"
        # VER-01 提案第 4 条：level 位（有 min_level 才打——不破坏现有行
        # grep 面；记录位升消费，装载期检查在 appd）。
        if m.min_level is not None:
            line += f" min_level={m.min_level}"
            result["min_level"] = m.min_level
    if m.fs:
        line += f" fs={m.fs}"
        result["fs"] = m.fs
    if m.net:
        line += f" net={m.net}"
        result["net"] = m.net
    if m.ui:
        line += f" icons={len(icon_items)} views={len(view_items)}"
        result["icons"] = len(icon_items)
        result["views"] = len(view_items)
    if m.agent_actions:
        line += f" actions={len(m.agent_actions)}"
        result["actions"] = len(m.agent_actions)
    if emit:
        print(line)
    return result


# ---- SDK-1a §4.4：--check（既有 .bap 复验，只验不写）----


def _cpio_entries(blob: bytes) -> list[tuple[bytes, bytes]]:
    """cpio(newc) 走查器（--check 专用；mkcpio 只产不收、svcmgr 侧在
    guest 内——宿主侧无第二实现可借，本函数对读 mkcpio.entry 的编码
    几何：magic 070701 + 13 × hex8，filesize = 域[6]、namesize = 域[11]，
    名起 off+110 不含 NUL，头+名垫页、数据垫页（X-3c：垫 4096，与
    berry-loader cpio_walk/cpio_step 同口径四点一动），TRAILER!!! 终止）。
    越界/坏魔数 = Reject（敌意包 loud，不静默截断）。"""
    out: list[tuple[bytes, bytes]] = []
    off = 0
    while True:
        if off + 110 > len(blob):
            raise Reject(f"bap: cpio 头越界 @ {off}",
                         code="bap.cpio.header", where=f"cpio@{off}")
        if blob[off : off + 6] != b"070701":
            raise Reject(f"bap: cpio 坏魔数 @ {off}",
                         code="bap.cpio.header", where=f"cpio@{off}",
                         expected="070701")
        try:
            fields = [
                int(blob[off + 6 + i * 8 : off + 6 + (i + 1) * 8], 16)
                for i in range(13)
            ]
        except ValueError:
            raise Reject(f"bap: cpio 坏字段 @ {off}",
                         code="bap.cpio.header", where=f"cpio@{off}")
        filesize, namesize = fields[6], fields[11]
        if namesize < 1:
            raise Reject(f"bap: cpio 名长非法 @ {off}",
                         code="bap.cpio.header", where=f"cpio@{off}")
        name_off = off + 110
        if name_off + namesize > len(blob):
            raise Reject(f"bap: cpio 名越界 @ {off}",
                         code="bap.cpio.range", where=f"cpio@{off}")
        name = blob[name_off : name_off + namesize - 1]  # 剥尾 NUL
        data_off = (name_off + namesize + 4095) & ~4095  # 头+名垫页（X-3c）
        if data_off + filesize > len(blob):
            raise Reject(f"bap: cpio 数据越界 @ {off}",
                         code="bap.cpio.range", where=f"cpio@{off}")
        if name == b"TRAILER!!!":
            break
        out.append((name, blob[data_off : data_off + filesize]))
        off = (data_off + filesize + 4095) & ~4095  # 数据垫页（X-3c）
    return out


def _verify_sums(sums: bytes, payload: list[tuple[bytes, bytes]]) -> None:
    """SHA256SUMS 复验（§4.4 步 4）：LF 收尾、行数 = 条目数、双空格行形、
    行序 = 条目名字节序、逐文件重算比对；任一不符 = 拒。"""
    if not sums.endswith(b"\n"):
        raise Reject("bap: SHA256SUMS 应 LF 收尾",
                     code="bap.sums", where="SHA256SUMS", expected="LF 收尾")
    lines = sums[:-1].split(b"\n")
    expect = sorted(payload)
    if len(lines) != len(expect):
        raise Reject(f"bap: SHA256SUMS 行数 {len(lines)} != 条目数 {len(expect)}",
                     code="bap.sums", where="SHA256SUMS",
                     expected=len(expect), got=len(lines))
    for line, (name, data) in zip(lines, expect):
        mo = SUMS_LINE_RE.fullmatch(line)
        if not mo or mo.group(2) != name:
            raise Reject(f"bap: SHA256SUMS 行形/行序不符 {line!r}",
                         code="bap.sums", where="SHA256SUMS",
                         got=line.decode("ascii", "replace"))
        if mo.group(1).decode("ascii") != hashlib.sha256(data).hexdigest():
            raise Reject(f"bap: SHA256SUMS 摘要不符 {name!r}",
                         code="bap.sums", where=name.decode("ascii", "replace"))


def check_bap(path: str, *, emit: bool = True) -> dict:
    """--check 五步（§4.4）：形状 → manifest 全量门禁 → entry 对表 →
    SHA256SUMS 复验（在场）→ ELF 魔数；通过打一行 OK（emit=False 供
    --json 静默取 result）。"""
    with open(path, "rb") as f:
        blob = f.read()
    if len(blob) > BAP_MAX:
        raise Reject(f"bap: 包体 {len(blob)}B 超 BAP_MAX {BAP_MAX}",
                     code="bap.size", expected=f"≤{BAP_MAX}B", got=len(blob))
    entries = _cpio_entries(blob)
    names = [n for n, _ in entries]
    if len(names) != len(set(names)):
        raise Reject(f"bap: 条目重名 {sorted(set(names))}",
                     code="bap.cpio.dup",
                     got=[n.decode("ascii", "replace") for n in sorted(set(names))
                          if names.count(n) > 1])
    if b"manifest.toml" not in names:
        raise Reject("bap: 缺 manifest.toml 条目",
                     code="bap.cpio.entries", where="manifest.toml", expected="在场")
    bin_entries = [n for n in names if n.startswith(b"bin/")]
    icon_entries = [n for n in names if n.startswith(b"icons/")]
    view_entries = [n for n in names if n.startswith(b"views/")]
    others = (
        set(names)
        - {b"manifest.toml", b"SHA256SUMS"}
        - set(bin_entries)
        - set(icon_entries)
        - set(view_entries)
    )
    # 条件改为按类清点（GUI-7 前恰 2/3 条历史形状仍收）：manifest 恰一 +
    # bin/ 恰一 + SHA256SUMS ≤1 + icons/ ≤帽 + views/ ≤帽 + 其余为零。
    if (
        len(bin_entries) != 1
        or len(icon_entries) > ICON_FILES_MAX
        or len(view_entries) > VIEWS_FILES_MAX
        or others
    ):
        raise Reject(
            "bap: 条目形不符（manifest.toml + 唯一 bin/ + SHA256SUMS? + "
            f"icons/*.byi ≤{ICON_FILES_MAX} + views/* ≤{VIEWS_FILES_MAX}），"
            f"得 {sorted(names)}",
            code="bap.cpio.shape",
            got=[n.decode("ascii", "replace") for n in sorted(names)],
        )
    for n in icon_entries:
        tail = n[len(b"icons/") :]
        if not tail.endswith(b".byi") or b"/" in tail or len(n) > UI_PATH_MAX:
            raise Reject(f"bap: icons/ 条目名形不符 {n!r}（扁平 .byi、≤{UI_PATH_MAX}B）",
                         code="bap.icons.name",
                         where=n.decode("ascii", "replace"),
                         expected=f"扁平 .byi、≤{UI_PATH_MAX}B")
        _byi_validate(dict(entries)[n], n)
    for n in view_entries:
        tail = n[len(b"views/") :]
        if not tail or b"/" in tail or len(n) > UI_PATH_MAX:
            raise Reject(f"bap: views/ 条目名形不符 {n!r}（扁平、≤{UI_PATH_MAX}B）",
                         code="bap.views.name",
                         where=n.decode("ascii", "replace"),
                         expected=f"扁平、≤{UI_PATH_MAX}B")
        if not dict(entries)[n]:
            raise Reject(f"bap: views/ 条目 {n!r} 为空",
                         code="bap.views.empty",
                         where=n.decode("ascii", "replace"))
    ent = dict(entries)

    raw = ent[b"manifest.toml"]
    if len(raw) > MANIFEST_MAX:
        raise Reject(f"manifest.toml 超 MANIFEST_MAX {MANIFEST_MAX}B",
                     code="manifest.size", where="manifest.toml",
                     expected=f"≤{MANIFEST_MAX}B", got=len(raw))
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError:
        raise Reject("manifest.toml 非 ASCII",
                     code="manifest.encoding", where="manifest.toml",
                     expected="ASCII")
    m = check_manifest_text(text)  # 全量门禁复跑（含三值链/词表/规范形）
    if m.entry.encode("ascii") != bin_entries[0]:
        raise Reject(
            f"manifest entry {m.entry!r} != 包内条目 {bin_entries[0].decode('ascii', 'replace')!r}",
            code="entry.mismatch", where="app.entry",
            expected=bin_entries[0].decode("ascii", "replace"), got=m.entry,
        )

    elf = ent[bin_entries[0]]
    if elf[:4] != b"\x7fELF":
        raise Reject(f"bap: 非 ELF（头 4 字节 {elf[:4]!r}）",
                     code="bap.elf",
                     where=bin_entries[0].decode("ascii", "replace"),
                     expected="7f454c46", got=elf[:4].hex())
    # GUI-7 交叉门（build 同源复跑——manifest 声明 ↔ 包内条目互指）
    _ui_cross_check(m, icon_entries, view_entries)

    digests = "n/a"
    if b"SHA256SUMS" in ent:
        _verify_sums(
            ent[b"SHA256SUMS"],
            [(n, d) for n, d in entries if n != b"SHA256SUMS"],
        )
        digests = "ok"
    if emit:
        print(f"mkapp: {path} OK {len(blob)}B entries={len(entries)} digests={digests}")
    return {"path": path, "bytes": len(blob), "entries": len(entries),
            "digests": digests}


# ---- P1 签名面（SDK-2 分发签名链集成刀；24 号 §4.3 半刀②③——签名产出
# + 验签门禁。可选：无 --sign-key/--verify-key 时本节零参与，行为与旧版
# 逐字节同。berry-sign = user/berry-crypto bin（信封 v1 TLV + ed25519 +
# SHA-512/256 摘要——envelope.rs/ed25519.rs/sha512.rs 单源）；验收线四件
# 归属 = `berry-sign selftest`（头注「签名面」段）。----


def _find_berry_sign() -> str | None:
    """berry-sign 二进制发现：env BERRY_SIGN → 构建产物候选（repo
    target-cargo 与 /tmp/target-p1 两个 CARGO_TARGET_DIR 惯用位 ×
    debug/release）。不可得 = None（调用方 loud 拒——签名面不静默）。"""
    env = os.environ.get("BERRY_SIGN")
    if env and os.path.isfile(env) and os.access(env, os.X_OK):
        return env
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    for tdir in (os.path.join(root, "target-cargo"), "/tmp/target-p1"):
        for profile in ("debug", "release"):
            cand = os.path.join(tdir, "aarch64-apple-darwin", profile, "berry-sign")
            if os.path.isfile(cand) and os.access(cand, os.X_OK):
                return cand
    return None


def _run_berry_sign(argv: list[str]) -> subprocess.CompletedProcess:
    bs = _find_berry_sign()
    if bs is None:
        raise Reject(
            "sign: berry-sign 不可得（设 BERRY_SIGN 或先 "
            "CARGO_TARGET_DIR=/tmp/target-p1 cargo build --manifest-path user/Cargo.toml "
            "-p berry-crypto --bin berry-sign --target aarch64-apple-darwin）",
            code="sign.tool-missing",
            fix="构建 berry-sign 或设 BERRY_SIGN 指向既有二进制",
        )
    try:
        return subprocess.run([bs, *argv], capture_output=True, text=True)
    except OSError as e:
        raise Reject(f"sign: 执行 {bs} 失败： {e}", code="sign.exec")


def _sign_bap(sk: str, bap_path: str) -> str:
    """打包尾部产 <bap>.sig（berry-sign sign；manifest 无 [security] 三键
    由 berry-sign 拒——key_lineage 是信封回声源，mkapp 不重复执法）。"""
    r = _run_berry_sign(["sign", sk, bap_path])
    if r.returncode != 0:
        raise Reject((r.stderr or r.stdout).strip(), code="sign.sign")
    return (r.stdout or "").strip()


def _verify_bap_sig(pk: str, bap_path: str) -> str:
    """check 尾部验 <bap>.sig（berry-sign verify；.sig 缺失 = berry-sign
    拒且文案含防剥离注——v1.2 §三 :63 护栏① host 对应物）。"""
    r = _run_berry_sign(["verify", pk, bap_path])
    if r.returncode != 0:
        raise Reject((r.stderr or r.stdout).strip(), code="sign.verify")
    return (r.stdout or "").strip()


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
   **GUI-7 可选**：manifest 声明 `[ui]` icon/views 后，staging 放
   `icons/<名>.png` 源（mkapp 编码为 `icons/<名>.byi` 入包——源不入包）
   与 `views/<名>` 文件（原样入包）；未声明而放置会被交叉门拒；
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


def do_new(app_dir: str, app_id: str, *, emit: bool = True) -> dict:
    """--new：脚手架四件（§4.5）。basename(dir) == ID.lower()（id 大写
    惯例对齐 demo 双 app）；目录已存在且非空 = 拒（防覆写）。**不含
    bin/ 内容**——产物是「不可打包的源码工程」，构建 ELF 放入 bin/ 后
    才可打包（空 bin/ 会被 build 的「恰一个文件」门禁拒，两步流有意）。"""
    slug = os.path.basename(os.path.normpath(app_dir))
    if slug != app_id.lower():
        raise Reject(
            f"--new: basename({app_dir!r}) = {slug!r} != --id {app_id!r} 的小写形",
            code="new.basename", where=app_dir,
            expected=app_id.lower(), got=slug,
        )
    _check_str(app_id, "id", ID_MAX, key="new.id")
    if not ID_RE.fullmatch(app_id):
        raise Reject("manifest: id 字符集 [A-Za-z0-9._-]、首字符字母数字",
                     code="new.id", where="id",
                     expected="[A-Za-z0-9._-]、首字符字母数字")
    # slug 直接成为 Cargo.toml 包名——cargo 包名词法拒 '.'（ID_RE 的宽松面
    # 是 schema 既有承诺，打包主路径不拦；--new 产出必须可构建，故此收紧）。
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]*", slug):
        raise Reject(
            f"--new: slug {slug!r} 不是合法 cargo 包名（含点号等 cargo 拒收字符；"
            "id 勿含 '.'，请改用 '-' 或 '_'）",
            code="new.slug.cargo", where=slug,
            fix="id 勿含 '.'，改用 '-' 或 '_'",
        )
    if os.path.isfile(app_dir):
        raise Reject(f"--new: {app_dir} 是文件",
                     code="new.isfile", where=app_dir)
    if os.path.isdir(app_dir) and os.listdir(app_dir):
        raise Reject(f"--new: {app_dir} 已存在且非空（防覆写）",
                     code="new.exists", where=app_dir,
                     fix="换新目录或先清空")
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
    if emit:
        print(f"mkapp: {app_dir} 脚手架四件就绪（{app_id}；构建步骤见 {app_dir}/README.md）")
    return {"dir": app_dir, "id": app_id, "files": sorted(files)}


BASE = (
    "[app]\n"
    'id = "APPDEMO"\n'
    'version = "0.1.0"\n'
    'entry = "bin/appdemo"\n'
    'type = "app"\n'
    'intent = "utility"\n'
    "\n[capabilities]\nconsole = true\n"
)


# 人读面钉板（刀7 --json 起）：selftest 主体 stdout 归一化（tempdir →
# <TD>）后与此逐字节比对；人读输出任何漂移（文案/顺序/增删行）即 FAIL。
# 基线 = 刀7 前实测捕获（85 行含尾 verdict；此处 84 行主体，verdict 另打），
# 生成法与再生法见 research 工作区 /tmp/k3-review/sdk-api/刀7落库/。
SELFTEST_BASELINE = """\
  selftest 拒（预期） : dotted-key
  selftest 拒（预期） : hex-int
  selftest 拒（预期） : underscore-int
  selftest 拒（预期） : crlf
  selftest 拒（预期） : escape-in-string
  selftest 拒（预期） : inline-table
  selftest 拒（预期） : multiline-string
  selftest 拒（预期） : single-quotes
  selftest 拒（预期） : trailing-comment
  selftest 拒（预期） : compat-missing-key
  selftest 拒（预期） : compat-order
  selftest 拒（预期） : compile-above-sdk
  selftest 拒（预期） : ver-leading-zero
  selftest 拒（预期） : ver-two-segs
  selftest 拒（预期） : ver-four-segs
  selftest 拒（预期） : fs-vocab-tmpfs
  selftest 拒（预期） : fs-vocab-appdata
  selftest 拒（预期） : fs-not-array
  selftest 拒（预期） : net-vocab-server
  selftest 拒（预期） : sec-missing-key
  selftest 拒（预期） : sec-missing-integrator
  selftest 拒（预期） : sec-value-space
  selftest 拒（预期） : sec-value-17b
  selftest 拒（预期） : ui-no-prefix
  selftest 拒（预期） : ui-wrong-prefix
  selftest 拒（预期） : ui-bare-prefix
  selftest 拒（预期） : ui-not-byi
  selftest 拒（预期） : ui-path-33b
  selftest 拒（预期） : ui-views-no-prefix
  selftest 拒（预期） : ui-views-bare
  selftest 拒（预期） : ui-unknown-key
  selftest 拒（预期） : ui-label-empty
  selftest 拒（预期） : ui-label-33b
  selftest 拒（预期） : ui-empty-section
  selftest 拒（预期） : ui-before-security
  selftest 拒（预期） : aa-unknown-tier
  selftest 拒（预期） : aa-upper-tier
  selftest 拒（预期） : aa-underscore-key
  selftest 拒（预期） : aa-digit-key
  selftest 拒（预期） : aa-over-cap
  selftest 拒（预期） : aa-empty-section
  selftest 拒（预期） : blob-missing-field
  selftest 拒（预期） : blob-unknown-key
  selftest 拒（预期） : blob-value-65b
  selftest 拒（预期） : blob-empty-section
  selftest 拒（预期） : apptomb-id-25b
  selftest 过（预期） : canonical id=APPDEMO
  selftest 过（预期） : canonical-runtime id=APPDEMO
  selftest 过（预期） : f1-compat id=X
  selftest 过（预期） : f2-fs id=X
  selftest 过（预期） : f3-net id=X
  selftest 过（预期） : f4-security id=X
  selftest 过（预期） : f6-kitchen-sink id=KITCHEN
  selftest 过（预期） : f7-ui-kitchen-sink id=KITCHEN
  selftest 过（预期） : f7b-ui-icon-only id=X
  selftest 过（预期） : f8-agent-actions id=KITCHEN
  selftest 过（预期） : f9-blob-kitchen-sink id=KITCHEN
  selftest 过（预期） : f9b-blob-only id=X
  selftest 过（预期） : apptomb id=APPTOMB
  selftest 过（预期） : enc-golden-2x2
  selftest 过（预期） : enc-rgb-alpha-255
  selftest 过（预期） : enc-run-split-65535
  selftest 拒（预期） : enc-not-png
  selftest 拒（预期） : enc-truncated
  selftest 拒（预期） : enc-bad-crc
  selftest 拒（预期） : enc-palette
  selftest 拒（预期） : enc-gray
  selftest 拒（预期） : enc-16bit
  selftest 拒（预期） : enc-interlaced
  selftest 拒（预期） : enc-dim-1025
  selftest 拒（预期） : enc-pixels-over
  selftest 拒（预期） : enc-raw-len
  selftest 拒（预期） : enc-filter-5
  selftest 拒（预期） : enc-trns
mkapp: <TD>/myapp 脚手架四件就绪（MYAPP；构建步骤见 <TD>/myapp/README.md）
mkapp: <TD>/MYAPP.bap 28672B MYAPP v0.1.0 console=true stack=8 intent=utility compat=0.1.0..0.1.0
mkapp: <TD>/MYAPP.bap OK 28672B entries=3 digests=ok
  selftest 过（预期） : new-build-check-roundtrip id=MYAPP
mkapp: <TD>/LEGACY.bap OK 20480B entries=2 digests=n/a
  selftest 过（预期） : legacy-2entry-check digests=n/a
mkapp: <TD>/APPUI.bap 45056B APPUI v0.1.0 console=true stack=8 intent=utility icons=1 views=1
mkapp: <TD>/APPUI.bap OK 45056B entries=5 digests=ok
  selftest 过（预期） : ui-build-check-roundtrip icons+views 在包
  selftest 拒（预期） : ui-stage-no-icons
  selftest 拒（预期） : ui-stage-undeclared-icons
  selftest 拒（预期） : ui-stage-undeclared-views
  selftest 拒（预期） : ui-stage-not-png
  selftest 拒（预期） : ui-stage-view-empty
  selftest 拒（预期） : ui-stage-icons-over-cap
  selftest 拒（预期） : ui-stage-non-ascii-icon
  selftest 拒（预期） : ui-stage-icon-name-too-long
"""


def selftest() -> int:
    """门禁自检（§2 + SDK-1a §4.6）：负样本 = tomllib 收下而门禁必须拒
    的形态——证明第 2 道规范形比较真的在收紧 tomllib 的超集（SDK-1a 起
    负面另含三值链/词表/security 的第 1 道拒面）；正对照 = 规范形必须过
    （防空转：check 恒拒也满足「负样本全拒」）——BASE 字节零漂移 =
    add-only 执法样本，F 形 = §3.2 字节模板七形态中五款 + roundtrip。
    刀7 起两腿增量：①主体输出归一化（td → <TD>）与 SELFTEST_BASELINE
    逐字节钉板（人读面 add-only 执法）后原样 replay；②--json 信封腿
    （_json_selftest——八类 canonical 拒因 code + 成功信封 + CLI 面）。"""
    import io

    real_stdout = sys.stdout
    buf = io.StringIO()
    tds: list[str] = []  # tempdir 收集——钉板比对前归一为 <TD>
    sys.stdout = buf
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
    f4 = f0 + '\n[security]\nprofile = "ed25519-main"\nkey_lineage = "root-2026"\nintegrator = "oem-main"\n'
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
        'integrator = "oem-main"\n'
    )
    # GUI-7 F7：[ui] 尾增（居末 = F0–F6 字节零漂移）；三键独立故
    # f7b icon-only 同为规范形。
    f7 = f6 + (
        "\n[ui]\n"
        'icon = "icons/icon.byi"\n'
        'label = "KITCHEN"\n'
        'views = "views/main.bui"\n'
    )
    f7b = f0 + '\n[ui]\nicon = "icons/icon.byi"\n'
    # 同意面先落件二 F8：[agent-actions] 尾增（居 [ui] 后 = F0–F7 字节
    # 零漂移）；键 kebab-case 文档序、值三档词。
    f8 = f7 + (
        "\n[agent-actions]\n"
        'open = "routine"\n'
        'save = "sensitive"\n'
        'empty-trash = "destructive"\n'
    )
    # ADR #82 V6 前置 F9：[blob] 尾增（居 [agent-actions] 后 = F0–F8 字节
    # 零漂移）；五字段键定序 license/origin/sha256/size/usage，值只校验
    # 形态。f9b 最小形（仅 [blob]）同为规范形。
    f9 = f8 + (
        "\n[blob]\n"
        'license = "Apache-2.0"\n'
        'origin = "soc-vendor-a"\n'
        'sha256 = "0123abcd"\n'
        'size = "65536"\n'
        'usage = "gpu-microcode"\n'
    )
    f9b = (
        f0
        + '\n[blob]\nlicense = "MIT"\norigin = "o"\nsha256 = "s"\nsize = "1"\nusage = "u"\n'
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
        # ---- ADR #82 V3 新增拒面（[security] 三键化：值变异在规范位上
        # ——门删则 canonical 重发射与输入恒等 → 样本被放行 → selftest
        # FAIL，同 SDK-1a 词表样本手法钉得住门；sec-missing-integrator =
        # V3 前旧两键形，三键化不变量的正身样本）----
        ("sec-missing-key", f4.replace('\nkey_lineage = "root-2026"\n', "\n")),
        ("sec-missing-integrator", f4.replace('\nintegrator = "oem-main"\n', "\n")),
        ("sec-value-space", f0 + '\n[security]\nprofile = "a b"\nkey_lineage = "x"\nintegrator = "i"\n'),
        ("sec-value-17b", f0 + '\n[security]\nprofile = "' + "a" * 17 + '"\nkey_lineage = "x"\nintegrator = "i"\n'),
        # ---- GUI-7 新增拒面（[ui] 值域/产面 .byi 后缀/空节/定序；值变异
        # 在规范位上——门删则 canonical 重发射与输入恒等 → 样本被放行 →
        # selftest FAIL，同 SDK-1a 词表样本手法钉得住门）----
        ("ui-no-prefix", f7b.replace("icons/icon.byi", "icon.byi")),
        ("ui-wrong-prefix", f7b.replace("icons/icon.byi", "bin/icon.byi")),
        ("ui-bare-prefix", f7b.replace("icons/icon.byi", "icons/")),
        ("ui-not-byi", f7b.replace("icons/icon.byi", "icons/icon.png")),
        ("ui-path-33b", f7b.replace("icons/icon.byi", "icons/" + "a" * 23 + ".byi")),
        ("ui-views-no-prefix", f0 + '\n[ui]\nviews = "main.bui"\n'),
        ("ui-views-bare", f0 + '\n[ui]\nviews = "views/"\n'),
        ("ui-unknown-key", f0 + '\n[ui]\nbadge = "x"\n'),
        ("ui-label-empty", f0 + '\n[ui]\nlabel = ""\n'),
        ("ui-label-33b", f0 + '\n[ui]\nlabel = "' + "a" * 33 + '"\n'),
        ("ui-empty-section", f0 + "\n[ui]\n"),
        (
            "ui-before-security",
            f0 + '\n[ui]\nicon = "icons/icon.byi"\n'
            '\n[security]\nprofile = "p"\nkey_lineage = "k"\nintegrator = "i"\n',
        ),
        # ---- 同意面先落件二拒面（[agent-actions] 档值域/键字符集/越帽/
        # 空节；值变异在规范位（f0 尾 = canonical [agent-actions] 位）——
        # 门删则 canonical 重发射与输入恒等 → 样本被放行 → selftest
        # FAIL，同 SDK-1a/GUI-7 样本手法钉得住门）----
        ("aa-unknown-tier", f0 + '\n[agent-actions]\nopen = "urgent"\n'),
        ("aa-upper-tier", f0 + '\n[agent-actions]\nopen = "Routine"\n'),
        ("aa-underscore-key", f0 + '\n[agent-actions]\nempty_trash = "destructive"\n'),
        ("aa-digit-key", f0 + '\n[agent-actions]\n1open = "routine"\n'),
        (
            "aa-over-cap",
            f0 + "\n[agent-actions]\n"
            + "".join(f'act{i:02} = "routine"\n' for i in range(AGENT_ACTIONS_MAX + 1)),
        ),
        ("aa-empty-section", f0 + "\n[agent-actions]\n"),
        # ---- ADR #82 V6 前置拒面（[blob] 声明校验：缺一/多键/越帽/空节；
        # 值变异在规范位（f0 尾 = canonical [blob] 位）——门删则 canonical
        # 重发射与输入恒等 → 样本被放行 → selftest FAIL，同上手法钉得住门）----
        ("blob-missing-field", f0 + '\n[blob]\nlicense = "MIT"\norigin = "o"\nsha256 = "s"\nsize = "1"\n'),
        (
            "blob-unknown-key",
            f9b + 'note = "x"\n',
        ),
        (
            "blob-value-65b",
            f0 + '\n[blob]\nlicense = "' + "a" * 65 + '"\norigin = "o"\nsha256 = "s"\nsize = "1"\nusage = "u"\n',
        ),
        ("blob-empty-section", f0 + "\n[blob]\n"),
        # app-1 段 b-2 补件（调和审计 mkapp 路余量）：APPTOMB id 25B 越界——
        # 钉 [app].id 长度门（ID_MAX=24 与墓碑表 id[24] 定长槽镜像同值，
        # app-1 草案 v1.2 §4/§7）；值变异唯一违规则 = 长度（"A"*25 过字符集，
        # 其余字节零扰动）——门删则 canonical 恒等重发射放行 → selftest FAIL。
        ("apptomb-id-25b", (
            "[app]\n"
            'id = "' + "A" * 25 + '"\n'
            'version = "0.1.0"\n'
            'entry = "bin/apptomb"\n'
            'type = "app"\n'
            'intent = "background"\n'
            "\n[capabilities]\n"
            "console = true\n"
            "\n[runtime]\n"
            "stack_pages = 4\n"
        )),
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
        ("f7-ui-kitchen-sink", f7),
        ("f7b-ui-icon-only", f7b),
        ("f8-agent-actions", f8),
        # ADR #82 V6 前置正形：F9 全组合尾增 + F9b 最小形（[blob] 五字段
        # 齐备 = canonical 位恰合——规范形定序互钉）。
        ("f9-blob-kitchen-sink", f9),
        ("f9b-blob-only", f9b),
        # app-1 段 b-2：APPTOMB 金件——真实包 user/bins/apptomb/
        # manifest.toml 逐字节镜像（intent="background" 首个正样本 =
        # 墓碑自验件定档；stack_pages=4 自验小栈）；形漂移（intent 词表/
        # 定序/节间空行）即炸此锚。add-only：居 f8 后零扰动既有锚。
        ("apptomb", (
            "[app]\n"
            'id = "APPTOMB"\n'
            'version = "0.1.0"\n'
            'entry = "bin/apptomb"\n'
            'type = "app"\n'
            'intent = "background"\n'
            "\n[capabilities]\n"
            "console = true\n"
            "\n[runtime]\n"
            "stack_pages = 4\n"
        )),
    ]
    for label, text in pos:
        try:
            m = check_manifest_text(text)
            print(f"  selftest 过（预期） : {label} id={m.id}")
        except Reject as e:
            print(f"  selftest FAIL: {label} 被门禁误拒： {e}")
            bad += 1
    # ---- GUI-7：编码器面（正 = 金字节钉板 / RGB alpha 强制 / 65536 run
    # 拆段；负 = 产面帽与 wire 收面拒）。----
    # 2×2 RGBA 四异像素 → 四段 count=1，字节逐位钉板（对读 berry-gfx
    # decode 约束——改 wire 格式必炸此锚）。像素值 = (a<<24)|(r<<16)|
    # (g<<8)|b；raw22 字节序 = PNG 行序 (r,g,b,a)。
    raw22 = (
        b"\x00" + bytes((255, 0, 0, 255)) + bytes((0, 255, 0, 255))
        + b"\x00" + bytes((0, 0, 255, 255)) + bytes((128, 128, 128, 128))
    )
    gold22 = (
        b"BYI1" + bytes((1, 0)) + struct.pack("<HH", 2, 2)
        + b"".join(
            struct.pack("<HI", 1, px)
            for px in (0xFFFF0000, 0xFF00FF00, 0xFF0000FF, 0x80808080)
        )
    )
    for label, got, want in [
        ("enc-golden-2x2", png_to_byi(_mk_png(2, 2, raw=raw22), "icon.png"), gold22),
        (
            "enc-rgb-alpha-255",
            png_to_byi(_mk_png(1, 1, color=2, raw=b"\x00\xff\x00\x00"), "icon.png"),
            b"BYI1" + bytes((1, 0)) + struct.pack("<HH", 1, 1)
            + struct.pack("<HI", 1, 0xFFFF0000),
        ),
    ]:
        if got == want:
            print(f"  selftest 过（预期） : {label}")
        else:
            print(f"  selftest FAIL: {label} 字节不等 {got.hex()} != {want.hex()}")
            bad += 1
    # 256×256 纯色：单 run 65536 > u16 帽 → 拆 [65535, 1] 两段（总 22B）
    split = png_to_byi(_mk_png(256, 256), "icon.png")
    if (
        len(split) == 22
        and struct.unpack("<HH", split[6:10]) == (256, 256)
        and struct.unpack("<HI", split[10:16]) == (65535, 0)
        and struct.unpack("<HI", split[16:22]) == (1, 0)
    ):
        print("  selftest 过（预期） : enc-run-split-65535")
    else:
        print(f"  selftest FAIL: enc-run-split-65535 得 {len(split)}B {split.hex()}")
        bad += 1
    for label, png in [
        ("enc-not-png", b"BYI1-definitely-not-a-png"),
        ("enc-truncated", _mk_png(2, 2)[:-4]),
        ("enc-bad-crc", _mk_png(2, 2)[:-4] + b"\xde\xad\xbe\xef"),
        ("enc-palette", _mk_png(1, 1, color=3, raw=b"\x00\x00")),
        ("enc-gray", _mk_png(1, 1, color=0, raw=b"\x00\x00")),
        ("enc-16bit", _mk_png(1, 1, depth=16, raw=b"\x00" + bytes(8))),
        ("enc-interlaced", _mk_png(1, 1, interlace=1)),
        ("enc-dim-1025", _mk_png(1025, 1)),
        ("enc-pixels-over", _mk_png(1024, 65)),
        ("enc-raw-len", _mk_png(2, 2, raw=b"\x00" * 4)),
        ("enc-filter-5", _mk_png(1, 1, raw=b"\x05\xff\x00\x00\xff")),
        (  # tRNS 透明色键：静默丢 = 半透明丢成不透明，须 loud 拒
            "enc-trns",
            _mk_png(1, 1, color=2, raw=b"\x00\xff\x00\x00",
                    extra_chunks=[(b"tRNS", struct.pack(">HHH", 255, 0, 0))]),
        ),
    ]:
        try:
            png_to_byi(png, "icon.png")
        except Reject:
            print(f"  selftest 拒（预期） : {label}")
            continue
        print(f"  selftest FAIL: {label} 被编码器放行")
        bad += 1
    # roundtrip：--new → 伪 ELF 入 bin/ → build → --check 全链（§4.5 尾约）
    # + 旧形 2 条目 .bap 过 --check（SDK-1a 前历史形状兼容）。
    import tempfile

    with tempfile.TemporaryDirectory() as td:
        tds.append(td)
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
        # ---- GUI-7：[ui] staging 全链（build 编码入包 → check 复验 →
        # icons//views/ 条目在包）+ 交叉门/staging 面负样本。----
        ui_mf = (
            "[app]\n"
            'id = "APPUI"\n'
            'version = "0.1.0"\n'
            'entry = "bin/appui"\n'
            'type = "app"\n'
            'intent = "utility"\n'
            "\n[capabilities]\nconsole = true\n"
            "\n[ui]\n"
            'icon = "icons/icon.byi"\n'
            'views = "views/main.bui"\n'
        )
        tiny_png = _mk_png(2, 2, raw=raw22)

        def ui_stage(name: str, mf: str, icons: list[tuple[str, bytes]],
                     views: list[tuple[str, bytes]]) -> str:
            d = os.path.join(td, name)
            os.makedirs(os.path.join(d, "bin"))
            with open(os.path.join(d, "manifest.toml"), "w") as f:
                f.write(mf)
            with open(os.path.join(d, "bin", "appui"), "wb") as f:
                f.write(b"\x7fELF")
            if icons:
                os.makedirs(os.path.join(d, "icons"))
            for fn, data in icons:
                with open(os.path.join(d, "icons", fn), "wb") as f:
                    f.write(data)
            if views:
                os.makedirs(os.path.join(d, "views"))
            for fn, data in views:
                with open(os.path.join(d, "views", fn), "wb") as f:
                    f.write(data)
            return d

        try:
            stage = ui_stage(
                "appui-stage", ui_mf,
                [("icon.png", tiny_png)], [("main.bui", b"# v\n")],
            )
            out = os.path.join(td, "APPUI.bap")
            build(out, stage)
            check_bap(out)
            with open(out, "rb") as f:
                names = [n for n, _ in _cpio_entries(f.read())]
            assert b"icons/icon.byi" in names and b"views/main.bui" in names, names
            print("  selftest 过（预期） : ui-build-check-roundtrip icons+views 在包")
        except (Reject, OSError, AssertionError) as e:
            print(f"  selftest FAIL: ui-build-check-roundtrip： {e}")
            bad += 1
        plain_mf = ui_mf.split("\n[ui]\n")[0]
        for label, stage_d in [
            ("ui-stage-no-icons", ui_stage("s1", ui_mf, [], [])),
            ("ui-stage-undeclared-icons", ui_stage("s2", plain_mf, [("icon.png", tiny_png)], [])),
            ("ui-stage-undeclared-views", ui_stage("s3", plain_mf, [], [("main.bui", b"# v\n")])),
            ("ui-stage-not-png", ui_stage("s4", ui_mf, [("icon.txt", b"note")], [("main.bui", b"# v\n")])),
            ("ui-stage-view-empty", ui_stage("s5", ui_mf, [("icon.png", tiny_png)], [("main.bui", b"")])),
            (
                "ui-stage-icons-over-cap",
                ui_stage(
                    "s6", ui_mf,
                    [("icon.png", tiny_png)] + [(f"i{k}.png", tiny_png) for k in range(4)],
                    [],
                ),
            ),
            (  # 非 ASCII 文件名：ASCII 拒点先于 PNG 解析炸响（免 traceback）
                "ui-stage-non-ascii-icon", ui_stage("s7", ui_mf, [("图.png", b"stub")], []),
            ),
            (  # 纯 ASCII 但条目 38B > UI_PATH_MAX——长度按字节计
                "ui-stage-icon-name-too-long",
                ui_stage("s8", ui_mf, [("a" * 28 + ".png", tiny_png)], []),
            ),
        ]:
            try:
                build(os.path.join(td, "APPUI.bap"), stage_d)
            except Reject:
                print(f"  selftest 拒（预期） : {label}")
                continue
            print(f"  selftest FAIL: {label} 被门禁放行")
            bad += 1
    if bad:
        print(f"mkapp selftest: FAIL x{bad}")
        return 1
    print("mkapp selftest: ok")
    return 0


def main() -> int:
    args = sys.argv[1:]
    # P1 签名面旗标提取（--sign-key/--verify-key 可前缀可夹带；无旗标时
    # rest == 原 args，行为与旧版逐字节同——签名面可选，向后兼容）。
    sign_key: str | None = None
    verify_key: str | None = None
    rest: list[str] = []
    i = 0
    while i < len(args):
        a = args[i]
        if a in ("--sign-key", "--verify-key"):
            if i + 1 >= len(args):
                sys.stderr.write(f"mkapp: {a} 缺值\n")
                return 2
            if a == "--sign-key":
                sign_key = args[i + 1]
            else:
                verify_key = args[i + 1]
            i += 2
            continue
        rest.append(a)
        i += 1
    args = rest
    if args == ["--selftest"]:
        if sign_key or verify_key:
            sys.stderr.write("mkapp: --selftest 不收签名面旗标\n")
            return 2
        return selftest()
    if len(args) == 2 and args[0] == "--check":
        if sign_key:
            sys.stderr.write("mkapp: --sign-key 只配打包形态（<out.bap> <app-dir>）\n")
            return 2
        try:
            check_bap(args[1])
            if verify_key:
                print(_verify_bap_sig(verify_key, args[1]))
        except Reject as e:
            sys.stderr.write(f"mkapp: {e}\n")
            return 1
        return 0
    if len(args) == 4 and args[0] == "--new" and args[2] == "--id":
        if sign_key or verify_key:
            sys.stderr.write("mkapp: --new 不收签名面旗标\n")
            return 2
        try:
            do_new(args[1], args[3])
        except Reject as e:
            sys.stderr.write(f"mkapp: {e}\n")
            return 1
        return 0
    if len(args) == 2 and not args[0].startswith("-"):
        if verify_key:
            sys.stderr.write("mkapp: --verify-key 只配 --check 形态\n")
            return 2
        try:
            build(args[0], args[1])
            if sign_key:
                print(_sign_bap(sign_key, args[0]))
        except Reject as e:
            sys.stderr.write(f"mkapp: {e}\n")
            return 1
        return 0
    sys.stderr.write(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
