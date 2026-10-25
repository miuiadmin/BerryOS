//! 内核微基准 + churn 泄漏金丝雀（B-0/S-0；设计 = `docs/基准与压力测试设计.md`）。
//!
//! 定位：**测量是优化的前提**——没有基线数字，任何「变快了/变慢了」都无法
//! 证明。本模块在 `ktest::run()` 之后、`timer::init`/`berry_sched::init` 之前
//! 运行：单线程、IRQ 屏蔽、分配器已活——零调度噪声，纯子系统成本。
//!
//! 两条产出线：
//! 1. **微基准**（`BENCH,<name>,<iters>,<cyc_per_op>` 机器可读行 × 6）——
//!    计时 = CNTVCT **原始周期数**。口径纪律（测试策略 §7）：TCG 下 CNTV
//!    推进 ≈ CNTFRQ/2，绝对值只做**同机同 QEMU 的相对比较**（基线对拍在
//!    宿主侧 `tools/bench.py`，宽松因子门禁）；内核只出数不做门禁。
//! 2. **churn 泄漏金丝雀**（`BENCHCHURN ok frame=0 vmo=0 handle=0`）——
//!    确定性**严格断言**：反复创建/销毁后 `mm::allocated_pages()` 水位与
//!    handle 表在用槽数必须精确归位，泄漏 ≠ 0 → panic。这属于「主干永远
//!    绿」红线的一部分（每次 boot 都跑），不是可选项。
//!
//! 收尾行 `BENCH ok n=6` 是 smoke 裸相关卡标记（标记清单单一事实源 =
//! `tools/smoke.sh`）。EL0 视角的 syscall/IPC 路径基准（含 SVC 陷阱开销）
//! 留 B-1 用户态基准程序（rootfs 生态）——内核内测不了异常级往返。

// 与 ktest 同款豁免：基准/金丝雀路径的 expect 表达「不可能失败」——失败即
// panic = 主干红线（见模块文档），不是可恢复路径。
#![allow(clippy::result_unwrap_used, clippy::result_expect_used)]

use crate::handle::{HandleTable, KernelObj, ObjType, RIGHT_SIGNAL};
use crate::mm;
use crate::vmo::Vmo;
use berry_hal_aarch64::timer::cntvct;
use berry_hal_aarch64::{uart_put_dec, uart_puts};

const PAGE: usize = 1 << 12;

/// 基准项计数（`BENCH ok n=` 的定值——增删基准项时同步改这里、smoke 两处
/// 标记表与 bench.py 的表头注释）。
const N_METRICS: usize = 6;

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
