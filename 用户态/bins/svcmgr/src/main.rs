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
/// hexyl 终验基准（d-3c）：上游 0.17.0（`用户态/bins/hexyl` 逐字内嵌）在
/// 宿主以 `--color=never` 管道跑同样 256B 输入的**逐字节输出快照**（版本随
/// Cargo.toml 钉死——升版须重产此文件，比对不等即 FAIL 是故意的）。
const HEXYL_PLAIN: &[u8] = include_bytes!("hexyl-plain.bin");

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

    // ---- G-eco-c/d：rootfs VMO 消费（流式步进 → ELF 泵送 → rootfs 程序）----
    // 裸跑（just run / smoke 相）无 rootfs：get_size -EBADF → 静默跳过，
    // 上述 4-5b/4-6 契约零扰动；带 rootfs（just run-rootfs / smoke rootfs
    // 相）→ hello + stdhello 各自拉起（`ECO ok` 第 24 标记 / 子自打
    // `STD ok`、`STD err ok`——G-eco-d d-1c）。
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

// ---- G-eco-c/d：rootfs 消费（流式步进 + 段泵送——d-1b 纯函数面的消费端） ----

/// rootfs VMO 定值 = boot 第 2 插（main.rs `assert_eq!` 同源硬编——
/// console endU 首插 0x0001_0001 之后）。rights 仅 READ（vmo_read/get_size 面）。
const H_ROOTFS: Handle = 0x0001_0002;

/// cpio 步进窗：头 110B + 名（mkcpio 名 ≤ 56B 绰绰有余，盖不住 = 诚实拒）。
static mut STEP_WIN: [u8; 512] = [0; 512];
/// ELF 首窗：ehdr + 全部 phdr（真实 lld 产物 phnum ≤ 10 量级，4KiB 绰绰）。
static mut ELF_WIN: [u8; 4096] = [0; 4096];
/// 段字节泵窗：rootfs VMO → 子 VMO 的 4KiB 中转（std 镜像 ~283KB ≈ 70 泵）。
static mut PUMP_WIN: [u8; 4096] = [0; 4096];
/// hexyl 终验收齐窗：子 stdout 全量（基准 2156B + 版本漂移余量）。
static mut HEX_WIN: [u8; 4096] = [0; 4096];

/// ELF 映幅帽 = 内核 `VMO_MAX_SIZE`（16MiB）同值（复盘雷 8 残留面）：
/// `elf_head` 只挡 >2^39 野值，(16MiB, 2^39) 区间的合法巨段会走到
/// vmo_create 才 -ENOMEM → 自陷——span 属 rootfs **输入面**（非内核契约
/// 面），按解析侧同款契约 Err 降级；真实 std 镜像 ~283KB，余量充足。
const ELF_MAP_MAX: u64 = 16 * 1024 * 1024;

