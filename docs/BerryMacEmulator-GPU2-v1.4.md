# Berry Mac Emulator GPU2（virtio-gpu HDI 显示框架）
Type: standard
Version: 1.4.0
Status: superseded
Superseded-by: [BerryMacEmulator-GPU2-v1.5.md](BerryMacEmulator-GPU2-v1.5.md)
Updated: 2026-11-15

## §1 产品裁决

Mac Apple Silicon 产品入口仍是 `just desktop`，初始几何不变（开机真实进度框架见 [BerryBoot-BootProgress-P1-v1.0.md](BerryBoot-BootProgress-P1-v1.0.md)）：

```text
1080×1920@60
QEMU Cocoa + HVF
virtio-gpu-device,id=vgpu0,xres=1080,yres=1920
ramfb（固件引导/回退显示面）
```

GPU2 不降分辨率、不把产品镜像变成测试镜像、不让普通 BerryUI app 绕过 Berry Room / gui-wmd。本规格取代 [BerryMacEmulator-GPU1-v1.0.md](BerryMacEmulator-GPU1-v1.0.md) 的显示路径现状；HVF/CPU、输入与启动基线仍见 [BerryMacEmulator-HVF1-v1.0.md](BerryMacEmulator-HVF1-v1.0.md)。

## §2 服务装配与几何真值

后端选择仍在 svcmgr 的 present/hdi 服务端点装配层：

```text
vgpud ready
  → svcmgr 把 present/hdi client 端交给 gui-wmd
  → guifbd/ramfb 不起产品显示服务

vgpud absent / ready 失败
  → guifbd 继续持 present/hdi
  → GUI8 验证拓扑零漂
```

`vgpud` 的显示几何来自 virtio-gpu `GET_DISPLAY_INFO` 第一个 enabled mode，不写死产品分辨率。因此同一服务覆盖：

```text
Mac 产品：1080×1920
GUI8 soft↔vgpu 共识拓扑：800×600
```

## §3 present 与 damage 策略

`SUBMIT` 保持 present wire、credits、damage 帽与 alias 生命周期不变。boot 首个 SUBMIT 仍 flush-at-ACK，防止启动里程碑与首帧真值赛跑；运行期 frame VMO 先进入 backing 池，校验并入队后立即 ACK token：

```text
boot 首帧：RESOURCE_ATTACH_BACKING（仅换背）→ SET_SCANOUT → TRANSFER/FLUSH → SUBMIT_ACK
运行期：  RESOURCE_ATTACH_BACKING（仅换背）→ 入队 PendingFlip → SUBMIT_ACK
serve：   SET_SCANOUT → TRANSFER_TO_HOST_2D → RESOURCE_FLUSH → VGPUDMA token/us
```

`FRAME_CALLBACK` 按既有语义阻塞到 token 终态。vgpud 在 pending flip 完成时立即回收对应 alias/credit；`RELEASE` 请求形仍为 RETURN-ALL，先排干剩余 pending 后归还尚未自动回收的 alias，常态 count=0。wmd 保留两个全屏 composition VMO，但双缓冲快路径只用于全屏 SYSTEM 合成：`back_stale` 记录 back 相对 front 的陈旧域，partial 只继承该域；FULL 提交回复后立即预取 back，避免 HOME 后第一帧横滑先付整屏继承。APP/window submit 先排干 credits 再原子更新权威 front，避免 window-local FULL 在交替 composition 中丢失窗外像素。launcher 持有两个 frame VMO，跟手拖动与 snap 吸附期间刻意保留一帧在途：下一输入/vsync 先 raster back，再 SUBMIT 并回调上一帧；静止 Settle 才排干末帧。Opening 后的 APP_SPAWN 与真实 SWIPE 标记绑定具体 frame token。空桌面 HOME 也向 SYSTEM launcher 发送 HOME action 并 snap 回 page 0。

damage 执行面按“字节面积 + 每额外 rect 的固定命令成本”择优：

```text
rect 面积和 + (rect数-1)×screen_area/12 ≤ 包络面积
    → ≤8 个 TRANSFER 一次发布/等待 → 单次并集 FLUSH
否则
    → 单包络 transfer/flush
```

1080p 实测中 5～8 条横滑行带的面积和约 0.43～0.49MPix，但多命令调度成本可超过单包络；固定成本模型会传输约 1.16～1.37MPix 的并集，换来更稳定的完成时间。稀疏小损伤仍保留 rect。控制队列等待先短自旋再 100µs 睡眠。`VGPUPERF last_us` 只表示 SUBMIT 入队/校验耗时；`dma_us` 与 `VGPUDMA token/us` 在 DMA 完成侧记录真实传输。两者分属不同阶段，不可合并读作“呈现延迟”。

## §4 HDI 四 op 契约

vgpud 实现同一 guiproto hdi 域，不再返回 `ST_UNSUPPORTED`：

