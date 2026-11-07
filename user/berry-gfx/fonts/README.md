# HD 字库源

## Noto Sans CJK SC（SIL OFL 1.1）

- `NotoSansCJKsc-Regular-ASCII.otf`：ASCII 32..126 历史子集。
- `NotoSansCJKsc-Regular-Desktop-ZH.otf`：ASCII + 桌面标签历史子集；现行
  `genhdfont.py` 只消费其 ASCII 部分（CJK 已切到全量 A2 页包）。
- 来源：`https://github.com/notofonts/noto-cjk`
  @ `f8d157532fbfaeda587e826d4cd5b21a49186f7c`。
- 全量源 SHA-256：
  `2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b`
 （源仓原文件 16MiB，本目录不重复入库）。
- 用途：宿主构建期生成 32×48 ASCII 位表 + A8，以及 3500 常用 CJK
  A2 分页压缩包；设备不链接字体引擎、不做运行时光栅化。
- 再生成：
  1. 按上表 commit 取 `Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf`；
  2. `python3 tools/genhdfont.py user/berry-gfx/fonts/NotoSansCJKsc-Regular-Desktop-ZH.otf`；
  3. `python3 tools/gencjkfont.py <full-NotoSansCJKsc-Regular.otf>`。
- 许可：见同目录 `OFL.txt`；生成物为该字体位图子集，随源遵守 OFL 1.1。

## CJK 频率字符表（The Unlicense）

- `cjk3500-frequency.txt`：从 `ling0322/webdict`
  @ `00430ba2cde04d19a78b34ddb87da2d931231877` 的
  `webdict_with_freq.txt` 词频加权推导的 top 3500 常用 CJK 集，并强制包含
  官方桌面标签字。
- SHA-256：
  `7bb32dbd458ed06e95236a3d915cd6053170a13ab5005fb0c56a73d1e8d3248a`。
- 许可：见同目录 `webdict-LICENSE`（Unlicense / public domain）。
