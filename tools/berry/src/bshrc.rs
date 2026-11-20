//! `berry bshrc`——BSHRC.CFG 宿主写者与键投影（export 刀挂账 #3 兑现；
//! BSHRC 宿主写者与 PATH 端到端刀 D1/D2）。
//!
//! 落点契约（docs/存储布局与沙箱设计-v1.0.md：跨 boot 持久层 v1＝FAT 数据
//! 盘卷根 8.3 文件族）：本模块只写卷根 `BSHRC.CFG` 名下三面（数据簇、其
//! FAT 表项、其根目录槽）——盘上其余字节按构造零变（settings 五键链零漂
//! 移，盘保真断言执法）。客机端零改：`bsh` rc_load 现语义（帽/静默/CRLF/
//! 装载点）是写端适配对象非改动对象——「写端适配读端而非反之」。
//!
//! 写基元单源＝berry-fatfs 纯函数（客机 fsmgr 消费同 crate——「写读两端
//! FAT 实现对拍」按构造成立；mkfat 降级 dev-dep 只做字节恒等对拍 oracle）。
//! 卷定位＝fsmgr 形（探 {LBA0,LBA16}＋parse_bpb 过＋hidden==探测位对账＋
//! bps==512/FatBits::Fat12 双断言——严于 crashlog 读面 bpb_ok 形）。crashlog
//! `read_root_file` 零改、独立读面 oracle 专用（与 berry-fatfs 不同码路径
//! 的真独立对拍；双读面公共层抽取挂账整洁刀）。
//!
//! 提交形＝整盘读入内存编辑（新链分配→数据写簇尾零填→FAT 全副本→目录项
//! 32B 同槽覆写→旧链 FAT 全副本释放）→同目录 temp 件 write_all＋fsync＋
//! rename 原子换名——rename 前任意宿主中断＝旧值完整（撕裂结构性不出现；
//! crashlog FatErr::Torn 前车在册）；像内提交序（目录项末位＋旧链释放后
//! 置）保留为内存编辑纪律。写后经 crashlog 独立读面自回验，不等值不落盘。
//!
//! 写端帽 fail-closed（总量 ≤2048B 单簇/Entry 行 ≤63/单行内容 ≤126B/键
//! 1..=32B/键值禁 CR·LF·NUL——读端 rc_load 截断面结构性不可达）；env 通道
//! 帽（16 项/256B）投影软警不拒写。多簇旧件（客机 `>` 手写形）list/get 只
//! 读容忍（≤8KiB/64 行同 rc_load 帽）；set/rm 收拢单簇新链旧链释放——旧件
//! 模型序列化超帽即拒写（fail-closed 指名违例帽，rm 可先收编）。
//!
//! 写前在用护栏（G7）：活 berry 会话的缺省盘与目标相同即拒写（fsmgr eager
//! 整读 FAT0＋根进 RAM——活跑宿主写必被客机下次 commit 覆写＝双向毁写态）；
//! 体系外手起 QEMU 无 meta＝残余风险（README 文档化，不筑锁）。

use std::path::{Path, PathBuf};

use berry_fatfs::{
    count_free_clusters, dir_entry, encode_file_entry, fat11_display, fat12_entry, fat12_set,
    find_free_cluster, name_to_fat11, parse_bpb, DirKind, FatBits, FatEntry, Bpb, FAT12_EOC,
};

use crate::error::{BerryError, Result};
use crate::qemu::default_data_disk;
use crate::run_dir;

// ── 帽常量（W4：写端 fail-closed；读端帽镜像自 bsh rc_load）──────────────

/// 键长帽（1..=32B——rc_load `eq==0 || eq>32` skip 镜像）。
pub const KEY_MAX: usize = 32;
/// 单行内容帽（key+'='+value 字节；读端 128B 行帽剥 CR 余量）。
pub const LINE_MAX: usize = 126;
/// Entry 行数帽（读端 64 行帽 −1——写端恒留余量）。
pub const ENTRY_MAX: usize = 63;
/// 序列化总量帽（单簇；settings_io 单簇覆写先例同款）。
pub const TOTAL_MAX: usize = 2048;
/// 读端总量帽（rc_load 8KiB——多簇旧件只读容忍界）。
pub const RC_READ_CAP: usize = 8192;
/// 读端行帽（rc_load `len > 128` skip）。
pub const RC_LINE_SKIP: usize = 128;
/// 读端 Entry 行帽（rc_load `lines >= 64` break）。
pub const RC_ENTRY_CAP: usize = 64;
/// env 通道项数帽（berry-libc io.rs 子侧 16 项截断——软警阈值＝帽本体）。
pub const ENV_ITEMS_CAP: usize = 16;
/// env 通道 blob 帽（berry_argv::env_blob 256B＝CHAN_MSG_MAX——软警阈值）。
pub const ENV_BLOB_CAP: usize = 256;

/// 目标文件 8.3 名（"BSHRC   CFG" 填空格形态）。
fn bshrc_name11() -> [u8; 11] {
    // name_to_fat11 只收 [A-Z0-9]——编译期常量输入恒过；失败即代码级错误。
    name_to_fat11(b"BSHRC.CFG").expect("BSHRC.CFG 恒为合法 8.3 名")
}

// ── 文档模型（W3）───────────────────────────────────────────────────────

/// 行模型：`Comment` 承载一切非 Entry 行的原字节（注释/空行/无 '=' 行/
/// 超长行/坏键行——原样保留原位，客机 `>` 手写件不破坏）；`Entry` 为解析
/// 面（值已剥 CR）。序列化 LF-only、行尾换行收尾（Comment 原字节直发——
/// 客机 CRLF 遗留的 `\r` 属内容面非分隔面）。
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Line {
    Comment(Vec<u8>),
    Entry { key: Vec<u8>, value: Vec<u8> },
}

/// rc_load 同款解析（宿主镜像）：split '\n' → 尾 '\r' 剥除（在行帽检查后）
/// → 首 '=' 切键 → 键 1..=32B；空行/首 '#'/行 >128B/无 '=' 行不入键面
/// （Comment 原样保留）。总量帽模拟：>8192 时读侧停在 ≤8192+256 处
/// （rc_load 256B 读块同构——见 bsh rc_load 的 read 循环）；尾换行产生的
/// 幻影空段丢弃（rc_load `is_empty` skip 的结构性对应，真空行保留）。
pub fn parse_lines(data: &[u8]) -> Vec<Line> {
    let end = if data.len() > RC_READ_CAP {
        (RC_READ_CAP + 256).min(data.len())
    } else {
        data.len()
    };
    let body = &data[..end];
    let mut segs: Vec<&[u8]> = body.split(|&b| b == b'\n').collect();
    if body.last() == Some(&b'\n') {
        segs.pop(); // 幻影空段（真末行换行收尾的副产物，非空行）
    }
    let mut out = Vec::with_capacity(segs.len());
    for raw in segs {
        if raw.is_empty() || raw[0] == b'#' || raw.len() > RC_LINE_SKIP {
            out.push(Line::Comment(raw.to_vec()));
            continue;
        }
        // CRLF 剥除在行帽检查之后（rc_load 微变钉板：CRLF 行内容帽实为
        // 127B+CR）。
        let line = match raw.strip_suffix(b"\r") {
            Some(stripped) => stripped,
            None => raw,
        };
        match line.iter().position(|&b| b == b'=') {
            Some(eq) if eq >= 1 && eq <= KEY_MAX => out.push(Line::Entry {
                key: line[..eq].to_vec(),
                value: line[eq + 1..].to_vec(),
            }),
            _ => out.push(Line::Comment(raw.to_vec())),
        }
    }
    out
}

/// 序列化：逐行发字节＋'\n'，行尾换行收尾；空模型产空串（零簇空文件形）。
pub fn serialize_lines(lines: &[Line]) -> Vec<u8> {
    let mut out = Vec::new();
    for l in lines {
        match l {
            Line::Comment(b) => out.extend_from_slice(b),
            Line::Entry { key, value } => {
                out.extend_from_slice(key);
                out.push(b'=');
                out.extend_from_slice(value);
            }
        }
        out.push(b'\n');
    }
    out
}

/// last-wins 唯一键投影（build_export_set 语义：rev 扫最新值胜出，正序按
/// 最后赋值先后）。get/list 取此投影。
pub fn project(lines: &[Line]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for l in lines.iter().rev() {
        if let Line::Entry { key, value } = l {
            if out.iter().any(|(k, _)| k == key) {
                continue;
            }
            out.push((key.clone(), value.clone()));
        }
    }
    out.reverse();
    out
}

/// set 模型编辑：改末次出现、清更早重复；缺席文件尾追。
fn model_set(lines: &mut Vec<Line>, key: &[u8], value: &[u8]) {
    let last = lines
        .iter()
        .rposition(|l| matches!(l, Line::Entry { key: k, .. } if k == key));
    match last {
        Some(i) => {
            lines[i] = Line::Entry { key: key.to_vec(), value: value.to_vec() };
            // 原地压缩清更早重复（swap 保序——只跳过被清项）。
            let mut w = 0usize;
            for j in 0..lines.len() {
                if j != i && matches!(&lines[j], Line::Entry { key: k, .. } if k == key) {
                    continue;
                }
                if w != j {
                    lines.swap(w, j);
                }
                w += 1;
            }
            lines.truncate(w);
        }
        None => lines.push(Line::Entry { key: key.to_vec(), value: value.to_vec() }),
    }
}

