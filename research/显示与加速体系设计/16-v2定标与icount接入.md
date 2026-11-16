# 16 号：v2 后三面定标 + B-2 icount 首接入显示/调度面

> 实测日期：2026-11-14。服务对象：调度+性能优化轮任务 A（v2〔853ac85c，per-rect flush + 异步化提交〕落定后的三面重测定标 + B-2 icount 定值通道首次接入显示/调度性能面）。任务：①12 号同口径三面重跑（桌面档 idle 60s CPU 面 / 滚屏档滚屏速率 / boot marker 链时序）——v2 后 vs 12 号 v1 数据 vs 07 号 ramfb 基线三方表 + 12 号 §5 60fps 差距表逐项回填；②`tools/bench.py --icount` 现役内核跑（六项 L2 对拍 + 漂移归因）+ vgpu/ramfb 桌面档 icount 轮（`VGPU2 reap` 观测行 ns→指令换算 + BENCH 六行 in-situ + switches 金丝雀定值指纹）。环境：Apple M3 Pro / 18GB / macOS 27.2.1 / QEMU 11.0.3 Homebrew + AAVMF edk2-stable202408（与 01/02/05/07/09/10/12 号同机同版本）；载体 `out/berryos.img`（sha256 `dfe3c06a…`，2026-11-14 09:17 构建——盘内 KERNEL.ELF 与仓内 `~/.cargo-target/.../berry-kernel`〔sha256 `10ff7ef0…`，02:01 构建〕内容同源：v2 提交只动 `user/bins/guifbd/*` 与 docs，kernel/ 无 .rs 晚于该 ELF〔find -newer 验证 0 命中〕，guifbd 由盘内 INITRD.BIN〔rootfs cpio〕递送——**v2 现役镜像**）；探针全落 `/tmp/b16probe/`（驱动 `round.py`/`round3.py` + 分析 `analyze.py` + 轮队列 `queue*.sh`——零仓改，本文与 README 索引行外）。方法：berry CLI `device start` 形逐参手工复刻（`tools/berry/src/qemu.rs` :196-288 的 spawn_image argv：machine virt+memory-backend-file share=on / AAVMF pflash×2 / usb-storage 盘 snapshot=on / tablet+数据盘 / ramfb / fw_cfg display `1080x1920@60` / fw_cfg rtc——vgpu 档在 ramfb 前追加 `-device virtio-gpu-device`〔12 号 wrapper 同位，`FB ok vgpu slot=29` 实证对齐〕）；CPU 面 = Python `os.wait4` rusage 全线程口径（06 号调度 §1.3-① 分线程坑的进程全量等价面）+ 成对轮差分（bootonly/full——07 号 §2.2 分离法）；串口 50ms 时戳 + marker 子串式首见（矩阵巨行免疫）；icount 轮 = 同形 + `-icount shift=1,sleep=off`。确定度：**[高·实测]** = 本机实测（双跑或双源）；**[中·模型]** = 锚实测的推演；推断一律显式标注。

---

## §0 结论速览（八条）

