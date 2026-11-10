#!/usr/bin/env python3
r"""berry_assemble——产品配置层组装器 v0（V13 实施刀，ADR #82「组装不 fork」；
设计权威 = `docs/产品配置层与槽位合同设计-v1.0.md` §1（product schema 全量）
与 §4（九步管线）；底稿 `research/厂商定制化深研/08 号` §3）。

用法：
  berry_assemble.py --check <product.toml>   # 只校验不产（gen_abi --check 先例）：
                                             #   schema/未知键硬拒/类型/值域/路径存在/
                                             #   基座对账——过则一行 OK
  berry_assemble.py --dry-run <product.toml> [--platform-revision N]
                                            # 校验 + 组装计划（rootfs 条目清单 +
                                            #   mkcpio/mkimage 参数集），不产件
  berry_assemble.py --sample                 # 打印内置样例 product.toml（体验起点）
  berry_assemble.py --selftest               # 自检：合成基准全谱（正例 + 拒面逐型；
                                             #   零构建依赖、零仓外写入）

v0 范围（最小可用，不做镜像全产线）——对照九步（设计文档 §4.1）：
  - 步① 清单校验：product 键子集（预装集/品牌资源路径/默认应用）全量执法；
    platform 键 v0 只做存在性 + 枚举轻校验（深合同执法随 #83 落册后的工具升版）。
    `[platform.boot.params]` 词表执法（Q2：键 ⊆ 启动参数词表、未知键硬拒——
    词表 v1 最小集 = sched_slice_hz/intent_default，§1.6）。
  - 步④ rootfs 条目解析：基座条目（tools/rootfs-manifest.toml——V13 数据化
    接口，经 `from mkcpio import load_manifest` 复用，禁第二份加载器）⊕ preset
    条目（app/ 前缀 ⊆ 基座 app 条目——v0 无 OEM 三包输入，新增/替换交换
    归 V13 后续刀）；排序 = 整条 name=path（mkcpio CLI 现役语义）。
  - 步⑦ mkimage 参数集：--size-mib 按清单 + --platform-revision 透传（V9：
    manifest v1 逐条目列，0 = 平台基线）；kernel 锚只验形（步⑧ 逐字节度量
    对账归全产线）。
  - 不做（如实记）：步②白名单校验（V13+V6+V8 归后续刀）、步⑤conformance 门
    （P5 随 #84）、步⑥⑧⑨⑩产件与对账（镜像全产线）、just 配方 assemble
    （归主会话收口）。

拒面纪律（berry-manifest 七拒同构——未知节/未知键/必填缺失/类型错/值域外）：
  一行理由 + 键名 + 非零退（mkapp Reject 先例，`tools/mkapp.py:165` 起）；未知键
  硬拒分两档——「schema 无此键」（V12）与「schema 在册、本工具 v0 未收录」
  （product 键 add-only 随组装工具版本化，§1.1——两档都拒，文案分档防误诊）。

常量对账锚（mkapp ↔ berry-manifest 镜像同款义务——改上游常量须同步此处）：
  APP_SLOTS=9 / 六档 intent 词表 / ID_MAX=24 同规（product.name）/ lineage
  1..=16B [a-z0-9-]（SDK-2 §二，mkapp SEC_RE 同款）；数值域两源 = 设计文档
  §1.3 逐键表 × 现役 justfile/mkimage 参数（image_size_mib [48,1024]——下界
  48 = mkimage 史注最小；rootfs_split_mib [8,64]——现役 32）。

确定性：dry-run 计划只依赖输入文件内容（无时刻输入）——同输入同计划行。
依赖：Python ≥3.11（stdlib tomllib——gen_abi/mkcpio 同先例），零三方依赖。
"""

import os
import re
import sys

try:
    import tomllib
except ModuleNotFoundError:  # CLT python3=3.9 无 tomllib（mkapp/gen_abi 同款门）
    sys.stderr.write("berry-assemble: 需要 Python >= 3.11（tomllib）\n")
    sys.exit(1)

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mkcpio import load_manifest  # noqa: E402（V13 数据化接口复用——单加载器纪律）

ROOT = os.path.dirname(os.path.abspath(__file__))            # .../tools
REPO = os.path.dirname(ROOT)
DEFAULT_BASE = os.path.join(ROOT, "rootfs-manifest.toml")    # V13 平台基座自描述
DEFAULT_USER_DIR = os.path.join(REPO, "user")                # 基座表 path 的相对基准

# ---- 帽与词表（对账锚：user/berry-manifest 常量段 + 设计文档 §1.3/§1.4 表）----
APP_SLOTS = 9          # berry-manifest APP_SLOTS（设计文档 §1.4 引 :120；mkapp 镜像同款）
NAME_MAX = 24          # product.name 帽（ID_MAX 同规，§1.4）
VENDOR_MAX = 16        # product.vendor lineage 帽（SDK-2 §二轨道）
BOARDS = ("qemu-virt",)                    # v0 板表单值（§1.4；板表=平台仓数据文件）
ARCHES = ("aarch64", "x86_64")             # x86_64=最小集（§1.3）
FEATURE_LEVELS = ("rich",)                 # Q7：v0 只 rich 在册（骨架 add-only）
BUILD_TYPES = ("eng", "userdebug", "user")
ATOMICITY_LEVELS = ("p0", "p1", "p2")      # §2.4 三档：词表先立、v1 只 p0 可组
SIZE_MIB_RANGE = (48, 1024)                # §1.3：下界=mkimage 史注最小、上界防误填
SPLIT_MIB_RANGE = (8, 64)                  # §1.3：现役 32（mkcpio --split 语义）
INTENTS = (                                # ADR #58 六档词表（mkapp INTENTS 同源镜像）
    "unspecified", "latency_critical", "interactive",
    "utility", "background", "maintenance",
)
PARAMS_MAX = 32        # params 子表条目帽（§1.6-4 解析面定界：INITRD 段 count 帽 32）

