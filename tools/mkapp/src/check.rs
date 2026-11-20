//! `--check` 五步复验面（W1-Q27 六刀）：Python `check_bap`（mkapp.py
//! :1245）的 Rust 镜像——§4.4 钉板五步读侧：
//! ① 包体帽 → ② cpio(newc) 走查 + 条目形（manifest 恰一 + 唯一 bin/ +
//! SHA256SUMS? + icons/*.byi ≤帽 + views/* ≤帽 + 其余为零 + 条目重名拒 +
//! icons/views 名形门 + .byi 全量重走）→ ③ manifest 全量门禁复跑 +
//! entry 对表 → ④ SHA256SUMS 复验（在场）→ ⑤ ELF 魔数；外加 GUI-7
//! 交叉门（build 同源复跑——manifest 声明 ↔ 包内条目互指）。
//!
//! 单源对齐义务：改任一侧必须同步另一侧（build.rs 五刀同款纪律）。
//! 拒因 code 族与 Python 同名（`bap.size`/`bap.cpio.*`/`bap.icons.name`/
//! `bap.views.*`/`manifest.*`/`entry.mismatch`/`bap.elf`/`bap.ui.cross`）。

use std::collections::HashSet;
use std::path::Path;

use crate::build::{ui_cross_check, BAP_MAX, MANIFEST_MAX};
use crate::byi::byi_validate;
use crate::cpio::{cpio_entries, verify_sums, BAP_MAX as CPIO_BAP_MAX};
use crate::gate::check_manifest_text;
use crate::gate::Reject;
use crate::{ICON_FILES_MAX, UI_PATH_MAX, VIEWS_FILES_MAX};

fn rej(msg: String, code: &'static str, where_: &str) -> Reject {
    Reject { message: msg, code, where_: Box::leak(where_.to_string().into_boxed_str()), line: None, jx_where: None, jx_expected: None, jx_got: None, jx_fix: None }
}

/// `--check` 结果摘要（Python `check_bap` 返回 dict 镜像：path/bytes/
/// entries/digests——`--json` result 舱与 OK 行数据源）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckSummary {
    pub bytes: usize,
    pub entries: usize,
    /// `"ok"` | `"n/a"`（SHA256SUMS 在场即复验并报 ok）。
    pub digests: &'static str,
}

impl CheckSummary {
    /// 人读 OK 行（Python `print(f"mkapp: {path} OK {len(blob)}B
    /// entries={len(entries)} digests={digests}")` 同构）。
    pub fn line(&self, path: &str) -> String {
        format!(
            "mkapp: {} OK {}B entries={} digests={}",
            path, self.bytes, self.entries, self.digests
        )
    }
}

/// `--check` 五步主面（Python `check_bap` 逐门镜像）。
pub fn check_bap(path: &str) -> Result<CheckSummary, Reject> {
    let p = Path::new(path);
    let blob = std::fs::read(p).map_err(|e| {
        rej(format!("{path}: 读失败 {e}"), "io.read", path)
    })?;
    if blob.len() > BAP_MAX {
        return Err(rej(
            format!("bap: 包体 {}B 超 BAP_MAX {CPIO_BAP_MAX}", blob.len()),
            "bap.size",
            "bap",
        ));
    }
    let entries = cpio_entries(&blob)?;
    check_entry_shape(&entries)?;

    let ent: std::collections::BTreeMap<Vec<u8>, Vec<u8>> = entries
        .iter()
        .cloned()
        .collect();
    // manifest 全量门禁复跑（含三值链/词表/规范形）
    let raw = &ent[b"manifest.toml".as_slice()];
    if raw.len() > MANIFEST_MAX {
        return Err(rej(
            format!("manifest.toml 超 MANIFEST_MAX {MANIFEST_MAX}B"),
            "manifest.size",
            "manifest.toml",
        ));
    }
    if !raw.is_ascii() {
        return Err(rej(
            "manifest.toml 非 ASCII".to_string(),
            "manifest.encoding",
            "manifest.toml",
        ));
    }
    let text = std::str::from_utf8(raw).map_err(|_| {
        rej("manifest.toml 非 ASCII".to_string(), "manifest.encoding", "manifest.toml")
    })?;
    let m = check_manifest_text(text)?;
    let bin_entry = entries
        .iter()
        .map(|(n, _)| n)
        .find(|n| n.starts_with(b"bin/"))
        .expect("shape 门已保证唯一 bin/ 在场");
    if m.entry.as_bytes() != bin_entry.as_slice() {
        return Err(rej(
            format!(
                "manifest entry {:?} != 包内条目 {:?}",
                m.entry,
                String::from_utf8_lossy(bin_entry)
            ),
            "entry.mismatch",
            "app.entry",
        ));
    }

    // ELF 魔数（步 5）
    let elf = &ent[bin_entry.as_slice()];
    if elf.len() < 4 || &elf[..4] != b"\x7fELF" {
        let got: String = elf.iter().take(4).map(|b| format!("{b:02x}")).collect();
        return Err(rej(
            format!(
                "bap: 非 ELF（头 4 字节 {}）",
                if elf.len() >= 4 {
                    elf[..4].iter().map(|b| format!("{b:02x}")).collect::<String>()
                } else {
                    got
                }
            ),
            "bap.elf",
            &String::from_utf8_lossy(bin_entry).into_owned(),
        ));
    }

    // GUI-7 交叉门（build 同源复跑）
    let icon_entries: Vec<&[u8]> = entries
        .iter()
        .map(|(n, _)| n.as_slice())
        .filter(|n| n.starts_with(b"icons/"))
        .collect();
    let view_entries: Vec<&[u8]> = entries
        .iter()
        .map(|(n, _)| n.as_slice())
        .filter(|n| n.starts_with(b"views/"))
        .collect();
    ui_cross_check(&m, &icon_entries, &view_entries)?;

    // SHA256SUMS 复验（步 4——在场才复验；缺席 = digests "n/a"）
    let mut digests = "n/a";
    if let Some(sums) = ent.get(b"SHA256SUMS".as_slice()) {
        let payload: Vec<(Vec<u8>, Vec<u8>)> = entries
            .iter()
            .filter(|(n, _)| n.as_slice() != b"SHA256SUMS")
            .cloned()
            .collect();
        verify_sums(sums, &payload)?;
        digests = "ok";
    }

    Ok(CheckSummary {
        bytes: blob.len(),
        entries: entries.len(),
        digests,
    })
}

