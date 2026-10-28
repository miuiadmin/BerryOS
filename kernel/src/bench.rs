//! 内核微基准 + churn 泄漏金丝雀（B-0/S-0；设计 = `docs/基准与压力测试设计.md`）。
//!
//! 定位：**测量是优化的前提**——没有基线数字，任何「变快了/变慢了」都无法
//! 证明。本模块在 `ktest::run()` 之后、`timer::init`/`berry_sched::init` 之前
//! 运行：单线程、IRQ 屏蔽、分配器已活——零调度噪声，纯子系统成本。
//!
//! 三条产出线：
//! 1. **微基准**（`BENCH,<name>,<iters>,<cyc_per_op>` 机器可读行 × 6）——
//!    计时 = CNTVCT **原始周期数**。口径纪律（测试策略 §7）：TCG 下 CNTV
//!    推进 ≈ CNTFRQ/2，绝对值只做**同机同 QEMU 的相对比较**（基线对拍在
//!    宿主侧 `tools/bench.py`，宽松因子门禁）；内核只出数不做门禁。
//! 2. **churn 泄漏金丝雀**（`BENCHCHURN ok frame=0 vmo=0 handle=0`）——
//!    确定性**严格断言**：反复创建/销毁后 `mm::allocated_pages()` 水位与
//!    handle 表在用槽数必须精确归位，泄漏 ≠ 0 → panic。这属于「主干永远
//!    绿」红线的一部分（每次 boot 都跑），不是可选项。
//! 3. **S-2 资源耗尽行为**（`BENCHXH ok frame=1 vmo=1 handle=1 chan=1`，
//!    设计 §12）——资源到顶时**错误码回、不 panic、可恢复**的确定性边界
//!    断言：ABI §8.0「永不 panic」承诺的耗尽版执法（全过才打，L0）。
//!
//! 收尾行 `BENCH ok n=6` 是 smoke 裸相关卡标记（标记清单单一事实源 =
//! `tools/smoke.sh`）。EL0 视角的 syscall/IPC 路径基准（含 SVC 陷阱开销）
//! 留 B-1 用户态基准程序（rootfs 生态）——内核内测不了异常级往返。

// 与 ktest 同款豁免：基准/金丝雀路径的 expect 表达「不可能失败」——失败即
// panic = 主干红线（见模块文档），不是可恢复路径。
#![allow(clippy::result_unwrap_used, clippy::result_expect_used)]

use crate::channel::CHAN_DEPTH;
use crate::handle::{
    ERR_AGAIN, ERR_BUSY, ERR_NO_MEMORY, HandleTable, KernelObj, ObjType, RIGHT_SIGNAL,
};
use crate::ktest::{svc4, svc6};
use crate::mm;
use crate::syscall::{
    NR_CHANNEL_CREATE, NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_HANDLE_CLOSE, NR_VMO_CREATE,
    VMO_MAX_SIZE,
};
use crate::vmo::Vmo;
use berry_hal_aarch64::timer::cntvct;
use berry_hal_aarch64::{uart_put_dec, uart_puts};
use berry_mm::{AllocError, MAX_ORDER};

const PAGE: usize = 1 << 12;

/// 基准项计数（`BENCH ok n=` 的定值——增删基准项时同步改这里、smoke 两处
/// 标记表与 bench.py 的表头注释）。
const N_METRICS: usize = 6;

// ---- S-2（设计 §12）常量 ----

/// handle 面 fill 槽数（v1 表无 syscall 硬帽、真帽 = 堆——4096 轮盖
/// 「错误码回 + 可回收」语义，§12 定值）。
const XH_HANDLE_N: usize = 4096;

