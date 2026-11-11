//! # berry-manifest —— BerryOS 应用清单 v0（+ SDK-1a 增量）纯函数解析器
//!
//! `docs/app-0实施设计草案.md` §2/§3（ADR #43/#44：应用 = 纯用户态协议
//! 约定 + cpio 容器 + TOML manifest）。「解析器即产品」家族第 8 例
//! （loader/fsproto/virtio/blkproto/fatfs/netproto/netdproto/tcp 之后）：**零分配、零
//! unsafe、零依赖、输入只借用、no_std**——宿主 svcmgr 是 none target 平像
//! 程序（aarch64-unknown-none，无 std 无 alloc），带 std/alloc 依赖即
//! workspace 构建链编译失败。
//!
//! ## schema v0（§2 冻结）
//!
//! `[app]` 五键必填：id / version / entry / type / intent；
//! `[capabilities]` 一键必填：console（v0 唯一能力词汇）；`[runtime]`
//! 整节可省：stack_pages（缺省 8，1..=64）。type = `"app"` 现役 /
//! `"driver"` 收格式不消费（ADR #82 V1 预埋）；`service` 位不进 v1。
//!
//! ## schema v1 增量（SDK-1a，`docs/SDK-1a实施设计草案.md` §2——
//! add-only：v0 冻结面一字不动，未知键放行永不发生）
//!
//! - `[compat]` 三值版本域：`min`/`target`/`compile` 三键**齐备或齐缺**，
//!   值 = SdkVer（`x.y.z`，每段 1..3 位纯十进制、无前导零、0..=999）。
//!   整节缺席 = `compat: None`（缺省 ≡ 按 SDK_CURRENT 处理，appd 不分
//!   支）。**本解析器不做链比较**——min ≤ target ≤ compile ≤
//!   SDK_CURRENT 是 mkapp 编译期执法（分层执法，parser 不知「当前级」
//!   语义）；appd 运行期只查 `min > SDK_CURRENT`。
//! - `[compat]` 增可选 `min_level`（u32 纯十进制整数，**缺省 0**——VER-01
//!   提案第 4 条「平台 API level 单调化」的包侧记录位）：**独立可选**，
//!   不参三值「齐备或齐缺」裁断（与三值域正交并行、互不替代——三值域
//!   语义一字不动）；但 `[compat]` 在场时三值仍须齐备（仅 `min_level`
//!   孤立在场 = `bad compat`，声明的约束不被静默吞弃）。appd 装载期查
//!   `min_level > BERRY_PLATFORM_LEVEL`（level 常量源 = berry-sys
//!   `abi_gen`，单源 `tools/abi.toml` `[platform]`）；本解析器照旧不做
//!   「当前级」比较（分层执法不变）。
//! - `[capabilities]` 增 `fs`/`net` 两键：作用域字符串数组（恰一元素，
//!   值域整体 trim 容忍与 bool 同口径——规范形门禁拒的两级执法）。
//!   v1 词表闭集 `fs ∈ {"rootfs"}`、`net ∈ {"client"}`——词表外/非数组/
//!   多元素/非双引号串一律 `bad scope`。net **parser 照收**（档 (b)：
//!   mkapp 收、appd loud 拒——「词表 = 镜子」的诚实形态）。
//! - `[security]` 预留节：`profile`/`key_lineage`/`integrator` 三键齐备
//!   或齐缺（ADR #82 V3 起三键化重立法），值 1..=16B `[a-z0-9-]`。纯
//!   占位（v1 appd 解析通过即忽略；签名实现 v2——节/键/值域今天定死，
//!   v2 只能填进这个壳）。
//!
//! ## schema v1 增量（GUI-7，`docs/GUI里程碑规划-2026-10.md` 裁决 F——
//! add-only：三键独立可选，全缺省 = 旧包零漂移）
//!
//! - `[ui]` 整节可省：`icon`/`label`/`views` 三键**独立可选**（无齐备
//!   约束——有图标无视图是常态；空节文本形态由 mkapp 规范形门禁拒，
//!   空 [runtime] 两级执法同款）。节内任何失败统一 `bad ui`（compat/
//!   security 理由单一化同款）。
//! - `icon` = 包内 `icons/` 前缀路径，**产品名 `.byi` 后缀**（宿主 mkapp
//!   端 PNG→BYI1 编码，裁决 B：设备端零解码依赖，线格式 = berry-gfx
//!   `icon.rs` BYI1——ARGB8888 RLE，mkapp 拒面须为 decode 域子集）；
//! - `views` = 包内 `views/` 前缀路径（berryui 声明式资源挂点，DSL 定形
//!   随 SDK-2——v1 只验位不释义）；
//! - `label` = 桌面显示名（1..=LABEL_MAX 可打印 ASCII——parse_string
//!   字符集；CJK 等非 ASCII 显示名随 v2 词面扩展再议）。
//! - 帽/前缀常量 `ICON_PREFIX`/`VIEWS_PREFIX`/`UI_PATH_MAX`/`LABEL_MAX`
//!   单一事实源 = 本 crate，mkapp.py 镜像（双侧同步义务同上）。
//!
//! ## schema v1 增量（同意面先落件二，`docs/AI同意面设计-v1.0.md` §5——
//! add-only：v1 收格式不消费，运行时消费者 v3 才出现）
//!
//! - `[agent-actions]` 整节可省：键 = 动作名（kebab-case：首字符小写
//!   字母、全串 `[a-z0-9-]`、1..=ACTION_NAME_MAX——[security] 值域与
//!   id 首字符律的循例合成），值 = 风险档三词闭集（`RiskTier`：
//!   routine/sensitive/destructive，同意面 §1 三档；判别值有序 u8
//!   1/2/3 对齐 Veda Risk 枚举序——Intent 判别值钉板同款「形状钉子」，
//!   v2/v3 消费点 `as u8` 即得编码）。条目数帽 AGENT_ACTIONS_MAX（零
//!   分配定长槽）；节内任何失败（键字符集/值域/重复动作名/越帽）统一
//!   `bad agent-actions`（compat/security/ui 理由单一化同款，节名入
//!   理由）。空节文本形态解析合法（= 缺席同义空集），mkapp 规范形门禁
//!   拒——空 [runtime]/[ui] 两级执法同款。
//!
//! ## schema v1 增量（ADR #82 厂商定制预埋 V1/V3/V6——add-only：
//! 既有冻结面一字不动，旧包零漂移）
//!
//! - type 词表加 `"driver"` 位（V1）：**收格式不消费**——app 现役、
//!   driver 的拉起语义随驱动模型 v2/devmgr 才出现（[hw] 节未立法 ⇒
//!   mkapp 产面不同步 = 无合法产出路径）；`service` 位不进 v1（04 号
//!   §2.2 双扩案作废，口径从驱动模型 §6）。消费侧闸门 = svcmgr 收集槽
//!   `type != app` 过滤（11 号证伪修正：svcmgr 对 `app/*.bap` 收集槽
//!   全量拉起，parser 放行而不过滤 = driver 包开机被当应用拉起——
//!   parser 放行后 type 的唯一运行期闸门）；解析产物入 `ty: AppType`。
//! - `[security]` 第三键 `integrator`（V3）：系统集成者身份 lineage
//!   （ADR #82 V3 预埋，消费随 #77——签名者身份的消费面在产品包信封，
//!   本键 = manifest 侧记录位），值域同 profile/key_lineage（1..=16B
//!   `[a-z0-9-]`，set_sec 同臂）；「两键齐备或齐缺」不变量三键化重立法。
//! - `[blob]` 载体节（V6 前置）：闭源件最低披露五字段
//!   license/origin/sha256/size/usage（字段名照 09 号 §4 最低披露
//!   五字段族），**现值不消费只校验形态**（1..=BLOB_FIELD_MAX 可打印
//!   ASCII 串）；节在场则五键必全（缺一 `bad blob`——声明完整性）。
//!   正面门禁（无声明 = 拒）随 V13 落 berry-assemble（provenance 只在
//!   组装层存在，11 号 §2-V6）；mkapp 只做本声明校验半。
//!
//! ## 语法子集（冻结到字节级——「编不出就拒绝，不截断」）
//!
//! - 换行 **LF-only**：任何 CR 出现即拒（git autocrlf 检出即触发，build
//!   门卫早炸优于运行期降级）；制表符一律拒；
//! - 整数 = 纯 `[0-9]+`：拒 hex/八进制/二进制前缀、下划线、正负号；
//! - 字符串内**反斜杠一律拒**（v0 无转义语义——`"a\nb"` 解出真实换行属
//!   静默错值类）；字符串限可打印 ASCII 0x20..=0x7E；
//! - `#` 注释：整行与行尾两形态都收（字符串内 `#` 不算注释），注释内容
//!   任意字节（多字节 UTF-8 亦然）——剥离后零语义面；
//! - 间距容忍集：等号两侧空格可省、行首缩进仅空格、节头内名前后各 ≤1
//!   空格；
//! - 空 `[runtime]` 合法（= 全缺省）；节序无关。
//!
//! ## 严格拒面（v0 无宽容忽略档）
//!
//! 未知节 / 未知键 / 重复节 / 重复键 / 必填缺失 / 类型错 / 值域外。未知键
//! loud 失败优于半工作——新键包在旧解析器上炸响 = 显式版本不匹配信号
//! （协商机制随 v2 版本化）。
//!
//! ## 帽常量双侧同步义务
//!
//! §1 尺寸帽表在 tools/mkapp.py（build 期门卫）与本 crate（运行期执法）
//! 各硬编码一份——mkapp 无法 import Rust，其门卫注释以本 crate 常量定义
//! 处为对账锚点；**改帽值须双侧同步**（mkfat → berry-fatfs 头注义务同
//! 款）。SDK_CURRENT/词表/值域同款义务（SDK-1a §2.5：SDK 级单一事实源
//! = 本 crate，mkapp.py 侧镜像）；抬级流程 = 改本 crate 两常量 + mkapp
//! 镜像 + 指南文档。entry 必 `bin/` 前缀（完整包内路径）；消费端按整串
//! 比对 cpio 条目名，无路径语义 = 无遍历面。

#![cfg_attr(not(test), no_std)]

use core::str::from_utf8;

// ---- 帽常量（§1 尺寸帽表；双侧同步义务见模块头注）----

/// manifest.toml ≤ 1024B（字段表绰绰有余；防敌意巨值）。
pub const MANIFEST_MAX: usize = 1024;
/// `.bap` 整包 ≤ 4 MiB（appd 走查几何界，防几何炸弹）。
pub const BAP_MAX: usize = 4 * 1024 * 1024;
/// id ≤ 24B，`[A-Za-z0-9._-]`，首字符字母数字。
pub const ID_MAX: usize = 24;
/// version ≤ 16B（记录不消费）。
pub const VERSION_MAX: usize = 16;
/// entry ≤ 32B，必 `bin/` 前缀（与包内文件一一对应）。
pub const ENTRY_MAX: usize = 32;
/// rootfs 内 app 条目 ≤ 9（appd 静态收集槽；标记 n= 单数字位帽同步）。
pub const APP_SLOTS: usize = 9;
/// stack_pages 缺省 8（std 小程序口径）。
pub const STACK_PAGES_DEFAULT: u32 = 8;
/// stack_pages 下界（0 页栈非法）。
pub const STACK_PAGES_MIN: u32 = 1;
/// stack_pages 上界（hexyl 32 页 = musl 128KiB 惯例之倍）。
pub const STACK_PAGES_MAX: u32 = 64;
// ---- GUI-7 增量帽（裁决 F；mkapp.py 镜像——双侧同步义务见模块头注）----
/// [ui] icon/views 包内路径帽（ENTRY_MAX 同值——包内路径统一帽）。
pub const UI_PATH_MAX: usize = 32;
/// [ui] label 显示名帽（可打印 ASCII，parse_string 字符集）。
pub const LABEL_MAX: usize = 32;
/// [ui] icon 值前缀（包内 icons/ 目录——entry 的 `bin/` 前缀同型）。
pub const ICON_PREFIX: &str = "icons/";
/// [ui] views 值前缀（包内 views/ 目录——berryui 声明式资源挂点）。
pub const VIEWS_PREFIX: &str = "views/";
// ---- 同意面先落件二帽（docs/AI同意面设计-v1.0.md §5；mkapp.py 镜像
// ——双侧同步义务见模块头注）----
/// [agent-actions] 动作名帽（kebab-case：首字符小写字母、全串 [a-z0-9-]）。
pub const ACTION_NAME_MAX: usize = 24;
/// [agent-actions] 条目数帽（零分配定长槽——APP_SLOTS 静态收集槽同思路；
/// MANIFEST_MAX 天然界外再设显式帽，解析产物尺寸可预算）。
pub const AGENT_ACTIONS_MAX: usize = 16;
// ---- ADR #82 厂商定制预埋帽（V6 前置；mkapp.py 镜像——双侧同步义务见
// 模块头注）----
/// [blob] 载体节单字段帽（可打印 ASCII 串——sha256 的 64 位 hex 形天然
/// 恰满帽；**值域精确文法随 berry-assemble/V13 执法刀再立法**，本层只
/// 校验形态——现值不消费）。
pub const BLOB_FIELD_MAX: usize = 64;

/// `ManifestErr` 理由帽——appd 降级行 `eco: app <id> <理由>` 的发射窗按
/// 24B 设计（§5）；超长理由自身截断后接省略号（「截断不 panic」）。
const REASON_MAX: usize = 24;

// ---- Intent：六档冻结词表 ----

/// 应用声明的调度 intent——**词表与判别值镜像 `sched/src/intent.rs`
/// 常量块（INTENT_UNSPECIFIED..INTENT_MAINTENANCE，单一事实源在彼处）**。
/// v0 只记录不消费；运行期 u8 映射表不在此落（避免与 intent.rs 成双表）。
/// 判别值 day-one 定型：v2 消费点 `as u8` 即得正确编码（TPIDR 偏移 /
/// vblk SECTOR_SZ const assert 同款「形状钉子」——无钉子则声明顺序一改
/// = 六档全错、静默错优先级带）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Intent {
    Unspecified = 0x00,
    LatencyCritical = 0x01,
    Interactive = 0x02,
    Utility = 0x03,
    Background = 0x04,
    Maintenance = 0x05,
}

