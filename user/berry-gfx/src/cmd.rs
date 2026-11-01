//! 显示列表（六件套 ④ 记录/回放分离）：命令先录入定容
//! [`DisplayList`]，再由 `Queue::submit` 对着纹理回放——记录期与纹理
//! 几何无关（越界在回放期裁剪），同一列表可对多纹理重放。
//!
//! v1 操作集边界（规划钦定最小面，防过度设计）：fill_rect / line /
//! blit（纹理内区域拷贝）/ glyph-run / draw_image（段②：单色位图图像/
//! 图标——与字形同一位法、任意几何；ARGB 彩色面——BYI1 解码产物的
//! 绘制消费口，A 位 1-bit 透明键）/ present——fbcon 六原语超集 +
//! 文本/图像位；AA/宽色域/compute/3D 可后补。
//!
//! 坐标域：全 i32。[`Rect`] 负宽/负高 = 非法（push 期 loud 拒
//! [`CmdError::BadRect`]，绝不静默取绝对值）；零宽/零高 = 合法空操作
//! （回放期 no-op——「空操作」是显式语义，不是错误）。
//!
//! 内存纪律：命令表定容（[`MAX_CMDS`]），满表 push → `CmdError::Full`
//! loud（零 alloc 的执法点，绝不静默丢命令）。

/// 单显示列表命令容量（定容数组的静态上界；`Caps::max_cmds` 同源引用）。
pub const MAX_CMDS: usize = 256;

/// 像素坐标矩形（x/y = 左上角，w/h = 尺寸；坐标域 i32——见模块头）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    /// 构造（纯值类型——合法性谓词 [`Rect::is_valid`] 供录入/回放口执法）。
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    /// 合法谓词：宽高非负（负值非法；零 = 合法空操作）。
    pub fn is_valid(&self) -> bool {
        self.w >= 0 && self.h >= 0
    }

    /// 交叠。i64 中转防 `x+w` 溢出；结果域回 i32 失败按无交叠处理
    /// （光栅核唯一用法 = 与帧界 [0,0,w,h] 相交，结果恒在域内——病态
    /// 巨参横跨 ±2^31 时宁可空操作不出界）。
    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let x1 = self.x.max(o.x) as i64;
        let y1 = self.y.max(o.y) as i64;
        let x2 = (self.x as i64 + self.w as i64).min(o.x as i64 + o.w as i64);
        let y2 = (self.y as i64 + self.h as i64).min(o.y as i64 + o.h as i64);
        if x2 <= x1 || y2 <= y1 {
            return None;
        }
        Some(Rect {
            x: x1 as i32, // x1 = 两个 i32 的 max，恒在域内
            y: y1 as i32,
            w: i32::try_from(x2 - x1).ok()?,
            h: i32::try_from(y2 - y1).ok()?,
        })
    }
}

/// 纯色。v1 无 alpha：所有写路径 = 不透明覆盖（blend/半透明留位——
/// 扩展时不改现有语义，以新变体/字段演进，绝不挪用保留字节装 alpha）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// 绘制描述（v1 = 纯色）。**着色器 = 构建产物**（六件套 ③）：API 面
/// 无运行时编译入口、无着色器源码字符串——渐变/程序化填充等以预编译
/// 产物 ID 形态在后续刀入册，不给 JIT 留门。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaintDesc {
    /// v1 唯一着色参数：纯色。
    pub color: Color,
}

/// glyph-run 单字形定位：codepoint + 像素坐标（shape 段 v1 = codepoint
/// 直映射点阵索引，无整形引擎——文字三段解耦的第一段在此最薄）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphPos {
    /// 字节码点（v1 图集面 = 8×16 点阵 ASCII，与 fbcon 同域）。
    pub ch: u8,
    /// 字形左上角像素 x。
    pub x: i32,
    /// 字形左上角像素 y。
    pub y: i32,
}

