//! handle 表 + rights + 内核对象壳（`能力模型设计.md` §2/§4；决策 #28 位域）。
//!
//! Phase 3-0：纯内核层——尚无 syscall 接线（3-1 起 v1 27 号经此门控），本步
//! ktest `HANDLE ok` 验证语义，外部行为零变化（语义用例见 `ktest::test_handle`；
//! 内核 bin 是 no_std 无 host test 入口，berry-mm 式 host 单测不适用）。
//!
//! - handle = **槽位号[15:0] | 代号 gen[29:16]**（bit31 预留类型编码，v2 可选，
//!   `docs/系统调用ABI设计-v1.0.md` §8.1）。`0` = 非法（槽 0 恒空）。
//! - **代号 v1 预留**：gen 从 1 起、槽位重用单调 +1；旧 gen 值查表 →
//!   [`ERR_BAD_HANDLE`]。v3 启用 DEAD 检测后，对象死亡主动作废代号 → 改返
//!   `ERR_DEAD`（「死活感知」与「权限派生」分工见能力模型 §2）。
//! - **权限在 handle 不在对象**：每槽自带 rights；对象本体由 [`ObjRef`] 共享
//!   （引用计数 = 持有它的 handle 数，最后 close 释放）。
//! - 权限只能收紧：`duplicate` 要求新 rights ⊆ 原（防提权，永不增权）。
//!
//! v1 裁剪：页表帧不回收（v1 政策）；`RIGHT_DUPLICATE` 位自 P3-1 起执法
//! ——syscall 面（`sys_handle_duplicate` 经 [`HandleTable::rights_of`] 探针）
//! 源 handle 缺位即 -EACCES，内核内部直调 [`HandleTable::duplicate`]（boot
//! 插表/ktest 表 API 的授能语境）不设此门。3-2 起 Vmo 销毁回收
//! 未映射物理页（`Vmo::Drop`），映射未解除的 VMO 拒 close（悬空 PTE 防线，
//! 见 [`HandleTable::close`]）。

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ptr::NonNull;

/// handle（`berry_handle_t`，u32；位域见模块注释）。
pub type Handle = u32;
/// rights（`berry_rights_t`，u64 位掩码）。
pub type Rights = u64;

// ---- rights / 错误码 / 信号位（ABI §8.2 数值冻结）——单源生成 ----
// 常量本体在 `crate::abi_gen`（`tools/abi.toml` 生成文件）；此处 re-export
// 维持既有路径（`crate::handle::RIGHT_*`/`ERR_*`/`SIGNAL_*` 被 syscall/ktest
// 直接引用）。位表是 ABI 冻结值、未消费位全量在册（dead_code 由 main.rs 的
// `mod abi_gen` 整体豁免）。改值/加位先改 abi.toml 再 `just abi-gen`。
// `mod abi_gen` 的 dead_code 豁免盖常量本体；`pub use` 转发的未消费冻结位
// （EXECUTE/INSPECT/TRANSFER 等）另触发 unused_imports——同样整体豁免。
#[allow(unused_imports)]
pub use crate::abi_gen::{
    ERR_ACCESS_DENIED, ERR_AGAIN, ERR_BAD_HANDLE, ERR_BAD_STATE, ERR_BUSY, ERR_FAULT,
    ERR_INVALID_ARG, ERR_NOT_FOUND, ERR_NOT_SUPPORTED, ERR_NO_MEMORY, ERR_OUT_OF_RANGE,
    ERR_TIMEDOUT, RIGHT_DUPLICATE, RIGHT_EXECUTE, RIGHT_INSPECT, RIGHT_MANAGE, RIGHT_MAP,
    RIGHT_READ, RIGHT_RECV, RIGHT_SEND, RIGHT_SET_POLICY, RIGHT_SIGNAL, RIGHTS_SAME,
    RIGHT_TRANSFER, RIGHT_WRITE, SIGNAL_PEER_CLOSED, SIGNAL_READABLE, SIGNAL_TERMINATED,
    SIGNAL_USER_MASK, SIGNAL_WAKE_PENDING, SIGNAL_WRITABLE,
};

// ---- handle 位域（决策 #28：低位槽位、中位代号、高位可选类型）----
// 常量与 encode/decode 单源在 berry-hdlface（W3-Q8 host 钉板——位域编码
// 律/别名拒/发行域门表驱动在册）；此处 re-export 维持 kernel 内路径。
pub use berry_hdlface::{
    decode_gen, decode_slot, encode, gen_in_issue_domain, has_reserved_bits,
    slot_index_valid, GEN_MASK,
};

/// 对象类型（镜像 `berry_obj_type` 枚举值，ABI §8.2 冻结）。
/// Process/Channel 等暂无构造点——3-3/3-4 增补对象时启用，全表先冻结在册。
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ObjType {
    Process = 1,
    Thread = 2,
    Vmo = 3,
    Vmar = 4,
    Channel = 5,
    Event = 6,
    EventPair = 7,
    Interrupt = 8,
    Resource = 9,
    Timer = 10,
    Job = 11,
    SuspendToken = 12,
}

