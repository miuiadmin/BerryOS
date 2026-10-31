//! berry-kernel —— BerryOS 内核镜像。
//!
//! Phase 2a Step 6（里程碑）：两内核任务在 CNTV 定时器上**抢占式分时**。`on_irq_tick`
//! 经 IRQ 派发槽被调，做真 `tick`（减 slice_left）+ 归 0 则 `pick_next` + `switch`。
//! task_a/task_b 是**忙循环**（不 yield）——A/B 交替**纯由 timer slice 驱动**，即里程碑
//! 原文「两个内核任务在定时器上分时」。每 50 次切换打一行 `switches=N` 金丝雀（liveness）。
//! 异常向量已装（HAL `trap`）；panic handler 输出消息 + 位置（file:line:col）（G0 判据），
//! CPU 异常寄存器/ESR/FAR dump 在 HAL `trap::exception_sync`。
#![no_std]
#![no_main]

extern crate alloc;

// ABI 常量层（生成文件）：`tools/abi.toml` 单源——syscall/handle 经 pub use
// 转发消费；冻结位全量在册（未消费位 + 0x35 在册未实现号）故整体豁免 dead_code。
#[allow(dead_code)]
mod abi_gen;
mod aspace;
mod bench; // B-0：内核微基准 + churn 泄漏金丝雀（docs/基准与压力测试设计.md）
mod channel;
mod dtb;
mod event;
mod handle;
mod handoff; // V-0b 刀 3：BerryBoot 交接协议双镜像（const assert 执法，消费随刀 4 起）
mod heap;
mod ktest;
mod mem; // GUI-0：本仓 mem* 整数版覆写（内核 SIMD-free 合同 builtins 半边）
mod mm;
mod pi;
mod process;
mod syscall;
mod telemetry;
mod user;
mod vmar;
mod vmo;

use berry_mm::va::{phys_to_virt, PAGE_SIZE};
use heap::SlabHeap;

/// G-eco 入机侧：boot 期装载的 rootfs 原字节（fw_cfg / DTB 双源合一产物）。
/// 消费点随下一步「包只读 VMO 交 svcmgr bootstrap」；boot 单线程写、之后
/// 只读，不进并发路径。
static mut ROOTFS: Option<RootfsBuf> = None;

/// rootfs 源缓冲形态。handoff/dtb 路**零拷贝借片**——源已页钉 DRAM
/// （BerryBoot AllocatePages 的 EfiLoaderData / boot 保留区），EBS 后内核
/// 即 owner，整档 to_vec 是纯浪费；fw_cfg 路 fw_cfg 无 RAM 副本，必须整读
/// 进堆（冷启 RAM 完整，ROOTFS_MAX 帽内单块可达）。不统一 to_vec 的硬理由：
/// 内核堆大对象 = 单块连续 buddy 块（heap.rs 整页路径），21.3MiB rootfs 在
/// EFI 路的固件碎块 + blob 自占 20.4MiB 地形下结构性拿不到 order-13 对齐
/// 块（C-2c smoke-image 实踩 boot panic）；借片 + VMO 逐页 commit 两路皆
/// 碎片免疫。A1-3b 起第三形态 **Assembled**：fw_cfg 多卷路（40MB+ blob）
/// boot 期就逐 chunk 拼装进 VMO——堆大单块形态整体绕开（A1-1 地形墙的正解）。
enum RootfsBuf {
    Borrowed(&'static [u8]),
    Owned(alloc::vec::Vec<u8>),
    /// fw_cfg 多卷拼装产物（A1-3b）：装载终点 = 单一 VMO（Σ ≤ 128MiB，A1-2
    /// 帽内），消费面与另两源归一（cpio walk 视图不变，berry-exec/svcmgr
    /// 零改动）。逻辑字节长（Σ 卷 size）仅供打点——VMO 容量页对齐含尾零
    /// 页，cpio walk 靠 TRAILER!!! 自终止不依赖精确尾（G-eco-c 口径）。
    Assembled(vmo::Vmo, usize),
}

impl RootfsBuf {
    fn len(&self) -> usize {
        match self {
            RootfsBuf::Borrowed(b) => b.len(),
            RootfsBuf::Owned(v) => v.len(),
            RootfsBuf::Assembled(_, n) => *n,
        }
    }
}

/// rootfs 大小上限——与 `vmo_create` 的 32MiB 同口径（no-panic：巨 blob 的
/// 分配 abort 是 boot 期可触发 panic，超限降级为「无 rootfs」存活）。
/// A1-3b 起语义拆分（帽分家，容量工程前置-A1 §3.1）：本帽 = **单卷帽**
/// （fw_cfg 单具名文件/单暂存块面，mkcpio `--split 32` 产链契约）；
/// 多卷总量另见 [`ROOTFS_TOTAL_MAX`]。
const ROOTFS_MAX: usize = 32 << 20;

/// fw_cfg 多卷**总量帽**（A1-3b）——拼装终点单一 VMO，须 ≤ A1-2 的
/// `VMO_MAX_SIZE`（128MiB，syscall.rs）同口径；Σ 超帽 = 产链违约，拒收
/// 降级（boot 期无 panic 纪律）。
const ROOTFS_TOTAL_MAX: usize = 128 << 20;

/// fw_cfg 多卷逐 chunk 拼装的堆暂存块（A1-3b）——4MiB = order 10 单块，
/// 冷启/常态地形均稳定可得（C-2c 实证 13.8MiB order 12 冷启可达，本块更
/// 低一档）；**堆峰 = 单 chunk 非全 blob**（40MB+ = order 15 > MAX_ORDER 13
/// 的堆单块结构性不可得，A1-1 地形墙的装载侧执法）。
const ROOTFS_CHUNK: usize = 4 << 20;

/// 全局分配器：内核 slab 堆（小对象走 slab，大对象走整页）。
#[global_allocator]
static ALLOC: SlabHeap = SlabHeap::new();

/// 把 `core::fmt` 格式串直接打到 UART（无缓冲、逐字节）——panic 消息/位置用。
struct UartWriter;

impl core::fmt::Write for UartWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        berry_hal_aarch64::uart_puts(s.as_bytes());
        Ok(())
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    // G0 判据：panic 必须有人可读的 dump。消息 + file:line:col。
    // （CPU 异常的寄存器/PC/ESR/FAR dump 另走 HAL `trap::exception_sync`。）
    let _ = writeln!(UartWriter, "\nKERNEL PANIC");
    if let Some(loc) = info.location() {
        let _ = writeln!(UartWriter, "  at {}:{}:{}", loc.file(), loc.line(), loc.column());
    }
    // PanicMessage 实现了 Display（1.95 起 message() 不再返回 Option）。
    let _ = writeln!(UartWriter, "  {}", info.message());
    berry_hal_aarch64::halt();
}

/// 字节流 → 只读装载 VMO（handoff/dtb/fw_cfg 单卷三源共用，G-eco 第二刀
/// 原路径：create+commit+write 三连，页模型零新变体——物理不连续无碍，
/// vmo_read 按页走 direct map）。boot-only 直构面，失败 = expect（与既有
/// 同款：此阶段堆/帧已自证可用，失败属不可达病态）。
fn rootfs_bytes_to_vmo(bytes: &[u8]) -> vmo::Vmo {
    let mut vmo = vmo::Vmo::create(bytes.len()).expect("main: rootfs Vmo create");
    let committed = vmo.size(); // create 已页对齐
    vmo.commit(mm::frames(), 0, committed)
        .expect("main: rootfs Vmo commit");
    vmo.write(0, bytes).expect("main: rootfs Vmo write");
    vmo
}

/// fw_cfg 多卷装载（A1-3b，容量工程前置-A1 §3.2）——`opt/berry/rootfs0..9`
/// 连续前缀（首个缺席序号止；其后卷忽略——mkcpio `--split` 产卷恒连续，
/// 中断 = 产链形态，按前缀收）→ **VMO 逐页拼装**：逐卷 `SeqRead` 顺序续读
/// 4MiB chunk → `Vmo::write` 到递增 offset。堆峰 = 单 chunk（ROOTFS_CHUNK，
/// 地形免疫——A1-1 地形墙的装载侧执法）；VMO 逐页 commit 物理不连续无碍
/// （A1-2 帽 128MiB 内）。
///
/// 两帽执法（§3.1 帽分家）：单卷 ≤ [`ROOTFS_MAX`]（产链契约）、Σ ≤
/// [`ROOTFS_TOTAL_MAX`]。rootfs0 缺席 → `None`（调用方回落旧名单单卷整读，
/// [`ROOTFS_NAME`] 原路）；卷在位但帽越/空卷/拼装对账不符 → 也 `None`
/// （整源降级为「无 rootfs」存活，boot 期无 panic 纪律）。
fn fwcfg_rootfs_multivol() -> Option<RootfsBuf> {
    use berry_hal_aarch64::fwcfg;

    // 发现：连续前缀（rootfs0 缺席 = 非多卷形态，直接回落）。
    let mut vols: alloc::vec::Vec<fwcfg::FileEntry> = alloc::vec::Vec::new();
    let mut total: usize = 0;
    for i in 0..fwcfg::ROOTFS_VOLS {
        let Some(e) = fwcfg::rootfs_volume(i) else { break };
        let sz = e.size as usize;
        // 单卷帽（含空卷拒）：mkcpio --split 32 产链契约，越帽 = 违约拒收。
        if sz == 0 || sz > ROOTFS_MAX {
            return None;
        }
        // 总量帽 + 溢出守卫（Σ 超帽 = 递送面违约，与单卷帽同降级语义）。
        total = total.checked_add(sz)?;
        if total > ROOTFS_TOTAL_MAX {
            return None;
        }
        vols.push(e);
    }
    if vols.is_empty() {
        return None;
    }

    // 拼装：VMO 一次性建+commit（页模型与单卷路 create/commit/write 三连
    // 同形——commit 逐页出帧清零、write 逐 chunk 覆写；40MB+ 走本路才是
    // 设计意图，≤32MiB 单卷走旧名单路不进此处）。
    let mut vmo = vmo::Vmo::create(total).ok()?;
    vmo.commit(mm::frames(), 0, vmo.size()).ok()?;
    let mut chunk = alloc::vec![0u8; ROOTFS_CHUNK];
    let mut off: usize = 0;
    for vol in &vols {
        let mut rd = fwcfg::SeqRead::open(vol)?;
        loop {
            let n = rd.read(&mut chunk);
            if n == 0 {
                break;
            }
            vmo.write(off, &chunk[..n]).ok()?;
            off += n;
        }
    }
    // 对账：实拼字节 == Σ 卷 size（SeqRead 项尾守卫的调用侧执法——差值
    // 即拼装错位，整源降级不裸奔半截 blob）。
    if off != total {
        return None;
    }
    Some(RootfsBuf::Assembled(vmo, total))
}

