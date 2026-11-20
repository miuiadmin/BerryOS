//! W1 gensourceindex 三刀第二片·生成面库刀：`load_app_entries`/
//! `build_entry`/`do_generate` 的 rootfs-manifest 路（Python
//! tools/gensourceindex.py :626-800 段镜像）。依赖全走已 Rust 化件——
//! 条目表 = mkcpio `load_manifest`（单加载器纪律）、manifest 语义 =
//! mkapp `gate::check_manifest_text`（不出现第二个 manifest 语义判断）、
//! cpio 走查 = mkapp `cpio::cpio_entries`、.sig 见证 = 自件 `envelope_pk`
//! 对读面、签名 = `sign::sign_index`。**`--preset` 面不镜像**（挂
//! berry_assemble Python 现役——`load_product`/`resolve_entries` 产品
//! 终表解析是 berry_assemble 域，Python 侧保留单源；CLI 面 Rust 件遇
//! `--preset` 显式让位指引 python3，拒因文案注明）。
//!
//! 输出文案与 Python 逐行对齐（stdout 全量 + 拒因 `gensourceindex:
//! FAIL: <msg>` 前缀同口径）；拒因里跨件错误透传形 = `条目表加载失败：
//! <mkcpio 拒文>`（Python `die(f"条目表加载失败：{e}")` 的 SystemExit
//! 文本透传镜像——mkcpio 拒文自带 `mkcpio: FAIL:` 前缀）。

use crate::{die, R, Entry, INDEX_NAME, KEYS_NAME, PK_LEN, SIG_LEN, build_index,
            build_keys_listing, check_index_version, parse_index, sha512_256,
            LINEAGE_MAX};

/// .bap 全字节 → manifest.toml 字节（Python `bap_manifest` 镜像）。
/// cpio 走查单源 = mkapp `cpio_entries`（宿主侧唯一解码器，本件 path
/// 依赖复用——不出现第二套 newc 走查）。
pub fn bap_manifest(bap: &[u8]) -> R<Vec<u8>> {
    let entries = mkapp::cpio::cpio_entries(bap)
        .map_err(|e| die(format!("bap cpio 走查拒：{e}")))?;
    for (name, data) in &entries {
        if name == b"manifest.toml" {
            return Ok(data.clone());
        }
    }
    Err(die("bap 缺 manifest.toml 条目（.bap 形制——mkapp 产物面）"))
}

/// manifest.toml → (id, version_str, key_lineage)（Python
/// `manifest_fields` 镜像）。语义单源 = mkapp `check_manifest_text`；
/// [security] 三键必在场（key_lineage 是索引身份半边——无 [security]
/// 的 .bap 不进官方源索引，loud 拒）。
pub fn manifest_fields(mf: &[u8]) -> R<(String, String, String)> {
    let text = std::str::from_utf8(mf)
        .map_err(|_| die("manifest 非 ASCII（berry-manifest 值域外）"))?;
    let m = mkapp::gate::check_manifest_text(text)
        .map_err(|e| die(format!("manifest 解析拒：{e}")))?;
    let Some(sec) = m.sec else {
        return Err(die(format!(
            "app {:?} 无 [security] 三键（ADR #82 V3——key_lineage 是索引身份半边，未签发 app 不进官方源索引；mkapp --sign-key 先签）",
            m.id
        )));
    };
    Ok((m.id.to_string(), m.version.to_string(), sec.key_lineage.to_string()))
}

/// 三值版本串 → (major, minor, patch)（Python `ver_triple` 镜像）。
/// 文法 = mkapp VER_RE 域（0..=999 无前导零）；索引面比 manifest 自由串
/// 收窄（水位比较前置）。
pub fn ver_triple(version_str: &str) -> R<(u32, u32, u32)> {
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() != 3 {
        return Err(ver_reject(version_str));
    }
    let mut out = [0u32; 3];
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() || p.len() > 3 || (p.len() > 1 && p.starts_with('0')) {
            return Err(ver_reject(version_str));
        }
        out[i] = p.parse().map_err(|_| ver_reject(version_str))?;
    }
    Ok((out[0], out[1], out[2]))
}

