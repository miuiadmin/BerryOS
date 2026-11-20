//! `berry bshrc` 集成测试（BSHRC 宿主写者刀 T2）——真进程边界钉 CLI 面
//! 语义：退出码族（clap usage 恒 2/键缺席/帽拒/盘缺/off）、--json stdout
//! 纯 JSON 可解析、human 诊断走 stderr、--disk 显式零 env 触碰、序贯
//! set→get→list→rm→get-miss 流、盘保真（settings 五键链零漂移执法）与
//! crash-log 读面交叉验。
//!
//! 盘件走 `tools/mkfat.py` 真产物（src/bshrc.rs 单测同款：--put 预置＋
//! 16 扇区零前缀＋hidden 补 16 对齐主路盘形态；mkfat 缺席即失败不静默
//! 跳过）。env 敏感用例经 `Command::env` 只注入子进程（测试进程 env 零
//! 触碰——无需串行锁）。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn berry(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_berry"))
        .args(args)
        .output()
        .expect("berry 子进程启动失败")
}

/// env 注入形（BERRY_HOME/BERRY_DATA_DISK 只进子进程——进程级 env 零触碰）。
fn berry_env(envs: &[(&str, &str)], args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_berry"));
    for (k, v) in envs {
        c.env(k, v);
    }
    c.args(args).output().expect("berry 子进程启动失败")
}

