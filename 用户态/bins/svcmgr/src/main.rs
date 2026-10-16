//! G-eco-b——svcmgr 服务管理器 Rust 化（替换 `内核/src/user.rs` 的 svcmgr
//! asm 模板，943-1663 行整块退役）。契约与 asm 逐字等价（4-5b + 4-6 A 半
//! 验收链，smoke 第 21/23 标记不许降级）：
//!
//! - B 半：拉起 echo 服务（process_create 双出参 → 2 页 VMO/commit/write
//!   内嵌 echo 镜像 → vmar_map@0x1000 → channel_create → thread_create →
//!   process_start(bootstrap=end1)——close 原值必 -EBADF 迁移自证）→
//!   ECHO_REQ 往返验回显（低 32 位）→ `SVC ok`；
//! - A 半：直线拉起 talker + echo2（同一份 echo 镜像第二实例），交接消息
//!   一次迁移双 handle（console 别名 + t2 对端——能力经消息授权，不经
//!   全局名）→ talker 打 `DEMO ok`；
//! - 稳态：节流 + ECHO_REQ 实证往返 + 'v'。
//!
//! 子程序镜像 `include_bytes!` 内嵌（两遍构建：leaf bins 先产平像再编本
//! crate，见 justfile `build-user`；改子程序须先重跑 pass 1）。入口 x0 =
//! console 发端（boot 首插定值 0x0001_0001）。任何链断 → 自陷被杀 →
//! 标记缺失 = smoke FAIL 兜底。

#![no_std]
#![no_main]

use berry_sys::{
    berry_channel_create, berry_channel_recv, berry_channel_send, berry_clock_read,
    berry_handle_close, berry_handle_duplicate, berry_object_wait_one, berry_process_create,
    berry_process_start, berry_thread_create, berry_vmar_map, berry_vmo_commit, berry_vmo_create,
    berry_vmo_get_size, berry_vmo_read, berry_vmo_write, Handle, BERRY_ERR_AGAIN,
    BERRY_ERR_BAD_HANDLE, BERRY_ERR_BUSY, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_SEND, RIGHT_WRITE,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};

/// echo 服务镜像（G-eco-b 编译产物；echo1/echo2 共用同一份字节——asm 同款）。
const ECHO_BIN: &[u8] = include_bytes!("../../../userbins/echo.bin");
/// talker 镜像。
const TALKER_BIN: &[u8] = include_bytes!("../../../userbins/talker.bin");

/// ECHO_REQ（4-5b 验收载荷；回显比对取低 32 位 0xE000_0E30）。
const ECHO_REQ: u64 = 0x00C0_FFEE_E000_0E30;
/// smoke 第 21 标记（7 字节）。
const SVC_OK: &[u8; 7] = b"SVC ok\n";

const TIMEOUT_NS: u64 = 10 * 1_000_000_000;
const THROTTLE: u64 = 0x30_0000;
/// 子进程布局：镜像首页 @0x1000 + 零栈页，映射 [0x1000,0x3000)，sp0=0x3000。
const CHILD_MAP_LEN: usize = 0x2000;
const CHILD_ENTRY: usize = 0x1000;
const CHILD_SP: usize = 0x3000;

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(console: Handle) -> ! {
    // ---- B 半：echo 服务 + ECHO_REQ 往返 + `SVC ok` ----
    let (end0, end1) = channel_pair();
    spawn_child(ECHO_BIN, end1);
    let mut buf = [0u8; 8];
    let req = ECHO_REQ.to_le_bytes();
    send_retry(end0, req.as_ptr(), 8);
    roundtrip(end0, &mut buf, &req);
    send_spin(console, SVC_OK.as_ptr(), 7);

    // ---- A 半：talker + echo2，交接双 handle ----
    let (s1, t1) = channel_pair();
    spawn_child(TALKER_BIN, t1);
    let (t2, e2) = channel_pair();
    spawn_child(ECHO_BIN, e2);
    let mut gift = [0 as Handle; 2];
    if berry_handle_duplicate(console, RIGHT_SEND, &mut gift[0]) != 0 {
        trap(); // 先复制再送：send 会 take 走句柄，原值须留给稳态
    }
    gift[1] = t2; // echo2 对端随消息交付 talker
    let payload = 0u64; // 纯载体，talker 不看内容
    if berry_channel_send(s1, payload.to_le_bytes().as_ptr(), 8, gift.as_ptr(), 2) != 0 {
        trap(); // 交接件未入队 = A 半链断
    }

    // ---- G-eco-c：rootfs VMO 消费（cpio → ELF → 首个 rootfs 程序）----
    // 裸跑（just run / smoke 相）无 rootfs：get_size -EBADF → 静默跳过，
    // 上述 4-5b/4-6 契约零扰动；带 rootfs（just run-rootfs / smoke rootfs
    // 相）→ hello 拉起 + `ECO ok`（smoke 第 24 标记）。
    run_rootfs_child(console);

    // ---- 稳态：节流 + 实证往返 + 'v'（每个 'v' = 一次 ECHO_REQ 往返）----
    let v = [b'v'];
    loop {
        throttle();
        send_retry(end0, req.as_ptr(), 8);
        roundtrip(end0, &mut buf, &req);
        send_spin(console, v.as_ptr(), 1);
    }
}

