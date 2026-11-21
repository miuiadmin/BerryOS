//! berryassemble 二刀第二段第一片：`load_product(lite=True)` 结构面镜像——
//! 顶层键两档硬拒 + `schema = 1` + `[product]` 全节（name/vendor/board/
//! apps.{preset,override,priv_grants}/branding/source/trust/defaults.launcher）
//! 键集/类型/值域执法（G4 两段式第一段——零文件对账，lite 语义）。
//!
//! 第二片：`[platform]` 全节执法（六键在场性 + 三枚举 + sideload bool +
//! user 收窄 + boot 锚形/params 词表 + storage 值域/atomicity 档）——
//! 本片后 lite 面与 Python `load_product(lite=True)` 全分支同构。
//! 词表/域界单源复用 lib（`boot_params_spec`/`INTENTS`/`parse_kernel_anchor`）。
//!
//! 第三片：`[product.defaults.locale]` i18n 八键执法（languages BCP 47 形/
//! 1..=8 帽/大小写不敏感查重、timezone Area[/Sub]/Location 段形、region/
//! regulatory_domain ISO 3166-1 alpha-2、calendar 等四枚举词表）——Python
//! `_check_locale_section` 逐枝镜像；BCP 47 简化 ABNF 手写段匹配器（零
//! regex 依赖——段界即 token 界，贪婪+回退同构）。本件未接 CLI
//! （产线零触碰——首刀「只挂测试」先例）。
//!
//! 已知边界（钉界）：①toml_doc 空节不产出——`[product.trust]` 空节 Python
//! 侧「在场 → pk_anchor 必填缺失」vs 本片「缺席跳过」，语面外（第三片随
//! 节在场性追踪收口）；②float/datetime 值 toml_doc 即拒（现役语面无此形）。
//!
//! 文案与 Python 件逐字对齐（`tests/parity.rs::parity_load_lite_product`
//! oracle 真跑钉板）；键序 = 插入序镜像（tomllib dict 语义）。

use crate::toml_doc::{self, Val};
use crate::Reject;
use std::collections::BTreeMap;

// ---- 帽与词表（Python 常量段镜像——对账锚：berry-manifest + 设计 §1.3/1.4）----

pub const APP_SLOTS: usize = 10;
pub const NAME_MAX: usize = 24;
pub const VENDOR_MAX: usize = 16;

const BOARDS: [&str; 1] = ["qemu-virt"];
const CAPS_KEYS: [&str; 3] = ["console", "fs", "net"];
const TOP_KEYS: [&str; 3] = ["platform", "product", "schema"];
/// Python `PLATFORM_KEYS` 镜像（必填六键——`_check_platform_section` 在场
/// 性巡检序：arch/feature_level/build_type/sideload/boot/storage）。
const PLATFORM_KEYS: [&str; 6] = [
    "arch",
    "boot",
    "build_type",
    "feature_level",
    "sideload",
    "storage",
];
const ARCHES: [&str; 2] = ["aarch64", "x86_64"];
const FEATURE_LEVELS: [&str; 1] = ["rich"];
const BUILD_TYPES: [&str; 3] = ["eng", "userdebug", "user"];
const ATOMICITY_LEVELS: [&str; 3] = ["p0", "p1", "p2"];
const SIZE_MIB_RANGE: (u32, u32) = (48, 1024);
const SPLIT_MIB_RANGE: (u32, u32) = (8, 64);
const BOOT_KEYS: [&str; 2] = ["kernel", "params"];
const STORAGE_KEYS: [&str; 3] = ["image_size_mib", "min_power_atomicity", "rootfs_split_mib"];
const PRODUCT_KEYS: [&str; 8] = [
    "apps", "board", "branding", "defaults", "name", "source", "trust", "vendor",
];
const PRODUCT_DEFERRED: [&str; 1] = ["services"];
const APPS_KEYS: [&str; 3] = ["override", "preset", "priv_grants"];
const BRANDING_KEYS: [&str; 2] = ["icon_mask", "wallpaper"];
const BRANDING_DEFERRED: [&str; 1] = ["tokens"];
const DEFAULTS_KEYS: [&str; 2] = ["launcher", "locale"];
const DEFAULTS_DEFERRED: [&str; 1] = ["scheduler"];
const SOURCE_KEYS: [&str; 1] = ["instance"];
const TRUST_KEYS: [&str; 1] = ["pk_anchor"];

// locale 八键（Python LOCALE_KEYS 镜像——§1.5 定形组）
const LOCALE_KEYS: [&str; 8] = [
    "calendar",
    "first_day_of_week",
    "hour_cycle",
    "languages",
    "region",
    "regulatory_domain",
    "temperature_unit",
    "timezone",
];
const LANGUAGES_MAX: usize = 8;
const BCP47_LEN_MAX: usize = 35;
const CALENDARS: [&str; 1] = ["gregory"];
const WEEKDAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
const HOUR_CYCLES: [&str; 4] = ["h11", "h12", "h23", "h24"];
const TEMP_UNITS: [&str; 2] = ["celsius", "fahrenheit"];

// ---- 展平表 → 树视图（段切分；引号键内 '.' 差一档——toml_doc 头注钉界）----

/// 节点：子表 / 叶值（Absent 由 Option 表达——Python `get` 返 None 面）。
enum Node {
    Table(View),
    Leaf(Val),
}

/// 节视图：直接子键 → 节点；键序 = 首现序（插入序镜像 tomllib dict）。
#[derive(Default)]
struct View {
    order: Vec<String>,
    kids: BTreeMap<String, Node>,
}

impl View {
    fn get(&self, key: &str) -> Option<&Node> {
        self.kids.get(key)
    }
    fn contains_key(&self, key: &str) -> bool {
        self.kids.contains_key(key)
    }
}

fn insert_path(view: &mut View, segs: &[&str], val: Val) {
    let head = segs[0];
    if !view.kids.contains_key(head) {
        view.order.push(head.to_string());
    }
    if segs.len() == 1 {
        view.kids.insert(head.to_string(), Node::Leaf(val));
    } else {
        let node = view
            .kids
            .entry(head.to_string())
            .or_insert_with(|| Node::Table(View::default()));
        match node {
            Node::Table(sub) => insert_path(sub, &segs[1..], val),
            _ => panic!("toml_doc path_conflicts 已拒标量/表互覆（不变量破坏）"),
        }
    }
}

