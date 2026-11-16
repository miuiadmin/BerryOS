//! berry-ui 度量供给面（GUI-6——`docs/GUI-6实施设计草案.md` §6 D7/D8）。
//!
//! - [`TextMetrics`]：文本内在测量缝（`scale` = 档位 id u8，**不焊**
//!   FontSize——GUI-2 增件⑧ 真字体面的迁移缝）；[`MonospaceTextMetrics`]
//!   逐位复刻 v1 现状测量（宽 = `len × cell_w`、高 = `cell_h`——
//!   `FontSize` 单源），换件前的默认供件；[`ProportionalTextMetrics`]
//!   比例供件（宽 = 逐字形 `GlyphAtlas::advance` 累计——与 paint 推进
//!   同源；三档点阵下与等宽供件逐值恒等，HD 档为真实比例宽，痛点⑥
//!   根修 03 号 §1.3-3）。
//!   按 UTF-8 码点等宽测量（宽 = `codepoints × cell_w`、高 =
//!   `cell_h`——`FontSize` 单源）；ASCII 与旧字节语义零漂，CJK 多字节
//!   折一个全宽码点。
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
/// **v1 契约不变量（R2-8 钉）**：等宽供件（[`MonospaceTextMetrics`] 与
/// 三档点阵）下等长字节串必等宽——B 面 set_text「等长换文 = 纯 PAINT
/// 零重测」三态判据的正确性前提。比例供件（[`ProportionalTextMetrics`]
/// + HD 档）下等长**不再蕴含等宽**（"iii"≠"WWW"）——set_text 三态判据
/// 已改经本缝比对内在宽（equal-len 且等宽才走纯 PAINT 分支），R2-8 面
/// 由判据侧兜底而非供件侧假设。
/// **v1 契约（R2-8 修订）**：同一字节序列测量恒等宽（确定性）；UTF-8
/// 下**等字节长不再必然等宽**（1 个 CJK 与 3 个 ASCII 同为 3B）。S7
/// `set_text` 的“等长换文 = 纯 PAINT”判据必须并列比较旧/新测量宽；
/// 自定义供件（比例字体）同样不得只看字节长度。
pub trait TextMetrics {
    fn measure(&self, text: &[u8], scale: u8) -> (u32, u32);

    /// LAYOUT-L5 box-sensitive 测量：默认退化为无界 [`Self::measure`]，
    /// 保持既有供件签名零级联。消费 wrap 的供件覆写本方法；`max_w`
    /// 来自 measure walk 的 incoming BoxConstraints。
    fn measure_with_box(&self, text: &[u8], scale: u8, _max_w: u32) -> (u32, u32) {
        self.measure(text, scale)
    }
}

/// 运行期字形供件。默认 host 面不走本 trait，直接使用 berry-gfx 静态
/// 图集；`guest-font-vmo` feature 下 BerryUI 编译掉静态 HD fallback，
/// 客机必须按字号档供给图集（S32 来自 Berry Room 系统 A8 VMO）。
pub trait FontSource {
    fn atlas(&self, scale: FontSize) -> Option<&'static dyn berry_gfx::font::GlyphAtlas>;
}

/// v1 等宽供件：`FontSize` 单源——宽 = `UTF-8 码点数 × cell_w`
/// （u64 累计饱和 u32）、高 = `cell_h`。ASCII 与旧字节长度语义零漂；
/// CJK UTF-8 三字节折一个全宽码点。
pub struct MonospaceTextMetrics;

impl TextMetrics for MonospaceTextMetrics {
    fn measure(&self, text: &[u8], scale: u8) -> (u32, u32) {
        let fs = tier(scale);
        let w = crate::utf8::count(text) as u64 * fs.cell_w() as u64;
        (crate::damage::u64_to_u32_sat(w), fs.cell_h() as u32)
    }

    fn measure_with_box(&self, text: &[u8], scale: u8, max_w: u32) -> (u32, u32) {
        let planned = crate::text_wrap::plan(text, scale, max_w, self);
        crate::text_wrap::measure(text, scale, &planned, self)
    }
}

impl MonospaceTextMetrics {
    /// 供 paint/诊断复用的换行计划（与测量共用 text_wrap 单源）。
    pub fn wrap_plan(&self, text: &[u8], scale: u8, max_w: u32) -> crate::text_wrap::WrapPlan {
        crate::text_wrap::plan(text, scale, max_w, self)
    }
}

/// 比例供件（痛点⑥根修 03 号 §1.3-3）：宽 = 逐字形 `GlyphAtlas::advance`
/// 累计、高 = `cell_h`——与 paint 侧推进同源单轨（排布/测量不会分叉）。
///
/// 三档点阵（advance 缺省 = cell_w）下输出与 [`MonospaceTextMetrics`]
/// **逐值恒等**——等宽语义零漂移，换装无三档回归面；HD 档（Noto Sans
/// CJK 比例字源）下为真实比例宽（「Browser Settings」= 269px 而等宽
/// 排布 512px，03 号 §1.1 实测）。
pub struct ProportionalTextMetrics;

impl TextMetrics for ProportionalTextMetrics {
    fn measure(&self, text: &[u8], scale: u8) -> (u32, u32) {
        let fs = tier(scale);
        let atlas = fs.atlas();
        let mut w: u64 = 0;
        let mut i = 0usize;
        while i < text.len() {
            let (cp, step) = crate::utf8::next(&text[i..]).unwrap_or((crate::utf8::REPLACEMENT, 1));
            w += atlas.advance(cp) as u64;
            i = i.saturating_add(step.max(1));
        }
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
    /// RGB565 静态源（大面壁纸专用；缺省 None = 普通 ARGB 供给）。
    fn pixels_rgb565(&self, _id: u16) -> Option<&[u16]> {
        None
    }
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
