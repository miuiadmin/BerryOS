//! gui-wmd —— GUI-5（ADR #61）窗口管理守护：呈现（guifbd）/窗口策略
//! （本服务）分离。guifbd 零改动；gfxdemo 改连 wm 域（同 op 换端——
//! present/hdi 六+四 op 经本服务透传中继，wm 六 op 本地权威处理）；
//! inputd 追加推送端收窗口表快照（WM_TABLE_WAIT 阻塞应答形）。
//!
//! **boot 授能链**：EFI 相 svcmgr guifbd arm Ok 后 wmd arm 预拉本服务
//! （门 = present_cli≠0 ∧ cpio 命中），extras 4 枚 = [wm_srv（gfxdemo
//! 客户端面）, push_srv（inputd 客户端面）, present_cli, hdi_cli（guifbd
//! 客户端端换手件）] 加 base 4 恰 8/8（§7 Q12 冻结）。rootfs 相（-kernel
//! 无 fb）svcmgr 探测失败不 spawn——本服务恒只在 EFI 链存活。
//!
//! **启动序五步**（§3 #4）：①收割四端（0 值 = 协议破缺面：eprintln +
//! park 挂起——**非 exit(0)**，svcmgr 已 ps_reg ALIVE，真退出无终态回写
//! 路径 = 永久 ALIVE 谎账；标记缺失 = smoke FAIL loud 兜底）→ ②present
//! 首发 RELEASE_WAIT 清前世孤儿别名（wmd 崩溃重启残留：guifbd held 表
//! 持前世在途别名 credits 永占；新 wmd 首发 ReleaseWait 全量清账，guifbd
//! :474-496 语义零 guifbd 改动；boot 期 guifbd 空转即答 = 无长 park 面）
//! → ③present GETINFO 缓存几何（真值源恒 guifbd——wmd 不持第二份 present
//! 常量副本，Q15）→ ④表初始化空表 version≥1 → ⑤`gui-wmd: up` 诊断行
//! （小写 wmd 前缀防误配 `WM ok` 标记——本行永不进 MARKERS）转 serve。
//!
//! **serve 两路 poll-first**：wm_srv→push_srv 确定性轮询（一路至多一条：
//! 两客户端面各自严格串行 RPC），全 EAGAIN 才 `berry_object_wait_many`
//! n=2 并等；deadline = now+10s **进调用本身**（guifbd 三路 serve 同款
//! ——醒来后补查不够）；到点 = 合法空闲重臂续等（真断链面 = recv 非
//! -EAGAIN 错码照 trap）。每轮 poll 前重置 nb/nh 容量（recv 出参 in-out，
//! 漏重置 = 恒 -EINVAL 死循环）。
//!
//! **新鲜度结构注记（§7 Q10）**：单 wm 客户端串行下中继期间表不可能变 =
//! 零新鲜度损失——serve 单线程，中继透传（present/hdi RPC）阻塞期间不可
//! 能处理另一 CREATE/SET，TABLE_GET/TABLE_WAIT 应答携带的快照与请求到达
//! 时表态一致；v1 wm 客户端恰一个（gfxdemo，严格串行 RPC 更无并发两请求）。
//! **后刀第二 wm 客户端注意**：中继阻塞期间对端可变更表 → TABLE_GET
//! 应答快照与「请求到达时」表态可能有出入（新鲜度窗口 = 一次中继时长），
//! 届时须评估 per-request 快照戳或拆线程，勿直接加客户端。

use berry_guiproto as gp;
use berry_libc::io::{wm_hdi_h, wm_present_h, wm_push_srv_h, wm_srv_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_object_wait_many, berry_object_wait_one, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, SIGNAL_READABLE, SIGNAL_WRITABLE,
};

mod relay;
mod table;

// 链接种子（guifbd 同款）：rlib/LTO 按需保留——无引用则 berry-libc 的 crt0
// （含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// serve 空闲并等帽：wait_many deadline 外层上界（合法空闲非死线——到点
/// 重臂续等；guifbd `IDLE_CAP_NS` 同款）。
const WMD_IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// 单调钟现值（失败 = clock 面断链，loud）。
pub(crate) fn now_ns() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
}

