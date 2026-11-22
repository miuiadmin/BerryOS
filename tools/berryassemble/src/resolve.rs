//! berryassemble 四刀第二片：基座表对账 `resolve_entries`（Python
//! `berry_assemble.py :842` 逐字镜像——步④ rootfs 条目解析面）。
//!
//! 层界：结构面（lite）+ 产物面（full 第一片）之后、kernel 锚现算
//! （G7 归后续片）之前的基座对账段。语义 = 基座（V13 表）⊕ preset
//! （app/ 条目筛选）⊕ override 替换（G1）⊕ trust 锚条目（G2 现场重锚）
//! ⊕ 构建树逐条在席 ⊕ launcher 终表执法；排序 = 整条 name=path 字符串
//! （mkcpio CLI 现役语义——确定性单点，勿改成按名排序再拼接）。
//!
//! 基座表加载 = `mkcpio load_manifest` 单解释器 path 依赖复用（Python
//! 侧 `from mkcpio import load_manifest` 同款单加载器纪律；gsi 依赖
//! mkcpio 先例同链）。拒面文案 = Python 现役件逐字镜像
//! （tests/resolve_parity.rs oracle 真跑对拍钉板）；基座 TOML 坏形
//! 文案 tomllib 与 Rust 子集不同源——prefix 对拍（首刀先例）。

use crate::full::{join_path, py_repr};
use crate::lite::{app_id_of, py_list, LiteProduct};
use crate::Reject;
// full.rs 私有底座（join_path/py_repr）提 pub(crate) 供本片复用——单源律。

/// trust 锚条目归档名（Python `TRUST_PK_ENTRY` 同值——svcmgr cpio_find
/// 同串消费面约定）。
pub const TRUST_PK_ENTRY: &str = "etc/trust-pk";

/// Python OSError str 形镜像（open() 失败三常见态；main.rs 同源律——
/// bin 面私有件不可跨用，lib 面自带）。
fn py_oserror_str(e: &std::io::Error, path: &str) -> String {
    match e.raw_os_error() {
        Some(2) => format!("[Errno 2] No such file or directory: '{path}'"),
        Some(13) => format!("[Errno 13] Permission denied: '{path}'"),
        Some(21) => format!("[Errno 21] Is a directory: '{path}'"),
        _ => e.to_string(),
    }
}

/// 步④ rootfs 条目解析（Python `resolve_entries` 镜像）。返回
/// （整条 name=path 排序列表, 基座非 app 条目数）。`src_dir` =
/// product.toml 所在目录（override/trust 绝对路径锚基准——Python
/// `prod.src_dir` 同口径，调用方传入）；`base_manifest` = 基座表路径；
/// `user_dir` = 基座 path 相对基准（构建树根）。
pub fn resolve_entries(
    p: &LiteProduct,
    src_dir: &str,
    base_manifest: &str,
    user_dir: &str,
) -> Result<(Vec<String>, usize), Reject> {
    // 基座表加载（单解释器——mkcpio load_manifest path 依赖；Python 侧
    // 惰性 import 同款）。读失败文案 = Python `mkcpio: FAIL: 读 {path}：
    // {e}` 逐字（OSError str 形同源可逐字对拍）；坏 TOML 文案 tomllib
    // 与 Rust 子集不同源——prefix 对拍（首刀先例），拒径同构。
    let text = std::fs::read_to_string(base_manifest).map_err(|e| {
        Reject(format!(
            "基座条目表加载失败：mkcpio: FAIL: 读 {base_manifest}：{}",
            py_oserror_str(&e, base_manifest)
        ))
    })?;
    let base = mkcpio::load_manifest(&text)
        .map_err(|e| Reject(format!("基座条目表加载失败：{e}")))?;
    let base_apps: std::collections::BTreeMap<&str, &str> = base
        .iter()
        .filter(|(n, _)| n.starts_with("app/"))
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    // preset 必须在基座表（纯新增仍拒——归 #83 键位定形）
    let missing: Vec<String> = p
        .preset
        .iter()
        .filter(|e| !base_apps.contains_key(e.as_str()))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(Reject(format!(
            "product.apps.preset: 条目不在基座表 {}（tools/rootfs-manifest.toml app/ 条目——纯新增归 #83 键位定形；替换已声明 app 走 [product.apps.override]）",
            py_list(&missing)
        )));
    }
    let mut final_: std::collections::BTreeMap<String, String> = base
        .iter()
        .filter(|(n, _)| !n.starts_with("app/"))
        .map(|(n, v)| (n.clone(), v.clone()))
        .collect();
    let n_base = final_.len();
    for entry in &p.preset {
        let app_id = app_id_of(entry).to_string();
        if let Some(rel) = p.apps_override.get(&app_id) {
            // G1：终表路径取 OEM 自有件（包内绝对）
            final_.insert(entry.clone(), join_path(src_dir, rel));
        } else if let Some(v) = base_apps.get(entry.as_str()) {
            final_.insert(entry.clone(), v.to_string());
        } else {
            unreachable!("missing 面已过检：{entry}");
        }
    }
    if !p.trust_pk_anchor.is_empty() {
        // G2：装载守卫锚条目（现场重锚——基座同名即覆盖）
        final_.insert(
            TRUST_PK_ENTRY.to_string(),
            join_path(src_dir, &p.trust_pk_anchor),
        );
    }
    // 构建树逐条在席对账（BTreeMap 迭代 = 名字典序——Python
    // `for name in sorted(final)` 同序，首缺席即拒）
    for (name, path) in &final_ {
        let is_abs = path.starts_with('/');
        let local = if is_abs {
            path.clone()
        } else {
            join_path(user_dir, path)
        };
        if !std::path::Path::new(&local).is_file() {
            let why = if is_abs {
                "OEM 锚件（override/trust——load 面已验后缺席）"
            } else {
                "基座表 path 相对 user/——先 just build-user"
            };
            return Err(Reject(format!(
                "rootfs 条目 {name}: 构建树缺席 {local}（{why}）"
            )));
        }
    }
    if !final_.contains_key(&p.launcher) {
        return Err(Reject(format!(
            "product.defaults.launcher: {} 不在 rootfs 终表（须指向预装集 app/ 条目或基座 bin/ 条目——schema §1.4）",
            py_repr(&p.launcher)
        )));
    }
    let mut items: Vec<String> = final_
        .iter()
        .map(|(n, v)| format!("{n}={v}"))
        .collect();
    items.sort();
    Ok((items, n_base))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oserror_str_law() {
        let e = std::io::Error::from_raw_os_error(2);
        assert_eq!(
            py_oserror_str(&e, "base.toml"),
            "[Errno 2] No such file or directory: 'base.toml'"
        );
    }
}
