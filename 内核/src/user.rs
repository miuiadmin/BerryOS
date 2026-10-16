//! 首个 EL0 用户进程（2b-4 收官步 + G1.5 隔离关卡，`内存管理设计.md` §5.1 第 5 步；
//! 3-2 改板：good 经真 syscall map 读写自己的 VMO = Phase 3 里程碑完全体验收）。
//!
//! 用户程序 = 内核 rodata 里的机器码模板（`global_asm!` 生成，boot 期整块拷进 VMO
//! 代码页后执行——程序内分支全为短距局部跳转/`movz` 绝对地址，无 PC 相对越界引用，
//! 整块搬迁不破坏寻址）。三个角色：
//! - `good`：入口 `x0` = root Vmar handle（3-2 首能力）。**一次性验收链**（进循环
//!   前）：`vmo_create→vmo_commit→vmo_write→vmar_map→经映射 VA 读写→vmo_read
//!   别名回读→映射中 close 拒(-EBUSY)→RO dup 写拒(-EACCES)→vmar_unmap→
//!   vmo_decommit→close` 全绿才继续；任一步错 → 故意自陷 wild 读（被杀 →
//!   watchdog 错杀断言 → FAIL 暴露）。之后稳态循环：读私有数据页 canary +
//!   `clock_read` 单调自检 + 打 'u'。链子只跑一次——handle 槽位代号有限
//!   （14 位），循环 create/close 会耗尽 gen；
//! - `rogue_ptr`：读未映射 VA 0x4000_0000（D-abort from EL0 → 杀）；
//! - `abuser`（原 rogue_svc，3-1 改板）：**真号 + 垃圾 handle 滥用**——`handle_close`
//!   (0x01) 传 0xDEAD_BEEF → 必须 -EBADF 且**存活**（syscall 层错误一律错误码自理，
//!   撤 2b-4 杀政策）；打 'S' 作存活金丝雀；
//! - `spawner`（3-3/3-4 验收）：**用户进程创建第二个进程 + 双向能力迁移**——
//!   `process_create` 拿双 handle（process + 子 root Vmar）→ vmo_create/commit/
//!   write（写入内嵌子代码字节）→ `vmar_map` 把代码+栈映进子空间 → 建礼物 VMO
//!   写魔数 → `channel_create` 建端点对 → `channel_send` 把 **PING + 礼物
//!   handle** 随消息发进对端队列（**handle 随消息迁出本表**——close 原值必
//!   -EBADF）→ `thread_create` → `process_start(…, bootstrap=end1)`（端点 handle
//!   迁入子表，close 原值必 -EBADF）。子进程（`user_child`）凭**自己表里**的
//!   端点 recv 出 PING+礼物 → `vmo_read` 出魔数比对 → 回 REPLY → 打 'C'；
//!   spawner 稳态发 PING、收 REPLY 验魔数 + 打 's'（**两进程经 channel 交换
//!   消息**；非阻塞：空 -EAGAIN / 满 -EBUSY 双侧容忍，无死锁）。任一步错 →
//!   自陷被杀 → watchdog 错杀断言 FAIL。
//! rogue_ptr 应被杀且**只死自己**（`user_fault_exit`）；abuser 与 good 全程存活。
//! [`watchdog`] 内核任务断言杀数==1 + switches/console 排水金丝雀仍在涨 → 打
//! `G1.5 ok`（smoke 固化标记）后退出。
//!
//! 4-5a：**EL0 打印全部改走真 IPC**——各模板 0xFF 调试槽 putchar 退役，改
//! `channel_send(console 发端, buf, len)`（console 服务任务排空落串口；满
//! -EBUSY 重试，EL0 可抢占有界）。console handle 来源：good/waiter/pinger 表
//! append 定值 0x0001_0002、ponger 0x0001_0003（boot 断言 + 模板硬编互为防线）、
//! abuser/spawner 入口 x0（首插 0x0001_0001）、child 经首条 PING 随行令牌
//! （运行期值）。

use crate::aspace::alloc_asid;
use crate::vmar::{Perm, Vmar};
use crate::vmo::Vmo;
use berry_mm::frame::FrameAllocator;
use berry_mm::va::PAGE_SIZE;

/// 用户 VA 布局（每个用户进程同构）：code @0x1000、data @0x2000、stack @
/// [0x3000, 0x4000)。全 UserRw、可执行（无 NX——v2 批次开，见安全设计）。
pub const USER_CODE_VA: usize = 0x1000;
pub const USER_DATA_VA: usize = 0x2000;
pub const USER_STACK_BASE: usize = 0x3000;
pub const USER_STACK_TOP: usize = 0x4000; // sp0（16 对齐）

/// 数据页 canary（非零——good 的「页内容正确」自检依据）。
pub const USER_MAGIC: u64 = 0x00C0_FFEe_E000_0E10;

