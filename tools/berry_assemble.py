#!/usr/bin/env python3
r"""berry_assemble——产品配置层组装器 v0（V13 实施刀，ADR #82「组装不 fork」；
设计权威 = `docs/产品配置层与槽位合同设计-v1.1.md` §1（product schema 全量）
与 §4（九步管线）；底稿 `research/厂商定制化深研/08 号` §3）。

用法：
  berry_assemble.py --check <product.toml>   # 全量校验不产（gen_abi --check 先例）=
                                             #   两段式第二段：结构面（schema/未知键
                                             #   硬拒/类型/值域）⊕ 产物面（branding
                                             #   资产在席/source 两件套深校验/基座对
                                             #   账/kernel 锚 SHA-256 现算对账）——过
                                             #   则一行 OK
  berry_assemble.py --check-lite <product.toml>
                                             # 只结构面 = 两段式第一段：键集/类型/
                                             #   值域——零文件对账（资产/源实例/基座
                                             #   表/锚件缺席可跑；产品编写期快检）
  berry_assemble.py --dry-run <product.toml> [--platform-revision N]
                                            # 校验 + 组装计划（rootfs 条目清单 +
                                            #   mkcpio/mkimage 参数集），不产件；
                                            #   N = 产品声明的平台代次——装配
                                            #   反回滚门：N > 平台 level（读
                                            #   tools/abi.toml 单源）即拒
  berry_assemble.py --emit <product.toml> --out-dir <dir> [--platform-revision N]
                                             # 计划（同 --dry-run）⊕ 最小产件：
                                             #   [platform.boot.params] 非空 →
                                             #   <dir>/etc/boot-params.toml 实件
                                             #   （G5 产品链最后一毫米——42 号
                                             #   risks「产品链接线后需一次端到端
                                             #   复验」的投放件；mkcpio 消费形 =
                                             #   产件行给出的 etc/boot-params.toml
                                             #   =<abs> 整条 name=path）。locale
                                             #   等余投放件仍归全产线（读者未在册
                                             #   ——最小刀不预产）
  berry_assemble.py --sample                 # 打印内置样例 product.toml（体验起点）
  berry_assemble.py --selftest               # 自检：合成基准全谱（正例 + 拒面逐型；
                                             #   零构建依赖、零仓外写入）

两段式校验流程（G4）：--check-lite（结构面——只读清单本身，产品包自含）先行，
  --check/--dry-run（结构 ⊕ 产物面对账，组装前门）收口；层界 = 是否触碰文件
  在席对账（结构面纯键集/类型/值域执法）。

v0 范围（最小可用，不做镜像全产线）——对照九步（设计文档 §4.1）：
  - 步① 清单校验：product 键子集（预装集/品牌资源路径/默认应用）全量执法；
    platform 键 v0 只做存在性 + 枚举轻校验（深合同执法随 #83 落册后的工具升版）。
    `[platform.boot.params]` 词表执法（Q2：键 ⊆ 启动参数词表、未知键硬拒——
    词表 v1 = sched_slice_hz/intent_default/timer_slack 三键，§1.6+CP-11）。
  - 步② 白名单校验·V15 半（收编注记：曾于实施轮二实装、rebase 事故冲掉后
    重放，源 = 21 号 §5 结构记录）：`[product.apps.priv_grants]`
    {app_id → [能力键]} 全量执法——app_id 对账 preset 预装集（白名单只授
    预装件，`app/<id>.bap` 未列即拒）+ issued_by 签发者 lineage（节在则
    必填，1..=16B [a-z0-9-]；保留子键不得撞保留名）。能力键值域 ⊆ 现役
    [caps] 闭集 CAPS_KEYS（berry-manifest Sec::Caps × mkapp [capabilities]
    键集两源对齐）。如实注：设计稿 §1.4 写「值 ⊆ G2/G3 词表（应用规范
    §11.2）」，而 §11.2 是立法文本、仓内无机器可读分级键表——v0 键级闭集
    校验先行，分级校验留开放问题（21 号 P1-5 在册；分级表机器可读化归
    V13 后续刀）。
  - 步①-i18n（G6）：`[product.defaults.locale]` 八键收录（§1.5 定形组——
    从 DEFAULTS_DEFERRED 摘除转正，非推断收录：键组/值域/缺省语义均 §1.5
    在册）：languages BCP 47 well-formed 列表 1..=8 查重 / timezone IANA
    tz ID 形（词表 = iana.org 时区集，不做仓内枚举）/ region·
    regulatory_domain ISO 3166-1 alpha-2 / calendar·first_day_of_week·
    hour_cycle·temperature_unit 枚举；全组键可选（缺省省略时消费方按
    region 查默认表——§1.5 归属规则）。计划面产 etc/locale.toml 条目
    （投放面最小——生成件归全产线；读者 = 设置应用 v2 转正刀，应用规范
    §15.3；完整 i18n 运行时未立项，「键组是预埋不是功能承诺」注记随行）。
  - 步④ rootfs 条目解析：基座条目（tools/rootfs-manifest.toml——V13 数据化
    接口，经 `from mkcpio import load_manifest` 复用，禁第二份加载器）⊕ preset
    条目（app/ 前缀 ⊆ 基座 app 条目——纯新增仍拒，归 #83 键位定形）⊕
    override 替换（G1 键位：[product.apps.override] 终表路径取 OEM 自有件，
    .sig 伴生随新路径同查找——mkcpio `sig_companions` 同名直映规则，本工具
    零第二映射）⊕ trust 锚条目（G2 键位：[product.trust].pk_anchor →
    etc/trust-pk，svcmgr `p3_sig_guard` 消费面同路径约定）；排序 = 整条
    name=path（mkcpio CLI 现役语义）。
  - 步⑦ mkimage 参数集：--size-mib 按清单（platform_revision **不透传**——
    mkimage 自 PR≡level 刀单源读 tools/abi.toml [platform].level，无 CLI
    注入位）；装配反回滚门（版本管理标准 §1 五轨道配对律首件落地、§5 v2
    OEM 配对矩阵前身）= --platform-revision 声明值 ≤ 平台 level：OEM 产品
    代次不得超前平台代次（旧产品装新平台合法、新产品装旧平台拒——G-ver-5
    .bap min_level 装载门同构）。声明位现 = CLI 旗（product.toml [platform]
    键位归 #83 落册后的工具升版——schema 权威在设计文档，本刀不动 docs）；
    kernel 锚形校验（结构面）⊕ 包内件在则 SHA-256 现算对账（G7 步⑧ 前置：
    不等 loud 拒、件缺 = 只形 + 注记——OEM 可先声明后放件；算法随锚标签，
    现役 sha256:64hex）；步⑧ 全产线产件对账另收。
  - 步⑦½ 源实例透传（SDK-2 P2 半刀）：[product.source] 官方源节在席则
    --source-instance 入 mkimage 参数集（两件套入 FAT /SOURCE/——v1.2
    §五镜像内置静态实例）。schema 家归**推断**：设计文档 §1 双键 28 键
    无此节（SDK-2 P2 刀新增，正式落册归 #77/设计文档升版；product 键
    add-only 随组装工具版本化收录——§1.1）；深校验复用 gensourceindex
    索引解析器（TLV 单解释器纪律——不在本工具长第二套）。
  - 步④-G1/G2 键位（38 号演练 P0 主断点收口——G1 自有 .bap 路径手工替换
    12 行脚本 / G2 etc/trust-pk 手工追加，两根产线缝针键位化）：
    `[product.apps.override]` {app_id → 包内 .bap 路径}——限替换 preset
    已声明 app（v1 不做纯新增——**推断标注**：白名单语义取最小，正式定形
    随 #83 键位升版）；`[product.trust].pk_anchor` = 32B raw 公钥锚件路径
    （装载守卫锚 etc/trust-pk 的计划面投放；bootseed 正式形随 P3 策略刀，
    词表外出现即未知键拒）。schema 家归**推断**：设计文档 §1 双键 28 键
    无此二节（G1/G2 刀新增，轻追记 = 设计文档 §1.4-补二——[product.source]
    先例同款；正式落册归 #83/设计文档升版）。
  - 步④-G5（boot.params 投放）：[platform.boot.params] 非空 → 计划面产
    etc/boot-params.toml 条目（etc/locale.toml 投放先例同款「投放面最小」
    ——条目名+内容定形进计划；**--emit 模式 = 最小产件面**：同清单复验后
    实写 <out-dir>/etc/boot-params.toml，格式 = schema=1 头 + 键值对，
    词表 v1 三键直映 BOOT_PARAMS_SPEC，读者 = 内核 bootparams 消费端
    〔kernel/src/bootparams.rs，TOML 子集：u32 裸值 / intent 带引号〕
    ——镜像全产线步⑥（locale 等余投放件 + just 配方接线）仍归 V13
    后续刀）。消费设计 = 设计文档 §1.6-补（内核启动序列早期 cpio 查找 →
    sched_slice_hz 换算 slice_cycles / intent_default 新任务缺省 intent
    ——内核消费端已落〔42 号 §1 七靴验证〕，--emit 补齐工具投放件后
    产品链贯通）。
  - 不做（如实记）：步②余量（V6 组装半——非构建产物无 [blob] 声明=拒；V8
    fallback 回退引用必填——归后续刀）、步⑤conformance 门
    （P5 随 #84）、步⑥⑧⑨⑩产件与对账（镜像全产线）、just 配方 assemble
    （归主会话收口）。

拒面纪律（berry-manifest 七拒同构——未知节/未知键/必填缺失/类型错/值域外）：
  一行理由 + 键名 + 非零退（mkapp Reject 先例，`tools/mkapp.py:165` 起）；未知键
  硬拒分两档——「schema 无此键」（V12）与「schema 在册、本工具 v0 未收录」
  （product 键 add-only 随组装工具版本化，§1.1——两档都拒，文案分档防误诊）。

常量对账锚（mkapp ↔ berry-manifest 镜像同款义务——改上游常量须同步此处）：
  APP_SLOTS=10 / 六档 intent 词表 / ID_MAX=24 同规（product.name）/ lineage
  1..=16B [a-z0-9-]（SDK-2 §二，mkapp SEC_RE 同款）/ caps 闭集 console/fs/net
  （CAPS_KEYS——V15 白名单值域，两源 = berry-manifest Sec::Caps × mkapp
  [capabilities] 键集）；数值域两源 = 设计文档
  §1.3 逐键表 × 现役 justfile/mkimage 参数（image_size_mib [48,1024]——下界
  48 = mkimage 史注最小；rootfs_split_mib [8,64]——现役 32）。

确定性：dry-run 计划只依赖输入文件内容（无时刻输入）——同输入同计划行。
依赖：Python ≥3.11（stdlib tomllib——gen_abi/mkcpio 同先例），零三方依赖。
"""

import hashlib
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
APP_SLOTS = 10         # berry-manifest APP_SLOTS（设计文档 §1.4 引 :120；mkapp 镜像同款）
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

