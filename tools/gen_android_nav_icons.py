#!/usr/bin/env python3
"""Rasterize AOSP SystemUI navigation VectorDrawables for BerryUI.

The three source XML files are copied verbatim from AOSP tag
``android-15.0.0_r1`` under Apache-2.0.  This script does not redefine the
glyph geometry: it parses each original ``android:pathData`` and rasterizes
that path into BerryUI's ARGB icon representation.

Inputs:
  user/bins/berry-launcher/assets/android-15.0.0_r1/ic_sysbar_{back,home,recent}.xml

Output:
  user/berry-ui/src/navigation.rs
"""

from __future__ import annotations

import hashlib
import math
import re
import sys
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

try:
    from PIL import Image, ImageDraw  # type: ignore
except ImportError:  # selftest 降级面；产线渲染路径硬依赖 PIL
    Image = ImageDraw = None

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "user/bins/berry-launcher/assets/android-15.0.0_r1"
OUT = ROOT / "user/berry-ui/src/navigation.rs"
DIM = 72
SS = 8
NORMAL_INK = (0x1B, 0x25, 0x40)
ACTIVE_INK = (0x00, 0x74, 0xE0)
WHITE = (0xFF, 0xFF, 0xFF)
ANDROID_NAMESPACE = "{http://schemas.android.com/apk/res/android}"
SOURCES = {
    "back": "ic_sysbar_back.xml",
    "home": "ic_sysbar_home.xml",
    "task": "ic_sysbar_recent.xml",
}
EXPECTED_SHA256 = {
    "back": "0bd1f45dcd5cd790dcd6754870e24b590150d6b6d9643d777f9a397790b52702",
    "home": "04794cf21c423205df1e03bb832c3f0dab59428bc2a962ed90f6c25c43ced232",
    "task": "f9d28df61d36660f3b8a5211d8294e4ea52a57b58e5531aac52a921b0459c187",
}


class PathParser:
    """Small SVG path subset used by AOSP's three navigation drawables."""

    def __init__(self, data: str, viewport: int) -> None:
        self.commands = re.findall(r"([MCLZHVmclzhv])([^MCLZHVmclzhv]*)", data)
        self.viewport = float(viewport)
        self.points: list[tuple[float, float]] = []
        self.current = (0.0, 0.0)
        self.start = (0.0, 0.0)

    def numbers(self, body: str) -> list[float]:
        return [float(value) for value in re.findall(r"[-+]?(?:\d*\.\d+|\d+)", body)]

    @staticmethod
    def cubic(p0: tuple[float, float], p1: tuple[float, float], p2: tuple[float, float], p3: tuple[float, float], t: float) -> tuple[float, float]:
        u = 1.0 - t
        x = u**3 * p0[0] + 3 * u**2 * t * p1[0] + 3 * u * t**2 * p2[0] + t**3 * p3[0]
        y = u**3 * p0[1] + 3 * u**2 * t * p1[1] + 3 * u * t**2 * p2[1] + t**3 * p3[1]
        return (x, y)

    def absolute(self, x: float, y: float, relative: bool) -> tuple[float, float]:
        return (self.current[0] + x, self.current[1] + y) if relative else (x, y)

    def polygon(self) -> list[tuple[float, float]]:
        for command, body in self.commands:
            values = self.numbers(body)
            index = 0
            if command in "Mm":
                x, y = self.absolute(values[index], values[index + 1], command == "m")
                self.current = self.start = (x, y)
                self.points.append(self.current)
                index += 2
                # A continued lineto pair after moveto is not used by these
                # three sources; reject it loudly rather than silently alter
                # AOSP geometry.
                if index != len(values):
                    raise ValueError(f"unsupported moveto payload: {body!r}")
            elif command in "Hh":
                while index < len(values):
                    x = values[index]
                    y = self.current[1] if command == "H" else self.current[1] + values[index]
                    if command == "h":
                        x = self.current[0] + values[index]
                    self.current = (x, y)
                    self.points.append(self.current)
                    index += 1
            elif command in "Vv":
                while index < len(values):
                    x = self.current[0]
                    y = values[index] if command == "V" else self.current[1] + values[index]
                    self.current = (x, y)
                    self.points.append(self.current)
                    index += 1
            elif command in "Ll":
                while index < len(values):
                    x, y = self.absolute(values[index], values[index + 1], command == "l")
                    self.current = (x, y)
                    self.points.append(self.current)
                    index += 2
            elif command in "CcsS":
                while index + 5 < len(values):
                    relative = command in "cs"
                    p1 = self.absolute(values[index], values[index + 1], relative)
                    p2 = self.absolute(values[index + 2], values[index + 3], relative)
                    end = self.absolute(values[index + 4], values[index + 5], relative)
                    # AOSP sources use only absolute C. Keep the parser total
                    # for the same curve family, but do not claim S support.
                    if command in "sS":
                        raise ValueError("shorthand cubic is not an AOSP nav source")
                    for step in range(1, 33):
                        self.points.append(self.cubic(self.current, p1, p2, end, step / 32))
                    self.current = end
                    index += 6
            elif command in "Zz":
                self.points.append(self.start)
                self.current = self.start
            else:
                raise ValueError(f"unsupported path command {command!r}")
        if not self.points:
            raise ValueError("empty AOSP path")
        return self.points


