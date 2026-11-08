#!/usr/bin/env python3
"""生成 berry-gfx 32×48 HD ASCII 点阵（Noto Sans CJK SC / 思源黑体 OFL 源）。

用法：
  python3 tools/genhdfont.py [font.otf]
  python3 tools/genhdfont.py --selftest   # 门禁自检（拒面 + 结构钉板；PIL 在位时重产对拍）

默认读取 `user/berry-gfx/fonts/NotoSansCJKsc-Regular-ASCII.otf`，写出
`user/berry-gfx/src/font_data_hd.rs`。设备仍只消费静态单色点阵；
本脚本仅在宿主改字源/字号时手工运行，生成物随源入册。

自检面（a-3 U4 纪律，genfbfont --check 同族——HD 档此前零守卫）：
- 结构钉板零依赖（re 解析在册产物）：95 形码点序 32..126 / 每形恰 192B /
  头部 CELL_W/CELL_H/FONT_HD 类型钉 / 95×192B 黄金哈希（字源/字号/阈值
  变更 → 生成物必漂移，须有意识重钉本哈希，防静默换脸）。
- 几何锚：SP 全零且唯一空形；数字 0-9 与大写 A-Z 基线 y1=37（唯 Q 尾
  伸 43）；下行部 g/j/p/q/y 尾 y1=45；'!' bbox(13,17,12,37) 且内部空行
  恰 {30,31,32}（stem/点间隙在席）。
- 8 拒面：截字节/码点序破坏/头钉漂移/坏 token/缺形/标签漂移/CELL_W 漂移/
  重复形——parse_artifact 严格拒绝。
- PIL 在位且字源在仓时：build() 重产 ↔ 在册文件逐字节对拍（真漂移守卫）；
  PIL 缺席打 skip 不 FAIL（宿主 python3 无 PIL 是合法环境——miniforge
  默认无、/usr/bin 与 homebrew 有；降级面由黄金哈希兜底）。
"""
from __future__ import annotations

from pathlib import Path
import re
import sys

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:  # selftest 面三降级；build() 路径仍硬依赖 PIL
    Image = ImageDraw = ImageFont = None

CELL_W = 32
CELL_H = 48
FONT_SIZE = 34
BASELINE = 38
THRESHOLD = 128
REPO = Path(__file__).resolve().parent.parent