# [platform.boot.params] 词表 v1（§1.6-3；词表本体 add-only、未知键硬拒——
# Q2。restricted/locked 档行随 PM/thermal 域刀增补——v1 词表内无 locked 行，
# locked 键「出现即拒」由未知键拒面先行覆盖）。CP-11（2026-11-15）词表 v1
# 扩第三键 timer_slack（§3.8 机制面——policy 三元组 (slice, intent_default,
# timer_slack) 补全；slackface PolicyTriple 同形）。
BOOT_PARAMS_SPEC = {
    "sched_slice_hz": ("u32", (16, 1024)),  # free 档（缺省归上游编译期常量）
    "intent_default": ("intent", None),     # free 档（六档词表；缺省 utility）
    "timer_slack": ("u32", (0, 999_999_999)),  # free 档（ns；缺省 0=无松弛——
    #   上界 = 内核 parse_u32 九位帽对读，全域 < slackface SLACK_MAINTENANCE_NS
    #   10s 帽；读者 = sched 全局 set_timer_slack_ns → block_current LATE-only
    #   顺延（intent >= UTILITY 档 +slack、交互档恒等））
}

# V15 特权白名单能力键闭集（priv_grants 值域——步②）。对账锚两源同集：
# user/berry-manifest `Sec::Caps` 三键臂（console/fs/net）× tools/mkapp.py
# [capabilities] 键集（console 必填 + fs/net 可选）——改上游任一键集须同步此处。
CAPS_KEYS = ("console", "fs", "net")

NAME_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*")     # 首字符字母数字（§1.4）
VENDOR_RE = re.compile(r"[a-z0-9-]{1,16}")               # lineage（SDK SEC_RE 同款）
ANCHOR_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*@sha256:[0-9a-fA-F]{64}$")

# 各节 v0 键集 + 在册未收录键（§1.2-1.5 全量减 v0 子集——拒面分档的数据源）
TOP_KEYS = {"schema", "product", "platform"}
PRODUCT_KEYS = {"name", "vendor", "board", "apps", "branding", "defaults",
                "source", "trust"}
PRODUCT_DEFERRED = {"services"}                          # [product.services.replace]
APPS_KEYS = {"preset", "priv_grants", "override"}        # G1：override = OEM 三包输入
APPS_DEFERRED = set()                                     # （priv_grants 原在此，已移入 KEYS）
BRANDING_KEYS = {"wallpaper", "icon_mask"}
BRANDING_DEFERRED = {"tokens"}                           # §1.4 token 闭集
DEFAULTS_KEYS = {"launcher", "locale"}                   # G6：locale 八键收录（§1.5）
DEFAULTS_DEFERRED = {"scheduler"}                        # §1.5-i18n 已转正；scheduler 归升版
PLATFORM_KEYS = {"arch", "feature_level", "build_type", "sideload", "boot", "storage"}
BOOT_KEYS = {"kernel", "params"}
STORAGE_KEYS = {"image_size_mib", "rootfs_split_mib", "min_power_atomicity"}
# SDK-2 P2：[product.source] 官方源节（步⑦½ 透传面；schema 家推断——
# 头注「步⑦½」段）。单键 v1 = instance（两件套目录）。
SOURCE_KEYS = {"instance"}

# G6：[product.defaults.locale] i18n/区域化八键（§1.5 定形组——18 号 G1 补空白
# 收编的键位建议表落体；三源对照 = Fuchsia hwinfo/intl FIDL × OpenHarmony i18n）。
# 全组 product 自由面、节内键可选（缺省省略时消费方按 region 查默认表）。
LOCALE_KEYS = {"languages", "timezone", "region", "regulatory_domain",
               "calendar", "first_day_of_week", "hour_cycle", "temperature_unit"}
LANGUAGES_MAX = 8      # languages 列表帽（§1.5 推断帽：语言列表场景足够，防清单膨胀）
CALENDARS = ("gregory",)                    # v0 唯一在册（词表 add-only——§1.5）
WEEKDAYS = ("sun", "mon", "tue", "wed", "thu", "fri", "sat")   # CLDR UTS-35 week data
HOUR_CYCLES = ("h11", "h12", "h23", "h24")  # BCP 47 hc 扩展（§1.5）
TEMP_UNITS = ("celsius", "fahrenheit")      # Fuchsia intl.Profile 同款（§1.5）
# BCP 47 well-formed 形（RFC 5646 简化 ABNF：langtag + extlang + script + region
# + variant + 扩展 + 私有段；大小写不敏感；grandfathered 弃用族不收——新清单
# 不应使用）。头注「步①-i18n」段。
BCP47_RE = re.compile(
    r"x(?:-[a-z0-9]{1,8})+"                              # privateuse 单独形（x-…）
    r"|(?:[a-z]{2,3}(?:-[a-z]{3}){0,3}|[a-z]{4}|[a-z]{5,8})"   # language（+extlang）
    r"(?:-[a-z]{4})?"                                    # script
    r"(?:-(?:[a-z]{2}|[0-9]{3}))?"                        # region
    r"(?:-(?:[a-z0-9]{5,8}|[0-9][a-z0-9]{3}))*"           # variant
    r"(?:-[0-9a-wyz](?:-[a-z0-9]{2,8})+)*"                # 扩展（singleton ≠ x）
    r"(?:-x(?:-[a-z0-9]{1,8})+)?",                        # 私有段后缀
    re.IGNORECASE)
BCP47_LEN_MAX = 35     # RFC 5646 §2.2.9 长帽（tag ≤ 35 字符）
# IANA tz ID 形（Area[/Sub]/Location：段 = [A-Za-z0-9_+-]{1,30}、1..=3 段；形校验
# only——词表 = iana.org 时区集，不做仓内枚举，§1.5 裁定）。
TZ_RE = re.compile(r"^[A-Za-z0-9_+\-]{1,30}(/[A-Za-z0-9_+\-]{1,30}){0,2}$")
REGION_RE = re.compile(r"^[A-Z]{2}$")       # ISO 3166-1 alpha-2（两枚大写字母）
# G2：[product.trust] 信任锚节（步④ 投放面；schema 家推断——头注
# 「步④-G1/G2 键位」段）。最小 sub-key v1 = pk_anchor；bootseed 正式形
# 随 P3 策略刀（38 号 G2 注记——词表 add-only，出现即未知键拒）。
TRUST_KEYS = {"pk_anchor"}
TRUST_PK_LEN = 32           # ed25519 raw 公钥尺寸（mkimage TRUSTPK 同约定）
TRUST_PK_ENTRY = "etc/trust-pk"   # 装载守卫锚归档名（svcmgr cpio_find 同串）
# G5：boot.params 序列化投放归档名——[platform.boot.params] 非空 → 计划面
# etc/boot-params.toml 条目（etc/locale.toml 投放先例同款「投放面最小」：
# 条目名+内容定形进计划，生成件归全产线步⑥）。文件格式 v1 = schema=1 头
# + 键值对（词表 v1 三键直映 BOOT_PARAMS_SPEC；键序 = 字典序——确定性
# 纪律，同输入同计划行）；空 params = 不投放（缺省零扰动）。
# 读者 = 内核启动序列早期 cpio 查找（消费设计 = 设计文档 §1.6-补；
# sched_slice_hz 换算 slice_cycles / intent_default 新任务缺省 intent
# ——内核实施窗 = 调度 lane 收工后，sched/kernel 系并行 lane 占用件）。
BOOT_PARAMS_ENTRY = "etc/boot-params.toml"


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
        self.priv_grants = {}       # app_id → 能力键元组（V15 步② 白名单过检视图）
        self.grants_issued_by = ""  # 白名单签发者 lineage（V8 子字段，节在则必填）
        self.apps_override = {}     # app_id → 包内相对路径（G1：替换 preset 同名件）
        self.trust_pk_anchor = ""   # [product.trust].pk_anchor 包内相对路径（G2）
        self.branding = {}          # 键 → 相对 product.toml 所在目录的路径
        self.launcher = ""
        self.locale = {}           # [product.defaults.locale] 过检视图（G6 八键 → 值；
                                   #   空表 = 节缺省——etc/locale.toml 条目随之不产）
        self.kernel_measured = False   # G7：锚是否已对包内件现算对账（件缺 = False）
        self.source_instance = ""   # [product.source].instance（SDK-2 P2 透传位）
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


def _check_priv_grants(prod: Product, grants) -> None:
    """步②·V15：[product.apps.priv_grants] 特权白名单构建期对账（设计 §1.4/
    §4.1-②；应用规范 §11.2「未列而申请=拒授」——构建期先咬）。

    节可选缺省；节在则 issued_by 必填（V8 签发者 lineage——白名单「谁预置
    谁负责」的机器对应物，与 product.vendor/manifest integrator 同正则——
    三键同域口径）。九型拒面（一型一咬，selftest 逐腿）：节非表/缺
    issued_by/issued_by 值域外/issued_by 作 app_id（值数组臂=保留子键不得
    撞保留名，设计 §1.4）/app_id 不在 preset（白名单只授预装件）/值非数组/
    元素非串/未知能力键/能力键重复；重复 app_id 由 tomllib 键唯一性天然
    执法（selftest 有腿钉）。能力键 ⊆ CAPS_KEYS 现役 [caps] 闭集——分级
    （G2/G3）校验留开放问题（头注如实注）。
    """
    if grants is None:
        return
    if not isinstance(grants, dict):
        raise Reject("product.apps.priv_grants: 应为节（键值表——"
                     "{app_id → [能力键]} + issued_by；schema §1.4）")
    issued = grants.get("issued_by")
    if isinstance(issued, list):  # 值数组臂 = 撞保留名（设计 §1.4 落体）
        raise Reject("product.apps.priv_grants.issued_by: 保留子键不得撞保留名"
                     f"（issued_by = V8 签发者 lineage 子字段，不得作 app_id；"
                     f"got 数组 {issued!r}——设计 §1.4）")
    if not isinstance(issued, str):
        raise Reject("product.apps.priv_grants.issued_by: 必填缺失（节存在时必填"
                     "——白名单「谁预置谁负责」的机器对应物，应用规范 §11.4）")
    if not VENDOR_RE.fullmatch(issued):
        raise Reject(f"product.apps.priv_grants.issued_by: 应为签发者 lineage 串"
                     f"（1..=16B [a-z0-9-]——product.vendor/manifest integrator "
                     f"同域；got {issued!r}）")
    prod.grants_issued_by = issued
    preset_ids = {e[len("app/"):-len(".bap")]: e for e in prod.preset}
    for app_id, caps in grants.items():
        if app_id == "issued_by":
            continue  # 保留子键已在上文单裁（TOML 键唯一，无重复臂）
        if app_id not in preset_ids:
            raise Reject(f"product.apps.priv_grants.{app_id}: 不在 preset 预装集"
                         f"（'app/{app_id}.bap' 未列——白名单只授预装件；"
                         f"应用规范 §11.2 未列而申请=拒授，构建期先咬）")
        if not isinstance(caps, list):
            raise Reject(f"product.apps.priv_grants.{app_id}: 应为能力键数组"
                         f"（got {type(caps).__name__}）")
        for c in caps:
            if not isinstance(c, str):
                raise Reject(f"product.apps.priv_grants.{app_id}: 能力键应为字符串"
                             f"（got {type(c).__name__}）")
            if c not in CAPS_KEYS:
                raise Reject(f"product.apps.priv_grants.{app_id}: 未知能力键 {c!r}"
                             f"（值域 ⊆ 现役 [caps] 闭集 {list(CAPS_KEYS)}——"
                             f"berry-manifest Sec::Caps × mkapp [capabilities] "
                             f"键集两源对齐；G2/G3 分级词表（应用规范 §11.2）"
                             f"为立法文本、仓内无机器可读分级表——v0 键级闭集"
                             f"先行，分级校验留开放问题）")
        dups = sorted({c for c in caps if caps.count(c) > 1})
        if dups:
            raise Reject(f"product.apps.priv_grants.{app_id}: 能力键重复 {dups}"
                         f"（查重——白名单授权面单键单义）")
        prod.priv_grants[app_id] = tuple(caps)