/// rm 模型编辑：删该键全部 Entry 行（注释保留）。返回是否有删动。
fn model_rm(lines: &mut Vec<Line>, key: &[u8]) -> bool {
    let before = lines.len();
    lines.retain(|l| !matches!(l, Line::Entry { key: k, .. } if k == key));
    lines.len() != before
}

/// 写端帽 fail-closed 校验（超帽拒绝指名违例帽——usage 形 code 2）：
/// 先验新写对（键帽/禁字面），再验整模型（Entry 行数/单行内容/总量——旧件
/// 遗留的超帽行在 set 时一并拦下，fail-closed）。返回序列化字节。
fn check_caps(pairs: &[(Vec<u8>, Vec<u8>)], lines: &[Line], clus_bytes: usize) -> Result<Vec<u8>> {
    let bad_byte = |b: &u8| matches!(b, b'\r' | b'\n' | 0);
    for (k, v) in pairs {
        // 跨刀 review 修（2026-11-20）：键字符集白名单——镜像 bsh is_var_name
        //（[A-Za-z_][A-Za-z0-9_]*）——否则 '9X=1' 落盘可装载可导出但展开语法
        // 不可寻址（幽灵变量），违背「写得出读端必可见」fail-closed 宣称。
        let name_ok = |k: &[u8]| {
            !k.is_empty()
                && (k[0].is_ascii_alphabetic() || k[0] == b'_')
                && k.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
        };
        if !name_ok(k) {
            return Err(BerryError::new(
                2,
                format!(
                    "bshrc: 键字符集违例（须 [A-Za-z_][A-Za-z0-9_]*——bsh 展开语法可寻址）：{}",
                    String::from_utf8_lossy(k)
                ),
            ));
        }
        if k.is_empty() || k.len() > KEY_MAX {
            return Err(BerryError::new(
                2,
                format!(
                    "bshrc: 键长 {}B 超帽（键 1..={KEY_MAX}B）：{}",
                    k.len(),
                    String::from_utf8_lossy(k)
                ),
            ));
        }
        if k.iter().any(bad_byte) || v.iter().any(bad_byte) {
            return Err(BerryError::new(
                2,
                format!(
                    "bshrc: 键值禁 CR·LF·NUL：{}={}",
                    String::from_utf8_lossy(k),
                    String::from_utf8_lossy(v)
                ),
            ));
        }
    }
    let entries: Vec<&Line> = lines
        .iter()
        .filter(|l| matches!(l, Line::Entry { .. }))
        .collect();
    if entries.len() > ENTRY_MAX {
        return Err(BerryError::new(
            2,
            format!(
                "bshrc: Entry 行 {} 超帽（≤{ENTRY_MAX}——读端 64 行帽留余量）",
                entries.len()
            ),
        ));
    }
    for e in &entries {
        if let Line::Entry { key, value } = e {
            let n = key.len() + 1 + value.len();
            if n > LINE_MAX {
                return Err(BerryError::new(
                    2,
                    format!(
                        "bshrc: 单行内容 {}B 超帽（≤{LINE_MAX}——键 {}）",
                        n,
                        String::from_utf8_lossy(key)
                    ),
                ));
            }
        }
    }
    let ser = serialize_lines(lines);
    if ser.len() > TOTAL_MAX || ser.len() > clus_bytes {
        return Err(BerryError::new(
            2,
            format!(
                "bshrc: 序列化总量 {}B 超帽（单簇 ≤{TOTAL_MAX}B/簇容 {clus_bytes}B）\
                 ——删键/缩值后重试",
                ser.len()
            ),
        ));
    }
    Ok(ser)
}

/// env 通道帽投影软警（W4：不拒写）：last-wins 投影按 berry-exec
/// `KEY=VALUE\0` 续装串律计 blob 需求，项数 ≥16 或 blob ≥256B 即将截断。
pub fn env_cap_warning(lines: &[Line]) -> Option<String> {
    let proj = project(lines);
    let mut blob = 0usize;
    for (k, v) in &proj {
        blob += k.len() + v.len() + 2; // '=' + '\0'
    }
    if proj.len() >= ENV_ITEMS_CAP || blob >= ENV_BLOB_CAP {
        return Some(format!(
            "bshrc: 子进程 env 通道帽（{ENV_ITEMS_CAP} 项/{ENV_BLOB_CAP}B）将截断\
             （现投影 {} 项/blob {}B——svcmgr/berry-term env 转发链挂账另刀）",
            proj.len(),
            blob
        ));
    }
    None
}

// ── 卷几何与定位（W2）───────────────────────────────────────────────────

/// 定位成功的卷（一切偏移为盘绝对字节）。
pub struct Vol {
    pub bpb: Bpb,
    /// 卷首扇区号（盘绝对）。
    pub vol_sec: usize,
}

impl Vol {
    fn byte_off(&self, sec: usize) -> usize {
        (self.vol_sec + sec) * 512
    }
    /// 第 i 份 FAT 首字节偏移。
    pub fn fat_off(&self, i: usize) -> usize {
        self.byte_off(self.bpb.fat_sector as usize + i * self.bpb.fat_sz as usize)
    }
    pub fn fat_len(&self) -> usize {
        self.bpb.fat_sz as usize * 512
    }
    pub fn root_off(&self) -> usize {
        self.byte_off(self.bpb.root_sector as usize)
    }
    pub fn data_off(&self) -> usize {
        self.byte_off(self.bpb.data_sector as usize)
    }
    pub fn clus_bytes(&self) -> usize {
        self.bpb.spc as usize * 512
    }
    /// 簇号 → 盘绝对字节偏移。
    pub fn clus_off(&self, c: u32) -> usize {
        self.data_off() + (c as usize - 2) * self.clus_bytes()
    }
}

/// 卷定位（fsmgr 形镜像）：探 {LBA0, LBA16}——parse_bpb 过 + hidden==探测位
/// 对账（hidden != 位 = 非 mkfat 卷，后续扇区折算整体错位，拒）＋bps==512
/// 与 fat_bits==Fat12 双断言（fsmgr 双拒面——畸形 BPB 防御）。FAT16 域/
/// bps 非 512 命中即拒带诊断（不换位重试——卷已找到，宽/粒度不符）。
pub fn locate(disk: &[u8]) -> Result<Vol> {
    for &cand in &[0usize, 16] {
        let off = cand * 512;
        let Some(boot) = disk.get(off..off + 512) else { continue };
        let Some(g) = parse_bpb(boot) else { continue };
        if g.hidden as usize != cand {
            continue; // EFI 形 vol@LBA0/主路形 vol@LBA16 之外的对账失配
        }
        if g.bps != 512 {
            return Err(BerryError::device(format!(
                "bshrc: 卷 bps={} 非 512（vblk 单扇区粒度域外——fsmgr 双拒面镜像）",
                g.bps
            )));
        }
        if g.fat_bits != FatBits::Fat12 {
            return Err(BerryError::device(format!(
                "bshrc: FAT16 域卷（簇数 {} ≥ 4085）——写面只认 FAT12",
                g.cluster_count
            )));
        }
        // 区域界防御：FAT 全副本＋根目录必须整在盘内（parse_bpb 不交叉核验
        // 盘长；数据区越界由簇级读写逐次拦）。
        let v = Vol { bpb: g, vol_sec: cand };
        let need = v
            .root_off()
            + v.bpb.root_entries as usize * 32;
        if need > disk.len() {
            return Err(BerryError::device(
                "bshrc: FAT/根目录区越出盘界（截短盘或畸形 BPB）",
            ));
        }
        return Ok(v);
    }
    Err(BerryError::device(
        "bshrc: 找不到合法 FAT12 卷（LBA0/LBA16 皆否——裸件或非 mkfat 形）",
    ))
}

fn fat0<'a>(disk: &'a [u8], v: &Vol) -> &'a [u8] {
    let o = v.fat_off(0);
    &disk[o..o + v.fat_len()]
}

/// 旧链走查（环防护＝链长超簇域拒）：Eoc 止；Free/畸形中途＝撕裂形 loud。
fn collect_chain(disk: &[u8], v: &Vol, first: u32) -> Result<Vec<u32>> {
    if first == 0 {
        return Ok(Vec::new()); // 空文件形（first=0/size=0）
    }
    if first < 2 {
        return Err(BerryError::device("bshrc: 旧链首簇 1（保留项）——撕裂形"));
    }
    let fat = fat0(disk, v);
    let mut chain = vec![first];
    let mut c = first;
    loop {
        match fat12_entry(fat, c) {
            Some(FatEntry::Eoc) => return Ok(chain),
            Some(FatEntry::Next(n)) => {
                chain.push(n);
                if chain.len() > v.bpb.cluster_count as usize {
                    return Err(BerryError::device("bshrc: 旧链环（链长超簇域）"));
                }
                c = n;
            }
            _ => {
                return Err(BerryError::device(format!(
                    "bshrc: 旧链撕裂/畸形（簇 {c} 表项非链指针）"
                )))
            }
        }
    }
}