1. **v2 三面全绿：A/B（vgpu vs ramfb）在 CPU 面、滚屏速率面、boot marker 链面全部不可分辨（≤噪声带）——「B1/v2 的全部变化在传输/呈现侧，渲染侧分毫未动」在 v2 后依然成立**。桌面档（1080×1920,gui=1）idle 58s 窗 user：vgpu 57.92-59.43s（100-102%）/ramfb 56.59-57.85s（98-100%）；滚屏档（800×600,gui=0）滚屏段 user：vgpu 73.84/73.49s vs ramfb 73.20/73.49s（~98%）；滚屏稳态串口率：vgpu 1.7-1.8 行/s vs ramfb 1.6-1.7 行/s [高·实测，桌面三轮+滚屏两轮]。
2. **v2 同步爆点拆除在 TCG 实时面双源坐实**：`VGPU2 reap ns=` 首次回收实等时长四轮 = 123008/110992/137008/116000ns（0.111-0.137ms，均值 0.122ms）——12 号 §4.3 预言的 v1 同步模型 K+1 次 poll「9-18ms 逼近预算爆线」在 v2 异步化（2-deep reap/发车不等回执）下拆到 **0.12ms 量级**，与 853ac85c 提交自称 0.117ms 互为两源 [高·实测]。**icount 定值面**：同观测行 reap ns=11616（虚拟 ns）→ 按 shift=1 每指令 2ns 折 **≈5,808 条指令 = reap 路径自身的指令成本**（读回 used.idx + 回执桶 + 校验的整段执行——首查命中形态：设备已在帧间完成，无 sleep 步进〔11616ns < 1ms 步进〕）——且该值在桌面档（iA-r6）与滚屏档（iS2A-r4）**两形态逐值相同** = 定值性实证 [高·实测+换算]。
3. **B-2 icount 通道首次接入显示/调度面（本论首例）**：`python3 tools/bench.py --icount` 双跑逐值相同（确定性自证）= 两源；对拍 `tools/bench_baseline_icount.txt`（2026-11-05 立档）：**四项漂移出 L2 界**——`frame_alloc64k` 37→82（+121.6%，4-2/4-3 史档已知布局振荡族回摆）、`handle_evt` 165→274（+66.1%）、`vmo_commit1p` 1617→1728（+6.9%）、`vmo_rw4k` 6937→6948（+0.16%）；`cntvct_pair`/`frame_alloc4k` 逐值不动。归因〔推断〕：基线树（11-05）后九日内核提交族（P2-1 sleepq per-key 唤醒链/P2-2 退出异步化/P3-3 配额等）在事件等待/唤醒与 VMO commit 路径的指令增量（handle_evt 与 commit1p 增量 +109/+111 同量级=共路增量面貌）+ buddy 布局重掷（64k 项）。**精确归因须 bisect——本轮如实挂账不代裁**；基线刷新（`--icount --update`）属 owner 域未动 [高·实测+中·归因]。
4. **in-situ 交叉验证（四轮定值矩阵）**：icount 桌面/滚屏×vgpu/ramfb 四轮串口内 BENCH 六行——`handle_evt` 274、`vmo_rw4k` 6948、`cntvct_pair` 0 与 bench.py 裸跑形态**逐值相同**（稳定项五源锚：bench.py 双跑 + 四 icount 轮）；布局项（alloc4k/64k/commit1p）**按后端分组、组内跨形态逐值同**（vgpu {51,162,1697} 两形态同 / ramfb {218,51,1864} 两形态同 / 裸跑 {82,82,1728}——vgpu 设备=布局钥，icount「同机同模式内可比」铁律的**后端维度**注脚）[高·实测]。
5. **调度面 icount 侧写（switches 金丝雀定值指纹）**：桌面档 vgpu icount 轮 switches@marker = FBW ok 800 → FB ok vgpu 900（vio_init 相 +100）→ WM ok 1100 → GFXF ok 1500 → GFX ok 1900 → LAUNCH v= 2400——**定值可复现指纹**（ramfb 对照与双跑见 §4.4）；实时面对拍：idle 稳态 233-247 switches/s（vgpu/ramfb 同带）、滚屏稳态 141-152 switches/s——调度节律在 A/B 两后端同带 [高·实测]。
6. **idle 满核税 v2 后仍在（两后端同）——归因引 06 号调度定谳收口**：boot 演示常驻任务（task_a/task_b 忙循环 + EL0 演示舰队 + ktest 矩阵）永不退出，guest 结构性无 idle；本轮 idle 窗串口持续增量（~1.75 行/s 含 switches 金丝雀）与 06 号 §0-1/§0-2 的 PC 采样定谳一致——**非 v2/显示面事项，修窗在 06 号 §4 清单（fw_cfg 门控首刀）** [高·实测+引用]。
7. **12 号 §5 60fps 差距表回填（§3.2）**：段 T 帧提交从「v1 推断 2-4ms + v2 同步模型预爆线」更新为「v2 实测 0.111-0.137ms（TCG 实时）/ reap 路径 5.8k 指令（icount）——**出瓶颈名单**」；`VGPU2 rect` 实测 k=2 env=1080x1920+0+0 **r=1000‰**——gfxdemo 首个 per-rect 帧的包络即全帧，**v2 的 r 缩放收益在现役 TCG 桌面链上尚未兑现**（真实局部 damage 帧源=launcher，TCG 下饿死无帧——07 号 §0-2 既有）；出口通道 33fps 硬顶、段 H 双通道并存、段 G 余量——三项引 12 号原判不动（v2 不触碰该三面）。
8. **本轮环境三坑如实记**（§6）：①宿主三会话并发（SMP 内核实验轮 + B 线 slice_hz GUI 轮 + owner HVF 交互会话）——QEMU 场空预检（pgrep 等空）串行化全部轮次，受扰轮（s1-full-A-r2：user 饱和度 75%/switches 率 136/s）识别弃用重跑（r3 型受扰 = 饱和而率降 183/s——判据双看，§6-①）；②icount 宿主侧指令总量采样三通道全灭（QEMU 11.0.3 `query-cpus-fast` 无 icount 字段〔活体实探〕/HMP `info icount` unknown command〔活体实探〕/gdbstub 动态 sysreg XML 无 CNTVCT_EL0〔qXfer 实探〕）——per-marker 宿主指令链不可得，icount 面收敛为 guest 自证面（VGPU2/BENCH/BENCHWAKE/switches）；③`-gdb` 旗标本身令 icount 桌面轮挂死（iA-r5：1.52s user 后 vCPU halt 7min 硬帽收卷——gdbstub×icount 交互，弃该旗标后复现消失）。

## §1 方法（轮矩阵 + 工具口径）

### 1.1 载体与版本

| 件 | 版本/值 | 说明 |
| --- | --- | --- |
| 镜像 | `out/berryos.img`（117440512B，2026-11-14 09:17，sha256 `dfe3c06a2ac7fd727fdcf184f2e3c8da…`） | **v2 现役镜像**：盘内 KERNEL.ELF（6716400B）= v2 提交前 02:01 构建（kernel/ 无源文件晚于它——v2 只动 guifbd+docs，kernel 面与 853ac85c 等价）；guifbd v2 代码由盘内 INITRD.BIN（34975744B rootfs cpio，`VGPU2 reap ns=`/`VGPU2 rect k=` 串在场验证）递送 |
| bench 内核 | `~/.cargo-target/aarch64-unknown-none/debug/berry-kernel`（6717176B，2026-11-14 02:01，sha256 `10ff7ef0f3ab58ec85b4016e1831f4d9…`） | `tools/bench.py --icount` 通道用（`paths.sh kernel` 解析）——与镜像内 KERNEL.ELF 同树构建 |
| QEMU | 11.0.3 Homebrew + AAVMF edk2-stable202408（`/opt/homebrew/share/qemu/edk2-aarch64-code.fd`） | 与 07/09/12 号同机同版本 |
| 对照基线 | 12 号（§2.1/§2.2 v1 六轮数据）/ 07 号（§2.1/§2.2 ramfb verified 镜像轮）/ `tools/bench_baseline_icount.txt`（2026-11-05 立档六项） | 三方对比锚 |
| berry CLI 形 | 未用 CLI 本体，逐参手工复刻 spawn_image（qemu.rs :196-288） | 复刻保真面：`FB ok vgpu slot=29 pmode=1280x800`（12 号 s1A1 同值）——设备序/槽位对齐即形态对齐的实证 |

### 1.2 轮矩阵（数据 25 轮全跑通 + 通道迭代 9 轮 + bench.py 双跑）

