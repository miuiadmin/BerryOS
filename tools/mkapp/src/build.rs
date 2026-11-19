//! build 编排面（W1-Q27 五刀·产链换轨）：Python `mkapp.py build()` 的
//! Rust 镜像——staging 走查 → manifest 全量门禁 → P1 abi_revision 打戳
//! → entry 对表 → GUI-7 交叉门 → 出名门 → ELF 魔数门 → SHA256SUMS →
//! cpio(newc) 落盘 + 摘要行。单源对齐义务：改任一侧必须同步另一侧
//! （elf2flat/mkcpio/mkfat/mkimage 换轨同款纪律）。
//!
//! Python 件不退役（p3sim/gensourceindex/berry_assemble import 图根 +
//! CLI 对拍 oracle + selftest 语义钉板）；产链现役件 = 本 crate bin。

use std::fs;
use std::path::Path;

use crate::byi::{byi_validate, png_to_byi};
use crate::cpio::sha256_hex;
use crate::gate::{check_manifest_text, Reject};
use crate::{ver_str, ManifestView, ABI_REVISION_V1, ICON_FILES_MAX, UI_PATH_MAX, VIEWS_FILES_MAX};
pub const MANIFEST_MAX: usize = 1024;
pub const BAP_MAX: usize = 4 * 1024 * 1024;
pub const ICON_PREFIX: &str = "icons/";
pub const VIEWS_PREFIX: &str = "views/";

/// build 成功摘要（`--json` 信封与 Python `result` 舱镜像；本刀只产
/// 人读行——JSON 面随消费需求再落）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildSummary {
    pub out: String,
    pub bytes: usize,
    pub id: String,
    pub version: String,
    pub console: bool,
    pub stack_pages: u32,
    pub intent: String,
    pub compat: Option<String>,
    pub min_level: Option<u32>,
    pub abi_revision: u32,
    pub fs: Option<String>,
    pub net: Option<String>,
    pub icons: usize,
    pub views: usize,
    pub actions: usize,
}

impl BuildSummary {
    /// 人读摘要行（Python `line` 逐字段同构——grep 面消费不破）。
    pub fn line(&self) -> String {
        let mut s = format!(
            "mkapp: {} {}B {} v{} console={} stack={} intent={}",
            self.out,
            self.bytes,
            self.id,
            self.version,
            if self.console { "true" } else { "false" },
            self.stack_pages,
            self.intent
        );
        if let Some(c) = &self.compat {
            s.push_str(&format!(" compat={c}"));
            if let Some(ml) = self.min_level {
                s.push_str(&format!(" min_level={ml}"));
            }
        }
        s.push_str(&format!(" abi_revision={}", self.abi_revision));
        if let Some(fs) = &self.fs {
            s.push_str(&format!(" fs={fs}"));
        }
        if let Some(net) = &self.net {
            s.push_str(&format!(" net={net}"));
        }
        if self.icons != 0 || self.views != 0 {
            s.push_str(&format!(" icons={} views={}", self.icons, self.views));
        }
        if self.actions != 0 {
            s.push_str(&format!(" actions={}", self.actions));
        }
        s
    }
}

fn rej(msg: String, code: &'static str, where_: &str) -> Reject {
    Reject { message: msg, code, where_: Box::leak(where_.to_string().into_boxed_str()), line: None, jx_where: None, jx_expected: None, jx_got: None, jx_fix: None }
}

/// staging 目录条目（名， 数据）——排序前收集形。
type Item = (Vec<u8>, Vec<u8>);

fn read_to_vec(p: &Path) -> Result<Vec<u8>, Reject> {
    fs::read(p).map_err(|e| rej(format!("{}: 读失败 {e}", p.display()), "io.read", &p.display().to_string()))
}

