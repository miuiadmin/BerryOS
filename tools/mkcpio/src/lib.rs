//! mkcpio——cpio newc 打包器 Rust 件（彻底 Rust 化轨道 b 层 #8 刀序首位，
//! 钉板五步 2/3/4：独立小工作区 + 字节级对拍 + selftest；产链换轨留二刀）。
//!
//! Python 现役件 `tools/mkcpio.py` 逐面对齐（docs/彻底Rust化计划-v1.0.md §5）：
//! - 纯编码面：`entry` / `build_archive`（不排序、ino=传入序+1、TRAILER+
//!   尾垫页；垫料 X-3c 4096——头+名与数据都垫到页，消费端五点同口径）；
//! - 切分面：`split_volumes`（只切字节流不对齐条目）/ `volume_paths` /
//!   `clean_stale_volumes`（VOL_SCAN_MAX=10 陈卷清理）；
//! - manifest 面：TOML 子集解析（version=1 + [entries] 表，拒面同 Python
//!   load_manifest）+ `merge_items`（双源撞名 loud 拒）；
//! - 伴生面：`sig_companions`（.bap 伴生 .sig 收编三条硬性质）。
//!
//! .berryver 烙印面（git ident 降级链 + abi.toml/Cargo.toml 双单源 + uname
//! 面对拍）在 Python 件由宿主侧 subprocess/git 完成——Rust 首刀只钉**纯
//! 编码面**，烙印归 CLI 层（二刀换轨时随产链一并迁）。

#![forbid(unsafe_code)]

use std::fmt;

/// 垫料粒度（X-3c）：头+名与数据都垫到 4096——条目长恒页倍数 → 每条
/// data_off 页对齐。消费端同口径五点：berry-loader cpio_walk/cpio_step +
/// mkapp _cpio_entries + kernel bootparams + Python mkcpio（单编码器纪律）。
pub const CPIO_PAD: usize = 4096;

/// 内核 fw_cfg 连续前缀发现域（A1 §3.2）：opt/berry/rootfs0..9——陈卷清理
/// 执法范围与之同口径。
pub const VOL_SCAN_MAX: usize = 10;

const VOL_SUFFIX: &str = ".cpio";
const SIG_SUFFIX: &str = ".sig";

/// 编码/切分/manifest 面错误（Python 侧 SystemExit/stderr 面的 Rust 化）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MkcpioError {
    /// 非正卷宽（split_volumes 拒面）。
    BadVolBytes,
    /// manifest 拒面（load_manifest 全谱）。
    Manifest(String),
    /// 双源撞名（merge_items / sig 面让位后的归档名唯一性）。
    DupNames(Vec<String>),
}

impl fmt::Display for MkcpioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MkcpioError::BadVolBytes => write!(f, "vol_bytes must be positive"),
            MkcpioError::Manifest(m) => write!(f, "mkcpio: FAIL: {m}"),
            MkcpioError::DupNames(d) => write!(f, "mkcpio: FAIL: 条目撞名 {d:?}"),
        }
    }
}

impl std::error::Error for MkcpioError {}

fn hex8(v: usize) -> [u8; 8] {
    let mut out = [b'0'; 8];
    let mut v = v;
    for i in (0..8).rev() {
        out[i] = b"0123456789ABCDEF"[v & 0xF];
        v >>= 4;
    }
    out
}

/// 单条 newc 条目（Python `entry()` 逐字节对齐）：magic + 13 × hex8 字段
/// （ino mode uid gid nlink mtime filesize devmaj devmin rdevmaj rdevmin
/// namesize check）+ name + NUL，头+名垫到页；data 后垫到页。
/// 字段全定值（mtime=0/uid=gid=0/mode 0755/nlink 1）——归档字节只随内容变。
pub fn entry(name: &[u8], data: &[u8], ino: usize) -> Vec<u8> {
    let namesize = name.len() + 1; // 含 NUL
    let mut out = Vec::with_capacity(CPIO_PAD.max(110 + namesize + data.len()));
    out.extend_from_slice(b"070701");
    for v in [
        ino,        // ino
        0o100755,   // mode
        0,          // uid
        0,          // gid
        1,          // nlink
        0,          // mtime
        data.len(), // filesize
        0,          // devmaj
        0,          // devmin
        0,          // rdevmaj
        0,          // rdevmin
        namesize,   // namesize
        0,          // check
    ] {
        out.extend_from_slice(&hex8(v));
    }
    out.extend_from_slice(name);
    out.push(0);
    while out.len() % CPIO_PAD != 0 {
        out.push(0);
    }
    out.extend_from_slice(data);
    while out.len() % CPIO_PAD != 0 {
        out.push(0);
    }
    out
}

