#!/usr/bin/env python3
"""从 OpenHarmony phone 默认壁纸生成 launcher 内建 135×240 静态源图。

产物：`user/bins/berry-launcher/src/wallpaper.rs`。

用法：
  python3 tools/genohwallpaper.py             # 重产 wallpaper.rs（硬依赖 PIL）
  python3 tools/genohwallpaper.py --selftest  # 门禁自检（零 PIL 可跑；PIL 在位时加做重产对拍）

OpenHarmony `backgroundImageSize(ImageSize.Cover)` 在 1080×1920 竖屏上取
1000×1000 源图中央 562.5×1000 裁切。这里宿主预采样到 135×240，launcher
经 `ImageSrc::SmoothArgb` 双线性放大到整屏；静态 rodata 直供，不占 1MiB 堆。

自检面（a-3 U4 纪律，genhdfont --selftest 同族——launcher 壁纸源此前零守卫）：
- 结构钉板零依赖（re 解析在册产物）：W/H/PX 三头钉 + 240 行×135 token +
  token 全 `0x%08X` 形 + alpha 全 0xFF（不透明钉）。
- 溯源钉：头部 SHA-256 ↔ 源 jpg 实测逐字节对账（换源不重产即炸）。
- 32400 px 黄金哈希（源图/裁切/重采样任何漂移必炸——须有意识重钉）。
- 角锚 4（四角像素）+ 唯一色数 19362（内容活性——退化纯色面不可能过）。
- 8 拒面：头钉/类型钉/行数两向/token 数/坏 token/短 token/alpha 位/溯源哈希。
- PIL 在位且源图在仓：build() 重产 ↔ 在册文件逐字节对拍（真漂移守卫）；
  PIL 缺席打 skip 不 FAIL（宿主无 PIL 是合法环境，降级面由黄金哈希兜底）。
"""
from __future__ import annotations

from pathlib import Path
import hashlib
import re
import sys

try:
    from PIL import Image
except ImportError:  # selftest 结构面零依赖可跑；build() 路径硬依赖 PIL
    Image = None

REPO = Path(__file__).resolve().parent.parent
SRC = REPO / 'user/bins/berry-launcher/ohwallpaper/img_wallpaper_default.jpg'
OUT = REPO / 'user/bins/berry-launcher/src/wallpaper.rs'
W, H = 135, 240