// ---- 用户程序机器码模板（global_asm 生成；ARM64）----
//
// 约定：入口 x0=arg。good 的 arg = root Vmar handle（3-2 首能力，boot 从引导
// 表插出）；节流阈值改 baked 常量（x0 不再当阈值用）。数字标签（0:/1:/3:）均为
// 函数内短距跳转，模板搬迁后依然成立；spawner 另用 `adr` 取**同页**内嵌数据
// （自相对、不越页——搬迁安全性同源，见 `user_spawner_code`）。
//
// good 的暂存槽（数据页 0x2000 内，canary@0x2000/clock@0x2008 之外）：
//   0x2010 vaddr in-out 槽 / 0x2018 handle out 槽 / 0x2020 pattern / 0x2028 回读
// （spawner 同页沿用该暂存区，布局见其模板注释）。
core::arch::global_asm!(
    ".section .rodata",
    ".balign 16",
    ".globl __user_good",
    "__user_good:",
    "    mov  x20, x0",                 // x20 常驻：root Vmar handle（首能力）
    // ---- 3-2 一次性验收链：map 读写自己的 VMO（Phase 3 里程碑完全体）----
    "    movz x0, #0x11",               // vmo_create(size, options, *out)
    "    movz x1, #0x1000",             // size = 1 页
    "    mov  x2, xzr",                 // options = 0
    "    movz x3, #0x2018",             // *out = handle 槽
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x8, [x3]",                // x8 = 新 VMO handle
    "    cbz  x8, 3f",
    "    movz x0, #0x12",               // vmo_commit(h, 0, 1 页)——write 的前提
    "    mov  x1, x8",
    "    mov  x2, xzr",
    "    movz x3, #0x1000",             // len（暂借 x3——槽基 0x2018 稍后重载）
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x3, #0x2018",             // 重载 handle 槽基（commit 借过 x3）
    "    movz x4, #0xBEEF",             // pattern = 0xC0DE_BEEF
    "    movk x4, #0xC0DE, lsl #16",
    "    str  x4, [x3, #8]",            // 存 0x2020（x3 = 0x2018 重载后）
    "    movz x0, #0x15",               // vmo_write(h, buf, offset, len)
    "    mov  x1, x8",
    "    add  x2, x3, #8",              // buf = 0x2020
    "    mov  x3, xzr",                 // offset = 0
    "    mov  x4, #8",                  // len = 8
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x5, #0x2010",             // vaddr 槽 ← 请求 VA 0x5000
    "    movz x6, #0x5000",
    "    str  x6, [x5]",
    "    movz x0, #0x16",               // vmar_map(vmar, vmo, off, *vaddr, len, rights)
    "    mov  x1, x20",
    "    mov  x2, x8",
    "    mov  x3, xzr",                 // vmo_offset = 0
    "    mov  x4, x5",                  // *vaddr = 0x2010
    "    movz x5, #0x1000",             // len = 1 页
    "    mov  x6, #3",                  // rights = READ|WRITE
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x4, #0x5000",             // 经映射 VA 读，应见 pattern（别名一致）
    "    ldr  x5, [x4]",
    "    movz x6, #0xBEEF",
    "    movk x6, #0xC0DE, lsl #16",
    "    cmp  x5, x6",
    "    b.ne 3f",
    "    movz x5, #0x1111",             // 经映射 VA 改写 0x2222_1111
    "    movk x5, #0x2222, lsl #16",
    "    str  x5, [x4]",
    "    movz x0, #0x14",               // vmo_read(h, buf, offset, len) 回读比对
    "    mov  x1, x8",
    "    movz x2, #0x2028",             // buf = 回读槽
    "    mov  x3, xzr",
    "    mov  x4, #8",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x6, #0x2028",
    "    ldr  x7, [x6]",
    "    cmp  x7, x5",
    "    b.ne 3f",
    "    movz x0, #0x01",               // 映射中 close 必须 -EBUSY（悬空 PTE 防线）
    "    mov  x1, x8",
    "    svc  #0",
    "    cmn  x0, #16",
    "    b.ne 3f",
    "    movz x0, #0x02",               // duplicate 收紧到 READ-only
    "    mov  x1, x8",
    "    mov  x2, #1",                  // RIGHT_READ
    "    movz x3, #0x2018",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x6, #0x2018",
    "    ldr  x7, [x6]",                // x7 = RO handle
    "    movz x0, #0x15",               // RO 上 vmo_write 必须 -EACCES（能力拒绝面）
    "    mov  x1, x7",
    "    movz x2, #0x2020",
    "    mov  x3, xzr",
    "    mov  x4, #8",
    "    svc  #0",
    "    cmn  x0, #13",
    "    b.ne 3f",
    "    movz x0, #0x01",               // close RO
    "    mov  x1, x7",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x17",               // vmar_unmap(vmar, 0x5000, 1 页)
    "    mov  x1, x20",
    "    movz x2, #0x5000",
    "    movz x3, #0x1000",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x13",               // vmo_decommit(h, 0, 1 页)——页已无映射
    "    mov  x1, x8",
    "    mov  x2, xzr",
    "    movz x3, #0x1000",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x01",               // close VMO（此刻应成功）
    "    mov  x1, x8",
    "    svc  #0",
    "    cbnz x0, 3f",
    // ---- 稳态循环：节流 + canary + clock 单调 + 'u' ----
    "    movz x9, #0x30, lsl #16",      // 节流阈值 ~3.1M（baked，与 abuser 同量级）
    "0:  mov  x10, xzr",                // 迭代计数
    "1:  add  x10, x10, #1",
    "    cmp  x10, x9",
    "    b.lo 1b",
    "    movz x3, #0x2000",             // 数据页 canary
    "    ldr  x4, [x3]",
    "    cbz  x4, 3f",                  // 页内容坏 → 自陷路径（被杀 = FAIL 暴露）
    "    movz x2, #0x2008",             // clock_read 的 out = 数据页+8（x2 = 第 2 参）
    "    mov  x0, #0x61",               // NR_CLOCK_READ（真 ABI 号）
    "    mov  x1, xzr",                 // CLOCK_MONOTONIC（x1 = 第 1 参）
    "    svc  #0",                      // x0 = status
    "    cbnz x0, 3f",
    "    ldr  x5, [x2]",                // t1（纳秒）
    "    cbz  x5, 3f",
    "    mov  x0, #0x61",               // 再读一次（dispatch 只回写 x0，x1/x2 未变）
    "    mov  x1, xzr",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x6, [x2]",                // t2
    "    cmp  x6, x5",
    "    b.lo 3f",                      // 单调性破坏 → 自陷路径
    "    movz x6, #0x75",               // 'u' → 数据页 0x2030（链槽之后再占）
    "    movz x5, #0x2030",
    "    str  x6, [x5]",
    "4:  movz x0, #0x52",               // send(console, 0x2030, 1, 0, 0)——4-5a
    "    movz x1, #2",                  // console handle 0x0001_0002（boot 断言定值）
    "    movk x1, #1, lsl #16",
    "    movz x2, #0x2030",
    "    mov  x3, #1",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 5f",
    "    cmn  x0, #16",                 // -EBUSY：console 消费慢，重试（可抢占）
    "    b.ne 3f",
    "    b    4b",
    "5:  b    0b",
    "3:  movz x4, #0x4000, lsl #16",    // 0x4000_0000 未映射 → D-abort → 杀
    "    ldr  x5, [x4]",
    "    b    .",
    ".globl __user_good_end",
    "__user_good_end:",

    ".balign 16",
    ".globl __user_rogue_ptr",
    "__user_rogue_ptr:",
    "    movz x4, #0x4000, lsl #16",   // 野指针读 → D-abort(lower) → 杀
    "    ldr  x5, [x4]",
    "    b    .",                      // 不应到达（到达 = kill 路径坏）
    ".globl __user_rogue_ptr_end",
    "__user_rogue_ptr_end:",

    ".balign 16",
    ".globl __user_abuser",
    "__user_abuser:",
    "    mov  x22, x0",                 // x22 常驻：console 发端（4-5a 首插 0x0001_0001）
    "    movz x9, #0x30, lsl #16",     // 节流阈值 ~3.1M（与 good 的 arg 同量级）
    "0:  mov  x10, xzr",
    "1:  add  x10, x10, #1",
    "    cmp  x10, x9",
    "    b.lo 1b",
    "    mov  x0, #0x01",              // NR_HANDLE_CLOSE（真 ABI 号）
    "    movz x1, #0xBEEF",            // 垃圾 handle 0xDEAD_BEEF
    "    movk x1, #0xDEAD, lsl #16",
    "    svc  #0",                     // x0 必须 = -EBADF（能力门控拒绝面）
    "    cmn  x0, #9",                 // x0 + 9 == 0 ⟺ x0 == -9
    "    b.ne 3f",                     // 返回值异常 → 自陷（kills>期望 → watchdog FAIL）
    "    movz x6, #0x53",              // 'S' → 数据页 0x2010（4-5a：真 IPC 打印）
    "    movz x5, #0x2010",
    "    str  x6, [x5]",
    "4:  movz x0, #0x52",              // send(console, 0x2010, 1, 0, 0)
    "    mov  x1, x22",
    "    movz x2, #0x2010",
    "    mov  x3, #1",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 5f",
    "    cmn  x0, #16",                // -EBUSY 重试（console 消费慢）
    "    b.ne 3f",
    "    b    4b",
    "5:  b    0b",
    "3:  movz x4, #0x4000, lsl #16",   // 自陷路径（被杀 = FAIL 暴露）
    "    ldr  x5, [x4]",
    "    b    .",
    ".globl __user_abuser_end",
    "__user_abuser_end:",

    // 3-3/3-4 spawner：经真 syscall 创建第二个进程 + **双向能力迁移**（3-4a
    // bootstrap 随 start + 3-4b handle 随消息）。暂存：x16=hp(进程)、x17=hv(子
    // Vmar)、x18=hvmo(子代码+栈)、x19=ht(线程)、x20=hgift(礼物 VMO，send 后
    // 失去)、x21=end0(channel 自留端)、x22=end1(作 bootstrap 迁子)、x23=console
    // 发端（4-5a 入口 x0，随首条 PING 转赠子进程）；槽：
    // 0x2010/0x2018 handle 双出参、0x2020 vmo/线程 out、0x2028 vaddr in-out、
    // 0x2030 礼物 out、0x2038 礼物魔数、0x2040/0x2048 channel 双出参、0x2050
    // PING 载荷、0x2058 回信 buf、0x2060/0x2068 recv 容量 in-out、0x2070 随行
    // handle 数组（u32）。
    ".balign 16",
    ".globl __user_spawner",
    "__user_spawner:",
    "    mov  x23, x0",                 // x23 常驻：console 发端（4-5a 首插 0x0001_0001）
    "    movz x0, #0x31",               // process_create(job=0, options=0, *p, *v)
    "    mov  x1, xzr",
    "    mov  x2, xzr",
    "    movz x3, #0x2010",
    "    movz x4, #0x2018",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x16, [x3]",               // hp
    "    cbz  x16, 3f",
    "    ldr  x17, [x4]",               // hv = 子 root Vmar
    "    cbz  x17, 3f",
    "    movz x0, #0x11",               // vmo_create(size=0x2000, 0, *out=0x2020)
    "    movz x1, #0x2000",             // 两页：子代码 @off0 + 子栈 @off1
    "    mov  x2, xzr",
    "    movz x3, #0x2020",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x18, [x3]",               // hvmo
    "    cbz  x18, 3f",
    "    movz x0, #0x12",               // vmo_commit(hvmo, 0, 0x2000)
    "    mov  x1, x18",
    "    mov  x2, xzr",
    "    movz x3, #0x2000",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x15",               // vmo_write(hvmo, buf=内嵌子代码, 0, 100)
    "    mov  x1, x18",
    "    adr  x2, 4f",                  // 本页内自相对取址（搬迁安全，见模块注）
    "    mov  x3, xzr",
    "    movz x4, #376",                // 子代码 94 指令（字节数精确——尾随零垃圾勿拷）
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x5, #0x2028",             // vaddr 槽 ← 请求子代码 VA 0x1000
    "    movz x6, #0x1000",
    "    str  x6, [x5]",
    "    movz x0, #0x16",               // vmar_map(hv, hvmo, 0, *vaddr, 0x2000, RW)
    "    mov  x1, x17",
    "    mov  x2, x18",
    "    mov  x3, xzr",
    "    mov  x4, x5",
    "    movz x5, #0x2000",
    "    mov  x6, #3",
    "    svc  #0",
    "    cbnz x0, 3f",
    // ---- 礼物 VMO（3-4a bootstrap 迁移）：写魔数，start 时送进子进程表 ----
    "    movz x0, #0x11",               // vmo_create(1 页, 0, *out=0x2030)
    "    movz x1, #0x1000",
    "    mov  x2, xzr",
    "    movz x3, #0x2030",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x20, [x3]",               // hgift
    "    cbz  x20, 3f",
    "    movz x0, #0x12",               // vmo_commit(hgift, 0, 1 页)
    "    mov  x1, x20",
    "    mov  x2, xzr",
    "    movz x3, #0x1000",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x4, #0x0E10",             // x4 = USER_MAGIC（与 good 数据页魔数同源）
    "    movk x4, #0xE000, lsl #16",
    "    movk x4, #0xFFEE, lsl #32",
    "    movk x4, #0x00C0, lsl #48",
    "    movz x6, #0x2038",
    "    str  x4, [x6]",
    "    movz x0, #0x15",               // vmo_write(hgift, 0x2038, 0, 8)
    "    mov  x1, x20",
    "    mov  x2, x6",
    "    mov  x3, xzr",
    "    mov  x4, #8",
    "    svc  #0",
    "    cbnz x0, 3f",
    // ---- channel 对（3-4b）：PING+礼物随消息迁给子，回信走另一向 ----
    //（4-3c 起 create 带 options 首参——options=0 常规通道；ABI 见 系统调用ABI设计 §8）
    "    movz x0, #0x51",               // channel_create(0, *out0=0x2040, *out1=0x2048)
    "    mov  x1, xzr",
    "    movz x2, #0x2040",
    "    movz x3, #0x2048",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x5, #0x2040",
    "    ldr  x21, [x5]",               // end0（自留收件）
    "    cbz  x21, 3f",
    "    ldr  x22, [x5, #8]",           // end1（作 bootstrap 迁子）
    "    cbz  x22, 3f",
    "    movz x4, #0x0E10",             // PING 载荷 = USER_MAGIC @0x2050
    "    movk x4, #0xE000, lsl #16",
    "    movk x4, #0xFFEE, lsl #32",
    "    movk x4, #0x00C0, lsl #48",
    "    movz x6, #0x2050",
    "    str  x4, [x6]",
    "    movz x6, #0x2070",
    "    str  w20, [x6]",               // 随行 handle 数组（u32 LE）[0] = 礼物
    "    movz x0, #0x02",               // handle_duplicate(console, SEND, *out=&数组[1])
    "    mov  x1, x23",
    "    movz x2, #0x100",              // RIGHT_SEND（1<<8）——别名随消息转赠，原件自留打印
    "    add  x3, x6, #4",              // *out = 0x2074：新 handle 直落数组 [1] 槽
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x52",               // send(end0, PING, 8, &随行, 2)——handle 随消息迁出本表
    "    mov  x1, x21",
    "    movz x2, #0x2050",
    "    movz x3, #8",
    "    movz x4, #0x2070",
    "    movz x5, #2",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x01",               // 礼物已随消息送走：close(x20) 必 -EBADF
    "    mov  x1, x20",
    "    svc  #0",
    "    cmn  x0, #9",
    "    b.ne 3f",
    "    movz x0, #0x34",               // thread_create(hp, 0, *out=0x2020)
    "    mov  x1, x16",
    "    mov  x2, xzr",
    "    movz x3, #0x2020",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x19, [x3]",               // ht
    "    cbz  x19, 3f",
    "    movz x0, #0x32",               // process_start(hp, ht, entry=0x1000, sp=0x3000, end1)
    "    mov  x1, x16",
    "    mov  x2, x19",
    "    movz x3, #0x1000",             // 子入口 = 映进的代码页
    "    movz x4, #0x3000",             // 子 sp0 = 映射顶（栈向下长，首写 0x2FF8）
    "    mov  x5, x22",                 // bootstrap = end1（channel 端点迁入子表，子 x0 收到）
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x01",               // 迁移后发送方必须失去：close(x22) → -EBADF
    "    mov  x1, x22",
    "    svc  #0",
    "    cmn  x0, #9",
    "    b.ne 3f",
    // ---- 稳态循环：节流 + 发 PING + 收 REPLY（验魔数）+ 's'（与子进程 'C' 交替）----
    "    movz x8, #0x0E20",             // REPLY 魔数低 32 位常驻（0x…0E20）
    "    movk x8, #0xE000, lsl #16",
    "    movz x9, #0x30, lsl #16",
    "    movz x4, #8",                  // recv caps：nbytes=8 @0x2060 / nhandles=0 @0x2068
    "    movz x5, #0x2060",
    "    str  x4, [x5]",
    "    str  xzr, [x5, #8]",
    "0:  mov  x10, xzr",
    "1:  add  x10, x10, #1",
    "    cmp  x10, x9",
    "    b.lo 1b",
    "    movz x0, #0x52",               // send(end0, PING@0x2050, 8, 0, 0)——稳态心跳
    "    mov  x1, x21",
    "    movz x2, #0x2050",
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 2f",
    "    cmn  x0, #16",                 // -EBUSY（子消费慢，队列满）容忍
    "    b.ne 3f",
    "2:  movz x0, #0x53",               // recv(end0, 0x2058, *8, 0, *0)
    "    mov  x1, x21",
    "    movz x2, #0x2058",
    "    movz x3, #0x2060",
    "    mov  x4, xzr",
    "    movz x5, #0x2068",
    "    svc  #0",
    "    cbz  x0, 9f",
    "    cmn  x0, #11",                 // -EAGAIN（子未回）容忍
    "    b.ne 3f",
    "    b    10f",
    "9:  ldr  w4, [x2]",                // 收到回信：低 32 位 == REPLY 魔数
    "    cmp  w4, w8",
    "    b.ne 3f",
    "10: movz x6, #0x73",              // 's' → 数据页 0x2080（4-5a：真 IPC 打印）
    "    movz x5, #0x2080",
    "    str  x6, [x5]",
    "13: movz x0, #0x52",              // send(console, 0x2080, 1, 0, 0)
    "    mov  x1, x23",
    "    movz x2, #0x2080",
    "    mov  x3, #1",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 14f",
    "    cmn  x0, #16",                // -EBUSY 重试（console 消费慢）
    "    b.ne 3f",
    "    b    13b",
    "14: b    0b",
    "3:  movz x4, #0x4000, lsl #16",    // 自陷路径（被杀 = FAIL 暴露）
    "    ldr  x5, [x4]",
    "    b    .",
    // ---- 子进程代码（数据形态嵌在 spawner 模板内；vmo_write 拷进子 0x1000）----
    // 3-4b：x0 = bootstrap（end1 在**子表**内的 handle）。一次性首链：recv 出
    // PING（验 USER_MAGIC）+ 随行礼物 handle → 凭**自己表里**的 handle vmo_read
    // 出魔数比对（能力随消息跨表流动自证）。稳态：收 PING → 回 REPLY → 打 'C'
    // （空 -EAGAIN / 满 -EBUSY 容忍，与父节流同量级，无死锁）。链断 → 自陷被杀
    // → watchdog 错杀断言 FAIL。子暂存：x19=end1、x20=console 发端（4-5a 随行
    // 令牌 [1]）、x8=PING 低 32 位常驻；槽（栈
    // 页 [0x2000,0x3000) 内）：0x2F90 REPLY 载荷、0x2FA0 收件 buf、0x2FA8/0x2FB0
    // 容量 in-out、0x2FB8 随行 handle 数组、0x2FC0 礼物读出 buf。
    "4:  mov  x19, x0",                 // 保存 end1（子表 handle）
    "    movz x4, #0x0E20",             // REPLY 载荷 = 0x00C0FFEE_0000_0E20 @0x2F90
    "    movk x4, #0xE000, lsl #16",
    "    movk x4, #0xFFEE, lsl #32",
    "    movk x4, #0x00C0, lsl #48",
    "    movz x5, #0x2F90",
    "    str  x4, [x5]",
    "    movz x4, #8",                  // 首链 caps：nbytes=8 / nhandles=2（礼物+console）
    "    str  x4, [x5, #24]",
    "    movz x4, #2",
    "    str  x4, [x5, #32]",
    "7:  movz x0, #0x53",               // recv(end1, 0x2FA0, *8, 0x2FB8, *1)
    "    mov  x1, x19",
    "    movz x2, #0x2FA0",
    "    movz x3, #0x2FA8",
    "    movz x4, #0x2FB8",
    "    movz x5, #0x2FB0",
    "    svc  #0",
    "    cmn  x0, #11",                 // 空重试（消息已预排队——理论立即到）
    "    b.eq 7b",
    "    cbnz x0, 8f",
    "    ldr  w6, [x2]",                // PING 低 32 位 == USER_MAGIC 低 32 位
    "    movz w7, #0x0E10",
    "    movk w7, #0xE000, lsl #16",
    "    cmp  w6, w7",
    "    b.ne 8f",
    "    movz x6, #0x2FB0",
    "    ldr  x7, [x6]",                // 实收 nhandles 必须 == 2（礼物 + console）
    "    cmp  x7, #2",
    "    b.ne 8f",
    "    ldr  w7, [x6, #8]",            // 随行 [0] = 礼物 handle（u32，零扩进 x7）
    "    movz x0, #0x14",               // vmo_read(礼物, 0x2FC0, 0, 8)
    "    mov  x1, x7",
    "    movz x2, #0x2FC0",
    "    mov  x3, xzr",
    "    movz x4, #8",
    "    svc  #0",
    "    cbnz x0, 8f",
    "    ldr  w5, [x2]",                // 礼物内容低 32 位（x2 派发保留）
    "    movz w6, #0x0E10",
    "    movk w6, #0xE000, lsl #16",
    "    cmp  w5, w6",
    "    b.ne 8f",
    "    movz x6, #0x2FB8",
    "    ldr  w20, [x6, #4]",           // 随行 [1] = console 发端（x20 常驻，4-5a）
    // ---- 稳态循环：节流 + 收 PING → 回 REPLY + 'C' ----
    "    movz x8, #0x0E10",             // PING 低 32 位常驻（载荷比对用）
    "    movk x8, #0xE000, lsl #16",
    "    movz x9, #0x30, lsl #16",      // 节流阈值（与诸位用户进程同量级）
    "5:  mov  x10, xzr",
    "6:  add  x10, x10, #1",
    "    cmp  x10, x9",
    "    b.lo 6b",
    "    movz x0, #0x53",               // recv(end1, 0x2FA0, *8, 0x2FB8, *1)
    "    mov  x1, x19",
    "    movz x2, #0x2FA0",
    "    movz x3, #0x2FA8",
    "    movz x4, #0x2FB8",
    "    movz x5, #0x2FB0",
    "    svc  #0",
    "    cbz  x0, 11f",                 // 收到 PING → 验 + 回信
    "    cmn  x0, #11",                 // 空（父未发）容忍
    "    b.ne 8f",
    "    b    12f",
    "11: ldr  w6, [x2]",
    "    cmp  w6, w8",
    "    b.ne 8f",
    "    movz x0, #0x52",               // send(end1, REPLY@0x2F90, 8, 0, 0)
    "    mov  x1, x19",
    "    movz x2, #0x2F90",
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 12f",
    "    cmn  x0, #16",                 // 回信队列满（父消费慢）容忍
    "    b.ne 8f",
    "12: movz x6, #0x43",              // 'C' → 栈页 0x2FD0（4-5a：真 IPC 打印）
    "    movz x5, #0x2FD0",
    "    str  x6, [x5]",
    "15: movz x0, #0x52",              // send(console, 0x2FD0, 1, 0, 0)
    "    mov  x1, x20",
    "    movz x2, #0x2FD0",
    "    mov  x3, #1",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 16f",
    "    cmn  x0, #16",                // -EBUSY 重试（console 消费慢）
    "    b.ne 8f",
    "    b    15b",
    "16: b    5b",
    "8:  movz x4, #0x4000, lsl #16",    // 自陷：迁移/读链断 → 被杀 → watchdog FAIL
    "    ldr  x5, [x4]",
    "    b    .",
    ".globl __user_spawner_end",
    "__user_spawner_end:",

    // 4-1 waiter：阻塞 object_wait_one 的 EL0 三路验收（拆步表：超时/命中双路）。
    // 入口 x0 = 本进程表里的 Event handle（boot 双表挂同一 ObjRef——waker2 持
    // 内核侧表 handle，signal 置位经 wake_matching 唤醒本进程）。三路：
    //   ① 超时：清掩码 → wait(U3, t0+100ms) → -ETIMEDOUT + observed==0 +
    //      t1 ≥ deadline（真睡到点——TCG tick 粗粒度由 deadline 绝对时刻兜住）；
    //   ② poll 命中：signal(UMASK, U2) 置恰 bit1(用户半) → wait(U2|U6 超集,
    //      有 deadline) → 即刻返 0 + observed==U2（**全满足集**＝当前掩码，
    //      非请求子集）；
    //   ③ 长睡被唤醒：清掩码 → wait(U3, t2+10s) 真睡 → waker2 经真 SVC
    //      signal(内核表 handle, U3) → 醒来重验命中 → 0 + observed==U3。
    // 4-2 位空间：Event 位面迁高 32 半——U2=1<<37（0x20<<32）、U3=1<<35
    // （0x8<<32）、超集 = 0x220<<32；清零掩码 = 0xFFFF_FFFF_0000_0000（用户
    // 半全集——低半越界掩码 4-2 起 -EINVAL，旧 !0 清零不再合法）。
    // 槽：0x2008 时钟、0x2010 observed。寄存器纪律：svc 只回写 x0，x1..x6 保
    // 留传入值（路 1 的 deadline 留 x3、δ 留 x6 直接复用）。
    ".balign 16",
    ".globl __user_waiter",
    "__user_waiter:",
    "    mov  x20, x0",                 // x20 常驻：Event handle（waiter 表）
    // ---- 路 1：超时自醒 ----
    "    movz x0, #0x06",               // signal(h, UMASK, 0) 清零（出生必零，防御位）
    "    mov  x1, x20",
    "    movz x2, #0xFFFF, lsl #32",    // x2 = 用户半全集掩码
    "    movk x2, #0xFFFF, lsl #48",
    "    mov  x3, xzr",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x61",               // t0 = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x5, [x2]",                // x5 = t0
    "    movz x6, #0xE100",             // x6 = δ = 1e8 ns（100ms ≥ 1 slice）
    "    movk x6, #0x05F5, lsl #16",
    "    add  x3, x5, x6",              // deadline = t0 + δ
    "    movz x0, #0x04",               // wait_one(h, U3, deadline, *obs=0x2010)
    "    mov  x1, x20",
    "    movz x2, #0x8, lsl #32",       // U3 = 1<<35
    "    movz x4, #0x2010",
    "    svc  #0",                      // x3 = deadline 值（派发保留）
    "    cmn  x0, #110",                // 须 -ETIMEDOUT（-110）
    "    b.ne 3f",
    "    ldr  x8, [x4]",                // observed（x4 = 0x2010 派发保留）
    "    cbnz x8, 3f",                  // 未命中不得回写（入口预写 0）
    "    movz x0, #0x61",               // t1 = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x9, [x2]",                // x9 = t1
    "    cmp  x9, x3",                  // t1 ≥ deadline（真睡到点，非空转早返）
    "    b.lo 3f",
    // ---- 路 2：poll 即刻命中 + 全满足集 ----
    "    movz x0, #0x06",               // signal(h, UMASK, U2)：清全部再置恰 bit1
    "    mov  x1, x20",
    "    movz x2, #0xFFFF, lsl #32",
    "    movk x2, #0xFFFF, lsl #48",
    "    movz x3, #0x20, lsl #32",      // U2 = 1<<37
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x04",               // wait_one(h, U2|U6, t1+δ, *obs)
    "    mov  x1, x20",
    "    movz x2, #0x220, lsl #32",     // 请求 U2|U6（超集——验回写非子集）
    "    add  x3, x9, x6",              // deadline（x9=t1、x6=δ 均未传保留）
    "    movz x4, #0x2010",
    "    svc  #0",                      // 信号已到：poll-first 即刻命中不睡
    "    cbnz x0, 3f",
    "    ldr  x8, [x4]",                // observed
    "    movz x9, #0x20, lsl #32",
    "    cmp  x8, x9",                  // 须恰 U2（全满足集 = current）
    "    b.ne 3f",
    // ---- 路 3：长睡被 waker2 signal 唤醒 ----
    "    movz x0, #0x06",               // signal(h, UMASK, 0) 再清零（waker2 最早
    "    mov  x1, x20",                 //   3 slice 后才置——此刻必无残留位）
    "    movz x2, #0xFFFF, lsl #32",
    "    movk x2, #0xFFFF, lsl #48",
    "    mov  x3, xzr",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x3, #0xE400",             // δ_far = 1e10 ns（10s，远超 waker2 时延）
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    movz x0, #0x61",               // t2 = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x5, [x2]",
    "    add  x3, x5, x3",              // deadline = t2 + 10s
    "    movz x0, #0x04",               // wait_one(h, U3, far, *obs)
    "    mov  x1, x20",
    "    movz x2, #0x8, lsl #32",       // U3
    "    movz x4, #0x2010",
    "    svc  #0",                      // 真睡 → waker2 signal(U3) 唤醒重验
    "    cbnz x0, 3f",                  // （waker2 先 signal → poll 命中同过；真睡由路 1 验）
    "    ldr  x8, [x4]",
    "    movz x9, #0x8, lsl #32",
    "    cmp  x8, x9",                  // observed == U3（全满足集）
    "    b.ne 3f",
    // ---- 稳态循环：节流 + canary + 'W' ----
    "    movz x9, #0x30, lsl #16",      // 节流阈值（与诸位用户进程同量级）
    "0:  mov  x10, xzr",
    "1:  add  x10, x10, #1",
    "    cmp  x10, x9",
    "    b.lo 1b",
    "    movz x3, #0x2000",             // 数据页 canary
    "    ldr  x4, [x3]",
    "    cbz  x4, 3f",
    "    movz x6, #0x57",              // 'W' → 数据页 0x2018（4-5a：真 IPC 打印）
    "    movz x5, #0x2018",
    "    str  x6, [x5]",
    "4:  movz x0, #0x52",              // send(console, 0x2018, 1, 0, 0)
    "    movz x1, #2",                 // console handle 0x0001_0002（boot 断言定值）
    "    movk x1, #1, lsl #16",
    "    movz x2, #0x2018",
    "    mov  x3, #1",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 5f",
    "    cmn  x0, #16",                // -EBUSY 重试（console 消费慢）
    "    b.ne 3f",
    "    b    4b",
    "5:  b    0b",
    "3:  movz x4, #0x4000, lsl #16",    // 自陷：三路断链 → 被杀 → watchdog 错杀断言 FAIL
    "    ldr  x5, [x4]",
    "    b    .",
    ".globl __user_waiter_end",
    "__user_waiter_end:",

    // 4-2 pinger：channel 阻塞链的 EL0 生产者验收（`IPC设计.md` Phase 4 拆步
    // 4-2）。入口 x0 = 本进程表里的 endA handle（boot 直插 RIGHT_SEND；对端
    // endB 在 ponger 表）。验收链（**EBUSY 驱动**——满即证明，不赌精确计数：
    // timer 抢占可能让 ponger 中途腾位）：send 灌到 -EBUSY → wait_one(WRITABLE,
    // now+10s) **必须命中**（ponger recv 腾位唤醒——超时 = 链断 FAIL）→ 补发
    // 一条须成功 → 首圈打 `CHAIN ok`（smoke 第 16 标记）、此后每圈节流打 'p'。
    // 稳态 = 生产者常态阻塞在 WRITABLE——「send 满 → 对端 recv 腾位唤醒」两
    // 进程阻塞链的活证。槽：0x2008 时钟、0x2010 observed、0x2020 PING 载荷、
    // 0x2030 首圈旗标（显式清零——新页不保证净）。
    ".balign 16",
    ".globl __user_pinger",
    "__user_pinger:",
    "    mov  x20, x0",                 // x20 常驻 endA handle
    "    movz x4, #0x0E10",             // PING 载荷 = 0x00C0FFEE_0000_0E10 @0x2020
    "    movk x4, #0xE000, lsl #16",
    "    movk x4, #0xFFEE, lsl #32",
    "    movk x4, #0x00C0, lsl #48",
    "    movz x5, #0x2020",
    "    str  x4, [x5]",
    "    movz x7, #0x2030",             // 首圈旗标槽
    "    str  xzr, [x7]",
    "    movz x9, #0x30, lsl #16",      // 节流阈值（稳态每圈 'p'）
    "0:  movz x0, #0x52",               // send(endA, PING@0x2020, 8, 0, 0)
    "    mov  x1, x20",
    "    movz x2, #0x2020",
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 0b",                  // 未满继续灌
    "    cmn  x0, #16",                 // -EBUSY = 满（阻塞链前置证明）
    "    b.ne 3f",
    "    movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",             // δ = 1e10 ns（10s，远超 ponger 节流周期）
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",              // deadline = now + 10s
    "    movz x0, #0x04",               // wait_one(endA, WRITABLE, deadline, *obs)
    "    mov  x1, x20",
    "    movz x2, #2",                  // SIGNAL_WRITABLE（内核派生位）
    "    movz x4, #0x2010",
    "    svc  #0",                      // 阻塞 → ponger recv 腾位唤醒重验命中
    "    cbnz x0, 3f",                  // 超时 = 链断（banner 缺失即 smoke FAIL）
    "    movz x0, #0x52",               // 补发一条（刚验过的空位）
    "    mov  x1, x20",
    "    movz x2, #0x2020",
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x8, [x7]",                // 首圈打 CHAIN ok，此后节流 'p'
    "    cbnz x8, 2f",
    "    movz x8, #1",
    "    str  x8, [x7]",
    "6:  adr  x5, 4f",                  // banner 整段一条消息（4-5a：真 IPC 打印）
    "    movz x0, #0x52",               // send(console, banner, 9, 0, 0)
    "    movz x1, #2",                  // console handle 0x0001_0002（boot 断言定值）
    "    movk x1, #1, lsl #16",
    "    mov  x2, x5",
    "    movz x3, #9",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 7f",
    "    cmn  x0, #16",                 // -EBUSY：console 消费慢，重试（可抢占）
    "    b.ne 3f",
    "    b    6b",
    "7:  b    0b",
    "2:  mov  x10, xzr",                // 节流
    "1:  add  x10, x10, #1",
    "    cmp  x10, x9",
    "    b.lo 1b",
    "    movz x6, #0x70",              // 'p' → 数据页 0x2040（4-5a：真 IPC 打印）
    "    movz x5, #0x2040",
    "    str  x6, [x5]",
    "8:  movz x0, #0x52",              // send(console, 0x2040, 1, 0, 0)
    "    movz x1, #2",                 // console handle 0x0001_0002（boot 断言定值）
    "    movk x1, #1, lsl #16",
    "    movz x2, #0x2040",
    "    mov  x3, #1",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 9f",
    "    cmn  x0, #16",                // -EBUSY 重试（console 消费慢）
    "    b.ne 3f",
    "    b    8b",
    "9:  b    0b",
    "3:  movz x4, #0x4000, lsl #16",    // 自陷：链断 → 被杀 → CHAIN 缺失 = smoke FAIL
    "    ldr  x5, [x4]",
    "    b    .",
    "4:  .ascii \"CHAIN ok\\n\"",
    "    .balign 4",                   // 尾部补齐（模板长度须 4 字节对齐）
    ".globl __user_pinger_end",
    "__user_pinger_end:",

    // 4-2 ponger：channel 阻塞链的 EL0 消费者验收；4-2b 稳态等待改
    // **wait_many**（0x05——多对象等待）。入口 x0 = 本进程表里的 endB handle
    //（boot 直插 RIGHT_RECV；表内另有一枚 boot 直插的哑 Event——只入数组、
    // 永不 signal，作为 wait_many「多对象里挑命中项」的对照项）。循环：recv →
    // 空 -EAGAIN → wait_many([endB:READABLE, evt:1<<32], now+10s) 阻塞（pinger
    // send 唤醒）→ 醒来核 *idx==0（命中 endB——哑 Event 位永不置、idx==1 即断链）
    // → 首次打 `MANY ok`（smoke 第 17 标记）→ 重 recv（重验——必命中）→ 验 PING
    // 载荷低 32 位 → 节流打 'P'。任一步非预期（超时/载荷错/idx 错）→ 自陷被杀
    //（pinger 侧 wait 随之超时 → CHAIN 缺失）。槽：0x2008 时钟、0x2010 observed、
    // 0x2020 收件 buf、0x2028/0x2030 容量 in-out（自复原型：载荷恒 8B/0 handle）、
    // 0x2038 随行数组、0x2040 handles 数组（endB + 哑 Event——boot 断言
    // 0x0001_0002 定值）、0x2050 signals 数组、0x2060 idx、0x2068 首圈旗标。
    // 4-5a 加固：MANY 标记改在**入口**先发一次 wait_many（队空 → 睡到 pinger
    // 首发唤醒；非空 → poll 即中——两路 ret==0/idx==0 必然）。稳态 -EAGAIN 何时
    // 来是排水/补灌速率的时序竞速（打印改走 console 往返后曾出现 120s 队列恒
    // 非空、一次 -EAGAIN 都不来的 run），不再作标记前提。
    ".balign 16",
    ".globl __user_ponger",
    "__user_ponger:",
    "    mov  x20, x0",                 // x20 常驻 endB handle
    "    movz x4, #8",                  // 容量槽：nbytes=8 / nhandles=0
    "    movz x5, #0x2028",
    "    str  x4, [x5]",
    "    str  xzr, [x5, #8]",
    "    movz x4, #0x2040",             // wait_many handles[2]：endB + 哑 Event
    "    str  w20, [x4]",
    "    movz x5, #2",                  // 哑 Event handle = 0x0001_0002（boot 断言定值）
    "    movk x5, #1, lsl #16",
    "    str  w5, [x4, #4]",
    "    movz x5, #0x2050",             // wait_many signals[2]：READABLE + 1<<32（永不置）
    "    movz x6, #1",
    "    str  x6, [x5]",
    "    movz x6, #1, lsl #32",
    "    str  x6, [x5, #8]",
    "    movz x7, #0x2068",             // 首圈旗标槽（显式清零——新页不保证净）
    "    str  xzr, [x7]",
    "    movz x8, #0x0E10",             // PING 低 32 位常驻（载荷比对）
    "    movk x8, #0xE000, lsl #16",
    "    movz x9, #0x30, lsl #16",      // 节流阈值
    "    movz x0, #0x61",               // 入口先等一条：now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",             // δ = 1e10 ns（10s，首发必早于此）
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x4, x5, x3",              // deadline = now + 10s
    "    movz x0, #0x05",               // wait_many(0x2040, 0x2050, 2, deadline,
    "    movz x1, #0x2040",             //          *idx=0x2060, *obs=0x2010)
    "    movz x2, #0x2050",
    "    movz x3, #2",
    "    movz x5, #0x2060",
    "    movz x6, #0x2010",
    "    svc  #0",                      // 队空睡到 pinger 首发 / 非空 poll 即中
    "    cbnz x0, 3f",                  // 10s 无首发 = pinger 断供 FAIL
    "    movz x4, #0x2060",
    "    ldr  x6, [x4]",
    "    cbnz x6, 3f",                  // idx≠0 = 命中哑 Event = 断链（永不 signal）
    "    b    5f",                      // 共用旗标/打印段（首圈旗标=0 → 打 MANY ok）
    "0:  movz x0, #0x53",               // recv(endB, 0x2020, *8, 0x2038, *0)
    "    mov  x1, x20",
    "    movz x2, #0x2020",
    "    movz x3, #0x2028",
    "    movz x4, #0x2038",
    "    movz x5, #0x2030",
    "    svc  #0",
    "    cbz  x0, 1f",                  // 收到 → 验载荷
    "    cmn  x0, #11",                 // -EAGAIN（pinger 未发/未灌满一圈）→ 阻塞等
    "    b.ne 3f",
    "    movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",             // δ = 1e10 ns（10s）
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x4, x5, x3",              // deadline = now + 10s
    "    movz x0, #0x05",               // wait_many(0x2040, 0x2050, 2, deadline,
    "    movz x1, #0x2040",             //          *idx=0x2060, *obs=0x2010)
    "    movz x2, #0x2050",
    "    movz x3, #2",
    "    movz x5, #0x2060",
    "    movz x6, #0x2010",
    "    svc  #0",                      // 阻塞 → pinger send 唤醒重验
    "    cbnz x0, 3f",                  // 超时 = pinger 断供 FAIL
    "    movz x4, #0x2060",
    "    ldr  x6, [x4]",
    "    cbnz x6, 3f",                  // idx≠0 = 命中哑 Event = 断链（永不 signal）
    "5:  ldr  x11, [x7]",               // 首圈打 MANY ok，此后直接重验（入口/稳态两路共用）
    "    cbnz x11, 6f",
    "    movz x11, #1",
    "    str  x11, [x7]",
    "7:  adr  x5, 4f",                  // banner 整段一条消息（4-5a：真 IPC 打印）
    "    movz x0, #0x52",               // send(console, banner, 8, 0, 0)
    "    movz x1, #3",                  // console handle 0x0001_0003（boot 断言定值）
    "    movk x1, #1, lsl #16",
    "    mov  x2, x5",
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 6f",
    "    cmn  x0, #16",                 // -EBUSY：console 消费慢，重试（可抢占）
    "    b.ne 3f",
    "    b    7b",
    "6:  b    0b",                      // 醒来重 recv（重验——必命中）
    "1:  ldr  w6, [x2]",                // 载荷低 32 位（x2 = buf 地址，派发保留）
    "    cmp  w6, w8",
    "    b.ne 3f",
    "    mov  x10, xzr",                // 节流
    "2:  add  x10, x10, #1",
    "    cmp  x10, x9",
    "    b.lo 2b",
    "    movz x6, #0x50",              // 'P' → 数据页 0x2070（4-5a：真 IPC 打印）
    "    movz x5, #0x2070",
    "    str  x6, [x5]",
    "8:  movz x0, #0x52",              // send(console, 0x2070, 1, 0, 0)
    "    movz x1, #3",                 // console handle 0x0001_0003（boot 断言定值）
    "    movk x1, #1, lsl #16",
    "    movz x2, #0x2070",
    "    mov  x3, #1",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 9f",
    "    cmn  x0, #16",                // -EBUSY 重试（console 消费慢）
    "    b.ne 3f",
    "    b    8b",
    "9:  b    0b",
    "3:  movz x4, #0x4000, lsl #16",    // 自陷：链断 → 被杀 → CHAIN 缺失 = smoke FAIL
    "    ldr  x5, [x4]",
    "    b    .",
    "4:  .ascii \"MANY ok\\n\"",
    "    .balign 4",                   // 尾部补齐（模板长度须 4 字节对齐）
    ".globl __user_ponger_end",
    "__user_ponger_end:",

    // 4-5b svcmgr：**服务管理器拉起一个服务**（Phase 4 拆步表 4-5 后半——
    // 里程碑：首个由用户态进程经真 syscall 全链创建的用户态服务）。入口
    // x0 = console 发端（boot 首插 0x0001_0001）。拉起链（spawner 同形减
    // 礼物段）：process_create(双出参) → vmo_create(0x2000：echo 代码 @off0
    // + 栈 @off1)/commit/write(内嵌 echo 模板 268B，字节数精确) →
    // vmar_map@0x1000 → channel_create 建对（end0 自留收发、end1 作
    // bootstrap 迁 echo 表——start 后 close 原值必 -EBADF 迁移自证）→
    // thread_create → process_start(entry 0x1000, sp 0x3000)。
    // 验收往返：send ECHO_REQ → wait_one(end0, READABLE, now+10s) → recv 验
    // 回显（低 32 位比对）→ 打 `SVC ok`（smoke 第 21 标记）→ 稳态每圈节流
    // 往返打 'v'（每个 'v' = 一次实证 echo 往返）。echo 模板静默（无 console
    // handle——boot 无法预知运行期建表的 handle 值；echo 断链由 svcmgr 侧
    // 标记缺失/watchdog 错杀断言兜底）。PI：svcmgr/echo 各持私有表、全
    // prio 0——互为 donor/受捐的捐赠额恒 0，无级联面（「常驻内核服务私有
    // 表」纪律只约束睡共享 BOOTSTRAP_HT 的内核任务，EL0 天然各持各表）。
    // 暂存：x16=hp、x17=hv(echo Vmar)、x18=hvmo、x19=ht、x21=end0(自留)、
    // x22=end1(bootstrap)、x23=console；槽：0x2008 时钟、0x2010/0x2018
    // handle 双出参、0x2020 vmo/线程 out、0x2028 vaddr in-out、0x2040/0x2048
    // channel 双出参、0x2050 ECHO_REQ 载荷、0x2058 回显 buf、0x2060/0x2068
    // recv 容量 in-out、0x2070 observed、0x2078 'v' 打印槽。
    // 4-6 A 半追加段（`SVC ok` 后、稳态 `7:` 前，直线无环）：同形拉起
    // talker（内嵌模板 936B = 232 指令 + 8B banner）与 echo2（复用同一份
    // echo 模板 268B——位置无关，一份字节两实例）各一对 channel；末段
    // duplicate(console, RIGHT_SEND=0x100) 出别名（**必须先复制**：send 会
    // take 走句柄，原值 x23 须留给稳态 'v'），send(s1, [别名, t2], 2) 把
    // console 能力 + echo2 对端**随消息**迁入 talker 表——服务间授权不经
    // 全局名，能力即权限。talker 凭收到的别名打 `DEMO ok`（smoke 第 22
    // 标记）。追加段槽：0x2100/0x2108 p2/v2、0x2110 hv2、0x2118 vaddr、
    // 0x2120/0x2128 s1/t1、0x2130 ht2、0x2138/0x2140 p3/v3、0x2148 hv3、
    // 0x2150 vaddr、0x2158/0x2160 t2/e2、0x2168 ht3、0x2178 随行数组
    //（u32 LE ×2：[0]=别名 dup out 直落、[1]=t2）、0x2188 payload。
    ".balign 16",
    ".globl __user_svcmgr",
    "__user_svcmgr:",
    "    mov  x23, x0",                 // x23 常驻：console 发端（boot 首插 0x0001_0001）
    "    movz x0, #0x31",               // process_create(job=0, options=0, *p, *v)
    "    mov  x1, xzr",
    "    mov  x2, xzr",
    "    movz x3, #0x2010",
    "    movz x4, #0x2018",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x16, [x3]",               // hp
    "    cbz  x16, 3f",
    "    ldr  x17, [x4]",               // hv = echo root Vmar
    "    cbz  x17, 3f",
    "    movz x0, #0x11",               // vmo_create(size=0x2000, 0, *out=0x2020)
    "    movz x1, #0x2000",             // 两页：echo 代码 @off0 + echo 栈 @off1
    "    mov  x2, xzr",
    "    movz x3, #0x2020",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x18, [x3]",               // hvmo
    "    cbz  x18, 3f",
    "    movz x0, #0x12",               // vmo_commit(hvmo, 0, 0x2000)
    "    mov  x1, x18",
    "    mov  x2, xzr",
    "    movz x3, #0x2000",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x15",               // vmo_write(hvmo, buf=内嵌 echo 模板, 0, 268)
    "    mov  x1, x18",
    "    adr  x2, 4f",                  // 本页内自相对取址（搬迁安全，见模块注）
    "    mov  x3, xzr",
    "    movz x4, #268",                // echo 模板 67 指令（字节数精确——改模板重数）
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x5, #0x2028",             // vaddr 槽 ← 请求 echo 代码 VA 0x1000
    "    movz x6, #0x1000",
    "    str  x6, [x5]",
    "    movz x0, #0x16",               // vmar_map(hv, hvmo, 0, *vaddr, 0x2000, RW)
    "    mov  x1, x17",
    "    mov  x2, x18",
    "    mov  x3, xzr",
    "    mov  x4, x5",
    "    movz x5, #0x2000",
    "    movz x6, #3",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x51",               // channel_create(0, *out0=0x2040, *out1=0x2048)
    "    mov  x1, xzr",                 // options=0 常规通道（intent 传播默认开）
    "    movz x2, #0x2040",
    "    movz x3, #0x2048",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x5, #0x2040",
    "    ldr  x21, [x5]",               // end0（自留：发请求 + 收回显）
    "    cbz  x21, 3f",
    "    ldr  x22, [x5, #8]",           // end1（作 bootstrap 迁 echo）
    "    cbz  x22, 3f",
    "    movz x0, #0x34",               // thread_create(hp, 0, *out=0x2020)
    "    mov  x1, x16",
    "    mov  x2, xzr",
    "    movz x3, #0x2020",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x19, [x3]",               // ht
    "    cbz  x19, 3f",
    "    movz x0, #0x32",               // process_start(hp, ht, entry=0x1000, sp=0x3000, end1)
    "    mov  x1, x16",
    "    mov  x2, x19",
    "    movz x3, #0x1000",             // echo 入口 = 映进的代码页
    "    movz x4, #0x3000",             // echo sp0 = 映射顶（栈向下长）
    "    mov  x5, x22",                 // bootstrap = end1（端点迁 echo 表，echo x0 收到）
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x01",               // 迁移自证：close(end1) 必 -EBADF
    "    mov  x1, x22",
    "    svc  #0",
    "    cmn  x0, #9",
    "    b.ne 3f",
    // ---- 验收往返：send ECHO_REQ → wait READABLE → recv 验回显 → SVC ok ----
    "    movz x4, #0x0E30",             // ECHO_REQ = 0x00C0FFEE_E000_0E30 @0x2050
    "    movk x4, #0xE000, lsl #16",
    "    movk x4, #0xFFEE, lsl #32",
    "    movk x4, #0x00C0, lsl #48",
    "    movz x6, #0x2050",
    "    str  x4, [x6]",
    "    movz x4, #8",                  // recv caps 预装：nbytes=8 @0x2060 / nhandles=0 @0x2068
    "    movz x5, #0x2060",
    "    str  x4, [x5]",
    "    str  xzr, [x5, #8]",
    "    movz x0, #0x52",               // send(end0, ECHO_REQ, 8, 0, 0)——echo 队出生必空必成
    "    mov  x1, x21",
    "    movz x2, #0x2050",
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",             // δ = 1e10 ns（10s 帽，echo 必早于此回件）
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",              // deadline = now + 10s
    "    movz x0, #0x04",               // wait_one(end0, READABLE, deadline, *obs=0x2070)
    "    mov  x1, x21",
    "    movz x2, #1",                  // SIGNAL_READABLE = 1<<0
    "    movz x4, #0x2070",
    "    svc  #0",
    "    cbnz x0, 3f",                  // 超时 = echo 未回 = 拉起/服务循环断 FAIL
    "    movz x0, #0x53",               // recv(end0, 0x2058, *8, 0, *0)——回显在队必命中
    "    mov  x1, x21",
    "    movz x2, #0x2058",
    "    movz x3, #0x2060",
    "    mov  x4, xzr",
    "    movz x5, #0x2068",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  w6, [x2]",                // 回显低 32 位 == ECHO_REQ 低 32 位
    "    movz w7, #0x0E30",
    "    movk w7, #0xE000, lsl #16",
    "    cmp  w6, w7",
    "    b.ne 3f",
    "    movz x4, #8",                  // caps 恢复（recv 扣减 *nbytes → 稳态前重装）
    "    movz x5, #0x2060",
    "    str  x4, [x5]",
    "    str  xzr, [x5, #8]",
    "6:  movz x0, #0x52",               // send(console, banner, 7, 0, 0)
    "    mov  x1, x23",
    "    adr  x2, 5f",                  // `SVC ok\n` 7 字节 + .balign 4（模板对齐纪律）
    "    movz x3, #7",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 16f",                // 成功 → 16：A 半链（EBUSY 重试环的正常出口）
    "    cmn  x0, #16",                 // -EBUSY 重试（console 消费慢）
    "    b.ne 3f",
    "    b    6b",
    // ---- A 半（4-6）：`SVC ok` 后再拉 talker + echo2——两个 EL0 服务互联， ----
    // ---- console 能力经消息迁移交给 talker（能力即权限的跨服务授权活证）。 ----
    // ---- 入口 = banner send 成功（16f）；链尾顺落 7: 稳态。 --------------------
    "16: movz x0, #0x31",               // process_create(0, 0, *p2, *v2) —— talker
    "    mov  x1, xzr",
    "    mov  x2, xzr",
    "    movz x3, #0x2100",
    "    movz x4, #0x2108",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x16, [x3]",
    "    cbz  x16, 3f",
    "    ldr  x17, [x4]",
    "    cbz  x17, 3f",
    "    movz x0, #0x11",               // vmo_create(0x2000, 0, *hv2)：代码 @off0 + 栈 @off1
    "    movz x1, #0x2000",
    "    mov  x2, xzr",
    "    movz x3, #0x2110",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x18, [x3]",
    "    cbz  x18, 3f",
    "    movz x0, #0x12",               // vmo_commit(hv2, 0, 0x2000)
    "    mov  x1, x18",
    "    mov  x2, xzr",
    "    movz x3, #0x2000",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x15",               // vmo_write(hv2, 内嵌 talker 模板, 0, LEN)
    "    mov  x1, x18",
    "    adr  x2, 8f",                  // 同页 adr 取址（整块搬迁偏移不变）
    "    mov  x3, xzr",
    "    movz x4, #936",                // talker 模板字节数（改模板重数指令再改此值）
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x5, #0x2118",             // vaddr 槽预写 0x1000
    "    movz x6, #0x1000",
    "    str  x6, [x5]",
    "    movz x0, #0x16",               // vmar_map(v2, hv2, 0, *vaddr, 0x2000, RW)
    "    mov  x1, x17",
    "    mov  x2, x18",
    "    mov  x3, xzr",
    "    mov  x4, x5",
    "    movz x5, #0x2000",
    "    movz x6, #3",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x51",               // channel_create(0, *s1, *t1) —— svcmgr↔talker
    "    mov  x1, xzr",
    "    movz x2, #0x2120",
    "    movz x3, #0x2128",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x34",               // thread_create(p2, 0, *ht2)
    "    mov  x1, x16",
    "    mov  x2, xzr",
    "    movz x3, #0x2130",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x11, [x3]",
    "    cbz  x11, 3f",
    "    movz x5, #0x2120",
    "    ldr  x12, [x5, #8]",           // t1 = bootstrap（随 start 迁入 talker 表）
    "    cbz  x12, 3f",
    "    movz x0, #0x32",               // process_start(p2, ht2, 0x1000, 0x3000, t1)
    "    mov  x1, x16",
    "    mov  x2, x11",
    "    movz x3, #0x1000",
    "    movz x4, #0x3000",
    "    mov  x5, x12",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x01",               // 迁移自证：close(t1) 必 -EBADF
    "    mov  x1, x12",
    "    svc  #0",
    "    cmn  x0, #9",
    "    b.ne 3f",
    "    movz x0, #0x31",               // process_create(0, 0, *p3, *v3) —— echo2
    "    mov  x1, xzr",
    "    mov  x2, xzr",
    "    movz x3, #0x2138",
    "    movz x4, #0x2140",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x16, [x3]",
    "    cbz  x16, 3f",
    "    ldr  x17, [x4]",
    "    cbz  x17, 3f",
    "    movz x0, #0x11",               // vmo_create(0x2000, 0, *hv3)
    "    movz x1, #0x2000",
    "    mov  x2, xzr",
    "    movz x3, #0x2148",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x18, [x3]",
    "    cbz  x18, 3f",
    "    movz x0, #0x12",               // vmo_commit(hv3, 0, 0x2000)
    "    mov  x1, x18",
    "    mov  x2, xzr",
    "    movz x3, #0x2000",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x15",               // vmo_write(hv3, 同一份 echo 模板字节, 0, 268)
    "    mov  x1, x18",
    "    adr  x2, 4f",                  // echo 模板位置无关——echo1/echo2 共用一份字节
    "    mov  x3, xzr",
    "    movz x4, #268",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x5, #0x2150",             // vaddr 槽预写 0x1000
    "    movz x6, #0x1000",
    "    str  x6, [x5]",
    "    movz x0, #0x16",               // vmar_map(v3, hv3, 0, *vaddr, 0x2000, RW)
    "    mov  x1, x17",
    "    mov  x2, x18",
    "    mov  x3, xzr",
    "    mov  x4, x5",
    "    movz x5, #0x2000",
    "    movz x6, #3",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x51",               // channel_create(0, *t2, *e2) —— talker↔echo2
    "    mov  x1, xzr",
    "    movz x2, #0x2158",
    "    movz x3, #0x2160",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x34",               // thread_create(p3, 0, *ht3)
    "    mov  x1, x16",
    "    mov  x2, xzr",
    "    movz x3, #0x2168",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x11, [x3]",
    "    cbz  x11, 3f",
    "    movz x5, #0x2160",
    "    ldr  x12, [x5]",               // e2 = bootstrap（随 start 迁入 echo2 表）
    "    cbz  x12, 3f",
    "    movz x0, #0x32",               // process_start(p3, ht3, 0x1000, 0x3000, e2)
    "    mov  x1, x16",
    "    mov  x2, x11",
    "    movz x3, #0x1000",
    "    movz x4, #0x3000",
    "    mov  x5, x12",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    movz x0, #0x01",               // 迁移自证：close(e2) 必 -EBADF
    "    mov  x1, x12",
    "    svc  #0",
    "    cmn  x0, #9",
    "    b.ne 3f",
    "    movz x6, #0x2178",             // 随行数组 = [console 别名, t2]（u32 LE 紧排）
    "    movz x0, #0x02",               // duplicate(console, RIGHT_SEND) → *out = 数组[0]
    "    mov  x1, x23",                 // 先复制再送：send 会 take 走句柄，原值须留给稳态
    "    movz x2, #0x100",              // RIGHT_SEND = 1<<8（权利只紧不松）
    "    mov  x3, x6",                  // *out = 0x2178：新别名直落数组 [0] 槽（spawner 同形）
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  w11, [x6, #-0x20]",       // t2 @0x2158（echo2 对端，随消息交 talker）
    "    str  w11, [x6, #4]",
    "    str  xzr, [x6, #16]",          // payload 8B 清零 @0x2188（纯载体，talker 不看内容）
    "    movz x0, #0x52",               // send(s1, 0x2188, 8, 0x2178, 2)——能力经消息迁移
    "    ldr  x1, [x6, #-0x58]",        // s1 @0x2120（自留端）
    "    add  x2, x6, #16",
    "    movz x3, #8",
    "    mov  x4, x6",
    "    movz x5, #2",
    "    svc  #0",
    "    cbnz x0, 3f",                  // 交接件未入队 = A 半链断
    // ---- 稳态：节流 + 实证往返 + 'v' ----
    "7:  movz x8, #0x0E30",             // ECHO_REQ 低 32 位常驻（稳态比对）
    "    movk x8, #0xE000, lsl #16",
    "    movz x9, #0x30, lsl #16",      // 节流阈值（与诸位用户进程同量级）
    "0:  mov  x10, xzr",
    "1:  add  x10, x10, #1",
    "    cmp  x10, x9",
    "    b.lo 1b",
    "10: movz x0, #0x52",               // send(end0, ECHO_REQ, 8, 0, 0)
    "    mov  x1, x21",
    "    movz x2, #0x2050",
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 11f",
    "    cmn  x0, #16",                 // -EBUSY（echo 消费慢）→ wait WRITABLE 再试
    "    b.ne 3f",
    "    movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",
    "    movz x0, #0x04",               // wait_one(end0, WRITABLE, deadline, *obs)
    "    mov  x1, x21",
    "    movz x2, #2",                  // SIGNAL_WRITABLE = 1<<1
    "    movz x4, #0x2070",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    b    10b",                     // 腾位后补发
    "11: movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",
    "    movz x0, #0x04",               // wait_one(end0, READABLE, deadline, *obs)
    "    mov  x1, x21",
    "    movz x2, #1",
    "    movz x4, #0x2070",
    "    svc  #0",
    "    cbnz x0, 3f",                  // 超时 = echo 断供 FAIL
    "12: movz x0, #0x53",               // recv(end0, 0x2058, *8, 0, *0)
    "    mov  x1, x21",
    "    movz x2, #0x2058",
    "    movz x3, #0x2060",
    "    mov  x4, xzr",
    "    movz x5, #0x2068",
    "    svc  #0",
    "    cbz  x0, 13f",
    "    cmn  x0, #11",                 // -EAGAIN（虚假唤醒竞速）→ 重 wait
    "    b.ne 3f",
    "    b    11b",
    "13: ldr  w6, [x2]",
    "    cmp  w6, w8",                  // 回显 == ECHO_REQ
    "    b.ne 3f",
    "    movz x4, #8",                  // caps 恢复
    "    movz x5, #0x2060",
    "    str  x4, [x5]",
    "    str  xzr, [x5, #8]",
    "    movz x6, #0x76",               // 'v' → 数据页 0x2078（每个 'v' = 一次实证往返）
    "    movz x5, #0x2078",
    "    str  x6, [x5]",
    "14: movz x0, #0x52",               // send(console, 0x2078, 1, 0, 0)
    "    mov  x1, x23",
    "    movz x2, #0x2078",
    "    movz x3, #1",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 15f",
    "    cmn  x0, #16",                 // -EBUSY 重试（console 消费慢）
    "    b.ne 3f",
    "    b    14b",
    "15: b    0b",
    "3:  movz x4, #0x4000, lsl #16",    // 自陷：拉起/往返链断 → 被杀 → SVC 缺失 = smoke FAIL
    "    ldr  x5, [x4]",
    "    b    .",
    "5:  .ascii \"SVC ok\\n\"",
    "    .balign 4",                   // 尾部补齐（模板长度须 4 字节对齐）
    // ---- echo 服务代码（数据形态嵌在 svcmgr 模板内；vmo_write 拷进 echo
    // 0x1000。入口 x0 = end1（echo 表内 handle，bootstrap 迁入）。服务循环：
    // recv → 原样回显 8 字节；空 -EAGAIN → wait_one(end1, READABLE, 10s)；
    // 回显 send -EBUSY（理论不至——svcmgr 每圈收一件）→ wait WRITABLE 重试；
    // 超时/错码自陷（静默进程，死 = svcmgr 侧标记缺失/watchdog 错杀断言暴露）。
    // x19 = end1 常驻；栈页槽：0x2F90 收件 buf、0x2F98/0x2FA0 容量 in-out、
    // 0x2FA8 时钟、0x2FB0 observed。67 指令 = 268B（字节数精确——改模板重数）。----
    "4:  mov  x19, x0",                 // 保存 end1（echo 表 handle）
    "    movz x4, #8",                  // caps 预装：nbytes=8 @0x2F98 / nhandles=0 @0x2FA0
    "    movz x5, #0x2F98",
    "    str  x4, [x5]",
    "    str  xzr, [x5, #8]",
    "20: movz x0, #0x53",               // recv(end1, 0x2F90, *8, 0, *0)
    "    mov  x1, x19",
    "    movz x2, #0x2F90",
    "    movz x3, #0x2F98",
    "    mov  x4, xzr",
    "    movz x5, #0x2FA0",
    "    svc  #0",
    "    cbz  x0, 21f",
    "    cmn  x0, #11",                 // 空（svcmgr 未发）→ wait READABLE
    "    b.ne 23f",
    "    movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2FA8",
    "    svc  #0",
    "    cbnz x0, 23f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",             // δ = 1e10 ns（10s 帽）
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",
    "    movz x0, #0x04",               // wait_one(end1, READABLE, deadline, *obs=0x2FB0)
    "    mov  x1, x19",
    "    movz x2, #1",                  // SIGNAL_READABLE
    "    movz x4, #0x2FB0",
    "    svc  #0",
    "    cbnz x0, 23f",                 // 超时 = svcmgr 断供 = 链断
    "    b    20b",                     // 醒来重 recv（重验——必命中）
    "21: movz x0, #0x52",               // send(end1, 0x2F90, 8, 0, 0)——原样回显
    "    mov  x1, x19",
    "    movz x2, #0x2F90",
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 22f",
    "    cmn  x0, #16",                 // 回显队列满（理论不至）→ wait WRITABLE 重试
    "    b.ne 23f",
    "    movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2FA8",
    "    svc  #0",
    "    cbnz x0, 23f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",
    "    movz x0, #0x04",               // wait_one(end1, WRITABLE, deadline, *obs)
    "    mov  x1, x19",
    "    movz x2, #2",                  // SIGNAL_WRITABLE
    "    movz x4, #0x2FB0",
    "    svc  #0",
    "    cbnz x0, 23f",
    "    b    21b",
    "22: movz x4, #8",                  // caps 恢复（recv 扣减 *nbytes → 重装 8）
    "    movz x5, #0x2F98",
    "    str  x4, [x5]",
    "    str  xzr, [x5, #8]",
    "    b    20b",
    "23: movz x4, #0x4000, lsl #16",    // 自陷：链断 → 被杀（echo 无标记，svcmgr 侧兜底）
    "    ldr  x5, [x4]",
    "    b    .",

    // 4-6 A 半 talker：svcmgr 拉起的第二个用户态服务——凭 bootstrap t1 收
    // 交接件（[console 别名 RIGHT_SEND, t2 echo2 对端] 随消息迁入自己表），
    // 与 echo2（同一份 echo 模板字节的第二实例）直接往返，不经 svcmgr 中
    // 转；首圈验回显后凭**收到的** console handle 打 `DEMO ok`（smoke 第 22
    // 标记）——「服务经 channel 全链互联 + 能力经消息授权」的出口演示。
    // 寄存器：x20=t1、x21=console 别名（消息送来）、x22=t2、x8=DEMO_REQ 低
    // 32 位、x9=节流阈值；槽：0x2F90 载荷 buf、0x2F98/0x2FA0 recv 容量
    // in-out、0x2FA8 时钟、0x2FB0 observed、0x2FC0/0x2FC4 随行 handle 数
    // 组（u32 LE 紧排——recv 新值回写同宽）、0x2FD0 'd' 打印槽。断链自陷
    // 被杀（标记缺失 = smoke FAIL 兜底）。
    "8:  mov  x20, x0",                 // t1：bootstrap 迁入 talker 表
    "    movz x4, #8",
    "    movz x5, #0x2F98",
    "    str  x4, [x5]",
    "    movz x4, #2",
    "    str  x4, [x5, #8]",            // caps 预装：nbytes=8 / nhandles=2（交接件）
    "50: movz x0, #0x53",               // recv(t1, buf, *8, hbuf, *2)
    "    mov  x1, x20",
    "    movz x2, #0x2F90",
    "    movz x3, #0x2F98",
    "    movz x4, #0x2FC0",
    "    movz x5, #0x2FA0",
    "    svc  #0",
    "    cbz  x0, 51f",
    "    cmn  x0, #11",                 // 空（交接件未到）→ wait READABLE
    "    b.ne 69f",
    "    movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2FA8",
    "    svc  #0",
    "    cbnz x0, 69f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",
    "    movz x0, #0x04",               // wait_one(t1, READABLE, now+10s, *obs)
    "    mov  x1, x20",
    "    movz x2, #1",                  // SIGNAL_READABLE = 1<<0
    "    movz x4, #0x2FB0",
    "    svc  #0",
    "    cbnz x0, 69f",
    "    b    50b",
    "51: movz x5, #0x2FA0",             // 随行 handle 数必须恰 2（console 别名 + t2）
    "    ldr  x6, [x5]",
    "    cmp  x6, #2",
    "    b.ne 69f",
    "    ldr  w21, [x5, #0x20]",        // console 别名 @0x2FC0（u32 LE 紧排）——进了自己表
    "    cbz  x21, 69f",
    "    ldr  w22, [x5, #0x24]",        // t2 @0x2FC4（echo2 对端）
    "    cbz  x22, 69f",
    "    movz x8, #0x0D30",             // DEMO_REQ 低 32 位（区别 echo1 的 0x0E30）
    "    movk x8, #0xE000, lsl #16",
    "    movz x9, #0x30, lsl #16",      // 节流阈值
    "    movz x5, #0x2F90",
    "    str  x8, [x5]",
    "52: movz x0, #0x52",               // send(t2, buf, 8, 0, 0) —— 首圈验收
    "    mov  x1, x22",
    "    movz x2, #0x2F90",
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 53f",
    "    cmn  x0, #16",                 // -EBUSY（echo2 消费慢）→ wait WRITABLE
    "    b.ne 69f",
    "    movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2FA8",
    "    svc  #0",
    "    cbnz x0, 69f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",
    "    movz x0, #0x04",               // wait_one(t2, WRITABLE, now+10s, *obs)
    "    mov  x1, x22",
    "    movz x2, #2",
    "    movz x4, #0x2FB0",
    "    svc  #0",
    "    cbnz x0, 69f",
    "    b    52b",
    "53: movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2FA8",
    "    svc  #0",
    "    cbnz x0, 69f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",
    "    movz x0, #0x04",               // wait_one(t2, READABLE, now+10s, *obs)
    "    mov  x1, x22",
    "    movz x2, #1",
    "    movz x4, #0x2FB0",
    "    svc  #0",
    "    cbnz x0, 69f",
    "54: movz x0, #0x53",               // recv(t2, buf, *8, 0, *0) —— 收回显
    "    mov  x1, x22",
    "    movz x2, #0x2F90",
    "    movz x3, #0x2F98",
    "    mov  x4, xzr",
    "    movz x5, #0x2FA0",
    "    svc  #0",
    "    cbz  x0, 55f",
    "    cmn  x0, #11",                 // -EAGAIN → 回 53b 重等（醒来重验）
    "    b.ne 69f",
    "    b    53b",
    "55: ldr  w6, [x2]",                // 回显低 32 位 == DEMO_REQ 低 32 位
    "    cmp  w6, w8",
    "    b.ne 69f",
    "    movz x4, #8",                  // caps 恢复（稳态 recv 复用）
    "    movz x5, #0x2F98",
    "    str  x4, [x5]",
    "    movz x4, #0x64",               // 'd' @0x2FD0（稳态字）
    "    movz x5, #0x2FD0",
    "    str  x4, [x5]",
    "56: movz x0, #0x52",               // send(console 别名, `DEMO ok\n`, 8, 0, 0)
    "    mov  x1, x21",
    "    adr  x2, 9f",                  // banner 在模板内（随整块搬迁，adr 偏移不变）
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 57f",
    "    cmn  x0, #16",                 // -EBUSY（console 消费慢）→ wait WRITABLE
    "    b.ne 69f",
    "    movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2FA8",
    "    svc  #0",
    "    cbnz x0, 69f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",
    "    movz x0, #0x04",               // wait_one(console, WRITABLE, now+10s, *obs)
    "    mov  x1, x21",
    "    movz x2, #2",
    "    movz x4, #0x2FB0",
    "    svc  #0",
    "    cbnz x0, 69f",
    "    b    56b",
    "57: mov  x10, xzr",                // 稳态：节流 + 往返验回显 + 'd'
    "58: add  x10, x10, #1",
    "    cmp  x10, x9",
    "    b.lo 58b",
    "    movz x5, #0x2F90",
    "    str  x8, [x5]",                // 重写 req（recv 覆写后保险）
    "60: movz x0, #0x52",               // send(t2, buf, 8, 0, 0)
    "    mov  x1, x22",
    "    movz x2, #0x2F90",
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 61f",
    "    cmn  x0, #16",                 // -EBUSY → wait WRITABLE 再试
    "    b.ne 69f",
    "    movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2FA8",
    "    svc  #0",
    "    cbnz x0, 69f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",
    "    movz x0, #0x04",               // wait_one(t2, WRITABLE, now+10s, *obs)
    "    mov  x1, x22",
    "    movz x2, #2",
    "    movz x4, #0x2FB0",
    "    svc  #0",
    "    cbnz x0, 69f",
    "    b    60b",
    "61: movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2FA8",
    "    svc  #0",
    "    cbnz x0, 69f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",
    "    movz x0, #0x04",               // wait_one(t2, READABLE, now+10s, *obs)
    "    mov  x1, x22",
    "    movz x2, #1",
    "    movz x4, #0x2FB0",
    "    svc  #0",
    "    cbnz x0, 69f",
    "62: movz x0, #0x53",               // recv(t2, buf, *8, 0, *0)
    "    mov  x1, x22",
    "    movz x2, #0x2F90",
    "    movz x3, #0x2F98",
    "    mov  x4, xzr",
    "    movz x5, #0x2FA0",
    "    svc  #0",
    "    cbz  x0, 63f",
    "    cmn  x0, #11",                 // -EAGAIN → 回 61b 重等
    "    b.ne 69f",
    "    b    61b",
    "63: ldr  w6, [x2]",                // 回显低 32 位 == DEMO_REQ 低 32 位
    "    cmp  w6, w8",
    "    b.ne 69f",
    "    movz x4, #8",                  // caps 恢复
    "    movz x5, #0x2F98",
    "    str  x4, [x5]",
    "64: movz x0, #0x52",               // send(console 别名, 'd', 1, 0, 0)
    "    mov  x1, x21",
    "    movz x2, #0x2FD0",
    "    movz x3, #1",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 57b",                 // 成功 → 下一圈
    "    cmn  x0, #16",                 // -EBUSY → wait WRITABLE 重试
    "    b.ne 69f",
    "    movz x0, #0x61",               // now = clock_read()
    "    mov  x1, xzr",
    "    movz x2, #0x2FA8",
    "    svc  #0",
    "    cbnz x0, 69f",
    "    ldr  x5, [x2]",
    "    movz x3, #0xE400",
    "    movk x3, #0x540B, lsl #16",
    "    movk x3, #0x2, lsl #32",
    "    add  x3, x5, x3",
    "    movz x0, #0x04",               // wait_one(console, WRITABLE, now+10s, *obs)
    "    mov  x1, x21",
    "    movz x2, #2",
    "    movz x4, #0x2FB0",
    "    svc  #0",
    "    cbnz x0, 69f",
    "    b    64b",
    "69: movz x4, #0x4000, lsl #16",    // 自陷：链断 → 被杀（DEMO ok 缺失兜底）
    "    ldr  x5, [x4]",
    "    b    .",
    "9:  .ascii \"DEMO ok\\n\"",
    "    .balign 4",
    ".globl __user_svcmgr_end",
    "__user_svcmgr_end:",
);

extern "C" {
    static __user_good: u8;
    static __user_good_end: u8;
    static __user_rogue_ptr: u8;
    static __user_rogue_ptr_end: u8;
    static __user_abuser: u8;
    static __user_abuser_end: u8;
    static __user_spawner: u8;
    static __user_spawner_end: u8;
    static __user_waiter: u8;
    static __user_waiter_end: u8;
    static __user_pinger: u8;
    static __user_pinger_end: u8;
    static __user_ponger: u8;
    static __user_ponger_end: u8;
    static __user_svcmgr: u8;
    static __user_svcmgr_end: u8;
}

/// 取 `[start,end)` 机器码模板。
///
/// # Safety
/// start/end 须来自同一 `global_asm!` 块的连续指令序列（模板整块拷贝语义成立）。
unsafe fn blob(start: *const u8, end: *const u8) -> &'static [u8] {
    core::slice::from_raw_parts(start, end as usize - start as usize)
}

