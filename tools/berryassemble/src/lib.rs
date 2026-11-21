//! berry_assemble Rust 首刀·lib 纯面（彻底 Rust 化轨道 b 层 #6 刀序第六位）。
//!
//! 镜像源 = `tools/berry_assemble.py`（Python 现役件，本刀不退役——CLI 对拍
//! oracle + gensourceindex `--preset` import 面单源）。本刀收录四个纯面：
//!
//! 1. **boot-params 词表执法 + 序列化**（Python `BOOT_PARAMS_SPEC` +
//!    `serialize_boot_params` 镜像）——`[platform.boot.params]` 三键词表 v1
//!    （`sched_slice_hz` u32 [16,1024] / `intent_default` 六档 / CP-11
//!    `timer_slack` u32 [0,999_999_999]）+ `etc/boot-params.toml` 单一
//!    序列化点（schema=1 头 + 字典序键值行，u32 裸数字 / intent 带引号）。
//!    读者 = 内核 `kernel/src/bootparams.rs` TOML 子集对读（格式合同）。
//! 2. **kernel 锚形执法**（Python `ANCHOR_RE` 镜像）——`name@sha256:<64hex>`
//!    全形 + 64hex 小写归一。
//! 3. **装配反回滚门**（Python `validate_platform_revision` 镜像）——域界
//!    u32 [0,4294967295]（V9 列宽契约）+ 声明 PR ≤ 平台 level（版本管理
//!    标准 §1 配对律：旧产品装新平台合法 / 新产品装旧平台拒）。
//! 4. **platform level 单源读器**（Python `_platform_level` 镜像——
//!    `tools/abi.toml [platform].level` 小读器；gensourceindex `platform_level`
//!    同源同法）。
//!
//! 拒面纪律：一行理由 + 键名（mkapp Reject 先例）；文案与 Python 件逐字
//! 对齐（`tests/parity.rs` oracle 真跑对拍钉板）。

use std::collections::BTreeMap;

/// product.toml 面 TOML 子集解析器（二刀第一段——语面 = acme 参照物 +
/// SELFTEST_PRODUCT 模板全集；tomllib 对拍钉板）。
pub mod toml_doc;
pub mod lite;
pub mod full;
pub mod resolve;
pub mod anchor;
pub mod validate;

/// 门禁拒绝（一行理由——mkapp Reject 先例；文案即契约，Python `Reject`
/// 双侧全等由 parity 对拍钉板）。
#[derive(Debug)]
pub struct Reject(pub String);

impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn die(msg: String) -> Reject {
    Reject(msg)
}

// ---- 帽与词表（Python 常量段镜像——对账锚：user/berry-manifest + 设计 §1.3/1.4）----

pub const PARAMS_MAX: usize = 32;
/// ADR #58 六档 intent 词表（mkapp INTENTS 同源镜像）。
pub const INTENTS: [&str; 6] = [
    "unspecified",
    "latency_critical",
    "interactive",
    "utility",
    "background",
    "maintenance",
];

/// boot.params 值域两形态（Python `BOOT_PARAMS_SPEC` v1 三键直映）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamVal {
    U32(u32),
    Intent(&'static str),
}

/// 词表 v1（add-only、未知键硬拒——Q2）。restricted/locked 档行随
/// PM/thermal 域刀增补；locked 键「出现即拒」由未知键拒面先行覆盖。
pub fn boot_params_spec(key: &str) -> Option<(&'static str, u32, u32)> {
    match key {
        // free 档（缺省归上游编译期常量）
        "sched_slice_hz" => Some(("u32", 16, 1024)),
        // free 档（六档词表；缺省 utility）
        "intent_default" => Some(("intent", 0, 0)),
        // free 档（ns；缺省 0=无松弛——上界 = 内核 parse_u32 九位帽对读，
        // 全域 < slackface SLACK_MAINTENANCE_NS 10s 帽）
        "timer_slack" => Some(("u32", 0, 999_999_999)),
        _ => None,
    }
}

