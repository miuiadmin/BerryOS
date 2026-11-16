# 15 号：wgpu 上屏 PoC——β-1 壳半卡尺（/tmp 独立实测）

> 实测日期：2026-11-14。服务对象：02 号 §2 β-1 壳半（wgpu/Metal 呈现）与 13 号 §2 排刀第 1–2 刀（B1-b 壳半 wgpu 骨架 → β-1 D-Bus 通道）。任务：`/tmp/wgpu-poc/` 独立 crate（wgpu 30.0.2 + winit 0.30.13，零仓改），合成 BGRX 共享图替身 + damage 16×10 增量上传 + 呈现链验证 + readback 逐字节对拍；卡尺 = 全帧/damage 矩形上传耗时 + Metal 后端确认 + 帧预算账 → β-1 壳半可行性判定。环境：Apple M3 Pro / 18GB / 本机（与 01–13 号同机）。方法：探针全部实跑（gauge 三轮 + 两子进程合法性探针 + 离屏全 GPU 链验证 + 窗口呈现尝试），像素正确性一律回读对拍；源码面 wgpu 30.0.2 本地 registry 直读（路径见 §7）。确定度：**[高]** = 本机实测/源码一手；推断显式标注。

---

## §0 结论速览（六条）

1. **β-1 壳半可行性判定：GO**——wgpu 30.0.2 在本机 Metal 后端（Apple M3 Pro，adapter 确认 `backend=Metal device_type=IntegratedGpu`）把 ScanoutMap 形数据（BGRX、stride 5120 类）到 GPU 纹理的整条上传链跑通：三几何全帧上传 readback 逐字节 PASS，damage K=8×(16×10) 每帧 **25–30µs（0.15–0.18% 帧预算）**，全帧最坏几何（1080×1920）**606–650µs（3.6–3.9% 预算）**，呈现链（纹理→采样→WGSL blit→渲染目标）离屏逐字节 PASS。60fps 上传面零障碍（§4/§5）。
2. **「行拷贝路径=真实形态难点」的预判在 wgpu 30 的 `write_texture` 语义下消解**：`bytes_per_row`/`offset` 对 `write_texture` **不受 256/512 对齐约束**（对齐执法仅限编码器 `copy_buffer_to_texture`/`copy_texture_to_buffer`——wgpu-types `texture.rs :844-849` 文档原文 + wgpu-core `device/queue.rs :892`「alignment not required」验证分支双源，且两探针实测 PASS：bpr=3200 全帧直配合法且内容正确、layout.offset=51264〔64B 对齐〕合法且矩形定位正确）；且 `write_texture` 内部 staging 自带 chunked 逐行重排（只搬 payload 行字节，`queue.rs :996-1040`）——**共享图 mmap 切片零用户态重排直配**即生产形态。用户态行重排对照路（B）在三个几何全面慢于直配（A），编码器逐行拷贝形态（C）慢 4–5×。残留对齐税只在 readback（截图面，bpr 须 256 对齐）。
3. **全帧直配实测**（三轮 p50）：1280×800/5120 = 303–419µs（持续 9.0–13.2GB/s）；800×600/3200 = 154–174µs（11.0–12.4GB/s）；1080×1920/4320 = 606–645µs（12.8–13.5GB/s）——为纯 memcpy 地板（72–77GB/s）的 5–8×：`write_texture` 每次调用新建 MTL TRANSIENT staging 缓冲（`resource.rs :1264-1270`）+ 首触页提交，是大帧的固定过路费；damage 小矩形（640B）则该费近零（单矩形 ≈3µs）。**13 号 §0-4-② 的算术预估（全帧 write_texture ≈ 0.12ms@1080×1920）按本文实测修正为 ≈0.62–0.65ms（×5）**——仍占预算 3.9% 非瓶颈，v1.0 升版时随 13 号 §5 清单回填。
4. **damage 上传 A/B 同效**：K=8×(16×10)/帧，A 直配切片（内部 chunked）p50 = 24.9–26.2µs，B 用户态重排紧凑 = 25.9–26.6µs——外部重排无收益，**壳半取直配**（代码最少、零像素接触、任意 stride 通吃）。
5. **呈现面如实裁**：窗口 + wgpu surface 初始化全通（1280×800 / scale 2.0 / surface_format=Bgra8Unorm / present=Fifo / Metal），但本探针运行在无可见窗会话（agent 沙箱），`get_current_texture` 恒 `Occluded`（重试 600 次 ≈3s）——**present 节拍未测**，任务书「呈现非必需如实裁」条款兑现；以离屏全 GPU 链（上传×2 → 采样 → WGSL → 渲染目标 → readback）逐字节 PASS 代偿，稳态 blit 编码+提交 p50 = 126.1µs（0.76% 预算），首帧含 MSL 编译 0.9–4.1ms（一次性）。真窗口节拍留 B1-b 壳半在有窗会话补测（壳已有 winit 先例 `tools/berry/Cargo.toml :23`，非本沙箱能力缺口）。
6. **帧预算账定谳**：60Hz（16,667µs）下 β-1 段 H（宿主呈现壳侧）= damage 帧 ≤0.2% + blit 0.76% + present（未测，Fifo vsync 计 1 帧内）+ 全帧最坏 3.9%——对照 07 号旧壳「静帧 6% 单核盲轮询 + 变化帧 ≥4 遍全帧遍历」，β-1 形态 idle 面归零（事件驱动）、变化帧 CPU 面从百分级降到千分级；段 H 从 12 号 §5 差距表第③位清账的技术路径实证成立。

