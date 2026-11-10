//! berry-exec —— rootfs 程序拉起链（A-2 自 svcmgr 抽取，svcmgr/bsh 共享）。
//!
//! 「不长第二套」：`spawn_rootfs_elf`/`channel_pair` 自 svcmgr 迁入（G-eco-c/d
//! 原语，行为逐字等价），签名 [`SpawnSpec`] 化 + **h_rootfs 参数化**——bsh 持
//! env 块 gift[2] 的 rootfs VMO READ dup（libc `rootfs_h()`），svcmgr 用表内
//! 定值 0x0001_0002，同一 VMO 两个取用途径。[`cpio_find`] 供 bsh 的
//! `run <name>` 单点查询（svcmgr 的一次遍历多分派留在其 main——遍历骨架
//! 同源 `berry_loader::cpio_step`，窗各自私有）。
//!
//! 语义纪律（迁自 svcmgr，勿漂移）：
//! - 解析/首窗读失败返 `Err`（调用方可见降级续走）；**syscall 面失败自陷**
//!   （真 syscall 失败 = 系统级异常，标记缺失 loud 兜底）——例外：资源耗尽
//!   （`process_create` -ENOMEM，PROCESS_MAX 出帽）= ABI 合法运行时条件，
//!   Err 降级不杀调用方（P5 v0.5 对抗审查）；
//! - 段幅前置 `ELF_MAP_MAX`（复盘雷 8）：巨段在几何层即降级；
//! - 栈与映像之间隔**保护页**（`stack_va = base + len + PAGE_SIZE`）——栈用
//!   穿 = 干净 D-abort（hexyl 栈溢出实踩，d-3c）；
//! - env 块 gift 位语义（libc `berry_libc_boot` 镜像）：[0] = stdout 端
//!   RIGHT_SEND dup、[1] = stdin 读端、[2..] = extra_gifts（A-2：rootfs VMO
//!   READ dup；P6 A-1：fsmgr 端/fs 客户端端；A2-3：netd 服务端端）。**随行
//!   handle 被 send take**——调用方需保留原值者先 duplicate；
//! - **回收（Plan B v0.5）**：成功返 [`ChildHandles`]，调用方收完输出后
//!   `reap()`（wait TERMINATED →（X-3c：demand 槽注销先于 close）→ unmap×3
//!   〔+零尾域〕 → close×5〔+zvmo〕——P6 A-2 起含 MMIO 窗口）
//!   ——完整回收映像/栈帧与对象 Box；不 reap = 有界泄漏（root Vmar L1/PT
//!   帧与 ASID v1 恒泄漏，见 [`ChildHandles::reap`] 注释）。

#![no_std]

use berry_sys::{
    berry_channel_create, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_handle_duplicate, berry_object_wait_one, berry_process_create,
    berry_process_get_exit_code, berry_process_start, berry_thread_create, berry_vmar_map,
    berry_vmar_unmap, berry_vmo_commit, berry_vmo_create, berry_vmo_get_size, berry_vmo_read,
    berry_vmo_write, Handle, BERRY_ERR_BAD_HANDLE, BERRY_ERR_BUSY, BERRY_ERR_NO_MEMORY,
    CLOCK_MONOTONIC, RIGHT_DUPLICATE, RIGHT_MANAGE, RIGHT_MAP, RIGHT_READ, RIGHT_SEND,
    RIGHT_WRITE, SIGNAL_TERMINATED, SIGNAL_WRITABLE, VMO_OPT_DEMAND,
};
// X-5 窗规划纯面（`plan_wins`/`WinPlan`/`WinKind`）自本 crate 外迁
// berry-winplan（host 可测——原处内嵌 FFI 是不可测根因），此处单源引回。
// WinPlan/WinKind 维持原私有可见性（原 crate 内即私有）；WIN_MAX 原为
// pub const，经 re-export 保住下游可见性零漂移。
use berry_winplan::{WinKind, WinPlan};
pub use berry_winplan::WIN_MAX;

/// ELF 首窗：ehdr + 全部 phdr（真实 lld 产物 phnum ≤ 10 量级，4KiB 绰绰）。
static mut ELF_WIN: [u8; 4096] = [0; 4096];
/// 段字节泵窗：rootfs VMO → 子 VMO 的 4KiB 中转（std 镜像 ~283KB ≈ 70 泵）。
static mut PUMP_WIN: [u8; 4096] = [0; 4096];
/// cpio 步进窗（cpio_find 用）：头 110B + 名（mkcpio 名 ≤ 56B 绰绰）。
static mut STEP_WIN: [u8; 512] = [0; 512];

// —— X-3c/X-5 demand 装载钩子面 ——
//
// 注入式可选路径：svcmgr boot 期（`set_demand_hooks`）把 pager 寄存表
// 钩子注入本 crate；bsh **从不注入** → 其链接副本恒 None → 永远 eager
// = exec 面零漂移隔离 by construction（X-3 spec：bsh 面保持 eager）。
/// 注册钩子（X-5 起逐窗）：一窗 = (dv, sv, delta) 三元组——file 窗
/// sv=rootfs 恒等供给、零窗 sv=0（内核 0x23 零页臂，delta 恒 0）；返
/// false = 槽满 → 调用方回滚已注窗走 eager。
pub type DemandRegFn = fn(dv: Handle, sv: Handle, delta: i64) -> bool;
/// 注销钩子（逐窗）：reap 在 close demand VMO **前**调（防句柄值复用后
/// pager 误供新主——槽表按 dv 值扫描回收）。
pub type DemandUnregFn = fn(dv: Handle);

static mut DEMAND_REG: Option<DemandRegFn> = None;
static mut DEMAND_UNREG: Option<DemandUnregFn> = None;

/// boot 期一次性注入（svcmgr `_start` 首段；单调用方契约，无并发写面）。
pub fn set_demand_hooks(reg: DemandRegFn, unreg: DemandUnregFn) {
    // SAFETY: boot 单线程期写、此后只读（ELF_WIN/PUMP_WIN 同款纪律）。
    unsafe {
        core::ptr::addr_of_mut!(DEMAND_REG).write(Some(reg));
        core::ptr::addr_of_mut!(DEMAND_UNREG).write(Some(unreg));
    }
}

