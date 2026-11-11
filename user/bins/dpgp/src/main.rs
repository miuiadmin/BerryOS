//! G4-2（pager 服务首形，`G4按需调页实施设计草案.md` §4）：demand 装载
//! 受验子——**跨进程供给形**的 EL0 侧（thrchild 同款裸 bin：零 TLS、零
//! console、x0 直达 _start 无 crt0）。
//!
//! **零句柄形态**：bootstrap=0（x0=0）——本进程不建线程不碰对象，供给面
//! 全在父 svcmgr（它持 demand/src/像三 VMO + 子 root Vmar：跨进程 0x21
//! 供给对 faulter 所在进程无感——内核身份验只核「请求区间所属 Vmo ==
//! 供给方句柄本体」，同 Vmo 对象跨表即同体）。这也正是 §1.3 首场景
//!（exec 装载 demand 化）的最小缩影：装载器（父）即 pager。
//!
//! 链：入口直读 demand VA（供给前无翻译）→ EL0 D-abort(0x24) 探针（fsc
//! 滤 + asid 反查 + region 命中 → 武装 EL0 请求槽 + 唤 0x21 收货方 → 睡
//! fault 键）→ svcmgr `berry_pager_supply_pages`（0x21 阻塞收货：源页拷
//! 入供给页）→ 唤 faulter → eret 重试 → 读值 == pager 源页 magic →
//! exit 7；错值 exit 9。父侧按 7 打 `DPGP ok`、否则 `eco: dpgp exit <n>`
//! 降级行（标记缺席 = smoke FAIL 执法）。
//!
//! G4-4 X-2：**I-abort 腿走第二入口 `dpgi_start`**（`dpgi_entry` 段定
//! 0x2000，svcmgr `DPGI_ENTRY` 同值双改）——dpg_selftest 专用；本入口
//! 双腿序（magic 读后 call 0x7000 demand 代码页，首取指 EL0
//! I-abort(0x20) 同臂探针 → 父二供 → 返 0x5a），三态 7/9/11。`_start`
//! 主径不动 = storm/negative/soak/bench 消费面零漂移。
//!
//! 双序天然安全：子先 fault 后 0x21 = 槽武装等收（poll-first 直取）；
//! 0x21 先睡子后 fault = 探针唤醒收货方——无丢唤窗（KL 窗查位+入队）。
#![no_std]
#![no_main]

use berry_sys::{berry_clock_read, berry_process_exit, berry_thread_sleep, CLOCK_MONOTONIC};

/// demand 页 VA（svcmgr `dpg_selftest` 同值单源——两文件各持 const，
/// 改须双改；0x6000 = 像窗 [0x1000,0x6000) 界外首页，读它必走缺页路径）。
const DPGP_VA: usize = 0x6000;

/// G4-4 X-2：I-abort 腿 demand **代码页** VA（svcmgr `DPG_VA2` 同值双改；
/// 0x7000 = DPGP_VA 界外次页——取指它必走 I-abort(0x20) 缺页路径，供
/// 给后跳转执行，§5-⑦ 留面的 exec 代码页用例最小受验形）。
const DPGP_VA2: usize = 0x7000;
/// I 腿供给页函数期望返回值（源页手排 `movz w0,#0x5a; ret` 的 0x5a——
/// svcmgr `DPG_IFN_RET` 同值双改）。
const DPGP_IFN_RET: u32 = 0x5a;