/// 步 ② 条目形门（Python `check_bap` :1255-1307 镜像）：重名 / 缺
/// manifest / 条目形不符 / icons 名形 / views 名形与空文件 + .byi 全量
/// 重走（byi_validate）。
fn check_entry_shape(entries: &[(Vec<u8>, Vec<u8>)]) -> Result<(), Reject> {
    let mut seen: HashSet<&[u8]> = HashSet::new();
    let mut dup: Vec<&[u8]> = Vec::new();
    for (n, _) in entries {
        if !seen.insert(n.as_slice()) {
            dup.push(n.as_slice());
        }
    }
    if !dup.is_empty() {
        let mut sorted = dup.clone();
        sorted.sort();
        let names: Vec<String> = sorted
            .iter()
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .collect();
        return Err(rej(
            format!("bap: 条目重名 {names:?}"),
            "bap.cpio.dup",
            "cpio",
        ));
    }
    let names: Vec<&[u8]> = entries.iter().map(|(n, _)| n.as_slice()).collect();
    if !names.iter().any(|n| *n == b"manifest.toml") {
        return Err(rej(
            "bap: 缺 manifest.toml 条目".to_string(),
            "bap.cpio.entries",
            "manifest.toml",
        ));
    }
    let bin_n = names.iter().filter(|n| n.starts_with(b"bin/")).count();
    let icon_entries: Vec<(&[u8], &[u8])> = entries
        .iter()
        .filter(|(n, _)| n.starts_with(b"icons/"))
        .map(|(n, d)| (n.as_slice(), d.as_slice()))
        .collect();
    let view_entries: Vec<(&[u8], &[u8])> = entries
        .iter()
        .filter(|(n, _)| n.starts_with(b"views/"))
        .map(|(n, d)| (n.as_slice(), d.as_slice()))
        .collect();
    let known = |n: &[u8]| -> bool {
        n == b"manifest.toml"
            || n == b"SHA256SUMS"
            || n.starts_with(b"bin/")
            || n.starts_with(b"icons/")
            || n.starts_with(b"views/")
    };
    let others: Vec<&[u8]> = names.iter().copied().filter(|n| !known(n)).collect();
    if bin_n != 1 || icon_entries.len() > ICON_FILES_MAX || view_entries.len() > VIEWS_FILES_MAX || !others.is_empty() {
        let mut all: Vec<String> = names
            .iter()
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .collect();
        all.sort();
        return Err(rej(
            format!(
                "bap: 条目形不符（manifest.toml + 唯一 bin/ + SHA256SUMS? + icons/*.byi ≤{ICON_FILES_MAX} + views/* ≤{VIEWS_FILES_MAX}），得 {all:?}"
            ),
            "bap.cpio.shape",
            "cpio",
        ));
    }
    for (n, d) in &icon_entries {
        let tail = &n[b"icons/".len()..];
        if !tail.ends_with(b".byi") || tail.contains(&b'/') || n.len() > UI_PATH_MAX {
            return Err(rej(
                format!(
                    "bap: icons/ 条目名形不符 {:?}（扁平 .byi、≤{UI_PATH_MAX}B）",
                    String::from_utf8_lossy(n)
                ),
                "bap.icons.name",
                &String::from_utf8_lossy(n).into_owned(),
            ));
        }
        byi_validate(d, n)?;
    }
    for (n, d) in &view_entries {
        let tail = &n[b"views/".len()..];
        if tail.is_empty() || tail.contains(&b'/') || n.len() > UI_PATH_MAX {
            return Err(rej(
                format!(
                    "bap: views/ 条目名形不符 {:?}（扁平、≤{UI_PATH_MAX}B）",
                    String::from_utf8_lossy(n)
                ),
                "bap.views.name",
                &String::from_utf8_lossy(n).into_owned(),
            ));
        }
        if d.is_empty() {
            return Err(rej(
                format!("bap: views/ 条目 {:?} 为空", String::from_utf8_lossy(n)),
                "bap.views.empty",
                &String::from_utf8_lossy(n).into_owned(),
            ));
        }
    }
    Ok(())
}
