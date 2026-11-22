# Agent Note: 刷新率刀4 呈现预告与回执（FRAME_TIMING 0x08 + A10 快路活证 + T3/T6 门重设计 + 遥测第一读者）

Status: implemented

## Problem

[刷新率规范 v1.0](../../../docs/刷新率体系规范-v1.0.md) §5/§7 立法了呈现预告（SUBMIT `rate_hint`，裁决面 A10「不迟一帧」）与呈现回执（P3 帧完成 + 0x08 查询面），刀3（[笔记](2026-11-16-refresh-knife3-arbiter.md)）收口仲裁器后三面欠账：A10 快路无 guest 活证（hint 到 commit 的端到端）；客户端 jank/deadline_miss/spinup_late 遥测只有 embedder 侧静态估计、无服务端单源真相接入（LTPO 切档后周期基即过期）；OP_FRAME_TIMING 因刀3 INPUT_PULSE 占 0x07 顺延 0x08 待落（规范 1.0.1 勘误随本刀）。刀4 = [分刀方案](../../plans/2026-11-15-刷新率体系实施分刀方案.md) §5。

## Decision

- **协议面**：`OP_FRAME_TIMING = 0x08` 入 present 域（域满 1..=8）；`VER_PRESENT` 1→2；应答 28B `{frame_token u32, presented_ns u64, seq u64, refresh_ns u32, flags u8}`（截断/超长/未知 flags wire 层拒——host golden 单测钉板）。gui-wmd 中继转发区间双端同步 1..=8（relay.rs expect_attach + main.rs 分发表）。
- **guifbd 回执单源**：SUBMIT 处理臂即时落 token 环四元组——`presented_ns = now`（P1 blit-at-SUBMIT 语义：呈现 = 提交处理瞬间）、`seq = panel.next_seq()`、`refresh_ns` = 切档后当前档、flags = PRESENTED|FULL；FrameTiming 查询臂按 token 查环即答。A10 消费：密集预告（hint 高于现档 ∧ 票有高帧偏好）单发即 commit（对照 streak 路 2 拍）。
- **berry-ui 遥测第一读者**：新 `telemetry` 模块（`FrameRec` 环形账本 + 计数器 frames/wakes/idle_exits/deadline_miss/spinup_late_frames）；`note_frame_timing` 采纳服务端 `refresh_ns` 为 jank 周期基、`presented_ns` 为 actual 轨——切档后静态 estimate 过期即纠。launcher frameloop 每帧 FrameCallback（P3）后追加 FrameTiming 查询喂账本，首帧打 `launcher: telem f= w= i= d= s= j=`。
- **gfxdemo 三段活证（重设计后形态，见下）**：(e) k4a A10 快路——Maintenance 驻留降 30 → 换 Interactive 120 票（Event 不积 streak）→ 单发 SUBMIT 携 rate_hint=120 → 恰 2 拍（生效窗）caps==120；判别性：无快路则 commit 落第 2 拍 streak、生效须第 4 拍。(f) k4t T3 拍点网格相位门——101 连 park OP_VBLANK_TIMING 取 `{seq, ts_ns, refresh_ns}` 三元组，残差 = |Δts − round(Δts/period)·period|，nearest-rank p99 ≤ 2ms 入门（TCG 档）；漏拍另计数不入门。(g) k4s T6 回执括号门——17 帧逐帧 {park → dup → Submit(FULL) → FrameTiming → ReleaseWait → close}，断 token 匹配、flags==PRESENTED|FULL、refresh_ns==16_666_667、括号贯穿 `parks[i] < rec_seq[i] <= parks[i+1]`，容 1 离群。
- **per-arm 回收帽**：berry-exec 拆 `reap_with_timeout(timeout_ns)`（`reap()` 委托 30s 原语义）；svcmgr gfxdemo 臂 120s，其余 arm 零扰动。
- **门值重设计（run3/4 实证驱动，本刀最大翻案）**：原案 101 帧 submit 驱动 p99 + T6 邻接等式，双重不可行——P1 下 presented_ns = 提交瞬间，客户端拉出的「呈现序列」量的是 wmd 中继 RPC 链节拍（每帧 5 RPC × TCG+宿主争用 ≈ 0.2-1.4s/帧），非拍点网格（run3/4 p99 1.37s/1.48s 恒炸 2ms 门）；邻接等式假设 1 拍/帧 cadence，TCG 下结构性不成立。改为：T3 量网格本身（纯 park 侧采名义点时刻）、T6 改 cadence 无关括号。FREE 追赶态名义网格在案（tick_vsync 用 `vb_next_ns` 标称点作 ts——迟到醒跳过已过点不耗相位），故相位残差结构性近零、粗钟只表现为漏拍。

## Alternatives considered

- 保留 submit 驱动 p99 门、放宽阈值：否——量错了对象；放宽到 RPC 节拍量级 = 门失去意义。
- 漏拍（miss）计数入门：否——K1 粗钟（BENCHWAKE p99=16.7ms）在刀5 hrtimer 前是结构性现状，入门 = 刀5 前恒红；只报数为刀5 留 before-shot（run5 miss=1497）。
- T6 邻接等式 `rec_seq[i+1] == rec_seq[i] + 1`：否——隐含 cadence 假设；括号 `(parks[i], parks[i+1]]` 对任意 cadence 成立（一号到底结构断言）。
- 全局抬 REAP_TIMEOUT：否——污染全部 arm 语义；per-arm 抬帽面最小。
- 遥测读者落 gfxdemo：否——遥测是 berry-ui 库面能力（host 可单测），launcher 是其自然消费者；gfxdemo 只做协议活证。
- 降 gfxdemo 统计段样本量（101→31）避 reap 帽：否——p99 统计效力不足；抬帽才是对症（慢的是 RPC 节拍不是统计本身）。