/// staging icons/ 走查：恰 .png 源、扁平常规文件、≤帽、ASCII 产物名
/// ≤UI_PATH_MAX；逐个 png_to_byi 编码 + byi_validate 产面自验。
fn collect_icons(app_dir: &Path) -> Result<Vec<Item>, Reject> {
    let dir = app_dir.join("icons");
    let mut items = Vec::new();
    let rd = match fs::read_dir(&dir) {
        Ok(rd) => rd,
        // 无 icons/ = 零条目（Python os.path.isdir 同语义）
        Err(_) => return Ok(items),
    };
    let mut names: Vec<String> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    if names.len() > ICON_FILES_MAX {
        return Err(rej(
            format!("{}: 条目越帽 {ICON_FILES_MAX}（得 {}）", dir.display(), names.len()),
            "stage.cap",
            "icons/",
        ));
    }
    for name in &names {
        let p = dir.join(name);
        if !p.is_file() {
            return Err(rej(
                format!("{}: 应为扁平常规文件（得 {name}）", dir.display()),
                "stage.icons.file",
                &format!("icons/{name}"),
            ));
        }
        if !name.ends_with(".png") {
            return Err(rej(
                format!("{}: 恰收 .png 源（得 {name}）", dir.display()),
                "stage.icons.png",
                &format!("icons/{name}"),
            ));
        }
        let stem = &name[..name.len() - 4];
        if stem.is_empty() {
            return Err(rej(
                format!("{}: 空 stem（得 {name}）", dir.display()),
                "stage.icons.stem",
                &format!("icons/{name}"),
            ));
        }
        let entry = format!("{ICON_PREFIX}{stem}.byi");
        let entry_b = ascii_or_reject(&entry, &format!("icons/{name}"), "stage.icons.name")?;
        if entry_b.len() > UI_PATH_MAX {
            return Err(rej(
                format!("{}: 产物名 {entry} 超长（{}B > {UI_PATH_MAX}）", dir.display(), entry_b.len()),
                "stage.icons.len",
                &format!("icons/{name}"),
            ));
        }
        let png = read_to_vec(&p)?;
        let byi = png_to_byi(&png, name)?;
        byi_validate(&byi, &entry_b)?;
        // 应用规范 §3.3 忠告面（Python :745 同款）：w/h < 108 loud warn
        // 不拒——现役 32×32 demo 资产先于规格。
        let ihdr = png.get(16..29).unwrap_or(&[]);
        if ihdr.len() == 13 {
            let w = u32::from_be_bytes([ihdr[0], ihdr[1], ihdr[2], ihdr[3]]);
            let h = u32::from_be_bytes([ihdr[4], ihdr[5], ihdr[6], ihdr[7]]);
            if w < 108 || h < 108 {
                eprintln!(
                    "mkapp: warn icon {name} {w}x{h} < 108 源规格（规范 §3.3：建议 108×108 居中 + ≥18px 裕量，过小源放大即糊）"
                );
            }
        }
        items.push((entry_b, byi));
    }
    Ok(items)
}

/// staging views/ 走查：扁平常规文件、非空、ASCII 条目名
/// views/<名> ≤UI_PATH_MAX 且 > 前缀长。
fn collect_views(app_dir: &Path) -> Result<Vec<Item>, Reject> {
    let dir = app_dir.join("views");
    let mut items = Vec::new();
    let rd = match fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(items),
    };
    let mut names: Vec<String> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    if names.len() > VIEWS_FILES_MAX {
        return Err(rej(
            format!("{}: 条目越帽 {VIEWS_FILES_MAX}（得 {}）", dir.display(), names.len()),
            "stage.cap",
            "views/",
        ));
    }
    for name in &names {
        let p = dir.join(name);
        if !p.is_file() {
            return Err(rej(
                format!("{}: 应为扁平常规文件（得 {name}）", dir.display()),
                "stage.views.file",
                &format!("views/{name}"),
            ));
        }
        let entry = format!("{VIEWS_PREFIX}{name}");
        let entry_b = ascii_or_reject(&entry, &format!("views/{name}"), "stage.views.name")?;
        if entry_b.len() > UI_PATH_MAX || entry_b.len() <= VIEWS_PREFIX.len() {
            return Err(rej(
                format!("{}: 条目名 {entry} 空/超长（≤{UI_PATH_MAX}）", dir.display()),
                "stage.views.len",
                &format!("views/{name}"),
            ));
        }
        let data = read_to_vec(&p)?;
        if data.is_empty() {
            return Err(rej(
                format!("{}: 空文件（views/ 须非空——占位至少一行注释）", p.display()),
                "stage.views.empty",
                &format!("views/{name}"),
            ));
        }
        items.push((entry_b, data));
    }
    Ok(items)
}