def _check_source_section(prod: Product, sect, src_dir: str,
                          lite: bool = False) -> None:
    """步⑦½：[product.source] 官方源节（SDK-2 P2 透传面——头注「步⑦½」
    段；可选节，节在则执法）。instance = 两件套目录（相对 product.toml
    所在目录）；存在性 + gensourceindex 索引解析深校验（TLV 单解释器
    复用——单源不在本工具长第二套）。lite=True 时只结构面（键集/类型——
    目录在席/两件套/深校验归全量，G4 层界）。"""
    if not isinstance(sect, dict):
        raise Reject("product.source: 应为节（键值表——{instance = \"<dir>\"}；"
                     "schema §推断，SDK-2 v1.2 §五）")
    _reject_unknown("product.source", sect, SOURCE_KEYS, set())
    inst = sect.get("instance")
    if not isinstance(inst, str) or not inst:
        raise Reject("product.source.instance: 应为非空字符串路径（必填——"
                     "两件套目录，gensourceindex --out-dir 产物）")
    prod.source_instance = inst
    if lite:
        return  # 产物面（目录/两件套/TLV 深校验）归 --check——G4 两段式第一段
    d = os.path.join(src_dir, inst)
    if not os.path.isdir(d):
        raise Reject(f"product.source.instance: 目录不存在 {inst!r}"
                     f"（相对 product.toml 所在目录解析——schema §1.4 路径"
                     f"同口径）")
    import gensourceindex as gsi  # 惰性导入（节缺席零耦合）
    for name in (gsi.INDEX_NAME, gsi.KEYS_NAME):
        if not os.path.isfile(os.path.join(d, name)):
            raise Reject(f"product.source.instance: 缺 {name}（两件套 = "
                         f"gensourceindex 产物——索引 + 源钥清单）")
    try:
        with open(os.path.join(d, gsi.INDEX_NAME), "rb") as f:
            gsi.parse_index(f.read())
    except SystemExit as e:
        raise Reject(f"product.source.instance: 索引解析拒：{e}"
                     f"（TLV 单解释器 = gensourceindex——深校验复用）")
    prod.source_instance = inst


def _check_apps_override(prod: Product, sect, lite: bool = False) -> None:
    """G1 键位：[product.apps.override] {app_id → 包内 .bap 路径}——OEM
    自有 .bap 替换 preset 同名件（38 号 §4-G1 主断点收口：终表路径键位化，
    产线去「12 行改写条目表脚本」手工缝针）。

    语义定最小（**推断标注**——正式定形随 #83 键位升版）：override 限替换
    preset 已声明 app（app_id ∈ preset 预装集；基座在册但未进 preset 的 app
    亦拒——「∪ 新件白名单」纯新增臂不采，v1 只做替换），值 = 相对
    product.toml 所在目录的路径（branding §1.4 同口径）+ 包内存在校验。
    .sig 伴生不在此执法——mkcpio `sig_companions` 同名直映规则随新路径
    自动查找（伴生在席即随档、缺席零漂移，`tools/mkcpio.py` 头注）。
    拒面五型（一型一咬，selftest 逐腿）：节非表/非 preset id/值非串/
    包内不存在；重复 app_id 由 tomllib 键唯一性天然执法。
    """
    if sect is None:
        return
    if not isinstance(sect, dict):
        raise Reject("product.apps.override: 应为节（键值表——"
                     "{app_id = \"<path>\"}；G1 键位，schema §1.4-补二推断）")
    preset_ids = {e[len("app/"):-len(".bap")]: e for e in prod.preset}
    for app_id, rel in sect.items():
        if app_id not in preset_ids:
            raise Reject(f"product.apps.override.{app_id}: 不在 preset 预装集"
                         f"（'app/{app_id}.bap' 未列——v1 语义 = 限替换已声明"
                         f" app、不做纯新增〔推断标注，正式定形随 #83〕；"
                         f"基座在册未进 preset 者同拒）")
        if not isinstance(rel, str) or not rel:
            raise Reject(f"product.apps.override.{app_id}: 应为非空字符串路径"
                         f"（OEM 自有 .bap——相对 product.toml 所在目录解析）")
        local = os.path.join(prod.src_dir, rel)
        if not lite and not os.path.isfile(local):
            raise Reject(f"product.apps.override.{app_id}: 包内不存在 {rel!r}"
                         f"（相对 product.toml 所在目录解析——schema §1.4 "
                         f"路径同口径）")
        prod.apps_override[app_id] = rel


def _check_trust_section(prod: Product, sect, src_dir: str,
                         lite: bool = False) -> None:
    """G2 键位：[product.trust] 信任锚节（38 号 §4-G2 主断点收口：装载守卫
    锚 etc/trust-pk 的计划面投放——svcmgr `p3_sig_guard` cpio_find 消费面
    同路径约定，产线去「手工追加条目 + 读源码才知道 32B raw」缝针）。

    最小 sub-key v1 = pk_anchor：32B raw 公钥锚件（ed25519——mkimage
    /TRUSTPK 同约定；存在性 + 恰长校验，读文件实测不猜）。bootseed 正式形
    仍随 P3 策略刀（38 号 G2 注记——svcmgr :5722「测试场景件，正式形 =
    §1.4 bootseed 随策略刀」同源）：词表 add-only，出现即未知键拒。
    拒面五型（selftest 逐腿）：非节/缺 pk_anchor/pk_anchor 非串/包内不
    存在/长度非 32B。
    """
    if not isinstance(sect, dict):
        raise Reject("product.trust: 应为节（键值表——{pk_anchor = \"<path>\"}；"
                     "G2 键位，schema §1.4-补二推断）")
    _reject_unknown("product.trust", sect, TRUST_KEYS, set())
    rel = sect.get("pk_anchor")
    if rel is None:
        raise Reject("product.trust.pk_anchor: 必填缺失（节存在时必填——"
                     "32B raw 公钥锚件路径，相对 product.toml 所在目录）")
    if not isinstance(rel, str) or not rel:
        raise Reject(f"product.trust.pk_anchor: 应为非空字符串路径"
                     f"（got {type(rel).__name__}）")
    prod.trust_pk_anchor = rel
    if lite:
        return  # 产物面（件在席 + 32B 恰长）归 --check——G4 两段式第一段
    local = os.path.join(src_dir, rel)
    if not os.path.isfile(local):
        raise Reject(f"product.trust.pk_anchor: 包内不存在 {rel!r}"
                     f"（相对 product.toml 所在目录解析——schema §1.4 "
                     f"路径同口径）")
    with open(local, "rb") as f:
        blob = f.read()
    if len(blob) != TRUST_PK_LEN:
        raise Reject(f"product.trust.pk_anchor: 须恰 {TRUST_PK_LEN}B raw 公钥"
                     f"（got {len(blob)}B——etc/trust-pk 装载守卫锚约定，"
                     f"svcmgr p3_sig_guard/mkimage TRUSTPK 同口径）")


def _check_locale_section(prod: Product, sect) -> None:
    """G6：[product.defaults.locale] i18n/区域化八键执法（§1.5 定形组——18 号
    G1 补空白收编；全组 product 自由面、节内键可选：缺省省略时消费方按
    region 查默认表，§1.5 归属规则）。可选节，节在则逐键执法：

    - languages：BCP 47 well-formed 串数组 1..=8 项（帽=推断：语言列表
      场景足够）、首项=系统语言（序即语义，不另执法）、大小写不敏感查重
      （BCP 47 标签大小写不敏感）；形校验 = RFC 5646 简化 ABNF（langtag/
      privateuse——grandfathered 弃用族不收，新清单不应使用）。
    - timezone：IANA tz ID 形（Area[/Sub]/Location 段形）——形校验 only，
      词表 = iana.org 时区集、不做仓内枚举（§1.5 裁定；缺省 "UTC" 归消费方）。
    - region / regulatory_domain：ISO 3166-1 alpha-2（两枚大写字母；
      regulatory_domain 同码异义——合规面，v1 消费=登记+设置应用展示）。
    - calendar / first_day_of_week / hour_cycle / temperature_unit：枚举
      （词表 add-only——calendar v0 唯一 gregory 在册，chinese/islamic 等
      随真需求进词表）。
    """
    if not isinstance(sect, dict):
        raise Reject("product.defaults.locale: 应为节（键值表——i18n 八键，"
                     "§1.5：languages/timezone/region/regulatory_domain/"
                     "calendar/first_day_of_week/hour_cycle/temperature_unit）")
    _reject_unknown("product.defaults.locale", sect, LOCALE_KEYS, set())
    loc = {}
    langs = sect.get("languages")
    if langs is not None:
        if not isinstance(langs, list):
            raise Reject("product.defaults.locale.languages: 应为字符串数组"
                         "（BCP 47 well-formed 列表，1..=8 项——§1.5）")
        if not 1 <= len(langs) <= LANGUAGES_MAX:
            raise Reject(f"product.defaults.locale.languages: {len(langs)} 项"
                         f"越 [1,{LANGUAGES_MAX}] 帽（帽=推断：语言列表场景"
                         f"足够，防清单膨胀——§1.5）")
        for i, tag in enumerate(langs):
            if not isinstance(tag, str):
                raise Reject(f"product.defaults.locale.languages[{i}]: 应为"
                             f"字符串（got {type(tag).__name__}）")
            if (len(tag) > BCP47_LEN_MAX
                    or not BCP47_RE.fullmatch(tag)):
                raise Reject(f"product.defaults.locale.languages[{i}]: "
                             f"BCP 47 well-formed 不符 {tag!r}"
                             f"（RFC 5646 langtag/privateuse 形——语言[-文字]"
                             f"[-地区][-变体…]；grandfathered 弃用族不收——§1.5）")
        lowered = [t.lower() for t in langs]
        dups = sorted({t for t in lowered if lowered.count(t) > 1})
        if dups:
            raise Reject(f"product.defaults.locale.languages: 条目重复 {dups}"
                         f"（BCP 47 大小写不敏感查重——§1.5）")
        loc["languages"] = list(langs)
    for key in ("timezone", "region", "regulatory_domain"):
        v = sect.get(key)
        if v is None:
            continue
        if not isinstance(v, str) or not v:
            raise Reject(f"product.defaults.locale.{key}: 应为非空字符串"
                         f"（got {type(v).__name__}）")
        if key == "timezone":
            if not TZ_RE.fullmatch(v):
                raise Reject(f"product.defaults.locale.timezone: IANA tz ID "
                             f"形不符 {v!r}（Area[/Sub]/Location 段形〔段内 "
                             f"[A-Za-z0-9_+-]、1..=3 段〕——词表 = iana.org "
                             f"时区集，不做仓内枚举；§1.5）")
        elif not REGION_RE.fullmatch(v):
            raise Reject(f"product.defaults.locale.{key}: 应为 ISO 3166-1 "
                         f"alpha-2（两枚大写字母；got {v!r}——§1.5）")
        loc[key] = v
    for key, vocab in (("calendar", CALENDARS), ("first_day_of_week", WEEKDAYS),
                       ("hour_cycle", HOUR_CYCLES),
                       ("temperature_unit", TEMP_UNITS)):
        v = sect.get(key)
        if v is None:
            continue
        if not isinstance(v, str) or v not in vocab:
            raise Reject(f"product.defaults.locale.{key}: 枚举外值 {v!r}"
                         f"（词表 {list(vocab)}——add-only，§1.5）")
        loc[key] = v
    prod.locale = loc


