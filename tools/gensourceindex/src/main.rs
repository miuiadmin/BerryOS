//! gensourceindex Rust CLI（三刀·CLI 面：--verify / --gen-keys 消费
//! 半边；五刀·生成面臂：rootfs-manifest 路换轨 Rust——`--out-dir` +
//! `--source-pk` 全旗标集实装，sign-user 产链位走本 bin）。出口码同
//! Python：拒因 1、用法错 2、成功 0。`--selftest` 走 python3
//! （selftest_guard 零豁免契约）；`--preset` 面不镜像（berry_assemble
//! Python 现役单源——遇即让位指引 python3，拒因文案注明）。

use std::process::ExitCode;

use gensourceindex::{parse_index, parse_keys_listing, KEYS_NAME};

const USAGE: &str = "用法：gensourceindex [--rootfs-manifest <toml>] [--user-dir <dir>] --out-dir <dir> --source-pk <pk> [--sign-key <sk>] [--source-id <id>] [--index-version N] [--prev <tlv>] | gensourceindex --verify <source-index.tlv> [--prev <tlv>] | gensourceindex --gen-keys <sk-out> <pk-out>（--preset/--selftest 走 python3 tools/gensourceindex.py）";

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(|s| s.as_str()) == Some("--verify") {
        return cmd_verify(&argv[1..]);
    }
    if argv.first().map(|s| s.as_str()) == Some("--gen-keys") {
        return cmd_gen_keys(&argv[1..]);
    }
    cmd_generate_or_usage(&argv)
}

/// --verify：结构/单调/两件套对拍（Python `do_verify` 镜像——签名
/// 真验子进程面 = 二刀 sign.rs，本刀 CLI 组装）。
fn cmd_verify(rest: &[String]) -> ExitCode {
    // 恰 1 或 3 参（<tlv> / <tlv> --prev <tlv>——Python argparse 同域）。
    let (path, prev) = match rest {
        [p] => (p.as_str(), None),
        [p, f, v] if f == "--prev" => (p.as_str(), Some(v.as_str())),
        _ => {
            eprintln!("gensourceindex: {USAGE}");
            return ExitCode::from(2);
        }
    };
    cmd_verify_parsed(path, prev)
}

/// --verify 已解析形（五刀通用旗标解析器共入口——`--verify` 非首参
/// 形经 `cmd_generate_or_usage` 解析后落此）。
fn cmd_verify_parsed(path: &str, prev: Option<&str>) -> ExitCode {
    let raw = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("gensourceindex: FAIL: 读 {path}：{e}");
            return ExitCode::from(1);
        }
    };
    let idx = match parse_index(&raw) {
        Ok(i) => i,
        Err(r) => {
            eprintln!("{r}");
            return ExitCode::from(1);
        }
    };
    // 八字段齐全性复核（parse 全量执法后逐条在场——Python n8 口径）。
    let n8 = idx
        .entries
        .iter()
        .filter(|e| {
            !e.id.is_empty()
                && !e.lineage.is_empty()
                && !e.cert.is_empty()
        })
        .count();
    let n_wit = idx.entries.iter().filter(|e| e.pk.is_some()).count();
    let sig_state = if idx.signed { "signed" } else { "placeholder" };
    println!(
        "gensourceindex: verify {path} OK entries={} index_version={} witnessed={n_wit} sig={sig_state}",
        idx.entries.len(),
        idx.index_ver
    );
    println!(
        "gensourceindex:   八字段 {n8}/{} 齐全（id/三值版本/size/哈希/key_lineage/公钥见证/last_revision/cert_status）",
        idx.entries.len()
    );
    if let Some(prev_path) = prev {
        let praw = match std::fs::read(prev_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("gensourceindex: FAIL: 读 {prev_path}：{e}");
                return ExitCode::from(1);
            }
        };
        let prev_ver = match parse_index(&praw) {
            Ok(i) => i.index_ver,
            Err(r) => {
                eprintln!("{r}");
                return ExitCode::from(1);
            }
        };
        if let Err(r) = gensourceindex::check_index_version(idx.index_ver, prev_ver) {
            eprintln!("{r}");
            return ExitCode::from(1);
        }
        println!(
            "gensourceindex:   单调: prev={prev_ver} → {} 过（无回降——check_gen 同口径）",
            idx.index_ver
        );
    }
    if idx.signed {
        let vr = gensourceindex::sign::verify_index_signature(
            &raw,
            &idx.source_pk,
            &idx.src_sig,
        );
        match vr {
            Some(true) => println!(
                "gensourceindex:   签名: 真验过（BSRC-TOP 覆盖域 × berry-sign verify-raw，源钥 ed25519）"
            ),
            Some(false) => {
                eprintln!(
                    "gensourceindex: FAIL: {path}: 签名验拒——BSRC-TOP 覆盖域对不上（索引被篡改或源钥不符；重签或回查 --sign-key/--source-pk 配对）"
                );
                return ExitCode::from(1);
            }
            None => println!(
                "gensourceindex:   签名: 未验（如实注——berry-sign 不可得，结构面已全量执法；验签面 = verify_index_signature）"
            ),
        }
    } else {
        println!(
            "gensourceindex:   签名: 占位如实注——索引未签（无 --sign-key 产物；向后兼容面——dev 链 unsigned 索引，设备侧语义随 P3 裁〔设计 §五-Q6〕）"
        );
    }
    let kpath = std::path::Path::new(path)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(KEYS_NAME);
    if kpath.is_file() {
        let kraw = match std::fs::read(&kpath) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("gensourceindex: FAIL: 读 {}: {e}", kpath.display());
                return ExitCode::from(1);
            }
        };
        let (sid, pk, signed) = match parse_keys_listing(&kraw) {
            Ok(t) => t,
            Err(r) => {
                eprintln!("{r}");
                return ExitCode::from(1);
            }
        };
        if pk != idx.source_pk {
            eprintln!(
                "gensourceindex: FAIL: 两件套对拍拒：清单 pk != 索引 source_pk（{}）",
                kpath.display()
            );
            return ExitCode::from(1);
        }
        if signed != idx.signed {
            eprintln!(
                "gensourceindex: FAIL: 两件套对拍拒：sig_state {} != 索引 flags.signed={}",
                if signed { "signed" } else { "unsigned" },
                idx.signed
            );
            return ExitCode::from(1);
        }
        println!(
            "gensourceindex:   两件套对拍: {KEYS_NAME} source_id={sid} pk/fingerprint/sig_state 一致"
        );
    }
    ExitCode::SUCCESS
}

