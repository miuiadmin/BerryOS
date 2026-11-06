#!/usr/bin/env python3
"""生成 OpenHarmony 状态图标与 dock 分段内建面。

产物：`user/bins/berry-launcher/src/status_icons.rs`。

- signal/wifi：`ohstatus/*.svg` 官方资产光栅化后的 1-bit 白色形。
- battery：按 OpenHarmony batteryPic 官方几何复刻（body/head 尺寸）。
- dock0..3：把 OH phone SmartDock 的 668×156 源条（目标 1002×234）切成四段，每段嵌入
  一个 108×108 app 图标；四段无缝拼回即完整 dock 背板，且每段仍是
  独立 Icon 命中面（保留 per-app 点击）。
"""
import importlib.util
from pathlib import Path
from io import BytesIO
from PIL import Image, ImageDraw
REPO=Path(__file__).resolve().parent.parent
ROOT=REPO/'user/bins/berry-launcher/ohstatus'
APPS=REPO/'user/bins/appstub/apps'
OUT=REPO/'user/bins/berry-launcher/src/status_icons.rs'
# 复用 mkapp 的宿主侧唯一 BYI1 生产者（大 dock 面走 RLE，防 launcher 二进制膨胀）。
_spec=importlib.util.spec_from_file_location('mkapp', REPO/'tools/mkapp.py')
_mkapp=importlib.util.module_from_spec(_spec); _spec.loader.exec_module(_mkapp)

def mono(name,w,h):
 im=Image.open(ROOT/f'{name.lower()}.png').convert('RGBA').resize((w,h))
 return [0xFFFF_FFFF if im.getpixel((x,y))[3]>=128 else 0 for y in range(h) for x in range(w)]

def wall_color(y):
 # 与 berry-launcher desktop.rs 默认 1080×1920 三停 48 带壁纸同式。
 bands=48; i=min(y*bands//1920,bands-1); den=bands-1
 stops=[(10,28,74),(22,100,190),(8,150,136)]
 def lerp(a,b,t): return tuple(round(a[k]+(b[k]-a[k])*t) for k in range(3))
 if i*2<=den: return lerp(stops[0],stops[1],i*2/den)
 return lerp(stops[1],stops[2],(i*2-den)/den)
def dock_segments():
 # PhoneSmartDock 2x 源：668×156、radius44、padding24、icon108、
 # gap62.67→63；目标 3x = 1002×234 由 ScaledArgb 1.5x 采样。
 W,H=668,156
 mask=Image.new('L',(W,H),0)
 ImageDraw.Draw(mask).rounded_rectangle([0,0,W-1,H-1],radius=44,fill=255)
 bg=wall_color(1726); a=0x85/255
 com=tuple(round(bg[k]*(1-a)+250*a) for k in range(3))+(255,)
 pill=Image.new('RGBA',(W,H),com)
 pill.putalpha(mask)
 order=['CONTACTS','MMS','BROWSER','CAMERA']
 widths=[167,167,167,167]
 for idx,(app,x0) in enumerate(zip(order,[24,195,366,537])):
  icon=Image.open(APPS/app/'icons/icon.png').convert('RGBA').resize((108,108),Image.Resampling.LANCZOS)
  # 5-bit/channel 量化：视觉不变级差，RLE 段数大幅下降（rootfs 帽纪律）。
  icon=icon.point(lambda v: min(255, round(v/8)*8))
  pill.alpha_composite(icon,(x0,(H-108)//2))
 # 1-bit alpha 阈值化（Icon 渲染面契约）
 alpha=pill.getchannel('A').point(lambda a:255 if a>=128 else 0)
 pill.putalpha(alpha)
 return [pill.crop((x,0,x+w,H)) for x,w in zip([0,167,334,501],widths)]

def emit_array(lines,name,img):
 # dock 面保留 RGB（背景 + 彩色 app 图标），alpha 二值化后走 BYI1 RLE。
 alpha=img.getchannel('A').point(lambda a:255 if a>=128 else 0); img.putalpha(alpha)
 bio=BytesIO(); img.save(bio,format='PNG'); blob=_mkapp.png_to_byi(bio.getvalue(),f'{name}.png')
 w,h=img.size
 lines += [f'pub const {name}_W: u32 = {w};',f'pub const {name}_H: u32 = {h};',f'pub const {name}_BYI: [u8; {len(blob)}] = [']
 for i in range(0,len(blob),24): lines.append('    '+', '.join(f'0x{b:02X}' for b in blob[i:i+24])+',')
 lines += ['];','']

lines=['//! 由 tools/genohstatus.py 生成——勿手改。','//! signal/wifi = OpenHarmony 官方资产；battery/dock = OH 官方几何复刻。','']
for name,w,h in [('SIGNAL',65,45),('WIFI',47,45),('BATTERY',64,45)]:
 lines += [f'pub const {name}_W: u32 = {w};',f'pub const {name}_H: u32 = {h};',f'pub const {name}_PX: [u32; {w*h}] = [']
 px=mono(name,w,h)
 for i in range(0,len(px),12): lines.append('    '+', '.join(f'0x{v:08X}' for v in px[i:i+12])+',')
 lines += ['];','']
for i,img in enumerate(dock_segments()): emit_array(lines,f'DOCK{i}',img)
OUT.write_text('\n'.join(lines))
print(f'genohstatus: {OUT} ({OUT.stat().st_size}B)')