/// 建通道对（两端均授 SEND|RECV|DUPLICATE；返回 (本端, 交接端)）。
fn channel_pair() -> (Handle, Handle) {
    let mut a = 0 as Handle;
    let mut b = 0 as Handle;
    if berry_channel_create(0, &mut a, &mut b) != 0 {
        trap();
    }
    (a, b)
}

/// 拉起一个子进程：process_create 双出参 → 2 页 VMO（镜像首页 + 零栈页）
/// → vmar_map@0x1000（rights 恰 READ|WRITE）→ thread_create →
/// process_start(entry=0x1000, sp=0x3000, bootstrap)——bootstrap 随 start
/// 迁入子表（子入口 x0 = 子表新值），close 原值必 -EBADF（迁移自证）。
fn spawn_child(image: &[u8], bootstrap: Handle) {
    let mut hp = 0 as Handle;
    let mut hv = 0 as Handle;
    if berry_process_create(0, 0, &mut hp, &mut hv) != 0 || hp == 0 || hv == 0 {
        trap();
    }
    let mut vmo = 0 as Handle;
    if berry_vmo_create(CHILD_MAP_LEN as u64, 0, &mut vmo) != 0 || vmo == 0 {
        trap();
    }
    if berry_vmo_commit(vmo, 0, CHILD_MAP_LEN as u64) != 0 {
        trap();
    }
    if berry_vmo_write(vmo, image.as_ptr(), 0, image.len()) != 0 {
        trap();
    }
    let mut vaddr: usize = CHILD_ENTRY; // 映射基址提示（in-out 槽预写）
    if berry_vmar_map(hv, 0, vmo, 0, &mut vaddr, CHILD_MAP_LEN, RIGHT_READ | RIGHT_WRITE) != 0 {
        trap();
    }
    let mut ht = 0 as Handle;
    if berry_thread_create(hp, 0, &mut ht) != 0 || ht == 0 {
        trap();
    }
    if berry_process_start(hp, ht, CHILD_ENTRY, CHILD_SP, bootstrap) != 0 {
        trap();
    }
    if berry_handle_close(bootstrap) != BERRY_ERR_BAD_HANDLE {
        trap(); // 迁移自证：原值必须已作废
    }
}

// ---- G-eco-c：rootfs 消费（草案 G-eco-c 行）----

/// rootfs VMO 定值 = boot 第 2 插（main.rs `assert_eq!` 同源硬编——
/// console endU 首插 0x0001_0001 之后）。rights 仅 READ（vmo_read/get_size 面）。
const H_ROOTFS: Handle = 0x0001_0002;
/// rootfs 暂存帽：cpio 一次读入 .bss 静态缓冲（svcmgr 平像 +16KiB 零页，
/// 仍在 USER_CODE_MAX=64KiB 内）。v1 单程序 rootfs 量级；越帽 = 诚实跳过。
const ROOTFS_MAX: usize = 16 * 1024;
static mut ROOTFS_BUF: [u8; ROOTFS_MAX] = [0; ROOTFS_MAX];

