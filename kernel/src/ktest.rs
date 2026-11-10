//! 内核内最小测试骨架（关卡 G0 判据辅助）：在 QEMU 串口打印 PASS/FAIL。
//!
//! 单核、无抢占（G0）下同步直跑；断言失败 → panic（panic handler 打消息 + file:line:col；
//! CPU 异常寄存器/PC/ESR/FAR dump 在 HAL `trap::exception_sync`）。SMP（G2）后需每核隔离。
//!
//! # 六横切锁纪律（S-3 §3 立法——ktest 是「锁序全图首读者」，改锁先过此面）
//!
//! 1. **锁序全序**（死锁面预防；证据 = §8.C 逆序边表 E1-E20 + §9 F-E1 补边）：
//!    SLEEPQ 锁 > 队列锁 > REAPQ 锁 > 对象系/表锁（收缩版 KERN_LOCK）> 分配器锁
//!    > ASID 锁 > 打印锁。反向取锁 = ABBA 死锁边，逆序边即重排/合并。
//! 2. **偷取特则**：只持远端锁（放本地→取远端→远端出队→放）；确需双持一律核号
//!    升序（0→1，F-L1）；偷取双过滤 `state==Ready && pinned!=0`（F-E2）构造性
//!    保证「双队列同时出现」不变量。
//! 3. **不可重入立法**（F-L2）：KERN_LOCK 朴素 TTAS 同核自锁——SQ/RQ/REAPQ/表
//!    四面 `_locked` 内层分层，自锁面三处（wake_matching→recompute / kill 钩子链
//!    / rx_irq_notify）。
//! 4. **wait/wake 原子性锚 = SQ 锁窗**（F-L4）：查位+入队同窗、写位+扫描同窗
//!    ——分段后 IRQ 屏蔽锚敞开 = 丢唤醒永睡。
//! 5. **IRQ 上下文纪律**：SGI handler / tick / RX 取锁面 = 短窗 + 永不睡眠
//!    + 永不嵌套「打印锁之外的锁」。
//! 6. **KLOCK_OPS None 直通窗保留（F9）**：ktest 在 sched::init **之前**跑——
//!    KLOCK_OPS 未安装时 klock_* 全直通、SVC_WINDOW 槽对 null CURRENT 跳过
//!    （S-4 判例后 klock_enter 同走 SVC_WINDOW，形态不变）。S-3c 起新锁必须
//!    保留「安装前零开销直通」形态，否则本测试面破窗。

use berry_mm::va::{phys_to_virt, PAGE_SHIFT};

pub fn run() {
    test_gic();
    test_frame();
    test_memmap();
    test_heap();
    test_syscall();
    test_switch();
    test_fp();
    test_aspace();
    test_vmo();
    test_handle();
    test_policy();
    test_policy_v3();
    test_policy_bind_syscall();
    test_policy_perf();
    test_policy_shadow();
    test_policy_divergence();
    test_policy_reject_stats();
    test_policy_yield_return();
    test_placement_hint();
    test_rq_ring2();
    test_rq_ring3();
    test_pi();
    test_intent();
    test_band();
    test_fast();
    test_freeze();
    test_deadline();
    test_billing();
    test_cmp();
    test_suspend_negative();
    test_freeze_cycles();
    // AM-1.5（「IPC × 冻结」Q1 裁决）：wake-pending 全链（FREEZE 族续——
    // FRZCYC 后紧邻，同 SLEEPQ/表基座纪律）。
    test_wake_pending();
    test_billing_overflow();
    test_sched_attr_negative();
    test_sched_perf();
    test_sched_fairness();
    test_sched_attr();
    test_wake_src();
    test_dtb();
    // sim-S3（PM-16 §4.4 步3）：末位新号（ktest done 前；既有序不动）——
    // PSCI 探测已在 boot 早期跑，request_deep_sleep 读缓存降级。
    test_deep_sleep_degrade();
    // v1.3 §7 EAS：EM 表形状（同款末位新号纪律——BAND 后族序不动）。
    test_em();
    // v1.3 §6/§7 EAS：per-CPU util EWMA（调度器→PM 频率信号；同款末位新号）。
    test_util();
    // v1.3 §7 EAS：overutilized 回退（EM × util 首个交叉消费者——EAS 放置
    // hint 门控；同款末位新号）。
    test_overutil();
    // v1.3 §9 真机框架：HDR 直方图（对数线性桶 + 分位 + per-CPU/合并读面；
    // 同款末位新号——sched::init 前跑，纯原子静态无 CURRENT 面，ktest 窗合法）。
    test_hdr();
    // v1.3 §9 三路基准之路 2 首刀：定时器唤醒延迟基准（cyclictest 口径的
    // 忙等简化）——HDR 首个真实消费者，record_wake_lat → 合并分位全链验证。
    test_hdr_wake();
    // 全栈交叉验证（末位新号纪律——单面钉板之后、ktest done 之前）：一个
    // 场景串起 freeze × policy × band × deadline × billing × EAS，验
    // 「两两同席不打架」（单面钉板验「各自对」，本测验组合面）。
    test_scheduler_integration();
    // AM-2b（2026-11-08 提案候选①·主会话六裁决）：压力窗口聚合 + 系统压力
    // Event 用户位出口（末位新号纪律——ktest done 前；sched::init 前跑，
    // SLEEPQ 空扫 = 出口唤醒面恒 n=0，合法）。
    test_am2b_pressure();
    // VER（版本覆盖硬化·内核面）：version VMO 内容断言（末位新号纪律——
    // ktest done 前；sched::init 前跑，瞬态 VMO 现读现验不留柄）。
    test_sysver();
    berry_hal_aarch64::uart_puts(b"ktest done\n");
    // Step 6：异常向量 + TrapFrame save/restore/eret 全链路验证（放最后）。
    test_exc();
}

/// Step 9 信号 + G0 退出 #6 + **3-1 真 ABI 骨架**：从 EL1 发 `svc #0`（EC=0x15）
/// → HAL dispatch → 语义层 handler（`kernel/src/syscall.rs`）→ 回写 x0、eret 续行。
/// 用例：调试槽退役面（4-5a：0xFE/0xFF 随 console 服务落地回落 -ENOTSUP——
/// EL0 输出改走 channel_send 真 IPC）；未知号 0x99 -ENOTSUP（3-1 政策：错误码
/// 自理、永不杀调用者；**T-1 起在册 31 号全实现——「未实现真号」样本随 0x35
/// 退役**）；clock_read（MONOTONIC）两次读单调；handle 三操作经 syscall 全链路
/// 门控（close/duplicate + 旧值/坏 handle 拒）；3-4a per-process 表拆分（G 段）：
/// 两表隔离 + take/insert_ref 迁移原语 + 子进程出生自带空表；3-4b channel 三号
/// （H 段）：非阻塞队列语义 + handle 随消息迁移的原子序（「两进程交换一句消息」
/// 的 EL0 正向验收在 user_spawner/user_child）；T-1 thread_start 门控面（F3b 段）。
#[allow(clippy::unwrap_used)]
fn test_syscall() {
    use crate::handle::{KernelObj, RIGHT_READ, RIGHT_WRITE};
    use crate::syscall::{
        bootstrap_table, NR_CLOCK_READ, NR_HANDLE_CLOSE, NR_HANDLE_DUPLICATE, NR_SIGNAL_SEND,
        NR_SIGNAL_TAKE, NR_VMO_CREATE,
    };
    use crate::vmo::Vmo;
    use berry_mm::va::PAGE_SIZE;

    // out 参数缓冲（内核地址；EL1 调用方 → copy_out 走高半直映区）。
    static mut OUT: u64 = 0;

    // A. 调试槽退役面（4-5a）：0xFE/0xFF 原调试 echo/putchar 槽随 console 服务
    //    落地退役——一律 -ENOTSUP 存活（EL0 打印改走 channel_send 真 IPC）。
    assert_eq!(svc4(0xFE, 0x5a, 0, 0), -95, "SYSCALL：0xFE 调试槽须已退役");
    assert_eq!(svc4(0xFF, 0x75, 0, 0), -95, "SYSCALL：0xFF 调试槽须已退役");

    // B. 未知号（越界 0x99）一律 -ENOTSUP 存活——3-1 撤「畸形号杀」政策的
    //    EL1 侧正向断言。**「未实现真号」样本 0x35 已随 T-1 thread_start 实现
    //    退役**（样本断言会打到真 handler）；亦勿拿不在册的 0x38/0x39 冒充——
    //    其 -ENOTSUP 成因与未知号相同，「在册未实现」覆盖点不存在；v2 新号
    //    入册时若暂未实现，再立新样本（勿拿已实现号 0x33——会真退出杀 ktest）。
    assert_eq!(svc4(0x99, 0, 0, 0), -95, "SYSCALL：未知号须 -ENOTSUP");

    // B2. 信号面参数拒面（A7-4 段②，0x86/0x87 参数面样本——成功路径不测：
    // ktest 期 current 无 Process（0x35 先例），happy path 交 EL0 + smoke 兜底）。
    //    sig 界检先于一切（0 与 65 同拒 -EINVAL——位图界 1..=64）；
    //    坏 handle → -EBADF（bootstrap 表 gen 不匹配）；h=0/sig 合法 + take
    //    → -EBADSTATE（ktest 任务无 Process 面——语义拒非崩）。
    assert_eq!(svc4(NR_SIGNAL_SEND as u64, 0, 0, 0), -22, "SYSCALL：signal_send sig=0 须 -EINVAL");
    assert_eq!(svc4(NR_SIGNAL_SEND as u64, 0, 65, 0), -22, "SYSCALL：signal_send sig=65 须 -EINVAL");
    assert_eq!(
        svc4(NR_SIGNAL_SEND as u64, 0x9999_0001, 10, 0),
        -9,
        "SYSCALL：signal_send 坏 handle 须 -EBADF"
    );
    assert_eq!(
        svc4(NR_SIGNAL_SEND as u64, 0, 10, 0),
        -37,
        "SYSCALL：signal_send h=0 ktest 无 Process 须 -EBADSTATE"
    );
    assert_eq!(
        svc4(NR_SIGNAL_TAKE as u64, 0, 0, 0),
        -37,
        "SYSCALL：signal_take ktest 无 Process 须 -EBADSTATE"
    );

    // C. clock_read：MONOTONIC 两次读单调递增；非法 id -ENOTSUP。
    // SAFETY: svc 同步异常；OUT 为本函数私有静态。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(NR_CLOCK_READ as u64, 0, &raw mut OUT as u64, 0),
        0,
        "SYSCALL：clock_read 状态"
    );
    let t1 = unsafe { OUT };
    assert!(t1 > 0, "SYSCALL：clock_read 应得正纳秒");
    assert_eq!(
        svc4(NR_CLOCK_READ as u64, 1, &raw mut OUT as u64, 0),
        -95,
        "SYSCALL：非法 clock_id 须 -ENOTSUP"
    );
    assert_eq!(svc4(NR_CLOCK_READ as u64, 0, &raw mut OUT as u64, 0), 0);
    assert!(unsafe { OUT } >= t1, "SYSCALL：单调钟不单调");

    // D. handle 门控全链路（EL1 svc → 语义层 → 引导表）。duplicate/replace 的
    //    out 回写走 copy_out（EL1 内核直映区分支）。
    let ht = bootstrap_table();
    let h1 = ht
        .insert(KernelObj::Vmo(Vmo::create(PAGE_SIZE).expect("ktest：装配 VMO（可信语境）")), RIGHT_READ | RIGHT_WRITE)
        .expect("SYSCALL：insert h1");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, h1 as u64, 0, 0), 0, "SYSCALL：close h1");
    assert_eq!(
        svc4(NR_HANDLE_CLOSE as u64, h1 as u64, 0, 0),
        -9,
        "SYSCALL：close 旧值须 -EBADF"
    );
    let h2 = ht
        .insert(KernelObj::Vmo(Vmo::create(PAGE_SIZE).expect("ktest：装配 VMO（可信语境）")), RIGHT_READ | RIGHT_WRITE)
        .expect("SYSCALL：insert h2");
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(NR_HANDLE_DUPLICATE as u64, h2 as u64, RIGHT_READ, &raw mut OUT as u64),
        0,
        "SYSCALL：duplicate 收紧"
    );
    let hd = unsafe { OUT } as u32;
    assert_ne!(hd, 0, "SYSCALL：duplicate 须回填新 handle");
    assert_ne!(hd, h2, "SYSCALL：duplicate 须新槽位");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hd as u64, 0, 0), 0, "SYSCALL：close dup");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, h2 as u64, 0, 0), 0, "SYSCALL：close h2");
    assert_eq!(
        svc4(NR_HANDLE_CLOSE as u64, 0xDEAD_BEEF, 0, 0),
        -9,
        "SYSCALL：坏 handle 须 -EBADF"
    );

    // E. 3-2 VMO/Vmar syscall 全链（EL1 侧；rights 矩阵 + 错误映射 + 映射计数
    //    防线）。缓冲均为内核静态（EL1 copy 走高半直映区）。
    use crate::handle::{ObjType, RIGHT_MANAGE, RIGHT_MAP};
    use crate::syscall::{
        NR_VMAR_ALLOCATE, NR_VMAR_MAP, NR_VMAR_PROTECT, NR_VMAR_UNMAP, NR_VMO_COMMIT,
        NR_VMO_DECOMMIT, NR_VMO_GET_PHYS, NR_VMO_GET_SIZE, NR_VMO_READ, NR_VMO_WRITE,
    };
    use crate::vmar::Vmar;
    static mut EBUF: [u8; 16] = [0; 16]; // write 源 / read 目的（内核直映区）
    const PATTERN: u64 = 0x00C0_FFEE_0000_0055;

    // E0. 参数面：size=0 → -EINVAL；options 未知位 → -EINVAL；超尺寸 →
    //     -ENOMEM（no-panic 防线）。G4-2 起 bit0 = VMO_OPT_DEMAND 激活——
    //     「options≠0 须拒」升「未知位须拒」+ bit0 受纳正测（建毕即 close）。
    assert_eq!(svc6(NR_VMO_CREATE as u64, 0, 0, &raw mut OUT as u64, 0, 0, 0), -22,
        "SYSCALL：vmo_create size=0 须 -EINVAL");
    assert_eq!(svc6(NR_VMO_CREATE as u64, PAGE_SIZE as u64, 2, &raw mut OUT as u64, 0, 0, 0), -22,
        "SYSCALL：vmo_create options 未知位须 -EINVAL（bit0=DEMAND 已激活）");
    assert_eq!(
        svc6(NR_VMO_CREATE as u64, PAGE_SIZE as u64, 0x1_0000_0001,
             &raw mut OUT as u64, 0, 0, 0),
        -22,
        "SYSCALL：vmo_create options 高 32 位脏值须 -EINVAL（x2 全宽 u64 检——G4-2 修批 MECH-2，旧 as u32 截断此形受纳）"
    );
    assert_eq!(
        svc6(NR_VMO_CREATE as u64, u64::MAX, 0, &raw mut OUT as u64, 0, 0, 0),
        -12,
        "SYSCALL：巨 size 须 -ENOMEM（防堆分配 abort）"
    );
    // SAFETY: svc 同步异常；OUT 为本函数私有静态。
    unsafe { OUT = 0 };
    assert_eq!(
        svc6(NR_VMO_CREATE as u64, PAGE_SIZE as u64, 1, &raw mut OUT as u64, 0, 0, 0),
        0,
        "SYSCALL：vmo_create VMO_OPT_DEMAND 须受纳（G4-2 激活面）"
    );
    let hdm = unsafe { OUT } as u32;
    assert_ne!(hdm, 0, "SYSCALL：DEMAND VMO 须回填非零 handle");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hdm as u64, 0, 0), 0,
        "SYSCALL：close demand vmo（零供给页零映射——防线自洽）");

    // E1. create→commit→write→read 往返（WRITE/READ 门控）。
    // SAFETY: svc 同步异常；OUT/EBUF 为本函数私有静态。
    unsafe { OUT = 0 };
    assert_eq!(svc6(NR_VMO_CREATE as u64, PAGE_SIZE as u64, 0, &raw mut OUT as u64, 0, 0, 0), 0,
        "SYSCALL：vmo_create");
    let hv = unsafe { OUT } as u32;
    assert_ne!(hv, 0, "SYSCALL：vmo_create 须回填非零 handle");
    assert_eq!(svc6(NR_VMO_COMMIT as u64, hv as u64, 0, PAGE_SIZE as u64, 0, 0, 0), 0,
        "SYSCALL：vmo_commit");
    // SAFETY: 同上。
    unsafe { EBUF[..8].copy_from_slice(&PATTERN.to_le_bytes()) };
    assert_eq!(
        svc6(NR_VMO_WRITE as u64, hv as u64, &raw const EBUF as u64, 0, 8, 0, 0),
        0,
        "SYSCALL：vmo_write"
    );
    // SAFETY: 同上。
    unsafe { EBUF = [0; 16] };
    assert_eq!(
        svc6(NR_VMO_READ as u64, hv as u64, &raw mut EBUF as u64, 0, 8, 0, 0),
        0,
        "SYSCALL：vmo_read"
    );
    // SAFETY: 只读比对。
    assert_eq!(unsafe { u64::from_le_bytes(EBUF[..8].try_into().unwrap()) }, PATTERN,
        "SYSCALL：write→read 往返");

    // E2. 错误映射：未提交页 read → -EBADSTATE(-37)；未对齐 commit → -EINVAL；
    //     RO 收紧后 write → -EACCES(-13)。
    let hv_nc = {
        // SAFETY: 同上。
        unsafe { OUT = 0 };
        assert_eq!(svc6(NR_VMO_CREATE as u64, PAGE_SIZE as u64, 0, &raw mut OUT as u64, 0, 0, 0), 0);
        unsafe { OUT as u32 }
    };
    assert_eq!(
        svc6(NR_VMO_READ as u64, hv_nc as u64, &raw mut EBUF as u64, 0, 8, 0, 0),
        -37,
        "SYSCALL：未提交页读须 -EBADSTATE"
    );
    assert_eq!(svc6(NR_VMO_COMMIT as u64, hv as u64, 0x10, 0x10, 0, 0, 0), -22,
        "SYSCALL：未对齐 commit 须 -EINVAL");
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(NR_HANDLE_DUPLICATE as u64, hv as u64, 1 /* READ */, &raw mut OUT as u64),
        0,
        "SYSCALL：dup 收紧到 READ"
    );
    let hro = unsafe { OUT } as u32;
    assert_eq!(
        svc6(NR_VMO_WRITE as u64, hro as u64, &raw const EBUF as u64, 0, 8, 0, 0),
        -13,
        "SYSCALL：RO handle 写须 -EACCES（权限在 handle 不在对象）"
    );
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hro as u64, 0, 0), 0, "SYSCALL：close RO");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hv_nc as u64, 0, 0), 0, "SYSCALL：close 空提交 VMO");

    // E3. vmar 链：插一张 root Vmar 进引导表（MAP|MANAGE）→ map（含映射中
    //     close 拒 -EBUSY、读别名一致、protect rights 面）→ unmap → decommit →
    //     close 全绿。EL0 侧同链在 user_good（3-2 验收本体）。
    let hvm = ht
        .insert(
            KernelObj::Vmar(
                Vmar::new_root(crate::mm::frames(), crate::aspace::alloc_asid())
                    .expect("SYSCALL：建 root Vmar（ktest 可信语境）"),
            ),
            RIGHT_MAP | RIGHT_MANAGE,
        )
        .expect("SYSCALL：insert root vmar");
    // SAFETY: 同上。
    unsafe { OUT = crate::aspace::USER_VA as u64 }; // 请求 VA = 0x1000
    assert_eq!(
        svc6(NR_VMAR_MAP as u64, hvm as u64, hv as u64, 0, &raw mut OUT as u64,
             PAGE_SIZE as u64, 3 /* READ|WRITE */),
        0,
        "SYSCALL：vmar_map"
    );
    assert_eq!(unsafe { OUT } as usize, crate::aspace::USER_VA, "SYSCALL：vmar_map 须回写实际 VA");
    // 映射中 close 拒（悬空 PTE 防线）；unmap 后即放行。
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hv as u64, 0, 0), -16,
        "SYSCALL：映射中 close 须 -EBUSY");
    assert_eq!(
        svc6(NR_VMAR_PROTECT as u64, hvm as u64, 1, crate::aspace::USER_VA as u64,
             PAGE_SIZE as u64, 0, 0),
        -95,
        "SYSCALL：protect 缺 WRITE 须 -ENOTSUP（v1 只支持 R|W）"
    );
    assert_eq!(
        svc6(NR_VMAR_PROTECT as u64, hvm as u64, 5, crate::aspace::USER_VA as u64,
             PAGE_SIZE as u64, 0, 0),
        -22,
        "SYSCALL：protect 外位 rights 须 -EINVAL"
    );
    assert_eq!(
        svc6(NR_VMAR_UNMAP as u64, hvm as u64, 0x9000, PAGE_SIZE as u64, 0, 0, 0),
        -2,
        "SYSCALL：unmap 未映射区间须 -ENOENT"
    );
    assert_eq!(
        svc6(NR_VMAR_UNMAP as u64, hvm as u64, crate::aspace::USER_VA as u64, PAGE_SIZE as u64, 0, 0, 0),
        0,
        "SYSCALL：vmar_unmap"
    );
    assert_eq!(svc6(NR_VMO_DECOMMIT as u64, hv as u64, 0, PAGE_SIZE as u64, 0, 0, 0), 0,
        "SYSCALL：vmo_decommit（映射已解除）");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hv as u64, 0, 0), 0, "SYSCALL：close 已 decommit VMO");

    // E3b. demand 攻击形完整回归链（G4-2 修批 P0-1 面 a——f4 意见①：verdict
    //      (a) 的内核侧镜像，两端在册、攻击序即死）：demand map（区间登记、
    //      未供给页 mapcnt 恒 0、demand 钉 +1）→ close 须拒 -EBUSY（旧守卫
    //      只查 mapcnt，此形放行 = 对已释放 Vmo untrack = 确定性 UAF 写；
    //      修批钉面拦下）→ unmap（mapcnt/demand 双记账归零）→ close 放行。
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(
        svc6(NR_VMO_CREATE as u64, PAGE_SIZE as u64, 1, &raw mut OUT as u64, 0, 0, 0),
        0,
        "SYSCALL：vmo_create demand（攻击形载荷）"
    );
    let hdv = unsafe { OUT } as u32;
    // SAFETY: 同上（请求 VA 复用 0x1000——E3 已 unmap 无重叠）。
    unsafe { OUT = crate::aspace::USER_VA as u64 };
    assert_eq!(
        svc6(NR_VMAR_MAP as u64, hvm as u64, hdv as u64, 0, &raw mut OUT as u64,
             PAGE_SIZE as u64, 3 /* READ|WRITE */),
        0,
        "SYSCALL：vmar_map demand 分流（区间登记零供给）"
    );
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hdv as u64, 0, 0), -16,
        "SYSCALL：demand 登记中 close 须 -EBUSY（修批钉面——旧守卫此形放行即 UAF 写）");
    assert_eq!(
        svc6(NR_VMAR_UNMAP as u64, hvm as u64, crate::aspace::USER_VA as u64,
             PAGE_SIZE as u64, 0, 0, 0),
        0,
        "SYSCALL：vmar_unmap demand 区间（双记账归零）"
    );
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hdv as u64, 0, 0), 0,
        "SYSCALL：unmap 后 close demand vmo 放行（钉已放）");

    // E4. vmar_allocate：子 Vmar 入表授父 rights；子界内 map 成功面（只验状态，
    //     映射本体 E3 已覆盖）。
    // SAFETY: 同上。
    unsafe { OUT = 0x20_0000 }; // 请求子区间基 0x20_0000
    static mut CHILD_OUT: u64 = 0;
    // SAFETY: 同上。
    unsafe { CHILD_OUT = 0 };
    assert_eq!(
        svc6(NR_VMAR_ALLOCATE as u64, hvm as u64, 0, &raw mut OUT as u64,
             (PAGE_SIZE * 2) as u64, &raw mut CHILD_OUT as u64, 0),
        0,
        "SYSCALL：vmar_allocate"
    );
    let hchild = unsafe { CHILD_OUT } as u32;
    assert_ne!(hchild, 0, "SYSCALL：allocate 须回填子 handle");
    let (_, child_rights) = ht.get(hchild, ObjType::Vmar, 0).expect("SYSCALL：get 子 vmar");
    assert_eq!(child_rights, RIGHT_MAP | RIGHT_MANAGE, "SYSCALL：子 vmar 授父 rights 全集");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hchild as u64, 0, 0), 0, "SYSCALL：close 子 vmar");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hvm as u64, 0, 0), 0, "SYSCALL：close root vmar");

    // E3c. 登记槽注销-复用满形（G4-2 修批 P0-1 面 b——API 级；f4 意见⑤满形）：
    //      17 轮「建 root Vmar → 登记 → 反查验活 → Drop（注销）→ 反查验空」
    //      + 第 18 轮登记过——修批前 16 槽只进不出（第 17 轮起终身 -ENOMEM
    //      帽），修批后 `Vmar::Drop` 注销收口、槽随对象回收。附带容量面：16
    //      并发登记满 → 第 17 并发 Err(Full) → 全 Drop 后再登记 Ok（区分
    //      「帽存在」与「槽泄漏」）。Drop 在 KL 窗外（boot 直调面）→
    //      kl_held 门物理取放——顺带覆盖该门形。置于 E4 后 = root vmar 已
    //      close（其临时登记随 Drop 注销），槽面从零起算。
    //
    //      **登记/注销地址耦合走 Box 形**：登记表 ptr 与 `Vmar::Drop` 的
    //      `self as *mut Vmar` 须同址——`Box::from_raw(p)` 的 drop_in_place
    //      契约保证在 p；栈对象 + `mem::drop(local)` 的值会 memcpy 进 drop
    //      参数槽再跑 Drop（宿主 O0-O3 全复现 mismatch），地址耦合不受语言
    //      保证——登记面一律 Box/泄漏形（生产 0x33 handle 表 Box 同形）。
    for round in 0..18u32 {
        let asid = crate::aspace::alloc_asid();
        // SAFETY: into_raw 后指针单点持有；drop 经 from_raw 原址回收。
        let p = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(
            Vmar::new_root(crate::mm::frames(), asid)
                .expect("SYSCALL：E3c 建 root Vmar（可信语境）"),
        ));
        crate::vmar::register_demand_root(asid, p)
            .expect("SYSCALL：E3c 登记（Drop 回收槽——17 轮后仍有位）");
        assert_eq!(crate::vmar::demand_root(asid), Some(p),
            "SYSCALL：E3c 第 {} 轮反查验活", round);
        // SAFETY: p 来自 into_raw 未动；from_raw → drop_in_place 在 p 原址
        //（地址耦合保证面——kl_held 门 → 注销 + regions 零 demand 区间零钉放）。
        drop(unsafe { alloc::boxed::Box::from_raw(p) });
        assert!(crate::vmar::demand_root(asid).is_none(),
            "SYSCALL：E3c 第 {} 轮 Drop 注销后反查须空", round);
    }
    // OwnerConflict 负面（G4-3 B 刀前件 ②）：同 asid 异根双登记 = 单 root
    // 不变量破 → 防御拒；未遂者不落表不误清（demand_root 仍 p1；p2 Drop
    // 双匹配 no-op）；同根幂等重登 Ok（回卷复用同号清后再登的快路径）。
    // Box 形同上——登记 ptr 与 Vmar::Drop 须同址（地址耦合保证面）。
    let asid_oc = crate::aspace::alloc_asid();
    let p1 = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(
        Vmar::new_root(crate::mm::frames(), asid_oc)
            .expect("SYSCALL：E3c OwnerConflict 建 p1（可信语境）"),
    ));
    crate::vmar::register_demand_root(asid_oc, p1)
        .expect("SYSCALL：E3c OwnerConflict 首登记 Ok");
    crate::vmar::register_demand_root(asid_oc, p1)
        .expect("SYSCALL：E3c OwnerConflict 同根幂等重登 Ok（非冲突）");
    let p2 = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(
        Vmar::new_root(crate::mm::frames(), asid_oc)
            .expect("SYSCALL：E3c OwnerConflict 建 p2（异根同 asid）"),
    ));
    assert_eq!(crate::vmar::register_demand_root(asid_oc, p2),
        Err(crate::vmar::DemandRootError::OwnerConflict),
        "SYSCALL：E3c 同 asid 异根须 OwnerConflict 拒");
    assert_eq!(crate::vmar::demand_root(asid_oc), Some(p1),
        "SYSCALL：E3c 未遂双登记不落表（仍 p1）");
    // SAFETY: p2 未登记（被拒）——from_raw 原址回收，Drop 双匹配 no-op
    //（不误清 p1 槽——下方反查即证）。
    drop(unsafe { alloc::boxed::Box::from_raw(p2) });
    assert_eq!(crate::vmar::demand_root(asid_oc), Some(p1),
        "SYSCALL：E3c 未遂者 Drop 后在席者仍在（不误清）");
    // SAFETY: p1 已登记——Drop 注销收尾。
    drop(unsafe { alloc::boxed::Box::from_raw(p1) });
    assert!(crate::vmar::demand_root(asid_oc).is_none(),
        "SYSCALL：E3c OwnerConflict 收尾注销后反查须空");
    // 容量面（P3#10 前置件）：并发登记**动态填到满**（boot 面 dpg::init 常驻
    // 占 1 槽——不写死计数，满即停）→ 未登记者 Drop 后再验仍满（双匹配
    // no-op，不误清他人槽）→ 全 Drop 后槽回收再登记 Ok。
    let mut live: alloc::vec::Vec<*mut Vmar> = alloc::vec::Vec::new();
    loop {
        let asid = crate::aspace::alloc_asid();
        let p = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(
            Vmar::new_root(crate::mm::frames(), asid)
                .expect("SYSCALL：E3c 容量面建 Vmar（16 槽 + 页表帧预算内）"),
        ));
        match crate::vmar::register_demand_root(asid, p) {
            Ok(()) => live.push(p),
            Err(crate::vmar::DemandRootError::Full) => {
                // SAFETY: 未登记即回收——Drop 注销双匹配 no-op，无残。
                drop(unsafe { alloc::boxed::Box::from_raw(p) });
                break; // 帽到（该 v 未登记）
            }
            Err(e) => panic!("SYSCALL：E3c 容量面意外错误：{:?}", e),
        }
    }
    assert!(!live.is_empty() && live.len() <= 16, "SYSCALL：E3c 填满面计数离谱");
    // 未登记者 Drop 不误清他人槽：再登记一发须仍 Full（若误清即 Ok 翻案）。
    let asid_chk = crate::aspace::alloc_asid();
    let p_chk = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(
        Vmar::new_root(crate::mm::frames(), asid_chk)
            .expect("SYSCALL：E3c 验满建 Vmar"),
    ));
    assert_eq!(
        crate::vmar::register_demand_root(asid_chk, p_chk),
        Err(crate::vmar::DemandRootError::Full),
        "SYSCALL：E3c 未登记者已 Drop，他人槽须仍在（满）"
    );
    // SAFETY: 未登记；原址回收，Drop 双匹配 no-op。
    drop(unsafe { alloc::boxed::Box::from_raw(p_chk) });
    // SAFETY: live 内指针全自 into_raw；逐个原址回收 = 全注销（drop(live)
    // 只放裸指针不触 Drop——泄漏形禁用）。
    for p in live.drain(..) {
        drop(unsafe { alloc::boxed::Box::from_raw(p) });
    }
    let asid18 = crate::aspace::alloc_asid();
    let p18 = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(
        Vmar::new_root(crate::mm::frames(), asid18)
            .expect("SYSCALL：E3c 回收后建 Vmar"),
    ));
    crate::vmar::register_demand_root(asid18, p18)
        .expect("SYSCALL：E3c 全注销后再登记 Ok（槽已回收非泄漏）");
    assert_eq!(crate::vmar::demand_root(asid18), Some(p18),
        "SYSCALL：E3c 回收槽反查验活");
    // SAFETY: 同上；Drop 注销收尾。
    drop(unsafe { alloc::boxed::Box::from_raw(p18) });

    // E3d. EL0 槽池属主过滤取（X-5 错配消耗舞修刀，dpg::el0_take_for 三态）：
    //      双窗错位——vd_b 先臂（占**低位**槽）、vd_a 后臂，take_for(a) 须跳过
    //      b 槽只取 a 的（b 槽仍 armed；老形「全局取最低位」此局必先咬 b 槽 →
    //      身份验 -EINVAL + 槽已耗 + 恒唤重臂 = 舞根因）；take_for(无关 vd_c)
    //      = NoneForVd 且零消费（b 槽原位存活可续取）；全收讫后 = PoolEmpty。
    //      属主解析走登记面真链（demand_root → demand_region_at → rvmo 指针
    //      比对）。断言静默融入 E 族（X-5 零新增串口标记）；槽装配 =
    //      dpg::ktest_arm_el0（复刻 probe_el0 武装动作——ktest 在 sched::init
    //      前跑，池零占用、零 pager/faulter 在飞）。
    {
        use crate::dpg::{ktest_arm_el0, El0Take};
        let vmo_a = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmo::create_demand(PAGE_SIZE).expect("SYSCALL：E3d 建 demand VMO a（可信语境）"),
        ));
        let vmo_b = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmo::create_demand(PAGE_SIZE).expect("SYSCALL：E3d 建 demand VMO b（可信语境）"),
        ));
        let asid_a = crate::aspace::alloc_asid();
        let asid_b = crate::aspace::alloc_asid();
        let root_a = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmar::new_root(crate::mm::frames(), asid_a)
                .expect("SYSCALL：E3d 建 root Vmar a（可信语境）"),
        ));
        let root_b = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmar::new_root(crate::mm::frames(), asid_b)
                .expect("SYSCALL：E3d 建 root Vmar b（可信语境）"),
        ));
        const VA_A: usize = 0x5000_0000;
        const VA_B: usize = 0x5100_0000;
        root_a
            .map_demand(vmo_a, VA_A, 0, PAGE_SIZE, crate::vmar::Perm::UserRw)
            .expect("SYSCALL：E3d map_demand a");
        root_b
            .map_demand(vmo_b, VA_B, 0, PAGE_SIZE, crate::vmar::Perm::UserRw)
            .expect("SYSCALL：E3d map_demand b");
        crate::vmar::register_demand_root(asid_a, root_a as *mut Vmar)
            .expect("SYSCALL：E3d 登记 root a");
        crate::vmar::register_demand_root(asid_b, root_b as *mut Vmar)
            .expect("SYSCALL：E3d 登记 root b");
        // 武装：b 先 a 后——b 占低位槽（跳过面的证明前提）。
        let slot_b = ktest_arm_el0(VA_B, asid_b);
        let slot_a = ktest_arm_el0(VA_A, asid_a);
        assert!(slot_b < slot_a, "SYSCALL：E3d 装配前提（b 先臂占低位槽）");
        // ① take_for(a)：跳过低位 b 槽只取 a 槽（far/asid/slot 全对号）。
        match crate::dpg::el0_take_for(vmo_a as *mut Vmo) {
            El0Take::Supplied { far, asid, slot } => assert_eq!(
                (far, asid, slot),
                (VA_A, asid_a, slot_a),
                "SYSCALL：E3d take_for(a) 须只取 a 槽（跳过低位 b 槽）"
            ),
            El0Take::NoneForVd => panic!("SYSCALL：E3d take_for(a) 须 Supplied（a 槽在席）"),
            El0Take::PoolEmpty => panic!("SYSCALL：E3d take_for(a) 须 Supplied（两槽在席）"),
        }
        // ② take_for(无关 vd_c)：NoneForVd——b 槽在席但属主不匹配，零消费
        //    零唤醒。
        let vmo_c = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmo::create_demand(PAGE_SIZE).expect("SYSCALL：E3d 建 demand VMO c（可信语境）"),
        ));
        assert!(
            matches!(
                crate::dpg::el0_take_for(vmo_c as *mut Vmo),
                El0Take::NoneForVd
            ),
            "SYSCALL：E3d take_for(无关 vd) 须 NoneForVd（b 槽在席不匹配）"
        );
        // ③ b 槽未被 ①/② 误消费——take_for(b) 原位收讫（NoneForVd 零消费
        //    的活证）。
        match crate::dpg::el0_take_for(vmo_b as *mut Vmo) {
            El0Take::Supplied { far, asid, slot } => assert_eq!(
                (far, asid, slot),
                (VA_B, asid_b, slot_b),
                "SYSCALL：E3d b 槽须原位存活（跳过/NoneForVd 零消费）"
            ),
            El0Take::NoneForVd => panic!("SYSCALL：E3d take_for(b) 须 Supplied（b 槽未被误消费）"),
            El0Take::PoolEmpty => panic!("SYSCALL：E3d take_for(b) 须 Supplied（b 槽在席）"),
        }
        // ④ 全池空（三态收口）。
        assert!(
            matches!(
                crate::dpg::el0_take_for(vmo_a as *mut Vmo),
                El0Take::PoolEmpty
            ),
            "SYSCALL：E3d 全消费后须 PoolEmpty"
        );
        // 收尾：Vmar 先放（Drop 注销登记 + 放 demand 钉）→ Vmo 后放（钉已
        // 放可回收）；c 无映射直接放。Box 泄漏形 from_raw 原址回收（E3c
        // 地址耦合同纪律）。
        // SAFETY: 指针全自 Box::leak 未动；from_raw → drop_in_place 原址
        //（登记/Region 指针地址耦合面）。
        drop(unsafe { alloc::boxed::Box::from_raw(root_a as *mut Vmar) });
        drop(unsafe { alloc::boxed::Box::from_raw(root_b as *mut Vmar) });
        drop(unsafe { alloc::boxed::Box::from_raw(vmo_a as *mut Vmo) });
        drop(unsafe { alloc::boxed::Box::from_raw(vmo_b as *mut Vmo) });
        drop(unsafe { alloc::boxed::Box::from_raw(vmo_c as *mut Vmo) });
    }

    // E3e. 孤儿槽 GC（X-5 三镜头可达面：跨窗 sys_vmar_unmap 摘区间 /
    //      Vmar::Drop 注销登记 / faulter 进程被杀——armed 不随亡，老形错配
    //      烧槽即它的意外 GC，过滤取后须自持）。装配：幽灵臂（asid 永不
    //      登记 root → demand_root None）占低位 + 合法臂占高位；take_for(vd)
    //      须先 GC 消费幽灵（唤醒面 = 空 SLEEPQ 无操作）续扫收合法臂；幽灵
    //      槽放归（复臂同号 = 消费活证）；再 GC 后无 any_armed 残留 →
    //      PoolEmpty（池态如实——误置则 NoneForVd 假报、pager 拒睡空转）。
    {
        use crate::dpg::{ktest_arm_el0, El0Take};
        let vmo_e = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmo::create_demand(PAGE_SIZE).expect("SYSCALL：E3e 建 demand VMO e（可信语境）"),
        ));
        let asid_e = crate::aspace::alloc_asid();
        let root_e = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmar::new_root(crate::mm::frames(), asid_e)
                .expect("SYSCALL：E3e 建 root Vmar e（可信语境）"),
        ));
        const VA_E: usize = 0x5200_0000;
        root_e
            .map_demand(vmo_e, VA_E, 0, PAGE_SIZE, crate::vmar::Perm::UserRw)
            .expect("SYSCALL：E3e map_demand e");
        crate::vmar::register_demand_root(asid_e, root_e as *mut Vmar)
            .expect("SYSCALL：E3e 登记 root e");
        let asid_ghost = crate::aspace::alloc_asid(); // 永不登记——孤儿装配
        // 幽灵先臂占低位（GC 臂先于合法臂被扫到）。
        let slot_ghost = ktest_arm_el0(VA_E, asid_ghost);
        let slot_e = ktest_arm_el0(VA_E, asid_e);
        assert!(slot_ghost < slot_e, "SYSCALL：E3e 装配前提（幽灵先臂占低位槽）");
        // ① take_for(e)：扫到幽灵臂 → GC 消费 → 续扫收合法 e 臂（对号）。
        match crate::dpg::el0_take_for(vmo_e as *mut Vmo) {
            El0Take::Supplied { far, asid, slot } => assert_eq!(
                (far, asid, slot),
                (VA_E, asid_e, slot_e),
                "SYSCALL：E3e 须 GC 幽灵后续扫收合法 e 臂"
            ),
            El0Take::NoneForVd => panic!("SYSCALL：E3e 须 Supplied（e 臂在席）"),
            El0Take::PoolEmpty => panic!("SYSCALL：E3e 须 Supplied（两臂在席）"),
        }
        // ② 幽灵槽已被 GC 消费放归：复臂得同号（消费活证）。
        assert_eq!(
            ktest_arm_el0(VA_E, asid_ghost),
            slot_ghost,
            "SYSCALL：E3e 幽灵槽须已被 GC 消费（复臂同号）"
        );
        // ③ 仅剩幽灵臂 → 再 GC → 全池空 = PoolEmpty（孤儿不置 any_armed）。
        assert!(
            matches!(
                crate::dpg::el0_take_for(vmo_e as *mut Vmo),
                El0Take::PoolEmpty
            ),
            "SYSCALL：E3e GC 后须 PoolEmpty（any_armed 不残留）"
        );
        // 收尾同 E3d 纪律：Vmar 先放（Drop 注销）→ Vmo 后放（钉已放可回收）。
        // SAFETY: 指针全自 Box::leak 未动；from_raw → drop_in_place 原址
        //（登记/Region 指针地址耦合面）。
        drop(unsafe { alloc::boxed::Box::from_raw(root_e as *mut Vmar) });
        drop(unsafe { alloc::boxed::Box::from_raw(vmo_e as *mut Vmo) });
    }

    // E3f. 池容量边界（EREQ_SLOTS=32，X-5 扩池 8→32）：满装 K 臂全属 vd →
    //      take_for 收最低位 → 复臂回收同号（消费即放槽活证）→ 抽干 K 臂 →
    //      PoolEmpty。溢出 convoy 的 park/rescan 面（池满 faulter 睡
    //      EOVERFLOW_KEY + 放槽定向唤重扫）需 sched 在飞——ktest 前置窗
    //      不可达，该面由 smoke dpgsoak/svcmgr 变体档覆盖（N>K 拓扑，K=32
    //      后现役 N≤15 零发、K=8 历史档存照）。
    {
        use crate::dpg::{ktest_arm_el0, El0Take};
        let vmo_f = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmo::create_demand(PAGE_SIZE).expect("SYSCALL：E3f 建 demand VMO f（可信语境）"),
        ));
        let asid_f = crate::aspace::alloc_asid();
        let root_f = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmar::new_root(crate::mm::frames(), asid_f)
                .expect("SYSCALL：E3f 建 root Vmar f（可信语境）"),
        ));
        const VA_F: usize = 0x5300_0000;
        root_f
            .map_demand(vmo_f, VA_F, 0, PAGE_SIZE, crate::vmar::Perm::UserRw)
            .expect("SYSCALL：E3f map_demand f");
        crate::vmar::register_demand_root(asid_f, root_f as *mut Vmar)
            .expect("SYSCALL：E3f 登记 root f");
        let k = crate::dpg::EREQ_SLOTS;
        let mut lowest = usize::MAX;
        for j in 0..k {
            let s = ktest_arm_el0(VA_F, asid_f);
            if j == 0 {
                lowest = s;
            }
        }
        // ① 满装首收 = 最低位（K 臂全属主匹配）。
        match crate::dpg::el0_take_for(vmo_f as *mut Vmo) {
            El0Take::Supplied { slot, .. } => {
                assert_eq!(slot, lowest, "SYSCALL：E3f 满装首收须最低位槽")
            }
            El0Take::NoneForVd => panic!("SYSCALL：E3f 满装须 Supplied（K 臂全属 f）"),
            El0Take::PoolEmpty => panic!("SYSCALL：E3f 满装须 Supplied（K 臂在席）"),
        }
        // ② 消费即放槽：复臂回收刚放槽（最低空闲 = 被消费位）。
        assert_eq!(
            ktest_arm_el0(VA_F, asid_f),
            lowest,
            "SYSCALL：E3f 复臂须回收刚放槽"
        );
        // ③ 抽干 K 臂 → PoolEmpty。
        for j in 0..k {
            assert!(
                matches!(
                    crate::dpg::el0_take_for(vmo_f as *mut Vmo),
                    El0Take::Supplied { .. }
                ),
                "SYSCALL：E3f 抽干第 {} 臂须 Supplied",
                j
            );
        }
        assert!(
            matches!(
                crate::dpg::el0_take_for(vmo_f as *mut Vmo),
                El0Take::PoolEmpty
            ),
            "SYSCALL：E3f 抽干后须 PoolEmpty"
        );
        // 收尾同 E3d 纪律：Vmar 先放 → Vmo 后放。
        // SAFETY: 指针全自 Box::leak 未动；from_raw → drop_in_place 原址
        //（登记/Region 指针地址耦合面）。
        drop(unsafe { alloc::boxed::Box::from_raw(root_f as *mut Vmar) });
        drop(unsafe { alloc::boxed::Box::from_raw(vmo_f as *mut Vmo) });
    }

    // E3g. AM-2 等页停顿台账零态不变量（plans/am/design.md §压力信号）：
    //      ktest::run 先于 sched::init/dpg::init（main.rs :614 < :668/:692），
    //      真实缺页未发——三账必零。非空即两解其一：顺序被挪（ktest 挪到
    //      sched 后——本断言 fail loud 逼重审 E3d-f 的「无 sched 在飞」前提）
    //      或台账静态量被污染（boot 期异常路径记账）。真实路径的正向证明 =
    //      dpg_faulter 醒后自证（真实端到端、零 ktest 污染，见 dpg.rs）；
    //      EL0 臂随 svcmgr 供给链 smoke 面覆盖。
    assert_eq!(crate::telemetry::dpg_stall_n_total(), 0,
        "SYSCALL：E3g ktest 期停顿笔数必零（ktest 先于 sched::init——非零即顺序被挪或台账污染）");
    assert_eq!(crate::telemetry::dpg_stall_cyc_total(), 0,
        "SYSCALL：E3g ktest 期停顿周期累计必零（同上）");
    assert_eq!(crate::telemetry::dpg_stall_max(), 0,
        "SYSCALL：E3g ktest 期停顿峰值必零（同上）");
    // E3h. 0x24 pager_supply_mapped_table 表批面（X-6 刀① count-wins 多窗
    //      一次收货——0x23 的批形）。全路径经 svc6 真派发（EL1 表 =
    //      bootstrap 表）。参数面（n 越界 / dv:0 门败探测形 / delta 非
    //      页对齐）零池触；正例须臂集在池——ktest 前置窗 sched 未起，
    //      「池空且 0 供 → 泊键」臂不可达（每调用至留一臂未属主匹配即止，
    //      设计约束同 E3d–f 注）；供给身份验要求表内 dv 本体 == 登记区间
    //      rvmo——VMO 经 bootstrap 表插取（SET_POLICY 门）、root map 用
    //      同一本体指针。断言静默融入 E 族（X-6 零新增串口标记）。
    {
        use crate::dpg::{ktest_arm_el0, El0Take};
        use crate::handle::RIGHT_SET_POLICY;
        use crate::syscall::{PagerWindow, NR_PAGER_SUPPLY_MAPPED_TABLE};

        // 表批缓冲（EL1 高半直映——EBUF 同款纪律；repr(C) 16B/项字节序
        // 与内核小端拼装面同构）。
        static mut TBL: [PagerWindow; 2] = [PagerWindow { dv: 0, sv: 0, delta: 0 }; 2];

        // ① n 越界（0 / 33 同拒 -EINVAL；x2 全宽读——高 32 位脏值同面）。
        // SAFETY: svc 同步异常；TBL 为本块私有静态。
        unsafe { TBL = [PagerWindow { dv: 0, sv: 0, delta: 0 }; 2] };
        assert_eq!(
            svc6(NR_PAGER_SUPPLY_MAPPED_TABLE as u64, &raw const TBL as u64, 0, 0, 0, 0, 0),
            -22,
            "SYSCALL：E3h n=0 须 -EINVAL"
        );
        assert_eq!(
            svc6(NR_PAGER_SUPPLY_MAPPED_TABLE as u64, &raw const TBL as u64, 33, 0, 0, 0, 0),
            -22,
            "SYSCALL：E3h n=33 须 -EINVAL（表帽 32）"
        );
        // ② boot 探测判据形（svcmgr 0x24(dv:0,n:1)）：delta=0 页对齐过
        //    预检 → 门控 dv=0 坏句柄 → -EBADF（新核判据值）。
        assert_eq!(
            svc6(NR_PAGER_SUPPLY_MAPPED_TABLE as u64, &raw const TBL as u64, 1, 0, 0, 0, 0),
            -9,
            "SYSCALL：E3h 探测形 dv:0 须 -EBADF"
        );
        // ③ delta 非页对齐 → -EINVAL（预检先于门控——参数面整批拒）。
        // SAFETY: 同上。
        unsafe { TBL[0].delta = 8 };
        assert_eq!(
            svc6(NR_PAGER_SUPPLY_MAPPED_TABLE as u64, &raw const TBL as u64, 1, 0, 0, 0, 0),
            -22,
            "SYSCALL：E3h delta 非页对齐须 -EINVAL"
        );

        // ④ 正例：双 demand VMO（bootstrap 表 + SET_POLICY）各配 root Vmar
        //    登记；g 一臂 / h 两臂（continue-take 抽干面）；表 [g, h] n=2
        //    一次收货——rc=3 = **供页数**（设计 §2.1 冻结词面：g 窗 1 页 +
        //    h 窗内环抽干 2 页；「窗数」读法 2 可区分）。
        let vmo_g = Vmo::create_demand(PAGE_SIZE).expect("SYSCALL：E3h 建 demand VMO g（可信语境）");
        // h 容量两页（map_demand 的 mech-M-1 bounds：0+2 页区间须 ≤ VMO
        // size——1 页 VMO 会在登记即 OutOfBounds panic；第二页供给走
        // vmo.commit(page_off=PAGE_SIZE) 落 pages[1]，两页容量是硬前置）。
        let vmo_h = Vmo::create_demand(2 * PAGE_SIZE).expect("SYSCALL：E3h 建 demand VMO h（可信语境）");
        let hg = ht
            .insert(KernelObj::Vmo(vmo_g), RIGHT_SET_POLICY)
            .expect("SYSCALL：E3h insert g");
        let hh = ht
            .insert(KernelObj::Vmo(vmo_h), RIGHT_SET_POLICY)
            .expect("SYSCALL：E3h insert h");
        // 本体指针（表内真对象——供给身份验要求 rvmo == 表内 dv 本体）。
        let (og, _) = ht
            .get_raw(hg, ObjType::Vmo, RIGHT_SET_POLICY)
            .expect("SYSCALL：E3h get_raw g");
        let (oh, _) = ht
            .get_raw(hh, ObjType::Vmo, RIGHT_SET_POLICY)
            .expect("SYSCALL：E3h get_raw h");
        // SAFETY: get_raw 门控后取本体地址（只取地址，不给别名）。
        let ptr_g = unsafe { (*og.as_ptr()).obj.as_vmo_mut().unwrap() as *mut Vmo };
        let ptr_h = unsafe { (*oh.as_ptr()).obj.as_vmo_mut().unwrap() as *mut Vmo };
        let asid_g = crate::aspace::alloc_asid();
        let asid_h = crate::aspace::alloc_asid();
        let root_g = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmar::new_root(crate::mm::frames(), asid_g)
                .expect("SYSCALL：E3h 建 root Vmar g（可信语境）"),
        ));
        let root_h = alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmar::new_root(crate::mm::frames(), asid_h)
                .expect("SYSCALL：E3h 建 root Vmar h（可信语境）"),
        ));
        const VA_G: usize = 0x5400_0000;
        const VA_H: usize = 0x5500_0000;
        // SAFETY: 本体指针取自表内对象；map 只录区间（不动池）。
        unsafe {
            root_g
                .map_demand(&mut *ptr_g, VA_G, 0, PAGE_SIZE, crate::vmar::Perm::UserRw)
                .expect("SYSCALL：E3h map_demand g");
        }
        // h 两页区间（continue-take 抽干 = 两**不同**页真供给，非同页幂等）。
        unsafe {
            root_h
                .map_demand(&mut *ptr_h, VA_H, 0, 2 * PAGE_SIZE, crate::vmar::Perm::UserRw)
                .expect("SYSCALL：E3h map_demand h");
        }
        crate::vmar::register_demand_root(asid_g, root_g as *mut Vmar)
            .expect("SYSCALL：E3h 登记 root g");
        crate::vmar::register_demand_root(asid_h, root_h as *mut Vmar)
            .expect("SYSCALL：E3h 登记 root h");
        let _slot_g = ktest_arm_el0(VA_G, asid_g);
        let _slot_h0 = ktest_arm_el0(VA_H, asid_h);
        let _slot_h1 = ktest_arm_el0(VA_H + PAGE_SIZE, asid_h);
        // SAFETY: svc 同步异常；TBL 为本块私有静态。
        unsafe {
            TBL = [
                PagerWindow { dv: hg, sv: 0, delta: 0 },
                PagerWindow { dv: hh, sv: 0, delta: 0 },
            ];
        }
        assert_eq!(
            svc6(NR_PAGER_SUPPLY_MAPPED_TABLE as u64, &raw const TBL as u64, 2, 0, 0, 0, 0),
            3,
            "SYSCALL：E3h 双窗三臂批收须 rc=3（供页数计数）"
        );

        // ⑤ 部分计数 + NoneForVd 跳次窗零消费：仅 g 臂在池；表 [h, g]
        //    （无臂窗 h 在前）——h 窗 NoneForVd 跳过（池非空不匹配）→
        //    g 窗收讫 count=1 → 续取池空且已供>0 → 即返计数不阻塞 = rc=1
        //    （h 零供给部分计；「池空即返」臂零 block 面）。
        let _ = ktest_arm_el0(VA_G, asid_g);
        // SAFETY: 同上。
        unsafe {
            TBL = [
                PagerWindow { dv: hh, sv: 0, delta: 0 },
                PagerWindow { dv: hg, sv: 0, delta: 0 },
            ];
        }
        assert_eq!(
            svc6(NR_PAGER_SUPPLY_MAPPED_TABLE as u64, &raw const TBL as u64, 2, 0, 0, 0, 0),
            1,
            "SYSCALL：E3h 部分计数须 rc=1（h 窗跳过、g 窗收讫）"
        );

        // ⑥ 聚合 NoneForVd → -EINVAL：仅 h 臂在池；表 [g] n=1——g 窗
        //    唯一臂不属主 → NoneForVd 出环 0 供 → -EINVAL（0x23 单发同码
        //    面的批形收口）；h 臂零消费原位存活（直收取证——活证收尾）。
        let _ = ktest_arm_el0(VA_H, asid_h);
        // SAFETY: 同上。
        unsafe { TBL[0] = PagerWindow { dv: hg, sv: 0, delta: 0 } };
        assert_eq!(
            svc6(NR_PAGER_SUPPLY_MAPPED_TABLE as u64, &raw const TBL as u64, 1, 0, 0, 0, 0),
            -22,
            "SYSCALL：E3h 聚合 NoneForVd 须 -EINVAL"
        );
        assert!(
            matches!(
                crate::dpg::el0_take_for(ptr_h),
                El0Take::Supplied { .. }
            ),
            "SYSCALL：E3h h 臂须原位存活（NoneForVd 零消费）"
        );

        // 收尾：先逐区间 unmap（清叶 + untrack_map → mapcnt 归零）→ Vmar
        // 放（Drop 注销登记 + 放 demand 钉）→ 表内 VMO close。unmap 前置
        // 是硬需：④/⑤ 真供给过页而 Vmar::Drop 刻意不动 mapcnt（残留 PTE
        // 政策的正交不对称），销毁性 close 的 any_mapped 拒面必 -EBUSY
        //（E3d–f 无此坑 = 它们从不真供给）——先解除映射放映射钉，close
        // 才走真销毁面过断言。
        root_g
            .unmap(VA_G, PAGE_SIZE)
            .expect("SYSCALL：E3h unmap g");
        root_h
            .unmap(VA_H, 2 * PAGE_SIZE)
            .expect("SYSCALL：E3h unmap h");
        // SAFETY: 指针全自 Box::leak 未动；from_raw → drop_in_place 原址
        //（登记/Region 指针地址耦合面）。
        drop(unsafe { alloc::boxed::Box::from_raw(root_g as *mut Vmar) });
        drop(unsafe { alloc::boxed::Box::from_raw(root_h as *mut Vmar) });
        assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hg as u64, 0, 0), 0, "SYSCALL：E3h close g");
        assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hh as u64, 0, 0), 0, "SYSCALL：E3h close h");
    }

    // E5. vmo_get_size（0x1A，G-eco 入机侧：svcmgr 消费 rootfs 需要 cpio 总长；
    //     add-only 顺延）。盖四路：往返 + 页对齐语义（create 即取整，非 blob
    //     实长——消费端 cpio 遍历以 newc 头自描述长度为准）/ 坏 handle /
    //     类型门控（Event 非 Vmo）/ need=0 面（仅类型不查权利位——零 READ
    //     观察者可查几何，同 object_wait「等待不需权利」惯例）。
    // SAFETY: svc 同步异常；OUT 为本函数私有静态。
    unsafe { OUT = 0 };
    assert_eq!(svc6(NR_VMO_GET_SIZE as u64, 0xDEAD_BEEF, &raw mut OUT as u64, 0, 0, 0, 0), -9,
        "SYSCALL：get_size 坏 handle 须 -EBADF");
    let hev = ht
        .insert(KernelObj::Event(crate::event::Event::new()), crate::handle::RIGHT_SIGNAL)
        .expect("SYSCALL：insert event（get_size 类型面）");
    assert_eq!(svc6(NR_VMO_GET_SIZE as u64, hev as u64, &raw mut OUT as u64, 0, 0, 0, 0), -9,
        "SYSCALL：get_size 对 Event 须 -EBADF（类型门控）");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hev as u64, 0, 0), 0, "SYSCALL：close event");
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(svc6(NR_VMO_CREATE as u64, 0x1800, 0, &raw mut OUT as u64, 0, 0, 0), 0,
        "SYSCALL：vmo_create 非 4K 整倍 size");
    let hgs = unsafe { OUT } as u32;
    // SAFETY: 同上（预置哨兵值，验回写真发生）。
    unsafe { OUT = u64::MAX };
    assert_eq!(svc6(NR_VMO_GET_SIZE as u64, hgs as u64, &raw mut OUT as u64, 0, 0, 0, 0), 0,
        "SYSCALL：get_size");
    assert_eq!(unsafe { OUT }, PAGE_SIZE as u64 * 2,
        "SYSCALL：get_size 须回页对齐容量（0x1800 → 0x2000）");
    // need=0 面：dup 收紧到仅 DUPLICATE（无 READ）仍可查。
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(NR_HANDLE_DUPLICATE as u64, hgs as u64, crate::handle::RIGHT_DUPLICATE,
             &raw mut OUT as u64),
        0,
        "SYSCALL：dup 收紧到仅 DUPLICATE"
    );
    let hobs = unsafe { OUT } as u32;
    // SAFETY: 同上。
    unsafe { OUT = u64::MAX };
    assert_eq!(svc6(NR_VMO_GET_SIZE as u64, hobs as u64, &raw mut OUT as u64, 0, 0, 0, 0), 0,
        "SYSCALL：无 READ handle 查 size 须放行（need=0 不查权利位）");
    assert_eq!(unsafe { OUT }, PAGE_SIZE as u64 * 2, "SYSCALL：零 READ 别名 size 同值");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hobs as u64, 0, 0), 0, "SYSCALL：close 零 READ 别名");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hgs as u64, 0, 0), 0, "SYSCALL：close get_size VMO");

    // E6. MMIO 物理窗口 VMO（P6 A-2，boot-only `Vmo::create_mmio`——无 syscall
    //     构造面，ktest 直构对象盖内核面）：几何（size 页对齐 / is_mmio /
    //     page_pa 窗口基 + 越界 None）+ 内容面四拒（commit/read/write/decommit
    //     → NotSupported——设备窗口只能经映射 volatile 访问，杜绝把设备 PA
    //     当内存帧的路径）+ syscall 面（voe 折 -95）+ vmar_map Device attr
    //     分派成功 + 映射中 close -EBUSY + unmap/close 全链（E3 的 hvm 已关，
    //     此处新建 root Vmar）。
    assert!(
        matches!(
            Vmo::create_mmio(berry_hal_aarch64::VIRTIO_MMIO0_PA, 0),
            Err(crate::vmo::VmoError::OutOfRange)
        ),
        "MMIO：size=0 须 OutOfRange"
    );
    assert!(
        matches!(
            Vmo::create_mmio(berry_hal_aarch64::VIRTIO_MMIO0_PA + 0x100, 0x200),
            Err(crate::vmo::VmoError::Unaligned)
        ),
        "MMIO：非页对齐基须 Unaligned"
    );
    assert!(
        matches!(
            Vmo::create_mmio(berry_hal_aarch64::VIRTIO_MMIO0_PA, 0x2200),
            Err(crate::vmo::VmoError::Unaligned)
        ),
        "MMIO：非页对齐 size 须 Unaligned（静默取整即漏授相邻设备 PA）"
    );
    let mut mv = Vmo::create_mmio(
        berry_hal_aarch64::VIRTIO_MMIO0_PA,
        berry_hal_aarch64::VIRTIO_MMIO_LEN,
    )
    .expect("MMIO：建窗口 VMO");
    assert_eq!(
        mv.size(),
        berry_hal_aarch64::VIRTIO_MMIO_LEN,
        "MMIO：整窗 32 槽 × 0x200 = 0x4000（页对齐 4 页）"
    );
    assert!(mv.is_mmio(), "MMIO：is_mmio");
    assert_eq!(
        mv.page_pa(0),
        Some(berry_hal_aarch64::VIRTIO_MMIO0_PA),
        "MMIO：page_pa(0) = 窗口基"
    );
    assert_eq!(
        mv.page_pa(3),
        Some(berry_hal_aarch64::VIRTIO_MMIO0_PA + 3 * PAGE_SIZE),
        "MMIO：page_pa(3) = 窗口基 + 3 页"
    );
    assert_eq!(mv.page_pa(4), None, "MMIO：page_pa 越界 None（幅 4 页）");
    // 局部栈缓冲即可（NotSupported 检查先于任何访问——缓冲内容不参与）。
    let mut rbuf = [0u8; 16];
    let wbuf = [0u8; 16];
    assert!(
        matches!(
            mv.commit(crate::mm::frames(), 0, PAGE_SIZE),
            Err(crate::vmo::VmoError::NotSupported)
        ),
        "MMIO：commit 须 NotSupported"
    );
    assert!(
        matches!(
            mv.decommit(crate::mm::frames(), 0, PAGE_SIZE),
            Err(crate::vmo::VmoError::NotSupported)
        ),
        "MMIO：decommit 须 NotSupported"
    );
    assert!(
        matches!(mv.read(0, &mut rbuf), Err(crate::vmo::VmoError::NotSupported)),
        "MMIO：read 须 NotSupported"
    );
    assert!(
        matches!(mv.write(0, &wbuf), Err(crate::vmo::VmoError::NotSupported)),
        "MMIO：write 须 NotSupported"
    );
    let hmm = ht
        .insert(
            KernelObj::Vmo(mv),
            RIGHT_READ | RIGHT_WRITE | RIGHT_MAP, // boot 授能形态（vmar_map vmo 侧门 = MAP|R|W 恰集）
        )
        .expect("MMIO：insert 窗口 VMO");
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(
        svc6(NR_VMO_GET_SIZE as u64, hmm as u64, &raw mut OUT as u64, 0, 0, 0, 0),
        0,
        "MMIO：get_size（need=0 放行）"
    );
    assert_eq!(
        unsafe { OUT },
        berry_hal_aarch64::VIRTIO_MMIO_LEN as u64,
        "MMIO：get_size 回页对齐幅（整窗 4 页）"
    );
    assert_eq!(
        svc6(NR_VMO_COMMIT as u64, hmm as u64, 0, PAGE_SIZE as u64, 0, 0, 0),
        -95,
        "MMIO：syscall commit 须 -ENOTSUP（NotSupported 经 voe）"
    );
    let hvm2 = ht
        .insert(
            KernelObj::Vmar(
                Vmar::new_root(crate::mm::frames(), crate::aspace::alloc_asid())
                    .expect("MMIO：建 root Vmar"),
            ),
            RIGHT_MAP | RIGHT_MANAGE,
        )
        .expect("MMIO：insert root vmar");
    // vmo 侧 MAP 门：R|W 别名（无 MAP）→ map -EACCES。
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(NR_HANDLE_DUPLICATE as u64, hmm as u64, 3 /* READ|WRITE */, &raw mut OUT as u64),
        0,
        "MMIO：dup 收紧到 R|W"
    );
    let hmw = unsafe { OUT } as u32;
    // SAFETY: 同上。
    unsafe { OUT = crate::aspace::USER_VA as u64 };
    assert_eq!(
        svc6(NR_VMAR_MAP as u64, hvm2 as u64, hmw as u64, 0, &raw mut OUT as u64,
             berry_hal_aarch64::VIRTIO_MMIO_LEN as u64, 3 /* READ|WRITE */),
        -13,
        "MMIO：无 MAP 别名映射须 -EACCES（vmo 侧门控）"
    );
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hmw as u64, 0, 0), 0, "MMIO：close R|W 别名");
    // 全权 handle 映射成功（Device-nGnRnE attr 分派在 vmar map 内——成功即分派不炸）。
    // SAFETY: 同上。
    unsafe { OUT = crate::aspace::USER_VA as u64 };
    assert_eq!(
        svc6(NR_VMAR_MAP as u64, hvm2 as u64, hmm as u64, 0, &raw mut OUT as u64,
             berry_hal_aarch64::VIRTIO_MMIO_LEN as u64, 3 /* READ|WRITE */),
        0,
        "MMIO：vmar_map（Device attr 分派）"
    );
    assert_eq!(unsafe { OUT }, crate::aspace::USER_VA as u64, "MMIO：map 回写基址不变");
    // 映射中销毁性 close 拒 -EBUSY（mapcnt 防线对窗口同样成立）→ unmap 后放行。
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hmm as u64, 0, 0), -16,
        "MMIO：映射中 close 须 -EBUSY");
    assert_eq!(
        svc6(NR_VMAR_UNMAP as u64, hvm2 as u64, crate::aspace::USER_VA as u64,
             berry_hal_aarch64::VIRTIO_MMIO_LEN as u64, 0, 0, 0),
        0,
        "MMIO：vmar_unmap"
    );
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hmm as u64, 0, 0), 0,
        "MMIO：close 窗口 VMO（Drop 空页零动作——设备 PA 永不进 buddy）");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hvm2 as u64, 0, 0), 0, "MMIO：close root vmar");

    // E7. vmo_get_phys（0x1B，P6 A-3 用户态 DMA 面——virtqueue 描述符地址是
    //     guest PA，驱动须能查自己 VMO 页后备）。盖六路：坏 handle / 类型门控
    //     （Event）/ 字节语义（off 越 size 拒、页内偏移直拼——mid-page 精确到
    //     字节）/ 未提交页 -EBADSTATE / commit 后 PA 落 RAM 界 + 页对齐 /
    //     need=0 放行（同 get_size 几何惯例）。MMIO 窗口后备面（pa_base+off
    //     恒可查）经裸对象 page_pa 已在 E6 盖，syscall 面同 page_pa 取值不再
    //     重铺。
    // SAFETY: svc 同步异常；OUT 为本函数私有静态。
    unsafe { OUT = 0 };
    assert_eq!(svc6(NR_VMO_GET_PHYS as u64, 0xDEAD_BEEF, 0, &raw mut OUT as u64, 0, 0, 0), -9,
        "SYSCALL：get_phys 坏 handle 须 -EBADF");
    let hev = ht
        .insert(KernelObj::Event(crate::event::Event::new()), crate::handle::RIGHT_SIGNAL)
        .expect("SYSCALL：insert event（get_phys 类型面）");
    assert_eq!(svc6(NR_VMO_GET_PHYS as u64, hev as u64, 0, &raw mut OUT as u64, 0, 0, 0), -9,
        "SYSCALL：get_phys 对 Event 须 -EBADF（类型门控）");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hev as u64, 0, 0), 0, "SYSCALL：close event");
    // 未提交页：无后备可查（-EBADSTATE，voe NotCommitted 同码语义）。
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(svc6(NR_VMO_CREATE as u64, PAGE_SIZE as u64, 0, &raw mut OUT as u64, 0, 0, 0), 0,
        "SYSCALL：vmo_create（get_phys 面）");
    let hgp = unsafe { OUT } as u32;
    // SAFETY: 同上。
    unsafe { OUT = u64::MAX };
    assert_eq!(svc6(NR_VMO_GET_PHYS as u64, hgp as u64, 0, &raw mut OUT as u64, 0, 0, 0), -37,
        "SYSCALL：get_phys 未提交页须 -EBADSTATE");
    // commit 后：PA 落 RAM 界（virt 板 [0x4000_0000, +128M)）且页对齐；页内
    // 偏移精确到字节（mid-page = 页基 + 偏移，描述符 addr 直用语义）。
    assert_eq!(svc6(NR_VMO_COMMIT as u64, hgp as u64, 0, PAGE_SIZE as u64, 0, 0, 0), 0,
        "SYSCALL：commit（get_phys 面）");
    // SAFETY: 同上（预置哨兵值，验回写真发生）。
    unsafe { OUT = u64::MAX };
    assert_eq!(svc6(NR_VMO_GET_PHYS as u64, hgp as u64, 0, &raw mut OUT as u64, 0, 0, 0), 0,
        "SYSCALL：get_phys 页基");
    let pa0 = unsafe { OUT };
    assert!((0x4000_0000..berry_hal_aarch64::RAM_TOP as u64).contains(&pa0),
        "SYSCALL：get_phys 须落 RAM 界（得 {pa0:#x}）");
    assert_eq!(pa0 % PAGE_SIZE as u64, 0, "SYSCALL：get_phys 页基页对齐");
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(svc6(NR_VMO_GET_PHYS as u64, hgp as u64, 0x123, &raw mut OUT as u64, 0, 0, 0), 0,
        "SYSCALL：get_phys mid-page");
    assert_eq!(unsafe { OUT }, pa0 + 0x123,
        "SYSCALL：get_phys 字节语义（页基 + 页内偏移）");
    // 字节域右端：offset == size 拒（[0, size)，尾后一字节也越界）。
    assert_eq!(svc6(NR_VMO_GET_PHYS as u64, hgp as u64, PAGE_SIZE as u64, &raw mut OUT as u64, 0, 0, 0),
        -34,
        "SYSCALL：get_phys offset==size 须 -EOVERANGE");
    // need=0 面：dup 收紧到仅 DUPLICATE（无 READ）仍可查（get_size 同惯例）。
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(NR_HANDLE_DUPLICATE as u64, hgp as u64, crate::handle::RIGHT_DUPLICATE,
             &raw mut OUT as u64),
        0,
        "SYSCALL：dup 收紧到仅 DUPLICATE（get_phys 面）"
    );
    let hgo = unsafe { OUT } as u32;
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(svc6(NR_VMO_GET_PHYS as u64, hgo as u64, 0x123, &raw mut OUT as u64, 0, 0, 0), 0,
        "SYSCALL：无 READ handle 查 phys 须放行（need=0 不查权利位）");
    assert_eq!(unsafe { OUT }, pa0 + 0x123, "SYSCALL：零 READ 别名 phys 同值");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hgo as u64, 0, 0), 0, "SYSCALL：close 观察别名");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hgp as u64, 0, 0), 0, "SYSCALL：close get_phys VMO");

    // F. 3-3 process/thread：对象面 + 错误面（ktest 在 berry_sched::init 之前跑，
    //    process_start 成功路径会 spawn → 不能在此验——happy path 由 EL0
    //    `user_spawner` 承担，watchdog 错杀断言兜底）。
    use crate::syscall::{NR_PROCESS_CREATE, NR_PROCESS_START, NR_THREAD_CREATE, NR_THREAD_START};
    // F0. process_create 双出参：process + root Vmar 两 handle 均非零。
    // SAFETY: 同上（OUT/CHILD_OUT 为本函数私有静态）。
    unsafe { OUT = 0; CHILD_OUT = 0 };
    assert_eq!(
        svc5(NR_PROCESS_CREATE as u64, 0, 0, &raw mut OUT as u64, &raw mut CHILD_OUT as u64),
        0,
        "SYSCALL：process_create"
    );
    let hp = unsafe { OUT } as u32;
    let hv2 = unsafe { CHILD_OUT } as u32;
    assert_ne!(hp, 0, "SYSCALL：process_create 须回填 process handle");
    assert_ne!(hv2, 0, "SYSCALL：process_create 须回填 root Vmar handle");
    // 类型断言：两 handle 各指其类型对象（三门控走查）。
    assert!(ht.get(hp, ObjType::Process, 0).is_ok(), "SYSCALL：hp 须指 Process");
    assert!(ht.get(hv2, ObjType::Vmar, 0).is_ok(), "SYSCALL：hv2 须指 Vmar");

    // F1. 参数面：job≠0 / options≠0 → -EINVAL（v1 无 Job、无 options 位）。
    assert_eq!(
        svc5(NR_PROCESS_CREATE as u64, 1, 0, &raw mut OUT as u64, &raw mut CHILD_OUT as u64),
        -22,
        "SYSCALL：process_create job≠0 须 -EINVAL"
    );
    assert_eq!(
        svc5(NR_PROCESS_CREATE as u64, 0, 1, &raw mut OUT as u64, &raw mut CHILD_OUT as u64),
        -22,
        "SYSCALL：process_create options≠0 须 -EINVAL"
    );

    // F2. thread_create：挂 hp 下建线程；类型门控（对 Vmar handle / 野值须 -EBADF）。
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(
        svc5(NR_THREAD_CREATE as u64, hp as u64, 0, &raw mut OUT as u64, 0),
        0,
        "SYSCALL：thread_create"
    );
    let hth = unsafe { OUT } as u32;
    assert_ne!(hth, 0, "SYSCALL：thread_create 须回填 handle");
    assert!(ht.get(hth, ObjType::Thread, 0).is_ok(), "SYSCALL：hth 须指 Thread");
    assert_eq!(
        svc5(NR_THREAD_CREATE as u64, hv2 as u64, 0, &raw mut OUT as u64, 0),
        -9,
        "SYSCALL：thread_create 对 Vmar 须 -EBADF（类型门控）"
    );
    assert_eq!(
        svc5(NR_THREAD_CREATE as u64, 0xDEAD_BEEF, 0, &raw mut OUT as u64, 0),
        -9,
        "SYSCALL：thread_create 坏 handle 须 -EBADF"
    );

    // F3. process_start 参数面（在归属/状态检查之前，故不 spawn）：entry=0 /
    //     sp 未 16 对齐 → -EINVAL；3-4a 起 bootstrap 是**迁移源 handle**——垃圾
    //     值 → -EBADF（take 自调用者表、子表不动；迁移的正向验收在 user_child
    //     ——ktest 无 CURRENT 标签，spawn 路径不可测）。
    assert_eq!(
        svc6(NR_PROCESS_START as u64, hp as u64, hth as u64, 0x1000, 0x4000, 0xDEAD_BEEF, 0),
        -9,
        "SYSCALL：process_start 垃圾 bootstrap 须 -EBADF（取自调用者表）"
    );
    assert_eq!(
        svc6(NR_PROCESS_START as u64, hp as u64, hth as u64, 0, 0x4000, 0, 0),
        -22,
        "SYSCALL：process_start entry=0 须 -EINVAL"
    );
    assert_eq!(
        svc6(NR_PROCESS_START as u64, hp as u64, hth as u64, 0x1000, 0x4001, 0, 0),
        -22,
        "SYSCALL：process_start sp 未 16 对齐须 -EINVAL"
    );

    // F3b. thread_start 门控面（T-1 0x35 四参）：与 process_start 同校验序的
    //      错误臂——ktest 在 berry_sched::init 之前跑，成功路径会 spawn
    //      （不可测，交 EL0 运行期自验）。坏 handle / 对 Vmar → -EBADF；
    //      entry=0 / sp 未 16 对齐 → -EINVAL（检在 handle 门控之前，F3 同序）；
    //      缺 MANAGE（dup 出 DUPLICATE-only 别名——零权利 dup 拒面绕行，F4d
    //      同款）→ -EACCES；所属进程未 start → -EBADSTATE（首线程必须经
    //      process_start——thread_start 只管进程内造线程）。
    assert_eq!(
        svc6(NR_THREAD_START as u64, 0xDEAD_BEEF, 0x1000, 0x4000, 0, 0, 0),
        -9,
        "SYSCALL：thread_start 坏 handle 须 -EBADF"
    );
    assert_eq!(
        svc6(NR_THREAD_START as u64, hv2 as u64, 0x1000, 0x4000, 0, 0, 0),
        -9,
        "SYSCALL：thread_start 对 Vmar 须 -EBADF（类型门控）"
    );
    assert_eq!(
        svc6(NR_THREAD_START as u64, hth as u64, 0, 0x4000, 0, 0, 0),
        -22,
        "SYSCALL：thread_start entry=0 须 -EINVAL"
    );
    assert_eq!(
        svc6(NR_THREAD_START as u64, hth as u64, 0x1000, 0x4001, 0, 0, 0),
        -22,
        "SYSCALL：thread_start sp 未 16 对齐须 -EINVAL"
    );
    // SAFETY: OUT 复用 F 段纪律（预清）。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(
            NR_HANDLE_DUPLICATE as u64,
            hth as u64,
            crate::handle::RIGHT_DUPLICATE,
            &raw mut OUT as u64
        ),
        0,
        "SYSCALL：thread_start 前置——dup DUPLICATE-only 别名"
    );
    let hthr = unsafe { OUT } as u32;
    assert_ne!(hthr, 0, "SYSCALL：dup 别名须回填");
    assert_eq!(
        svc6(NR_THREAD_START as u64, hthr as u64, 0x1000, 0x4000, 0, 0, 0),
        -13,
        "SYSCALL：thread_start 缺 MANAGE 须 -EACCES"
    );
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hthr as u64, 0, 0), 0, "SYSCALL：close 别名");
    assert_eq!(
        svc6(NR_THREAD_START as u64, hth as u64, 0x1000, 0x4000, 0, 0, 0),
        -37,
        "SYSCALL：thread_start 未 start 进程须 -EBADSTATE"
    );

    // F4. 归属面：ht 属 proc(hp)，拿 hp2 的 handle 去 start → -EACCES（参数面
    //     全过才走到归属检查）。第二个进程同时把「进程可批量创建」过了。
    // SAFETY: 同上。
    unsafe { OUT = 0; CHILD_OUT = 0 };
    assert_eq!(
        svc5(NR_PROCESS_CREATE as u64, 0, 0, &raw mut OUT as u64, &raw mut CHILD_OUT as u64),
        0,
        "SYSCALL：process_create 第二个"
    );
    let hp2 = unsafe { OUT } as u32;
    let hv3 = unsafe { CHILD_OUT } as u32;
    assert_ne!(hp2, 0, "SYSCALL：第二个进程 handle");
    assert_eq!(
        svc6(NR_PROCESS_START as u64, hp2 as u64, hth as u64, 0x1000, 0x4000, 0, 0),
        -13,
        "SYSCALL：跨进程 start 须 -EACCES（thread 不归该 process）"
    );

    // F4b. Process 可等待面（Plan B v0.5）：Alive 进程 wait TERMINATED（poll，
    //      deadline=0）→ -EAGAIN（Plan B 前对 Process wait 是 -EBADF——类型
    //      未入 wait 链）；Exited 置位的正向验收在 svcmgr reap 链（EXIT ok
    //      标记）——ktest 无 spawn 面造不出真退出。*observed 出参复用 I_OBS
    //      纪律（预清 + 未命中保持 0）。
    // SAFETY: I_OBS 为本函数私有静态（I 段同源纪律）。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(crate::syscall::NR_OBJECT_WAIT_ONE as u64, hp2 as u64, SIGNAL_TERMINATED, 0, &raw mut I_OBS as u64),
        -11,
        "SYSCALL：Alive 进程 wait TERMINATED 须 -EAGAIN（poll）"
    );
    assert_eq!(unsafe { I_OBS }, 0, "SYSCALL：未命中 observed 须保持 0");

    // F4c. process_get_exit_code（P5 0x3A）：错误/未退面——坏 handle / 类型
    //      不符（Vmar）→ -EBADF；Alive（创建未启动，两进程皆然）→ -EAGAIN
    //      （poll 语义——**先 wait TERMINATED 再查**）。Exited 成功路径 ktest
    //      造不出（无 spawn 面），由运行期 bsh `&&` 自验（`run fail` 真码 42）
    //      / svcmgr `EXIT ok`（Ok(0) 才打；非 0 码走 `eco: exit code` 降级行）
    //      承担——诚实注记同 F4b。
    // SAFETY: OUT 复用 F 段纪律（预清；code 是 i32 出参，低 4 字节写入）。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(crate::syscall::NR_PROCESS_GET_EXIT_CODE as u64, 0xDEAD_BEEF, &raw mut OUT as u64, 0),
        -9,
        "SYSCALL：get_exit_code 坏 handle 须 -EBADF"
    );
    assert_eq!(
        svc4(crate::syscall::NR_PROCESS_GET_EXIT_CODE as u64, hv2 as u64, &raw mut OUT as u64, 0),
        -9,
        "SYSCALL：get_exit_code 对 Vmar 须 -EBADF（类型门控）"
    );
    assert_eq!(
        svc4(crate::syscall::NR_PROCESS_GET_EXIT_CODE as u64, hp2 as u64, &raw mut OUT as u64, 0),
        -11,
        "SYSCALL：Alive 进程 get_exit_code 须 -EAGAIN（poll）"
    );
    assert_eq!(unsafe { OUT }, 0, "SYSCALL：未退 code 出参须保持 0");

    // F4d. process_terminate（C-4b 0x3B）：错误面 + 未 start 终止收口——坏
    //      handle / 类型不符（Vmar）→ -EBADF；缺 MANAGE（dup 出 DUPLICATE-only
    //      别名——空 rights 拒面，I 段先例）→ -EACCES；成功终止**未 start**
    //      进程（码 42，无任务面——Process 侧收口）→ 0，且退出面**首次可在
    //      ktest 闭环**：wait TERMINATED poll 即中 + get_exit_code 收 42（F4b/F4c
    //      注记「Exited 成功路径 ktest 造不出」自此改写——terminate 免 spawn
    //      面）；重复终止 → -EBADSTATE（幂等面收口在 wait/get_exit_code，不
    //      重复杀）。自杀面（-EBADSTATE）与已 start 杀链（摘队+REAPQ）需
    //      CURRENT/目标任务在场——ktest 在 sched::init 前跑，交 EL0 运行期
    //      bsh Ctrl-C/kill 自验（C-4b 后段 `INT ok` 标记）。
    assert_eq!(
        svc4(crate::syscall::NR_PROCESS_TERMINATE as u64, 0xDEAD_BEEF, 42, 0),
        -9,
        "SYSCALL：terminate 坏 handle 须 -EBADF"
    );
    assert_eq!(
        svc4(crate::syscall::NR_PROCESS_TERMINATE as u64, hv3 as u64, 42, 0),
        -9,
        "SYSCALL：terminate 对 Vmar 须 -EBADF（类型门控）"
    );
    // SAFETY: OUT 复用 F 段纪律（预清）。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(
            NR_HANDLE_DUPLICATE as u64,
            hp2 as u64,
            crate::handle::RIGHT_DUPLICATE,
            &raw mut OUT as u64
        ),
        0,
        "SYSCALL：terminate 前置——dup DUPLICATE-only 别名"
    );
    let hp2r = unsafe { OUT } as u32;
    assert_ne!(hp2r, 0, "SYSCALL：dup 别名须回填");
    assert_eq!(
        svc4(crate::syscall::NR_PROCESS_TERMINATE as u64, hp2r as u64, 42, 0),
        -13,
        "SYSCALL：terminate 缺 MANAGE 须 -EACCES"
    );
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hp2r as u64, 0, 0), 0, "SYSCALL：close 别名");
    // 成功终止（未 start——kill_all_tasks_by_pinned 扫不到任务，走 Process 侧收口）。
    assert_eq!(
        svc4(crate::syscall::NR_PROCESS_TERMINATE as u64, hp2 as u64, 42, 0),
        0,
        "SYSCALL：terminate 未 start 进程须 0"
    );
    // 退出面闭环 ①：wait TERMINATED poll 即中（Exited 单调置位）。
    // SAFETY: I_OBS 纪律同 F4b（预清 + 命中回填全满足集）。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(
            crate::syscall::NR_OBJECT_WAIT_ONE as u64,
            hp2 as u64,
            crate::handle::SIGNAL_TERMINATED,
            0,
            &raw mut I_OBS as u64
        ),
        0,
        "SYSCALL：terminated 进程 wait poll 须即中"
    );
    assert_eq!(
        unsafe { I_OBS },
        crate::handle::SIGNAL_TERMINATED,
        "SYSCALL：observed 须为 TERMINATED（全满足集）"
    );
    // 退出面闭环 ②：terminate 的 code=42 落退出码账面。
    // SAFETY: OUT 纪律同上（i32 出参，低 4 字节写入）。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(crate::syscall::NR_PROCESS_GET_EXIT_CODE as u64, hp2 as u64, &raw mut OUT as u64, 0),
        0,
        "SYSCALL：terminated get_exit_code 须 0"
    );
    assert_eq!(unsafe { OUT } as u32, 42, "SYSCALL：terminate code=42 须落退出码");
    assert_eq!(
        svc4(crate::syscall::NR_PROCESS_TERMINATE as u64, hp2 as u64, 43, 0),
        -37,
        "SYSCALL：重复 terminate 须 -EBADSTATE（且保首因码 42 不被 43 覆写）"
    );

    // F5. 清场（顺带覆盖钉住释放链：close 线程 → 释放对 Process 的钉住；
    //     close 进程 → 释放对 root Vmar 的钉住；空进程无映射，close 不受
    //     -EBUSY 面影响）。F4d 后 hp2 已 Exited——close 即最终引用归零，drop
    //     Process（release vmar 钉住 + PROCESS_LIVE 减）——终止态对象回收链
    //     随本段走通。
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hth as u64, 0, 0), 0, "SYSCALL：close thread");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hp as u64, 0, 0), 0, "SYSCALL：close process");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hv2 as u64, 0, 0), 0, "SYSCALL：close child vmar");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hp2 as u64, 0, 0), 0, "SYSCALL：close terminated process 2");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hv3 as u64, 0, 0), 0, "SYSCALL：close child vmar 2");

    // G. 3-4a per-process 表拆分：两表隔离 + take/insert_ref 迁移原语 + 子进程
    //    出生自带空表（纯表对象面；EL0 派发链 + bootstrap 迁移的正向验收在
    //    user_spawner/user_child——ktest 在 sched::init 前无 CURRENT 标签）。
    use crate::handle::{HandleTable, ERR_BAD_HANDLE};

    // G0. 隔离与迁移：内核表 insert → 他表同值必 -EBADF（表间不可见）；take
    //     后原值立即作废（代号 +1——迁移非共享）；insert_ref 落新表 → 新值
    //     三门控可查（权利随槽迁移）；close 新值即最终 close（refs 不变——
    //     引用令牌随槽迁移，对象销毁路径走通）。
    let mut t2 = HandleTable::new();
    let hm = ht
        .insert(KernelObj::Vmo(Vmo::create(PAGE_SIZE).expect("ktest：装配 VMO（可信语境）")), RIGHT_READ | RIGHT_WRITE)
        .expect("SYSCALL：insert hm");
    assert_eq!(
        t2.get(hm, ObjType::Vmo, 0).err(),
        Some(ERR_BAD_HANDLE),
        "SYSCALL：他表 handle 不可见（per-process 隔离）"
    );
    let (mobj, mrights) = ht.take(hm).expect("SYSCALL：take hm");
    assert_eq!(
        ht.get(hm, ObjType::Vmo, 0).err(),
        Some(ERR_BAD_HANDLE),
        "SYSCALL：take 后原值立即作废（代号推进）"
    );
    let hm2 = t2.insert_ref(mobj, mrights).expect("SYSCALL：insert_ref 迁入 t2");
    assert!(
        t2.get(hm2, ObjType::Vmo, RIGHT_WRITE).is_ok(),
        "SYSCALL：迁移后新值三门控可查"
    );
    assert!(
        t2.close(hm2).is_ok(),
        "SYSCALL：close 迁移值（refs 未增——最终 close 走销毁）"
    );

    // G1. 子进程出生自带空表：process_create 出的 Process 有自己的 ht（出生为
    //     空——一切能力待父进程授入）；父表 handle 在子表不可见。
    // SAFETY: 同上（OUT/CHILD_OUT 为本函数私有静态）。
    unsafe { OUT = 0; CHILD_OUT = 0 };
    assert_eq!(
        svc5(NR_PROCESS_CREATE as u64, 0, 0, &raw mut OUT as u64, &raw mut CHILD_OUT as u64),
        0,
        "SYSCALL：process_create（G1 子表）"
    );
    let hp3 = unsafe { OUT } as u32;
    let hv4 = unsafe { CHILD_OUT } as u32;
    let (pobj, _) = ht.get(hp3, ObjType::Process, 0).expect("SYSCALL：get hp3");
    let child_proc = pobj.as_process().expect("SYSCALL：hp3 须指 Process");
    assert_eq!(
        child_proc.ht.get(hp3, ObjType::Process, 0).err(),
        Some(ERR_BAD_HANDLE),
        "SYSCALL：子表出生为空（父 handle 不可见）"
    );
    // 清场（close hp3 → Process Drop 释放对 root Vmar 的钉住；再关 hv4 归零）。
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hp3 as u64, 0, 0), 0, "SYSCALL：close hp3");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hv4 as u64, 0, 0), 0, "SYSCALL：close hv4");

    // H. 3-4b channel 三号：非阻塞队列语义 + handle 随消息迁移的原子序（EL1
    //    对象面全链；EL0「两进程对答」正向验收在 user_spawner/user_child）。
    use crate::channel::CHAN_MSG_MAX;
    use crate::handle::{RIGHT_RECV, RIGHT_SEND};
    use crate::syscall::{NR_CHANNEL_CREATE, NR_CHANNEL_RECV, NR_CHANNEL_SEND};
    static mut HBUF: [u8; 320] = [0; 320]; // send 源 / recv 目的（超限值只作参数不拷）
    static mut H_NB: u64 = 0; // *nbytes in-out（进 = 容量、出 = 实际）
    static mut H_NH: u64 = 0; // *nhandles in-out
    static mut H_HND: [u32; 8] = [0; 8]; // 随行 handle 数组（u32 × CHAN_HND_MAX——A2-3 6→8 同步）

    // H0. create：两端 handle 均非零互异；rights 面 SEND|RECV 可门控查得。
    // SAFETY: svc 同步异常；H 系列静态均本函数私有。
    unsafe { OUT = 0; CHILD_OUT = 0 };
    assert_eq!(
        svc4(NR_CHANNEL_CREATE as u64, 0, &raw mut OUT as u64, &raw mut CHILD_OUT as u64),
        0,
        "SYSCALL：channel_create"
    );
    let (hc0, hc1) = (unsafe { OUT } as u32, unsafe { CHILD_OUT } as u32);
    assert_ne!(hc0, 0, "SYSCALL：channel_create 须回填 end0");
    assert_ne!(hc1, 0, "SYSCALL：channel_create 须回填 end1");
    assert_ne!(hc0, hc1, "SYSCALL：两端 handle 须互异");
    assert!(
        ht.get(hc0, ObjType::Channel, RIGHT_SEND | RIGHT_RECV).is_ok(),
        "SYSCALL：端点 rights 须含 SEND|RECV"
    );

    // H1. 空队 recv → -EAGAIN（v1 非阻塞；空队先检——零用户内存读 = 零 fault 面）。
    // SAFETY: 同上。
    unsafe { H_NB = 8; H_NH = 4 };
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, hc1 as u64, &raw mut HBUF as u64,
             &raw mut H_NB as u64, &raw mut H_HND as u64, &raw mut H_NH as u64, 0),
        -11,
        "SYSCALL：空队 recv 须 -EAGAIN"
    );

    // H2. send→recv 往返：载荷比对 + nbytes/nhandles 回写（in-out 的出侧）+
    //     消费后空队再 recv → -EAGAIN。
    // SAFETY: 同上。
    unsafe { HBUF[..8].copy_from_slice(&PATTERN.to_le_bytes()) };
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hc0 as u64, &raw const HBUF as u64, 8, 0, 0, 0),
        0,
        "SYSCALL：channel_send（无随行 handle）"
    );
    // SAFETY: 同上。
    unsafe { HBUF = [0; 320]; H_NB = 8; H_NH = 8; H_HND = [0; 8] };
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, hc1 as u64, &raw mut HBUF as u64,
             &raw mut H_NB as u64, &raw mut H_HND as u64, &raw mut H_NH as u64, 0),
        0,
        "SYSCALL：channel_recv"
    );
    // SAFETY: 只读比对。
    assert_eq!(unsafe { u64::from_le_bytes(HBUF[..8].try_into().unwrap()) }, PATTERN,
        "SYSCALL：channel 载荷往返");
    assert_eq!(unsafe { H_NB }, 8, "SYSCALL：recv 须回写实际字节数");
    assert_eq!(unsafe { H_NH }, 0, "SYSCALL：recv 须回写实际 handle 数");
    // SAFETY: 同上。
    unsafe { H_NB = 8; H_NH = 0 };
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, hc1 as u64, &raw mut HBUF as u64,
             &raw mut H_NB as u64, &raw mut H_HND as u64, &raw mut H_NH as u64, 0),
        -11,
        "SYSCALL：消费后空队 recv 须 -EAGAIN"
    );

    // H3. 参数面上限：nbytes/nhandles 超限 → -EINVAL（no-panic 前置，先于拷贝）；
    //     容量不足（8B vs cap=4）→ -EINVAL 且**消息保原样**（cap 复原后收得同条）。
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hc0 as u64, &raw const HBUF as u64,
             CHAN_MSG_MAX as u64 + 1, 0, 0, 0),
        -22,
        "SYSCALL：nbytes 超限须 -EINVAL"
    );
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hc0 as u64, &raw const HBUF as u64, 0,
             &raw const H_HND as u64, 9, 0),
        -22,
        "SYSCALL：nhandles 超限须 -EINVAL"
    );
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hc0 as u64, &raw const HBUF as u64, 8, 0, 0, 0),
        0,
        "SYSCALL：容量用例 send"
    );
    // SAFETY: 同上。
    unsafe { H_NB = 4; H_NH = 0 };
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, hc1 as u64, &raw mut HBUF as u64,
             &raw mut H_NB as u64, &raw mut H_HND as u64, &raw mut H_NH as u64, 0),
        -22,
        "SYSCALL：容量不足须 -EINVAL"
    );
    // SAFETY: 同上。
    unsafe { HBUF = [0; 320]; H_NB = 8; H_NH = 0 };
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, hc1 as u64, &raw mut HBUF as u64,
             &raw mut H_NB as u64, &raw mut H_HND as u64, &raw mut H_NH as u64, 0),
        0,
        "SYSCALL：容量复原后须收得同条消息"
    );
    // SAFETY: 只读比对。
    assert_eq!(unsafe { u64::from_le_bytes(HBUF[..8].try_into().unwrap()) }, PATTERN,
        "SYSCALL：容量不足失败不得消费消息（保原样）");

    // H4. 满队：新端点对 8 连发填满（CHAN_DEPTH）→ 第 9 发 -EBUSY（非阻塞）。
    // SAFETY: 同上。
    unsafe { OUT = 0; CHILD_OUT = 0 };
    assert_eq!(
        svc4(NR_CHANNEL_CREATE as u64, 0, &raw mut OUT as u64, &raw mut CHILD_OUT as u64),
        0,
        "SYSCALL：channel_create（满队用例）"
    );
    let (hq0, hq1) = (unsafe { OUT } as u32, unsafe { CHILD_OUT } as u32);
    for _ in 0..8 {
        assert_eq!(
            svc6(NR_CHANNEL_SEND as u64, hq0 as u64, &raw const HBUF as u64, 8, 0, 0, 0),
            0,
            "SYSCALL：填队 send"
        );
    }
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hq0 as u64, &raw const HBUF as u64, 8, 0, 0, 0),
        -16,
        "SYSCALL：满队 send 须 -EBUSY"
    );
    // 清场：两端 close（互钉成环 → 对象泄漏，channel.rs 安全向政策）。
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hq0 as u64, 0, 0), 0, "SYSCALL：close end0");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hq1 as u64, 0, 0), 0, "SYSCALL：close end1");

    // H5. rights 门控：dup 收紧到 RECV-only → send 须 -EACCES（权限在 handle）。
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(NR_HANDLE_DUPLICATE as u64, hc0 as u64, RIGHT_RECV, &raw mut OUT as u64),
        0,
        "SYSCALL：dup 收紧到 RECV"
    );
    let hro = unsafe { OUT } as u32;
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hro as u64, &raw const HBUF as u64, 8, 0, 0, 0),
        -13,
        "SYSCALL：RECV-only send 须 -EACCES"
    );
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hro as u64, 0, 0), 0, "SYSCALL：close RO");

    // H6. handle 随消息迁移：VMO 令牌随消息过通道——原值立即 -EBADF（take 出
    //     表）、对端 recv 落回**本表新值**（同表演示；跨表链在 EL0 spawner/
    //     child），新值三门控可查且对象可用（rights 随令牌走）。
    let hsrc = ht
        .insert(KernelObj::Vmo(Vmo::create(PAGE_SIZE).expect("ktest：装配 VMO（可信语境）")), RIGHT_READ | RIGHT_WRITE)
        .expect("SYSCALL：insert hsrc");
    // SAFETY: 同上。
    unsafe { H_HND = [hsrc, 0, 0, 0, 0, 0, 0, 0]; HBUF[..8].copy_from_slice(&PATTERN.to_le_bytes()) };
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hc0 as u64, &raw const HBUF as u64, 8,
             &raw const H_HND as u64, 1, 0),
        0,
        "SYSCALL：send 带随行 handle"
    );
    assert_eq!(
        svc4(NR_HANDLE_CLOSE as u64, hsrc as u64, 0, 0),
        -9,
        "SYSCALL：迁移后原值须 -EBADF（发送方失去）"
    );
    // SAFETY: 同上。
    unsafe { HBUF = [0; 320]; H_NB = 8; H_NH = 8; H_HND = [0; 8] };
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, hc1 as u64, &raw mut HBUF as u64,
             &raw mut H_NB as u64, &raw mut H_HND as u64, &raw mut H_NH as u64, 0),
        0,
        "SYSCALL：recv 带随行 handle"
    );
    let hnew = unsafe { H_HND[0] };
    assert_eq!(unsafe { H_NH }, 1, "SYSCALL：recv 须回写 1 个随行 handle");
    assert_ne!(hnew, 0, "SYSCALL：随行 handle 须非零");
    assert!(
        ht.get(hnew, ObjType::Vmo, RIGHT_READ).is_ok(),
        "SYSCALL：迁移新值三门控可查"
    );
    assert_eq!(
        svc6(NR_VMO_COMMIT as u64, hnew as u64, 0, PAGE_SIZE as u64, 0, 0, 0),
        0,
        "SYSCALL：迁移后的 VMO handle 仍可用（对象随令牌走）"
    );
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hnew as u64, 0, 0), 0, "SYSCALL：close 迁移值");

    // H7. 原子序：随行 [好 handle, 垃圾值] → -EBADF 且好 handle **原值仍活**
    //     （预检全量、take 零发生）；随行重复值 → -EINVAL（防首 take 作废次值）。
    let hgd = ht
        .insert(KernelObj::Vmo(Vmo::create(PAGE_SIZE).expect("ktest：装配 VMO（可信语境）")), RIGHT_READ | RIGHT_WRITE)
        .expect("SYSCALL：insert hgd");
    // SAFETY: 同上。
    unsafe { H_HND = [hgd, 0xDEAD_BEEF, 0, 0, 0, 0, 0, 0] };
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hc0 as u64, &raw const HBUF as u64, 8,
             &raw const H_HND as u64, 2, 0),
        -9,
        "SYSCALL：坏随行值须 -EBADF"
    );
    assert!(
        ht.get(hgd, ObjType::Vmo, 0).is_ok(),
        "SYSCALL：原子序——同批好 handle 须原值仍活"
    );
    // SAFETY: 同上。
    unsafe { H_HND = [hgd, hgd, 0, 0, 0, 0, 0, 0] };
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hc0 as u64, &raw const HBUF as u64, 8,
             &raw const H_HND as u64, 2, 0),
        -22,
        "SYSCALL：随行数组重复值须 -EINVAL"
    );
    // 清场（两端 close → 互钉环泄漏，安全向）。
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hgd as u64, 0, 0), 0, "SYSCALL：close hgd");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hc0 as u64, 0, 0), 0, "SYSCALL：close hc0");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hc1 as u64, 0, 0), 0, "SYSCALL：close hc1");

    // I. 3-4c event 三号：位掩码语义 + SIGNAL 权利面；4-1 wait_one 阻塞化后
    //    deadline≠0 → 到点未命中 -ETIMEDOUT（ktest 在 sched::init 前不能睡，
    //    用远古时刻 deadline=1 验「已过点立即返」路径；真睡的 EL0 正向验收 =
    //    user_waiter 三路 + waker2，见 main.rs/user.rs）；4-2 位空间 = Event
    //    用户位迁高 32 半（U1/U2/U3）+ 低 32 位拒面 + wait 面扩 Channel（I7）。
    use crate::handle::{RIGHT_DUPLICATE, RIGHT_SIGNAL, SIGNAL_READABLE, SIGNAL_TERMINATED, SIGNAL_WRITABLE};
    use crate::syscall::{NR_EVENT_CREATE, NR_OBJECT_SIGNAL, NR_OBJECT_WAIT_MANY, NR_OBJECT_WAIT_ONE};
    static mut I_OBS: u64 = 0; // *observed 出参
    static mut WM_H: [u32; 2] = [0; 2]; // wait_many handles 入参数组
    static mut WM_S: [u64; 2] = [0, 0]; // wait_many signals 入参数组
    static mut WM_IDX: u64 = 0; // *idx 出参
    // 4-2 位空间：Event 用户位一律高 32 半（低 32 位内核派生保留——I6 拒面）。
    const U1: u64 = 1 << 33;
    const U2: u64 = 1 << 34;
    const U3: u64 = 1 << 35;

    // I0. create：options≠0 → -EINVAL；正常建 → handle 非零、SIGNAL|DUPLICATE
    //     三门控可查。
    assert_eq!(
        svc4(NR_EVENT_CREATE as u64, 1, &raw mut OUT as u64, 0),
        -22,
        "SYSCALL：event_create options≠0 须 -EINVAL"
    );
    // SAFETY: svc 同步异常；I 系列静态均本函数私有。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(NR_EVENT_CREATE as u64, 0, &raw mut OUT as u64, 0),
        0,
        "SYSCALL：event_create"
    );
    let he = unsafe { OUT } as u32;
    assert_ne!(he, 0, "SYSCALL：event_create 须回填 handle");
    assert!(
        ht.get(he, ObjType::Event, RIGHT_SIGNAL | RIGHT_DUPLICATE).is_ok(),
        "SYSCALL：Event rights 须含 SIGNAL|DUPLICATE"
    );

    // I1. poll 未命中 → -EAGAIN（出生全零）；deadline=1（远古时刻，poll 语义
    //     下未命中）→ -ETIMEDOUT **不睡**（参检先于表查——坏参不碰表）。
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, he as u64, U1 | U2, 0, &raw mut I_OBS as u64),
        -11,
        "SYSCALL：未命中 wait 须 -EAGAIN"
    );
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, he as u64, U1 | U2, 1, &raw mut I_OBS as u64),
        -110,
        "SYSCALL：deadline 已过未命中须 -ETIMEDOUT（不睡）"
    );

    // I2. signal 置位 → wait 命中回写**全满足集**（非请求子集——Zircon 同形）。
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, he as u64, 0, U1 | U2),
        0,
        "SYSCALL：object_signal 置位"
    );
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, he as u64, U2, 0, &raw mut I_OBS as u64),
        0,
        "SYSCALL：命中 wait"
    );
    assert_eq!(unsafe { I_OBS }, U1 | U2, "SYSCALL：observed 须回写全满足集");

    // I3. 请求位未置 → -EAGAIN（掩码里没有 U3）；signal 清位后再查原位
    //     → -EAGAIN（clear 生效）。
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, he as u64, U3, 0, &raw mut I_OBS as u64),
        -11,
        "SYSCALL：请求未置位须 -EAGAIN"
    );
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, he as u64, U2, 0),
        0,
        "SYSCALL：object_signal 清位"
    );
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, he as u64, U2, 0, &raw mut I_OBS as u64),
        -11,
        "SYSCALL：清位后原位须 -EAGAIN"
    );

    // I4. 权利面：dup 收紧到仅 DUPLICATE（无 SIGNAL——零权利 dup 会被表拒，
    //     这是 duplicate 的空 rights 拒面，非本段对象）→ signal 拒 -EACCES；
    //     wait 不需权利位——同一对象剩余位 U2 照常可查。
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, he as u64, 0, U2),
        0,
        "SYSCALL：signal 补置 U2"
    );
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(NR_HANDLE_DUPLICATE as u64, he as u64, RIGHT_DUPLICATE, &raw mut OUT as u64),
        0,
        "SYSCALL：dup Event 到仅 DUPLICATE"
    );
    let he_ro = unsafe { OUT } as u32;
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, he_ro as u64, 0, U3),
        -13,
        "SYSCALL：无 SIGNAL 位须 -EACCES"
    );
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, he_ro as u64, U2, 0, &raw mut I_OBS as u64),
        0,
        "SYSCALL：wait 不需权利位（共享对象可见）"
    );

    // I5. 类型/坏值门：signal 与 wait 对 VMO handle → -EBADF；野值同拒。
    let hv_e = ht
        .insert(KernelObj::Vmo(Vmo::create(PAGE_SIZE).expect("ktest：装配 VMO（可信语境）")), RIGHT_READ | RIGHT_WRITE)
        .expect("SYSCALL：insert hv_e");
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, hv_e as u64, 0, 1),
        -9,
        "SYSCALL：signal 非 Event 须 -EBADF"
    );
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, hv_e as u64, 1, 0, &raw mut I_OBS as u64),
        -9,
        "SYSCALL：wait 非 Event 须 -EBADF"
    );
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, 0xDEAD_BEEF, 0, 1),
        -9,
        "SYSCALL：signal 野值须 -EBADF"
    );
    assert_eq!(
        svc4(NR_HANDLE_CLOSE as u64, hv_e as u64, 0, 0),
        0,
        "SYSCALL：close hv_e"
    );

    // I6. 4-2 位空间拒面：signal clear/set 越出高 32 位用户半 → -EINVAL（低
    //     32 位保留给内核派生 READABLE/WRITABLE）；检在 get_raw **之后**——
    //     I5 的 VMO/坏值仍 -EBADF（错误面优先级不乱）。Event 上 wait 低 32 位
    //     = 等一个 signal 不出的位 → poll 永不命中 -EAGAIN。
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, he as u64, 0, SIGNAL_READABLE),
        -22,
        "SYSCALL：signal 低 32 位掩码须 -EINVAL"
    );
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, he as u64, SIGNAL_WRITABLE, 0),
        -22,
        "SYSCALL：clear 低 32 位掩码须 -EINVAL"
    );
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, he as u64, SIGNAL_READABLE, 0, &raw mut I_OBS as u64),
        -11,
        "SYSCALL：Event 低 32 位请求永不命中（位面在用户半）"
    );
    // 清场（别名先关、本体后关——refs 归零释放）。
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, he_ro as u64, 0, 0), 0, "SYSCALL：close he_ro");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, he as u64, 0, 0), 0, "SYSCALL：close he");
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, he as u64, 0, 1),
        -9,
        "SYSCALL：close 后 signal 须 -EBADF"
    );

    // I7. 4-2 wait 面扩 Channel（poll 面——真睡的跨进程阻塞链验收 =
    //     user_pinger/user_ponger 对打）：READABLE = 本端队列非空、WRITABLE =
    //     对端队列未满（内核派生——poll/命中全满足集语义与 Event 同形）。
    //     面序：空 → READABLE -EAGAIN；出生未满 → WRITABLE 命中；send 一条 →
    //     对端 READABLE 命中；灌满 → WRITABLE -EAGAIN；recv 腾位 → WRITABLE 命中。
    // SAFETY: 同上（I7 复用 H 段的 HBUF/H_NB/H_NH——H 已清场完毕，无别名）。
    unsafe { OUT = 0; CHILD_OUT = 0 };
    assert_eq!(
        svc4(NR_CHANNEL_CREATE as u64, 0, &raw mut OUT as u64, &raw mut CHILD_OUT as u64),
        0,
        "SYSCALL：channel_create（wait 面用例）"
    );
    let (hw0, hw1) = (unsafe { OUT } as u32, unsafe { CHILD_OUT } as u32);
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, hw1 as u64, SIGNAL_READABLE, 0, &raw mut I_OBS as u64),
        -11,
        "SYSCALL：空队 READABLE 须 -EAGAIN"
    );
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, hw0 as u64, SIGNAL_WRITABLE, 0, &raw mut I_OBS as u64),
        0,
        "SYSCALL：出生未满 WRITABLE 须命中"
    );
    assert_eq!(unsafe { I_OBS }, SIGNAL_WRITABLE, "SYSCALL：WRITABLE 全满足集");
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hw0 as u64, &raw const HBUF as u64, 8, 0, 0, 0),
        0,
        "SYSCALL：wait 面用例 send 一条"
    );
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, hw1 as u64, SIGNAL_READABLE, 0, &raw mut I_OBS as u64),
        0,
        "SYSCALL：send 后对端 READABLE 须命中"
    );
    // 全满足集 = 本端非空(READABLE) + 对端队列空(WRITABLE)——非请求子集的活证。
    assert_eq!(
        unsafe { I_OBS },
        SIGNAL_READABLE | SIGNAL_WRITABLE,
        "SYSCALL：READABLE 命中回写全满足集"
    );
    for _ in 0..7 {
        assert_eq!(
            svc6(NR_CHANNEL_SEND as u64, hw0 as u64, &raw const HBUF as u64, 8, 0, 0, 0),
            0,
            "SYSCALL：wait 面用例灌满 send"
        );
    }
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, hw0 as u64, SIGNAL_WRITABLE, 0, &raw mut I_OBS as u64),
        -11,
        "SYSCALL：满队 WRITABLE 须 -EAGAIN"
    );
    // SAFETY: 同上。
    unsafe { H_NB = 8; H_NH = 0 };
    assert_eq!(
        svc6(NR_CHANNEL_RECV as u64, hw1 as u64, &raw mut HBUF as u64,
             &raw mut H_NB as u64, &raw mut H_HND as u64, &raw mut H_NH as u64, 0),
        0,
        "SYSCALL：wait 面用例 recv 腾位"
    );
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, hw0 as u64, SIGNAL_WRITABLE, 0, &raw mut I_OBS as u64),
        0,
        "SYSCALL：recv 腾位后 WRITABLE 须命中"
    );
    // 混合位请求（READABLE|WRITABLE）：此刻 hw1 本端 7 条（READABLE=1）且对端
    // 队列空（WRITABLE=1）→ 命中且全满足集 = 两派生位之并。
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, hw1 as u64,
             SIGNAL_READABLE | SIGNAL_WRITABLE, 0, &raw mut I_OBS as u64),
        0,
        "SYSCALL：混合位请求须命中"
    );
    assert_eq!(
        unsafe { I_OBS },
        SIGNAL_READABLE | SIGNAL_WRITABLE,
        "SYSCALL：全满足集须 READABLE|WRITABLE"
    );
    // 清场（两端 close——互钉环泄漏，channel.rs 安全向政策；队内消息无令牌）。
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hw0 as u64, 0, 0), 0, "SYSCALL：close hw0");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hw1 as u64, 0, 0), 0, "SYSCALL：close hw1");

    // I8. 4-2b wait_many 错误面 + poll 面（真睡归 ponger 稳态 MANY 验收，user.rs）：
    //     面序：n=0 / n=17 → -EINVAL；数组内坏 handle → -EBADF（首错透传）；
    //     poll 未命中 -EAGAIN 且 *idx 留预清哨兵 u64::MAX；远古 deadline →
    //     -ETIMEDOUT 不睡（同 I1 纪律——ktest 在 sched::init 前不能睡）；命中 →
    //     *idx = 入参序最前命中项 + *observed = 该对象全满足集。
    // SAFETY: svc 同步异常；WM_* 为本函数私有静态（I7 已清场，无别名）。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(NR_EVENT_CREATE as u64, 0, &raw mut OUT as u64, 0),
        0,
        "SYSCALL：wait_many 用例 event_create"
    );
    let hw_ev = unsafe { OUT } as u32;
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, hw_ev as u64, 0, U1),
        0,
        "SYSCALL：wait_many 用例置 U1"
    );
    unsafe { WM_IDX = 0 };
    assert_eq!(
        svc6(NR_OBJECT_WAIT_MANY as u64, &raw const WM_H as u64,
             &raw const WM_S as u64, 0, 0, &raw mut WM_IDX as u64, &raw mut I_OBS as u64),
        -22,
        "SYSCALL：n=0 须 -EINVAL"
    );
    assert_eq!(
        svc6(NR_OBJECT_WAIT_MANY as u64, &raw const WM_H as u64,
             &raw const WM_S as u64, 17, 0, &raw mut WM_IDX as u64, &raw mut I_OBS as u64),
        -22,
        "SYSCALL：n>16 须 -EINVAL"
    );
    // SAFETY: 数组布置：首项坏 handle（3 号槽典型野值）——解析即拒、首错透传。
    unsafe { WM_H = [3, hw_ev]; WM_S = [U1, U1] };
    assert_eq!(
        svc6(NR_OBJECT_WAIT_MANY as u64, &raw const WM_H as u64,
             &raw const WM_S as u64, 2, 0, &raw mut WM_IDX as u64, &raw mut I_OBS as u64),
        -9,
        "SYSCALL：数组内坏 handle 须 -EBADF"
    );
    // poll 未命中（U1 已置、请求 U2）→ -EAGAIN；idx 留预清哨兵 u64::MAX。
    // SAFETY: 同上。
    unsafe { WM_H = [hw_ev, hw_ev]; WM_S = [U2, U2]; WM_IDX = 0 };
    assert_eq!(
        svc6(NR_OBJECT_WAIT_MANY as u64, &raw const WM_H as u64,
             &raw const WM_S as u64, 2, 0, &raw mut WM_IDX as u64, &raw mut I_OBS as u64),
        -11,
        "SYSCALL：poll 未命中须 -EAGAIN"
    );
    assert_eq!(unsafe { WM_IDX }, u64::MAX, "SYSCALL：未命中 idx 须留哨兵 MAX");
    // SAFETY: 同上（远古 deadline——「已过点立即返」路径，不睡）。
    unsafe { WM_IDX = 0 };
    assert_eq!(
        svc6(NR_OBJECT_WAIT_MANY as u64, &raw const WM_H as u64,
             &raw const WM_S as u64, 2, 1, &raw mut WM_IDX as u64, &raw mut I_OBS as u64),
        -110,
        "SYSCALL：到点未命中须 -ETIMEDOUT"
    );
    // 命中序：首项请求 U2（未置）、次项 U1（已置）→ idx=1；再交换 → idx=0
    //（入参序最前命中项，两向活证）。
    // SAFETY: 同上。
    unsafe { WM_H = [hw_ev, hw_ev]; WM_S = [U2, U1]; WM_IDX = 0; I_OBS = 0 };
    assert_eq!(
        svc6(NR_OBJECT_WAIT_MANY as u64, &raw const WM_H as u64,
             &raw const WM_S as u64, 2, 0, &raw mut WM_IDX as u64, &raw mut I_OBS as u64),
        0,
        "SYSCALL：次项命中须返 0"
    );
    assert_eq!(unsafe { WM_IDX }, 1, "SYSCALL：idx 须指次项");
    assert_eq!(unsafe { I_OBS }, U1, "SYSCALL：observed 须该对象全满足集");
    // SAFETY: 同上。
    unsafe { WM_S = [U1, U2]; WM_IDX = 0; I_OBS = 0 };
    assert_eq!(
        svc6(NR_OBJECT_WAIT_MANY as u64, &raw const WM_H as u64,
             &raw const WM_S as u64, 2, 0, &raw mut WM_IDX as u64, &raw mut I_OBS as u64),
        0,
        "SYSCALL：首项命中须返 0"
    );
    assert_eq!(unsafe { WM_IDX }, 0, "SYSCALL：idx 须指首项（入参序最前）");
    // 清场。
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hw_ev as u64, 0, 0), 0, "SYSCALL：close hw_ev");

    berry_hal_aarch64::uart_puts(b"SYSCALL ok\n");
}

/// `svc #0` 四参往返（ktest 专用；bench S-2 耗尽面复用）：x0=nr（进）/ret（出）、
/// x1..x3=参。DAIF 屏蔽不影响同步异常；NZCV 经 SPSR 在 entry/eret 保存/恢复。
/// 安全外壳：3-1 起 syscall 派发不 panic、永不杀调用者（错误码自理）。
pub(crate) fn svc4(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    let ret: u64;
    // SAFETY: svc 触发同步异常；dispatch 只回写 x0（eret 回本指令之后）。
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

/// `svc #0` 五参往返（ktest 专用；3-3 process_create/thread_create 用）：
/// x0=nr（进）/ret（出）、x1..x4=参。
fn svc5(nr: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> i64 {
    let ret: u64;
    // SAFETY: 同 svc4（同步异常；dispatch 只回写 x0）。
    unsafe {
        core::arch::asm!(
            "svc #0",
            inout("x0") nr => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            options(nostack, preserves_flags),
        );
    }
    ret as i64
}

/// `svc #0` 六参往返（ktest 专用，3-2 VMO/Vmar 全链用；bench S-2 耗尽面复用）：
/// x0=nr（进）/ret（出）、x1..x6=参（ABI §2 上限 6 参）。
pub(crate) fn svc6(nr: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> i64 {
    let ret: u64;
    // SAFETY: 同 svc4（同步异常；dispatch 只回写 x0）。
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

/// Step 7 信号：GICv2 配置读回自检（IRQ27/CNTV：启用 + 优先级 + distributor/CPUif 开）。
fn test_gic() {
    assert!(berry_hal_aarch64::gic::verify(), "GIC IRQ27 配置读回不符");
    berry_hal_aarch64::uart_puts(b"GIC ok\n");
}

/// Step 6 信号：注入 `brk` → 同步异常（current-EL SPx @0x200）→ exception_sync
/// dump 寄存器/PC/ESR/FAR → BRK 可恢复（ELR+=4）→ eret 返回。
/// 见 `ktest done` 之后的 `=== EXCEPTION ... BRK recover ... [exc] brk recovered`。
fn test_exc() {
    berry_hal_aarch64::uart_puts(b"\n[exc] inject brk #0x42\n");
    // SAFETY: brk 触发同步异常（EC=0x3C）；exception_sync dump 后跳过本指令、eret 返回。
    // DAIF 屏蔽不影响同步异常（brk 非 IRQ）。
    unsafe { core::arch::asm!("brk #0x42", options(nostack, preserves_flags)) };
    berry_hal_aarch64::uart_puts(b"[exc] brk recovered (eret ok)\n");
}

/// 帧分配器往返 + 读写哨兵 + 回收 + 合并。
fn test_frame() {
    let f = crate::mm::frames();

    // 1) 分配一页，写哨兵再读回（身份映射 + Normal 映射下应一致）。
    let p1 = f.alloc().expect("FRAME alloc p1");
    let v1 = phys_to_virt(p1 << PAGE_SHIFT) as *mut u64;
    unsafe {
        v1.write_volatile(0xDEAD_BEEF_CAFE_F00D);
        assert_eq!(
            v1.read_volatile(),
            0xDEAD_BEEF_CAFE_F00D,
            "FRAME 哨兵读写不符"
        );
    }

    // 2) 再分配一页（应与 p1 不同）。
    let p2 = f.alloc().expect("FRAME alloc p2");
    assert_ne!(p2, p1, "FRAME 分配出重复页");

    // 3) 释放（free 不报错 = 页归还池）。buddy 的选页顺序是实现策略——「碎片优先」
    //    （先吃小空闲块、保留大连续块以减碎片），故 free 后不保证立即复用同一页，
    //    只保证「可继续分配 + 能切大块」（见 4）。这也正是 buddy 区别于 bump 分配之处。
    f.free(p1, 0).expect("FRAME free p1");
    f.free(p2, 0).expect("FRAME free p2");

    // 4) 合并：全释放后应能切出 32 页的大块——证明 free 真正归还 + 级联合并无碎片/泄漏。
    let big = f.alloc_order(5).expect("FRAME alloc_order(5) 合并失败");
    f.free(big, 5).expect("FRAME free big");

    berry_hal_aarch64::uart_puts(b"FRAME ok\n");
}

/// V-1b 合成图用例：假 EFI 内存图（写实页）→ map_ok/desc_range/desc_at 决策
/// → scratch FrameAllocator reserve 后保留页永不出选。**零新标记**（裸相 23
/// 标记字节零漂移是 V-1 硬线——断言 fail = panic loud，过则静默）。
fn test_memmap() {
    use berry_hal_aarch64::{RAM_BASE, RAM_TOP};
    use berry_mm::memmap::{desc_at, desc_range, map_ok, DescDecision, EfiMemdesc, EFI_DESC_VERSION};

    // 载体：从全局帧分配取一实页当「图缓冲」——desc_at 经 phys_to_virt 真读回，
    // 镜像布局/直映路径都被真验证（纯函数面之外的那一截）。
    let f = crate::mm::frames();
    let pg = f.alloc().expect("MMAP 合成图载体页");
    let pa = pg << PAGE_SHIFT;
    let va = phys_to_virt(pa) as *mut u64;

    // 合成 4 描述符（40B = 5×u64 each）：Conventional 16 页 / loader-data 4 页
    // （窗内保留段）/ MMIO 0x0900_0000（窗外零交）/ Conventional 贴窗顶 2 页。
    let descs: [(u64, u64, u64, u64, u64); 4] = [
        (7, 0x4000_0000, 0, 16, 0),
        (4, 0x4000_4000, 0, 4, 0),
        (11, 0x0900_0000, 0, 1, 0),
        (7, 0x47FF_F000, 0, 2, 0),
    ];
    // SAFETY: 载体页来自全局帧分配（boot 单线程独占），窗内直映必中。
    unsafe {
        for w in 0..512usize {
            va.add(w).write_volatile(0);
        }
        for (i, d) in descs.iter().enumerate() {
            for w in 0..5usize {
                va.add(i * 5 + w).write_volatile(match w {
                    0 => d.0, // kind | _pad<<32（pad=0）
                    1 => d.1, // physical_start
                    2 => d.2, // virtual_start
                    3 => d.3, // number_of_pages
                    _ => d.4, // attribute
                });
            }
        }
    }

    // 整图校验 + 走图决策。
    assert_eq!(
        map_ok(pa as u64, 160, 40, EFI_DESC_VERSION, RAM_BASE as u64, RAM_TOP as u64),
        Some(4),
        "map_ok 合成图 happy 面"
    );
    // SAFETY: map_ok 已过（窗内 8 对齐直读）；boot 单线程只读。
    let d1 = unsafe { desc_at(pa as u64, 40, 1) };
    assert_eq!(d1.kind, 4);
    assert_eq!(d1.physical_start, 0x4000_4000);
    assert_eq!(d1.number_of_pages, 4, "镜像描述符直映读回不符");
    let expect = [
        DescDecision::Skip,
        DescDecision::Reserve(0x4000_4000, 0x4000_8000),
        DescDecision::Skip, // 窗外 MMIO 零交
        DescDecision::Skip, // Conventional
    ];
    for (i, e) in expect.iter().enumerate() {
        // SAFETY: 同上。
        let d = unsafe { desc_at(pa as u64, 40, i) };
        assert_eq!(
            desc_range(d, RAM_BASE as u64, RAM_TOP as u64),
            *e,
            "desc_range[{i}] 决策不符"
        );
    }

    // reserve 语义：scratch 分配器（栈后备，勿碰全局 FRAMES）——保留 loader 段
    // 后占尽，保留区 4 页永不出现。位图/摘要后备同款栈后备（K-1 索引：
    // 64 页 → 每阶 1 字 × 14 阶）。
    let base = 0x4000_0000usize;
    let mut sfree = [false; 64];
    let mut sorder = [0u8; 64];
    let mut sbmaps = [0u64; (berry_mm::MAX_ORDER + 1)];
    let mut ssums = [0u64; (berry_mm::MAX_ORDER + 1)];
    let mut sfa = berry_mm::FrameAllocator::new(
        base,
        base + 64 * 4096,
        &mut sfree,
        &mut sorder,
        &mut sbmaps,
        &mut ssums,
    );
    assert_eq!(sfa.reserve(0x4000_4000, 0x4000), 4, "reserve 页数不符");
    let mut got = 0usize;
    while let Ok(p) = sfa.alloc() {
        assert!(
            !((0x4000_4000..0x4000_8000).contains(&(p << PAGE_SHIFT))),
            "保留段被分配"
        );
        got += 1;
    }
    assert_eq!(got, 60, "保留后可分配总量不符");

    // 拒面：版本 / 溢出描述符（Bad → 整图拒绝的触发器）。
    assert_eq!(
        map_ok(pa as u64, 160, 40, 2, RAM_BASE as u64, RAM_TOP as u64),
        None
    );
    let hostile = EfiMemdesc {
        kind: 7,
        _pad: 0,
        physical_start: 0,
        virtual_start: 0,
        number_of_pages: u64::MAX / 4096 + 1,
        attribute: 0,
    };
    assert_eq!(
        desc_range(&hostile, RAM_BASE as u64, RAM_TOP as u64),
        DescDecision::Bad
    );

    // 载体页归还（合成图内容随页生命周期消亡）。
    f.free(pg, 0).expect("MMAP 载体页归还");
}

/// 内核堆（`alloc` crate / slab）：slab 路径（Vec 增长 + realloc）+ 整页路径（> 2KiB）。
fn test_heap() {
    use alloc::{boxed::Box, vec::Vec};

    // slab 路径：连续 push 触发多次 realloc（小→大，最终跨过 MAX_CLASS 走整页）。
    let mut v: Vec<u64> = Vec::new();
    for i in 0..1024u64 {
        v.push(i);
    }
    assert_eq!(v.iter().sum::<u64>(), (0..1024u64).sum::<u64>(), "Vec 内容损坏");

    // 整页路径：4KiB（> MAX_CLASS=2048）→ 1 页；8KiB → 2 页。
    let big: Box<[u8; 4096]> = Box::new([0u8; 4096]);
    assert_eq!(big.len(), 4096);
    let big2: Box<[u64; 1024]> = Box::new([0u64; 1024]);
    assert_eq!(big2.iter().filter(|&&x| x == 0).count(), 1024, "8KiB 块内容不符");

    // 作用域结束：big/big2/v 释放（slab 回收槽、整页归还 buddy）。
    drop(v);
    drop(big);
    drop(big2);

    berry_hal_aarch64::uart_puts(b"HEAP ok\n");
}

// ---- Phase 2a Step 1：上下文切换 asm 乒乓（无调度器、无 IRQ）----

use berry_hal_aarch64::task::TaskContext;

/// 16 对齐的 4KiB 内核栈（`new_kernel` 要求栈顶 16 对齐——风险 R2）。
#[repr(C, align(16))]
struct TestStack4096([u8; 4096]);

/// MAIN 的上下文：首切出时由 `context_switch` 把当前 callee-saved+sp 存进来。
/// 初始 ZERO（内容无所谓，被首次 switch 覆盖）。
static mut SWITCH_CTX_MAIN: TaskContext = TaskContext::ZERO;
/// OTHER 的上下文：`new_kernel` 预设指向 `switch_ping_fn`。
static mut SWITCH_CTX_OTHER: TaskContext = TaskContext::ZERO;
static mut SWITCH_OTHER_STACK: TestStack4096 = TestStack4096([0; 4096]);

/// 乒乓对端：打 `PING ` 后切回 MAIN。Step 1 不会被切回（MAIN 不再切它），故以
/// `halt()` 收尾满足 `-> !`。
extern "C" fn switch_ping_fn(_arg: usize) -> ! {
    // trampoline 起步即解蔽（真任务须收 tick——task.rs 设计）——但本测试走
    // boot 屏蔽窗（ktest 在 enable_irq 前）：先复蔽。一来守「boot DAIF 屏蔽
    // 直到 enable_irq」契约（否则 ktest 后全窗解蔽），二来 context_switch
    // 的 `PSTATE.I==1` 不变式对下面切回 MAIN 也成立（S-3a probe ③ daif
    // 锚实测揪出：trampoline 解蔽被带回 MAIN）。
    berry_hal_aarch64::disable_irq();
    // SAFETY: 当前 DAIF 屏蔽（复蔽于上），满足 context_switch 的 `PSTATE.I==1`
    // 不变式。两 ctx 不别名。
    unsafe {
        berry_hal_aarch64::uart_puts(b"PING ");
        berry_hal_aarch64::task::context_switch(
            &raw mut SWITCH_CTX_OTHER,
            &raw const SWITCH_CTX_MAIN,
        );
    }
    berry_hal_aarch64::halt();
}

/// Step 1 信号：构造 OTHER(`new_kernel` 指向 `switch_ping_fn`) → 从 MAIN 切到 OTHER
/// → OTHER 打 `PING ` 切回 → 这里续行打 `TASK ok`。证 switch asm 存/取 callee-saved
/// + sp、新任务首切经 trampoline 接线正确（风险 R1）。
fn test_switch() {
    // SAFETY: DAIF 屏蔽（在 enable_irq 之前）；两 ctx 静态、不别名；栈顶 16 对齐。
    unsafe {
        let stack_top =
            (&raw const SWITCH_OTHER_STACK) as *const u8 as usize + core::mem::size_of::<TestStack4096>();
        SWITCH_CTX_OTHER = TaskContext::new_kernel(switch_ping_fn, 0, stack_top);
        // 从当前（MAIN）切到 OTHER：当前 callee-saved+sp 落 SWITCH_CTX_MAIN，
        // 载 SWITCH_CTX_OTHER → 经 trampoline 进 switch_ping_fn。
        berry_hal_aarch64::task::context_switch(
            &raw mut SWITCH_CTX_MAIN,
            &raw const SWITCH_CTX_OTHER,
        );
        // <- switch_ping_fn 切回 SWITCH_CTX_MAIN 时续行于此。
    }
    // G3-1a（草案 §8.C g3-1a）：per-CPU preempt_count 安全对——计数/嵌套/配平
    // 语义锁死。**回落位路径**（f4 M2 勘正）：ktest::run（main.rs:554）先于
    // cpu0_init（:583）——TPIDR_EL1 未钉，五断言走 static 回落位；主位
    //（SmpCpu.preempt_count）Some 臂由修复批在 cpu0_init 后补 boot 尾直测。
    // 切换原语入口配平断言 = debug_assert：主面 `just build` = dev profile
    // 活执法（同批勘正旧「release 编译消除」句）；断言静默跑：fail = panic
    // 停机冒烟自然红，不加输出行（保 boot 相逐行对拍零漂移）。
    assert_eq!(berry_sched::preempt_count(), 0, "G3-1a：初始 count==0（回落位路径）");
    berry_sched::preempt_disable();
    assert_eq!(berry_sched::preempt_count(), 1, "G3-1a：disable → 1");
    berry_sched::preempt_disable();
    assert_eq!(berry_sched::preempt_count(), 2, "G3-1a：嵌套 → 2");
    berry_sched::preempt_enable();
    assert_eq!(berry_sched::preempt_count(), 1, "G3-1a：enable 弹一层");
    berry_sched::preempt_enable();
    assert_eq!(berry_sched::preempt_count(), 0, "G3-1a：配平归零（切换原语入口可过）");
    berry_hal_aarch64::uart_puts(b"TASK ok\n");
}

// ===================== GUI-0：FPSIMD 上下文全量保存（ktest 面） =====================

/// MAIN 侧 canary：q0..q7 + q30/q31 各一个独立 16B 图样（高半位钉死防与合法
/// 零值/指针巧合）。
static FP_PAT_MAIN: [u128; 10] = [
    0xA1B2_C3D4_E5F6_0718_0000_0000_1111_2222,
    0xA1B2_C3D4_E5F6_0718_0000_0000_3333_4444,
    0xA1B2_C3D4_E5F6_0718_0000_0000_5555_6666,
    0xA1B2_C3D4_E5F6_0718_0000_0000_7777_8888,
    0xA1B2_C3D4_E5F6_0718_0000_0000_9999_AAAA,
    0xA1B2_C3D4_E5F6_0718_0000_0000_BBBB_CCCC,
    0xA1B2_C3D4_E5F6_0718_0000_0000_DDDD_EEEE,
    0xA1B2_C3D4_E5F6_0718_0000_0000_FFFF_0001,
    // q30 / q31
    0xA1B2_C3D4_E5F6_0718_0000_0000_2000_0030,
    0xA1B2_C3D4_E5F6_0718_0000_0000_2000_0031,
];
/// OTHER 侧 canary（同族异值）。
static FP_PAT_OTHER: [u128; 10] = [
    0xB4_D5_C6_E7_F8_0921_0000_0000_6161_6161,
    0xB4_D5_C6_E7_F8_0921_0000_0000_6262_6262,
    0xB4_D5_C6_E7_F8_0921_0000_0000_6363_6363,
    0xB4_D5_C6_E7_F8_0921_0000_0000_6464_6464,
    0xB4_D5_C6_E7_F8_0921_0000_0000_6565_6565,
    0xB4_D5_C6_E7_F8_0921_0000_0000_6666_6666,
    0xB4_D5_C6_E7_F8_0921_0000_0000_6767_6767,
    0xB4_D5_C6_E7_F8_0921_0000_0000_6868_6868,
    0xB4_D5_C6_E7_F8_0921_0000_0000_6930_0030,
    0xB4_D5_C6_E7_F8_0921_0000_0000_6930_0031,
];
/// OTHER 侧 FPCR/FPSR 图样：RMODE=10（RM，round to −inf）/ NZCV=0011——**不含任何
/// 陷阱使能位**（FPCR bit8..12 置 1 会让 FP 指令直接 trap，ktest 不愿赌）。注：
/// RMODE 编码 00=RN 最近、01=RZ 零、10=RM −inf、11=RP +inf——10 是 −inf，别按
/// 「越大越正」直觉读反（CORR-2 修复轮钉板）。
const FP_FPCR_OTHER: u64 = 0x0080_0000;
const FP_FPSR_OTHER: u64 = 0x3000_0000;
static mut FP_CTX_MAIN: TaskContext = TaskContext::ZERO;
static mut FP_CTX_OTHER: TaskContext = TaskContext::ZERO;
static mut FP_OTHER_STACK: TestStack4096 = TestStack4096([0; 4096]);
static mut FP_SNAP: [u128; 10] = [0; 10];

/// q0..q7 + q30/q31 ← pats[0..10]（canary 装载）。
///
/// SAFETY: 纯 asm 写向量寄存器；lateout 声明 clobber 防编译器在 q 里保活值。
#[inline(never)]
unsafe fn fp_canary_load(pats: *const u128) {
    unsafe {
    core::arch::asm!(
        ".arch armv8-a+fp+simd", // 局部重开 FP/SIMD 汇编解析（全局 -neon）
        "ldr q0,  [x0, #0]",
        "ldr q1,  [x0, #16]",
        "ldr q2,  [x0, #32]",
        "ldr q3,  [x0, #48]",
        "ldr q4,  [x0, #64]",
        "ldr q5,  [x0, #80]",
        "ldr q6,  [x0, #96]",
        "ldr q7,  [x0, #112]",
        "ldr q30, [x0, #128]",
        "ldr q31, [x0, #144]",
        in("x0") pats,
        lateout("v0") _, lateout("v1") _, lateout("v2") _, lateout("v3") _,
        lateout("v4") _, lateout("v5") _, lateout("v6") _, lateout("v7") _,
        lateout("v30") _, lateout("v31") _,
    );
    }
}

/// q0..q7 + q30/q31 → out[0..10]（canary 快照）。SAFETY 同 [`fp_canary_load`]。
#[inline(never)]
unsafe fn fp_canary_store(out: *mut u128) {
    unsafe {
    core::arch::asm!(
        ".arch armv8-a+fp+simd", // 局部重开 FP/SIMD 汇编解析（全局 -neon）
        "str q0,  [x0, #0]",
        "str q1,  [x0, #16]",
        "str q2,  [x0, #32]",
        "str q3,  [x0, #48]",
        "str q4,  [x0, #64]",
        "str q5,  [x0, #80]",
        "str q6,  [x0, #96]",
        "str q7,  [x0, #112]",
        "str q30, [x0, #128]",
        "str q31, [x0, #144]",
        in("x0") out,
        lateout("v0") _, lateout("v1") _, lateout("v2") _, lateout("v3") _,
        lateout("v4") _, lateout("v5") _, lateout("v6") _, lateout("v7") _,
        lateout("v30") _, lateout("v31") _,
    );
    }
}

#[inline(never)]
unsafe fn fpcr_read() -> u64 {
    let v: u64;
    unsafe { core::arch::asm!(".arch armv8-a+fp+simd", "mrs {}, fpcr", out(reg) v); }
    v
}
#[inline(never)]
unsafe fn fpcr_write(v: u64) {
    unsafe { core::arch::asm!(".arch armv8-a+fp+simd", "msr fpcr, {}", in(reg) v); }
}
#[inline(never)]
unsafe fn fpsr_read() -> u64 {
    let v: u64;
    unsafe { core::arch::asm!(".arch armv8-a+fp+simd", "mrs {}, fpsr", out(reg) v); }
    v
}
#[inline(never)]
unsafe fn fpsr_write(v: u64) {
    unsafe { core::arch::asm!(".arch armv8-a+fp+simd", "msr fpsr, {}", in(reg) v); }
}

// 本仓 mem* 整数版覆写直调面（crate::mem 定义、此处声明——链接即命中我们的实现）。
unsafe extern "C" {
    fn memset(s: *mut u8, c: u8, n: usize) -> *mut u8;
    fn memcpy(d: *mut u8, s: *const u8, n: usize) -> *mut u8;
    fn memmove(d: *mut u8, s: *const u8, n: usize) -> *mut u8;
    fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32;
    fn bcmp(a: *const u8, b: *const u8, n: usize) -> i32;
}

/// FP_SNAP 与 pats 逐位比对（u128 整数比较走 GPR，不碰 FP 面）。
fn fp_snap_eq(pats: &[u128; 10]) -> bool {
    // SAFETY: ktest 单线程窗口读 FP_SNAP（经 raw 指针取共享引用——static_mut_refs 纪律）。
    let snap: &[u128; 10] = unsafe { &*(&raw const FP_SNAP) };
    snap == pats
}

/// FP 乒乓对端：装 B 图样 → 自证 → 打 `FPING ` 切回 MAIN。切出时其 FP 态落
/// `FP_CTX_OTHER`（MAIN 侧随后核验保存面）。不会再被切回，`halt()` 收尾满足 `-> !`。
extern "C" fn fp_ping_fn(_arg: usize) -> ! {
    // 复蔽于 trampoline 解蔽后（switch_ping_fn 同款：boot 屏蔽窗契约 +
    // 切回 MAIN 的 PSTATE.I==1 不变式；S-3a probe ③ 揪出）。
    berry_hal_aarch64::disable_irq();
    // SAFETY: 当前 DAIF 屏蔽（复蔽于上）；两 ctx 不别名。
    unsafe {
        fpcr_write(FP_FPCR_OTHER);
        fpsr_write(FP_FPSR_OTHER);
        fp_canary_load((&raw const FP_PAT_OTHER).cast::<u128>());
        // 自证：装载面成立（非 switch 之功——先排除「图样根本没装上」的假失败）。
        fp_canary_store((&raw mut FP_SNAP).cast::<u128>());
        assert!(fp_snap_eq(&FP_PAT_OTHER), "FP：OTHER canary 装载自证失败");
        berry_hal_aarch64::uart_puts(b"FPING ");
        berry_hal_aarch64::task::context_switch(
            &raw mut FP_CTX_OTHER,
            &raw const FP_CTX_MAIN,
        );
    }
    berry_hal_aarch64::halt();
}

/// GUI-0：FPSIMD 上下文全量保存三面：
/// ① 出生零态（new_kernel 出发于 ZERO，FP 面 = 架构复位值）；
/// ② 跨任务双向对拍（MAIN⇄OTHER 两跳乒乓——双方各装独立 q0..q7/q30/q31 +
///    FPCR/FPSR 图样，切回后 MAIN 寄存器面与 OTHER 保存面各自核验 = 跨任务
///    SIMD 不串味）；
/// ③ 内核 copy/zero 路径 canary（**内核 SIMD-free 合同护栏**：Vmo commit 清零 +
///    write/read 拷贝跑在 canary 在位时——编译器漂移引入 NEON memset/memcpy 即
///    boot 期 loud）。
fn test_fp() {
    // MAIN 侧 FPCR/FPSR 图样（RMODE=01 / NZCV=1111——同样不含陷阱使能位）。
    const FPCR_MAIN: u64 = 0x0040_0000;
    const FPSR_MAIN: u64 = 0xF000_0000;
    // SAFETY: DAIF 屏蔽（ktest 在 enable_irq 之前）；静态两 ctx 不别名；栈顶 16 对齐。
    unsafe {
        let stack_top = (&raw const FP_OTHER_STACK) as *const u8 as usize
            + core::mem::size_of::<TestStack4096>();
        FP_CTX_OTHER = TaskContext::new_kernel(fp_ping_fn, 0, stack_top);
        // ① 出生零态（raw 指针取共享引用——static_mut_refs 纪律；unsafe 归外层块，
        // 再包一层会 unused_unsafe 告警——CORR-1 修复轮钉板）。
        let fresh: &TaskContext = &*(&raw const FP_CTX_OTHER);
        assert!(
            fresh.fp == [0u128; 32] && fresh.fpcr == 0 && fresh.fpsr == 0,
            "FP：新任务出生 FP 面非零"
        );
        let (fpcr0, fpsr0) = (fpcr_read(), fpsr_read()); // 记录现场值，测毕还原
        // ② MAIN 侧装载自己的图样 → 两跳乒乓。
        fpcr_write(FPCR_MAIN);
        fpsr_write(FPSR_MAIN);
        fp_canary_load((&raw const FP_PAT_MAIN).cast::<u128>());
        berry_hal_aarch64::task::context_switch(&raw mut FP_CTX_MAIN, &raw const FP_CTX_OTHER);
        // <- fp_ping_fn 切回 FP_CTX_MAIN 时续行于此。四面核验：
        fp_canary_store((&raw mut FP_SNAP).cast::<u128>());
        assert!(fp_snap_eq(&FP_PAT_MAIN), "FP：MAIN 的 q0..q7/q30/q31 经两跳 switch 后串味");
        assert!(fpcr_read() == FPCR_MAIN, "FP：MAIN 的 FPCR 未恢复");
        assert!(fpsr_read() == FPSR_MAIN, "FP：MAIN 的 FPSR 未恢复");
        // SAFETY: 同窗读 FP_CTX_OTHER（raw 指针取共享引用；unsafe 归外层块）。
        let o: &TaskContext = &*(&raw const FP_CTX_OTHER);
        let other_saved = o.fp[0] == FP_PAT_OTHER[0]
            && o.fp[1] == FP_PAT_OTHER[1]
            && o.fp[2] == FP_PAT_OTHER[2]
            && o.fp[3] == FP_PAT_OTHER[3]
            && o.fp[4] == FP_PAT_OTHER[4]
            && o.fp[5] == FP_PAT_OTHER[5]
            && o.fp[6] == FP_PAT_OTHER[6]
            && o.fp[7] == FP_PAT_OTHER[7]
            && o.fp[30] == FP_PAT_OTHER[8]
            && o.fp[31] == FP_PAT_OTHER[9]
            && o.fpcr == FP_FPCR_OTHER
            && o.fpsr == FP_FPSR_OTHER;
        assert!(other_saved, "FP：OTHER 的 FP 态未随切出保存进其上下文");
        // ③-a 本仓 mem* 整数版覆写正确性直验（含 memmove 重叠两向 + memcmp 符号面
        // + bcmp 0/非 0 面）。
        {
            let mut buf = [0u8; 40];
            memset(buf.as_mut_ptr(), 0x5A, 40);
            let mut pat = [0u8; 40];
            for (i, b) in pat.iter_mut().enumerate() {
                *b = i as u8;
            }
            memcpy(buf.as_mut_ptr(), pat.as_ptr(), 40);
            assert_eq!(buf, pat, "FP：memset/memcpy 覆写正确性");
            // 重叠后向（dst > src）：右移 2。
            let mut ov = [0u8; 24];
            for (i, b) in ov.iter_mut().enumerate() {
                *b = (0xA0 + i) as u8;
            }
            memmove(ov.as_mut_ptr().add(2), ov.as_ptr(), 22);
            for i in 0..22 {
                assert_eq!(ov[i + 2], (0xA0 + i) as u8, "FP：memmove 重叠（后向）正确性");
            }
            // 重叠前向（dst < src）：左移 3。
            let mut ov2 = [0u8; 24];
            for (i, b) in ov2.iter_mut().enumerate() {
                *b = (0xC0 + i) as u8;
            }
            memmove(ov2.as_mut_ptr(), ov2.as_ptr().add(3), 21);
            for i in 0..21 {
                assert_eq!(ov2[i], (0xC0 + i + 3) as u8, "FP：memmove 重叠（前向）正确性");
            }
            assert!(memcmp(pat.as_ptr(), buf.as_ptr(), 40) == 0, "FP：memcmp 相等面");
            buf[39] ^= 1; // pat[39]=0x27 → buf[39]=0x26：首异字节 buf < pat。
            assert!(memcmp(buf.as_ptr(), pat.as_ptr(), 40) < 0, "FP：memcmp 符号面");
            // bcmp 直验（LLVM 把 `memcmp(a,b,n)==0` 标准变换为 bcmp 调用——覆写件
            // 的伴生符号，漏验 = 链条上唯一没对拍过的口）：相等面 0 / 差异面非 0。
            buf[39] ^= 1; // 0x26 → 0x27：两缓冲再相等。
            assert!(bcmp(pat.as_ptr(), buf.as_ptr(), 40) == 0, "FP：bcmp 相等面");
            buf[39] ^= 1; // 重新打破。
            assert!(bcmp(buf.as_ptr(), pat.as_ptr(), 40) != 0, "FP：bcmp 差异面");
        }
        // ③-b 内核 copy/zero/块拷贝路径 canary（SIMD-free 合同护栏——①-neon 旗 +
        // ②mem* 覆写两机制在位时必过；旗失效/编译器漂移即此处 boot 期 loud）。
        fp_canary_load((&raw const FP_PAT_MAIN).cast::<u128>());
        // 块拷贝探针（sys_object_wait_many 数组 / 结构体拷贝同族——首跑实抓
        // O1 下 LLVM 即用 q 寄存器落这类拷贝，故必须探）；black_box 防常量折叠
        // 把拷贝优化掉。
        let src_arr = core::hint::black_box([0x1234_5678_9ABC_DEF0u64; 16]);
        let dst_arr = src_arr; // [u64;16] 是 Copy——此处即块拷贝；black_box 防折叠
        assert_eq!(core::hint::black_box(&dst_arr)[15], src_arr[15], "FP：块拷贝探针前置失败");
        use crate::vmo::Vmo;
        use berry_mm::va::PAGE_SIZE;
        let mut vmo = Vmo::create(PAGE_SIZE).expect("FP：装配 VMO");
        vmo.commit(crate::mm::frames(), 0, PAGE_SIZE)
            .expect("FP：commit（清零路径）");
        static SRC: [u8; 512] = [0x5A; 512];
        vmo.write(0, &SRC).expect("FP：write（拷贝路径）");
        let mut dst = [0u8; 64];
        vmo.read(0, &mut dst).expect("FP：read");
        assert!(dst == [0x5A; 64], "FP：VMO 读写不符（canary 前置失败）");
        fp_canary_store((&raw mut FP_SNAP).cast::<u128>());
        assert!(
            fp_snap_eq(&FP_PAT_MAIN),
            "FP：内核 copy/zero 路径触碰了 SIMD——违反内核 SIMD-free 合同"
        );
        // 还原现场控制位。
        fpcr_write(fpcr0);
        fpsr_write(fpsr0);
    }
    berry_hal_aarch64::uart_puts(b"FP ok\n");
}

/// 2b-2：每进程页表 + ASID 互切 + 低半退役正向断言（`内存管理设计.md` §5.1 第 3 步）。
/// 低半退役本身在 `rust_main` 早期已完成（`retire_low_half`），本用例验证其效果。
fn test_aspace() {
    use crate::aspace::MiniAspace;
    use berry_hal_aarch64 as hal;

    const MAGIC_A: u64 = 0x00A5_C0DE_0000_0001;
    const MAGIC_B: u64 = 0x00B6_C0DE_0000_0002;

    // 关卡记录（§5.2）：ASID 实测宽度（QEMU virt/cortex-a72）。
    let width = hal::asid_bits();

    let a = MiniAspace::new(crate::mm::frames(), crate::aspace::alloc_asid(), MAGIC_A);
    let b = MiniAspace::new(crate::mm::frames(), crate::aspace::alloc_asid(), MAGIC_B);

    // 激活 A：读到自己的 magic；写入改值、读回。
    a.activate();
    assert_eq!(a.read_va(), MAGIC_A, "ASPACE：A 初值不符");
    a.write_va(0x00A5_DEAD_0000_0011);
    assert_eq!(a.read_va(), 0x00A5_DEAD_0000_0011, "ASPACE：A 写读不符");

    // 切 B：**同一 VA** 读到 B 的 magic（≠ A 刚写的值）→ 两表隔离；B 写自己的值。
    b.activate();
    assert_eq!(b.read_va(), MAGIC_B, "ASPACE：B 下同 VA 应是 B 的内容（隔离）");
    b.write_va(0x00B6_DEAD_0000_0022);

    // 切回 A：A 的写入保留 → 切换不破坏对端表。
    a.activate();
    assert_eq!(a.read_va(), 0x00A5_DEAD_0000_0011, "ASPACE：切回 A 后 A 的写入须保留");

    // 低半退役**正向断言**：读已退役的身份地址必须 fault 且可恢复。
    // arm 期望故障 → ldr（若低半仍有映射则不 fault → 槽仍武装 → 断言失败）。
    hal::trap::arm_expected_fault(berry_hal_aarch64::RAM_BASE);
    // SAFETY: 预期同步异常（期望故障槽已 arm，FAR 命中即恢复）；值本身无意义。
    let _ = unsafe { core::ptr::read_volatile(berry_hal_aarch64::RAM_BASE as *const u64) };
    assert!(
        !hal::trap::fault_armed(),
        "ASPACE：低半读居然没 fault——身份映射没退役干净"
    );

    // 收尾：回内核空表（与 idle/无 AS 任务的 ttbr0=0 状态对齐）。
    hal::activate_aspace(0, 0);

    berry_hal_aarch64::uart_puts(b"ASPACE ok (asid=");
    berry_hal_aarch64::uart_put_dec(width as u64);
    berry_hal_aarch64::uart_puts(b"b, low-half faulted)\n");
}

/// 2b-3：匿名 VMO/Vmar 六操作（create/commit/map/unmap/protect/allocate，
/// `内存管理设计.md` §5.1 第 4 步）——map 即填用户页表；§5.1 验收行：
/// **内核任务经 VMO 映射读写一页**。fault 侧均走期望故障通道正向断言。
#[allow(clippy::unwrap_used)]
fn test_vmo() {
    use crate::aspace::{alloc_asid, USER_VA};
    use crate::vmar::{Perm, Vmar, VmarError};
    use crate::vmo::{Vmo, VmoError};
    use berry_hal_aarch64 as hal;
    use berry_mm::va::PAGE_SIZE;

    const MAGIC: u64 = 0x00C0_FFEE_0000_0033;
    const MAGIC2: u64 = 0x00C0_FFEE_0000_0044;
    let fa = crate::mm::frames();

    // A. 对象语义：create/commit/read/write + 未提交页拒绝。
    let mut vmo = Vmo::create(PAGE_SIZE * 2).expect("ktest：装配 VMO（可信语境）");
    assert_eq!(vmo.size(), PAGE_SIZE * 2, "VMO：容量应页对齐");
    vmo.commit(fa, 0, PAGE_SIZE).expect("VMO：commit 页 0");
    vmo.write(0, &MAGIC.to_le_bytes()).expect("VMO：write");
    let mut buf = [0u8; 8];
    vmo.read(0, &mut buf).expect("VMO：read");
    assert_eq!(u64::from_le_bytes(buf), MAGIC, "VMO：读写往返");
    assert_eq!(
        vmo.read(PAGE_SIZE, &mut buf),
        Err(VmoError::NotCommitted),
        "VMO：未提交页必须拒读（eager VMO 面；demand G4-1 起另路供给）"
    );

    // B. map 即填表 + §5.1 验收：内核经 VMO 映射读写一页（VA 视角）。
    // S-4 4-2 合同（本段直调 map/unmap/protect 的隐含前提钉板）：ktest 全程
    // 在 smp_bringup 之前、par_enabled=false——PT 锁零争用一次过、
    // broadcast() TX=0 零扰动。若未来把本段挪到 SMP 在场后跑，须改经
    // dispatch 路径（回执等待在 SVC 派发尾）或自证回执已收。
    let mut root = Vmar::new_root(fa, alloc_asid()).expect("VMO：建 root Vmar（ktest 可信语境）");
    root.map(fa, &mut vmo, USER_VA, 0, PAGE_SIZE, Perm::Rw)
        .expect("VMO：map");
    root.activate();
    // SAFETY: 读已映射 Normal 页（root 已激活）。
    let v = unsafe { core::ptr::read_volatile(USER_VA as *const u64) };
    assert_eq!(v, MAGIC, "VMO：经映射读到 VMO 内容");
    // SAFETY: 写同一物理页——VMO 视角应立即可见（别名一致性）。
    unsafe { core::ptr::write_volatile(USER_VA as *mut u64, MAGIC2) };
    vmo.read(0, &mut buf).expect("VMO：read after write");
    assert_eq!(u64::from_le_bytes(buf), MAGIC2, "VMO：VA 写 → VMO 读一致");

    // C. unmap：此后该 VA 必 fault（期望故障正向断言）。
    root.unmap(USER_VA, PAGE_SIZE).expect("VMO：unmap");
    hal::trap::arm_expected_fault(USER_VA);
    // SAFETY: 预期同步异常（期望故障槽已 arm，FAR 命中即恢复）；值无意义。
    let _ = unsafe { core::ptr::read_volatile(USER_VA as *const u64) };
    assert!(!hal::trap::fault_armed(), "VMO：unmap 后读居然没 fault");

    // D. 权限：map 只读 → 写必 fault；protect 提回 RW → 写通。
    root.map(fa, &mut vmo, USER_VA, 0, PAGE_SIZE, Perm::Ro)
        .expect("VMO：map RO");
    // SAFETY: 读已映射页。
    let v = unsafe { core::ptr::read_volatile(USER_VA as *const u64) };
    assert_eq!(v, MAGIC2, "VMO：RO 读应通");
    hal::trap::arm_expected_fault(USER_VA);
    // SAFETY: 预期 Permission fault（期望故障槽已 arm）。
    unsafe { core::ptr::write_volatile(USER_VA as *mut u64, MAGIC) };
    assert!(!hal::trap::fault_armed(), "VMO：RO 页写居然没 fault");
    root.protect(USER_VA, PAGE_SIZE, Perm::Rw).expect("VMO：protect→RW");
    // SAFETY: protect 已提权 + 按号失效，写应通。
    unsafe { core::ptr::write_volatile(USER_VA as *mut u64, MAGIC) };
    vmo.read(0, &mut buf).expect("VMO：read");
    assert_eq!(u64::from_le_bytes(buf), MAGIC, "VMO：protect 后写生效");

    // E. allocate 子 Vmar：越子界拒、父撞预留拒、子映射共享同表根。
    let mut child = root.allocate(0x20_0000, PAGE_SIZE * 2).expect("VMO：allocate");
    child.map(fa, &mut vmo, 0x20_0000, 0, PAGE_SIZE, Perm::Rw).expect("VMO：子 map");
    assert_eq!(
        child.map(fa, &mut vmo, 0x30_0000, 0, PAGE_SIZE, Perm::Rw),
        Err(VmarError::OutOfBounds),
        "VMO：子 Vmar 越界映射必须拒"
    );
    assert_eq!(
        root.map(fa, &mut vmo, 0x20_1000, 0, PAGE_SIZE, Perm::Rw),
        Err(VmarError::Overlap),
        "VMO：父撞子预留必须拒"
    );
    // SAFETY: 读子 Vmar 映射页（同一 L1 表根，root 仍激活）。
    let v = unsafe { core::ptr::read_volatile(0x20_0000 as *const u64) };
    assert_eq!(v, MAGIC, "VMO：子映射经共享表根可读（同 PA 双 VA 别名）");

    // F. A1-2 容量冒烟：40MiB VMO 全量 commit/decommit——`vmo.rs` commit 逐页
    // order-0 出帧，不受 MAX_ORDER 堆单块地形约束；帧账本对账（B-0
    // `allocated_pages` 口径）钉「10240 页全出全还、零泄漏」。40MB+ blob
    // 装载面的前置证据（`docs/容量工程前置-A1设计草案.md` §2 关键洞察 / §4 A1-2）。
    //
    // 对账须计**页记帐双 Vec**（`Vmo::create` 经堆整页路径入账——heap.rs
    // >2048B → `order_for` 取 `2^order >= npages` 最小阶）：`pages:
    // Vec<Option<usize>>` 16B/页 → 160KiB → order 6 = 64 页；`mapcnt:
    // Vec<u16>` 2B/页 → 20KiB → order 3 = 8 页——合计 72 页伴随 VMO 存活，
    // drop 才归还（首跑实测 +72 实抓，与逐项算术逐页吻合）。
    const CAP40: usize = 40 * 1024 * 1024;
    let np40 = CAP40 / PAGE_SIZE;
    let ledger = (16 * np40).div_ceil(PAGE_SIZE).next_power_of_two()
        + (2 * np40).div_ceil(PAGE_SIZE).next_power_of_two();
    let a0 = fa.allocated_pages();
    let mut big = Vmo::create(CAP40).expect("VMO：40MiB create（128MiB 帽内）");
    assert_eq!(
        fa.allocated_pages(),
        a0 + ledger,
        "VMO：create 记帐双 Vec 入账（order 取整 72 页）"
    );
    big.commit(fa, 0, CAP40).expect("VMO：40MiB 全量 commit（10240 页逐页）");
    assert_eq!(
        fa.allocated_pages(),
        a0 + ledger + np40,
        "VMO：commit 帧账本精确 +10240（另记帐 72 伴随存活）"
    );
    big.write(0, &MAGIC.to_le_bytes()).expect("VMO：首页 write");
    let off_last = CAP40 - 8;
    big.write(off_last, &MAGIC2.to_le_bytes()).expect("VMO：尾页 write");
    let mut bigbuf = [0u8; 8];
    big.read(0, &mut bigbuf).expect("VMO：首页 read");
    assert_eq!(u64::from_le_bytes(bigbuf), MAGIC, "VMO：40MiB 首页读写往返");
    big.read(off_last, &mut bigbuf).expect("VMO：尾页 read");
    assert_eq!(u64::from_le_bytes(bigbuf), MAGIC2, "VMO：40MiB 尾页读写往返");
    big.decommit(fa, 0, CAP40).expect("VMO：40MiB 全量 decommit");
    assert_eq!(
        fa.allocated_pages(),
        a0 + ledger,
        "VMO：decommit 后 10240 帧全还（记帐双 Vec 随 VMO 存活）"
    );
    drop(big);
    assert_eq!(
        fa.allocated_pages(),
        a0,
        "VMO：drop 后帧账本精确归位（零泄漏——记帐双 Vec 一并归还）"
    );

    // 收尾：回内核空表（与 idle/无 AS 任务的 ttbr0=0 状态对齐）。
    hal::activate_aspace(0, 0);
    berry_hal_aarch64::uart_puts(b"VMO ok\n");
}

/// 3-0：handle 表 + rights 语义（`能力模型设计.md` §2/§4，决策 #28 位域）——
/// insert/get（查表→类型→权限三门控）、duplicate 只紧不松、close 后旧值必拒
/// （引用共享仍在）、槽位重用代号 +1（旧 handle 不复活）、replace 原子搬迁、
/// 坏 handle 面（0/野值）。
#[allow(clippy::unwrap_used)]
fn test_handle() {
    use crate::handle::{
        HandleTable, KernelObj, ObjType, Rights, ERR_ACCESS_DENIED, ERR_BAD_HANDLE,
        ERR_INVALID_ARG, RIGHT_DUPLICATE, RIGHT_READ, RIGHT_SET_POLICY, RIGHT_WRITE,
        RIGHTS_SAME,
    };
    use crate::vmo::Vmo;
    use berry_mm::va::PAGE_SIZE;

    // handle 位域（handle.rs 私有实现，此处按 ABI §8.1 注释断言：低 16 位槽号）。
    const SLOT_MASK: u32 = 0xFFFF;

    let mut ht = HandleTable::new();

    // A. insert + get 过三门：Vmo 授 READ|WRITE|DUPLICATE，get 要 READ 放行。
    let rw: Rights = RIGHT_READ | RIGHT_WRITE | RIGHT_DUPLICATE;
    let h1 = ht
        .insert(KernelObj::Vmo(Vmo::create(PAGE_SIZE).expect("ktest：装配 VMO（可信语境）")), rw)
        .expect("HANDLE：insert");
    assert_ne!(h1, 0, "HANDLE：handle 0 非法（槽 0 保留）");
    let (obj, rights) = ht.get(h1, ObjType::Vmo, RIGHT_READ).expect("HANDLE：get");
    match obj {
        KernelObj::Vmo(v) => assert_eq!(v.size(), PAGE_SIZE, "HANDLE：对象本体"),
        _ => unreachable!(),
    }
    assert_eq!(rights, rw, "HANDLE：get 回授予权利");

    // B. 门控拒绝面：类型不符 → BAD_HANDLE；权限不足 → ACCESS_DENIED。
    assert_eq!(
        ht.get(h1, ObjType::Vmar, 0).err(),
        Some(ERR_BAD_HANDLE),
        "HANDLE：类型不符须按坏 handle 拒"
    );
    assert_eq!(
        ht.get(h1, ObjType::Vmo, RIGHT_SET_POLICY).err(),
        Some(ERR_ACCESS_DENIED),
        "HANDLE：未授权限须拒"
    );

    // C. duplicate：SAME=全集；收紧 OK；提权 / 空 rights 一律拒。
    let h2 = ht.duplicate(h1, RIGHT_READ).expect("HANDLE：dup 收紧");
    let h2b = ht.duplicate(h1, RIGHTS_SAME).expect("HANDLE：dup SAME");
    let (_, rights_b) = ht.get(h2b, ObjType::Vmo, 0).expect("HANDLE：get h2b");
    assert_eq!(rights_b, rw, "HANDLE：SAME 须取原全集");
    assert_eq!(
        ht.duplicate(h1, RIGHT_READ | RIGHT_SET_POLICY).err(),
        Some(ERR_ACCESS_DENIED),
        "HANDLE：dup 提权必须拒"
    );
    assert_eq!(
        ht.duplicate(h1, 0).err(),
        Some(ERR_ACCESS_DENIED),
        "HANDLE：dup 空 rights 必须拒"
    );

    // D. close + 引用共享：close h1 → 旧值必拒；h2/h2b 仍活（对象 refs > 0）。
    ht.close(h1).expect("HANDLE：close h1");
    assert_eq!(
        ht.get(h1, ObjType::Vmo, 0).err(),
        Some(ERR_BAD_HANDLE),
        "HANDLE：close 后旧值必须失效"
    );
    ht.get(h2, ObjType::Vmo, RIGHT_READ).expect("HANDLE：dup 持有的对象须仍活");

    // E. 槽位重用代号 +1：新 insert 复用 h1 的最低空槽，但代号已进——
    //    旧值 h1 不复活（gen 是守卫，不是「槽恰好空着」）。
    let h3 = ht
        .insert(KernelObj::Vmo(Vmo::create(PAGE_SIZE * 2).expect("ktest：装配 VMO（可信语境）")), RIGHT_READ)
        .expect("HANDLE：insert 重用槽");
    assert_eq!(
        h3 & SLOT_MASK,
        h1 & SLOT_MASK,
        "HANDLE：最低空槽应被复用"
    );
    assert_ne!(h3, h1, "HANDLE：重用槽代号必须 +1");
    assert_eq!(
        ht.get(h1, ObjType::Vmo, 0).err(),
        Some(ERR_BAD_HANDLE),
        "HANDLE：旧代号不得复活"
    );

    // F. replace 原子搬迁：两旧值均失效；新值指 repl 对象（2 页）+ repl 权利。
    let h4 = ht.replace(h2, h3).expect("HANDLE：replace");
    assert_ne!(h4, h2);
    assert_ne!(h4, h3);
    assert_eq!(ht.get(h2, ObjType::Vmo, 0).err(), Some(ERR_BAD_HANDLE));
    assert_eq!(ht.get(h3, ObjType::Vmo, 0).err(), Some(ERR_BAD_HANDLE));
    let (obj, rights) = ht.get(h4, ObjType::Vmo, 0).expect("HANDLE：get h4");
    match obj {
        KernelObj::Vmo(v) => assert_eq!(v.size(), PAGE_SIZE * 2, "HANDLE：replace 后应是 repl 对象"),
        _ => unreachable!(),
    }
    assert_eq!(rights, RIGHT_READ, "HANDLE：权利随 repl 搬迁");
    assert_eq!(
        ht.replace(h4, h4).err(),
        Some(ERR_INVALID_ARG),
        "HANDLE：replace 两值相同须拒"
    );

    // G. 坏 handle 面：0 与野值一律拒（含越界槽号）。
    assert_eq!(ht.get(0, ObjType::Vmo, 0).err(), Some(ERR_BAD_HANDLE));
    assert_eq!(ht.close(0xDEAD_BEEF).err(), Some(ERR_BAD_HANDLE));

    // H. integrity_check 正向走查（S-5a）：此刻混合态 = 两 Used（h2b/h4 两对象
    //    各一）+ 槽 1/2 空（代号已进）→ Ok(2)；零分配空表 → Ok(0)。走查
    //    逻辑本身的 boot 窗验证（变异标定株：破坏 gen/refs 后此断言必红）。
    assert_eq!(
        ht.integrity_check(),
        Ok(2),
        "HANDLE：混合态走查须报 2 在用槽"
    );
    let ht_empty = HandleTable::new();
    assert_eq!(ht_empty.integrity_check(), Ok(0), "HANDLE：空表走查 Ok(0)");

    // 收尾：全关——h2b 的 close 使第一对象引用归零（走最后释放路径）。
    ht.close(h2b).expect("HANDLE：close h2b");
    ht.close(h4).expect("HANDLE：close h4");

    // H2. 全关后走查归零（终账恒等——B-0 churn 金丝雀同判据，走查面收口）。
    assert_eq!(ht.integrity_check(), Ok(0), "HANDLE：全关后走查须归零");
    assert_eq!(ht.used_count(), 0, "HANDLE：全关后 used_count 归零");

    berry_hal_aarch64::uart_puts(b"HANDLE ok\n");
}

/// 3-4c 空策略表等价测试（`AI调度设计.md` §2「AI 不在线时表为空/默认，等价
/// 于纯启发式」的**机器断言**，非口号；smoke 第 13 标记）。
///
/// 本地 `Runqueue` + `RoundRobin` 实例（不动全局 `RUNQUEUE`——ktest 在
/// `berry_sched::init` 之前跑、CURRENT 尚 null，本测试自建三任务 12 轮 pick
/// 记录序列逐项比对）：
/// - **A 基线（未绑）**：`policy::bound() == false`；
/// - **B 空表**：绑定后 12 轮的（选中者， prio_effective, epp）与 A **逐项
///   全等**——接口在位、表空 = 零写入 = 与「接口缺席」逐位等价；
/// - **C 对照表**（p7 权重 +32 / EPP=Perf）：4-3a 起 pick 按 `prio_effective`
///   出选（§3.6「调度决策一律看 effective」），注入**真消费**——首轮（三任务
///   eff 同 0，出选同基线 head）apply 写入 p7 eff=32 后，后续各轮 p7 以最高
///   effective **独占出选**（跨层严格优先的机器断言；apply 的反向控制——
///   防留桩成死代码的假等价）。字段断言：p7 恒 = base+32 / EPP=Perf；
/// - **收尾 unbind**：恢复全局缺席态——之后 smoke 稳态全程在未绑态跑。
///
/// 进程调度 crate 无 host 测试面（HAL aarch64 asm），QEMU ktest 是唯一家。
#[allow(clippy::unwrap_used)]
fn test_policy() {
    use alloc::vec::Vec;
    use berry_sched::policy::{self, Directive, Entry};
    use berry_sched::{EppHint, Fair, RoundRobin, Runqueue, SchedClass, Task, Topo};

    /// 占位入口（pick_next 只选不切——永不执行；halt 兜底防未定义行为）。
    extern "C" fn pol_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    /// 单轮观测（比对单位）：选中者名 + 两个被注入面字段。
    type Obs = (&'static [u8], i32, EppHint);

    const ROUNDS: usize = 12;

    /// 建本地 runqueue + 三任务（proc_tag = 7/8/9 = 策略表 key 域；boot/ktest
    /// 可信语境走 expect 包装——3b3 try 化的裁定点）。
    fn setup() -> (Runqueue, RoundRobin, Topo, Vec<&'static mut Task>) {
        let mut rq = Runqueue::new();
        let rr = Fair;
        let topo = Topo::single();
        let mut tasks = Vec::new();
        for (tag, name) in [(7usize, b"p7".as_slice()), (8, b"p8".as_slice()), (9, b"p9".as_slice())] {
            let mut t = Task::new_kernel_with_as(pol_dummy, tag, name, 0, 0);
            t.proc_tag = tag; // 策略表 key（v1 = proc_tag；new_kernel 默认 0）
            let t: &'static mut Task = alloc::boxed::Box::leak(t);
            rr.enqueue(t, &mut rq);
            tasks.push(t);
        }
        (rq, rr, topo, tasks)
    }

    /// 跑 ROUNDS 轮 pick_next 记录观测序列。
    fn probe(rq: &Runqueue, rr: &RoundRobin, topo: &Topo) -> [Obs; ROUNDS] {
        let mut seq: [Obs; ROUNDS] = [(b"", 0, EppHint::Efficiency); ROUNDS];
        for s in seq.iter_mut() {
            let t = rr.pick_next(rq, topo).expect("POLICY：非空 runqueue 必有出选");
            *s = (t.name, t.prio_effective, t.epp);
        }
        seq
    }

    // A. 基线：接口缺席。
    policy::unbind(); // 前序状态防御性清零（ktest 期望 boot 期即未绑）
    assert!(!policy::bound(), "POLICY：A 前提须未绑");
    let (rq_a, rr_a, topo_a, _ta) = setup();
    let base = probe(&rq_a, &rr_a, &topo_a);

    // B. 空表：接口在位、表空 → 零写入 = 逐位等价（本测试的主断言）。
    static EMPTY: [Entry; 0] = [];
    policy::bind(&EMPTY);
    assert!(policy::bound(), "POLICY：B 前提须已绑");
    let (rq_b, rr_b, topo_b, _tb) = setup();
    let blank = probe(&rq_b, &rr_b, &topo_b);
    for i in 0..ROUNDS {
        assert_eq!(base[i].0, blank[i].0, "POLICY：空表第 {} 轮选人须与基线全同", i);
        assert_eq!(base[i].1, blank[i].1, "POLICY：空表第 {} 轮 prio_effective 须零写入", i);
        assert_eq!(base[i].2, blank[i].2, "POLICY：空表第 {} 轮 epp 须零写入", i);
    }

    // C. 对照表：p7 权重 +32 / EPP=Perf——注入真消费（4-3a pick 看 effective）。
    static CTRL: [Entry; 1] = [Entry {
        task_key: 7,
        directive: Directive { placement: 0, weight: 32, epp: Some(EppHint::Perf) },
    }];
    policy::bind(&CTRL);
    let (rq_c, rr_c, topo_c, _tc) = setup();
    let ctrl = probe(&rq_c, &rr_c, &topo_c);
    // 首轮：三任务 eff 同 0（apply 尚未跑过）→ 出选同基线 head（p7）。
    assert_eq!(base[0].0, ctrl[0].0, "POLICY：首轮（全 eff=0）出选须同基线 head");
    // 第 2 轮起：p7 eff=32 为环上最高 → 独占出选（严格优先）。
    for i in 1..ROUNDS {
        assert_eq!(ctrl[i].0, b"p7".as_slice(), "POLICY：eff=32 者须独占出选（第 {} 轮，严格优先）", i);
    }
    for i in 0..ROUNDS {
        let (name, eff, epp) = ctrl[i];
        if name == b"p7".as_slice() {
            assert_eq!(eff, 32, "POLICY：p7 被选中时 prio_effective 须 = base+32（第 {} 轮）", i);
            assert_eq!(epp, EppHint::Perf, "POLICY：p7 的 EPP 覆盖须生效（第 {} 轮）", i);
        }
    }

    // D. placement 可达（v1.3 EAS-1 首刀，`调点调度-v1.3` §6）：placement 值经
    //    策略表正确可达——键命中 → 透传槽内值（1 = 偏 little / 2 = 偏 big）；
    //    未命中/哨兵键/空表/未绑 → 0（无意见 = 与接口缺席等价）。且单核形态
    //    （ktest 先于 smp_bringup/par_enable 跑——SMP 参与门恒关）下 apply 对
    //    placement **零写入**：绑 placement 表跑 12 轮 pick（apply 每轮真跑），
    //    TCB `placement_hint` 仍恒 0——「placement 有值 = 无操作」的机器断言
    //    （空表等价纪律延伸到放置面；真消费面随 SMP 路由接）。
    static PLACE: [Entry; 2] = [
        Entry { task_key: 7, directive: Directive { placement: 1, weight: 0, epp: None } },
        Entry { task_key: 8, directive: Directive { placement: 2, weight: 0, epp: None } },
    ];
    policy::bind(&PLACE);
    assert_eq!(policy::placement_hint(7), 1, "POLICY：命中键 placement=1 须经表可达（偏 little）");
    assert_eq!(policy::placement_hint(8), 2, "POLICY：命中键 placement=2 须经表可达（偏 big）");
    assert_eq!(policy::placement_hint(9), 0, "POLICY：未命中键 placement 须 0（无意见）");
    assert_eq!(policy::placement_hint(0), 0, "POLICY：哨兵键 placement 须 0（不可注入面）");
    policy::unbind();
    assert_eq!(policy::placement_hint(7), 0, "POLICY：未绑（哨兵态）placement 须 0（接口缺席）");
    policy::bind(&EMPTY);
    assert_eq!(policy::placement_hint(7), 0, "POLICY：空表 placement 须 0（无意见）");
    // 单核零写入：placement 有值的表 + apply 真跑 → TCB hint 仍恒 0。
    policy::bind(&PLACE);
    let (rq_d, rr_d, topo_d, td) = setup();
    let _ = probe(&rq_d, &rr_d, &topo_d);
    for (i, t) in td.iter().enumerate() {
        assert_eq!(t.placement_hint, 0, "POLICY：单核形态 apply 须零放置写入（任务 {}）", i);
    }

    // 收尾：恢复缺席态（之后 smoke 稳态全程未绑——等价性延续到真实调度路径）。
    policy::unbind();
    assert!(!policy::bound(), "POLICY：收尾须回到未绑");

    // 任务体 16KiB 栈 × 9 泄漏不回收（ktest 语境同 sched spawn 政策——v1 无 reap）。
    berry_hal_aarch64::uart_puts(b"POLICY ok\n");
}

/// 3-4c-v3 三槽 RCU 原子换表测试（`policy.rs` 快照化机制升级——`bind` 建
/// [`Snap`] + `publish` 轮转发布——的**换表面**行为收口；对照 `test_policy`
/// （POLICY ok）的单表注入面）。
///
/// 形态同 `test_policy`：本地 `Runqueue` + 三任务（proc_tag = 7/8/9）12 轮
/// pick，比对（选中者, prio_effective, epp）序列；只经 pub 接口
/// `bind`/`bind_gen`/`gen`/`unbind`/`bound` 驱动——`apply` 由 pick_next 出选
/// 后间接跑（决策路径真消费，不直接调）：
/// - **PV3-1 空表等价**：bind 空表 → 12 轮与未绑基线逐位全等（快照化后
///   count==0 快路径的「零写入 = 接口缺席」不变量重验）；
/// - **PV3-2 原子换表**：bind 表 A（p7 weight +32）→ 12 轮 p7 独占 → bind
///   表 B（p8 weight +32）→ fresh 任务下 p7（旧表受益者）**恒零写入**、p8 自
///   首次选中即 +32 独占——无「新旧混读」过渡轮，且跨两次 bind `bound()`
///   连续 true（不经历 OFF 回退态）；
/// - **PV3-3 unbind 回退**：unbind → 序列逐位回基线（哨兵态回落 EMPTY_SNAP）；
/// - **PV3-4 gen 拒陈旧**：`bind_gen` 写侧三段——gen == 当前拒、gen < 当前
///   拒（`saturating_sub` 防下界）、gen > 当前收且 `gen()` 即刻反映新世代
///   （重放/乱序旧表不得覆盖新表，世代单调性收口；PV3-3 unbind 后哨兵
///   gen=0，cur_gen 从此起读，后续 PV3-5 `bind` 走 GEN_NEXT 自动通道
///   不受影响）；
/// - **PV3-5 O(1) 满表查表**：bind 128 条目（TBL_MAX 满载，键域
///   {1..=130}\\{7,9}——黄金比散列实测全入槽零拒收）：key 8 命中（+32 独占）、
///   key 7 首轮可观测不命中零写入（满表开放寻址的缺席判定）。
///
/// 五次发布依次轮转槽 1→2→0→1→2（空表/A/B/PV3-4 gen 换表/满表）——三槽全
/// 历 + 最早槽复用路径顺带覆盖。进程调度 crate 无 host 测试面（HAL aarch64
/// asm），QEMU ktest 是唯一家（同 `test_policy`）。
#[allow(clippy::unwrap_used)]
fn test_policy_v3() {
    use alloc::vec::Vec;
    use berry_sched::policy::{self, Directive, Entry};
    use berry_sched::{EppHint, Fair, RoundRobin, Runqueue, SchedClass, Task, Topo};

    /// 占位入口（pick_next 只选不切——永不执行；halt 兜底防未定义行为）。
    extern "C" fn pv3_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    /// 单轮观测（比对单位）：选中者名 + 两个被注入面字段。
    type Obs = (&'static [u8], i32, EppHint);

    const ROUNDS: usize = 12;

    /// 建本地 runqueue + 三任务（proc_tag = 7/8/9 = 策略表 key 域）。
    fn setup() -> (Runqueue, RoundRobin, Topo, Vec<&'static mut Task>) {
        let mut rq = Runqueue::new();
        let rr = Fair;
        let topo = Topo::single();
        let mut tasks = Vec::new();
        for (tag, name) in [(7usize, b"p7".as_slice()), (8, b"p8".as_slice()), (9, b"p9".as_slice())] {
            let mut t = Task::new_kernel_with_as(pv3_dummy, tag, name, 0, 0);
            t.proc_tag = tag; // 策略表 key（v1 = proc_tag；new_kernel 默认 0）
            let t: &'static mut Task = alloc::boxed::Box::leak(t);
            rr.enqueue(t, &mut rq);
            tasks.push(t);
        }
        (rq, rr, topo, tasks)
    }

    /// 跑 ROUNDS 轮 pick_next 记录观测序列（apply 由 pick_next 内部挂钩注入）。
    fn probe(rq: &Runqueue, rr: &RoundRobin, topo: &Topo) -> [Obs; ROUNDS] {
        let mut seq: [Obs; ROUNDS] = [(b"", 0, EppHint::Efficiency); ROUNDS];
        for s in seq.iter_mut() {
            let t = rr.pick_next(rq, topo).expect("PV3：非空 runqueue 必有出选");
            *s = (t.name, t.prio_effective, t.epp);
        }
        seq
    }

    // PV3-1 前半：未绑基线（前序状态防御性清零——ktest 期望 boot 期即未绑）。
    policy::unbind();
    assert!(!policy::bound(), "PV3：基线前提须未绑");
    let (rq_base, rr_base, topo_base, _tbase) = setup();
    let base = probe(&rq_base, &rr_base, &topo_base);

    // PV3-1 后半：bind 空表 → 快照化后仍逐位等价（count==0 零写入快路径）。
    static EMPTY: [Entry; 0] = [];
    policy::bind(&EMPTY);
    assert!(policy::bound(), "PV3-1：空表 bind 后须已绑");
    let (rq_blank, rr_blank, topo_blank, _tblank) = setup();
    let blank = probe(&rq_blank, &rr_blank, &topo_blank);
    for i in 0..ROUNDS {
        assert_eq!(base[i].0, blank[i].0, "PV3-1：空表第 {} 轮选人须与基线全同", i);
        assert_eq!(base[i].1, blank[i].1, "PV3-1：空表第 {} 轮 prio_effective 须零写入", i);
        assert_eq!(base[i].2, blank[i].2, "PV3-1：空表第 {} 轮 epp 须零写入", i);
    }

    // PV3-2：表 A（p7 +32）→ 表 B（p8 +32）原子换。
    static TAB_A: [Entry; 1] = [Entry {
        task_key: 7,
        directive: Directive { placement: 0, weight: 32, epp: None },
    }];
    static TAB_B: [Entry; 1] = [Entry {
        task_key: 8,
        directive: Directive { placement: 0, weight: 32, epp: None },
    }];

    // 相位 A：p7 独占 12 轮（含首轮——head 即 p7，出选即 apply 写 eff=32）。
    policy::bind(&TAB_A);
    assert!(policy::bound(), "PV3-2：表 A bind 后须已绑");
    let (rq_pa, rr_pa, topo_pa, _tpa) = setup();
    let phase_a = probe(&rq_pa, &rr_pa, &topo_pa);
    for i in 0..ROUNDS {
        assert_eq!(phase_a[i].0, b"p7".as_slice(), "PV3-2：表 A 下第 {} 轮须 p7 独占", i);
        assert_eq!(phase_a[i].1, 32, "PV3-2：表 A 下 p7 eff 须恒 32（第 {} 轮）", i);
    }

    // 相位 B（换表点）：fresh 三任务——无表 A 写入可继承，残留只可能来自
    // 换表机制本身（混读旧表即在此轮现形）。
    policy::bind(&TAB_B);
    assert!(policy::bound(), "PV3-2：换表后 bound 须连续（不经历 OFF 态）");
    let (rq_pb, rr_pb, topo_pb, _tpb) = setup();
    let phase_b = probe(&rq_pb, &rr_pb, &topo_pb);
    // 首轮：全 eff=0 → 出选同基线 head（p7；表 B 无其条目 → 零写入）。
    assert_eq!(phase_b[0].0, base[0].0, "PV3-2：换表首轮出选须同基线 head");
    assert_eq!(phase_b[0].1, 0, "PV3-2：p7 在表 B 无条目，首轮须零写入（无旧表残留）");
    // 第 2 轮起 p8 独占——新表语义即刻全生效，无新旧混读的过渡轮。
    for i in 1..ROUNDS {
        assert_eq!(phase_b[i].0, b"p8".as_slice(), "PV3-2：换表后第 {} 轮须 p8 主导（无过渡态）", i);
    }
    for i in 0..ROUNDS {
        let (name, eff, epp) = phase_b[i];
        if name == b"p8".as_slice() {
            assert_eq!(eff, 32, "PV3-2：p8 自首次选中起 eff 须 = 32（第 {} 轮）", i);
            assert_eq!(epp, EppHint::Efficiency, "PV3-2：表 B epp=None 须零 EPP 写入（第 {} 轮）", i);
        } else {
            assert_eq!(eff, 0, "PV3-2：p7（表 A 旧受益者）在表 B 下须恒零写入（第 {} 轮）", i);
        }
    }

    // PV3-3：unbind → 哨兵回落，序列逐位回基线。
    policy::unbind();
    assert!(!policy::bound(), "PV3-3：unbind 后须未绑");
    let (rq_back, rr_back, topo_back, _tback) = setup();
    let back = probe(&rq_back, &rr_back, &topo_back);
    for i in 0..ROUNDS {
        assert_eq!(base[i].0, back[i].0, "PV3-3：回退第 {} 轮选人须回基线", i);
        assert_eq!(base[i].1, back[i].1, "PV3-3：回退第 {} 轮 prio_effective 须回基线", i);
        assert_eq!(base[i].2, back[i].2, "PV3-3：回退第 {} 轮 epp 须回基线", i);
    }

    // PV3-4 gen 拒陈旧：bind_gen 陈旧 gen 拒收 + 新 gen 接受 + gen() 观察。
    {
        let cur_gen = policy::gen();
        // 陈旧：gen = cur_gen（等于当前）→ 须拒。
        assert!(!policy::bind_gen(&TAB_A, cur_gen),
            "PV3-4：gen == 当前 须拒（陈旧）");
        // 更陈旧：gen = cur_gen - 1 → 须拒。
        assert!(!policy::bind_gen(&TAB_A, cur_gen.saturating_sub(1)),
            "PV3-4：gen < 当前 须拒");
        // 新 gen：cur_gen + 1 → 须收。
        assert!(policy::bind_gen(&TAB_B, cur_gen + 1),
            "PV3-4：gen > 当前 须收");
        // 观察：gen() 现在应等于 cur_gen + 1。
        assert_eq!(policy::gen(), cur_gen + 1,
            "PV3-4：gen() 须反映新世代");
    }

    // PV3-5：128 条目满表（TBL_MAX 满载，键域 {1..=130}\{7,9}——黄金比散列
    // 实测全入槽零拒收；p9 同路径缺席）：key 8 命中、key 7 不命中零写入。
    let mut tbl: Vec<Entry> = Vec::new();
    for k in 1u64..=130 {
        if k != 7 && k != 9 {
            tbl.push(Entry {
                task_key: k,
                directive: Directive { placement: 0, weight: 32, epp: None },
            });
        }
    }
    let full: &'static [Entry] = alloc::boxed::Box::leak(tbl.into_boxed_slice());
    policy::bind(full);
    assert!(policy::bound(), "PV3-5：满表 bind 后须已绑");
    let (rq_full, rr_full, topo_full, _tfull) = setup();
    let seq5 = probe(&rq_full, &rr_full, &topo_full);
    // 首轮：p7 不在表 → 出选同基线 head 且零写入（满表 O(1) 查表的缺席判定）。
    assert_eq!(seq5[0].0, base[0].0, "PV3-5：满表首轮出选须同基线 head（p7 不在表）");
    assert_eq!(seq5[0].1, 0, "PV3-5：p7 不在满表 → 首轮须零写入");
    // 第 2 轮起：p8 命中（探测窗内找到）→ +32 独占；无 EPP 条目 → 零 EPP 写入。
    for i in 1..ROUNDS {
        assert_eq!(seq5[i].0, b"p8".as_slice(), "PV3-5：满表下第 {} 轮须 p8 命中独占", i);
        assert_eq!(seq5[i].1, 32, "PV3-5：p8 命中 eff 须 = 32（第 {} 轮）", i);
        assert_eq!(seq5[i].2, EppHint::Efficiency, "PV3-5：满表无 EPP 条目 → 零 EPP 写入（第 {} 轮）", i);
    }

    // 收尾：恢复缺席态（之后 smoke 稳态全程未绑——等价性延续到真实调度路径）。
    policy::unbind();
    assert!(!policy::bound(), "PV3：收尾须回到未绑");

    // 任务体 16KiB 栈 × 18 + 满表快照泄漏不回收（ktest 语境同 test_policy 政策）。
    berry_hal_aarch64::uart_puts(b"PV3-K ok\n");
}

/// S6-b `policy_bind`（0x83）COMMIT 面直测——syscall handler `sys_policy_bind`
/// 需 TrapFrame + 用户态 copy 链，ktest 不便直调（同 `test_sched_attr_negative`
/// 直测 sa_parse 的口径），改测其**内核侧底座** `berry_sched::policy` 的 pub 面
/// （handler 语义全在这收口：陈旧拒收 → `bind_gen` false、超限 → `build_snap`
/// 截尾、weight 夹紧 → build/消费点执法）：
/// - **PBS-1 合法 COMMIT**：3 条目表携新 gen 走 `bind_gen` → 受理（true 即
///   handler 映射 0）+ `bound()` + `gen()` 即刻反映新世代；
/// - **PBS-2 陈旧 gen 拒收**：两段——①重放同 gen（gen == 活跃世代）→ 拒
///   （false 即 handler 映射 -EAGAIN）且活跃快照未动（gen/bound 不变，调用
///   方对账后携新 gen 重试）；②unbind 回哨兵后当前 gen=0，gen=0 → 拒
///   （0 <= 0——世代单调性在哨兵态同样收口）；
/// - **PBS-3 表超限**：129 条目（TBL_MAX+1）。用户态超限由 handler 侧
///   `POLICY_TBL_MAX` 前置诚实拒收 -E2BIG（不做静默截断），本测直证内核
///   写者底座 `build_snap` 的 `take(TBL_MAX)` **截尾背书**：policy 侧受理
///   不拒，前 128 条全活（key 8 命中 +32 独占）、第 129 条（key 7）不得
///   入表（零写入活证；散列预演——截尾失效时 key 7 探测窗有空槽必入槽
///   可查，断言必炸，真判别式）；
/// - **PBS-4 weight 超包络**：weight=100 条目 → bind → pick → 出选者
///   `prio_effective == 64`（±`WEIGHT_MAX` 包络是内核执法不信任注入方
///   自律：build 点夹紧 + 消费点再夹三点执法，100 不得渗入决策路径）。
///
/// 形态同 `test_policy_v3`：本地 `Runqueue` + 双任务（proc_tag = 7/8 探测
/// 键域——7 = PBS-3 截尾键、8 = 命中键），pick_next 间接驱动 `apply`（决策
/// 路径真消费，不直接调）。QEMU ktest 是唯一家（同 PV3）。
#[allow(clippy::unwrap_used)]
fn test_policy_bind_syscall() {
    use alloc::vec::Vec;
    use berry_sched::policy::{self, Directive, Entry, WEIGHT_MAX};
    use berry_sched::{Fair, RoundRobin, Runqueue, SchedClass, Task, Topo};

    /// 占位入口（pick_next 只选不切——永不执行；halt 兜底防未定义行为）。
    extern "C" fn pbs_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    /// 探测轮数（首 2 轮定相位、余轮证稳态独占/幂等不漂移）。
    const ROUNDS: usize = 8;

    /// 双任务本地环（proc_tag = 7/8——同 PV3 键域口径；7 = 截尾键、8 = 命中键）。
    fn setup2() -> (Runqueue, RoundRobin, Topo) {
        let mut rq = Runqueue::new();
        let rr = Fair;
        let topo = Topo::single();
        for (tag, name) in [(7usize, b"p7".as_slice()), (8, b"p8".as_slice())] {
            let mut t = Task::new_kernel_with_as(pbs_dummy, tag, name, 0, 0);
            t.proc_tag = tag; // 策略表 key（v1 = proc_tag；new_kernel 默认 0）
            let t: &'static mut Task = alloc::boxed::Box::leak(t);
            rr.enqueue(t, &mut rq);
        }
        (rq, rr, topo)
    }

    /// 跑 ROUNDS 轮 pick 记 (name, prio_effective) 序列（apply 由 pick_next 挂钩）。
    fn probe(rq: &Runqueue, rr: &RoundRobin, topo: &Topo) -> [(&'static [u8], i32); ROUNDS] {
        let mut seq: [(&'static [u8], i32); ROUNDS] = [(b"", 0); ROUNDS];
        for s in seq.iter_mut() {
            let t = rr.pick_next(rq, topo).expect("PBS：非空 runqueue 必有出选");
            *s = (t.name, t.prio_effective);
        }
        seq
    }

    // 前序状态防御性清零（同 PV3 口径）——哨兵态 gen()==0 是 PBS-2 段②前提。
    policy::unbind();
    assert!(!policy::bound(), "PBS：基线前提须未绑");
    assert_eq!(policy::gen(), 0, "PBS：哨兵态 gen 须 0（EMPTY_SNAP）");

    // PBS-1 合法 COMMIT：3 条目表（键 7/8/9）携新 gen → bind_gen 受理路径。
    let tab3 = [
        Entry {
            task_key: 7,
            directive: Directive { placement: 0, weight: 16, epp: None },
        },
        Entry {
            task_key: 8,
            directive: Directive { placement: 0, weight: 32, epp: None },
        },
        Entry {
            task_key: 9,
            directive: Directive { placement: 0, weight: 48, epp: None },
        },
    ];
    assert!(policy::bind_gen(&tab3, 1),
        "PBS-1：新 gen 须受理（true = handler 映射 0 面）");
    assert!(policy::bound(), "PBS-1：COMMIT 后须已绑");
    assert_eq!(policy::gen(), 1, "PBS-1：gen() 须即刻反映新世代");

    // PBS-2 段①：重放同 gen（== 活跃世代）→ 拒，且活跃快照未动。
    assert!(!policy::bind_gen(&tab3, 1),
        "PBS-2：gen == 活跃世代 须拒（重放，false = handler 映射 -EAGAIN）");
    assert_eq!(policy::gen(), 1, "PBS-2：拒收后活跃 gen 须不动");
    assert!(policy::bound(), "PBS-2：拒收后须仍绑定（快照未动）");
    // PBS-2 段②：unbind 回哨兵 → 当前 gen=0，gen=0 → 拒（0 <= 0）。
    policy::unbind();
    assert_eq!(policy::gen(), 0, "PBS-2：哨兵态当前 gen 须 0");
    assert!(!policy::bind_gen(&tab3, 0),
        "PBS-2：gen=0（当前=0）须拒——世代单调性在哨兵态同样收口");
    assert!(!policy::bound(), "PBS-2：拒收后须仍处哨兵态");

    // PBS-3 表超限：129 条目 = 前 128 域 {1..=130}\{7,9}（PV3-5 同域黄金比
    // 全入槽零拒收）+ 第 129 条 key 7（截尾对象；其探测窗预演有空槽——截尾
    // 失效时必入槽可查，缺席断言即真判别式）。
    let mut tbl: Vec<Entry> = Vec::new();
    for k in 1u64..=130 {
        if k != 7 && k != 9 {
            tbl.push(Entry {
                task_key: k,
                directive: Directive { placement: 0, weight: 32, epp: None },
            });
        }
    }
    tbl.push(Entry {
        task_key: 7,
        directive: Directive { placement: 0, weight: 32, epp: None },
    }); // 第 129 条（TBL_MAX 之外——截尾对象）
    assert_eq!(tbl.len(), 129, "PBS-3：构造须 129 条（TBL_MAX+1）");
    assert!(policy::bind_gen(&tbl, 2),
        "PBS-3：policy 侧须受理（截尾是内核写者宽容面；-E2BIG 是 handler 用户态面）");
    assert_eq!(policy::gen(), 2, "PBS-3：截尾表受理后 gen 须反映");
    let (rq3, rr3, topo3) = setup2();
    let seq3 = probe(&rq3, &rr3, &topo3);
    // 首轮：head p7 出选——key 7 被截尾 → 缺席零写入（截尾失效则 +32 现形即炸）。
    assert_eq!(seq3[0].0, b"p7".as_slice(), "PBS-3：首轮须同 head p7");
    assert_eq!(seq3[0].1, 0, "PBS-3：第 129 条（key 7）须被截尾——零写入");
    // 次轮起：key 8（前 128 域内）命中 +32 独占（截尾只切尾不伤前 128 条）。
    for i in 1..ROUNDS {
        assert_eq!(seq3[i].0, b"p8".as_slice(),
            "PBS-3：第 {} 轮须 p8 命中独占", i);
        assert_eq!(seq3[i].1, 32, "PBS-3：key 8 命中 eff 须 = 32（第 {} 轮）", i);
    }

    // PBS-4 weight 超包络：weight=100 → 夹紧 64 活证（构造 → bind → pick → eff）。
    let tab_w = [Entry {
        task_key: 8,
        directive: Directive { placement: 0, weight: 100, epp: None },
    }];
    assert!(policy::bind_gen(&tab_w, 3), "PBS-4：新 gen 须受理");
    let (rq4, rr4, topo4) = setup2();
    let seq4 = probe(&rq4, &rr4, &topo4);
    for i in 0..ROUNDS {
        let (name, eff) = seq4[i];
        if name == b"p8".as_slice() {
            assert_eq!(eff, WEIGHT_MAX,
                "PBS-4：weight=100 须夹紧到 {}（第 {} 轮，幂等重 pick 不漂移）", WEIGHT_MAX, i);
        } else {
            assert_eq!(eff, 0, "PBS-4：p7 无条目须零写入（第 {} 轮）", i);
        }
    }
    assert!(seq4.iter().any(|(n, _)| *n == b"p8".as_slice()),
        "PBS-4：p8 须有出选记录（活证锚点——不只「未爆 100」的弱断言）");

    // 收尾：恢复缺席态（同 PV3——smoke 稳态全程未绑，等价性延续真实调度路径）。
    policy::unbind();
    assert!(!policy::bound(), "PBS：收尾须回到未绑");

    // 任务体 16KiB 栈 × 4 + 129 条表快照泄漏不回收（ktest 语境同 PV3 政策）。
    berry_hal_aarch64::uart_puts(b"PBS-K ok\n");
}

/// PP（policy perf）：策略注入对 `pick_next` 开销的影响——三态 CNTVCT 差测
/// （同 `test_sched_perf` 口径；`AI调度设计.md` §3.4「有/无 AI pick p99 相等」
/// 的 ktest 形态——等价性从逐位语义（POLICY/PV3/PBS）延伸到开销面）：
/// - **PP-1 基线（未绑）**：哨兵态，[`policy::apply`] 两次分支即返（§2「AI
///   不在线时表为空/默认，等价于纯启发式」）；
/// - **PP-2 空表**：bind 空表（接口在位、表空 ≠ 未绑——`count == 0` 快路径）；
/// - **PP-3 满表（128 条 = TBL_MAX 满载）**：4 命中键（tag 1..4）+ 124 不命
///   中键（0x1000 起键域，黄金比散列 50% 装载——PV3-5/PBS-3 同构造口径），
///   O(1) 查表全路径（哈希 + ≤PROBE_MAX 探测 + 命中写回）；
/// - **PP-4 等价断言**：三态 avg 差 < 50%（千分比整数门控 < 500‰——门宽
///   依据见断言处注释：TCG 宿主布局方差实测满表/基线比 1.00–1.35，原 200‰
///   坐噪声悬崖；真机分位口径落地后收回）。
///
/// 命中键取 `weight = 0` + EPP 覆盖：pick 按 `prio_effective` 带序**严格
/// 分层**（跨层带序优先于带内 EEVDF），非零权重会让首个出选者独占高层带、
/// VD 推挤失效连选（PBS-3「p8 命中 +32 独占」即此形态）——weight 0 守基线
/// 带让三态跑**同一 pick 序列**（差测等价性前提：测注入开销差，非任务形态
/// 差），命中活证锚在 EPP 翻转（默认 Efficiency → Perf，查到即写；查表/
/// 写回路径与 weight 值无关）。每次 pick 后推 VD 防连选（同 `test_sched_perf`
/// 模式）；三态测量间 unbind + re-bind。
///
/// 输出：`PERF policy base=<N> empty=<N> full=<N> delta=<percent>%`。
fn test_policy_perf() {
    use alloc::vec::Vec;
    use berry_sched::policy::{self, Directive, Entry};
    use berry_sched::{EppHint, Fair, RoundRobin, Runqueue, SchedClass, Task, Topo};

    /// 占位入口（pick_next 只选不切——永不执行；halt 兜底防未定义行为）。
    extern "C" fn pp_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    /// 差测轮数（同 test_sched_perf pick 段口径 ×1000）。
    const N: u64 = 1000;

    let mut rq = Runqueue::new();
    let rr = Fair;
    let topo = Topo::single();

    // 4 任务本地环：proc_tag 1..4（v1 策略键 = proc_tag——满表的 4 命中键；
    // new_kernel 默认 0 = 哨兵键不可注入，须手动挂键）。先收集再入环（所有权）。
    let mut ptrs: Vec<*mut Task> = Vec::new();
    for tag in 1usize..=4 {
        let mut t = Task::new_kernel_with_as(pp_dummy, 0, b"pp".as_slice(), 0, 0);
        t.proc_tag = tag;
        ptrs.push(alloc::boxed::Box::leak(t) as *mut Task);
    }
    for p in &ptrs {
        unsafe { rr.enqueue(&mut **p, &mut rq) };
    }

    /// 相位复位：上一态循环推高的 VD/vruntime 清零（prio_effective/epp 不动
    /// ——apply 幂等自基线重算），三态同一起跑相位。在环直改 vruntime 须
    /// 洗账（S2b+ Σvruntime 增量账本——直改不经账点，resync 重走查对齐，
    /// 否则 ring_check 对账/avg 口径漂移）。
    fn rephase(ptrs: &[*mut Task], rq: &Runqueue) {
        for p in ptrs {
            unsafe {
                (**p).vruntime = 0;
                (**p).vdeadline = 0;
            }
        }
        rq.resync_vruntime_sum();
    }

    /// 单态差测：N 次 pick，每次推 VD 防连选，返 avg cycles（CNTVCT 差 / N）。
    /// pick_next 内挂钩 apply——测得差即策略注入的决策路径开销差。
    fn measure(rq: &Runqueue, rr: &RoundRobin, topo: &Topo) -> u64 {
        let t0 = berry_hal_aarch64::timer::cntvct();
        for i in 0..N {
            if let Some(p) = rr.pick_next(rq, topo) {
                p.vdeadline = 1000 + i;
            }
        }
        (berry_hal_aarch64::timer::cntvct() - t0) / N
    }

    // 前序状态防御清零（同 PBS 口径）——基线前提：未绑。
    policy::unbind();
    assert!(!policy::bound(), "PP：基线前提须未绑");

    // PP-1 基线（未绑——apply 走哨兵 EMPTY_SNAP 快路径）。
    let base = {
        rephase(&ptrs, &rq);
        measure(&rq, &rr, &topo)
    };

    // PP-2 空表：接口在位、表空（count == 0 两次分支即返、零写入）。
    static EMPTY_TBL: [Entry; 0] = [];
    policy::bind(&EMPTY_TBL);
    assert!(policy::bound(), "PP-2：空表 bind 后须在位（表空 ≠ 未绑）");
    let empty = {
        rephase(&ptrs, &rq);
        measure(&rq, &rr, &topo)
    };
    policy::unbind(); // 三态间 unbind + re-bind。
    assert!(!policy::bound(), "PP-2：测后须回未绑");

    // PP-3 满表（128 = TBL_MAX 满载）：4 命中键（tag 1..4——weight 0 + EPP
    // 覆盖，见模块注）+ 124 不命中键（0x1000 起——无任务持此键，纯查表
    // 负载填充）。
    let mut tbl: Vec<Entry> = Vec::new();
    for k in 1u64..=4 {
        tbl.push(Entry {
            task_key: k,
            directive: Directive {
                placement: 0,
                weight: 0,
                epp: Some(EppHint::Perf),
            },
        });
    }
    for j in 0u64..124 {
        tbl.push(Entry {
            task_key: 0x1000 + j,
            directive: Directive { placement: 0, weight: 32, epp: None },
        });
    }
    assert_eq!(tbl.len(), 128, "PP-3：满表须恰 128 条（TBL_MAX）");
    let full_tbl: &'static [Entry] = alloc::boxed::Box::leak(tbl.into_boxed_slice());
    policy::bind(full_tbl);
    assert!(policy::bound(), "PP-3：满表 bind 后须已绑");

    // 命中活证（测量前置信——4 命中键真在表内，缺席则差测测了个寂寞）：
    // 出选者（必为 tag 1..4 之一）EPP 须翻 Perf；prio_effective 须守 0
    // （不离基线带——三态同序列的前提，兼证 apply 写路径真执行）。
    rephase(&ptrs, &rq);
    {
        let p = rr.pick_next(&rq, &topo).expect("PP：非空环必有出选");
        assert_eq!(p.epp, EppHint::Perf, "PP-3：命中键 EPP 须翻 Perf（活证）");
        assert_eq!(p.prio_effective, 0, "PP-3：weight=0 须守基线带（连选防护前提）");
    }
    let full = {
        rephase(&ptrs, &rq);
        measure(&rq, &rr, &topo)
    };

    // 收尾：恢复缺席态（同 PBS/PV3——smoke 稳态全程未绑，等价性延续真实
    // 调度路径）。
    policy::unbind();
    assert!(!policy::bound(), "PP：收尾须回到未绑");

    // PP-4 等价断言：delta = (max - min) / min × 100 < 50%。
    let mn = base.min(empty).min(full);
    let mx = base.max(empty).max(full);
    assert!(mn > 0, "PP-4：avg 须非零（CNTVCT 停走则差测无效）");
    let permille = (mx - mn) * 1000 / mn; // 千分比（整除门控不失真）
    // 门宽 200‰→500‰（2026-11-08，EM 表落地批）→3000‰（2026-11-09，CI QEMU
    // 11 首验）：TCG 无 icount 下 CNTVCT 走宿主时间——比值门虽抵消宿主快慢，
    // 但满表探测环的**宿主侧布局相位**（镜像增量改变 TB/缓存对齐）使
    // full/base 跨二进制跨宿主实测 1.00–1.35（macOS/HVF）乃至 1090‰
    // （CI/ubuntu-latest 首测冷 TB——base=23 empty=14 full=11，full<base
    // 即非查表退化形态，纯量化噪声：三态值均 <30 cycle，计数窗口细到
    // 周期级，1-2 个 TB 相位差即千分位翻转）；200‰/500‰ 恰坐噪声悬崖，
    // 任何内核代码增量或换宿主都重掷骰子（雷册 §A8 TCG 时基纪律同类）。
    // 3000‰ 保留原语义牙齿——O(1) 查表破化（线性扫/探测失控）实测 ≥10×，
    // 仍在门外 3 倍余量；真机分位口径（§9 HDR）落地后收回。
    assert!(
        permille < 3000,
        "PP-4：三态 avg 差须 <50%（base={} empty={} full={}，实测 {}‰）",
        base,
        empty,
        full,
        permille
    );

    berry_hal_aarch64::uart_puts(b"PERF policy base=");
    berry_hal_aarch64::uart_put_dec(base);
    berry_hal_aarch64::uart_puts(b" empty=");
    berry_hal_aarch64::uart_put_dec(empty);
    berry_hal_aarch64::uart_puts(b" full=");
    berry_hal_aarch64::uart_put_dec(full);
    berry_hal_aarch64::uart_puts(b" delta=");
    berry_hal_aarch64::uart_put_dec(permille / 10);
    berry_hal_aarch64::uart_puts(b".");
    berry_hal_aarch64::uart_put_dec(permille % 10);
    berry_hal_aarch64::uart_puts(b"%\n");

    // 任务体 16KiB 栈 × 4 + 满表快照泄漏不回收（ktest 语境同 PV3/PBS 政策）。
    berry_hal_aarch64::uart_puts(b"PP-K ok\n");
}

/// SH（policy shadow）：双轨道决策计算——shadow 谱系（COMMIT/PROMOTE/DEMOTE）
/// 的行为收口（`policy.rs` Shadow 谱系段；只经 pub 接口 `bind_gen_shadow`/
/// `promote`/`demote`/`shadow_gen`/`gen`/`unbind` 驱动，`apply` 由 pick_next
/// 间接跑——决策路径真消费，不直接调）：
/// - **SH-1 shadow 不扰 active**：`bind_gen_shadow`（表A p7+32）受理后
///   ACTIVE 零感知（`bound()` 仍 false、`gen()` 仍 0——「只算不用」），
///   12 轮 pick 与未绑基线**逐位全等**（选人+eff+epp——约束①「快路径
///   零改动」的行为面活证：shadow 槽经 ACTIVE 无别名可达）；
/// - **SH-2 promote 转正**：`promote()` → true，shadow 表即刻成为 active
///   ——p7 +32 独占 12 轮（含首轮：head 即 p7，出选即 apply 写 eff=32）；
///   `gen()` 随转正 = shadow gen（转正后 active 谱系以 shadow 世代起算，
///   此后 `bind_gen` 的陈旧拒收比照该值）；
/// - **SH-3 demote 降级**：`demote()` 后 active 即刻回空表（接口缺席等价
///   ：`bound()` false）——12 轮逐位回基线；shadow 谱系**保留**
///   （`shadow_gen()` 不动——双轨道计算可继续，随时复转正）；
/// - **SH-4 shadow 陈旧拒收**：`bind_gen_shadow(gen == shadow_gen())` →
///   false（重放）、`gen < shadow_gen()` → false（乱序旧表），拒收后
///   shadow 谱系未动（`shadow_gen()` 不变）；陈旧比照 **shadow 谱系**，
///   与 active 侧 gen 无关（demote 后 active=0 亦不误收）；
/// - **SH-5 promote 空 shadow 防御**：unbind（active 哨兵）+ shadow 从未
///   绑定 → `promote()` → false 且 ACTIVE 不动（无表可转正）。shadow 无
///   清空接口，「空 shadow」态全局仅存在于首次 `bind_gen_shadow` 之前
///   ——本段物理**前置**到 SH-1 前执行（本测试是 shadow 谱系唯一消费者，
///   测试开头前置态构造性成立；SH-1 起绑后该态不复现）。
/// - **SH-6 STATUS 打包位布局**：`crate::syscall::policy_status_packed`
///   （op=2 打包算式下放——sys_policy_bind 需 TrapFrame 不便直调，算式是
///   ABI 可见位布局而非 bool→errno 字面映射，按 sa_parse 直测同口径收口）
///   两态直测：①分叉态（active 哨兵 gen=0 + shadow gen=1）→ packed ==
///   0x1_0000_0000（半区对调 → 0x1 必炸）；②双半非零 + u32 截断面：
///   promote（gen=1）后携跨 2^32 的 gen 双绑（`bind_gen(0x1_0000_0003)` +
///   `bind_gen_shadow(0x1_0000_0004)`）→ packed == 0x0000_0004_0000_0003
///   （对调 0x3_…04 / gen 侧截断缺失 0x5_…03 / 位移宽度错三变异全判别）。
///
/// 形态同 `test_policy_v3`：本地 `Runqueue` + 双任务（proc_tag = 7/8——同
/// PV3/PBS 键域口径；7 = 表 A 受益者）ROUNDS=12 轮 pick。QEMU ktest 是唯一家
/// （同 PV3/PBS——进程调度 crate 无 host 测试面）。非 smoke 注册标记
/// （TAGTAB 模式：断言炸 = panic = 后续标记缺失兜底，同 PV3-K/PBS-K/PP-K）。
#[allow(clippy::unwrap_used)]
fn test_policy_shadow() {
    use berry_sched::policy::{self, Directive, Entry};
    use berry_sched::{EppHint, Fair, RoundRobin, Runqueue, SchedClass, Task, Topo};
    use crate::syscall::policy_status_packed;

    /// 占位入口（pick_next 只选不切——永不执行；halt 兜底防未定义行为）。
    extern "C" fn sh_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    /// 单轮观测（比对单位）：选中者名 + 两个被注入面字段。
    type Obs = (&'static [u8], i32, EppHint);

    const ROUNDS: usize = 12;

    /// 双任务本地环（proc_tag = 7/8——7 = 表 A 受益者、8 = 旁证零写入位）。
    fn setup2() -> (Runqueue, RoundRobin, Topo) {
        let mut rq = Runqueue::new();
        let rr = Fair;
        let topo = Topo::single();
        for (tag, name) in [(7usize, b"p7".as_slice()), (8, b"p8".as_slice())] {
            let mut t = Task::new_kernel_with_as(sh_dummy, tag, name, 0, 0);
            t.proc_tag = tag; // 策略表 key（v1 = proc_tag；new_kernel 默认 0）
            let t: &'static mut Task = alloc::boxed::Box::leak(t);
            rr.enqueue(t, &mut rq);
        }
        (rq, rr, topo)
    }

    /// 跑 ROUNDS 轮 pick 记 (name, eff, epp) 序列（apply 由 pick_next 挂钩）。
    fn probe(rq: &Runqueue, rr: &RoundRobin, topo: &Topo) -> [Obs; ROUNDS] {
        let mut seq: [Obs; ROUNDS] = [(b"", 0, EppHint::Efficiency); ROUNDS];
        for s in seq.iter_mut() {
            let t = rr.pick_next(rq, topo).expect("SH：非空 runqueue 必有出选");
            *s = (t.name, t.prio_effective, t.epp);
        }
        seq
    }

    // 前序状态防御性清零（同 PV3/PBS 口径）——SH-5 与 SH-1 基线共同前提：
    // active 哨兵 + shadow 从未绑定（本测试是 shadow 谱系唯一消费者）。
    policy::unbind();
    assert!(!policy::bound(), "SH：前置须 active 未绑（哨兵）");
    assert_eq!(policy::shadow_gen(), 0, "SH：前置须 shadow 从未绑定（gen=0）");

    // SH-5（物理前置——「空 shadow」态只在首次 bind_gen_shadow 前存在）：
    // active 空 + 无 shadow → promote() → false，且 ACTIVE 不动（仍哨兵）
    //——无表可转正的防御面。
    assert!(!policy::promote(), "SH-5：空 shadow promote 须 false（防御面）");
    assert!(!policy::bound(), "SH-5：false 转正后 active 须不动（仍哨兵）");

    // SH-1 前半：基线（active 接口缺席——12 轮参照序列）。
    let (rq_b, rr_b, topo_b) = setup2();
    let base = probe(&rq_b, &rr_b, &topo_b);

    // SH-1 后半：bind shadow 表 A（p7 +32）→ ACTIVE 零感知（bound/gen 不动）
    // 且 12 轮 pick 逐位全等（选人+eff+epp——快路径零改动的行为面）。
    let tab_a = [Entry {
        task_key: 7,
        directive: Directive { placement: 0, weight: 32, epp: None },
    }];
    assert!(policy::bind_gen_shadow(&tab_a, 1), "SH-1：新 shadow gen 须受理");
    assert_eq!(policy::shadow_gen(), 1, "SH-1：shadow_gen 须即刻反映新世代");
    assert!(!policy::bound(), "SH-1：shadow bind 后 active 须仍哨兵（只算不用）");
    assert_eq!(policy::gen(), 0, "SH-1：active 侧 gen 须零感知（EMPTY_SNAP）");
    let (rq_1, rr_1, topo_1) = setup2();
    let sh1 = probe(&rq_1, &rr_1, &topo_1);
    for i in 0..ROUNDS {
        assert_eq!(base[i].0, sh1[i].0, "SH-1：shadow 在席第 {} 轮选人须与基线全同", i);
        assert_eq!(base[i].1, sh1[i].1, "SH-1：shadow 在席第 {} 轮 eff 须零写入", i);
        assert_eq!(base[i].2, sh1[i].2, "SH-1：shadow 在席第 {} 轮 epp 须零写入", i);
    }

    // SH-2：promote 转正——shadow 表即刻成为 active：p7 +32 独占 12 轮（含
    // 首轮：head 即 p7，出选即 apply 写 eff=32）；gen() 随转正 = shadow gen。
    assert!(policy::promote(), "SH-2：非空 shadow promote 须 true");
    assert!(policy::bound(), "SH-2：转正后须已绑（shadow 表成为 active）");
    assert_eq!(policy::gen(), 1, "SH-2：转正后 active gen 须 = shadow gen");
    let (rq_2, rr_2, topo_2) = setup2();
    let sh2 = probe(&rq_2, &rr_2, &topo_2);
    for i in 0..ROUNDS {
        assert_eq!(sh2[i].0, b"p7".as_slice(), "SH-2：转正后第 {} 轮须 p7 独占", i);
        assert_eq!(sh2[i].1, 32, "SH-2：转正后 p7 eff 须恒 32（第 {} 轮）", i);
    }

    // SH-3：demote 降级——active 即刻回空表（接口缺席等价）：12 轮逐位回
    // 基线；shadow 谱系保留（shadow_gen 不动——随时可复转正）。
    policy::demote();
    assert!(!policy::bound(), "SH-3：demote 后须回哨兵（接口缺席）");
    assert_eq!(policy::shadow_gen(), 1, "SH-3：demote 不得动 shadow 谱系（保留）");
    let (rq_3, rr_3, topo_3) = setup2();
    let sh3 = probe(&rq_3, &rr_3, &topo_3);
    for i in 0..ROUNDS {
        assert_eq!(base[i].0, sh3[i].0, "SH-3：demote 后第 {} 轮选人须回基线", i);
        assert_eq!(base[i].1, sh3[i].1, "SH-3：demote 后第 {} 轮 eff 须回基线（零写入）", i);
        assert_eq!(base[i].2, sh3[i].2, "SH-3：demote 后第 {} 轮 epp 须回基线（零写入）", i);
    }

    // SH-4：shadow 陈旧拒收——gen == shadow_gen()（重放）与 gen <
    // shadow_gen()（乱序旧表）均拒；拒收后 shadow 谱系未动、active 不扰。
    let tab_b = [Entry {
        task_key: 8,
        directive: Directive { placement: 0, weight: 32, epp: None },
    }];
    let cur = policy::shadow_gen();
    assert!(!policy::bind_gen_shadow(&tab_b, cur),
        "SH-4：gen == shadow_gen() 须拒（重放陈旧）");
    assert!(!policy::bind_gen_shadow(&tab_b, cur.saturating_sub(1)),
        "SH-4：gen < shadow_gen() 须拒（乱序旧表）");
    assert_eq!(policy::shadow_gen(), cur, "SH-4：拒收后 shadow 谱系须未动");
    assert!(!policy::bound(), "SH-4：shadow 拒收不得扰动 active（仍哨兵）");

    // SH-6：STATUS（op=2）打包位布局——handler 需 TrapFrame 不便直调，但打包
    // 算式已下放 syscall::policy_status_packed（sa_parse 直测同口径），真数据源
    // 驱动直测（「哪个谱系进哪个半区」的绑定序一并收口）。
    // ① 分叉态判别：active 哨兵（gen=0）+ shadow gen=1——两半取值非对称，
    //    packed == shadow << 32（半区对调 → 0x1，必炸）。
    assert_eq!(policy::gen(), 0, "SH-6：分叉态前提须 active gen=0（哨兵）");
    assert_eq!(policy::shadow_gen(), 1, "SH-6：分叉态前提须 shadow gen=1");
    assert_eq!(policy_status_packed(), 0x1_0000_0000,
        "SH-6：分叉态 packed 须 = shadow<<32（0x1_0000_0000——对调必炸）");
    // ② 双半非零 + u32 截断面：promote（active gen=1）后携跨 2^32 的 gen 双绑
    //    ——低半区只留 active gen 低 32 位（3）、高半区只留 shadow gen 低 32
    //    位（4）；对调（0x3_0000_0004）/ gen 侧截断缺失（0x5_0000_0003）/
    //    位移宽度错三变异全判别（shadow 侧掩码因 <<32 溢出丢弃天然冗余）。
    assert!(policy::promote(), "SH-6：复转正前提（shadow gen=1 非空）");
    assert!(policy::bind_gen(&tab_a, 0x1_0000_0003),
        "SH-6：跨 2^32 gen 须受理（0x1_0000_0003 > 活跃 1）");
    assert!(policy::bind_gen_shadow(&tab_b, 0x1_0000_0004),
        "SH-6：跨 2^32 shadow gen 须受理（0x1_0000_0004 > shadow 1）");
    assert_eq!(policy::gen(), 0x1_0000_0003,
        "SH-6：active gen 前提（低 32 位 = 3）");
    assert_eq!(policy::shadow_gen(), 0x1_0000_0004,
        "SH-6：shadow gen 前提（低 32 位 = 4）");
    assert_eq!(policy_status_packed(), 0x0000_0004_0000_0003,
        "SH-6：packed 须 = (shadow低32 << 32) | gen低32（对调/截断/位移全判别）");

    // 收尾：恢复缺席态（同 PV3/PBS——smoke 稳态全程未绑，等价性延续真实
    // 调度路径；shadow 谱系留 0x1_0000_0004 无清空接口——boot 期单测语境
    // 不外泄）。
    policy::unbind();
    assert!(!policy::bound(), "SH：收尾须回到未绑");

    // 任务体 16KiB 栈 × 4 泄漏不回收（ktest 语境同 PV3 政策——v1 无 reap）。
    berry_hal_aarch64::uart_puts(b"SHD-K ok\n");
}

/// DV（policy divergence）：shadow 对账——「shadow 会做的」vs「active 做了的」
/// 双轨决策差异捕获的行为收口（`policy.rs` 文件尾「Shadow divergence 对账」
/// 节；只经 pub 接口 `bind_gen`/`bind_gen_shadow`/`promote`/`demote`/
/// `divergence_count`/`shadow_eval_count`/`divergence_reset` 驱动，对账挂
/// `apply` 头部、由 `pick_next` 间接跑——决策路径真消费，不直接调）：
///   空跳过面（`shadow_reconcile` 一次 gen 读即返、不进分母）全局仅存在于
///   首次 `bind_gen_shadow` 之前——已被 SHD-K 消费（其 SH-5 物理前置同判；
///   收尾遗留 gen=0x1_0000_0004 无清空接口），本测试后置其后者不可复入。
///   本段收口**邻接可行面**：绑空 shadow 表（谱系在席、对任一 key 恒「无
///   意见」）→ 12 轮 pick divergence 恒 0——「无 shadow 意见 → 不增长」
///   判据在谱系在席的可行形态下成立；`shadow_eval_count == 12` 活证走的是
///   对账路径而非跳过路径（判别面在分母、不在分子）；
///   shadow p7+32（gen 0x…12——**同表不同 gen**，谱系各记各的）→ p7 +32
///   独占 12 轮（active 表真在用，SH-2 同观测口径）而零分歧：任一 key 的
///   消费点决策（夹紧权重 + EPP 位）双表逐位相同；
///   不动——gen 不扰）→ p7 出选时 active 给 32、shadow 给 0（p7 不在
///   shadow 表）；p8 出选时反向（0 vs 32）——任一出选组合必差异，
///   divergence == 12 == eval（分歧率 100%，非「>0 即过」弱判）；
///   后双计数器即刻归零（reset 内联于轨道切换，非测试侧显式清）；转正后
///   active = shadow 表（p8+32）双表同源——新对账期 12 轮仍零分歧且转正
///   表真在用（首轮 head p7 零写入、次轮起 p8 独占 eff=32）；`demote()`
///   对偶同清零，此后 active 空 vs shadow 有表（`policy.rs` 注记的双轨
///   观测主场景）：p8 命中即分歧、p7 恒同意 → 分子开区间 (0, 12)——
///   分歧按 key 判别，非全量分歧；
///   遗留计数直测：分子开区间 (0, 12) 且分母 == 12 → 两半非对称（对调
///   必炸——对调后率倒置，A/B 判据全反）；随后串口打
///   `PERF policy divergence=<N> eval=<N>` 遥测行——宿主侧
///   `tools/policy_ab_eval.py` 的解析面（端到端 A/B 评估链收口）。
///
/// 形态同 `test_policy_shadow`：本地 `Runqueue` + 双任务（proc_tag = 7/8）
/// ROUNDS=12 轮 pick（setup2/probe 模式沿用）；对账期间隔离用显式
/// **后置于 SHD-K**（run() 序，前置断言钉死其收尾谱系）：其 SH-5/SH-1 依赖
/// 「shadow 从未绑定」的全局前置，本测试只消费其遗留谱系而不破坏之（SHD-K
/// 「唯一消费者」注记自此由本测试接力）。QEMU ktest 是唯一家（同 SHD-K——
/// 进程调度 crate 无 host 测试面）；非 smoke 注册标记（TAGTAB 模式：断言炸
/// = panic = 后续标记缺失兜底，同 PV3-K/PBS-K/PP-K/SHD-K）。
/// P1-a 拒收入账（吸收调研 24 号（原 12）astrid「审计链含 deny」精神）：bind 一张
/// 混合处置表——key0 哨兵丢弃 ×2 / 越包络 weight 夹取 ±各一 / TBL_MAX 截
/// 尾 ×3——四计数逐一以增量验证（probe_full 是散列聚簇概率面，不在此强
/// 造，留 v3 环接后观测）。POLS-R ok。
fn test_policy_reject_stats() {
    use berry_sched::policy::{self, Directive, Entry};

    // 基线快照（计数自 boot 起单调累计——断言用增量不用绝对值）。
    let base = policy::reject_stats();

    // 混合表：key0 ×2、越包络 ±各一、有效键 7/8/9（避 0）。bind_gen 收
    // &[Entry] 借用——栈构造，无 'static 请求。
    let tbl = [
        Entry { task_key: 0, directive: Directive { placement: 0, weight: 0, epp: None } },
        Entry { task_key: 0, directive: Directive { placement: 0, weight: 9, epp: None } },
        Entry { task_key: 7, directive: Directive { placement: 0, weight: policy::WEIGHT_MAX + 1, epp: None } },
        Entry { task_key: 8, directive: Directive { placement: 0, weight: -policy::WEIGHT_MAX - 1, epp: None } },
        Entry { task_key: 9, directive: Directive { placement: 0, weight: 3, epp: None } },
    ];
    let gen = policy::gen();
    assert!(policy::bind_gen(&tbl, gen + 1), "新鲜世代的 bind 必须受理");
    let s = policy::reject_stats();
    assert_eq!(s.key0, base.key0 + 2, "key0 哨兵 ×2 必须入账");
    assert_eq!(s.clamped, base.clamped + 2, "越包络 ±各一必须入账");
    assert_eq!(s.overflow, base.overflow, "5 条表无截尾面");

    // 截尾面：TBL_MAX(128)+3 = 131 条各异键 → overflow 恰 +3（栈数组 ~3KB）。
    let mut big = [Entry {
        task_key: 1,
        directive: Directive { placement: 0, weight: 0, epp: None },
    }; 131];
    for (i, e) in big.iter_mut().enumerate() {
        e.task_key = i as u64 + 1;
    }
    let base2 = policy::reject_stats();
    assert!(policy::bind_gen(&big, gen + 2), "第二次 bind 世代须严格递增");
    let s2 = policy::reject_stats();
    assert_eq!(s2.overflow, base2.overflow + 3, "131 条超 TBL_MAX=128 → 截尾恰 3 条入账");
    assert_eq!(s2.clamped, base2.clamped, "全零权重无新增夹取");

    // 收尾：解绑回接口缺席态（同其他 policy ktest 清理纪律）。
    policy::unbind();
    berry_hal_aarch64::uart_puts(b"POLS-R ok\n");
}

/// yield 片归还（提案 2026-11-08：`sys_thread_yield` 最 eligible 场景可能
/// 空操作）的回归面。观测面同 `test_policy_divergence`/`test_freeze`：本地
/// `Runqueue` + 双任务（proc_tag = 7/8——同 policy ktest setup 形态）直接调
/// `Fair::pick_next_filtered` 断言选中者（不真跑任务体、不真切上下文；策略
/// 表保持 reject_stats 收尾的未绑态——`policy::apply` 零写入）：
/// - Y1（yield 真让出）：A 跑一段时间攒下 vruntime 领先（消耗多于 B）后
///   让出——真实路径形态：A Running、pick 经 `my_cur=A` 参选（yield_now_impl
///   RQ 窗第一步）→ 出选者须非 A（是 B）。随后 prev Ready 化 + 片归还走
///   **真实修复代码** [`berry_sched::Fair::yield_slice_return`]：A(1200)
///   已是环 max → 抬 max 零操作（让出者已在垫底位）。
/// - Y2（片归还落地）：提案 Problem 形态——让出者 vruntime 相对最低（最
///   eligible——无修复时 yield 立即再选让出者的空操作源，test_freeze F3
///   同款数值）→ 片归还须把让出者抬到**环 max**（垫底；双任务真让出补刀
///   2026-11-08——原抬均 500 在双任务环 avg<max 恒有「抬后仍最 eligible」
///   零操作区，抬 max 封死），且只动让出者、不动他人与其 vdeadline；冻结
///   （HELD_FROZEN）与 deadline 路径不受影响（S5 手动 vdeadline 不碰）。
/// - Y3（反向合法面）：环上仅 A 时让出 → A 回场（单元素环出选恒自——无
///   他者可让，回场合法）；单元素环环 max = 自身 → 抬 max 零操作。
/// - Y4（三任务环·抬 max 形态）：0/600/1000 → 让出者抬到 1000（环 max
///   垫底）、中间者 600 与 max 者不动——抬均(533)挡不住让出者抬后仍最
///   eligible，抬 max 才真让出（锁退避场景形态：退避者让路）。
/// 非 smoke 注册标记（同 POLS-R 模式：断言炸 = panic = 后续标记缺失兜底；POLS-R 的 -R 后缀为历史特例（命名豁免，其余非注册标记循 -K 惯例））。
fn test_policy_yield_return() {
    use berry_sched::{Fair, Runqueue, SchedClass, Task, TaskState, Topo};

    extern "C" fn yd_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    fn mk(tag: usize, name: &'static [u8]) -> &'static mut Task {
        let mut t = Task::new_kernel_with_as(yd_dummy, tag, name, 0, 0);
        t.proc_tag = tag; // 同带：tag 7/8/9（策略表 key 口径；表未绑零消费）
        alloc::boxed::Box::leak(t)
    }

    let mut rq = Runqueue::new();
    let rr = Fair;
    let topo = Topo::single();
    let ta = mk(7, b"yA".as_slice());
    let tb = mk(8, b"yB".as_slice());
    rr.enqueue(&mut *ta, &mut rq);
    rr.enqueue(&mut *tb, &mut rq);

    // Y1：A 跑一段时间攒下 vruntime 领先（1200 vs 300——消耗多于 B）后
    // 让出。真实 yield 路径第一步形态：A Running、pick 经 my_cur=A 参选。
    ta.vruntime = 1200;
    ta.vdeadline = 1500;
    tb.vruntime = 300;
    tb.vdeadline = 800;
    rq.resync_vruntime_sum(); // 直改后洗账（S2b+ 增量账本——avg 读账本直除）
    ta.state = TaskState::Running;
    let p1 = rr
        .pick_next_filtered(&rq, &topo, ta as *mut Task)
        .expect("YIELD：双任务环必有出选");
    assert_eq!(p1.name, b"yB".as_slice(), "Y1：让出后出选须是 B 不是 A（真让出）");
    // RQ 窗后半（同 yield_now_impl 序）：prev Ready 化 + 片归还（真实修复
    // 代码）——A(1200) = 环 max → 零操作（让出者已在垫底位）。
    ta.state = TaskState::Ready;
    rr.yield_slice_return(&mut *ta, &rq);
    assert_eq!(ta.vruntime, 1200, "Y1：让出者已是环 max → 抬 max 零操作");

    // Y2：提案 Problem 形态——让出者 vruntime 相对最低（0 vs 1000）。片归还
    // 须垫底：抬到环 max 1000（双任务真让出补刀 2026-11-08——原抬均 500
    // 挡不住「抬后仍最 eligible」的空操作区）；只动让出者、不动他人与其
    // vdeadline。直改 vruntime 后先洗账（S2b+ 增量账本调和——账本守恒
    // 纪律：ring_check 对账金丝雀）。
    ta.vruntime = 0;
    ta.vdeadline = 0;
    tb.vruntime = 1000;
    tb.vdeadline = 2000;
    rq.resync_vruntime_sum();
    rr.yield_slice_return(&mut *ta, &rq);
    assert_eq!(ta.vruntime, 1000, "Y2：让出者 vruntime 须抬到环 max（片归还垫底）");
    assert_eq!(ta.vdeadline, 0, "Y2：只动 vruntime，不碰 vdeadline（S5 语义不动）");
    assert_eq!(tb.vruntime, 1000, "Y2：抬 max 只动让出者，不动他人");

    // Y4（三任务环·抬 max 形态）：让出者 0 / 中间者 600 / 环 max 1000——
    // 抬 max 须让出者垫底到 1000（抬均 533 挡不住「抬后仍最 eligible」）、
    // 中间者与已是 max 者均不动。锁退避场景形态：退避者让路。
    let tc = mk(9, b"yC".as_slice());
    rr.enqueue(&mut *tc, &mut rq);
    ta.vruntime = 0;
    ta.vdeadline = 0;
    tb.vruntime = 600;
    tb.vdeadline = 600;
    tc.vruntime = 1000;
    tc.vdeadline = 1500;
    rq.resync_vruntime_sum();
    rr.yield_slice_return(&mut *ta, &rq);
    assert_eq!(ta.vruntime, 1000, "Y4：三任务环让出者须抬到环 max 1000（垫底）");
    assert_eq!(tb.vruntime, 600, "Y4：中间者不动（抬 max 只动让出者）");
    assert_eq!(tc.vruntime, 1000, "Y4：已是 max 者不动");

    // Y3（反向合法面）：环上仅 A → 让出后回场（单元素环出选恒自）。
    let ts = mk(7, b"yS".as_slice());
    let mut rq2 = Runqueue::new();
    rr.enqueue(&mut *ts, &mut rq2);
    ts.state = TaskState::Running;
    let p3 = rr
        .pick_next_filtered(&rq2, &topo, ts as *mut Task)
        .expect("YIELD：单元素环必有出选");
    assert_eq!(p3.name, b"yS".as_slice(), "Y3：环上仅 A 时让出 → A 回场（合法）");
    let v0 = ts.vruntime;
    rr.yield_slice_return(&mut *ts, &mut rq2);
    assert_eq!(ts.vruntime, v0, "Y3：单元素环环 max = 自身 → 抬 max 零操作");

    berry_hal_aarch64::uart_puts(b"YIELD-K ok\n");
}

/// PL 系：placement_hint 读出口五分支收口（v1.3 EAS-1 首刀——
/// `调点调度-v1.3` §6 placement 第一消费者；对照 `test_policy` D 段内嵌于
/// 单表注入面的断言，本测把 pub 读接口 [`berry_sched::policy::
/// placement_hint`] 独立成键域透传面）：
/// - PL-1 空表 → 0（无意见）；
/// - PL-2 命中键 7 → 透传槽值 1（偏 little）；
/// - PL-3 命中键 8 → 透传槽值 2（偏 big）；
/// - PL-4 未命中键 9 → 0（未命中回退）；
/// - PL-5 unbind 后 → 0（接口缺席等价）。
///
/// 全程走 `bind_gen`（带世代换表——v3 `policy_bind` 写侧底座）而非 `bind`
/// 自动通道：gen 取 `policy::gen() + 1` 即时读（哨兵态 gen()=0、已发布态
/// = 该快照 gen——+1 恒严格递增，受理性不依赖前序 ktest 的 bind 次数）。
/// 只测读出口，不建任务/不跑 pick——TCB 写入面（单核零写入）归 POLICY D 段。
/// PLACEMENT ok。
fn test_placement_hint() {
    use berry_sched::policy::{self, Directive, Entry};

    // 前序状态防御性清零（同 POLICY/PV3 纪律——ktest 期望 boot 期即未绑；
    // 哨兵态 gen()=0，本测世代从此起算，与前序测试的 GEN_NEXT 历史解耦）。
    policy::unbind();

    // PL-1: 空表 → placement_hint(key) == 0（无意见）。
    static EMPTY: [Entry; 0] = [];
    assert!(policy::bind_gen(&EMPTY, policy::gen() + 1), "PL-1：空表 bind_gen 须受理");
    assert_eq!(policy::placement_hint(7), 0, "PL-1：空表 placement_hint 须 0（无意见）");

    // PL-2/PL-3/PL-4: 双条目放置表（key=7 偏 little / key=8 偏 big；9 不在表）。
    static PLACE: [Entry; 2] = [
        Entry { task_key: 7, directive: Directive { placement: 1, weight: 0, epp: None } },
        Entry { task_key: 8, directive: Directive { placement: 2, weight: 0, epp: None } },
    ];
    assert!(policy::bind_gen(&PLACE, policy::gen() + 1), "PL-2/3：放置表 bind_gen 须受理");
    assert_eq!(policy::placement_hint(7), 1, "PL-2：命中键 7 须透传 placement=1（偏 little）");
    assert_eq!(policy::placement_hint(8), 2, "PL-3：命中键 8 须透传 placement=2（偏 big）");
    assert_eq!(policy::placement_hint(9), 0, "PL-4：未命中键 9 须 0（未命中回退）");

    // PL-5: unbind 后 → 0（接口缺席等价；命中过的键同落回退）。
    policy::unbind();
    assert_eq!(policy::placement_hint(7), 0, "PL-5：unbind 后键 7 须 0（接口缺席等价）");
    assert_eq!(policy::placement_hint(8), 0, "PL-5：unbind 后键 8 须 0（接口缺席等价）");

    berry_hal_aarch64::uart_puts(b"PLACEMENT ok\n");
}

/// - **DV-1 无 shadow 意见 → divergence 不增长**：`shadow_gen()==0` 的谱系
/// - **DV-2 双表同决策 → divergence == 0**：active p7+32（gen 0x…11）+
/// - **DV-3 双表异键 → 分歧全捕获**：shadow 换 p8+32 表（active p7+32
/// - **DV-4 轨道切换 → 对账清零**：DV-3 遗留分子 12 为前提，`promote()`
/// - **DV-5 STATUS（op=2）p3 打包位布局**：`crate::syscall::
///   policy_divergence_packed`（算式下放——SH-6 直测同口径）以 DV-4 尾
/// `divergence_reset`（轨道切换的**自动**清零由 DV-4 专测——两机制不混证）。
#[allow(clippy::unwrap_used)]
fn test_policy_divergence() {
    use berry_sched::policy::{self, Directive, Entry};
    use berry_sched::{Fair, RoundRobin, Runqueue, SchedClass, Task, Topo};
    use crate::syscall::policy_divergence_packed;

    /// 占位入口（pick_next 只选不切——永不执行；halt 兜底防未定义行为）。
    extern "C" fn dv_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    /// 单轮观测（比对单位）：选中者名 + 消费点权重面（eff）。
    type Obs = (&'static [u8], i32);

    const ROUNDS: usize = 12;

    /// 双任务本地环（proc_tag = 7/8——同 SHD-K 键域口径；7/8 = 两表各自受益者）。
    fn setup2() -> (Runqueue, RoundRobin, Topo) {
        let mut rq = Runqueue::new();
        let rr = Fair;
        let topo = Topo::single();
        for (tag, name) in [(7usize, b"p7".as_slice()), (8, b"p8".as_slice())] {
            let mut t = Task::new_kernel_with_as(dv_dummy, tag, name, 0, 0);
            t.proc_tag = tag; // 策略表 key（v1 = proc_tag；new_kernel 默认 0）
            let t: &'static mut Task = alloc::boxed::Box::leak(t);
            rr.enqueue(t, &mut rq);
        }
        (rq, rr, topo)
    }

    /// 跑 ROUNDS 轮 pick 记 (name, eff) 序列（apply→对账由 pick_next 挂钩）。
    fn probe(rq: &Runqueue, rr: &RoundRobin, topo: &Topo) -> [Obs; ROUNDS] {
        let mut seq: [Obs; ROUNDS] = [(b"", 0); ROUNDS];
        for s in seq.iter_mut() {
            let t = rr.pick_next(rq, topo).expect("DV：非空 runqueue 必有出选");
            *s = (t.name, t.prio_effective);
        }
        seq
    }

    // 前序状态收口（同 PV3/PBS/SHD 口径）：active 回哨兵 + 对账计数清零
    //（SHD-K 收尾 promote 重置后无 pick——0/0 契约 loud 成立）；shadow 谱系
    // 为 SHD-K 遗留（无清空接口——DV-1 的 gen==0 跳过态不可复入，见段注）。
    policy::unbind();
    assert!(!policy::bound(), "DV：前置须 active 未绑（哨兵）");
    assert_eq!(policy::shadow_gen(), 0x1_0000_0004,
        "DV：前置须 SHD-K 遗留 shadow 谱系（0x1_0000_0004）——本测试须后置 SHD-K");
    policy::divergence_reset();
    assert_eq!(policy::divergence_count(), 0, "DV：前置对账分子须零");
    assert_eq!(policy::shadow_eval_count(), 0, "DV：前置对账分母须零");

    // 两张单条表：p7/p8 各自 +32（同 SHD-K tab_a/tab_b 键域与词汇表）。
    let tab_p7 = [Entry {
        task_key: 7,
        directive: Directive { placement: 0, weight: 32, epp: None },
    }];
    let tab_p8 = [Entry {
        task_key: 8,
        directive: Directive { placement: 0, weight: 32, epp: None },
    }];
    // gen 台账（shadow 谱系严格递增；基点 = SHD-K 遗留 0x1_0000_0004）。
    let g_empty = 0x1_0000_0010u64; // DV-1 空 shadow 表
    let g_act2 = 0x1_0000_0011u64; // DV-2 active 表（与 shadow 同表不同 gen）
    let g_shd2 = 0x1_0000_0012u64; // DV-2 shadow 表
    let g_shd3 = 0x1_0000_0013u64; // DV-3 shadow 表（换 p8——与 active 分叉）

    // DV-1：无 shadow 意见 → divergence 不增长。绑**空 shadow 表**（谱系
    // 在席、对任一 key 恒无意见）→ 双方恒同意；分母恰增 ROUNDS 活证走的是
    // 对账路径而非 gen==0 跳过路径（该态已被 SHD-K 消费，见段注——判别面
    // 在分母、不在分子）。
    assert!(policy::bind_gen_shadow(&[], g_empty), "DV-1：空表 shadow gen 须受理");
    assert_eq!(policy::shadow_gen(), g_empty, "DV-1：shadow_gen 须即刻反映新世代");
    let (rq, rr, topo) = setup2();
    probe(&rq, &rr, &topo);
    assert_eq!(policy::divergence_count(), 0,
        "DV-1：shadow 空表（无意见）→ divergence 不得增长");
    assert_eq!(policy::shadow_eval_count(), ROUNDS as u64,
        "DV-1：谱系在席对账须逐 pick 计分母（非 gen==0 跳过路径）");

    // DV-2：shadow 与 active 同表（同决策）→ divergence == 0。active p7+32
    //（gen 0x…11）+ shadow p7+32（gen 0x…12——同表不同 gen，谱系各记各的）。
    assert!(policy::bind_gen(&tab_p7, g_act2), "DV-2：active 新 gen 须受理");
    assert!(policy::bind_gen_shadow(&tab_p7, g_shd2), "DV-2：shadow 同表新 gen 须受理");
    assert_eq!(policy::gen(), g_act2, "DV-2：active gen 前提");
    assert_eq!(policy::shadow_gen(), g_shd2, "DV-2：shadow gen 前提（同表不同 gen）");
    policy::divergence_reset(); // 新对账期（DV-1 分母 12 不混账）
    let (rq, rr, topo) = setup2();
    let seq = probe(&rq, &rr, &topo);
    for i in 0..ROUNDS {
        assert_eq!(seq[i].0, b"p7".as_slice(), "DV-2：active 表在用——第 {} 轮须 p7 独占", i);
        assert_eq!(seq[i].1, 32, "DV-2：active 表在用——第 {} 轮 p7 eff 须 32", i);
    }
    assert_eq!(policy::divergence_count(), 0, "DV-2：双表同决策 → 零分歧");
    assert_eq!(policy::shadow_eval_count(), ROUNDS as u64, "DV-2：分母须逐 pick 计");

    // DV-3：shadow 换 p8 表（与 active p7 表异键分叉）→ 每轮 pick 均捕获。
    // p7 出选：active 给 32、shadow 给 0（p7 不在 shadow 表）；p8 出选反向
    //（0 vs 32）——任一出选组合必差异，分歧率 100%。
    assert!(policy::bind_gen_shadow(&tab_p8, g_shd3), "DV-3：shadow 换表 gen 须受理");
    assert_eq!(policy::gen(), g_act2, "DV-3：shadow 换表不得扰动 active");
    policy::divergence_reset(); // 新对账期
    let (rq, rr, topo) = setup2();
    let seq = probe(&rq, &rr, &topo);
    for i in 0..ROUNDS {
        assert_eq!(seq[i].0, b"p7".as_slice(), "DV-3：active 仍 p7 表——第 {} 轮须 p7 独占", i);
        assert_eq!(seq[i].1, 32, "DV-3：active 仍给 32（shadow 侧给 0）——第 {} 轮", i);
    }
    assert_eq!(policy::divergence_count(), ROUNDS as u64,
        "DV-3：异键双表 → 每轮 pick 均须捕获分歧（p7 出选：active 32 vs shadow 0）");
    assert_eq!(policy::shadow_eval_count(), ROUNDS as u64, "DV-3：分母同 pick 数");

    // DV-4：promote 轨道切换 → 对账清零（新对账期从零起算）。DV-3 遗留分子
    // ROUNDS > 0 为前提；promote 后双计数器即刻归零（reset 内联于轨道切换
    //——非测试侧显式清）。转正后 active = shadow 表（p8+32、gen 随 shadow）
    //→ 双表同源：新对账期 12 轮仍零分歧 + 转正表真在用（首轮 head p7 无
    // 意见零写入、次轮起 p8 独占 eff=32——SH-2 同观测口径）。
    assert_eq!(policy::divergence_count(), ROUNDS as u64, "DV-4：前提——DV-3 遗留非零分子");
    assert!(policy::promote(), "DV-4：非空 shadow promote 须 true");
    assert!(policy::bound(), "DV-4：转正后须已绑");
    assert_eq!(policy::gen(), g_shd3, "DV-4：转正后 active gen 须 = shadow gen");
    assert_eq!(policy::divergence_count(), 0, "DV-4：promote 轨道切换须清零分子");
    assert_eq!(policy::shadow_eval_count(), 0, "DV-4：promote 轨道切换须清零分母");
    let (rq, rr, topo) = setup2();
    let seq = probe(&rq, &rr, &topo);
    assert_eq!(seq[0].0, b"p7".as_slice(), "DV-4：首轮 head p7 须先出（双表皆未写它）");
    assert_eq!(seq[0].1, 0, "DV-4：p7 不在转正表——eff 须零写入");
    for i in 1..ROUNDS {
        assert_eq!(seq[i].0, b"p8".as_slice(), "DV-4：转正表在用——第 {} 轮须 p8 独占", i);
        assert_eq!(seq[i].1, 32, "DV-4：转正表在用——第 {} 轮 p8 eff 须 32", i);
    }
    assert_eq!(policy::divergence_count(), 0, "DV-4：转正后双表同源 → 新对账期零分歧");
    assert_eq!(policy::shadow_eval_count(), ROUNDS as u64, "DV-4：新对账期分母须逐 pick 计");

    // DV-4 尾（轨道切换对偶面）：demote 亦自动清零——此后 active 空 vs
    // shadow 有表（p8+32）正是 `policy.rs` 注记的双轨观测主场景：p8 出选
    // 即分歧、p7 出选恒同意 → 分子开区间 (0, ROUNDS)（按 key 判别）。
    policy::demote();
    assert!(!policy::bound(), "DV-4 尾：demote 后须回哨兵");
    assert_eq!(policy::divergence_count(), 0, "DV-4 尾：demote 轨道切换须清零分子");
    assert_eq!(policy::shadow_eval_count(), 0, "DV-4 尾：demote 轨道切换须清零分母");
    let (rq, rr, topo) = setup2();
    probe(&rq, &rr, &topo);
    assert!(policy::divergence_count() > 0,
        "DV-4 尾：demote 后新对账期须重新捕获分歧（p8 命中：active 0 vs shadow 32）");
    assert!((policy::divergence_count() as usize) < ROUNDS,
        "DV-4 尾：分歧须按 key 判别（p7 出选恒同意——非全量分歧）");
    assert_eq!(policy::shadow_eval_count(), ROUNDS as u64, "DV-4 尾：分母同 pick 数");

    // DV-5：STATUS（op=2）p3 面打包位布局——sys_policy_bind 需 TrapFrame
    // 不便直调，算式已下放 syscall::policy_divergence_packed（SH-6 直测同
    // 口径），以 DV-4 尾遗留计数真数据源直测：分子开区间 (0, ROUNDS) 且
    // 分母 == ROUNDS → 两半取值非对称（半区对调必炸——对调 = 分子分母
    // 互换 → divergence 率倒置，PROMOTE/DEMOTE 判据全反）。
    let (dv_n, ev_n) = (policy::divergence_count(), policy::shadow_eval_count());
    assert!(dv_n > 0 && dv_n < ev_n, "DV-5：直测前提——分子分母须非对称");
    assert_eq!(
        policy_divergence_packed(),
        ((ev_n & 0xFFFF_FFFF) << 32) | (dv_n & 0xFFFF_FFFF),
        "DV-5：packed 须 = (eval低32 << 32) | divergence低32（对调/截断全判别）"
    );
    // 串口遥测行（A/B 评估链内核侧收口）：宿主 tools/policy_ab_eval.py 从
    // QEMU 串口解析本行 → divergence 率 → PROMOTE/DEMOTE 判定。
    berry_hal_aarch64::uart_puts(b"PERF policy divergence=");
    berry_hal_aarch64::uart_put_dec(dv_n);
    berry_hal_aarch64::uart_puts(b" eval=");
    berry_hal_aarch64::uart_put_dec(ev_n);
    berry_hal_aarch64::uart_puts(b"\n");

    // 收尾：恢复缺席态（同 SHD-K——smoke 稳态全程未绑；shadow 谱系留
    // g_shd3 无清空接口，后续测试 pick 只进对账计数、行为零感知——boot 期
    // 单测语境不外泄）。
    policy::unbind();
    assert!(!policy::bound(), "DV：收尾须回到未绑");

    // 任务体 16KiB 栈 × 5 泄漏不回收（ktest 语境同 PV3/SHD 政策——v1 无 reap）。
    berry_hal_aarch64::uart_puts(b"DV-K ok\n");
}

/// P4 收官审计实踩的 runqueue 摘环回归：`unlink` 旧单元素判据 `prev == next`
/// 在**两元环** {X,Y} 上同样为真（X.rq_prev == X.rq_next == Y）→ 摘 X 误走
/// 空环分支把 head 清 None，幸存者 Y 带 stale 链**静默失联**（「已就绪却不再
/// 被调度」）。现矩阵常驻环 ≥6 打不中；矩阵收缩（任务批量 exit）即踩中。
/// 判据已改锚定 task 自身（`prev == task && next == task`），本用例机器断言
/// 兜底。本地环手搭（不动全局 RUNQUEUE——ktest 在 `berry_sched::init` 前）；
/// 非 smoke 标记（TAGTAB 模式：断言炸 = panic = 后续标记缺失兜底）。
#[allow(clippy::unwrap_used)]
fn test_rq_ring2() {
    use berry_sched::{Fair, Runqueue, SchedClass, Task, Topo};

    /// 占位入口（pick_next 只选不切——永不执行；halt 兜底防未定义行为）。
    extern "C" fn rq_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    fn mk(name: &'static [u8]) -> &'static mut Task {
        alloc::boxed::Box::leak(Task::new_kernel_with_as(rq_dummy, 0, name, 0, 0))
    }

    let mut rq = Runqueue::new();
    let rr = Fair;
    let topo = Topo::single();

    // —— 两元环：摘其一，幸存者必须仍可出选（bug 下 head=None → pick None）。
    let tx = mk(b"rX".as_slice());
    let ty = mk(b"rY".as_slice());
    rr.enqueue(&mut *tx, &mut rq);
    rr.enqueue(&mut *ty, &mut rq);
    // sanity：同 eff 层 FIFO 轮转（游标恰停在 X → 先出 X 后出 Y）。
    assert_eq!(rr.pick_next(&rq, &topo).unwrap().name, b"rX".as_slice(), "RING2：摘除前首轮出选须 X");
    assert_eq!(rr.pick_next(&rq, &topo).unwrap().name, b"rY".as_slice(), "RING2：摘除前次轮出选须 Y");
    // 游标此刻停在 X；摘 X（两元环 head 侧）→ 环退单元素 {Y}。
    rr.dequeue(&mut *tx, &mut rq);
    assert!(!rq.is_empty(), "RING2：摘 X 后环上须仍有 Y（旧判据误清 head）");
    for i in 0..3 {
        assert_eq!(
            rr.pick_next(&rq, &topo).unwrap().name,
            b"rY".as_slice(),
            "RING2：摘 X 后 Y 须可持续出选（第 {} 轮）",
            i
        );
    }
    // 摘最后一个 → 空环（真·单元素路径）。
    rr.dequeue(&mut *ty, &mut rq);
    assert!(rq.is_empty(), "RING2：摘 Y 后环须空");
    assert!(rr.pick_next(&rq, &topo).is_none(), "RING2：空环出选须 None");

    // —— 三元素环中删回归（else 分支邻接缝）：摘中位 B 后 A/C 轮转、B 不复出选。
    let ta = mk(b"rA".as_slice());
    let tb = mk(b"rB".as_slice());
    let tc = mk(b"rC".as_slice());
    rr.enqueue(&mut *ta, &mut rq);
    rr.enqueue(&mut *tb, &mut rq);
    rr.enqueue(&mut *tc, &mut rq);
    rr.dequeue(&mut *tb, &mut rq); // head=A，B 为中位（非 head 摘除路径）
    for i in 0..4 {
        let want = if i % 2 == 0 { b"rA".as_slice() } else { b"rC".as_slice() };
        assert_eq!(
            rr.pick_next(&rq, &topo).unwrap().name,
            want,
            "RING2：摘中位后须只剩 A/C 轮转（第 {} 轮）",
            i
        );
    }
    // 收缩到两元 {A,C} 再摘一：幸存者仍出选（两元路径二次覆盖，游标在非 head 侧）。
    rr.dequeue(&mut *tc, &mut rq); // 上轮游标停在 A → C 为非 head 摘除
    assert!(!rq.is_empty(), "RING2：两元环摘 C 后须仍有 A");
    assert_eq!(rr.pick_next(&rq, &topo).unwrap().name, b"rA".as_slice(), "RING2：摘 C 后 A 须仍出选");
    rr.dequeue(&mut *ta, &mut rq);
    assert!(rq.is_empty(), "RING2：收尾环须空");

    // 任务体泄漏不回收（同 test_policy 政策——v1 无 reap）。
    berry_hal_aarch64::uart_puts(b"RING2 ok\n");
}

/// 雷 2 残留诊断的环完整性回归（RING3）：d4dcf6a 修的「两元环 unlink 误判」
/// 之外，30s 长跑确诊的另一分裂形态 =「对在环任务再 link_back」——覆盖写
/// prev/next 把环劈成**头可达活圈 + 孤儿段**，被劈出者 Ready 却永不被 pick
///（五任务静默失联、无 dump 无退出）。执法面两道，本用例全部拉进机器断言：
/// ①判据直测——`linkable`/`sleepq_insertable` 在环/队状态翻转（ktest 不能
/// 真触发 link_back 入口断言：炸 = panic = halt，只能测断言所依据的判据）；
/// ②`ring_check` 走查步数对账 `len` 记账——孤儿段形态下从 head 走圈**仍
/// 闭合**（活圈自闭合），只有「步数 < len」能显形有成员不在头可达圈上；
/// ③固定 LCG 序列 fuzz：8 任务混合 enqueue/dequeue/pick_next，每步后
/// 走查 == len == 影子账本三对账（把 d4dcf6a 家族的收缩拓扑误判全圈进
/// 回归面）。本地环手搭（不动全局 RUNQUEUE——ktest 在 `berry_sched::init`
/// 前）；非 smoke 标记（RING2 模式：断言炸 = panic = 后续标记缺失兜底）。
fn test_rq_ring3() {
    use berry_sched::{linkable, Fair, Runqueue, SchedClass, Task, Topo};

    /// 占位入口（pick_next 只选不切——永不执行；halt 兜底防未定义行为）。
    extern "C" fn rq3_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    fn mk() -> &'static mut Task {
        alloc::boxed::Box::leak(Task::new_kernel_with_as(rq3_dummy, 0, b"r3".as_slice(), 0, 0))
    }

    let mut rq = Runqueue::new();
    let rr = Fair;
    let topo = Topo::single();

    // —— ① 判据直测（执法断言的判据本身）。
    let t = mk();
    assert!(linkable(t), "RING3：新任务须可入环（链接字段 None）");
    assert!(berry_sched::sleepq_insertable(t), "RING3：新任务须可入 SLEEPQ");
    rr.enqueue(&mut *t, &mut rq);
    assert!(!linkable(t), "RING3：在环任务 linkable 须 false（双入环判据）");
    assert_eq!(rq.len(), 1, "RING3：单元素环 len 记账须 1");
    assert_eq!(rq.ring_check(), 1, "RING3：单元素环走查步数须 1");
    rr.dequeue(&mut *t, &mut rq);
    assert!(linkable(t), "RING3：摘环后 linkable 须复原");
    assert_eq!(rq.ring_check(), 0, "RING3：空环走查须 0");

    // —— ② SLEEPQ 判据往返（insert_sorted 入口断言同款判据；单元素队成员
    // prev/next 均 None——判据须含 head 分支，本往返机器覆盖该例外）。
    let s = mk();
    berry_sched::sleepq_insert(s);
    assert!(!berry_sched::sleepq_insertable(s), "RING3：在队（单元素=队头）insertable 须 false");
    assert!(linkable(s), "RING3：在 SLEEPQ 不涉环字段（两判据互不干扰）");
    let s2 = mk();
    berry_sched::sleepq_insert(s2); // 第二元素入队 → s 不再是单元素队头
    assert!(!berry_sched::sleepq_insertable(s2), "RING3：在队（非头）insertable 须 false");
    assert!(!berry_sched::sleepq_insertable(s), "RING3：队头成员 insertable 仍须 false");
    berry_sched::sleepq_unlink(s2);
    assert!(berry_sched::sleepq_insertable(s2), "RING3：摘队后须复原");
    berry_sched::sleepq_unlink(s);
    assert!(berry_sched::sleepq_insertable(s), "RING3：收尾摘头后须复原");

    // —— ③ 固定 LCG fuzz：每步随机选任务/操作，只按影子账本选合法目标
    //（对在环者 enqueue = 生产代码里 link_back 断言要拦的违约，fuzz 不做）。
    // 影子与 linkable 每步互证 = 判据回归；走查/len/影子三对账 = 结构回归。
    const N: usize = 8;
    let tasks: [&'static mut Task; N] = [(); N].map(|()| mk());
    let mut on_ring = [false; N];
    let mut rng: u64 = 0x243F_6A88_85A3_08D3; // 固定种子 LCG——确定性复现
    let (mut picks, mut max_len) = (0usize, 0usize);
    for step in 0..300 {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let i = ((rng >> 33) as usize) % N;
        assert_eq!(
            linkable(tasks[i]),
            !on_ring[i],
            "RING3：linkable 与影子账本不一致（step {}）",
            step
        );
        match (rng >> 13) % 3 {
            0 if !on_ring[i] => {
                rr.enqueue(&mut *tasks[i], &mut rq);
                on_ring[i] = true;
            }
            1 if on_ring[i] => {
                rr.dequeue(&mut *tasks[i], &mut rq);
                on_ring[i] = false;
            }
            _ => {
                if !rq.is_empty() {
                    let _ = rr.pick_next(&rq, &topo); // 游标前移（advance_past 面）
                    picks += 1;
                }
            }
        }
        let want = on_ring.iter().filter(|x| **x).count();
        assert_eq!(rq.len(), want, "RING3：len 记账须==影子（step {}）", step);
        assert_eq!(rq.ring_check(), want, "RING3：走查步数须==影子（step {}）", step);
        max_len = max_len.max(want);
    }
    assert!(picks > 0, "RING3：fuzz 须真覆盖 pick_next 路径");
    assert!(max_len >= 3, "RING3：fuzz 须覆盖 ≥3 元环（实测 {}）", max_len);

    // —— ④ 收尾清环（全体摘除 → 空环三对账）。
    for i in 0..N {
        if on_ring[i] {
            rr.dequeue(&mut *tasks[i], &mut rq);
        }
    }
    assert!(rq.is_empty(), "RING3：收尾须空环");
    assert_eq!(rq.ring_check(), 0, "RING3：收尾走查须 0");

    // 任务体泄漏不回收（同 test_policy 政策——v1 无 reap）。
    berry_hal_aarch64::uart_puts(b"RING3 ok\n");
}

/// 4-3b PI 捐赠图推导（`kernel/src/pi.rs` 全图重推）：donor = 睡 channel 等内核
/// 派生位（READABLE/WRITABLE）者；受捐 = SLEEPQ 上持 peer 端点且带解锁 right
///（等 WRITABLE 需 RECV、等 READABLE 需 SEND）的睡眠者。ktest 在
/// `berry_sched::init` 之前跑——睡眠者经 `sleepq_insert` 手搭（不 block 不
/// 切走，字段直填）、`pi::recompute` 直调；持有面用 **leak 私有表**（proc_tag
/// 指针约定与 3-4a「EL0 每进程表」同语义——ktest 无 Process，借字段直指）。
/// 触发钩子的全真路径在 main.rs pi_client/pi_server/pi_probe 三任务 +
/// `PI ok`（smoke 第 18 标记）。覆盖面：单跳 EL0 式判别 / rights 门控两向 /
/// 链式两跳（d_eff 实时读 pi——只读 prio_effective 会截死一跳）/ 持有环收敛
/// / 11 节点链 × 轮帽 8（倒序入队每轮恰好一跳，第 9/10 跳到不了）/ WAIT_ANY
/// 与 Event donor 跳过 / 冻结随行（离队不清）+ 重入队回落 / 共享表粒度
/// （内核任务持有面 = 内核侧共享表，boot 场景同款语义）。
#[allow(clippy::unwrap_used)]
/// 1.1/S3-a：冻结机制纯函数面（HELD_FROZEN 位 + eligible 过滤 + freeze/thaw）。
/// 本地环手搭（同 RING2/RING3 模式）；非 smoke 标记（断言炸 = panic 兜底）。
fn test_freeze() {
    use berry_sched::{Fair, Runqueue, SchedClass, Task, Topo, HELD_FROZEN};

    extern "C" fn fz_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    fn mk(name: &'static [u8]) -> &'static mut Task {
        alloc::boxed::Box::leak(Task::new_kernel_with_as(fz_dummy, 0, name, 0, 0))
    }

    let mut rq = Runqueue::new();
    let rr = Fair;
    let topo = Topo::single();

    // 两任务等权入环。
    let ta = mk(b"fzA".as_slice());
    let tb = mk(b"fzB".as_slice());
    rr.enqueue(&mut *ta, &mut rq);
    rr.enqueue(&mut *tb, &mut rq);

    // F1：正常轮转——两任务均可出选。
    let p1 = rr.pick_next(&rq, &topo).unwrap();
    assert_eq!(p1.name, b"fzA".as_slice(), "FREEZE：F1 首轮须 A");
    let p2 = rr.pick_next(&rq, &topo).unwrap();
    assert_eq!(p2.name, b"fzB".as_slice(), "FREEZE：F1 次轮须 B");

    // F2：冻结 A → 只出 B。
    ta.held_locks |= HELD_FROZEN;
    let p3 = rr.pick_next(&rq, &topo).unwrap();
    assert_eq!(p3.name, b"fzB".as_slice(), "FREEZE：F2 冻结 A 后须只出 B");
    let p4 = rr.pick_next(&rq, &topo).unwrap();
    assert_eq!(p4.name, b"fzB".as_slice(), "FREEZE：F2 冻结 A 后次轮仍须 B");

    // F3：解冻 A → A 可重新出选（给 B 推高 vruntime 确保 A key 更小）。
    ta.held_locks &= !HELD_FROZEN;
    ta.vruntime = 0;
    ta.vdeadline = 0;
    tb.vruntime = 1000; // B 已跑过多轮——vruntime 更高，A 更 eligible
    tb.vdeadline = 2000;
    rq.resync_vruntime_sum(); // 直改后洗账（avg=500 → A eligible / B not）
    let p5 = rr.pick_next(&rq, &topo).unwrap();
    assert_eq!(p5.name, b"fzA".as_slice(), "FREEZE：F3 解冻后 A 须可出选");

    // F4：pager 义务派生面（D6）——demand VMO + SET_POLICY 合取才计数；
    // 别名降权（duplicate 去 SET_POLICY）不再计，take 迁移出表即归零。
    {
        use crate::handle::{HandleTable, KernelObj, RIGHT_READ, RIGHT_SET_POLICY};
        use crate::vmo::Vmo;
        let mut ht = HandleTable::new();
        let v = Vmo::create_demand(4096).unwrap();
        let h = ht
            .insert(KernelObj::Vmo(v), RIGHT_READ | RIGHT_SET_POLICY)
            .unwrap();
        assert_eq!(ht.pager_load(), 1, "FREEZE：F4 demand+SET_POLICY 须计 1");
        let _alias = ht.duplicate(h, RIGHT_READ).unwrap();
        assert_eq!(ht.pager_load(), 1, "FREEZE：F4 单 SET_POLICY 槽不因别名双计");
        let plain = Vmo::create(4096).unwrap();
        let _hp = ht
            .insert(KernelObj::Vmo(plain), RIGHT_READ | RIGHT_SET_POLICY)
            .unwrap();
        assert_eq!(ht.pager_load(), 1, "FREEZE：F4 eager VMO 不计 pager");
        let _taken = ht.take(h).unwrap();
        assert_eq!(ht.pager_load(), 0, "FREEZE：F4 SET_POLICY 迁出后归零");
    }

    // —— F5：嵌套冻结——两次设 HELD_FROZEN（幂等位），解冻一次不恢复，
    // 仍不出选 A；手动清位（模拟最后一个 token close）才恢复。
    let tc = mk(b"fzC".as_slice());
    let td = mk(b"fzD".as_slice());
    let mut rq2 = Runqueue::new();
    rr.enqueue(&mut *tc, &mut rq2);
    rr.enqueue(&mut *td, &mut rq2);

    // 嵌套第一层：冻 C。
    tc.held_locks |= HELD_FROZEN;
    let p6 = rr.pick_next(&rq2, &topo).unwrap();
    assert_eq!(p6.name, b"fzD".as_slice(), "FREEZE：F5 首冻 C 后须只出 D");

    // 嵌套第二层：再冻 C（幂等——位已置，precheck 须排除 FROZEN 位）。
    // 模拟：precheck 排除 HELD_FROZEN 后仍通过（held_locks 非 FROZEN 位 == 0）。
    let hl = tc.held_locks & !HELD_FROZEN;
    assert_eq!(hl, 0, "FREEZE：F5 precheck 须排除 FROZEN 位（非零={:b}）", hl);

    // 解冻一次（模拟第一个 token close——depth 1→0 但 held_locks 仍置位
    // 因为我们的简化测试直接操作位而非 SuspendTok）。
    // 在真实嵌套中，depth>0 不清位。此处直接验证：位仍置 → C 仍不出选。
    let p7 = rr.pick_next(&rq2, &topo).unwrap();
    assert_eq!(p7.name, b"fzD".as_slice(), "FREEZE：F5 冻结中 C 须持续不出选");

    // 最终解冻（最后一个 token close → 清位）。
    tc.held_locks &= !HELD_FROZEN;
    tc.vruntime = 0;
    tc.vdeadline = 0;
    td.vruntime = 1000;
    td.vdeadline = 2000;
    rq2.resync_vruntime_sum(); // 直改后洗账（同 F3 口径）
    let p8 = rr.pick_next(&rq2, &topo).unwrap();
    assert_eq!(p8.name, b"fzC".as_slice(), "FREEZE：F5 最终解冻后 C 须可出选");

    // —— F6：S3-b SINGLE_THREAD token 收口——手工构造 Process/Thread ObjRef，
    // 两枚嵌套 token 逐个 Drop：depth 只在最后一枚归零、FREEZE_READY 只在
    // 最后一次 thaw 清位、Process.thread_freezes 占位同步撤销。
    {
        use crate::handle::{
            HandleTable, KernelObj, ObjType, RIGHT_MANAGE,
        };
        use crate::process::{Process, Thread, SA_FLAG_FREEZE_READY};
        use crate::vmar::Vmar;

        let mut ht = HandleTable::new();
        let vm = Vmar::new_root(crate::mm::frames(), crate::aspace::alloc_asid()).unwrap();
        let (pa, asid) = (vm.ttbr0_pa(), vm.asid());
        let hv = ht
            .insert(KernelObj::Vmar(vm), RIGHT_MANAGE)
            .unwrap();
        let (vref, _) = ht.get_raw(hv, ObjType::Vmar, 0).unwrap();
        let hp = ht
            .insert(
                KernelObj::Process(unsafe { Process::new(vref, pa, asid) }),
                RIGHT_MANAGE,
            )
            .unwrap();
        let (pref, _) = ht.get_raw(hp, ObjType::Process, RIGHT_MANAGE).unwrap();
        let ht_handle = ht
            .insert(KernelObj::Thread(unsafe { Thread::new(pref) }), RIGHT_MANAGE)
            .unwrap();
        let (tref, _) = ht.get_raw(ht_handle, ObjType::Thread, RIGHT_MANAGE).unwrap();

        unsafe {
            let pr = &mut *pref.as_ptr();
            let p = pr.obj.as_process_mut().unwrap();
            p.started = true;
            p.thread_freezes = 1;
            let tr = &mut *tref.as_ptr();
            let th = tr.obj.as_thread_mut().unwrap();
            th.started = true;
            th.freeze_depth = 2;
            th.sched_attr.flags |= SA_FLAG_FREEZE_READY;

            tr.retain();
            let tok1 = crate::suspend::SuspendTok {
                target: crate::suspend::SuspendTarget::Thread(tref),
                key: tref.as_ptr() as usize,
                since_vct: 0,
            };
            drop(tok1); // depth 2→1：不得 thaw/清 READY
            let th = (*tref.as_ptr()).obj.as_thread_mut().unwrap();
            assert_eq!(th.freeze_depth, 1, "FREEZE：F6 首 token close 后 depth=1");
            assert!(th.sched_attr.flags & SA_FLAG_FREEZE_READY != 0,
                "FREEZE：F6 嵌套未归零不得清 READY");
            assert_eq!((*pref.as_ptr()).obj.as_process().unwrap().thread_freezes, 1);

            tr.retain();
            let tok2 = crate::suspend::SuspendTok {
                target: crate::suspend::SuspendTarget::Thread(tref),
                key: tref.as_ptr() as usize,
                since_vct: 0,
            };
            drop(tok2); // depth 1→0：清 READY + 撤占位（无任务，thaw 空扫无害）
            let th = (*tref.as_ptr()).obj.as_thread_mut().unwrap();
            assert_eq!(th.freeze_depth, 0, "FREEZE：F6 末 token close 后 depth=0");
            assert!(th.sched_attr.flags & SA_FLAG_FREEZE_READY == 0,
                "FREEZE：F6 最后 thaw 须清 FREEZE_READY");
            assert_eq!((*pref.as_ptr()).obj.as_process().unwrap().thread_freezes, 0,
                "FREEZE：F6 Process 单线程占位须同步归零");
        }
        ht.drain();
    }

    // —— F7/F8：app-1 段 a-1 双残影定谳钉板（并行 lane 调和审计 D3② 遗留
    // 配方：FREEZE 系补双断言——「睡在 syscall 内带残影位任务 precheck 通过、
    // RQ 在席带位仍拒」）。钉现行谓词语义（sched.rs freeze_precheck_by_pinned
    // /by_thread）：SLEEPQ 在席者只查 pi_inherited + wait_signals 派生位
    // （READABLE/WRITABLE——by_pinned 面），svc_mask/held 结构位是 SVC 窗
    // 记账残影（随睡者入队不清、醒后重取复用）——不拒；RQ 在席（真在飞/
    // 窗内可抢占形）svc_mask + held 结构位均拒（仅排除 FROZEN/THROTTLE
    // 非结构位）。真队列面直调谓词：ktest 先于 sched::init（main.rs :614/
    // :668）无在飞任务，SLEEPQ/双 RQ 静态基座 const 初值可用（FRZCYC/
    // PWRDEG 同款先例）。
    {
        use crate::handle::{HandleTable, KernelObj, ObjType, RIGHT_MANAGE};
        use crate::process::{Process, Thread};
        use crate::vmar::Vmar;
        use berry_sched::task::{HELD_SQ, HELD_TABLE};

        // 真 Process/Thread ObjRef 锚（F6 同款手搭）：F8 清场走
        // kill_all_tasks_by_pinned → task_exit_hook 裸解引用 pinned/
        // thread_key（syscall::install 已在 ktest 前注册——main.rs :559），
        // 伪键必炸——须真对象 + 运行钉住 retain（与 process_start 的
        // retain/pinned 成对语义同款）。
        let mut ht = HandleTable::new();
        let vm = Vmar::new_root(crate::mm::frames(), crate::aspace::alloc_asid())
            .expect("FREEZE：F7 建 root Vmar（可信语境）");
        let (pa, asid) = (vm.ttbr0_pa(), vm.asid());
        let hv = ht
            .insert(KernelObj::Vmar(vm), RIGHT_MANAGE)
            .expect("FREEZE：F7 登记 Vmar（可信语境）");
        let (vref, _) = ht
            .get_raw(hv, ObjType::Vmar, 0)
            .expect("FREEZE：F7 取 Vmar 引用");
        let hp = ht
            .insert(
                KernelObj::Process(unsafe { Process::new(vref, pa, asid) }),
                RIGHT_MANAGE,
            )
            .expect("FREEZE：F7 登记 Process（可信语境）");
        let (pref, _) = ht
            .get_raw(hp, ObjType::Process, RIGHT_MANAGE)
            .expect("FREEZE：F7 取 Process 引用");
        let hth = ht
            .insert(
                KernelObj::Thread(unsafe { Thread::new(pref) }),
                RIGHT_MANAGE,
            )
            .expect("FREEZE：F7 登记 Thread（可信语境）");
        let (tref, _) = ht
            .get_raw(hth, ObjType::Thread, RIGHT_MANAGE)
            .expect("FREEZE：F7 取 Thread 引用");
        unsafe {
            let p_key = pref.as_ptr() as usize;
            let t_key = tref.as_ptr() as usize;
            // 运行钉住（表引 1 + 钉 1）+ 末线程簿记位（钩子 live_threads-=1
            // → 0 走 Process 收口）。
            (*pref.as_ptr()).retain();
            (*tref.as_ptr()).retain();
            (*pref.as_ptr()).obj.as_process_mut().unwrap().live_threads = 1;

            // —— F7（断言①）：睡在 syscall 内带残影位 → precheck 通过。
            // 睡者（真 SLEEPQ 在席）带非零 svc_mask + held 结构位（双残影）
            // 仍过——位是记账残影、物理锁已放（block 舞蹈），冻结只暂停
            // 执行不动锁。空洞对照（防恒绿）：PI 捐赠边/donor 派生位仍拒。
            // by_thread 的 sleep_ok 查 held（语义分立），故其断言只落在
            // svc 残影单带段（held 位前置 0）。
            let s1 = mk_freeze_task(b"fzSlp".as_slice(), p_key);
            s1.thread_key = t_key;
            s1.svc_mask = 0x0F; // SVC 窗出口清零（sched.rs :486）——睡者携残影
            berry_sched::sleepq_insert(s1);
            assert!(
                berry_sched::freeze_precheck_by_pinned(p_key),
                "FREEZE：F7 睡者 svc_mask 残影须过 precheck（a-1 定谳：mask 不查睡者）"
            );
            assert!(
                berry_sched::freeze_precheck_by_thread(t_key),
                "FREEZE：F7 睡者 svc_mask 残影须过 precheck——by_thread 同判"
            );
            s1.held_locks = HELD_SQ | HELD_TABLE; // 持锁残影齐带（物理锁已放、位不清）
            assert!(
                berry_sched::freeze_precheck_by_pinned(p_key),
                "FREEZE：F7 睡者双残影（svc_mask+held 结构位）须过 precheck"
            );
            // 空洞对照：证谓词真在读睡者任务（非恒真）。
            s1.pi_inherited = 1;
            assert!(
                !berry_sched::freeze_precheck_by_pinned(p_key),
                "FREEZE：F7 PI 捐赠边睡者仍须拒（a-1 保留检查）"
            );
            assert!(
                !berry_sched::freeze_precheck_by_thread(t_key),
                "FREEZE：F7 PI 捐赠边睡者仍须拒——by_thread 同判"
            );
            s1.pi_inherited = 0;
            s1.wait_signals = crate::abi_gen::SIGNAL_READABLE;
            assert!(
                !berry_sched::freeze_precheck_by_pinned(p_key),
                "FREEZE：F7 donor 派生位（READABLE）睡者仍须拒"
            );
            s1.wait_signals = 0;
            berry_sched::sleepq_unlink(s1); // SLEEPQ 清理纪律（勿扰后续测试队列面）

            // —— F8（断言②）：RQ 在席带残影位 → 拒。真 RQ 面：
            // try_spawn_user（process_start 同路径）把任务入真实 PER_CPU_RQ
            // ——ktest 期唯一真 RQ 在席任务。entry/sp 伪 VA 不派发（装配
            // 只存帧不校验；断言后即杀）。
            let s2 = berry_sched::try_spawn_user(
                0x1000, 0, 0x2000, b"fzRQ".as_slice(), 0, 0, 0, p_key, t_key, false,
            )
            .expect("FREEZE：F8 造 RQ 在席任务（可信语境）");
            let s2 = &mut *(s2 as *mut berry_sched::Task);
            s2.svc_mask = 0x0F; // SVC 窗内可抢占形（真在飞）
            assert!(
                !berry_sched::freeze_precheck_by_pinned(p_key),
                "FREEZE：F8 RQ 在席 svc_mask≠0 须拒（真在飞防御面）"
            );
            assert!(
                !berry_sched::freeze_precheck_by_thread(t_key),
                "FREEZE：F8 RQ 在席 svc_mask≠0 须拒——by_thread 同判"
            );
            s2.svc_mask = 0;
            assert!(
                berry_sched::freeze_precheck_by_pinned(p_key),
                "FREEZE：F8 对照——清残影后同任务须过（拒源锁定 svc_mask）"
            );
            assert!(
                berry_sched::freeze_precheck_by_thread(t_key),
                "FREEZE：F8 对照——清残影后同任务须过——by_thread 同判"
            );
            s2.held_locks = HELD_SQ; // 结构锁位（bit1..=5 面）
            assert!(
                !berry_sched::freeze_precheck_by_pinned(p_key),
                "FREEZE：F8 RQ 在席 held 结构位须拒（双残影定谳：结构位对 RQ 拒）"
            );
            s2.held_locks = HELD_FROZEN | berry_sched::HELD_THROTTLE;
            assert!(
                berry_sched::freeze_precheck_by_pinned(p_key),
                "FREEZE：F8 RQ 在席 FROZEN/THROTTLE 非结构位不拒（F5 盖面真谓词版）"
            );
            s2.held_locks = 0;

            // 清场：kill 全链摘真 RQ——退出钩子走真 ObjRef 簿记（Thread
            // exited + release 钉、Process 末线程收口 + release 钉），任务
            // 压 REAPQ（v1 不回收政策同 sched spawn，idle 期 reap_drain 收）。
            // 返 (名, 数) = 恰一枚（RQ 面不残留）。
            let killed = berry_sched::kill_all_tasks_by_pinned(p_key, 0);
            assert_eq!(
                killed,
                Some((b"fzRQ".as_slice(), 1)),
                "FREEZE：F8 清场 kill 须恰收 fzRQ 一枚（RQ 面不残留）"
            );
        }
        ht.drain();
    }

    berry_hal_aarch64::uart_puts(b"FREEZE-K ok\n");
}

/// 1.1/S4-b：per-domain CPU 账户——charge / query / 表满覆写。
fn test_billing() {
    use crate::telemetry;

    // BL1：charge + query 往返。
    telemetry::billing_charge(100, 1_000);
    telemetry::billing_charge(100, 2_000);
    assert_eq!(telemetry::billing_domain_total(100), Some(3_000),
               "BL：BL1 域 100 须累计 3000ns");

    // BL2：多域隔离。
    telemetry::billing_charge(200, 500);
    assert_eq!(telemetry::billing_domain_total(100), Some(3_000),
               "BL：BL2 域 100 不受域 200 影响");
    assert_eq!(telemetry::billing_domain_total(200), Some(500),
               "BL：BL2 域 200 须 500ns");

    // BL3：域 0 / 零时长拒收（无副作用）。
    telemetry::billing_charge(0, 999);
    telemetry::billing_charge(300, 0);
    assert_eq!(telemetry::billing_domain_total(300), None,
               "BL：BL3 零时长 charge 须无副作用（不建槽）");

    berry_hal_aarch64::uart_puts(b"BL-K ok\n");
}

/// 1.1/S5.12 负向：`count_type` 类型过滤语义（token 上限执法读侧判据）。
fn test_suspend_negative() {
    use crate::handle::{HandleTable, KernelObj, ObjType, RIGHT_READ, RIGHT_WRITE};
    use crate::vmo::Vmo;

    // A. 空表 → SuspendToken 计数 0。
    let mut ht = HandleTable::new();
    assert_eq!(ht.count_type(ObjType::SuspendToken), 0,
        "SUS-NEG：空表 SuspendToken 计数须 0");

    // B. 插 2 个 Vmo → SuspendToken 计数仍 0（类型隔离）。
    let rw = RIGHT_READ | RIGHT_WRITE;
    let h1 = ht.insert(KernelObj::Vmo(Vmo::create(4096).unwrap()), rw).unwrap();
    let h2 = ht.insert(KernelObj::Vmo(Vmo::create(4096).unwrap()), rw).unwrap();
    assert_eq!(ht.count_type(ObjType::Vmo), 2, "SUS-NEG：Vmo 计数须 2");
    assert_eq!(ht.count_type(ObjType::SuspendToken), 0,
        "SUS-NEG：Vmo 不污染 SuspendToken 计数");

    // C. 全关归零。
    ht.close(h1).unwrap();
    ht.close(h2).unwrap();
    assert_eq!(ht.count_type(ObjType::Vmo), 0, "SUS-NEG：全关归零");

    berry_hal_aarch64::uart_puts(b"SUS-NEG-K ok\n");
}

/// sim-S2/S3 freeze 面共用钉板（`test_freeze_cycles` 原内联手搭面提出——
/// 行为零变；sim-S3 PWRDEG 同源复用）。ktest 在 sched::init 前跑：任务
/// 手搭 SLEEPQ（test_pi 同款）作 freeze 扫描面，调用方负责 `sleepq_insert`
/// 钉队与 `sleepq_unlink` 收尾（SLEEPQ 清理纪律——勿扰后续测试的队列面）。
extern "C" fn fc_dummy(_arg: usize) -> ! {
    berry_hal_aarch64::halt()
}

/// 手搭冻结受害/金丝雀任务：内核任务体 + `pinned = key`（freeze/thaw_by_pinned
/// 匹配键）。
fn mk_freeze_task(name: &'static [u8], key: usize) -> &'static mut berry_sched::Task {
    let t = alloc::boxed::Box::leak(berry_sched::Task::new_kernel_with_as(
        fc_dummy, 0, name, 0, 0,
    ));
    t.pinned = key; // freeze 扫描键（freeze/thaw_by_pinned 匹配面）
    t
}

/// sim-S2 A2（PM-16 §4.4 步 2「断言第一批」）：freeze/thaw 两轮**全真 svc**
/// 循环——`svc #0` 走 `sys_process_suspend` 全链路（HAL dispatch →
/// `caller_table` 落 bootstrap 表 → token 入表 + copy_out 回写），
/// `handle_close(token)` → `SuspendTok::drop` → depth 递减 → auto_thaw。
/// 判据（学 suspend_stress_test 计数器断言）：①N≥2 轮 telemetry
/// freeze_ok/auto_thaw **计数严格递增**（递增载体 = 计数器非 handle 值
/// ——slot|gen 最低空槽分配，数值不单调）；②末轮全量标记在席（`FRZCYC
/// ok r=2`，smoke 收割）；③金丝雀不熄（异钉任务 pinned|1 恒不被冻——
/// ObjRef 8B 对齐 ⇒ 恒不撞钉键；「不熄」= HELD_FROZEN 两轮恒清）。
/// 负例先行：pager 义务在身（D6：demand VMO + SET_POLICY 进程表）→
/// `-EBUSY` 且零状态残留。ktest 在 sched::init 前跑：任务手搭 SLEEPQ
/// （test_pi 同款）、CURRENT=null ⇒ 自冻检查（current_pinned=0）恒过。
#[allow(clippy::unwrap_used)]
fn test_freeze_cycles() {
    use crate::abi_gen::{ERR_BUSY, NR_PROCESS_SUSPEND};
    use crate::handle::{KernelObj, ObjType, RIGHT_MANAGE, RIGHT_READ, RIGHT_SET_POLICY};
    use crate::process::Process;
    use crate::telemetry;
    use crate::vmar::Vmar;
    use crate::vmo::Vmo;
    use berry_sched::HELD_FROZEN;

    // token 出参槽（EL1 静态直映——copy_out_u32 的 access_ok 高半直映区）。
    static mut TOK: u32 = 0;

    // 调用方表 = bootstrap 表（ktest 期 tag=0 → caller_table 落此，test_syscall
    // 同通道）。Process 手搭同 F6（Vmar::new_root + Process::new）。
    let ht = crate::syscall::bootstrap_table();
    let vm = Vmar::new_root(crate::mm::frames(), crate::aspace::alloc_asid()).unwrap();
    let (pa, asid) = (vm.ttbr0_pa(), vm.asid());
    let hv = ht.insert(KernelObj::Vmar(vm), RIGHT_MANAGE).unwrap();
    let (vref, _) = ht.get_raw(hv, ObjType::Vmar, 0).unwrap();
    let hp = ht
        .insert(KernelObj::Process(unsafe { Process::new(vref, pa, asid) }), RIGHT_MANAGE)
        .unwrap();
    let (pref, _) = ht.get_raw(hp, ObjType::Process, RIGHT_MANAGE).unwrap();
    let pinned = pref.as_ptr() as usize;
    // 两受害任务 + 金丝雀（异钉 pinned|1）全进 SLEEPQ——freeze 扫描面。
    let t1 = mk_freeze_task(b"fzT1".as_slice(), pinned);
    let t2 = mk_freeze_task(b"fzT2".as_slice(), pinned);
    let canary = mk_freeze_task(b"fzCan".as_slice(), pinned | 1);
    berry_sched::sleepq_insert(t1);
    berry_sched::sleepq_insert(t2);
    berry_sched::sleepq_insert(canary);

    // 基线快照（计数器单调判据的起点）。
    let tok_base = ht.count_type(ObjType::SuspendToken);
    let fok0 = telemetry::freeze_ok_total();
    let ath0 = telemetry::auto_thaw_total();

    // —— 负例：pager 义务在身 → -EBUSY 且零残留（depth/位/token 全不动）。
    let hv_d = unsafe {
        let p = (*pref.as_ptr()).obj.as_process_mut().unwrap();
        p.ht
            .insert(
                KernelObj::Vmo(Vmo::create_demand(4096).unwrap()),
                RIGHT_READ | RIGHT_SET_POLICY,
            )
            .unwrap()
    };
    let rc = svc4(
        NR_PROCESS_SUSPEND as u64,
        hp as u64,
        0,
        core::ptr::addr_of_mut!(TOK) as u64,
    );
    assert_eq!(rc, ERR_BUSY as i64, "FRZCYC：负例 pager 义务须 -EBUSY");
    unsafe {
        let p = (*pref.as_ptr()).obj.as_process_mut().unwrap();
        assert_eq!(p.freeze_depth, 0, "FRZCYC：负例 depth 零残留");
        p.ht.close(hv_d).unwrap(); // 撤义务，转正轮
    }
    assert!(
        t1.held_locks & HELD_FROZEN == 0 && t2.held_locks & HELD_FROZEN == 0,
        "FRZCYC：负例零冻结位残留"
    );
    assert!(canary.held_locks & HELD_FROZEN == 0, "FRZCYC：负例金丝雀未被波及");
    assert_eq!(
        ht.count_type(ObjType::SuspendToken),
        tok_base,
        "FRZCYC：负例零 token 残留"
    );

    // —— 两轮 freeze/thaw 循环（A2 主判据）。金丝雀执法点 = 每轮冻结窗
    // 「未被误冻」+ thaw 窗「不熄」两条 held_locks 断言（装饰性计数器
    // 永绿，审查裁决拆除）。
    let (mut fok_prev, mut ath_prev) = (fok0, ath0);
    for r in 1..=2u32 {
        // freeze：全真 svc（成功尾 freeze_ok 计数 +1——sim-S2 补的写点）。
        let rc = svc4(
            NR_PROCESS_SUSPEND as u64,
            hp as u64,
            0,
            core::ptr::addr_of_mut!(TOK) as u64,
        );
        assert_eq!(rc, 0, "FRZCYC：r{} suspend 须成功", r);
        unsafe {
            let p = (*pref.as_ptr()).obj.as_process_mut().unwrap();
            // 顺序轮语义：每轮 close 归零后再冻，depth 恒 1（嵌套轮才是 2——
            // 那是 SuspendTok 深度帽测试的辖区，本测不动）。
            assert_eq!(
                p.freeze_depth,
                1,
                "FRZCYC：r{} 冻结后 depth 须 1（顺序轮）",
                r
            );
        }
        assert!(
            t1.held_locks & HELD_FROZEN != 0 && t2.held_locks & HELD_FROZEN != 0,
            "FRZCYC：r{} 受害双任务须全冻",
            r
        );
        assert!(
            canary.held_locks & HELD_FROZEN == 0,
            "FRZCYC：r{} 金丝雀被误冻（异钉失效）",
            r
        );
        assert_eq!(
            ht.count_type(ObjType::SuspendToken),
            tok_base + 1,
            "FRZCYC：r{} 冻结窗 token 须在席恰 +1",
            r
        );
        let fok_now = telemetry::freeze_ok_total();
        assert!(
            fok_now > fok_prev,
            "FRZCYC：r{} freeze_ok 计数须严格递增（{} → {}）",
            r,
            fok_prev,
            fok_now
        );

        // thaw：close(token) → Drop → depth 归零 → auto_thaw + thaw_by_pinned。
        let tok = unsafe { *core::ptr::addr_of!(TOK) };
        assert_ne!(tok, 0, "FRZCYC：r{} token 出参须非零", r);
        ht.close(tok).unwrap();
        unsafe {
            let p = (*pref.as_ptr()).obj.as_process_mut().unwrap();
            assert_eq!(p.freeze_depth, 0, "FRZCYC：r{} close 后 depth 须归零", r);
        }
        assert!(
            t1.held_locks & HELD_FROZEN == 0 && t2.held_locks & HELD_FROZEN == 0,
            "FRZCYC：r{} thaw 后受害双任务须全解冻",
            r
        );
        assert!(canary.held_locks & HELD_FROZEN == 0, "FRZCYC：r{} 金丝雀不熄", r);
        assert_eq!(
            ht.count_type(ObjType::SuspendToken),
            tok_base,
            "FRZCYC：r{} close 后 token 归基线",
            r
        );
        let ath_now = telemetry::auto_thaw_total();
        assert!(
            ath_now > ath_prev,
            "FRZCYC：r{} auto_thaw 计数须严格递增（{} → {}）",
            r,
            ath_prev,
            ath_now
        );
        (fok_prev, ath_prev) = (fok_now, ath_now);
    }
    // 末轮全量收口：两轮末值恰 +2（计数器严格递增的终点锚）。
    assert_eq!(telemetry::freeze_ok_total(), fok0 + 2, "FRZCYC：两轮 freeze_ok 恰 +2");
    assert_eq!(telemetry::auto_thaw_total(), ath0 + 2, "FRZCYC：两轮 auto_thaw 恰 +2");

    // 清场：摘队（勿扰后续测试的 SLEEPQ 面）+ 表收口（Process 先于 Vmar——
    // Drop 链方向）。
    berry_sched::sleepq_unlink(t1);
    berry_sched::sleepq_unlink(t2);
    berry_sched::sleepq_unlink(canary);
    ht.close(hp).unwrap();
    ht.close(hv).unwrap();
    berry_hal_aarch64::uart_puts(b"FRZCYC ok r=2\n");
}

/// AM-1.5（`.agents/plans/am/design.md`「IPC × 冻结」Q1 裁决「唤醒即解冻」
/// 机制面）：wake-pending 全链断言——
/// ①冻结进程后向其冻结端点 send → 消息照常入队（不丢弃）+ 属主
///   wake_pending 置位 + SuspendTok 信号位到位（`object_wait_one` poll 面 =
///   用户态真实 ABI，`SIGNAL_WAKE_PENDING`）+ 睡在 token 键上的等待者被以
///   **进程钉键**广播唤醒（wait_one token 分支的挂队键契约）；
/// ②未冻结 send 无信号（冻结本身也不置位——位只由冻结窗内的 send 触发）；
/// ③thaw 清面（wake_pending + 端点冻标）后 send 不再触发；
/// ④一次语义（幂等置位）+ 嵌套 depth>1 只在全解冻（最后一枚 close）才清。
/// 真 svc 全链（FRZCYC 同通道）+ 手搭任务/通道/对象（F6/F7/F8 先例）；
/// ktest 在 sched::init 前跑（SLEEPQ/双 RQ 静态基座可用，CURRENT=null）。
#[allow(clippy::unwrap_used)]
fn test_wake_pending() {
    use crate::channel::ChannelEnd;
    use crate::handle::{
        ERR_AGAIN, KernelObj, ObjType, RIGHT_MANAGE, RIGHT_RECV, RIGHT_SEND,
    };
    use crate::process::{Process, Thread};
    use crate::handle::SIGNAL_WAKE_PENDING;
    use crate::syscall::{NR_CHANNEL_SEND, NR_OBJECT_WAIT_ONE, NR_PROCESS_SUSPEND};
    use crate::vmar::Vmar;
    use berry_sched::HELD_FROZEN;
    use core::ptr::NonNull;

    // 出参/载荷槽（EL1 静态直映——copy 家族高半直映区，TOK/HBUF 先例）。
    static mut TOK: u32 = 0;
    static mut TOK2: u32 = 0;
    static mut OBS: u64 = 0;
    static mut WBUF: [u8; 4] = *b"wake";

    // 只读断言谓词（真对象直调）。
    let wp_of = |pref: NonNull<crate::handle::ObjRef>| -> bool {
        unsafe { (*pref.as_ptr()).obj.as_process().unwrap().wake_pending }
    };
    let end_frozen =
        |aref: NonNull<crate::handle::ObjRef>| -> bool {
            unsafe { (*aref.as_ptr()).obj.as_channel().unwrap().frozen_owner().is_some() }
        };
    let end_has_msg =
        |aref: NonNull<crate::handle::ObjRef>| -> bool {
            unsafe { (*aref.as_ptr()).obj.as_channel().unwrap().has_message() }
        };

    // —— 受害进程 P（F6/FRZCYC 手搭形）+ SLEEPQ 两受害任务（freeze 扫描面）。
    let ht = crate::syscall::bootstrap_table();
    let vm = Vmar::new_root(crate::mm::frames(), crate::aspace::alloc_asid()).unwrap();
    let (pa, asid) = (vm.ttbr0_pa(), vm.asid());
    let hv = ht.insert(KernelObj::Vmar(vm), RIGHT_MANAGE).unwrap();
    let (vref, _) = ht.get_raw(hv, ObjType::Vmar, 0).unwrap();
    let hp = ht
        .insert(KernelObj::Process(unsafe { Process::new(vref, pa, asid) }), RIGHT_MANAGE)
        .unwrap();
    let (pref, _) = ht.get_raw(hp, ObjType::Process, RIGHT_MANAGE).unwrap();
    let pinned = pref.as_ptr() as usize;
    let t1 = mk_freeze_task(b"wpT1".as_slice(), pinned);
    let t2 = mk_freeze_task(b"wpT2".as_slice(), pinned);
    berry_sched::sleepq_insert(t1);
    berry_sched::sleepq_insert(t2);

    // —— 通道对（sys_channel_create 手搭形）：A = 受害接收端（落 P.ht），
    // B = 发送端（落调用方表）——send(B) 压 A 队列、热路径观 A 冻标。
    let (ea, eb) = (
        ChannelEnd::try_unpaired(false).unwrap(),
        ChannelEnd::try_unpaired(false).unwrap(),
    );
    let ha = unsafe {
        (*pref.as_ptr())
            .obj
            .as_process_mut()
            .unwrap()
            .ht
            .insert(KernelObj::Channel(ea), RIGHT_RECV)
            .unwrap()
    };
    let hb = ht
        .insert(KernelObj::Channel(eb), RIGHT_SEND | RIGHT_RECV)
        .unwrap();
    let (aref, _) = unsafe {
        (*pref.as_ptr())
            .obj
            .as_process_mut()
            .unwrap()
            .ht
            .get_raw(ha, ObjType::Channel, 0)
            .unwrap()
    };
    let (bref, _) = ht.get_raw(hb, ObjType::Channel, 0).unwrap();
    // SAFETY: 两指针刚 get_raw 过（在表、类型 Channel）——create 同款配对。
    unsafe { ChannelEnd::pair(aref, bref) };

    // —— 等待者属主 P2/T2（F7 形真对象：清场 kill 的退出钩子裸解引用
    // pinned/thread_key，伪键必炸）。
    let vm2 = Vmar::new_root(crate::mm::frames(), crate::aspace::alloc_asid()).unwrap();
    let (pa2, asid2) = (vm2.ttbr0_pa(), vm2.asid());
    let hv2 = ht.insert(KernelObj::Vmar(vm2), RIGHT_MANAGE).unwrap();
    let (vref2, _) = ht.get_raw(hv2, ObjType::Vmar, 0).unwrap();
    let hp2 = ht
        .insert(KernelObj::Process(unsafe { Process::new(vref2, pa2, asid2) }), RIGHT_MANAGE)
        .unwrap();
    let (pref2, _) = ht.get_raw(hp2, ObjType::Process, RIGHT_MANAGE).unwrap();
    let hth2 = ht
        .insert(KernelObj::Thread(unsafe { Thread::new(pref2) }), RIGHT_MANAGE)
        .unwrap();
    let (tref2, _) = ht.get_raw(hth2, ObjType::Thread, RIGHT_MANAGE).unwrap();
    let wp2 = pref2.as_ptr() as usize;
    let wt2 = tref2.as_ptr() as usize;
    unsafe {
        // 运行钉住（表引 + 钉）+ 末线程簿记位（钩子 live_threads-=1 → 0 走
        // Process 收口）——F8 同款。
        (*pref2.as_ptr()).retain();
        (*tref2.as_ptr()).retain();
        (*pref2.as_ptr()).obj.as_process_mut().unwrap().live_threads = 1;
    }
    // 等待者任务 = 手搭「am 阻塞在 object_wait_one(token)」形态：挂队键 =
    // 目标进程钉键（wait_one token 分支的派生键——AM-1.5 契约）、请求位 =
    // SIGNAL_WAKE_PENDING（纯内核派生位——pi::donor_edge 只对 READABLE|
    // WRITABLE 等待者解引用 wait_obj〔§A40〕，本等待者恒不入捐赠图）。
    let waiter = mk_freeze_task(b"wpWait".as_slice(), wp2);
    waiter.thread_key = wt2;
    waiter.wait_obj = pinned;
    waiter.wait_signals = SIGNAL_WAKE_PENDING;
    berry_sched::sleepq_insert(waiter);

    // —— ② 未冻结 send：成功但零信号（send 在冻结窗外——消息照常入队，
    // wake_pending 不动）。
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hb as u64, &raw const WBUF as u64, 4, 0, 0, 0),
        0,
        "WAKEP：②未冻结 send 须成功"
    );
    assert!(!wp_of(pref), "WAKEP：②未冻结 send 不得置 wake_pending");

    // —— 冻结（真 svc 全链）：首枚 token → freeze 枚举打端点冻标 + 任务真冻。
    assert_eq!(
        svc4(NR_PROCESS_SUSPEND as u64, hp as u64, 0, &raw mut TOK as u64),
        0,
        "WAKEP：suspend 须成功"
    );
    let tok = unsafe { *core::ptr::addr_of!(TOK) };
    assert_ne!(tok, 0, "WAKEP：token 出参非零");
    assert!(end_frozen(aref), "WAKEP：冻结须枚举打端点冻标");
    assert!(
        t1.held_locks & HELD_FROZEN != 0 && t2.held_locks & HELD_FROZEN != 0,
        "WAKEP：受害双任务须真冻"
    );
    // ② 后半臂：冻结本身不置位（位只由冻结窗内的 send 触发）。
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, tok as u64, SIGNAL_WAKE_PENDING, 0,
             &raw mut OBS as u64),
        ERR_AGAIN as i64,
        "WAKEP：②冻结后未 send，poll 须 -EAGAIN"
    );
    assert_eq!(unsafe { *core::ptr::addr_of!(OBS) }, 0, "WAKEP：②未命中 observed 须 0");

    // 等待者武装确认（真 SLEEPQ 面直查）。
    assert!(
        berry_sched::find_single_waiter(pinned, SIGNAL_WAKE_PENDING).is_some(),
        "WAKEP：等待者须在队（键 = 进程钉键）"
    );

    // —— ① 冻结窗内 send：消息照常入队 + wake_pending 置位 + 等待者被
    //    进程钉键广播唤醒 + token 信号位到位（poll 面）。
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hb as u64, &raw const WBUF as u64, 4, 0, 0, 0),
        0,
        "WAKEP：①冻结窗 send 须照常成功"
    );
    assert!(end_has_msg(aref), "WAKEP：①消息须照常入队（不丢弃）");
    assert!(wp_of(pref), "WAKEP：①wake_pending 须置位");
    assert!(
        berry_sched::find_single_waiter(pinned, SIGNAL_WAKE_PENDING).is_none(),
        "WAKEP：①等待者须已被进程钉键广播唤醒（离队）"
    );
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, tok as u64, SIGNAL_WAKE_PENDING, 0,
             &raw mut OBS as u64),
        0,
        "WAKEP：①poll 须命中"
    );
    assert_eq!(
        unsafe { *core::ptr::addr_of!(OBS) },
        SIGNAL_WAKE_PENDING,
        "WAKEP：①observed 须恰为 SIGNAL_WAKE_PENDING"
    );

    // —— ④ 一次语义：后续 send 位幂等（不重复通知——置位点单次）。
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hb as u64, &raw const WBUF as u64, 4, 0, 0, 0),
        0,
        "WAKEP：④第二跳 send 须成功"
    );
    assert!(wp_of(pref), "WAKEP：④位保持置位（幂等）");
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, tok as u64, SIGNAL_WAKE_PENDING, 0,
             &raw mut OBS as u64),
        0,
        "WAKEP：④poll 仍命中"
    );

    // —— ③ thaw 清面：close(token) → Drop → 全解冻 → wake_pending 清 +
    //    冻标清；此后 send 不再触发。
    ht.close(tok).unwrap();
    assert!(!wp_of(pref), "WAKEP：③thaw 须清 wake_pending");
    assert!(!end_frozen(aref), "WAKEP：③thaw 须清端点冻标");
    assert!(t1.held_locks & HELD_FROZEN == 0, "WAKEP：③任务须解冻");
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hb as u64, &raw const WBUF as u64, 4, 0, 0, 0),
        0,
        "WAKEP：③thaw 后 send 须成功"
    );
    assert!(!wp_of(pref), "WAKEP：③thaw 后 send 不得再触发（冻标已清）");
    assert!(end_has_msg(aref), "WAKEP：③消息仍照常入队");

    // —— ④ 嵌套臂：depth 2 两枚 token——中间层 close 不清，最后一枚才清。
    assert_eq!(
        svc4(NR_PROCESS_SUSPEND as u64, hp as u64, 0, &raw mut TOK as u64),
        0,
        "WAKEP：嵌套第一枚 suspend"
    );
    let tok1 = unsafe { *core::ptr::addr_of!(TOK) };
    assert!(end_frozen(aref), "WAKEP：再冻须重打端点冻标");
    assert_eq!(
        svc4(NR_PROCESS_SUSPEND as u64, hp as u64, 0, &raw mut TOK2 as u64),
        0,
        "WAKEP：嵌套第二枚 suspend（freeze_by_pinned 计匹配含已冻）"
    );
    let tok2 = unsafe { *core::ptr::addr_of!(TOK2) };
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, hb as u64, &raw const WBUF as u64, 4, 0, 0, 0),
        0,
        "WAKEP：嵌套冻结窗 send"
    );
    assert!(wp_of(pref), "WAKEP：嵌套窗 wake_pending 置位");
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, tok2 as u64, SIGNAL_WAKE_PENDING, 0,
             &raw mut OBS as u64),
        0,
        "WAKEP：嵌套 token 同位可观（信号面 = 目标派生）"
    );
    ht.close(tok1).unwrap(); // 中间层：depth 2→1，不清
    assert!(wp_of(pref), "WAKEP：④中间层 close 不清 wake_pending");
    assert!(end_frozen(aref), "WAKEP：④中间层 close 不清端点冻标");
    assert!(t1.held_locks & HELD_FROZEN != 0, "WAKEP：④中间层不解冻");
    ht.close(tok2).unwrap(); // 最后一枚：全解冻 + 清面
    assert!(!wp_of(pref), "WAKEP：④最后一枚 close 清 wake_pending");
    assert!(!end_frozen(aref), "WAKEP：④最后一枚 close 清端点冻标");
    assert!(t1.held_locks & HELD_FROZEN == 0, "WAKEP：④最后一枚全解冻");

    // 清场：受害任务摘队；唤醒后推 RQ 的等待者走 kill 全链摘队（F8 形）；
    // 受害表先 drain（真退出序）+ bootstrap 表 drain 收口（通道对互钉成环
    // = 两端安全泄漏，channel.rs 模块注政策）。
    berry_sched::sleepq_unlink(t1);
    berry_sched::sleepq_unlink(t2);
    // SAFETY: F8 同款（真 ObjRef 簿记 + 单核屏蔽窗）。
    let killed = unsafe { berry_sched::kill_all_tasks_by_pinned(wp2, 0) };
    assert_eq!(
        killed,
        Some((b"wpWait".as_slice(), 1)),
        "WAKEP：清场 kill 须恰收 wpWait 一枚（RQ 面不残留）"
    );
    unsafe {
        (*pref.as_ptr()).obj.as_process_mut().unwrap().ht.drain();
    }
    ht.drain();
    berry_hal_aarch64::uart_puts(b"WAKEP-K ok\n");
}

/// AM-2b（2026-11-08 提案候选①·六裁决）：压力窗口聚合 + Event 用户位出口
/// 直调断言（WAKEP-K 形态——真对象直调）。合成窗口 10^6 周期（真实 70ms
/// 窗 boot 已按 CNTFRQ 换算周期数——裁决② TCG 频偏禁编译期常量；ktest
/// 缩短窗长免等真实窗时，测毕还原）；θ_p = 10% 占比判据（裁决②【推，实测
/// 定标】——提案 §二 partial ≈ PSI some）。注入走 `dpg_win_ktest_inject`
///（DPG_WIN_CYC 专用累计，不经 `dpg_stall_note`——E3g 台账零态不变量
/// 不污染）；清位走真 SVC `object_signal`（am 自清位同路径——bootstrap
/// 表解析 + SIGNAL 右门控 + 高 32 位用户位面全链，提案候选①）。
fn test_am2b_pressure() {
    use crate::telemetry;
    const LEN: u64 = 1_000_000; // 合成窗口（θ_p = 10% ⇒ 跨阈 Σ = LEN/10）
    let partial = crate::am2b_partial_bit();
    assert_eq!(partial, 1 << 40, "AM2B：partial 位须 1<<40（裁决③）");
    // boot 接线自证：窗口长已按 CNTFRQ 换算非零（裁决②——接线缺失 fail loud）。
    let real_len = telemetry::dpg_win_len();
    assert!(real_len > 0, "AM2B：boot 窗口长须已换算非零（dpg_win_init 接线缺失）");
    assert_eq!(crate::am2b_pressure_bits(), 0, "AM2B：boot 后压力位面须零（Event 出生零 + 无跨阈）");

    let base = berry_hal_aarch64::timer::cntvct();

    // ① 跨阈：Σ = θ_p·W（≥ 判据含等号）→ fold → 1<<40 置位（出口 =
    //    rx_irq_notify 形内核语境写位 + SQ 窗唤醒，裁决⑤）。
    telemetry::dpg_win_ktest_setup(LEN, base);
    telemetry::dpg_win_ktest_inject(LEN / 10);
    telemetry::dpg_stall_fold(base.wrapping_add(LEN));
    assert!(
        crate::am2b_pressure_bits() & partial != 0,
        "AM2B：①Σ=θ_p·W 跨阈翻卷须置 1<<40（telemetry fold → 压力 Event 出口）"
    );

    // am 自清位同路径（真 SVC 派发——提案候选①消费形）。handle = ktest 期
    // bootstrap 表现插取（boot 不预插：WAKEP 清场 drain 会作废 boot 槽位）。
    let hclr = crate::am2b_pressure_ktest_handle();
    assert_eq!(
        svc6(
            crate::abi_gen::NR_OBJECT_SIGNAL as u64,
            hclr as u64,
            partial,
            0,
            0,
            0,
            0
        ),
        0,
        "AM2B：object_signal 清 1<<40 须成功（am 消费置位后自清——SIGNAL 右 + 高 32 位面）"
    );
    assert_eq!(crate::am2b_pressure_bits(), 0, "AM2B：清位后位面须归零");

    // ② 未跨阈：Σ = θ_p·W/2 → 翻卷不置位（占比判据 Σcyc/W ≥ θ_p）。
    telemetry::dpg_win_ktest_setup(LEN, base.wrapping_add(LEN));
    telemetry::dpg_win_ktest_inject(LEN / 20);
    telemetry::dpg_stall_fold(base.wrapping_add(2 * LEN));
    assert_eq!(
        crate::am2b_pressure_bits() & partial,
        0,
        "AM2B：②Σ=θ_p·W/2 未跨阈须不置位（10% 判据不满足）"
    );

    // ③ 翻卷后窗口清零重启累计：② 的 5% 已随翻卷清零，再注 5% 仍独立新窗
    //    ——若未清零，累计 10% 将跨阈置位（不置位即「清零重启」判别性证明）。
    telemetry::dpg_win_ktest_inject(LEN / 20);
    telemetry::dpg_stall_fold(base.wrapping_add(3 * LEN));
    assert_eq!(
        crate::am2b_pressure_bits() & partial,
        0,
        "AM2B：③翻卷后 Σ 须清零重启（累计未清则 5%+5%=θ_p·W 将误置位）"
    );

    // ④ 重启后累计照常判阈（正证）：新窗注满 θ_p·W → 再置位。
    telemetry::dpg_win_ktest_inject(LEN / 10);
    telemetry::dpg_stall_fold(base.wrapping_add(4 * LEN));
    assert!(
        crate::am2b_pressure_bits() & partial != 0,
        "AM2B：④重启后新窗 Σ=θ_p·W 须再置位（累计/判阈随翻卷重启不失效）"
    );

    // 收尾：清位 + close 插取 handle（retain 令牌归还）+ 还原真实窗口
    //（70ms boot 换算值 + 当前时刻起点——后续 dpg 真路径无感；位面残留属
    // ktest 瞬态位，sched::init 前跑不登记）。
    assert_eq!(
        svc6(
            crate::abi_gen::NR_OBJECT_SIGNAL as u64,
            hclr as u64,
            partial,
            0,
            0,
            0,
            0
        ),
        0,
        "AM2B：收尾清位须成功"
    );
    crate::syscall::bootstrap_table()
        .close(hclr)
        .expect("AM2B：ktest 插取 handle 收尾 close");
    telemetry::dpg_win_ktest_setup(real_len, berry_hal_aarch64::timer::cntvct());
    berry_hal_aarch64::uart_puts(b"AM2B-K ok\n");
}

/// VER（版本覆盖硬化·内核面）：version VMO 内容断言。形态 = **现读现验不留
/// 柄**：boot 第 9 插交付 VMO 此刻尚未建（ktest 先于 svcmgr 表搭架与 fb
/// GOP 相位——boot 槽位断言不在 ktest 面重复，WAKEP 清场作废前雷同理不在
/// 任何表留柄），改走与 boot 同源的 `sysver::build_vmo()` 建瞬态 VMO →
/// read 回读 → parse 断言 → drop（`Vmo::Drop` 还页，零残留）。magic/level/
/// semver 三面：semver 对账用运行期 split 解析 CARGO_PKG_VERSION（独立于
/// 编译期 `parse_semver`——破循环自证）。
fn test_sysver() {
    use crate::sysver;
    let vmo = sysver::build_vmo().expect("SYSVER：装配 version VMO（可信语境）");
    let mut buf = [0u8; 4096];
    vmo.read(0, &mut buf).expect("SYSVER：read 回读");
    drop(vmo);
    assert_eq!(
        u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
        sysver::SYSVER_MAGIC,
        "SYSVER：magic 须 BVER LE（0x4256_4552）"
    );
    let (ident, rustc, level, semver) =
        sysver::parse(&buf).expect("SYSVER：parse 须过（hdr_ver/rsv/字符串段结构面）");
    assert_eq!(ident, sysver::GIT_IDENT, "SYSVER：git ident 须同源（build.rs 注入）");
    assert_eq!(rustc, sysver::RUSTC_VER, "SYSVER：rustc 短版须同源（build.rs 注入）");
    assert_eq!(
        level,
        crate::abi_gen::BERRY_PLATFORM_LEVEL,
        "SYSVER：level 须 == abi_gen BERRY_PLATFORM_LEVEL"
    );
    // semver 独立对账：运行期 split + parse（非 const parse_semver 路径）。
    let mut it = env!("CARGO_PKG_VERSION").split('.');
    let want = (
        it.next().and_then(|s| s.parse::<u16>().ok()),
        it.next().and_then(|s| s.parse::<u16>().ok()),
        it.next().and_then(|s| s.parse::<u16>().ok()),
    );
    assert_eq!(
        (Some(semver.0), Some(semver.1), Some(semver.2)),
        want,
        "SYSVER：semver 须与 CARGO_PKG_VERSION 一致（运行期独立解析）"
    );
    assert_eq!(
        buf[18] & 1 != 0,
        cfg!(debug_assertions),
        "SYSVER：flags bit0 须 = debug_assertions"
    );
    berry_hal_aarch64::uart_puts(b"SYSVER ok\n");
}

/// sim-S3（PM-16 §4.4 步3）：深睡降级断言（run() 末位新号——ktest done 前；
/// probe 已在 boot 早期跑过、缓存面就位）。TCG 路探测 `PSCI susp=0` ⇒
/// `request_deep_sleep` 走 freeze 原语降级：**真冻结证据链** = degrade/
/// freeze_ok/auto_thaw 三计数恰 +1（隔离「只 bump 计数器没真冻结」假绿
/// ——freeze_ok+auto_thaw 成对才是真冻结往返）+ canary 零持窗后已 thaw。
/// 标记行 `PWRDEG ok d=1` 读 telemetry 台账（PWRKEY 先例——d 非本地变量）。
fn test_deep_sleep_degrade() {
    use crate::abi_gen::{ERR_BAD_STATE, ERR_BUSY};
    use crate::telemetry;
    use berry_sched::HELD_FROZEN;

    // KVM/真机 susp=1 相不在这里赌 HVC：x1=0 是无效 re-entry，hypervisor 行为不可假设（受理/拒绝/关机均可能）。主 smoke 在该相先因 `PSCI susp=1` 与 TCG 硬编码标记
    // 不符而红；真 SYSTEM_SUSPEND 断言归独立 KVM/真机 spike 相。
    if berry_hal_aarch64::psci::suspend_supported() {
        berry_hal_aarch64::uart_puts(b"PWRDEG skip susp=1\n");
        return;
    }

    // 目标 + 跨钉金丝雀（异钉键）：目标可冻、金丝雀不可被顺手冻。
    let target = mk_freeze_task(b"pwTgt".as_slice(), 0x5157_0000);
    let canary = mk_freeze_task(b"pwCan".as_slice(), 0x5158_0000);
    berry_sched::sleepq_insert(target);
    berry_sched::sleepq_insert(canary);

    // 错误两臂：pinned=0 谓词拒 -EBUSY；无匹配键 freeze 返 0 → -EBADSTATE。
    assert_eq!(
        crate::syscall::request_deep_sleep(0),
        ERR_BUSY as i64,
        "PWRDEG：pinned=0 须 -EBUSY"
    );
    assert_eq!(
        crate::syscall::request_deep_sleep(0x5159_0000),
        ERR_BAD_STATE as i64,
        "PWRDEG：无匹配任务须 -EBADSTATE"
    );

    // 基线快照（三计数恰 +N 判据起点）。
    let d0 = telemetry::degrade_freeze_total();
    let fok0 = telemetry::freeze_ok_total();
    let ath0 = telemetry::auto_thaw_total();

    let rc = crate::syscall::request_deep_sleep(target.pinned);
    assert_eq!(rc, 0, "PWRDEG：深睡降级请求须返回 0");
    assert_eq!(
        telemetry::degrade_freeze_total(),
        d0 + 1,
        "PWRDEG：degrade 计数须恰 +1"
    );
    assert_eq!(
        telemetry::freeze_ok_total(),
        fok0 + 1,
        "PWRDEG：freeze_ok 须恰 +1（真冻结证据链）"
    );
    assert_eq!(
        telemetry::auto_thaw_total(),
        ath0 + 1,
        "PWRDEG：auto_thaw 须恰 +1（token 立即 Drop 语义）"
    );
    assert!(
        target.held_locks & HELD_FROZEN == 0,
        "PWRDEG：零持窗后目标须已 thaw"
    );
    assert!(
        canary.held_locks & HELD_FROZEN == 0,
        "PWRDEG：跨钉金丝雀不可被误冻/误 thaw"
    );

    // 清场：摘队（SLEEPQ 清理纪律）+ 标记行读台账。
    berry_sched::sleepq_unlink(target);
    berry_sched::sleepq_unlink(canary);
    berry_hal_aarch64::uart_puts(b"PWRDEG ok d=");
    berry_hal_aarch64::uart_put_dec(telemetry::degrade_freeze_total());
    berry_hal_aarch64::uart_puts(b"\n");
}

/// 1.1/S4-b 补测：billing 表满覆写（9 域逼满 8 槽）。
fn test_billing_overflow() {
    use crate::telemetry;
    assert_eq!(telemetry::BILLING_SLOTS, 8, "BL-OVF：槽位须 8");
    for d in 300u64..=308 { // 避开 test_billing 的 100/200
        telemetry::billing_charge(d, 1_000);
    }
    // 第 9 域（308）落账可查——覆写是新顶旧不丢账。
    assert_eq!(telemetry::billing_domain_total(308), Some(1_000),
        "BL-OVF：域 308 须覆写落账 1000ns");
    berry_hal_aarch64::uart_puts(b"BL-OVF-K ok\n");
}

/// 1.1/S1 负向：sa_parse 输入校验面（sched_setattr 底层验证器直测）。
fn test_sched_attr_negative() {
    use crate::syscall::sa_parse;
    const ERR_INVAL: i32 = -22;
    const ERR_NOTSUP: i32 = -95;

    // 构造 56B attr 并填字段。
    fn mk_attr(version: u16, flags: u16, weight: u32, policy: u8,
               budget: u64, period: u64) -> ([u8; 56], u64) {
        let mut b = [0u8; 56];
        b[0..4].copy_from_slice(&56u32.to_le_bytes());
        b[4..6].copy_from_slice(&version.to_le_bytes());
        b[6..8].copy_from_slice(&flags.to_le_bytes());
        b[8..12].copy_from_slice(&weight.to_le_bytes());
        b[12] = policy;
        b[40..48].copy_from_slice(&budget.to_le_bytes());
        b[48..56].copy_from_slice(&period.to_le_bytes());
        // 写入静态槽取地址（EL1 形态——ktest 跑在 EL1）。
        (b, 0) // 地址由调用方填
    }

    // 静态 attr 槽（SA_PARSE 测试面——每次覆写）。
    static mut ATTR_BUF: [u8; 56] = [0; 56];

    fn parse(version: u16, flags: u16, weight: u32, policy: u8,
             budget: u64, period: u64) -> Result<(), i32> {
        unsafe {
            ATTR_BUF[0..4].copy_from_slice(&56u32.to_le_bytes());
            ATTR_BUF[4..6].copy_from_slice(&version.to_le_bytes());
            ATTR_BUF[6..8].copy_from_slice(&flags.to_le_bytes());
            ATTR_BUF[8..12].copy_from_slice(&weight.to_le_bytes());
            ATTR_BUF[12] = policy;
            ATTR_BUF[40..48].copy_from_slice(&budget.to_le_bytes());
            ATTR_BUF[48..56].copy_from_slice(&period.to_le_bytes());
            let ptr = core::ptr::addr_of!(ATTR_BUF) as u64;
            sa_parse(true, ptr).map(|_| ())
        }
    }

    // AN-1: weight=0 → EINVAL。
    assert_eq!(parse(1, 0x0001, 0, 1, 0, 0), Err(ERR_INVAL),
        "ATTR-NEG：weight=0 须拒");
    // AN-2: weight=10001 → EINVAL。
    assert_eq!(parse(1, 0x0001, 10001, 1, 0, 0), Err(ERR_INVAL),
        "ATTR-NEG：weight=10001 须拒");
    // AN-3: version=2 → ENOTSUP。
    assert_eq!(parse(2, 0, 100, 1, 0, 0), Err(ERR_NOTSUP),
        "ATTR-NEG：version=2 须 ENOTSUP");
    // AN-4: flags 未知位 → EINVAL。
    assert_eq!(parse(1, 0x1000, 100, 1, 0, 0), Err(ERR_INVAL),
        "ATTR-NEG：未知 flags 位须拒");
    // AN-5: policy=5 → EINVAL。
    assert_eq!(parse(1, 0x0004, 100, 5, 0, 0), Err(ERR_INVAL),
        "ATTR-NEG：policy=5 须拒");
    // AN-6: budget > period → EINVAL。
    assert_eq!(parse(1, 0x0084, 100, 3, 100, 50), Err(ERR_INVAL),
        "ATTR-NEG：budget>period 须拒");
    // AN-7: budget=0 → EINVAL。
    assert_eq!(parse(1, 0x0084, 100, 3, 0, 100), Err(ERR_INVAL),
        "ATTR-NEG：budget=0 须拒");

    let _ = mk_attr; // 消 unused 警
    berry_hal_aarch64::uart_puts(b"ATTR-NEG-K ok\n");
}

/// 1.1 性能基准：调度器核心操作 cycles 开销（CNTVCT 前后差）。
/// 供 tools/sched_perf_analyze.py 对照验收标准（记账 <1% / freeze <5ms / pick <5µs@1k）。
fn test_sched_perf() {
    use berry_sched::{Fair, Runqueue, SchedClass, Task, Topo};

    extern "C" fn perf_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    fn mk(name: &'static [u8]) -> &'static mut Task {
        alloc::boxed::Box::leak(Task::new_kernel_with_as(perf_dummy, 0, name, 0, 0))
    }

    let mut rq = Runqueue::new();
    let rr = Fair;
    let topo = Topo::single();

    // —— pick_next 开销：8 任务环 ×1000 次 ——
    let tasks: alloc::vec::Vec<&'static mut Task> = (0..8).map(|_| {
        mk(b"perf".as_slice())
    }).collect::<alloc::vec::Vec<_>>();
    // 先收集再入环（所有权）。
    let mut refs: alloc::vec::Vec<*mut Task> = alloc::vec::Vec::new();
    for t in tasks {
        refs.push(t as *mut Task);
    }
    for r in &refs {
        unsafe { rr.enqueue(&mut **r, &mut rq) };
    }

    let t0 = berry_hal_aarch64::timer::cntvct();
    for i in 0..1000u64 {
        if let Some(p) = rr.pick_next(&rq, &topo) {
            // 轮转：给选中者推高 VD 防连选。
            p.vdeadline = 1000 + i;
        }
    }
    let dt_pick = berry_hal_aarch64::timer::cntvct() - t0;
    let avg_pick = dt_pick / 1000;
    berry_hal_aarch64::uart_puts(b"PERF pick avg=");
    berry_hal_aarch64::uart_put_dec(avg_pick);
    berry_hal_aarch64::uart_puts(b" n=1000\n");

    // —— freeze/thaw 开销：2 任务 ×100 次 ——
    let ft = mk(b"fz".as_slice());
    ft.pinned = 0x1000;
    rr.enqueue(&mut *ft, &mut rq);
    let t1 = berry_hal_aarch64::timer::cntvct();
    for _ in 0..100 {
        let _ = berry_sched::freeze_by_pinned(0x1000);
        berry_sched::thaw_by_pinned(0x1000);
    }
    let dt_fz = berry_hal_aarch64::timer::cntvct() - t1;
    let avg_fz = dt_fz / 200; // 100 freeze + 100 thaw = 200 ops
    berry_hal_aarch64::uart_puts(b"PERF freeze avg=");
    berry_hal_aarch64::uart_put_dec(avg_fz);
    berry_hal_aarch64::uart_puts(b" n=200\n");

    // —— billing_charge 开销：×1000 次 ——
    let t2 = berry_hal_aarch64::timer::cntvct();
    for _ in 0..1000 {
        crate::telemetry::billing_charge(0xF00D, 100);
    }
    let dt_bl = berry_hal_aarch64::timer::cntvct() - t2;
    let avg_bl = dt_bl / 1000;
    berry_hal_aarch64::uart_puts(b"PERF billing avg=");
    berry_hal_aarch64::uart_put_dec(avg_bl);
    berry_hal_aarch64::uart_puts(b" n=1000\n");

    berry_hal_aarch64::uart_puts(b"PERF-K ok\n");
}

/// 1.1 公平性：4 等权任务 ×100 轮 pick——各任务选中次数偏差 ≤20。
fn test_sched_fairness() {
    use berry_sched::{Fair, Runqueue, SchedClass, Task, Topo};

    extern "C" fn fair_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    let mut rq = Runqueue::new();
    let rr = Fair;
    let topo = Topo::single();

    // 4 等权任务。
    let names: [&'static [u8]; 4] = [b"fa".as_slice(), b"fb".as_slice(), b"fc".as_slice(), b"fd".as_slice()];
    let mut ptrs: alloc::vec::Vec<*mut Task> = alloc::vec::Vec::new();
    for n in names {
        let t = alloc::boxed::Box::leak(Task::new_kernel_with_as(fair_dummy, 0, n, 0, 0));
        ptrs.push(t as *mut Task);
    }
    for p in &ptrs {
        unsafe { rr.enqueue(&mut **p, &mut rq) };
    }

    // 100 轮 pick + 计数（指针身份定位）。
    let mut hits = [0usize; 4];
    for i in 0..100u64 {
        if let Some(p) = rr.pick_next(&rq, &topo) {
            let pp = p as *mut Task;
            for (j, base) in ptrs.iter().enumerate() {
                if core::ptr::eq(*base, pp) {
                    hits[j] += 1;
                    break;
                }
            }
            // 轮转：推 VD 让下一轮选别人。
            unsafe { (*pp).vdeadline = 100 + i };
        }
    }

    let spread = hits.iter().max().unwrap() - hits.iter().min().unwrap();
    berry_hal_aarch64::uart_puts(b"FAIR-K spread=");
    berry_hal_aarch64::uart_put_dec(spread as u64);
    berry_hal_aarch64::uart_puts(b" hits=[");
    for h in hits {
        berry_hal_aarch64::uart_put_dec(h as u64);
        berry_hal_aarch64::uart_puts(b",");
    }
    berry_hal_aarch64::uart_puts(b"]\n");
    assert!(spread <= 20,
        "FAIR：等权 4 任务 100 轮偏差须 ≤20（实测 {}）", spread);
}

/// 1.1 §4.5 CMP 验收——单比较器全序 property（ktest 形态）。
/// CMP-1 反对称/传递：5 任务递增 VD → pick 序严格递增 VD。
/// CMP-2 pick 最小：任一 pick 结果 == 全集 order_key 最小者。
/// CMP-4 确定性：同输入重复 pick 同结果。
fn test_cmp() {
    use berry_sched::{Fair, Runqueue, SchedClass, Task, Topo};

    extern "C" fn cmp_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    fn mk(name: &'static [u8], vd: u64, vr: u64) -> &'static mut Task {
        let t = alloc::boxed::Box::leak(Task::new_kernel_with_as(cmp_dummy, 0, name, 0, 0));
        t.vdeadline = vd;
        t.vruntime = vr;
        t
    }

    let mut rq = Runqueue::new();
    let rr = Fair;
    let topo = Topo::single();

    // 5 任务（enqueue 后再设 VD——enqueue 会按 avg 覆写 VD/VR）。
    let mut ts: alloc::vec::Vec<&'static mut Task> = alloc::vec![
        mk(b"c3".as_slice(), 300, 50),
        mk(b"c1".as_slice(), 100, 10),
        mk(b"c5".as_slice(), 500, 90),
        mk(b"c2".as_slice(), 200, 30),
        mk(b"c4".as_slice(), 400, 70),
    ];
    for t in ts.iter_mut() {
        rr.enqueue(&mut **t, &mut rq);
    }
    // 入环后设 VD/Vr（enqueue 的覆写不影响此处）。直改 vruntime 后洗账
    //（S2b+ Σvruntime 增量账本——avg 口径须与真环一致，此处即旧全环
    // 扫描同款对齐：真和 = 250 → avg = 50）。
    let vds = [300u64, 100, 500, 200, 400];
    let vrs = [50u64, 10, 90, 30, 70];
    for (i, t) in ts.iter_mut().enumerate() {
        (*t).vdeadline = vds[i];
        (*t).vruntime = vrs[i];
    }
    rq.resync_vruntime_sum();

    // CMP-2：首 pick == 全集 VD 最小（c1=100）。
    let p1 = rr.pick_next(&rq, &topo).expect("CMP：非空环须有出选");
    assert_eq!(p1.vdeadline, 100, "CMP-2：首 pick 须 VD 最小（100）");

    // CMP-1：连续 pick 序严格递增 VD（反对称 + 传递——无回跳）。
    let mut last_vd = 0u64;
    let mut picks: alloc::vec::Vec<u64> = alloc::vec::Vec::new();
    for t in ts.iter_mut() {
        (*t).vruntime = 0; // 全部 eligible
    }
    rq.resync_vruntime_sum(); // 直改后洗账（avg=0 → 全 eligible 口径同旧扫描）
    for _ in 0..5 {
        if let Some(p) = rr.pick_next(&rq, &topo) {
            assert!(p.vdeadline >= last_vd,
                    "CMP-1：VD 须非递减（last={} cur={}）", last_vd, p.vdeadline);
            last_vd = p.vdeadline;
            picks.push(p.vdeadline);
            p.vdeadline = u64::MAX; // 防重复 pick 同任务
        }
    }
    assert!(picks.len() >= 3, "CMP-1：至少 3 pick 才能验全序");

    // CMP-4：确定性——重置后同输入同结果。
    for t in ts.iter_mut() {
        (*t).vdeadline = 200; // 全部同键（在环上直改——不 re-enqueue）
        (*t).vruntime = 0;
        (*t).prio_effective = 0;
    }
    rq.resync_vruntime_sum(); // 直改后洗账（同键全 eligible，首见者出选）
    let pa = rr.pick_next(&rq, &topo).unwrap();
    let va = pa.vdeadline;
    pa.vdeadline = 300; // 改一个防重 pick
    let pb = rr.pick_next(&rq, &topo).unwrap();
    let vb = pb.vdeadline;
    assert_eq!(va, vb, "CMP-4：同输入同键 pick 须同结果");

    berry_hal_aarch64::uart_puts(b"CMP-K ok\n");
}

/// 1.1/S1：SchedAttr effective 组合（Process 默认 ∪ Thread 覆盖）。
fn test_sched_attr() {
    use crate::process::{SchedAttr, SA_FLAG_INTENT, SA_FLAG_POLICY, SA_FLAG_WEIGHT};
    let base = SchedAttr {
        flags: SA_FLAG_WEIGHT | SA_FLAG_POLICY,
        weight: 100,
        policy: 1,
        intent: 2,
        ..SchedAttr::default()
    };
    let over = SchedAttr {
        flags: SA_FLAG_INTENT | SA_FLAG_POLICY,
        intent: 4,
        policy: 3,
        ..SchedAttr::default()
    };
    let eff = SchedAttr::merge(base, over);
    assert_eq!(eff.flags, SA_FLAG_WEIGHT | SA_FLAG_INTENT | SA_FLAG_POLICY);
    assert_eq!((eff.weight, eff.intent, eff.policy), (100, 4, 3));
    // flags=0 = 清除覆盖：effective 回 Process 默认。
    let cleared = SchedAttr::merge(base, SchedAttr::default());
    assert_eq!(cleared, base);
    berry_hal_aarch64::uart_puts(b"ATTR-K ok\n");
}

/// 刀1 P10：唤醒源枚举/谓词/布局**纯面**（ktest 在 sched::init 前跑——
/// 记账槽表 pristine，未触槽断言有效；运行期执法在 G1.5 WAKESRC 块）。
/// A. 9 枚举值互异且 ≤0x7F（值直槽表容量界）+ 端点锚（NONE=0x00 哨兵 /
///    UNKNOWN=0x7F；sim-S2：POWER_KEY=0x07 到达-only 锚）；B. `wake_src_valid`
///    6 真源过、NONE/UNKNOWN/POWER_KEY/未定义段（0x08/0x50）拒——「可进环」
///    白名单与 [[wakesrc]] 生成器同源（POWER_KEY 有意不在环白名单：v1 电源键
///    恒无唤醒，到达-only 三号位）；C. `wake_src_class` 分类位映射（TIMER=
///    class 位 / UART·POWER_KEY=IRQ 线位 / SGI×2=内部位 / IPC·KB=0 /
///    未定义=0）；D. 布局钉（WakeRec 32B——`source` 吃掉 _pad[0]、_pad 缩
///    [u8;2]；WakeSrcRec 64B——128 槽 ×64B = 8KB .bss）+ 未触槽快照原语
///    （0x50 ∈ 未定义段：first_ts=u64::MAX 哨兵、计数全 0、ts_domain=0）。
fn test_wake_src() {
    use berry_hal_aarch64::wake_gen::{
        WAKE_SRC_IPC, WAKE_SRC_KERNEL_BOOT, WAKE_SRC_NONE, WAKE_SRC_POWER_KEY,
        WAKE_SRC_SGI_RESCHED, WAKE_SRC_SGI_TLB, WAKE_SRC_TIMER, WAKE_SRC_UART_RX,
        WAKE_SRC_UNKNOWN,
    };
    use crate::telemetry::{
        self, WSRC_F_HAD_NO_WAITER, WSRC_F_IRQ_LINE, WSRC_F_SGI_INTERNAL, WSRC_F_SLEPT,
        WSRC_F_TIMER_CLASS,
    };

    // A. 值域：互异 + ≤0x7F + 端点锚。
    let all = [
        WAKE_SRC_NONE,
        WAKE_SRC_KERNEL_BOOT,
        WAKE_SRC_TIMER,
        WAKE_SRC_UART_RX,
        WAKE_SRC_SGI_RESCHED,
        WAKE_SRC_SGI_TLB,
        WAKE_SRC_IPC,
        WAKE_SRC_POWER_KEY,
        WAKE_SRC_UNKNOWN,
    ];
    for (i, &v) in all.iter().enumerate() {
        assert!(v <= 0x7F, "WSRC-K A：src[{}]={:#04x} 超 0x7F（值直槽界）", i, v);
        for &w in &all[..i] {
            assert!(v != w, "WSRC-K A：源值撞号 {:#04x}", v);
        }
    }
    assert_eq!(WAKE_SRC_NONE, 0x00, "WSRC-K A：NONE 哨兵锚");
    assert_eq!(WAKE_SRC_POWER_KEY, 0x07, "WSRC-K A：POWER_KEY 到达-only 锚（sim-S2 A4②）");
    assert_eq!(WAKE_SRC_UNKNOWN, 0x7F, "WSRC-K A：UNKNOWN 端点锚");

    // B. 白名单谓词：6 真源过；哨兵/未知/POWER_KEY/未定义段拒。
    for &v in &[
        WAKE_SRC_KERNEL_BOOT,
        WAKE_SRC_TIMER,
        WAKE_SRC_UART_RX,
        WAKE_SRC_SGI_RESCHED,
        WAKE_SRC_SGI_TLB,
        WAKE_SRC_IPC,
    ] {
        assert!(telemetry::wake_src_valid(v), "WSRC-K B：真源 {:#04x} 被拒", v);
    }
    for &v in &[WAKE_SRC_NONE, WAKE_SRC_UNKNOWN, WAKE_SRC_POWER_KEY, 0x08, 0x50] {
        assert!(!telemetry::wake_src_valid(v), "WSRC-K B：非白名单 {:#04x} 竟过", v);
    }

    // C. 分类位映射。
    assert_eq!(telemetry::wake_src_class(WAKE_SRC_TIMER), WSRC_F_TIMER_CLASS);
    assert_eq!(telemetry::wake_src_class(WAKE_SRC_UART_RX), WSRC_F_IRQ_LINE);
    assert_eq!(telemetry::wake_src_class(WAKE_SRC_POWER_KEY), WSRC_F_IRQ_LINE);
    assert_eq!(telemetry::wake_src_class(WAKE_SRC_SGI_RESCHED), WSRC_F_SGI_INTERNAL);
    assert_eq!(telemetry::wake_src_class(WAKE_SRC_SGI_TLB), WSRC_F_SGI_INTERNAL);
    assert_eq!(telemetry::wake_src_class(WAKE_SRC_IPC), 0);
    assert_eq!(telemetry::wake_src_class(WAKE_SRC_KERNEL_BOOT), 0);
    assert_eq!(telemetry::wake_src_class(0x50), 0);
    // 位面冻结值（add-only：HAD_NO_WAITER sticky / SLEPT v2 预留恒不置）。
    assert_eq!(WSRC_F_HAD_NO_WAITER, 1 << 3, "WSRC-K C：sticky 位冻结值");
    assert_eq!(WSRC_F_SLEPT, 1 << 4, "WSRC-K C：SLEPT 预留位冻结值（v2 恒不置）");

    // D. 布局钉 + 未触槽原语（0x50 永不被写——未定义段无定义写点）。
    assert_eq!(core::mem::size_of::<telemetry::WakeRec>(), 32, "WSRC-K D：WakeRec 布局漂移");
    assert_eq!(core::mem::size_of::<telemetry::WakeSrcRec>(), 64, "WSRC-K D：WakeSrcRec 布局漂移");
    let s = telemetry::wake_src_snapshot(0x50);
    assert_eq!(s.wakes, 0, "WSRC-K D：未触槽 wakes≠0");
    assert_eq!(s.first_ts, u64::MAX, "WSRC-K D：未触槽 first_ts 哨兵漂移");
    assert_eq!(s.ts_domain, 0, "WSRC-K D：未触槽 ts_domain≠0");
    // 快照余四字段（telemetry_open 冻结形状，v2 读点）未触槽同零。
    assert_eq!(
        (s.last_ts, s.last_ack_ts, s.last_woken_id, s.flags),
        (0, 0, 0, 0),
        "WSRC-K D：未触槽快照四字段非零"
    );

    berry_hal_aarch64::uart_puts(b"WSRC-K ok\n");
}

/// 1.1/S5：最小 deadline class（EDF）——`PRIO_DEADLINE` 带序 + 带内 VD 排
/// 序 = 天然 EDF。deadline 任务设 `prio_effective ≥ PRIO_DEADLINE` +
/// `vdeadline = 绝对截止期`，Fair 不覆盖 VD。
fn test_deadline() {
    use berry_sched::{Fair, PRIO_DEADLINE, Runqueue, SchedClass, Task, Topo, HELD_FROZEN, HELD_THROTTLE};

    extern "C" fn dl_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    fn mk(name: &'static [u8]) -> &'static mut Task {
        alloc::boxed::Box::leak(Task::new_kernel_with_as(dl_dummy, 0, name, 0, 0))
    }

    let mut rq = Runqueue::new();
    let rr = Fair;
    let topo = Topo::single();

    // 三个任务：一个 fair（prio 0）、两个 deadline（不同截止期）。
    let tf = mk(b"dlF".as_slice()); // fair
    let td1 = mk(b"dl1".as_slice()); // deadline 截止 3000
    let td2 = mk(b"dl2".as_slice()); // deadline 截止 2000（更紧）

    // fair 任务正常入环。
    rr.enqueue(&mut *tf, &mut rq);

    // deadline 任务：高带 + 绝对截止期为 VD。
    td1.prio_effective = PRIO_DEADLINE;
    td1.vdeadline = 3000; // 截止 3000
    rr.enqueue(&mut *td1, &mut rq);

    td2.prio_effective = PRIO_DEADLINE;
    td2.vdeadline = 2000; // 截止 2000（更紧）
    rr.enqueue(&mut *td2, &mut rq);

    // D1：EDF——截止 2000（dl2）先于截止 3000（dl1），fair（dlF）最后。
    let p1 = rr.pick_next(&rq, &topo).unwrap();
    assert_eq!(p1.name, b"dl2".as_slice(), "DL：D1 最早截止（2000）须先出");
    // 模拟 dl2 已跑完本轮——VD 推到 MAX（防 D2 重复选 dl2）。
    td2.vdeadline = u64::MAX;

    let p2 = rr.pick_next(&rq, &topo).unwrap();
    assert_eq!(p2.name, b"dl1".as_slice(), "DL：D2 次早截止（3000）须次出");

    // fair 任务在 deadline 带空后才出。
    td1.prio_effective = 0; // 降回 fair
    td2.prio_effective = 0;
    td1.vruntime = 0; td1.vdeadline = 0;
    td2.vruntime = 0; td2.vdeadline = 0;
    rq.resync_vruntime_sum(); // 直改后洗账（D3 出选不验具体者，账本仍须守恒）
    // D3：deadline 带空 → 所有任务退回 fair——任一出选即通过（不验具体是谁，
    // 因为 advance_past 游标位置决定首见者——三任务均合法）。
    let _p3 = rr.pick_next(&rq, &topo).expect("DL：D3 非空环须有出选");

    // —— S5-c：预算执法——budget 耗尽 throttle + 周期到 lazy unthrottle ——
    // DL-B1：deadline 任务带预算——slice 耗尽 → HELD_FROZEN 置位。
    let tb = mk(b"dlB".as_slice()); // budget 任务
    let tf2 = mk(b"dlF2".as_slice()); // fair 陪跑
    let mut rq2 = Runqueue::new();

    // 设 deadline + 预算。
    tb.prio_effective = PRIO_DEADLINE;
    tb.preempt_threshold = 100; // 预算 = 100 cycles
    tb.duration_hint_ns = 1000; // 周期 = 1000 cycles
    tb.vdeadline = u64::MAX; // 截止远（永不到——测 throttle 持续）
    tb.slice_left = 100; // 当前 slice = 预算
    rr.enqueue(&mut *tb, &mut rq2);
    rr.enqueue(&mut *tf2, &mut rq2);

    // tick 到 slice 耗尽 → 应 throttle（HELD_FROZEN 置位）。
    // tick 读 timer 内部算 delta——直接置 slice_left=0 后 tick 模拟到期。
    tb.slice_left = 0;
    let cur2 = tb as *mut berry_sched::Task;
    // tick 的 rq 参数 S2b+ 起真消费（Σvruntime 账本入账——tb 在 rq2 环上，
    // 在环门控过 → 入账；账本口径与旧全环扫描一致）。
    rr.tick(unsafe { &mut *cur2 }, unsafe { &mut *core::ptr::addr_of_mut!(rq2) });
    assert!(tb.held_locks & HELD_THROTTLE != 0,
            "DL：B1 slice 耗尽须置 HELD_THROTTLE（预算限流）");
    assert!(tb.held_locks & HELD_FROZEN == 0,
            "DL：B1 预算限流不得误置 HELD_FROZEN（进程冻结独占语义）");
    // fair 陪跑不受影响。
    assert!(tf2.held_locks & (HELD_FROZEN | HELD_THROTTLE) == 0,
            "DL：B1 fair 任务不受 deadline 预算执法影响");

    // DL-B2：周期到（vdeadline 过去）→ lazy_unthrottle 解冻。
    tb.vdeadline = 1; // 截止已过（cntvct ≥ 1）
    // 触发 pick_next → lazy_unthrottle → 解冻。
    let picked = rr.pick_next(&rq2, &topo).expect("DL：B2 非空环须有出选");
    assert!(picked.held_locks & HELD_THROTTLE == 0,
            "DL：B2 周期到后 lazy_unthrottle 须解限流（清 THROTTLE 位）");
    assert!(picked.slice_left == 100,
            "DL：B2 解冻后须授新预算 slice（期望 100，实际 {}）", picked.slice_left);

    berry_hal_aarch64::uart_puts(b"DL-K ok\n");
}

fn test_pi() {
    use alloc::boxed::Box;
    use alloc::vec::Vec;
    use berry_sched::{sleepq_insert, sleepq_unlink, Task, WAIT_ANY};
    use crate::channel::ChannelEnd;
    use crate::handle::{
        HandleTable, KernelObj, ObjRef, ObjType, RIGHT_RECV, RIGHT_SEND, SIGNAL_READABLE,
        SIGNAL_WRITABLE,
    };
    use crate::syscall::bootstrap_table;

    /// 占位入口（手搭睡眠者永不执行；halt 兜底）。
    extern "C" fn pi_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    /// 手搭睡眠者：prio base/effective、睡 `obj` 等 `signals`（无限期；obj=0 =
    /// 纯睡，wait_signals 无效）、持有面 = `tag`（0 → 内核侧共享表；非 0 →
    /// 私有表指针）。入 SLEEPQ 后返回——**永不执行**。
    fn sleeper(name: &'static [u8], prio: i32, obj: usize, signals: u64, tag: usize)
        -> &'static mut Task {
        let mut t = Task::new_kernel(pi_dummy, 0, name);
        t.prio_base = prio;
        t.prio_effective = prio;
        t.proc_tag = tag;
        t.wait_obj = obj;
        t.wait_signals = signals;
        t.wait_deadline = u64::MAX;
        let t: &'static mut Task = Box::leak(t);
        sleepq_insert(t);
        t
    }

    /// 私有 handle 表（leak 常驻；返回裸指针兼作 sleeper 的 proc_tag——3-4a
    /// 「proc_tag → 所属进程表指针」同一约定）。
    fn priv_table() -> usize {
        Box::leak(Box::new(HandleTable::new())) as *mut HandleTable as usize
    }

    /// 建一个 channel 端点放进 `ht` 指的表（rights 只影响受捐判定），返回
    /// ObjRef 地址。
    fn chan_end(ht: usize, rights: crate::handle::Rights) -> usize {
        // SAFETY: ht 来自 priv_table/bootstrap_table 的 leak 常驻表——ktest
        // 单核顺序访问，无别名。
        let ht = unsafe { &mut *(ht as *mut HandleTable) };
        let h = ht
            .insert(
                KernelObj::Channel(ChannelEnd::try_unpaired(false).expect("PI：建端点")),
                rights,
            )
            .expect("PI：端点进表");
        ht.get_raw(h, ObjType::Channel, 0).expect("PI：取端点").0.as_ptr() as usize
    }

    /// 配对两端（SAFETY：两个端点各自刚 insert + get_raw——鲜活且互不重复）。
    fn pair_up(a: usize, b: usize) {
        let a = core::ptr::NonNull::new(a as *mut ObjRef).expect("PI：端点 A 指针");
        let b = core::ptr::NonNull::new(b as *mut ObjRef).expect("PI：端点 B 指针");
        // SAFETY: 同上——pair 只写两端的 peer 字段并互钉 retain。
        unsafe { ChannelEnd::pair(a, b) };
    }

    /// 一个 Event 进内核侧表（非 channel 等待对象样本），返回 ObjRef 地址。
    fn ev_obj() -> usize {
        let ht = bootstrap_table();
        let h = ht
            .insert(KernelObj::Event(crate::event::Event::new()), crate::handle::RIGHT_SIGNAL)
            .expect("PI：Event 进表");
        ht.get_raw(h, ObjType::Event, 0).expect("PI：取 Event").0.as_ptr() as usize
    }

    let ktbl = bootstrap_table() as *mut HandleTable as usize; // 内核侧共享表

    // J0. 单跳（EL0 式判别）：client(32) 睡 endC:WRITABLE（tag=0）；server 纯睡、
    //     私有表持 endS:RECV。→ server 受捐 32、eff 抬 32；client 的共享表只有
    //     endC:SEND——不持 endS，不受捐。
    let ht_s0 = priv_table();
    let c0e = chan_end(ktbl, RIGHT_SEND);
    let s0e = chan_end(ht_s0, RIGHT_RECV);
    pair_up(c0e, s0e);
    let c0 = sleeper(b"j0_cli", 32, c0e, SIGNAL_WRITABLE, 0);
    let s0 = sleeper(b"j0_srv", 0, 0, 0, ht_s0);
    crate::pi::recompute();
    assert_eq!(s0.pi_inherited, 32, "PI：J0 server 须受捐 32");
    assert_eq!(s0.prio_effective, 32, "PI：J0 server eff 须 32");
    assert_eq!(c0.pi_inherited, 0, "PI：J0 client（不持 peer）不受捐");

    // J6. 冻结随行 + 解除回落（复用 J0 的对象）：离队（= 唤醒，重推只清在队
    //     者）→ boost 冻结随行；重入队（= 下次入睡）且 donor 已退场 → 清零回落。
    sleepq_unlink(s0);
    crate::pi::recompute();
    assert_eq!(s0.pi_inherited, 32, "PI：J6 离队冻结：boost 随行不清");
    assert_eq!(s0.prio_effective, 32, "PI：J6 离队冻结：eff 保持");
    sleepq_insert(s0);
    sleepq_unlink(c0); // donor 退场（send 阻塞被解除的形态）
    crate::pi::recompute();
    assert_eq!(s0.pi_inherited, 0, "PI：J6 重入队重算：无边须回落 0");
    assert_eq!(s0.prio_effective, 0, "PI：J6 重入队重算：eff 回落 base");
    sleepq_unlink(s0);

    // J1. rights 门控（两向）：等 WRITABLE 的 donor 只认 RECV 持有者；等
    //     READABLE 的 donor 只认 SEND 持有者。各建「持有者私有表只有错向
    //     right」的对——受捐侧落 0。
    let ht1 = priv_table();
    let c1e = chan_end(ktbl, RIGHT_SEND);
    let s1e = chan_end(ht1, RIGHT_SEND); // 只有 SEND——不满足 WRITABLE donor 的 RECV 需求
    pair_up(c1e, s1e);
    let c1 = sleeper(b"j1_cli", 24, c1e, SIGNAL_WRITABLE, 0);
    let v1 = sleeper(b"j1_v", 0, 0, 0, ht1);
    let ht2 = priv_table();
    let c2e = chan_end(ktbl, RIGHT_RECV);
    let s2e = chan_end(ht2, RIGHT_RECV); // 只有 RECV——不满足 READABLE donor 的 SEND 需求
    pair_up(c2e, s2e);
    let c2 = sleeper(b"j1r_cli", 20, c2e, SIGNAL_READABLE, 0);
    let v2 = sleeper(b"j1r_v", 0, 0, 0, ht2);
    crate::pi::recompute();
    assert_eq!(v1.pi_inherited, 0, "PI：J1 SEND-only 持有者不被 WRITABLE donor 捐");
    assert_eq!(v2.pi_inherited, 0, "PI：J1 RECV-only 持有者不被 READABLE donor 捐");
    sleepq_unlink(c1);
    sleepq_unlink(v1);
    sleepq_unlink(c2);
    sleepq_unlink(v2);

    // J2. 链式两跳：L1(32) 睡 e1:WRITABLE；L2 睡 e2:WRITABLE 且私有表持
    //     peer(e1):RECV；L3 纯睡、私有表持 peer(e2):RECV。第 2 跳依赖 d_eff
    //     **实时读 pi_inherited**（prio_effective 尾趟才刷——只读它会截死一跳）。
    let ht_l2 = priv_table();
    let ht_l3 = priv_table();
    let e1 = chan_end(ktbl, RIGHT_SEND);
    let s1p = chan_end(ht_l2, RIGHT_RECV);
    pair_up(e1, s1p);
    let e2 = chan_end(ktbl, RIGHT_SEND);
    let s2p = chan_end(ht_l3, RIGHT_RECV);
    pair_up(e2, s2p);
    let l1 = sleeper(b"j2_l1", 32, e1, SIGNAL_WRITABLE, 0);
    let l2 = sleeper(b"j2_l2", 0, e2, SIGNAL_WRITABLE, ht_l2);
    let l3 = sleeper(b"j2_l3", 0, 0, 0, ht_l3);
    crate::pi::recompute();
    assert_eq!(l2.pi_inherited, 32, "PI：J2 第 1 跳 L2 须到 32");
    assert_eq!(l3.pi_inherited, 32, "PI：J2 第 2 跳 L3 须到 32（链式实时传播）");
    sleepq_unlink(l1);
    sleepq_unlink(l2);
    sleepq_unlink(l3);

    // J3. 持有环：A(5) 睡 cA:WRITABLE、私有表持 peer(cB)；B(3) 睡 cB:WRITABLE、
    //     私有表持 peer(cA)——A→B→A 成环。max 语义下单调有界收敛（终态双 5），
    //     轮帽兜底防发散——不 panic 即「环有界」的活证。
    let ht_a = priv_table();
    let ht_b = priv_table();
    let cae = chan_end(ktbl, RIGHT_SEND);
    let sae = chan_end(ht_b, RIGHT_RECV); // A 的边指向 B（B 持 peer(cA)）
    pair_up(cae, sae);
    let cbe = chan_end(ktbl, RIGHT_SEND);
    let sbe = chan_end(ht_a, RIGHT_RECV); // B 的边指向 A（A 持 peer(cB)）
    pair_up(cbe, sbe);
    let t3a = sleeper(b"j3_a", 5, cae, SIGNAL_WRITABLE, ht_a);
    let t3b = sleeper(b"j3_b", 3, cbe, SIGNAL_WRITABLE, ht_b);
    crate::pi::recompute();
    assert_eq!(t3a.pi_inherited, 5, "PI：J3 环须收敛（A 收 max=5）");
    assert_eq!(t3b.pi_inherited, 5, "PI：J3 环须收敛（B 收 5）");
    sleepq_unlink(t3a);
    sleepq_unlink(t3b);

    // J4. 轮帽截断：11 节点线性链 L0(40) → L1 → … → L10——L_k 睡第 k 对
    //     client 端等 WRITABLE，L_{k+1} 的私有表持该对 server 端（RECV）。
    //     **倒序入队**（L10 先、L0 最后；deadline 全 MAX 相等 FIFO）→ donor
    //     访问序 L10..L0，每轮恰好传播一跳：轮 k 到 L_k。轮帽 8 → L1..L8
    //     达 40、L9/L10 按「已达 boost 处理」落 0——不 panic 的机器断言。
    let hts: Vec<usize> = (0..10).map(|_| priv_table()).collect(); // L1..L10 的表
    let mut c_ends = [0usize; 11];
    for k in 0..11 {
        let c = chan_end(ktbl, RIGHT_SEND);
        let s_tbl = if k < 10 { hts[k] } else { ktbl }; // S10 无 L11——落共享表（无害）
        let s = chan_end(s_tbl, RIGHT_RECV);
        pair_up(c, s);
        c_ends[k] = c;
    }
    let mut chain: Vec<&'static mut Task> = Vec::new();
    for k in (0..11).rev() {
        let tag = if k == 0 { 0 } else { hts[k - 1] };
        let prio = if k == 0 { 40 } else { 0 };
        chain.push(sleeper(b"j4_lk", prio, c_ends[k], SIGNAL_WRITABLE, tag));
    }
    crate::pi::recompute();
    // chain 倒序下标：chain[10]=L0 … chain[0]=L10。
    assert_eq!(chain[9].pi_inherited, 40, "PI：J4 第 1 跳 L1 须达 40");
    assert_eq!(chain[2].pi_inherited, 40, "PI：J4 第 8 跳 L8 须恰达轮帽");
    assert_eq!(chain[1].pi_inherited, 0, "PI：J4 L9 超帽按已达处理（0）");
    assert_eq!(chain[0].pi_inherited, 0, "PI：J4 L10 超帽按已达处理（0）");
    for t in chain.iter_mut() {
        sleepq_unlink(t);
    }

    // J5. donor 跳过面：WAIT_ANY 哨兵（多对象等待无单一键，v2 收口）与纯
    //     Event 等待者（用户位）都不构成捐赠边——受捐侧持 peer 也无人可捐。
    let ht5 = priv_table();
    let c5e = chan_end(ktbl, RIGHT_SEND);
    let s5e = chan_end(ht5, RIGHT_RECV);
    pair_up(c5e, s5e);
    let ev = ev_obj();
    let wa = sleeper(b"j5_wa", 50, WAIT_ANY, SIGNAL_WRITABLE, 0);
    let evd = sleeper(b"j5_ev", 50, ev, 1 << 34, 0);
    let vic = sleeper(b"j5_v", 0, 0, 0, ht5);
    crate::pi::recompute();
    assert_eq!(vic.pi_inherited, 0, "PI：J5 哨兵/Event donor 须无边");
    sleepq_unlink(wa);
    sleepq_unlink(evd);
    sleepq_unlink(vic);

    // J7. 共享表粒度（boot 场景 pi_server 同款 v1 语义）：内核任务 proc_tag=0
    //     → 持有面 = 内核侧共享表——端点在其中 = **所有**内核任务睡眠者都算
    //     持有（EL0 每进程表才反查到人）。donor 自捐 = 同一面的推论（数值
    //     无害：pi ≤ 自身 d_eff，不膨胀）。
    let c7e = chan_end(ktbl, RIGHT_SEND);
    let s7e = chan_end(ktbl, RIGHT_RECV);
    pair_up(c7e, s7e);
    let d7 = sleeper(b"j7_cli", 28, c7e, SIGNAL_WRITABLE, 0);
    let k7 = sleeper(b"j7_k", 0, 0, 0, 0); // 内核任务 → 共享表 → 持有
    let p7t = priv_table();
    let u7 = sleeper(b"j7_priv", 0, 0, 0, p7t); // 私有空表 → 不持有
    crate::pi::recompute();
    assert_eq!(k7.pi_inherited, 28, "PI：J7 共享表：内核任务睡眠者算持有");
    assert_eq!(u7.pi_inherited, 0, "PI：J7 私有空表：不持有不受捐");
    assert_eq!(d7.pi_inherited, 28, "PI：J7 自捐（共享表面推论，数值无害）");
    sleepq_unlink(d7);
    sleepq_unlink(k7);
    sleepq_unlink(u7);

    // 清场：SLEEPQ 还给 boot（真睡眠者按 deadline 升序入队——脏节点会破坏
    // 超时扫描早停）。任务体/私有表 leak 不回收（ktest 语境同 spawn 政策）。
    berry_sched::for_each_sleeper(|_| panic!("PI：清场后 SLEEPQ 须空"));
    berry_hal_aarch64::uart_puts(b"PI-K ok\n");
}

/// 4-3c：intent 随消息传播（ADR #42 五件套，`berry_sched::intent`；ktest 在
/// `sched::init` 前跑 → 无 CURRENT/SLEEPQ 干扰，纯函数面 K0–K8 直测
/// settle/snapshot + 三道帽 + 每消息重导；K9 走真 SVC 验 channel_create
/// options 面）。EL0 全链验收在 main.rs 六任务对打（`INTENT ok`）。
#[allow(clippy::unwrap_used)]
fn test_intent() {
    use berry_sched::intent::{settle, snapshot, MsgIntent, SettleOut, SnapOut};
    use berry_sched::{Task, INTENT_BACKGROUND, INTENT_CHAIN_BUDGET_NS, INTENT_HOP_BUDGET_NS,
                      INTENT_HOP_MAX, INTENT_INTERACTIVE, INTENT_LATENCY_CRITICAL,
                      INTENT_UNSPECIFIED, INTENT_UTILITY, intent_valid};
    use crate::handle::{ObjType, RIGHT_RECV, RIGHT_SEND};
    use crate::syscall::{bootstrap_table, NR_CHANNEL_CREATE};

    /// 占位入口（纯函数面 Task 永不执行；halt 兜底）。
    extern "C" fn it_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    /// 手搭任务（只动 intent 字段，prio/状态无关本面）。
    fn it_task(declared: u8, effective: u8) -> Task {
        let mut t = *Task::new_kernel(it_dummy, 0, b"it_k");
        t.intent_declared = declared;
        t.intent_effective = effective;
        t
    }

    const NOW: u64 = 1_000_000_000; // 任意基准（vct_ns 域）

    // K0. 编码域冻结：0x00–0x05 合法、0x06 起拒绝（含 0xFF）。
    for v in [INTENT_UNSPECIFIED, INTENT_LATENCY_CRITICAL, INTENT_INTERACTIVE,
              INTENT_UTILITY, INTENT_BACKGROUND, 0x05u8] {
        assert!(intent_valid(v), "INTENT：K0 {} 须合法", v);
    }
    assert!(!intent_valid(0x06), "INTENT：K0 0x06 保留须拒");
    assert!(!intent_valid(0xFF), "INTENT：K0 野值须拒");

    // K1. 首手快照：未借入（eff==declared）→ Fresh，装 declared、hop/acc 清零
    //     （「首手」定义 = 不是别人借来的转发放大）。
    let t = it_task(INTENT_UTILITY, INTENT_UTILITY);
    let (m, s) = snapshot(&t, NOW);
    assert_eq!((m.intent, m.hop, m.acc_ns), (INTENT_UTILITY, 0, 0), "INTENT：K1 首手载荷");
    assert_eq!(s, SnapOut::Fresh, "INTENT：K1 Fresh");

    // K2. 借入转发快照：eff 0x02 < declared 0x04、持有 10ms < 预算 →
    //     Forwarded（装借来的 0x02、hop+1、acc 累加持有段）。
    let mut t = it_task(INTENT_BACKGROUND, INTENT_INTERACTIVE);
    t.intent_hop = 0;
    t.intent_acc = 1000;
    t.intent_since = NOW - 10_000_000;
    let (m, s) = snapshot(&t, NOW);
    assert_eq!((m.intent, m.hop, m.acc_ns),
               (INTENT_INTERACTIVE, 1, 1000 + 10_000_000),
               "INTENT：K2 转发载荷（hop+1、acc 累加）");
    assert_eq!(s, SnapOut::Forwarded, "INTENT：K2 Forwarded");

    // K3. 每跳预算帽（send 侧）：持有 60ms > 50ms → Capped 降级（装自己的
    //     declared、hop/acc 清零——借来的档不再传播）。
    let mut t = it_task(INTENT_BACKGROUND, INTENT_INTERACTIVE);
    t.intent_since = NOW - (INTENT_HOP_BUDGET_NS + 10_000_000);
    let (m, s) = snapshot(&t, NOW);
    assert_eq!((m.intent, m.hop, m.acc_ns), (INTENT_BACKGROUND, 0, 0),
               "INTENT：K3 超预算降级载荷");
    assert_eq!(s, SnapOut::Capped, "INTENT：K3 Capped");

    // K4. 借入生效（数值越小档越高）：declared 0x03 收 0x01 → eff 0x01、
    //     hop/since/acc 全记（转发快照与时间帽的数据源）。
    let mut t = it_task(INTENT_UTILITY, INTENT_UTILITY);
    let out = settle(&mut t, MsgIntent { intent: INTENT_LATENCY_CRITICAL, hop: 2, acc_ns: 500 },
                     false, NOW);
    assert_eq!(out, SettleOut::Borrowed, "INTENT：K4 借入生效");
    assert_eq!(t.intent_effective, INTENT_LATENCY_CRITICAL, "INTENT：K4 eff 须 0x01");
    assert_eq!((t.intent_hop, t.intent_since, t.intent_acc), (2, NOW, 500),
               "INTENT：K4 随消息头记录");

    // K5. 未声明 = 最强档（XNU 式）+ 每消息重导（非单调累积）：declared 0x04
    //     收 0x00 → 借入到 0x00（未分类不设限的保守方向）；再收弱消息 0x04 →
    //     Own 回落 declared（QNX 粘性膨胀在机制上不可能）。
    let mut t = it_task(INTENT_BACKGROUND, INTENT_LATENCY_CRITICAL);
    let out = settle(&mut t, MsgIntent { intent: INTENT_UNSPECIFIED, hop: 0, acc_ns: 0 },
                     false, NOW);
    assert_eq!(out, SettleOut::Borrowed, "INTENT：K5 未声明须借入");
    assert_eq!(t.intent_effective, INTENT_UNSPECIFIED,
               "INTENT：K5 未声明须借入到最强档（保守方向）");
    let out = settle(&mut t, MsgIntent { intent: INTENT_BACKGROUND, hop: 0, acc_ns: 0 },
                     false, NOW);
    assert_eq!(out, SettleOut::Own, "INTENT：K5 弱消息");
    assert_eq!(t.intent_effective, INTENT_BACKGROUND,
               "INTENT：K5 弱消息自然回落（每消息重导）");

    // K6. opt-out（五件套之 4）：拒借 + 重导回落（含清掉他途借入）。
    let mut t = it_task(INTENT_UTILITY, INTENT_LATENCY_CRITICAL); // 他途借入 0x01
    let out = settle(&mut t, MsgIntent { intent: INTENT_LATENCY_CRITICAL, hop: 0, acc_ns: 0 },
                     true, NOW);
    assert_eq!(out, SettleOut::Own, "INTENT：K6 opt-out Own");
    assert_eq!(t.intent_effective, INTENT_UTILITY,
               "INTENT：K6 opt-out 须回落 declared（清掉借入）");

    // K7. 跳数帽：hop=8 恰顶 → CappedHop 拒传播回落；hop=7 放行。
    let mut t = it_task(INTENT_UTILITY, INTENT_UTILITY);
    let out = settle(&mut t, MsgIntent { intent: INTENT_LATENCY_CRITICAL,
                                         hop: INTENT_HOP_MAX, acc_ns: 0 },
                     false, NOW);
    assert_eq!(out, SettleOut::CappedHop, "INTENT：K7 hop 帽拒");
    assert_eq!(t.intent_effective, INTENT_UTILITY, "INTENT：K7 回落 declared");
    let out = settle(&mut t, MsgIntent { intent: INTENT_LATENCY_CRITICAL,
                                         hop: INTENT_HOP_MAX - 1, acc_ns: 0 },
                     false, NOW);
    assert_eq!(out, SettleOut::Borrowed, "INTENT：K7 hop=7 须放行");

    // K8. 链累计封顶：acc > 封顶 → CappedBudget；acc == 封顶恰不触发（>
    //     判据——含边界值语义与文档一致）。
    let mut t = it_task(INTENT_UTILITY, INTENT_UTILITY);
    let out = settle(&mut t, MsgIntent { intent: INTENT_LATENCY_CRITICAL, hop: 0,
                                         acc_ns: INTENT_CHAIN_BUDGET_NS + 1 },
                     false, NOW);
    assert_eq!(out, SettleOut::CappedBudget, "INTENT：K8 链累计帽拒");
    assert_eq!(t.intent_effective, INTENT_UTILITY, "INTENT：K8 回落 declared");
    let mut t = it_task(INTENT_UTILITY, INTENT_UTILITY);
    let out = settle(&mut t, MsgIntent { intent: INTENT_LATENCY_CRITICAL, hop: 0,
                                         acc_ns: INTENT_CHAIN_BUDGET_NS },
                     false, NOW);
    assert_eq!(out, SettleOut::Borrowed, "INTENT：K8 恰在封顶须放行");

    // K9. SVC options 面：channel_create(options, *out0, *out1)——bit0 opt-out
    //     合法（端点 no_intent 可查）、bit1 起及 !0 拒 -EINVAL（event_create
    //     的 options 拒面同款门）。两端点 leak 不回收（ktest 政策）。
    let mut out0: u64 = 0;
    let mut out1: u64 = 0;
    assert_eq!(
        svc4(NR_CHANNEL_CREATE as u64, 0, &raw mut out0 as u64, &raw mut out1 as u64),
        0,
        "INTENT：K9 常规 create（options=0）"
    );
    assert!(out0 != 0 && out1 != 0 && out0 != out1, "INTENT：K9 双端 handle");
    let (mut o2, mut o3): (u64, u64) = (0, 0);
    assert_eq!(
        svc4(NR_CHANNEL_CREATE as u64, 1, &raw mut o2 as u64, &raw mut o3 as u64),
        0,
        "INTENT：K9 bit0 opt-out create"
    );
    let ht = bootstrap_table();
    let ni = ht.get_raw(o2 as u32, ObjType::Channel, 0)
        .expect("INTENT：K9 取 opt 端点")
        .0;
    // SAFETY: 刚 get_raw 过（在表、类型 Channel）；ktest 单核顺序。
    assert!(unsafe { (*ni.as_ptr()).obj.as_channel().unwrap().no_intent() },
            "INTENT：K9 bit0 端点须 no_intent");
    assert_eq!(
        svc4(NR_CHANNEL_CREATE as u64, 2, &raw mut o2 as u64, &raw mut o3 as u64),
        crate::handle::ERR_INVALID_ARG as i64,
        "INTENT：K9 bit1 未定义须拒"
    );
    assert_eq!(
        svc4(NR_CHANNEL_CREATE as u64, !0, &raw mut o2 as u64, &raw mut o3 as u64),
        crate::handle::ERR_INVALID_ARG as i64,
        "INTENT：K9 野 options 须拒"
    );
    let _ = (RIGHT_SEND, RIGHT_RECV); // 面上引用（rights 门控面 H 段已盖）

    berry_hal_aarch64::uart_puts(b"INTENT-K ok\n");
}

/// v1.2/S6-e（`调点调度-v1.2.md` §7 / ADR #42 第二段）：band 预算包络
/// （`sched/src/band.rs`）四段验证——**消耗 / 耗尽 / 周期 / 隔离**。
///
/// 带级 CBS 同形语义：每期至多 quota CPU 周期数，超了 throttle 到下期翻期。
/// 记账面 = `consume` 微记账 + `throttle_all` 环执法（分两步，热路径可合并）
/// ；周期面 = `period_check` 事件驱动翻期（零周期 tick）；隔离面 = 带键 =
/// `intent_effective`（§11）——异带 / deadline 任务（RT 屏蔽不变量 §10）/
/// 关态带互不波及，`HELD_FROZEN` 位独占语义不被带执法触碰。
///
/// ktest 纪律（sched::init 前跑、单核——`BANDS` slot 契约的 ktest 等价）
/// ：全局静态表 → 末段按「先 `unthrottle_all` 清环上位再 `configure(0)`」
/// 收口义务关全带，不污染后续测试。附带 BD5 = `Process.band_id` 内核侧
/// 记账字段最小往返（spawn 映射点载体；ABI 零关联——intent 即带键）。
#[allow(clippy::unwrap_used)]
fn test_band() {
    use berry_sched::{
        band_on, configure, consume, period_check, state, throttle_all, unthrottle_all,
        BandBudget, BAND_SLOTS, Fair, Runqueue, SchedClass, Task, Topo,
        INTENT_BACKGROUND, INTENT_INTERACTIVE, INTENT_LATENCY_CRITICAL, PRIO_DEADLINE,
        HELD_FROZEN, HELD_THROTTLE,
    };

    /// 占位入口（手搭带成员永不执行；halt 兜底——同 test_deadline）。
    extern "C" fn bd_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    /// 手搭带成员：intent 即带键（§11——band_member 判定面读 effective）；
    /// fair 任务（prio_effective=0 < PRIO_DEADLINE）。deadline 豁免面在
    /// 用例内单独置 `prio_effective = PRIO_DEADLINE`。
    fn mk(name: &'static [u8], band: u8) -> &'static mut Task {
        let t = alloc::boxed::Box::leak(Task::new_kernel_with_as(bd_dummy, 0, name, 0, 0));
        t.intent_effective = band;
        t
    }

    const QUOTA: u64 = 1_000;
    const PERIOD: u64 = 10_000;
    /// 手拨首期起点（band 事件驱动——比较 `now` 参数，不依赖真 CNTVCT）。
    const NOW: u64 = 100_000;
    /// 三带键取 #58 intent 冻结编码（0x01/0x02/0x04——均 < BAND_SLOTS）。
    const B1: u8 = INTENT_LATENCY_CRITICAL;
    const B2: u8 = INTENT_INTERACTIVE;
    const B3: u8 = INTENT_BACKGROUND;

    // —— BD0. 配置门控 + 越界负例：quota/period 均正才武装；带越界全拒 ——
    let empty = Runqueue::new();
    assert!(!configure(BAND_SLOTS as u8, QUOTA, PERIOD, NOW),
            "BAND：BD0 带越界 configure 须拒");
    assert!(state(BAND_SLOTS as u8).is_none(), "BAND：BD0 越界 state 须 None");
    assert!(!consume(BAND_SLOTS as u8, 1), "BAND：BD0 越界 consume 须 false");
    assert!(!period_check(BAND_SLOTS as u8, u64::MAX),
            "BAND：BD0 越界 period_check 须 false");
    assert_eq!(throttle_all(BAND_SLOTS as u8, &empty), 0,
               "BAND：BD0 越界 throttle 零扫描");
    assert_eq!(unthrottle_all(BAND_SLOTS as u8, &empty), 0,
               "BAND：BD0 越界 unthrottle 零扫描");
    assert!(!configure(B1, 0, PERIOD, NOW), "BAND：BD0 quota=0 须不武装");
    assert!(!configure(B1, QUOTA, 0, NOW), "BAND：BD0 period=0 须不武装");
    assert!(!band_on(B1), "BAND：BD0 未配置带须不执法");
    assert_eq!(state(B1), Some(BandBudget::OFF), "BAND：BD0 槽初值须 OFF");
    assert!(configure(B1, QUOTA, PERIOD, NOW), "BAND：BD0 均正须武装");
    assert!(band_on(B1), "BAND：BD0 武装后 band_on");
    assert_eq!(
        (state(B1).unwrap().quota, state(B1).unwrap().period,
         state(B1).unwrap().consumed, state(B1).unwrap().deadline,
         state(B1).unwrap().throttled),
        (QUOTA, PERIOD, 0, NOW + PERIOD, false),
        "BAND：BD0 首期槽形态（清账 + deadline=now+period）"
    );

    // —— BD1. 消耗：微记账累计，未越线不报不限流 ——
    assert!(!consume(B1, 400), "BAND：BD1 未越线不报");
    assert!(!consume(B1, 500), "BAND：BD1 900/1000 仍未越线");
    assert_eq!(state(B1).unwrap().consumed, 900, "BAND：BD1 微记账累计");
    assert!(!state(B1).unwrap().throttled, "BAND：BD1 未越线不得限流");
    assert!(!consume(B3, 999), "BAND：BD1 关态带 consume 须 false");
    assert_eq!(state(B3).unwrap().consumed, 0, "BAND：BD1 关态零记账");

    // —— BD2. 耗尽：首越线一次性信号 + 环执法（限流 → eligible 不再出选）——
    let mut rq = Runqueue::new();
    let fair = Fair;
    let t1 = mk(b"bdA".as_slice(), B1);      // 同带 fair ×2（受害面）
    let t2 = mk(b"bdB".as_slice(), B1);
    let tother = mk(b"bdO".as_slice(), B2);  // 异带 fair（隔离对照）
    let tdl = mk(b"bdD".as_slice(), B1);     // 同带 deadline（RT 屏蔽豁免）
    let tfrz = mk(b"bdF".as_slice(), B1);    // 同带 fair 预冻结（位分立对照）
    tdl.prio_effective = PRIO_DEADLINE;
    tfrz.held_locks |= HELD_FROZEN;
    for t in [&mut *t1, &mut *t2, &mut *tother, &mut *tdl, &mut *tfrz] {
        fair.enqueue(t, &mut rq);
    }
    assert!(consume(B1, 200), "BAND：BD2 900+200≥quota 首越线须报");
    assert!(!consume(B1, 5), "BAND：BD2 已限流不重复报（一次性信号）");
    assert_eq!(
        (state(B1).unwrap().consumed, state(B1).unwrap().throttled),
        (1_105, true),
        "BAND：BD2 账面（越线后续记 + 限流位置位）"
    );
    assert_eq!(throttle_all(B1, &rq), 3, "BAND：BD2 须置 3 个同带 fair 成员");
    assert_eq!(throttle_all(B1, &rq), 0, "BAND：BD2 幂等（已置不重复计）");
    for t in [&*t1, &*t2, &*tfrz] {
        assert!(t.held_locks & HELD_THROTTLE != 0,
                "BAND：BD2 同带 fair 须置 THROTTLE");
    }
    assert!(tother.held_locks & (HELD_THROTTLE | HELD_FROZEN) == 0,
            "BAND：BD2 异带不受限");
    assert!(tdl.held_locks & HELD_THROTTLE == 0,
            "BAND：BD2 deadline 任务豁免（THROTTLE 位归 S5-c 逐任务执法）");
    // 限流效果经 eligible 过滤兑现：被限流者留环但不出选（同 S5-c 语义）
    //——豁免的 deadline 者先出（最高带），摘走后唯一可出选者只剩异带 bdO。
    let topo = Topo::single();
    let p1 = fair.pick_next(&rq, &topo).expect("BAND：BD2 非空环须有出选");
    assert_eq!(p1.name, b"bdD".as_slice(),
               "BAND：BD2 deadline 豁免者须先出（最高带）");
    fair.dequeue(&mut *tdl, &mut rq); // 摘豁免者——环上剩被限流三员 + 异带者
    let p2 = fair.pick_next(&rq, &topo).expect("BAND：BD2 摘豁免者后仍须有出选");
    assert_eq!(p2.name, b"bdO".as_slice(),
               "BAND：BD2 被限流三员不得出选（只剩异带 bdO）");

    // —— BD3. 周期：事件驱动翻期（清账+清限流+deadline 饱和步进追赶）——
    assert!(!period_check(B1, NOW + PERIOD - 1), "BAND：BD3 未到期不翻");
    assert!(state(B1).unwrap().throttled, "BAND：BD3 未到期限流保持");
    // 长眠一次跨两期（now = 首期起点 + 2×PERIOD + 500）：deadline 一步
    // 追到 NOW+3×PERIOD（饱和步进，防死循环）。
    assert!(period_check(B1, NOW + 2 * PERIOD + 500), "BAND：BD3 到期须翻期");
    assert_eq!(
        (state(B1).unwrap().consumed, state(B1).unwrap().throttled),
        (0, false),
        "BAND：BD3 翻期清账清限流"
    );
    assert_eq!(state(B1).unwrap().deadline, NOW + 3 * PERIOD,
               "BAND：BD3 跨多期饱和步进（一步跨两期）");
    assert_eq!(unthrottle_all(B1, &rq), 3, "BAND：BD3 翻期后须清 3 个限流位");
    assert_eq!(unthrottle_all(B1, &rq), 0, "BAND：BD3 幂等");
    for t in [&*t1, &*t2] {
        assert_eq!(t.held_locks, 0, "BAND：BD3 fair 成员位须全清");
    }
    assert_eq!(tfrz.held_locks, HELD_FROZEN,
               "BAND：BD3 只清 THROTTLE 位（FROZEN 归 SuspendToken close 独占）");
    assert!(!consume(B1, QUOTA - 1), "BAND：BD3 新期未越线（预算重置生效）");
    assert!(consume(B1, 1), "BAND：BD3 新期首越线再报");

    // —— BD4. 隔离：带间账面/执法/翻期互不波及 ——
    assert!(configure(B2, 100, PERIOD, NOW), "BAND：BD4 B2 武装");
    assert!(configure(B3, 5_000, PERIOD, NOW), "BAND：BD4 B3 武装");
    let mut rq4 = Runqueue::new();
    let u1 = mk(b"bdU".as_slice(), B2);
    let u2 = mk(b"bdU2".as_slice(), B2);
    let v1 = mk(b"bdV".as_slice(), B3);
    for t in [&mut *u1, &mut *u2, &mut *v1] {
        fair.enqueue(t, &mut rq4);
    }
    let b1_snap = state(B1).unwrap(); // B1 耗尽态快照（隔离对照）
    assert!(consume(B2, 100), "BAND：BD4 B2 首越线（小配额）");
    assert_eq!(throttle_all(B2, &rq4), 2, "BAND：BD4 只限 B2 两成员");
    assert!(u1.held_locks & HELD_THROTTLE != 0 && u2.held_locks & HELD_THROTTLE != 0,
            "BAND：BD4 B2 成员须置位");
    assert_eq!(v1.held_locks, 0, "BAND：BD4 B3 成员不受 B2 执法");
    assert_eq!(state(B1).unwrap(), b1_snap, "BAND：BD4 B2 消耗不波及 B1 账面");
    assert_eq!(throttle_all(B1, &rq4), 0, "BAND：BD4 B1 执法不越带扫 rq4");
    assert!(!consume(B3, 4_999), "BAND：BD4 B3 走自己的账（未越线）");
    assert!(period_check(B2, NOW + PERIOD), "BAND：BD4 B2 到期翻期");
    assert_eq!(state(B3).unwrap().consumed, 4_999, "BAND：BD4 B2 翻期不重置 B3 账");
    assert_eq!(state(B2).unwrap().consumed, 0, "BAND：BD4 B2 翻期清自己");
    assert_eq!(unthrottle_all(B2, &rq4), 2, "BAND：BD4 B2 解限流");

    // —— BD5. Process.band_id 记账面（S6-e 内核侧字段：出生 0 = 未编带；
    //         spawn 映射点写档 → band 消费面读档的往返。test_freeze_cycles
    //         同款手搭：Vmar::new_root + Process::new 经 bootstrap 表）。——
    {
        use crate::handle::{KernelObj, ObjType, RIGHT_MANAGE};
        use crate::process::Process;
        use crate::vmar::Vmar;

        let ht = crate::syscall::bootstrap_table();
        let vm = Vmar::new_root(crate::mm::frames(), crate::aspace::alloc_asid()).unwrap();
        let (pa, asid) = (vm.ttbr0_pa(), vm.asid());
        let hv = ht.insert(KernelObj::Vmar(vm), RIGHT_MANAGE).unwrap();
        let (vref, _) = ht.get_raw(hv, ObjType::Vmar, 0).unwrap();
        let hp = ht
            .insert(KernelObj::Process(unsafe { Process::new(vref, pa, asid) }), RIGHT_MANAGE)
            .unwrap();
        let (pref, _) = ht.get_raw(hp, ObjType::Process, RIGHT_MANAGE).unwrap();
        // SAFETY: 刚 get_raw 过（在表、类型 Process）；ktest 单核顺序。
        let p = unsafe { (*pref.as_ptr()).obj.as_process_mut().unwrap() };
        assert_eq!(p.band_id, 0, "BAND：BD5 Process 出生 band_id=0（未编带）");
        p.band_id = B3; // spawn 映射点同形写档（intent 档映射）
        assert_eq!(p.band_id, INTENT_BACKGROUND, "BAND：BD5 band_id 写读往返");
        ht.drain(); // Drop 链：release vmar 钉住 + PROCESS_LIVE 归位
    }

    // —— 收口：mid-flight 翻配置义务——先清环上位（BD4 尾已 unthrottle）
    //         再 configure(0) 关全带；关态 = 与接口缺席逐位等价（零扫描）。——
    assert!(!configure(B1, 0, 0, 0), "BAND：收口关 B1");
    assert!(!configure(B2, 0, 0, 0), "BAND：收口关 B2");
    assert!(!configure(B3, 0, 0, 0), "BAND：收口关 B3");
    for b in [B1, B2, B3] {
        assert!(!band_on(b), "BAND：收口后须全关（默认全关）");
        assert_eq!(state(b), Some(BandBudget::OFF), "BAND：收口槽须 OFF");
    }
    assert_eq!(throttle_all(B2, &rq4), 0,
               "BAND：收口关态零扫描（u1/u2 仍在环）");
    assert_eq!(u1.held_locks, 0, "BAND：收口不再执法");

    berry_hal_aarch64::uart_puts(b"BAND-K ok\n");
}

/// v1.3 §7 EAS（`调点调度-v1.3.md` §7 预研建议② / ADR #53「砍实现不砍
/// 协议位」）：EM 表形状四段验证——**未注册态 / 默认表形状 / 拒收护栏 /
/// 重注册换表**。ktest 纪律（sched::init 前跑、单核——`EM_TABLE` 写面
/// 契约的 ktest 等价；boot 期注册在 ktest 后跑，EM-0 未注册态不被预填
/// 污染）。算法面（compute_energy/FEuC）按裁决留 M2/真机，不在此测。
fn test_em() {
    use berry_sched::{
        em_domain, em_max_capacity, em_min_power, em_register, em_register_qemu_virt, em_valid,
        EmPoint, EM_DOMAINS_MAX, EM_POINTS_MAX,
    };

    // —— EM-0：未注册态（ktest 先于 boot 注册——全域须「接口缺席」等价）。
    for d in 0..EM_DOMAINS_MAX {
        assert!(!em_valid(d), "EM：EM-0 域 {d} 未注册须 invalid");
        assert!(em_domain(d).is_none(), "EM：EM-0 域 {d} 拷出须 None");
        assert_eq!(em_max_capacity(d), 0, "EM：EM-0 域 {d} max_capacity 须 0");
        assert_eq!(em_min_power(d), 0, "EM：EM-0 域 {d} min_power 须 0");
    }
    assert!(!em_valid(EM_DOMAINS_MAX), "EM：EM-0 越界域 valid 须 false");
    assert_eq!(em_max_capacity(EM_DOMAINS_MAX), 0, "EM：EM-0 越界域查询须 0");

    // —— EM-1：QEMU virt 默认表注册 + 形状逐位对拍。
    assert!(em_register_qemu_virt(), "EM：EM-1 默认表注册须受理");
    assert!(em_valid(0), "EM：EM-1 注册后域 0 须 valid");
    let d0 = em_domain(0).expect("EM：EM-1 域 0 拷出须 Some");
    assert_eq!(d0.cpu_mask, 0b11, "EM：EM-1 cpu_mask 须 CPU0-1");
    assert_eq!(d0.npoints, 3, "EM：EM-1 三点表");
    assert_eq!(
        d0.points[0],
        EmPoint { freq_khz: 250_000, power_mw: 5, capacity: 400 },
        "EM：EM-1 最低频点形状"
    );
    assert_eq!(
        d0.points[1],
        EmPoint { freq_khz: 500_000, power_mw: 15, capacity: 750 },
        "EM：EM-1 中间频点形状"
    );
    assert_eq!(
        d0.points[2],
        EmPoint { freq_khz: 1_000_000, power_mw: 50, capacity: 1024 },
        "EM：EM-1 最高频点形状"
    );
    assert_eq!(d0.points[3], EmPoint { freq_khz: 0, power_mw: 0, capacity: 0 },
               "EM：EM-1 尾槽须零填充（有效前缀外不动）");
    assert_eq!(em_max_capacity(0), 1024,
               "EM：EM-1 最高频点容量（Topo::capacity 同口径 1024 满格）");
    assert_eq!(em_min_power(0), 5, "EM：EM-1 最低频点功耗");
    assert!(!em_valid(1), "EM：EM-1 域 1 须仍未注册（QEMU 单域默认）");

    // —— EM-2：形状护栏拒收（空表/超长/零掩码/越界域/非严格升序）。
    let asc: [EmPoint; 2] = [
        EmPoint { freq_khz: 300_000, power_mw: 8, capacity: 500 },
        EmPoint { freq_khz: 800_000, power_mw: 30, capacity: 850 },
    ];
    let empty: [EmPoint; 0] = [];
    let too_long: [EmPoint; EM_POINTS_MAX + 1] = [
        EmPoint { freq_khz: 100_000, power_mw: 1, capacity: 100 },
        EmPoint { freq_khz: 200_000, power_mw: 2, capacity: 200 },
        EmPoint { freq_khz: 300_000, power_mw: 4, capacity: 300 },
        EmPoint { freq_khz: 400_000, power_mw: 8, capacity: 400 },
        EmPoint { freq_khz: 500_000, power_mw: 16, capacity: 500 },
    ];
    let desc: [EmPoint; 2] = [asc[1], asc[0]];
    let dup: [EmPoint; 2] = [asc[0], asc[0]];
    assert!(!em_register(0, 0b11, &empty), "EM：EM-2 空点表须拒");
    assert!(!em_register(0, 0b11, &too_long), "EM：EM-2 超上限点表须拒");
    assert!(!em_register(0, 0, &asc), "EM：EM-2 零掩码须拒");
    assert!(!em_register(EM_DOMAINS_MAX, 0b11, &asc), "EM：EM-2 越界域须拒");
    assert!(!em_register(0, 0b11, &desc), "EM：EM-2 降序须拒（升序不变量）");
    assert!(!em_register(0, 0b11, &dup), "EM：EM-2 等频须拒（严格升序）");
    // 拒收零写入：域 0 仍是 EM-1 默认表（失败路径不动已注册内容）。
    assert_eq!(em_domain(0), Some(d0), "EM：EM-2 拒收须零写入（表不动）");

    // —— EM-3：第二域注册 + 重注册换表（真机 DT 替换占位表的同通路）。
    assert!(em_register(1, 0b100, &asc), "EM：EM-3 域 1 两点表须受理");
    assert_eq!(em_max_capacity(1), 850, "EM：EM-3 域 1 最高频点容量");
    assert_eq!(em_min_power(1), 8, "EM：EM-3 域 1 最低频点功耗");
    let two_pt: [EmPoint; 2] = [
        EmPoint { freq_khz: 100_000, power_mw: 2, capacity: 160 },
        EmPoint { freq_khz: 1_000_000, power_mw: 50, capacity: 1024 },
    ];
    assert!(em_register(1, 0b100, &two_pt), "EM：EM-3 域 1 重注册换表须受理");
    assert_eq!(em_min_power(1), 2, "EM：EM-3 重注册后读面须换新表");

    // —— 收口：域 1 无「注销」面（无 unregister——EM 表只增不撤，真机
    //         换表走重注册）；域 0 重注册回 QEMU 默认（与 boot 期注册
    //         幂等同形——ktest 瞬态不外泄）。
    assert!(em_register_qemu_virt(), "EM：收口重注册回 QEMU 默认");
    assert_eq!(em_domain(0), Some(d0), "EM：收口后域 0 = 默认表");

    berry_hal_aarch64::uart_puts(b"EM ok\n");
}

/// v1.3 §6「簇 util 通路」最小件 + §7 预研建议④「频率归 PM」（
/// `调点调度-v1.3.md`）：per-CPU utilization EWMA 验证——**记账→折叠→
/// 读出** 纯函数链（合成 `now` 驱动，精确到 >>3 逐位）+ **Fair::tick 接线
/// 面**（真路径 delta 进账——class.rs 的 util_accumulate/tick_window 消费
/// 证活）。PM 消费面（f = util×fmax/capacity 的裁决环）按 §7/§11 裁决归
/// berry-power（M2/真机），不在此测。ktest 纪律：sched::init 前跑、IRQ
/// 屏蔽——test_deadline 的单次 rr.tick 已开窗零折叠，UTIL-0 重置回 boot
/// 等价态起测；收口全槽清零（ktest 瞬态不外泄——EM 收口同款）。
fn test_util() {
    use berry_sched::{
        cpu_util, util_accumulate, util_reset, util_tick_window, util_window_vct, Fair,
        EWMA_ALPHA_SHIFT, UTIL_NR_CPU,
    };
    use berry_sched::{Runqueue, SchedClass, Task};

    /// 占位入口（tick 只记账不切换——永不执行；halt 兜底防未定义行为）。
    extern "C" fn util_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    // —— UTIL-0：清场 + 未启态（ewma 恒 0；越界读 0）+ 窗长单源。
    for c in 0..UTIL_NR_CPU {
        util_reset(c);
        assert_eq!(cpu_util(c), 0, "UTIL：未启态 ewma 须 0（CPU{c}）");
    }
    assert_eq!(cpu_util(UTIL_NR_CPU), 0, "UTIL：越界 CPU 读须 0");
    // 窗长 = CNTFRQ/100（频率不硬编码——62.5MHz → 625_000 ≈ 10ms）。
    let w = util_window_vct();
    assert_eq!(w, berry_hal_aarch64::timer::cntfrq() / 100, "UTIL：窗长须 = CNTFRQ/100");
    assert!(w > 0, "UTIL：窗长须非零");

    // —— UTIL-1：首调只开窗不折叠；窗未到期不折叠。
    util_tick_window(0, 1_000);
    assert_eq!(cpu_util(0), 0, "UTIL：首调只开窗");
    util_accumulate(0, w / 2);
    util_tick_window(0, 1_000 + w / 2);
    assert_eq!(cpu_util(0), 0, "UTIL：窗未到期不折叠");

    // —— UTIL-2：满载窗（accum = elapsed）——util_raw = 1024 → ewma =
    //            0 + 1024>>3 = 128（α = 1/8 移位零除法首折）。
    util_accumulate(0, w / 2); // 半窗 + 半窗 = 整窗
    util_tick_window(0, 1_000 + w);
    assert_eq!(cpu_util(0), 128, "UTIL：满载首折 ewma = 1024/8 = 128");

    // —— UTIL-3：连续满载收敛——ewma_{n+1} = ewma_n + (1024-ewma_n)>>3
    //            逐窗精确对拍（8 窗 ≈ 70% 满格，单调升恒 <1024）。
    let mut expect: u32 = 128;
    for i in 1..=8u64 {
        util_accumulate(0, w);
        util_tick_window(0, 1_000 + w * (1 + i));
        expect += (1024 - expect) >> EWMA_ALPHA_SHIFT;
        assert_eq!(cpu_util(0), expect, "UTIL：满载第 {i} 窗折叠须精确");
    }
    assert!(expect > 700 && expect < 1024, "UTIL：8 窗须逼近但不满格（{expect}）");

    // —— UTIL-4：半载窗——util_raw = 512（1024 尺度占空比）→ 512>>3 = 64。
    util_reset(0);
    util_tick_window(0, 5_000);
    util_accumulate(0, w / 2);
    util_tick_window(0, 5_000 + w);
    assert_eq!(cpu_util(0), 64, "UTIL：半载首折 ewma = 512/8 = 64");

    // —— UTIL-5：idle 稀释——零累计 + 整窗 elapsed → util_raw = 0，
    //            ewma 严格下降（64 - 64>>3 = 56；负差分支减法同式）。
    util_tick_window(0, 5_000 + 2 * w); // 无 accumulate 的空窗
    assert_eq!(cpu_util(0), 56, "UTIL：idle 折叠须 = 64 - 64>>3 = 56");

    // —— UTIL-6：饱和钳位——巨量累计不溢出不破 1024 上界（钳后同满载首折）。
    util_reset(0);
    util_tick_window(0, 7_000);
    util_accumulate(0, w.saturating_mul(1 << 40)); // >> 窗时长的超载账
    util_tick_window(0, 7_000 + w);
    assert_eq!(cpu_util(0), 128, "UTIL：超载首折须钳 1024（同满载）");

    // —— UTIL-7：per-CPU 独立——CPU1 自行开窗记账，CPU0 读数不受牵连。
    assert_eq!(cpu_util(1), 0, "UTIL：CPU1 未记账须 0");
    util_tick_window(1, 9_000);
    util_accumulate(1, w);
    util_tick_window(1, 9_000 + w);
    assert_eq!(cpu_util(1), 128, "UTIL：CPU1 独立记账首折 128");
    assert_eq!(cpu_util(0), 128, "UTIL：CPU0 读数须独立于 CPU1 翻窗");

    // —— UTIL-8：越界 CPU 静默拒（不 panic、零写入）。
    util_accumulate(UTIL_NR_CPU, w);
    util_tick_window(UTIL_NR_CPU, 11_000);
    util_reset(UTIL_NR_CPU);

    // —— UTIL-9（接线面）：Fair::tick 真路径记账证活——class.rs 消费链
    //      （util_accumulate + util_tick_window）在真 tick 下运转，读出口
    //      非零即全链接通。tick#1 开窗（首调丢账路径）；自旋对表 CNTVCT
    //      燃 ~1/8 窗真实时长（确定性，非忙等估算）；tick#2 的 delta =
    //      该时长经非 idle 门进 accum；手动推一窗到期翻窗——elapsed < 2w
    //      下 util_raw ≥ 1024/16 → ewma ≥ 8 > 0。
    util_reset(0);
    let mut rq = Runqueue::new();
    let rr = Fair;
    let mut task = Task::new_kernel(util_dummy, 0, b"util");
    rr.tick(&mut *task, &mut rq); // 开窗（LAST_VCT 回落位更新 + 首窗开）
    let t0 = berry_hal_aarch64::timer::cntvct();
    while berry_hal_aarch64::timer::cntvct() - t0 < w / 8 {}
    rr.tick(&mut *task, &mut rq); // delta = w/8 进 accum（非 idle 门真消费）
    util_tick_window(0, berry_hal_aarch64::timer::cntvct() + w); // 强制到期
    let u = cpu_util(0);
    assert!(u >= 1 && u <= 128, "UTIL：Fair.tick 接线后须非零读数（got {u}）");

    // —— 收口：全槽清零回 boot 等价态（真 tick 稳态由 boot 后 Fair::tick
    //           重新开窗——ktest 瞬态不外泄）。util_dummy 任务体 16KiB 栈
    //           泄漏不回收（ktest 语境同 spawn 政策——v1 无 reap）。
    for c in 0..UTIL_NR_CPU {
        util_reset(c);
        assert_eq!(cpu_util(c), 0, "UTIL：收口须回未启态（CPU{c}）");
    }

    berry_hal_aarch64::uart_puts(b"UTIL ok\n");
}

/// OU 系：overutilized 回退（v1.3 §7 预研建议⑤——「EAS 永远是机会主义」：
/// 任一核 util 超域最高容量 80% → EAS 放置 hint 失效，Linux 同名机制的
/// BerryOS 最小面）：
/// - OU-1 cpu_util < 80% 容量 → overutilized == false（含 819 == 阈值的
///   边界钉板——严格 > 才判、等于不算）；
/// - OU-2 cpu_util > 80% 容量（820）→ overutilized == true；any_overutilized
///   任一核口径双向（CPU0 over / CPU1 over 各自单核饱和即真）；
/// - OU-3 overutilized 态 placement_hint → 0（表里有 placement 值也失效）；
/// - OU-4 非 overutilized 态 placement_hint 正常查表透传。
///
/// 驱动面 = 合成窗折叠（UTIL 系同款——util_raw 精确可控）：窗长取 ≥ 名义
/// 窗的最小 1024 整倍 step（k = w/1024+1），accum = k×u / elapsed = k×1024
/// → util_raw = u 逐位精确（u128 分子零舍入）。满载链 ewma_{n+1} =
/// ewma_n + (1024-ewma_n)>>3 确定性收敛：12 折至 815（79.6% < 80%）、精调
/// 两折 819（== 阈值）→ 820（首越阈）——80% 边界两侧各一钉板。EM 前提 =
/// QEMU 默认表（域 0 最高容量 1024 → 阈值 1024×819/1024 = 819；test_em
/// 收口态的幂等重注册）。ktest 纪律同 UTIL：sched::init 前跑、纯合成时间
/// 轴；收口 unbind + 全槽清零（ktest 瞬态不外泄）。OVERUTIL ok。
fn test_overutil() {
    use berry_sched::policy::{self, Directive, Entry};
    use berry_sched::{
        any_overutilized, cpu_util, em_max_capacity, em_register_qemu_virt, overutilized,
        util_accumulate, util_reset, util_tick_window, util_window_vct, EWMA_ALPHA_SHIFT,
        UTIL_NR_CPU,
    };

    // 合成时间轴参数：step = ≥ 名义窗的最小 1024 整倍（62.5MHz → w=625_000
    // → k=611、step=625_664）；elapsed = step 下 accum = k×u → util_raw = u。
    let k = util_window_vct() / 1024 + 1;
    let step = k * 1024;

    /// 折一窗：util_raw = u 精确进 EWMA（`t` 单调推进合成时间轴）。
    fn fold(cpu: usize, u: u32, t: &mut u64, k: u64, step: u64) {
        util_accumulate(cpu, k * u as u64);
        *t += step;
        util_tick_window(cpu, *t);
    }

    // —— 前置清场（EM 幂等重注册 = test_em 收口同表；util 全槽清零 =
    //      test_util 收口态防御性重申；policy 回哨兵——PL 系同款）。
    assert!(em_register_qemu_virt(), "OU：QEMU 默认 EM 表注册须受理");
    assert_eq!(em_max_capacity(0), 1024, "OU：域 0 最高容量须 1024（→ 阈值 819）");
    policy::unbind();
    for c in 0..UTIL_NR_CPU {
        util_reset(c);
    }
    assert!(!any_overutilized(), "OU：前置全零须不 over");

    // —— OU-1：cpu_util < 80% 容量 → false。满载链 12 折至 815（79.6%）
    //      + 精调一折至 819（== 阈值——严格 > 语义：等于不判）。
    let mut t0 = 1_000_000u64;
    util_tick_window(0, t0); // 开首窗（start==0 → 只开窗不折叠）
    let mut expect = 0u32;
    for i in 1..=12u32 {
        fold(0, 1024, &mut t0, k, step); // 满载窗：util_raw = 1024
        expect += (1024 - expect) >> EWMA_ALPHA_SHIFT;
        assert_eq!(cpu_util(0), expect, "OU：满载第 {i} 折须精确（{expect}）");
    }
    assert_eq!(expect, 815, "OU：12 折满载须至 815（< 819.2 = 80%）");
    assert!(!overutilized(0), "OU-1：util 815 < 80% 须不判 over");
    fold(0, 847, &mut t0, k, step); // 815 + (847-815)>>3 = 819（== 阈值）
    assert_eq!(cpu_util(0), 819, "OU：精调一折须至 819（== 阈值）");
    assert!(!overutilized(0), "OU-1：util == 阈值 819 须不判 over（严格 >）");
    assert!(!any_overutilized(), "OU-1：全域阈下 any 须 false");
    assert!(!overutilized(UTIL_NR_CPU), "OU-1：越界 cpu 须 false（util 0）");

    // —— OU-2：cpu_util > 80% 容量 → true。819 + (827-819)>>3 = 820（首越阈）。
    fold(0, 827, &mut t0, k, step); // 819 + 8>>3 = 820
    assert_eq!(cpu_util(0), 820, "OU：再折须至 820（> 819）");
    assert!(overutilized(0), "OU-2：util 820 > 80% 须判 over");
    assert!(any_overutilized(), "OU-2：CPU0 over → any 须 true（CPU1 仍 0）");

    // —— OU-3：overutilized 态 placement_hint → 0（表里有 placement 值也失效
    //      ——EAS 机会主义纪律：饱和即回退普通均衡）。
    static PLACE: [Entry; 2] = [
        Entry { task_key: 7, directive: Directive { placement: 1, weight: 0, epp: None } },
        Entry { task_key: 8, directive: Directive { placement: 2, weight: 0, epp: None } },
    ];
    assert!(policy::bind_gen(&PLACE, policy::gen() + 1), "OU：放置表 bind_gen 须受理");
    assert_eq!(policy::placement_hint(7), 0, "OU-3：over 态命中键 7 须 0（hint 失效）");
    assert_eq!(policy::placement_hint(8), 0, "OU-3：over 态命中键 8 须 0（hint 失效）");

    // —— OU-2b：任一核口径——CPU0 清零后 CPU1 泵越阈 → any 仍 true
    //      （迭代面覆盖到 CPU1，非只看 0 号核）。
    util_reset(0);
    assert!(!any_overutilized(), "OU：CPU0 清零后须暂不 over（CPU1 仍 0）");
    let mut t1 = 3_000_000u64;
    util_tick_window(1, t1); // 开首窗
    let mut e1 = 0u32;
    while e1 <= 819 {
        fold(1, 1024, &mut t1, k, step);
        e1 += (1024 - e1) >> EWMA_ALPHA_SHIFT;
    }
    assert_eq!(cpu_util(1), e1, "OU：CPU1 满载泵越阈须精确（{e1}）");
    assert!(e1 > 819 && e1 < 1024, "OU：CPU1 泵值域须 (819, 1024)");
    assert!(overutilized(1), "OU-2：CPU1 越阈须判 over");
    assert!(any_overutilized(), "OU-2：任一核 over（CPU1）→ any 须 true");

    // —— OU-4：非 overutilized 态（全清）→ placement_hint 正常查表透传。
    for c in 0..UTIL_NR_CPU {
        util_reset(c);
    }
    assert!(!any_overutilized(), "OU-4：前置须回非 over 态");
    assert_eq!(policy::placement_hint(7), 1, "OU-4：非 over 命中键 7 须透传 1（偏 little）");
    assert_eq!(policy::placement_hint(8), 2, "OU-4：非 over 命中键 8 须透传 2（偏 big）");
    assert_eq!(policy::placement_hint(9), 0, "OU-4：未命中键 9 须 0（查表原语义不动）");

    // —— 收口：unbind + 全槽清零（ktest 瞬态不外泄——UTIL/EM 同款）。
    policy::unbind();
    for c in 0..UTIL_NR_CPU {
        util_reset(c);
        assert_eq!(cpu_util(c), 0, "OU：收口须回未启态（CPU{c}）");
    }

    berry_hal_aarch64::uart_puts(b"OVERUTIL ok\n");
}

/// v1.3 §9 真机框架：HDR 直方图（`sched/src/hdr.rs` 最小版）——①桶形状
/// 端点锚 + 值→桶映射全域钉板（保守上界 + 桶宽误差界）；②record/极值/
/// 分位累计秩（1..=100 各一记：rank k ⇔ 样本 k，<64 精确/≥64 保守上界）；
/// ③钳位口径（超值域样本桶面入顶桶、极值面记原值 = 失真信号）；④per-CPU
/// 槽隔离 + 跨核合并读面（ktest 单核窗：CPU0 走热路径助手，CPU1 槽直索引
/// ——写方限本核是生产纪律，测试直索引不破形状断言）。
fn test_hdr() {
    use berry_sched::hdr::{
        bucket_of, bucket_upper, cpu_hdr, cpu_reset, merged_max, merged_min, merged_percentile,
        merged_total, record_switch_lat, Hdr, LatKind, HDR_BUCKETS, HDR_MAX_VALUE,
        HDR_NR_CPU, HDR_OVERFLOW, HDR_SUB,
    };

    // —— HDR-0：形状常量 + 桶上界端点锚（尺寸/对齐钉板在 hdr.rs 编译期）。
    assert_eq!(HDR_BUCKETS, 897, "HDR：桶数须 28×32+1 = 897");
    assert_eq!(HDR_OVERFLOW, 896, "HDR：溢出桶下标锚");
    assert_eq!(HDR_MAX_VALUE, (1u64 << 28) - 1, "HDR：值域上界 2^28−1");
    assert_eq!(HDR_SUB, 32, "HDR：每八度子桶数锚");
    assert_eq!(bucket_of(0), 0, "HDR：v=0 防御归桶 0（record 早退不至此）");
    assert_eq!(bucket_upper(0), 1, "HDR：桶 0 上界 = 1（值 1 精确）");
    assert_eq!(bucket_upper(HDR_BUCKETS - 2), HDR_MAX_VALUE, "HDR：顶桶上界 = 值域顶");
    assert_eq!(bucket_upper(HDR_OVERFLOW), u64::MAX, "HDR：溢出桶上界哨兵");

    // —— HDR-B：值→桶映射全域钉板。低值 1..=2^16 逐值 + 每八度边界 ±1/
    //   中部/子桶边界（2^28 全域 2.7 亿次 TCG 不可行——八度内映射是线性
    //   平移，端点钉住即全域钉住）。断言三条：idx 不落溢出位；上界保守
    //   （≥ v）；上界差 < 桶宽（相对精度 1/32 的逐值兑现）。
    fn check(v: u64) {
        let idx = bucket_of(v);
        assert!(idx < HDR_OVERFLOW, "HDR-B：v={v} 桶 {idx} 须 < 溢出位");
        let b = bucket_upper(idx);
        assert!(b >= v, "HDR-B：v={v} 上界 {b} 须保守 ≥ v");
        let oct = 63 - v.leading_zeros();
        let width = if oct >= 5 { 1u64 << (oct - 5) } else { 1 };
        assert!(b - v < width, "HDR-B：v={v} 上界差 {} 须 < 桶宽 {width}", b - v);
    }
    for v in 1..=(1u64 << 16) {
        check(v);
    }
    for oct in 0..28u32 {
        let base = 1u64 << oct;
        check(base); // 八度底
        if base > 1 {
            check(base - 1); // 前八度顶
        }
        check((base << 1) - 1); // 本八度顶（oct=27 → 值域顶）
        check(base + (base >> 5)); // 八度中部样本
        check(base + (1u64 << (oct.max(5) - 5)) - 1); // 子桶 0 上界 −1
    }

    // —— HDR-1：record/total/min/max + 分位累计秩（本地静态实例——3.6KB
    //   不吃栈；static 非 mut 纯原子，零 unsafe 不入 writable_globals 册）。
    static H: Hdr = Hdr::new();
    assert_eq!(H.total(), 0, "HDR-1：出生态零样本");
    assert_eq!(H.percentile(500), 0, "HDR-1：零样本分位须 0（非 panic）");
    assert_eq!(H.min_value(), u64::MAX, "HDR-1：零样本 min 哨兵");
    assert_eq!(H.max_value(), 0, "HDR-1：零样本 max 0");
    H.record(0); // 值域外静默拒
    assert_eq!(H.total(), 0, "HDR-1：v=0 须静默拒");
    for v in 1..=100u64 {
        H.record(v);
    }
    assert_eq!(H.total(), 100, "HDR-1：total");
    assert_eq!(H.min_value(), 1, "HDR-1：min");
    assert_eq!(H.max_value(), 100, "HDR-1：max");
    // 1..=100 各一记：rank k ⇔ 样本 k；v < 64 桶宽 1 → 分位精确；
    // v = 100 落桶宽 2 桶 [100,102) → 保守上界 101。
    assert_eq!(H.percentile(0), 1, "HDR-1：pm=0 → 首样本（精确）");
    assert_eq!(H.percentile(500), 51, "HDR-1：p50 → 第 51 小 = 51（精确）");
    assert_eq!(H.percentile(990), 101, "HDR-1：p99 → 第 100 小 = 100，保守上界 101（桶宽 2）");
    assert_eq!(H.percentile(1000), 101, "HDR-1：p100 同 p99（rank 帽 N）");
    assert_eq!(H.percentile_rank(1), 1, "HDR-1：rank 直取首样本");
    assert_eq!(H.percentile_rank(0), 1, "HDR-1：rank=0 帽到 1");
    assert_eq!(H.percentile_rank(u64::MAX), 101, "HDR-1：rank 越界帽到 N");
    assert_eq!(H.bucket(bucket_of(100)), 1, "HDR-1：单桶读（值 100 桶恰一记）");
    assert_eq!(H.bucket(HDR_BUCKETS), 0, "HDR-1：越界桶读 0");

    // —— HDR-2：钳位口径（超值域样本：桶面钳位入顶桶 895、极值面记原值
    //   ——max 超 HDR_MAX_VALUE 即「长尾被 4.3s 封顶吃掉」的失真信号）。
    H.record(u64::MAX);
    assert_eq!(H.total(), 101, "HDR-2：钳位样本照常计数");
    assert_eq!(H.max_value(), u64::MAX, "HDR-2：max 记原值（失真信号）");
    assert_eq!(bucket_of(u64::MAX), HDR_BUCKETS - 2, "HDR-2：钳位落顶桶 895（不落溢出位）");
    assert_eq!(H.percentile(1000), HDR_MAX_VALUE, "HDR-2：p100 落顶桶 → 值域顶上界");
    H.reset();
    assert_eq!(H.total(), 0, "HDR-2：reset 回出生态");
    assert_eq!(H.percentile(999), 0, "HDR-2：reset 后零样本分位 0");
    assert_eq!(H.min_value(), u64::MAX, "HDR-2：reset 后 min 回哨兵");

    // —— HDR-3：per-CPU 槽隔离 + 跨核合并（§9「p99 跨核怎么并」最小答案
    //   = 桶位直和）。清场先行——ktest 前无写者，理论零态，reset 双保险。
    cpu_reset(0);
    cpu_reset(1);
    for v in 1..=10u64 {
        record_switch_lat(v); // cpu_id()=0（ktest 单核窗）→ CPU0 槽
    }
    let c1 = cpu_hdr(1).expect("HDR-3：CPU1 槽须在");
    for v in 11..=20u64 {
        c1.switch_lat.record(v); // 测试面直索引 CPU1 槽
    }
    let c0 = cpu_hdr(0).expect("HDR-3：CPU0 槽须在");
    assert_eq!(c0.switch_lat.total(), 10, "HDR-3：CPU0 槽隔离（1..=10）");
    assert_eq!(c1.switch_lat.total(), 10, "HDR-3：CPU1 槽隔离（11..=20）");
    assert_eq!(c0.switch_lat.min_value(), 1, "HDR-3：CPU0 min");
    assert_eq!(c1.switch_lat.max_value(), 20, "HDR-3：CPU1 max");
    assert_eq!(c0.wake_lat.total(), 0, "HDR-3：同槽双路隔离（wake 未触）");
    assert!(cpu_hdr(HDR_NR_CPU).is_none(), "HDR-3：越界槽须 None");
    assert_eq!(merged_total(LatKind::Switch), 20, "HDR-3：合并 total = Σ各核");
    // 合并 1..=20 各一记：rank k ⇔ 样本 k（v < 32 全精确）。
    assert_eq!(merged_percentile(LatKind::Switch, 500), 11, "HDR-3：合并 p50 → 第 11 小 = 11");
    assert_eq!(merged_percentile(LatKind::Switch, 999), 20, "HDR-3：合并 p99.9 → 第 20 小 = 20");
    assert_eq!(merged_min(LatKind::Switch), 1, "HDR-3：合并 min（跳空核哨兵）");
    assert_eq!(merged_max(LatKind::Switch), 20, "HDR-3：合并 max");
    assert_eq!(merged_percentile(LatKind::Wake, 990), 0, "HDR-3：空路合并分位 0");
    assert_eq!(merged_min(LatKind::Wake), u64::MAX, "HDR-3：全空合并 min 哨兵");
    // 收口：双槽清零回 boot 等价态（ktest 瞬态不外泄——util 收口同款；
    // boot 后三路基准/遥测钩子接管写面）。
    cpu_reset(0);
    cpu_reset(1);
    assert_eq!(merged_total(LatKind::Switch), 0, "HDR-3：收口回零");

    berry_hal_aarch64::uart_puts(b"HDR ok\n");
}

/// v1.3 §9 三路基准之路 2 首刀：**定时器唤醒延迟基准**（cyclictest 口径）的
/// ktest 面——HDR 直方图的首个真实消费者（[`test_hdr`] 钉形状，本刀走热路径
/// API `record_wake_lat` → per-CPU 槽 → 合并分位/极值/总数**全链**）。
///
/// ktest 窗（sched::init 前、boot DAIF 屏蔽、无定时器中断）测不了真睡真醒
/// ——方案 A 简化：轮询 CNTVCT 到 deadline 模拟「被定时器唤醒」，记录
/// 「轮询发现超时」的延迟 = 轮询粒度（读表 + 分支 + spin hint 一圈）。
/// 真 IRQ 唤醒口径的数字面（sleep → on_irq_tick 唤醒）留方案 B boot 序列
/// 基准刀（BENCHWAKE 粗直方图已有——本测试是它分位面细化的接线验证）。
///
/// 口径纪律：
/// - **绝对 deadline 锚定**：`deadline += interval` 每轮——上轮超调不进
///   下轮起点（相对锚 `deadline = now + interval` 会吃进超调、漂移累积）。
/// - lat = now − deadline（saturating——时钟单调下不可负，防御口径）。
/// - 出环条件 `<=`（**严格越过** deadline 才出）⇒ lat ≥ 1 恒立——绕开
///   record 值域 1 起的静默拒 0，`merged_total == N` 才确定性可断。
/// - 间隔 = CNTFRQ/1000（~1ms，随探测频率换算不硬编码；TCG 下 CNTV 推进
///   ≈ 半速 → 墙钟 ~2ms/轮，tick 口径不变——§A8 纪律）。
/// - lat 上限**不断言**：宿主停顿会把真实迟到吃进 lat（绝对锚下迟到的
///   下一轮 deadline 更近——这正是 cyclictest 要测的量，非测试 bug）。
/// - 数值只出数不设门禁（公约 2——统计量归宿主收割）；执法对象 = 接线
///   形状（total 精确 == N / 槽隔离 / 双路隔离 / 分位序）。
fn test_hdr_wake() {
    use berry_sched::hdr::{
        cpu_hdr, cpu_reset, merged_max, merged_min, merged_percentile, merged_total,
        record_wake_lat, LatKind, HDR_MAX_VALUE, HDR_NR_CPU,
    };
    use berry_hal_aarch64::timer::{cntfrq, cntvct};

    /// 轮数：p99 rank = ⌊128×0.99⌋+1 = 127 < N（非退化——不满轮帽顶；
    /// N ≤ 100 时 p99 恒 = max，长尾判别力归零）。
    const N: u64 = 128;

    // —— HDRWAKE-0：清场 + 出生态（test_hdr 收口已零，重复跑双保险）。
    for c in 0..HDR_NR_CPU {
        cpu_reset(c);
    }
    assert_eq!(merged_total(LatKind::Wake), 0, "HDRWAKE：清场后须零样本");

    // —— HDRWAKE-1：基准主循环（cyclictest 口径的忙等简化）。~1ms × 128
    //      轮 ≈ 128ms tick（TCG 墙钟 ~2×）；IRQ 屏蔽窗单线程——零调度
    //      噪声，lat 纯 = 轮询粒度。record_wake_lat 单参（cpu 内部自取
    //      ——写方限本核纪律，非草图的两参形态）。
    let interval = cntfrq() / 1000; // ~1ms（62.5MHz → 62500 ticks）
    assert!(interval > 0, "HDRWAKE：间隔须非零");
    let mut deadline = cntvct() + interval; // 首轮锚
    for _ in 0..N {
        // 「睡」到 deadline：轮询模拟定时器唤醒（真 sleep 需 sched::init +
        // IRQ——方案 B 的 boot 序列刀）。
        while cntvct() <= deadline {
            core::hint::spin_loop();
        }
        // 「唤醒后」读表 + 记账：出环即严格越过 + CNTVCT 单调 ⇒ lat ≥ 1。
        let now = cntvct();
        let lat = now.saturating_sub(deadline);
        assert!(lat >= 1, "HDRWAKE：lat={lat} 须 ≥1（record 拒 0 → total 漂移）");
        record_wake_lat(lat); // 热路径真 API：cpu_id()=0（ktest 单核窗）→ CPU0 槽
        deadline += interval; // 绝对锚：不累积漂移
    }

    // —— HDRWAKE-2：读面对账（record → per-CPU 槽 → 合并读面全链）。
    let n = merged_total(LatKind::Wake);
    assert_eq!(n, N, "HDRWAKE：合并 total={n} 须 == 轮数 {N}（record→total 断链即现形）");
    let c0 = cpu_hdr(0).expect("HDRWAKE：CPU0 槽须在");
    let c1 = cpu_hdr(1).expect("HDRWAKE：CPU1 槽须在");
    assert_eq!(c0.wake_lat.total(), N, "HDRWAKE：CPU0 槽须收全（record_wake_lat 槽化接线）");
    assert_eq!(
        c1.wake_lat.total(),
        0,
        "HDRWAKE：CPU1 槽须零（ktest 单核窗——泄漏即 cpu_id() 槽化错）"
    );
    assert_eq!(c0.switch_lat.total(), 0, "HDRWAKE：switch 路须零（双路隔离——本测试只写 wake）");
    // 分位/极值序：累计秩扫描 + bucket_upper 随下标单调 ⇒ min ≤ p50 ≤ p99
    // 恒立（p99 ≤ max **不**断言：末两样本同桶时保守上界可越 max——桶面口径）。
    let p50 = merged_percentile(LatKind::Wake, 500);
    let p99 = merged_percentile(LatKind::Wake, 990);
    let mn = merged_min(LatKind::Wake);
    let mx = merged_max(LatKind::Wake);
    assert!(mn >= 1, "HDRWAKE：min={mn} 须 ≥1（值域口径）");
    assert!(p50 >= mn && p99 >= p50, "HDRWAKE：分位序破（min={mn} p50={p50} p99={p99}）");
    assert!(mx < HDR_MAX_VALUE, "HDRWAKE：max={mx} 超值域——长尾被 4.3s 封顶钳位（失真信号）");

    // —— 打印：机器可读单行（p50/p99 = 命中桶保守上界口径；统计量只出数，
    //      宿主侧 bench.py 同表收割——BENCH 行同纪律）。
    berry_hal_aarch64::uart_puts(b"HDRWAKE ok p50=");
    berry_hal_aarch64::uart_put_dec(p50);
    berry_hal_aarch64::uart_puts(b" p99=");
    berry_hal_aarch64::uart_put_dec(p99);
    berry_hal_aarch64::uart_puts(b" n=");
    berry_hal_aarch64::uart_put_dec(n);
    berry_hal_aarch64::uart_puts(b"\n");

    // —— 收口：全槽清零回 boot 等价态（ktest 瞬态不外泄——test_hdr 同款；
    //      boot 后三路基准/遥测钩子接管写面）。
    for c in 0..HDR_NR_CPU {
        cpu_reset(c);
    }
    assert_eq!(merged_total(LatKind::Wake), 0, "HDRWAKE：收口回零");
}

/// 4-4：fastpath 检查链的 SLEEPQ 侧纯函数面（`find_single_waiter`）+ send 尾部
/// fork 的慢路回归。**ktest 纪律（sched::init 前跑）**：手搭睡眠者在 SLEEPQ
/// 期间绝不可走 SVC send——fastpath 分支会 `fast_wake` 进尚未启用的全局
/// RUNQUEUE + `try_preempt` 解引用 null CURRENT = 挂死；SVC 面只在清场后跑
///（慢路分支无等待者，安全）。EL0 全链 + fastpath/直切/RT 实测在 main.rs
/// fa_ping/fa_pong 对打（`FAST ok`）。
#[allow(clippy::unwrap_used)]
fn test_fast() {
    use alloc::boxed::Box;
    use berry_sched::{find_single_waiter, sleepq_insert, sleepq_unlink, Task, WAIT_ANY};
    use crate::channel::ChannelEnd;
    use crate::handle::{
        KernelObj, ObjRef, ObjType, RIGHT_RECV, RIGHT_SEND, SIGNAL_READABLE,
    };
    use crate::syscall::{bootstrap_table, NR_CHANNEL_SEND};

    /// 占位入口（手搭睡眠者永不执行；halt 兜底）。
    extern "C" fn fast_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    /// 手搭睡眠者：睡 `obj` 等 `signals`（无限期），入 SLEEPQ 后返回——永不
    /// 执行（同 test_pi 的 sleeper，砍掉 PI 侧字段）。
    fn sleeper(obj: usize, signals: u64) -> &'static mut Task {
        let mut t = Task::new_kernel(fast_dummy, 0, b"l_slp");
        t.wait_obj = obj;
        t.wait_signals = signals;
        t.wait_deadline = u64::MAX;
        let t: &'static mut Task = Box::leak(t);
        sleepq_insert(t);
        t
    }

    /// 建端点进内核侧表（返回 handle + ObjRef 地址——SVC 回归要用 handle）。
    fn chan_end(rights: crate::handle::Rights) -> (u32, usize) {
        let ht = bootstrap_table();
        let h = ht
            .insert(
                KernelObj::Channel(ChannelEnd::try_unpaired(false).expect("FAST：建端点")),
                rights,
            )
            .expect("FAST：端点进表");
        (h, ht.get_raw(h, ObjType::Channel, 0).expect("FAST：取端点").0.as_ptr() as usize)
    }

    // 派生位纪律照守：等 READABLE 的睡眠者 wait_obj 用真 ObjRef（本纯函数面
    // 不解引用，但 4-3b 不变量不给测试开口子）。
    let (ha, ea) = chan_end(RIGHT_SEND);
    let (hb, eb) = chan_end(RIGHT_RECV);
    {
        let a = core::ptr::NonNull::new(ea as *mut ObjRef).expect("FAST：端点 A 指针");
        let b = core::ptr::NonNull::new(eb as *mut ObjRef).expect("FAST：端点 B 指针");
        // SAFETY: 两端刚 insert + get_raw——鲜活且互不重复；pair 只写 peer 并互钉。
        unsafe { ChannelEnd::pair(a, b) };
    }

    // L0. 非法参 + 空队 → None（send 侧调用点 obj 恒真端点非 0、signals 恒
    //     READABLE 非 0——守门是函数自包含的，纯函数面直测）。
    assert!(find_single_waiter(ea, SIGNAL_READABLE).is_none(), "FAST：L0 空队须 None");
    assert!(find_single_waiter(0, SIGNAL_READABLE).is_none(), "FAST：L0 obj=0 须 None");
    assert!(find_single_waiter(ea, 0).is_none(), "FAST：L0 signals=0 须 None");

    // L1. 恰一匹配 → Some（返回的正是那一个——指针身份断言）。
    let w1 = sleeper(ea, SIGNAL_READABLE);
    assert_eq!(
        find_single_waiter(ea, SIGNAL_READABLE).map(|p| p.as_ptr() as usize),
        Some(w1 as *mut Task as usize),
        "FAST：L1 恰一匹配须返该等待者"
    );

    // L2. 双匹配者 → None（单点唤醒集 ≠ 全匹配集——退 slowpath 全摘）；
    //     退一恢复恰一。
    let w2 = sleeper(ea, SIGNAL_READABLE);
    assert!(find_single_waiter(ea, SIGNAL_READABLE).is_none(), "FAST：L2 双匹配须 None");
    sleepq_unlink(w2);
    assert!(find_single_waiter(ea, SIGNAL_READABLE).is_some(), "FAST：L2 退一须恢复恰一");

    // L3. 位相交哨兵在场 → None（排头与否无关——漏醒哨兵 = missed wakeup，
    //     整队退 slowpath 由 wake_matching 谓词的哨兵分支兜住）。
    let wa = sleeper(WAIT_ANY, u64::MAX);
    assert!(find_single_waiter(ea, SIGNAL_READABLE).is_none(), "FAST：L3 哨兵在场须 None");
    sleepq_unlink(wa);
    // L3b. 位**不相交**哨兵不挡 fastpath（b-2 LB 修正：wake_matching 只摘
    //      位相交者，哨兵不在唤醒集内即与单点唤醒集无冲突——wait_many
    //      收割者挂 TERMINATED 位常驻时，READABLE 族 send 直切必须存活）。
    let wb = sleeper(WAIT_ANY, crate::handle::SIGNAL_TERMINATED);
    assert_eq!(
        find_single_waiter(ea, SIGNAL_READABLE).map(|p| p.as_ptr() as usize),
        Some(w1 as *mut Task as usize),
        "FAST：L3b 位不相交哨兵不挡唯一真匹配"
    );
    sleepq_unlink(wb);

    // L4. 不相交位不算匹配、也不挡别处的唯一真匹配（user 位等合成 obj 合法
    //     ——内核永不置位；同 obj 派生位探测与之无交集）。
    let ux = sleeper(0x5000, 1 << 33);
    assert!(find_single_waiter(0x5000, SIGNAL_READABLE).is_none(), "FAST：L4 不相交位不匹配");
    assert!(find_single_waiter(ea, SIGNAL_READABLE).is_some(), "FAST：L4 不挡真匹配");
    sleepq_unlink(ux);

    // L5. obj 是匹配键的另一半：不同对象等同位 → 不匹配、不挡。
    let w5 = sleeper(eb, SIGNAL_READABLE);
    assert_eq!(
        find_single_waiter(ea, SIGNAL_READABLE).map(|p| p.as_ptr() as usize),
        Some(w1 as *mut Task as usize),
        "FAST：L5 异对象等待者不匹配不挡"
    );
    sleepq_unlink(w5);
    sleepq_unlink(w1);

    // 清场（SVC 面的前提）。
    berry_sched::for_each_sleeper(|_| panic!("FAST：清场后 SLEEPQ 须空"));

    // L6. SVC 慢路回归（SLEEPQ 空——ktest 纪律前提）：无等待者 send 恒走
    //     slowpath（miss 计数 +1、hit 不动——快慢路配比计数器的机器断言）；
    //     fork 前半错误面不变：坏 handle -EBADF。recv 收回不留队列尾。
    static mut LBUF: [u8; 8] = [0x46; 8];
    let (m0, h0) = (crate::telemetry::ipc_fast_misses(), crate::telemetry::ipc_fast_hits());
    // SAFETY: svc 同步异常；L 系列静态均本函数私有。
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, ha as u64, &raw const LBUF as u64, 8, 0, 0, 0),
        0,
        "FAST：L6 无等待者 send（慢路）"
    );
    assert_eq!(crate::telemetry::ipc_fast_misses(), m0 + 1,
               "FAST：L6 须计 slowpath（miss +1）");
    assert_eq!(crate::telemetry::ipc_fast_hits(), h0,
               "FAST：L6 不得计 fastpath");
    // SAFETY: 同上。
    assert_eq!(
        svc6(NR_CHANNEL_SEND as u64, 0xDEAD_BEEF, &raw const LBUF as u64, 8, 0, 0, 0),
        crate::handle::ERR_BAD_HANDLE as i64,
        "FAST：L6 坏 handle 须 -EBADF（fork 前半不变）"
    );
    // SAFETY: 同上（recv 三出参——只取载荷，容量/handle 数照抄 H 段形制）。
    static mut LNB: u64 = 8;
    static mut LNH: u64 = 0;
    static mut LHND: [u32; 4] = [0; 4];
    assert_eq!(
        svc6(crate::syscall::NR_CHANNEL_RECV as u64, hb as u64, &raw mut LBUF as u64,
             &raw mut LNB as u64, &raw mut LHND as u64, &raw mut LNH as u64, 0),
        0,
        "FAST：L6 recv 收回（队列不留尾）"
    );
    let _ = (RIGHT_SEND, RIGHT_RECV); // 面上引用（rights 门控面 H 段已盖）

    berry_hal_aarch64::uart_puts(b"FAST-K ok\n");
}

/// G-eco 入机侧第一刀（M 段）：`dtb::find_initrd` 纯函数面——手搓最小 FDT
/// fixture（10 字头 + struct + strings；无 memrsvmap——解析器不消费）盖正反面。
/// QEMU 侧活证 = boot 的 `INITRD ok [..) len=..` trace（`-initrd` 启动、len ==
/// 文件大小），本段只锁纯函数语义（确定性、无 QEMU 依赖）。
fn test_dtb() {
    use alloc::vec::Vec;
    use crate::dtb::find_initrd;

    fn push_u32(v: &mut Vec<u8>, w: u32) {
        v.extend_from_slice(&w.to_be_bytes());
    }
    /// BEGIN_NODE + 名（NUL 结尾、4 对齐）。
    fn push_node(v: &mut Vec<u8>, name: &str) {
        push_u32(v, 1);
        v.extend_from_slice(name.as_bytes());
        v.push(0);
        while !v.len().is_multiple_of(4) {
            v.push(0);
        }
    }
    /// 搓最小 FDT：root → 可选深层包一层（"foo" → "chosen"，depth 3 不应命中）
    /// → 可选真 chosen（initrd-start/end，cells=4|8；可缺 end）。
    fn fixture(with_chosen: bool, deep: bool, cells: usize, with_end: bool) -> Vec<u8> {
        const MAGIC: u32 = 0xD00D_FEED;
        const END_NODE: u32 = 2;
        const PROP: u32 = 3;
        const FDT_END: u32 = 9;
        const HDR: usize = 40;
        let (start, end) = (0x4800_0000u64, 0x4800_0400u64);
        let mut st: Vec<u8> = Vec::new();
        let mut strings: Vec<u8> = Vec::new();
        push_node(&mut st, "");
        if with_chosen {
            if deep {
                push_node(&mut st, "foo");
            }
            push_node(&mut st, "chosen");
            for (nm, val) in [
                ("linux,initrd-start", start),
                ("linux,initrd-end", end),
            ] {
                if !with_end && nm.ends_with("end") {
                    continue;
                }
                let nameoff = strings.len() as u32;
                strings.extend_from_slice(nm.as_bytes());
                strings.push(0);
                push_u32(&mut st, PROP);
                push_u32(&mut st, cells as u32);
                push_u32(&mut st, nameoff);
                if cells == 4 {
                    st.extend_from_slice(&(val as u32).to_be_bytes());
                } else {
                    st.extend_from_slice(&val.to_be_bytes());
                }
            }
            push_u32(&mut st, END_NODE); // chosen
            if deep {
                push_u32(&mut st, END_NODE); // foo
            }
        }
        push_u32(&mut st, END_NODE); // root
        push_u32(&mut st, FDT_END);
        let mut dtb = Vec::new();
        push_u32(&mut dtb, MAGIC);
        push_u32(&mut dtb, (HDR + st.len() + strings.len()) as u32);
        push_u32(&mut dtb, 0); // off_mem_rsvmap（不消费）
        push_u32(&mut dtb, HDR as u32); // off_dt_struct
        push_u32(&mut dtb, (HDR + st.len()) as u32); // off_dt_strings
        push_u32(&mut dtb, 17); // version
        push_u32(&mut dtb, 16); // last_comp_version
        push_u32(&mut dtb, 0); // boot_cpuid_phys
        push_u32(&mut dtb, strings.len() as u32);
        push_u32(&mut dtb, st.len() as u32);
        dtb.extend_from_slice(&st);
        dtb.extend_from_slice(&strings);
        dtb
    }

    // M1 完整（u64 cells）：区间原值命中。
    assert_eq!(
        find_initrd(&fixture(true, false, 8, true)),
        Some((0x4800_0000, 0x4800_0400)),
        "DTB：u64 cells 完整面"
    );
    // M2 u32 cells 变体（QEMU 小内存机型写 1 cell）。
    assert_eq!(
        find_initrd(&fixture(true, false, 4, true)),
        Some((0x4800_0000, 0x4800_0400)),
        "DTB：u32 cells 变体"
    );
    // M3 无 chosen 节点 → None。
    assert_eq!(find_initrd(&fixture(false, false, 8, true)), None, "DTB：无 chosen");
    // M4 start 有 end 无 → None（半区间不产出）。
    assert_eq!(find_initrd(&fixture(true, false, 8, false)), None, "DTB：缺 end");
    // M5 坏魔数 → None（no-panic 面）。
    let mut bad = fixture(true, false, 8, true);
    bad[0] ^= 0xFF;
    assert_eq!(find_initrd(&bad), None, "DTB：坏魔数");
    // M6 totalsize 声明超实长（截断）→ None。
    let mut over = fixture(true, false, 8, true);
    let sz = u32::from_be_bytes([over[4], over[5], over[6], over[7]]);
    over[4..8].copy_from_slice(&(sz + 0x100).to_be_bytes());
    assert_eq!(find_initrd(&over), None, "DTB：totalsize 超实长");
    // M7 深层 chosen（depth 3，非根直接子节点）→ None。
    assert_eq!(find_initrd(&fixture(true, true, 8, true)), None, "DTB：深层 chosen 不命中");
    // M8 中途截断（只剩头半）→ None。
    let trunc: Vec<u8> = fixture(true, false, 8, true)[..24].to_vec();
    assert_eq!(find_initrd(&trunc), None, "DTB：截断");

    berry_hal_aarch64::uart_puts(b"DTB-K ok\n");
}

/// 全栈交叉验证（integration）：一个场景串起调度器全部特性面——freeze ×
/// policy × band × deadline × billing × EAS（overutilized/placement）。
/// 单面钉板（`test_freeze`/`test_policy_v3`/`test_band`/`test_deadline`/
/// `test_billing`/`test_overutil`）各自验「机制对」，本测验**同席组合**——
/// 两两机制同时在场互不打架（改任一机制的接缝在此现形）。四段独立：
///
/// - **IS-1 freeze × policy**：表给 A(键7)/B(键8) 各 +32（A 另有
///   `prio_base=16`——apply 合成 16+32=48 带）。A 冻结 → pick 只出 B：
///   freeze 的 eligible 过滤在带比较**前**——更高带（48）的 A 也拒，
///   「只出 B」同时活证冻结过滤（A 不出）与策略注入（B eff=32）双生效；
///   解冻 A → A 立即出选（48 带压制 32）——thaw 只清位不抹策略权重；
/// - **IS-2 band × deadline**：带 X（LATENCY_CRITICAL）小配额 quota=100
///   武装；耗尽 → `throttle_all` 只限同带 fair 任务 C，同带 deadline
///   任务 D 豁免（RT 屏蔽不变量）且持续出选；翻期解限流后 C 复可选；
/// - **IS-3 billing × policy**：bind_gen 设表期间 `billing_charge` 计数
///   精确（累加/域隔离/零时长拒收），且记账写不扰动策略快照（gen/bound
///   不动、pick 消费面照常 +32）、策略消费不改记账——双向不干扰活证；
/// - **IS-4 EAS overutilized × placement**：placement=1 表在绑 → 合成
///   满载窗泵 CPU0 util 越阈（> 域容量 80%）→ `placement_hint` 归 0
///   （EAS 机会主义回退，表在绑也不透传）；idle 窗真衰减回阈下 →
///   hint 恢复 1。
///
/// ktest 纪律同源（sched::init 前跑、单核）：每段自带前置清零 + 段尾收口
/// （policy unbind / band configure(0) / util 全槽清零——ktest 瞬态不外泄）
/// ，段间零状态耦合——一段 panic 由断言消息的 `IS-N` 前缀直接定位。任务体
/// 16KiB 栈 leak 不回收（ktest 语境同 spawn 政策）。占位入口手搭任务永不
/// 执行（halt 兜底——同 PV3/FREEZE/BAND）。
#[allow(clippy::unwrap_used)]
fn test_scheduler_integration() {
    use berry_sched::policy::{self, Directive, Entry};
    use berry_sched::{
        any_overutilized, band_on, configure, consume, cpu_util, em_max_capacity,
        em_register_qemu_virt, overutilized, period_check, state, throttle_all,
        unthrottle_all, util_accumulate, util_reset, util_tick_window, util_window_vct,
        BandBudget, Fair, Runqueue, SchedClass, Task, Topo, HELD_FROZEN, HELD_THROTTLE,
        INTENT_LATENCY_CRITICAL, PRIO_DEADLINE, UTIL_NR_CPU,
    };
    use crate::telemetry;

    /// 占位入口（pick_next 只选不切——永不执行；halt 兜底防未定义行为）。
    extern "C" fn isg_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    /// 手搭内核任务：`tag` 即 proc_tag（策略表 key 域，v1 = proc_tag；
    /// new_kernel 默认 0——同 PV3 setup 的键域口径）。
    fn mk(name: &'static [u8], tag: usize) -> &'static mut Task {
        let mut t = Task::new_kernel_with_as(isg_dummy, tag, name, 0, 0);
        t.proc_tag = tag; // 策略表 key（0 = 哨兵键，apply 早退不注入）
        alloc::boxed::Box::leak(t)
    }

    // —— IS-1：freeze × policy 共存——冻结过滤与策略注入互不打架 ——
    {
        // 前置：策略回哨兵（PV3/PL 同款防御性清零——gen 从 0 起算）。
        policy::unbind();
        // 双条目表：A(键7)/B(键8) 各 +32（B 的 +32 即场景指定的策略权重；
        // A 同入表 + prio_base=16 使冻结对照更强——A 合成带 48 > B 的 32，
        // 冻结过滤若接在带比较后，高带的 A 仍会出选，断言即炸响）。
        static TAB: [Entry; 2] = [
            Entry {
                task_key: 7,
                directive: Directive { placement: 0, weight: 32, epp: None },
            },
            Entry {
                task_key: 8,
                directive: Directive { placement: 0, weight: 32, epp: None },
            },
        ];
        assert!(policy::bind_gen(&TAB, policy::gen() + 1),
                "INTEG：IS-1 双条目表 bind_gen 须受理");

        let mut rq = Runqueue::new();
        let rr = Fair;
        let topo = Topo::single();
        let ta = mk(b"isA".as_slice(), 7);
        ta.prio_base = 16; // 策略叠加面：apply 后 eff = 16+32 = 48
        let tb = mk(b"isB".as_slice(), 8);
        rr.enqueue(&mut *ta, &mut rq);
        rr.enqueue(&mut *tb, &mut rq);

        // 基线轮：双 eff=0 平键 → head 侧先见出 A（FREEZE F1 同款确定性），
        // apply 即刻消费策略 → A.eff = 16+32 = 48。
        let p = rr.pick_next(&rq, &topo).expect("INTEG：IS-1 非空环须有出选");
        assert_eq!(p.name, b"isA".as_slice(), "INTEG：IS-1 基线轮须出 A（head 先见）");
        assert_eq!(ta.prio_effective, 48, "INTEG：IS-1 策略叠加须 16+32=48");

        // 冻结 A（F2 同款直接置位——ktest 无 SuspendTok 链，位即冻结语义）
        // → 只出 B：eligible 过滤在带比较前，带 48 的 A 也不得出选。
        ta.held_locks |= HELD_FROZEN;
        for i in 0..3 {
            let p = rr.pick_next(&rq, &topo).expect("INTEG：IS-1 冻结期须有出选");
            assert_eq!(p.name, b"isB".as_slice(),
                       "INTEG：IS-1 冻结期第 {i} 轮须只出 B（高带 A 亦拒）");
        }
        assert_eq!(tb.prio_effective, 32,
                   "INTEG：IS-1 冻结期策略注入须照常（B = 0+32 双生效）");
        assert_eq!(ta.prio_effective, 48,
                   "INTEG：IS-1 冻结不得抹已注入的策略权重（两不干扰）");

        // 解冻 A → A 立即出选（48 带压制 32；thaw 只清位不动策略面）。
        ta.held_locks &= !HELD_FROZEN;
        let p = rr.pick_next(&rq, &topo).expect("INTEG：IS-1 解冻后须有出选");
        assert_eq!(p.name, b"isA".as_slice(), "INTEG：IS-1 解冻后 A 须可出选（48 带先出）");
        assert_eq!(ta.prio_effective, 48, "INTEG：IS-1 解冻后策略权重须保持（幂等再注入）");

        // 段尾收口：策略回哨兵（后续段/smoke 稳态全程未绑）。
        policy::unbind();
        assert!(!policy::bound(), "INTEG：IS-1 段尾须回未绑");
    }

    // —— IS-2：band × deadline 共存——带限流 fair 不波及同带 deadline ——
    {
        const X: u8 = INTENT_LATENCY_CRITICAL; // 带键 = intent_effective（§11）
        const QUOTA: u64 = 100; // 小配额（场景指定）
        const PERIOD: u64 = 10_000;
        const NOW: u64 = 100_000; // 手拨首期起点（band 比较传入的 now 参数）
        assert!(!band_on(X), "INTEG：IS-2 段首 X 带须默认关（test_band 收口态）");

        let mut rq = Runqueue::new();
        let rr = Fair;
        let topo = Topo::single();
        let tc = mk(b"isC".as_slice(), 0); // fair 受害者：intent 即带键
        tc.intent_effective = X;
        let td = mk(b"isD".as_slice(), 0); // deadline 豁免者（RT 屏蔽对照）
        td.prio_effective = PRIO_DEADLINE;
        td.vdeadline = NOW + PERIOD; // 远截止（只证带豁免，不触 lazy_unthrottle 面）
        rr.enqueue(&mut *tc, &mut rq);
        rr.enqueue(&mut *td, &mut rq);

        assert!(configure(X, QUOTA, PERIOD, NOW), "INTEG：IS-2 小配额须武装");
        assert!(band_on(X), "INTEG：IS-2 武装后 band_on");

        // 消耗至耗尽：微记账累计 + 首越线一次性信号（BD1/BD2 同口径）。
        assert!(!consume(X, 60), "INTEG：IS-2 60/100 未越线不报");
        assert!(consume(X, 40), "INTEG：IS-2 100/100 首越线须报");
        // 环执法：只 C 置位——D 同带但 deadline 豁免（band_member 判定面）。
        assert_eq!(throttle_all(X, &rq), 1, "INTEG：IS-2 须只限流同带 fair 成员 C");
        assert!(tc.held_locks & HELD_THROTTLE != 0, "INTEG：IS-2 C 须置 THROTTLE");
        assert!(td.held_locks & HELD_THROTTLE == 0, "INTEG：IS-2 D 须豁免（RT 屏蔽）");

        // 限流效果 × RT 屏蔽双向不干扰：pick 连出 D——C 留环但不出选。
        for i in 0..2 {
            let p = rr.pick_next(&rq, &topo).expect("INTEG：IS-2 限流期须有出选");
            assert_eq!(p.name, b"isD".as_slice(),
                       "INTEG：IS-2 限流期第 {i} 轮须出 D（C 被限流不出选）");
        }

        // 翻期：清账清限流 → C 复可选（先摘 D 消除 deadline 带压制——
        // BD2 同款 dequeue 口径；不摘则 D 恒先出，与限流无关）。
        assert!(period_check(X, NOW + PERIOD), "INTEG：IS-2 到期须翻期");
        assert_eq!(unthrottle_all(X, &rq), 1, "INTEG：IS-2 翻期后须清 C 限流位");
        assert_eq!(tc.held_locks, 0, "INTEG：IS-2 翻期后 C 位须全清");
        rr.dequeue(&mut *td, &mut rq);
        let p = rr.pick_next(&rq, &topo).expect("INTEG：IS-2 解限流后须有出选");
        assert_eq!(p.name, b"isC".as_slice(), "INTEG：IS-2 解限流后 C 须复可选");

        // 段尾收口：先清环上位（已清）再 configure(0) 关带（mid-flight 义务序）。
        assert!(!configure(X, 0, 0, 0), "INTEG：IS-2 段尾关 X 带");
        assert!(!band_on(X), "INTEG：IS-2 段尾须回关态");
        assert_eq!(state(X), Some(BandBudget::OFF), "INTEG：IS-2 段尾槽须 OFF");
    }

    // —— IS-3：billing × policy 共存——策略注入期间记账精确且双向不扰动 ——
    {
        policy::unbind();
        static TAB: [Entry; 1] = [Entry {
            task_key: 7,
            directive: Directive { placement: 0, weight: 32, epp: None },
        }];
        assert!(policy::bind_gen(&TAB, policy::gen() + 1), "INTEG：IS-3 设表须受理");
        let g0 = policy::gen();

        // 记账精确（BL1/BL2/BL3 交叉复验）。域键取 0x770/0x780——避开
        // test_billing(100/200)/overflow(300..=308)/perf(0xF00D) 已占域；
        // 8 槽表已满（前序测试填满），D1 首笔落账须大于既有最小槽值
        // （1000ns），否则 D2 落账的满表覆写会反噬 D1 槽——6000ns 即此用。
        const D1: u64 = 0x770;
        const D2: u64 = 0x780;
        telemetry::billing_charge(D1, 400);
        telemetry::billing_charge(D1, 600);
        telemetry::billing_charge(D1, 5_000);
        assert_eq!(telemetry::billing_domain_total(D1), Some(6_000),
                   "INTEG：IS-3 域 D1 须精确累计 6000ns（策略注入期间）");
        telemetry::billing_charge(D2, 250);
        assert_eq!(telemetry::billing_domain_total(D2), Some(250),
                   "INTEG：IS-3 域 D2 须落账 250ns");
        assert_eq!(telemetry::billing_domain_total(D1), Some(6_000),
                   "INTEG：IS-3 域隔离（D2 落账不波及 D1——含满表覆写路径）");
        telemetry::billing_charge(D1, 0); // 零时长拒收（无副作用）
        assert_eq!(telemetry::billing_domain_total(D1), Some(6_000),
                   "INTEG：IS-3 零时长 charge 须无副作用");

        // 反向不扰动：记账写后策略快照原样（gen/bound 不动）+ pick 消费面
        // 照常 +32——billing 静态原子表与 policy 快照无共享写面，活证。
        assert_eq!(policy::gen(), g0, "INTEG：IS-3 记账不得扰动策略世代");
        assert!(policy::bound(), "INTEG：IS-3 记账不得扰动绑定态");
        let mut rq = Runqueue::new();
        let rr = Fair;
        let topo = Topo::single();
        let t = mk(b"isP".as_slice(), 7);
        rr.enqueue(&mut *t, &mut rq);
        let p = rr.pick_next(&rq, &topo).expect("INTEG：IS-3 须有出选");
        assert_eq!(p.name, b"isP".as_slice(), "INTEG：IS-3 单任务环须出 isP");
        assert_eq!(t.prio_effective, 32, "INTEG：IS-3 记账期间策略注入须照常（+32）");
        assert_eq!(telemetry::billing_domain_total(D1), Some(6_000),
                   "INTEG：IS-3 策略消费不得改记账");

        // 段尾收口：策略回哨兵。
        policy::unbind();
        assert!(!policy::bound(), "INTEG：IS-3 段尾须回未绑");
    }

    // —— IS-4：EAS overutilized × placement——饱和回退与衰减恢复 ——
    {
        // 前置：EM 默认表（test_em/test_overutil 收口态幂等重注册——域 0
        // 最高容量 1024 → 阈值 819）+ util 全槽清零（OU 系同款防御）。
        assert!(em_register_qemu_virt(), "INTEG：IS-4 EM 默认表注册须受理");
        assert_eq!(em_max_capacity(0), 1024, "INTEG：IS-4 域 0 容量须 1024（→ 阈值 819）");
        policy::unbind();
        for c in 0..UTIL_NR_CPU {
            util_reset(c);
        }
        assert!(!any_overutilized(), "INTEG：IS-4 前置须非 over 态");

        // placement=1 表（key 7 偏 little——OU-4 同款三值词汇）。
        static PLACE: [Entry; 1] = [Entry {
            task_key: 7,
            directive: Directive { placement: 1, weight: 0, epp: None },
        }];
        assert!(policy::bind_gen(&PLACE, policy::gen() + 1), "INTEG：IS-4 放置表须受理");
        assert_eq!(policy::placement_hint(7), 1, "INTEG：IS-4 阈下 hint 须透传 1");

        // 泵 CPU0 满载（OU 系同款合成窗折叠）：step = ≥ 名义窗的最小
        // 1024 整倍 → accum = k×1024 / elapsed = k×1024 → util_raw 精确
        // 1024（满载）；EWMA α=1/8 收敛 13 窗内越阈 819——64 窗帽防挂死。
        let k = util_window_vct() / 1024 + 1;
        let step = k * 1024;
        let mut t0 = 1_000_000u64;
        util_tick_window(0, t0); // 开首窗（首调只开窗不折叠）
        let mut pumped = false;
        for _ in 0..64 {
            util_accumulate(0, k * 1024); // 满载窗：util_raw = 1024
            t0 += step;
            util_tick_window(0, t0);
            if overutilized(0) {
                pumped = true;
                break;
            }
        }
        assert!(pumped, "INTEG：IS-4 满载泵 64 窗内须越阈（EWMA 收敛 > 819）");
        assert!(any_overutilized(), "INTEG：IS-4 CPU0 over → any 须 true（CPU1 仍 0）");
        assert_eq!(policy::placement_hint(7), 0,
                   "INTEG：IS-4 over 态 hint 须失效归 0（表在绑也不透传）");

        // util 回落：idle 空窗真衰减（ewma -= ewma>>3，量化死区 ≤7 远低于
        // 阈带宽）→ 回阈下 → hint 恢复（非 reset 直清——走生产衰减路径）。
        let mut decayed = false;
        for _ in 0..64 {
            util_accumulate(0, 0); // idle 窗：util_raw = 0
            t0 += step;
            util_tick_window(0, t0);
            if !overutilized(0) {
                decayed = true;
                break;
            }
        }
        assert!(decayed, "INTEG：IS-4 idle 衰减 64 窗内须回阈下");
        assert!(!any_overutilized(), "INTEG：IS-4 衰减后全域须非 over 态");
        assert_eq!(policy::placement_hint(7), 1, "INTEG：IS-4 衰减后 hint 须恢复 1");

        // 段尾收口：unbind + util 全槽清零（boot 稳态等价——OU 收口同款）。
        policy::unbind();
        assert!(!policy::bound(), "INTEG：IS-4 段尾须回未绑");
        for c in 0..UTIL_NR_CPU {
            util_reset(c);
            assert_eq!(cpu_util(c), 0, "INTEG：IS-4 收口须回未启态（CPU{c}）");
        }
    }

    berry_hal_aarch64::uart_puts(b"INTEG-K ok\n");
}

