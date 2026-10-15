//! handle 表 + rights + 内核对象壳（`能力模型设计.md` §2/§4；决策 #28 位域）。
//!
//! Phase 3-0：纯内核层——尚无 syscall 接线（3-1 起 v1 27 号经此门控），本步
//! ktest `HANDLE ok` 验证语义，外部行为零变化（语义用例见 `ktest::test_handle`；
//! 内核 bin 是 no_std 无 host test 入口，bos-mm 式 host 单测不适用）。
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

/// handle（`bos_handle_t`，u32；位域见模块注释）。
pub type Handle = u32;
/// rights（`bos_rights_t`，u64 位掩码）。
pub type Rights = u64;

// ---- rights 位（`系统调用ABI设计.md` §8.2，数值冻结；镜像 bos-sys）----
// EXECUTE/MAP/… 消费者在 3-2（vmo_map 门控）/3-4（channel/event）落地——
// 位表是 ABI 冻结值，先全量定义（内核 bin 无 cfg(test)，dead_code 需显式豁免）。
pub const RIGHT_READ: Rights = 1 << 0;
pub const RIGHT_WRITE: Rights = 1 << 1;
#[allow(dead_code)] // 消费者 3-2/3-4 落地（ABI 冻结位，先全量定义）
pub const RIGHT_EXECUTE: Rights = 1 << 2;
#[allow(dead_code)] // 消费者 3-2/3-4 落地（ABI 冻结位，先全量定义）
pub const RIGHT_MAP: Rights = 1 << 3;
pub const RIGHT_DUPLICATE: Rights = 1 << 4;
#[allow(dead_code)] // 消费者 3-2/3-4 落地（ABI 冻结位，先全量定义）
pub const RIGHT_TRANSFER: Rights = 1 << 5;
#[allow(dead_code)] // 消费者 3-2/3-4 落地（ABI 冻结位，先全量定义）
pub const RIGHT_INSPECT: Rights = 1 << 6;
#[allow(dead_code)] // 消费者 3-2/3-4 落地（ABI 冻结位，先全量定义）
pub const RIGHT_SIGNAL: Rights = 1 << 7;
#[allow(dead_code)] // 消费者 3-2/3-4 落地（ABI 冻结位，先全量定义）
pub const RIGHT_SEND: Rights = 1 << 8;
#[allow(dead_code)] // 消费者 3-2/3-4 落地（ABI 冻结位，先全量定义）
pub const RIGHT_RECV: Rights = 1 << 9;
#[allow(dead_code)] // 消费者 3-2/3-4 落地（ABI 冻结位，先全量定义）
pub const RIGHT_MANAGE: Rights = 1 << 10;
pub const RIGHT_SET_POLICY: Rights = 1 << 11;
/// `handle_duplicate`「取原权利全集」哨兵（= `BOS_RIGHT_SAME_RIGHTS`，全 1）。
pub const RIGHTS_SAME: Rights = u64::MAX;

// ---- 错误码（ABI §8.2 数值冻结，posix-ish 负数；3-1 syscall 层共用）----

/// EBADF：坏 handle / 类型不符 / 代号过期。
pub const ERR_BAD_HANDLE: i32 = -9;
/// EACCES：权限位不足（能力拒绝）。
pub const ERR_ACCESS_DENIED: i32 = -13;
/// EINVAL：参数非法（如 replace 两值相同）。
pub const ERR_INVALID_ARG: i32 = -22;
/// ENOMEM：无空间（表满 / gen 耗尽 / 超尺寸）。
pub const ERR_NO_MEMORY: i32 = -12;
/// ENOTSUP：未知/未实现编号或能力（ABI §8.4「永不 panic」统一错误面）。
pub const ERR_NOT_SUPPORTED: i32 = -95;
/// EFAULT：用户缓冲不可访问（未映射/只读页要写/越出用户区）——3-2 copy 路径。
pub const ERR_FAULT: i32 = -14;
/// EBUSY：资源占用（映射未解除的 VMO 不能 close/decommit——悬空 PTE 防线）。
pub const ERR_BUSY: i32 = -16;
/// ENOENT：区间不存在（unmap/protect 找不到精确匹配）。
pub const ERR_NOT_FOUND: i32 = -2;
/// EBADSTATE：对象状态不满足（读/映射未提交页——v1 无 demand paging）。
pub const ERR_BAD_STATE: i32 = -37;
/// EOVERFLOW-style：offset/len 越出对象容量或区间管辖界。
pub const ERR_OUT_OF_RANGE: i32 = -34;