fn tree_of(table: &toml_doc::Table) -> View {
    let mut root = View::default();
    for (path, val) in table {
        let segs: Vec<&str> = path.split('.').collect();
        insert_path(&mut root, &segs, val.clone());
    }
    root
}

/// 空节在场性预置：节头路径下若无节点则造空表（有则不动——键条目已建）。
fn seed_empty_table(view: &mut View, segs: &[&str]) {
    let head = segs[0];
    if !view.kids.contains_key(head) {
        view.order.push(head.to_string());
        view.kids.insert(head.to_string(), Node::Table(View::default()));
    }
    if segs.len() > 1 {
        if let Some(Node::Table(sub)) = view.kids.get_mut(head) {
            seed_empty_table(sub, &segs[1..]);
        }
    }
}

// ---- Python repr / type-name 镜像（文案即契约——parity 钉板）----

/// Python list repr（`{v!r}` 镜像——f"[{', '.join(map(repr, …))}]"；
/// pub(crate)：resolve.rs 基座对账面复用——单源不复制。）
pub(crate) fn py_list(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", inner.join(", "))
}

fn py_repr(val: &Val) -> String {
    match val {
        Val::Int(n) => n.to_string(),
        Val::Str(s) => format!("'{s}'"),
        Val::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Val::Arr(a) => py_repr_arr(a),
    }
}

fn py_repr_arr(a: &[Val]) -> String {
    let inner: Vec<String> = a.iter().map(py_repr).collect();
    format!("[{}]", inner.join(", "))
}

/// 节点 Python `type().__name__`（Table → dict）。
fn type_name(n: &Node) -> &'static str {
    match n {
        Node::Table(_) => "dict",
        Node::Leaf(v) => val_type_name(v),
    }
}

/// 叶值 Python `type().__name__`。
fn val_type_name(v: &Val) -> &'static str {
    match v {
        Val::Int(_) => "int",
        Val::Str(_) => "str",
        Val::Bool(_) => "bool",
        Val::Arr(_) => "list",
    }
}

/// 节点 repr（schema `got {…!r}` 面；Table 语面外——corpora 避让）。
fn py_node_repr(n: &Node) -> String {
    match n {
        Node::Table(_) => "{}".to_string(),
        Node::Leaf(v) => py_repr(v),
    }
}

// ---- 通用拒面（Python `_reject_unknown` / `_need_table` 镜像）----

fn reject_unknown(
    sect: &str,
    view: &View,
    allowed: &[&str],
    deferred: &[&str],
) -> Result<(), Reject> {
    let mut unknown: Vec<String> = view
        .order
        .iter()
        .filter(|k| !allowed.contains(&k.as_str()) && !deferred.contains(&k.as_str()))
        .cloned()
        .collect();
    unknown.sort();
    if !unknown.is_empty() {
        let mut a: Vec<&str> = allowed.to_vec();
        a.sort();
        let a: Vec<String> = a.into_iter().map(|s| s.to_string()).collect();
        return Err(Reject(format!(
            "{sect}: 未知键 {}（V12 未知键硬拒——schema 无此键；收 {}）",
            py_list(&unknown),
            py_list(&a)
        )));
    }
    let mut hit: Vec<String> = view
        .order
        .iter()
        .filter(|k| deferred.contains(&k.as_str()))
        .cloned()
        .collect();
    hit.sort();
    if !hit.is_empty() {
        return Err(Reject(format!(
            "{sect}: 键 {} schema 在册、berry-assemble v0 未收录（product 键 add-only 随工具版本化——随 V13 后续刀扩；设计文档 §1.2-§1.5）",
            py_list(&hit)
        )));
    }
    Ok(())
}

fn need_table<'a>(root: &'a View, key: &str) -> Result<&'a View, Reject> {
    match root.get(key) {
        Some(Node::Table(v)) => Ok(v),
        _ => Err(Reject(format!("缺 [{key}] 节（必填表——schema §1.2）"))),
    }
}

// ---- 字符集（Python NAME_RE / VENDOR_RE 手写镜像——零 regex 依赖）----

