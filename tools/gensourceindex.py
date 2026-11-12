#!/usr/bin/env python3
"""gensourceindex——官方源静态实例生成器（SDK-2 P2 工具面；24 号 §4.4
「镜像内置静态源」件一；设计母法 = docs/SDK-2-分发与签名链设计-v1.2.md
§五 :84–101——v1 = 镜像内置官方源静态实例：源元数据（源钥 + 签名索引）
随镜像打包，首触信任不走网络 §二 :46–47）。

用法：
  gensourceindex.py --rootfs-manifest <toml> [--user-dir <dir>] --out-dir <dir>
                    --source-pk <pk-file> [--source-id <id>]
                    [--index-version N] [--prev <source-index.tlv>]
                    [--sign-key <sk-file>]
                                      # rootfs app 条目表 → 两件套：
                                      #   <out>/source-index.tlv（条目八字段
                                      #   索引，TLV 单解释器）+
                                      #   <out>/source-keys.txt（源钥清单）
                                      #   --sign-key 在席 = 索引真签（64B
                                      #   ed25519 裸签名 → flags.signed=1）；
                                      #   缺席 = unsigned 诚实降级（兼容）
  gensourceindex.py --preset <product.toml> [--rootfs-manifest <toml>]
                    [--user-dir <dir>] --out-dir <dir> --source-pk <pk-file>
                                      # G16 双源对账件（38 号 §4：同一 app
                                      #   集合 OEM 三写漂移的收口）：app 条目
                                      #   直接派生自 product.toml preset
                                      #   （berry_assemble load_product +
                                      #   resolve_entries 复用——override 路径
                                      #   替换同语义，零第二解析器），消
                                      #   「源索引条目表」自持面；基座表仍
                                      #   经 --rootfs-manifest 供路径解析
  gensourceindex.py --verify <source-index.tlv> [--prev <source-index.tlv>]
                                      # 结构/八字段/单调/签名验证（signed =
                                      #   berry-sign verify-raw 真验签；同
                                      #   目录 source-keys.txt 在席则两件套
                                      #   对拍）
  gensourceindex.py --gen-keys <sk-out> <pk-out>
                                      # 透传 berry-sign keygen（dev 密钥对；
                                      #   签发面纪律 = berry-sign main.rs
                                      #   头注「dev/test 面纪律」——测试钥
                                      #   永不真签）
  gensourceindex.py --selftest       # 零构建依赖自检（合成基准全谱；
                                      #   berry-sign 在席加跑真签名腿）

条目八字段（v1.2 §五 :88–91——六→八字段，ADR #82 V5 预埋）：
  id（≤24B，berry-manifest ID_MAX 同源）· 三值版本（major/minor/patch
  各 u16——mkapp VER_RE 值域 0..=999；比 manifest「≤16B 自由串」窄：
  索引水位比较〔freshness ver_max 防回滚，§五 :91/:74–75〕需要可排序
  数值，自由串进索引 = 未来水位面不可比——**推断**，manifest 面不变）·
  size（u64）· 哈希（32B = SHA-512/256(.bap 全字节)——与 BSIG bap_digest
  同算法〔envelope.rs DIGEST_LEN/sha512::digest256〕，链内单摘要算法）·
  key_lineage（1..=16B [a-z0-9-]，mkapp SEC_RE 同源）· 公钥见证
  （0/32B：.bap.sig 信封 pk——无 .sig = 0B 无见证）· last_revision
  （u32 = 见证时平台 level——tools/abi.toml [platform].level 单源；逐条目
  粒度，mkimage manifest v1 PR 列同先例〔mkimage.py :190–195 逐条目论证〕）·
  cert_status（v1 恒 "active"——v1.2 §二 :42–43「v1 值恒空/active」的
  索引侧落体；词表 add-only，未知值保守拒）。

source-index.tlv v1 布局（**推断**——v1.2 §五只裁「复用 TLV 单解释器
纪律」未定字节形；本文件是第一版实现，Q3/#77 重议窗内（P3 合入前冻结，
24 号 §4.2-2）改版成本内野为零；BSIG 信封〔envelope.rs :8–22 字段表〕
同族风格——magic/ver/alg/flags 头 + 变长条目 + 尾 64B 签名槽）：

  偏移  字段         型/长       说明
  0     magic        "BSRC" 4B   容器识别（BSIG 先例）
  4     ver          u8 = 1      索引格式版本（Q3 重议轨）
  5     alg_id       u8 = 1      1 = ed25519 + SHA-512/256（与信封同表 add-only）
  6     flags        u8          bit0 = signed（尾签名在席）；bit1..=7 保留必 0
  7     reserved     u8 = 0      前向位（保留必 0——未知值保守拒）
  8     index_ver    u32 BE      索引级单调版本号（防回滚/防冻结，§五 :91）
  12    entry_count  u16 BE      条目数（帽 ENTRIES_MAX）
  14    source_pk    32B         源钥自述（信封 pk 字段同构——签名者身份随件）
  46    entries[…]   变长        条目序列（id 字节序升序——canonical 单形）
  尾    src_sig      64B         源钥签名槽（flags.signed=0 时恒全零）

  条目：id_len u8 + id ‖ 三值版本 3×u16 BE ‖ size u64 BE ‖ digest 32B ‖
  lineage_len u8 + lineage ‖ pk_len u8(0|32) + pk ‖ last_revision u32 BE ‖
  cert_len u8 + cert。

  签名覆盖域 = "BSRC-TOP" ‖ src_sig 前全字节（BSIG-TOP 域分隔标签同
  精神，envelope.rs :75–76）。实装法（本轮 #77 索引签名刀）：先构签名
  预置形（flags.signed=1 + 尾 64B 零槽——build_index(src_sig=64 零字节)）
  → 对 SRC_LABEL ‖ 槽前全字节签名 → 零槽换真签名——终件「槽前全字节」
  即被签字节，验证侧零改字节重组覆盖域（无「翻位再签」的重组歧义）。
  **索引签名实装**（ADR #77 Q3 窗内·P3 前落地的兑现）：--sign-key 在席
  → 单点缝 sign_index() 调 berry-sign `sign-raw`（64B ed25519 裸签名，
  **独立合同**——berry-sign `sign` 的 BSIG 信封是 cpio 载荷语义合同，
  与本槽 64B 裸签名是两份合同不互换；SDK-2 §五 :88「源钥签索引」原义）
  → flags.signed=1 + 源钥清单 sig_state=signed；--verify 侧真验签
  （verify_index_signature × berry-sign verify-raw，源钥 = 索引
  source_pk 自述 + 两件套对拍）；无 --sign-key = 现状面零动（签名槽零
  占位、flags.signed=0、sig_state=unsigned、stdout 诚实声明——向后
  兼容，v1 dev 链 unsigned 索引可过 smoke；设备侧对 unsigned 索引的
  语义 = 设计 §五-Q6 开放问题，随 P3 裁）。

回降拒（v1.2 §九验收线 2「索引版本号回降拒」）：check_index_version =
berry-crypto ledger.rs check_gen（:319–325「new < seen 拒；等值过〔幂等
面，tests :1015〕」）的 Python 侧镜像——单源在 Rust，本函数口径同步义务。

输入面（任务书「rootfs app 条目表(rootfs-manifest.toml 或 mkimage
manifest)」如实核）：app 条目表单源 = rootfs-manifest.toml（V13 数据化
接口，`from mkcpio import load_manifest` 复用——berry_assemble 同纪律
禁第二加载器）；mkimage manifest.txt（件 3）现列镜像 FAT 四件套
（mkimage.py :288–291 EFI/hello/kernel/initrd），**不含 rootfs app 条目**
——该支线核为空集，索引输入不取自彼。索引条目 = 表内 `app/*.bap` 条目
（`selftest/*.bap` = 测试夹具不进官方源索引——如实注）；path 相对
user/ 目录（表头注契约 rootfs-manifest.toml :10–12）。

G16 单源派生（--preset，38 号 §4 双源漂移收口）：演练实咬 = 同一 5-app
集合三写（product.toml preset / 源索引条目表 acme-apps.toml / rootfs
条目表——后者已收敛一处，gensourceindex 侧仍独立自持）——本模式把
源索引条目**直接派生自 product.toml**（preset 集 ⊕ override 路径替换，
经 berry_assemble load_product + resolve_entries 全量复验与解析，
零第二产品解析器；berry_assemble 亦惰性 import 本文件——双向函数级
导入无模块环），「源索引条目表」自持面就此消解；拒面 = berry_assemble
Reject 全谱转 die（产品清单坏/条目不在基座/构建树缺席等，文案原样透传）。

两件套消费链：本工具产 <out-dir>/{source-index.tlv,source-keys.txt} →
mkimage --source-instance <dir> 挂入镜像 FAT /SOURCE/（路径推断——v1.2
§五无路径/卷名定则，标注随 mkimage；两文件名 = 本处常量单源，mkimage
镜像同步义务、selftest 互检绊线）。

确定性：无时刻输入、同输入同字节（条目 id 字节序、清单定宽列、冻结行
格式——mkimage manifest 同纪律）。依赖：Python ≥3.11（tomllib）零三方
依赖；SHA-512/256 = hashlib OpenSSL 面（sha512_256 不可得 = loud 拒）。
"""