fn ver_reject(version_str: &str) -> crate::Reject {
    die(format!(
        "app 版本 {version_str:?} 非三值文法 x.y.z（mkapp VER_RE 域——索引面比 manifest 自由串收窄，水位比较前置）"
    ))
}

/// .sig 信封 → pk 见证提取（Python `envelope_pk` 镜像——对读面，非第二
/// 解析器：只按 envelope.rs :8-22 字段表取 lineage_len〔偏移 7〕与 pk
/// 〔偏移 8+L+4 起 32B〕两域 + lineage 回声对账；信封完整验签单源 =
/// berry-sign verify）。
pub fn envelope_pk(sig: &[u8], lineage: &[u8]) -> R<[u8; PK_LEN]> {
    if sig.len() < 8 + 1 + 4 + PK_LEN || sig[0..4] != *b"BSIG" || sig[4] != 1 {
        return Err(die(".sig 非信封 v1（BSIG 头不符——envelope.rs 字段表对读）"));
    }
    let lin_len = sig[7] as usize;
    if !(1..=LINEAGE_MAX).contains(&lin_len) {
        return Err(die(format!(".sig lineage_len {lin_len} 越域（1..=16）")));
    }
    let off = 8 + lin_len + 4;
    if off + PK_LEN > sig.len() {
        return Err(die(".sig 短于 pk 字段（信封截断？）"));
    }
    if &sig[8..8 + lin_len] != lineage {
        return Err(die(format!(
            ".sig lineage 回声不符：{:?} != manifest {:?}（跨 lineage 对——berry-sign verify 同拒面）",
            String::from_utf8_lossy(&sig[8..8 + lin_len]),
            String::from_utf8_lossy(lineage)
        )));
    }
    let mut pk = [0u8; PK_LEN];
    pk.copy_from_slice(&sig[off..off + PK_LEN]);
    Ok(pk)
}

/// rootfs-manifest.toml 文本 + user 基准目录 → app/*.bap 条目（Python
/// `load_app_entries` 镜像；表加载 = mkcpio `load_manifest` 单加载器）。
/// selftest/* 测试夹具不进官方源索引（app/ 前缀 + .bap 后缀双筛）。
/// 返回 [(name, local 绝对/原样路径)]，排序与 Python `sorted` 同口径
/// （字节序——mkcpio 表保序 + BTreeMap 语义）。
pub fn load_app_entries(
    manifest_text: &str,
    manifest_path: &str,
    user_dir: &std::path::Path,
) -> R<Vec<(String, std::path::PathBuf)>> {
    let table = mkcpio::load_manifest(manifest_text)
        .map_err(|e| die(format!("条目表加载失败：{e}")))?;
    let mut apps: Vec<(String, String)> = table
        .into_iter()
        .filter(|(n, _)| n.starts_with("app/") && n.ends_with(".bap"))
        .collect();
    apps.sort();
    if apps.is_empty() {
        return Err(die(format!(
            "{manifest_path} 无 app/*.bap 条目（官方源索引对象缺席——selftest/* 夹具不计）"
        )));
    }
    let mut out = Vec::with_capacity(apps.len());
    for (name, rel) in apps {
        let local = user_dir.join(&rel);
        if !local.is_file() {
            return Err(die(format!(
                "rootfs 条目 {name}: 构建树缺席 {}（表 path 相对 user/——先 just build-user）",
                local.display()
            )));
        }
        out.push((name, local));
    }
    Ok(out)
}