fn ascii_or_reject(s: &str, where_: &str, code: &'static str) -> Result<Vec<u8>, Reject> {
    if s.is_ascii() {
        Ok(s.as_bytes().to_vec())
    } else {
        Err(rej(format!("{s}: 条目名须 ASCII"), code, where_))
    }
}

/// staging 顶层形状门：{manifest.toml, bin/, icons/?, views/?} 恰形——未知
/// 内容拒、bin/ 恰一常规文件。
fn stage_walk(app_dir: &Path) -> Result<(String, Vec<u8>, Vec<Item>, Vec<Item>), Reject> {
    let mf = app_dir.join("manifest.toml");
    let bin_dir = app_dir.join("bin");
    if !mf.is_file() || !bin_dir.is_dir() {
        return Err(rej(
            format!("{}: 应含 manifest.toml 与 bin/", app_dir.display()),
            "stage.shape",
            &app_dir.display().to_string(),
        ));
    }
    // 顶层未知内容门（manifest.toml/bin/icons/views 外皆拒）
    let mut extra: Vec<String> = Vec::new();
    if let Ok(rd) = fs::read_dir(app_dir) {
        for e in rd.filter_map(|e| e.ok()) {
            let n = e.file_name().to_string_lossy().into_owned();
            if !matches!(n.as_str(), "manifest.toml" | "bin" | "icons" | "views") {
                extra.push(n);
            }
        }
    }
    if !extra.is_empty() {
        extra.sort();
        return Err(rej(
            format!("{}: 未知内容 {extra:?}", app_dir.display()),
            "stage.unknown",
            &app_dir.display().to_string(),
        ));
    }
    let mut bins: Vec<String> = fs::read_dir(&bin_dir)
        .map_err(|e| rej(format!("{}: 读失败 {e}", bin_dir.display()), "io.read", "bin/"))?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    bins.sort();
    if bins.len() != 1 {
        // Python kwargs 镜像（--json 钉板类⑥）：where="bin/" / expected=1 /
        // got=len int。
        let n = bins.len();
        return Err(Reject::jx(
            format!("{}: 应恰一个文件， 得 {n}", bin_dir.display()),
            "stage.bin.count", "bin/",
            Some("1".to_string()), Some(format!("{n}")), None,
        ));
    }
    let bin_name = bins.remove(0);
    let bin_path = bin_dir.join(&bin_name);
    if !bin_path.is_file() {
        return Err(rej(
            format!("{}: 应为常规文件", bin_path.display()),
            "stage.bin.file",
            &format!("bin/{bin_name}"),
        ));
    }
    let icon_items = collect_icons(app_dir)?;
    let view_items = collect_views(app_dir)?;
    Ok((bin_name, Vec::new(), icon_items, view_items))
}

/// P1 abi_revision 打戳（Python build() :1050-1082 镜像）：未声明 = 打
/// ABI_REVISION_V1；注入位 = [compat] 节尾或新起节（[app] 与
/// [capabilities] 之间——规范形 marker `\n\n[capabilities]\n` 恰一）。
fn stamp_abi_revision(text: &str, rev: u32) -> Result<String, Reject> {
    let marker = "\n\n[capabilities]\n";
    if text.matches(marker).count() != 1 {
        return Err(rej(
            "mkapp 内部错：abi_revision 注入标记不唯一（规范形不变量破）".to_string(),
            "manifest.stamp.internal",
            "manifest.toml",
        ));
    }
    let inject = if text.contains("\n[compat]\n") {
        format!("\nabi_revision = {rev}\n\n[capabilities]\n")
    } else {
        format!("\n\n[compat]\nabi_revision = {rev}\n\n[capabilities]\n")
    };
    Ok(text.replacen(marker, &inject, 1))
}

