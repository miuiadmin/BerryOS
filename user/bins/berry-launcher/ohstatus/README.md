# OpenHarmony 状态栏 / dock 资产源

- `signal.svg` / `wifi.svg`：来自 `openharmony/applications_systemui`
  @ `413f701448a7923a18a761e3cb54f0b871cfef40`，Apache-2.0。
  源路径见 `docs/第三方资产清单-v1.0.md`。
- `battery.svg`：BerryOS 按 OpenHarmony `batteryPic` 官方几何复刻
  （body/gap/head/border/padding 尺寸，3x 折算），非复制商业资源。
- 状态布局：`StatusbarConfig.ts` 官方顺序——左 signal/wifi，右
  battery/clock；生成期按 systemui `opacity=0.7` 与默认壁纸预合成。
- dock 常驻顺序：`PhonePresetDockItem.ts` 官方 Contacts→Photos→Settings→MMS。
- dock 背板：PhoneSmartDock `calculateDock` 官方几何——源条 668×156、
  radius44、4 位、108px 图标，运行期 1.5× 采样到 1002×234；背板与默认壁纸
  逐像素预合成，四段拆分保留每 app 独立命中面。
- 再生成顺序：先 `python3 tools/genohicons.py` 刷新 app 源图标，再
  `python3 tools/genohstatus.py` 把这些图标嵌入 dock 分段。