/// 纯编码器（app-0 §4 单编码器纪律）：不排序、按传入序编码、ino = 传入
/// 序号 + 1——排序归调用方。TRAILER!!! + 尾垫页在内。
pub fn build_archive(items: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut blob = Vec::new();
    for (i, (name, data)) in items.iter().enumerate() {
        let e = entry(name, data, i + 1);
        blob.extend_from_slice(&e);
    }
    blob.extend_from_slice(&entry(b"TRAILER!!!", b"", 0));
    while blob.len() % CPIO_PAD != 0 {
        blob.push(0);
    }
    blob
}

/// 纯切分器（容量工程前置-A1 §3.3）：只切字节流、不做条目对齐——TRAILER!!!
/// 自然只在末卷。重装恒等 = volumes.concat() == blob；恰整数倍无空尾卷。
pub fn split_volumes(blob: &[u8], vol_bytes: usize) -> Result<Vec<Vec<u8>>, MkcpioError> {
    if vol_bytes == 0 {
        return Err(MkcpioError::BadVolBytes);
    }
    Ok(blob.chunks(vol_bytes).map(<[u8]>::to_vec).collect())
}

/// 编号卷路径：`rootfs.cpio` → `rootfs0.cpio/rootfs1.cpio/…`；无后缀直拼
/// （`out` → `out0.cpio`）；目录分量保留。与 fw_cfg 具名文件同一序号口径。
pub fn volume_paths(out_path: &str, n: usize) -> Vec<String> {
    let stem = out_path.strip_suffix(VOL_SUFFIX).unwrap_or(out_path);
    (0..n).map(|i| format!("{stem}{i}{VOL_SUFFIX}")).collect()
}

/// 清陈卷（A1-3b 产链卫生）：写卷前 unlink 序号 ≥keep 的卷（ENOENT 静默
/// ——常态即不存在）；返回实删数。范围 [keep, VOL_SCAN_MAX)。
pub fn clean_stale_volumes(out_path: &str, keep: usize) -> usize {
    let stem = out_path.strip_suffix(VOL_SUFFIX).unwrap_or(out_path);
    let mut removed = 0;
    for idx in keep..VOL_SCAN_MAX {
        if std::fs::remove_file(format!("{stem}{idx}{VOL_SUFFIX}")).is_ok() {
            removed += 1;
        }
    }
    removed
}

// ─── V13（ADR #82 预埋刀）：条目映射数据文件加载面 ─────────────────────