/// frame 面耗尽块收集槽容量：**定长数组、不用 Vec**——全帧耗尽的瞬态里
/// 「堆不可用」正是被测状态，收集路径自身必须零分配（§12 风险注记的落地
/// 形态）。上界 = 窗口期空闲区的 buddy 块切分总数：QEMU 直载路径空闲区
/// 1–2 段（每段每阶 ≤1 块）、EFI handoff 31 段图残余 ~200 块——512 = 2x
/// 余量；溢出即 panic（L0 执法，届时按 §12 降级注记改「order-13 大块耗尽」
/// 形态）。
const XH_MAX_BLOCKS: usize = 512;

/// chan 面载荷长度（8B 最小有意义载荷；recv 容量/字节数回写断言同值）。
const XH_MSG: u64 = 8;

// ---- S-2 静态缓冲（boot 窗口单线程 IRQ 屏蔽；只经 raw 指针触碰——
// static mut 禁直接取引用，与 ktest `&raw mut` / 本模块 SRC 同纪律）----

// channel_create 双端出参 / vmo_create 出参（出参宽 4B——写 u64 槽低半）。
static mut XH_C0: u64 = 0;
static mut XH_C1: u64 = 0;
static mut XH_VOUT: u64 = 0;
// chan 面收发缓冲 + recv 容量槽（in-out：进 = 容量，出 = 实际值）。
static mut XH_SBUF: [u8; 8] = [0x58; 8];
static mut XH_RBUF: [u8; 8] = [0; 8];
static mut XH_NB: u64 = 0;
static mut XH_NH: u64 = 0;
static mut XH_HND: [u32; 4] = [0; 4];
// handle 面 4096 槽值收集（静态化——16KiB 不吃主栈）。
static mut XH_HS: [u32; XH_HANDLE_N] = [0; XH_HANDLE_N];

pub fn run() {
    bench_cntvct_pair();
    bench_frame_alloc4k();
    bench_frame_alloc64k();
    bench_vmo_commit1p();
    bench_vmo_rw4k();
    bench_handle_evt();
    churn_frame();
    churn_vmo();
    churn_handle();
    // 收尾关卡行：三项 churn 金丝雀全部严格断言过了才走到这里（delta 恒 0，
    // 非 0 即已在上面 panic——本行是状态谓词，非竞速面）。
    uart_puts(b"BENCHCHURN ok frame=0 vmo=0 handle=0\n");
    // S-2 六面耗尽/恢复（boot 输出序：BENCHCHURN 行后、BENCH 收尾行前——
    // 设计 §12；全过才打 BENCHXH，任一断言破 = panic = smoke FAIL）。
    xh_run();
    uart_puts(b"BENCH ok n=");
    uart_put_dec(N_METRICS as u64);
    uart_puts(b"\n");
}

/// 计时循环骨架：`iters` 次 `f`，打一行 `BENCH,<name>,<iters>,<cyc_per_op>`。
/// 整除取每操作周期数（不做小数——相对比较用，精度足够）。
fn bench<F: FnMut()>(name: &[u8], iters: u32, mut f: F) {
    let t0 = cntvct();
    for _ in 0..iters {
        f();
    }
    let t1 = cntvct();
    let per = (t1 - t0) / iters as u64;
    uart_puts(b"BENCH,");
    uart_puts(name);
    uart_puts(b",");
    uart_put_dec(iters as u64);
    uart_puts(b",");
    uart_put_dec(per);
    uart_puts(b"\n");
}

/// 测量基线：一对 cntvct 读 + 循环骨架自身开销——其余基准项与它相减
/// 才是子系统净成本（宿主侧对拍时同表呈现，人工心算口径）。
fn bench_cntvct_pair() {
    bench(b"cntvct_pair", 4096, || {
        let _ = cntvct();
    });
}

/// 单页帧分配 + 释放往返（buddy 最小阶全链）。
fn bench_frame_alloc4k() {
    bench(b"frame_alloc4k", 1024, || {
        let pfn = mm::frames().alloc().expect("bench: frame alloc");
        mm::frames().free(pfn, 0).expect("bench: frame free");
    });
}