/// 内核 Rust 入口（由 HAL boot asm 的 `br rust_main` 调用——2b-1 起高半入口）。
/// C-ABI 约定（ADR #18），符号名与 asm 精确匹配；首参 = 入口 x0（boot asm 的
/// x19 抢救）——**双语义**（V-1a 分流）：EFI 路 = BerryHandoff PA（BerryBoot
/// `br entry`，按 BRYH magic 认领，`handoff::probe`）；QEMU `-kernel` 路 =
/// DTB PA/0（G-eco 入机侧，`dtb::boot_probe`）。
#[no_mangle]
pub extern "C" fn rust_main(boot_x0: usize) -> ! {
    // 2b-1：注入 direct map 基址——必须先于任何 phys_to_virt 消费者（mm/heap）。
    berry_mm::va::set_kernel_va_base(berry_hal_aarch64::KERNEL_VA_BASE);
    berry_hal_aarch64::init();
    berry_hal_aarch64::uart_puts(b"BerryOS boot\n");

    // 2b-0：TTBR1 高半表自校验（RAM 魔数 + GIC PIDR2 双别名，见 HAL `mmu`）。
    // 不过 = 高半翻译未生效，2b-1 重链前必须在此暴露——panic 停机优于带病前进。
    if !berry_hal_aarch64::kva_ok() {
        panic!("KVA alias check failed: TTBR1 high-half not translating");
    }
    berry_hal_aarch64::uart_puts(b"KVA ok\n");

    // V-1a：入口首参分流——EFI 路命中即打标记（**EFI 路专属**，smoke 的
    // -kernel 五跑不见此行）；未中 → None，既有 DTB/fw_cfg 路零扰动。
    // 插点在 KVA ok 之后：probe 经高半直映读 handoff 页，别名已自证。
    let handoff = handoff::probe(boot_x0);
    // S-1：EFI 路标记（handoff 后续被 rootfs 装载消费 move 走，先钉存）——
    // smp_bringup 的 conduit 门之一（EFI 路 PSCI 归固件管，S-1 盒子跳过）。
    let efi_boot = handoff.is_some();
    // GUI-1：GOP 事实五域钉存（V-6a 只打点，GUI-1 起消费——fb VMO 授能块
    // 在 rootfs/virtio 插表段；handoff @下文 move 进 mm::init 前取走，
    // `efi_boot` 同款先例）。
    let mut gop_saved: Option<(u64, u32, u32, u32, u32)> = None;
    if let Some(h) = handoff {
        berry_hal_aarch64::uart_puts(b"HANDOFF ok ver=");
        berry_hal_aarch64::uart_put_dec(h.version as u64);
        berry_hal_aarch64::uart_puts(b"\n");
        // V-6a：GOP 携面消费（BerryBoot banner 期 QueryMode → handoff
        // @56..@76；fbcon 数据源先行到内核——打点即「事实到内核」证据，
        // MMIO 窗授能/fbcon 服务随后刀）。零值 = 无此物：QEMU `-kernel`
        // 路与无显示设备 EFI 路恒缺席，零扰动（EFI 路专属行，同 HANDOFF）。
        if let Some(g) = handoff::gop_info(h) {
            gop_saved = Some((g.fb_pa, g.width, g.height, g.stride, g.format));
            berry_hal_aarch64::uart_puts(b"FB ok ");
            berry_hal_aarch64::uart_put_dec(g.width as u64);
            berry_hal_aarch64::uart_puts(b"x");
            berry_hal_aarch64::uart_put_dec(g.height as u64);
            berry_hal_aarch64::uart_puts(b" fmt=");
            berry_hal_aarch64::uart_put_dec(g.format as u64);
            berry_hal_aarch64::uart_puts(b" stride=");
            berry_hal_aarch64::uart_put_dec(g.stride as u64);
            berry_hal_aarch64::uart_puts(b" fb=");
            berry_hal_aarch64::uart_put_hex64(g.fb_pa);
            berry_hal_aarch64::uart_puts(b"\n");
        }
    }

    // 2b-2：低半身份表退役——TTBR0 ← 内核空表 + `tlbi vmalle1is`。此后低半任何
    // 访问必然 fault（ktest ASPACE 用例正向断言）；UART/GIC MMIO 已走高半直映，
    // 内核对低半零依赖。回退开关见 HAL `mmu::RETIRE_IDENTITY`。
    berry_hal_aarch64::retire_low_half();

    // SAFETY: 启动单线程、`rust_main` 入口仅调用一次。V-1b 起 handoff 透传：
    // EFI 路按 memmap reserve（打 MMAP ok 标记），QEMU 路/坏图降级仅保
    // [RAM_BASE, kernel_end)——两侧同为合法 buddy 序（守恒面见 buddy 测试）。
    unsafe { mm::init(handoff) };

    // G-eco 入机侧第一刀：rootfs（initrd 角色）装载，三源按序——
    // ①（V-1c/A1-3b）handoff init：BerryBoot 从盘读 INITRD.BIN 页钉后经
    //    handoff 递送（fw_cfg 的 EFI 路替代者——EFI 固件面无 fw_cfg 具名
    //    文件机制）。**按 version 分派**（A1-3b §3.2.1）：v1 = 分段描述符
    //    数组（ BerryBoot 8MiB 目标段逐段 AllocatePages——40MB+ blob 的
    //    递送形态，绕开固件大连续段地形墙）：单段零拷贝借片、多段 VMO
    //    逐页拼装（源切片直传，堆零暂存）；v0 = init_blob_*（@80/@88）
    //    单 blob 原路（旧装载器照跑）。零段/坏值降级不死（boot 期无
    //    panic 纪律）；
    // ② 入口 x0 的 DTB `/chosen` initrd 物理区间 → 高半直映拷出（真机/
    //    Linux boot 协议路径；QEMU 非 Linux ELF 直加载实测 x0=0、RAM 无
    //    DTB、`-initrd` 整体不加载——initrd/dtb/cmdline 机制全在 QEMU
    //    hw/arm/boot.c 的 is_linux 分支，2026-10 实证）；**V-1a 起 handoff
    //    命中时本源按缺席处理**（x0 是 handoff PA 非 DTB）；
    // ③ QEMU fw_cfg 具名文件（目录自描述名字+大小、无魔法地址）。A1-3b 起
    //    双形态：**多卷优先**（`opt/berry/rootfs0..N` 连续前缀，mkcpio
    //    `--split` 产链对口——40MB+ blob 的递送形态）→ boot 期 VMO 逐页
    //    拼装（堆峰 = 单 chunk）；rootfs0 缺席回落**旧名单**
    //    `opt/berry/rootfs` 单卷整读（原路零漂移；两名并在 = rootfs0 优先，
    //    多卷是权威形态旧名忽略）。
    // 装进 boot 期缓冲，下一步包只读 VMO 交 svcmgr（cpio/ELF 解析全在
    // 用户态——微内核只补「字节流变对象」原语）。无 rootfs = 正常降级
    // （不带 -fw_cfg 的既有 run/smoke 零扰动）。
    let rootfs: Option<(&[u8], RootfsBuf)> = handoff
        .and_then(|h| {
            if h.version >= 1 {
                // v1：分段数组是唯一 init 事实源（init_blob_* 置零不读）。
                // 结构校验（同页自相对/对齐/界检）在 `handoff::init_segs`；
                // 空段（无 INITRD/违约）→ None 回落下一源。失败路打
                // `rootfs: <成因>` 诊断行（只此几处、恒在 `ROOTFS absent`
                // 前一行——机器可读缺席报告，canon 成功路零输出）。
                let segs: &[handoff::InitSeg] = match handoff::init_segs(h, boot_x0) {
                    Some(s) => s,
                    None => {
                        berry_hal_aarch64::uart_puts(b"rootfs: isegs=rej\n");
                        &[]
                    }
                };
                if segs.is_empty() {
                    berry_hal_aarch64::uart_puts(b"rootfs: isegs=0\n");
                    return None;
                }
                // Σ 总量帽（与 fw_cfg 多卷路同口径——A1-1 帽分家执法点）。
                let mut total: usize = 0;
                for s in segs {
                    total = total.checked_add(s.len as usize)?;
                    if total > ROOTFS_TOTAL_MAX {
                        berry_hal_aarch64::uart_puts(b"rootfs: isegs=cap\n");
                        return None;
                    }
                }
                if segs.len() == 1 {
                    // 单段零拷贝借片：与 v0 同纪律（源已在 DRAM，EfiLoaderData
                    // 保留区即归宿——RootfsBuf 头注）。
                    let (pa, len) = (segs[0].pa as usize, segs[0].len as usize);
                    // SAFETY: init_segs 已界检整段在 [RAM_BASE, RAM_TOP)——
                    // 高半直映 L2 全覆盖，from_raw_parts 必落已映射块。
                    let p = phys_to_virt(pa) as *const u8;
                    let bytes: &'static [u8] =
                        unsafe { core::slice::from_raw_parts(p, len) };
                    Some((b"handoff" as &[u8], RootfsBuf::Borrowed(bytes)))
                } else {
                    // 多段 VMO 逐页拼装（A1-3b 终段：**逐段增量
                    // commit→copy→即回收**）：源切片直传 `Vmo::write`（源
                    // 已在 DRAM 高半别名——堆零暂存，比 fw_cfg 路 4MiB
                    // chunk 更低调堆峰）；页模型与 fw_cfg 多卷路同形（commit
                    // 逐页出帧、物理不连续无碍）。commit 不整刀 upfront——
                    // 一次性 `commit(0, total)` 令段页 + VMO 页**瞬态双份**
                    // （44.4MiB blob 首验实踩：MMAP 保留 21861 页 → 空闲
                    // ~10907 < commit 需求 11367 → NoFrames → ROOTFS absent；
                    // 次段「双份 80MiB/128MiB 结构性不可行」的瞬态版，常驻
                    // 版已被段页回收治、瞬态版在此治）；逐段化后峰值 = 已
                    // 提交 VMO + 当前段宽，已拷段页即时回 buddy 供后续段
                    // commit 复用。前置条件：BerryBoot 分段器 8MiB 折半 +
                    // AllocatePages 页粒度 → 内段恒页倍数（末段任意——commit
                    // 页取整覆盖，VMO 容量 = ceil(total/4096) 恰容纳；内段
                    // 若违约非页倍 → commit -Unaligned loud absent，非静默）。
                    let mut vmo = match vmo::Vmo::create(total) {
                        Ok(v) => v,
                        Err(_) => {
                            berry_hal_aarch64::uart_puts(b"rootfs: vmo=create\n");
                            return None;
                        }
                    };
                    let mut off: usize = 0;
                    for s in segs {
                        let len = s.len as usize;
                        let plen = len.div_ceil(PAGE_SIZE) * PAGE_SIZE;
                        if vmo.commit(mm::frames(), off, plen).is_err() {
                            berry_hal_aarch64::uart_puts(b"rootfs: vmo=commit\n");
                            return None;
                        }
                        // SAFETY: 同上——init_segs 已逐段界检。
                        let src = unsafe {
                            core::slice::from_raw_parts(
                                phys_to_virt(s.pa as usize) as *const u8,
                                len,
                            )
                        };
                        if vmo.write(off, src).is_err() {
                            berry_hal_aarch64::uart_puts(b"rootfs: vmo=write\n");
                            return None;
                        }
                        // 段页即时回收（A1-3b）：本段内容已全量进 VMO——
                        // EfiLoaderData 段页按段区间归还 buddy（AllocatePages
                        // 页粒度 == ceil(len/4096)，回收恰无漏无溢）。单段
                        // Borrowed 不回收（段即活 rootfs 本体）；fw_cfg 路
                        // chunk 缓冲逐卷复用无此面。
                        mm::frames().release(s.pa as usize, len);
                        off += len;
                    }
                    // 对账（拼装完整性——同 fw_cfg 路纪律）。
                    if off != total {
                        berry_hal_aarch64::uart_puts(b"rootfs: vmo=off\n");
                        return None;
                    }
                    Some((b"handoff" as &[u8], RootfsBuf::Assembled(vmo, total)))
                }
            } else {
                // v0：init_blob_* 单 blob 原路（旧装载器兼容面）。
                let (pa, len) = (h.init_blob_pa, h.init_blob_len);
                if pa == 0 || len == 0 || len > ROOTFS_MAX as u64 {
                    return None;
                }
                if pa < berry_hal_aarch64::RAM_BASE as u64
                    || pa.checked_add(len)? > berry_hal_aarch64::RAM_TOP as u64
                {
                    return None;
                }
                // SAFETY: 界检已过（整段在 DRAM 窗内）——高半直映 L2 全覆盖，
                // from_raw_parts 读取必落在已映射块内（与 dtb 路同款论证）。
                let p = phys_to_virt(pa as usize) as *const u8;
                // 'static：DRAM 直映视图，寿命 = 整机；blob 留 EfiLoaderData
                // 保留区即其归宿（零拷贝直用——RootfsBuf 头注）。
                let bytes: &'static [u8] =
                    unsafe { core::slice::from_raw_parts(p, len as usize) };
                Some((b"handoff" as &[u8], RootfsBuf::Borrowed(bytes)))
            }
        })
        .or_else(|| {
            if handoff.is_none() {
                dtb::boot_probe(boot_x0)
                    .map(|(s, e)| {
                        // SAFETY: boot_probe 已界检区间在 [RAM_BASE, RAM_TOP)——高半
                        // 直映 L2 全覆盖，from_raw_parts 读取必落在已映射块内。
                        let pa = phys_to_virt(s as usize) as *const u8;
                        // 'static 同 handoff 路：boot 保留区直映视图，零拷贝。
                        let bytes: &'static [u8] =
                            unsafe { core::slice::from_raw_parts(pa, (e - s) as usize) };
                        (b"dtb" as &[u8], RootfsBuf::Borrowed(bytes))
                    })
            } else {
                None // EFI 路：x0 非 DTB——dtb 源缺席（fw_cfg 链照常兜底）
            }
        })
    .or_else(|| {
            use berry_hal_aarch64::fwcfg;
            // A1-3b：多卷优先——rootfs0 在位即走拼装路（发现/帽检/拼装全在
            // `fwcfg_rootfs_multivol`，bytes= 打点走 RootfsBuf::len 同源）。
            if let Some(buf) = fwcfg_rootfs_multivol() {
                return Some((b"fwcfg" as &[u8], buf));
            }
            let entry = fwcfg::find_file(fwcfg::ROOTFS_NAME)?;
            let size = entry.size as usize;
            // no-panic：与 vmo_create 32MiB 同口径——巨 blob 分配 abort 是
            // boot 期可触发 panic，超限/空文件一律降级为「无 rootfs」存活。
            if size == 0 || size > ROOTFS_MAX {
                return None;
            }
            let mut buf = alloc::vec![0u8; size];
            if !fwcfg::read_to(&entry, &mut buf) {
                return None;
            }
            Some((b"fwcfg" as &[u8], RootfsBuf::Owned(buf)))
        });
    match rootfs {
        Some((src, buf)) => {
            let bytes = buf.len();
            // SAFETY: boot 单线程写入、此后只读；消费点随下一步「包只读
            // VMO 交 svcmgr」——不进任何并发路径。
            unsafe { *(&raw mut ROOTFS) = Some(buf) };
            berry_hal_aarch64::uart_puts(b"ROOTFS ok src=");
            berry_hal_aarch64::uart_puts(src);
            berry_hal_aarch64::uart_puts(b" bytes=");
            berry_hal_aarch64::uart_put_dec(bytes as u64);
            berry_hal_aarch64::uart_puts(b"\n");
        }
        None => berry_hal_aarch64::uart_puts(b"ROOTFS absent\n"),
    }

    // 3-1：注册真 syscall 语义 handler（handle 三操作 + clock_read）进 HAL 表。
    // 必须先于 ktest::run（SYSCALL 用例）与一切用户进程。
    syscall::install();

    // S-2a E0（SMP-S2实施设计草案 §2.1/§2.3）：锁槽对安装——①arch SVC/fault
    // 入口的当前任务位 置/清器（sched 侧 klock_flag_current）；②sched 五切走点
    // 舞蹈的锁原语（arch kern_lock 四函数）。ktest 前装好 → ktest 期 SVC 也走
    // 锁路径（单核非竞争纯开销、恒配平）；CURRENT 空窗位跳过。gate（副核参与）
    // 不置位 = S-2a 单核可证，改动全量单核零漂移。
    berry_hal_aarch64::kern_lock::set_klock_flag_hook(berry_sched::klock_flag_current);
    berry_sched::set_klock_ops(berry_sched::KlockOps {
        kern_acquire: berry_hal_aarch64::kern_lock::kern_acquire,
        kern_release: berry_hal_aarch64::kern_lock::kern_release,
        print_lock: berry_hal_aarch64::kern_lock::print_lock,
        print_unlock: berry_hal_aarch64::kern_lock::print_unlock,
    });

    ktest::run();

    // B-0（基准与压力测试设计 §4）：内核微基准 + churn 泄漏金丝雀——ktest 后、
    // timer/sched init 前的干净窗口（单线程 IRQ 屏蔽零调度噪声）。每次 boot
    // 都跑：金丝雀属「主干永远绿」红线，BENCH 行供 smoke/bench.py 收割。
    bench::run();

    // Phase 2a Step 5：IRQ 派发钩子 + timer 管线（规律 tick，**不从 IRQ 调 switch**）。
    // SAFETY: EL1、启动单线程、仅一次：缓存 CNTFRQ/算 SLICE（不武装）。
    unsafe { berry_hal_aarch64::timer::init() };

    // 2b-2：两忙循环任务各挂一个私有地址空间——timer 抢占切任务 = 切 TTBR0/ASID
    // （switch 路径真 churn），任务体每次打印节流点自证私有页内容（隔离金丝雀）。
    // `Box::leak`：AS 须与任务同寿（v1 任务不退出）。
    let as_a: &'static aspace::MiniAspace = alloc::boxed::Box::leak(alloc::boxed::Box::new(
        aspace::MiniAspace::new(mm::frames(), aspace::alloc_asid(), TASK_A_MAGIC),
    ));
    let as_b: &'static aspace::MiniAspace = alloc::boxed::Box::leak(alloc::boxed::Box::new(
        aspace::MiniAspace::new(mm::frames(), aspace::alloc_asid(), TASK_B_MAGIC),
    ));

    // S-2 前置件 2（S-2b 前移至 sched::init 前）：主核自钉 per-CPU 块
    //（TPIDR_EL1 ← SMP_CPU0）——LAST_VCT per-CPU 记账（A11）+ CURRENT per-CPU
    // 主位（S-2b）的本核基座。**必须在 sched::init 之前**：init 的
    // set_current(IDLE_TASK) 经 current_slot() 落 SmpCpu0.current 主位——
    // TPIDR 后装则先写的 static 回落位被零初始化主位覆盖（current_task()
    // 读主位恒 null）。ktest/bench 期（slot 未装）回落 static null = 现状
    // 语义逐位对齐；arm_slice/enable_irq（首 tick）前、单核机型同走（per-CPU
    // 化不分核数）。副核同款在 secondary_entry asm ⑩。
    berry_hal_aarch64::smp::cpu0_init();

    // IRQ 此刻仍屏蔽（boot DAIF）；spawn 要求屏蔽态（alloc/enqueue 不可打断）。
    berry_sched::init();
    // 3-4c 遥测环：注册切换钩子（init 后、首个切换前——rust_main 尾部的
    // yield_now 即产生首条记录；watchdog 断言环在涨 + vct 单调）。
    telemetry::init();
    // 4-3b PI：注册 SLEEPQ 变更钩子（block 睡下后 / wake 摘队后全图重推；
    // 与 telemetry 同位——均须在首个切换前）。
    pi::init();
    berry_sched::spawn_as(task_a, TASK_A_MAGIC as usize, b"task_a", as_a.ttbr0_pa, as_a.asid);
    berry_sched::spawn_as(task_b, TASK_B_MAGIC as usize, b"task_b", as_b.ttbr0_pa, as_b.asid);

    // 2b-4（§5.1 第 5 步收官）：首批 EL0 用户进程 + G1.5 隔离关卡。good 打 'u'
    // （syscall 往返在 EL0 生效的活证）；rogue_ptr 野指针自作死 → 只死自己
    // （user_fault_exit）；abuser 坏 handle 滥用 → **-EBADF 存活**（3-1 撤
    // 「畸形号杀」政策：syscall 层错误一律错误码自理）。watchdog 断言后打关卡标记。
    let mut up_good =
        user::UserProcess::build(mm::frames(), user::user_good_code(), user::USER_MAGIC);
    let up_rogue_ptr =
        user::UserProcess::build(mm::frames(), user::user_rogue_ptr_code(), user::USER_MAGIC);
    let up_abuser =
        user::UserProcess::build(mm::frames(), user::user_abuser_code(), user::USER_MAGIC);
    let up_spawner =
        user::UserProcess::build(mm::frames(), user::user_spawner_code(), user::USER_MAGIC);

    // 3-4a：per-process 表拆分——每个 boot 手搭用户进程一张 leak 常驻表
    //（任务 tag 指向它，EL0 syscall 派发到自己的表；v1 无进程退出，表同寿）。
    use handle::{HandleTable, KernelObj, RIGHT_DUPLICATE, RIGHT_MANAGE, RIGHT_MAP};
    let ht_good: &'static mut HandleTable = alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let ht_rogue: &'static mut HandleTable = alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let ht_abuser: &'static mut HandleTable = alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let ht_spawner: &'static mut HandleTable = alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let (tag_good, tag_rogue, tag_abuser, tag_spawner) = (
        ht_good as *mut HandleTable as usize,
        ht_rogue as *mut HandleTable as usize,
        ht_abuser as *mut HandleTable as usize,
        ht_spawner as *mut HandleTable as usize,
    );

    // 4-5a：console 服务通道——**首个服务循环**（v1 留内核任务形态：用户态无
    // MMIO 映射，输出须经内核代劳，ADR「启动关键暂留内核」；4-5b svcmgr 落地
    // 后用户服务的通道面/循环形状与此同构）。**console 挂 leak 私有表**
    // （spawn_with_tag——4-4 随附「4-5 前置补丁」方向：常驻内核服务不睡共享
    // BOOTSTRAP_HT，PI 受捐判定只看自己的表）+ 稳态等待走 **wait_many
    // （WAIT_ANY）**——PI 双保险（私有表断错捐喷洒、哨兵断 donor 边，详见
    // spawn 处说明，勿改回 wait_one(endK)）。endU（发端 ObjRef）以 insert_ref
    // 落各用户表（RIGHT_SEND）——**用户打印全部改走真 IPC**（HAL 调试槽
    // 0xFE/0xFF 退役）。通道 opt-out（4-3c 五件套之 4）：固定优先级系统服务
    // 不借入调用方 intent。发送侧 -EBUSY/-EAGAIN 重试（用户模板，EL0 可抢占
    // 有界）、服务侧 wait READABLE → drain 循环。
    let ct: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let h_end_k = ct
        .insert(
            KernelObj::Channel(
                crate::channel::ChannelEnd::try_unpaired(true).expect("main: 建 console 收端"),
            ),
            handle::RIGHT_RECV,
        )
        .expect("main: console 收端进私有表");
    let h_end_u = syscall::bootstrap_table()
        .insert(
            KernelObj::Channel(
                crate::channel::ChannelEnd::try_unpaired(true).expect("main: 建 console 发端"),
            ),
            handle::RIGHT_SEND,
        )
        .expect("main: console 发端进内核侧表");
    let (pk, pu) = (
        ct.get_raw(h_end_k, handle::ObjType::Channel, 0)
            .expect("main: 取 console 收端 ObjRef")
            .0,
        syscall::bootstrap_table()
            .get_raw(h_end_u, handle::ObjType::Channel, 0)
            .expect("main: 取 console 发端 ObjRef")
            .0,
    );
    // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
    unsafe { crate::channel::ChannelEnd::pair(pk, pu) };
    // 先于用户矩阵入队（RR 序早 → 早起服务）；消息纯字节不携带 handle。
    //
    // **稳态等待走 wait_many（WAIT_ANY）而非 wait_one(endK)——PI donor 边规避**
    // （4-5a 排障实证）：4-3b 的受捐判定对内核任务取**共享内核侧表**粒度
    // （ktest J 段「共享表粒度」固化的设计语义）——pi_client(32) 阻塞期间，
    // 凡睡着的内核任务（含当时挂共享表的 console）都「名义持有」其 peer 端而
    // 受捐 32；若 console 直睡 endK:READABLE（内核派生位），它自己又构成
    // donor 边（peer=endU、need SEND），把受捐额**级联给所有持 console 发端的
    // 用户进程**——waiter 冻结 32 后永不入睡，4-3a 严格优先把永不入睡的
    // prio-0 waker 永久饿死（sleeper 路径 2 无人唤、BLOCK ok 永不出现）。
    // 两道修法正交共存：①私有表（spawn_with_tag——受捐只看自己的表，错捐
    // 喷洒断根，fa bench/其他共享表 donor 不再波及 console）；②`donor_edge`
    // 对 WAIT_ANY 一律 None（4-2b「v2 收口」既有语义）——console 借
    // wait_many+哑 Event（ponger 4-2b 同款对照项习语）主动退出 v1 donor 面；
    // 高优 client 阻塞 console-full 时对本服务的真实捐赠不受影响（受捐方向
    // 不经 wait 对象）。
    let h_dummy_ev = ct
        .insert(
            KernelObj::Event(crate::event::Event::new()),
            handle::RIGHT_SIGNAL,
        )
        .expect("main: console 哑 Event 进私有表");
    // arg 打包（pi_server 同款）：高 32 位哑 Event、低 32 位 endK——均 console
    // 私有表本地值（SVC 经 caller_table tag≠0 分支落到本表解析）。
    // prio 0（S-2b fix5 回滚）：消费者提优先在「生产有限」时成立——双核副核
    // 无界生产下 console 排空即被填满 = 8 层常驻，0 层验收任务（BLOCK/INTENT/
    // SVC/DEMO 宿主）全饿（fix5 轮实锤）。正解在源头限频（park 派发 mod4，
    // sched.rs），console 与生产者同层靠轮转分槽。
    berry_sched::spawn_with_tag(
        task_console,
        ((h_dummy_ev as usize) << 32) | h_end_k as usize,
        b"console",
        0,
        ct as *mut HandleTable as usize,
    );

    // P5 A-1：内核 RX 中断接线——console 的 wait_many 扩第三项（RX Event），
    // IRQ 侧「RX 到数据」→ 置位 + wake_matching 唤醒 console 消费（HAL
    // `uart.rs` 静态环 + `set_rx_notify` 槽；使能在 main 尾注册块）。RX
    // Event = console 私有表**第 3 插**（endK 首插/哑 Event 第 2 插之后），
    // 表序定值断言（4-2b/4-5a 惯例——错值 boot 期炸响，非静默错 handle）。
    let h_rx_ev = ct
        .insert(
            KernelObj::Event(crate::event::Event::new()),
            handle::RIGHT_SIGNAL,
        )
        .expect("main: RX Event 进 console 私有表");
    assert_eq!(
        h_rx_ev, 0x0001_0003,
        "main: console 表 RX Event 须槽 3/代号 1（第 3 插定值，console 读静态槽依赖）"
    );
    let p_rx = ct
        .get_raw(h_rx_ev, handle::ObjType::Event, 0)
        .expect("main: 取 RX Event ObjRef")
        .0;
    // 钉住（4-1 睡眠钉住纪律的静态版）：IRQ 侧裸用 ObjRef——retain 后永不
    // release = 有意泄漏（同 Box::leak 表惯例）。睡眠钉住防「全表 handle 被
    // 关光后 waker 解引用悬空」；本对象永不 close、_IRQ 也永不撒手，双保险。
    // SAFETY: boot 单线程；刚 get_raw 过（在表、类型 Event）。
    unsafe { (*p_rx.as_ptr()).retain() };
    // 静态槽传递（spawn arg 已满：hi32 哑 Event + lo32 endK——INTENT_H 同款
    // 惯例）：handle 给 console 任务读、ObjRef 给 IRQ 通知（rx_irq_notify 置位）
    // 与 console 清位（rx_ev_clear）共用。
    // SAFETY: boot 单线程写、先于一切任务与 IRQ 读（单核 DAIF 屏蔽窗 = 无竞态）。
    unsafe {
        *core::ptr::addr_of_mut!(RX_EV_H) = h_rx_ev;
        *core::ptr::addr_of_mut!(RX_EV_REF) = Some(p_rx);
    }

    // P5 A-2：bsh stdin 通道对的 console 面（endK2）——console 私有表**第 4
    // 插**（endK=1/哑 Event=2/RX Event=3 之后，定值断言惯例）。console 持
    // RIGHT_SEND（键盘完整行转发给 bsh）；对端 endB2 落 svcmgr 表（svcmgr 段
    // 插入并 pair——两表异地、配对延到后插入侧；boot 单线程，中间无人能发）。
    // spawn arg 已满 → handle 走静态槽 `BSH_IN_H`（RX_EV_H 同款惯例）。
    // 通道 opt-out（console 对同款）：系统服务转发键盘不借出 intent。
    let h_end_k2 = ct
        .insert(
            KernelObj::Channel(
                crate::channel::ChannelEnd::try_unpaired(true).expect("main: 建 bsh-stdin console 端"),
            ),
            handle::RIGHT_SEND,
        )
        .expect("main: bsh-stdin console 端进私有表");
    assert_eq!(
        h_end_k2, 0x0001_0004,
        "main: console 表 endK2 须第 4 插 0x0001_0004（console 读静态槽依赖）"
    );
    // SAFETY: boot 单线程写、先于 console 任务读（同 RX_EV_H 口径）。
    unsafe {
        *core::ptr::addr_of_mut!(BSH_IN_H) = h_end_k2;
    }

    // 3-2：good 的 root Vmar 进**它自己的表** = 首能力（能力模型 §5「第一个
    // 能力从哪来」——用户拿自己的地址空间根，MAP|MANAGE|DUPLICATE）。handle
    // 作 spawn arg → 入口 x0 → good 的 3-2 验收链经真 syscall map 读写自己的
    // VMO（全链落在 good 自己的表里——per-process 拆分后其余进程看不见）。
    let good_vmar = up_good.vmar.take().expect("main: good vmar");
    let good_vmar_h = ht_good
        .insert(KernelObj::Vmar(good_vmar), RIGHT_MAP | RIGHT_MANAGE | RIGHT_DUPLICATE)
        .expect("main: 插 root Vmar 进 good 表");
    // 4-5a：console 发端 append 进 good 表（vmar 后首插 = 槽 2/代号 1）——
    // good 模板硬编码依赖，boot 断言即防线（漂移 → boot 期炸响）。
    let h_cu_good = ht_good
        .insert_ref(pu, handle::RIGHT_SEND)
        .expect("main: console 发端进 good 表");
    assert_eq!(h_cu_good, 0x0001_0002, "main: good 表 console 须槽 2/代号 1（模板硬编码）");

    // 4-5a：abuser/spawner 的 console 发端 = 各表**首插**（0x0001_0001）——入口
    // x0 改传之（原 0；两模板自带 handle 都在运行期创建，首插槽位无既有依赖）。
    let h_cu_abuser = ht_abuser
        .insert_ref(pu, handle::RIGHT_SEND)
        .expect("main: console 发端进 abuser 表");
    let h_cu_spawner = ht_spawner
        .insert_ref(pu, handle::RIGHT_SEND)
        .expect("main: console 发端进 spawner 表");
    berry_sched::spawn_user(user::USER_CODE_VA, good_vmar_h as usize, user::USER_STACK_TOP,
                          b"user_good", up_good.ttbr0_pa, up_good.asid, tag_good);
    berry_sched::spawn_user(user::USER_CODE_VA, 0, user::USER_STACK_TOP,
                          b"user_rogue_ptr", up_rogue_ptr.ttbr0_pa, up_rogue_ptr.asid, tag_rogue);
    berry_sched::spawn_user(user::USER_CODE_VA, h_cu_abuser as usize, user::USER_STACK_TOP,
                          b"user_abuser", up_abuser.ttbr0_pa, up_abuser.asid, tag_abuser);
    // 3-3/3-4 验收：spawner 经真 syscall 创建第二个进程；process_start 的
    // bootstrap 迁移把「礼物 VMO」handle 送进子进程表（子入口 x0 收到后读出
    // 魔数自证——跨进程能力流动，见 user.rs spawner 模板注释）。
    berry_sched::spawn_user(user::USER_CODE_VA, h_cu_spawner as usize, user::USER_STACK_TOP,
                          b"user_spawner", up_spawner.ttbr0_pa, up_spawner.asid, tag_spawner);
    berry_sched::spawn(user::watchdog, 0, b"watchdog");
    // 4-0 验收（`IPC设计.md` Phase 4 拆步表）：sleeper/waker 对打——超时自醒
    // （'T'）+ 条件唤醒（'K'）双路径全绿打 `BLOCK ok`（smoke 第 14 标记）。
    // ktest 在 sched::init 前跑不能睡，睡眠面只能走 boot spawn 活证。
    berry_sched::spawn(task_sleeper, 0, b"sleeper");
    berry_sched::spawn(task_waker, 0, b"waker");

    // 4-1 验收（`IPC设计.md` Phase 4 拆步表 4-1）：阻塞 object_wait_one——
    // waiter（EL0）三路（超时 -ETIMEDOUT / poll 命中回写全满足集 / 真睡被
    // signal 唤醒）+ waker2（内核任务）经真 SVC signal 内核侧表 handle。
    // 同一 Event **双表挂同一 ObjRef**（insert_ref）——signal 侧与 wait 侧各
    // 持自己表的 handle（能力跨表互通的活证，匹配键 = ObjRef 地址）。
    let up_waiter =
        user::UserProcess::build(mm::frames(), user::user_waiter_code(), user::USER_MAGIC);
    let ht_waiter: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    use handle::{ObjType, RIGHT_SIGNAL};
    let h_ev_kernel = syscall::bootstrap_table()
        .insert(KernelObj::Event(event::Event::new()), RIGHT_SIGNAL)
        .expect("main: Event 进内核侧表");
    let (ev_ptr, _) = syscall::bootstrap_table()
        .get_raw(h_ev_kernel, ObjType::Event, 0)
        .expect("main: 取 Event ObjRef");
    let h_ev_waiter = ht_waiter
        .insert_ref(ev_ptr, RIGHT_SIGNAL)
        .expect("main: Event 进 waiter 表（同一 ObjRef）");
    // 4-5a：console 发端 append（Event 后首插 = 槽 2/代号 1）——waiter 模板硬编码。
    let h_cu_waiter = ht_waiter
        .insert_ref(pu, handle::RIGHT_SEND)
        .expect("main: console 发端进 waiter 表");
    assert_eq!(h_cu_waiter, 0x0001_0002, "main: waiter 表 console 须槽 2/代号 1（模板硬编码）");
    berry_sched::spawn_user(user::USER_CODE_VA, h_ev_waiter as usize, user::USER_STACK_TOP,
                          b"user_waiter", up_waiter.ttbr0_pa, up_waiter.asid,
                          ht_waiter as *mut HandleTable as usize);
    berry_sched::spawn(task_waker2, h_ev_kernel as usize, b"waker2");

    // 4-2 验收（`IPC设计.md` Phase 4 拆步表 4-2）：pinger/ponger 两 EL0 进程经
    // channel 阻塞链对打——pinger（endA：RIGHT_SEND）send 灌到 -EBUSY →
    // wait_one(WRITABLE) 阻塞 → ponger（endB：RIGHT_RECV）recv 腾位唤醒 →
    // 补发成功打 `CHAIN ok`（smoke 第 16 标记）。两端各进**对方进程的表**
    //（`channel_create` 建对语义的 boot 手搭版：两表 insert + `pair` 互钉）。
    let up_pinger =
        user::UserProcess::build(mm::frames(), user::user_pinger_code(), user::USER_MAGIC);
    let up_ponger =
        user::UserProcess::build(mm::frames(), user::user_ponger_code(), user::USER_MAGIC);
    let ht_pinger: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let ht_ponger: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    use crate::channel::ChannelEnd;
    use handle::{RIGHT_RECV, RIGHT_SEND};
    let h_end_a = ht_pinger
        .insert(
            KernelObj::Channel(ChannelEnd::try_unpaired(false).expect("main: 建 endA")),
            RIGHT_SEND,
        )
        .expect("main: endA 进 pinger 表");
    // 4-5a：console 发端 append（endA 后首插 = 槽 2/代号 1）——pinger 模板硬编码。
    let h_cu_pinger = ht_pinger
        .insert_ref(pu, RIGHT_SEND)
        .expect("main: console 发端进 pinger 表");
    assert_eq!(h_cu_pinger, 0x0001_0002, "main: pinger 表 console 须槽 2/代号 1（模板硬编码）");
    let h_end_b = ht_ponger
        .insert(
            KernelObj::Channel(ChannelEnd::try_unpaired(false).expect("main: 建 endB")),
            RIGHT_RECV,
        )
        .expect("main: endB 进 ponger 表");
    let (pa, pb) = (
        ht_pinger.get_raw(h_end_a, ObjType::Channel, 0).expect("main: 取 endA ObjRef").0,
        ht_ponger.get_raw(h_end_b, ObjType::Channel, 0).expect("main: 取 endB ObjRef").0,
    );
    // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
    unsafe { ChannelEnd::pair(pa, pb) };
    // 4-2b：哑 Event 进 ponger 表——wait_many 稳态等待的对照项（只入数组、
    // 永不 signal；等 1<<32 位 = 永不命中）。boot 直插定值断言：ponger 模板把
    // 该 handle 硬编为 0x0001_0002（表内第 2 插 = 槽 2/代号 1）——表序有变须同步
    // 模板（断言即防线，错值 → wait_many -EBADF → ponger 自陷被杀 → MANY 缺失）。
    let h_evt_ponger = ht_ponger
        .insert(KernelObj::Event(crate::event::Event::new()), RIGHT_SIGNAL)
        .expect("main: 哑 Event 进 ponger 表");
    assert_eq!(
        h_evt_ponger, 0x0001_0002,
        "main: ponger 表哑 Event 须为槽 2/代号 1（ponger 模板硬编码依赖）"
    );
    // 4-5a：console 发端 append（endB + 哑 Event 后 = 槽 3/代号 1）——ponger 模板硬编码。
    let h_cu_ponger = ht_ponger
        .insert_ref(pu, RIGHT_SEND)
        .expect("main: console 发端进 ponger 表");
    assert_eq!(h_cu_ponger, 0x0001_0003, "main: ponger 表 console 须槽 3/代号 1（模板硬编码）");
    berry_sched::spawn_user(user::USER_CODE_VA, h_end_a as usize, user::USER_STACK_TOP,
                          b"user_pinger", up_pinger.ttbr0_pa, up_pinger.asid,
                          ht_pinger as *mut HandleTable as usize);
    berry_sched::spawn_user(user::USER_CODE_VA, h_end_b as usize, user::USER_STACK_TOP,
                          b"user_ponger", up_ponger.ttbr0_pa, up_ponger.asid,
                          ht_ponger as *mut HandleTable as usize);

    // 4-3b 验收（`IPC设计.md` Phase 4 拆步表 4-3 前半）：PI 捐赠三任务链——
    // pi_client（prio 32，donor）send 灌满后 wait WRITABLE 阻塞；pi_server
    //（prio 0，持 recv 端 endS）阻塞在 Event E 上（受捐方——有界队列把 QNX
    //「双方同睡一个 channel」解耦，接收方持 handle 可睡任何对象，捐赠边靠
    // handle 表反查）；pi_probe 睡 5 slice 后断言 server 被捐到 eff=32、
    // signal E 放行，打 `PI ok`（smoke 第 18 标记）。三者全走真 SVC（EL1
    // 入口与 EL0 同权，3-1）；边闭合时序：client(32) 先跑睡 send-wait →
    // server 跑睡 E（block 钩子重推成边）→ probe 醒断言。
    //（RIGHT_SEND/RECV、ChannelEnd、ObjType、RIGHT_SIGNAL 均已在本函数
    // 更早的 use 里。）
    let h_end_c = syscall::bootstrap_table()
        .insert(KernelObj::Channel(ChannelEnd::try_unpaired(false).expect("main: 建 endC")),
                RIGHT_SEND)
        .expect("main: endC 进内核侧表");
    let h_end_s = syscall::bootstrap_table()
        .insert(KernelObj::Channel(ChannelEnd::try_unpaired(false).expect("main: 建 endS")),
                RIGHT_RECV)
        .expect("main: endS 进内核侧表");
    let (pc, ps) = (
        syscall::bootstrap_table().get_raw(h_end_c, ObjType::Channel, 0)
            .expect("main: 取 endC ObjRef").0,
        syscall::bootstrap_table().get_raw(h_end_s, ObjType::Channel, 0)
            .expect("main: 取 endS ObjRef").0,
    );
    // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
    unsafe { ChannelEnd::pair(pc, ps) };
    let h_ev_pi = syscall::bootstrap_table()
        .insert(KernelObj::Event(event::Event::new()), RIGHT_SIGNAL)
        .expect("main: PI Event 进内核侧表");
    // arg 打包（spawn 只有单 usize 参）：server = (h_ev_pi << 32) | h_end_s；
    // probe = h_ev_pi；client = h_end_c。
    berry_sched::spawn(task_pi_server,
        ((h_ev_pi as usize) << 32) | h_end_s as usize, b"pi_server");
    berry_sched::spawn(task_pi_probe, h_ev_pi as usize, b"pi_probe");
    berry_sched::spawn_with(task_pi_client, h_end_c as usize, b"pi_client", 32);

    // 4-3c 验收（`IPC设计.md` Phase 4 拆步表 4-3 后半，intent 五件套落地）：
    // 六内核任务全真 SVC——driver(0x02) 三发 → mid(0x04) recv 借入 0x02 / 借入
    // 转发 hop+1 / opt-out 通道 recv 回落 → sinkA(0x03) 收转发件（hop==1 = mid
    // 借入转发的活证，eff==0x02）→ slow(0x04) 借入后睡 200ms 超每跳预算 50ms，
    // 转发降级（装自己的 0x04；借入字段保持——无回落事件）→ sinkB(0x03) 吃
    // 两条 boot 伪造超帽消息（hop 帽 / 链累计封顶，拒传播回落）→ probe 睡
    // 8 slice 遍历 SLEEPQ 断言四睡眠者字段 + 遥测三计数器，打 `INTENT ok`
    //（smoke 第 19 标记）后 exit。通道全走内核侧表（boot 手搭版，单核 IRQ
    // 屏蔽窗 = 无竞态）；CH3 置 options bit0（opt-out）；伪造消息绕过 send 直
    // 压收端队列——超帽样本的确定性触发（队列 boot 期必空不满）。
    let mk_it_chan = |no_intent: bool| -> (u32, u32) {
        let hs = syscall::bootstrap_table()
            .insert(KernelObj::Channel(
                ChannelEnd::try_unpaired(no_intent).expect("main: 建 it send 端")),
                RIGHT_SEND)
            .expect("main: it send 端进内核侧表");
        let hr = syscall::bootstrap_table()
            .insert(KernelObj::Channel(
                ChannelEnd::try_unpaired(no_intent).expect("main: 建 it recv 端")),
                RIGHT_RECV)
            .expect("main: it recv 端进内核侧表");
        let (ps, pr) = (
            syscall::bootstrap_table().get_raw(hs, ObjType::Channel, 0)
                .expect("main: 取 it send ObjRef").0,
            syscall::bootstrap_table().get_raw(hr, ObjType::Channel, 0)
                .expect("main: 取 it recv ObjRef").0,
        );
        // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
        unsafe { ChannelEnd::pair(ps, pr) };
        (hs, hr)
    };
    let (h_d1, h_m1) = mk_it_chan(false);   // CH1 driver → mid（首手 0x02）
    let (h_m2, h_a2) = mk_it_chan(false);   // CH2 mid → sinkA（借入转发 hop=1）
    let (h_d3, h_m3) = mk_it_chan(true);    // CH3 opt-out（bit0）——拒借回落面
    let (_h_x4a, h_b4a) = mk_it_chan(false); // CH4a 伪造 hop 帽样本（发送端闲置）
    let (_h_x4b, h_b4b) = mk_it_chan(false); // CH4b 伪造链累计封顶样本
    let (h_d5, h_l5) = mk_it_chan(false);   // CH5 driver → slow（借入后超预算）
    let (h_l6, h_b6) = mk_it_chan(false);   // CH6 slow → sinkB（降级转发件）
    // 伪造消息直压收端队列（LC 档 0x01 载荷——帽子拦下才不至抬档）：
    // hop=8 恰顶跳数帽；acc = 链封顶 +1ns 恰超累计帽。两道帽的 recv 结算
    // 拒传播路径由此确定性触发（不赌 send 快照路径）。
    let push_forged = |hr: u32, meta: berry_sched::MsgIntent| {
        let p = syscall::bootstrap_table().get_raw(hr, ObjType::Channel, 0)
            .expect("main: 取伪造收端 ObjRef").0;
        let mut m = crate::channel::Message::new();
        m.bytes[..4].copy_from_slice(b"ITK!");
        m.nbytes = 4;
        m.meta = meta;
        // SAFETY: 刚 get_raw 过（在表、类型 Channel）；boot 单线程。
        unsafe { (*p.as_ptr()).obj.as_channel_mut().unwrap().push(m) };
    };
    push_forged(h_b4a, berry_sched::MsgIntent {
        intent: berry_sched::INTENT_LATENCY_CRITICAL,
        hop: berry_sched::INTENT_HOP_MAX,
        acc_ns: 0,
    });
    push_forged(h_b4b, berry_sched::MsgIntent {
        intent: berry_sched::INTENT_LATENCY_CRITICAL,
        hop: 0,
        acc_ns: berry_sched::INTENT_CHAIN_BUDGET_NS + 1,
    });
    // handle 传递槽（spawn 单 usize 参装不下 12 个 handle）：boot 写入先于一切
    // 任务启动（单核屏蔽窗）。槽位：0=d1 1=m1 2=m2 3=a2 4=d3 5=m3 6=b4a
    // 7=b6 8=b4b 9=d5 10=l5 11=l6。
    {
        let slots = [h_d1, h_m1, h_m2, h_a2, h_d3, h_m3,
                     h_b4a, h_b6, h_b4b, h_d5, h_l5, h_l6];
        // SAFETY: boot 单线程写入，先于任何任务读。
        unsafe {
            for (i, &h) in slots.iter().enumerate() {
                *core::ptr::addr_of_mut!(INTENT_H).cast::<u32>().add(i) = h;
            }
        }
    }
    berry_sched::spawn_intent(task_intent_driver, 0, b"it_driver",
                              berry_sched::INTENT_INTERACTIVE);
    berry_sched::spawn_intent(task_intent_mid, 0, b"it_mid", berry_sched::INTENT_BACKGROUND);
    berry_sched::spawn_intent(task_intent_sink_a, 0, b"it_sinkA", berry_sched::INTENT_UTILITY);
    berry_sched::spawn_intent(task_intent_sink_b, 0, b"it_sinkB", berry_sched::INTENT_UTILITY);
    berry_sched::spawn_intent(task_intent_slow, 0, b"it_slow", berry_sched::INTENT_BACKGROUND);
    berry_sched::spawn(task_intent_probe, 0, b"it_probe");

    // ---- 4-4 fastpath/slowpath 分层验收（`IPC设计.md` §7）----
    // 专用点对点 bench 对（方法论语：量 cycle 别拿稳态矩阵当基线——console
    // 排队是噪声源）。fa_ping(16)/fa_pong(32) 各持一端 SEND|RECV 双权
    //（`mk_it_chan` 只授单端单权，不适用）。prio 16/32 > 矩阵/intent 全 0 →
    // bench 独占 CPU 至完成（ms 级；且此刻其余任务全 Ready 未睡 → SLEEPQ 只有
    // pong 短暂在队，PI 共享表粒度零外溢）。稳态每轮：pong 先睡 wait READABLE
    // → ping send = 检查链全过 fastpath + 直切（32>16）→ pong 醒 recv+回 send
    //（ping 尚 Ready 未睡 → miss 走 slowpath）→ pong 循环回 wait 阻塞 → 切回
    // ping → ping wait poll 即命中（reply 已在队）→ recv 验回显。每轮
    // hits=1/miss=1/direct=1；ping 收尾断言三计数器 + RT（CNTVCT 差均值，
    // 参考 cycle = ticks×1e9/CNTFRQ；TCG 先出量级、真机复测标注）打 `FAST ok`
    //（smoke 第 20 标记）后双双 exit。
    let mk_fast_chan = || -> (u32, u32) {
        let hs = syscall::bootstrap_table()
            .insert(KernelObj::Channel(
                ChannelEnd::try_unpaired(false).expect("main: 建 fa A 端")),
                RIGHT_SEND | RIGHT_RECV)
            .expect("main: fa A 端进内核侧表");
        let hr = syscall::bootstrap_table()
            .insert(KernelObj::Channel(
                ChannelEnd::try_unpaired(false).expect("main: 建 fa B 端")),
                RIGHT_SEND | RIGHT_RECV)
            .expect("main: fa B 端进内核侧表");
        let (ps, pr) = (
            syscall::bootstrap_table().get_raw(hs, ObjType::Channel, 0)
                .expect("main: 取 fa A ObjRef").0,
            syscall::bootstrap_table().get_raw(hr, ObjType::Channel, 0)
                .expect("main: 取 fa B ObjRef").0,
        );
        // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
        unsafe { ChannelEnd::pair(ps, pr) };
        (hs, hr)
    };
    let (h_fa, h_fb) = mk_fast_chan();
    berry_sched::spawn_with(task_fast_ponger, h_fb as usize, b"fa_pong", 32);
    berry_sched::spawn_with(task_fast_pinger, h_fa as usize, b"fa_ping", 16);

    // ---- 4-5 前置：常驻内核服务私有表使能自证（spawn_with_tag + caller_table
    // EL1 tag≠0 分支，修 4-3b 共享表粒度的错捐级联——常驻服务须挂私有表）----
    // svc_tag 持 leak 私有表里的通道端 + 拿共享表真 handle（fa B 端，fa 任务
    // 不 close 恒有效）作负向判别样本：poll-only 不入 SLEEPQ，PI/intent 零纠缠。
    {
        let st: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        let h_own = st
            .insert(KernelObj::Channel(
                ChannelEnd::try_unpaired(false).expect("main: 建 tag A 端")),
                RIGHT_SEND | RIGHT_RECV)
            .expect("main: tag A 端进私有表");
        let h_peer = syscall::bootstrap_table()
            .insert(KernelObj::Channel(
                ChannelEnd::try_unpaired(false).expect("main: 建 tag B 端")),
                RIGHT_SEND | RIGHT_RECV)
            .expect("main: tag B 端进内核侧表");
        let (pa, pb) = (
            st.get_raw(h_own, ObjType::Channel, 0).expect("main: 取 tag A ObjRef").0,
            syscall::bootstrap_table().get_raw(h_peer, ObjType::Channel, 0)
                .expect("main: 取 tag B ObjRef").0,
        );
        // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
        unsafe { ChannelEnd::pair(pa, pb) };
        // arg 打包双 handle：低 32 = 自己表的端，高 32 = 共享表的 fa B 端。
        let packed = h_own as usize | ((h_fb as usize) << 32);
        berry_sched::spawn_with_tag(task_svc_tag, packed, b"svc_tag", 0,
                                    st as *mut HandleTable as usize);
    }

    // ---- 4-5b 验收（`IPC设计.md` Phase 4 拆步表 4-5 后半）：svcmgr（EL0）经
    // 真 syscall 全链拉起 echo 服务（用户态进程）——里程碑「服务管理器拉起
    // 一个服务」。svcmgr 自身 boot 手搭（spawner 同形）；echo 由 svcmgr 运行期
    // 创建：process_create → vmo_create/commit/write(内嵌模板) → vmar_map →
    // channel_create → thread_create → process_start(end1 迁 echo 表)。验收
    // 往返（send ECHO_REQ → wait READABLE → recv 验回显）打 `SVC ok`（smoke
    // 第 21 标记）、稳态每圈 'v'（每个 'v' = 一次实证往返）。echo 静默（无
    // console handle——boot 无法预知运行期建表的 handle 值；断链由 svcmgr
    // 侧标记缺失/watchdog 错杀断言兜底）。PI：两 EL0 各持私有表、全 prio 0
    // ——互为 donor/受捐的捐赠额恒 0，无级联面。----
    let up_svcmgr =
        user::UserProcess::build(mm::frames(), user::user_svcmgr_code(), user::USER_MAGIC);
    let ht_svcmgr: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    // console 发端 = svcmgr 表**首插**（0x0001_0001）——入口 x0 传入（模板
    // 硬编码依赖，boot 断言拦表序漂移）。
    let h_cu_svcmgr = ht_svcmgr
        .insert_ref(pu, RIGHT_SEND)
        .expect("main: console 发端进 svcmgr 表");
    assert_eq!(
        h_cu_svcmgr, 0x0001_0001,
        "main: svcmgr 表 console 须首插 0x0001_0001（模板硬编码）"
    );

    // ---- G-eco 入机侧第二刀：rootfs 字节 → 只读 VMO 交 svcmgr（编译侧
    // G-eco-c 消费端的 seam：cpio/ELF 解析全在用户态，内核只交字节流对象）。
    // 传递 = 表内**第 2 插定值 0x0001_0002**——svcmgr 是 boot 手搭进程，x0
    // bootstrap 槽已被 console 发端占用（4-5b），走 4-2b ponger 哑 Event /
    // 4-5a console endU 同款「boot 直插 + 定值断言」惯例；缺席语义 = 表内
    // 无此 handle → 消费端查表 -EBADF 即「无 rootfs」降级（裸 run/smoke 零
    // 扰动）。rights 仅 READ——vmo_read/vmo_get_size 可用，map/write/dup
    // 放宽面拒（权限在 handle 不在对象）。数据面 = 第一刀的 ROOTFS static
    // 整段搬进标准匿名 VMO（create+commit+write 内核侧三连，页模型零新
    // 变体——物理不连续无碍，vmo_read 本就按页走 direct map）；搬运完原
    // 缓冲即释放（Owned 路 峰值 ≈ 2× rootfs；Borrowed 零新增——blob 留
    // 保留区即其归宿）。----
    // SAFETY: boot 单线程；take 取走后置 None（字节已移交 VMO，源缓冲释放）。
    // A1-3b 三态分派：Assembled（fw_cfg 多卷）boot 期已是 VMO，直交零拷贝；
    // 另两源走字节三连（rootfs_bytes_to_vmo，原路径逐字同）。
    let h_rootfs = unsafe { (*(&raw mut ROOTFS)).take() }.map(|buf| {
        let vmo = match buf {
            RootfsBuf::Assembled(v, _) => v,
            RootfsBuf::Borrowed(b) => rootfs_bytes_to_vmo(b),
            RootfsBuf::Owned(v) => {
                // Owned 先释放源缓冲再入表（峰值 ≈2× rootfs 的窗口收窄）。
                let vmo = rootfs_bytes_to_vmo(&v);
                drop(v);
                vmo
            }
        };
        ht_svcmgr
            .insert(handle::KernelObj::Vmo(vmo), handle::RIGHT_READ)
            .expect("main: rootfs VMO 进 svcmgr 表")
    });
    // （A-2）rootfs 相位标记——endB2 插序断言用：rootfs 槽缺席 = 少一插。
    let rootfs_present = h_rootfs.is_some();
    match h_rootfs {
        Some(h) => {
            assert_eq!(
                h, 0x0001_0002,
                "main: svcmgr 表 rootfs VMO 须第 2 插 0x0001_0002（编译侧硬编码）"
            );
            berry_hal_aarch64::uart_puts(b"ROOTFS vmo=");
            berry_hal_aarch64::uart_put_hex64(h as u64);
            berry_hal_aarch64::uart_puts(b"\n");
        }
        None => berry_hal_aarch64::uart_puts(b"ROOTFS vmo=none\n"),
    }

    // P5 A-2：bsh stdin 对端的 svcmgr 面（endB2）——console(1)/rootfs VMO(有则
    // 2) 之后插入。**带 rootfs = 第 3 插 0x0001_0003**（svcmgr 编译侧硬编码
    // 依赖——spawn bsh 时随 env 块 gift[1] 迁交，仅 rootfs 相消费故只对
    // rootfs 相定值）；裸跑 rootfs 槽缺席 = 第 2 插 0x0001_0002（无人读——
    // console 转发进 8 深队列灌满后丢弃计数，零扰动）。svcmgr 仅中转不自读；
    // RIGHTS_RECV 随迁移原样进 bsh 表（fd0 = libc d-3b 既有链）。此处
    // pair——两端 ObjRef 齐活（endK2 已在 console 表，互钉 retain）。
    let h_end_b2 = ht_svcmgr
        .insert(
            KernelObj::Channel(
                crate::channel::ChannelEnd::try_unpaired(true).expect("main: 建 bsh-stdin svcmgr 端"),
            ),
            handle::RIGHT_RECV,
        )
        .expect("main: bsh-stdin 端进 svcmgr 表");
    assert_eq!(
        h_end_b2,
        if rootfs_present { 0x0001_0003 } else { 0x0001_0002 },
        "main: svcmgr 表 endB2 插序（rootfs 相须 0x0001_0003——编译侧硬编码）"
    );
    let (pk2, pu2) = (
        ct.get_raw(h_end_k2, handle::ObjType::Channel, 0)
            .expect("main: 取 endK2 ObjRef")
            .0,
        ht_svcmgr
            .get_raw(h_end_b2, handle::ObjType::Channel, 0)
            .expect("main: 取 endB2 ObjRef")
            .0,
    );
    // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
    unsafe { crate::channel::ChannelEnd::pair(pk2, pu2) };

    // ---- P6 A-2：virtio-mmio 物理窗口 VMO（用户态驱动地基第一刀）。QEMU
    // virt 板 virtio-mmio 整窗（HAL 板级常量：32 槽 × 0x200 = [0x0A00_0000,
    // 0x0A00_4000)——槽 31→0 倒序建总线、首个 -device 落最高槽，**设备落位
    // 随 -device 顺序漂移，消费端扫窗定位**；DTB 枚举随 v2 真机路径）包成
    // MMIO VMO 进 svcmgr 表——**仅 rootfs 相**（rootfs_present gate：裸相零
    // 扰动，缺席 = svcmgr 查表 -EBADF → eco 降级）**第 4 插定值 0x0001_0004**
    // （console 1 / rootfs VMO 2 / endB2 3 之后，boot assert 拦表序漂移——
    // svcmgr 编译侧硬编码同款惯例）。
    // rights = READ|WRITE|MAP：vmar_map 面恰 R|W + 映射权（3-2 门控）。
    // `create_mmio` 是 boot-only 构造——物理窗口是 boot 授的能力，无 syscall
    // 面（Zircon `zx_vmo_create_physical` 收紧版）；commit/read/write/decommit
    // 对它一律 -ENOTSUP（设备只能经映射 volatile 访问），map/unmap 照常。----
    if rootfs_present {
        let vmo = vmo::Vmo::create_mmio(
            berry_hal_aarch64::VIRTIO_MMIO0_PA,
            berry_hal_aarch64::VIRTIO_MMIO_LEN,
        )
        .expect("main: mmio Vmo create");
        let h_mmio = ht_svcmgr
            .insert(
                handle::KernelObj::Vmo(vmo),
                handle::RIGHT_READ | handle::RIGHT_WRITE | handle::RIGHT_MAP,
            )
            .expect("main: mmio VMO 进 svcmgr 表");
        assert_eq!(
            h_mmio, 0x0001_0004,
            "main: svcmgr 表 mmio VMO 须第 4 插 0x0001_0004（编译侧硬编码）"
        );
        berry_hal_aarch64::uart_puts(b"VIRTIO vmo=");
        berry_hal_aarch64::uart_put_hex64(h_mmio as u64);
        berry_hal_aarch64::uart_puts(b"\n");
    }

    // ---- GUI-1（V-6 刀 2 后段挂账）：GOP framebuffer MMIO 窗授能 + 几何
    // info VMO。fb 窗包成 MMIO VMO（`create_mmio` boot-only——设计文档
    // 明文否掉 vmo_create_physical syscall 路线；ramfb 后备虽是普通 DRAM，
    // 按设备窗语义授能 = Device-nGnRnE 直写直达）进 svcmgr 表**第 5 插定值
    // 0x0001_0005**（console 1 / rootfs 2 / endB2 3 / virtio 4 之后，boot
    // assert 拦表序漂移——svcmgr 编译侧硬编码惯例）。几何五域走 40B 匿名
    // VMO（5×u64 LE）**第 6 插 0x0001_0006**——fb 窗 VMO 页取整丢信息、
    // size 反推不可行；svcmgr 转授 guifbd（info 走 gift[2]，fb 窗走
    // SpawnSpec mmio_vmo 面映固定 VA）。**EFI 路专属**（gop 缺席 = -kernel
    // 五跑/无显示 EFI 路零插表零扰动）。create_mmio 对齐/界拒 → 诊断行
    // 降级不 panic（boot 期无 panic 纪律），svcmgr 探测 -EBADF 即 eco 降级。
    // fb 段 buddy 双写风险不在此打点自证（handoff 已 move）：RamfbDxe fb
    // 是 AllocatePages 产物（非 Conventional，V-1b pass2 已 reserve），
    // 兜底证据 = guifbd 像素读回差分断言（`FBW ok` 缺失即 loud）。----
    if rootfs_present {
        if let Some((fb_pa, w, h, stride, fmt)) = gop_saved {
            // 幅 = ceil_page(stride×h)（实测 800×600×4 = 1,920,000B → 469 页）。
            let fb_bytes = (stride as u64) * (h as u64);
            let fb_len = ((fb_bytes + 0xFFF) / 0x1000) * 0x1000;
            match vmo::Vmo::create_mmio(fb_pa as usize, fb_len as usize) {
                Ok(vmo) => {
                    let h_fb = ht_svcmgr
                        .insert(
                            handle::KernelObj::Vmo(vmo),
                            handle::RIGHT_READ | handle::RIGHT_WRITE | handle::RIGHT_MAP,
                        )
                        .expect("main: fb VMO 进 svcmgr 表");
                    assert_eq!(
                        h_fb, 0x0001_0005,
                        "main: svcmgr 表 fb VMO 须第 5 插 0x0001_0005（编译侧硬编码）"
                    );
                    // 几何 info VMO：40B = fb_pa/w/h/stride/fmt 五域 u64 LE
                    //（一页 VMO，create+commit+write 三连——rootfs VMO 同款）。
                    let mut info = [0u8; 40];
                    info[0..8].copy_from_slice(&fb_pa.to_le_bytes());
                    info[8..16].copy_from_slice(&(w as u64).to_le_bytes());
                    info[16..24].copy_from_slice(&(h as u64).to_le_bytes());
                    info[24..32].copy_from_slice(&(stride as u64).to_le_bytes());
                    info[32..40].copy_from_slice(&(fmt as u64).to_le_bytes());
                    let mut ivmo =
                        vmo::Vmo::create(4096).expect("main: fbinfo Vmo create");
                    ivmo.commit(mm::frames(), 0, 4096)
                        .expect("main: fbinfo Vmo commit");
                    ivmo.write(0, &info).expect("main: fbinfo Vmo write");
                    let h_info = ht_svcmgr
                        .insert(handle::KernelObj::Vmo(ivmo), handle::RIGHT_READ)
                        .expect("main: fbinfo VMO 进 svcmgr 表");
                    assert_eq!(
                        h_info, 0x0001_0006,
                        "main: svcmgr 表 fbinfo VMO 须第 6 插 0x0001_0006（编译侧硬编码）"
                    );
                    berry_hal_aarch64::uart_puts(b"FB vmo=0x10005 pages=");
                    berry_hal_aarch64::uart_put_dec(fb_len / 0x1000);
                    berry_hal_aarch64::uart_puts(b"\n");
                    // GUI-1 段②：console fan-out 双路上屏管道对。console 表
                    // endK_fb **第 5 插**（endK=1/哑 Event=2/RX Event=3/endK2=4
                    // 之后，定值断言惯例）；peer 落 svcmgr 表**第 7 插
                    // 0x0001_0007**（fb VMO 5/info 6 之后，随 env 块 extras[1]
                    // take 迁入 guifbd——libc `gift3_h()`）。console 持
                    // RIGHT_SEND（drain 循环 best-effort 转发：满则丢弃 +
                    // fb_drop 计数，不重试不阻塞保串口主路）；guifbd 只 recv
                    // 不回发 = 无回环风暴（**wait 数组不加第 5 项**——endK_fb
                    // READABLE 永不置位 = 死项，best-effort 发送也不需要
                    // WRITABLE 等待）。通道 opt-out（console/endK2 同款）：
                    // 显示转发不借出 intent。spawn arg 已满 → handle 走静态槽
                    // `FB_PIPE_H`（RX_EV_H/BSH_IN_H 惯例）。
                    let h_end_kfb = ct
                        .insert(
                            KernelObj::Channel(
                                crate::channel::ChannelEnd::try_unpaired(true)
                                    .expect("main: 建 fb-上屏 console 端"),
                            ),
                            handle::RIGHT_SEND,
                        )
                        .expect("main: fb-上屏 console 端进私有表");
                    assert_eq!(
                        h_end_kfb, 0x0001_0005,
                        "main: console 表 endK_fb 须第 5 插 0x0001_0005（console 读静态槽依赖）"
                    );
                    let h_end_bfb = ht_svcmgr
                        .insert(
                            KernelObj::Channel(
                                crate::channel::ChannelEnd::try_unpaired(true)
                                    .expect("main: 建 fb-上屏 guifbd 端"),
                            ),
                            handle::RIGHT_RECV,
                        )
                        .expect("main: fb-上屏 guifbd 端进 svcmgr 表");
                    assert_eq!(
                        h_end_bfb, 0x0001_0007,
                        "main: svcmgr 表 endB_fb 须第 7 插 0x0001_0007（编译侧硬编码）"
                    );
                    let (pkf, puf) = (
                        ct.get_raw(h_end_kfb, handle::ObjType::Channel, 0)
                            .expect("main: 取 endK_fb ObjRef")
                            .0,
                        ht_svcmgr
                            .get_raw(h_end_bfb, handle::ObjType::Channel, 0)
                            .expect("main: 取 endB_fb ObjRef")
                            .0,
                    );
                    // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——
                    // pair 完成互钉（endK2/endB2 同款：两表异地、配对延到后
                    // 插入侧；boot 单线程，中间无人能发）。
                    unsafe { crate::channel::ChannelEnd::pair(pkf, puf) };
                    // SAFETY: boot 单线程写、先于 console 任务读（BSH_IN_H 口径）。
                    unsafe {
                        *core::ptr::addr_of_mut!(FB_PIPE_H) = h_end_kfb;
                    }
                }
                Err(_) => {
                    // 界检已过仍拒 = fb_pa 非页对齐（RamfbDxe AllocatePages
                    // 产物按页对齐，理论不可达）——降级 loud，FBW ok 缺失
                    // 由 smoke-image 兜底。
                    berry_hal_aarch64::uart_puts(b"FB vmo=fail pa=");
                    berry_hal_aarch64::uart_put_hex64(fb_pa);
                    berry_hal_aarch64::uart_puts(b"\n");
                }
            }
        }
    }

    // D3（G-eco-b）：svcmgr 是首个多页镜像（编译产物内嵌 echo/talker 平像），
    // data/stack 上移——sp 从 `stack_top` 取，不读单页常量。
    berry_sched::spawn_user(user::USER_CODE_VA, h_cu_svcmgr as usize, up_svcmgr.stack_top,
                          b"user_svcmgr", up_svcmgr.ttbr0_pa, up_svcmgr.asid,
                          ht_svcmgr as *mut HandleTable as usize);

    // ---- 4-6 B 半（AI 最小演示，`docs/4-6出口演示设计草案.md` B 半管线）：
    // 遥测 dump（`tele:` 行）→ host 侧 advisor.py（`tools/`，工具非关卡）规则
    // 出建议 → 构建期回填注入表（演示级通道，v3 policy_bind 随 ADR #21 实验
    // 轨道再立运行时协议）→ 内核执法活证（越界 +200 被 clamp 到 ±64——apply/
    // weight_of 双点既有包络、热路径零改动，probe 读在队字段做机器断言）+
    // F9① 开销不变量最小兑现（bind 前后 bench RT 同量级，容差 2x——TCG 噪声
    // 界，-icount 精确版/真机 p99 随 FAST 线留 G 卡复测）。三张 leak 表：
    // tag_D（ai_demo 私有——睡 Event 等用户位，无 channel 受捐面）、tag_P
    //（bench 对两轮共用、bind 合法项 key）、tag_pr（probe 私有——**长命睡眠
    // 内核任务不睡共享表**，4-5 纪律：pi 窗错捐 eff 32 冻结随行、严格优先反
    // 噬 prio-0 轮转、probe 唤醒周期反把 demo 轮次推过自身重试预算，本步
    // bisect 实踩；表内仅 E_rel 一个 Event、无 channel 端点 → 零受捐面）。----
    let ht_ai: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let ht_ab: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    // bench 通道对（fa 同款：双端各 SEND|RECV，两轮复用——每轮 send/recv 严格
    // 配对，轮末两队列必空回原始态）。
    let h_ab_a = ht_ab
        .insert(KernelObj::Channel(
            ChannelEnd::try_unpaired(false).expect("main: 建 ai A 端")),
            RIGHT_SEND | RIGHT_RECV)
        .expect("main: ai A 端进 bench 表");
    let h_ab_b = ht_ab
        .insert(KernelObj::Channel(
            ChannelEnd::try_unpaired(false).expect("main: 建 ai B 端")),
            RIGHT_SEND | RIGHT_RECV)
        .expect("main: ai B 端进 bench 表");
    let (pab_a, pab_b) = (
        ht_ab.get_raw(h_ab_a, ObjType::Channel, 0).expect("main: 取 ai A ObjRef").0,
        ht_ab.get_raw(h_ab_b, ObjType::Channel, 0).expect("main: 取 ai B ObjRef").0,
    );
    // SAFETY: 两端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成互钉。
    unsafe { ChannelEnd::pair(pab_a, pab_b) };
    // E_done（bench 完成事件）：demo 表原件（等 1<<34）+ bench 表 insert_ref
    //（pinger 收尾 signal 用——跨表同一 ObjRef，匹配键互通）。
    let h_done_d = ht_ai
        .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
        .expect("main: E_done 进 demo 表");
    let p_done = ht_ai
        .get_raw(h_done_d, ObjType::Event, 0)
        .expect("main: 取 E_done ObjRef").0;
    let h_done_p = ht_ab
        .insert_ref(p_done, handle::RIGHT_SIGNAL)
        .expect("main: E_done 落 bench 表");
    // E_rel（probe 放行事件）：demo 表原件（等 1<<36）+ probe 私有表
    // insert_ref（probe signal 用——跨表同一 ObjRef，匹配键互通）。
    let h_rel_d = ht_ai
        .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
        .expect("main: E_rel 进 demo 表");
    let p_rel = ht_ai
        .get_raw(h_rel_d, ObjType::Event, 0)
        .expect("main: 取 E_rel ObjRef").0;
    let ht_pr: &'static mut HandleTable =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
    let h_rel_p = ht_pr
        .insert_ref(p_rel, handle::RIGHT_SIGNAL)
        .expect("main: E_rel 落 probe 表");
    // demo 上下文（spawn arg 单指针携带；tag 值是运行期表指针，注入表只能
    // 运行期构造——key 编译期不可知）。
    let ai_ctx: &'static AiDemoCtx =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(AiDemoCtx {
            h_done: h_done_d,
            h_rel: h_rel_d,
            h_ping_arg: h_ab_a as usize | (h_done_p as usize) << 32,
            h_pong_arg: h_ab_b as usize,
            tag_p: ht_ab as *mut HandleTable as usize,
            tag_d: ht_ai as *mut HandleTable as usize,
        }));
    berry_sched::spawn_with_tag(task_ai_demo, ai_ctx as *const AiDemoCtx as usize,
                                b"ai_demo", 0, ht_ai as *mut HandleTable as usize);
    berry_sched::spawn_with_tag(task_ai_probe, h_rel_p as usize, b"ai_probe", 0,
                                ht_pr as *mut HandleTable as usize);

    // ---- R-1（docs/基准与压力测试设计.md §10）：rt_probe 门控 spawn ----
    // 设计只写了「不进 smoke」（公约 7：~34s 采样循环 vs boot 开销 >5s /
    // smoke 总时长劣化 >10% 界），未定稿门控机制——smoke 与 bench-rt 同一
    // 内核 ELF，无条件 spawn = 每次常规 boot 白付 34s。以 **fw_cfg 具名项**
    // 闭合：`-fw_cfg name=opt/berry/rt,string=1`（bench.py --rt 专递）在位
    // 才 spawn——缺席 = run/smoke 零扰动（rootfs 探测 find_file 同款先例，
    // main.rs 头段）；EFI 路无 fw_cfg 具名文件 → smoke-image 亦天然缺席。
    // 位置约束：sched::init 后（spawn 前置）+ enable_irq 前（boot DAIF
    // 屏蔽窗内 spawn，全体先例共同约束）；find_file 的 MMIO 读与 rootfs
    // 探测同窗同款、微秒级。
    if berry_hal_aarch64::fwcfg::find_file(RT_PROBE_NAME).is_some() {
        let ht_rt: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        // 表内仅一个哑 Event（ai_probe 表同形）：纯用户位等待 →
        // pi::donor_edge 返 None（零受捐，公约 3 判据②）+ 私有表（无跨表
        // 错捐面）。arg = ObjRef 地址（wait_obj 匹配键——无人 signal 它，
        // 唯一醒因 Timeout）。表/Event 有意泄漏（有界、boot 生命周期——
        // 内核任务 pinned==0 退出钩子直通不 drain，4-6 坑退出版）。
        let h_rt = ht_rt
            .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
            .expect("main: rt 哑 Event 进 rt 表");
        let p_rt = ht_rt
            .get_raw(h_rt, ObjType::Event, 0)
            .expect("main: 取 rt ObjRef")
            .0;
        // prio 8：全体常驻（矩阵/console/watchdog/EL0 用户）之上、瞬态 boot
        // bench 对（fa/ai ping=16 / pong=32）之下——「被饿本身就是待测的调度
        // 延迟」（设计 §10）。落地实录（首跑实踩）：设计原文 prio 24 会以
        // 24>16 抢占 ai_ping 的 IPC 往返测量窗，一次 rt 醒来（双切换 + PI
        // 重推 ≈360k 周期）就把 AIDEMO 的 rt2 顶到 rt1×4，撞容差 2x 断言
        // panic（boot 死 = RT ok 永缺失）。8 结构性排除该类干扰（对 16/32
        // 永让位——测不到它们，也污染不到它们）；稳态（bench-rt boot 无
        // 0..24 之间任务）唤醒测量与 24 恒等。
        berry_sched::spawn_with_tag(task_rt_probe, p_rt.as_ptr() as usize, b"rt_probe", 8,
                                    ht_rt as *mut HandleTable as usize);
    }

    // ---- S-1（docs/基准与压力测试设计.md §11）：soakmon 门控 spawn ----
    // 设计只写了「不进 CI 主线（小时级），周跑或手跑」，未定稿门控机制——
    // 但 soakmon **常驻不退出**（比 rt_probe 一次性采样更强）：无门控 = 每次
    // 常规 boot 永久多一个任务（runqueue/PI 图节点 + 私有表/Event 泄漏 +
    // 每 60s vct 一次 prio-8 抢占唤醒），稳态组成被永久改变 + 交互
    // boot 串口每 ~60-120s wall（机型 CNTVCT 速率相关）冒一行——R-1「不进
    // smoke 需要机制不是自律」先例直接继承：`-fw_cfg name=opt/berry/soak,
    // string=1`（soak.sh 专递，与 rootfs 具名项并存——fw_cfg FILE_DIR 多项
    // 按名走查）在位才 spawn；缺席 = run/run-rootfs/smoke 零扰动，EFI 路无
    // fw_cfg 具名文件 = smoke-image 亦天然缺席。位置约束同 rt 块：sched::
    // init 后 + boot DAIF 屏蔽窗内（enable_irq 前）。表/Event 有意泄漏
    // （有界、boot 生命周期——常驻任务永不退出，泄漏即生命周期，console
    // RX_EV「retain 后永不 release」同族惯例）。块内 spawn 后打
    // `SOAK gate on`（门控自证行，S-1 对抗审查 finding 2）：打印与 spawn
    // 同块 = 「行在 ⟺ soakmon 已 spawn」——缺席侧抽查从「等首拍超时」
    // （潜伏 5 周期 ≈300-600s wall，短窗抽查无判别力）降为短窗 grep 本行。
    if berry_hal_aarch64::fwcfg::find_file(SOAK_PROBE_NAME).is_some() {
        let ht_soak: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        // 表内仅一个哑 Event（rt 块同形）：纯用户位等待 → pi::donor_edge
        // 返 None（零受捐面）+ 私有表（无跨表错捐面）。arg = ObjRef 地址
        // （wait_obj 匹配键——无人 signal 它，唯一醒因 Timeout）。
        let h_soak = ht_soak
            .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
            .expect("main: soak 哑 Event 进 soak 表");
        let p_soak = ht_soak
            .get_raw(h_soak, ObjType::Event, 0)
            .expect("main: 取 soak ObjRef")
            .0;
        // prio 8（R-1 同构）：全体常驻（矩阵/console/watchdog/EL0 用户全 0）
        // 之上——心跳到点立即抢占打出，「心跳在 = 调度活」语义干净；瞬态
        // boot bench 对（16/32）之下——boot 窗口零干涉（首拍潜伏 5 周期后
        // 它们早已退出，结构性让位仍是免费保险）。
        berry_sched::spawn_with_tag(task_soakmon, p_soak.as_ptr() as usize, b"soakmon", 8,
                                    ht_soak as *mut HandleTable as usize);
        berry_hal_aarch64::uart_puts(b"SOAK gate on\n");
    }

    // 注册回调（HAL→sched 反转，避循环依赖；均须在 enable_irq / 首个用户任务被
    // 调度之前）。IRQ 槽驱动抢占；用户故障槽驱动 G1.5 kill。
    berry_hal_aarch64::set_irq_dispatch(berry_sched::on_irq_tick);
    berry_hal_aarch64::set_user_fault_exit(berry_sched::user_fault_exit);
    // P5 A-1：RX 通知槽 + 使能 PL011 RX（SPI 33）——均在 DAIF 屏蔽窗内
    // （enable_irq 前）。次序硬约束：通知槽须先注册（槽空时 rx_irq 平凡
    // 跳过——首字节静默落环无人唤醒）；rx_init 的 IMSC.RXIM 最后开（线一拉
    // 高 pending 即真数据，此前 GIC/粘滞已清干净）。
    berry_hal_aarch64::uart::set_rx_notify(rx_irq_notify);
    // SAFETY: MMIO 写；boot 单线程、DAIF 屏蔽。
    unsafe { berry_hal_aarch64::uart::rx_init() };
    // S-1 副核 bring-up（`SMP设计草案.md` §3.2/§4）：窗口 = DAIF 屏蔽尾窗
    //（全 spawn/回调注册后、arm_slice/enable_irq 前）——主核 CPU_ON 后静默
    // 轮询心跳，无中断无调度，与副核 bring-up 窗口结构性无重叠（UART 单写者
    // 纪律）。单核（默认 -smp 1）CPU_ON 返回 -2 → 一行降级，boot 继续；
    // EL2 机型 / EFI 路按 conduit 门跳过（见 smp.rs 注）。`SMP ok ncores=2`
    // 仅 -smp 2 双核心跳闭环后打（状态谓词，smoke-smp 专用标记）。
    berry_hal_aarch64::smp::smp_bringup(efi_boot);
    // S-2b PAR 金丝雀门（`docs/SMP-S2实施设计草案.md` §5.3）：副核 bring-up
    // 成功（hb2 心跳闭环 → smp_active）才 spawn task_par_probe + 置参与 gate
    //（soak 块同款形态：私有表 + 哑 Event + prio 8 + 有意泄漏）。序硬约束：
    // **spawn 先于 par_enable**——gate 先开而探针缺位 = 副核已开始拉用户任务、
    // 金丝雀缺席；gate 在 arm_slice/enable_irq（主核首 tick）前置位 = 副核
    // 探测环首轮 tick 即可开始参与。`PAR gate on` 自证行与 spawn 同块
    //（SOAK gate on 先例：行在 ⟺ 探针已 spawn）。
    if berry_hal_aarch64::smp::smp_active() {
        let ht_par: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        // 表内仅一个哑 Event（soak 块同形）：纯用户位等待 → pi::donor_edge
        // 返 None（零受捐面）+ 私有表（无跨表错捐面）。无人 signal 它，唯一
        // 醒因 Timeout。
        let h_par = ht_par
            .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
            .expect("main: par 哑 Event 进 par 表");
        let p_par = ht_par
            .get_raw(h_par, ObjType::Event, 0)
            .expect("main: 取 par ObjRef")
            .0;
        // prio 8（soakmon 同档）：常驻全体（矩阵/console/watchdog/EL0 用户全 0）
        // 之上——探针醒即抢占读数；boot 瞬态 bench 对（16/32）之下零干涉。
        berry_sched::spawn_with_tag(task_par_probe, p_par.as_ptr() as usize, b"parprobe", 8,
                                    ht_par as *mut HandleTable as usize);
        berry_hal_aarch64::smp::par_enable();
        berry_hal_aarch64::uart_puts(b"PAR gate on\n");
    }
    // 武装首个时间片 + 解蔽 IRQ：之后 timer 每 slice（10Hz）落 exception_irq → on_irq_tick。
    // SAFETY: 写 CNTV_CTL/CVAL（绝对截止）；EL1。arm_slice 设 ENABLE+不屏蔽（0b01）。
    unsafe { berry_hal_aarch64::timer::arm_slice() };
    berry_hal_aarch64::enable_irq();
    berry_sched::yield_now(); // idle(main) 让出 → task_a；timer 每 slice 抢占切到 task_b → A/B 交替。
    // 不可达（忙循环任务不退出）；防御兜底：若被切回则进 tickless idle。
    berry_sched::idle_loop();
}

