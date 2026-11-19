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
import re
import xml.etree.ElementTree as ET
from pathlib import Path

from PIL import Image, ImageDraw

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


if __name__ == "__main__":
    generate()
