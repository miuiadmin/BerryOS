# 10 号：渲染卸载真相——virtio-gpu 2D 边界与自研光栅优化方向

> 定谳日期：2026-11-13。服务对象：B1 落地后的模拟器性能定标与光栅核排刀（用户令三题之一：①渲染卸载真相）。方法：外部一手直抓（Linux uapi `virtio_gpu.h` master 全文 + QEMU master `hw/display/virtio-gpu.c` 2007 行逐函数核 2D 实现面，当日 HTTP 200，产物 `/tmp/num10/`）+ 仓内源码逐行（`user/berry-gfx/src/` 光栅核 + ADR #34/#51 + 规划 :187 + 02/07/09 号对账）+ **本机 host 定标探针**（`/tmp/num10/bench/`，berry-gfx 公开面 12 场景实测，Apple M3 Pro release 构建——零仓改）。确定度：**[高]** = 一手源码/本机实测；**[中]** = 二手或时效敏感；**[低]** = 单源推测；推断一律显式标注。

---

## §0 结论速览

1. **命令集定谳：virtio-gpu 2D 命令集零渲染 op——14 条 2D 命令全部是资源管理/传输/呈现通知，无 FILL/BLIT/COPY/DRAW/BATCH 任何画图原语**[高·uapi 一手]。渲染语义只存在于 3D 命令段（CTX_*/SUBMIT_3D——virgl/gfxstream/venus 宿主渲染器路径），Mac QEMU 构建无 `*-gl` 设备（02 号 §2 本机实证）+ ADR #51 自研不引 Mesa ⇒ 模拟器档结构性出局。**「GPU 加速」在 virtio-gpu 2D 档的真实语义 = 传输卸载（DMA 环替代宿主轮询 pread）+ 宿主呈现卸载（scanout/flush→console 增量）——渲染 100% 仍由 guest CPU 在 berry-gfx soft 光栅核完成**。
2. **QEMU 侧 2D 实现面一手定谳**：`resource_create_2d` = 分配 pixman 图（资源管理）；`transfer_to_host_2d` = `iov_to_buf` 纯拷贝（行循环或整块两条路）；`resource_flush` = flush 矩形 ∩ scanout 求交后 `qemu_console_update`（damage 通知）；`set_scanout` = fb 元数据绑定——设备侧无一行画图代码 [高·源码]。
3. **术语对账**：02 号 §4.2 目标档行写明「guest 软光栅近原生」（即渲染留在 guest）——02 号口径与本判一致、无翻案；但 01 号 §5「把光栅搬出 CPU 链」「宿主 GPU 加速 2D 光栅」两处措辞过强，按本判纠偏为「把**传输与呈现**搬出 CPU 链、宿主 GPU 加速**合成**」。
4. **berry-gfx 光栅核现状 = 纯标量**：零 `core::arch`/SIMD 命中；所有写像素经 `put_px` 单口（双重界检 + u32 pack + 4B 字节拷贝/像素）。host 定标（M3 Pro）：fill 0.95 ns/px（≈3.9 cyc@4.05GHz）vs 行内 u32 循环锚 0.053 ns/px——**同机器同格式 18× 差 = 单口开销的实测证明，也 = 优化空间的上界锚**；blit 行拷已 88.8GB/s 等效（memcpy 速率，无需优化）；SmoothArgb 双线性 6.61 ns/px 全帧 13.7ms = 16.67ms 帧预算的 82%（唯一超预算路径，须保持一次性使用）[高·本机实测]。
5. **60fps@1080P 算账定谳（推荐判）：HVF 档 guest 软光栅 + 增量传输 = 够，典型帧余量 8–280×**——典型交互帧实测 0.059–0.31ms（预算 16.67ms 的 0.35–1.8%）；最坏连续场景（每帧全帧 fill/clear）1.90–1.97ms（≈12%）仍过线；唯一红线 = 每帧全帧 SmoothArgb 放大（13.7ms）——壁纸放大保持一次性（boot/换壁纸），动态面走 NN（2.56ms）或后续 span 化。TCG 档按仓内 1.7× 税锚（01 号 §1.3 ktest 前缀基准）典型帧 0.1–0.5ms 仍宽裕，全帧 smooth 必炸——TCG 定位本就是确定性/CI 档非性能档（docs/开发体验 双模式口径）。
6. **优化方向矩阵四向裁**：a) 光栅 span 化（行内快路，先于显式 NEON）= 最高性价比（fill/clear 面 10–17×、零 API 变化、像素共识对拍可执法）；b) 脏区细化 = guest 光栅面已就位（berry-ui DamageSet K≤8 + Σ×1.25 合并帽在库），真缺口在 **guifbd v2 per-rect flush**（v1 全帧 flush 使传输/宿主面每帧全帧放大——09 号 §4 挂账）+ berry-gfx 单 dirty → K 矩形透传；c) drawcall 合并 = 边际小（实测典型帧命令验证占比可忽略）；d) Vulkan 原生 = 真机第三档不变。**内核 SIMD-free 三层合同（ADR #34）不挡用户态 SIMD**：`-neon` 旗只属内核（根 `.cargo/config.toml:24`），user/ 构建显式清空 RUSTFLAGS 还原 NEON codegen——justfile :52 注释「GUI 光栅后续正要用 SIMD」在册，内核全量保存 q0..q31 + FPEN=0b11 使 guest 用户态 SIMD 合法且安全。