// ── 根目录扫描 ──────────────────────────────────────────────────────────

/// 根目录一次扫描的产物：目标项在席（槽位/首簇/size）、缺席时的插入槽
/// （首个 0x00/0xE5 空槽）、在场名清单（根满报错附清单用）。
struct RootScan {
    hit: Option<(usize, u32, u32)>,
    free_slot: Option<usize>,
    names: Vec<String>,
}

fn scan_root(disk: &[u8], v: &Vol, name11: &[u8; 11]) -> Result<RootScan> {
    let root_off = v.root_off();
    let mut scan = RootScan { hit: None, free_slot: None, names: Vec::new() };
    for i in 0..v.bpb.root_entries as usize {
        let o = root_off + i * 32;
        let Some(e) = disk.get(o..o + 32) else { break };
        if e[0] == 0x00 {
            scan.free_slot.get_or_insert(i); // End——其后全无效
            break;
        }
        if e[0] == 0xE5 {
            scan.free_slot.get_or_insert(i); // 删除槽可复用
            continue;
        }
        let attr = e[11];
        if attr == 0x0F || attr & 0x08 != 0 {
            continue; // LFN/卷标（dir_entry Skip 同判）
        }
        let mut cur11 = [0u8; 11];
        cur11.copy_from_slice(&e[0..11]);
        if &cur11 == name11 {
            // End/Skip 两臂结构性不可达（首字节 0x00/0xE5 与 LFN/卷标 attr
            // 已在前滤除）——留作防御归并进畸形拒绝。
            return match dir_entry(e) {
                Some(DirKind::File(f)) => {
                    scan.hit = Some((i, f.first_clus, f.size));
                    Ok(scan)
                }
                Some(DirKind::Dir(_)) => Err(BerryError::device(
                    "bshrc: 根目录 BSHRC.CFG 是目录项（类型冲突）",
                )),
                Some(DirKind::End | DirKind::Skip) | None => Err(BerryError::device(
                    "bshrc: BSHRC.CFG 目录项畸形（首簇/size 域非法）",
                )),
            };
        }
        let mut buf = [0u8; 12];
        if let Some(n) = fat11_display(&cur11, &mut buf) {
            scan.names.push(String::from_utf8_lossy(&buf[..n]).into_owned());
        }
    }
    Ok(scan)
}

// ── 读面（自有——berry-fatfs 基元走查；crashlog 读面是写后 oracle）──────

/// 卷根文件直读（链走查＋簇尾截到 size；撕裂/越界 loud）。
fn read_file(disk: &[u8], v: &Vol, first: u32, size: u32) -> Result<Vec<u8>> {
    let chain = collect_chain(disk, v, first)?;
    let size = size as usize;
    let mut out = Vec::with_capacity(size.min(disk.len()));
    for &c in &chain {
        let o = v.clus_off(c);
        let seg = disk
            .get(o..o + v.clus_bytes())
            .ok_or_else(|| BerryError::device(format!("bshrc: 簇 {c} 越出盘界（截短盘）")))?;
        out.extend_from_slice(seg);
        if out.len() >= size {
            break;
        }
    }
    if out.len() < size {
        return Err(BerryError::device(format!(
            "bshrc: 链短于 size（{}/{}B——撕裂形）",
            out.len(),
            size
        )));
    }
    out.truncate(size);
    Ok(out)
}

/// 读 BSHRC.CFG 原字节（缺席产 None——rc_load 缺席常态镜像）。
fn read_bshrc(disk: &Path) -> Result<Option<Vec<u8>>> {
    let image = read_disk(disk)?;
    let v = locate(&image)?;
    let scan = scan_root(&image, &v, &bshrc_name11())?;
    match scan.hit {
        None => Ok(None),
        Some((_, first, size)) => Ok(Some(read_file(&image, &v, first, size)?)),
    }
}

fn read_disk(disk: &Path) -> Result<Vec<u8>> {
    std::fs::read(disk).map_err(|e| {
        BerryError::device(format!("bshrc: 读 {}: {e}", disk.display()))
            .with_hint("盘缺席时先 just desktop 造盘，或 --disk 显式指路")
    })
}

// ── 命令面操作（C4：错误全走 BerryError 既有家族，非零退出禁静默）───────

/// `get KEY`：last-wins 末值；缺席 `Ok(None)`（CLI 面转 rc≠0＋诊断）。
pub fn get(disk: &Path, key: &[u8]) -> Result<Option<Vec<u8>>> {
    let Some(data) = read_bshrc(disk)? else { return Ok(None) };
    Ok(project(&parse_lines(&data))
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v))
}

/// `list`：last-wins 投影（缺席产空表——rc_load 缺席常态镜像，rc 0）。
pub fn list(disk: &Path) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let Some(data) = read_bshrc(disk)? else { return Ok(Vec::new()) };
    Ok(project(&parse_lines(&data)))
}

/// set/rm 落盘结果（--json 面的 path/key/bytes/clusters 材料）。
#[derive(Debug)]
pub struct WriteOutcome {
    pub bytes: usize,
    pub clusters: usize,
}

/// `set KEY=VALUE ...`：多对同批顺序应用、模型改毕一次落盘。
pub fn set(disk: &Path, pairs: &[(Vec<u8>, Vec<u8>)]) -> Result<WriteOutcome> {
    // 早验新写对（键帽/禁字面——盘未动先拒，usage 形）。
    check_caps(pairs, &[] as &[Line], usize::MAX)?;
    commit(disk, |lines| {
        for (k, v) in pairs {
            model_set(lines, k, v);
        }
        true
    })
    .map(|opt| opt.expect("set 编辑恒为 Some"))
}

/// `rm KEY`：删该键全部行；键缺席 `Ok(None)`（CLI 面转 rc≠0）。
pub fn rm(disk: &Path, key: &[u8]) -> Result<Option<WriteOutcome>> {
    commit(disk, |lines| model_rm(lines, key))
}

/// 提交序（W5）。编辑闭包返回 false＝键缺席（rm miss）——盘未动直接回。
fn commit(disk: &Path, edit: impl FnOnce(&mut Vec<Line>) -> bool) -> Result<Option<WriteOutcome>> {
    assert_disk_idle(disk)?; // G7 在用护栏（写前——活会话拒写）
    let mut image = read_disk(disk)?;
    let v = locate(&image)?;
    // FAT 双副本一致性：撕裂盘不静默「修复」（mkfat/fsmgr 恒双副本同值）。
    let fat_len = v.fat_len();
    let fat0_range = v.fat_off(0)..v.fat_off(0) + fat_len;
    for i in 1..v.bpb.nfats as usize {
        let o = v.fat_off(i);
        if image[o..o + fat_len] != image[fat0_range.clone()] {
            return Err(BerryError::device(
                "bshrc: FAT 双副本不一致（撕裂盘）——拒绝写",
            ));
        }
    }
    let name11 = bshrc_name11();
    let scan = scan_root(&image, &v, &name11)?;
    // ① 旧链 collect 暂记（缺席则无旧链）＋旧内容读出 → 模型。
    let (old_chain, mut lines) = match scan.hit {
        Some((_, first, size)) => {
            let data = read_file(&image, &v, first, size)?;
            (collect_chain(&image, &v, first)?, parse_lines(&data))
        }
        None => (Vec::new(), Vec::new()),
    };
    if !edit(&mut lines) {
        return Ok(None); // rm 键缺席：盘零触
    }
    let ser = check_caps(&[], &lines, v.clus_bytes())?;
    // ② 新链分配（find_free_cluster 自 2 升序——mkfat/fsmgr 确定性惯例；
    //    单簇帽下恒 1 簇；旧链未释放先验＝无重用面。空内容特判零簇）。
    let first_new = if ser.is_empty() {
        0
    } else {
        let free_total = count_free_clusters(fat0(&image, &v), v.bpb.cluster_count, FatBits::Fat12)
            .ok_or_else(|| BerryError::device("bshrc: FAT 畸形项（空闲计数拒绝）"))?;
        find_free_cluster(fat0(&image, &v), 2, v.bpb.cluster_count, FatBits::Fat12).ok_or_else(
            || {
                BerryError::device(format!(
                    "bshrc: 无空闲簇（free={free_total}）——盘满，清理后重试"
                ))
            },
        )?
    };
    // ③ 数据逐簇写（簇尾 0x00 填——单簇即一簇）。
    if first_new != 0 {
        let o = v.clus_off(first_new);
        let end = o + v.clus_bytes();
        if end > image.len() {
            return Err(BerryError::device(format!(
                "bshrc: 簇 {first_new} 越出盘界（截短盘）"
            )));
        }
        image[o..end].fill(0);
        image[o..o + ser.len()].copy_from_slice(&ser);
    }
    // ④ FAT 全副本写新链（末簇 FAT12_EOC——单簇链即首簇 EOC）。
    for i in 0..v.bpb.nfats as usize {
        let o = v.fat_off(i);
        if first_new != 0
            && fat12_set(&mut image[o..o + fat_len], first_new, FAT12_EOC).is_none()
        {
            return Err(BerryError::device("bshrc: FAT 表项写拒绝（切片短）"));
        }
    }
    // ⑤ 目录项 32B 一次写（encode_file_entry——attr 0x20/date 定值/time 0，
    //    与 mkfat 及客机 fsmgr 写字节同形；同槽覆写/缺席取空槽）。
    let slot = match scan.hit {
        Some((slot, _, _)) => slot,
        None => scan.free_slot.ok_or_else(|| {
            BerryError::device(format!(
                "bshrc: 根目录 16 槽满（在场：{}）——扩帽归 mkfat 几何面",
                scan.names.join(", ")
            ))
        })?,
    };
    let mut e32 = [0u8; 32];
    if encode_file_entry(&name11, first_new, ser.len() as u32, &mut e32, FatBits::Fat12)
        .is_none()
    {
        return Err(BerryError::device("bshrc: 目录项编码拒绝（簇号/size 域）"));
    }
    let eo = v.root_off() + slot * 32;
    image[eo..eo + 32].copy_from_slice(&e32);
    // ⑥ 旧链 FAT 全副本释放（改 0——新链分配在其前，无成员交集）。
    for i in 0..v.bpb.nfats as usize {
        let o = v.fat_off(i);
        for &c in &old_chain {
            if fat12_set(&mut image[o..o + fat_len], c, 0).is_none() {
                return Err(BerryError::device("bshrc: FAT 表项写拒绝（切片短）"));
            }
        }
    }
    // ⑦ 独立读面自回验（crashlog read_root_file——与 berry-fatfs 不同码
    //    路径；不等值不落盘）。
    match crate::crashlog::read_root_file(&image, "BSHRC.CFG") {
        Ok(got) if got == ser => {}
        other => {
            return Err(BerryError::device(format!(
                "bshrc: 写后回验失败（crashlog 读面 {other:?}）——不落盘"
            )))
        }
    }
    persist_atomic(disk, &image)?;
    if let Some(w) = env_cap_warning(&lines) {
        eprintln!("{w}"); // 软警走 stderr（--json 契约：stdout 纯 JSON）
    }
    Ok(Some(WriteOutcome { bytes: ser.len(), clusters: usize::from(first_new != 0) }))
}

