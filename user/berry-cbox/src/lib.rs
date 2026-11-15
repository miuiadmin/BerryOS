//! berry-cbox —— 黑匣子账本纯函数库（cbox 刀 2，`.agents/plans/cbox/design.md`）。
//!
//! 三面（零分配、零 unsafe、零依赖，输入只借用；`berry-appproto` 同体例）：
//! - **排液快照解码** [`drain_decode`]：内核 `cbox.rs` 12B 头 + 104B 记录的
//!   用户态镜像（偏移表与内核 const assert 同源；magic=0 哨兵 = 撕裂跳过，
//!   seq 占位保去重序）。
//! - **账本行编解码** [`line_encode`]/[`line_parse`]：`CBOX.LOG` 一行一记录
//!   的 ASCII 格式（冻结字段序，add-only——未知字段拒收不误读）。
//! - **合并轮转** [`merge`] + 跨写者续号 [`boot_pick`]：整文件读-改-写链的
//!   纯核——`(boot, seq)` 去重、超帽老行先行轮出（文件序 = 年龄序）。
//!
//! 消费：svcmgr（battery + 监督面）、berry-launcher（init 落账）、刀 3 宿主
//! `berry crash-log`（行解析）。「不长第二套」：行格式与合并语义单源在此。

#![cfg_attr(not(test), no_std)]
#![deny(clippy::missing_safety_doc)]

/// 记录 magic（"CBOX" 小端 = 0x58424F43；内核 `cbox.rs` 同源）。
pub const REC_MAGIC: u32 = 0x5842_4F43;
/// 记录格式版本（尾部追加字段 = ver 递增，禁中插）。
pub const REC_VER: u8 = 1;
/// 1 = EL0 sync（用户任务 fault）；2 = EL1 sync（内核 fault）。
pub const KIND_EL0: u8 = 1;
pub const KIND_EL1: u8 = 2;
/// 单记录定宽（内核 `CboxRec` repr(C) 钉板）。
pub const REC_SIZE: usize = 104;
/// 排液头定宽（`{ver u32, total u32, stored u32}`）。
pub const HDR_SIZE: usize = 12;
/// 环形槽容量（快照记录数上限）。
pub const SLOTS: usize = 8;
/// 排液缓冲帽 = 头 + 满表（syscall cap 直用）。
pub const DRAIN_MAX: usize = HDR_SIZE + SLOTS * REC_SIZE; // 844

/// 账本帽：2 簇（数据盘簇 = mkfat SPC=4 × 512B = 2048B）。安全性不靠单簇
/// 原子——PUT 全程 staging、`TRUNC` 才是 commit 点（中断 = 旧账原样保命，
/// C-1 裁决 2 两段式同款）；超帽老行先行轮出（环形语义，与内核 8 槽同构）。
pub const LEDGER_MAX: usize = 4096;
/// 行缓冲帽：全 16 位十六进制 ×8 + 20 位十进制 ×3 worst ≈ 277B + 换行。
pub const LINE_MAX: usize = 288;

/// 一条崩溃记录（用户态镜像；`kind == 0` = 撕裂哨兵——内容跳过、seq 占位）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rec {
    pub kind: u8,
    /// 全局单调写序（1 起；0 保留）。
    pub seq: u32,
    /// crash 时刻 CNTVCT 原始周期。
    pub vct: u64,
    /// 任务名 NUL 填充（`(boot)` = CURRENT 空态）。
    pub name: [u8; 16],
    pub pc: u64,
    pub far: u64,
    pub esr: u64,
    pub lr: u64,
    pub sp: u64,
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
}

impl Rec {
    pub const EMPTY: Rec = Rec {
        kind: 0,
        seq: 0,
        vct: 0,
        name: [0; 16],
        pc: 0,
        far: 0,
        esr: 0,
        lr: 0,
        sp: 0,
        x19: 0,
        x20: 0,
        x21: 0,
    };

