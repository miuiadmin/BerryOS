//! sysver——版本身份 VMO（版本覆盖硬化·运行时查询面，内核侧）。
//!
//! 现役唯一版本身份原是串口 banner 一行字（进程无法编程查询——uname 等价物
//! 缺位）。本模块把版本身份烙进**只读 1 页 VMO**：结构体全部字段 = 编译期
//! env!/cfg! 常量（`build.rs` 的 BERRY_GIT_IDENT/BERRY_RUSTC_VER + Cargo
//! CARGO_PKG_VERSION + `abi_gen::BERRY_PLATFORM_LEVEL`），整页像在 const
//! 上下文成像——零运行时取数、boot 写入即拷贝。
//!
//! 内容布局（双路共享合同，逐字段冻结）：
//! ```text
//! offset  0  magic u32 = 0x4256_4552（"BVER" LE）
//! offset  4  hdr_ver u16 = 1
//! offset  6  rsv u16 = 0
//! offset  8  level u32（= BERRY_PLATFORM_LEVEL）
//! offset 12  semver maj/min/patch u16×3（= CARGO_PKG_VERSION 编译期解析）
//! offset 18  flags u16（bit0 = debug_assertions）
//! offset 20  起 NUL 结尾字符串两段连续：git ident → rustc 短版（总长 ≤128B）
//! ```
//! 交付：boot fb GOP 相位插 svcmgr 表**第 9 插 0x0001_0009**（与第 8 插
//! H_PRESSURE 同分支；rights READ——版本面禁写，权限在 handle 不在对象）。
//! svcmgr 对拍（fb 相）读前 128B 经 [`parse`] 同源口径解三元组。

use berry_mm::va::PAGE_SIZE;
use crate::abi_gen::BERRY_PLATFORM_LEVEL;
use crate::mm;
use crate::vmo::Vmo;

/// 头部定长（offset 20 起字符串段）。
const HDR_LEN: usize = 20;
/// 字符串段帽（双路合同：git ident + NUL + rustc 短版 + NUL 总长 ≤128B）。
const STRS_CAP: usize = 128;

/// magic "BVER" LE（消费者按 u32 LE 读 offset 0 比对）。
pub const SYSVER_MAGIC: u32 = 0x4256_4552;
/// 版本身份结构体版本（结构布局变更才 +1——加字段须向后兼容）。
pub const SYSVER_HDR_VER: u16 = 1;

/// git 构建标识（`git describe --tags --long` [+ "-dirty"]，build.rs 注入；
/// 无 git 降级 "nogit"）。
pub const GIT_IDENT: &str = env!("BERRY_GIT_IDENT");

/// rustc 短版（`rustc --version` 版本号段，build.rs 剥前后缀注入；缺席 "?"）。
pub const RUSTC_VER: &str = match option_env!("BERRY_RUSTC_VER") {
    Some(s) => s,
    None => "?",
};

/// flags 位（bit0 = debug_assertions；cfg! 编译期定值）。
const FLAGS: u16 = 0 | (cfg!(debug_assertions) as u16);

/// CARGO_PKG_VERSION（"maj.min.patch"）编译期解析为 u16 三元组。
const fn parse_semver(v: &str) -> (u16, u16, u16) {
    let b = v.as_bytes();
    let mut out = [0u16; 3];
    let mut idx = 0;
    let mut cur: u32 = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'.' {
            out[idx] = cur as u16;
            idx += 1;
            cur = 0;
        } else {
            cur = cur * 10 + (b[i] - b'0') as u32;
        }
        i += 1;
    }
    out[idx] = cur as u16;
    (out[0], out[1], out[2])
}

/// semver 三元组（= CARGO_PKG_VERSION；banner 同源常量）。
pub const SEMVER: (u16, u16, u16) = parse_semver(env!("CARGO_PKG_VERSION"));

/// 字符串段实长（ident + NUL + rustc + NUL）。
const STRS_LEN: usize = GIT_IDENT.len() + 1 + RUSTC_VER.len() + 1;
// 双路合同执法：字符串段超帽 = 编译失败（超长 ident 提前炸，不进运行时）。
const _: () = assert!(STRS_LEN <= STRS_CAP);
const _: () = assert!(HDR_LEN + STRS_LEN <= PAGE_SIZE);