def _check_product_section(prod: Product, sect: dict, src_dir: str,
                           lite: bool = False) -> None:
    """[product] 自由面 v0 子集（预装集/品牌资源路径/默认应用）。lite=True
    时只结构面——文件在席对账（branding 资产/override 件/trust 件/source
    两件套）全部归全量，G4 两段式层界。"""
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

    # [product.apps].override——OEM 自有件替换（G1 键位；可选节，节在则执法）
    _check_apps_override(prod, apps.get("override"), lite)

    # [product.apps].priv_grants——特权白名单（V15 步②；可选节，节在则执法）
    _check_priv_grants(prod, apps.get("priv_grants"))

    # [product.branding]——品牌资源路径（可选；包内存在校验）
    branding = sect.get("branding", {})
    if not isinstance(branding, dict):
        raise Reject("product.branding: 应为节（键值表）")
    _reject_unknown("product.branding", branding, BRANDING_KEYS, BRANDING_DEFERRED)
    for key, rel in branding.items():
        if not isinstance(rel, str) or not rel:
            raise Reject(f"product.branding.{key}: 应为非空字符串路径")
        if not lite and not os.path.isfile(os.path.join(src_dir, rel)):
            raise Reject(f"product.branding.{key}: 包内不存在 {rel!r}"
                         f"（相对 product.toml 所在目录解析——schema §1.4）")
        prod.branding[key] = rel

    # [product.source]——官方源静态实例（SDK-2 P2 步⑦½；可选节，节在则执法）
    if "source" in sect:
        _check_source_section(prod, sect["source"], src_dir, lite)

    # [product.trust]——信任锚投放（G2 键位；可选节，节在则执法）
    if "trust" in sect:
        _check_trust_section(prod, sect["trust"], src_dir, lite)

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

    # [product.defaults].locale——i18n 八键（G6 收录；§1.5 定形组；可选节）
    if "locale" in defaults:
        _check_locale_section(prod, defaults["locale"])


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


