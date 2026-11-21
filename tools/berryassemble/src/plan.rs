//! berryassemble 六刀：`--dry-run` 计划面（Python `build_plan` :890
//! + `do_dry_run` :1020 逐字镜像——dry-run 组装计划行）。
//!
//! 层界：五刀 `--check` 全量臂组合面之后的计划面段。`do_dry_run` =
//! `validate_platform_revision`（装配门两臂——首刀 lib 已钉板）⊕
//! `platform_level_of`（abi.toml 单源读器——首刀 lib 已钉板）⊕
//! `validate_all`（五刀组合面）⊕ `build_plan` 行面（本片主体，零新
//! 语义纯行拼装）。无时刻输入——确定性（同清单同 pr 同基座同字节）。
//!
//! 行面文案 = Python 现役件逐字镜像（tests/dryrun_parity.rs oracle
//! 真跑对拍钉板——stdout 逐行全等剥路径段）。`user_disp` =
//! `os.path.relpath(user_dir)`（CWD 相对形）由调用方（main.rs）传入。

use crate::full::{py_repr, TRUST_PK_LEN};
use crate::lite::LiteProduct;
use crate::resolve::TRUST_PK_ENTRY;


/// `etc/boot-params.toml` 归档名（Python `BOOT_PARAMS_ENTRY` 同值）。
pub const BOOT_PARAMS_ENTRY: &str = "etc/boot-params.toml";

/// dry-run 组装计划行（Python `build_plan` :890-982 逐字镜像）。
/// `user_disp` = user 基准的显示形（Python `os.path.relpath(user_dir)`
/// ——CWD 相对；main.rs 同法现算注入）。pr = 过装配门的产品声明代次；
/// level = tools/abi.toml 平台 level（门基准回显——mkimage 侧 PR 即
/// 此值，单源无注入位）。
pub fn build_plan(
    p: &LiteProduct,
    items: &[String],
    n_base: usize,
    pr: u32,
    level: u64,
    user_disp: &str,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    // G1 替换集（Python ovr_names dict 形——键 = app/{id}.bap 条目名形，
    // 值 id；Rust 面只需键集与计数）
    let ovr_names: std::collections::BTreeSet<String> = p
        .apps_override
        .keys()
        .map(|k| format!("app/{k}.bap"))
        .collect();
    let n_ov = items
        .iter()
        .filter(|it| {
            let name = it.split('=').next().unwrap_or("");
            ovr_names.iter().any(|k| k == name)
        })
        .count();
    let n_tr = items
        .iter()
        .filter(|it| it.starts_with(&format!("{TRUST_PK_ENTRY}=")))
        .count();
    let mut seg = format!(
        "berry-assemble: rootfs 条目 {} 条 = 基座 {}（bin/*+selftest/*）⊕ preset {}（app/*）",
        items.len(),
        n_base,
        p.preset.len()
    );
    if n_ov > 0 {
        seg += &format!("，其中 {n_ov} 件 override 替换（路径 = OEM 自有件——G1）");
    }
    if n_tr > 0 {
        seg += &format!(" ⊕ trust {n_tr}（etc/trust-pk——G2）");
    }
    lines.push(format!(
        "berry-assemble: 产品 {}（vendor={} board={} arch={} feature_level={} build_type={} sideload={}）",
        p.name, p.vendor, p.board, p.arch, p.feature_level, p.build_type,
        py_bool(p.sideload)
    ));
    lines.push(seg + "——排序 = 整条 name=path（mkcpio CLI 现役语义，步④）");
    for item in items {
        let (name, path) = match item.split_once('=') {
            Some((n, r)) => (n, r),
            None => (item.as_str(), ""),
        };
        if path.starts_with('/') {
            // override/trust：包内已解析绝对路径（load 面验在）
            let tag = if ovr_names.iter().any(|k| k == name) {
                "override（OEM 自有件）"
            } else {
                "trust 锚"
            };
            lines.push(format!("  {name} = {path}  ← {tag}"));
        } else {
            lines.push(format!("  {name} = {user_disp}/{path}"));
        }
    }
    if !p.apps_override.is_empty() {
        let keys: Vec<String> = p
            .apps_override
            .keys()
            .map(|k| format!("'{k}'"))
            .collect();
        lines.push(format!(
            "berry-assemble: apps.override {} 件 [{}]（G1：限替换 preset 已声明 app——推断键位，正式定形随 #83；.sig 伴生随新路径同查找 = mkcpio sig_companions 同名直映规则，本工具零第二映射）",
            p.apps_override.len(),
            keys.join(", ")
        ));
    }
    if !p.trust_pk_anchor.is_empty() {
        lines.push(format!(
            "berry-assemble: trust.pk_anchor → {TRUST_PK_ENTRY} 条目（G2：{} 恰 {TRUST_PK_LEN}B raw——svcmgr p3_sig_guard 装载守卫锚消费面同路径约定；bootseed 正式形随 P3 策略刀，词表外出现即拒）",
            p.trust_pk_anchor
        ));
    }
    lines.push(format!(
        "berry-assemble: mkcpio 参数集（步⑥）: tools/mkcpio/cargo-target/<host>/release/mkcpio <out.cpio> <{} 条 name=path·见上清单> --split {}（单编码器复用，禁第二份 newc 逻辑——产链换轨二刀后现役 = Rust bin）",
        items.len(),
        p.rootfs_split_mib
    ));
    lines.push(format!(
        "berry-assemble: mkimage 参数集（步⑦）: tools/mkimage/cargo-target/<host>/release/mkimage out/{}.img --efi <配方注入位> --hello <配方注入位> --kernel <配方注入位> --initrd <配方注入位> --size-mib {}{}",
        p.name,
        p.image_size_mib,
        if p.source_instance.is_empty() {
            String::new()
        } else {
            format!(" --source-instance {}", p.source_instance)
        }
    ));
    if !p.source_instance.is_empty() {
        lines.push(format!(
            "berry-assemble: 源实例透传（SDK-2 P2 步⑦½）: [product.source].instance = {}（两件套入 FAT /SOURCE/——v1.2 §五镜像内置官方源静态实例；深校验过 gensourceindex）",
            p.source_instance
        ));
    }
    lines.push(format!(
        "berry-assemble:   platform_revision 无注入位（mkimage 单源读 tools/abi.toml [platform].level={}——V9 裁决 PR ≡ level，三方联动 manifest PR == version VMO level == rootfs .berryver level）；装配反回滚门：声明 PR {} ≤ 平台 level {} 过（产品代次不超前平台代次）；--key-hash 缺省 none（QEMU 路不烧，件 5）",
        level, pr, level
    ));
    if p.kernel_measured {
        lines.push(format!(
            "berry-assemble: kernel 锚（步⑧ 前置·G7）: {}@sha256:{}——包内件在，SHA-256 现算对账过",
            p.kernel_name, p.kernel_sha
        ));
    } else {
        lines.push(format!(
            "berry-assemble: kernel 锚（步⑧ 前置·G7）: {}@sha256:{}——包内件缺 = 只形校验过（OEM 可先声明后放件；放件后 --check 即现算对账）",
            p.kernel_name, p.kernel_sha
        ));
    }
    if !p.params.is_empty() {
        // Python f"{v!r}"：params 存储形 = 统一串面（lite.rs 二刀转换桥
        // ——intent 存原串、u32 存十进制串），repr 分型由键词表单源定
        // （boot_params_spec——值型已执法，无歧义形）：u32 裸数字 /
        // intent 单引号。
        let kv: Vec<String> = p
            .params
            .iter()
            .map(|(k, v)| {
                let repr = match crate::boot_params_spec(k).map(|s| s.0) {
                    Some("u32") => v.clone(),
                    _ => py_repr(v),
                };
                format!("{k}={repr}")
            })
            .collect();
        lines.push(format!(
            "berry-assemble: boot-params 条目（步④·G5 投放面最小）: {BOOT_PARAMS_ENTRY} ← [platform.boot.params] {}（格式 = schema=1 头 + 键值对、键序 = 字典序；override 语义——缺省归上游〔§1.6〕；产件面 = --emit 模式（最小产件，非全产线）；读者 = 内核启动序列早期 cpio 查找〔§1.6-补：sched_slice_hz 换算 slice_cycles / intent_default 新任务缺省 intent / timer_slack 系统缺省松弛（CP-11）——消费端 已落〔kernel/src/bootparams.rs〕〕）",
            kv.join(" ")
        ));
    }
    if p.locale_on() {
        // Python f"{k}={v!r}"：locale 八键全串面（lite.rs 逐键存储）。
        let mut kv: Vec<String> = Vec::new();
        for (k, v) in locale_pairs(p) {
            // languages 值 = Python list（repr 形已带 [] 与内层引号，
            // 不再包串面 repr）；余键 = str 串面 repr。
            let disp = if k == "languages" { v } else { py_repr(&v) };
            kv.push(format!("{k}={disp}"));
        }
        lines.push(format!(
            "berry-assemble: locale 条目（步④·投放面最小）: etc/locale.toml ← [product.defaults.locale] {}（生成件归全产线；读者 = 设置应用 v2 转正刀〔应用规范 §15.3〕——完整 i18n 运行时〔berry-intl〕未立项，键组是预埋不是功能承诺，§1.5）",
            kv.join(" ")
        ));
    }
    lines.push(
        "berry-assemble: 计划完——产件面 = --emit（boot-params 最小产件）；镜像全产线与 just 配方 assemble 归 V13 后续刀/主会话收口）".to_string(),
    );
    lines
}