/// 生成面旗标集（Python main() argparse 面 :1298-1352 镜像——五刀换轨
/// 臂）。默认值：`--rootfs-manifest` = <repo>/tools/rootfs-manifest.toml
/// （表头注契约——发现序 = bin 所在 tools/ 目录）、`--user-dir` =
/// <repo>/user、`--source-id` = berryos、`--index-version` = 1。
struct GenArgs {
    rootfs_manifest: Option<String>,
    user_dir: Option<String>,
    out_dir: Option<String>,
    source_pk: Option<String>,
    sign_key: Option<String>,
    source_id: String,
    index_version: u32,
    prev: Option<String>,
    /// `--verify` 值（旗标可非首参——Python argparse 混排域）。
    verify: Option<String>,
}

/// 生成面 / --verify 通用旗标解析（Python argparse 同域：值旗标
/// `--flag v` 与 `--flag=v` 双形、`--selftest`/`--preset` 即时让位）。
/// 未知旗标 / 缺值 = 用法错 2（argparse error 出口同构）。
fn parse_gen_flags(argv: &[String]) -> Result<GenArgs, ExitCode> {
    let mut a = GenArgs {
        rootfs_manifest: None,
        user_dir: None,
        out_dir: None,
        source_pk: None,
        sign_key: None,
        source_id: "berryos".to_string(),
        index_version: 1,
        prev: None,
        verify: None,
    };
    let val = |name: &str, inline: Option<&str>, i: &mut usize, argv: &[String]|
        -> Result<String, ExitCode> {
        if let Some(v) = inline {
            return Ok(v.to_string());
        }
        *i += 1;
        let Some(v) = argv.get(*i) else {
            eprintln!("gensourceindex: {name} 缺值（expected one argument）");
            return Err(ExitCode::from(2));
        };
        Ok(v.clone())
    };
    let mut i = 0usize;
    while i < argv.len() {
        let (flag, inline): (&str, Option<&str>) = match argv[i].split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v)),
            _ => (argv[i].as_str(), None),
        };
        match flag {
            "--selftest" | "--preset" => {
                // Python 现役面即时让位（单源纪律——berry_assemble 模块
                // 图根 + selftest_guard 零豁免契约）。
                let face = if flag == "--preset" {
                    "G16 产品派生面 berry_assemble Python 单源"
                } else {
                    "门禁自检 oracle 面"
                };
                eprintln!("gensourceindex: {flag} 走 python3 tools/gensourceindex.py（{face}——Rust 件不镜像，让位指引）");
                return Err(ExitCode::from(2));
            }
            "--rootfs-manifest" => a.rootfs_manifest = Some(val(flag, inline, &mut i, argv)?),
            "--user-dir" => a.user_dir = Some(val(flag, inline, &mut i, argv)?),
            "--out-dir" => a.out_dir = Some(val(flag, inline, &mut i, argv)?),
            "--source-pk" => a.source_pk = Some(val(flag, inline, &mut i, argv)?),
            "--sign-key" => a.sign_key = Some(val(flag, inline, &mut i, argv)?),
            "--source-id" => a.source_id = val(flag, inline, &mut i, argv)?,
            "--prev" => a.prev = Some(val(flag, inline, &mut i, argv)?),
            "--verify" => a.verify = Some(val(flag, inline, &mut i, argv)?),
            "--index-version" => {
                let v = val(flag, inline, &mut i, argv)?;
                match v.parse::<u32>() {
                    Ok(n) => a.index_version = n,
                    Err(_) => {
                        eprintln!("gensourceindex: invalid int value '{v}'（--index-version）");
                        return Err(ExitCode::from(2));
                    }
                }
            }
            other => {
                eprintln!("gensourceindex: 未知旗标 {other:?}（unrecognized arguments: {other}）");
                return Err(ExitCode::from(2));
            }
        }
        i += 1;
    }
    Ok(a)
}

