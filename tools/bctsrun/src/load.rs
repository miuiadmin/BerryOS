//! bctsrun 七刀：装载/守卫段镜像（现役单源 = `tools/bcts-run.sh` :566-629
//! 步①装载清单 + 步②空集守卫 + `--image` 腿装载守卫 + `--dry` 回显，彻底
//! Rust 化轨道 b 层 #7 刀序末档七刀）。
//!
//! 输入 = 六刀 `Resolved` 快照（Tier 1——Tier 2 分流在装载段之前，本刀不
//! 涉），出口协议两段：
//! - `load_entries`：装载（frozen → `check_t1` TSV；live →
//!   `build_live_tsv`）+ 计数（N_MARKERS/N_GATES/bare 相）+ 步②空集守卫
//!   （frozen floor 取 META 行第 3 域；live 取在册常量 91）+ 镜像腿守卫
//!   （bare 相 ≥32 + manifest 在席 + PR 锚〔frozen = META 第 2 域；live =
//!   abi.toml level〕+ `img_pr_check` 对账）。Err = (stderr 文案, 退码)
//!   逐字镜像；Ok 附装载期 stdout 行（img_pr_check ok 行——bash 侧在
//!   回显前打印）。
//! - `dry_echo`：`--dry` 回显逐行（模式行/标记计数行/platform_revision
//!   行〔frozen〕/镜像腿行/`  marker %-9s %-10s` /`  gate %-10s` AWK
//!   printf 形/dry 完行）。
//!
//! 已知边界（真跑钉板）：manifest 路径 = `${IMAGE_FILE%/*}/${MANIFEST_NAME}`
//! 纯词面拼接（非 canonicalize）——绝对形 image 无斜线时前缀空串
//! （`/name`），与 bash `${var%/*}` 同形。

use crate::abipr::{abi_level, img_pr_check};
use crate::args::{Mode, Resolved};
use crate::live::build_live_tsv;
use crate::{check_t1, parse_frozen_toml, Reject, TomlError};

/// 活树自检态守卫下限（bcts-run.sh :102 在册常量 91 = 裸 32 + rootfs 56
/// + RX 3；镜像腿 bare 相下限 32 = :107）。
pub const TIER1_FLOOR_LIVE: usize = 91;
pub const TIER1_BARE_FLOOR: usize = 32;

/// 装载完成态（dry 回显与后续步③④的输入快照）。
#[derive(Debug, Clone, PartialEq)]
pub struct LoadOutcome {
    /// 步①出口协议 TSV 行（META/MARK/GATE）。
    pub entries: Vec<String>,
    pub n_markers: usize,
    pub n_gates: usize,
    /// 步②守卫所用 floor（frozen = META 第 3 域；live = 91）。
    pub floor: i64,
    /// bare 相计数（无 --image 恒 0）。
    pub n_bare: usize,
    /// 版本对账锚 pr（无 --image 恒 None）。
    pub img_pr: Option<i64>,
    pub img_pr_anchor: Option<String>,
    /// 装载期已发 stdout 行（img_pr_check ok 行——先于回显行）。
    pub pre_stdout: Vec<String>,
}

fn fail(msg: String, code: i32) -> (String, i32) {
    (msg, code)
}

/// 步②空集守卫文案（bcts-run.sh :582 逐字）。
fn floor_guard_msg(n: usize, floor: i64) -> String {
    format!("bcts: FAIL: 空集守卫——清单 {n} 件 < tier1_floor {floor}（防假绿；设计稿 §4.2-②）")
}

/// 镜像腿 manifest 路径（`${IMAGE_FILE%/*}/${MANIFEST_NAME}` 词面拼接）。
pub fn manifest_path(image_file: &str, manifest_name: &str) -> String {
    let dir = image_file.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    format!("{dir}/{manifest_name}")
}

