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
mod dpg; // G4-1：demand paging 内核机制自喂闭环（docs/G4按需调页实施设计草案.md）
mod event;
mod handle;
mod handoff; // V-0b 刀 3：BerryBoot 交接协议双镜像（const assert 执法，消费随刀 4 起）
mod heap;
mod ktest;
mod mem; // GUI-0：本仓 mem* 整数版覆写（内核 SIMD-free 合同 builtins 半边）
mod mm;
mod pi;
mod process;
mod suspend; // 1.1/S3-a：冻结令牌对象（process_suspend handler 配对）
mod syscall;
mod sysver; // VER：版本身份 VMO（运行时查询面——boot 编译期成像 + 第 9 插只读交付）
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
    // 语义声明〔f4 F9〕：SMP 下 halt 只停当核——fail-loud 是消息级非系统
    // 级，他核续跑直至外部收杀（G3-4b 首跑双 panic ~60s 间隔实录）；系统
    // 级停机不归本 handler（后续 kill criteria 面）。
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

/// GUI P2：从 fw_cfg `opt/berry/display` 读 `WxH[@Hz]`；缺席/畸形 = None。
/// 只接受 ASCII 十进制；字符串项可能带 NUL/空白，统一 trim 后再拆。
fn display_mode_req() -> Option<(u32, u32)> {
    use berry_hal_aarch64::fwcfg;
    let e = fwcfg::find_file("opt/berry/display")?;
    if e.size == 0 || e.size > 32 {
        return None;
    }
    let mut buf = [0u8; 32];
    if !fwcfg::read_to(&e, &mut buf[..e.size as usize]) {
        return None;
    }
    let s = core::str::from_utf8(&buf[..e.size as usize]).ok()?;
    let s = s.trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
    let s = s.split('@').next()?;
    let (w, h) = s.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
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
    // VER-01 第一档：构建标识——版本（Cargo workspace version）+ git 短哈希
    // + dirty 位（build.rs 注入）。marker "BerryOS boot" 子串保持冻结（追加
    // 后缀不前置——smoke -qF 零漂移）。
    berry_hal_aarch64::uart_puts(b"BerryOS boot ");
    berry_hal_aarch64::uart_puts(env!("CARGO_PKG_VERSION").as_bytes());
    berry_hal_aarch64::uart_puts(b" (");
    berry_hal_aarch64::uart_puts(env!("BERRY_GIT_IDENT").as_bytes());
    // 构建元数据（VER §1）：工具链 + profile——行为异常时排除工具链变量。
    berry_hal_aarch64::uart_puts(b" rustc");
    berry_hal_aarch64::uart_puts(option_env!("BERRY_RUSTC_VER").unwrap_or("?").as_bytes());
    if cfg!(debug_assertions) {
        berry_hal_aarch64::uart_puts(b" dbg)\n");
    } else {
        berry_hal_aarch64::uart_puts(b" rel)\n");
    }

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
    // sim-S3（PM-16 §4.4 步3）：PSCI 深睡能力探测——EFI/EL2/TCG/KVM 四路
    // 同点打 `PSCI susp=`（EL1 路前置 `PSCI version 0x…` conduit 活证行）。
    // **必须在 ktest::run 之前**：PWRDEG 断言读缓存；probe 只读 boot EL 静态
    // 量 + 至多两条 HVC，对后续 boot 链零扰动。
    berry_hal_aarch64::psci::probe(efi_boot);
    // GUI-1：GOP 事实五域钉存（V-6a 只打点，GUI-1 起消费——fb VMO 授能块
    // 在 rootfs/virtio 插表段；handoff @下文 move 进 mm::init 前取走，
    // `efi_boot` 同款先例）。
    let mut gop_saved: Option<(u64, u32, u32, u32, u32)> = None;
    // GUI P2：ramfb 手机档成功时置 1——随 fbinfo 第 6 域交 guifbd，
    // GUI 模式 console 只走串口/记账，不上屏盖桌面。
    let mut gui_mode = false;
    // 540×1200 等超出固件 GOP 缓冲的档：boot 先记 pending，mm::init 后从
    // buddy 取连续块、重编程 ramfb；失败回退固件几何。
    let mut pending_mode: Option<(u32, u32)> = None;
    let print_fb = |g: (u64, u32, u32, u32, u32)| {
        berry_hal_aarch64::uart_puts(b"FB ok ");
        berry_hal_aarch64::uart_put_dec(g.1 as u64);
        berry_hal_aarch64::uart_puts(b"x");
        berry_hal_aarch64::uart_put_dec(g.2 as u64);
        berry_hal_aarch64::uart_puts(b" fmt=");
        berry_hal_aarch64::uart_put_dec(g.4 as u64);
        berry_hal_aarch64::uart_puts(b" stride=");
        berry_hal_aarch64::uart_put_dec(g.3 as u64);
        berry_hal_aarch64::uart_puts(b" fb=");
        berry_hal_aarch64::uart_put_hex64(g.0);
        berry_hal_aarch64::uart_puts(b"\n");
    };
    if let Some(h) = handoff {
        berry_hal_aarch64::uart_puts(b"HANDOFF ok ver=");
        berry_hal_aarch64::uart_put_dec(h.version as u64);
        berry_hal_aarch64::uart_puts(b"\n");
        // V-6a：GOP 携面消费（BerryBoot banner 期 QueryMode → handoff
        // @56..@76；fbcon 数据源先行到内核——打点即「事实到内核」证据，
        // MMIO 窗授能/fbcon 服务随后刀）。零值 = 无此物：QEMU `-kernel`
        // 路与无显示设备 EFI 路恒缺席，零扰动（EFI 路专属行，同 HANDOFF）。
        if let Some(g) = handoff::gop_info(h) {
            // GUI P2：可选启动模式请求（`-fw_cfg name=opt/berry/display,
            // string=480x1000@60`）。一次性把 ramfb scanout 切到手机档；
            // 不每帧 commit——`berry shell` 直接读 memory-backend-file
            // 的 RAM 镜（宿主共享 RAM 面）。缺席 = 原 800×600 零扰动。
            let (mut gw, mut gh, mut gs) = (g.width, g.height, g.stride);
            let mut need_alloc = false;
            if let Some((w, h)) = display_mode_req() {
                let stride = w.saturating_mul(4);
                let cap = (g.stride as u64) * (g.height as u64);
                let need = (stride as u64) * (h as u64);
                if w == 0 || h == 0 {
                    berry_hal_aarch64::uart_puts(b"DISP fail\n");
                } else if need <= cap {
                    if berry_hal_aarch64::fwcfg::ramfb_set(g.fb_pa, w, h, stride) {
                        gw = w;
                        gh = h;
                        gs = stride;
                        gui_mode = true;
                        berry_hal_aarch64::uart_puts(b"DISP ok ");
                        berry_hal_aarch64::uart_put_dec(gw as u64);
                        berry_hal_aarch64::uart_puts(b"x");
                        berry_hal_aarch64::uart_put_dec(gh as u64);
                        berry_hal_aarch64::uart_puts(b"\n");
                    } else {
                        berry_hal_aarch64::uart_puts(b"DISP fail\n");
                    }
                } else {
                    pending_mode = Some((w, h));
                    need_alloc = true;
                    berry_hal_aarch64::uart_puts(b"DISP defer ");
                    berry_hal_aarch64::uart_put_dec(w as u64);
                    berry_hal_aarch64::uart_puts(b"x");
                    berry_hal_aarch64::uart_put_dec(h as u64);
                    berry_hal_aarch64::uart_puts(b"\n");
                }
            }
            gop_saved = Some((g.fb_pa, gw, gh, gs, g.format));
            if !need_alloc {
                print_fb((g.fb_pa, gw, gh, gs, g.format));
            }
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

    // GUI P2：超出 GOP 缓冲的模式（如 540×1200）在 buddy 就绪后取连续块，
    // 重编程 ramfb；失败回退固件几何并补 FB ok（host 只等这一行）。
    if let Some((w, h)) = pending_mode {
        let stride = w.saturating_mul(4);
        let bytes = (stride as u64) * (h as u64);
        let pages = ((bytes + 0xFFF) / 0x1000) as usize;
        let order = pages.next_power_of_two().trailing_zeros() as usize;
        let mut done = false;
        if let Ok(pfn) = mm::frames().alloc_order(order) {
            let pa = (pfn << 12) as u64;
            if berry_hal_aarch64::fwcfg::ramfb_set(pa, w, h, stride) {
                let fmt = gop_saved.map(|g| g.4).unwrap_or(1);
                gop_saved = Some((pa, w, h, stride, fmt));
                gui_mode = true;
                done = true;
                berry_hal_aarch64::uart_puts(b"DISP ok ");
                berry_hal_aarch64::uart_put_dec(w as u64);
                berry_hal_aarch64::uart_puts(b"x");
                berry_hal_aarch64::uart_put_dec(h as u64);
                berry_hal_aarch64::uart_puts(b"\n");
                print_fb((pa, w, h, stride, fmt));
            } else {
                let _ = mm::frames().free(pfn, order);
            }
        }
        if !done {
            berry_hal_aarch64::uart_puts(b"DISP fail\n");
            if let Some(g) = gop_saved {
                print_fb(g);
            }
        }
    }

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

    //（S-2a E0 历史注：sched 舞蹈锁原语槽 set_klock_ops 在下方 park 块前
    // 安装；同批的「①位 置/清器 KLOCK_FLAG 槽」已随 S-4 判例 klock_enter
    // 改道全谱 SVC 窗退役——位面收编 svc_window，见 kern_lock.rs klock_enter 注。）
    // S-3c c-2c slice 2b（3-1b 翻终态正序）：SVC 入口**结构锁窗**槽——HAL
    // 侧结构先行、KL 类内末取/先放（终态全局序 KL 最内，七层冻结序
    // SQ>队列>REAPQ>表），结构位集（SQ→RQ0→
    // RQ1→REAPQ 升序）物理预取 + held 记账 + svc_mask 声明经此回调；handler
    // 内层窗由此全退化。CURRENT 空（ktest/boot）跳过 = ktest 期 SVC 形态
    // 不变（KL 由 HAL 直取恒配平）。fault 面（trap.rs svc_enter 全量形）
    // 同槽同构。
    berry_hal_aarch64::kern_lock::set_svc_window_hook(berry_sched::svc_window);
    // 跨 crate 位对齐编译期执法：HAL 类常量位值 ≡ sched HELD_* 组合——ALL
    // 钉并集 + 中间三档逐档钉组合（保并集的位重排同样编译失败，防 HAL KL
    // 门与 sched 结构锁取放对同一 bit 语义分叉）。
    const _: () = assert!(
        berry_hal_aarch64::kern_lock::SVC_CLASS_ALL
            == (berry_sched::task::HELD_SQ
                | berry_sched::task::HELD_RQ0
                | berry_sched::task::HELD_RQ1
                | berry_sched::task::HELD_REAPQ
                | berry_sched::task::HELD_TABLE)
    );
    const _: () = assert!(
        berry_hal_aarch64::kern_lock::SVC_CLASS_TABLE == berry_sched::task::HELD_TABLE
    );
    const _: () = assert!(
        berry_hal_aarch64::kern_lock::SVC_CLASS_RQ_TABLE
            == (berry_sched::task::HELD_RQ0
                | berry_sched::task::HELD_RQ1
                | berry_sched::task::HELD_TABLE)
    );
    const _: () = assert!(
        berry_hal_aarch64::kern_lock::SVC_CLASS_SQ_RQ_TABLE
            == (berry_sched::task::HELD_SQ
                | berry_sched::task::HELD_RQ0
                | berry_sched::task::HELD_RQ1
                | berry_sched::task::HELD_TABLE)
    );
    // S-3c slice 3-1a：无表档首类（0x37 降档）——同法逐位钉（表位缺席
    // 同样编译失败，防位集漂移）。
    const _: () = assert!(
        berry_hal_aarch64::kern_lock::SVC_CLASS_SQ_RQ
            == (berry_sched::task::HELD_SQ
                | berry_sched::task::HELD_RQ0
                | berry_sched::task::HELD_RQ1)
    );
    berry_sched::set_klock_ops(berry_sched::KlockOps {
        kern_acquire: berry_hal_aarch64::kern_lock::kern_acquire,
        kern_release: berry_hal_aarch64::kern_lock::kern_release,
        print_lock: berry_hal_aarch64::kern_lock::print_lock,
        print_unlock: berry_hal_aarch64::kern_lock::print_unlock,
    });

    // ---- AM-2b（候选①·裁决①）：系统压力 Event + 窗口聚合接线。先于
    // ktest::run（AM2B-K 直调 fold 与真 SVC 清位）与 dpg 计值点
    //（sched::init/dpg::init 均在后）。Event 落内核 bootstrap 表（EL1
    // tag=0 面——ktest svc6 真派发经此表解析）；ObjRef retain ×2：一枚 =
    // 内核静态钉住（fold 出口写位，RX_EV 同款 :845-846 锚——永不
    // release），一枚 = 待移交 svcmgr 表的 dup 令牌（boot fb 段 insert_ref
    // 接管，handle.rs「调用方移交的引用令牌」契约）。窗口长 70ms 按
    // CNTFRQ 换算周期数（裁决②——TCG 频偏，禁编译期常量）。----
    let h_am2b = syscall::bootstrap_table()
        .insert(
            handle::KernelObj::Event(crate::event::Event::new()),
            handle::RIGHT_SIGNAL,
        )
        .expect("main: AM-2b 压力 Event 进 bootstrap 表");
    let p_am2b = syscall::bootstrap_table()
        .get_raw(h_am2b, handle::ObjType::Event, 0)
        .expect("main: 取 AM-2b 压力 Event ObjRef")
        .0;
    // SAFETY: boot 单线程；刚 insert + get_raw 过（在表、类型 Event）——
    // retain ×2（静态钉住 + svcmgr dup 令牌）；静态槽写先于一切读者
    //（RX_EV_H 口径）。handle 本体不留静态槽：ktest 期 WAKEP 清场会 drain
    // bootstrap 表作废槽位（实踩），SVC 清位面改 ktest 现插现关
    //（`am2b_pressure_ktest_handle`）。
    unsafe {
        (*p_am2b.as_ptr()).retain();
        (*p_am2b.as_ptr()).retain();
        *core::ptr::addr_of_mut!(PRESSURE_EV_REF) = Some(p_am2b);
    }
    crate::telemetry::dpg_win_init(
        AM2B_WIN_MS,
        berry_hal_aarch64::timer::cntfrq(),
        berry_hal_aarch64::timer::cntvct(),
        pressure_partial_cross,
    );

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
    // G3-1a 主位 Some 臂直测（f4 M2 修复批）：ktest::run（:554）先于
    // cpu0_init（:583）——其五断言走的 static 回落位；此处 TPIDR_EL1 已钉，
    // 主位（SmpCpu.preempt_count@TPIDR）一次读断言补齐。静默执法（fail =
    // panic 停机冒烟自然红），零输出行保 boot 相逐行零漂移。
    assert_eq!(
        berry_sched::preempt_count(),
        0,
        "G3-1a：主位 Some 臂读通（cpu0_init 后恒 0——无人开窗）"
    );

    // G3-3 lockdep 阳性自证（g3-3b）：TPIDR 已钉、链净态；活性（真链全序
    // 八连推弹走本体）+ 合成违约阳性对照（证明「零 VIOLATION」≠「检测器
    // 死了」）。marker `LOCKDEP selftest ok` = feature 镜像专属行（default
    // 面编译消除零漂移）；FAIL 不停机——冒烟 grep 自然红。
    #[cfg(feature = "lockdep")]
    berry_hal_aarch64::lockdep::selftest();

    // v1.3 §7 EAS：EM 表形状注册——QEMU virt 手动默认表（真机 DT/OPP 解析
    // 后由 boot 换注重注册；算法面留 M2/真机）。ktest 后调 = ktest 自管
    // 注册面不被预填污染（EM-0 未注册态断言）；boot DAIF 屏蔽窗内单写者
    //（EM_TABLE static mut 契约同 BANDS）。
    assert!(berry_sched::em_register_qemu_virt(), "EM：QEMU virt 默认表注册");

    // IRQ 此刻仍屏蔽（boot DAIF）；spawn 要求屏蔽态（alloc/enqueue 不可打断）。
    berry_sched::init();
    // 3-4c 遥测环：注册切换钩子（init 后、首个切换前——rust_main 尾部的
    // yield_now 即产生首条记录；watchdog 断言环在涨 + vct 单调）。
    telemetry::init();
    // 4-3b PI：注册 SLEEPQ 变更钩子（block 睡下后 / wake 摘队后全图重推；
    // 与 telemetry 同位——均须在首个切换前）。
    pi::init();
    berry_sched::spawn_as_spread(
        task_a,
        TASK_A_MAGIC as usize,
        b"task_a",
        as_a.ttbr0_pa,
        as_a.asid,
    );
    berry_sched::spawn_as_spread(
        task_b,
        TASK_B_MAGIC as usize,
        b"task_b",
        as_b.ttbr0_pa,
        as_b.asid,
    );

    // G4-1：demand paging 内核机制自喂闭环（内核任务 faulter/pager 对——无
    // EL0 pager、无 0x21；marker `DPG ok` = smoke 裸相第 28 标记）。
    dpg::init();

    // ---- 方案 B（调点调度-v1.3 §9 留刀 / 调度器设计-v1.1 §测量基础设施）：
    // rwake 真 sleep/wake 唤醒基准——HDR 第二个真实消费者（首个 = ktest
    // test_hdr_wake 方案 A 忙等；本任务是它的真睡相：`block_current(0,0,
    // deadline)` 纯睡 + on_irq_tick 超时扫描唤醒，**IRQ 唤醒而非忙等** =
    // cyclictest 真口径）。门控同 rt_probe 形（R-1 判例：boot 相 bench 开销
    // 秒级，「不进 smoke 需要机制不是自律」）：fw_cfg 具名键 `opt/berry/rwake`
    // 在位才 spawn——缺席 = run/smoke/boot 相逐行零扰动（EFI 路无 fw_cfg 具名
    // 文件 = 天然缺席）；也不进 markers.sh（gated 非 smoke 面，RT ok 同族）。
    // 位置约束：ktest 后、首个用户态 spawn 前（任务书口径）；sched::init 后
    // + boot DAIF 屏蔽窗内（enable_irq 前——全体 spawn 先例共同约束）。----
    if berry_hal_aarch64::fwcfg::find_file(RWAKE_PROBE_NAME).is_some() {
        // prio 8（rt_probe/soakmon 同档裁决）：全体常驻（矩阵/console/
        // watchdog/EL0 用户全 0）之上——醒后即时出选续行（被测的唤醒延迟
        // 不掺轮转排队）；瞬态 boot bench 对（fa/ai 16/32）之下——结构性
        // 让位 = 不 preempt 它们的 RT 测量窗（rt_probe 首跑 prio 24 一次醒
        // 来就把 AIDEMO rt2 顶到 rt1×4 撞容差判例）。
        berry_sched::spawn_with(task_bench_rwake, 0, b"rwake", 8);
    }

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
                    // 几何 info VMO：48B = fb_pa/w/h/stride/fmt/gui 六域 u64 LE
                    //（一页 VMO，create+commit+write 三连——rootfs VMO 同款）。
                    let mut info = [0u8; 48];
                    info[0..8].copy_from_slice(&fb_pa.to_le_bytes());
                    info[8..16].copy_from_slice(&(w as u64).to_le_bytes());
                    info[16..24].copy_from_slice(&(h as u64).to_le_bytes());
                    info[24..32].copy_from_slice(&(stride as u64).to_le_bytes());
                    info[32..40].copy_from_slice(&(fmt as u64).to_le_bytes());
                    info[40..48].copy_from_slice(&(gui_mode as u64).to_le_bytes());
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
                    // AM-2b 裁决①：压力 Event dup 进 svcmgr 表**第 8 插定值
                    // 0x0001_0008**（console 1/rootfs 2/endB2 3/virtio 4/
                    // fb VMO 5/fbinfo 6/endB_fb 7 之后——GOP 相位内表序确定，
                    // boot assert 拦漂移；裸相/rootfs-无-fb 相缺席 = svcmgr
                    // 查表 -EBADF，同 fb/virtio 惯例——am 未成形，env 块
                    // extras 迁交随消费链段）。授 SIGNAL|DUPLICATE
                    //（`event_create` 出生授同口径，abi.toml 0x54 note——
                    // am 持 SIGNAL 右 `object_signal` 自清位 + svcmgr 需
                    // DUPLICATE 再迁）。令牌 = boot 接线段第二枚 retain
                    //（insert_ref 接管既有令牌、不 +1——handle.rs 契约）。
                    let h_am2b_svc = ht_svcmgr
                        .insert_ref(p_am2b, handle::RIGHT_SIGNAL | handle::RIGHT_DUPLICATE)
                        .expect("main: AM-2b 压力 Event dup 进 svcmgr 表");
                    assert_eq!(
                        h_am2b_svc, 0x0001_0008,
                        "main: svcmgr 表 AM-2b 压力 Event 须第 8 插 0x0001_0008（编译侧硬编码）"
                    );
                    // VER 运行时查询面：boot 建只读 version VMO——1 页版本身份
                    // 结构体（sysver 模块编译期 env! 成像，零运行时取数；fbinfo
                    // VMO 同款 create+commit+write 三连）插 svcmgr 表**第 9 插
                    // 定值 0x0001_0009**（与第 8 插同 fb GOP 相位同分支，表序
                    // 确定——boot assert 拦漂移；裸相/rootfs-无-fb 相缺席 =
                    // svcmgr 查表 -EBADF，同 fb/virtio 惯例）。rights 仅
                    // READ——版本面禁写（vmo_write 放宽面拒；权限在 handle
                    // 不在对象），内核侧不留柄（boot 即写即交，无 dup 消费面）。
                    let h_sysver = ht_svcmgr
                        .insert(
                            handle::KernelObj::Vmo(
                                sysver::build_vmo().expect("main: version Vmo 装配（create/commit/write）"),
                            ),
                            handle::RIGHT_READ,
                        )
                        .expect("main: version VMO 进 svcmgr 表");
                    assert_eq!(
                        h_sysver, 0x0001_0009,
                        "main: svcmgr 表 version VMO 须第 9 插 0x0001_0009（编译侧硬编码）"
                    );
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
    // X-3c-1：svcmgr pager 任务（exec demand 装载收货面）——共享同
    // ttbr0/asid/句柄表：寄存表即 svcmgr .bss 静态（PAGER_REQS），零 IPC。
    // 入口/栈顶 = svcmgr link-user.ld 钉位（.pager_entry@0x1E000 /
    // .pager_stack 顶 0x20000——**同值双改**）；x0=0（pager 无 console 面，
    // 入口不读参数）。boot 即在（svcmgr _start 装钩子前空转小睡，无害）。
    berry_sched::spawn_user(0x1E_000, 0, 0x20_000, b"user_svcmgr_pager",
                          up_svcmgr.ttbr0_pa, up_svcmgr.asid,
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

    // ---- G3-4（docs/G3全抢占实施设计草案.md §4）：bench-lat IPC 路门控 spawn ----
    // wake→run 段补盲（草案 §4 暂名 bench-lat）两路之一：IPC send 路 =
    // **内核任务对**（侦察定谳：SGI 路须 EL0 用户对——place() 内核任务恒
    // CPU0 + CPU1 出选 = user_only 臂（pinned!=0），内核对跨核架构堵死；
    // IPC 路同核，内核对零 SVC 噪声、rt_probe 确定性同款）。waker 经
    // `wake_matching` 直驱（IPC send 尾的唤醒面本体——send 路在 wake 前的
    // 拷贝/入队机件不在被测的 wake→run 段内，直驱 = 段的隔离测量）。
    // 门控同 rt_probe 形：fw_cfg `opt/berry/lat`（bench.py --lat 专递）在位
    // 才 spawn，缺席 = run/smoke 零扰动；EFI 路天然缺席。
    if berry_hal_aarch64::fwcfg::find_file(LAT_PROBE_NAME).is_some() {
        let ht_lat: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        // 表内仅一个握手 Event（rt_probe 表同形：私有表零跨表错捐面；
        // Event 态无人读写——纯地址作 wait_obj 匹配键 + LAT_EV_BIT 按位唤醒）。
        let h_lat = ht_lat
            .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
            .expect("main: lat 握手 Event 进 lat 表");
        let p_lat = ht_lat
            .get_raw(h_lat, ObjType::Event, 0)
            .expect("main: 取 lat ObjRef")
            .0;
        // sleeper 先 spawn（prio 8 先被 pick → 先睡下，waker 首轮间隔睡前
        // 必已在队）；waker prio 0——P-B 直切面（G3-2 段①）恰由 8>0 触发，
        // 每轮 wake_matching 后 sleeper 即时抢占续行 = 被测路径本体。
        berry_sched::spawn_with_tag(task_lat_slp, p_lat.as_ptr() as usize, b"lat_slp", 8,
                                    ht_lat as *mut HandleTable as usize);
        berry_sched::spawn_with_tag(task_lat_wak, p_lat.as_ptr() as usize, b"lat_wak", 0,
                                    ht_lat as *mut HandleTable as usize);
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
        // G3-5 候选① C 形（常驻再触发）：soak∧lat 双门都在位 → 装两路
        // 触发 Event（每路一个——恰一等待者/位，杜绝共享事件双等待者的
        // 清位竞速）。复用本表 boot 单例（+2 Event 句柄，有界）；地址置
        // 静态供 waker 侧读。顺序安全：本块微秒级落笔，两路 waker 的静态
        // 读点在各自 boot 周期（秒级）之后——waker prio 0，init 直通本块
        // 不 block = 结构性无竞速。自证行 `LAT trig armed`（门键漂移 =
        // 常驻面静默退化一次性，宿主 COVER + 恰 N+1 行断言双抓）。
        if berry_hal_aarch64::fwcfg::find_file(LAT_PROBE_NAME).is_some() {
            let h_trig_ipc = ht_soak
                .insert(KernelObj::Event(crate::event::Event::new()),
                        handle::RIGHT_SIGNAL)
                .expect("main: lat 触发 Event(ipc) 进 soak 表");
            let p_trig_ipc = ht_soak
                .get_raw(h_trig_ipc, ObjType::Event, 0)
                .expect("main: 取 lat 触发 ObjRef(ipc)")
                .0;
            LAT_TRIG_IPC_OBJ.store(p_trig_ipc.as_ptr() as usize,
                                   core::sync::atomic::Ordering::Release);
            let h_trig_sgi = ht_soak
                .insert(KernelObj::Event(crate::event::Event::new()),
                        handle::RIGHT_SIGNAL)
                .expect("main: lat 触发 Event(sgi) 进 soak 表");
            let p_trig_sgi = ht_soak
                .get_raw(h_trig_sgi, ObjType::Event, 0)
                .expect("main: 取 lat 触发 ObjRef(sgi)")
                .0;
            LAT_TRIG_SGI_OBJ.store(p_trig_sgi.as_ptr() as usize,
                                   core::sync::atomic::Ordering::Release);
            berry_hal_aarch64::uart_puts(b"LAT trig armed\n");
        }
    }

    // 注册回调（HAL→sched 反转，避循环依赖；均须在 enable_irq / 首个用户任务被
    // 调度之前）。IRQ 槽驱动抢占；用户故障槽驱动 G1.5 kill；SGI 槽（义务⑧
    // S-3b b-1）驱动 CPU0 快速重调度（跨核唤醒不等整 tick）。刀1：tick 槽外套
    // 计数包装 [`irq_tick_acct`]（TIMER 到达 + 派发戳入 per-source 表）——转发
    // 原 on_irq_tick，语义零漂移。
    berry_hal_aarch64::set_irq_dispatch(irq_tick_acct);
    berry_hal_aarch64::set_sgi_dispatch(berry_sched::on_sgi_resched);
    berry_hal_aarch64::set_user_fault_exit(berry_sched::user_fault_exit);
    // G3-1b 落点①接线：syscall 返回边界检查点（SVC 派发尾空探；sched 侧
    // 自带 irqsave 包络）。零行为变更步——注册与否不改变现役行为（探峰恒空），
    // g3-1c 域解蔽后承重。
    berry_hal_aarch64::set_svc_exit_checkpoint(berry_sched::preempt_checkpoint);
    // G3-1c-1 接线（console 写域解蔽）：with_print_lock 窗包络从 irqsave 换
    // preempt 计数对——enter/+1，exit = 配平 + 按 DAIF 分诊检查点（解蔽窗
    // 出口探峰兑现被双门挡下的切换诉求；屏蔽窗出口跳检查点，零行为差）。
    // 须在 enable_irq 前（首个解蔽窗打印前）注册。
    berry_hal_aarch64::kern_lock::set_preempt_envelope(
        berry_sched::print_window_enter,
        berry_sched::print_window_exit,
    );
    // P5 A-1：RX 通知槽 + 使能 PL011 RX（SPI 33）——均在 DAIF 屏蔽窗内
    // （enable_irq 前）。次序硬约束：通知槽须先注册（槽空时 rx_irq 平凡
    // 跳过——首字节静默落环无人唤醒）；rx_init 的 IMSC.RXIM 最后开（线一拉
    // 高 pending 即真数据，此前 GIC/粘滞已清干净）。
    berry_hal_aarch64::uart::set_rx_notify(rx_irq_notify);
    // SAFETY: MMIO 写；boot 单线程、DAIF 屏蔽。
    unsafe { berry_hal_aarch64::uart::rx_init() };
    // sim-S1（`docs/sim设备通道与契约设计-v1.1.md` §4）：UART1 注入通道全链
    // 接线——**在席门 = fw_cfg 具名键 `opt/berry/simdev`**（GUI P2
    // `opt/berry/display` 同款拓扑真值纪律：justfile 同一行配第二 `-serial`
    // 与此键，键缺 = 设备不存在——0x0904_0000 是洞，盲配即 external abort
    // boot 崩；EFI 路无具名键面 = 门恒闭零扰动；既有 run/smoke 不带键零漂移）。
    // 门开四件（全在 DAIF 屏蔽窗内）：
    // ① simbattery 礼物对进 svcmgr 表（帧 RECV **0x0001_0005** / 回执 SEND
    //   **0x0001_0006**——-kernel rootfs 相专属槽位：EFI-GOP 相 5/6/7 被
    //   fb/info/endB_fb 占（上方块），而 fw_cfg 键在 EFI 不可见 = 两相结构性
    //   互斥；assert = 并存绊网——真并存即拓扑互斥裁决破产，boot 期炸响）；
    // ② task_simcon 私有表（Event 1 / 帧 SEND 2 / 回执 RECV 3——console
    //   私有表同款错捐断根）+ 双通道 pair（endK2/endB2 同款：两端异地入表、
    //   配对延到后插入侧；boot 单线程中间无人能发）+ 静态槽/retain 钉住；
    // ③ 通知槽先注册（rx_notify 同款次序硬约束——槽空时首字节静默落环）；
    // ④ spawn + `uart1_init`（IMSC.RXIM 最后开——线一拉高即真数据，等待者
    //   与任务须已就位）。
    if rootfs_present
        && berry_hal_aarch64::fwcfg::find_file(SIM_DEV_KEY).is_some()
    {
        // ① simbattery 礼物对（svcmgr 表，rootfs 相槽 5/6——simbattery 由
        // svcmgr spawn 时经 SpawnSpec stdin/extra_gifts 领走，M2 后段接线）。
        let h_simb_rx = ht_svcmgr
            .insert(
                KernelObj::Channel(
                    crate::channel::ChannelEnd::try_unpaired(true)
                        .expect("main: 建 sim 帧通道 simbattery 端"),
                ),
                handle::RIGHT_RECV,
            )
            .expect("main: sim 帧 RECV 端进 svcmgr 表");
        assert_eq!(
            h_simb_rx, 0x0001_0005,
            "main: svcmgr 表 sim 帧 RECV 须 0x0001_0005（-kernel 相第 5 插；EFI-GOP 相此槽 = fb VMO——并存即互斥裁决破产）"
        );
        let h_simb_tx = ht_svcmgr
            .insert(
                KernelObj::Channel(
                    crate::channel::ChannelEnd::try_unpaired(true)
                        .expect("main: 建 sim 回执通道 simbattery 端"),
                ),
                handle::RIGHT_SEND,
            )
            .expect("main: sim 回执 SEND 端进 svcmgr 表");
        assert_eq!(
            h_simb_tx, 0x0001_0006,
            "main: svcmgr 表 sim 回执 SEND 须 0x0001_0006（-kernel 相第 6 插）"
        );
        // ② simcon 私有表三插（表序定值断言惯例——错值 boot 期炸响）。
        let ht_sim: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        let h_ev = ht_sim
            .insert(
                KernelObj::Event(crate::event::Event::new()),
                handle::RIGHT_SIGNAL,
            )
            .expect("main: UART1 Event 进 simcon 表");
        assert_eq!(h_ev, 0x0001_0001, "main: simcon 表 Event 须首插 0x0001_0001");
        let h_tx = ht_sim
            .insert(
                KernelObj::Channel(
                    crate::channel::ChannelEnd::try_unpaired(true)
                        .expect("main: 建 sim 帧通道 simcon 端"),
                ),
                handle::RIGHT_SEND,
            )
            .expect("main: sim 帧 SEND 端进 simcon 表");
        assert_eq!(h_tx, 0x0001_0002, "main: simcon 表帧 SEND 须 0x0001_0002");
        let h_ack = ht_sim
            .insert(
                KernelObj::Channel(
                    crate::channel::ChannelEnd::try_unpaired(true)
                        .expect("main: 建 sim 回执通道 simcon 端"),
                ),
                handle::RIGHT_RECV,
            )
            .expect("main: sim 回执 RECV 端进 simcon 表");
        assert_eq!(h_ack, 0x0001_0003, "main: simcon 表回执 RECV 须 0x0001_0003");
        // ObjRef 四取：Event（IRQ 通知/清位共用）+ simcon 双端（pair 用）+
        // svcmgr 双端（pair 对侧）。get_raw 即过在表/类型门控。
        let (p_ev, p_tx, p_ack) = (
            ht_sim
                .get_raw(h_ev, handle::ObjType::Event, 0)
                .expect("main: 取 sim Event ObjRef")
                .0,
            ht_sim
                .get_raw(h_tx, handle::ObjType::Channel, 0)
                .expect("main: 取 sim 帧 SEND ObjRef")
                .0,
            ht_sim
                .get_raw(h_ack, handle::ObjType::Channel, 0)
                .expect("main: 取 sim 回执 RECV ObjRef")
                .0,
        );
        let (p_b_rx, p_b_tx) = (
            ht_svcmgr
                .get_raw(h_simb_rx, handle::ObjType::Channel, 0)
                .expect("main: 取 sim 帧 RECV ObjRef")
                .0,
            ht_svcmgr
                .get_raw(h_simb_tx, handle::ObjType::Channel, 0)
                .expect("main: 取 sim 回执 SEND ObjRef")
                .0,
        );
        // SAFETY: 四端刚 insert + get_raw 过（在表、类型 Channel）——pair 完成
        // 互钉（endK2/endB2 同款：两表异地、配对在后插入侧；boot 单线程）。
        unsafe {
            crate::channel::ChannelEnd::pair(p_tx, p_b_rx); // 帧通道
            crate::channel::ChannelEnd::pair(p_b_tx, p_ack); // 回执通道
        }
        // 静态槽 + retain 钉住（RX Event 块同款有意泄漏双保险：永不 close +
        // 永不 release = IRQ 语境无 UAF 面）。
        // SAFETY: boot 单线程写、先于一切任务与 IRQ 读（DAIF 屏蔽窗无竞态）。
        unsafe {
            (*p_ev.as_ptr()).retain();
            *core::ptr::addr_of_mut!(UART1_EV_H) = h_ev;
            *core::ptr::addr_of_mut!(UART1_EV_REF) = Some(p_ev);
            *core::ptr::addr_of_mut!(SIM_TX_H) = h_tx;
            *core::ptr::addr_of_mut!(SIM_ACK_H) = h_ack;
        }
        // ③④ spawn → 通知槽 → 使能（次序硬约束：消费者就位先于线拉高）。
        // prio 0（console 同层常驻——系统转发服务，瞬态验收任务之下靠轮转）。
        berry_sched::spawn_with_tag(
            task_simcon,
            0,
            b"simcon",
            0,
            ht_sim as *mut HandleTable as usize,
        );
        berry_hal_aarch64::uart::set_uart1_notify(uart1_notify);
        // SAFETY: MMIO 写；boot 单线程、DAIF 屏蔽；fw_cfg 门开 = 第二 serial
        // 已配、设备在席（0x0904_0000 非洞）。
        unsafe { berry_hal_aarch64::uart::uart1_init() };
        // sim-S2 A4②：PL061 电源键接线（同门同窗——PL061 板级常在但裸跑/EFI
        // 零扰动，策略门与 UART1 同线）。次序硬约束同 UART1：通知槽先注册、
        // `pl061_init` 内 IMSC 等价面（GPIO_IE）最后开——首脉冲即真到达时
        // 记账/标记面已就位。SPI 39 不在路由表时 init 内 loud 让位（拓扑
        // 漂移在 enable 处炸响，不静默）。
        berry_hal_aarch64::pl061::set_pl061_notify(pl061_notify);
        // SAFETY: MMIO 写；boot 单线程、DAIF 屏蔽；PL061 板级常在（0x0903_0000
        // 恒非洞）——门只裁「是否使能」，不裁「是否存在」。
        unsafe { berry_hal_aarch64::pl061::pl061_init() };
        berry_hal_aarch64::uart_puts(b"SIM gate on\n");
    }
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
        // S-3b（子案 §9.6 填充点）：bring-up 成功真化拓扑（nr_cpus=2 +
        // capacity[1]）——par_enable 前（gate Release 序先于副核任何读）。
        berry_sched::topo_smp_online();
        berry_hal_aarch64::smp::par_enable();
        berry_hal_aarch64::uart_puts(b"PAR gate on\n");
    }
    // ---- S-3b b-2（子案 §9.6 义务③④⑤）：LB 金丝雀门控 spawn ----
    // 双门：smp_active（单核 -smp 1 boot 零面——LB 无副核无偷取语义）+
    // fw_cfg `opt/berry/lb` 具名项（smoke-smp 相 3 专递;RT/SOAK 同款「两侧
    // 对读」+ 缺席 = run/smoke 零扰动 + EFI 路天然缺席）。序约束（全体先例
    // 共同）：sched::init 后 + enable_irq 前（boot DAIF 屏蔽窗内 spawn）;
    // **set_lb_match 注册在 spawn 同窗**（sched 采样点 boot 期未启——首 tick
    // 前注入生效,归因零漏）。`LB gate on` 自证行（SOAK/PAR 先例：行在 ⟺
    // lb_master 已 spawn + 判别已注入——smoke 相 3 排障首查行）。
    if berry_hal_aarch64::smp::smp_active()
        && berry_hal_aarch64::fwcfg::find_file(LB_PROBE_NAME).is_some()
    {
        let ht_lb: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        // lb_master 无等待对象（收割走 wait_many 的 WAIT_ANY 哨兵——不挂
        // 哑 Event）,表纯为子 Process/VMO/Thread handle 而立（A45 私有表:
        // 归因键 get_raw 也读它）。arg = 表指针（rt_probe 形态）。
        berry_sched::set_lb_match(Some(lb_match));
        // prio 8（parprobe/soakmon 同档）：收割/打行醒即抢占;burst 造子期
        // 7×4 条 svc 全在自身时间片内连贯（无中途被常驻同级打断的时序面）。
        berry_sched::spawn_with_tag(task_lb_master, ht_lb as *mut HandleTable as usize,
                                    b"lb_master", 8, ht_lb as *mut HandleTable as usize);
        berry_hal_aarch64::uart_puts(b"LB gate on\n");
    }
    // ---- S-4 4-3：ASID 回卷 quiesce 门控 spawn（fw_cfg `opt/berry/asidroll`；
    // smoke-smp 相 4 专递——lb 同款双门）----
    // 双门同 LB：smp_active（单核 boot 零面——无他核 stale 面，arm 短路仅
    // 本地失效）+ fw_cfg 具名项（缺席 = run/smoke 零扰动 + EFI 天然缺席）。
    // 同窗约束同全体先例：sched::init 后 + enable_irq 前（boot DAIF 屏蔽窗
    // spawn）。任务 prio 8（parprobe/lb_master 同档），私有空表（A45 归因面
    // 同款——任务零 handle 操作，表纯为 spawn_with_tag 合同）。`ASIDR gate
    // on` 自证行（PAR/LB 先例：行在 ⟺ 任务已 spawn——相 4 排障首查行）。
    if berry_hal_aarch64::smp::smp_active()
        && berry_hal_aarch64::fwcfg::find_file(ASIDROLL_PROBE_NAME).is_some()
    {
        let ht_asidr: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        berry_sched::spawn_with_tag(task_asid_roll, 0, b"asidroll", 8,
                                    ht_asidr as *mut HandleTable as usize);
        berry_hal_aarch64::uart_puts(b"ASIDR gate on\n");
    }
    // ---- S-4 4-4：TLB shootdown canary 压测门控 spawn（fw_cfg
    // `opt/berry/tlbs`；smoke-smp 相 5 专递——lb/asidroll 同款双门）----
    // 双门同前例：smp_active（单核 boot 零面——无 stale TLB 可言）+
    // fw_cfg 具名项（缺席 = run/smoke 零扰动 + EFI 天然缺席）。任务 prio 8
    // （parprobe/lb_master/asidroll 同档）；表 = 私有表（svc 链 handle 落
    // 脚点，caller_table 经 current_tag 取——lb_master 同合同）+ 表内一个哑
    // Event（par 形态：arg = ObjRef 供 block_current 栅障/收割轮询超时醒，
    // 永不置位）；`TLBS gate on` 自证行（相 5 排障首查行）。
    if berry_hal_aarch64::smp::smp_active()
        && berry_hal_aarch64::fwcfg::find_file(TLBS_PROBE_NAME).is_some()
    {
        let ht_tlbs: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        let h_tlbs_ev = ht_tlbs
            .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
            .expect("main: tlbs 哑 Event 进 tlbs 表");
        let p_tlbs_ev = ht_tlbs
            .get_raw(h_tlbs_ev, ObjType::Event, 0)
            .expect("main: 取 tlbs ObjRef")
            .0;
        berry_sched::spawn_with_tag(task_tlbs, p_tlbs_ev.as_ptr() as usize, b"tlbs", 8,
                                    ht_tlbs as *mut HandleTable as usize);
        berry_hal_aarch64::uart_puts(b"TLBS gate on\n");
    }
    // ---- S-5b 相 6：ALLOC churn 压测门控 spawn（fw_cfg `opt/berry/alloc`；
    // smoke-smp 相 6 专递——前例同款双门）----
    // 双门同前例：smp_active（单核 boot 零面——无对核铺位）+ fw_cfg 具名项
    // （缺席 = run/smoke 零扰动 + EFI 天然缺席）。任务 prio 8（tlbs 同档）；
    // 表 = 私有表 + 一个哑 Event（arg = ObjRef 供 storm_poll 超时醒，永不
    // 置位）；`ALLOC gate on` 自证行（相 6 排障首查行）。
    if berry_hal_aarch64::smp::smp_active()
        && berry_hal_aarch64::fwcfg::find_file(ALLOC_PROBE_NAME).is_some()
    {
        let ht_alloc: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        let h_alloc_ev = ht_alloc
            .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
            .expect("main: alloc 哑 Event 进 alloc 表");
        let p_alloc_ev = ht_alloc
            .get_raw(h_alloc_ev, ObjType::Event, 0)
            .expect("main: 取 alloc ObjRef")
            .0;
        berry_sched::spawn_with_tag(task_alloc, p_alloc_ev.as_ptr() as usize, b"alloc", 8,
                                    ht_alloc as *mut HandleTable as usize);
        berry_hal_aarch64::uart_puts(b"ALLOC gate on\n");
    }
    // ---- S-5b 相 7：HT 风暴压测门控 spawn（fw_cfg `opt/berry/ht`；smoke-smp
    // 相 7 专递——alloc 同款双门）----
    // 结构同相 6（表 + 哑 Event + prio 8 + `HT gate on` 自证行）。
    if berry_hal_aarch64::smp::smp_active()
        && berry_hal_aarch64::fwcfg::find_file(HT_PROBE_NAME).is_some()
    {
        let ht_storm: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        let h_ht_ev = ht_storm
            .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
            .expect("main: ht 哑 Event 进 ht 表");
        let p_ht_ev = ht_storm
            .get_raw(h_ht_ev, ObjType::Event, 0)
            .expect("main: 取 ht ObjRef")
            .0;
        berry_sched::spawn_with_tag(task_ht, p_ht_ev.as_ptr() as usize, b"htstorm", 8,
                                    ht_storm as *mut HandleTable as usize);
        berry_hal_aarch64::uart_puts(b"HT gate on\n");
    }
    // ---- S-5c 相 8：PP ping-pong 压测门控 spawn（fw_cfg `opt/berry/pp`；
    // smoke-smp 相 8 专递——alloc/ht 同款双门）----
    // 结构同相 6/7（表 + 哑 Event + prio 8 + `PP gate on` 自证行）。
    if berry_hal_aarch64::smp::smp_active()
        && berry_hal_aarch64::fwcfg::find_file(PP_PROBE_NAME).is_some()
    {
        let ht_pp: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        let h_pp_ev = ht_pp
            .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
            .expect("main: pp 哑 Event 进 pp 表");
        let p_pp_ev = ht_pp
            .get_raw(h_pp_ev, ObjType::Event, 0)
            .expect("main: 取 pp ObjRef")
            .0;
        berry_sched::spawn_with_tag(task_pp, p_pp_ev.as_ptr() as usize, b"pp", 8,
                                    ht_pp as *mut HandleTable as usize);
        berry_hal_aarch64::uart_puts(b"PP gate on\n");
    }
    // ---- S-5d：soakload 满载载荷门控 spawn（fw_cfg `opt/berry/soakload`；
    // tools/soak-smp.sh 专递——alloc/ht/pp 同款双门）----
    // D2 裁决（SMP-S5 草案 §2）：双核 soak 满载载荷 = 内核门控 EL0 fleet
    //（root 家核 churn + 写者 spread→CPU1 互抢轮转）。master prio 0（全体
    // 常驻同档——boot 期一次性 spawn 链后即永睡，不与 soakmon prio-8 心跳
    // 争档）；自证行 `SOAKLOAD fleet on n=` 由 master 在舰队真 spawn 后打
    //（with_print_lock，enable_irq 后语境）——行在 ⟺ 载荷已在打，比 gate
    // 块先打更贴断言语义（ALLOC gate on 先例打在 master spawn 前，但相 6
    // 载荷是一次性有限轮，此处载荷即被测对象本体）。
    if berry_hal_aarch64::smp::smp_active()
        && berry_hal_aarch64::fwcfg::find_file(SOAKLOAD_PROBE_NAME).is_some()
    {
        let ht_sl: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        // 表内仅一个哑 Event（soak 块同形）：master 永睡的死线轮转等待对象
        //（无人 signal，唯一醒因 Timeout）。
        let h_sl_ev = ht_sl
            .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
            .expect("main: soakload 哑 Event 进表");
        let p_sl_ev = ht_sl
            .get_raw(h_sl_ev, ObjType::Event, 0)
            .expect("main: 取 soakload ObjRef")
            .0;
        berry_sched::spawn_with_tag(task_soakload, p_sl_ev.as_ptr() as usize, b"soakload", 0,
                                    ht_sl as *mut HandleTable as usize);
    }
    // ---- G3-4b：bench-lat SGI 路门控 spawn（fw_cfg `opt/berry/lat`——与
    // IPC 路同键双消费者，bench.py --lat 单 boot 收两路；pp 同款双门）----
    // 双门：smp_active（SGI 路 = EL0 sleeper spread→CPU1 的跨核对，单核无
    // 形态）+ fw_cfg 具名项（与 IPC 路共用 `opt/berry/lat`——缺席 =
    // run/smoke 零扰动 + EFI 天然缺席）。waker prio 0（soakload master
    // 同档：纯背景司机，被测方 sleeper 在 CPU1 不受 CPU0 档位干扰）。
    // ARMED 置位在 spawn 前同块（钩子三落点此起恒可入；Release 写 +
    // thread_start 锁链保 CPU1 可见序）。`LATSGI gate on` 自证行（storm
    // 门族同款：行在 ⟺ waker 已 spawn——缺席侧抽查短窗 grep 判别）。
    if berry_hal_aarch64::smp::smp_active()
        && berry_hal_aarch64::fwcfg::find_file(LAT_PROBE_NAME).is_some()
    {
        let ht_latsgi: &'static mut HandleTable =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(HandleTable::new()));
        // 表内仅一个哑 Event（storm master 族同形：表 = fleet spawn 的 svc
        // 落脚点，Event 纯占位无等待面）。
        let h_latsgi_ev = ht_latsgi
            .insert(KernelObj::Event(crate::event::Event::new()), handle::RIGHT_SIGNAL)
            .expect("main: latsgi 哑 Event 进表");
        let p_latsgi_ev = ht_latsgi
            .get_raw(h_latsgi_ev, ObjType::Event, 0)
            .expect("main: 取 latsgi ObjRef")
            .0;
        LATSGI_ARMED.store(true, core::sync::atomic::Ordering::Release);
        berry_sched::spawn_with_tag(task_latsgi_wak, p_latsgi_ev.as_ptr() as usize,
                                    b"latsgi", 0, ht_latsgi as *mut HandleTable as usize);
        berry_hal_aarch64::uart_puts(b"LATSGI gate on\n");
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
    // S-4 4-2：双核窗标记行 PRINT_LOCK 收编（bringup 后裸打与副核调度打印
    // 并发即字节撕行 = smoke 假阴性 240s 超时——实测本行与 sS2b steal 撞拍）。
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"K BLOCK ok\n");
    });
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
    // S-4 4-2：双核窗标记行 PRINT_LOCK 收编（同 K BLOCK——裸打撕行面）。
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"W WAIT ok\n");
    });
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
    // S-4 4-2：双核窗标记行 PRINT_LOCK 收编（同 K BLOCK——裸打撕行面）。
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"I PI ok\n");
    });
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

