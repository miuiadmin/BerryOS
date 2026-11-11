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

/// 用户 VA 布局：**D3（G-eco-b）起按镜像长度动态**——code @0x1000 占
/// `ceil(len/页)*页`，data/stack 顺次上移，sp 见 `UserProcess::stack_top`。
/// 单页程序（现存全员除 svcmgr）布局与旧三页固定布局逐位相同：data @0x2000、
/// stack @[0x3000,0x4000)。全 UserRw、可执行（无 NX——v2 批次开，见安全设计）。
pub const USER_CODE_VA: usize = 0x1000;
/// 单页布局的 sp0（16 对齐；多页进程改读 `UserProcess::stack_top`）。
pub const USER_STACK_TOP: usize = 0x4000;

/// TLB shootdown canary 双页 data VA（S-4 4-4）：page0 = 0x4800_0000 canary
/// 写点（warm + unmap 后写回期望 fault）/ page1 = +0x1000（VMO off 0x1000:
/// +0x0 clock 出参 / +0x10 flag）/ probe = +0x2_0000（永不映射——slot1 汇报
/// 点；64K 粒度单 movz 产物）。三面钉同值：`__user_tlbc` 模板 movz/movk
/// 立即数 + main.rs task_tlbs arm/unmap 实参——改值须两处同步（asm 侧注释
/// 已回指本行）。**选址纪律（§A119，两轮撞车沉淀）**：canary 槽 arm 后整个
/// boot 期全局常驻、纯数值 FAR 匹配无进程校验——同 boot 并发载荷的任意同
/// VA fault 都被静默吞 +1 且该杀不杀。选址须过全仓**双面净区扫描**：①内
/// 核侧——ktest 自杀 VA 族钉死 0x4000_0000（本文件 :14/:245/:478/:600/:759/
/// :897/:1076，首轮撞车 11≠10 实锤）；②user/ 侧常驻 VA 段表——mmap 窗
/// [0x5000_0000,0x6000_0000)（berry-mmap）/ **pthread 栈段 [0x6000_0000,
/// 0x7000_0000)**（berry-libc STACK_VA_BASE 段基 = 首栈保护页恰在段基——
/// 二轮撞车，审查实抓）/ virtio MMIO 0x7000_0000 + FB 0x7100_0000（berry-
/// exec）/ 用户 ELF 链接基 0x1000 低端。**0x4800_0000 = DRAM 物理窗外
/// （[0x4000_0000,0x4800_0000) 128M，物理不可达故语义无主）+ mmap 窗前
/// 0x800_0000 远**——双面净。
pub const TLBS_DATA_VA: usize = 0x4800_0000;

/// S-5b 相 6/7 storm fleet 共用数据 VMO 窗 VA（ALLOC churn / HT 风暴）。
/// 选址纪律同上（§A119 双面净区扫描）：0x4900_0000 与 0x4800 同段
/// （DRAM 物理窗外 + mmap 窗 0x700_0000 远），全仓双面零命中实证
/// （2026-11-03，含 user/ 常驻 VA 段表）。布局：page0 = go-flag 页
/// （master vmo_write 放行位——基线语义完美化：fleet 帧先立 → 快照 →
/// 放行 → 只对账瞬态 churn）；page1..(1+N) = per-worker 窗口页
/// （模板 SP 自推导：sp & ~0xFFF = 自身窗口基——解 out-param 并发踩踏，
/// 窗口偏移 0x40 出参/0x80 err/0x90 完成轮数/0xC0 write 源，main.rs
/// 常量三面钉同值）。
pub const STORM_DATA_VA: usize = 0x4900_0000;

/// 代码镜像帽（D3；A7-4 段② 64→128KiB——svcmgr 平像贴墙后 SDK-1b 型
/// doc 漂移越页，C-2c 容量同步先例抬墙）：flat binary 约定输入的体积上限
/// ——防调用方手滑把整段 ELF 塞进来（elf2flat.py `--max 131072` 同值执法）。
pub const USER_CODE_MAX: usize = 128 * PAGE_SIZE;

/// 数据页 canary（非零——good 的「页内容正确」自检依据）。
pub const USER_MAGIC: u64 = 0x00C0_FFEe_E000_0E10;

