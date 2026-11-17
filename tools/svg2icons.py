#!/usr/bin/env python3
"""SVG → ARGB 位图库生成器（genohstatus.py 先例的批量版）。

源资产（均 Apache-2.0，fetch_material.py / 稀疏 clone 收敛到 /tmp/icons-src）：
- material/*.svg —— Material Icons outlined/filled 24px 源（名 md_<cat>_<name>）；
- oh-sysres/systemres/main/resources/base/media/*.svg —— OH sysResource（名 oh_<stem>）；
- oh-systemui 导航栏/信号/WiFi 组件 media/*.svg —— OH systemui（名 oh_<stem>）。

管线：resvg 光栅化 66×66（透明底）→ PIL 读 RGBA → alpha≥128 二值化
（前景 0xFFF0F0F4 / 背景 0x00000000 全透明）→ ARGB u32 数组。

产物：user/berry-appshell/src/icon_data/<组名>.rs ×3 + mod.rs
（ICON_DATA: &[(&str, &[u32], u32, u32)] 全量注册表——icons::bitmap_id 消费）。

用法：
  python3 tools/svg2icons.py            # 重产 icon_data/（硬依赖 resvg + PIL）
  python3 tools/svg2icons.py --selftest # 结构钉板自检（零依赖面）
"""
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SRC = Path('/tmp/icons-src')
OUT = REPO / 'user/berry-appshell/src/icon_data'
RESVG = '/opt/homebrew/bin/resvg'
SIZE = 66
FG = 0xFFF0F0F4
SYSTEMUI_DIRS = [
    'product/default/navigationBar/src/main/resources/base/media',
    'features/signalcomponent/src/main/resources/base/media',
    'features/wificomponent/src/main/resources/base/media',
]
# 溯源钉：源目录 → (组名, 命名前缀)。名字域唯一性由生成器全局查重执法。
GROUPS = [
    ('material', [SRC / 'material'], 'md_'),
    ('oh_sysres', [SRC / 'oh-sysres/systemres/main/resources/base/media'], 'oh_'),
    ('oh_systemui', [SRC / 'oh-systemui' / d for d in SYSTEMUI_DIRS], 'oh_'),
]


def icon_names(group: str, dirs, prefix: str):
    """(图标名, Rust 数组名, svg 路径) 迭代——文件名序确定。"""
    out = []
    for d in dirs:
        for f in sorted(d.glob('*.svg')):
            # 名字域统一小写（bitmap_id 查询面）；Rust 标识符由名字派生。
            name = (prefix + f.stem).lower()
            ident = re.sub(r'[^A-Za-z0-9]', '_', name).upper()
            out.append((name, ident, f))
    return out


def rasterize(svg: Path, png: Path) -> bool:
    png.parent.mkdir(parents=True, exist_ok=True)
    r = subprocess.run(
        [RESVG, '--width', str(SIZE), '--height', str(SIZE), str(svg), str(png)],
        capture_output=True,
    )
    return r.returncode == 0 and png.exists()


def px_array(svg: Path, cache: Path):
    """SVG → [u32; 66*66]（alpha 二值单色面）；空面/渲染失败 → None。"""
    # PIL 惰性导入：selftest/结构钉板路径零 PIL 依赖（CI runner 无 Pillow，
    # 顶层导入曾使 --selftest 即 ModuleNotFoundError 堵全部 PR light 门——
    # #406 遗留 base 债，本刀随收口刀清偿）。
    from PIL import Image
    if not (cache.exists() and cache.stat().st_size > 0):
        if not rasterize(svg, cache):
            return None
    im = Image.open(cache).convert('RGBA').resize((SIZE, SIZE))
    px = []
    for y in range(SIZE):
        for x in range(SIZE):
            _, _, _, a = im.getpixel((x, y))
            px.append(FG if a >= 128 else 0)
    if not any(px):
        return None  # 全透明 = 渲染空面（源不兼容/空 svg）——不入库
    return px


