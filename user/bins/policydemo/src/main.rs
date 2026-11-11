//! S6 消费面 demo——`berry_policy_bind`（0x83）首个用户态消费者：STATUS
//! 对账自身策略键 → 以**真实键**设 1 条策略表（weight=+32）→ 验证世代
//! 协议行为变化。
//!
//! 标记：`POLICYDEMO ok`（成功）/ `POLICYDEMO FAIL <面>`（失败），退出码
//! 0/1（hello 同链 rootfs ELF：入口 x0 = env 通道端，console 经 env 块
//! gift[0] 迁入本表——能力随消息流动，非全局命名）。
//!
//! # 策略键（key）从哪来——STATUS（op=2）对账
//!
//! v1 策略键 = `Task::proc_tag`（`sched/src/policy.rs`：`Entry.task_key`
//! 命中面，`apply()`/`weight_of()` 以 `task.proc_tag as u64` 查表）。而
//! `proc_tag` 是**内核态指针**——`spawn_user`/`spawn_with_tag` 注入的进程
//! `HandleTable` leak 常驻地址，刻意对用户态不透明、不可合成。查询面 =
//! 0x83 op=2 STATUS（S6-e 落地）：`berry_policy_status(&mut packed,
//! &mut self_key)` 的 p2 出参——内核经 `current_tag` 与 apply/weight_of
//! 消费点**同源**读取。真实用户进程的 tag 恒非 0（哨兵键 0 留给内核任务
//! /boot 期——表侧本就丢弃 0 键条目）；拿到 0 = 身份异常或查询面失守，
//! 本 demo loud 拒走。S6-b 期的 `FIXED_KEY=1` 临时形态就此退役——pick
//! 命中面（`apply` 写 `prio_effective` 的调度行为变化）自此真实指向本
//! 进程，不再只是机制面验证。
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
    berry_channel_recv, berry_channel_send, berry_policy_bind, berry_policy_status,
    berry_process_exit, Handle, PolicyEntry, POLICY_OP_COMMIT, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
};

/// gen 重试帽：全新系统 gen=1 首发即中；帽内全程 -EAGAIN = 前序 binder
/// 世代异常高（病态面，loud 失败交标记）。
const GEN_RETRY_CAP: u64 = 8;

/// smoke 标记（字节串字面量直传 + `.len()`——零手排切片，无宽长错位面）。
const OK: &[u8] = b"POLICYDEMO ok\n";
const FAIL_KEY: &[u8] = b"POLICYDEMO FAIL key\n";
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

    // ---- 1. STATUS（op=2）对账自身策略键——真实键替换 FIXED_KEY ----
    // `packed` 顺带回双谱系世代（active@bit0-31 / shadow@bit32-63，单读窗
    // 无撕裂）；本 demo 的 gen 起点仍按「全新系统 gen=1」盲走重试协议
    //（下一步），不依赖该读数。self_key == 0 = 哨兵键（内核任务/boot 期
    // 形态）——真实用户进程的 proc_tag 恒非 0（spawn 注入的 HandleTable
    // leak 常驻地址），且 0 键条目表侧本就丢弃（pick 命中不可能成立），
    // 查询面失守或身份异常都 loud 拒走。
    let mut packed: u64 = 0;
    let mut self_key: u64 = 0;
    if berry_policy_status(&mut packed, &mut self_key) != 0 || self_key == 0 {
        putline(console, FAIL_KEY);
        berry_process_exit(1);
    }

    // ---- 2. 构造 1 条策略表（以对账到的真实键给自己 +32 权重）----
    // `..Default::default()`：placement/epp_flags/_rsv 全零——未定义
    // epp_flags 位或 _rsv 非 0 会被内核 -EINVAL（options 纪律），零值恰是
    // 唯一合法「无意见」形态。weight=32 在包络 ±64 内（内核 build/apply
    // 双点再夹——包络是内核执法，不信任注入方自律）。键 = self_key：
    // `apply` 写 `prio_effective` 的命中面自此真实指向本进程。
    let entry = PolicyEntry {
        key: self_key,
        weight: 32,
        ..Default::default()
    };

    // ---- 3. COMMIT + gen 重试（陈旧 -EAGAIN → 携新 gen 重试）----
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

    // ---- 4. 验证行为变化：同 gen 重放必 -EAGAIN ----
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
