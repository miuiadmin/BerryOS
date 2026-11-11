#!/usr/bin/env python3
"""从 OpenHarmony 原始视觉资产生成 appstub 108×108 桌面图标。

产物：`user/bins/appstub/apps/<ID>/icons/icon.png`。
输入：`user/bins/appstub/ohassets/`（raw 源 + glyphs 白色 alpha 形）。

用法：
  python3 tools/genohicons.py             # 重产全部图标（硬依赖 PIL）
  python3 tools/genohicons.py --selftest  # 门禁自检（零 PIL 可跑；PIL 在位时加做纯函数向量 + 重产像素对拍）

设计口径：
- 外轮廓 = OpenHarmony `ohos_icon_mask.png`（非自绘圆角）。
- glyph = OpenHarmony SVG/PNG 源光栅化后的 alpha 形。
- 渐变色底 = 本仓生成的 OH 风格配色；仅色值自造，资产形状来自 OH。
- BYI1 消费面是 1-bit alpha：生成末段统一 threshold，防宿主预览与客机不一致。

自检面（a-3 U4 纪律，genhdfont/genohwallpaper --selftest 同族——7 桌面图标此前
只靠二进制入册无守卫，生成器坏或资产换脸 = launcher 桌面静默劣化）：
- 结构钉板零依赖（stdlib zlib 解码在册 PNG）：108×108 RGBA8 + **像素级**黄金
  哈希 ×7（钉解码后像素而非文件字节——PNG 编码器随 PIL 版本漂移属合法通道，
  实测 11.3/12.3 双版本重产像素逐字节等而文件字节 7/7 不等）。
- 消费契约钉：alpha 全 {0,255} 二值（BYI1 1-bit 前置）+ EXACT 族 6-bit 量化
  不变量（rgb 全落 step=4 网格或 255 端点）+ 不透明像素数账面 ×7。
- 锚点 ×3/图标（首不透明/中心/末像素）——与黄金哈希局部冗余，失败更快定位。
- 8 拒面：解码器严格拒绝坏签名/坏 IHDR/坏 IDAT/zlib 腐蚀/截断/非 RGBA8 形/
  尺寸漂移/行数据短缺。
- PIL 在位时加做：gradient_tile 纯函数向量（角恒等/对角对称/单调/确定性合成）+
  量化映射向量 + 重产**像素**对拍 ×7（encoder 字节不比——合法漂移面）；
  PIL 缺席打 skip 不 FAIL（宿主无 PIL 是合法环境，黄金哈希兜底）。
"""
from __future__ import annotations

from pathlib import Path
import hashlib
import struct
import sys
import zlib

try:
    from PIL import Image
except ImportError:  # selftest 结构面零依赖可跑；生成路径硬依赖 PIL
    Image = None

REPO = Path(__file__).resolve().parent.parent
ROOT = REPO / "user/bins/appstub/ohassets"
OUT_ROOT = REPO / "user/bins/appstub/apps"
S = 108
SS = 4
if Image is not None:
    R = Image.Resampling.LANCZOS

# app ID -> (glyph stem, 渐变起色, 渐变止色)。色底为本仓生成的 OH 风格配色。
# 官方 AppScope/entry 图标可直接使用的应用（其余继续 OH glyph + 本仓色底）。
EXACT_ICONS = {
    "SETTINGS": "settings_app.png",
    "FILES": "files.png",
    "PHOTOS": "photos_app.png",
    "CAMERA": "camera_app.png",
    "CONTACTS": "contacts_app.png",
    "MMS": "mms_app.png",
}

APPS = {
    "SETTINGS": ("settings", (31, 143, 255), (0, 199, 190)),
    "FILES": ("files", (255, 159, 46), (255, 106, 0)),
    "PHOTOS": ("photos", (255, 126, 179), (255, 61, 119)),
    "CAMERA": ("camera", (53, 181, 255), (0, 102, 255)),
    "CONTACTS": ("contacts", (52, 195, 160), (0, 125, 100)),
    "MMS": ("messages", (77, 141, 255), (0, 82, 217)),
    "BROWSER": ("browser", (0, 199, 255), (0, 102, 212)),
}


