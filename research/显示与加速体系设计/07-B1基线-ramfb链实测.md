# 07 号：B1 基线——ramfb 链帧成本实测（B1-a 前后对比锚）

> 实测日期：2026-11-13。服务对象：02 号 §6 刀序 B1-a（virtio-gpu 协议 crate）——本文即 B1 前基线锚，B1 落地后按 §5 同法重测对拍。任务：现役 ramfb 链帧链基线采集（场景化帧序列/每帧成本/帧率上限/带宽面，TCG 与 HVF 两档）。探针环境：Apple M3 Pro / 18GB / hw.pagesize=16384 / QEMU 11.0.3 Homebrew（与 01/02/05 号同机同版本）。方法：全探针落 `/tmp/b1probe/`（`round.sh` 会话轮 / `mrun.sh` 探针轮 / `probe.py` host 轮询签名探针 / `pread.py` 直读探针 / `fbgeom.py`）+ `/tmp/berryos-b1base/`（HEAD `2152c71b` detached worktree 构建产物）——**零仓内改动**（本文与 README 索引行外）。

---

## §0 结论速览（六条基线事实，全部实测两源）

1. **idle 稳态满核：guest 静止、FB 静止、QEMU 满转一个核**——60s idle 场景段 CPU：TCG headless 100.9% / HVF headless 100.1% / TCG 窗口 99.9% / HVF 窗口 101.6%（§2.2，/usr/bin/time 分离法）。串口零新增（金丝雀 4 行后静止）、FB 全帧 sha256 纹丝不动，满核全在 QEMU 进程（headless 的 berry host 无轮询 ≈0；QMP `query-cpus-fast` 连发无响应阻塞 = QEMU 主循环忙的旁证）。**归因未定谳**（推断：ramfb/xhci/tablet/VNC 无客户端某面的轮询税），对 B1 的含义：换 virtio-gpu 后同场景重测，此面若消失即设备面差异的直接证据。
2. **交互零重绘：swipe×10 / tap / 200s 跨三个分钟界时钟窗，FB 字节级零变化**（§2.4，sha256 全帧对拍 + 512 点 FNV 签名 12000 次采样）——verified 镜像 + 空数据盘（`mkfat` 新盘，仅 BERRYOSVBLK! 签名）形态下，`launcher: icon decode BadMagic`（serial :189，图标源全零）后桌面不再产出变化帧；`berry swipe` CLI 返回 ok、输入链 armed（serial :192 `launcher: input poll armed`）但无重绘兑现。**现役链唯一实测真渲染帧 = boot 首帧**。
3. **boot 首帧全链成本：TCG 15.3–16.3s / HVF 11.4–11.6s**（LAUNCH v= → LAUNCH ok，launcher 首帧呈现回执过链；§2.1，各档两轮）——1080×1920 首帧（壁纸+状态栏+布局+光栅+SUBMIT blit+FRAME_CALLBACK）是现役链最重的单变化帧实测锚。boot 整段（spawn → LAUNCH ok）：TCG 26.8–26.9s / HVF 20.0–20.1s（user 23.76s / 17.44s）。
4. **host 壳面 = 7–9% 单核**（窗口形态 − headless 形态差：idle 60s 多付 4.3–5.6s，几乎全在 sys——pread 系统调用面；`probe.py` 复刻 `RamSource::try_next_frame`（全帧 pread + 512 点 FNV 签名）60Hz 节拍自测 7.0–9.4% 对拍一致）——02 号「静止帧也要全帧 pread」的定量兑现：**纯读只占 6%，标量转换/缩放/上屏被签名短路（静帧 return None），四遍全帧遍历在静帧只发生第一遍**。
5. **pread 假设验证：8.29MB 全帧读 p50 = 0.993ms**（≈8.35GB/s，page cache 热态；`pread.py` 300 次分布 min 0.753 / p90 1.107 / max 1.424ms）——02 号 §1「每变化帧全帧 pread 8.29MB」假设成立；60Hz 轮询形态纯读 = 0.060ms/16.667ms = **6.0% 单核**。
6. **HEAD 镜像不可用作基线载体（两因如实记）**：① `2152c71b` 自身不自洽——`42a794e0` 引入 kernel 侧 `rq_insert_kt/for_each_runnable/pi_rq_lock` 等五符号但漏提交 `sched/src/lib.rs` re-export 三行（owner 工作树 M 正是此补），worktree 内补齐后才可构建；② HEAD 内核 HVF 档 ktest FILE/STD 相 fatal（TAGTAB ok 后 D-abort current-EL，PC=0xffffff804004ca0c FAR=0x21038 ISS=0x4f，halt——01 号 §4-2 缺口族又一例）；TCG 档无 panic 但 ktest soak 金丝雀洪泛下 **launcher 首帧 >946s 未呈现**（3600×0.25s 上限耗尽）。基线改用 owner 现役稳定件 `/tmp/berryos-verified.img`（2026-11-13 04:15 ≈ `dae1f49d`，01 号用户在跑会话同款——两档桌面链全通）。