/// GUI-7 交叉门（Python `_ui_cross_check` 镜像）：manifest 声明 ↔ 包内
/// icons//views/ 条目互指 + 帽。
pub fn ui_cross_check(
    m: &ManifestView<'_>,
    icon_names: &[&[u8]],
    view_names: &[&[u8]],
) -> Result<(), Reject> {
    if icon_names.len() > ICON_FILES_MAX {
        return Err(rej(
            format!("bap: icons/ 条目 {} 越帽 {ICON_FILES_MAX}", icon_names.len()),
            "bap.ui.cross",
            "icons/",
        ));
    }
    if view_names.len() > VIEWS_FILES_MAX {
        return Err(rej(
            format!("bap: views/ 条目 {} 越帽 {VIEWS_FILES_MAX}", view_names.len()),
            "bap.ui.cross",
            "views/",
        ));
    }
    let want_icon = m.ui.as_ref().and_then(|u| u.icon);
    match want_icon {
        None => {
            if !icon_names.is_empty() {
                return Err(rej(
                    format!("bap: 未声明 ui.icon 而包含 icons/ 条目 {icon_names:?}"),
                    "bap.ui.cross",
                    "ui.icon",
                ));
            }
        }
        Some(w) => {
            if !icon_names.iter().any(|n| *n == w.as_bytes()) {
                return Err(rej(
                    format!("bap: manifest ui.icon 不在包内（{w}）"),
                    "bap.ui.cross",
                    "ui.icon",
                ));
            }
        }
    }
    let want_views = m.ui.as_ref().and_then(|u| u.views);
    match want_views {
        None => {
            if !view_names.is_empty() {
                return Err(rej(
                    format!("bap: 未声明 ui.views 而包含 views/ 条目 {view_names:?}"),
                    "bap.ui.cross",
                    "ui.views",
                ));
            }
        }
        Some(w) => {
            if !view_names.iter().any(|n| *n == w.as_bytes()) {
                return Err(rej(
                    format!("bap: manifest ui.views 不在包内（{w}）"),
                    "bap.ui.cross",
                    "ui.views",
                ));
            }
        }
    }
    Ok(())
}