def gradient_tile(w: int, h: int, c0: tuple[int, int, int], c1: tuple[int, int, int]) -> "Image.Image":
    im = Image.new("RGB", (w, h))
    px = im.load()
    den = w + h - 2
    for y in range(h):
        for x in range(w):
            t = (x + y) / den
            px[x, y] = tuple(round(c0[i] * (1 - t) + c1[i] * t) for i in range(3))
    return im


def make_icon(app: str, glyph: str, c0: tuple, c1: tuple) -> None:
    assert Image is not None  # 生成路径硬依赖 PIL（selftest 结构面零依赖，见 import 注释）
    exact = EXACT_ICONS.get(app)
    if exact is not None:
        im = Image.open(ROOT / "raw" / exact).convert("RGBA")
        im = im.resize((S, S), Image.Resampling.LANCZOS)
        # 6-bit/channel 量化：视觉无级差，BYI1 RLE 显著小于照片级原图。
        step = 255 // 63
        im = im.point(lambda v: round(v / step) * step)
        alpha = im.getchannel("A").point(lambda a: 255 if a >= 128 else 0)
        im.putalpha(alpha)
        dest = OUT_ROOT / app / "icons" / "icon.png"
        dest.parent.mkdir(parents=True, exist_ok=True)
        im.save(dest)
        print(f"genohicons: {dest.relative_to(REPO)} exact ({dest.stat().st_size}B)")
        return

    big = S * SS
    mask = Image.open(ROOT / "raw/icon_mask.png").convert("RGBA").getchannel("A")
    mask = mask.resize((big, big), R)

    tile = gradient_tile(big, big, c0, c1).convert("RGBA")
    tile.putalpha(mask)

    g = Image.open(ROOT / "glyphs" / f"{glyph}.png").convert("RGBA")
    bbox = g.getchannel("A").getbbox()
    if bbox is None:
        raise RuntimeError(f"empty glyph: {glyph}")
    g = g.crop(bbox)
    target = int(S * SS * 0.60)
    scale = target / max(g.width, g.height)
    g = g.resize((max(1, round(g.width * scale)), max(1, round(g.height * scale))), R)
    gx = (big - g.width) // 2
    gy = (big - g.height) // 2
    solid = Image.new("RGBA", g.size, (255, 255, 255, 255))
    tile.paste(solid, (gx, gy), g)

    out = tile.resize((S, S), R)
    alpha = out.getchannel("A").point(lambda a: 255 if a >= 128 else 0)
    out.putalpha(alpha)
    dest = OUT_ROOT / app / "icons" / "icon.png"
    dest.parent.mkdir(parents=True, exist_ok=True)
    out.save(dest)
    print(f"genohicons: {dest.relative_to(REPO)} ({dest.stat().st_size}B)")


# ---- selftest（a-3 U4 门禁化纪律；genhdfont/genohwallpaper/elf2flat --selftest 同位） ----

# 黄金像素哈希：7 图标解码后 RGBA 字节流的 sha256。钉**像素**不钉文件字节——
# PNG 编码器输出随 PIL 版本漂移属合法通道（实测 11.3/12.3 重产像素逐字节等、
# 文件字节 7/7 不等）；源资产/掩膜/glyph/配色/量化语义任何变更必漂移——此时
# 跑 `python3 tools/genohicons.py` 重产入册后 **有意识** 更新本表
# （连坐核对 berry-launcher 图标消费面），防静默换脸。
GOLDEN_PX_SHA256 = {
    "SETTINGS": "c7430790d0b3294151846ae1992360a0c76537336d4efff6e3b690949f038da4",
    "FILES": "cb56d18a330baa4a3d339c993b541b96ab9199a852a6b6e32c1995b3b721911d",
    "PHOTOS": "c683342e2c11e89ea7d4a0e22902b572945e9e68878c2537e590db006d1808b4",
    "CAMERA": "3c34306294df403ac5f3964660ab15d1e57977b8c2c90f139c8cf2d44f06b4c2",
    "CONTACTS": "9c0883e8b00830969a901f4a826a3cdc22bce316ac5b36e3b27df36ea62b74f7",
    "MMS": "785785eba1aa3a7f54915b7ff858504a9e02adb15572e86861d973f1a05b3a4d",
    "BROWSER": "ef6834d0d98b0a25968ea69314399dcfd61b8bcf116a81e54be9c13064806355",
}

