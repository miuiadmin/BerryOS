//! `berry crash-log`——CBOX.LOG FAT12 直读 + 指纹聚合报告（cbox 刀 3）。
//!
//! 读面（fatcat.py 毕业超集）：卷面发现（LBA0 印章区非 MBR——验 BPB 健全性
//! 后落 LBA16）→ 根目录 8.3 扫描 → FAT12 簇链。**链断 = 账本撕裂**（目录项
//! 在而 FAT 首簇项已释放——B4 掉电竞技场实测形态，刀 2 笔记边界节）→ 如实
//! 报错非零退出，不静默截断。行解析单源 `berry-cbox`（「不长第二套」）。
//!
//! 聚合：`(kind, task, pc, lr)` 指纹 → 次数 / 首末 boot id。符号化：
//! userbins ELF 优先（现实全 strip → 双空报错指路）；`user_<名>` 族内嵌
//! blob 任务走内核 ELF `__user_<名>` 锚点重定基（任务空间基址 0x1000）；
//! 内核名直查。责任定界 = 任务名单值判 ELF（加载基址全系统固定）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use berry_cbox::{Rec, KIND_EL0, KIND_EL1};

use crate::error::Result;
use crate::symbolize::{self, Sym};

/// FAT12 读面错误（盘/账本不可读族——CLI 面非零退出）。
#[derive(Debug)]
pub enum FatErr {
    NoVolume,
    NoEnt(String),
    /// 链断/短链：目录项在而簇链不足 size（掉电撕裂形态）。
    Torn { have: usize, want: usize },
    Loop,
}

impl FatErr {
    pub fn message(&self) -> String {
        match self {
            FatErr::NoVolume => "找不到合法 FAT 卷 BPB（LBA0/LBA16 皆否）".into(),
            FatErr::NoEnt(n) => format!("根目录无 {n}"),
            FatErr::Torn { have, want } => format!(
                "账本簇链撕裂（链内 {have}B < 目录项 {want}B——掉电/中断形态）"
            ),
            FatErr::Loop => "簇链环".into(),
        }
    }
}