/// rootfs 全链：get_size（裸跑 -EBADF 静默跳过）→ vmo_read 整段入 .bss →
/// cpio walk 找 `bin/hello` → berry-loader ELF 解析 → spawn_elf（console
/// 别名作 bootstrap）→ `ECO ok`。有 rootfs 而链断 = 打一行原因后跳过
/// （可见降级，不静默吞——AI 测试友好化）。
fn run_rootfs_child(console: Handle) {
    let mut cap = 0u64;
    if berry_vmo_get_size(H_ROOTFS, &mut cap) != 0 {
        return; // 裸跑：vmo=none → -EBADF，零扰动
    }
    let cap = cap as usize;
    // SAFETY: boot 后本进程独占 ROOTFS_BUF；写在前、共享引用在后，单线程。
    let buf: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(ROOTFS_BUF) as *mut u8, ROOTFS_MAX)
    };
    let eco_skip = |why: &[u8]| {
        let mut line = [0u8; 24];
        line[..4].copy_from_slice(b"eco:");
        line[4] = b' ';
        let n = 5 + why.len();
        line[5..n].copy_from_slice(why);
        line[n] = b'\n';
        send_spin(console, line.as_ptr(), n + 1);
    };
    if cap > ROOTFS_MAX {
        eco_skip(b"rootfs too big");
        return;
    }
    if berry_vmo_read(H_ROOTFS, buf.as_mut_ptr(), 0, cap) != 0 {
        eco_skip(b"vmo_read failed");
        return;
    }
    let buf: &[u8] = &buf[..cap]; // cpio 视角（容量含尾零页，walk 被 trailer 截住）
    let mut hello: Option<&[u8]> = None;
    for e in berry_loader::cpio_walk(buf) {
        match e {
            Ok(en) if en.name == "bin/hello" => hello = Some(en.data),
            Ok(_) => {}
            Err(_) => {
                eco_skip(b"cpio bad");
                return;
            }
        }
    }
    let elf = match hello {
        Some(b) => b,
        None => {
            eco_skip(b"no bin/hello");
            return;
        }
    };
    let img = match berry_loader::elf_parse(elf) {
        Ok(i) => i,
        Err(_) => {
            eco_skip(b"elf bad");
            return;
        }
    };
    // console 别名作 bootstrap：hello 凭它自己打 `rootfs hello`（链活证与
    // `ECO ok` 互证——smoke rootfs 相两条都断言）。先复制：send 会 take 走
    // 句柄，console 原值须留给 ECO ok 与稳态 'v'。
    let mut hboot = 0 as Handle;
    if berry_handle_duplicate(console, RIGHT_SEND, &mut hboot) != 0 {
        eco_skip(b"dup failed");
        return;
    }
    spawn_elf(&img, hboot);
    send_spin(console, ECO_OK.as_ptr(), ECO_OK.len());
}

/// smoke 第 24 标记（7 字节，尾插随合并序）。
const ECO_OK: &[u8; 7] = b"ECO ok\n";