def emit_group(name: str, entries) -> str:
    lines = [
        f'//! 由 tools/svg2icons.py 生成——勿手改。',
        f'//! {name} 组：SVG 源 → resvg 66×66 → alpha≥128 二值 ARGB（前景 0xFFF0F0F4）。',
        '',
    ]
    for ident, px in entries:
        lines.append(f'pub const {ident}: [u32; {SIZE * SIZE}] = [')
        for i in range(0, len(px), 12):
            lines.append('    ' + ', '.join('0x%08X' % v for v in px[i:i + 12]) + ',')
        lines += ['];', '']
    return '\n'.join(lines)


def build():
    OUT.mkdir(parents=True, exist_ok=True)
    cache_root = SRC / 'raster'
    total, skipped, seen = 0, [], {}
    registry = []  # (name, group, ident)
    for gname, dirs, prefix in GROUPS:
        entries, group_reg = [], []
        for name, ident, svg in icon_names(gname, dirs, prefix):
            if name in seen:
                skipped.append(f'{name}（与 {seen[name]} 重名）')
                continue
            px = px_array(svg, cache_root / f'{gname}_{ident}.png')
            if px is None:
                skipped.append(f'{name}（渲染空面/失败）')
                continue
            seen[name] = gname
            entries.append((ident, px))
            group_reg.append((name, ident))
            total += 1
        (OUT / f'{gname}.rs').write_text(emit_group(gname, entries), encoding='utf-8')
        registry.append((gname, group_reg))
        print(f'[group {gname}] icons {len(entries)} skipped {len([s for s in skipped if gname in s])}')
    # mod.rs：全量注册表（名字域全库查重后唯一）
    mod = [
        '//! 由 tools/svg2icons.py 生成——勿手改。',
        '//! 位图库全量注册表（Apache-2.0：Material Icons + OH sysResource +',
        '//! OH systemui；66×66 二值 ARGB 面）。消费面：icons::bitmap_id(name)',
        '//! → BITMAP_ID_BASE 起稠密 id 域（与 app 稠密域/WALLPAPER 特例正交）。',
        '',
    ]
    for gname, modname in (('Material Icons', 'material'), ('OH sysResource', 'oh_sysres'), ('OH systemui', 'oh_systemui')):
        mod.append(f'pub mod {modname};')
    mod += [
        '',
        '/// (name, px, w, h) 全量位图注册表。',
        'pub const ICON_DATA: &[(&str, &[u32], u32, u32)] = &[',
    ]
    for gname, group_reg in registry:
        for name, ident in group_reg:
            mod.append(f'    ("{name}", &{gname}::{ident}, {SIZE}, {SIZE}),')
    mod += ['];', '']
    (OUT / 'mod.rs').write_text('\n'.join(mod), encoding='utf-8')
    print(f'svg2icons: total {total} icons, skipped {len(skipped)}')
    for s in skipped:
        print(f'  skip {s}')
    return total, skipped


# ---- selftest：结构钉板（零渲染依赖；PIL 在位时加做产物↔注册表一致性） ----

def selftest() -> int:
    art = (OUT / 'mod.rs').read_text(encoding='utf-8')
    assert '由 tools/svg2icons.py 生成' in art, 'mod.rs 生成器头注缺席'
    rows = re.findall(r'^    \("([a-z0-9_]+)", &(\w+)::(\w+), (\d+), (\d+)\),$', art, re.M)
    assert rows, 'ICON_DATA 注册表为空'
    names = [r[0] for r in rows]
    assert len(names) == len(set(names)), 'ICON_DATA 名字域有重复'
    for _, g, ident, w, h in rows:
        assert (int(w), int(h)) == (SIZE, SIZE), f'{g}::{ident} 尺寸漂移 {w}x{h}'
        src = (OUT / f'{g}.rs').read_text(encoding='utf-8')
        assert f'pub const {ident}: [u32; {SIZE * SIZE}] = [' in src, f'{g}::{ident} 数组缺席'
    print(f'svg2icons: selftest ok ({len(rows)} 枚注册，尺寸/名字域/数组在席钉板)')
    return 0


if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == '--selftest':
        raise SystemExit(selftest())
    build()