/// 步①装载 + 步②守卫 + 镜像腿装载守卫（Tier 1）。`repo` = 仓库根绝对形。
pub fn load_entries(r: &Resolved, repo: &str) -> Result<LoadOutcome, (String, i32)> {
    debug_assert!(r.tier == 1, "Tier 2 分流在装载段之前（bcts-run.sh :556）");

    let entries: Vec<String> = match r.mode {
        Mode::Frozen => {
            let text = std::fs::read_to_string(&r.frozen_file).map_err(|e| {
                fail(
                    format!("bcts: FAIL: 读 {} 失败：{}", r.frozen_file, e),
                    1,
                )
            })?;
            let doc = parse_frozen_toml(&text)
                .map_err(|TomlError(m)| fail(format!("bcts: REJECT: {m}"), 2))?;
            check_t1(&doc, &r.frozen_file).map_err(|Reject(m)| fail(m, 2))?
        }
        Mode::Live => {
            let src = format!("{repo}/tools/markers.sh");
            let text = std::fs::read_to_string(&src)
                .map_err(|e| fail(format!("bcts: FAIL: 读 {src} 失败：{e}"), 1))?;
            build_live_tsv(&text, &src)
                .map_err(|Reject(m)| fail(m, 2))?
                .lines()
                .map(|l| l.to_string())
                .collect()
        }
    };

    let n_markers = entries.iter().filter(|l| l.starts_with("MARK\t")).count();
    let n_gates = entries.iter().filter(|l| l.starts_with("GATE\t")).count();
    // META 域（frozen 态 pr/floor 锚；live 态 META 行不在场——build_live_tsv
    // 首行即 MARK，meta 恒空）。
    let meta: Vec<String> = entries[0]
        .strip_prefix("META\t")
        .map(|m| m.split('\t').map(|s| s.to_string()).collect())
        .unwrap_or_default();

    // 步②空集守卫（frozen floor = META 第 3 域；live = 在册 91）。
    let floor = match r.mode {
        Mode::Frozen => meta[1].parse::<i64>().unwrap_or(0),
        Mode::Live => TIER1_FLOOR_LIVE as i64,
    };
    if (n_markers as i64) < floor {
        return Err(fail(floor_guard_msg(n_markers, floor), 1));
    }

    let mut out = LoadOutcome {
        entries,
        n_markers,
        n_gates,
        floor,
        n_bare: 0,
        img_pr: None,
        img_pr_anchor: None,
        pre_stdout: Vec::new(),
    };

    // ---- --image 腿装载（G3）：bare 相空集守卫 + 版本声称位对账 ----
    if !r.image_file.is_empty() {
        out.n_bare = out
            .entries
            .iter()
            .filter(|l| l.starts_with("MARK\tbare\t"))
            .count();
        if out.n_bare < TIER1_BARE_FLOOR {
            return Err(fail(format!(
                "bcts: FAIL: 镜像腿空集守卫——bare 相 {} 件 < {TIER1_BARE_FLOOR}（防假绿；rootfs/RX 相 EFI 路结构性缺席，bare 相是镜像腿唯一回放面）",
                out.n_bare
            ), 1));
        }
        let img_manifest = manifest_path(&r.image_file, &r.manifest_name);
        if !std::path::Path::new(&img_manifest).is_file() {
            return Err(fail(format!(
                "bcts: FAIL: 镜像同目录无 {}：{}（mkimage 件 3 侧产物——缺席即非平台产链镜像；防互踩名镜像加 --manifest-name 指 mkimage 同名，G8 联动）",
                r.manifest_name, img_manifest
            ), 1));
        }
        let (pr, anchor) = match r.mode {
            Mode::Frozen => (
                meta[0].parse::<i64>().unwrap_or(0),
                format!("冻结件 platform_revision（{}）", r.frozen_file),
            ),
            Mode::Live => {
                let abi_toml = format!("{repo}/tools/abi.toml");
                let lv = abi_level(&abi_toml).map_err(|m| fail(m, 1))?;
                (
                    lv,
                    "tools/abi.toml [platform].level（活树现值，mkimage 取值同源）".to_string(),
                )
            }
        };
        let ok_line = img_pr_check(&img_manifest, pr, &anchor).map_err(|m| fail(m, 1))?;
        out.pre_stdout.push(ok_line);
        out.img_pr = Some(pr);
        out.img_pr_anchor = Some(anchor);
    }
    Ok(out)
}