/// good 用户程序模板。
pub fn user_good_code() -> &'static [u8] {
    // SAFETY: 同一 global_asm 块内连续。
    unsafe { blob(&__user_good, &__user_good_end) }
}

/// rogue_ptr 用户程序模板。
pub fn user_rogue_ptr_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_rogue_ptr, &__user_rogue_ptr_end) }
}

/// abuser 用户程序模板（真号 + 垃圾 handle 滥用 → -EBADF 存活）。
pub fn user_abuser_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_abuser, &__user_abuser_end) }
}

/// spawner 用户程序模板（3-3 验收：经真 syscall 创建第二个进程）。
///
/// 模板内含 `adr`（取同页内嵌的子代码字节）——自相对取址在整块搬迁后偏移
/// 不变，拷贝安全性与「仅局部数字标号 + movz」约定同源（无跨块/绝对引用）。
pub fn user_spawner_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_spawner, &__user_spawner_end) }
}

/// waiter 用户程序模板（4-1 验收：阻塞 object_wait_one 三路——超时/poll 命中/
/// 真睡被 signal 唤醒；时序对手方 = main.rs 的 waker2 内核任务）。
pub fn user_waiter_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_waiter, &__user_waiter_end) }
}

/// pinger 用户程序模板（4-2 验收：channel 阻塞链生产者——send 满 → wait
/// WRITABLE 阻塞 → ponger recv 腾位唤醒 → 补发成功打 `CHAIN ok`）。
pub fn user_pinger_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_pinger, &__user_pinger_end) }
}