const fn put_u16(img: &mut [u8; PAGE_SIZE], off: usize, v: u16) {
    let b = v.to_le_bytes();
    img[off] = b[0];
    img[off + 1] = b[1];
}

const fn put_u32(img: &mut [u8; PAGE_SIZE], off: usize, v: u32) {
    let b = v.to_le_bytes();
    img[off] = b[0];
    img[off + 1] = b[1];
    img[off + 2] = b[2];
    img[off + 3] = b[3];
}

const fn put_str(img: &mut [u8; PAGE_SIZE], off: usize, s: &str) {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        img[off + i] = b[i];
        i += 1;
    }
    img[off + b.len()] = 0; // NUL 结尾
}

/// 版本身份页像（编译期整页成像；boot/ktest 共用唯一数据源）。
const fn build_image() -> [u8; PAGE_SIZE] {
    let mut img = [0u8; PAGE_SIZE]; // rsv（offset 6）与页尾随零随零初始化
    put_u32(&mut img, 0, SYSVER_MAGIC);
    put_u16(&mut img, 4, SYSVER_HDR_VER);
    put_u32(&mut img, 8, BERRY_PLATFORM_LEVEL);
    put_u16(&mut img, 12, SEMVER.0);
    put_u16(&mut img, 14, SEMVER.1);
    put_u16(&mut img, 16, SEMVER.2);
    put_u16(&mut img, 18, FLAGS);
    put_str(&mut img, HDR_LEN, GIT_IDENT);
    put_str(&mut img, HDR_LEN + GIT_IDENT.len() + 1, RUSTC_VER);
    img
}

/// 版本身份页像（.rodata 常量——boot 建 VMO 只写 [`SYSVER_LEN`] 有效前缀）。
pub const SYSVER_IMAGE: [u8; PAGE_SIZE] = build_image();

/// 有效长（头 20B + 字符串段；对拍面读 VMO 前 128B 即全覆盖）。
pub const SYSVER_LEN: usize = HDR_LEN + STRS_LEN;

/// boot 建 version VMO：1 页 `create + commit + write` 三连（fbinfo VMO
/// 同款页模型零新变体）+ 写入编译期页像。调用方持 VMO 以 READ-only rights
/// 进表（「只读 dup 交付」——内核不留柄，权限在 handle 不在对象）。
pub fn build_vmo() -> Option<Vmo> {
    let mut vmo = Vmo::create(PAGE_SIZE).ok()?;
    vmo.commit(mm::frames(), 0, PAGE_SIZE).ok()?;
    vmo.write(0, &SYSVER_IMAGE[..SYSVER_LEN]).ok()?;
    Some(vmo)
}

/// 解析面：前缀缓冲（≥ [`SYSVER_LEN`] 有效长即够，对拍面惯读 128B）→
/// (git ident, rustc 短版, level, semver)。magic/hdr_ver/rsv/字符串段
/// 任一不符 → None（svcmgr 对拍与 ktest 同源口径——本函数单源）。
pub fn parse(buf: &[u8]) -> Option<(&str, &str, u32, (u16, u16, u16))> {
    if buf.len() < SYSVER_LEN {
        return None;
    }
    if u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) != SYSVER_MAGIC {
        return None;
    }
    if u16::from_le_bytes([buf[4], buf[5]]) != SYSVER_HDR_VER {
        return None;
    }
    if buf[6] != 0 || buf[7] != 0 {
        return None; // rsv 须零
    }
    let level = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let semver = (
        u16::from_le_bytes([buf[12], buf[13]]),
        u16::from_le_bytes([buf[14], buf[15]]),
        u16::from_le_bytes([buf[16], buf[17]]),
    );
    let ident_end = HDR_LEN + buf[HDR_LEN..].iter().position(|&b| b == 0)?;
    let ident = core::str::from_utf8(&buf[HDR_LEN..ident_end]).ok()?;
    let r_off = ident_end + 1;
    let r_end = r_off + buf[r_off..].iter().position(|&b| b == 0)?;
    let rustc = core::str::from_utf8(&buf[r_off..r_end]).ok()?;
    Some((ident, rustc, level, semver))
}