/// 忙循环任务 A（Step 5/6 demo）：纯忙转、不打 yield——证 timer 驱动。
/// Step 5：`on_irq` 不 switch → task_a 独占、task_b 饿死（无 `B` 输出）。
/// Step 6：`on_irq` 接 switch → task_a/task_b 在 slice 下交替。
/// 2b-2：arg = 本任务私有页 magic；每次打印节流点经当前 TTBR0 读私有页自证
/// （隔离金丝雀——切错表/泄漏立即 panic 暴露）。
extern "C" fn task_a(arg: usize) -> ! {
    let magic = arg as u64;
    static A_COUNT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
    loop {
        let c = A_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        if c >= A_PRINT_EVERY {
            A_COUNT.store(0, core::sync::atomic::Ordering::Relaxed);
            // SAFETY: 读自身 AS 的私有页（Normal 映射）；错值非安全问题是正确性问题。
            let v = unsafe { core::ptr::read_volatile(aspace::USER_VA as *const u64) };
            assert_eq!(v, magic, "task_a AS 隔离破坏：私有页读到对端内容");
            // G43 拼行（S-2b 副核参与前防 switches= 金丝雀行撕入矩阵字符流）。
            berry_hal_aarch64::kern_lock::with_print_lock(|| {
                berry_hal_aarch64::uart_puts(b"A");
            });
        }
    }
}

