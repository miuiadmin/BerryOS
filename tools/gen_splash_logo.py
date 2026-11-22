#!/usr/bin/env python3
"""gen_splash_logo.py — splash BERRYOS 大字 logo HD 字模生成器（splash_logo.rs 的源）。

    python3 tools/gen_splash_logo.py             # 产线生成（写 kernel/src/splash_logo.rs，须在仓根跑）
    python3 tools/gen_splash_logo.py --check     # 漂移守卫：在册产物 ↔ 黄金哈希/PIL 重产（绝不写盘）
    python3 tools/gen_splash_logo.py --selftest  # 门禁自检（挂 just test-host）

Helvetica Bold 120px 1bpp 紧致 bbox；行宽 475 位 > 128 → 每行 4×u128 位包。
改字体/字号后重跑本脚本再编内核。依赖：Pillow（brew python3）。

自检面（a-3 U4 纪律，genfbfont/genhdfont 同族——本件此前零守卫）：
- 结构律零依赖（re 解析在册产物）：LOGO_W/H/WORDS 三常量 ↔ 行数/每行包数
  互锁 + 每 word 恰 32 位小写 hex + LOGO_ROWS 类型行 {H}×{WORDS} 钉板。
- 位域律：包 i 覆盖列 [i*128,(i+1)*128)——尾包（WORDS-1）列 475..511 的
  无效位区（bit 0..36）全零；派生律 WORDS == ceil(W/128)。
- 紧致 bbox 律：首/末行有墨（垂直紧致）+ 列 0 与列 W-1 至少一行有墨
  （水平紧致）——getbbox 语义的机器镜像。
- 黄金哈希：整件 sha256（字源/字号/阈值变更 → 必漂移，有意识重钉本哈希）。
- PIL 重产对拍（PIL 与字源在位才跑；缺席打 skip 不 FAIL——CI Linux 面降级，
  黄金哈希兜底）：render()+emit() ↔ 在册产物逐字节恒等。
- 突变红分支：尾包无效位置位/削包/文本单字符篡改 三突变各咬对应律。
"""
from __future__ import annotations

import hashlib
import io
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "kernel" / "src" / "splash_logo.rs"
FONT = "/System/Library/Fonts/Helvetica.ttc"

# 黄金哈希（2026-11-22 首钉，本机 PIL 12.3 重产逐字节恒等双源）。
GOLDEN_SHA256 = "858f9ceba39fcddaa00af638fa47526dbaee0ca19ac592703689d8d2ad5f66e6"

try:
    from PIL import Image, ImageDraw, ImageFont  # type: ignore
except ImportError:  # selftest/--check 降级面；产线生成路径硬依赖 PIL
    Image = ImageDraw = ImageFont = None


