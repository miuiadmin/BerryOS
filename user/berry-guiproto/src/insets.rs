//! R2 typed occlusion vocabulary（host-side，add-only，不上 wire）。
//!
//! 这不是 SafeArea/KeyboardAvoider 的产品化承诺：v1 只有 SYSTEM 窗口能
//! 生产 persistent occlusion，并继续经 P0 0x67 的 `safe_*` 字段授予。
//! IME/display cutout/gesture/tappable/overlay 只有词表与 producer 裁决，
//! 没有真值源时恒不授予。derived effective padding 永远现场计算，不落
//! 第二份存储真值。

/// Persistent system occlusion（状态栏/导航栏/系统壳）。Berry 简化词：
/// 它对应 Android `systemBars` 的常用子集，不是与 Android 聚合语义的一
/// 一映射；`SystemOverlays` 独立成 kind。
pub const INSET_KIND_SYSTEM_BARS: u8 = 0;
/// 显示形状/挖孔。producer = 未来 display metadata；R2 无生产者。
pub const INSET_KIND_DISPLAY_CUTOUT: u8 = 1;
/// Transient IME occlusion。producer = 未来 IME Host；不得改变 raw bounds。
pub const INSET_KIND_IME: u8 = 2;
/// 手势排除区。producer = 未来系统手势策略；R2 无生产者。
pub const INSET_KIND_SYSTEM_GESTURES: u8 = 3;
/// 强制系统手势区。producer = 未来系统手势策略；R2 无生产者。
pub const INSET_KIND_MANDATORY_SYSTEM_GESTURES: u8 = 4;
/// 不可点区。producer = 未来系统 overlay 策略；R2 无生产者。
pub const INSET_KIND_TAPPABLE_ELEMENT: u8 = 5;
/// 不可隐藏 overlay。与 SystemBars 分离，避免聚合语义被隐式折叠。
pub const INSET_KIND_SYSTEM_OVERLAYS: u8 = 6;

/// v1 已知 kind 域。未知值双侧拒绝，不做“未知当 0”的假默认。
pub const fn inset_kind_known(kind: u8) -> bool {
    kind <= INSET_KIND_SYSTEM_OVERLAYS
}

/// R2 可生产的 kind 集合：只有 SYSTEM 窗口遮挡。其余 kind 保留词表。
pub const PRODUCIBLE_INSET_KINDS: [u8; 1] = [INSET_KIND_SYSTEM_BARS];

/// 每边 `[left, top, right, bottom]`。
pub type InsetSides = [u16; 4];

/// Host 侧快照词表：`view_padding` = persistent occlusion；
/// `view_insets` = transient occlusion（IME）。effective padding 是派生值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OcclusionSnapshot {
    pub view_padding: InsetSides,
    pub view_insets: InsetSides,
}

impl OcclusionSnapshot {
    pub const fn new(view_padding: InsetSides, view_insets: InsetSides) -> Self {
        Self { view_padding, view_insets }
    }

    /// Flutter 同形代数：`padding = max(0, viewPadding - viewInsets)`。
    /// 结果逐边饱和到 0；调用方消费后不得回写为本快照的第二真值。
    pub fn effective_padding(&self) -> InsetSides {
        let mut out = [0u16; 4];
        for i in 0..4 {
            out[i] = self.view_padding[i].saturating_sub(self.view_insets[i]);
        }
        out
    }
}