# [platform.boot.params] 词表 v1 最小集（§1.6-3；词表本体 add-only、未知键硬拒——
# Q2。restricted/locked 档行随 PM/thermal 域刀增补——v1 词表内无 locked 行，
# locked 键「出现即拒」由未知键拒面先行覆盖）。
BOOT_PARAMS_SPEC = {
    "sched_slice_hz": ("u32", (16, 1024)),  # free 档（缺省归上游编译期常量）
    "intent_default": ("intent", None),     # free 档（六档词表；缺省 utility）
}

NAME_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*")     # 首字符字母数字（§1.4）
VENDOR_RE = re.compile(r"[a-z0-9-]{1,16}")               # lineage（SDK SEC_RE 同款）
ANCHOR_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*@sha256:[0-9a-fA-F]{64}$")

# 各节 v0 键集 + 在册未收录键（§1.2-1.5 全量减 v0 子集——拒面分档的数据源）
TOP_KEYS = {"schema", "product", "platform"}
PRODUCT_KEYS = {"name", "vendor", "board", "apps", "branding", "defaults"}
PRODUCT_DEFERRED = {"services"}                          # [product.services.replace]
APPS_KEYS = {"preset"}
APPS_DEFERRED = {"priv_grants"}                          # V8/步② 归后续刀
BRANDING_KEYS = {"wallpaper", "icon_mask"}
BRANDING_DEFERRED = {"tokens"}                           # §1.4 token 闭集
DEFAULTS_KEYS = {"launcher"}
DEFAULTS_DEFERRED = {"scheduler", "locale"}              # §1.5 i18n 八键归升版
PLATFORM_KEYS = {"arch", "feature_level", "build_type", "sideload", "boot", "storage"}
BOOT_KEYS = {"kernel", "params"}
STORAGE_KEYS = {"image_size_mib", "rootfs_split_mib", "min_power_atomicity"}


class Reject(Exception):
    """门禁拒绝（一行理由 + 键名，非零退出）——mkapp Reject 先例（本工具
    无 --json 面，不带结构化舱；文案即契约）。"""


class Product:
    """过检后的 product.toml 视图（组装计划的数据源）。"""

    def __init__(self):
        self.name = ""
        self.vendor = ""
        self.board = ""
        self.arch = ""
        self.feature_level = ""
        self.build_type = ""
        self.sideload = False
        self.kernel_name = ""       # 工件名（如 KERNEL.ELF）
        self.kernel_sha = ""        # 64hex 小写归一
        self.params = {}            # boot.params 覆盖值（override 语义——缺省归上游）
        self.image_size_mib = 0
        self.rootfs_split_mib = 0
        self.min_power_atomicity = "p0"
        self.preset = []            # rootfs app/ 条目名列表（保清单序）
        self.branding = {}          # 键 → 相对 product.toml 所在目录的路径
        self.launcher = ""
        self.src_dir = ""           # product.toml 所在目录（branding 相对基准）


def _reject_unknown(sect: str, keys, allowed: set, deferred: set) -> None:
    """未知键硬拒（两档：schema 无此键 / 在册未收录——§1.1 版本化纪律）。"""
    unknown = sorted(set(keys) - allowed - deferred)
    if unknown:
        raise Reject(f"{sect}: 未知键 {unknown}（V12 未知键硬拒——schema 无此键；"
                     f"收 {sorted(allowed)}）")
    hit = sorted(set(keys) & deferred)
    if hit:
        raise Reject(f"{sect}: 键 {hit} schema 在册、berry-assemble v0 未收录"
                     f"（product 键 add-only 随工具版本化——随 V13 后续刀扩；"
                     f"设计文档 §1.2-§1.5）")


def _need_table(doc: dict, key: str, what: str) -> dict:
    v = doc.get(key)
    if not isinstance(v, dict):
        raise Reject(f"缺 [{what}] 节（必填表——schema §1.2）")
    return v


def _need_u32(sect: str, key: str, v, lo: int, hi: int) -> int:
    # bool 是 int 子类，type() 才拦得住（mkapp min_level/stack_pages 同款门）
    if type(v) is not int:
        raise Reject(f"{sect}.{key}: 应为整数（got {type(v).__name__}）")
    if not lo <= v <= hi:
        raise Reject(f"{sect}.{key}: 越域 [{lo},{hi}]（got {v}）")
    return v


