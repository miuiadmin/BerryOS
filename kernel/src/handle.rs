//! handle 表 + rights + 内核对象壳（`能力模型设计.md` §2/§4；决策 #28 位域）。
//!
//! Phase 3-0：纯内核层——尚无 syscall 接线（3-1 起 v1 27 号经此门控），本步
//! ktest `HANDLE ok` 验证语义，外部行为零变化（语义用例见 `ktest::test_handle`；
//! 内核 bin 是 no_std 无 host test 入口，berry-mm 式 host 单测不适用）。
//!
//! - handle = **槽位号[15:0] | 代号 gen[29:16]**（bit31 预留类型编码，v2 可选，
//!   `系统调用ABI设计.md` §8.1）。`0` = 非法（槽 0 恒空）。
//! - **代号 v1 预留**：gen 从 1 起、槽位重用单调 +1；旧 gen 值查表 →
//!   [`ERR_BAD_HANDLE`]。v3 启用 DEAD 检测后，对象死亡主动作废代号 → 改返
//!   `ERR_DEAD`（「死活感知」与「权限派生」分工见能力模型 §2）。
//! - **权限在 handle 不在对象**：每槽自带 rights；对象本体由 [`ObjRef`] 共享
//!   （引用计数 = 持有它的 handle 数，最后 close 释放）。
//! - 权限只能收紧：`duplicate` 要求新 rights ⊆ 原（防提权，永不增权）。
//!
//! v1 裁剪：页表帧不回收（v1 政策）；`RIGHT_DUPLICATE` 位已定义但不门控
//! `duplicate`（Zircon 同款：位留 vocab、暂不执法）。3-2 起 Vmo 销毁回收
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
    RIGHT_TRANSFER, RIGHT_WRITE, SIGNAL_READABLE, SIGNAL_TERMINATED, SIGNAL_USER_MASK,
    SIGNAL_WAKE_PENDING,
    SIGNAL_WRITABLE,
};

