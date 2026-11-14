# BerryBoot Boot Progress P1（真实 0→100% 首屏框架）

Type: standard
Version: 1.0
Status: superseded
Superseded-by: [BerryBoot-BootProgress-P1-v1.1.md](BerryBoot-BootProgress-P1-v1.1.md)
Updated: 2026-11-15

## §1 产品裁决

Mac 产品初始几何仍是：

```text
1080×1920@60
HVF + QEMU Cocoa
vgpu0 主显示 / ramfb 固件与首屏面
```

Boot Progress P1 不降分辨率、不做墙钟动画、不在 app/demo 里补启动画面。进度值只来自已完成的 boot service、实际读写字节、段装载或系统服务状态谓词。

## §2 进度真值模型

新增 host 可测纯函数 crate：

```text
bootloader/berryboot/berry-bootprogress
```

模型：

```text
permille ∈ 0..=1000
Stage ∈ Display / Verify / Kernel / Ramdisk / Prepare / Handoff
span(start, end, done, total) -> permille
```

`span` 对零总量、完成数大于总量、区间倒退一律失败；渲染器拒绝进度回退。_host 测试钉板真实计数映射、1080×1920 / 800×600 原生布局、短帧缓冲拒面与单调性。_

BerryBoot 权重：

```text
0        display mode / first surface
30       HELLO.TXT verified
30..300  KERNEL.ELF bytes read
300..800 INITRD.BIN bytes read
800..900 ELF parse + segment copy/verify
900..1000 watchdog / memory map / EBS / handoff
```

KERNEL 与 INITRD 读取以 1MiB 粒度更新；粒度只影响采样频率，不制造进度。

## §3 1080×1920 首屏 mode-set

BerryBoot 在 GOP 事实收集后读取 fw_cfg：

```text
opt/berry/display
```

产品请求 `1080x1920@60` 时：

```text
stride = 4320
frame bytes = 8,294,400
```

若请求面大于固件 GOP 缓冲，BerryBoot 用 `AllocatePages(EfiLoaderData)` 钉完整帧页，写 `etc/ramfb` fw_cfg DMA 配置，并把 handoff `gop_*` 更新为 1080×1920。该页进入终局 memory map，内核 reserve 后不得回收。

fw_cfg DMA descriptor 使用目标帧缓冲自身的确定 PA：

```text
frame + 0x00 descriptor
frame + 0x10 28B ramfb config
```

这避免假定 EFI static/stack VA=PA。首帧渲染会覆盖这 64B 临时区。无 display 请求时，smoke-image 仍使用固件 GOP 原生几何并渲染真实进度。

状态谓词：

```text
BOOTPROG ok display <W>x<H>
```

产品实测：

```text
BOOTPROG ok display 1080x1920
```

## §4 首屏视觉

1080×1920 原生布局：

```text
背景 #000000
BerryOS 白色标题
白色进度条 756×20 @ y=1305
无百分比 / 无阶段文字
```

视觉裁决对齐 macOS：启动面只保留白色标志与白色进度条。百分比与阶段文字不参与首屏；`Stage` 仍是协议真值，不再转成模糊文本。

主色：

```text
背景 #000000
标志 / 进度填充 #FFFFFF（纯白）
进度槽 #3A3A3C
```

像素通道修正：QEMU ramfb/vgpu 面是 little-endian XRGB8888（内存 B,G,R,X）。渲染器统一用 `PixelFormat::Rgbx.pack` 的 raw=0 写入，禁止把 RGB 蓝误写成橙。host 测试直接钉住白色填充内存三字节与进度条下方黑场，防止百分比/文字回归。

## §5 vgpu 主显示进度与交接

present 域新增控制帧：

```text
OP_BOOT_PROGRESS = 7
[op][stage u8][r16][permille u16][r16]
```

值域与 `berry-bootprogress` 镜像：

```text
stage 0..5
permille 0..1000
```

这不是 app 呈现请求，不携带 surface/buffer，也不进入 Berry Room SDK。普通 app 无法取得该控制端。

产品链（九个真实里程碑，全部由状态谓词触发，禁止运行时墙钟插值）：

```text
vgpud ready                     → 450
present backend ready           → 500
wmd spawned                     → 520
input arm reached               → 560
inputd spawned                  → 620
progress control handoff        → 700
SYSTEM launcher WM_CONNECT      → 800
first SYSTEM surface-0 SUBMIT   → 900
desktop handoff                 → 1000
→ wmd SYSTEM SUBMIT replaces progress with desktop
```

900/1000 只属于 SYSTEM surface 0。GUI8 中 gfxdemo/普通 app 可以先提交，但不能消费“桌面交接”里程碑。

vgpud 渲染进度使用私有 VMO，映射在本进程 VMAR 后写入。800×600 的线性帧是 468.75 页，VMO commit 面按 4KiB 向上取整；渲染与 resource 指纹仍使用精确线性长度。随后执行：

```text
ATTACH_BACKING
SET_SCANOUT
TRANSFER_TO_HOST_2D
RESOURCE_FLUSH
```

状态谓词：

```text
VGPUBOOT ok stage=<stage> permille=<permille> rect=<WxH>@<Y>
```

首帧整面 `1080×1920@0`；后续 renderer 只改白色进度条本身，当前布局 damage 为：

```text
1080×20@1305
```

GUI8 800×600 后续条带为：

```text
800×12@432
```

最终实测序列：

```text
stage=4 permille=450
stage=4 permille=500
stage=4 permille=520
stage=4 permille=560
stage=4 permille=620
stage=5 permille=700
stage=5 permille=800
stage=5 permille=900
stage=5 permille=1000
LAUNCH ok
```

显式 `device=vgpu0` 截图验证 70% 白色中间进度面、100% 终态序列与随后的桌面首帧；ramfb 与 vgpu0 的进度面像素一致。首帧后的条带 damage 从整面 8.3MiB 降到 86.4KiB，且不再携带文字区。

## §6 回归与守门

### Host

```text
berry-bootprogress tests
berryboot tests
berry-guiproto tests
just test-host
```

### Guest / image

```text
smoke-image:
  BOOTPROG ok display WxH 恰一次
  EFI 十段 / 接续 / 裸相 / 生态既有标记零漂

GUI8:
  boot base markers 加入 BOOTPROG ok display 800x600
  self-test fixture 覆盖该状态谓词
```

### Mac / HVF

产品诊断链要求：

```text
BOOTPROG ok display 1080x1920
VGPUBOOT ok ... 450/500/520/560/620/700/800/900/1000 严格单调
WMNAVINVAL ok sid=1（导航后 SYSTEM 全屏重绘）
LAUNCH ok
KERNEL PANIC = 0
```

截图面要求 1080×1920 PPM 尺寸，且 ramfb 首屏与 vgpu 主显示各自可见。

## §7 边界与后续

- BerryBoot 0→100% 首屏在 ramfb/GOP 面；QEMU 默认主 console 在 vgpu 时，产品窗口在 vgpud ready 前仍可能短暂显示 vgpu 自验空白面。
- vgpu 主显示滚动进度从 svcmgr/vgpud 接管后开始，当前覆盖 45%→100%，再由 wmd 首帧替换为桌面。
- `BootProgress` 是系统控制面，不是 BerryUI app API；SDK ledger 不新增 facade。
- ramfb 后端收到 `BootProgress` 返回 `ST_UNSUPPORTED`，防止端点装配错位。
- 后续 P2 需解决 QEMU Cocoa 单主 console 的宿主切换：ramfb 0% 首屏与 vgpu 桌面之间的无缝主显示切换需要设备壳/QMP 控制面协作，不能用第二像素真相源伪造。