## §1 方法与形态

- **crate**：`/tmp/wgpu-poc/`（`wgpu =30.0.2` + `winit 0.30.13` + pollster，release 构建，rustc 1.97.0），单 binary 四模式：`gauge`（headless 卡尺）/`offscreen`（离屏呈现链验证）/`unaligned-bpr` 与 `offset-probe`（合法性探针，子进程跑——若验证层拒绝即 panic，不杀卡尺主进程）/`window`（呈现循环 best-effort）。
- **共享图替身**：堆内 `Vec<u8>` 按 ScanoutMap 几何分配（mmap 的替身——数据面等价，11 号 §2.3 已证 mmap 读通，本 PoC 不重复 D-Bus 面）。几何三档覆盖真实形态：**GA 1280×800/stride 5120**（11 号 §2.2 实收；算术注：5120=1280×4 紧凑且恰 256 对齐，任务书「非紧凑」措辞对该几何不成立——非紧凑/非对齐面由 GB/GC 与探针覆盖）、**GB 800×600/3200**（09 号 §1-3 m1 GOP 实收，3200%256=128）、**GC 1080×1920/4320**（03 号桌面档，4320%256=224）。pixman x8r8g8b8 = BGRX 字节序直配 `Bgra8Unorm`（B/G/R/X ↔ B/G/R/A，11 号 §2.2 同判）。
- **上传路径四条**：0 纯 memcpy 地板（隔离框架开销）；A 直配 `write_texture`（data 直取替身切片，bpr=stride，零用户态重排）；B 用户态行重排到紧凑 pitch + `write_texture`（内部快路）；C `write_buffer` 原图进 staging + 逐行 `copy_buffer_to_texture`（256 对齐执法的世界，编码器形态对照，仅 GC 档测）。
- **damage 口径**：16×10 字符格矩形逐格步进（09 号 §4 / 12 号 §2.2 实测矩形形状同族），K=8/帧（guifbd DAMAGE_MAX 同值，`user/berry-ui/src/damage.rs` K≤8）；**先预写替身（模拟 QEMU 写共享图），只对上传面计时**——壳半真实成本。计时三轮（run1/2/3 全档留存），p50/p95/min/max + 持续吞吐（N 次背靠背 + 一次 GPU 同步摊还）。
- **正确性面**：每档测毕 `copy_texture_to_buffer`（bpr 按 256 对齐——读回路径的合法形态）+ 逐像素对拍确定性图案；damage 链（全帧 + 1550 帧×K 矩形流）累积态对拍。

## §2 数据（三轮实测，µs 口径 p50；完整日志见 §7）

### 2.1 全帧上传（payload：GA 4.10MB / GB 1.92MB / GC 8.29MB）