def _check_product_section(prod: Product, sect: dict, src_dir: str) -> None:
    """[product] 自由面 v0 子集（预装集/品牌资源路径/默认应用）。"""
    _reject_unknown("product", sect, PRODUCT_KEYS, PRODUCT_DEFERRED)
    for key, re_, maxlen in (("name", NAME_RE, NAME_MAX), ("vendor", VENDOR_RE, VENDOR_MAX)):
        v = sect.get(key)
        if not isinstance(v, str):
            raise Reject(f"product.{key}: 应为字符串（got {type(v).__name__}）")
        if not 1 <= len(v) <= maxlen:
            raise Reject(f"product.{key}: 空/超长（1..={maxlen}，got {len(v)}B）")
        if not re_.fullmatch(v):
            raise Reject(f"product.{key}: 字符集不符（name=[A-Za-z0-9][A-Za-z0-9._-]、"
                         f"vendor=[a-z0-9-]——got {v!r}）")
        setattr(prod, key, v)
    board = sect.get("board")
    if not isinstance(board, str) or not board:
        raise Reject(f"product.board: 应为非空字符串（got {type(board).__name__}）")
    prod.board = board
    if prod.board not in BOARDS:
        raise Reject(f"product.board: 不在板表 {list(BOARDS)}（v0 板表单值——"
                     f"板表=平台仓数据文件，P8 前治理见 18 号增补 A；got {prod.board!r}）")

    # [product.apps].preset——预装集（v0 主读面，必填）
    apps = sect.get("apps")
    if not isinstance(apps, dict):
        raise Reject("缺 [product.apps].preset 节（v0 预装集必填——schema §1.2）")
    _reject_unknown("product.apps", apps, APPS_KEYS, APPS_DEFERRED)
    preset = apps.get("preset")
    if not isinstance(preset, list):
        raise Reject("product.apps.preset: 应为路径数组（got "
                     f"{type(preset).__name__}）")
    for i, item in enumerate(preset):
        if not isinstance(item, str):
            raise Reject(f"product.apps.preset[{i}]: 应为字符串（got "
                         f"{type(item).__name__}）")
        if not item.startswith("app/"):
            raise Reject(f"product.apps.preset[{i}]: 非 app/ 前缀路径 {item!r}"
                         f"（预装集 = rootfs app/ 条目——schema §1.4）")
    dups = sorted({e for e in preset if preset.count(e) > 1})
    if dups:
        raise Reject(f"product.apps.preset: 条目重复 {dups}（查重——schema §1.4）")
    if len(preset) > APP_SLOTS:
        raise Reject(f"product.apps.preset: {len(preset)} 项越 APP_SLOTS 帽 "
                     f"{APP_SLOTS}（berry-manifest 常量——schema §1.4）")
    prod.preset = list(preset)

    # [product.branding]——品牌资源路径（可选；包内存在校验）
    branding = sect.get("branding", {})
    if not isinstance(branding, dict):
        raise Reject("product.branding: 应为节（键值表）")
    _reject_unknown("product.branding", branding, BRANDING_KEYS, BRANDING_DEFERRED)
    for key, rel in branding.items():
        if not isinstance(rel, str) or not rel:
            raise Reject(f"product.branding.{key}: 应为非空字符串路径")
        local = os.path.join(src_dir, rel)
        if not os.path.isfile(local):
            raise Reject(f"product.branding.{key}: 包内不存在 {rel!r}"
                         f"（相对 product.toml 所在目录解析——schema §1.4）")
        prod.branding[key] = rel

    # [product.defaults].launcher——默认应用（Q13 过渡：app/ 或 bin/ 前缀皆合法）
    defaults = _need_table(sect, "defaults", "product.defaults")
    _reject_unknown("product.defaults", defaults, DEFAULTS_KEYS, DEFAULTS_DEFERRED)
    launcher = defaults.get("launcher")
    if not isinstance(launcher, str) or not launcher:
        raise Reject("product.defaults.launcher: 应为非空字符串（必填——v0 默认应用）")
    if not (launcher.startswith("app/") or launcher.startswith("bin/")):
        raise Reject(f"product.defaults.launcher: 应为 app/ 或 bin/ 前缀路径"
                     f"（Q13 过渡：v1 bin/ 合法、.bap 转正后收 app/——got {launcher!r}）")
    prod.launcher = launcher


