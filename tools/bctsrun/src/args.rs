//! bctsrun 六刀：CLI 参数面镜像（现役单源 = `tools/bcts-run.sh` 参数段
//! :117-221——参数 while/case 循环 + BCTS_FROZEN 前缀识别 Tier + 名↔tier
//! 冲突门 + --manifest-name 纯文件名守卫 + --image 约束面 + Tier 2 缺省
//! 工件解析，彻底 Rust 化轨道 b 层 #7 刀序末档六刀）。
//!
//! 输入 = `argv`（含 argv[0] 佔位——本刀只镜像到「解析完成」的出口协议：
//! 拒腿 stderr 文案 + 退码逐字；绿腿产出 `Resolved` 三态快照（缺省值/
//! 显式位/路径归一结果）。后续段（清单装载/守卫/回显）不在本刀面。
//!
//! 已知边界（真跑钉板，非镜像缺陷）：
//! - 路径归一 = 纯词面 `REPO/` 前缀拼接（非 canonicalize）；相对
//!   BCTS_FROZEN/--image 拒文案含拼接后的绝对形。
//! - 工件在席检查 = `[ -f ]` 语义（符号链接跟随后缺席即拒）。
//! - Tier 2 缺省工件序 = `sort -t@ -k2 -n`（pr 十进制数值序，尾大者胜；
//!   同 pr 字典序并列时 sort 稳定序）。

use std::path::{Path, PathBuf};

/// 解析完成态（bash 后续段的输入快照）。
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    /// 缺省 0；--dry 置 1。
    pub dry: bool,
    /// 缺省 0（本刀不消费，照记）。
    pub selftest: bool,
    /// 缺省 1；BCTS_FROZEN 前缀/显式 --tier 可改。
    pub tier: u8,
    /// live | frozen。
    pub mode: Mode,
    /// frozen 态工件绝对形；live 态空串。
    pub frozen_file: String,
    /// 缺省空串；--image 归一后绝对形。
    pub image_file: String,
    /// 缺省 "manifest.txt"；显式 --manifest-name 值。
    pub manifest_name: String,
    /// --manifest-name 显式位。
    pub manifest_name_explicit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Live,
    Frozen,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Mode::Live => "live",
            Mode::Frozen => "frozen",
        })
    }
}

/// 解析出口：Ok = 绿腿（继续走后续段）；Err = 拒腿（stderr 文案 + 退码，
/// 逐字镜像 bash `echo … >&2; exit N`）。
pub type ArgsOutcome = Result<Resolved, (String, i32)>;

/// `sort -t@ -k2 -n | tail -1`：pr 十进制数值序取尾（同分并列字典序——
/// coreutils sort 稳定序下并列时后读入者胜，语料不构造并列形）。
fn latest_t2(frozen_home: &Path) -> Option<PathBuf> {
    let mut best: Option<(i64, PathBuf)> = None;
    for ent in std::fs::read_dir(frozen_home).ok()?.flatten() {
        let name = ent.file_name();
        let name = match name.to_str() {
            Some(n) => n,
            None => continue,
        };
        if !(name.starts_with("bcts-t2@") && name.ends_with(".toml")) {
            continue;
        }
        let pr: i64 = name["bcts-t2@".len()..name.len() - 5].parse().unwrap_or(0);
        // sort -n 数值序取尾：pr 更大者替换；并列时保持先读入者（coreutils
        // 稳定序 + tail -1 = 并列段最后一行，read_dir 序与 ls 序在此面等
        // 价——语料不构造并列形，取「不小于即换」贴合 tail 语义）。
        let replace = match &best {
            Some((bp, _)) => pr >= *bp,
            None => true,
        };
        if replace {
            best = Some((pr, ent.path()));
        }
    }
    best.map(|(_, p)| p)
}

