//! virtio-input 设备类编解码（GUI-4 段①，virtio 规范 §5.8；实施依据 =
//! docs/GUI-4实施设计草案.md §4/§6-I2）。
//!
//! 消费者 = gui-inputd（drivers/bins/gui-inputd，v1 唯一）——eventq(q0)
//! dequeue 的 8B 事件流经 `get_input_event` 逐个解码；config 查询区 union
//! 按 select 类型经 `parse_input_devids`/`parse_input_absinfo`/`bitmap_test`
//! 解释。host 单测全盖（inline `input_tests`）。
//!
//! wire 事实（规范 §5.8.5，与 linux/virtio_input.h 逐字段对拍确认）：
//! - event 恰 8B 全 LE：`{type u16, code u16, value u32}`；value 线上
//!   u32、语义可负（REL 轴）——**按 i32 解释归消费侧**（gui-inputd 的
//!   `raw_from_wire` 转换点）。
//! - **无时间戳字段**：规范不提供设备侧时间戳，驱动在 dequeue 时刻自打
//!   ts_ns（Linux evdev 同款——泵在 gui-inputd main）。
//! - config 恰 136B @ MMIO 0x100（字节口径访问）：`{select u8, subsel u8,
//!   size u8, res[5], union[128]}`——驱动写 select/subsel 选定查询目标、
//!   回读 size 后 union 的前 size 字节有效；本模块按查询类型**恰长执法**
//!   （devids 恰 8B / absinfo 恰 20B，截断与尾随全拒 None，不截断不猜测）。
//! - 位图（PROP_BITS/EV_BITS 查询）LSB-first：code n 的位 =
//!   `bitmap[n/8] & 1<<(n%8)`；n 越出位图幅 = 位不在场（false，不越界读）。
//! - eventq = q0（设备→驱动事件流）、statusq = q1（LED/力反馈，v1 不用）；
//!   设备 feature bits 零（仅传输层 VERSION_1）；QEMU 缓冲到 SYN_REPORT
//!   才投递——一个完整批的 8B×N 在一次 used 环更新内可见（§4.1 流控）。
//!
//! 纪律同 crate 头：volatile MMIO 读写、扫窗探测/协商（STATUS=0 复位回读
//! 帽 → ACK|DRIVER → FEATURES_OK 回读验）、eventq 泵与 always-posted 补投
//! 都在驱动本体（gui-inputd main——vnet/vblk 先例）；本模块只管「内存里
//! 的字节长什么样」。

use crate::{get_u16, get_u32, put_u16, put_u32};

/// virtio-input 事件幅（规范 §5.8.5 struct virtio_input_event）：
/// type u16 + code u16 + value u32 = 恰 8B。
pub const INPUT_EVENT_SZ: usize = 8;

/// virtio-input 事件（恰 8B 全 LE）。`ev_type`/`code` 是 evdev 事件类型/
/// 事件码（本模块常量表，linux/input-event-codes.h 冻结值）；`value` 线上
/// u32——REL 轴语义可负，按 i32 解释归消费侧。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtioInputEvent {
    pub ev_type: u16,
    pub code: u16,
    pub value: u32,
}

/// 编事件到 `buf[off..off+8]`（LE 逐字段）。缓冲不足拒 None——「编不出就
/// 拒绝，不截断」（调用方缓冲 = eventq 预投读缓冲的暂存页；失败可能留
/// 部分写入，调用方按 None 整体弃用，同 `put_desc` 形态）。
pub fn put_input_event(buf: &mut [u8], off: usize, ev: &VirtioInputEvent) -> Option<()> {
    put_u16(buf, off, ev.ev_type)?;
    put_u16(buf, off + 2, ev.code)?;
    put_u32(buf, off + 4, ev.value)
}

/// 解码 `buf[off..off+8]` 为事件（LE 逐字段）。越界/截断拒 None——
/// 流尽与残尾都产出 None，不猜测半事件。
pub fn get_input_event(buf: &[u8], off: usize) -> Option<VirtioInputEvent> {
    Some(VirtioInputEvent {
        ev_type: get_u16(buf, off)?,
        code: get_u16(buf, off + 2)?,
        value: get_u32(buf, off + 4)?,
    })
}

// ---- evdev 常量（§4.2 全集 + 留位；值域 = linux/input-event-codes.h）----