/// glyph-run 绘制描述（文字三段的第三段——pass 内绘制）：借用切片 +
/// 图集引用零分配，随命令表存续（记录/回放自足——回放期不依赖外部
/// 上下文，图集引用入列）。
// Clone/Copy 可派生：共享引用（含 `&dyn` 胖引用）恒 Copy——glyphs
// 切片与图集引用随命令表按位复制，零分配纪律不变。Debug 手写：
// `&dyn GlyphAtlas` 无 Debug 面，省略图集只打概要。
#[derive(Clone, Copy)]
pub struct GlyphRun<'a> {
    /// 字形来源图集（v1 唯一实现 = `font::FontAtlas8x16`；trait 面向
    /// 段② 多字号/图标图集扩档）。
    pub atlas: &'a dyn crate::font::GlyphAtlas,
    /// 逐字形定位（像素坐标，越界部分回放期裁剪）。
    pub glyphs: &'a [GlyphPos],
    /// 前景色（着墨位）。
    pub fg: Color,
    /// 背景色：`Some` = 不透明衬底（零位铺 bg——fbcon `put_char` 同形）；
    /// `None` = 透明底（零位不写——glyph 区外画面穿透）。
    pub bg: Option<Color>,
}

impl core::fmt::Debug for GlyphRun<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GlyphRun")
            .field("glyphs", &self.glyphs.len())
            .field("fg", &self.fg)
            .field("bg", &self.bg)
            .finish_non_exhaustive()
    }
}

/// 图像位来源（段② `DrawImage` 的载荷）：直接位图 或 图标图集槽位。
///
/// 位法与字形同一条（[`crate::font::GlyphBits`]：行主序、每行
/// `(w+7)/8` 字节、MSB = 最左列）——一条位法、两处消费（字形蜂格 /
/// 任意几何图像）；绘制语义 = 着墨位 fg、零位 bg（`None` = 透明不写）。
///
/// Clone/Copy 可派生（位图字段与 `&dyn` 胖引用皆 Copy）；Debug 手写
/// （`&dyn IconAtlas` 无 Debug 面，概要打印几何/槽位）。
#[derive(Clone, Copy)]
pub enum ImageSrc<'a> {
    /// 直接位图（借用切片随命令表存续——记录/回放自足）。
    /// **权威构造经 `GlyphBits::new`**；字面构造绕过恰长校验的由
    /// `Queue::submit` 期 `ImagePixelMismatch` 硬门拦截（先验后写）。
    Bits(crate::font::GlyphBits<'a>),
    /// 图标图集槽位：回放期经 [`crate::icon::IconAtlas`] 解析（LRU
    /// 碰热经内部可变性——共享引用即查询）。缺图标（`icon()==None`）
    /// = 整命令 no-op（稀疏表合法形态，见 icon.rs 头注）。
    Icon {
        atlas: &'a dyn crate::icon::IconAtlas,
        id: crate::icon::IconId,
    },
    /// ARGB 彩色位图（段② 数据面的绘制消费口——[`crate::icon::decode`]
    /// 产物同序直用，零转换接驳）：`px` = `0xAARRGGBB` row-major 借用
    /// 切片，`px.len()` 必须 `== w*h`（submit 期恰长硬门同 [`ImageSrc::Bits`]
    /// ——`ImagePixelMismatch`）。
    ///
    /// **A 位 = 1-bit 透明键**：`A==0` 跳过不写（透明）、`A!=0` 一律
    /// 不透明覆盖 RGB——这不是 blend（半透明混合留位；与 mono 面
    /// `bg=None` 透明底同族先例）。[`Cmd::DrawImage`] 的 `fg/bg` 参数
    /// 对本变体不适用（像素自带色，绘制忽略）。
    Argb {
        w: u32,
        h: u32,
        px: &'a [u32],
    },
}

impl core::fmt::Debug for ImageSrc<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ImageSrc::Bits(b) => f
                .debug_struct("ImageSrc::Bits")
                .field("w", &b.w)
                .field("h", &b.h)
                .field("rows", &b.rows.len())
                .finish(),
            ImageSrc::Icon { id, .. } => f.debug_tuple("ImageSrc::Icon").field(id).finish(),
            ImageSrc::Argb { w, h, px } => f
                .debug_struct("ImageSrc::Argb")
                .field("w", w)
                .field("h", h)
                .field("px", &px.len())
                .finish(),
        }
    }
}