/// 读注册钩子（spawn 路径每调用读一次；None = 未注入 → eager）。
fn demand_reg_hook() -> Option<DemandRegFn> {
    // SAFETY: 只读（boot 后无写者——单调用方契约）。
    unsafe { core::ptr::addr_of!(DEMAND_REG).read() }
}

/// 读注销钩子（spawn 半途回滚 + reap 摘槽两用；None = 未注入 → 跳过）。
fn demand_unreg_hook() -> Option<DemandUnregFn> {
    // SAFETY: 只读（同上）。
    unsafe { core::ptr::addr_of!(DEMAND_UNREG).read() }
}

/// X-4b eager 双生护栏：临时摘 demand 钩子（两 static None 化，Drop 原值
/// 恢复）——**同 bin 二次拉起走 eager 原路径的唯一变量对照**（dpgx_foot
/// eager 胎：congruent ELF 在钩子缺席下恒走 eager 全幅 commit，对照面 =
/// 装载分支本身，非 ELF 形状）。
///
/// SAFETY 契约：boot 期后写 static 仅此一处——**持有窗内不得并发拉起任何
/// 进程**（svcmgr 主循环串行 spawn 契约面；dpgx_foot 帧内无并发 spawn，
/// 满足）。
pub struct EagerGuard {
    reg: Option<DemandRegFn>,
    unreg: Option<DemandUnregFn>,
}

impl EagerGuard {
    pub fn new() -> Self {
        // SAFETY: 单线程 spawn 窗写（见类型注）；Drop 恢复原值。
        unsafe {
            let reg = core::mem::replace(&mut *core::ptr::addr_of_mut!(DEMAND_REG), None);
            let unreg = core::mem::replace(&mut *core::ptr::addr_of_mut!(DEMAND_UNREG), None);
            Self { reg, unreg }
        }
    }
}

impl Drop for EagerGuard {
    fn drop(&mut self) {
        // SAFETY: 同 new 的写窗（guard 存活期 = 临界区，drop 即出界）。
        unsafe {
            core::ptr::addr_of_mut!(DEMAND_REG).write(self.reg.take());
            core::ptr::addr_of_mut!(DEMAND_UNREG).write(self.unreg.take());
        }
    }
}

/// X-5 映像窗数帽 = `berry_winplan::WIN_MAX`（顶部单源引回；回收表容量
/// 注释在彼处）。

/// X-5 demand 窗规划（berry-winplan::plan_wins 的 FFI 桥）：根幅经
/// `berry_vmo_get_size` 现查——查不到（无 rootfs 面）= None，eager 由
/// 调用方 pump 报。纯几何与 congruent 门 (a')-(e') 全在 berry-winplan
/// （host 测试盖，tests/plan.rs）。
fn demand_plan_wins(
    h: &berry_loader::ElfHead,
    base: u64,
    len: u64,
    data_off: u64,
    h_rootfs: Handle,
    out: &mut [WinPlan],
) -> Option<usize> {
    let mut cap = 0u64;
    if berry_vmo_get_size(h_rootfs, &mut cap) != 0 {
        return None;
    }
    berry_winplan::plan_wins(h, base, len, data_off, cap, out)
}

/// `ChildHandles::reap` 的等待帽（绝对）：EOF 先行约定下子进程退出在
/// EOF 后微秒量级，30s 帽只兜挂死面（未退出 = Err 交调用方定死法）。
const REAP_TIMEOUT_NS: u64 = 30 * 1_000_000_000;

/// MMIO 窗口映射 VA（P6 A-2）：[`SpawnSpec::mmio_vmo`] 的落位——远离映像
/// /栈区（0x1000 起的低带）的高带定值，驱动探测/编程面读它。与内核
/// `USER_VA_TOP = 1<<39` 界内相安（vmar_map 的 OutOfBounds 兜底）。
pub const VIRTIO_MMIO_VA: u64 = 0x7000_0000;

/// GUI-1：framebuffer MMIO 窗映射 VA（[`SpawnSpec::mmio_vmo`] 传 fb VMO
/// 时的 [`SpawnSpec::mmio_va`] 落位）——与 virtio 窗相邻的高带定值
/// （0x7000_0000 起 +32KiB 窗），guifbd 引同常量消费（单一事实源）。
pub const FB_VA: u64 = 0x7100_0000;

/// virtio-mmio 窗口扫描几何（QEMU `virt` 板，与内核 HAL
/// `VIRTIO_MMIO0_PA/LEN` 对读维护——两工作区不共享 crate，靠注释互指）：
/// 32 槽 × 槽距 0x200。板子按槽 31→0 倒序建总线、首个 `-device` 落最高槽
/// （QEMU monitor 实证）——**设备落位随 `-device` 顺序漂移，消费端扫窗
/// 定位、不可绑死槽号**（P6 A-2 首版绑 transport[0] 实踩：读到空槽
/// ver=1/did=0，`VIRTIO ok` 缺失）。
pub const VIRTIO_MMIO_SLOTS: usize = 32;
/// 单槽寄存器组幅（virtio 1.x 规范）。
pub const VIRTIO_MMIO_STRIDE: usize = 0x200;

/// 单窗回收记录（X-5）：`ChildHandles::wins` 表的一项——三色窗统一
/// 回收面（demand 窗注销 → 逐窗 unmap → 逐窗 close）。
#[derive(Clone, Copy)]
pub struct ChildWin {
    /// 窗 VMO 句柄（0 = 空槽；demand 窗 = `VMO_OPT_DEMAND`、hybrid 窗 =
    /// 常规预填页、eager 路 = 全幅单窗）。
    pub vmo: Handle,
    /// 窗基（绝对 VA，页对齐）+ 幅（页倍数）。
    pub start: u64,
    pub len: u64,
    /// demand 窗标记（true = reap 先经注销钩子摘 pager 槽再回收；
    /// hybrid/eager 窗 false——无 pager 面）。
    pub demand: bool,
}

