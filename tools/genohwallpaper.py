#!/usr/bin/env python3
"""从 OpenHarmony phone 默认壁纸生成 launcher 内建 135×240 静态源图。

产物：`user/bins/berry-launcher/src/wallpaper.rs`。

OpenHarmony `backgroundImageSize(ImageSize.Cover)` 在 1080×1920 竖屏上取
1000×1000 源图中央 562.5×1000 裁切。这里宿主预采样到 135×240，launcher
经 `ImageSrc::SmoothArgb` 双线性放大到整屏；静态 rodata 直供，不占 1MiB 堆。
"""
from pathlib import Path
import hashlib
from PIL import Image
REPO=Path(__file__).resolve().parent.parent
SRC=REPO/'user/bins/berry-launcher/ohwallpaper/img_wallpaper_default.jpg'
OUT=REPO/'user/bins/berry-launcher/src/wallpaper.rs'
W,H=135,240
im=Image.open(SRC).convert('RGB')
# Cover 裁切：目标宽高比 1080/1920；方图保留中央竖条。
crop_w=round(im.height*W/H)
if crop_w>im.width:
 crop_h=round(im.width*H/W); box=((im.width-crop_h)//2,0,(im.width+crop_h)//2,im.height)
else:
 box=((im.width-crop_w)//2,0,(im.width+crop_w)//2,im.height)
im=im.crop(box).resize((W,H),Image.Resampling.LANCZOS)
source_hash=hashlib.sha256(SRC.read_bytes()).hexdigest()
lines=[
 '//! 由 tools/genohwallpaper.py 生成——勿手改。',
 '//! OpenHarmony phone 默认壁纸中央 cover 裁切，135×240 静态源图。',
 f'//! 源：applications_launcher @ f75dfb6bf727...，SHA-256 {source_hash}。',
 f'pub const WALLPAPER_W: u32 = {W};',
 f'pub const WALLPAPER_H: u32 = {H};',
 f'pub const WALLPAPER_PX: [u32; {W*H}] = [',
]
for y in range(H):
 row=[]
 for x in range(W):
  r,g,b=im.getpixel((x,y)); row.append(f'0xFF{r:02X}{g:02X}{b:02X}')
 lines.append('    '+', '.join(row)+',')
lines.append('];')
OUT.write_text('\n'.join(lines)+'\n')
print(f'genohwallpaper: {OUT} ({OUT.stat().st_size}B)')
