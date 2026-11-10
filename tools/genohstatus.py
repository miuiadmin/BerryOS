#!/usr/bin/env python3
"""生成 OpenHarmony 状态图标与 dock 分段内建面。

产物：`user/bins/berry-launcher/src/status_icons.rs`。

- signal/wifi：`ohstatus/*.svg` 官方资产光栅化后的 1-bit 白色形。
- battery：按 OpenHarmony batteryPic 官方几何复刻（body/head 尺寸）。
- dock0..3：把 OH phone SmartDock 的 668×156 源条（目标 1002×234）切成四段，每段嵌入
  一个 108×108 app 图标；四段无缝拼回即完整 dock 背板，且每段仍是
  独立 Icon 命中面（保留 per-app 点击）。

用法：
  python3 tools/genohstatus.py             # 重产 status_icons.rs（硬依赖 PIL）
  python3 tools/genohstatus.py --selftest  # 门禁自检（零 PIL 可跑；PIL 在位时加做重产对拍）
"""
import importlib.util
from pathlib import Path
from io import BytesIO
import hashlib
import re
import struct
import sys

try:
    from PIL import Image, ImageDraw
except ImportError:  # selftest 结构面零依赖可跑；生成路径硬依赖 PIL
    Image = ImageDraw = None

REPO=Path(__file__).resolve().parent.parent
ROOT=REPO/'user/bins/berry-launcher/ohstatus'
APPS=REPO/'user/bins/appstub/apps'
OUT=REPO/'user/bins/berry-launcher/src/status_icons.rs'
WALL_SRC=REPO/'user/bins/berry-launcher/ohwallpaper/img_wallpaper_default.jpg'
# 复用 mkapp 的宿主侧唯一 BYI1 生产者（大 dock 面走 RLE，防 launcher 二进制膨胀）。
_spec=importlib.util.spec_from_file_location('mkapp', REPO/'tools/mkapp.py')
_mkapp=importlib.util.module_from_spec(_spec); _spec.loader.exec_module(_mkapp)