def _check_platform_section(prod: Product, sect: dict) -> None:
    """[platform] 合同面 v0：存在性 + 枚举/类型轻校验（深合同执法随工具升版）。"""
    _reject_unknown("platform", sect, PLATFORM_KEYS, set())
    for key in ("arch", "feature_level", "build_type", "sideload", "boot", "storage"):
        if key not in sect:
            raise Reject(f"platform.{key}: 必填缺失（v0 存在性校验——schema §1.3）")
    for key, vocab, why in (
        ("arch", ARCHES, "x86_64=最小集"),
        ("feature_level", FEATURE_LEVELS, "Q7：v0 只 rich 在册（骨架 add-only）"),
        ("build_type", BUILD_TYPES, "user 型收窄见 §1.7"),
    ):
        v = sect[key]
        if not isinstance(v, str) or v not in vocab:
            raise Reject(f"platform.{key}: 枚举外值 {v!r}（词表 {list(vocab)}——{why}）")
    sideload = sect.get("sideload")
    if type(sideload) is not bool:
        raise Reject(f"platform.sideload: 应为 bool（got {type(sideload).__name__}）")
    if sect["build_type"] == "user" and sideload:
        raise Reject("platform.sideload: user 型下必须缺省或 false"
                     "（§1.3 收窄清单——v1 立法面）")
    prod.arch, prod.feature_level = sect["arch"], sect["feature_level"]
    prod.build_type, prod.sideload = sect["build_type"], sideload

    # [platform.boot]——kernel 工件锚 + params 覆盖子表（Q2）
    boot = _need_table(sect, "boot", "platform.boot")
    _reject_unknown("platform.boot", boot, BOOT_KEYS, set())
    kernel = boot.get("kernel")
    if not isinstance(kernel, str) or not kernel:
        raise Reject("platform.boot.kernel: 应为非空字符串（必填——工件锚）")
    if not ANCHOR_RE.fullmatch(kernel):
        raise Reject(f"platform.boot.kernel: 工件锚形不符 {kernel!r}"
                     f"（须 name@sha256:<64hex>——schema §1.3；逐字节对账归全产线步⑧）")
    prod.kernel_name, _, sha = kernel.partition("@sha256:")
    prod.kernel_sha = sha.lower()
    params = boot.get("params", {})
    if not isinstance(params, dict):
        raise Reject("platform.boot.params: 应为节（键值表）")
    if len(params) > PARAMS_MAX:
        raise Reject(f"platform.boot.params: {len(params)} 键越帽 {PARAMS_MAX}"
                     f"（§1.6-4 解析面定界——INITRD 段 count 帽 32）")
    for key, val in params.items():
        if key not in BOOT_PARAMS_SPEC:
            raise Reject(f"platform.boot.params.{key}: 不在启动参数词表 "
                         f"{sorted(BOOT_PARAMS_SPEC)}（Q2：词表 add-only、未知键硬拒"
                         f"——新键进词表走 #83 后续升版）")
        kind, rng = BOOT_PARAMS_SPEC[key]
        if kind == "u32":
            prod.params[key] = _need_u32("platform.boot.params", key, val, *rng)
        else:  # intent 六档
            if not isinstance(val, str) or val not in INTENTS:
                raise Reject(f"platform.boot.params.{key}: 不在六档词表 {INTENTS}"
                             f"（ADR #58——got {val!r}）")
            prod.params[key] = val

    # [platform.storage]——产物几何（组装器步⑥⑦消费面）+ FS 档位执法
    storage = _need_table(sect, "storage", "platform.storage")
    _reject_unknown("platform.storage", storage, STORAGE_KEYS, set())
    for key in STORAGE_KEYS:
        if key not in storage:
            raise Reject(f"platform.storage.{key}: 必填缺失（schema §1.3）")
    prod.image_size_mib = _need_u32("platform.storage", "image_size_mib",
                                    storage["image_size_mib"], *SIZE_MIB_RANGE)
    prod.rootfs_split_mib = _need_u32("platform.storage", "rootfs_split_mib",
                                      storage["rootfs_split_mib"], *SPLIT_MIB_RANGE)
    pa = storage["min_power_atomicity"]
    if pa not in ATOMICITY_LEVELS:
        raise Reject(f"platform.storage.min_power_atomicity: 枚举外值 {pa!r}"
                     f"（词表 {list(ATOMICITY_LEVELS)}——§2.4）")
    if pa != "p0":
        raise Reject(f"platform.storage.min_power_atomicity: 词表内但 v1 只 p0 可组"
                     f"（{pa} 无在册实现可满足——§1.3/§2.4 词表先立值域收窄，"
                     f"08 号 §6-Q11 采纳形）")
    prod.min_power_atomicity = pa


def load_product(path: str) -> Product:
    """product.toml → 过检视图（步① 清单校验；TOML 解析 + 双键结构 + 逐节执法）。

    拒面：坏 TOML/读失败/未知顶层键/schema ≠ 1/缺节/缺键/类型错/值域外/
    枚举外/params 词表外/越帽——一律 Reject 一行理由带键名（§1.4 通用行）。
    """
    try:
        with open(path, "rb") as f:
            doc = tomllib.load(f)
    except OSError as e:
        raise Reject(f"读 {path} 失败：{e}")
    except tomllib.TOMLDecodeError as e:
        raise Reject(f"{path}: TOML 解析失败：{e}")
    _reject_unknown("(顶层)", doc, TOP_KEYS, set())
    if "schema" not in doc:
        raise Reject("schema: 必填缺失（清单 schema 版本——§1.2）")
    if type(doc["schema"]) is not int or doc["schema"] != 1:
        raise Reject(f"schema: 须 = 1（u32；v1 唯一值、升版 add-only——got "
                     f"{doc['schema']!r}）")
    prod = Product()
    prod.src_dir = os.path.dirname(os.path.abspath(path))
    _check_product_section(prod, _need_table(doc, "product", "product"), prod.src_dir)
    _check_platform_section(prod, _need_table(doc, "platform", "platform"))
    return prod


def validate_platform_revision(pr: int) -> int:
    """--platform-revision 域守卫（V9 透传面）——u32 全域 [0,4294967295]，与
    mkimage build_manifest 拒面同口径（列宽 10 是 manifest v1 冻结契约的一部分）。"""
    if type(pr) is not int or not 0 <= pr <= 2**32 - 1:
        raise Reject(f"--platform-revision: 须 u32 域 [0,4294967295]（V9 列宽契约"
                     f"——mkimage 同口径拒；got {pr!r}）")
    return pr


def resolve_entries(prod: Product, base_manifest: str, user_dir: str):
    """步④ rootfs 条目解析：基座（V13 表）⊕ preset（app/ 条目筛选）。

    返回 (排序条目列表, 基座非 app 条目数)。v0 边界：preset 条目必须在基座
    表内（无 OEM 三包输入——新增/替换交换归后续刀）；基座 path 相对 user/
    （表头注契约——挪用方自行解析相对基准，本函数即「挪用方」）；条目文件
    构建树存在性逐条对账；launcher 须在终表。排序 = 整条 name=path 字符串
    （mkcpio CLI 现役语义——确定性单点，勿改成按名排序再拼接）。
    """
    try:
        base = load_manifest(base_manifest)
    except SystemExit as e:  # mkcpio 拒面是 SystemExit——转本工具 Reject 单一口径
        raise Reject(f"基座条目表加载失败：{e}")
    base_apps = {n: p for n, p in base.items() if n.startswith("app/")}
    missing = [e for e in prod.preset if e not in base_apps]
    if missing:
        raise Reject(f"product.apps.preset: 条目不在基座表 {missing}"
                     f"（tools/rootfs-manifest.toml app/ 条目——v0 无 OEM 三包输入，"
                     f"新增/替换归 V13 后续刀）")
    final = {n: p for n, p in base.items() if not n.startswith("app/")}
    n_base = len(final)
    for entry in prod.preset:
        final[entry] = base_apps[entry]
    for name in sorted(final):
        local = os.path.join(user_dir, final[name])
        if not os.path.isfile(local):
            raise Reject(f"rootfs 条目 {name}: 构建树缺席 {local}"
                         f"（基座表 path 相对 user/——先 just build-user）")
    if prod.launcher not in final:
        raise Reject(f"product.defaults.launcher: {prod.launcher!r} 不在 rootfs 终表"
                     f"（须指向预装集 app/ 条目或基座 bin/ 条目——schema §1.4）")
    return sorted(f"{n}={p}" for n, p in final.items()), n_base


