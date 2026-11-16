# GUI主线合线与BootProgress尾域裁决

Status: implemented

## Problem

GUI 1080×1920 产品线长期推进在 `wip/gui-1080p`，而 `origin/main` 同时合入 refresh 刀1..6、CBOX 与调度 1.1。两侧不是快进关系：GUI 线领先 103 个提交，主线领先 51 个提交。最关键的协议撞点有两处：

1. GUI BootProgress 已把 present op 7 用于启动进度；mainline refresh 把 op 7/8 分配给 INPUT_PULSE 与 FRAME_TIMING。
2. GUI boot face 曾写 fbinfo 第 7 域 offset 48..56；mainline refresh 把第 7..9 域用于 refresh_hz 与软面板配置。

## Decision

### Present op

合线后采用 add-only 尾插：

```text
op 7 = INPUT_PULSE
op 8 = FRAME_TIMING
op 9 = BOOT_PROGRESS
VER_PRESENT = 3
```

op9 仍是 wmd 自产系统控制面，不进入普通 app 的 wmd relay 分发表。guifbd 对 op9 保持 `ST_UNSUPPORTED`；vgpud 是唯一图形 BootProgress 后端。

### fbinfo 十域布局

fbinfo 扩为 80B 十域：

```text
0..40   fb_pa/w/h/stride/fmt/gui
48..56  refresh_hz
56..64  softpanel domain 7
64..72  softpanel domain 8
72..80  boot face
```

guifbd 只读前 72B；svcmgr/vgpud 显式读取 offset 72 的 boot face。这样 refresh 与 BootProgress 互不覆写，旧读者按各自长度继续工作。

### refresh guest 职责分离

mainline refresh 刀1..6 的 guest 活证以 guifbd softpanel/arbiter 语义为权威。vgpud 仍是异步 virtio-gpu DMA 后端，本合线不伪装拥有完整 refresh knife 契约：

```text
svcmgr: vgpud absent  -> gfxdemo argv 追加 --refresh-guest
svcmgr: vgpud present -> gfxdemo 保持 GUI8 WM/DMA/真实输入链
```

refresh guest 的 FULL submit 改走 gfxdemo 自己的 `wid`，surface 0 继续保留为 APP 未授权全屏负例。FULL 在 wmd 合成下是窗口本地语义，FRAME_TIMING 断言只保 PRESENTED，不把半屏窗口放大成整屏 FULL。P2 future-target submit 不再先 drain 后端，wmd 保留 guifbd depth-1 挂起/压弃语义；P5 槽提交使用真实全屏 VMO 与即时 target，让 guifbd 自己落 `P5C` 证据行。产品 vgpu 链继续验证 BootProgress、透明系统栏、AOSP 三键、edge-to-edge、Recents 与软件 LTPO。

### 调度合并

以主线调度 1.1 为基底，只重放 GUI 侧四个必要面：`current_asid`、channel reply handoff、睡眠 deadline 精确武装、deadline wake 后的立即重算出选。主线 A136 环形遍历、S-6 切换记账与看门狗面不回退。writable globals 台账补入 mainline CBOX 两个全局。

### 验证拓扑内存

smoke-image 是 GUI 常驻服务与 mainline 全探针合跑的验证拓扑，不是 128MiB 产品面。本合线把其 QEMU 内存从 128MiB 提到 192MiB：GUI wmd/launcher 常驻帧面叠加 U3 四段 uutils 管道后，128MiB 下 `pthread_create` 的 eager 栈 commit 已到边界。产品镜像内存与 1080×1920@60 约束不变。

## Alternatives considered

- 直接把 GUI 分支推 main：main 有 51 个独立提交，会丢 refresh/CBOX/调度线，拒绝。
- 保留 BootProgress op7：与 INPUT_PULSE 撞号且破坏 present v2 语义，拒绝。
- 把 refresh_hz 覆写回 offset48：会破坏 mainline softpanel 率通路，拒绝。
- 让 vgpud 半伪装 hdi v3 全链：当前异步 DMA/LTPO 管线与 guifbd arbiter/latch 语义不同，假兼容会让探针变成纸面通过，拒绝。

## Consequences

合并树通过 `just check`、docs guard、writable globals、gui boundary、SDK/design ledger、berry-guiproto + BerryUI host、gui-wmd host 与 product e2e all。GUI8 vgpu 在 refresh guest 职责分离后复跑。BootProgress 现行标准升 v1.3，记录 op9 与 fbinfo 第 10 域。

## Related

- [BootProgress P1 v1.3](../../../docs/BerryBoot-BootProgress-P1-v1.3.md)
- [透明悬浮系统栏](2026-11-16-transparent-floating-system-bars.md)
- [Android 沉浸系统栏与软件 LTPO](2026-11-16-android-immersive-bars-ltpo.md)