import hashlib
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))            # .../tools
REPO = os.path.dirname(ROOT)
DEFAULT_MANIFEST = os.path.join(ROOT, "rootfs-manifest.toml")  # V13 基座表
DEFAULT_USER_DIR = os.path.join(REPO, "user")                 # 表 path 相对基准

# ---- 格式常量（布局推断标注见模块头注；值域常量注明同源锚）----
MAGIC = b"BSRC"                 # 容器识别（BSIG 先例——envelope.rs :51）
VER_V1 = 1                      # 索引格式版本（Q3 重议轨）
ALG_ED25519_SHA512_256 = 1      # 与信封同表（envelope.rs :56 add-only）
FLAG_SIGNED = 1 << 0            # 尾签名槽在席（诚实降级位——unsigned 可见）
FLAGS_KNOWN = FLAG_SIGNED       # bit1..=7 保留必 0（未知位保守拒）
PK_LEN = 32                     # ed25519 公钥长（envelope.rs PK_LEN 同源）
SIG_LEN = 64                    # ed25519 签名长（envelope.rs SIG_LEN 同源）
DIGEST_LEN = 32                 # SHA-512/256 摘要长（同上）
ID_MAX = 24                     # berry-manifest ID_MAX（lib.rs :136 同源）
LINEAGE_MAX = 16                # mkapp SEC_RE 帽（berry_assemble VENDOR_MAX 同锚）
VER_SEG_MAX = 999               # 三值版本段域（mkapp VER_RE 0..=999 同源）
ENTRIES_MAX = 64                # 条目数帽（**推断**——24 号未设帽；APP_SLOTS=9
                                # 基座 + OEM 三包预装余量上取整）
CERT_ACTIVE = "active"          # v1 恒值（v1.2 §二 :42–43；词表 add-only）
CERT_MAX = 16                   # cert_status 长帽（**推断**——lineage 同量级）
SRC_LABEL = b"BSRC-TOP"         # 签名覆盖域分隔标签（BSIG-TOP 同精神，推断）
INDEX_MIN = 46 + SIG_LEN        # 头 46B + 空条目 + 签名槽 = 110B
INDEX_MAX = 16384               # 源字节帽（**推断**——64×142B 条目 + 头尾余量）

# 两件套文件名（mkimage --source-instance 消费合同单源；同步义务注记
# 于 mkimage.py SRCINST_ 段 + 双侧 selftest 互检绊线）
INDEX_NAME = "source-index.tlv"
KEYS_NAME = "source-keys.txt"
# 源清单冻结行格式（v1 冻结于 2026-11-11——改列宽/行序/字段 = 破冻须升 v2，
# mkimage manifest.txt 同纪律）
KEYS_FROZEN = "2026-11-11"
KEYS_HEAD = f"berryos-source-keys v1 format_frozen={KEYS_FROZEN}"
SID_W = 16                      # source_id 列宽（lineage 帽同宽——列宽即契约）

LINEAGE_RE = re.compile(r"[a-z0-9-]{1,16}")   # mkapp SEC_RE 同款（同步义务）
ID_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,23}")  # berry-manifest id 域
                                              # （lib.rs :706–708：1..=24、首字符
                                              #  字母数字、[A-Za-z0-9._-]）


def die(msg: str) -> None:
    raise SystemExit(f"gensourceindex: FAIL: {msg}")


def sha512_256(data: bytes) -> bytes:
    """SHA-512/256 摘要（= BSIG 链 bap_digest 同算法——envelope.rs :68/
    sha512::digest256；hashlib OpenSSL 面，不可得 loud 拒）。"""
    try:
        return hashlib.new("sha512_256", data).digest()
    except ValueError:
        die("SHA-512/256（hashlib 'sha512_256'）不可得——需 OpenSSL 支撑面"
            "（链内单摘要算法，不降级到第二种哈希）")


def platform_level() -> int:
    """平台 API level 单源现值：tools/abi.toml `[platform].level`（mkimage
    platform_level/berry_assemble _platform_level 同源同法——数据单源在
    abi.toml，各工具自携小读器，gen_abi/mkcpio 既有先例）。last_revision
    列的唯一取值源；坏源一律 loud。"""
    import tomllib  # 函数内导入（mkimage platform_level 同纪律）
    path = os.path.join(ROOT, "abi.toml")
    try:
        with open(path, "rb") as f:
            level = tomllib.load(f).get("platform", {}).get("level")
    except OSError as e:
        die(f"读 {path}：{e}")
    except tomllib.TOMLDecodeError as e:
        die(f"坏 TOML {path}：{e}")
    if not isinstance(level, int) or isinstance(level, bool):
        die(f"tools/abi.toml [platform].level 非整数：{level!r}"
            "（last_revision 单源）")
    return level


def check_index_version(new: int, seen: int) -> None:
    """索引版本单调门（回降拒）——berry-crypto ledger.rs `check_gen`
    （:319–325）的口径镜像：new < seen = 拒（回滚/回放）；等值过（同代
    幂等面，ledger tests :1015）；升过。口径同步义务：Rust 侧改义本函数
    随笔。"""
    if new < seen:
        die(f"索引版本回降拒：index_version {new} < 已见水位 {seen}"
            "（v1.2 §五 :91 索引级单调版本号防回滚——check_gen 同口径）")


# ---- .bap 对读面（单源复用，不长第二套）----

def bap_manifest(bap: bytes) -> bytes:
    """.bap 全字节 → manifest.toml 字节（cpio 走查单源 = mkapp
    `_cpio_entries`——宿主侧唯一解码器〔mkapp.py :1053–1059〕，本工具
    import 复用；berry-sign main.rs cpio_manifest 为 Rust 侧对读面先例）。"""
    from mkapp import Reject, _cpio_entries  # 惰性导入（mkapp 活文件解耦）
    try:
        entries = _cpio_entries(bap)
    except Reject as e:
        die(f"bap cpio 走查拒：{e}")
    for name, data in entries:
        if name == b"manifest.toml":
            return data
    die("bap 缺 manifest.toml 条目（.bap 形制——mkapp 产物面）")


def manifest_fields(mf: bytes) -> tuple:
    """manifest.toml → (id, version_str, key_lineage)。语义单源 = mkapp
    `check_manifest_text`（berry-manifest 解析的 host 面——不出现第二个
    manifest 语义判断）；[security] 三键必在场（key_lineage 是索引身份
    半边——无 [security] 的 .bap 不进官方源索引，loud 拒）。"""
    from mkapp import Reject, check_manifest_text
    try:
        m = check_manifest_text(mf.decode("ascii", "strict"))
    except Reject as e:
        die(f"manifest 解析拒：{e}")
    except UnicodeDecodeError:
        die("manifest 非 ASCII（berry-manifest 值域外）")
    if m.sec is None:
        die(f"app {m.id!r} 无 [security] 三键（ADR #82 V3——key_lineage 是"
            "索引身份半边，未签发 app 不进官方源索引；mkapp --sign-key 先签）")
    return m.id, m.version, m.sec[1]


def ver_triple(version_str: str) -> tuple:
    """三值版本串 → (major, minor, patch) int 三元组。文法单源 = mkapp
    `VER_RE`（0..=999 无前导零——SDK-1a §4.1）；[app].version 在
    berry-manifest 是 ≤16B 自由串，索引面收窄为三值文法（头注推断注：
    水位比较需要可排序数值）。"""
    from mkapp import VER_RE
    mo = VER_RE.fullmatch(version_str)
    if not mo:
        die(f"app 版本 {version_str!r} 非三值文法 x.y.z（mkapp VER_RE 域——"
            "索引面比 manifest 自由串收窄，水位比较前置）")
    return tuple(int(g) for g in mo.groups())


def envelope_pk(sig: bytes, lineage: bytes) -> bytes:
    """.sig 信封 → pk 见证提取（对读面，非第二解析器：只按 envelope.rs
    :8–22 字段表取 lineage_len〔偏移 7〕与 pk〔偏移 8+L+4 起 32B〕两域 +
    lineage 回声对账；信封完整验签单源 = berry-sign verify）。回声不等 =
    loud 拒（索引不得见证自相矛盾的 bap/sig 对）。"""
    if len(sig) < 8 + 1 + 4 + PK_LEN or sig[0:4] != b"BSIG" or sig[4] != 1:
        die(".sig 非信封 v1（BSIG 头不符——envelope.rs 字段表对读）")
    lin_len = sig[7]
    if not 1 <= lin_len <= LINEAGE_MAX:
        die(f".sig lineage_len {lin_len} 越域（1..=16）")
    off = 8 + lin_len + 4
    if off + PK_LEN > len(sig):
        die(".sig 短于 pk 字段（信封截断？）")
    if bytes(sig[8:8 + lin_len]) != lineage:
        die(f".sig lineage 回声不符：{bytes(sig[8:8+lin_len])!r} != manifest "
            f"{lineage!r}（跨 lineage 对——berry-sign verify 同拒面）")
    return bytes(sig[off:off + PK_LEN])


# ---- berry-sign 发现与调用（mkapp.py :1234–1247 同步义务镜像）----

