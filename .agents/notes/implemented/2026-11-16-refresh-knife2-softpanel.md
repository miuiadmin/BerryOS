# Agent Note: 刷新率刀2 软面板件四面（softpanel 纯核 + 0x45 三元组 + fw_cfg 旋钮）

Status: implemented

## Problem

[刷新率规范 v1.0](../../../docs/刷新率体系规范-v1.0.md) §5 立法了软面板件四面，刀1（[笔记](2026-11-15-refresh-knife1-rate-path.md)）只通了能力面的档位表宣告：事件面仍停在 16B `{seq u32, ts u64}` 二元组（refresh 缺席——客户端无从感知切档）；控制面/故障注入面全无；档位周期表硬在 guifbd `mod panel`，host 不可测。刀2 = 四面一次落地（[分刀方案](../../plans/2026-11-15-刷新率体系实施分刀方案.md) §3）。

## Decision

- **纯核上架 `guiproto::softpanel`**：TIERS 九档表按概念迁移不按文件形态——从 guifbd `mod panel` 迁入 guiproto 单源（guifbd 只留 `boot_panel_tier` loud 回退活设备面）。模式机（FREE/DET）、切档 2 帧窗、故障旋钮全进纯核 → host 单测全钉板，guest 只验端到端。
- **add-only 三面（ADR #60 ⑥）**：`OP_VBLANK_TIMING = 0x45` 新 op 承 24B 三元组应答 `{seq u64, ts_ns u64, refresh_ns u32}`（VBLANK_WAIT 16B 应答 GOLDEN 冻结零动）；VER_HDI 2→3；PANEL_CAPS 应答 version 1→2 + 档位行保留字节 o+3 承首启阶段位（STAGE_V1 产品/V2 测试/V3 LTPO）。
- **boot VMO 56B→72B 尾插域 7/8**：域 7 `[model u8][mode u8][seed u32]`、域 8 `[drop_pm u16][jitter_ns u32][sw_drop u8]`。内核手工镜像打包（内核不依赖用户 crate——手镜像布局由 guiproto host 测试钉板防漂移）。
- **fw_cfg 三旋钮**（kernel `panel_cfg_req`）：`opt/berry/panel-model=tiered|freerange`、`opt/berry/panel-mode=free|det|det:SEED`（HOST_LOCK 在册不收——刀3 仲裁器）、`opt/berry/panel-fault=D,J,S` 严格三字段（多余/越界/非数字 = 整旋钮回退零）。坏值 loud `PANEL model/mode/fault fail` 行；非缺省打 `PANEL cfg …` 证据行（无 `ok` 形态——不撞在册 smoke 标记）。
- **T2 回放语义**：DET 虚钟 t0 = 1e9 + seed·6700417 精确步进，墙钟无关；**DET 关追赶**——追赶步数依赖墙钟滞后，开着必破回放。丢帧消耗 seq（诚实跳号信号），追赶跳拍不消耗 seq（保刀1 无故障跑「seq 无重复无跳号」不变量）。
- **切档 2 帧窗 + ACK**：request → pending(hz, p, left=2)；第 2 拍生效（ts 按旧钟出、refresh 自本拍报新周期、DET 钟重锚 det_base=nominal/det_k=1）；`SwitchAck.effective_ns` 记 pre-jitter 标称拍。故障 LCG 抽签序恒 jitter 先于 drop（两旋钮并存序列仍确定）。
- **guifbd 单 vb 停泊槽双面复用**：`vb_park_timing` 旗分 16B 老面 / 24B 新面，双停泊 ST_BUSY 拒——不新增第二停泊槽（wake 语义单源）。gui-wmd 中继双端区间同步 `..=OP_VBLANK_TIMING`（main.rs hdi 臂 + relay.rs expect_attach 臂）。
- **guest 面可判不变量 = 格点式**：seq 随 boot 墙钟浮动，裸行对拍 flaky；判 `ts == t0 + (seq-1)·period`（同种子双 boot 各自落格点 = 墙钟无关实证）；fault 面判 `|ts − 格点| ≤ jitter_ns`。

## Alternatives considered

- 三元组塞进 VBLANK_WAIT 16B 应答：否——GOLDEN 冻结面，改应答长即破 v1 客户端；新 op 0x45 零碰撞。
- 内核引 guiproto crate 共享打包：否——内核不依赖用户 crate 家法；手镜像 + host 钉板测试已防双写漂移。
- softpanel 模块落 guifbd：否——host 不可测；纯核归 guiproto，guifbd 只持活设备面（回退行/停泊/调度）。
- guest 裸行对拍判 T2：否——TCG boot 时序浮动使 seq 抖动；格点不变量才是墙钟无关判面（裸行同 seq 时作 bonus 报告）。

## Consequences

- guiproto host 测试 54/54 绿（新增 8：timing 编解码含 16B/24B 不混面、档位表单源镜像、period_of/quantize、域 7/8 打包往返、FREE 直通零扰、DET 回放位等、切档 2 帧窗、故障三旋钮；caps 测试重写带 stage 列 + version 2）。
- 四件套 + kernel 构建绿（guifbd/gui-wmd/gfxdemo berry-ld 交叉链接，worktree 本地 target 让行）。
- 门序全绿：smoke-image PASS（`gfx: caps model=0 n=9 cur=60` + `gfx: vb3 seq=11 ts=70522823347 refresh=16666667`——0x45 三元组 guest 首证，串口帧镜噪声内嵌可 grep）；gui8-smoke PASS（boot B 零重写断言在席）；just smoke PASS（零 FAIL）。
- 刀2 探针四 boot 全绿（/tmp/knife2-probe，拓扑照 knife1 探针）：det:42 双 boot `gfx: vb3 seq=11 ts=1448084184 refresh=16666667` 与格点 t0+10·16666667 = 1448084184 精确相等，且裸行逐字节相同（同种子全量回放 guest 实证）；freerange boot `PANEL cfg model=freerange mode=free` + `gfx: caps model=1 n=0 cur=60` + `PANEL ok hz=60`；fault boot（det:42 + 100,500000,2）`PANEL cfg model=tiered mode=det seed=42 drop=100 jit=500000 sw=2` 三旋钮证据行到席 + ts 距格点 −145158ns ∈ ±500000（jitter 只扰上报 ts、格点锚不动；10% drop 在席不误伤上报拍）。
- **教训：QEMU `-fw_cfg` 逗号撕裂**——string 值内的逗号被 QemuOpts 当选项分隔符撕开（`Invalid parameter` 即刻退出、零串口输出）；CSV 旋钮值递入必须 `,` → `,,` 转义（`string=100,,500000,,2` 递 `100,500000,2`）。smoke-image 主路零 fw_cfg 注入，此类旋钮只能走手搓 QEMU 探针面（knife1 拓扑复用）。

## Related

- [刷新率体系实施分刀方案](../../plans/2026-11-15-刷新率体系实施分刀方案.md)（本刀 §3）
- [刷新率刀1 笔记](2026-11-15-refresh-knife1-rate-path.md)（能力面前驱；本刀 caps 应答 version 抬 2 与其 1 → 2 版本门语义衔接）