// ---- sim-S1：UART1 注入面静态槽（task_simcon 等待/消费侧——
// `docs/sim设备通道与契约设计-v1.1.md` §4）----
//
// console 输入面三件套全套镜像（RX_EV_BIT/RX_EV_H/RX_EV_REF 同形）：IRQ 侧
// 置位（`uart1_notify`）、simcon 消费包络内清位（`uart1_ev_clear`）。差异两
// 点：①唤醒记账与 UART0 **归并不拆**（`wake.rs src_of` 同 U 类——per-UART
// 粒度归 0x84 telemetry_open 终身面，设计文档 §3 裁决）；②多一对通道槽
// （`SIM_TX_H` 帧 SEND / `SIM_ACK_H` 回执 RECV——spawn arg=0，handle 全走
// 静态槽，RX_EV_H/BSH_IN_H 惯例）。

/// sim-S1 在席门键：fw_cfg 具名文件 `opt/berry/simdev`（LAT_PROBE_NAME 同款
/// `opt/berry/` 命名空间——justfile 配方同一行配第二 `-serial` 与此键，键在
/// = UART1 设备在席的拓扑真值）。值本身不消费（在席即门开）。
const SIM_DEV_KEY: &str = "opt/berry/simdev";

/// UART1 RX Event 的用户位（RX_EV_BIT=1<<39 顺延 1<<40；高半用户位纪律同
/// `USER_SIGNAL_BITS`）。
const UART1_EV_BIT: u64 = 1 << 40;

/// UART1 RX Event handle 静态槽（task_simcon 读；写点 = boot sim 接线段，
/// 先于一切任务）。
static mut UART1_EV_H: u32 = 0;
/// UART1 RX Event 的 ObjRef（`uart1_notify` 置位 / `uart1_ev_clear` 清位共用；
/// boot 写入后不变 + retain 钉住永不 release——RX_EV_REF 同款双保险）。
static mut UART1_EV_REF: Option<core::ptr::NonNull<handle::ObjRef>> = None;

/// 读 UART1 RX Event handle 静态槽（task_simcon 侧）。
fn uart1_ev_h() -> u32 {
    // SAFETY: 只读；全部写点先于任务启动（boot 顺序）。
    unsafe { *core::ptr::addr_of!(UART1_EV_H) }
}

/// sim 帧通道 simcon 发端 handle 静态槽（UART1 排水 → simbattery）。
static mut SIM_TX_H: u32 = 0;
/// sim 回执通道 simcon 收端 handle 静态槽（simbattery ACK → UART1 TX 回写）。
static mut SIM_ACK_H: u32 = 0;

/// 读 SIM_TX_H 静态槽（task_simcon 侧）。
fn sim_tx_h() -> u32 {
    // SAFETY: 只读；写点先于任务启动。
    unsafe { *core::ptr::addr_of!(SIM_TX_H) }
}

/// 读 SIM_ACK_H 静态槽（task_simcon 侧）。
fn sim_ack_h() -> u32 {
    // SAFETY: 只读；写点先于任务启动。
    unsafe { *core::ptr::addr_of!(SIM_ACK_H) }
}

