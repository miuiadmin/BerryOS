//! berry-wakeface —— 唤醒归因链纯函数面（W3-Q66）。
//!
//! **动机**：刀1（PM-17 §5）per-CPU 唤醒归因语境 cell 的判定律——GIC INTID
//! → 唤醒源映射（[`src_of`]/现 HAL `wake::src_of`）、IRQ enter 保存值域判
//! （[`restore_valid`]——保存值必须是已定义源集成员）、槽位读取钳位
//! （[`slot_index`]）——此前内联在 `arch/aarch64/src/wake.rs`，host 测不可达
//! （HAL crate no_std + MMIO 面）。号表的精细边界（未知号恒 UNKNOWN / SGI
//! 号表与 SPI/PPI 三段正交 / 保存值域 = 生成器已定义源集 / SPURIOUS 1023
//! 走早退不进号表）需要一张 host 可测的**钉板**：本 crate 把三律按值域抽成
//! 纯函数 + 表驱动测试，HAL wake.rs 消费同一函数（零第二实现——抽离即唯一
//! 事实源，非镜像）。
//!
//! 姊妹件 [`berry-wsrcface`]（W3-Q28）：wsrcface 钉「源值怎么分类/谁可进环/
//! 槽表怎么钳」，本面钉「IRQ 号怎么归源/保存恢复协议值域」——两件合成刀1
//! 归因链的完整 host 钉板（号 → 源 → 分类/进环）。
//!
//! 抽离面（与 HAL 侧逐字对齐，改这里的语义 = 改 HAL 行为）：
//! - [`src_of`]：GIC INTID → 唤醒源。板上五活锚（CNTV 27=TIMER / PL011 RX
//!   33 与 PL1 RX 40 双 UART 归并不拆=UART_RX / PL061 39=POWER_KEY）+ SGI
//!   双号（1=RESCHED、2=TLB——SGI 号段 0..16 与 SPI 段 32+ 正交，PPI 段
//!   16..32 板上无主）+ 未知号恒 UNKNOWN。SPURIOUS(1023) 不进号表——
//!   `exception_irq` 在 `wake_src_irq_enter` **之前**早退（零字节窗口），
//!   本函数收到 1023 也是未知臂（语义一致：恒 UNKNOWN，钉板防误加臂）。
//! - [`restore_valid`]：IRQ exit 恢复协议的值域判——保存值必须是**已定义
//!   源集**成员（九源全集见 [`DEFINED`]；保存值来自 enter 时的 cell 读，
//!   cell 写点三处全在控——非法值恢复 = cell 被外物写坏，fail-loud 面）。
//! - [`slot_index`]：槽位钳位读——`(src as usize).min(SLOTS - 1)`，与
//!   wsrcface::slot_index 同律（HAL 端 G1.5 快照合并消费点同款钳位，单源
//!   化防漂移）。0x80–0xFF 值钳到 127 号槽（UNKNOWN 端点槽），不越界
//!   不 panic。
//!
//! **源值常量不迁**（wsrcface 同款纪律）：`WAKE_SRC_*` 九值定义在 HAL
//! （`arch/aarch64/src/wake_gen.rs`——`tools/abi.toml` `[[wakesrc]]` 第五
//! 产物，add-only）。本面以**值域参数化**形态钉板：测试用 HAL 冻结值镜像
//! （0x00/0x01/…/0x07/0x7F——与 ktest WSRC-K A 端点锚同源），消费侧 HAL
//! 传自家常量。若 HAL 改值，本 crate 测试的镜像锚与 ktest 同时红——双面
//! 执法。号表锚同理：GIC INTID 常量（IRQ_CNTV 等）留 HAL gic.rs，本面
//! 参数化注入（`IrqMap::new(...)` 每锚显式），HAL 消费用真常量——号表
//! 改号时本面测试镜像红 + HAL 接线编译红双执法。
//!
//! 不在此 crate（留 HAL 壳）：per-CPU `CELL`（AtomicU8 static 数组——语境
//! cell 的原子读写物理臂）、`TLB_SEEN`/`UNKNOWN_SEEN`/`SGI_SEEN` 到达计数
//! （AtomicU32——G1.5 快照合并面）、`cpu_id` 拓扑槽收敛尾槽的防御臂——本面
//! 只钉「号怎么归源、保存值域怎么判、槽怎么钳」，不钉「原子怎么打」。

#![cfg_attr(not(test), no_std)]

/// 槽表槽数 = 值域上界 + 1（源值 ≤ 0x7F；值直索引——`table[src]`；与
/// wsrcface::SLOTS / kernel WAKE_SRC_SLOTS 三方同值，HAL const assert 执法）。
pub const SLOTS: usize = 128;

/// 值域上界（UNKNOWN 端点锚——HAL 冻结值 0x7F；测试镜像同源）。
pub const SRC_MAX: u8 = 0x7F;

// ---- HAL 冻结值镜像（wake_gen.rs 生成物——ktest WSRC-K A 端点锚同源；
// ---- 消费侧 HAL 传真常量，镜像只住测试）----

/// 哨兵：未接线语境（仅 CPU1 首任务切入前窗口可持有）。
pub const MIRROR_NONE: u8 = 0x00;
/// boot 期非 IRQ 语境（rust_main 直跑窗）。
pub const MIRROR_KERNEL_BOOT: u8 = 0x01;
/// CNTV PPI 27（tick 到期唤醒）。
pub const MIRROR_TIMER: u8 = 0x02;
/// PL011 SPI 33 / PL1 RX 40（控制台 RX 到达——双 UART 归并不拆）。
pub const MIRROR_UART_RX: u8 = 0x03;
/// SGI 1（跨核重调度投递）。
pub const MIRROR_SGI_RESCHED: u8 = 0x04;
/// SGI 2（TLB shootdown 收端——无唤醒路径，只计到达）。
pub const MIRROR_SGI_TLB: u8 = 0x05;
/// 任务语境基线（note_switch 切入即设）。
pub const MIRROR_IPC: u8 = 0x06;
/// PL061 电源键（到达-only——永不进环）。
pub const MIRROR_POWER_KEY: u8 = 0x07;
/// 未知 IRQ 号（GIC IAR 落未知臂）。
pub const MIRROR_UNKNOWN: u8 = 0x7F;