/// 应答 send（无随行 handle 面；EBUSY → wait WRITABLE 重臂重验）。与
/// guifbd 无限重臂不同：**总帽 10s 到点 trap**（§3 #6 失败面——「持续
/// EBUSY = trap」，对端不排空 10s = 链病态，不留无限空转面）。
pub(crate) fn send_rep(h: Handle, rep: &[u8]) {
    let deadline = now_ns() + WMD_IDLE_CAP_NS;
    loop {
        let s = berry_channel_send(h, rep.as_ptr(), rep.len(), core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            println!("gui-wmd: send 失败 {}", s);
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            println!("gui-wmd: send busy 帽到点");
            trap(); // 持续 EBUSY（10s 未排空）= fail loud
        }
        // 命中 WRITABLE → 下一轮 send 重验（obs 不作判据——重验循环兜底）
    }
}

/// 应答 send（随行 handle 面——快照/present 域中继应答：handle 随应答
/// 迁回客户端表）；busy 帽语义同 [`send_rep`]。
pub(crate) fn send_rep_handles(h: Handle, rep: &[u8], hs: &[Handle]) {
    let deadline = now_ns() + WMD_IDLE_CAP_NS;
    loop {
        let s = berry_channel_send(h, rep.as_ptr(), rep.len(), hs.as_ptr(), hs.len());
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            println!("gui-wmd: send 失败 {}", s);
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            println!("gui-wmd: send busy 帽到点");
            trap();
        }
    }
}

/// 1B 状态应答（非 OK 烘焙线 + DESTROY/SET 成功形）。
pub(crate) fn st_rep(h: Handle, st: u8) {
    let mut out = [0u8; 1];
    let n = gp::encode_status_rep(&mut out, st);
    send_rep(h, &out[..n]);
}