// ---- 用户程序机器码模板（ARM64）----
//
// 两种产源（G-eco-a 起）：编译产物（user/bins/* 经 just build-user 出
// flat binary，`include_bytes!` 直嵌——abuser 是首个换轨者）与 global_asm
// 手写（其余模板，逐个换轨中）。约定：入口 x0=arg。good 的 arg = root Vmar handle（3-2 首能力，boot 从引导
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
    "    cmn  x0, #16",                 // -EBUSY：阻塞等 WRITABLE 再重试（雷 2 残留
    "    b.ne 3f",                      //   修复——纯自旋在饱和下被排水窗口结构性饿死）
    "6:  movz x0, #0x61",               // clock_read() → 0x2008
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x3, [x2]",                // now
    "    movz x6, #0xE400",             // δ = 1e10 ns（10s 帽）
    "    movk x6, #0x540B, lsl #16",
    "    movk x6, #0x2, lsl #32",
    "    add  x3, x3, x6",              // deadline = now + 10s
    "    movz x0, #0x04",               // wait_one(console, WRITABLE, deadline, *obs)
    "    movz x1, #2",
    "    movk x1, #1, lsl #16",
    "    movz x2, #2",                  // SIGNAL_WRITABLE（内核派生位）
    "    movz x4, #0x2008",             // obs 槽（时钟值已消费，覆盖安全）
    "    svc  #0",
    "    cbz  x0, 4b",                  // 命中（poll-first 可即中）→ 重发
    "    cmn  x0, #110",                // -ETIMEDOUT：重臂再等（打印路径无限耐心——
    "    b.eq 6b",                      //   与原自旋同耐心，只换阻塞不烧片）
    "    b    3f",                      // 其它错 = 断链
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

    // （abuser 模板 G-eco-a 起退役为编译产物：user/bins/abuser →
    //  just build-user → kernel/src/userbins/abuser.bin，见 user_abuser_code()。）

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
    "    movz x0, #0x15",               // vmo_write(hvmo, buf=内嵌子代码, 0, 452)
    "    mov  x1, x18",
    "    adr  x2, 4f",                  // 本页内自相对取址（搬迁安全，见模块注）
    "    mov  x3, xzr",
    "    movz x4, #452",                // 子代码 113 指令（字节数精确——尾随零垃圾勿拷）
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
    "    cmn  x0, #16",                // -EBUSY：阻塞等 WRITABLE 再重试（雷 2 残留修
    "    b.ne 3f",                     //   复——纯自旋在饱和下被排水窗口结构性饿死）
    "17: movz x0, #0x61",              // clock_read() → 0x2088（本页空闲槽）
    "    mov  x1, xzr",
    "    movz x2, #0x2088",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x3, [x2]",               // now
    "    movz x6, #0xE400",            // δ = 1e10 ns（10s 帽）
    "    movk x6, #0x540B, lsl #16",
    "    movk x6, #0x2, lsl #32",
    "    add  x3, x3, x6",             // deadline = now + 10s
    "    movz x0, #0x04",              // wait_one(console, WRITABLE, deadline, *obs)
    "    mov  x1, x23",
    "    movz x2, #2",                 // SIGNAL_WRITABLE（内核派生位）
    "    movz x4, #0x2088",            // obs 槽（时钟值已消费，覆盖安全）
    "    svc  #0",
    "    cbz  x0, 13b",                // 命中（poll-first 可即中）→ 重发
    "    cmn  x0, #110",               // -ETIMEDOUT：重臂再等（打印路径无限耐心）
    "    b.eq 17b",
    "    b    3f",                     // 其它错 = 断链
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
    "    cmn  x0, #16",                // -EBUSY：阻塞等 WRITABLE 再重试（雷 2 残留修
    "    b.ne 8f",                     //   复——纯自旋在饱和下被排水窗口结构性饿死）
    "18: movz x0, #0x61",              // clock_read() → 0x2FD8（栈页空闲槽）
    "    mov  x1, xzr",
    "    movz x2, #0x2FD8",
    "    svc  #0",
    "    cbnz x0, 8f",
    "    ldr  x3, [x2]",               // now
    "    movz x6, #0xE400",            // δ = 1e10 ns（10s 帽）
    "    movk x6, #0x540B, lsl #16",
    "    movk x6, #0x2, lsl #32",
    "    add  x3, x3, x6",             // deadline = now + 10s
    "    movz x0, #0x04",              // wait_one(console, WRITABLE, deadline, *obs)
    "    mov  x1, x20",
    "    movz x2, #2",                 // SIGNAL_WRITABLE（内核派生位）
    "    movz x4, #0x2FD8",            // obs 槽（时钟值已消费，覆盖安全）
    "    svc  #0",
    "    cbz  x0, 15b",                // 命中（poll-first 可即中）→ 重发
    "    cmn  x0, #110",               // -ETIMEDOUT：重臂再等（打印路径无限耐心）
    "    b.eq 18b",
    "    b    8f",                     // 其它错 = 断链
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
    //   ③ 长睡被唤醒：清掩码 → wait(U4, t2+10s) → waker2 经真 SVC
    //      signal(内核表 handle, U4) → 醒来重验命中 → 0 + observed==U4。
    // 4-2 位空间：Event 位面迁高 32 半——U2=1<<37（0x20<<32）、U3=1<<35
    // （0x8<<32）、超集 = 0x220<<32；清零掩码 = 用户半全集（低半越界掩码
    // 4-2 起 -EINVAL，旧 !0 清零不再合法）。
    // P5-0 位拆分（P4 收官审计修复）：握手信号专用位 U4=1<<38（0x40<<32），
    // 三处清位豁免 U4（0xFFFF_FFBF_0000_0000）——waker2 的一次性 signal 相对
    // waiter 三路是**任意相位**（原模板烤死「最早 3 slice 后才置」的调度时序
    // 假设：早到信号被清位抹掉 → 路 3 十秒空等超时 / 未抹掉则路 1 意外命中
    // 严断 -110 炸自陷——dump 相位随 run 漂移）。豁免后早到信号任一路无害：
    // 路 1/2 等 U3/U2 不看 U4，路 2 observed 先掩 bit38 再断言，路 3 等/验
    // U4——poll 即中或睡到置位，两相位同过。模板纪律：**握手信号用专用位、
    // 清位豁免——模板不烤调度时序假设**。
    // 槽：0x2008 时钟、0x2010 observed。寄存器纪律：svc 只回写 x0，x1..x6 保
    // 留传入值（路 1 的 deadline 留 x3、δ 留 x6 直接复用）。
    ".balign 16",
    ".globl __user_waiter",
    "__user_waiter:",
    "    mov  x20, x0",                 // x20 常驻：Event handle（waiter 表）
    // ---- 路 1：超时自醒 ----
    "    movz x0, #0x06",               // signal(h, UMASK, 0) 清零（出生必零，防御位）
    "    mov  x1, x20",
    "    movz x2, #0xFFBF, lsl #32",    // x2 = 用户半全集掩码～U4（bit38 豁免，头注）
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
    "    movz x2, #0xFFBF, lsl #32",
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
    "    bfc  x8, #38, #1",             // 掩掉早到 U4（全满足集含它——非断言面）
    "    movz x9, #0x20, lsl #32",
    "    cmp  x8, x9",                  // 须恰 U2（全满足集 = current）
    "    b.ne 3f",
    // ---- 路 3：长睡被 waker2 signal 唤醒 ----
    "    movz x0, #0x06",               // signal(h, UMASK, 0) 再清零（U4 豁免——
    "    mov  x1, x20",                 //   早到的 waker2 置位须存活到路 3）
    "    movz x2, #0xFFBF, lsl #32",
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
    "    movz x0, #0x04",               // wait_one(h, U4, far, *obs)
    "    mov  x1, x20",
    "    movz x2, #0x40, lsl #32",      // U4 = 1<<38（握手专用位，头注）
    "    movz x4, #0x2010",
    "    svc  #0",                      // 睡到 waker2 signal(U4) 或 poll 即中
    "    cbnz x0, 3f",                  // （早到相位 poll 命中同过；真睡由路 1 验）
    "    ldr  x8, [x4]",
    "    movz x9, #0x40, lsl #32",
    "    cmp  x8, x9",                  // observed == U4（全满足集）
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
    "    cmn  x0, #16",                // -EBUSY：阻塞等 WRITABLE 再重试（雷 2 残留修
    "    b.ne 3f",                     //   复——纯自旋在饱和下被排水窗口结构性饿死）
    "6:  movz x0, #0x61",              // clock_read() → 0x2008
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x3, [x2]",               // now
    "    movz x6, #0xE400",            // δ = 1e10 ns（10s 帽）
    "    movk x6, #0x540B, lsl #16",
    "    movk x6, #0x2, lsl #32",
    "    add  x3, x3, x6",             // deadline = now + 10s
    "    movz x0, #0x04",              // wait_one(console, WRITABLE, deadline, *obs)
    "    movz x1, #2",
    "    movk x1, #1, lsl #16",
    "    movz x2, #2",                 // SIGNAL_WRITABLE（内核派生位）
    "    movz x4, #0x2010",            // obs 槽（握手已毕，稳态可覆盖）
    "    svc  #0",
    "    cbz  x0, 4b",                 // 命中（poll-first 可即中）→ 重发
    "    cmn  x0, #110",               // -ETIMEDOUT：重臂再等（打印路径无限耐心）
    "    b.eq 6b",
    "    b    3f",                     // 其它错 = 断链
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
    "    movz x3, #0x8C80",             // δ = 3e10 ns（30s 帽；原 10s——S-2b 双核
    "    movk x3, #0xFC18, lsl #16",    // BKL 争用 + MTTCG 互拖调度税下 ponger 轮转
    "    movk x3, #0x6, lsl #32",       // 变慢实测超 10s（pinger -110 自陷错杀，
    "    add  x3, x5, x3",              // deadline = now + 30s）
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
    "    cmn  x0, #16",                 // -EBUSY：阻塞等 WRITABLE 再重试（雷 2 残留
    "    b.ne 3f",                      //   修复——纯自旋在饱和下被排水窗口结构性饿死）
    "10: movz x0, #0x61",               // clock_read() → 0x2008
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x3, [x2]",                // now
    "    movz x6, #0xE400",             // δ = 1e10 ns（10s 帽）
    "    movk x6, #0x540B, lsl #16",
    "    movk x6, #0x2, lsl #32",
    "    add  x3, x3, x6",              // deadline = now + 10s
    "    movz x0, #0x04",               // wait_one(console, WRITABLE, deadline, *obs)
    "    movz x1, #2",
    "    movk x1, #1, lsl #16",
    "    movz x2, #2",                  // SIGNAL_WRITABLE（内核派生位）
    "    movz x4, #0x2010",             // obs 槽（验收已毕，稳态可覆盖）
    "    svc  #0",
    "    cbz  x0, 6b",                  // 命中（poll-first 可即中）→ 重发
    "    cmn  x0, #110",                // -ETIMEDOUT：重臂再等（打印路径无限耐心）
    "    b.eq 10b",
    "    b    3f",                      // 其它错 = 断链
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
    "    cmn  x0, #16",                // -EBUSY：阻塞等 WRITABLE 再重试（同 banner 站）
    "    b.ne 3f",
    "11: movz x0, #0x61",              // clock_read() → 0x2008
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x3, [x2]",               // now
    "    movz x6, #0xE400",            // δ = 1e10 ns（10s 帽）
    "    movk x6, #0x540B, lsl #16",
    "    movk x6, #0x2, lsl #32",
    "    add  x3, x3, x6",             // deadline = now + 10s
    "    movz x0, #0x04",              // wait_one(console, WRITABLE, deadline, *obs)
    "    movz x1, #2",
    "    movk x1, #1, lsl #16",
    "    movz x2, #2",                 // SIGNAL_WRITABLE（内核派生位）
    "    movz x4, #0x2010",            // obs 槽（验收已毕，稳态可覆盖）
    "    svc  #0",
    "    cbz  x0, 8b",                 // 命中 → 重发
    "    cmn  x0, #110",               // -ETIMEDOUT：重臂再等
    "    b.eq 11b",
    "    b    3f",                     // 其它错 = 断链
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
    "    cmn  x0, #16",                 // -EBUSY：阻塞等 WRITABLE 再重试（雷 2 残留
    "    b.ne 3f",                      //   修复——纯自旋在饱和下被排水窗口结构性饿死）
    "10: movz x0, #0x61",               // clock_read() → 0x2008
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x3, [x2]",                // now
    "    movz x6, #0xE400",             // δ = 1e10 ns（10s 帽）
    "    movk x6, #0x540B, lsl #16",
    "    movk x6, #0x2, lsl #32",
    "    add  x3, x3, x6",              // deadline = now + 10s
    "    movz x0, #0x04",               // wait_one(console, WRITABLE, deadline, *obs)
    "    movz x1, #3",                  // console handle 0x0001_0003
    "    movk x1, #1, lsl #16",
    "    movz x2, #2",                  // SIGNAL_WRITABLE（内核派生位）
    "    movz x4, #0x2010",             // obs 槽（wait_many 每次重写，覆盖安全）
    "    svc  #0",
    "    cbz  x0, 7b",                  // 命中（poll-first 可即中）→ 重发
    "    cmn  x0, #110",                // -ETIMEDOUT：重臂再等（打印路径无限耐心）
    "    b.eq 10b",
    "    b    3f",                      // 其它错 = 断链
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
    "    cmn  x0, #16",                // -EBUSY：阻塞等 WRITABLE 再重试（同 banner 站）
    "    b.ne 3f",
    "11: movz x0, #0x61",              // clock_read() → 0x2008
    "    mov  x1, xzr",
    "    movz x2, #0x2008",
    "    svc  #0",
    "    cbnz x0, 3f",
    "    ldr  x3, [x2]",               // now
    "    movz x6, #0xE400",            // δ = 1e10 ns（10s 帽）
    "    movk x6, #0x540B, lsl #16",
    "    movk x6, #0x2, lsl #32",
    "    add  x3, x3, x6",             // deadline = now + 10s
    "    movz x0, #0x04",              // wait_one(console, WRITABLE, deadline, *obs)
    "    movz x1, #3",
    "    movk x1, #1, lsl #16",
    "    movz x2, #2",                 // SIGNAL_WRITABLE（内核派生位）
    "    movz x4, #0x2010",            // obs 槽（wait_many 每次重写，覆盖安全）
    "    svc  #0",
    "    cbz  x0, 8b",                 // 命中 → 重发
    "    cmn  x0, #110",               // -ETIMEDOUT：重臂再等
    "    b.eq 11b",
    "    b    3f",                     // 其它错 = 断链
    "9:  b    0b",
    "3:  movz x4, #0x4000, lsl #16",    // 自陷：链断 → 被杀 → CHAIN 缺失 = smoke FAIL
    "    ldr  x5, [x4]",
    "    b    .",
    "4:  .ascii \"MANY ok\\n\"",
    "    .balign 4",                   // 尾部补齐（模板长度须 4 字节对齐）
    ".globl __user_ponger_end",
    "__user_ponger_end:",

    // ---- S-3b b-2 LB 金丝雀子模板（main.rs lb_master 经 svc 链造 5 份）----
    //
    // 纯 CPU-bound 自旋 + svc 0x36 汇报：`mrs CNTVCT_EL0` 差值 vs D 常数
    // （500M 周期 ≈ 8s 名义 @62.5MHz——QEMU virt CNTFRQ 常量，多处先例）；
    // TCG 半速（雷 A8）→ 墙钟 ~16s。无 block/wake（LB 判据纯净：直路路由
    // 与偷取是仅有的跨核路径）；无打印（裸相零噪声——LB ok 行归 lb_master
    // 汇总）；不用 x0（bootstrap=0 造入，无能力需求）。EL0VCTEN 双核均已
    // 开（timer::init 主核 B-1 修复 + 副核 bring-up 自开）——静默杀死状排障
    // 先查 F-A9（本模板设计已排该面）。svc 0x36 = thread_exit：**单进程五
    // 任务形态的生死线**——0x33 会 kill_all_tasks_by_pinned 连杀同进程四条
    // 在转兄弟（run7/8 后形态裁决），0x36 仅退自身 + join 唤醒（非末线程
    // 「进程续活」），谁先转完谁先汇报、互不牵连；「汇报」即退出本身。
    ".globl __user_lb",
    "__user_lb:",
    "    mrs  x9, CNTVCT_EL0",         // t0
    "    movz x10, #0x6500",
    "    movk x10, #0x1DCD, lsl #16",  // D = 0x1DCD_6500 = 500M 周期 ≈ 8s @62.5MHz
    "    add  x10, x9, x10",           // deadline = t0 + D
    "1:  mrs  x9, CNTVCT_EL0",
    "    cmp  x9, x10",
    "    b.le 1b",                     // 自旋到点（纯忙转，无系统调用）
    "    movz x0, #0x36",              // thread_exit（svc 汇报——仅退自身）
    "    svc  #0",
    "    b    .",                      // 不返；防御兜底（返回 = fault 杀兜底）
    ".globl __user_lb_end",
    "__user_lb_end:",

    // ---- S-4 4-4 TLB shootdown canary 子模板（main.rs task_tlbs 经 svc 链
    // 造 5 份：root 家核 + 4 写者 spread→CPU1）----
    //
    // 五步 canary 的用户侧半场：①warm str×4（page0 旧映射驻留本核 TLB）→
    // ②probe ldr（base+0x2_0000 永不映射 → 必 fault → trap.rs CANARY_SLOTS
    // slot1 静默恢复计数 = per-核 warm 汇报，master 据此收栅障）→ ③风暴轮询
    // 环（每迭代一次 clock_read svc = 对核满载荷 SVC 风暴面 + page1+0x10
    // flag 轮询）→ ④flag 置 1 后写回 page0 旧 VA（master 已 unmap+还帧+复用
    // → stale TLB 则写入成功〔变异形态〕/ 已失效则必 fault → slot0 静默恢复
    // 计数 = canary 命中）→ ⑤thread_exit（0x36 仅退自身——单进程五任务形态
    // 生死线，__user_lb 头注同款）。
    // 寄存器合同：x20 = page0 基址（canary 写点）/ x21 = page1 基址（= base
    // +0x1000，VMO off 0x1000；+0x0 clock 出参 / +0x10 flag）/ x1 = probe VA
    // / x0 = NR→ret（svc dispatch 只回写 x0——x1-x30 跨 svc 稳定，svc4 头注
    // 同契约）。VA 三面钉同值：本模板 movz/movk 立即数 = main.rs TLBS_DATA_VA
    // （pub const 单源，0x4800_0000）+ task_tlbs arm/unmap 实参。
    ".globl __user_tlbc",
    "__user_tlbc:",
    "    movz x20, #0x4800, lsl #16",  // x20 = 0x4800_0000（page0：canary 写点）
    "    str  x20, [x20]",             // warm ×4（旧映射驻留本核 TLB）
    "    str  x20, [x20]",
    "    str  x20, [x20]",
    "    str  x20, [x20]",
    "    movz x1, #0x4802, lsl #16",   // probe VA = 0x4802_0000（base+0x2_0000，
    "                                   //  永不映射——64K 粒度单 movz 即得）",
    "    ldr  x2, [x1]",               // 必 fault → slot1 静默恢复（warm 汇报）
    "    movz x21, #0x4800, lsl #16",  // x21 = 0x4800_1000（page1 = VMO off
    "    movk x21, #0x1000",           //  0x1000——imm12 加不了 0x1000，movk 合成）
    "    //                                  +0x0 clock 出参 / +0x10 flag",
    "1:  movz x0, #0x61",              // NR_CLOCK_READ（SVC 风暴——满载荷面）
    "    movz x1, #0",                 // CLOCK_MONOTONIC
    "    mov  x2, x21",                // 出参 = page1+0x0（copy_out 8B）
    "    movz x3, #0",
    "    svc  #0",
    "    ldr  x3, [x21, #0x10]",       // flag（master vmo_write 放行位）
    "    cbz  x3, 1b",
    "    // canary 写点：stale→写入成功（变异形态）/ 已失效→必 fault→slot0",
    "    str  x20, [x20]",
    "    movz x0, #0x36",              // thread_exit（仅退自身）
    "    svc  #0",
    "    b    .",                      // 不返；防御兜底
    ".balign 4",                       // 尾部补齐（模板长度 4 字节对齐）
    ".globl __user_tlbc_end",
    "__user_tlbc_end:",

    // S-5b 相 6 ALLOC churn worker（storm fleet——tlbs_spawn_fleet 形态，
    // 双核 EL0 分配器压测）。流程：①SP 自推导窗口（mov/lsr/lsl——
    // sp & ~0xFFF = 自身窗口页基；fleet 按线程错落 sp 派发窗口，解
    // out-param 并发踩踏）→ ②go-flag 自旋（flag = 绝对 VA movz 合成
    // ——**邻页推导（win-0x1000）仅 root 成立**，写者窗口前页是他人
    // 窗口读恒 0 永自旋，S-5b 首跑实证；0x4900 与 STORM_DATA_VA 三面钉）
    // 纯 EL0 装载环——park 期零内核态面，master 水位快照语义成立 →
    // ③64 轮 churn：vmo_create(0x11)→commit(0x12)→write(0x15, 8B)→
    // close(0x01)，任一 svc 非零 →err 路径（win+0x80 计数，master 即红
    // 早断）→ ④轮数计满 win+0x90 → ⑤thread_exit（0x36 仅退自身——
    // 单进程五任务生死线，__user_lb 头注同款）。寄存器合同：x20 = 窗口基
    // / x23 = flag 页（绝对）/ x22 = 剩余轮数 / x0 = NR→ret（svc
    // dispatch 只回写 x0——x1-x30 跨 svc 稳定；vmo_write 后 x4 已转 len，
    // close 须从 win+0x40 重载 handle）。ROUNDS=64 与 main.rs STORM_ROUNDS
    // 三面钉。
    ".globl __user_allocst",
    "__user_allocst:",
    "    mov  x21, sp",               // SP 自推导（lsr/lsl 合成页基）
    "    lsr  x21, x21, #12",
    "    lsl  x20, x21, #12",         // x20 = 自身窗口页基
    "    movz x23, #0x4900, lsl #16", // x23 = flag 页（绝对 VA 三面钉）
    "1:  ldr  x3, [x23]",             // go-flag 自旋
    "    cbz  x3, 1b",
    "    movz x22, #64",              // ROUNDS（三面钉）
    "2:  movz x0, #0x11",             // vmo_create(0x1000, 0, win+0x40)
    "    movz x1, #0x1000",           //  imm12 恰容 0x1000
    "    movz x2, #0",
    "    add  x3, x20, #0x40",
    "    svc  #0",
    "    cbnz x0, 9f",
    "    ldr  x4, [x20, #0x40]",      // churn handle（copy_out 已落窗）
    "    movz x0, #0x12",             // vmo_commit(h, 0, 0x1000)
    "    mov  x1, x4",
    "    movz x2, #0",
    "    movz x3, #0x1000",
    "    svc  #0",
    "    cbnz x0, 9f",
    "    movz x0, #0x15",             // vmo_write(h, win+0xC0, 0, 8)
    "    mov  x1, x4",
    "    add  x2, x20, #0xC0",
    "    movz x3, #0",
    "    movz x4, #8",                // x4 转入 len（4 参签名）
    "    svc  #0",
    "    cbnz x0, 9f",
    "    movz x0, #0x01",             // handle_close(h)
    "    ldr  x1, [x20, #0x40]",      // 重载（x4 已被 write 借走）
    "    movz x2, #0",
    "    movz x3, #0",
    "    svc  #0",
    "    cbnz x0, 9f",
    "    ldr  x5, [x20, #0x90]",      // 完成轮数++
    "    add  x5, x5, #1",
    "    str  x5, [x20, #0x90]",
    "    subs x22, x22, #1",
    "    b.ne 2b",
    "    b    3f",
    "9:  ldr  x6, [x20, #0x80]",      // err++（master 早断位）
    "    add  x6, x6, #1",
    "    str  x6, [x20, #0x80]",
    "3:  mov  x5, #1",                // 就绪位自宣（win+0x98——master 静默收割）
    "    str  x5, [x20, #0x98]",
    "4:  ldr  x3, [x23, #8]",         // 第二放行位（flag 页+8）——park 自旋
    "    cbz  x3, 4b",                //（末线程收口 drain 前留水位终账窗）
    "    movz x0, #0x36",             // thread_exit（仅退自身）
    "    svc  #0",
    "    b    .",                     // 不返；防御兜底
    ".balign 4",
    ".globl __user_allocst_end",
    "__user_allocst_end:",

    // S-5b 相 7 HT 风暴 worker（同 fleet 形态，载荷换 event 风暴：每轮
    // event_create(0x54)→object_signal(0x06, set=高 32 位用户位——
    // SIGNAL_USER_MASK=0xFFFF_FFFF_0000_0000，低 32 位内核派生保留)→
    // handle_close(0x01)）。被打的共享结构 = 子进程 handle 表（单进程
    // 五线程跨两核共用一表，SVC_CLASS_SQ_RQ_TABLE 锁竞争面）。窗口
    // 自推导/go-flag/err 路径/0x36 退出全同 __user_allocst（头注互见）。
    ".globl __user_htst",
    "__user_htst:",
    "    mov  x21, sp",
    "    lsr  x21, x21, #12",
    "    lsl  x20, x21, #12",         // x20 = 自身窗口页基
    "    movz x23, #0x4900, lsl #16", // x23 = flag 页（绝对 VA 三面钉）
    "1:  ldr  x3, [x23]",
    "    cbz  x3, 1b",
    "    movz x22, #64",              // ROUNDS（三面钉）
    "2:  movz x0, #0x54",             // event_create(0, win+0x40)
    "    movz x1, #0",
    "    add  x2, x20, #0x40",
    "    svc  #0",
    "    cbnz x0, 9f",
    "    ldr  x4, [x20, #0x40]",
    "    movz x0, #0x06",             // object_signal(h, 0, 用户位)
    "    mov  x1, x4",
    "    movz x2, #0",
    "    movz x3, #1, lsl #32",       // 高 32 位用户位（movz lsl 合成）
    "    svc  #0",
    "    cbnz x0, 9f",
    "    movz x0, #0x01",             // handle_close(h)
    "    mov  x1, x4",
    "    movz x2, #0",
    "    movz x3, #0",
    "    svc  #0",
    "    cbnz x0, 9f",
    "    ldr  x5, [x20, #0x90]",      // 完成轮数++
    "    add  x5, x5, #1",
    "    str  x5, [x20, #0x90]",
    "    subs x22, x22, #1",
    "    b.ne 2b",
    "    b    3f",
    "9:  ldr  x6, [x20, #0x80]",      // err++
    "    add  x6, x6, #1",
    "    str  x6, [x20, #0x80]",
    "3:  mov  x5, #1",                // 就绪位自宣（win+0x98——master 静默收割）
    "    str  x5, [x20, #0x98]",
    "4:  ldr  x3, [x23, #8]",         // 第二放行位（flag 页+8）——park 自旋
    "    cbz  x3, 4b",                //（末线程收口 drain 前留静默走查窗）
    "    movz x0, #0x36",             // thread_exit（仅退自身）
    "    svc  #0",
    "    b    .",
    ".balign 4",
    ".globl __user_htst_end",
    "__user_htst_end:",

    // ---- S-5c 相 8：PP ping-pong 双角色模板（main.rs task_pp 经真 svc 链
    // 造 2 份：root A 家核 CPU0 + 写者 B thread_start spread→CPU1）----
    //
    // 单页单码 **arg 分流**（root x0=bootstrap=0 → A；thread_start arg=1 →
    // B）：双核 channel 对打面。A 先 channel_create 出两端点（outs 落自窗
    // 0xA0/0xA8），把端点 1 **中继**至 B 窗 0xA0（跨核 handle 传递无别桥
    // ——v1 无 IPC-for-handles，data 窗口中继是 fleet 形态唯一通道；B 自旋
    // 等就位 = 跨核 EL0 可见性顺带活证）。此后 N=320 轮严格交替 ping-pong：
    // A send(seq i)@endA → recv@endA 验回声==i；B recv@endB 验 seq==i →
    // echo send(i)@endB。**双向逐轮验序 = 零丢失/零乱序/零重复对账**（错
    // 即 ERR 早断红）。交替下发两方向队列深度恒 ≤1（EBUSY 仅防御重试；
    // EAGAIN 自旋 = 阻塞面的忙等形态）。窗口槽（allocst/htst 头注互见）：
    // 0x80 err / 0x90 DONE（各自 N）/ 0x98 READY 自宣 / 0xA0+0xA8 channel
    // outs（B 窗 0xA0 = 中继端点）/ 0xB0 载荷+收件 / 0xB8 回声 / 0xC0+0xC8
    // recv 双出参（cap=8 / nh=0 预写）。**无 go-flag/二次放行**：断言面 =
    // 窗口槽值（master 持 data VMO handle 直读；进程收口 drain 清的是子表
    // handle，洗不掉窗口页——S-5b「join 后恒真洞」不适用），DONE 收满即
    // thread_exit（0x36 仅退自身——同进程双线程互不牵连）；err 路径 =
    // ERR++ → 0x36 早退（master errs>0 早断红，不挂死）。
    ".globl __user_pp",
    "__user_pp:",
    "    cmp  x0, #1",                // arg 分流：1 = B 写者，0 = A root
    "    b.eq 4f",
    // ---- A（root）----
    "    mov  x21, sp",
    "    lsr  x21, x21, #12",
    "    lsl  x20, x21, #12",         // x20 = A 窗口页基（SP 自推导）
    "    movz x0, #0x51",             // channel_create(0, out0=0xA0, out1=0xA8)
    "    movz x1, #0",
    "    add  x2, x20, #0xA0",
    "    add  x3, x20, #0xA8",
    "    svc  #0",
    "    cbnz x0, 9f",
    "    ldr  x21, [x20, #0xA0]",     // x21 = endA 句柄（A 侧对打端）
    "    ldr  x4, [x20, #0xA8]",      // endB → 中继 B 窗 0xA0
    "    add  x5, x20, #0x1000",      // B 窗口页（相邻页——fleet sp 布局钉死）
    "    str  x4, [x5, #0xA0]",
    "    mov  x5, #1",                // READY 自宣
    "    str  x5, [x20, #0x98]",
    "    movz x8, #0x140",            // N=320（三面钉：PP_ROUNDS/smoke 断言）
    "    mov  x7, xzr",               // i
    "1:  str  x7, [x20, #0xB0]",      // 载荷槽 = seq i
    "    movz x0, #0x52",             // channel_send(endA, 0xB0, 8, 0, 0)
    "    mov  x1, x21",
    "    add  x2, x20, #0xB0",
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 2f",
    "    cmn  x0, #16",               // -EBUSY 防御重试（交替形队列 ≤1 不期遇）
    "    b.eq 1b",
    "    b    9f",
    "2:  movz x9, #8",                // recv(endA, 0xB8, &n=8, 0, &nh=0)
    "    str  x9, [x20, #0xC0]",
    "    str  xzr, [x20, #0xC8]",
    "    movz x0, #0x53",
    "    mov  x1, x21",
    "    add  x2, x20, #0xB8",
    "    add  x3, x20, #0xC0",
    "    mov  x4, xzr",
    "    add  x5, x20, #0xC8",
    "    svc  #0",
    "    cbz  x0, 3f",
    "    cmn  x0, #11",               // -EAGAIN 自旋（等 B 回声——对端活性面）
    "    b.eq 2b",
    "    b    9f",
    "3:  ldr  x9, [x20, #0xB8]",      // 回声序号必须 == i（A 面对账）
    "    cmp  x9, x7",
    "    b.ne 9f",
    "    add  x7, x7, #1",
    "    cmp  x7, x8",
    "    b.lo 1b",
    "    str  x8, [x20, #0x90]",      // DONE_A = N
    "    movz x0, #0x36",             // thread_exit（仅退自身——B 不受牵连）
    "    svc  #0",
    "    b    .",
    // ---- B（写者，spread→CPU1）----
    "4:  mov  x21, sp",
    "    lsr  x21, x21, #12",
    "    lsl  x20, x21, #12",         // x20 = B 窗口页基
    "5:  ldr  x21, [x20, #0xA0]",     // 等中继端点就位（A 落笔 → 跨核可见）
    "    cbz  x21, 5b",
    "    mov  x5, #1",                // READY 自宣
    "    str  x5, [x20, #0x98]",
    "    movz x8, #0x140",            // N=320（三面钉）
    "    mov  x7, xzr",
    "6:  movz x9, #8",                // recv(endB, 0xB0, &n=8, 0, &nh=0)
    "    str  x9, [x20, #0xC0]",
    "    str  xzr, [x20, #0xC8]",
    "    movz x0, #0x53",
    "    mov  x1, x21",
    "    add  x2, x20, #0xB0",
    "    add  x3, x20, #0xC0",
    "    mov  x4, xzr",
    "    add  x5, x20, #0xC8",
    "    svc  #0",
    "    cbz  x0, 7f",
    "    cmn  x0, #11",               // -EAGAIN 自旋（等 A 下发）
    "    b.eq 6b",
    "    b    9f",
    "7:  ldr  x9, [x20, #0xB0]",      // 序号必须 == i（B 面对账——丢/乱/重即红）
    "    cmp  x9, x7",
    "    b.ne 9f",
    "    str  x7, [x20, #0xB8]",      // 回声载荷 = i
    "8:  movz x0, #0x52",             // channel_send(endB, 0xB8, 8, 0, 0)
    "    mov  x1, x21",
    "    add  x2, x20, #0xB8",
    "    movz x3, #8",
    "    mov  x4, xzr",
    "    mov  x5, xzr",
    "    svc  #0",
    "    cbz  x0, 10f",               // 成功 → 推进轮
    "    cmn  x0, #16",               // -EBUSY 防御重试
    "    b.eq 8b",
    "    b    9f",
    "10: add  x7, x7, #1",
    "    cmp  x7, x8",
    "    b.lo 6b",
    "    str  x8, [x20, #0x90]",      // DONE_B = N
    "    movz x0, #0x36",
    "    svc  #0",
    "    b    .",
    "9:  ldr  x6, [x20, #0x80]",      // err++ → 早退（master errs>0 早断红）
    "    add  x6, x6, #1",
    "    str  x6, [x20, #0x80]",
    "    movz x0, #0x36",
    "    svc  #0",
    "    b    .",
    ".balign 4",
    ".globl __user_pp_end",
    "__user_pp_end:",

    // S-5d soakload 永续 churn 子模板（storm fleet 形态——soak-smp 双核
    // 满载载荷，D2 裁决：fw_cfg opt/berry/soakload 门控；不越基准文档
    // 用户态服务 churn 边界、不新增 user bins、载荷零打印〔panic SMP
    // 安全面〕、无 go-flag/DONE 面〔常驻不退〕）。
    // 每环三段：①churn vmo_create(0x11)→commit(0x12)→write(0x15, 8B)
    // →close(0x01)——**返回值全忽略**（载荷非断言面：svc 失败 = 槽旧值
    // 空转无害；恒败/gen 顶格由内核 churn 计数观测——vmo_create 成功面
    // 计数入心跳 v2.1 churn= 字段，冻结即宿主断言红〔S-5e M1 端到端防
    // 假绿〕）；②clock_read(0x61)×64 内核面风暴（无 handle、无预算帽）；
    // ③CNTVCT_EL0 自旋 1s 摊频——**gen 预算纪律**（草案 §1.5/§5.3）：
    // handle 表为可增长 Vec（硬帽 65536 槽 × 14 位 gen；churn 复用集中
    // 最低空槽）——S-5e m3 勘误旧「子表 8 槽/24x 裕量」口径：30min 三
    // 线程 ≈5.4k create 对复用集中帽实为 ≈3x 裕量；gen 顶格只推表增长
    //（帽前不缩水面），真到硬帽 = create 持久失败 → churn 冻红 loud 非
    // 静默缩水；1s 摊频 → ≈1 churn/s/线程。观测面 = 宿主 soak-smp.sh
    // 心跳 v2.1：sw1 严格递增 = 写者互抢轮转、park1 停增 = 满载谓词
    //（三段全满载形态）、churn 递增 = ①段业务进度。寄存器
    // 合同：x20 = 窗口基（handle 槽 win+0x40 / write 源 win+0xC0 / clock
    // 出参 win+0x0）；x0 = NR→ret（svc dispatch 只回写 x0）；write 借走
    // x4 → 后续 svc 前 win+0x40 重载（allocst 同款）。root（x0=0
    // bootstrap）与写者（x0=writer_arg）跑同一环——arg 不分流。每轮一
    // 瞬态 VMO 对消 → 水位有界（宿主 ±64 带宽容纳三份瞬态）。
    ".globl __user_soakload",
    "__user_soakload:",
    "    mov  x21, sp",               // SP 自推导（allocst 同式）
    "    lsr  x21, x21, #12",
    "    lsl  x20, x21, #12",         // x20 = 自身窗口页基
    "1:  movz x0, #0x11",             // vmo_create(0x1000, 0, win+0x40)
    "    movz x1, #0x1000",
    "    movz x2, #0",
    "    add  x3, x20, #0x40",
    "    svc  #0",
    "    ldr  x4, [x20, #0x40]",      // churn handle（失败轮 = 槽旧值，后续空转无害）
    "    movz x0, #0x12",             // vmo_commit(h, 0, 0x1000)
    "    mov  x1, x4",
    "    movz x2, #0",
    "    movz x3, #0x1000",
    "    svc  #0",
    "    movz x0, #0x15",             // vmo_write(h, win+0xC0, 0, 8)
    "    ldr  x1, [x20, #0x40]",      // 重载（x4 不跨 svc 假设）
    "    add  x2, x20, #0xC0",
    "    movz x3, #0",
    "    movz x4, #8",
    "    svc  #0",
    "    movz x0, #0x01",             // handle_close(h)
    "    ldr  x1, [x20, #0x40]",
    "    movz x2, #0",
    "    movz x3, #0",
    "    svc  #0",
    "    movz x22, #64",              // clock_read ×64（无 handle 内核面风暴）
    "2:  movz x0, #0x61",             // NR_CLOCK_READ（tlbc 风暴同款）
    "    movz x1, #0",                // CLOCK_MONOTONIC
    "    mov  x2, x20",               // 出参 = win+0x0（8B，本模板唯一写点族）
    "    movz x3, #0",
    "    svc  #0",
    "    subs x22, x22, #1",
    "    b.ne 2b",
    "    mrs  x9, CNTVCT_EL0",        // 自旋摊频到点（1s vct——gen 预算纪律，头注）
    "    movz x10, #0xACA0",
    "    movk x10, #0x3B9, lsl #16",  // 62.5M 周期 = 1s @62.5MHz（EL0VCTEN 双核已开）
    "    add  x10, x9, x10",
    "3:  mrs  x9, CNTVCT_EL0",
    "    cmp  x9, x10",
    "    b.lt 3b",
    "    b    1b",                    // 永续（常驻载荷——无退出路径）
    ".globl __user_soakload_end",
    "__user_soakload_end:",

    // G3-4b bench-lat SGI 路 sleeper 子模板（arg 分流：root x0=0 = fleet 占位
    // 即刻退；写者 x0=1 = sleeper，thread_start spread→CPU1）。G3-5 候选①
    // C 形常驻化（2026-11-05）：永续「clock_read(0x61) 锚 → wait_one(0x04,
    // Event, 1<<49, now+480s ns) 阻塞跨核睡 → 被 CPU0 内核 waker（Event::
    // signal 置位 + wake_matching 跨核唤醒——sys_object_signal 尾同链）→
    // 醒来 object_signal(0x06, clear=1<<49) 消费位再睡」——无 N 计数/无
    // 退出路径（批界由 waker 周期信号面定义；boot 周期照跑）。480s = 首
    // 拍网格远界（soakmon 首触发 t0+5×60s=300s，LAT_TRIG_FAR_S 同值口径）
    // ——超时面真兜底 = 内核 latsgi_ret 尾钩 L0 panic 先响（f4 F7 改口：
    // EL0 err 早退对超时不可达——svc 返回前内核已 panic；现网两 harness
    // 均先杀 QEMU，此路不触发）。测量
    // 读点在内核钩子（syscall 层 wait 醒来侧——main.rs latsgi_* 面）；载荷
    // 只管握手循环 + err 窗口汇报（PP 同形——err 窗兜非超时错路：
    // event_create/clock_read/object_signal 失败类）。
    // 窗口槽：0x0 clock 出参 / 0x40 event 句柄 / 0x48 observed / 0x80 err
    //（0x90 DONE 槽随一次性形退役——内核收柜走 LATSGI_DONE 静态）。
    // 寄存器合同：x20 窗口基、x21 句柄（svc 只回写 x0——其余跨 svc 常驻）。
    ".globl __user_latslp",
    "__user_latslp:",
    "    cmp  x0, #1",                // arg 分流：1 = sleeper，0 = root 占位
    "    b.eq 2f",
    "    movz x0, #0x36",             // root：仅 bootstrap 占位，即刻退（0x36
    "    svc  #0",                    // 仅退自身——sleeper 不受牵连，PP 同注）
    "    b    .",
    "2:  mov  x21, sp",               // SP 自推导（PP/soakload 同式）
    "    lsr  x21, x21, #12",
    "    lsl  x20, x21, #12",         // x20 = 自身窗口页基
    "    movz x0, #0x54",             // event_create(0, win+0x40)——sleeper
    "    movz x1, #0",                // 自建 Event（进程私有表；内核钩子在
    "    add  x2, x20, #0x40",        // wait 进入点钉 ObjRef 供 waker 跨表
    "    svc  #0",                    // 唤醒——v1 无跨表 handle 传递的替代桥）
    "    cbnz x0, 9f",
    "    ldr  x21, [x20, #0x40]",     // x21 = Event 句柄
    "3:  movz x0, #0x61",             // now = clock_read(MONOTONIC, win+0x0)
    "    movz x1, #0",                // §A118：deadline = ns 绝对时刻，锚
    "    mov  x2, x20",               // 必经 clock_read——勿喂 CNTVCT 周期
    "    svc  #0",
    "    cbnz x0, 9f",
    "    ldr  x3, [x20, #0x0]",
    "    movz x6, #0xC000",           // δ = 480s ns（0x6F_C23A_C000 = 首拍
    "    movk x6, #0xC23A, lsl #16",  // 网格 300s + 裕量——首触发在 soakmon
    "    movk x6, #0x6F, lsl #32",    // t0+5×60s 才来，180s 会先到期双 PANIC
    "    add  x3, x3, x6",            // deadline = now + 480s
    "    movz x0, #0x04",             // wait_one(h, 1<<49, deadline, win+0x48)
    "    mov  x1, x21",               //   ——阻塞跨核睡，醒因 = waker 置位
    "    movz x2, #2, lsl #48",       // LAT_SGI_BIT = 1<<49（位签名门——内核
    "    add  x4, x20, #0x48",        // 钩子双过滤之一；USER_SIGNAL_BITS 在册）
    "    svc  #0",
    "    cbnz x0, 9f",                // 超时/错路兜底（内核尾钩 L0 先响）
    "    movz x0, #0x06",             // object_signal(h, clear=1<<49, set=0)
    "    mov  x1, x21",               //   ——消费锁存位（§A61 清位纪律），下轮
    "    movz x2, #2, lsl #48",       // wait 前 Event 态归零 → 恒真睡不 poll
    "    movz x3, #0",
    "    svc  #0",
    "    cbnz x0, 9f",
    "    b    3b",                    // C 形永续（批界 = waker 周期信号面——
                                      // 无 N 计数/无退出，boot 周期照跑）
    "9:  ldr  x6, [x20, #0x80]",      // err++ → 早退（非超时错路兜底——超时
    "    add  x6, x6, #1",            // 走内核 latsgi_ret 钩 L0 panic 先响
                                      // 〔f4 F7/F14 改口〕；断流判口径 = 连
                                      // 漏 8 拍〔480s/60s——旧「3 拍」残注正〕）
    "    str  x6, [x20, #0x80]",
    "    movz x0, #0x36",
    "    svc  #0",
    "    b    .",
    ".balign 4",
    ".globl __user_latslp_end",
    "__user_latslp_end:",

    // 4-5b/4-6 svcmgr 模板已随 G-eco-b 退役换编译产物（user/bins/svcmgr，
    // include_bytes! 内嵌 echo/talker 平像——原模板内嵌的 echo 段（268B）与
    // talker 段（936B）随之换轨 user/bins/{echo,talker}）。契约逐字保留
    // （SVC ok/DEMO ok = smoke 第 21/23 标记），见 user_svcmgr_code()。
);