## §1 命令集定谳：2D 命令表零渲染 op

### 1.1 uapi 逐命令表（一手直抓，master 当日版）

Linux `include/uapi/linux/virtio_gpu.h` `enum virtio_gpu_ctrl_type` 2D 段 0x0100 起 14 条 [高·一手]：

| 命令 | 语义 | 类别 |
| --- | --- | --- |
| GET_DISPLAY_INFO 0x0100 | 查询扫描输出几何 | 查询 |
| RESOURCE_CREATE_2D | 建 2D 资源（id+格式+宽高） | 资源管理 |
| RESOURCE_UNREF | 销毁资源 | 资源管理 |
| SET_SCANOUT | 资源子矩形绑定到扫描输出 | 呈现配置 |
| RESOURCE_FLUSH | 通知「这块矩形画好了」 | 呈现通知（damage） |
| TRANSFER_TO_HOST_2D | guest 内存矩形拷入宿主资源（DMA） | 传输 |
| RESOURCE_ATTACH_BACKING | 登记资源后备 guest 物理页表 | 资源管理 |
| RESOURCE_DETACH_BACKING | 摘除后备 | 资源管理 |
| GET_CAPSET_INFO / GET_CAPSET | 查询 3D capset（virgl/venus…） | 3D 前置查询 |
| GET_EDID | 查 EDID（F_EDID 位） | 查询 |
| RESOURCE_ASSIGN_UUID | 资源 UUID（跨设备共享） | 资源管理 |
| RESOURCE_CREATE_BLOB | blob 资源（F_BLOB 位） | 资源管理 |
| SET_SCANOUT_BLOB | blob 绑定扫描输出 | 呈现配置 |

**逐条核完：无一条携带画图语义**——没有 fill（纯色填充）、没有 blit/copy（资源间或资源内像素搬移）、没有 batch/command-list 下推位；`virtio_gpu_rect` 只出现在传输（transfer 的源矩形）、呈现（flush/scanout 的通知矩形）两侧。cursor 段两条（UPDATE/MOVE_CURSOR 0x0300）是硬件光标平面（sprite），非绘制。**结论：2D 命令集 = 传输面 + 呈现通知面，定谳无渲染 op**[高]。

### 1.2 QEMU 2D 实现面：只搬运不画图（源码一手）

