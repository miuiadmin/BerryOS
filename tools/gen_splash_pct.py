#!/usr/bin/env python3
"""gen_splash_pct.py — splash 百分比 HD 字模生成器（splash_pct.rs 的源）。

    python3 tools/gen_splash_pct.py             # 产线生成（写 kernel/src/splash_pct.rs，须在仓根跑）
    python3 tools/gen_splash_pct.py --check     # 漂移守卫：在册产物 ↔ 黄金哈希/PIL 重产（绝不写盘）
    python3 tools/gen_splash_pct.py --selftest  # 门禁自检（挂 just test-host）

Helvetica 96px 1bpp 紧致 bbox，u128 行位包。改字体/字号后重跑本脚本再编内核。
依赖：Pillow（brew python3）。

自检面（a-3 U4 纪律，genfbfont/genhdfont/gen_splash_logo 同族——本件此前零守卫）：
- 结构律零依赖（re 解析在册产物）：字形序恰 '0'-'9'+'%' 11 形 + 每形
  (b'c', w, h, [72×u128]) 元组形 + PCT_GLYPHS 类型行 [u128; 72] 钉板。
- 位域律：值域 v < 2^w（bit (w-1-x) = 列 x，w 位宽外无墨）+ 行位图恰
  72 行 + 紧致 bbox 垂直律（行 0 与行 h-1 有墨、行 h..71 零填充）。
- hex 宽度律：每值 (w+3)//4*4 位小写 hex（w=25→7 位、w=79→20 位）。
- 黄金哈希：整件 sha256（字源/字号/阈值变更 → 必漂移，有意识重钉本哈希）。
- PIL 重产对拍（PIL 与字源在位才跑；缺席打 skip 不 FAIL——CI Linux 面降级，
  黄金哈希兜底）：render()+emit() ↔ 在册产物逐字节恒等。
- 突变红分支：值域越帽/削行/文本单字符篡改 三突变各咬对应律。
"""
from __future__ import annotations

import copy
import hashlib
import io
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "kernel" / "src" / "splash_pct.rs"
FONT = "/System/Library/Fonts/Helvetica.ttc"
ROWS_FIXED = 72  # [u128; 72] 定容行位图（类型行钉板值）
CHARS = '0123456789%'

# 黄金哈希（2026-11-22 首钉，本机 PIL 12.3 重产逐字节恒等双源）。
GOLDEN_SHA256 = "2a43c600e960f2b1f9d02195eee4fa581593d807a47b2c71c5c5733d2e6b55b4"

try:
    from PIL import Image, ImageDraw, ImageFont  # type: ignore
except ImportError:  # selftest/--check 降级面；产线生成路径硬依赖 PIL
    Image = ImageDraw = ImageFont = None


def render() -> list[tuple[str, int, int, list[int]]]:
    """PIL 渲染 CHars → [(字符, w, h, 72 行位图)]；bit (w-1-x) = 列 x。"""
    f = ImageFont.truetype(FONT, 96)
    out = []
    for c in CHARS:
        img = Image.new('L', (160, 160), 0)
        d = ImageDraw.Draw(img)
        d.text((20, 20), c, font=f, fill=255)
        bb = img.getbbox()
        w, h = bb[2]-bb[0], bb[3]-bb[1]
        px = img.load()
        rows = []
        for y in range(h):
            v = 0
            for x in range(w):
                if px[bb[0]+x, bb[1]+y] > 96:
                    v |= 1 << (w-1-x)
            rows.append(v)
        rows += [0]*(ROWS_FIXED-h)
        out.append((c, w, h, rows))
    return out


def emit(glyphs: list[tuple[str, int, int, list[int]]]) -> str:
    """字形集 → splash_pct.rs 全文（与产线写盘内容同源单发射器）。"""
    out = []
    out.append('//! splash 百分比 HD 字模（自动生成勿手编——tools/gen_splash_pct.py；')
    out.append('//! Helvetica 96px 1bpp 紧致 bbox，u128 行位包。iOS 式黑白 splash 专用。')
    out.append('')
    out.append('/// (字符, 宽, 高, 行位图)。位序：bit (w-1-x) = 列 x（左起高位）。')
    out.append(f'pub static PCT_GLYPHS: &[(u8, u32, u32, [u128; {ROWS_FIXED}])] = &[')
    for c, w, h, rows in glyphs:
        width = (w + 3) // 4 * 4
        out.append(f"    (b'{c}', {w}, {h}, [")
        for v in rows:
            out.append('        0x' + format(v, '0' + str(width) + 'x') + ',')
        out.append('    ]),')
    out.append('];')
    return '\n'.join(out)


# ---- 解析与执法律（零 PIL 依赖——CI Linux 面同样执法） ----

GLYPH_RE = re.compile(
    r"\(b'(.)', (\d+), (\d+), \[\n((?:        0x[0-9a-f]+,\n)+)    \]\)")


def parse(text: str) -> dict:
    """在册产物 → {glyphs, text}；结构不合形即抛（拒面即守卫）。"""
    glyphs = []
    for m in GLYPH_RE.finditer(text):
        c, w, h, body = m.group(1), int(m.group(2)), int(m.group(3)), m.group(4)
        vs = [int(x, 16) for x in re.findall(r'0x([0-9a-f]+)', body)]
        hexes = re.findall(r'0x([0-9a-f]+),', body)
        glyphs.append({"c": c, "w": w, "h": h, "rows": vs, "hexes": hexes})
    return {"glyphs": glyphs, "text": text}