/// 内核对象壳（Zircon 务实派，ADR #4：对象在内核、用户态只见整数 handle）。
/// 3-3 起 Process/Thread 入壳（`process.rs`；对象间钉住见 [`ObjRef::retain`]）。
pub enum KernelObj {
    Vmo(crate::vmo::Vmo),
    Vmar(crate::vmar::Vmar),
    Process(crate::process::Process),
    Thread(crate::process::Thread),
    Channel(crate::channel::ChannelEnd),
    Event(crate::event::Event),
    #[allow(dead_code)] // Drop 即语义（thaw_by_pinned）——field 不被直接读
    SuspendToken(crate::suspend::SuspendTok),
}

impl KernelObj {
    pub fn obj_type(&self) -> ObjType {
        match self {
            KernelObj::Vmo(_) => ObjType::Vmo,
            KernelObj::Vmar(_) => ObjType::Vmar,
            KernelObj::Process(_) => ObjType::Process,
            KernelObj::Thread(_) => ObjType::Thread,
            KernelObj::Channel(_) => ObjType::Channel,
            KernelObj::Event(_) => ObjType::Event,
            KernelObj::SuspendToken(_) => ObjType::SuspendToken,
        }
    }

    /// 只读取 Vmo 本体（类型门控后的防御性解包——不符返 None，调用方折错误码
    /// 而非 panic，守 syscall 边界无 panic 纪律）。
    pub fn as_vmo(&self) -> Option<&crate::vmo::Vmo> {
        match self {
            KernelObj::Vmo(v) => Some(v),
            _ => None,
        }
    }

    /// 可变取 Vmo 本体（syscall 写路径；同上防御性）。
    pub fn as_vmo_mut(&mut self) -> Option<&mut crate::vmo::Vmo> {
        match self {
            KernelObj::Vmo(v) => Some(v),
            _ => None,
        }
    }

    /// 只读取 Vmar 本体（Process 绑定底层 AddressSpace 等）。
    pub fn as_vmar(&self) -> Option<&crate::vmar::Vmar> {
        match self {
            KernelObj::Vmar(v) => Some(v),
            _ => None,
        }
    }

    /// 可变取 Vmar 本体（vmar_* syscall；同上防御性）。
    pub fn as_vmar_mut(&mut self) -> Option<&mut crate::vmar::Vmar> {
        match self {
            KernelObj::Vmar(v) => Some(v),
            _ => None,
        }
    }

    /// 只读取 Process 本体（process_start 校验归属等）。
    pub fn as_process(&self) -> Option<&crate::process::Process> {
        match self {
            KernelObj::Process(p) => Some(p),
            _ => None,
        }
    }

    /// 可变取 Process 本体（process_start 写子进程 handle 表 / spawn 派发标签）。
    pub fn as_process_mut(&mut self) -> Option<&mut crate::process::Process> {
        match self {
            KernelObj::Process(p) => Some(p),
            _ => None,
        }
    }

    /// 可变取 Thread 本体（process_start 写 started/entry/sp）。
    pub fn as_thread_mut(&mut self) -> Option<&mut crate::process::Thread> {
        match self {
            KernelObj::Thread(t) => Some(t),
            _ => None,
        }
    }

    /// 可变取 Channel 端点本体（channel_send/recv 队列操作；同上防御性）。
    pub fn as_channel_mut(&mut self) -> Option<&mut crate::channel::ChannelEnd> {
        match self {
            KernelObj::Channel(c) => Some(c),
            _ => None,
        }
    }

    /// 只读取 Channel 端点本体（wait 面信号状态查询——4-2）。
    pub fn as_channel(&self) -> Option<&crate::channel::ChannelEnd> {
        match self {
            KernelObj::Channel(c) => Some(c),
            _ => None,
        }
    }

    /// 可变取 Event 本体（object_signal 置/清位；同上防御性——wait 面统一走
    /// syscall.rs `signal_state`，Event 只读路径不再单独取本体）。
    pub fn as_event_mut(&mut self) -> Option<&mut crate::event::Event> {
        match self {
            KernelObj::Event(e) => Some(e),
            _ => None,
        }
    }
}

/// 共享对象体：引用计数 + 对象本体。`Box` 进堆、槽持裸指针；64 位组合原子态归零
/// 是本模块唯一释放点。
///
/// F1-2：`state` 低 32 位为句柄与内部对象钉住计数（`refs`），高 32 位为活跃地址空间
/// 页级映射保活计数（`map_refs`）。仅当 `state == 0`（`refs == 0 && map_refs == 0`）
/// 时销毁 `Box<ObjRef>`，彻底删除 `HandleTable::drain()` 的 `leak = true` 分支并根除
/// 父进程先于子进程退出或同表跨槽析构顺序导致的 `*mut Vmo` UAF。
pub struct ObjRef {
    state: core::sync::atomic::AtomicU64,
    pub obj: KernelObj,
}

impl ObjRef {
    #[inline]
    pub fn refs(&self) -> usize {
        (self.state.load(core::sync::atomic::Ordering::Acquire) & 0xFFFF_FFFF) as usize
    }

    #[inline]
    pub fn retain(&self) {
        self.state
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }

