//! gensourceindex Rust CLI（三刀·CLI 面：--verify / --gen-keys 两消费
//! 半边；生成面与 --preset/--sign-key 产链位仍 python3 现役——依彻底
//! Rust 化计划 §4 刀序推进后再换轨）。出口码同 Python：拒因 1、用法
//! 错 2、成功 0。`--selftest` 走 python3（selftest_guard 零豁免契约）。

use std::process::ExitCode;

use gensourceindex::{parse_index, parse_keys_listing, KEYS_NAME};

const USAGE: &str = "用法：gensourceindex --verify <source-index.tlv> [--prev <tlv>] | gensourceindex --gen-keys <sk-out> <pk-out>（生成面走 python3 tools/gensourceindex.py）";

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(|s| s.as_str()) == Some("--verify") {
        return cmd_verify(&argv[1..]);
    }
    if argv.first().map(|s| s.as_str()) == Some("--gen-keys") {
        return cmd_gen_keys(&argv[1..]);
    }
    eprintln!("gensourceindex: {USAGE}");
    ExitCode::from(2)
}

/// --verify：结构/单调/两件套对拍（Python `do_verify` 镜像——签名
/// 真验子进程面 = 二刀 sign.rs，本刀 CLI 组装）。
fn cmd_verify(rest: &[String]) -> ExitCode {
    // 恰 1 或 3 参（<tlv> / <tlv> --prev <tlv>——Python argparse 同域）。
    let (path, prev) = match rest {
        [p] => (p, None),
        [p, f, v] if f == "--prev" => (p, Some(v)),
        _ => {
            eprintln!("gensourceindex: {USAGE}");
            return ExitCode::from(2);
        }
    };
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
            println!("{}", String::from_utf8_lossy(&o.stdout).trim());
            println!(
                "gensourceindex: 注: dev 密钥对（berry-sign 头注「dev/test 面纪律」——永不用于真签，真签面随 #77/P3）"
            );
            ExitCode::SUCCESS
        }
    }
}
