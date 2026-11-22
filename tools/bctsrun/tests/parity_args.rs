//! bctsrun 六刀·parity：CLI 参数面对拍。oracle = `tools/bcts-run.sh` 真跑
//! 子进程——同一 argv + BCTS_FROZEN 环境，双侧拒腿（stderr 首行 + 退码）
//! 与绿腿（退出 0 后达「参数段完成」的间接面：--dry 回显首行）逐字对齐。
//!
//! 面域：参数 while/case 全分支（缺值/值域/未知/重复覆盖）+ BCTS_FROZEN
//! 前缀识别与冲突门 + 名不符拒 + 在席检查 + 相对路径归一 + manifest-name
//! 守卫全族 + --image 约束面（tier2 冲突/selftest 互斥/在席）+ T2 缺省
//! 工件解析。
//!
//! 绿腿出口差异注记：bash 侧参数段完成后继续走清单装载（真实文件系统
//! 依赖），本刀只镜像到参数段出口——绿腿对拍 = 双侧 rc=0 面（bash 用
//! 仓内真源 --dry）+ Rust 侧 Resolved 快照断言。bash 缺席 = skip。

use bctsrun::resolve_args;
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

/// bash 真跑：返回 (rc, stderr 首行)。
fn run_bash(args: &[&str], frozen_env: Option<&str>) -> (i32, String) {
    let root = repo_root();
    let script = root.join("tools/bcts-run.sh");
    let mut cmd = Command::new(script.as_os_str());
    cmd.args(args)
        .current_dir(&root)
        .env_remove("BCTS_FROZEN");
    if let Some(f) = frozen_env {
        cmd.env("BCTS_FROZEN", f);
    }
    let o = cmd.output().expect("spawn bcts-run.sh");
    let err = String::from_utf8_lossy(&o.stderr)
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    (o.status.code().unwrap_or(-1), err)
}