// 编译期钉（首/尾；六变体全量断言在 host 测试 intent_discriminants_pinned）。
const _: () = assert!(Intent::Unspecified as u8 == 0x00);
const _: () = assert!(Intent::Maintenance as u8 == 0x05);

impl Intent {
    /// manifest 小写蛇形词 → 枚举（大小写敏感；野词 None）。
    pub fn from_word(w: &str) -> Option<Self> {
        Some(match w {
            "unspecified" => Intent::Unspecified,
            "latency_critical" => Intent::LatencyCritical,
            "interactive" => Intent::Interactive,
            "utility" => Intent::Utility,
            "background" => Intent::Background,
            "maintenance" => Intent::Maintenance,
            _ => return None,
        })
    }

    /// 枚举 → manifest 小写蛇形词（canonical 重序列化 / 诊断行用）。
    pub fn word(self) -> &'static str {
        match self {
            Intent::Unspecified => "unspecified",
            Intent::LatencyCritical => "latency_critical",
            Intent::Interactive => "interactive",
            Intent::Utility => "utility",
            Intent::Background => "background",
            Intent::Maintenance => "maintenance",
        }
    }
}

// ---- AppType：[app] type 位词表（ADR #82 V1 预埋）----

/// `[app]` type 位——v0 唯一值 `"app"`；ADR #82 V1 加 `"driver"` 位
/// （**收格式不消费**：driver 的拉起语义随驱动模型 v2/devmgr 才出现，
/// [hw] 节未立法 ⇒ mkapp 产面不同步 = 无合法产出路径）。`service` 位
/// 不进 v1（04 号 §2.2 双扩案作废，口径从驱动模型 §6）。消费侧闸门 =
/// svcmgr 收集槽 `AppType::App` 过滤（11 号证伪修正：收集槽全量拉起，
/// parser 放行而不过滤 = driver 包开机被当应用拉起）——parser 放行后
/// type 的唯一运行期闸门。判别值 0x01/0x02 = Intent 判别值钉板同款
/// 「形状钉子」（v2 消费点 `as u8` 即得编码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AppType {
    App = 0x01,
    /// 预埋位：词表收、无消费者（devmgr v2 刀 B 四消费点同步承接）。
    Driver = 0x02,
}

// 编译期钉（首/尾；两变体全量断言在 host 测试 app_type_words_and_discriminants）。
const _: () = assert!(AppType::App as u8 == 0x01);
const _: () = assert!(AppType::Driver as u8 == 0x02);

impl AppType {
    /// manifest 小写词（字节面）→ 枚举（大小写敏感；野词 None）。词表
    /// {app,driver} = ADR #82 预埋登记表 V1 冻结面：driver 收格式不消费
    /// （[hw] 立法前无合法产出路径——svcmgr app_launch 消费侧闸镜像）。
    pub fn from_word(w: &[u8]) -> Option<Self> {
        Some(match w {
            b"app" => AppType::App,
            b"driver" => AppType::Driver,
            _ => return None,
        })
    }

    /// 枚举 → manifest 小写词（canonical 重序列化 / 诊断行用）。
    pub fn word(self) -> &'static str {
        match self {
            AppType::App => "app",
            AppType::Driver => "driver",
        }
    }
}

// ---- RiskTier：同意面三档冻结词表（docs/AI同意面设计-v1.0.md §1/§5）----

/// 动作风险档——同意面 §1 三值有序枚举（判据轴 = 可撤销性 × 影响范围，
/// 不是概率风险）。判别值 1/2/3 对齐 Veda Risk 枚举序（Intent 判别值
/// 钉板同款「形状钉子」：v1 收格式不消费，v2/v3 消费点 `as u8` 即得
/// 正确编码）。词面 = manifest 值域闭集 routine/sensitive/destructive。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RiskTier {
    Routine = 0x01,
    Sensitive = 0x02,
    Destructive = 0x03,
}

// 编译期钉（首/尾；三变体全量断言在 host 测试 risk_tier_words_and_discriminants）。
const _: () = assert!(RiskTier::Routine as u8 == 0x01);
const _: () = assert!(RiskTier::Destructive as u8 == 0x03);

impl RiskTier {
    /// manifest 小写词 → 枚举（大小写敏感；野词 None）。
    pub fn from_word(w: &str) -> Option<Self> {
        Some(match w {
            "routine" => RiskTier::Routine,
            "sensitive" => RiskTier::Sensitive,
            "destructive" => RiskTier::Destructive,
            _ => return None,
        })
    }

    /// 枚举 → manifest 小写词（canonical 重序列化 / 诊断行用）。
    pub fn word(self) -> &'static str {
        match self {
            RiskTier::Routine => "routine",
            RiskTier::Sensitive => "sensitive",
            RiskTier::Destructive => "destructive",
        }
    }
}

// ---- SDK 版本域（SDK-1a §2.1：BerryOS SDK 契约级语义版本——与
// berry_* syscall ABI 号、服务 wire 协议版本无耦合，各自版本化）----

/// 工具链当前 SDK 级（发射形态；与 SDK_CURRENT 双轨同值——
/// sdk_current_shape 测试钉两轨一致）。
pub const SDK_CURRENT_STR: &str = "0.1.0";

/// SDK 版本三元组：derive Ord 按字段声明序 = `(major, minor, patch)`
/// 字典序（appd `min > SDK_CURRENT` 运行检查的比较基元）。
/// 文法（§2.1 冻结）：`seg "." seg "." seg`，seg = "0" | [1-9][0-9]{0,2}
/// （1..3 位纯十进制、无前导零、值 0..=999）——文法天然封顶 11 字符
/// （`999.999.999`），无需另设长度帽。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SdkVer {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

// 形状钉子（TCPS_PATTERN 定值同款）：SDK_CURRENT_STR 必须是合法 SdkVer
// （编译期执法——两轨漂移即断链）。
const _: () = assert!(SdkVer::parse(SDK_CURRENT_STR).is_some());

/// 常量形态的 SDK_CURRENT（比较用）。
pub const SDK_CURRENT: SdkVer = SdkVer { major: 0, minor: 1, patch: 0 };

impl SdkVer {
    /// §2.1 文法解析；**不涉 SDK_CURRENT 比较**（分层执法：链比较是
    /// mkapp 编译期门禁，parser 不知「当前级」语义）。const fn 是形状
    /// 钉子（上行 const assert）的前提——故全用直接索引（`b[i]` 在 const
    /// 语境稳定），不用子切片（Range Index 非 const 稳定，实测钉板）。
    pub const fn parse(s: &str) -> Option<SdkVer> {
        let b = s.as_bytes();
        let mut segs = [0u16; 3];
        let mut si = 0usize; // 已完结段数（'.' 计数）
        let mut start = 0usize; // 当前段起点（字节下标）
        let mut i = 0usize;
        while i < b.len() {
            if b[i] == b'.' {
                if si >= 2 {
                    return None; // 第三个 '.' = 四段
                }
                match parse_seg(b, start, i) {
                    Some(v) => segs[si] = v,
                    None => return None,
                }
                si += 1;
                start = i + 1;
            }
            i += 1;
        }
        if si != 2 {
            return None; // 少于两个 '.' = 两段/裸词
        }
        match parse_seg(b, start, b.len()) {
            Some(v) => segs[2] = v,
            None => return None,
        }
        Some(SdkVer { major: segs[0], minor: segs[1], patch: segs[2] })
    }
}

/// seg 文法（字节区间 `[start, end)`——const 语境无子切片，区间传参）：
/// 1..3 位纯十进制、无前导零（"0" 合法、"00"/"01" 拒）、值 0..=999
/// （3 位上限天然封顶）。调用方保证 end ≥ start。
const fn parse_seg(b: &[u8], start: usize, end: usize) -> Option<u16> {
    let len = end - start;
    if len == 0 || len > 3 {
        return None;
    }
    if len > 1 && b[start] == b'0' {
        return None; // 前导零
    }
    let mut n: u16 = 0;
    let mut i = 0;
    while i < len {
        let d = b[start + i];
        if d < b'0' || d > b'9' {
            return None;
        }
        let dv = (d - b'0') as u16;
        n = match n.checked_mul(10) {
            Some(x) => match x.checked_add(dv) {
                Some(y) => y,
                None => return None,
            },
            None => return None,
        };
        i += 1;
    }
    Some(n)
}

// ---- 解析产物 ----

/// `[compat]` 三值版本域（SDK-1a §2.1）。app 自述 `version` 键（≤16B
/// 自由串）与 SdkVer 无关，勿混淆。三值 = BerryOS SDK 契约级：target =
/// 应用声明按哪一级平台行为契约测试（v1 占位）；min = 运行下界（appd
/// 唯一运行时检查项）；compile = 打包所用工具链级（mkapp 闸「不能面向
/// 未来 SDK 编译」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compat {
    pub min: SdkVer,
    pub target: SdkVer,
    pub compile: SdkVer,
    /// 平台 API level 轨道包侧位（VER-01 提案第 4 条；u32，缺省 0）——
    /// appd 装载期与 `BERRY_PLATFORM_LEVEL` 比较的最低平台级。与三值域
    /// 正交：独立可选、add-only 零义务（旧包缺省 0 = 不设限）。
    pub min_level: u32,
}

/// `[security]` 预留节（SDK-1a §2.3；ADR #82 V3 三键化）——v2 密钥
/// 别名/谱系标识形制占位（如 "ed25519-main" / "root-2026"）；v1 解析
/// 通过即忽略。integrator = 系统集成者身份 lineage（ADR #82 V3 预埋，
/// 消费随 #77——签名者身份的消费面在产品包信封，本键 = manifest 侧
/// 记录位），值域同其余两键。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Security<'a> {
    pub profile: &'a str,
    pub key_lineage: &'a str,
    pub integrator: &'a str,
}

/// `[blob]` 载体节（ADR #82 V6 前置）——闭源件最低披露五字段声明（字段
/// 名照 09 号 §4 最低披露五字段族）。**现值不消费只校验形态**：五字段皆
/// 1..=BLOB_FIELD_MAX 可打印 ASCII 串；值域精确文法（sha256 hex 形 /
/// size 整数形等）与内容对拍随 berry-assemble/V13 执法刀再立法。节在场
/// 则五键必全（声明完整性，缺一 `bad blob`）；「无声明 = 拒」的正面门禁
/// 在组装层（provenance 只在组装层存在——11 号 §2-V6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blob<'a> {
    pub license: &'a str,
    pub origin: &'a str,
    pub sha256: &'a str,
    pub size: &'a str,
    pub usage: &'a str,
}

/// `[ui]` 三键独立可选（GUI-7 裁决 F）：icon/label/views——全缺省 None =
/// v0 旧包零漂移（add-only）。icon/views = 包内路径（前缀/帽执法在
/// `set_ui_path`）；label = 桌面显示名。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ui<'a> {
    /// 包内 `icons/` 前缀 + `.byi` 后缀产品路径（mkapp 端 PNG 源编译）。
    pub icon: Option<&'a str>,
    pub label: Option<&'a str>,
    /// 包内 `views/` 前缀路径（berryui DSL 挂点，语义随 SDK-2）。
    pub views: Option<&'a str>,
}

/// `[agent-actions]` 动作风险档声明（同意面 §5 先落件二）——零分配定长
/// 槽（帽 AGENT_ACTIONS_MAX；APP_SLOTS 静态收集槽同思路）。v1 收格式
/// 不消费（运行时消费者 v3 才出现）；空集 = 节缺席或空节（add-only：
/// v0 旧包路径零漂移）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentActions<'a> {
    slots: [(&'a str, RiskTier); AGENT_ACTIONS_MAX],
    len: usize,
}

impl<'a> AgentActions<'a> {
    /// 空集（Fields 缺省构造用）。
    pub const EMPTY: Self =
        Self { slots: [("", RiskTier::Routine); AGENT_ACTIONS_MAX], len: 0 };

    /// 声明条数（0..=AGENT_ACTIONS_MAX）。
    pub fn len(&self) -> usize {
        self.len
    }

    /// 空集 = 节缺席/空节（缺省形态）。
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 声明序（manifest 文档序保序）迭代——mkapp 规范形重发射同序。
    pub fn iter(&self) -> impl Iterator<Item = (&'a str, RiskTier)> + '_ {
        self.slots[..self.len].iter().copied()
    }

    /// 动作名 → 档位（v3 同意面分类步查询面；帽 16 线性扫即可）。
    pub fn get(&self, name: &str) -> Option<RiskTier> {
        self.iter().find(|&(n, _)| n == name).map(|(_, t)| t)
    }

    /// 定长槽压入（帽满 false = 调用方 loud 拒）。
    fn push(&mut self, name: &'a str, tier: RiskTier) -> bool {
        if self.len >= AGENT_ACTIONS_MAX {
            return false;
        }
        self.slots[self.len] = (name, tier);
        self.len += 1;
        true
    }
}

/// 解析成功的 manifest（全借用——零拷贝切进输入缓冲）。type 位入结构
/// （ADR #82 V1 起二值词表 {app,driver}——app 现役、driver 收格式不
/// 消费）；stack_pages 已合并缺省。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Manifest<'a> {
    pub id: &'a str,
    pub version: &'a str,
    /// 完整包内路径，含 `bin/` 前缀（如 `bin/appdemo`）。
    pub entry: &'a str,
    /// [app] type 位（ADR #82 V1——driver 收格式不消费，svcmgr 收集槽
    /// 以 `AppType::App` 过滤防 driver 包被当应用拉起——app_launch 闸
    /// 同源）。
    pub ty: AppType,
    pub console: bool,
    pub stack_pages: u32,
    pub intent: Intent,
    // ---- SDK-1a 增量（add-only：缺省全 None = v0 旧包路径零漂移）----
    /// 三值齐备或齐缺（整节缺席 = None ≡ 按 SDK_CURRENT 处理）。
    pub compat: Option<Compat>,
    /// 作用域词（值域闭集 {"rootfs"}——词表即类型）。
    pub fs: Option<&'a str>,
    /// 值域闭集 {"client"}（parser 照收；appd 侧 loud 拒——档 (b)）。
    pub net: Option<&'a str>,
    pub security: Option<Security<'a>>,
    /// GUI-7：三键独立可选（全缺省 None——add-only 执法锚同 compat 族）。
    pub ui: Option<Ui<'a>>,
    /// 同意面先落件二：动作风险档声明（缺席/空节 = 空集——add-only
    /// 执法锚同上；v1 收格式不消费）。
    pub agent_actions: AgentActions<'a>,
    /// ADR #82 V6 前置：[blob] 载体节五字段声明（缺席/空节 = None；
    /// 在场必全——现值不消费只校验形态）。
    pub blob: Option<Blob<'a>>,
}

/// 解析失败理由（定长内联，零分配）——appd 降级行按 `reason_str()` 打印。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestErr {
    reason: [u8; REASON_MAX],
    len: usize,
}

