# Agent Note: 刷新率刀3 仲裁器（arbiter 纯核 + guifbd 活接线 + INPUT_PULSE + guest 四段活证）

Status: implemented

## Problem

[刷新率规范 v1.0](../../../docs/刷新率体系规范-v1.0.md) §6.2/§6.3 立法了裁决面 A1-A11，刀2（[笔记](2026-11-16-refresh-knife2-softpanel.md)）收口软面板后仍是「只记账不仲裁」：VOTE/POWER_MODE 收下即回执、无 Intent→档位映射、无迟滞、无触摸加速、静止停帧未收编现行零唤醒基线、切档请求与软面板 ACK 未接。刀3 = 仲裁器整刀落地（[分刀方案](../../plans/2026-11-15-刷新率体系实施分刀方案.md) §4）。

## Decision

- **纯核上架 `guiproto::arbiter`（时钟注入——host 可测）**：票面 `Ballot{sticky,intent,min,max,pref}` 单槽（A1 换票即顶替、断连即清空）；裁决 `Decision{Stay,Switch{hz,src},StopFrame,Resume}` × `Src{Vote,Dwell,Boost,Dim,Platform,Fallback}`；驱动 `Kind{Tick,Time,Event,BoostArm}`——升档 streak 只在 Tick 上积（Event/Time 不积，防票到达瞬跳）。
- **A2-A4 选档链**：全局区间 G = [max(30, 票 min), min(panel_max, 票 max)]，无解 → SAFE_HZ 60 Fallback + `warn_no_g`（读清面，guifbd 打 `PANELARB nog`）；候选 = 产品档类 ∩ G（测试档仅 det_test 面板宣告在席才入、LTPO 被下限 30 > 10 结构性排除）；A4 选档 FreeRange 区间直夹 / Tiered 取 ≥pref 最小档否则顶档。
- **A5 迟滞非对称**：升 2 拍连击、降 1000ms 驻留；同目标保 t0、换目标重锚；A6 触摸加速 = BoostArm 事件即时 min(120, gmax)（绕 streak）+ 500ms 窗，目标仍受 G 帽（票面 max 参与——boost 不破候选合法性）；A7 停帧先于迟滞且清之（资格 = quiet ∧ (无票 ∨ Intent ∈ {Background, Unspecified})）；A9 Switch → `request_refresh`，软面板拒收 → `on_switch_rejected(actual)` 回滚（升清 streak；降保守全窗重锚——commit 清驻留是既定语义）；A11 无票缺省 = select(boot 档) 非硬编码 60（`@30` 启动应回 30）。
- **Intent 单源**：guiproto `pub use berry_manifest::Intent;`（禁第三份手抄）；arbiter 内部 pattern 位置用模块私有 cast 常量（`const I_UNSPEC: u8 = Intent::Unspecified as u8;`——pattern 要 const 路径不要 cast 表达式）。
- **OP_INPUT_PULSE = 0x07 入 present 域**（域满 1..=7）；guifbd 臂 on_activity（Resume 面）+ on_input_pulse(BoostArm)；gui-wmd 中继双端区间同步（main.rs present 臂 + relay.rs expect_attach 臂）。**OP 分配冲突发现：分刀方案刀4 原文 `OP_FRAME_TIMING = 0x07` 与本刀撞——刀4 顺延 0x08（方案文本随本批修订）**。
- **guifbd 活接线四路**：`quiet_now = !(vb_armed ∥ vb_parked ∥ boost_active)`（SUBMIT/pulse 活动走 on_activity 直入、不经 quiet 布尔）；tick_vsync 每拍 `on_tick(point, quiet)`；**REG_VBLANK(0) 双空撤点路由补 on_time A7 评估**（撤点 = 不再有拍——停帧评估义务随撤点自评，否则停帧资格场景永不触发）；`arb_exec(Resume)` 不 vsync_schedule——零唤醒不变量（活动源自持调度，唯 Switch 排程驱动在途 2 帧窗）；serve 环 `arb_on_time()` + `next_wake` deadline min-arm。
- **证据行 `PANELARB sw hz=N src=…` / `stop` / `resume` / `nog`**（小写 src、无 ` ok` 形态、不撞在册 smoke 标记——串口帧镜噪声内嵌可 grep）。
- **gfxdemo 四段活证**：(0) 前置清场 POWER_NORMAL + VOTE Unspecified（**雷：前序 Interactive max=60 票会把 boost 目标帽进 G=[30,60]；DIM 残留会武装降档**）；(a) 脉冲 → `sw hz=120 src=boost` + 2 拍生效窗 caps==120；(b) Maintenance pref=30 → 1s 驻留 + 3 拍 → caps==30；(c) Background 票 + REG_VBLANK(0) 撤点 → `PANELARB stop`；重臂（新 event——旧 ev 已迁服务端 stale）→ `resume`；(d) Interactive max=120 pref=120 → 升档 2 拍连击（无驻留——TCG 鲁棒）→ `sw hz=120 src=vote` + caps==120。