/// 事件类型：同步分隔（批界——SYN_REPORT 关批）。
pub const EV_SYN: u16 = 0x00;
/// 事件类型：按键/按钮（BTN_* 空间）。
pub const EV_KEY: u16 = 0x01;
/// 事件类型：相对轴（鼠标位移/滚轮）。
pub const EV_REL: u16 = 0x02;
/// 事件类型：绝对轴（平板/触摸屏坐标）。
pub const EV_ABS: u16 = 0x03;
/// 事件类型：杂项消息（v1 忽略但计数）。
pub const EV_MSC: u16 = 0x04;

/// EV_SYN code：本批事件已完整（pump 关批判据）。
pub const SYN_REPORT: u16 = 0;
/// EV_SYN code：多点触摸批界（v1 忽略计数）。
pub const SYN_MT_REPORT: u16 = 2;
/// EV_SYN code：设备侧丢事件（序列完整性丢失——消费侧应作废当前手势
/// 状态、下批合成 PHASE_CANCEL，Linux evdev SYN_DROPPED 处置同型）。
pub const SYN_DROPPED: u16 = 3;

/// EV_ABS code：X 轴。
pub const ABS_X: u16 = 0x00;
/// EV_ABS code：Y 轴（向下为正）。
pub const ABS_Y: u16 = 0x01;

/// EV_KEY code：左键（鼠标形态的接触判据——与 BTN_TOUCH 等价对待）。
pub const BTN_LEFT: u16 = 0x110;
/// EV_KEY code：右键（PointerEvent BUTTON_R 位）。
pub const BTN_RIGHT: u16 = 0x111;
/// EV_KEY code：中键（PointerEvent BUTTON_M 位）。
pub const BTN_MIDDLE: u16 = 0x112;
/// EV_KEY code：触摸接触（平板形态的接触判据——QEMU virtio-tablet 发它）。
pub const BTN_TOUCH: u16 = 0x14a;

/// EV_ABS code：多点槽序号（v1 忽略计数——多点留位）。
pub const ABS_MT_SLOT: u16 = 0x2f;
/// EV_ABS code：多点 X（v1 忽略计数——多点留位）。
pub const ABS_MT_POSITION_X: u16 = 0x35;
/// EV_ABS code：多点 Y（v1 忽略计数——多点留位）。
pub const ABS_MT_POSITION_Y: u16 = 0x36;
/// EV_ABS code：多点跟踪 id（v1 忽略计数——多点留位）。
pub const ABS_MT_TRACKING_ID: u16 = 0x39;

/// EV_REL code：滚轮（v1 忽略但计数，不入 PointerEvent）。
pub const REL_WHEEL: u16 = 0x08;

// ---- config 查询区（规范 §5.8.4）----

/// virtio-input config 查询区幅：`{select u8, subsel u8, size u8, res[5],
/// union[128]}` = 恰 136B @ MMIO 0x100（字节口径访问）。
pub const INPUT_CFG_SZ: usize = 136;
/// union（string/bitmap/devids/absinfo）在 config 区内的偏移（私有——
/// 驱动侧经 `mmio::CONFIG + 8` 同值访问，几何由编译期断言钉死）。
const INPUT_CFG_UNION_OFF: usize = 8;
/// union 幅（string[128]/bitmap[128] 同窗）。
const INPUT_CFG_UNION_LEN: usize = 128;

/// select：未选定（复位值）。
pub const INPUT_CFG_SEL_UNSET: u8 = 0;
/// select：设备名（string 查询）。
pub const INPUT_CFG_SEL_ID_NAME: u8 = 1;
/// select：序列号（string 查询）。
pub const INPUT_CFG_SEL_ID_SERIAL: u8 = 2;
/// select：设备 ID 四元组（devids，恰 8B）。
pub const INPUT_CFG_SEL_ID_DEVIDS: u8 = 3;
/// select：属性位图（PROP_BITS；位图 LSB-first）。
pub const INPUT_CFG_SEL_PROP_BITS: u8 = 0x10;
/// select：事件码位图（EV_BITS；subsel = EV 类型，位图 = 该类型的 code 集）。
pub const INPUT_CFG_SEL_EV_BITS: u8 = 0x11;
/// select：绝对轴信息（ABS_INFO；subsel = ABS 轴号，恰 20B）。
pub const INPUT_CFG_SEL_ABS_INFO: u8 = 0x12;