impl ManifestErr {
    /// 理由 ≤24B 直存；超长截断到 21B + "…"（UTF-8 三字节）= 恰 24B。
    /// 现役理由全 ASCII，21B 处截断不会劈开多字节字符（防御面：万一
    /// 劈开，`reason_str()` 兜底空串，绝不 panic）。
    fn new(reason: &str) -> Self {
        let b = reason.as_bytes();
        if b.len() <= REASON_MAX {
            let mut r = [0u8; REASON_MAX];
            r[..b.len()].copy_from_slice(b);
            ManifestErr { reason: r, len: b.len() }
        } else {
            let mut r = [0u8; REASON_MAX];
            r[..21].copy_from_slice(&b[..21]);
            r[21] = 0xE2; // U+2026 HORIZONTAL ELLIPSIS
            r[22] = 0x80;
            r[23] = 0xA6;
            ManifestErr { reason: r, len: REASON_MAX }
        }
    }

    pub fn reason_str(&self) -> &str {
        from_utf8(&self.reason[..self.len]).unwrap_or("")
    }
}

fn err(reason: &str) -> ManifestErr {
    ManifestErr::new(reason)
}

// ---- 内部解析器 ----

#[derive(Clone, Copy)]
enum Sec {
    App,
    Caps,
    Run,
    Compat,
    Secu,
    Ui,
    /// [agent-actions]（同意面先落件二——键 = 开放动作名词表）。
    Acts,
    /// [blob]（ADR #82 V6 前置——载体节最低披露五字段）。
    Blob,
}

// ---- SDK-1a 词表/值域（mkapp.py FS_SCOPES/NET_SCOPES/SEC_RE 镜像）----

/// fs 作用域 v1 词表闭集（v2 增 `$appdata` 等 = add-only 扩表）。
const FS_SCOPES: &[&[u8]] = &[b"rootfs"];
/// net 作用域 v1 词表闭集。
const NET_SCOPES: &[&[u8]] = &[b"client"];
/// [security] 值域长度帽（字符集 `[a-z0-9-]`）。
const SEC_VAL_MAX: usize = 16;

struct Fields<'a> {
    id: Option<&'a [u8]>,
    version: Option<&'a [u8]>,
    entry: Option<&'a [u8]>,
    ty: Option<&'a [u8]>,
    intent: Option<Intent>,
    console: Option<bool>,
    stack_pages: Option<u32>,
    // SDK-1a 增量槽（齐备或齐缺的裁断在收尾）
    c_min: Option<SdkVer>,
    c_target: Option<SdkVer>,
    c_compile: Option<SdkVer>,
    // VER-01 提案第 4 条：平台 API level 包侧位（独立可选——不参齐缺裁断）
    c_min_level: Option<u32>,
    fs: Option<&'a [u8]>,
    net: Option<&'a [u8]>,
    sec_profile: Option<&'a [u8]>,
    sec_lineage: Option<&'a [u8]>,
    // ADR #82 V3：[security] 第三键槽（三键齐备或齐缺的裁断在收尾）
    sec_integrator: Option<&'a [u8]>,
    // GUI-7 增量槽（三键独立可选——独立 Option，无齐缺裁断）
    ui_icon: Option<&'a [u8]>,
    ui_label: Option<&'a [u8]>,
    ui_views: Option<&'a [u8]>,
    // 同意面先落件二槽（定长槽 + 在用数——动态键词表，Option 族不适用）
    agent_actions: AgentActions<'a>,
    // ADR #82 V6 前置槽（[blob] 五字段——齐备裁断在收尾）
    blob_license: Option<&'a [u8]>,
    blob_origin: Option<&'a [u8]>,
    blob_sha256: Option<&'a [u8]>,
    blob_size: Option<&'a [u8]>,
    blob_usage: Option<&'a [u8]>,
}

/// 两端去空格（空格是唯一合法空白——CR/tab 已在全文级拒）。
fn trim(s: &[u8]) -> &[u8] {
    let mut a = 0;
    let mut b = s.len();
    while a < b && s[a] == b' ' {
        a += 1;
    }
    while b > a && s[b - 1] == b' ' {
        b -= 1;
    }
    &s[a..b]
}

/// `#` 注释剥离：字符串外首个 `#` 起整段截断（行尾注释；整行注释由此
/// 自然得出——剥离后为空行跳过）。扫描维护引号开关；字符串内 `#` 不算
/// 注释。反斜杠在值层另拒，此处不假设转义。
fn strip_comment(line: &[u8]) -> &[u8] {
    let mut in_str = false;
    for (i, &b) in line.iter().enumerate() {
        match b {
            b'"' => in_str = !in_str,
            b'#' if !in_str => return &line[..i],
            _ => {}
        }
    }
    line
}

/// 双引号字符串：内容非空集、无内嵌引号/反斜杠、可打印 ASCII 0x20..=0x7E。
/// 空串语法合法（语义层再按字段拒）。
fn parse_string(v: &[u8]) -> Option<&[u8]> {
    if v.len() < 2 {
        return None;
    }
    if v[0] != b'"' || v[v.len() - 1] != b'"' {
        return None;
    }
    let inner = &v[1..v.len() - 1];
    for &b in inner {
        if b == b'"' || b == b'\\' {
            return None;
        }
        if !(0x20..=0x7E).contains(&b) {
            return None;
        }
    }
    Some(inner)
}

fn parse_bool(v: &[u8]) -> Option<bool> {
    match v {
        b"true" => Some(true),
        b"false" => Some(false),
        _ => None,
    }
}

/// 纯 `[0-9]+` 十进制 u32：非数字（含 hex 前缀/下划线/正负号/小数点）或
/// 溢出即 None。
fn parse_u32(v: &[u8]) -> Option<u32> {
    if v.is_empty() {
        return None;
    }
    let mut n: u32 = 0;
    for &b in v {
        if !b.is_ascii_digit() {
            return None;
        }
        n = n.checked_mul(10)?.checked_add((b - b'0') as u32)?;
    }
    Some(n)
}

/// id 合法性：1..=ID_MAX、首字符字母数字、全串 `[A-Za-z0-9._-]`。
fn valid_id(id: &[u8]) -> bool {
    if id.is_empty() || id.len() > ID_MAX {
        return false;
    }
    if !id[0].is_ascii_alphanumeric() {
        return false;
    }
    id.iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

fn set_str<'a>(slot: &mut Option<&'a [u8]>, val: &'a [u8]) -> Result<(), ManifestErr> {
    if slot.is_some() {
        return Err(err("dup key"));
    }
    match parse_string(val) {
        Some(s) => {
            *slot = Some(s);
            Ok(())
        }
        None => Err(err("bad value")),
    }
}

/// [compat] 键槽：SdkVer 文法。节内任何不齐/文法错/重复键统一
/// `bad compat`——三键「齐备或齐缺」的执法理由单一化（部分在场是
/// 收尾裁断，键内文法错在此 loud）。
fn set_sdkver(slot: &mut Option<SdkVer>, val: &[u8]) -> Result<(), ManifestErr> {
    if slot.is_some() {
        return Err(err("bad compat"));
    }
    let s = parse_string(val).ok_or_else(|| err("bad compat"))?;
    // parse_string 已验可打印 ASCII，from_utf8 必然成功——防御 unwrap_or。
    let v = SdkVer::parse(from_utf8(s).unwrap_or("")).ok_or_else(|| err("bad compat"))?;
    *slot = Some(v);
    Ok(())
}

/// [compat] `min_level` 槽（VER-01 提案第 4 条·平台 API level 单调化的
/// 包侧位）：u32 纯十进制——节内任何文法错/重复键统一 `bad compat`
///（set_sdkver 理由单一化同款）。独立可选，不参三值齐缺裁断。
fn set_compat_level(slot: &mut Option<u32>, val: &[u8]) -> Result<(), ManifestErr> {
    if slot.is_some() {
        return Err(err("bad compat"));
    }
    *slot = Some(parse_u32(val).ok_or_else(|| err("bad compat"))?);
    Ok(())
}

/// fs/net 作用域槽：`[` 双引号串 `]` 恰一元素 + 词表闭集。值域整体
/// trim 容忍与 bool 同口径（`[ "rootfs" ]` 解析收、规范形门禁拒——
/// 两级执法与 `console =  true` 双空格现状一致）；数组文法/词表外/
/// 多元素一律 `bad scope`。
fn set_scope<'a>(
    slot: &mut Option<&'a [u8]>,
    val: &'a [u8],
    vocab: &[&[u8]],
) -> Result<(), ManifestErr> {
    if slot.is_some() {
        return Err(err("dup key"));
    }
    if val.len() < 2 || val[0] != b'[' || val[val.len() - 1] != b']' {
        return Err(err("bad scope"));
    }
    let inner = trim(&val[1..val.len() - 1]);
    if inner.contains(&b',') {
        return Err(err("bad scope")); // 多元素（v1 恰一词）
    }
    let s = parse_string(inner).ok_or_else(|| err("bad scope"))?;
    if !vocab.contains(&s) {
        return Err(err("bad scope"));
    }
    *slot = Some(s);
    Ok(())
}

/// [security] 键槽：值 1..=SEC_VAL_MAX、字符集 `[a-z0-9-]`——节内任何
/// 不齐/值域外/重复键统一 `bad security`。
fn set_sec<'a>(slot: &mut Option<&'a [u8]>, val: &'a [u8]) -> Result<(), ManifestErr> {
    if slot.is_some() {
        return Err(err("bad security"));
    }
    let s = parse_string(val).ok_or_else(|| err("bad security"))?;
    if s.is_empty()
        || s.len() > SEC_VAL_MAX
        || !s.iter().all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(err("bad security"));
    }
    *slot = Some(s);
    Ok(())
}

/// [blob] 键槽（ADR #82 V6 前置）：值 1..=BLOB_FIELD_MAX、可打印 ASCII
/// （parse_string 字符集）——节内任何失败统一 `bad blob`（compat/
/// security/ui 理由单一化同款）；**只校验形态不消费值**（值域精确文法
/// 与对拍随 berry-assemble/V13）。空节 = 缺席同义（空 [runtime] 先例
/// ——其文本形态由 mkapp 规范形门禁拒，两级执法互补）。
fn set_blob<'a>(slot: &mut Option<&'a [u8]>, val: &'a [u8]) -> Result<(), ManifestErr> {
    if slot.is_some() {
        return Err(err("bad blob"));
    }
    let s = parse_string(val).ok_or_else(|| err("bad blob"))?;
    if s.is_empty() || s.len() > BLOB_FIELD_MAX {
        return Err(err("bad blob"));
    }
    *slot = Some(s);
    Ok(())
}

/// [ui] icon/views 路径槽（GUI-7）：前缀 + 非空名 + UI_PATH_MAX——节内
/// 任何失败统一 `bad ui`（compat/security 理由单一化同款）。
fn set_ui_path<'a>(
    slot: &mut Option<&'a [u8]>,
    val: &'a [u8],
    prefix: &[u8],
) -> Result<(), ManifestErr> {
    if slot.is_some() {
        return Err(err("bad ui"));
    }
    let s = parse_string(val).ok_or_else(|| err("bad ui"))?;
    if s.len() <= prefix.len() || s.len() > UI_PATH_MAX || !s.starts_with(prefix) {
        return Err(err("bad ui"));
    }
    *slot = Some(s);
    Ok(())
}

/// [agent-actions] 键槽（同意面 §5 先落件二）：键 = 动作名（kebab-case：
/// 首字符小写字母、全串 `[a-z0-9-]`、1..=ACTION_NAME_MAX），值 =
/// RiskTier 三词闭集。键字符集已验 ⇒ ASCII ⇒ from_utf8 必然成功
/// （unwrap_or 仅防御，永不 panic 承诺）。节内任何失败统一
/// `bad agent-actions`（compat/security/ui 理由单一化同款——节名入理由）。
fn set_agent_action<'a>(
    aa: &mut AgentActions<'a>,
    key: &'a [u8],
    val: &'a [u8],
) -> Result<(), ManifestErr> {
    const WHY: &str = "bad agent-actions";
    let bad = || err(WHY);
    if key.is_empty() || key.len() > ACTION_NAME_MAX {
        return Err(bad());
    }
    if !key[0].is_ascii_lowercase()
        || !key
            .iter()
            .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(bad());
    }
    let name = from_utf8(key).unwrap_or("");
    if aa.get(name).is_some() {
        return Err(bad()); // 重复动作名（dup key 键粒度同执法）
    }
    let s = parse_string(val).ok_or_else(bad)?;
    let tier = RiskTier::from_word(from_utf8(s).unwrap_or("")).ok_or_else(bad)?;
    if !aa.push(name, tier) {
        return Err(bad()); // 条目越帽 AGENT_ACTIONS_MAX
    }
    Ok(())
}

