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
//!   READ dup；P6 A-1：fsmgr 端/fs 客户端端）。**随行 handle 被 send
//!   take**——调用方需保留原值者先 duplicate；
//! - **回收（Plan B v0.5）**：成功返 [`ChildHandles`]，调用方收完输出后
//!   `reap()`（wait TERMINATED → unmap×3 → close×5——P6 A-2 起含 MMIO 窗口）
//!   ——完整回收映像/栈帧与对象 Box；不 reap = 有界泄漏（root Vmar L1/PT
//!   帧与 ASID v1 恒泄漏，见 [`ChildHandles::reap`] 注释）。

#![no_std]

use berry_sys::{
    berry_channel_create, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_handle_duplicate, berry_object_wait_one, berry_process_create,
    berry_process_get_exit_code, berry_process_start, berry_thread_create, berry_vmar_map,
    berry_vmar_unmap, berry_vmo_commit, berry_vmo_create, berry_vmo_get_size, berry_vmo_read,
    berry_vmo_write, Handle, BERRY_ERR_BAD_HANDLE, BERRY_ERR_BUSY, BERRY_ERR_NO_MEMORY,
    CLOCK_MONOTONIC, RIGHT_READ, RIGHT_SEND, RIGHT_WRITE, SIGNAL_TERMINATED, SIGNAL_WRITABLE,
};

/// ELF 首窗：ehdr + 全部 phdr（真实 lld 产物 phnum ≤ 10 量级，4KiB 绰绰）。
static mut ELF_WIN: [u8; 4096] = [0; 4096];
/// 段字节泵窗：rootfs VMO → 子 VMO 的 4KiB 中转（std 镜像 ~283KB ≈ 70 泵）。
static mut PUMP_WIN: [u8; 4096] = [0; 4096];
/// cpio 步进窗（cpio_find 用）：头 110B + 名（mkcpio 名 ≤ 56B 绰绰）。
static mut STEP_WIN: [u8; 512] = [0; 512];

/// ELF 映幅帽 = 内核 `VMO_MAX_SIZE`（16MiB）同值（复盘雷 8 残留面）：
/// `elf_head` 只挡 >2^39 野值，(16MiB, 2^39) 区间的合法巨段会走到
/// vmo_create 才 -ENOMEM → 自陷——span 属 rootfs **输入面**（非内核契约
/// 面），按解析侧同款契约 Err 降级；真实 std 镜像 ~283KB，余量充足。
const ELF_MAP_MAX: u64 = 16 * 1024 * 1024;

/// `ChildHandles::reap` 的等待帽（绝对）：EOF 先行约定下子进程退出在
/// EOF 后微秒量级，30s 帽只兜挂死面（未退出 = Err 交调用方定死法）。
const REAP_TIMEOUT_NS: u64 = 30 * 1_000_000_000;

/// MMIO 窗口映射 VA（P6 A-2）：[`SpawnSpec::mmio_vmo`] 的落位——远离映像
/// /栈区（0x1000 起的低带）的高带定值，驱动探测/编程面读它。与内核
/// `USER_VA_TOP = 1<<39` 界内相安（vmar_map 的 OutOfBounds 兜底）。
pub const VIRTIO_MMIO_VA: u64 = 0x7000_0000;