/// bcts-run.sh 参数段镜像（:117-221）。`argv` 含 argv[0]；`repo` = 仓库根
/// （SCRIPT_DIR/.. 归一位，调用方注入——parity 腿真跑对拍时传仓根）。
pub fn resolve_args(argv: &[String], repo: &str, frozen_env: Option<&str>) -> ArgsOutcome {
    let mut dry = false;
    let mut selftest = false;
    let mut tier: u8 = 1;
    let mut tier_explicit = false;
    let mut image_file = String::new();
    let mut manifest_name = "manifest.txt".to_string();
    let mut manifest_name_explicit = false;

    let mut it = argv.iter().skip(1).peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dry" => dry = true,
            "--selftest" => selftest = true,
            "--image" => match it.next() {
                None => {
                    return Err((
                        "bcts: --image 缺值（<FAT 整盘镜像路径>）".to_string(),
                        2,
                    ))
                }
                Some(v) => image_file = v.clone(),
            },
            "--manifest-name" => match it.next() {
                None => {
                    return Err((
                        "bcts: --manifest-name 缺值（<纯文件名——mkimage --manifest-name 同名>）"
                            .to_string(),
                        2,
                    ))
                }
                Some(v) => {
                    manifest_name = v.clone();
                    manifest_name_explicit = true;
                }
            },
            "--tier" => match it.next() {
                None => return Err(("bcts: --tier 缺值（1 | 2）".to_string(), 2)),
                Some(v) => match v.as_str() {
                    "1" | "2" => {
                        tier = v.parse().unwrap();
                        tier_explicit = true;
                    }
                    _ => {
                        return Err((format!(
                            "bcts: --tier 须 1 或 2，实得 {v}（v0 只开通 Tier 1/2）"
                        ), 2))
                    }
                },
            },
            other => {
                return Err((format!(
                    "bcts: 未知参数：{other}（--dry | --selftest | --tier 1|2 | --image <img> | --manifest-name <name>；冻结件经 BCTS_FROZEN=<file> 指定）"
                ), 2))
            }
        }
    }

    // BCTS_FROZEN：前缀识别 + 名↔tier 冲突门（:149-177）。
    let mut mode = Mode::Live;
    let mut frozen_file = String::new();
    if let Some(f) = frozen_env {
        if !f.is_empty() {
            mode = Mode::Frozen;
            frozen_file = if f.starts_with('/') {
                f.to_string()
            } else {
                format!("{repo}/{f}")
            };
            if !Path::new(&frozen_file).is_file() {
                return Err((
                    format!("bcts: FAIL: BCTS_FROZEN 工件不存在：{frozen_file}"),
                    1,
                ));
            }
            let base = basename(&frozen_file);
            if base.starts_with("bcts-t2@") && base.ends_with(".toml") {
                if tier_explicit && tier == 1 {
                    return Err((format!(
                        "bcts: FAIL: 工件 {base} 是 Tier 2 件，与 --tier 1 冲突（用 --tier 2 或去参自动识别）"
                    ), 2));
                }
                tier = 2;
            } else if base.starts_with("bcts@") && base.ends_with(".toml") {
                if tier == 2 {
                    return Err((format!(
                        "bcts: FAIL: 工件 {base} 是 Tier 1 件，与 --tier 2 冲突（Tier 2 工件前缀 = bcts-t2@*.toml）"
                    ), 2));
                }
            } else {
                return Err((format!(
                    "bcts: FAIL: 工件名 {base} 不符（bcts@<pr>.toml | bcts-t2@<pr>.toml）"
                ), 2));
            }
        }
    }

    // --manifest-name 守卫（:187-196）：只与 --image 同席 + 纯文件名。
    if manifest_name_explicit && image_file.is_empty() {
        return Err(("bcts: FAIL: --manifest-name 只与 --image 同席（标准流无 manifest 消费面——G8 联动位在镜像腿版本声称位对账）".to_string(), 2));
    }
    if manifest_name.is_empty()
        || manifest_name == "."
        || manifest_name == ".."
        || manifest_name.contains('/')
        || manifest_name.contains('\\')
    {
        return Err((format!(
            "bcts: FAIL: --manifest-name 须纯文件名（非空、无路径分隔符——mkimage manifest_path_for 同款守卫，同目录侧产物语义），got '{}'",
            manifest_name
        ), 2));
    }

    // --image 约束面（:201-215）：Tier 2 冲突 / selftest 互斥 / 路径归一 +
    // 在席（[ -f ] 语义）。
    if !image_file.is_empty() {
        if tier == 2 {
            return Err(("bcts: FAIL: --image 只开通 Tier 1 镜像回放（Tier 2 = 零 QEMU 静态对账，无镜像位）".to_string(), 2));
        }
        if selftest {
            return Err(("bcts: FAIL: --selftest 与 --image 互斥（selftest = 零 QEMU 合成腿，无回放对象；镜像腿拒面见 --selftest 内合成腿）".to_string(), 2));
        }
        if !image_file.starts_with('/') {
            image_file = format!("{repo}/{image_file}");
        }
        if !Path::new(&image_file).is_file() {
            return Err((format!(
                "bcts: FAIL: 镜像不存在：{image_file}（标准镜像先 just image；定制镜像走 mkimage 产链）"
            ), 1));
        }
    }

    // Tier 2 缺省工件（:216-221）：无活树单源 → frozen/ 最新 bcts-t2@*.toml。
    if tier == 2 && frozen_file.is_empty() {
        let home = Path::new(repo).join("tools/bcts/frozen");
        match latest_t2(&home) {
            Some(p) => {
                frozen_file = p.to_string_lossy().into_owned();
                mode = Mode::Frozen;
            }
            None => {
                return Err((format!(
                    "bcts: FAIL: {home_display} 无 bcts-t2@*.toml 冻结件（Tier 2 无活树单源）",
                    home_display = home.to_string_lossy()
                ), 1));
            }
        }
    }

    Ok(Resolved {
        dry,
        selftest,
        tier,
        mode,
        frozen_file,
        image_file,
        manifest_name,
        manifest_name_explicit,
    })
}