    /// 逐字段相等（哨兵 kind=0 恒不等——账本里没有哨兵行）。
    fn eq_full(&self, o: &Rec) -> bool {
        self.kind != 0
            && self.kind == o.kind
            && self.seq == o.seq
            && self.vct == o.vct
            && self.name == o.name
            && self.pc == o.pc
            && self.far == o.far
            && self.esr == o.esr
            && self.lr == o.lr
            && self.sp == o.sp
            && self.x19 == o.x19
            && self.x20 == o.x20
            && self.x21 == o.x21
    }
}

/// 排液快照（`drain_decode` 产物；`recs()` 按 seq 升序含哨兵）。
pub struct Snapshot {
    pub ver: u32,
    /// 全局累计写入数（含已轮出——表头 total 锚定值）。
    pub total: u32,
    pub stored: u32,
    pub slots: [Rec; SLOTS],
}

impl Snapshot {
    pub const EMPTY: Snapshot = Snapshot {
        ver: REC_VER as u32,
        total: 0,
        stored: 0,
        slots: [Rec::EMPTY; SLOTS],
    };

    /// 存量记录切片（seq 升序；撕裂哨兵在内，`kind == 0` 跳内容）。
    pub fn recs(&self) -> &[Rec] {
        &self.slots[..self.stored as usize]
    }
}

fn rd32(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn rd64(b: &[u8], off: usize) -> Option<u64> {
    let s = b.get(off..off + 8)?;
    Some(u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
}

/// 排液快照解码。None = 头版本不符 / stored 越界 / 长度畸 / 野魔数（快照
/// 腐坏——调用方按失败收口，勿落盘）。magic=0 哨兵记录合法（kind 保持 0、
/// seq 占位——排液方按 seq 去重仍一致）。
pub fn drain_decode(buf: &[u8]) -> Option<Snapshot> {
    if buf.len() < HDR_SIZE {
        return None;
    }
    let ver = rd32(buf, 0)?;
    let total = rd32(buf, 4)?;
    let stored = rd32(buf, 8)?;
    if ver != REC_VER as u32 {
        return None;
    }
    if stored > SLOTS as u32 {
        return None;
    }
    if buf.len() < HDR_SIZE + stored as usize * REC_SIZE {
        return None;
    }
    let mut snap = Snapshot::EMPTY;
    snap.ver = ver;
    snap.total = total;
    snap.stored = stored;
    for i in 0..stored as usize {
        let b = &buf[HDR_SIZE + i * REC_SIZE..HDR_SIZE + (i + 1) * REC_SIZE];
        let mut r = Rec::EMPTY;
        r.seq = rd32(b, 8)?;
        match rd32(b, 0)? {
            REC_MAGIC => {
                if b[5] != REC_VER {
                    return None;
                }
                r.kind = b[4];
                r.vct = rd64(b, 16)?;
                r.name.copy_from_slice(b.get(24..40)?);
                r.pc = rd64(b, 40)?;
                r.far = rd64(b, 48)?;
                r.esr = rd64(b, 56)?;
                r.lr = rd64(b, 64)?;
                r.sp = rd64(b, 72)?;
                r.x19 = rd64(b, 80)?;
                r.x20 = rd64(b, 88)?;
                r.x21 = rd64(b, 96)?;
            }
            0 => {} // 哨兵：kind=0、seq 占位
            _ => return None,
        }
        snap.slots[i] = r;
    }
    Some(snap)
}

// —— 行编码（追加式小写器；满 = None 不截断）——

struct Wr<'a> {
    buf: &'a mut [u8],
    n: usize,
}

impl<'a> Wr<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Wr { buf, n: 0 }
    }
    fn put(&mut self, s: &[u8]) -> Option<()> {
        let e = self.n.checked_add(s.len())?;
        if e > self.buf.len() {
            return None;
        }
        self.buf[self.n..e].copy_from_slice(s);
        self.n = e;
        Some(())
    }
    fn digit(&mut self, v: u64) -> Option<()> {
        // 十进制：递归先写高位（栈深 ≤ 20）。
        if v >= 10 {
            self.digit(v / 10)?;
        }
        self.put(&[(b'0' + (v % 10) as u8)])
    }
    fn hex64(&mut self, v: u64) -> Option<()> {
        for i in (0..16).rev() {
            let d = ((v >> (i * 4)) & 0xF) as u8;
            let c = if d < 10 { b'0' + d } else { b'a' + d - 10 };
            self.put(&[c])?;
        }
        Some(())
    }
}