QEMU master `hw/display/virtio-gpu.c`（2007 行）四条像素相关命令的实现体 [高·源码]：

- `virtio_gpu_resource_create_2d`（:281-）：查重→`virtio_gpu_simple_resource_new`→pixman 格式合法性→hostmem 预算——**分配宿主侧 pixman 图，纯资源管理**；
- `virtio_gpu_transfer_to_host_2d`（:526-）：界检后两条路——部分宽走行循环逐行 `iov_to_buf(res->iov,…, t2d.r.width*bpp)`、整宽走单发整块 `iov_to_buf(…, stride*height)`——**guest→宿主纯 DMA 拷贝，零像素运算**；
- `virtio_gpu_resource_flush`（:598-）：flush 矩形与各 scanout 逐一 `qemu_rect_intersect` 求交，交矩形 `qemu_console_update`（:668-684）——**damage 通知前端（VNC/D-Bus）重绘，设备不产像素**；
- `virtio_gpu_set_scanout`（:825-）：取 pixman 图几何组 `virtio_gpu_framebuffer` 元数据绑定 scanout（含 `check_scanout_bounds` ≥16×16 执法）。

设备唯一的「绘制」= pixman 图这个**存储介质**本身；2D 路径全命令即答（`virtio_gpu_simple_process_cmd` :1181-1230 派发 + 循环尾统一回包，无 fence 挂起——02 号 §3.1 已核）。**定谳：QEMU 2D 面 = 资源管理 + 拷贝 + 通知，无渲染**[高]。

### 1.3 virgl/venus/gfxstream：真 GPU 渲染的唯一门，Mac 出局

渲染语义住 3D 段：CTX_CREATE/SUBMIT_3D（0x0200 系）携不透明命令流，由宿主渲染器（capset 枚举 VIRGL/VIRGL2/GFXSTREAM_VULKAN/VENUS，uapi 一手）解释执行——这才是「GPU 渲染」。三门全闭 [高·02 号实证 + ADR]：①本机 QEMU 构建无 `virtio-gpu-gl-device`（02 号 §2 `-device help` 实证）；②ADR #51 已裁自研栈不引 Mesa（guest 需 Gallium 级栈）；③virgl aarch64 史坑（QEMU #1141）。venus/gfxstream 同样依赖宿主渲染器构建位，同一出局面。

### 1.4 「2D 命令下推」不可能性（为什么不能自己加渲染 op）

四条路全断 [高·结构判]：①改协议加命令 = 自造非标设备，丢 Linux 驱动互操作 + QEMU 上游兼容（ADR #62 QEMU 仅作引擎子进程的整个前提）；②vhost-user-gpu（自实现 GPU 设备的正路）PCI-only 不支持 MMIO（QEMU 官方文档 + 02 号 §2 本机零命中双证）；③Homebrew QEMU 不可注入自定义设备代码（无构建面）；④「guest 渲染结果直写宿主显存」无 2D 语义位（blob/map 是 3D 族能力）。**推论：模拟器档的渲染卸载 = 结构性不可能，性能工作面唯一真路 = 自研光栅核提速 + 脏区细化（§4）**。

## §2 「GPU 加速」语义对账：加速了什么、没加速什么

| 链路环节 | ramfb 现役 | virtio-gpu 2D 档 | 加速判 |
| --- | --- | --- | --- |
| 像素计算（fill/glyph/blit/缩放） | guest CPU berry-gfx | guest CPU berry-gfx（同码） | **未加速——渲染原地** |
| guest→宿主像素搬运 | 宿主 16ms 轮询 pread 全帧（07 号 §0-4：7–9% 单核 + 静帧也读） | TRANSFER_TO_HOST_2D DMA 环 + flush 边界 | **已卸载：轮询消失、传输有界化** |
| 宿主呈现 | 标量转换 + CPU scale + softbuffer | console 增量 → VNC 矩形（09 号 76+ 帧实证）/β-1 wgpu/Metal 合成 | **已卸载：宿主面改矩形级/GPU 合成** |
| damage 语义 | ramfb 边界蒸发（02 号 §1-2） | flush 矩形端到端保真（:668-684） | **已修复：协议级 damage** |

