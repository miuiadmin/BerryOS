//! vDSO 数据页——clock_read MONOTONIC vDSO 化的内核半（W3-Q106，03 号
//! syscall 分派审计机会三）。
//!
//! 一次完整 SVC 包络（入口 19 存 + KERN_LOCK 全对 + 尾三查 + 恢复 19 读）
//! 换两条本可在 EL0 执行的指令（`mrs cntvct_el0` + 页内参数乘除）——审计
//! 判词「整号消灭」。机制：boot 建**全局单页 VMO**（内容 = magic + CNTFRQ
//! 快照，boot 后恒定不变 → 无 seqlock 面），每个用户 AS 装配期把同一 VMO
//! 以 `Perm::UserRo` 映到固定 [`VDSO_VA`]（布局与换算单源 =
//! `user/berry-vdsoface`，双消费 = 本模块 + berry-sys fast path）。
//!
//! 权衡裁决（为何全局共享一页而非 per-AS 拷贝）：
//! - 内容 boot 后恒定（freq 读 CNTFRQ 寄存器，复位后不变；无 REALTIME
//!   epoch——注入面语义仍归 syscall 回落，v2 seqlock 扩展预留 `seq` 域），
//!   只读映射下用户改不了 → 共享安全；
//! - per-AS 拷贝 = 每进程一页帧 + 每页一份换算参数，零语义增益；
//! - 共享页的 mapcnt 记账天然成立（VMO 永生 = static 持有，Region 指针
//!   有效性依托「直构面生存期」政策——与 `UserProcess::_vmos` 同款论证，
//!   见 `vmar.rs` Region 注释三条政策）。
//!
//! EL0 直读 CNTVCT 的硬件门（CNTKCTL_EL1.EL0VCTEN=1）由 `timer::init`
//! 自 B-1 起显式打开（bench1 计时面先例），本刀零新增系统寄存器面。

use berry_mm::frame::FrameAllocator;
use crate::vmar::{Perm, Vmar, VmarError};
use crate::vmo::Vmo;
use berry_vdsoface::{VDSO_MAGIC, VDSO_OFF_FREQ, VDSO_OFF_SEQ, VDSO_PAGE, VDSO_VA};

/// 全局 vDSO 数据页 VMO（boot 一次建定，终生只读映射——static 持有即
/// 「永生」论证：Region 指针永不悬空，unmap 只减 mapcnt 不销毁对象）。
static mut VDSO_VMO: Option<Vmo> = None;

/// boot 期建页（timer::init 之后、首个用户 AS 装配之前；DAIF 屏蔽单线程
/// 窗——与 `sysver` 成像同款 boot 语境）。
///
/// 页内容：magic + freq 快照 + seq=0 + 余零。失败 = expect 炸响（boot 可信
/// 语境——一页帧都拿不到的机器没有继续的意义，`UserProcess::build` 同款
/// 裁决）。
pub fn init(fa: &mut FrameAllocator) {
    let freq = berry_hal_aarch64::timer::cntfrq();
    let mut vmo = Vmo::create(VDSO_PAGE).expect("vdso: 建 VMO（boot 可信语境）");
    vmo.commit(fa, 0, VDSO_PAGE).expect("vdso: commit");
    let mut page = [0u8; VDSO_PAGE];
    page[..8].copy_from_slice(&VDSO_MAGIC.to_le_bytes());
    page[VDSO_OFF_FREQ..VDSO_OFF_FREQ + 8].copy_from_slice(&freq.to_le_bytes());
    page[VDSO_OFF_SEQ..VDSO_OFF_SEQ + 8].copy_from_slice(&0u64.to_le_bytes());
    vmo.write(0, &page).expect("vdso: 填页");
    // SAFETY: boot 单线程（DAIF 屏蔽）唯一写点；此后恒 Option::Some 只读。
    unsafe { VDSO_VMO = Some(vmo) };
}

/// 把 vDSO 页映进给定 root Vmar 的固定 [`VDSO_VA`]（每 AS 装配期调用；
/// `UserProcess::build` 与 `sys_process_create` 两个装配路径共用）。
///
/// `init` 前调（boot 序错）= panic 炸响——装配路径在 boot 尾，早于它说明
/// 调用序破约，loud not silent。
pub fn map_into(fa: &mut FrameAllocator, vmar: &mut Vmar) -> Result<(), VmarError> {
    // SAFETY: init 后恒 Some 只读；boot 单线程建、此后多核只读 = 无竞态。
    let vmo = unsafe { VDSO_VMO.as_mut().expect("vdso: init 前装配 AS（boot 序破约）") };
    vmar.map(fa, vmo, VDSO_VA, 0, VDSO_PAGE, Perm::UserRo)
        .map(|_| ())
}
