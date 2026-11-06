//! SuspendToken（1.1/S3）：冻结令牌——持有即冻结，close 即解冻。
//!
//! 嵌套 depth：多 token 同进程——第一个 close 不解冻，最后一个才解冻。
//! `Process.freeze_depth` 计数；token 持有 Process ObjRef 引用（retain/
//! release——保证 Drop 时 Process 仍存活可访问 freeze_depth）。

use berry_sched;
use core::ptr::NonNull;
use crate::handle::ObjRef;

/// 冻结令牌对象。
pub struct SuspendTok {
    /// 目标 Process 的 ObjRef（retain 过——Drop 时 release）。
    pub(crate) proc: NonNull<ObjRef>,
    /// 备份钉住键（proc 可能已退——此时 thaw 面无任务，无害）。
    pub(crate) pinned: usize,
    /// 冻结时刻 CNTVCT（遥测面预留）。
    pub(crate) since_vct: u64,
}

impl Drop for SuspendTok {
    fn drop(&mut self) {
        // SAFETY: proc 已 retain（refs ≥ 1 保证 Drop 时对象存活）；Drop 链
        // 在 IRQ 屏蔽路径（handle close），单核顺序。
        unsafe {
            let r = &mut *self.proc.as_ptr();
            if let Some(p) = r.obj.as_process_mut() {
                if p.freeze_depth > 0 {
                    p.freeze_depth -= 1;
                    if p.freeze_depth == 0 {
                        berry_sched::thaw_by_pinned(self.pinned);
                    }
                }
            }
            // 释放 ObjRef 引用（可能归零 → Process 销毁）。
            ObjRef::release(self.proc);
        }
    }
}
