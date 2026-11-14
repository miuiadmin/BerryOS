# Berry Mac Emulator GPU1（virtio-gpu 2D 产品显示）
Type: standard
Version: 1.0
Status: superseded
Updated: 2026-11-13
Superseded-by: [BerryMacEmulator-GPU2-v1.5.md](BerryMacEmulator-GPU2-v1.5.md)

> 定位：GPU1 基线规格；现行 HDI、backing 池、像素共识与性能定标由
> [BerryMacEmulator-GPU2-v1.5.md](BerryMacEmulator-GPU2-v1.5.md) 接管。HVF/CPU、产品快启动、输入与 damage 基线见 [BerryMacEmulator-HVF1-v1.0.md](BerryMacEmulator-HVF1-v1.0.md)；本规格接管其显示路径现状。
>
> 相关决策：[架构决策.md](架构决策.md) ADR #50/#51/#90/#91；执行计划归档见 [2026-11-13-virtio-gpu-2d-mac-emulator.md](../.agents/plans/2026-11-13-virtio-gpu-2d-mac-emulator.md)。

## §1 产品裁决

Mac Apple Silicon 产品入口仍是 `just desktop`，初始分辨率不变：

```text
1080×1920@60
QEMU Cocoa + HVF
virtio-gpu-device,id=vgpu0,xres=1080,yres=1920
ramfb（固件引导/回退显示面）
```

本刀不降分辨率、不把未驱动的设备伪装成 GPU。`vgpu0` 排在 ramfb 前形成主显示 console；ramfb 仍供 EDK2/BerryBoot 取得早期 GOP，避免 virtio-gpu-only 时 `fb=0` 导致 `eco: fb absent`。

## §2 显示后端框架

后端选择发生在 **svcmgr 的 present/hdi 服务端点装配层**，不在客户端协议里加分支，也不把 guifbd 改成进程内 if/else：

```text
产品相：
  svcmgr → vgpud ready 握手
        → present/hdi client 端交给 gui-wmd
        → BerryUI/Room/wmd 继续发同一 PresentReq

回退/验证相：
  vgpud ready 缺席或超时
  → guifbd 继续持 present/hdi 端点
  → ramfb 行弹路径与 GUI8/smoke 像素锚不变
```

这是微内核服务级多态：客户端与 wire 协议零改动，后端可独立崩溃、替换和验收；也避免 `CHAN_HND_MAX` 8→9 的内核连带抬帽。vgpud 启动成功后发 1B `V` ready；svcmgr 等 3s，失败关闭本侧 client 端并回落 guifbd。

## §3 vgpud 驱动链

`drivers/bins/vgpud` 复用 vblk 的用户态 virtio 框架：

```text
virtio-mmio did=16 扫描
→ modern VERSION_1 协商
→ ctrlq split ring
→ GET_DISPLAY_INFO
→ RESOURCE_CREATE_2D(1080×1920, BGRX8888)
→ RESOURCE_ATTACH_BACKING
→ SET_SCANOUT
→ TRANSFER_TO_HOST_2D
→ RESOURCE_FLUSH
→ VGPU ok w=1080 h=1920 scanout=1
```

控制命令编解码单源在 `drivers/berry-virtio/src/gpu.rs`，wire 形态与 Linux UAPI `virtio_gpu.h` 对拍，host golden/bounds 测试钉字段偏移。MMIO config 全对齐 `u32` 读，沿用 HVF no-ISV 教训。

## §4 帧面与 damage

present SUBMIT 随行 frame VMO。vgpud 只在首次或 frame VMO 物理首页变化时重建 backing：

```text
2025 × 4KiB frame 页 → 2025 个 virtio_gpu_mem_entry
entries 按 4KiB 分页，经 descriptor chain 供给 ATTACH_BACKING
同一 client frame VMO 的后续 fresh alias 不重复 get_phys/attach
```

每个 damage 矩形按同一几何执行：

```text
TRANSFER_TO_HOST_2D(offset = y*4320 + x*4)
RESOURCE_FLUSH(rect)
```

FULL 转为全屏 rect；partial 直接沿用 BerryUI/Berry Room/wmd 转发的矩形。相比 ramfb 路径，这消除 guifbd 的逐行 `berry_vmo_read`/bounce copy，并让 QEMU 只刷新 damage 区域。guest CPU 中的 BerryGFX 光栅仍保留；本刀加速的是显示传输/呈现链，不宣称 3D/virgl。

## §5 协议面

vgpud 实现产品主链 present 六 op：

```text
GETINFO
SUBMIT
FRAME_CALLBACK
RELEASE_WAIT
VOTE
POWER_MODE
```

SUBMIT 保留 credits ≤2、无 damage 不提交、FULL/K 互斥与 alias 生命周期；FRAME_CALLBACK 返回 `FRAME_PRESENTED`；RELEASE_WAIT 全量归还别名。GPU2 起vgpud 提供 REG_VBLANK / VBLANK_WAIT / PREPARE / COMMIT，且 `gpuhdi.on` 诊断镜像覆盖 fence 真链；COMMIT 驱动像素呈现仍归后续 GPU2。

## §6 验证与实测

2026-11-13 GPU1 终验源码实测：

```text
VGPU ok                          1
explicit QMP screendump(device=vgpu0)
  横滑前后像素帧变化              true
boot to LAUNCH ok                21.817s（重载宿主）
真实横滑注入                      0.731s
横滑后 Files 冷启动               0.562s
Berry Demo 冷启动                 1.263s
页面切换                         0.486s
Switch                           <20ms（轮询分辨率）
WMIDROP bad / INPUTDROPS bad    0 / 0
KERNEL PANIC                     0
```

无 virtio-gpu 设备的产品镜像回退实测：

```text
VGPU ok = 0
FBW ok / FBSERVE ok / LAUNCH ok = 1 / 1 / 1
KERNEL PANIC = 0
```

GPU1 基线产物（后续 AppBar D3 的最新镜像哈希归
[BerryDemo-D1-v1.0.md](BerryDemo-D1-v1.0.md) 单源记录）：

```text
out/berryos.img          sha256 0884441868b4162b1e21c507d2ca1831f71f760249bba79f3865f58fc31afe7d
out/berryos-desktop.img  sha256 dead7e369c5336e021d767a9475c5c57904738385b04c51d5ee00e23e0a18429
rootfs.cpio              34,452,424B
rootfs-product.cpio      34,452,280B
vgpud ELF                332,808B
```

回归口径：GUI8 normal / GUI8 dual canonical / smoke-image 146/146 / `just test-host` / `just check` / `cargo deny licenses` 全部 PASS。产品 GPU 路径不改变验证 TCG 拓扑，`probe.on` 分流与 146 个 smoke 标记不变。

## §7 已知边界

- vgpud 已有 vblank event、PREPARE fence 与 COMMIT 定时释放；COMMIT 尚未把 slot VMO 排队呈现。
- soft↔vgpu 的机器像素共识尚未形成独立 CI gate；当前由 GUI8/ramfb 与产品 vgpu 显式截图双链证明。
- frame backing 已用双 resource + first/last PA + READ pin 池化；COMMIT 呈现与更多并发槽归 GPU2。
- cache maintain 仍是 vblk 同款 v1 缺口：QEMU/TCG/HVF 一致，真机 DMA API 另刀。
- Mac shell shm-ring/Metal 呈现组合归 GPU2；Metal 不得成为第二客机像素真相源。
- 现行执行拆解见 [../.agents/plans/2026-11-13-virtio-gpu-hdi-pool-calibration.md](../.agents/plans/2026-11-13-virtio-gpu-hdi-pool-calibration.md)。
