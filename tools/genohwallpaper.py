#!/usr/bin/env python3
"""从 OpenHarmony phone 默认壁纸生成 launcher 内建 324×576 RGB565 源图。

产物：`user/bins/berry-launcher/src/wallpaper.rs`。

OpenHarmony `backgroundImageSize(ImageSize.Cover)` 在 1080×1920 竖屏上取
1000×1000 源图中央 562.5×1000 裁切。这里宿主预采样到 324×576，并用
Floyd-Steinberg 抖动压成 RGB565；launcher 经 `ImageSrc::SmoothRgb565`
双线性放大到整屏。静态 rodata 直供，不占 launcher 1MiB 堆。
"""
from pathlib import Path
import hashlib
from PIL import Image

REPO = Path(__file__).resolve().parent.parent
SRC = REPO / "user/bins/berry-launcher/ohwallpaper/img_wallpaper_default.jpg"
OUT = REPO / "user/bins/berry-launcher/src/wallpaper.rs"
W, H = 324, 576


def cover_center() -> Image.Image:
    im = Image.open(SRC).convert("RGB")
    crop_w = round(im.height * W / H)
    if crop_w > im.width:
        crop_h = round(im.width * H / W)
        box = ((im.width - crop_h) // 2, 0, (im.width + crop_h) // 2, im.height)
    else:
        box = ((im.width - crop_w) // 2, 0, (im.width + crop_w) // 2, im.height)
    return im.crop(box).resize((W, H), Image.Resampling.LANCZOS)


def rgb565_dither(im: Image.Image) -> list[int]:
    """Floyd-Steinberg RGB565 量化；返回行主序 u16 值。"""
    rgb = im.load()
    carry = [[0.0] * 3 for _ in range(W)]
    out: list[int] = []
    for y in range(H):
        row_err = [0.0] * 3
        for x in range(W):
            levels = []
            for k in range(3):
                q = 31 if k != 1 else 63
                v = float(rgb[x, y][k]) + carry[x][k] + row_err[k]
                ideal = max(0, min(q, round(v * q / 255.0)))
                levels.append(ideal)
                err = v - ideal * 255.0 / q
                if x + 1 < W:
                    row_err[k] = err * 7 / 16
                if y + 1 < H:
                    carry[x][k] += err * 5 / 16
                    if x > 0:
                        carry[x - 1][k] += err * 3 / 16
                    if x + 1 < W:
                        carry[x + 1][k] += err * 1 / 16
            out.append((levels[0] << 11) | (levels[1] << 5) | levels[2])
    return out


def main() -> int:
    encoded = rgb565_dither(cover_center())
    source_hash = hashlib.sha256(SRC.read_bytes()).hexdigest()
    lines = [
        "//! 由 tools/genohwallpaper.py 生成——勿手改。",
        "//! OpenHarmony phone 默认壁纸中央 cover 裁切，324×576 RGB565。",
        f"//! 源：applications_launcher @ f75dfb6bf727...，SHA-256 {source_hash}。",
        f"pub const WALLPAPER_W: u32 = {W};",
        f"pub const WALLPAPER_H: u32 = {H};",
        f"pub const WALLPAPER_PX: [u16; {W * H}] = [",
    ]
    for i in range(0, len(encoded), 24):
        lines.append("    " + ", ".join(f"0x{v:04X}" for v in encoded[i : i + 24]) + ",")
    lines.append("];")
    OUT.write_text("\n".join(lines) + "\n")
    print(f"genohwallpaper: {OUT} ({OUT.stat().st_size}B)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