/// 64KiB 连续块（order 4 = 16 页）分配 + 释放往返——拆块/合并路径。
fn bench_frame_alloc64k() {
    bench(b"frame_alloc64k", 256, || {
        let pfn = mm::frames()
            .alloc_order(4)
            .expect("bench: frame alloc_order");
        mm::frames().free(pfn, 4).expect("bench: frame free");
    });
}

/// 单页 VMO 全生命周期：create + commit（含 4KiB 清零）+ drop（还帧）。
fn bench_vmo_commit1p() {
    bench(b"vmo_commit1p", 256, || {
        let mut vmo = Vmo::create(PAGE).expect("bench: vmo create");
        vmo.commit(mm::frames(), 0, PAGE).expect("bench: vmo commit");
        drop(vmo);
    });
}

/// 已提交单页 VMO 的 4KiB write + 4KiB read 往返（内核侧拷贝路径——
/// syscall 面 vmo_read/vmo_write 拷贝成本的下界）。源/目的是静态缓冲，
/// 读回首尾断言防死存储消除。
fn bench_vmo_rw4k() {
    static mut SRC: [u8; PAGE] = [0xA5; PAGE];
    static mut DST: [u8; PAGE] = [0; PAGE];
    // SAFETY: bench 期单线程、IRQ 屏蔽；静态缓冲只在本函数经 raw 指针触碰
    //（static mut 禁直接取引用——与 ktest `&raw mut` 同纪律）。
    let src: &[u8] = unsafe { &*core::ptr::addr_of!(SRC) };
    let dst: &mut [u8] = unsafe { &mut *core::ptr::addr_of_mut!(DST) };
    let mut vmo = Vmo::create(PAGE).expect("bench: vmo create");
    vmo.commit(mm::frames(), 0, PAGE).expect("bench: vmo commit");
    bench(b"vmo_rw4k", 128, || {
        vmo.write(0, src).expect("bench: vmo write");
        vmo.read(0, dst).expect("bench: vmo read");
    });
    // 读回形状断言：SRC 全 0xA5（含首尾），读回应逐字节相同。
    assert_eq!(dst[0], 0xA5, "bench: vmo_rw4k readback head");
    assert_eq!(dst[PAGE - 1], 0xA5, "bench: vmo_rw4k readback tail");
}

/// Event 对象经 handle 表全生命周期：new + insert + get（查表热路径）+
/// close（drop Box）——每次 syscall 都要过的查表门在 get 里。
fn bench_handle_evt() {
    let mut ht = HandleTable::new();
    bench(b"handle_evt", 1024, || {
        let h = ht
            .insert(KernelObj::Event(crate::event::Event::new()), RIGHT_SIGNAL)
            .expect("bench: insert");
        let _ = ht.get(h, ObjType::Event, 0).expect("bench: get");
        ht.close(h).expect("bench: close");
    });
    assert_eq!(ht.used_count(), 0, "bench: handle_evt 后表须空");
}

/// **churn 金丝雀①帧**：混合阶（0/2/4）交错分配释放 64 轮，水位必须精确
/// 归位——buddy 合并路径任何丢失/双释都会在此现形（确定性断言，非统计）。
fn churn_frame() {
    let base = mm::frames().allocated_pages();
    for _ in 0..64 {
        let mut held: [(usize, usize); 11] = [(0, 0); 11];
        let mut n = 0;
        // 混合 8×单页 + 2×4 页块 + 1×16 页块；分配序 ≠ 释放序（洗牌效应）。
        for _ in 0..8 {
            held[n] = (mm::frames().alloc().expect("churn: alloc"), 0);
            n += 1;
        }
        for _ in 0..2 {
            held[n] = (mm::frames().alloc_order(2).expect("churn: alloc2"), 2);
            n += 1;
        }
        held[n] = (mm::frames().alloc_order(4).expect("churn: alloc4"), 4);
        // 逆序释放：与分配序相反的固定序（合并路径走足）。
        for i in (0..=n).rev() {
            let (pfn, o) = held[i];
            mm::frames().free(pfn, o).expect("churn: free");
        }
        // 逐轮对账：哪一轮泄漏当场指认（弱断言会掩盖逐轮漂移）。
        assert_eq!(
            mm::frames().allocated_pages(),
            base,
            "churn: frame watermark drifted (round leak)"
        );
    }
}

