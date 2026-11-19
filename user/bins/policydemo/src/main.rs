//! S6 消费面 demo——`berry_policy_bind`（0x83）用户态消费者。
//!
//! 标记：`POLICYDEMO ok`（双拒恰 -EBADF——门在位）/ `POLICYDEMO FAIL status`
//! / `POLICYDEMO FAIL bind`（任一调用返回值 ≠ -EBADF），退出码 0/1（hello
//! 同链 rootfs ELF：入口 x0 = env 通道端，console 经 env 块 gift[0] 迁入本表
//! ——能力随消息流动，非全局命名）。
//!
//! # arch-r2 起 0x83 全 op 面挂 MANAGE 能力门——demo 形态就地切换
//!
//! 安全轮 High 缺口收口（`sys_policy_bind` 首检）：`auth` 从「预留不消费」
//! 改为 **Process handle + RIGHT_MANAGE 三门控**（在表/类型/权利），门在 op
//! 分派**之前**——策略注入是全局旁路（COMMIT/PROMOTE 可给任意 proc_tag 加
//! 权 ±64 自肥抢占、UNBIND 可拆其他服务的策略），按「机制在内核、策略注入
//! 须过能力门控」纲领，只有持某 Process MANAGE 权的调用者可操作全局策略表。
//! 本 demo 持有的唯一能力是 env 迁入的 console 通道端点（Channel 类型）——
//! v1 无授予方递 Process handle（spawn 链不赠权），因此：
//!
//! - **STATUS（op=2，读面）**：auth=0 裸调须恰 **-EBADF**（坏 handle）——读
//!   面也在门后，本 demo 旧路径「STATUS 对账自身策略键」（S6-f）随之退役；
//! - **COMMIT（op=1，写面）**：auth=0 裸调同拒 -EBADF——注入面对无权进程
//!   关闭。两拒恰 -EBADF = 门在位的用户态活证（门失守或门形态漂移都 loud）。
//!
//! 正向注入链（真实键对账 + gen 重试协议 + 同 gen 重放 -EAGAIN 世代断言）
//! 的内核侧底座由 ktest PBS-K/SHD-K 承担；用户态正向链待 spawn 链授
//! Process MANAGE handle（S6-c Job 对象落地）后回归本 demo。
//!
//! 注：内核侧同判据的 EL1 面钉板 = ktest test_syscall H8（auth=0 / 类型不
//! 符 / 缺 MANAGE 三向 + gen 不动）——本 demo 是其 EL0 侧姊妹面。

#![no_std]
#![no_main]

use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_policy_bind, berry_policy_status,
    berry_process_exit, Handle, PolicyEntry, BERRY_ERR_BAD_HANDLE, BERRY_ERR_BUSY,
    POLICY_OP_COMMIT,
};

/// smoke 标记（字节串字面量直传 + `.len()`——零手排切片，无宽长错位面）。
const OK: &[u8] = b"POLICYDEMO ok\n";
const FAIL_STATUS: &[u8] = b"POLICYDEMO FAIL status\n";
const FAIL_BIND: &[u8] = b"POLICYDEMO FAIL bind\n";

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env: Handle) -> ! {
    // ---- console 经 env 块 gift[0] 迁入（hello 同款）----
    let mut blob = [0u8; 64];
    let mut nb = blob.len();
    let mut hs = [0 as Handle; 6];
    let mut nh = hs.len();
    if berry_channel_recv(env, blob.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh) != 0
        || nh < 1
        || hs[0] == 0
    {
        trap(); // env 块畸形/无 console：标记打不出 = smoke FAIL 兜底
    }
    let console = hs[0];

    // ---- 1. STATUS（op=2，读面）auth=0 → 须恰 -EBADF ----
    // 门在 op 分派之前：读面同拒（出参 packed/self_key 不被写——返回非
    // -EBADF 即门失守（返 0 = 门缺席）或门形态漂移，均 loud）。
    let mut packed: u64 = 0;
    let mut self_key: u64 = 0;
    if berry_policy_status(0, &mut packed, &mut self_key) != BERRY_ERR_BAD_HANDLE {
        putline(console, FAIL_STATUS);
        berry_process_exit(1);
    }

    // ---- 2. COMMIT（op=1，写面）auth=0 → 须恰 -EBADF ----
    // 表内容任意（门先于表解析——p1 在门后才被解引用），沿用 1 条 weight=32
    // 条目的原注入形态：本断言验证的是注入面已关，不是表语义。
    let entry = PolicyEntry {
        key: 1,
        weight: 32,
        ..Default::default()
    };
    if berry_policy_bind(0, POLICY_OP_COMMIT, &entry, 1, 1, 0) != BERRY_ERR_BAD_HANDLE {
        putline(console, FAIL_BIND);
        berry_process_exit(1);
    }

    putline(console, OK);
    berry_process_exit(0);
}

/// 打一行到 console（自旋重发：满 -EBUSY 环 = 重试，其余错码自陷——
/// 启动期饱和窗口内阻塞者会被自旋友商抢槽饿到超时，B13 契约）。
fn putline(console: Handle, s: &[u8]) {
    loop {
        let st = berry_channel_send(console, s.as_ptr(), s.len(), core::ptr::null(), 0);
        if st == 0 {
            return;
        }
        if st != BERRY_ERR_BUSY {
            trap(); // 链断：标记缺失 = smoke FAIL 兜底
        }
    }
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己。
#[cold]
fn trap() -> ! {
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    trap()
}