fn u16le(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// BPB 健全性（fatcat.py 同款校验面 + spc 域）：510 引导签名 / bps 合法 /
/// nfats∈{1,2} / rents>0 / spf>0 / **spc 非零 2 的幂**（spc=0 → clus_bytes=0
/// → 除零；构造盘实测复现 panic——FatErr 契约禁 panic）。
fn bpb_ok(d: &[u8], off: usize) -> bool {
    let spc = *d.get(off + 13).unwrap_or(&0);
    d.get(off + 510..off + 512) == Some(b"\x55\xaa")
        && matches!(u16le(d, off + 11), 512 | 1024 | 2048 | 4096)
        && matches!(d[off + 16], 1 | 2)
        && u16le(d, off + 17) > 0
        && u16le(d, off + 22) > 0
        && spc.is_power_of_two()
}

/// 根文件直读（只借用整盘字节；`name` 8.3 大写）。
pub fn read_root_file(d: &[u8], name: &str) -> std::result::Result<Vec<u8>, FatErr> {
    let vol = if bpb_ok(d, 0) { 0 } else { 16 * 512 };
    if !bpb_ok(d, vol) {
        return Err(FatErr::NoVolume);
    }
    let bps = u16le(d, vol + 11) as usize;
    let spc = d[vol + 13] as usize;
    let rsvd = u16le(d, vol + 14) as usize;
    let nfats = d[vol + 16] as usize;
    let rents = u16le(d, vol + 17) as usize;
    let spf = u16le(d, vol + 22) as usize;
    let fat_off = vol + rsvd * bps;
    let root_off = vol + (rsvd + nfats * spf) * bps;
    let data_off = root_off + rents * 32;
    let clus_bytes = spc * bps;
    // bpb_ok 已验 spc 非零——防御面双保险（几何乘零 = 后续除零）。
    if clus_bytes == 0 {
        return Err(FatErr::NoVolume);
    }

    let mut ent: Option<(usize, usize)> = None; // (first_clus, size)
    for i in 0..rents {
        let o = root_off + i * 32;
        let e = match d.get(o..o + 32) {
            Some(e) => e,
            None => break,
        };
        if e[0] == 0x00 {
            break;
        }
        if e[0] == 0xE5 || e[11] & 0x08 != 0 {
            continue; // 已删/卷标
        }
        let base = e[0..8].iter().take_while(|&&b| b != b' ').count();
        let ext = e[8..11].iter().take_while(|&&b| b != b' ').count();
        let mut full = String::from_utf8_lossy(&e[..base]).to_uppercase();
        if ext > 0 {
            full.push('.');
            full.push_str(&String::from_utf8_lossy(&e[8..8 + ext]).to_uppercase());
        }
        if full == name.to_uppercase() {
            ent = Some((u16le(e, 26) as usize, u32le(e, 28) as usize));
            break;
        }
    }
    let (first, size) = ent.ok_or_else(|| FatErr::NoEnt(name.into()))?;

    let fat12 = |n: usize| -> Option<u32> {
        let o = fat_off + n + n / 2;
        // FAT 表项越界（坏簇号把偏移推出盘尾）= None → Torn——u16le 直接
        // 下标在构造盘上实测 panic（FatErr 契约禁 panic）。
        let b = d.get(o..o + 2)?;
        let v = u16::from_le_bytes([b[0], b[1]]);
        Some(if n & 1 == 1 { v >> 4 } else { v & 0x0FFF } as u32)
    };
    let mut out = Vec::with_capacity(size.min(d.len()));
    let mut clus = first;
    let mut hops = 0usize;
    while (2..0xFF8).contains(&clus) {
        hops += 1;
        if hops > d.len() / clus_bytes + 2 {
            return Err(FatErr::Loop);
        }
        let start = data_off + (clus - 2) * clus_bytes;
        let end = start + clus_bytes;
        let (start, end) = (start.min(d.len()), end.min(d.len()));
        out.extend_from_slice(d.get(start..end).unwrap_or(&[]));
        if out.len() >= size {
            break;
        }
        clus = fat12(clus).ok_or(FatErr::Torn { have: out.len(), want: size })?
            as usize;
        if clus == 0 {
            // 目录项在、链已释放——掉电竞技场实测形态（刀 2 笔记边界）。
            return Err(FatErr::Torn { have: out.len(), want: size });
        }
    }
    if out.len() < size {
        return Err(FatErr::Torn { have: out.len(), want: size });
    }
    out.truncate(size);
    Ok(out)
}

/// 账本解析：一行一记录（`berry-cbox` 单源；空行跳过——账本可能以换行
/// 收尾）。返回 `(记录, 拒行数)`——非空行解析失败计入拒行（坏行/残尾如实
/// 上报，不静默丢：全毁形态 `非空 && 零记录` 由 CLI 判非零退出）。
pub fn parse_ledger(buf: &[u8]) -> (Vec<(u32, Rec)>, usize) {
    let mut recs = Vec::new();
    let mut rejected = 0usize;
    for l in buf.split(|&b| b == b'\n') {
        if l.is_empty() {
            continue;
        }
        match berry_cbox::line_parse(l) {
            Some(r) => recs.push(r),
            None => rejected += 1,
        }
    }
    (recs, rejected)
}

/// 指纹聚合键（`(kind, task, pc, lr)`——spec 冻结）。
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Fp {
    pub kind: u8,
    pub task: String,
    pub pc: u64,
    pub lr: u64,
}

/// 聚合值：次数 + 首末 boot id。
#[derive(Debug)]
pub struct FpStat {
    pub count: usize,
    pub boot_first: u32,
    pub boot_last: u32,
}

pub fn aggregate(recs: &[(u32, Rec)]) -> BTreeMap<Fp, FpStat> {
    let mut m: BTreeMap<Fp, FpStat> = BTreeMap::new();
    for (boot, r) in recs {
        let fp = Fp {
            kind: r.kind,
            task: task_name(r),
            pc: r.pc,
            lr: r.lr,
        };
        let st = m.entry(fp).or_insert(FpStat {
            count: 0,
            boot_first: *boot,
            boot_last: *boot,
        });
        st.count += 1;
        st.boot_first = st.boot_first.min(*boot);
        st.boot_last = st.boot_last.max(*boot);
    }
    m
}

/// 任务名（`[u8; 16]` NUL 填充 → 截断字符串）。
pub fn task_name(r: &Rec) -> String {
    let end = r.name.iter().position(|&b| b == 0).unwrap_or(r.name.len());
    String::from_utf8_lossy(&r.name[..end]).into_owned()
}

fn kind_str(kind: u8) -> &'static str {
    match kind {
        KIND_EL0 => "el0",
        KIND_EL1 => "el1",
        _ => "?",
    }
}