def load_product(path: str, lite: bool = False) -> Product:
    """product.toml → 过检视图（步① 清单校验；TOML 解析 + 双键结构 + 逐节
    执法）。lite=True = 两段式第一段（--check-lite，G4）：只结构面执法
    （键集/类型/值域），文件在席对账（branding/override/trust/source 件）
    全部让位全量面；产物面度量（kernel 锚对账）不在此函数——归
    measure_kernel_anchor。

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
    _check_product_section(prod, _need_table(doc, "product", "product"),
                           prod.src_dir, lite)
    _check_platform_section(prod, _need_table(doc, "platform", "platform"))
    return prod


def measure_kernel_anchor(prod: Product) -> bool:
    """G7 步⑧ 前置：kernel 锚对产品包内实存件 SHA-256 现算对账。

    解析基准 = product.toml 所在目录、件名 = 锚 name 段（branding/
    source.instance 相对路径同款纪律）。三态：
    - 件在且等 → 过（prod.kernel_measured = True，计划面回显「现算对账过」）；
    - 件在而不等 → loud 拒（锚是合同不是装饰——OEM 拿旧锚装新件当场咬）；
    - 件缺 → 只形校验过 + 调用面注记（OEM 可先声明后放件：清单先行、
      工件后置的合法时序；放件后 --check 即现算对账）。
    算法随锚标签（ANCHOR_RE 现役 sha256:64hex）；全产线步⑧ 的产件级
    度量对账另收。返回值 = 是否已度量（False = 件缺未度量）。
    """
    local = os.path.join(prod.src_dir, prod.kernel_name)
    if not os.path.isfile(local):
        prod.kernel_measured = False
        return False
    h = hashlib.sha256()
    with open(local, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    got = h.hexdigest()
    if got != prod.kernel_sha:
        prod.kernel_measured = False
        raise Reject(f"platform.boot.kernel: 锚对账不等——包内 {prod.kernel_name}"
                     f" 现算 sha256:{got} ≠ 声明 sha256:{prod.kernel_sha}"
                     f"（件与锚不符，步⑧ 前置门——G7；改件须同步改锚）")
    prod.kernel_measured = True
    return True


def _platform_level() -> int:
    """平台 API level 单源现值（tools/abi.toml `[platform].level`——版本
    管理标准 §1 平台 level 轨；mkcpio `_berryver_sources`/mkimage
    `platform_level` 同源同法——数据单源在 abi.toml，各工具自携小读器）。
    装配反回滚门的比对基准；坏源一律 Reject loud（基准注入口炸好过
    静默放行）。"""
    path = os.path.join(ROOT, "abi.toml")
    try:
        with open(path, "rb") as f:
            doc = tomllib.load(f)
    except OSError as e:
        raise Reject(f"读 {path} 失败：{e}")
    except tomllib.TOMLDecodeError as e:
        raise Reject(f"{path}: TOML 解析失败：{e}")
    level = doc.get("platform", {}).get("level")
    if type(level) is not int:  # bool 是 int 子类，type() 才拦得住
        raise Reject(f"abi.toml [platform].level: 非整数（got {level!r}"
                     f"——装配反回滚门比对基准单源）")
    return level


def validate_platform_revision(pr: int) -> int:
    """--platform-revision 装配门（域界 + 反回滚两臂——V9 透传面升装配
    语义面，PR ≡ 平台 API level 刀）。

    臂① 域界：u32 全域 [0,4294967295]，与 mkimage build_manifest 拒面同
    口径（列宽 10 是 manifest v1 冻结契约的一部分）。
    臂② 反回滚（版本管理标准 §1 配对律首件落地）：声明 PR 须 ≤ 平台
    level（tools/abi.toml [platform].level 单源）——OEM 产品代次不得超前
    平台代次。旧产品装新平台合法（level 单调轨、高位兼容低位——G-ver-5
    .bap min_level 装载门同构）；新产品装旧平台拒（平台代次被回滚）。
    manifest 侧 PR 值不取本参数（mkimage 单源读 level）——本门只裁「装
    配组合合法性」，两轨各守一面。"""
    if type(pr) is not int or not 0 <= pr <= 2**32 - 1:
        raise Reject(f"--platform-revision: 须 u32 域 [0,4294967295]（V9 列宽契约"
                     f"——mkimage 同口径拒；got {pr!r}）")
    level = _platform_level()
    if pr > level:
        raise Reject(f"--platform-revision: 声明 PR {pr} 超前平台 level {level}"
                     f"（tools/abi.toml [platform].level 单源——装配反回滚："
                     f"OEM 产品代次不得超前平台代次，旧产品装新平台合法/新产品装"
                     f"旧平台拒；版本管理标准 §1 配对律）")
    return pr


def resolve_entries(prod: Product, base_manifest: str, user_dir: str):
    """步④ rootfs 条目解析：基座（V13 表）⊕ preset（app/ 条目筛选）⊕
    override 替换（G1）⊕ trust 锚条目（G2）。

    返回 (排序条目列表, 基座非 app 条目数)。边界：preset 条目必须在基座表
    内（纯新增仍拒——归 #83 键位定形；已声明 app 的件替换走
    [product.apps.override]，终表路径取包内解析的绝对路径）；trust 锚在席
    即产 etc/trust-pk 条目（svcmgr 装载守卫消费面同名——现场重锚语义：
    基座表已有同名条目则覆盖）。基座 path 相对 user/（表头注契约——挪用
    方自行解析相对基准，本函数即「挪用方」）；override/trust 路径已在
    load 面按包内基准验存在，此处对账绝对路径直查；条目文件存在性逐条
    对账；launcher 须在终表。排序 = 整条 name=path 字符串（mkcpio CLI
    现役语义——确定性单点，勿改成按名排序再拼接）。
    """
    try:
        base = load_manifest(base_manifest)
    except SystemExit as e:  # mkcpio 拒面是 SystemExit——转本工具 Reject 单一口径
        raise Reject(f"基座条目表加载失败：{e}")
    base_apps = {n: p for n, p in base.items() if n.startswith("app/")}
    missing = [e for e in prod.preset if e not in base_apps]
    if missing:
        raise Reject(f"product.apps.preset: 条目不在基座表 {missing}"
                     f"（tools/rootfs-manifest.toml app/ 条目——纯新增归 "
                     f"#83 键位定形；替换已声明 app 走 [product.apps.override]）")
    final = {n: p for n, p in base.items() if not n.startswith("app/")}
    n_base = len(final)
    for entry in prod.preset:
        app_id = entry[len("app/"):-len(".bap")]
        if app_id in prod.apps_override:  # G1：终表路径取 OEM 自有件（包内绝对）
            final[entry] = os.path.join(prod.src_dir, prod.apps_override[app_id])
        else:
            final[entry] = base_apps[entry]
    if prod.trust_pk_anchor:  # G2：装载守卫锚条目（现场重锚——基座同名即覆盖）
        final[TRUST_PK_ENTRY] = os.path.join(prod.src_dir, prod.trust_pk_anchor)
    for name in sorted(final):
        path = final[name]
        local = path if os.path.isabs(path) else os.path.join(user_dir, path)
        if not os.path.isfile(local):
            why = ("OEM 锚件（override/trust——load 面已验后缺席）"
                   if os.path.isabs(path)
                   else "基座表 path 相对 user/——先 just build-user")
            raise Reject(f"rootfs 条目 {name}: 构建树缺席 {local}（{why}）")
    if prod.launcher not in final:
        raise Reject(f"product.defaults.launcher: {prod.launcher!r} 不在 rootfs 终表"
                     f"（须指向预装集 app/ 条目或基座 bin/ 条目——schema §1.4）")
    return sorted(f"{n}={p}" for n, p in final.items()), n_base


def build_plan(prod: Product, items: list, n_base: int, pr: int, level: int,
               user_disp: str) -> list:
    """dry-run 组装计划行（步④⑥⑦参数集 + 步⑧锚回显；无时刻输入——确定性）。
    pr = 过装配门的产品声明代次；level = tools/abi.toml 平台 level（门基准
    回显——mkimage 侧 PR 即此值，单源无注入位）。"""
    ovr_names = {f"app/{i}.bap": i for i in prod.apps_override}  # G1 替换集
    n_ov = sum(1 for it in items if it.partition("=")[0] in ovr_names)
    n_tr = sum(1 for it in items if it.startswith(TRUST_PK_ENTRY + "="))
    seg = (f"berry-assemble: rootfs 条目 {len(items)} 条 = 基座 {n_base}"
           f"（bin/*+selftest/*）⊕ preset {len(prod.preset)}（app/*）")
    if n_ov:
        seg += f"，其中 {n_ov} 件 override 替换（路径 = OEM 自有件——G1）"
    if n_tr:
        seg += f" ⊕ trust {n_tr}（etc/trust-pk——G2）"
    lines = [
        f"berry-assemble: 产品 {prod.name}（vendor={prod.vendor} board={prod.board}"
        f" arch={prod.arch} feature_level={prod.feature_level}"
        f" build_type={prod.build_type} sideload={prod.sideload}）",
        seg + "——排序 = 整条 name=path（mkcpio CLI 现役语义，步④）",
    ]
    for item in items:
        name, _, path = item.partition("=")
        if os.path.isabs(path):  # override/trust：包内已解析绝对路径（load 面验在）
            tag = "override（OEM 自有件）" if name in ovr_names else "trust 锚"
            lines.append(f"  {name} = {path}  ← {tag}")
        else:
            lines.append(f"  {name} = {user_disp}/{path}")
    if prod.apps_override:
        lines.append(
            f"berry-assemble: apps.override {len(prod.apps_override)} 件"
            f" {sorted(prod.apps_override)}（G1：限替换 preset 已声明 app——"
            f"推断键位，正式定形随 #83；.sig 伴生随新路径同查找 = mkcpio "
            f"sig_companions 同名直映规则，本工具零第二映射）")
    if prod.trust_pk_anchor:
        lines.append(
            f"berry-assemble: trust.pk_anchor → {TRUST_PK_ENTRY} 条目"
            f"（G2：{prod.trust_pk_anchor} 恰 {TRUST_PK_LEN}B raw——svcmgr "
            f"p3_sig_guard 装载守卫锚消费面同路径约定；bootseed 正式形随 "
            f"P3 策略刀，词表外出现即拒）")
    lines.append(
        f"berry-assemble: mkcpio 参数集（步⑥）: python3 tools/mkcpio.py <out.cpio>"
        f" <{len(items)} 条 name=path·见上清单> --split {prod.rootfs_split_mib}"
        f"（单编码器复用，禁第二份 newc 逻辑）")
    lines.append(
        f"berry-assemble: mkimage 参数集（步⑦）: python3 tools/mkimage.py"
        f" out/{prod.name}.img --efi <配方注入位> --hello <配方注入位>"
        f" --kernel <配方注入位> --initrd <配方注入位>"
        f" --size-mib {prod.image_size_mib}"
        + (f" --source-instance {prod.source_instance}"
           if prod.source_instance else ""))
    if prod.source_instance:
        lines.append(
            f"berry-assemble: 源实例透传（SDK-2 P2 步⑦½）: [product.source]"
            f".instance = {prod.source_instance}（两件套入 FAT /SOURCE/——"
            f"v1.2 §五镜像内置官方源静态实例；深校验过 gensourceindex）")
    lines.append(
        f"berry-assemble:   platform_revision 无注入位（mkimage 单源读"
        f" tools/abi.toml [platform].level={level}——V9 裁决 PR ≡ level，"
        f"三方联动 manifest PR == version VMO level == rootfs .berryver"
        f" level）；装配反回滚门：声明 PR {pr} ≤ 平台 level {level} 过"
        f"（产品代次不超前平台代次）；--key-hash 缺省 none（QEMU 路不烧，件 5）")
    if prod.kernel_measured:
        lines.append(
            f"berry-assemble: kernel 锚（步⑧ 前置·G7）: {prod.kernel_name}"
            f"@sha256:{prod.kernel_sha}——包内件在，SHA-256 现算对账过")
    else:
        lines.append(
            f"berry-assemble: kernel 锚（步⑧ 前置·G7）: {prod.kernel_name}"
            f"@sha256:{prod.kernel_sha}——包内件缺 = 只形校验过"
            f"（OEM 可先声明后放件；放件后 --check 即现算对账）")
    if prod.params:
        kv = " ".join(f"{k}={prod.params[k]!r}" for k in sorted(prod.params))
        lines.append(
            f"berry-assemble: boot-params 条目（步④·G5 投放面最小）: "
            f"{BOOT_PARAMS_ENTRY} ← [platform.boot.params] {kv}（格式 = "
            f"schema=1 头 + 键值对、键序 = 字典序；override 语义——缺省归"
            f"上游〔§1.6〕；产件面 = --emit 模式（最小产件，非全产线）；"
            f"读者 = 内核启动序列早期 cpio 查找〔§1.6-补：sched_slice_hz"
            f" 换算 slice_cycles / intent_default 新任务缺省 intent / "
            f"timer_slack 系统缺省松弛（CP-11）——消费端"
            f" 已落〔kernel/src/bootparams.rs〕〕）")
    if prod.locale:
        kv = " ".join(f"{k}={prod.locale[k]!r}" for k in sorted(prod.locale))
        lines.append(
            f"berry-assemble: locale 条目（步④·投放面最小）: etc/locale.toml"
            f" ← [product.defaults.locale] {kv}（生成件归全产线；读者 = "
            f"设置应用 v2 转正刀〔应用规范 §15.3〕——完整 i18n 运行时"
            f"〔berry-intl〕未立项，键组是预埋不是功能承诺，§1.5）")
    lines.append("berry-assemble: 计划完——产件面 = --emit（boot-params 最小产件）；"
                 "镜像全产线与 just 配方 assemble 归 V13 后续刀/主会话收口）")
    return lines


def _validate_all(path: str, base_manifest: str, user_dir: str):
    """全量面（两段式第二段，G4）：结构（load）⊕ 产物（基座对账 +
    kernel 锚现算对账——件缺不拒、注记）。"""
    prod = load_product(path)
    items, n_base = resolve_entries(prod, base_manifest, user_dir)
    measure_kernel_anchor(prod)   # G7：件缺 = False 不拒；件在不等 = loud 拒
    return prod, items, n_base


def do_check(path: str, base_manifest: str, user_dir: str,
             lite: bool = False) -> None:
    """两段式收口（G4）：lite = 第一段（--check-lite，结构面——零文件对账，
    产品编写期快检）；全量 = 第二段（结构 ⊕ 产物：基座对账 + kernel 锚
    现算对账——G7）。"""
    if lite:
        prod = load_product(path, lite=True)
        print(f"berry-assemble: {path} OK[lite·结构面]（product={prod.name} "
              f"board={prod.board} preset={len(prod.preset)}/{APP_SLOTS}"
              f" override={len(prod.apps_override)}"
              f" locale={'on' if prod.locale else 'off'}"
              f" trust={'on' if prod.trust_pk_anchor else 'off'}"
              f"——产物面对账〔branding/override/trust/source 件、基座表、"
              f"kernel 锚度量〕归 --check）")
        return
    prod, _, _ = _validate_all(path, base_manifest, user_dir)
    print(f"berry-assemble: {path} OK（product={prod.name} board={prod.board}"
          f" preset={len(prod.preset)}/{APP_SLOTS}"
          f" override={len(prod.apps_override)}"
          f" size={prod.image_size_mib}MiB split={prod.rootfs_split_mib}MiB"
          f" source={'on' if prod.source_instance else 'off'}"
          f" locale={'on' if prod.locale else 'off'}"
          f" trust={'on' if prod.trust_pk_anchor else 'off'}"
          f" kernel={'对账过' if prod.kernel_measured else '仅形（包内件缺）'}）")


def do_dry_run(path: str, pr: int, base_manifest: str, user_dir: str) -> None:
    pr = validate_platform_revision(pr)   # 域界 + 装配反回滚两臂（loud 拒）
    level = _platform_level()
    prod, items, n_base = _validate_all(path, base_manifest, user_dir)
    user_disp = os.path.relpath(user_dir)
    for line in build_plan(prod, items, n_base, pr, level, user_disp):
        print(line)


def serialize_boot_params(prod: Product) -> bytes:
    """[platform.boot.params] → etc/boot-params.toml 字节（G5 产件面的
    单一序列化点）。格式合同 = 内核消费端 `kernel/src/bootparams.rs`
    `parse`（TOML 子集对读）：`schema = 1` 头 + `key = value` 行，键序 =
    字典序（确定性——同清单同字节），u32 裸数字 / intent 带引号字面量；
    空行/`#` 注释内核侧容忍但本序列化器不产（最小面）。值域已在
    load_product 词表执法（BOOT_PARAMS_SPEC）——此处纯直映零再裁。"""
    lines = ["schema = 1"]
    for key in sorted(prod.params):
        val = prod.params[key]
        if isinstance(val, str):        # intent 六档（带引号形）
            lines.append(f'{key} = "{val}"')
        else:                           # u32（裸数字形）
            lines.append(f"{key} = {val}")
    return ("\n".join(lines) + "\n").encode("ascii")


def do_emit(path: str, out_dir: str, pr: int,
            base_manifest: str, user_dir: str) -> None:
    """--emit 最小产件面（G5 产品链最后一毫米——42 号 risks「产品链接线
    后需一次端到端复验」的投放件）：复走 --dry-run 全量复验（装配门 +
    结构/产物面）后实写 <out-dir>/etc/boot-params.toml。

    消费形态（mkcpio 条目形，stdout 末行给出）：把产件以
    `etc/boot-params.toml=<abs>` 整条 name=path 并入 rootfs 条目集——
    与 override/trust 条目同形（mkcpio CLI 现役语义，无第二映射）。
    空 params = 零产件零目录（缺省零扰动合同——内核侧件缺静默缺省的
    投放面配套）。locale 等余投放件不在此产（读者未在册——最小刀，
    全产线归 V13 后续刀）。"""
    pr = validate_platform_revision(pr)
    level = _platform_level()
    prod, items, n_base = _validate_all(path, base_manifest, user_dir)
    user_disp = os.path.relpath(user_dir)
    for line in build_plan(prod, items, n_base, pr, level, user_disp):
        print(line)
    if not prod.params:
        print("berry-assemble: emit 空 params 零产件（缺省零扰动——"
              "不产 etc/boot-params.toml，内核侧静默缺省态）")
        return
    dest = os.path.join(out_dir, BOOT_PARAMS_ENTRY)
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    blob = serialize_boot_params(prod)
    with open(dest, "wb") as f:
        f.write(blob)
    print(f"berry-assemble: 产件 {dest} {len(blob)}B（G5 实件投放——"
          f"mkcpio 条目形：{BOOT_PARAMS_ENTRY}={dest}；读者 = 内核 "
          f"bootparams 消费端〔cpio 定址查找 → eco: bootparams …〕）")


# ---- 内置样例（设计文档 §1.2 顶层形状裁剪至 v0 键集；--sample 产出）----
# 锚值为空串 SHA-256 占位（真值随平台发版工件计算——步⑧）。
SAMPLE_PRODUCT = """\
# product.toml —— berry-assemble v0 内置样例（产品配置层与槽位合同设计-v1.1.md §1.2 裁剪）
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
# sched_slice_hz = 250             # 词表 u32 [16,1024]；desktop/interactive 档推荐 250
#                                  #   （17 号实验：marker 链 9.47→4.48s/吞吐 +26%/QEMU CPU
#                                  #   100→84%——10Hz 缺省 GUI 档错配；默认值改裁归 owner，
#                                  #   样例只落推荐面。research/显示与加速体系设计/17 号 §4）
# intent_default = "utility"       # 六档词表（ADR #58）；节非空 → 计划面产
#                                  #   etc/boot-params.toml 条目（G5——内核启动序列读）
# timer_slack = 50000000           # 词表 v1 扩第三键（CP-11）：u32 ns [0,999999999]
#                                  #   系统缺省 timer slack——非交互档（utility 及以下）
#                                  #   到期顺延上界、交互档恒等；缺省 0 = 无松弛零漂移

