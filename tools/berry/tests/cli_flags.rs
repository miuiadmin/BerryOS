//! 二十三刀补：CLI 集成测试第三批——dispatch 校验面经真实进程边界钉板
//! （cbox 刀 3 补面续）。校验分支在 find_run 之前早期返回，不碰运行面：
//! `vnc passwd --generate` 族互斥、`vnc web` 标志族前置条件/互斥矩阵
//! （--host/--qr/--no-qr/--ca/--insecure × --lan）、`--host` 值校验经
//! lan::validate_host_override 全拒面。全部 rc=2 usage 语义 + stderr 带
//! 诊断（宿主脚本面契约：非零退出码 + stderr 可读，非只靠文本）。

use std::path::PathBuf;
use std::process::Output;

fn berry(args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_berry"))
        .args(args)
        .output()
        .expect("berry 子进程启动失败")
}

/// 断言 usage 拒形：退出码恰为 2、stderr 非空且含期望片段。
fn assert_usage(out: Output, expect: &str) {
    assert_eq!(
        out.status.code(),
        Some(2),
        "rc 须恰为 2（usage 语义）：stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.is_empty(), "stderr 须有诊断");
    assert!(err.contains(expect), "stderr 须含 {expect:?}：{err}");
}

/// 测试围栏目录（pid 命名——与 cli.rs 先例同构，本文件各测各用子目录）。
fn fence(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "berry-cli-flags-{}-{tag}",
        std::process::id()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `vnc passwd` 标志族：`--generate` 与 `--set`/`--expire` 互斥（dispatch
/// 内联拒——commands::vnc_passwd 只见 --set × --expire，generate 对组合
/// 在 main.rs 拒）。三拒形 + 两合法形（default 生成 / --generate 单独）
/// 走不到 find_run 才可无运行面直测——合法形会先找 run（无 run 时报
/// 4 not-found，非 2），钉「合法组合通过校验层」的边界。
#[test]
fn vnc_passwd_generate_mutex_family() {
    // 拒形 ①：--generate × --set。
    assert_usage(
        berry(&["vnc", "passwd", "--generate", "--set", "pw123456"]),
        "mutually exclusive",
    );
    // 拒形 ②：--generate × --expire。
    assert_usage(
        berry(&["vnc", "passwd", "--generate", "--expire", "never"]),
        "mutually exclusive",
    );
    // 拒形 ③：三者同上（generate 与任一即拒）。
    assert_usage(
        berry(&[
            "vnc", "passwd", "--generate", "--set", "pw123456", "--expire", "3600",
        ]),
        "mutually exclusive",
    );
    // 边界：合法组合（无 --generate）不在 dispatch 拒——进入 find_run，
    // 无运行会话时 rc=4（run 定位失败），证明校验层放行。
    let pass = berry(&["vnc", "passwd", "--expire", "never"]);
    assert_ne!(pass.status.code(), Some(2), "合法组合不得在 usage 层拒");
}

/// `vnc web` 标志族前置条件/互斥矩阵（wave-10/11 校验分支全集）：
/// 六拒形逐条钉板——无 --lan 时 --host / --qr / --no-qr / --ca /
/// --insecure 各拒；--qr × --no-qr 矛盾拒；--ca × --insecure 互斥拒。
#[test]
fn vnc_web_flag_family_guards() {
    // ① --host 无 --lan。
    assert_usage(
        berry(&["vnc", "web", "--host", "192.168.1.105"]),
        "--host requires --lan",
    );
    // ② --qr 无 --lan。
    assert_usage(berry(&["vnc", "web", "--qr"]), "--qr/--no-qr require --lan");
    // ③ --no-qr 无 --lan。
    assert_usage(berry(&["vnc", "web", "--no-qr"]), "--qr/--no-qr require --lan");
    // ④ --ca 无 --lan。
    let dir = fence("ca");
    let ca = dir.join("ca");
    std::fs::create_dir_all(&ca).unwrap();
    let ca_arg = ca.display().to_string();
    assert_usage(
        berry(&["vnc", "web", "--ca", &ca_arg]),
        "--ca/--insecure require --lan",
    );
    // ⑤ --insecure 无 --lan。
    assert_usage(
        berry(&["vnc", "web", "--insecure"]),
        "--ca/--insecure require --lan",
    );
    // ⑥ --qr × --no-qr 矛盾（带 --lan 走到该分支）。
    assert_usage(
        berry(&["vnc", "web", "--lan", "--qr", "--no-qr"]),
        "--qr and --no-qr are mutually exclusive",
    );
    // ⑦ --ca × --insecure 互斥（带 --lan 走到该分支）。
    assert_usage(
        berry(&["vnc", "web", "--lan", "--ca", &ca_arg, "--insecure"]),
        "--ca and --insecure are mutually exclusive",
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// `--host` 值校验（lan::validate_host_override 经 dispatch 接线）：空串、
/// 含空白、含 `/`、含 `\` 四类拒形；拒形须发生在 --lan 语义之前（配对
/// --lan 时校验仍拒，rc=2 + hint 词表面）。合法形（裸 IPv4）放行进入
/// run 定位层（rc≠2）。
#[test]
fn vnc_web_host_value_validation() {
    for bad in ["", "  ", "a b", "host/path", "host\\path", "192.168.1.1/x"] {
        let out = berry(&["vnc", "web", "--lan", "--host", bad]);
        assert_eq!(
            out.status.code(),
            Some(2),
            "bad host={bad:?} 须 rc=2：{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("invalid --host value"),
            "拒形 stderr 须含 invalid --host value：{err}"
        );
    }
    // 合法裸地址：值校验放行 → 进入 run 定位层（rc≠2）。传不存在的
    // run id 把失败点钉在 find_run（确定性 env 错 rc=3）：无参 find_run
    // 会回落本机最新 run 目录——共享工作树上并行流水线的活 run 会把
    // 进程一路带到 serve accept 循环，子进程永不退出（挂死测试宿主）。
    let ok = berry(&[
        "vnc",
        "web",
        "--lan",
        "--host",
        "192.168.1.105",
        "--run",
        "w1-cli-flags-nonexistent-run",
    ]);
    let code = ok.status.code();
    assert_ne!(code, Some(2), "合法 host 不得在 usage 层拒");
    assert!(
        code.is_some(),
        "run 定位层应给出确定性退出码而非信号：{}",
        String::from_utf8_lossy(&ok.stderr)
    );
}