/// bash `basename`（最后一个 `/` 后段；无分隔符 = 全串）。
fn basename(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[i + 1..],
        None => p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn args_law() {
        // 缺省：live / tier1 / manifest.txt。
        let r = resolve_args(&s(&["bcts-run.sh"]), "/repo", None).unwrap();
        assert_eq!(r.mode, Mode::Live);
        assert_eq!(r.tier, 1);
        assert_eq!(r.manifest_name, "manifest.txt");
        assert!(!r.manifest_name_explicit);
        // 旗标重复 = 后写覆盖（bash 同形）。
        let r = resolve_args(&s(&["bcts-run.sh", "--dry", "--dry"]), "/repo", None).unwrap();
        assert!(r.dry);
        // --tier 后写覆盖 + 显式位。
        let r = resolve_args(
            &s(&["bcts-run.sh", "--tier", "2", "--tier", "1"]),
            "/repo",
            None,
        )
        .unwrap();
        assert_eq!(r.tier, 1);
        // --image 后写覆盖 + 归一（缺席即拒——绿腿用 repo 下真路径构造）。
        let err = resolve_args(&s(&["bcts-run.sh", "--image", "x.img"]), "/repo", None)
            .unwrap_err();
        assert_eq!(err.1, 1);
        // 未知参数拒。
        let (msg, code) =
            resolve_args(&s(&["bcts-run.sh", "--bogus"]), "/repo", None).unwrap_err();
        assert_eq!(code, 2);
        assert!(msg.starts_with("bcts: 未知参数：--bogus（--dry | --selftest"));
        // 缺值三面。
        assert_eq!(
            resolve_args(&s(&["bcts-run.sh", "--tier"]), "/repo", None).unwrap_err().1,
            2
        );
        assert_eq!(
            resolve_args(&s(&["bcts-run.sh", "--image"]), "/repo", None)
                .unwrap_err()
                .0,
            "bcts: --image 缺值（<FAT 整盘镜像路径>）"
        );
        assert_eq!(
            resolve_args(&s(&["bcts-run.sh", "--manifest-name"]), "/repo", None)
                .unwrap_err()
                .0,
            "bcts: --manifest-name 缺值（<纯文件名——mkimage --manifest-name 同名>）"
        );
        // --tier 值域。
        let (msg, code) = resolve_args(
            &s(&["bcts-run.sh", "--tier", "3"]),
            "/repo",
            None,
        )
        .unwrap_err();
        assert_eq!(code, 2);
        assert_eq!(msg, "bcts: --tier 须 1 或 2，实得 3（v0 只开通 Tier 1/2）");
    }

    #[test]
    fn manifest_name_guard_law() {
        let img = "/repo/std.img"; // 缺席——但同席门先于在席门，文案钉同席面。
        for bad in ["", ".", "..", "a/b.txt", "a\\b.txt"] {
            let (msg, code) = resolve_args(
                &s(&["bcts-run.sh", "--image", img, "--manifest-name", bad]),
                "/repo",
                None,
            )
            .unwrap_err();
            assert_eq!(code, 2, "拒码：{bad}");
            let want = format!(
                "bcts: FAIL: --manifest-name 须纯文件名（非空、无路径分隔符——mkimage manifest_path_for 同款守卫，同目录侧产物语义），got '{bad}'"
            );
            assert_eq!(msg, want);
        }
        // 只与 --image 同席（守卫先于 image 在席检查——文案钉顺序）。
        let (msg, code) = resolve_args(
            &s(&["bcts-run.sh", "--manifest-name", "alt.txt"]),
            "/repo",
            None,
        )
        .unwrap_err();
        assert_eq!(code, 2);
        assert!(msg.contains("--manifest-name 只与 --image 同席"));
    }

    #[test]
    fn t2_default_pick_law() {
        // 数值序取尾（@2 > @10 字典序坑的镜像面）：纯目录扫描函数直测。
        let tmp = std::env::temp_dir().join(format!(
            "bctsrun-t2pick-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        for n in ["10", "9", "2"] {
            std::fs::write(tmp.join(format!("bcts-t2@{n}.toml")), "x").unwrap();
        }
        std::fs::write(tmp.join("bcts@5.toml"), "x").unwrap();
        let got = latest_t2(&tmp).unwrap();
        assert!(got.to_string_lossy().ends_with("bcts-t2@10.toml"));
        std::fs::remove_dir_all(&tmp).unwrap();
        // 空目录 = None。
        let empty = std::env::temp_dir().join("bctsrun-t2pick-empty-nonexistent");
        assert!(latest_t2(&empty).is_none());
    }
}