/// 供给页首 8 字节期望值（svcmgr 预写入源 VMO 同值——"DPGPmagi" LE）。
const DPGP_MAGIC: u64 = 0x6967_616d_5047_5044;
/// C-full 奇数子停泊判据：process_start **bootstrap 非 0**——句柄迁移
/// （syscall.rs 3-4a：非 0 = 调用者表 take 迁入子表，子 x0 = 子表新句柄
/// 值，gen≥1 恒 ≥0x1_0000 **非 0 保证**；0 = 现形首触路径）。main 停泊 =
/// fault 面移交 svcmgr 侧 thread_start(spread=true) 线程——奇偶同像 x0
/// 分流，零第二入口零链接器面。停泊令载体 = 迁入的 Event 句柄（本像
/// 不解引用，子 exit 收表即释——首光实证：magic 值 arg 被 take 当句柄
/// 拒 -EBADF，裸 arg 落 x0 是 thread_start 语义、process_start 是迁移）。
const DPGP_PARK_NS: u64 = 1_000_000_000;

/// G4-3 D/E 刀 bench 子标签——**thread_start 裸 arg 通道**入（process_start
/// 第 5 参 = 句柄迁移不可传裸值，首光判例）。两值恒 < 0x1_0000（句柄
/// gen≥1 下界）与停泊判据（x0 ≥ 0x1_0000）零交叠，且 ≠ 0（现形）：
/// B3_ALL = 触遍区域全 32 页（D 延迟逐页样本）；B3_8 = 只触前 8 页
///（E 足迹 demand/eager 同形对照触面）。
const DPGP_BENCH_ALL: usize = 0xB301;
const DPGP_BENCH_8: usize = 0xB302;
/// bench 区域/结果页 VA 与页数（svcmgr `dpg_bench`/`dpg_foot` 同值单源
/// ——两文件各持 const 改须双改；0x20_0000/0x30_0000 = 像窗/栈/demand
/// 首页〔<0x7000〕界外，两段互不邻接）。
const DPGP_BENCH_VA: usize = 0x20_0000;
const DPGP_BENCH_PAGES: usize = 32;
const DPGP_RES_VA: usize = 0x30_0000;
/// 结果页布局：[0..n) = 逐页首触延迟 ns；[511] = done 哨（magic——E 相
/// svcmgr 轮询此位收「触面毕」，D 相走 exit join 不经它）。
const DPGP_RES_DONE: usize = 511;

/// bench 相：逐页首触环夹 t0=clock → volatile 读 → t1=clock，延迟样本
/// 写结果页（经共享 VMO 回传——零句柄零 console 零 ABI 新面）；值 ≠
/// magic = exit 9（供给链正确性随测）。B3_ALL（D 相）：写毕 exit 7——
/// 父 join 收割走 `DPGP ok` 同码面。B3_8（E 相）：写毕打 done 哨后
/// **停泊不退**——父持 rv/dv/ev 柄至 E 臂终局 = 记账面稳定（v1 无
/// Vmar 页表回收〔vmar.rs:11〕，页属 VMO 对象）；停泊由父 terminate
/// 收口（park 同形 sleep 帽循环）。
fn bench(tag: usize) -> ! {
    let n = if tag == DPGP_BENCH_8 { 8 } else { DPGP_BENCH_PAGES };
    let res = DPGP_RES_VA as *mut u64;
    for i in 0..n {
        let mut t0 = 0u64;
        if berry_clock_read(CLOCK_MONOTONIC, &mut t0) != 0 {
            berry_process_exit(9);
        }
        // SAFETY: 读本 AS bench 区域页——demand 映射 = 供给前无翻译，本读
        // 即首触断言；volatile = 延迟测量的被测对象本体（防重排/折叠）。
        let v = unsafe { core::ptr::read_volatile((DPGP_BENCH_VA + i * 4096) as *const u64) };
        let mut t1 = 0u64;
        if berry_clock_read(CLOCK_MONOTONIC, &mut t1) != 0 {
            berry_process_exit(9);
        }
        // SAFETY: 结果页父预映 RW——本相唯一出口，页内偏移恒 < 512。
        unsafe { *res.add(i) = t1.wrapping_sub(t0) };
        if v != DPGP_MAGIC {
            berry_process_exit(9);
        }
    }
    if tag == DPGP_BENCH_8 {
        // SAFETY: done 哨位 = 页内第 511 槽（4096B 页内恒在界）。
        unsafe { *res.add(DPGP_RES_DONE) = DPGP_MAGIC };
        loop {
            let mut now = 0u64;
            if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
                berry_process_exit(9);
            }
            berry_thread_sleep(now + DPGP_PARK_NS);
        }
    }
    berry_process_exit(7);
}

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(x0: usize) -> ! {
    // G4-3 D/E bench 相优先分流（两标签 < 0x1_0000 = 句柄域外，先于停泊
    // 判据零误吞）。
    if x0 == DPGP_BENCH_ALL || x0 == DPGP_BENCH_8 {
        bench(x0);
    }
    // C-full 奇数子：main 停泊（bootstrap 事件句柄在场 = 停泊令；fault 面
    // 在 svcmgr 侧 spread 线程——本线程只让路：sleep 帽循环不烧 TCG 墙钟，
    // pager 轮供与 spread 首触独享；spread 线程 process_exit → kill_all
    // 收割本睡者）。钟读败 = exit 9 fail loud（停泊变自旋才是污染面）。
    if x0 != 0 {
        loop {
            let mut now = 0u64;
            if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
                berry_process_exit(9);
            }
            berry_thread_sleep(now + DPGP_PARK_NS);
        }
    }
    // SAFETY: 读本 AS demand 页（供给前无翻译——本读即断言「EL0 缺页 →
    // 探针 → 0x21 供给 → eret 重试」全链；错值非安全问题是正确性问题）。
    let v = unsafe { core::ptr::read_volatile(DPGP_VA as *const u64) };
    // 读到即闭环（供给页内容 = 源页逐字节拷贝）；7/9 与 thrchild 同码面。
    berry_process_exit(if v == DPGP_MAGIC { 7 } else { 9 });
}

