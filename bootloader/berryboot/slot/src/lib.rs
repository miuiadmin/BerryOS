//! berry-slot——BerryBootControl A/B 槽位仲裁纯面 crate（生态闭环 G0 刀 1）。
//!
//! 跨界共享：bootloader（berryboot app 仲裁+写回）与用户态（pkgmgr/am 的
//! successful 置位/tries 重置）共用同一结构编码与裁决——berry-loader 跨
//! user/引导两界先例同款。纯数据+纯函数、no_std、host 可测（*face 纪律）。
//!
//! 结构契约 = [bootloader design §2.1](../../.agents/plans/bootloader/design.md)
//! v1.0.1（version=3）：magic 'BRYC' + version + slots[2] + recovery_trigger
//! + boot_count + crc32，共 52B。双副本协议：BOOTCTL1.DAT/BOOTCTL2.DAT
//! （8.3 名）交替写——读侧取「CRC 过且 boot_count 最高」者为权威；单副本
//! 撕裂/位翻转由对侧兜底（G-BOOT-3 单点不改砖）。
//!
//! 仲裁三律（design §2.1 规则 1/3；规则 2 属用户态，本 crate 只供接口）：
//! ① tries>0 且 priority 最高者当选，引导前 tries 减一（写回）；
//! ② tries==0 → 该槽 priority 置 0、换对侧（对侧也空 → recovery）；
//! ③ successful 置位与 tries 重置=用户态动作（[`BootControl::mark_success`] 供其复用）。
#![cfg_attr(not(test), no_std)]

use core::mem::size_of;

/// 控制块魔数 'BRYC'（小端读序）。
pub const MAGIC: u32 = 0x4252_5943;
/// 结构版本（design v1.0.1：v2 增 boot_count）。
pub const VERSION: u32 = 3;
/// 单槽字节数（slot_id..reserved 共 16B）。
pub const SLOT_INFO_LEN: usize = 16;
/// 控制块总长：4+4+16*2+4+4+4 = 52。
pub const CONTROL_LEN: usize = 52;

/// 单槽状态（design §2.1 BerrySlotInfo 的 Rust 镜像，repr(C) 冻结布局）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SlotInfo {
    pub slot_id: u8,
    pub priority: u8,
    pub tries_remaining: u8,
    pub successful: u8,
    pub rollback_version: u32,
}

impl SlotInfo {
    /// 全零出厂的空槽（不可引导——priority=0）。
    pub const EMPTY: SlotInfo = SlotInfo {
        slot_id: 0,
        priority: 0,
        tries_remaining: 0,
        successful: 0,
        rollback_version: 0,
    };

    fn to_bytes(self) -> [u8; SLOT_INFO_LEN] {
        let mut b = [0u8; SLOT_INFO_LEN];
        b[0] = self.slot_id;
        b[1] = self.priority;
        b[2] = self.tries_remaining;
        b[3] = self.successful;
        b[4..8].copy_from_slice(&self.rollback_version.to_le_bytes());
        // reserved[8] 恒零。
        b
    }

    fn from_bytes(b: &[u8]) -> Option<SlotInfo> {
        if b.len() < SLOT_INFO_LEN {
            return None;
        }
        Some(SlotInfo {
            slot_id: b[0],
            priority: b[1],
            tries_remaining: b[2],
            successful: b[3],
            rollback_version: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
        })
    }
}

/// 控制块（design §2.1 BerryBootControl v3，52B）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BootControl {
    /// slots[0]=A、slots[1]=B（slot_id 字段与下标一致性在读侧校验）。
    pub slots: [SlotInfo; 2],
    pub recovery_trigger: u32,
    pub boot_count: u32,
}

/// 仲裁产物：本靴引导哪个槽 + 已按规则①/②演进的写回态。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Arbitration {
    /// 当选槽下标（0=A、1=B）。
    pub chosen: usize,
    /// 引导文件名（8.3 大写，含结尾 NUL 由调用方处理）。
    pub kernel_name: &'static str,
    pub initrd_name: &'static str,
    /// 写回态（tries 已减一；若触发②则对侧 priority 已清零）。
    pub writeback: BootControl,
}

