# GUI double-buffered present pipeline

Status: implemented

## Problem

P2 之后 1080×1920 桌面横滑仍常表现为一帧跳动。`desktop-calib` 已证明单个真实手势只有 1 个新 `VGPUPERF` 帧，整窗 `area=2,073,600` 的 virtio-gpu transfer 约 107～175ms；launcher 在 raster 后同步等待本帧 DMA，wmd 也只有一层 composition VMO，无法在旧帧上呈现时准备下一帧。QEMU 11.0.3/HVF 已实测拒绝非零子矩形 `SET_SCANOUT`，host-side pan 不能作为下一步。

## Decision

保持 present wire、credits、damage 帽、alias 生命周期与普通 app 能力零改动，只在显示服务内部流水化：

- vgpud 维护最多 2 个 `PendingFlip`。boot 首个 SUBMIT 仍在 flush 完成后 ACK，保证启动真值；运行期 SUBMIT 校验并入队后早回 token，serve loop 后续对队首执行 `SET_SCANOUT→TRANSFER_TO_HOST_2D→RESOURCE_FLUSH`。`FRAME_CALLBACK` 按既有语义阻塞到 token 完成，`RELEASE` 先排干 pending 再归还 alias。`VGPUPERF last_us` 只表示入队/校验，新增 `dma_us` 与 `VGPUDMA token/us` 单独记录真实 DMA。
- gui-wmd 保留两个全屏 composition VMO，但双缓冲快路径只用于全屏 SYSTEM 合成：FULL 写 back，partial 先继承 last submitted 的未变像素。APP/window submit 不是滑动快路径，先排干 present credits 再原子更新权威 front buffer；GUI8 对拍证明若 APP FULL 也走 back，半屏窗会在交替 composition 中丢失像素。owner frame cache 捕获/回放都指向最新已提交 composition，不读取正在写的 back。
- berry-launcher 持有两个 frame VMO（`0x5100_0000` / `0x5300_0000`，壁纸 cache 已占 `0x5200_0000`）。运行期顺序为 raster back → SUBMIT back → 回调上一帧；动画停止的 Settle 相完成并释放最后一帧。Opening 后的 `APP_SPAWN` 和真实 `SWIPE ok` 标记绑定到具体 frame token 的完成回执，不再用 presented 计数提前宣布。
- 空桌面 HOME 即使 WmTable version 不变也向 SYSTEM launcher 发送 HOME action；launcher `snap_to(page 0)` 并全屏重绘，保证 HOME 是规范桌面态而不是仅“无 APP 窗”。`desktop-calib` 用真实 HOME 复位，并按 `VGPUDMA` 新行单独归因本手势 DMA。

## Alternatives considered

- QEMU 2D 非零子矩形 scanout/host-side pan：实测 `SET_SCANOUT(x=1,y=0,w=1,h=2)` 返回 `0x1205`，拒绝。
- 降低 1080×1920 或改回 800×600：用户明确裁决 1080p 是初始产品形态，拒绝。
- 让普通 app 直接获得 present/HDI 或为 launcher 开 surface escape：破坏 Berry Room 与显示服务权威，拒绝。
- 用 wall-clock 插值或 host 假动画补帧：破坏启动/交互真值，拒绝。
- 让 APP window-local FULL 也写 back：GUI8 vgpu 实测左半窗整面变黑、R2 damage 像素失败；APP 更新必须排干后写权威 front。
- 宿主直接消费 shared composition/shm-ring：保留为后续方向；本刀先用零 wire 改动把现有链路流水化。

## Consequences

- 最终源码产品标定（Apple Silicon/HVF/1080×1920/virtio-gpu）：同一真实 launcher 横滑从基线 `1 frame / last_us≈154,991` 变为 `2 frames / 0.891s / submit≈257µs / DMA≈162,597µs`。`reattaches=3` 稳定，backing 池无抖动；仍不得宣称 60fps 或连续流畅动画。
- `VGPUPERF last_us` 与 DMA 分离后，短 ACK 不再被误读为呈现完成；DMA 波动（约 98µs 小损伤、100～190ms 整窗）由 `VGPUDMA` 独立记录。
- 双 frame VMO + 双 composition VMO 增加 1080p 下约 16MiB committed 内存；产品 256MiB 实测通过。Recents/frame-cache、Boot Progress、输入与 Berry Room 边界不变。
- 空桌面 HOME 的页复位行为被产品校准覆盖；横滑后 HOME → page0 → BerryDemo 启动链不再依赖第二个反向合成滑动。最终回归：产品 Recents/frame-cache e2e、GUI8 vgpu `--only-a`、`smoke-image` 147/147、gui-wmd host 8/8、BerryUI 334/334、berry host 18/18、`just check` 全绿。

## Related

- [GUI P2 swipe groundwork](2026-11-14-gui-p2-swipe-groundwork.md) established the one-frame baseline and the rejected scanout-pan branch; this note implements the double-buffer/async-present branch it left open.
- [Recents owner frame cache](2026-11-14-recents-owner-frame-cache.md) remains the owner-pixel truth source; this pipeline changes only which full composition VMO is submitted.
