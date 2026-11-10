//! berry-spawngeo —— spawn 几何前置门（A-2 自 berry-exec 抽取的纯函数面）。
//!
//! 语义源 = berry-exec `spawn_rootfs_elf` 的 process_create 前置段（迁出时
//! 逐字等价，勿漂移）——三段几何门，全部把「rootfs 输入面可达的坏值」从
//! **深处自陷**（vmar_map 才 Overlap/OutOfBounds 拒 → trap() 杀调用方 →
//! svcmgr 死 = watchdog 错杀 panic 连坐 halt）前置成 **Err 降级**：
//!
//! 1. **映幅帽** [`ELF_MAP_MAX`]（复盘雷 8）：`elf_head` 只挡 >2^39 野值，
//!    (16MiB, 2^39) 区间的合法巨段会走到 vmo_create 才 -ENOMEM → 自陷；
//!    span 属 rootfs **输入面**（非内核契约面），按解析侧同款契约降级。
//!    帽 = 内核 `VMO_MAX_SIZE`（16MiB）同值——真实 std 镜像 ~283KB 余量足。
//! 2. **栈顶前置**（对抗审查补，帽同族输入面）：映像顶 + 保护页 + 栈幅越
//!    `USER_TOP` 的 ELF（`elf_head` 只严格拒 vend > USER_TOP——贴顶映像
//!    本身合法可映）此前会走到 vmar_map 栈页才被 OutOfBounds 拒 → 自陷。
//!    `stack_pages == 0` 同拒（护全族调用方——manifest 域检 1..=64 之外的
//!    面，app-0 a-3 U3）；checked 链防巨值溢出绕检。
//! 3. **MMIO 窗口重叠前置**（对抗审查补，第三块几何约束）：[base,
//!    stack_end) 与 [mmio_va, +mmio_len) 相交即拒——PT_LOAD vaddr 落
//!    0x7000_0000 完全合法过既有界检，此前会走到 vmar_map 才 Overlap 拒
//!    → 自陷。窗口幅越界溢出（mmio_va+len 进位）同族保守拒。
//!
//! 纯函数纪律（berry-argv/berry-winplan 同款）：零 FFI 零 unsafe 零分配；
//! `base`/`len` 由调用方经 berry-loader 的 page_floor/page_ceil 算得后传入
//! （几何公式两侧单源在 loader，本 crate 不长第二套）。

// berry-virtio/berry-netproto 同款：非 test 构建无 std（workspace
// none-target 成员），host 测试构建放开 std（tests/ 独立 crate 本就带 std）。
#![cfg_attr(not(test), no_std)]

use berry_loader::{PAGE_SIZE, USER_TOP};

/// ELF 映幅帽 = 内核 `VMO_MAX_SIZE`（16MiB）同值（复盘雷 8 残留面；帽常量
/// 双侧同步纪律：改内核侧必同步此处）。
pub const ELF_MAP_MAX: u64 = 16 * 1024 * 1024;

/// [`crate::spawn_geometry`] 的拒绝原因（原 berry-exec 三段共用 `Err(())`
/// 不可分辨——外迁时枚举化供调用方/测试面区分，消费端行为零漂移：三类
/// 均走 Err 降级）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnGeoErr {
    /// 映像幅（page_ceil 后）超 [`ELF_MAP_MAX`]。
    SpanTooBig,
    /// stack_pages == 0，或映像顶+保护页+栈幅越 `USER_TOP` / checked 链溢出。
    StackOverflowTop,
    /// [base, stack_end) 与 [mmio_va, +mmio_len) 相交，或窗口端点溢出。
    MmioOverlap,
}

/// spawn 几何前置门（spawn_rootfs_elf 的 process_create 前置段，迁出时
/// 逐字等价）：三段任一失面即返对应原因；全过返栈带几何 [`SpawnGeoOk`]
/// （stack_end = 映像顶 + 保护页 + 栈幅——MMIO 重叠判定的实际上界，调用方
/// spawn 后续栈映射直接消费）。
///
/// - `base`/`len`：映像映射基/幅（page_floor/ceil 后值——几何公式单源在
///   berry-loader，此处只消费）；
/// - `stack_pages`：消费方参数（bsh 32/svcmgr 8，域检 1..=64 在 manifest
///   侧——本门护全族：0 与溢出同拒）；
/// - `mmio`：`Some((va, len))` = boot 授能窗口几何（berry-exec 侧经
///   vmo_get_size 取得——FFI 面留消费端，纯函数面参数化）；None = 无窗口
///   （第三段跳过）。
pub fn spawn_geometry(
    base: u64,
    len: u64,
    stack_pages: u64,
    mmio: Option<(u64, u64)>,
) -> Result<SpawnGeoOk, SpawnGeoErr> {
    if len > ELF_MAP_MAX {
        return Err(SpawnGeoErr::SpanTooBig);
    }
    // 原式逐字保真：checked_mul → (base+len+PAGE) checked_add（基址算术
    // 不设防面与 berry-exec 原值一致——elf_head 已把 base+len 帽在 USER_TOP
    // (2^39) 内，加法不溢出）；0 页/溢出/越顶三态同归 StackOverflowTop。
    let stack_end = match stack_pages
        .checked_mul(PAGE_SIZE)
        .and_then(|s| (base + len + PAGE_SIZE).checked_add(s))
    {
        Some(e) if stack_pages != 0 && e <= USER_TOP => e,
        _ => return Err(SpawnGeoErr::StackOverflowTop),
    };
    if let Some((mmio_va, mmio_len)) = mmio {
        let hit = match mmio_va.checked_add(mmio_len) {
            Some(me) => base < me && mmio_va < stack_end,
            None => true, // 窗口幅越界溢出：同族保守降级
        };
        if hit {
            return Err(SpawnGeoErr::MmioOverlap);
        }
    }
    Ok(SpawnGeoOk { stack_end })
}

/// 几何门全过产物：栈带末端（绝对 VA）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnGeoOk {
    /// 栈带末端 = base + len + 保护页 + 栈幅（调用方栈映射上界）。
    pub stack_end: u64,
}