fn kind_word(kind: u8, out: &mut [u8]) -> Option<usize> {
    match kind {
        KIND_EL0 => {
            out[..3].copy_from_slice(b"el0");
            Some(3)
        }
        KIND_EL1 => {
            out[..3].copy_from_slice(b"el1");
            Some(3)
        }
        k => {
            out[0] = b'k';
            let mut w = Wr::new(&mut out[1..]);
            w.digit(k as u64)?;
            Some(1 + w.n)
        }
    }
}

/// 任务名可见形：NUL 前可见段，非打印字节替 `?`；空名防御回 `(boot)`
/// （内核写侧保证非空——此为编码侧兜底，保行恒可解析）。
fn name_visible(name: &[u8; 16]) -> [u8; 16] {
    let mut out = [0u8; 16];
    let mut n = 0;
    for &b in name.iter() {
        if b == 0 {
            break;
        }
        out[n] = if (0x20..0x7f).contains(&b) { b } else { b'?' };
        n += 1;
    }
    if n == 0 {
        out[..6].copy_from_slice(b"(boot)");
    }
    out
}

/// 账本行编码（设计稿冻结字段序）：`CBOX v1 seq=<u32> kind=<el0|el1|kN>
/// task=<名> pc=0x<16hex> far=… esr=… lr=… sp=… x19=… x20=… x21=…
/// vct=<u64 十进制> boot=<u32 十进制>\n`。`out.len() ≥ LINE_MAX` 保底；
/// 返回含 `\n` 长度；None = out 不足（调用方契约违约）。
pub fn line_encode(r: &Rec, boot: u32, out: &mut [u8]) -> Option<usize> {
    let mut w = Wr::new(out);
    w.put(b"CBOX v1 seq=")?;
    w.digit(r.seq as u64)?;
    w.put(b" kind=")?;
    let mut kw = [0u8; 5];
    let kl = kind_word(r.kind, &mut kw)?;
    w.put(&kw[..kl])?;
    w.put(b" task=")?;
    let nv = name_visible(&r.name);
    let mut nl = 0;
    while nl < 16 && nv[nl] != 0 {
        nl += 1;
    }
    w.put(&nv[..nl])?;
    let hexfields: [(&[u8], u64); 8] = [
        (b" pc=", r.pc),
        (b" far=", r.far),
        (b" esr=", r.esr),
        (b" lr=", r.lr),
        (b" sp=", r.sp),
        (b" x19=", r.x19),
        (b" x20=", r.x20),
        (b" x21=", r.x21),
    ];
    for (label, v) in hexfields {
        w.put(label)?;
        w.put(b"0x")?;
        w.hex64(v)?;
    }
    w.put(b" vct=")?;
    w.digit(r.vct)?;
    w.put(b" boot=")?;
    w.digit(boot as u64)?;
    w.put(b"\n")?;
    Some(w.n)
}

// —— 行解析（冻结字段序严格解析：缺字段 / 值越界 / 尾随野字节 = None
// ——add-only 纪律：新版本行让旧解析器失败，不误读）——

struct Rd<'a> {
    buf: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn lit(&mut self, s: &[u8]) -> Option<()> {
        if self.buf.get(self.p..self.p + s.len())? != s {
            return None;
        }
        self.p += s.len();
        Some(())
    }
    fn word(&mut self) -> Option<&'a [u8]> {
        let start = self.p;
        // 空格与换行皆界（尾随 `\n` 不吞进词——round-trip 直通形）。
        while self.p < self.buf.len()
            && self.buf[self.p] != b' '
            && self.buf[self.p] != b'\n'
        {
            self.p += 1;
        }
        Some(&self.buf[start..self.p])
    }
    fn dec(&mut self) -> Option<u64> {
        let w = self.word()?;
        if w.is_empty() || w.len() > 20 {
            return None;
        }
        let mut v: u64 = 0;
        for &c in w {
            if !c.is_ascii_digit() {
                return None;
            }
            v = v.checked_mul(10)?.checked_add((c - b'0') as u64)?;
        }
        Some(v)
    }
    fn hex(&mut self) -> Option<u64> {
        let w = self.word()?;
        if w.len() < 3 || w.len() > 18 || &w[..2] != b"0x" {
            return None;
        }
        let mut v: u64 = 0;
        for &c in &w[2..] {
            let d = match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                _ => return None,
            };
            v = v.checked_mul(16)?.checked_add(d as u64)?;
        }
        Some(v)
    }
    fn end(&mut self) -> Option<()> {
        match self.buf.get(self.p) {
            None => Some(()),
            // 单个尾随换行可收（round-trip 直通——行切调用方剥/不剥皆合法）。
            Some(b'\n') if self.p + 1 == self.buf.len() => Some(()),
            _ => None,
        }
    }
}