// 布局 const assert（编译期钉板——幅漂移即编译失败，§4.3 五件套之
// size 上限）。
const _: () = assert!(INPUT_EVENT_SZ == 8);
const _: () = assert!(INPUT_CFG_UNION_OFF + INPUT_CFG_UNION_LEN == INPUT_CFG_SZ);

/// ID_DEVIDS 查询结果：四 u16 LE（bustype/vendor/product/version）——
/// 值域由设备自述（诊断打印面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputDevids {
    pub bustype: u16,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
}

/// 解 ID_DEVIDS 查询的有效载荷：**恰 8B**（四 u16 LE）——截断与尾随全拒
/// （size 域撒谎/野值形态在解码层结构性拒收）。
pub fn parse_input_devids(data: &[u8]) -> Option<InputDevids> {
    if data.len() != 8 {
        return None;
    }
    Some(InputDevids {
        bustype: get_u16(data, 0)?,
        vendor: get_u16(data, 2)?,
        product: get_u16(data, 4)?,
        version: get_u16(data, 6)?,
    })
}

/// ABS_INFO 查询结果：五 i32 LE（min/max/fuzz/flat/res——Linux
/// input_absinfo 同形）。归一化用 min/max：**以设备自述为准，勿硬编
/// 32767**（QEMU virtio-tablet 事实对拍值只是参考，设计 §4.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputAbsInfo {
    pub min: i32,
    pub max: i32,
    pub fuzz: i32,
    pub flat: i32,
    pub res: i32,
}

/// 解 ABS_INFO 查询的有效载荷：**恰 20B**（五 i32 LE）——截断与尾随全拒。
/// 负值经 u32 位型读出后 `as i32` 还原（LE 位型同形，语义位归消费侧）。
pub fn parse_input_absinfo(data: &[u8]) -> Option<InputAbsInfo> {
    if data.len() != 20 {
        return None;
    }
    Some(InputAbsInfo {
        min: get_u32(data, 0)? as i32,
        max: get_u32(data, 4)? as i32,
        fuzz: get_u32(data, 8)? as i32,
        flat: get_u32(data, 12)? as i32,
        res: get_u32(data, 16)? as i32,
    })
}

