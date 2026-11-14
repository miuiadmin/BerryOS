# BerryBoot Boot Progress P1（双模式与胶囊进度）

Type: standard
Version: 1.1.0
Status: current
Updated: 2026-11-15

## §1 产品裁决

产品几何与显示栈保持：

```text
1080×1920@60
HVF + virtio-gpu 主显示
ramfb 固件与装载器首屏
```

不降分辨率、不伪造进度、不在 app/demo 里补启动画面。进度真值只来自完成的装载/服务状态谓词与真实字节计数；墙钟不能单独制造进度。

## §2 双模式选择

BerryBoot 读取 fw_cfg：

```text
opt/berry/bootmode
```

取值：

```text
absent / graphical → 图形胶囊面
verbose            → 命令行装载日志面
```

`BERRY_BOOTMODE=verbose` 的 berry CLI 会把该 fw_cfg 项传给产品镜像。verbose 面显示阶段名、白色胶囊进度与最近 10 条真实 BerryBoot 诊断行；图形面保持 Logo + 胶囊，不显示百分比。两种 Face 消费同一 `(Stage, permille)` 真值流。

## §3 真值与平缓算法

核心仍是：

```text
permille ∈ 0..=1000
Stage ∈ Display / Verify / Kernel / Ramdisk / Prepare / Handoff
span(start, end, done, total)
```

新增：

```text
span_smooth(start, end, done, total)
slew(prev, truth, dt_ms)
```

`span_smooth` 用 Q16.16 smoothstep 只平滑真实 `done/total`，让 1MiB 文件读取在阶段两端平缓推进；零总量、倒区间、完成数超量仍 fail-closed。`slew` 是可 host 测试的呈现限速：普通变化最多 8 permille/100ms，跨度 ≥80 permille 或完成 1000 立即收敛，且永不回退、永不超前真值。

装载器纪元改为保留 0..390，系统纪元从 450 开始，消除旧版“装载器 100% 后回到系统 45%”的视觉回退：

```text
0      display mode / first surface
20     HELLO.TXT verified
20..160 KERNEL.ELF bytes read
160..330 INITRD.BIN bytes read
330..370 ELF parse + segment copy
370..390 watchdog / memory map / EBS / handoff
450    vgpud ready
...
1000   desktop handoff
```

系统里程碑仍为 450/500/520/560/620/700/800/900/1000，900..1000 只属于 SYSTEM surface0。

## §4 图形与 verbose 视觉

图形面：

```text
背景 #000000
BerryOS 白色 wordmark
白色长椭圆形胶囊进度
进度槽 #3A3A3C
无百分比 / 无阶段文字
```

进度条外包络仍为产品 756×20 @ y=1305，但填充与槽均为圆角胶囊；每次真实更新完整重绘胶囊外包络，增量 DMA 矩形含上下 2px 圆角清边。

verbose 面：

```text
黑色背景
阶段 label
同真值胶囊
最近 10 条 BerryBoot 诊断行
```

诊断行来自与 EFI ConOut 同一次 `print_line` 的 ASCII Line，不复制第二份日志语义。日志环属于 `ProgressRuntime` 栈/运行时状态；EFI 镜像单节文本不得作为可写全局存储。

## §5 接管与完成

vgpud 的私有 BootProgress VMO 收到 `OP_BOOT_PROGRESS` 后按相同 Face/胶囊渲染并执行 ATTACH/SET_SCANOUT/TRANSFER/FLUSH。首个 SYSTEM surface0 成功 SUBMIT 前，进度最高到 900；desktop handoff 置 1000 后立即由 wmd 聚合桌面帧替换 BootProgress scanout，不插入 timer 动画。

普通 app 无 `OP_BOOT_PROGRESS` 能力；ramfb 后端收到该控制帧仍返回 unsupported。

## §6 验收

Host：

```text
berry-bootprogress tests
  span/span_smooth 单调与 fail-closed
  slew 单调 / 限速 / 1000 收敛
  胶囊圆角像素
  verbose 阶段与日志可见
  1080×1920 与 800×600 native layout
```

产品：

```text
默认模式产品 Recents e2e
verbose fw_cfg 早期 ramfb 截图出现非黑诊断文本与胶囊
VGPUBOOT 450..1000 严格单调
LAUNCH ok 后无进度回退
KERNEL PANIC = 0
```

## §7 边界与后续

- verbose 当前覆盖 BerryBoot 装载/交接日志；内核与用户服务纪元仍主要输出到串口，尚未进入同一屏幕日志总线。
- fw_cfg 运行期 Esc 切换、auto-failure latch、handoff 携带最终 permille、系统服务日志面与 shutdown face 留后续版本。
- `BootProgress` 是系统控制面，不进 BerryUI SDK；不改 present wire，不给普通 app present/HDI。