/// **churn 金丝雀②VMO**：64KiB VMO（16 页）create + 全量 commit + 首页写 +
/// drop，64 轮——页数最多的归还路径（Vmo::Drop 还帧链）反复走。
fn churn_vmo() {
    static mut BUF: [u8; PAGE] = [0x77; PAGE];
    // SAFETY: 同 bench_vmo_rw4k——raw 指针取用，单线程窗口。
    let buf: &[u8] = unsafe { &*core::ptr::addr_of!(BUF) };
    let base = mm::frames().allocated_pages();
    for _ in 0..64 {
        let mut vmo = Vmo::create(16 * PAGE).expect("churn: vmo create");
        vmo.commit(mm::frames(), 0, 16 * PAGE)
            .expect("churn: vmo commit");
        vmo.write(0, buf).expect("churn: vmo write");
        drop(vmo);
        assert_eq!(
            mm::frames().allocated_pages(),
            base,
            "churn: vmo watermark drifted (Vmo::Drop leak)"
        );
    }
}

/// **churn 金丝雀③handle**：私有表 32 槽 insert/close × 64 轮——表须精确
/// 清空（used_count == 0），Event/Box 分配释放配平。帧水位作旁证——Event
/// 不持帧，恒等；**先预热一轮再取基线**（首轮 slots Vec 懒增长会合法地向
/// 堆要一页——首跑实抓 +1 页假阳性；预热后 Vec 容量已定，漂移即真泄漏）。
fn churn_handle() {
    let mut ht = HandleTable::new();
    // 预热：一次满 32 槽 insert/close——把 Vec 增长/堆 bin 预热全部吃完。
    let mut warm = [0u32; 32];
    for h in warm.iter_mut() {
        *h = ht
            .insert(KernelObj::Event(crate::event::Event::new()), RIGHT_SIGNAL)
            .expect("churn: warm insert");
    }
    for &h in warm.iter() {
        ht.close(h).expect("churn: warm close");
    }
    assert_eq!(ht.used_count(), 0, "churn: 预热轮后表须空");
    let base = mm::frames().allocated_pages();
    for _ in 0..64 {
        let mut hs = [0u32; 32];
        for h in hs.iter_mut() {
            *h = ht
                .insert(KernelObj::Event(crate::event::Event::new()), RIGHT_SIGNAL)
                .expect("churn: insert");
        }
        assert_eq!(ht.used_count(), 32, "churn: 32 槽全在用");
        for &h in hs.iter() {
            ht.close(h).expect("churn: close");
        }
        assert_eq!(ht.used_count(), 0, "churn: 全关光后表须空");
    }
    assert_eq!(
        mm::frames().allocated_pages(),
        base,
        "churn: handle path frame watermark drifted"
    );
}

// =========================================================================================
// S-2：资源耗尽行为（设计 `docs/基准与压力测试设计.md` §12）
// =========================================================================================