/// `--dry` 回显（装载已过；返回 stdout 逐行——含装载期 pre_stdout 行）。
pub fn dry_echo(r: &Resolved, o: &LoadOutcome) -> Vec<String> {
    let mut lines = o.pre_stdout.clone();
    let list = if r.frozen_file.is_empty() {
        "tools/markers.sh（活树单源）".to_string()
    } else {
        r.frozen_file.clone()
    };
    lines.push(format!("bcts: 模式 = {}；清单 = {list}", r.mode));
    lines.push(format!(
        "bcts: 标记 {} 件（≥ tier1_floor {}）；host 门禁 {} 个",
        o.n_markers, o.floor, o.n_gates
    ));
    if r.mode == Mode::Frozen {
        let m: Vec<&str> = o.entries[0].split('\t').collect();
        lines.push(format!(
            "bcts: platform_revision = {}；frozen_at = {}；abi_snapshot = {}（{}）",
            m[1], m[5], m[6], m[7]
        ));
    }
    if !r.image_file.is_empty() {
        lines.push(format!(
            "bcts: 镜像腿 = EFI 镜像流（AAVMF + {}）；manifest = {}；回放面 = bare 相 {} 件 + EFI 十段/接续结构面；版本对账锚 = {}（pr={}，已对账 ok）",
            r.image_file, r.manifest_name, o.n_bare,
            o.img_pr_anchor.clone().unwrap_or_default(),
            o.img_pr.unwrap_or(0)
        ));
    }
    for e in &o.entries {
        let f: Vec<&str> = e.split('\t').collect();
        match f[0] {
            "MARK" => lines.push(format!("  marker {:<9} {:<10} {}", f[1], f[2], f[3])),
            "GATE" => lines.push(format!("  gate   {:<10} {}", f[1], f[2])),
            _ => {}
        }
    }
    if !r.image_file.is_empty() {
        lines.push("bcts: dry 完（镜像腿判定式 = 观测标记 ⊇ bare 相清单 + EFI 结构面在席；rootfs/RX 相留标准流——EFI 路无 virtio/simdev 面；去 --dry 跑 EFI boot 回放）".to_string());
    } else {
        lines.push("bcts: dry 完（判定式 = 观测标记 ⊇ 清单全集；多打不算 fail）".to_string());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_guard_text() {
        assert_eq!(
            floor_guard_msg(90, 91),
            "bcts: FAIL: 空集守卫——清单 90 件 < tier1_floor 91（防假绿；设计稿 §4.2-②）"
        );
    }

    #[test]
    fn manifest_path_word_join() {
        assert_eq!(manifest_path("/a/b/std.img", "manifest.txt"), "/a/b/manifest.txt");
        // 无斜线绝对形：bash ${var%/*} → 空前缀。
        assert_eq!(manifest_path("/std.img", "m.txt"), "/m.txt");
        assert_eq!(manifest_path("/x/y/z.img", "acme.manifest.txt"), "/x/y/acme.manifest.txt");
    }

    #[test]
    fn live_floor_fail_guard_via_fake_repo() {
        // live 态步② FAIL 守卫（frozen 态 floor 守卫在 check_t1 REJECT 面先发，
        // 步② FAIL 守卫对 frozen 不可达——bash 同序）。合成小 repo 触发。
        let dir = std::env::temp_dir().join(format!("bctsrun-loadguard-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("tools")).unwrap();
        std::fs::write(
            dir.join("tools/markers.sh"),
            "MARKERS=(\"a\" \"b\")\nROOTFS_MARKERS=(\"r\")\nRX_MARKERS=(\"x\")\nECO_MARKERS=(\"e\")\n",
        )
        .unwrap();
        let r = Resolved {
            dry: true,
            selftest: false,
            tier: 1,
            mode: Mode::Live,
            frozen_file: String::new(),
            image_file: String::new(),
            manifest_name: "manifest.txt".into(),
            manifest_name_explicit: false,
        };
        let err = load_entries(&r, dir.to_str().unwrap()).unwrap_err();
        assert_eq!(err, (
            "bcts: FAIL: 空集守卫——清单 4 件 < tier1_floor 91（防假绿；设计稿 §4.2-②）".to_string(),
            1
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dry_echo_shapes() {
        let r = Resolved {
            dry: true,
            selftest: false,
            tier: 1,
            mode: Mode::Live,
            frozen_file: String::new(),
            image_file: String::new(),
            manifest_name: "manifest.txt".into(),
            manifest_name_explicit: false,
        };
        let o = LoadOutcome {
            entries: vec![
                "META\t1\t91\t2\t1\tsha\ttools/abi.toml\tsha256:x".into(),
                "MARK\tbare\tbare-01\tBerryOS boot".into(),
                "MARK\trootfs\trootfs-01\tEPL ok n=8".into(),
                "GATE\tabi-check\tjust abi-check".into(),
            ],
            n_markers: 2,
            n_gates: 1,
            floor: 91,
            n_bare: 0,
            img_pr: None,
            img_pr_anchor: None,
            pre_stdout: vec![],
        };
        let l = dry_echo(&r, &o);
        assert_eq!(l[0], "bcts: 模式 = live；清单 = tools/markers.sh（活树单源）");
        assert_eq!(l[1], "bcts: 标记 2 件（≥ tier1_floor 91）；host 门禁 1 个");
        // AWK printf %-9s/%-10s 对齐律。
        assert_eq!(l[2], "  marker bare      bare-01    BerryOS boot");
        assert_eq!(l[3], "  marker rootfs    rootfs-01  EPL ok n=8");
        assert_eq!(l[4], "  gate   abi-check  just abi-check");
        assert_eq!(*l.last().unwrap(), "bcts: dry 完（判定式 = 观测标记 ⊇ 清单全集；多打不算 fail）");
    }

    #[test]
    fn dry_echo_frozen_meta_line() {
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
        let o = LoadOutcome {
            entries: vec![
                "META\t6\t91\t91\t1\t51932bff\ttools/abi.toml\tsha256:d47c".into(),
                "GATE\tabi-check\tjust abi-check".into(),
            ],
            n_markers: 0,
            n_gates: 1,
            floor: 91,
            n_bare: 0,
            img_pr: None,
            img_pr_anchor: None,
            pre_stdout: vec![],
        };
        let l = dry_echo(&r, &o);
        assert_eq!(
            l[2],
            "bcts: platform_revision = 6；frozen_at = 51932bff；abi_snapshot = tools/abi.toml（sha256:d47c）"
        );
        assert!(l[0].ends_with("清单 = /repo/tools/bcts/frozen/bcts@6.toml"));
    }
}
