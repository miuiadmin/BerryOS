//! P1 签名面（W1-Q27 九刀）：Python `_find_berry_sign`/`_run_berry_sign`/
//! `_sign_bap`/`_verify_bap_sig`（tools/mkapp.py :1358-1405 段）的 Rust
//! 镜像——SDK-2 分发签名链集成刀（24 号 §4.3 半刀②③的 CLI 面）。
//! berry-sign = user/berry-crypto bin（信封 v1 TLV + ed25519 + SHA-512/256
//! 摘要——envelope.rs/ed25519.rs/sha512.rs 单源）；本模块只做子进程封装、
//! 二进制发现序与拒因透传，不重复执法——验收线四件（正样例 + 篡改/
//! 断链/换钥三负样例）主归属 = `berry-sign selftest`，CLI 面对拍 =
//! tests/sign_parity.rs（Python oracle 真流）。
//!
//! 可选面纪律：CLI 无 --sign-key/--verify-key 时本模块零参与，行为与
//! 旧版逐字节同（向后兼容）。单源对齐义务：改任一侧必须同步另一侧
//! （elf2flat/mkcpio/mkfat/mkimage 换轨同款纪律）。
//!
//! 拒因形态（Python `Reject` kwargs 镜像；CLI stderr 只印 message——
//! `mkapp: {message}`。fix 仅进 --json 拒因舱，而签名面不可达 --json
//! （组合 = 用法错，mkapp.py 头注 :102），故 jx_* 恒 None、where_ 恒
//! ""——CLI 可见面与 Python 全等）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::gate::Reject;

/// 发现序候选：repo 根下两个 CARGO_TARGET_DIR 惯用位（target-cargo 先于
/// user/cargo-target——Python :1366 元组序镜像）。
pub const SIGN_TARGET_DIRS: [&str; 2] = ["target-cargo", "user/cargo-target"];
/// 发现序候选：profile（debug 先于 release——Python :1367 元组序镜像）。
pub const SIGN_PROFILES: [&str; 2] = ["debug", "release"];
/// 发现序候选：host triple（berry-sign 惯用构建位）。
pub const SIGN_TRIPLE: &str = "aarch64-apple-darwin";

/// 可执行常规文件判（Python `os.path.isfile` + `os.access(X_OK)` 合取）。
fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match fs::metadata(p) {
        Ok(md) => md.is_file() && md.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

fn rej(msg: String, code: &'static str) -> Reject {
    Reject {
        message: msg,
        code,
        where_: "",
        line: None,
        jx_where: None,
        jx_expected: None,
        jx_got: None,
        jx_fix: None,
    }
}

/// 发现序核心（纯函数——env 值与 repo 根由调用方注入，测试免进程全局
/// 态）：`env_val` 非空且为可执行常规文件 → 直接采用；否则按
/// SIGN_TARGET_DIRS × SIGN_PROFILES 扫候选。`Some("")` = Python 真值判
/// 的「未设」同语义。不可得 = None（调用方 loud 拒——签名面不静默）。
pub fn find_berry_sign_at(root: &Path, env_val: Option<&str>) -> Option<PathBuf> {
    if let Some(v) = env_val {
        if !v.is_empty() && is_executable_file(Path::new(v)) {
            return Some(PathBuf::from(v));
        }
    }
    for tdir in SIGN_TARGET_DIRS {
        for profile in SIGN_PROFILES {
            let cand = root
                .join(tdir)
                .join(SIGN_TRIPLE)
                .join(profile)
                .join("berry-sign");
            if is_executable_file(&cand) {
                return Some(cand);
            }
        }
    }
    None
}

/// 现役发现入口（Python `_find_berry_sign` 镜像）：repo 根 = 编译期仓根
/// （`CARGO_MANIFEST_DIR/../..`——Python `abspath(__file__)` 的源定位同
/// 位语义；仓内构建（justfile 先编后用）恒成立）。
pub fn find_berry_sign() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().unwrap_or(root);
    find_berry_sign_at(&root, std::env::var("BERRY_SIGN").ok().as_deref())
}

fn run_berry_sign(argv: &[&str]) -> Result<Output, Reject> {
    let bs = find_berry_sign().ok_or_else(|| {
        rej(
            "sign: berry-sign 不可得（设 BERRY_SIGN 或先 CARGO_TARGET_DIR=<隔离目录> cargo build --manifest-path user/Cargo.toml -p berry-crypto --bin berry-sign --target aarch64-apple-darwin）".to_string(),
            "sign.tool-missing",
        )
    })?;
    Command::new(&bs)
        .args(argv)
        .output()
        .map_err(|e| rej(format!("sign: 执行 {} 失败： {e}", bs.display()), "sign.exec"))
}

