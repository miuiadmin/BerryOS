# OpenHarmony 状态栏 / dock 资产源

- `signal.svg` / `wifi.svg`：来自 `openharmony/applications_systemui`
  @ `413f701448a7923a18a761e3cb54f0b871cfef40`，Apache-2.0。
  源路径见 `docs/第三方资产清单-v1.0.md`。
- `battery.svg`：BerryOS 按 OpenHarmony `batteryPic` 官方几何复刻
  （body/gap/head/border/padding 尺寸，3x 折算），非复制商业资源。
- `*.png`：源 SVG 的宿主光栅化中间件；`tools/genohstatus.py` 生成
  launcher 内建 1-bit 状态图标与 BYI1 dock 分段；dock 源条 668×156。
- 再生成顺序：先 `python3 tools/genohicons.py` 刷新 app 源图标，再
  `python3 tools/genohstatus.py` 把这些图标嵌入 dock 分段。
- dock 背板：按 `applications_launcher` PhoneSmartDock `calculateDock`/PhoneSmartDock 官方几何复刻：源条 668×156、radius44、
  4 位、108px 图标，运行期 1.5× 采样到 1002×234；四段拆分保留每 app
  独立命中面。