def cover_crop_resize(im):
    assert Image is not None  # 仅 build() 内调用，彼时 PIL 必在位
    # Cover 裁切：目标宽高比 1080/1920；方图保留中央竖条。
    crop_w = round(im.height * W / H)
    if crop_w > im.width:
        crop_h = round(im.width * H / W)
        box = ((im.width - crop_h) // 2, 0, (im.width + crop_h) // 2, im.height)
    else:
        box = ((im.width - crop_w) // 2, 0, (im.width + crop_w) // 2, im.height)
    return im.crop(box).resize((W, H), Image.Resampling.LANCZOS)  # type: ignore[union-attr]


def build(src: Path) -> str:
    if Image is None:
        raise SystemExit('genohwallpaper: PIL 缺席（生成路径硬依赖）——brew/系统 python3 或 pip install pillow')
    im = cover_crop_resize(Image.open(src).convert('RGB'))
    source_hash = hashlib.sha256(src.read_bytes()).hexdigest()
    lines = [
        '//! 由 tools/genohwallpaper.py 生成——勿手改。',
        '//! OpenHarmony phone 默认壁纸中央 cover 裁切，135×240 静态源图。',
        f'//! 源：applications_launcher @ f75dfb6bf727...，SHA-256 {source_hash}。',
        f'pub const WALLPAPER_W: u32 = {W};',
        f'pub const WALLPAPER_H: u32 = {H};',
        f'pub const WALLPAPER_PX: [u32; {W * H}] = [',
    ]
    for y in range(H):
        row = []
        for x in range(W):
            r, g, b = im.getpixel((x, y)); row.append(f'0xFF{r:02X}{g:02X}{b:02X}')
        lines.append('    ' + ', '.join(row) + ',')
    lines.append('];')
    return '\n'.join(lines) + '\n'


# ---- selftest（a-3 U4 门禁化纪律；genhdfont/elf2flat/pwrcls/siminj --selftest 同位） ----

# 黄金哈希：32400 px 大端 u32 串接的 sha256。源图/裁切/重采样任何变更必漂移
# ——此时跑 `python3 tools/genohwallpaper.py` 重产后 **有意识** 更新本值
# （连坐核对 berry-launcher wallpaper.rs 消费面与 ROOTFS/rootfs 帽），防静默换脸。
GOLDEN_SHA256 = '8e99e913203a2b9c18a88ce6ec014043ebd2f0cee33569de874baf2868d010ae'

# 角锚 + 内容活性钉（与黄金哈希局部冗余：失败时更快定位）。
CORNER_PX = {0: 0xFF02997B, 134: 0xFF01EAFB, 32265: 0xFF027BCE, 32399: 0xFF0077D5}
UNIQUE_COLORS = 19362

_PINS = (
    'pub const WALLPAPER_W: u32 = 135;',
    'pub const WALLPAPER_H: u32 = 240;',
    'pub const WALLPAPER_PX: [u32; 32400] = [',
)
_TOKEN = re.compile(r'^0x[0-9A-F]{8}$')
_SRC_HASH = re.compile(r'SHA-256 ([0-9a-f]{64})')


def parse_artifact(text: str) -> tuple[str, list[int]]:
    """在册产物严格解析：三头钉 + 溯源哈希行 + 240 行×135 token + alpha 全 FF。

    返回 (头部源哈希, [px...])；任何结构破坏 raise ValueError。
    """
    for pin in _PINS:
        if pin not in text:
            raise ValueError(f'头钉漂移: {pin!r} 不在席')
    m = _SRC_HASH.search(text)
    if m is None:
        raise ValueError('溯源哈希行缺席（SHA-256 注释头）')
    body_m = re.search(r'WALLPAPER_PX: \[u32; 32400\] = \[\n(.*)\n\];', text, re.S)
    if body_m is None:
        raise ValueError('PX 数组体缺席')
    rows = body_m.group(1).split('\n')
    if len(rows) != H:
        raise ValueError(f'行数 {len(rows)} ≠ {H}')
    vals: list[int] = []
    for y, line in enumerate(rows):
        if not (line.startswith('    ') and line.endswith(',')):
            raise ValueError(f'行 {y} 缩进/尾逗形态破坏')
        toks = [t.strip() for t in line[:-1].split(',')]
        if len(toks) != W:
            raise ValueError(f'行 {y} token 数 {len(toks)} ≠ {W}')
        for t in toks:
            if not _TOKEN.match(t):
                raise ValueError(f'行 {y} 坏 token 形: {t!r}')
        vals.extend(int(t, 16) for t in toks)
    if len(vals) != W * H:
        raise ValueError(f'像素数 {len(vals)} ≠ {W * H}')
    if any(v >> 24 != 0xFF for v in vals):
        raise ValueError('alpha 钉破坏：存在非 0xFF 不透明像素')
    return m.group(1), vals


def selftest() -> int:
    art_path = REPO / 'user/bins/berry-launcher/src/wallpaper.rs'
    text = art_path.read_text(encoding='utf-8')
    src_hash, vals = parse_artifact(text)  # 拒面零容忍：结构坏即抛

    # —— 溯源对账：头部哈希 ↔ 源 jpg 实测（换源不重产即炸） ——
    live = hashlib.sha256(SRC.read_bytes()).hexdigest()
    assert src_hash == live, (
        f'溯源漂移：产物头 {src_hash[:12]}… ≠ 源 jpg {live[:12]}…——'
        '换源后须 python3 tools/genohwallpaper.py 重产随源入册'
    )

    # —— 黄金哈希 + 角锚 + 内容活性 ——
    import struct

    blob = b''.join(struct.pack('>I', v) for v in vals)
    got = hashlib.sha256(blob).hexdigest()
    assert got == GOLDEN_SHA256, (
        f'黄金哈希漂移 {got}（源图/裁切/重采样语义分叉）——'
        '重产: python3 tools/genohwallpaper.py，有意识核对后更新 GOLDEN_SHA256'
    )
    for idx, want in CORNER_PX.items():
        assert vals[idx] == want, f'角锚漂移 px[{idx}]={vals[idx]:08X}，期望 {want:08X}'
    uniq = len(set(vals))
    assert uniq == UNIQUE_COLORS, f'唯一色数 {uniq} ≠ {UNIQUE_COLORS}（内容退化/换脸）'

    # —— 8 拒面：parse_artifact 严格拒绝各种结构破坏 ——
    body_m = re.search(r'WALLPAPER_PX: \[u32; 32400\] = \[\n(.*)\n\];', text, re.S)
    assert body_m is not None  # parse_artifact 已过，此提取必成功
    body = body_m.group(1)
    first_tok = body.split('\n')[0][4:].split(',')[0].strip()

    def rej(mutate, name):
        try:
            parse_artifact(mutate(text))
        except ValueError:
            return
        raise AssertionError(f'拒面失守: {name}')

    rej(lambda t: t.replace('pub const WALLPAPER_W: u32 = 135;', 'pub const WALLPAPER_W: u32 = 136;'), 'W 头钉漂移')
    rej(lambda t: t.replace('pub const WALLPAPER_PX: [u32; 32400]', 'pub const WALLPAPER_PX: [u32; 32000]'), 'PX 类型钉漂移')
    rej(lambda t: t.replace(body.split('\n')[0] + '\n', '', 1), '删首行→239 行')
    rej(lambda t: t.replace(body.split('\n')[0] + '\n', body.split('\n')[0] + '\n' + body.split('\n')[0] + '\n', 1), '重复行→241 行')
    rej(lambda t: t.replace(first_tok + ',', first_tok[2:] + ',', 1), '短 token（剥 0x）')
    rej(lambda t: t.replace(first_tok + ',', '0xG2997B,', 1), '坏 hex token')
    rej(lambda t: t.replace(first_tok + ',', '0x0002997B,', 1), 'alpha 位破坏')
    rej(lambda t: t.replace(f'SHA-256 {src_hash}', f'SHA-256 {"A" * 64}'), '溯源哈希行形态破坏（大写不匹配 [0-9a-f]）')

    # —— PIL 重产对拍（在位才跑；缺席打 skip，黄金哈希已兜底） ——
    pil_note = 'skip（PIL 缺席——黄金哈希兜底）'
    if Image is not None:
        if SRC.exists():
            regen = build(SRC)
            assert regen == text, (
                '重产对拍 FAIL：在册 wallpaper.rs ≠ 生成器重产——'
                '重产: python3 tools/genohwallpaper.py 并随源入册'
            )
            pil_note = 'repro byte-equal'
        else:
            pil_note = 'skip（源图不在仓）'

    print(
        f'genohwallpaper: selftest ok (135×240 结构钉板 + 溯源对账 + 黄金哈希 + '
        f'角锚 4 + 唯一色 {UNIQUE_COLORS} + 拒面 8；PIL 对拍 {pil_note})'
    )
    return 0


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == '--selftest':
        return selftest()
    OUT.write_text(build(SRC))
    print(f'genohwallpaper: {OUT} ({OUT.stat().st_size}B)')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