## Consequences

- guiproto host 测试全绿（新增 frame_timing wire golden/截断拒 + `arb_a10_hint_dense_upshift` 正负四例：非密集 Stay 且 streak 零污染、背景类 Intent 不入快路、无票密集 Stay、预告禁降档快路）；berry-ui frame.rs 全绿（新增 note_presented actual 轨/jank 边界、note_frame_timing 采纳服务端周期、telemetry 环回绕 late-fill noop）。
- **run3/4「假挂死」取证链**（误诊教训入档）：grep 只搜 `k4t|k4s` 漏了 fail() 前缀 `gfxdemo: t3 p99=` → 一小时幻影卡死排查（VBLANK park/中继白名单/控制台背压全排除）→ 插桩实锤均匀慢非卡死（~14.6 点/迭代）→ 真相：30s reap 炸 → `eco: gfx reap` → WMD_CLI_DUP 关 → launcher 臂静默跳过 → 全链标记缺席（单根因级联）；且循环本已正常走完并 fail。修法双管：门重设计（量对象纠偏）+ per-arm 120s 帽。
- smoke-image run5 全绿 exit 0：`gfx: k4a hz=120` / `gfx: k4t p99=0ns miss=1497 n=100` / `gfx: k4s n=17 bad=0` / `GFX ok` / `launcher: telem f=1 w=1 i=1 d=1 s=0 j=1` / `LAUNCH ok` / `PAGE ok n=2` / `RX ok n=3 lost=0`。p99=0ns = 名义网格相位结构性零残差的实证；miss=1497 = 101 park 窗内粗钟漏拍总数——刀5 hrtimer before-shot 基线。
- gfxdemo 栈 8→16 页：k4 双 101 数组把 main 帧推过 8 页帽，guest 冒烟 D-abort 实抓（SP 低于栈底 0x150、写落保护页）；帧内大数组留栈走抬页路，不动源（d-3c hexyl 同口径先例）。
- 头注交叉引用两处修正：gfxdemo `arb_a10_negative` → `arb_a10_hint_dense_upshift`（实名，正负例同测）；guiproto OP_FRAME_TIMING 头注「26B 应答」→ 28B（FRAME_TIMING_REP_LEN=28 处处一致）。均纯注释零码差，镜像内二进制与 run5 验证版一致。
- 终态 host 测试（显式 `--target aarch64-apple-darwin`）：guiproto 73/73（刀3 70 基础 +3）+ berry-ui 全套件绿。雷：user/ 目录裸 `cargo test` 吃 CWD 向上 config 锁靶坑报 685 假错——user 工作区测试必须显式 host 三元组（justfile `test-host` 同口径）。
- gui8-smoke 双 boot PASS：boot A `CLICK ok n=1` + LAYOUTW + APPDEMO up；boot B `LAYOUT ok n=2` 零重写断言 + 数据盘 sha256 前后一致 + LAYOUT.JSN canonical 109B 跨 boot 存活；exit 0。
- **`just smoke` 六相三跑三红全 18080 端口占（环境碰撞非回归）**：兄弟会话 berry-adv 故障注入验证战役（measure.py + toxproxy 舰队）宿主侧持 18080 + QEMU hostfwd 18081/19501/19502，轮长 ~23 分、轮隙 19s 起变长（第三跑撞在 8 分钟真空窗尾与新一轮同秒对撞）；三跑裸相+rootfs 相全绿、炸点恒在宿主 echod bind EADDRINUSE（持有者 ps 实锤归战役）。同码对照 + 场勘察 = 五形态「端口占」判例再现；修法 = 挂「四口持续自由」哨等真终局窗重跑（19s 轮间隙判别阈值 8 分钟不够——第三跑撞在 8 分钟真空窗尾与新一轮同秒对撞，抬 15 分钟），并向 berry3-b6/berry3-93/test-7c 三会话广播问询（均否认归属并助取证：属主 = dsh 宿主的非 Claude 兄弟会话，网络故障注入战役落主树 debug 内核，讯息面够不着）。08:48 四口 15 分钟持续自由（战役终局）后第四跑六相全 PASS exit 0（金丝雀 + rootfs 全链 + RX 交互相 + FAT 子目录相 + FAT 持久化相 + B4 掉电三段）。

## Related

- [刷新率体系实施分刀方案](../../plans/2026-11-15-刷新率体系实施分刀方案.md)（本刀 §5；勾选随本批）
- [刷新率刀3 笔记](2026-11-16-refresh-knife3-arbiter.md)（仲裁器前驱——A10 快路是其 on_present_hint 的第一个活消费者；op 0x07 撞位勘误源）
- [刷新率规范 v1.0](../../../docs/刷新率体系规范-v1.0.md) §5/§7（预告与回执立法；1.0.1 勘误 VER_PRESENT=2 + op 0x08）