    /// 句柄/对象钉住释放：低 32 位 -1，若整体 64 位计数（含 `map_refs`）归零则立即销毁对象。
    ///
    /// # Safety
    /// `ptr` 须来自一次有效的句柄槽或 `retain`，且调用方此后不再持有该令牌。
    pub unsafe fn release(ptr: NonNull<Self>) {
        let prev = (*ptr.as_ptr())
            .state
            .fetch_sub(1, core::sync::atomic::Ordering::AcqRel);
        debug_assert!((prev & 0xFFFF_FFFF) >= 1, "ObjRef::release refs underflow");
        if prev == 1 {
            drop(Box::from_raw(ptr.as_ptr()));
        }
    }

    /// 地址空间映射保活 +`npages`（高 32 位计数，不影响 `close` 的 `refs == 1` `-EBUSY` 门控）。
    #[inline]
    pub fn retain_map(&self, npages: usize) {
        if npages > 0 {
            self.state.fetch_add(
                (npages as u64) << 32,
                core::sync::atomic::Ordering::Relaxed,
            );
        }
    }

    /// 地址空间映射保活 -`npages`：若句柄侧 `refs` 已先归零（如父进程先退出），本最后一次解映射即销毁 `ObjRef`。
    ///
    /// # Safety
    /// `ptr` 须来自配对的 `retain_map(npages)`。
    pub unsafe fn release_map(ptr: NonNull<Self>, npages: usize) {
        if npages == 0 {
            return;
        }
        let delta = (npages as u64) << 32;
        let prev = (*ptr.as_ptr())
            .state
            .fetch_sub(delta, core::sync::atomic::Ordering::AcqRel);
        debug_assert!(prev >= delta, "ObjRef::release_map underflow");
        if prev == delta {
            drop(Box::from_raw(ptr.as_ptr()));
        }
    }
}

/// 槽状态：空槽记住 `next_gen`（重用时代号单调 +1——旧 handle 值自然作废）。
enum SlotState {
    Free { next_gen: u32 },
    Used { gen: u32, obj: NonNull<ObjRef>, rights: Rights },
}

/// handle 表（能力模型 §4：每进程一张——3-4a 起真拆分：Process 拥有自己的表，
/// EL0 syscall 经任务标签派发到所属进程表；本类型另作内核侧表供 ktest/boot）。
///
/// 单核顺序访问（v1 无 SMP；IRQ 上下文不碰表——syscall 均同步异常路径）。
pub struct HandleTable {
    /// 槽 0 恒 `Free`（handle 0 = `BERRY_HANDLE_INVALID`）。
    slots: Vec<SlotState>,
}

impl HandleTable {
    /// 建空表（槽 0 保留）。
    pub fn new() -> Self {
        // 零分配惰性（3b3 审查 H2 try 化）：曾 `Vec::with_capacity(8)`——EL0 经
        // `process_create` 建 Process 即触堆分配，OOM abort。保留槽 0 与首扩都
        // 延迟到首次 insert 的 `alloc_slot`（其内 try_reserve）。
        Self { slots: Vec::new() }
    }