## Alternatives considered

- 仲裁器落 guifbd：否——host 不可测；纯核归 guiproto + guifbd 活接线（softpanel 先例同构）。
- INPUT_PULSE 走 input 域 0x20+：否——present 域 0x07 空位在册，wmd 透传零新面；0x20+ 是 inputd 专属域（0x20/0x21 错投拒面在册）。
- boost 目标独立于票面 G：否——G 外档不破候选集合法性这一不变量（Tiered 面板 G 外档根本不存在）。
- 停帧评估只挂 tick：否——撤点后无拍可依，REG_VBLANK(0) 路由必须自评（on_time）。
- 无票缺省硬编码 60：否——A11 要求随 boot 档（@30 启动回 30）；`select(no_vote_hz)` 同一选档链出 Platform src。

## Consequences

- guiproto host 测试 70/70 绿（新增 16：A1-A11 决策表全分支一正一负、Intent 六值、区间冲突、boost 窗 + G 帽量化（Tiered max=90 → boost 落 60 = 60 起无操作；FreeRange 直落 90）、DIM 升降/地板/无票负面、停帧资格矩阵、T4 平滑门栈测 SoftPanel+Arbiter——单次切换、seq 1..=12 连续零丢、ack effective_ns = 3T、120 收敛）。
- 测试侧修正：`submit_reject_matrix` unknown-op 样本 0x07 → 0x1F（0x07 已被 INPUT_PULSE 占用——测试先于代码暴露 OP 分配冲突）。
- 四件 berry-ld 交叉链接绿（guifbd/gfxdemo/gui-wmd，worktree 本地 target）。
- smoke-image 78/78 PASS 且**一 boot 全链证据**：`sw hz=120 src=boost` / `gfx: arb1 hz=120` / `sw hz=30 src=dwell` / `gfx: arb2 hz=30` / `stop` / `resume` / `sw hz=120 src=vote` / `gfx: arb3 hz=120`；bonus 三条真行为：前序 POWER_DIM demo 被真仲裁（`sw hz=30 src=dwell` DIM 驻留降档）、POWER_NORMAL 回升 `sw hz=60 src=vote`、gfxdemo 退场后 launcher 自然 stop/resume 各一。
- **TCG 时序观察（规约内行为非 flake）**：客户端 `sleep_ns(600ms)` 在宿主争用下实测拉长 ≥1.47s → boost 窗尾中性票自然驻留降 60 可先于 (b) 票落账——驻留换目标重锚语义保终态 30 正确；demo 断言全时间鲁棒（dwell/deadline 皆服务端单调钟，客户端睡眠只作下界盖帽）。
- gui8-smoke 双 boot PASS（boot B 零重写断言在席）。
- **smoke 门三段周折（rootfs 相失败定性为外来内核投毒，非本刀回归）**：首跑缺 `AIDEMO ok`（boot 相 120s 帽被 6-QEMU 宿主负载吃掉——环境性）；重跑 boot 28/28 到席但 rootfs 相全标未到（串口只见调度演示 + `eco: cpio bad`）。取证链：宿主 python 复演 svcmgr 512B 步进窗走查同语义全绿（54 条目 + trailer 恰收档尾——字节无罪）→ multivol/旧名单双 fw_cfg 名 A/B 探针同炸（装载路径无罪）→ **boot banner 哈希 `f45b7985b-dirty` ≠ 本树 HEAD `779991d65`**——共享 target dir 的 berry-kernel 在 03:15 被主树 `wip/sched-1.1` in-flight 构建覆盖，cargo 看 mtime 新跳过重建 → 我的 smoke boot 的是别树内核 + 本树用户态（不兼容 → cpio 走查 Err；外来内核还自带 `dpg.rs` panic）。修法：touch kernel/build.rs 强刷 + 重建本树内核（5111064B，外来 6995800B）→ 150s 最小探针 boot 见 `rootfs hello` + `STD ok` + `ECO ok` 全绿。just smoke 终 PASS（重建后盯口即发第 3 窗——前 2 窗与主树 drive.py 抢口 RACE；六相全链：金丝雀 + rootfs 相 + RX 交互相 + FAT 子目录相 + FAT 持久化相 + B4 掉电相）。

## Related

- [刷新率体系实施分刀方案](../../plans/2026-11-15-刷新率体系实施分刀方案.md)（本刀 §4；刀4 OP 0x07→0x08 修订随本批）
- [刷新率刀2 笔记](2026-11-16-refresh-knife2-softpanel.md)（软面板前驱——本刀 Switch 即其 `request_refresh` 的第一个活消费者）
- [刷新率规范 v1.0](../../../docs/刷新率体系规范-v1.0.md) §6.2/§6.3（裁决面立法依据）