def build_plan(prod: Product, items: list, n_base: int, pr: int,
               user_disp: str) -> list:
    """dry-run 组装计划行（步④⑥⑦参数集 + 步⑧锚回显；无时刻输入——确定性）。"""
    lines = [
        f"berry-assemble: 产品 {prod.name}（vendor={prod.vendor} board={prod.board}"
        f" arch={prod.arch} feature_level={prod.feature_level}"
        f" build_type={prod.build_type} sideload={prod.sideload}）",
        f"berry-assemble: rootfs 条目 {len(items)} 条 = 基座 {n_base}"
        f"（bin/*+selftest/*）⊕ preset {len(prod.preset)}（app/*）——"
        f"排序 = 整条 name=path（mkcpio CLI 现役语义，步④）",
    ]
    lines += [f"  {item.partition('=')[0]} = {user_disp}/{item.partition('=')[2]}"
              for item in items]
    lines.append(
        f"berry-assemble: mkcpio 参数集（步⑥）: python3 tools/mkcpio.py <out.cpio>"
        f" <{len(items)} 条 name=path·见上清单> --split {prod.rootfs_split_mib}"
        f"（单编码器复用，禁第二份 newc 逻辑）")
    lines.append(
        f"berry-assemble: mkimage 参数集（步⑦）: python3 tools/mkimage.py"
        f" out/{prod.name}.img --efi <配方注入位> --hello <配方注入位>"
        f" --kernel <配方注入位> --initrd <配方注入位>"
        f" --size-mib {prod.image_size_mib} --platform-revision {pr}")
    lines.append(
        f"berry-assemble:   --platform-revision {pr}（V9 透传：manifest v1 逐条目"
        f"列，0=平台基线）；--key-hash 缺省 none（QEMU 路不烧，件 5）")
    lines.append(
        f"berry-assemble: kernel 锚（步⑧）: {prod.kernel_name}@sha256:"
        f"{prod.kernel_sha}——v0 只验锚形，逐字节度量对账归全产线")
    if prod.params:
        lines.append(f"berry-assemble: boot.params 覆盖 {sorted(prod.params)}"
                     f"（Q2 override 语义——缺省归上游；序列化进 INITRD 段归全产线）")
    lines.append("berry-assemble: dry-run 完——v0 不产件（镜像全产线与 just 配方 "
                 "assemble 归 V13 后续刀/主会话收口）")
    return lines


def _validate_all(path: str, base_manifest: str, user_dir: str):
    prod = load_product(path)
    items, n_base = resolve_entries(prod, base_manifest, user_dir)
    return prod, items, n_base


def do_check(path: str, base_manifest: str, user_dir: str) -> None:
    prod, _, _ = _validate_all(path, base_manifest, user_dir)
    print(f"berry-assemble: {path} OK（product={prod.name} board={prod.board}"
          f" preset={len(prod.preset)}/{APP_SLOTS}"
          f" size={prod.image_size_mib}MiB split={prod.rootfs_split_mib}MiB）")


def do_dry_run(path: str, pr: int, base_manifest: str, user_dir: str) -> None:
    validate_platform_revision(pr)
    prod, items, n_base = _validate_all(path, base_manifest, user_dir)
    user_disp = os.path.relpath(user_dir)
    for line in build_plan(prod, items, n_base, pr, user_disp):
        print(line)


# ---- 内置样例（设计文档 §1.2 顶层形状裁剪至 v0 键集；--sample 产出）----
# 锚值为空串 SHA-256 占位（真值随平台发版工件计算——步⑧）。
SAMPLE_PRODUCT = """\
# product.toml —— berry-assemble v0 内置样例（产品配置层与槽位合同设计-v1.0.md §1.2 裁剪）
schema = 1

[product]                         # —— 自由面：产品定义 ——
name    = "berry-qemu-rich"       # 镜像命名 out/<name>.img
vendor  = "berryos"               # 集成者 lineage（1..=16B [a-z0-9-]）
board   = "qemu-virt"             # v0 板表单值

[platform]                        # —— 合同面：平台档位（v0 存在性/枚举轻校验）——
arch          = "aarch64"
feature_level = "rich"            # Q7：v0 只 rich 在册
build_type    = "eng"
sideload      = false

[platform.boot]
kernel = "KERNEL.ELF@sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
# [platform.boot.params]           # ADR #42 启动参数表落点（Q2；override 语义——缺省归上游）
# sched_slice_hz = 64              # 词表 v1 最小集：sched_slice_hz u32 [16,1024]
# intent_default = "utility"       # 六档词表（ADR #58）

[platform.storage]
image_size_mib      = 96          # 现役值（[48,1024]）
rootfs_split_mib    = 32          # 现役 --split 32（[8,64]）
min_power_atomicity = "p0"        # v1 只 p0 可组（词表 {p0,p1,p2} 先立）

[product.apps]                    # —— 预装集：基座表 app/ 条目 ⊆ 筛选 ——
preset = [
  "app/SETTINGS.bap", "app/FILES.bap", "app/PHOTOS.bap", "app/CAMERA.bap",
  "app/CONTACTS.bap", "app/MMS.bap", "app/BROWSER.bap", "app/APPDEMO.bap",
  "app/APPQUIET.bap",
]

[product.branding]                # —— 品牌资源路径（相对本文件目录）——
wallpaper = "assets/wallpaper.byi"
icon_mask = "assets/mask.byi"

[product.defaults]
launcher = "bin/berry-launcher"   # Q13 过渡值：v1 bin/ 路径合法
"""


