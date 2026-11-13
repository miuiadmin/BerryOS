# BerryBoot Boot Progress P1（真实 0→100% 首屏框架）
Type: standard
Version: 1.0
Status: current
Updated: 2026-11-13

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
背景 #080E18
BerryOS 标题
进度条 756×20 @ y=1305
28×54 专用抗锯齿数字与 `%`
阶段文本
```

百分比不是 8×16 点阵放大字：0–9 与 `%` 由专用几何段（横/竖/圆/斜线）构成，每像素 4×4 supersampling 生成 16 级覆盖度并与背景混合。host 测试钉住高分辨率字形与抗锯齿中间色，终态显示完整 `100%`。

主色：

```text
#0074E0
```

阶段文案：

```text
Starting display
Verifying boot disk
Loading kernel
Loading system
Preparing BerryOS
Starting BerryOS
```

渲染器初始化清完整 native framebuffer；后续更新只画新增进度条段和两个小文本区，不整屏重画。视觉截图已验证 0%、滚动中与 100% 状态。

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

产品链：

```text
vgpud ready
→ svcmgr duplicate present client（SEND|RECV）
→ BootProgress 450
→ wmd spawn / inputd / launcher assembly 前推进 520 / 560 / 700
→ close duplicate before the first GUI client
→ wmd sends Handoff permille=1000 immediately before its first real SUBMIT
→ wmd first SUBMIT replaces progress with desktop
```

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

首帧整面 `1080×1920@0`；后续 renderer 只改底部进度区（含更高数字区），当前布局 damage 为：

```text
1080×152@1305
```

GUI8 800×600 后续条带为：

```text
800×140@432
```

最终实测序列：

```text
stage=4 permille=450
stage=4 permille=520
stage=4 permille=560
stage=5 permille=700
stage=5 permille=1000
LAUNCH ok
```

显式 `device=vgpu0` 截图验证 70% 中间进度面、100% 终态序列与随后的桌面首帧；默认 QEMU console 与 vgpu0 逐字节一致。首帧后的条带 damage 把三次系统阶段更新的传输面从整面 8.3MiB 降到约 0.75MiB。

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
VGPUBOOT ok ... permille=450
VGPUBOOT ok ... permille=700
VGPUBOOT ok ... permille=1000
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