## §1 测量方法（形态矩阵 + 产物口径 + 限制如实）

### 1.1 产物与版本

| 件 | 版本 | 说明 |
| --- | --- | --- |
| 镜像 | `/tmp/berryos-verified.img`（117440512B，2026-11-13 04:15） | owner 现役稳定件（≈ `dae1f49d`，桌面UX-B+D 波：真时钟/三键导航），01 号 §1.2 用户在跑 HVF 桌面会话同源——「现役 ramfb 链」字面载体 |
| host 半 berry CLI | HEAD `2152c71b` worktree 构建（`/tmp/berryos-b1base/tools/berry/cargo-target/aarch64-apple-darwin/release/berry`） | 壳代码与 02 号引用行号（ram.rs/window.rs/qemu.rs）精确对齐 |
| HEAD 镜像对照 | 同 worktree `just image` 产物（sha256 `d49321ff…`） | 仅用于 §0-6 两缺陷取证，不作基线载体 |
| 数据盘 | `tools/mkfat.py` 造 1MiB FAT + `BERRYOSVBLK!` 签名（`just desktop` 同法） | **每轮私有拷贝**（QEMU raw flock 互斥——batch1 残留会话抢锁全灭的教训） |
| QEMU | 11.0.3 Homebrew，AAVMF edk2-stable202408 | 桌面档 profile=desktop（fw_cfg `1080x1920@60`，`-device ramfb` + memory-backend-file share=on） |

### 1.2 形态矩阵（每轮 = `/usr/bin/time -l` 包整会话 berry 进程，QEMU 为其子进程计入 children rusage；swipe/screenshot/probe 等 CLI 调用在 time 之外）

| 轮 | 形态 | 档 | 场景 | 用途 |
| --- | --- | --- | --- | --- |
| rI/rJ | headless `device start --foreground` | tcg/hvf | boot 完即停 | boot 段 user 分离锚 |
| rA/rC | headless | tcg/hvf | idle 60s | QEMU+guest idle 稳态 |
| rB/rD | headless | tcg/hvf | swipe×10（1.2s 间隔） | 输入链场景 CPU |
| rE–rH | window `berry shell`（winit 真窗口，用户桌面闪现） | 同上四种 | idle 60s / swipe×10 | 壳 host 半（差值） |
| rP | headless | tcg | `probe.py` 200s 时钟窗 | 帧时戳 + host 轮询自测 |
| rN4 | headless | tcg | probe 30s + swipe×10 | 交互帧观测 |
| rK/rL | headless | tcg/hvf | screenshot 连拍×72 | 工具陷阱取证（§4） |
| rL-hl-tcg-wait | headless | tcg | HEAD 镜像 boot 946s 上限 | ktest 洪泛对照 |

**观测工具三陷阱（B1 后重测必读）**：① `berry screenshot`/control 路恒走 **VNC**（`control.rs` :468-488 `VncSource::connect`）——ramfb 档 QEMU console 永不刷新（guest 不写 fw_cfg RAMFB config），截出恒为初始帧/黑（实测 6121B 恒尺寸、hash 恒定）；真值必须直读 `ram.bin`。② 串口 `FB ok` 行的 fb= **无 0x 前缀**（`fb=0000000040800000`），BerryBoot 的 `gop:` 行才带 0x——几何解析须锚整行（`fbgeom.py`）。③ ps/psamples 在本沙箱不可用（`/bin/ps: Operation not permitted`）——CPU 一律 `/usr/bin/time -l` 整组口径，进程分离靠形态差分。