| 轮 | 场景 | 后端 | 窗 | 采集面 |
| --- | --- | --- | --- | --- |
| s1-boot-A/B-r1..r3 | S1 桌面档（fw_cfg `1080x1920@60`，gui=1，256M，tablet+数据盘） | vgpu/ramfb | 起→GFXF ok+2s | marker 时戳（50ms 串口首见）+ wait4 rusage（boot 段 user 分离锚） |
| s1-full-A/B-r1..r3 | 同上 | 同上 | 起→GFXF ok+60s | 同上 + switches 金丝雀流（idle 窗调度节律） |
| s2-boot-A/B-r1,r2 | S2 滚屏档（09 号 m1 最小形：无 display fw_cfg → gui=0，128M，`-vnc :7`；vgpu=先声明位） | vgpu/ramfb | 起→FBSERVE ok+1s | 同上（滚屏段起点锚） |
| s2-full-A/B-r1,r2 | 同上 | 同上 | 起→FBSERVE ok+75s | 同上 + 串口行率（滚屏速率面） |
| iA-r6/iB-r6 | 桌面档 icount（s1 形 + `-icount shift=1,sleep=off`） | vgpu/ramfb | 起→GFX ok+90s（帽 420s） | 串口面：VGPU2 行/BENCH 六行/BENCHWAKE 行/switches@marker 定值指纹 |
| iS2A/iS2B-r4 | 滚屏档 icount（s2 形 + icount） | vgpu/ramfb | 起→FBSERVE ok+75s | 同上（console 路帧批 icount 面） |
| bench×2 | `python3 tools/bench.py --icount`（裸跑 B-0 形） | — | boot→BENCHWAKE（实测 2.0s 墙） | 六行 BENCH（L2 对拍）+ BENCHWAKE 观测行；回执 `out/bench-receipts/icount-20261114-*.log` |

**工具口径四则**：①CPU 分离 = `os.wait4` rusage（`ru_utime` 全线程进程全量——06 号调度 §1.3-① 实证 macOS `proc_pid_rusage(进程pid)` 不含 vCPU 线程，wait4 口径为其进程全量等价面；07 号/12 号 `/usr/bin/time` 整组口径同族）+ bootonly/full 成对差分（07 号 §2.2 先例）；②marker 时戳 = 宿主 50ms 轮询串口文件增量 + 子串首见（B-1 矩阵巨行教训同防）；③场空预检 = `pgrep -f "qemu-system.*(-machine virt|-kernel)"` 等空（bench.py qemu_preflight 同签名）——宿主三会话并发期全部轮次串行化，等待如实计；④icount 换算三式（bench.py :32-42/:172-177 单源）：shift=1 → 每指令 2ns 虚拟钟；CNTVCT（QEMU virt CNTFRQ=62.5MHz）→ 0.125 cyc/instr；故 **指令数 = cyc×8 = 虚拟 ns/2**——「周期数」在 icount 通道即指令数代理。

### 1.3 现役链观测行（v2 新增面，guifbd 侧）

- `VGPU2 reap ns={wait} k={n}`（guest.rs :1249-1252）：async 首次回收的实等时长 + 批命令数——**一次性观测行**（log_reap 单发）。
- `VGPU2 rect k={kept} env={w}x{h}+{x}+{y} r={permille}`（guest.rs :1396-1401）：per-rect 首用的矩形数 + 包络 + 脏区率千分比——12 号 §4 r 模型的串口读数源。
- console 路（gui=0）：每消息 `put_str` + `submit_full()`（guest.rs :453-465——v2 保持全帧形〔console per-rect 挂 v3〕但获异步化收益）。

## §2 三面数据（v2 后实测，A/B 对比口径）

### 2.1 S1 桌面档（1080×1920，gui=1——flush 仅 SUBMIT 路：gfxdemo 7 帧 + init 首推）

| 量 | vgpu（A） | ramfb（B） | 判读 |
| --- | --- | --- | --- |
| boot 段 QEMU user（起→GFXF ok+2s） | 7.69 / 9.39 / 8.13（均值 8.40） | 9.22 / 8.95 / 8.53（均值 8.90） | A/B 带内不可分辨（三轮交叠）——12 号 v1：7.63 vs 7.41（+3%）同判 |
| idle 58s 窗 user（full−boot 均值差分） | r1 57.92s（100%）/ r3 59.43s（102%）〔r2 受扰弃：75%〕 | r1 56.59s（98%）/ r2 57.85s（100%）/ r3 57.45s（99%） | **vgpu 不消 idle 满核税**（12 号 99.5%/99.2% 同判；归因=06 号调度 boot 演示常驻面定谳——§0-6） |
| idle 窗 switches 率 | 233 / 183 /s（r2 136 受扰弃） | 235 / 247 / 210 /s | 调度节律同带；**率是宿主负载敏感面**（switch 墙钟成本随缓存压力浮动——saturated 轮率仍可降，§6-① 口径：受扰判据=user 饱和度+率双看） |
| marker 链（段时长，范围） | FBW 8.4-9.1 \| FBSERVE +0.4-1.2 \| WM +0.8-1.3 \| GFXF +0.8-1.3 \| GFX +1.7-2.7 \| LAUNCH v= +3.1-4.9 | FBW 8.0-8.8 \| FBSERVE +0.0-0.7 \| WM +0.7-2.9 \| GFXF +0.5-1.3 \| GFX +2.2-2.2 \| LAUNCH v= +2.9-3.8 | 形状与 12 号 v1 一致：ramfb 的 FBSERVE→WM 长段（~2.5s）与 vgpu 的 FBW→FBSERVE 段（含 vio_init）各自在席；段值噪声带 ±0.8s（TCG+宿主并发） |
| FBW→GFXF 段（7 帧提交段） | 1.95 / 3.32（两轮） | 3.39 / 3.57（两轮） | vgpu 段均值 2.64 vs ramfb 3.48——**两轮同号偏移〔中·观察〕**：v1 时 3.75/3.97（vgpu 持平偏高），v2 异步化后 vgpu 段反快 ~0.8s——与 reap 0.12ms 面同向（发车不等回执把提交从段里拆出）；噪声带内不作定谳，记两轮同号 |
| `FB ok vgpu` | slot=29 pmode=1280×800（在场） | 缺席 | 后端切换生效唯一面；12 号同槽位 |
| `VGPU2 reap ns=`（TCG 实时） | 123008 / 110992 / 137008 / 116000（4 轮） | — | **0.111-0.137ms**——§0-2 |
| `VGPU2 rect` | k=2 env=1080x1920+0+0 **r=1000‰** | — | gfxdemo 首个 per-rect 帧包络=全帧——v2 字节面收益未在现役链兑现（§3.2-③） |
| LAUNCH ok | 双档均饿死（TCG 既有，09 号 §3-3/12 号 §6） | 同 | boot 首帧墙钟指标维持「不可比」口径 |