# ---- selftest（合成基准全谱：正例 + 拒面逐型；零构建/零仓外写入）----

SELFTEST_BASE_TOML = """\
version = 1

[entries]
"app/A.bap" = "userbins/app/A.bap"
"app/B.bap" = "userbins/app/B.bap"
"bin/berry-launcher" = "cargo-target/release/berry-launcher"
"bin/bsh" = "cargo-target/release/bsh"
"selftest/T.bap" = "userbins/app/T.bap"
"""

# 正例模板（拒绝腿 = 定点替换突变——每腿独立文件独立断言，一型一咬）
SELFTEST_PRODUCT = """\
schema = 1

[product]
name    = "sample-rich"
vendor  = "berryos"
board   = "qemu-virt"

[platform]
arch          = "aarch64"
feature_level = "rich"
build_type    = "eng"
sideload      = false

[platform.boot]
kernel = "KERNEL.ELF@sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

[platform.storage]
image_size_mib      = 96
rootfs_split_mib    = 32
min_power_atomicity = "p0"

[product.apps]
preset = ["app/A.bap"]

[product.branding]
wallpaper = "assets/wallpaper.byi"
icon_mask = "assets/mask.byi"

[product.defaults]
launcher = "bin/berry-launcher"
"""


def selftest() -> int:
    import tempfile

    bad = 0

    def leg(ok: bool, label: str) -> None:
        nonlocal bad
        if ok:
            print(f"  selftest 过（预期） : {label}")
        else:
            print(f"  selftest FAIL: {label}")
            bad += 1

    def leg_reject(label: str, want: str, fn) -> None:
        nonlocal bad
        try:
            fn()
        except Reject as e:
            ok = want in str(e)
            print(f"  selftest 拒（预期） : {label}" + ("" if ok else
                  f"【文案缺 {want!r}：{e}】"))
            if not ok:
                bad += 1
            return
        except SystemExit as e:  # mkcpio 转包面（基座表拒）
            ok = want in str(e)
            print(f"  selftest 拒（预期） : {label}" + ("" if ok else
                  f"【文案缺 {want!r}：{e}】"))
            if not ok:
                bad += 1
            return
        print(f"  selftest FAIL: {label} 被门禁放行")
        bad += 1

    with tempfile.TemporaryDirectory() as td:
        base_path = os.path.join(td, "base.toml")
        with open(base_path, "w", encoding="utf-8") as f:
            f.write(SELFTEST_BASE_TOML)
        user = os.path.join(td, "user")
        for rel in ("userbins/app/A.bap", "userbins/app/B.bap",
                    "userbins/app/T.bap", "cargo-target/release/berry-launcher",
                    "cargo-target/release/bsh"):
            p = os.path.join(user, rel.replace("/", os.sep))
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "wb") as f:
                f.write(b"stub")
        prod_dir = os.path.join(td, "prod")
        os.makedirs(os.path.join(prod_dir, "assets"), exist_ok=True)
        for asset in ("wallpaper.byi", "mask.byi"):
            with open(os.path.join(prod_dir, "assets", asset), "wb") as f:
                f.write(b"BYI1-stub")

        def write_prod(text: str, name="p.toml") -> str:
            # 落点 = prod_dir（branding 相对基准与正例夹具一致；拒面腿只回文案不读资产）
            p = os.path.join(prod_dir, name)
            with open(p, "w", encoding="utf-8") as f:
                f.write(text)
            return p

        good = write_prod(SELFTEST_PRODUCT)

        # ---- 正例：加载面（字段提取 + 锚归一）----
        try:
            prod = load_product(good)
            leg(prod.name == "sample-rich" and prod.vendor == "berryos"
                and prod.board == "qemu-virt", "load 字段（name/vendor/board）")
            leg(prod.preset == ["app/A.bap"] and prod.launcher == "bin/berry-launcher",
                "load 字段（preset/launcher）")
            leg(prod.kernel_name == "KERNEL.ELF" and prod.kernel_sha.islower()
                and len(prod.kernel_sha) == 64 and prod.params == {},
                "load 字段（kernel 锚拆分 + params 缺省空）")
            leg(prod.image_size_mib == 96 and prod.rootfs_split_mib == 32
                and prod.min_power_atomicity == "p0", "load 字段（storage 三键）")
        except Reject as e:
            leg(False, f"正例被拒（模板漂移）：{e}")

        # ---- 正例：解析面（基座 ⊕ preset + 排序 + launcher 对账）----
        try:
            items, n_base = resolve_entries(prod, base_path, user)
            leg(items == sorted([
                "app/A.bap=userbins/app/A.bap",
                "bin/berry-launcher=cargo-target/release/berry-launcher",
                "bin/bsh=cargo-target/release/bsh",
                "selftest/T.bap=userbins/app/T.bap",
            ]) and n_base == 3, "resolve 终表（基座 3 ⊕ preset 1，整条排序）")
        except Reject as e:
            leg(False, f"resolve 正例被拒：{e}")

        # launcher 指向预装集 app/ 条目形（Q13 双前缀的另一臂）
        p2 = write_prod(SELFTEST_PRODUCT.replace(
            'preset = ["app/A.bap"]', 'preset = ["app/A.bap", "app/B.bap"]').replace(
            'launcher = "bin/berry-launcher"', 'launcher = "app/B.bap"'), "p2.toml")
        try:
            prod2 = load_product(p2)
            items2, _ = resolve_entries(prod2, base_path, user)
            leg(len(items2) == 5 and any(i.startswith("app/B.bap=") for i in items2),
                "resolve app/ 形 launcher（preset 双条目）")
        except Reject as e:
            leg(False, f"app/ 形 launcher 正例被拒：{e}")

        # ---- 正例：计划面（mkcpio/mkimage 参数集含 V9 透传）----
        plan = build_plan(prod, items, n_base, 7, "user")
        leg(any("--split 32" in ln and "mkcpio 参数集" in ln for ln in plan),
            "plan mkcpio 参数集（--split 按清单）")
        leg(any("--size-mib 96" in ln and "--platform-revision 7" in ln
               and "mkimage 参数集" in ln for ln in plan), "plan mkimage 参数集（V9 透传）")
        leg(any("KERNEL.ELF@sha256:" in ln for ln in plan), "plan kernel 锚回显")

        # V9 域守卫：u32 全域过、域外拒（mkimage 同口径）
        leg(validate_platform_revision(2**32 - 1) == 2**32 - 1
            and validate_platform_revision(0) == 0, "platform-revision 域界值")
        for bad_pr in (-1, 2**32):
            leg_reject(f"platform-revision 域外 {bad_pr}", "u32 域",
                       lambda bp=bad_pr: validate_platform_revision(bp))

        # ---- 拒面：清单/schema 逐型（label, 旧锚串, 新串, 期望文案——每腿独立
        # 文件独立断言，一型一咬；文案期望钉「带键名」纪律）----
        KERNEL_LINE = ('kernel = "KERNEL.ELF@sha256:e3b0c44298fc1c149afbf4c8996'
                       "fb92427ae41e4649b934ca495991b7852b855\"")
        PARAMS_BLOCK = "\n\n[platform.boot.params]\n"
        legs = [
            ("坏 TOML", 'vendor  = "berryos"', 'vendor  = "berryos',
             "TOML 解析失败"),
            ("未知顶层键", "schema = 1\n", "extra = 1\nschema = 1\n", "未知键"),
            ("schema=2", "schema = 1\n", "schema = 2\n", "schema: 须 = 1"),
            ("schema 字符串", "schema = 1\n", 'schema = "1"\n', "schema: 须 = 1"),
            ("缺 schema", "schema = 1\n\n", "", "schema: 必填缺失"),
            ("坏节名（未知顶层键先咬）", "[product]\n", "[productx]\n", "未知键"),
            ("product 未知键", 'board   = "qemu-virt"', 'boardx  = "qemu-virt"',
             "product: 未知键"),
            ("product 在册未收录（services）", "[product.apps]",
             '[product.services.replace]\nsvc-fs = "app/X.bap"\n\n[product.apps]',
             "未收录"),
            ("name 25B", 'name    = "sample-rich"',
             'name    = "' + "a" * 25 + '"', "product.name"),
            ("name 坏字符", 'name    = "sample-rich"', 'name    = "bad name!"',
             "product.name"),
            ("vendor 17B", 'vendor  = "berryos"', 'vendor  = "' + "a" * 17 + '"',
             "product.vendor"),
            ("board 枚举外", 'board   = "qemu-virt"', 'board   = "virt-qemu"',
             "product.board"),
            ("arch 枚举外", 'arch          = "aarch64"', 'arch          = "riscv64"',
             "platform.arch"),
            ("arch 缺席（存在性）", 'arch          = "aarch64"', "",
             "platform.arch: 必填缺失"),
            ("feature_level 枚举外", 'feature_level = "rich"',
             'feature_level = "lite"', "platform.feature_level"),
            ("build_type 枚举外", 'build_type    = "eng"', 'build_type    = "debug"',
             "platform.build_type"),
            ("sideload 非 bool", "sideload      = false",
             'sideload      = "false"', "platform.sideload"),
            ("user 型 sideload", 'build_type    = "eng"\nsideload      = false',
             'build_type    = "user"\nsideload      = true',
             "user 型下必须缺省或 false"),
            ("boot.kernel 缺", KERNEL_LINE, "", "platform.boot.kernel"),
            ("kernel 锚坏形", KERNEL_LINE, 'kernel = "KERNEL.ELF@md5:0123456789abc"',
             "工件锚形不符"),
            ("params 词表外（Q2）", KERNEL_LINE,
             KERNEL_LINE + PARAMS_BLOCK + "thermal_cap_cpu = 80",
             "不在启动参数词表"),
            ("params sched_slice_hz 域外", KERNEL_LINE,
             KERNEL_LINE + PARAMS_BLOCK + "sched_slice_hz = 15", "sched_slice_hz"),
            ("params intent 六档外", KERNEL_LINE,
             KERNEL_LINE + PARAMS_BLOCK + 'intent_default = "turbo"', "六档词表"),
            ("storage 缺键", 'min_power_atomicity = "p0"', "", "必填缺失"),
            ("image_size_mib 域外", "image_size_mib      = 96",
             "image_size_mib      = 47", "image_size_mib"),
            ("image_size_mib bool", "image_size_mib      = 96",
             "image_size_mib      = true", "应为整数"),
            ("rootfs_split_mib 域外", "rootfs_split_mib    = 32",
             "rootfs_split_mib    = 65", "rootfs_split_mib"),
            ("atomicity 词表外", 'min_power_atomicity = "p0"',
             'min_power_atomicity = "p9"', "枚举外值"),
            ("atomicity p1 只立不可组", 'min_power_atomicity = "p0"',
             'min_power_atomicity = "p1"', "v1 只 p0 可组"),
            ("apps 缺 preset", 'preset = ["app/A.bap"]', "",
             "product.apps.preset"),
            ("preset 前缀外", 'preset = ["app/A.bap"]', 'preset = ["bin/bsh"]',
             "app/ 前缀"),
            ("preset 查重", 'preset = ["app/A.bap"]',
             'preset = ["app/A.bap", "app/A.bap"]', "条目重复"),
            ("preset 越 APP_SLOTS 帽", 'preset = ["app/A.bap"]',
             "preset = [" + ", ".join(f'"app/X{i}.bap"' for i in range(10)) + "]",
             "APP_SLOTS"),
            ("branding 包内不存在", 'wallpaper = "assets/wallpaper.byi"',
             'wallpaper = "assets/missing.byi"', "包内不存在"),
            ("launcher 前缀外", 'launcher = "bin/berry-launcher"',
             'launcher = "sbin/init"', "app/ 或 bin/ 前缀"),
        ]
        for k, (label, old, new, want) in enumerate(legs):
            assert old in SELFTEST_PRODUCT, f"腿锚串缺席：{label}"
            text = SELFTEST_PRODUCT.replace(old, new, 1)
            leg_reject(label, want, lambda t=text: load_product(
                write_prod(t, f"bad{k}.toml")))

        # 在册未收录第二型（locale 键组——§1.5；文案须点名键）
        locale_text = SELFTEST_PRODUCT.replace(
            "[product.defaults]", '[product.defaults.locale]\ntimezone = "UTC"'
            "\n\n[product.defaults]", 1)
        leg_reject("defaults 在册未收录（locale）", "未收录",
                   lambda: load_product(write_prod(locale_text, "locale.toml")))

        # ---- 拒面：解析面（基座对账/构建树存在/launcher 终表对账）----
        leg_reject("preset 不在基座表", "不在基座表",
                   lambda: resolve_entries(
                       load_product(write_prod(SELFTEST_PRODUCT.replace(
                           'preset = ["app/A.bap"]', 'preset = ["app/C.bap"]'),
                           "c.toml")), base_path, user))
        gone = os.path.join(user, "userbins", "app", "A.bap")
        os.rename(gone, gone + ".bak")
        leg_reject("基座文件构建树缺席", "构建树缺席",
                   lambda: resolve_entries(prod, base_path, user))
        os.rename(gone + ".bak", gone)
        leg_reject("launcher 不在终表", "product.defaults.launcher",
                   lambda: resolve_entries(
                       load_product(write_prod(SELFTEST_PRODUCT.replace(
                           'launcher = "bin/berry-launcher"',
                           'launcher = "bin/nolauncher"'), "nl.toml")),
                       base_path, user))

        # ---- 内置样例自证（mkapp --new 自产自证同款——防模板漂移）----
        # 写进 prod_dir（branding 相对基准 = 样例所在目录，assets/ 在席）
        sample_path = os.path.join(prod_dir, "sample.toml")
        with open(sample_path, "w", encoding="utf-8") as f:
            f.write(SAMPLE_PRODUCT)
        try:
            s = load_product(sample_path)
            leg(s.name == "berry-qemu-rich" and len(s.preset) == APP_SLOTS
                and s.launcher == "bin/berry-launcher",
                "内置样例过检（v0 键集全谱 + preset 恰 APP_SLOTS）")
        except Reject as e:
            leg(False, f"内置样例被拒（模板漂移）：{e}")

    if bad:
        print(f"berry-assemble selftest: FAIL x{bad}")
        return 1
    print("berry-assemble: selftest ok")
    return 0