/// **S-2 资源耗尽行为**：六面确定性严格断言——资源到顶时**错误码回、不
/// panic、可恢复**（ABI §8.0「永不 panic」承诺的耗尽边界执法，进 smoke 裸相
/// L0）。公约 6：boot 窗口单线程 IRQ 屏蔽 = 全帧耗尽瞬态无并发分配者，安全。
///
/// **复位对账结构**（S-0 预热立法的 S-2 版）：窗口内三个**永久性**帧效应全部
/// 预付在全局基线之前——①通道对端点队列（`ChannelEnd::pair` 互 retain →
/// close 不 drop，队列 Vec ≈2.7KB/端 = 1 页/端永不还——channel.rs 环泄漏
/// 安全向政策）；②handle 面 4096 槽的 `ObjRef` Box slab 页（G0 政策：slab
/// 页永不还 buddy——预热满轮把池扩到位，真轮纯复用）；③BOOTSTRAP_HT 槽位
/// Vec 残余增长（ktest 期已基本定型，随①落定）。基线后六面只做
/// 「吃光 → 全量归还」的自愈往返，末尾 `allocated_pages() == base` 恒等。
fn xh_run() {
    // ---- 预热①：SVC 造通道对（EL1 tag==0 → BOOTSTRAP_HT，ktest H4 同路）。
    // 端点队列 ×2 页 + Box ×2（slab）落定；chan 两面复用这对端点。
    // SAFETY: 静态出参槽经 raw 指针传入（4B 写入 u64 槽低半，高半保持 0）。
    unsafe { XH_C0 = 0; XH_C1 = 0; }
    assert_eq!(
        svc4(
            NR_CHANNEL_CREATE as u64,
            0,
            &raw mut XH_C0 as u64,
            &raw mut XH_C1 as u64,
        ),
        0,
        "bench: xh 预热 channel_create"
    );
    let (hc0, hc1) = (unsafe { XH_C0 } as u32, unsafe { XH_C1 } as u32);
    assert_ne!(hc0, 0, "bench: xh 预热端点 0 须非零");
    assert_ne!(hc1, 0, "bench: xh 预热端点 1 须非零");
    assert_ne!(hc0, hc1, "bench: xh 预热两端须互异");

    // ---- 预热②：handle 满表一轮——slab 池扩容 + slots Vec 懒增长全预付
    //（churn_handle 预热先例的一般化）。表**存活到函数尾**：其 slots Vec 的
    // 大块（>2048B 走 buddy 直径）在对账后才随 drop 归还——早还会把水位打到
    // 基线之下、恒等断言反手误伤。
    let mut ht = HandleTable::new();
    // SAFETY: 单线程窗口；静态槽值数组只经 raw 指针触碰。
    let hs: &mut [u32] = unsafe { &mut *core::ptr::addr_of_mut!(XH_HS) };
    for h in hs.iter_mut() {
        *h = ht
            .insert(KernelObj::Event(crate::event::Event::new()), RIGHT_SIGNAL)
            .expect("bench: xh 预热 insert");
    }
    assert_eq!(ht.used_count(), XH_HANDLE_N, "bench: xh 预热满表");
    for &h in hs.iter() {
        ht.close(h).expect("bench: xh 预热 close");
    }
    assert_eq!(ht.used_count(), 0, "bench: xh 预热清空");

    // ---- 全局基线（三个永久效应已全部入账）----
    let base = mm::frames().allocated_pages();

    // ---- 六面（设计表序）----
    let f_frame = xh_frame_exhaust(base);
    let f_vmo = xh_vmo_cap(base);
    let f_handle = xh_handle_fill(base, &mut ht, hs);
    let f_chan = xh_chan_full(base, hc0, hc1) && xh_chan_drain(hc1);

    // ---- 面⑥复位对账：全部面跑完水位恒等（S-0 同口径金丝雀）----
    assert_eq!(
        mm::frames().allocated_pages(),
        base,
        "bench: xh 复位对账（六面后水位须恒等）"
    );
    // 通道对两端 close（环对象互钉不 drop——零帧变化；ktest H4 清场同款）。
    assert_eq!(
        svc4(NR_HANDLE_CLOSE as u64, hc0 as u64, 0, 0),
        0,
        "bench: xh close end0"
    );
    assert_eq!(
        svc4(NR_HANDLE_CLOSE as u64, hc1 as u64, 0, 0),
        0,
        "bench: xh close end1"
    );

    // 状态谓词收尾行：六面全过才走到这里（任一断言破已在上面 panic）。
    uart_puts(b"BENCHXH ok frame=");
    uart_put_dec(f_frame as u64);
    uart_puts(b" vmo=");
    uart_put_dec(f_vmo as u64);
    uart_puts(b" handle=");
    uart_put_dec(f_handle as u64);
    uart_puts(b" chan=");
    uart_put_dec(f_chan as u64);
    uart_puts(b"\n");
    // ht（slots Vec 大块随 drop 归还 buddy）在此函数尾自然失效——对账已毕。
}