def cover_wall():
    assert Image is not None  # 生成路径硬依赖 PIL（selftest 结构面零依赖）
    wall=Image.open(WALL_SRC).convert('RGB')
    crop_w=round(wall.height*135/240)
    if crop_w>wall.width:
        crop_h=round(wall.width*240/135); wall=wall.crop(((wall.width-crop_h)//2,0,(wall.width+crop_h)//2,wall.height))
    else:
        wall=wall.crop(((wall.width-crop_w)//2,0,(wall.width+crop_w)//2,wall.height))
    return wall.resize((135,240),Image.Resampling.LANCZOS)

def wall_at(wall,x,y):
    # 1080×1920 目标中心 → 135×240 官方壁纸源，复刻 SmoothArgb 的
    # Q16 floor + Q8 四采样核（全不透明源，RGB 直接归一加权）。
    def axis(v,n,d):
        c=((2*v+1)*n*65536//(2*d))-32768
        raw=int(c/65536)  # Rust trunc-toward-zero；负边缘由 clamp+max(0) 退化。
        i=min(max(raw,0),n-1); j=min(i+1,n-1)
        f=max(0,c-raw*65536)//256
        return i,j,f
    x0,x1,fx=axis(x,135,1080); y0,y1,fy=axis(y,240,1920)
    ps=[wall.getpixel((x0,y0)),wall.getpixel((x1,y0)),wall.getpixel((x0,y1)),wall.getpixel((x1,y1))]
    ws=[(256-fx)*(256-fy),fx*(256-fy),(256-fx)*fy,fx*fy]
    return tuple(sum(p[k]*w for p,w in zip(ps,ws))//65536 for k in range(3))

def status_px(wall,name,w,h,dx,dy):
    # systemui 图标行 opacity=0.7；v1 无 alpha blend，按官方壁纸在固定
    # 布局位置预合成 0.7*white + 0.3*wallpaper。
    im=Image.open(ROOT/f'{name.lower()}.png').convert('RGBA').resize((w,h))
    out=[]
    for y in range(h):
        for x in range(w):
            r,g,b,a=im.getpixel((x,y))
            if a<128: out.append(0); continue
            wr,wg,wb=wall_at(wall,dx+x,dy+y)
            out.append(0xFF00_0000 | (round(255*.7+wr*.3)<<16) | (round(255*.7+wg*.3)<<8) | round(255*.7+wb*.3))
    return out

def dock_segments(wall):
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
                bg=wall_at(wall,39+round(x*1.5),1551+round(y*1.5))
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

def build() -> str:
    """重产 status_icons.rs 全文（供 main 写盘与 selftest 重产对拍共用）。"""
    wall=cover_wall()
    lines=['//! 由 tools/genohstatus.py 生成——勿手改。','//! signal/wifi = OpenHarmony 官方资产；battery/dock = OH 官方几何复刻。','']
    for name,w,h,dx in [('SIGNAL',65,45,72),('WIFI',47,45,152),('BATTERY',64,45,769)]:
        lines += [f'pub const {name}_W: u32 = {w};',f'pub const {name}_H: u32 = {h};',f'pub const {name}_PX: [u32; {w*h}] = [']
        px=status_px(wall,name,w,h,dx,36)
        for i in range(0,len(px),12): lines.append('    '+', '.join(f'0x{v:08X}' for v in px[i:i+12])+',')
        lines += ['];','']
    for i,img in enumerate(dock_segments(wall)): emit_array(lines,f'DOCK{i}',img)
    return '\n'.join(lines)  # 与在册产物同形（无尾换行——emit_array 尾已留空行分隔）

def main() -> int:
    if len(sys.argv)>1 and sys.argv[1]=='--selftest':
        return selftest()
    OUT.write_text(build())
    print(f'genohstatus: {OUT} ({OUT.stat().st_size}B)')
    return 0

# ---- selftest（a-3 U4 门禁化纪律；genohwallpaper/genohicons --selftest 同位） ----
#
# 本生成器此前 import 即执行、零守卫（换源/改几何须心里记得重跑）。自检面：
# - 结构钉板（零 PIL 可跑，re 解析在册产物）：7 组 W/H 头钉 + PX 数组
#   类型钉 ×3 + BYI 数组长度钉 ×4 + 生成器头注在席（勿手改契约）。
# - PX 语义钉：token 全 0x%08X 形 + alpha 域二值（0x00/0xFF——1-bit 阈值化
#   契约）+ 黄金哈希 ×3（源资产/预合成/布局漂移必炸，须有意识重钉）+
#   内容活性（uniq 色 ≥10，防退化纯色面）。
# - BYI wire 语义（对读 user/berry-gfx icon.rs decode + mkapp 编码契约）：
#   magic BYI1 + ver=1/flags=0 + 尺寸 LE u16 恰 167×156 + RLE 走查
#   Σcount==w*h + count≥1 + 帧尾恰尽（无拖尾）+ 黄金 sha256 ×4。
# - 溯源钉：五个输入源（3 状态 png + 壁纸 jpg + CONTACTS 图标 png）sha256
#   实测对账——换源不重产即炸。
# - 拒面 8：W 头钉 / PX 类型钉 / 短 token / alpha 域破坏 / BYI 长度钉 /
#   BYI magic 破坏 / RLE Σcount 破坏 / 溯源篡改。
# - PIL 在位且源在仓：build() 重产 ↔ 在册文件逐字节对拍（真漂移守卫）；
#   PIL 缺席打 skip 不 FAIL（宿主无 PIL 合法，降级面由黄金哈希兜底）。

# 黄金哈希：PX = 大端 u32 串接 sha256；BYI = 整段 wire 字节 sha256。
GOLDEN_PX = {
    'SIGNAL':  'a92a0154e474d217f896612b27f08e3aa566b0ab9a90386b0b0cca620409e872',
    'WIFI':    'fa1e071f459f39ee7bd9f31fb6ff842767ac3607ed1785d0209a1cedac380e25',
    'BATTERY': '28cb655a64a2da25cfea5cb970cc72da5113600c24c0dd6c346fafd48194c768',
}
GOLDEN_BYI = {
    'DOCK0': '682bba317305e1c01147a6be9000720b9a04fb1b318b51a4233c0da6db46a942',
    'DOCK1': '2e8e9b2d0a773b707f23aa4155dbd95035992f23263a4507b605cdbdafbe5a45',
    'DOCK2': '4c8d7e57b0ee41dc0c42c9e285efab534c07fb4cafe06fb4b6e268e7113db221',
    'DOCK3': 'da97bb0e20bfb4bf106f5052f20343bdae33501befe3426945d99b3250757613',
}
# 溯源钉：五个输入源文件 sha256（相对 ROOT；CONTACTS 条目相对 APPS）。
SRC_SHA256 = {
    'ROOT/signal.png': '3f8c91e8596f7a0e8dc5956dce79d9d08960c209b9a8bf82def0e7bf1da118e0',
    'ROOT/wifi.png': '290af7ea548752bdeae0f0c8da088fe73d744872eb93aade61adfcb56802e8ad',
    'ROOT/battery.png': '702b2ebd21c38b491aafb44f16e8b63b55892b7ab2856e76d6d4f145aae10ee9',
    'WALL/img_wallpaper_default.jpg': '20c7fa0a817edb0d630bf356714ee2cb22dcec0f57cb4a6879e2bc2c40b164b5',
    'APPS/CONTACTS/icons/icon.png': '1f649893219e5e7e2945a32725c943659adb5a904f4289b38466a856ad6487ca',
}

_PX_NAMES = ('SIGNAL', 'WIFI', 'BATTERY')
_PX_DIMS = {'SIGNAL': (65, 45), 'WIFI': (47, 45), 'BATTERY': (64, 45)}
_DOCK_DIM = (167, 156)
_DOCK_LEN = {'DOCK0': 39790, 'DOCK1': 32488, 'DOCK2': 32446, 'DOCK3': 38290}
_TOKEN32 = re.compile(r'^0x[0-9A-F]{8}$')
_TOKEN8 = re.compile(r'^0x[0-9A-F]{2}$')


def _px_body(text: str, name: str) -> list:
    w, h = _PX_DIMS[name]
    m = re.search(r'pub const %s_PX: \[u32; (\d+)\] = \[\n(.*?)\n\];' % name, text, re.S)
    if m is None:
        raise ValueError('%s_PX 数组缺席' % name)
    if int(m.group(1)) != w * h:
        raise ValueError('%s_PX 类型钉漂移: %s != %d' % (name, m.group(1), w * h))
    toks = [t.strip() for line in m.group(2).split('\n') for t in line.rstrip(',').split(',')]
    vals = []
    for t in toks:
        if not _TOKEN32.match(t):
            raise ValueError('%s_PX 坏 token 形: %r' % (name, t))
        vals.append(int(t, 16))
    if len(vals) != w * h:
        raise ValueError('%s_PX 像素数 %d != 帽 %d' % (name, len(vals), w * h))
    if any((v >> 24) not in (0x00, 0xFF) for v in vals):
        raise ValueError('%s_PX alpha 域破坏（1-bit 阈值化契约外值）' % name)
    return vals


def _byi_bytes(text: str, name: str) -> bytes:
    m = re.search(r'pub const %s_BYI: \[u8; (\d+)\] = \[\n(.*?)\n\];' % name, text, re.S)
    if m is None:
        raise ValueError('%s_BYI 数组缺席' % name)
    if int(m.group(1)) != _DOCK_LEN[name]:
        raise ValueError('%s_BYI 长度钉漂移: %s != %d' % (name, m.group(1), _DOCK_LEN[name]))
    toks = [t.strip() for line in m.group(2).split('\n') for t in line.rstrip(',').split(',')]
    out = bytearray()
    for t in toks:
        if not _TOKEN8.match(t):
            raise ValueError('%s_BYI 坏 token 形: %r' % (name, t))
        out.append(int(t, 16))
    if len(out) != _DOCK_LEN[name]:
        raise ValueError('%s_BYI 字节数 %d != 声明' % (name, len(out)))
    b = bytes(out)
    # BYI wire 语义（对读 user/berry-gfx icon.rs decode + mkapp 编码契约）：
    # magic/ver/flags/尺寸/RLE 走查——结构破坏即抛（拒面与正检同一条路径）。
    if b[:4] != b'BYI1':
        raise ValueError('%s magic 破坏: %r' % (name, b[:4]))
    if b[4] != 1 or b[5] != 0:
        raise ValueError('%s ver/flags 破坏: %d/%d' % (name, b[4], b[5]))
    w, h = struct.unpack('<HH', b[6:10])
    if (w, h) != _DOCK_DIM:
        raise ValueError('%s 尺寸破坏: %dx%d' % (name, w, h))
    off, total, cmin = 10, 0, None
    while off + 6 <= len(b):
        (c,) = struct.unpack('<H', b[off:off + 2])
        total += c
        if cmin is None or c < cmin:
            cmin = c
        off += 6
    if off != len(b):
        raise ValueError('%s RLE 帧尾拖尾 %dB' % (name, len(b) - off))
    if total != w * h:
        raise ValueError('%s Σcount=%d != %d' % (name, total, w * h))
    if cmin is None or cmin < 1:
        raise ValueError('%s 零长 RLE 段' % name)
    return b


def parse_artifact(text: str):
    """在册产物严格解析：W/H 头钉 ×7 + PX 体 ×3 + BYI 体 ×4；结构破坏即抛。"""
    dims = dict(_PX_DIMS)
    dims.update({'DOCK%d' % i: _DOCK_DIM for i in range(4)})
    for name, (w, h) in dims.items():
        for dim, want in (('W', w), ('H', h)):
            pin = 'pub const %s_%s: u32 = %d;' % (name, dim, want)
            if pin not in text:
                raise ValueError('头钉漂移: %r 不在席' % pin)
    px = {n: _px_body(text, n) for n in _PX_NAMES}
    byi = {n: _byi_bytes(text, n) for n in _DOCK_LEN}
    return px, byi


def _sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def _src_path(key: str) -> Path:
    base, rel = key.split('/', 1)
    return {'ROOT': ROOT, 'WALL': WALL_SRC.parent, 'APPS': APPS}[base] / rel


def selftest() -> int:
    art = OUT.read_text(encoding='utf-8')
    assert '由 tools/genohstatus.py 生成' in art, '生成器头注缺席（勿手改在册产物）'
    px, byi = parse_artifact(art)  # 拒面零容忍：结构坏即抛

    # —— PX 黄金哈希 + 内容活性 ——
    for n, vals in px.items():
        blob = b''.join(struct.pack('>I', v) for v in vals)
        got = _sha(blob)
        assert got == GOLDEN_PX[n], (
            '%s_PX 黄金哈希漂移 %s…（源资产/预合成/布局语义分叉）——'
            '重产: python3 tools/genohstatus.py，有意识核对后更新 GOLDEN_PX' % (n, got[:12])
        )
        uniq = len(set(vals))
        assert uniq >= 10, '%s_PX 唯一色数 %d < 10（内容退化）' % (n, uniq)

    # —— BYI 黄金哈希（wire 语义已由 parse_artifact 内 _byi_bytes 走查） ——
    for n, b in byi.items():
        got = _sha(b)
        assert got == GOLDEN_BYI[n], (
            '%s_BYI 黄金哈希漂移 %s…（dock 几何/图标源/量化语义分叉）——'
            '重产: python3 tools/genohstatus.py，有意识核对后更新 GOLDEN_BYI' % (n, got[:12])
        )

    # —— 溯源钉：五个输入源 sha256 实测对账 ——
    for key, want in SRC_SHA256.items():
        got = _sha(_src_path(key).read_bytes())
        assert got == want, '溯源漂移 %s: %s… != 在册（换源须重产入册）' % (key, got[:12])

    # —— 7 个文本拒面：parse_artifact 严格拒绝各种结构破坏 ——
    m = re.search(r'SIGNAL_PX: \[u32; 2925\] = \[\n(    .*)', art)
    first_tok = m.group(1).strip().rstrip(',').split(',')[0].strip()

    def rej(mutate, name):
        try:
            parse_artifact(mutate(art))
        except ValueError:
            return
        raise AssertionError('拒面失守: %s' % name)

    rej(lambda t: t.replace('pub const SIGNAL_W: u32 = 65;', 'pub const SIGNAL_W: u32 = 64;'), 'W 头钉漂移')
    rej(lambda t: t.replace('SIGNAL_PX: [u32; 2925]', 'SIGNAL_PX: [u32; 2900]'), 'PX 类型钉漂移')
    rej(lambda t: t.replace(first_tok + ',', first_tok[2:] + ',', 1), '短 token（剥 0x）')
    rej(lambda t: t.replace(first_tok + ',', '0x55' + first_tok[4:] + ',', 1), 'alpha 域破坏（0x55 顶字节）')
    rej(lambda t: t.replace('DOCK0_BYI: [u8; 39790]', 'DOCK0_BYI: [u8; 39789]'), 'BYI 长度钉漂移')
    rej(lambda t: t.replace('    0x42, 0x59, 0x49, 0x31,', '    0x43, 0x59, 0x49, 0x31,', 1), 'BYI magic 破坏（B→C）')
    rej(_rle_count_break, 'RLE Σcount 破坏（首段 count+1）')

    # —— 第 8 拒面：溯源篡改（源文件 +1 字节 → 对账炸；还原在 finally） ——
    saved = (ROOT / 'signal.png').read_bytes()
    try:
        (ROOT / 'signal.png').write_bytes(saved + b'\x00')
        try:
            _src_probe()
            raise AssertionError('拒面失守: 溯源篡改未炸')
        except AssertionError as e:
            if '拒面失守' in str(e):
                raise
            assert '溯源漂移' in str(e), '预期溯源漂移，得 %s' % e
    finally:
        (ROOT / 'signal.png').write_bytes(saved)

    # —— PIL 重产对拍（在位才跑；缺席打 skip，黄金哈希已兜底） ——
    pil_note = 'skip（PIL 缺席——黄金哈希兜底）'
    if Image is not None:
        regen = build()
        assert regen == art, (
            '重产对拍 FAIL：在册 status_icons.rs != 生成器重产——'
            '重产: python3 tools/genohstatus.py 并随源入册'
        )
        pil_note = 'repro byte-equal'

    print(
        'genohstatus: selftest ok (7 组结构钉板 + PX 黄金哈希 x3 + alpha 二值域 + '
        'BYI wire 走查 x4（magic/ver/尺寸/Σcount/帧尾）+ 黄金 sha256 x4 + 溯源 x5 + '
        '拒面 8；PIL 对拍 %s)' % pil_note
    )
    return 0


def _src_probe():
    """溯源对账独立探针（拒面 8 复用同一条对账路径——勿长第二套）。"""
    for key, want in SRC_SHA256.items():
        got = _sha(_src_path(key).read_bytes())
        assert got == want, '溯源漂移 %s: %s… != 在册（换源须重产入册）' % (key, got[:12])


def _rle_count_break(t: str) -> str:
    # DOCK0 段体首个 RLE 段（头 10B 后 count LE u16）：count+1 破 Σcount。
    m = re.search(r'DOCK0_BYI: \[u8; 39790\] = \[\n(.*?)\n\];', t, re.S)
    toks = [x.strip() for line in m.group(1).split('\n') for x in line.rstrip(',').split(',')]
    c0 = int(toks[10], 16) | (int(toks[11], 16) << 8)
    c1 = (c0 + 1) & 0xFFFF
    toks[10], toks[11] = '0x%02X' % (c1 & 0xFF), '0x%02X' % (c1 >> 8)
    body = '\n'.join('    ' + ', '.join(toks[i:i + 24]) + ',' for i in range(0, len(toks), 24))
    return t[:m.start(1)] + body + t[m.end(1):]


if __name__ == '__main__':
    raise SystemExit(main())
