//! W1 gensourceindex 首刀·对拍与钉板：①Python 现役件（tools/gensourceindex.py）
//! 真跑 oracle 对拍——build_index/parse_index/build_keys_listing 五组黄金
//! 形字节恒等 + 拒面文案全等；②SHA-512/256 NIST 向量钉板；③结构/值域/
//! canonical/flags-slot 拒面全谱（selftest ①④⑥⑩ 腿的 Rust 镜像）。
//!
//! Python oracle 经 `python3 -c` 真跑（import gensourceindex 现役件），
//! hex 输出对拍——不改 Python 件零耦合。

use gensourceindex::*;
use std::process::Command;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

const fn const_pk(start: u8) -> [u8; 32] {
    let mut a = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        a[i] = start + i as u8;
        i += 1;
    }
    a
}
const PK: [u8; 32] = const_pk(0xA0);
const PK2: [u8; 32] = const_pk(0x50);

fn mk_entry(
    eid: &str,
    ver: (u32, u32, u32),
    size: u64,
    lineage: &str,
    pk: Option<[u8; 32]>,
    last_rev: u32,
) -> Entry {
    Entry {
        id: eid.to_string(),
        ver,
        size,
        digest: core::array::from_fn(|i| i as u8),
        lineage: lineage.to_string(),
        pk,
        last_rev,
        cert: "active".to_string(),
    }
}

