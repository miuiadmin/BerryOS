#!/usr/bin/env python3
"""生成 berry-gfx 32×48 HD ASCII 点阵（Noto Sans CJK SC / 思源黑体 OFL 源）。

用法：
  python3 tools/genhdfont.py [font.otf]
  python3 tools/genhdfont.py --selftest   # 门禁自检（拒面 + 结构钉板；PIL 在位时重产对拍）

默认读取 `user/berry-gfx/fonts/NotoSansCJKsc-Regular-ASCII.otf`，写出
`user/berry-gfx/src/font_data_hd.rs`。设备仍只消费静态单色点阵；
本脚本仅在宿主改字源/字号时手工运行，生成物随源入册。

产物格式（FORMAT_VERSION 版本位钉）：
- v1 = 仅 FONT_HD 位图段（95 形 × 192B）；
- v2 = 追加 ADVANCE_HD 比例推进段（95B，round(getlength) 真实 advance，
  段序 = 码点 32..126 与 FONT_HD 同序）。**只追加不改 v1 段**——旧消费
  面按 v1 解析零破坏；位图字节零漂移（黄金哈希不变），advance 表另钉
  独立黄金哈希。痛点⑥根修（research/桌面UX审计/03 号 §1.3-1）。

自检面（a-3 U4 纪律，genfbfont --check 同族——HD 档此前零守卫）：
- 结构钉板零依赖（re 解析在册产物）：95 形码点序 32..126 / 每形恰 192B /
  头部 CELL_W/CELL_H/FONT_HD 类型钉 / 95×192B 黄金哈希（字源/字号/阈值
  变更 → 生成物必漂移，须有意识重钉本哈希，防静默换脸）。
- ADVANCE_HD 面钉板：FORMAT_VERSION 头钉 + 95B 值域 1..=CELL_W +
  独立黄金哈希 + 已知字形锚（'i'=9/'W'=30/'m'=31——03 号 §1.1 表与
  宿主 PIL 实测双源钉）+ 'Browser Settings' 累计 269px 锚（03 号实测
  重现实证：等宽排布 512px，比例 269px）。
- 几何锚：SP 全零且唯一空形；数字 0-9 与大写 A-Z 基线 y1=37（唯 Q 尾
  伸 43）；下行部 g/j/p/q/y 尾 y1=45；'!' bbox(13,17,12,37) 且内部空行
  恰 {30,31,32}（stem/点间隙在席）。
- 贴边截断守卫（PIL 在位时）：FONT_SIZE=34 对 CELL_W=32 存在宽字形
  （W=30/m=31）贴格边截断风险（03 号 §1.2-2 推断项）——全 95 形以
  2×CELL_W 宽画布重渲染对比 ink 宽，窄格截断即咬。
- 12 拒面：截字节/码点序破坏/头钉漂移/坏 token/缺形/标签漂移/CELL_W
  漂移/重复形（位图段 8 面）+ advance 段缺段/截短/坏 token/越域值
  （advance 段 4 面）——parse_artifact/parse_advance 严格拒绝。
- PIL 在位且字源在仓时：build() 重产 ↔ 在册文件逐字节对拍（真漂移
  守卫，含 advance 段）；PIL 缺席打 skip 不 FAIL（宿主 python3 无 PIL
  是合法环境——miniforge 默认无、/usr/bin 与 homebrew 有；降级面由
  黄金哈希兜底）。
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
    # 比例推进表：与 glyph_rows 内部同式（round(getlength)）——位图段的
    # 居中偏移消费同一 advance，本表把值另行烘焙（v1 曾算出即丢，03 号
    # §1.2-2 根因环②）。
    advances = [round(font.getlength(chr(ch))) for ch in range(32, 127)]
    for ch, adv in zip(range(32, 127), advances):
        assert 1 <= adv <= CELL_W, f"{ch!r} advance {adv} 越域 [1, {CELL_W}]——字号对字格溢出"
    lines = [
        "//! 本文件由 tools/genhdfont.py 生成——勿手改；改字源/字号后重跑生成器。",
        "//! 字源：Noto Sans CJK SC Regular（ASCII 子集）（SIL Open Font License 1.1，见 fonts/OFL.txt）。",
        "//! 32×48 HD 档：ASCII 32..126，原生矢量光栅化（非 8×16 最近邻放大）",
        "//! 行序自上而下；每行 4 字节，MSB=最左列。",
        "//! 产物格式 v2：v1 = 仅 FONT_HD 位图段；v2 = 追加 ADVANCE_HD 比例",
        "//! 推进段（段序 = 码点 32..126 与 FONT_HD 同序；v1 段字节零漂移）。",
        "",
        "pub const CELL_W: usize = 32;",
        "pub const CELL_H: usize = 48;",
        "",
        "/// 产物格式版本位：1 = 仅位图段；2 = 追加 [`ADVANCE_HD`] advance 表。",
        "#[allow(dead_code)] // 格式自描述——Rust 消费面暂无，selftest 头钉执法",
        "pub const FORMAT_VERSION: u16 = 2;",
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
    # —— v2 追加段：比例推进表（行内多值、无 per 形注释——与 FONT_HD 的
    # per 形 `/* cp label */ [...]` 格式刻意区分，旧位图解析零干扰）。
    lines.append("")
    lines.append("/// 比例推进表（round(getlength) 真实 advance，值域 1..=CELL_W）：")
    lines.append("/// Noto Sans CJK 是比例字体——等宽占格曾致「i」9px 塞 32px 格（03 号 §1.1）。")
    lines.append("/// 消费 = berry-gfx FontAtlas32x48::advance（图集面覆写）；位图仍居中")
    lines.append("/// 在 32px 格内不动，只改排布推进量。")
    lines.append("pub static ADVANCE_HD: [u8; 95] = [")
    for i in range(0, 95, 16):
        row = ", ".join(f"0x{a:02X}" for a in advances[i : i + 16])
        tail = "," if i + 16 < 95 else ""
        lines.append(f"    {row}{tail}")
    lines.append("];")
    return "\n".join(lines) + "\n"


# ---- selftest（a-3 U4 门禁化纪律；elf2flat/pwrcls/siminj --selftest 同位） ----

# 黄金哈希：95 形 × 192B 串接的 sha256。字源/字号/阈值/列序任何变更必漂移
# ——此时跑 `python3 tools/genhdfont.py` 重产后 **有意识** 更新本值（连坐
# 核对 berry-gfx FontAtlas32x48 消费面几何），防生成物静默换脸。
# v2 位图段零漂移（advance 損烘焙不动位图）：本哈希不变——如实记录。
GOLDEN_SHA256 = "57ec8c95b3cbcde25643d6adcb4fdaab8330c0261e9b1918511de90aca1e6b18"

# advance 段独立黄金哈希（95B 串接 sha256）：字源/字号变更必漂移，同上
# 纪律有意识重钉。v2 首轮生成实测钉入（2026-11-13；位图段哈希零漂移）。
GOLDEN_ADVANCE_SHA256 = "34cdbfb895f379feb9d6d95ba713e72c0eb9ac88af0b57b0f6bcb2a0ccdc2b2b"

_ENTRY = re.compile(r"/\*\s*(\d+)\s+(\S+)\s*\*/\s*\[([^\]]*)\]")
_ADV_ENTRY = re.compile(r"pub static ADVANCE_HD: \[u8; 95\] = \[(.*?)\];", re.S)


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


def parse_advance(text: str) -> list[int]:
    """在册产物 advance 段严格解析（v2 起在席）：恰 95 token + 0x 前缀 +
    值域 1..=CELL_W。任何结构破坏 raise ValueError。

    段格式刻意与 FONT_HD 的 per 形注释格式区分（行内多值、无 per 形
    `/* cp label */` 注释）——_ENTRY 位图解析零干扰。
    """
    m = _ADV_ENTRY.search(text)
    if m is None:
        raise ValueError("ADVANCE_HD 段缺席（产物格式 < v2，或段头漂移）")
    toks = [t for t in m.group(1).split(",") if t.strip()]
    if len(toks) != 95:
        raise ValueError(f"ADVANCE_HD 形数 {len(toks)} ≠ 95")
    vals: list[int] = []
    for t in toks:
        if not (t.strip().startswith("0x") or t.strip().startswith("0X")):
            raise ValueError(f"ADVANCE_HD token 非 0x 前缀: {t!r}")
        try:
            vals.append(int(t.strip(), 16))
        except ValueError as e:
            raise ValueError(f"ADVANCE_HD 坏 token: {t!r}") from e
    for v in vals:
        if not 1 <= v <= CELL_W:
            raise ValueError(f"ADVANCE_HD 值越域 [1, {CELL_W}]: {v}")
    return vals


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

    # —— ADVANCE_HD 面钉板（v2 段）——
    assert "pub const FORMAT_VERSION: u16 = 2;" in text, "FORMAT_VERSION 版本位缺席/漂移"
    adv = parse_advance(text)  # 拒面零容忍：段坏即抛
    adv_blob = bytes(adv)
    adv_got = hashlib.sha256(adv_blob).hexdigest()
    assert adv_got == GOLDEN_ADVANCE_SHA256, (
        f"advance 黄金哈希漂移 {adv_got}（字源/字号变更未重钉）——"
        "重产: python3 tools/genhdfont.py，有意识核对后更新 GOLDEN_ADVANCE_SHA256"
    )
    # 已知字形锚：03 号 §1.1 表（宿主 PIL 实测）与本轮重跑双源钉。
    for ch, want in [("i", 9), ("I", 10), ("r", 13), ("s", 16), ("e", 19),
                     ("B", 22), ("W", 30), ("m", 31), (" ", 8)]:
        got_v = adv[ord(ch) - 32]
        assert got_v == want, f"'{ch}' advance {got_v} ≠ {want}（03 号 §1.1 表锚）"
    # 'Browser Settings' 累计锚：等宽 16×32=512px vs 比例 269px（03 号实测
    # 重现实证——排布面对拍的事实源）。
    bs = sum(adv[ord(c) - 32] for c in "Browser Settings")
    assert bs == 269, f"'Browser Settings' 比例累计 {bs} ≠ 269"

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

    # —— advance 段 4 拒面：parse_advance 严格拒绝（mutate 限定段内文本，
    # 不与位图段 token 串味）——
    m_adv = _ADV_ENTRY.search(text)
    assert m_adv is not None
    head, adv_body, tail = text[: m_adv.start(1)], m_adv.group(1), text[m_adv.end(1):]

    def rej_adv(mutate_body, name):
        try:
            parse_advance(head + mutate_body(adv_body) + tail)
        except ValueError:
            return
        raise AssertionError(f"advance 拒面失守: {name}")

    rej_adv(lambda b: b.replace(f"0x{adv[94]:02X}\n", "\n", 1), "advance 截短→94 值")
    rej_adv(lambda b: b.replace(f"0x{adv[0]:02X}", "0xZZ", 1), "advance 坏 token")
    rej_adv(lambda b: b.replace(f"0x{adv[0]:02X}", "0x21", 1), "advance 越域值 33>CELL_W")
    rej_adv(lambda b: b.replace(f"0x{adv[0]:02X}", "0x00", 1), "advance 越域值 0")
    try:
        parse_advance(text.replace("pub static ADVANCE_HD: [u8; 95] = [",
                                   "pub static ADVANCE_XX: [u8; 95] = ["))
    except ValueError:
        pass
    else:
        raise AssertionError("advance 拒面失守: 段缺席")

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
            font = ImageFont.truetype(str(font_path), FONT_SIZE)
            # —— 贴边截断守卫（03 号 §1.2-2 推断项收口）：FONT_SIZE=34 对
            # CELL_W=32，宽字形（W=30/m=31）贴格边——全 95 形以 2×CELL_W
            # 宽画布同参数重渲染，ink 宽必与窄格渲染一致，否则 32px 格
            # 存在截断（生成期就该改字号或格宽，不许带病入册）。
            for cp, _label, vals in entries:
                ch = chr(cp)
                a = round(font.getlength(ch))
                img = Image.new("L", (CELL_W * 2, CELL_H), 0)
                ImageDraw.Draw(img).text(
                    ((CELL_W * 2 - a) // 2, BASELINE), ch, font=font, fill=255, anchor="ls"
                )
                wxs: list[int] = []
                for y in range(CELL_H):
                    for x in range(CELL_W * 2):
                        if img.getpixel((x, y)) >= THRESHOLD:
                            wxs.append(x)
                if not wxs:
                    continue  # SP 等空形无 ink
                bb = _ink_bbox(vals)
                assert bb is not None, f"{ch!r} 窄格无 ink 但宽画布有——渲染不一致"
                w_wide = max(wxs) - min(wxs) + 1
                w_narrow = bb[1] - bb[0] + 1
                assert w_wide == w_narrow, (
                    f"{ch!r} ink 宽窄格 {w_narrow}px < 宽画布 {w_wide}px——CELL_W=32 截断"
                )
        else:
            pil_note = "skip（字源不在仓）"

    print(
        f"genhdfont: selftest ok (95 形结构钉板 + 黄金哈希 ×2 + advance 锚 9 形 + "
        f"'Browser Settings'=269 + 几何锚 8 面 + 拒面 12；PIL 对拍 {pil_note})"
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
