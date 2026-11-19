# GUI swipe precise damage and completion reclaim

Status: implemented

## Problem

双缓冲异步 present 后，真实 launcher 横滑仍出现 `area=2,073,600` 的单整窗 rect。workflow 审计确认 wmd 并未放大 damage：整窗在 launcher/BerryUI 侧已经产生。四个放大器叠加：输入批在 `pump_input` 前调用 `wake`，上一帧 render 已排空 damage，wake 的“空集置 FULL”吞掉随后生成的行带；两页各 5 条行带超过 `DAMAGE_MAX=8`；同一输入批多次 `set_offset_sparse` 重复入集；翻页时全部圆点无条件重摆。呈现侧还有两个结构性成本：wmd 对 SYSTEM partial 帧整屏 front→back 继承，vgpud alias 只有 RELEASE 才回收，launcher 回调后 RELEASE 会排干刚保留的新帧。

## Decision

保持 present wire、credits 上限、damage 帽、普通 app 能力与 1080×1920 几何不变，只做精确化与内部流水化：

- launcher 输入批在消息分发后按需 wake；无 damage 的批零渲染零提交，boot 起振与合成注入路径不变。
- BerryUI `DamageSet::add_row_band` 按 y 与 bottom 各 ≤2px 合并行带；Grid 页行带跨页、同帧多次 offset 写入共用该合并，超过 8 条独立带仍保守 FULL，不漏画。
- SwipePager 只更新前后两个圆点；同页回弹/HOME 归零零写。
- launcher 过拍 snap 用推进后的单一时间戳 tick，不再双 tick 造成重复 damage 和动画时间回摆。
- wmd `Compositor` 维护 `back_stale`：back 相对 front 只在该域陈旧，partial 继承只拷该域；SYSTEM FULL 回复后立即预取 back，避免 HOME 后第一帧横滑先付整屏继承。APP/window 仍排干后写权威 front。
- vgpud `PendingFlip` 关联 held alias；DMA 完成即关闭 alias并回收 credit。boot 同步首帧内联回收，RELEASE 请求形仍为 RETURN-ALL，只归还剩余 alias，常态 count=0。
- launcher 在跟手拖动与 snap 吸附期间保留一帧在途；下一输入/vsync 先 raster back，再 SUBMIT 并回调上一帧。静止 Settle 才排干末帧。`complete_front_frame` 不再无条件 RELEASE，避免排干新帧；guifbd 后端由 ST_FULL 恢复路径释放旧 alias。
- vgpud damage plan 增加“每额外 rect 固定命令成本”模型：`sum_area + (n-1)×screen_area/12` 超过 union 时改传包络。1080p 横滑 5～8 行带会合并为约 1.16～1.37MPix 的单包络。
- `desktop-calib` 以 submit 序号 ↔ DMA token 精确切片手势窗口，输出首帧、逐帧 area/rects/DMA、帧距、排水状态与多次 run 中位数/最差值。
- 对抗审计后补上两个像素边界：后端 damage plan 并入 `back_stale` 继承域，避免 vgpud 持久 resource 保留上一代窄条；owner replay 对全部窗口取单调包络，不再在 `WM_WINDOWS_MAX>DAMAGE_MAX` 时丢前 8 个窗口。SYSTEM FULL 预取按实际写入域是否覆盖整屏触发，不依赖 wire FULL 位。

## Alternatives considered

- 继续按 rect 面积和择优：拒绝；1080p 实测多条行带的固定 virtio-gpu/QEMU 调度成本可超过单包络。
- 提高 `CREDITS_MAX` 或给三帧 VMO：本轮拒绝；在 alias 完成时回收后，两帧 VMO 已可保留一帧在途，扩大 credits 增加内存与状态面而无必要。
- 把 RELEASE 改成按 id 归还：拒绝；改变 present wire 请求形。
- APP window-local FULL 写 back：拒绝；GUI8 vgpu 已实测半屏窗整面变黑。
- 宿主假动画或降低分辨率：拒绝；破坏真值与产品裁决。

## Consequences

- 最终三次真实手势：frames median 4、min 3；首帧中位 0.886s；submit 最坏 258µs；逐帧 DMA 中位 137,296µs；所有 token 排水完成。单次 DMA 最坏 174,940µs。横滑帧不再出现 2,073,600px，稳定在约 1.16～1.37MPix 包络。动画总 span 中位 1.899s，因为包含比一帧跳变更多的真实帧，不能与旧 0.891s 当同类墙钟直接比较。
- 横滑 transfer 从 2,073,600px/1rect 降为约 1.16～1.37MPix/1rect；`reattaches=3` 稳定。仍不得宣称 60fps 或连续流畅动画终态。
- BerryUI 337/337、berry-guiproto 73/73、gui-wmd host 8/8、berry host 18/18、`just check`、产品 Recents/frame-cache e2e、GUI8 vgpu `--only-a`、`smoke-image` 147/147 全绿。
- gfxdemo 对 RELEASE count 放宽为 count==handle 数，兼容 vgpud 完成即回收与 guifbd 延迟归还两种后端。

## Related

- [GUI double-buffered present pipeline](2026-11-14-gui-present-pipeline.md) established the two-frame async base; this note removes the damage amplifiers and credit drain that limited it.
- [GUI P2 swipe groundwork](2026-11-14-gui-p2-swipe-groundwork.md) defined the one-frame/full-transfer baseline and rejected scanout pan.
