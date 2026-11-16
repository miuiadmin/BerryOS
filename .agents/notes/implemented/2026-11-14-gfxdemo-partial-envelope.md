# Agent Note: gfxdemo 部分包络模式——真 damage 帧源首落（VGPU2 rect r≈500‰ 首实证）

Status: implemented

## Problem

16 号定标实测：现役链首个（也是唯一）per-rect 帧来自 gfxdemo SUBMIT，`VGPU2 rect k=2 env=1080x1920+0+0 r=1000‰`——包络即全帧，v2 per-rect flush 的 r 缩放收益（12 号 §4.1 模型）在现役 TCG 桌面链上无活体证据（[16-v2定标与icount接入](../../../research/显示与加速体系设计/16-v2定标与icount接入.md) §3.2-③：真实局部 damage 帧源挂 launcher，TCG 下饿死）。18 号复核逐字确认该读数（[18-定标与调参复核](../../../research/显示与加速体系设计/18-定标与调参复核.md) §1.2）。β-1（VNC 60fps 通道）的增量粒度设计缺一枚 r<1000‰ 的实测锚点。

## Decision

- **定谳（任务②）：damage 语义在 SUBMIT 侧（声明），不在绘制侧**。guifbd `rects_clip_into`/`frame_flush_batch`（[guifbd lib.rs](../../../user/bins/guifbd/src/lib.rs) :206/:247）只消费 SUBMIT wire 上的 rect 集；绘制侧 berry-gfx pass 本就有 dirty 面（[pass.rs](../../../user/berry-gfx/src/pass.rs) :34-41，queue 屏障裁剪执法）而 gfxdemo 此前未用——「双整帧」是消费者用法非协议限制，最小改 = 消费者侧一处，协议/服务端零触碰。
- **实施（任务①）：编译分支 `PARTIAL_ENVELOPE: bool = true`**（[gfxdemo main.rs](../../../user/bins/gfxdemo/src/main.rs) :87）。为什么是编译分支而非 env/argv：berry-libc environ 是空表桩（getenv 恒空）、boot 链 svcmgr spawn `args: &[]` 恒空参——运行期参数面在本程序结构性不可达（改 spawn 面出本轮范围）。
- **绘制域 == 声明域**：pass dirty = 下半屏 `Rect::new(0, h/2, w, h-h/2)`（Clear 与全部命令裁剪到区内——顶部红带/顶部字形整条裁空、底部绿条落区内）；SUBMIT K=2 下半屏二分（`{0,960,1080,480}`+`{0,1440,1080,480}`），并集 = dirty 区 = 包络 r≈500‰——不产声明外笔迹（真 damage）。K 面与 16 号基线同形（批形状/reap 可比）。false = 基线形态（上下半帧分区 r=1000‰），A/B 回放翻此重建。
- 波一刀 B（残帧清屏）不受扰：清屏帧恒 FULL 形（K=0 + `SUBMIT_F_DAMAGE_FULL`），不触一次性 `log_rects` 观测位。

## Alternatives considered

- **env 变量开关**：弃——environ 空表桩，运行期不可达（见上）。
- **argv 参数**：弃——boot 链预拉恒空参；改 svcmgr spawn 面超本轮范围（壳族零碰纪律）。
- **服务端/协议侧改 damage 语义**：弃——定谳显示协议面本就正确（K rect 集直通设备上传），问题只在消费者用法；动 guifbd = 扩爆炸半径且 16 号基线链漂移。

## Consequences

- 标准 boot 即产真部分包络帧源：vgpu 桌面档 `VGPU2 rect` 观测行（per-rect 首用一次性）显 **r=500‰**（实测值见下）——16 号 §5-2「r 缩放收益挂账到帧源面」的首枚活体锚落位，β-1（VNC 60fps 出口通道）增量粒度设计的参考数据点在席。
- 绘制面变化：gfxdemo 自测图案上半屏不再绘制（Clear 只铺下半屏）——该帧本就是 boot 期过渡图案且随后被波一刀 B 清屏归黑覆盖，视觉面无契约。
- guifbd px() 抽样面零连带：采样点 = rect 内分位点（guest.rs :682-693），恒落声明并集内；`VGPU2 reap` 行与 16 号带（0.108-0.137ms）同量级（本轮 0.118ms）。
- 回归面：berry-gfx 114 测 / guifbd lib 13 测全绿；smoke 断言面对 rect 值零依赖（tools/ 无 rect 值断言）。