```text
REG_VBLANK(enable, event)
VBLANK_WAIT
PREPARE(frame VMO, geometry, XRGB8888)
COMMIT(slot, target_ns)
```

语义：

- `REG_VBLANK` 迁入客户端 event，软 vblank 周期 16,666,666ns。
- `VBLANK_WAIT` 单车位 park；到点返回 seq/ts。
- `PREPARE` 校验格式、非零且不越 scanout 的几何、tight stride、VMO 大小；slot VMO 立即入 backing 池。
- `COMMIT` 先记帐；在下一拍或 target 后首拍执行 select resource → 全 slot rect `TRANSFER_TO_HOST_2D` → `RESOURCE_FLUSH`，完成后才释放 fence。
- fence 与 vblank 用户信号位单源为 `gp::HDI_USER_SIGNAL = 1<<32`。
- 服务循环 deadline 为 `min(next vblank, idle cap)`，无请求零主动唤醒。

`target_ns` 是拍点资格，不是忙等 deadline。GUI8 共识镜像的 HDI 自测 VMO 大小按 4KiB 向上取整，兼容 800×600 的非整页线性长度。

## §5 frame backing 池

池容量为 3 个 virtio-gpu resource：

```text
present(wmd composition VMO) + 2 个 HDI slot
```

每个 pool entry 记录：

```text
frame VMO READ pin
entries VMO
resource id
first physical page
last physical page
LRU stamp
```

first/last PA 只是快速指纹；READ pin 才是身份边界：只要 pool entry 存活，内核不能回收或复用该 VMO 已 commit 的页。换背时先 `RESOURCE_DETACH_BACKING`，再重建 mem-entry 链。resource 2/3 尚未 attach 时跳过 detach；QEMU 对空 resource detach 返回 `0x1200`，该错误曾实际杀死 vgpud。

## §6 共识与诊断产链

正式产品镜像和正式验证镜像都不含 `gpuhdi.on`。新增两条显式产链：

```text
just gpu2-vgpu-image
  rootfs.cpio（保留 probe.on）+ gpuhdi.on
  → out/berryos-vgpu.img

just gpu2-hdi-image
  rootfs-product.cpio（去掉 probe.on）+ gpuhdi.on
  → out/berryos-gpuhdi.img
```

`tools/cpio_filter.py` 支持 `--add NAME=FILE`，重名 loud 拒绝；newc 编码仍单源走 `mkcpio.build_archive`。

GUI8 新增 `--vgpu`：

```text
默认 image = out/berryos-vgpu.img
-device virtio-gpu-device,id=vgpu0,xres=800,yres=600
boot 前强制 VGPU ok + VGPUHDI ok
QMP screendump 显式 device=vgpu0
既有 GUI8 像素锚 / damage / occlusion / interaction 全部原样执行
```

多 console 拓扑下 `QmpClient.default_console` 控制 screendump 目标；输入事件仍省 device 键，避免 QEMU 11.0.3 console 查找崩溃。

## §7 Mac 性能定标

`tools/desktop-calib.py` 在同一产品镜像、同一数据盘、同一 QMP 动作序列下比较 vgpu 与 ramfb：

```text
boot → LAUNCH
等待 launcher boot 自测 VGPUPERF 静止
真实 virtio-tablet 横滑 + 回滑复位（记录帧数/时长/submit）
Berry Demo cold launch
detail AppBar back
Home → detail page transition
Switch 状态翻转
```

入口：

```text
just gpu2-desktop-calib
```

产物：

```text
out/desktop-calib.json
out/desktop-calib.md
```

2026-11-13 本机样本（Apple Silicon / HVF / 1080×1920，负载运行，绝对值不进跨机器 CI）：

```text
vgpu   boot 20.523s  launch 3.251s  back 2.241s  page 2.324s  switch 1.759s
ramfb  boot 20.194s  launch 1.660s  back 1.233s  page 0.980s  switch 0.651s
```

`VGPUPERF` 尾部样本显示整窗提交约 114–171ms，小的 damage submit 可到 97µs；首次 backing 建链显著抬高累计均值。damage 包络合并把同类动作的累计均值从约 280ms 降到约 85–98ms。结论：

- virtio-gpu 显示框架正确接通 HDI、pool、damage 与像素链。
- QEMU 2D `TRANSFER_TO_HOST_2D` 的整窗 8.3MiB 传输仍是当前主要瓶颈。
- BerryGFX raster 仍在 guest CPU；本规格不得宣称全 GPU raster。
- ramfb 在部分小交互上仍更快；GPU2 的价值是系统显示契约和后续加速路径，不是伪造性能优势。

### §7.1 Interaction P1（2026-11-13）

本轮落地的是减载与反馈，不宣称 60fps 已解决：