def glyph_rows(font: ImageFont.FreeTypeFont, ch: int) -> list[int]:
    img = Image.new("L", (CELL_W, CELL_H), 0)
    draw = ImageDraw.Draw(img)
    advance = round(font.getlength(chr(ch)))
    x = max(0, (CELL_W - advance) // 2)
    draw.text((x, BASELINE), chr(ch), font=font, fill=255, anchor="ls")
    rows: list[int] = []
    for y in range(CELL_H):
        row = 0
        for xpos in range(CELL_W):
            row <<= 1
            if img.getpixel((xpos, y)) >= THRESHOLD:
                row |= 1
        rows.append(row)
    return rows


def build(font_path: Path) -> str:
    if ImageFont is None:
        raise SystemExit("genhdfont: PIL 缺席（生成路径硬依赖）——brew/系统 python3 或 pip install pillow")
    font = ImageFont.truetype(str(font_path), FONT_SIZE)
    lines = [
        "//! 本文件由 tools/genhdfont.py 生成——勿手改；改字源/字号后重跑生成器。",
        "//! 字源：Noto Sans CJK SC Regular（ASCII 子集）（SIL Open Font License 1.1，见 fonts/OFL.txt）。",
        "//! 32×48 HD 档：ASCII 32..126，原生矢量光栅化（非 8×16 最近邻放大）。",
        "//! 行序自上而下；每行 4 字节，MSB=最左列。",
        "",
        "pub const CELL_W: usize = 32;",
        "pub const CELL_H: usize = 48;",
        "",
        "pub static FONT_HD: [[u8; 192]; 95] = [",
    ]
    for ch in range(32, 127):
        vals = glyph_rows(font, ch)
        assert len(vals) == CELL_H
        # 32 位/行 → 恰 4 字节，大端位序与 MSB=最左列一致。
        body = ", ".join(
            f"0x{((v >> 24) & 0xff):02X}, 0x{((v >> 16) & 0xff):02X}, "
            f"0x{((v >> 8) & 0xff):02X}, 0x{v & 0xff:02X}" for v in vals
        )
        label = "SP" if ch == 32 else chr(ch)
        lines.append(f"    /* {ch:3} {label:>2} */ [{body}],")
    lines.append("];")
    return "\n".join(lines) + "\n"


# ---- selftest（a-3 U4 门禁化纪律；elf2flat/pwrcls/siminj --selftest 同位） ----

# 黄金哈希：95 形 × 192B 串接的 sha256。字源/字号/阈值/列序任何变更必漂移
# ——此时跑 `python3 tools/genhdfont.py` 重产后 **有意识** 更新本值（连坐
# 核对 berry-gfx FontAtlas32x48 消费面几何），防生成物静默换脸。
GOLDEN_SHA256 = "57ec8c95b3cbcde25643d6adcb4fdaab8330c0261e9b1918511de90aca1e6b18"

_ENTRY = re.compile(r"/\*\s*(\d+)\s+(\S+)\s*\*/\s*\[([^\]]*)\]")


def parse_artifact(text: str) -> list[tuple[int, str, list[int]]]:
    """在册产物严格解析：头部钉 + 95 形序 32..126 + 每形恰 192 字节 token。

    返回 [(cp, label, bytes192), ...]；任何结构破坏 raise ValueError。
    """
    for pin in (
        "pub const CELL_W: usize = 32;",
        "pub const CELL_H: usize = 48;",
        "pub static FONT_HD: [[u8; 192]; 95] = [",
    ):
        if pin not in text:
            raise ValueError(f"头钉漂移: {pin!r} 不在席")
    entries = _ENTRY.findall(text)
    if len(entries) != 95:
        raise ValueError(f"形数 {len(entries)} ≠ 95")
    out: list[tuple[int, str, list[int]]] = []
    seen: set[int] = set()
    for i, (cp_s, label, body) in enumerate(entries):
        cp = int(cp_s)
        if cp != 32 + i:
            raise ValueError(f"码点序破坏: 第 {i} 位是 {cp}，期望 {32 + i}")
        if cp in seen:
            raise ValueError(f"重复形: {cp}")
        seen.add(cp)
        expect_label = "SP" if cp == 32 else chr(cp)
        if label != expect_label:
            raise ValueError(f"标签漂移: {cp} 标 {label!r}，期望 {expect_label!r}")
        toks = [t for t in body.split(",") if t.strip()]
        if len(toks) != 192:
            raise ValueError(f"形 {cp} 字节数 {len(toks)} ≠ 192")
        try:
            vals = [int(t.strip(), 16) for t in toks]
        except ValueError as e:
            raise ValueError(f"形 {cp} 坏 token: {e}") from e
        if any(not (t.strip().startswith("0x") or t.strip().startswith("0X")) for t in toks):
            raise ValueError(f"形 {cp} token 非 0x 前缀")
        out.append((cp, label, vals))
    return out


def _rows_u32(vals: list[int]) -> list[int]:
    """192B → 48 行 32 位（MSB=最左列，与生成器行序一致）。"""
    return [
        (vals[i * 4] << 24) | (vals[i * 4 + 1] << 16) | (vals[i * 4 + 2] << 8) | vals[i * 4 + 3]
        for i in range(48)
    ]


def _ink_bbox(vals: list[int]) -> tuple[int, int, int, int] | None:
    xs: list[int] = []
    ys: list[int] = []
    for y, v in enumerate(_rows_u32(vals)):
        for b in range(32):
            if (v >> (31 - b)) & 1:
                xs.append(b)
                ys.append(y)
    return (min(xs), max(xs), min(ys), max(ys)) if xs else None


def selftest() -> int:
    import hashlib

    art_path = REPO / "user/berry-gfx/src/font_data_hd.rs"
    text = art_path.read_text(encoding="utf-8")
    entries = parse_artifact(text)  # 拒面零容忍：结构坏即抛

    # —— 结构钉板 ——
    glyphs = {cp: vals for cp, _label, vals in entries}
    blob = b"".join(bytes(glyphs[cp]) for cp in range(32, 127))
    got = hashlib.sha256(blob).hexdigest()
    assert got == GOLDEN_SHA256, (
        f"黄金哈希漂移 {got}（在册产物与生成器语义分叉，或字源变更未重钉）——"
        "重产: python3 tools/genhdfont.py，有意识核对后更新 GOLDEN_SHA256"
    )

    # —— 几何锚（矢量光栅化形态在席证明，非任意 192B 均可过） ——
    assert all(v == 0 for v in glyphs[32]), "SP 须全零"
    empty = [cp for cp in range(32, 127) if not any(glyphs[cp])]
    assert empty == [32], f"唯一空形须仅 SP，实得 {empty}"
    for cp in [c for c in list(range(48, 58)) + list(range(65, 91)) if c != 81]:
        bb = _ink_bbox(glyphs[cp])
        assert bb is not None and bb[3] == 37, f"{chr(cp)} 基线锚 y1≠37: {bb}"
    assert _ink_bbox(glyphs[81]) == (5, 26, 13, 43), "Q 尾锚漂移（基线 37 + 尾伸 43）"
    for ch in "gjpqy":
        bb = _ink_bbox(glyphs[ord(ch)])
        assert bb is not None and bb[3] == 45, f"下行部 {ch} 尾锚 y1≠45: {bb}"
    excl = _rows_u32(glyphs[33])
    assert _ink_bbox(glyphs[33]) == (13, 17, 12, 37), "'!' bbox 锚漂移"
    ink = [y for y, v in enumerate(excl) if v]
    gaps = {y for y in range(min(ink), max(ink) + 1) if excl[y] == 0}
    assert gaps == {30, 31, 32}, f"'!' stem/点间隙漂移: {sorted(gaps)}"

    # —— 8 拒面：parse_artifact 严格拒绝各种结构破坏 ——
    base_entries = _ENTRY.findall(text)
    body33 = base_entries[1][2]

    def rej(mutate, name):
        bad = mutate(text)
        try:
            parse_artifact(bad)
        except ValueError:
            return
        raise AssertionError(f"拒面失守: {name}")

    rej(lambda t: t.replace(body33 + "],", body33[1:-1] + "],"), "截首字节→191B")
    rej(lambda t: t.replace("/*  33  ! */", "/*  34  ! */", 1), "码点序破坏")
    rej(lambda t: t.replace("pub const CELL_W: usize = 32;", "pub const CELL_W: usize = 16;"), "CELL_W 头钉漂移")
    rej(lambda t: t.replace(body33.split(",")[0].strip() + ",", "0xZZ,", 1), "坏 token")
    rej(lambda t: t.replace("    /*  82  R */ [" + base_entries[50][2] + "],\n", "", 1), "缺形→94 形")
    rej(lambda t: t.replace("/*  65  A */", "/*  65  B */", 1), "标签漂移")
    rej(lambda t: t.replace("pub static FONT_HD: [[u8; 192]; 95]", "pub static FONT_HD: [[u8; 96]; 95]"), "FONT_HD 类型钉漂移")
    rej(lambda t: t.replace("\n];", "    /* 126  ~ */ [" + base_entries[94][2] + "],\n];", 1), "重复形→96 形")

    # —— PIL 重产对拍（在位才跑；缺席打 skip，黄金哈希已兜底） ——
    pil_note = "skip（PIL 缺席——黄金哈希兜底）"
    if ImageFont is not None:
        font_path = REPO / "user/berry-gfx/fonts/NotoSansCJKsc-Regular-ASCII.otf"
        if font_path.exists():
            regen = build(font_path)
            assert regen == text, (
                "重产对拍 FAIL：在册 font_data_hd.rs ≠ 生成器重产——"
                "重产: python3 tools/genhdfont.py 并随源入册"
            )
            pil_note = "repro byte-equal"
        else:
            pil_note = "skip（字源不在仓）"

    print(
        f"genhdfont: selftest ok (95 形结构钉板 + 黄金哈希 + 几何锚 8 面 + 拒面 8；"
        f"PIL 对拍 {pil_note})"
    )
    return 0


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--selftest":
        return selftest()
    arg = Path(sys.argv[1]) if len(sys.argv) > 1 else REPO / "user/berry-gfx/fonts/NotoSansCJKsc-Regular-ASCII.otf"
    out = REPO / "user/berry-gfx/src/font_data_hd.rs"
    out.write_text(build(arg))
    print(f"genhdfont: {out} ({out.stat().st_size}B, Noto Sans CJK SC {FONT_SIZE}px → {CELL_W}×{CELL_H})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