/// 显示列表命令（v1 操作集——见模块头边界）。
#[derive(Debug, Clone, Copy)]
pub enum Cmd<'a> {
    /// 矩形填充（回放期裁剪到 纹理界 ∩ dirty 区）。
    FillRect {
        rect: Rect,
        paint: PaintDesc,
    },
    /// 整数 Bresenham 线（无浮点；端点任意 i32，出界裁剪）。巨跨度
    /// 带窗有界——迭代上界 = 帧带宽 + 2（`raster::line` 模块注；i32
    /// 域极值端点与本帧短段同阶完成）。
    DrawLine {
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        paint: PaintDesc,
    },
    /// 纹理内区域拷贝（图像 blit 的 v1 超集位）：`src` 必须整区落在
    /// 纹理界内（回放期 loud 拒——半出界裁剪源 = 静默改语义，禁）；
    /// dst 越界照常裁剪。**重叠 = memmove 语义**（逐行 `core::ptr::copy`
    /// + 行序按重叠方向取安全向——`raster` 模块头论证）。
    Blit { src: Rect, dst_x: i32, dst_y: i32 },
    /// glyph-run 绘制（缺字形 → 全零位图 = 空格语义，fbcon 同形）。
    DrawGlyphRun { run: GlyphRun<'a> },
    /// 位图图像绘制（段②）：`src` 位图画到 `(x,y)`，出界裁剪。两族
    /// 语义按变体分派：单色面（`Bits`/`Icon`）走 `raster::mono_blit`
    /// 共核——着墨位 fg、零位 bg（`None` = 透明不写），像素语义与
    /// glyph-run 同一条；ARGB 面（`Argb`）走 `raster::argb_blit`——
    /// 像素自带色，`fg/bg` 不适用（A 位 1-bit 透明键，见变体注）。
    DrawImage {
        src: ImageSrc<'a>,
        x: i32,
        y: i32,
        fg: Color,
        bg: Option<Color>,
    },
    /// 呈现（**flip 留位**）：soft 后端 = no-op——命令直接写进目标
    /// 缓冲，整帧已直写，无可「翻」；真后端（双缓冲/virtio-gpu）在此
    /// 承接页翻转/提交，`rect` = 损伤区提示。
    Present { rect: Rect },
}

/// 显示列表录入错误（loud——零分配定容与先验执法的两个拒面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdError {
    /// 命令表满（`MAX_CMDS` 定容上界——绝不静默丢命令）。
    Full,
    /// 非法矩形（负宽/负高）。
    BadRect,
}

/// 定容显示列表：命令的记录载体（零 alloc——`[Option<Cmd>; MAX_CMDS]`
/// 栈上定容）。只能经 push/CommandEncoder 录入（`cmds` 私有 = 录入期
/// 先验的不变量有守）。
pub struct DisplayList<'a> {
    cmds: [Option<Cmd<'a>>; MAX_CMDS],
    len: usize,
}

impl<'a> DisplayList<'a> {
    pub fn new() -> DisplayList<'a> {
        DisplayList { cmds: [None; MAX_CMDS], len: 0 }
    }

    /// 追加命令。满表 → [`CmdError::Full`]；带负宽/高矩形的命令 →
    /// [`CmdError::BadRect`]（先验后录——表内容恒合法，回放期免再检）。
    pub fn push(&mut self, cmd: Cmd<'a>) -> Result<(), CmdError> {
        if self.len >= MAX_CMDS {
            return Err(CmdError::Full);
        }
        if let Some(r) = rect_of(&cmd) {
            if !r.is_valid() {
                return Err(CmdError::BadRect);
            }
        }
        self.cmds[self.len] = Some(cmd);
        self.len += 1;
        Ok(())
    }

    /// 已录命令数。
    pub fn len(&self) -> usize {
        self.len
    }

    /// 空表谓词。
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 已录命令迭代（回放入口）。
    pub fn iter(&self) -> impl Iterator<Item = &Cmd<'a>> {
        self.cmds[..self.len].iter().filter_map(|c| c.as_ref())
    }
}

impl Default for DisplayList<'_> {
    fn default() -> Self {
        DisplayList::new()
    }
}