/// 测试围栏目录（pid 命名——本文件各测各用子目录，与 cli.rs 先例同构）。
fn fence(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("berry-bshrc-it-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 造盘：mkfat.py 产物——空件表＝裸调默认模式（主路盘形态成品：前缀＋
/// hidden=16＋烘焙件）；非空＝--put 预置多件（EFI 卷形）后补 16 扇区零
/// 前缀＋hidden=16 对齐主路盘形态（src/bshrc.rs 单测同款）。
fn mkfat_disk(dir: &Path, files: &[(&str, &[u8])]) -> PathBuf {
    let mut args: Vec<String> = Vec::new();
    for (name, data) in files {
        let src = dir.join(format!("put-{name}"));
        std::fs::write(&src, data).unwrap();
        args.push("--put".into());
        args.push(format!("{}:/{}", src.display(), name));
    }
    let img = dir.join("disk.img");
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = Command::new("python3")
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/mkfat.py"),
        )
        .arg(&img)
        .args(&arg_refs)
        .output()
        .expect("python3 起 mkfat 失败");
    assert!(
        out.status.success(),
        "mkfat 造盘失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let vol = std::fs::read(&img).unwrap();
    if files.is_empty() {
        std::fs::write(&img, &vol).unwrap();
        return img; // 默认模式已是整盘主路形态
    }
    let mut d = vec![0u8; 16 * 512];
    d.extend_from_slice(&vol);
    let voff = 16 * 512;
    d[voff + 28..voff + 32].copy_from_slice(&16u32.to_le_bytes());
    std::fs::write(&img, &d).unwrap();
    img
}

/// 断言 usage 拒形：退出码恰为 2、stderr 非空。
fn assert_usage(out: Output, why: &str) {
    assert_eq!(
        out.status.code(),
        Some(2),
        "{why}：rc 须恰为 2（usage 语义）：stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.stderr.is_empty(), "{why}：stderr 须有诊断");
}

/// usage 拒形矩阵（C4：钉进本文件——cli_flags.rs 属 hermes 活跃 lane 零触）。
#[test]
fn usage_rejection_matrix() {
    let dir = fence("usage");
    let img = mkfat_disk(&dir, &[]);
    let img_arg = img.display().to_string();
    // 无子动作 / set 空参 / set 非 KEY=VALUE 形 / get·rm 缺键位。
    assert_usage(berry(&["bshrc"]), "无子动作");
    assert_usage(berry(&["bshrc", "--disk", &img_arg, "set"]), "set 空参");
    assert_usage(berry(&["bshrc", "--disk", &img_arg, "set", "NOEQ"]), "无 '=' 形");
    assert_usage(berry(&["bshrc", "get"]), "get 缺键位");
    assert_usage(berry(&["bshrc", "rm"]), "rm 缺键位");
    // --disk × --profile 互斥（clap conflicts_with）。
    assert_usage(
        berry(&["bshrc", "--disk", &img_arg, "--profile", "desktop", "set", "K=v"]),
        "--disk × --profile 互斥",
    );
    // 帽拒走 usage 语义：键 33B / 值带 LF。
    assert_usage(
        berry(&["bshrc", "--disk", &img_arg, "set", &format!("{}=v", "K".repeat(33))]),
        "键长帽",
    );
    assert_usage(
        berry(&["bshrc", "--disk", &img_arg, "set", "K=a\nb"]),
        "值带 LF",
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 序贯 set→get→list→rm→get-miss 全族（human 面：get 裸值、list KEY=VALUE
/// 行、set/rm 成功零 stdout、诊断走 stderr）＋ crash-log 读面交叉验。
#[test]
fn set_get_list_rm_sequence_human_face() {
    let dir = fence("seq");
    let img = mkfat_disk(&dir, &[]);
    let img_arg = img.display().to_string();

    // set：成功零 stdout、零 stderr。
    let out = berry(&["bshrc", "--disk", &img_arg, "set", "PATH=/etc", "BSHRC_E2E=host-writer-pass"]);
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty(), "human set 成功零 stdout：{}", String::from_utf8_lossy(&out.stdout));
    assert!(out.stderr.is_empty());

    // 同键更新＋新键尾追（一批）。
    let out = berry(&["bshrc", "--disk", &img_arg, "set", "PATH=/apps", "EXTRA=1"]);
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));

    // get：裸值 stdout。
    let out = berry(&["bshrc", "--disk", &img_arg, "get", "PATH"]);
    assert!(out.status.success());
    assert_eq!(out.stdout, b"/apps\n".to_vec(), "get 打裸值");

    // list：KEY=VALUE 行（与文件形一致；投影序=各键最后赋值位置升序）。
    let out = berry(&["bshrc", "--disk", &img_arg, "list"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        vec!["PATH=/apps", "BSHRC_E2E=host-writer-pass", "EXTRA=1"],
        "{text}"
    );

    // get 缺席：rc≠0（码 3）＋stderr 诊断。
    let out = berry(&["bshrc", "--disk", &img_arg, "get", "NOPE"]);
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("NOPE") && err.contains("缺席"), "{err}");

    // rm → get-miss → rm-miss。
    let out = berry(&["bshrc", "--disk", &img_arg, "rm", "EXTRA"]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let out = berry(&["bshrc", "--disk", &img_arg, "get", "EXTRA"]);
    assert_eq!(out.status.code(), Some(3));
    let out = berry(&["bshrc", "--disk", &img_arg, "rm", "EXTRA"]);
    assert_eq!(out.status.code(), Some(3));

    // crash-log 读面交叉验（独立读面=最终真值）：last-wins 后的文件字节。
    let disk = std::fs::read(&img).unwrap();
    let got = berry::crashlog::read_root_file(&disk, "BSHRC.CFG").unwrap();
    assert_eq!(
        got,
        b"PATH=/apps\nBSHRC_E2E=host-writer-pass\n".to_vec(),
        "crash-log 交叉验"
    );

    // list 缺席盘（无 BSHRC 的 fresh 盘）：空输出 rc 0（rc_load 缺席常态镜像）。
    let fdir = dir.join("f");
    std::fs::create_dir_all(&fdir).unwrap();
    let fresh = mkfat_disk(&fdir, &[]);
    // mkfat_disk 写死了 disk.img 名——fence 子目录隔离。
    let out = berry(&["bshrc", "--disk", &fresh.display().to_string(), "list"]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty(), "缺席空输出 rc 0");
    let out = berry(&["bshrc", "--disk", &fresh.display().to_string(), "get", "ANY"]);
    assert_eq!(out.status.code(), Some(3), "缺席盘 get 同 miss 语义");
    std::fs::remove_dir_all(&dir).ok();
}

/// --json 面：stdout 纯 JSON 可解析（get 值对象/list entries 数组/set·rm
/// 结果对象含 path/keys/bytes/clusters）；human 注记不混机器面。
#[test]
fn json_face_pure_stdout() {
    let dir = fence("json");
    let img = mkfat_disk(&dir, &[]);
    let img_arg = img.display().to_string();

    let out = berry(&["--json", "bshrc", "--disk", &img_arg, "set", "PATH=/etc", "G=1"]);
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("set stdout 须纯 JSON");
    assert_eq!(v["t"], "bshrc.set");
    // path 是 canonicalize 后真值（macOS /tmp symlink 落 /private/tmp）。
    assert_eq!(v["path"], std::fs::canonicalize(&img).unwrap().display().to_string());
    assert_eq!(v["keys"], serde_json::json!(["PATH", "G"]));
    assert_eq!(v["bytes"], 14);
    assert_eq!(v["clusters"], 1);

    let out = berry(&["--json", "bshrc", "--disk", &img_arg, "get", "PATH"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["t"], "bshrc.get");
    assert_eq!(v["key"], "PATH");
    assert_eq!(v["value"], "/etc");

    let out = berry(&["--json", "bshrc", "--disk", &img_arg, "list"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["t"], "bshrc.list");
    let entries = v["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["key"], "PATH");
    assert_eq!(entries[0]["value"], "/etc");
    assert_eq!(entries[1]["key"], "G");

    let out = berry(&["--json", "bshrc", "--disk", &img_arg, "rm", "G"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["t"], "bshrc.rm");
    assert_eq!(v["key"], "G");
    assert_eq!(v["bytes"], 10);
    assert_eq!(v["clusters"], 1);

    // 错误对象沿 error.rs t=error 契约（--json 失败也走 stdout JSON）。
    let out = berry(&["--json", "bshrc", "--disk", &img_arg, "get", "NOPE"]);
    assert_eq!(out.status.code(), Some(3));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["t"], "error");
    assert_eq!(v["code"], 3);

    // env 通道帽软警走 stderr（stdout 仍纯 JSON）。
    let mut batch: Vec<String> = vec![
        "--json".into(),
        "bshrc".into(),
        "--disk".into(),
        img_arg.clone(),
        "set".into(),
    ];
    for i in 0..16 {
        batch.push(format!("E{i:02}=v"));
    }
    let refs: Vec<&str> = batch.iter().map(|s| s.as_str()).collect();
    let out = berry(&refs);
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("env 通道帽") && err.contains("将截断"), "软警在 stderr：{err}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["t"], "bshrc.set");
    std::fs::remove_dir_all(&dir).ok();
}

/// 盘保真（G2 执法——非 BSHRC 域零漂移）：预置 SETTINGS.CFG＋LAYOUT.JSN
/// 的盘上跑完 set/rm 全族后，两件内容经 crash-log 读面字节恒等、盘长恒等。
#[test]
fn disk_fidelity_settings_zero_drift() {
    let dir = fence("fidelity");
    let settings = b"{\"v\":1,\"five-keys\":true,\"theme\":\"dark\"}\n";
    let layout = b"[7,8,9]\n";
    let img = mkfat_disk(&dir, &[("SETTINGS.CFG", settings), ("LAYOUT.JSN", layout)]);
    let img_arg = img.display().to_string();
    let before = std::fs::read(&img).unwrap();

    let ops: [&[&str]; 5] = [
        &["bshrc", "--disk", &img_arg, "set", "PATH=/etc"],
        &["bshrc", "--disk", &img_arg, "set", "PATH=/apps", "A=1"],
        &["bshrc", "--disk", &img_arg, "list"],
        &["bshrc", "--disk", &img_arg, "rm", "A"],
        &["bshrc", "--disk", &img_arg, "get", "PATH"],
    ];
    for op in ops {
        let out = berry(op);
        assert!(out.status.success(), "{}：stderr={}", op.join(" "), String::from_utf8_lossy(&out.stderr));
    }

    let after = std::fs::read(&img).unwrap();
    assert_eq!(before.len(), after.len(), "盘长恒等");
    // settings 五键链零漂移（crash-log 读面字节恒等）。
    assert_eq!(&berry::crashlog::read_root_file(&after, "SETTINGS.CFG").unwrap(), settings);
    assert_eq!(&berry::crashlog::read_root_file(&after, "LAYOUT.JSN").unwrap(), layout);
    // 与写前对拍：两件字节域不变。
    assert_eq!(
        &berry::crashlog::read_root_file(&before, "SETTINGS.CFG").unwrap(),
        settings
    );
    assert_eq!(berry::crashlog::read_root_file(&after, "BSHRC.CFG").unwrap(), b"PATH=/apps\n".to_vec());
    std::fs::remove_dir_all(&dir).ok();
}

/// 盘解析失败族：--disk 不可达 / BERRY_DATA_DISK=off / 缺省盘缺席（附
/// home 清单与 just desktop 指路）/ 非 FAT 裸件。env 用例经 Command::env
/// 只注入子进程（测试进程 env 零触碰）。
#[test]
fn disk_resolution_failure_family() {
    let dir = fence("resolve");
    // ① --disk 不可达：device 族报错＋指路。
    let out = berry(&["bshrc", "--disk", &dir.join("nope.img").display().to_string(), "set", "K=v"]);
    assert_eq!(out.status.code(), Some(6));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("不可达") || err.contains("nope"), "{err}");
    // ② BERRY_DATA_DISK=off：显式关——写语境报错说明（env 族）。
    let out = berry_env(
        &[("BERRY_DATA_DISK", "off")],
        &["bshrc", "set", "K=v"],
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("off"), "{err}");
    // ③ 缺省位缺席：G4 报错附 home()/data 盘清单与 just desktop 指路。
    let home = dir.join("home");
    std::fs::create_dir_all(home.join("data")).unwrap();
    std::fs::write(home.join("data/phone.img"), b"x").unwrap();
    let out = berry_env(
        &[("BERRY_HOME", &home.display().to_string())],
        &["bshrc", "set", "K=v"],
    );
    assert_eq!(out.status.code(), Some(6));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("desktop.img") && err.contains("phone.img"), "报错附盘清单：{err}");
    assert!(err.contains("缺席"), "{err}");
    let err_hint = String::from_utf8_lossy(&out.stderr);
    assert!(err_hint.contains("just desktop"), "指路：{err_hint}");
    // ④ env 显式路径直用：全族走通。
    let img = mkfat_disk(&dir, &[]);
    let out = berry_env(
        &[("BERRY_DATA_DISK", &img.display().to_string())],
        &["bshrc", "set", "K=v"],
    );
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    // ⑤ 缺省位存在即用（BERRY_HOME 指向带 desktop.img 的 home）。
    let home2 = dir.join("home2");
    std::fs::create_dir_all(home2.join("data")).unwrap();
    std::fs::copy(&img, home2.join("data/desktop.img")).unwrap();
    let out = berry_env(
        &[("BERRY_HOME", &home2.display().to_string())],
        &["bshrc", "get", "K"],
    );
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.stdout, b"v\n".to_vec());
    // ⑥ 非 FAT 裸件：usage 之外的真盘拒形（device 族，非 panic）。
    let bare = dir.join("bare.img");
    std::fs::write(&bare, b"just bytes here").unwrap();
    let out = berry(&["bshrc", "--disk", &bare.display().to_string(), "list"]);
    assert_eq!(out.status.code(), Some(6));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("FAT12 卷"), "{err}");
    // ⑦ 未知 profile：env 族（profile 单源校验）。
    let out = berry(&["bshrc", "--profile", "nope", "list"]);
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unknown profile"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}
