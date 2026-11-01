//! 蜂格渲染核（V-6 刀 2 前置段——`docs/VM里程碑设计草案.md` §4 V-6）：
//! 帧缓冲 console 的几何/打包/滚动半边，与 [`crate::font`]（字形半边）
//! 只经 `font::glyph` 一个接缝对偶。谁消费：刀 2 后段的 fbcon 服务
//! （BerryHandoff `gop_info` 的 w/h/stride/fmt 四事实 → [`Fbcon::new`]
//! → earlycon/诊断落屏）+ 本文件尾的 host 纯函数测试。
//!
//! 为什么纯函数化先行：V-6a 刀 1 已把 GOP 事实（fb 地址/几何/格式）收进
//! handoff，但「往帧缓冲写字」的每一寸都该先在 host 测对——渲染核零 IO、
//! 零全局态，输入输出全显式，走 berry-virtio/xHCI 的「纯函数 crate 化
//! 先 host 测」纪律；真机首跑只该暴露接线错，不该暴露几何错。
//!
//! 几何铁律（lib.rs 钦定，逐条执法）：
//! - 蜂格 8×16：`cols = w/8`、`rows = h/16`；右侧 `w%8` / 底部 `h%16`
//!   的 sliver **不属蜂格，一律不触碰**（固件划定的帧缓冲里 sliver 不归
//!   console——写过去 = 腐化别人的画面，且无诊断面）。
//! - stride 是**行字节数**（GOP 口径，可 > w×4）：像素 (x,y) 恒按
//!   `fb[y*stride + x*4 ..]` 寻址；行尾 padding 永不写（同上，不归我们）。
//! - 像素 = u32 小端落盘（`to_le_bytes` 字节拷贝）：AAVMF/arm64 小端面
//!   显式化，且不赌 fb 4 字节对齐（stride 契约没保证对齐，直写 u32 会
//!   在非对齐地形崩）。
//! - 滚动 = `core::ptr::copy`（memmove 语义）：源目标同在 fb 内必重叠，
//!   `copy_from_slice` 自重叠 = UB（P6 A-7 组帧立法同款）。
//! - blit 位序与 font 对偶：**MSB=最左列**（font 约定的消费侧执法点）。

/// GOP 像素格式（UEFI `EFI_GRAPHICS_PIXEL_FORMAT` 原值对齐——handoff
/// `gop_format` 透传过来就是这批值，枚举判别值即协议值，零翻译层）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 0 = PixelRedGreenBlueReserved8BitPerColor：R@bits16-23、G@8-15、B@0-7。
    Rgbx = 0,
    /// 1 = PixelBlueGreenRedReserved8BitPerColor：B@bits16-23、G@8-15、R@0-7。
    Bgrx = 1,
}

impl PixelFormat {
    /// handoff `gop_format` 原值 → 格式。2 = PixelBitMask / 3 = PixelBltOnly
    /// → `None`（前者要解位掩码、后者根本没有线性 fb——留刀 2 后段；
    /// 宁缺勿猜：把 BitMask 猜成任一硬编码格式 = 整屏换色且静默）。
    pub fn from_raw(v: u32) -> Option<PixelFormat> {
        match v {
            0 => Some(PixelFormat::Rgbx),
            1 => Some(PixelFormat::Bgrx),
            _ => None,
        }
    }

    /// RGB 三元组 → 32 位像素值（edk2 GOP 口径：通道序自 MSB 起——
    /// RGBX 即 R 在最高内容字节，与手写 `0x00RRGGBB` 直觉一致）。
    pub fn pack(&self, r: u8, g: u8, b: u8) -> u32 {
        match self {
            PixelFormat::Rgbx => (r as u32) << 16 | (g as u32) << 8 | b as u32,
            PixelFormat::Bgrx => (b as u32) << 16 | (g as u32) << 8 | r as u32,
        }
    }
}