| 路径 | GA 1280×800/5120 | GB 800×600/3200 | GC 1080×1920/4320 |
| --- | --- | --- | --- |
| 0 纯 memcpy 地板 | 53.2（77.0GB/s） | 24.9（77.1GB/s） | 115.3（71.9GB/s） |
| **A 直配 write_texture（p50）** | **419.1 / 303.3 / 304.1** | **173.6 / 154.3 / 156.8** | **623.4 / 645.3 / 606.0** |
| A 持续吞吐（背靠背） | 452.5 / 309.4 / 313.1（9.0–13.2GB/s） | 174.2 / 155.0 / 159.4（11.0–12.4GB/s） | 634.3 / 649.8 / 616.3（12.8–13.5GB/s） |
| B 行重排+紧凑（p50） | 514.1 / 383.5 / 373.5 | 203.3 / 209.4 / 197.9 | 2103.6 / 830.0 / 777.0 |
| C 编码器逐行（p50，仅 GC） | — | — | 3774.2（持续 2622–4076） |
| e2e 上传+提交+GPU 完成 | 317.2 / 305.8 / 334.6 | 159.2 / 148.0 / 158.9 | 655.2 / 617.2 / 671.5 |
| readback 对拍 | PASS | PASS | PASS |

判读：①A 三几何全胜且 readback 全 PASS——**直配即生产形态**；GA 首轮 p50 偏高（419 vs 二三轮 ≈304）为 Metal transient 堆首长效应，如实记。②B 的行重排部分本身不贵（GA 60–66µs/GB 45–50µs/GC 150–191µs ≈ memcpy 地板），但叠加 `write_texture` 后全面劣于 A（用户态多搬一遍 + 同样的 per-call staging 税）；GC 首轮 B 尾部 p95 5.7ms/max 69ms（分配器尖峰），二三轮收敛。③C（write_buffer+1920 条单行 copy 命令）= 2622–4076µs，命令编码 + per-call staging 双税，出局——**编码器拷贝形态只该用在 readback**。

### 2.2 damage 矩形上传（GA，K=8×(16×10)/帧，payload 640B/矩形）

| 路径 | p50（三轮） | 持续（三轮） |
| --- | --- | --- |
| A 直配切片（内部 chunked 重排） | 26.2 / 24.9 / 25.1 | 29.6 / 27.9 / 27.8 |
| B 用户态重排紧凑（内部快路） | 26.6 / 25.9 / 26.5 | 28.5 / 27.7 / 28.4 |

判读：A≈B（单矩形 ≈3µs 含全部框架费）——`write_texture` 内部 chunked 只搬 payload 行字节（每矩形 10×64B），外部重排无收益；**K=8 帧 ≤30µs = 60fps 预算的 0.18%**。A+B 两路各 1550 帧（累计 24,800 矩形写入）后 readback 全帧 PASS。

### 2.3 离屏呈现链与窗口尝试

- **离屏全 GPU 链 PASS（两轮）**：全帧 + damage `write_texture` → 纹理 → 最近采样 → WGSL blit（强制 alpha=1.0）→ 渲染目标 → readback 逐字节一致；blit 首帧 886.7µs（含 MSL 编译，另轮 4084.8µs——一次性成本波动如实记），**稳态编码+提交 p50 = 126.1µs**。
- **窗口尝试**：窗口创建/surface 配置全通（`inner=1280x800 scale=2.0 surface_format=Bgra8Unorm present=Fifo adapter=Apple M3 Pro backend=Metal`）；`get_current_texture` 恒 `Occluded`（5ms 间隔重试 600 次）——agent 沙箱无可见窗，present 节拍与 vsync 帧格未测（如实裁，任务书预授权）。

### 2.4 合法性探针（子进程，[高·实测]）

- `unaligned-bpr`：`write_texture bpr=3200`（3200%256=128）**PASS**——验证层接受且 readback 逐字节正确。
- `offset-probe`：`layout.offset=51264`（64B 对齐、非 512/256）+ origin=(16,10) **PASS**——矩形定位正确。

## §3 源码定谳：wgpu 30 `write_texture` 的三段语义（对齐豁免 / 双路 staging / per-call 缓冲）

