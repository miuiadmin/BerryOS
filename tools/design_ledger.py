#!/usr/bin/env python3
"""DESIGN-D2 token-pair governance gate.

The ledger is intentionally small and declarative:

* every visual pair carries background + foreground (no orphan background);
* active text pairs must pass WCAG >= 4.5:1;
* Disabled state pairs are explicit `inactive` entries, not silently omitted;
* all RGB bytes must physically exist in `user/berry-ui/src/theme.rs`;
* `Color { r: 0x.. }` and decimal `Color { r: .. }` literals may only
  live in theme.rs, except the documented paint/swipe legacy allowlist.

Usage:
  tools/design_ledger.py --check
  tools/design_ledger.py --write
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LEDGER = ROOT / "tools/design_tokens.toml"
THEME = ROOT / "user/berry-ui/src/theme.rs"
DOC = ROOT / "docs/BerryDesign-D2TokenPairContract-v1.0.md"
BEGIN = "<!-- BEGIN generated:design-token-pairs -->\n"
END = "<!-- END generated:design-token-pairs -->\n"

STATE_TOKENS = {"normal", "pressed", "disabled", "selected", "focused"}
BASE_TOKENS = {
    "background",
    "surface",
    "surface_variant",
    "primary",
    "success",
    "warning",
    "danger",
}


class DesignLedgerError(Exception):
    pass


def parse_color(value: object, field: str) -> str:
    if not isinstance(value, str) or not re.fullmatch(r"#[0-9a-fA-F]{6}", value):
        raise DesignLedgerError(f"{field} must be #rrggbb, got {value!r}")
    return value[1:].lower()


def linear(channel: int) -> float:
    value = channel / 255.0
    if value <= 0.03928:
        return value / 12.92
    return ((value + 0.055) / 1.055) ** 2.4


def luminance(rgb: str) -> float:
    channels = [int(rgb[index : index + 2], 16) for index in (0, 2, 4)]
    red, green, blue = (linear(channel) for channel in channels)
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue


def contrast_x100(background: str, foreground: str) -> int:
    first = luminance(foreground) + 0.05
    second = luminance(background) + 0.05
    light, dark = sorted((first, second), reverse=True)
    return round(light / dark * 100)


def enum_variants(source: str, name: str) -> set[str]:
    match = re.search(rf"pub enum {name} \{{(?P<body>.*?)\}}", source, re.S)
    if not match:
        raise DesignLedgerError(f"{name} enum missing from theme.rs")
    return set(re.findall(r"^\s*([A-Za-z][A-Za-z0-9_]*)", match.group("body"), re.M))


def validate() -> list[dict]:
    with LEDGER.open("rb") as stream:
        data = tomllib.load(stream)
    if data.get("format") != 1:
        raise DesignLedgerError("meta.format must be 1")
    if data.get("owner") != "berry-ui":
        raise DesignLedgerError("meta.owner must be berry-ui")
    threshold = data.get("text_contrast_x100")
    if threshold != 450:
        raise DesignLedgerError("text_contrast_x100 must be 450")
    pairs = data.get("pair")
    if not isinstance(pairs, list) or not pairs:
        raise DesignLedgerError("no [[pair]] entries")

    seen: set[tuple[str, str]] = set()
    for index, raw in enumerate(pairs):
        if not isinstance(raw, dict):
            raise DesignLedgerError(f"pair[{index}] is not a table")
        required = {"scheme", "token", "background", "foreground", "level"}
        missing = required - raw.keys()
        if missing:
            raise DesignLedgerError(f"pair[{index}] missing {sorted(missing)}")
        scheme = raw["scheme"]
        token = raw["token"]
        level = raw["level"]
        if scheme not in {"light", "dark", "high_contrast_light"}:
            raise DesignLedgerError(f"unknown scheme {scheme!r}")
        if token not in BASE_TOKENS | STATE_TOKENS:
            raise DesignLedgerError(f"unknown token {token!r}")
        if level not in {"text", "inactive"}:
            raise DesignLedgerError(f"unknown level {level!r}")
        key = (scheme, token)
        if key in seen:
            raise DesignLedgerError(f"duplicate pair {scheme}/{token}")
        seen.add(key)
        background = parse_color(raw["background"], f"{scheme}/{token}.background")
        foreground = parse_color(raw["foreground"], f"{scheme}/{token}.foreground")
        ratio = contrast_x100(background, foreground)
        if level == "text" and ratio < threshold:
            raise DesignLedgerError(
                f"{scheme}/{token} contrast {ratio}:{threshold} required"
            )
        if (token == "disabled") != (level == "inactive"):
            raise DesignLedgerError(
                f"{scheme}/{token}: inactive exemption must map bijectively to disabled"
            )

    source = THEME.read_text(encoding="utf-8")
    source_schemes = enum_variants(source, "ColorScheme")
    if source_schemes != {"Light", "Dark", "HighContrastLight"}:
        raise DesignLedgerError(
            f"ColorScheme enum mismatch: {sorted(source_schemes)}"
        )
    source_states = enum_variants(source, "InteractionState")
    if source_states != {
        "Normal", "Pressed", "Disabled", "Selected", "Focused",
    }:
        raise DesignLedgerError(
            f"InteractionState enum mismatch: {sorted(source_states)}"
        )
    expected_keys = {
        (scheme, token)
        for scheme in ("light", "dark", "high_contrast_light")
        for token in BASE_TOKENS | STATE_TOKENS
    }
    if seen != expected_keys:
        missing = sorted(expected_keys - seen)
        extra = sorted(seen - expected_keys)
        raise DesignLedgerError(f"scheme matrix mismatch missing={missing} extra={extra}")

    enum_match = re.search(
        r"pub enum ColorPairToken \{(?P<body>.*?)\}",
        source,
        re.S,
    )
    if not enum_match:
        raise DesignLedgerError("ColorPairToken enum missing from theme.rs")
    variants = set(re.findall(r"^\s*([A-Za-z][A-Za-z0-9_]*)", enum_match.group("body"), re.M))
    expected_variants = {
        "".join(part.capitalize() for part in token.split("_"))
        for token in BASE_TOKENS
    }
    if variants != expected_variants:
        raise DesignLedgerError(
            f"ColorPairToken enum mismatch source={sorted(variants)} ledger={sorted(expected_variants)}"
        )
    for raw in pairs:
        for field in ("background", "foreground"):
            rgb = parse_color(raw[field], field).upper()
            literal = f"0x{rgb[0:2]}, g: 0x{rgb[2:4]}, b: 0x{rgb[4:6]}"
            if literal not in source:
                raise DesignLedgerError(f"{raw['scheme']}/{raw['token']} {field} {literal} absent from theme.rs")

    legacy_decimal_color_paths = {
        # Image key black is a raster algorithm sentinel, not a theme surface.
        "paint.rs",
        # Swipe dot pixels remain pixel-golden legacy until the desktop
        # indicator migrates to a tokenized widget.
        "swipe.rs",
    }
    for path in (ROOT / "user/berry-ui/src").glob("*.rs"):
        if path == THEME:
            continue
        text = path.read_text(encoding="utf-8")
        # Unit-test fixtures deliberately pin raw pixels; governance covers
        # production color sources only.
        text = text.split("#[cfg(test)]", 1)[0]
        literal = re.search(r"Color\s*\{\s*r:\s*(?:0x[0-9a-fA-F]{2}|\d+)", text)
        if literal and path.name not in legacy_decimal_color_paths:
            raise DesignLedgerError(f"hardcoded Color literal outside theme.rs: {path.name}")
    return pairs


def matrix(pairs: list[dict]) -> str:
    rows = [
        "| scheme | token | background | foreground | contrast | level |",
        "|---|---|---|---|---:|---|",
    ]
    for raw in sorted(pairs, key=lambda item: (item["scheme"], item["token"])):
        background = parse_color(raw["background"], "background")
        foreground = parse_color(raw["foreground"], "foreground")
        rows.append(
            f"| {raw['scheme']} | {raw['token']} | `#{background}` | "
            f"`#{foreground}` | {contrast_x100(background, foreground) / 100:.2f}:1 | {raw['level']} |"
        )
    return "\n".join(rows) + "\n"


def write_doc(pairs: list[dict]) -> None:
    text = DOC.read_text(encoding="utf-8")
    if BEGIN not in text or END not in text:
        raise DesignLedgerError(f"{DOC} lacks generated markers")
    before, rest = text.split(BEGIN, 1)
    _, after = rest.split(END, 1)
    DOC.write_text(before + BEGIN + "\n" + matrix(pairs) + "\n" + END + after, encoding="utf-8")


def check_doc(pairs: list[dict]) -> None:
    text = DOC.read_text(encoding="utf-8")
    if BEGIN not in text or END not in text:
        raise DesignLedgerError(f"{DOC} lacks generated markers")
    _, rest = text.split(BEGIN, 1)
    captured, _ = rest.split(END, 1)
    if captured.strip("\n") + "\n" != matrix(pairs):
        raise DesignLedgerError("generated design-token matrix is stale; run --write")


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    args = parser.parse_args()
    try:
        pairs = validate()
        if args.write:
            write_doc(pairs)
        else:
            check_doc(pairs)
    except (DesignLedgerError, OSError) as error:
        print(f"design ledger: FAIL: {error}", file=sys.stderr)
        return 1
    print(f"design ledger: PASS ({len(pairs)} pairs)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