```text
wmd copy:       源行 bulk-read + 目的行按全屏 stride 写
host VNC:       32bpp byte-channel fast path
BerryUI swipe:  sparse painted-leaf damage（按直接 page child 聚合）
inputd:         空闲首 harvest 100ms → 16ms
launcher:       Opening/pressed 反馈帧先于阻塞 APP_SPAWN
```

本机单核样本仍显示 vgpu 整窗 submit 约 90～163ms；`page/back` 墙钟受宿主负载与动作标记影响波动，不能拿单次值宣称优化终点。SMP 试跑（G2 内核、`-smp 2`）未改善 GUI 动作，暂不改变产品默认拓扑。结构性瓶颈仍是 plan P2 的呈现流水化与 shm-ring/宿主直呈现。

### §7.2 Interaction P2 地基（2026-11-14）

本轮继续减载并把横滑变成可复现数据，但仍不宣称 60fps：

```text
BerryUI swipe:  Col→Grid→cells 页精确行带（DamageSet::add_exact，≤8）
vgpud ctrlq:    ≤8 个 TRANSFER 一次发布/等待 + 单次并集 FLUSH
launcher:       壁纸按视口预缩放进 committed VMO，失败回退 SmoothRgb565
host VNC:       staging frame + shared FrameRef，按 damage 行同步，零整帧 clone
calibration:    真实 launcher 横滑帧数/时长/submit + rects/area/union
```

2026-11-14 本机产品样本仍出现整窗 `area=2,073,600`、`last_us≈107k～175k`、launcher 横滑新帧数 1。结论是：guest 光栅/宿主解码减载正确，但 QEMU 2D 整窗 DMA transfer 仍是主导瓶颈。下一刀必须先验证 `SET_SCANOUT` 非零子矩形；可用则做显示服务内部宽 resource/host-side pan，不可用则转向双缓冲异步 present 流水线。

随后在 QEMU 11.0.3 / HVF 上用 2×2 probe resource 实测：`SET_SCANOUT(x=1,y=0,w=1,h=2)` 返回控制响应 `0x1205`（invalid parameter），同一 resource 的 `(0,0,2,2)` 正常。当前后端的 host-side pan 路线已被实测排除；滑动后续方向收敛为 wmd 双 composition buffer + present 异步流水线/早回 token，或宿主直接消费 shared composition。

### §7.3 Interaction P2 present pipeline（2026-11-14）

双 composition/frame VMO 与异步 pending flip 落地后，`desktop-calib` 把同一真实 launcher 横滑从基线 `1 frame / last_us≈154,991` 提升到最终源码 `2 frames / 0.891s / submit≈257µs / DMA≈162,597µs`。`reattaches=3` 稳定，小 damage DMA 约 98µs，整窗 DMA 约 100～190ms 且受宿主负载影响。结论只声明“多帧真实呈现已出现”，不得宣称 60fps 或连续流畅动画；决策记录见 `.agents/notes/implemented/2026-11-14-gui-present-pipeline.md`。

### §7.4 Interaction P3 precise damage + completion reclaim（2026-11-15）

本轮查明整窗 transfer 的四个放大器：输入批在 pump 前 wake 会把空 damage 置 FULL；两页×5 行带超过 `DAMAGE_MAX=8`；同帧多次 offset 重复入集；圆点全量重摆。修复后横滑 damage 从 `2,073,600px/1rect` 变为约 `1.16～1.37MPix/1rect`（vgpud 按固定命令成本合并行带包络），wmd 只继承 `back_stale` 域，vgpud 在 flip 完成时回收 alias/credit，launcher 在跟手/snap 期间保留一帧在途。

三次真实手势最终样本：`frames median=4 / min=3`，首帧中位 `0.886s`，submit 最坏 `258µs`，逐帧 DMA 中位 `137,296µs`，所有 token 排水完成；单次 DMA 最坏 `174,940µs`。横滑帧不再出现 `2,073,600px`，稳定在约 `1.16～1.37MPix` 包络。动画总 span 中位 `1.899s`——它包含比一帧跳变更多的真实帧，不能与旧 `0.891s` 直接当同类墙钟比较；首帧反馈与逐帧 DMA 中位改善，但仍不得宣称 60fps 或连续流畅动画终态。

### §7.5 60Hz stage one：消除 DMA/raster 百毫秒预算（2026-11-15）

用户把目标明确为 60Hz 后，验收口径升级为 `median/p95 frame gap≤16.7ms`、240ms snap 至少约 14 帧、1s 持续动画至少 60 帧。`desktop-calib` 以 12ms 真实 QMP move 为输入源，串口观察粒度降到 2ms，并输出输入 QMP 耗时、实际输入间隔、逐帧 gap、p95、snap240 与等效 fps。

阶段一消除两个已定谳的百毫秒/整帧预算：