    /// 放入对象并授予 rights，返回新 handle（取最低可用空槽）。**先占槽后
    /// 装箱**（3b3 审查 H3）：入表失败路径发生在任何分配之前，无对象可泄漏。
    pub fn insert(&mut self, obj: KernelObj, rights: Rights) -> Result<Handle, i32> {
        let slot = self.alloc_slot()?;
        let gen = self.take_gen(slot)?;
        let Some(boxed) = crate::heap::try_box(ObjRef {
            state: core::sync::atomic::AtomicU64::new(1),
            obj,
        }) else {
            return Err(ERR_NO_MEMORY);
        };
        // SAFETY: 刚 into_raw 的裸指针，非空。
        let obj = unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) };
        self.slots[slot] = SlotState::Used { gen, obj, rights };
        Ok(encode(slot as u32, gen))
    }

    /// 复制 handle（= seL4 mint 的收紧侧）：新 handle 权利 = `rights`
    /// （[`RIGHTS_SAME`] = 原全集），必须非空且 ⊆ 原——**永不增权**。
    /// 同对象引用 +1。
    pub fn duplicate(&mut self, h: Handle, rights: Rights) -> Result<Handle, i32> {
        let (obj, orig_rights) = self.lookup(h)?;
        let new_rights = if rights == RIGHTS_SAME { orig_rights } else { rights };
        if new_rights == 0 || new_rights & !orig_rights != 0 {
            return Err(ERR_ACCESS_DENIED); // 空 rights / 提权尝试一律拒
        }
        // 先占槽、后 +1 引用（3b3 审查 H3）：失败路径零副作用，无需回滚。
        let slot = self.alloc_slot()?;
        let gen = self.take_gen(slot)?;
        // SAFETY: lookup 已验指针在表且 refs ≥ 1。
        unsafe {
            let r = &*obj.as_ptr();
            r.retain();
            if let KernelObj::Channel(c) = &r.obj {
                c.retain_handle();
            }
        }
        self.slots[slot] = SlotState::Used { gen, obj, rights: new_rights };
        Ok(encode(slot as u32, gen))
    }

    /// 关闭 handle：引用 -1，若整体计数（含地址空间 `map_refs`）归零则释放对象。
    pub fn close(&mut self, h: Handle) -> Result<(), i32> {
        let (slot, gen, obj, _) = self.lookup_at(h)?;
        // SAFETY: 指针在表、refs ≥ 1；只读探测（close 拒绝面——只拦销毁性 close）。
        unsafe {
            let r = &*obj.as_ptr();
            if r.refs() == 1 {
                if let KernelObj::Vmo(v) = &r.obj {
                    if v.any_mapped() || v.demand_mapped() {
                        return Err(ERR_BUSY);
                    }
                }
            }
        }
        self.slots[slot] = SlotState::Free { next_gen: gen + 1 };
        // SAFETY: 先腾空槽位再递减通道句柄计数并归还引用令牌，确保重入查表一致。
        unsafe {
            if let KernelObj::Channel(c) = &(*obj.as_ptr()).obj {
                c.release_handle();
            }
            ObjRef::release(obj);
        }
        Ok(())
    }

    /// 原子替换（Zircon `handle_replace` 语义）：`h` 与 `repl` 两值均失效，
    /// `repl` 的对象+权利入驻**新槽**并以新 handle 返回；`h` 的原对象按
    /// close 处理（引用 -1，可能释放）。对象引用数不变（槽间搬迁）。
    pub fn replace(&mut self, h: Handle, repl: Handle) -> Result<Handle, i32> {
        if h == repl {
            return Err(ERR_INVALID_ARG);
        }
        // 先验两值（原子性：任一无效则表不动）。
        let (_, _, hobj, _) = self.lookup_at(h)?;
        let (r_slot, r_gen, obj, rights) = self.lookup_at(repl)?;
        // SAFETY: 指针在表（lookup_at 过）；只读探测。
        unsafe {
            let r = &*hobj.as_ptr();
            if r.refs() == 1 {
                if let KernelObj::Vmo(v) = &r.obj {
                    if v.any_mapped() || v.demand_mapped() {
                        return Err(ERR_BUSY);
                    }
                }
            }
        }
        // repl 槽先腾空（对象仍被本 handle 引用着，不 drop）。
        self.slots[r_slot] = SlotState::Free { next_gen: r_gen + 1 };
        let slot = self.alloc_slot()?;
        let gen = self.take_gen(slot)?;
        self.slots[slot] = SlotState::Used { gen, obj, rights };
        // h 按 close 处理（可能 drop 其原对象）。
        self.close(h)?;
        Ok(encode(slot as u32, gen))
    }

    /// **取出**（3-4a：handle 迁移原语）：把槽整体摘走——对象引用数**不变**
    /// （引用令牌随槽迁移，非销毁），槽位代号 +1（原 handle 值立即作废）。
    pub fn take(&mut self, h: Handle) -> Result<(NonNull<ObjRef>, Rights), i32> {
        let (slot, gen, obj, rights) = self.lookup_at(h)?;
        self.slots[slot] = SlotState::Free { next_gen: gen + 1 };
        Ok((obj, rights))
    }

    /// **放入既有对象**（[`Self::take`] 的对侧）：已 Boxing 的 [`ObjRef`] 挂进
    /// 新槽并授予 rights，返回新 handle。引用数**不变**。
    pub fn insert_ref(&mut self, obj: NonNull<ObjRef>, rights: Rights) -> Result<Handle, i32> {
        let slot = self.alloc_slot()?;
        let gen = self.take_gen(slot)?;
        self.slots[slot] = SlotState::Used { gen, obj, rights };
        Ok(encode(slot as u32, gen))
    }

<<<<<<< HEAD
    /// **清空整表**（Plan B v0.5：进程退出钩子 drain 死亡进程的表）。逐槽走
    /// [`Self::close`] 的释放语义（引用 -1、归零 drop Box、槽代号 +1），但
    /// **不做销毁性拒检**——close 的 -EBUSY 拒面保护的是**活进程**的映射
    /// 一致性；表主已死，映射清账由 `Vmar::Drop` 兜底（帧归还刀件 A：残留
    /// Region 的 clear_leaves + untrack——mapcnt 归零后 VMO 可还帧）。
    ///
    /// **双相序（帧归还刀件 B）**：phase-1 先落全部 Vmar → phase-2 落其余
    /// 对象。为什么必须分相：`Vmar::Drop` 清账要解引用 `Region.vmo`（untrack
    /// 回调）——**要求表内 VMO 仍活**；`Vmo::Drop` 还帧要求 mapcnt==0——
    /// **要求 Vmar 已清账**。单相遍历槽序不定（VMO 槽可能先于 Vmar 槽），
    /// 双相强制「Vmar 全落 → VMO 才落」的因果序。
    ///
    /// **槽状态机（先审后写）**：phase-1 对 Vmar 槽递减 refs，归零即
    /// `Box::from_raw` drop；**槽同步置 Free**——from_raw 已消费 Box 所有权
    /// ，槽若仍持 Used 态指针，phase-2/后续遍历再 from_raw 同一指针 = 双
    /// drop 堆腐化（前次实施 boot 崩二级候选①「drain 双相双放」即此形）
    /// 。每槽处理点单出口统一置 Free，无中间态。
    ///
    /// phase-1 的 Vmar 递减后仍 ≥1（被 Process 钉住或跨表持有）= 不 drop
    /// 不清账（对象活、属主另有其人——保守：flush 侧由
    /// `drain_pinned_process` 的 vmar 钉预放保证表内 root Vmar 恒可归零，
    /// 见 process.rs；跨表持有者〔channel 迁移〕的映射无恙）。
    ///
    /// **对象本体亦不销毁（VMO 泄漏兜底保留）**：`Region.vmo` 裸指针有效性
    /// 依托「close 拒 -EBUSY 保对象存活」（vmar.rs Region 注释）——phase-2
    /// 对 refs 将归零且仍被映射 **或 demand 登记中** 的 VMO 整档跳过递减
    /// （令牌随死表泄漏、对象保活——迟到 Drop 的清账解引用仍安全；mapcnt
    /// ==0 且 demand 钉==0 ⟺ 无 Region 指着它——两防线判据并集，精确）。
    /// **仅 IRQ 屏蔽的退出路径调用**（对象释放窗口内不可被切走）。