/// 生成面主臂（Python main() 末段 :1336-1352 判序镜像：--verify 独占
/// （不与生成面混用）→ --sign-key 需 --source-pk 同席 → 生成需
/// --out-dir + --source-pk）。
fn cmd_generate_or_usage(argv: &[String]) -> ExitCode {
    let a = match parse_gen_flags(argv) {
        Ok(a) => a,
        Err(code) => return code,
    };
    if let Some(vpath) = a.verify {
        if a.out_dir.is_some() || a.source_pk.is_some() || a.sign_key.is_some() {
            eprintln!("gensourceindex: --verify 独占（不与生成面混用）");
            return ExitCode::from(2);
        }
        return cmd_verify_parsed(&vpath, a.prev.as_deref());
    }
    if a.sign_key.is_some() && a.source_pk.is_none() {
        eprintln!(
            "gensourceindex: --sign-key 需与 --source-pk 同席（pk 进索引自述 + sk 签名——钥对同源）"
        );
        return ExitCode::from(2);
    }
    let (Some(out_dir), Some(source_pk)) = (a.out_dir.as_deref(), a.source_pk.as_deref()) else {
        eprintln!("gensourceindex: 生成需 --out-dir + --source-pk（或用 --verify/--gen-keys/--selftest）");
        return ExitCode::from(2);
    };
    // 默认值：tools/ 目录发现（gen.rs repo_tools_dir——CWD/exe 双上溯）。
    let tools = gensourceindex::gen::repo_tools_dir();
    let manifest_path = a
        .rootfs_manifest
        .clone()
        .unwrap_or_else(|| tools.join("rootfs-manifest.toml").display().to_string());
    let user_dir = a
        .user_dir
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| tools.parent().unwrap_or(std::path::Path::new(".")).join("user"));
    let mtext = match std::fs::read_to_string(&manifest_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("gensourceindex: FAIL: 读 {manifest_path}：{e}");
            return ExitCode::from(1);
        }
    };
    let entries = match gensourceindex::gen::load_app_entries(&mtext, &manifest_path, &user_dir) {
        Ok(es) => es,
        Err(r) => {
            eprintln!("{r}");
            return ExitCode::from(1);
        }
    };
    let p = gensourceindex::gen::GenParams {
        entries,
        source_pk: std::path::Path::new(source_pk),
        sign_key: a.sign_key.as_deref().map(std::path::Path::new),
        source_id: &a.source_id,
        index_version: a.index_version,
        prev: a.prev.as_deref().map(std::path::Path::new),
        out_dir: std::path::Path::new(out_dir),
    };
    match gensourceindex::gen::do_generate(&p) {
        Ok(lines) => {
            for l in lines {
                println!("{l}");
            }
            ExitCode::SUCCESS
        }
        Err(r) => {
            eprintln!("{r}");
            ExitCode::from(1)
        }
    }
}

/// --gen-keys：berry-sign keygen 透传（Python `do_gen_keys` 镜像）。
fn cmd_gen_keys(rest: &[String]) -> ExitCode {
    let [sk_out, pk_out] = match rest {
        [a, b] => [a, b],
        _ => {
            eprintln!("gensourceindex: {USAGE}");
            return ExitCode::from(2);
        }
    };
    let out = gensourceindex::sign::run_berry_sign(&["keygen", sk_out, pk_out]);
    match out {
        None => {
            eprintln!(
                "gensourceindex: FAIL: berry-sign 不可得（设 BERRY_SIGN 或先 CARGO_TARGET_DIR=/tmp/target-p2 cargo build --manifest-path user/Cargo.toml -p berry-crypto --bin berry-sign --target aarch64-apple-darwin——mkapp sign.tool-missing 同文案）"
            );
            ExitCode::from(1)
        }
        Some(Err(e)) => {
            eprintln!("gensourceindex: FAIL: berry-sign keygen 拒：{e}");
            ExitCode::from(1)
        }
        Some(Ok(o)) => {
            if !o.status.success() {
                let t = if !o.stderr.is_empty() {
                    String::from_utf8_lossy(&o.stderr)
                } else {
                    String::from_utf8_lossy(&o.stdout)
                };
                eprintln!("gensourceindex: FAIL: berry-sign keygen 拒：{}", t.trim());
                return ExitCode::from(1);
            }
            match gensourceindex::gen_keys_lines(Some(&String::from_utf8_lossy(&o.stdout))) {
                Ok(ls) => {
                    for l in ls {
                        println!("{l}");
                    }
                    ExitCode::SUCCESS
                }
                Err(r) => {
                    eprintln!("{r}");
                    ExitCode::from(1)
                }
            }
        }
    }
}