def find_berry_sign() -> str | None:
    """berry-sign 二进制发现：env BERRY_SIGN → target-cargo 与
    /tmp/target-p1、/tmp/target-p2 三 CARGO_TARGET_DIR 惯用位 ×
    debug/release（mkapp `_find_berry_sign` 同序——同步义务）。"""
    env = os.environ.get("BERRY_SIGN")
    if env and os.path.isfile(env) and os.access(env, os.X_OK):
        return env
    for tdir in (os.path.join(REPO, "target-cargo"), "/tmp/target-p1",
                 "/tmp/target-p2"):
        for profile in ("debug", "release"):
            cand = os.path.join(tdir, "aarch64-apple-darwin", profile,
                                "berry-sign")
            if os.path.isfile(cand) and os.access(cand, os.X_OK):
                return cand
    return None


def run_berry_sign(argv: list):
    """berry-sign 子进程调用（--gen-keys/.sig 加固验/sign_index 真签/
    verify_index_signature 真验共用）；不可得 = None（调用方如实注或
    loud——显式签名面（--sign-key）不静默，加固面不硬依）。"""
    bs = find_berry_sign()
    if bs is None:
        return None
    try:
        return subprocess.run([bs, *argv], capture_output=True, text=True)
    except OSError:
        return None


def sign_index(index_form: bytes, sk_path: str | None) -> tuple:
    """索引签名单点缝（#77/Q3 窗内实装——P2 占位的兑现位，语义不变只翻
    真签）。index_form = 签名预置形（flags.signed=1 + 尾 64B 零槽——
    build_index(src_sig=b"\\x00"*SIG_LEN) 产物）；覆盖域 = SRC_LABEL ‖
    槽前全字节（模块头注布局节）。返回 (sig_64B | None, 如实注)：
    - sk_path 缺席 → (None, 如实注)——unsigned 诚实降级（向后兼容，
      调用方不换槽、清单 sig_state=unsigned）；
    - sk_path 在席 → berry-sign `sign-raw`（64B ed25519 裸签名——独立
      于 .bap 信封合同，模块头注「索引签名实装」节）；berry-sign 不可得
      /子进程拒 = loud die（显式签名面不静默降级——mkapp sign 同纪律）。
    """
    if sk_path is None:
        return None, ("--sign-key 缺席——索引未签（签名槽零占位 + "
                      "flags.signed=0 + 清单 sig_state=unsigned，向后兼容"
                      "现状面；签名面 = sign_index() × berry-sign sign-raw）")
    cover = SRC_LABEL + index_form[:-SIG_LEN]
    import tempfile
    with tempfile.TemporaryDirectory(prefix="gsi-sign-") as td:
        cover_p = os.path.join(td, "cover.bin")
        sig_p = os.path.join(td, "src.sig")
        with open(cover_p, "wb") as f:
            f.write(cover)
        r = run_berry_sign(["sign-raw", sk_path, cover_p, "--out", sig_p])
        if r is None:
            die("berry-sign 不可得（设 BERRY_SIGN 或先 CARGO_TARGET_DIR="
                "/tmp/target-p2 cargo build --manifest-path user/Cargo.toml "
                "-p berry-crypto --bin berry-sign --target "
                "aarch64-apple-darwin——do_gen_keys 同文案）——--sign-key "
                "显式签名面不静默降级（mkapp sign.tool-missing 同纪律）")
        if r.returncode != 0:
            die(f"索引签名拒：{(r.stderr or r.stdout).strip()}")
        with open(sig_p, "rb") as f:
            sig = f.read()
    if len(sig) != SIG_LEN:
        die(f"sign-raw 产物非 {SIG_LEN}B（got {len(sig)}B——裸签名独立合同面）")
    return sig, (f"berry-sign sign-raw 64B ed25519 裸签名（覆盖域 "
                 f"{SRC_LABEL!r} ‖ 槽前 {len(cover) - len(SRC_LABEL)}B；"
                 "独立于 .bap 信封合同）")


def verify_index_signature(raw: bytes, source_pk: bytes, src_sig: bytes):
    """索引签名真验（--verify 消费面；selftest 复用）。覆盖域重建 =
    SRC_LABEL ‖ 槽前全字节 → berry-sign `verify-raw`（pk = 索引
    source_pk 自述——两件套对拍另执法清单 pk 一致）。返回 True / False /
    None：None = berry-sign 不可得（如实注跳过——零构建依赖纪律，结构面
    已由 parse_index 全量执法；False = 篡改/换钥，调用方 loud 拒）。"""
    import tempfile
    with tempfile.TemporaryDirectory(prefix="gsi-verify-") as td:
        pkf = os.path.join(td, "pk.bin")
        cover_p = os.path.join(td, "cover.bin")
        sig_p = os.path.join(td, "sig.bin")
        for path, data in ((pkf, source_pk),
                           (cover_p, SRC_LABEL + raw[:-SIG_LEN]),
                           (sig_p, src_sig)):
            with open(path, "wb") as f:
                f.write(data)
        r = run_berry_sign(["verify-raw", pkf, cover_p, sig_p])
    if r is None:
        return None
    return r.returncode == 0


# ---- 索引构造/解析（TLV 单解释器——本文件是唯一编码/解码点）----

class Entry:
    """条目八字段视图（id/三值版本/size/哈希/lineage/pk 见证/last_rev/cert）。"""

    def __init__(self, eid, ver, size, digest, lineage, pk, last_rev,
                 cert=CERT_ACTIVE):
        self.id = eid              # str（≤24B ID_RE 域）
        self.ver = ver             # (major, minor, patch) 各 0..=999
        self.size = size           # u64
        self.digest = digest       # bytes 32B
        self.lineage = lineage     # str（1..=16B LINEAGE_RE 域）
        self.pk = pk               # bytes 32B 或 None（无见证）
        self.last_rev = last_rev   # u32（平台 level）
        self.cert = cert           # str（v1 恒 "active"）

    def key(self):
        return self.id.encode("ascii")

    def encode(self) -> bytes:
        out = bytearray()
        ib = self.id.encode("ascii")
        out.append(len(ib))
        out += ib
        for seg in self.ver:
            out += seg.to_bytes(2, "big")
        out += self.size.to_bytes(8, "big")
        out += self.digest
        lb = self.lineage.encode("ascii")
        out.append(len(lb))
        out += lb
        out.append(PK_LEN if self.pk else 0)
        if self.pk:
            out += self.pk
        out += self.last_rev.to_bytes(4, "big")
        cb = self.cert.encode("ascii")
        out.append(len(cb))
        out += cb
        return bytes(out)


class Index:
    """索引视图（parse 产物——字段与布局一一对应）。"""

    def __init__(self, index_ver, source_pk, entries, signed, src_sig):
        self.index_ver = index_ver  # u32
        self.source_pk = source_pk  # bytes 32B
        self.entries = entries      # list[Entry]（id 升序）
        self.signed = signed        # bool（flags.bit0）
        self.src_sig = src_sig      # bytes 64B（unsigned = 全零）


def _check_common(entry: Entry, what: str) -> None:
    """构造/解析共域执法（值域单点——encode/parse 同口径）。"""
    if not isinstance(entry.id, str) or not ID_RE.fullmatch(entry.id):
        die(f"{what}: id {entry.id!r} 越域（1..=24B、首字符字母数字、"
            "[A-Za-z0-9._-]——berry-manifest ID 域同源）")
    if not (isinstance(entry.ver, tuple) and len(entry.ver) == 3
            and all(isinstance(v, int) and 0 <= v <= VER_SEG_MAX
                    for v in entry.ver)):
        die(f"{what}: 三值版本 {entry.ver!r} 越域（各 u16 且 ≤{VER_SEG_MAX}"
            "——mkapp VER_RE 同源）")
    if not 0 <= entry.size <= 2**64 - 1:
        die(f"{what}: size 越域（u64）")
    if not isinstance(entry.digest, bytes) or len(entry.digest) != DIGEST_LEN:
        die(f"{what}: 哈希非 {DIGEST_LEN}B（SHA-512/256）")
    if not isinstance(entry.lineage, str) or not LINEAGE_RE.fullmatch(
            entry.lineage):
        die(f"{what}: key_lineage {entry.lineage!r} 越域（1..=16B [a-z0-9-]"
            "——mkapp SEC_RE 同源）")
    if entry.pk is not None and (not isinstance(entry.pk, bytes)
                                 or len(entry.pk) != PK_LEN):
        die(f"{what}: 公钥见证非 0/32B")
    if not 0 <= entry.last_rev <= 2**32 - 1:
        die(f"{what}: last_revision 越域（u32）")
    if not isinstance(entry.cert, str) or not 1 <= len(entry.cert) <= CERT_MAX:
        die(f"{what}: cert_status 长越域（1..={CERT_MAX}）")
    try:
        entry.cert.encode("ascii", "strict")
    except UnicodeEncodeError:
        die(f"{what}: cert_status 非 ASCII")


