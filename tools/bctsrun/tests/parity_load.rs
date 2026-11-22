//! bctsrun 七刀·parity：装载/守卫段对拍。oracle = `tools/bcts-run.sh`
//! 真跑子进程（--dry 两态 + 镜像腿 + 拒面族）——stdout 逐行全等 + 退码
//! 全等，Rust 侧走 `resolve_args → load_entries → dry_echo` 同链。
//!
//! 面域：live/frozen 两态装载 + 步②空集守卫（合成 floor 溢出件）+
//! 镜像腿（bare 守卫过 + manifest 在席拒 + live/frozen 双锚对账 ok +
//! PR 失配拒 + --manifest-name 联动）+ dry 回显全行（含装载期 ok 行）。
//!
//! bash 缺席 = skip（parity 先例同款守卫）。

use bctsrun::{dry_echo, load_entries, resolve_args, LoadOutcome, Mode, Resolved};
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

/// bash 真跑：返回 (rc, stdout 逐行)。
fn run_bash(args: &[&str], frozen_env: Option<&str>) -> (i32, Vec<String>) {
    let root = repo_root();
    let mut cmd = Command::new(root.join("tools/bcts-run.sh").as_os_str());
    cmd.args(args).current_dir(&root).env_remove("BCTS_FROZEN");
    if let Some(f) = frozen_env {
        cmd.env("BCTS_FROZEN", f);
    }
    let o = cmd.output().expect("spawn bcts-run.sh");
    let out = String::from_utf8_lossy(&o.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect();
    (o.status.code().unwrap_or(-1), out)
}

/// Rust 侧全链：argv → resolve_args → load_entries → dry_echo。
fn run_rust(args: &[&str], frozen_env: Option<&str>) -> (i32, Vec<String>, String) {
    let root = repo_root();
    let repo = root.to_string_lossy().into_owned();
    let argv: Vec<String> = std::iter::once("bcts-run.sh".to_string())
        .chain(args.iter().map(|s| s.to_string()))
        .collect();
    match resolve_args(&argv, &repo, frozen_env) {
        Err((msg, code)) => (code, Vec::new(), msg),
        Ok(r) => match load_entries(&r, &repo) {
            Err((msg, code)) => (code, Vec::new(), msg),
            Ok(o) => (0, dry_echo(&r, &o), String::new()),
        },
    }
}

/// 剥路径段（仓根前缀随 canonicalize 浮动——对拍只比词面尾部）。
fn strip_repo(l: &str, repo: &str) -> String {
    l.replace(&format!("{repo}/"), "")
}

#[test]
fn parity_live_dry() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let root = repo_root();
    let repo = root.to_string_lossy().into_owned();
    let (brc, bout) = run_bash(&["--dry"], None);
    assert_eq!(brc, 0);
    let (rrc, rout, _) = run_rust(&["--dry"], None);
    assert_eq!(rrc, 0);
    assert_eq!(bout.len(), rout.len(), "行数不等：\nbash={bout:?}\nrust={rout:?}");
    for (b, r) in bout.iter().zip(rout.iter()) {
        assert_eq!(strip_repo(b, &repo), strip_repo(r, &repo), "行不等：bash={b:?} rust={r:?}");
    }
    // 形锚：live 模式行 + 91 件 + GATE 行 + dry 完行。
    assert!(rout[0].starts_with("bcts: 模式 = live；清单 = "));
    assert!(rout.iter().any(|l| l.contains("标记 91 件（≥ tier1_floor 91）")));
    assert!(rout.iter().any(|l| l == "  marker bare      bare-01    BerryOS boot"));
    assert!(rout.iter().any(|l| l == "  gate   abi-check  just abi-check"));
    assert_eq!(rout.last().unwrap(), "bcts: dry 完（判定式 = 观测标记 ⊇ 清单全集；多打不算 fail）");
}

#[test]
fn parity_frozen_dry_all_t1() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let root = repo_root();
    let repo = root.to_string_lossy().into_owned();
    for n in 0..=6 {
        let f = format!("tools/bcts/frozen/bcts@{n}.toml");
        let (brc, bout) = run_bash(&["--dry"], Some(&f));
        assert_eq!(brc, 0, "{f} bash 应绿");
        let (rrc, rout, _) = run_rust(&["--dry"], Some(&f));
        assert_eq!(rrc, 0, "{f} rust 应绿");
        assert_eq!(bout.len(), rout.len(), "{f} 行数不等");
        for (b, r) in bout.iter().zip(rout.iter()) {
            assert_eq!(
                strip_repo(b, &repo),
                strip_repo(r, &repo),
                "{f} 行不等：bash={b:?} rust={r:?}"
            );
        }
        assert!(rout[0].contains(&format!("bcts@{n}.toml")));
        // platform_revision 行只在 frozen 态发射。
        assert!(rout.iter().any(|l| l.starts_with("bcts: platform_revision = ")));
    }
}