// ---- handle 位域（决策 #28：低位槽位、中位代号、高位可选类型）----

/// 槽位号位宽（低 16 位 = 每表最多 65536 槽）。
const SLOT_BITS: u32 = 16;
const SLOT_MASK: u32 = (1 << SLOT_BITS) - 1;
/// 代号位宽（bits [16, 30)；bit31 预留类型编码，v1 不写）。
const GEN_BITS: u32 = 14;
const GEN_MASK: u32 = (1 << GEN_BITS) - 1;

fn encode(slot: u32, gen: u32) -> Handle {
    debug_assert!(slot <= SLOT_MASK && gen >= 1 && gen <= GEN_MASK);
    (gen << SLOT_BITS) | slot
}

fn decode_slot(h: Handle) -> u32 {
    h & SLOT_MASK
}

fn decode_gen(h: Handle) -> u32 {
    (h >> SLOT_BITS) & GEN_MASK
}

/// 对象类型（镜像 `bos_obj_type` 枚举值，ABI §8.2 冻结）。
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
}

/// 内核对象壳（Zircon 务实派，ADR #4：对象在内核、用户态只见整数 handle）。
/// v1 先两种（3-3/3-4 增补 Process/Thread/Channel/Event）。
#[allow(dead_code)] // Vmar 变体的首个构造点随 3-2 vmar_create 接线
pub enum KernelObj {
    Vmo(crate::vmo::Vmo),
    Vmar(crate::vmar::Vmar),
}