/// [`spawn_rootfs_elf`] 的产物（Plan B v0.5）：回收面所需的全部子进程
/// 句柄 + 几何。**所有权归调用方**——不回收 = 有界泄漏（v1 前 `run` 一次
/// 漏一个子进程的旧常态）；`reap()` = 完整回收链。
pub struct ChildHandles {
    /// process handle（wait TERMINATED 用；最后关）。
    pub hp: Handle,
    /// root Vmar handle（unmap 用）。
    pub hv: Handle,
    /// thread handle。
    pub ht: Handle,
    /// 栈 VMO（unmap 后 close = destroy → Drop 还帧）。
    pub svmo: Handle,
    /// 映像窗表（X-5：升序铺满映像幅 [base, base+len)——demand 路 =
    /// 规划窗序、eager 路 = 全幅单窗；`wins[..nwin]` 为有效前缀）。
    pub wins: [ChildWin; WIN_MAX],
    pub nwin: usize,
    /// 栈映射基址/幅。
    pub stack_va: u64,
    pub stack_len: u64,
    /// MMIO 窗口映射基址/幅（P6 A-2；幅 0 = 无窗口——`SpawnSpec::mmio_vmo`
    /// 为 None 时）。reap 末刀 unmap 用（mapcnt 归零，设备窗口不受影响——
    /// 只清叶失效 TLB）。
    pub mmio_base: u64,
    pub mmio_len: u64,
}

impl ChildHandles {
    /// 回收已退出的子进程（Plan B v0.5；P5 起**返退出码**——`&&`/`$?` 的
    /// 真值源）：wait `SIGNAL_TERMINATED`（30s 绝对帽——EOF 先行约定下常态
    /// 瞬回）→ **read exit code（0x3A，wait 后 close 前读——close hp 即销毁
    /// Process 对象，读完码才许放）** → unmap 映像/栈/MMIO 窗口（mapcnt 归零；
    /// 窗口幅 0 = 无第三刀）→ close
    /// 栈/映像 VMO（destroy-close 不再拒，Drop 还帧）→ close thread/vmar/
    /// process（对象 Box 释放；root Vmar 的 L1/PT 帧与 ASID v1 仍泄漏——
    /// `process.rs` PROCESS_MAX 注释同口径）。任一步失败返 `Err`（句柄留存
    /// = 有界泄漏，死法交调用方：标记缺失 / 续走警告）。wait 已过但读码失败
    /// （0x3A 异常面）= `Err`：子已死句柄照收（后续 close 仍执行）…… 简化
    /// 起见读码失败即整链 Err 返（句柄留存有界，不二段收）。
    pub fn reap(&self) -> Result<i32, ()> {
        let mut now = 0u64;
        if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
            return Err(());
        }
        let mut obs = 0u64;
        if berry_object_wait_one(self.hp, SIGNAL_TERMINATED, now + REAP_TIMEOUT_NS, &mut obs) != 0 {
            return Err(()); // 未按期退出（挂死/帽尽）：不冒险回收
        }
        // wait 命中 = Exited 已落账（mark_exited 先于 wake TERMINATED——退出
        // 钩子语义序），0x3A 必成功；防御面返 Err（未读码不放对象）。
        let mut code: i32 = 0;
        if berry_process_get_exit_code(self.hp, &mut code) != 0 {
            return Err(());
        }
        // X-5：demand 槽注销——**先于 close dv**（防句柄值复用后 pager 误供
        // 新主）。按值扫描在活句柄域内唯一 ⇒ hybrid/eager 窗（非 demand
        // VMO）无假命中；钩子缺席（bsh 副本）= 恒跳过，零漂移。
        // SAFETY: 只读（boot 后无写者）。
        if let Some(unreg) = unsafe { core::ptr::addr_of!(DEMAND_UNREG).read() } {
            for w in &self.wins[..self.nwin] {
                if w.demand {
                    unreg(w.vmo);
                }
            }
        }
        // 逐窗 unmap（X-5：窗表升序铺满映像幅——逐刀即全幅）+ 栈/MMIO。
        // 任一失败记账不早退（后续 close 照走——比早退泄漏面更紧；失败
        // 路径只影响死账可观测面，绿色路径零差）。
        let mut ok = true;
        for w in &self.wins[..self.nwin] {
            if w.vmo != 0 && berry_vmar_unmap(self.hv, w.start as usize, w.len as usize) != 0 {
                ok = false;
            }
        }
        if berry_vmar_unmap(self.hv, self.stack_va as usize, self.stack_len as usize) != 0 {
            ok = false;
        }
        if self.mmio_len != 0
            && berry_vmar_unmap(self.hv, self.mmio_base as usize, self.mmio_len as usize) != 0
        {
            ok = false; // 设备窗口 unmap 失败 = 叶残留（AS 将销毁，只脏死表）
        }
        // close 链：栈 VMO → 各窗 VMO → thread/vmar/process（destroy-close
        // 不再拒，Drop 还帧）。
        if berry_handle_close(self.svmo) != 0 {
            ok = false;
        }
        for w in &self.wins[..self.nwin] {
            if w.vmo != 0 && berry_handle_close(w.vmo) != 0 {
                ok = false;
            }
        }
        if berry_handle_close(self.ht) != 0
            || berry_handle_close(self.hv) != 0
            || berry_handle_close(self.hp) != 0
        {
            ok = false;
        }
        if !ok {
            return Err(());
        }
        Ok(code)
    }
}

/// 建通道对（两端均授 SEND|RECV|DUPLICATE；返回 (本端, 交接端)）。syscall
/// 失败自陷（通道是启动关键原语——失败 = 系统级异常）。
pub fn channel_pair() -> (Handle, Handle) {
    let mut a = 0 as Handle;
    let mut b = 0 as Handle;
    if berry_channel_create(0, &mut a, &mut b) != 0 {
        trap();
    }
    (a, b)
}

/// feeder 发送（P5 v0.5）：-EBUSY（对端满队）→ 阻塞等 WRITABLE 10s 重臂
/// （libc `console_write` 同款契约——`recv` 腾位经 `wake_matching(对端,
/// WRITABLE)` 唤醒，匹配键 = ObjRef 地址，跨 handle 别名互通）；其余非 0
/// 错误自陷（feeder 链断 = 系统级异常，标记缺失 loud 兜底）。零长发送
/// （EOF 标记）同路——满队时 EOF 也要排队。
fn send_retry(h: Handle, p: *const u8, n: usize) {
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(h, p, n, core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            trap();
        }
        if deadline == 0 {
            let mut now = 0u64;
            if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
                trap();
            }
            deadline = now + 10_000_000_000;
        }
        let mut obs = 0u64;
        let _ = berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs);
        deadline = 0; // 重臂（命中后若仍满则续等——公平排队）
    }
}