/// 「UART1 RX 到数据」通知（HAL `exception_irq` PL1_RX 臂 → `uart::uart1_rx`
/// 经槽调；`rx_irq_notify` 逐句同形——IRQ 上下文 PSTATE.I=1、pre-EOI）。到达
/// 记账归 `WAKE_SRC_UART_RX`（与 UART0 归并口径）→ SQ 窗内置位 + `wake_
/// matching_locked`（义务③写位+扫描同窗）→ n=0 记 no_waiter → 回报 n>0
/// （检查点决策在 trap.rs PL1_RX 臂 EOIR 后——本槽不切，EOIR 饿死防线）。
unsafe extern "C" fn uart1_notify() -> bool {
    // SAFETY: IRQ 上下文（单核屏蔽窗串行）；UART1_EV_REF boot 后不变。
    let p = unsafe { *core::ptr::addr_of!(UART1_EV_REF) }
        .expect("uart1_notify: UART1_EV_REF 未初始化（boot 接线缺失）");
    // sim-S1：与 UART0 同源归并记账（per-UART 拆分归 v2 0x84 遥测面）。
    crate::telemetry::note_irq_arrival(
        berry_hal_aarch64::wake_gen::WAKE_SRC_UART_RX,
        berry_hal_aarch64::timer::cntvct(),
    );
    unsafe {
        // 写位+扫描同窗（c-2b slice 4 自锁面；KL 外层包络剥除后的纯 SQ 单锁窗）。
        let sq_phys = berry_sched::sq_lock(); // 位感知退化形（c-2c slice 2a）
        if let Some(ev) = (*p.as_ptr()).obj.as_event_mut() {
            ev.signal(0, UART1_EV_BIT);
        }
        let n = berry_sched::wake_matching_locked(p.as_ptr() as usize, UART1_EV_BIT);
        berry_sched::sq_unlock(sq_phys);
        if n == 0 {
            crate::telemetry::note_irq_no_waiter(berry_hal_aarch64::wake_gen::WAKE_SRC_UART_RX);
        }
        // 只回报不检查点（F1 定谳——检查点在 trap 臂 EOIR 后）。
        n > 0
    }
}

/// 「PL061 电源键到达」通知（HAL `exception_irq` SPI39 臂 → `pl061::power_
/// key_irq` 经槽调；调用语境与 [`uart1_notify`] 同：IRQ 上下文 PSTATE.I=1、
/// pre-EOI——ICR 去活在槽调用方已完成）。v1 无主 IRQ 正向对照（A4②）：
/// 电源管理服务 v2 才落，**恒无等待者/恒无唤醒**——无 Event 置位、无
/// wake_matching，只做双台账：到达 `note_irq_arrival`（POWER_KEY，IRQ 级）
/// + no_waiter `note_irq_no_waiter`（结构性无主——每次到达都记，与
/// uart1_notify 的「n==0 才记」语义不同）；回报恒 false（trap 臂无
/// woke-检查点面）。标记行 `PWRKEY unowned n=` 的数值**读自 telemetry
/// no_waiter 台账**（记账为 unowned 的验收本体——telemetry 被拆/漏记时
/// n=0 直红，本地计数器会让变异假绿，审查裁决改源）。打印门 = trap.rs
/// 未知臂同款 preempt_count 门（count>0 = 被打断者可能在 PRINT_LOCK 窗
/// 内，裸取 = 同核自旋等不可运行者自死锁——只记账让位；QMP 重注入可补拍
/// 标记行，台账不丢）。
unsafe extern "C" fn pl061_notify() -> bool {
    crate::telemetry::note_irq_arrival(
        berry_hal_aarch64::wake_gen::WAKE_SRC_POWER_KEY,
        berry_hal_aarch64::timer::cntvct(),
    );
    crate::telemetry::note_irq_no_waiter(berry_hal_aarch64::wake_gen::WAKE_SRC_POWER_KEY);
    let in_preempt_window = berry_hal_aarch64::smp::preempt_count_slot()
        .map_or(false, |c| c.load(core::sync::atomic::Ordering::Relaxed) > 0);
    if !in_preempt_window {
        let n = crate::telemetry::wake_src_snapshot(
            berry_hal_aarch64::wake_gen::WAKE_SRC_POWER_KEY,
        )
        .no_waiter;
        berry_hal_aarch64::kern_lock::print_lock();
        berry_hal_aarch64::uart_puts(b"PWRKEY unowned n=");
        berry_hal_aarch64::uart_put_dec(n);
        berry_hal_aarch64::uart_puts(b"\n");
        berry_hal_aarch64::kern_lock::print_unlock();
    }
    false
}

/// 清 UART1 RX Event 位（task_simcon 消费包络内调；`rx_ev_clear` 同形——
/// IRQ 在包络内被屏蔽，位只可能在包络外被置，置后下轮 poll-first 必命中）。
///
/// # Safety
/// 须在 `disable_irq` 包络内调（与 UART1 RX IRQ 生产者串行）；引用不变。
unsafe fn uart1_ev_clear() {
    // SAFETY: 调用方保证屏蔽包络 + 引用不变。
    let p = unsafe { *core::ptr::addr_of!(UART1_EV_REF) }
        .expect("uart1_ev_clear: UART1_EV_REF 未初始化");
    unsafe {
        if let Some(ev) = (*p.as_ptr()).obj.as_event_mut() {
            ev.signal(UART1_EV_BIT, 0);
        }
    }
}

/// 刀1：CNTV tick 槽计数包装（HAL `exception_irq` CNTV 臂经 `irq_dispatch`
/// 槽调，post-EOIR）。先记 TIMER 到达 + 派发戳（`last_ack_ts` = 槽派发点
/// ——post-EOI 语义；per-source 记账面见 `telemetry::WAKE_SRC_TABLE`）再转
/// [`berry_sched::on_irq_tick`]。IRQ 语境纪律：零分配、纯原子，槽原语义
/// 零漂移。
unsafe extern "C" fn irq_tick_acct(tf: &mut berry_hal_aarch64::trap::TrapFrame) {
    crate::telemetry::note_irq_arrival(
        berry_hal_aarch64::wake_gen::WAKE_SRC_TIMER,
        berry_hal_aarch64::timer::cntvct(),
    );
    // SAFETY: IRQ 上下文（PSTATE.I=1，post-EOIR）；转发原槽契约不变。
    unsafe { berry_sched::on_irq_tick(tf) }
}

/// 「RX 到数据」通知（HAL `exception_irq` → `uart::rx_irq` 经槽调；IRQ 上下文
/// PSTATE.I=1）。置位 RX Event（用户半位）+ `wake_matching`（匹配键 = ObjRef
/// 地址，同 4-1 `sys_object_signal` 唤醒路径；console 的 wait_many 挂
/// WAIT_ANY 哨兵——任何非零 set 命中，醒来重验定夺）。**G3-2（P-B
/// preempt-on-wake IRQ 面；修复批 F1 改版）**：v1「不立即抢占（醒者等
/// tick/yield，交互延迟 ≤ 1 slice）」退役——本槽做**唤醒 + 回报**：置位 +
/// SQ 窗内 wake_matching（同窗契约不变），**返回「是否唤醒」**（n>0）；
/// 直切检查点**不在本槽**（挪 trap.rs PL011_RX 臂 `gic::end` 之后——
/// ack↔EOIR 之间切换 = context_switch 不归 ⇒ EOIR 无限推迟 ⇒ SPI33 保持
/// active 钉 RPR ⇒ 同级 CNTV 发不出（EOIR 饿死，trap.rs Bug 1 立法）；
/// tick/SGI 臂 dispatch 全 post-EOIR 才是真同形先例，f4 F1 定谳）。检查点
/// 语义不变：严格更高 eff 才切、IRQ 上下文 PSTATE.I=1 天然满足切换原语
/// 屏蔽窗契约、count>0 禁切窗自豁免、探峰取 RQ 在 SQ 窗外独立窗正序。
/// n=0（键入无等待者）回报 false 不探。写
/// Event 与遥测环同纪律：零分配、纯指针写字段。S-3c c-2b slice 4 起位写+
/// 扫描同 **SQ 窗**（`wake_matching_locked`——义务③「写位+扫描同窗」的
/// IRQ 生产者面，F-L2 不可重入立法）；3-1b 起 KL 外层包络剥除（E27 退役）
/// ——本面 = 纯 SQ 独立单锁窗。
unsafe extern "C" fn rx_irq_notify() -> bool {
    // SAFETY: IRQ 上下文（单核屏蔽窗串行）；RX_EV_REF boot 后不变。
    let p = unsafe { *core::ptr::addr_of!(RX_EV_REF) }
        .expect("rx_irq_notify: RX_EV_REF 未初始化（boot 接线缺失）");
    // 刀1：UART_RX 到达记账（pre-EOI 到达戳——本槽在 uart::rx_irq 内、
    // gic::end 之前跑；post-EOI ack 接力精化归 v2，设计文档演进节）。
    crate::telemetry::note_irq_arrival(
        berry_hal_aarch64::wake_gen::WAKE_SRC_UART_RX,
        berry_hal_aarch64::timer::cntvct(),
    );
    unsafe {
        // c-2b slice 4（自锁面第三处，F-L2 + 义务③ IRQ 面）：**写位+扫描
        // 同窗**——Event 位写与 SLEEPQ 扫描摘队在同一 SQ 窗内原子（分段后
        // 「位已置而队上无等待者」窗口 = 丢唤醒面；消费者 poll-first +
        // rx_ev_clear 豁免位协议不变）。持窗版 wake_matching_locked 不再
        // 自取 SQ（不可重入）；3-1b 起 KL 不再外层包络（终态 KL 最内，
        // SQ 独立窗即终态形）。
        let sq_phys = berry_sched::sq_lock(); // 位感知退化形（c-2c slice 2a）
        if let Some(ev) = (*p.as_ptr()).obj.as_event_mut() {
            ev.signal(0, RX_EV_BIT);
        }
        let n = berry_sched::wake_matching_locked(p.as_ptr() as usize, RX_EV_BIT);
        berry_sched::sq_unlock(sq_phys);
        // 刀1：n=0 = 到达无等待者（console 醒-再睡窗内到达的良性竞态——位
        // 已置、下轮 poll-first 必命中，无丢唤醒；计数 + sticky，不设门禁）。
        if n == 0 {
            crate::telemetry::note_irq_no_waiter(berry_hal_aarch64::wake_gen::WAKE_SRC_UART_RX);
        }
        // G3-2 修复批（F1）：本槽只回报，不检查点——SQ 窗内禁切换（义务③
        // 「写位+扫描同窗」契约不破）且检查点须在 trap 臂 EOIR 之后（EOIR
        // 饿死防线，详见函数头注）。n=0 = 无唤醒诉求，false 不探。
        n > 0
    }
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

// ---- AM-2b（2026-11-08 提案候选①·主会话六裁决）：系统压力 Event 出口。
// 阈值判据/惰性翻卷在 telemetry 窗口聚合块（提案 §二）；本块 = 位定义 +
// ObjRef 静态槽 + 跨阈置位回调。写者语境 = dpg 探针 note 点的**内核任务
// 语境**（KL 窗内 SQ 嵌套 = dpg.rs `wake_matching` 同构）——免 event.rs
// 非 SVC 写者修约（裁决④）。complete 位（1<<41）登记预留不置位（裁决②，
// 登记面 = tools/abi.toml [[signal]] 注记区）。----

/// 压力 Event 的 partial 跨阈位（1<<40，裁决③；complete 预留 1<<41 不落
/// 常量——本刀只登记不置位）。内核占用用户位登记面 = abi.toml [[signal]]
/// 注记区（防撞单源）。位空间按对象隔离：与 UART1_EV_BIT（1<<40·另一
/// Event 对象）/ `USER_SIGNAL_BITS` 在册哑位（1<<40 起各自对象）无清位/
/// 唤醒纠缠——登记语义 = 内核写者在席公示，非全局排他。
const AM2B_PARTIAL_BIT: u64 = 1 << 40;

/// partial 窗口长初值 70ms（裁决②【推，实测定标】——boot 期按 CNTFRQ
/// 换算周期数，TCG 频偏禁编译期常量；complete 700ms 同式随 waiter 深度
/// 跟踪后置）。
const AM2B_WIN_MS: u64 = 70;

/// 压力 Event 的 ObjRef（`pressure_partial_cross` 置位用；boot 写入后
/// 不变 + retain 钉住永不 release——RX_EV_REF 同款双保险）。
static mut PRESSURE_EV_REF: Option<core::ptr::NonNull<handle::ObjRef>> = None;

/// AM-2b：partial 跨阈出口（telemetry `dpg_stall_fold` 的跨阈回调）。
/// `rx_irq_notify` 形先例（:2653-2658）：写位 + 扫描同 SQ 窗（义务③「写
/// 位+扫描同窗」）；非 IRQ 到达**不记** WAKE_SRC 台账；n=0（am 未成形无
/// 等待者）静默——位已置，消费者 poll-first 必命中。complete 位不置。
fn pressure_partial_cross() {
    // SAFETY: 只读；boot 写点先于一切 dpg 计值点（boot 顺序）。
    let p = unsafe { *core::ptr::addr_of!(PRESSURE_EV_REF) }
        .expect("am2b: PRESSURE_EV_REF 未初始化（boot 接线缺失）");
    let sq_phys = berry_sched::sq_lock();
    // SAFETY: retain 钉住 + boot 后不变（RX_EV_REF 口径）。
    unsafe {
        if let Some(ev) = (*p.as_ptr()).obj.as_event_mut() {
            ev.signal(0, AM2B_PARTIAL_BIT);
        }
        let _n = berry_sched::wake_matching_locked(p.as_ptr() as usize, AM2B_PARTIAL_BIT);
    }
    berry_sched::sq_unlock(sq_phys);
}

/// 读压力 Event 当前位掩码（ktest 直读面——`object_wait_one` 的 poll-first
/// 同源读；ktest 在 sched::init 前跑不能睡，走直读〔任务书步骤⑤〕）。
pub(crate) fn am2b_pressure_bits() -> u64 {
    // SAFETY: 只读；boot 写点先于 ktest::run。
    let p = unsafe { *core::ptr::addr_of!(PRESSURE_EV_REF) }
        .expect("am2b_pressure_bits: PRESSURE_EV_REF 未初始化");
    // SAFETY: 同上（retain 钉住 + 类型恒 Event）。
    unsafe { (*p.as_ptr()).obj.as_event_mut().map(|e| e.load()).unwrap_or(0) }
}

/// ktest 期压力 Event 的 SVC 面 handle（bootstrap 表插取——E3h「VMO 经
/// bootstrap 表插取」同款惯例。**boot 期不预插**：ktest 前序测试（WAKEP
/// 清场）会 drain bootstrap 表作废槽位（实踩）；retain 一枚随 insert_ref
/// 移交（handle.rs「接管既有令牌」契约），测毕 close 归还）。
pub(crate) fn am2b_pressure_ktest_handle() -> u32 {
    // SAFETY: 只读；boot 写点先于 ktest::run。
    let p = unsafe { *core::ptr::addr_of!(PRESSURE_EV_REF) }
        .expect("am2b: PRESSURE_EV_REF 未初始化（boot 接线缺失）");
    // SAFETY: retain 后随 insert_ref 移交（在表、类型 Event、retain 钉住）。
    unsafe {
        (*p.as_ptr()).retain();
        crate::syscall::bootstrap_table()
            .insert_ref(p, handle::RIGHT_SIGNAL)
            .expect("am2b: 压力 Event ktest 插取进 bootstrap 表")
    }
}

/// 读 partial 位常量（ktest 断言位值 = 裁决③的 1<<40）。
pub(crate) fn am2b_partial_bit() -> u64 {
    AM2B_PARTIAL_BIT
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
                    // 收到完整行 token（状态谓词，非注入即达的回声）。整段
                    // PRINT_LOCK 包络（3-1a 判例刀同款——多段 uart_puts 拼行
                    // 被矩阵单字符织入 = smoke「RX ok」前缀假阴性同族面）。
                    berry_hal_aarch64::kern_lock::with_print_lock(|| {
                        berry_hal_aarch64::uart_puts(b"RX ok n=");
                        berry_hal_aarch64::uart_put_dec(rx_total);
                        berry_hal_aarch64::uart_puts(b" lost=");
                        berry_hal_aarch64::uart_put_dec(
                            berry_hal_aarch64::uart::rx_lost() as u64,
                        );
                        // A-2：转发丢弃计数（bsh 断供面诊断位；正常恒 0——
                        // smoke 只 grep "RX ok" 前缀，行尾追加零扰动）。
                        berry_hal_aarch64::uart_puts(b" fdrop=");
                        berry_hal_aarch64::uart_put_dec(fwd_drop);
                        // GUI-1 段②：fb fan-out 丢弃计数（同款行尾诊断位）。
                        berry_hal_aarch64::uart_puts(b" fbdrop=");
                        berry_hal_aarch64::uart_put_dec(fb_drop);
                        berry_hal_aarch64::uart_puts(b"\n");
                    });
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
            // 3-1a 判例刀（smoke-smp 相 2 实抓「LSN oBk n=7」）：用户打印
            // drain 改整条消息 PRINT_LOCK 包络——原逐字节裸 uart_putc 与
            // ktest 矩阵任务单字符直写者**字节级并发**（无共同锁面），标记
            // 行中段可被织入单字符 → smoke grep 假 FAIL（840s 假阴性）。包
            // 络后矩阵字符只能落消息前后（整条原子），标记恒完整。
            // with_print_lock 包络 = preempt 计数对（G43 改版 G3-1c-1——
            // 任务态持锁期 IRQ 解蔽，tick/SGI 抢占支按计数闭门「入而不切」；
            // 详 kern_lock 模块头/§A114 追记）；n ≤ 256B @ QEMU PL011 TX
            // 恒 ready = 微秒级锁窗。
            berry_hal_aarch64::kern_lock::with_print_lock(|| {
                for i in 0..n as usize {
                    berry_hal_aarch64::uart_putc(buf[i]);
                    CONSOLE_DRAINED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                }
            });
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

/// sim-S1 EL1 转发任务（`docs/sim设备通道与契约设计-v1.1.md` §3 裁决案 C：
/// 内核 drain + EL1 转发 + 纯协议 bin）。链 = UART1 RX（HAL 环）→ 帧通道 →
/// simbattery（解码/验证/打 `SIMBAT ok seq=N`）→ 回执通道 → `uart1_tx` 回写
/// 宿主 client。**task_console 全套镜像**：wait_many [UART1 Event:1<<40,
/// 回执:READABLE, (条件)帧SEND:WRITABLE] 10s 心跳帽 → `disable_irq` 包络内
/// `uart1_rx_pop_all` + 清位（「位 0 ⟺ 环空」不变量包络内闭环；包络内无
/// SVC/阻塞）→ 帧块发（`CHAN_MSG_MAX`=256 ≥ 帧帽 98B——一次 drained burst
/// 恰一条 send；-EBUSY = simbattery 欠账，记 pending 偏移等 WRITABLE——
/// console C-4r 吸收环的块级简化版）→ 回执 drain（-EAGAIN 即空）→ 逐字节
/// `uart1_tx`（自带 TXFF 有界自旋）。opt-out 通道 + 私有表 + WAIT_ANY 哨兵
/// （console 同款三保险：固定优先级常驻、错捐断根、退出 donor 面）。常驻
/// 不退出；spawn arg=0（三 handle 全走静态槽）。
extern "C" fn task_simcon(_arg: usize) -> ! {
    use crate::handle::{ERR_AGAIN, ERR_BUSY, ERR_TIMEDOUT, SIGNAL_READABLE, SIGNAL_WRITABLE};
    use crate::syscall::{NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_CLOCK_READ, NR_OBJECT_WAIT_MANY};
    let h_ev = uart1_ev_h();
    let h_tx = sim_tx_h();
    let h_ack = sim_ack_h();
    // wait_many 三数组（栈上 EL1 内核高半——copy_from_user 的 EL1 分支收）。
    // 第 3 项（帧 SEND WRITABLE）只在 pending 非空时入列——console endK2 条件
    // 项同纪律（恒满足项钉死 drain 相位 = A-2 自旋饿死面）。
    let hs = [h_ev, h_ack, h_tx];
    let sigs = [UART1_EV_BIT, SIGNAL_READABLE, SIGNAL_WRITABLE];
    let mut now: u64 = 0;
    // SAFETY: svc 同步异常；栈变量私有（内核高半，copy 直拷）。
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    let (mut idx, mut observed): (u64, u64) = (0, 0);
    let (pidx, pobs) = (
        core::ptr::addr_of_mut!(idx) as u64,
        core::ptr::addr_of_mut!(observed) as u64,
    );
    // 排水缓冲 = 环容量（包络内必搬空）；pending 缓冲 = 环帽 + 帧帽余量
    //（EBUSY 残余 ≤ 帧帽 + 新排水 ≤ 环帽 → 352 几何不溢；溢 = simbattery
    // 死面，丢新计数、SIMBAT 缺席由 smoke 兜底——不 panic 不 wedge）。
    let mut rxbuf = [0u8; berry_hal_aarch64::uart::RX_RING_SIZE];
    let mut pend = [0u8; 352];
    let (mut pend_off, mut pend_len) = (0usize, 0usize);
    let mut sim_drop: u64 = 0;
    let mut ackbuf = [0u8; 256];
    let (mut n, mut nh): (u64, u64);
    loop {
        assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "simcon: clock_read");
        let n_items: u64 = if pend_off < pend_len { 3 } else { 2 };
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
            "simcon: wait_many 异常 ret={}",
            r
        );
        if r == 0 {
            // 命中面全集：ev(0) 注入数据 / ack(1) 回执到 / tx(2) 队列腾位
            //（仅 pending 非空轮入列）。
            assert!(
                idx == 0 || idx == 1 || idx == 2,
                "simcon: wait 命中须 ev(0)/ack(1)/tx(2)，得 {}",
                idx
            );
        }
        // UART1 消费——**每次唤醒无条件执行**（console 同款：wait 返回与 IRQ
        // 置位任意交错——早到/迟到/心跳顺带——都收敛到「环搬空 + 位清零」）。
        // 包络内无 SVC/阻塞（屏蔽窗内切走 = 违约）。
        berry_hal_aarch64::disable_irq();
        let nrx = berry_hal_aarch64::uart::uart1_rx_pop_all(&mut rxbuf);
        // SAFETY: 包络内与 UART1 RX IRQ 生产者串行；引用不变。
        unsafe { uart1_ev_clear() };
        berry_hal_aarch64::enable_irq();
        // 新排水并 pending（残余 + 新到；满 = 消费者死面丢新计数）。
        for i in 0..nrx {
            if pend_len < pend.len() {
                pend[pend_len] = rxbuf[i];
                pend_len += 1;
            } else {
                sim_drop += 1;
            }
        }
        // 帧块发：chunk ≤ CHAN_MSG_MAX(256)——StreamDecoder 字节流语义对任意
        // 分片边界鲁棒（§5 静默重同步）；-EBUSY 停排留余量（下轮第 3 项
        // WRITABLE 唤醒续发）。其它错码 = 接线错乱（boot 定值 + SEND right
        // 在手），panic 兜底。
        while pend_off < pend_len {
            let chunk = core::cmp::min(pend_len - pend_off, 256);
            let rs = svc6(
                NR_CHANNEL_SEND as u64,
                h_tx as u64,
                core::ptr::addr_of!(pend[pend_off]) as u64,
                chunk as u64,
                0,
                0,
                0,
            );
            if rs == ERR_BUSY as i64 {
                break;
            }
            assert_eq!(rs, 0, "simcon: sim 帧 send ret={}", rs);
            pend_off += chunk;
        }
        if pend_off >= pend_len {
            pend_off = 0;
            pend_len = 0;
        }
        // 回执 drain：排空本端队列（simbattery 逐条回 9B ACK；深度 8 → 批量
        // 有界）→ 逐字节 uart1_tx。UART1 单写者 = simcon，无需 PRINT_LOCK
        //（UART0 多写者争面不存在；ACK 帧定界由宿主 client 解码器负责）。
        loop {
            n = 256;
            nh = 0;
            let r2 = svc6(
                NR_CHANNEL_RECV as u64,
                h_ack as u64,
                ackbuf.as_mut_ptr() as u64,
                core::ptr::addr_of_mut!(n) as u64,
                0,
                core::ptr::addr_of_mut!(nh) as u64,
                0,
            );
            if r2 == ERR_AGAIN as i64 {
                break; // 排空——回外层阻塞等
            }
            assert_eq!(r2, 0, "simcon: ack recv 异常 ret={}", r2);
            for i in 0..n as usize {
                berry_hal_aarch64::uart::uart1_tx(ackbuf[i]);
            }
        }
        // 诊断面：心跳顺带打丢弃计数（健康路径恒 0 且无行——只在丢过才打，
        // 零扰动 smoke 面；TXFF 触界同 UART1_TX_LOST 静默计数面）。
        if sim_drop > 0 {
            berry_hal_aarch64::kern_lock::with_print_lock(|| {
                berry_hal_aarch64::uart_puts(b"SIM drop=");
                berry_hal_aarch64::uart_put_dec(sim_drop);
                berry_hal_aarch64::uart_puts(b"\n");
            });
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
    // S-4 4-2：双核窗标记行 PRINT_LOCK 收编（同 K BLOCK——裸打撕行面）。
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"I INTENT ok\n");
    });
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
    // g3-1d 检查点观测面：ckpt= 为**全局累计**命中数（SVC 派发尾落点主导，
    // 段出口只占少数）——真判别力 = 全局绊网：PreemptSpan 泄漏钉死 count>0
    // ⇒ 全站空探（计数>0 直返）⇒ 此处归零即断（f4 g3-1d p1 语义正名：对
    // 「段出口是否真探」零判别——逐段判别需每轮基线相对增量 ≥2×128）。
    // 行尾字段不新增金丝雀行族（f4 g3-1d 预警②零同步负担）。
    let ckpt = berry_sched::preempt_checkpoint_hits();
    assert!(ckpt >= 1, "fa_ping: 检查点全局计数归零——疑 PreemptSpan 泄漏使全站空探（实跑 {}）", ckpt);
    let avg_ticks = rt_ticks / FAST_ROUNDS;
    let ref_cyc = avg_ticks * 1_000_000_000 / freq; // 1GHz 参考 cycle（数值 = ns）
    // S-4 4-2：双核窗标记行 PRINT_LOCK 收编（多段拼行整段入锁——RX ok 同款）。
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
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
        berry_hal_aarch64::uart_puts(b" ckpt=");
        berry_hal_aarch64::uart_put_dec(ckpt);
        berry_hal_aarch64::uart_puts(b"\n");
    });
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
    // S-4 4-2：双核窗标记行 PRINT_LOCK 收编（同 K BLOCK——裸打撕行面）。
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"TAGTAB ok\n");
    });
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
    (1 << 43, "tlbs 哑 Event（S-4 4-4 canary 压测 master——永不置位，仅超时醒）"),
    (1 << 44, "alloc 哑 Event（S-5b 相 6 churn 压测 master——永不置位，仅超时醒）"),
    (1 << 45, "ht 哑 Event（S-5b 相 7 风暴压测 master——永不置位，仅超时醒）"),
    (1 << 46, "pp 哑 Event（S-5c 相 8 ping-pong 压测 master——永不置位，仅超时醒）"),
    (1 << 47, "soakload 哑 Event（S-5d 双核 soak 满载载荷 master——永不置位，仅超时醒）"),
    (1 << 48, "lat 握手 Event（G3-4 bench-lat IPC 路——waker wake_matching 唤醒，sleeper 测 wake→run 段）"),
    (1 << 49, "latsgi 位签名（G3-4b bench-lat SGI 路——EL0 sleeper wait_one 位签名门 + 内核 waker Event 置位跨核唤醒）"),
    (1 << 50, "lat 触发 Event 内部位（G3-5 候选① C 形常驻再触发——soakmon 拍后 signal+wake_matching；Event 落内核私有表不经 ABI object_signal 面，登记 = 防撞/去重面〔f4 F8 补〕）"),
    (1 << 51, "dpg 握手位（G4-1 demand 闭环——裸键族共用一位：内核 fault/pager/done 三键 + EL0 键化族 EFAULT×32〔X-5 扩池 8→32〕/EOVERFLOW/EPAGER〔G4-2 起、B-1 池化〕共 37 键；不经 Event 对象，登记 = 防撞/去重面）"),
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
/// tlbs 哑 Event 的用户位（S-4 4-4，占用登记见 [`USER_SIGNAL_BITS`]——
/// PAR_EV_BIT 的续位）。永不置位：唯一醒因 Timeout（canary 压测 master 的
/// 栅障/收割轮询节拍——计数面是 CANARY_SLOTS 原子，非事件面）。
const TLBS_EV_BIT: u64 = 1 << 43;