/// 单条目装配（Python `build_entry` 镜像）：.bap 读入 → manifest 单源
/// 解析 → .sig 见证提取（在席）→ Entry。harden = berry-sign verify 加固
/// （在席则跑——顶签名真验；不在席如实注跳过，不硬依）。注行走返回值
/// `notes`（调用方按条目序收进 stdout 行——Python print 时序镜像）。
pub fn build_entry(
    name: &str,
    local: &std::path::Path,
    last_rev: u32,
) -> R<(Entry, Vec<String>)> {
    let mut notes = Vec::new();
    let bap = std::fs::read(local).map_err(|e| die(format!("读 {}：{e}", local.display())))?;
    let (app_id, ver_str, lineage) = manifest_fields(&bap_manifest(&bap)?)?;
    let ver = ver_triple(&ver_str)?;
    let mut pk = None;
    let sig_path = {
        let mut p = local.as_os_str().to_os_string();
        p.push(".sig");
        std::path::PathBuf::from(p)
    };
    if sig_path.is_file() {
        let sig = std::fs::read(&sig_path)
            .map_err(|e| die(format!("读 {}：{e}", sig_path.display())))?;
        let pkv = envelope_pk(&sig, lineage.as_bytes())?;
        // 加固验（Python harden=True 默认面）：berry-sign 在席真验顶签名。
        let td = crate::sign::tempdir_pub("gsi-harden");
        let pkf = td.join("witness.pk");
        std::fs::write(&pkf, pkv)
            .map_err(|e| die(format!("写 {}：{e}", pkf.display())))?;
        match crate::sign::run_berry_sign(&[
            "verify",
            pkf.to_str().unwrap_or(""),
            local.to_str().unwrap_or(""),
        ]) {
            None | Some(Err(_)) => notes.push(format!(
                "gensourceindex: 注: {name} .sig 加固验跳过（berry-sign 不可得——见证 pk 未过顶签名验，构建期加固缺席如实注；完整验签单源 = berry-sign）"
            )),
            Some(Ok(o)) => {
                if !o.status.success() {
                    let t = if o.stderr.is_empty() {
                        String::from_utf8_lossy(&o.stdout).into_owned()
                    } else {
                        String::from_utf8_lossy(&o.stderr).into_owned()
                    };
                    return Err(die(format!(
                        "{name}: .sig 顶签名验拒：{}（索引不见证坏 .sig——先重签）",
                        t.trim()
                    )));
                }
            }
        }
        let _ = std::fs::remove_dir_all(&td);
        pk = Some(pkv);
    }
    Ok((
        Entry {
            id: app_id,
            ver,
            size: bap.len() as u64,
            digest: sha512_256(&bap),
            lineage,
            pk,
            last_rev,
            cert: crate::CERT_ACTIVE.to_string(),
        },
        notes,
    ))
}

/// 生成面参数集（CLI 换轨刀消费——本刀库面定形）。
pub struct GenParams<'a> {
    /// 条目表（rootfs-manifest 路；--preset 面 Python 现役不镜像）。
    pub entries: Vec<(String, std::path::PathBuf)>,
    /// 源钥 pk 文件路径。
    pub source_pk: &'a std::path::Path,
    /// 源钥 sk（在席 = 真签；None = unsigned 诚实降级）。
    pub sign_key: Option<&'a std::path::Path>,
    /// 官方源 id（默认 berryos）。
    pub source_id: &'a str,
    /// 索引级单调版本号（默认 1）。
    pub index_version: u32,
    /// 前代索引（在席 = 单调水位对账）。
    pub prev: Option<&'a std::path::Path>,
    /// 输出目录。
    pub out_dir: &'a std::path::Path,
}

