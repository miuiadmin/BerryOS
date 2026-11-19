//! 二刀·berry-sign 面（W1-Q4 续七·彻底 Rust 化轨道 b 层 #5 刀序第五位）：
//! Python `find_berry_sign`/`run_berry_sign`/`sign_index`/
//! `verify_index_signature`（tools/gensourceindex.py :291-433 段）的 Rust
//! 镜像——SDK-2 索引签名链子进程面。 berry-sign = user/berry-crypto bin
//! （envelope.rs/ed25519.rs/sha512.rs 单源）；本模块只做二进制发现序、
//! 子进程封装、覆盖域重组建与拒因透传，不重复执法——验收主归属 =
//! `berry-sign selftest`，CLI 面对拍 = tests/sign_parity.rs（Python
//! oracle 真流）。
//!
//! 单源对齐义务：改任一侧必须同步另一侧（elf2flat/mkcpio/mkfat/mkimage/
//! mkapp 换轨同款纪律）。发现序与 mkapp sign.rs（九刀）同构但候选表不同
//! （Python 两件各自硬编：gensourceindex = REPO/target-cargo + /tmp/
//! target-p1 + /tmp/target-p2；mkapp = target-cargo + user/cargo-target）
//! ——两侧均按各自 Python 元组序镜像，不合并单源。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::{Reject, SIG_LEN, SRC_LABEL};

/// 发现序候选：三个 CARGO_TARGET_DIR 惯用位（Python :299 元组序镜像——
/// repo 根下 target-cargo 先于 /tmp 两位）。
pub const SIGN_TARGET_DIRS: [&str; 3] = ["target-cargo", "/tmp/target-p1", "/tmp/target-p2"];
/// 发现序候选：profile（debug 先于 release——Python :300 元组序镜像）。
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