/// G4-4 X-2 **I-abort 腿第二入口**：`dpgi_entry` 段经 link-user.ld 定
/// **0x2000**（svcmgr `DPGI_ENTRY` 同值双改）——专供 dpg_selftest 拉起；
/// `_start` 主径零漂移 = storm/negative/soak/bench 全消费面行为不变
///（共享嵌内 bin = 共享行为面——G4-3 判例重申：新腿走新入口不走主径）。
/// 双腿序：①读 magic（0x24 D 腿）→ ②call 0x7000（0x20 I 腿：取指
/// fault → 探针同臂武装 → 父二供 → eret 重执行 call → 手排
/// `movz w0,#0x5a; ret` 返 0x5a）。三态：两腿全过 7 / magic 错 9 /
/// I 腿错 11（父降级行承载）。
#[link_section = "dpgi_entry"]
#[no_mangle]
pub extern "C" fn dpgi_start(x0: usize) -> ! {
    // 零句柄形态专用（bootstrap=0 → x0=0）：无 bench 标签/停泊分流面。
    let _ = x0;
    // ① D 腿（与 _start 主径同形）：SAFETY 注同上——断言全链，非安全问题。
    let v = unsafe { core::ptr::read_volatile(DPGP_VA as *const u64) };
    if v != DPGP_MAGIC {
        berry_process_exit(9);
    }
    // ② I 腿：SAFETY: 调本 AS demand 代码页（映射 RW 同像窗——v1 无
    // W^X 取指合法；供给前无翻译 = 本取指即 I-abort(0x20) 断言；错值非
    // 安全问题是正确性问题）。transmute 整数 → fn 指针 = 本受验链本体
    //（页内代码由 pager 供给，非静态可证）。
    let f: extern "C" fn() -> u32 = unsafe { core::mem::transmute(DPGP_VA2) };
    let r = f();
    berry_process_exit(if r == DPGP_IFN_RET { 7 } else { 11 });
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    // 裸 bin 无诊断面：错码 9 = 通用失败（父侧降级行承载）。
    berry_process_exit(9);
}