对账三方：02 号 §4.2 三档表目标行「guest 软光栅=近原生」列原样保留——**02 号从未主张渲染下放，本判与其一致**；01 号 §5「把光栅搬出 CPU 链」「宿主 GPU 加速 2D 光栅」两处措辞应读作「把**传输链**搬出 CPU 链、宿主 GPU 加速**合成**」（B1 后以本判为准）；规划 :187 序列措辞「前两档得到『流畅/正确』，第三档才兑现『华丽』」——「流畅」的兑现面 = §5 算账的 guest 软光栅余量 + 传输/呈现卸载，与「华丽」（任意变换/混合/着色器）住第三档 Vulkan 的分工原文明晰，零冲突。

## §3 berry-gfx 光栅核现状与 host 定标

### 3.1 现役 op 清单与成本结构（源码逐行）

`Queue::submit`（queue.rs:50-134）= 四道先验拒面 → clip = 纹理界∩dirty → loadOp Clear 铺 clip → 逐命令 `raster::exec`（raster.rs:26-46）分派六命令：**FillRect / DrawLine / Blit / DrawGlyphRun / DrawImage{Bits·Icon·Argb·ScaledArgb·SmoothArgb} / Present(soft no-op)**。像素成本结构 [高·源码]：

| op | 内核循环 | 每像素工作 | 成本面判 |
| --- | --- | --- | --- |
| FillRect | 逐 y 逐 x `put_px`（raster.rs:78-90） | 双重界检 + usize 转换 + 乘加寻址 + 4B 拷贝 | **标量单口——fill/clear 主成本** |
| DrawLine | 巨跨度安全带 Bresenham（:106-163） | 同 put_px | 线长主导，量级低 |
| Blit | 逐行 `core::ptr::copy`（:168-201） | 行级 memmove | **已优等（memcpy 速率）** |
| GlyphRun→mono_blit | 逐像素位测 + put_px（:212-250） | `bits.bit()` + 分支 + put_px | 文本主成本；透明底短路零位 |
| Argb | 逐像素解包/pack + put_px（:329-368） | 通道移位 + pack | 图标/位图主成本 |
| ScaledArgb | 每目标像素一次坐标映射（:373-415） | 2×u64 乘除 + 采样 | NN 缩放 |
| SmoothArgb | 每像素 4 采样 + Q16 权重归一（:422-491） | ~4×Argb + 16 次乘 | **最重——壁纸专用** |

全库零 `core::arch` 命中（grep 实证）；唯一向量化点 = blit 行拷与 libc memcpy。v1 无 blend/无 AA/无 float（Q16 定点）——这些「缺」同时是成本下限的来源。

### 3.2 host 定标（/tmp/num10/bench，M3 Pro release，1920×1080 Rgbx）[高·实测]

