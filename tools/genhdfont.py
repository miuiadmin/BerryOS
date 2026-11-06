#!/usr/bin/env python3
"""生成 berry-gfx 32×48 HD ASCII 点阵（Noto Sans OFL 源）。

用法：
  python3 tools/genhdfont.py [font.ttf]

默认读取 `user/berry-gfx/fonts/NotoSans-Regular.ttf`，写出
`user/berry-gfx/src/font_data_hd.rs`。设备仍只消费静态单色点阵；
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


def build(font_path: Path) -> str:
    font = ImageFont.truetype(str(font_path), FONT_SIZE)
    lines = [
        "//! 本文件由 tools/genhdfont.py 生成——勿手改；改字源/字号后重跑生成器。",
        "//! 字源：Noto Sans Regular（SIL Open Font License 1.1，见 fonts/OFL.txt）。",
        "//! 32×48 HD 档：ASCII 32..126，原生矢量光栅化（非 8×16 最近邻放大）。",
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
    return "\n".join(lines) + "\n"


def main() -> int:
    arg = Path(sys.argv[1]) if len(sys.argv) > 1 else REPO / "user/berry-gfx/fonts/NotoSans-Regular.ttf"
    out = REPO / "user/berry-gfx/src/font_data_hd.rs"
    out.write_text(build(arg))
    print(f"genhdfont: {out} ({out.stat().st_size}B, Noto Sans {FONT_SIZE}px → {CELL_W}×{CELL_H})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