def build_index(entries, index_ver, source_pk, src_sig=None) -> bytes:
    """条目表 → 索引字节（canonical 单形：id 字节序升序 + 查重 + 全域执法）。
    src_sig = None → 签名槽零占位（flags.signed=0）；64B → 在席。"""
    if not isinstance(source_pk, bytes) or len(source_pk) != PK_LEN:
        die(f"source_pk 非 {PK_LEN}B")
    if not 0 <= index_ver <= 2**32 - 1:
        die(f"index_version 越域（u32）：{index_ver}")
    if len(entries) > ENTRIES_MAX:
        die(f"条目数 {len(entries)} 越帽 {ENTRIES_MAX}（推断帽——基座 9 +"
            " OEM 三包余量）")
    ids = sorted(e.id for e in entries)
    dups = sorted({i for i in ids if ids.count(i) > 1})
    if dups:
        die(f"条目 id 重复 {dups}（索引身份单义——查重）")
    for e in entries:
        _check_common(e, f"条目 {e.id!r}")
    out = bytearray()
    out += MAGIC
    out.append(VER_V1)
    out.append(ALG_ED25519_SHA512_256)
    out.append(FLAG_SIGNED if src_sig else 0)
    out.append(0)                       # reserved
    out += index_ver.to_bytes(4, "big")
    out += len(entries).to_bytes(2, "big")
    out += source_pk
    for e in sorted(entries, key=lambda x: x.key()):
        out += e.encode()
    if src_sig:
        if len(src_sig) != SIG_LEN:
            die(f"签名槽非 {SIG_LEN}B")
        out += src_sig
    else:
        out += b"\x00" * SIG_LEN
    if len(out) > INDEX_MAX:
        die(f"索引 {len(out)}B 越帽 {INDEX_MAX}（推断帽——防敌意巨值）")
    return bytes(out)


def parse_index(data: bytes) -> Index:
    """单一解释解析：结构/值域/canonical 序/flags-slot 一致性全量执法
    （永不静默截断；未知 ver/alg/flags/reserved 保守拒——envelope.rs
    parse 同纪律）。"""
    if len(data) < INDEX_MIN:
        die(f"索引短于最小 {INDEX_MIN}B（got {len(data)}）")
    if data[0:4] != MAGIC:
        die(f"magic 不符 {data[0:4]!r}（应 {MAGIC!r}）")
    if data[4] != VER_V1:
        die(f"ver {data[4]} ≠ 1（未知版本保守拒——升版走 add-only 轨）")
    if data[5] != ALG_ED25519_SHA512_256:
        die(f"alg_id {data[5]} 未在册（1 = ed25519+SHA-512/256，add-only）")
    flags = data[6]
    if flags & ~FLAGS_KNOWN:
        die(f"flags 保留位非 0：0x{flags:02x}（未知位保守拒）")
    signed = bool(flags & FLAG_SIGNED)
    if data[7] != 0:
        die(f"reserved {data[7]} ≠ 0（保守拒）")
    index_ver = int.from_bytes(data[8:12], "big")
    count = int.from_bytes(data[12:14], "big")
    if count > ENTRIES_MAX:
        die(f"entry_count {count} 越帽 {ENTRIES_MAX}")
    source_pk = bytes(data[14:46])
    off = 46
    entries = []
    for i in range(count):
        try:
            id_len = data[off]
            off += 1
            eid = data[off:off + id_len].decode("ascii", "strict")
            off += id_len
            ver = tuple(int.from_bytes(data[off + k * 2:off + k * 2 + 2], "big")
                        for k in range(3))
            off += 6
            size = int.from_bytes(data[off:off + 8], "big")
            off += 8
            digest = bytes(data[off:off + DIGEST_LEN])
            off += DIGEST_LEN
            lin_len = data[off]
            off += 1
            lineage = data[off:off + lin_len].decode("ascii", "strict")
            off += lin_len
            pk_len = data[off]
            off += 1
            if pk_len not in (0, PK_LEN):
                die(f"条目#{i} pk_len {pk_len} 非 0/32（单一解释——无见证/"
                    "见证两态外无第三读）")
            pk = bytes(data[off:off + pk_len]) if pk_len else None
            off += pk_len
            last_rev = int.from_bytes(data[off:off + 4], "big")
            off += 4
            cert_len = data[off]
            off += 1
            cert = data[off:off + cert_len].decode("ascii", "strict")
            off += cert_len
        except IndexError:
            die(f"条目#{i} 越界（索引截断——精确总长纪律）")
        except UnicodeDecodeError as e:
            die(f"条目#{i} 非 ASCII 字段：{e}")
        e = Entry(eid, ver, size, digest, lineage, pk, last_rev, cert)
        _check_common(e, f"条目#{i}")
        if e.cert != CERT_ACTIVE:
            die(f"条目#{i} cert_status {e.cert!r} 未在册（v1 词表 = "
                "{'active'}——add-only，未知值保守拒）")
        entries.append(e)
    if off + SIG_LEN != len(data):
        die(f"总长不符：条目尾 {off} + 槽 {SIG_LEN} != {len(data)}"
            "（无尾随字节容差——单一解释纪律）")
    src_sig = bytes(data[off:off + SIG_LEN])
    if signed and src_sig == b"\x00" * SIG_LEN:
        die("flags.signed=1 但签名槽全零（谎报位——拒）")
    if not signed and src_sig != b"\x00" * SIG_LEN:
        die("flags.signed=0 但签名槽非零（未签索引携带签名字节——单一"
            "解释拒；真签名走 --sign-key → flags.signed=1 同步翻位）")
    keys = [e.key() for e in entries]
    if keys != sorted(keys) or len(set(keys)) != len(keys):
        die("条目非 id 升序或重复（canonical 单形——build 对拍前提）")
    return Index(index_ver, source_pk, entries, signed, src_sig)


# ---- 源钥清单（冻结行格式 v1）----

def build_keys_listing(source_id: str, pk: bytes, signed: bool) -> bytes:
    """源钥清单字节（v1 冻结行格式——改列宽/行序/字段 = 破冻升 v2）：
    行 1 头 `berryos-source-keys v1 format_frozen=<日> sources=NN`；
    行 2 `key source_id=<16 左对齐> pk=<64hex> fingerprint=<64hex>
    sig_state=<signed|unsigned>`。fingerprint = SHA-512/256(pk)（§五 :98
    per-source 指纹的 P2 静态半——代际/pending 归 P3 账本）；sig_state =
    诚实降级位（unsigned 可见——§九 :135 同精神）。ASCII、LF、末行换行。"""
    if not LINEAGE_RE.fullmatch(source_id):
        die(f"source_id {source_id!r} 越域（1..=16B [a-z0-9-]——lineage "
            "同域口径）")
    if not isinstance(pk, bytes) or len(pk) != PK_LEN:
        die(f"pk 非 {PK_LEN}B")
    state = "signed" if signed else "unsigned"
    lines = [f"{KEYS_HEAD} sources=01",
             f"key source_id={source_id:<{SID_W}} pk={pk.hex()} "
             f"fingerprint={sha512_256(pk).hex()} sig_state={state}"]
    return ("\n".join(lines) + "\n").encode("ascii")


def parse_keys_listing(data: bytes) -> tuple:
    """清单回读（两件套对拍用）：→ (source_id, pk 32B, signed: bool)。
    冻结格式逐列执法（定宽正则整行钉——头行/列序/列宽/词表/小写 hex/
    fingerprint，漂移即拒；source_id 定宽 16 含右垫空格）。"""
    try:
        text = data.decode("ascii")
    except UnicodeDecodeError:
        die("source-keys 非 ASCII")
    lines = text.splitlines()
    if len(lines) != 2:
        die(f"source-keys 行数 {len(lines)} ≠ 2（v1 单源清单形）")
    if lines[0] != f"{KEYS_HEAD} sources=01":
        die(f"source-keys 头行漂移：{lines[0]!r}（v1 冻结格式）")
    mo = re.fullmatch(
        r"key source_id=(.{16}) pk=([0-9a-f]{64}) "
        r"fingerprint=([0-9a-f]{64}) sig_state=(signed|unsigned)", lines[1])
    if not mo:
        die(f"source-keys 条目行形漂移：{lines[1]!r}（冻结列序/列宽/词表）")
    sid = mo.group(1).rstrip(" ")
    if not sid or not LINEAGE_RE.fullmatch(sid):
        die(f"source_id 列漂移：{mo.group(1)!r}（{SID_W} 定宽 + lineage 值域）")
    pk = bytes.fromhex(mo.group(2))
    fp = bytes.fromhex(mo.group(3))
    if fp != sha512_256(pk):
        die("source-keys fingerprint 对拍不符（SHA-512/256(pk)）")
    return sid, pk, mo.group(4) == "signed"


# ---- 生成主流程 ----

def load_app_entries(manifest_path: str, user_dir: str) -> list:
    """rootfs-manifest.toml → app/*.bap 条目（mkcpio load_manifest 单
    加载器复用；selftest/* 测试夹具不进官方源索引）。返回 [(name, local)]。"""
    from mkcpio import load_manifest
    try:
        table = load_manifest(manifest_path)
    except SystemExit as e:
        die(f"条目表加载失败：{e}")
    apps = [(n, p) for n, p in sorted(table.items())
            if n.startswith("app/") and n.endswith(".bap")]
    if not apps:
        die(f"{manifest_path} 无 app/*.bap 条目（官方源索引对象缺席——"
            "selftest/* 夹具不计）")
    out = []
    for name, rel in apps:
        local = os.path.join(user_dir, rel)
        if not os.path.isfile(local):
            die(f"rootfs 条目 {name}: 构建树缺席 {local}"
                "（表 path 相对 user/——先 just build-user）")
        out.append((name, local))
    return out