/// manifest v0 解析主入口。永不 panic；一切拒绝经 `ManifestErr` 带理由。
pub fn parse(bytes: &[u8]) -> Result<Manifest<'_>, ManifestErr> {
    if bytes.len() > MANIFEST_MAX {
        return Err(err("too big"));
    }
    if bytes.contains(&b'\r') {
        return Err(err("CRLF"));
    }
    if bytes.contains(&b'\t') {
        return Err(err("tab"));
    }

    let mut f = Fields {
        id: None,
        version: None,
        entry: None,
        ty: None,
        intent: None,
        console: None,
        stack_pages: None,
        c_min: None,
        c_target: None,
        c_compile: None,
        c_min_level: None,
        fs: None,
        net: None,
        sec_profile: None,
        sec_lineage: None,
        sec_integrator: None,
        ui_icon: None,
        ui_label: None,
        ui_views: None,
        agent_actions: AgentActions::EMPTY,
        blob_license: None,
        blob_origin: None,
        blob_sha256: None,
        blob_size: None,
        blob_usage: None,
    };
    let (mut saw_app, mut saw_caps, mut saw_run, mut saw_compat, mut saw_secu, mut saw_ui) =
        (false, false, false, false, false, false);
    let mut saw_acts = false; // [agent-actions]（同意面先落件二——add-only 加行）
    let mut saw_blob = false; // [blob]（ADR #82 V6 前置——add-only 加行）
    let mut cur: Option<Sec> = None;

    for raw in bytes.split(|&b| b == b'\n') {
        let line = trim(strip_comment(raw));
        if line.is_empty() {
            continue;
        }

        // 节头：[name]，名前后各 ≤1 可选空格；多余空格折出未知节。
        if line[0] == b'[' {
            if line.len() < 2 || line[line.len() - 1] != b']' {
                return Err(err("bad section"));
            }
            let mut inner = &line[1..line.len() - 1];
            if inner.first() == Some(&b' ') {
                inner = &inner[1..];
            }
            if inner.last() == Some(&b' ') {
                inner = &inner[..inner.len() - 1];
            }
            let sec = match inner {
                b"app" => Sec::App,
                b"capabilities" => Sec::Caps,
                b"runtime" => Sec::Run,
                b"compat" => Sec::Compat,
                b"security" => Sec::Secu,
                b"ui" => Sec::Ui,
                b"agent-actions" => Sec::Acts,
                b"blob" => Sec::Blob,
                _ => return Err(err("unknown section")),
            };
            let dup = match sec {
                Sec::App => saw_app,
                Sec::Caps => saw_caps,
                Sec::Run => saw_run,
                Sec::Compat => saw_compat,
                Sec::Secu => saw_secu,
                Sec::Ui => saw_ui,
                Sec::Acts => saw_acts,
                Sec::Blob => saw_blob,
            };
            if dup {
                return Err(err("dup section"));
            }
            match sec {
                Sec::App => saw_app = true,
                Sec::Caps => saw_caps = true,
                Sec::Run => saw_run = true,
                Sec::Compat => saw_compat = true,
                Sec::Secu => saw_secu = true,
                Sec::Ui => saw_ui = true,
                Sec::Acts => saw_acts = true,
                Sec::Blob => saw_blob = true,
            }
            cur = Some(sec);
            continue;
        }

        // key = value
        let eq = match line.iter().position(|&b| b == b'=') {
            Some(i) => i,
            None => return Err(err("bad line")),
        };
        let key = trim(&line[..eq]);
        let val = trim(&line[eq + 1..]);
        if key.is_empty() {
            return Err(err("bad line"));
        }
        let sec = match cur {
            Some(s) => s,
            None => return Err(err("key before section")),
        };
        match (sec, key) {
            (Sec::App, b"id") => set_str(&mut f.id, val)?,
            (Sec::App, b"version") => set_str(&mut f.version, val)?,
            (Sec::App, b"entry") => set_str(&mut f.entry, val)?,
            (Sec::App, b"type") => set_str(&mut f.ty, val)?,
            (Sec::App, b"intent") => {
                if f.intent.is_some() {
                    return Err(err("dup key"));
                }
                let w = parse_string(val).ok_or_else(|| err("bad value"))?;
                // parse_string 已验可打印 ASCII，from_utf8 必然成功——防御
                // unwrap_or 兜底。
                let word = from_utf8(w).unwrap_or("");
                f.intent = Some(Intent::from_word(word).ok_or_else(|| err("bad intent"))?);
            }
            (Sec::Caps, b"console") => {
                if f.console.is_some() {
                    return Err(err("dup key"));
                }
                f.console = Some(parse_bool(val).ok_or_else(|| err("bad value"))?);
            }
            // SDK-1a：fs/net 作用域数组（词表闭集执法在 set_scope）
            (Sec::Caps, b"fs") => set_scope(&mut f.fs, val, FS_SCOPES)?,
            (Sec::Caps, b"net") => set_scope(&mut f.net, val, NET_SCOPES)?,
            (Sec::Run, b"stack_pages") => {
                if f.stack_pages.is_some() {
                    return Err(err("dup key"));
                }
                let n = parse_u32(val).ok_or_else(|| err("bad int"))?;
                if !(STACK_PAGES_MIN..=STACK_PAGES_MAX).contains(&n) {
                    return Err(err("range"));
                }
                f.stack_pages = Some(n);
            }
            // SDK-1a：三值版本域（值文法执法在 set_sdkver；齐缺裁断在收尾）
            (Sec::Compat, b"min") => set_sdkver(&mut f.c_min, val)?,
            (Sec::Compat, b"target") => set_sdkver(&mut f.c_target, val)?,
            (Sec::Compat, b"compile") => set_sdkver(&mut f.c_compile, val)?,
            // VER-01 提案第 4 条：平台 API level 包侧位（独立可选缺省 0——
            // 文法执法在 set_compat_level，不参三值齐缺裁断）
            (Sec::Compat, b"min_level") => set_compat_level(&mut f.c_min_level, val)?,
            // SDK-1a：预留节（值域执法在 set_sec；齐缺裁断在收尾）
            (Sec::Secu, b"profile") => set_sec(&mut f.sec_profile, val)?,
            (Sec::Secu, b"key_lineage") => set_sec(&mut f.sec_lineage, val)?,
            // ADR #82 V3：第三键 integrator（系统集成者身份 lineage——
            // set_sec 同臂同值域；三键齐缺裁断在收尾）
            (Sec::Secu, b"integrator") => set_sec(&mut f.sec_integrator, val)?,
            // GUI-7：[ui] 三键独立可选（路径执法在 set_ui_path；label 帽在此）
            (Sec::Ui, b"icon") => set_ui_path(&mut f.ui_icon, val, ICON_PREFIX.as_bytes())?,
            (Sec::Ui, b"views") => set_ui_path(&mut f.ui_views, val, VIEWS_PREFIX.as_bytes())?,
            (Sec::Ui, b"label") => {
                if f.ui_label.is_some() {
                    return Err(err("bad ui"));
                }
                let s = parse_string(val).ok_or_else(|| err("bad ui"))?;
                if s.is_empty() || s.len() > LABEL_MAX {
                    return Err(err("bad ui"));
                }
                f.ui_label = Some(s);
            }
            // 同意面先落件二：[agent-actions] 键 = 动作名（开放词表——
            // kebab-case/三档值域/重复名/越帽执法在 set_agent_action，
            // 节内失败统一 bad agent-actions）
            (Sec::Acts, k) => set_agent_action(&mut f.agent_actions, k, val)?,
            // ADR #82 V6 前置：[blob] 载体节五字段（形态执法在 set_blob；
            // 五键齐备裁断在收尾——现值不消费）
            (Sec::Blob, b"license") => set_blob(&mut f.blob_license, val)?,
            (Sec::Blob, b"origin") => set_blob(&mut f.blob_origin, val)?,
            (Sec::Blob, b"sha256") => set_blob(&mut f.blob_sha256, val)?,
            (Sec::Blob, b"size") => set_blob(&mut f.blob_size, val)?,
            (Sec::Blob, b"usage") => set_blob(&mut f.blob_usage, val)?,
            _ => return Err(err("unknown key")),
        }
    }

    // ---- 收尾：节在场 → 必填键在场 → 语义校验（§2 冻结面）----
    if !saw_app {
        return Err(err("missing [app]"));
    }
    if !saw_caps {
        return Err(err("missing [capabilities]"));
    }
    let id = f.id.ok_or_else(|| err("missing id"))?;
    if !valid_id(id) {
        return Err(err("bad id"));
    }
    let version = f.version.ok_or_else(|| err("missing version"))?;
    if version.is_empty() || version.len() > VERSION_MAX {
        return Err(err("bad version"));
    }
    let entry = f.entry.ok_or_else(|| err("missing entry"))?;
    // entry = 恰 `bin/` 前缀的完整包内路径：短于/等于前缀、超长、无前缀
    // 皆拒（`bin/` 裸前缀 = 空文件名，拒）。
    if entry.len() <= b"bin/".len() || entry.len() > ENTRY_MAX || !entry.starts_with(b"bin/") {
        return Err(err("bad entry"));
    }
    let ty = f.ty.ok_or_else(|| err("missing type"))?;
    // ADR #82 V1：type 词表 {app,driver}——"driver" 收格式不消费（AppType
    // 入结构，svcmgr 收集槽过滤）；"service" 位不进 v1（野词面）。
    // parse_string 已验可打印 ASCII，字节面直接判（from_word 吃 &[u8]）。
    let ty = AppType::from_word(ty).ok_or_else(|| err("bad type"))?;
    let intent = f.intent.ok_or_else(|| err("missing intent"))?;
    let console = f.console.ok_or_else(|| err("missing console"))?;

    // ---- SDK-1a 收尾：三值/两键「齐备或齐缺」+ 缺省 None（add-only）----
    // min_level 独立可选（VER-01 提案第 4 条）：缺省 0、不参三值齐缺裁断；
    // 但**不被静默吞弃**——[compat] 在场而三值齐缺（哪怕 min_level 在场）
    // = bad compat（空 [compat] 节先例只对真空节；声明了最低平台级却丢约
    // 束 = 静默错值类，loud 拒）。
    let compat = match (f.c_min, f.c_target, f.c_compile) {
        (None, None, None) if f.c_min_level.is_none() => None,
        (Some(min), Some(target), Some(compile)) => Some(Compat {
            min,
            target,
            compile,
            min_level: f.c_min_level.unwrap_or(0),
        }),
        _ => return Err(err("bad compat")),
    };
    let fs = f.fs.map(|s| from_utf8(s).unwrap_or(""));
    let net = f.net.map(|s| from_utf8(s).unwrap_or(""));
    // ADR #82 V3：三键齐备或齐缺（「两键齐备或齐缺」不变量的三键化重
    // 立法——旧两键形（V3 前形）不齐即拒，schema 在野前收口零迁移）。
    let security = match (f.sec_profile, f.sec_lineage, f.sec_integrator) {
        (None, None, None) => None,
        (Some(profile), Some(key_lineage), Some(integrator)) => Some(Security {
            profile: from_utf8(profile).unwrap_or(""),
            key_lineage: from_utf8(key_lineage).unwrap_or(""),
            integrator: from_utf8(integrator).unwrap_or(""),
        }),
        _ => return Err(err("bad security")),
    };
    // GUI-7：三键独立可选——全缺省 None（空 [ui] 节合法同空 [runtime]，
    // 其文本形态由 mkapp 规范形门禁拒——两级执法互补）。
    let ui = match (f.ui_icon, f.ui_label, f.ui_views) {
        (None, None, None) => None,
        (icon, label, views) => Some(Ui {
            icon: icon.map(|s| from_utf8(s).unwrap_or("")),
            label: label.map(|s| from_utf8(s).unwrap_or("")),
            views: views.map(|s| from_utf8(s).unwrap_or("")),
        }),
    };
    // ADR #82 V6：[blob] 在场则五键必全（缺一 bad blob——声明完整性）；
    // 空节 = 缺席同义 None（空 [runtime]/[compat] 先例——其文本形态由
    // mkapp 规范形门禁拒，两级执法互补）。
    let blob = match (
        f.blob_license,
        f.blob_origin,
        f.blob_sha256,
        f.blob_size,
        f.blob_usage,
    ) {
        (None, None, None, None, None) => None,
        (Some(license), Some(origin), Some(sha256), Some(size), Some(usage)) => Some(Blob {
            license: from_utf8(license).unwrap_or(""),
            origin: from_utf8(origin).unwrap_or(""),
            sha256: from_utf8(sha256).unwrap_or(""),
            size: from_utf8(size).unwrap_or(""),
            usage: from_utf8(usage).unwrap_or(""),
        }),
        _ => return Err(err("bad blob")),
    };

    // 字节集已由 valid_id/parse_string/词表验为 ASCII——from_utf8 必然成功，
    // unwrap_or 仅防御（永不 panic 承诺）。
    Ok(Manifest {
        id: from_utf8(id).unwrap_or(""),
        version: from_utf8(version).unwrap_or(""),
        entry: from_utf8(entry).unwrap_or(""),
        ty,
        console,
        stack_pages: f.stack_pages.unwrap_or(STACK_PAGES_DEFAULT),
        intent,
        compat,
        fs,
        net,
        security,
        ui,
        // 同意面先落件二：缺席/空节 = 空集（收尾无齐缺裁断——动态键
        // 无「必填集」概念，UI 独立可选族的自然延伸）。
        agent_actions: f.agent_actions,
        blob,
    })
}

// ---- host 测试面（§3 清单：完整钉板 / 缺省合并 / 坏输入族 + 恰满边界
// 正例 / 真 manifest fixture 共识 / intent 判别值钉板）----