# 不透明像素数账面（掩膜覆盖面几何）+ 锚点 ×3/图标（首不透明/中心/末像素——
# 与黄金哈希局部冗余，失败时更快定位；首不透明索引顺带钉 mask 边缘几何）。
OPAQUE_COUNTS = {
    "SETTINGS": 11148, "FILES": 2160, "PHOTOS": 11148, "CAMERA": 11148,
    "CONTACTS": 11016, "MMS": 11140, "BROWSER": 11140,
}
# {app: (首个不透明像素索引, 首 anchor RGBA, 中心 RGBA, 末像素 RGBA)}
ANCHORS = {
    "SETTINGS": (22, (212, 216, 232, 255), (200, 204, 224, 255), (0, 0, 0, 0)),
    "FILES": (1529, (0, 0, 0, 255), (0, 0, 0, 0), (0, 0, 0, 0)),
    "PHOTOS": (22, (0, 176, 255, 255), (0, 152, 255, 255), (0, 0, 0, 0)),
    "CAMERA": (22, (255, 255, 255, 255), (248, 248, 248, 255), (0, 0, 0, 0)),
    "CONTACTS": (24, (64, 228, 212, 255), (32, 216, 172, 255), (0, 0, 0, 0)),
    "MMS": (21, (4, 124, 255, 255), (0, 124, 255, 255), (0, 0, 0, 0)),
    "BROWSER": (21, (0, 189, 251, 255), (0, 174, 244, 255), (0, 0, 0, 0)),
}

_SIG = b"\x89PNG\r\n\x1a\n"