[platform.storage]
image_size_mib      = 96          # 现役值（[48,1024]）
rootfs_split_mib    = 32          # 现役 --split 32（[8,64]）
min_power_atomicity = "p0"        # v1 只 p0 可组（词表 {p0,p1,p2} 先立）

[product.apps]                    # —— 预装集：基座表 app/ 条目 ⊆ 筛选 ——
preset = [
  "app/SETTINGS.bap", "app/FILES.bap", "app/PHOTOS.bap",
  "app/CONTACTS.bap", "app/MMS.bap", "app/BROWSER.bap", "app/APPDEMO.bap",
  "app/APPQUIET.bap", "app/BERRYDEMO.bap", "app/UITEST.bap",
]

[product.apps.priv_grants]        # —— 特权白名单（V15 步②：只授 preset 预装件）——
issued_by = "berryos"             # 签发者 lineage（1..=16B [a-z0-9-]，节在则必填）
SETTINGS = ["console"]            # 能力键 ⊆ [caps] 闭集（console/fs/net）

# [product.apps.override]         # —— G1：OEM 自有 .bap 替换 preset 同名件（可选）——
# SETTINGS = "oem/SETTINGS.bap"   # {app_id → 包内路径}；限替换 preset 已声明 app
#                                 #   （v1 不做纯新增——推断键位，#83 升版定形）；
#                                 #   .sig 伴生随新路径同查找（mkcpio 自动收编）

# [product.trust]                 # —— G2：信任锚投放（可选；bootseed 随 P3 策略刀）——
# pk_anchor = "etc/trust-pk"      # 32B raw 公钥锚件 → rootfs etc/trust-pk 条目
#                                 #   （svcmgr 装载守卫消费面同路径约定）

[product.branding]                # —— 品牌资源路径（相对本文件目录）——
wallpaper = "assets/wallpaper.byi"
icon_mask = "assets/mask.byi"

[product.defaults]
launcher = "bin/berry-launcher"   # Q13 过渡值：v1 bin/ 路径合法

[product.defaults.locale]         # —— i18n/区域化八键（G6 收录；§1.5；可选节）——
languages         = ["zh-Hans-CN"]  # BCP 47 列表（1..=8 项；首项=系统语言）
timezone          = "Asia/Shanghai" # IANA tz ID（形校验；缺省 UTC 归消费方）
region            = "CN"            # ISO 3166-1 alpha-2（文化区域）
regulatory_domain = "CN"            # 同码异义：监管域（合规面）
calendar          = "gregory"       # v0 唯一在册（词表 add-only）
first_day_of_week = "mon"           # {sun,mon,tue,wed,thu,fri,sat}
hour_cycle        = "h23"           # {h11,h12,h23,h24}（BCP 47 hc）
temperature_unit  = "celsius"       # {celsius,fahrenheit}
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