/// 忙循环任务 B（同 A，打 `B`）。Step 5 饿死（无切换）；Step 6 被调起。
extern "C" fn task_b(arg: usize) -> ! {
    let magic = arg as u64;
    static B_COUNT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
    loop {
        let c = B_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        if c >= B_PRINT_EVERY {
            B_COUNT.store(0, core::sync::atomic::Ordering::Relaxed);
            // SAFETY: 同 task_a。
            let v = unsafe { core::ptr::read_volatile(aspace::USER_VA as *const u64) };
            assert_eq!(v, magic, "task_b AS 隔离破坏：私有页读到对端内容");
            // G43 拼行（同 task_a）。
            berry_hal_aarch64::kern_lock::with_print_lock(|| {
                berry_hal_aarch64::uart_puts(b"B");
            });
        }
    }
}

/// A/B 的打印节流阈值（迭代次数）。小→刷屏（直观见交替）；大→低噪（soak 时虚拟时间
/// 不被 UART 背压吃掉，switches 计数贴近 wall-clock × SLICE_HZ）。SOAK=1 为低噪 soak。
const A_PRINT_EVERY: u32 = 2_000_000;
const B_PRINT_EVERY: u32 = 2_000_000;
// good 的节流阈值 3-2 起 baked 进用户模板（movz x9, #0x30, lsl #16 ≈ 3.1M）——
// 入口 x0 改传 root Vmar handle（首能力）。

/// task_a/task_b 私有页 magic（2b-2 隔离金丝雀；互异且非零）。
const TASK_A_MAGIC: u64 = 0x00A5_C0DE_AAAA_0001;
const TASK_B_MAGIC: u64 = 0x00B6_C0DE_BBBB_0002;

/// 4-0 睡眠/唤醒验收的等待对象标识（静态地址 = `wake_matching` 的 obj 匹配键，
/// 不透明 token——4-1 起该语义交给 Event/Channel 的 ObjRef 地址）。
static SLEEP_OBJ: u8 = 0;

/// sleeper 双路径全过标志（唤醒环 watchdog 断言的门槛——置位即两条 boot
/// 唤醒记录已落 `telemetry::WAKE_RING`，断言才不赌 boot 时序）。单核 v1
/// Relaxed 足够（读写都在屏蔽窗/单核串行语境；S-2 起口径不变——内核任务
/// 全钉 CPU0，副核 pick 过滤只放行 EL0 钉核任务，读写同核串行保持）。
pub static BLOCK_BOOT_DONE: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// 4-0 sleeper：两条唤醒路径各验一遍，全绿打 `BLOCK ok`。
///
/// - 路径 1（超时自醒 'T'）：纯 sleep 1 slice——醒因必 Timeout 且实际醒来
///   ≥ deadline（tick 粗粒度扫描：到期后最迟一个 slice 被摘）。
/// - 路径 2（条件唤醒 'K'）：挂 SLEEP_OBJ 无限等（deadline=MAX），由 waker 的
///   `wake_matching` 摘醒——醒因必 Signaled（单核屏蔽窗原子：唤醒方只在匹配
///   时摘队，不存在虚假唤醒；醒后重验条件的兜底语义归 4-1 调用方）。
///
/// block_current 契约：内核任务显式 disable → block → 续行后 enable（对称
/// 包络，同 yield_now 模式；SVC 语境无此步——eret 自然解蔽）。S-2 E0 起
/// 直调位点统一升级 klock_enter/klock_exit（F31，草案 §2.3——包络含取
/// KERN_LOCK + Task 位，block_current 内舞蹈按位放锁/重取）。
extern "C" fn task_sleeper(_arg: usize) -> ! {
    use berry_sched::WakeReason;
    let slice = berry_hal_aarch64::timer::slice_cycles();
    // 路径 1：超时自醒。
    berry_hal_aarch64::kern_lock::klock_enter();
    let t0 = berry_hal_aarch64::timer::cntvct();
    let r = berry_sched::block_current(0, 0, t0 + slice);
    berry_hal_aarch64::kern_lock::klock_exit();
    let elapsed = berry_hal_aarch64::timer::cntvct() - t0;
    assert!(r == WakeReason::Timeout, "sleeper: 超时路径醒因非 Timeout");
    assert!(elapsed >= slice, "sleeper: 超时醒来早于 deadline");
    berry_hal_aarch64::uart_puts(b"T");
    // 路径 2：条件唤醒（无限等——无哨兵，deadline=MAX 等价）。**只等用户位**
    // 1<<33（内核永不置位）——4-3b 起 `donor_edge` 的不变量是「请求位含内核
    // 派生位（READABLE/WRITABLE）⟹ wait_obj 是真 channel ObjRef」：SLEEP_OBJ
    // 是不透明 token（u8 静态，非对象），带派生位睡它 = 让 PI 图解引用垃圾
    // 指针（曾以 !0 触发 pi.rs 对齐 panic 实证）。
    berry_hal_aarch64::kern_lock::klock_enter();
    let r2 = berry_sched::block_current(&SLEEP_OBJ as *const u8 as usize, 1 << 33, u64::MAX);
    berry_hal_aarch64::kern_lock::klock_exit();
    assert!(r2 == WakeReason::Signaled, "sleeper: 条件路径醒因非 Signaled");
    // 双路径全过——置门槛放行 watchdog 唤醒环断言（此刻两条 boot 唤醒记录必
    // 已落环：钩子在唤醒时刻发射，先于 sleeper 续行到这里）。
    BLOCK_BOOT_DONE.store(true, core::sync::atomic::Ordering::Relaxed);
    berry_hal_aarch64::uart_puts(b"K BLOCK ok\n");
    berry_sched::exit_current()
}

