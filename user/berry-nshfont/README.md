# berry-nshfont

Native Shell 系统 HD 字形与 UI 基元：

- wmd 为每个 APP `WM_CONNECT` 创建私有 32×48 ASCII A8 字形 VMO；
- `map` 固定映射到 `0x5300_0000`；
- `text` 做 straight-alpha 抗锯齿文本；
- `round_rect` / `circle` 做 3×3 超采样抗锯齿几何；
- `shadow` 提供轻量卡片阴影。

v1 `vmar_map` 只有 UserRw，因此字体 VMO 必须 per-APP 私有，防止一个
app 写穿破坏其它 app 字体。
