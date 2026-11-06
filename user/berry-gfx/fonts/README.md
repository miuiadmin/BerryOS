# HD 字库源

- `NotoSansCJKsc-Regular-ASCII.otf`：Noto Sans CJK SC Regular 的 ASCII 32..126 子集，SIL OFL 1.1。
- 来源：`https://github.com/notofonts/noto-cjk` @ `f8d157532fbfaeda587e826d4cd5b21a49186f7c`。
- 用途：仅宿主构建期生成 ASCII 32..126 的 32×48 单色点阵；设备不链接字体引擎。
- 再生成：`python3 tools/genhdfont.py user/berry-gfx/fonts/NotoSansCJKsc-Regular-ASCII.otf`。
- 许可：见同目录 `OFL.txt`；生成物为该字体的位图子集，随源遵守 OFL 1.1。