/// 命令编码器：record 面（类型化便捷方法）→ [`DisplayList`] 终结。
/// 语义 = DisplayList.push 的薄包装（同一拒面原样透传）。
pub struct CommandEncoder<'a> {
    list: DisplayList<'a>,
}

impl<'a> CommandEncoder<'a> {
    pub fn new() -> CommandEncoder<'a> {
        CommandEncoder { list: DisplayList::new() }
    }

    /// 通用录入（类型化方法的兜底口——命令形态新变体优先走这里）。
    pub fn record(&mut self, cmd: Cmd<'a>) -> Result<(), CmdError> {
        self.list.push(cmd)
    }

    /// 矩形填充。
    pub fn fill_rect(&mut self, rect: Rect, color: Color) -> Result<(), CmdError> {
        self.record(Cmd::FillRect { rect, paint: PaintDesc { color } })
    }

    /// 整数线。
    pub fn draw_line(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: Color,
    ) -> Result<(), CmdError> {
        self.record(Cmd::DrawLine { x0, y0, x1, y1, paint: PaintDesc { color } })
    }

    /// 纹理内区域拷贝。
    pub fn blit(&mut self, src: Rect, dst_x: i32, dst_y: i32) -> Result<(), CmdError> {
        self.record(Cmd::Blit { src, dst_x, dst_y })
    }

    /// glyph-run 绘制。
    pub fn draw_glyph_run(&mut self, run: GlyphRun<'a>) -> Result<(), CmdError> {
        self.record(Cmd::DrawGlyphRun { run })
    }

    /// 单色位图图像绘制（段②）。位图几何的恰长先验在 submit 期硬门
    /// （`ImageSrc::Bits` 字面构造面）；此处无矩形可先验（出界裁剪）。
    pub fn draw_image(
        &mut self,
        src: ImageSrc<'a>,
        x: i32,
        y: i32,
        fg: Color,
        bg: Option<Color>,
    ) -> Result<(), CmdError> {
        self.record(Cmd::DrawImage { src, x, y, fg, bg })
    }

    /// 呈现（soft no-op / flip 留位）。
    pub fn present(&mut self, rect: Rect) -> Result<(), CmdError> {
        self.record(Cmd::Present { rect })
    }

    /// 终结为不可变显示列表（记录/回放分离的形态面：记录期结束）。
    pub fn finish(self) -> DisplayList<'a> {
        self.list
    }
}

impl Default for CommandEncoder<'_> {
    fn default() -> Self {
        CommandEncoder::new()
    }
}

