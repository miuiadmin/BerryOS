//! bctsrun 十刀·parity：镜像腿轮询谓词 + banner 门 oracle 对拍。oracle =
//! bash 真跑——`bcts-run.sh` :704-723 轮询三段循环体逐字提取（合成
//! img-serial.log + 合成 entries.tsv 驱动；QEMU/deadline 面不在此刀——
//! 谓词体即「一轮全查」的纯文本判定）+ :730-738 banner/ident 两门
//!（`img_fail` 文案面：echo FAIL 行 + 日志尾部行不在谓词内，只取首行）。
//!
//! 对拍面：`missing` 列表全等（构造序：固定串 → ERE: 前缀 → bare mid）。
//! banner 门另以 bash `grep -m1` + 两 grep 逐行对拍（正面/形态缺/nogit/
//! 无 banner 行四态）。
//!
//! bash 缺席 = skip（parity 先例同款守卫）。段体单源 = 文件定位锚提取，
//! 锚漂移即测试红（八/九刀先例：`sed` 语义从真文件取段，防双写）。

use bctsrun::efiboot::{banner_gate, poll_missing, IMG_CONT_MARKERS, IMG_EFI_FIXED};
use bctsrun::load::LoadOutcome;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn sh_ok() -> bool {
    Command::new("bash")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// oracle：轮询三段循环体（:704-723 逐字；`$WORK/entries.tsv` → 入参路径）。
///
/// 定位锚：`missing=""` 行起至 `sleep 2` 后的 `done` 行止（循环体自身）。
/// 前置 = assert_marker 函数体 + IMG_EFI_FIXED/IMG_EFI_ERE/IMG_CONT_MARKERS
/// 三数组定义（轮询体引用）。
fn oracle_poll_script(work: &str, log: &str) -> String {
    let src = std::fs::read_to_string(repo_root().join("tools/bcts-run.sh")).unwrap();
    let lines: Vec<&str> = src.lines().collect();
    // 三数组定义（:326-338）：IMG_EFI_FIXED 单行起，IMG_EFI_ERE 多行块居中，
    // IMG_CONT_MARKERS 单行收——区间整取（锚 = 两单行定义，ERE 块随之）。
    let fi = lines
        .iter()
        .position(|l| l.starts_with("IMG_EFI_FIXED=("))
        .expect("bcts-run.sh 缺 IMG_EFI_FIXED 定义");
    let cont = lines
        .iter()
        .position(|l| l.starts_with("IMG_CONT_MARKERS=("))
        .expect("bcts-run.sh 缺 IMG_CONT_MARKERS 定义");
    assert!(fi < cont, "数组区间序漂移");
    let arrays = lines[fi..=cont].join("\n");
    assert!(arrays.contains("IMG_EFI_ERE=("), "ERE 块未入区间");
    // 轮询体：`missing=""` 行（4 空格缩进）起至 `sleep 2` 行止（循环体一轮
    // 全查面；QEMU 活性探/deadline 判在段外或参数化——谓词纯面）。
    let ms = lines
        .iter()
        .position(|l| l.trim() == "missing=\"\"")
        .expect("bcts-run.sh 缺轮询体 missing=\"\" 行");
    let de = ms
        + lines[ms..]
            .iter()
            .position(|l| l.trim() == "sleep 2")
            .expect("轮询体缺 sleep 2")
        + 1;
    let body = lines[ms..de].join("\n");
    // assert_marker 函数体（九刀同款提取）。
    let am = lines
        .iter()
        .position(|l| l.starts_with("assert_marker() {"))
        .expect("bcts-run.sh 缺 assert_marker 函数");
    let ame = am
        + lines[am..]
            .iter()
            .position(|l| *l == "}")
            .expect("assert_marker 未闭合")
        + 1;
    let amfn = lines[am..ame].join("\n");
    format!(
        "IMAGE_FILE=\"/img/x.img\"\nIMG_LOG=\"{log}\"\nCORPUS=\"$IMG_LOG\"\nWORK=\"{work}\"\nt0=$(date +%s)\nIMG_DEADLINE=360\nsleep() {{ :; }}\n{arrays}\n{amfn}\nRUN=1\nwhile [ \"$RUN\" = 1 ]; do\n{body}\nRUN=2\ndone\necho \"MISS:$missing\"\nexit 0\n"
    )
}

/// oracle：banner 两门（:730-738 的两 `grep -m1` 判定 + img_fail 首行文案）。
///
/// img_fail 的日志尾部/exit 面不取——只镜像两门的判定与 FAIL 文案行本身
///（`echo \"bcts: FAIL: $1\"`：`$1` 为入参字面）。
fn oracle_banner_script(log: &str) -> String {
    format!(
        "IMG_LOG=\"{log}\"\n\
grep -m1 'BerryOS boot' \"$IMG_LOG\" | grep -Eq 'BerryOS boot [0-9]+\\.[0-9]+\\.[0-9]+ \\([A-Za-z0-9.-]+( rustc[A-Za-z0-9.?]+ (dbg|rel))?\\)' \\\n\
  || {{ echo \"bcts: FAIL: boot banner 形态缺（G-ver-2；正则 = smoke-image.sh 同款）：$(grep -m1 'BerryOS boot' \"$IMG_LOG\" || echo '（日志无 BerryOS boot 行）')\"; exit 1; }}\n\
grep -m1 'BerryOS boot' \"$IMG_LOG\" | grep -q 'nogit' \\\n\
  && {{ echo \"bcts: FAIL: boot banner ident 为 nogit——nogit 标识产物不得进发布链/过认证门（版本管理标准 §6）：$(grep -m1 'BerryOS boot' \"$IMG_LOG\")\"; exit 1; }}\n\
echo OK\nexit 0\n"
    )
}

/// 段体提取形状钉板（防定位锚漂移后取到错段而静默假绿）。
#[test]
fn extracted_segments_have_expected_shape() {
    let src = std::fs::read_to_string(repo_root().join("tools/bcts-run.sh")).unwrap();
    let lines: Vec<&str> = src.lines().collect();
    let fi = lines.iter().position(|l| l.starts_with("IMG_EFI_FIXED=(")).unwrap();
    assert!(lines[fi].contains("BerryBoot EFI skeleton") && lines[fi].contains("fw: EDK II"));
    assert!(lines[fi].contains("BERRYBOOT ok") && lines[fi].trim_end().ends_with(")"), "FIXED 单行定义形：{}", lines[fi]);
    let ms = lines.iter().position(|l| l.trim() == "missing=\"\"").unwrap();
    let poll_from: Vec<&str> = lines[ms..].iter().take(24).copied().collect();
    assert!(poll_from.iter().any(|l| l.contains("\"${IMG_EFI_FIXED[@]}\"") && l.contains("\"${IMG_CONT_MARKERS[@]}\"")));
    assert!(poll_from.iter().any(|l| l.contains("\"${IMG_EFI_ERE[@]}\"")));
    let _ = oracle_poll_script("/nonexistent-work", "/nonexistent-log"); // 锚完整性自证
    let _ = oracle_banner_script("/nonexistent-log");
}

fn make_lo(rows: &[(&str, &str, &str, &str)]) -> LoadOutcome {
    LoadOutcome {
        entries: rows
            .iter()
            .map(|(t, p, m, e)| format!("{t}\t{p}\t{m}\t{e}"))
            .collect(),
        n_markers: 91,
        n_gates: 0,
        floor: 91,
        n_bare: 32,
        img_pr: None,
        img_pr_anchor: None,
        pre_stdout: vec![],
    }
}

/// 双跑对拍：bash 轮询体 vs 镜像 poll_missing。
fn parity(rows: &[(&str, &str, &str, &str)], log: &str, tag: &str) -> String {
    let root = repo_root();
    let work = root.join(format!("target/bctsrun-parity-efiboot-{tag}"));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let tsv = work.join("entries.tsv");
    std::fs::write(
        &tsv,
        rows.iter().map(|r| format!("{}\t{}\t{}\t{}\n", r.0, r.1, r.2, r.3)).collect::<String>(),
    )
    .unwrap();
    let log_path = work.join("img-serial.log");
    std::fs::write(&log_path, log).unwrap();

    let script = oracle_poll_script(work.to_str().unwrap(), log_path.to_str().unwrap())
        .replace("done < \"$WORK/entries.tsv\"", &format!("done < \"{}\"", tsv.to_str().unwrap()));
    let o = Command::new("bash")
        .arg("-c")
        .arg(&script)
        .current_dir(&root)
        .output()
        .expect("spawn bash oracle");
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(
        o.status.success(),
        "bash oracle 非零退（{tag}）：rc={:?} stderr={err}",
        o.status.code()
    );
    let miss_line = out
        .lines()
        .rev()
        .find(|l| l.starts_with("MISS:"))
        .expect("oracle 缺 MISS: 行");
    let b_missing = miss_line.trim_start_matches("MISS:").to_string();

    let m = poll_missing(&make_lo(rows), log);
    assert_eq!(
        m, b_missing,
        "missing 不等（{tag}）：bash={b_missing:?} rust={m:?}"
    );
    let _ = std::fs::remove_dir_all(&work);
    m
}

/// banner 门对拍：bash 两门 vs 镜像 banner_gate。
fn parity_banner(log: &str, tag: &str) {
    let root = repo_root();
    let work = root.join(format!("target/bctsrun-parity-banner-{tag}"));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let log_path = work.join("img-serial.log");
    std::fs::write(&log_path, log).unwrap();

    let script = oracle_banner_script(log_path.to_str().unwrap());
    let o = Command::new("bash")
        .arg("-c")
        .arg(&script)
        .current_dir(&root)
        .output()
        .expect("spawn bash oracle");
    let b_stdout = String::from_utf8_lossy(&o.stdout).to_string();
    let b_ok = o.status.success();
    let r = banner_gate(log);
    let (r_ok, r_msg) = match &r {
        Ok(()) => (true, String::new()),
        Err(e) => (false, e.clone()),
    };
    // bash 侧 FAIL 文案行 = stdout 首行（img_fail 的 echo 面保真；exit 1）。
    let b_msg = if b_ok {
        String::new()
    } else {
        b_stdout.lines().next().unwrap_or("").to_string()
    };
    assert_eq!(r_ok, b_ok, "判定不等（{tag}）：bash={b_ok} rust={r_ok}");
    if !b_ok {
        assert_eq!(r_msg, b_msg, "FAIL 文案不等（{tag}）：bash={b_msg:?} rust={r_msg:?}");
    }
    let _ = std::fs::remove_dir_all(&work);
}

/// 合成满配日志（EFI 十段 + 接续 + banner + KVA）。
fn full_log() -> String {
    let mut s = String::new();
    for m in IMG_EFI_FIXED {
        s.push_str(m);
        s.push('\n');
    }
    s.push_str("gop: 1280x800\nkfile: 2048B ok\ninit: 4096B ok\n");
    s.push_str("elf: segs=4 entry=0x48020000 ok\n");
    s.push_str("load: segs=4 pages=512 ok\nmmap: n=7 ok\n");
    s.push_str("KVA ok\n");
    for m in IMG_CONT_MARKERS {
        s.push_str(m);
        s.push_str("5\n");
    }
    s.push_str("BerryOS boot 0.9.7 (abc1234 rustc1.99.0 rel)\n");
    s
}

const BARE_ROWS: &[(&str, &str, &str, &str)] = &[
    ("MARK", "bare", "bare-01", "BerryOS boot"),
    ("MARK", "bare", "bare-02", "KVA ok"),
    ("MARK", "bare", "bare-03", "DEMO ok"),
    ("MARK", "bare", "bare-04", "TAB ok"),
    ("MARK", "rootfs", "rf-01", "ZZZ structurally absent"),
    ("MARK", "rx", "rx-01", "YYY absent"),
    ("GATE", "abi-check", "just abi-check", ""),
];

#[test]
fn parity_full_log_all_present() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    parity(BARE_ROWS, &full_log(), "full");
}

