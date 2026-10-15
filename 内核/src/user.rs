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
//! [`watchdog`] 内核任务断言杀数==1 + switches/putchar 金丝雀仍在涨 → 打 `G1.5 ok`
//! （smoke 固化标记）后退出。

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
    "    mov  x0, #0xFF",               // NR_DEBUG_PUTCHAR（调试槽，非 ABI）
    "    mov  x1, #0x75",               // 'u'
    "    svc  #0",
    "    cbnz x0, 3f",                  // putchar 失败 → 自陷路径
    "    b    0b",
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
    "    mov  x0, #0xFF",              // NR_DEBUG_PUTCHAR：'S' = 存活金丝雀
    "    mov  x1, #0x53",
    "    svc  #0",
    "    b    0b",
    "3:  movz x4, #0x4000, lsl #16",   // 自陷路径（被杀 = FAIL 暴露）
    "    ldr  x5, [x4]",
    "    b    .",
    ".globl __user_abuser_end",
    "__user_abuser_end:",

    // 3-3/3-4 spawner：经真 syscall 创建第二个进程 + **双向能力迁移**（3-4a
    // bootstrap 随 start + 3-4b handle 随消息）。暂存：x16=hp(进程)、x17=hv(子
    // Vmar)、x18=hvmo(子代码+栈)、x19=ht(线程)、x20=hgift(礼物 VMO，send 后
    // 失去)、x21=end0(channel 自留端)、x22=end1(作 bootstrap 迁子)；槽：
    // 0x2010/0x2018 handle 双出参、0x2020 vmo/线程 out、0x2028 vaddr in-out、
    // 0x2030 礼物 out、0x2038 礼物魔数、0x2040/0x2048 channel 双出参、0x2050
    // PING 载荷、0x2058 回信 buf、0x2060/0x2068 recv 容量 in-out、0x2070 随行
    // handle 数组（u32）。
    ".balign 16",
    ".globl __user_spawner",
    "__user_spawner:",
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
    "    movz x4, #324",                // 子代码 81 指令（字节数精确——尾随零垃圾勿拷）
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
    "    movz x0, #0x51",               // channel_create(*out0=0x2040, *out1=0x2048)
    "    movz x1, #0x2040",
    "    movz x2, #0x2048",
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
    "    str  w20, [x6]",               // 随行 handle 数组（u32 LE）= 礼物
    "    movz x0, #0x52",               // send(end0, PING, 8, &礼物, 1)——handle 随消息迁出本表
    "    mov  x1, x21",
    "    movz x2, #0x2050",
    "    movz x3, #8",
    "    movz x4, #0x2070",
    "    movz x5, #1",
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
    "10: movz x0, #0xFF",
    "    movz x1, #0x73",               // 's' = spawner 存活金丝雀
    "    svc  #0",
    "    b    0b",
    "3:  movz x4, #0x4000, lsl #16",    // 自陷路径（被杀 = FAIL 暴露）
    "    ldr  x5, [x4]",
    "    b    .",
    // ---- 子进程代码（数据形态嵌在 spawner 模板内；vmo_write 拷进子 0x1000）----
    // 3-4b：x0 = bootstrap（end1 在**子表**内的 handle）。一次性首链：recv 出
    // PING（验 USER_MAGIC）+ 随行礼物 handle → 凭**自己表里**的 handle vmo_read
    // 出魔数比对（能力随消息跨表流动自证）。稳态：收 PING → 回 REPLY → 打 'C'
    // （空 -EAGAIN / 满 -EBUSY 容忍，与父节流同量级，无死锁）。链断 → 自陷被杀
    // → watchdog 错杀断言 FAIL。子暂存：x19=end1、x8=PING 低 32 位常驻；槽（栈
    // 页 [0x2000,0x3000) 内）：0x2F90 REPLY 载荷、0x2FA0 收件 buf、0x2FA8/0x2FB0
    // 容量 in-out、0x2FB8 随行 handle 数组、0x2FC0 礼物读出 buf。
    "4:  mov  x19, x0",                 // 保存 end1（子表 handle）
    "    movz x4, #0x0E20",             // REPLY 载荷 = 0x00C0FFEE_0000_0E20 @0x2F90
    "    movk x4, #0xE000, lsl #16",
    "    movk x4, #0xFFEE, lsl #32",
    "    movk x4, #0x00C0, lsl #48",
    "    movz x5, #0x2F90",
    "    str  x4, [x5]",
    "    movz x4, #8",                  // 首链 caps：nbytes=8 / nhandles=1
    "    str  x4, [x5, #24]",
    "    movz x4, #1",
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
    "    ldr  x7, [x6]",                // 实收 nhandles 必须 == 1
    "    cmp  x7, #1",
    "    b.ne 8f",
    "    ldr  w7, [x6, #8]",            // 礼物 handle（u32，零扩进 x7）
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
    "12: movz x0, #0xFF",               // NR_DEBUG_PUTCHAR：'C' = 子进程活证
    "    movz x1, #0x43",
    "    svc  #0",
    "    b    5b",
    "8:  movz x4, #0x4000, lsl #16",    // 自陷：迁移/读链断 → 被杀 → watchdog FAIL
    "    ldr  x5, [x4]",
    "    b    .",
    ".globl __user_spawner_end",
    "__user_spawner_end:",
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
/// good 的 putchar / switches 金丝雀仍在涨 → 打关卡标记 `G1.5 ok` 后 `exit_current`
/// 退场（A/B/good/abuser 继续分时）。杀数超期望（abuser 被误杀 = 3-1 错误码政策
/// 破坏）或超时未达成 → panic（系统停 = 关卡失败）。
pub extern "C" fn watchdog(_arg: usize) -> ! {
    const TRIES: u32 = 2_000;
    let mut last_puts = berry_hal_aarch64::syscall::user_putcnt();
    let mut last_switches = berry_sched::switches();
    for _ in 0..TRIES {
        // 忙等一拍（无 sleep 原语——Phase 4 通知落地前的土法；轮到别的任务跑）。
        for _ in 0..200_000 {
            core::hint::spin_loop();
        }
        let kills = berry_sched::user_kills();
        let puts = berry_hal_aarch64::syscall::user_putcnt();
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
                berry_hal_aarch64::uart_puts(b"\nG1.5 ok (rogue dead, abuser alive, canaries alive)\n");
                berry_sched::exit_current()
            }
        }
        last_puts = puts;
        last_switches = switches;
    }
    panic!("G1.5 FAIL: 超时（kills={} puts={}）", berry_sched::user_kills(), last_puts);
}