/// [`spawn_rootfs_elf`] 的参数面（A-2 SpawnSpec 化——9 参位置列表易错位，
/// G-eco-c 的 ABI 错位教训）。
pub struct SpawnSpec<'a> {
    /// cpio 名 = argv0（NUL 分隔 blob 首项；blob 192B，截 191B）。
    pub name: &'a [u8],
    /// rootfs VMO 内 ELF 偏移（cpio_find/遍历所得 data_off）。
    pub data_off: u64,
    /// ELF 长度（data_len）。
    pub data_len: u64,
    /// fd1/2 之源：console 发端（直打串口）或私有通道对端（调用方收齐终验）。
    /// 内部 dup 成 RIGHT_SEND 交付，原值留调用方。
    pub stdout: Handle,
    /// 栈页数（按程序面：平像 1 / std 小程序 8 / clap 级 32——musl 默认
    /// 128KiB 口径，d-3c 实踩）。
    pub stack_pages: u64,
    /// Some(data) → 建 stdin 对 + `process_start` 后本进程作 feeder 泵
    /// data + 零长 EOF（libc read 的 EOF 约定，d-3b）；None = 无 stdin
    /// （子 read 面恒 EOF，/dev/null 语义）。
    pub stdin_feed: Option<&'a [u8]>,
    /// gift[1] 直传既有 stdin 端（A-2：bsh = boot 手搭的 endB2——console 转发
    /// 键盘行，**非 feeder 模型**）。**被 send take**——调用方保留原值者先
    /// duplicate；与 `stdin_feed` 互斥（调试断言拦混用）。
    pub stdin_handle: Option<Handle>,
    /// argv[1..] 追加项列表（hexyl 的 `--color=never` 单参 / C-2a 起 uu 家族
    /// cp/mv 的双操作数走这条）。空片 = 无参。
    pub args: &'a [&'a [u8]],
    /// env 块 gift[2..]（A2-3 起切片 ≤3 项）：[2] = rootfs VMO READ dup
    /// （bsh 的 run 流式取 ELF）/ P6 的 fsmgr 服务端端 fs_b；[3] = fs 客户
    /// 端端（bsh 的 `>`/`cat`/`ls`，P6 A-1）；[4] = netd 服务端端（bsh 的
    /// EVS 自验，A2-3）。**全被 send take**，调用方保留原值者先 duplicate。
    /// 空片 = 老两/三元 gift（hello/stdhello/hexyl）。
    pub extra_gifts: &'a [Handle],
    /// MMIO 物理窗口 VMO（P6 A-2，boot 授能面——virtio-mmio 窗口）：Some 时
    /// 在栈页之后把它映到 [`VIRTIO_MMIO_VA`]（rights 恰 READ|WRITE，v1
    /// vmar_map 唯一档）——子进程驱动探测/编程面。调用方自留原值（map 不
    /// take 句柄；回收 unmap 由 [`ChildHandles::reap`] 按 `mmio_len` 执行）。
    pub mmio_vmo: Option<Handle>,
    /// [`Self::mmio_vmo`] 的映射落位 VA（GUI-1 参数化：virtio 窗用
    /// [`VIRTIO_MMIO_VA`]、fb 窗用 [`FB_VA`]——同字段面第二消费者）。
    /// `mmio_vmo == None` 时无效（填 0 即可）。Some 且 0 = 配置错，trap。
    pub mmio_va: u64,
    /// berry_intent（ADR #42 两段式；GUI-8 §4 段① SDK 透传口②）：随
    /// `berry_thread_create` options 低 8 位透传（berry-sys 同口径注释）。
    /// **默认 0 = 现行为零漂**。dormant：v1 内核只收 options==0，非 0 落
    /// -EINVAL 走本 crate syscall 面自陷纪律 trap()——消费面保持 0，内核
    /// 收 intent 后（ADR #42 v2 段）此口自然激活。
    pub intent: u8,
    /// X-5 常驻豁免：true = 跳过 demand 规划直走 eager。恢复 X-3c-3 让位
    /// 闸的隔离不变式「dpg 窗内无外来在册 demand 窗」——长命常驻子
    /// （vblk/guifbd/gui-wmd/fsmgr/vnet/gui-inputd/bsh）任意时刻武装缺页
    /// 会毒化 0x21 全局取货（rvmo≠dv 身份错配 EINVAL 单发 bail）+ hold
    /// 期供给暂停挂常驻子（首轮 X-5 smoke 实炸：`eco: dpg sup` 后死寂 /
    /// INPUT 步挂死）。短命测试族保持 demand（X-5 足迹收益主体）；dpg 自
    /// 测族子必须 false（0x21 测试本体）。
    pub force_eager: bool,
}

