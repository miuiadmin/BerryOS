//! 内核内最小测试骨架（关卡 G0 判据辅助）：在 QEMU 串口打印 PASS/FAIL。
//!
//! 单核、无抢占（G0）下同步直跑；断言失败 → panic（panic handler 打消息 + file:line:col；
//! CPU 异常寄存器/PC/ESR/FAR dump 在 HAL `trap::exception_sync`）。SMP（G2）后需每核隔离。

use berry_mm::va::{phys_to_virt, PAGE_SHIFT};

pub fn run() {
    test_gic();
    test_frame();
    test_heap();
    test_syscall();
    test_switch();
    test_aspace();
    test_vmo();
    test_handle();
    test_policy();
    berry_hal_aarch64::uart_puts(b"ktest done\n");
    // Step 6：异常向量 + TrapFrame save/restore/eret 全链路验证（放最后）。
    test_exc();
}

/// Step 9 信号 + G0 退出 #6 + **3-1 真 ABI 骨架**：从 EL1 发 `svc #0`（EC=0x15）
/// → HAL dispatch → 语义层 handler（`内核/src/syscall.rs`）→ 回写 x0、eret 续行。
/// 用例：调试 echo 往返（G0 原始判据）；未知号 0x99 / 未实现真号 0x04 一律
/// -ENOTSUP（3-1 政策：错误码自理、永不杀调用者）；clock_read（MONOTONIC）两次
/// 读单调；handle 三操作经 syscall 全链路门控（close/duplicate + 旧值/坏 handle 拒）；
/// 3-4a per-process 表拆分（G 段）：两表隔离 + take/insert_ref 迁移原语 + 子进程
/// 出生自带空表；3-4b channel 三号（H 段）：非阻塞队列语义 + handle 随消息迁移
/// 的原子序（「两进程交换一句消息」的 EL0 正向验收在 user_spawner/user_child）。
#[allow(clippy::result_unwrap_used)]
fn test_syscall() {
    use crate::handle::{KernelObj, RIGHT_READ, RIGHT_WRITE};
    use crate::syscall::{
        bootstrap_table, NR_CLOCK_READ, NR_HANDLE_CLOSE, NR_HANDLE_DUPLICATE, NR_VMO_CREATE,
    };
    use crate::vmo::Vmo;
    use berry_hal_aarch64::syscall::NR_DEBUG_ECHO;
    use berry_mm::va::PAGE_SIZE;

    // out 参数缓冲（内核地址；EL1 调用方 → copy_out 走高半直映区）。
    static mut OUT: u64 = 0;

    // A. 调试 echo 往返（G0 原始判据；0xFE 调试槽，非 ABI）。
    assert_eq!(svc4(NR_DEBUG_ECHO as u64, 0x5a, 0, 0), 0x5a, "SYSCALL：echo 往返");

    // B. 未知号（越界 0x99）与未实现真号（object_wait_many 0x05，P4 阻塞等待
    //    同批）一律 -ENOTSUP 存活——3-1 撤「畸形号杀」政策的 EL1 侧正向断言
    //    （3-2 起 0x11 已实现、3-4c 起 0x04/0x06/0x54 已实现，未实现样本换 0x05）。
    assert_eq!(svc4(0x99, 0, 0, 0), -95, "SYSCALL：未知号须 -ENOTSUP");
    assert_eq!(
        svc4(0x05, 0, 0, 0),
        -95,
        "SYSCALL：未实现真号须 -ENOTSUP"
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
        svc4(NR_HANDLE_DUPLICATE as u64, h2 as u64, RIGHT_READ as u64, &raw mut OUT as u64),
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
        NR_VMO_DECOMMIT, NR_VMO_READ, NR_VMO_WRITE,
    };
    use crate::vmar::Vmar;
    static mut EBUF: [u8; 16] = [0; 16]; // write 源 / read 目的（内核直映区）
    const PATTERN: u64 = 0x00C0_FFEE_0000_0055;

    // E0. 参数面：size=0 / options≠0 → -EINVAL；超尺寸 → -ENOMEM（no-panic 防线）。
    assert_eq!(svc6(NR_VMO_CREATE as u64, 0, 0, &raw mut OUT as u64, 0, 0, 0), -22,
        "SYSCALL：vmo_create size=0 须 -EINVAL");
    assert_eq!(svc6(NR_VMO_CREATE as u64, PAGE_SIZE as u64, 1, &raw mut OUT as u64, 0, 0, 0), -22,
        "SYSCALL：vmo_create options≠0 须 -EINVAL");
    assert_eq!(
        svc6(NR_VMO_CREATE as u64, u64::MAX, 0, &raw mut OUT as u64, 0, 0, 0),
        -12,
        "SYSCALL：巨 size 须 -ENOMEM（防堆分配 abort）"
    );

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

    // F. 3-3 process/thread：对象面 + 错误面（ktest 在 berry_sched::init 之前跑，
    //    process_start 成功路径会 spawn → 不能在此验——happy path 由 EL0
    //    `user_spawner` 承担，watchdog 错杀断言兜底）。
    use crate::syscall::{NR_PROCESS_CREATE, NR_PROCESS_START, NR_THREAD_CREATE};
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

    // F5. 清场（顺带覆盖钉住释放链：close 线程 → 释放对 Process 的钉住；
    //     close 进程 → 释放对 root Vmar 的钉住；空进程无映射，close 不受
    //     -EBUSY 面影响）。
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hth as u64, 0, 0), 0, "SYSCALL：close thread");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hp as u64, 0, 0), 0, "SYSCALL：close process");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hv2 as u64, 0, 0), 0, "SYSCALL：close child vmar");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, hp2 as u64, 0, 0), 0, "SYSCALL：close process 2");
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
    static mut H_HND: [u32; 4] = [0; 4]; // 随行 handle 数组（u32 × CHAN_HND_MAX）

    // H0. create：两端 handle 均非零互异；rights 面 SEND|RECV 可门控查得。
    // SAFETY: svc 同步异常；H 系列静态均本函数私有。
    unsafe { OUT = 0; CHILD_OUT = 0 };
    assert_eq!(
        svc4(NR_CHANNEL_CREATE as u64, &raw mut OUT as u64, &raw mut CHILD_OUT as u64, 0),
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
    unsafe { HBUF = [0; 320]; H_NB = 8; H_NH = 4; H_HND = [0; 4] };
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
             &raw const H_HND as u64, 5, 0),
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
        svc4(NR_CHANNEL_CREATE as u64, &raw mut OUT as u64, &raw mut CHILD_OUT as u64, 0),
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
        svc4(NR_HANDLE_DUPLICATE as u64, hc0 as u64, RIGHT_RECV as u64, &raw mut OUT as u64),
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
    unsafe { H_HND = [hsrc, 0, 0, 0]; HBUF[..8].copy_from_slice(&PATTERN.to_le_bytes()) };
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
    unsafe { HBUF = [0; 320]; H_NB = 8; H_NH = 4; H_HND = [0; 4] };
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
    unsafe { H_HND = [hgd, 0xDEAD_BEEF, 0, 0] };
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
    unsafe { H_HND = [hgd, hgd, 0, 0] };
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
    //    user_waiter 三路 + waker2，见 main.rs/user.rs）。
    use crate::handle::{RIGHT_DUPLICATE, RIGHT_SIGNAL};
    use crate::syscall::{NR_EVENT_CREATE, NR_OBJECT_SIGNAL, NR_OBJECT_WAIT_ONE};
    static mut I_OBS: u64 = 0; // *observed 出参

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
        svc5(NR_OBJECT_WAIT_ONE as u64, he as u64, 0b0110, 0, &raw mut I_OBS as u64),
        -11,
        "SYSCALL：未命中 wait 须 -EAGAIN"
    );
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, he as u64, 0b0110, 1, &raw mut I_OBS as u64),
        -110,
        "SYSCALL：deadline 已过未命中须 -ETIMEDOUT（不睡）"
    );

    // I2. signal 置位 → wait 命中回写**全满足集**（非请求子集——Zircon 同形）。
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, he as u64, 0, 0b0110),
        0,
        "SYSCALL：object_signal 置位"
    );
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, he as u64, 0b0010, 0, &raw mut I_OBS as u64),
        0,
        "SYSCALL：命中 wait"
    );
    assert_eq!(unsafe { I_OBS }, 0b0110, "SYSCALL：observed 须回写全满足集");

    // I3. 请求位未置 → -EAGAIN（掩码里没有 0b1000）；signal 清位后再查原位
    //     → -EAGAIN（clear 生效）。
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, he as u64, 0b1000, 0, &raw mut I_OBS as u64),
        -11,
        "SYSCALL：请求未置位须 -EAGAIN"
    );
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, he as u64, 0b0010, 0),
        0,
        "SYSCALL：object_signal 清位"
    );
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, he as u64, 0b0010, 0, &raw mut I_OBS as u64),
        -11,
        "SYSCALL：清位后原位须 -EAGAIN"
    );

    // I4. 权利面：dup 收紧到仅 DUPLICATE（无 SIGNAL——零权利 dup 会被表拒，
    //     这是 duplicate 的空 rights 拒面，非本段对象）→ signal 拒 -EACCES；
    //     wait 不需权利位——同一对象剩余位 0b0100 照常可查。
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, he as u64, 0, 0b0100),
        0,
        "SYSCALL：signal 补置 0b0100"
    );
    // SAFETY: 同上。
    unsafe { OUT = 0 };
    assert_eq!(
        svc4(NR_HANDLE_DUPLICATE as u64, he as u64, RIGHT_DUPLICATE as u64, &raw mut OUT as u64),
        0,
        "SYSCALL：dup Event 到仅 DUPLICATE"
    );
    let he_ro = unsafe { OUT } as u32;
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, he_ro as u64, 0, 0b1000),
        -13,
        "SYSCALL：无 SIGNAL 位须 -EACCES"
    );
    // SAFETY: 同上。
    unsafe { I_OBS = 0 };
    assert_eq!(
        svc5(NR_OBJECT_WAIT_ONE as u64, he_ro as u64, 0b0100, 0, &raw mut I_OBS as u64),
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
    // 清场（别名先关、本体后关——refs 归零释放）。
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, he_ro as u64, 0, 0), 0, "SYSCALL：close he_ro");
    assert_eq!(svc4(NR_HANDLE_CLOSE as u64, he as u64, 0, 0), 0, "SYSCALL：close he");
    assert_eq!(
        svc4(NR_OBJECT_SIGNAL as u64, he as u64, 0, 1),
        -9,
        "SYSCALL：close 后 signal 须 -EBADF"
    );

    berry_hal_aarch64::uart_puts(b"SYSCALL ok\n");
}