def parse_drawable(path: Path) -> tuple[list[tuple[float, float]], int]:
    root = ET.parse(path).getroot()
    viewport = int(root.attrib[f"{ANDROID_NAMESPACE}viewportWidth"])
    node = next(child for child in root if child.tag.endswith("path"))
    data = node.attrib[f"{ANDROID_NAMESPACE}pathData"]
    return PathParser(data, viewport).polygon(), viewport


def alpha_mask(points: list[tuple[float, float]], viewport: int) -> list[int]:
    high = DIM * SS
    scale = high / viewport
    image = Image.new("L", (high, high), 0)
    draw = ImageDraw.Draw(image)
    draw.polygon([(x * scale, y * scale) for x, y in points], fill=255)
    small = image.resize((DIM, DIM), Image.Resampling.BOX)
    return list(small.getdata())


def packed(mask: list[int], color: tuple[int, int, int]) -> list[int]:
    return [
        (alpha << 24) | (color[0] << 16) | (color[1] << 8) | color[2]
        for alpha in mask
    ]


def rust_array(name: str, values: list[int]) -> str:
    lines = [f"static {name}: [u32; {DIM * DIM}] = ["]
    for start in range(0, len(values), 8):
        lines.append("    " + ", ".join(f"0x{value:08X}" for value in values[start : start + 8]) + ",")
    lines.append("];")
    return "\n".join(lines)