def load_preset_entries(product_path: str, base_manifest: str,
                        user_dir: str) -> list:
    """G16 单源派生（--preset）：product.toml → app/*.bap 条目。

    解析/复验全量复用 berry_assemble（load_product 结构+产物面执法 ⊕
    resolve_entries 终表解析——preset 筛选、override 路径替换、构建树
    存在性、launcher 终表对账一身；零第二产品解析器），本函数只做
    app/ 前缀过滤 + 绝对/相对路径归一。拒面 = berry_assemble Reject
    → die（文案原样透传，前缀 --preset 标注）。返回 [(name, local)]，
    条目集 ≡ 产品终表 app 集（preset ⊕ override——与 rootfs 条目表
    同源，双源自持消解）。"""
    import berry_assemble  # 惰性导入（彼方亦函数级 import 本文件——无环）
    try:
        prod = berry_assemble.load_product(product_path)
        items, _ = berry_assemble.resolve_entries(prod, base_manifest, user_dir)
    except berry_assemble.Reject as e:
        die(f"--preset 产品解析拒：{e}")
    apps = []
    for item in items:
        name, _, path = item.partition("=")
        if name.startswith("app/") and name.endswith(".bap"):
            local = path if os.path.isabs(path) else os.path.join(user_dir, path)
            apps.append((name, local))
    if not apps:
        die(f"{product_path} 终表无 app/*.bap 条目（preset 空——官方源"
            "索引对象缺席）")
    return apps


def build_entry(name: str, local: str, last_rev: int,
                harden: bool = True) -> Entry:
    """单条目装配：.bap 读入 → manifest 单源解析 → .sig 见证提取
    （在席）→ Entry。harden = berry-sign verify 加固（二进制在席则跑——
    顶签名真验；不在席如实注跳过，不硬依）。"""
    try:
        with open(local, "rb") as f:
            bap = f.read()
    except OSError as e:
        die(f"读 {local}：{e}")
    app_id, ver_str, lineage = manifest_fields(bap_manifest(bap))
    ver = ver_triple(ver_str)
    pk = None
    sig_path = local + ".sig"
    if os.path.isfile(sig_path):
        try:
            with open(sig_path, "rb") as f:
                sig = f.read()
        except OSError as e:
            die(f"读 {sig_path}：{e}")
        pk = envelope_pk(sig, lineage.encode("ascii"))
        if harden:
            import tempfile
            with tempfile.NamedTemporaryFile(suffix=".pk", delete=False) as tf:
                tf.write(pk)
                pkf = tf.name
            try:
                r = run_berry_sign(["verify", pkf, local])
            finally:
                os.unlink(pkf)
            if r is None:
                print(f"gensourceindex: 注: {name} .sig 加固验跳过"
                      "（berry-sign 不可得——见证 pk 未过顶签名验，构建期"
                      "加固缺席如实注；完整验签单源 = berry-sign）")
            elif r.returncode != 0:
                die(f"{name}: .sig 顶签名验拒：{(r.stderr or r.stdout).strip()}"
                    "（索引不见证坏 .sig——先重签）")
    return Entry(app_id, ver, len(bap), sha512_256(bap),
                 lineage, pk, last_rev)


def do_generate(args) -> int:
    last_rev = platform_level()
    if getattr(args, "preset", None):   # G16 单源派生：product.toml preset
        pairs = load_preset_entries(args.preset, args.rootfs_manifest,
                                    args.user_dir)
    else:
        pairs = load_app_entries(args.rootfs_manifest, args.user_dir)
    entries = [build_entry(name, local, last_rev) for name, local in pairs]
    try:
        with open(args.source_pk, "rb") as f:
            pk = f.read()
    except OSError as e:
        die(f"读 {args.source_pk}：{e}")
    if len(pk) != PK_LEN:
        die(f"--source-pk 非 {PK_LEN}B（berry-sign keygen 产物）")
    if args.prev:
        try:
            with open(args.prev, "rb") as f:
                check_index_version(args.index_version,
                                    parse_index(f.read()).index_ver)
        except OSError as e:
            die(f"读 {args.prev}：{e}")
    # 索引签名（#77 实装）：--sign-key 在席 = 单点缝 sign_index() 真签
    #（签名预置形 flags.signed=1 + 64B 零槽 → 覆盖域签名 → 换真签名）；
    # 缺席 = unsigned 诚实降级（现状面零动——向后兼容）。
    if args.sign_key:
        form = build_index(entries, args.index_version, pk,
                           src_sig=b"\x00" * SIG_LEN)
        sig, note = sign_index(form, args.sign_key)
        index_bytes = form[:-SIG_LEN] + sig
        signed = True
    else:
        index_bytes = build_index(entries, args.index_version, pk)
        _, note = sign_index(None, None)
        signed = False
    keys_bytes = build_keys_listing(args.source_id, pk, signed=signed)
    os.makedirs(args.out_dir, exist_ok=True)
    ipath = os.path.join(args.out_dir, INDEX_NAME)
    kpath = os.path.join(args.out_dir, KEYS_NAME)
    with open(ipath, "wb") as f:
        f.write(index_bytes)
    with open(kpath, "wb") as f:
        f.write(keys_bytes)
    n_wit = sum(1 for e in entries if e.pk)
    print(f"gensourceindex: {ipath} {len(index_bytes)}B entries={len(entries)}"
          f" index_version={args.index_version} witnessed={n_wit}"
          f" last_revision={last_rev}")
    if getattr(args, "preset", None):
        print(f"gensourceindex: 条目单源派生（G16）: --preset {args.preset}"
              f"（preset ⊕ override = 产品终表 app 集——「源索引条目表」"
              f"自持面消解，38 号 §4）")
    print(f"gensourceindex: {kpath} {len(keys_bytes)}B source_id="
          f"{args.source_id} sig_state={'signed' if signed else 'unsigned'}")
    if signed:
        print(f"gensourceindex: 注: 签名槽在席（flags.signed=1）：{note}")
    else:
        print(f"gensourceindex: 注: 索引未签（如实注）：{note}")
    print("gensourceindex: ok（两件套——mkimage --source-instance 挂接）")
    return 0


def do_verify(args) -> int:
    try:
        with open(args.verify, "rb") as f:
            raw = f.read()
    except OSError as e:
        die(f"读 {args.verify}：{e}")
    idx = parse_index(raw)
    # 八字段齐全性：parse 全量执法后逐条复核在场（验收线 2——id/三值版本/
    # size/哈希/key_lineage/公钥见证/last_revision/cert_status 八字段齐）。
    n8 = sum(1 for e in idx.entries
             if e.id and isinstance(e.ver, tuple) and len(e.ver) == 3
             and isinstance(e.size, int) and len(e.digest) == DIGEST_LEN
             and e.lineage and isinstance(e.last_rev, int) and e.cert)
    n_wit = sum(1 for e in idx.entries if e.pk)
    sig_state = "signed" if idx.signed else "placeholder"
    print(f"gensourceindex: verify {args.verify} OK entries={len(idx.entries)}"
          f" index_version={idx.index_ver} witnessed={n_wit}"
          f" sig={sig_state}")
    print(f"gensourceindex:   八字段 {n8}/{len(idx.entries)} 齐全"
          "（id/三值版本/size/哈希/key_lineage/公钥见证/last_revision"
          "/cert_status）")
    if args.prev:
        try:
            with open(args.prev, "rb") as f:
                prev_ver = parse_index(f.read()).index_ver
        except OSError as e:
            die(f"读 {args.prev}：{e}")
        check_index_version(idx.index_ver, prev_ver)
        print(f"gensourceindex:   单调: prev={prev_ver} → "
              f"{idx.index_ver} 过（无回降——check_gen 同口径）")
    if idx.signed:
        # 真验签（#77 实装）：覆盖域 = SRC_LABEL ‖ 槽前全字节，源钥 =
        # 索引 source_pk 自述；berry-sign 不可得 = 如实注跳过（零构建依赖
        # 纪律——结构面已全量执法），验拒 = loud die。
        vr = verify_index_signature(raw, idx.source_pk, idx.src_sig)
        if vr is True:
            print("gensourceindex:   签名: 真验过（BSRC-TOP 覆盖域 × "
                  "berry-sign verify-raw，源钥 ed25519）")
        elif vr is False:
            die(f"{args.verify}: 签名验拒——BSRC-TOP 覆盖域对不上（索引被"
                "篡改或源钥不符；重签或回查 --sign-key/--source-pk 配对）")
        else:
            print("gensourceindex:   签名: 未验（如实注——berry-sign 不可得"
                  "，结构面已全量执法；验签面 = verify_index_signature）")
    else:
        print("gensourceindex:   签名: 占位如实注——索引未签（无 --sign-key"
              " 产物；向后兼容面——dev 链 unsigned 索引，设备侧语义随 P3 "
              "裁〔设计 §五-Q6〕）")
    kpath = os.path.join(os.path.dirname(os.path.abspath(args.verify)),
                         KEYS_NAME)
    if os.path.isfile(kpath):
        with open(kpath, "rb") as f:
            sid, pk, signed = parse_keys_listing(f.read())
        if pk != idx.source_pk:
            die(f"两件套对拍拒：清单 pk != 索引 source_pk（{kpath}）")
        if signed != idx.signed:
            die(f"两件套对拍拒：sig_state {'signed' if signed else 'unsigned'}"
                f" != 索引 flags.signed={idx.signed}")
        print(f"gensourceindex:   两件套对拍: {KEYS_NAME} source_id={sid}"
              " pk/fingerprint/sig_state 一致")
    return 0


