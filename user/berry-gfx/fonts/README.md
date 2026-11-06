# HD 字库源

- `NotoSansCJKsc-Regular-ASCII.otf`：Noto Sans CJK SC Regular 的 ASCII 32..126 子集，SIL OFL 1.1。
- `NotoSansCJKsc-Regular-Desktop-ZH.otf`：ASCII 32..126 + 桌面官方中文标签子集（设置/文件/图库/相机/通讯录/信息/浏览器），SIL OFL 1.1。
- 来源：`https://github.com/notofonts/noto-cjk` @ `f8d157532fbfaeda587e826d4cd5b21a49186f7c`。
- 用途：宿主构建期生成 32×48 ASCII 位表 + A8；另为中文标签生成 Unicode A8 子表。设备不链接字体引擎。
- 再生成：`python3 tools/genhdfont.py user/berry-gfx/fonts/NotoSansCJKsc-Regular-Desktop-ZH.otf`。
- 许可：见同目录 `OFL.txt`；生成物为该字体的位图子集，随源遵守 OFL 1.1。
