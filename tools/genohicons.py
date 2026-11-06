#!/usr/bin/env python3
"""从 OpenHarmony 原始视觉资产生成 appstub 96×96 桌面图标。

产物：`user/bins/appstub/apps/<ID>/icons/icon.png`。
输入：`user/bins/appstub/ohassets/`（raw 源 + glyphs 白色 alpha 形）。

设计口径：
- 外轮廓 = OpenHarmony `ohos_icon_mask.png`（非自绘圆角）。
- glyph = OpenHarmony SVG/PNG 源光栅化后的 alpha 形。
- 渐变色底 = 本仓生成的 OH 风格配色；仅色值自造，资产形状来自 OH。
- BYI1 消费面是 1-bit alpha：生成末段统一 threshold，防宿主预览与客机不一致。
"""
from pathlib import Path
from PIL import Image

REPO = Path(__file__).resolve().parent.parent
ROOT = REPO / "user/bins/appstub/ohassets"
OUT_ROOT = REPO / "user/bins/appstub/apps"
S = 96
SS = 4
R = Image.Resampling.LANCZOS

# app ID -> (glyph stem, 渐变起色, 渐变止色)。色底为本仓生成的 OH 风格配色。
APPS = {
    "SETTINGS": ("settings", (31, 143, 255), (0, 199, 190)),
    "FILES": ("files", (255, 159, 46), (255, 106, 0)),
    "PHOTOS": ("photos", (255, 126, 179), (255, 61, 119)),
    "CAMERA": ("camera", (53, 181, 255), (0, 102, 255)),
    "CONTACTS": ("contacts", (52, 195, 160), (0, 125, 100)),
    "MMS": ("messages", (77, 141, 255), (0, 82, 217)),
    "BROWSER": ("browser", (0, 199, 255), (0, 102, 212)),
}


def gradient_tile(w: int, h: int, c0: tuple[int, int, int], c1: tuple[int, int, int]) -> Image.Image:
    im = Image.new("RGB", (w, h))
    px = im.load()
    den = w + h - 2
    for y in range(h):
        for x in range(w):
            t = (x + y) / den
            px[x, y] = tuple(round(c0[i] * (1 - t) + c1[i] * t) for i in range(3))
    return im


def make_icon(app: str, glyph: str, c0: tuple, c1: tuple) -> None:
    big = S * SS
    mask = Image.open(ROOT / "raw/icon_mask.png").convert("RGBA").getchannel("A")
    mask = mask.resize((big, big), R)

    tile = gradient_tile(big, big, c0, c1).convert("RGBA")
    tile.putalpha(mask)

    g = Image.open(ROOT / "glyphs" / f"{glyph}.png").convert("RGBA")
    bbox = g.getchannel("A").getbbox()
    if bbox is None:
        raise RuntimeError(f"empty glyph: {glyph}")
    g = g.crop(bbox)
    target = int(S * SS * 0.60)
    scale = target / max(g.width, g.height)
    g = g.resize((max(1, round(g.width * scale)), max(1, round(g.height * scale))), R)
    gx = (big - g.width) // 2
    gy = (big - g.height) // 2
    solid = Image.new("RGBA", g.size, (255, 255, 255, 255))
    tile.paste(solid, (gx, gy), g)

    out = tile.resize((S, S), R)
    alpha = out.getchannel("A").point(lambda a: 255 if a >= 128 else 0)
    out.putalpha(alpha)
    dest = OUT_ROOT / app / "icons" / "icon.png"
    dest.parent.mkdir(parents=True, exist_ok=True)
    out.save(dest)
    print(f"genohicons: {dest.relative_to(REPO)} ({dest.stat().st_size}B)")


def main() -> int:
    for app, spec in APPS.items():
        make_icon(app, *spec)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