### 2.2 S2 滚屏档（800×600，gui=0——console 路每消息 put_str+submit_full 全帧批〔v2 异步形〕）

| 量 | vgpu（A） | ramfb（B） | 判读 |
| --- | --- | --- | --- |
| boot 段 QEMU user（起→FBSERVE ok+1s） | 3.94 / 4.18 | 4.25 / 3.64 | A/B 不可分辨；绝对值高于 12 号（2.4/0.9）——**树演化**（现役镜像 boot 相含 BENCH/BENCHWAKE/ktest 全批 + 更多设备面），口径以 A/B 对比为主 |
| 滚屏段 user（FBSERVE→+75s，full−boot） | 73.84 / 73.49 | 73.20 / 73.49 | ~98% 单核——12 号 99.4/99.7% 同判（满核主项=TCG vCPU 翻译） |
| 滚屏稳态串口率（t+5..+70） | 1.7 / 1.8 行/s | 1.6 / 1.7 行/s | **A/B 无差**——渲染/产出行率不随后端变（12 号 19.0/18.9 的 A/B 同判）；绝对值差 11×=**相位错位**（§2.2-注） |
| boot 相（0-9s）串口率 | ~30 行/s（A 轮实测窗） | 同量级 | 与 12 号 19 行/s 同相同量级——12 号滚屏窗覆盖 ktest 矩阵相尾段，本轮窗（ktest 先完成）覆盖稳态舰队涓流；**跨版本绝对率对比须同相**（如实标） |
| 滚屏窗 switches 率 | 148.5 / 151.8 /s | 141.9 / 140.5 /s | 同带（滚屏态比 idle 态低 ~90/s——矩阵任务阻塞节奏差异，调度面侧写） |
| LAUNCH v= | 77.5s 到达（窗尾） | 同量级 | 800×600 无 input gift（input= 空）——launcher 晚到不产出变化帧（07 号 §0-2 同族） |

**注（滚屏速率相位错位定谳）**：12 号 s2A1 的 75s 窗内 VNC 稳态 2.6-3.2 updates/s（其 §2.2）与其串口 19 行/s 并存——其窗覆盖 ktest 矩阵相（打印密集段）；本轮窗内矩阵相已于 FBSERVE 前完成（BENCHWAKE 7.0s < FBSERVE 7.9s），稳态舰队率 1.6-1.8 行/s 与 12 号 VNC 更新率（2.7/s）同量级——**console 消费面两版本同阶，串口产出面相位不同**。A/B 对比（同相同窗）不受影响。

### 2.3 本机拷贝/比对定标（沿用 12 号 §2.3 锚，未重测）

1080p 全帧 memcpy 0.119ms（69.5GB/s）/ 800×600 0.024ms / 1080p memcmp 0.167ms——本轮模型段直接引用（同机同版本，12 号 2026-11-13 立锚）。

## §3 三方表 + 60fps 差距表回填

### 3.1 三方表（v2 本轮 vs 12 号 v1 vs 07 号 ramfb 基线）

| 面 | 07 号（ramfb 现役链，verified 镜像） | 12 号 v1（B1 镜像 e316ebfe） | 本轮 v2（853ac85c 镜像） | 判 |
| --- | --- | --- | --- | --- |
| S1 idle 稳态 QEMU CPU | 60.91s/60.3s = **100.9%**（TCG headless） | vgpu 56.54 = 99.5% / ramfb 56.82 = 99.2% | vgpu 57.92-59.43 = 100-102% / ramfb 56.59-57.85 = 98-100% | 三代同判：**idle 满核税不随后端/版本消**——06 号调度定谳归因（boot 演示常驻面），修窗在 kernel 侧门控 |
| S1 boot 段 user | 23.76s（spawn→LAUNCH ok 全段，**口径不同不可比**——07 号 §5-①） | vgpu 7.63 / ramfb 7.41（→GFXF ok） | vgpu 8.40 / ramfb 8.90（→GFXF ok+2s，三轮均值） | v1→v2 A/B 差保持在噪声带内（±0.9） |
| S1 marker 链形状 | FB ok 5.95-5.98 / FBW 9.96-11.03 / LAUNCH v= 10.50-11.57 / LAUNCH ok 26.80-26.88 | vgpu 1.42/0.56/1.77/2.06/2.90；ramfb 0.00/2.72/1.24/2.39/3.43 | vgpu 0.4-1.2/0.8-1.3/0.8-1.3/1.7-2.7/3.1-4.9；ramfb 0.0-0.7/0.7-2.9/0.5-1.3/2.2/2.9-3.8 | 三代同形：ramfb FBSERVE→WM 长段 + vgpu FBW→FBSERVE（vio_init）段；v2 无形状漂移 |
| S1 FBW→GFXF（7 帧段） | —（LAUNCH ok 全段口径） | 3.75 / 3.97 | 2.64（vgpu 两轮均值）/ 3.48（ramfb） | v2 vgpu 段两轮同号偏快〔中·观察〕——异步化提交的段面投影 |
| S2 滚屏段 user | — | vgpu 67.1/67.5 / ramfb 68.9/69.1（99%+） | vgpu 73.8 / ramfb 73.2-73.5（~98%） | 同判（满核主项 TCG；绝对值随树 boot 内容涨——A/B 无差恒成立） |
| S2 滚屏速率 | 0 fps（交互零重绘） | 19.0 / 18.9 行/s（窗覆盖矩阵相） | 1.7/1.6 行/s（稳态相）+ ~30 行/s（boot 矩阵相同量级） | **A/B 无差三代恒成立**；跨版本绝对率=相位敏感（§2.2 注） |
| 帧提交观测 | —（ramfb 直写无此面） | 推断 2-4ms/帧（v1 同步 poll 模型） | **实测 reap 0.111-0.137ms**（4 轮）+ icount 5.8k 指令 | §3.2-② 回填主项 |
| VGPU2 rect（r‰ 面） | — | —（模型 r=5% 假设） | k=2 env=全帧 r=1000‰ | 现役链首帧包络=全帧——r 收益未兑现（§3.2-③） |