1. **对齐豁免**：`TexelCopyBufferLayout` 文档原文——bytes_per_row「Must be a multiple of 256 for `CommandEncoder::copy_buffer_to_texture` and `copy_texture_to_buffer` … `Queue::write_texture` does not have this requirement」（wgpu-types `src/texture.rs :844-849`）；验证代码双分支 `validate_texture_buffer_copy(…, false /* alignment not required for buffer offset or bytes per row */)`（wgpu-core `src/device/queue.rs :892`），对齐门（offset 须块大小倍数、bpr 须 256 倍）仅 `aligned=true`（编码器路径）执法（`wgpu-core/src/command/transfer.rs :480-487`）。offset 约束 = 纹素块大小（BGRA8 为 4）——64B 对齐自然合法（§2.4 探针互证）。
2. **内部 staging 双路**（`queue.rs :994-1040`）：declared bpr == align(行字节, LCD(Metal pitch=4, 块=4)) → **快路**单段拷贝（全帧紧凑 stride 必中）；不等 → **chunked 逐行重排**（只读用户切片的每行 payload 字节，写进对齐 staging）——damage 小矩形直配切片即走此路，零用户态像素接触。
3. **per-call staging 税**：`write_texture` 每次调用 `StagingBuffer::new`（`resource.rs :1264-1270`，`MemoryFlags::TRANSIENT` MTL 缓冲 + 立即 map）——大帧固定过路费 ≈ 实测吞吐（9–13.5GB/s）与 memcpy 地板（72–77GB/s）之差；640B 小缓冲则近零（≈3µs/矩形）。规避面（持久 mapped staging + `write_buffer`）留壳半优化刀，本文不预支。

实现尖角四条（B1-b 壳半直接受益，全部本 PoC 踩过）：wgpu 30 的 `present` 移到 `Queue::present(surface_texture)`（非 `SurfaceTexture::present`）；`get_current_texture` 返回 `CurrentSurfaceTexture` 枚举（`Success/Suboptimal/Timeout/Occluded/Outdated/Lost/Validation`，Occluded 须按「跳帧重试」处理——无头 CI 会撞）；`InstanceDescriptor` 新增 `display` 字段；winit 0.30.13 用 `EventLoop::run_app(&mut app)`（ApplicationHandler 形态）。

## §4 帧预算账（60Hz = 16,667µs；β-1 段 H 壳侧，全部实测锚）

| 场景 | 壳侧成本（p50，三轮区间） | 占预算 | 对照（12 号 §3 四段 / 07 号旧壳） |
| --- | --- | --- | --- |
| damage 帧（典型交互，K=8×16×10） | 上传 25–30µs + blit 126µs | **0.15–0.18% + 0.76% ≈ 0.9%** | 旧壳变化帧 ≥4 遍全帧遍历 + 标量转换（07 号 §0-4）；VNC 相每 update 全帧 Frame clone（11 号 §3 表） |
| 全帧（最坏 GC 1080×1920） | 上传 606–645µs + blit 126µs | **3.6–3.9% + 0.76% ≈ 4.7%** | 12 号段 Q QEMU 侧全帧 0.119ms + memcmp 摊 0.55% 单核；段 T guest 提交 2–4ms（12–24%，推断）——**段 H 实测比段 T 推断还小一个量级** |
| 全帧（GA 1280×800，β-1 默认 pmode） | 上传 303–419µs | 1.8–2.5% | 同上 |
| 单次同步往返（保守上界，含 GPU 完成等待） | e2e 317–672µs（三几何） | 1.9–4.0% | — |
| idle（无 UpdateMap） | **0**（事件驱动，无轮询） | 0 | 旧壳 60Hz 盲轮询静帧 6% 单核（07 号 §0-4）——β-1 结构性清零 |
| present（Fifo） | **未测**（无可见窗会话，§2.3 如实裁） | — | 60fps 供给面另需 β-1 出口通道（VNC 33fps 硬顶，12 号 §0-4）——本表只证「上传+合成不挡 60fps」 |