/// 4-0 waker：轮询 `wake_matching(SLEEP_OBJ, 1<<33)` 直到 sleeper 双路径全过。
/// 未挂上时零副作用——单核屏蔽窗原子保证无丢唤醒：要么本循环先扫（sleeper
/// 稍后挂上，下轮摘到）、要么 sleeper 已挂（本轮即摘）。轮询体 IRQ 基本解蔽
/// → tick 正常抢占 → sleeper 得以上 CPU 挂队。（置位集与 sleeper 的请求位
/// 同步收窄到用户位 1<<33——理由见路径 2 注释。）
///
/// **收工判据 = sleeper 侧 `BLOCK_BOOT_DONE`**（4-5a 修正）：4-2b 起
/// `wake_matching` 对 `WAIT_ANY` 哨兵睡眠者（wait_many）「任何对象、任何
/// 非零位」都命中——ponger 稳态全程挂哨兵，本循环的定向轮询会误摘它一票，
/// 返回计数混入哨兵误醒（可能 1 也可能 ≥2），不再可信。改看 sleeper 置的
/// 完成标志：定向唤醒语义不变（SLEEP_OBJ+1<<33 的精确匹配只可能来自本循
/// 环）；被误醒的哨兵者由其重验循环自行回睡（4-2b 设计内行为，无害）。
extern "C" fn task_waker(_arg: usize) -> ! {
    while !BLOCK_BOOT_DONE.load(core::sync::atomic::Ordering::Relaxed) {
        berry_hal_aarch64::kern_lock::klock_enter();
        berry_sched::wake_matching(&SLEEP_OBJ as *const u8 as usize, 1 << 33);
        berry_hal_aarch64::kern_lock::klock_exit();
        // 降频小忙转（可被 tick 抢占）。
        for _ in 0..100_000 {
            core::hint::spin_loop();
        }
    }
    berry_sched::exit_current()
}

/// 4-1 waker2 的 EL1 真 SVC 往返（ktest `svc6` 同款裁 4 参）：x0=nr（进）/
/// ret（出）、x1..x3=参。走 current-EL SVC 向量 → HAL dispatch → 语义层——
/// 全真 syscall 链（非直调 handler）。
fn svc4(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    let ret: u64;
    // SAFETY: 同步异常；dispatch 只回写 x0。
    unsafe {
        core::arch::asm!(
            "svc #0",
            inout("x0") nr => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            options(nostack, preserves_flags),
        );
    }
    ret as i64
}

/// 4-1 waker2：boot 期睡 3 slice 后经**真 SVC** 对内核侧表 handle 发
/// `object_signal(h, 0, U4)`——唤醒链全真：EL1 SVC 入口 → 语义层 signal →
/// `wake_matching` → waiter 醒来重验命中。再睡 1 slice 让 waiter 跑完三路进
/// 稳态，打关卡标记 `WAIT ok`（smoke 第 15）。waiter 断链被杀 → 杀数超 1 →
/// watchdog 错杀断言拦下（无需本任务二次确认）。
/// P5-0 位拆分（P4 收官审计修复）：signal 用握手专用位 U4=1<<38 且 waiter
/// 三处清位豁免之——本信号相对 waiter 三路是**任意相位**（3 slice 只是 boot
/// 常态时延，非契约；PI 错捐等放大器可让相位漂移，早到/迟到两相位都须过，
/// 见 user.rs waiter 头注）。
extern "C" fn task_waker2(arg: usize) -> ! {
    use crate::syscall::{NR_CLOCK_READ, NR_OBJECT_SIGNAL, NR_THREAD_SLEEP};
    let h_kernel = arg as u32;
    // SAFETY: 只读 CNTFRQ / slice 常量（timer::init 后恒有效）。
    let freq = berry_hal_aarch64::timer::cntfrq();
    assert!(freq != 0, "waker2: CNTFRQ 异常");
    let slice_ns = berry_hal_aarch64::timer::slice_cycles() * 1_000_000_000 / freq;
    let mut now: u64 = 0;
    // SAFETY: svc 同步异常；now 为本任务私有栈变量（内核高半，copy_out 直拷）。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "waker2: clock_read");
    assert_eq!(
        svc4(NR_THREAD_SLEEP as u64, now + 3 * slice_ns, 0, 0),
        0,
        "waker2: thread_sleep 3 slice"
    );
    // 置位握手专用位 U4=1<<38（4-2 位空间起 Event 用户位在高 32 半；waiter
    // 清位豁免 U4——本信号任意相位无竞速，见 user.rs waiter 头注）。
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, h_kernel as u64, 0, 1 << 38),
        0,
        "waker2: object_signal U4"
    );
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "waker2: clock_read 2");
    assert_eq!(
        svc4(NR_THREAD_SLEEP as u64, now + slice_ns, 0, 0),
        0,
        "waker2: thread_sleep 1 slice（让 waiter 收尾）"
    );
    berry_hal_aarch64::uart_puts(b"W WAIT ok\n");
    berry_sched::exit_current()
}

/// 4-3b 三任务的 EL1 真 SVC 往返（waker2 `svc4` 同款扩到 6 参）：channel_send/
/// recv（5 参）与 object_wait_one（4 参）的载体，x0=nr（进）/ret（出）。
fn svc6(nr: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> i64 {
    let ret: u64;
    // SAFETY: 同步异常；dispatch 只回写 x0（ktest `svc6` 同款）。
    unsafe {
        core::arch::asm!(
            "svc #0",
            inout("x0") nr => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            in("x5") a5,
            in("x6") a6,
            options(nostack, preserves_flags),
        );
    }
    ret as i64
}

/// 4-3b pi_server（prio 0，受捐方）：先挂 Event E（用户位 1<<35，100s 帽）睡
/// ——client 的 8 条 PING 此刻已在 endS 队列堆着（server 没人 recv），本任务
/// 的「处理慢」由睡 E 模拟（下游依赖）。被 probe 的 signal 唤醒后醒来 recv
/// ×8 腾位（第一条即唤醒 client 的 WRITABLE）→ exit。醒来时 `pi_inherited`
/// 冻结随行（「server 处理消息期间持有」的 v1 形态——冻结面由 ktest J 段
/// 机器断言，本任务只走功能闭环）。
extern "C" fn task_pi_server(arg: usize) -> ! {
    use crate::syscall::{NR_CHANNEL_RECV, NR_CLOCK_READ, NR_OBJECT_WAIT_ONE};
    let h_end_s = arg as u32;
    let h_ev = (arg >> 32) as u32;
    let mut now: u64 = 0;
    // SAFETY: svc 同步异常；now 为本任务私有栈变量（内核高半，copy_out 直拷）。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "pi_server: clock_read");
    let mut observed = 0u64;
    let pobs = core::ptr::addr_of_mut!(observed) as u64;
    assert_eq!(
        svc6(NR_OBJECT_WAIT_ONE as u64, h_ev as u64, 1 << 35,
             now + 100 * 1_000_000_000, pobs, 0, 0),
        0,
        "pi_server: wait E（被 probe signal 唤醒）"
    );
    // 醒来（冻结 boost 随行）：清空 endS 队列——第一条 recv 即 wake client。
    let mut buf = [0u8; 8];
    let mut hs = [0u32; 4];
    let (mut n, mut nh): (u64, u64);
    for i in 0..8 {
        // 容量 in-out：每次 recv 前重置（实际值出参回写）。
        n = 8;
        nh = 4;
        assert_eq!(
            svc6(NR_CHANNEL_RECV as u64, h_end_s as u64, buf.as_mut_ptr() as u64,
                 core::ptr::addr_of_mut!(n) as u64, hs.as_mut_ptr() as u64,
                 core::ptr::addr_of_mut!(nh) as u64, 0),
            0,
            "pi_server: recv 第 {} 条（腾位）", i + 1
        );
    }
    berry_sched::exit_current()
}

/// 4-3b pi_probe（prio 0，验收司机）：睡 5 slice 等 server/client 双双挂队
/// （client(32) 先跑睡 send-wait、server 随后睡 E——block 钩子重推成边），醒
/// 来遍历 SLEEPQ 断言捐赠字段（server pi_inherited==32 且 eff==32；client 自身
/// eff==32）→ signal E 放行 → 打 `PI ok`（smoke 第 18 标记）→ exit。字段断言
/// 即机器关卡；「醒来冻结随行 + 4-3a 严格优先」的行为面交 ktest J 段与既有
/// POLICY 面（G 卡用例的延迟上界验收留正式关卡）。
extern "C" fn task_pi_probe(arg: usize) -> ! {
    use crate::syscall::{NR_CLOCK_READ, NR_OBJECT_SIGNAL, NR_THREAD_SLEEP};
    let h_ev = arg as u32;
    // SAFETY: 同 pi_server。
    let freq = berry_hal_aarch64::timer::cntfrq();
    let slice_ns = berry_hal_aarch64::timer::slice_cycles() * 1_000_000_000 / freq;
    let mut now: u64 = 0;
    // SAFETY: 同 pi_server。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "pi_probe: clock_read");
    assert_eq!(
        svc4(NR_THREAD_SLEEP as u64, now + 5 * slice_ns, 0, 0),
        0,
        "pi_probe: thread_sleep 5 slice（等双睡者挂队）"
    );
    // 断言捐赠图：server（睡 E）受捐 32；client（睡 endC:WRITABLE）自身 32。
    // 契约包络（复盘雷 2 残留诊断）：for_each_sleeper 须 IRQ 屏蔽窗内调用
    //（tick 超时摘队会动睡眠链接——见 sched.rs 契约注释）；thread_sleep 的
    // SVC eret 后 IRQ 已解蔽，故显式包络（ai_probe 同款）。S-2 E0 起包络
    // 升级 klock_enter/klock_exit（F36，草案 §2.3——锁窗等价口径，B16）。
    let (mut saw_server, mut saw_client) = (false, false);
    berry_hal_aarch64::kern_lock::klock_enter();
    berry_sched::for_each_sleeper(|t| {
        if t.name == b"pi_server" {
            saw_server = true;
            assert_eq!(t.pi_inherited, 32, "pi_probe: server 须受捐到 32");
            assert_eq!(t.prio_effective, 32, "pi_probe: server eff 须被抬到 32");
        }
        if t.name == b"pi_client" {
            saw_client = true;
            assert_eq!(t.prio_effective, 32, "pi_probe: client(donor) 自身 eff");
        }
    });
    berry_hal_aarch64::kern_lock::klock_exit();
    assert!(saw_server && saw_client, "pi_probe: 双睡眠者未同时在线（时序未达）");
    // 放行：signal E → server 醒（冻结 boost）→ recv 腾位 → client 醒。
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, h_ev as u64, 0, 1 << 35),
        0,
        "pi_probe: signal E 放行"
    );
    berry_hal_aarch64::uart_puts(b"I PI ok\n");
    berry_sched::exit_current()
}

/// 4-3b pi_client（prio 32，donor）：send×8 灌满 endS 队列（每条成功即
/// wake peer READABLE——无人等，无害）→ 第 9 条 -EBUSY → wait_one(endC,
/// WRITABLE, 100s 帽) 睡——**block 钩子触发 PI 全图重推**（server 尚未睡 E
/// 时无边；server 睡 E 的 block 重推才闭合边——两种时序都收敛到同一图）。
/// server recv 腾位唤醒后：补发一条须成功（腾位自证）→ exit。
extern "C" fn task_pi_client(arg: usize) -> ! {
    use crate::handle::ERR_BUSY;
    use crate::syscall::{NR_CHANNEL_SEND, NR_CLOCK_READ, NR_OBJECT_WAIT_ONE};
    let h_end_c = arg as u32;
    let msg = *b"PI!\0";
    let mut saw_full = false;
    for i in 0..9 {
        let r = svc6(NR_CHANNEL_SEND as u64, h_end_c as u64,
                     msg.as_ptr() as u64, 4, 0, 0, 0);
        if r == ERR_BUSY as i64 {
            saw_full = true; // 第 i+1 条撞满（i==8 为正常路径；早满也收敛）
            break;
        }
        assert_eq!(r, 0, "pi_client: send 第 {} 条", i + 1);
    }
    assert!(saw_full, "pi_client: 9 条未撞满（队列深 8——异常）");
    let mut now: u64 = 0;
    // SAFETY: 同 pi_server。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "pi_client: clock_read");
    let mut observed = 0u64;
    let pobs = core::ptr::addr_of_mut!(observed) as u64;
    assert_eq!(
        svc6(NR_OBJECT_WAIT_ONE as u64, h_end_c as u64,
             crate::handle::SIGNAL_WRITABLE, now + 100 * 1_000_000_000, pobs, 0, 0),
        0,
        "pi_client: wait WRITABLE（server recv 腾位唤醒）"
    );
    assert!(
        observed & crate::handle::SIGNAL_WRITABLE != 0,
        "pi_client: observed 须含 WRITABLE"
    );
    // 腾位自证：补发一条须成功。
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, h_end_c as u64, msg.as_ptr() as u64, 4, 0, 0, 0),
        0,
        "pi_client: 腾位后补发"
    );
    berry_sched::exit_current()
}

// ---- 4-5a console 服务（首个服务循环，`IPC设计.md` Phase 4 拆步表 4-5 前半）----

/// console 已落串口字节数——4-5a 起 watchdog 金丝雀（替 0xFF 调试槽的
/// USER_PUTCNT：打印异步化后「发出」与「落串口」分离，活证取**服务侧**计数）。
static CONSOLE_DRAINED: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// 读 console 服务排水计数（watchdog 用）。
pub fn console_drained() -> u64 {
    CONSOLE_DRAINED.load(core::sync::atomic::Ordering::Relaxed)
}

// ---- P5 A-1：console 输入面（内核 RX 中断的等待/消费侧）----

/// RX Event 的用户位（占用登记见 [`USER_SIGNAL_BITS`]——1<<32..1<<38 已满，
/// 顺延 1<<39）。IRQ 侧置位（`rx_irq_notify`）、console 侧包络内清位
/// （`rx_ev_clear`）。
const RX_EV_BIT: u64 = 1 << 39;

/// RX Event handle 静态传递槽（console 任务读；spawn arg 已满走静态槽——
/// INTENT_H 同款惯例；写点 = boot 接线段，先于一切任务）。
static mut RX_EV_H: u32 = 0;
/// RX Event 的 ObjRef（`rx_irq_notify` 置位 / `rx_ev_clear` 清位共用；boot
/// 写入后不变，boot 侧 retain 钉住永不 release——IRQ 语境无 UAF 面）。
static mut RX_EV_REF: Option<core::ptr::NonNull<handle::ObjRef>> = None;

/// 读 RX Event handle 静态槽（console 任务侧）。
fn rx_ev_h() -> u32 {
    // SAFETY: 只读；全部写点先于任务启动（boot 顺序）。
    unsafe { *core::ptr::addr_of!(RX_EV_H) }
}

/// bsh stdin 发端（endK2）handle 静态槽（console 任务读；spawn arg 已满走
/// 静态槽——RX_EV_H 同款惯例；写点 = boot 接线段，先于一切任务）。P5 A-2。
static mut BSH_IN_H: u32 = 0;

/// 读 endK2 handle 静态槽（console 任务侧）。
fn bsh_in_h() -> u32 {
    // SAFETY: 只读；全部写点先于任务启动（boot 顺序）。
    unsafe { *core::ptr::addr_of!(BSH_IN_H) }
}

/// fb 上屏管道 console 端（endK_fb）handle 静态槽（console 任务读；spawn
/// arg 已满走静态槽——RX_EV_H/BSH_IN_H 同款惯例；写点 = boot fb 授能块，
/// 先于一切任务；0 = 非 EFI-GOP 路（-kernel 五跑零插表），console 跳过
/// fan-out）。GUI-1 段②。
static mut FB_PIPE_H: u32 = 0;

/// 读 endK_fb handle 静态槽（console 任务侧）。
fn fb_pipe_h() -> u32 {
    // SAFETY: 只读；全部写点先于任务启动（boot 顺序）。
    unsafe { *core::ptr::addr_of!(FB_PIPE_H) }
}

/// 「RX 到数据」通知（HAL `exception_irq` → `uart::rx_irq` 经槽调；IRQ 上下文
/// PSTATE.I=1）。置位 RX Event（用户半位）+ `wake_matching`（匹配键 = ObjRef
/// 地址，同 4-1 `sys_object_signal` 唤醒路径；console 的 wait_many 挂
/// WAIT_ANY 哨兵——任何非零 set 命中，醒来重验定夺）。**不立即抢占**（v1
/// `wake_matching` 契约——醒者等 tick/yield，交互延迟 ≤ 1 slice）；也不可
/// 在 IRQ 语境碰 fast_wake/try_preempt。写 Event 与遥测环同纪律：零分配、
/// 纯指针写字段。S-2 E0 起 `ev.signal + wake_matching` 段包 KERN_LOCK
/// （E27，草案 §2.4：IRQ 上下文无 Task 位、无 DAIF 操作——同 on_irq_tick
/// 的 `klock_acquire_raw` 目标形态，kernel 侧直接调 arch 原语）。
unsafe extern "C" fn rx_irq_notify() {
    // SAFETY: IRQ 上下文（单核屏蔽窗串行）；RX_EV_REF boot 后不变。
    let p = unsafe { *core::ptr::addr_of!(RX_EV_REF) }
        .expect("rx_irq_notify: RX_EV_REF 未初始化（boot 接线缺失）");
    berry_hal_aarch64::kern_lock::kern_acquire();
    unsafe {
        if let Some(ev) = (*p.as_ptr()).obj.as_event_mut() {
            ev.signal(0, RX_EV_BIT);
        }
        berry_sched::wake_matching(p.as_ptr() as usize, RX_EV_BIT);
    }
    berry_hal_aarch64::kern_lock::kern_release();
}

/// 清 RX Event 位（console 消费包络内调）。位由 IRQ 置、由消费侧清——与
/// waiter U4 握手位同款「专用位 + 豁免清位」模式：早到/晚到任意相位皆成立
/// （IRQ 在包络内被屏蔽，位只可能在包络外被置——置后 console 下轮 wait 的
/// poll-first 必命中，无丢唤醒）。
///
/// # Safety
/// 须在 `disable_irq` 包络内调（与 RX IRQ 生产者串行）；RX_EV_REF 不变。
unsafe fn rx_ev_clear() {
    // SAFETY: 调用方保证屏蔽包络 + 引用不变。
    let p = unsafe { *core::ptr::addr_of!(RX_EV_REF) }
        .expect("rx_ev_clear: RX_EV_REF 未初始化");
    unsafe {
        if let Some(ev) = (*p.as_ptr()).obj.as_event_mut() {
            ev.signal(RX_EV_BIT, 0);
        }
    }
}

// ---- P5 v0.5：console mini-readline（历史环 + 召回 + Ctrl-U）——**C-4 已
// 退役**：行编辑归属下沉 bsh（Tab 补全需 rootfs/fs 知识——用户态；P5 留档
// 的「编辑会话归属」裁定兑现，shell设计 §2.4）。内核删历史环/ESC 状态机/
// 退格/Ctrl-U/echo 全套约 150 行，console 退化 = 逐字节透传 bsh + 原始行
// 观察 + 本端 drain——见 [`task_console`]。编辑器等价实现（std 版 + Tab
// 补全）在 user/bins/bsh/src/editor.rs，语义逐字对齐本版（历史环 8 条
// newest-first、非空且≠newest 才入环、usize::MAX=live 游标、浏览态编辑
// 已载入行、下翻到 live 清草稿、ESC 状态机三分支含非 final 复位落下）。----