#[test]
fn parity_empty_log_all_missing_join_order() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    // 空日志：全缺。构造序 = 固定 7 串 → ERE 6 条 → bare mid。
    let m = parity(BARE_ROWS, "", "empty");
    assert!(m.starts_with("BerryBoot EFI skeleton fw: EDK II"), "首段固定串序：{m}");
    assert!(m.contains("ERE:gop: [0-9]+x[0-9]+"), "ERE 前缀：{m}");
    assert!(m.ends_with("bare-04"), "尾段 bare mid（rootfs/rx/GATE 不进面）：{m}");
}

#[test]
fn parity_partial_absences() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    // 抹 gop 行 + banner 行 + DEMO ok（bare-03 面）→ 三类缺项并存。
    let log: String = full_log()
        .lines()
        .filter(|l| !l.starts_with("gop:") && !l.contains("DEMO ok"))
        .map(|l| format!("{l}\n"))
        .collect();
    let m = parity(BARE_ROWS, &log, "partial");
    assert!(m.contains("ERE:gop: [0-9]+x[0-9]+"), "{m}");
    assert!(m.contains("bare-03"), "{m}");
    // banner 行未抹：bare-01（BerryOS boot）仍在席——只 gop/DEMO 面。
    assert!(!m.contains("bare-01"), "{m}");
    assert!(m.contains("bare-04"), "{m}");
}