/// 蜂格 console 渲染器：持有一片帧缓冲借用 + 光标/配景色，只写蜂格区
/// （sliver 与行尾 padding 结构性不可达——见模块头铁律）。
///
/// 两套坐标面互不纠缠：`put_char` 显式蜂格坐标（一次性定位，不动光标）；
/// `put_str` 内部流式光标（`'\n'`/行尾回绕推进）。`clear`/`scroll_up`
/// 纯屏面操作，均不动光标——「`scroll_up(n ≥ rows)` 等价 `clear(bg)`」
/// 在内容面与光标面严格同形，归位语义留给上层（服务可重建实例）。
pub struct Fbcon<'a> {
    /// 帧缓冲借用（服务侧是 VMO 映射；测试侧是宿主 Vec）。
    fb: &'a mut [u8],
    /// 像素宽/高（sliver 含在内的几何全集——写入面按 cols/rows 收敛）。
    w: usize,
    h: usize,
    /// 行字节数（≥ w×4，`new` 拒面执法）。
    stride: usize,
    fmt: PixelFormat,
    /// `put_str` 流式光标（蜂格坐标；写满行后的「待回绕」态合法：
    /// col == cols，下一字节先回绕再落笔）。
    col: usize,
    row: usize,
    /// 配景记账：`scroll_up` 无 bg 参，滚出的空行取**最近一次写入面**
    /// 携带的 bg（`put_str` 入口即记——首字节就是 `'\n'` 的滚屏也在
    /// 当前调配色内；越界的 `put_char` 不记账——被拒调用不改屏上语义）。
    /// `new` 初始黑。
    bg: (u8, u8, u8),
}

