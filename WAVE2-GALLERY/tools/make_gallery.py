#!/usr/bin/env python3
"""WAVE2-GALLERY PNG 联络表拼版（源 ../ui-wave2 波 2 验收工作树
tools/make_gallery.py 的主树移植版——宿主工具转 PPM→PNG）。

前置：`cargo test --target aarch64-apple-darwin -p berry-ui --test gallery2`
已在 `WAVE2-GALLERY/ppm/` 落 43 格 PPM 与 `cells.tsv`（格序单源）。

产出：`WAVE2-GALLERY/gallery.png`——单张网格大图（41 kind 全量格 +
pattern.rs 值面格各一格 + 标签行）。零仓内依赖（宿主 python3 + Pillow）。
"""

import csv
import pathlib
import sys

from PIL import Image, ImageDraw

HERE = pathlib.Path(__file__).resolve().parent.parent  # WAVE2-GALLERY/
COLS = 5
CELL_W = 460          # 格逻辑宽（PPM 实际宽同源）
LABEL_H = 30          # 标签行
PAD = 14              # 格间留白
BG = (250, 250, 252)
FRAME = (200, 203, 210)
INK = (24, 36, 49)


def main() -> int:
    tsv = HERE / "cells.tsv"
    rows = list(csv.DictReader(tsv.open(), delimiter="\t"))
    if not rows:
        print("cells.tsv 空——先跑 cargo test --test gallery2", file=sys.stderr)
        return 1
    cells = []
    for r in rows:
        ppm = HERE / "ppm" / f"{r['name']}.ppm"
        im = Image.open(ppm)
        assert (im.width, im.height) == (int(r["w"]), int(r["h"])), r["name"]
        cells.append((r["name"], r["label"], im))

    # 行分组（按 cells.tsv 序 = 测试格序）
    rows_grouped = [cells[i : i + COLS] for i in range(0, len(cells), COLS)]
    sheet_w = PAD + COLS * (CELL_W + PAD)
    sheet_h = PAD + sum(LABEL_H + max(im.height for _, _, im in g) + PAD for g in rows_grouped)
    sheet = Image.new("RGB", (sheet_w, sheet_h), BG)
    draw = ImageDraw.Draw(sheet)

    y = PAD
    for group in rows_grouped:
        row_h = max(im.height for _, _, im in group)
        for i, (name, label, im) in enumerate(group):
            x = PAD + i * (CELL_W + PAD)
            draw.rectangle([x - 1, y - 1, x + im.width, y + LABEL_H + row_h], outline=FRAME)
            sheet.paste(im, (x, y + LABEL_H))
            draw.text((x + 4, y + 7), f"{name}  |  {label}", fill=INK)
        y += LABEL_H + row_h + PAD

    out = HERE / "gallery.png"
    sheet.save(out, optimize=True)
    print(f"{out}  {sheet.width}x{sheet.height}  {len(cells)} cells")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