/// ai_demo ↔ ai_probe 握手标志：demo bind 注入表后、睡 E_rel 前置位——probe
/// 只在该窗口内做 clamp 断言（E_done 等待期 demo 同样在队但 eff=0，flag 区分
/// 两窗）。单核 v1 Relaxed 足够（读写都在单核串行语境；S-2 起口径不变——
/// 内核任务全钉 CPU0，读写同核串行保持）。
static AI_BOUND: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// 最近一轮 bench 的均轮 RT（CNTVCT ticks）：pinger 收尾写、demo 醒来读——
/// 单核串行天然有序（先写后 signal、醒来必读到本轮值；S-2 起同核钉 CPU0
/// 口径保持）。
static AI_RT_LAST: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// 最近一轮 bench 的**最小轮 RT**（噪声底统计量，同写读序）：均值被宿主
/// 瞬态争用挤穿（2026-11-04 慢夜 rt2/rt1≈2.04x 撞 2x 断言——TCG 下 guest
/// 钟=墙钟，宿主挤谁谁的读数涨，与内核回归无因果；§A110 时敏 flake 族同
/// 源：昼夜算力波动，复跑即绿非代码回归）；64
/// 轮取 min = 宿主尖峰只污少数轮，min 几乎必然落在无争用轮 = 真开销估计。
/// 断言面迁 min（真回归 = 全轮一致变慢必抬 min，判别力不降）。
static AI_RT_MIN: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

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
    // 1.1/S4-b：billing 遥测面——跨域消息计数 + 各域 CPU 时长（非零槽）。
    berry_hal_aarch64::uart_puts(b"tele: billing cross=");
    berry_hal_aarch64::uart_put_dec(crate::telemetry::billing_cross_domain_total());
    for i in 0..crate::telemetry::BILLING_SLOTS {
        let (d, ns) = crate::telemetry::billing_slot_at(i);
        if d != 0 {
            berry_hal_aarch64::uart_puts(b" d");
            berry_hal_aarch64::uart_put_dec(d);
            berry_hal_aarch64::uart_puts(b"=");
            berry_hal_aarch64::uart_put_dec(ns);
            berry_hal_aarch64::uart_puts(b"ns");
        }
    }
    // 1.1/S5.11 freeze 遥测面。
    berry_hal_aarch64::uart_puts(b" freeze=");
    berry_hal_aarch64::uart_put_dec(crate::telemetry::freeze_ok_total());
    berry_hal_aarch64::uart_puts(b"/busy=");
    berry_hal_aarch64::uart_put_dec(crate::telemetry::freeze_busy_total());
    berry_hal_aarch64::uart_puts(b"/thaw=");
    berry_hal_aarch64::uart_put_dec(crate::telemetry::auto_thaw_total());
    berry_hal_aarch64::uart_puts(b"\n");
    // W3-Q6：intent 档位直方 + 通胀计数行（`berry_itally::tally_line` 单源
    // 装配——纯 ASCII 定长缓冲一次写出；advisor.py 摘要行族面，全零表打
    // `none` 省略形）。三帽计数 = 4-3c 既有计数器（borrows/hop/budget），
    // 与档位直方同窗同源——advisor 无需第二数据面。
    {
        let mut line = [0u8; berry_itally::LINE_MAX];
        let n = berry_itally::tally_line(
            crate::telemetry::intent_tally(),
            crate::telemetry::intent_borrows(),
            crate::telemetry::intent_hop_capped_total(),
            crate::telemetry::intent_budget_capped_total(),
            &mut line,
        );
        berry_hal_aarch64::uart_puts(&line[..n]);
    }
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
/// → ⑦unbind → ⑧开销断言（F9① 最小兑现：rt2_min ≤ 2×rt1_min——min =
/// 噪声底统计量，均值被宿主争用挤穿先例见 AI_RT_MIN 注释）→
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
    let rt1_min = AI_RT_MIN.load(core::sync::atomic::Ordering::Relaxed);
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
    let rt2_min = AI_RT_MIN.load(core::sync::atomic::Ordering::Relaxed);
    // ⑦ unbind（恢复接口缺席态；klock 包络同 ④，F41）。
    berry_hal_aarch64::kern_lock::klock_enter();
    berry_sched::policy::unbind();
    berry_hal_aarch64::kern_lock::klock_exit();
    // ⑧ 开销断言（F9① 最小兑现：注入表在场时 bench RT 须同量级——热路径增
    // 量 = 2 条目线性查表 + 权重算术，真开销远低于容差）。**断言面 = min
    // （噪声底统计量）**：均值 rt/rt2 被 2026-11-04 慢夜宿主争用挤穿 2x
    //（rt1=7147 rt2=14589——TCG guest 钟=墙钟，§A110 时敏 flake 族）；64 轮
    // 取 min = 宿主尖峰只污少数轮，min 落在无争用轮 = 真开销估计；真回归
    // = 全轮一致变慢必抬 min，判别力不降。均值仍打印留观测（连续性）；
    // -icount 精确版/真机 p99 随 FAST 线留 G 卡复测。
    assert!(rt1_min > 0, "ai_demo: rt1_min 须非零");
    assert!(
        rt2_min <= rt1_min * 2,
        "ai_demo: 注入后 bench RT 噪声底须同量级（min1={} min2={}，rt1={} rt2={} 观测）",
        rt1_min, rt2_min, rt1, rt2
    );
    // ⑨ 收官标记（smoke 第 22）。G43 拼行（S-2b 副核参与前多核撕行防线）：
    // 整条消息 PRINT_LOCK 内一次拼完。
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"J AIDEMO ok: rt1=");
        berry_hal_aarch64::uart_put_dec(rt1);
        berry_hal_aarch64::uart_puts(b" rt2=");
        berry_hal_aarch64::uart_put_dec(rt2);
        berry_hal_aarch64::uart_puts(b" min1=");
        berry_hal_aarch64::uart_put_dec(rt1_min);
        berry_hal_aarch64::uart_puts(b" min2=");
        berry_hal_aarch64::uart_put_dec(rt2_min);
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

// ---- 方案 B：rwake 真 sleep/wake 唤醒基准（HDR 第二个真实消费者；boot 门控
// spawn 块见上方 dpg::init 后；设计 = docs/调点调度-v1.3.md §9 留刀 +
// docs/调度器设计-v1.1.md §测量基础设施「boot 序列真 sleep/wake」）----

/// rwake 的 fw_cfg 具名项名（门控键——手跑/bench 专递
/// `-fw_cfg name=opt/berry/rwake,string=1`；RT_PROBE_NAME 同款「两侧对读
/// 维护」纪律）。独立键、不与 rt 同 boot 混发：两基准同 prio 8 并跑互相
/// 加噪（bench 单通道纪律——bench.py --rt 通道拒绝混跑同因）。
const RWAKE_PROBE_NAME: &str = "opt/berry/rwake";

/// rwake 样本轮数（任务书定值 64；间隔 = CNTFRQ/100 ≈ 10ms）。`BENCHRWAKE
/// n=` 收割面与此镜像（n = record_wake_lat 实收非零样本数 ≤ 64——lat==0
/// 被写点静默拒，实测结构上不可达：唤醒路径 IRQ+切换 ≥ 数百周期）。
const RWAKE_ROUNDS: u64 = 64;

/// rwake 任务（方案 B）：64 轮「block_current 纯睡到 deadline → IRQ tick
/// 超时唤醒」的唤醒延迟采样 → `record_wake_lat` 灌 HDR → merged_percentile
/// 出分位。与方案 A（ktest test_hdr_wake 忙等）的差别即本基准的存在理由：
/// 方案 A 量的是「轮询粒度」（醒来是自查 deadline），本任务真睡 SLEEPQ、由
/// on_irq_tick 的 expire_timeouts 摘队唤醒——量到的才是 v1 真实唤醒路径
/// （tick 粒度 + 出选调度）的延迟，HDR wake_lat 路自此有真消费者。
///
/// - **睡点 = `block_current(0, 0, deadline)`**：obj=0/signals=0 = 纯 sleep
///   （wake_matching 对 obj==0 直返 0——纯睡者只能超时自醒，无 signal 面；
///   task_sleeper 路径 1 同款先例）。deadline 直接传 CNTVCT 周期，不走 ns
///   换算（公约 5：换算舍入污染被测量；rt_probe 同纪律）。
/// - 包络（task_sleeper/rt_probe 同款，S-2 E0）：klock_enter → block →
///   **锁窗内先读 wake_vct** → klock_exit（F33/F42 顺序例外：block 返回前
///   舞蹈已重取锁，读点落在重取后且与醒点之间零 tick/切换噪声混入）。
/// - **漏整周期再锚（cyclictest overrun 形——rt_probe 首跑实踩判例）**：
///   间隔 10ms < v1 唤醒粒度（超时扫描只在 tick 跑 = 恒 1 slice = 100ms）
///   ⇒ 每轮 oversleep 几乎必 ≥ δ，纯 `deadline += interval` 绝对递推只会
///   量出漂移阶梯（对调度器的错误指控）——再锚 `醒时刻 + δ` 后每样本恢复
///   「δ 未来请求的唤醒延迟」语义。短档量出的正是定时器粒度本身（p50 ≈
///   1 slice 量级是 v1 的诚实答案；arm-to-deadline 落地后该值应塌缩，本
///   基准即其验收面）。
/// - 断言面 = L0 形状执法（接线执法非数值门禁，test_hdr_wake 同政策）：
///   醒因恒 Timeout / 负延迟 panic（再睡循环破损）/ HDR n ≥ 实录样本数
///   （写点与合并读面不失联）/ p99 ≥ p50（分位单调）。统计数值只出数
///   不设门（公约 2——数值归收割面同表比较）。
extern "C" fn task_bench_rwake(_arg: usize) -> ! {
    use berry_sched::{LatKind, WakeReason};
    // ~10ms（CNTFRQ/100）；不换算 ns——被测量保 CNTVCT 原周期口径。
    let interval = berry_hal_aarch64::timer::cntfrq() / 100;
    let mut deadline = berry_hal_aarch64::timer::cntvct() + interval; // 档首锚
    let mut recorded = 0u64; // 实录非零样本数（HDR n 面的对账基数）
    for _ in 0..RWAKE_ROUNDS {
        berry_hal_aarch64::kern_lock::klock_enter();
        let r = berry_sched::block_current(0, 0, deadline);
        // 顺序例外（rt_probe 行同款）：醒后锁窗内先读 wake_vct、后 klock_exit。
        let wake_vct = berry_hal_aarch64::timer::cntvct();
        berry_hal_aarch64::kern_lock::klock_exit();
        // obj=0 纯睡无 signal 面——非 Timeout = 纯睡语义被破（wake 面破损）。
        assert!(r == WakeReason::Timeout,
                "rwake: 醒因非 Timeout（obj=0 纯睡只能超时自醒——wake 面破损）");
        // oversleep = wake_vct − deadline；负值 = 提前醒 = 再睡循环破损（L0）。
        let lat = match wake_vct.checked_sub(deadline) {
            Some(l) => l,
            None => panic!("rwake: 负唤醒延迟（wake={} < deadline={}）= 再睡循环破损",
                           wake_vct, deadline),
        };
        // record 面值域 1 起（0 静默拒）——显式门 + 本地对账计数。
        if lat > 0 {
            berry_sched::record_wake_lat(lat); // 热路径真 API（本核槽化）
            recorded += 1;
        }
        // 漏整周期再锚（cyclictest overrun 形，取舍实录见任务 doc 注）。
        deadline = if lat >= interval { wake_vct + interval } else { deadline + interval };
    }
    // 分位/总数走 HDR 合并读面（跨核桶位直和；本任务恒 CPU0——place() 内核
    // 任务家核定值，record 已落 CPU0 槽，merged_* 两核和不受损）。
    let n = berry_sched::merged_total(LatKind::Wake);
    let p50 = berry_sched::merged_percentile(LatKind::Wake, 500);
    let p99 = berry_sched::merged_percentile(LatKind::Wake, 990);
    // L0 形状断言：n ≥ 实录（写点→per-CPU 槽→合并读面不失联；ktest 方案 A
    // 收口已清零、boot 期无其他 wake_lat 写者，等值是常态——取 ≥ 容后续
    // 新写者共灌）；p99 ≥ p50（rank 单调 ⇒ 桶上界单调，破坏 = 桶映射 bug）。
    assert!(n >= recorded && n > 0,
            "rwake: HDR 接线断（n={n} < recorded={recorded}——写点与合并读面失联）");
    assert!(p99 >= p50, "rwake: p99={p99} < p50={p50}（分位非单调 = 桶映射 bug）");
    // 单行标记（BENCHRT/BENCHWAKE 族形）：with_print_lock 整行拼完（G43——
    // smp_bringup 后裸打撕行面判例；纯 ASCII + uart 直写，内核任务非 console
    // 通道无分片面）。数值口径 = CNTVCT 周期（换算留收割面）。
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"BENCHRWAKE p50=");
        berry_hal_aarch64::uart_put_dec(p50);
        berry_hal_aarch64::uart_puts(b" p99=");
        berry_hal_aarch64::uart_put_dec(p99);
        berry_hal_aarch64::uart_puts(b" n=");
        berry_hal_aarch64::uart_put_dec(n);
        berry_hal_aarch64::uart_puts(b"\n");
    });
    // 跑完即退（REAPQ 单条流量，rt_probe 同款——非常驻）；标记在 exit 前
    //（其后是死码，Plan B v0.5 gotcha 同族）。
    berry_sched::exit_current()
}

// ---- G3-4：bench-lat IPC 路（草案 §4 wake→run 段补盲；boot 门控 spawn 块
// 见 main 尾段；设计 = docs/G3全抢占实施设计草案.md §4 + 基准与压力测试
// 设计.md R 线家族）----

/// bench-lat 的 fw_cfg 具名项名（门控键——bench.py --lat 专递
/// `-fw_cfg name=opt/berry/lat,string=1`；RT_PROBE_NAME 同款「两侧对读
/// 维护」纪律）。
const LAT_PROBE_NAME: &str = "opt/berry/lat";

/// bench-lat 每路样本数（草案 §4 直方图复用 R-0/R-1 家族口径；与 bench.py
/// BENCHLAT_RE 的 n=32 字面镜像——改样本数须同步分位秩与收割面）。
const LAT_SAMPLES: usize = 32;
// 分位秩与 LAT_SAMPLES 钉板耦合（rt_probe 同款编译期拦静默漂移）：
// p50 = ceil(50·32/100) = 16 → s[15]、p90 = ceil(90·32/100) = 29 → s[28]、
// p99 = ceil(99·32/100) = 32 → s[31]（== max，最近秩的诚实结果）。
const _: () = {
    assert!(LAT_SAMPLES == 32, "LAT_SAMPLES 变更须同步分位秩与 bench.py 镜像");
};

/// G3-5 候选① C 形（常驻再触发，evidence-g3-5recon.md 后继刀形 1）：
/// 两路 lat 触发 Event 的 ObjRef 地址（soak∧lat 双门在位时 soakmon 门块
/// 创建置入——soak-g3.sh 三键齐发形态；0 = 未装）。bench.py --lat 单门
/// （无 soakmon）= 两路保持一次性原形：waker/sleeper boot 周期毕读静态
/// 得 0 → exit_current 退场（bench.py --lat 收割面零改动）。
/// 信号面：task_soakmon 每拍行后在锁窗内 signal(LAT_TRIG_BIT) +
/// wake_matching；位清理由被唤醒 waker 自做（每事件恰一等待者——共享
/// 清位竞速结构性不发生）。漏拍自愈：signal 置位先于 wake_matching，
/// waker 周期竞速中未在等 = 位锁存，下轮 poll-first 即醒（追拍不丢；
/// 连续多拍并至只并成一次周期 = 收割行数短于拍数——宿主恰 N+1 行断言
/// 抓形破）。〔f4 F1 修实装 2026-11-05：两睡点睡前锁窗内补位 poll——
/// 原裸 block_current 不查位 = 本注宣称语义虚构，已修。〕
static LAT_TRIG_IPC_OBJ: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
static LAT_TRIG_SGI_OBJ: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
/// 触发位（LAT 家族位段 1<<48/49 顺延；内核内部位——Event 对象落内核
/// 私有表，不过 ABI object_signal 面；(obj,bit) 匹配键因对象隔离不撞）。
const LAT_TRIG_BIT: u64 = 1 << 50;
/// 触发等待远界（秒）：**首拍网格 t0+5×period=300s**（soakmon 有意留 5
/// 拍前置让基线落稳态）——boot 周期末重睡点到首触发 gap 恒 <300s（重睡点
/// X>0 ⟹ gap<300），稳态拍距 60s+jitter；480 = 300 + 60% 裕量。首验红案
/// （2026-11-05 验形跑）：180s「3 拍裕量」误按稳态拍距算——首拍前 180s
/// 界先到期 → 两路 sleeper 双 Timeout panic。连漏 8 拍 = Timeout 醒因断言
/// 红（fail-loud，不挂死）。同值喂 IPC 路 sleeper 握手 far（周期间隔同
/// 拍距）与 EL0 latslp 模板 δ（480e9 ns）。
const LAT_TRIG_FAR_S: u64 = 480;

/// lat sleeper（G3-4 IPC 路；G3-5 候选① C 形常驻化）：批环「32 轮睡
/// 唤醒 → run 侧测 wake→run 段 → 批毕打 BENCHLAT 行 → 回睡等下周期」。
///
/// - 睡点直调 `block_current`（rt_probe 同款：SVC 面折算被绕过，deadline
///   本来就是 CNTVCT 周期；**不走 ns 换算**——公约 5）。deadline = 480s
///   远界（C 形：首触发在 soakmon 首拍网格 t0+300s，稳态 60s 拍距——远界
///   口径见 LAT_TRIG_FAR_S 注；漏醒兜底 = Timeout 醒因断言 fail-loud，
///   不挂死）。
/// - **run 侧读点**（本路的被测量）：醒后续行锁窗内先取 `wake_stamp_take`
///   （= note_wake 戳）再读 now（rt_probe wake_vct 读序同款——读点与醒点
///   之间零 tick/切换噪声混入）；样本 = now − 戳，`checked_sub` 负值 panic
///   （L0 严格执法，R-0 on_wake 负延迟同族）。
/// - 醒因恒须 Signaled（waker 侧 wake_matching 按位唤醒；Timeout = 漏醒
///   = 握手面破损）；戳缺位（0）= note_wake 戳面破损——双双 L0 断言。
/// - 批尾：全排序 → 单行 `BENCHLAT,lane=ipc,n=,min=,p50=,p90=,p99=,max=`
///   （BENCHRT 六字段族形，G43 拼行）+ 完成标记 `LATIPC ok n=32`
///   （bench.py --lat 通道 DONE 面，`RT ok` 同形）。
/// - **C 形常驻再触发**（evidence-g3-5recon.md 后继刀形 1，2026-11-05）：
///   批毕读 `LAT_TRIG_IPC_OBJ`——非 0（soak∧lat 双门）= 回睡续批（下周期
///   由 soakmon 拍后触发 waker 再发起，批界即周期界恰一唤醒一样本）；
///   0（bench --lat 单门）= 原一次性形逐字保留（boot 周期照跑，采样毕
///   exit_current 退场——bench.py --lat 收割面零改动）。
extern "C" fn task_lat_slp(arg: usize) -> ! {
    use berry_sched::WakeReason;
    // 480s 远界（LAT_TRIG_FAR_S 注——首拍网格 300s 后才首触发，180s 会
    // 先到期；boot 周期毫秒-秒级轮转不贴界）。
    let far = LAT_TRIG_FAR_S * berry_hal_aarch64::timer::cntfrq();
    let mut samples = [0u64; LAT_SAMPLES];
    loop {
        for slot in samples.iter_mut() {
            berry_hal_aarch64::kern_lock::klock_enter();
            let r = berry_sched::block_current(
                arg, LAT_EV_BIT,
                berry_hal_aarch64::timer::cntvct() + far,
            );
            // 顺序例外（rt_probe 行同款，F33/F42 分工）：醒后锁窗内先取戳、
            // 后 klock_exit——读点即 run 侧首段内核执行（被测段的下界锚）。
            let wv = berry_sched::wake_stamp_take();
            let now = berry_hal_aarch64::timer::cntvct();
            berry_hal_aarch64::kern_lock::klock_exit();
            assert!(r == WakeReason::Signaled,
                    "lat_slp: 醒因非 Signaled（Timeout = waker 漏醒——握手面破损）");
            assert!(wv != 0, "lat_slp: wake 戳缺位（note_wake 戳面破损）");
            *slot = match now.checked_sub(wv) {
                Some(l) => l,
                None => panic!("lat_slp: 负 wake→run（now={} < 戳={}）= 时序破损",
                               now, wv),
            };
        }
        samples.sort_unstable(); // core 面板全排序（32 元素，零分配；rt_probe 同形）
        // 分位 = 最近秩 ceil(p·n/100)（n=32：p50=s[15]/p90=s[28]/p99=s[31]，
        // p99==max 是 32 样本最近秩的诚实结果——rt_probe 注同款，勿为观感改
        // 次大值）。行包络：with_print_lock 整行拼完（G43；每批一行，
        // BENCHRT 同形先例）。
        berry_hal_aarch64::kern_lock::with_print_lock(|| {
            berry_hal_aarch64::uart_puts(b"BENCHLAT,lane=ipc,n=");
            berry_hal_aarch64::uart_put_dec(LAT_SAMPLES as u64);
            berry_hal_aarch64::uart_puts(b",min=");
            berry_hal_aarch64::uart_put_dec(samples[0]);
            berry_hal_aarch64::uart_puts(b",p50=");
            berry_hal_aarch64::uart_put_dec(samples[15]);
            berry_hal_aarch64::uart_puts(b",p90=");
            berry_hal_aarch64::uart_put_dec(samples[28]);
            berry_hal_aarch64::uart_puts(b",p99=");
            berry_hal_aarch64::uart_put_dec(samples[31]);
            berry_hal_aarch64::uart_puts(b",max=");
            berry_hal_aarch64::uart_put_dec(samples[31]);
            berry_hal_aarch64::uart_puts(b"\n");
            berry_hal_aarch64::uart_puts(b"LATIPC ok n=");
            berry_hal_aarch64::uart_put_dec(LAT_SAMPLES as u64);
            berry_hal_aarch64::uart_puts(b"\n");
        });
        // C 形分叉：触发未装 = 一次性原形退场（bench --lat 单门）；已装 =
        // 回睡续批（批界即周期界——waker 拍后再发起恰 32 轮）。
        if LAT_TRIG_IPC_OBJ.load(core::sync::atomic::Ordering::Acquire) == 0 {
            break;
        }
    }
    berry_sched::exit_current()
}

/// lat waker（G3-4 IPC 路；G3-5 候选① C 形常驻化）：周期环「32 轮间隔
/// 纯睡 + `wake_matching` 唤醒 sleeper → 等 soakmon 拍后触发 → 下周期」。
///
/// - 间隔 = ¼ slice 纯睡（obj=0，唯一醒因 Timeout——间隔面的定时器唤醒
///   **不入** IPC 路测量窗：被测的是 sleeper 的 Signaled 唤醒）。
/// - 唤醒 = klock 窗内 `wake_matching(obj, LAT_EV_BIT)`（IPC send 尾的
///   唤醒面本体，IRQ 屏蔽窗契约同其真实调用面）；返回 1 = 恰唤醒 sleeper
///   （0 = sleeper 未在睡 = 握手竞速破损，L0 fail-loud 不静默重试）。
/// - P-B 抢占资格（G3-2 段①；f4 F15 改口——G3-2 谱系旧句失实）：8>0
///   sleeper 入 runqueue 即时抢占资格——但唤醒方不切换（本调用在 klock
///   屏蔽窗内无 tick），wake_matching 返回时 sleeper 仅就绪未续行；续行
///   待 waker 下轮间隔睡让出后 tick/yield 出选（n==1 断言由优先级出选
///   保真，无运行时影响）。
/// - **C 形触发等待**（evidence-g3-5recon.md 后继刀形 1，2026-11-05）：
///   boot 周期照跑（bench.py --lat 收割面零改动）后等 `LAT_TRIG_IPC_OBJ`
///   （soakmon 每拍行后 signal+wake_matching）。醒来同窗清触发位（§A61
///   清位纪律内核版：不清 = 恒真锁存下轮 poll 直醒变自旋；每事件恰一
///   等待者 = 无共享清位竞速）。静态 0（bench --lat 单门）= 原一次性形
///   退场。远界 480s（LAT_TRIG_FAR_S）：连漏 8 拍 = Timeout 醒因断言
///   fail-loud（soakmon 断流/周期竞速超 far）。
extern "C" fn task_lat_wak(arg: usize) -> ! {
    use berry_sched::WakeReason;
    let spacer = berry_hal_aarch64::timer::slice_cycles() / 4;
    let trig_far = LAT_TRIG_FAR_S * berry_hal_aarch64::timer::cntfrq();
    loop {
        for _ in 0..LAT_SAMPLES {
            berry_hal_aarch64::kern_lock::klock_enter();
            let r = berry_sched::block_current(
                0, 0,
                berry_hal_aarch64::timer::cntvct() + spacer,
            );
            berry_hal_aarch64::kern_lock::klock_exit();
            assert!(r == WakeReason::Timeout,
                    "lat_wak: 间隔睡醒因非 Timeout（obj=0 纯睡——跨表 wake 面破损）");
            berry_hal_aarch64::kern_lock::klock_enter();
            let n = berry_sched::wake_matching(arg, LAT_EV_BIT);
            berry_hal_aarch64::kern_lock::klock_exit();
            assert!(n == 1, "lat_wak: 唤醒数 {n} ≠ 1（sleeper 未在睡——首轮竞速面破损）");
        }
        // C 形：等 soakmon 拍后触发再发起下周期（0 = 一次性原形退场）。
        let trig = LAT_TRIG_IPC_OBJ.load(core::sync::atomic::Ordering::Acquire);
        if trig == 0 {
            break;
        }
        berry_hal_aarch64::kern_lock::klock_enter();
        // f4 F1 修（2026-11-05）：睡前锁窗内补位 poll——sched.rs block_current
        // 契约「调用方须已验条件不满足」前置补齐。soakmon 触发若已在本周期
        // 竞速窗内落位（signal 置位先于 wake_matching，waker 未在睡 = 位锁
        // 存于 Event），此处 load 查见即跳睡直取 = poll-first 追拍；未查见
        // 才睡——查位与入队同 KL 窗、写位与扫描同窗（event.rs 原子化纪律
        // 义务③），无丢唤醒面（trig 有效性同下 SAFETY 注）。
        let pending = unsafe {
            (*(trig as *mut crate::handle::ObjRef))
                .obj
                .as_event_mut()
                .expect("lat_wak: 触发 ObjRef 非 Event")
                .load()
                & LAT_TRIG_BIT
        } != 0;
        let mut r = WakeReason::Signaled;
        if !pending {
            r = berry_sched::block_current(
                trig, LAT_TRIG_BIT,
                berry_hal_aarch64::timer::cntvct() + trig_far,
            );
        }
        // SAFETY: trig = soakmon 门块置入 ht_soak（boot Box::leak 永生表）
        // 的 Event ObjRef——常驻任务生命周期内恒有效。block_current 返回
        // 已重取 KL（soakmon 包络同款），清位同窗后释放。
        unsafe {
            (*(trig as *mut crate::handle::ObjRef))
                .obj
                .as_event_mut()
                .expect("lat_wak: 触发 ObjRef 非 Event")
                .signal(LAT_TRIG_BIT, 0);
        }
        berry_hal_aarch64::kern_lock::klock_exit();
        assert!(r == WakeReason::Signaled,
                "lat_wak: 触发醒因非 Signaled（Timeout = 连漏 8 拍——soakmon 断流/竞速超 far）");
    }
    berry_sched::exit_current()
}