def do_gen_keys(sk_out: str, pk_out: str) -> int:
    """berry-sign keygen 透传（host 面真调用——P1 产物消费半边）。"""
    r = run_berry_sign(["keygen", sk_out, pk_out])
    if r is None:
        die("berry-sign 不可得（设 BERRY_SIGN 或先 CARGO_TARGET_DIR="
            "/tmp/target-p2 cargo build --manifest-path user/Cargo.toml "
            "-p berry-crypto --bin berry-sign --target aarch64-apple-darwin"
            "——mkapp sign.tool-missing 同文案）")
    if r.returncode != 0:
        die(f"berry-sign keygen 拒：{(r.stderr or r.stdout).strip()}")
    print((r.stdout or "").strip())
    print("gensourceindex: 注: dev 密钥对（berry-sign 头注「dev/test 面"
          "纪律」——永不用于真签，真签面随 #77/P3）")
    return 0


# ---- FAT16 只读走查器（对读面——验证专用，非生产解码器）----

def fat16_ls(img: bytes, vol_off: int = 2048 * 512) -> dict:
    """整盘 → {FAT 路径: (size, data)}（mkfat build_volume_puts 产物的
    只读回读对拍面——BPB 驱动：bps/spc/reserved/nfats/root_entries/
    fat_sz16；根目录 + 一层子目录；FAT16 链跟读至 EOF ≥0xFFF8）。
    mkfat 无读侧（「FAT 写法只在 mkfat 一处」），本函数 = 验证专用对读
    （mkapp._cpio_entries「对读 mkcpio 编码几何」同物种——几何同步义务）。"""
    def u16(o):
        return int.from_bytes(img[o:o + 2], "little")

    bps = u16(vol_off + 11)
    spc = img[vol_off + 13]
    reserved = u16(vol_off + 14)
    nfats = img[vol_off + 16]
    root_entries = u16(vol_off + 17)
    fat_sz = u16(vol_off + 22)
    if bps != 512 or spc < 1 or nfats < 1 or fat_sz < 1:
        die(f"fat16_ls: BPB 域非法（bps={bps} spc={spc} nfats={nfats}"
            f" fat_sz={fat_sz}——mkfat 产物面外）")
    fat0 = vol_off + reserved * bps
    root_off = vol_off + (reserved + nfats * fat_sz) * bps
    root_secs = (root_entries * 32 + bps - 1) // bps
    data_sec = reserved + nfats * fat_sz + root_secs

    def read_chain(c, size):
        out = bytearray()
        while 2 <= c < 0xFFF8:
            off = vol_off + (data_sec + (c - 2) * spc) * bps
            out += img[off:off + spc * bps]
            c = int.from_bytes(img[fat0 + 2 * c:fat0 + 2 * c + 2], "little")
        return bytes(out[:size])

    def name_of(raw11):
        base = raw11[0:8].decode("ascii", "strict").rstrip(" ")
        ext = raw11[8:11].decode("ascii", "strict").rstrip(" ")
        return f"{base}.{ext}" if ext else base

    def walk_dir(data, prefix, into):
        for i in range(0, len(data) - 31, 32):
            ent = data[i:i + 32]
            if ent[0] == 0x00:
                break
            if ent[0] == 0xE5:
                continue
            name = name_of(ent[0:11])
            attr = ent[11]
            if name in (".", ".."):
                continue
            if attr & 0x08:      # 卷标项（mkfat build_volume_puts 根首项
                continue         # "BERRYBOOT  "，mkfat.py :458——非文件跳过）
            clus = int.from_bytes(ent[26:28], "little")
            size = int.from_bytes(ent[28:32], "little")
            path = f"{prefix}/{name}"
            if attr & 0x10:
                walk_dir(read_chain(clus, spc * bps), path, into)
            else:
                into[path] = (size, read_chain(clus, size))

    out = {}
    walk_dir(img[root_off:root_off + root_entries * 32], "", out)
    return out


# ---- selftest（零构建依赖——合成基准全谱：正例 + 拒面逐型）----

SELFTEST_PK = bytes(range(0xA0, 0xA0 + 32))       # 合成源钥（确定性）
SELFTEST_PK2 = bytes(range(0x50, 0x50 + 32))


def _mk_entry(eid="P2APP", ver=(0, 1, 0), size=4096, lineage="berryos",
              pk=SELFTEST_PK, last_rev=1, cert="active",
              digest=None):
    return Entry(eid, ver, size,
                 digest if digest is not None else bytes(range(32)),
                 lineage, pk, last_rev, cert)