[product.apps.priv_grants]         # V15 步②正例：A 在 preset、键在闭集
issued_by = "oem-main"
A = ["console"]

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
            leg(prod.priv_grants == {"A": ("console",)}
                and prod.grants_issued_by == "oem-main",
                "load 字段（priv_grants 白名单 + issued_by——V15）")
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

        # ---- 正例：计划面（mkcpio 参数集 + mkimage 单源形 + 装配门行）----
        level = _platform_level()
        plan = build_plan(prod, items, n_base, level, level, "user")
        leg(any("--split 32" in ln and "mkcpio 参数集" in ln for ln in plan),
            "plan mkcpio 参数集（--split 按清单）")
        leg(any("--size-mib 96" in ln and "mkimage 参数集" in ln for ln in plan),
            "plan mkimage 参数集（--size-mib 按清单）")
        leg(not any("--platform-revision" in ln for ln in plan),
            "plan 无 --platform-revision 注入位（mkimage 单源读 level）")
        leg(any(f"声明 PR {level} ≤ 平台 level {level}" in ln for ln in plan),
            "plan 装配反回滚门行（声明 PR ≤ level 过）")
        leg(any("KERNEL.ELF@sha256:" in ln for ln in plan), "plan kernel 锚回显")

        # V9 装配门·域界臂：u32 域外拒（mkimage 同口径）+ 域内基线 0 过
        # （level 单调初值 1 ⇒ 0 恒 ≤ level——「旧产品装新平台合法」族下界）。
        leg(validate_platform_revision(0) == 0, "platform-revision 域内基线 0")
        for bad_pr in (-1, 2**32):
            leg_reject(f"platform-revision 域外 {bad_pr}", "u32 域",
                       lambda bp=bad_pr: validate_platform_revision(bp))
        # V9 装配门·反回滚臂正负两钉（PR ≡ level 刀）：同代/旧代过（旧
        # 产品装新平台合法）、超前拒（新产品装旧平台拒——产品代次不得
        # 超前平台代次，版本管理标准 §1 配对律首件落地）。
        leg(validate_platform_revision(level) == level,
            "platform-revision 同代过（PR == level）")
        leg(validate_platform_revision(level - 1) == level - 1,
            "platform-revision 旧代过（PR < level——旧产品装新平台合法）")
        leg_reject(f"platform-revision 超前（{level + 1} > level {level}）",
                   "超前平台 level",
                   lambda: validate_platform_revision(level + 1))

        # ---- 拒面：清单/schema 逐型（label, 旧锚串, 新串, 期望文案——每腿独立
        # 文件独立断言，一型一咬；文案期望钉「带键名」纪律）----
        KERNEL_LINE = ('kernel = "KERNEL.ELF@sha256:e3b0c44298fc1c149afbf4c8996'
                       "fb92427ae41e4649b934ca495991b7852b855\"")
        PARAMS_BLOCK = "\n\n[platform.boot.params]\n"
        GRANTS_BLOCK = ('[product.apps.priv_grants]         # V15 步②正例：A 在 '
                        'preset、键在闭集\n'
                        'issued_by = "oem-main"\n'
                        'A = ["console"]')
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
            ("params timer_slack 域外（CP-11）", KERNEL_LINE,
             KERNEL_LINE + PARAMS_BLOCK + "timer_slack = 1000000000", "timer_slack"),
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
            ("apps 未知键（override 收编后仍硬拒）", 'preset = ["app/A.bap"]',
             'preset = ["app/A.bap"]\ninject = "x"', "product.apps: 未知键"),
            ("preset 前缀外", 'preset = ["app/A.bap"]', 'preset = ["bin/bsh"]',
             "app/ 前缀"),
            ("preset 查重", 'preset = ["app/A.bap"]',
             'preset = ["app/A.bap", "app/A.bap"]', "条目重复"),
            ("preset 越 APP_SLOTS 帽", 'preset = ["app/A.bap"]',
             "preset = [" + ", ".join(f'"app/X{i}.bap"' for i in range(11)) + "]",
             "APP_SLOTS"),
            ("branding 包内不存在", 'wallpaper = "assets/wallpaper.byi"',
             'wallpaper = "assets/missing.byi"', "包内不存在"),
            ("launcher 前缀外", 'launcher = "bin/berry-launcher"',
             'launcher = "sbin/init"', "app/ 或 bin/ 前缀"),
            # ---- V15 步②：priv_grants 九型拒面（一型一咬）----
            ("priv_grants 节非表", GRANTS_BLOCK, 'priv_grants = "x"', "应为节"),
            ("priv_grants 缺 issued_by", GRANTS_BLOCK,
             GRANTS_BLOCK.replace('issued_by = "oem-main"\n', ""),
             "issued_by: 必填缺失"),
            ("issued_by 值域外（lineage）", 'issued_by = "oem-main"',
             'issued_by = "Berry OS!"', "1..=16B"),
            ("issued_by 作 app_id（撞保留名）", 'issued_by = "oem-main"',
             'issued_by = ["net"]', "不得撞保留名"),
            ("priv_grants 非 preset app", 'A = ["console"]', 'C = ["console"]',
             "不在 preset 预装集"),
            ("priv_grants 值非数组", 'A = ["console"]', 'A = "console"',
             "应为能力键数组"),
            ("priv_grants 元素非串", 'A = ["console"]', "A = [1]",
             "能力键应为字符串"),
            ("priv_grants 未知能力键", 'A = ["console"]', 'A = ["push:send"]',
             "未知能力键"),
            ("priv_grants 能力键重复", 'A = ["console"]',
             'A = ["console", "console"]', "能力键重复"),
        ]
        for k, (label, old, new, want) in enumerate(legs):
            assert old in SELFTEST_PRODUCT, f"腿锚串缺席：{label}"
            text = SELFTEST_PRODUCT.replace(old, new, 1)
            leg_reject(label, want, lambda t=text: load_product(
                write_prod(t, f"bad{k}.toml")))

        # 在册未收录余型（scheduler——locale 已 G6 收录转正，拒面只剩此型）
        sched_text = SELFTEST_PRODUCT.replace(
            "[product.defaults]", '[product.defaults.scheduler]\nlatencynice = [0]'
            "\n\n[product.defaults]", 1)
        leg_reject("defaults 在册未收录（scheduler）", "未收录",
                   lambda: load_product(write_prod(sched_text, "sched.toml")))

        # V15：重复 app_id 无专臂——tomllib 键唯一性天然执法（解析面先咬）
        dup_grants = SELFTEST_PRODUCT.replace(
            'A = ["console"]', 'A = ["console"]\nA = ["net"]', 1)
        leg_reject("priv_grants app_id 重复（tomllib 键唯一）", "TOML 解析失败",
                   lambda: load_product(write_prod(dup_grants, "dup.toml")))

        # ---- SDK-2 P2 步⑦½：[product.source] 官方源节（正例 + 拒面逐型）----
        import gensourceindex as gsi
        sid_dir = os.path.join(prod_dir, "sourceinst")
        os.makedirs(sid_dir, exist_ok=True)
        gsi_pk = bytes(range(0xA0, 0xC0))
        gsi_entry = gsi.Entry("P2DEMO", (0, 1, 0), 4, bytes(32), "berryos",
                              None, 1)
        with open(os.path.join(sid_dir, gsi.INDEX_NAME), "wb") as f:
            f.write(gsi.build_index([gsi_entry], 1, gsi_pk))
        with open(os.path.join(sid_dir, gsi.KEYS_NAME), "wb") as f:
            f.write(gsi.build_keys_listing("berryos", gsi_pk, False))
        SRC_BLOCK = '\n\n[product.source]\ninstance = "sourceinst"'
        try:
            ps = load_product(write_prod(SELFTEST_PRODUCT + SRC_BLOCK,
                                         "src.toml"))
            leg(ps.source_instance == "sourceinst",
                "load source.instance（透传位——[product.source] P2 新节）")
            items_s, n_base_s = resolve_entries(ps, base_path, user)
            plan_s = build_plan(ps, items_s, n_base_s, level, level, "user")
            leg(any(f"--source-instance {ps.source_instance}" in ln
                    and "mkimage 参数集" in ln for ln in plan_s),
                "plan mkimage 参数集透传 --source-instance（步⑦½）")
            leg(any("源实例透传" in ln and "FAT /SOURCE/" in ln
                    for ln in plan_s), "plan 源实例透传导言行")
        except Reject as e:
            leg(False, f"source 正例被拒（模板漂移）：{e}")
        # 无节零扰动（缺省 = 不透传——向后兼容面）
        leg(load_product(good).source_instance == ""
            and not any("--source-instance" in ln for ln in plan),
            "无 source 节零透传（缺省向后兼容）")
        empty_dir = os.path.join(prod_dir, "emptysrc")
        os.makedirs(empty_dir, exist_ok=True)
        bad_dir = os.path.join(prod_dir, "badsrc")
        os.makedirs(bad_dir, exist_ok=True)
        with open(os.path.join(bad_dir, gsi.INDEX_NAME), "wb") as f:
            f.write(b"BSRX" + b"\x00" * 200)
        with open(os.path.join(bad_dir, gsi.KEYS_NAME), "wb") as f:
            f.write(b"berryos-source-keys v1 stub\n")
        for label, old, new, want in [
            ("source 未知键", SRC_BLOCK, SRC_BLOCK + '\nextra = 1',
             "product.source: 未知键"),
            ("source 缺两件套", 'instance = "sourceinst"',
             'instance = "emptysrc"', "缺 source-index.tlv"),
            ("source 目录不存在", 'instance = "sourceinst"',
             'instance = "nope-dir"', "目录不存在"),
            ("source instance 非串", 'instance = "sourceinst"',
             "instance = 5", "应为非空字符串路径"),
            ("source 非节", None,
             'board   = "qemu-virt"\nsource = "sourceinst"', "应为节"),
            ("source 坏索引深校验拒", 'instance = "sourceinst"',
             'instance = "badsrc"', "索引解析拒"),
        ]:
            if old == SRC_BLOCK:
                text = SELFTEST_PRODUCT + new
            elif old is None:  # 非节腿：裸键入 [product] 节（不带 SRC_BLOCK
                text = SELFTEST_PRODUCT.replace(new.split("\n")[0],
                                                new, 1)  # ——防表/值撞名）
            else:
                text = (SELFTEST_PRODUCT + SRC_BLOCK).replace(old, new, 1)
            leg_reject(label, want, lambda t=text: load_product(
                write_prod(t, "badsrc.toml")))

        # ---- G1/G2 键位：apps.override + trust.pk_anchor（正例 + 拒面逐型
        # ——38 号 §4 P0 主断点收口：产线两根手工缝针的键位闭环）----
        os.makedirs(os.path.join(prod_dir, "oem"), exist_ok=True)
        with open(os.path.join(prod_dir, "oem", "A.bap"), "wb") as f:
            f.write(b"OEM-A-bap-stub")
        os.makedirs(os.path.join(prod_dir, "trust"), exist_ok=True)
        with open(os.path.join(prod_dir, "trust", "anchor.pk"), "wb") as f:
            f.write(bytes(range(32)))            # 恰 32B raw（ed25519 尺寸）
        with open(os.path.join(prod_dir, "trust", "short.pk"), "wb") as f:
            f.write(b"\x01" * 31)                # 非 32B 拒面腿件
        OVR_BLOCK = '\n\n[product.apps.override]\nA = "oem/A.bap"'
        TRUST_BLOCK = '\n\n[product.trust]\npk_anchor = "trust/anchor.pk"'
        try:
            pg = load_product(write_prod(
                SELFTEST_PRODUCT + OVR_BLOCK + TRUST_BLOCK, "g1g2.toml"))
            leg(pg.apps_override == {"A": "oem/A.bap"}
                and pg.trust_pk_anchor == "trust/anchor.pk",
                "load override/trust（G1/G2 键位提取——推断键位）")
            items_g, n_base_g = resolve_entries(pg, base_path, user)
            ov_abs = os.path.join(prod_dir, "oem", "A.bap")
            tp_abs = os.path.join(prod_dir, "trust", "anchor.pk")
            leg(f"app/A.bap={ov_abs}" in items_g
                and f"{TRUST_PK_ENTRY}={tp_abs}" in items_g
                and len(items_g) == 5,
                "resolve 终表（override 路径替换 + etc/trust-pk 条目——G1/G2）")
            leg(not any(i.startswith("app/A.bap=userbins/") for i in items_g),
                "resolve 替换语义（基座同名路径不并存——真替换非叠加）")
            plan_g = build_plan(pg, items_g, n_base_g, level, level, "user")
            leg(any(ln.startswith("  app/A.bap = ") and "override" in ln
                    for ln in plan_g),
                "plan 条目行 override 标注（路径 = OEM 自有件绝对路径）")
            leg(any("apps.override" in ln and "sig_companions" in ln
                    for ln in plan_g),
                "plan override 导言行（.sig 伴生随新路径同查找）")
            leg(any(TRUST_PK_ENTRY in ln and "bootseed" in ln
                    for ln in plan_g),
                "plan trust 导言行（etc/trust-pk + bootseed 随策略刀注记）")
            leg(any("⊕ trust 1" in ln for ln in plan_g),
                "plan 计数行含 trust 分量")
        except Reject as e:
            leg(False, f"G1/G2 正例被拒（模板漂移）：{e}")
        # 无节零扰动（缺省 = 不替换/不产锚条目——向后兼容面）
        leg(prod.apps_override == {} and prod.trust_pk_anchor == ""
            and not any(i.startswith(TRUST_PK_ENTRY + "=") for i in items),
            "无 override/trust 零扰动（缺省向后兼容）")
        for label, old, new, want in [
            # G1 拒面：三主面（非 preset id〔含基座在册未进 preset 的更锐臂〕
            # /路径缺/值非串）+ 节非表
            ("override 非 preset id（B 在基座不在 preset）", 'A = "oem/A.bap"',
             'B = "oem/B.bap"', "不在 preset 预装集"),
            ("override 包内不存在", 'A = "oem/A.bap"',
             'A = "oem/missing.bap"', "包内不存在"),
            ("override 值非串", 'A = "oem/A.bap"', "A = 5",
             "应为非空字符串路径"),
            ("override 节非表", None, 'preset = ["app/A.bap"]\noverride = "x"',
             "应为节"),
            # G2 拒面：bootseed（策略刀前词表外）/路径缺/非 32B/缺必填/非串/非节
            ("trust 未知键（bootseed 随策略刀）",
             'pk_anchor = "trust/anchor.pk"',
             'pk_anchor = "trust/anchor.pk"\nbootseed = "x"',
             "product.trust: 未知键"),
            ("trust pk_anchor 包内不存在", 'pk_anchor = "trust/anchor.pk"',
             'pk_anchor = "trust/missing.pk"', "包内不存在"),
            ("trust pk_anchor 非 32B", 'pk_anchor = "trust/anchor.pk"',
             'pk_anchor = "trust/short.pk"', "须恰 32B"),
            ("trust 缺 pk_anchor", 'pk_anchor = "trust/anchor.pk"', "",
             "必填缺失"),
            ("trust pk_anchor 非串", 'pk_anchor = "trust/anchor.pk"',
             "pk_anchor = 5", "应为非空字符串路径"),
            ("trust 非节", None, 'board   = "qemu-virt"\ntrust = "x"',
             "应为节"),
        ]:
            if old is None:  # 非表/裸键腿：值落进 [product.apps]/[product] 节
                text = SELFTEST_PRODUCT.replace(new.split("\n")[0], new, 1)
            else:
                text = (SELFTEST_PRODUCT + OVR_BLOCK + TRUST_BLOCK).replace(
                    old, new, 1)
            leg_reject(label, want, lambda t=text: load_product(
                write_prod(t, "badg1g2.toml")))
        # G1 重复 app_id 无专臂——tomllib 键唯一性天然执法（priv_grants 同钉）
        dup_ovr = SELFTEST_PRODUCT + OVR_BLOCK + OVR_BLOCK
        leg_reject("override app_id 重复（tomllib 键唯一）", "TOML 解析失败",
                   lambda: load_product(write_prod(dup_ovr, "dupovr.toml")))

        # ---- G6：[product.defaults.locale] 八键收录（正例 + 拒面逐型——
        # 从 DEFAULTS_DEFERRED 摘除转正；§1.5 定形组）----
        LOCALE_BLOCK = ('\n\n[product.defaults.locale]    # G6 正例：zh-Hans-CN 组\n'
                        'languages = ["zh-Hans-CN", "en-US"]\n'
                        'timezone = "Asia/Shanghai"\n'
                        'region = "CN"\n'
                        'regulatory_domain = "CN"\n'
                        'calendar = "gregory"\n'
                        'first_day_of_week = "mon"\n'
                        'hour_cycle = "h23"\n'
                        'temperature_unit = "celsius"\n')
        LOCALIZED = SELFTEST_PRODUCT + LOCALE_BLOCK
        try:
            plo = load_product(write_prod(LOCALIZED, "g6.toml"))
            leg(len(plo.locale) == 8
                and plo.locale["languages"] == ["zh-Hans-CN", "en-US"]
                and plo.locale["timezone"] == "Asia/Shanghai"
                and plo.locale["first_day_of_week"] == "mon",
                "load locale 八键（G6 收录——拒转收）")
            items_lo, n_base_lo = resolve_entries(plo, base_path, user)
            plan_lo = build_plan(plo, items_lo, n_base_lo, level, level, "user")
            leg(any("etc/locale.toml" in ln and "设置应用" in ln
                    and "预埋不是功能承诺" in ln for ln in plan_lo),
                "plan etc/locale.toml 条目（投放面最小——读者=设置应用 v2）")
        except Reject as e:
            leg(False, f"G6 正例被拒（模板漂移）：{e}")
        # 无节零扰动（缺省 = 不产 etc/locale.toml 条目——向后兼容面）
        leg(prod.locale == {} and not any("etc/locale.toml" in ln
                                          for ln in plan),
            "无 locale 节零扰动（缺省向后兼容）")
        for k, (label, old, new, want) in enumerate([
            ("locale 节非表", LOCALE_BLOCK, '\n\nlocale = "zh-CN"', "应为节"),
            ("locale 未知键", LOCALE_BLOCK, LOCALE_BLOCK + "extra = 1\n",
             "product.defaults.locale: 未知键"),
            ("languages 非数组", 'languages = ["zh-Hans-CN", "en-US"]',
             'languages = "zh-Hans-CN"', "应为字符串数组"),
            ("languages 空数组", 'languages = ["zh-Hans-CN", "en-US"]',
             "languages = []", "越 [1,8]"),
            ("languages 9 项越帽", 'languages = ["zh-Hans-CN", "en-US"]',
             "languages = [" + ", ".join(
                 f'"{t}"' for t in ("zh", "en", "fr", "de", "ja", "ko",
                                    "ru", "es", "it")) + "]", "越 [1,8] 帽"),
            ("languages 元素非串", '"zh-Hans-CN", "en-US"', '"zh-Hans-CN", 5',
             "应为字符串"),
            ("languages 坏 BCP 47", 'languages = ["zh-Hans-CN", "en-US"]',
             'languages = ["zh_CN"]', "BCP 47"),
            ("languages 查重（大小写不敏感）",
             'languages = ["zh-Hans-CN", "en-US"]',
             'languages = ["zh-Hans-CN", "zh-hans-cn"]', "条目重复"),
            ("timezone 坏形", 'timezone = "Asia/Shanghai"',
             'timezone = "Asia/Shanghai/"', "IANA tz ID 形不符"),
            ("region 坏形（小写）", 'region = "CN"', 'region = "cn"',
             "ISO 3166-1"),
            ("regulatory_domain 坏形（3 字母）",
             'regulatory_domain = "CN"', 'regulatory_domain = "CHN"',
             "ISO 3166-1"),
            ("calendar 枚举外", 'calendar = "gregory"', 'calendar = "chinese"',
             "枚举外值"),
            ("first_day_of_week 枚举外", 'first_day_of_week = "mon"',
             'first_day_of_week = "monday"', "枚举外值"),
            ("hour_cycle 枚举外", 'hour_cycle = "h23"', 'hour_cycle = "h25"',
             "枚举外值"),
            ("temperature_unit 枚举外", 'temperature_unit = "celsius"',
             'temperature_unit = "kelvin"', "枚举外值"),
        ]):
            assert old in LOCALIZED, f"腿锚串缺席：{label}"
            leg_reject(label, want, lambda t=LOCALIZED.replace(old, new, 1):
                       load_product(write_prod(t, f"badg6-{k}.toml")))

        # ---- G5：[platform.boot.params] 序列化投放（正例 + 零扰动；词表
        # 拒面三型已在 legs 表——词表外/域外/六档外，本块不重钉）----
        PARAMS_OK = (SELFTEST_PRODUCT + "\n\n[platform.boot.params]\n"
                     'sched_slice_hz = 64\n'
                     'intent_default = "interactive"\n')
        try:
            ppar = load_product(write_prod(PARAMS_OK, "g5.toml"))
            leg(ppar.params == {"sched_slice_hz": 64,
                                "intent_default": "interactive"},
                "load params 两键（词表 v1 全谱——G5 投放数据源）")
            items_bp, n_base_bp = resolve_entries(ppar, base_path, user)
            plan_bp = build_plan(ppar, items_bp, n_base_bp, level, level,
                                 "user")
            leg(any(BOOT_PARAMS_ENTRY in ln and "schema=1" in ln
                    and "sched_slice_hz=64" in ln
                    and "intent_default='interactive'" in ln
                    for ln in plan_bp),
                "plan etc/boot-params.toml 条目（投放面最小——读者=内核启动序列）")
        except Reject as e:
            leg(False, f"G5 正例被拒（模板漂移）：{e}")
        # 空 params 零扰动（缺省 = 不投放——向后兼容面）
        leg(prod.params == {} and not any(BOOT_PARAMS_ENTRY in ln
                                          for ln in plan),
            "空 params 零投放（缺省向后兼容）")
        # CP-11（词表 v1 扩第三键 timer_slack）：正例 load + 序列化黄金字节
        #（u32 裸数字形——与 sched_slice_hz 同形；G5-3 数据源钉板）。
        PARAMS_OK3 = (SELFTEST_PRODUCT + "\n\n[platform.boot.params]\n"
                      "timer_slack = 50000000\n")
        try:
            p3 = load_product(write_prod(PARAMS_OK3, "g5-3.toml"))
            leg(p3.params == {"timer_slack": 50000000},
                "load params 第三键 timer_slack（词表 v1 扩——CP-11 机制面"
                "数据源；域 [0,999999999]）")
            leg(serialize_boot_params(p3)
                == b"schema = 1\ntimer_slack = 50000000\n",
                "serialize 第三键黄金字节（u32 裸值——内核 TOML 子集合同"
                "对读；0 缺省语义 = 不投放/投放 0 值均合法）")
        except Reject as e:
            leg(False, f"CP-11 正例被拒（词表漂移）：{e}")
        # G5 产件面（--emit）：序列化黄金字节（内核 bootparams parse 合同：
        # schema=1 头 + 键值对、字典序、u32 裸值/intent 带引号）+ do_emit
        # 实写/空 params 零产件——产品链最后一毫米的自检钉。
        leg(serialize_boot_params(ppar)
            == b'schema = 1\nintent_default = "interactive"\n'
               b"sched_slice_hz = 64\n",
            "serialize 黄金字节（字典序 + u32 裸值/intent 带引号——内核"
            "TOML 子集合同对读）")
        emit_dir = os.path.join(td, "emit-out")
        import io
        import contextlib
        with contextlib.redirect_stdout(io.StringIO()):
            do_emit(write_prod(PARAMS_OK, "g5e.toml"), emit_dir, level,
                    base_path, user)
        emit_path = os.path.join(emit_dir, BOOT_PARAMS_ENTRY)
        leg(os.path.isfile(emit_path)
            and open(emit_path, "rb").read()
            == serialize_boot_params(ppar),
            "do_emit 实写 <out>/etc/boot-params.toml（内容 = 序列化字节）")
        emit_empty = os.path.join(td, "emit-empty")
        with contextlib.redirect_stdout(io.StringIO()):
            do_emit(good, emit_empty, level, base_path, user)
        leg(not os.path.exists(emit_empty),
            "do_emit 空 params 零产件零目录（缺省零扰动）")

        # ---- G4：两段式（lite=结构面；文件在席对账归全量面）----
        lite_text = (SELFTEST_PRODUCT
                     .replace('wallpaper = "assets/wallpaper.byi"',
                              'wallpaper = "assets/missing.byi"')
                     + SRC_BLOCK.replace('instance = "sourceinst"',
                                         'instance = "nope-dir"')
                     + OVR_BLOCK.replace('A = "oem/A.bap"',
                                         'A = "oem/missing.bap"')
                     + TRUST_BLOCK.replace('pk_anchor = "trust/anchor.pk"',
                                           'pk_anchor = "trust/missing.pk"')
                     + LOCALE_BLOCK)
        g4_path = write_prod(lite_text, "g4-lite.toml")
        try:
            plite = load_product(g4_path, lite=True)
            leg(plite.locale.get("region") == "CN"
                and plite.branding.get("wallpaper") == "assets/missing.byi"
                and plite.source_instance == "nope-dir"
                and plite.apps_override == {"A": "oem/missing.bap"}
                and plite.trust_pk_anchor == "trust/missing.pk",
                "check-lite 结构面过（branding/override/trust/source 件缺不触——G4 第一段）")
        except Reject as e:
            leg(False, f"lite 正例被拒：{e}")
        leg_reject("同清单全量面拒（branding 件缺先咬）", "包内不存在",
                   lambda: load_product(g4_path))

        # ---- G7：kernel 锚度量对账（件在→SHA-256 现算；不等→loud 拒；
        # 件缺→只形+注记）----
        NOBRANDING = SELFTEST_PRODUCT.replace(
            '[product.branding]\nwallpaper = "assets/wallpaper.byi"\n'
            'icon_mask = "assets/mask.byi"\n\n', "")
        assert NOBRANDING != SELFTEST_PRODUCT  # 腿锚串在席（模板漂移护栏）
        g7_ok = os.path.join(td, "g7-ok")
        os.makedirs(g7_ok)
        # 模板锚 = sha256("")——空件现算同值（e3b0c442… 即空串摘要）
        with open(os.path.join(g7_ok, "KERNEL.ELF"), "wb"):
            pass
        with open(os.path.join(g7_ok, "p.toml"), "w", encoding="utf-8") as f:
            f.write(NOBRANDING)
        try:
            prod_k = load_product(os.path.join(g7_ok, "p.toml"))
            leg(measure_kernel_anchor(prod_k) is True and prod_k.kernel_measured,
                "G7 件在 → SHA-256 现算对账过")
            plan_k = build_plan(prod_k, [], 0, level, level, "user")
            leg(any("kernel 锚" in ln and "现算对账过" in ln for ln in plan_k),
                "plan kernel 锚对账过行（G7）")
        except Reject as e:
            leg(False, f"G7 对账正例被拒（夹具漂移）：{e}")
        g7_bad = os.path.join(td, "g7-bad")
        os.makedirs(g7_bad)
        with open(os.path.join(g7_bad, "KERNEL.ELF"), "wb") as f:
            f.write(b"not-the-declared-kernel")
        with open(os.path.join(g7_bad, "p.toml"), "w", encoding="utf-8") as f:
            f.write(NOBRANDING)
        g7_bad_p = os.path.join(g7_bad, "p.toml")
        leg_reject("G7 锚对账不等（loud 拒）", "锚对账不等",
                   lambda: measure_kernel_anchor(load_product(g7_bad_p)))
        try:
            load_product(g7_bad_p)  # load 不度量（层界钉——度量归全量面）
            leg(True, "G7 度量归全量面（load 结构面放行件不符——层界钉）")
        except Reject as e:
            leg(False, f"G7 层界腿被 load 拒：{e}")
        leg(measure_kernel_anchor(load_product(good)) is False,
            "G7 件缺 → 只形校验过（OEM 可先声明后放件）")
        leg(any("件缺" in ln and "先声明后放件" in ln for ln in plan),
            "plan kernel 锚件缺注记行（G7）")

        # ---- 拒面：解析面（基座对账/构建树存在/launcher 终表对账）----
        # （白名单键随腿同步换 C——模板含 V15 正例节 A，防 load 面先咬）
        leg_reject("preset 不在基座表", "不在基座表",
                   lambda: resolve_entries(
                       load_product(write_prod(SELFTEST_PRODUCT.replace(
                           'preset = ["app/A.bap"]', 'preset = ["app/C.bap"]').replace(
                           'A = ["console"]', 'C = ["console"]'),
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
                and s.launcher == "bin/berry-launcher"
                and s.priv_grants == {"SETTINGS": ("console",)}
                and s.grants_issued_by == "berryos"
                and s.locale.get("languages") == ["zh-Hans-CN"],
                "内置样例过检（v0 键集全谱 + preset 恰 APP_SLOTS + 白名单节"
                " + locale 八键——G6）")
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
    out_dir = None
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
        if args[i] == "--out-dir" and i + 1 < len(args):
            out_dir = args[i + 1]
            i += 2
            continue
        if args[i] in ("--check", "--check-lite", "--dry-run", "--emit") \
                and i + 1 < len(args):
            if mode is not None:
                sys.stderr.write("berry-assemble: --check/--check-lite/"
                                 "--dry-run/--emit 互斥（单模式）\n")
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
    if mode == "--check-lite":   # G4 两段式第一段：只结构面
        try:
            do_check(path, DEFAULT_BASE, DEFAULT_USER_DIR, lite=True)
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
    if mode == "--emit":         # G5 最小产件面（--out-dir 必随）
        if not out_dir:
            sys.stderr.write("berry-assemble: --emit 需 --out-dir <dir>"
                             "（产件落点——etc/boot-params.toml 所在）\n")
            return 2
        try:
            do_emit(path, out_dir, pr, DEFAULT_BASE, DEFAULT_USER_DIR)
        except Reject as e:
            sys.stderr.write(f"berry-assemble: {e}\n")
            return 1
        return 0
    if out_dir is not None:
        sys.stderr.write("berry-assemble: --out-dir 只随 --emit（计划面模式"
                         "不产件）\n")
        return 2
    sys.stderr.write(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
