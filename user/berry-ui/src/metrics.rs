//! berry-ui 度量供给面（GUI-6——`docs/GUI-6实施设计草案.md` §6 D7/D8）。
//!
//! - [`TextMetrics`]：文本内在测量缝（`scale` = 档位 id u8，**不焊**
//!   FontSize——GUI-2 增件⑧ 真字体面的迁移缝）；[`MonospaceTextMetrics`]
//!   逐位复刻 v1 现状测量（宽 = `len × cell_w`、高 = `cell_h`——
//!   `FontSize` 单源），换件前的默认供件。
//! - [`IconSource`]：图标/图片资源缝（dims 内在尺寸 + 解码后 ARGB 借用
//!   ——LRU/解码编排留消费侧）；[`EmptyIconSource`] = 默认空供件（未
//!   注册资源恒 None——空绘不炸的供给面）。
//! - 供给通道 = D28 槽制：`Ui` 持 `&'static dyn` 两槽 + setter，门面
//!   `Ui::layout()`/`render_frame()` 签名不变（host 测调用面零级联）。
//!
//! 零 panic / 零分配；宽累计 u64 → u32 饱和收口（§0 坐标铁律单源
//! `damage::u64_to_u32_sat`）。

use berry_gfx::font::FontSize;

/// 档位 id → FontSize（与 desc wire 域同序：0/1/2/3 =
/// S8x16/S16x32/S24x48/S32x48；越域 = 敌意/漂移供件——debug_assert
/// loud + 折最小档保守收口）。
fn tier(scale: u8) -> FontSize {
    match scale {
        0 => FontSize::S8x16,
        1 => FontSize::S16x32,
        2 => FontSize::S24x48,
        3 => FontSize::S32x48,
        other => {
            debug_assert!(false, "TextMetrics scale 档位 id 越域: {other}");
            FontSize::S8x16
        }
    }
}

/// 文本内在测量缝（§6 冻结签名）：`text` = 有效字节切片；`scale` = 档位
/// id（u8，不焊 FontSize——GUI-2 增件⑧ 迁移缝）。返回 (w, h)。
///
/// **v1 契约不变量（R2-8 钉）**：等长字节串必等宽（等宽字体假设）——
/// B 面 set_text「等长换文 = 纯 PAINT 零重测」三态判据的正确性前提。
/// 自定义供件（比例字体）须自证或回避该面；GUI-2 真字体落地时三态
/// 判据改经度量缓存比对内在宽（equal-len 不再蕴含等尺寸）。
pub trait TextMetrics {
    fn measure(&self, text: &[u8], scale: u8) -> (u32, u32);
}

/// v1 等宽供件：`FontSize` 单源——宽 = `len × cell_w`（u64 累计饱和
/// u32）、高 = `cell_h`。逐位复刻 layout 现状 Text 内在测量（既有 layout
/// 测试零漂移的生命线）。
pub struct MonospaceTextMetrics;

impl TextMetrics for MonospaceTextMetrics {
    fn measure(&self, text: &[u8], scale: u8) -> (u32, u32) {
        let fs = tier(scale);
        let w = text.len() as u64 * fs.cell_w() as u64;
        (crate::damage::u64_to_u32_sat(w), fs.cell_h() as u32)
    }
}

/// 图标/图片资源缝（§6 冻结签名）：`dims` = 内在尺寸（Icon w/h None 时
/// 用，未注册 → None）；`pixels` = 解码后 ARGB 借用（0xAARRGGBB
/// row-major；LRU/解码编排留消费侧）。
///
/// `is_empty` = 空供件标记（GUI-6 段C 缺省面收窄）：默认 `false`（真
/// 供件）；空供件覆写 `true`——消费侧据此把「未注册 dims/pixels」区分
/// 为**静默 (0,0)/空绘**（默认空供件面 = guest 未注册不炸）与 **loud
/// debug_assert**（真供件缺注册 = 注册 bug）两档（§4.3/§11-14 两面兼
/// 顾）。默认实现不改冻结签名面（实现方零级联）。
pub trait IconSource {
    fn dims(&self, id: u16) -> Option<(u32, u32)>;
    fn pixels(&self, id: u16) -> Option<&[u32]>;
    /// 请求 SmoothArgb 缩放（大面壁纸；小图标默认保持最近邻硬边）。
    fn smooth_scale(&self, _id: u16) -> bool {
        false
    }
    /// 空供件（恒 None）标记——见 trait 注；真供件不覆写。
    fn is_empty(&self) -> bool {
        false
    }
}

/// 默认空供件：dims/pixels 恒 None（资源未注册 = 空绘不炸的供给面）。
pub struct EmptyIconSource;

impl IconSource for EmptyIconSource {
    fn dims(&self, _id: u16) -> Option<(u32, u32)> {
        None
    }

    fn pixels(&self, _id: u16) -> Option<&[u32]> {
        None
    }

    fn is_empty(&self) -> bool {
        true
    }
}