#[test]
fn parity_idemo_prefix_false_positive_guard() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    // IDEMO ok 不算 DEMO ok 命中（前邻 I 守卫经轮询体透传）。
    let mut log = String::new();
    for m in IMG_EFI_FIXED {
        log.push_str(m);
        log.push('\n');
    }
    log.push_str("gop: 1x1\nkfile: 1B ok\ninit: 1B ok\n");
    log.push_str("elf: segs=1 entry=0xf ok\nload: segs=1 pages=1 ok\nmmap: n=1 ok\n");
    log.push_str("IDEMO ok\n");
    for m in IMG_CONT_MARKERS {
        log.push_str(m);
        log.push_str("9\n");
    }
    log.push_str("BerryOS boot 1.2.3 (x)\n");
    let m = parity(BARE_ROWS, &log, "idemo");
    assert_eq!(m, "bare-02 bare-03 bare-04", "IDEMO 守卫 + TAB/KVA 缺：{m}");
}

#[test]
fn parity_banner_gate_matrix() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    parity_banner(&full_log(), "banner-ok");
    parity_banner("", "banner-empty");
    parity_banner("BerryOS boot 1.2.3 (abc)\n", "banner-min");
    parity_banner("BerryOS boot 1.2 (abc)\n", "banner-2seg");
    parity_banner("BerryOS boot 1.2.3 ()\n", "banner-empty-ident");
    parity_banner("BerryOS boot 1.2.3 (nogit)\n", "banner-nogit");
    parity_banner("BerryOS boot 1.2.3 (abc) tail nogit\n", "banner-tail-nogit");
    parity_banner("pre\nBerryOS boot 2.0.0 (g7f8b2c1 rustc1.84.1 dbg)\npost\n", "banner-mid");
}