def main() -> int:
    args = sys.argv[1:]
    if args == ["--selftest"]:
        return selftest()
    if args == ["--sample"]:
        sys.stdout.write(SAMPLE_PRODUCT)
        return 0
    pr = 0
    mode = None
    path = None
    i = 0
    while i < len(args):
        if args[i] == "--platform-revision":
            if i + 1 >= len(args):
                sys.stderr.write("berry-assemble: --platform-revision 需要 <N> 参数\n")
                return 2
            try:
                pr = int(args[i + 1])
            except ValueError:
                sys.stderr.write(f"berry-assemble: 坏 --platform-revision 值 "
                                 f"{args[i + 1]!r}\n")
                return 2
            i += 2
            continue
        if args[i] in ("--check", "--dry-run") and i + 1 < len(args):
            if mode is not None:
                sys.stderr.write("berry-assemble: --check/--dry-run 互斥（单模式）\n")
                return 2
            mode, path = args[i], args[i + 1]
            i += 2
            continue
        sys.stderr.write(__doc__)
        return 2
    if mode == "--check":
        try:
            do_check(path, DEFAULT_BASE, DEFAULT_USER_DIR)
        except Reject as e:
            sys.stderr.write(f"berry-assemble: {e}\n")
            return 1
        return 0
    if mode == "--dry-run":
        try:
            do_dry_run(path, pr, DEFAULT_BASE, DEFAULT_USER_DIR)
        except Reject as e:
            sys.stderr.write(f"berry-assemble: {e}\n")
            return 1
        return 0
    sys.stderr.write(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