// ---- G3-4b：bench-lat SGI 路（EL0 用户对——内核任务对跨核被 place() 堵死
// 后的唯一可落形态；boot 门控 spawn 块见 main 尾段 SGI 门；载荷 =
// user.rs __user_latslp，测量钩子 = syscall.rs wait 醒来侧三落点）----

/// SGI 路 armed 门（boot SGI 门块置 true；缺席 = 钩子三落点恒短路——
/// 默认面每次 wait_one 只付一次寄存器比较）。Release 写：门块在 fleet
/// spawn 前落笔，thread_start 锁链保证 CPU1 侧可见序。
pub(crate) static LATSGI_ARMED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
/// SGI 路握手就绪面：wait 进入点钩子钉住的 sleeper Event ObjRef 地址
///（Release 写/Acquire 读）。也是 v1 无跨表 handle 传递下 waker 拿到
/// sleeper 进程私有表内对象的唯一桥。
pub(crate) static LATSGI_OBJ: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
/// SGI 路 wait 进入序号（每轮 wait_enter 递增——单调）。**跨 vCPU 握手序号
/// 面**：waker 轮询 `ENTER ≥ i+1` 才发信号——替代 guest 钟 spacer 盲 pacing
///（TCG 下 CPU0 的钟睡不保证 CPU1 vCPU 被宿主调度过——boot 高载饿副核，
/// sleeper 未及回睡即被下一轮信号 = n==0 竞速，首跑实录）。正确性根基 =
/// KL 串行：hook_enter 在 sleeper 的 SVC 屏蔽窗内执行，block_current 的
/// insert_sorted 先于 klock_release_switch——waker 取到 KL 时 sleeper
/// 必已入 SLEEPQ（观察 ENTER ≥ i+1 后取 KL = 无 TOCTOU 窗）。
pub(crate) static LATSGI_ENTER: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
/// SGI 路样本落柜计数（单写者 = sleeper 的 wait 醒来侧钩子 CPU1；waker
/// CPU0 Acquire 读到 32 = 全样本已发布——Release/Acquire 对 BUF 建
/// happens-before，单写者下计数即槽位）。
pub(crate) static LATSGI_DONE: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
/// SGI 路 sleeper 落核背书（wait_enter 钩子记 `cpu_id()`；waker 每轮断言
/// ==1〔f4 F3〕）。必要性：spread→CPU1 靠 `place()` 队列深度失衡启发，
/// 失衡不满足时静默留 CPU0 = lane=sgi 退化同核测量而 ENTER/n==1/DONE 全
/// 断言照过——跨核量级签名（p50 数量级差）此前只有人眼察觉；本原子把
/// 落核事实升为结构保证。发布序：Relaxed 先写，紧随的 ENTER fetch_add
///（Release）一并发布——waker Acquire 读到 ENTER 即读到本值。
pub(crate) static LATSGI_CPU: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
/// SGI 路样本柜（CPU1 写/CPU0 读；Relaxed 访问 + DONE 的 Release/Acquire
/// 配对提供跨核发布序）。
pub(crate) static LATSGI_BUF: [core::sync::atomic::AtomicU64; LAT_SAMPLES] =
    [const { core::sync::atomic::AtomicU64::new(0) }; LAT_SAMPLES];

/// SGI 路 wait 进入点钩子（`sys_object_wait_one` 解析并钉住 obj 后调用；
/// 调用面已过 `signals == LAT_SGI_BIT && LATSGI_ARMED` 双过滤）。
pub(crate) fn latsgi_wait_enter(obj: usize) {
    LATSGI_OBJ.store(obj, core::sync::atomic::Ordering::Release);
    LATSGI_CPU.store(
        berry_hal_aarch64::smp::cpu_id(),
        core::sync::atomic::Ordering::Relaxed,
    );
    LATSGI_ENTER.fetch_add(1, core::sync::atomic::Ordering::Release);
}

/// SGI 路 wait 醒来侧钩子（`sys_object_wait_one` 的 block_current 返回后
/// 调用——**run 侧读点本体**：醒后首段内核执行取戳+now，与 IPC 路
/// task_lat_slp 读点同定义）。非 Signaled 醒因不采样（重验/尾钩执法面）。
pub(crate) fn latsgi_wake(reason: berry_sched::WakeReason) {
    if reason != berry_sched::WakeReason::Signaled {
        return;
    }
    let wv = berry_sched::wake_stamp_take();
    assert!(wv != 0, "latsgi: wake 戳缺位（note_wake 戳面破损）");
    let now = berry_hal_aarch64::timer::cntvct();
    let lat = match now.checked_sub(wv) {
        Some(l) => l,
        None => panic!("latsgi: 负 wake→run（now={now} < 戳={wv}）= 时序破损"),
    };
    // 单写者（sleeper 的 SVC 路径 CPU1）：DONE 即本样本槽位，先存样本后
    // 发布计数（Release）——waker Acquire 读到计数即读到样本。
    let i = LATSGI_DONE.load(core::sync::atomic::Ordering::Relaxed);
    assert!(i < LAT_SAMPLES,
            "latsgi_wake: 样本柜索引 {i} ≥ {LAT_SAMPLES}（一轮多睡/位复用/改样本数类演化先在此 fail-loud，非静默越界写静态数组〔f4 F4〕）");
    LATSGI_BUF[i].store(lat, core::sync::atomic::Ordering::Relaxed);
    LATSGI_DONE.fetch_add(1, core::sync::atomic::Ordering::Release);
}

/// SGI 路 wait 返程尾钩子（`sys_object_wait_one` 出口调用；非 0 返回 =
/// 超时/错路 = waker 漏醒——L0 fail-loud，与 IPC 路醒因断言同族执法）。
pub(crate) fn latsgi_ret(ret: i64) {
    if ret != 0 {
        panic!("latsgi: wait 返程 ret={ret}（Timeout = waker 漏醒——握手面破损）");
    }
}

/// SGI 路 waker/master（G3-4b；G3-5 候选① C 形常驻化）：spawn EL0 舰队
/// （root 占位 + sleeper spread→CPU1，恰一次——A45 泄漏 × 周期 respawn =
/// 线性累积判死，侦察档三形裁决 C 形）→ 周期环「N 轮**等 sleeper wait
/// 进入序号** → Event 置位 + `wake_matching` 跨核唤醒」→ 收 32 样本 →
/// 全排序打印 `BENCHLAT,lane=sgi` + `LATSGI ok` → 复位样本柜 → 等
/// soakmon 拍后触发 → 下周期。
///
/// - **唤醒面 = sys_object_signal 尾同链**（`Event::signal(0, bit)` +
///   `wake_matching(addr, bit)`）：跨核睡的醒来重验（`signal_state` 重读
///   Event 态）须真置位才命中——纯 `wake_matching` 不写 Event 态即漏醒
///   （LAT_EV_BIT 注同源口径）。klock 窗内调用（真实调用面同屏蔽窗契约）。
/// - **跨 vCPU 握手 = 序号面非钟面**（首跑实录：guest 钟 spacer 盲 pacing
///   在 TCG boot 高载下饿副核 vCPU——CPU0 钟睡了 slice/4 而 CPU1 尚未跑
///   完上轮回睡，n==0 竞速 panic）。每轮先有界轮询 `LATSGI_ENTER ≥ 目标`
///   （hook_enter 在 sleeper 的 SVC 屏蔽窗内递增；block 的 insert_sorted
///   先于 klock_release_switch ⇒ waker 观察到序号后取 KL，sleeper 必已
///   在 SLEEPQ——KL 串行闭 TOCTOU 窗），再发信号。n==1 断言保留 = 终背书。
/// - **C 形绝对序号目标**（2026-11-05）：EL0 sleeper 永续等待不清零 →
///   ENTER 单调永续，轮目标 = cyc*32+i+1（boot 周期 cyc=0 照跑 = bench.py
///   --lat 收割面零改动）。每周期毕复位 LATSGI_DONE=0（单写者交接窗：
///   末样本已发布、sleeper 已回睡，下周期首轮 signal 前零写者——KL 串行
///   闭并发窗；latsgi_wake 的 i<LAT_SAMPLES 断言逐周期照执法）。
/// - 被测段 = note_wake 戳（CPU0 唤醒侧）→ SGI 到达尾延 → CPU1 park 出场
///   → 派发切回 → sleeper 醒后首段内核执行取戳。**勘正〔f4 F2〕：远端 RQ
///   入队与 SGI 发送（`enqueue_with_kick` 内含 send_sgi MMIO）在 wake_one_inner
///   实序中先于 note_wake 戳完成、不在被测段内**；IPC 路 = 同核直切段
///   （两 lane 同点打戳，对照差 = 尾延+出场+切回段的跨核增量）。
/// - 收尾等位：DONE 计数 Acquire 读到 32（有界轮询，超时 L0——TCG 副核
///   滞后同因，预算按 lag 容忍非按工作量）→ 排序打印 → 触发等待
///   （`LAT_TRIG_SGI_OBJ`，同 task_lat_wak 形——0 = bench --lat 单门
///   一次性原形退场）。
extern "C" fn task_latsgi_wak(_arg: usize) -> ! {
    use berry_sched::WakeReason;
    // 舰队 spawn（PP 同形：svc 链落本任务私有表，handle 常驻泄漏持有）。
    // root x0=0 占位即刻退；写者 x0=1 = sleeper spread→CPU1。
    let _fleet = storm_spawn_fleet("latsgi", crate::user::user_latslp_code(), 1, 1);
    // 轮询节拍 = ¼ slice 纯睡（obj=0，唯一醒因 Timeout——序号未到位时的
    // 让位等，不参与测量窗）。
    let spacer = berry_hal_aarch64::timer::slice_cycles() / 4;
    let trig_far = LAT_TRIG_FAR_S * berry_hal_aarch64::timer::cntfrq();
    let mut obj = 0usize;
    // 周期序（boot 周期 = 0 立跑；此后每 soakmon 拍一周期）。
    let mut cyc = 0usize;
    loop {
        for i in 0..LAT_SAMPLES {
            // 等 sleeper 第 cyc*32+i+1 次 wait 进入（单调 ≥——首轮兼就位
            // 握手；有界，超时 L0：sleeper 断链/err 早退同状收口）。
            let target = cyc * LAT_SAMPLES + i + 1;
            let mut enter = 0usize;
            for _ in 0..LATSGI_POLL_MAX {
                enter = LATSGI_ENTER.load(core::sync::atomic::Ordering::Acquire);
                if enter >= target {
                    break;
                }
                berry_hal_aarch64::kern_lock::klock_enter();
                let r = berry_sched::block_current(0, 0, berry_hal_aarch64::timer::cntvct() + spacer);
                berry_hal_aarch64::kern_lock::klock_exit();
                assert!(r == WakeReason::Timeout, "latsgi_wak: 等位睡醒因非 Timeout");
            }
            assert!(enter >= target,
                    "latsgi_wak: 周期 {cyc} 第 {} 轮 wait 进入序号超时（enter={enter}——sleeper 断链/err 早退）",
                    i + 1);
            // 落核结构背书（f4 F3）：ENTER 的 Release/Acquire 对同时发布了钩子
            // 先写的 CPU 落点——place() 失衡启发未跨核时在此 fail-loud，杜绝
            // lane=sgi 静默退化同核。
            let cpu = LATSGI_CPU.load(core::sync::atomic::Ordering::Acquire);
            assert!(cpu == 1,
                    "latsgi_wak: sleeper 落 CPU{cpu} ≠ 1（place() 未跨核——lane=sgi 将退化为同核测量）");
            if obj == 0 {
                obj = LATSGI_OBJ.load(core::sync::atomic::Ordering::Acquire);
            }
            berry_hal_aarch64::kern_lock::klock_enter();
            // SAFETY: OBJ = wait 进入点钩子钉住的活 ObjRef——deref 时 sleeper
            // 恒在睡（wait_one 的 retain 钉住覆盖全部睡眠轮次，§A28 跨睡眠
            // 版；C 形 sleeper 永续等待 = 钉住永不解除，比一次性形更强）；
            // 序号+KL 串行背书；n==1 断言即时复核。
            let objp = obj as *mut crate::handle::ObjRef;
            unsafe {
                (*objp)
                    .obj
                    .as_event_mut()
                    .expect("latsgi_wak: ObjRef 非 Event")
                    .signal(0, LAT_SGI_BIT);
            }
            let n = berry_sched::wake_matching(obj, LAT_SGI_BIT);
            berry_hal_aarch64::kern_lock::klock_exit();
            assert!(n == 1, "latsgi_wak: 唤醒数 {n} ≠ 1（sleeper 未在睡——握手竞速面破损）");
        }
        // 收尾等位：末轮样本由 CPU1 异步落柜（DONE Acquire 读 32 = 全发布）。
        let mut done = 0usize;
        for _ in 0..LATSGI_POLL_MAX {
            done = LATSGI_DONE.load(core::sync::atomic::Ordering::Acquire);
            if done == LAT_SAMPLES {
                break;
            }
            berry_hal_aarch64::kern_lock::klock_enter();
            let r = berry_sched::block_current(0, 0, berry_hal_aarch64::timer::cntvct() + spacer);
            berry_hal_aarch64::kern_lock::klock_exit();
            assert!(r == WakeReason::Timeout, "latsgi_wak: 收尾等位醒因非 Timeout");
        }
        assert!(done == LAT_SAMPLES,
                "latsgi_wak: 样本落柜超时（done={done} < {}——sleeper err 早退或挂死）",
                LAT_SAMPLES);
        let mut samples = [0u64; LAT_SAMPLES];
        for i in 0..LAT_SAMPLES {
            samples[i] = LATSGI_BUF[i].load(core::sync::atomic::Ordering::Relaxed);
        }
        samples.sort_unstable(); // 全排序精确分位（IPC 路同形；p99==max=最近秩诚实结果）
        berry_hal_aarch64::kern_lock::with_print_lock(|| {
            berry_hal_aarch64::uart_puts(b"BENCHLAT,lane=sgi,n=");
            berry_hal_aarch64::uart_put_dec(LAT_SAMPLES as u64);
            berry_hal_aarch64::uart_puts(b",min=");
            berry_hal_aarch64::uart_put_dec(samples[0]);
            berry_hal_aarch64::uart_puts(b",p50=");
            berry_hal_aarch64::uart_put_dec(samples[15]);
            berry_hal_aarch64::uart_puts(b",p90=");
            berry_hal_aarch64::uart_put_dec(samples[28]);
            berry_hal_aarch64::uart_puts(b",p99=");
            berry_hal_aarch64::uart_put_dec(samples[31]);
            berry_hal_aarch64::uart_puts(b",max=");
            berry_hal_aarch64::uart_put_dec(samples[31]);
            berry_hal_aarch64::uart_puts(b"\n");
            berry_hal_aarch64::uart_puts(b"LATSGI ok n=");
            berry_hal_aarch64::uart_put_dec(LAT_SAMPLES as u64);
            berry_hal_aarch64::uart_puts(b"\n");
        });
        // C 形周期复位（交接窗安全性见任务注「C 形绝对序号目标」——
        // 复位先于下周期首轮 signal，窗内零 BUF/DONE 写者）。
        LATSGI_DONE.store(0, core::sync::atomic::Ordering::Release);
        cyc += 1;
        // C 形：等 soakmon 拍后触发再发起下周期（同 task_lat_wak 形；0 =
        // bench --lat 单门一次性原形退场——EL0 sleeper 不随退：480s wait
        // 超时的真兜底 = 内核 latsgi_ret 尾钩 L0 panic 先响〔死讯 fail-loud
        // 非 EL0 自回收——err 早退对超时不可达，f4 F7 改口〕；现网两
        // harness 均先杀 QEMU 收割，此路不触发）。
        let trig = LAT_TRIG_SGI_OBJ.load(core::sync::atomic::Ordering::Acquire);
        if trig == 0 {
            break;
        }
        berry_hal_aarch64::kern_lock::klock_enter();
        // f4 F1 修（2026-11-05）：睡前锁窗内补位 poll（同 task_lat_wak 形
        // ——sched.rs 契约前置 + 位锁存 poll-first 追拍实装；查位与入队同
        // KL 窗，无丢唤醒面）。
        let pending = unsafe {
            (*(trig as *mut crate::handle::ObjRef))
                .obj
                .as_event_mut()
                .expect("latsgi_wak: 触发 ObjRef 非 Event")
                .load()
                & LAT_TRIG_BIT
        } != 0;
        let mut r = WakeReason::Signaled;
        if !pending {
            r = berry_sched::block_current(
                trig, LAT_TRIG_BIT,
                berry_hal_aarch64::timer::cntvct() + trig_far,
            );
        }
        // SAFETY: 同 task_lat_wak 注——ht_soak 永生表 ObjRef，KL 窗内清位。
        unsafe {
            (*(trig as *mut crate::handle::ObjRef))
                .obj
                .as_event_mut()
                .expect("latsgi_wak: 触发 ObjRef 非 Event")
                .signal(LAT_TRIG_BIT, 0);
        }
        berry_hal_aarch64::kern_lock::klock_exit();
        assert!(r == WakeReason::Signaled,
                "latsgi_wak: 触发醒因非 Signaled（Timeout = 连漏 8 拍——soakmon 断流/竞速超 far）");
    }
    berry_sched::exit_current()
}

/// SGI 路有界轮询帽（就位/收尾两等位共用；¼ slice × 4096 ≈ 1024 slice ≈
/// 100s @10Hz 片频——sleeper 就位/回环实测 ≪ 单个 spacer，帽 = 故障时
/// fail-loud 上界非正常路径预算）。
const LATSGI_POLL_MAX: usize = 4096;

// ---- S-1：soakmon（soak 长跑心跳，boot 门控 spawn 块见 main 尾段；
// 设计 = docs/基准与压力测试设计.md §11 + 落地实录。命名注记：sched.rs 的
// SOAK_PRINT_EVERY（金丝雀节流常量）与 task_a 注释「SOAK=1 低噪 soak」是
// 历史名，与本文 soakmon 无关，勿混读）----

/// soakmon 的 fw_cfg 具名项名（门控键——soak.sh 专递
/// `-fw_cfg name=opt/berry/soak,string=1`；RT_PROBE_NAME 同款「两侧对读
/// 维护」纪律）。
const SOAK_PROBE_NAME: &str = "opt/berry/soak";

