# GUI P2 滑动减载与量化边界

Status: implemented

## Problem

P1 之后用户仍会看到 1080×1920 桌面横滑一顿一顿。已有 `desktop-calib` 只测 BerryDemo 的 page/back/switch 墙钟，没有直接度量 launcher 横滑的帧数与 vgpu submit；宿主 VNC 每个更新还会 clone 一份 8.3MiB 帧；launcher 每个动画帧重复对 2.07M 像素壁纸做双线性；稀疏 damage 在 Grid 页上容易按 1.25 浪费帽合并回整页包络。

## Decision

先落可验证的 P2 地基，不冒充终态：

- `tools/desktop-calib.py` 新增真实 virtio-tablet 横滑：等待 boot 自测 VGPUPERF 静止后注入左滑并回滑复位，记录新 VGPUPERF 帧数、动作时长与该帧 submit `last_us`。`VGPUPERF` 增加 `rects/area/union`，把“传了多大”变成串口真值。
- BerryUI 对 SwipePager 的 `Col→Grid→cells` 页生成 ≤8 条精确行带，`DamageSet::add_exact` 绕过普通 1.25 合并帽；非该形态回落原 sparse 包络。
- vgpud 为稀疏 plan 增加 batch 控制队列：≤8 个 `TRANSFER_TO_HOST_2D` 一次 doorbell/等待，再对并集做一次 `RESOURCE_FLUSH`，不再逐 rect 乘固定往返。
- launcher 把官方 RGB565 壁纸按当前视口一次性双线性预缩放进独立 committed VMO（1080p 约 8.3MiB），IconSource 后续供不透明 ARGB；分配/映射失败时回退原 SmoothRgb565，不崩不伪造。
- berry host 的 VNC Reader 改为 staging frame + shared `Arc<Mutex<Frame>>`：解码期间窗口继续画旧帧，提交时按 VNC damage 行拷贝，不再每帧 clone 全帧像素。

## Alternatives considered

- 直接用 `Cmd::Blit` 平移合成结果：拒绝；launcher 页透明，平移会把壁纸/旧底拖影，且 present wire 无 scroll 语义，vgpu 字节不降。
- 用 8.3MiB heap Vec 存壁纸缓存：实测 launcher `memory allocation of 8294400 bytes failed` 并重启；改为 kernel VMO + VMAR 映射。
- 把 `MAX_NODES` 或 a11y 表继续扩容：P1/Recents 调试已证明满帽来自生命周期而非容量；不重复掩盖。
- 宿主 CopyRect/Tight 解码与 host-side pan：列为后续方案；未先验证 QEMU `SET_SCANOUT` 子矩形行为前不落地。

## Consequences

- BerryUI 全套、berry host 18/18、`just check`、产品 Recents e2e、`smoke-image` 147/147、GUI8 vgpu `--only-a` 全绿；1080×1920、HVF、virtio-gpu 与 Berry Room 边界不变。
- 本轮产品 `VGPUPERF` 仍可见整窗 `area=2,073,600`、约 107～175ms、真实横滑帧数仍为 1。结论必须写成“P2 地基与量化边界”，不能宣称 60fps、连续动画或多帧横滑已解决。
- 下一步优先验证 QEMU 2D `SET_SCANOUT` 非零子矩形行为；若可用，走显示服务内部宽 resource/host-side pan，把每帧 8.3MiB transfer 降到 chrome 小矩形。否则做双缓冲/异步 present 流水线。
- 后续实测（未改变产品代码）：QEMU 11.0.3/HVF 对 2×2 resource 的 `SET_SCANOUT(x=1,y=0,w=1,h=2)` 返回 `0x1205` invalid parameter，`(0,0,2,2)` 正常。当前 QEMU 2D 后端不能作为 host-side pan 基础。

## Related

- [GUI interaction quality P1](2026-11-13-gui-interaction-quality-phase1.md) established sparse damage, bulk copy, and VNC fast decode; this note adds direct launcher swipe measurement and the first P2 caching/batching layer.