/// 同目录 temp 件 write_all＋fsync＋rename 原子换名（失败清 temp；原盘
/// 模式位保随）。
fn persist_atomic(path: &Path, image: &[u8]) -> Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or(Path::new("."));
    let fname = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "disk.img".into());
    let tmp = dir.join(format!(".{fname}.bshrc-tmp-{}", std::process::id()));
    let r = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(image)?;
        f.sync_all()?;
        drop(f);
        if let Ok(meta) = std::fs::metadata(path) {
            std::fs::set_permissions(&tmp, meta.permissions())?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if r.is_err() {
        std::fs::remove_file(&tmp).ok();
    }
    r.map_err(|e| BerryError::device(format!("bshrc: 原子换名落盘失败（{}）：{e}", path.display())))
}

// ── 盘解析（C2——与 data_disk_plan 三级同序的写语境独立薄函数）───────────

/// 数据盘写语境解析：--disk 显式（canonicalize）> BERRY_DATA_DISK（"off"＝
/// 显式关→写语境报错说明；路径直用）> default_data_disk(profile) 存在即用。
/// 与 qemu.rs `data_disk_plan` 同序但独立（挂载语义 None 是合法态不可复用
/// ——写语境须拆「off 显式关」与「缺盘」两类可行动报错）。缺盘报错附
/// home()/data 盘清单与造盘指路（G4：只复用不造盘）。
pub fn resolve_disk(disk: Option<&Path>, profile_id: &str) -> Result<PathBuf> {
    if let Some(p) = disk {
        return std::fs::canonicalize(p).map_err(|e| {
            BerryError::device(format!("bshrc: --disk {} 不可达：{e}", p.display()))
                .with_hint("盘路径须存在（berry 只复用不造盘——首造归 just desktop）")
        });
    }
    if let Some(v) = std::env::var_os("BERRY_DATA_DISK") {
        if v.to_string_lossy() == "off" {
            return Err(BerryError::env(
                "bshrc: BERRY_DATA_DISK=off（显式关）——写语境无盘可写",
            ));
        }
        let p = PathBuf::from(v);
        if !p.is_file() {
            return Err(BerryError::device(format!(
                "bshrc: BERRY_DATA_DISK 盘缺席：{}",
                p.display()
            ))
            .with_hint("先 just desktop 造盘或 --disk 指路"));
        }
        return Ok(std::fs::canonicalize(&p).unwrap_or(p));
    }
    // profile 取值校验沿单源（缺省解析路径才消费 profile）。
    crate::profile::get(profile_id)?;
    let p = default_data_disk(profile_id);
    if !p.is_file() {
        let listing = data_dir_listing();
        return Err(BerryError::device(format!(
            "bshrc: 缺省数据盘缺席：{}（data/ 现有 [{}]）",
            p.display(),
            listing
        ))
        .with_hint("先 just desktop 造盘（一次造跨启动复用），或 --disk 显式指路"));
    }
    Ok(std::fs::canonicalize(&p).unwrap_or(p))
}

fn data_dir_listing() -> String {
    let dir = run_dir::home().join("data");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_file() && e.path().extension().is_some_and(|x| x == "img"))
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names.join(", ")
}

// ── 在用护栏（C3/G7）────────────────────────────────────────────────────

/// 写前扫 run 注册表：活 berry 会话的缺省盘与目标 canonical 路径相等即拒写。
/// 活态判据＝meta qemu_pid 探活，或（meta pid 探活＋state.json=running）。
/// meta 无 profile 的活 shot 会话保守拒写（shot 起像挂 profile 盘但 meta
/// 未记——无法对账，宁拒勿毁）；`berry run` 起核不挂数据盘（label=run 放
/// 行）。体系外手起 QEMU 无 meta＝残余风险（README 文档化，不筑锁）。
pub fn assert_disk_idle(target: &Path) -> Result<()> {
    let target = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let root = run_dir::home().join("run");
    let Ok(rd) = std::fs::read_dir(&root) else { return Ok(()) };
    let mut dirs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for d in dirs {
        let Ok(b) = std::fs::read(d.join("meta.json")) else { continue };
        let Ok(meta) = serde_json::from_slice::<serde_json::Value>(&b) else { continue };
        let qemu_pid = meta.get("qemu_pid").and_then(|x| x.as_u64()).unwrap_or(0);
        let pid = meta.get("pid").and_then(|x| x.as_u64()).unwrap_or(0);
        let running = run_dir::read_state(&d).as_deref() == Some("running");
        let live = crate::commands::berry_pid_alive(qemu_pid)
            || (running && crate::commands::berry_pid_alive(pid));
        if !live {
            continue;
        }
        let id = d
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        match meta.get("profile").and_then(|x| x.as_str()) {
            Some(profile) => {
                let dp = default_data_disk(profile);
                let same = std::fs::canonicalize(&dp)
                    .map(|c| c == target)
                    .unwrap_or(false);
                if same {
                    return Err(BerryError::device(format!(
                        "bshrc: 数据盘 {} 正被活会话 {id}（profile {profile}）使用\
                         ——fsmgr 整读 FAT0 进 RAM，宿主写会被客机下次 commit 覆写",
                        target.display()
                    ))
                    .with_hint(format!("先 berry device stop --run {id} 再写")));
                }
            }
            None => {
                if meta.get("label").and_then(|x| x.as_str()) == Some("shot") {
                    // shot 起像挂 data_disk_plan(profile) 盘但 meta 未记
                    // profile——对账收窄为「target == 任一 profile 缺省盘」
                    //（shot 盘恒在 home()/data/ 域内；显式 BERRY_DATA_DISK
                    // 的 shot env 不可知＝残余文档化）。
                    let hit_profile = crate::profile::PROFILES.iter().any(|p| {
                        std::fs::canonicalize(default_data_disk(p.id))
                            .map(|c| c == target)
                            .unwrap_or(false)
                    });
                    if hit_profile {
                        return Err(BerryError::device(format!(
                            "bshrc: 活 shot 会话 {id} 在途（起像挂数据盘但 meta 未记\
                             profile——保守拒写）"
                        ))
                        .with_hint("等 shot 结束（自起自停）后再写"));
                    }
                }
            }
        }
    }
    Ok(())
}

// ── doctor 薄面（C5——crashlog 读面缺席探测，静态只读零副作用）───────────

