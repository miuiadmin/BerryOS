#!/usr/bin/env python3
"""生成 32×48 A2 分页压缩 CJK 字库（Noto Sans CJK SC / 思源黑体 OFL 源）。

用法：
  python3 tools/gencjkfont.py [NotoSansCJKsc-Regular.otf]
  python3 tools/gencjkfont.py --check     # 漂移守卫（在册产物，绝不写盘）
  python3 tools/gencjkfont.py --selftest  # 门禁自检（挂 just test-host，零字源面）

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

try:
    from PIL import Image, ImageDraw, ImageFont  # type: ignore
except ImportError:  # selftest/--check 降级面；产线生成路径硬依赖 PIL
    Image = ImageDraw = ImageFont = None

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


def render_a2(font, ch: str) -> tuple[bytes, tuple[int, int, int, int], int, int]:
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


def selftest() -> int:
    """门禁自检（a-3 U4 纪律，genfbfont/genhdfont/gen_splash_* 同族）——零字源面。

    字源 16MiB 不入库（docstring 建档缺席为常态），自检只用 tracked 产物 +
    字符表做全链结构/语义对账：
    - 常量互锁律：.rs 侧常量 ↔ 本件常量 ↔ 数学关系逐项对账。
    - A2 页流全链解码对账：按 .rs 侧页偏移解 zlib → 逐页几何对账 →
      19 条 CJK_GOLDEN 语义 golden（bbox/ink/first/head/tail）从解出的
      A2 字节重算对账——位打包律（2bit/像素、shift = 6-2*(i%4)）+
      四档量化律（(v*3+127)//255）双律全链验证。
    - 码点表律：3500 项升序去重 + CJK 区段 + 排序 = 字符表排序
      （chars_sorted 律）+ LABELS 十字全在表。
    - 量化数学钉板：四档边界值的档位归属。
    - 系统字体烟测（PIL 在位才跑；缺席 skip 不 FAIL——字源恒缺席 CI 面
      黄金 SHA 兜底）：render_a2 输出非空紧致 bbox。
    - 突变红分支：golden 域篡改/偏移越尾/码点乱序 三突变各咬对应律。
    """
    import re

    # ---- 律函数（纯）：真实对账与突变红分支共用同一实现。----
    def law_offsets(offs: list[int], data_len: int) -> list[str]:
        bad = []
        if not offs or offs[0] != 0 or offs[-1] != data_len:
            bad.append("偏移首 0 / 尾 = bin 长不成立")
        if any(b <= a for a, b in zip(offs, offs[1:])):
            bad.append("偏移非严格递增")
        return bad

    def law_pages(raw_pages: list[bytes], n_chars: int) -> list[str]:
        bad = []
        for p, raw in enumerate(raw_pages):
            expect = min(n_chars - p * PAGE_CHARS, PAGE_CHARS) * GLYPH_A2_BYTES
            if len(raw) != expect:
                bad.append(f"页 {p} 解压长 {len(raw)} != {expect}")
        return bad

    def law_cptable(cps: list[int], chars: str) -> list[str]:
        bad = []
        if len(cps) != len(chars):
            bad.append(f"项数 {len(cps)} != {len(chars)}")
        if cps != sorted(cps) or len(set(cps)) != len(cps):
            bad.append("非升序或去重律破")
        if not all(0x4E00 <= cp <= 0x9FFF for cp in cps):
            bad.append("含 CJK 区段外码点")
        if cps != sorted(ord(ch) for ch in chars):
            bad.append("!= 字符表排序（chars_sorted 律）")
        return bad

    def law_quant(v: int) -> int:
        return (v * 3 + 127) // 255

    def metrics(packed: bytes) -> tuple:
        """render_a2 打包字节 -> (bbox, ink, first, head, tail) 重算。"""
        ink = 0
        first = 0xFFFF
        x0, y0, x1, y1 = CELL_W, CELL_H, -1, -1
        for i in range(CELL_W * CELL_H):
            level = (packed[i // 4] >> (6 - 2 * (i % 4))) & 0x3
            if level:
                ink += 1
                if first == 0xFFFF:
                    first = i
                x, y = i % CELL_W, i // CELL_W
                x0, y0 = min(x0, x), min(y0, y)
                x1, y1 = max(x1, x), max(y1, y)
        return (x0, y0, x1, y1), ink, first, packed[0], packed[-1]

    fails: list[str] = []
    charset_path = REPO / "user/berry-gfx/fonts/cjk3500-frequency.txt"
    source_path = REPO / "user/berry-gfx/src/font_data_cjk.rs"
    binary = REPO / "user/berry-gfx/src/font_data_cjk_a2.bin"
    chars_freq = charset_path.read_text(encoding="utf8")
    source = source_path.read_text(encoding="utf8")
    data = binary.read_bytes()

    # ① 常量互锁律。
    if sha256(charset_path) != CHARSET_SHA:
        fails.append("常量互锁: 字符表 sha256 漂移")
    m = re.search(r'CJK_A2_SHA256: &str = "([0-9a-f]{64})"', source)
    if m is None:
        fails.append("常量互锁: .rs 缺 CJK_A2_SHA256 声明")
    elif m.group(1) != hashlib.sha256(data).hexdigest():
        fails.append("常量互锁: bin sha256 与 .rs 在册值不等")
    n_chars = len(chars_freq)
    for pat, want in [
        (r"CJK_CHAR_COUNT: usize = (\d+)", n_chars),
        (r"CJK_PAGE_CHARS: usize = (\d+)", PAGE_CHARS),
        (r"CJK_PAGE_RAW_BYTES: usize = (\d+)", PAGE_RAW_BYTES),
        (r"CJK_GLYPH_A2_BYTES: usize = (\d+)", GLYPH_A2_BYTES),
        (r"CJK_PAGE_COUNT: usize = (\d+)", (n_chars + PAGE_CHARS - 1) // PAGE_CHARS),
    ]:
        mm = re.search(pat, source)
        if mm is None or int(mm.group(1)) != want:
            fails.append(f"常量互锁: {pat.split(':')[0]} != {want}")

    # 码点表（golden 对账的索引基座，先于 ② 解析）。
    m_cp = re.search(r"CJK_CODEPOINTS: \[u32; (\d+)\] = \[(.*?)\];", source, re.S)
    if m_cp is None:
        fails.append("码点表: .rs 缺 CJK_CODEPOINTS 声明")
        cp_table_list: list[int] = []
    else:
        cp_table_list = [int(x, 16) for x in re.findall(r"0x([0-9A-F]{4})", m_cp.group(2))]
    cp_table = {cp: i for i, cp in enumerate(cp_table_list)}

    # ② A2 页流全链解码对账。
    m_off = re.search(r"CJK_PAGE_OFFSETS: \[u32; (\d+)\] = \[(.*?)\];", source, re.S)
    offsets: list[int] = []
    raw_pages: list[bytes] = []
    if m_off is None:
        fails.append("页流: .rs 缺 CJK_PAGE_OFFSETS 声明")
    else:
        offsets = [
            int(x, 16)
            for x in m_off.group(2).replace("\n", "").split(",")
            if x.strip()
        ]
        fails += [f"页流: {msg}" for msg in law_offsets(offsets, len(data))]
        if not fails or all(not f.startswith("页流") for f in fails):
            try:
                raw_pages = [
                    zlib.decompress(data[offsets[p]:offsets[p + 1]])
                    for p in range(len(offsets) - 1)
                ]
                fails += [f"页流: {msg}" for msg in law_pages(raw_pages, n_chars)]
            except zlib.error as e:
                fails.append(f"页流: zlib 解压失败 {e}")
    glyphs: list[bytes] = []
    if raw_pages:
        raw_all = b"".join(raw_pages)
        glyphs = [raw_all[g:g + GLYPH_A2_BYTES] for g in range(0, len(raw_all), GLYPH_A2_BYTES)]
        if len(glyphs) != n_chars:
            fails.append(f"页流: 字形总数 {len(glyphs)} != {n_chars}")
            glyphs = []

    # ②b 语义 golden 重算对账（位打包律 + 量化律全链）。
    golden_decl = re.findall(
        r"cp: 0x([0-9A-F]{4}), bbox: \[(\d+), (\d+), (\d+), (\d+)\], "
        r"ink: (\d+), first: (\d+), head: 0x([0-9A-F]{2}), tail: 0x([0-9A-F]{2})",
        source,
    )
    if not golden_decl:
        fails.append("golden: .rs 缺 CJK_GOLDEN 声明")
    g_metrics = {}
    for g in golden_decl:
        cp = int(g[0], 16)
        if cp not in cp_table:
            fails.append(f"golden: U+{cp:04X} 不在码点表")
            continue
        want = (
            (int(g[1]), int(g[2]), int(g[3]), int(g[4])),
            int(g[5]), int(g[6]), int(g[7], 16), int(g[8], 16),
        )
        g_metrics[cp] = want
        if not glyphs:
            continue
        got = metrics(glyphs[cp_table[cp]])
        if got != want:
            fails.append(f"golden: U+{cp:04X} 重算 {got} != 在册 {want}")

    # ③ 码点表律。
    if cp_table_list:
        fails += [f"码点表: {msg}" for msg in law_cptable(cp_table_list, chars_freq)]
    if not all(ch in chars_freq for ch in LABELS):
        fails.append("码点表: 桌面标签十字缺席")

    # ④ 量化数学钉板：(v*3+127)//255 四档边界归属。
    for v, want in {0: 0, 42: 0, 43: 1, 84: 1, 85: 1, 127: 1, 128: 2,
                    169: 2, 170: 2, 212: 2, 213: 3, 255: 3}.items():
        if law_quant(v) != want:
            fails.append(f"量化: v={v} 档 {law_quant(v)} != {want}")

    # ⑤ 系统字体烟测（PIL 在位才跑；缺席 skip 不 FAIL）。
    pil_note = "skip（PIL/字源缺席——黄金 SHA 兜底）"
    try:
        from PIL import ImageFont  # noqa: F401
        font = ImageFont.truetype("/System/Library/Fonts/PingFang.ttc", FONT_SIZE)
        _, bbox, ink, _first = render_a2(font, "设")
        if bbox[0] > bbox[2] or bbox[1] > bbox[3]:
            fails.append("PIL 烟测: bbox 非紧致")
        if ink == 0:
            fails.append("PIL 烟测: 零着墨")
        pil_note = "PingFang 烟测过"
    except Exception:
        pass

    # ⑥ 突变红分支——三律各咬一口（守卫自身坏防线）。
    if glyphs and golden_decl:
        # 突变①：golden head 域篡改 → 重算不等必咬。
        cp0 = int(golden_decl[0][0], 16)
        good = g_metrics[cp0]
        mutated = (good[0], good[1], good[2], (good[3] ^ 0xFF) & 0xFF, good[4])
        if metrics(glyphs[cp_table[cp0]]) == mutated:
            fails.append("突变①未咬：head 域篡改后重算仍等（守卫失效）")
    if offsets:
        # 突变②：偏移尾削 1 → offsets[-1] != len(data) 必咬。
        if not law_offsets(offsets[:-1] + [offsets[-1] - 1], len(data)):
            fails.append("突变②未咬：偏移尾削 1 溜过偏移律")
    if len(cp_table_list) > 202:
        # 突变③：码点 200/201 互换 → 升序律必咬。
        cp_bad = cp_table_list[:200] + [cp_table_list[201], cp_table_list[200]] + cp_table_list[202:]
        if not law_cptable(cp_bad, chars_freq):
            fails.append("突变③未咬：码点乱序溜过码点表律")

    if fails:
        for msg in fails:
            print(f"gencjkfont selftest: FAIL {msg}", file=sys.stderr)
        return 1
    print(
        f"gencjkfont: selftest ok (常量互锁 x5 + 页流 {len(raw_pages)} 页 "
        f"{len(glyphs)} 形 + golden {len(golden_decl)} 条 + 量化 x12 + "
        f"突变红 x3；PIL {pil_note})"
    )
    return 0


def main() -> int:
    args = sys.argv[1:]
    if "--selftest" in args:
        return selftest()
    if args == ["--check"]:
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
