//! berry-launcher —— GUI-8 桌面 launcher 骨架（可先行面 #2，docs/
//! GUI-8实施侦察-2026-11.md §5；规划与侦察文档定名 berry-launcher）。
//!
//! **v0 冻结面 = guifbd 同形 fb 探测门控**（本刀 svcmgr 面零触碰——
//! 预拉/arm/reap 归 gui3→main 并线后的下一刀，见下 TODO 锚）：
//!
//! - fb info VMO 缺席（`gift2_h() == 0`——-kernel 主路 boot 不插 fb 表，
//!   亦含 wiring 刀前一切无预拉 spawn）→ **单行降级输出 + park**。镜像
//!   guifbd 降级形态（`eprintln!` loud + park），**零标记扰动**——绝不
//!   打任何 smoke 断言标记（主 smoke rootfs 相 46 canon 不受扰；svcmgr
//!   主走查只认在册名，rootfs 新增 bin/ 条目 = 纯惰性字节）。
//! - 在场（EFI 路：boot 几何 info VMO 经 env 块 extras[0] 随行——wiring
//!   刀 `SpawnSpec::extra_gifts` 交付，guifbd 第 6 插同源）→ 读 40B 五域
//!   （fb_pa/w/h/stride/fmt，u64 LE）打 **单行非断言骨架日志**（`LAUNCH`
//!   前缀 canon 零碰撞——2026-11-02 grep 全树实证；v0 只打日志不打标记）
//!   + park。
//!
//! **v0 明确不做（挂并线后 wiring 刀，刀前勿在此扩面）**：
//!
//! - TODO(GUI-8 wiring 刀): svcmgr 预拉/arm/阻塞 reap——镜像 guifbd 预拉
//!   段（svcmgr main.rs 逐字锚可 grep 整串定位；短写「guifbd 预拉起」
//!   全树 0 命中勿用）：
//!   `---- GUI-1：guifbd（framebuffer 显示服务）预拉起 ----`
//!   fbinfo 经
//!   `extra_gifts` take 迁入本表、`ps_reg` 记账；reap 内嵌 wait 30s 绝对
//!   帽 → GUI app 常跑分钟级，须先自行 `berry_object_wait_one`(
//!   TERMINATED, 长帽) 再 reap（侦察 §3.3）。
//! - TODO(GUI-8 wiring 刀): present/input 客户端接线——GUI-2段③（present
//!   服务端真身）/GUI-4段②（gui-inputd）均落 gui3 未并 main，帧循环
//!   SUBMIT/CALLBACK/RELEASE 全踩彼刀；input 域端点经 gift 交付（GUI-4
//!   草案 §5.3：launcher 不直连 inputd）。
//! - TODO(GUI-8 wiring 刀): fs/布局读写——berry-layout（布局 JSON v1
//!   纯函数解析/发射，已落 main）parse/emit 消费后置；落点 = FAT12 卷根
//!   LAYOUT.JSN（≤2048B 单簇近原子；侦察 §3.4）；fs 单活跃客户让位序
//!   随裁决四。
//! - TODO(GUI-8 wiring 刀): spawn——点击图标拉起 .bap（裁决二待裁：
//!   直行复刻 app_launch 守卫 vs svcmgr 常驻 spawn RPC）。
//! - TODO(GUI-8 wiring 刀): 一切 smoke 标记——LAUNCH* 标记方案 + canon
//!   碰撞复验 + smoke 三脚本同刀（smoke.sh 改动归主线串行，规划 §4:97）。

use berry_libc::io::gift2_h;
use berry_sys::berry_vmo_read;

// 链接种子（fsmgr/guifbd 同款）：rlib/LTO 按需保留——无引用则 berry-libc
// 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    // fbinfo 探测门（guifbd 同形）：extras[0] = boot 几何 info VMO
    // （40B 五域 u64 LE）——0 = 未交付。
    let h = gift2_h();
    if h == 0 {
        // -kernel 主路（boot 不插 fb 表）或 wiring 刀前无预拉：单行降级
        // + park，零标记扰动（svcmgr `eco: fb absent` 同语义、进程内版）。
        eprintln!("LAUNCH skel fb absent (no fbinfo gift)");
        berry_libc::park();
    }
    let mut info = [0u8; 40];
    if berry_vmo_read(h, info.as_mut_ptr(), 0, info.len()) != 0 {
        eprintln!("LAUNCH skel fbinfo read failed");
        berry_libc::park();
    }
    let rd = |i: usize| {
        u64::from_le_bytes(info[i * 8..i * 8 + 8].try_into().unwrap()) as usize
    };
    let (_fb_pa, w, h, stride) = (rd(0), rd(1), rd(2), rd(3));
    let fmt_raw = (rd(4) as u64 & 0xFFFF_FFFF) as u32;
    // 单行非断言骨架日志（LAUNCH 前缀 canon 零碰撞；无 " ok" 形 = 非
    // smoke 标记语义）。fmt 原值上报不做 PixelFormat 判别——v0 无渲染
    // 面，宁报勿猜（guifbd from_raw 宁缺勿滥同精神）。
    println!("LAUNCH skel w={} h={} stride={} fmt={}", w, h, stride, fmt_raw);
    // 骨架收口：park 挂起（stdout EOF + 10s 周期睡眠；guifbd 自验后
    // park 同款）。帧循环/serve 循环随 wiring 刀接入。
    berry_libc::park();
}