fn parse_kind(w: &[u8]) -> Option<u8> {
    match w {
        b"el0" => Some(KIND_EL0),
        b"el1" => Some(KIND_EL1),
        _ => {
            if w.len() >= 2 && w[0] == b'k' && w.len() <= 4 {
                let mut v: u64 = 0;
                for &c in &w[1..] {
                    if !c.is_ascii_digit() {
                        return None;
                    }
                    v = v.checked_mul(10)?.checked_add((c - b'0') as u64)?;
                }
                if v == 0 || v > 255 {
                    return None;
                }
                Some(v as u8)
            } else {
                None
            }
        }
    }
}

/// 账本行解析 → `(boot, Rec)`。严格字段序（见 [`line_encode`]）；行内不含
/// `\n`（调用方按行切好传入）。name 行内形须 1..=15 个可见字节（与 16B
/// NUL 填充字段互逆；超长 = 编不出也读不回，拒）。
pub fn line_parse(line: &[u8]) -> Option<(u32, Rec)> {
    let mut r = Rd { buf: line, p: 0 };
    r.lit(b"CBOX v1 seq=")?;
    let mut rec = Rec::EMPTY;
    let seq = r.dec()?;
    if seq > u32::MAX as u64 {
        return None;
    }
    rec.seq = seq as u32;
    r.lit(b" kind=")?;
    rec.kind = parse_kind(r.word()?)?;
    r.lit(b" task=")?;
    let name = r.word()?;
    if name.is_empty() || name.len() > 15 {
        return None;
    }
    for &b in name {
        if !(0x20..0x7f).contains(&b) {
            return None;
        }
    }
    rec.name[..name.len()].copy_from_slice(name);
    r.lit(b" pc=")?;
    rec.pc = r.hex()?;
    r.lit(b" far=")?;
    rec.far = r.hex()?;
    r.lit(b" esr=")?;
    rec.esr = r.hex()?;
    r.lit(b" lr=")?;
    rec.lr = r.hex()?;
    r.lit(b" sp=")?;
    rec.sp = r.hex()?;
    r.lit(b" x19=")?;
    rec.x19 = r.hex()?;
    r.lit(b" x20=")?;
    rec.x20 = r.hex()?;
    r.lit(b" x21=")?;
    rec.x21 = r.hex()?;
    r.lit(b" vct=")?;
    rec.vct = r.dec()?;
    r.lit(b" boot=")?;
    let boot = r.dec()?;
    if boot > u32::MAX as u64 {
        return None;
    }
    r.end()?;
    Some((boot as u32, rec))
}

// —— 合并轮转 + 跨写者续号 ——

/// 逐行回调（行不含 `\n`；尾部无换行残行忽略——本 crate 只产完整行）。
fn for_each_line(data: &[u8], mut f: impl FnMut(&[u8])) {
    let mut start = 0;
    for (i, &b) in data.iter().enumerate() {
        if b == b'\n' {
            f(&data[start..i]);
            start = i + 1;
        }
    }
}

/// `(boot, seq)` 行在册与否（去重键存在性扫描）。
fn ledger_contains(data: &[u8], boot: u32, seq: u32) -> bool {
    let mut hit = false;
    for_each_line(data, |line| {
        if let Some((b, r)) = line_parse(line) {
            if b == boot && r.seq == seq {
                hit = true;
            }
        }
    });
    hit
}

