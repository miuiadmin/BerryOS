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

WALL=Image.open(REPO/'user/bins/berry-launcher/ohwallpaper/img_wallpaper_default.jpg').convert('RGB')
crop_w=round(WALL.height*135/240)
if crop_w>WALL.width:
 crop_h=round(WALL.width*240/135); WALL=WALL.crop(((WALL.width-crop_h)//2,0,(WALL.width+crop_h)//2,WALL.height))
else:
 WALL=WALL.crop(((WALL.width-crop_w)//2,0,(WALL.width+crop_w)//2,WALL.height))
WALL=WALL.resize((135,240),Image.Resampling.LANCZOS)

def wall_at(x,y):
 # 1080×1920 目标中心 → 135×240 官方壁纸源，复刻 SmoothArgb 的
 # Q16 floor + Q8 四采样核（全不透明源，RGB 直接归一加权）。
 def axis(v,n,d):
  c=((2*v+1)*n*65536//(2*d))-32768
  raw=int(c/65536)  # Rust trunc-toward-zero；负边缘由 clamp+max(0) 退化。
  i=min(max(raw,0),n-1); j=min(i+1,n-1)
  f=max(0,c-raw*65536)//256
  return i,j,f
 x0,x1,fx=axis(x,135,1080); y0,y1,fy=axis(y,240,1920)
 ps=[WALL.getpixel((x0,y0)),WALL.getpixel((x1,y0)),WALL.getpixel((x0,y1)),WALL.getpixel((x1,y1))]
 ws=[(256-fx)*(256-fy),fx*(256-fy),(256-fx)*fy,fx*fy]
 return tuple(sum(p[k]*w for p,w in zip(ps,ws))//65536 for k in range(3))

def status_px(name,w,h,dx,dy):
 # systemui 图标行 opacity=0.7；v1 无 alpha blend，按官方壁纸在固定
 # 布局位置预合成 0.7*white + 0.3*wallpaper。
 im=Image.open(ROOT/f'{name.lower()}.png').convert('RGBA').resize((w,h))
 out=[]
 for y in range(h):
  for x in range(w):
   r,g,b,a=im.getpixel((x,y))
   if a<128: out.append(0); continue
   wr,wg,wb=wall_at(dx+x,dy+y)
   out.append(0xFF00_0000 | (round(255*.7+wr*.3)<<16) | (round(255*.7+wg*.3)<<8) | round(255*.7+wb*.3))
 return out
def dock_segments():
 # PhoneSmartDock 2x 源：668×156、radius44、padding24、icon108、
 # gap62.67→63；目标 3x = 1002×234 由 ScaledArgb 1.5x 采样。
 W,H=668,156
 mask=Image.new('L',(W,H),0)
 ImageDraw.Draw(mask).rounded_rectangle([0,0,W-1,H-1],radius=44,fill=255)
 pill=Image.new('RGBA',(W,H),(0,0,0,0)); p=pill.load(); a=0x85/255
 # 官方 #85FAFAFA 与壁纸逐像素预合成（v1 无 alpha blend）；目标 dock
 # 左上 = (39,1551)，源 668×156 → 目标 1002×234 的 1.5x 映射。
 for y in range(H):
  for x in range(W):
   if mask.getpixel((x,y))!=0:
    bg=wall_at(39+round(x*1.5),1551+round(y*1.5))
    com=tuple(round(bg[k]*(1-a)+250*a) for k in range(3))+(255,)
    p[x,y]=com
 order=['CONTACTS','PHOTOS','SETTINGS','MMS']
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
for name,w,h,dx in [('SIGNAL',65,45,72),('WIFI',47,45,152),('BATTERY',64,45,769)]:
 lines += [f'pub const {name}_W: u32 = {w};',f'pub const {name}_H: u32 = {h};',f'pub const {name}_PX: [u32; {w*h}] = [']
 px=status_px(name,w,h,dx,36)
 for i in range(0,len(px),12): lines.append('    '+', '.join(f'0x{v:08X}' for v in px[i:i+12])+',')
 lines += ['];','']
for i,img in enumerate(dock_segments()): emit_array(lines,f'DOCK{i}',img)
OUT.write_text('\n'.join(lines))
print(f'genohstatus: {OUT} ({OUT.stat().st_size}B)')