/// 生成主流程（Python `do_generate` rootfs-manifest 路镜像）。
/// 返回 stdout 全量行（调用方打印）。文件落盘两件套 + 单调对账 +
/// 签名单点缝全在内；拒因一律 `Reject`（loud）。
pub fn do_generate(p: &GenParams) -> R<Vec<String>> {
    let mut lines = Vec::new();
    let last_rev = p.last_revision()?;
    let mut entries = Vec::with_capacity(p.entries.len());
    for (name, local) in &p.entries {
        let (e, notes) = build_entry(name, local, last_rev)?;
        // 加固验跳过注按条目装配时序先于两件套汇总行（Python print 镜像）。
        lines.extend(notes);
        entries.push(e);
    }
    let pk = std::fs::read(p.source_pk)
        .map_err(|e| die(format!("读 {}：{e}", p.source_pk.display())))?;
    if pk.len() != PK_LEN {
        return Err(die(format!(
            "--source-pk 非 {PK_LEN}B（berry-sign keygen 产物）"
        )));
    }
    let pk: [u8; PK_LEN] = pk.try_into().unwrap();
    if let Some(prev) = p.prev {
        let praw = std::fs::read(prev)
            .map_err(|e| die(format!("读 {}：{e}", prev.display())))?;
        let seen = parse_index(&praw)?.index_ver;
        check_index_version(p.index_version, seen)?;
    }
    // 索引签名（#77 实装同款）：--sign-key 在席 = 预置形真签换槽；缺席 =
    // unsigned 诚实降级。
    let (index_bytes, signed, note) = if let Some(sk) = p.sign_key {
        let form = build_index(&entries, p.index_version, &pk, Some(&[0u8; SIG_LEN]))?;
        let (sig, note) = crate::sign::sign_index(Some(&form), Some(sk.to_str().unwrap_or("")))?;
        let mut b = form;
        let n = b.len();
        b[n - SIG_LEN..].copy_from_slice(&sig);
        (b, true, note)
    } else {
        let b = build_index(&entries, p.index_version, &pk, None)?;
        let (_, note) = crate::sign::sign_index(None, None)?;
        (b, false, note)
    };
    let keys_bytes = build_keys_listing(p.source_id, &pk, signed)?;
    std::fs::create_dir_all(p.out_dir)
        .map_err(|e| die(format!("建 {}: {e}", p.out_dir.display())))?;
    let ipath = p.out_dir.join(INDEX_NAME);
    let kpath = p.out_dir.join(KEYS_NAME);
    std::fs::write(&ipath, &index_bytes)
        .map_err(|e| die(format!("写 {}: {e}", ipath.display())))?;
    std::fs::write(&kpath, &keys_bytes)
        .map_err(|e| die(format!("写 {}: {e}", kpath.display())))?;
    let n_wit = entries.iter().filter(|e| e.pk.is_some()).count();
    lines.push(format!(
        "gensourceindex: {} {}B entries={} index_version={} witnessed={n_wit} last_revision={}",
        ipath.display(),
        index_bytes.len(),
        entries.len(),
        p.index_version,
        last_rev
    ));
    lines.push(format!(
        "gensourceindex: {} {}B source_id={} sig_state={}",
        kpath.display(),
        keys_bytes.len(),
        p.source_id,
        if signed { "signed" } else { "unsigned" }
    ));
    if signed {
        lines.push(format!("gensourceindex: 注: 签名槽在席（flags.signed=1）：{note}"));
    } else {
        lines.push(format!("gensourceindex: 注: 索引未签（如实注）：{note}"));
    }
    lines.push("gensourceindex: ok（两件套——mkimage --source-instance 挂接）".to_string());
    Ok(lines)
}

impl GenParams<'_> {
    /// last_revision 取值源 = 平台 level（Python `platform_level`——
    /// abi.toml 单源；mkimage `platform_level` 同源同法）。独立小工作区
    /// 不 path 依赖 mkimage——abi.toml 小读器同 gen_abi/mkcpio 先例自携。
    fn last_revision(&self) -> R<u32> {
        platform_level().map(|v| v as u32)
    }
}