#[test]
fn parity_floor_guard_reject() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let root = repo_root();
    let tmp = root.join("target/bctsrun-parity-load");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    // 合成 floor 溢出件：真件 tier1_floor 91 → 999（92 行 < 999 触发步②守卫）。
    let src = std::fs::read_to_string(root.join("tools/bcts/frozen/bcts@6.toml")).unwrap();
    let bad = src.replace("tier1_floor = 91", "tier1_floor = 999");
    assert_ne!(bad, src, "替换面应命中");
    let badp = tmp.join("bcts@6.toml");
    std::fs::write(&badp, bad).unwrap();
    let bad_str = badp.to_string_lossy().into_owned();
    // frozen 态 floor 守卫在步① parse（t1_check）REJECT 面先发（退 2）——
    // 步② FAIL 守卫对 frozen 不可达（bash 同序；live 面 FAIL 守卫 = lib
    // 单测 live_floor_fail_guard_via_fake_repo）。
    let (brc, _) = run_bash(&["--dry"], Some(&bad_str));
    let (rrc, _, rmsg) = run_rust(&["--dry"], Some(&bad_str));
    assert_eq!(rrc, brc);
    assert_eq!(brc, 2);
    assert_eq!(rmsg, "bcts: REJECT: 空集守卫：marker 91 件 < tier1_floor 999——拒跑防假绿");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn parity_image_leg_family() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let root = repo_root();
    let repo = root.to_string_lossy().into_owned();
    let tmp = root.join("target/bctsrun-parity-img");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("dir")).unwrap();
    std::fs::write(tmp.join("dir/std.img"), b"x").unwrap();
    let img = tmp.join("dir/std.img").to_string_lossy().into_owned();
    let manifest = |dir: &str, pr: i64, name: &str| {
        std::fs::write(
            tmp.join(dir).join(name),
            format!(
                "berryos-manifest v1 format_frozen=2026-11-10 entries=01\nitem path=/KERNEL.ELF sha256={:0>64} size=000000000001 rollback_index=00000000000000000000 platform_revision={pr:010}\n",
                0
            ),
        )
        .unwrap();
    };
    // live 锚 = abi.toml level（现值 1）。
    let lv: i64 = 1;
    manifest("dir", lv, "manifest.txt");

    // —— live 态镜像腿 dry 全绿：stdout 逐行全等（含 ok 行 + 镜像腿行）——
    let (brc, bout) = run_bash(&["--image", &img, "--dry"], None);
    assert_eq!(brc, 0, "live 镜像腿应绿（PR=1 对上 abi.toml level）");
    let (rrc, rout, _) = run_rust(&["--image", &img, "--dry"], None);
    assert_eq!(rrc, 0);
    assert_eq!(bout.len(), rout.len(), "行数不等：\nbash={bout:?}\nrust={rout:?}");
    for (b, r) in bout.iter().zip(rout.iter()) {
        assert_eq!(strip_repo(b, &repo), strip_repo(r, &repo), "行不等：bash={b:?} rust={r:?}");
    }
    assert!(rout.iter().any(|l| l == "bcts: 版本声称位对账 ok：manifest platform_revision=1 == tools/abi.toml [platform].level（活树现值，mkimage 取值同源）"));
    assert!(rout.iter().any(|l| l.starts_with("bcts: 镜像腿 = EFI 镜像流（AAVMF + ") && l.contains("已对账 ok）")));

    // —— frozen 态镜像腿：锚 = 冻结件 pr（bcts@6 pr=6）——
    let f6 = "tools/bcts/frozen/bcts@6.toml";
    let img2 = tmp.join("dir2/std.img");
    std::fs::create_dir_all(tmp.join("dir2")).unwrap();
    std::fs::write(&img2, b"x").unwrap();
    let img2 = img2.to_string_lossy().into_owned();
    manifest("dir2", 6, "manifest.txt");
    let (brc, bout) = run_bash(&["--image", &img2, "--dry"], Some(f6));
    assert_eq!(brc, 0, "frozen 镜像腿应绿（PR=6 == bcts@6 pr）");
    let (rrc, rout, _) = run_rust(&["--image", &img2, "--dry"], Some(f6));
    assert_eq!(rrc, 0);
    assert_eq!(bout.len(), rout.len());
    for (b, r) in bout.iter().zip(rout.iter()) {
        assert_eq!(strip_repo(b, &repo), strip_repo(r, &repo), "frozen 镜像腿行不等");
    }
    assert!(rout.iter().any(|l| *l == format!("bcts: 版本声称位对账 ok：manifest platform_revision=6 == 冻结件 platform_revision（{repo}/tools/bcts/frozen/bcts@6.toml）")));

    // —— PR 失配拒（live 锚 1 vs manifest 9）——
    let img3 = tmp.join("dir3/std.img");
    std::fs::create_dir_all(tmp.join("dir3")).unwrap();
    std::fs::write(&img3, b"x").unwrap();
    let img3 = img3.to_string_lossy().into_owned();
    manifest("dir3", 9, "manifest.txt");
    let (brc, _) = run_bash(&["--image", &img3, "--dry"], None);
    let (rrc, _, rmsg) = run_rust(&["--image", &img3, "--dry"], None);
    assert_eq!((rrc, brc), (1, 1));
    assert!(rmsg.contains("platform_revision=9 != 回放清单 pr=1"), "{rmsg}");

    // —— manifest 缺席拒（G8 文案）——
    let img4 = tmp.join("dir4/std.img");
    std::fs::create_dir_all(tmp.join("dir4")).unwrap();
    std::fs::write(&img4, b"x").unwrap();
    let img4 = img4.to_string_lossy().into_owned();
    let (brc, _) = run_bash(&["--image", &img4, "--dry"], None);
    let (rrc, _, rmsg) = run_rust(&["--image", &img4, "--dry"], None);
    assert_eq!((rrc, brc), (1, 1));
    assert!(rmsg.starts_with("bcts: FAIL: 镜像同目录无 manifest.txt："), "{rmsg}");
    assert!(rmsg.contains("（mkimage 件 3 侧产物——缺席即非平台产链镜像；防互踩名镜像加 --manifest-name 指 mkimage 同名，G8 联动）"));

    // —— --manifest-name 联动：非缺省名在席 + 缺省缺席 → 名对上过 ——
    let img5 = tmp.join("dir5/std.img");
    std::fs::create_dir_all(tmp.join("dir5")).unwrap();
    std::fs::write(&img5, b"x").unwrap();
    let img5 = img5.to_string_lossy().into_owned();
    manifest("dir5", lv, "acme-rich.manifest.txt");
    let (brc, _) = run_bash(&["--image", &img5, "--manifest-name", "acme-rich.manifest.txt", "--dry"], None);
    assert_eq!(brc, 0, "名对上应绿");
    let (rrc, rout, _) = run_rust(&["--image", &img5, "--manifest-name", "acme-rich.manifest.txt", "--dry"], None);
    assert_eq!(rrc, 0);
    assert!(rout.iter().any(|l| l.contains("版本声称位对账 ok")));
    // 名不对拒（无静默回落）。
    let (brc, _) = run_bash(&["--image", &img5, "--manifest-name", "other.manifest.txt", "--dry"], None);
    let (rrc, _, _) = run_rust(&["--image", &img5, "--manifest-name", "other.manifest.txt", "--dry"], None);
    assert_eq!((rrc, brc), (1, 1));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn parity_bad_frozen_reject() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let root = repo_root();
    let tmp = root.join("target/bctsrun-parity-bad");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    // 坏件：名随真件 pr（防名不符截胡）+ 顶层未知键 → REJECT 退 2。
    let p = tmp.join("bcts@6.toml");
    std::fs::write(&p, "schema = 1\nsuite = \"bcts\"\nbogus = \"x\"\n").unwrap();
    let ps = p.to_string_lossy().into_owned();
    let (brc, _) = run_bash(&["--dry"], Some(&ps));
    let (rrc, _, rmsg) = run_rust(&["--dry"], Some(&ps));
    assert_eq!((rrc, rmsg.is_empty()), (2, false));
    assert_eq!(brc, 2);
    assert!(rmsg.starts_with("bcts: REJECT: 顶层未知键：['bogus']"), "{rmsg}");
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Resolved/LaunchOutcome 快照与 dry 链纯面（无 bash 依赖）。
#[test]
fn load_chain_snapshot_law() {
    let o = LoadOutcome {
        entries: vec!["META\t6\t91\t91\t1\tx\ttools/abi.toml\tsha256:y".into()],
        n_markers: 0,
        n_gates: 1,
        floor: 91,
        n_bare: 0,
        img_pr: None,
        img_pr_anchor: None,
        pre_stdout: vec![],
    };
    let r = Resolved {
        dry: true,
        selftest: false,
        tier: 1,
        mode: Mode::Frozen,
        frozen_file: "/repo/tools/bcts/frozen/bcts@6.toml".into(),
        image_file: String::new(),
        manifest_name: "manifest.txt".into(),
        manifest_name_explicit: false,
    };
    let lines = dry_echo(&r, &o);
    // frozen 态清单行用绝对形（Resolved.frozen_file）。
    assert!(lines[0].ends_with("清单 = /repo/tools/bcts/frozen/bcts@6.toml"));
}