/// 账本扫描 → `(最大 boot id, 行数)`；空/全不可解析账本 = `(0, 行数)`。
pub fn ledger_scan(old: &[u8]) -> (u32, usize) {
    let mut max_boot = 0u32;
    let mut lines = 0usize;
    for_each_line(old, |line| {
        lines += 1;
        if let Some((b, _)) = line_parse(line) {
            if b > max_boot {
                max_boot = b;
            }
        }
    });
    (max_boot, lines)
}

/// 本 boot 的账本 boot id：账本最新 boot 组若与 RAM 快照逐字段全等命中
/// （同 seq 行内容一致——battery 已落过盘）→ 续用；否则 → 最新 + 1；空
/// 账本 → 1。已知边界（设计稿挂账）：battery 失败 + 上一 boot 崩溃集与
/// 本 boot 逐字节相同（含 vct——确定性虚拟机下理论可遇）→ 误续号，后果
/// = 两 boot 并组（指纹聚合少记一次 boot 维度，零数据丢失）。
pub fn boot_pick(old: &[u8], snap: &Snapshot) -> u32 {
    let (max_boot, _) = ledger_scan(old);
    if max_boot == 0 {
        return 1;
    }
    let mut hit = false;
    for_each_line(old, |line| {
        if let Some((b, r)) = line_parse(line) {
            if b == max_boot && snap.recs().iter().any(|s| s.eq_full(&r)) {
                hit = true;
            }
        }
    });
    if hit {
        max_boot
    } else {
        max_boot + 1
    }
}

/// [`merge`] 产物统计。
pub struct MergeStat {
    /// 结果长度（字节，含行尾 `\n`）。
    pub len: usize,
    /// 结果行数。
    pub lines: usize,
    /// 本次新增行数（去重后）。
    pub appended: usize,
    /// 轮出老行数。
    pub rotated: usize,
}