合成算术（显式标注「算术+锚」）：β-1+v1 全帧最坏合计 ≈ 段 T 2–4ms（推断）+ 段 Q 0.119ms + 段 H 0.77ms ≈ **2.9–4.9ms < 16.67ms**（余量 3.4–5.7×）；damage 帧 ≈ 段 T + 段 H 0.15ms 量级——**60fps 的卡点按 13 号 §2 排序仍在出口通道与段 T 异步化，不在 wgpu 上传面**。

## §5 β-1 壳半可行性判定与形态建议

**判定：GO**——wgpu 30.0.2/Metal 呈现半在本机实测立住，B1-b 壳半 wgpu 骨架可直接按下列形态落：

1. **上传形态 = mmap 直配切片**：`ScanoutMap` mmap 后按 `UpdateMap(x,y,w,h)` 矩形切 `&map[y*stride+x*4 ..]`（span=(h-1)*stride+w*4）单次 `write_texture`，bpr=stride 原样、layout.offset=0（偏移用切片起点表达，天然合法）——零用户态重排、任意 stride 通吃、内部自动 chunked；全帧同路（初始化/Resize 后首帧）。**不需要行拷贝路径、不需要紧凑重排、不需要 staging 缓冲管理**（02 号 §2「按 UpdateMap 矩形 write_texture」的原设想成立且更简）。
2. **格式**：`Bgra8Unorm` 直配（B/G/R/X ↔ B/G/R/A）；sRGB 变体（`Bgra8UnormSrgb`）裁决留壳半设计（颜色正确性 vs 直配简单性——pixman 数据无 sRGB 语义标注，直配 Unorm 为零转换保守选）。
3. **alpha 尖角**：pixman X 字节实为未初始化内存（本 PoC 合成侧固定 0xFF 只是替身口径）——**呈现着色器必须强制 alpha=1.0**（本 PoC WGSL 已按此写），否则任意 blend/合成路径会得到随机透明像素。
4. **readback（截图面）**：`copy_texture_to_buffer` bpr 须 256 对齐（§3-1）——按 align256(w*4) 落缓冲后逐行对拍/落盘，一次性成本非每帧路径。
5. **呈现循环**：Fifo + `Queue::present`；`CurrentSurfaceTexture::Occluded/Timeout` 按跳帧处理；首帧 MSL 编译 0.9–4ms 属启动面（可预热）。
6. **对账修正**：13 号 §0-4-②「壳全帧 write_texture ≈ 0.12ms/帧」升版时按本文修正为 ≈0.62–0.65ms@1080×1920（per-call staging 税 ×5，仍非瓶颈）；02 号 §2 β-1 设想的 upload 面无需修改。

## §6 风险与如实注

- **单机三轮**：与 07/11/12 号同机（M3 Pro）；绝对值跨机外推谨慎（per-call staging 税随驱动/分配器版本波动——GA 首轮 419µs vs 二三轮 304µs 即同类波动），结构性结论（直配合法/最优、A>B>C、预算量级）稳健。
- **present 未测**：无可见窗会话（Occluded ×600）；vsync 节拍、desired_maximum_frame_latency 影响、Retina scale（本机 scale=2.0，surface 2560×1600 物理像素 blit）留 B1-b 有窗会话首刀补测——离屏链 PASS + blit 126µs 编码提交已覆盖 present 以外的全部 GPU 面。
- **per-call staging 税的规避位**：持久 mapped staging + `write_buffer`（免 per-call MTL 缓冲）理论上可把全帧推回 ≈memcpy 地板 + 命令编码，但 damage 帧已 3µs/矩形、全帧 3.9% 预算——**无紧迫性**，留壳半优化刀（本文 §3-3）。
- **wgpu 版本面**：30.0.2 API 与 22–27 世代差异大（§3 尖角四条）；壳半锁 `=30.x` 语义并留升级注记。crates.io 依赖树 255 包、release 构建一次 ≈2 分钟（M3 Pro），CI 面可控。
- **「非紧凑」措辞对账**：任务书「1280×800 stride 5120 非紧凑」与算术不符（5120=1280×4 紧凑）；非紧凑/非对齐真实面已由 GB/GC 几何 + 双探针覆盖且全部 PASS——结论对该措辞不敏感，如实记。

## §7 产物与复现