/// rootfs ELF 流式拉起（页铺平单 VMO，同 `tools/elf2flat.py` 语义）：
/// ELF 首窗 `elf_head` 解析（**跨度按 p_memsz**——std RW 段 memsz 含静态堆
/// + .bss；入口入映像检在装载端拦）→ 子 VMO `[floor页(min), +ceil页(span))`
/// 一次 commit/map → 各段文件字节按 4KiB 泵窗从 rootfs VMO 直送子 VMO 相对
/// 偏移（**ELF 本体永不整档驻留**）→ 栈页映其上（隔一页保护页）→ **env 块**
/// （d-3a，BerryOS processargs v1）：channel 建 env 对 → `send(env_s,
/// argv0\0…, [stdout 端别名, (stdin 端), (extra_gifts…)])`（能力随消息迁移）→
/// `process_start(bootstrap=env_c)`——子进程入口 x0 = env 端，close 原值必
/// -EBADF（迁移自证）。
///
/// 解析/首窗读失败返 Err（调用方可见降级续走归档）；syscall 面失败自陷
/// ——链断 loud（标记缺失兜底）。成功返 [`ChildHandles`]（Plan B v0.5：
/// 调用方收完输出后 `reap()` 完整回收；不回收 = 有界泄漏）。
pub fn spawn_rootfs_elf(h_rootfs: Handle, spec: &SpawnSpec) -> Result<ChildHandles, ()> {
    let SpawnSpec {
        name,
        data_off,
        data_len,
        stdout,
        stack_pages,
        stdin_feed,
        stdin_handle,
        args,
        extra_gifts,
        mmio_vmo,
        mmio_va,
        intent,
        force_eager,
    } = spec;
    assert!(
        stdin_feed.is_none() || stdin_handle.is_none(),
        "spawn: stdin_feed/stdin_handle 互斥"
    );
    // 标量字段解包一次（SpawnSpec 经引用传入，后续按值用）。
    let (data_off, data_len, stdout, stack_pages, mmio_va, intent) =
        (*data_off, *data_len, *stdout, *stack_pages, *mmio_va, *intent);
    let want = (data_len.min(4096u64)) as usize;
    // SAFETY: 调用方单线程顺序调用（svcmgr/bsh 主循环），本窗独占。
    let win: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(ELF_WIN) as *mut u8, 4096)
    };
    if berry_vmo_read(h_rootfs, win.as_mut_ptr(), data_off, want) != 0 {
        return Err(());
    }
    let h = match berry_loader::elf_head(&win[..want], data_len) {
        Ok(h) => h,
        Err(_) => return Err(()), // 截断/野值/不收面：可见降级，不杀调用方
    };
    let base = berry_loader::page_floor(h.image_min);
    let len = berry_loader::page_ceil(h.image_span);
    // —— spawn 几何前置门（映幅帽/栈顶/MMIO 重叠三段）：纯面单源在
    //    berry-spawngeo（host 可测——原处内嵌 FFI 令此面零测试，winplan/
    //    argv 同款外迁纪律），此处 FFI 桥。门序保真（原处三段序）：帽/栈
    //    两段先裁（rootfs 输入面降级先于 boot 契约面 trap——巨段 + 契约破
    //    的组合面不因外迁改判 trap）；窗口幅经 vmo_get_size 取得（boot
    //    授能契约：查不到/零幅仍 trap 非输入面）后全门终裁。
    if berry_spawngeo::spawn_geometry(base, len, stack_pages, None).is_err() {
        return Err(());
    }
    let mut mmio_len = 0u64;
    let mmio_geo = if let Some(mh) = *mmio_vmo {
        if mmio_va == 0 {
            trap(); // Some 窗口零 VA = 调用面配置错（boot 授能契约）
        }
        if berry_vmo_get_size(mh, &mut mmio_len) != 0 || mmio_len == 0 {
            trap(); // 窗口几何是 boot 授能契约——查不到即链断
        }
        Some((mmio_va, mmio_len))
    } else {
        None
    };
    if berry_spawngeo::spawn_geometry(base, len, stack_pages, mmio_geo).is_err() {
        return Err(()); // 三段同走降级（原因枚举在纯面侧，消费端不分流）
    }
    let mut hp = 0 as Handle;
    let mut hv = 0 as Handle;
    let s = berry_process_create(0, 0, &mut hp, &mut hv);
    if s == BERRY_ERR_NO_MEMORY {
        // 资源耗尽 = ABI 合法运行时条件（内核 PROCESS_MAX=64 出帽返 -ENOMEM），
        // 非系统级异常——Err 降级不杀调用方（P5 v0.5 对抗审查：管道不退出段
        // 僵尸累积到帽后，trap 反杀 bsh = 交互 shell 死亡；出参在帽检前预清
        // 0，此刻零已建对象，直接返回零清理面）。其余站点（vmo/commit/map/
        // thread）的帧耗尽在 64 帽内不可达（64×~0.7MB ≈ 45MB ≪ 128M）仍
        // trap——PROCESS_MAX 提高时（v2）须同步扩这条降级。
        return Err(());
    }
    if s != 0 || hp == 0 || hv == 0 {
        trap();
    }
    // —— X-5 demand 装载（多窗 congruent 门 + 钩子在位才走；bsh 无钩子恒
    //    eager）：映像幅 [base, base+len) 按 `demand_plan_wins` 分解为升序
    //    三色窗——file 窗 = 本窗恒等映射 demand VMO（0x23 sv=rootfs,
    //    delta_i）、零窗 = 零供给 demand VMO（0x23 src=0 臂）、hybrid 尾页
    //    = eager 预填 1 页（段尾混合页零语义保真，见 `WinKind::Hybrid`）。
    //    任一失面（含 reg false 槽满）= 已建已注窗全回滚走下方 eager 原路
    //    ——启动内存足迹 ∝ 实触页（X-3 验收面），非 congruent/无钩子永有
    //    eager 兜底。
    let mut wins: [ChildWin; WIN_MAX] =
        [ChildWin { vmo: 0, start: 0, len: 0, demand: false }; WIN_MAX];
    let mut nwin = 0usize;
    let mut plan: [WinPlan; WIN_MAX] = [WinPlan { start: 0, len: 0, kind: WinKind::Zero }; WIN_MAX];
    if let (Some(nw), Some(reg)) = (
        if *force_eager {
            None // 常驻豁免（SpawnSpec::force_eager）：直落 eager 原路
        } else {
            demand_plan_wins(&h, base, len, data_off, h_rootfs, &mut plan)
        },
        demand_reg_hook(),
    ) {
        for w in &plan[..nw] {
            let mut dv = 0 as Handle;
            let mut mapped = false;
            let mut regd = false;
            let demand = match w.kind {
                WinKind::File { delta } => {
                    // file 域窗：demand VMO + 恒等供给注册。
                    if berry_vmo_create(w.len, VMO_OPT_DEMAND, &mut dv) == 0 && dv != 0 {
                        let mut addr = w.start as usize;
                        mapped = berry_vmar_map(
                            hv,
                            0,
                            dv,
                            0,
                            &mut addr,
                            w.len as usize,
                            RIGHT_READ | RIGHT_WRITE,
                        ) == 0
                            && addr == w.start as usize;
                        if mapped {
                            regd = reg(dv, h_rootfs, delta);
                        }
                    }
                    mapped && regd
                }
                WinKind::Zero => {
                    // 零域窗：demand VMO + 零供给注册（sv=0 → 内核零页臂）。
                    if berry_vmo_create(w.len, VMO_OPT_DEMAND, &mut dv) == 0 && dv != 0 {
                        let mut addr = w.start as usize;
                        mapped = berry_vmar_map(
                            hv,
                            0,
                            dv,
                            0,
                            &mut addr,
                            w.len as usize,
                            RIGHT_READ | RIGHT_WRITE,
                        ) == 0
                            && addr == w.start as usize;
                        if mapped {
                            regd = reg(dv, 0 as Handle, 0);
                        }
                    }
                    mapped && regd
                }
                WinKind::Hybrid { soff, flen } => {
                    // hybrid 尾页：常规 VMO + commit + flen 字节泵（余零——
                    // fresh VMO 零底 = eager 全幅 VMO 零语义同源）+ 映射；
                    // 无 pager 面（不注册）。
                    if berry_vmo_create(w.len, 0, &mut dv) == 0 && dv != 0 {
                        if berry_vmo_commit(dv, 0, w.len) == 0 {
                            // SAFETY: 同 eager 段泵窗纪律（单线程顺序调用）。
                            let pump: &mut [u8] = unsafe {
                                core::slice::from_raw_parts_mut(
                                    core::ptr::addr_of_mut!(PUMP_WIN) as *mut u8,
                                    4096,
                                )
                            };
                            let n = flen as usize;
                            if berry_vmo_read(h_rootfs, pump.as_mut_ptr(), soff, n) == 0
                                && berry_vmo_write(dv, pump.as_ptr(), 0, n) == 0
                            {
                                let mut addr = w.start as usize;
                                mapped = berry_vmar_map(
                                    hv,
                                    0,
                                    dv,
                                    0,
                                    &mut addr,
                                    w.len as usize,
                                    RIGHT_READ | RIGHT_WRITE,
                                ) == 0
                                    && addr == w.start as usize;
                            }
                        }
                    }
                    mapped
                }
            };
            if !demand {
                // 本窗失面：先回滚本窗半成品（unreg → unmap → close，序同
                // reap），再回滚此前已建成窗，eager 原路重走（同一 hv）。
                if regd {
                    if let Some(unreg) = demand_unreg_hook() {
                        unreg(dv);
                    }
                }
                if mapped {
                    let _ = berry_vmar_unmap(hv, w.start as usize, w.len as usize);
                }
                if dv != 0 {
                    let _ = berry_handle_close(dv);
                }
                if let Some(unreg) = demand_unreg_hook() {
                    for w2 in &wins[..nwin] {
                        if w2.demand {
                            unreg(w2.vmo);
                        }
                    }
                }
                for w2 in &wins[..nwin] {
                    let _ = berry_vmar_unmap(hv, w2.start as usize, w2.len as usize);
                    let _ = berry_handle_close(w2.vmo);
                }
                nwin = 0;
                break;
            }
            wins[nwin] = ChildWin {
                vmo: dv,
                start: w.start,
                len: w.len,
                demand: !matches!(w.kind, WinKind::Hybrid { .. }),
            };
            nwin += 1;
        }
    }
    if nwin == 0 {
        // —— eager 原路（全量铺 + 全幅一次 commit/map；v1 既有行为零漂移）——
        let mut vmo = 0 as Handle;
        if berry_vmo_create(len, 0, &mut vmo) != 0 || vmo == 0 {
            trap();
        }
        if berry_vmo_commit(vmo, 0, len) != 0 {
            trap();
        }
        // 段文件字节泵：rootfs VMO [data_off+file_off, +filesz) → 子 VMO。
        // SAFETY: 同上，泵窗独占。
        let pump: &mut [u8] = unsafe {
            core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(PUMP_WIN) as *mut u8, 4096)
        };
        for seg in h.segments() {
            let mut off = 0u64;
            while off < seg.filesz {
                let n = ((seg.filesz - off).min(pump.len() as u64)) as usize;
                if berry_vmo_read(h_rootfs, pump.as_mut_ptr(), data_off + seg.file_off + off, n)
                    != 0
                {
                    trap();
                }
                if berry_vmo_write(vmo, pump.as_ptr(), seg.vaddr - base + off, n) != 0 {
                    trap();
                }
                off += n as u64;
            }
        }
        let mut vaddr = base as usize; // 映射基址提示（in-out 槽预写）
        if berry_vmar_map(hv, 0, vmo, 0, &mut vaddr, len as usize, RIGHT_READ | RIGHT_WRITE) != 0
            || vaddr != base as usize
        {
            trap(); // v1 无重排：回写须 == 请求基址
        }
        wins[0] = ChildWin {
            vmo,
            start: base,
            len,
            demand: false,
        };
        nwin = 1;
    }
    // 栈页映在映像之上——中间隔一页**保护页**：栈用穿 = 踩未映射页 → 干净
    // D-abort 被杀（dump SP/FAR 直指祸首）；无保护页时下行栈会静默扎进
    // 映像 RW 页腐化 .bss/堆簿记，死状与死因南辕北辙（d-3c hexyl 实踩：
    // 栈穿 32KiB 覆写 `__aarch64_have_lse_atomics` → LSE 误判 → `casa`
    // UNDEFINED）。映幅尾 = ceil 页，+1 页即页对齐保护页。
    let stack_len = stack_pages * berry_loader::PAGE_SIZE;
    let stack_va = base + len + berry_loader::PAGE_SIZE;
    let mut svmo = 0 as Handle;
    if berry_vmo_create(stack_len, 0, &mut svmo) != 0 || svmo == 0 {
        trap();
    }
    if berry_vmo_commit(svmo, 0, stack_len) != 0 {
        trap();
    }
    let mut saddr = stack_va as usize;
    if berry_vmar_map(hv, 0, svmo, 0, &mut saddr, stack_len as usize, RIGHT_READ | RIGHT_WRITE)
        != 0
        || saddr != stack_va as usize
    {
        trap();
    }
    // —— MMIO 物理窗口（P6 A-2；GUI-1 起 VA 参数化）：boot 授能的 MMIO VMO
    //    （virtio 窗 / fb 窗同字段面）映到 [`SpawnSpec::mmio_va`]（高带定值；
    //    与映像/栈带的重叠已在 process_create 前置 Err 降级——rootfs 输入面
    //    纪律，栈顶前置同族）。rights 恰 READ|WRITE（v1 vmar_map 唯一档）。
    //    map 不 take 句柄（调用方自留）；子进程只拿映射、不拿 handle——
    //    设备窗口能力不出 svcmgr 表。窗口幅 mmio_len 来自前置 get_size
    //    （boot 授能契约面）。
    if let Some(mh) = *mmio_vmo {
        let mut maddr = mmio_va as usize;
        if berry_vmar_map(hv, 0, mh, 0, &mut maddr, mmio_len as usize, RIGHT_READ | RIGHT_WRITE) != 0
            || maddr != mmio_va as usize
        {
            trap();
        }
    }
    let mut ht = 0 as Handle;
    // options = spec intent 低 8 位透传（ADR #42 占位口②；berry-libc attr 面
    // 同口径）。v1 内核只收 0——非 0（dormant 调用面）-EINVAL 走自陷纪律
    // （词表与激活时序见 `SpawnSpec::intent`）。
    if berry_thread_create(hp, intent as u32, &mut ht) != 0 || ht == 0 {
        trap();
    }
    // —— env 块（start 前压队，确定性先行）：argv = argv0\0[+arg\0]；handles
    //    按位 [stdout 端别名, (stdin 端), (self_proc/self_vmar), (extra_gift…)]
    //    （先 dup stdout：send 会 take 走句柄，原值留调用方——G-eco-b 交接
    //    消息同款）。
    //    **前缀紧致律（T-1 终验实抓回归的立法）**：gift[0..nhandles] 必须
    //    是满前缀、零 0 洞——内核 send 预检逐 handle `alive()`，0（INVALID_
    //    HANDLE）即 -EBADF 自陷（T-1 首版把自描述对钉死 [4]/[5]、extra<2
    //    时中段留洞 = svcmgr 拉起面全体连灭，vblk/ps/fsmgr/uu_run 族）。
    //    修法 = 自描述对前移固定位 [2]/[3]、extras 挪 [4..4+len]——每槽
    //    语义固定、nhandles 单调门控 presence，消费侧（libc io.rs boot）同
    //    步对偶。无 stdin 的老 spawn 面（svcmgr 无管道子）nhandles=1 不装。
    let (env_s, env_c) = channel_pair();
    // 8 槽（A2-3：[4] rootfs / [5] fs / [6] netd；GUI-4 段②：+ [7] input
    // ——svcmgr bsh arm 四 extras；bsh 内 uu_run 族仍 ≤2 消费同一面零漂移）。
    let mut gift = [0 as Handle; 8];
    if berry_handle_duplicate(stdout, RIGHT_SEND, &mut gift[0]) != 0 {
        trap();
    }
    let stdin_pair = stdin_feed.map(|_| channel_pair());
    let mut nhandles = if stdin_pair.is_some() { 2 } else { 1 };
    if let Some((_, in_c)) = stdin_pair {
        gift[1] = in_c; // stdin 读端随 env 块迁移（handle 数组按位语义）
    }
    if let Some(h) = stdin_handle {
        // A-2：既有 stdin 端直传（bsh 的 endB2——console 键盘行转发对端）。
        gift[1] = *h; // 被 send take（迁移语义同上）
        nhandles = nhandles.max(2);
    }
    // slice 帽（指令字面 ≤4）+ 容量执法（2 base + 2 自描述对 + extra ≤ 8 →
    // extra ≤ 4）双断言：越界即 panic loud（bsh arm 四 extras = 满配面）。
    assert!(extra_gifts.len() <= 4, "spawn: extra_gifts slice 帽 4");
    assert!(
        4 + extra_gifts.len() <= gift.len(),
        "spawn: extra_gifts ≤ 4（GUI-4 段②：bsh 面 +input 端——8 槽满配）"
    );
    if !extra_gifts.is_empty() {
        // extras 落 [4] 起要求 stdin 在位（前缀紧致律：[2]/[3] 缺 stdin 即
        // 洞——运行时断言拦调用面漂移）。
        assert!(
            stdin_pair.is_some() || stdin_handle.is_some(),
            "spawn: extra_gifts 需 stdin 在位"
        );
    }
    // —— T-1 自描述对（条件装配：stdin 在位 = T-1 时代的调用面），固定位
    //    [2]/[3]：[2] = 自身 Process（MANAGE|DUPLICATE——子内 pthread_
    //    create 走 thread_create）、[3] = 自身 root Vmar（MAP|MANAGE|
    //    DUPLICATE——vmar_map 授片 + vmar_unmap 门控 RIGHT_MAP）。
    //    extras 紧随落 [4..4+len]（**装在块内** = 结构性保证前缀无洞：
    //    extras 不可能先于自描述对出现）；nhandles = 4 + len ∈ 4..=8。
    //    子内 `self_proc_h()==0`（无 stdin 面）→ pthread_create EAGAIN
    //    守门（旧桩语义零漂移）。
    if stdin_pair.is_some() || stdin_handle.is_some() {
        if berry_handle_duplicate(hp, RIGHT_MANAGE | RIGHT_DUPLICATE, &mut gift[2]) != 0 {
            trap();
        }
        if berry_handle_duplicate(hv, RIGHT_MAP | RIGHT_MANAGE | RIGHT_DUPLICATE, &mut gift[3]) != 0
        {
            trap();
        }
        for (i, g) in extra_gifts.iter().enumerate() {
            gift[4 + i] = *g; // 被 send take（迁移语义同上）
        }
        nhandles = 4 + extra_gifts.len();
    }
    // argv 串 = argv0\0 + 各参\0（C-2a 起多参）。blob 192B：消息 inline 帽
    // 256 内、argv0 + 数参的 cp/mv 形态留足余量；截断按「整参或零参」——
    // 装不下就不装（调用面 v1 参数都是短 CLI 词，实测最长的 uu_ls 长参面
    // 也在 192 内），绝不截半参。
    // 组装律单源在 berry-argv（纯面外迁，berry-winplan 同款纪律），host
    // 全盖；此处原值引回。
    let mut blob = [0u8; 192];
    let plen = berry_argv::argv_blob(&mut blob, name, args);
    if berry_channel_send(env_s, blob.as_ptr(), plen, gift.as_ptr(), nhandles) != 0 {
        trap();
    }
    if berry_process_start(hp, ht, h.entry as usize, saddr + stack_len as usize, env_c) != 0 {
        trap();
    }
    if berry_handle_close(env_c) != BERRY_ERR_BAD_HANDLE {
        trap(); // 迁移自证：原值必须已作废
    }
    if berry_handle_close(env_s) != 0 {
        trap(); // 发端使命已毕（消息已入队、对端互钉保队列）；关不掉即泄漏面
    }
    // —— feeder：数据分片 + 零长 EOF 标记（libc read 的 EOF 约定，d-3b）。
    //    P5 v0.5 分片化：消息 inline 帽 256 且队列深 8——>256B 输入（管道自验
    //    参照要喂 ~659B）单发即 -EINVAL / 多发必中途满队 -EBUSY。按 192B
    //    分片（libc `console_write` 同粒度）+ EBUSY 阻塞等 WRITABLE（10s 重
    //    臂，A-2 修订同款——自旋是雷 2 形态）：子进程 recv 腾位即唤醒公平
    //    续泵。EOF 同一重试（满队时 EOF 也得排队）。
    //    **互锁门限（对抗审查核实，改此块前必读）**：feeder 内联于本函数、
    //    返回前调用方进不了 collect——子进程 stdout 队（8×192B）先满则双侧
    //    互等死锁。安全前提 = 子进程在 stdout 灌满前消费到 feeder 收尾
    //    （≈ L−1536B）；hexyl 8KB BufWriter 下实测门限 L≥2689B（首 flush 点
    //    ≈ 已收 1344B）。**stdin 面调用方喂 >2KB 量级前先核此账**；根修 =
    //    feeder 移出 spawn + 收发交错泵（v2，随多线程/关闭检测）。
    if let (Some((in_s, _)), Some(data)) = (stdin_pair, stdin_feed) {
        let mut off = 0usize;
        while off < data.len() {
            let n = core::cmp::min(192, data.len() - off);
            let p = unsafe { data.as_ptr().add(off) };
            send_retry(in_s, p, n);
            off += n;
        }
        send_retry(in_s, core::ptr::null(), 0);
        if berry_handle_close(in_s) != 0 {
            trap();
        }
    }
    Ok(ChildHandles {
        hp,
        hv,
        ht,
        svmo,
        wins,
        nwin,
        stack_va,
        stack_len,
        mmio_base: if mmio_len > 0 { mmio_va } else { 0 },
        mmio_len,
    })
}