/// `svc #0` 四参往返（ktest 专用）：x0=nr（进）/ret（出）、x1..x3=参。
/// DAIF 屏蔽不影响同步异常；NZCV 经 SPSR 在 entry/eret 保存/恢复。
/// 安全外壳：3-1 起 syscall 派发不 panic、永不杀调用者（错误码自理）。
fn svc4(nr: u64, a1: u64, a2: u64, a3: u64) -> i64 {
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

/// `svc #0` 六参往返（ktest 专用；3-2 VMO/Vmar 全链用）：x0=nr（进）/ret（出）、
/// x1..x6=参（ABI §2 上限 6 参）。
fn svc6(nr: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> i64 {
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
    // SAFETY: 当前 DAIF 屏蔽（boot 起 IRQ 未解蔽，ktest 在 enable_irq 之前），
    // 满足 context_switch 的 `PSTATE.I==1` 不变式。两 ctx 不别名。
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
    berry_hal_aarch64::uart_puts(b"TASK ok\n");
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
#[allow(clippy::result_unwrap_used)]
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
        "VMO：未提交页必须拒读（v1 无 demand paging）"
    );

    // B. map 即填表 + §5.1 验收：内核经 VMO 映射读写一页（VA 视角）。
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

    // 收尾：回内核空表（与 idle/无 AS 任务的 ttbr0=0 状态对齐）。
    hal::activate_aspace(0, 0);
    berry_hal_aarch64::uart_puts(b"VMO ok\n");
}

/// 3-0：handle 表 + rights 语义（`能力模型设计.md` §2/§4，决策 #28 位域）——
/// insert/get（查表→类型→权限三门控）、duplicate 只紧不松、close 后旧值必拒
/// （引用共享仍在）、槽位重用代号 +1（旧 handle 不复活）、replace 原子搬迁、
/// 坏 handle 面（0/野值）。
#[allow(clippy::result_unwrap_used)]
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

    // 收尾：全关——h2b 的 close 使第一对象引用归零（走最后释放路径）。
    ht.close(h2b).expect("HANDLE：close h2b");
    ht.close(h4).expect("HANDLE：close h4");

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
/// - **C 对照表**（p7 权重 +32 / EPP=Perf）：p7 被选中时字段**必须真的变了**
///   （apply 的反向控制——防留桩成死代码的假等价），但**选人序列与基线全同**
///   （RR 不看优先级 = v1 护栏：注入面只写 TCB 表示法字段，绝不碰机制决策）；
/// - **收尾 unbind**：恢复全局缺席态——之后 smoke 稳态全程在未绑态跑。
///
/// 进程调度 crate 无 host 测试面（HAL aarch64 asm），QEMU ktest 是唯一家。
#[allow(clippy::result_unwrap_used)]
fn test_policy() {
    use alloc::vec::Vec;
    use berry_sched::policy::{self, Directive, Entry};
    use berry_sched::{EppHint, RoundRobin, Runqueue, SchedClass, Task, Topo};

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
        let rr = RoundRobin;
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

    // C. 对照表：p7 权重 +32 / EPP=Perf——字段真变（反向控制）+ 序列不变（护栏）。
    static CTRL: [Entry; 1] = [Entry {
        task_key: 7,
        directive: Directive { placement: 0, weight: 32, epp: Some(EppHint::Perf) },
    }];
    policy::bind(&CTRL);
    let (rq_c, rr_c, topo_c, _tc) = setup();
    let ctrl = probe(&rq_c, &rr_c, &topo_c);
    let mut p7_seen = 0;
    for i in 0..ROUNDS {
        assert_eq!(base[i].0, ctrl[i].0, "POLICY：注入不得改 RR 选人序列（第 {} 轮）", i);
        let (name, eff, epp) = ctrl[i];
        if name == b"p7".as_slice() {
            p7_seen += 1;
            assert_eq!(eff, 32, "POLICY：p7 被选中时 prio_effective 须 = base+32（第 {} 轮）", i);
            assert_eq!(epp, EppHint::Perf, "POLICY：p7 的 EPP 覆盖须生效（第 {} 轮）", i);
        } else {
            assert_eq!(eff, 0, "POLICY：未命中者零写入（第 {} 轮 {}）", i, core::str::from_utf8(name).unwrap_or("?"));
            assert_eq!(epp, EppHint::Efficiency, "POLICY：未命中者 EPP 保持默认（第 {} 轮）", i);
        }
    }
    assert!(p7_seen >= 3, "POLICY：12 轮三轮转 p7 至少被选 3 次（实得 {}）", p7_seen);

    // 收尾：恢复缺席态（之后 smoke 稳态全程未绑——等价性延续到真实调度路径）。
    policy::unbind();
    assert!(!policy::bound(), "POLICY：收尾须回到未绑");

    // 任务体 16KiB 栈 × 9 泄漏不回收（ktest 语境同 sched spawn 政策——v1 无 reap）。
    berry_hal_aarch64::uart_puts(b"POLICY ok\n");
}