/// Python bool str 形（f"{x}" 镜像——True/False 非 true/false）。
fn py_bool(b: bool) -> &'static str {
    if b { "True" } else { "False" }
}

/// locale 八键非空对（键序 = Python `sorted(prod.locale)` 字典序——
/// 非空键收集后按键排序复现）。
fn locale_pairs(p: &LiteProduct) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    if !p.locale_languages.is_empty() {
        let langs: Vec<String> = p
            .locale_languages
            .iter()
            .map(|s| format!("'{s}'"))
            .collect();
        out.push((
            "languages".to_string(),
            format!("[{}]", langs.join(", ")),
        ));
    }
    let singles = [
        ("timezone", &p.locale_timezone),
        ("region", &p.locale_region),
        ("regulatory_domain", &p.locale_regulatory_domain),
        ("calendar", &p.locale_calendar),
        ("first_day_of_week", &p.locale_first_day_of_week),
        ("hour_cycle", &p.locale_hour_cycle),
        ("temperature_unit", &p.locale_temperature_unit),
    ];
    for (k, v) in singles {
        if !v.is_empty() {
            out.push((k.to_string(), v.clone()));
        }
    }
    // BTreeMap 语义复现：sorted(dict) = 键字典序（八键英文键名字典序）。
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_pairs_sorted_law() {
        let mut p = LiteProduct::default();
        p.locale_timezone = "UTC".into();
        p.locale_languages = vec!["zh-Hans-CN".into(), "en-US".into()];
        let pairs = locale_pairs(&p);
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["languages", "timezone"]);
        // languages = Python list repr 形
        assert_eq!(pairs[0].1, "['zh-Hans-CN', 'en-US']");
    }
}