/// Rust 侧出口：(拒文案, 退码)。
fn run_rust(args: &[&str], frozen_env: Option<&str>) -> (String, i32) {
    let root = repo_root();
    let argv: Vec<String> = std::iter::once("bcts-run.sh".to_string())
        .chain(args.iter().map(|s| s.to_string()))
        .collect();
    let repo = root.to_string_lossy().into_owned();
    match resolve_args(&argv, &repo, frozen_env) {
        Ok(_) => (String::new(), 0),
        Err((msg, code)) => (msg, code),
    }
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn parity_reject_family() {
    if !sh_ok() {
        eprintln!("skip: bash absent");
        return;
    }
    let root = repo_root();
    let repo = root.to_string_lossy().into_owned();
    let t2 = "tools/bcts/frozen/bcts-t2@1.toml";
    let t1 = "tools/bcts/frozen/bcts@6.toml";
    let abs_t2 = format!("{repo}/{t2}");
    let abs_t1 = format!("{repo}/{t1}");
    // 合成现场：真镜像件（在席检查用）+ 缺席相对名。
    let tmp = root.join("target/bctsrun-parity-args");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("imgdir")).unwrap();
    std::fs::write(tmp.join("imgdir/std.img"), b"x").unwrap();
    // manifest.txt 缺席拒腿属装载段（bash :590）非参数段——钉界注记归后
    // 续刀；本腿现场只备在席镜像（imgdir/）。
    std::fs::create_dir_all(tmp.join("imgdir")).unwrap();
    let img_abs = tmp.join("imgdir/std.img").to_string_lossy().into_owned();
    let missing_abs = format!("{repo}/nonexistent.img");
    let t2_abs_latest = format!(
        "{}/{}",
        repo,
        "tools/bcts/frozen/bcts-t2@1.toml"
    );

    // 镜像缺席拒腿（bash 后续段装载面，非参数段——注记钉界）：镜像在席
    // 门本身在参数段（:210-211），manifest 缺席门在装载段（:590）。本刀
    // 面到参数段出口为止，此腿归后续刀。
    let img_missing_case: (&[&str], Option<&str>, String) = (
        &["--image", "nonexistent.img", "--dry"],
        None,
        format!("bcts: FAIL: 镜像不存在：{missing_abs}（标准镜像先 just image；定制镜像走 mkimage 产链）"),
    );

    let mut cases: Vec<(&[&str], Option<&str>, String)> = vec![
        // —— 参数 case 面 ——
        (
            &["--tier"],
            None,
            "bcts: --tier 缺值（1 | 2）".into(),
        ),
        (
            &["--tier", "3"],
            None,
            "bcts: --tier 须 1 或 2，实得 3（v0 只开通 Tier 1/2）".into(),
        ),
        (
            &["--bogus"],
            None,
            "bcts: 未知参数：--bogus（--dry | --selftest | --tier 1|2 | --image <img> | --manifest-name <name>；冻结件经 BCTS_FROZEN=<file> 指定）".into(),
        ),
        (
            &["--image"],
            None,
            "bcts: --image 缺值（<FAT 整盘镜像路径>）".into(),
        ),
        (
            &["--manifest-name"],
            None,
            "bcts: --manifest-name 缺值（<纯文件名——mkimage --manifest-name 同名>）".into(),
        ),
        // —— BCTS_FROZEN 面 ——
        (
            &["--dry"],
            Some("/nonexistent.toml"),
            "bcts: FAIL: BCTS_FROZEN 工件不存在：/nonexistent.toml".into(),
        ),
        (
            &["--dry"],
            Some("tools/bcts/frozen/zz.toml"),
            format!("bcts: FAIL: BCTS_FROZEN 工件不存在：{abs_t2}")
                .replace("bcts-t2@1.toml", "zz.toml"),
        ),
        (
            &["--tier", "1", "--dry"],
            Some(t2),
            "bcts: FAIL: 工件 bcts-t2@1.toml 是 Tier 2 件，与 --tier 1 冲突（用 --tier 2 或去参自动识别）".into(),
        ),
        (
            &["--tier", "2", "--dry"],
            Some(t1),
            "bcts: FAIL: 工件 bcts@6.toml 是 Tier 1 件，与 --tier 2 冲突（Tier 2 工件前缀 = bcts-t2@*.toml）".into(),
        ),
        // 名不符（真件、错名形）：touch 合成。
        // —— manifest-name 守卫 ——
        (
            &["--manifest-name", "alt.txt"],
            None,
            "bcts: FAIL: --manifest-name 只与 --image 同席（标准流无 manifest 消费面——G8 联动位在镜像腿版本声称位对账）".into(),
        ),
        (
            &["--image", "PLACEHOLDER_IMG", "--manifest-name", "a/b.txt"],
            None,
            "bcts: FAIL: --manifest-name 须纯文件名（非空、无路径分隔符——mkimage manifest_path_for 同款守卫，同目录侧产物语义），got 'a/b.txt'".into(),
        ),
        (
            &["--image", "PLACEHOLDER_IMG", "--manifest-name", "a\\b.txt"],
            None,
            "bcts: FAIL: --manifest-name 须纯文件名（非空、无路径分隔符——mkimage manifest_path_for 同款守卫，同目录侧产物语义），got 'a\\b.txt'".into(),
        ),
        // —— --image 约束面 ——
        (
            &["--image", "nonexistent.img", "--tier", "2"],
            None,
            "bcts: FAIL: --image 只开通 Tier 1 镜像回放（Tier 2 = 零 QEMU 静态对账，无镜像位）".into(),
        ),
        (
            &["--image", "nonexistent.img", "--selftest"],
            None,
            "bcts: FAIL: --selftest 与 --image 互斥（selftest = 零 QEMU 合成腿，无回放对象；镜像腿拒面见 --selftest 内合成腿）".into(),
        ),
    ];
    cases.push(img_missing_case);
    // nomanifest/ 目录腿 = 装载段（bash :590）非参数段——钉界注记，归后续刀。

    for (args, env, want_msg) in cases {
        let args_ref: Vec<&str> = args.iter().copied().collect();
        let (brc, bmsg) = run_bash(&args_ref, env);
        let (rmsg, rrc) = run_rust(&args_ref, env);
        assert_eq!(rrc, brc, "退码不等：{args_ref:?} env={env:?}\nbash={bmsg}\nrust={rmsg}");
        assert_eq!(rmsg, bmsg, "文案不等：{args_ref:?} env={env:?}");
        assert_eq!(rmsg, want_msg, "期望锚不等：{args_ref:?}\ngot  {rmsg}\nwant {want_msg}");
        assert_ne!(brc, 0);
    }

    // —— 名不符拒（真件、错名形——touch 合成后双侧真跑）——
    let bad = tmp.join("bogus@x.toml");
    std::fs::write(&bad, b"x").unwrap();
    let bad_str = bad.to_string_lossy().into_owned();
    let (brc, bmsg) = run_bash(&["--dry"], Some(&bad_str));
    let (rmsg, rrc) = run_rust(&["--dry"], Some(&bad_str));
    assert_eq!((rrc, rmsg.as_str()), (brc, bmsg.as_str()));
    assert_eq!(bmsg, "bcts: FAIL: 工件名 bogus@x.toml 不符（bcts@<pr>.toml | bcts-t2@<pr>.toml）");
    assert_eq!(brc, 2);

    // —— 绿腿：参数段过 + Resolved 快照（bash --dry rc=0 面）——
    let (brc, _) = run_bash(&["--dry"], None);
    assert_eq!(brc, 0);
    let argv = s(&["bcts-run.sh", "--dry"]);
    let r = resolve_args(&argv, &repo, None).unwrap();
    assert_eq!(r.tier, 1);
    assert_eq!(r.mode.to_string(), "live");

    let (brc, _) = run_bash(&["--tier", "2", "--dry"], Some(t2));
    assert_eq!(brc, 0, "t2 前缀识别 + --tier 2 应绿");
    let argv = s(&["bcts-run.sh", "--tier", "2", "--dry"]);
    let r = resolve_args(&argv, &repo, Some(t2)).unwrap();
    assert_eq!(r.tier, 2);
    assert_eq!(r.mode.to_string(), "frozen");
    assert_eq!(r.frozen_file, abs_t2);

    // 前缀自动识别（无 --tier）：t2 件 → tier 2。
    let argv = s(&["bcts-run.sh", "--dry"]);
    let r = resolve_args(&argv, &repo, Some(t2)).unwrap();
    assert_eq!(r.tier, 2);
    assert_eq!(r.frozen_file, abs_t2);

    // T1 件 + 无 --tier：tier 1 保持。
    let argv = s(&["bcts-run.sh", "--dry"]);
    let r = resolve_args(&argv, &repo, Some(t1)).unwrap();
    assert_eq!(r.tier, 1);
    assert_eq!(r.frozen_file, abs_t1);

    // --tier 2 无 FROZEN：缺省工件 = 数值序最新 bcts-t2@*.toml。
    let (brc, _) = run_bash(&["--tier", "2", "--dry"], None);
    assert_eq!(brc, 0);
    let argv = s(&["bcts-run.sh", "--tier", "2", "--dry"]);
    let r = resolve_args(&argv, &repo, None).unwrap();
    assert_eq!(r.tier, 2);
    assert_eq!(r.mode.to_string(), "frozen");
    assert_eq!(r.frozen_file, t2_abs_latest);

    // 镜像绿腿：合成 manifest.txt 后参数段 + dry 全过。
    std::fs::write(
        tmp.join("imgdir/manifest.txt"),
        format!(
            "berryos-manifest v1 format_frozen=2026-11-10 entries=01\nitem path=/K sha256={:0>64} size=000000000001 rollback_index=00000000000000000000 platform_revision=0000000001\n",
            0
        ),
    )
    .unwrap();
    let (brc, _) = run_bash(&["--image", &img_abs, "--dry"], None);
    assert_eq!(brc, 0, "镜像 dry 应绿（PR=1 对上 live 锚？——abi.toml level 非 1 时拒）");
    let argv = s(&["bcts-run.sh", "--image", &img_abs, "--dry"]);
    let r = resolve_args(&argv, &repo, None).unwrap();
    assert_eq!(r.image_file, img_abs);
    assert_eq!(r.manifest_name, "manifest.txt");

    let _ = std::fs::remove_dir_all(&tmp);
}