/// soakmon（S-1）：常驻心跳任务，每 60s（CNTVCT 周期口径）打一行
/// `SOAK t=<拍序> sw=<切换总数> sw0=<CPU0 切换> sw1=<CPU1 切换>
/// park0=<CPU0 park> park1=<CPU1 park> pages=<allocated_pages>
/// lost=<RX丢字> churn=<载荷vmo_create成功数>`
///（S-5d 心跳 v2 八字段 + S-5e M1 v2.1 追加第 9 字段——端到端防假绿）。
///
/// - 九字段全是现成计数器直读（`berry_sched::switches` = switches=N 金丝雀
///   同源标量；`smp::sw_park_readout` = S-5a per-CPU sw/park 四数〔跨核
///   volatile 单字读，谓词为严格递增/有界增量断言，无需一致快照〕；
///   `mm::frames().allocated_pages` = B-0/S-2 对账口径（D24 起
///   O(1) 水位原子读，口径由 buddy 对拍测试钉板）；`uart::rx_lost` =
///   console 环丢字；`syscall::soakload_churn_readout` = S-5e M1 载荷
///   进度计数〔churn 冻结 = svc 恒败/gen 顶格/写者被杀——soak-smp 断言
///   严格递增；单核 soak 无载荷恒 0 不断言〕）——零新锁零新打印面。宿主
///   断言分工：soak-smp = sw0/sw1 各自严格递增（对核冻死即红）+ park
///   增量有界 + churn 严格递增（载荷业务进度）；单核 soak.sh = 副核四数
///   恒零自证（sw1/park1 零活性；churn 恒 0 无载荷面）。
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
///   行 ≤256B（v2.1 固定段 56B + 9 数值字段各至多 20 位 = 绝对界 236B）。
/// - **G3-5 候选① C 形拍后触发**（2026-11-05）：拍行打出后、同 klock 窗
///   内 signal+wake_matching 两路 lat 触发 Event（静态 0 = 未装，寄存器
///   比较零开销跳过）——BENCHLAT 周期行与 SOAK 拍行一一对应（宿主恰
///   N+1 行断言），周期时延即稳态受扰时延（被测对象本体）。
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
        //（G43 拼行——KERN→PRINT 单向嵌套合法；G3-1c-1 改版后包络 = preempt
        // 计数对不触 DAIF，本调用点在 klock 屏蔽窗内 = 出口跳检查点、屏蔽态
        // 原样维持——旧「had_irq=false 不误恢复」机制随 irqsave 包络退役）。
        assert!(r == WakeReason::Timeout,
                "soakmon: 醒因非 Timeout（哑 Event 永不置位——跨表 wake 面破损）");
        let sw = berry_sched::switches();
        // S-5d 心跳 v2（D4）：per-CPU sw/park 四数入行；S-5e M1 v2.1 追加 churn。
        let (sw0, sw1, park0, park1) = berry_hal_aarch64::smp::sw_park_readout();
        let pages = mm::frames().allocated_pages() as u64;
        let lost = berry_hal_aarch64::uart::rx_lost() as u64;
        // S-5e M1（心跳 v2.1）：载荷业务进度第 9 字段——调度/内存面照绿而
        // churn 冻结 = svc 恒败/gen 顶格/写者被杀，端到端假绿在此开口。
        let churn = crate::syscall::soakload_churn_readout();
        berry_hal_aarch64::kern_lock::with_print_lock(|| {
            berry_hal_aarch64::uart_puts(b"SOAK t=");
            berry_hal_aarch64::uart_put_dec(n);
            berry_hal_aarch64::uart_puts(b" sw=");
            berry_hal_aarch64::uart_put_dec(sw);
            berry_hal_aarch64::uart_puts(b" sw0=");
            berry_hal_aarch64::uart_put_dec(sw0);
            berry_hal_aarch64::uart_puts(b" sw1=");
            berry_hal_aarch64::uart_put_dec(sw1);
            berry_hal_aarch64::uart_puts(b" park0=");
            berry_hal_aarch64::uart_put_dec(park0);
            berry_hal_aarch64::uart_puts(b" park1=");
            berry_hal_aarch64::uart_put_dec(park1);
            berry_hal_aarch64::uart_puts(b" pages=");
            berry_hal_aarch64::uart_put_dec(pages);
            berry_hal_aarch64::uart_puts(b" lost=");
            berry_hal_aarch64::uart_put_dec(lost);
            berry_hal_aarch64::uart_puts(b" churn=");
            berry_hal_aarch64::uart_put_dec(churn);
            berry_hal_aarch64::uart_puts(b"\n");
        });
        // G3-5 候选① C 形：拍后触发两路 lat waker（恰在本拍行后——周期行
        // 与拍行一一对应）。锁窗序 = sys_object_signal 尾同链（signal 置位
        // → wake_matching；latsgi_wak 真实调用面同款）；n 不断言：waker
        // 周期竞速中未在等 = 位锁存 poll-first 自愈追拍（收割面恰 N+1 行
        // 断言兜形破，LAT_TRIG_IPC_OBJ 静态注）。SAFETY: 静态值 = 门块置入
        // ht_soak（boot Box::leak 永生表）的 Event ObjRef——本任务生命周期
        // 内恒有效。
        for trig in [&LAT_TRIG_IPC_OBJ, &LAT_TRIG_SGI_OBJ] {
            let t = trig.load(core::sync::atomic::Ordering::Acquire);
            if t != 0 {
                unsafe {
                    (*(t as *mut crate::handle::ObjRef))
                        .obj
                        .as_event_mut()
                        .expect("soakmon: 触发 ObjRef 非 Event")
                        .signal(0, LAT_TRIG_BIT);
                }
                berry_sched::wake_matching(t, LAT_TRIG_BIT);
            }
        }
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
/// **统一探测/参与环**（两态合一；S-3a tickless park 形——子案 §9.5 步 A）：
/// gate 未置位 = park_arm 走 arm_slice 周期醒（探测态轮询，粒度 = 1 slice
/// ——park_recheck 见 gate 关即早退，置位后 ≤1 slice 内开始真派发）；gate
/// 置位 = park_recheck 承接一切派发（CNTV 醒 eret 后单路径于此——**截止
/// 驱动醒**口径），park_arm 按 SLEEPQ 最近 deadline 浅 park（到点醒 →
/// on_irq_tick expire → 回环 recheck 摘醒者）或 mask 深 park（SLEEPQ 尽空
/// 时）。自续性实证（noSGI 150s）：钉本核任务阻塞带 deadline 入 SLEEPQ →
/// arm_at 到点醒 → 自续 27 笔派发——「深 park 无 SGI 结构性饿死」对当前
/// 面不成立（CPU1 唯一客户 = 恒 deadline 阻塞的 pinned child；边界 = 无
/// 限期阻塞 + SLEEPQ 空窗时外部唤醒，留 S-3b 路由化收口）。原义务①
/// enqueue SGI 已除名（判词 → 子案 §9.5），唤醒戳随 S-3b 回归。
/// 任务尽则 block/exit/yield 切回本环（sec_park_ctx）续行。
/// wfi 真睡且全程 PSTATE.I=1（masked-wfi：GIC 线 pending 即醒，enable→wfi
/// 间到达的 IRQ 先被服务后 wfi 直睡的丢唤醒窗由此收口——Linux arm64
/// cpu_do_idle 同款；TCG 实测 wfe 立返拖垮主核吞吐——S-1 铁律）。
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
        // S-4 4-1：本核 SGI_TLB_SHOOTDOWN(2) banked 补使能（CPU0 init 位点同
        // 款纪律）。bringup probe ④ 早于此也可能已锁存 pending——使能即投递。
        berry_hal_aarch64::gic::enable_sgi_this_core(
            berry_hal_aarch64::gic::SGI_TLB_SHOOTDOWN,
        );
    }
    loop {
        // S-3a tickless park 环（子案 §9.5 步 A）：派发收敛到 park_recheck
        //（CNTV 醒 eret 后到这——截止驱动醒口径）；武装策略按 gate 两态。
        // SAFETY: PSTATE.I=1（起手 DAIF 窗 / 上轮 disable_irq）；klock 自管。
        unsafe { berry_sched::park_recheck() };
        // PAR 清钉（tickless 伴生洞收口）：深 park 窗无 tick 采样，last_task
        // 若滞留上一个 EL0 任务指针，CPU0 侧 witness 会误置假并行见证——
        // wfi 前清（本核块独占写、无跨核读：par_sample 的 cpu==0 分支才读
        // 对岸，免锁）。
        berry_hal_aarch64::smp::par_sample(0, false);
        if berry_hal_aarch64::smp::par_enabled() {
            match berry_sched::nearest_deadline() {
                // SAFETY: 本核 CVAL 编程到 SLEEPQ 最近 deadline（绝对截止，
                // 过期即醒由回环 expire 承接）；屏蔽窗内。
                Some(d) => unsafe { berry_hal_aarch64::timer::arm_at(d) },
                // SAFETY: CNTV IMASK=1（深 park——SLEEPQ 尽空无醒源，倒计仍
                // 跑；后续就绪的钉本核任务暂由 S-3b 路由化前的边界注收口，
                // 详函数头自续性实证段）。
                None => unsafe { berry_hal_aarch64::timer::mask() },
            }
        } else {
            // SAFETY: 本核 CVAL 重臂（探测态周期醒，gate 开启 ≤1 slice 可见）；
            // DAIF 屏蔽窗内。
            unsafe { berry_hal_aarch64::timer::arm_slice() };
        }
        // masked-wfi（丢唤醒窗收口）：wfi 全程 PSTATE.I=1——若先 enable 再
        // wfi，enable→wfi 间到达的 IRQ 会被先服务（pending 位消费掉）后 wfi
        // 直睡 = 丢唤醒；屏蔽态 wfi 由 GIC 线 pending 唤醒，醒后 enable_irq
        // 即取该中断（无 pending 则直落 disable，窗微无害）——三步闭环。
        // S-5a：park 入口计数（本核块独占写；TPIDR 未挂的防御窗内早退）。
        berry_hal_aarch64::smp::note_park_cpu();
        berry_hal_aarch64::wfi();
        berry_hal_aarch64::enable_irq();
        berry_hal_aarch64::disable_irq();
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

// ---- S-3b b-2：LB 金丝雀（子案 §9.6 义务③④⑤；boot 门控块见 main 尾段）----

/// LB gate 的 fw_cfg 具名项名（门控键——smoke-smp 相 3 专递
/// `-fw_cfg name=opt/berry/lb,string=1`；RT/SOAK_PROBE_NAME 同款「两侧对读
/// 维护」纪律）。
const LB_PROBE_NAME: &str = "opt/berry/lb";

/// ASID 回卷 gate 的 fw_cfg 具名项名（S-4 4-3 门控键——smoke-smp 相 4 专递
/// `-fw_cfg name=opt/berry/asidroll,string=1`；LB_PROBE_NAME 同款「两侧对读
/// 维护」纪律）。
const ASIDROLL_PROBE_NAME: &str = "opt/berry/asidroll";

/// S-4 4-4 TLB shootdown canary 压测门控名（smoke-smp 相 5 专递：
/// `-fw_cfg name=opt/berry/tlbs,string=1`；ASIDROLL 同款「两侧对读维护」
/// 纪律——tools/smoke-smp.sh 相 5 段与本常量对读）。
const TLBS_PROBE_NAME: &str = "opt/berry/tlbs";

/// LB 子进程数（蓝图 N=4）：LB_PROCS 槽位、`LB ok` 行 n= 字面、smoke 相 3
/// 的 n==4 断言三面钉同值——改此须三处同步。
const LB_CHILD_N: usize = 4;

/// TLB canary 写者线程数（S-4 4-4，lb 4-burst 同构）。
const TLBS_WRITER_N: usize = 4;

/// TLB canary 每轮参与者总数 = root（家核）+ `TLBS_WRITER_N` 写者。双轮
/// （decommit/Drop）毕 `TLBS ok n=` 行 n = 2 × 本值 = 10，与 smoke 相 5
/// n==10 断言三面钉同值——改此须三处同步。
const TLBS_PARTICIPANTS: usize = 1 + TLBS_WRITER_N;
/// TLBS 诊断：本轮 root Thread handle（fleet 装配期单写；收割失败时 poll
/// TERMINATED 区分「root 未跑」与「root 已写成功 = CPU0 stale TLB」）。
static TLBS_ROOT_H: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// S-5b 相 6 ALLOC churn 压测门控名（smoke-smp 相 6 专递：
/// `-fw_cfg name=opt/berry/alloc,string=1`；lb/asidroll/tlbs 同款「两侧
/// 对读维护」纪律——tools/smoke-smp.sh 相 6 段与本常量对读）。
const ALLOC_PROBE_NAME: &str = "opt/berry/alloc";

/// S-5b 相 7 HT 风暴压测门控名（smoke-smp 相 7 专递：
/// `-fw_cfg name=opt/berry/ht,string=1`；ALLOC 同款「两侧对读维护」纪律）。
const HT_PROBE_NAME: &str = "opt/berry/ht";

/// storm fleet 写者线程数（S-5b 相 6/7 同构：root 家核 + N 写者
/// spread=true → K=2 路由 CPU1——lb 4-burst/tlbs 同构）。
const STORM_WRITER_N: usize = 4;

/// storm 每轮参与者总数 = root + `STORM_WRITER_N`（水位/终账分母）。
const STORM_PARTICIPANTS: usize = 1 + STORM_WRITER_N;

/// storm 每参与者 churn 轮数。`ALLOC ok n=`/`HT ok n=` 行 n =
/// `STORM_ROUNDS` × `STORM_PARTICIPANTS` = 320，与模板 asm movz #64
/// （user.rs `__user_allocst`/`__user_htst`）、smoke 相 6/7 n==320 断言
/// 三面钉同值——改此须三处同步。
const STORM_ROUNDS: usize = 64;

/// storm 窗口偏移（模板 asm 立即数三面钉同值——见 user.rs 模板头注）：
/// 0x40 churn 出参 handle / 0x80 err 计数 / 0x90 完成轮数 / 0xC0
/// vmo_write 源。
const STORM_WIN_ERR: usize = 0x80;
const STORM_WIN_DONE: usize = 0x90;
const STORM_WIN_READY: usize = 0x98; // 相 7 二次放行形：就绪位（0x90 槽 8B 域外）

/// S-5c 相 8 PP ping-pong 压测门控名（smoke-smp 相 8 专递：
/// `-fw_cfg name=opt/berry/pp,string=1`；ALLOC/HT 同款「两侧对读维护」
/// 纪律——tools/smoke-smp.sh 相 8 段与本常量对读）。
const PP_PROBE_NAME: &str = "opt/berry/pp";

/// PP 交替轮数（单进程双线程各计 N）。`PP ok a= b=` 行 = 各自 N=320，
/// 与模板 asm movz #0x140（user.rs `__user_pp`）、smoke 相 8 a==b==320
/// 断言三面钉同值——改此须三处同步。
const PP_ROUNDS: usize = 320;

/// S-5d soakload 满载载荷门控名（tools/soak-smp.sh 专递：
/// `-fw_cfg name=opt/berry/soakload,string=1`；ALLOC/HT/PP 同款「两侧
/// 对读维护」纪律——tools/soak-smp.sh 门控段与本常量对读）。
const SOAKLOAD_PROBE_NAME: &str = "opt/berry/soakload";

/// soakload 写者数（spread→CPU1 互抢轮转——宿主 sw1 严格递除断言的物理
/// 来源：单写者恒 Running 无抢占对象 → sw1 冻结；两写者 tick 轮转 ≈10Hz）。
/// root（家核）另计 = 舰队共 1+N。满载谓词：写者恒 Runnable → CPU1 不
/// idle → park1 停增；任一写者死 → CPU1 idle park 环 ≈10Hz 刷穿宿主帽。
const SOAKLOAD_WRITER_N: usize = 2;

/// LB 计算线程 Thread ObjRef 地址表（per-child 归因判别面，义务③ 9.2 裁决二）：
/// lb_master 造线程时写入（thread_create 后、thread_start 前——thread_key
/// 值在 create 即定）；sched 侧 tick 采样经 `set_lb_match` 注入的 [`lb_match`]
/// 读表比对 `cur.thread_key`。归因键选 thread_key（Thread ObjRef 地址）而非
/// 裸 Task 指针：svc 链不透出任务指针，而 handle→ObjRef 内核直取（get_raw）
/// 在同 crate 面内；thread_key 在任务全生命周期恒定（F-E1 前移写点），无
/// ABA 面。进程 root（spread=false 家核自旋，仅为保进程 Alive）为 Process
/// 键，天然不入集——归因只记 4 计算线程。
/// Relaxed：写点在 lb_master 语境（klock 窗内直调写）、读点在 tick 锁窗
/// ——锁序传递可见性，计数纯归因无跨核序角色。
static LB_PROCS: [core::sync::atomic::AtomicUsize; 4] = [
    core::sync::atomic::AtomicUsize::new(0),
    core::sync::atomic::AtomicUsize::new(0),
    core::sync::atomic::AtomicUsize::new(0),
    core::sync::atomic::AtomicUsize::new(0),
];

/// LB 线程判别（sched 侧注入回调体）：thread_key ∈ LB_PROCS 即 LB 计算线程。
fn lb_match(key: usize) -> bool {
    LB_PROCS
        .iter()
        .any(|p| p.load(core::sync::atomic::Ordering::Relaxed) == key)
}

/// lb_master（S-3b b-2 LB 金丝雀主驱动，prio 8，私有表 A45）：fw_cfg lb
/// gate + smp_active 双门控 spawn（boot 块）。真 svc 驱动链（EL1 同权 A14
/// + waker2/ai_demo `svc4`/`svc6` 先例）造 **单进程五任务**：root 经
/// process_start（spread=false 恒家核）+ 4 计算线程经 thread_start
/// （spread=true，place K=2 深度失衡路由）→ wait_many（0x05，WAIT_ANY
/// 哨兵）挂 4 个 Thread 的 `SIGNAL_TERMINATED` 收割 → klock 包络内 Δ 采样
/// 打 `LB ok c0=Δ c1=Δ st1=Δ n=4`。机制语义（蓝图钉板）：
///
/// - **spread 混形 = 对齐已修订蓝图**（裁决二 + F-A1 修订 + place 终裁）：
///   「4 子结构性全落 rq0、偷取是唯一路径」旧叙事已被 F-A1 修订明文废除
///   （K=2 算术与「全落 rq0」互斥），断言与路由形态**解耦**——直接路由与
///   偷取**同格合格**。首版实现误钉 process_start（spread=false），run7
///   （rootfs 形态）/run8（bare 验收形态）双实证：单发 FIFO 头偷取在常驻
///   矩阵噪声下**结构性到不了子**（st1=1 偷走最老常驻者后迁移粘性占用
///   CPU1，rq1 永不清空、探测停摆——c1=0 恒红，非竞速）。改 thread_start
///   后 4-burst K=2 算术保底第 3 线程直路 rq1 + SGI 醒 park_recheck 派发
///   ——c1 见证「子任务在副核跑」的 G2 均衡性质本身；偷取（F-E2 过滤
///   `ttbr0!=0` 含线程）仍是第二路径，st1 留诊断（裁决二降格）。
/// - **计算线程 = spread 正典类**（place 终裁：thread_start 同进程计算
///   线程 spread=true，并行收益面；服务根留家核）。root 同模板自旋仅保
///   进程 Alive+started（thread_start 前置门），退场与线程同代——归因面
///   用 thread_key 天然排除 root（Process 键），零污染。
/// - **svc 链全真**（非直调 handler）：0x31 process_create → 0x11
///   vmo_create → 0x12 vmo_commit → 0x15 vmo_write（灌 [`user::user_lb_code`]
///   模板）→ 0x16 vmar_map（**0x31 后 0x32/0x35 前**，F-A16）→ 0x34
///   thread_create ×5 → 0x32 process_start（root）→ 0x35 thread_start ×4。
///   五任务共享单页映像：entry 0x1000（模板 ~9 指令 + svc 0x36 thread_exit
///   出口——**单进程生死线**：0x33 会连杀同进程兄弟线程，0x36 仅退自身 +
///   join 唤醒，谁先转完谁先汇报；自旋不压栈）+ 栈带 256B/线程自页顶向下
///   排（root 0x2000、t0-t3 递减 0x100——sp 16 对齐）。
/// - **收割帽分层**（义务⑤帽序③）：guest 侧 wait_many deadline 90s（子
///   8s 名义 × TCG 半速 ≈16s 墙钟；K=2 路由常态两核分流 ≤3×16s，全落
///   单核最坏 5×16s=80s + 余量）与 harness DEADLINE_LB=240 禁打平竞速；
///   -ETIMEDOUT = assert FAIL loud（子未按帽完成 = 调度面破损,不是
///   「可容噪声」）。
/// - **Δ 采样**（F-A8）：首线程入队前 / 末线程收割后两快照,klock 包络内
///   直调 [`berry_sched::lb_stats`]（kernel→sched 直调,锁窗序保证原子性）。
/// - 表/对象生命周期：lb_master pinned==0 退出直通**不 drain** 私有表
///   （A45 有意泄漏,par/rt/soak 同款）;子 VMO 的 -EBUSY 收割面
///   （A18/A74）不触发（无 close 动作——悬空 PTE 防线的 close 拒绝面
///   与本链无交）。
extern "C" fn task_lb_master(arg: usize) -> ! {
    use crate::abi_gen::{
        NR_CLOCK_READ, NR_OBJECT_WAIT_MANY, NR_PROCESS_CREATE, NR_PROCESS_START,
        NR_THREAD_CREATE, NR_THREAD_START, NR_VMAR_MAP, NR_VMO_COMMIT, NR_VMO_CREATE,
        NR_VMO_WRITE,
    };
    use crate::abi_gen::SIGNAL_TERMINATED;
    use crate::handle::ObjType;
    // SAFETY: ht_lb 由 boot Box::leak 常驻（活过本任务全程）。
    let ht: &'static mut crate::handle::HandleTable =
        unsafe { &mut *(arg as *mut crate::handle::HandleTable) };
    // Δ 基线（F-A8 采样点一：首线程入队前）。
    berry_hal_aarch64::kern_lock::klock_enter();
    let (b0, b1, bs) = berry_sched::lb_stats();
    berry_hal_aarch64::kern_lock::klock_exit();
    let code = crate::user::user_lb_code();
    // 收割对象 = 4 计算线程句柄；root 句柄单槽（不收割——保 Alive 用）。
    let mut hths = [0u32; LB_CHILD_N];
    // 出参/入出参槽（本任务内核栈——svc el1 分支直拷先例 waker2/ai_demo）。
    let (mut hp, mut hv, mut hvmo, mut hth_root) = (0u32, 0u32, 0u32, 0u32);
    let mut vaddr: u64;
    // svc4/svc6 是安全封装（同步异常、槽为私有栈变量——el1 直拷先例
    // waker2/ai_demo 同款零外层 unsafe）。
    // 0x31：process_create(job=0, options=0, *p, *v)——单进程（4 计算线程
    // 共享同一地址空间/代码页；svc4 裁 4 参,svc6 裁 6 参）。
    assert_eq!(
        svc6(
            NR_PROCESS_CREATE as u64, 0, 0,
            core::ptr::addr_of_mut!(hp) as u64,
            core::ptr::addr_of_mut!(hv) as u64, 0, 0,
        ),
        0,
        "lb: process_create"
    );
    // 0x11：vmo_create(size=1 页, options=0, *out)。
    assert_eq!(
        svc4(
            NR_VMO_CREATE as u64, 0x1000, 0,
            core::ptr::addr_of_mut!(hvmo) as u64,
        ),
        0,
        "lb: vmo_create"
    );
    // 0x12：vmo_commit(h, off=0, len=1 页)。
    assert_eq!(svc4(NR_VMO_COMMIT as u64, hvmo as u64, 0, 0x1000), 0,
               "lb: vmo_commit");
    // 0x15：vmo_write(h, buf=模板 rodata, off=0, len)——4 参 svc6 裁。
    assert_eq!(
        svc6(
            NR_VMO_WRITE as u64, hvmo as u64, code.as_ptr() as u64, 0,
            code.len() as u64, 0, 0,
        ),
        0,
        "lb: vmo_write"
    );
    // 0x16：vmar_map(vmar, vmo, off=0, *vaddr, len, rights=R|W)——in-out 槽
    // 先进请求 VA（F-A16：0x31 后 0x32/0x35 前补映射）。
    vaddr = crate::user::USER_CODE_VA as u64;
    assert_eq!(
        svc6(
            NR_VMAR_MAP as u64, hv as u64, hvmo as u64, 0,
            core::ptr::addr_of_mut!(vaddr) as u64, 0x1000,
            (crate::abi_gen::RIGHT_READ | crate::abi_gen::RIGHT_WRITE) as u64,
        ),
        0,
        "lb: vmar_map"
    );
    assert_eq!(vaddr as usize, crate::user::USER_CODE_VA,
               "lb: vmar_map 实际 VA 偏离请求（分配器改道?）");
    // root：0x34 thread_create + 0x32 process_start（spread=false 恒家核——
    // 同模板自旋保进程 Alive+started（thread_start 前置门），归因面
    // thread_key 排除）。sp=0x2000（页顶栈带第一格）。
    assert_eq!(
        svc4(
            NR_THREAD_CREATE as u64, hp as u64, 0,
            core::ptr::addr_of_mut!(hth_root) as u64,
        ),
        0,
        "lb: thread_create[root]"
    );
    assert_eq!(
        svc6(
            NR_PROCESS_START as u64, hp as u64, hth_root as u64,
            crate::user::USER_CODE_VA as u64, 0x2000, 0, 0,
        ),
        0,
        "lb: process_start[root]"
    );
    // 4 计算线程：0x34 thread_create + 0x35 thread_start(th, entry, sp,
    // arg=0——svc6 显式清尾参)。thread_start → spread=true → place K=2
    // 深度失衡路由（4-burst 算术保底第 3 线程直路 rq1 + SGI 醒 park 环）。
    // 栈带自页顶向下 256B/线程（自旋不压栈——纯隔离垫；sp 16 对齐）。
    for i in 0..LB_CHILD_N {
        assert_eq!(
            svc4(
                NR_THREAD_CREATE as u64, hp as u64, 0,
                core::ptr::addr_of_mut!(hths[i]) as u64,
            ),
            0,
            "lb: thread_create[{}]",
            i
        );
        // 归因登记（create 后、start 前——thread_key 值 create 即定；
        // 内核直调 get_raw 须自持 klock：svc 返回时 SVC 主体已放锁）。
        berry_hal_aarch64::kern_lock::klock_enter();
        // SAFETY: hths[i] 刚由 thread_create 插入本表;类型/权利由 get_raw 门控。
        let (thr, _) = ht
            .get_raw(hths[i], ObjType::Thread, 0)
            .expect("lb: 取 Thread ObjRef");
        LB_PROCS[i].store(thr.as_ptr() as usize, core::sync::atomic::Ordering::Relaxed);
        berry_hal_aarch64::kern_lock::klock_exit();
        let sp = 0x2000 - (i as u64 + 1) * 0x100; // 0x1F00/0x1E00/0x1D00/0x1C00
        assert_eq!(
            svc6(
                NR_THREAD_START as u64, hths[i] as u64,
                crate::user::USER_CODE_VA as u64, sp, 0, 0, 0,
            ),
            0,
            "lb: thread_start[{}]",
            i
        );
    }
    // 收割（wait_many 派生位——A45 表形状钉板件）：WAIT_ANY 哨兵睡,计算线程
    // 退出置 th.exited → signal_state 出 TERMINATED + wake_matching 命中唤醒
    // → sys 面重验循环定夺命中项;逐次 swap-remove 至 4 尽（Thread 句柄收割
    // = T-1 join 面,root 不在集——spread=false 自旋永真 Alive）。deadline =
    // now + 90s（guest 收割帽,义务⑤帽序③）。SIGNAL_TERMINATED 是 Exited
    // 永真位——已收割项若再入集会即时假命中,swap-remove 保证活跃集单调收缩。
    let mut now: u64 = 0;
    let pnow = core::ptr::addr_of_mut!(now) as u64;
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "lb: clock_read");
    let deadline = now + 90 * 1_000_000_000;
    let mut hs = hths;
    let sigs = [SIGNAL_TERMINATED as u64; LB_CHILD_N];
    let (mut idx, mut obs) = (0u64, 0u64);
    let mut n = LB_CHILD_N;
    while n > 0 {
        // 两数组/两出参槽均私有栈（svc 安全封装同上）。
        assert_eq!(
            svc6(
                NR_OBJECT_WAIT_MANY as u64,
                hs.as_ptr() as u64,
                sigs.as_ptr() as u64,
                n as u64,
                deadline,
                core::ptr::addr_of_mut!(idx) as u64,
                core::ptr::addr_of_mut!(obs) as u64,
            ),
            0,
            "lb: wait_many 收割（剩 {} 个子未按 90s 帽退出——调度面破损）",
            n
        );
        assert!((idx as usize) < n, "lb: wait_many idx 越界（{}≥{}）", idx, n);
        hs[idx as usize] = hs[n - 1];
        n -= 1;
    }
    // Δ 终值（F-A8 采样点二：末子收割后）+ 汇总行（G43 拼行）。
    berry_hal_aarch64::kern_lock::klock_enter();
    let (e0, e1, es) = berry_sched::lb_stats();
    berry_hal_aarch64::kern_lock::klock_exit();
    let (d0, d1, ds) = (e0 - b0, e1 - b1, es - bs);
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"LB ok c0=");
        berry_hal_aarch64::uart_put_dec(d0);
        berry_hal_aarch64::uart_puts(b" c1=");
        berry_hal_aarch64::uart_put_dec(d1);
        berry_hal_aarch64::uart_puts(b" st1=");
        berry_hal_aarch64::uart_put_dec(ds);
        berry_hal_aarch64::uart_puts(b" n=");
        berry_hal_aarch64::uart_put_dec(LB_CHILD_N as u64);
        berry_hal_aarch64::uart_puts(b"\n");
    });
    berry_sched::exit_current()
}

/// S-4 4-3 相 4（smoke-smp `asidroll` 门）：ASID 回卷 quiesce 验收。
/// 四步：①[`aspace::asid_force_near_roll`] 快进；②[`aspace::alloc_asid`]
/// 真回卷（锁内代数 bump + 锁外武装——本核全量失效 + 广播置闩，不等待）；
/// ③显式 [`tlbshoot::roll_quiesce_check`] 驱动（内核任务无 dispatch 尾
/// 等待点——IRQ 开零锁语境内联等待，§3.2 同守）；④断言 + `ASIDR ok n=`
/// 打标。n = 全核回执增量（双核 Others 自核排除——总增量 ≥1 ⟺ 对核已为
/// 本轮回卷做过全量失效；按核取数有抢占-迁移错位面，总数面免疫）。
/// one-shot：打标毕 [`exit_current`]（lb_master 同款）。
extern "C" fn task_asid_roll(_arg: usize) -> ! {
    let ack0 = berry_hal_aarch64::tlbshoot::ack_count(0)
        + berry_hal_aarch64::tlbshoot::ack_count(1);
    crate::aspace::asid_force_near_roll();
    let _last = crate::aspace::alloc_asid(); // 快进后必真回卷（g ≥ 1 断言兜底）
    let g = berry_hal_aarch64::tlbshoot::roll_gen();
    assert!(g >= 1, "asidroll: 回卷代数零前进（force_near_roll 未生效？）");
    berry_hal_aarch64::tlbshoot::roll_quiesce_check();
    let ack1 = berry_hal_aarch64::tlbshoot::ack_count(0)
        + berry_hal_aarch64::tlbshoot::ack_count(1);
    let n = ack1 - ack0;
    assert!(n >= 1, "asidroll: 全核回执零前进（quiesce 回执链断——查 SGI #2 面）");
    let q = berry_hal_aarch64::tlbshoot::quiesced_gen();
    assert!(q >= g, "asidroll: QUIESCED({q}) < ROLL({g})（驱动退出但未追平）");
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"ASIDR ok n=");
        berry_hal_aarch64::uart_put_dec(n as u64);
        berry_hal_aarch64::uart_puts(b"\n");
    });
    berry_sched::exit_current()
}

/// tlbs 栅障/收割共用的有界轮询（par_probe 形）：哑 Event `block_current`
/// 超时醒（50ms 步）——master prio 8 醒即抢占采样、睡即让出 CPU0（root/
/// 常驻才有时间片跑 warm/风暴；裸自旋会饿死同核 root）。返回末次观测的
/// `canary_hits(0, slot) + canary_hits(1, slot)`（预算尽仍未达 = 调用方
/// assert 兜底 fail-loud）。deadline = CNTVCT 周期域（block_current 契约
/// ——勿喂 ns，§A118 面）。
fn tlbs_poll(arg: usize, want: usize, slot: usize, budget_vct: u64) -> usize {
    use berry_sched::WakeReason;
    let step = berry_hal_aarch64::timer::cntfrq() / 20; // 50ms 步
    let t0 = berry_hal_aarch64::timer::cntvct();
    loop {
        berry_hal_aarch64::kern_lock::klock_enter();
        // SAFETY: 调用方契约 = IRQ 屏蔽窗（par_probe 同款）。
        let deadline = berry_hal_aarch64::timer::cntvct() + step;
        let r = berry_sched::block_current(arg, TLBS_EV_BIT, deadline);
        assert!(r == WakeReason::Timeout,
                "tlbs: 醒因非 Timeout（哑 Event 永不置位——跨表 wake 面破损）");
        let total = berry_hal_aarch64::trap::canary_hits(0, slot)
            + berry_hal_aarch64::trap::canary_hits(1, slot);
        let expired = berry_hal_aarch64::timer::cntvct().wrapping_sub(t0) > budget_vct;
        let done = total >= want;
        berry_hal_aarch64::kern_lock::klock_exit();
        if done || expired {
            return total;
        }
    }
}