/// close 请求随行迁入的全部 handle（拒收公共出口——坏客户端不得杀服务，
/// close 失败忽略）。
pub(crate) fn close_all(hs: &[Handle]) {
    for &h in hs {
        let _ = berry_handle_close(h);
    }
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己
/// （guifbd 同款——链断 loud，标记缺失 = smoke FAIL 兜底）。
#[cold]
pub(crate) fn trap() -> ! {
    // SAFETY: 故意触发的未映射读（0x4000_0000 与用户布局无交集——家族惯例值）。
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}

/// wm_srv 帧分发（§3 #6 分发表执法）：present 1..=6 → present_cli 透传；
/// hdi 0x40..=0x43 → hdi_cli 透传；wm 0x60..=0x7F → 本地表处理（六 op
/// 之外 0x66..=0x7F 未定形 → 1B ST_BAD）；**其余一切 op（含 0x20/0x21
/// input 域错投）→ 1B ST_BAD + close 随行**。
fn on_wm(table: &mut table::WmTable, present_cli: Handle, hdi_cli: Handle, ch: Handle,
         msg: &[u8], hs: &[Handle]) {
    match msg.first() {
        Some(&op) if (gp::OP_GETINFO..=gp::OP_POWER_MODE).contains(&op) => {
            relay::relay(ch, present_cli, msg, hs);
        }
        Some(&op) if (gp::OP_REG_VBLANK..=gp::OP_COMMIT).contains(&op) => {
            relay::relay(ch, hdi_cli, msg, hs);
        }
        Some(&op) if (gp::OP_WM_GETINFO..=0x7F).contains(&op) => {
            table.handle_wm(ch, msg, hs);
        }
        _ => {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
        }
    }
}

/// push_srv 帧分发：**只收 TABLE_WAIT**（inputd 单客户端串行 WAIT 面）；
/// 其余帧（含其它 wm op 错投）1B ST_BAD + close 随行。
fn on_push(table: &mut table::WmTable, ch: Handle, msg: &[u8], hs: &[Handle]) {
    if !hs.is_empty() {
        close_all(hs);
        st_rep(ch, gp::ST_BAD);
        return;
    }
    match gp::parse_wm_table_wait_req(msg) {
        Some(last_seen) => table.wait(ch, last_seen),
        None => st_rep(ch, gp::ST_BAD),
    }
}

fn main() {
    // —— 启动序①：收割四端（extras 4 枚恰 8/8——Q12 冻结面）。0 值 =
    // 协议破缺（EFI 链 svcmgr arm Ok 恒随行）：eprintln + park 兜底。
    let wm_srv = wm_srv_h();
    if wm_srv == 0 {
        eprintln!("gui-wmd: no wm srv gift");
        berry_libc::park();
    }
    let push_srv = wm_push_srv_h();
    if push_srv == 0 {
        eprintln!("gui-wmd: no push srv gift");
        berry_libc::park();
    }
    let present_cli = wm_present_h();
    if present_cli == 0 {
        eprintln!("gui-wmd: no present cli gift");
        berry_libc::park();
    }
    let hdi_cli = wm_hdi_h();
    if hdi_cli == 0 {
        eprintln!("gui-wmd: no hdi cli gift");
        berry_libc::park();
    }

    // —— 启动序②：present 首发 RELEASE_WAIT 清前世孤儿别名（guifbd
    // boot 期空转即答——无长 park 边界问题；应答随行归还别名全量即关）。
    relay::boot_release_all(present_cli);

    // —— 启动序③：present GETINFO 缓存几何（快照头/wm GETINFO 的
    // disp_w/disp_h 域同源取值）。
    let info = relay::boot_getinfo(present_cli);
    // wm wire 几何域 = u16——表示不了的屏 = 协议破缺面（宁可不起）。
    if info.display_w > u16::MAX as u32 || info.display_h > u16::MAX as u32 {
        eprintln!("gui-wmd: display {}x{} over u16", info.display_w, info.display_h);
        berry_libc::park();
    }

    // —— 启动序④：表初始化（空表 + version≥1：counter 起 1 → 冷启动
    // WAIT(0) 恒立即答；epoch = 启动 CNTVCT 派生 u8——无持久化下唯一跨
    // 启动区分源）。
    let mut table = table::WmTable::new(info.display_w as u16, info.display_h as u16);

    // —— 启动序⑤：诊断行转 serve（小写 wmd 前缀——永不进 MARKERS）。
    println!("gui-wmd: up");

    // —— serve 两路 poll-first：wm_srv→push_srv 确定性轮询；全 EAGAIN
    // 才 wait_many n=2 并等（deadline 进调用本身）。命中/到点皆回环顶
    // 两路重询（poll-first 无丢唤醒）。
    loop {
        if let Some((msg, hs)) = poll(wm_srv) {
            on_wm(&mut table, present_cli, hdi_cli, wm_srv, &msg.0[..msg.1], &hs.0[..hs.1]);
            continue;
        }
        if let Some((msg, hs)) = poll(push_srv) {
            on_push(&mut table, push_srv, &msg.0[..msg.1], &hs.0[..hs.1]);
            continue;
        }
        let hs = [wm_srv, push_srv];
        let sigs = [SIGNAL_READABLE, SIGNAL_READABLE];
        let mut idx = 0usize;
        let mut obs = 0u64;
        let s = berry_object_wait_many(
            hs.as_ptr(),
            sigs.as_ptr(),
            2,
            now_ns() + WMD_IDLE_CAP_NS,
            &mut idx,
            &mut obs,
        );
        if s != 0 && s != BERRY_ERR_TIMEDOUT {
            trap(); // 到点 = 空闲重臂（回环顶兜底）；余错 = 死亡面
        }
    }
}

/// 单路轮询（一路至多一条：两客户端面各自严格串行 RPC）。**每轮 poll 前
/// 重置 nb/nh 容量**（recv 出参 in-out，漏重置 = 容量 0 恒 -EINVAL 死
/// 循环）；出参 8 槽（wm 域零随行——非空即拒面 close；中继域随行透传）。
fn poll(ch: Handle) -> Option<(([u8; 256], usize), ([Handle; 8], usize))> {
    let mut msg = [0u8; 256];
    let mut hout = [0u32; 8];
    let mut nb = msg.len();
    let mut nh = hout.len();
    let s = berry_channel_recv(ch, msg.as_mut_ptr(), &mut nb, hout.as_mut_ptr(), &mut nh);
    if s == BERRY_ERR_AGAIN {
        return None;
    }
    if s != 0 {
        trap(); // 非 -EAGAIN 错码 = 通道链断（boot 定值接线，真死亡面）
    }
    Some(((msg, nb), (hout, nh)))
}