#[cfg(test)]
mod tests {
    use super::*;

    /// §2 样例同形（stack_pages = 16 以区分显式值与缺省 8）。
    const BASE: &str = r#"[app]
id = "APPDEMO"
version = "0.1.0"
entry = "bin/appdemo"
type = "app"
intent = "utility"

[capabilities]
console = true

[runtime]
stack_pages = 16
"#;

    fn ok(s: &str) -> Manifest<'_> {
        parse(s.as_bytes()).expect("parse ok")
    }

    fn bad(s: &str) -> ManifestErr {
        parse(s.as_bytes()).expect_err("parse err")
    }

    // 1. 完整钉板：§2 样例逐字段断言。
    #[test]
    fn full_pin_canonical() {
        let m = ok(BASE);
        assert_eq!(m.id, "APPDEMO");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.entry, "bin/appdemo");
        assert_eq!(m.ty, AppType::App);
        assert!(m.console);
        assert_eq!(m.stack_pages, 16);
        assert_eq!(m.intent, Intent::Utility);
    }

    // 2. 缺省合并：省 [runtime] → stack_pages == 8。
    #[test]
    fn defaults_when_runtime_omitted() {
        let s = BASE.replace("\n[runtime]\nstack_pages = 16\n", "\n");
        assert_eq!(ok(&s).stack_pages, STACK_PAGES_DEFAULT);
    }

    // 2b. 空 [runtime] 节合法（= 全缺省，与省节同义）。
    #[test]
    fn empty_runtime_section_legal() {
        let s = BASE.replace("stack_pages = 16\n", "");
        assert!(s.trim_end().ends_with("[runtime]"));
        assert_eq!(ok(&s).stack_pages, STACK_PAGES_DEFAULT);
    }

    // 3. 恰满边界正例（fsproto out_exact_fit 同族——帽值 `<` vs `<=` 漂移
    // 时任一侧静默分叉、测试照绿的防线）。
    #[test]
    fn exact_fit_boundaries() {
        let id24 = "A".repeat(ID_MAX);
        assert_eq!(ok(&BASE.replace("APPDEMO", &id24)).id, id24);

        let v16 = "0".repeat(VERSION_MAX);
        assert_eq!(ok(&BASE.replace("0.1.0", &v16)).version, v16);

        let e32 = format!("bin/{}", "a".repeat(ENTRY_MAX - 4));
        assert_eq!(e32.len(), ENTRY_MAX);
        assert_eq!(ok(&BASE.replace("bin/appdemo", &e32)).entry, e32);

        assert_eq!(ok(&BASE.replace("16", "1")).stack_pages, STACK_PAGES_MIN);
        assert_eq!(ok(&BASE.replace("16", "64")).stack_pages, STACK_PAGES_MAX);
    }

    // 3b. ID_MAX 绝对钉（=24）——berry-layout::ID_MAX 跨面镜像的对偶侧
    //     （其 cap_anchor_pins 钉本侧值）：此前两侧测试皆常量相对取界，
    //     单侧改帽全绿静默分叉；本钉使「两侧各钉一份，改帽任一侧即红」
    //     名实相符（tools/mkapp.py python 镜像仍注释锚，改帽三处同步）。
    #[test]
    fn id_max_pinned() {
        assert_eq!(ID_MAX, 24);
    }

    // 4. manifest 恰 1024B（注释补零——注释内容零语义面）；1025B 拒。
    #[test]
    fn manifest_exactly_1024_bytes() {
        let mut s = BASE.to_string();
        s.push('\n');
        s.push('#');
        s.push_str(&"a".repeat(MANIFEST_MAX - BASE.len() - 2));
        assert_eq!(s.len(), MANIFEST_MAX);
        assert!(parse(s.as_bytes()).is_ok());

        let mut t = s.clone();
        t.push('a');
        assert_eq!(bad(&t).reason_str(), "too big");
    }

    // 5. 间距容忍集全形：整行/行尾注释（多字节 UTF-8 注释内容）、
    // `[ app ]`、`id="X"` 无空格、行首缩进、空行。
    #[test]
    fn syntax_tolerance_set() {
        let s = "\n# 整行注释——内容任意字节（多字节 UTF-8 亦然）\n[ app ]\nid=\"APPQUIET\"\n  version = \"0.2.0\"\nentry = \"bin/appquiet\"   # 行尾注释\n\ntype = \"app\"\nintent = \"background\"\n\n[capabilities]\nconsole = false   # false = appd 私有采集对\n\n[runtime]\nstack_pages = 8\n";
        let m = ok(s);
        assert_eq!(m.id, "APPQUIET");
        assert_eq!(m.version, "0.2.0");
        assert_eq!(m.entry, "bin/appquiet");
        assert!(!m.console);
        assert_eq!(m.stack_pages, 8);
        assert_eq!(m.intent, Intent::Background);
    }

    // 6. intent 词表往返（大小写敏感；野词 None）。
    #[test]
    fn intent_words_roundtrip() {
        let pairs = [
            ("unspecified", Intent::Unspecified),
            ("latency_critical", Intent::LatencyCritical),
            ("interactive", Intent::Interactive),
            ("utility", Intent::Utility),
            ("background", Intent::Background),
            ("maintenance", Intent::Maintenance),
        ];
        for (w, v) in pairs {
            assert_eq!(Intent::from_word(w), Some(v));
            assert_eq!(v.word(), w);
        }
        assert_eq!(Intent::from_word("urgent"), None);
        assert_eq!(Intent::from_word("Utility"), None);
    }

    // 7. intent 判别值钉板（§3 item 5：跨 crate 词表对账的唯一机器锚）。
    #[test]
    fn intent_discriminants_pinned() {
        assert_eq!(Intent::Unspecified as u8, 0x00);
        assert_eq!(Intent::LatencyCritical as u8, 0x01);
        assert_eq!(Intent::Interactive as u8, 0x02);
        assert_eq!(Intent::Utility as u8, 0x03);
        assert_eq!(Intent::Background as u8, 0x04);
        assert_eq!(Intent::Maintenance as u8, 0x05);
    }

    // 8. 空输入 / 裸输入。
    #[test]
    fn reject_empty_and_bare() {
        assert!(parse(b"").is_err());
        assert!(parse(b"\n\n").is_err());
        // 只有节头无键 → missing 面
        assert!(parse(b"[app]\nid = \"X\"\n").is_err());
    }

    // 9. CR（CRLF 与裸 CR 同拒）与 tab。
    #[test]
    fn reject_cr_and_tab() {
        let s = BASE.replace('\n', "\r\n");
        assert_eq!(bad(&s).reason_str(), "CRLF");
        let s2 = format!("{BASE}\r");
        assert_eq!(bad(&s2).reason_str(), "CRLF");
        let s3 = BASE.replace("id = ", "\tid = ");
        assert_eq!(bad(&s3).reason_str(), "tab");
    }

    // 10. 未知节 / 未知键 / 节前键 / 坏节头 / 越节键。
    #[test]
    fn reject_unknown_and_misplaced() {
        // GUI-7 起 [ui] 是在册节——未知节探针换 [extra]
        assert_eq!(bad(&BASE.replace("[runtime]", "[extra]")).reason_str(), "unknown section");
        // [ app  ]（双空格）折出 "unknown section"——容忍集外
        assert_eq!(bad(&BASE.replace("[runtime]", "[  runtime ]")).reason_str(), "unknown section");
        assert_eq!(bad(&BASE.replace("[runtime]", "[runtime")).reason_str(), "bad section");
        assert_eq!(bad(&BASE.replace("id = ", "uid = ")).reason_str(), "unknown key");
        // console 出现在 [app] 节 = 越节未知键
        assert_eq!(
            bad(&BASE.replace("type = \"app\"", "type = \"app\"\nconsole = true")).reason_str(),
            "unknown key"
        );
        // 任何节头之前的键
        assert_eq!(
            bad(&BASE.replace("[app]\n", "")).reason_str(),
            "key before section"
        );
        // 缺 [app] 节（无 app 键越节干扰的纯形态）
        assert_eq!(
            bad("[capabilities]\nconsole = true\n").reason_str(),
            "missing [app]"
        );
    }

    // 11. 重复节 / 重复键。
    #[test]
    fn reject_duplicates() {
        assert_eq!(bad(&format!("{BASE}[runtime]\nstack_pages = 4\n")).reason_str(), "dup section");
        assert_eq!(bad(&format!("{BASE}[app]\n")).reason_str(), "dup section");
        assert_eq!(
            bad(&BASE.replace("[runtime]\n", "[runtime]\nstack_pages = 8\n")).reason_str(),
            "dup key"
        );
        assert_eq!(bad(&BASE.replace("[app]\n", "[app]\nid = \"OTHER\"\n")).reason_str(), "dup key");
        assert_eq!(
            bad(&BASE.replace("[capabilities]\n", "[capabilities]\nconsole = false\n"))
                .reason_str(),
            "dup key"
        );
    }

    // 12. 必填缺失六路 + 缺 [capabilities]。
    #[test]
    fn reject_missing_fields() {
        for (line, reason) in [
            ("id = \"APPDEMO\"\n", "missing id"),
            ("version = \"0.1.0\"\n", "missing version"),
            ("entry = \"bin/appdemo\"\n", "missing entry"),
            ("type = \"app\"\n", "missing type"),
            ("intent = \"utility\"\n", "missing intent"),
            ("console = true\n", "missing console"),
        ] {
            assert_eq!(bad(&BASE.replace(line, "")).reason_str(), reason);
        }
        assert_eq!(
            bad(&BASE.replace("[capabilities]\nconsole = true\n", "")).reason_str(),
            "missing [capabilities]"
        );
    }

    // 13. type / intent 词面（ADR #82 V1 起 "driver" 放行——13b 钉板；
    //     探针 = "service"（不进 v1 词表）/野词/裸词）。
    #[test]
    fn reject_bad_type_and_intent() {
        assert_eq!(bad(&BASE.replace("\"app\"", "\"service\"")).reason_str(), "bad type");
        assert_eq!(bad(&BASE.replace("\"app\"", "\"library\"")).reason_str(), "bad type");
        assert_eq!(bad(&BASE.replace("\"app\"", "app")).reason_str(), "bad value");
        assert_eq!(bad(&BASE.replace("\"app\"", "driver")).reason_str(), "bad value");
        assert_eq!(bad(&BASE.replace("\"utility\"", "\"urgent\"")).reason_str(), "bad intent");
        // 裸词（无引号）
        assert_eq!(bad(&BASE.replace("\"utility\"", "utility")).reason_str(), "bad value");
    }

    // 13b. ADR #82 V1：type 词表 {app,driver}——driver 正例收格式不消费
    // + 判别值钉板 + 词面往返（首/尾 const assert 的全量侧证）。driver
    // 包 manifest 其余面照常解析（m.id 钉——svcmgr 收集槽 `type != app`
    // 过滤的 BerryOS 侧对象面：ty 位是唯一差异）。
    #[test]
    fn app_type_words_and_discriminants() {
        let drv = BASE.replace("\"app\"", "\"driver\"");
        let m = ok(&drv);
        assert_eq!(m.ty, AppType::Driver);
        assert_eq!(m.id, "APPDEMO"); // 其余面照常解析（收格式不消费）
        assert_eq!(ok(BASE).ty, AppType::App);
        // 判别值形状钉子（0x01/0x02——v2 消费点 as u8 即得编码）。
        assert_eq!(AppType::App as u8, 0x01);
        assert_eq!(AppType::Driver as u8, 0x02);
        // 词面往返闭包：from_word ∘ word = id。
        for t in [AppType::App, AppType::Driver] {
            assert_eq!(AppType::from_word(t.word().as_bytes()), Some(t));
        }
        // 野词/大小写敏感。
        assert_eq!(AppType::from_word(b"App"), None);
        assert_eq!(AppType::from_word(b"apps"), None);
        assert_eq!(AppType::from_word(b""), None);
        assert_eq!(AppType::from_word(b"service"), None); // 不进 v1（04 号双扩案作废）
        // 坏词单测（含 v0 冻结注记里的已废位 "service"）。
        for w in ["\"service\"", "\"Service\"", "\"\"", "\" app\""] {
            assert_eq!(bad(&BASE.replace("\"app\"", w)).reason_str(), "bad type", "{w}");
        }
    }

    // 14. 布尔严格面。
    #[test]
    fn reject_bad_bool() {
        assert_eq!(bad(&BASE.replace("true", "yes")).reason_str(), "bad value");
        assert_eq!(bad(&BASE.replace("console = true", "console = \"true\"")).reason_str(), "bad value");
        assert_eq!(bad(&BASE.replace("console = true", "console = 1")).reason_str(), "bad value");
    }

    // 15. 整数族：非数字 / 前缀 / 下划线 / 符号 / 小数 / 溢出 / 空 / 越界。
    #[test]
    fn reject_bad_ints() {
        for (v, reason) in [
            ("1.5", "bad int"),
            ("0x10", "bad int"),
            ("1_0", "bad int"),
            ("+1", "bad int"),
            ("-1", "bad int"),
            ("99999999999", "bad int"), // u32 溢出
            ("\"8\"", "bad int"),       // 引号裹的数字不是整数
            ("", "bad int"),            // 空值
            ("0", "range"),
            ("65", "range"),
        ] {
            let s = BASE.replace("stack_pages = 16", &format!("stack_pages = {v}"));
            assert_eq!(bad(&s).reason_str(), reason, "stack_pages = {v:?}");
        }
    }

    // 16. 字符串族：反斜杠 / 未闭合 / 内嵌引号 / 非可打印。
    #[test]
    fn reject_bad_strings() {
        assert_eq!(bad(&BASE.replace("APPDEMO", "A\\B")).reason_str(), "bad value");
        assert_eq!(bad(&BASE.replace("\"APPDEMO\"", "\"APPDEMO")).reason_str(), "bad value");
        assert_eq!(bad(&BASE.replace("APPDEMO", "A\"B")).reason_str(), "bad value");
        // \x7f（DEL）非可打印 ASCII
        assert_eq!(bad(&BASE.replace("APPDEMO", "A\u{7f}B")).reason_str(), "bad value");
    }

    // 17. id 字符集与长度（含首字符非字母数字、空 id）。
    #[test]
    fn reject_bad_id() {
        assert_eq!(bad(&BASE.replace("APPDEMO", "-APPDEMO")).reason_str(), "bad id");
        assert_eq!(bad(&BASE.replace("APPDEMO", "APP DEMO")).reason_str(), "bad id");
        assert_eq!(bad(&BASE.replace("APPDEMO", "APP/DEMO")).reason_str(), "bad id");
        assert_eq!(bad(&BASE.replace("APPDEMO", "A".repeat(ID_MAX + 1).as_str())).reason_str(), "bad id");
        assert_eq!(bad(&BASE.replace("\"APPDEMO\"", "\"\"")).reason_str(), "bad id");
    }

    // 18. version / entry 形状。
    #[test]
    fn reject_bad_version_entry() {
        assert_eq!(
            bad(&BASE.replace("0.1.0", "0".repeat(VERSION_MAX + 1).as_str())).reason_str(),
            "bad version"
        );
        assert_eq!(bad(&BASE.replace("\"0.1.0\"", "\"\"")).reason_str(), "bad version");
        // 无 bin/ 前缀 / 裸前缀 / 超长
        assert_eq!(bad(&BASE.replace("bin/appdemo", "sbin/appdemo")).reason_str(), "bad entry");
        assert_eq!(bad(&BASE.replace("bin/appdemo", "bin/")).reason_str(), "bad entry");
        let long_entry = format!("bin/{}", "a".repeat(ENTRY_MAX - 3));
        assert!(long_entry.len() > ENTRY_MAX);
        assert_eq!(bad(&BASE.replace("bin/appdemo", &long_entry)).reason_str(), "bad entry");
    }

    // 19. 超集语法逐项（tomllib 收而子集拒——mkapp 门卫自测的运行期对照面）。
    #[test]
    fn reject_superset_syntax() {
        // 点键
        assert_eq!(bad(&BASE.replace("id = ", "app.id = ")).reason_str(), "unknown key");
        // 数组
        assert!(parse(BASE.replace("[capabilities]\nconsole = true", "[capabilities]\nconsole = [1]").as_bytes()).is_err());
        // 内联表
        assert!(parse(BASE.replace("console = true", "console = { on = 1 }").as_bytes()).is_err());
        // 多行字符串（"""……"""——首尾引号剥离后内嵌引号即拒）
        assert!(parse(b"[app]\nid = \"\"\"X\"\"\"\n[capabilities]\nconsole = true\n").is_err());
        // 浮点 / 负 stack_pages 已在 reject_bad_ints 覆盖
    }

    // 20. 真 manifest fixture 共识（§3 item 4 正例半——真源文件不落入
    // 「tomllib 收而子集拒」缝里的单边钉板；负例半在 mkapp 门卫自测）。
    #[test]
    fn fixture_appdemo() {
        const M: &str = include_str!("../../bins/appdemo/manifest.toml");
        let m = ok(M);
        assert_eq!(m.id, "APPDEMO");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.entry, "bin/appdemo");
        assert!(m.console);
        assert_eq!(m.stack_pages, STACK_PAGES_DEFAULT); // 省 [runtime]
        assert_eq!(m.intent, Intent::Utility);
        // SDK-1a（§6.1 新 manifest = F1+F2 组合）：三值齐备 0.1.0 +
        // fs=rootfs；net/security 缺席 None（§7.1 断言扩）。
        let c = m.compat.expect("appdemo compat");
        assert_eq!(
            (c.min, c.target, c.compile),
            (SDK_CURRENT, SDK_CURRENT, SDK_CURRENT)
        );
        // VER-01 提案第 4 条：appdemo = min_level 首个正样本（装载期
        // lvl min 检查的活证）；appquiet/apptomb 缺席 = 零义务负样本。
        assert_eq!(c.min_level, 1);
        assert_eq!(m.fs, Some("rootfs"));
        assert_eq!(m.net, None);
        assert_eq!(m.security, None);
        // GUI-7：appdemo = 首个带 [ui] 的在册样本（icon/label/views 三键
        // 齐用——mkapp 编码链 + appd 枚举标记的消费面对象）。
        let u = m.ui.expect("appdemo ui");
        assert_eq!(u.icon, Some("icons/icon.byi"));
        assert_eq!(u.label, Some("APPDEMO"));
        assert_eq!(u.views, Some("views/main.bui"));
    }

    // 21. appquiet：显式 stack_pages = 8 + console = false + background 词。
    #[test]
    fn fixture_appquiet() {
        const M: &str = include_str!("../../bins/appquiet/manifest.toml");
        let m = ok(M);
        assert_eq!(m.id, "APPQUIET");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.entry, "bin/appquiet");
        assert!(!m.console);
        assert_eq!(m.stack_pages, 8); // 显式路径
        assert_eq!(m.intent, Intent::Background);
        // GUI-7：无 [ui] 旧形照收（add-only 双半活证：APPDEMO 带样本 vs
        // APPQUIET 不带）。
        assert_eq!(m.ui, None);
    }

    // 22. 理由截断（「编不出就拒绝」对诊断行放宽为「截断不 panic」）。
    #[test]
    fn err_reason_truncation() {
        let e = ManifestErr::new("abcdefghijklmnopqrstuvwxyz"); // 26B > 24B
        assert_eq!(e.reason_str().len(), REASON_MAX);
        assert!(e.reason_str().ends_with('…'));
        let e2 = ManifestErr::new("short");
        assert_eq!(e2.reason_str(), "short");
        // 现役全部理由 ≤24B 的自证（发射窗契约面；SDK-1a 新增三词随入）
        for r in [
            "too big", "CRLF", "tab", "bad section", "unknown section", "dup section",
            "bad line", "key before section", "unknown key", "dup key", "bad value",
            "bad intent", "bad int", "range", "missing [app]", "missing [capabilities]",
            "missing id", "bad id", "missing version", "bad version", "missing entry",
            "bad entry", "missing type", "bad type", "missing intent", "missing console",
            "bad compat", "bad scope", "bad security", "bad ui", "bad agent-actions",
            "bad blob",
        ] {
            assert!(r.len() <= REASON_MAX, "{r}");
            assert_eq!(ManifestErr::new(r).reason_str(), r);
        }
    }

    // ---- SDK-1a 增量测试面（§7.1 表：sdkver_parse..canonical_forms_parse）----

    /// BASE 的 `[capabilities]` 行前插 [compat] 段（§3.2 F1 定位同款）。
    fn with_compat(base: &str, body: &str) -> String {
        base.replace("\n[capabilities]", &format!("\n[compat]\n{body}\n[capabilities]"))
    }

    // 23. SdkVer 文法：三段合法全过；前导零/四段/两段/字母/空段/4 位段全 None。
    #[test]
    fn sdkver_parse() {
        assert_eq!(SdkVer::parse("0.1.0"), Some(SdkVer { major: 0, minor: 1, patch: 0 }));
        assert_eq!(
            SdkVer::parse("999.999.999"),
            Some(SdkVer { major: 999, minor: 999, patch: 999 })
        );
        assert_eq!(SdkVer::parse("0.0.0"), Some(SdkVer { major: 0, minor: 0, patch: 0 }));
        assert!(SdkVer::parse("1.20.300").is_some());
        for s in [
            "01.0.0",   // 前导零（"0" 单独合法、多位必 [1-9] 开头）
            "1.02.0",
            "0.1.00",
            "1.2.3.4",  // 四段
            "1.2",      // 两段
            "0.1",
            "1.2.x",    // 字母段
            "a.b.c",
            "1..2",     // 空段
            ".1.2",
            "1.2.",
            "1000.0.0", // 4 位段（文法天然封顶 3 位）
            "0.1.1000",
            "",         // 空输入
            "0.1.0 ",   // 尾随空格（parse 不 trim——值内域已由 parse_string 裁）
        ] {
            assert!(SdkVer::parse(s).is_none(), "{s:?}");
        }
    }

    // 24. SDK_CURRENT 双轨同值（STR 轨 parse == 常量轨）+ Ord 字典序钉板
    // （appd `min > SDK_CURRENT` 检查的比较基元——derive 序 = 字段声明序）。
    #[test]
    fn sdk_current_shape() {
        assert_eq!(SdkVer::parse(SDK_CURRENT_STR), Some(SDK_CURRENT));
        let v = |major, minor, patch| SdkVer { major, minor, patch };
        assert!(v(0, 0, 999) < v(0, 1, 0));
        assert!(v(0, 1, 0) < v(0, 1, 1));
        assert!(v(0, 1, 1) < v(0, 2, 0));
        assert!(v(0, 2, 0) < v(1, 0, 0));
    }

    // 25. [compat] 三键齐备解析钉板（逐字段）+ 分层执法面（链比较不在本层）。
    #[test]
    fn compat_parse_pin() {
        let s = with_compat(BASE, "min = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"");
        let m = ok(&s);
        let c = m.compat.expect("compat");
        assert_eq!(
            (c.min, c.target, c.compile),
            (SDK_CURRENT, SDK_CURRENT, SDK_CURRENT)
        );
        // 分层执法（§2.1）：链比较（min ≤ target ≤ compile ≤ SDK_CURRENT）
        // 是 mkapp 编译期门禁——parser 对「compile 超当前级」照收（appd
        // 运行期只查 min > SDK_CURRENT）。
        let s2 = with_compat(BASE, "min = \"0.0.1\"\ntarget = \"0.1.0\"\ncompile = \"0.2.0\"");
        let m2 = ok(&s2);
        assert_eq!(m2.compat.unwrap().compile, SdkVer { major: 0, minor: 2, patch: 0 });
        // 节序无关（[compat] 居尾同收——语法子集节序无关面）
        let s = format!(
            "{BASE}[compat]\nmin = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\n"
        );
        assert!(parse(s.as_bytes()).is_ok());
    }

    // 26. v0 旧形（无任何新键）全缺省 None——**add-only 执法锚**（v0 冻结面
    // 零漂移：旧包在增量解析器上行为不变）。
    #[test]
    fn compat_absent() {
        let m = ok(BASE);
        assert_eq!(m.compat, None);
        assert_eq!(m.fs, None);
        assert_eq!(m.net, None);
        assert_eq!(m.security, None);
        // ADR #82 V6：[blob] 缺席 None（add-only 执法锚同上）
        assert_eq!(m.blob, None);
        // 空 [compat] 节 = 三键齐缺（空 [runtime] 节先例同款）→ None；
        // 其文本形态由 mkapp 规范形门禁另拒——两层执法互补。
        let s = BASE.replace("\n[capabilities]", "\n[compat]\n[capabilities]");
        assert!(parse(s.as_bytes()).is_ok());
        assert_eq!(ok(&s).compat, None);
    }

    // 27. compat 拒面：键不齐/重复键/值文法全 `bad compat`（理由单一化）。
    #[test]
    fn compat_rejects() {
        for body in [
            "min = \"0.1.0\"\ntarget = \"0.1.0\"", // 缺 compile
            "min = \"0.1.0\"",                     // 仅 min
            "target = \"0.1.0\"",
            "compile = \"0.1.0\"",
        ] {
            let s = with_compat(BASE, body);
            assert_eq!(bad(&s).reason_str(), "bad compat", "{body:?}");
        }
        // 重复键
        let s = with_compat(
            BASE,
            "min = \"0.1.0\"\nmin = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"",
        );
        assert_eq!(bad(&s).reason_str(), "bad compat");
        // 值文法（SEG 文法拒面逐项）
        for v in ["1.2", "1.2.3.4", "01.0.0", "1.02.0", "1000.0.0", "1.2.x", "0.1.-1"] {
            let body = format!("min = \"{v}\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"");
            let s = with_compat(BASE, &body);
            assert_eq!(bad(&s).reason_str(), "bad compat", "min = {v:?}");
        }
        // 裸词（无引号）——parse_string 拒
        let s = with_compat(BASE, "min = 0.1.0\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"");
        assert_eq!(bad(&s).reason_str(), "bad compat");
        // 序不合（min > target）parser 照收——链比较在 mkapp（分层执法显式钉住）
        let s = with_compat(BASE, "min = \"0.2.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"");
        assert!(parse(s.as_bytes()).is_ok());
    }

    // 27b. [compat] min_level（VER-01 提案第 4 条·平台 API level 单调化的
    //      包侧位）：在场解析 / 缺省 0（add-only 零义务——旧包零漂移）/
    //      坏值族统一 bad compat（理由单一化同款）。
    #[test]
    fn compat_min_level() {
        let full = "min = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\nmin_level = 1";
        let c = ok(&with_compat(BASE, full)).compat.expect("compat");
        assert_eq!(
            (c.min, c.target, c.compile),
            (SDK_CURRENT, SDK_CURRENT, SDK_CURRENT)
        );
        assert_eq!(c.min_level, 1);
        // 缺省 0：三值齐备而 min_level 缺席 = 零义务默认（appquiet/apptomb 负样本面）
        let plain = "min = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"";
        assert_eq!(ok(&with_compat(BASE, plain)).compat.unwrap().min_level, 0);
        // 坏值族：引号串/符号/hex 前缀/u32 溢出/重复键 → bad compat
        for v in ["\"1\"", "-1", "+1", "0x1", "1_0", "4294967296", "1.0"] {
            let body = format!("min = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\nmin_level = {v}");
            assert_eq!(
                bad(&with_compat(BASE, &body)).reason_str(),
                "bad compat",
                "min_level = {v:?}"
            );
        }
        let dup = "min = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\nmin_level = 1\nmin_level = 1";
        assert_eq!(bad(&with_compat(BASE, dup)).reason_str(), "bad compat");
        // 独立可选不救三值齐缺：仅 min_level 而无三值 → bad compat
        let lone = "min_level = 1";
        assert_eq!(bad(&with_compat(BASE, lone)).reason_str(), "bad compat");
    }

    // 28. fs 作用域：词表内过；词表外/空元素/多元素/非数组一律 bad scope。
    #[test]
    fn fs_scope() {
        let s = BASE.replace("console = true", "console = true\nfs = [\"rootfs\"]");
        let m = ok(&s);
        assert_eq!(m.fs, Some("rootfs"));
        for v in ["tmpfs", "$appdata", ""] {
            let s = BASE.replace("console = true", &format!("console = true\nfs = [\"{v}\"]"));
            assert_eq!(bad(&s).reason_str(), "bad scope", "fs = {v:?}");
        }
        // 多元素（v1 恰一词）
        let s = BASE.replace(
            "console = true",
            "console = true\nfs = [\"rootfs\", \"rootfs\"]",
        );
        assert_eq!(bad(&s).reason_str(), "bad scope");
        // 非数组（裸串）
        let s = BASE.replace("console = true", "console = true\nfs = \"rootfs\"");
        assert_eq!(bad(&s).reason_str(), "bad scope");
    }

    // 29. net 作用域：`client` **parser 照收**（档 (b)：mkapp 收、appd loud
    // 拒——「词表 = 镜子」的诚实形态）；词表外拒。
    #[test]
    fn net_scope() {
        let s = BASE.replace("console = true", "console = true\nnet = [\"client\"]");
        let m = ok(&s);
        assert_eq!(m.net, Some("client"));
        let s = BASE.replace("console = true", "console = true\nnet = [\"server\"]");
        assert_eq!(bad(&s).reason_str(), "bad scope");
        // fs+net 同节共存（F6 kitchen-sink 子集）
        let s = BASE.replace(
            "console = true",
            "console = true\nfs = [\"rootfs\"]\nnet = [\"client\"]",
        );
        let m = ok(&s);
        assert_eq!((m.fs, m.net), (Some("rootfs"), Some("client")));
    }

    // 30. [capabilities] 未知键（枚举式增长执法——新能力词必须动 parser +
    // appd 两处；`display = true` 是 v2 候选词的探针样本）。
    #[test]
    fn caps_unknown_key() {
        let s = BASE.replace("console = true", "console = true\ndisplay = true");
        assert_eq!(bad(&s).reason_str(), "unknown key");
        // fs 不能顶替 console（必填缺失面不受新键扰动）
        let s = BASE.replace("console = true", "fs = [\"rootfs\"]");
        assert_eq!(bad(&s).reason_str(), "missing console");
    }

    // 31. 数组文法细面：值域整体 trim 容忍收；串内空格/单引号/尾逗号拒。
    #[test]
    fn array_grammar() {
        // 整体 trim 容忍（`console =  true` 双空格现状同口径——解析收、
        // 规范形门禁拒的两级执法）
        let s = BASE.replace("console = true", "console = true\nfs = [ \"rootfs\" ]");
        assert_eq!(ok(&s).fs, Some("rootfs"));
        // 串内空格 → 词表外
        let s = BASE.replace("console = true", "console = true\nfs = [ \" rootfs\" ]");
        assert_eq!(bad(&s).reason_str(), "bad scope");
        // 单引号串（parse_string 只收双引号）
        let s = BASE.replace("console = true", "console = true\nfs = ['rootfs']");
        assert_eq!(bad(&s).reason_str(), "bad scope");
        // 尾逗号（恰一元素文法外）
        let s = BASE.replace("console = true", "console = true\nfs = [\"rootfs\",]");
        assert_eq!(bad(&s).reason_str(), "bad scope");
    }

    // 32. [security] 形状：三键齐备（ADR #82 V3 三键化重立法）+ 值域
    // [a-z0-9-]{1,16}；键不齐（含 V3 前旧两键形）/越界拒。
    #[test]
    fn security_shape() {
        let s0 = format!(
            "{BASE}\n[security]\nprofile = \"ed25519-main\"\nkey_lineage = \"root-2026\"\nintegrator = \"oem-main\"\n"
        );
        let m = ok(&s0);
        let s = m.security.expect("security");
        assert_eq!(
            (s.profile, s.key_lineage, s.integrator),
            ("ed25519-main", "root-2026", "oem-main")
        );
        // 恰 16B 边界正例（帽值 `<` vs `<=` 漂移防线；integrator 同域）
        let s16 = format!(
            "{BASE}\n[security]\nprofile = \"{}\"\nkey_lineage = \"x\"\nintegrator = \"x\"\n",
            "a".repeat(16)
        );
        let m = ok(&s16);
        assert_eq!(m.security.unwrap().profile.len(), 16);
        let i16 = format!(
            "{BASE}\n[security]\nprofile = \"x\"\nkey_lineage = \"x\"\nintegrator = \"{}\"\n",
            "b".repeat(16)
        );
        assert_eq!(ok(&i16).security.unwrap().integrator.len(), 16);
        // 键不齐：单键 ×3 / 两键 ×3（含 V3 前旧形 profile+key_lineage——
        // 三键化后不齐即拒，schema 在野前收口零迁移）。
        for body in [
            "profile = \"ed25519-main\"\n",
            "key_lineage = \"root-2026\"\n",
            "integrator = \"oem-main\"\n",
            "profile = \"ed25519-main\"\nkey_lineage = \"root-2026\"\n",
            "profile = \"ed25519-main\"\nintegrator = \"oem-main\"\n",
            "key_lineage = \"root-2026\"\nintegrator = \"oem-main\"\n",
        ] {
            let s = format!("{BASE}\n[security]\n{body}");
            assert_eq!(bad(&s).reason_str(), "bad security", "{body:?}");
        }
        // 值域：空格 / 17B / 空串 / 大写（三键同域——set_sec 同臂）
        for (k, v) in [
            ("profile", "a b"),
            ("profile", "aaaaaaaaaaaaaaaaa"),
            ("key_lineage", "a b"),
            ("integrator", "a b"),
            ("integrator", "aaaaaaaaaaaaaaaaa"),
            ("integrator", ""),
            ("integrator", "Abc"),
        ] {
            let line = format!("{k} = \"{v}\"\n");
            let body = match k {
                "profile" => format!("{line}key_lineage = \"k\"\nintegrator = \"i\"\n"),
                "key_lineage" => format!("profile = \"p\"\n{line}integrator = \"i\"\n"),
                _ => format!("profile = \"p\"\nkey_lineage = \"k\"\n{line}"),
            };
            let s = format!("{BASE}\n[security]\n{body}");
            assert_eq!(bad(&s).reason_str(), "bad security", "{k} = {v:?}");
        }
        // 重复键
        let s = format!(
            "{BASE}\n[security]\nprofile = \"a\"\nprofile = \"b\"\nkey_lineage = \"c\"\nintegrator = \"d\"\n"
        );
        assert_eq!(bad(&s).reason_str(), "bad security");
    }

    // ---- GUI-7 增量测试面（[ui] 三键独立可选 + 拒面单一化 bad ui）----

    /// BASE 尾插 [ui] 段（F7 定位同 F4 尾追加）。
    fn with_ui(base: &str, body: &str) -> String {
        format!("{base}\n[ui]\n{body}")
    }

    // 32a. [ui] 三键独立可选：全在场逐字段 + 单键/双键组合各自合法
    //（无齐备约束——compat/security 的显式反例）。
    #[test]
    fn ui_parse_pin() {
        let s = with_ui(
            BASE,
            "icon = \"icons/icon.byi\"\nlabel = \"Demo App\"\nviews = \"views/main.bui\"",
        );
        let m = ok(&s);
        let u = m.ui.expect("ui");
        assert_eq!(u.icon, Some("icons/icon.byi"));
        assert_eq!(u.label, Some("Demo App"));
        assert_eq!(u.views, Some("views/main.bui"));
        // 单键三形（独立可选面）
        for (body, want) in [
            ("icon = \"icons/a.byi\"\n", (Some("icons/a.byi"), None, None)),
            ("label = \"X\"\n", (None, Some("X"), None)),
            ("views = \"views/v\"\n", (None, None, Some("views/v"))),
        ] {
            let s = with_ui(BASE, body);
            let m = ok(&s);
            let u = m.ui.unwrap();
            assert_eq!((u.icon, u.label, u.views), want, "{body:?}");
        }
        // 恰满边界：32B 路径（icons/6 + 名 22 + .byi 4）/ 32B label
        let p32 = format!("icons/{}.byi", "a".repeat(22));
        assert_eq!(p32.len(), UI_PATH_MAX);
        let s = with_ui(BASE, &format!("icon = \"{p32}\""));
        let m = ok(&s);
        assert_eq!(m.ui.unwrap().icon, Some(p32.as_str()));
        let l32 = "a".repeat(LABEL_MAX);
        let s = with_ui(BASE, &format!("label = \"{l32}\""));
        let m = ok(&s);
        assert_eq!(m.ui.unwrap().label, Some(l32.as_str()));
    }

    // 32b. [ui] 拒面统一 `bad ui`：前缀/裸前缀/超长/label 空/超长/重复键/
    // 未知键/越节键。
    #[test]
    fn ui_rejects() {
        for body in [
            "icon = \"icon.byi\"\n",               // 无 icons/ 前缀
            "icon = \"bin/icon.byi\"\n",           // 错前缀
            "icon = \"icons/\"\n",                 // 裸前缀（空名）
            "views = \"main.bui\"\n",              // 无 views/ 前缀
            "views = \"views/\"\n",                // 裸前缀
            "icon = \"icons/a.byi\"\nicon = \"icons/b.byi\"\n", // 重复键
            "label = \"A\"\nlabel = \"B\"\n",      // 重复键
            "label = \"\"\n",                      // 空 label
        ] {
            let s = with_ui(BASE, body);
            assert_eq!(bad(&s).reason_str(), "bad ui", "{body:?}");
        }
        // 未知键走全 schema 通例 `unknown key`（同 test 30 caps 探针）
        assert_eq!(
            bad(&with_ui(BASE, "badge = \"x\"\n")).reason_str(),
            "unknown key"
        );
        // 超长：33B 路径 / 33B label
        let p33 = format!("icons/{}.byi", "a".repeat(23));
        assert!(p33.len() > UI_PATH_MAX);
        assert_eq!(
            bad(&with_ui(BASE, &format!("icon = \"{p33}\""))).reason_str(),
            "bad ui"
        );
        let l33 = "a".repeat(LABEL_MAX + 1);
        assert_eq!(
            bad(&with_ui(BASE, &format!("label = \"{l33}\""))).reason_str(),
            "bad ui"
        );
        // 越节键：icon 出现在 [app] = unknown key（[ui] 键不泄漏）
        assert_eq!(
            bad(&BASE.replace("[app]\n", "[app]\nicon = \"icons/a.byi\"\n")).reason_str(),
            "unknown key"
        );
        // 重复节
        let s = format!("{}{}[ui]\nlabel = \"B\"\n", with_ui(BASE, "label = \"A\"\n"), "");
        assert_eq!(bad(&s).reason_str(), "dup section");
    }

    // 32c. [ui] 缺席/空节：全缺省 None（add-only 执法锚）；空节 = 解析合法
    // ui None（空 [runtime] 先例同款——文本形态由 mkapp 规范形门禁拒）。
    #[test]
    fn ui_absent() {
        assert_eq!(ok(BASE).ui, None);
        let s = BASE.replace("\n[capabilities]", "\n[ui]\n[capabilities]");
        let m = ok(&s);
        assert_eq!(m.ui, None);
    }

    // ---- 同意面先落件二测试面（[agent-actions] + 拒面单一化
    // bad agent-actions）----

    /// BASE 尾插 [agent-actions] 段（F4/F7 尾追加定位同款）。
    fn with_actions(base: &str, body: &str) -> String {
        format!("{base}\n[agent-actions]\n{body}")
    }

    // 32d. 合法三档全钉板：逐条目断言 + get 查询 + 文档序保序 + 单键形。
    #[test]
    fn agent_actions_parse_pin() {
        let s = with_actions(
            BASE,
            "open = \"routine\"\nsave = \"sensitive\"\nempty-trash = \"destructive\"",
        );
        let m = ok(&s);
        let aa = &m.agent_actions;
        assert_eq!(aa.len(), 3);
        assert_eq!(aa.get("open"), Some(RiskTier::Routine));
        assert_eq!(aa.get("save"), Some(RiskTier::Sensitive));
        assert_eq!(aa.get("empty-trash"), Some(RiskTier::Destructive));
        assert_eq!(aa.get("nope"), None);
        // 文档序保序（mkapp 规范形重发射同序——iter 契约）
        let words: Vec<&str> = aa.iter().map(|(_, t)| t.word()).collect();
        assert_eq!(words, ["routine", "sensitive", "destructive"]);
        // 单键形合法（无齐备约束——[ui] 三键独立同款）
        let s = with_actions(BASE, "open = \"routine\"");
        assert_eq!(ok(&s).agent_actions.len(), 1);
        // 恰满动作名边界（24B——帽值 `<` vs `<=` 漂移防线）
        let n24 = "a".repeat(ACTION_NAME_MAX);
        let s = with_actions(BASE, &format!("{} = \"routine\"", n24));
        assert_eq!(ok(&s).agent_actions.get(&n24), Some(RiskTier::Routine));
    }

    // 32e. 拒面统一 `bad agent-actions`：未知档值/大小写敏感/裸词值/键
    // 字符集（snake_case、大写、数字/连字符开头、超长）/重复动作名/越帽。
    #[test]
    fn agent_actions_rejects() {
        let k25 = format!("{} = \"routine\"", "a".repeat(ACTION_NAME_MAX + 1));
        for body in [
            "open = \"urgent\"\n",                        // 未知档位值
            "open = \"Routine\"\n",                       // 大小写敏感
            "open = routine\n",                           // 裸词（无引号）
            "empty_trash = \"destructive\"\n",            // snake_case 键
            "Open = \"routine\"\n",                       // 大写开头
            "1open = \"routine\"\n",                      // 数字开头
            "-open = \"routine\"\n",                      // 连字符开头
            k25.as_str(),                                 // 键超长（25B）
            "open = \"routine\"\nopen = \"sensitive\"\n", // 重复动作名
        ] {
            let s = with_actions(BASE, body);
            assert_eq!(bad(&s).reason_str(), "bad agent-actions", "{body:?}");
        }
        // 越帽：AGENT_ACTIONS_MAX + 1 条（push 帽满拒）
        let over = (0..=AGENT_ACTIONS_MAX)
            .map(|i| format!("act{i:02} = \"routine\""))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(bad(&with_actions(BASE, &over)).reason_str(), "bad agent-actions");
        // 恰满边界正例：16 条全收（exact_fit_boundaries 同族防线）
        let full = (0..AGENT_ACTIONS_MAX)
            .map(|i| format!("act{i:02} = \"routine\""))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(ok(&with_actions(BASE, &full)).agent_actions.len(), AGENT_ACTIONS_MAX);
    }

    // 32f. 缺席/空节：空集 = 缺席同义（add-only 执法锚——v0 旧包零漂移）；
    // 空节文本解析合法（空 [runtime]/[ui] 先例同款，文本形态由 mkapp 规范
    // 形门禁拒）；节名拼错仍走 unknown section（白名单只收一字不差一形）。
    #[test]
    fn agent_actions_absent_empty() {
        let m = ok(BASE);
        assert!(m.agent_actions.is_empty());
        assert_eq!(m.agent_actions.len(), 0);
        // 空节合法（= 空集）
        let s = BASE.replace("\n[capabilities]", "\n[agent-actions]\n[capabilities]");
        let m = ok(&s);
        assert!(m.agent_actions.is_empty());
        // 未知节仍拒：下划线/单数/连写变体全不收
        for sec in ["[agent_actions]", "[agent-action]", "[agentactions]"] {
            let s = BASE.replace("[runtime]", sec);
            assert_eq!(bad(&s).reason_str(), "unknown section", "{sec}");
        }
        // 重复节
        let s = format!(
            "{}[agent-actions]\nsave = \"sensitive\"\n",
            with_actions(BASE, "open = \"routine\"\n")
        );
        assert_eq!(bad(&s).reason_str(), "dup section");
    }

    // 32g. RiskTier 词表往返 + 判别值钉板（Intent 两钉同款：大小写敏感、
    // 野词 None；1/2/3 = Veda Risk 枚举序——v2/v3 消费点 as u8 即得编码）。
    #[test]
    fn risk_tier_words_and_discriminants() {
        let pairs = [
            ("routine", RiskTier::Routine),
            ("sensitive", RiskTier::Sensitive),
            ("destructive", RiskTier::Destructive),
        ];
        for (w, v) in pairs {
            assert_eq!(RiskTier::from_word(w), Some(v));
            assert_eq!(v.word(), w);
        }
        assert_eq!(RiskTier::from_word("urgent"), None);
        assert_eq!(RiskTier::from_word("Routine"), None);
        assert_eq!(RiskTier::Routine as u8, 0x01);
        assert_eq!(RiskTier::Sensitive as u8, 0x02);
        assert_eq!(RiskTier::Destructive as u8, 0x03);
    }

    // ---- ADR #82 V6 前置测试面（[blob] 载体节——五字段必全 + 形态帽）----

    /// BASE 尾插 [blob] 段（F9 尾追加定位同款）。
    fn with_blob(base: &str, body: &str) -> String {
        format!("{base}\n[blob]\n{body}")
    }

    /// 五字段齐备基准体（mkapp canonical 键定序同序——F9 双向钉板义务）。
    const BLOB_FULL: &str = "license = \"Apache-2.0\"\norigin = \"soc-vendor-a\"\n\
                              sha256 = \"0123abcd\"\nsize = \"65536\"\n\
                              usage = \"gpu-microcode\"\n";

    // 32h. [blob] 载体节：全在场逐字段钉板 + 缺一逐路拒（声明完整性）+
    // 恰满边界 + 拒面统一 bad blob。
    #[test]
    fn blob_section() {
        let s_full = with_blob(BASE, BLOB_FULL);
        let m = ok(&s_full);
        let b = m.blob.expect("blob");
        assert_eq!(
            (b.license, b.origin, b.sha256, b.size, b.usage),
            ("Apache-2.0", "soc-vendor-a", "0123abcd", "65536", "gpu-microcode")
        );
        // 缺席 = None（add-only 执法锚——旧包零漂移）
        assert_eq!(ok(BASE).blob, None);
        // 缺一逐路拒（五字段各缺一路——「在场则必全」的收尾裁断）
        for k in ["license", "origin", "sha256", "size", "usage"] {
            let body: String = BLOB_FULL
                .lines()
                .filter(|l| !l.starts_with(k))
                .collect::<Vec<_>>()
                .join("\n");
            let s = with_blob(BASE, &format!("{body}\n"));
            assert_eq!(bad(&s).reason_str(), "bad blob", "缺 {k}");
        }
        // 恰满边界：64B 值过（帽值 `<` vs `<=` 漂移防线——sha256 64 位
        // hex 形天然恰满帽）；65B 拒。
        let v64 = "a".repeat(BLOB_FIELD_MAX);
        let s = with_blob(
            BASE,
            &format!("license = \"{v64}\"\norigin = \"o\"\nsha256 = \"s\"\nsize = \"1\"\nusage = \"u\"\n"),
        );
        assert_eq!(ok(&s).blob.unwrap().license.len(), BLOB_FIELD_MAX);
        let v65 = "a".repeat(BLOB_FIELD_MAX + 1);
        for line in [
            format!("license = \"{v65}\""),
            "license = \"\"".to_string(),
            "license = Apache-2.0".to_string(),
        ] {
            let s = with_blob(
                BASE,
                &format!("{line}\norigin = \"o\"\nsha256 = \"s\"\nsize = \"1\"\nusage = \"u\"\n"),
            );
            assert_eq!(bad(&s).reason_str(), "bad blob", "{line:?}");
        }
        // 重复键 / 未知键 / 重复节 / 节名变体（白名单只收一字不差一形）
        let dup = format!("{BLOB_FULL}license = \"MIT\"\n");
        assert_eq!(bad(&with_blob(BASE, &dup)).reason_str(), "bad blob");
        assert_eq!(
            bad(&with_blob(BASE, &format!("{BLOB_FULL}note = \"x\"\n"))).reason_str(),
            "unknown key"
        );
        let s = format!("{}[blob]\nlicense = \"MIT\"\n", with_blob(BASE, BLOB_FULL));
        assert_eq!(bad(&s).reason_str(), "dup section");
        for sec in ["[blobs]", "[Blob]"] {
            let s = BASE.replace("[runtime]", sec);
            assert_eq!(bad(&s).reason_str(), "unknown section", "{sec}");
        }
        // 空节 = 缺席同义 None（空 [runtime]/[compat] 先例——其文本形态
        // 由 mkapp 规范形门禁拒，两级执法互补）。
        let s = BASE.replace("\n[capabilities]", "\n[blob]\n[capabilities]");
        assert_eq!(ok(&s).blob, None);
    }

    // 33. 规范形七形态 F0-F6（§3.2 字节模板，测试内字面常量）：parse Ok +
    // 字段逐项——「mkapp 发射的字节 berry-manifest 必收」双向钉板
    // （mkfat ↔ berry-fatfs fixture 共识同型；mkapp 侧同款样本在 selftest）。
    #[test]
    fn canonical_forms_parse() {
        // F0 v0 旧形（全缺省——add-only 执法锚）
        let f0 = "[app]\nid = \"X\"\nversion = \"0.1.0\"\nentry = \"bin/x\"\ntype = \"app\"\n\
                  intent = \"utility\"\n\n[capabilities]\nconsole = true\n";
        let m = ok(f0);
        assert_eq!((m.id, m.entry), ("X", "bin/x"));
        assert_eq!((m.compat, m.fs, m.net, m.security), (None, None, None, None));

        // F1 +[compat]（[capabilities] 行前插入；compile 后空行与 §3.2/mkapp
        // f1 逐字节一致——跨实现钉板义务，防 canonical 空行形态漂移漏检）
        let f1 = f0.replace(
            "\n[capabilities]",
            "\n[compat]\nmin = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\n\n[capabilities]",
        );
        let m = ok(&f1);
        let c = m.compat.expect("F1 compat");
        assert_eq!((c.min, c.target, c.compile), (SDK_CURRENT, SDK_CURRENT, SDK_CURRENT));

        // F2 +fs（console 行后）
        let f2 = f0.replace("console = true", "console = true\nfs = [\"rootfs\"]");
        assert_eq!(ok(&f2).fs, Some("rootfs"));

        // F3 +net（单独形态）
        let f3 = f0.replace("console = true", "console = true\nnet = [\"client\"]");
        assert_eq!(ok(&f3).net, Some("client"));

        // F4 +[security]（尾部追加；ADR #82 V3 起三键形）
        let f4 = format!(
            "{f0}\n[security]\nprofile = \"ed25519-main\"\nkey_lineage = \"root-2026\"\nintegrator = \"oem-main\"\n"
        );
        let m = ok(&f4);
        let s = m.security.expect("F4 security");
        assert_eq!(
            (s.profile, s.key_lineage, s.integrator),
            ("ed25519-main", "root-2026", "oem-main")
        );

        // F5 +[runtime]（appquiet 现行 manifest 同形——仓内活样本）
        let f5 = "[app]\nid = \"APPQUIET\"\nversion = \"0.1.0\"\nentry = \"bin/appquiet\"\n\
                  type = \"app\"\nintent = \"background\"\n\n[capabilities]\nconsole = false\n\
                  \n[runtime]\nstack_pages = 8\n";
        let m = ok(f5);
        assert!(!m.console);
        assert_eq!(m.stack_pages, 8);

        // F6 全组合 kitchen-sink（五平面叠加；[security] 三键形——V3）
        let f6 = "[app]\nid = \"KITCHEN\"\nversion = \"0.1.0\"\nentry = \"bin/kitchen\"\n\
                  type = \"app\"\nintent = \"utility\"\n\n[compat]\nmin = \"0.1.0\"\n\
                  target = \"0.1.0\"\ncompile = \"0.1.0\"\n\n[capabilities]\nconsole = true\n\
                  fs = [\"rootfs\"]\nnet = [\"client\"]\n\n[runtime]\nstack_pages = 8\n\
                  \n[security]\nprofile = \"ed25519-main\"\nkey_lineage = \"root-2026\"\n\
                  integrator = \"oem-main\"\n";
        let m = ok(f6);
        assert_eq!(m.id, "KITCHEN");
        assert_eq!(m.compat.expect("F6 compat").compile, SDK_CURRENT);
        assert_eq!((m.fs, m.net), (Some("rootfs"), Some("client")));
        assert_eq!(m.stack_pages, 8);
        let s = m.security.expect("F6 security");
        assert_eq!(
            (s.profile, s.key_lineage, s.integrator),
            ("ed25519-main", "root-2026", "oem-main")
        );

        // F7 +[ui]（GUI-7，尾部追加——规范形定序 [ui] 居末、节间恰一空行
        //（mkapp canonical 发射形逐字节互钉）；三键独立故子集形态
        // icon-only 同为规范形）。appdemo 现行 manifest 同形。
        let f7 = format!(
            "{f6}\n[ui]\nicon = \"icons/icon.byi\"\nlabel = \"KITCHEN\"\nviews = \"views/main.bui\"\n"
        );
        let m = ok(&f7);
        let u = m.ui.expect("F7 ui");
        assert_eq!(
            (u.icon, u.label, u.views),
            (Some("icons/icon.byi"), Some("KITCHEN"), Some("views/main.bui"))
        );
        let f7b = format!("{f0}\n[ui]\nicon = \"icons/icon.byi\"\n");
        assert_eq!(ok(&f7b).ui.unwrap().icon, Some("icons/icon.byi"));

        // F8 +[agent-actions]（同意面先落件二，尾部追加——规范形定序居
        // [ui] 之后，mkapp canonical 发射形逐字节互钉；键 = kebab-case
        // 动作名、值 = 三档词，文档序保序）。
        let f8 = format!(
            "{f7}\n[agent-actions]\nopen = \"routine\"\nsave = \"sensitive\"\n\
             empty-trash = \"destructive\"\n"
        );
        let m = ok(&f8);
        let aa = &m.agent_actions;
        assert_eq!(aa.len(), 3);
        assert_eq!(aa.get("open"), Some(RiskTier::Routine));
        assert_eq!(aa.get("empty-trash"), Some(RiskTier::Destructive));

        // F9 +[blob]（ADR #82 V6 前置，尾部追加——规范形定序居
        // [agent-actions] 之后，mkapp canonical 发射形逐字节互钉；五字段
        // 键定序 license/origin/sha256/size/usage，现值不消费只校验形态）。
        let f9 = format!(
            "{f8}\n[blob]\nlicense = \"Apache-2.0\"\norigin = \"soc-vendor-a\"\n\
             sha256 = \"0123abcd\"\nsize = \"65536\"\nusage = \"gpu-microcode\"\n"
        );
        let m = ok(&f9);
        let b = m.blob.expect("F9 blob");
        assert_eq!(b.license, "Apache-2.0");
        assert_eq!(b.usage, "gpu-microcode");
        // 最小形：仅 [blob]（无 [compat]/[security]/[ui]/[agent-actions]
        // ——独立可选节，F9b 同为规范形）。
        let f9b = format!(
            "{f0}\n[blob]\nlicense = \"MIT\"\norigin = \"o\"\nsha256 = \"s\"\nsize = \"1\"\nusage = \"u\"\n"
        );
        assert_eq!(ok(&f9b).blob.unwrap().size, "1");
    }
}