/// rootfs 全链（G-eco-d d-1c 起流式）：get_size（裸跑 -EBADF 静默跳过）→
/// `cpio_step` 步进遍历（头+名进窗、数据按偏移算术跳过——**归档永不整档
/// 驻留**，旧 16KiB 整档帽随 std 镜像 ~283KB 一并退役）→ 命中 `bin/hello`
/// / `bin/stdhello`/`bin/hexyl` 各自流式拉起。hello 后打 `ECO ok`（smoke
/// 第 24 标记）；stdhello 自打 `STD ok`/`STD err ok`（d-1c 接线）；hexyl
/// 走私有 stdout 终验后打 `HEXYL ok`（d-3c，见 `verify_hexyl`）。
/// 有 rootfs 而链断 = 打一行原因后跳过（可见降级，不静默吞）。
fn run_rootfs_child(console: Handle) {
    let mut cap = 0u64;
    if berry_vmo_get_size(H_ROOTFS, &mut cap) != 0 {
        return; // 裸跑：vmo=none → -EBADF，零扰动
    }
    let eco_skip = |why: &[u8]| {
        let mut line = [0u8; 24];
        line[..4].copy_from_slice(b"eco:");
        line[4] = b' ';
        let n = 5 + why.len();
        line[5..n].copy_from_slice(why);
        line[n] = b'\n';
        send_spin(console, line.as_ptr(), n + 1);
    };
    // SAFETY: boot 后本进程独占步进窗；顺序写/读，单线程（G-eco-c 同款口径）。
    let step: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(STEP_WIN) as *mut u8, 512)
    };
    let mut pos = 0u64;
    while pos < cap {
        let want = ((cap - pos).min(step.len() as u64)) as usize;
        if berry_vmo_read(H_ROOTFS, step.as_mut_ptr(), pos, want) != 0 {
            eco_skip(b"vmo_read failed");
            return;
        }
        match berry_loader::cpio_step(&step[..want], pos, cap) {
            Ok(berry_loader::StepOutcome::End) => break,
            Ok(berry_loader::StepOutcome::Entry(s)) => {
                let (name, data_off, data_len, next) = (s.name, s.data_off, s.data_len, s.next_off);
                // 名字从步进窗借引用：下一轮 cpio_step 前用完（spawn 内的
                // env 发送即消费点，之后只碰 ELF_WIN/PUMP_WIN）。
                if name == "bin/hello" {
                    match spawn_rootfs_elf(name.as_bytes(), data_off, data_len, console, 1, None, None) {
                        Ok(()) => send_spin(console, ECO_OK.as_ptr(), ECO_OK.len()),
                        Err(()) => eco_skip(b"elf bad"),
                    }
                } else if name == "bin/stdhello" {
                    // std 栈 8 页：std::fmt/environ 机制栈深超平像程序。
                    // stdin 面（d-3b）：feeder 泵 64B 确定性 pattern + EOF。
                    const STDIN_DATA: [u8; 64] =
                        *b"berryos!berryos!berryos!berryos!berryos!berryos!berryos!berryos!";
                    let stdin: &[u8] = &STDIN_DATA;
                    if spawn_rootfs_elf(
                        name.as_bytes(),
                        data_off,
                        data_len,
                        console,
                        8,
                        Some(stdin),
                        None,
                    )
                    .is_err()
                    {
                        eco_skip(b"elf bad");
                    }
                } else if name == "bin/hexyl" {
                    // d-3c：D4 路线①最终验证——上游 main.rs 逐字未动的生态
                    // 程序。argv 追加 `--color=never`（0.17 默认 Color=Always
                    // 会打 ANSI 码；plain 输出 = 宿主同版管道对照可逐字比）
                    // ——clap 真解析面，多 argv 首例。stdin = 0x00..=0xff
                    // 全字节 pattern（256B 恰一条消息 = inline 帽边界活证）。
                    // 栈 32 页：clap/Once/fmt 链实测深 ≈64KB——8 页栈用穿后
                    // **下行栈静默扎进自己映像 RW 页**，首中 0x30a000 页的
                    // `__aarch64_have_lse_atomics` 翻非零 → cortex-a72（无
                    // LSE）上 `casa` = UNDEFINED 自陷（d-3c 排障实录）。
                    // musl 默认 128KiB 栈同此口径。
                    const HEXYL_STDIN: [u8; 256] = {
                        let mut a = [0u8; 256];
                        let mut i = 0;
                        while i < 256 {
                            a[i] = i as u8;
                            i += 1;
                        }
                        a
                    };
                    let stdin: &[u8] = &HEXYL_STDIN;
                    // 私有 stdout 终验通道（d-3c）：fd1/2 交接本对 `out_c` 而
                    // 非共享 console——稳态矩阵的打印会在 192B 分片边界间插花
                    // （实跑：0xa0 行被 A/B 循环 + `DEMO ok` 撕开），对表行做
                    // grep 标记 = 时序竞速；svcmgr 收齐全量逐字节比对基准后打
                    // **单消息原子** `HEXYL ok`（每 boot 确定性成立）。
                    let (out_s, out_c) = channel_pair();
                    if spawn_rootfs_elf(
                        name.as_bytes(),
                        data_off,
                        data_len,
                        out_c,
                        32,
                        Some(stdin),
                        Some(b"--color=never"),
                    )
                    .is_err()
                    {
                        eco_skip(b"elf bad");
                    } else {
                        verify_hexyl(out_s, console);
                    }
                    // out_c 原值：子表已持 SEND 别名（dup 迁移），本表副本使命已毕
                    if berry_handle_close(out_c) != 0 {
                        trap();
                    }
                    if berry_handle_close(out_s) != 0 {
                        trap();
                    }
                }
                pos = next;
            }
            Err(_) => {
                eco_skip(b"cpio bad");
                return;
            }
        }
    }
}