/// Python 现役件真跑（import + build_index hex）——oracle 单点。
fn py_build(entries_expr: &str, index_ver: u32, src_sig: Option<&str>) -> String {
    let sig = match src_sig {
        Some(s) => format!("src_sig=bytes.fromhex('{s}')"),
        None => "src_sig=None".to_string(),
    };
    let code = format!(
        "import sys; sys.path.insert(0, 'tools')\nimport gensourceindex as g\nprint(g.build_index({entries_expr}, {index_ver}, bytes.fromhex('{}'), {sig}).hex())",
        hex(&PK)
    );
    let o = Command::new("python3")
        .arg("-c")
        .arg(&code)
        .current_dir(repo_root())
        .output()
        .expect("spawn python3");
    assert!(o.status.success(), "python oracle 失败：{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn py_keys(sid: &str, pk: &[u8; 32], signed: bool) -> String {
    let code = format!(
        "import sys; sys.path.insert(0, 'tools')\nimport gensourceindex as g\nprint(g.build_keys_listing('{sid}', bytes.fromhex('{}'), signed={py_signed}).hex())",
        hex(pk),
        py_signed = if signed { "True" } else { "False" }
    );
    let o = Command::new("python3")
        .arg("-c")
        .arg(&code)
        .current_dir(repo_root())
        .output()
        .expect("spawn python3");
    assert!(o.status.success(), "python oracle 失败：{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

// ---- ① oracle 字节级对拍（五组黄金形）----

#[test]
fn parity_build_index_five_forms() {
    let cases: Vec<(&str, Vec<Entry>, u32)> = vec![
        ("single", vec![mk_entry("P2APP", (0, 1, 0), 4096, "berryos", Some(PK), 1)], 7),
        (
            "capfull",
            vec![mk_entry(&"A".repeat(24), (999, 999, 999), 4096, &"a".repeat(16), Some(PK), u32::MAX)],
            1,
        ),
        (
            "two_nowit",
            vec![
                mk_entry("AAA", (0, 1, 0), 4096, "berryos", Some(PK), 1),
                mk_entry("BBB", (0, 1, 0), 4096, "oem-main", None, 0),
            ],
            9,
        ),
        (
            "pk2",
            vec![mk_entry("EIGHT", (1, 2, 3), 77, "oe-m", Some(PK2), 5)],
            1,
        ),
        ("unsigned_min", vec![mk_entry("NOWIT", (0, 1, 0), 1, "oe-m", None, 2)], 2),
    ];
    for (name, entries, ver) in cases {
        let py_hex = py_build(&entries_expr(&entries), ver, None);
        let got = build_index(&entries, ver, &PK, None).unwrap();
        assert_eq!(hex(&got), py_hex, "oracle 漂移：{name}");
        // round-trip：parse → 再 build 字节恒等（canonical 单形）。
        let idx = parse_index(&got).unwrap();
        let again = build_index(
            &idx.entries,
            idx.index_ver,
            &idx.source_pk,
            if idx.signed { Some(&idx.src_sig) } else { None },
        )
        .unwrap();
        assert_eq!(again, got, "round-trip 非恒等：{name}");
    }
}

#[test]
fn parity_build_index_signed_slot() {
    let synth: [u8; 64] = core::array::from_fn(|i| 0xC0 + i as u8);
    let entries = vec![
        mk_entry("P2APP", (0, 1, 0), 4096, "berryos", Some(PK), 1),
        mk_entry("ZETA", (0, 1, 0), 4096, "berryos", Some(PK), 1),
    ];
    let py_hex = py_build(&entries_expr(&entries), 9, Some(&hex(&synth)));
    let got = build_index(&entries, 9, &PK, Some(&synth)).unwrap();
    assert_eq!(hex(&got), py_hex);
    let idx = parse_index(&got).unwrap();
    assert!(idx.signed && idx.src_sig == synth);
}

fn entries_expr(entries: &[Entry]) -> String {
    let items: Vec<String> = entries
        .iter()
        .map(|e| {
            format!(
                "g.Entry({:?}, ({}, {}, {}), {}, bytes.fromhex('{}'), {:?}, {}, {}, 'active')",
                e.id,
                e.ver.0,
                e.ver.1,
                e.ver.2,
                e.size,
                hex(&e.digest),
                e.lineage,
                e.pk.map(|p| format!("bytes.fromhex('{}')", hex(&p))).unwrap_or_else(|| "None".to_string()),
                e.last_rev
            )
        })
        .collect();
    format!("[{}]", items.join(", "))
}

#[test]
fn parity_keys_listing() {
    for (sid, pk, signed) in [("berryos", &PK, false), ("oe-m", &PK2, true)] {
        let py_hex = py_keys(sid, pk, signed);
        let got = build_keys_listing(sid, pk, signed).unwrap();
        assert_eq!(hex(&got), py_hex, "keys oracle 漂移：{sid}");
        // 回读 + 再发射恒等。
        let (sid2, pk2, signed2) = parse_keys_listing(&got).unwrap();
        assert_eq!((sid2.as_str(), pk2, signed2), (sid, *pk, signed));
        assert_eq!(build_keys_listing(&sid2, &pk2, signed2).unwrap(), got);
    }
}

// ---- ② SHA-512/256 NIST 向量 + Python hashlib 交叉 ----//

#[test]
fn sha512_256_nist_vectors() {
    assert_eq!(
        hex(&sha512_256(b"abc")),
        "53048e2681941ef99b2e29b76b4c7dabe4c2d0c634fc6d46e0e2f13107e7af23"
    );
    assert_eq!(
        hex(&sha512_256(b"")),
        "c672b8d1ef56ed28ab87c3622c5114069bdd3ad7b8f9737498d0c01ecef0967a"
    );
    // 多块长（1000B——8 块消息面）。
    let long: Vec<u8> = (0..1000).map(|i| b'0' + (i % 10) as u8).collect();
    assert_eq!(
        hex(&sha512_256(&long)),
        "016050abbba6f4c134950a88fbe0a6b315500ad4056e95746cf70e789026921a"
    );
}

#[test]
fn sha512_256_python_cross() {
    for msg in [&b""[..], b"abc", b"abcdefgh", &vec![0u8; 111][..], &vec![0xABu8; 129][..]] {
        let code = format!(
            "import hashlib\nprint(hashlib.new('sha512_256', bytes.fromhex('{}')).hexdigest())",
            hex(msg)
        );
        let o = Command::new("python3").arg("-c").arg(&code).output().unwrap();
        let py = String::from_utf8(o.stdout).unwrap().trim().to_string();
        assert_eq!(hex(&sha512_256(msg)), py, "hashlib 交叉漂移 len={}", msg.len());
    }
}

// ---- ③ 拒面全谱（selftest ④ 腿 Rust 镜像 + 补充）----

fn expect_die<T: std::fmt::Debug>(r: R<T>, want: &str) {
    match r {
        Ok(v) => panic!("被放行：{v:?}（应含 {want}）"),
        Err(e) => assert!(
            e.message.contains(want),
            "拒面文案缺 {want:?}：{}",
            e.message
        ),
    }
}

type R<T> = Result<T, Reject>;

#[test]
fn reject_structural_spectrum() {
    let good = build_index(
        &[mk_entry("P2APP", (0, 1, 0), 4096, "berryos", Some(PK), 1), mk_entry("ZETA", (0, 1, 0), 4096, "berryos", Some(PK), 1)],
        9,
        &PK,
        None,
    )
    .unwrap();
    let mangle = |f: &dyn Fn(&mut Vec<u8>)| parse_index(&{
        let mut c = good.clone();
        f(&mut c);
        c
    });
    expect_die(mangle(&|c: &mut Vec<u8>| c[0] = 0x58), "magic");
    expect_die(mangle(&|c: &mut Vec<u8>| c[4] = 2), "保守拒");
    expect_die(mangle(&|c: &mut Vec<u8>| c[5] = 9), "未在册");
    expect_die(mangle(&|c: &mut Vec<u8>| c[6] = 0x02), "保留位");
    expect_die(mangle(&|c: &mut Vec<u8>| c[7] = 1), "reserved");
    expect_die(mangle(&|c: &mut Vec<u8>| c[13] = 3), "越域");
    expect_die(mangle(&|c: &mut Vec<u8>| c.truncate(c.len() - 1)), "总长不符");
    expect_die(mangle(&|c: &mut Vec<u8>| c.extend_from_slice(&[0])), "总长不符");
    // 谎报位/夹带签名。
    let mut lie = good.clone();
    lie[6] = FLAG_SIGNED;
    expect_die(parse_index(&lie), "谎报位");
    let mut stow = good.clone();
    let n = stow.len();
    stow[n - 1] = 0x01;
    expect_die(parse_index(&stow), "单一解释拒");
    // pk_len=31 单一解释拒（变首个条目 pk_len 字节——Python 同位点）。
    let mut pk31 = build_index(&[mk_entry("NOWIT", (0, 1, 0), 4096, "berryos", None, 1)], 1, &PK, None).unwrap();
    pk31[106] = 31;
    expect_die(parse_index(&pk31), "非 0/32");
    // 条目区截断（< INDEX_MIN → 短于最小拒；Python 先咬长度门同形）。
    let mut trunc = good.clone();
    trunc.truncate(46);
    expect_die(parse_index(&trunc), "短于最小");
}

#[test]
fn reject_value_domain() {
    expect_die(
        build_index(&[mk_entry(&"A".repeat(25), (0, 1, 0), 1, "berryos", Some(PK), 1)], 1, &PK, None),
        "越域",
    );
    expect_die(
        build_index(&[mk_entry("bad id", (0, 1, 0), 1, "berryos", Some(PK), 1)], 1, &PK, None),
        "越域",
    );
    expect_die(
        build_index(&[mk_entry("X", (1000, 0, 0), 1, "berryos", Some(PK), 1)], 1, &PK, None),
        "越域",
    );
    expect_die(
        build_index(&[mk_entry("X", (0, 1, 0), 1, &"a".repeat(17), Some(PK), 1)], 1, &PK, None),
        "越域",
    );
    expect_die(
        build_index(&[mk_entry("X", (0, 1, 0), 1, "Oem", Some(PK), 1)], 1, &PK, None),
        "越域",
    );
    expect_die(
        build_index(
            &[mk_entry("P2APP", (0, 1, 0), 4096, "berryos", Some(PK), 1), mk_entry("P2APP", (0, 1, 0), 4096, "berryos", Some(PK), 1)],
            1,
            &PK,
            None,
        ),
        "重复",
    );
    // cert 非 active（build 侧值域放行、parse 侧词表拒——Python 同口径：
    // build_index 不执法 cert 词表，parse_index 执法）。
    let mut e = mk_entry("X", (0, 1, 0), 1, "berryos", Some(PK), 1);
    e.cert = "revoked".to_string();
    let blob = build_index(&[e], 1, &PK, None).unwrap();
    expect_die(parse_index(&blob), "未在册");
}

#[test]
fn deterministic_and_canonical() {
    let a = vec![
        mk_entry("P2APP", (0, 1, 0), 4096, "berryos", Some(PK), 1),
        mk_entry("ZETA", (0, 1, 0), 4096, "berryos", Some(PK), 1),
    ];
    let b = vec![
        mk_entry("ZETA", (0, 1, 0), 4096, "berryos", Some(PK), 1),
        mk_entry("P2APP", (0, 1, 0), 4096, "berryos", Some(PK), 1),
    ];
    assert_eq!(
        build_index(&a, 9, &PK, None).unwrap(),
        build_index(&b, 9, &PK, None).unwrap(),
        "构造确定性（输入序无关——canonical 单形）"
    );
}

#[test]
fn keys_listing_rejects() {
    let golden = build_keys_listing("berryos", &PK, false).unwrap();
    expect_die(parse_keys_listing(&{
        let mut c = golden.clone();
        for i in 0..c.len() {
            if c[i..].starts_with(b"v1 ") {
                c[i + 1] = b'2';
                break;
            }
        }
        c
    }), "头行漂移");
    expect_die(
        parse_keys_listing(&{
            let mut c = golden.clone();
            let fp = hex(&sha512_256(&PK));
            let pos = c.windows(fp.len()).position(|w| w == fp.as_bytes()).unwrap();
            c[pos..pos + 64].copy_from_slice(&[b'0'; 64]);
            c
        }),
        "fingerprint",
    );
    expect_die(build_keys_listing(&"a".repeat(17), &PK, false), "越域");
    // source_id 定宽列漂移（17B 挤进 16 列 → 条目行形拒）。
    expect_die(
        parse_keys_listing(&{
            let c = golden.clone();
            // 把 "berryos          " 16 列换 17B sid——行形漂移拒。
            let pos = c.windows(10).position(|w| w == b"source_id=").unwrap() + 10;
            let mut r = c[..pos].to_vec();
            r.extend_from_slice(b"aaaaaaaaaaaaaaaaa");
            r.extend_from_slice(&c[pos + 16..]);
            r
        }),
        "行形漂移",
    );
}

#[test]
fn parse_eight_fields_roundtrip() {
    let blob = build_index(&[mk_entry("EIGHT", (1, 2, 3), 77, "oe-m", Some(PK2), 5)], 1, &PK, None).unwrap();
    let e = &parse_index(&blob).unwrap().entries[0];
    assert_eq!(
        (e.id.as_str(), e.ver, e.size, e.lineage.as_str(), e.last_rev, e.cert.as_str()),
        ("EIGHT", (1, 2, 3), 77, "oe-m", 5, "active")
    );
    assert_eq!(e.digest, core::array::from_fn(|i| i as u8));
    assert_eq!(e.pk, Some(PK2));
    let blob0 = build_index(&[mk_entry("NOWIT", (0, 1, 0), 1, "berryos", None, 1)], 1, &PK, None).unwrap();
    assert!(parse_index(&blob0).unwrap().entries[0].pk.is_none(), "公钥见证 0B 臂");
}

#[test]
fn unhex_helper_matches() {
    assert_eq!(unhex("53048e26"), vec![0x53, 0x04, 0x8e, 0x26]);
}