/// 符号解析结果（报告行用）。
#[derive(Debug)]
pub enum SymHit {
    None(String),                         // 无符号源（如实注，不造假）
    Hit { name: String, off: u64, src: &'static str },
}

/// 内核内嵌 blob 任务的加载基址（`kernel/src/user.rs` 模板统一 @0x1000）。
const USER_BLOB_BASE: u64 = 0x1000;

/// 符号源解析结果（四态如实话——无 --elf-dir / 有目录无 ELF / 直查 /
/// 重定基，外加 userbins 首选与无锚点两分支；Err 保留给加载真失败
/// ——strip 双空报错带 hint 如实冒泡）。
pub enum SymSrc {
    /// 未给 `--elf-dir`（跳过符号化）。
    NoDir,
    /// `--elf-dir` 给定，userbins 与内核 ELF 候选全缺（构建产物不在位）。
    NoElf,
    /// userbins release 件直查。
    UserBins(Vec<Sym>),
    /// 内核 ELF 直查（内核名任务）。
    Kernel(Vec<Sym>),
    /// `user_<名>` 内嵌 blob：`__user_<名>` 锚点重定基。
    Blob { syms: Vec<Sym>, base: u64 },
    /// `user_<名>` 在内核 ELF 无 `__user_<名>` 锚点（拼写/构建漂移）。
    NoAnchor,
}

/// 任务 → 符号源（`user_<名>` 族先试 userbins release 件，回落内核 ELF
/// `__user_<名>` 锚点重定基；内核名直查）。
fn resolve_syms(task: &str, elf_dir: Option<&Path>) -> Result<SymSrc> {
    let Some(dir) = elf_dir else { return Ok(SymSrc::NoDir) };
    // ① userbins：`<dir>/user/cargo-target/.../release/<task>`（现实全 strip
    //    ——load_symbols 双空会带指路 hint 报错，如实冒泡）。
    let user_elf = dir
        .join("user/cargo-target/aarch64-unknown-none/release")
        .join(task);
    if user_elf.is_file() {
        let syms = symbolize::load_symbols(&std::fs::read(&user_elf)?)?;
        return Ok(SymSrc::UserBins(syms));
    }
    // ② 内核 ELF：worktree 本地 debug → 全局 target dir（just build 实际落
    //    位——smoke.sh berryos_kernel() 同解析序）→ release 变体。
    let mut kernel = None;
    for cand in [
        dir.join("cargo-target/aarch64-unknown-none/debug/berry-kernel"),
        home_target().join("aarch64-unknown-none/debug/berry-kernel"),
        dir.join("cargo-target/aarch64-unknown-none/release/berry-kernel"),
    ] {
        if cand.is_file() {
            kernel = Some(cand);
            break;
        }
    }
    let Some(kpath) = kernel else { return Ok(SymSrc::NoElf) };
    let syms = symbolize::load_symbols(&std::fs::read(&kpath)?)?;
    if !task.starts_with("user_") {
        return Ok(SymSrc::Kernel(syms));
    }
    // ③ `user_<名>` 内嵌 blob：`__user_<名>` 锚点（无锚点 = 独立态——
    //    blob 任务 pc 是任务空间地址，直查内核表必错位，不静默降级）。
    let anchor = format!("__{task}");
    let base = syms.iter().find(|s| s.name == anchor).map(|a| a.value);
    match base {
        Some(base) => Ok(SymSrc::Blob { syms, base }),
        None => Ok(SymSrc::NoAnchor),
    }
}

fn home_target() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".cargo-target")
}