- vgpud `wait_used` 自旋窗从 100µs 提升到 2ms。旧睡眠臂被 10Hz 调度量化为约 100ms；新窗口按 8.3MB wire 约 0.15ms、8 命令最坏约 1.2ms 留出余量。`VGPUDMA` 追加 `xfer_us/flush_us`。
- BerryUI 多矩形 damage 帧为每个 rect 加 `PushClip/PopClip`。旧 raster 对每条 damage rect 都重复发射全屏壁纸命令，并用包络裁剪；新路径只画各自行带。1080p 横滑 raster 从约 17～20ms 降到约 1.5～2.5ms。
- 单核 idle 与刚阻塞任务的短 deadline 改为精确 CVAL 武装，不再固定 100ms slice。

阶段一干净三次真实手势：输入间隔中位约 17ms，DMA 中位 186µs、最坏 1,187µs，raster 进入 2ms 级；但 launcher 帧 gap 中位仍 292.8ms、p95 487.4ms，frames=5、snap240=2，60Hz 未达。插桩证明 vgpud `CBREQ→CBREADY` 同毫秒完成，剩余延迟在 wmd/launcher 同优先级 RPC relay 的调度与公平轮转，不在 virtio-gpu 带宽或 BerryUI raster。两个激进调度实验已被拒绝：全局同层 direct preempt 破坏 intent 两跳验收；静态把 inputd/wmd/vgpud 放入 deadline 带导致应用内点击链回归。下一刀必须做受限的动态 handoff / RPC reply 调度，不能宣称 60fps。

### §7.6 60Hz stage two：显式协作 reply handoff（2026-11-15）

阶段二不改变 present wire、不新增隐式同层抢占、不给 app present/HDI 能力：

- ChannelEnd 记录最近 receiver 的 Process tag；channel fastpath 记录唯一唤醒者，slowpath 记录忙碌 receiver。该 hint 只在发送方随后显式 `thread_yield` 时生效，且目标不得越过更高优先级 eligible 任务。
- vgpud/wmd/launcher 只对 `FRAME_CALLBACK` 和非空 `INPUT_POLL` 使用 handoff；SUBMIT ACK 保持普通应答，避免生产者在 wmd 排干前继续入队。
- inputd 活动 200ms 窗内的收割 tick 从 2ms 收紧到 500µs，静默仍 16ms；公平片从 100ms 收到 50ms。
- launcher 把 wire 批内最多 3 个真实 pointer position 保序暂存，一帧只推一个真实样本；不插值、不合成、不改时间戳。

三次干净产品手势：`frames median=11 / min=9`，首帧中位 `0.559s`，DMA 中位约 `133µs`，frame gap median `117.9ms`、p95 `312.1ms`，snap240=3。相比阶段一 `5 / 292.8 / 487.4 / 2` 有实质改善，但 60Hz 门仍全项未达。剩余瓶颈在 inputd→wmd→launcher 的真实输入 catch-up 调度；下一层需要该链路的低延迟架构，不能把本版称作流畅终态。

## §8 Mac 呈现路径设计

当前路径：

```text
BerryUI/Berry Room raster（guest CPU，双 frame VMO）
→ gui-wmd 权威窗口表 + 双 composition VMO
→ vgpud virtio-gpu backing/pending transfer/flush
→ QEMU host surface / Cocoa
```

后续 Mac shell 或 Metal presenter 必须满足：

1. 消费同一 wmd composition VMO 与 HDI fence/vblank 契约。
2. 不在宿主侧建立第二套窗口权威或像素真相源。
3. 不让普通 app 获得 present/hdi 能力。
4. 与 soft↔vgpu 像素共识 gate 对拍后才可替换 QEMU surface。
5. 继续保留 ramfb/guifbd 作为无设备回退，验证镜像零漂。

## §9 验收与边界

已定门禁：

```text
python3 tools/gui8-smoke.py --self-test
python3 tools/gui8-smoke.py --vgpu ...
python3 tools/gui8-smoke.py --normal-boot ...
python3 tools/gui8-smoke.py ...        # ramfb dual canonical
bash tools/smoke-image.sh out/berryos.img ...
just gpu2-hdi-image + 1080p HVF 诊断 boot
just gpu2-desktop-calib
just test-host / just check / cargo deny licenses
Curia strict / docs_guard / gui_guard / sdk_ledger / git diff --check
```

边界：

- `COMMIT` fence 在像素 transfer/flush 后释放；不承诺 host scanout 的微秒级可见时序。
- cache maintain 仍是 vblk 同款缺口；QEMU/TCG/HVF 一致，真机 DMA API 另刀。
- 普通 app 不消费 HDI；只有系统显示服务与诊断 gate 使用。
- `gpuhdi.on` 不进正式产品/验证镜像。
- Mac 绝对性能数据只记录本机报告，不做跨机器 CI 阈值。