fn name_charset_ok(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || !b[0].is_ascii_alphanumeric() {
        return false;
    }
    b[1..]
        .iter()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

fn vendor_charset_ok(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

/// Python `e[4:-4]` 切片语义（app/ 前缀 + .bap 后缀盲剥——字符索引非字节
/// 索引，非 ASCII 键不 panic；长度 ≤ 8 时 start≥end 得空串，同 Python 钳位）。
/// （pub(crate)：resolve.rs 基座对账面复用——单源不复制。）
pub(crate) fn app_id_of(entry: &str) -> &str {
    let n = entry.chars().count();
    if n > 8 {
        let start = entry.char_indices().nth(4).unwrap().0;
        let end = entry.char_indices().nth(n - 4).unwrap().0;
        &entry[start..end]
    } else {
        ""
    }
}

fn sorted_dups(items: &[String]) -> Vec<String> {
    let mut s = items.to_vec();
    s.sort();
    let dups: std::collections::BTreeSet<String> = s
        .windows(2)
        .filter(|w| w[0] == w[1])
        .map(|w| w[0].clone())
        .collect();
    dups.into_iter().collect()
}

// ---- locale 形校验（Python BCP47_RE / TZ_RE / REGION_RE 手写镜像——零 regex）----

/// RFC 5646 简化 ABNF 手写段匹配器（Python BCP47_RE IGNORECASE fullmatch
/// 镜像）。策略 = 按 '-' 切段后逐段归亚型（language/extlang/script/region/
/// variant/extension/privateuse），贪婪分配 + 定长回退——子段串唯一分解同
/// Python 正则回溯语义（歧义形现役语面无同席；parity 梯语料钉板）。
fn bcp47_ok(tag: &str) -> bool {
    if tag.is_empty() || tag.chars().count() > BCP47_LEN_MAX {
        return false;
    }
    let segs: Vec<&str> = tag.split('-').collect();
    // privateuse 单独形：x-…
    if segs[0].eq_ignore_ascii_case("x") {
        return segs.len() >= 2
            && segs[1..].iter().all(|s| (1..=8).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric()));
    }
    // language：{2,3} (+0..3 extlang 各 3) | 4 | {5,8}
    let prim = segs[0];
    let pl = prim.chars().count();
    let alpha = |s: &str| s.chars().all(|c| c.is_ascii_alphabetic());
    if !(2..=8).contains(&pl) || !alpha(prim) {
        return false;
    }
    let mut i = 1;
    if (2..=3).contains(&pl) {
        while i < segs.len() && i <= 3 {
            let s = segs[i];
            if s.len() == 3 && alpha(s) {
                i += 1;
            } else {
                break;
            }
        }
    }
    // script：4 alpha（恰 4——5 alpha 语言已耗，无歧义）
    if i < segs.len() && segs[i].chars().count() == 4 && alpha(segs[i]) {
        i += 1;
    }
    // region：2 alpha | 3 digit
    if i < segs.len() {
        let s = segs[i];
        let cl = s.chars().count();
        if (cl == 2 && alpha(s)) || (cl == 3 && s.chars().all(|c| c.is_ascii_digit())) {
            i += 1;
        }
    }
    // variant*：{5,8} alnum | 4 = digit+3 alnum
    while i < segs.len() {
        let s = segs[i];
        let cl = s.chars().count();
        let is_variant = (5..=8).contains(&cl) && s.chars().all(|c| c.is_ascii_alphanumeric())
            || cl == 4 && s.chars().next().map_or(false, |c| c.is_ascii_digit())
                && s.chars().skip(1).all(|c| c.is_ascii_alphanumeric());
        if !is_variant {
            break;
        }
        i += 1;
    }
    // extension*：singleton（alnum ≠ x/X）+ {2,8} alnum 子段 1..=n
    while i < segs.len() {
        let s = segs[i];
        if s.chars().count() != 1 || !s.chars().next().map_or(false, |c| c.is_ascii_alphanumeric() && c != 'x' && c != 'X') {
            break;
        }
        i += 1;
        let mut sub = 0;
        while i < segs.len() {
            let t = segs[i];
            if (2..=8).contains(&t.chars().count()) && t.chars().all(|c| c.is_ascii_alphanumeric()) {
                i += 1;
                sub += 1;
            } else {
                break;
            }
        }
        if sub == 0 {
            return false;
        }
    }
    // 私有段后缀：x + {1,8} alnum 子段 1..=n
    if i < segs.len() && segs[i].eq_ignore_ascii_case("x") {
        i += 1;
        let mut sub = 0;
        while i < segs.len() {
            let t = segs[i];
            if (1..=8).contains(&t.chars().count()) && t.chars().all(|c| c.is_ascii_alphanumeric()) {
                i += 1;
                sub += 1;
            } else {
                break;
            }
        }
        if sub == 0 {
            return false;
        }
    }
    i == segs.len()
}

/// IANA tz ID 形（Area[/Sub]/Location：段 = [A-Za-z0-9_+-]{1,30}、1..=3 段）。
fn tz_ok(v: &str) -> bool {
    let segs: Vec<&str> = v.split('/').collect();
    (1..=3).contains(&segs.len())
        && segs.iter().all(|s| {
            (1..=30).contains(&s.chars().count())
                && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
        })
}

/// ISO 3166-1 alpha-2（两枚大写字母）。
fn region_ok(v: &str) -> bool {
    v.chars().count() == 2 && v.chars().all(|c| c.is_ascii_uppercase())
}

/// `[product.defaults.locale]` i18n 八键执法（Python `_check_locale_section`
/// 逐枝镜像——文案即契约，parity 真跑钉板）。
fn check_locale_section(p: &mut LiteProduct, sect: &View) -> Result<(), Reject> {
    let node = match sect.get("locale") {
        Some(n) => n,
        None => return Ok(()),
    };
    let lv = match node {
        Node::Table(v) => v,
        _ => {
            return Err(Reject(
                "product.defaults.locale: 应为节（键值表——i18n 八键，§1.5：languages/timezone/region/regulatory_domain/calendar/first_day_of_week/hour_cycle/temperature_unit）"
                    .to_string(),
            ))
        }
    };
    reject_unknown("product.defaults.locale", lv, &LOCALE_KEYS, &[])?;
    // languages：BCP 47 数组 1..=8 + 大小写不敏感查重
    if let Some(node) = lv.get("languages") {
        let langs = match node {
            Node::Leaf(Val::Arr(a)) => a.clone(),
            _ => {
                return Err(Reject(
                    "product.defaults.locale.languages: 应为字符串数组（BCP 47 well-formed 列表，1..=8 项——§1.5）"
                        .to_string(),
                ))
            }
        };
        if !(1..=LANGUAGES_MAX).contains(&langs.len()) {
            return Err(Reject(format!(
                "product.defaults.locale.languages: {} 项越 [1,{}] 帽（帽=推断：语言列表场景足够，防清单膨胀——§1.5）",
                langs.len(), LANGUAGES_MAX
            )));
        }
        let mut items: Vec<String> = Vec::new();
        for (i, tag) in langs.iter().enumerate() {
            let t = match tag {
                Val::Str(s) => s.clone(),
                other => {
                    return Err(Reject(format!(
                        "product.defaults.locale.languages[{i}]: 应为字符串（got {}）",
                        val_type_name(other)
                    )))
                }
            };
            if t.chars().count() > BCP47_LEN_MAX || !bcp47_ok(&t) {
                return Err(Reject(format!(
                    "product.defaults.locale.languages[{i}]: BCP 47 well-formed 不符 '{t}'（RFC 5646 langtag/privateuse 形——语言[-文字][-地区][-变体…]；grandfathered 弃用族不收——§1.5）"
                )));
            }
            items.push(t);
        }
        let lowered: Vec<String> = items.iter().map(|t| t.to_lowercase()).collect();
        let dups = sorted_dups(&lowered);
        if !dups.is_empty() {
            return Err(Reject(format!(
                "product.defaults.locale.languages: 条目重复 {}（BCP 47 大小写不敏感查重——§1.5）",
                py_list(&dups)
            )));
        }
        p.locale_languages = items;
    }
    // timezone / region / regulatory_domain：非空串 + 各自形
    for key in ["timezone", "region", "regulatory_domain"] {
        let v = match lv.get(key) {
            Some(Node::Leaf(Val::Str(s))) => s.clone(),
            Some(other) => {
                return Err(Reject(format!(
                    "product.defaults.locale.{key}: 应为非空字符串（got {}）",
                    type_name(other)
                )))
            }
            None => continue,
        };
        if v.is_empty() {
            return Err(Reject(format!(
                "product.defaults.locale.{key}: 应为非空字符串（got str）"
            )));
        }
        if key == "timezone" {
            if !tz_ok(&v) {
                return Err(Reject(format!(
                    "product.defaults.locale.timezone: IANA tz ID 形不符 '{}'（Area[/Sub]/Location 段形〔段内 [A-Za-z0-9_+-]、1..=3 段〕——词表 = iana.org 时区集，不做仓内枚举；§1.5）",
                    v
                )));
            }
            p.locale_timezone = v;
        } else {
            if !region_ok(&v) {
                return Err(Reject(format!(
                    "product.defaults.locale.{key}: 应为 ISO 3166-1 alpha-2（两枚大写字母；got '{}'——§1.5）",
                    v
                )));
            }
            if key == "region" {
                p.locale_region = v;
            } else {
                p.locale_regulatory_domain = v;
            }
        }
    }
    // 四枚举：calendar / first_day_of_week / hour_cycle / temperature_unit
    for (key, vocab) in [
        ("calendar", &CALENDARS[..]),
        ("first_day_of_week", &WEEKDAYS[..]),
        ("hour_cycle", &HOUR_CYCLES[..]),
        ("temperature_unit", &TEMP_UNITS[..]),
    ] {
        let node = match lv.get(key) {
            Some(n) => n,
            None => continue,
        };
        let ok = matches!(node, Node::Leaf(Val::Str(s)) if vocab.contains(&s.as_str()));
        if !ok {
            let vocab_list: Vec<String> = vocab.iter().map(|s| s.to_string()).collect();
            return Err(Reject(format!(
                "product.defaults.locale.{key}: 枚举外值 {}（词表 {}——add-only，§1.5）",
                py_node_repr(node),
                py_list(&vocab_list)
            )));
        }
        if let Node::Leaf(Val::Str(s)) = node {
            let sv = s.clone();
            match key {
                "calendar" => p.locale_calendar = sv,
                "first_day_of_week" => p.locale_first_day_of_week = sv,
                "hour_cycle" => p.locale_hour_cycle = sv,
                _ => p.locale_temperature_unit = sv,
            }
        }
    }
    Ok(())
}

/// lite `Val` → lib `toml_val::Val` 转换桥（params 词表执法单源复用——
/// 二刀第二片：`[platform.boot.params]` 不再长第二套词表/域界实现）。
fn to_lib_val(v: &Val) -> crate::toml_val::Val {
    match v {
        Val::Int(n) => crate::toml_val::Val::Int(*n),
        Val::Str(s) => crate::toml_val::Val::Str(s.clone()),
        Val::Bool(b) => crate::toml_val::Val::Bool(*b),
        Val::Arr(a) => crate::toml_val::Val::List(a.iter().map(to_lib_val).collect()),
    }
}

/// View params 节 → 有序键值对（键序 = 首现序——`check_boot_params` 的
/// BTreeMap 只用于确定性序列化面，拒面先检不受键序影响）。
fn view_pairs(v: &View) -> Vec<(String, Val)> {
    v.order
        .iter()
        .filter_map(|k| match v.get(k) {
            Some(Node::Leaf(val)) => Some((k.clone(), val.clone())),
            _ => None,
        })
        .collect()
}

/// Python `枚举外值 {v!r}` 拒面（`isinstance(v, str) and v in vocab` 单
/// 分支——非串与枚举外同文案，got 段 = repr；表形走 dict repr 面）。
fn vocab_check(sect_key: &str, node: &Node, vocab: &[&str], why: &str) -> Result<(), Reject> {
    let ok = matches!(node, Node::Leaf(Val::Str(s)) if vocab.contains(&s.as_str()));
    if !ok {
        let vocab_list: Vec<String> = vocab.iter().map(|s| s.to_string()).collect();
        return Err(Reject(format!(
            "{sect_key}: 枚举外值 {}（词表 {}——{why}）",
            py_node_repr(node),
            py_list(&vocab_list)
        )));
    }
    Ok(())
}

/// `[platform]` 全节执法（Python `_check_platform_section` 逐枝镜像）。
fn check_platform_section(p: &mut LiteProduct, sect: &View) -> Result<(), Reject> {
    reject_unknown("platform", sect, &PLATFORM_KEYS, &[])?;
    // 必填六键巡检（Python for key in (…) 序）
    for key in ["arch", "feature_level", "build_type", "sideload", "boot", "storage"] {
        if !sect.contains_key(key) {
            return Err(Reject(format!(
                "platform.{key}: 必填缺失（v0 存在性校验——schema §1.3）"
            )));
        }
    }
    vocab_check(
        "platform.arch",
        sect.get("arch").unwrap(),
        &ARCHES,
        "x86_64=最小集",
    )?;
    vocab_check(
        "platform.feature_level",
        sect.get("feature_level").unwrap(),
        &FEATURE_LEVELS,
        "Q7：v0 只 rich 在册（骨架 add-only）",
    )?;
    vocab_check(
        "platform.build_type",
        sect.get("build_type").unwrap(),
        &BUILD_TYPES,
        "user 型收窄见 §1.7",
    )?;
    let sideload = match sect.get("sideload") {
        Some(Node::Leaf(Val::Bool(b))) => *b,
        Some(other) => {
            return Err(Reject(format!(
                "platform.sideload: 应为 bool（got {}）",
                type_name(other)
            )))
        }
        None => {
            return Err(Reject(
                "platform.sideload: 应为 bool（got NoneType）".to_string(),
            ))
        }
    };
    if matches!(sect.get("build_type"), Some(Node::Leaf(Val::Str(s))) if s == "user")
        && sideload
    {
        return Err(Reject(
            "platform.sideload: user 型下必须缺省或 false（§1.3 收窄清单——v1 立法面）"
                .to_string(),
        ));
    }
    if let Some(Node::Leaf(Val::Str(s))) = sect.get("arch") {
        p.arch = s.clone();
    }
    if let Some(Node::Leaf(Val::Str(s))) = sect.get("feature_level") {
        p.feature_level = s.clone();
    }
    p.build_type = match sect.get("build_type") {
        Some(Node::Leaf(Val::Str(s))) => s.clone(),
        _ => String::new(),
    };
    p.sideload = sideload;

    // [platform.boot]——kernel 工件锚 + params 词表子表
    let boot = need_table(sect, "boot")?;
    reject_unknown("platform.boot", boot, &BOOT_KEYS, &[])?;
    let kernel = match boot.get("kernel") {
        Some(Node::Leaf(Val::Str(s))) if !s.is_empty() => s.clone(),
        _ => {
            return Err(Reject(
                "platform.boot.kernel: 应为非空字符串（必填——工件锚）".to_string(),
            ))
        }
    };
    let (kname, ksha) = crate::parse_kernel_anchor(&kernel)?;
    p.kernel_name = kname;
    p.kernel_sha = ksha;
    let params = match boot.get("params") {
        Some(Node::Table(v)) => v,
        Some(_) => {
            return Err(Reject(
                "platform.boot.params: 应为节（键值表）".to_string(),
            ))
        }
        None => {
            // Python `boot.get("params", {})` 缺席 = 空表（零拒面路径）
            return check_platform_storage(p, sect);
        }
    };
    let pairs = view_pairs(params);
    if pairs.len() > crate::PARAMS_MAX {
        return Err(Reject(format!(
            "platform.boot.params: {} 键越帽 {}（§1.6-4 解析面定界——INITRD 段 count 帽 32）",
            pairs.len(),
            crate::PARAMS_MAX
        )));
    }
    let lib_pairs: std::collections::BTreeMap<String, crate::toml_val::Val> =
        pairs
            .iter()
            .map(|(k, v)| (k.clone(), to_lib_val(v)))
            .collect();
    crate::check_boot_params(&lib_pairs)?;
    for (k, v) in pairs {
        if let Val::Str(s) = v {
            p.params.insert(k, s);
        } else if let Val::Int(n) = v {
            p.params.insert(k, n.to_string());
        }
    }
    check_platform_storage(p, sect)
}

/// `[platform.storage]` 产物几何 + atomicity 档执法。
fn check_platform_storage(p: &mut LiteProduct, sect: &View) -> Result<(), Reject> {
    let storage = need_table(sect, "storage")?;
    reject_unknown("platform.storage", storage, &STORAGE_KEYS, &[])?;
    for key in STORAGE_KEYS {
        if !storage.contains_key(key) {
            return Err(Reject(format!(
                "platform.storage.{key}: 必填缺失（schema §1.3）"
            )));
        }
    }
    let img = match storage.get("image_size_mib") {
        Some(Node::Leaf(v)) => to_lib_val(v),
        _ => crate::toml_val::Val::Other,
    };
    let img = crate_need_u32("platform.storage", "image_size_mib", &img, SIZE_MIB_RANGE.0, SIZE_MIB_RANGE.1)?;
    p.image_size_mib = img;
    let split = match storage.get("rootfs_split_mib") {
        Some(Node::Leaf(v)) => to_lib_val(v),
        _ => crate::toml_val::Val::Other,
    };
    let split = crate_need_u32("platform.storage", "rootfs_split_mib", &split, SPLIT_MIB_RANGE.0, SPLIT_MIB_RANGE.1)?;
    p.rootfs_split_mib = split;
    let pa = match storage.get("min_power_atomicity") {
        Some(Node::Leaf(v)) => v.clone(),
        _ => Val::Arr(vec![]),
    };
    if !matches!(&pa, Val::Str(s) if ATOMICITY_LEVELS.contains(&s.as_str())) {
        let vocab: Vec<String> = ATOMICITY_LEVELS.iter().map(|s| s.to_string()).collect();
        return Err(Reject(format!(
            "platform.storage.min_power_atomicity: 枚举外值 {}（词表 {}——§2.4）",
            py_repr(&pa),
            py_list(&vocab)
        )));
    }
    let pa_str = match &pa {
        Val::Str(s) => s.clone(),
        _ => String::new(),
    };
    if pa_str != "p0" {
        return Err(Reject(format!(
            "platform.storage.min_power_atomicity: 词表内但 v1 只 p0 可组（{pa_str} 无在册实现可满足——§1.3/§2.4 词表先立值域收窄，08 号 §6-Q11 采纳形）"
        )));
    }
    p.min_power_atomicity = pa_str;
    Ok(())
}

/// lib 面 `need_u32` 复用桥（lite Val → lib Val 后调 lib 私有实现的公开
/// 包装——零第二套域界文案）。
fn crate_need_u32(
    sect: &str,
    key: &str,
    v: &crate::toml_val::Val,
    lo: u32,
    hi: u32,
) -> Result<u32, Reject> {
    // 直接走 lib 的 check_boot_params 通道外露的单值域界执法——
    // 与 Python `_need_u32` 文案逐字对齐（got 段 = type name）。
    crate::check_single_u32(sect, key, v, lo, hi)
}



/// `load_product(lite=True)` 的过检视图（结构面——第一片 [product] 全节 +
/// 第二片 [platform] 全节）。
#[derive(Debug, Default, PartialEq)]
pub struct LiteProduct {
    pub name: String,
    pub vendor: String,
    pub board: String,
    pub preset: Vec<String>,
    pub apps_override: BTreeMap<String, String>,
    pub grants_issued_by: String,
    pub priv_grants: BTreeMap<String, Vec<String>>,
    pub branding: BTreeMap<String, String>,
    pub launcher: String,
    pub trust_pk_anchor: String,
    pub source_instance: String,
    pub arch: String,
    pub feature_level: String,
    pub build_type: String,
    pub sideload: bool,
    pub params: BTreeMap<String, String>,
    pub kernel_name: String,
    pub kernel_sha: String,
    pub image_size_mib: u32,
    pub rootfs_split_mib: u32,
    pub min_power_atomicity: String,
    pub locale_languages: Vec<String>,
    pub locale_timezone: String,
    pub locale_region: String,
    pub locale_regulatory_domain: String,
    pub locale_calendar: String,
    pub locale_first_day_of_week: String,
    pub locale_hour_cycle: String,
    pub locale_temperature_unit: String,
}

/// locale 节开关位（Python `if prod.locale` 镜像——空 dict falsy）。
/// 三刀 CLI 消费面（--check-lite OK 行 locale=on/off）。
impl LiteProduct {
    pub fn locale_on(&self) -> bool {
        !(self.locale_languages.is_empty()
            && self.locale_timezone.is_empty()
            && self.locale_region.is_empty()
            && self.locale_regulatory_domain.is_empty()
            && self.locale_calendar.is_empty()
            && self.locale_first_day_of_week.is_empty()
            && self.locale_hour_cycle.is_empty()
            && self.locale_temperature_unit.is_empty())
    }
}

/// 结构面执法主入口（Python `load_product(path, lite=True)` 镜像——文本入，
/// 零文件对账）。检查序与 Python 逐枝对齐（首拒分支双侧同构）。
pub fn load_lite_product(text: &str) -> Result<LiteProduct, Reject> {
    let (table, sections) = toml_doc::parse_with_sections(text).map_err(|e| Reject(e.0))?;
    let root = tree_of(&table);
    // 节在场性预置：tomllib 语义「节头出现 = table 在场（空也在场）」——
    // 空节在 Table 零条目，经 sections 清单补空 View 节点（缺 pk_anchor
    // 等必填执法分支由此可达）。与键路径冲突面：节头已过 path_conflicts
    // （toml_doc），此处 insert 只补缺省空表不覆叶。
    let mut root = root;
    for sec in &sections {
        let segs: Vec<&str> = sec.split('.').collect();
        seed_empty_table(&mut root, &segs);
    }
    reject_unknown("(顶层)", &root, &TOP_KEYS, &[])?;
    match root.get("schema") {
        None => {
            return Err(Reject(
                "schema: 必填缺失（清单 schema 版本——§1.2）".to_string(),
            ))
        }
        Some(Node::Leaf(Val::Int(1))) => {}
        Some(other) => {
            return Err(Reject(format!(
                "schema: 须 = 1（u32；v1 唯一值、升版 add-only——got {}）",
                py_node_repr(other)
            )))
        }
    }
    let mut p = LiteProduct::default();
    check_product_section(&mut p, need_table(&root, "product")?)?;
    check_platform_section(&mut p, need_table(&root, "platform")?)?;
    Ok(p)
}

fn check_product_section(p: &mut LiteProduct, sect: &View) -> Result<(), Reject> {
    reject_unknown("product", sect, &PRODUCT_KEYS, &PRODUCT_DEFERRED)?;
    for (key, maxlen) in [("name", NAME_MAX), ("vendor", VENDOR_MAX)] {
        let v = match sect.get(key) {
            Some(Node::Leaf(Val::Str(s))) => s.clone(),
            Some(other) => {
                return Err(Reject(format!(
                    "product.{key}: 应为字符串（got {}）",
                    type_name(other)
                )))
            }
            None => {
                return Err(Reject(format!(
                    "product.{key}: 应为字符串（got NoneType）"
                )))
            }
        };
        let n = v.chars().count(); // Python len() = 字符数（非字节数——非 ASCII 同口径）
        if !(1..=maxlen).contains(&n) {
            return Err(Reject(format!(
                "product.{key}: 空/超长（1..={maxlen}，got {n}B）"
            )));
        }
        let ok = if key == "name" {
            name_charset_ok(&v)
        } else {
            vendor_charset_ok(&v)
        };
        if !ok {
            return Err(Reject(format!(
                "product.{key}: 字符集不符（name=[A-Za-z0-9][A-Za-z0-9._-]、vendor=[a-z0-9-]——got '{v}'）"
            )));
        }
        if key == "name" {
            p.name = v;
        } else {
            p.vendor = v;
        }
    }
    let board = match sect.get("board") {
        Some(Node::Leaf(Val::Str(s))) => s.clone(),
        Some(other) => {
            return Err(Reject(format!(
                "product.board: 应为非空字符串（got {}）",
                type_name(other)
            )))
        }
        None => {
            return Err(Reject(
                "product.board: 应为非空字符串（got NoneType）".to_string(),
            ))
        }
    };
    if board.is_empty() {
        return Err(Reject(
            "product.board: 应为非空字符串（got str）".to_string(),
        ));
    }
    if !BOARDS.contains(&board.as_str()) {
        let boards: Vec<String> = BOARDS.iter().map(|s| s.to_string()).collect();
        return Err(Reject(format!(
            "product.board: 不在板表 {}（v0 板表单值——板表=平台仓数据文件，P8 前治理见 18 号增补 A；got '{board}'）",
            py_list(&boards)
        )));
    }
    p.board = board;

    // [product.apps]——preset 必填 + override/priv_grants 可选（节在则执法）
    let apps = match sect.get("apps") {
        Some(Node::Table(v)) => v,
        _ => {
            return Err(Reject(
                "缺 [product.apps].preset 节（v0 预装集必填——schema §1.2）".to_string(),
            ))
        }
    };
    reject_unknown("product.apps", apps, &APPS_KEYS, &[])?;
    let preset = match apps.get("preset") {
        Some(Node::Leaf(Val::Arr(a))) => a.clone(),
        Some(other) => {
            return Err(Reject(format!(
                "product.apps.preset: 应为路径数组（got {}）",
                type_name(other)
            )))
        }
        None => {
            return Err(Reject(
                "product.apps.preset: 应为路径数组（got NoneType）".to_string(),
            ))
        }
    };
    let mut items: Vec<String> = Vec::new();
    for (i, item) in preset.iter().enumerate() {
        let s = match item {
            Val::Str(s) => s.clone(),
            other => {
                return Err(Reject(format!(
                    "product.apps.preset[{i}]: 应为字符串（got {}）",
                    val_type_name(other)
                )))
            }
        };
        if !s.starts_with("app/") {
            return Err(Reject(format!(
                "product.apps.preset[{i}]: 非 app/ 前缀路径 '{s}'（预装集 = rootfs app/ 条目——schema §1.4）"
            )));
        }
        items.push(s);
    }
    let dups = sorted_dups(&items);
    if !dups.is_empty() {
        return Err(Reject(format!(
            "product.apps.preset: 条目重复 {}（查重——schema §1.4）",
            py_list(&dups)
        )));
    }
    if items.len() > APP_SLOTS {
        return Err(Reject(format!(
            "product.apps.preset: {} 项越 APP_SLOTS 帽 {APP_SLOTS}（berry-manifest 常量——schema §1.4）",
            items.len()
        )));
    }
    p.preset = items;
    let preset_ids: std::collections::BTreeSet<String> =
        p.preset.iter().map(|e| app_id_of(e).to_string()).collect();

    // [product.apps.override]——G1 键位（lite：包内存在校验让位全量）
    if let Some(node) = apps.get("override") {
        let ov = match node {
            Node::Table(v) => v,
            _ => {
                return Err(Reject(
                    "product.apps.override: 应为节（键值表——{app_id = \"<path>\"}；G1 键位，schema §1.4-补二推断）"
                        .to_string(),
                ))
            }
        };
        for app_id in &ov.order {
            if !preset_ids.contains(app_id) {
                return Err(Reject(format!(
                    "product.apps.override.{app_id}: 不在 preset 预装集（'app/{app_id}.bap' 未列——v1 语义 = 限替换已声明 app、不做纯新增〔推断标注，正式定形随 #83〕；基座在册未进 preset 者同拒）"
                )));
            }
            match ov.get(app_id) {
                Some(Node::Leaf(Val::Str(rel))) if !rel.is_empty() => {
                    p.apps_override.insert(app_id.clone(), rel.clone());
                }
                _ => {
                    return Err(Reject(format!(
                        "product.apps.override.{app_id}: 应为非空字符串路径（OEM 自有 .bap——相对 product.toml 所在目录解析）"
                    )))
                }
            }
        }
    }

    // [product.apps.priv_grants]——V15 步② 白名单
    if let Some(node) = apps.get("priv_grants") {
        let grants = match node {
            Node::Table(v) => v,
            _ => {
                return Err(Reject(
                    "product.apps.priv_grants: 应为节（键值表——{app_id → [能力键]} + issued_by；schema §1.4）"
                        .to_string(),
                ))
            }
        };
        if let Some(Node::Leaf(Val::Arr(a))) = grants.get("issued_by") {
            return Err(Reject(format!(
                "product.apps.priv_grants.issued_by: 保留子键不得撞保留名（issued_by = V8 签发者 lineage 子字段，不得作 app_id；got 数组 {}——设计 §1.4）",
                py_repr_arr(a)
            )));
        }
        let issued = match grants.get("issued_by") {
            Some(Node::Leaf(Val::Str(s))) => s.clone(),
            _ => {
                return Err(Reject(
                    "product.apps.priv_grants.issued_by: 必填缺失（节存在时必填——白名单「谁预置谁负责」的机器对应物，应用规范 §11.4）"
                        .to_string(),
                ))
            }
        };
        // VENDOR_RE {1,16} 双臂：长度帽（字符数）+ 字符集
        if issued.chars().count() > VENDOR_MAX || !vendor_charset_ok(&issued) {
            return Err(Reject(format!(
                "product.apps.priv_grants.issued_by: 应为签发者 lineage 串（1..=16B [a-z0-9-]——product.vendor/manifest integrator 同域；got '{issued}'）"
            )));
        }
        p.grants_issued_by = issued;
        for app_id in &grants.order {
            if app_id == "issued_by" {
                continue;
            }
            if !preset_ids.contains(app_id) {
                return Err(Reject(format!(
                    "product.apps.priv_grants.{app_id}: 不在 preset 预装集（'app/{app_id}.bap' 未列——白名单只授预装件；应用规范 §11.2 未列而申请=拒授，构建期先咬）"
                )));
            }
            let caps = match grants.get(app_id) {
                Some(Node::Leaf(Val::Arr(a))) => a.clone(),
                Some(other) => {
                    return Err(Reject(format!(
                        "product.apps.priv_grants.{app_id}: 应为能力键数组（got {}）",
                        type_name(other)
                    )))
                }
                None => {
                    return Err(Reject(format!(
                        "product.apps.priv_grants.{app_id}: 应为能力键数组（got NoneType）"
                    )))
                }
            };
            let mut caps_list: Vec<String> = Vec::new();
            for c in &caps {
                let cs = match c {
                    Val::Str(s) => s.clone(),
                    other => {
                        return Err(Reject(format!(
                            "product.apps.priv_grants.{app_id}: 能力键应为字符串（got {}）",
                            val_type_name(other)
                        )))
                    }
                };
                if !CAPS_KEYS.contains(&cs.as_str()) {
                    let vocab: Vec<String> =
                        CAPS_KEYS.iter().map(|s| s.to_string()).collect();
                    return Err(Reject(format!(
                        "product.apps.priv_grants.{app_id}: 未知能力键 '{cs}'（值域 ⊆ 现役 [caps] 闭集 {}——berry-manifest Sec::Caps × mkapp [capabilities] 键集两源对齐；G2/G3 分级词表（应用规范 §11.2）为立法文本、仓内无机器可读分级表——v0 键级闭集先行，分级校验留开放问题）",
                        py_list(&vocab)
                    )));
                }
                caps_list.push(cs);
            }
            let dups = sorted_dups(&caps_list);
            if !dups.is_empty() {
                return Err(Reject(format!(
                    "product.apps.priv_grants.{app_id}: 能力键重复 {}（查重——白名单授权面单键单义）",
                    py_list(&dups)
                )));
            }
            p.priv_grants.insert(app_id.clone(), caps_list);
        }
    }

    // [product.branding]——品牌资源路径（lite：包内存在校验让位全量）
    if let Some(node) = sect.get("branding") {
        let br = match node {
            Node::Table(v) => v,
            _ => {
                return Err(Reject(
                    "product.branding: 应为节（键值表）".to_string(),
                ))
            }
        };
        reject_unknown("product.branding", br, &BRANDING_KEYS, &BRANDING_DEFERRED)?;
        for key in &br.order {
            match br.get(key) {
                Some(Node::Leaf(Val::Str(rel))) if !rel.is_empty() => {
                    p.branding.insert(key.clone(), rel.clone());
                }
                _ => {
                    return Err(Reject(format!(
                        "product.branding.{key}: 应为非空字符串路径"
                    )))
                }
            }
        }
    }

    // [product.source]——官方源节（步⑦½；lite：目录/两件套/深校验让位全量）
    if let Some(node) = sect.get("source") {
        let sv = match node {
            Node::Table(v) => v,
            _ => {
                return Err(Reject(
                    "product.source: 应为节（键值表——{instance = \"<dir>\"}；schema §推断，SDK-2 v1.2 §五）"
                        .to_string(),
                ))
            }
        };
        reject_unknown("product.source", sv, &SOURCE_KEYS, &[])?;
        let inst = match sv.get("instance") {
            Some(Node::Leaf(Val::Str(s))) => s.clone(),
            _ => {
                return Err(Reject(
                    "product.source.instance: 应为非空字符串路径（必填——两件套目录，gensourceindex --out-dir 产物）"
                        .to_string(),
                ))
            }
        };
        if inst.is_empty() {
            return Err(Reject(
                "product.source.instance: 应为非空字符串路径（必填——两件套目录，gensourceindex --out-dir 产物）"
                    .to_string(),
            ));
        }
        p.source_instance = inst;
    }

    // [product.trust]——信任锚节（G2；lite：件在席 + 32B 恰长让位全量）
    if let Some(node) = sect.get("trust") {
        let tv = match node {
            Node::Table(v) => v,
            _ => {
                return Err(Reject(
                    "product.trust: 应为节（键值表——{pk_anchor = \"<path>\"}；G2 键位，schema §1.4-补二推断）"
                        .to_string(),
                ))
            }
        };
        reject_unknown("product.trust", tv, &TRUST_KEYS, &[])?;
        let rel = match tv.get("pk_anchor") {
            None => {
                return Err(Reject(
                    "product.trust.pk_anchor: 必填缺失（节存在时必填——32B raw 公钥锚件路径，相对 product.toml 所在目录）"
                        .to_string(),
                ))
            }
            Some(Node::Leaf(Val::Str(s))) => s.clone(),
            Some(other) => {
                return Err(Reject(format!(
                    "product.trust.pk_anchor: 应为非空字符串路径（got {}）",
                    type_name(other)
                )))
            }
        };
        if rel.is_empty() {
            return Err(Reject(
                "product.trust.pk_anchor: 应为非空字符串路径（got str）".to_string(),
            ));
        }
        p.trust_pk_anchor = rel;
    }

    // [product.defaults]——默认应用（locale 八键归第三片——出现即 loud 拒）
    let dv = match sect.get("defaults") {
        Some(Node::Table(v)) => v,
        _ => {
            return Err(Reject(
                "缺 [product.defaults] 节（必填表——schema §1.2）".to_string(),
            ))
        }
    };
    reject_unknown("product.defaults", dv, &DEFAULTS_KEYS, &DEFAULTS_DEFERRED)?;
    let launcher = match dv.get("launcher") {
        Some(Node::Leaf(Val::Str(s))) => s.clone(),
        _ => {
            return Err(Reject(
                "product.defaults.launcher: 应为非空字符串（必填——v0 默认应用）".to_string(),
            ))
        }
    };
    if launcher.is_empty() {
        return Err(Reject(
            "product.defaults.launcher: 应为非空字符串（必填——v0 默认应用）".to_string(),
        ));
    }
    if !(launcher.starts_with("app/") || launcher.starts_with("bin/")) {
        return Err(Reject(format!(
            "product.defaults.launcher: 应为 app/ 或 bin/ 前缀路径（Q13 过渡：v1 bin/ 合法、.bap 转正后收 app/——got '{launcher}'）"
        )));
    }
    p.launcher = launcher;
    // [product.defaults.locale]——i18n 八键（第三片——Python 逐枝镜像）
    check_locale_section(p, dv)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good_min() -> String {
        [
            "schema = 1\n",
            "[product]\n",
            "name = \"p\"\n",
            "vendor = \"o\"\n",
            "board = \"qemu-virt\"\n",
            "[product.apps]\n",
            "preset = [\"app/A.bap\"]\n",
            "[product.defaults]\n",
            "launcher = \"bin/sh\"\n",
            "[platform]\n",
            "arch = \"x86_64\"\n",
            "feature_level = \"rich\"\n",
            "build_type = \"eng\"\n",
            "sideload = false\n",
            "[platform.boot]\n",
            "kernel = \"krnl@sha256:",
        ]
        .concat()
            + &"a".repeat(64)
            + "\"\n[platform.storage]\nimage_size_mib = 96\nrootfs_split_mib = 32\nmin_power_atomicity = \"p0\"\n"
    }

    #[test]
    fn lite_product_law() {
        let p = load_lite_product(&good_min()).unwrap();
        assert_eq!(p.name, "p");
        assert_eq!(p.vendor, "o");
        assert_eq!(p.board, "qemu-virt");
        assert_eq!(p.preset, vec!["app/A.bap".to_string()]);
        assert_eq!(p.launcher, "bin/sh");
        // app_id 剥离律（Python e[4:-4] 盲剥镜像）：.bap 后缀非必须
        assert_eq!(app_id_of("app/SETTINGS.bap"), "SETTINGS");
        assert_eq!(app_id_of("app/X"), "");
        // 拒面抽查（全谱归 parity oracle 对拍）
        assert!(load_lite_product(&good_min().replacen("board = \"qemu-virt\"", "board = \"rpi4\"", 1)).is_err());
        assert!(load_lite_product("schema = 2\n").is_err()); // 须 = 1 先于缺节
        assert!(load_lite_product("schema = 1\n[platform]\narch = \"x86_64\"\n").is_err()); // 缺 [product]
        // 第三片 locale：八键全席过检 + 过检视图字段
        let full = good_min().replacen(
            "[platform]\n",
            "[product.defaults.locale]\nlanguages = [\"zh-Hans-CN\", \"en-US\"]\ntimezone = \"Asia/Shanghai\"\nregion = \"CN\"\nregulatory_domain = \"CN\"\ncalendar = \"gregory\"\nfirst_day_of_week = \"mon\"\nhour_cycle = \"h23\"\ntemperature_unit = \"celsius\"\n[platform]\n",
            1,
        );
        let lp = load_lite_product(&full).unwrap();
        assert_eq!(lp.locale_languages.len(), 2);
        assert_eq!(lp.locale_timezone, "Asia/Shanghai");
        assert_eq!(lp.locale_region, "CN");
        assert_eq!(lp.locale_hour_cycle, "h23");
        // bcp47_ok 手写 matcher 单源钉板（Python BCP47_RE 梯形——parity 面
        // 全谱对拍，此处钉结构性关键形）
        for good in ["zh", "zh-Hans", "zh-Hans-CN", "zh-cmn-Hans-CN", "en-419",
                     "de-CH-1901", "sl-rozaj-biske", "hy-Latn-IT-arevela",
                     "en-a-bbb-x-a-ccc", "x-pig-latin", "ZH-HANS", "abcdefgh"] {
            assert!(bcp47_ok(good), "rust 拒了 accept 形 {good}");
        }
        for bad in ["z", "7z", "zh-", "-zh", "zh--cn", "x-", "i-klingon",
                    &"a".repeat(36), "zh-x-9toolongsegment"] {
            assert!(!bcp47_ok(bad), "rust 过了 reject 形 {bad}");
        }
        assert!(tz_ok("UTC") && tz_ok("Asia/Shanghai") && tz_ok("America/Argentina/Buenos_Aires"));
        assert!(!tz_ok("") && !tz_ok("a/b/c/d") && !tz_ok("Asia/") && tz_ok(&format!("a/{}", "b".repeat(30))) && !tz_ok(&format!("a/{}", "b".repeat(31))));
        assert!(region_ok("CN") && !region_ok("cn") && !region_ok("CHN") && !region_ok("C1"));
    }
}