=======
    /// **排空整表**（F1-2 两阶段退出第二阶段）：在 `AddressSpace::teardown()` 完成之后逐槽
    /// 调用 `ObjRef::release` 归还句柄引用令牌，彻底废除旧版 `leak = true` 整对象泄漏分支。
    /// 若某 VMO 仍被另一存活子进程映射，`ObjRef` 高 32 位 `map_refs` 自动保活对象本体直至
    /// 该子进程 `teardown()` 完成最后一次 `release_map` 时零泄漏归还物理页。
>>>>>>> private/wip
    pub fn drain(&mut self) {
        // phase-1：先落全部 Vmar（mapcnt 归零）。槽审（类型判别）→ refs
        // 递减 → 归零 from_raw drop → 槽置 Free（防双 drop，见方法注）。
        for slot in self.slots.iter_mut() {
            if let SlotState::Used { gen, obj, .. } = *slot {
                // SAFETY: 指针在表；只读类型判别零解引用深度（match 对
                // KernelObj 判 tag）。
                let is_vmar =
                    unsafe { matches!((*obj.as_ptr()).obj, KernelObj::Vmar(_)) };
                if is_vmar {
                    // SAFETY: 指针在表、refs ≥ 1；单核顺序（屏蔽窗内）。
                    let refs = unsafe {
                        let r = &mut *obj.as_ptr();
                        r.refs -= 1;
                        r.refs
                    };
                    if refs == 0 {
                        // SAFETY: 引用归零、无别名——归还 Box（Vmar::Drop
                        // 清账：clear_leaves + untrack，解引用的 VMO 全活
                        // ——phase-2 才动 VMO 槽）。
                        unsafe { drop(Box::from_raw(obj.as_ptr())) };
                    }
                    *slot = SlotState::Free { next_gen: gen + 1 };
                }
            }
        }
        // phase-2：其余对象真正递减 + 触发 Vmo::Drop 还帧（此刻 Vmar 已清
        // 账，mapcnt 归零——VMO 的 droppable_pages 全量放行）。
        for slot in self.slots.iter_mut() {
            if let SlotState::Used { gen, obj, .. } = *slot {
                *slot = SlotState::Free { next_gen: gen + 1 };
                // SAFETY: 槽内活跃引用令牌；Channel 同步递减句柄级开闭计数触发 PEER_CLOSED。
                unsafe {
                    if let KernelObj::Channel(c) = &(*obj.as_ptr()).obj {
                        c.release_handle();
                    }
                    ObjRef::release(obj);
                }
            }
        }
    }

    /// **退出即时 IPC 排水**（F1-3）：进程末线程退出或被 `process_terminate` 强杀时，在持全谱锁窗口内
    /// 立即摘除并释放全部 `Channel` 与 `SuspendToken` 句柄槽，确保对端 `SIGNAL_PEER_CLOSED` 唤醒与
    /// 自动解冻零延迟触发，不受 `HT_DRAINQ` 异步排队影响。
    pub fn drain_ipc_on_exit(&mut self) {
        for slot in self.slots.iter_mut() {
            if let SlotState::Used { gen, obj, .. } = *slot {
                // SAFETY: 槽内活跃对象只读判别类型。
                let is_ipc = unsafe {
                    matches!(
                        &(*obj.as_ptr()).obj,
                        KernelObj::Channel(_) | KernelObj::SuspendToken(_)
                    )
                };
<<<<<<< HEAD
                if !leak {
                    // SAFETY: 指针在表、refs ≥ 1；单核顺序（屏蔽窗内）。
                    let refs = unsafe {
                        let r = &mut *obj.as_ptr();
                        r.refs -= 1;
                        r.refs
                    };
                    if refs == 0 {
                        // SAFETY: 引用归零、无别名——归还 Box（drop 对象本体；
                        // Vmo::Drop 经 with_frames 还帧——F0-3 ALLOC_LOCK 面）。
                        unsafe { drop(Box::from_raw(obj.as_ptr())) };
=======
                if is_ipc {
                    *slot = SlotState::Free { next_gen: gen + 1 };
                    // SAFETY: 摘槽后独占释放本槽引用。
                    unsafe {
                        if let KernelObj::Channel(c) = &(*obj.as_ptr()).obj {
                            c.release_handle();
                        }
                        ObjRef::release(obj);
>>>>>>> private/wip
                    }
                }
            }
        }
    }

    /// 查表取对象（syscall 门控入口：查表 → 类型 → 权限，能力模型 §7）。
    /// `need` = 本次操作要求的权利位（0 = 不要求）。
    pub fn get(
        &self,
        h: Handle,
        ty: ObjType,
        need: Rights,
    ) -> Result<(&KernelObj, Rights), i32> {
        let (obj, rights) = self.get_raw(h, ty, need)?;
        // SAFETY: 槽在表、gen 匹配 → 指针有效；`&self` 下只读借用。
        Ok((&unsafe { &*obj.as_ptr() }.obj, rights))
    }

    /// 1.1/S3-b：本表在册 pager 义务数——`Used` 槽持 demand VMO 且带
    /// `RIGHT_SET_POLICY` 的计数（0x21 门权与 demand 属性合取 = 真 pager 面）。
    /// `process_suspend` 消费（D6：冻 pager = 缺页者永久阻塞）。扫描派生而非
    /// sidecar 计数：close/take/drain/handle 迁移天然随表走，无漏减/UAF。
    pub fn pager_load(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| match s {
                SlotState::Used { obj, rights, .. } => {
                    *rights & RIGHT_SET_POLICY != 0
                        && unsafe {
                            (*obj.as_ptr()).obj.as_vmo().map_or(false, |v| v.is_demand())
                        }
                }
                SlotState::Free { .. } => false,
            })
            .count()
    }

    /// 本表是否持有 `obj`（ObjRef 指针相等）且 rights 含 `any` 中**任一位**
    /// （4-3b PI 图推导：受捐者 = 持有 channel peer 端点且带解锁 right 的阻塞
    /// 任务——等 WRITABLE 找带 RECV 者、等 READABLE 找带 SEND 者，任一解锁
    /// 动作可用即构成捐赠边）。遍历全部槽，v1 表个位数槽——O(槽) 可忽略。
    pub fn holder_has(&self, obj: NonNull<ObjRef>, any: Rights) -> bool {
        if any == 0 || self.slots.is_empty() {
            return false;
        }
        self.slots.iter().any(|s| match s {
            SlotState::Used { obj: o, rights, .. } => o == &obj && rights & any != 0,
            SlotState::Free { .. } => false,
        })
    }

    /// AM-1.5 wake-pending 冻结面：枚举本表全部 Channel 端点打「属主冻结」
    /// 标（`ChannelEnd.frozen_owner` = `owner` + retain——配平在
    /// [`Self::thaw_channel_ends`] / 端点 Drop 防御面）。**幂等**：已标端点
    /// 不动——同表 duplicate 别名（多槽共对象）不重复 retain。调用点 =
    /// `process_suspend` 首枚 token 冻结生效后（首层才标，嵌套层不重复）；
    /// 冻进程任务全冻 ⇒ 本表此后不可变 ⇒ 标集到 thaw 恒稳定。send 命中
    /// 冻端点 → 属主 wake-pending + token 持有者通知（`sys_channel_send` 尾）。
    pub fn freeze_channel_ends(&mut self, owner: NonNull<ObjRef>) {
        for slot in self.slots.iter_mut() {
            if let SlotState::Used { obj, .. } = *slot {
                // SAFETY: Used 槽持引用（对象活）；单核 SVC 串行（调用点
                // IRQ 屏蔽）；mark 幂等（同对象多槽别名不重复 retain）。
                unsafe {
                    if let Some(end) = (*obj.as_ptr()).obj.as_channel_mut() {
                        end.mark_frozen(owner);
                    }
                }
            }
        }
    }

    /// AM-1.5 wake-pending 清面（thaw 侧）：清全部由 `owner` 打的端点冻标
    ///（release 与 [`Self::freeze_channel_ends`] 的 retain 配平——mark 幂等 ⇒
    /// 同表 duplicate 别名只有首槽见 Some，恰一次配对）。调用点 =
    /// `SuspendTok::drop` **全解冻**（freeze_depth 归零）+ `process_suspend`
    /// 插表失败回滚（首层撤冻同点）。枚举期间属主 ObjRef 恒活：调用方
    ///（token Drop / 回滚序）此刻仍持 ≥1 引用——本面内的 release 只是退还
    /// mark 时的份额，归零销毁不可能发生在枚举中途。
    pub fn thaw_channel_ends(&mut self, owner: NonNull<ObjRef>) {
        for slot in self.slots.iter_mut() {
            if let SlotState::Used { obj, .. } = *slot {
                // SAFETY: 同 freeze_channel_ends（在表活对象 + 单核串行）；
                // clear 的 release 配平与属主存活论证见函数注释。
                unsafe {
                    if let Some(end) = (*obj.as_ptr()).obj.as_channel_mut() {
                        end.clear_frozen(owner);
                    }
                }
            }
        }
    }

    /// 在用（Used 态）槽计数（B-0 churn 金丝雀：全关光后必须归零——确定性
    /// 严格断言的判据，非热路径）。
    pub fn used_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| matches!(s, SlotState::Used { .. }))
            .count()
    }

    /// **完整性走查**（S-5 相 7 HT 风暴终账 + 任意静默点可用）：全表扫描验证
    /// 结构不变量，违规返 `Err`（定位描述）——**零分配、panic-free**：调用方
    /// 打专用 FAIL 行（`with_print_lock` 整段包络）+ 受控停机，**不裸 panic**
    /// （SMP 下 panic handler 无锁打印 + halt 本核，对核仍在跑 = 撕行风险）。
    ///
    /// 不变量（违反即表已腐败——并发竞态的真病灶面）：
    /// 1. 槽 0 恒 `Free`（handle 0 = `BERRY_HANDLE_INVALID` 保留位）；
    /// 2. `Free.next_gen ≥ 1`（出生即 1，close/take/drain 只增不减；0 = 写坏。
    ///    `> GEN_MASK` 合法——gen 耗尽槽由 [`Self::alloc_slot`] 跳过驻留）；
    /// 3. `Used.gen ∈ [1, GEN_MASK]`（[`Self::take_gen`] 只发行此域值）；
    /// 4. `Used` 槽指向的 `ObjRef.refs ≥ 1`（0 = 引用归零对象已释放但槽仍
    ///    Used = UAF——并发 close/drain 竞态在此暴露）。
    ///
    /// 调用契约：与表的其他访问同序——syscall 表类临界窗内，或 IRQ 屏蔽静默
    /// 点（风暴前基线 / fleet join 后终账）。返回 `Ok(在用槽数)` 供调用方
    /// 对账（走查计数独立累计，不复用 [`Self::used_count`] 的遍历）。
    pub fn integrity_check(&self) -> Result<usize, &'static str> {
        // 零分配出生态：无槽可查（槽 0 尚未惰性铺出）。
        let Some(first) = self.slots.first() else {
            return Ok(0);
        };
        if !matches!(first, SlotState::Free { .. }) {
            return Err("HT integrity: slot 0 not Free");
        }
        let mut used = 0usize;
        for s in &self.slots {
            match s {
                SlotState::Free { next_gen } => {
                    if *next_gen == 0 {
                        return Err("HT integrity: Free next_gen=0");
                    }
                }
                SlotState::Used { gen, obj, .. } => {
                    if !gen_in_issue_domain(*gen) {
                        return Err("HT integrity: Used gen out of range");
                    }
                    // SAFETY: 槽在表且 gen 在发行域内 → 按表不变量指针有效；
                    // 本走查职责恰是探测不变量破缺（refs==0 即 UAF 信号），
                    // 读面止于诊断取值（usize），无派生解引用。
                    if unsafe { (*obj.as_ptr()).refs() } == 0 {
                        return Err("HT integrity: Used refs=0 (UAF)");
                    }
                    used += 1;
                }
            }
        }
        Ok(used)
    }

    /// [`Self::get`] 的裸指针底层：查表 → 类型 → 权限后返回对象引用指针 + 权利。
    /// **可变访问的正规入口**（syscall 写路径 `as_*_mut`）与**双对象同借**
    /// （`vmar_map` 的 vmar + vmo）用——两个 `get_raw` 指针类型门控互异 ⇒ 必指
    /// 不同对象 ⇒ `&mut` 无别名（同 handle 值不可能既是 Vmar 又是 Vmo；
    /// duplicate 不跨类型）。
    pub fn get_raw(
        &self,
        h: Handle,
        ty: ObjType,
        need: Rights,
    ) -> Result<(NonNull<ObjRef>, Rights), i32> {
        let (obj, rights) = self.lookup(h)?;
        // SAFETY: 槽在表、gen 匹配 → 指针有效；类型/权限只读探测。
        let r = unsafe { &*obj.as_ptr() };
        if r.obj.obj_type() != ty {
            return Err(ERR_BAD_HANDLE); // 类型不符按坏 handle（ABI §8.2 注）
        }
        if need != 0 && rights & need != need {
            return Err(ERR_ACCESS_DENIED);
        }
        Ok((obj, rights))
    }

    /// **类型无关权利探针**（P3-1 `handle_duplicate` 执法面）：查表 + 代号
    /// 校验后返回该 handle 的权利全集。不验类型、不验 need 位——
    /// `handle_duplicate` 对一切对象类型开放，`get_raw` 的具体 `ObjType`
    /// 门在此不适配；need 位判定归调用方（syscall 入口查
    /// [`RIGHT_DUPLICATE`]）。执法边界 = syscall 面（不受信面）：内核内部
    /// 直调 [`Self::duplicate`]（boot 插表 / ktest 表 API）是授能语境，
    /// 不经此门。
    pub fn rights_of(&self, h: Handle) -> Result<Rights, i32> {
        Ok(self.lookup(h)?.1)
    }

    /// **wait 面取对象**（W3-Q91——`wait_get` 五类型链折叠为单查）：查表 +
    /// 代号校验出对象槽，一次 `obj_type()` 判 waitable（词表单源
    /// `berry_hdlface::probe::waitable`，host 钉板）。词表外类型 →
    /// [`ERR_BAD_HANDLE`]（与旧五链「全拒回首个（Event 的）错误」语义逐位
    /// 一致——need=0 下旧链的拒源只有 -EBADF 一种，类型不符即该码）。
    /// 旧形态每枚 2-5 次完整查表（每个类型一次 lookup + ObjRef 解引用 +
    /// 类型比对），本面 1 次（02 号 IPC 审计机会四）。
    pub fn get_waitable(&self, h: Handle) -> Result<NonNull<ObjRef>, i32> {
        let (obj, _) = self.lookup(h)?;
        // SAFETY: 槽在表、gen 匹配 → 指针有效；类型只读探测。
        let r = unsafe { &*obj.as_ptr() };
        if !berry_hdlface::probe::waitable(r.obj.obj_type() as u8) {
            return Err(ERR_BAD_HANDLE); // 词表外 = 类型不符按坏 handle（ABI §8.2 注）
        }
        Ok(obj)
    }

    /// 查表（gen 校验）：槽空 / 越界 / 代号不符 / **预留位非零** →
    /// [`ERR_BAD_HANDLE`]。
    fn lookup(&self, h: Handle) -> Result<(NonNull<ObjRef>, Rights), i32> {
        if has_reserved_bits(h) {
            return Err(ERR_BAD_HANDLE); // 预留位非零 = 非法值（v1 从不发行）
        }
        let slot = decode_slot(h) as usize;
        match self.slots.get(slot) {
            Some(SlotState::Used { gen, obj, rights }) if *gen == decode_gen(h) => {
                Ok((*obj, *rights))
            }
            _ => Err(ERR_BAD_HANDLE),
        }
    }

    /// 探活（3-4b `channel_send` 预检）：handle 在表且代号匹配即活——不验类型/
    /// 权限（随行迁移的是对象令牌本身，资格随槽走）。供「先验全部、后 take」
    /// 的原子序：任一预检失败时调用者表零变化。
    pub fn alive(&self, h: Handle) -> bool {
        self.lookup(h).is_ok()
    }

    /// 1.1/S5.12：按类型计数在册活 handle（token 上限执法面）。
    /// O(n) 只扫 Used 槽——64 槽小表，SVC 路径可接受。
    pub fn count_type(&self, ty: ObjType) -> usize {
        let mut n = 0;
        for slot in &self.slots {
            if let SlotState::Used { obj, .. } = slot {
                // SAFETY: Used 槽持有引用，obj_type 只读。
                if unsafe { (*obj.as_ptr()).obj.obj_type() == ty } {
                    n += 1;
                }
            }
        }
        n
    }

    /// 同 [`Self::lookup`]，另带 (槽位下标, gen)（close/replace 改槽用）。
    fn lookup_at(
        &self,
        h: Handle,
    ) -> Result<(usize, u32, NonNull<ObjRef>, Rights), i32> {
        if has_reserved_bits(h) {
            return Err(ERR_BAD_HANDLE); // 预留位非零 = 非法值（v1 从不发行）
        }
        let slot = decode_slot(h) as usize;
        match self.slots.get(slot) {
            Some(SlotState::Used { gen, obj, rights }) if *gen == decode_gen(h) => {
                Ok((slot, *gen, *obj, *rights))
            }
            _ => Err(ERR_BAD_HANDLE),
        }
    }

    /// 取最低可用空槽下标（槽 0 永久保留给 `BERRY_HANDLE_INVALID`，从 1 起找；
    /// **跳过代号耗尽的槽**——`next_gen > GEN_MASK` 的空槽过不了 `take_gen`，
    /// 选中即白堵一个插入位，3b3 审查 H3；无可用槽且未满则尾插）。
    fn alloc_slot(&mut self) -> Result<usize, i32> {
        // 惰性铺保留槽 0（表出生零分配——首插才落 0 号 Free 槽，3b3 H2）。
        if self.slots.is_empty() {
            self.slots.try_reserve(1).map_err(|_| ERR_NO_MEMORY)?;
            self.slots.push(SlotState::Free { next_gen: 1 }); // 槽 0 恒空 = INVALID
        }
        if let Some(off) = self
            .slots
            .iter()
            .skip(1)
            .position(|s| matches!(s, SlotState::Free { next_gen } if *next_gen <= GEN_MASK))
        {
            return Ok(off + 1);
        }
        if !slot_index_valid(self.slots.len()) {
            return Err(ERR_NO_MEMORY); // 表满（65536 槽）
        }
        // 尾插前预留（3b3 H2 try 化：push 增长的 OOM 曾是 abort 点）。
        self.slots.try_reserve(1).map_err(|_| ERR_NO_MEMORY)?;
        self.slots.push(SlotState::Free { next_gen: 1 });
        Ok(self.slots.len() - 1)
    }

    /// 空槽取出 `next_gen` 作本槽代号。gen 耗尽（> [`GEN_MASK`]）拒绝——
    /// 防回卷复活旧 handle（14 位 ≈ 1.6 万次重用；EL0 循环 create/close 可打满
    /// 单槽，耗尽槽由 [`Self::alloc_slot`] 跳过、不再堵插入——3b3 审查 H3）。
    fn take_gen(&mut self, slot: usize) -> Result<u32, i32> {
        match self.slots[slot] {
            SlotState::Free { next_gen } => {
                debug_assert!(next_gen >= 1);
                if !gen_in_issue_domain(next_gen) {
                    return Err(ERR_NO_MEMORY);
                }
                Ok(next_gen)
            }
            SlotState::Used { .. } => unreachable!("take_gen on used slot"),
        }
    }
}