// ---- handle 位域（决策 #28：低位槽位、中位代号、高位可选类型）----
// 常量与 encode/decode 单源在 berry-hdlface（W3-Q8 host 钉板——位域编码
// 律/别名拒/发行域门表驱动在册）；此处 re-export 维持 kernel 内路径。
pub use berry_hdlface::{
    decode_gen, decode_slot, encode, gen_in_issue_domain, has_reserved_bits,
    slot_index_valid, GEN_BITS, GEN_MASK, RESERVED_MASK, SLOT_BITS, SLOT_MASK,
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

/// 共享对象体：引用计数 + 对象本体。`Box` 进堆、槽持裸指针；引用计数归零
/// 的 drop 是本模块唯一释放点（unsafe 收敛：指针有效性由表全程维护）。
pub struct ObjRef {
    /// 持有本对象的引用数（handle 计 + **内部钉住计**，见 [`ObjRef::retain`]）。
    refs: usize,
    pub obj: KernelObj,
}

impl ObjRef {
    /// **内部钉住**（3-3）：对象间跨界引用（Thread→Process、Process→root
    /// Vmar、运行中任务→Process）+1 引用——持有者 handle 关光后被引用对象
    /// 仍存活。`retain`/`release` 成对，与 handle 计共用同一归零释放点。
    pub fn retain(&mut self) {
        self.refs += 1;
    }

    /// 钉住释放：-1，归零则销毁（与 close 归零同路）。Drop 链安全：内部钉
    /// 住的对象销毁时其 Drop 再 release 它钉住的对象（纯指针操作，不回表，
    /// 无重入）。
    ///
    /// # Safety
    /// `ptr` 须来自一次成功的 `retain`（表内活对象），且此后不再使用。
    pub unsafe fn release(ptr: NonNull<Self>) {
        // SAFETY: 调用方契约（retain 过、单核顺序——对象仅在 IRQ 屏蔽路径销毁）。
        let r = &mut *ptr.as_ptr();
        r.refs -= 1;
        if r.refs == 0 {
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
        // 装箱 try 化（3b3 H2）：失败时槽仍 Free 未写、gen 未发行——零副作用
        // 返 -ENOMEM（原 Box::new OOM 直接 abort）。
        let Some(boxed) = crate::heap::try_box(ObjRef { refs: 1, obj }) else {
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
        // SAFETY: lookup 已验指针在表且 refs ≥ 1；单核顺序访问。
        unsafe { (*obj.as_ptr()).refs += 1 };
        self.slots[slot] = SlotState::Used { gen, obj, rights: new_rights };
        Ok(encode(slot as u32, gen))
    }

    /// 关闭 handle：引用 -1，归零则释放对象。旧 handle 值此后必
    /// [`ERR_BAD_HANDLE`]（槽 `next_gen` 已 +1）。
    ///
    /// 3-2/G4-2 拒绝面：**本次 close 会销毁独体（refs==1）且 VMO 映射未解除
    /// 或 demand 区间仍登记 → 拒**（-EBUSY，判据 `any_mapped() ||
    /// demand_mapped()`）——归零销毁 Vmo 有两个后患：Vmar Region 存着该对象
    /// 裸指针（unmap 回调映射计数/供给链解引用）→ 悬空（demand 分量 = G4-2
    /// 修批 P0-1 面 a/c：未供给页 mapcnt 恒 0 而指针已活，独立钉）；仍被映射
    /// 的物理页若归还 → PA 复用经残留 PTE 静默腐败。**别名 close（refs≥2）
    /// 放行**——对象不死，存活句柄继续钉住它；先 unmap（或先 close 别名）
    /// 再 close 最后一个。Vmar 带 live 映射时 close 不拒（v1 政策维持——
    /// G4-2 修批起由 `Vmar::Drop` 收口：注销 demand 登记槽 + 放 demand 钉，
    /// 不产悬垂）。
    pub fn close(&mut self, h: Handle) -> Result<(), i32> {
        let (slot, gen, obj, _) = self.lookup_at(h)?;
        // SAFETY: 指针在表、refs ≥ 1；只读探测（close 拒绝面——只拦销毁性 close）。
        unsafe {
            let r = &*obj.as_ptr();
            if r.refs == 1 {
                if let KernelObj::Vmo(v) = &r.obj {
                    if v.any_mapped() || v.demand_mapped() {
                        return Err(ERR_BUSY);
                    }
                }
            }
        }
        // SAFETY: 指针在表、refs ≥ 1；单核顺序访问。
        let refs = unsafe {
            let r = &mut *obj.as_ptr();
            r.refs -= 1;
            r.refs
        };
        if refs == 0 {
            // SAFETY: 引用已归零、无别名——归还 Box（drop Vmo/Vmar 本体）。
            unsafe { drop(Box::from_raw(obj.as_ptr())) };
        }
        self.slots[slot] = SlotState::Free { next_gen: gen + 1 };
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
        // 同 close 拒绝面（含 demand 钉分量，G4-2 修批）：h 的对象**销毁性**
        // （refs==1）且映射未解除/demand 仍登记时结尾的 close 段会拒——先拒，
        // 保证「任一拒绝则表不动」的原子承诺（别名 replace 不销毁对象，放行；
        // repl 对象只搬迁不销毁，无需查）。
        // SAFETY: 指针在表（lookup_at 过）；只读探测。
        unsafe {
            let r = &*hobj.as_ptr();
            if r.refs == 1 {
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
    /// 消费者：`process_start` 的 bootstrap 迁移（3-4a）、`channel_send` 的
    /// handle 随消息迁移（3-4b）。取出后对象由调用方经 [`Self::insert_ref`]
    /// 落进目标表（或暂挂消息里）。**不做销毁性检查**（refs 不减、无 drop——
    /// 映射中 VMO 照样可迁：对象不死，EPTE 防线由最终 close 把守）。
    pub fn take(&mut self, h: Handle) -> Result<(NonNull<ObjRef>, Rights), i32> {
        let (slot, gen, obj, rights) = self.lookup_at(h)?;
        self.slots[slot] = SlotState::Free { next_gen: gen + 1 };
        Ok((obj, rights))
    }

    /// **放入既有对象**（[`Self::take`] 的对侧）：已 Boxing 的 [`ObjRef`] 挂进
    /// 新槽并授予 rights，返回新 handle。引用数**不变**——调用方移交的引用
    /// 令牌由本槽接管（与 [`Self::insert`] 的差别：insert 新建对象 refs=1，
    /// insert_ref 接管既有令牌）。
    pub fn insert_ref(&mut self, obj: NonNull<ObjRef>, rights: Rights) -> Result<Handle, i32> {
        let slot = self.alloc_slot()?;
        let gen = self.take_gen(slot)?;
        self.slots[slot] = SlotState::Used { gen, obj, rights };
        Ok(encode(slot as u32, gen))
    }

    /// **清空整表**（Plan B v0.5：进程退出钩子 drain 死亡进程的表）。逐槽走
    /// [`Self::close`] 的释放语义（引用 -1、归零 drop Box、槽代号 +1），但
    /// **不做销毁性拒检**——close 的 -EBUSY 拒面保护的是**活进程**的映射
    /// 一致性；表主已死，其表内映射中 VMO 的页由 `Vmo::Drop` 的「映射中页
    /// 只泄漏」政策兜底（安全向：不还帧 = 无 PA 复用经残留 PTE 腐败）。
    /// **对象本体亦不销毁**（对抗审查修）：`Region.vmo` 裸指针的有效性依托
    /// 「close 拒 -EBUSY 保对象存活」（vmar.rs Region 注释）——drain 无拒
    /// 面，照常 drop Box 会让存活 Vmar 的 unmap/protect 解引用悬空（UAF 写）
    /// ；故 refs 将归零且仍被映射 **或 demand 登记中** 的 VMO 整档跳过递减
    /// （令牌随死表泄漏、对象保活；mapcnt==0 且 demand 钉==0 ⟺ 无 Region
    /// 指着它——两防线判据并集，精确；demand 分量 = G4-2 修批 P0-1）。Vmar
    /// 对象照常 drop：其 `Drop` 注销 demand 登记槽 + 放自身 regions 的 demand
    /// 钉（KL 窗外语境由 kl_held 门收口）。drop 链纯指针操作不回表（无重入）
    /// ；**仅 IRQ 屏蔽的退出路径调用**（对象释放窗口内不可被切走）。
    pub fn drain(&mut self) {
        for slot in self.slots.iter_mut() {
            if let SlotState::Used { gen, obj, .. } = *slot {
                // SAFETY: 指针在表；只读探测（any_mapped/demand_mapped 零分配）。
                let leak = unsafe {
                    let r = &*obj.as_ptr();
                    r.refs == 1
                        && match &r.obj {
                            KernelObj::Vmo(v) => v.any_mapped() || v.demand_mapped(),
                            _ => false,
                        }
                };
                if !leak {
                    // SAFETY: 指针在表、refs ≥ 1；单核顺序（屏蔽窗内）。
                    let refs = unsafe {
                        let r = &mut *obj.as_ptr();
                        r.refs -= 1;
                        r.refs
                    };
                    if refs == 0 {
                        // SAFETY: 引用归零、无别名——归还 Box（drop 对象本体）。
                        unsafe { drop(Box::from_raw(obj.as_ptr())) };
                    }
                }
                *slot = SlotState::Free { next_gen: gen + 1 };
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
                    if unsafe { (*obj.as_ptr()).refs } == 0 {
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