/// 面①frame_exhaust：高阶到低阶把 buddy 空闲块**全部吃光** → 再分配必
/// `Err(OutOfMemory)`（「Err 非 panic」即被测行为）→ 逆序全量释放 → 水位
/// 精确归位。收集 = 定长数组（见 `XH_MAX_BLOCKS`——耗尽期间路径零分配，
/// §12 风险注记的执法形态）。
fn xh_frame_exhaust(base: usize) -> bool {
    // 8KiB 定长收集槽（主栈 256KiB——boot 窗口零压力）。
    let mut held: [(usize, u8); XH_MAX_BLOCKS] = [(0, 0); XH_MAX_BLOCKS];
    let mut n: usize = 0;
    let mut pages: usize = 0;
    // 高阶先吃：空闲区按 2^order 整数倍逐级分解，高阶先行让收集块数最少
    //（低阶零头在高位耗尽后由本阶兜住）。
    for order in (0..=MAX_ORDER).rev() {
        while let Ok(pfn) = mm::frames().alloc_order(order) {
            assert!(n < XH_MAX_BLOCKS, "bench: xh 收集槽溢出（§12 降级注记）");
            held[n] = (pfn, order as u8);
            n += 1;
            pages += 1 << order;
        }
    }
    // 全阶皆空：最小阶（最不易整块的粒度）再分配必 OutOfMemory。
    assert!(n > 0, "bench: xh 耗尽轮零分配（窗口内必有空闲——不可能）");
    assert_eq!(
        mm::frames().alloc(),
        Err(AllocError::OutOfMemory),
        "bench: xh 全阶耗尽后 alloc 须 Err(OutOfMemory)"
    );
    // 耗尽中对账：水位 = 基线 + 吃掉的页数（块头 2^order 口径、TAIL 不计——
    // 与 S-0 对账同源；任何阶的错账在此现形）。
    assert_eq!(
        mm::frames().allocated_pages(),
        base + pages,
        "bench: xh 耗尽中对账"
    );
    // 逆序全量释放：与分配序相反（buddy 合并路径走足——相邻块归并回高阶）。
    for i in (0..n).rev() {
        let (pfn, o) = held[i];
        mm::frames().free(pfn, o as usize).expect("bench: xh free");
    }
    assert_eq!(
        mm::frames().allocated_pages(),
        base,
        "bench: xh 释放后水位须精确归位"
    );
    // 量级下限：两路（QEMU 直载 / EFI handoff）窗口内空闲都 ≫ 16MiB——
    // 防「耗尽几个零散小块也算过」的空心绿。
    assert!(pages >= 4096, "bench: xh 耗尽页数下限（4k 页 = 16MiB）");
    true
}

