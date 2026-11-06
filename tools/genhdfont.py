#!/usr/bin/env python3
"""生成 berry-gfx 32×48 HD ASCII 点阵（Noto Sans CJK SC / 思源黑体 OFL 源）。

用法：
  python3 tools/genhdfont.py [font.otf]

默认读取 `user/berry-gfx/fonts/NotoSansCJKsc-Regular-Desktop-ZH.otf`
（ASCII + 桌面中文标签子集），写出 `user/berry-gfx/src/font_data_hd.rs`。
设备仍只消费静态点阵/A8 覆盖表；
本脚本仅在宿主改字源/字号时手工运行，生成物随源入册。
"""
from pathlib import Path
import sys
from PIL import Image, ImageDraw, ImageFont

CELL_W = 32
CELL_H = 48
FONT_SIZE = 34
BASELINE = 38
THRESHOLD = 128
SS = 4
CJK_LABELS = "".join(sorted("设置相机通讯录文件图库信息浏览器"))
REPO = Path(__file__).resolve().parent.parent


def glyph_rows(font: ImageFont.FreeTypeFont, ch: int) -> list[int]:
    img = Image.new("L", (CELL_W, CELL_H), 0)
    draw = ImageDraw.Draw(img)
    advance = round(font.getlength(chr(ch)))
    x = max(0, (CELL_W - advance) // 2)
    draw.text((x, BASELINE), chr(ch), font=font, fill=255, anchor="ls")
    rows: list[int] = []
    for y in range(CELL_H):
        row = 0
        for xpos in range(CELL_W):
            row <<= 1
            if img.getpixel((xpos, y)) >= THRESHOLD:
                row |= 1
        rows.append(row)
    return rows


def glyph_coverage(font_ss: ImageFont.FreeTypeFont, ch: int) -> list[int]:
    # 4x 超采样 + box filter：一个 32×48 输出像素聚合 16 个子样本，
    # 保持 34px/基线 38 的 HD 形状不变，仅把硬边换成覆盖率。
    big_w, big_h = CELL_W * SS, CELL_H * SS
    img = Image.new("L", (big_w, big_h), 0)
    draw = ImageDraw.Draw(img)
    advance = round(font_ss.getlength(chr(ch)))
    x = max(0, (big_w - advance) // 2)
    draw.text((x, BASELINE * SS), chr(ch), font=font_ss, fill=255, anchor="ls")
    out: list[int] = []
    for y in range(CELL_H):
        for x in range(CELL_W):
            total = 0
            for sy in range(SS):
                for sx in range(SS):
                    total += img.getpixel((x * SS + sx, y * SS + sy))
            out.append(total // (SS * SS))
    return out


def build(font_path: Path) -> str:
    font = ImageFont.truetype(str(font_path), FONT_SIZE)
    font_ss = ImageFont.truetype(str(font_path), FONT_SIZE * SS)
    lines = [
        "//! 本文件由 tools/genhdfont.py 生成——勿手改；改字源/字号后重跑生成器。",
        "//! 字源：Noto Sans CJK SC Regular（ASCII + 桌面中文标签子集）（SIL Open Font License 1.1）。",
        "//! 32×48 HD 档：ASCII 位表 + A8；中文标签字形仅 A8 覆盖表（HD 消费面）。",
        "//! 行序自上而下；每行 4 字节，MSB=最左列。",
        "",
        "pub const CELL_W: usize = 32;",
        "pub const CELL_H: usize = 48;",
        "",
        "pub static FONT_HD: [[u8; 192]; 95] = [",
    ]
    for ch in range(32, 127):
        vals = glyph_rows(font, ch)
        assert len(vals) == CELL_H
        # 32 位/行 → 恰 4 字节，大端位序与 MSB=最左列一致。
        body = ", ".join(
            f"0x{((v >> 24) & 0xff):02X}, 0x{((v >> 16) & 0xff):02X}, "
            f"0x{((v >> 8) & 0xff):02X}, 0x{v & 0xff:02X}" for v in vals
        )
        label = "SP" if ch == 32 else chr(ch)
        lines.append(f"    /* {ch:3} {label:>2} */ [{body}],")
    lines.append("];")
    lines.extend([
        "",
        "/// HD 抗锯齿覆盖面：ASCII 32..126，行主序 8-bit coverage。",
        f"pub static FONT_HD_A8: [[u8; {CELL_W * CELL_H}]; 95] = [",
    ])
    for ch in range(32, 127):
        vals = glyph_coverage(font_ss, ch)
        assert len(vals) == CELL_W * CELL_H
        body = ", ".join(f"0x{v:02X}" for v in vals)
        label = "SP" if ch == 32 else chr(ch)
        lines.append(f"    /* {ch:3} {label:>2} */ [{body}],")
    lines.append("];")
    lines.extend([
        "",
        "/// HD 中文标签 A8 覆盖面（Unicode 码点表见 CJK_CODEPOINTS）。",
        f"pub static FONT_HD_CJK_A8: [[u8; {CELL_W * CELL_H}]; {len(CJK_LABELS)}] = [",
    ])
    for ch in CJK_LABELS:
        vals = glyph_coverage(font_ss, ord(ch))
        assert len(vals) == CELL_W * CELL_H
        body = ", ".join(f"0x{v:02X}" for v in vals)
        lines.append(f"    /* U+{ord(ch):04X} {ch} */ [{body}],")
    lines.append("];")
    lines.extend([
        "",
        "/// 与 FONT_HD_CJK_A8 同序的 Unicode 码点（升序——运行期二分查找）。",
        f"pub static CJK_CODEPOINTS: [u32; {len(CJK_LABELS)}] = [",
    ])
    lines.append("    " + ", ".join(f"0x{ord(ch):X}" for ch in CJK_LABELS) + ",")
    lines.append("];")
    return "\n".join(lines) + "\n"


def main() -> int:
    arg = Path(sys.argv[1]) if len(sys.argv) > 1 else REPO / "user/berry-gfx/fonts/NotoSansCJKsc-Regular-Desktop-ZH.otf"
    out = REPO / "user/berry-gfx/src/font_data_hd.rs"
    out.write_text(build(arg))
    print(f"genhdfont: {out} ({out.stat().st_size}B, Noto Sans CJK SC {FONT_SIZE}px → {CELL_W}×{CELL_H} + 中文标签{len(CJK_LABELS)}字)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