/// 合并轮转（纯核）：`old`（账本现容）+ 快照记录（`kind != 0`）→ `out`。
/// `(boot, seq)` 已在册的记录跳过（跨写者幂等——battery/监督/init 三路
/// 同 boot 互去重）；空间不足先轮出最老行（文件序 = 年龄序）再追加。
/// `out.len() ≥ LINE_MAX` 调用方契约；哨兵记录零输出（seq 只占位）。
pub fn merge(old: &[u8], snap: &Snapshot, boot: u32, out: &mut [u8]) -> MergeStat {
    debug_assert!(out.len() >= LINE_MAX);
    let cap = out.len();
    // 残行防御：只搬到最后一个 `\n` 为止的完整行前缀。
    let useful = match old.iter().rposition(|&b| b == b'\n') {
        Some(i) => &old[..=i],
        None => &old[..0],
    };
    out[..useful.len()].copy_from_slice(useful);
    let mut n = useful.len();
    let mut st = MergeStat { len: 0, lines: 0, appended: 0, rotated: 0 };
    for r in snap.recs() {
        if r.kind == 0 {
            continue;
        }
        if ledger_contains(&out[..n], boot, r.seq) {
            continue;
        }
        // 一行帽预留（编码可能短于帽——多轮一行属保守轮转，环形语义无害）。
        while cap - n < LINE_MAX {
            let Some(nl) = out[..n].iter().position(|&b| b == b'\n') else {
                n = 0;
                break;
            };
            out.copy_within(nl + 1..n, 0);
            n -= nl + 1;
            st.rotated += 1;
        }
        if let Some(l) = line_encode(r, boot, &mut out[n..]) {
            n += l;
            st.appended += 1;
        }
    }
    st.lines = out[..n].iter().filter(|&&b| b == b'\n').count();
    st.len = n;
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(seq: u32, kind: u8, name: &[u8]) -> Rec {
        let mut r = Rec::EMPTY;
        r.seq = seq;
        r.kind = kind;
        r.vct = 0x1234_5678_9abc_def0;
        r.name[..name.len()].copy_from_slice(name);
        r.pc = 0x0000_0000_0040_00a0;
        r.far = 0x0000_0000_00be_ef00;
        r.esr = 0x9200_0046;
        r.lr = 0x0000_0000_0040_0080;
        r.sp = 0x0000_0000_007f_f000;
        r.x19 = 11;
        r.x20 = 22;
        r.x21 = 33;
        r
    }

    fn enc_bytes(recs: &[Rec], total: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&(REC_VER as u32).to_le_bytes());
        v.extend_from_slice(&total.to_le_bytes());
        v.extend_from_slice(&(recs.len() as u32).to_le_bytes());
        for r in recs {
            v.extend_from_slice(&REC_MAGIC.to_le_bytes());
            v.push(r.kind);
            v.push(REC_VER);
            v.extend_from_slice(&0u16.to_le_bytes());
            v.extend_from_slice(&r.seq.to_le_bytes());
            v.extend_from_slice(&0u32.to_le_bytes()); // stamp
            v.extend_from_slice(&r.vct.to_le_bytes());
            v.extend_from_slice(&r.name);
            v.extend_from_slice(&r.pc.to_le_bytes());
            v.extend_from_slice(&r.far.to_le_bytes());
            v.extend_from_slice(&r.esr.to_le_bytes());
            v.extend_from_slice(&r.lr.to_le_bytes());
            v.extend_from_slice(&r.sp.to_le_bytes());
            v.extend_from_slice(&r.x19.to_le_bytes());
            v.extend_from_slice(&r.x20.to_le_bytes());
            v.extend_from_slice(&r.x21.to_le_bytes());
        }
        v
    }

    fn snap_of(recs: &[Rec], total: u32) -> Snapshot {
        let mut s = Snapshot::EMPTY;
        s.total = total;
        s.stored = recs.len() as u32;
        s.slots[..recs.len()].copy_from_slice(recs);
        s
    }

    #[test]
    fn decode_full_and_fields() {
        let recs = [rec(1, KIND_EL0, b"user_rogue_ptr"), rec(2, KIND_EL1, b"(boot)")];
        let buf = enc_bytes(&recs, 2);
        let s = drain_decode(&buf).unwrap();
        assert_eq!(s.total, 2);
        assert_eq!(s.recs().len(), 2);
        assert_eq!(s.recs()[0], recs[0]);
        assert_eq!(s.recs()[1], recs[1]);
    }

    #[test]
    fn decode_sentinel_and_rejects() {
        // 哨兵：magic=0、seq 占位。
        let mut buf = enc_bytes(&[rec(1, KIND_EL0, b"a")], 1);
        let magic_off = HDR_SIZE;
        buf[magic_off..magic_off + 4].fill(0);
        let s = drain_decode(&buf).unwrap();
        assert_eq!(s.recs()[0].kind, 0);
        assert_eq!(s.recs()[0].seq, 1);
        // 野魔数 = None。
        let mut bad = enc_bytes(&[rec(1, KIND_EL0, b"a")], 1);
        bad[magic_off] = 0x99;
        assert!(drain_decode(&bad).is_none());
        // 头版本 / stored 越界 / 截断。
        let mut v = enc_bytes(&[], 0);
        v[0] = 2;
        assert!(drain_decode(&v).is_none());
        let mut st = enc_bytes(&[], 0);
        st[8..12].copy_from_slice(&9u32.to_le_bytes());
        assert!(drain_decode(&st).is_none());
        assert!(drain_decode(&enc_bytes(&[rec(1, 1, b"a")], 1)[..100]).is_none());
        // 记录 ver 不符 = None。
        let mut rv = enc_bytes(&[rec(1, KIND_EL0, b"a")], 1);
        rv[HDR_SIZE + 5] = 9;
        assert!(drain_decode(&rv).is_none());
    }

    #[test]
    fn line_roundtrip_and_golden() {
        let r = rec(7, KIND_EL0, b"user_rogue_ptr");
        let mut out = [0u8; LINE_MAX];
        let n = line_encode(&r, 3, &mut out).unwrap();
        let gold = b"CBOX v1 seq=7 kind=el0 task=user_rogue_ptr pc=0x00000000004000a0 far=0x0000000000beef00 esr=0x0000000092000046 lr=0x0000000000400080 sp=0x00000000007ff000 x19=0x000000000000000b x20=0x0000000000000016 x21=0x0000000000000021 vct=1311768467463790320 boot=3\n";
        assert_eq!(&out[..n], gold);
        let (boot, back) = line_parse(&out[..n]).unwrap();
        assert_eq!(boot, 3);
        assert_eq!(back, r);
    }

    #[test]
    fn line_roundtrip_extremes() {
        // el1 + 最宽数值 + 短名 + 未知 kind k9。
        let mut r = Rec::EMPTY;
        r.seq = u32::MAX;
        r.kind = KIND_EL1;
        r.vct = u64::MAX;
        r.name[..1].copy_from_slice(b"x");
        r.pc = u64::MAX;
        r.far = u64::MAX;
        r.esr = u64::MAX;
        r.lr = u64::MAX;
        r.sp = u64::MAX;
        r.x19 = u64::MAX;
        r.x20 = u64::MAX;
        r.x21 = u64::MAX;
        let mut out = [0u8; LINE_MAX];
        let n = line_encode(&r, u32::MAX, &mut out).unwrap();
        let (boot, back) = line_parse(&out[..n]).unwrap();
        assert_eq!(boot, u32::MAX);
        assert_eq!(back, r);
        assert!(n < LINE_MAX);
        // k9 未知 kind 往返（防御面）。
        let mut k = rec(3, 9, b"zz");
        let n2 = line_encode(&k, 1, &mut out).unwrap();
        let (b2, back2) = line_parse(&out[..n2]).unwrap();
        assert_eq!(b2, 1);
        assert_eq!(back2, k);
        k.kind = 0; // 哨兵不可编码消费——但 encode 本身有定义（防御）
        let _ = line_encode(&k, 1, &mut out).unwrap();
    }

    #[test]
    fn line_parse_rejects() {
        let r = rec(7, KIND_EL0, b"user_rogue_ptr");
        let mut out = [0u8; LINE_MAX];
        let n = line_encode(&r, 3, &mut out).unwrap();
        let good = out[..n].to_vec();
        let gs = String::from_utf8(good.clone()).unwrap();
        // 字段序换位 / 未知尾随字段 / 截断 / 空名 / 超长名 / 非 0x 十六进制。
        let swapped = gs.replacen("pc=", "lr=", 1);
        assert!(line_parse(swapped.as_bytes()).is_none());
        let extra = format!("{}extra=1\n", gs.trim_end());
        assert!(line_parse(extra.as_bytes()).is_none());
        assert!(line_parse(&good[..good.len() - 5]).is_none());
        let noname = gs.replacen("task=user_rogue_ptr", "task=", 1);
        assert!(line_parse(noname.as_bytes()).is_none());
        let longname = gs.replacen("task=user_rogue_ptr", "task=abcdefghijklmnop", 1);
        assert!(line_parse(longname.as_bytes()).is_none());
        let nohex = gs.replacen("pc=0x", "pc=0y", 1);
        assert!(line_parse(nohex.as_bytes()).is_none());
    }

    #[test]
    fn merge_fresh_idempotent_append() {
        let snap = snap_of(&[rec(1, 1, b"a"), rec(2, 1, b"b"), rec(3, 2, b"c")], 3);
        let mut out = [0u8; LEDGER_MAX];
        let st = merge(b"", &snap, 1, &mut out);
        assert_eq!(st.appended, 3);
        assert_eq!(st.lines, 3);
        assert_eq!(st.rotated, 0);
        // 幂等：同快照再合 = 零新增。
        let first = out[..st.len].to_vec();
        let st2 = merge(&first, &snap, 1, &mut out);
        assert_eq!(st2.appended, 0);
        assert_eq!(&out[..st2.len], &first[..]);
        // 新 seq 追加在尾。
        let snap2 = snap_of(&[rec(4, 1, b"d")], 4);
        let st3 = merge(&first, &snap2, 1, &mut out);
        assert_eq!(st3.appended, 1);
        assert_eq!(st3.lines, 4);
        let s = String::from_utf8_lossy(&out[..st3.len]);
        assert!(s.lines().last().unwrap().starts_with("CBOX v1 seq=4 "));
    }

    #[test]
    fn merge_rotation_oldest_first() {
        // 帽内先铺 3 boot × 4 记录 = 12 行，再并 8 新记录 → 轮出到帽内。
        let mut old = Vec::new();
        let mut seq = 0u32;
        for boot in 1..=3u32 {
            for _ in 0..4 {
                seq += 1;
                let r = rec(seq, 1, b"old");
                let mut lb = [0u8; LINE_MAX];
                let n = line_encode(&r, boot, &mut lb).unwrap();
                old.extend_from_slice(&lb[..n]);
            }
        }
        assert!(old.len() <= LEDGER_MAX);
        let news: Vec<Rec> = (1..=8).map(|i| rec(100 + i, 1, b"new")).collect();
        let snap = snap_of(&news, 108);
        let mut out = [0u8; LEDGER_MAX];
        let st = merge(&old, &snap, 4, &mut out);
        assert_eq!(st.appended, 8);
        assert!(st.rotated > 0);
        assert!(st.len <= LEDGER_MAX);
        // 年龄序：幸存行 = 老行后段 + 全部新行；最老 boot=1 行已轮出（boot 是
        // 行尾字段——按解析值断言，非子串）。
        let text = String::from_utf8_lossy(&out[..st.len]).into_owned();
        let boots: Vec<u32> = text
            .lines()
            .filter_map(|l| l.rsplit(" boot=").next().and_then(|v| v.parse().ok()))
            .collect();
        assert!(!boots.contains(&1));
        assert!(boots.contains(&4));
        // 行序 = 先老后新（boot 不减）。
        let mut sorted = boots.clone();
        sorted.sort_unstable();
        assert_eq!(boots, sorted);
    }

    #[test]
    fn merge_sentinel_skipped_and_partial_tail_dropped() {
        let s = rec(1, 1, b"a");
        let mut sentinel = rec(2, 1, b"b");
        sentinel.kind = 0; // 哨兵
        let snap = snap_of(&[s, sentinel], 2);
        let mut out = [0u8; LEDGER_MAX];
        let st = merge(b"", &snap, 1, &mut out);
        assert_eq!(st.appended, 1);
        // 尾部残行（无换行）被丢弃：完整 seq=1 行保留去重，残行不复活。
        let mut lb = [0u8; LINE_MAX];
        let n1 = line_encode(&rec(1, 1, b"a"), 1, &mut lb).unwrap();
        let mut torn: Vec<u8> = lb[..n1].to_vec();
        torn.extend_from_slice(b"CBOX v1 seq=8"); // 无换行残尾
        let st2 = merge(&torn, &snap, 1, &mut out);
        let text = String::from_utf8_lossy(&out[..st2.len]);
        assert!(text.contains("seq=1 "));
        assert!(!text.contains("seq=8"));
        assert_eq!(st2.appended, 0); // seq=1 已在（完整行）——残尾未入册
    }

    #[test]
    fn boot_pick_rules() {
        let snap = snap_of(&[rec(5, 1, b"a")], 5);
        assert_eq!(boot_pick(b"", &snap), 1);
        // 最新 boot 组命中（同 seq 同内容）= 续用。
        let mut lb = [0u8; LINE_MAX];
        let n = line_encode(&rec(5, 1, b"a"), 2, &mut lb).unwrap();
        let ledger = lb[..n].to_vec();
        assert_eq!(boot_pick(&ledger, &snap), 2);
        // 未命中（内容异/seq 异）= +1。
        let mut other = rec(5, 1, b"a");
        other.pc += 1;
        let n2 = line_encode(&other, 2, &mut lb).unwrap();
        assert_eq!(boot_pick(&lb[..n2], &snap), 3);
        let n3 = line_encode(&rec(6, 1, b"a"), 2, &mut lb).unwrap();
        assert_eq!(boot_pick(&lb[..n3], &snap), 3);
        // 哨兵（kind=0）不参与命中。
        let sent = snap_of(&[Rec { kind: 0, seq: 5, ..Rec::EMPTY }], 5);
        assert_eq!(boot_pick(&ledger, &sent), 3);
        // scan：行数与最大 boot。
        let (mb, lines) = ledger_scan(&ledger);
        assert_eq!((mb, lines), (2, 1));
        assert_eq!(ledger_scan(b""), (0, 0));
    }
}
