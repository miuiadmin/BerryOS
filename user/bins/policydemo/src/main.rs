//! S6-b 消费面 demo——`berry_policy_bind`（0x83 COMMIT，S6-b 最小版）首个
//! 用户态消费者：设 1 条策略表（weight=+32）→ 验证世代协议行为变化。
//!
//! 标记：`POLICYDEMO ok`（成功）/ `POLICYDEMO FAIL <面>`（失败），退出码
//! 0/1（hello 同链 rootfs ELF：入口 x0 = env 通道端，console 经 env 块
//! gift[0] 迁入本表——能力随消息流动，非全局命名）。
//!
//! # 策略键（key）从哪来——S6-b 的关键缺口
//!
//! v1 策略键 = `Task::proc_tag`（`sched/src/policy.rs`：`Entry.task_key`
//! 命中面，`apply()`/`weight_of()` 以 `task.proc_tag as u64` 查表）。而
//! `proc_tag` 是**内核态指针**——`spawn_user`/`spawn_with_tag` 注入的进程
//! `HandleTable` leak 常驻地址（`kernel/src/syscall.rs::caller_table` 同源
//! 派发），刻意对用户态不透明。现有接口**无查询面**：
//! - berry-sys v1 36 号全表无「查自身 tag/策略键」的 syscall；
//! - `sched_getattr` 只回 `SchedAttr`（weight/policy/intent/deadline…，
//!   无 tag 字段）；
//! - policy 的 STATUS op 属 S6-c（BIND/STATUS/PROMOTE/DEMOTE/UNBIND）未落地。
//!
//! 故本 demo 按约定用**固定 key = 1**（`FIXED_KEY`）：验证的是机制面——
//! COMMIT 收表路径（copy_from_user → 逐条转换 → 夹紧 ±64 → `bind_gen`
//! 世代协议）全成立；「pick 命中面」（`apply` 写 `prio_effective` 的调度
//! 行为变化）需真实 proc_tag 作键，本 demo 不覆盖，留待 S6-c 键域查询面。
//!
//! # 行为验证（无直接查询面 → 用 bind 返回值）
//!
//! `bind_gen` 的世代单调协议是可观察的：**首次 COMMIT(gen) 受理后活跃
//! 世代 ≥ gen，同 gen 重放必 -EAGAIN（陈旧拒收）**。据此两步验证：
//! ① `gen` 从 1 起（全新系统活跃世代 0；若前序 binder 已推高则 -EAGAIN
//!   递增重试——换表协议文档的「携新 gen 重试」路径本身也是受验面）；
//! ② 受理后同 gen 重放，断言恰 -EAGAIN（重放返 0 = 世代协议失守 = FAIL）。

#![no_std]
#![no_main]

use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_policy_bind, berry_process_exit, Handle,
    PolicyEntry, POLICY_OP_COMMIT, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
};

/// 固定策略键（真实键 = 自身 `proc_tag`，用户态无查询面——见模块注）。
/// 非 0 即可过哨兵键检查；本 demo 只验机制面，不指望 pick 命中。
const FIXED_KEY: u64 = 1;

/// gen 重试帽：全新系统 gen=1 首发即中；帽内全程 -EAGAIN = 前序 binder
/// 世代异常高（病态面，loud 失败交标记）。
const GEN_RETRY_CAP: u64 = 8;

/// smoke 标记（字节串字面量直传 + `.len()`——零手排切片，无宽长错位面）。
const OK: &[u8] = b"POLICYDEMO ok\n";
const FAIL_BIND: &[u8] = b"POLICYDEMO FAIL bind\n";
const FAIL_GEN: &[u8] = b"POLICYDEMO FAIL gen\n";

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

    // ---- 1. 构造 1 条策略表（给自己 +32 权重；键 = 固定 1，见模块注）----
    // `..Default::default()`：placement/epp_flags/_rsv 全零——未定义
    // epp_flags 位或 _rsv 非 0 会被内核 -EINVAL（options 纪律），零值恰是
    // 唯一合法「无意见」形态。weight=32 在包络 ±64 内（内核 build/apply
    // 双点再夹——包络是内核执法，不信任注入方自律）。
    let entry = PolicyEntry {
        key: FIXED_KEY,
        weight: 32,
        ..Default::default()
    };

    // ---- 2. COMMIT + gen 重试（陈旧 -EAGAIN → 携新 gen 重试）----
    let mut gen: u64 = 1;
    let committed = loop {
        let st = berry_policy_bind(0, POLICY_OP_COMMIT, &entry, 1, gen, 0);
        if st == 0 {
            break gen; // 受理：活跃快照已换，世代 = gen
        }
        if st != BERRY_ERR_AGAIN || gen >= GEN_RETRY_CAP {
            putline(console, FAIL_BIND);
            berry_process_exit(1);
        }
        gen += 1; // 陈旧拒收：对账后携新 gen 重试（换表协议正路径）
    };

    // ---- 3. 验证行为变化：同 gen 重放必 -EAGAIN ----
    // 首次受理已把活跃世代推到 ≥ committed；重放旧表不得覆盖新表
    //（写者重放/乱序到达防线）。返 0 = 世代单调性失守；其他错码 = 协议
    // 外路径，同样 loud。
    let replay = berry_policy_bind(0, POLICY_OP_COMMIT, &entry, 1, committed, 0);
    if replay != BERRY_ERR_AGAIN {
        putline(console, FAIL_GEN);
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
