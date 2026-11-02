//! berry-launcher —— GUI-8 桌面 launcher（可先行面 #2，docs/
//! GUI-8实施侦察-2026-11.md §5；规划与侦察文档定名 berry-launcher）。
//! 前置 wiring 刀（gui3）：v0 fbinfo 探测骨架升级 wm 域接线——svcmgr
//! 预拉 arm（紧随 gfxdemo arm）交付 wm 客户端端 dup 柄（extras[0]，
//! libc `wm_cli_h()` 同槽顶替 `gift2_h()`——多产面惯例），本程序以它
//! 为门、发一次 WM_GETINFO、打单行 LAUNCH 非标记日志后 park 永驻。
//!
//! **门控面（wm_cli_h()）**：
//!
//! - wm 客户端端未交付（0——rootfs 相 wmd 随 fb 探测门不被拉起 → 本
//!   程序不被拉起；亦含理论不可达兜底 = 一切无预拉 spawn/bsh 手跑）→
//!   **单行降级输出 + park**（v0 骨架形态延续，hdi_srv_h 先例；勿改成
//!   gfxdemo 的 exit(0) 形——`LAUNCH wm absent` 永不进 MARKERS，零标记
//!   扰动）。
//! - 在场（EFI 路）→ 严格串行一次 WM_GETINFO RPC（gfxdemo `rpc` 形态
//!   镜像精简：send -EBUSY → WRITABLE 10s 帽重臂；recv -EAGAIN →
//!   READABLE 10s 帽；无 attach 面——GETINFO 无随行 handle）→ parse
//!   `WmGetinfoRep` → **单行非标记日志**（`LAUNCH` 前缀 canon 零碰撞
//!   ——2026-11-02 grep 全树实证；无 " ok" 形 = 非 smoke 标记语义）+
//!   park。**原值宁报勿猜**（fmt 原值先例）：不做 gfxdemo 式断言链
//!   （那是 WM ok 标记语义面，launcher 非标记面）；parse 失败 =
//!   eprintln 诊断 + park（不留半开态）。
//!
//! **wiring 刀明确不做（挂 GUI-8 主体刀，刀前勿在此扩面）**：
//!
//! - svcmgr 预拉/arm 已随本刀兑现（D1-D8 定案）：不 ps_reg（psproto 17
//!   帽不留裕——launcher v0 = 验收件沿 gfxdemo「瞬态验收件不入账」先例，
//!   PS ok n= 零漂；入账归 GUI-8 主体刀连带抬帽裁决）；park 永驻不
//!   reap（常驻面——gfxdemo 阻塞 reap 形不适用）。
//! - TODO(GUI-8 主体刀): present/input 客户端接线——GUI-2段③（present
//!   服务端真身）/GUI-4段②（gui-inputd）均落 gui3 未并 main，帧循环
//!   SUBMIT/CALLBACK/RELEASE 全踩彼刀；input 域端点经 gift 交付（GUI-4
//!   草案 §5.3：launcher 不直连 inputd）。
//! - TODO(GUI-8 主体刀): fs/布局读写——berry-layout（布局 JSON v1
//!   纯函数解析/发射，已落 main）parse/emit 消费后置；落点 = FAT12 卷根
//!   LAYOUT.JSN（≤2048B 单簇近原子；侦察 §3.4）；fs 单活跃客户让位序
//!   随裁决四。
//! - TODO(GUI-8 主体刀): spawn——点击图标拉起 .bap（裁决二待裁：直行
//!   复刻 app_launch 守卫 vs svcmgr 常驻 spawn RPC）。
//! - TODO(GUI-8 主体刀): 一切 smoke 标记——LAUNCH* 标记方案 + canon
//!   碰撞复验 + smoke 三脚本同刀（smoke.sh 改动归主线串行，规划 §4:97）。

use berry_guiproto as gp;
use berry_libc::io::wm_cli_h;
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_object_wait_one, Handle,
    BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC, SIGNAL_READABLE, SIGNAL_WRITABLE,
};

// 链接种子（fsmgr/guifbd 同款）：rlib/LTO 按需保留——无引用则 berry-libc
// 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 收侧 READABLE 帽（gfxdemo `RPC_CAP_NS` 同值 10s——A6-a 形态；本
/// 程序无 exit 面，超时/错码 = eprintln 诊断 + park 挂起，不留半开态）。
const RPC_CAP_NS: u64 = 10 * 1_000_000_000;