### 3.2 60fps 差距表回填（12 号 §5 逐项更新——「现状」列刷 v2 实测）

| 段 | 12 号 v1 现状/推演 | **v2 后现状（本轮）** | 60fps 需求 | 差距/余量 | 源 |
| --- | --- | --- | --- | --- | --- |
| G guest 光栅（HVF/TCG） | 典型帧 0.059-0.31ms；SmoothArgb 13.7ms 红线 | **不变**（v2 不碰渲染——本轮 A/B 滚屏速率/CPU 三面无差同场侧写） | ≤16.67ms | 余量 8-280×（已过线） | 10 号实测 + 本轮 A/B 侧写 |
| T 帧提交 | **推断 2-4ms/帧（12-24% 预算）；v2 同步模型 K=8 预爆线 9-18ms** | **实测 0.111-0.137ms（TCG 实时 4 轮）/ reap 路径 5.8k 指令（icount）——0.7-0.8% 预算** | ≤16.67ms | **出瓶颈名单**（余量 >120×） | 本轮实测〔12 号 §4.3 爆点预言的 v2 拆除兑现〕 |
| Q QEMU 段 | 满帧 ≈1.3ms/帧摊 + memcmp 0.55% 单核；v2 r=5% 余量 250× | 理论面不变；**r 实测=1000‰（首帧全帧包络）**——r 缩放待真实 damage 帧源 | — | 满帧口径 <8% 单核（非瓶颈）——r 兑现挂 launcher 帧源 | 12 号模型 + 本轮 rect 实测 |
| 出口通道 | VNC 33fps 硬顶（30ms 定时，源码定谳） | **不变**（v2/v1 均与出口定时无关） | 60fps | **差 1.8×，结构性——β-1 才解**（仍居差距榜首） | 12 号源码定谳（引） |
| H 宿主呈现 | 双通道并存：壳 6% 静帧税 + 变化帧四遍遍历 | **不变**（headless 形态壳轮询不跑——本轮 berry host 面零负担同 12 号；窗口形态旧税待 β-0 壳半退役） | — | β-0 壳半一刀 | 07 号 + 12 号（引） |
| idle 稳态 | ~100% 单核（真源开放） | **仍在（两后端同带 97-101%）——归因收口引 06 号调度**：boot 演示常驻面，修窗=kernel fw_cfg 门控 | — | 体验/续航面，非 60fps 卡点 | 本轮实测 + 06 号调度定谳 |

**总判更新（对 12 号 §5 总判的 v2 修订）**：60fps 差距排序在 v2 后 = **①出口通道 33fps（1.8×，须 β-1）→ ②段 H 旧壳税未退役（β-0 壳半）→ ③idle 满核税（kernel 门控修窗——06 号 §4）→ ④r 缩放兑现（须 launcher 真实 damage 帧源）**；段 T 已从榜二出列（0.12ms 实测）——「下一程 = 通道与帧源，不是更快的搬运」的 12 号判断在 v2 后进一步坐实。

## §4 B-2 icount 首接入（显示/调度面第一批数据）

### 4.1 bench.py 六项 L2 对拍（双跑逐值相同 = 确定性两源自证）

| 项 | 基线（2026-11-05 立档） | 本轮双跑 | 漂移 | 归因〔推断〕 |
| --- | --- | --- | --- | --- |
| cntvct_pair | 0 | 0 | = | 观测锚不动（收割面无扰佐证） |
| frame_alloc4k | 82 | 82 | = | buddy 4k 路径不动 |
| frame_alloc64k | 37 | 82 | **+121.6%** | 4-2/4-3 史档已知布局振荡族回摆（82↔37 镜像布局/连扫边界项——bench.py ICOUNT_TOL 台账同族） |
| handle_evt | 165 | 274 | **+66.1%** | 事件等待/唤醒路径指令增量——基线树后 sched 系提交（P2-1 sleepq per-key/P2-2 退出异步化等）候选；精确归因须 bisect（挂账） |
| vmo_commit1p | 1617 | 1728 | +6.9% | +111 与 handle_evt +109 同量级=共路增量面貌（klock/irq 包络族候选） |
| vmo_rw4k | 6937 | 6948 | +0.16% | 微量（拷贝路径稳定——GUI-0 后名项） |