#[test]
fn parity_banner_ere_corpus_matrix() {
    // banner 形态正则边界对拍（bash grep -Eq 真跑 vs 手写镜像，不依赖轮询体）。
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let lines = [
        "BerryOS boot 1.2.3 (abc)",
        "BerryOS boot 1.2.3 (a.b-c_d)",
        "BerryOS boot 1.2.3 (abc rustc1.99.0 rel)",
        "BerryOS boot 1.2.3 (abc rustc1.99.0 dbg)",
        "BerryOS boot 1.2.3 (abc rustc1.99.0? rel)",
        "BerryOS boot 1.2.3 (rustc1.0 rel)",
        "BerryOS boot 1.2.3 ()",
        "BerryOS boot 1.2.3 abc",
        "BerryOS boot 1.2.3 (abc rustc1.0 rel extra)",
        "BerryOS boot 01.02.03 (abc)",
        "x BerryOS boot 1.2.3 (abc) y",
        "BerryOS boot 1.2.3 (abc) nogit",
        "BerryOS boot 1.2.3a (abc)",
    ];
    let root = repo_root();
    let work = root.join("target/bctsrun-parity-banner-matrix");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    for (i, l) in lines.iter().enumerate() {
        let p = work.join(format!("m{i}.log"));
        std::fs::write(&p, format!("{l}\n")).unwrap();
        // bash：grep -Eq 单行真跑。
        let script = format!(
            "grep -Eq 'BerryOS boot [0-9]+\\.[0-9]+\\.[0-9]+ \\([A-Za-z0-9.-]+( rustc[A-Za-z0-9.?]+ (dbg|rel))?\\)' \"$1\" && echo Y || echo N",
        );
        let o = Command::new("bash")
            .arg("-c")
            .arg(&script)
            .arg("bash")
            .arg(p.to_str().unwrap())
            .output()
            .unwrap();
        let b = String::from_utf8_lossy(&o.stdout).trim() == "Y";
        let r = banner_gate(&format!("{l}\n"));
        // 形态面单独比（banner_gate 含 nogit 门——nogit 样本只验形态过）。
        let r_shape = r.is_ok() || matches!(&r, Err(e) if !e.contains("形态缺"));
        assert_eq!(r_shape, b, "banner 形态不等：{l:?} bash={b} rust={r_shape}");
    }
    let _ = std::fs::remove_dir_all(&work);
}