/// 诊断 + park 收口（gfxdemo `fail` 的 park 形——launcher 常驻不被 reap，
/// exit 码无消费者；park = stdout EOF + 10s 周期睡眠，guifbd 自验后
/// park 同款；v0 骨架形态延续，勿改成 exit 形）。
fn fail(what: &str) -> ! {
    eprintln!("launcher: {what}");
    berry_libc::park()
}

/// now = clock_read；失败按 0 → deadline 即刻到期 = fail-fast（gfxdemo
/// `now` 同口径——时钟不可用当死路，不静默自旋）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

/// 严格串行 RPC 单趟（gfxdemo `rpc` 精简形——无 attach 面：本程序唯一
/// 请求 WM_GETINFO 无随行 handle，发送侧恒 null/0）。发送侧：-EBUSY →
/// 阻塞等 WRITABLE（10s 帽重臂，雷 2 纪律不自旋；send 原子性——EBUSY/
/// 错码时随行 handle 零迁移）。收侧：空队 -EAGAIN → 阻塞等 READABLE
/// （`RPC_CAP_NS` 帽）。返回应答长；应答随行 handle 收槽保留 gfxdemo 形
/// ——GETINFO 应答恒零随行，收下不消费。
fn rpc(ch: Handle, req: &[u8], rep: &mut [u8; 256]) -> usize {
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(ch, req.as_ptr(), req.len(), core::ptr::null(), 0);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            fail(&format!("rpc send err={s}"));
        }
        if deadline == 0 {
            deadline = now() + RPC_CAP_NS;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            fail("rpc send wait timeout");
        }
        deadline = 0; // 重臂（命中后仍满则续等——公平排队）
    }
    let mut rhs = [0 as Handle; 8];
    loop {
        // 每轮重置容量（in-out 出参——漏重置 = 恒 -EINVAL 死循环，A1-b）。
        let mut nb = rep.len();
        let mut nh = rhs.len();
        let s = berry_channel_recv(ch, rep.as_mut_ptr(), &mut nb, rhs.as_mut_ptr(), &mut nh);
        if s == 0 {
            if nb == 0 {
                fail("rpc empty reply");
            }
            return nb;
        }
        if s != BERRY_ERR_AGAIN {
            fail(&format!("rpc recv err={s}"));
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            fail("rpc reply timeout 10s");
        }
    }
}

fn main() {
    // —— 降级门：wm 客户端端未交付（rootfs 相 wmd 随 fb 探测门不被拉起
    // → 本程序不被拉起；bsh 手跑无 extras）→ 一行诊断 + park（v0 骨架
    // 形态延续——理论不可达兜底，hdi_srv_h 先例；勿改成 gfxdemo 的
    // exit(0) 形）。
    let hw = wm_cli_h();
    if hw == 0 {
        eprintln!("LAUNCH wm absent (no wm client gift)");
        berry_libc::park();
    }

    // —— WM_GETINFO 一次握手（4B 无参请求 / 12B 应答——wmd 权威能力面，
    // disp 域 = wmd 启动期 present GETINFO 缓存同源）。原值宁报勿猜——
    // 不做 gfxdemo 式断言链（WM ok 标记语义面归 gfxdemo，launcher 非标记
    // 面）；parse 失败 = eprintln 诊断 + park（不留半开态）。
    let mut req = [0u8; 4];
    let mut rep = [0u8; 256];
    let m = gp::encode_wm_getinfo_req(&mut req);
    let n = rpc(hw, &req[..m], &mut rep);
    let info = match gp::parse_wm_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("wm getinfo rep bad st={}", rep[0])),
    };
    // 单行非标记日志（LAUNCH 前缀 canon 零碰撞；无 " ok" 形）——EFI 相
    // 唯一新增行。原值直报（v0 fmt 原值先例延续）。
    println!(
        "LAUNCH v={} wmax={} {}x{}",
        info.version_wm, info.windows_max, info.disp_w, info.disp_h
    );
    // 收口：park 挂起（stdout EOF + 10s 周期睡眠）——常驻桌面占位，帧
    // 循环/serve 循环随 GUI-8 主体刀接入。
    berry_libc::park();
}
