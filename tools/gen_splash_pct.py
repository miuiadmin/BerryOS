#!/usr/bin/env python3
"""gen_splash_pct.py — splash 百分比 HD 字模生成器（splash_pct.rs 的源）。
Helvetica 96px 1bpp 紧致 bbox，u128 行位包。改字体/字号后重跑本脚本再编内核。
依赖：Pillow（brew python3）。"""
from PIL import Image, ImageDraw, ImageFont

f = ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc', 96)
chars = [c for c in '0123456789%']
out = []
out.append('//! splash 百分比 HD 字模（自动生成勿手编——tools/gen_splash_pct.py；')
out.append('//! Helvetica 96px 1bpp 紧致 bbox，u128 行位包。iOS 式黑白 splash 专用。')
out.append('')
out.append('/// (字符, 宽, 高, 行位图)。位序：bit (w-1-x) = 列 x（左起高位）。')
out.append('pub static PCT_GLYPHS: &[(u8, u32, u32, [u128; 72])] = &[')
for c in chars:
    img = Image.new('L', (160, 160), 0)
    d = ImageDraw.Draw(img)
    d.text((20, 20), c, font=f, fill=255)
    bb = img.getbbox()
    w, h = bb[2]-bb[0], bb[3]-bb[1]
    px = img.load()
    rows = []
    for y in range(h):
        v = 0
        for x in range(w):
            if px[bb[0]+x, bb[1]+y] > 96:
                v |= 1 << (w-1-x)
        rows.append(v)
    rows += [0]*(72-h)
    out.append(f"    (b'{c}', {w}, {h}, [")
    for v in rows:
        out.append(f'        0x{v:0{(w+3)//4*4:x},')
    out.append('    ]),')
out.append('];')
open('kernel/src/splash_pct.rs', 'w').write('\n'.join(out))
print('ok')
