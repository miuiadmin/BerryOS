//! SuspendToken（1.1/S3）：冻结令牌——持有即冻结，close 即解冻。
//!
//! 嵌套 depth：多 token 同一目标——第一个 close 不解冻，最后一个才解冻。
//! 目标可为 Process（全任务冻结）或 Thread（`SINGLE_THREAD`）。token retain
//! 目标 ObjRef；Thread 自身又 retain 所属 Process，Drop 链保证两者均存活。

use crate::handle::ObjRef;
use crate::process::SA_FLAG_FREEZE_READY;
use core::ptr::NonNull;

/// AM-1.5 wake-pending 通知位（`.agents/plans/am/design.md`「IPC × 冻结」
/// Q1 裁决「唤醒即解冻」的机制面）。**内核派生位**（低 32 位域——与
/// READABLE(1<<0)/WRITABLE(1<<1)/TERMINATED(1<<2) 同域共处、位不重叠；
/// 用户位高 32 位不受扰，`object_signal` 的用户半门控照旧）。语义：置位 =
/// 冻结目标（token 持有期间 retain）。
pub enum SuspendTarget {
    Process(NonNull<ObjRef>),
    Thread(NonNull<ObjRef>),
}

/// 冻结令牌对象。
pub struct SuspendTok {
    /// 目标对象（Process 或 Thread ObjRef；retain 过——Drop 时 release）。
    pub(crate) target: SuspendTarget,
    /// 备份定位键：Process = pinned；Thread = Thread ObjRef 地址。
    /// 目标可能已退——此时 thaw 面无任务，无害。
    pub(crate) key: usize,
    /// 冻结时刻 CNTVCT（遥测面预留）。
    #[allow(dead_code)] // 遥测面预留（S5-b 起消费）
    pub(crate) since_vct: u64,
}

impl Drop for SuspendTok {
    fn drop(&mut self) {
        let ptr = match self.target {
            SuspendTarget::Process(p) => p,
            SuspendTarget::Thread(t) => t,
        };
        let mut thaw_key = None;

        // SAFETY: 目标已 retain（refs ≥ 1 保证 Drop 时对象存活）；Drop 链在
        // IRQ 屏蔽路径（handle close），单核顺序。
        unsafe {
            let r = &mut *ptr.as_ptr();
            match &mut r.obj {
                crate::handle::KernelObj::Process(p) => {
                    if p.freeze_depth > 0 {
                        p.freeze_depth -= 1;
                        if p.freeze_depth == 0 {
                            // AM-1.5 清面完整（全解冻才清——嵌套中间层不解冻
                            // 不清）：wake_pending 归零 + 全部端点冻标释放
                            //（retain 配平）。此刻本 token 对目标仍持 1 引用
                            //（函数尾才 release）⇒ 枚举中属主恒活，clear 的
                            // release 只退 mark 份额、归零销毁不落枚举中途
                            //（ptr = 目标 Process ObjRef，打标侧的 owner 键）。
                            p.wake_pending = false;
                            p.ht.thaw_channel_ends(ptr);
                            thaw_key = Some(self.key);
                        }
                    }
                }
                crate::handle::KernelObj::Thread(t) => {
                    if t.freeze_depth > 0 {
                        t.freeze_depth -= 1;
                        if t.freeze_depth == 0 {
                            // 设计契约：最后一次 thaw 自动清 FREEZE_READY；
                            // 下次单线程冻结必须重新自证。同时撤销该 Thread 在
                            // Process 侧的单线程冻结占位（进程全冻互斥面）。
                            t.sched_attr.flags &= !SA_FLAG_FREEZE_READY;
                            let owner = t.owner();
                            if let Some(p) = (*owner.as_ptr()).obj.as_process_mut() {
                                p.thread_freezes = p.thread_freezes.saturating_sub(1);
                            }
                            thaw_key = Some(self.key);
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(key) = thaw_key {
            crate::telemetry::auto_thaw(); // S5.11 遥测
            match self.target {
                SuspendTarget::Process(_) => berry_sched::thaw_by_pinned(key),
                SuspendTarget::Thread(_) => berry_sched::thaw_by_thread(key),
            }
        }

        // SAFETY: 目标引用计数配平；可能归零并触发 Thread→Process 的 Drop 链。
        unsafe { ObjRef::release(ptr) }
    }
}