/// rootfs-manifest.toml → {name: path} 条目表（V13）。schema 与拒面同
/// Python load_manifest：version=1 / 必有非空 [entries] / 坏条目名（空、
/// 绝对路径、含 `=`）/ 坏路径（非字符串、空）/ 坏 TOML / 读失败，一律
/// `MkcpioError::Manifest` loud。TOML 解析 = 零依赖子集（本表语法域：
/// 顶层 `version = 1` + `[entries]` 节内 `"name" = "path"` 行——现役
/// tools/rootfs-manifest.toml 全量实测过）。
pub fn load_manifest(text: &str) -> Result<Vec<(String, String)>, MkcpioError> {
    let mut version: Option<i64> = None;
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut in_entries = false;
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            if line == "[entries]" {
                in_entries = true;
            } else {
                return Err(MkcpioError::Manifest(format!(
                    "未知节 {line}（收 [entries]——berry-manifest 未知键硬拒同纪律）"
                )));
            }
            continue;
        }
        let Some(eq) = line.find('=') else {
            return Err(MkcpioError::Manifest(format!(
                "第 {} 行非赋值：{line}",
                lineno + 1
            )));
        };
        let key = line[..eq].trim();
        let val = line[eq + 1..].trim();
        if in_entries {
            let name = unquote(key)?;
            let path = unquote(val)?;
            if name.is_empty() || name.starts_with('/') || name.contains('=') {
                return Err(MkcpioError::Manifest(format!(
                    "坏条目名 {name:?}（空/绝对路径/含 = 拒——CLI name=path 同口径）"
                )));
            }
            if path.is_empty() {
                return Err(MkcpioError::Manifest(format!(
                    "条目 {name:?} 坏路径 {path:?}（须非空字符串）"
                )));
            }
            entries.push((name, path));
        } else if key == "version" {
            version = Some(
                val.parse::<i64>().map_err(|_| {
                    MkcpioError::Manifest(format!("version 非整数：{val:?}"))
                })?,
            );
        } else {
            return Err(MkcpioError::Manifest(format!(
                "未知顶层键 {key}（收 version/entries——berry-manifest 未知键硬拒同纪律）"
            )));
        }
    }
    if version != Some(1) {
        return Err(MkcpioError::Manifest(format!(
            "version 须 = 1（got {version:?}）"
        )));
    }
    if entries.is_empty() {
        return Err(MkcpioError::Manifest("缺非空 [entries] 条目表".into()));
    }
    // 表键去重（TOML 表键天然去重——Python 侧由 tomllib 解析错兜底，Rust
    // 子集解析面在此显式拒）。
    let mut seen = std::collections::BTreeSet::new();
    for (name, _) in &entries {
        if !seen.insert(name.clone()) {
            return Err(MkcpioError::Manifest(format!(
                "坏 TOML：[entries] 重复键 {name:?}"
            )));
        }
    }
    Ok(entries)
}

fn unquote(s: &str) -> Result<String, MkcpioError> {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        Ok(s[1..s.len() - 1].to_string())
    } else {
        Err(MkcpioError::Manifest(format!("值非带引号字符串：{s:?}")))
    }
}

/// --manifest 条目表与位置参数 name=path 合并（V13）：返回未排序合并列表
/// （排序仍归 CLI 的整条字符串排序——确定性语义单点）。双源撞名 loud 拒。
pub fn merge_items(positional: &[String], manifest: &[(String, String)]) -> Result<Vec<String>, MkcpioError> {
    let mut items: Vec<String> = positional.to_vec();
    items.extend(manifest.iter().map(|(n, p)| format!("{n}={p}")));
    let names: Vec<&str> = items.iter().map(|i| i.split('=').next().unwrap_or("")).collect();
    let mut dups: Vec<String> = names
        .iter()
        .filter(|n| names.iter().filter(|m| *m == *n).count() > 1)
        .map(|n| n.to_string())
        .collect();
    dups.sort();
    dups.dedup();
    if !dups.is_empty() {
        return Err(MkcpioError::DupNames(dups));
    }
    Ok(items)
}

/// .bap 条目的伴生 .sig 自动收编（P3 数据面案 a）：`name=path` 且 name 以
/// .bap 结尾、构建树上 `path.sig` 在席、且条目表中无显式 `name.sig` 条目
/// 时追加。三条硬性质：同名+.sig 直映 / 显式胜出 / 缺席零漂移。
/// 在席性由调用方注入（`exists` 闭包——纯函数面不持 IO）。
pub fn sig_companions<F>(items: &[String], exists: F) -> Vec<String>
where
    F: Fn(&str) -> bool,
{
    let explicit: std::collections::HashSet<&str> =
        items.iter().map(|it| it.split('=').next().unwrap_or("")).collect();
    let mut out = Vec::new();
    for it in items {
        let Some((name, path)) = it.split_once('=') else { continue };
        if !name.ends_with(".bap") {
            continue;
        }
        let cname = format!("{name}{SIG_SUFFIX}");
        if explicit.contains(cname.as_str()) {
            continue;
        }
        let cpath = format!("{path}{SIG_SUFFIX}");
        if exists(&cpath) {
            out.push(format!("{cname}={cpath}"));
        }
    }
    out
}

