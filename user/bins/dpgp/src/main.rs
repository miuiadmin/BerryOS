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
//! 双序天然安全：子先 fault 后 0x21 = 槽武装等收（poll-first 直取）；
//! 0x21 先睡子后 fault = 探针唤醒收货方——无丢唤窗（KL 窗查位+入队）。
#![no_std]
#![no_main]

use berry_sys::{berry_clock_read, berry_process_exit, berry_thread_sleep, CLOCK_MONOTONIC};

/// demand 页 VA（svcmgr `dpg_selftest` 同值单源——两文件各持 const，
/// 改须双改；0x6000 = 像窗 [0x1000,0x6000) 界外首页，读它必走缺页路径）。
const DPGP_VA: usize = 0x6000;

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

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(x0: usize) -> ! {
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

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    // 裸 bin 无诊断面：错码 9 = 通用失败（父侧降级行承载）。
    berry_process_exit(9);
}
