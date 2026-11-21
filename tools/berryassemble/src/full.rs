//! berryassemble 四刀第一片：`--check` 产物对账面（G4 两段式第二段的
//! 文件面——berry_assemble.py `load_product(lite=False)` 全量臂镜像）。
//!
//! 层界：本片只做「结构面过检后的文件在席对账」——branding 资产 /
//! override OEM 件 / trust 锚件 32B 恰长 / source 两件套在席 + TLV 深校验。
//! 基座表对账（resolve_entries）与 kernel 锚现算（measure_kernel_anchor）
//! 归后续片（依赖 rootfs-manifest 真源与构建树，CI 环境面待评估）。
//!
//! 拒面文案 = Python 现役件逐字镜像（tests/full_parity.rs oracle 真跑
//! 对拍钉板）。路径解析基准 = product.toml 所在目录（Python `src_dir`
//! 同口径——调用方传入，本层不做 cwd 推断）。

use crate::lite::{load_lite_product, LiteProduct};
use crate::Reject;

/// trust 锚件长度契约（Python TRUST_PK_LEN 同值）。
pub const TRUST_PK_LEN: usize = 32;

/// gensourceindex 两件套名（Python `gsi.INDEX_NAME`/`KEYS_NAME` 同值——
/// 现役源码单源对齐，gensourceindex 升版须同步此处）。
pub const GSI_INDEX_NAME: &str = "source-index.tlv";
pub const GSI_KEYS_NAME: &str = "source-keys.txt";

/// 产物面对账入口：结构面（lite）先过，文件面逐节对账。
/// `src_dir` = product.toml 所在目录（路径解析基准）。
pub fn check_full(text: &str, src_dir: &str) -> Result<LiteProduct, Reject> {
    let p = load_lite_product(text)?;
    check_full_files(&p, src_dir)?;
    Ok(p)
}

/// 产物面文件对账（结构面已过检的 LiteProduct 复用——Python
/// `load_product(lite=False)` 在 load 面内联文件臂的镜像；检查序对齐：
/// preset/override（product 节内序）→ branding → source → trust）。
pub fn check_full_files(p: &LiteProduct, src_dir: &str) -> Result<(), Reject> {
    // [product.apps.override]——OEM 件包内在席（Python `lite=False` 臂：
    // `if not lite and not os.path.isfile(local)` 逐字镜像）
    for (app_id, rel) in &p.apps_override {
        let local = join_path(src_dir, rel);
        if !std::path::Path::new(&local).is_file() {
            return Err(Reject(format!(
                "product.apps.override.{app_id}: 包内不存在 {}（相对 product.toml 所在目录解析——schema §1.4 路径同口径）",
                py_repr(rel)
            )));
        }
    }
    // [product.branding]——品牌资产在席（Python 同臂镜像）
    for (key, rel) in &p.branding {
        let local = join_path(src_dir, rel);
        if !std::path::Path::new(&local).is_file() {
            return Err(Reject(format!(
                "product.branding.{key}: 包内不存在 {}（相对 product.toml 所在目录解析——schema §1.4）",
                py_repr(rel)
            )));
        }
    }
    // [product.source]——两件套在席 + TLV 深校验（Python 惰性 import
    // gensourceindex 复用单解释器；Rust 侧走 path 依赖同一 crate 的
    // parse_index——单源不弃）
    if !p.source_instance.is_empty() {
        let d = join_path(src_dir, &p.source_instance);
        if !std::path::Path::new(&d).is_dir() {
            return Err(Reject(format!(
                "product.source.instance: 目录不存在 {}（相对 product.toml 所在目录解析——schema §1.4 路径同口径）",
                py_repr(&p.source_instance)
            )));
        }
        for name in [GSI_INDEX_NAME, GSI_KEYS_NAME] {
            if !std::path::Path::new(&join_path(&d, name)).is_file() {
                return Err(Reject(format!(
                    "product.source.instance: 缺 {name}（两件套 = gensourceindex 产物——索引 + 源钥清单）"
                )));
            }
        }
        let blob = std::fs::read(join_path(&d, GSI_INDEX_NAME)).map_err(|e| {
            Reject(format!(
                "product.source.instance: 索引解析拒：{e}（TLV 单解释器 = gensourceindex——深校验复用）"
            ))
        })?;
        if let Err(msg) = gensourceindex::parse_index(&blob) {
            return Err(Reject(format!(
                // Python die() 前缀 `gensourceindex: FAIL:` 逐字镜像
                //（SystemExit str 形——CLI 面文案即契约）
                "product.source.instance: 索引解析拒：gensourceindex: FAIL: {}（TLV 单解释器 = gensourceindex——深校验复用）",
                msg.message
            )));
        }
    }
    // [product.trust]——锚件在席 + 32B 恰长
    if !p.trust_pk_anchor.is_empty() {
        let local = join_path(src_dir, &p.trust_pk_anchor);
        if !std::path::Path::new(&local).is_file() {
            return Err(Reject(format!(
                "product.trust.pk_anchor: 包内不存在 {}（相对 product.toml 所在目录解析——schema §1.4 路径同口径）",
                py_repr(&p.trust_pk_anchor)
            )));
        }
        let len = std::fs::metadata(&local).map(|m| m.len() as usize).unwrap_or(usize::MAX);
        if len != TRUST_PK_LEN {
            return Err(Reject(format!(
                "product.trust.pk_anchor: 须恰 {TRUST_PK_LEN}B raw 公钥（got {len}B——etc/trust-pk 装载守卫锚约定，svcmgr p3_sig_guard/mkimage TRUSTPK 同口径）"
            )));
        }
    }
    Ok(())
}

/// Python repr 串形（f"{v!r}" 镜像——单引号；三刀 main.rs py_repr 同源律）。
fn py_repr(s: &str) -> String {
    // Python repr 单引号形：内嵌单引号时转义，反斜杠转义（str 面
    // simple-path 足够——路径语料不含引号/反斜杠边界，双侧对拍钉板）。
    let inner = s.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{inner}'")
}

/// `os.path.join` 形目录拼接（rel 含前导 / 时 Python 归绝对——镜像）。
fn join_path(dir: &str, rel: &str) -> String {
    if rel.starts_with('/') {
        rel.to_string()
    } else if dir.is_empty() {
        rel.to_string()
    } else {
        format!("{dir}/{rel}")
    }
}

// ---- gensourceindex parse_index 单源复用（path 依赖同 crate）----
// Python 侧惰性 `import gensourceindex as gsi` 单解释器复用；Rust 侧
// 等价物 = Cargo.toml path 依赖（mkapp→mkcpio path 依赖先例同款——
// 单解释器纪律不弃，双实现禁）。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_path_forms() {
        assert_eq!(join_path("a/b", "c.toml"), "a/b/c.toml");
        assert_eq!(join_path("", "c.toml"), "c.toml");
        assert_eq!(join_path("a/b", "/abs"), "/abs");
    }
}