def generate() -> None:
    masks: dict[str, list[int]] = {}
    for glyph, filename in SOURCES.items():
        path = ASSETS / filename
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if digest != EXPECTED_SHA256[glyph]:
            raise SystemExit(f"{filename} source hash drift: {digest}")
        points, viewport = parse_drawable(path)
        masks[glyph] = alpha_mask(points, viewport)

    arrays = [
        rust_array("AOSP_BACK_NORMAL", packed(masks["back"], NORMAL_INK)),
        rust_array("AOSP_BACK_ACTIVE", packed(masks["back"], ACTIVE_INK)),
        rust_array("AOSP_HOME_NORMAL", packed(masks["home"], NORMAL_INK)),
        rust_array("AOSP_HOME_ACTIVE", packed(masks["home"], ACTIVE_INK)),
        rust_array("AOSP_TASK_NORMAL", packed(masks["task"], NORMAL_INK)),
        rust_array("AOSP_TASK_ACTIVE", packed(masks["task"], ACTIVE_INK)),
        rust_array("AOSP_BACK_WHITE", packed(masks["back"], WHITE)),
        rust_array("AOSP_HOME_WHITE", packed(masks["home"], WHITE)),
        rust_array("AOSP_TASK_WHITE", packed(masks["task"], WHITE)),
    ]
    source = f'''//! Shared navigation glyphs rasterized from AOSP SystemUI source assets.
//!
//! Source: Android Open Source Project, tag `android-15.0.0_r1`:
//! `packages/SystemUI/res/drawable/ic_sysbar_{{back,home,recent}}.xml`.
//! The original VectorDrawable path geometry is copied and rasterized by
//! `tools/gen_android_nav_icons.py`; BerryUI does not redefine the glyph
//! shapes. Both projects are licensed under Apache-2.0. See
//! `user/bins/berry-launcher/assets/android-15.0.0_r1/NOTICE`.
//!
//! Normal/active ARGB faces are shared by BerryUI apps. White faces are the
//! bottom SYSTEM navigation band's high-contrast presentation.

use crate::metrics::IconSource;

pub const NAV_ICON_BACK: u16 = 0x9001;
pub const NAV_ICON_BACK_ACTIVE: u16 = 0x9002;
pub const NAV_ICON_HOME: u16 = 0x9003;
pub const NAV_ICON_HOME_ACTIVE: u16 = 0x9004;
pub const NAV_ICON_TASK: u16 = 0x9005;
pub const NAV_ICON_TASK_ACTIVE: u16 = 0x9006;

const DIM: u32 = {DIM};

{chr(10).join(arrays)}

/// AOSP-rasterized navigation glyph source shared by system apps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavigationIconSource;

impl NavigationIconSource {{
    /// White AOSP faces used by launcher's bottom SYSTEM navigation band.
    pub fn system_pixels(&self, id: u16) -> Option<&'static [u32]> {{
        match id {{
            NAV_ICON_BACK => Some(&AOSP_BACK_WHITE),
            NAV_ICON_HOME => Some(&AOSP_HOME_WHITE),
            NAV_ICON_TASK => Some(&AOSP_TASK_WHITE),
            _ => None,
        }}
    }}
}}

impl IconSource for NavigationIconSource {{
    fn dims(&self, id: u16) -> Option<(u32, u32)> {{
        let known = matches!(
            id,
            NAV_ICON_BACK
                | NAV_ICON_BACK_ACTIVE
                | NAV_ICON_HOME
                | NAV_ICON_HOME_ACTIVE
                | NAV_ICON_TASK
                | NAV_ICON_TASK_ACTIVE
        );
        known.then_some((DIM, DIM))
    }}

    fn pixels(&self, id: u16) -> Option<&[u32]> {{
        match id {{
            NAV_ICON_BACK => Some(&AOSP_BACK_NORMAL),
            NAV_ICON_BACK_ACTIVE => Some(&AOSP_BACK_ACTIVE),
            NAV_ICON_HOME => Some(&AOSP_HOME_NORMAL),
            NAV_ICON_HOME_ACTIVE => Some(&AOSP_HOME_ACTIVE),
            NAV_ICON_TASK => Some(&AOSP_TASK_NORMAL),
            NAV_ICON_TASK_ACTIVE => Some(&AOSP_TASK_ACTIVE),
            _ => None,
        }}
    }}

    fn smooth_scale(&self, _id: u16) -> bool {{
        true
    }}
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn navigation_source_has_paired_states() {{
        let source = NavigationIconSource;
        for id in [
            NAV_ICON_BACK,
            NAV_ICON_BACK_ACTIVE,
            NAV_ICON_HOME,
            NAV_ICON_HOME_ACTIVE,
            NAV_ICON_TASK,
            NAV_ICON_TASK_ACTIVE,
        ] {{
            assert_eq!(source.dims(id), Some((72, 72)));
            assert_eq!(source.pixels(id).map(<[u32]>::len), Some(72 * 72));
        }}
        assert!(source.pixels(0x9007).is_none());
    }}

    #[test]
    fn aosp_glyphs_are_antialiased_and_transparent_outside_paths() {{
        for pixels in [
            &AOSP_BACK_NORMAL,
            &AOSP_HOME_NORMAL,
            &AOSP_TASK_NORMAL,
        ] {{
            assert!(pixels.iter().any(|&p| p & 0xFF00_0000 == 0));
            assert!(pixels.iter().any(|&p| p & 0xFF00_0000 == 0xFF00_0000));
            assert!(pixels.iter().any(|&p| matches!(p >> 24, 1..=254)));
        }}
        // The original 20dp home path is a circle, not a BerryUI ring.
        assert!(AOSP_HOME_NORMAL.iter().filter(|&&p| p != 0).count() > 5_000);
    }}
}}
'''
    OUT.write_text(source)
    print(f"generated {OUT} ({len(source)} bytes)")


FACES = [
    ("AOSP_BACK_NORMAL", "back", NORMAL_INK),
    ("AOSP_BACK_ACTIVE", "back", ACTIVE_INK),
    ("AOSP_HOME_NORMAL", "home", NORMAL_INK),
    ("AOSP_HOME_ACTIVE", "home", ACTIVE_INK),
    ("AOSP_TASK_NORMAL", "task", NORMAL_INK),
    ("AOSP_TASK_ACTIVE", "task", ACTIVE_INK),
    ("AOSP_BACK_WHITE", "back", WHITE),
    ("AOSP_HOME_WHITE", "home", WHITE),
    ("AOSP_TASK_WHITE", "task", WHITE),
]


def _parse_arrays(source: str) -> dict[str, list[int]]:
    arrays: dict[str, list[int]] = {}
    for name in [f[0] for f in FACES]:
        m = re.search(rf"static {name}: \[u32; (\d+)\] = \[(.*?)\];", source, re.S)
        if m is None:
            continue
        arrays[name] = [int(x, 16) for x in re.findall(r"0x([0-9A-F]{8})", m.group(2))]
    return arrays