/// virtio-mmio 窗口扫描几何（QEMU `virt` 板，与内核 HAL
/// `VIRTIO_MMIO0_PA/LEN` 对读维护——两工作区不共享 crate，靠注释互指）：
/// 32 槽 × 槽距 0x200。板子按槽 31→0 倒序建总线、首个 `-device` 落最高槽
/// （QEMU monitor 实证）——**设备落位随 `-device` 顺序漂移，消费端扫窗
/// 定位、不可绑死槽号**（P6 A-2 首版绑 transport[0] 实踩：读到空槽
/// ver=1/did=0，`VIRTIO ok` 缺失）。
pub const VIRTIO_MMIO_SLOTS: usize = 32;
/// 单槽寄存器组幅（virtio 1.x 规范）。
pub const VIRTIO_MMIO_STRIDE: usize = 0x200;

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
    /// 映像 VMO（unmap 后 close = destroy → Drop 还帧）。
    pub vmo: Handle,
    /// 栈 VMO（同上）。
    pub svmo: Handle,
    /// 映像映射基址/幅（unmap 精确匹配用）。
    pub img_base: u64,
    pub img_len: u64,
    /// 栈映射基址/幅。
    pub stack_va: u64,
    pub stack_len: u64,
    /// MMIO 窗口映射基址/幅（P6 A-2；幅 0 = 无窗口——`SpawnSpec::mmio_vmo`
    /// 为 None 时）。reap 第三 unmap 用（mapcnt 归零，设备窗口不受影响——
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
        if berry_vmar_unmap(self.hv, self.img_base as usize, self.img_len as usize) != 0
            || berry_vmar_unmap(self.hv, self.stack_va as usize, self.stack_len as usize) != 0
        {
            return Err(());
        }
        if self.mmio_len != 0
            && berry_vmar_unmap(self.hv, self.mmio_base as usize, self.mmio_len as usize) != 0
        {
            return Err(()); // 设备窗口 unmap 失败 = 叶残留（AS 将销毁，只脏死表）
        }
        if berry_handle_close(self.svmo) != 0
            || berry_handle_close(self.vmo) != 0
            || berry_handle_close(self.ht) != 0
            || berry_handle_close(self.hv) != 0
            || berry_handle_close(self.hp) != 0
        {
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
    /// cpio 名 = argv0（NUL 分隔 blob 首项；截 63B）。
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
    /// argv[1] 追加项（hexyl 的 `--color=never` 走这条）。
    pub extra_arg: Option<&'a [u8]>,
    /// env 块 gift[2..]（P6 A-1 起切片 ≤2 项）：[2] = rootfs VMO READ dup
    /// （bsh 的 run 流式取 ELF）/ P6 的 fsmgr 服务端端 fs_b；[3] = fs 客户
    /// 端端（bsh 的 `>`/`cat`/`ls`，P6 A-1）。**全被 send take**，调用方
    /// 保留原值者先 duplicate。空片 = 老两/三元 gift（hello/stdhello/hexyl）。
    pub extra_gifts: &'a [Handle],
    /// MMIO 物理窗口 VMO（P6 A-2，boot 授能面——virtio-mmio 窗口）：Some 时
    /// 在栈页之后把它映到 [`VIRTIO_MMIO_VA`]（rights 恰 READ|WRITE，v1
    /// vmar_map 唯一档）——子进程驱动探测/编程面。调用方自留原值（map 不
    /// take 句柄；回收 unmap 由 [`ChildHandles::reap`] 按 `mmio_len` 执行）。
    pub mmio_vmo: Option<Handle>,
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
        extra_arg,
        extra_gifts,
        mmio_vmo,
    } = spec;
    assert!(
        stdin_feed.is_none() || stdin_handle.is_none(),
        "spawn: stdin_feed/stdin_handle 互斥"
    );
    // 标量字段解包一次（SpawnSpec 经引用传入，后续按值用）。
    let (data_off, data_len, stdout, stack_pages) =
        (*data_off, *data_len, *stdout, *stack_pages);
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
    if len > ELF_MAP_MAX {
        return Err(()); // 段幅前置（雷 8）：巨段在几何层即降级
    }
    // 栈顶前置（对抗审查补，ELF_MAP_MAX 同族输入面）：映像顶 + 保护页 +
    // 栈幅越 USER_TOP 的 ELF（`elf_head` 只严格拒 vend > USER_TOP——贴顶
    // 映像本身合法可映）此前会走到 vmar_map 栈页才被 OutOfBounds 拒 →
    // 自陷杀调用方（svcmgr 死 = watchdog 错杀 panic 连坐 halt）。映像+
    // 栈几何同属 rootfs 输入面，前置 Err 降级；`stack_pages` 是消费方参数
    // （bsh 32/svcmgr 8），故检在 berry-exec 而非 loader。checked 链防
    // 调用方巨值溢出绕检；stack_pages==0 只过上限不过下限——原值会走到
    // vmo_create(0) -EINVAL → trap() 杀调用方（a-2 面由 manifest 域检
    // 1..=64 挡住，此检护全族其它调用方——app-0 a-3 U3）。
    let stack_end = stack_pages
        .checked_mul(berry_loader::PAGE_SIZE)
        .and_then(|s| (base + len + berry_loader::PAGE_SIZE).checked_add(s));
    if stack_pages == 0 || stack_end.map_or(true, |e| e > berry_loader::USER_TOP) {
        return Err(());
    }
    // —— MMIO 固定 VA 前置（对抗审查补，栈顶前置同族第三块几何约束）：
    //    [base, stack_end) 与 [VIRTIO_MMIO_VA, +窗口幅) 相交的 ELF（rootfs
    //    输入面可达：PT_LOAD vaddr 落 0x7000_0000 完全合法过 loader/exec
    //    既有界检）此前会走到 vmar_map 才 Overlap 拒 → 自陷杀调用方
    //    （svcmgr 死 = watchdog 错杀 panic 连坐 halt）。前置 Err 降级走
    //    eco_skip 面；窗口幅 get_size 失败仍 trap（boot 授能契约，非输入面）。
    let mut mmio_len = 0u64;
    if let Some(mh) = *mmio_vmo {
        if berry_vmo_get_size(mh, &mut mmio_len) != 0 || mmio_len == 0 {
            trap(); // 窗口几何是 boot 授能契约——查不到即链断
        }
        let hit = match VIRTIO_MMIO_VA.checked_add(mmio_len) {
            Some(me) => base < me && VIRTIO_MMIO_VA < stack_end.unwrap_or(u64::MAX),
            None => true, // 窗口幅越界溢出：同族保守降级
        };
        if hit {
            return Err(());
        }
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
            if berry_vmo_read(h_rootfs, pump.as_mut_ptr(), data_off + seg.file_off + off, n) != 0 {
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
    // —— MMIO 物理窗口（P6 A-2）：boot 授能的 virtio-mmio VMO 映到固定 VA
    //    （高带定值；与映像/栈带的重叠已在 process_create 前置 Err 降级——
    //    rootfs 输入面纪律，栈顶前置同族）。rights 恰 READ|WRITE（v1
    //    vmar_map 唯一档）。map 不 take 句柄（调用方自留）；子进程只拿
    //    映射、不拿 handle——设备窗口能力不出 svcmgr 表。窗口幅 mmio_len
    //    来自前置 get_size（boot 授能契约面）。
    if let Some(mh) = *mmio_vmo {
        let mut maddr = VIRTIO_MMIO_VA as usize;
        if berry_vmar_map(hv, 0, mh, 0, &mut maddr, mmio_len as usize, RIGHT_READ | RIGHT_WRITE) != 0
            || maddr != VIRTIO_MMIO_VA as usize
        {
            trap();
        }
    }
    let mut ht = 0 as Handle;
    if berry_thread_create(hp, 0, &mut ht) != 0 || ht == 0 {
        trap();
    }
    // —— env 块（start 前压队，确定性先行）：argv = argv0\0[+arg\0]；handles
    //    按位 [stdout 端别名, (stdin 端), (extra_gift)]（先 dup stdout：
    //    send 会 take 走句柄，原值留调用方——G-eco-b 交接消息同款）。
    let (env_s, env_c) = channel_pair();
    let mut gift = [0 as Handle; 4];
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
    assert!(extra_gifts.len() <= 2, "spawn: extra_gifts ≤ 2（gift 数组容量 4）");
    for (i, g) in extra_gifts.iter().enumerate() {
        gift[2 + i] = *g; // 被 send take（迁移语义同上）
    }
    if !extra_gifts.is_empty() {
        // gift[2] 前要求 gift[1] 已在位（0 空位随行会被 send 预检 -EBADF 自陷
        // ——运行时断言拦调用面漂移，老注释的纪律检升级）。
        assert!(
            stdin_pair.is_some() || stdin_handle.is_some(),
            "spawn: extra_gifts 需 stdin 在位"
        );
        nhandles = nhandles.max(2 + extra_gifts.len());
    }
    let mut blob = [0u8; 64];
    let n = name.len().min(63);
    blob[..n].copy_from_slice(&name[..n]);
    let mut plen = n + 1;
    if let Some(arg) = extra_arg {
        let m = arg.len().min(63 - plen);
        blob[plen..plen + m].copy_from_slice(&arg[..m]);
        plen += m + 1;
    }
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
        vmo,
        svmo,
        img_base: base,
        img_len: len,
        stack_va,
        stack_len,
        mmio_base: VIRTIO_MMIO_VA,
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