| 场景 | 每帧实测 | 折算 | 帧预算占比（16.67ms） |
| --- | --- | --- | --- |
| anchor：u8 memset | （被 DSE 消除，无锚值） | — | — |
| anchor：行内 u32 循环（无界检写全帧） | **0.110 ms** | 18.8 Gpx/s / 0.053 ns/px | 0.66% |
| fill 全帧（现役 put_px 路） | **1.972 ms** | 1.05 Gpx/s / 0.95 ns/px | 11.8% |
| Clear pass 全帧（loadOp 路，同 put_px） | 1.901 ms | 1.09 Gpx/s | 11.4% |
| fill 5% 脏区（432×240） | 0.103 ms | 1.00 Gpx/s | 0.62% |
| glyph 16×32×120 字（不透明衬底） | 0.133 ms | 2.16 ns/px | 0.80% |
| glyph 32×48×60 字（透明底） | 0.078 ms | 0.85 ns/px | 0.47% |
| Argb 图标 64×64 | 0.0034 ms | 0.83 ns/px | 0.02% |
| blit 纹理内 400×300 | 0.0108 ms | 0.09 ns/px / 88.8 GB/s 等效读写 | 0.06% |
| 线 100 条×200px | 0.190 ms | — | 1.1% |
| **SmoothArgb 全帧 8× 放大（240×135→全屏）** | **13.705 ms** | 151 Mpx/s / 6.61 ns/px | **82.2%** |
| ScaledArgb NN 全帧 8× | 2.555 ms | 811 Mpx/s | 15.3% |
| **典型交互帧复合**（状态栏 400×80 fill + 12 字 32×48 + 48×48 图标 + 2 线） | **0.059 ms** | — | **0.35%** |
| 滑动最坏帧（1920×1032 blit + 1920×64 fill + 50 字） | 0.306 ms | — | 1.8% |

方法注记：探针零仓改（path 依赖 berry-gfx 公开面，非侵入）；时钟取 4.05GHz 假设标「推断」（macOS 不可读 Apple Silicon 频率；fill 0.95 ns/px ↔ 每像素 ~10 指令/IPC2.5 的结构账互证成立）；未钉核（速率量级指向 P 核；E 核约折半，推断）；HVF guest 用户态与宿主同核直执——上表可当 HVF 档近似（推断；01 号 §1.3 佐证 HVF CPU 段近原生）。

## §4 优化方向矩阵（渲染提速的真路排序）

| 方向 | 内容 | 收益上限（实测锚） | 工作量（推断） | 依赖/风险 |
| --- | --- | --- | --- | --- |
| **a1 光栅 span 化（首选）** | fill_rect/Clear 行内快路：矩形∩clip 一次求出，行内 u32 直写切片（界检外提）——不改 API、put_px 单口纪律降为行写口 | fill/clear 1.9→~0.11-0.2ms（**10–17×**，行循环锚 0.053 ns/px 为地板） | 0.5–1 人日（raster.rs 内部 + 像素共识测试照抄） | 零；像素逐位不变可对拍执法 |
| **a2 显式 NEON（第二级）** | glyph 位扩张 8px/迭代（查表/vtbl）、Argb 4px/迭代 unpack-pack、双线性 4px 交织 | glyph/Argb 2–4×、SmoothArgb 13.7→~4ms（推断） | 1.5–3 人日 + host/guest 双面 cfg | `core::arch` stable；**ADR #34 不挡**（内核 SIMD-free 三层合同只限内核；justfile :52「GUI 光栅后续正要用 SIMD」在册；TaskContext 全量保存 q0..q31 + FPEN=0b11 = guest 用户态 SIMD 合法安全）；host 测 aarch64-apple-darwin NEON 在场 |
| **b 脏区细化（传输侧为主）** | ①guifbd v2 per-rect flush（damage K≤8 → 逐矩形 transfer+flush，02 号 §3.2 Linux virtgpu_plane.c 同构流）②berry-gfx `PassDescriptor.dirty` 单矩形 → K 矩形透传（上层现以包络合并损耗）③v1.5 属性级失效（#52 既定路线不提前） | guest 光栅面零新收益（脏区已免画未变像素）；**传输/宿主面 5–20×**（02 号 §4.2：全帧→damage 5–20%）+ 帧率上限场景（大屏小动区）质变 | 1–2 人日（v2 flush）+ 0.5–1（K 透传） | INDIRECT 表已就位（09 号注记四）；QEMU 侧行循环天然支持 |
| **c drawcall 合并** | 相邻 fill 合并、重复命令折叠 | 典型帧命令验证+分派实测占比可忽略（0.059ms 全含）——**边际小** | 0.5 人日级 | 低优先；随 berry-ui dirty 合并自然发生（DamageSet 浪费帽已在库） |
| **d Vulkan 原生** | 第三档真机 TBDR（规划 :187/ADR #51 既定） | 渲染真卸载（唯一路） | 独立里程碑 | 真机 GPU 面；模拟器档不依赖 |