impl BootControl {
    /// 出厂对称默认（mkimage --boot-ctl 工厂件）：A pri=15/tries=3/successful=1，
    /// B pri=14/tries=3/successful=0——B 试用期。
    pub fn factory() -> BootControl {
        BootControl {
            slots: [
                SlotInfo { slot_id: 0, priority: 15, tries_remaining: 3, successful: 1, rollback_version: 0 },
                SlotInfo { slot_id: 1, priority: 14, tries_remaining: 3, successful: 0, rollback_version: 0 },
            ],
            recovery_trigger: 0,
            boot_count: 0,
        }
    }

    /// 编码为 52B（含尾 CRC32——覆盖前 48B）。
    pub fn to_bytes(self) -> [u8; CONTROL_LEN] {
        let mut b = [0u8; CONTROL_LEN];
        b[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        b[4..8].copy_from_slice(&VERSION.to_le_bytes());
        b[8..24].copy_from_slice(&self.slots[0].to_bytes());
        b[24..40].copy_from_slice(&self.slots[1].to_bytes());
        b[40..44].copy_from_slice(&self.recovery_trigger.to_le_bytes());
        b[44..48].copy_from_slice(&self.boot_count.to_le_bytes());
        let crc = crc32(&b[..48]);
        b[48..52].copy_from_slice(&crc.to_le_bytes());
        b
    }

    /// 解码并全验（魔数/版本/CRC/slot_id 与下标一致）；任一不过 → None
    /// （调用方按副本协议取对侧）。
    pub fn from_bytes(b: &[u8]) -> Option<BootControl> {
        if b.len() < CONTROL_LEN {
            return None;
        }
        if u32::from_le_bytes([b[0], b[1], b[2], b[3]]) != MAGIC {
            return None;
        }
        if u32::from_le_bytes([b[4], b[5], b[6], b[7]]) != VERSION {
            return None;
        }
        let crc_stored = u32::from_le_bytes([b[48], b[49], b[50], b[51]]);
        if crc32(&b[..48]) != crc_stored {
            return None;
        }
        let s0 = SlotInfo::from_bytes(&b[8..24])?;
        let s1 = SlotInfo::from_bytes(&b[24..40])?;
        if s0.slot_id != 0 || s1.slot_id != 1 {
            return None;
        }
        Some(BootControl {
            slots: [s0, s1],
            recovery_trigger: u32::from_le_bytes([b[40], b[41], b[42], b[43]]),
            boot_count: u32::from_le_bytes([b[44], b[45], b[46], b[47]]),
        })
    }

    /// 双副本择权威：CRC 过且 boot_count 最高者；双有效同 count（出厂对称
    /// 形态）取副本 1；双败 → None（recovery 路径）。
    pub fn pick_copy(c1: &[u8], c2: &[u8]) -> Option<BootControl> {
        match (BootControl::from_bytes(c1), BootControl::from_bytes(c2)) {
            (Some(a), Some(b)) => Some(if b.boot_count > a.boot_count { b } else { a }),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// 仲裁（规则①②）：recovery_trigger 非零或双槽皆不可引导（priority 全 0
    /// 或全空）→ None（调用方走 recovery 镜像）；否则返回当选槽与写回态。
    /// 幂等纯函数——不读不写外设。
    pub fn arbitrate(&self) -> Option<Arbitration> {
        if self.recovery_trigger != 0 {
            return None;
        }
        // 当选律：priority 最高者；平 priority（出厂对称前 B=successful=0 的
        // 试用期不与 A 平——工厂件 A=15/B=14 不平；运行期平局取 successful
        // 高者，再平取 A——确定性钉死）。
        let mut best: Option<usize> = None;
        for i in 0..2 {
            let s = self.slots[i];
            if s.priority == 0 {
                continue;
            }
            best = match best {
                None => Some(i),
                Some(j) => {
                    let cur = self.slots[j];
                    let better = s.priority > cur.priority
                        || (s.priority == cur.priority && s.successful > cur.successful);
                    if better { Some(i) } else { Some(j) }
                }
            };
        }
        let chosen = best?;
        // 规则①：tries>0 才可引导（priority>0 但 tries==0 的残留态按规则②
        // 清 priority 换对侧——arbitrate 不跳过它，先清后选）。
        if self.slots[chosen].tries_remaining == 0 {
            let mut wb = *self;
            wb.slots[chosen].priority = 0;
            wb.boot_count = self.boot_count.wrapping_add(1);
            return wb.arbitrate();
        }
        let mut wb = *self;
        wb.slots[chosen].tries_remaining -= 1;
        wb.boot_count = self.boot_count.wrapping_add(1);
        Some(Arbitration {
            chosen,
            kernel_name: if chosen == 0 { "KERNELA.ELF" } else { "KERNELB.ELF" },
            initrd_name: if chosen == 0 { "INITRDA.BIN" } else { "INITRDB.BIN" },
            writeback: wb,
        })
    }

    /// 规则②的用户态半边（pkgmgr/am 60s 健康后调用）：successful 置 1、
    /// tries 重置 3、boot_count 前进。
    pub fn mark_success(&self, slot: usize) -> BootControl {
        let mut wb = *self;
        wb.slots[slot].successful = 1;
        wb.slots[slot].tries_remaining = 3;
        wb.boot_count = self.boot_count.wrapping_add(1);
        wb
    }
}

/// IEEE CRC32（多项式 0xEDB88320，反射式，初值/终值翻转）——纯实现
/// （确定性 host 钉板 + 用户态复用；boot 冷路径不计性能）。
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

const _: () = assert!(size_of::<[u8; CONTROL_LEN]>() == 52);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_ieee_vectors() {
        // IEEE 802.3 标准向量。
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0x0000_0000);
        assert_eq!(crc32(b"a"), 0xE8B7_BE43);
        assert_eq!(crc32(b"abcdef"), 0x4B8E_39EF);
    }

    #[test]
    fn control_roundtrip_and_size() {
        assert_eq!(CONTROL_LEN, 52);
        let c = BootControl::factory();
        let b = c.to_bytes();
        assert_eq!(b.len(), 52);
        assert_eq!(BootControl::from_bytes(&b), Some(c));
    }

    #[test]
    fn any_byte_corrupt_rejected() {
        let b = BootControl::factory().to_bytes();
        for i in 0..52 {
            let mut t = b;
            t[i] ^= 0x01;
            assert_eq!(BootControl::from_bytes(&t), None, "byte {i} 翻转须拒");
        }
    }

    #[test]
    fn magic_version_slotid_guards() {
        let mut b = BootControl::factory().to_bytes();
        b[0] = 0x99; // 魔数破
        assert_eq!(BootControl::from_bytes(&b), None);
        let mut b = BootControl::factory().to_bytes();
        b[4..8].copy_from_slice(&2u32.to_le_bytes()); // 版本回 v2
        assert_eq!(BootControl::from_bytes(&b), None);
        let mut b = BootControl::factory().to_bytes();
        b[8] = 1; // slot_id 下标错位
        assert_eq!(BootControl::from_bytes(&b), None);
    }

    #[test]
    fn pick_copy_prefers_higher_boot_count() {
        let mut newer = BootControl::factory();
        newer.boot_count = 7;
        let older = BootControl::factory();
        let n = newer.to_bytes();
        let o = older.to_bytes();
        assert_eq!(BootControl::pick_copy(&o, &n), Some(newer));
        assert_eq!(BootControl::pick_copy(&n, &o), Some(newer)); // 序无关
        // 单副本撕裂：对侧兜底。
        let mut torn = n;
        torn[20] ^= 0xFF;
        assert_eq!(BootControl::pick_copy(&torn, &o), Some(older));
        assert_eq!(BootControl::pick_copy(&torn, &torn), None); // 双败
        // 等计数（出厂对称）取第一副本——确定性。
        assert_eq!(BootControl::pick_copy(&o, &o), Some(older));
    }

    #[test]
    fn rule1_factory_picks_a_and_decrements() {
        let arb = BootControl::factory().arbitrate().expect("factory 可引导");
        assert_eq!(arb.chosen, 0);
        assert_eq!(arb.kernel_name, "KERNELA.ELF");
        assert_eq!(arb.initrd_name, "INITRDA.BIN");
        assert_eq!(arb.writeback.slots[0].tries_remaining, 2);
        assert_eq!(arb.writeback.boot_count, 1);
        assert_eq!(arb.writeback.slots[1].tries_remaining, 3); // B 不动
    }

    #[test]
    fn rule2_exhaustion_switches_side() {
        // A 三靴耗尽后的第四靴：A.priority 清零、B 当选。
        let mut c = BootControl::factory();
        c.slots[0].tries_remaining = 0;
        let arb = c.arbitrate().expect("B 在席可引导");
        assert_eq!(arb.chosen, 1);
        assert_eq!(arb.kernel_name, "KERNELB.ELF");
        assert_eq!(arb.writeback.slots[0].priority, 0, "A 须降 0");
        assert_eq!(arb.writeback.slots[1].tries_remaining, 2);
    }

    #[test]
    fn both_exhausted_is_recovery() {
        let mut c = BootControl::factory();
        c.slots[0].priority = 0;
        c.slots[1].priority = 0;
        assert_eq!(c.arbitrate(), None);
        let mut c = BootControl::factory();
        c.recovery_trigger = 1;
        assert_eq!(c.arbitrate(), None, "recovery 触发位优先");
    }

    #[test]
    fn three_crash_walk_pure_state() {
        // 纯态走查：出厂 → 三靴 A（tries 3→0）→ 第四靴自动切 B。
        let mut c = BootControl::factory();
        let mut seq = [9usize; 4];
        for i in 0..4 {
            let arb = c.arbitrate().expect("四靴内皆有可引导槽");
            seq[i] = arb.chosen;
            c = arb.writeback; // 失败靴：无人 mark_success，直接续走
        }
        assert_eq!(seq, [0, 0, 0, 1], "A 三靴后第四靴切 B");
        assert_eq!(c.slots[0].tries_remaining, 0);
        assert_eq!(c.slots[0].priority, 0);
        assert_eq!(c.slots[1].tries_remaining, 2, "B 第四靴刚用一次");
        assert!(c.arbitrate().is_some(), "B 仍有 tries——第五靴续 B");
    }

    #[test]
    fn mark_success_resets_and_advances() {
        let c = BootControl::factory();
        let booted = c.arbitrate().unwrap().writeback; // tries 3→2
        let confirmed = booted.mark_success(0);
        assert_eq!(confirmed.slots[0].successful, 1);
        assert_eq!(confirmed.slots[0].tries_remaining, 3, "确认后重置");
        assert_eq!(confirmed.boot_count, booted.boot_count + 1);
    }

    #[test]
    fn torn_write_recovery_walk() {
        // 交替写协议走查：权威在 copy1(count N) → 写 copy2(count N+1) 中途撕裂
        // → 择权威回到 copy1 → 系统续跑（G-BOOT-3 单点不改砖）。
        let mut c = BootControl::factory();
        c.boot_count = 10;
        let copy1 = c.to_bytes();
        let next = c.arbitrate().unwrap().writeback; // count 11
        let mut copy2 = next.to_bytes();
        copy2[30..].fill(0xFF); // 撕裂：后半乱（等价截断后补写未达）
        let picked = BootControl::pick_copy(&copy1, &copy2).expect("copy1 兜底");
        assert_eq!(picked.boot_count, 10);
        let arb = picked.arbitrate().unwrap();
        assert_eq!(arb.chosen, 0, "撕裂后仍引导 A");
    }
}