/// tlbs 单轮 canary 舰队装配（S-4 4-4 双轮共用；svc 落脚 = 本任务私有表，
/// caller_table 经 current_tag 取）：进程 + 代码页 + **双 1 页 data VMO**
/// （page0 = canary 写点 @`TLBS_DATA_VA` / page1 = flag 时钟页 @+0x1000
/// ——分体而非合体：合体下 page1 映射钉住 VMO，round B 的 handle_close
/// 永不达 Drop）+ root + `TLBS_WRITER_N` 写者。返回 (Vmar, page0 VMO,
/// page1 VMO)——还帧机制由调用方选（round A decommit / round B close→
/// Drop）。tag 仅入 assert 标签（排障按轮定位）。
fn tlbs_spawn_fleet(tag: &str) -> (u32, u32, u32) {
    use crate::abi_gen::{
        NR_PROCESS_CREATE, NR_PROCESS_START, NR_THREAD_CREATE, NR_THREAD_START,
        NR_VMAR_MAP, NR_VMO_COMMIT, NR_VMO_CREATE, NR_VMO_WRITE, RIGHT_READ, RIGHT_WRITE,
    };
    use crate::user::{user_tlbc_code, TLBS_DATA_VA, USER_CODE_VA};
    let code = user_tlbc_code();
    let (mut hp, mut hv, mut hc) = (0u32, 0u32, 0u32);
    let (mut hd0, mut hd1, mut hth_root) = (0u32, 0u32, 0u32);
    let mut hths = [0u32; TLBS_WRITER_N];
    let mut vaddr: u64;
    // 进程 + 代码页（lb_master 同链 0x31/0x11/0x12/0x15/0x16——svc4/svc6
    // 安全封装，槽为私有栈变量，el1 直拷先例）。
    assert_eq!(
        svc6(
            NR_PROCESS_CREATE as u64, 0, 0,
            core::ptr::addr_of_mut!(hp) as u64,
            core::ptr::addr_of_mut!(hv) as u64, 0, 0,
        ),
        0,
        "tlbs{}: process_create",
        tag
    );
    assert_eq!(
        svc4(
            NR_VMO_CREATE as u64, 0x1000, 0,
            core::ptr::addr_of_mut!(hc) as u64,
        ),
        0,
        "tlbs{}: vmo_create[code]",
        tag
    );
    assert_eq!(svc4(NR_VMO_COMMIT as u64, hc as u64, 0, 0x1000), 0,
               "tlbs{}: vmo_commit[code]", tag);
    assert_eq!(
        svc6(
            NR_VMO_WRITE as u64, hc as u64, code.as_ptr() as u64, 0,
            code.len() as u64, 0, 0,
        ),
        0,
        "tlbs{}: vmo_write[code]",
        tag
    );
    vaddr = USER_CODE_VA as u64;
    assert_eq!(
        svc6(
            NR_VMAR_MAP as u64, hv as u64, hc as u64, 0,
            core::ptr::addr_of_mut!(vaddr) as u64, 0x1000,
            (RIGHT_READ | RIGHT_WRITE) as u64,
        ),
        0,
        "tlbs{}: vmar_map[code]",
        tag
    );
    assert_eq!(vaddr as usize, USER_CODE_VA, "tlbs{}: code VA 偏离请求", tag);
    // 双 1 页 data VMO。page1（flag 页）：+0x0 clock 出参 / +0x10 flag；
    // commit 清零 = flag 天然 0，参与者风暴轮询至 master 放行。
    assert_eq!(
        svc4(
            NR_VMO_CREATE as u64, 0x1000, 0,
            core::ptr::addr_of_mut!(hd0) as u64,
        ),
        0,
        "tlbs{}: vmo_create[page0]",
        tag
    );
    assert_eq!(svc4(NR_VMO_COMMIT as u64, hd0 as u64, 0, 0x1000), 0,
               "tlbs{}: vmo_commit[page0]", tag);
    vaddr = TLBS_DATA_VA as u64;
    assert_eq!(
        svc6(
            NR_VMAR_MAP as u64, hv as u64, hd0 as u64, 0,
            core::ptr::addr_of_mut!(vaddr) as u64, 0x1000,
            (RIGHT_READ | RIGHT_WRITE) as u64,
        ),
        0,
        "tlbs{}: vmar_map[page0]",
        tag
    );
    assert_eq!(vaddr as usize, TLBS_DATA_VA, "tlbs{}: page0 VA 偏离请求", tag);
    assert_eq!(
        svc4(
            NR_VMO_CREATE as u64, 0x1000, 0,
            core::ptr::addr_of_mut!(hd1) as u64,
        ),
        0,
        "tlbs{}: vmo_create[page1]",
        tag
    );
    assert_eq!(svc4(NR_VMO_COMMIT as u64, hd1 as u64, 0, 0x1000), 0,
               "tlbs{}: vmo_commit[page1]", tag);
    vaddr = (TLBS_DATA_VA + 0x1000) as u64;
    assert_eq!(
        svc6(
            NR_VMAR_MAP as u64, hv as u64, hd1 as u64, 0,
            core::ptr::addr_of_mut!(vaddr) as u64, 0x1000,
            (RIGHT_READ | RIGHT_WRITE) as u64,
        ),
        0,
        "tlbs{}: vmar_map[page1]",
        tag
    );
    assert_eq!(vaddr as usize, TLBS_DATA_VA + 0x1000,
               "tlbs{}: page1 VA 偏离请求", tag);
    // root（spread=false 恒家核）+ 写者（thread_start spread=true → K=2
    // 路由 CPU1——lb 4-burst 同构）。sp = page1 顶内错落（asm 不压栈——
    // 纯合法垫；page1 恒映射，永不随 page0 unmap 撤）。
    assert_eq!(
        svc4(
            NR_THREAD_CREATE as u64, hp as u64, 0,
            core::ptr::addr_of_mut!(hth_root) as u64,
        ),
        0,
        "tlbs{}: thread_create[root]",
        tag
    );
    let sp_root = (TLBS_DATA_VA + 0x1FF0) as u64; // page1 顶（16 对齐）
    assert_eq!(
        svc6(
            NR_PROCESS_START as u64, hp as u64, hth_root as u64,
            USER_CODE_VA as u64, sp_root, 0, 0,
        ),
        0,
        "tlbs{}: process_start[root]",
        tag
    );
    TLBS_ROOT_H.store(hth_root, core::sync::atomic::Ordering::Relaxed);
    for i in 0..TLBS_WRITER_N {
        assert_eq!(
            svc4(
                NR_THREAD_CREATE as u64, hp as u64, 0,
                core::ptr::addr_of_mut!(hths[i]) as u64,
            ),
            0,
            "tlbs{}: thread_create[{}]",
            tag,
            i
        );
        let sp = (TLBS_DATA_VA + 0x1F00 - i * 0x40) as u64; // 0x1F00..0x1E40
        assert_eq!(
            svc6(
                NR_THREAD_START as u64, hths[i] as u64,
                USER_CODE_VA as u64, sp, 0, 0, 0,
            ),
            0,
            "tlbs{}: thread_start[{}]",
            tag,
            i
        );
    }
    (hv, hd0, hd1)
}

/// tlbs 还帧机制（S-4 4-4 冻结规格「decommit 与 Drop 双构造」）。
enum TlbsFree {
    /// round A：`vmo_decommit`——VMO 对象活、页帧还 buddy。
    Decommit,
    /// round B：`handle_close`——引用归零走 VMO Drop 还帧（与生产进程
    /// teardown 同机制「join 同形」）。
    Drop,
}

/// tlbs 单轮 canary 五步（增量口径——槽 hits 跨轮累计不打清零，断言全用
/// Δ）：①舰队 spawn；②warm 栅障（Δslot1 == P + Δwarm[1]≥1 跨核路由
/// 结构断言）；③unmap page0（整方法 PT 锁 + 本核 tlbi_aside1(进程 ASID)
/// + SGI #2 广播即发，SVC dispatch 尾零锁等回执——svc 返回即全核失效毕）
/// + 还帧（按 `how`）+ 复用（新 vmo_create+commit 吸收还帧，**不回 map
/// 同 VA**——回 map 则 canary 写命中新映射不 fault）；④放行 flag（page1
/// VMO off 0x10，内核侧写帧——EL0 同帧可见）；⑤收割（Δslot0 恰 == P +
/// Δhits[1][0]≥1 = 对核 stale 已被广播打掉；计数不足 = 有写入成功 =
/// stale 残留 = shootdown 断链面）。reuse 句柄泄漏持有（表随任务常驻——
/// lb_master 同合同）。
fn tlbs_round(arg: usize, tag: &str, how: TlbsFree, budget: u64) {
    use crate::abi_gen::{
        NR_HANDLE_CLOSE, NR_VMAR_UNMAP, NR_VMO_COMMIT, NR_VMO_CREATE, NR_VMO_DECOMMIT,
        NR_VMO_WRITE,
    };
    use crate::user::TLBS_DATA_VA;
    let hits = |slot: usize| {
        berry_hal_aarch64::trap::canary_hits(0, slot)
            + berry_hal_aarch64::trap::canary_hits(1, slot)
    };
    // ① 先取 warm 基线再 spawn 舰队（对抗审查 R0）：spawn 尾 thread_start
    // 经 place→enqueue_with_kick 在 **syscall 内**即发 SGI_RESCHEDULE——CPU1
    // 写者即刻可跑且其 probe 命中路径零锁（EL0 canary 早退臂），可先于本核
    // 落地；基线取在 spawn 后 = 早到 probe 被吸进基线 → Δslot1 恒差 1 →
    // warm 栅栏烧满 budget 假红（CI 争用/宿主抢占可翻面）。spawn 前取 = 本
    // 轮参与者尚不存在（round B 时上轮舰队已全退出——Δslot0==P 收割门保证
    // 命中先于 thread_exit）、槽跨轮常驻累计，Δ 口径不变。
    let w_base = hits(1);
    let w1_base = berry_hal_aarch64::trap::canary_hits(1, 1);
    let (hv, hd0, hd1) = tlbs_spawn_fleet(tag);
    // ② warm 栅障：Δslot1 == P（每参与者必经 warm str×4 → probe fault 才
    // 入风暴环，Δ到位 ⟺ 本轮 P 份旧映射 TLB 驻留毕）。
    tlbs_poll(arg, w_base + TLBS_PARTICIPANTS, 1, budget);
    let wd = hits(1) - w_base;
    assert!(
        wd >= TLBS_PARTICIPANTS,
        "tlbs{}: warm 栅障超时（Δslot1 {}/{}——参与者未起或 slot1 未接）",
        tag,
        wd,
        TLBS_PARTICIPANTS
    );
    assert!(
        berry_hal_aarch64::trap::canary_hits(1, 1) > w1_base,
        "tlbs{}: 本轮写者全落 CPU0（Δwarm[1]=0）——K=2 路由结构性意外，查 place/park 面",
        tag
    );
    {
        let (wc0, wc1) = (
            berry_hal_aarch64::trap::canary_hits(0, 1),
            berry_hal_aarch64::trap::canary_hits(1, 1),
        );
        berry_hal_aarch64::kern_lock::with_print_lock(|| {
            berry_hal_aarch64::uart_puts(b"tlbs");
            berry_hal_aarch64::uart_puts(tag.as_bytes());
            berry_hal_aarch64::uart_puts(b" warm c0=");
            berry_hal_aarch64::uart_put_dec(wc0 as u64);
            berry_hal_aarch64::uart_puts(b" c1=");
            berry_hal_aarch64::uart_put_dec(wc1 as u64);
            berry_hal_aarch64::uart_puts(b"\n");
        });
    }
    // ③ unmap + 还帧（双构造分岔点）+ 复用。
    let h_base = hits(0);
    let (c0_base, c1_base) = (
        berry_hal_aarch64::trap::canary_hits(0, 0),
        berry_hal_aarch64::trap::canary_hits(1, 0),
    );
    assert_eq!(
        svc4(NR_VMAR_UNMAP as u64, hv as u64, TLBS_DATA_VA as u64, 0x1000),
        0,
        "tlbs{}: vmar_unmap[page0]",
        tag
    );
    match how {
        TlbsFree::Decommit => {
            assert_eq!(svc4(NR_VMO_DECOMMIT as u64, hd0 as u64, 0, 0x1000), 0,
                       "tlbs{}: vmo_decommit[page0]", tag);
            // 维度差分（审查 R3）：Decommit 后 VMO 对象仍活——同 VMO 再
            // commit 须成功（两臂互换/close 形态变异时此断言红）。
            assert_eq!(svc4(NR_VMO_COMMIT as u64, hd0 as u64, 0, 0x1000), 0,
                       "tlbs{}: 维度差分[Decommit: VMO 应活——commit-again]", tag);
        }
        TlbsFree::Drop => {
            // 真实不变量（审查 R2 改述）：此刻舰队 5 线程**仍在风暴环内存活**
            // （flag 步④才置位），close 归零的依据不是「已退出」而是——
            // 参与者不持任何 VMO ObjRef（子进程表出生为空、只调 clock_read/
            // thread_exit，纯 VA 经 TLB 访存不取引用）+ unmap 已 untrack_map
            // 撤 mapcnt → refs==1（仅本任务表槽）过 -EBUSY 拒面 → Drop 还帧。
            assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hd0 as u64, 0, 0), 0,
                       "tlbs{}: handle_close[page0]", tag);
            // 维度差分（审查 R3）：close 后 handle 应已死——写之须非 0
            // （两臂互换/decommit 形态变异时此断言红）。
            assert!(svc6(NR_VMO_WRITE as u64, hd0 as u64, &1u8 as *const u8 as u64, 0, 1, 0, 0) != 0,
                    "tlbs{}: 维度差分[Drop: handle 应死——write 须非 0]", tag);
        }
    }
    // 注：decommit/Drop 的「帧真还 buddy」账回归不在本门观测面（TLB 面归
    // canary 断言链、帧账归 ktest VMO 面——互不代偿，审查 R3 边界记录）。
    let mut hr = 0u32;
    assert_eq!(
        svc4(
            NR_VMO_CREATE as u64, 0x1000, 0,
            core::ptr::addr_of_mut!(hr) as u64,
        ),
        0,
        "tlbs{}: vmo_create[reuse]",
        tag
    );
    assert_eq!(svc4(NR_VMO_COMMIT as u64, hr as u64, 0, 0x1000), 0,
               "tlbs{}: vmo_commit[reuse]", tag);
    // ④ 放行 flag：参与者出风暴环 → 写旧 VA → 期望 fault。
    let one: u8 = 1;
    assert_eq!(
        svc6(
            NR_VMO_WRITE as u64, hd1 as u64,
            core::ptr::addr_of!(one) as u64, 0x10, 1, 0, 0,
        ),
        0,
        "tlbs{}: vmo_write[flag]",
        tag
    );
    // ⑤ 收割：Δslot0 恰 == P（模板写点恰一次 + 静默恢复后 thread_exit
    // 不复触）。
    tlbs_poll(arg, h_base + TLBS_PARTICIPANTS, 0, budget);
    let (c0_now, c1_now) = (
        berry_hal_aarch64::trap::canary_hits(0, 0),
        berry_hal_aarch64::trap::canary_hits(1, 0),
    );
    let hd = (c0_now + c1_now) - h_base;
    if hd < TLBS_PARTICIPANTS {
        use crate::abi_gen::SIGNAL_TERMINATED;
        use crate::syscall::NR_OBJECT_WAIT_ONE;
        let rh = TLBS_ROOT_H.load(core::sync::atomic::Ordering::Relaxed);
        let mut obs = 0u64;
        let rr = svc6(
            NR_OBJECT_WAIT_ONE as u64, rh as u64, SIGNAL_TERMINATED, 0,
            core::ptr::addr_of_mut!(obs) as u64, 0, 0,
        );
        berry_hal_aarch64::kern_lock::with_print_lock(|| {
            berry_hal_aarch64::uart_puts(b"tlbs");
            berry_hal_aarch64::uart_puts(tag.as_bytes());
            berry_hal_aarch64::uart_puts(b" rootpoll ret=");
            berry_hal_aarch64::uart_put_dec(rr as u64);
            berry_hal_aarch64::uart_puts(b" obs=");
            berry_hal_aarch64::uart_put_dec(obs);
            berry_hal_aarch64::uart_puts(b"\n");
        });
    }
    assert!(
        hd >= TLBS_PARTICIPANTS,
        "tlbs{}: canary 收割超时（Δslot0 {}/{}——Δc0={} Δc1={}；计数不足 = 有写入成功 = stale 残留 = shootdown 断链）",
        tag,
        hd,
        TLBS_PARTICIPANTS,
        c0_now - c0_base,
        c1_now - c1_base
    );
    assert!(
        c1_now > c1_base,
        "tlbs{}: 对核 canary 零命中（Δhits[1][0]=0——CPU1 写入成功 = SGI #2 广播失效面）",
        tag
    );
    assert_eq!(hd, TLBS_PARTICIPANTS, "tlbs{}: canary 命中超额（重触面?）", tag);
}

/// S-4 4-4 相 5（smoke-smp `tlbs` 门）：TLB shootdown canary 压测验收。
/// canary 五步 × 双轮（SMP-S4 草案 §2 4-4 冻结规格「decommit 与 Drop 双
/// 构造」；§5.1 变异标定兼任答案）：round A 还帧 = vmo_decommit / round B
/// 还帧 = handle_close→Drop（与生产 teardown 同机制）。每轮 5 参与者
/// （root spread=false 家核 + 4 写者 spread=true → K=2 路由 CPU1），warm
/// str 旧 VA 驻留 TLB → probe fault 汇报（slot1）→ SVC 风暴轮询（每迭代
/// 一次 clock_read = 对核满载荷 SVC 中发 shootdown——P0 死锁环钉进可断言
/// 面）→ unmap+广播+还帧+复用（不回 map 同 VA）→ 放行 flag → 写旧 VA
/// 期望 fault（slot0 增量恰 == 参与者数 + Δhits[1][0]≥1）。变异形态（去
/// tlbi/去广播）→ 对核写入成功 → 计数不足 → assert 炸 = canary 在 TCG
/// 上真 红。one-shot：双轮毕打标 `TLBS ok n=`（n = 两轮 slot0 命中总
/// 数 = 2×参与者数——三面钉同值：本注释、`TLBS ok` 行、smoke 相 5 断言）
/// 后 exit_current。
extern "C" fn task_tlbs(arg: usize) -> ! {
    use crate::user::TLBS_DATA_VA;
    // canary 槽双核双槽武装（幂等——双轮同 VA；远程 arm：master CPU0 写、
    // 故障核消费。probe = base+0x2_0000，与模板 movz 单源）。
    for cpu in 0..berry_hal_aarch64::trap::CANARY_CPUS {
        berry_hal_aarch64::trap::canary_arm(cpu, 0, TLBS_DATA_VA); // slot0：写点
        berry_hal_aarch64::trap::canary_arm(cpu, 1, TLBS_DATA_VA + 0x2_0000); // slot1：probe
    }
    let budget = 8 * berry_hal_aarch64::timer::cntfrq();
    tlbs_round(arg, "A", TlbsFree::Decommit, budget);
    tlbs_round(arg, "B", TlbsFree::Drop, budget);
    // 双轮总账：恰 2×P（每参与者写点恰一次、跨轮槽累计）。
    let n = berry_hal_aarch64::trap::canary_hits(0, 0)
        + berry_hal_aarch64::trap::canary_hits(1, 0);
    assert_eq!(n, 2 * TLBS_PARTICIPANTS, "tlbs: 双轮总命中 {n} != {}（重触/漏触面?）",
               2 * TLBS_PARTICIPANTS);
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"TLBS ok n=");
        berry_hal_aarch64::uart_put_dec(n as u64);
        berry_hal_aarch64::uart_puts(b"\n");
    });
    berry_sched::exit_current()
}

// ---- S-5b 相 6/7：storm fleet（ALLOC churn / HT 风暴双核 EL0 压测）----

/// alloc 哑 Event 的用户位（S-5b 相 6，占用登记见 [`USER_SIGNAL_BITS`]——
/// TLBS_EV_BIT 的续位）。永不置位：唯一醒因 Timeout（storm_poll 节拍——
/// 计数面是 data VMO 窗口轮询，非事件面）。
const ALLOC_EV_BIT: u64 = 1 << 44;
/// ht 哑 Event 的用户位（S-5b 相 7，同上——ALLOC_EV_BIT 的续位）。
const HT_EV_BIT: u64 = 1 << 45;

/// pp 哑 Event 的用户位（S-5c 相 8，同上——HT_EV_BIT 的续位）。
const PP_EV_BIT: u64 = 1 << 46;

/// soakload 哑 Event 的用户位（S-5d，占用登记见 [`USER_SIGNAL_BITS`]——
/// PP_EV_BIT 的续位）。永不置位：唯一醒因 Timeout（master 永睡节拍）。
const SOAKLOAD_EV_BIT: u64 = 1 << 47;
/// lat 握手 Event 的用户位（G3-4 bench-lat IPC 路，占用登记见
/// [`USER_SIGNAL_BITS`]——SOAKLOAD_EV_BIT 的续位）。**会被置位**（waker
/// 经 wake_matching 按位唤醒 sleeper）——与哑 Event 族相反的真握手位；
/// 位本身不锁存（wake_matching 只扫睡眠队不写 Event 态），漏醒由 waker
/// 侧 n==1 断言 + sleeper 侧醒因/醒时断言双 L0 执法兜底。
const LAT_EV_BIT: u64 = 1 << 48;

/// bench-lat SGI 路位签名（G3-4b，占用登记见 [`USER_SIGNAL_BITS`]——
/// LAT_EV_BIT 的续位）。与 IPC 路不同源的两用位：EL0 sleeper `wait_one`
/// 的请求位（位签名 = 内核钩子双过滤之一）+ 内核 waker `Event::signal`
/// 真置位（跨核睡的醒来重验靠它命中——`wake_matching` 只扫睡眠队不写
/// Event 态，位不锁存即重验失败再睡 = 漏醒）。sleeper 逐轮 `object_signal`
/// clear 消费（§A61 清位纪律）——下轮 wait 前 Event 态恒零，恒真睡不 poll。
pub(crate) const LAT_SGI_BIT: u64 = 1 << 49;