排序判据：a1 用最小工作量拆最大实测倍率且零语义风险；b 与 a 并行不冲突（一在 guifbd 传输面一在 berry-gfx 光栅面）；a2 等 a1 后以实测决定是否值得（典型帧已 0.35% 预算——**若只为 60fps 则 a2 可缓**，若备 4K/TCG/多应用则提前）；c 观察项；d 不变。

## §5 60fps@1080P 算账与推荐

预算：60Hz 帧预算 16.67ms（guifbd SOFT_VSYNC_NS=16_666_666≈60Hz，07 号 §2.4 时基在册）；单核 CPU 口径（现役渲染单线程）。HVF 档逐场景 [高·§3.2 实测外推，HVF≈原生标推断]：

| 场景（每帧） | 脏区像素 | 光栅实测 | 余量 |
| --- | --- | --- | --- |
| 时钟秒跳/状态栏 | ~4.7 万 | 0.059 ms | **280×** |
| 文本输入/光标行 | ~2 万 | ~0.1 ms | 160× |
| 列表滑动（最坏全幅滚动） | 200 万（blit 主） | 0.306 ms | 54× |
| 窗口拖动（600×400 blit） | 24 万（blit 主） | ~0.05 ms | 300×+ |
| 全帧重绘（clear+fill） | 207 万 | 1.9–2.0 ms | **8.4×** |
| 全帧 SmoothArgb 壁纸放大 | 207 万 | **13.7 ms** | **1.2×（贴线——禁入每帧路径）** |

传输/宿主半（叠加账）：v1 全帧 flush = guest 侧零拷贝（TRANSFER 读 guest RAM 已写好的 fb）+ QEMU `iov_to_buf` 8.29MB≈0.14ms（07 号 pread 8.35GB/s 锚同构）+ VNC 增量编码（09 号 76+ 帧实证小矩形流）——宿主合计 <1ms 量级；v2 per-rect 后典型帧传输面再降 5–20×（02 号 §4.2）。TCG 档：01 号 §1.3 ktest 前缀 1.7× 为仓内税锚（boot 首帧比值 1.34× 为 I/O 混合下界）——典型帧 0.1–0.5ms×1.7 仍宽裕，全帧 smooth 13.7×1.7=23ms 必炸；**TCG 不背 60fps KPI（确定性/CI 档，docs/开发体验 双模式口径）**。

**推荐（定谳）**：①**够**——1080P 桌面 60fps 交互在 HVF 档由「guest 软光栅 + 增量传输」承接，典型帧余量两个数量级，最坏连续场景（每帧全帧重绘）仍有 8× 余量；②唯一的结构性红线 = 每帧全帧 SmoothArgb（壁纸放大只许一次性：boot/换壁纸/转屏，动态缩放走 NN 2.6ms）；③为保余量与多应用/4K 演进，排刀顺序 = b(v2 per-rect flush，传输面) → a1(span 化，光栅面) → 复测定标 → 视需 a2(NEON)；④验收锚 = 07 号 §5 四指标同法重测（boot 首帧墙钟/变化帧数/idle CPU/壳轮询面）+ 本表场景进 GUI-9 帧时间线（input_to_present 四桶归因位已裁）。

## §6 风险与开放问题

