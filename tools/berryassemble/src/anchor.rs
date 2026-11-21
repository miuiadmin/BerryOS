//! berryassemble 四刀第三片：kernel 锚现算对账面（Python
//! `measure_kernel_anchor` 镜像——G7 步⑧ 前置门）。
//!
//! 三态（Python :767-794 逐枝镜像）：
//! - 件在且等 → `Ok(true)`（计划面回显「现算对账过」位）；
//! - 件在而不等 → loud 拒（锚是合同不是装饰——OEM 拿旧锚装新件当场咬）；
//! - 件缺 → `Ok(false)`（只形校验过 + 调用面注记；OEM 可先声明后放件：
//!   清单先行、工件后置的合法时序；放件后 --check 即现算对账）。
//!
//! 解析基准 = product.toml 所在目录（`src_dir`）、件名 = 锚 name 段
//! （branding/source.instance 相对路径同款纪律）。SHA-256 = mkapp
//! `sha256_hex` path 依赖单解释器（自持 FIPS 180-4；Python 侧
//! `hashlib.sha256` 的 Rust 对偶——berryassemble→mkcpio→mkapp path 依赖
//! 链先例同款，零外部依赖纪律不破）。
//!
//! 拒面文案 = Python 现役件逐字镜像（tests/anchor_parity.rs oracle 真跑
//! 对拍钉板）。

use crate::lite::LiteProduct;
use crate::Reject;

/// G7 kernel 锚现算对账（`measure_kernel_anchor` 三态镜像）。
/// 返回值 = 是否已度量（`Ok(false)` = 件缺未度量——调用面注记，不拒）。
pub fn measure_kernel_anchor(p: &mut LiteProduct, src_dir: &str) -> Result<bool, Reject> {
    let local = join_path(src_dir, &p.kernel_name);
    let Ok(data) = std::fs::read(&local) else {
        // 件缺 → 只形校验过（is_file 分支镜像）
        p.kernel_measured = false;
        return Ok(false);
    };
    let got = mkapp::cpio::sha256_hex(&data);
    if got != p.kernel_sha {
        p.kernel_measured = false;
        return Err(Reject(format!(
            "platform.boot.kernel: 锚对账不等——包内 {} 现算 sha256:{} ≠ 声明 sha256:{}（件与锚不符，步⑧ 前置门——G7；改件须同步改锚）",
            p.kernel_name, got, p.kernel_sha
        )));
    }
    p.kernel_measured = true;
    Ok(true)
}

/// `os.path.join` 形目录拼接（full.rs join_path 同源复用——单源不复制）。
fn join_path(dir: &str, rel: &str) -> String {
    crate::full::join_path_pub(dir, rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_missing_and_empty_file() {
        // 件缺 → Ok(false)（OEM 先声明后放件合法时序）
        let mut p = LiteProduct {
            kernel_name: "KERNEL.ELF".into(),
            kernel_sha: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
            ..Default::default()
        };
        let tmp = std::env::temp_dir().join(format!("basm-anchor-t-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        assert_eq!(measure_kernel_anchor(&mut p, tmp.to_str().unwrap()).unwrap(), false);
        assert!(!p.kernel_measured);
        // 空件 → sha256("") = e3b0c442… 现算对账过（模板锚即空串摘要）
        std::fs::write(tmp.join("KERNEL.ELF"), b"").unwrap();
        assert_eq!(measure_kernel_anchor(&mut p, tmp.to_str().unwrap()).unwrap(), true);
        assert!(p.kernel_measured);
        // 件在而不等 → loud 拒（拒面文案钉板）
        std::fs::write(tmp.join("KERNEL.ELF"), b"not-the-declared-kernel").unwrap();
        let e = measure_kernel_anchor(&mut p, tmp.to_str().unwrap()).unwrap_err();
        assert!(e.0.starts_with("platform.boot.kernel: 锚对账不等——包内 KERNEL.ELF 现算 sha256:"));
        assert!(e.0.ends_with("（件与锚不符，步⑧ 前置门——G7；改件须同步改锚）"));
        assert!(!p.kernel_measured);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