/// 4-5a console 服务任务：**首个服务循环**——wait_many [endK:READABLE, 哑
/// Event:1<<32]（任一命中；10s 心跳帽，空等超时非错误、回环重等）→ drain
/// recv 排空本端队列 → 逐字节 UART + 计数。全真 SVC 链（EL1 入口与 EL0 同权，
/// 3-1）；消息纯字节、不携带 handle（recv cap 0）。opt-out 通道 = 固定优先级
/// 系统服务（recv 不结算 intent 借入——4-3c 五件套之 4 的常驻消费面）。
///
/// **为何 wait_many 而非 wait_one(endK)**：多对象等待者挂 WAIT_ANY 哨兵 →
/// `pi::donor_edge` 返回 None（4-2b「v2 收口」）——console 退出 v1 PI donor
/// 面，否则（共享内核侧表的受捐粒度下）它把受捐额级联给全体持发端用户、
/// 冻结 boost + 4-3a 严格优先 = 永不入睡任务的饥饿死锁（见 boot spawn 处
/// 完整因果链）。哑 Event 永不 signal → 命中必 idx∈{0,2}（idx==1 即自陷被
/// watchdog 错杀断言拦下）。console 是常驻服务**不退出**（与 sleeper/waker
/// 等验收任务不同）；4-5b svcmgr 落地后用户态服务与此同构。
///
/// **P5 A-1 / C-4 输入面**：wait_many 扩第三项 [RX Event:1<<39]——内核 RX
/// 中断（SPI 33）drain UART FIFO 进 HAL 静态环后经通知槽置位唤醒本任务。消费 =
/// `disable_irq` 包络内 `rx_pop_all`（搬空环）+ `rx_ev_clear`（清位）——
/// 「位 0 ⟺ 环空」不变量在包络内闭环（包络内**无 SVC/阻塞**：屏蔽窗内切走
/// 是违约）。原始行观察（可打印序列、\n/\r 断行）仅供 token 判定：首条恰为
/// `rx` 的行直接 uart 打 `RX ok n=… lost=… fdrop=`（smoke 标记，**直接
/// uart 打**——console 自己就是排水者，不能再给自己发消息）。
///
/// **P5 A-2 / C-4 交互面**：wait_many 再扩第四项 [endK2:WRITABLE]（bsh
/// stdin 发端；队列腾位即吸收环排水窗）。**C-4 起逐字节透传**：编辑归属
/// 下沉 bsh（echo/退格/历史/Tab 补全全在 bsh——console 无 rootfs/fs 知识，
/// 微内核纪律），console 对每个收到的字节（含 \n/\r/DEL/ESC 序列字节）
/// `send(endK2)` 一条 1B 消息。**C-4r 吸收环（回归修复，2026-10-28）**：
/// 首版单字节 pending 的吸收面只有「8 深队列 + 1B」≈ 4.5s 打字流——bsh
/// 忙窗（`run fail` spawn+reap 数秒，TCG 宿主高载更长）内后续字节被覆盖
/// 丢弃（smoke RX 相段 d 零间歇三行注入实录两连败，f42t 缺失；`pop_all`
/// 突发多字节更只存活末字节——循环内零发送尝试）。修法 = 128B 字节级 FIFO
/// 环（纯缓冲，console 仍无行/编辑语义——归属裁定不破）：环满丢新计数
/// `fdrop=`（bsh 缺席/死面诊断位，128B ≈ 64s 打字流，健康路径恒 0）。
/// token 行也照常透传（bsh 收 `rx\n` 走 not found 应答——编辑归属下沉后
/// console 无「吞行」语义，转发面无条件字节级）。n=4 仍 wait_many（WAIT_ANY
/// 哨兵 + 私有表——PI 双保险纪律不因 C-4 破；恒满足项条件化纪律同前——环
/// 空时 endK2-WRITABLE 不入列防 drain 相位钉死）。
extern "C" fn task_console(arg: usize) -> ! {
    use crate::handle::{ERR_AGAIN, ERR_BUSY, ERR_TIMEDOUT, SIGNAL_READABLE, SIGNAL_WRITABLE};
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_CLOCK_READ, NR_OBJECT_WAIT_MANY};
    let h_end_k = arg as u32;
    let h_dummy = (arg >> 32) as u32;
    let h_rx = rx_ev_h();
    let h_bsh = bsh_in_h();
    // GUI-1 段②：fb 上屏端（0 = 非 EFI-GOP 路，fan-out 全跳过）。
    let h_fb = fb_pipe_h();
    // wait_many 四数组（栈上、EL1 内核高半——copy_from_user 的 EL1 分支收）。
    let hs = [h_end_k, h_dummy, h_rx, h_bsh];
    let sigs = [SIGNAL_READABLE, 1u64 << 32, RX_EV_BIT, SIGNAL_WRITABLE];
    let mut now: u64 = 0;
    // SAFETY: svc 同步异常；栈变量私有（内核高半，copy 直拷）。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    let (mut idx, mut observed): (u64, u64) = (0, 0);
    let (pidx, pobs) = (
        core::ptr::addr_of_mut!(idx) as u64,
        core::ptr::addr_of_mut!(observed) as u64,
    );
    let mut buf = [0u8; 256];
    // P5 A-1：RX 消费面——搬出缓冲（= 环容量 → 包络内必搬空）、计数。
    let mut rxbuf = [0u8; berry_hal_aarch64::uart::RX_RING_SIZE];
    // C-4：原始行观察窗（token 判定专用——96B 帽钉满即不匹配；可打印
    // ASCII 之外（DEL/ESC/控制字节）不进观察面——编辑语义在 bsh）。
    let mut raw = [0u8; 96];
    let mut rlen = 0usize;
    // C-4r：转发吸收环（128B 字节级 FIFO——逐字节透传的吸收面回归修复，
    // 见任务头注）。fhead=出位、ftail=入位、flen=在环字节数。
    let mut fring = [0u8; 128];
    let (mut fhead, mut ftail, mut flen) = (0usize, 0usize, 0usize);
    let mut fwd_drop: u64 = 0;
    // GUI-1 段②：fb fan-out 丢弃计数（guifbd 未起/慢 = 有界丢弃；RX ok
    // 行尾诊断位，正常 EFI 链稳态应恒 0——矩阵打印节流下 8 深队列+秒级
    // 排水不构成背压面）。
    let mut fb_drop: u64 = 0;
    let mut rx_total: u64 = 0;
    let mut rx_ok_done = false;
    let (mut n, mut nh): (u64, u64);
    loop {
        assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "console: clock_read");
        // 第 4 项（endK2 WRITABLE）**只在吸收环非空时入列**——bsh 缺席/无键盘
        // 输入时 endB2 队列恒空 = 该项恒满足 → wait_many 立即返 idx==3 → console
        // 永不入睡：既违 tickless 纪律，更把 drain 相位钉死在 RR 环上（环序固定
        // 后，填槽序末位的自旋生产者结构性饿死——A-2 实踩，svcmgr send_spin
        // 「SVC ok」每片只见满队）。环空 = 3 项真睡（A-1 形态，相位随发送方
        // 唤醒旋转）；环非空 = 4 项入列，idx==3 即排水窗。
        let n_items: u64 = if flen > 0 { 4 } else { 3 };
        let r = svc6(
            NR_OBJECT_WAIT_MANY as u64,
            hs.as_ptr() as u64,
            sigs.as_ptr() as u64,
            n_items,
            now + 10 * 1_000_000_000,
            pidx,
            pobs,
        );
        assert!(
            r == 0 || r == ERR_TIMEDOUT as i64,
            "console: wait_many 异常 ret={}",
            r
        );
        if r == 0 {
            // 哑 Event（idx==1）永不置位 = 命中它即接线错乱；endK(0)/RX(2)/
            // endK2-WRITABLE(3) 合法（idx==3 = bsh 队列腾位——pending 重发窗）。
            assert!(
                idx == 0 || idx == 2 || idx == 3,
                "console: wait 命中须 endK(0)/RX(2)/endK2(3)，得 {}",
                idx
            );
        }
        // P5 A-1：RX 消费——包络内搬空环 + 清位（不变量闭环）；包络内无
        // SVC/阻塞。**每次唤醒无条件执行**（不只 RX 命中时）：wait 返回与
        // IRQ 置位间的任意交错（早到/迟到/心跳超时顺带）都收敛到「环搬空 +
        // 位清零」，无丢唤醒窗口。
        berry_hal_aarch64::disable_irq();
        let nrx = berry_hal_aarch64::uart::rx_pop_all(&mut rxbuf);
        // SAFETY: 包络内与 RX IRQ 生产者串行；RX_EV_REF boot 后不变。
        unsafe { rx_ev_clear() };
        berry_hal_aarch64::enable_irq();
        for i in 0..nrx {
            let b = rxbuf[i];
            rx_total += 1;
            // C-4 原始行观察（token 判定）：可打印 ASCII 入行、\n/\r 断行
            // 判定 + 清行、控制字节/DEL/ESC 不进（编辑语义在 bsh——观察面
            // 只看可打印序列，smoke `rx\n` 注入与编辑时代等价过）。
            if b == b'\n' || b == b'\r' {
                if !rx_ok_done && rlen == 2 && raw[..2] == *b"rx" {
                    rx_ok_done = true;
                    // 直接 uart（console 即排水者，不走自家通道）；标记条件 =
                    // 收到完整行 token（状态谓词，非注入即达的回声）。
                    berry_hal_aarch64::uart_puts(b"RX ok n=");
                    berry_hal_aarch64::uart_put_dec(rx_total);
                    berry_hal_aarch64::uart_puts(b" lost=");
                    berry_hal_aarch64::uart_put_dec(
                        berry_hal_aarch64::uart::rx_lost() as u64,
                    );
                    // A-2：转发丢弃计数（bsh 断供面诊断位；正常恒 0——smoke
                    // 只 grep "RX ok" 前缀，行尾追加零扰动）。
                    berry_hal_aarch64::uart_puts(b" fdrop=");
                    berry_hal_aarch64::uart_put_dec(fwd_drop);
                    // GUI-1 段②：fb fan-out 丢弃计数（同款行尾诊断位）。
                    berry_hal_aarch64::uart_puts(b" fbdrop=");
                    berry_hal_aarch64::uart_put_dec(fb_drop);
                    berry_hal_aarch64::uart_puts(b"\n");
                }
                rlen = 0;
            } else if b >= 0x20 && b < 0x7f && rlen < raw.len() {
                raw[rlen] = b;
                rlen += 1;
            }
            // C-4 逐字节透传（无条件——含断行/控制字节：行编辑/echo 归 bsh）
            // = 进转发吸收环；环满丢新计数（顺序保持：老字节先走）。
            if flen == fring.len() {
                fwd_drop += 1;
            } else {
                fring[ftail] = b;
                ftail = (ftail + 1) % fring.len();
                flen += 1;
            }
        }

        // C-4r：吸收环排水（bsh stdin）——非空即逐字节试发；满 -EBUSY 停排
        // （余量留环，等 idx==3 WRITABLE / 心跳唤醒续排；bsh 缺席/死 = 环满
        // 后丢新，见 fwd_drop）。其它错码 = 接线错乱（boot 定值 + SEND right
        // 在手），panic 兜底。
        while flen > 0 {
            let rs = svc6(
                NR_CHANNEL_SEND as u64,
                h_bsh as u64,
                core::ptr::addr_of!(fring[fhead]) as u64,
                1,
                0,
                0,
                0,
            );
            assert!(
                rs == 0 || rs == ERR_BUSY as i64,
                "console: bsh-stdin send ret={}",
                rs
            );
            if rs == ERR_BUSY as i64 {
                break;
            }
            fhead = (fhead + 1) % fring.len();
            flen -= 1;
        }
        // drain：排空本端队列（发送侧重试 + 深度 8 → 批量有界）。
        loop {
            n = 256;
            nh = 0;
            let r2 = svc6(
                NR_CHANNEL_RECV as u64,
                h_end_k as u64,
                buf.as_mut_ptr() as u64,
                core::ptr::addr_of_mut!(n) as u64,
                0,
                core::ptr::addr_of_mut!(nh) as u64,
                0,
            );
            if r2 == ERR_AGAIN as i64 {
                break; // 排空——回外层阻塞等
            }
            assert_eq!(r2, 0, "console: recv 异常");
            for i in 0..n as usize {
                berry_hal_aarch64::uart_putc(buf[i]);
                CONSOLE_DRAINED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
            // GUI-1 段②：fb 上屏 fan-out——整段 best-effort 一发（n ≤ 256B
            // = channel 内联额度内；满 -EBUSY/-EAGAIN → 丢弃 + fb_drop 计数
            // 不重试不阻塞，保串口主路——guifbd 未起（boot 早期）/慢的输出
            // 丢 = 既定语义）。零长 EOF 消息不转发（guifbd 无收口语义，省
            // 队列槽）。非满错码 = 接线错乱（boot 定值 + SEND right 在手），
            // panic 兜底。
            if h_fb != 0 && n > 0 {
                let rf = svc6(
                    NR_CHANNEL_SEND as u64,
                    h_fb as u64,
                    buf.as_ptr() as u64,
                    n,
                    0,
                    0,
                    0,
                );
                assert!(
                    rf == 0 || rf == ERR_BUSY as i64 || rf == ERR_AGAIN as i64,
                    "console: fb send ret={}",
                    rf
                );
                if rf != 0 {
                    fb_drop += 1;
                }
            }
        }
    }
}

// ---- 4-3c intent 验收六任务（全真 SVC；ADR #42 五件套，`berry_sched::intent`）----

/// 4-3c handle 传递槽（spawn 只有单 usize 参——多 handle 走静态槽传递）。
/// boot 在 spawn 前写入、先于一切任务读（单核 IRQ 屏蔽窗 = 无竞态）；槽位
/// 表见 main 的 4-3c 段注释。
static mut INTENT_H: [u32; 12] = [0; 12];

/// 读 4-3c 传递槽（任务侧）。
fn it_h(i: usize) -> u32 {
    // SAFETY: 只读；全部写点先于任务启动（boot 顺序）。
    unsafe { *core::ptr::addr_of!(INTENT_H).cast::<u32>().add(i) }
}

/// it_driver（declared INTERACTIVE 0x02，首手）：三发——CH1 借出 0x02 给
/// mid、CH3 opt-out 通道照发（拒借在收方结算侧）、CH5 借出 0x02 给 slow。
/// driver 先入队（spawn FIFO）→ 收方 recv 必不空。发完即退。
extern "C" fn task_intent_driver(_arg: usize) -> ! {
    use crate::syscall::NR_CHANNEL_SEND;
    let msg = *b"ITK!\0\0\0\0";
    for &slot in &[0usize, 4, 9] {
        assert_eq!(
            svc6(NR_CHANNEL_SEND as u64, it_h(slot) as u64,
                 msg.as_ptr() as u64, 8, 0, 0, 0),
            0,
            "it_driver: send 槽 {} 须成功（driver 先于收方入队）", slot
        );
    }
    berry_sched::exit_current()
}

/// it_mid（declared BACKGROUND 0x04）：recv CH1 借入 0x02（hop=0 结算）→
/// send CH2 借入转发（快照 hop+1/acc+持有段——sinkA 的 hop==1 即本跳活证）→
/// recv CH3（opt-out 通道：不结算，回落 declared 0x04，含清掉他途借入）。
/// 此后纯睡不醒（deadline=MAX），留 SLEEPQ 供 probe 断言。
extern "C" fn task_intent_mid(_arg: usize) -> ! {
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND};
    let mut buf = [0u8; 8];
    let (mut n, mut nh): (u64, u64) = (8, 0);
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(1) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_mid: recv CH1（借入 0x02）"
    );
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, it_h(2) as u64, buf.as_ptr() as u64, 8, 0, 0, 0),
        0,
        "it_mid: 转发 CH2（借入快照 hop+1）"
    );
    n = 8; // 容量 in-out 每次 recv 前重置
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(5) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_mid: recv CH3（opt-out 拒借回落）"
    );
    // 契约包络（复盘雷 2 残留诊断）：block_current 须 IRQ 屏蔽窗内调用（SVC
    // 语境天然满足；内核任务显式 disable——sched.rs 契约）。裸调时入 SLEEPQ→
    // 摘环→置 Blocked→切走 各步间可被 tick 闯入（身份错乱/Blocked 覆写等窗口）。
    // 本睡 MAX 帽永不醒（obj=0 唤不醒），无续行侧 enable 面。S-2 E0 起包络
    // 升级 klock_enter（F32，草案 §2.3）：睡前放锁由 block_current 舞蹈自动
    //（Task 位分诊），进锁窗只为 enqueue——本睡永不归，klock_exit 不可达。
    berry_hal_aarch64::kern_lock::klock_enter();
    berry_sched::block_current(0, 0, u64::MAX); // 纯睡永不醒：留 SLEEPQ 供 probe 断言
    unreachable!("it_mid: u64::MAX 不到期")
}

/// it_sinkA（declared UTILITY 0x03）：recv CH2——mid 借入转发的消息
///（intent 0x02 / hop 1 / acc>0）结算借入生效。此后纯睡，字段定格供 probe
/// 断言（eff==0x02、hop==1）。
extern "C" fn task_intent_sink_a(_arg: usize) -> ! {
    use crate::syscall::NR_CHANNEL_RECV;
    let mut buf = [0u8; 8];
    let (mut n, mut nh): (u64, u64) = (8, 0);
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(3) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_sinkA: recv CH2（两跳链尾）"
    );
    // 契约包络同 it_mid（block_current 屏蔽窗纪律；本睡永不醒；klock_enter
    // 同款——睡眠放锁由舞蹈自动）。
    berry_hal_aarch64::kern_lock::klock_enter();
    berry_sched::block_current(0, 0, u64::MAX);
    unreachable!("it_sinkA: u64::MAX 不到期")
}

/// it_sinkB（declared UTILITY 0x03）：三连收——CH4a 伪造 hop 帽（拒传播回
/// 落）→ wait_one(CH6, READABLE) 阻塞等 slow 睡 200ms 后发出（4-2 阻塞链
/// 复用，不赌时序）→ recv CH6（slow 的降级转发件，Own 不计数）→ recv CH4b
/// 伪造链累计封顶（拒传播回落）。终态 eff==declared 0x03。
extern "C" fn task_intent_sink_b(_arg: usize) -> ! {
    use crate::handle::SIGNAL_READABLE;
    use crate::syscall::{NR_CHANNEL_RECV, NR_CLOCK_READ, NR_OBJECT_WAIT_ONE};
    let mut buf = [0u8; 8];
    let (mut n, mut nh): (u64, u64) = (8, 0);
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(6) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_sinkB: recv CH4a（hop 帽拒传播）"
    );
    let mut now: u64 = 0;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "it_sinkB: clock_read");
    let mut observed = 0u64;
    // SAFETY: 同上。
    let pobs = core::ptr::addr_of_mut!(observed) as u64;
    assert_eq!(
        svc6(NR_OBJECT_WAIT_ONE as u64, it_h(7) as u64, SIGNAL_READABLE,
             now + 100 * 1_000_000_000, pobs, 0, 0),
        0,
        "it_sinkB: wait CH6 READABLE（slow 发出唤醒）"
    );
    assert!(observed & SIGNAL_READABLE != 0, "it_sinkB: observed 须含 READABLE");
    n = 8;
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(7) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_sinkB: recv CH6（降级转发件）"
    );
    n = 8;
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(8) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_sinkB: recv CH4b（链累计封顶拒传播）"
    );
    // 契约包络同 it_mid（block_current 屏蔽窗纪律；本睡永不醒；klock_enter
    // 同款——睡眠放锁由舞蹈自动）。
    berry_hal_aarch64::kern_lock::klock_enter();
    berry_sched::block_current(0, 0, u64::MAX);
    unreachable!("it_sinkB: u64::MAX 不到期")
}

/// it_slow（declared BACKGROUND 0x04）：recv CH5 借入 0x02 → 睡 200ms（超
/// 每跳预算 50ms）→ send CH6：快照降级——借来的 0x02 不再传播，装自己的
/// declared 0x04（budget_capped 计数 + trace 行）。**借入字段保持**（无回
/// 落事件——转发降级不清自身借入，probe 断言 eff==0x02）。
extern "C" fn task_intent_slow(_arg: usize) -> ! {
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_CLOCK_READ, NR_THREAD_SLEEP};
    let mut buf = [0u8; 8];
    let (mut n, mut nh): (u64, u64) = (8, 0);
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, it_h(10) as u64, buf.as_mut_ptr() as u64,
             core::ptr::addr_of_mut!(n) as u64, 0,
             core::ptr::addr_of_mut!(nh) as u64, 0),
        0,
        "it_slow: recv CH5（借入 0x02）"
    );
    let mut now: u64 = 0;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "it_slow: clock_read");
    assert_eq!(
        svc4(NR_THREAD_SLEEP as u64, now + 200_000_000, 0, 0),
        0,
        "it_slow: 睡 200ms（超每跳预算 50ms）"
    );
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, it_h(11) as u64, buf.as_ptr() as u64, 8, 0, 0, 0),
        0,
        "it_slow: send CH6（降级转发）"
    );
    // 契约包络同 it_mid（block_current 屏蔽窗纪律；本睡永不醒；klock_enter
    // 同款——睡眠放锁由舞蹈自动）。
    berry_hal_aarch64::kern_lock::klock_enter();
    berry_sched::block_current(0, 0, u64::MAX);
    unreachable!("it_slow: u64::MAX 不到期")
}

/// it_probe（验收司机）：睡 8 slice（slice_ns 换算同 waker2——clock 域
/// 确定性）等全链落定 → 遍历 SLEEPQ 断言四睡眠者 intent 字段 + 遥测三计数
/// 器（borrows≥3 / hop_capped≥1 / budget_capped≥2——send 降级 + recv 封顶
/// 两路）→ 打 `INTENT ok`（smoke 第 19 标记）→ exit。字段即机器关卡：借入
/// 生效 / opt-out 回落 / hop==1 转发活证 / 双帽拒传播 / 降级转发不清借入，
/// 全链一次断言。
extern "C" fn task_intent_probe(_arg: usize) -> ! {
    use crate::syscall::{NR_CLOCK_READ, NR_THREAD_SLEEP};
    let freq = berry_hal_aarch64::timer::cntfrq();
    let slice_ns = berry_hal_aarch64::timer::slice_cycles() * 1_000_000_000 / freq;
    let mut now: u64 = 0;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "it_probe: clock_read");
    assert_eq!(
        svc4(NR_THREAD_SLEEP as u64, now + 8 * slice_ns, 0, 0),
        0,
        "it_probe: thread_sleep 8 slice（等全链落定）"
    );
    let (mut saw_mid, mut saw_a, mut saw_b, mut saw_slow) = (false, false, false, false);
    // 契约包络（复盘雷 2 残留诊断）：for_each_sleeper 须 IRQ 屏蔽窗内调用
    //（thread_sleep 的 SVC eret 后 IRQ 已解蔽；ai_probe 同款显式包络）。
    // S-2 E0 起包络升级 klock_enter/klock_exit（F39，草案 §2.3——锁窗等价
    // 口径，B16）；窗口内计数读（I50 原子化）窗外照旧安全。
    berry_hal_aarch64::kern_lock::klock_enter();
    berry_sched::for_each_sleeper(|t| {
        match t.name {
            b"it_mid" => {
                saw_mid = true;
                assert_eq!(t.intent_effective, berry_sched::INTENT_BACKGROUND,
                           "it_probe: mid 须 opt-out 回落 0x04");
            }
            b"it_sinkA" => {
                saw_a = true;
                assert_eq!(t.intent_effective, berry_sched::INTENT_INTERACTIVE,
                           "it_probe: sinkA 须借入 0x02");
                assert_eq!(t.intent_hop, 1,
                           "it_probe: sinkA hop==1（mid 借入转发活证）");
            }
            b"it_sinkB" => {
                saw_b = true;
                assert_eq!(t.intent_effective, berry_sched::INTENT_UTILITY,
                           "it_probe: sinkB 须双帽+降级后回落 0x03");
            }
            b"it_slow" => {
                saw_slow = true;
                assert_eq!(t.intent_effective, berry_sched::INTENT_INTERACTIVE,
                           "it_probe: slow 借入须保持（转发降级不清借入）");
            }
            _ => {}
        }
    });
    berry_hal_aarch64::kern_lock::klock_exit();
    assert!(saw_mid && saw_a && saw_b && saw_slow,
           "it_probe: 四睡眠者未同时在线（时序未达）");
    assert!(crate::telemetry::intent_borrows() >= 3,
           "it_probe: 借入计数须 ≥3（mid/sinkA/slow 三次结算）");
    assert!(crate::telemetry::intent_hop_capped_total() >= 1,
           "it_probe: hop 帽计数须 ≥1（CH4a）");
    assert!(crate::telemetry::intent_budget_capped_total() >= 2,
           "it_probe: 时间帽计数须 ≥2（slow send 降级 + CH4b 结算封顶）");
    berry_hal_aarch64::uart_puts(b"I INTENT ok\n");
    berry_sched::exit_current()
}

// ---- 4-4 fastpath/slowpath 验收 bench（表见 main 的 4-4 段注释）----

/// bench 轮数：128 轮 × 每轮 µs–ms 级 = 整体 ms 级完成（prio 16/32 独占，
/// 先于一切矩阵/intent 任务跑完退出）。
const FAST_ROUNDS: u64 = 128;

/// fa_pong（prio 32，持 B 端）：每轮 wait READABLE 阻塞（正常路径被 ping 的
/// send fastpath 定点唤醒 + 直切上 CPU）→ recv 验 PING → 回 PONG（此刻 ping
/// 尚 Ready 未睡 → 这条 send 恒走 slowpath，bench 的 miss 来源）。N 轮后
/// exit——末轮回件后即退，不在 SLEEPQ 留尾。
extern "C" fn task_fast_ponger(arg: usize) -> ! {
    use crate::handle::SIGNAL_READABLE;
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_CLOCK_READ, NR_OBJECT_WAIT_ONE};
    let hb = arg as u32;
    let mut buf = [0u8; 8];
    for _ in 0..FAST_ROUNDS {
        let mut now: u64 = 0;
        // SAFETY: 本任务内核栈局部变量，活过调用。
        let pnow = core::ptr::addr_of_mut!(now) as u64;
        assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "fa_pong: clock_read");
        let mut observed = 0u64;
        // SAFETY: 同上。
        let pobs = core::ptr::addr_of_mut!(observed) as u64;
        // 10s 帽兜底：断链（fastpath 漏醒）则到点 -ETIMEDOUT 断言失败 = smoke
        // FAIL（fail loud，不静默挂死拖 DEADLINE_BOOT）。
        assert_eq!(
            svc6(NR_OBJECT_WAIT_ONE as u64, hb as u64, SIGNAL_READABLE,
                 now + 10_000_000_000, pobs, 0, 0),
            0,
            "fa_pong: wait READABLE（ping send 定点唤醒）"
        );
        assert!(observed & SIGNAL_READABLE != 0, "fa_pong: observed 须含 READABLE");
        let (mut n, mut nh): (u64, u64) = (8, 0); // 容量 in-out（recv 前备好）
        assert_eq!(
            svc6(NR_CHANNEL_RECV as u64, hb as u64, buf.as_mut_ptr() as u64,
                 core::ptr::addr_of_mut!(n) as u64, 0,
                 core::ptr::addr_of_mut!(nh) as u64, 0),
            0,
            "fa_pong: recv PING"
        );
        assert!(buf[..4] == *b"PING", "fa_pong: 载荷须 PING");
        buf[..4].copy_from_slice(b"PONG"); // 回件换文（PING→PONG）
        assert_eq!(
            svc6(NR_CHANNEL_SEND as u64, hb as u64, buf.as_ptr() as u64, 4, 0, 0, 0),
            0,
            "fa_pong: 回 PONG"
        );
    }
    berry_sched::exit_current()
}

/// fa_ping（prio 16，持 A 端）：每轮计 t0 → send PING（fastpath 检查链全过 →
/// 直切到 pong）→ 被切回（pong 回件后 block）→ wait poll 即命中 PONG 在队
///（deadline=0 纯 poll——断链 -EAGAIN 秒失败）→ recv 验回显 → 累计 RT。收尾
/// 断言三计数器（hits/miss/direct，弹性 N-2 容 tick 边角）+ 打 `FAST ok`
///（smoke 第 20 标记）与 RT 数字（ticks 均值 + 1GHz 参考 cycle = ticks×1e9/
/// CNTFRQ——TCG 先出量级，真机复测标注）。
extern "C" fn task_fast_pinger(arg: usize) -> ! {
    use crate::handle::SIGNAL_READABLE;
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_OBJECT_WAIT_ONE};
    let ha = arg as u32;
    let freq = berry_hal_aarch64::timer::cntfrq();
    let mut buf = [0u8; 8];
    let mut rt_ticks: u64 = 0;
    for _ in 0..FAST_ROUNDS {
        buf[..4].copy_from_slice(b"PING");
        let t0 = berry_hal_aarch64::timer::cntvct();
        assert_eq!(
            svc6(NR_CHANNEL_SEND as u64, ha as u64, buf.as_ptr() as u64, 4, 0, 0, 0),
            0,
            "fa_ping: send PING（fastpath：pong 在睡等 READABLE）"
        );
        let mut observed = 0u64;
        // SAFETY: 本任务内核栈局部变量，活过调用。
        let pobs = core::ptr::addr_of_mut!(observed) as u64;
        // poll-first 命中：pong 回件先于切回 ping 落队（确定性时序），零阻塞。
        assert_eq!(
            svc6(NR_OBJECT_WAIT_ONE as u64, ha as u64, SIGNAL_READABLE, 0, pobs, 0, 0),
            0,
            "fa_ping: wait poll 命中（PONG 已在队；断链 -EAGAIN 即 FAIL）"
        );
        let (mut n, mut nh): (u64, u64) = (8, 0); // 容量 in-out（recv 前备好）
        assert_eq!(
            svc6(NR_CHANNEL_RECV as u64, ha as u64, buf.as_mut_ptr() as u64,
                 core::ptr::addr_of_mut!(n) as u64, 0,
                 core::ptr::addr_of_mut!(nh) as u64, 0),
            0,
            "fa_ping: recv PONG"
        );
        assert!(buf[..4] == *b"PONG", "fa_ping: 载荷须 PONG");
        rt_ticks += berry_hal_aarch64::timer::cntvct() - t0;
    }
    let (hits, miss, direct) = (
        crate::telemetry::ipc_fast_hits(),
        crate::telemetry::ipc_fast_misses(),
        crate::telemetry::ipc_direct_total(),
    );
    assert!(hits >= FAST_ROUNDS - 2,
            "fa_ping: fastpath 命中须 ≥N-2（实跑 {}）", hits);
    assert!(miss >= 1, "fa_ping: slowpath 须有走数（pong 回件时 ping Ready 未睡）");
    assert!(direct >= FAST_ROUNDS - 2,
            "fa_ping: 直切须 ≥N-2（32>16 每轮必切；实跑 {}）", direct);
    let avg_ticks = rt_ticks / FAST_ROUNDS;
    let ref_cyc = avg_ticks * 1_000_000_000 / freq; // 1GHz 参考 cycle（数值 = ns）
    berry_hal_aarch64::uart_puts(b"F FAST ok\n  rt_ticks=");
    berry_hal_aarch64::uart_put_dec(avg_ticks);
    berry_hal_aarch64::uart_puts(b" ref_cyc=");
    berry_hal_aarch64::uart_put_dec(ref_cyc);
    berry_hal_aarch64::uart_puts(b" hits=");
    berry_hal_aarch64::uart_put_dec(hits);
    berry_hal_aarch64::uart_puts(b" miss=");
    berry_hal_aarch64::uart_put_dec(miss);
    berry_hal_aarch64::uart_puts(b" direct=");
    berry_hal_aarch64::uart_put_dec(direct);
    berry_hal_aarch64::uart_puts(b"\n");
    berry_sched::exit_current()
}