/// 位图位测（PROP_BITS/EV_BITS 查询面）：LSB-first——code n 的位 =
/// `bitmap[n/8] & 1<<(n%8)`。n 越出位图幅 = 位不在场（false，不越界读、
/// 不 panic）——「能力缺席」与「位图短」同判。
pub fn bitmap_test(bitmap: &[u8], code: u16) -> bool {
    let idx = (code as usize) / 8;
    let bit = code % 8; // 0..=7
    match bitmap.get(idx) {
        Some(&b) => b & (1u8 << bit) != 0,
        None => false,
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;

    /// EV_KEY BTN_TOUCH value=1（按下）整帧黄金字节（手拼 LE，独立于被测 put）。
    const EV_PRESS: [u8; 8] = [0x01, 0x00, 0x4a, 0x01, 0x01, 0x00, 0x00, 0x00];
    /// EV_ABS ABS_X value=0x4000 整帧。
    const EV_ABSX: [u8; 8] = [0x03, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00];
    /// EV_ABS ABS_Y value=0x2666 整帧。
    const EV_ABSY: [u8; 8] = [0x03, 0x00, 0x01, 0x00, 0x66, 0x26, 0x00, 0x00];
    /// EV_SYN SYN_REPORT value=0 整帧（全零——批界）。
    const EV_SYN0: [u8; 8] = [0x00; 8];

    #[test]
    fn event_roundtrip() {
        // 含边界值：0 / u16::MAX / u32::MAX（REL 负值的位型）。
        let table = [
            VirtioInputEvent { ev_type: 0, code: 0, value: 0 },
            VirtioInputEvent { ev_type: EV_KEY, code: BTN_TOUCH, value: 1 },
            VirtioInputEvent { ev_type: EV_ABS, code: ABS_X, value: 32767 },
            VirtioInputEvent { ev_type: EV_REL, code: REL_WHEEL, value: u32::MAX },
            VirtioInputEvent { ev_type: u16::MAX, code: u16::MAX, value: u32::MAX },
        ];
        let mut buf = [0u8; 64];
        for (i, ev) in table.iter().enumerate() {
            let off = i * 8;
            put_input_event(&mut buf, off, ev).unwrap();
            assert_eq!(get_input_event(&buf, off).unwrap(), *ev, "event #{i}");
        }
    }

    #[test]
    fn event_wire_le_golden() {
        // LE 钉板：逐字节对拍手拼黄金帧（不循环依赖被测 get——字节面独立
        // 自证）；首字节 = ev_type 低字节；code=0x14a 跨字节序钉 [4a,01]。
        let mut buf = [0u8; 8];
        put_input_event(
            &mut buf,
            0,
            &VirtioInputEvent { ev_type: EV_KEY, code: BTN_TOUCH, value: 1 },
        )
        .unwrap();
        assert_eq!(buf, EV_PRESS);
        assert_eq!(buf[0], (EV_KEY & 0xff) as u8); // 首字节 = type 低字节

        put_input_event(
            &mut buf,
            0,
            &VirtioInputEvent { ev_type: EV_ABS, code: ABS_X, value: 0x4000 },
        )
        .unwrap();
        assert_eq!(buf, EV_ABSX);

        put_input_event(
            &mut buf,
            0,
            &VirtioInputEvent { ev_type: EV_ABS, code: ABS_Y, value: 0x2666 },
        )
        .unwrap();
        assert_eq!(buf, EV_ABSY);

        // REL 负值位型：-1 = 0xffffffff（value 尾四字节全 ff）。
        put_input_event(
            &mut buf,
            0,
            &VirtioInputEvent { ev_type: EV_REL, code: REL_WHEEL, value: u32::MAX },
        )
        .unwrap();
        assert_eq!(&buf[0..2], &[0x02, 0x00]);
        assert_eq!(&buf[2..4], &[0x08, 0x00]);
        assert_eq!(&buf[4..8], &[0xff; 4]);
    }

    #[test]
    fn event_put_get_bounds() {
        let ev = VirtioInputEvent { ev_type: EV_SYN, code: SYN_REPORT, value: 0 };
        let mut buf = [0u8; 16];
        // 尾部恰容：off=8 恰入 16B；off=9 越尾拒。
        assert!(put_input_event(&mut buf, 8, &ev).is_some());
        assert!(put_input_event(&mut buf, 9, &ev).is_none());
        // 短缓冲：7B 放不下任何整事件。
        let mut tiny = [0u8; 7];
        assert!(put_input_event(&mut tiny, 0, &ev).is_none());
        // get 同口径：恰容 Some / 越尾 None / 短缓冲 None。
        assert!(get_input_event(&buf, 8).is_some());
        assert!(get_input_event(&buf, 9).is_none());
        assert!(get_input_event(&tiny, 0).is_none());
    }

    #[test]
    fn stream_decode_press_then_move() {
        // 事件序列最小样本：「按下」批（BTN_TOUCH=1 + X + Y + SYN）接
        // 「移动」批（X + Y + SYN）——QEMU 缓冲到 SYN_REPORT 才投递、
        // 一个完整批 8B×N 在一次 used 环更新内可见的流形态（§4.1）。
        // 56B 连续流逐 8B 解码对拍。
        let mv_x: [u8; 8] = [0x03, 0x00, 0x00, 0x00, 0x39, 0x30, 0x00, 0x00]; // X=12345
        let mv_y: [u8; 8] = [0x03, 0x00, 0x01, 0x00, 0x85, 0x1a, 0x00, 0x00]; // Y=6789
        let mut stream = [0u8; 56];
        stream[0..8].copy_from_slice(&EV_PRESS);
        stream[8..16].copy_from_slice(&EV_ABSX);
        stream[16..24].copy_from_slice(&EV_ABSY);
        stream[24..32].copy_from_slice(&EV_SYN0);
        stream[32..40].copy_from_slice(&mv_x);
        stream[40..48].copy_from_slice(&mv_y);
        stream[48..56].copy_from_slice(&EV_SYN0);

        let expect = [
            VirtioInputEvent { ev_type: EV_KEY, code: BTN_TOUCH, value: 1 },
            VirtioInputEvent { ev_type: EV_ABS, code: ABS_X, value: 0x4000 },
            VirtioInputEvent { ev_type: EV_ABS, code: ABS_Y, value: 0x2666 },
            VirtioInputEvent { ev_type: EV_SYN, code: SYN_REPORT, value: 0 },
            VirtioInputEvent { ev_type: EV_ABS, code: ABS_X, value: 12345 },
            VirtioInputEvent { ev_type: EV_ABS, code: ABS_Y, value: 6789 },
            VirtioInputEvent { ev_type: EV_SYN, code: SYN_REPORT, value: 0 },
        ];
        for (i, want) in expect.iter().enumerate() {
            assert_eq!(get_input_event(&stream, i * 8).unwrap(), *want, "event #{i}");
        }
        // 流尽：越界拒；截一字节：末事件不完整 → None（畸形长度拒，
        // 不产出半事件）；界内不受牵连。
        assert!(get_input_event(&stream, 56).is_none());
        let cut = &stream[..55];
        assert!(get_input_event(cut, 48).is_none());
        assert!(get_input_event(cut, 40).is_some());
    }

    #[test]
    fn devids_parse_exact_len() {
        // 合成样本（值域由设备自述，这里钉编解码面）：手拼 LE 黄金字节。
        let wire = [0x06, 0x00, 0xf4, 0x1a, 0x52, 0x10, 0x00, 0x00];
        let d = parse_input_devids(&wire).unwrap();
        assert_eq!(
            d,
            InputDevids { bustype: 0x06, vendor: 0x1af4, product: 0x1052, version: 0 }
        );
        // 恰长执法：截断（7B）/ 尾随（9B）/ 空 全拒；恰 8B 恒过。
        assert!(parse_input_devids(&[]).is_none());
        assert!(parse_input_devids(&wire[..7]).is_none());
        assert!(parse_input_devids(&wire).is_some());
        let mut padded = [0u8; 9];
        padded[..8].copy_from_slice(&wire);
        assert!(parse_input_devids(&padded).is_none());
    }

    #[test]
    fn absinfo_parse_exact_len() {
        // QEMU virtio-tablet ABS 自述形态（min=0 max=32767，§4.2 事实）。
        let mut wire = [0u8; 20];
        wire[4..8].copy_from_slice(&[0xff, 0x7f, 0x00, 0x00]); // max = 32767
        let a = parse_input_absinfo(&wire).unwrap();
        assert_eq!(a, InputAbsInfo { min: 0, max: 32767, fuzz: 0, flat: 0, res: 0 });
        // 负值 LE 位型：min=-1 → 全 ff（i32 经 u32 位型还原）。
        wire[0..4].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
        let a = parse_input_absinfo(&wire).unwrap();
        assert_eq!(a.min, -1);
        assert_eq!(a.max, 32767);
        // 恰长执法：19B（截断）/ 21B（尾随）全拒。
        assert!(parse_input_absinfo(&[]).is_none());
        assert!(parse_input_absinfo(&wire[..19]).is_none());
        let mut padded = [0u8; 21];
        padded[..20].copy_from_slice(&wire);
        assert!(parse_input_absinfo(&padded).is_none());
    }

    #[test]
    fn config_union_slice_decode() {
        // 消费路径对拍：驱动从 config 区 union 偏移 8 切前 size 字节再解析
        // ——模块头声明的几何（8 + 128 = 136）在真实消费形态下自证。
        let mut cfg = [0u8; INPUT_CFG_SZ];
        cfg[0] = INPUT_CFG_SEL_ID_DEVIDS;
        cfg[1] = 0; // subsel：ID_DEVIDS 无 subsel
        cfg[2] = 8; // size：union 有效字节数
        cfg[8..16].copy_from_slice(&[0x06, 0x00, 0xf4, 0x1a, 0x52, 0x10, 0x00, 0x00]);
        let d = parse_input_devids(&cfg[8..8 + cfg[2] as usize]).unwrap();
        assert_eq!(d.bustype, 0x06);
        assert_eq!(d.vendor, 0x1af4);
        assert_eq!(d.product, 0x1052);
    }

    #[test]
    fn config_absinfo_and_evbits_decode() {
        // ABS_INFO 查询形态：subsel = ABS 轴号、union 前 20B。
        let mut cfg = [0u8; INPUT_CFG_SZ];
        cfg[0] = INPUT_CFG_SEL_ABS_INFO;
        cfg[1] = ABS_X as u8; // subsel = ABS_X
        cfg[2] = 20;
        cfg[8..12].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]); // min = -1
        cfg[12..16].copy_from_slice(&[0xff, 0x7f, 0x00, 0x00]); // max = 32767
        let a = parse_input_absinfo(&cfg[8..8 + 20]).unwrap();
        assert_eq!((a.min, a.max), (-1, 32767));
        // EV_BITS 查询形态：subsel = EV_ABS、union 前 8B 位图盖 code 0..64。
        let mut bm = [0u8; INPUT_CFG_SZ];
        bm[0] = INPUT_CFG_SEL_EV_BITS;
        bm[1] = EV_ABS as u8;
        bm[2] = 8;
        bm[8] = 0x03; // ABS_X + ABS_Y 在场
        let bits = &bm[8..8 + 8];
        assert!(bitmap_test(bits, ABS_X));
        assert!(bitmap_test(bits, ABS_Y));
        assert!(!bitmap_test(bits, 2));
    }

    #[test]
    fn bitmap_lsb_first() {
        // LSB-first 定位钉板：byte0=0x81 → code 0/7 在场；byte1=0x01 →
        // code 8 在场、code 9 不在（跨字节边界）；code 0x110=272 →
        // byte34 bit0（BTN_* 越单字节的实际档）。
        let mut bm = [0u8; 48];
        bm[0] = 0x81;
        bm[1] = 0x01;
        bm[34] = 0x03; // 272、273 在场；274 不在
        assert!(bitmap_test(&bm, 0));
        assert!(!bitmap_test(&bm, 1));
        assert!(bitmap_test(&bm, 7));
        assert!(bitmap_test(&bm, 8));
        assert!(!bitmap_test(&bm, 9));
        assert!(bitmap_test(&bm, BTN_LEFT)); // 0x110 = 272
        assert!(bitmap_test(&bm, BTN_RIGHT)); // 0x111 = 273
        assert!(!bitmap_test(&bm, BTN_MIDDLE)); // 0x112 = 274
        // BTN_TOUCH = 0x14a = 330 → byte41 bit2（置位前后对拍）。
        assert!(!bitmap_test(&bm, BTN_TOUCH));
        bm[41] = 1 << 2;
        assert!(bitmap_test(&bm, BTN_TOUCH));
    }

    #[test]
    fn bitmap_out_of_range_is_false() {
        // 位图短/空：位不在场恒 false，不越界读不 panic（u16 code 全域 safe）。
        assert!(!bitmap_test(&[], 0));
        assert!(!bitmap_test(&[0xff], 8)); // byte1 越出 1B 幅
        assert!(!bitmap_test(&[0xff; 40], 320)); // code 320 → byte40 越出
        assert!(bitmap_test(&[0xff; 41], 320)); // 恰容 → bit0 在场
        assert!(!bitmap_test(&[], u16::MAX));
    }

    #[test]
    fn size_and_select_consts() {
        // 编译期 const assert 的运行期镜像（文档化钉板）。
        assert_eq!(INPUT_EVENT_SZ, 8);
        assert_eq!(INPUT_CFG_SZ, 136);
        assert_eq!(crate::mmio::DEVICE_ID_INPUT, 18);
        // select 值域 = linux/virtio_input.h 枚举冻结值。
        assert_eq!(INPUT_CFG_SEL_UNSET, 0x00);
        assert_eq!(INPUT_CFG_SEL_ID_NAME, 0x01);
        assert_eq!(INPUT_CFG_SEL_ID_SERIAL, 0x02);
        assert_eq!(INPUT_CFG_SEL_ID_DEVIDS, 0x03);
        assert_eq!(INPUT_CFG_SEL_PROP_BITS, 0x10);
        assert_eq!(INPUT_CFG_SEL_EV_BITS, 0x11);
        assert_eq!(INPUT_CFG_SEL_ABS_INFO, 0x12);
        // evdev 常量抽钉（冻结值对拍 linux/input-event-codes.h）。
        assert_eq!((EV_SYN, EV_KEY, EV_REL, EV_ABS, EV_MSC), (0, 1, 2, 3, 4));
        assert_eq!((SYN_REPORT, SYN_MT_REPORT, SYN_DROPPED), (0, 2, 3));
        assert_eq!((ABS_X, ABS_Y), (0, 1));
        assert_eq!((BTN_LEFT, BTN_RIGHT, BTN_MIDDLE, BTN_TOUCH), (0x110, 0x111, 0x112, 0x14a));
        assert_eq!(
            (ABS_MT_SLOT, ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_MT_TRACKING_ID, REL_WHEEL),
            (0x2f, 0x35, 0x36, 0x39, 0x08)
        );
    }
}