## §2 数据（时间戳双源：`/usr/bin/time` + 串口 marker 到达时刻 `rounds.tsv`）

### 2.1 boot 段（起 → LAUNCH ok；marker 首见时戳）

| 相（spawn 起） | TCG（rA/rB） | HVF（rC/rD） |
| --- | --- | --- |
| → `FB ok`（guifbd/ramfb 就绪） | 5.95–5.98s | 5.70–5.73s |
| → `FBW ok`（guifbd 写自验过） | 9.96–11.03s | 7.83–8.41s |
| → `LAUNCH v=1`（launcher 进程在场+input gift） | 10.50–11.57s | 8.11–8.69s |
| → `LAUNCH ok`（首帧呈现回执过链） | 26.80–26.88s | 19.72–20.08s |
| **首帧墙钟（LAUNCH v= → LAUNCH ok）** | **15.32–16.30s** | **11.40–11.61s** |
| boot 段 user（bootonly 轮实测） | **23.76s** | **17.44s** |

### 2.2 idle 60s 稳态（user 分离 = idle 轮 − bootonly 轮；场景段墙钟 60.3s）

| 形态/档 | 段 user | CPU 单核占比 | 窗口−headless 差（壳面） |
| --- | --- | --- | --- |
| headless TCG | 60.91s | **100.9%** | — |
| headless HVF | 60.41s | **100.1%** | — |
| window TCG | 60.24s | 99.9% | user −0.67s / **sys +4.94s** |
| window HVF | 61.29s | 101.6% | user +0.88s / **sys +4.76s** |

### 2.3 swipe×10 场景（12.7s 场景段）

| 形态/档 | 段 user | CPU 单核占比 | 对 idle 的增量 |
| --- | --- | --- | --- |
| headless TCG | 17.04s | 134.2% | +33.3pp |
| headless HVF | 16.17s | 127.3% | +27.2pp |
| window TCG | 15.97s | 125.7% | +25.8pp |
| window HVF | 16.35s | 128.7% | +27.1pp |

增量 = 输入中断 + 事件链处理的账（**FB 零变化**，§0-2）——「无输出的功」。

### 2.4 帧观测（probe.py：16.667ms 节拍全帧 pread + 512 点 FNV 签名，与 `ram.rs :136-158` 同构）

- rP（TCG，200s = 12000 拍，跨 :41 :42 :43 三个分钟界）：**changes=0**；selfcpu 14.05s = **7.0% 单核**。
- rN4（TCG，30s = 1800 拍 + swipe×10）：**changes=0**；selfcpu 9.4%。
- 交互复核（活会话）：swipe/tap 前后全帧 sha256 逐字节相同（`be66cc53724cee5b…`）；FB 内容 = 壁纸渐变（`7b96/7b97…` 序列）+ 时钟区，确证非空帧而是真静止。
- screenshot 连拍（rK/rL，72 张/档）：全部同 hash（`39aa7f1310ca4eec`，VNC 黑帧）——工具陷阱 §1.2-①。

### 2.5 pread 探针（`pread.py`，run 目录 ram.bin，fb 偏移 0x800000，300 次）

```
pread 8294400B: min=0.753 p50=0.993 p90=1.107 max=1.424 mean=1.008 ms
带宽 p50 ≈ 8.35 GB/s（page cache 热态；02 号 dd 59.6GB/s 零拷贝锚的 1/7——read 拷贝路径）
60Hz 轮询形态纯读 = 0.993/16.667 = 6.0% 单核
```

## §3 量化表（B1 对比锚正式口径）