extern "C" {
    static __user_good: u8;
    static __user_good_end: u8;
    static __user_rogue_ptr: u8;
    static __user_rogue_ptr_end: u8;
    static __user_spawner: u8;
    static __user_spawner_end: u8;
    static __user_waiter: u8;
    static __user_waiter_end: u8;
    static __user_pinger: u8;
    static __user_pinger_end: u8;
    static __user_ponger: u8;
    static __user_ponger_end: u8;
    static __user_lb: u8;
    static __user_lb_end: u8;
    static __user_tlbc: u8;
    static __user_tlbc_end: u8;
    static __user_allocst: u8;
    static __user_allocst_end: u8;
    static __user_htst: u8;
    static __user_htst_end: u8;
    static __user_pp: u8;
    static __user_pp_end: u8;
    static __user_soakload: u8;
    static __user_soakload_end: u8;
    static __user_latslp: u8;
    static __user_latslp_end: u8;
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
///
/// **G-eco-a 起为编译产物**：`user/bins/abuser`（no_std + berry-sys）经
/// `just build-user` 出 flat binary——契约与原 asm 模板逐字一致（节流 →
/// close 0xDEAD_BEEF 必须 -EBADF → 'S' 经 console send/-EBUSY 重试 →
/// 异常自陷）。缺产物 = `include_bytes!` 编译期炸响（D6：loud not silent）。
pub fn user_abuser_code() -> &'static [u8] {
    include_bytes!("userbins/abuser.bin")
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

/// LB 金丝雀子模板（S-3b b-2：纯 CNTVCT 自旋 + svc 0x36 thread_exit 汇报
/// ——见模板内注释）。lb_master 经真 svc 链（0x31/0x11/0x12/0x15/0x16/
/// 0x34/0x32/0x35）单进程造 5 份本镜像（root + 4 计算线程），vmar_map 进
/// 程空间后 process_start/thread_start。
pub fn user_lb_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_lb, &__user_lb_end) }
}