- **L2 门禁 FAIL（4 项出界，rc=1 双跑一致）**——基线 9 日龄 vs 现役树的合法漂移揭露，**非 v2 事项**（v2 不进 boot 前段 bench 窗——bench 六窗前置调度器纪元且先于 guifbd spawn）。
- BENCHWAKE 观测行（R-0 面，双跑逐字节同）：`n=582 min=1193 p50=8192 p90=32768 p99=33554432 max=107587192 sig=563 tmo=19`——vs 4-3 史档 `n=210 … max=46253511`：boot 期唤醒样本量 2.8×（树 boot 内容变多），max 段位同 p99=33554432（虚拟钟等待族不变）。
- 回执：`out/bench-receipts/icount-20261114-1312*.log`（×2，serial_sha256 头注在档）。

### 4.2 语义钉板（icount cyc/ns 的两面性——本轮新增口径注）

icount 通道 cyc = 虚拟钟周期（0.125 cyc/instr）→ **指令数 = cyc×8 = 虚拟 ns/2**，但按被测路径分两面：①**CPU 工作路**（memcpy/alloc/编帧等）：cyc 直接 = 工作指令数×8——vmo_rw4k 6948 cyc = 55,584 指令/次即为例；②**等待路**（poll/sleep 等设备/事件）：cyc = 等待者经历的虚拟时长。`VGPU2 reap ns=11616` < 1ms 步进（VIO_POLL_STEP_NS）＝**未发生 sleep、首查命中**——故该 11,616ns 全部是 reap 路径自身的执行（虚拟时长 = 自身指令×2ns）→ **reap 路径指令成本 ≈ 5,808 条**（vmo_read used.idx + 回执桶读回 + batch_resps_ok 校验整段）；若曾 sleep，则 ns 含等待段（等待段在单核饱和下 ≈ 他任务指令×2ns——两段须分账）。QEMU 侧设备处理（BH/iov_to_buf memcpy）不耗 guest 指令、不入 icount 计数——「vgpu 传输面的 QEMU 侧成本」只在 TCG 实时面可见（12 号 §3.2），icount 面天然只量 guest 半边，如实分账。

### 4.3 桌面档 icount 轮（iA-r6 vgpu / iB-r6 ramfb）

| 面 | iA-r6（vgpu） | iB-r6（ramfb） | A−B 判读 |
| --- | --- | --- | --- |
| boot→LAUNCH v= 墙（icount 下参考值） | 13.86s（BERRYBOOT ok 1.59s——固件虚拟等待消除） | 12.78s（BERRYBOOT ok 1.54s） | 墙钟在 icount 下非 guest 量（宿主调度噪声）——只作到达序证 |
| BENCH 六行（in-situ） | 0/51/162/1697/6948/274 | 0/218/51/1864/6948/274 | **稳定项三源逐值同**（handle_evt 274 / vmo_rw4k 6948 / cntvct 0——跨后端跨形态）；布局项三套全不同（§4.3-注） |
| `VGPU2 reap` | **ns=11616 k=2** → ≈5,808 指令（reap 路径自身成本） | —（ramfb 相无此路） | vgpu 独有观测面；滚屏档 icount 轮（iS2A-r4）**逐值同 11616**——跨形态定值 |
| `VGPU2 rect` | k=2 env=1080x1920+0+0 r=1000‰ | — | 同上 |
| BENCHWAKE | n=570 min=1260 p50=8192 p90=32768 p99=33554432 max=100141292 sig=549 tmo=21 | n=571 min=1260 p50=8192 p90=32768 p99=33554432 max=100141334 sig=550 tmo=21 | 近逐值同（n±1、max 差 42 cyc=336 指令——wake 分布尾部的后端微扰；分位面逐值同） |
| switches@marker | FBW 800 → FBokvgpu **900** → WM 1100 → GFXF 1500 → GFX 1900 → LAUNCH v= 2400 | FBW 800 →（无 vio_init 相）→ WM 1200 → GFXF 1400 → GFX 1800 → LAUNCH v= 2300 | **vio_init 相 A +100**（4 条 cmd_sync 逐条睡=4 次切换量级 + 链路醒）；此后 ±100 振荡、净差 +100@LAUNCH——**vgpu 帧链 boot 段调度增量 ≈ +100/2400 ≈ +4%**〔高·实测——guest 状态量非墙钟〕 |

- **跨形态三源锚**：handle_evt 274 / vmo_rw4k 6948 / cntvct_pair 0 在裸跑（bench.py）与两后端桌面 icount 轮逐值同——icount 定值性的形态无关项实证；alloc4k/64k/commit1p 三套形态三套值（bench.py 裸跑 {82,82,1728} / iA {51,162,1697} / iB {218,51,1864}——128M 裸跑 vs 256M 桌面 + vgpu 设备改 DTB/内存拓扑 → buddy 布局三掷）——「同机同模式」铁律的**形态/后端维度**注脚：布局敏感项连后端都不可跨，稳定项跨形态逐值钉死。
- **vgpu−ramfb icount 差值面（任务书 B-2 项）如实降级**：宿主侧 per-marker 指令总量通道三灭（§0-8-②）→ 差值面以 guest 自证面替代：①switches@marker A/B 差（vio_init +100、净 +100——**vgpu 传输面的调度成本实测**：每条 cmd_sync/reap 睡 = 一次切换，boot 段 vgpu 多付 ~100 切换）；②reap 路径指令成本 5.8k（vgpu 独有，跨形态定值）；③TCG 实时 A/B 三面（§2）——**vgpu 传输面的 guest 指令成本 = reap 路径 5.8k 指令（读回/校验段）+ 编帧/发车工作（模型：frame_flush_batch K+1 命令 ≈ 2×(56B 编帧+vmo_write 4KiB+门铃) ≈ 数千指令级〔中·模型，锚 vmo_rw4k 55.6k 指令/4KiB 写量级〕）+ ~100 切换/Boot 段**；QEMU 侧设备处理指令不在 icount 面内（定值通道只计 guest 指令——§4.2 分账）。
- **滚屏档 icount 对（iS2A-r4 vgpu/iS2B-r4 ramfb，console 路 put_str+submit_full 全帧批）——四轮定值矩阵**：