def selftest() -> int:
    bad = 0

    def leg(ok, label):
        nonlocal bad
        print(f"  selftest {'过（预期）' if ok else 'FAIL'} : {label}")
        if not ok:
            bad += 1

    def leg_die(label, want, fn):
        nonlocal bad
        try:
            fn()
        except SystemExit as e:
            ok = want in str(e)
            print(f"  selftest 拒（预期） : {label}"
                  + ("" if ok else f"【文案缺 {want!r}：{e}】"))
            if not ok:
                bad += 1
            return
        print(f"  selftest FAIL: {label} 被放行")
        bad += 1

    # ① TLV round-trip：合成三档（单条目/帽满字段/双条目含无见证）——
    #    parse(encode) 结构恒等 + 字节再发射恒等。
    for label, entries in (
        ("单条目", [_mk_entry()]),
        ("帽满字段", [_mk_entry(eid="A" * 24, ver=(999, 999, 999),
                               lineage="a" * 16, last_rev=2**32 - 1)]),
        ("双条目（含无见证）", [_mk_entry(eid="AAA"), _mk_entry(
            eid="BBB", pk=None, lineage="oem-main", last_rev=0)]),
    ):
        blob = build_index(entries, 7, SELFTEST_PK)
        idx = parse_index(blob)
        again = build_index(idx.entries, idx.index_ver, idx.source_pk,
                            idx.src_sig if idx.signed else None)
        leg(again == blob and len(idx.entries) == len(entries)
            and idx.index_ver == 7 and not idx.signed,
            f"round-trip {label}（字节恒等 + 字段对账）")

    # ② 八字段完整：parse 产物八字段逐一对账（含无见证 0B 臂）。
    blob = build_index([_mk_entry(eid="EIGHT", ver=(1, 2, 3), size=77,
                                  lineage="oe-m", pk=SELFTEST_PK2,
                                  last_rev=5)], 1, SELFTEST_PK)
    e = parse_index(blob).entries[0]
    leg((e.id, e.ver, e.size, e.lineage, e.last_rev, e.cert) ==
        ("EIGHT", (1, 2, 3), 77, "oe-m", 5, "active")
        and e.digest == bytes(range(32)) and e.pk == SELFTEST_PK2,
        "八字段齐全（id/三值版本/size/哈希/lineage/pk 见证/last_revision/"
        "cert_status）")
    e0 = parse_index(build_index([_mk_entry(eid="NOWIT", pk=None)], 1,
                                 SELFTEST_PK)).entries[0]
    leg(e0.pk is None, "公钥见证 0B 臂（无 .sig——诚实缺席）")

    # ③ 回降拒（check_gen 同口径三钉 + 生成侧 --prev 流）。
    check_index_version(3, 3)   # 等值过（幂等面）
    check_index_version(4, 3)   # 升过
    leg(True, "单调门 等值/升 过（ledger.rs tests :1015-1016 同口径）")
    leg_die("单调门 回降拒", "回降拒", lambda: check_index_version(2, 3))
    prev = parse_index(build_index([_mk_entry()], 5, SELFTEST_PK))
    leg_die("生成侧 --prev 回降拒", "回降拒",
            lambda: check_index_version(prev.index_ver - 1, prev.index_ver))

    # ④ 拒面逐型（结构/值域/canonical/flags-slot 一致性）。
    good = build_index([_mk_entry(), _mk_entry(eid="ZETA")], 9, SELFTEST_PK)

    def mangled(mut):
        c = bytearray(good)
        mut(c)
        return parse_index(bytes(c))

    for label, want, mut in [
        ("magic", "magic", lambda c: c.__setitem__(0, 0x58)),
        ("ver=2", "保守拒", lambda c: c.__setitem__(4, 2)),
        ("alg=9", "未在册", lambda c: c.__setitem__(5, 9)),
        ("flags 保留位", "保留位", lambda c: c.__setitem__(6, 0x02)),
        ("reserved≠0", "reserved", lambda c: c.__setitem__(7, 1)),
        ("entry_count+1", "越域", lambda c: c.__setitem__(13, 3)),
        ("截断 1B", "总长不符", lambda c: c.__delitem__(slice(len(c) - 1,
                                                            len(c)))),
        ("trailer 1B", "总长不符", lambda c: c.extend(b"\x00")),
    ]:
        leg_die(f"结构拒面 {label}", want, lambda m=mut: mangled(m))
    # 值域/canonical/flags-slot 拒面（build/parse 同口径单点执法）。
    leg_die("id 25B", "越域", lambda: build_index(
        [_mk_entry(eid="A" * 25)], 1, SELFTEST_PK))
    leg_die("id 坏字符", "越域", lambda: build_index(
        [_mk_entry(eid="bad id")], 1, SELFTEST_PK))
    leg_die("版本段 1000", "越域", lambda: build_index(
        [_mk_entry(ver=(1000, 0, 0))], 1, SELFTEST_PK))
    leg_die("lineage 17B", "越域", lambda: build_index(
        [_mk_entry(lineage="a" * 17)], 1, SELFTEST_PK))
    leg_die("lineage 大写", "越域", lambda: build_index(
        [_mk_entry(lineage="Oem")], 1, SELFTEST_PK))
    leg_die("cert 非 active", "未在册", lambda: parse_index(
        build_index([_mk_entry(cert="revoked")], 1, SELFTEST_PK)))
    leg_die("id 重复", "重复", lambda: build_index(
        [_mk_entry(), _mk_entry()], 1, SELFTEST_PK))
    # pk_len=31 单一解释拒（变首个条目 pk_len 字节——头 46+1+5+6+8+32+1+7=106）
    pk31 = bytearray(build_index([_mk_entry(pk=None)], 1, SELFTEST_PK))
    pk31[106] = 31
    leg_die("pk_len 31（单一解释）", "非 0/32", lambda: parse_index(
        bytes(pk31)))
    lie = bytearray(build_index([_mk_entry()], 1, SELFTEST_PK))
    lie[6] = FLAG_SIGNED
    leg_die("signed 位 + 零槽（谎报位）", "谎报位", lambda: parse_index(
        bytes(lie)))
    stow = bytearray(build_index([_mk_entry()], 1, SELFTEST_PK))
    stow[-1] = 0x01
    leg_die("signed=0 + 非零槽", "单一解释拒", lambda: parse_index(
        bytes(stow)))

    # ⑤ 源钥清单：黄金字节（冻结格式执法点）+ 回读 + fingerprint 交叉。
    golden = (f"{KEYS_HEAD} sources=01\n"
              f"key source_id={'berryos':<{SID_W}} pk={SELFTEST_PK.hex()} "
              f"fingerprint={sha512_256(SELFTEST_PK).hex()} "
              "sig_state=unsigned\n").encode("ascii")
    got = build_keys_listing("berryos", SELFTEST_PK, signed=False)
    leg(got == golden, "source-keys v1 黄金字节（冻结行格式）")
    sid, pk, signed = parse_keys_listing(got)
    leg((sid, pk, signed) == ("berryos", SELFTEST_PK, False)
        and build_keys_listing(sid, pk, signed) == got,
        "source-keys 回读 + 再发射恒等（fingerprint = SHA-512/256(pk) 交叉）")
    leg_die("清单头行漂移", "头行漂移", lambda: parse_keys_listing(
        got.replace(b"v1 ", b"v2 ")))
    leg_die("清单 fingerprint 篡改", "fingerprint", lambda: parse_keys_listing(
        got.replace(sha512_256(SELFTEST_PK).hex().encode(), b"0" * 64)))
    leg_die("source_id 17B", "越域", lambda: build_keys_listing(
        "a" * 17, SELFTEST_PK, False))

    # ⑥ 确定性：同输入同字节（条目序无关——canonical 单形）。
    leg(build_index([_mk_entry(), _mk_entry(eid="ZETA")], 9, SELFTEST_PK)
        == build_index([_mk_entry(eid="ZETA"), _mk_entry()], 9, SELFTEST_PK),
        "构造确定性（双跑逐字节恒等）")

    # ⑦ SHA-512/256 量纲钉板（NIST "abc" 向量——与 berry-crypto
    #    sha512::digest256 同向量族交叉）。
    leg(sha512_256(b"abc").hex() ==
        "53048e2681941ef99b2e29b76b4c7dabe4c2d0c634fc6d46e0e2f13107e7af23",
        'SHA-512/256("abc") 钉板')

    # ⑧ mkimage 挂接合同绊线（跨工具常量漂移守卫——同步义务的执法点）。
    import mkimage
    leg((mkimage.SRCINST_INDEX_NAME, mkimage.SRCINST_KEYS_NAME,
         mkimage.SRCINST_FAT_INDEX, mkimage.SRCINST_FAT_KEYS) ==
        (INDEX_NAME, KEYS_NAME, "/SOURCE/SRCINDEX.TLV",
         "/SOURCE/SRCKEYS.TXT"),
        "mkimage --source-instance 合同对拍（两件套名 + FAT 路径）")

    # ⑨ FAT16 回读对拍（fat16_ls 对读面）：6 件套造盘 → 存在性 + 尺寸 +
    #    内容字节三对拍（E2E「镜像内文件存在性对拍」的机制面前置）。
    puts = [("/EFI/BOOT/BOOTAA64.EFI", b"\x00\x31\x01\xaa" * 800),
            ("/HELLO.TXT", b"berryboot efi hello\n"),
            ("/KERNEL.ELF", bytes((i * 31 + 7) % 256 for i in range(5000))),
            ("/INITRD.BIN", b"berryos initrd stub\n"),
            (mkimage.SRCINST_FAT_INDEX, blob),
            (mkimage.SRCINST_FAT_KEYS, got)]
    img, _ = mkimage.build_image(puts, 9)
    ls = fat16_ls(img)
    leg(ls.get(mkimage.SRCINST_FAT_INDEX) == (len(blob), blob)
        and ls.get(mkimage.SRCINST_FAT_KEYS) == (len(got), got),
        "镜像内存在性对拍（/SOURCE/ 两件——尺寸+内容字节）")
    leg(ls.get("/HELLO.TXT") == (20, b"berryboot efi hello\n")
        and ls.get("/KERNEL.ELF", (0, b""))[0] == 5000,
        "既有四件套不漂移（HELLO/KERNEL 回读）")

    # ⑩ 签名索引合成腿（零构建依赖）：64B 合成槽 round-trip + 清单
    #     signed 位 + unsigned 兼容（无 --sign-key 现状面钉板）。
    synth = bytes(range(0xC0, 0xC0 + SIG_LEN))
    sform = build_index([_mk_entry(), _mk_entry(eid="ZETA")], 9, SELFTEST_PK,
                        src_sig=synth)
    sidx = parse_index(sform)
    leg(sidx.signed and sidx.src_sig == synth
        and build_index(sidx.entries, sidx.index_ver, sidx.source_pk,
                        sidx.src_sig) == sform,
        "签名索引 round-trip（合成 64B 槽——flags.signed=1 + 字节恒等）")
    leg(parse_keys_listing(build_keys_listing("berryos", SELFTEST_PK,
                                              signed=True))[2] is True,
        "清单 signed 位（两件套对拍面 sig_state=signed）")
    uidx = parse_index(build_index([_mk_entry()], 2, SELFTEST_PK))
    leg(not uidx.signed and uidx.src_sig == b"\x00" * SIG_LEN,
        "unsigned 兼容（无 --sign-key 现状面：flags.signed=0 + 槽恒零）")

    # ⑪ 真签名腿（berry-sign 在席则跑；不在席如实注跳过——零构建依赖
    #     自检纪律）：单点缝全链 keygen → 预置形 → sign_index 真签 →
    #     换槽 → parse → verify_index_signature 正/篡改拒/换钥拒 →
    #     mkimage 挂接镜像内对拍（E2E 机制面前置；全链 CLI 实跑 = E2E）。
    if find_berry_sign() is None:
        print("  selftest 注: 真签名腿跳过（berry-sign 不可得——合成腿⑩"
              "已钉布局面；设 BERRY_SIGN 或先构建 berry-sign 后复跑）")
    else:
        import tempfile
        with tempfile.TemporaryDirectory(prefix="gsi-selftest-") as td:
            sk_p = os.path.join(td, "t.sk")
            pk_p = os.path.join(td, "t.pk")
            r = run_berry_sign(["keygen", sk_p, pk_p])
            leg(r is not None and r.returncode == 0,
                "真签名 keygen（dev 钥——测试钥永不真签纪律）")
            with open(pk_p, "rb") as f:
                rpk = f.read()
            form = build_index([_mk_entry(), _mk_entry(eid="ZETA")], 4, rpk,
                               src_sig=b"\x00" * SIG_LEN)
            sig, note = sign_index(form, sk_p)
            signed_idx = form[:-SIG_LEN] + sig
            leg(isinstance(sig, bytes) and len(sig) == SIG_LEN,
                f"sign_index 真签（{note}）")
            pidx = parse_index(signed_idx)
            leg(pidx.signed and pidx.src_sig == sig and pidx.source_pk == rpk,
                "签名索引 parse（flags.signed=1 + 槽在席 + 源钥自述）")
            leg(build_index(pidx.entries, pidx.index_ver, pidx.source_pk,
                            pidx.src_sig) == signed_idx,
                "签名索引 round-trip 字节恒等（canonical 单形不破）")
            leg(verify_index_signature(signed_idx, rpk, sig) is True,
                "verify_index_signature 正样例过（berry-sign verify-raw）")
            tam = bytearray(signed_idx)
            tam[60] ^= 0x01  # 条目区一字节（头 46B 后 size 域——结构仍过检）
            tam_idx = parse_index(bytes(tam))
            leg(verify_index_signature(bytes(tam), tam_idx.source_pk,
                                       tam_idx.src_sig) is False,
                "验签拒篡改（条目区翻 1 字节——BSRC-TOP 覆盖槽前全字节）")
            leg(verify_index_signature(signed_idx, SELFTEST_PK2, sig) is False,
                "验签拒换钥（错源钥）")
            keys_signed = build_keys_listing("berryos", rpk, signed=True)
            puts_r = [("/EFI/BOOT/BOOTAA64.EFI", b"\x00\x31\x01\xaa" * 800),
                      ("/HELLO.TXT", b"berryboot efi hello\n"),
                      ("/KERNEL.ELF", bytes((i * 31 + 7) % 256
                                            for i in range(5000))),
                      ("/INITRD.BIN", b"berryos initrd stub\n"),
                      (mkimage.SRCINST_FAT_INDEX, signed_idx),
                      (mkimage.SRCINST_FAT_KEYS, keys_signed)]
            img_r, _ = mkimage.build_image(puts_r, 9)
            ls_r = fat16_ls(img_r)
            leg(ls_r.get(mkimage.SRCINST_FAT_INDEX)
                == (len(signed_idx), signed_idx)
                and ls_r.get(mkimage.SRCINST_FAT_KEYS)
                == (len(keys_signed), keys_signed),
                "镜像内对拍（真签名两件套——尺寸+内容字节）")

    # ⑫ G16 单源派生（--preset）：product.toml preset ⊕ override → app 条目
    #     集（berry_assemble load_product + resolve_entries 复用——零第二
    #     产品解析器；38 号 §4 双源漂移场景的收口面）；拒面 = Reject 透传。
    import tempfile
    with tempfile.TemporaryDirectory(prefix="gsi-preset-") as td:
        base = os.path.join(td, "base.toml")
        with open(base, "w", encoding="utf-8") as f:
            f.write("version = 1\n\n[entries]\n"
                    '"app/A.bap" = "userbins/app/A.bap"\n'
                    '"app/B.bap" = "userbins/app/B.bap"\n'
                    '"bin/bsh" = "cargo-target/release/bsh"\n')
        user = os.path.join(td, "user")
        for rel in ("userbins/app/A.bap", "userbins/app/B.bap",
                    "cargo-target/release/bsh"):
            p = os.path.join(user, rel.replace("/", os.sep))
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "wb") as f:
                f.write(b"stub")
        prod_dir = os.path.join(td, "prod")
        os.makedirs(os.path.join(prod_dir, "oem"), exist_ok=True)
        with open(os.path.join(prod_dir, "oem", "A.bap"), "wb") as f:
            f.write(b"OEM-A-stub")
        PRODUCT = ("schema = 1\n\n[product]\n"
                   'name    = "oem-x"\nvendor  = "oem"\n'
                   'board   = "qemu-virt"\n\n[platform]\n'
                   'arch          = "aarch64"\n'
                   'feature_level = "rich"\n'
                   'build_type    = "eng"\nsideload      = false\n\n'
                   "[platform.boot]\n"
                   'kernel = "KERNEL.ELF@sha256:e3b0c44298fc1c149afbf4c8996'
                   "fb92427ae41e4649b934ca495991b7852b855\"\n\n"
                   "[platform.storage]\n"
                   "image_size_mib      = 96\n"
                   "rootfs_split_mib    = 32\n"
                   'min_power_atomicity = "p0"\n\n'
                   "[product.apps]\n"
                   'preset = ["app/A.bap"]\n\n'
                   "[product.apps.override]\n"
                   'A = "oem/A.bap"\n\n'
                   "[product.defaults]\n"
                   'launcher = "bin/bsh"\n')

        def write_product(text, name):
            p = os.path.join(prod_dir, name)
            with open(p, "w", encoding="utf-8") as f:
                f.write(text)
            return p

        p_ovr = write_product(PRODUCT, "p.toml")
        got = load_preset_entries(p_ovr, base, user)
        leg(got == [("app/A.bap", os.path.join(prod_dir, "oem", "A.bap"))],
            "--preset 派生（preset 单条 + override 绝对路径——基座 B 不进）")
        p_two = write_product(PRODUCT.replace(
            'preset = ["app/A.bap"]', 'preset = ["app/A.bap", "app/B.bap"]').
            replace('[product.apps.override]\nA = "oem/A.bap"\n\n', ""),
            "p2.toml")
        got2 = dict(load_preset_entries(p_two, base, user))
        leg(sorted(got2) == ["app/A.bap", "app/B.bap"]
            and got2["app/B.bap"] == os.path.join(user, "userbins", "app",
                                                 "B.bap"),
            "--preset 派生（双 preset 无 override——基座相对路径归一）")
        leg_die("--preset 拒面透传（preset 不在基座表）", "--preset 产品解析拒",
                lambda: load_preset_entries(write_product(
                    PRODUCT.replace('preset = ["app/A.bap"]',
                                    'preset = ["app/C.bap"]'), "bad.toml"),
                    base, user))
        leg_die("--preset 空 preset（终表无 app 条目）", "preset 空",
                lambda: load_preset_entries(write_product(
                    PRODUCT.replace('preset = ["app/A.bap"]', "preset = []").
                    replace('[product.apps.override]\nA = "oem/A.bap"\n\n',
                            ""), "empty.toml"), base, user))

    if bad:
        print(f"gensourceindex: selftest FAIL x{bad}")
        return 1
    print("gensourceindex: selftest ok")
    return 0


