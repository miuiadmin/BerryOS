//! berryassemble 二刀第二段第一片：`load_product(lite=True)` 结构面镜像——
//! 顶层键两档硬拒 + `schema = 1` + `[product]` 全节（name/vendor/board/
//! apps.{preset,override,priv_grants}/branding/source/trust/defaults.launcher）
//! 键集/类型/值域执法（G4 两段式第一段——零文件对账，lite 语义）。
//!
//! 未收录面（后续片）：`[product.defaults.locale]` 八键（BCP 47/tz/枚举）与
//! `[platform]` 全节执法——locale 节出现即 loud 拒（未收录面不静默放行）；
//! platform 只查在场性。本片未接 CLI（产线零触碰——首刀「只挂测试」先例）。
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

/// Python list repr：`['a', 'b']`。
fn py_list(items: &[String]) -> String {
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
fn app_id_of(entry: &str) -> &str {
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

// ---- load_lite 主面 ----

/// `load_product(lite=True)` 的过检视图（结构面——本片收录 [product] 全节）。
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
    // platform 在场性（Python _need_table 位）；全节执法归后续片
    need_table(&root, "platform")?;
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
    if dv.contains_key("locale") {
        return Err(Reject(
            "product.defaults.locale: 未收录面（Rust lite 二刀第三段——locale 八键执法归后续片；本片 loud 拒，不静默放行）"
                .to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD_MIN: &str = concat!(
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
    );

    #[test]
    fn lite_product_law() {
        let p = load_lite_product(GOOD_MIN).unwrap();
        assert_eq!(p.name, "p");
        assert_eq!(p.vendor, "o");
        assert_eq!(p.board, "qemu-virt");
        assert_eq!(p.preset, vec!["app/A.bap".to_string()]);
        assert_eq!(p.launcher, "bin/sh");
        // app_id 剥离律（Python e[4:-4] 盲剥镜像）：.bap 后缀非必须
        assert_eq!(app_id_of("app/SETTINGS.bap"), "SETTINGS");
        assert_eq!(app_id_of("app/X"), "");
        // 拒面抽查（全谱归 parity oracle 对拍）
        assert!(load_lite_product(&GOOD_MIN.replacen("board = \"qemu-virt\"", "board = \"rpi4\"", 1)).is_err());
        assert!(load_lite_product("schema = 2\n").is_err()); // 须 = 1 先于缺节
        assert!(load_lite_product("schema = 1\n[platform]\narch = \"x86_64\"\n").is_err()); // 缺 [product]
    }
}