## Verification（实测 2026-11-14 深夜，TCG 桌面档 1080×1920@60/256M/tablet+数据盘，宿主并发场）

- cargo：berry-gfx host 114 测全过；guifbd lib host 13 测全过；`just build-user` 全绿（gfxdemo musl 链 + mkcpio 60 条目 + tlsscan 58 ELF 全过）；rootfs.cpio 内 bin/gfxdemo 与新构建字节级同一（newc 页对齐 walk 对拍）。
- QEMU 双态（探针镜像 = 干净 HEAD 内核 `2904910a` + 新 rootfs，sha256 `f52c71b2ff1d2037fb295819c179163e3109714fe446809f2fe5c4f7680be2ea`；每轮 timeout 帽）：
  - **vgpu 档（E 轮）**：`FB ok vgpu slot=29 pmode=1280x800`（与 16 号同槽位——形态对齐）→ `VGPU2 reap ns=118000 k=2` → **`VGPU2 rect k=2 env=1080x960+0+960 r=500‰`** → GFXF ok n=7 → GFX ok 全链绿。对照 16 号基线 `k=2 env=1080x1920+0+0 r=1000‰`：**包络 = 恰下半屏（0+960 起 960 高），r 1000‰→500‰**——任务「VGPU2 rect r<1000‰ 首实证」达成。
  - **ramfb 档（F 轮）**：FBW ok / WM ok / GFXF ok n=7 / GFX ok 标记链与 vgpu 档同形，vgpu/VGPU2 行计数 0（E=3/F=0）——**零漂移**（16 号 A/B 判读同款）。
  - smoke-image boot 相：`timeout 150 env SMOKE_PHASES=boot tools/smoke-image.sh`（镜像 /tmp 拷贝跑——`out/berryos.img` 被并行会话 qemu 持写锁）**45/45 到齐**，exit 0。
- docs_guard：`node .agents/tools/check-docs.mjs` 227 篇 / 0 警。

## 本轮踩坑（陈核断 vgpu 交接——owner 须知的镜像链风险）

- 现象：新 gfxdemo 初轮 QEMU（A/C/D 轮，含 berry CLI 精确 argv + `-device virtio-gpu-device` 注入复刻）全部 ramfb 形——`FB ok vgpu` 缺席、guifbd 零诊断、QEMU qtree 证 gpu 设备在席（slot 29）、vblk/tablet 同窗认领正常。
- 定位（排除法 + 换核判别）：唯一变量 = 内核 ELF。**`~/.cargo-target/.../berry-kernel`（02:01 构建，banner `g429675903-dirty`——v2 提交前夜脏树）断 svcmgr→guifbd 的 virtio 窗第二映射交接**（交接字节落 'n' → 缺省安全落 Ramfb——设计如此静默）。换干净 HEAD worktree 重建内核（`2904910a`，6720056B）同 INITRD 即全绿。
- 风险面：本机 `just image` 经 `tools/paths.sh kernel` 解析到该陈核（sandbox/并行会话下无人重编内核则持续）——**凡取现役 `out/berryos.img` 跑 vgpu 面前，先验 boot banner 不是 `g429675903-dirty`**；根治 = 重建内核（owner 域）。16 号盘内核（6716400B，已随镜像覆写不可考）与此陈核差 776B = 不同构建。
- 附带损失如实记：16 号实测镜像 `dfe3c06a…` 随本轮 `out/berryos.img` 重组覆灭（gitignored 可重建产物；其数据已在 16 号文内在档）。

## Related

- [16-v2定标与icount接入](../../../research/显示与加速体系设计/16-v2定标与icount接入.md)（r=1000‰ 基线——本轮对照锚）
- [18-定标与调参复核](../../../research/显示与加速体系设计/18-定标与调参复核.md)（「现役链无 r<1000‰ 触发」点名——本轮回应）