/// 发现序核心（纯函数——env 值与 repo 根由调用方注入，测试免进程全局
/// 态）：`env_val` 非空且为可执行常规文件 → 直接采用；否则候选位
/// target-dir × profile 双层扫描。`Some(\"\")` = Python 真值判的「未设」
/// 同语义。不可得 = None（调用方决定 loud 或如实注）。
pub fn find_berry_sign_at(root: &Path, env_val: Option<&str>) -> Option<PathBuf> {
    if let Some(v) = env_val {
        if !v.is_empty() && is_executable_file(Path::new(v)) {
            return Some(PathBuf::from(v));
        }
    }
    for tdir in SIGN_TARGET_DIRS {
        for profile in SIGN_PROFILES {
            let cand = if Path::new(tdir).is_absolute() {
                PathBuf::from(tdir)
            } else {
                root.join(tdir)
            }
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

/// 现役发现入口（Python `find_berry_sign` 镜像）：repo 根 = 编译期仓根
/// （`CARGO_MANIFEST_DIR/../..`——Python `os.path.dirname(REPO)` 的源
/// 定位同位语义；仓内构建恒成立）。
pub fn find_berry_sign() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().unwrap_or(root);
    find_berry_sign_at(&root, std::env::var("BERRY_SIGN").ok().as_deref())
}

/// 子进程调用（Python `run_berry_sign` 镜像）：不可得 / spawn 失败 =
/// None（调用方如实注或 loud——显式签名面不静默，加固面不硬依）。
pub fn run_berry_sign(argv: &[&str]) -> Option<std::io::Result<Output>> {
    let bs = find_berry_sign()?;
    Some(Command::new(&bs).args(argv).output())
}

fn rej(msg: impl Into<String>) -> Reject {
    Reject::new(msg)
}

/// berry-sign 出向文本（Python `(r.stderr or r.stdout).strip()` 取材序
/// 镜像——stderr 非空优先，空则 stdout）。
fn out_text(o: &Output) -> String {
    let s = if !o.stderr.is_empty() {
        String::from_utf8_lossy(&o.stderr).into_owned()
    } else {
        String::from_utf8_lossy(&o.stdout).into_owned()
    };
    s.trim().to_string()
}

/// 索引签名单点缝（Python `sign_index` 镜像——#77 实装）。`index_form`
/// = 签名预置形（flags.signed=1 + 尾 64B 零槽——`build_index(.., Some
/// (&[0u8; SIG_LEN]))` 产物）；覆盖域 = SRC_LABEL ‖ 槽前全字节。
/// Ok = (64B 签名, 如实注)。
///
/// `sk_path = None` → unsigned 诚实降级（不调子进程，返回如实注——调用
/// 方不换槽、清单 sig_state=unsigned）。显式签名面：berry-sign 不可得 /
/// 子进程拒 = loud die（不静默降级——mkapp sign 同纪律）。调用形 =
/// `sign-raw <sk> <cover>` stdout 面（berry-sign CLI 合同：`--out` 缺省
/// = stdout 原始 64B 零附注——管道纯度），覆盖域经临时文件传递。
pub fn sign_index(index_form: Option<&[u8]>, sk_path: Option<&str>) -> Result<([u8; SIG_LEN], String), Reject> {
    let Some(sk_path) = sk_path else {
        return Ok((
            [0u8; SIG_LEN],
            "--sign-key 缺席——索引未签（签名槽零占位 + flags.signed=0 + 清单 sig_state=unsigned，向后兼容现状面；签名面 = sign_index() × berry-sign sign-raw）".to_string(),
        ));
    };
    let index_form = index_form
        .ok_or_else(|| rej("sign_index：签名预置形缺席（内部调用序错——index_form 必随 sk_path 在席）"))?;
    let cover = [SRC_LABEL, &index_form[..index_form.len() - SIG_LEN]].concat();
    let td = tempdir_create("gsi-sign");
    let cover_p = td.join("cover.bin");
    write_or_die(&cover_p, &cover)?;
    let out = match run_berry_sign(&["sign-raw", sk_path, cover_p.to_str().unwrap_or("")]) {
        Some(Ok(o)) => o,
        _ => {
            let _ = fs::remove_dir_all(&td);
            return Err(rej(
                "berry-sign 不可得（设 BERRY_SIGN 或先 CARGO_TARGET_DIR=/tmp/target-p2 cargo build --manifest-path user/Cargo.toml -p berry-crypto --bin berry-sign --target aarch64-apple-darwin——do_gen_keys 同文案）——--sign-key 显式签名面不静默降级（mkapp sign.tool-missing 同纪律）",
            ));
        }
    };
    let _ = fs::remove_dir_all(&td);
    if !out.status.success() {
        return Err(rej(format!("索引签名拒：{}", out_text(&out))));
    }
    let sig = out.stdout;
    if sig.len() != SIG_LEN {
        return Err(rej(format!(
            "sign-raw 产物非 {SIG_LEN}B（got {}B——裸签名独立合同面）",
            sig.len()
        )));
    }
    let mut s = [0u8; SIG_LEN];
    s.copy_from_slice(&sig);
    Ok((
        s,
        format!(
            "berry-sign sign-raw 64B ed25519 裸签名（覆盖域 {:?} ‖ 槽前 {}B；独立于 .bap 信封合同）",
            String::from_utf8_lossy(SRC_LABEL),
            cover.len() - SRC_LABEL.len()
        ),
    ))
}

/// 索引签名真验（Python `verify_index_signature` 镜像——--verify 消费
/// 面）。覆盖域重建 = SRC_LABEL ‖ 槽前全字节 → berry-sign verify-raw
/// （pk/cover/sig 三临时文件——CLI 合同为文件参数）。Some(true)/
/// Some(false) = 过/拒；None = berry-sign 不可得（如实注跳过——零构建
/// 依赖纪律，结构面已由 parse_index 全量执法）。
pub fn verify_index_signature(raw: &[u8], source_pk: &[u8], src_sig: &[u8]) -> Option<bool> {
    use std::io::Write;
    let bs = find_berry_sign()?;
    let cover = [SRC_LABEL, &raw[..raw.len() - SIG_LEN]].concat();
    let td = tempdir_create("gsi-verify");
    let (pk_p, cover_p, sig_p) = (td.join("pk.bin"), td.join("cover.bin"), td.join("sig.bin"));
    let put = |p: &Path, data: &[u8]| -> Option<()> {
        let mut f = fs::File::create(p).ok()?;
        f.write_all(data).ok()?;
        Some(())
    };
    put(&pk_p, source_pk)?;
    put(&cover_p, &cover)?;
    put(&sig_p, src_sig)?;
    let out = Command::new(&bs)
        .arg("verify-raw")
        .arg(&pk_p)
        .arg(&cover_p)
        .arg(&sig_p)
        .output();
    let _ = fs::remove_dir_all(&td);
    match out {
        Ok(o) => Some(o.status.success()),
        Err(_) => None,
    }
}

/// 测试/签名面共用临时目录（Python `tempfile.TemporaryDirectory(prefix=)`
/// 同形；调用方负责 remove_dir_all）。
fn tempdir_create(prefix: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = fs::create_dir_all(&d);
    d
}

fn write_or_die(path: &Path, data: &[u8]) -> Result<(), Reject> {
    std::fs::write(path, data).map_err(|e| rej(format!("写 {}: {e}", path.display())))
}

#[cfg(test)]
mod sign_tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gsi-signunit-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn mk_exe(p: &Path) {
        if let Some(par) = p.parent() {
            fs::create_dir_all(par).unwrap();
        }
        fs::write(p, b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn discovery_env_branch_python_mirror() {
        let t = tmpdir("env");
        let root = t.join("root"); // 空根：/tmp 候选位由绝对路径独立构成——
        fs::create_dir_all(&root).unwrap(); // 测试不污染真 /tmp，只验相对位序
        let envbs = t.join("bs");
        mk_exe(&envbs);
        assert_eq!(
            find_berry_sign_at(&root, Some(envbs.to_str().unwrap())),
            Some(envbs.clone())
        );
        // 空串 = Python 真值判「未设」。
        assert_eq!(find_berry_sign_at(&root, Some("")), None);
        // env 指不存在件 → 落候选（不静默回落 env 值）。
        assert_eq!(find_berry_sign_at(&root, Some("/nonexistent/bs")), None);
    }

    #[test]
    fn discovery_candidate_order_python_mirror() {
        // Python :299-300 双层循环序：target-cargo/debug →
        // target-cargo/release → /tmp/target-p1/debug → …（绝对位
        // /tmp/target-p{1,2} 不注入合成件——不污染真 /tmp；相对位序由
        // 本腿钉死，绝对位形由 find_berry_sign_at 的 is_absolute 分支
        // + Python 元组序注释双侧背书）。
        let t = tmpdir("order");
        let root = t.join("root");
        fs::create_dir_all(&root).unwrap();
        // 只放 target-cargo/release → 相对位首选命中。
        let t_r = root
            .join("target-cargo")
            .join(SIGN_TRIPLE)
            .join("release")
            .join("berry-sign");
        mk_exe(&t_r);
        assert_eq!(find_berry_sign_at(&root, None), Some(t_r));
        // 放 target-cargo/debug → debug 先于 release。
        let t_d = root
            .join("target-cargo")
            .join(SIGN_TRIPLE)
            .join("debug")
            .join("berry-sign");
        mk_exe(&t_d);
        assert_eq!(find_berry_sign_at(&root, None), Some(t_d));
    }

    #[test]
    fn sign_index_unsigned_honest_note() {
        // sk 缺席 = 诚实降级：零子进程调用（berry-sign 缺席环境下依然
        // Ok——Python sign_index(None, None) 同形），注文本含关键词。
        let (sig, note) = sign_index(None, None).unwrap();
        assert_eq!(sig, [0u8; SIG_LEN]);
        assert!(note.contains("缺席") && note.contains("unsigned"));
    }

    #[test]
    fn sign_index_missing_form_is_internal_error() {
        // sk 在席但预置形缺席 = 内部调用序错（loud——不静默产零槽）。
        assert!(sign_index(None, Some("whatever.sk")).is_err());
    }

    #[test]
    fn verify_unavailable_returns_none() {
        // berry-sign 不可得（空根 + 无 env）→ None（如实注跳过语义）。
        let t = tmpdir("vnone");
        let root = t.join("root");
        fs::create_dir_all(&root).unwrap();
        assert_eq!(find_berry_sign_at(&root, None), None);
        // 现役入口带真 env：BERRY_SIGN 指不存在件 + /tmp 候选位无 berry-sign
        // 时 = None——此处仅验 API 形（返回 Option<bool>）。
        let _ = verify_index_signature(&[0u8; 110], &[0u8; 32], &[0u8; 64]);
    }
}