def main() -> int:
    import argparse
    ap = argparse.ArgumentParser(
        description="BerryOS 官方源静态实例生成器（SDK-2 P2 工具面——"
                    "source-index.tlv 八字段索引 + source-keys.txt 源钥清单）")
    ap.add_argument("--rootfs-manifest", default=DEFAULT_MANIFEST,
                    help=f"rootfs app 条目表（默认 {DEFAULT_MANIFEST}；"
                         "app/*.bap 条目进索引，selftest/* 夹具不计；"
                         "--preset 在席时改任基座表供路径解析）")
    ap.add_argument("--user-dir", default=DEFAULT_USER_DIR,
                    help="条目 path 相对基准（默认 <repo>/user——表头注契约）")
    ap.add_argument("--out-dir", help="两件套输出目录")
    ap.add_argument("--source-pk", help="源钥 pk 文件（32B——berry-sign "
                                        "keygen 产物）")
    ap.add_argument("--sign-key", metavar="SK",
                    help="源钥 sk 文件（32B）——在席则索引真签（单点缝 "
                         "sign_index() × berry-sign sign-raw → 64B 裸签名"
                         " + flags.signed=1）；缺席则 unsigned 诚实降级"
                         "（向后兼容）")
    ap.add_argument("--source-id", default="berryos",
                    help="官方源 id（默认 berryos；1..=16B [a-z0-9-]）")
    ap.add_argument("--index-version", type=int, default=1,
                    help="索引级单调版本号（默认 1；--prev 在席则回降拒）")
    ap.add_argument("--prev", help="前代 source-index.tlv（单调水位对账）")
    ap.add_argument("--preset", metavar="PRODUCT.TOML",
                    help="G16 单源派生：app 条目直接派生自 product.toml"
                         "（berry_assemble load_product + resolve_entries"
                         " 复用——preset ⊕ override 终表 app 集），消"
                         "「源索引条目表」自持；--rootfs-manifest 改任"
                         "基座表")
    ap.add_argument("--verify", help="验证既有 source-index.tlv（结构/八字段"
                                     "/单调/签名验证（signed = 真验签）/"
                                     "两件套对拍）")
    ap.add_argument("--gen-keys", nargs=2, metavar=("SK-OUT", "PK-OUT"),
                    help="透传 berry-sign keygen 产 dev 密钥对")
    ap.add_argument("--selftest", action="store_true",
                    help="零构建依赖自检（合成基准全谱；berry-sign 在席"
                         "加跑真签名腿）")
    args = ap.parse_args()

    if args.selftest:
        return selftest()
    if args.gen_keys:
        return do_gen_keys(args.gen_keys[0], args.gen_keys[1])
    if args.verify:
        if args.out_dir or args.source_pk or args.sign_key or args.preset:
            ap.error("--verify 独占（不与生成面混用）")
        return do_verify(args)
    if args.sign_key and not args.source_pk:
        ap.error("--sign-key 需与 --source-pk 同席（pk 进索引自述 + sk 签名"
                 "——钥对同源）")
    if not (args.out_dir and args.source_pk):
        ap.error("生成需 --out-dir + --source-pk（或用 --verify/--gen-keys/"
                 "--selftest）")
    return do_generate(args)


if __name__ == "__main__":
    sys.exit(main())