/// berry-sign 出向文本（Python `(r.stderr or r.stdout).strip()` 前置的
/// 取材序镜像——stderr 非空优先，空则 stdout）。
fn out_text(o: &Output) -> String {
    if !o.stderr.is_empty() {
        String::from_utf8_lossy(&o.stderr).into_owned()
    } else {
        String::from_utf8_lossy(&o.stdout).into_owned()
    }
}

/// 打包尾部产 `<bap>.sig`（Python `_sign_bap` 镜像：berry-sign sign；包内
/// manifest 缺 [security] 三键由 berry-sign 拒——key_lineage 是信封回声
/// 源，mkapp 不重复执法）。Ok = sign 行（stdout strip）。
pub fn sign_bap(sk: &str, bap_path: &str) -> Result<String, Reject> {
    let out = run_berry_sign(&["sign", sk, bap_path])?;
    if !out.status.success() {
        return Err(rej(out_text(&out).trim().to_string(), "sign.sign"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// check 尾部验既有 `<bap>.sig`（Python `_verify_bap_sig` 镜像：berry-sign
/// verify；.sig 缺失 = berry-sign 拒且文案含防剥离注——v1.2 §三 :63
/// 护栏① host 对应物）。Ok = verify 行。
pub fn verify_bap_sig(pk: &str, bap_path: &str) -> Result<String, Reject> {
    let out = run_berry_sign(&["verify", pk, bap_path])?;
    if !out.status.success() {
        return Err(rej(out_text(&out).trim().to_string(), "sign.verify"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod sign_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mkapp-signunit-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn mk_exe(p: &Path) {
        if let Some(par) = p.parent() {
            fs::create_dir_all(par).unwrap();
        }
        fs::write(p, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn discovery_env_branch_python_mirror() {
        let t = tmpdir("env");
        let root = t.join("root"); // 空根：无候选位
        fs::create_dir_all(&root).unwrap();
        let envbs = t.join("bs"); mk_exe(&envbs);
        // env 在席且可执行 → 直接采用（优先于一切候选）
        assert_eq!(
            find_berry_sign_at(&root, Some(envbs.to_str().unwrap())),
            Some(envbs.clone())
        );
        // 空串 = Python 真值判「未设」→ 落候选（空根 → None）
        assert_eq!(find_berry_sign_at(&root, Some("")), None);
        // env 指不存在件 → 非可执行 → 落候选（None）——不静默回落 env 值
        assert_eq!(find_berry_sign_at(&root, Some("/nonexistent/bs")), None);
        assert_eq!(find_berry_sign_at(&root, None), None);
    }

    #[test]
    fn discovery_candidate_order_python_mirror() {
        // Python :1366-1370 双层循环序：target-cargo/debug →
        // target-cargo/release → user/cargo-target/debug → user/…/release
        let t = tmpdir("order");
        let root = t.join("root");
        let cand = |tdir: &str, prof: &str| {
            root.join(tdir).join(SIGN_TRIPLE).join(prof).join("berry-sign")
        };
        let u_d = cand("user/cargo-target", "debug");
        mk_exe(&u_d);
        assert_eq!(find_berry_sign_at(&root, None), Some(u_d), "仅 user/debug 在席");
        let t_r = cand("target-cargo", "release");
        mk_exe(&t_r);
        assert_eq!(find_berry_sign_at(&root, None), Some(t_r), "target-cargo/release 先于 user/debug");
        let t_d = cand("target-cargo", "debug");
        mk_exe(&t_d);
        assert_eq!(find_berry_sign_at(&root, None), Some(t_d), "debug 先于 release");
    }

    #[test]
    fn discovery_rejects_non_executable_and_dir() {
        // os.X_OK 镜像：普通文件 0o644 与目录都不算候选
        let t = tmpdir("xok");
        let root = t.join("root");
        let c = cand_of(&root);
        fs::create_dir_all(c.parent().unwrap()).unwrap();
        fs::write(&c, b"not exec").unwrap();
        fs::set_permissions(&c, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(find_berry_sign_at(&root, None), None, "0o644 不算");
        let _ = fs::remove_file(&c);
        fs::create_dir_all(&c).unwrap();
        assert_eq!(find_berry_sign_at(&root, None), None, "目录不算");
    }

    fn cand_of(root: &Path) -> PathBuf {
        root.join(SIGN_TARGET_DIRS[0]).join(SIGN_TRIPLE).join(SIGN_PROFILES[0]).join("berry-sign")
    }

    #[test]
    fn reject_shape_cli_visible_face() {
        // 拒因形态钉板：code 稳定名 + message 全文（CLI stderr 唯一可见面）
        let e = rej("x".to_string(), "sign.exec");
        assert_eq!(e.code, "sign.exec");
        assert_eq!(e.message, "x");
        assert_eq!(e.where_, "");
        assert!(e.jx_fix.is_none() && e.line.is_none());
    }
}