/// storm 舰队装配（S-5b 相 6/7 + S-5c 相 8 共用；svc 落脚 = 本任务私有表，
/// caller_table 经 current_tag 取）：进程 + 代码页 + 单 data VMO
/// （(2+`writer_n`) 页 = flag 页@0 + root 窗口页@1 + 写者窗口页@2..——与
/// tlbs 分体双 VMO 不同形：flag 与窗口同 VMO 且全程恒映射，churn 对象
/// （VMO/Event）每轮独立 create/close 对消）+ root（process_start
/// spread=false 恒家核，x0=bootstrap=0）+ `writer_n` 写者（thread_start
/// spread=true → K=2 路由 CPU1，x0=`writer_arg`——相 8 PP 双角色模板 arg
/// 分流用）。sp = 各自窗口页顶内错落（模板 SP 自推导窗口基——错落纯防
/// 未来模板压栈，同页同基）。返回 (process, Vmar, data VMO)——两 handle 随
/// master 表常驻（reuse 句柄泄漏持有，lb_master 同合同）。tag 仅入
/// assert 标签。
fn storm_spawn_fleet(tag: &str, code: &'static [u8], writer_n: usize, writer_arg: u64)
    -> (u32, u32, u32)
{
    use crate::abi_gen::{
        NR_PROCESS_CREATE, NR_PROCESS_START, NR_THREAD_CREATE, NR_THREAD_START,
        NR_VMAR_MAP, NR_VMO_COMMIT, NR_VMO_CREATE, NR_VMO_WRITE, RIGHT_READ, RIGHT_WRITE,
    };
    use crate::user::{STORM_DATA_VA, USER_CODE_VA};
    debug_assert!(writer_n > 0 && writer_n <= STORM_WRITER_N, "storm: writer_n 域");
    let (mut hp, mut hv, mut hc) = (0u32, 0u32, 0u32);
    let (mut hd, mut hth_root) = (0u32, 0u32);
    let mut hths = [0u32; STORM_WRITER_N];
    let mut vaddr: u64;
    // 进程 + 代码页（tlbs_spawn_fleet 同链 0x31/0x11/0x12/0x15/0x16——svc4/
    // svc6 安全封装，槽为私有栈变量，el1 直拷先例）。
    assert_eq!(
        svc6(
            NR_PROCESS_CREATE as u64, 0, 0,
            core::ptr::addr_of_mut!(hp) as u64,
            core::ptr::addr_of_mut!(hv) as u64, 0, 0,
        ),
        0,
        "storm{}: process_create",
        tag
    );
    assert_eq!(
        svc4(
            NR_VMO_CREATE as u64, 0x1000, 0,
            core::ptr::addr_of_mut!(hc) as u64,
        ),
        0,
        "storm{}: vmo_create[code]",
        tag
    );
    assert_eq!(svc4(NR_VMO_COMMIT as u64, hc as u64, 0, 0x1000), 0,
               "storm{}: vmo_commit[code]", tag);
    assert_eq!(
        svc6(
            NR_VMO_WRITE as u64, hc as u64, code.as_ptr() as u64, 0,
            code.len() as u64, 0, 0,
        ),
        0,
        "storm{}: vmo_write[code]",
        tag
    );
    vaddr = USER_CODE_VA as u64;
    assert_eq!(
        svc6(
            NR_VMAR_MAP as u64, hv as u64, hc as u64, 0,
            core::ptr::addr_of_mut!(vaddr) as u64, 0x1000,
            (RIGHT_READ | RIGHT_WRITE) as u64,
        ),
        0,
        "storm{}: vmar_map[code]",
        tag
    );
    assert_eq!(vaddr as usize, USER_CODE_VA, "storm{}: code VA 偏离请求", tag);
    // data VMO 一炉（(2+writer_n) 页）：页 0 = go-flag（commit 清零天然
    // 0——参与者自旋至 master 放行；相 8 PP 无 go-flag，页 0 空置保布局）
    // ；页 1 = root 窗口、页 2..=1+writer_n = 写者窗口（SP 自推导）。
    let data_len = ((2 + writer_n) * 0x1000) as u64;
    assert_eq!(
        svc4(
            NR_VMO_CREATE as u64, data_len, 0,
            core::ptr::addr_of_mut!(hd) as u64,
        ),
        0,
        "storm{}: vmo_create[data]",
        tag
    );
    assert_eq!(svc4(NR_VMO_COMMIT as u64, hd as u64, 0, data_len), 0,
               "storm{}: vmo_commit[data]", tag);
    vaddr = STORM_DATA_VA as u64;
    assert_eq!(
        svc6(
            NR_VMAR_MAP as u64, hv as u64, hd as u64, 0,
            core::ptr::addr_of_mut!(vaddr) as u64, data_len,
            (RIGHT_READ | RIGHT_WRITE) as u64,
        ),
        0,
        "storm{}: vmar_map[data]",
        tag
    );
    assert_eq!(vaddr as usize, STORM_DATA_VA, "storm{}: data VA 偏离请求", tag);
    // root（spread=false 恒家核）+ 写者（thread_start spread=true → K=2
    // 路由 CPU1——lb/tlbs 同构）。sp = 各自窗口页顶内错落（页 1..=P，
    // 0xFF0 顶/写者逐个 -0x40——16 对齐门 + 页内不越界）。
    assert_eq!(
        svc4(
            NR_THREAD_CREATE as u64, hp as u64, 0,
            core::ptr::addr_of_mut!(hth_root) as u64,
        ),
        0,
        "storm{}: thread_create[root]",
        tag
    );
    let sp_root = (STORM_DATA_VA + 0x1FF0) as u64; // 页 1 顶（16 对齐）
    assert_eq!(
        svc6(
            NR_PROCESS_START as u64, hp as u64, hth_root as u64,
            USER_CODE_VA as u64, sp_root, 0, 0,
        ),
        0,
        "storm{}: process_start[root]",
        tag
    );
    for i in 0..writer_n {
        assert_eq!(
            svc4(
                NR_THREAD_CREATE as u64, hp as u64, 0,
                core::ptr::addr_of_mut!(hths[i]) as u64,
            ),
            0,
            "storm{}: thread_create[{}]",
            tag,
            i
        );
        let sp = (STORM_DATA_VA + (2 + i) * 0x1000 + 0xFF0 - i * 0x40) as u64;
        assert_eq!(
            svc6(
                NR_THREAD_START as u64, hths[i] as u64,
                USER_CODE_VA as u64, sp, writer_arg, 0, 0,
            ),
            0,
            "storm{}: thread_start[{}]",
            tag,
            i
        );
    }
    (hp, hv, hd)
}

/// storm 窗口 8B 读（master 轮询面）：svc vmo_read 直读 data VMO 窗口
/// 字段（el1 直拷先例——buf 内核栈裸指针）。`idx` = 参与者序（0=root）。
fn storm_win_read(hd: u32, idx: usize, off: usize) -> u64 {
    use crate::abi_gen::NR_VMO_READ;
    let mut v = 0u64;
    let off = ((1 + idx) * 0x1000 + off) as u64;
    assert_eq!(
        svc6(
            NR_VMO_READ as u64, hd as u64,
            core::ptr::addr_of_mut!(v) as u64, off, 8, 0, 0,
        ),
        0,
        "storm: vmo_read[win{}+{off:#x}]",
        idx
    );
    v
}

/// storm master 有界轮询（tlbs_poll 同形）：哑 Event `block_current` 超时
/// 醒（50ms 步——master prio 8 醒即抢占采样、睡即让出 CPU0，裸自旋会饿死
/// 同核 root）；每醒扫全窗（err>0 即红早断 / 全 done 即收）。`parts`/
/// `rounds` = 参与者数与各自轮数（相 6/7 = STORM 常量；相 8 PP = 2×
/// PP_ROUNDS）——DONE 面满值 = rounds×parts、READY 面满值 = parts。返回
/// (err 和, done 和)——预算尽仍未达 = 调用方 assert 兜底 fail-loud。
/// deadline = CNTVCT 周期域（§A118）。**窗读在 klock 窗外**（与 tlbs_poll
/// 的关键差异：观测面是 svc vmo_read 而非原子计数——svc dispatch 自带
/// 锁窗，外层再包 klock = 重入死锁，S-5b 首跑实证；block_current 仍须
/// 锁窗内调用——tlbs_poll 同契约）。
fn storm_poll(arg: usize, ev_bit: u64, hd: u32, budget_vct: u64, done_off: usize,
              parts: usize, rounds: usize) -> (u64, u64)
{
    use berry_sched::WakeReason;
    let step = berry_hal_aarch64::timer::cntfrq() / 20; // 50ms 步
    let t0 = berry_hal_aarch64::timer::cntvct();
    loop {
        berry_hal_aarch64::kern_lock::klock_enter();
        // SAFETY: 调用方契约 = IRQ 屏蔽窗（tlbs_poll 同款）。
        let deadline = berry_hal_aarch64::timer::cntvct() + step;
        let r = berry_sched::block_current(arg, ev_bit, deadline);
        berry_hal_aarch64::kern_lock::klock_exit();
        assert!(r == WakeReason::Timeout,
                "storm: 醒因非 Timeout（哑 Event 永不置位——跨表 wake 面破损）");
        // 锁窗外扫窗（svc 面自带锁窗——见头注）。done_off = 收割槽偏移，
        // 满值随形态：DONE（完成轮数）= rounds×parts；READY（就绪位）= parts。
        let (mut errs, mut dones) = (0u64, 0u64);
        for i in 0..parts {
            errs += storm_win_read(hd, i, STORM_WIN_ERR);
            dones += storm_win_read(hd, i, done_off);
        }
        let full = if done_off == STORM_WIN_DONE {
            (rounds * parts) as u64
        } else {
            parts as u64
        };
        let expired = berry_hal_aarch64::timer::cntvct().wrapping_sub(t0) > budget_vct;
        let done = dones >= full || errs > 0;
        if done || expired {
            return (errs, dones);
        }
    }
}

/// S-5b 相 6（smoke-smp `alloc` 门）：双核 EL0 分配器 churn 压测验收。
/// 载荷 = root + 4 写者 × 64 轮 vmo create/commit/write/close（模板
/// `__user_allocst`——瞬态 VMO 每轮对消）。验收链：①舰队 spawn（线程
/// park 在 go-flag 自旋——纯 EL0 装载环零内核态面）；②**水位基线快照**
/// （go-flag 使基线语义完美：fleet 帧 + data VMO 全已在册，唯瞬态 churn
/// VMO 来去）+ per-CPU sw 基线；③放行 flag（svc vmo_write 写 data VMO
/// off 0——EL0 同帧可见）；④有界轮询至全 done/err/超时；⑤终账四断言：
/// err 全 0 / done 全满（n=320 三面钉）/ `allocated_pages()` == 基线
/// （水位恒等——churn 帧全还，Relaxed 纪律同 churn 终账）/ sw0 与 sw1
/// 均严格增（双核参与证——master+root 家核切、写者 CPU1 切）。one-shot：
/// `ALLOC ok n=` 后 exit_current。
extern "C" fn task_alloc(arg: usize) -> ! {
    use crate::abi_gen::NR_VMO_WRITE;
    use crate::user::user_allocst_code;
    // boot 沉降：gate spawn 于 boot 尾窗（sched::init 后、enable_irq 前），
    // 但 F/rt/S2b 演示与常驻面（echod/svcmgr/par_probe）在 gate on 后仍在
    // 初始化分配——基线快照前须等系统态沉降（水位连续两读相等；60s 帽
    // TCG 覆盖演示面完跑）。S-5b 二跑实证跳过此步则 boot 噪声帧混入对账窗。
    {
        use berry_sched::WakeReason;
        let step = berry_hal_aarch64::timer::cntfrq() / 20;
        let t0 = berry_hal_aarch64::timer::cntvct();
        let cap = 60 * berry_hal_aarch64::timer::cntfrq();
        let (mut last, mut stable) = (usize::MAX, 0);
        while stable < 2 {
            berry_hal_aarch64::kern_lock::klock_enter();
            let deadline = berry_hal_aarch64::timer::cntvct() + step;
            let r = berry_sched::block_current(arg, ALLOC_EV_BIT, deadline);
            berry_hal_aarch64::kern_lock::klock_exit();
            assert!(r == WakeReason::Timeout, "alloc: 沉降等待醒因非 Timeout");
            let p = mm::frames().allocated_pages();
            if p == last {
                stable += 1;
            } else {
                stable = 0;
                last = p;
            }
            if berry_hal_aarch64::timer::cntvct().wrapping_sub(t0) > cap {
                break;
            }
        }
    }
    let budget = 8 * berry_hal_aarch64::timer::cntfrq();
    let (_hp, _hv, hd) = storm_spawn_fleet("A", user_allocst_code(), STORM_WRITER_N, 0);
    // ② 基线：水位（fleet 帧已在——flag 门保证）+ per-CPU sw（写者已
    // thread_start，park 自旋不占内核态面）。
    let pages_base = mm::frames().allocated_pages();
    let (sw0_b, sw1_b, _, _) = berry_hal_aarch64::smp::sw_park_readout();
    // ③ 放行。
    let one: u8 = 1;
    assert_eq!(
        svc6(
            NR_VMO_WRITE as u64, hd as u64,
            core::ptr::addr_of!(one) as u64, 0, 1, 0, 0,
        ),
        0,
        "alloc: vmo_write[flag]"
    );
    // ④ 就绪收割（READY 槽——二次放行形）：全员轮毕 + park 自旋于第二
    // 放行位 = **真静默窗**（churn 帧全在册、进程未收口——drain 洗不掉）。
    let (errs, readies) = storm_poll(arg, ALLOC_EV_BIT, hd, budget, STORM_WIN_READY,
                                      STORM_PARTICIPANTS, STORM_ROUNDS);
    assert_eq!(errs, 0, "alloc: churn err {errs}（svc 非零——handles/帧面）");
    assert_eq!(readies, STORM_PARTICIPANTS as u64,
               "alloc: 就绪 {readies} != {}（早断/超时面）", STORM_PARTICIPANTS);
    // ⑤ 静默窗终账（S-5b 变异标定 ALLOC-1 实证修正：worker thread_exit →
    // 末线程收口 drain 子表 → 泄漏 VMO 全关帧全还——旧位（join 后）终账
    // 被 drain 洗成恒过；真静默窗内 churn 帧全在册，带宽断言才有活性）。
    // 水位断言 = **带宽口径**：SMP EL0 形态全局水位无干净窗（常驻面 ±几帧
    // 波动 = 噪声底；S-5b 三跑实证），对账主信号 = churn 帧全泄（320 轮
    // × 1 帧 = ±320 量级）——带宽 64 = 常驻波动余量，主信号裕度 5×。
    let pages_end = mm::frames().allocated_pages() as i64;
    let delta = pages_end - pages_base as i64;
    assert!(
        delta.abs() <= 64,
        "alloc: 水位越带（base {pages_base} -> end {pages_end}，Δ{delta} ——\
         churn 帧泄漏主信号为 ±320 量级，越 64 带 = 泄漏或多还面）"
    );
    let (sw0_e, sw1_e, _, _) = berry_hal_aarch64::smp::sw_park_readout();
    // 双核参与证 = 合计式：per-CPU sw 零前进在「满载荷独占核」是物理
    // 正常态（root 纯 EL0 churn——svc 进出不计任务切换、rq 空无抢占对象；
    // S-5b 全相跑实证：沉降帽跑满后 CPU0 只剩 root 自旋 → sw0 恒平）。
    // 双核各自跑载荷的结构证 = spread 路由（root 家核 + 写者 K=2→CPU1，
    // lb/tlbs 同构）+ dones 全满；sw 合计涨 = 窗内调度器双核活（master 的
    // block/醒节拍必涨其所在核）。
    assert!(sw0_e + sw1_e > sw0_b + sw1_b,
            "alloc: sw 合计零前进（b {}+{} e {}+{}——调度器停摆面）",
            sw0_b, sw1_b, sw0_e, sw1_e);
    // ⑥ 二次放行（flag 页 +8）：全员退（末线程收口 drain——帧归还属
    // 进程生命周期正确语义，非断言面）+ 终账完成轮数。
    let one8: u8 = 1;
    assert_eq!(
        svc6(
            NR_VMO_WRITE as u64, hd as u64,
            core::ptr::addr_of!(one8) as u64, 8, 1, 0, 0,
        ),
        0,
        "alloc: vmo_write[flag2]"
    );
    let mut dones = 0u64;
    for i in 0..STORM_PARTICIPANTS {
        dones += storm_win_read(hd, i, STORM_WIN_DONE);
    }
    assert_eq!(dones, (STORM_ROUNDS * STORM_PARTICIPANTS) as u64,
               "alloc: 完成轮数 {dones} != 320");
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"ALLOC ok n=");
        berry_hal_aarch64::uart_put_dec(dones);
        berry_hal_aarch64::uart_puts(b"\n");
    });
    berry_sched::exit_current()
}

/// S-5b 相 7（smoke-smp `ht` 门）：双核 EL0 HandleTable 风暴压测验收。
/// 载荷 = 同 fleet 形态换 event 风暴（`__user_htst`：每轮 event_create→
/// object_signal→handle_close）。**被打的共享结构 = 子进程 handle 表**
/// （单进程五线程跨两核共用一表，SVC_CLASS_SQ_RQ_TABLE 锁竞争面——与
/// 相 6 的 per-worker 独立 churn 不同维度）。**二次放行形**（S-5b 变异
/// 标定 HT-1 实证修正）：worker 轮毕后不直退——就绪位自宣（win+0x94）
/// + park 自旋第二放行位（flag 页+8）→ master 就绪收割后在**真静默窗**
/// 走查（表操作全停且进程未收口——join 后旧位被末线程 drain 恒清，
/// 恒真断言无活性）→ 二次放行全员退 → 终账。静默点双走查（S-5a
/// integrity_check 调用契约）：①风暴前（线程 park 自旋中，子表出生为
/// 空——fleet svc 全落 master 表）+ ②真静默窗（全员就绪 parked）——两点
/// 均须 `integrity_check() == Ok(0)` 且 `used_count() == 0`（风暴
/// create/close 对消终态）。终账同相 6（err/done/双核 sw）。`HT ok n=`
/// 后 exit_current。
extern "C" fn task_ht(arg: usize) -> ! {
    use crate::abi_gen::NR_VMO_WRITE;
    use crate::handle::ObjType;
    use crate::user::user_htst_code;
    let budget = 8 * berry_hal_aarch64::timer::cntfrq();
    let (hp, _hv, hd) = storm_spawn_fleet("H", user_htst_code(), STORM_WRITER_N, 0);
    // 子表可达面：master 表的 process handle → KernelObj::Process → .ht。
    // SAFETY: current_tag = spawn_with_tag 合同钉死的本任务私有表（非 0）；
    // 本任务执行流内独占（svc 串行于自身），走查窗在静默点（见上）。
    let t: &mut crate::handle::HandleTable =
        unsafe { &mut *(berry_sched::current_tag() as *mut crate::handle::HandleTable) };
    let child = {
        let (oref, _) = t
            .get_raw(hp, ObjType::Process, 0)
            .expect("ht: 取子 Process");
        // SAFETY: get_raw 已三门控（在表、类型 Process）；NonNull 指向表内
        // ObjRef（handle 持有钉住存活——tlbs_round Drop 臂同论证）。
        let objref: &crate::handle::ObjRef = unsafe { oref.as_ref() };
        objref.obj.as_process().expect("ht: 类型面意外")
    };
    // 静默点①：风暴前（park 自旋中）。
    berry_hal_aarch64::kern_lock::klock_enter();
    assert_eq!(child.ht.integrity_check(), Ok(0), "ht: 风暴前走查非 Ok(0)");
    assert_eq!(child.ht.used_count(), 0, "ht: 风暴前子表非空");
    berry_hal_aarch64::kern_lock::klock_exit();
    // 放行 + 轮询收割（同相 6）。
    let (sw0_b, sw1_b, _, _) = berry_hal_aarch64::smp::sw_park_readout();
    let one: u8 = 1;
    assert_eq!(
        svc6(
            NR_VMO_WRITE as u64, hd as u64,
            core::ptr::addr_of!(one) as u64, 0, 1, 0, 0,
        ),
        0,
        "ht: vmo_write[flag]"
    );
    // 就绪收割（READY 槽）：全员到 3: 段（轮毕 + 就绪位自宣 + park 自旋
    // 于第二放行位）——**真静默窗**：表操作全停（末次 close 毕）、进程未
    // 收口（无人 thread_exit——drain 不发生）。
    let (errs, readies) = storm_poll(arg, HT_EV_BIT, hd, budget, STORM_WIN_READY,
                                      STORM_PARTICIPANTS, STORM_ROUNDS);
    assert_eq!(errs, 0, "ht: 风暴 err {errs}（svc 非零）");
    assert_eq!(readies, STORM_PARTICIPANTS as u64,
               "ht: 就绪 {readies} != {}（早断/超时面）", STORM_PARTICIPANTS);
    // 静默点②（真静默窗）：走查 + 空表断言（风暴 create/close 对消终态；
    // join 后旧位已被末线程收口 drain 恒清——S-5b 变异标定 HT-1 实证，
    // 断言移此才有活性）。
    berry_hal_aarch64::kern_lock::klock_enter();
    assert_eq!(child.ht.integrity_check(), Ok(0),
               "ht: 静默窗走查非 Ok(0)（残留态/UAF 面——竞态真病灶）");
    assert_eq!(child.ht.used_count(), 0, "ht: 静默窗子表非空（close 漏）");
    berry_hal_aarch64::kern_lock::klock_exit();
    // 二次放行（flag 页 +8）：全员 thread_exit（末线程触发收口 drain——
    // 此后表归零属进程生命周期正确语义，非断言面）。
    let one8: u8 = 1;
    assert_eq!(
        svc6(
            NR_VMO_WRITE as u64, hd as u64,
            core::ptr::addr_of!(one8) as u64, 8, 1, 0, 0,
        ),
        0,
        "ht: vmo_write[flag2]"
    );
    // 终账完成轮数 = 手动 Σ DONE 槽（worker 退出与读窗竞速无关——
    // 0x90 轮满即写，退出不改）。
    let mut dones = 0u64;
    for i in 0..STORM_PARTICIPANTS {
        dones += storm_win_read(hd, i, STORM_WIN_DONE);
    }
    assert_eq!(dones, (STORM_ROUNDS * STORM_PARTICIPANTS) as u64,
               "ht: 完成轮数 {dones} != 320");
    let (sw0_e, sw1_e, _, _) = berry_hal_aarch64::smp::sw_park_readout();
    // 同相 6 合计式（per-CPU 满载荷独占核零前进是物理正常态）。
    assert!(sw0_e + sw1_e > sw0_b + sw1_b,
            "ht: sw 合计零前进（b {}+{} e {}+{}）", sw0_b, sw1_b, sw0_e, sw1_e);
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"HT ok n=");
        berry_hal_aarch64::uart_put_dec(dones);
        berry_hal_aarch64::uart_puts(b"\n");
    });
    berry_sched::exit_current()
}

/// S-5c 相 8（smoke-smp `pp` 门）：双核 channel ping-pong 压测验收。
/// 载荷 = 单进程双线程（root A 家核 + B 写者 spread=true→CPU1——
/// `__user_pp` arg 分流单模板，fleet writer_n=1 形）：A channel_create 两
/// 端点、端点 1 经 data 窗口中继 B 窗 0xA0（跨核 handle 传递桥——v1 无
/// IPC-for-handles），N=320 轮严格交替 send/recv——**双向序号逐轮验**
/// （零丢失/零乱序/零重复对账）。与相 6/7 的关键形态差：**无 go-flag/
/// 二次放行**——断言面 = 窗口槽值（master 持 data VMO handle 直读；进程
/// 收口 drain 清的是子表 handle，洗不掉窗口页——S-5b「join 后恒真洞」
/// 不适用），DONE 收满即线程自退。终账四断言：err 全 0 / DONE 合计
/// 2N / a=b=320（三面钉）/ sw 合计式参与证（同相 6/7）。`PP ok a= b=`
/// 后 exit_current。
extern "C" fn task_pp(arg: usize) -> ! {
    use crate::user::user_pp_code;
    let budget = 8 * berry_hal_aarch64::timer::cntfrq();
    let (_hp, _hv, hd) = storm_spawn_fleet("P", user_pp_code(), 1, 1);
    let (sw0_b, sw1_b, _, _) = berry_hal_aarch64::smp::sw_park_readout();
    // DONE 面直收（a+b 满值 2N；err>0 早断——任一侧早退则对侧卡 EAGAIN
    // 自旋，errs 非零即红不挂死——超时兜底 = assert fail-loud）。
    let (errs, dones) = storm_poll(arg, PP_EV_BIT, hd, budget, STORM_WIN_DONE, 2, PP_ROUNDS);
    assert_eq!(errs, 0, "pp: 载荷 err {errs}（svc 非零/序号错序面）");
    assert_eq!(dones, (2 * PP_ROUNDS) as u64,
               "pp: 完成 {dones} != {}（早断/超时面）", 2 * PP_ROUNDS);
    // 双面分账：a = A 窗（发送+验回声满轮）/ b = B 窗（收+验序满轮）。
    let a = storm_win_read(hd, 0, STORM_WIN_DONE);
    let b = storm_win_read(hd, 1, STORM_WIN_DONE);
    assert_eq!(a, PP_ROUNDS as u64, "pp: A 面 {a} != {}（A 侧缺轮）", PP_ROUNDS);
    assert_eq!(b, PP_ROUNDS as u64, "pp: B 面 {b} != {}（B 侧缺轮——丢消息面）",
               PP_ROUNDS);
    let (sw0_e, sw1_e, _, _) = berry_hal_aarch64::smp::sw_park_readout();
    // 合计式同相 6/7（per-CPU 满载荷独占核零前进是物理正常态；双核结构证
    // = root A 家核 + B spread K=2→CPU1 + a/b 双满）。
    assert!(sw0_e + sw1_e > sw0_b + sw1_b,
            "pp: sw 合计零前进（b {}+{} e {}+{}——调度器停摆面）",
            sw0_b, sw1_b, sw0_e, sw1_e);
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"PP ok a=");
        berry_hal_aarch64::uart_put_dec(a);
        berry_hal_aarch64::uart_puts(b" b=");
        berry_hal_aarch64::uart_put_dec(b);
        berry_hal_aarch64::uart_puts(b"\n");
    });
    berry_sched::exit_current()
}

/// S-5d soakload master（fw_cfg `opt/berry/soakload` 门控）：spawn 永续
/// churn 舰队（root 家核 + `SOAKLOAD_WRITER_N` 写者 spread→CPU1——
/// storm_spawn_fleet 复用；data VMO 仅作窗口/sp 布局，无 go-flag/DONE 面）
/// 后**常驻不退**——master 表持有舰队三 handle（进程/Vmar/data VMO）=
/// 载荷进程生命周期锚（master 退 = 表 drain 关进程 handle = 舰队连坐死，
/// 「常驻载荷」合同破坏）。`SOAKLOAD fleet on n=` 自证行打在舰队真 spawn
/// 之后（行在 ⟺ 载荷在打）；随后 arm churn 计数（S-5e M1：vmo_create 成功
/// 面计数 → 心跳 v2.1 churn=——svc 恒败/gen 顶格/写者被杀即冻红）。永睡形：
/// 1h 死线轮转 block（醒因必 Timeout——哑 Event 无人 signal，soakmon 同款
/// 契约）；boot 期一次性 spawn 链（~10 svc）后零内核态足迹。
extern "C" fn task_soakload(arg: usize) -> ! {
    use berry_sched::WakeReason;
    use crate::user::user_soakload_code;
    let (_hp, _hv, _hd) = storm_spawn_fleet("L", user_soakload_code(), SOAKLOAD_WRITER_N, 1);
    crate::syscall::soakload_churn_arm();
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(b"SOAKLOAD fleet on n=");
        berry_hal_aarch64::uart_put_dec((1 + SOAKLOAD_WRITER_N) as u64);
        berry_hal_aarch64::uart_puts(b"\n");
    });
    let hour = 3600 * berry_hal_aarch64::timer::cntfrq();
    loop {
        berry_hal_aarch64::kern_lock::klock_enter();
        let deadline = berry_hal_aarch64::timer::cntvct() + hour;
        let r = berry_sched::block_current(arg, SOAKLOAD_EV_BIT, deadline);
        berry_hal_aarch64::kern_lock::klock_exit();
        assert!(r == WakeReason::Timeout,
                "soakload: 醒因非 Timeout（哑 Event 永不置位——跨表 wake 面破损）");
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
    use crate::syscall::{
        NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_CLOCK_READ, NR_OBJECT_SIGNAL, NR_OBJECT_WAIT_ONE,
    };
    let ha = arg as u32;
    let h_done = (arg >> 32) as u32;
    let mut buf = [0u8; 8];
    let mut rt_ticks: u64 = 0;
    // min 种子 u64::MAX：首轮必落入；测量语义见 AI_RT_MIN 注释（噪声底）。
    let mut rt_min: u64 = u64::MAX;
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
        // S-4 4-3：有界等待，deadline = **MONOTONIC 纳秒**（`sys_object_wait_one`
        // 契约；clock_read 锚 + 1s 帽 = 正常轮 10× 余量，fail loud 语义不变；
        // pong 侧 10s 帽不动）。
        // 沿革定谳（三轮 lldb 尸检，雷册 §A118 修订版）：4-2 引入有界等待时
        // 把 `t0 + 10_000_000`（CNTVCT 周期数）直接喂给了 ns 语义参数——
        // `ns_to_vct` 换算后 deadline 恒在远古（62.5MHz 下阈值 t0 ≥ 1e7/15
        // ≈ 0.67M 周期 ≈ boot 后 ~11ms 起，任意时刻都过点），「睡到点」自
        // 出生起退化为**瞬时超时 poll**：SMP 下
        // pong 回件与 ping 的 poll 竞速，回件未落队即 -ETIMEDOUT 假阴性
        //（概率性 per-boot——同核直切链赢则绿、跨核竞速输则红）。尸检签名 =
        // 回件 AIR 活躺 ha 队列 + pong 已完成本轮回睡 + ping ETIMEDOUT——
        // 睡眠从未发生。修复 = 单位归一（同 pong 的 clock_read 锚）。
        let mut now: u64 = 0;
        // SAFETY: 同上。
        let pnow = core::ptr::addr_of_mut!(now) as u64;
        assert_eq!(svc4(NR_CLOCK_READ as u64, 0, pnow, 0), 0, "ai_ping: clock_read");
        assert_eq!(
            svc6(NR_OBJECT_WAIT_ONE as u64, ha as u64, SIGNAL_READABLE,
                 now + 1_000_000_000, pobs, 0, 0),
            0,
            "ai_ping: wait READABLE（断链超时即 FAIL）"
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
        let d = berry_hal_aarch64::timer::cntvct() - t0;
        rt_ticks += d;
        if d < rt_min {
            rt_min = d;
        }
    }
    AI_RT_LAST.store(rt_ticks / AI_ROUNDS, core::sync::atomic::Ordering::Relaxed);
    AI_RT_MIN.store(rt_min, core::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, h_done as u64, 0, AI_EV_DONE_BIT),
        0,
        "ai_ping: signal E_done"
    );
    berry_sched::exit_current()
}