/// TLB shootdown canary 子模板（S-4 4-4：warm → probe 汇报 → SVC 风暴
/// 轮询 → canary 写点 → thread_exit——见模板内注释）。task_tlbs 经真 svc
/// 链单进程造 5 份镜像（root 家核 + 4 写者 spread→CPU1）。
pub fn user_tlbc_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_tlbc, &__user_tlbc_end) }
}

/// ALLOC churn storm 子模板（S-5b 相 6：SP 自推导窗口 → go-flag → 64 轮
/// vmo create/commit/write/close → 0x36 退出——见模板内注释）。task_alloc
/// 经真 svc 链单进程造 5 份镜像（root 家核 + 4 写者 spread→CPU1）。
pub fn user_allocst_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_allocst, &__user_allocst_end) }
}

/// HT 风暴 storm 子模板（S-5b 相 7：同 fleet 形态，载荷换 event 风暴
/// create/signal/close——被打面 = 子进程 handle 表跨核并发）。
pub fn user_htst_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_htst, &__user_htst_end) }
}

/// PP ping-pong 双角色子模板（S-5c 相 8：arg 分流 A/B——A channel_create
/// 中继端点 + N 轮 send/recv 逐轮验序，B recv 验序 + echo send——见模板
/// 内注释）。task_pp 经真 svc 链单进程造 2 份镜像（root A 家核 + B 写者
/// spread→CPU1——storm_spawn_fleet writer_n=1 形）。
pub fn user_pp_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_pp, &__user_pp_end) }
}