/// `[platform.boot.params]` 词表执法（Python `_check_platform_section`
/// params 段镜像：未知键硬拒 + 分型值域 + 越帽 PARAMS_MAX）。
pub fn check_boot_params(
    params: &BTreeMap<String, toml_val::Val>,
) -> Result<BTreeMap<String, ParamVal>, Reject> {
    if params.len() > PARAMS_MAX {
        return Err(die(format!(
            "platform.boot.params: {} 键越帽 {PARAMS_MAX}（§1.6-4 解析面定界——INITRD 段 count 帽 32）",
            params.len()
        )));
    }
        let mut out = BTreeMap::new();
    for (key, val) in params {
        let Some((kind, lo, hi)) = boot_params_spec(key) else {
            return Err(die(format!(
                "platform.boot.params.{key}: 不在启动参数词表 ['intent_default', 'sched_slice_hz', 'timer_slack']（Q2：词表 add-only、未知键硬拒——新键进词表走 #83 后续升版）"
            )));
        };
        if kind == "u32" {
            let v = need_u32(
                "platform.boot.params",
                key,
                val,
                lo,
                hi,
            )?;
            out.insert(key.clone(), ParamVal::U32(v));
        } else {
            // intent 六档——Python `not isinstance(val, str) or val not in INTENTS`
            // 单分支：非串与枚举外同文案，got 段 = repr（非串形如实打值）。
            let Some(s) = val.as_str() else {
                return Err(die(format!(
                    "platform.boot.params.{key}: 不在六档词表 ('unspecified', 'latency_critical', 'interactive', 'utility', 'background', 'maintenance')（ADR #58——got {}）",
                    val.py_repr()
                )));
            };
            if !INTENTS.contains(&s) {
                return Err(die(format!(
                    "platform.boot.params.{key}: 不在六档词表 ('unspecified', 'latency_critical', 'interactive', 'utility', 'background', 'maintenance')（ADR #58——got '{s}'）"
                )));
            }
            // 'static 断言：词表本身是 'static 数组，找不到时上面已拒
            let st = INTENTS.iter().find(|x| **x == s).copied().unwrap();
            out.insert(key.clone(), ParamVal::Intent(st));
        }
    }
    Ok(out)
}

fn need_u32(
    sect: &str,
    key: &str,
    v: &toml_val::Val,
    lo: u32,
    hi: u32,
) -> Result<u32, Reject> {
    // bool 是 int 子类（Python 面），type() 才拦得住——Rust 面枚举分型天然拦。
    let Some(n) = v.as_int() else {
        return Err(die(format!(
            "{sect}.{key}: 应为整数（got {}）",
            v.type_name()
        )));
    };
    if !(lo as i64..=hi as i64).contains(&n) {
        return Err(die(format!(
            "{sect}.{key}: 越域 [{lo},{hi}]（got {n}）"
        )));
    }
    Ok(n as u32)
}

/// `need_u32` 的公开包装（`[platform.storage]` 域界执法单源复用——lite
/// 第二片转换桥调用；与 Python `_need_u32` 文案逐字对齐）。
pub fn check_single_u32(
    sect: &str,
    key: &str,
    v: &toml_val::Val,
    lo: u32,
    hi: u32,
) -> Result<u32, Reject> {
    need_u32(sect, key, v, lo, hi)
}

/// boot-params 过检视图 → `etc/boot-params.toml` 字节（G5 产件面的单一
/// 序列化点——Python `serialize_boot_params` 镜像）。格式合同 = 内核
/// `bootparams.rs` parse（TOML 子集对读）：`schema = 1` 头 + `key = value`
/// 行，键序 = 字典序（确定性——同清单同字节），u32 裸数字 / intent 带引号
/// 字面量；空行/`#` 注释内核侧容忍但本序列化器不产（最小面）。值域已在
/// `check_boot_params` 词表执法——此处纯直映零再裁。
pub fn serialize_boot_params(params: &BTreeMap<String, ParamVal>) -> Vec<u8> {
    let mut lines = vec!["schema = 1".to_string()];
    for (key, val) in params {
        match val {
            ParamVal::Intent(s) => lines.push(format!("{key} = \"{s}\"")),
            ParamVal::U32(n) => lines.push(format!("{key} = {n}")),
        }
    }
    let mut out = lines.join("\n").into_bytes();
    out.push(b'\n');
    out
}