/// 面②vmo_cap：尺寸帽两侧——超帽 `VMO_MAX_SIZE+1` 须 `-ENOMEM`（C-2c 在册
/// 帽，帽检先于出参/入表 = 零副作用）；边界恰 `VMO_MAX_SIZE` 须成（不
/// commit 不吃帧——页记账 Vec 走堆不走帧分配器）。syscall 面（真实 ABI 帽
/// 执法，EL1 → BOOTSTRAP_HT）+ 对象层直调（对象层无帽——结构性行为）双验。
fn xh_vmo_cap(base: usize) -> bool {
    // 超帽（syscall 面）：帽检在 copy_out/入表之前（sys_vmo_create 序）——
    // 错误码精确且零副作用。
    // SAFETY: 出参槽经 raw 指针传入（本调用不写——帽拒在写入之前）。
    assert_eq!(
        svc6(
            NR_VMO_CREATE as u64,
            (VMO_MAX_SIZE + 1) as u64,
            0,
            &raw mut XH_VOUT as u64,
            0,
            0,
            0,
        ),
        ERR_NO_MEMORY as i64,
        "bench: xh vmo 超帽须 -ENOMEM"
    );
    // 边界（syscall 面）：恰 VMO_MAX_SIZE（A1-2 起 128MiB）成 → close 即净（记账 Vec 经堆归还）。
    // SAFETY: 出参槽 4B 写入 u64 槽低半。
    unsafe { XH_VOUT = 0; }
    assert_eq!(
        svc6(
            NR_VMO_CREATE as u64,
            VMO_MAX_SIZE as u64,
            0,
            &raw mut XH_VOUT as u64,
            0,
            0,
            0,
        ),
        0,
        "bench: xh vmo 帽边界 create 须成"
    );
    let hv = unsafe { XH_VOUT } as u32;
    assert_ne!(hv, 0, "bench: xh vmo 帽边界 handle 须非零");
    assert_eq!(
        svc4(NR_HANDLE_CLOSE as u64, hv as u64, 0, 0),
        0,
        "bench: xh vmo close"
    );
    // 边界（对象层直调）：对象层无帽——32MiB 结构性成功；drop 即净。
    let vmo = Vmo::create(VMO_MAX_SIZE).expect("bench: xh vmo 对象层边界 create");
    assert_eq!(vmo.size(), VMO_MAX_SIZE, "bench: xh vmo 尺寸");
    drop(vmo);
    // 恒等：两面全程不 commit——帧水位不动（页记账在堆，与帧分配器无关）。
    assert_eq!(
        mm::frames().allocated_pages(),
        base,
        "bench: xh vmo 面水位恒等"
    );
    true
}

/// 面③handle_fill：私有表 4096 槽 insert → 全 close——`used_count`
/// 0↔4096 精确翻转 + 帧水位恒等（v1 表无 syscall 硬帽、真帽 = 堆；4096 轮
/// 盖「错误码回 + 可回收」语义，§12 定值）。复用预热表（容量已定型——真轮
/// 零增长零扩池）。
fn xh_handle_fill(base: usize, ht: &mut HandleTable, hs: &mut [u32]) -> bool {
    assert_eq!(hs.len(), XH_HANDLE_N, "bench: xh 槽值数组长度");
    for h in hs.iter_mut() {
        *h = ht
            .insert(KernelObj::Event(crate::event::Event::new()), RIGHT_SIGNAL)
            .expect("bench: xh insert");
    }
    assert_eq!(ht.used_count(), XH_HANDLE_N, "bench: xh 4096 槽全在用");
    for &h in hs.iter() {
        ht.close(h).expect("bench: xh close");
    }
    assert_eq!(ht.used_count(), 0, "bench: xh 全关光后表须空");
    assert_eq!(
        mm::frames().allocated_pages(),
        base,
        "bench: xh handle 面水位恒等"
    );
    true
}