/// 已定义源集（九源全集——保存值域判的右侧；`[[wakesrc]]` 生成器同源，
/// add-only：加源须同步本表 + HAL 消费点）。
pub const DEFINED: [u8; 9] = [
    MIRROR_NONE,
    MIRROR_KERNEL_BOOT,
    MIRROR_TIMER,
    MIRROR_UART_RX,
    MIRROR_SGI_RESCHED,
    MIRROR_SGI_TLB,
    MIRROR_IPC,
    MIRROR_POWER_KEY,
    MIRROR_UNKNOWN,
];

/// 号→源映射表（GIC INTID 常量参数化注入——HAL 消费用 gic.rs 真常量）。
///
/// 七锚：CNTV(27)→TIMER、PL011_RX(33)→UART_RX、PL1_RX(40)→UART_RX
/// （sim-S1 双 UART 归并不拆——PL011 同设备类，设计文档 §3 裁决）、
/// PL061(39)→POWER_KEY、SGI_RESCHEDULE(1)→SGI_RESCHED、
/// SGI_TLB_SHOOTDOWN(2)→SGI_TLB；未命中恒 UNKNOWN（只进到达计数、
/// 永不进环）。SPURIOUS(1023) 由 `exception_irq` 在 enter 之前早退，
/// 不进号表——语义上它也是未知臂（本表不含 1023，钉板防误加臂）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct IrqMap {
    /// CNTV PPI 号（板上活锚 27）。
    pub cntv: u32,
    /// PL011 RX SPI 号（板上活锚 33）。
    pub pl011_rx: u32,
    /// 第二 PL011 RX SPI 号（sim-S1 活锚 40；与 33 同归 UART_RX）。
    pub pl1_rx: u32,
    /// PL061 电源键 SPI 号（sim-S2 活锚 39）。
    pub pl061: u32,
    /// SGI_RESCHEDULE 号（S-3 §2.2 钉 1）。
    pub sgi_resched: u32,
    /// SGI_TLB_SHOOTDOWN 号（S-3 §2.2 钉 2）。
    pub sgi_tlb: u32,
    /// TIMER 源值（HAL WAKE_SRC_TIMER）。
    pub src_timer: u8,
    /// UART_RX 源值（HAL WAKE_SRC_UART_RX）。
    pub src_uart_rx: u8,
    /// POWER_KEY 源值（HAL WAKE_SRC_POWER_KEY）。
    pub src_power_key: u8,
    /// SGI_RESCHED 源值（HAL WAKE_SRC_SGI_RESCHED）。
    pub src_sgi_resched: u8,
    /// SGI_TLB 源值（HAL WAKE_SRC_SGI_TLB）。
    pub src_sgi_tlb: u8,
    /// UNKNOWN 源值（HAL WAKE_SRC_UNKNOWN）。
    pub src_unknown: u8,
}

impl IrqMap {
    /// GIC INTID → 唤醒源（HAL `wake::src_of` 判序逐字等价迁移——
    /// 板锚在前、SGI 殿后、未知兜底 UNKNOWN）。
    #[inline]
    pub fn src_of(&self, id: u32) -> u8 {
        match id {
            i if i == self.cntv => self.src_timer,
            i if i == self.pl011_rx => self.src_uart_rx,
            i if i == self.pl1_rx => self.src_uart_rx,
            i if i == self.pl061 => self.src_power_key,
            i if i == self.sgi_resched => self.src_sgi_resched,
            i if i == self.sgi_tlb => self.src_sgi_tlb,
            _ => self.src_unknown,
        }
    }

    /// 号表活锚全集（selftest/钉板遍历用——六锚按判序）。
    #[inline]
    pub fn anchors(&self) -> [(u32, u8); 6] {
        [
            (self.cntv, self.src_timer),
            (self.pl011_rx, self.src_uart_rx),
            (self.pl1_rx, self.src_uart_rx),
            (self.pl061, self.src_power_key),
            (self.sgi_resched, self.src_sgi_resched),
            (self.sgi_tlb, self.src_sgi_tlb),
        ]
    }
}

/// IRQ exit 恢复协议值域判：保存值必须是已定义源集 [`DEFINED`] 成员。
///
/// 保存值来自 enter 时的 cell 读，cell 写点三处全在控（note_switch 基线 /
/// exception_irq enter / init 初值）——非法值恢复 = cell 被外物写坏或保存
/// 寄存器被踩，fail-loud 面（HAL 消费点 debug_assert；release 形恢复值
/// 直写不放大损伤——毒值本就已在 cell 里，恢复不引入第二份）。
#[inline]
pub fn restore_valid(saved: u8) -> bool {
    DEFINED.contains(&saved)
}

/// 槽位钳位读：`(src as usize).min(SLOTS - 1)`。
///
/// HAL 端 G1.5 快照合并与到达记账的读点同款钳位（wsrcface::slot_index
/// 同律三方同源）；0x80–0xFF 毒值钳到 127 号槽（UNKNOWN 端点槽），不越界
/// 不 panic。
#[inline]
pub fn slot_index(src: u8) -> usize {
    (src as usize).min(SLOTS - 1)
}

#[cfg(test)]
mod tests;
