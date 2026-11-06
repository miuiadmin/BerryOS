#!/usr/bin/env python3
"""生成 OpenHarmony 状态图标与 dock 分段内建抗锯齿面。

产物：`user/bins/berry-launcher/src/status_icons.rs`。

- signal/wifi：官方 SVG 光栅化后的 8-bit alpha 白色形。
- battery：按 OpenHarmony batteryPic 官方几何复刻，8-bit alpha。
- systemui 图标行 opacity=0.7：alpha 直接乘 0.7，客机 straight-alpha blend。
- dock：PhoneSmartDock 668×156 官方源条切成四段，背景 #85FAFAFA 保持
  半透明，app 图标按官方顺序嵌入；四段拼回即完整 dock，且每段独立命中。
"""
import importlib.util
from io import BytesIO
from pathlib import Path

from PIL import Image, ImageDraw

REPO = Path(__file__).resolve().parent.parent
ROOT = REPO / "user/bins/berry-launcher/ohstatus"
APPS = REPO / "user/bins/appstub/apps"
OUT = REPO / "user/bins/berry-launcher/src/status_icons.rs"

_spec = importlib.util.spec_from_file_location("mkapp", REPO / "tools/mkapp.py")
_mkapp = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mkapp)


def status_px(name: str, w: int, h: int) -> list[int]:
    im = Image.open(ROOT / f"{name.lower()}.png").convert("RGBA").resize(
        (w, h), Image.Resampling.LANCZOS
    )
    out: list[int] = []
    for y in range(h):
        for x in range(w):
            _r, _g, _b, a = im.getpixel((x, y))
            # systemui phone `icon_component_opacity = 0.7`。
            alpha = round(a * 0.7)
            out.append((alpha << 24) | 0xFF_FFFF)
    return out


def quantize_rgba(im: Image.Image) -> Image.Image:
    # RGB 5-bit + alpha 4-bit：抗锯齿层次保留，RLE 段数受控。
    rgb = im.convert("RGBA").point(lambda v: min(255, round(v / 8) * 8))
    a = rgb.getchannel("A").point(lambda v: round(v / 17) * 17)
    rgb.putalpha(a)
    return rgb


def dock_segments() -> list[Image.Image]:
    # PhoneSmartDock 2x 源：668×156、radius44、padding24、icon108、
    # gap62.67→63；目标 3x = 1002×234 由 SmoothArgb 1.5x 双线性采样。
    W, H = 668, 156
    mask = Image.new("L", (W, H), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, W - 1, H - 1], radius=44, fill=255)

    # #85FAFAFA：straight RGBA。圆角 mask 的 AA 边同时缩放 alpha。
    pill = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    p = pill.load()
    for y in range(H):
        for x in range(W):
            cov = mask.getpixel((x, y))
            if cov:
                p[x, y] = (250, 250, 250, round(0x85 * cov / 255))

    order = ["CONTACTS", "PHOTOS", "SETTINGS", "MMS"]
    for app, x0 in zip(order, [24, 195, 366, 537]):
        icon = Image.open(APPS / app / "icons/icon.png").convert("RGBA").resize(
            (108, 108), Image.Resampling.LANCZOS
        )
        pill.alpha_composite(icon, (x0, (H - 108) // 2))

    return [pill.crop((x, 0, x + 167, H)) for x in (0, 167, 334, 501)]


def emit_array(lines: list[str], name: str, img: Image.Image) -> None:
    img = quantize_rgba(img)
    bio = BytesIO()
    img.save(bio, format="PNG")
    blob = _mkapp.png_to_byi(bio.getvalue(), f"{name}.png")
    w, h = img.size
    lines += [
        f"pub const {name}_W: u32 = {w};",
        f"pub const {name}_H: u32 = {h};",
        f"pub const {name}_BYI: [u8; {len(blob)}] = [",
    ]
    for i in range(0, len(blob), 24):
        lines.append("    " + ", ".join(f"0x{b:02X}" for b in blob[i : i + 24]) + ",")
    lines += ["];", ""]


def main() -> int:
    lines = [
        "//! 由 tools/genohstatus.py 生成——勿手改。",
        "//! signal/wifi = OpenHarmony 官方资产；battery/dock = OH 官方几何复刻。",
        "//! 8-bit alpha；systemui 0.7 透明度在像素 alpha 中保留。",
        "",
    ]
    for name, w, h in (("SIGNAL", 65, 45), ("WIFI", 47, 45), ("BATTERY", 64, 45)):
        lines += [
            f"pub const {name}_W: u32 = {w};",
            f"pub const {name}_H: u32 = {h};",
            f"pub const {name}_PX: [u32; {w * h}] = [",
        ]
        px = status_px(name, w, h)
        for i in range(0, len(px), 12):
            lines.append("    " + ", ".join(f"0x{v:08X}" for v in px[i : i + 12]) + ",")
        lines += ["];", ""]
    for i, img in enumerate(dock_segments()):
        emit_array(lines, f"DOCK{i}", img)
    OUT.write_text("\n".join(lines))
    print(f"genohstatus: {OUT} ({OUT.stat().st_size}B)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
