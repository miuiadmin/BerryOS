# BerryBoot Boot Progress P1（双模式、系统里程碑与胶囊进度）

Type: standard
Version: 1.2.0
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

`BERRY_BOOTMODE=verbose` 的 berry CLI 会把该 fw_cfg 项传给产品镜像。BerryBoot verbose 面显示阶段名、白色胶囊进度与真实装载诊断；vgpud 接管后显示同一胶囊与系统 milestone 短日志。图形面保持 Logo + 胶囊，不显示百分比。两种 Face 消费同一 `(Stage, permille, milestone)` 真值流。

内核用同一 fw_cfg 键解析 boot face，并把它作为 fbinfo VMO 第 7 域（offset 48..56，u64 LE）add-only 交给 svcmgr；svcmgr 只在 verbose 时给 vgpud 追加 `--boot-face=verbose` argv。该 VMO 其余消费者仍只读前 48B。

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

系统 milestone 真值表为：

```text
id  permille  stage     truth
1   450       Prepare   vgpud display ready
2   500       Prepare   present backend ready
3   520       Prepare   gui-wmd spawned
4   560       Prepare   input service armed
5   620       Prepare   gui-inputd spawned
6   650       Prepare   fsmgr spawned（非 FAT ready）
7   680       Prepare   vnet spawned（非 NET ready）
8   700       Handoff   progress control handoff
9   800       Handoff   SYSTEM UI connected
10  900       Handoff   first real SYSTEM submit
11  1000      Handoff   desktop handoff
```

产品拓扑必须按 id 1..=11 严格单调各一次。GUI8 探测拓扑在 gfxdemo 首个真实 SUBMIT 前提前交接控制面，id 6/7 结构性缺席，合法序列为 1..=5、8；vgpud 对首个真实 SUBMIT 后的迟到 milestone 返回 OK 幂等确认、不再重绘 boot face。

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
最近真实诊断/milestone 行
```

BerryBoot 装载纪元显示与 EFI ConOut 同一次 `print_line` 的 ASCII 诊断行；vgpud 系统纪元显示 milestone 表单源生成的罐头短行（例如 `fs: service spawned`、`desktop: ready`）。wire 不携带自由文本，参与者不能伪造系统日志。日志环容量足以容纳全部 11 个系统 milestone；小几何仍按可用高度裁剪。

## §5 接管与完成

`OP_BOOT_PROGRESS` 仍为 8B、0 handle、1B status 的 present 域控制帧：

```text
[0] op = 7
[1] stage
[2] milestone id（0 = legacy absent）
[3] reserved
[4..6] permille LE16
[6..8] reserved
```

milestone 1..=11 是稳定判别值；非 0 id 的 stage/permille 必须与单源表一致，编码与解析双侧拒绝手拼三元组。milestone 0 仅作为旧形缺席值，服务端按表内 permille 反查 id，并仍要求 stage 匹配。

vgpud 的私有 BootProgress VMO 收到该帧后按相同 Face/胶囊渲染并执行 ATTACH/SET_SCANOUT/TRANSFER/FLUSH。首个真实 SUBMIT 拥有 scanout 后，迟到或重放 milestone 是 OK 幂等 no-op，不把 boot face 重新选回 scanout。渲染、backing 或 flush 失败不消耗 milestone，重试仍可进入；成功后才落 once 表与日志环。

普通 app 无 `OP_BOOT_PROGRESS` 能力；ramfb 后端收到该控制帧仍返回 unsupported。wmd 只转发 present op 1..=6，app session 发 op7 会得到 ST_BAD。

## §6 验收

Host：

```text
berry-bootprogress tests
  span/span_smooth 单调与 fail-closed
  slew 单调 / 限速 / 1000 收敛
  胶囊圆角像素
  verbose 阶段与日志可见
  milestone 表全量、严格单调、日志环容量覆盖 11 点
  1080×1920 与 800×600 native layout
  op7 milestone 三元组不一致 encode/parse 双拒
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

- verbose 系统纪元当前显示 milestone 罐头短行；内核全文日志、驱动自由文本日志与装载器诊断跨 390..450 接缝尚未进入同一屏幕日志总线。
- fw_cfg 运行期 Esc 切换、auto-failure latch、handoff 携带最终 permille、系统日志自由文本与 shutdown face 留后续版本。
- `BootProgress` 是系统控制面，不进 BerryUI SDK；op7 不给普通 app present/HDI 能力，present op 集合不变。
- GUI8 探测拓扑的 id 6/7 缺席是 gfxdemo 首个真实 SUBMIT 与私有 dup 生命周期共同决定的显式例外，不得当作产品缺帧。
- verbose vgpu 纪元当前每次 milestone 全幅 TRANSFER/FLUSH；boot 期 ≤11 次的可接受开销先保留，后续再评估条带并集优化。