| 量 | 现役 ramfb 链实测 | 两源对拍 |
| --- | --- | --- |
| 每变化帧成本（唯一实测帧 = boot 首帧全链） | TCG 15.32–16.30s / HVF 11.40–11.61s 墙钟（boot 段 user 23.76s/17.44s 的一部分） | 01 号 §3.3 结构账（首帧 = 全量光栅 2.07M 像素 + blit + 过链）同构；HVF/TCG ≈ 1.37× 与 01 号 §1.3 前缀基准 1.7× 同向（首帧含 I/O 等待故折减） |
| 稳态变化帧率 | **0 fps**（交互/时钟 200s 窗零变化帧——空数据盘形态） | §2.4 双探针（签名 + sha256）；时基仍在（guifbd SOFT_VSYNC_NS=16.67ms + frameloop VSYNC_NS）——「60Hz 上限」只是时基上限，非产出上限 |
| idle 稳态 QEMU CPU | ~100% 单核（两档两形态，guest 静止） | time 分离法 + serial 零增 + FB 静三源互证；归因未定谳（推断=设备/循环轮询税，B1 后同法重测即对照） |
| 输入链 CPU 增量 | +25.8~33.3pp（swipe×10，无重绘兑现） | time 分离法 |
| host 壳轮询面 | 7.0–9.4% 单核（60Hz pread+签名；sys 面主导） | probe.py 自测与窗口−headless 差（+4.8~4.9s sys/60s）双源一致 |
| 全帧 pread | p50 0.993ms / 8.29MB | pread.py 300 次 + ram.rs read_at 同构复刻 |
| 带宽面（60Hz 假想满帧） | 读 497.7MB/s = 6.0% 单核；宿主带宽 59.6GB/s 占 <1% | 与 02 号 §1「慢不在带宽」结论定量互证 |

## §4 HEAD 镜像两缺陷取证（复现命令与读数）

1. **漏提交**：`git worktree add --detach /tmp/berryos-b1base 2152c71b` 后 `just build` 报 `cannot find function rq_insert_kt in crate berry_sched` ×9——`42a794e0` 动了 `sched/src/sched.rs`（:2847/:2867/:2889 有 pi_rq_lock/for_each_runnable/rq_insert_kt）但 lib.rs re-export 白名单漏五名（owner 工作树 M `sched/src/lib.rs` ±3 行正是补丁）；worktree 补齐后 `just image` 成（隔离 `CARGO_TARGET_DIR=/tmp/berryos-b1base/target`——全局 `~/.cargo-target` 属 owner 且 sandbox 拒锁）。
2. **HVF ktest fatal**（HEAD 镜像 + `--accel hvf`）：`TAGTAB ok`（boot 无条件 spawn 的 svc_tag 任务，`main.rs :1311/:3760`——noktest 门 :732 拦不住）之后 `EXCEPTION sync current-EL SPx EC=25 ISS=0x4f PC=0xffffff804004ca0c FAR=0x21038` → halt。`opt/berry/noktest` BERRY_QEMU 包装可跳 ktest::run 批（ASPACE/FRZCYC 消失实证）但拦不住 boot spawn 族。01 号 §4-2「HVF 档跑 ktest 语义 ≠ TCG」缺口族第三例之后又一例，如实立账不门控了事。
3. **TCG ktest 洪泛**（HEAD 镜像无包装）：FB ok +6.1s / LAUNCH v= +18.2s 后 **LAUNCH ok 946s 上限未现**（switches 金丝雀 + soak 相独占单核，桌面饿死）——verified 镜像 boot 相干净（金丝雀 4 行静止）是可测前提。

## §5 B1 后同法重测口径（对比锚使用说明）

1. **载体**：同一 `/tmp/berryos-verified.img` + HEAD+3 行导出补丁 worktree 的 berry CLI 重建（B1 落地后 guest 侧换 B1 产物镜像，host 半不变）；脚本 `/tmp/b1probe/{round.sh,mrun.sh,probe.py,pread.py,fbgeom.py}` 原样重放。
2. **核心对比四指标**：① boot 首帧墙钟（LAUNCH v= → LAUNCH ok，现值 TCG 15.32–16.30s / HVF 11.40–11.61s）；② 交互变化帧数（swipe×10 场景 probe changes，现值 0——B1 + guifbd virtio-gpu 后端若通，`VGPU ok` 标记在场且 damage 帧应 >0）；③ idle 稳态 QEMU CPU（现值 ~100% 单核——若 virtio-gpu 设备面取代 ramfb 后消失，即 §0-1 归因收口）；④ host 壳轮询面（现值 7–9%——β-0 增量矩形路应显著低于全帧 pread 形态）。
3. **注意**：B1-a 验收面（02 号 §6：双 boot `VGPU ok`×2 + FBW/FBSERVE/GFXF 全绿）与本基线同链重放即得；`berry screenshot` 在 virtio-gpu 档仍走 VNC 但 console 届时有真更新（flush → qemu_console_update），工具陷阱 §1.2-① 届时自动缓解。

