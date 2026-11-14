# Berry Mac Emulator HVF1（1080p 产品桌面）

> 版本：**1.0**｜2026-11-13
>
> 定位：Mac Apple Silicon 上 BerryOS 交互模拟器的 HVF/CPU、产品快启动、输入与 damage 基线；显示后端现状由 [BerryMacEmulator-GPU2-v1.2.md](BerryMacEmulator-GPU2-v1.2.md) 接管。分辨率裁决 = 初始 **1080×1920@60**，不提供降分辨率默认值；验收确定性仍归 TCG/GUI8，交互性能归 HVF。
>
> 相关决策：[架构决策.md](架构决策.md) ADR #90；应用级桌面演示规格 = [BerryDemo-D1-v1.0.md](BerryDemo-D1-v1.0.md)；历史设备壳架构 = [Berry设备平台-CLI-first设计-v1.0.md](Berry设备平台-CLI-first设计-v1.0.md)。

## §1 产品入口

Mac `arm64` 上的默认交互入口是：

```sh
just desktop
```

该配方优先使用产品镜像 `out/berryos-desktop.img`，通过 QEMU Cocoa 直接开原生窗口，并固定：

```text
-machine virt
-cpu host
-accel hvf
-display cocoa
-m 256M
-device virtio-tablet-device
-device virtio-keyboard-device
-device ramfb
-fw_cfg name=opt/berry/display,string=1080x1920@60
```

产品数据盘由 `tools/make_desktop_disk.py` 确定性生成：首屏 Browser / Berry Demo，第二页 Files；Contacts / Photos / Settings / MMS 由 SmartDock 常驻。`just desktop` 不改写用户数据盘之外的系统镜像。

非 Darwin/arm64 平台回落 `berry shell --profile desktop`；宿主运行目录可用 `BERRY_HOME` 显式指定，避免受限环境写 `~/.berry` 失败。

## §2 产品镜像与验收镜像分流

`just image` 同时产出两份镜像：

```text
out/berryos-desktop.img  产品桌面（无 probe.on）
out/berryos.img          验证主镜像（含 probe.on）
```

`tools/cpio_filter.py` 从验证 rootfs 确定性移除 `probe.on` 后重编码产品 rootfs。产品模式保留 vblk / guifbd / gui-wmd / gui-inputd / fsmgr / vnet / berry-launcher、APP_LIST 与 APP_SPAWN，跳过串行 app boot-probe、gfxdemo 与 rootfs 自测批；验证模式全部保留，GUI8 与 smoke-image 标记零漂。

launcher 通过 `argv[2] = product` 只跳过 boot 期两轮合成 swipe 自测；真实点击、拖动、布局持久化与 APP_SPAWN 仍走原链。验证镜像不带该参数，`LAUNCH ok` / `PAGE ok n=2` / `CLICK ok` / `SWIPE ok` 继续执行。

## §3 HVF 语义修正

HVF1 修复四个 Apple Hypervisor 差异，HVF 与 TCG 由同一源码承载：

1. Apple Silicon FPSR 的 NZCV 位 RAZ/WI。`ktest` 在运行时探测该能力，只按硬件可保存位断言恢复；FP 寄存器、FPCR 与 FPSR 异常累计位仍全量检查。
2. HVF 启用 PAN。`SCTLR_EL1.SPAN` 随 MMU 使能置位，内核在系统调用中写用户缓冲不再触发数据中止。
3. HVF 下 TTBR0/CONTEXTIDR 的 ASID 字段读回为 0。功能 ASID 单源改为 `berry_sched::current_asid()` 读取调度器 `Task::asid`；TTBR 读取只留作诊断。
4. QEMU HVF 不能仿真无 ISV 的 virtio-input 配置区拆分读。`gui-inputd` 改为对齐 `u32` 读后按小端拆字节，消除 QEMU `assert(isv)` 崩溃。

## §4 输入低延迟合同

`gui-inputd` 在指针或按键活动后的 200ms 窗口内使用 2ms 等待 deadline；静默期回落 100ms，避免空闲轮询。