| 面 | vgpu 桌面（iA-r6） | vgpu 滚屏（iS2A-r4） | ramfb 桌面（iB-r6） | ramfb 滚屏（iS2B-r4） |
| --- | --- | --- | --- | --- |
| BENCH 六行 | 0/51/162/1697/6948/274 | **逐值同** | 0/218/51/1864/6948/274 | **逐值同** |
| BENCHWAKE | n=570 …max=100141292 | n=570 …max=100141293（差 1 cyc） | n=571 …max=100141334 | **逐值同** |
| reap ns | 11616 | **逐值同** | — | — |
| switches@LAUNCH v= | 2400 | 2400 | 2300 | 2300 |

  **判读**：布局敏感项（alloc4k/64k/commit1p）**按后端分组、组内跨形态逐值同**（vgpu 设备改 DTB/内存拓扑=布局钥；128M↔256M、tablet/数据盘/display fw_cfg 在席与否不动它）——icount 定值性的四轮实证 + 「同机同模式」铁律的**后端维度**注脚；稳定项（handle_evt 274/vmo_rw4k 6948/cntvct 0）跨后端跨形态跨通道（bench.py 裸跑）五源逐值钉死。console 路稳态批量（每消息一批 2 命令全帧）的 per-消息成本无逐消息打点（VGPU2 行一次性立法）——总量面挂宿主通道（灭）如实标。

### 4.4 调度面侧写（switches 金丝雀 × icount 定值 + 实时节律）

- **实时节律（TCG 轮）**：idle 稳态 233-247 switches/s（A/B 同带）；滚屏稳态 141-152/s（A/B 同带）；boot 相（BENCH→FBW 段）~1700-2000 switches/首 8s。
- **定值指纹（icount 轮，@marker 累计，双后端×双形态四轮）**：vgpu {FBW 800, FBokvgpu 900, WM 1100, GFXF 1500, GFX 1900, LAUNCHv 2400}；ramfb {FBW 800, WM 1200, GFXF 1400, GFX 1800, LAUNCHv 2300}（桌面/滚屏两形态各自逐值同）——A−B 净差 +100（vio_init 相注入后全程保持）＝**vgpu 帧链 boot 段调度增量的定值读数**（每 cmd_sync/reap 睡 = 一次切换；boot 段 vgpu 帧链多付 ~100 切换 ≈ +4%）。该指纹即 B-2 观测面在显示/调度面的第一枚钉子——后续任意树变动可在同形轮重放比对（漂移=指令流变化的调度投影）。
- **换算侧写**：icount 桌面轮 GFXF→GFX 段（gfxdemo 帧相）400 switches；配合 reap 窗 5.8k 指令/帧量级 → 帧相每百次切换 ≈ 数十万指令级推进〔中·模型侧写——无宿主总量锚，标推断〕。

## §5 模型回填（12 号 §3/§4 的 v2 后状态）

1. **段 T 模型收口**：12 号 §3.1「每命令 ≈1-2ms guest 墙钟、每帧 2 命令 2-4ms〔推断〕」→ v2 实测拆解：**等待面**（reap）0.111-0.137ms TCG 实时 / 5.8k 指令 icount；**工作面**（编帧+DMA 落盘+门铃）被异步化摊入帧间 CPU 工作（正常态 reap 实等 ≈ 设备已完成的零等待——guest.rs :1190-1193 注释的「摊薄到零」机制）。v1 同步模型的 K+1 次 1ms 步进 poll 爆点（12 号 §4.3）不复在。
2. **r 模型状态**：12 号 §4.1「段 Q 随 r 线性缩、r=5% 时 20×」——机制在席（frame_flush_batch per-rect TRANSFER + 包络 FLUSH，lib.rs :247-293），但**现役链首个 per-rect 帧实测 r=1000‰**（env=全帧）——r 缩放的兑现条件 = 真实局部 damage 帧源（launcher），现役 TCG 链无此帧源（LAUNCH ok 饿死 + 交互零重绘）——**收益挂账到帧源面（β-1/launcher 复活后复测）**。
3. **VGPU2 rect 观测行的读法**（后续轮的口径）：k=2 为 gfxdemo SUBMIT 的 damage 矩形数（≤DAMAGE_MAX=8），env 为包络——r=env_area/full_area 千分比。本轮 r=1000‰ ≠ v2 无效，而是**首帧即全帧 damage**（壁纸/布局首铺）——12 号 §4.1 模型的 r=5% 场景对应交互稳态帧，非首帧。
4. **idle 满核归因链更新**：12 号 §2.1「真源开放」→ 06 号调度（2026-11-14，同镜像前代）三面定谳「boot 演示常驻面」→ 本轮 v2 复测两后端同带满核 = 该归因在 v2 后仍成立的侧写——**显示面（B1/v1/v2）与 idle 税正交性二次实证**。

## §6 环境限制如实