/// 面④chan_full：自持双端 send×`CHAN_DEPTH` 全成 → 第 9 条 `-EBUSY`（非
/// 阻塞满即返）→ recv 排空（载荷/字节数逐条对账）→ **再 send 成**（恢复
/// 面：排空后端点可再用）→ 收回该条留空队给面⑤。send/recv 全走 SVC 真路径
///（错误码经 ABI 派发面回，非直调内核函数）。
fn xh_chan_full(base: usize, hc0: u32, hc1: u32) -> bool {
    // 填队：CHAN_DEPTH 连发全成。
    // SAFETY: 发送载荷静态缓冲经 raw 指针传入（EL1 拷贝面只读）。
    for _ in 0..CHAN_DEPTH {
        assert_eq!(
            svc6(
                NR_CHANNEL_SEND as u64,
                hc0 as u64,
                core::ptr::addr_of!(XH_SBUF) as u64,
                XH_MSG,
                0,
                0,
                0,
            ),
            0,
            "bench: xh 填队 send"
        );
    }
    // 第 CHAN_DEPTH+1 条 → -EBUSY（错误码精确——在册 CHAN_DEPTH=8 满点）。
    // SAFETY: 同上。
    assert_eq!(
        svc6(
            NR_CHANNEL_SEND as u64,
            hc0 as u64,
            core::ptr::addr_of!(XH_SBUF) as u64,
            XH_MSG,
            0,
            0,
            0,
        ),
        ERR_BUSY as i64,
        "bench: xh 满队 send 须 -EBUSY"
    );
    // 排水：CHAN_DEPTH 连收全成（每条容量槽进容量、回写实际值）。
    for _ in 0..CHAN_DEPTH {
        // SAFETY: 容量槽 in-out（进 = 容量 8B/4 槽，出 = 实际 8B/0 条）。
        unsafe {
            XH_NB = XH_MSG;
            XH_NH = 4;
        }
        assert_eq!(
            svc6(
                NR_CHANNEL_RECV as u64,
                hc1 as u64,
                &raw mut XH_RBUF as u64,
                &raw mut XH_NB as u64,
                &raw mut XH_HND as u64,
                &raw mut XH_NH as u64,
                0,
            ),
            0,
            "bench: xh 排水 recv"
        );
        assert_eq!(unsafe { XH_NB }, XH_MSG, "bench: xh recv 字节数回写");
        assert_eq!(unsafe { XH_RBUF[0] }, 0x58, "bench: xh recv 载荷首字节");
        assert_eq!(unsafe { XH_NH }, 0, "bench: xh recv handle 数回写");
    }
    // 恢复面：排空后再 send 成 → 收回该条 → 空队。
    // SAFETY: 同填队。
    assert_eq!(
        svc6(
            NR_CHANNEL_SEND as u64,
            hc0 as u64,
            core::ptr::addr_of!(XH_SBUF) as u64,
            XH_MSG,
            0,
            0,
            0,
        ),
        0,
        "bench: xh 恢复后 send 须成"
    );
    // SAFETY: 同排水。
    unsafe {
        XH_NB = XH_MSG;
        XH_NH = 4;
    }
    assert_eq!(
        svc6(
            NR_CHANNEL_RECV as u64,
            hc1 as u64,
            &raw mut XH_RBUF as u64,
            &raw mut XH_NB as u64,
            &raw mut XH_HND as u64,
            &raw mut XH_NH as u64,
            0,
        ),
        0,
        "bench: xh 恢复后 recv 收回"
    );
    // 恒等：send/recv 路径零分配（队列预储、无随行 handle）。
    assert_eq!(
        mm::frames().allocated_pages(),
        base,
        "bench: xh chan 面水位恒等"
    );
    true
}

/// 面⑤chan_drain：空队 recv 须 `-EAGAIN`（非阻塞空即返——与 ktest H1 同
/// 判据，S-2 视角重验「空也是资源边界」）。前提 = 面④收尾留空队。
fn xh_chan_drain(hc1: u32) -> bool {
    // SAFETY: 容量槽进容量（空队路径先检队空——出参不写）。
    unsafe {
        XH_NB = XH_MSG;
        XH_NH = 4;
    }
    assert_eq!(
        svc6(
            NR_CHANNEL_RECV as u64,
            hc1 as u64,
            &raw mut XH_RBUF as u64,
            &raw mut XH_NB as u64,
            &raw mut XH_HND as u64,
            &raw mut XH_NH as u64,
            0,
        ),
        ERR_AGAIN as i64,
        "bench: xh 空队 recv 须 -EAGAIN"
    );
    true
}