`gui-wmd` per-session 输入环保留 DOWN / UP / CANCEL，连续同指针 MOVE 按低延迟帽合并：

```text
MOVE_QUEUE_LATENCY_MAX = 4
```

合并总是更新最后一个 MOVE，保最新坐标与最多三个历史采样；队列满且控制事件到达时淘汰最旧 MOVE，不让 UP 排到移动回放之后。溢出诊断只在首丢与每 64 次打印。

桌面层补两件反馈：真实格位按下即显示 pressed 背景；APP_SPAWN 有 500ms 去抖。Dock 拼图 icon 不是 press-map 合法 visual，v1 不为其伪造反馈。

## §5 damage 面

Berry Room 新增 `render_display_list_damaged`：embedder 把 BerryUI `FrameOutcome.damage` 的包络交给 BerryGFX pass，只在 dirty 区光栅化；随后 `submit` 用同一矩形发起 partial present。FULL、空集或零面积矩形保守回落整帧。

gui-wmd 对 app FULL 先收缩到 visible-local，再向 guifbd 转发已翻译可见矩形，禁止“窗口 FULL 放大成整屏 FULL”。launcher 保持自己的 damage 矩形集提交，不经过 Berry Room。

## §6 实测口径与结果

HVF 交互测量使用 QEMU Cocoa/headless、真实 `virtio-tablet` QMP 注入，不用应用内 mock。2026-11-13 同机实测：

```text
boot to LAUNCH ok（重载宿主）       17.801s
真实横滑注入                         0.747s，帧变化 = true
滑动后 Files 冷启动到 FILES up       0.320s
主屏 Berry Demo 冷启动               0.579s
页面切换到 BERRYDEMOPAGE ok          0.169s
Switch 到 BERRYDEMOSWITCH ok         0.148s
WMIDROP bad / INPUTDROPS bad        0 / 0
KERNEL PANIC                         0
```

更早的轻载宿主实测产品桌面 ready 为 8.78s；本表采用重载保守值，不把竞态时点写成合同。

最终产物：

```text
out/berryos.img          sha256 f250a1c0fc5cc73bf0c5e12e268255b18dedb31d61e361df2ff56fc59a6079ca
out/berryos-desktop.img  sha256 a1d1e82789863521e15882bdc1f75352f6f7906458c177aaf56112308c561e2b
rootfs.cpio              34,119,496B
rootfs-product.cpio      34,119,352B
```

最终回归：

```text
GUI8 normal boot                 PASS
GUI8 dual boot canonical         PASS
smoke-image                      PASS 146/146
just test-host                   PASS
just check                       PASS
cargo deny licenses              PASS
```

## §7 GPU 路径事实

本刀没有把 guest 光栅迁到 GPU，也不宣称 virgl 可用。2026-11-13 在 macOS QEMU 11.0.3 实测：

```text
-device apple-gfx-pci            可冷插，可经 JSON display-modes 指定 1080x1920@60
EDK2 → BerryBoot handoff         eco: fb absent（无 GOP framebuffer）
-device virtio-gpu-device        QEMU 可提供，BerryOS guest 驱动未实现
```

HVF1 时点显示路径仍为 guest CPU BerryGFX 光栅 + ramfb + QEMU Cocoa 宿主呈现；该现状由 [BerryMacEmulator-GPU2-v1.2.md](BerryMacEmulator-GPU2-v1.2.md) 的 vgpud/virtio-gpu HDI 后端接管。`apple-gfx-pci` 仍不可用，禁以空设备冒充 GPU 加速。

## §8 不变量

- 初始产品分辨率恒为 1080×1920；任何降分辨率方案只能是显式开发配置，不是默认。
- TCG 验证镜像的标记、像素锚与 GUI8 计数不得因产品快启动削减。
- 产品桌面必须保留真实 APP_LIST / APP_SPAWN、Berry Room 生命周期与真实输入路由。
- 普通应用仍不映射 VMO、不计算 stride、不写 framebuffer、不使用 surface escape。
- HVF 修正不得依赖 `cpu host` 写死 ID 值；硬件差异运行时探测。
