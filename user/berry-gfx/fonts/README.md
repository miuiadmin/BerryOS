# HD 字库源

- `NotoSans-Regular.ttf`：Noto Sans Regular，SIL Open Font License 1.1。
- 来源：`https://github.com/googlefonts/noto-fonts`（latin-greek-cyrillic 家族）。
- 用途：仅宿主构建期生成 ASCII 32..126 的 32×48 单色点阵；设备不链接字体引擎。
- 再生成：`python3 tools/genhdfont.py user/berry-gfx/fonts/NotoSans-Regular.ttf`。
- 许可：见同目录 `OFL.txt`；生成物为该字体的位图子集，随源遵守 OFL 1.1。