def png_decode_rgba8(data: bytes) -> tuple[int, int, bytes]:
    """stdlib 极简 PNG 解码器：仅 RGBA8（bitdepth=8, colortype=6），逐行反滤波。

    在册图标全为此形（生成器 save 无参 = PIL 默认 RGBA8）。任何签名/IHDR/
    IDAT/zlib/尺寸/行数据破坏 raise ValueError。
    """
    if data[:8] != _SIG:
        raise ValueError("坏 PNG 签名")
    pos, idat, w, h, first = 8, b"", None, None, True
    while pos + 8 <= len(data):
        ln, typ = struct.unpack(">I4s", data[pos : pos + 8])
        pos += 8
        chunk = data[pos : pos + ln]
        if len(chunk) != ln:
            raise ValueError("chunk 截断")
        pos += ln + 4  # CRC 不校验（消费面是像素不是完整性——zlib 已兜底）
        if first and typ != b"IHDR":
            raise ValueError(f"首 chunk 非 IHDR: {typ!r}")
        first = False
        if typ == b"IHDR":
            if len(chunk) < 10:
                raise ValueError("IHDR 短")
            w, h, bd, ct = struct.unpack(">IIBB", chunk[:10])
            if (bd, ct) != (8, 6):
                raise ValueError(f"非 RGBA8 形: bitdepth={bd} colortype={ct}")
        elif typ == b"IDAT":
            idat += chunk
        elif typ == b"IEND":
            break
    if w is None or h is None:
        raise ValueError("IHDR 缺席")
    if not idat:
        raise ValueError("IDAT 缺席")
    try:
        raw = zlib.decompress(idat)
    except zlib.error as e:
        raise ValueError(f"zlib 解压失败: {e}") from e
    stride = w * 4
    if len(raw) != (stride + 1) * h:
        raise ValueError(f"行数据账面不符: {len(raw)} ≠ {(stride + 1) * h}")
    out = bytearray()
    prev = bytearray(stride)
    for y in range(h):
        ft = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1 : (y + 1) * (stride + 1)])
        if ft == 0:
            pass
        elif ft == 1:
            for i in range(4, stride):
                line[i] = (line[i] + line[i - 4]) & 0xFF
        elif ft == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif ft == 3:
            for i in range(stride):
                a = line[i - 4] if i >= 4 else 0
                line[i] = (line[i] + (a + prev[i]) // 2) & 0xFF
        elif ft == 4:
            for i in range(stride):
                a = line[i - 4] if i >= 4 else 0
                b = prev[i]
                c = prev[i - 4] if i >= 4 else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 0xFF
        else:
            raise ValueError(f"坏滤波型 {ft}")
        out += line
        prev = line
    return w, h, bytes(out)


def _rgba(px: bytes, i: int) -> tuple[int, int, int, int]:
    return (px[i * 4], px[i * 4 + 1], px[i * 4 + 2], px[i * 4 + 3])


def selftest() -> int:
    apps = list(APPS)
    px_map: dict[str, bytes] = {}

    # —— 结构钉板 + 消费契约（零 PIL）——
    for app in apps:
        w, h, px = png_decode_rgba8((OUT_ROOT / app / "icons" / "icon.png").read_bytes())
        assert (w, h) == (S, S), f"{app}: 尺寸 {w}×{h} ≠ {S}×{S}"
        px_map[app] = px
        # 契约 1：BYI1 1-bit alpha 前置——alpha 只许 {0,255}
        bad_a = {a for a in px[3::4] if a not in (0, 255)}
        assert not bad_a, f"{app}: alpha 非二值 {sorted(bad_a)[:4]}（BYI1 消费面破坏）"
        # 契约 2：EXACT 族 6-bit 量化网格（rgb ∈ step=4 网格或 255 端点；alpha 已二值）
        if app in EXACT_ICONS:
            off = [v for i, v in enumerate(px) if i % 4 != 3 and v != 255 and v % 4 != 0]
            assert not off, f"{app}: 量化网格破坏 {len(off)} 字节（首例 {off[0]}）"
        # 账面：不透明像素数
        opaque = sum(1 for a in px[3::4] if a == 255)
        assert opaque == OPAQUE_COUNTS[app], (
            f"{app}: 不透明像素 {opaque} ≠ {OPAQUE_COUNTS[app]}（掩膜/资产换脸）"
        )
        # 锚点 ×3
        fo, a_first, a_mid, a_last = ANCHORS[app]
        assert _rgba(px, fo) == a_first, f"{app}: 首不透明锚点 px[{fo}] 漂移"
        assert _rgba(px, S * S // 2) == a_mid, f"{app}: 中心锚点漂移"
        assert _rgba(px, S * S - 1) == a_last, f"{app}: 末像素锚点漂移"
        # 黄金像素哈希（像素级——编码器字节漂移不误伤）
        got = hashlib.sha256(px).hexdigest()
        assert got == GOLDEN_PX_SHA256[app], (
            f"{app}: 黄金像素哈希漂移 {got[:12]}…（源资产/掩膜/glyph/配色/量化语义分叉）"
            "——重产: python3 tools/genohicons.py，有意识核对后更新 GOLDEN_PX_SHA256"
        )

    # —— 8 拒面：解码器严格拒绝各种字节级破坏 ——
    good = (OUT_ROOT / "SETTINGS" / "icons" / "icon.png").read_bytes()
    ihdr_off = good.index(b"IHDR") - 4
    idat_off = good.index(b"IDAT") - 4

    def rej(data, name):
        try:
            png_decode_rgba8(data)
        except ValueError:
            return
        raise AssertionError(f"拒面失守: {name}")

    rej(b"\x89PNX\r\n\x1a\n" + good[8:], "坏签名")
    rej(good[:ihdr_off] + struct.pack(">I", 13) + b"IHDR" + good[ihdr_off + 12 :], "IHDR 长度字段坏")
    rej(good[:idat_off] + good[idat_off + 25 :], "IHDR chunk 整体摘除（首 chunk 非 IHDR）")
    rej(good[: idat_off + 8] + b"\x00" + good[idat_off + 9 :], "IDAT 首字节腐蚀（zlib 头坏）")
    rej(good[: len(good) // 2] + good[len(good) // 2 + 40 :], "中部截断 40B")
    rej(
        good[: ihdr_off + 8] + struct.pack(">IIBB", S, S, 8, 2) + good[ihdr_off + 18 :],
        "colortype=2（非 RGBA8）",
    )
    rej(
        good[: ihdr_off + 8] + struct.pack(">IIBB", 107, S, 8, 6) + good[ihdr_off + 18 :],
        "宽 107（尺寸漂移 + 行账面炸）",
    )
    rej(good[: idat_off + 8 + 100] + good[idat_off + 8 + 200 :], "IDAT 腐蚀挖洞 100B（zlib 流断）")
    rej(good[: len(good) // 2], "尾部截断（IDAT 半途而废）")

    # —— PIL 面（在位才跑；缺席打 skip，黄金哈希已兜底） ——
    pil_note = "skip（PIL 缺席——黄金哈希兜底）"
    if Image is not None:
        # gradient_tile 纯函数向量：角恒等 / 对角对称 / 单调 / 合成向量
        t = gradient_tile(10, 8, (31, 143, 255), (0, 199, 190))
        assert t.size == (10, 8) and t.mode == "RGB"
        assert t.getpixel((0, 0)) == (31, 143, 255), "gradient 角 (0,0) ≠ c0"
        assert t.getpixel((9, 7)) == (0, 199, 190), "gradient 角 (w-1,h-1) ≠ c1"
        assert t.getpixel((9, 0)) == t.getpixel((2, 7)), "gradient 等参数线对称性破坏（x+y 同和）"
        row = [t.getpixel((x, 0))[0] for x in range(10)]
        assert all(row[i] >= row[i + 1] for i in range(9)), f"gradient 单调性破坏: {row}"
        t2 = gradient_tile(2, 2, (100, 60, 200), (20, 180, 40))
        assert t2.tobytes() == bytes(
            (100, 60, 200, 60, 120, 120, 60, 120, 120, 20, 180, 40)
        ), f"gradient 2×2 合成向量漂移: {t2.tobytes().hex()}"
        # 量化映射向量（round 银行家舍入 + 255 端点收缩）
        step = 255 // 63

        def _quant(v: int) -> int:
            return round(v / step) * step  # noqa: E501 —— 与生成路径同式

        q = Image.new("L", (10, 1))
        q.putdata([0, 2, 125, 126, 127, 128, 252, 253, 254, 255])
        qv = list(q.point(_quant).tobytes())
        assert qv == [0, 0, 124, 128, 128, 128, 252, 252, 255, 255], f"量化映射漂移: {qv}"
        # 重产像素对拍 ×7（encoder 字节不比——合法漂移面，像素守卫已是黄金哈希）
        import tempfile

        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            o_out, o_repo = OUT_ROOT, REPO
            globals()["OUT_ROOT"], globals()["REPO"] = tmp, tmp
            try:
                for app, spec in APPS.items():
                    make_icon(app, *spec)
            finally:
                globals()["OUT_ROOT"], globals()["REPO"] = o_out, o_repo
            for app in apps:
                w, h, regen = png_decode_rgba8((tmp / app / "icons" / "icon.png").read_bytes())
                assert (w, h, regen) == (S, S, px_map[app]), (
                    f"{app}: 重产像素对拍 FAIL（源资产/生成器语义分叉）——"
                    "重产: python3 tools/genohicons.py 并随源入册"
                )
        pil_note = "gradient/quant 向量 + 重产像素对拍 7/7"

    print(
        f"genohicons: selftest ok (7 图标结构钉板 + 契约（alpha 二值 + EXACT 量化网格 + "
        f"不透明账面）+ 黄金像素哈希 ×7 + 锚点 ×3 + 拒面 8；PIL 面 {pil_note})"
    )
    return 0


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--selftest":
        return selftest()
    for app, spec in APPS.items():
        make_icon(app, *spec)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