/// ELF 拉起（与 spawn_child 的平像路径同构——**页铺平单 VMO**，同
/// `工具链/elf2flat.py` 语义）：真实 ld 的 PT_LOAD 不页对齐（rust-lld 实测
/// RO 段 vaddr=0x10c0 与 text 段共享页），逐段独立映射会撞已映页——
/// 故一枚 VMO 覆盖 `[floor页(min vaddr), ceil页(max end))`，各段文件字节
/// 按相对偏移写入（.bss 尾部由零页覆盖），一次 vmar_map 映整幅，栈页
/// 映其上，entry = e_entry。vaddr in-out 槽回写须 == 请求基址（v1 无重排）。
fn spawn_elf(img: &berry_loader::ElfImage, bootstrap: Handle) {
    use berry_loader::PAGE_SIZE;
    let mut hp = 0 as Handle;
    let mut hv = 0 as Handle;
    if berry_process_create(0, 0, &mut hp, &mut hv) != 0 || hp == 0 || hv == 0 {
        trap();
    }
    // 页铺平几何：跨段并集的单幅映射。
    let lo = img.segments().iter().map(|s| s.vaddr).min().unwrap();
    let hi = img.segments().iter().map(|s| s.vaddr + s.memsz).max().unwrap();
    let base = lo & !(PAGE_SIZE - 1);
    let len = (hi - base + PAGE_SIZE - 1) / PAGE_SIZE * PAGE_SIZE;
    let mut vmo = 0 as Handle;
    if berry_vmo_create(len, 0, &mut vmo) != 0 || vmo == 0 {
        trap();
    }
    if berry_vmo_commit(vmo, 0, len) != 0 {
        trap();
    }
    for seg in img.segments() {
        if !seg.data.is_empty()
            && berry_vmo_write(vmo, seg.data.as_ptr(), (seg.vaddr - base) as u64,
                               seg.data.len()) != 0
        {
            trap();
        }
    }
    let mut vaddr = base as usize; // 映射基址提示（in-out 槽预写）
    if berry_vmar_map(hv, 0, vmo, 0, &mut vaddr, len as usize, RIGHT_READ | RIGHT_WRITE) != 0
        || vaddr != base as usize
    {
        trap();
    }
    // 栈页映在映幅之上。
    let stack_va = base + len;
    let mut svmo = 0 as Handle;
    if berry_vmo_create(PAGE_SIZE, 0, &mut svmo) != 0 || svmo == 0 {
        trap();
    }
    if berry_vmo_commit(svmo, 0, PAGE_SIZE) != 0 {
        trap();
    }
    let mut saddr = stack_va as usize;
    if berry_vmar_map(hv, 0, svmo, 0, &mut saddr, PAGE_SIZE as usize, RIGHT_READ | RIGHT_WRITE)
        != 0
        || saddr != stack_va as usize
    {
        trap();
    }
    let mut ht = 0 as Handle;
    if berry_thread_create(hp, 0, &mut ht) != 0 || ht == 0 {
        trap();
    }
    if berry_process_start(hp, ht, img.entry as usize, saddr + PAGE_SIZE as usize, bootstrap) != 0 {
        trap();
    }
    if berry_handle_close(bootstrap) != BERRY_ERR_BAD_HANDLE {
        trap(); // 迁移自证：原值必须已作废
    }
}

/// 对端回显验收：wait READABLE → recv（-EAGAIN 重等）→ 低 32 位比对。
fn roundtrip(h: Handle, buf: &mut [u8; 8], req: &[u8; 8]) {
    wait(h, SIGNAL_READABLE);
    loop {
        let mut nb = 8usize;
        let mut nh = 0usize;
        let s = berry_channel_recv(h, buf.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        wait(h, SIGNAL_READABLE);
    }
    if buf[..4] != req[..4] {
        trap(); // 回显低 32 位不符（期望 0xE000_0E30）
    }
}

/// 发送（满 -EBUSY → wait WRITABLE 腾位重试；其余自陷）。
fn send_retry(h: Handle, buf: *const u8, len: usize) {
    loop {
        let s = berry_channel_send(h, buf, len, core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            trap();
        }
        wait(h, SIGNAL_WRITABLE);
    }
}

/// console 发送——**自旋重发**（asm 同形：`SVC ok`/稳态 'v' 的 EBUSY 环是
/// `b 6b`/`b 14b` 纯重发、无 wait）。启动期 console 队列饱和窗口内阻塞等
/// WRITABLE 会被自旋友商（abuser/spawner 等）持续抢槽——唤醒后须等一整圈
/// 轮转才拿到片，三场竞速即耗尽 10s 帽（G-eco-b 实踩：SVC ok 从未落串口、
/// echo1 连带饿死）；自旋者由轮转兜底必达。EL0 可抢占 = 自旋有界。
fn send_spin(h: Handle, buf: *const u8, len: usize) {
    loop {
        let s = berry_channel_send(h, buf, len, core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            trap();
        }
    }
}

/// 阻塞等信号（10s 绝对帽；超时/错码自陷——对端断供即链断）。
fn wait(h: Handle, signals: u64) {
    let mut obs = 0u64;
    if berry_object_wait_one(h, signals, now() + TIMEOUT_NS, &mut obs) != 0 {
        trap();
    }
}

/// now = clock_read（失败自陷）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
}

#[inline(never)]
fn throttle() {
    let mut i: u64 = 0;
    while i < THROTTLE {
        core::hint::black_box(&mut i);
        i += 1;
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