## §6 环境限制如实

- **headless 为主**：window 轮（rE–rH）winit 真窗口在用户桌面闪现（每轮 ~90s×4），已控制为最小必要；softbuffer 上屏/缩放面（变化帧才发生）因 §0-2 零变化帧而未获实测样本——02 号 §1 结构账（207 万像素标量转换 + CPU scale + softbuffer）在该形态下无实测锚，B1 后有变化帧时补。
- **ps/psamples 不可用**（sandbox Operation not permitted）：CPU 全部 /usr/bin/time 整组口径 + 形态差分；QEMU 与 berry host 的进程内分解未做（QMP 探针阻塞弃用）。
- **verified 镜像非 HEAD**（§0-6 决策链）：基线口径 = 「现役桌面」（owner 稳定件），HEAD 缺陷如实另记；B1 后重测若用 HEAD+fix 镜像，boot 段绝对值会漂（ktest 相差异），对比指标用 §5-②③④（对 boot 相不敏感）为主。
- 时钟场景：verified 内核（dae1f49d）真时钟首摆 + 每分钟续摆在 headless 空盘形态未产出变化帧（§2.4）——01 号 §1.2 用户会话时钟重摆是 owner 工作树（M 版 launcher/berry）行为，与本基线口径不同源，如实分账。

## §7 来源索引

- 探针与数据：`/tmp/b1probe/`（rounds.tsv 全轮时戳 / rA–rJ time.txt+serial.log / rK–rP frames.tsv+probe.log / pread.py 输出在 §2.5 正文）；worktree `/tmp/berryos-b1base`（sched lib.rs 三行导出补丁 diff 见 §4-1）。
- 仓内（HEAD `2152c71b`，路径:行号）：`tools/berry/src/display/ram.rs` :63-99（open/offset=fb_pa−0x40000000）、:136-158（全帧 pread→512 点 FNV 后验签名→静帧 None——probe.py 同构蓝本）、:164-177（变化帧标量转换，本轮零样本）；`tools/berry/src/control.rs` :468-488（screenshot 恒 VNC——陷阱①）；`tools/berry/src/window.rs` :930-934（WaitUntil+16ms 轮询节拍）；`tools/berry/src/qemu.rs` :196-201（memory-backend-file share=on）、:242-249（ramfb/fw_cfg display）；`tools/berry/src/device.rs` :44-62（run 目录 ram.bin 路径）；`user/bins/guifbd/src/main.rs` :77-78（SOFT_VSYNC_NS=16,666,666）；`user/bins/berry-launcher/src/frameloop.rs` :95（VSYNC_NS）、:520（LAUNCH ok 打点）、:543-572（自测 swipe——smoke 门控，空盘不跑）；`kernel/src/main.rs` :726-732（noktest 门）、:1311/:3760（svc_tag 无条件 spawn）；`kernel/src/bootparams.rs` :31（eco 打点形态——任务书「eco 形态」定谳：QEMU 可观测串口打点行风格，本基线按 marker 到达时戳采集）；`sched/src/sched.rs` :2847/:2867/:2889（漏导出五函数实存）。
- 01 号（`01-HVF真相与页粒度.md`）：§1.2 用户会话取证（verified 同源）、§1.3 双档 boot 基准、§3.3 渲染链四段账、§4-2 HVF ktest 缺口族、§6 复现纪律（BERRY_HOME 短路径/数据盘私有拷贝/按 run 目录点名清场——本轮全部遵守，零误杀）。
- 02 号（`02-virtio-gpu-2D档-GPU加速路径兑现设计-v1.0.md`）：§1 逐环节账（对拍 §3 表）、§4 帧预算（497.7MB/s/59.6GB/s）、§6 刀序（本文=B1-a 前锚）。
- 05 号（`05-S1spike-三验实录.md`）：S1 GO 三验 + 三陷阱（virtio-gpu-device 全名/force-legacy=false/INDIRECT 默认 on——B1 重测时同批适用）。
