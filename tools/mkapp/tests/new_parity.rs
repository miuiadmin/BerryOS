//! W1-Q27 八刀·`--new` CLI oracle 对拍：Rust bin ↔ Python `mkapp.py` 真跑
//! 对拍（build_parity/json_parity 同款纪律）。Python 缺席打 skip（CI 净仓
//! 面——src/new.rs 纯面钉板兜底）。
//!
//! 覆盖：正例四件逐字节恒等 + `--json --new` 信封 result 舱逐键全等 +
//! 拒因信封（code/jx 三舱）同构 + 人读行全等 + 防覆写拒面 + 用法错形。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn run(cmd: &str, args: &[String], cwd: &Path) -> (i32, String, String) {
    let out = Command::new(cmd)
        .args(args)
        .current_dir(cwd)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .expect("spawn");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn py(args: &[String]) -> (i32, String, String) {
    let mut a = vec![repo_root().join("tools/mkapp.py").to_string_lossy().into_owned()];
    a.extend_from_slice(args);
    run("python3", &a, &repo_root())
}

fn rs(args: &[String]) -> (i32, String, String) {
    run(env!("CARGO_BIN_EXE_mkapp"), args, &repo_root())
}

fn new_args(dir: &Path, id: &str, json: bool) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    if json {
        v.push("--json".into());
    }
    v.push("--new".into());
    v.push(dir.to_string_lossy().into_owned());
    v.push("--id".into());
    v.push(id.into());
    v
}

/// 信封文本剥 dir 实例段（tempdir 两侧分实例）。
fn strip(t: &str, path: &str) -> String {
    t.replace(path, "<D>")
}