/// 指纹 pc 符号化（blob 重定基：pc - 0x1000 + 锚点 vma）。
pub fn symbolize_fp(fp: &Fp, elf_dir: Option<&Path>) -> SymHit {
    match resolve_syms(&fp.task, elf_dir) {
        Err(e) => SymHit::None(e.message),
        Ok(SymSrc::NoDir) => SymHit::None("无 --elf-dir（跳过符号化）".into()),
        Ok(SymSrc::NoElf) => SymHit::None(
            "--elf-dir 内无 ELF（userbins release 件与内核 cargo-target \
             debug/~/.cargo-target/release 候选皆缺）"
                .into(),
        ),
        Ok(SymSrc::UserBins(syms)) => match symbolize::nearest(&syms, fp.pc) {
            Some((s, off)) => SymHit::Hit {
                name: s.name.clone(),
                off,
                src: "userbins ELF",
            },
            None => SymHit::None("pc 早于首符号".into()),
        },
        Ok(SymSrc::Kernel(syms)) => match symbolize::nearest(&syms, fp.pc) {
            Some((s, off)) => SymHit::Hit {
                name: s.name.clone(),
                off,
                src: "内核 ELF",
            },
            None => SymHit::None("pc 早于首符号".into()),
        },
        Ok(SymSrc::Blob { syms, base }) => {
            let Some(pc_k) = fp.pc.checked_sub(USER_BLOB_BASE) else {
                return SymHit::None("pc < blob 基址 0x1000".into());
            };
            let Some(pc_k) = pc_k.checked_add(base) else {
                return SymHit::None("重定基溢出".into());
            };
            match symbolize::nearest(&syms, pc_k) {
                Some((s, off)) => SymHit::Hit {
                    name: s.name.clone(),
                    off,
                    src: "内核 blob 重定基",
                },
                None => SymHit::None("重定基 pc 早于首符号".into()),
            }
        }
        Ok(SymSrc::NoAnchor) => {
            SymHit::None("user_ 任务在内核 ELF 无 __user_ 锚点".into())
        }
    }
}