/// soakload 永续载荷子模板（S-5d：每环 = vmo churn + clock_read×64 内核
/// 面风暴 + 1s CNTVCT 自旋摊频〔gen 预算纪律〕，返回值全忽略、无退出
/// 路径——见模板内注释）。task_soakload 经真 svc 链单进程造
/// `1+SOAKLOAD_WRITER_N` 份镜像（root 家核 + 写者 spread→CPU1）。
pub fn user_soakload_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_soakload, &__user_soakload_end) }
}

/// bench-lat SGI 路 sleeper 子模板（G3-4b：root x0=0 占位即刻退 + 写者
/// x0=1 sleeper spread→CPU1——`wait_one(1<<49)` 跨核阻塞对，waker = 内核
/// 任务 `task_latsgi_wak`（Event 置位 + wake_matching 跨核唤醒 + 醒来侧
/// 内核钩子取戳采样）。arg 分流/window 槽/N=32 三面钉见模板头注。
pub fn user_latslp_code() -> &'static [u8] {
    // SAFETY: 同上。
    unsafe { blob(&__user_latslp, &__user_latslp_end) }
}

/// svcmgr 用户程序模板（4-5b 验收：服务管理器拉起 echo 服务——真 syscall
/// 全链创建用户态进程 + 通道，ECHO_REQ 往返验证打 `SVC ok`；4-6 A 半追加
/// talker/echo2 双服务互联 → `DEMO ok`）。
///
/// **G-eco-b 起为编译产物**：`user/bins/svcmgr`（no_std + berry-sys，
/// `include_bytes!` 内嵌 echo/talker 平像——原 asm 模板内嵌的 echo 段与
/// talker 段随之换轨 `user/bins/{echo,talker}`）。契约与原 asm 逐字
/// 一致；缺产物 = 编译期炸响（D6：loud not silent）。首个多页用户镜像
/// （D3 放宽，data/stack 上移，sp 见 `UserProcess::stack_top`）。
pub fn user_svcmgr_code() -> &'static [u8] {
    include_bytes!("userbins/svcmgr.bin")
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
    /// sp0（16 对齐；多页镜像时 = 动态栈顶，见 `build`——单页程序恒
    /// `USER_STACK_TOP`，`spawn_user` 调用方从这取、别再读常量）。
    pub stack_top: usize,
    /// 地址空间根（good 的被 boot 取走进引导表；其余泄漏持有）。
    pub vmar: Option<Vmar>,
    /// code/data/stack 三 Vmo（Region 指针有效性依托；见结构体注释）。
    _vmos: [Vmo; 3],
}

