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


def selftest() -> int:
    """自检合成腿（releng_pack --selftest 同款风格；零真仓依赖——合成
    tmp 树直调 validate/matrix/check_doc 全链，WCAG 锚用官方公布值钉板）。

    覆盖面：
      1. WCAG 数学锚（白/黑 21:1 官方极值 + WCAG 样例 #767676 vs #ffffff
         恰 4.54:1——text 门槛 450 两侧恰界）；
      2. parse_color 拒梯（非串/非 6 位/非 hex/大写归一）；
      3. enum_variants 提取与缺席 loud；
      4. matrix 生成黄金行形（定序 + 对比度两位小数）；
      5. validate 全链腿：合成最小合规 TOML+theme.rs 过检，单点变异
         （阈值/owner/scheme/token/level 双射/对比度不足/重复键/矩阵缺口）
         逐项拒面文案钉板；
      6. write_doc→check_doc 往返（fresh 落盘过、漂移红）；
      7. matrix 锚：合成 36 对全 3 scheme × 12 token 恰矩阵全覆盖。
    """
    import tempfile

    # ① WCAG 数学锚（官方公布极值与样例——linear 段公式漂移即红）。
    assert contrast_x100("000000", "ffffff") == 2100, "WCAG 白/黑极值 21:1"
    # WCAG 2.x 样例 #767676 vs #ffffff = 4.54:1（AA text 恰过线档）。
    assert contrast_x100("ffffff", "767676") == 454, "WCAG 灰样例 4.54:1"
    # 门槛 450 夹逼两腿（448 拒 / 451 过——阈值严格 < 比较的两侧）。
    assert contrast_x100("ffffff", "777777") == 448
    assert contrast_x100("ffffff", "76767c") == 451
    assert contrast_x100("000000", "000000") == 100, "同色 1:1"
    # 对称律（bg/fg 交换不变）。
    assert contrast_x100("1b2540", "f5f7fa") == contrast_x100("f5f7fa", "1b2540")

    # ② parse_color 拒梯与归一。
    for bad in (0x112233, "#12345", "#1234567", "#12345g", None, ("#111111",)):
        try:
            parse_color(bad, "test")
        except DesignLedgerError:
            pass
        else:
            raise AssertionError(f"parse_color 必须拒 {bad!r}")
    assert parse_color("#F5F7FA", "t") == "f5f7fa", "大写归一小写 hex"

    # ③ enum_variants 提取与缺席 loud。
    src = (
        "pub enum ColorScheme {\n    Light,\n    Dark,\n    // comment\n"
        "    HighContrastLight,\n}\n"
    )
    assert enum_variants(src, "ColorScheme") == {
        "Light", "Dark", "HighContrastLight",
    }
    assert enum_variants("pub enum ColorScheme {\n}\n", "ColorScheme") == set()
    try:
        enum_variants("pub struct X {}", "ColorScheme")
    except DesignLedgerError as error:
        assert "missing" in str(error)
    else:
        raise AssertionError("enum_variants 缺席必须 loud")

    # ④⑦ matrix 黄金行形 + 全矩阵覆盖数（合成 36 对全谱）。
    schemes = ("light", "dark", "high_contrast_light")
    base_tokens = sorted(BASE_TOKENS)
    state_tokens = sorted(STATE_TOKENS)
    assert len(BASE_TOKENS) == 7 and len(STATE_TOKENS) == 5
    text_bg, text_fg = "ffffff", "000000"      # 21:1 过 text 门
    inactive_bg, inactive_fg = "808080", "707070"  # <4.5 仅 inactive 可用
    pairs = []
    for scheme in schemes:
        for token in base_tokens:
            pairs.append({
                "scheme": scheme, "token": token,
                "background": f"#{text_bg}", "foreground": f"#{text_fg}",
                "level": "text",
            })
        for token in state_tokens:
            level = "inactive" if token == "disabled" else "text"
            bg = inactive_bg if token == "disabled" else text_bg
            fg = inactive_fg if token == "disabled" else text_fg
            pairs.append({
                "scheme": scheme, "token": token,
                "background": f"#{bg}", "foreground": f"#{fg}", "level": level,
            })
    assert len(pairs) == 36, "3 scheme × 12 token 恰 36 对全谱"
    rows = matrix(pairs)
    assert rows.splitlines()[0] == (
        "| scheme | token | background | foreground | contrast | level |"
    )
    first_data = rows.splitlines()[2]
    assert first_data == (
        f"| dark | background | `#{text_bg}` | `#{text_fg}` | 21.00:1 | text |"
    ), "定序 dark 先于 high_contrast_light，token 字典序"
    # inactive 行对比度如实低值。
    disabled_row = [r for r in rows.splitlines() if " disabled " in r][0]
    assert ":1 | inactive |" in disabled_row

    # ⑤ validate 全链腿（合成 tmp 树真跑 validate()，monkeypatch 三路径）。
    theme = []
    for scheme_cls, states_cls in (("ColorScheme", "InteractionState"),):
        theme.append(f"pub enum {scheme_cls} {{\n    Light,\n    Dark,\n    HighContrastLight,\n}}\n")
        theme.append(f"pub enum {states_cls} {{\n    Normal,\n    Pressed,\n    Disabled,\n    Selected,\n    Focused,\n}}\n")
    theme.append("pub enum ColorPairToken {\n    Background,\n    Surface,\n    SurfaceVariant,\n    Primary,\n    Success,\n    Warning,\n    Danger,\n}\n")
    # 每个 hex 字面量三处形态入库（validate 搜 `0xRR, g: 0xGG, b: 0xBB`
    # 大写无前缀形——theme.rs 里须逐字在场）。
    for rgb in (text_bg, text_fg, inactive_bg, inactive_fg):
        theme.append(
            f"Color {{ r: 0x{rgb[0:2].upper()}, g: 0x{rgb[2:4].upper()}, b: 0x{rgb[4:6].upper()} }}"
        )
    theme_text = "\n".join(theme)

    def run_validate(ledger_text: str, theme_src: str, rogue: str | None = None):
        import design_ledger as mod

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            toml = tmp_path / "design_tokens.toml"
            toml.write_text(ledger_text, encoding="utf-8")
            root = tmp_path / "root"
            src_dir = root / "user/berry-ui/src"
            src_dir.mkdir(parents=True)
            theme_file = src_dir / "theme.rs"
            theme_file.write_text(theme_src, encoding="utf-8")
            if rogue is not None:
                (src_dir / "widgets.rs").write_text(rogue, encoding="utf-8")
            doc = tmp_path / "doc.md"
            doc.write_text(BEGIN + "\nplaceholder\n" + END, encoding="utf-8")
            real = {
                "LEDGER": mod.LEDGER, "THEME": mod.THEME, "DOC": mod.DOC,
                "ROOT": mod.ROOT,
            }
            mod.LEDGER = toml
            mod.THEME = theme_file
            mod.DOC = doc
            mod.ROOT = root
            try:
                return ("ok", mod.validate())
            except mod.DesignLedgerError as error:
                return ("fail", str(error))
            finally:
                mod.LEDGER = real["LEDGER"]
                mod.THEME = real["THEME"]
                mod.DOC = real["DOC"]
                mod.ROOT = real["ROOT"]

    def make_ledger(mutate=None) -> str:
        lines = [
            "format = 1", 'owner = "berry-ui"',
            "text_contrast_x100 = 450",
        ]
        for pair in pairs:
            pair = dict(pair)
            if mutate:
                mutate(pair)
            lines.append("[[pair]]")
            for key in ("scheme", "token", "background", "foreground", "level"):
                lines.append(f'{key} = "{pair[key]}"')
        return "\n".join(lines) + "\n"

    status, payload = run_validate(make_ledger(), theme_text)
    assert status == "ok" and len(payload) == 36, f"合成合规 36 对须过检: {payload}"

    def expect_fail(mutate, needle, theme=None, rogue=None):
        status, message = run_validate(
            make_ledger(mutate),
            theme_text if theme is None else theme,
            rogue,
        )
        assert status == "fail" and needle in message, (
            f"变异 {needle!r} 未咬: {message}"
        )

    def mutate_duplicate(pair):
        if pair["scheme"] == "dark" and pair["token"] == "background":
            pair["scheme"] = "light"

    expect_fail(mutate_duplicate, "duplicate pair")

    def mutate_level_broken(pair):
        if pair["token"] == "disabled" and pair["scheme"] == "light":
            pair["level"] = "text"
            pair["background"] = "#ffffff"
            pair["foreground"] = "#000000"

    expect_fail(mutate_level_broken, "inactive exemption must map bijectively")

    def mutate_scheme(pair):
        if pair["scheme"] == "dark" and pair["token"] == "background":
            pair["scheme"] = "solarized"

    expect_fail(mutate_scheme, "unknown scheme")

    def mutate_token(pair):
        if pair["token"] == "primary" and pair["scheme"] == "light":
            pair["token"] = "accent"

    expect_fail(mutate_token, "unknown token")

    def mutate_low_contrast(pair):
        if pair["level"] == "text" and pair["token"] == "background":
            pair["background"] = "#ffffff"
            pair["foreground"] = "#777777"  # 448 < 450 恰拒侧

    expect_fail(mutate_low_contrast, "contrast 448:450 required")

    def mutate_missing_cell(pair):
        return None if (
            pair["scheme"] == "dark" and pair["token"] == "warning"
        ) else pair

    # 单删一对 = 矩阵缺口（先在 pair 级拒「no [[pair]]」之外的矩阵不匹配）。
    lines = []
    for pair in pairs:
        if pair["scheme"] == "dark" and pair["token"] == "warning":
            continue
        lines.append("[[pair]]")
        for key in ("scheme", "token", "background", "foreground", "level"):
            lines.append(f'{key} = "{pair[key]}"')
    text = "format = 1\nowner = \"berry-ui\"\ntext_contrast_x100 = 450\n" + "\n".join(lines) + "\n"
    status, message = run_validate(text, theme_text)
    assert status == "fail" and "scheme matrix mismatch" in message, message

    # meta 三拒（format/owner/阈值——逐条精确锚替换）。
    meta_cases = (
        ("format = 1", "format = 2", "meta.format must be 1"),
        ('owner = "berry-ui"', 'owner = "berry-kernel"', "meta.owner must be berry-ui"),
        ("text_contrast_x100 = 450", "text_contrast_x100 = 300",
         "text_contrast_x100 must be 450"),
    )
    for old, new, needle in meta_cases:
        text = make_ledger().replace(old, new, 1)
        status, message = run_validate(text, theme_text)
        assert status == "fail" and needle in message, message

    # theme.rs 枚举缺席 / 字面量缺席 / 非 theme.rs 硬编码色三拒。
    expect_fail(None, "ColorScheme enum missing", theme="pub enum Theme {\n}\n")
    status, message = run_validate(make_ledger(), theme_text.replace(
        "Color { r: 0xFF, g: 0xFF, b: 0xFF }", "", 1))
    assert status == "fail" and "absent from theme.rs" in message, message
    # rogue 文件：非 theme.rs 生产面硬编码 Color 字面量（cfg(test) 段外；
    # paint.rs/swipe.rs 在 legacy 豁免表故选 widgets.rs）。
    expect_fail(
        None, "hardcoded Color literal outside theme.rs: widgets.rs",
        rogue="pub fn f() -> Color { Color { r: 0x11, g: 0x22, b: 0x33 } }",
    )
    # rogue 文件 cfg(test) 段内的钉板 fixture 豁免腿（治理只罩生产面）。
    status, message = run_validate(
        make_ledger(), theme_text,
        rogue="#[cfg(test)]\nmod t {\n    fn g() -> Color { Color { r: 1, g: 2, b: 3 } }\n}\n",
    )
    assert status == "ok", f"cfg(test) 段 fixture 须豁免: {message}"

    # ⑥ write_doc→check_doc 往返。
    with tempfile.TemporaryDirectory() as tmp:
        doc = Path(tmp) / "doc.md"
        doc.write_text("head\n" + BEGIN + "\nstale\n" + END + "\ntail\n", encoding="utf-8")
        real_doc = globals()["DOC"]
        globals()["DOC"] = doc
        try:
            write_doc(pairs)
            check_doc(pairs)  # fresh 必过
            drifted = [dict(p) for p in pairs]
            drifted[0]["background"] = "#000001"
            try:
                check_doc(drifted)
            except DesignLedgerError as error:
                assert "stale" in str(error)
            else:
                raise AssertionError("check_doc 漂移必须红")
            text = doc.read_text(encoding="utf-8")
            assert text.startswith("head\n") and text.endswith("tail\n")
            assert BEGIN in text and END in text
        finally:
            globals()["DOC"] = real_doc

    print(
        "design_ledger selftest: 7 腿全绿"
        "（WCAG 锚/parse_color/enum 提取/matrix 黄金形/validate 全链拒面/doc 往返/36 对全谱）"
    )
    return 0


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
    mode.add_argument("--selftest", action="store_true",
                      help="合成自检腿（零真仓依赖）")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
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