/// 平台 API level 单源现值（Python `platform_level` 镜像）：tools/abi.toml
/// `[platform].level`（数据单源在 abi.toml，各工具自携小读器——
/// gen_abi/mkcpio/mkimage 既有先例）。坏源一律 loud（`Reject`）。
pub fn platform_level() -> R<u64> {
    let path = repo_tools_dir().join("abi.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| die(format!("读 {}：{e}", path.display())))?;
    let mut in_platform = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_platform = l == "[platform]";
            continue;
        }
        if in_platform {
            if let Some(rest) = l.strip_prefix("level") {
                if let Some(v) = rest.trim().strip_prefix('=') {
                    let v = v.split('#').next().unwrap_or("").trim();
                    if let Ok(n) = v.parse::<u64>() {
                        return Ok(n);
                    }
                    return Err(die(format!(
                        "tools/abi.toml [platform].level 非整数：{v:?}（last_revision 单源）"
                    )));
                }
            }
        }
    }
    Err(die("tools/abi.toml [platform].level 缺席（last_revision 单源）"))
}

/// tools/ 目录发现（CWD 上溯 + exe 路径上溯双兜底——mkimage
/// find_repo_root 同款；库面无 CARGO_MANIFEST_DIR）。
pub fn repo_tools_dir() -> std::path::PathBuf {
    for base in [std::env::current_dir().ok(), std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))]
    {
        let Some(mut dir) = base else { continue };
        loop {
            if dir.file_name().map(|f| f == "tools").unwrap_or(false) {
                return dir.to_path_buf();
            }
            if dir.join("abi.toml").is_file() && dir.join("gensourceindex.py").is_file() {
                return dir.to_path_buf();
            }
            match dir.parent() {
                Some(p) => dir = p.to_path_buf(),
                None => break,
            }
        }
    }
    std::env::current_dir().unwrap_or_default().join("tools")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_keys_listing;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("gsi-gen-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 合成 .bap（mkapp Rust 门禁真产——Python oracle 对拍面在
    /// tests/gen_parity.rs，此处只造最小可解析件）。
    fn synth_bap(dir: &std::path::Path, id: &str) -> std::path::PathBuf {
        let stage = dir.join(format!("stage-{id}"));
        std::fs::create_dir_all(stage.join("bin")).unwrap();
        std::fs::write(
            stage.join("manifest.toml"),
            format!(
                "[app]\nid = \"{id}\"\nversion = \"0.1.0\"\nentry = \"bin/x\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[security]\nprofile = \"standard\"\nkey_lineage = \"berryos\"\nintegrator = \"oem-1\"\n"
            ),
        )
        .unwrap();
        std::fs::write(stage.join("bin").join("x"), b"\x7fELFx".to_vec()).unwrap();
        // mkcpio Rust 件直产（单编码器纪律——build_archive 排序单源）。
        let items = vec![
            (b"bin/x".to_vec(), b"\x7fELFx".to_vec()),
            (b"manifest.toml".to_vec(), std::fs::read(stage.join("manifest.toml")).unwrap()),
        ];
        let mut names: Vec<String> = items.iter().map(|(n, _)| String::from_utf8_lossy(n).into_owned()).collect();
        names.sort();
        let sorted: Vec<(Vec<u8>, Vec<u8>)> = names
            .iter()
            .map(|n| items.iter().find(|(en, _)| String::from_utf8_lossy(en) == *n).unwrap().clone())
            .collect();
        let blob = mkcpio::build_archive(&sorted);
        let p = dir.join(format!("{id}.bap"));
        std::fs::write(&p, blob).unwrap();
        p
    }

    #[test]
    fn bap_manifest_roundtrip_and_rejects() {
        let t = tmp("bap");
        let bap = synth_bap(&t, "GENA");
        let mf = bap_manifest(&std::fs::read(&bap).unwrap()).unwrap();
        assert!(mf.starts_with(b"[app]"));
        // 缺 manifest.toml 条目拒。
        let bare = mkcpio::build_archive(&[(b"bin/x".to_vec(), b"ELF".to_vec())]);
        let r = bap_manifest(&bare).unwrap_err();
        assert!(r.message.contains("缺 manifest.toml"), "{}", r.message);
        // 坏 cpio 拒（走查单源透传）。
        let r = bap_manifest(b"garbage").unwrap_err();
        assert!(r.message.contains("cpio 走查拒"), "{}", r.message);
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn manifest_fields_security_gate() {
        let t = tmp("mf");
        let bap = synth_bap(&t, "GENB");
        let mf = bap_manifest(&std::fs::read(&bap).unwrap()).unwrap();
        let (id, ver, lineage) = manifest_fields(&mf).unwrap();
        assert_eq!((id.as_str(), ver.as_str(), lineage.as_str()), ("GENB", "0.1.0", "berryos"));
        // 无 [security] 拒。
        let nosec = "[app]\nid = \"NOS\"\nversion = \"0.1.0\"\nentry = \"bin/x\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n";
        let r = manifest_fields(nosec.as_bytes()).unwrap_err();
        assert!(r.message.contains("无 [security] 三键"), "{}", r.message);
        // 非 ASCII 拒。
        let r = manifest_fields(&[0xFF, 0xFE]).unwrap_err();
        assert!(r.message.contains("非 ASCII"), "{}", r.message);
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn ver_triple_domain() {
        assert_eq!(ver_triple("0.1.0").unwrap(), (0, 1, 0));
        assert_eq!(ver_triple("999.999.999").unwrap(), (999, 999, 999));
        for bad in ["1.0", "01.0.0", "1.0.0000", "1000.0.0", "a.b.c", "1..0"] {
            let r = ver_triple(bad).unwrap_err();
            assert!(r.message.contains("非三值文法"), "{bad}: {}", r.message);
        }
    }

    #[test]
    fn envelope_pk_mirror() {
        // 最小合法信封：BSIG v1 + reserved + lin_len + lineage + u32 + pk。
        let mut sig = Vec::new();
        sig.extend_from_slice(b"BSIG");
        sig.push(1);
        sig.push(0);
        sig.push(0);
        sig.push(7); // lin_len
        sig.extend_from_slice(b"berryos");
        sig.extend_from_slice(&[0u8; 4]);
        let pk: Vec<u8> = (0xA0..0xC0).collect();
        sig.extend_from_slice(&pk);
        let got = envelope_pk(&sig, b"berryos").unwrap();
        assert_eq!(got.as_slice(), &pk[..]);
        // 回声不符拒。
        let r = envelope_pk(&sig, b"oem-x").unwrap_err();
        assert!(r.message.contains("回声不符"), "{}", r.message);
        // 头不符拒。
        let r = envelope_pk(b"XXXX\x01\x00\x00\x01berryos\x00\x00\x00\x00" , &[0u8; 32]).unwrap_err();
        assert!(r.message.contains("非信封 v1"), "{}", r.message);
        // 截断拒（<45B 落首检——len 与头合并条件，Python 同口径）。
        let r = envelope_pk(&sig[..20], b"berryos").unwrap_err();
        assert!(
            r.message.contains("越域") || r.message.contains("截断") || r.message.contains("非信封 v1"),
            "{}",
            r.message
        );
        // 中段截断（>45B、过首检）落「短于 pk 字段」分支。
        let mut mid = sig.clone();
        mid.truncate(48);
        let r = envelope_pk(&mid, b"berryos").unwrap_err();
        assert!(r.message.contains("短于 pk 字段"), "{}", r.message);
    }

    #[test]
    fn load_app_entries_filter_and_rejects() {
        let t = tmp("lae");
        std::fs::write(t.join("b1.bap"), b"x").unwrap();
        let good = "version = 1\n\n[entries]\n\"selftest/x.bap\" = \"b1.bap\"\n";
        let r = load_app_entries(good, "m.toml", &t).unwrap_err();
        assert!(r.message.contains("无 app/*.bap 条目"), "{}", r.message);
        std::fs::write(t.join("b2.bap"), b"x").unwrap();
        let good2 = "version = 1\n\n[entries]\n\"app/x.bap\" = \"b2.bap\"\n\"bin/sh\" = \"b1.bap\"\n";
        let got = load_app_entries(good2, "m.toml", &t).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "app/x.bap");
        // 构建树缺席拒。
        let missing = "version = 1\n\n[entries]\n\"app/y.bap\" = \"nope.bap\"\n";
        let r = load_app_entries(missing, "m.toml", &t).unwrap_err();
        assert!(r.message.contains("构建树缺席"), "{}", r.message);
        // 表加载拒透传（mkcpio 前缀保留）。
        let r = load_app_entries("version = 2\n", "m.toml", &t).unwrap_err();
        assert!(r.message.contains("条目表加载失败"), "{}", r.message);
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn do_generate_unsigned_flow() {
        let t = tmp("gen");
        let bap = synth_bap(&t, "GENC");
        let entries = vec![("app/genc.bap".to_string(), bap.clone())];
        let pk = t.join("pk.bin");
        std::fs::write(&pk, [0xA5u8; 32]).unwrap();
        let out = t.join("src");
        let p = GenParams {
            entries,
            source_pk: &pk,
            sign_key: None,
            source_id: "berryos",
            index_version: 1,
            prev: None,
            out_dir: &out,
        };
        let lines = do_generate(&p).unwrap();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("entries=1") && lines[0].contains("witnessed=0"));
        assert!(lines[1].contains("sig_state=unsigned"));
        assert!(lines[2].contains("索引未签"));
        assert!(lines[3].contains("ok"));
        // 产物可回读（parse_index 往返）。
        let raw = std::fs::read(out.join(INDEX_NAME)).unwrap();
        let idx = parse_index(&raw).unwrap();
        assert_eq!(idx.entries.len(), 1);
        assert_eq!(idx.entries[0].id, "GENC");
        assert_eq!(idx.entries[0].last_rev, platform_level().unwrap() as u32);
        assert!(!idx.signed);
        // 清单往返。
        let (sid, pk2, signed) =
            parse_keys_listing(&std::fs::read(out.join(KEYS_NAME)).unwrap()).unwrap();
        assert_eq!((sid.as_str(), signed), ("berryos", false));
        assert_eq!(pk2, [0xA5u8; 32]);
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn do_generate_pk_len_gate_and_prev_gate() {
        let t = tmp("gate");
        let bap = synth_bap(&t, "GEND");
        let pk = t.join("pk.bin");
        std::fs::write(&pk, [0xA5u8; 31]).unwrap();
        let p = GenParams {
            entries: vec![("app/gend.bap".to_string(), bap.clone())],
            source_pk: &pk,
            sign_key: None,
            source_id: "berryos",
            index_version: 1,
            prev: None,
            out_dir: &t.join("o1"),
        };
        let r = do_generate(&p).unwrap_err();
        assert!(r.message.contains("--source-pk 非 32B"), "{}", r.message);
        // prev 单调门：index_version=1 < 已见 5 → 拒。
        std::fs::write(&pk, [0xA5u8; 32]).unwrap();
        let prev = t.join("prev.tlv");
        std::fs::write(&prev, {
            let e = Entry {
                id: "OLD".to_string(),
                ver: (0, 0, 1),
                size: 1,
                digest: [0u8; 32],
                lineage: "berryos".to_string(),
                pk: None,
                last_rev: 1,
                cert: "active".to_string(),
            };
            build_index(&[e], 5, &[0u8; 32], None).unwrap()
        })
        .unwrap();
        let p2 = GenParams {
            entries: vec![("app/gend.bap".to_string(), bap.clone())],
            source_pk: &pk,
            sign_key: None,
            source_id: "berryos",
            index_version: 1,
            prev: Some(&prev),
            out_dir: &t.join("o2"),
        };
        let r = do_generate(&p2).unwrap_err();
        assert!(r.message.contains("回降拒"), "{}", r.message);
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn platform_level_live() {
        // 现役 abi.toml 真值面（测试运行于 tools/gensourceindex/——
        // GSI_TOOLS_DIR 由集成测试腿注入；单元腿直接上溯 CWD）。
        let v = platform_level().unwrap();
        assert!(v > 0, "abi.toml level 须为正现值");
    }
}