impl UserProcess {
    /// 建一个用户进程：root Vmar + code（页取整，单程序常 1 页）+ data/stack
    /// 各一页（全 `UserRw`）+ 拷代码 + 写数据页魔数。栈页全零（sp0 自顶向下）。
    ///
    /// **D3（G-eco-b）多页代码**：code 区按页取整后，data/stack 顺次上移
    /// （`0x1000 + code_len` / 再上一页）——单页程序布局与 2b-4 起逐位相同
    /// （data@0x2000、stack@[0x3000,0x4000)），asm 模板的硬编槽位零扰动；
    /// 多页（svcmgr 编译镜像，内嵌双子程序平像）由 `stack_top` 交给调用方。
    /// 帽 128KiB（`USER_CODE_MAX`，A7-4 段② 64→128KiB 同步抬）：防调用方手滑
    /// 传整段 ELF——flat 才是约定输入（elf2flat.py `--max` 同值执法）。
    pub fn build(fa: &mut FrameAllocator, code: &[u8], data_magic: u64) -> Self {
        assert!(code.len() <= USER_CODE_MAX, "用户代码镜像超 128KiB 帽（flat 才是约定输入）");
        assert!(code.len() % 4 == 0, "用户代码模板须 4 字节对齐长度");
        let code_pages = (code.len() + PAGE_SIZE - 1) / PAGE_SIZE;
        let code_len = code_pages * PAGE_SIZE;
        let data_va = USER_CODE_VA + code_len;
        let stack_base = data_va + PAGE_SIZE;
        let asid = alloc_asid();
        let mut vmar = Vmar::new_root(fa, asid).expect("user: 建 root Vmar（boot 可信语境）");

        let mut code_vmo = Vmo::create(code_len).expect("user: 装 code VMO（boot 可信语境）");
        code_vmo.commit(fa, 0, code_len).expect("user: commit code");
        code_vmo.write(0, code).expect("user: 拷代码");

        let mut data_vmo = Vmo::create(PAGE_SIZE).expect("user: 装 data VMO（boot 可信语境）");
        data_vmo.commit(fa, 0, PAGE_SIZE).expect("user: commit data");
        data_vmo.write(0, &data_magic.to_le_bytes()).expect("user: 数据页魔数");

        let mut stack_vmo = Vmo::create(PAGE_SIZE).expect("user: 装 stack VMO（boot 可信语境）");
        stack_vmo.commit(fa, 0, PAGE_SIZE).expect("user: commit stack");

        vmar.map(fa, &mut code_vmo, USER_CODE_VA, 0, code_len, Perm::UserRw)
            .expect("user: map code");
        vmar.map(fa, &mut data_vmo, data_va, 0, PAGE_SIZE, Perm::UserRw)
            .expect("user: map data");
        vmar.map(fa, &mut stack_vmo, stack_base, 0, PAGE_SIZE, Perm::UserRw)
            .expect("user: map stack");

        // 表树对页表走查可见（map 内 dsb ish）；三 Vmo 进结构体（见结构体注释）。
        Self {
            ttbr0_pa: vmar.ttbr0_pa(),
            asid,
            stack_top: stack_base + PAGE_SIZE,
            vmar: Some(vmar),
            _vmos: [code_vmo, data_vmo, stack_vmo],
        }
    }
}