`/tmp/wgpu-poc/`：`Cargo.toml`（wgpu =30.0.2 / winit 0.30.13 / pollster 0.4，`CARGO_HOME=/tmp/wgpu-poc/cargo-home`——沙箱拒写 `~/.cargo`，依赖树锁定 255 包）、`src/main.rs`（四模式全实现，1161 行含注释）、`gauge-run1/2/3.log`（三轮卡尺全档）、`offscreen-run2.log`、`window-run1/2.log`（窗口尝试两轮含 Occluded 实录）。复现：`CARGO_HOME=/tmp/wgpu-poc/cargo-home cargo run --release -- gauge|offscreen|unaligned-bpr|offset-probe|window`。数据核对源：wgpu 30.0.2 registry 源码镜像 `/tmp/wgpu-poc/cargo-home/registry/src/index.crates.io-1949cf8c6b5b557f/{wgpu-types-30.0.2/src/texture.rs, wgpu-core-30.0.2/src/device/queue.rs, wgpu-core-30.0.2/src/resource.rs, wgpu-core-30.0.2/src/command/transfer.rs, wgpu-30.0.2/src/api/*.rs}`（§3 行号即出于此）。

## 来源索引

**本 PoC 实测（2026-11-14，Apple M3 Pro / wgpu 30.0.2 / winit 0.30.13 / rustc 1.97.0）**：§2 全部读数（gauge 三轮 + offscreen 两轮 + window 两轮 + 探针双跑），readback 对拍全 PASS。

**wgpu 30.0.2 源码（本地 registry 镜像，行号 §7 路径）**：wgpu-types `src/texture.rs :834-860`（TexelCopyBufferLayout 契约 + 256 规则的执法范围原文）；wgpu-core `src/device/queue.rs :886-892`（write_texture 验证对齐豁免）`:994-1040`（快路/chunked 双路 staging）`:1264-1270` 处引用的 StagingBuffer per-call 分配（`src/resource.rs`）；wgpu-core `src/command/transfer.rs :482-487`（对齐门仅在 aligned 分支执法）；wgpu `src/api/queue.rs :253-261`（write_texture 签名）`api/queue.rs :377`（Queue::present）`api/surface_texture.rs :48-90`（CurrentSurfaceTexture 枚举）。

**仓内（路径:行号 / 相对链接）**：[02-virtio-gpu-2D档-GPU加速路径兑现设计-v1.0.md](02-virtio-gpu-2D档-GPU加速路径兑现设计-v1.0.md) §2（β-1 wgpu 呈现半原设计）/§3.3（壳 β-1 估 3–5 人日——wgpu 骨架半本文实证支撑）；[07-B1基线-ramfb链实测.md](07-B1基线-ramfb链实测.md) §0-4（旧壳 6% 静帧轮询 + 变化帧四遍遍历——段 H 对照锚）；[09-B1bc复核.md](09-B1bc复核.md) §1-3（m1 GOP 800×600/stride 3200 实收）/§4（16×10 字符格 damage 形状锚）；[11-B1宿主通道-dbus-p2p实证.md](11-B1宿主通道-dbus-p2p实证.md) §2.2（ScanoutMap 1280×800/5120/pixman x8r8g8b8 实收——本 PoC 几何与格式的直接来源）/§2.3（mmap 语义）/§4-2（β-1 八步——本 PoC 实证其中第 5–6 步 wgpu 面）/§5-5（UnknownObject 竞态——壳半 D-Bus 半未涉）；[12-B1性能定标-双后端实测与v2模型.md](12-B1性能定标-双后端实测与v2模型.md) §2.3（memcpy 69.5GB/s 锚——本文 72–77GB/s 同量级互证）/§3（四段分解——段 H 实测填充）/§0-4（VNC 33fps 硬顶）；[13-GPU加速深化综合定谳.md](13-GPU加速深化综合定谳.md) §0-4-②（0.12ms 算术预估——本文修正 ×5）/§2（排刀第 1–2 刀验收面）；`tools/berry/Cargo.toml :23`（winit 0.30 壳先例）；`user/berry-ui/src/damage.rs`（K≤8）；[README.md](README.md)（目录索引）。