impl KernelObj {
    pub fn obj_type(&self) -> ObjType {
        match self {
            KernelObj::Vmo(_) => ObjType::Vmo,
            KernelObj::Vmar(_) => ObjType::Vmar,
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
}

/// 共享对象体：引用计数 + 对象本体。`Box` 进堆、槽持裸指针；引用计数归零
/// 的 drop 是本模块唯一释放点（unsafe 收敛：指针有效性由表全程维护）。
pub struct ObjRef {
    /// 持有本对象的 handle 数（dup +1 / close -1）。
    refs: usize,
    pub obj: KernelObj,
}

/// 槽状态：空槽记住 `next_gen`（重用时代号单调 +1——旧 handle 值自然作废）。
enum SlotState {
    Free { next_gen: u32 },
    Used { gen: u32, obj: NonNull<ObjRef>, rights: Rights },
}

/// handle 表（能力模型 §4：每进程一张；3-0 先内核自用，3-2 起挂进程）。
///
/// 单核顺序访问（v1 无 SMP；IRQ 上下文不碰表——syscall 均同步异常路径）。
pub struct HandleTable {
    /// 槽 0 恒 `Free`（handle 0 = `BOS_HANDLE_INVALID`）。
    slots: Vec<SlotState>,
}

impl HandleTable {
    /// 建空表（槽 0 保留）。
    pub fn new() -> Self {
        let mut slots = Vec::with_capacity(8);
        slots.push(SlotState::Free { next_gen: 1 });
        Self { slots }
    }

    /// 放入对象并授予 rights，返回新 handle（取最低空槽）。
    pub fn insert(&mut self, obj: KernelObj, rights: Rights) -> Result<Handle, i32> {
        // SAFETY: 刚 into_raw 的裸指针，非空。
        let obj = unsafe {
            NonNull::new_unchecked(Box::into_raw(Box::new(ObjRef { refs: 1, obj })))
        };
        let slot = self.alloc_slot()?;
        let gen = self.take_gen(slot)?;
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
        // SAFETY: lookup 已验指针在表且 refs ≥ 1；单核顺序访问。
        unsafe { (*obj.as_ptr()).refs += 1 };
        let slot = self.alloc_slot()?;
        let gen = self.take_gen(slot)?;
        self.slots[slot] = SlotState::Used { gen, obj, rights: new_rights };
        Ok(encode(slot as u32, gen))
    }

    /// 关闭 handle：引用 -1，归零则释放对象。旧 handle 值此后必
    /// [`ERR_BAD_HANDLE`]（槽 `next_gen` 已 +1）。
    ///
    /// 3-2 拒绝面：**本次 close 会销毁独体（refs==1）且 VMO 映射未解除 → 拒**
    /// （-EBUSY）——归零销毁 Vmo 有两个后患：Vmar Region 存着该对象指针（unmap
    /// 回调映射计数）→ 悬空；仍被映射的物理页若归还 → PA 复用经残留 PTE 静默
    /// 腐败。**别名 close（refs≥2）放行**——对象不死，存活句柄继续钉住它；
    /// 先 unmap（或先 close 别名）再 close 最后一个。Vmar 带 live 映射时 close
    /// 只留残留 PTE 与泄漏页（v1 已接受政策），不拒。
    pub fn close(&mut self, h: Handle) -> Result<(), i32> {
        let (slot, gen, obj, _) = self.lookup_at(h)?;
        // SAFETY: 指针在表、refs ≥ 1；只读探测（close 拒绝面——只拦销毁性 close）。
        unsafe {
            let r = &*obj.as_ptr();
            if r.refs == 1 {
                if let KernelObj::Vmo(v) = &r.obj {
                    if v.any_mapped() {
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
        // 同 close 拒绝面：h 的对象**销毁性**（refs==1）且映射未解除时结尾的
        // close 段会拒——先拒，保证「任一拒绝则表不动」的原子承诺（别名 replace
        // 不销毁对象，放行；repl 对象只搬迁不销毁，无需查）。
        // SAFETY: 指针在表（lookup_at 过）；只读探测。
        unsafe {
            let r = &*hobj.as_ptr();
            if r.refs == 1 {
                if let KernelObj::Vmo(v) = &r.obj {
                    if v.any_mapped() {
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

    /// 查表（gen 校验）：槽空 / 越界 / 代号不符 → [`ERR_BAD_HANDLE`]。
    fn lookup(&self, h: Handle) -> Result<(NonNull<ObjRef>, Rights), i32> {
        let slot = decode_slot(h) as usize;
        match self.slots.get(slot) {
            Some(SlotState::Used { gen, obj, rights }) if *gen == decode_gen(h) => {
                Ok((*obj, *rights))
            }
            _ => Err(ERR_BAD_HANDLE),
        }
    }

    /// 同 [`Self::lookup`]，另带 (槽位下标, gen)（close/replace 改槽用）。
    fn lookup_at(
        &self,
        h: Handle,
    ) -> Result<(usize, u32, NonNull<ObjRef>, Rights), i32> {
        let slot = decode_slot(h) as usize;
        match self.slots.get(slot) {
            Some(SlotState::Used { gen, obj, rights }) if *gen == decode_gen(h) => {
                Ok((slot, *gen, *obj, *rights))
            }
            _ => Err(ERR_BAD_HANDLE),
        }
    }

    /// 取最低空槽下标（槽 0 永久保留给 `BOS_HANDLE_INVALID`，从 1 起找；
    /// 无空槽且未满则尾插）。
    fn alloc_slot(&mut self) -> Result<usize, i32> {
        if let Some(off) = self
            .slots
            .iter()
            .skip(1)
            .position(|s| matches!(s, SlotState::Free { .. }))
        {
            return Ok(off + 1);
        }
        if self.slots.len() > SLOT_MASK as usize {
            return Err(ERR_NO_MEMORY); // 表满（65536 槽）
        }
        self.slots.push(SlotState::Free { next_gen: 1 });
        Ok(self.slots.len() - 1)
    }

    /// 空槽取出 `next_gen` 作本槽代号。gen 耗尽（> [`GEN_MASK`]）拒绝——
    /// 防回卷复活旧 handle（14 位 ≈ 1.6 万次重用，v1 实际打不满）。
    fn take_gen(&mut self, slot: usize) -> Result<u32, i32> {
        match self.slots[slot] {
            SlotState::Free { next_gen } => {
                debug_assert!(next_gen >= 1);
                if next_gen > GEN_MASK {
                    return Err(ERR_NO_MEMORY);
                }
                Ok(next_gen)
            }
            SlotState::Used { .. } => unreachable!("take_gen on used slot"),
        }
    }
}