/// smoke 第 24 标记（7 字节，尾插随合并序）。
const ECO_OK: &[u8; 7] = b"ECO ok\n";

/// rootfs ELF 流式拉起（页铺平单 VMO，同 `工具链/elf2flat.py` 语义）：
/// ELF 首窗 `elf_head` 解析（**跨度按 p_memsz**——std RW 段 memsz 0x41241
/// = 静态堆 + .bss 计入；入口入映像检在装载端拦）→ 子 VMO
/// `[floor页(min), +ceil页(span))` 一次 commit/map → 各段文件字节按 4KiB
/// 泵窗从 rootfs VMO 直送子 VMO 相对偏移（**ELF 本体永不整档驻留**——
/// stdhello ~283KB 只经 4KiB 中转）→ 栈页映其上（页数按程序面）→
/// **env 块**（d-3a，BerryOS processargs v1）：channel 建 env 对 →
/// `send(env_s, argv0\0, [stdout 端别名, (stdin 端)])`（能力随消息迁移）→
/// `process_start(bootstrap=env_c)`——子进程入口 x0 = env 端。`stdin`
/// = Some(data) 时再建一对，读端随 env 块迁移、`process_start` 后本进程
/// 作 feeder 泵数据 + 零长 EOF 标记（libc `read` 的 EOF 约定——队列深 8，
/// 2 条无阻塞面）。
/// 解析/首窗读失败返 Err（调用方可见降级续走归档）；syscall 面失败自陷
/// ——链断 loud（标记缺失兜底）。
///
/// `extra_arg`（d-3c）：argv[1] 追加项（None = argv 只有 argv0）——env 块
/// payload 即 `argv0\0[extra\0]`，libc 侧 NUL 分隔逐串入 argv；hexyl 的
/// `--color=never` 走这条（clap 真解析面）。
///
/// `stdout_end`（d-3c）：随 env 块交给子进程当 fd1/2 的通道端（dup 成
/// RIGHT_SEND）——传 console 发端 = 子直打串口（hello/stdhello）；传私有
/// 对端 = svcmgr 侧收齐终验（hexyl，见 `verify_hexyl`）。
fn spawn_rootfs_elf(
    name: &[u8],
    data_off: u64,
    data_len: u64,
    stdout_end: Handle,
    stack_pages: u64,
    stdin: Option<&[u8]>,
    extra_arg: Option<&[u8]>,
) -> Result<(), ()> {
    let want = (data_len.min(4096u64)) as usize;
    // SAFETY: 独占首窗；boot 后单线程顺序读写。
    let win: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(ELF_WIN) as *mut u8, 4096)
    };
    if berry_vmo_read(H_ROOTFS, win.as_mut_ptr(), data_off, want) != 0 {
        return Err(());
    }
    let h = match berry_loader::elf_head(&win[..want], data_len) {
        Ok(h) => h,
        Err(_) => return Err(()), // 截断/野值/不收面：可见降级，不杀 svcmgr
    };
    let base = berry_loader::page_floor(h.image_min);
    let len = berry_loader::page_ceil(h.image_span);
    if len > ELF_MAP_MAX {
        return Err(()); // 段幅前置（雷 8）：巨段在几何层即降级，不等到 vmo_create
    }
    let mut hp = 0 as Handle;
    let mut hv = 0 as Handle;
    if berry_process_create(0, 0, &mut hp, &mut hv) != 0 || hp == 0 || hv == 0 {
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
    // SAFETY: 独占泵窗（同上）。
    let pump: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(PUMP_WIN) as *mut u8, 4096)
    };
    for seg in h.segments() {
        let mut off = 0u64;
        while off < seg.filesz {
            let n = ((seg.filesz - off).min(pump.len() as u64)) as usize;
            if berry_vmo_read(H_ROOTFS, pump.as_mut_ptr(), data_off + seg.file_off + off, n) != 0 {
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
    let mut ht = 0 as Handle;
    if berry_thread_create(hp, 0, &mut ht) != 0 || ht == 0 {
        trap();
    }
    // —— env 块（start 前压队，确定性先行）：argv0 = cpio 名 + NUL（+ 可选
    //    argv1）；handles = [stdout 端别名, (stdin 端)]（先复制 stdout 端：
    //    send 会 take 走句柄，原值留调用方——G-eco-b 交接消息同款）。
    let (env_s, env_c) = channel_pair();
    let mut gift = [0 as Handle; 2];
    if berry_handle_duplicate(stdout_end, RIGHT_SEND, &mut gift[0]) != 0 {
        trap();
    }
    let stdin_pair = stdin.map(|_| channel_pair());
    if let Some((_, in_c)) = stdin_pair {
        gift[1] = in_c; // stdin 读端随 env 块迁移（handle 数组按位语义）
    }
    let nhandles = if stdin_pair.is_some() { 2 } else { 1 };
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
    // —— feeder：数据 + 零长 EOF 标记（libc read 的 EOF 约定，d-3b）。
    if let (Some((in_s, _)), Some(data)) = (stdin_pair, stdin) {
        if berry_channel_send(in_s, data.as_ptr(), data.len(), core::ptr::null(), 0) != 0 {
            trap();
        }
        if berry_channel_send(in_s, data.as_ptr(), 0, core::ptr::null(), 0) != 0 {
            trap();
        }
        if berry_handle_close(in_s) != 0 {
            trap();
        }
    }
    Ok(())
}

/// hexyl 终验（d-3c，D4 路线①终验）：收齐子 stdout 全量 → 与宿主基准
/// `HEXYL_PLAIN` 逐字节比对 → 全等才打 `HEXYL ok`（smoke rootfs 相标记）。
/// **EOF = libc `park` 前的零长消息**（d-3b 约定的写侧对称面）——确定性
/// 终点，不赌超时；10s **活动帽**（每次收包刷新）兜底 abort/断链路径
/// （无 EOF → 打可见降级行，标记缺失 = smoke FAIL）。
/// 比对通过后全表回放 console（人类可读面；与稳态矩阵插花无碍——已非
/// 标记）。
fn verify_hexyl(h: Handle, console: Handle) {
    // SAFETY: svcmgr 单线程独占（同 ELF_WIN/PUMP_WIN 纪律）。
    let cap: &mut [u8] =
        unsafe { core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(HEX_WIN) as *mut u8, 4096) };
    let mut got = 0usize;
    let mut msg = [0u8; 256];
    let mut eof = false;
    while !eof {
        let mut nb = msg.len();
        let mut nh = 0usize;
        let s = berry_channel_recv(h, msg.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == 0 {
            if nb == 0 {
                eof = true;
            } else {
                if got + nb > cap.len() {
                    send_spin(console, b"hexyl overflow\n".as_ptr(), 15);
                    return; // 超收 = 输出面已不等，比对无意义
                }
                cap[got..got + nb].copy_from_slice(&msg[..nb]);
                got += nb;
            }
            continue;
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        // 空队：阻塞等 READABLE（10s 活动帽，收包刷新——慢而不僵不误杀）
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_READABLE, now() + TIMEOUT_NS, &mut obs) != 0 {
            send_spin(console, b"hexyl: no EOF\n".as_ptr(), 14);
            return; // EOF 永不来 = 子进程异常路径（abort 无 EOF）——loud 降级
        }
    }
    if got == HEXYL_PLAIN.len() && cap[..got] == HEXYL_PLAIN[..got] {
        send_spin(console, b"HEXYL ok\n".as_ptr(), 9);
    } else {
        // 首差诊断行（hex4 两数：got/want 长度；逐字节定位交给离线复盘）
        let mut line = *b"hexyl mismatch got=0000 want=0000\n";
        put_hex4(&mut line[19..23], got);
        put_hex4(&mut line[30..34], HEXYL_PLAIN.len());
        send_spin(console, line.as_ptr(), line.len());
        return; // 不回放错表（错表 + 回放 = 两个混淆源）
    }
    // 全表回放（12×192B 分片，与稳态矩阵插花无碍——终验已过，纯展示）
    let mut off = 0usize;
    while off < got {
        let n = core::cmp::min(192, got - off);
        // SAFETY: cap 只读视图的分片指针。
        send_spin(console, unsafe { cap.as_ptr().add(off) }, n);
        off += n;
    }
}

/// 4 位十六进制落字（got/want 诊断行用；值 < 0x10000 由 4KiB 窗保证）。
fn put_hex4(out: &mut [u8], v: usize) {
    for i in 0..4 {
        out[i] = b"0123456789abcdef"[(v >> (4 * (3 - i))) & 0xf];
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