/// ponger 用户程序模板（4-2 验收：channel 阻塞链消费者——recv 空 → wait
/// READABLE 阻塞 → pinger send 唤醒 → 重验收得 PING）。
pub fn user_ponger_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_ponger, &__user_ponger_end) }
}

/// svcmgr 用户程序模板（4-5b 验收：服务管理器拉起 echo 服务——真 syscall
/// 全链创建用户态进程 + 通道，ECHO_REQ 往返验证打 `SVC ok`）。
///
/// 模板内含 `adr`（取同页内嵌的 echo 服务代码字节）——自相对取址在整块
/// 搬迁后偏移不变（spawner 子代码同款纪律）；echo 段字节数须与模板内
/// `movz x4, #268` 精确一致（改 echo 段后重数指令再改常量）。
pub fn user_svcmgr_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_svcmgr, &__user_svcmgr_end) }
}

/// 一个用户进程的装配结果：表根 + ASID（交给 `spawn_user`）+ 地址空间根。
///
/// v1 对象常驻不回收（页表帧/栈帧均不退）；**拥有关系的 Vmo 以结构体泄漏持有**
/// （3-2：Vmar Region 存 Vmo 对象指针——unmap 回调映射计数要求对象存活；
/// 本结构 v1 永不销毁，指针恒有效）。good 的 root Vmar 由 boot `take()` 出去
/// 进引导表（首能力）；rogue/abuser 的留结构体内泄漏。
pub struct UserProcess {
    pub ttbr0_pa: usize,
    pub asid: u16,
    /// 地址空间根（good 的被 boot 取走进引导表；其余泄漏持有）。
    pub vmar: Option<Vmar>,
    /// code/data/stack 三 Vmo（Region 指针有效性依托；见结构体注释）。
    _vmos: [Vmo; 3],
}