- **定标外推面**：探针在宿主原生跑，HVF guest 含 stage-2 翻译/缓存效应未计（推断误差 <10% 量级，方向偏保守）；TCG 纯光栅循环税率未单独实测（1.7× 为 ktest 前缀锚，紧循环或更高——B-2 icount 定标后回填）。
- **put_px 单口纪律 vs 行写口**：a1 需把「所有写经 put_px」模块头纪律改为「界内行段经行写口、边界仍单口」——像素共识对拍（fill/glyph 全场景）是执法面，需同批补 host 测试。
- **SmoothArgb 每帧路径禁令的执法**：靠约定不如靠 Caps/预算断言（GUI-3 overdraw 预算断言先例）——「全帧 smooth 每帧」应在遥测可见（帧耗时桶），挂 GUI-9 四桶归因。
- **K 透传的 API 演进**：`PassDescriptor.dirty: Option<Rect>` → K≤8 数组是 berry-gfx 冻结面小改，随 v2 flush 同刀走对拍（同场景软硬双后端纪律不受影响——soft 回放语义不变）。
- **berry-fbcon 共识面**：fill 行写口改动只涉 berry-gfx raster 内部，fbcon 蜂格路径不受影响；glyph 路径共识测试在库（glyph_consensus_with_fbcon）自动覆盖。

## 来源索引

**外部一手（当日 HTTP 200，产物 `/tmp/num10/`）**：Linux uapi [virtio_gpu.h](https://github.com/torvalds/linux/blob/master/include/uapi/linux/virtio_gpu.h)（2D/3D/cursor 命令全集、capset 枚举、格式枚举——命令表定谳依据）；QEMU master [hw/display/virtio-gpu.c](https://github.com/qemu/qemu/blob/master/hw/display/virtio-gpu.c)（:281 create_2d/:526 transfer_2d/:598+668-684 flush∩scanout→console_update/:825 set_scanout/:1181-1230 simple_process_cmd——2D 实现面定谳依据；GitHub 镜像与 02 号所引 gitlab 同源）。

**本机实测（2026-11-13，Apple M3 Pro）**：`/tmp/num10/bench/`（Cargo.toml + src/main.rs——berry-gfx path 依赖 12 场景定标，§3.2 表全部读数；`anchor_memset_u8` 行如实记 DSE 消除无锚值）；隔离 `CARGO_TARGET_DIR=/tmp/num10/bench/target`（全局 ~/.cargo-target 沙箱拒锁，07 号 §4 同款坑）。

**仓内（路径:行号）**：`user/berry-gfx/src/raster.rs`（:26-46 exec 分派/:54-75 put_px/:78-90 fill/:106-163 line/:168-201 blit/:212-250 mono_blit/:258-276 glyph_run/:329-368 argb/:373-415 NN/:422-491 smooth）；`queue.rs`（:50-134 submit/:112-117 clip 构造）；`pass.rs`（:30-41 dirty 单矩形）；`cmd.rs`（:18-19 MAX_CMDS=256/:130-167 ImageSrc 变体）；`lib.rs`（:20-27 v1 操作集边界）；`font.rs`（四档图集）；`user/berry-ui/src/damage.rs`（:1-20 DamageSet K≤8 + Σ×1.25 合并帽）；`user/bins/guifbd/src/guest.rs`（:653-658 SUBMIT 全帧 flush 钩子——09 号 §4 同源）；`.cargo/config.toml:24`（-neon 旗）；`justfile:44-55`（user 构建 RUSTFLAGS="" 还原 NEON + 「GUI 光栅后续正要用 SIMD」注释）；`docs/架构决策.md`（:43 ADR #34 内核 SIMD-free 三层合同/:60 ADR #51 三档后端 + 自研光栅）；`docs/GUI里程碑规划-2026-10-v1.0.md`（:187 后端兑现序列「前两档流畅/正确、第三档华丽」）；`research/显示与加速体系设计/02-…md`（§2 Mac 实证/§3.2 per-rect 同构流/§4.1-4.2 预算口径）；`07-…md`（§0 四基线/§2.5 pread 锚/§5 重测口径）；`09-…md`（§4 β-0 增量实证/v1 全帧 flush damage=全帧矩形）；`01-…md`（§1.3 TCG/HVF 1.7× 前缀锚/§5 措辞纠偏对象）。
