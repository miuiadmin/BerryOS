//! berryassemble 首刀·oracle 对拍：Python 现役件（tools/berry_assemble.py）
//! 真跑（import + `python3 -c` 直驱函数面），四个纯面双侧同构：
//!
//! ①serialize_boot_params 字节恒等（SimpleNamespace 直驱——同清单同字节）；
//! ②validate_platform_revision 拒面文案全等（域界/反回滚两臂 + 过面值）；
//! ③kernel 锚 ANCHOR_RE fullmatch 对拍（accept 梯 + 拒梯形）；
//! ④platform level 真源单拍（tools/abi.toml 现值——Rust 读器 vs Python
//! `_platform_level()`，含坏源拒面文案）。
//!
//! Python 缺席/版本面炸 = skip（gensourceindex parity 同款守卫）。

use berryassemble::*;
use std::process::Command;

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Python 现役件直驱（berry_assemble import 面）。stdout 尾行 = 结果；
/// Reject 走 stderr（前置标记行 REJ:）。
fn py_run(code: &str) -> (String, String, Option<i32>) {
    let o = Command::new("python3")
        .arg("-c")
        .arg(code)
        .current_dir(repo_root())
        .output()
        .expect("spawn python3");
    (
        String::from_utf8_lossy(&o.stdout).trim().to_string(),
        String::from_utf8_lossy(&o.stderr).trim().to_string(),
        o.status.code(),
    )
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Python serialize_boot_params 直驱（params 字典字面量 → hex）。
fn py_serialize(params_expr: &str) -> (String, bool) {
    let (out, err, _) = py_run(&format!(
        "import sys; sys.path.insert(0, 'tools')\nfrom types import SimpleNamespace\nimport berry_assemble as b\nprint(b.serialize_boot_params(SimpleNamespace(params={params_expr})).hex())"
    ));
    (out, err.is_empty())
}

fn py_serialize_ok(params_expr: &str) -> String {
    let (out, ok) = py_serialize(params_expr);
    assert!(ok, "python oracle 失败：{out}");
    out
}

#[test]
fn parity_serialize_boot_params() {
    // 三键全席（acme 演练参照物同款清单）
    let mut p = check_boot_params(&map(&[
        ("sched_slice_hz", val_i(64)),
        ("intent_default", val_s("interactive")),
        ("timer_slack", val_i(1_250_000)),
    ]))
    .unwrap();
    assert_eq!(
        hex(&serialize_boot_params(&p)),
        py_serialize_ok(
            "{'sched_slice_hz': 64, 'intent_default': 'interactive', 'timer_slack': 1250000}"
        )
    );
    // 单键 u32 + 字典序（intent 先于 sched_）
    p.retain(|k, _| k != "timer_slack");
    assert_eq!(
        hex(&serialize_boot_params(&p)),
        py_serialize_ok("{'sched_slice_hz': 64, 'intent_default': 'interactive'}")
    );
    // 恰界值：1024 / 999_999_999
    let q = check_boot_params(&map(&[
        ("sched_slice_hz", val_i(1024)),
        ("timer_slack", val_i(999_999_999)),
    ]))
    .unwrap();
    assert_eq!(
        hex(&serialize_boot_params(&q)),
        py_serialize_ok("{'sched_slice_hz': 1024, 'timer_slack': 999999999}")
    );
    // 空 params = 纯 schema 头行（内核消费端空清单同字节）
    let empty = std::collections::BTreeMap::new();
    assert_eq!(hex(&serialize_boot_params(&empty)), py_serialize_ok("{}"));
}

fn map(kv: &[(&str, toml_val::Val)]) -> std::collections::BTreeMap<String, toml_val::Val> {
    kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn val_i(n: i64) -> toml_val::Val {
    toml_val::Val::Int(n)
}

fn val_s(s: &str) -> toml_val::Val {
    toml_val::Val::Str(s.to_string())
}

#[test]
fn parity_validate_platform_revision_rejects() {
    // Python 直驱两臂拒面（level 真源现值注入双侧同读）
    let cases: Vec<(String, i64)> = vec![
        ("域界上".into(), 4294967296),
        ("域界下".into(), -1),
        ("反回滚超前".into(), 1 << 40),
    ];
    let level = platform_level_of(&std::fs::read_to_string(repo_root().join("tools/abi.toml")).unwrap()).unwrap();
    for (name, pr) in cases {
        let rust = validate_platform_revision(pr, level).err().map(|r| r.0);
        let (out, err, _) = py_run(&format!(
            "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\ntry:\n    b.validate_platform_revision({pr})\n    print('OK')\nexcept b.Reject as e:\n    print('REJ:', e)"
        ));
        let py_rej = out.strip_prefix("REJ: ").map(|s| s.to_string());
        assert!(py_rej.is_some(), "{name}: python 未拒（{out} {err}）");
        match rust {
            Some(r) => assert_eq!(r, py_rej.unwrap(), "{name}: 拒面文案漂移"),
            None => panic!("{name}: rust 未拒（python 拒了）"),
        }
    }
    // 过面：0 与 level 等值（等值 = 合法旧产品装新平台）
    assert_eq!(validate_platform_revision(0, level).unwrap(), 0);
    let (out, _, _) = py_run(
        "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\nprint(b.validate_platform_revision(0))",
    );
    assert_eq!(out, "0");
}

#[test]
fn parity_kernel_anchor_regex() {
    // Python ANCHOR_RE.fullmatch 对拍：accept 梯 + 拒梯
    let accepts = [
        format!("K@sha256:{}", "a".repeat(64)),
        format!("KERNEL.ELF@sha256:{}", "A19b".repeat(16)),
        format!("k-9._@sha256:{}", "0".repeat(64)),
    ];
    let rejects = [
        ".K@sha256:aaaa".to_string(),
        format!("K@sha256:{}", "g".repeat(64)),
        format!("K@sha256:{}", "a".repeat(63)),
        format!("K@sha256:{}", "a".repeat(65)),
        "K@md5:aaaa".to_string(),
        format!("a@b@sha256:{}", "a".repeat(64)),
        format!("K@sha256:{}/", "a".repeat(63)),
    ];
    for a in accepts {
        assert!(parse_kernel_anchor(&a).is_ok(), "rust 拒了 accept 形 {a}");
        let (out, _, _) = py_run(&format!(
            "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\nprint(bool(b.ANCHOR_RE.fullmatch({a:?})))"
        ));
        assert_eq!(out, "True", "python 拒了 {a}");
    }
    for r in rejects {
        assert!(parse_kernel_anchor(&r).is_err(), "rust 过了 reject 形 {r}");
        let (out, _, _) = py_run(&format!(
            "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\nprint(bool(b.ANCHOR_RE.fullmatch({r:?})))"
        ));
        assert_eq!(out, "False", "python 过了 {r}");
    }
}

#[test]
fn parity_platform_level_true_source() {
    // 真源单拍：tools/abi.toml 现值双侧同读
    let text = std::fs::read_to_string(repo_root().join("tools/abi.toml")).unwrap();
    let rust = platform_level_of(&text).unwrap();
    let (out, err, _) = py_run(
        "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\nprint(b._platform_level())",
    );
    assert!(err.is_empty(), "python _platform_level 失败：{err}");
    assert_eq!(rust.to_string(), out);
    // 坏源拒面分支对拍（合法 TOML 非整型值 "7"——双侧同咬「非整数」臂；
    // 文案 repr 形不同源〔Rust raw reader 保留引号 vs Python tomllib 剥壳〕，
    // 分支标识全等 + 双侧 loud 拒即钉板，逐字文案归后续 CLI 刀统一）
    let rust_bad = platform_level_of("[platform]\nlevel = \"7\"\n").err().unwrap().0;
    assert!(rust_bad.contains("非整数"), "rust 分支漂移：{rust_bad}");
    let (out, _, _) = py_run(
        "import sys, tempfile, os; sys.path.insert(0, 'tools')\nimport berry_assemble as b\nb.ROOT = tempfile.mkdtemp()\nopen(os.path.join(b.ROOT, 'abi.toml'), 'w').write('[platform]\\nlevel = \"7\"\\n')\ntry:\n    b._platform_level()\nexcept b.Reject as e:\n    print('REJ:', e)",
    );
    let py_bad = out.strip_prefix("REJ: ").unwrap().to_string();
    assert!(py_bad.contains("非整数"), "python 分支漂移：{py_bad}");
}

/// toml_doc 解析 ↔ tomllib 展平对拍：`k = repr` 逐行全等（Python repr
/// 形镜像——串单引号、bool 大写、数组逗号后空格）。语料经 hex 传载防
/// 引号面互扰（python bytes.fromhex 解回）。tomllib 拒 → Ok(None)
/// （分支面：tomllib 拒的形本子集必拒）。
fn py_tomllib_flatten(text: &str) -> Option<Vec<String>> {
    let hex = text.bytes().map(|b| format!("{b:02x}")).collect::<String>();
    let py = format!(
        "import tomllib\n\
         raw = bytes.fromhex('{hex}').decode()\n\
         try:\n\
         \x20   doc = tomllib.loads(raw)\n\
         except Exception:\n\
         \x20   print('PYREJ')\n\
         \x20   raise SystemExit(0)\n\
         def _flat(d, prefix, out):\n\
         \x20   for k, v in d.items():\n\
         \x20       key = f'{{prefix}}.{{k}}' if prefix else k\n\
         \x20       if isinstance(v, dict):\n\
         \x20           _flat(v, key, out)\n\
         \x20       else:\n\
         \x20           out.append(f'{{key}} = {{v!r}}')\n\
         out = []\n\
         _flat(doc, '', out)\n\
         print('\\n'.join(out))"
    );
    let (out, err, _) = py_run(&py);
    if !err.is_empty() {
        panic!("python oracle 崩（非拒面）：{err}");
    }
    if out == "PYREJ" {
        return None;
    }
    Some(out.split('\n').map(|s| s.to_string()).collect())
}

#[test]
fn parity_toml_doc_flatten() {
    // 语料 1：acme 参照物真源（现役语面全集）
    let acme = std::fs::read_to_string(repo_root().join("tools/drill/acme/product.toml")).unwrap();
    // 语料 2：SELFTEST_PRODUCT 模板（berry_assemble.py 内嵌真源）
    let (tpl, tpl_err, _) = py_run(
        "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\nprint(b.SELFTEST_PRODUCT, end='')",
    );
    assert!(tpl_err.is_empty(), "SELFTEST_PRODUCT 提取失败：{tpl_err}");
    // 语料 3：合成面（引号键/负 int/串内 # 与 ,/bool 两态/空行注释）
    let synth = "[t]\n\"app/A.bap\" = \"userbins/app/A.bap\"\nneg = -7\ns = \"a#b,c]d\"\nb1 = true\nb2 = false\nai = [1, -2]\n\n# 注\nc = 2 # 尾\n";
    for (name, text) in [("acme", acme), ("selftest-tpl", tpl), ("synth", synth.to_string())] {
        let rust = toml_doc::flatten(&toml_doc::parse(&text).unwrap());
        let py = py_tomllib_flatten(&text)
            .unwrap_or_else(|| panic!("{name}: python tomllib 拒了语料（分支面漂移）"));
        assert_eq!(
            rust, py,
            "{name}: flatten 对拍漂移\nrust: {rust:#?}\npy:   {py:#?}"
        );
    }
    // 拒面分支：tomllib 拒的形本子集必拒（重复键/未闭合/重复节头）
    for bad in [
        "[a]\nk = 1\nk = 2\n",
        "[a]\nk = \"x\n",
        "[a]\nk = 1\n[a]\nj = 2\n",
    ] {
        assert!(toml_doc::parse(bad).is_err(), "rust 未拒：{bad}");
        assert!(
            py_tomllib_flatten(bad).is_none(),
            "python tomllib 未拒（分支面漂移）：{bad}"
        );
    }
}

/// Python `load_product(lite=True)` 直驱（语料 hex 传载防引号互扰）：
/// 返回 (拒面文案 or OK, exit)。写临时文件真跑文件面入口。
fn py_load_lite(text: &str) -> (String, bool) {
    #[allow(unused_variables)]
    let h = text.bytes().map(|b| format!("{b:02x}")).collect::<String>();
    let code = format!(
        "import sys, tempfile, os\n\
         sys.path.insert(0, 'tools')\n\
         import berry_assemble as b\n\
         raw = bytes.fromhex('{h}').decode()\n\
         d = tempfile.mkdtemp()\n\
         p = os.path.join(d, 'product.toml')\n\
         open(p, 'w').write(raw)\n\
         try:\n\
         \x20   prod = b.load_product(p, lite=True)\n\
         \x20   print('OK', prod.name, prod.vendor, prod.board, ','.join(prod.preset), prod.launcher)\n\
         except b.Reject as e:\n\
         \x20   print('REJ:', e)"
    );
    let (out, err, code) = py_run(&code);
    assert!(err.is_empty(), "python oracle 崩（非拒面）：{err}");
    (out, code == Some(0))
}

/// 拒面文案全等对拍（rust 拒 ⇔ python 拒且逐字同文案）。
fn assert_reject_both(name: &str, text: &str) {
    let rust = lite::load_lite_product(text).err().map(|r| r.0);
    let (py_out, _) = py_load_lite(text);
    let py_rej = py_out.strip_prefix("REJ: ").map(|s| s.to_string());
    match (rust, py_rej) {
        (Some(r), Some(p)) => assert_eq!(r, p, "{name}: 拒面文案漂移"),
        (None, None) => panic!("{name}: 双侧都过了（语料应拒）"),
        (r, p) => panic!("{name}: 分支面漂移 rust={r:?} py={p:?}"),
    }
}

#[test]
fn parity_load_lite_product() {
    // 语料 1：acme 参照物（现役语面——branding 注释化位不在场）
    let acme = std::fs::read_to_string(repo_root().join("tools/drill/acme/product.toml")).unwrap();
    let p = lite::load_lite_product(&acme).unwrap();
    assert_eq!(p.name, "acme-rich");
    assert_eq!(p.vendor, "acme");
    assert_eq!(p.priv_grants.get("SETTINGS").unwrap(), &vec!["console".to_string(), "fs".to_string()]);
    let (py_out, ok) = py_load_lite(&acme);
    assert!(ok, "acme: python 未过：{py_out}");
    assert_eq!(py_out, "OK acme-rich acme qemu-virt app/SETTINGS.bap,app/FILES.bap,app/PHOTOS.bap,app/CAMERA.bap,app/BROWSER.bap bin/berry-launcher");

    // 语料 2：SELFTEST_PRODUCT 模板（branding 在场 + priv_grants 双 app 面）
    let (tpl, err, _) = py_run(
        "import sys; sys.path.insert(0, 'tools')\nimport berry_assemble as b\nprint(b.SELFTEST_PRODUCT, end='')",
    );
    assert!(err.is_empty());
    let p = lite::load_lite_product(&tpl).unwrap();
    assert_eq!(p.branding.get("wallpaper").unwrap(), "assets/wallpaper.byi");
    assert_eq!(p.grants_issued_by, "oem-main");
    let (py_out, ok) = py_load_lite(&tpl);
    assert!(ok, "tpl: python 未过：{py_out}");
    assert_eq!(py_out, "OK sample-rich berryos qemu-virt app/A.bap bin/berry-launcher");

    // 语料 3：全拒面谱（文案即契约——双侧逐字对拍）
    for (name, text) in [
        ("未知顶层键", "schema = 1\nextra = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("schema 缺", "[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("schema 非 1", "schema = 2\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("schema 串型", "schema = \"1\"\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("缺 product 节", "schema = 1\n[platform]\narch = \"x86_64\"\n"),
        ("product 非表", "schema = 1\nproduct = 1\n[platform]\narch = \"x86_64\"\n"),
        ("name 缺", "schema = 1\n[product]\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("name 非 str", "schema = 1\n[product]\nname = 7\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("name 超长", &format!("schema = 1\n[product]\nname = \"{}\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n", "a".repeat(25))),
        ("name 字符集", "schema = 1\n[product]\nname = \"_bad\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("vendor 超长", &format!("schema = 1\n[product]\nname = \"p\"\nvendor = \"{}\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n", "v".repeat(17))),
        ("vendor 大写", "schema = 1\n[product]\nname = \"p\"\nvendor = \"Oem\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("board 词表外", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"rpi4\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("board 空串", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("缺 apps 节", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("apps 未知键", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\nextra = 1\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("preset 非 arr", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = \"app/A.bap\"\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("preset 元素非 str", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [7]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("preset 非 app/ 前缀", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"bin/sh\"]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("preset 重复", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\", \"app/A.bap\"]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("preset 越 APP_SLOTS 帽", &format!("schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [{}]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n", (0..11).map(|i| format!("\"app/I{i}.bap\"")).collect::<Vec<_>>().join(", "))),
        ("override 非 preset id", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\"]\n[product.apps.override]\nB = \"baps/B.bap\"\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("override 值非 str", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\"]\n[product.apps.override]\nA = 7\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("priv_grants 非 table", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\"]\npriv_grants = 1\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("issued_by 撞保留名", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\"]\n[product.apps.priv_grants]\nissued_by = [\"x\"]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("issued_by 缺", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\"]\n[product.apps.priv_grants]\nA = [\"console\"]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("issued_by 值域外", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\"]\n[product.apps.priv_grants]\nissued_by = \"OEM_X\"\nA = [\"console\"]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("issued_by 超长", &format!("schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\"]\n[product.apps.priv_grants]\nissued_by = \"{}\"\nA = [\"console\"]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n", "i".repeat(17))),
        ("grants 非 preset id", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\"]\n[product.apps.priv_grants]\nissued_by = \"o\"\nB = [\"console\"]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("grants 值非 arr", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\"]\n[product.apps.priv_grants]\nissued_by = \"o\"\nA = \"console\"\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("能力键非 str", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\"]\n[product.apps.priv_grants]\nissued_by = \"o\"\nA = [7]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("未知能力键", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\"]\n[product.apps.priv_grants]\nissued_by = \"o\"\nA = [\"gpu\"]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("能力键重复", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = [\"app/A.bap\"]\n[product.apps.priv_grants]\nissued_by = \"o\"\nA = [\"console\", \"console\"]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("branding 非 table", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\nbranding = 1\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("branding 未知键", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.branding]\ntheme = \"dark\"\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("branding 值空", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.branding]\nwallpaper = \"\"\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("source 非 table", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\nsource = 1\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("source.instance 空", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.source]\ninstance = \"\"\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("trust 缺 pk_anchor", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.trust]\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("trust pk_anchor 空", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.trust]\npk_anchor = \"\"\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("缺 defaults 节", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[platform]\narch = \"x86_64\"\n"),
        ("launcher 缺", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\n[platform]\narch = \"x86_64\"\n"),
        ("launcher 前缀", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"usr/bin/sh\"\n[platform]\narch = \"x86_64\"\n"),
        ("缺 platform 节", "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n"),
    ] {
        assert_reject_both(name, text);
    }

    // 坏 TOML：双侧 loud 拒 + 分支标识对齐（解析器文案不同源——首刀坏源
    // 先例：tomllib 自带文案 vs 本子集解析器文案，逐字统一归 CLI 刀）。
    {
        let text = "[product\nname = \"p\"\n";
        assert!(lite::load_lite_product(text).is_err(), "rust 未拒坏 TOML");
        // Python 拒面文案 = 文件路径前缀 + tomllib 自带英文细节（不同源——
        // 首刀先例：分支标识「TOML 解析失败」对齐即钉板）。oracle 捕
        // Reject 后打印再退 0——拒态以 REJ: 前缀判，非退出码。
        let (py_out, _) = py_load_lite(text);
        assert!(py_out.starts_with("REJ: ") && py_out.contains("TOML 解析失败"),
            "python 分支漂移：{py_out}");
    }

    // 边界对齐钉板（双侧分支形对账，非文案面）
    // app_id 盲剥律：Python e[4:-4] 字符索引——非 ASCII 不 panic、恰 9 字符得空
    // app_id 盲剥律经 lib 内单测钉板（lite_product_law——parity 面走 load 主面）
    // preset 空 + override 全空 = 过面（v0 空 preset 合法）
    let empty_preset = "schema = 1\n[product]\nname = \"p\"\nvendor = \"o\"\nboard = \"qemu-virt\"\n[product.apps]\npreset = []\n[product.defaults]\nlauncher = \"bin/sh\"\n[platform]\narch = \"x86_64\"\n";
    let (py_out, ok) = py_load_lite(empty_preset);
    assert!(ok, "空 preset: python 未过：{py_out}");
    assert!(lite::load_lite_product(empty_preset).is_ok());
}