impl UserProcess {
    /// 建一个用户进程：root Vmar + code/data/stack 三页（全 `UserRw`）+ 拷代码 +
    /// 写数据页魔数。栈页全零（sp0 自顶向下）。
    pub fn build(fa: &mut FrameAllocator, code: &[u8], data_magic: u64) -> Self {
        assert!(code.len() <= PAGE_SIZE, "用户代码模板须单页");
        assert!(code.len() % 4 == 0, "用户代码模板须 4 字节对齐长度");
        let asid = alloc_asid();
        let mut vmar = Vmar::new_root(fa, asid).expect("user: 建 root Vmar（boot 可信语境）");

        let mut code_vmo = Vmo::create(PAGE_SIZE).expect("user: 装 code VMO（boot 可信语境）");
        code_vmo.commit(fa, 0, PAGE_SIZE).expect("user: commit code");
        code_vmo.write(0, code).expect("user: 拷代码");

        let mut data_vmo = Vmo::create(PAGE_SIZE).expect("user: 装 data VMO（boot 可信语境）");
        data_vmo.commit(fa, 0, PAGE_SIZE).expect("user: commit data");
        data_vmo.write(0, &data_magic.to_le_bytes()).expect("user: 数据页魔数");

        let mut stack_vmo = Vmo::create(PAGE_SIZE).expect("user: 装 stack VMO（boot 可信语境）");
        stack_vmo.commit(fa, 0, PAGE_SIZE).expect("user: commit stack");

        vmar.map(fa, &mut code_vmo, USER_CODE_VA, 0, PAGE_SIZE, Perm::UserRw)
            .expect("user: map code");
        vmar.map(fa, &mut data_vmo, USER_DATA_VA, 0, PAGE_SIZE, Perm::UserRw)
            .expect("user: map data");
        vmar.map(fa, &mut stack_vmo, USER_STACK_BASE, 0, PAGE_SIZE, Perm::UserRw)
            .expect("user: map stack");

        // 表树对页表走查可见（map 内 dsb ish）；三 Vmo 进结构体（见结构体注释）。
        Self {
            ttbr0_pa: vmar.ttbr0_pa(),
            asid,
            vmar: Some(vmar),
            _vmos: [code_vmo, data_vmo, stack_vmo],
        }
    }
}