/// svc_tag（4-5 前置自证，prio 0）：私有表 tag 任务——两断言判别
/// `caller_table` 的 EL1 tag 路由：① 自己表 handle poll 空队列 → -EAGAIN
/// （表内有效；-EBADF 即误路由共享表）；② 共享表真 handle（fa B 端）→
/// -EBADF（自己表查无；-EAGAIN 即还在走共享表）。poll-only 不睡，验完即
/// exit（断言炸 = KERNEL PANIC → 后续标记缺失 = smoke FAIL，fail loud）。
extern "C" fn task_svc_tag(arg: usize) -> ! {
    use crate::handle::SIGNAL_READABLE;
    use crate::syscall::NR_OBJECT_WAIT_ONE;
    let h_own = arg as u32;
    let h_shared = (arg >> 32) as u32;
    let mut observed = 0u64;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pobs = core::ptr::addr_of_mut!(observed) as u64;
    assert_eq!(
        svc6(NR_OBJECT_WAIT_ONE as u64, h_own as u64, SIGNAL_READABLE, 0, pobs, 0, 0),
        -11, // -EAGAIN：自己表 handle 有效、队列空（误路由共享表则 -EBADF=-9）
        "svc_tag: 自己表 handle poll 空队列须 -EAGAIN"
    );
    assert_eq!(
        svc6(NR_OBJECT_WAIT_ONE as u64, h_shared as u64, SIGNAL_READABLE, 0, pobs, 0, 0),
        -9, // -EBADF：共享表 handle 在自己表查无（返 -EAGAIN 即还在走共享表）
        "svc_tag: 共享表 handle 须 -EBADF（tag 路由活证）"
    );
    berry_hal_aarch64::uart_puts(b"TAGTAB ok\n");
    berry_sched::exit_current()
}

// ---- 4-6 B 半：AI 最小演示（boot spawn 块见 main 尾段；设计 =
// `docs/4-6出口演示设计草案.md` B 半管线：遥测 → advisor → 注入 → 执法活证）----

// ---- 用户半信号位登记表（单一事实源——挑新位前必查；新增必登记）----
// 高 32 位用户位（SIGNAL_USER_MASK）的占用登记。此前唯一「登记」是散布各处
// 的散文注释（含「1<<36 全库未用」这类只反映写注释当天的口头声明），多会话
// 并行下已发生真实冲突——a27eb9c 为 waiter 修复新占 1<<38 时 HEAD 无从得知。
// asm 模板魔数无法引用本表，表的价值 = 人查的集中清单 + 编译期去重断言；
// 同位双用途 = 互相清位/误唤醒（waiter 雷的同型成因），显形即 missed wakeup
// 级排障成本。ktest 瞬态位（sched::init 前跑、运行期不在场）不登记。
const USER_SIGNAL_BITS: &[(u64, &str)] = &[
    (1 << 32, "哑 Event 对照项（console/ponger wait_many，永不 signal）"),
    (1 << 33, "SLEEP_OBJ（4-0 sleeper 条件唤醒）"),
    (1 << 34, "AI_EV_DONE（4-6B bench 完成事件）"),
    (1 << 35, "waiter U3（4-1 路③ 长睡握手）"),
    (1 << 36, "AI_EV_REL（4-6B probe 放行事件）"),
    (1 << 37, "waiter U2（4-1 路② poll 命中）"),
    (1 << 38, "waiter U4（a27eb9c 握手位拆分——合入即生效）"),
    (1 << 39, "RX Event（P5 A-1 console 输入——内核 RX 中断到数据置位）"),
    (1 << 40, "rt_probe 哑 Event（R-1 cyclictest oversleep——永不置位，仅超时醒）"),
    (1 << 41, "soakmon 哑 Event（S-1 soak 心跳——永不置位，仅超时醒）"),
    (1 << 42, "par_probe 哑 Event（S-2b 副核参与金丝雀——永不置位，仅超时醒）"),
];
// 编译期执法：登记表不得重复位（挑位前查表 + 本断言 = 双保险）。
const _: () = {
    let bits = USER_SIGNAL_BITS;
    let mut i = 0;
    while i < bits.len() {
        let mut j = i + 1;
        while j < bits.len() {
            assert!(bits[i].0 != bits[j].0, "user signal bit collision");
            j += 1;
        }
        i += 1;
    }
};

/// bench 完成事件 E_done 的用户位（占用登记见 [`USER_SIGNAL_BITS`]）。
const AI_EV_DONE_BIT: u64 = 1 << 34;
/// probe 放行事件 E_rel 的用户位（占用登记见 [`USER_SIGNAL_BITS`]）。
const AI_EV_REL_BIT: u64 = 1 << 36;
/// rt_probe 哑 Event 的用户位（R-1，占用登记见 [`USER_SIGNAL_BITS`]——R 线
/// 1<<40 起逐位递增，设计公约 3）。永不置位：唯一醒因 Timeout。
const RT_EV_BIT: u64 = 1 << 40;
/// soakmon 哑 Event 的用户位（S-1，占用登记见 [`USER_SIGNAL_BITS`]——
/// RT_EV_BIT 的续位）。永不置位：唯一醒因 Timeout（活性心跳不是死线，
/// 超时醒 = 正常节拍）。
const SOAK_EV_BIT: u64 = 1 << 41;
/// par_probe 哑 Event 的用户位（S-2b，占用登记见 [`USER_SIGNAL_BITS`]——
/// SOAK_EV_BIT 的续位）。永不置位：唯一醒因 Timeout（soakmon 同款契约）。
const PAR_EV_BIT: u64 = 1 << 42;

/// ai_demo ↔ ai_probe 握手标志：demo bind 注入表后、睡 E_rel 前置位——probe
/// 只在该窗口内做 clamp 断言（E_done 等待期 demo 同样在队但 eff=0，flag 区分
/// 两窗）。单核 v1 Relaxed 足够（读写都在单核串行语境；S-2 起口径不变——
/// 内核任务全钉 CPU0，读写同核串行保持）。
static AI_BOUND: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// 最近一轮 bench 的均轮 RT（CNTVCT ticks）：pinger 收尾写、demo 醒来读——
/// 单核串行天然有序（先写后 signal、醒来必读到本轮值；S-2 起同核钉 CPU0
/// 口径保持）。
static AI_RT_LAST: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// bench 轮数（fa 128 的一半：两轮 + 探针窗都压在秒级虚拟时间内）。
const AI_ROUNDS: u64 = 64;

/// ai_demo 上下文（boot 构造、leak 常驻；spawn arg 单指针携带）。
struct AiDemoCtx {
    /// demo 表：E_done handle（bench 完成等待）。
    h_done: u32,
    /// demo 表：E_rel handle（probe 放行等待）。
    h_rel: u32,
    /// pinger arg（低 32 = bench 表通道 A 端、高 32 = bench 表 E_done）。
    h_ping_arg: usize,
    /// ponger arg（bench 表通道 B 端）。
    h_pong_arg: usize,
    /// bench 对共享表 tag（注入表合法项 key）。
    tag_p: usize,
    /// demo 自己的 tag（注入表越界探针项 key）。
    tag_d: usize,
}

/// 遥测摘要 dump（管线第 1 段）：固定 `tele:` 前缀、`k=v` 空格分隔——host 侧
/// `tools/advisor.py` 的输入面（F6 第一级 schema：切换分型 + 唤醒分型，环内
/// 样本口径）。只读既有环（total/span/last/wake_*），不碰热路径（读点在 demo
/// 任务里，遥测写入的零分配纪律不约束消费侧）。
fn tele_dump() {
    let (mut y, mut p, mut e, mut b) = (0u64, 0u64, 0u64, 0u64);
    for i in 0..crate::telemetry::span() {
        match crate::telemetry::last(i).expect("span 内必有").kind {
            0 => y += 1,
            1 => p += 1,
            2 => e += 1,
            _ => b += 1,
        }
    }
    berry_hal_aarch64::uart_puts(b"tele: sw total=");
    berry_hal_aarch64::uart_put_dec(crate::telemetry::total());
    berry_hal_aarch64::uart_puts(b" span=");
    berry_hal_aarch64::uart_put_dec(crate::telemetry::span() as u64);
    berry_hal_aarch64::uart_puts(b" y=");
    berry_hal_aarch64::uart_put_dec(y);
    berry_hal_aarch64::uart_puts(b" p=");
    berry_hal_aarch64::uart_put_dec(p);
    berry_hal_aarch64::uart_puts(b" e=");
    berry_hal_aarch64::uart_put_dec(e);
    berry_hal_aarch64::uart_puts(b" b=");
    berry_hal_aarch64::uart_put_dec(b);
    berry_hal_aarch64::uart_puts(b"\n");
    let (mut s, mut t) = (0u64, 0u64);
    for i in 0..crate::telemetry::wake_span() {
        match crate::telemetry::wake_last(i).expect("span 内必有").reason {
            crate::telemetry::REASON_SIGNALED => s += 1,
            _ => t += 1,
        }
    }
    berry_hal_aarch64::uart_puts(b"tele: wake total=");
    berry_hal_aarch64::uart_put_dec(crate::telemetry::wake_total());
    berry_hal_aarch64::uart_puts(b" span=");
    berry_hal_aarch64::uart_put_dec(crate::telemetry::wake_span() as u64);
    berry_hal_aarch64::uart_puts(b" sig=");
    berry_hal_aarch64::uart_put_dec(s);
    berry_hal_aarch64::uart_puts(b" tmo=");
    berry_hal_aarch64::uart_put_dec(t);
    berry_hal_aarch64::uart_puts(b"\n");
}

/// ai_demo 私助：clock_read → wait_one(h, bit, now+cap) → 断言命中 → 清用户
/// 半。清位是复用前提（Event 位不自动清——不清则次轮 poll 秒中，demo 醒着
/// 高优饿死 bench）；4-2 纪律：clear 掩码用用户半全集。
fn ai_wait_ev(h: u32, bit: u64, cap_ns: u64) {
    use crate::syscall::{NR_CLOCK_READ, NR_OBJECT_SIGNAL, NR_OBJECT_WAIT_ONE};
    let mut now: u64 = 0;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "ai_wait_ev: clock_read");
    let mut observed = 0u64;
    // SAFETY: 同上。
    let pobs = core::ptr::addr_of_mut!(observed) as u64;
    assert_eq!(
        svc6(NR_OBJECT_WAIT_ONE as u64, h as u64, bit, now + cap_ns, pobs, 0, 0),
        0,
        "ai_wait_ev: wait（断链超时 -ETIMEDOUT 即 FAIL，fail loud）"
    );
    assert!(observed & bit != 0, "ai_wait_ev: observed 须含请求位");
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, h as u64, crate::handle::SIGNAL_USER_MASK, 0),
        0,
        "ai_wait_ev: 清用户半"
    );
}

/// ai_demo 私助：spawn bench 对（tag_P 表，fa 同序——pong 先入队先睡等，ping
/// 首 send 走快路）。**运行期 spawn 须 IRQ 屏蔽**：boot 期 spawn 天然屏蔽，
/// 运行期 runqueue 链接中途被 tick 抢占 = 半链接表被 pick_next 走——手动包络。
/// S-2 E0 起包络升级 klock_enter/klock_exit（F34，草案 §2.3）。
fn ai_bench_round(ctx: &AiDemoCtx) {
    berry_hal_aarch64::kern_lock::klock_enter();
    berry_sched::spawn_with_tag(task_ai_pong, ctx.h_pong_arg, b"ai_pong", 32, ctx.tag_p);
    berry_sched::spawn_with_tag(task_ai_ping, ctx.h_ping_arg, b"ai_ping", 16, ctx.tag_p);
    berry_hal_aarch64::kern_lock::klock_exit();
}

/// ai_demo（4-6 B 半主角，prio 0，tag_D 私有表）：F1 管线一次走完——①睡
/// 2 slice（boot 期任务先跑、遥测积累）→ ②tele dump → ③bench 轮 1（无表
/// 基线 rt1）→ ④bind 注入表（合法 +8 / 越界 +200 探针；IRQ 屏蔽窗写点）→
/// ⑤睡 E_rel（bind 后睡入 SLEEPQ，PI 尾趟 refresh_effective 经 weight_of 同
/// clamp 把本任务 eff 刷到 0+64——probe 的断言窗口，醒来由 probe signal 放行）
/// → ⑥bench 轮 2（表在：16+8/32+8=24/40 同层相对序保持；demo 照睡不抢 CPU）
/// → ⑦unbind → ⑧开销断言（F9① 最小兑现：rt2 ≤ 2×rt1，TCG 噪声容差）→
/// ⑨打 `AIDEMO ok`（smoke 第 22 标记）→ exit。
extern "C" fn task_ai_demo(arg: usize) -> ! {
    use crate::syscall::{NR_CLOCK_READ, NR_THREAD_SLEEP};
    // SAFETY: ctx 由 boot Box::leak 常驻（活过本任务全程）。
    let ctx = unsafe { &*(arg as *const AiDemoCtx) };
    let freq = berry_hal_aarch64::timer::cntfrq();
    let slice_ns = berry_hal_aarch64::timer::slice_cycles() * 1_000_000_000 / freq;
    let mut now: u64 = 0;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "ai_demo: clock_read");
    // ① 睡 2 slice（bench 对 prio 16/32 严格优先本就先完成，此睡更稳）。
    assert_eq!(
        svc4(NR_THREAD_SLEEP as u64, now + 2 * slice_ns, 0, 0),
        0,
        "ai_demo: thread_sleep 2 slice"
    );
    // ② 遥测 dump（advisor 输入面）。
    tele_dump();
    // ③ bench 轮 1（无表基线）。
    ai_bench_round(ctx);
    ai_wait_ev(ctx.h_done, AI_EV_DONE_BIT, 100 * 1_000_000_000);
    let rt1 = AI_RT_LAST.load(core::sync::atomic::Ordering::Relaxed);
    // ④ bind 注入表（advisor.py 产出快照——见 boot 4-6 段与 tools/advisor.py；
    // 表运行期 Box::leak：key 是运行期表指针）。越界项 = 拒收面探针：内核在
    // apply/weight_of 双点夹到 ±64（F9② 单调收窄——夹取是唯一方向，无提权
    // 路径），机器断言交 probe。
    let tbl: &'static [berry_sched::policy::Entry] =
        alloc::boxed::Box::leak(alloc::boxed::Box::new([
            berry_sched::policy::Entry {
                task_key: ctx.tag_p as u64,
                directive: berry_sched::policy::Directive {
                    placement: 0, weight: 8, epp: None,
                },
            },
            berry_sched::policy::Entry {
                task_key: ctx.tag_d as u64,
                directive: berry_sched::policy::Directive {
                    placement: 0, weight: 200, epp: None,
                },
            },
        ]));
    // 写点 IRQ 屏蔽（policy 契约：bind/unbind 仅屏蔽窗——决策路径只读）。
    // S-2 E0 起包络升级 klock_enter/klock_exit（F41，草案 §2.3）。
    berry_hal_aarch64::kern_lock::klock_enter();
    berry_sched::policy::bind(tbl);
    berry_hal_aarch64::kern_lock::klock_exit();
    // 拒收 trace（demo 侧打——apply 是 IRQ 热路径不得加打印，F9①；机器断言
    // 交 probe 读字段）。
    berry_hal_aarch64::uart_puts(b"  policy: reject w=+200 -> clamp +64 (envelope +-64)\n");
    // ⑤ 睡 E_rel：probe 断言窗口（醒来由 probe signal 放行）。
    AI_BOUND.store(true, core::sync::atomic::Ordering::Relaxed);
    ai_wait_ev(ctx.h_rel, AI_EV_REL_BIT, 100 * 1_000_000_000);
    // ⑥ bench 轮 2（表在）。
    ai_bench_round(ctx);
    ai_wait_ev(ctx.h_done, AI_EV_DONE_BIT, 100 * 1_000_000_000);
    let rt2 = AI_RT_LAST.load(core::sync::atomic::Ordering::Relaxed);
    // ⑦ unbind（恢复接口缺席态；klock 包络同 ④，F41）。
    berry_hal_aarch64::kern_lock::klock_enter();
    berry_sched::policy::unbind();
    berry_hal_aarch64::kern_lock::klock_exit();
    // ⑧ 开销断言（F9① 最小兑现：注入表在场时 bench RT 须同量级——热路径增
    // 量 = 2 条目线性查表 + 权重算术，TCG 下远低于噪声；容差 2x 是噪声界非
    // 精度断言，-icount 精确版/真机 p99 随 FAST 线留 G 卡复测）。
    assert!(rt1 > 0, "ai_demo: rt1 须非零");
    assert!(
        rt2 <= rt1 * 2,
        "ai_demo: 注入后 bench RT 须同量级（rt1={} rt2={}，容差 2x）", rt1, rt2
    );
    // ⑨ 收官标记（smoke 第 22）。G43 拼行（S-2b 副核参与前多核撕行防线）：
    // 整条消息 PRINT_LOCK 内一次拼完。
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"J AIDEMO ok: rt1=");
        berry_hal_aarch64::uart_put_dec(rt1);
        berry_hal_aarch64::uart_puts(b" rt2=");
        berry_hal_aarch64::uart_put_dec(rt2);
        berry_hal_aarch64::uart_puts(b" clamp=64\n");
    });
    berry_sched::exit_current()
}

/// ai_probe（4-6 B 半验收司机，prio 0，挂第三张私有表）：1 slice 步进重试
/// （≤64 次）等 `AI_BOUND` 窗口内的 ai_demo 在队 → 断言其 `prio_effective
/// == 64` 且 `wait_signals == AI_EV_REL_BIT`（raw 指令 +200、在队字段 64
/// ——内核 clamp 执法的机器活证）→ signal E_rel 放行 → exit。步进重试而非
/// 一次长睡：boot 期任务次序不定，「标记不得建立在时序竞速上」——条件
/// （flag && 在队）满足才前进，窗口由 demo 的 100s 帽保持敞开、预算 64
/// slice ≈ 6.4s 远超最慢轮转。**私有表纪律（4-5 同款，本步实踩）**：probe
/// 是长命睡眠内核任务，睡共享表会被 pi 窗错捐 eff 32（冻结随行、严格优先
/// 反噬 prio-0 轮转）——自己的唤醒周期反而把 ai_demo 的轮次推过自身重试
/// 预算（bisect 实测：demo 第二轮 ~830ms 才到、8×1 slice 预算 ~816ms 先
/// 尽，panic「时序未达」）。挂私有表断根：pi 受捐反查只看自己的表。另
/// for_each_sleeper 契约须 IRQ 屏蔽窗内调用（tick 超时摘队会动睡眠链接，
/// 遍历中接线即扬队）——probe 任务体经 SVC eret 后 IRQ 已解蔽，扫描窗手动
/// 包络（it_probe 未包络属侥幸窗口小；本 probe 扫描最多 64 次不赌）。
extern "C" fn task_ai_probe(arg: usize) -> ! {
    use crate::syscall::{NR_CLOCK_READ, NR_OBJECT_SIGNAL, NR_THREAD_SLEEP};
    let h_rel = arg as u32;
    let freq = berry_hal_aarch64::timer::cntfrq();
    let slice_ns = berry_hal_aarch64::timer::slice_cycles() * 1_000_000_000 / freq;
    let mut now: u64 = 0;
    // SAFETY: 本任务内核栈局部变量，活过调用。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    let mut saw = false;
    for _ in 0..64u64 {
        if AI_BOUND.load(core::sync::atomic::Ordering::Relaxed) {
            // S-2 E0 起扫描包络升级 klock_enter/klock_exit（F40，草案 §2.3
            // ——锁窗等价口径，B16）。
            berry_hal_aarch64::kern_lock::klock_enter();
            berry_sched::for_each_sleeper(|t| {
                match t.name {
                    b"ai_demo" => {
                        saw = true;
                        assert_eq!(t.wait_signals, AI_EV_REL_BIT,
                                   "ai_probe: demo 须睡 E_rel 位（AI_BOUND 窗口）");
                        assert_eq!(t.prio_effective, 64,
                                   "ai_probe: demo eff 须为 clamp 后的 64（+200 越界探针）");
                    }
                    _ => {}
                }
            });
            berry_hal_aarch64::kern_lock::klock_exit();
            if saw {
                break;
            }
        }
        assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "ai_probe: clock_read");
        assert_eq!(
            svc4(NR_THREAD_SLEEP as u64, now + slice_ns, 0, 0),
            0,
            "ai_probe: thread_sleep 1 slice（步进重试）"
        );
    }
    assert!(saw, "ai_probe: 64 次重试未见 bind 窗口内的 ai_demo 在队（时序未达）");
    // 放行：signal E_rel（probe 私有表 handle——与 demo 表同一 ObjRef，跨表互通）。
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, h_rel as u64, 0, AI_EV_REL_BIT),
        0,
        "ai_probe: signal E_rel 放行"
    );
    berry_sched::exit_current()
}

// ---- R-1：rt_probe（cyclictest 式 oversleep，boot 门控 spawn 块见 main 尾段；
// 设计 = docs/基准与压力测试设计.md §10 + 落地实录）----

/// rt_probe 的 fw_cfg 具名项名（门控键——bench.py --rt 专递
/// `-fw_cfg name=opt/berry/rt,string=1`；与 ROOTFS_NAME 同款「两侧对读维护」
/// 纪律，fwcfg.rs 单文件放不下双先例故立此处）。
const RT_PROBE_NAME: &str = "opt/berry/rt";

/// rt_probe 每档样本数（设计 §10 定值 N=32；bench.py --rt 通道期望
/// `RT ok n=96` = 3 档 × 32 与此镜像）。
const RT_SAMPLES: usize = 32;
// 分位秩与 RT_SAMPLES 钉板耦合：p50 = ceil(50·32/100) = 16 → s[15]、
// p99 = ceil(99·32/100) = 32 → s[31]（== max，最近秩的诚实结果）。改样本数
// 须同步重算两秩 + bench.py BENCHRT_RE 的 n=32 字面——编译期拦静默漂移。
const _: () = {
    assert!(RT_SAMPLES == 32, "RT_SAMPLES 变更须同步分位秩与 bench.py 镜像");
};