/// 命令携带矩形的提取（BadRect 先验的执法点；无矩形命令返 None）。
fn rect_of<'a>(cmd: &'a Cmd<'_>) -> Option<&'a Rect> {
    match cmd {
        Cmd::FillRect { rect, .. } => Some(rect),
        Cmd::Blit { src, .. } => Some(src),
        Cmd::Present { rect } => Some(rect),
        Cmd::DrawLine { .. } | Cmd::DrawGlyphRun { .. } | Cmd::DrawImage { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— 容量满 loud：恰 256 过、第 257 拒 Full（零静默丢）——
    #[test]
    fn display_list_full_loud() {
        let mut dl = DisplayList::new();
        assert!(dl.is_empty());
        for i in 0..MAX_CMDS {
            // 零尺寸 Present = 合法空操作（拿它当最便宜的填充命令）。
            dl.push(Cmd::Present { rect: Rect::new(i as i32, 0, 0, 0) }).unwrap();
        }
        assert_eq!(dl.len(), MAX_CMDS);
        assert_eq!(dl.iter().count(), MAX_CMDS);
        // 满 → Full loud。
        assert_eq!(
            dl.push(Cmd::Present { rect: Rect::new(0, 0, 0, 0) }),
            Err(CmdError::Full)
        );
        // 拒后长度不变（拒面不留半执行面）。
        assert_eq!(dl.len(), MAX_CMDS);
    }

    // —— 负宽/高矩形 loud 拒（三种带矩形命令全验；零尺寸放行）——
    #[test]
    fn display_list_bad_rect_rejected() {
        let mut dl = DisplayList::new();
        let c = Color { r: 1, g: 2, b: 3 };
        assert_eq!(
            dl.push(Cmd::FillRect { rect: Rect::new(0, 0, -1, 4), paint: PaintDesc { color: c } }),
            Err(CmdError::BadRect)
        );
        assert_eq!(
            dl.push(Cmd::Blit { src: Rect::new(0, 0, 4, -1), dst_x: 0, dst_y: 0 }),
            Err(CmdError::BadRect)
        );
        assert_eq!(
            dl.push(Cmd::Present { rect: Rect::new(0, 0, 4, -1) }),
            Err(CmdError::BadRect)
        );
        assert_eq!(dl.len(), 0, "拒面零录入");
        // 零宽/零高 = 合法空操作；负 x/y 原点同样合法（回放期裁剪）。
        assert!(dl.push(Cmd::FillRect { rect: Rect::new(0, 0, 0, 8), paint: PaintDesc { color: c } }).is_ok());
        assert!(dl.push(Cmd::FillRect { rect: Rect::new(-4, -4, 8, 8), paint: PaintDesc { color: c } }).is_ok());
        // 线/glyph-run 无矩形先验，任何 i32 坐标可录（回放期裁剪）。
        assert!(dl.push(Cmd::DrawLine { x0: i32::MIN, y0: 0, x1: i32::MAX, y1: 3, paint: PaintDesc { color: c } }).is_ok());
        assert_eq!(dl.len(), 3);
    }

    // —— 编码器 record 面 → finish 终结（内容与顺序保真）——
    #[test]
    fn encoder_finish_roundtrip() {
        let c = Color { r: 9, g: 8, b: 7 };
        let mut enc = CommandEncoder::new();
        enc.fill_rect(Rect::new(1, 2, 3, 4), c).unwrap();
        enc.draw_line(0, 0, 5, 5, c).unwrap();
        enc.blit(Rect::new(0, 0, 8, 8), 4, 4).unwrap();
        enc.present(Rect::new(0, 0, 16, 16)).unwrap();
        let dl = enc.finish();
        assert_eq!(dl.len(), 4);
        let mut it = dl.iter();
        assert!(matches!(it.next(), Some(Cmd::FillRect { .. })));
        assert!(matches!(it.next(), Some(Cmd::DrawLine { .. })));
        assert!(matches!(it.next(), Some(Cmd::Blit { .. })));
        assert!(matches!(it.next(), Some(Cmd::Present { .. })));
        assert!(it.next().is_none());
    }

    // —— intersect：交叠/相离/包含/边界恰触 ——
    #[test]
    fn rect_intersect_basics() {
        let a = Rect::new(0, 0, 10, 10);
        assert_eq!(a.intersect(&Rect::new(5, 5, 10, 10)), Some(Rect::new(5, 5, 5, 5)));
        assert_eq!(a.intersect(&Rect::new(20, 20, 4, 4)), None, "相离");
        assert_eq!(
            a.intersect(&Rect::new(2, 3, 4, 5)),
            Some(Rect::new(2, 3, 4, 5)),
            "包含"
        );
        assert_eq!(a.intersect(&Rect::new(10, 0, 4, 4)), None, "边界恰触 = 空");
        // 巨参不炸：横跨 ±2^31 的矩形交帧界不 panic（huge 的 x 范围
        // [i32::MIN, -1)——与帧界 [0,100) 相离）。
        let huge = Rect::new(i32::MIN, 0, i32::MAX, 4);
        let frame = Rect::new(0, 0, 100, 100);
        assert_eq!(huge.intersect(&frame), None, "巨参 ∩ 帧界 = 空");
        // 自交恒等（w = x2-x1 = i32::MAX 恰在域内——合法 w≥0 矩形的
        // 交集宽恒 ≤ max(wa,wb) ≤ i32::MAX，try_from 兜底纯防御）。
        assert_eq!(huge.intersect(&huge), Some(Rect::new(i32::MIN, 0, i32::MAX, 4)));
        assert_eq!(
            Rect::new(-5, -5, 10, 10).intersect(&frame),
            Some(Rect::new(0, 0, 5, 5)),
            "负原点裁到帧界"
        );
    }
}