/// 人类可读报告（一行一指纹，按 次数 降序、并列按指纹序——确定性输出）。
/// `rejected > 0` 头行注记（坏行如实上报不静默丢）。
pub fn report_human(
    recs: &[(u32, Rec)],
    rejected: usize,
    disk: &Path,
    elf_dir: Option<&Path>,
) -> String {
    let fps = aggregate(recs);
    let mut lines = vec![format!(
        "cbox crash-log: {} CBOX.LOG {} 条记录 / {} 个指纹{}",
        disk.display(),
        recs.len(),
        fps.len(),
        if rejected > 0 {
            format!("（拒 {rejected} 坏行）")
        } else {
            String::new()
        }
    )];
    let mut order: Vec<_> = fps.iter().collect();
    order.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(b.0)));
    for (i, (fp, st)) in order.iter().enumerate() {
        let sym = match symbolize_fp(fp, elf_dir) {
            SymHit::Hit { name, off, src } => format!(" → {name}+0x{off:x}（{src}）"),
            SymHit::None(why) => format!("（无符号：{why}）"),
        };
        lines.push(format!(
            "#{} ×{} {} {} pc=0x{:x} lr=0x{:x} boot {}..{}{}",
            i + 1,
            st.count,
            kind_str(fp.kind),
            fp.task,
            fp.pc,
            fp.lr,
            st.boot_first,
            st.boot_last,
            sym
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(seq: u32, kind: u8, name: &str, pc: u64, lr: u64) -> Rec {
        let mut r = Rec::EMPTY;
        r.seq = seq;
        r.kind = kind;
        r.name[..name.len()].copy_from_slice(name.as_bytes());
        r.pc = pc;
        r.far = 0x4000_0000;
        r.esr = 0x9200_0005;
        r.lr = lr;
        r.vct = 42;
        r
    }

    /// 对真产物造盘：`tools/mkfat.py --put` 注入 CBOX.LOG（确定性、零
    /// fixture 入库；mkfat 缺席/失败即测试失败——不许静默跳过）。
    /// 文件名 = pid + 原子序号（测试并行线程同 pid，裸 pid 会互踩）。
    fn mkfat_disk(ledger: &[u8]) -> Vec<u8> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let uniq = format!(
            "{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let tmp = std::env::temp_dir();
        let lfile = tmp.join(format!("berry-cbox-test-ledger-{uniq}.log"));
        let img = tmp.join(format!("berry-cbox-test-disk-{uniq}.img"));
        std::fs::write(&lfile, ledger).unwrap();
        let mk = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/mkfat.py");
        let out = std::process::Command::new("python3")
            .arg(mk)
            .arg(&img)
            .args(["--put", &format!("{}:/CBOX.LOG", lfile.display())])
            .output()
            .expect("python3 起 mkfat 失败");
        std::fs::remove_file(&lfile).ok();
        assert!(
            out.status.success(),
            "mkfat 造盘失败：{}",
            String::from_utf8_lossy(&out.stderr)
        );
        // mkfat --put 写裸卷（EFI 引导盘形态，vol@LBA0）；主路盘 = 16 扇区
        // 零前缀 + vol@LBA16——测试对齐主路形态（走 LBA16 发现路径）。
        let mut d = vec![0u8; 16 * 512];
        d.extend_from_slice(&std::fs::read(&img).unwrap());
        std::fs::remove_file(&img).ok();
        d
    }

    fn two_line_ledger() -> Vec<u8> {
        let mut buf = Vec::new();
        // LINE_MAX 单源（berry-cbox 契约帽；256 硬编码在真行 245B 逼近下
        // 是隐形截断面）。
        let mut line = [0u8; berry_cbox::LINE_MAX];
        for (boot, r) in [
            (1u32, rec(1, KIND_EL0, "user_rogue_ptr", 0x1004, 0x4000990c)),
            (2u32, rec(1, KIND_EL0, "user_rogue_ptr", 0x1004, 0x4000990c)),
        ] {
            let n = berry_cbox::line_encode(&r, boot, &mut line).unwrap();
            buf.extend_from_slice(&line[..n]);
            buf.push(b'\n');
        }
        buf
    }

    #[test]
    fn fat12_read_against_real_mkfat() {
        let d = mkfat_disk(&two_line_ledger());
        let got = read_root_file(&d, "CBOX.LOG").unwrap();
        assert_eq!(got, two_line_ledger());
        // 8.3 大小写不敏感 + 卷标项跳过（盘根首项是卷标）。
        assert_eq!(read_root_file(&d, "cbox.log").unwrap().len(), got.len());
        assert!(matches!(
            read_root_file(&d, "NOPE.TXT"),
            Err(FatErr::NoEnt(_))
        ));
        // 裸卷形态（mkfat --put 原始产物，vol@LBA0）：LBA0 直发现路径。
        let bare = &d[16 * 512..];
        assert_eq!(read_root_file(bare, "CBOX.LOG").unwrap(), got);
    }

    #[test]
    fn parse_and_aggregate_roundtrip() {
        let d = mkfat_disk(&two_line_ledger());
        let (recs, rejected) = parse_ledger(&read_root_file(&d, "CBOX.LOG").unwrap());
        assert_eq!(recs.len(), 2);
        assert_eq!(rejected, 0);
        let fps = aggregate(&recs);
        assert_eq!(fps.len(), 1);
        let (fp, st) = fps.iter().next().unwrap();
        assert_eq!(fp.task, "user_rogue_ptr");
        assert_eq!(fp.kind, KIND_EL0);
        assert_eq!((st.count, st.boot_first, st.boot_last), (2, 1, 2));
        let r = &recs[0].1;
        assert_eq!(r.pc, 0x1004);
        assert_eq!(r.far, 0x4000_0000);
        // 合成负例：坏行被拒不误读（解析严格单源；拒行计数如实）。
        assert!(berry_cbox::line_parse(b"CBOX v1 seq=x kind=el0").is_none());
        let (rs, rj) = parse_ledger(b"CBOX v1 seq=x kind=el0\nCBOX v1 seq=y\n\n");
        assert!(rs.is_empty());
        assert_eq!(rj, 2);
    }

    #[test]
    fn torn_chain_detected() {
        // 真盘上把 CBOX.LOG 的 FAT 首簇项清零 = 掉电竞技场实测形态
        //（目录项在、链释放）。账本必须跨多簇（2048B/簇）——单簇件
        // 读满首簇即止不走链，撕链不可见（B4 的 735B 件即此形态）。
        let ledger = big_ledger();
        assert!(ledger.len() > 2048, "测试账本必须跨簇");
        let mut d = mkfat_disk(&ledger);
        let vol = 16 * 512;
        let bps = u16le(&d, vol + 11) as usize;
        let rsvd = u16le(&d, vol + 14) as usize;
        let nfats = d[vol + 16] as usize;
        let spf = u16le(&d, vol + 22) as usize;
        let root_off = vol + (rsvd + nfats * spf) * bps;
        let first = u16le(&d, root_off + 1 * 32 + 26) as usize; // [0]=卷标
        let fat_off = vol + rsvd * bps;
        let fo = fat_off + first + first / 2;
        let v = u16le(&d, fo);
        let wiped = if first & 1 == 1 { v & 0x000F } else { v & 0xF000 };
        d[fo..fo + 2].copy_from_slice(&wiped.to_le_bytes());
        match read_root_file(&d, "CBOX.LOG") {
            Err(FatErr::Torn { have, want }) => {
                assert_eq!(want, ledger.len());
                assert_eq!(have, 2048); // 首簇读满、链断在第二簇前
            }
            other => panic!("期望 Torn，得 {other:?}"),
        }
    }

    /// 跨簇账本（30 行 ≈ 7.5KB = 4 簇）——torn/cycle 测试用。
    fn big_ledger() -> Vec<u8> {
        let mut buf = Vec::new();
        let mut line = [0u8; berry_cbox::LINE_MAX];
        for seq in 0..30u32 {
            let r = rec(seq, KIND_EL0, "user_rogue_ptr", 0x1004, 0x4000990c);
            let n = berry_cbox::line_encode(&r, 1, &mut line).unwrap();
            buf.extend_from_slice(&line[..n]);
            buf.push(b'\n');
        }
        buf
    }

    /// 几何参数（bps/rsvd/nfats/spf/根目录偏移/FAT 偏移）——torn/cycle
    /// 测试的盘面定位用。
    fn geom(d: &[u8]) -> (usize, usize, usize) {
        let vol = 16 * 512;
        let bps = u16le(d, vol + 11) as usize;
        let rsvd = u16le(d, vol + 14) as usize;
        let nfats = d[vol + 16] as usize;
        let spf = u16le(d, vol + 22) as usize;
        let root_off = vol + (rsvd + nfats * spf) * bps;
        (bps, root_off, vol + rsvd * bps)
    }

    #[test]
    fn fat_cycle_detected() {
        // 簇链环的**可检形态**：目录项 size 野值（大于全盘容量）+ FAT 环
        //（3→2）——out 永远凑不满 size，hops 帽触发 Loop。环 + 诚实 size
        // 反而不可检（簇数据反复追加直至 size 满足——数据错但读面无感，
        // 与单簇撕裂不可见同族的 FAT 元数据盲区）。
        let mut d = mkfat_disk(&big_ledger());
        let (_, root_off, fat_off) = geom(&d);
        let first = u16le(&d, root_off + 1 * 32 + 26) as usize; // [0]=卷标
        assert_eq!(first, 2, "mkfat 首数据簇约定");
        let fo3 = fat_off + 3 + 3 / 2; // FAT12 奇簇项 = 高 12 位
        d[fo3] = (d[fo3] & 0x0F) | 0x20; // 项 3 ← 2（环）
        d[fo3 + 1] = 0;
        let wild = 0x1000_000u32; // 16MB > 任意 mkfat 测试盘
        d[root_off + 1 * 32 + 28..root_off + 1 * 32 + 32]
            .copy_from_slice(&wild.to_le_bytes());
        assert!(matches!(read_root_file(&d, "CBOX.LOG"), Err(FatErr::Loop)));
    }

    #[test]
    fn corrupted_disk_never_panics() {
        // 构造盘负例族（审计复现形态）：spc=0 → NoVolume（非除零 panic）；
        // 目录项首簇野值 → Torn/Loop（非 FAT 下标越界 panic）。
        let mut d = mkfat_disk(&big_ledger());
        let vol = 16 * 512;
        d[vol + 13] = 0; // spc=0
        assert!(matches!(read_root_file(&d, "CBOX.LOG"), Err(FatErr::NoVolume)));

        let mut d = mkfat_disk(&big_ledger());
        let (_, root_off, _) = geom(&d);
        let wild = 0x0FE0u16; // 合法数据簇区间内的野簇号
        d[root_off + 1 * 32 + 26..root_off + 1 * 32 + 28]
            .copy_from_slice(&wild.to_le_bytes());
        assert!(matches!(
            read_root_file(&d, "CBOX.LOG"),
            Err(FatErr::Torn { .. }) | Err(FatErr::Loop)
        ));
    }

    #[test]
    fn rebase_math_and_source_states() {
        // 合成内核 ELF 进 tempdir（候选①worktree debug 位）钉四态：
        // blob 重定基命中 / 内核名直查 / 无锚点 / 无 --elf-dir。
        use crate::symbolize::testutil::build_elf;
        let dir = std::env::temp_dir().join(format!(
            "berry-cbox-test-resolve-{}",
            std::process::id()
        ));
        let kdir = dir.join("cargo-target/aarch64-unknown-none/debug");
        std::fs::create_dir_all(&kdir).unwrap();
        std::fs::write(
            kdir.join("berry-kernel"),
            build_elf(&[
                ("__user_rogue", 0x4000_0000),
                ("kern_main", 0x4000_2000),
            ]),
        )
        .unwrap();
        // blob 重定基：pc 0x1004 − 0x1000 + 0x4000_0000 → __user_rogue+0x4。
        let fp = Fp {
            kind: KIND_EL0,
            task: "user_rogue".into(),
            pc: 0x1004,
            lr: 0,
        };
        match symbolize_fp(&fp, Some(&dir)) {
            SymHit::Hit { name, off, src } => {
                assert_eq!((name.as_str(), off, src), ("__user_rogue", 4, "内核 blob 重定基"));
            }
            other => panic!("期望 Hit，得 {other:?}"),
        }
        // 内核名直查。
        let fpk = Fp { task: "sched".into(), pc: 0x4000_2100, ..fp };
        match symbolize_fp(&fpk, Some(&dir)) {
            SymHit::Hit { name, src, .. } => {
                assert_eq!((name.as_str(), src), ("kern_main", "内核 ELF"));
            }
            other => panic!("期望 Hit，得 {other:?}"),
        }
        // 无锚点：user_ghost 无 __user_ghost。
        let fpg = Fp { task: "user_ghost".into(), pc: 0x1004, ..fp };
        match symbolize_fp(&fpg, Some(&dir)) {
            SymHit::None(why) => assert!(why.contains("锚点"), "{why}"),
            other => panic!("期望 None，得 {other:?}"),
        }
        // 无 --elf-dir。
        match symbolize_fp(&fp, None) {
            SymHit::None(why) => assert!(why.contains("--elf-dir"), "{why}"),
            other => panic!("期望 None，得 {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