/// cpio 单点查询（bsh 的 `run <name>`）：全档步进找 `name`（如
/// `bin/hexyl`），返回 (data_off, data_len)。无 rootfs（get_size 失败）/
/// 档坏/未命中 → None（调用方打 not found，不死——查询面无自陷）。
/// 归档永不整档驻留（步进窗 512B，同 svcmgr 遍历纪律）。
pub fn cpio_find(h_rootfs: Handle, name: &[u8]) -> Option<(u64, u64)> {
    let mut cap = 0u64;
    if berry_vmo_get_size(h_rootfs, &mut cap) != 0 {
        return None; // 无 rootfs：vmo=none → -EBADF
    }
    // SAFETY: 调用方单线程顺序调用，本窗独占。
    let win: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(STEP_WIN) as *mut u8, 512)
    };
    let mut pos = 0u64;
    while pos < cap {
        let want = ((cap - pos).min(win.len() as u64)) as usize;
        if berry_vmo_read(h_rootfs, win.as_mut_ptr(), pos, want) != 0 {
            return None;
        }
        match berry_loader::cpio_step(&win[..want], pos, cap) {
            Ok(berry_loader::StepOutcome::End) => return None,
            Ok(berry_loader::StepOutcome::Entry(s)) => {
                if s.name.as_bytes() == name {
                    return Some((s.data_off, s.data_len));
                }
                pos = s.next_off;
            }
            Err(_) => return None, // 档坏：查询面诚实返 None（不杀 shell）
        }
    }
    None
}

