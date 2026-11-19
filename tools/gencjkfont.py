#!/usr/bin/env python3
"""生成 32×48 A2 分页压缩 CJK 字库（Noto Sans CJK SC / 思源黑体 OFL 源）。

用法：
  python3 tools/gencjkfont.py [NotoSansCJKsc-Regular.otf]

默认字源为已建档的上游全量
`Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf`
（notofonts/noto-cjk @ f8d157532fbfaeda587e826d4cd5b21a49186f7c，
SHA-256 2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b；
本仓不重复入库 16MiB 源字体）。字符表固定为
`user/berry-gfx/fonts/cjk3500-frequency.txt`（Jun Da webdict 词频加权推导，
源仓 ling0322/webdict @ 00430ba2cde04d19a78b34ddb87da2d931231877）。

产物：
- `user/berry-gfx/src/font_data_cjk_a2.bin`：64 字/页 zlib 流串联；
- `user/berry-gfx/src/font_data_cjk.rs`：码点表、页偏移与 golden（勿手改）。

设备运行期零 FreeType/零分配：每页 A2 解压到 24,576B 单页缓存，
coverage 只有 0/85/170/255 四档。
"""
from pathlib import Path
import hashlib
import sys
import zlib
from PIL import Image, ImageDraw, ImageFont

CELL_W = 32
CELL_H = 48
GLYPH_A2_BYTES = CELL_W * CELL_H // 4
PAGE_CHARS = 64
PAGE_RAW_BYTES = PAGE_CHARS * GLYPH_A2_BYTES
FONT_SIZE = 136
BASELINE = 152
FONT_SHA = "2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b"
CHARSET_SHA = "7bb32dbd458ed06e95236a3d915cd6053170a13ab5005fb0c56a73d1e8d3248a"
LABELS = "设置相机通讯录文件图库信息浏览器"
REPO = Path(__file__).resolve().parent.parent


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def render_a2(font: ImageFont.FreeTypeFont, ch: str) -> tuple[bytes, tuple[int, int, int, int], int, int]:
    """渲染一个字并返回 (A2 packed bytes, bbox, 着墨像素数, 首个非零像素 index)。"""
    big_w, big_h = CELL_W * 4, CELL_H * 4
    img = Image.new("L", (big_w, big_h), 0)
    draw = ImageDraw.Draw(img)
    advance = round(font.getlength(ch))
    x = max(0, (big_w - advance) // 2)
    draw.text((x, BASELINE), ch, font=font, fill=255, anchor="ls")
    cov = img.resize((CELL_W, CELL_H), Image.Resampling.BOX).tobytes()
    assert len(cov) == CELL_W * CELL_H

    packed = bytearray(GLYPH_A2_BYTES)
    ink = 0
    first = 0xFFFF
    x0, y0, x1, y1 = CELL_W, CELL_H, -1, -1
    for i, value in enumerate(cov):
        # 最近邻四档量化：0/85/170/255（round(v*3/255)）。
        level = (value * 3 + 127) // 255
        if level:
            ink += 1
            if first == 0xFFFF:
                first = i
            x, y = i % CELL_W, i // CELL_W
            x0, y0 = min(x0, x), min(y0, y)
            x1, y1 = max(x1, x), max(y1, y)
        shift = 6 - 2 * (i % 4)
        packed[i // 4] |= level << shift
    assert x1 >= x0
    return bytes(packed), (x0, y0, x1, y1), ink, first


def build(font_path: Path, chars: str) -> tuple[bytes, str, list[int], list[tuple]]:
    if sha256(font_path) != FONT_SHA:
        raise SystemExit(f"gencjkfont: font SHA mismatch: {font_path}")
    font = ImageFont.truetype(str(font_path), FONT_SIZE)
    pages = bytearray()
    offsets = [0]
    golden = []
    for start in range(0, len(chars), PAGE_CHARS):
        group = chars[start:start + PAGE_CHARS]
        raw = bytearray()
        for ch in group:
            raw.extend(render_a2(font, ch)[0])
        pages.extend(zlib.compress(bytes(raw), 9))
        offsets.append(len(pages))

    # 语义 golden：首/中/尾 + 官方桌面标签各一字。生成器重跑必须同值。
    indexes = {0, len(chars) // 2, len(chars) - 1}
    indexes.update(chars.index(ch) for ch in LABELS)
    for i in sorted(indexes):
        packed, bbox, ink, first = render_a2(font, chars[i])
        golden.append((ord(chars[i]), bbox, ink, first, packed[0], packed[-1]))
    return bytes(pages), "".join(
        f"    0x{x:04X}," for x in sorted(ord(ch) for ch in chars)
    ), offsets, golden


def check() -> int:
    charset_path = REPO / "user/berry-gfx/fonts/cjk3500-frequency.txt"
    if sha256(charset_path) != CHARSET_SHA:
        raise SystemExit("gencjkfont check FAIL: charset SHA drift")
    binary = REPO / "user/berry-gfx/src/font_data_cjk_a2.bin"
    source_path = REPO / "user/berry-gfx/src/font_data_cjk.rs"
    source = source_path.read_text(encoding="utf8")
    import re
    m = re.search(r'CJK_A2_SHA256: &str = "([0-9a-f]{64})"', source)
    if m is None:
        raise SystemExit("gencjkfont check FAIL: source lacks binary SHA")
    got_bin = sha256(binary)
    if got_bin != m.group(1):
        raise SystemExit(
            f"gencjkfont check FAIL: A2 binary SHA drift {got_bin} != {m.group(1)}"
        )
    data = binary.read_bytes()
    offsets = [
        int(x, 16)
        for x in re.search(
            r"CJK_PAGE_OFFSETS: \[u32; ([0-9]+)\] = \[(.*?)\];", source, re.S
        )
        .group(2)
        .replace("\n", "")
        .split(",")
        if x.strip()
    ]
    if not offsets or offsets[0] != 0 or offsets[-1] != len(data):
        raise SystemExit("gencjkfont check FAIL: page offsets/size drift")
    for page in range(len(offsets) - 1):
        raw = zlib.decompress(data[offsets[page]:offsets[page + 1]])
        expect = min(3500 - page * PAGE_CHARS, PAGE_CHARS) * GLYPH_A2_BYTES
        if len(raw) != expect:
            raise SystemExit(
                f"gencjkfont check FAIL: page {page} raw len {len(raw)} != {expect}"
            )
    chars = charset_path.read_text(encoding="utf8")
    if len(chars) != 3500 or not all(ch in chars for ch in LABELS):
        raise SystemExit("gencjkfont check FAIL: charset/labels drift")
    print(
        f"gencjkfont: check ok ({len(data)}B, {len(offsets) - 1} pages, "
        f"3500 chars, sha256={got_bin[:16]}…)"
    )
    return 0


def main() -> int:
    if len(sys.argv) == 2 and sys.argv[1] == "--check":
        return check()

    charset_path = REPO / "user/berry-gfx/fonts/cjk3500-frequency.txt"
    charset_hash = sha256(charset_path)
    if charset_hash != CHARSET_SHA:
        raise SystemExit(f"gencjkfont: charset SHA mismatch: {charset_path}")
    chars_freq = charset_path.read_text(encoding="utf8")
    if len(chars_freq) != 3500 or len(set(chars_freq)) != 3500:
        raise SystemExit("gencjkfont: charset must be 3500 unique CJK chars")
    if not all("\u4e00" <= ch <= "\u9fff" for ch in chars_freq):
        raise SystemExit("gencjkfont: charset contains non-CJK codepoint")
    if not all(ch in chars_freq for ch in LABELS):
        raise SystemExit("gencjkfont: desktop labels absent")

    arg = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(
        "/tmp/NotoSansCJKsc-Regular-full.otf"
    )
    # 页数据与运行期二分表同为码点升序；频率文件保留推导序作溯源。
    chars_sorted = "".join(sorted(chars_freq))
    pages, cp_body, offsets, golden = build(arg, chars_sorted)
    binary = REPO / "user/berry-gfx/src/font_data_cjk_a2.bin"
    binary.write_bytes(pages)
    binary_hash = hashlib.sha256(pages).hexdigest()

    golden_body = "\n".join(
        "    CjkGolden { "
        f"cp: 0x{cp:04X}, bbox: [{x0}, {y0}, {x1}, {y1}], "
        f"ink: {ink}, first: {first}, head: 0x{head:02X}, tail: 0x{tail:02X} }},"
        for cp, (x0, y0, x1, y1), ink, first, head, tail in golden
    )
    offsets_body = ", ".join(f"0x{x:08X}" for x in offsets)
    source = f"""//! 本文件由 tools/gencjkfont.py 生成——勿手改；改字源/字符表后重跑生成器。
//! 字源：Noto Sans CJK SC Regular full OTF（SIL OFL 1.1），上游 commit
//! f8d157532fbfaeda587e826d4cd5b21a49186f7c；频率字符表来自
//! ling0322/webdict @ 00430ba2cde04d19a78b34ddb87da2d931231877 的词频加权。
//! 32×48 A2：每像素 2bit（0/85/170/255），64 字/页 zlib 分页压缩。

pub const CJK_CHAR_COUNT: usize = {len(chars_freq)};
pub const CJK_PAGE_CHARS: usize = {PAGE_CHARS};
pub const CJK_PAGE_COUNT: usize = {len(offsets) - 1};
pub const CJK_PAGE_RAW_BYTES: usize = {PAGE_RAW_BYTES};
pub const CJK_GLYPH_A2_BYTES: usize = {GLYPH_A2_BYTES};

pub static CJK_A2_DATA: &[u8] = include_bytes!("font_data_cjk_a2.bin");
#[cfg(test)]
pub const CJK_A2_SHA256: &str = "{binary_hash}";
#[cfg(test)]
pub const CJK_CHARSET_SHA256: &str = "{charset_hash}";

/// 升序 Unicode 码点表（运行期二分查找；与频率文件排序无关）。
pub static CJK_CODEPOINTS: [u32; {len(chars_freq)}] = [
{cp_body}
];

/// 每页 zlib 流 `[start, end)` 偏移；长度 = 页数 + 1。
pub static CJK_PAGE_OFFSETS: [u32; {len(offsets)}] = [
    {offsets_body},
];

/// 生成期语义 golden：bbox/着墨数/首非零位/首尾 A2 字节。
#[cfg(test)]
pub struct CjkGolden {{
    pub cp: u32,
    pub bbox: [usize; 4],
    pub ink: usize,
    pub first: usize,
    pub head: u8,
    pub tail: u8,
}}

#[cfg(test)]
pub static CJK_GOLDEN: [CjkGolden; {len(golden)}] = [
{golden_body}
];

const _: () = assert!(CJK_PAGE_OFFSETS.len() == CJK_PAGE_COUNT + 1);
const _: () = assert!(CJK_PAGE_OFFSETS[0] == 0);
const _: () = assert!(CJK_A2_DATA.len() == CJK_PAGE_OFFSETS[CJK_PAGE_COUNT] as usize);
const _: () = assert!(CJK_CHAR_COUNT <= CJK_PAGE_COUNT * CJK_PAGE_CHARS);
"""
    out = REPO / "user/berry-gfx/src/font_data_cjk.rs"
    out.write_text(source)
    print(
        f"gencjkfont: {out.relative_to(REPO)} + {binary.relative_to(REPO)} "
        f"({len(pages)}B compressed, {len(chars_freq)} chars, "
        f"{len(offsets) - 1} pages)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