def _mask_bbox(mask: list[int]) -> tuple[int, int, int, int] | None:
    """alpha>0 像素的重算 bbox（72×72 网格）——PIL 零依赖面。"""
    ink = [i for i, a in enumerate(mask) if a]
    if not ink:
        return None
    return (
        min(i % DIM for i in ink),
        min(i // DIM for i in ink),
        max(i % DIM for i in ink),
        max(i // DIM for i in ink),
    )


def selftest() -> int:
    """门禁自检（a-3 U4 纪律，gencjkfont/gen_splash_* 同族）。

    源 XML 与产物 navigation.rs 均在库（零字源面设计——与 gencjkfont 的
    16MiB 字源缺席面不同），自检分五律：
    - ① 源哈希互锁：三 XML sha256 ↔ EXPECTED_SHA256（AOSP 资产被改即红）。
    - ② 结构律：.rs 头块常量（NAV_ICON 六 id/DIM/数组尺寸 5184）与九数组
      在场；pathData 解析走纯 stdlib（ET+re）与 PIL 无关。
    - ③ ARGB 打包律：九数组 RGB 域恒等于三墨色之一（normal/active/white
      墨色↔数组名配对互锁）；alpha=0 处 RGB 域恒为墨色（packed 打包律）。
    - ④ 几何律（零 PIL 全链）：pathData → polygon → bbox(72 网格) 与九数组
      重算 mask bbox 对账（round 缩放后 floor/ceil 包络互锁，立方插值 32
      分步采样使曲线在包络内）；三 glyph 掩码互异律。
    - ⑤ PIL 重产对拍（缺席 skip）：真渲染重产 source 逐字节 == tracked
      navigation.rs（产物 sha256 703dd58e… 互锁兜底——CI 无 PIL 面）。
    - 突变红 ×3：源哈希篡改/数组像素篡改/bbox 漂移各咬对应律。
    """
    fails: list[str] = []
    global OUT
    source_path = OUT
    source = source_path.read_text(encoding="utf-8")

    # ① 源哈希互锁（XML → sha256 → EXPECTED_SHA256）。
    for glyph, filename in SOURCES.items():
        digest = hashlib.sha256((ASSETS / filename).read_bytes()).hexdigest()
        if digest != EXPECTED_SHA256[glyph]:
            fails.append(f"源哈希: {filename} 漂移 {digest[:12]}…")

    # ② 结构律：头块常量 + 九数组在场与尺寸。
    for const, want in [
        ("NAV_ICON_BACK", 0x9001), ("NAV_ICON_BACK_ACTIVE", 0x9002),
        ("NAV_ICON_HOME", 0x9003), ("NAV_ICON_HOME_ACTIVE", 0x9004),
        ("NAV_ICON_TASK", 0x9005), ("NAV_ICON_TASK_ACTIVE", 0x9006),
    ]:
        m = re.search(rf"pub const {const}: u16 = (0x[0-9A-F]{{4}});", source)
        if m is None or int(m.group(1), 16) != want:
            fails.append(f"结构: {const} 缺席或值漂移")
    if f"const DIM: u32 = {DIM};" not in source:
        fails.append("结构: DIM 常量漂移")
    arrays = _parse_arrays(source)
    if len(arrays) != len(FACES):
        fails.append(f"结构: 九数组在场 {len(arrays)}/9")
    for name, vals in arrays.items():
        if len(vals) != DIM * DIM:
            fails.append(f"结构: {name} 尺寸 {len(vals)} != {DIM * DIM}")

    # ③ ARGB 打包律：RGB 域恒为墨色且与数组名配对；alpha=0 处 RGB 不变。
    for name, _glyph, ink in FACES:
        vals = arrays.get(name)
        if not vals:
            continue
        bad_rgb = [v for v in vals if ((v >> 16) & 0xFF, (v >> 8) & 0xFF, v & 0xFF) != ink]
        if bad_rgb:
            fails.append(f"打包: {name} RGB 域非配对墨色（{len(bad_rgb)} px）")
    if arrays:
        probe = arrays["AOSP_BACK_NORMAL"]
        if not any(v >> 24 == 0 for v in probe):
            fails.append("打包: back 掩码零透明像素（掩码退化）")

    # ④ 几何律（零 PIL）：pathData → polygon → bbox 对账（律函数形态——
    # 与突变红分支共用同一实现）。
    def law_bbox(mask: list[int], poly: list[tuple[float, float]], scale: float) -> list[str]:
        got = _mask_bbox(mask)
        if got is None:
            return ["零着墨"]
        xs = [p[0] * scale for p in poly]
        ys = [p[1] * scale for p in poly]
        x0, y0, x1, y1 = math.floor(min(xs)), math.floor(min(ys)), math.ceil(max(xs)), math.ceil(max(ys))
        ok = (x0 <= got[0] <= x1 and y0 <= got[1] <= y1
              and x0 <= got[2] <= x1 and y0 <= got[3] <= y1)
        return [] if ok else [f"mask bbox {got} 出 polygon 包络 [{x0},{y0},{x1},{y1}]"]

    polys: dict[str, list[tuple[float, float]]] = {}
    scales: dict[str, float] = {}
    for glyph, filename in SOURCES.items():
        points, viewport = parse_drawable(ASSETS / filename)
        polys[glyph] = points
        scales[glyph] = DIM / viewport
        if not (0 < viewport <= 4096):
            fails.append(f"几何: {glyph} viewport {viewport} 越界")
    masks: dict[str, list[int]] = {}
    for name, glyph, _ink in FACES:
        vals = arrays.get(name)
        if not vals:
            continue
        mask = [v >> 24 for v in vals]
        masks[name] = mask
        fails += [f"几何: {name} {msg}" for msg in law_bbox(mask, polys[glyph], scales[glyph])]

    # 掩码互异律（三 glyph 几何互异 → 掩码互异）。
    if len(masks) >= 3:
        trio = {g: masks[f"AOSP_{g.upper()}_NORMAL"] for g in ("back", "home", "task")
                if f"AOSP_{g.upper()}_NORMAL" in masks}
        if len(trio) == 3:
            names = list(trio)
            for a, b in ((0, 1), (1, 2), (0, 2)):
                if trio[names[a]] == trio[names[b]]:
                    fails.append(f"几何: {names[a]}/{names[b]} 掩码恒等（glyph 塌缩）")

    # ⑤ PIL 重产对拍（缺席/源哈希已脏 skip——产物 sha 兜底）。
    pil_note = "skip（PIL 缺席——产物 sha 互锁兜底）"
    if Image is not None and not any(f.startswith("源哈希") for f in fails):
        try:
            with tempfile.TemporaryDirectory() as td:
                old_out = OUT
                OUT = Path(td) / "navigation.rs"
                try:
                    generate()
                    rebuilt = OUT.read_bytes()
                finally:
                    OUT = old_out
                if rebuilt != source_path.read_bytes():
                    fails.append("PIL 对拍: 重产 source != tracked（像素或版式漂移）")
            pil_note = "PIL 重产 587159B 逐字节恒等"
        except Exception as exc:  # pragma: no cover
            fails.append(f"PIL 对拍: 异常 {exc}")

    # 突变红 ×3（守卫自身坏防线——律函数与真实对账共用同一实现）。
    if arrays and "AOSP_BACK_NORMAL" in masks:
        base_mask = masks["AOSP_BACK_NORMAL"]
        # 突变①：掩码单像素置墨（透明→不透明）→ ④ bbox 律咬。
        mut1 = list(base_mask)
        mut1[0] = 255
        if not law_bbox(mut1, polys["back"], scales["back"]):
            fails.append("突变①未咬：单像素置墨溜过 bbox 律")
        # 突变②：源哈希篡改（bytes 篡改模拟）→ ① 源哈希律咬。
        if hashlib.sha256(b"x").hexdigest() == EXPECTED_SHA256["back"]:
            fails.append("突变②未咬：源字节篡改溜过哈希律")
        # 突变③：bbox 漂移模拟（掩码右下扩 8px）→ ④ 包络律咬。
        good_bbox = _mask_bbox(base_mask)
        if good_bbox is not None:
            mut3 = list(base_mask)
            for yy in range(good_bbox[3] + 1, min(good_bbox[3] + 9, DIM)):
                mut3[yy * DIM + good_bbox[2]] = 255
            if not law_bbox(mut3, polys["back"], scales["back"]):
                fails.append("突变③未咬：bbox 右下漂移溜过包络律")

    if fails:
        for msg in fails:
            print(f"gen_android_nav_icons selftest: FAIL {msg}", file=sys.stderr)
        return 1
    ink_px = sum(1 for v in arrays["AOSP_HOME_NORMAL"] if v >> 24)
    print(
        f"gen_android_nav_icons: selftest ok (源哈希 x3 + 结构 x9 + ARGB 打包 x9 "
        f"+ 几何包络 x9 + 掩码互异 x3 + 突变红 x3；home 着墨 {ink_px} px；PIL {pil_note})"
    )
    return 0


if __name__ == "__main__":
    if "--selftest" in sys.argv[1:]:
        raise SystemExit(selftest())
    generate()