/// 打包（门禁 1-5 全过才落盘）。Python `build()` 逐门镜像——拒面 code
/// 三元组同构（消费端按舱分支不破）。
pub fn build(out_path: &str, app_dir: &str) -> Result<BuildSummary, Reject> {
    let app = Path::new(app_dir);
    let (bin_name, _elf_later, icon_items, view_items) = stage_walk(app)?;
    // manifest 读 + 帽 + ASCII 门
    let mf_path = app.join("manifest.toml");
    let raw = read_to_vec(&mf_path)?;
    if raw.len() > MANIFEST_MAX {
        return Err(rej(
            format!("manifest.toml 超 MANIFEST_MAX {MANIFEST_MAX}B（得 {}B）", raw.len()),
            "manifest.size",
            "manifest.toml",
        ));
    }
    let text = String::from_utf8(raw).map_err(|_| {
        rej("manifest.toml 非 ASCII".to_string(), "manifest.encoding", "manifest.toml")
    })?;
    // 注：from_utf8 收 UTF-8；Python decode("ascii") 更严——非 ASCII 字节
    // 在 from_utf8 通过时由下方 gate 键/值字符集门拒（值域词表 + 显式
    // 字符集校验），拒面等效。
    let m = check_manifest_text(&text)?;

    // P1 打戳
    let rev = m.abi_revision.unwrap_or(ABI_REVISION_V1);
    let stamped_text = if m.abi_revision.is_none() {
        stamp_abi_revision(&text, rev)?
    } else {
        text.clone()
    };
    let stamped = stamped_text.as_bytes().to_vec();
    if stamped.len() > MANIFEST_MAX {
        return Err(rej(
            format!("manifest.toml 打戳后 {}B 超 MANIFEST_MAX {MANIFEST_MAX}B", stamped.len()),
            "manifest.size.stamped",
            "manifest.toml",
        ));
    }
    // 打戳产物自证：全量门禁复跑（规范形回放）
    let m2 = check_manifest_text(&stamped_text)?;
    if m2.abi_revision != Some(rev) {
        return Err(rej(
            "mkapp 内部错：abi_revision 打戳回读不等（注入值漂移）".to_string(),
            "manifest.stamp.internal",
            "compat.abi_revision",
        ));
    }

    // 门禁 3：entry == bin/ 唯一文件
    if m.entry != format!("bin/{bin_name}") {
        // Python kwargs 镜像（--json 钉板类⑦）：expected="bin/<名>" / got=entry。
        let got = m.entry.to_string();
        return Err(Reject::jx(
            format!("manifest entry '{got}' != bin/'{bin_name}'"),
            "entry.mismatch", "app.entry",
            Some(format!("\"bin/{bin_name}\"")), Some(format!("\"{got}\"")), None,
        ));
    }
    // GUI-7 交叉门
    let n_icons = icon_items.len();
    let n_views = view_items.len();
    let icon_refs: Vec<&[u8]> = icon_items.iter().map(|(n, _)| n.as_slice()).collect();
    let view_refs: Vec<&[u8]> = view_items.iter().map(|(n, _)| n.as_slice()).collect();
    ui_cross_check(&m, &icon_refs, &view_refs)?;
    // 门禁 4：输出名 == <id>.bap
    let out_base = Path::new(out_path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if out_base != format!("{}.bap", m.id) {
        return Err(rej(format!("输出名应为 {}.bap", m.id), "output.name", &out_base));
    }
    // 门禁 5：ELF 魔数
    let elf = read_to_vec(&app.join("bin").join(&bin_name))?;
    if elf.len() < 4 || &elf[..4] != b"\x7fELF" {
        let got = if elf.len() >= 4 { hex(&elf[..4]) } else { hex(&elf) };
        // Python kwargs 镜像（--json 钉板类⑧）：expected="7f454c46" / got=hex 串。
        let where_static: &'static str = Box::leak(format!("bin/{bin_name}").into_boxed_str());
        return Err(Reject::jx(
            format!("bin/{bin_name}: 非 ELF（头 4 字节 {got}）"),
            "bin.elf", where_static,
            Some("\"7f454c46\"".to_string()), Some(format!("\"{got}\"")), None,
        ));
    }

    // 条目集：打戳后 manifest + ELF + icons + views → 排序（条目名字节序）
    let mut payload: Vec<Item> = vec![
        (b"manifest.toml".to_vec(), stamped),
        (m.entry.as_bytes().to_vec(), elf),
    ];
    payload.extend(icon_items);
    payload.extend(view_items);
    payload.sort();
    // SHA256SUMS（双空格、LF 收尾、行序 = 条目名字节序）+ 自身入包再排序
    let mut sums = String::new();
    for (name, data) in &payload {
        sums.push_str(&sha256_hex(data));
        sums.push_str("  ");
        sums.push_str(&String::from_utf8_lossy(name));
        sums.push('\n');
    }
    let mut items = payload;
    items.push((b"SHA256SUMS".to_vec(), sums.into_bytes()));
    items.sort();
    let blob = mkcpio::build_archive(&items);
    if blob.len() > BAP_MAX {
        return Err(rej(
            format!("包体 {}B 超 BAP_MAX {BAP_MAX}B", blob.len()),
            "bap.size",
            "bap",
        ));
    }
    fs::write(out_path, &blob)
        .map_err(|e| rej(format!("{}: 写失败 {e}", out_path), "io.write", out_path))?;

    let compat = m.compat.map(|(a, _, c)| format!("{}..{}", ver_str(a), ver_str(c)));
    Ok(BuildSummary {
        out: out_path.to_string(),
        bytes: blob.len(),
        id: m.id.to_string(),
        version: m.version.to_string(),
        console: m.console,
        stack_pages: m.stack_pages,
        intent: m.intent.to_string(),
        compat,
        min_level: m.min_level,
        abi_revision: rev,
        fs: m.fs.map(|s| s.to_string()),
        net: m.net.map(|s| s.to_string()),
        icons: n_icons,
        views: n_views,
        actions: m.agent_actions.len(),
    })
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
