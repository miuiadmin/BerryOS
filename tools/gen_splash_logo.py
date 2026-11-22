#!/usr/bin/env python3
"""gen_splash_logo.py — splash BERRYOS 大字 logo HD 字模生成器（splash_logo.rs 的源）。
Helvetica Bold 120px 1bpp 紧致 bbox；行宽 475 位 > 128 → 每行 4×u128 位包。
改字体/字号后重跑本脚本再编内核。依赖：Pillow（brew python3）。"""
from PIL import Image, ImageDraw, ImageFont

f = ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc', 120, index=1)  # index 1 = Bold
text = "BerryOS"
img = Image.new('L', (1400, 220), 0)
d = ImageDraw.Draw(img)
d.text((20, 20), text, font=f, fill=255)
bb = img.getbbox()
assert bb, "empty render"
w, h = bb[2]-bb[0], bb[3]-bb[1]
px = img.load()
WORDS = (w + 127) // 128  # 每行 u128 包数
rows = []
for y in range(h):
    words = [0] * WORDS
    for x in range(w):
        if px[bb[0]+x, bb[1]+y] > 96:
            wi = x // 128
            words[wi] |= 1 << (127 - (x % 128))
    rows.append(words)
with open('kernel/src/splash_logo.rs', 'w') as fp:
    fp.write('//! splash BERRYOS 大字 logo HD 字模（自动生成勿手编——\n')
    fp.write('//! tools/gen_splash_logo.py；Helvetica Bold 120px 1bpp 紧致 bbox，\n')
    fp.write(f'//! 行宽 {w} 位 → 每行 {WORDS}×u128 位包（包 i 覆盖列 [i*128, (i+1)*128)，\n')
    fp.write('//! 包内 bit (127-(x%128)) = 列 x（左起高位））。\n\n')
    fp.write(f'/// logo 宽（px）。\npub const LOGO_W: u32 = {w};\n')
    fp.write(f'/// logo 高（px）。\npub const LOGO_H: u32 = {h};\n')
    fp.write(f'/// 每行 u128 包数。\npub const LOGO_WORDS: usize = {WORDS};\n')
    fp.write(f'/// 行位包（[{h}][{WORDS}]，包 i bit (127-(x%128)) = 列 x）。\n')
    fp.write(f'pub static LOGO_ROWS: &[[u128; {WORDS}]; {h}] = &[\n')
    for words in rows:
        fp.write('    [' + ', '.join(f'0x{v:032x}' for v in words) + '],\n')
    fp.write('];\n')
print(f"written kernel/src/splash_logo.rs  {w}x{h}  words/row={WORDS}")
