//! berryassemble 四刀第五片·收口：`--check` 全量臂组合面（Python
//! `_validate_all` + `do_check` 全量 OK 行镜像——:985/:1009）。
//!
//! 组合律 = 结构 ⊕ 产物 ⊕ 基座 ⊕ 锚：`check_full`（load_product(lite=False)
//! = 结构面 + 文件面四臂）→ `resolve_entries`（基座表对账）→
//! `measure_kernel_anchor`（kernel 锚现算三态）——三面均为前四刀已钉板
//! 的单解释器件，本片零新语义、纯聚合 + OK 行单源。
//!
//! 拒面文案 = Python 现役件逐字镜像（tests/validate_parity.rs oracle
//! 真跑对拍钉板——OK 行六段〔size/split/source/locale/trust/kernel〕
//! 逐字全等 + kernel 三态回显 + 组合序拒面）。

use crate::anchor::measure_kernel_anchor;
use crate::full::check_full;
use crate::lite::{LiteProduct, APP_SLOTS};
use crate::resolve::resolve_entries;
use crate::Reject;

/// 全量面组合入口（Python `_validate_all` :985 镜像——两段式第二段：
/// 结构（load）⊕ 产物（基座对账 + kernel 锚现算对账——件缺不拒、注记））。
/// 返回（过检视图, 排序条目, 基座非 app 条目数）——items/n_base 供后续
/// dry-run 计划面消费（`--dry-run` 仍 Python 单源，本件先备好组合面）。
pub fn validate_all(
    text: &str,
    src_dir: &str,
    base_manifest: &str,
    user_dir: &str,
) -> Result<(LiteProduct, Vec<String>, usize), Reject> {
    let mut p = check_full(text, src_dir)?;
    let (items, n_base) = resolve_entries(&p, src_dir, base_manifest, user_dir)?;
    // G7：件缺 = false 不拒（OK 行注记「仅形」）；件在不等 = loud 拒
    measure_kernel_anchor(&mut p, src_dir)?;
    Ok((p, items, n_base))
}

/// 全量 `--check` OK 行（Python `do_check` 非 lite 臂 :1012-1019 逐字
/// 镜像——六段：preset 计数/size·split/source/locale/trust 开关位 +
/// kernel 三态回显「对账过｜仅形（包内件缺）」）。
pub fn format_check_ok(path: &str, p: &LiteProduct) -> String {
    format!(
        "berry-assemble: {path} OK（product={} board={} preset={}/{} \
override={} size={}MiB split={}MiB source={} locale={} trust={} kernel={}）",
        p.name,
        p.board,
        p.preset.len(),
        APP_SLOTS,
        p.apps_override.len(),
        p.image_size_mib,
        p.rootfs_split_mib,
        on_off(!p.source_instance.is_empty()),
        on_off(p.locale_on()),
        on_off(!p.trust_pk_anchor.is_empty()),
        if p.kernel_measured { "对账过" } else { "仅形（包内件缺）" },
    )
}

/// Python `{'on' if x else 'off'}` 三目镜像（OK 行开关位单源）。
fn on_off(x: bool) -> &'static str {
    if x { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prod() -> LiteProduct {
        LiteProduct {
            name: "p".into(),
            board: "qemu-virt".into(),
            preset: vec!["app/A.bap".into()],
            image_size_mib: 96,
            rootfs_split_mib: 32,
            source_instance: "srcinst".into(),
            trust_pk_anchor: "trust/anchor.pk".into(),
            kernel_measured: true,
            ..Default::default()
        }
    }

    #[test]
    fn check_ok_line_law() {
        // 黄金形：六段全席 + 段间空格逐字（Python f 串拼接段内空格律）
        let line = format_check_ok("pkg/product.toml", &prod());
        assert_eq!(
            line,
            "berry-assemble: pkg/product.toml OK（product=p board=qemu-virt preset=1/10 override=0 size=96MiB split=32MiB source=on locale=off trust=on kernel=对账过）"
        );
        // kernel 三态回显 + 开关位 off 族
        let mut p = prod();
        p.kernel_measured = false;
        p.source_instance.clear();
        p.trust_pk_anchor.clear();
        let line = format_check_ok("x.toml", &p);
        assert!(line.ends_with("source=off locale=off trust=off kernel=仅形（包内件缺））"));
    }

    #[test]
    fn validate_all_composite_order() {
        // 组合序钉板：结构拒先于基座拒先于锚拒（Python _validate_all 同序）
        let tmp = std::env::temp_dir().join(format!("basm-va-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("pkg")).unwrap();
        let base = "version = 1\n\n[entries]\n\"bin/berry-launcher\" = \"cargo-target/release/berry-launcher\"\n";
        std::fs::write(tmp.join("base.toml"), base).unwrap();
        let user = tmp.join("user");
        std::fs::create_dir_all(user.join("cargo-target/release")).unwrap();
        std::fs::write(user.join("cargo-target/release/berry-launcher"), b"x").unwrap();
        let good = format!(
            "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/berry-launcher\"\n[platform]\narch = \"x86_64\"\nfeature_level = \"rich\"\nbuild_type = \"eng\"\nsideload = false\n[platform.boot]\nkernel = \"K.ELF@sha256:{}\"\n[platform.storage]\nimage_size_mib = 96\nrootfs_split_mib = 32\nmin_power_atomicity = \"p0\"\n",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let pkg = tmp.join("pkg");
        // ① 结构拒前置（坏 schema）
        let bad = good.replacen("schema = 1", "schema = 2", 1);
        let e = validate_all(&bad, &pkg.to_string_lossy(), &tmp.join("base.toml").to_string_lossy(), &user.to_string_lossy()).unwrap_err();
        assert!(e.0.starts_with("schema:"));
        // ② 基座拒中置（基座表缺席——先于 kernel 锚面）
        let e = validate_all(&good, &pkg.to_string_lossy(), &tmp.join("nope.toml").to_string_lossy(), &user.to_string_lossy()).unwrap_err();
        assert!(e.0.starts_with("基座条目表加载失败："));
        // ③ 全过 → kernel 件缺 = 仅形不拒（kernel_measured=false）
        let (p, items, n_base) = validate_all(&good, &pkg.to_string_lossy(), &tmp.join("base.toml").to_string_lossy(), &user.to_string_lossy()).unwrap();
        assert!(!p.kernel_measured);
        assert_eq!(n_base, 1);
        assert_eq!(items, vec!["bin/berry-launcher=cargo-target/release/berry-launcher".to_string()]);
        // ④ 放件后现算对账过（空件 sha = e3b0c442… 与声明锚等）
        std::fs::write(pkg.join("K.ELF"), b"").unwrap();
        let (p, _, _) = validate_all(&good, &pkg.to_string_lossy(), &tmp.join("base.toml").to_string_lossy(), &user.to_string_lossy()).unwrap();
        assert!(p.kernel_measured);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