impl<'a> Fbcon<'a> {
    /// 几何验收 + 构造。拒面（ABI 边界纪律——畸形几何绝不带病进渲染，
    /// 宁可 `None` 让 caller loud）：
    /// - `w < 8 || h < 16`：容纳不下一个蜂格；
    /// - `stride < w*4`：一行像素都铺不下；
    /// - `fb.len() < stride*(h-1) + w*4`：最后像素 (w-1, h-1) 的尾字节
    ///   触不到 fb 末尾（乘加 checked 防敌意巨参溢出——「永不 panic」
    ///   的库内版）。
    ///
    /// 达标下界恰含边界：`fb.len() == stride*(h-1) + w*4` 合法。
    pub fn new(
        fb: &'a mut [u8],
        w: usize,
        h: usize,
        stride: usize,
        fmt: PixelFormat,
    ) -> Option<Fbcon<'a>> {
        if w < 8 || h < 16 {
            return None;
        }
        let row_bytes = w.checked_mul(4)?;
        if stride < row_bytes {
            return None;
        }
        let need = stride.checked_mul(h - 1)?.checked_add(row_bytes)?;
        if fb.len() < need {
            return None;
        }
        Some(Fbcon {
            fb,
            w,
            h,
            stride,
            fmt,
            col: 0,
            row: 0,
            bg: (0, 0, 0),
        })
    }

    /// 蜂格列数 = w/8（右侧余数 sliver 不属蜂格）。
    pub fn cols(&self) -> usize {
        self.w / 8
    }

    /// 蜂格行数 = h/16（底部余数 sliver 不属蜂格）。
    pub fn rows(&self) -> usize {
        self.h / 16
    }

    /// 单像素落盘：stride 寻址 + u32 小端字节拷贝（4B 定长、源是栈上
    /// 数组——非重叠，`copy_from_slice` 合法；P6 A-7 禁令只针对滚动面
    /// 的整段重叠搬移）。
    #[inline]
    fn put_px(&mut self, x: usize, y: usize, px: u32) {
        let off = y * self.stride + x * 4;
        self.fb[off..off + 4].copy_from_slice(&px.to_le_bytes());
    }

    /// 蜂格 blit：显式坐标写一字节字符。越界（col/row 越蜂格面）→
    /// `false` 且不写；缺字形（非 ASCII / 控制符，`font::glyph` 返
    /// `None`）→ 按空格语义全 bg 铺蜂格——绝不把陈旧数据当字形写出去。
    pub fn put_char(
        &mut self,
        col: usize,
        row: usize,
        ch: u8,
        fg: (u8, u8, u8),
        bg: (u8, u8, u8),
    ) -> bool {
        if col >= self.cols() || row >= self.rows() {
            return false;
        }
        self.bg = bg;
        const BLANK: [u8; 16] = [0; 16]; // 全零位图 = 空格（bit 全 0 → 全 bg）
        let g: &[u8] = match crate::font::glyph(ch) {
            Some(g) => &g[..],
            None => &BLANK,
        };
        let fg_px = self.fmt.pack(fg.0, fg.1, fg.2);
        let bg_px = self.fmt.pack(bg.0, bg.1, bg.2);
        for gy in 0..16 {
            let bits = g[gy];
            for gx in 0..8 {
                // MSB=最左列（font 约定对偶）。
                let px = if bits & (0x80 >> gx) != 0 { fg_px } else { bg_px };
                self.put_px(col * 8 + gx, row * 16 + gy, px);
            }
        }
        true
    }

    /// 流式写字（内部光标推进，跨调用持续）：
    /// - `'\n'` → col=0、row+1，越底自动 `scroll_up(1)`；
    /// - 行尾（col == cols）自动回绕（隐式续行——长行不打断内容）；
    /// - 其余字节经 `put_char`（缺字形 → 空格蜂格）。&str 逐字节处理：
    ///   非 ASCII 多字节序列每字节各占一空格蜂格（v1 字形面只有
    ///   ASCII，占位保蜂格对齐不炸不漏）。
    pub fn put_str(&mut self, s: &str, fg: (u8, u8, u8), bg: (u8, u8, u8)) {
        // 入口即记账：本调用内自动滚屏露出的空行用当前 bg。
        self.bg = bg;
        for &b in s.as_bytes() {
            if b == b'\n' {
                self.col = 0;
                self.feed_row();
                continue;
            }
            if self.col >= self.cols() {
                self.col = 0;
                self.feed_row();
            }
            self.put_char(self.col, self.row, b, fg, bg);
            self.col += 1;
        }
    }

    /// 光标下移一行；越底（row == rows）滚一屏行并把光标钉在新底行。
    #[inline]
    fn feed_row(&mut self) {
        self.row += 1;
        if self.row >= self.rows() {
            self.scroll_up(1);
            self.row = self.rows() - 1;
        }
    }

    /// 上移 n 蜂格行，底部 n 行铺 bg（取最近记账 bg——见字段注）。
    /// `n >= rows` 等价 `clear(bg)`；n == 0 无操作。不触 sliver/padding。
    pub fn scroll_up(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let rows = self.rows();
        if n >= rows {
            self.clear(self.bg);
            return;
        }
        let cell_h = rows * 16; // 蜂格区像素高（底 sliver 之外）
        let cell_w = self.cols() * 8; // 蜂格区像素宽（右 sliver 之外）
        let bytes = cell_w * 4; // 行内容宽（行移不带 padding——它不归我们）
        // 行 [0, cell_h - n*16) ← 行 [n*16, cell_h)：逐行 memmove。
        // unsafe 论证（P6 A-7：源目标同片必重叠，copy_from_slice = UB）：
        // - 边界由 `new` 拒面导出：max(src) = (cell_h-1)*stride + bytes
        //   ≤ (h-1)*stride + w*4 ≤ fb.len()，max(dst) 更低——无越界；
        // - `core::ptr::copy` = memmove 语义，重叠合法；
        // - u8 拷贝，无对齐要求。
        let base = self.fb.as_mut_ptr();
        for dy in 0..cell_h - n * 16 {
            let sy = dy + n * 16;
            unsafe {
                core::ptr::copy(base.add(sy * self.stride), base.add(dy * self.stride), bytes);
            }
        }
        // 底部 n 蜂格行铺 bg（蜂格宽度内——sliver 不碰）。
        let bg_px = self.fmt.pack(self.bg.0, self.bg.1, self.bg.2);
        for y in cell_h - n * 16..cell_h {
            for x in 0..cell_w {
                self.put_px(x, y, bg_px);
            }
        }
    }

    /// 全蜂格面铺 bg（sliver/padding 不碰；光标不动——见类型注）。
    pub fn clear(&mut self, bg: (u8, u8, u8)) {
        // 记账同 put_str：清屏后的自动滚屏空行沿用本次 bg。
        self.bg = bg;
        let px = self.fmt.pack(bg.0, bg.1, bg.2);
        let (cell_w, cell_h) = (self.cols() * 8, self.rows() * 16);
        for y in 0..cell_h {
            for x in 0..cell_w {
                self.put_px(x, y, px);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 读单像素（u32 小端对偶解码）。
    fn px(fb: &[u8], stride: usize, x: usize, y: usize) -> u32 {
        let o = y * stride + x * 4;
        u32::from_le_bytes(fb[o..o + 4].try_into().unwrap())
    }

    // —— new 拒面四路 + 恰好达标的边界 ——
    #[test]
    fn new_rejects_degenerate_geometry() {
        // w < 8（其余合法：h=16、stride=28=w*4、fb 恰够）。
        let mut fb = vec![0u8; 28 * 15 + 28];
        assert!(Fbcon::new(&mut fb, 7, 16, 28, PixelFormat::Rgbx).is_none());
        // h < 16。
        let mut fb = vec![0u8; 32 * 15 + 32];
        assert!(Fbcon::new(&mut fb, 8, 15, 32, PixelFormat::Rgbx).is_none());
        // stride < w*4（64×4=256，给 255）。
        let mut fb = vec![0u8; 255 * 31 + 255];
        assert!(Fbcon::new(&mut fb, 64, 32, 255, PixelFormat::Rgbx).is_none());
        // fb 过小：恰差 1 字节（need = 256*31+256 = 8192，给 8191）。
        let mut fb = vec![0u8; 8191];
        assert!(Fbcon::new(&mut fb, 64, 32, 256, PixelFormat::Rgbx).is_none());
        // 边界正面：need 恰等（8192）合法——拒面不含「恰好装下」。
        let mut fb = vec![0u8; 8192];
        assert!(Fbcon::new(&mut fb, 64, 32, 256, PixelFormat::Rgbx).is_some());
    }

    // —— cols/rows 取整 ——
    #[test]
    fn cols_rows_floor() {
        // 100x33 → 12x2（右 4px / 底 1px sliver）。
        let mut fb = vec![0u8; 400 * 32 + 400];
        let c = Fbcon::new(&mut fb, 100, 33, 400, PixelFormat::Rgbx).unwrap();
        assert_eq!((c.cols(), c.rows()), (12, 2));
        drop(c);
        // 最小蜂格恰一格。
        let mut fb = vec![0u8; 32 * 15 + 32];
        let c = Fbcon::new(&mut fb, 8, 16, 32, PixelFormat::Bgrx).unwrap();
        assert_eq!((c.cols(), c.rows()), (1, 1));
    }

    // —— sliver 铁律：右 w%8 / 底 h%16 永不触碰（写满 + 滚屏 + 清屏全路径）——
    #[test]
    fn sliver_never_touched() {
        // w=100 → cols=12（右 4px sliver）；h=33 → rows=2（底 1px sliver）。
        let mut fb = vec![0xEEu8; 400 * 32 + 400];
        {
            let mut c = Fbcon::new(&mut fb, 100, 33, 400, PixelFormat::Rgbx).unwrap();
            // 24 字符写满 12x2 蜂格（第 24 字后 col=12 待回绕态）。
            c.put_str("ABCDEFGHIJKLMNOPQRSTUVWX", (255, 255, 255), (0, 0, 0));
            // 再写一字：回绕 → 越底滚屏（滚动行移 + 底行 bg 铺填全路径）。
            c.put_str("Y", (255, 255, 255), (0, 0, 0));
            c.clear((0, 0, 0));
            c.put_str("Z", (255, 255, 255), (0, 0, 0));
        }
        // 右 sliver：所有 33 行 x∈[96,100) 原样 0xEE。
        for y in 0..33 {
            for x in 96..100 {
                let o = y * 400 + x * 4;
                assert_eq!(fb[o..o + 4], [0xEE; 4], "right sliver @({x},{y})");
            }
        }
        // 底 sliver：y=32 整行（含蜂格宽度内）原样。
        for x in 0..100 {
            let o = 32 * 400 + x * 4;
            assert_eq!(fb[o..o + 4], [0xEE; 4], "bottom sliver x={x}");
        }
    }

    // —— stride padding 面：内容列寻址不漂 + padding 字节未动 ——
    #[test]
    fn stride_padding_and_addressing() {
        // w=16（cols=2）h=32（rows=2）stride=80（行内容 64B + 16B padding）。
        let mut fb = vec![0xEEu8; 80 * 32];
        {
            let mut c = Fbcon::new(&mut fb, 16, 32, 80, PixelFormat::Rgbx).unwrap();
            c.put_char(0, 1, b'#', (255, 255, 255), (0, 0, 0));
        }
        // 蜂格 (0,1) 像素 y∈[16,32) x∈[0,8)：白/黑二值——若误按 w*4 当
        // stride 寻址，这些像素落在 0xEE 区、此区全 0xEE。
        for y in 16..32 {
            for x in 0..8 {
                let v = px(&fb, 80, x, y);
                assert!(v == 0xFFFFFF || v == 0, "cell(0,1)@({x},{y})={v:#x}");
            }
        }
        // 蜂格 (0,0) 未写仍 0xEE——错位寻址（w*4 当 stride）会踩进这里。
        for y in 0..16 {
            for x in 0..8 {
                let o = y * 80 + x * 4;
                assert_eq!(fb[o..o + 4], [0xEE; 4], "cell(0,0)@({x},{y})");
            }
        }
        // 每行 16B padding 原样。
        for y in 0..32 {
            assert_eq!(fb[y * 80 + 64..y * 80 + 80], [0xEE; 16], "padding y={y}");
        }
    }

    // —— put_char 越界拒面 ——
    #[test]
    fn put_char_bounds() {
        let mut fb = vec![0u8; 32 * 31 + 32]; // 8x32 → 1x2 蜂格
        let mut c = Fbcon::new(&mut fb, 8, 32, 32, PixelFormat::Rgbx).unwrap();
        assert_eq!((c.cols(), c.rows()), (1, 2));
        assert!(c.put_char(0, 0, b'#', (255, 255, 255), (0, 0, 0)));
        assert!(!c.put_char(1, 0, b'#', (255, 255, 255), (0, 0, 0)), "col == cols");
        assert!(!c.put_char(0, 2, b'#', (255, 255, 255), (0, 0, 0)), "row == rows");
    }

    // —— (0,0) 写 '#'：逐像素对拍 font::glyph（fg 白 bg 黑、MSB=最左列）——
    #[test]
    fn put_char_glyph_bit_exact() {
        let mut fb = vec![0u8; 256 * 31 + 256]; // 64x32 stride=256
        {
            let mut c = Fbcon::new(&mut fb, 64, 32, 256, PixelFormat::Rgbx).unwrap();
            assert!(c.put_char(0, 0, b'#', (255, 255, 255), (0, 0, 0)));
        }
        let g = crate::font::glyph(b'#').expect("font 覆盖可打印 ASCII 32..=126");
        for gy in 0..16 {
            for gx in 0..8 {
                let expect = if g[gy] & (0x80 >> gx) != 0 { 0xFFFFFF } else { 0 };
                assert_eq!(px(&fb, 256, gx, gy), expect, "@({gx},{gy})");
            }
        }
    }

    // —— 缺字形（非 ASCII/控制符）→ 全 bg 蜂格，不写垃圾 ——
    #[test]
    fn put_char_missing_glyph_fills_bg() {
        let bgc = (0xAA, 0xBB, 0xCC);
        let expect = 0x00AA_BB_CC; // Rgbx pack
        for ch in [0x01u8, b'\t', 0x7f, 0xC3] {
            // 0x01/0x09 控制、0x7f=DEL（font 覆盖 32..=126 之外）、
            // 0xC3 非 ASCII（UTF-8 引导字节面）。
            let mut fb = vec![0u8; 256 * 31 + 256];
            {
                let mut c = Fbcon::new(&mut fb, 64, 32, 256, PixelFormat::Rgbx).unwrap();
                assert!(c.put_char(2, 1, ch, (255, 255, 255), bgc), "ch={ch:#x}");
            }
            for gy in 0..16 {
                for gx in 0..8 {
                    assert_eq!(
                        px(&fb, 256, 16 + gx, 16 + gy),
                        expect,
                        "ch={ch:#x} @({gx},{gy})"
                    );
                }
            }
            // 蜂格外零扰动（fb 初值 0）。
            assert_eq!(px(&fb, 256, 0, 0), 0, "ch={ch:#x} 邻格被踩");
            assert_eq!(px(&fb, 256, 8, 16), 0, "ch={ch:#x} 右邻格被踩");
        }
    }

    // —— '\n'：col=0、row+1 ——
    #[test]
    fn put_str_newline_advances_row() {
        // 1x2 蜂格：A\nB → A@(0,0)、B@(0,1)。
        let mut fb = vec![0u8; 32 * 31 + 32];
        let mut ref_fb = vec![0u8; 32 * 31 + 32];
        {
            let mut c = Fbcon::new(&mut fb, 8, 32, 32, PixelFormat::Rgbx).unwrap();
            c.put_str("A\nB", (255, 255, 255), (0, 0, 0));
            let mut r = Fbcon::new(&mut ref_fb, 8, 32, 32, PixelFormat::Rgbx).unwrap();
            r.put_char(0, 0, b'A', (255, 255, 255), (0, 0, 0));
            r.put_char(0, 1, b'B', (255, 255, 255), (0, 0, 0));
        }
        assert_eq!(fb, ref_fb);
    }

    // —— 行尾回绕（无换行隐式续行）——
    #[test]
    fn put_str_wraps_at_eol() {
        // 2x2 蜂格：AB CD 四字恰满屏，C/D 全靠回绕落位。
        let mut fb = vec![0u8; 64 * 31 + 64];
        let mut ref_fb = vec![0u8; 64 * 31 + 64];
        {
            let mut c = Fbcon::new(&mut fb, 16, 32, 64, PixelFormat::Rgbx).unwrap();
            c.put_str("ABCD", (255, 255, 255), (0, 0, 0));
            let mut r = Fbcon::new(&mut ref_fb, 16, 32, 64, PixelFormat::Rgbx).unwrap();
            r.put_char(0, 0, b'A', (255, 255, 255), (0, 0, 0));
            r.put_char(1, 0, b'B', (255, 255, 255), (0, 0, 0));
            r.put_char(0, 1, b'C', (255, 255, 255), (0, 0, 0));
            r.put_char(1, 1, b'D', (255, 255, 255), (0, 0, 0));
        }
        assert_eq!(fb, ref_fb);
    }

    // —— 写满一屏后再 put_str：首行滚没、末行 = 新内容（自动滚屏）——
    #[test]
    fn put_str_autoscrolls_when_full() {
        // 2x2 蜂格三条路都收敛到 [C D / E 空]：①满屏后回绕触发滚屏；
        // ②满屏后 '\n' 越底触发滚屏；③单串内回绕触发滚屏。
        let variants: [&str; 3] = ["E", "\nE", "ABCDE"];
        for v in variants {
            let mut fb = vec![0u8; 64 * 31 + 64];
            let mut ref_fb = vec![0u8; 64 * 31 + 64];
            {
                let mut c = Fbcon::new(&mut fb, 16, 32, 64, PixelFormat::Rgbx).unwrap();
                c.put_str("ABCD", (255, 255, 255), (0, 0, 0));
                c.put_str(v, (255, 255, 255), (0, 0, 0));
                let mut r = Fbcon::new(&mut ref_fb, 16, 32, 64, PixelFormat::Rgbx).unwrap();
                r.put_char(0, 0, b'C', (255, 255, 255), (0, 0, 0));
                r.put_char(1, 0, b'D', (255, 255, 255), (0, 0, 0));
                r.put_char(0, 1, b'E', (255, 255, 255), (0, 0, 0));
                // 滚屏露出的空蜂格 = 全 bg（缺字形蜂格恰为其复刻）。
                r.put_char(1, 1, 0x01, (255, 255, 255), (0, 0, 0));
            }
            assert_eq!(fb, ref_fb, "variant {v:?}");
        }
    }

    // —— scroll_up：部分上移 + n>=rows 等价清屏 ——
    #[test]
    fn scroll_up_partial_then_full() {
        // 2x3 蜂格：三行 AB/CD/EF，上移 1 → [CD/EF/空]。
        let mut fb = vec![0u8; 64 * 47 + 64];
        let mut ref_fb = vec![0u8; 64 * 47 + 64];
        {
            let mut c = Fbcon::new(&mut fb, 16, 48, 64, PixelFormat::Rgbx).unwrap();
            c.put_str("AB\nCD\nEF", (255, 255, 255), (0, 0, 0));
            c.scroll_up(1);
            let mut r = Fbcon::new(&mut ref_fb, 16, 48, 64, PixelFormat::Rgbx).unwrap();
            r.put_char(0, 0, b'C', (255, 255, 255), (0, 0, 0));
            r.put_char(1, 0, b'D', (255, 255, 255), (0, 0, 0));
            r.put_char(0, 1, b'E', (255, 255, 255), (0, 0, 0));
            r.put_char(1, 1, b'F', (255, 255, 255), (0, 0, 0));
            r.put_char(0, 2, 0x01, (255, 255, 255), (0, 0, 0));
            r.put_char(1, 2, 0x01, (255, 255, 255), (0, 0, 0));
        }
        assert_eq!(fb, ref_fb, "scroll_up(1)");
        // n >= rows（5 > 3）→ 等价 clear(bg)：记账 bg=黑 → 全蜂格 0。
        {
            let mut c = Fbcon::new(&mut fb, 16, 48, 64, PixelFormat::Rgbx).unwrap();
            c.scroll_up(5);
        }
        for y in 0..48 {
            for x in 0..16 {
                assert_eq!(px(&fb, 64, x, y), 0, "@({x},{y})");
            }
        }
    }

    // —— pack 不对称色位序 ——
    #[test]
    fn pack_asymmetric_color() {
        let (r, g, b) = (0xAA, 0xBB, 0xCC);
        let rgbx = PixelFormat::Rgbx.pack(r, g, b);
        let bgrx = PixelFormat::Bgrx.pack(r, g, b);
        assert_eq!(rgbx, 0x00AA_BB_CC);
        assert_eq!(bgrx, 0x00CC_BB_AA);
        assert_ne!(rgbx, bgrx);
    }

    // —— from_raw：0/1 → Some；2=BitMask/3=BltOnly → None ——
    #[test]
    fn pixel_format_from_raw() {
        assert_eq!(PixelFormat::from_raw(0), Some(PixelFormat::Rgbx));
        assert_eq!(PixelFormat::from_raw(1), Some(PixelFormat::Bgrx));
        assert_eq!(PixelFormat::from_raw(2), None, "PixelBitMask 留刀 2 后段");
        assert_eq!(PixelFormat::from_raw(3), None, "PixelBltOnly 无线性 fb");
        assert_eq!(PixelFormat::from_raw(4), None, "敌意值同拒");
    }

    // —— 连续多屏长串（对抗审查 F3）：滚屏串联不 panic、末屏收敛 ——
    #[test]
    fn put_str_multi_screen_stress() {
        // 2x2 蜂格（4 字/屏）：16 字 = 4 屏。逐字追踪收敛：满屏后每 2 字
        // 滚一行，末 4 字 M..P 恰落满末屏——首 12 字全部滚没。
        let text = "ABCDEFGHIJKLMNOP";
        assert_eq!(text.len(), 16);
        let mut fb = vec![0u8; 64 * 31 + 64];
        let mut ref_fb = vec![0u8; 64 * 31 + 64];
        {
            let mut c = Fbcon::new(&mut fb, 16, 32, 64, PixelFormat::Rgbx).unwrap();
            c.put_str(text, (255, 255, 255), (0, 0, 0));
            let mut r = Fbcon::new(&mut ref_fb, 16, 32, 64, PixelFormat::Rgbx).unwrap();
            r.put_char(0, 0, b'M', (255, 255, 255), (0, 0, 0));
            r.put_char(1, 0, b'N', (255, 255, 255), (0, 0, 0));
            r.put_char(0, 1, b'O', (255, 255, 255), (0, 0, 0));
            r.put_char(1, 1, b'P', (255, 255, 255), (0, 0, 0));
        }
        assert_eq!(fb, ref_fb);
    }

    // —— 滚屏露出行的 bg 记账（对抗审查 F3）：非黑可区分色差分 ——
    #[test]
    fn scroll_exposed_row_uses_last_write_bg() {
        // 红底写满 2x2 屏 + 1 字触发自动滚屏：E 落 (0,1)，露出蜂格 (1,1)
        // 整格应为纯红 pack（0x00FF0000）——bg 记账只测过黑色时此面不可区分。
        let red = (255u8, 0, 0);
        let mut fb = vec![0u8; 64 * 31 + 64];
        {
            let mut c = Fbcon::new(&mut fb, 16, 32, 64, PixelFormat::Rgbx).unwrap();
            c.put_str("ABCDE", (255, 255, 255), red);
        }
        for y in 16..32 {
            for x in 8..16 {
                assert_eq!(px(&fb, 64, x, y), 0x00FF_0000, "@({x},{y})");
            }
        }
    }
}