#[test]
fn new_parity_oracle() {
    let root = repo_root();
    if !root.join("tools/mkapp.py").exists()
        || Command::new("python3").arg("--version").output().is_err()
    {
        eprintln!("mkapp: tools/mkapp.py/python3 缺席——CLI 对拍 skip（纯面钉板兜底）");
        return;
    }
    let tmp = std::env::temp_dir().join(format!("mkapp-newparity-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    // ---- 正例：四件逐字节恒等 + 人读行全等（剥路径）----
    let dr = tmp.join("r").join("myapp");
    let dp = tmp.join("p").join("myapp");
    let (rrc, rout, rerr) = rs(&new_args(&dr, "MYAPP", false));
    let (prc, pout, perr) = py(&new_args(&dp, "MYAPP", false));
    assert_eq!((rrc, prc), (0, 0), "rerr={rerr} perr={perr}");
    assert_eq!(
        strip(&rout, &tmp.join("r").to_string_lossy()),
        strip(&pout, &tmp.join("p").to_string_lossy()),
        "人读行不等：\nR={rout}\nP={pout}"
    );
    for rel in ["manifest.toml", "src/main.rs", "Cargo.toml", "README.md"] {
        let a = fs::read(dr.join(rel)).unwrap();
        let b = fs::read(dp.join(rel)).unwrap();
        assert_eq!(a, b, "四件 {rel} 字节不等");
    }
    // src/ 目录外无多余产物（恰四件）
    let mut got: Vec<String> = Vec::new();
    for e in fs::read_dir(&dr).unwrap() {
        got.push(e.unwrap().file_name().to_string_lossy().into_owned());
    }
    got.sort();
    assert_eq!(got, vec!["Cargo.toml", "README.md", "manifest.toml", "src"]);

    // ---- --json --new 成功信封：result 舱逐键全等 ----
    let jr = tmp.join("jr").join("myapp");
    let jp = tmp.join("jp").join("myapp");
    let (rrc, rout, _) = rs(&new_args(&jr, "MYAPP", true));
    let (prc, pout, _) = py(&new_args(&jp, "MYAPP", true));
    assert_eq!((rrc, prc), (0, 0));
    let rn = strip(&rout, &tmp.join("jr").to_string_lossy());
    let pn = strip(&pout, &tmp.join("jp").to_string_lossy());
    assert_eq!(
        rn, pn,
        "--json --new 信封不等：\nR={rn}\nP={pn}"
    );
    assert!(rn.contains("\"op\":\"new\",\"ok\":true"));
    assert!(rn.contains("\"files\":[\"Cargo.toml\",\"README.md\",\"manifest.toml\",\"src/main.rs\"]"));
    assert_eq!(rout.lines().count(), 1, "单行纪律");

    // ---- 拒因信封同构（五门逐类）----
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "new.basename",
            tmp.join("other").to_string_lossy().into_owned(),
            "MYAPP",
        ),
        (
            "new.slug.cargo",
            tmp.join("app.t2").to_string_lossy().into_owned(),
            "APP.T2",
        ),
        (
            "new.exists",
            { // 已存在非空目录
                let d = tmp.join("busy");
                fs::create_dir_all(&d).unwrap();
                fs::write(d.join("x"), b"keep").unwrap();
                d.to_string_lossy().into_owned()
            },
            "BUSY",
        ),
        (
            "new.isfile",
            { // 已存在文件
                let f = tmp.join("afile");
                fs::write(&f, b"x").unwrap();
                f.to_string_lossy().into_owned()
            },
            "AFILE",
        ),
        (
            "new.id",
            tmp.join("-bad").to_string_lossy().into_owned(),
            "-BAD",
        ),
    ];
    for (want_code, dir, id) in &cases {
        let a: Vec<String> = vec!["--json".into(), "--new".into(), dir.clone(), "--id".into(), (*id).to_string()];
        let (rrc, rout, _) = rs(&a);
        let (prc, pout, _) = py(&a);
        assert_eq!(
            (rrc, prc),
            (1, 1),
            "{want_code}：rc {rrc}≠{prc}\nR={rout}\nP={pout}"
        );
        assert!(rout.contains(&format!("\"code\":\"{want_code}\"")), "R code：{rout}");
        assert!(pout.contains(&format!("\"code\":\"{want_code}\"")), "P code：{pout}");
        // message 双侧同构（剥 dir 路径实例）
        let rm = strip(&rout, dir);
        let pm = strip(&pout, dir);
        assert_eq!(
            rm, pm,
            "{want_code} 信封（剥路径）不等：\nR={rm}\nP={pm}"
        );
        assert_eq!(rout.lines().count(), 1, "{want_code} 单行纪律");
        for key in ["code", "where", "line", "expected", "got", "fix", "message"] {
            assert!(rout.contains(&format!("\"{key}\":")), "恒 7 键缺 {key}：{rout}");
        }
        // 拒面零落盘（basename/slug/exists/isfile/id 门均不建目录）
        if *want_code != "new.exists" && *want_code != "new.isfile" {
            let p = Path::new(dir);
            if let Some(name) = p.file_name() {
                let probe = tmp.join(name);
                if want_code == &"new.basename" {
                    assert!(!probe.join("src").exists(), "拒面不得落盘 src/");
                }
            }
        }
    }

    // ---- 用法错形：--id 缺位 / --new 缺参（双模式 exit 2）----
    let a: Vec<String> = vec!["--json".into(), "--new".into(), tmp.join("z").to_string_lossy().into_owned()];
    let (rrc, _, _) = rs(&a);
    let (prc, _, _) = py(&a);
    assert_eq!((rrc, prc), (2, 2));
    let mut half = new_args(&tmp.join("z"), "MYAPP", false);
    half.truncate(3);
    let (rrc, _, _) = rs(&half);
    assert_eq!(rrc, 2, "--new 缺 --id 用法错");

    // ---- 防覆写双证：正例后重跑同目录拒 ----
    let (rrc, rout, _) = rs(&new_args(&dr, "MYAPP", true));
    assert_eq!(rrc, 1);
    assert!(rout.contains("\"code\":\"new.exists\""));

    let _ = fs::remove_dir_all(&tmp);
}