/// 单元测试：纯面快速钉板（重装恒等/垫页几何/manifest 拒面抽测）——
/// 全面对拍在 tests/selftest.rs（与 Python oracle 字节级对拍）。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex8_pad_and_caps() {
        assert_eq!(hex8(0), *b"00000000");
        assert_eq!(hex8(1), *b"00000001");
        assert_eq!(hex8(0o100755), *b"000081ED");
        assert_eq!(hex8(0xABCDEF1), *b"0ABCDEF1");
    }

    #[test]
    fn split_reassemble_identity() {
        let blob: Vec<u8> = (0..1000u32).map(|k| (k % 251) as u8).collect();
        for vol in [1usize, 7, 64, 999, 1000, 1001, 4096] {
            let vols = split_volumes(&blob, vol).unwrap();
            let joined = vols.concat();
            assert_eq!(joined, blob, "重装恒等破 @vol={vol}");
            assert!(vols.iter().all(|v| v.len() <= vol), "卷越界 @vol={vol}");
            assert!(!vols.is_empty() && !vols.last().unwrap().is_empty(), "空尾卷 @vol={vol}");
        }
        assert_eq!(split_volumes(&blob, 0), Err(MkcpioError::BadVolBytes));
    }

    #[test]
    fn volume_paths_faces() {
        assert_eq!(volume_paths("rootfs.cpio", 3), ["rootfs0.cpio", "rootfs1.cpio", "rootfs2.cpio"]);
        assert_eq!(volume_paths("build/rootfs.cpio", 2), ["build/rootfs0.cpio", "build/rootfs1.cpio"]);
        assert_eq!(volume_paths("out", 1), ["out0.cpio"]);
    }

    #[test]
    fn manifest_reject_faces() {
        let good = "version = 1\n[entries]\n\"bin/aa\" = \"x/aa\"\n\"bb\" = \"y/bb\"\n";
        assert_eq!(
            load_manifest(good).unwrap(),
            vec![("bin/aa".into(), "x/aa".into()), ("bb".into(), "y/bb".into())]
        );
        for (bad, why) in [
            ("[entries\n", "坏 TOML 语法"),
            ("extra = 1\nversion = 1\n[entries]\n\"a\" = \"b\"\n", "未知顶层键"),
            ("version = 2\n[entries]\n\"a\" = \"b\"\n", "version ≠ 1"),
            ("version = \"1\"\n[entries]\n\"a\" = \"b\"\n", "version 非整数"),
            ("[entries]\n\"a\" = \"b\"\n", "缺 version"),
            ("version = 1\n", "缺 entries"),
            ("version = 1\n[entries]\n", "空 entries"),
            ("version = 1\n[entries]\n\"\" = \"b\"\n", "空条目名"),
            ("version = 1\n[entries]\n\"/abs\" = \"b\"\n", "绝对路径名"),
            ("version = 1\n[entries]\n\"a=b\" = \"c\"\n", "名含 ="),
            ("version = 1\n[entries]\n\"a\" = \"\"\n", "空路径"),
            ("version = 1\n[entries]\n\"a\" = 3\n", "路径非字符串"),
        ] {
            assert!(load_manifest(bad).is_err(), "拒面未咬：{why}");
        }
        assert!(load_manifest("version = 1\n[entries]\n\"a\" = \"b\"\n\"a\" = \"c\"\n").is_err(), "重复键未拒");
    }

    #[test]
    fn merge_dup_rejected() {
        let m = vec![("bin/aa".to_string(), "x/aa".to_string())];
        assert_eq!(
            merge_items(&["bin/cc=z/cc".into()], &m).unwrap(),
            ["bin/cc=z/cc", "bin/aa=x/aa"]
        );
        assert_eq!(
            merge_items(&["bin/aa=other/aa".into()], &m),
            Err(MkcpioError::DupNames(vec!["bin/aa".into()]))
        );
    }

    #[test]
    fn sig_companion_faces() {
        // 布景：a.bap+伴生（直映）、bin/zz+stray（非 .bap 不收）、b.bap+
        // 显式 .sig 条目（显式胜出）。
        let items = vec![
            "app/a.bap=td/a.bap".to_string(),
            "bin/zz=td/zz".to_string(),
            "app/b.bap=td/b.bap".to_string(),
            "app/b.bap.sig=td/explicit.sig".to_string(),
        ];
        let out = sig_companions(&items, |p| p == "td/a.bap.sig");
        assert_eq!(out, ["app/a.bap.sig=td/a.bap.sig"], "直映且只直映");
        // 缺席零漂移：伴生不在席 → 零追加。
        assert!(sig_companions(&items, |_| false).is_empty());
    }
}