/// G1.5 验收看门狗（内核任务）：等 rogue_ptr 被杀、abuser（坏 handle 滥用）与
/// good 的 console 排水 / switches 金丝雀仍在涨 → 打关卡标记 `G1.5 ok` 后
/// `exit_current` 退场（A/B/good/abuser 继续分时）。杀数超期望（abuser 被误杀
/// = 3-1 错误码政策破坏）或超时未达成 → panic（系统停 = 关卡失败）。
/// 4-5a 起金丝雀 = console 服务排水计数（EL0 打印异步化：send 入队 ≠ 落串口，
/// 活证取**服务侧**——console 任务活着且在排空 = 用户打印链整体活着）。
pub extern "C" fn watchdog(_arg: usize) -> ! {
    const TRIES: u32 = 2_000;
    let mut last_puts = crate::console_drained();
    let mut last_switches = berry_sched::switches();
    for _ in 0..TRIES {
        // 忙等一拍（无 sleep 原语——Phase 4 通知落地前的土法；轮到别的任务跑）。
        for _ in 0..200_000 {
            core::hint::spin_loop();
        }
        let kills = berry_sched::user_kills();
        let puts = crate::console_drained();
        let switches = berry_sched::switches();
        if kills > 1 {
            panic!("G1.5 FAIL: 杀数 {} > 1（abuser/good 被误杀 = 错误码政策或验收链破坏）", kills);
        }
        if kills == 1 {
            // 杀对了人才能过关：kills==1 但死的是 good/abuser（如 3-2 验收链断链
            // 自陷）不算 PASS——只认 rogue_ptr。
            if berry_sched::last_killed() != b"user_rogue_ptr".as_slice() {
                panic!("G1.5 FAIL: 错杀（须只杀 rogue_ptr；死者看上一条 kill dump）");
            }
            if puts > last_puts && switches > last_switches {
                // 3-4c 遥测环存活断言：环计**全部**三型切换 ≥ SWITCHES（只计抢占），
                // 且环内 vct 从旧到新单调（乱序 = 写路径并发/覆盖 bug）。
                let tel = crate::telemetry::total();
                assert!(tel >= switches, "G1.5 FAIL: 遥测环未跟上（total={} < switches={}）", tel, switches);
                assert!(crate::telemetry::check(), "G1.5 FAIL: 遥测环 vct 非单调");
                // 4-0 配套唤醒环断言：sleeper 双路径全过（BLOCK_BOOT_DONE 置位
                // 前提 = 两条 boot 记录已落环）后才验——①计数 ≥ 2；②健康自检
                // （单调/非负延迟/原因合法）；③环未淘汰期（total ≤ 容量）加验
                // 两类原因都见过（淘汰后掩码只反映环内现状，不再断言）。
                if crate::BLOCK_BOOT_DONE.load(core::sync::atomic::Ordering::Relaxed) {
                    let wt = crate::telemetry::wake_total();
                    assert!(wt >= 2, "G1.5 FAIL: 唤醒环未记满 boot 双路径（wake_total={}）", wt);
                    assert!(crate::telemetry::wake_check(), "G1.5 FAIL: 唤醒环非单调/负延迟/非法原因");
                    if wt <= crate::telemetry::WAKE_RING_LEN as u64 {
                        let mask = crate::telemetry::wake_reason_mask();
                        assert!(mask == 0b11, "G1.5 FAIL: 唤醒环缺路径（mask={:#b}，须 Signaled|Timeout 双覆盖）", mask);
                    }
                }
                berry_hal_aarch64::uart_puts(b"\nG1.5 ok (rogue dead, abuser alive, canaries alive)\n");
                berry_sched::exit_current()
            }
        }
        last_puts = puts;
        last_switches = switches;
    }
    panic!("G1.5 FAIL: 超时（kills={} drained={}）", berry_sched::user_kills(), last_puts);
}