def law_structure(d: dict) -> list[str]:
    """字形序 + 元组形 + 类型行。"""
    f = []
    got = ''.join(g["c"] for g in d["glyphs"])
    if got != CHARS:
        f.append(f"字形序 {got!r} ≠ {CHARS!r}")
    if f'pub static PCT_GLYPHS: &[(u8, u32, u32, [u128; {ROWS_FIXED}])]' not in d["text"]:
        f.append("PCT_GLYPHS 类型行 [u128; 72] 漂移")
    return f


def law_bitdomain(d: dict) -> list[str]:
    """每形：72 行恰容 + 值域 v < 2^w + 尾段零填充 + hex 宽度律。"""
    f = []
    for g in d["glyphs"]:
        c, w, h = g["c"], g["w"], g["h"]
        if len(g["rows"]) != ROWS_FIXED:
            f.append(f"{c!r} 行数 {len(g['rows'])} ≠ {ROWS_FIXED}")
            continue
        for i, v in enumerate(g["rows"]):
            if v >= (1 << w):
                f.append(f"{c!r} 行 {i} 值越 {w} 位帽")
                break
        if not (1 <= h <= ROWS_FIXED):
            f.append(f"{c!r} 高 {h} 出域 [1,{ROWS_FIXED}]")
            continue
        if any(v != 0 for v in g["rows"][h:]):
            f.append(f"{c!r} 零填充段行 {h}..{ROWS_FIXED-1} 有墨")
        width = (w + 3) // 4 * 4
        if any(len(x) != width for x in g["hexes"]):
            f.append(f"{c!r} hex 宽度 ≠ (w+3)//4*4 = {width}")
        # 紧致 bbox 垂直律：行 0 与行 h-1 有墨。
        if g["rows"][0] == 0 or g["rows"][h-1] == 0:
            f.append(f"{c!r} 行 0/行 {h-1} 非双有墨（垂直 bbox 非紧致）")
    return f


def law_golden(text: str) -> list[str]:
    got = hashlib.sha256(text.encode()).hexdigest()
    return [] if got == GOLDEN_SHA256 else [f"整件 sha256 {got} ≠ 黄金 {GOLDEN_SHA256}（改字源/字号/阈值须有意识重钉）"]


def _pil_available() -> bool:
    return Image is not None and Path(FONT).is_file()


def check() -> int:
    """--check 漂移守卫：黄金哈希恒执法 + PIL 在位时重产逐字节对拍；绝不写盘。"""
    text = ART.read_text(encoding="utf-8")
    fails = law_golden(text)
    note = "skip（PIL/字源缺席——黄金哈希兜底）"
    if _pil_available():
        note = "PIL 重产对拍"
        if emit(render()) != text:
            fails.append("--check 失败：在册产物 ≠ PIL 重产（改字体/字号后重跑生成器）")
    if fails:
        for m in fails:
            print(f"gen_splash_pct --check: FAIL {m}", file=sys.stderr)
        return 1
    print(f"check ok: 在册产物 = 黄金哈希（{note}）")
    return 0


def selftest() -> int:
    fails: list[str] = []
    text = ART.read_text(encoding="utf-8")
    d = parse(text)

    # ① 结构律。
    fails += [f"结构律: {m}" for m in law_structure(d)]
    # ② 位域律（值域/恰容/零填充/hex 宽度/紧致垂直）。
    fails += [f"位域律: {m}" for m in law_bitdomain(d)]
    # ③ 黄金哈希。
    fails += [f"黄金哈希: {m}" for m in law_golden(text)]
    # ④ 突变红分支——三条律各咬一口（守卫自身坏防线）。
    m1 = copy.deepcopy(d)
    m1["glyphs"][0]["rows"][0] |= (1 << m1["glyphs"][0]["w"])  # 值域越帽
    if not law_bitdomain(m1):
        fails.append("突变①未咬：值域越帽溜过位域律")
    m2 = copy.deepcopy(d)
    m2["glyphs"][0]["rows"].pop()  # 削一行 → 71 行
    if not law_bitdomain(m2):
        fails.append("突变②未咬：削行溜过位域律")
    # 突变③：文本单字符篡改 → 黄金哈希必红（law_golden 非空 = 咬住 = 正确）。
    old = "0x" + d["glyphs"][0]["hexes"][0] + ","
    new = "0x" + format(d["glyphs"][0]["rows"][0] + 1, "0" + str(len(d["glyphs"][0]["hexes"][0])) + "x") + ","
    m3 = text.replace(old, new, 1)
    if m3 == text:
        fails.append("突变③构造失败：目标串缺席")
    elif not law_golden(m3):
        fails.append("突变③未咬：篡改后黄金哈希律仍绿")
    # ⑤ PIL 重产对拍（在位才跑；缺席 skip 不 FAIL——genhdfont 同族降级面）。
    pil_note = "skip（PIL/字源缺席——黄金哈希兜底）"
    if _pil_available():
        pil_note = "恒等"
        if emit(render()) != text:
            fails.append("PIL 重产对拍失败：emit(render()) ≠ 在册产物")

    if fails:
        for m in fails:
            print(f"gen_splash_pct selftest: FAIL {m}", file=sys.stderr)
        return 1
    print(f"gen_splash_pct: selftest ok (结构/位域/黄金哈希 ×3 + 突变红 ×3；PIL 对拍 {pil_note})")
    return 0


def main() -> int:
    args = sys.argv[1:]
    if "--selftest" in args:
        return selftest()
    if "--check" in args:
        return check()
    if args:
        print(f"未知参数: {args}（用法: gen_splash_pct.py [--check|--selftest]）", file=sys.stderr)
        return 2
    # 产线生成：与历史行为逐字节同（仓根相对路径写盘）。
    glyphs = render()
    open('kernel/src/splash_pct.rs', 'w').write(emit(glyphs))
    print('ok')
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