/// 盘上 BSHRC.CFG 在席探测（读失败/无卷＝false——doctor 是推演面不是诊断面）。
pub fn bshrc_present(disk: &Path) -> bool {
    std::fs::read(disk)
        .ok()
        .and_then(|image| crate::crashlog::read_root_file(&image, "BSHRC.CFG").ok())
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crashlog::read_root_file;
    use crate::run_dir::ENV_LOCK;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 对真产物造盘：`tools/mkfat.py` 产物（确定性、零 fixture 入库；mkfat
    /// 缺席/失败即测试失败——不许静默跳过）。空件表＝裸调默认模式（主路盘
    /// 形态成品：16 扇区保留区前缀＋vol@LBA16＋hidden=16＋烘焙件）；非空＝
    /// `--put` 可重复预置多件（EFI 引导卷形：vol@LBA0、hidden=0）＋测试侧
    /// 补 16 扇区零前缀与 BPB hidden 字段改 16 对齐主路盘形态（just desktop
    /// 的 Rust mkfat 盘同形——fsmgr 形对账 hidden==探测位由此通过；crashlog
    /// 读面 bpb_ok 不查 hidden，两种形态同读）。
    fn mkfat_disk(files: &[(&str, &[u8])]) -> Vec<u8> {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let uniq = format!(
            "{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let tmp = std::env::temp_dir();
        let img = tmp.join(format!("berry-bshrc-test-disk-{uniq}.img"));
        let mut cmd = std::process::Command::new("python3");
        cmd.arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/mkfat.py"),
        )
        .arg(&img);
        for (name, data) in files {
            let src = tmp.join(format!("berry-bshrc-test-src-{uniq}-{name}"));
            std::fs::write(&src, data).unwrap();
            cmd.args(["--put", &format!("{}:/{}", src.display(), name)]);
        }
        let out = cmd.output().expect("python3 起 mkfat 失败");
        for (name, _) in files {
            std::fs::remove_file(tmp.join(format!("berry-bshrc-test-src-{uniq}-{name}"))).ok();
        }
        assert!(
            out.status.success(),
            "mkfat 造盘失败：{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let vol = std::fs::read(&img).unwrap();
        std::fs::remove_file(&img).ok();
        if files.is_empty() {
            return vol; // 默认模式已是整盘主路形态
        }
        let mut d = vec![0u8; 16 * 512];
        d.extend_from_slice(&vol);
        // hidden 补 16（BPB 偏移 28——主路形态对账，见头注）。
        let voff = 16 * 512;
        d[voff + 28..voff + 32].copy_from_slice(&16u32.to_le_bytes());
        d
    }

    fn write_tmp(data: &[u8]) -> PathBuf {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "berry-bshrc-test-file-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&p, data).unwrap();
        p
    }

    /// 写路径测试的串行＋净 home：持 ENV_LOCK（env 是进程级全局——本模块
    /// 写路径经 assert_disk_idle 读 BERRY_HOME，与设置 env 的测试〔含伪
    /// run 注册表护栏测试〕必须互斥，run_dir.rs 先例纪律）并把 BERRY_HOME
    /// 指到空目录（无 run/ → 护栏 no-op）。guard 活到测试尾。
    fn lock_and_clean_home(tag: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
        let g = ENV_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!(
            "berry-bshrc-home-{tag}-{}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&home).ok();
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("BERRY_HOME", &home);
        (g, home)
    }

    /// 审查修复：RAII 清理面——ENV_LOCK guard 析构时同步清 BERRY_HOME 与
    /// 临时 home 目录（commands.rs:2880「泄漏 BERRY_HOME 连环污染」纪律对齐；
    /// MutexGuard 无法直接附加逻辑，包一层带 Drop 的 newtype）。
    struct HomeGuard(std::sync::MutexGuard<'static, ()>, PathBuf);
    impl Drop for HomeGuard {
        fn drop(&mut self) {
            std::env::remove_var("BERRY_HOME");
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }

    fn lock_home_raii(tag: &str) -> HomeGuard {
        let (g, home) = lock_and_clean_home(tag);
        HomeGuard(g, home)
    }

    // ── 模型族 ──────────────────────────────────────────────────────────

    #[test]
    fn parse_serialize_roundtrip_lf() {
        let src = b"# note\nPATH=/etc\nA=1\n\nB=x=y\nno-eq-line\nK=\n";
        let lines = parse_lines(src);
        // 尾换行幻影空段不产生尾随 Comment；真空行（第 4 段）保留。
        assert_eq!(serialize_lines(&lines), src.to_vec());
        // 值含 '='（首 '=' 切键）与空值均入 Entry。
        let n_entries = lines
            .iter()
            .filter(|l| matches!(l, Line::Entry { .. }))
            .count();
        assert_eq!(n_entries, 4, "{lines:?}");
    }

    #[test]
    fn comments_blanks_and_bad_lines_preserved() {
        // 客机 `>` 手写件不破坏：注释/空行/无 '=' 行/坏键行（空键）原样
        // 原位；模型序列化恒尾换行收尾（输入已收尾＝逐字节往返）。
        let src = b"# c1\nNOKEYLINE\n=NOKEY\nplain text\n\nA=1\n#tail\n";
        let lines = parse_lines(src);
        assert_eq!(serialize_lines(&lines), src.to_vec());
        // 空键（= 开头）与无 '=' 行均不入键面——模型里只有 A 一个 Entry。
        assert_eq!(project(&lines), vec![(b"A".to_vec(), b"1".to_vec())]);
        // 尾无换行形：序列化补尾换行（行尾换行收尾纪律）。
        assert_eq!(serialize_lines(&parse_lines(b"A=1")), b"A=1\n".to_vec());
    }

    #[test]
    fn last_wins_projection() {
        let lines = parse_lines(b"A=1\nB=2\nA=3\nC=4\nB=5\n");
        // rev 扫最新值胜出；正序按各键最后赋值位置升序（A 末值位 3、
        // C 位 4、B 位 5——build_export_set 同序）。
        assert_eq!(
            project(&lines),
            vec![
                (b"A".to_vec(), b"3".to_vec()),
                (b"C".to_vec(), b"4".to_vec()),
                (b"B".to_vec(), b"5".to_vec()),
            ]
        );
    }

    #[test]
    fn crlf_read_tolerance() {
        // CRLF 读容：值不带 \r 入表；序列化产 LF-only（Entry 行归一）。
        let lines = parse_lines(b"PATH=/etc\r\nN=1\r\n");
        assert_eq!(
            project(&lines),
            vec![(b"PATH".to_vec(), b"/etc".to_vec()), (b"N".to_vec(), b"1".to_vec())]
        );
        assert_eq!(serialize_lines(&lines), b"PATH=/etc\nN=1\n".to_vec());
    }

    #[test]
    fn overlong_line_read_tolerated_write_raw() {
        // >128B 行：读端 rc_load skip（不入键面）——模型保留原字节（原样
        // 保留原位），不静默丢。
        let mut src = Vec::new();
        src.extend_from_slice(b"# ok\n");
        src.extend_from_slice(b"L=");
        src.extend_from_slice(&vec![b'x'; 200]);
        src.push(b'\n');
        src.extend_from_slice(b"K=9\n");
        let lines = parse_lines(&src);
        assert_eq!(project(&lines), vec![(b"K".to_vec(), b"9".to_vec())]);
        assert_eq!(serialize_lines(&lines), src);
    }

    #[test]
    fn key_cap_boundaries() {
        // 键恰 32B 过、33B 拒（rc_load eq>32 skip 镜像）。
        let mut src = vec![b'K'; 32];
        src.extend_from_slice(b"=v\n");
        let lines = parse_lines(&src);
        assert_eq!(project(&lines), vec![(vec![b'K'; 32], b"v".to_vec())]);
        let mut src = vec![b'K'; 33];
        src.extend_from_slice(b"=v\n");
        let lines = parse_lines(&src);
        assert!(project(&lines).is_empty(), "超长键不入键面（Comment 保留）");
    }

    #[test]
    fn cap_rejections_name_the_cap() {
        let _hg = lock_home_raii("caps");
        let d = mkfat_disk(&[]);
        let p = write_tmp(&d);
        // 键长帽：33B 键 set 拒（code 2 usage 形，报文指名键帽）。
        let e = set(&p, &[(vec![b'K'; 33], b"v".to_vec())]).unwrap_err();
        assert_eq!(e.code, 2);
        assert!(e.message.contains("键 1..=32B"), "{}", e.message);
        // 键值禁 CR·LF·NUL。
        for bad in [
            (b"K".to_vec(), b"a\rb".to_vec()),
            (b"K".to_vec(), b"a\nb".to_vec()),
        ] {
            let e = set(&p, &[bad]).unwrap_err();
            assert_eq!(e.code, 2, "{}", e.message);
            assert!(e.message.contains("CR·LF·NUL"), "{}", e.message);
        }
        // 键内 CR：字符集白名单先拒（语义同族——写端 fail-closed，2026-11-20
        // 跨刀 review 后 charset 检查先于值字面检查，如实滚期望）。
        {
            let e = set(&p, &[(b"K\r".to_vec(), b"v".to_vec())]).unwrap_err();
            assert_eq!(e.code, 2, "{}", e.message);
            assert!(e.message.contains("字符集"), "{}", e.message);
        }
        // 键字符集白名单（跨刀 review）：非 [A-Za-z_][A-Za-z0-9_]* 拒。
        for bad_key in [b"9X".to_vec(), b"A B".to_vec(), b"_OK9".to_vec().iter().take(0).cloned().collect::<Vec<u8>>()] {
            if bad_key.is_empty() { continue; }
            let e = set(&p, &[(bad_key, b"v".to_vec())]).unwrap_err();
            assert_eq!(e.code, 2);
            assert!(e.message.contains("字符集"), "{}", e.message);
        }
        // 单行内容帽：126B 过、127B 拒（读端 128B 行帽剥 CR 余量）。
        let val = vec![b'x'; 126 - 2]; // K= + 124x = 126B 恰帽
        set(&p, &[(b"K".to_vec(), val.clone())]).unwrap();
        let e = set(&p, &[(b"K".to_vec(), vec![b'x'; 125])]).unwrap_err();
        assert_eq!(e.code, 2);
        assert!(e.message.contains("126"), "{}", e.message);
        // Entry 行帽：64 对 set 拒（≤63）。
        let many: Vec<(Vec<u8>, Vec<u8>)> = (0..64)
            .map(|i| (format!("K{i:02}").into_bytes(), b"v".to_vec()))
            .collect();
        let e = set(&p, &many).unwrap_err();
        assert_eq!(e.code, 2);
        assert!(e.message.contains("63"), "{}", e.message);
        // 63 对恰帽过（清场后重验——K=xx 已在，总量远低帽）。
        let _ = rm(&p, b"K").unwrap();
        let many63: Vec<(Vec<u8>, Vec<u8>)> = (0..63)
            .map(|i| (format!("K{i:02}").into_bytes(), b"v".to_vec()))
            .collect();
        set(&p, &many63).unwrap();
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn total_cap_2048_rejected() {
        let _hg = lock_home_raii("total");
        let d = mkfat_disk(&[]);
        let p = write_tmp(&d);
        // 63 行 × 恰 126B（键 5+值 120）——行帽/行数帽全过、总量超 2048。
        let pairs: Vec<(Vec<u8>, Vec<u8>)> = (0..63)
            .map(|i| (format!("KEY{i:02}").into_bytes(), vec![b'v'; 120]))
            .collect();
        let ser_len: usize = pairs.iter().map(|(k, v)| k.len() + v.len() + 2).sum();
        assert!(ser_len > TOTAL_MAX, "测试载荷须真超帽（{ser_len}）");
        let e = set(&p, &pairs).unwrap_err();
        assert_eq!(e.code, 2);
        assert!(e.message.contains("2048"), "{}", e.message);
        // 盘零触：未落盘（文件无 BSHRC.CFG）。
        let image = std::fs::read(&p).unwrap();
        assert!(matches!(
            read_root_file(&image, "BSHRC.CFG"),
            Err(crate::crashlog::FatErr::NoEnt(_))
        ));
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn env_soft_warning_thresholds() {
        let _hg = lock_home_raii("envwarn");
        // 项数帽：15 项静默、16 项即警（帽本体阈值）。
        let mk = |n: usize| -> Vec<Line> {
            (0..n)
                .map(|i| Line::Entry {
                    key: format!("K{i:02}").into_bytes(),
                    value: b"v".to_vec(),
                })
                .collect()
        };
        assert!(env_cap_warning(&mk(15)).is_none());
        let w = env_cap_warning(&mk(16)).unwrap();
        assert!(w.contains("16 项/256B") && w.contains("将截断"), "{w}");
        // blob 帽：3 项长值顶破 256B（3×(4+84+2)=270 ≥ 256）即警。
        let fat: Vec<Line> = (0..3)
            .map(|i| Line::Entry {
                key: format!("KEY{i}").into_bytes(),
                value: vec![b'v'; 84],
            })
            .collect();
        let w = env_cap_warning(&fat).unwrap();
        assert!(w.contains("256B"), "{w}");
        // 软警不拒写：16 项照写成功（commit 层不因帽拦）。
        let d = mkfat_disk(&[]);
        let p = write_tmp(&d);
        let pairs: Vec<(Vec<u8>, Vec<u8>)> = (0..16)
            .map(|i| (format!("K{i:02}").into_bytes(), b"v".to_vec()))
            .collect();
        set(&p, &pairs).unwrap();
        assert_eq!(list(&p).unwrap().len(), 16);
        std::fs::remove_file(&p).ok();
    }

    // ── 写面族 ──────────────────────────────────────────────────────────

    fn free_count(image: &[u8]) -> u32 {
        let v = locate(image).unwrap();
        count_free_clusters(fat0(image, &v), v.bpb.cluster_count, FatBits::Fat12).unwrap()
    }

    #[test]
    fn create_readback_byte_identical() {
        let _hg = lock_home_raii("readback");
        let d = mkfat_disk(&[]);
        let p = write_tmp(&d);
        let out = set(&p, &[(b"PATH".to_vec(), b"/etc".to_vec())]).unwrap();
        assert_eq!((out.bytes, out.clusters), (10, 1));
        let image = std::fs::read(&p).unwrap();
        // 独立读面回读字节恒等。
        assert_eq!(
            read_root_file(&image, "BSHRC.CFG").unwrap(),
            b"PATH=/etc\n".to_vec()
        );
        // 空闲簇守恒：新建耗 1。
        assert_eq!(free_count(&image), free_count(&d) - 1);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn update_append_and_rm_to_empty_form() {
        let _hg = lock_home_raii("updrm");
        let d = mkfat_disk(&[]);
        let p = write_tmp(&d);
        set(&p, &[(b"A".to_vec(), b"1".to_vec())]).unwrap();
        // 同键更新（末值胜出）＋新键尾追。
        set(&p, &[(b"A".to_vec(), b"2".to_vec()), (b"B".to_vec(), b"x".to_vec())])
            .unwrap();
        assert_eq!(
            list(&p).unwrap(),
            vec![(b"A".to_vec(), b"2".to_vec()), (b"B".to_vec(), b"x".to_vec())]
        );
        // rm 至空文件形：目录项留驻 first=0/size=0，簇全释放。
        rm(&p, b"A").unwrap();
        rm(&p, b"B").unwrap();
        let image = std::fs::read(&p).unwrap();
        assert_eq!(read_root_file(&image, "BSHRC.CFG").unwrap(), Vec::<u8>::new());
        let v = locate(&image).unwrap();
        let scan = scan_root(&image, &v, &bshrc_name11()).unwrap();
        assert_eq!(scan.hit.map(|(_, f, s)| (f, s)), Some((0, 0)), "空文件形留驻");
        assert_eq!(free_count(&image), free_count(&d), "簇全释放");
        // rm 缺席键：Ok(None) 且盘零触。
        let before = std::fs::read(&p).unwrap();
        assert!(rm(&p, b"NOPE".to_vec().as_slice()).unwrap().is_none());
        assert_eq!(std::fs::read(&p).unwrap(), before);
        // list 空文件 → 空表；get miss。
        assert!(list(&p).unwrap().is_empty());
        assert!(get(&p, b"A").unwrap().is_none());
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn duplicate_keys_set_clears_earlier_and_rm_keeps_comments() {
        let _hg = lock_home_raii("dup");
        // 客机 `>` 手写形：重复键 + 注释混排。
        let src = b"# head\nA=1\n# mid\nB=2\nA=3\n";
        let d = mkfat_disk(&[("BSHRC.CFG", src)]);
        let p = write_tmp(&d);
        // get/list 投影 last-wins（build_export_set 同序：按最后赋值升序）。
        assert_eq!(get(&p, b"A").unwrap(), Some(b"3".to_vec()));
        assert_eq!(
            list(&p).unwrap(),
            vec![(b"B".to_vec(), b"2".to_vec()), (b"A".to_vec(), b"3".to_vec())]
        );
        // set 改末次出现清更早重复——注释保留原位。
        set(&p, &[(b"A".to_vec(), b"9".to_vec())]).unwrap();
        let image = std::fs::read(&p).unwrap();
        assert_eq!(
            read_root_file(&image, "BSHRC.CFG").unwrap(),
            b"# head\n# mid\nB=2\nA=9\n".to_vec()
        );
        // rm 删该键全部行（此前重复已清，仍验全删语义）＋注释保留。
        rm(&p, b"A").unwrap();
        rm(&p, b"B").unwrap();
        let image = std::fs::read(&p).unwrap();
        assert_eq!(
            read_root_file(&image, "BSHRC.CFG").unwrap(),
            b"# head\n# mid\n".to_vec()
        );
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn multi_cluster_old_file_read_and_collapse() {
        let _hg = lock_home_raii("multiclu");
        // 多簇旧件（--put 预置 >2048B 模拟客机 `>` 手写）：
        // ① 只读容忍——list/get 正常（≤8KiB/64 行同 rc_load 帽内）。
        let mut src = Vec::new();
        for i in 0..40 {
            src.extend_from_slice(format!("KEY{:02}={}\n", i, "v".repeat(105)).as_bytes());
        }
        assert!(src.len() > 2048, "测试旧件必须跨簇（{}B）", src.len());
        let d = mkfat_disk(&[("BSHRC.CFG", &src)]);
        let p = write_tmp(&d);
        assert_eq!(list(&p).unwrap().len(), 40);
        assert_eq!(get(&p, b"KEY07").unwrap().map(|v| v.len()), Some(105));
        std::fs::remove_file(&p).ok();
        // ② set 收拢单簇（可达面＝模型序列化入帽者——CRLF 遗留收编形：
        //    旧件字节 >2048（60×35=2100B 双簇）而剥 CR 后模型 2040B ≤2048；
        //    set 再缩 C00 值后一次落盘）。
        let mut crlf = Vec::new();
        for i in 0..60 {
            crlf.extend_from_slice(format!("C{:02}={}\r\n", i, "w".repeat(29)).as_bytes());
        }
        assert!(crlf.len() > 2048, "CRLF 旧件必须跨簇（{}B）", crlf.len());
        let model_len: usize = parse_lines(&crlf)
            .iter()
            .map(|l| serialize_lines(std::slice::from_ref(l)).len())
            .sum();
        assert!(model_len <= TOTAL_MAX && model_len > 2000, "剥 CR 模型恰入帽（{model_len}B）");
        let d2 = mkfat_disk(&[("BSHRC.CFG", &crlf)]);
        let p2 = write_tmp(&d2);
        let old_chain_len = {
            let image = std::fs::read(&p2).unwrap();
            let v = locate(&image).unwrap();
            let scan = scan_root(&image, &v, &bshrc_name11()).unwrap();
            let (_, first, _) = scan.hit.unwrap();
            collect_chain(&image, &v, first).unwrap().len()
        };
        assert!(old_chain_len > 1, "旧链必须多簇（{old_chain_len}）");
        set(&p2, &[(b"C00".to_vec(), b"9".to_vec())]).unwrap();
        // 期望字节手工构造（不经被测序列化器）：C00 缩值行 + 其余 59 行剥 CR。
        let mut expect = b"C00=9\n".to_vec();
        for i in 1..60 {
            expect.extend_from_slice(format!("C{:02}={}\n", i, "w".repeat(29)).as_bytes());
        }
        let image = std::fs::read(&p2).unwrap();
        assert_eq!(read_root_file(&image, "BSHRC.CFG").unwrap(), expect);
        // 新链恰单簇＋旧链全释放（守恒差分）。
        let v = locate(&image).unwrap();
        let scan = scan_root(&image, &v, &bshrc_name11()).unwrap();
        let (_, first, size) = scan.hit.unwrap();
        assert_eq!(collect_chain(&image, &v, first).unwrap().len(), 1);
        assert!(size as usize <= v.clus_bytes());
        assert_eq!(
            free_count(&image),
            free_count(&d2) - 1 + old_chain_len as u32
        );
        std::fs::remove_file(&p2).ok();
    }

    #[test]
    fn multi_cluster_lf_old_file_set_rejected_by_total_cap() {
        let _hg = lock_home_raii("multilf");
        // LF 多簇旧件（模型即超帽）：set 拒写指名总量帽（fail-closed——
        // rm 可先收编）；list/get 只读容忍照常；盘零触。
        let mut src = Vec::new();
        for i in 0..40 {
            src.extend_from_slice(format!("KEY{:02}={}\n", i, "v".repeat(105)).as_bytes());
        }
        let d = mkfat_disk(&[("BSHRC.CFG", &src)]);
        let p = write_tmp(&d);
        let e = set(&p, &[(b"NEW".to_vec(), b"v".to_vec())]).unwrap_err();
        assert_eq!(e.code, 2);
        assert!(e.message.contains("2048"), "{}", e.message);
        assert_eq!(std::fs::read(&p).unwrap(), d, "拒写＝盘零触");
        assert_eq!(list(&p).unwrap().len(), 40, "只读容忍照常");
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn disk_fidelity_three_faces_only() {
        let _hg = lock_home_raii("fidelity");
        // 盘保真（G2）：预置 SETTINGS.CFG＋LAYOUT.JSN，单次提交写前后全盘
        // diff 恰限定 BSHRC 三面（数据簇/FAT 表项/根目录槽）——非 BSHRC 域
        // 字节恒等执法。（多次提交的中间链会留「已释放但脏」的数据簇残迹
        // ——仍是 BSHRC 名下数据簇、FAT 已 Free 的正常 FAT 语义；三面字节
        // 级对账以单次提交为口径，后续操作走内容零漂移执法。）
        let settings = b"{\"v\":1,\"five-keys\":true}\n";
        let layout = b"[1,2,3]\n";
        let d = mkfat_disk(&[
            ("SETTINGS.CFG", settings),
            ("LAYOUT.JSN", layout),
        ]);
        let p = write_tmp(&d);
        set(&p, &[(b"PATH".to_vec(), b"/etc".to_vec()), (b"G".to_vec(), b"1".to_vec())])
            .unwrap();
        let after = std::fs::read(&p).unwrap();
        // 非三面字节恒等。
        let v = locate(&after).unwrap();
        let fat_faces: Vec<(usize, usize)> = (0..v.bpb.nfats as usize)
            .map(|i| (v.fat_off(i), v.fat_off(i) + v.fat_len()))
            .collect();
        let root_face = (v.root_off(), v.root_off() + v.bpb.root_secs as usize * 512);
        let bshrc_first = {
            let scan = scan_root(&after, &v, &bshrc_name11()).unwrap();
            scan.hit.map(|(_, f, _)| f).unwrap_or(0)
        };
        let cluster_face = if bshrc_first >= 2 {
            (v.clus_off(bshrc_first), v.clus_off(bshrc_first) + v.clus_bytes())
        } else {
            (usize::MAX, usize::MAX) // 空文件形无数据簇面
        };
        let in_face = |i: usize| {
            fat_faces.iter().any(|&(a, b)| i >= a && i < b)
                || (i >= root_face.0 && i < root_face.1)
                || (i >= cluster_face.0 && i < cluster_face.1)
        };
        let diffs: Vec<usize> = (0..d.len().min(after.len()))
            .filter(|&i| d[i] != after[i])
            .collect();
        assert!(!diffs.is_empty(), "须有真实写动");
        for i in &diffs {
            assert!(in_face(*i), "字节 {i} 漂出三面（fat={fat_faces:?} root={root_face:?} clus={cluster_face:?}）");
        }
        assert_eq!(d.len(), after.len(), "盘长恒等");
        // settings 五键链零漂移（后续操作族也零漂移）：crashlog 读面字节
        // 恒等对拍。
        assert_eq!(&read_root_file(&after, "SETTINGS.CFG").unwrap(), settings);
        assert_eq!(&read_root_file(&after, "LAYOUT.JSN").unwrap(), layout);
        set(&p, &[(b"G".to_vec(), b"22".to_vec())]).unwrap();
        rm(&p, b"G").unwrap();
        let after2 = std::fs::read(&p).unwrap();
        assert_eq!(&read_root_file(&after2, "SETTINGS.CFG").unwrap(), settings);
        assert_eq!(&read_root_file(&after2, "LAYOUT.JSN").unwrap(), layout);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn root_full_error_lists_names() {
        let _hg = lock_home_raii("rootfull");
        // 根 16 槽预满（卷标 + 15 文件）：新建 BSHRC 报错附在场清单。
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for i in 0..15 {
            files.push((format!("F{i:02}.DAT"), b"x".to_vec()));
        }
        let refs: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(n, d)| (n.as_str(), d.as_slice()))
            .collect();
        let d = mkfat_disk(&refs);
        let p = write_tmp(&d);
        let e = set(&p, &[(b"K".to_vec(), b"v".to_vec())]).unwrap_err();
        assert_eq!(e.code, 6);
        assert!(e.message.contains("16 槽满"), "{}", e.message);
        for i in 0..15 {
            assert!(e.message.contains(&format!("F{i:02}.DAT")), "{}", e.message);
        }
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn bare_image_without_volume_rejected() {
        let _hg = lock_home_raii("bare");
        // 裸件无卷：拒形报错（非 panic）。
        let p = write_tmp(b"not a fat volume, just bytes\n");
        let e = set(&p, &[(b"K".to_vec(), b"v".to_vec())]).unwrap_err();
        assert_eq!(e.code, 6);
        assert!(e.message.contains("找不到合法 FAT12 卷"), "{}", e.message);
        let e = list(&p).unwrap_err();
        assert!(e.message.contains("找不到合法 FAT12 卷"), "{}", e.message);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn hidden_mismatch_volume_rejected() {
        let _hg = lock_home_raii("hidden");
        // hidden 对账：mkfat.py --put EFI 形（hidden=0）被人为挪到 LBA16
        // （不改 hidden）＝非 mkfat 形拒（fsmgr 对账面）。
        let mut d = mkfat_disk(&[]);
        let voff = 16 * 512;
        d[voff + 28..voff + 32].copy_from_slice(&0u32.to_le_bytes()); // hidden 撕回 0
        let p = write_tmp(&d);
        let e = set(&p, &[(b"K".to_vec(), b"v".to_vec())]).unwrap_err();
        assert!(e.message.contains("找不到合法 FAT12 卷"), "{}", e.message);
        // 同盘裸 EFI 形（去掉前缀，vol@LBA0/hidden=0 自洽）反而可写。
        let bare = write_tmp(&d[16 * 512..]);
        set(&bare, &[(b"K".to_vec(), b"v".to_vec())]).unwrap();
        std::fs::remove_file(&p).ok();
        std::fs::remove_file(bare).ok();
    }

    // ── 字节恒等对拍（mkfat dev-dep——写方言≡造盘方言同源证）────────────

    #[test]
    fn byte_identity_with_mkfat_build_volume() {
        let _hg = lock_home_raii("ident");
        // 预置双件名序在 BSHRC.CFG 前（A1/A2 < BSHRC）——mkfat 按名序分簇，
        // 写者 first-fit 升序分配在「双件先占簇 2/3」的前缀盘上与造盘序
        // 重合（新键落簇 4、根槽 [3]）；整盘逐字节等值＝写方言≡造盘方言。
        let bshrc_body = b"PATH=/etc\nG=1\n".to_vec();
        let mut oracle_listing = vec![
            (b"A1.TXT".to_vec(), b"first file\n".to_vec()),
            (b"A2.TXT".to_vec(), b"second file\n".to_vec()),
        ];
        oracle_listing.push((b"BSHRC.CFG".to_vec(), bshrc_body.clone()));
        let (oracle_vol, _) = mkfat::build_volume(Some(&oracle_listing), &[]).unwrap();
        let mut oracle = vec![0u8; 16 * 512];
        oracle.extend_from_slice(&oracle_vol);
        // 写者路：双件前缀盘上写同内容。
        let base_listing = &oracle_listing[..2];
        let (base_vol, _) = mkfat::build_volume(Some(base_listing), &[]).unwrap();
        let mut base = vec![0u8; 16 * 512];
        base.extend_from_slice(&base_vol);
        let p = write_tmp(&base);
        set(&p, &[(b"PATH".to_vec(), b"/etc".to_vec()), (b"G".to_vec(), b"1".to_vec())])
            .unwrap();
        let after = std::fs::read(&p).unwrap();
        assert_eq!(after.len(), oracle.len(), "盘长须等");
        let diffs: Vec<usize> = (0..oracle.len()).filter(|&i| after[i] != oracle[i]).collect();
        assert!(
            diffs.is_empty(),
            "写方言≠造盘方言（漂移字节 {} 处，首个 @ {:?}）",
            diffs.len(),
            diffs.first()
        );
        std::fs::remove_file(&p).ok();
    }

    // ── 在用护栏（G7）与盘解析（C2）────────────────────────────────────

    #[test]
    fn live_session_default_disk_refused() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("berry-bshrc-guard-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(tmp.join("data")).unwrap();
        std::env::set_var("BERRY_HOME", &tmp);
        // 造一张真盘（走 mkfat——盘内容无关，只要合法卷）。
        let disk = tmp.join("data/desktop.img");
        let d = mkfat_disk(&[]);
        std::fs::write(&disk, &d).unwrap();
        // 伪活会话：meta 带 profile=desktop＋qemu_pid=本进程（探活必真）。
        let run = tmp.join("run").join("1770000000-1-device");
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(
            run.join("meta.json"),
            serde_json::json!({"v":1,"label":"device","pid":1u64,"profile":"desktop","qemu_pid":std::process::id() as u64}).to_string(),
        )
        .unwrap();
        run_dir::write_state(&run, "running");
        // 拒写：报文带 run id 与 stop 指路。
        let e = set(&disk, &[(b"K".to_vec(), b"v".to_vec())]).unwrap_err();
        assert_eq!(e.code, 6);
        assert!(e.message.contains("1770000000-1-device"), "{}", e.message);
        assert!(e.hint.unwrap_or_default().contains("device stop"), "{}", e.message);
        // get/list 不受护栏拦（只读面）。
        assert!(list(&disk).unwrap().is_empty());
        // 会话停态（state=stopped＋qemu_pid 死值）：放行。
        run_dir::write_state(&run, "stopped");
        std::fs::write(
            run.join("meta.json"),
            serde_json::json!({"v":1,"label":"device","pid":1u64,"profile":"desktop","qemu_pid":1u64}).to_string(),
        )
        .unwrap();
        set(&disk, &[(b"K".to_vec(), b"v".to_vec())]).unwrap();
        // 活 shot 无 profile：保守拒写。
        let shot = tmp.join("run").join("1770000001-2-shot");
        std::fs::create_dir_all(&shot).unwrap();
        std::fs::write(
            shot.join("meta.json"),
            serde_json::json!({"v":1,"label":"shot","pid":std::process::id() as u64}).to_string(),
        )
        .unwrap();
        run_dir::write_state(&shot, "running");
        let e = set(&disk, &[(b"K2".to_vec(), b"v".to_vec())]).unwrap_err();
        assert!(e.message.contains("shot"), "{}", e.message);
        // 活 run（起核不挂数据盘）：放行。
        std::fs::remove_dir_all(&shot).unwrap();
        let r = tmp.join("run").join("1770000002-3-run");
        std::fs::create_dir_all(&r).unwrap();
        std::fs::write(
            r.join("meta.json"),
            serde_json::json!({"v":1,"label":"run","pid":std::process::id() as u64}).to_string(),
        )
        .unwrap();
        run_dir::write_state(&r, "running");
        set(&disk, &[(b"K3".to_vec(), b"v".to_vec())]).unwrap();
        std::env::remove_var("BERRY_HOME");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn resolve_disk_three_tiers_write_context() {
        let _g = ENV_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!("berry-bshrc-resolve-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(tmp.join("data")).unwrap();
        // macOS /tmp 是 symlink（canonicalize 落 /private/tmp）——对拍走
        // canonicalize 同侧，不拿字面路径比。
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap();
        // ① --disk 显式最优先（与 env 同设时）。
        let d = mkfat_disk(&[]);
        let f = tmp.join("explicit.img");
        std::fs::write(&f, &d).unwrap();
        std::env::set_var("BERRY_DATA_DISK", "off");
        std::env::set_var("BERRY_HOME", &tmp);
        assert_eq!(resolve_disk(Some(&f), "desktop").unwrap(), canon(&f));
        // ② off 显式关：写语境报错说明（可行动报错）。
        let e = resolve_disk(None, "desktop").unwrap_err();
        assert_eq!(e.code, 3);
        assert!(e.message.contains("off"), "{}", e.message);
        // ③ env 显式路径直用（缺席报错指路）。
        std::env::set_var("BERRY_DATA_DISK", f.to_string_lossy().as_ref());
        assert_eq!(resolve_disk(None, "desktop").unwrap(), canon(&f));
        std::env::set_var("BERRY_DATA_DISK", tmp.join("nope.img").to_string_lossy().as_ref());
        let e = resolve_disk(None, "desktop").unwrap_err();
        assert!(e.message.contains("缺席"), "{}", e.message);
        // ④ 缺省位：存在即用 / 缺席附盘清单与造盘指路。
        std::env::remove_var("BERRY_DATA_DISK");
        let desk = tmp.join("data/desktop.img");
        std::fs::write(&desk, &d).unwrap();
        assert_eq!(resolve_disk(None, "desktop").unwrap(), canon(&desk));
        std::fs::remove_file(&desk).unwrap();
        std::fs::write(tmp.join("data/phone.img"), &d).unwrap();
        let e = resolve_disk(None, "desktop").unwrap_err();
        assert_eq!(e.code, 6);
        assert!(e.message.contains("phone.img"), "报错附盘清单：{}", e.message);
        assert!(e.hint.unwrap_or_default().contains("just desktop"), "{}", e.message);
        // ⑤ profile 取值校验沿单源。
        let e = resolve_disk(None, "nope-profile").unwrap_err();
        assert_eq!(e.code, 3);
        assert!(e.message.contains("unknown profile"), "{}", e.message);
        std::env::remove_var("BERRY_HOME");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn persist_keeps_mode_bits_and_cleans_temp() {
        let _hg = lock_home_raii("persist");
        // 独占子目录（并行测试的 temp 件互不掺入残留扫描）。
        let dir = std::env::temp_dir().join(format!(
            "berry-bshrc-persist-{}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let d = mkfat_disk(&[]);
        let p = dir.join("disk.img");
        std::fs::write(&p, &d).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o640)).unwrap();
        }
        set(&p, &[(b"K".to_vec(), b"v".to_vec())]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o640, "模式位保随");
        }
        // 独占目录内无残留 temp 件（含并发窗——本目录只有本测试写）。
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains(".bshrc-tmp-")
            })
            .collect();
        assert!(leftovers.is_empty(), "temp 件残留：{leftovers:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bshrc_present_probe() {
        let with = mkfat_disk(&[("BSHRC.CFG", b"K=v\n")]);
        let p1 = write_tmp(&with);
        assert!(bshrc_present(&p1));
        let without = mkfat_disk(&[]);
        let p2 = write_tmp(&without);
        assert!(!bshrc_present(&p2));
        assert!(!bshrc_present(Path::new("/nonexistent-berry-bshrc.img")));
        std::fs::remove_file(&p1).ok();
        std::fs::remove_file(&p2).ok();
    }
}
