# Berry Mac Emulator GPU2（virtio-gpu HDI 显示框架）
Type: standard
Version: 1.0
Status: current
Updated: 2026-11-13

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

`SUBMIT` 保持 present wire、credits、damage 帽与 alias 生命周期不变。每个 frame VMO 先进入 backing 池，再执行：

```text
RESOURCE_ATTACH_BACKING（仅换背）
SET_SCANOUT（仅 resource 改变）
TRANSFER_TO_HOST_2D
RESOURCE_FLUSH
SUBMIT_ACK
```

damage 执行面按实际拷贝字节数择优：

```text
rect 面积和 ≤ 包络面积 → 逐 rect transfer/flush
包络面积 < rect 面积和 → 单包络 transfer/flush
```

这保留了稀疏小损伤，又避免重叠 damage 被放大成多次整窗拷贝。控制队列等待先短自旋再 100µs 睡眠，不再为每个命令固定 1ms 睡眠。`VGPUPERF` 在 ACK 后输出累计均值、单次耗时、reattach 数和 vblank jitter，串口开销不进入 guest 可见提交延迟。

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

## §8 Mac 呈现路径设计

当前路径：

```text
BerryUI/Berry Room raster（guest CPU）
→ gui-wmd 权威窗口表 + composition VMO
→ vgpud virtio-gpu backing/transfer/flush
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
