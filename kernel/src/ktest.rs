//! 内核内最小测试骨架（关卡 G0 判据辅助）：在 QEMU 串口打印 PASS/FAIL。
//!
//! 单核、无抢占（G0）下同步直跑；断言失败 → panic（panic handler 打消息 + file:line:col；
//! CPU 异常寄存器/PC/ESR/FAR dump 在 HAL `trap::exception_sync`）。SMP（G2）后需每核隔离。

use berry_mm::va::{phys_to_virt, PAGE_SHIFT};

pub fn run() {
    test_gic();
    test_frame();
    test_memmap();
    test_heap();
    test_syscall();
    test_switch();
    test_aspace();
    test_vmo();
    test_handle();
    test_policy();
    test_rq_ring2();
    test_rq_ring3();
    test_pi();
    test_intent();
    test_fast();
    test_dtb();
    berry_hal_aarch64::uart_puts(b"ktest done\n");
    // Step 6：异常向量 + TrapFrame save/restore/eret 全链路验证（放最后）。
    test_exc();
}

/// Step 9 信号 + G0 退出 #6 + **3-1 真 ABI 骨架**：从 EL1 发 `svc #0`（EC=0x15）
/// → HAL dispatch → 语义层 handler（`kernel/src/syscall.rs`）→ 回写 x0、eret 续行。
/// 用例：调试槽退役面（4-5a：0xFE/0xFF 随 console 服务落地回落 -ENOTSUP——
/// EL0 输出改走 channel_send 真 IPC）；未知号 0x99 / 未实现真号 0x35 一律
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
    use berry_mm::va::PAGE_SIZE;

    // out 参数缓冲（内核地址；EL1 调用方 → copy_out 走高半直映区）。
    static mut OUT: u64 = 0;

    // A. 调试槽退役面（4-5a）：0xFE/0xFF 原调试 echo/putchar 槽随 console 服务
    //    落地退役——一律 -ENOTSUP 存活（EL0 打印改走 channel_send 真 IPC）。
    assert_eq!(svc4(0xFE, 0x5a, 0, 0), -95, "SYSCALL：0xFE 调试槽须已退役");
    assert_eq!(svc4(0xFF, 0x75, 0, 0), -95, "SYSCALL：0xFF 调试槽须已退役");

    // B. 未知号（越界 0x99）与未实现真号（thread_start 0x35，v2 多线程面）
    //    一律 -ENOTSUP 存活——3-1 撤「畸形号杀」政策的 EL1 侧正向断言
    //    （3-2 起 0x11、3-4c 起 0x04/0x06/0x54、4-2b 起 0x05、Plan B 起 0x33/0x36
    //    已实现——**勿再拿 0x33 当样本：已实现后会真退出杀掉 ktest**）。
    assert_eq!(svc4(0x99, 0, 0, 0), -95, "SYSCALL：未知号须 -ENOTSUP");
    assert_eq!(
        svc4(0x35, 0, 0, 0),
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
        NR_VMO_DECOMMIT, NR_VMO_GET_PHYS, NR_VMO_GET_SIZE, NR_VMO_READ, NR_VMO_WRITE,
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
    assert!(pa0 >= 0x4000_0000 && pa0 < 0x4000_0000 + 128 * 1024 * 1024,
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
        svc4(NR_HANDLE_DUPLICATE as u64, he as u64, RIGHT_DUPLICATE as u64, &raw mut OUT as u64),
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
    // 后占尽，保留区 4 页永不出现。
    let base = 0x4000_0000usize;
    let mut sfree = [false; 64];
    let mut sorder = [0u8; 64];
    let mut sfa = berry_mm::FrameAllocator::new(base, base + 64 * 4096, &mut sfree, &mut sorder);
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
/// - **C 对照表**（p7 权重 +32 / EPP=Perf）：4-3a 起 pick 按 `prio_effective`
///   出选（§3.6「调度决策一律看 effective」），注入**真消费**——首轮（三任务
///   eff 同 0，出选同基线 head）apply 写入 p7 eff=32 后，后续各轮 p7 以最高
///   effective **独占出选**（跨层严格优先的机器断言；apply 的反向控制——
///   防留桩成死代码的假等价）。字段断言：p7 恒 = base+32 / EPP=Perf；
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

    // 收尾：恢复缺席态（之后 smoke 稳态全程未绑——等价性延续到真实调度路径）。
    policy::unbind();
    assert!(!policy::bound(), "POLICY：收尾须回到未绑");

    // 任务体 16KiB 栈 × 9 泄漏不回收（ktest 语境同 sched spawn 政策——v1 无 reap）。
    berry_hal_aarch64::uart_puts(b"POLICY ok\n");
}

/// P4 收官审计实踩的 runqueue 摘环回归：`unlink` 旧单元素判据 `prev == next`
/// 在**两元环** {X,Y} 上同样为真（X.rq_prev == X.rq_next == Y）→ 摘 X 误走
/// 空环分支把 head 清 None，幸存者 Y 带 stale 链**静默失联**（「已就绪却不再
/// 被调度」）。现矩阵常驻环 ≥6 打不中；矩阵收缩（任务批量 exit）即踩中。
/// 判据已改锚定 task 自身（`prev == task && next == task`），本用例机器断言
/// 兜底。本地环手搭（不动全局 RUNQUEUE——ktest 在 `berry_sched::init` 前）；
/// 非 smoke 标记（TAGTAB 模式：断言炸 = panic = 后续标记缺失兜底）。
#[allow(clippy::result_unwrap_used)]
fn test_rq_ring2() {
    use berry_sched::{RoundRobin, Runqueue, SchedClass, Task, Topo};

    /// 占位入口（pick_next 只选不切——永不执行；halt 兜底防未定义行为）。
    extern "C" fn rq_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    fn mk(name: &'static [u8]) -> &'static mut Task {
        alloc::boxed::Box::leak(Task::new_kernel_with_as(rq_dummy, 0, name, 0, 0))
    }

    let mut rq = Runqueue::new();
    let rr = RoundRobin;
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
    use berry_sched::{linkable, RoundRobin, Runqueue, SchedClass, Task, Topo};

    /// 占位入口（pick_next 只选不切——永不执行；halt 兜底防未定义行为）。
    extern "C" fn rq3_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    fn mk() -> &'static mut Task {
        alloc::boxed::Box::leak(Task::new_kernel_with_as(rq3_dummy, 0, b"r3".as_slice(), 0, 0))
    }

    let mut rq = Runqueue::new();
    let rr = RoundRobin;
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
#[allow(clippy::result_unwrap_used)]
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
#[allow(clippy::result_unwrap_used)]
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

/// 4-4：fastpath 检查链的 SLEEPQ 侧纯函数面（`find_single_waiter`）+ send 尾部
/// fork 的慢路回归。**ktest 纪律（sched::init 前跑）**：手搭睡眠者在 SLEEPQ
/// 期间绝不可走 SVC send——fastpath 分支会 `fast_wake` 进尚未启用的全局
/// RUNQUEUE + `try_preempt` 解引用 null CURRENT = 挂死；SVC 面只在清场后跑
///（慢路分支无等待者，安全）。EL0 全链 + fastpath/直切/RT 实测在 main.rs
/// fa_ping/fa_pong 对打（`FAST ok`）。
#[allow(clippy::result_unwrap_used)]
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

    // L3. 哨兵在场 → None（排头与否无关——漏醒哨兵 = missed wakeup，整队退
    //     slowpath 由 wake_matching 谓词的哨兵分支兜住）。
    let wa = sleeper(WAIT_ANY, u64::MAX);
    assert!(find_single_waiter(ea, SIGNAL_READABLE).is_none(), "FAST：L3 哨兵在场须 None");
    sleepq_unlink(wa);

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
        while v.len() % 4 != 0 {
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