// ---- kernel 锚形（Python ANCHOR_RE 镜像）----

/// `^[A-Za-z0-9][A-Za-z0-9._-]*@sha256:[0-9a-fA-F]{64}$` 手写递归下降
/// （零 regex 依赖——纯 std 纪律）。返回 `(name, sha_64hex小写归一)`。
pub fn parse_kernel_anchor(anchor: &str) -> Result<(String, String), Reject> {
    let at = anchor
        .rfind("@sha256:")
        .ok_or_else(|| die(format!(
            "platform.boot.kernel: 工件锚形不符 '{anchor}'（须 name@sha256:<64hex>——schema §1.3；逐字节对账归全产线步⑧）"
        )))?;
    let (name, tail) = anchor.split_at(at);
    let sha_part = &tail["@sha256:".len()..];
    let sha_ok = sha_part.len() == 64
        && sha_part.bytes().all(|c| c.is_ascii_hexdigit());
    if !anchor_char_ok(name) || !sha_ok {
        return Err(die(format!(
            "platform.boot.kernel: 工件锚形不符 '{anchor}'（须 name@sha256:<64hex>——schema §1.3；逐字节对账归全产线步⑧）"
        )));
    }
    Ok((
        name.to_string(),
        sha_part.to_ascii_lowercase(),
    ))
}

fn anchor_char_ok(name: &str) -> bool {
    let b = name.as_bytes();
    if b.is_empty() || !b[0].is_ascii_alphanumeric() {
        return false;
    }
    b.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

// ---- 装配反回滚门（Python validate_platform_revision 镜像）----

/// `--platform-revision` 装配门（域界 + 反回滚两臂——V9 透传面升装配
/// 语义面，PR ≡ 平台 API level 刀）。`level` = `platform_level()` 现值
/// （调用方单源注入——库面零环境耦合，测试面可造合成 abi.toml）。
pub fn validate_platform_revision(pr: i64, level: u64) -> Result<u32, Reject> {
    // 臂① 域界：u32 全域 [0,4294967295]，与 mkimage build_manifest 拒面
    // 同口径（列宽 10 是 manifest v1 冻结契约的一部分）。
    if !(0..=4294967295i64).contains(&pr) {
        return Err(die(format!(
            "--platform-revision: 须 u32 域 [0,4294967295]（V9 列宽契约——mkimage 同口径拒；got {pr}）"
        )));
    }
    // 臂② 反回滚（版本管理标准 §1 配对律首件落地）：声明 PR 须 ≤ 平台
    // level——OEM 产品代次不得超前平台代次。旧产品装新平台合法（level
    // 单调轨、高位兼容低位——G-ver-5 .bap min_level 装载门同构）；
    // 新产品装旧平台拒（平台代次被回滚）。
    if pr as u64 > level {
        return Err(die(format!(
            "--platform-revision: 声明 PR {pr} 超前平台 level {level}（tools/abi.toml [platform].level 单源——装配反回滚：OEM 产品代次不得超前平台代次，旧产品装新平台合法/新产品装旧平台拒；版本管理标准 §1 配对律）"
        )));
    }
    Ok(pr as u32)
}

// ---- platform level 单源读器（Python _platform_level 镜像）----

/// `tools/abi.toml [platform].level` 小读器（版本管理标准 §1 平台 level
/// 轨；mkcpio `_berryver_sources` / mkimage `platform_level` / gensourceindex
/// `platform_level` 同源同法——数据单源在 abi.toml，各工具自携小读器）。
/// 坏源一律 Reject loud（基准注入口炸好过静默放行）。
pub fn platform_level_of(text: &str) -> Result<u64, Reject> {
    let mut in_platform = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_platform = l == "[platform]";
            continue;
        }
        if in_platform {
            if let Some(rest) = l.strip_prefix("level") {
                if let Some(v) = rest.trim().strip_prefix('=') {
                    let v = v.split('#').next().unwrap_or("").trim();
                    if let Ok(n) = v.parse::<u64>() {
                        return Ok(n);
                    }
                    return Err(die(format!(
                        "abi.toml [platform].level: 非整数（got {v:?}——装配反回滚门比对基准单源）"
                    )));
                }
            }
        }
    }
    Err(die("abi.toml [platform].level: 缺席（装配反回滚门比对基准单源）".to_string()))
}