/// R-0 私助：分位**桶界保守上界**——**单源 = berry-telface::
/// hist_percentile（W3-Q10 迁出）**：本壳透传 WakeHist 拆参（num/den
/// 有理数形避免浮点；最近秩 ceil(num/den · n) 的 log2 桶保守上界——
/// 桶 i 覆盖 [2^i, 2^(i+1)) 周期数，顶桶 u64::MAX 防溢出）。指数粒度只够
/// 看形状，精确分位归 R-1 采样集全排序。扫描必达：wake_hist 读序纪律保证
/// sum(桶) ≥ n ≥ 目标。host 钉板在 telface（最近秩舍入/顶桶饱和/分位
/// 单调/全排序对照）。
fn benchwake_percentile(h: &crate::telemetry::WakeHist, num: u64, den: u64) -> u64 {
    berry_telface::hist_percentile(&h.buckets, h.n, num, den)
}

/// 刀1 G1.5 私助：KERNEL_BOOT 冻结 round-over-round 检查。`*prev ==
/// u64::MAX` = 首轮只采样建基；此后每轮等值——调度器启动后 boot 归因再涨
/// = `note_switch` 基线写点漏路由（cell 残留 KERNEL_BOOT 语境），L0 红线。
/// 消费两点：watchdog 环尾（稳态窗）+ G1.5 退出块（终检，同轮内先于
/// exit_current）。
fn wake_kb_freeze_round(prev: &mut u64) {
    let kb =
        crate::telemetry::wake_src_wakes(berry_hal_aarch64::wake_gen::WAKE_SRC_KERNEL_BOOT);
    if *prev != u64::MAX {
        assert!(
            kb == *prev,
            "G1.5 FAIL: KERNEL_BOOT 唤醒在调度器启动后再涨（{} → {}）——cell 基线写点漏路由",
            *prev,
            kb
        );
    }
    *prev = kb;
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
    // 刀1：KERNEL_BOOT 冻结基线（u64::MAX = 首轮只采样不比对——见
    // [`wake_kb_freeze_round`]；稳态窗 = watchdog 环尾 round-over-round，
    // 终检 = G1.5 退出块内一锤）。
    let mut kb_wake_prev: u64 = u64::MAX;
    for _ in 0..TRIES {
        // 忙等一拍（无 sleep 原语——Phase 4 通知落地前的土法；轮到别的任务跑）。
        // **留在锁窗外**（F37）：IRQ 解蔽 tick 才轮转、金丝雀才能涨——进了窗
        // 全系统只剩本任务串行，等金丝雀 = 自锁（草案 §2.2 短窗纪律）。
        for _ in 0..200_000 {
            core::hint::spin_loop();
        }
        // F37 读侧收口（草案 §2.2）：kills/puts/switches 三读 + 错杀验证 +
        // 遥测双环断言 + wake_hist 快照收进单一短窗（B16 锁窗等价口径）。
        // 成功路经 exit_current 不归——放锁由 sched switch_away_forever 舞蹈
        // 收口（Task 位=1，`-> !` svc_exit 同形），尾 klock_exit 对其不可达
        // （死码勿补）；循环续行路对称 klock_exit；panic 路走锁 bypass
        //（panic handler 直写 UART + halt，无锁递归——kern_lock.rs 模块头）。
        berry_hal_aarch64::kern_lock::klock_enter();
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
                // G3-4b trace 记录面关适配：环/直方图断言对象不在场（钩子
                // 整体让位）还执法即错杀——断言块与 BENCHWAKE 打印随记录面
                // 让位，替代标记 `TRACE off ok`（off 通道 done 锚，bench.py
                // --trace-dual 收割面）。金丝雀/杀数断言不动（不属记录面）。
                let t_off = crate::telemetry::trace_off();
                if !t_off {
                // 3-4c 遥测环存活断言：环计任务↔任务切换 ≥ switches_tt（S-6 ①
                // 双计数器——环只记任务↔任务，分母同口径；SWITCHES 自 A12/S-6
                // 起 11 位全计含 park 语境，不可再作分母），且环内 vct 从旧到新
                // 单调（乱序 = 写路径并发/覆盖 bug）。
                let tel = crate::telemetry::total();
                let switches_tt = berry_sched::switches_tt();
                assert!(tel >= switches_tt, "G1.5 FAIL: 遥测环未跟上（total={} < switches_tt={}）", tel, switches_tt);
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
                }
                // S-4 4-2：双核窗标记行 PRINT_LOCK 收编（bringup 后裸打撕行面，
                // 同 main.rs K BLOCK 判例）。
                berry_hal_aarch64::kern_lock::with_print_lock(|| {
                    berry_hal_aarch64::uart_puts(b"\nG1.5 ok (rogue dead, abuser alive, canaries alive)\n");
                });
                // R-0（基准与压力测试设计.md §9）：唤醒延迟直方图 boot 尾一行。
                // 落点契约：watchdog 单次执行 + 下行 exit_current 切走不归 →
                // **恰一次**；写在 exit 之后 = 死码（Plan B v0.5 gotcha 同族）。
                // 形状严格断言（L0 执法）：n>0 且 min<=max（两条皆撕裂安全——
                // 见 telemetry::wake_hist 读序纪律）；分位数值是统计量只出数
                // 不设门禁（公约 2——数值 L1 归 bench.py 收割同表）。单行
                // 纯 ASCII、uart 直写（内核任务非 console 通道——无分片撕
                // 行面，d-3c 192B 雷不适用，超长亦无失败模式）。长度算术
                // 绝对界 208B（固定段 48B = 头 12 + 7 个标签各 5 + 换行 1，
                // + 8 数值字段各至多 20 位）——触发需某延迟 ≥2^63 周期
                // （62.5MHz 下 ≈293 年），不可达；实践界远低于此（boot 实
                // 测 81B、一年 soak 上界约 152B）。
                if !t_off {
                let h = crate::telemetry::wake_hist();
                assert!(
                    h.n > 0,
                    "BENCHWAKE FAIL: n=0（直方图零样本——唤醒环计数={} 却桶空 = 写点两路失联）",
                    crate::telemetry::wake_total()
                );
                assert!(h.min <= h.max, "BENCHWAKE FAIL: min={} > max={} = 直方图写点 bug", h.min, h.max);
                let p50 = benchwake_percentile(&h, 1, 2);
                let p90 = benchwake_percentile(&h, 9, 10);
                let p99 = benchwake_percentile(&h, 99, 100);
                // BENCHWAKE 十二段拼行收口（G43/G44）：整行在 PRINT_LOCK 内
                // 一次拼完（原子）——嵌套 KERN→PRINT 单向合法（soakmon/
                // BENCHRT 同形；窗内 IRQ 已屏蔽，with_print_lock 自适应不复
                // 原解蔽）。
                berry_hal_aarch64::kern_lock::with_print_lock(|| {
                    berry_hal_aarch64::uart_puts(b"BENCHWAKE n=");
                    berry_hal_aarch64::uart_put_dec(h.n);
                    berry_hal_aarch64::uart_puts(b" min=");
                    berry_hal_aarch64::uart_put_dec(h.min);
                    berry_hal_aarch64::uart_puts(b" p50=");
                    berry_hal_aarch64::uart_put_dec(p50);
                    berry_hal_aarch64::uart_puts(b" p90=");
                    berry_hal_aarch64::uart_put_dec(p90);
                    berry_hal_aarch64::uart_puts(b" p99=");
                    berry_hal_aarch64::uart_put_dec(p99);
                    berry_hal_aarch64::uart_puts(b" max=");
                    berry_hal_aarch64::uart_put_dec(h.max);
                    berry_hal_aarch64::uart_puts(b" sig=");
                    berry_hal_aarch64::uart_put_dec(h.sig);
                    berry_hal_aarch64::uart_puts(b" tmo=");
                    berry_hal_aarch64::uart_put_dec(h.tmo);
                    berry_hal_aarch64::uart_puts(b"\n");
                });
                } else {
                    // off 臂替代标记（与 BENCHWAKE 同位：G1.5 ok 之后、
                    // exit_current 之前——恰一次；含空格 = 矩阵字符流不可
                    // 伪命中，全形状正则收割面同 BENCHWAKE 纪律）。
                    // 真空执法〔f4 F14〕：total() 只在环写路径递增，off 面
                    // 钩子整体让位 ⇒ 必恒 0——锚打印前断言，替代锚自身
                    // 携带「记录面真关」证据（对半开泄漏：TRACE_OFF 置位
                    // 过迟/新写路径未让位，在此 loud 红而非 TRACE_CAP 盲区
                    // 照绿）。
                    assert!(crate::telemetry::total() == 0,
                            "watchdog: trace off 而 telemetry total={} ≠ 0（记录面泄漏——门置位过迟或新写路径未让位）",
                            crate::telemetry::total());
                    berry_hal_aarch64::kern_lock::with_print_lock(|| {
                        berry_hal_aarch64::uart_puts(b"TRACE off ok (record face closed)\n");
                    });
                }
                // 刀1 G1.5 WAKESRC 块（**无条件**——不随 TRACE off 让位：
                // per-source 记账面独立于 WakeRing 记录面，trace_off 早退
                // 只关环记录，不关记账）。位置契约同 BENCHWAKE：watchdog
                // 尾单次执行 + 下行 exit_current 切走不归 → 恰一次（R-0
                // 落点）。先并 HAL 侧到达计数（SGI×2/未知臂无内核回调槽，
                // store 单调值幂等）。
                crate::telemetry::wake_src_merge_hal();
                {
                    use berry_hal_aarch64::wake_gen::{
                        WAKE_SRC_IPC, WAKE_SRC_KERNEL_BOOT, WAKE_SRC_NONE, WAKE_SRC_POWER_KEY,
                        WAKE_SRC_SGI_RESCHED, WAKE_SRC_SGI_TLB, WAKE_SRC_TIMER, WAKE_SRC_UART_RX,
                        WAKE_SRC_UNKNOWN,
                    };
                    use crate::telemetry as tel;
                    // ① 守恒：双读稳定窗内 Σ槽 wakes == 独立总账。写序
                    //    槽先、N 末位 Release；并发瞬态 Σ≥N，固定差值帽会
                    //    被一次多任务广播假红，故不读一次即断等。
                    assert!(tel::wake_acct_conserved(),
                        "WAKESRC FAIL: 稳定窗槽和≠总账（wake 写点两路失联）");
                    let acct = tel::wake_acct_n();
                    // ② 无主零容忍终检：NONE 哨兵/UNKNOWN 只许 IRQ 级到达
                    //    计数，wakes 恒 0（on_wake 写点 panic 在前，此处
                    //    收口防线）。
                    assert!(tel::wake_src_wakes(WAKE_SRC_NONE) == 0,
                        "WAKESRC FAIL: NONE 槽 wakes ≠ 0");
                    assert!(tel::wake_src_wakes(WAKE_SRC_UNKNOWN) == 0,
                        "WAKESRC FAIL: UNKNOWN 槽 wakes ≠ 0");
                    // sim-S2 A4②：POWER_KEY 到达-only 同款终检（写点 on_wake
                    // 已 panic 在前，此处收口防线——wakes 恒 0 = 恒无持有者）。
                    assert!(tel::wake_src_wakes(WAKE_SRC_POWER_KEY) == 0,
                        "WAKESRC FAIL: POWER_KEY 槽 wakes ≠ 0（无主 IRQ 出现唤醒路径——A4② 契约违约）");
                    // ③ 环境活水兜底：TIMER（expire_timeouts ← on_irq_tick
                    //    全程在跑）与 IPC（任务语境基线）必有唤醒——零值
                    //    = 记账链断而非「静默全绿」。
                    assert!(tel::wake_src_wakes(WAKE_SRC_TIMER) >= 1,
                        "WAKESRC FAIL: TIMER wakes==0（expire_timeouts 归因链断）");
                    assert!(tel::wake_src_wakes(WAKE_SRC_IPC) >= 1,
                        "WAKESRC FAIL: IPC wakes==0（任务语境基线断链）");
                    // ④ KERNEL_BOOT 冻结终检（环尾已 round-over-round 跑
                    //    多轮，此处同轮收口——u64::MAX 首轮早退不触发）。
                    wake_kb_freeze_round(&mut kb_wake_prev);
                    // ⑤ ts_domain==0（v1 恒虚拟单调域；活跃槽查——未触槽
                    //    构造即 0，断言无执法意义）。
                    let t = tel::wake_src_snapshot(WAKE_SRC_TIMER);
                    let u = tel::wake_src_snapshot(WAKE_SRC_UART_RX);
                    let r = tel::wake_src_snapshot(WAKE_SRC_SGI_RESCHED);
                    let l = tel::wake_src_snapshot(WAKE_SRC_SGI_TLB);
                    let i = tel::wake_src_snapshot(WAKE_SRC_IPC);
                    let k = tel::wake_src_snapshot(WAKE_SRC_KERNEL_BOOT);
                    let x = tel::wake_src_snapshot(WAKE_SRC_UNKNOWN);
                    let pw = tel::wake_src_snapshot(WAKE_SRC_POWER_KEY);
                    for s in [t, u, r, l, i, k, x, pw] {
                        if s.wakes > 0 || s.irq > 0 || s.no_waiter > 0 {
                            assert!(s.ts_domain == 0,
                                "WAKESRC FAIL: src={} ts_domain={} ≠ 0", s.src, s.ts_domain);
                        }
                    }
                    // 单行标记（smoke 收割面）。行宽上界：acct/计数各 ≤20
                    // 位 ×16 ≈ 340B 绝对最坏，实际量级 ~80B——串口缓冲窗
                    // 同 BENCHWAKE 纪律（with_print_lock 内逐段 puts，IRQ
                    // 已屏蔽）。U 的 nw = UART 无等待者计数：**良性竞态值，
                    // 只打印不断言**（控制台醒-复睡窗内 n==0 不丢唤醒，
                    // 设计文档 §no_waiter）。
                    berry_hal_aarch64::kern_lock::with_print_lock(|| {
                        berry_hal_aarch64::uart_puts(b"WAKESRC acct=");
                        berry_hal_aarch64::uart_put_dec(acct);
                        berry_hal_aarch64::uart_puts(b" T=");
                        berry_hal_aarch64::uart_put_dec(t.wakes);
                        berry_hal_aarch64::uart_putc(b'/');
                        berry_hal_aarch64::uart_put_dec(t.irq);
                        berry_hal_aarch64::uart_puts(b" U=");
                        berry_hal_aarch64::uart_put_dec(u.wakes);
                        berry_hal_aarch64::uart_putc(b'/');
                        berry_hal_aarch64::uart_put_dec(u.irq);
                        berry_hal_aarch64::uart_putc(b'/');
                        berry_hal_aarch64::uart_put_dec(u.no_waiter);
                        berry_hal_aarch64::uart_puts(b" R=");
                        berry_hal_aarch64::uart_put_dec(r.wakes);
                        berry_hal_aarch64::uart_putc(b'/');
                        berry_hal_aarch64::uart_put_dec(r.irq);
                        berry_hal_aarch64::uart_puts(b" L=");
                        berry_hal_aarch64::uart_put_dec(l.wakes);
                        berry_hal_aarch64::uart_putc(b'/');
                        berry_hal_aarch64::uart_put_dec(l.irq);
                        berry_hal_aarch64::uart_puts(b" I=");
                        berry_hal_aarch64::uart_put_dec(i.wakes);
                        berry_hal_aarch64::uart_puts(b" K=");
                        berry_hal_aarch64::uart_put_dec(k.wakes);
                        // sim-S2 A4②：POWER_KEY 三段面（w/i/nw）。**boot 期快照
                        // 面**——本行打印点在 watchdog 稳态窗收口（早于 smoke RX
                        // 相的 QMP system_powerdown 注入数分钟），故 i/nw 在本行
                        // 恒 0/0：其语义 = 「boot 期恒无持有者」的证据（非注入期
                        // 数值面）。注入期数值面由 `PWRKEY unowned n=` 标记行承载
                        // （读 telemetry no_waiter 台账 + smoke n∈[1,6] 值域门）；
                        // 注入后读点挂 0x84 telemetry_open 终身面落地刀再补。
                        berry_hal_aarch64::uart_puts(b" P=");
                        berry_hal_aarch64::uart_put_dec(pw.wakes);
                        berry_hal_aarch64::uart_putc(b'/');
                        berry_hal_aarch64::uart_put_dec(pw.irq);
                        berry_hal_aarch64::uart_putc(b'/');
                        berry_hal_aarch64::uart_put_dec(pw.no_waiter);
                        berry_hal_aarch64::uart_puts(b" X=");
                        berry_hal_aarch64::uart_put_dec(x.wakes);
                        berry_hal_aarch64::uart_putc(b'/');
                        berry_hal_aarch64::uart_put_dec(x.irq);
                        berry_hal_aarch64::uart_puts(b"\n");
                    });
                }
                berry_sched::exit_current()
            }
        }
        last_puts = puts;
        last_switches = switches;
        // 刀1 稳态窗：KERNEL_BOOT 冻结（round-over-round——watchdog 是任务
        // 语境，此断言 = 调度器启动后 boot 归因零复活）。
        wake_kb_freeze_round(&mut kb_wake_prev);
        // sim-S2 A4①（PM-16 §4.2）：稳态窗逐醒因查持有者——①未知醒因
        // 绝对零（任一未知 IRQ 号到达 = 归因面破洞，boot 起全程不许有，
        // 非仅窗口内增量；trap.rs 未知臂首 4 loud `irq? id=` 是人肉排查面）；
        // ②唤醒环归因合法性（wake_check ⑤：环内每条源 ∈ 可进环白名单——
        // NONE/UNKNOWN/POWER_KEY 到达-only 源进环即红）。trace off 时环
        // 记录面已关（复检无对象），只留 ①；BLOCK_BOOT_DONE 前 boot 双路径
        // 未落环，同 4-0 断言的置位前提。
        assert_eq!(
            berry_hal_aarch64::wake::unknown_arrivals(),
            0,
            "A4① FAIL: 未知醒因到达 ≠ 0（无主 IRQ 漏归因——查串口 irq? id= 行）"
        );
        if crate::BLOCK_BOOT_DONE.load(core::sync::atomic::Ordering::Relaxed)
            && !crate::telemetry::trace_off()
        {
            assert!(
                crate::telemetry::wake_check(),
                "A4① FAIL: 稳态窗唤醒环含无主/非法归因源（wake_check ⑤）"
            );
        }
        berry_hal_aarch64::kern_lock::klock_exit();
    }
    panic!("G1.5 FAIL: 超时（kills={} drained={}）", berry_sched::user_kills(), last_puts);
}