/// cpio 目录前缀枚举（C-4 Tab 补全命令名候选源）：步进走档收集 name 以
/// `dir` 开头的**直接子项**（余部非空且不含 '/'），截去前缀经回调送出。
/// 与 [`cpio_find`] 同款步进纪律（512B 窗、归档永不整档驻留）；无 rootfs/
/// 档坏 → 中途返（已收部分保留——补全候选的非完整性无正确性面，静默）。
/// 回调形（`&mut dyn FnMut`）保 no_std 零 alloc：收集容器归调用方（bsh
/// std 侧 Vec）。
pub fn cpio_names(h_rootfs: Handle, dir: &[u8], f: &mut dyn FnMut(&[u8])) {
    let mut cap = 0u64;
    if berry_vmo_get_size(h_rootfs, &mut cap) != 0 {
        return; // 无 rootfs：vmo=none → 空枚举（补全静默无操作）
    }
    // SAFETY: 调用方单线程顺序调用，本窗独占（cpio_find 同款纪律）。
    let win: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(STEP_WIN) as *mut u8, 512)
    };
    let mut pos = 0u64;
    while pos < cap {
        let want = ((cap - pos).min(win.len() as u64)) as usize;
        if berry_vmo_read(h_rootfs, win.as_mut_ptr(), pos, want) != 0 {
            return;
        }
        match berry_loader::cpio_step(&win[..want], pos, cap) {
            Ok(berry_loader::StepOutcome::End) => return,
            Ok(berry_loader::StepOutcome::Entry(s)) => {
                let name = s.name.as_bytes();
                if let Some(rest) = name.strip_prefix(dir) {
                    if !rest.is_empty() && !rest.contains(&b'/') {
                        f(rest);
                    }
                }
                pos = s.next_off;
            }
            Err(_) => return, // 档坏：同 cpio_find 的查询面降级
        }
    }
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己
/// （svcmgr 同款——libc `park` 不 trap 的 watchdog 契约不适用于 syscall 面
/// 链断，标记缺失 = smoke FAIL 兜底）。
#[cold]
fn trap() -> ! {
    // SAFETY: 故意触发的未映射读（进程私有映像外；0x4000_0000 与用户程序
    // 布局 0x1000.. 无交集——svcmgr 迁移原值）。
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}