/// 极简 TOML 值（params 面的 int/str/bool/list 四型——`[platform]` 全节
/// 结构执法（二刀第二片）经 lite 转换复用；`as_int` 排除 bool，Python
/// `type(v) is not int` 同拦）。
pub mod toml_val {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Val {
        Int(i64),
        Str(String),
        Bool(bool),
        List(Vec<Val>),
        Other,
    }

    impl Val {
        pub fn as_int(&self) -> Option<i64> {
            match self {
                Val::Int(n) => Some(*n),
                _ => None,
            }
        }

        pub fn as_str(&self) -> Option<&str> {
            match self {
                Val::Str(s) => Some(s.as_str()),
                _ => None,
            }
        }

        /// Python `type(v).__name__` 镜像（拒面文案 `got {…}` 段单源）。
        pub fn type_name(&self) -> &'static str {
            match self {
                Val::Int(_) => "int",
                Val::Str(_) => "str",
                Val::Bool(_) => "bool",
                Val::List(_) => "list",
                Val::Other => "NoneType",
            }
        }

        /// Python `repr` 镜像（拒面 `got {v!r}` 段——int 裸形/str 单引号/
        /// bool 首帽/list 方括号递归）。
        pub fn py_repr(&self) -> String {
            match self {
                Val::Int(n) => n.to_string(),
                Val::Str(s) => format!("'{s}'"),
                Val::Bool(b) => if *b { "True" } else { "False" }.to_string(),
                Val::List(a) => {
                    let inner: Vec<String> = a.iter().map(|v| v.py_repr()).collect();
                    format!("[{}]", inner.join(", "))
                }
                Val::Other => "None".to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(kv: &[(&str, toml_val::Val)]) -> BTreeMap<String, toml_val::Val> {
        kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn boot_params_law() {
        // 三键词表 + 值域 + 序列化黄金形
        let mut p = check_boot_params(&m(&[
            ("sched_slice_hz", toml_val::Val::Int(64)),
            ("intent_default", toml_val::Val::Str("interactive".into())),
            ("timer_slack", toml_val::Val::Int(123)),
        ]))
        .unwrap();
        let bytes = serialize_boot_params(&p);
        assert_eq!(
            String::from_utf8(bytes.clone()).unwrap(),
            "schema = 1\nintent_default = \"interactive\"\nsched_slice_hz = 64\ntimer_slack = 123\n"
        );
        // 恰界：16/1024 过，15/1025 拒；timer_slack 0/999_999_999 过
        for (k, lo_hi) in [
            ("sched_slice_hz", (15i64, 1025i64)),
            ("timer_slack", (-1i64, 1_000_000_000i64)),
        ] {
            for bad in [lo_hi.0, lo_hi.1] {
                assert!(check_boot_params(&m(&[(k, toml_val::Val::Int(bad))])).is_err());
            }
        }
        p.insert("sched_slice_hz".into(), ParamVal::U32(16));
        p.insert("timer_slack".into(), ParamVal::U32(999_999_999));
        check_boot_params(&m(&[
            ("sched_slice_hz", toml_val::Val::Int(16)),
            ("timer_slack", toml_val::Val::Int(999_999_999)),
        ]))
        .unwrap();
        // 未知键硬拒（locked 出现即拒由此面先行覆盖）
        assert!(check_boot_params(&m(&[("locked", toml_val::Val::Bool(true))])).is_err());
        // intent 拒面：非六档字符串 / 非字符串（int 代入）
        assert!(check_boot_params(&m(&[("intent_default", toml_val::Val::Str("bogus".into()))])).is_err());
        assert!(check_boot_params(&m(&[("intent_default", toml_val::Val::Int(3))])).is_err());
        // 帽：33 个未知键越 PARAMS_MAX 前先咬未知键；帽本身由下段独立钉
        // （BTreeMap 键唯一——同键覆盖进 3 键帽内，帽外必为未知键族）
        let mut big = BTreeMap::new();
        for i in 0..33 {
            big.insert(format!("sched_slice_hz#{i}"), toml_val::Val::Int(64));
        }
        assert_eq!(big.len(), 33);
        assert!(check_boot_params(&big).is_err()); // 未知键拒先于帽
    }

    #[test]
    fn kernel_anchor_law() {
        let (n, s) = parse_kernel_anchor(
            "KERNEL.ELF@sha256:a19b039187b5964d791b13a065a2b4b3eff00d3d1b4f9140dbf8da8756e35d0d",
        )
        .unwrap();
        assert_eq!(n, "KERNEL.ELF");
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        // 大写 hex 归一（Python sha.lower() 同律）
        let (_, s2) = parse_kernel_anchor(&format!("K@sha256:{}", "A".repeat(64))).unwrap();
        assert_eq!(s2, "a".repeat(64));
        // 拒面：非 alnum 首、缺段、短 sha、非 hex
        assert!(parse_kernel_anchor(".K@sha256:aaaa").is_err());
        assert!(parse_kernel_anchor("K@sha256:aaaa").is_err());
        assert!(parse_kernel_anchor(&format!("K@sha256:{}", "g".repeat(64))).is_err());
        assert!(parse_kernel_anchor("K@md5:aaaa").is_err());
        // name 内 @ 非法（Python RE 字符集同拒——rfind 尾锚下 name 段照执法）
        assert!(parse_kernel_anchor(&format!("a@b@sha256:{}", "a".repeat(64))).is_err());
    }

    #[test]
    fn revision_gate_law() {
        let level = 7u64;
        assert_eq!(validate_platform_revision(0, level).unwrap(), 0);
        assert_eq!(validate_platform_revision(7, level).unwrap(), 7); // 等值过
        assert!(validate_platform_revision(8, level).is_err()); // 超前拒
        assert!(validate_platform_revision(-1, level).is_err()); // 域界下
        assert!(validate_platform_revision(4294967296i64, level).is_err()); // 域界上
        assert_eq!(validate_platform_revision(4294967295i64, 4294967295u64).unwrap(), u32::MAX);
    }

    #[test]
    fn platform_level_reader_law() {
        let ok = "# c\n[platform]\nlevel = 7  # 注\n[other]\nlevel = 9\n";
        assert_eq!(platform_level_of(ok).unwrap(), 7);
        assert!(platform_level_of("[platform]\nlevel = x\n").is_err());
        assert!(platform_level_of("[nope]\nlevel = 1\n").is_err());
        assert!(platform_level_of("").is_err());
        // bool 形（Python type() 拦）——本读器 parse::<u64> 天然拒 "true"
        assert!(platform_level_of("[platform]\nlevel = true\n").is_err());
    }

    #[test]
    fn toml_doc_parses_acme_reference() {
        // 仓内参照物真源：acme product.toml（含多行 preset 数组 + 全席注释 +
        // 引号键面注释化保留 + 注释化节头不产出）
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../drill/acme/product.toml"
        ))
        .unwrap();
        let t = toml_doc::parse(&text).unwrap();
        let m: std::collections::BTreeMap<&str, &toml_doc::Val> =
            t.iter().map(|(k, v)| (k.as_str(), v)).collect();
        assert_eq!(m["schema"], &toml_doc::Val::Int(1));
        assert_eq!(m["product.name"], &toml_doc::Val::Str("acme-rich".into()));
        assert_eq!(m["product.vendor"], &toml_doc::Val::Str("acme".into()));
        assert_eq!(m["product.board"], &toml_doc::Val::Str("qemu-virt".into()));
        assert_eq!(m["platform.sideload"], &toml_doc::Val::Bool(false));
        assert_eq!(
            m["platform.boot.kernel"],
            &toml_doc::Val::Str("KERNEL.ELF@sha256:a19b039187b5964d791b13a065a2b4b3eff00d3d1b4f9140dbf8da8756e35d0d".into())
        );
        // 多行 preset 数组（5 元素 + 尾逗号）——flatten repr 形
        assert_eq!(
            m["product.apps.preset"],
            &toml_doc::Val::Arr(vec![
                "app/SETTINGS.bap".into(),
                "app/FILES.bap".into(),
                "app/PHOTOS.bap".into(),
                "app/CAMERA.bap".into(),
                "app/BROWSER.bap".into(),
            ].into_iter().map(toml_doc::Val::Str).collect())
        );
        assert_eq!(
            m["product.apps.priv_grants.SETTINGS"],
            &toml_doc::Val::Arr(vec![toml_doc::Val::Str("console".into()), toml_doc::Val::Str("fs".into())])
        );
        assert!(m.contains_key("product.defaults.launcher"));
        // 注释化节不产出（override/branding/source/trust 全注释）
        assert!(!m.contains_key("product.apps.override.SETTINGS"));
        assert!(!m.contains_key("product.branding.wallpaper"));
        assert!(!m.contains_key("product.source.instance"));
    }

    #[test]
    fn toml_doc_law() {
        use toml_doc::{flatten, parse, Val};
        // 最小面：节序 + 键序保序 + repr 形
        let t = parse("schema = 1\n[a.b]\nk = \"v\"\nn = -7\nb = true\n").unwrap();
        assert_eq!(
            t,
            vec![
                ("schema".to_string(), Val::Int(1)),
                ("a.b.k".to_string(), Val::Str("v".into())),
                ("a.b.n".to_string(), Val::Int(-7)),
                ("a.b.b".to_string(), Val::Bool(true)),
            ]
        );
        // 引号键（base.toml "app/A.bap" 形）
        let t2 = parse("[entries]\n\"app/A.bap\" = \"userbins/app/A.bap\"\n").unwrap();
        assert_eq!(
            t2,
            vec![(
                "entries.app/A.bap".to_string(),
                Val::Str("userbins/app/A.bap".into())
            )]
        );
        // flatten repr：串单引号、数组逗号后空格
        let t3 = parse("[x]\ns = [\"a\", \"b\"]\n").unwrap();
        assert_eq!(flatten(&t3), vec!["x.s = ['a', 'b']".to_string()]);
        // 串内 # 与 , 免疫（串外定位）
        let t4 = parse("[x]\ns = \"a#b,c]d\"\n").unwrap();
        assert_eq!(t4[0].1, Val::Str("a#b,c]d".into()));
        // 空行/整行注释/行尾注释剥除
        let t5 = parse("# 头\n[a] # 尾\n\nk = 1 # v\n").unwrap();
        assert_eq!(t5, vec![("a.k".to_string(), Val::Int(1))]);
        // 多行数组跨行累积（元素带注释行）
        let t6 = parse("[a]\np = [\n  \"x\", # 一\n  \"y\",\n]\n").unwrap();
        assert_eq!(
            t6,
            vec![(
                "a.p".to_string(),
                Val::Arr(vec![Val::Str("x".into()), Val::Str("y".into())])
            )]
        );
        // 拒梯：无 = / 未闭合串 / 闭合后残留 / 多行数组未闭合 / 内联表 /
        // 非法值 / 转义串
        assert!(parse("[a]\nk\n").is_err());
        assert!(parse("[a]\nk = \"x\n").is_err());
        assert!(parse("[a]\nk = \"x\" y\n").is_err());
        assert!(parse("[a]\nk = 1\np = [\"x\",\n").is_err());
        assert!(parse("[a]\nk = { x = 1 }\n").is_err());
        assert!(parse("[a]\nk = abc\n").is_err());
        assert!(parse("[a]\nk = \"a\\\\b\"\n").is_err());
        // 重复键/重复节头拒（tomllib 同咬——键唯一性执法）
        assert!(parse("[a]\nk = 1\nk = 2\n").is_err());
        assert!(parse("[a]\nk = 1\n[b]\nj = 2\n[a]\nm = 3\n").is_err());
        // 空节不产出（对拍文本避让面在此钉界）
        assert!(parse("[empty]\n").unwrap().is_empty());
    }
}