def render() -> tuple[int, int, list[list[int]]]:
    """PIL 渲染 → (w, h, rows)；每行 WORDS 个 u128 位包（bit 127-(x%128) = 列 x）。"""
    f = ImageFont.truetype(FONT, 120, index=1)  # index 1 = Bold
    text = "BerryOS"
    img = Image.new('L', (1400, 220), 0)
    d = ImageDraw.Draw(img)
    d.text((20, 20), text, font=f, fill=255)
    bb = img.getbbox()
    assert bb, "empty render"
    w, h = bb[2]-bb[0], bb[3]-bb[1]
    px = img.load()
    words = (w + 127) // 128  # 每行 u128 包数
    rows = []
    for y in range(h):
        ws = [0] * words
        for x in range(w):
            if px[bb[0]+x, bb[1]+y] > 96:
                ws[x // 128] |= 1 << (127 - (x % 128))
        rows.append(ws)
    return w, h, rows


def emit(w: int, h: int, rows: list[list[int]]) -> str:
    """位包 → splash_logo.rs 全文（与产线写盘内容同源单发射器）。"""
    words = (w + 127) // 128
    buf = io.StringIO()
    buf.write('//! splash BERRYOS 大字 logo HD 字模（自动生成勿手编——\n')
    buf.write('//! tools/gen_splash_logo.py；Helvetica Bold 120px 1bpp 紧致 bbox，\n')
    buf.write(f'//! 行宽 {w} 位 → 每行 {words}×u128 位包（包 i 覆盖列 [i*128, (i+1)*128)，\n')
    buf.write('//! 包内 bit (127-(x%128)) = 列 x（左起高位））。\n\n')
    buf.write(f'/// logo 宽（px）。\npub const LOGO_W: u32 = {w};\n')
    buf.write(f'/// logo 高（px）。\npub const LOGO_H: u32 = {h};\n')
    buf.write(f'/// 每行 u128 包数。\npub const LOGO_WORDS: usize = {words};\n')
    buf.write(f'/// 行位包（[{h}][{words}]，包 i bit (127-(x%128)) = 列 x）。\n')
    buf.write(f'pub static LOGO_ROWS: &[[u128; {words}]; {h}] = &[\n')
    for ws in rows:
        buf.write('    [' + ', '.join(f'0x{v:032x}' for v in ws) + '],\n')
    buf.write('];\n')
    return buf.getvalue()


# ---- 解析与执法律（零 PIL 依赖——CI Linux 面同样执法） ----

ROW_RE = re.compile(r"^    \[(0x[0-9a-f]{32}(?:, 0x[0-9a-f]{32})*)\],$", re.M)
CONST_RE = r"pub const LOGO_%s: %s = (\d+);"


def parse(text: str) -> dict:
    """在册产物 → {w, h, words, rows}；结构不合形即抛（拒面即守卫）。"""
    mw = re.search(CONST_RE % ("W", "u32"), text)
    mh = re.search(CONST_RE % ("H", "u32"), text)
    mo = re.search(CONST_RE % ("WORDS", "usize"), text)
    assert mw and mh and mo, "常量行缺席/形漂"
    rows = [[int(x, 16) for x in re.findall(r"0x[0-9a-f]{32}", m)]
            for m in ROW_RE.findall(text)]
    return {"w": int(mw.group(1)), "h": int(mh.group(1)),
            "words": int(mo.group(1)), "rows": rows, "text": text}


def law_structure(d: dict) -> list[str]:
    """常量 ↔ 行几何互锁 + hex 形态 + 类型行钉板。"""
    f = []
    if len(d["rows"]) != d["h"]:
        f.append(f"行数 {len(d['rows'])} ≠ LOGO_H {d['h']}")
    if any(len(r) != d["words"] for r in d["rows"]):
        f.append("存在行包数 ≠ LOGO_WORDS")
    if d["words"] != (d["w"] + 127) // 128:
        f.append(f"LOGO_WORDS {d['words']} ≠ ceil(W/128) {(d['w'] + 127) // 128}")
    if f"pub static LOGO_ROWS: &[[u128; {d['words']}]; {d['h']}]" not in d["text"]:
        f.append("LOGO_ROWS 类型行 {H}×{WORDS} 漂移")
    # hex 32 位小写形态由 ROW_RE 全量匹配保证：任何一行不合形即行数缺口 → 上面必咬
    return f


def law_bitdomain(d: dict) -> list[str]:
    """尾包无效位区（列 w..words*128-1 → bit 127-(x%128) 全段）全零。"""
    tail = d["words"] * 128 - d["w"]
    if tail <= 0:
        return []
    invalid_mask = (1 << tail) - 1  # 尾包低 tail 位 = 列 w..words*128-1
    bad = [i for i, r in enumerate(d["rows"]) if r[-1] & invalid_mask]
    return [f"行 {i} 尾包列 {d['w']}..{d['words']*128-1} 无效位区有墨" for i in bad]


def law_compact(d: dict) -> list[str]:
    """getbbox 紧致语义镜像：四缘必有墨。"""
    f = []
    ink = [any(r) for r in d["rows"]]
    if not (ink and ink[0] and ink[-1]):
        f.append("首/末行非双有墨（垂直 bbox 非紧致）")
    left = any(r[0] & (1 << 127) for r in d["rows"])
    right_bit = 127 - ((d["w"] - 1) % 128)
    right = any(r[-1] & (1 << right_bit) for r in d["rows"])
    if not left:
        f.append("列 0 全空（左缘非紧致）")
    if not right:
        f.append(f"列 {d['w']-1} 全空（右缘非紧致）")
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
        w, h, rows = render()
        note = "PIL 重产对拍"
        if emit(w, h, rows) != text:
            fails.append("--check 失败：在册产物 ≠ PIL 重产（改字体/字号后重跑生成器）")
    if fails:
        for m in fails:
            print(f"gen_splash_logo --check: FAIL {m}", file=sys.stderr)
        return 1
    print(f"check ok: 在册产物 = 黄金哈希（{note}）")
    return 0


def selftest() -> int:
    fails: list[str] = []
    text = ART.read_text(encoding="utf-8")
    d = parse(text)

    # ① 结构律。
    fails += [f"结构律: {m}" for m in law_structure(d)]
    # ② 位域律（尾包无效位区全零 + 派生包数）。
    fails += [f"位域律: {m}" for m in law_bitdomain(d)]
    # ③ 紧致 bbox 律（四缘有墨）。
    fails += [f"紧致律: {m}" for m in law_compact(d)]
    # ④ 黄金哈希。
    fails += [f"黄金哈希: {m}" for m in law_golden(text)]
    # ⑤ 突变红分支——三条律各咬一口（守卫自身坏防线）。
    import copy
    m1 = copy.deepcopy(d)
    m1["rows"][0][-1] |= 1  # 尾包无效位置位
    if not law_bitdomain(m1):
        fails.append("突变①未咬：尾包无效位置位溜过位域律")
    m2 = copy.deepcopy(d)
    m2["rows"][0].pop()  # 削一包
    if not law_structure(m2):
        fails.append("突变②未咬：削包溜过结构律")
    # 突变③：文本单字符篡改 → 黄金哈希必红（law_golden 非空 = 咬住 = 正确）。
    old = "0x" + format(d["rows"][0][-1], "032x") + "]"
    new = "0x" + format(d["rows"][0][-1] + 1, "032x") + "]"
    m3 = text.replace(old, new, 1)
    if m3 == text:
        fails.append("突变③构造失败：目标串缺席")
    elif not law_golden(m3):
        fails.append("突变③未咬：篡改后黄金哈希律仍绿")
    # ⑥ PIL 重产对拍（在位才跑；缺席 skip 不 FAIL——genhdfont 同族降级面）。
    pil_note = "skip（PIL/字源缺席——黄金哈希兜底）"
    if _pil_available():
        w, h, rows = render()
        pil_note = "恒等"
        if emit(w, h, rows) != text:
            fails.append("PIL 重产对拍失败：emit(render()) ≠ 在册产物")

    if fails:
        for m in fails:
            print(f"gen_splash_logo selftest: FAIL {m}", file=sys.stderr)
        return 1
    print(f"gen_splash_logo: selftest ok (结构/位域/紧致/黄金哈希 ×4 + 突变红 ×3；PIL 对拍 {pil_note})")
    return 0


def main() -> int:
    args = sys.argv[1:]
    if "--selftest" in args:
        return selftest()
    if "--check" in args:
        return check()
    if args:
        print(f"未知参数: {args}（用法: gen_splash_logo.py [--check|--selftest]）", file=sys.stderr)
        return 2
    # 产线生成：与历史行为逐字节同（仓根相对路径写盘）。
    w, h, rows = render()
    with open('kernel/src/splash_logo.rs', 'w') as fp:
        fp.write(emit(w, h, rows))
    print(f"written kernel/src/splash_logo.rs  {w}x{h}  words/row={(w + 127) // 128}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