/// rt_probe（R-1）：三档 δ × 32 样本 oversleep（cyclictest 式绝对周期锚）。
///
/// - δ ∈ {slice/4, slice, 4·slice}（slice_cycles() 单源——短档量定时器精度、
///   长档量轮转排队尾）；**不走 ns 换算**（公约 5：换算舍入会污染被测量）
///   ——睡点直调 `block_current`（`object_wait_one` 的内部形态，SVC 面的
///   ns→vct 折算被绕过，deadline 参数本来就是 CNTVCT 周期）。
/// - deadline 档首锚 `cntvct()+δ`、此后每轮 `+= δ`（绝对周期锚——不随
///   醒来时刻漂移）；**漏整周期再锚（cyclictest overrun 形）**：本样本
///   oversleep ≥ δ（下一周期 deadline 也已成过去 = 结构性追不上）时重锚
///   `deadline = 醒时刻 + δ`。首跑实踩（纯绝对递推版被否的实录）：δ=¼slice
///   < v1 唤醒粒度（超时扫描只在 tick 跑、无 arm-to-deadline——粒度恒
///   1 slice）→ 每轮 lag 增 (slice−δ)、32 轮累计成阶梯：p50=95M/max=193M
///   随样本序线性涨——量出的是**漂移**不是**延迟**（对调度器的错误指控；
///   4·slice 档同 boot 实测 p50=4M = 真实唤醒延迟 ≤1 tick）。再锚后每样本
///   回到「δ 未来请求的唤醒延迟」语义（短档量出的正是定时器粒度本身
///   ——tier-1 p50 ≈ 1 slice 量级是 v1 的诚实答案，arm-to-nearest-deadline
///   （优化议程挂账）落地后该值应塌缩，本基准即其验收面）。
/// - oversleep = wake_vct − deadline，`checked_sub` 负值 panic（提前醒 = 再睡
///   循环破损 bug——L0 严格执法，R-0 on_wake 负延迟 panic 同款；正常统计
///   数值零断言，统计归统计）。漏周期样本的 oversleep 按原 deadline 记账
///   （迟到的账记在迟到者头上，不因再锚抹零）。
/// - 包络纪律（task_sleeper 同款，S-2 E0 起升级 klock 包络，F33）：klock_enter
///   → block（睡眠放锁/醒后重取由舞蹈自动）→ 续行（锁窗+IRQ 屏蔽）→ 窗内读
///   wake_vct → klock_exit。
extern "C" fn task_rt_probe(arg: usize) -> ! {
    use berry_sched::WakeReason;
    let s = berry_hal_aarch64::timer::slice_cycles();
    let deltas = [s / 4, s, s * 4];
    let mut samples = [0u64; RT_SAMPLES];
    for &delta in &deltas {
        let mut deadline = berry_hal_aarch64::timer::cntvct() + delta; // 档首锚
        for slot in samples.iter_mut() {
            berry_hal_aarch64::kern_lock::klock_enter();
            let r = berry_sched::block_current(arg, RT_EV_BIT, deadline);
            // **顺序例外（草案 §2.3 rt_probe 行，F33/F42 分工）**：醒后先读
            // wake_vct、后 klock_exit——block_current 返回前舞蹈已重取锁，故
            // wake_vct 读点落在重取之后（单核无争用 CAS 近零且逐样本均匀，
            // 不入 oversleep 样本）；读点与醒点之间零 tick/切换噪声混入
            // （BENCHRT 零噪声契约保持——rt_probe 钉 CPU0、CNTVCT 两核同源
            // 只读无竞争，E29，BENCHRT 基线不漂）。
            let wake_vct = berry_hal_aarch64::timer::cntvct();
            berry_hal_aarch64::kern_lock::klock_exit();
            // 哑 Event 表私有、无人 signal（boot 侧唯一持有者本任务、永不
            // object_signal）——唯一醒因 Timeout；非 Timeout = 跨表 wake 面
            // 破损（wake_matching 按地址精确匹配，误醒只可能来自真 signal）。
            assert!(r == WakeReason::Timeout,
                    "rt_probe: 醒因非 Timeout（哑 Event 永不置位——跨表 wake 面破损）");
            *slot = match wake_vct.checked_sub(deadline) {
                Some(o) => o,
                None => panic!("rt_probe: 负 oversleep（wake={} < deadline={}）= 再睡循环破损",
                               wake_vct, deadline),
            };
            // 漏整周期再锚（cyclictest overrun 形，取舍实录见任务 doc 注）：
            // oversleep ≥ δ = 下轮 deadline 已在过去，纯绝对递推只会积累漂移
            // 阶梯——重锚到醒时刻 + δ，每样本恢复「δ 未来请求」的延迟语义。
            deadline = if *slot >= delta {
                wake_vct + delta
            } else {
                deadline + delta
            };
        }
        samples.sort_unstable(); // core 面板全排序（32 元素，零分配）
        // 分位 = 最近秩 ceil(p·n/100)（user.rs benchwake_percentile 同形）：
        // n=32 → p50=s[15]、p99=s[31]。p99 == max 是 32 样本最近秩 99 分位的
        // 诚实结果（两列同值非 bug，勿为观感改次大值——偏离标准定义）。
        // 档行包络：打印时本任务 prio 8 可能正嵌在 prio 0 矩阵任务的逐字符
        // 打印流中段——G43 拼行（S-2 E0 起 PRINT_LOCK 整条消息原子）消灭
        // IRQ 语境 switches=N 行撕入向量（S-2b 副核参与前同面防线；免费
        // 保险，打印发生在两档采样之间、下一档锚点在打印后才取，零测量污染；
        // BENCHWAKE 先例无包络亦成立）。行 ≤192B（固定段 35B + 4 数值字段
        // 各至多 20 位 = 绝对界 115B）。
        berry_hal_aarch64::kern_lock::with_print_lock(|| {
            berry_hal_aarch64::uart_puts(b"BENCHRT,delta=");
            berry_hal_aarch64::uart_put_dec(delta);
            berry_hal_aarch64::uart_puts(b",n=");
            berry_hal_aarch64::uart_put_dec(RT_SAMPLES as u64);
            berry_hal_aarch64::uart_puts(b",p50=");
            berry_hal_aarch64::uart_put_dec(samples[15]);
            berry_hal_aarch64::uart_puts(b",p99=");
            berry_hal_aarch64::uart_put_dec(samples[31]);
            berry_hal_aarch64::uart_puts(b",max=");
            berry_hal_aarch64::uart_put_dec(samples[31]);
            berry_hal_aarch64::uart_puts(b"\n");
        });
    }
    // 收尾标记（须在 exit_current 之前——其后是死码，Plan B v0.5 gotcha 同族）；
    // 跑完即退：REAPQ 单条流量（Task Box + 内核栈，帽 32 内），非常驻。
    // n = 3 档 × RT_SAMPLES（wire 面 `RT ok n=96` = bench.py DONE_RT_RE 镜像）。
    // G43 拼行（F33/F42 分工——顺序例外归 wake_vct 读点，打印拼行归此）。
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"RT ok n=");
        berry_hal_aarch64::uart_put_dec((deltas.len() * RT_SAMPLES) as u64);
        berry_hal_aarch64::uart_puts(b"\n");
    });
    berry_sched::exit_current()
}

// ---- S-1：soakmon（soak 长跑心跳，boot 门控 spawn 块见 main 尾段；
// 设计 = docs/基准与压力测试设计.md §11 + 落地实录。命名注记：sched.rs 的
// SOAK_PRINT_EVERY（金丝雀节流常量）与 task_a 注释「SOAK=1 低噪 soak」是
// 历史名，与本文 soakmon 无关，勿混读）----

/// soakmon 的 fw_cfg 具名项名（门控键——soak.sh 专递
/// `-fw_cfg name=opt/berry/soak,string=1`；RT_PROBE_NAME 同款「两侧对读
/// 维护」纪律）。
const SOAK_PROBE_NAME: &str = "opt/berry/soak";

/// soakmon（S-1）：常驻心跳任务，每 60s（CNTVCT 周期口径）打一行
/// `SOAK t=<拍序> sw=<切换总数> pages=<allocated_pages> lost=<RX丢字>`。
///
/// - 四字段全是现成计数器直读（`berry_sched::switches` = switches=N 金丝雀
///   同源标量；`mm::frames().allocated_pages` = B-0/S-2 对账口径（D24 起
///   O(1) 水位原子读，口径由 buddy 对拍测试钉板）；`uart::rx_lost` =
///   console 环丢字）——零新内核状态。
/// - 周期 = `60 * cntfrq()`（CNTFRQ_EL0 直读，QEMU virt=62.5MHz 不硬编码；
///   不走 slice_cycles——那是 0.1s slice 单位，60s 是墙钟语义直乘最诚实）。
/// - **首拍潜伏 5 周期**（落地取舍，闭合设计缺口；S-1 对抗审查 finding 1
///   由 3 周期加到 5）：设计断言「pages ∈ [首拍值, 首拍值+64]」以首拍为
///   基线，但无潜伏的首拍落在 60s vct（本机实测 ≈60s wall，历史半率机型
///   ≈120s）< smoke 实测全链 ~190s wall——boot 尾部瞬态（walk 子进程映像/
///   栈 unmap 真还、REAPQ 内核栈 dealloc）在首拍后退出 → pages 下穿首拍 =
///   假 FAIL。3 周期（首拍实测 ≈182s wall）在首验日志里恰贴 walk 尾部
///   末段（SOAK t=1 紧随 CD ok 之后落地）——宿主负载拉伸指令驱动的尾部
///   而计数器驱动的首拍不动，基线可落回收瞬态中段。5 周期（首拍
///   ≈300-600s wall）对 smoke worst 300s 帽 + 数十秒尾部 ≥1.6x 裕量，
///   让基线必落稳态之后；t 仍从 1 起——断言保持设计原文形状，宿主零状态。
/// - deadline 绝对网格 `t0 + (n+4)*period`（n 从 1 起）：漂移自校正（每拍
///   独立锚定在网格上，不随醒来时刻累积），60s 周期 >> 1 tick 唤醒粒度，
///   R-1 式漏整周期再锚结构性不发生（追不上 = 须连续漏 1500 万 tick）。
/// - 包络纪律（task_rt_probe 同款，S-2 E0 起升级 klock 包络，F33）：klock_enter
///   → block（续行锁窗）→ 醒因断言 + 读计数器 + **整行打印**（with_print_lock
///   拼行，G43）→ klock_exit。打印必须整行包络：IRQ 语境 switches=N 行
///   （canary 带 \n）若撕进 SOAK 行中段会碎字段（`sw=5switches=300`）→
///   宿主正则失配漏拍 = 断言假阴性。
///   行 ≤192B（固定段 25B + 4 数值字段各至多 20 位 = 绝对界 105B）。
/// - 常驻不退出：活性心跳不是死线，超时醒 = 正常节拍（「服务端 wait 超时
///   是心跳不是死线」纪律的内核任务版）；私有表 + 哑 Event `Box::leak`
///   有意泄漏（有界、boot 生命周期）。醒因恒 Timeout（哑 Event 表私有、
///   无人 signal；非 Timeout = 跨表 wake 面破损 bug 路径）。
extern "C" fn task_soakmon(arg: usize) -> ! {
    use berry_sched::WakeReason;
    let period = 60 * berry_hal_aarch64::timer::cntfrq();
    let t0 = berry_hal_aarch64::timer::cntvct();
    let mut n: u64 = 0;
    loop {
        n += 1;
        // 首拍（n=1）→ t0+5*period；此后每拍 +1 周期（绝对网格，见任务注）。
        let deadline = t0 + (n + 4) * period;
        berry_hal_aarch64::kern_lock::klock_enter();
        let r = berry_sched::block_current(arg, SOAK_EV_BIT, deadline);
        // S-2 E0 包络升级（F33 同款，草案 §2.3）：醒因断言/计数器读在锁窗内
        //（block_current 返回前舞蹈已重取锁）；SOAK 行打印再包 with_print_lock
        //（G43 拼行——KERN→PRINT 单向嵌套合法，with_print_lock 在已屏蔽窗
        // 安全：had_irq=false 不误恢复）。
        assert!(r == WakeReason::Timeout,
                "soakmon: 醒因非 Timeout（哑 Event 永不置位——跨表 wake 面破损）");
        let sw = berry_sched::switches();
        let pages = mm::frames().allocated_pages() as u64;
        let lost = berry_hal_aarch64::uart::rx_lost() as u64;
        berry_hal_aarch64::kern_lock::with_print_lock(|| {
            berry_hal_aarch64::uart_puts(b"SOAK t=");
            berry_hal_aarch64::uart_put_dec(n);
            berry_hal_aarch64::uart_puts(b" sw=");
            berry_hal_aarch64::uart_put_dec(sw);
            berry_hal_aarch64::uart_puts(b" pages=");
            berry_hal_aarch64::uart_put_dec(pages);
            berry_hal_aarch64::uart_puts(b" lost=");
            berry_hal_aarch64::uart_put_dec(lost);
            berry_hal_aarch64::uart_puts(b"\n");
        });
        berry_hal_aarch64::kern_lock::klock_exit();
    }
}

/// 副核高半入口（S-2b，`docs/SMP-S2实施设计草案.md` §2.3）：secondary_entry
/// asm `.Lsmp_final` 经 `__secondary_main_ptr` 指针字 blr 进入——不归（`-> !`）。
/// `#[no_mangle]`：链接脚本 QUAD(secondary_main) 按名解析。
///
/// 起手序（每步都在 DAIF 全屏蔽窗内）：
/// 1. `daifset #0xf` 钉板——PSCI CPU_ON 出来的 DAIF 态不作假设（asm 期虽
///    未解蔽，钉死防未来 asm 改动漂移）；
/// 2. [`berry_hal_aarch64::trap::init`]：VBAR_EL1 per-CPU 复位 0——asm 期靠
///    屏蔽窗硬扛；参与环前必须重指高半 `__vectors`（enable_irq 后 CNTV →
///    exception_irq 经 VBAR 派发）；
/// 3. [`berry_hal_aarch64::gic::enable_cntv_this_core`]：本核 banked PPI27
///    使能（GICC PMR/CTLR 面 asm §3.2 ⑥ 已配）——GICD 全局面零触碰（S-1
///    纪律：副核绝不动 distributor 全局面）；SGI_RESCHEDULE banked 使能
///    同窗（S-3a——收端使能面收端自理）。
///
/// **统一探测/参与环**（两态合一，gate 检查在 on_irq_tick 早退——本环自身
/// 无 gate 分支）：gate 未置位 = 每 slice CNTV 唤醒 → on_irq_tick 锁窗内
/// 早退（klock 旋取即放）→ 回环重臂续睡 = 探测态纯空转，轮询粒度 = 1 slice
///（无 IPI（E0 代价），置位后 ≤1 slice 内开始拉任务）；gate 置位 = 同一环里
/// on_irq_tick 走 park 派发（cur==null 出选 `pinned!=0` 用户任务切入）或正常
/// preempt，任务尽则 block/exit/yield 的 park 分支切回本环（sec_park_ctx）
/// 续 wfi。wfi 真睡（TCG 实测 wfe 立返拖垮主核吞吐——S-1 铁律）。
#[no_mangle]
pub unsafe extern "C" fn secondary_main() -> ! {
    // S-2b 修复：换离低半早期栈（__boot_stack_top）到高半静态栈——必须在
    // 任何调用/IRQ 之前（本函数 `-> !`，旧帧弃置无害）。此后本核全部栈
    // 访问（探测环帧/tick IRQ 帧/park 存档帧）都在高半：主表与用户表均映，
    // park 帧恢复后读栈不再依赖低半映射（2b-2 退役；风暴雷详注见 smp.rs
    // `SMP_SEC_STACK`）。
    {
        let top = berry_hal_aarch64::smp::sec_stack_top();
        // SAFETY: 特权寄存器写；副核 DAIF 屏蔽窗内单线程；nostack——只改
        // 寄存器不访问内存（此刻旧栈仍在低半可用，但本指令不触栈）。
        unsafe { core::arch::asm!("mov sp, {0}", in(reg) top, options(nostack)) };
    }
    // SAFETY: 特权寄存器写；副核 DAIF 窗内单线程。
    // （排障期哨兵 enter/init ok 已随 C1 复活验证回收——bring-up 窗口 =
    // UART 单写者纪律窗，副核静默是 SMP ok 完整落行的前提，勿在此窗加打印。）
    unsafe {
        core::arch::asm!("msr daifset, #0xf", options(nostack, preserves_flags));
        berry_hal_aarch64::trap::init();
        berry_hal_aarch64::gic::enable_cntv_this_core();
        // S-3a：本核 SGI_RESCHEDULE(1) banked 补使能——CPU0 init 的
        // `ICENABLER0=!0` 只禁了 CPU0 bank，但副核 bank 复位态不可假设，
        // 显式使能 + 擦历史 pending（收端纪律：使能面收端自理）。
        berry_hal_aarch64::gic::enable_sgi_this_core(
            berry_hal_aarch64::gic::SGI_RESCHEDULE,
        );
    }
    loop {
        // SAFETY: 本核 CVAL 重臂（写 CNTV_CTL/CVAL 绝对截止）；DAIF 屏蔽窗内
        //（disable_irq/wfi 后或起手窗）。
        unsafe { berry_hal_aarch64::timer::arm_slice() };
        berry_hal_aarch64::enable_irq();
        berry_hal_aarch64::wfi(); // 真睡；CNTV IRQ → exception_irq → on_irq_tick
        berry_hal_aarch64::disable_irq();
        // park 派发/正常 preempt 都经 on_irq_tick 内 context_switch 发生；
        // 任务尽切回 sec_park_ctx 时续行点在其调用者栈帧——经 eret 回到本循环。
    }
}

/// S-2b 副核参与金丝雀探针（`docs/SMP-S2实施设计草案.md` §5.3，soakmon
/// envelope 变体）：每 2s 超时醒一次读 PAR 读数——`PAR ok c0=<n> c1=<n>` =
/// 「两核各自跑过 ≥2 个 EL0 用户任务 tick + 存在某一 tick 两核同时在跑
/// **不同** EL0 任务」（witness 粘性位，par_sample 在 on_irq_tick 锁窗内
/// 采样）→ 副核参与（E0「内核串行/用户并行」）的活跃证。
///
/// - 预算 = 120 轮 × 2s = 240s：条件满足即提前打点退出（典型路径远早于
///   预算尾——rootfs 相用户进程持续 churn 期间两核并发 EL0 是常态）；轮尽
///   仍不满足 = 打 `PAR absent c0=<n> c1=<n> w=<w>` 诚实缺证行后安静退出
///   （**不是破损证明**：裸相 EL0 用户进程一次性短命、耗尽后无 churn，
///   结构性不达条件——absent 是合法态。`PAR ok` 的机器断言归 smoke-smp
///   扩相（rootfs 相用户 churn 充足，破损时 ok 缺失 = harness FAIL loud）；
///   断言/panic 形态在此被明令禁止：裸相长跑（run-smp 交互/soak 类）跑过
///   预算即炸 = 时序竞速型假阴性（「标记不得建立在竞速上」纪律）。
/// - 醒因恒 Timeout（哑 Event 表私有、永不置位；非 Timeout = 跨表 wake 面
///   破损 bug 路径）。
/// - 包络纪律（soakmon 同款 F33）：klock_enter → block → 醒因断言 + 读数 +
///   整行打印（with_print_lock 拼行 G43）→ klock_exit。读数在锁窗内
///   （par_sample 写面同在 tick 锁窗——单写者序不赌跨核可见性时机）。
/// - 打点后 exit_current 退出（探针非常驻——与 soakmon 的差异：PAR 是一次性
///   验收断言，不是活性心跳；表/Event 泄漏同款有意）。
extern "C" fn task_par_probe(arg: usize) -> ! {
    use berry_hal_aarch64::smp::par_readout;
    use berry_sched::WakeReason;
    let period = 2 * berry_hal_aarch64::timer::cntfrq();
    let t0 = berry_hal_aarch64::timer::cntvct();
    let mut n: u64 = 0;
    loop {
        n += 1;
        let deadline = t0 + n * period;
        berry_hal_aarch64::kern_lock::klock_enter();
        let r = berry_sched::block_current(arg, PAR_EV_BIT, deadline);
        assert!(r == WakeReason::Timeout,
                "par_probe: 醒因非 Timeout（哑 Event 永不置位——跨表 wake 面破损）");
        let (c0, c1, witness, t0, t1) = par_readout();
        let (pre0, pre1, d1) = berry_hal_aarch64::smp::diag_readout();
        if n == 1 {
            // 早期诊断行（2s 档即出）：t1==0 = 副核 tick 链路死（对照 sS2b
            // 哨兵定位分段）；t1>0 && c1==0 = tick 活但派发/出选面问题。
            berry_hal_aarch64::kern_lock::with_print_lock(|| {
                berry_hal_aarch64::uart_puts(b"PAR diag t0=");
                berry_hal_aarch64::uart_put_dec(t0);
                berry_hal_aarch64::uart_puts(b" t1=");
                berry_hal_aarch64::uart_put_dec(t1);
                berry_hal_aarch64::uart_puts(b"\n");
            });
        }
        if witness != 0 && c0 >= 2 && c1 >= 2 {
            berry_hal_aarch64::kern_lock::with_print_lock(|| {
                berry_hal_aarch64::uart_puts(b"PAR ok c0=");
                berry_hal_aarch64::uart_put_dec(c0);
                berry_hal_aarch64::uart_puts(b" c1=");
                berry_hal_aarch64::uart_put_dec(c1);
                berry_hal_aarch64::uart_puts(b"\n");
            });
            berry_hal_aarch64::kern_lock::klock_exit();
            berry_sched::exit_current();
        }
        if n >= 120 {
            // 预算尽诚实缺证（见任务注：裸相结构性不达条件 = 合法 absent，
            // 非 FAIL；ok 断言归扩相 harness）。
            berry_hal_aarch64::kern_lock::with_print_lock(|| {
                berry_hal_aarch64::uart_puts(b"PAR absent c0=");
                berry_hal_aarch64::uart_put_dec(c0);
                berry_hal_aarch64::uart_puts(b" c1=");
                berry_hal_aarch64::uart_put_dec(c1);
                berry_hal_aarch64::uart_puts(b" w=");
                berry_hal_aarch64::uart_put_dec(witness);
                berry_hal_aarch64::uart_puts(b" t0=");
                berry_hal_aarch64::uart_put_dec(t0);
                berry_hal_aarch64::uart_puts(b" t1=");
                berry_hal_aarch64::uart_put_dec(t1);
                // S-2b 排障期诊断尾（格式冻结面之后追加，不进 harness 消费）：
                // pre0/pre1 = klock 前 tick 入口计数（pre1>t1 = 卡锁；同冻 =
                // CNTV/IRQ 链死）；d1 = 副核 park 派发计数。
                berry_hal_aarch64::uart_puts(b" pre0=");
                berry_hal_aarch64::uart_put_dec(pre0);
                berry_hal_aarch64::uart_puts(b" pre1=");
                berry_hal_aarch64::uart_put_dec(pre1);
                berry_hal_aarch64::uart_puts(b" d1=");
                berry_hal_aarch64::uart_put_dec(d1);
                berry_hal_aarch64::uart_puts(b"\n");
            });
            berry_hal_aarch64::kern_lock::klock_exit();
            berry_sched::exit_current();
        }
        berry_hal_aarch64::kern_lock::klock_exit();
    }
}

/// ai_pong（prio 32，bench 对 B 端）：fa_pong 同构、AI_ROUNDS 轮、不触
/// ipc_fast 三计数器断言（demo bench 与 fa 口径分离——fa 断言虽已先过，别混
/// 进 demo 轮次的计数）。
extern "C" fn task_ai_pong(arg: usize) -> ! {
    use crate::handle::SIGNAL_READABLE;
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_CLOCK_READ, NR_OBJECT_WAIT_ONE};
    let hb = arg as u32;
    let mut buf = [0u8; 8];
    for _ in 0..AI_ROUNDS {
        let mut now: u64 = 0;
        // SAFETY: 本任务内核栈局部变量，活过调用。
        let pnow = core::ptr::addr_of_mut!(now) as u64;
        assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "ai_pong: clock_read");
        let mut observed = 0u64;
        // SAFETY: 同上。
        let pobs = core::ptr::addr_of_mut!(observed) as u64;
        // 10s 帽兜底（fa_pong 同款：断链则 -ETIMEDOUT 断言失败 = smoke FAIL）。
        assert_eq!(
            svc6(NR_OBJECT_WAIT_ONE as u64, hb as u64, SIGNAL_READABLE,
                 now + 10_000_000_000, pobs, 0, 0),
            0,
            "ai_pong: wait READABLE"
        );
        assert!(observed & SIGNAL_READABLE != 0, "ai_pong: observed 须含 READABLE");
        let (mut n, mut nh): (u64, u64) = (8, 0);
        assert_eq!(
            svc6(NR_CHANNEL_RECV as u64, hb as u64, buf.as_mut_ptr() as u64,
                 core::ptr::addr_of_mut!(n) as u64, 0,
                 core::ptr::addr_of_mut!(nh) as u64, 0),
            0,
            "ai_pong: recv"
        );
        assert!(buf[..3] == *b"AIP", "ai_pong: 载荷须 AIP");
        buf[..3].copy_from_slice(b"AIR");
        assert_eq!(
            svc6(NR_CHANNEL_SEND as u64, hb as u64, buf.as_ptr() as u64, 4, 0, 0, 0),
            0,
            "ai_pong: 回件"
        );
    }
    berry_sched::exit_current()
}

/// ai_ping（prio 16，bench 对 A 端）：fa_ping 同构 + 收尾写 AI_RT_LAST →
/// signal E_done（bench 表 handle）通知 demo。RT 计量同 fa（CNTVCT 差累计/
/// 轮均）——bind 前后两轮同法，差分即 F9① 断言对象（先写 RT 再 signal：单
/// 核串行，demo 醒来读到的必是本轮值）。
extern "C" fn task_ai_ping(arg: usize) -> ! {
    use crate::handle::SIGNAL_READABLE;
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_OBJECT_SIGNAL, NR_OBJECT_WAIT_ONE};
    let ha = arg as u32;
    let h_done = (arg >> 32) as u32;
    let mut buf = [0u8; 8];
    let mut rt_ticks: u64 = 0;
    for _ in 0..AI_ROUNDS {
        buf[..4].copy_from_slice(b"AIP0");
        let t0 = berry_hal_aarch64::timer::cntvct();
        assert_eq!(
            svc6(NR_CHANNEL_SEND as u64, ha as u64, buf.as_ptr() as u64, 4, 0, 0, 0),
            0,
            "ai_ping: send（pong 在睡等 READABLE）"
        );
        let mut observed = 0u64;
        // SAFETY: 本任务内核栈局部变量，活过调用。
        let pobs = core::ptr::addr_of_mut!(observed) as u64;
        assert_eq!(
            svc6(NR_OBJECT_WAIT_ONE as u64, ha as u64, SIGNAL_READABLE, 0, pobs, 0, 0),
            0,
            "ai_ping: wait poll 命中（断链 -EAGAIN 即 FAIL）"
        );
        let (mut n, mut nh): (u64, u64) = (8, 0);
        assert_eq!(
            svc6(NR_CHANNEL_RECV as u64, ha as u64, buf.as_mut_ptr() as u64,
                 core::ptr::addr_of_mut!(n) as u64, 0,
                 core::ptr::addr_of_mut!(nh) as u64, 0),
            0,
            "ai_ping: recv 回件"
        );
        assert!(buf[..3] == *b"AIR", "ai_ping: 载荷须 AIR");
        rt_ticks += berry_hal_aarch64::timer::cntvct() - t0;
    }
    AI_RT_LAST.store(rt_ticks / AI_ROUNDS, core::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, h_done as u64, 0, AI_EV_DONE_BIT),
        0,
        "ai_ping: signal E_done"
    );
    berry_sched::exit_current()
}