- **宿主三会话并发全程在场**：SMP 内核实验轮（-smp 2 裸跑循环）+ B 线 slice_hz GUI 活体轮（berry 形 TCG）+ owner HVF 交互会话——全部轮次经场空预检串行化（pgrep 等空，帽 1800s），但**非 QEMU 负载（cargo 构建等）不在预检射程**：s1-full-A-r2 受扰（idle 窗 user 75% 饱和度、switches 率 136/s）识别弃用并以 r3 补跑；**r3 型受扰轮另现**：user 仍饱和（102%）而 switches 率降至 183/s——switch 墙钟成本随宿主缓存压力浮动，**受扰判据 = user 饱和度 + switches 率双看**（新增口径：CPU 面数值附带 switches 率佐证；icount 轮的 switches@marker 计数为负载免疫变体——§4.4）。
- **icount 宿主侧指令总量通道三灭（活体实探三证）**：①QMP `query-cpus-fast` 响应无 icount 字段（11.0.3）；②HMP `info icount` = unknown command（QMP human-monitor-command 实探）；③gdbstub 动态 sysreg XML（qXfer:features 实探，239 寄存器）**无 CNTVCT_EL0**（有 CNTFRQ_EL0/CNTV_CVAL_EL0 等比较器族）——per-marker 宿主指令链结构性不可得；icount 面收敛为 guest 自证面（§4）。**-gdb 旗标另令 icount 桌面轮挂死**（iA-r5：HANDOFF ok 后 vCPU halt，1.52s user/7min 帽——gdbstub×icount 交互，弃旗标后 iA-r6 复现消失；根因未究，挂账）。
- **滚屏速率跨版本绝对值不可比（相位错位）**：§2.2 注——12 号 19 行/s 的窗覆盖 ktest 矩阵相；本轮稳态窗 1.6-1.8 行/s；同相对比（boot 相 ~30 行/s vs 19 行/s，或 A/B 内对比）才有效。
- **boot 段绝对值跨树漂**：本轮 s2 boot 段 user（~4s）vs 12 号（0.9-2.4s）——现役镜像 boot 内容（BENCH/ktest 全批 + 设备面）多于 12 号镜像；三方表以 A/B 对比与形状面为主，绝对值标「树演化」。
- **LAUNCH ok 双档饿死**（TCG 既有，09 号 §3-3）——07 号 §5-① boot 首帧墙钟指标维持不可比口径；icount 桌面轮 LAUNCH v= 到达（13.9s）但 ok 仍缺席。
- **icount 跨模式/跨机不可比铁律**（bench_baseline_icount.txt 头注）：本轮 icount 数值全部同机同模式（shift=1,sleep=off）内对比；TCG 实时面与 icount 面互不作数值换算等同（§4.2 语义钉板）——reap 0.12ms（实时）与 5.8k 指令（icount）是同一路径的两面读数，不作等价宣称。
- **HVF 档未测**（12 号 §6 同限）——icount 通道本身 TCG-only。
- **berry CLI 本体未用**（手工复刻 argv）：复刻保真以 `FB ok vgpu slot=29 pmode=1280x800`（与 12 号 wrapper 形同槽位）+ marker 链形状对齐背书；owner 在途 M（tools/berry VNC 波族）不影响本轮面。

## §7 来源索引

- **本轮实测产物**：`/tmp/b16probe/`——`round.py`（轮驱动：场空预检/50ms marker 时戳/switches 流/行率/wait4 rusage）、`round3.py`（icount 形——host 采样弃用版）、`round2.py`（gdbstub CNTVCT 探针版——通道三灭取证件）、`analyze.py`、`queue*.sh` + `queue.log`（全轮时序账）；数据轮档 `s1-*-{A,B}-r{1,2,3}/`（12）、`s2-*-{A,B}-r{1,2}/` + `s2-boot-B-r0/`（9）、`iA-r6/`、`iB-r6/`、`iS2{A,B}-r4/`（4）＝25 轮（markers.tsv/switches.tsv/rate.tsv/time.txt/qemu.cmd/serial.log）；`bench-icount.out`（bench.py 双跑全输出）；`iA-r1..r5`、`iB-r1..r4`、`iS2A-r1` 为 icount 采样通道迭代轮（QMP/gdb 取证 + -gdb 挂死轮，§6-②）。
- **仓内（HEAD `853ac85c`，路径:行号）**：`user/bins/guifbd/src/guest.rs` :87-110（v2 模块头注——2-deep reap 语义）/:453-465（console 路 put_str+submit_full——gui=0 滚屏路径）/:909-910（VIO_POLL_CAP/STEP_NS=1ms）/:1194-1256（reap 实现 + :1249-1252 VGPU2 reap 行）/:1258-1402（submit_frame per-rect 批 + :1396-1401 VGPU2 rect 行）/:1406（submit_full）/:1510（FB ok vgpu）；`user/bins/guifbd/src/lib.rs` :23-26（协议注）/:160（frame_flush_cmds v1 形保留）/:206（rects_clip_into）/:247-293（frame_flush_batch——K×TRANSFER+包络 FLUSH）/295（batch_resps_ok）；`tools/bench.py` :32-42（--icount 通道语义三条）/:170（BOOT_TIMEOUT_ICOUNT=120）/:172-177（ICOUNT_ARGS=shift=1,sleep=off + 换算注）/:181（BASELINE_ICOUNT 独立文件）/:285（ICOUNT_TOL=0.0 + 证明台账）/:1064-1171（run_icount——L2 收割）/:179-187（跨模式/跨机不可比头注立法位）；`tools/bench_baseline_icount.txt` :1-19（基线六项 + 头注）；`tools/bench_baseline_icount.notes.md`（基线史档——{0,82,37,165,1617,6937} = 2026-11-05 f4 批复验值）；`tools/berry/src/qemu.rs` :196-288（spawn_image argv——本轮手工复刻蓝本）/:264-267（display fw_cfg）；`tools/berry/src/profile.rs` :34-41（desktop profile 1080x1920@60/256M）；`kernel/src/main.rs` :4846-4924（SOAK 行——switches 读数族）。
- **谱系**：12 号（§1.2 轮矩阵/§2.1/§2.2 v1 六轮数据——三方表主锚；§2.3 拷贝定标锚；§3/§4 成本模型与 v2 预期——§5 回填对象；§6 限制清单）；07 号（§2.1/§2.2/§5——ramfb 基线锚 + 成对轮差分法）；06 号调度（`research/调度彻底计划/06-idle满核归因.md` §0-1/§0-2/§1.3-①——idle 归因定谳 + 分线程采样坑）；09 号（§1-3 槽位序/§3-3 TCG 饥饿——口径引用）；10 号（段 G 光栅定标——引）；`docs/基准与压力测试设计.md` §13（B-2 通道设计——语义三条立法位）；README.md（目录索引）。
