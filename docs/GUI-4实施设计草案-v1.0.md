# GUI-4 段① 实施设计草案：输入协议栈三件（berry-guiproto / virtio-input / gui-inputd）

> 版本：**1.0**（v1.0）｜2026-11-06

> 状态：草案（2026-10-29，GUI-4 段①设计员产出）。impl 代理（I1/I2/I3）以本文为唯一实施依据；
> §6 钉板接口**逐字冻结**——impl 不得改签名、只能补实现体与测试。与本文冲突处以本文为准，
> 与 `docs/GUI里程碑规划-2026-10-v1.0.md` 冲突处以规划为准并回报设计员。

## §0 范围裁定（工作区边界 + 段①/段②切分）

### 0.1 工作区边界（最高优先级纪律，违者产出作废）

本刀期间姊妹会话在途，以下一律**禁改**（只读允许）：`kernel/**`、`arch/**`、
`.cargo/config.toml`、`Cargo.lock`、`justfile`、`tools/smoke.sh`、`tools/soak.sh`、
`tools/genfbfont.py`、`docs/GUI里程碑规划-2026-10-v1.0.md`、`docs/架构决策.md`、
`docs/阶段沿革.md`、`docs/事件循环A2设计草案-v1.0.md`、`docs/文件系统设计-v1.0.md`、`docs/索引.md`、
`research/**`、`CLAUDE.md`、`AGENTS.md`、`.gitignore`、`drivers/bins/vnet/**`、
`user/berry-gfx/**`、`user/berry-fatfs/**`、`user/bins/fsmgr/**`。

只允许的落点：

| 落点 | 形态 |
|------|------|
| `user/berry-guiproto/**` | 【新建】协议纯函数 crate（host 测试） |
| `drivers/bins/gui-inputd/**` | 【新建】输入服务 bin（lib host 测试 + main guest 面） |
| `drivers/berry-virtio/src/**` | 纯增量（新文件 `src/input.rs` + `lib.rs` 加一行 `pub mod input;`） |
| `user/Cargo.toml` | 纯增量（members 追加 `berry-guiproto` + 一行注释） |
| `drivers/Cargo.toml` | 纯增量（members 追加 `bins/gui-inputd` + 一行注释） |

### 0.2 段①（本刀做）与段②（挂账，见 §9）

**段① = 三件套协议定形 + 纯函数实现 + host 测试 + guest 入口可编译**：

- A) `berry-guiproto` v0 全量编解码（present / input / hdi 三域 op 表）+ host 测试；
- B) `berry-virtio` virtio-input 设备类（event 解码 + config 区解析纯函数面）+ host 测试；
- C) `gui-inputd` lib（pump / 归一化 / 重采样纯逻辑，host 测试）+ main（guest 入口，
  MMIO 探测模板 + 服务循环骨架，**musl release 构建通过即验收**——真机接线在段②）。

**段②挂账（本刀明确不做）**：justfile / smoke.sh / test-host 显式清单注册、rootfs 产链
（mkcpio）、run 注入事件往返标记、svcmgr 拉起段、intent 内核面。逐条见 §9。

### 0.3 风格铁律（repo 既有约定，impl 必须遵守）

1. **协议 crate 跟 berry-fsproto / berry-psproto / berry-netdproto 先例**：op 表驱动
   编解码、严格串行 RPC（一请求一应答、队列深 1）、畸形输入一律 `None` 拒绝**绝不 panic**、
   host 测试含 wire 黄金字节钉板。
2. 跨边界数据**显式小端字节串编码**（协议 crate 先例——非 `#[repr(C)]` 结构直拷）；
   布局用偏移常量 + 编解码函数双向钉死（`offset_of!` 钉板风格在结构体面由编解码对偶承担）。
3. 服务调度铁律（gui-inputd main）：**wait_many 阻塞形态**（勿自旋/勿 wait_one 单睡）、
   **恒满足项条件化入列**（P5 A-2——eventq 非空才入列设备项）、**wait 超时是心跳不是死线**
   （P6 A-3 echo 案——到点重臂续等，只有非 -EAGAIN 错码才 loud）、**recv 收口容量给富余**
   （P6 A-4——容量 > 协议定值，`nh != 协议值 → ST_BAD` 分流勿 trap）。
4. **标记不得建立在竞速上**（段②写 smoke 时适用：断言 = 状态谓词 + 字面常量）。
5. 中文注释、英文 snake_case 测试名、crate 头注写拓扑（双消费方是谁）。

---

## §1 立项依据（规划裁决映射）

| 来源 | 裁决 | 本刀落点 |
|------|------|----------|
| ADR #50（B 形态协议） | B 三件事进 wire | §3.2 SUBMIT/FRAME_CALLBACK/RELEASE |
| ADR #50 增件（GUI-10） | latch/squash、无 damage 不提交 + 零唤醒、per-interface version | §3.2/§3.5 |
| ADR #51（pass 形态 API） | berry-gfx 消费的呈现协议归 proto 层 | berry-guiproto present 域 |
| ADR #53（低功耗缝六件）+ GUI-10 C1 | vote {min/max/preferred,scene,sticky} + power_mode op day-one 进 wire | §3.2 OP_VOTE/OP_POWER_MODE |
| ADR #54（SDK 序） | 协议先定形、无消费者不接线 | gui-inputd 是段①唯一接线面；present/hdi 域无消费者（§9-⑧⑨） |
| 规划裁决 A | virtio-input 默认输入通道 | §4/§5 |
| 规划裁决 D | RGB32 800x600 起步，格式运行时协商进版本域 | §3.2 GETINFO format_bits |
| 规划裁决 G + GUI-10 C1 | per-surface 帧率意图位（route A：day-one wire 位） | §3.2 SUBMIT rate_hint |
| 规划裁决 I | berry-guiproto 提前：B 三件事 + vote + mode op + HDI 三口，host 测编解码无消费者不接线 | 本文全部 |
| GUI-10 不可逆五件 | damage / 帧调度模型 / 时序反馈（presented/discarded）/ buffer 格式协商 / per-surface 帧率意图位 | §3.2/§3.5 全部在场 |
| GUI-4 规划行 | 时间戳序列 + 稳定 pointerId + 重采样常量集中 + intent INTERACTIVE+PM + 调度铁律 + 即到即派 | §3.3/§3.6/§5 |

---

## §2 总体拓扑：三通道域

```
  应用/gui-wm(GUI-5)          guifbd(GUI-2)         virtio-gpu(GUI-2 S1)
       │ present 域               │ hdi 域                │ hdi 域
       │ (submit/callback/release │ (reg_vblank/          │
       │  /vote/power_mode)       │  vblank_wait/         │
       │                          │  prepare/commit)      │
       ▼                          ▼                       ▼
  ┌─────────────────────────────────────────────────────────┐
  │                berry-guiproto v0（wire 单源）             │
  │   present 域 op 1..6 │ input 域 op 0x20.. │ hdi 域 op 0x40.. │
  └─────────────────────────────────────────────────────────┘
       ▲ input 域                        ▲
       │ INPUT_SUBSCRIBE / INPUT_POLL     │
  ┌────┴─────────┐                        │
  │  gui-inputd  │=== virtio-input(q0) ===│  QEMU virtio-tablet-pci
  │ (输入域服务端) │    DMA VMO + always-posted
  └──────────────┘
```

- **三域 = 三对独立 channel**，不是一个 op 空间。理由：**能力即权限**——客户端拿不到它域
  channel 端点就结构性发不了它域 op（微内核纪律，替代「op 层鉴权」的第二套机制）。
- 每域独立**严格串行 RPC**（一请求一应答、队列深 1；netdproto 同款会话纪律）。
- 服务端→客户端的「推送」一律建模为**阻塞应答**：frame_callback / release /
  vblank_wait 阻塞到终态才应答——严格串行下**零服务端主动 send**（规划「零唤醒」增件的
  结构性保证：无未决请求时服务端不发任何消息）。
- 段①唯一接线的域 = **input 域**（gui-inputd 服务端）。present / hdi 域只有编解码 + 测试，
  无消费者（GUI-2 / GUI-5 / virtio-gpu S1 接线挂账 §9-⑧⑨）。
- 所有帧 ≤ `CHAN_MSG_MAX = 256`（kernel/src/channel.rs 单一事实源）——全部上限 const assert
  钉死（§3.1）。

---

## §3 berry-guiproto v0 wire 规范（逐字节）

### 3.0 通用约定

- 全字段**小端**；`u16/u32/u64` 逐字节显式拼装（协议 crate 先例，勿 `#[repr(C)]` 直拷）。
- 帧首 4B 对齐头：`[op u8][域内参数 a u8][域内参数 b u8][r u8]`（各 op 自定义 a/b 语义，
  不用则置 0 保留——**保留位编码侧恒写 0、解析侧恒不校验**，前向兼容锚）。
- **应答烘焙线不变量**（netdproto 两形先例）：
  - **非 OK 应答恰 1 字节 `[st]`，永不携带数据**；
  - **成功应答 per-op 定长**（定长 ≠ 依内容变长，变长面用 count 前缀收在定长头内）。
- `ST_*` 是 **u8 协议错误空间**（不是 errno——不与 libc 错误码混编）：

| 常量 | 值 | 语义 |
|------|----|------|
| `ST_OK` | 0 | 成功 |
| `ST_BAD` | 1 | 畸形请求（长度/值域/保留位非零——语义拒绝与畸形拒绝中取「请求本身不合法」） |
| `ST_EAGAIN` | 2 | 暂无资源/暂无数据（input poll 空批合法态） |
| `ST_BUSY` | 3 | 单会话独占冲突（netdproto ST_BUSY 同款） |
| `ST_NOENT` | 4 | 引用不存在（frame_token/buffer_id/slot 无效） |
| `ST_FULL` | 5 | 容量满（credits/slots/damage 超帽） |
| `ST_UNSUPPORTED` | 6 | 能力不支持（格式不在协商集等） |

### 3.1 帽常量（全部 `const _: () = assert!(… <= 256)` 钉板）

```
DAMAGE_MAX        = 8     // SUBMIT damage 矩形数上限
RELEASE_MAX       = 4     // 单次 RELEASE 归还 buffer 数上限
CREDITS_MAX       = 2     // 每 surface 未决帧信用上限（S6）
INPUT_SAMPLES_MAX = 4     // 单 PointerEvent 采样数上限（含 header 采样）
INPUT_EVENTS_MAX  = 3     // 单次 INPUT_POLL 应答事件数上限
HDI_SLOTS_MAX     = 2     // PREPARE/COMMIT slot 数上限

PRESENT_REQ_MAX = 152     // = 24 + 16×8（SUBMIT 最坏）
PRESENT_REP_MAX = 24      // GETINFO 应答
INPUT_EVT_MAX_LEN = 80    // = 32 + 16×(4-1)（PointerEvent 最坏）
INPUT_REP_MAX   = 244     // = 4 + 3×80（INPUT_POLL 应答最坏）
HDI_REQ_MAX     = 20      // PREPARE 请求
HDI_REP_MAX     = 16      // VBLANK_WAIT 应答
```

随行 handle（channel_send 附带 handle 表）容量常量：`ATTACH_SUBMIT=1`（buffer VMO）、
`ATTACH_REG_VBLANK=1`（Event）、`ATTACH_PREPARE=1`（buffer VMO）、
`REPLY_ATTACH_PREPARE=1`（fence Event）、`REPLY_ATTACH_RELEASE_MAX=4`。

### 3.2 present 域（op 1..6）

**OP_GETINFO = 1** —— 能力/版本/格式协商（GUI-10 #50 per-interface version：Wayland 先例，
三域各自独立版本域，v0 全部 = 1）。

- 请求 4B：`[op=1][r][r16]`
- 应答 24B（`ST_OK` 时）：

| off | 字段 | 类型 | 说明 |
|-----|------|------|------|
| 0 | st | u8 | ST_OK |
| 1 | version_present | u8 | present 域版本（v0=1） |
| 2 | version_input | u8 | input 域版本（v0=1） |
| 3 | version_hdi | u8 | hdi 域版本（v0=1） |
| 4 | r | u32 | 保留 0 |
| 8 | format_bits | u32 | 格式**位域**：FMT_XRGB8888=1<<0 / FMT_ARGB8888=1<<1 / FMT_RGB565=1<<2（裁决 D） |
| 12 | display_w | u32 | 逻辑显示宽（px） |
| 16 | display_h | u32 | 逻辑显示高（px） |
| 20 | max_credits | u8 | = CREDITS_MAX |
| 21 | r | u8 | 保留 0 |
| 22 | r16 | u16 | 保留 0 |

**OP_SUBMIT = 2** —— 提交一帧（B 三件事之一；不可逆五件中 damage/帧调度/格式/帧率意图四件的载体）。

- 请求：头 24B + `K×16B` 矩形（K ≤ DAMAGE_MAX），随行 **1 个 buffer VMO handle**（必须为
  dup 别名——映射中的唯一 handle 走销毁性 close 会被内核拒 -EBUSY，vblk 协议头注同款契约）：

| off | 字段 | 类型 | 说明 |
|-----|------|------|------|
| 0 | op | u8 | =2 |
| 1 | r | u8 | 保留 0 |
| 2 | r16 | u16 | 保留 0 |
| 4 | surface_id | u32 | |
| 8 | buffer_id | u32 | 客户端侧 buffer 编号（RELEASE 归还时对账） |
| 12 | flags | u8 | SUBMIT_F_DAMAGE_FULL=1<<0（整面 damage，K 必须为 0）/ SUBMIT_F_UNSQUASHABLE=1<<1 |
| 13 | rate_hint | u8 | **per-surface 帧率意图位**（C1 route A——见 §3.6 intent 注记） |
| 14 | r16 | u16 | 保留 0 |
| 16 | target_present_ns | u64 | 期望呈现时刻（0 = 尽快）；未来时刻表模型 |
| 24 | rects[K] | 16B×K | Rect{x u32,y u32,w u32,h u32} 逐个 |

- 应答 8B：`[st=0][r][r16][frame_token u32]`（frame_token 后续 FRAME_CALLBACK 对账）。

**OP_FRAME_CALLBACK = 3** —— 帧完成回执（**阻塞应答**：阻塞到该 token 终态——呈现或被弃）。

- 请求 8B：`[op=3][r][r16][frame_token u32]`
- 应答 16B（`ST_OK` 时；时序反馈 = 不可逆五件之三）：

| off | 字段 | 类型 | 说明 |
|-----|------|------|------|
| 0 | st | u8 | ST_OK |
| 1 | status | u8 | FRAME_PRESENTED=1 / FRAME_DISCARDED=2（0 非法值） |
| 2 | credits | u8 | 应答时刻剩余信用（≤ CREDITS_MAX） |
| 3 | r | u8 | 保留 0 |
| 4 | present_seq | u32 | 呈现序号（DISCARDED 时 = 被弃帧前最近呈现序） |
| 8 | presented_ns | u64 | 实际呈现时刻（DISCARDED 时 = 判弃时刻） |

**OP_RELEASE = 4** —— 归还 buffer（**阻塞应答**：阻塞到服务端确认可回收——呈现完成或判弃）。

- 请求 4B：`[op=4][r][r16]`
- 应答：4B 头 + `count×4B`，count ≤ RELEASE_MAX，**应答随行 count 个 VMO handle**（服务端
  归还的 buffer 别名；客户端 close 即回收）：
  `[st=0][count u8][r16 u16][buffer_ids: count×u32]`（buffer_id 与请求序对账）。

**OP_VOTE = 5** —— 帧率投票（低功耗缝 + C1；**幂等、无应答数据**）。

- 请求 12B：`[op=5][sticky u8][scene u8][r u8][min_hz u16][max_hz u16][preferred_hz u16][r16 u16]`
  （min_hz@4——2026-10-29 修复回写轮勘误：初稿字段清单漏计尾部 r16、min_hz 偏移误标 @5，
  与实现/黄金测试对齐后回写）
  - scene ∈ SCENE_NORMAL=0 / SCENE_SCROLL=1 / SCENE_DRAG=2 / SCENE_ANIMATED=3 / SCENE_IDLE=4；
  - sticky=1 表示该投票跨场景切换保持（直到下一次 sticky 投票覆盖）。
- 应答 1B：`[ST_OK]`。

**OP_POWER_MODE = 6** —— 显示电源模式（低功耗缝）。

- 请求 4B：`[op=6][mode u8][r16]`，mode ∈ POWER_NORMAL=0 / POWER_DIM=1 / POWER_AOD=2 / POWER_OFF=3。
- 应答 1B：`[ST_OK]`。

### 3.3 input 域（op 0x20..0x21）

**PointerEvent v1 布局**（input 域与 gui-inputd 内部共用同一定义——单源编解码）：

定长头 32B + `(n-1)×16B` 历史采样，`n = sample_count ∈ 1..=INPUT_SAMPLES_MAX`：

| off | 字段 | 类型 | 说明 |
|-----|------|------|------|
| 0 | phase | u8 | PHASE_DOWN=1 / MOVE=2 / UP=3 / CANCEL=4 / HOVER_ENTER=5 / HOVER_MOVE=6 / HOVER_EXIT=7（hover 三相 v1 留位不解码语义——pump 不产） |
| 1 | device_id | u8 | 多设备留位（v1 恒 0） |
| 2 | buttons | u8 | 位域 BUTTON_L=1<<0 / BUTTON_R=1<<1 / BUTTON_M=1<<2 |
| 3 | flags | u8 | PE_FL_RESAMPLED=1<<0（本事件含重采样点）/ PE_FL_SYNTHESIZED=1<<1（合成事件，如 CANCEL） |
| 4 | pointer_id | u16 | **稳定指针 id**（接触期间不变；规划钦定） |
| 6 | rsv | u16 | 保留 0 |
| 8 | timestamp_ns | u64 | **u64 ns 时间戳**（CNTVCT 口径；反 timeval ABI 漂移教训——u32 秒/毫秒两代 ABI 都碎过，GUI 面一天然 u64） |
| 16 | x | u32 | **整数物理像素**（GUI-0 FPU 未落地——非 Q16.16；Y 向下为正） |
| 20 | y | u32 | 同上 |
| 24 | pressure | u16 | **毫单位压力**（0..=1000）；PRESSURE_UNAVAILABLE=0x8000 哨兵（W3C 语义：设备无压力面按 0.5 折叠由**消费侧**做——wire 只传哨兵）。值域**双侧执法**（0..=1000 ∪ {0x8000}，越域 encode/parse 双拒） |
| 26 | contact_major | u16 | 接触椭圆长轴（px）——v1 留位恒 0（**双侧执法**，PREPARE flags 同款；v2 赋义随 input 版本域升级） |
| 28 | contact_minor | u16 | 同上恒 0（双侧执法） |
| 30 | sample_count | u8 | n（**header 采样 = 最新**；history 数组 oldest→newest，Android getHistorical* 同序） |
| 31 | rsv | u8 | 保留 0 |
| 32 | history | (n-1)×16B | 每项 `{timestamp_ns u64, x u32, y u32}` |

- 长度函数：`32 + (n-1)*16`；n=1 → 32B，n=4 → 80B = INPUT_EVT_MAX_LEN。

**OP_INPUT_SUBSCRIBE = 0x20** —— 订阅 + 投递模式声明。

- 请求 4B：`[op=0x20][delivery u8][r16]`，delivery ∈ DELIVERY_BATCHED=0 / DELIVERY_IMMEDIATE=1。
- 应答 1B：`[ST_OK]`。
- 语义立法（§3.6）：IMMEDIATE = 即到即派逃生门；**v0 服务端允许按 BATCHED 对待 IMMEDIATE**
  （降级合法不违协议——wire 兼容优先，段②接真消费者再裁真即派）。

**OP_INPUT_POLL = 0x21** —— 拉取事件批（可阻塞：阻塞到至少 1 个完整事件或客户端侧超时）。

- 请求 4B：`[op=0x21][max_events u8][r16]`，max_events ∈ **1..=INPUT_EVENTS_MAX**
  （0 = 野值拒——成功应答必携 count ≥ 1、空批走 1B `[ST_EAGAIN]` 形，「拉零个事件」
  无合法成功形；netdproto port 0 同款立法）。
- 应答两形：
  - 无事件：1B `[ST_EAGAIN]`；
  - 有事件：`[st=0][count u8][r16 u16][count 个 PointerEvent 依次串接]`，总长 ≤ INPUT_REP_MAX。

### 3.4 hdi 域（op 0x40..0x43）

**OP_REG_VBLANK = 0x40** —— 注册 vblank 通知源。

- 请求 4B + 随行 1 个 Event handle：`[op=0x40][enable u8][r16]`（enable=1 注册 / 0 注销）。
- 应答 1B：`[ST_OK]`。

**OP_VBLANK_WAIT = 0x41** —— 等下一次 vblank（**阻塞应答**）。

- 请求 4B：`[op=0x41][r][r16]`
- 应答 16B：`[st=0][r][r16][seq u32][ts_ns u64]`（seq = vblank 序号，ts_ns = 时刻）。

**OP_PREPARE = 0x42** —— buffer 进槽（hdi 域 buffer 管理；**HDI_SLOTS_MAX=2 槽**）。

- 请求 20B + 随行 1 个 buffer VMO handle：

| off | 字段 | 类型 |
|-----|------|------|
| 0 | op/flags/r16 | `[op=0x42][flags u8][r16]`（flags v1 恒 0） |
| 4 | width | u32 |
| 8 | height | u32 |
| 12 | stride | u32 |
| 16 | format | u32（FMT_* **值**非位域；不在协商集 → ST_UNSUPPORTED） |

- 应答 4B + 随行 1 个 fence Event handle（acquire fence **v1 恒空置位态**——留位，不阻塞语义）：
  `[st=0][slot u8][r16]`。

**OP_COMMIT = 0x43** —— 提交槽到目标时刻。

- 请求 12B：`[op=0x43][slot u8][r16][target u64]`（target = 期望呈现 ns，0 = 尽快）。
- 应答 1B：`[ST_OK]`（终态回执走 present 域 FRAME_CALLBACK——slot 提交后服务端生成
  frame_token，映射关系服务端内部账）。

### 3.5 语义立法（GUI-10 #50 增件逐条）

1. **无 damage 不提交**：SUBMIT 满足「K>0 或 SUBMIT_F_DAMAGE_FULL」二取一，两者皆无 →
   `ST_BAD` 拒。配套**零唤醒**：无未决 FRAME_CALLBACK 时服务端对提交帧的呈现路径不得向
   客户端发任何消息（回执只在客户端主动请求时作为阻塞应答交付）。
2. **latch/squash**：服务端可 latch 最新提交并 squash（丢弃）更早的未呈现提交；被 squash
   的 FRAME_CALLBACK 应答 `FRAME_DISCARDED`。`SUBMIT_F_UNSQUASHABLE` 位禁止 squash
   （只能延迟，不得判弃）。
3. **credits ≤ 2**：单 surface 「未决 FRAME_CALLBACK + 未 RELEASE buffer」计入信用，
   超出 → `ST_FULL`（背压闸——客户端必须在提交新帧前收旧回执/归还旧 buffer）。
4. **damage 合并定值**（客户端侧启发式，协议钉死阈值防矩形并集爆炸）：多个 rect 合并后
   包络面积 > `Σ原面积 × (1 + 1/4)` → 必须退化 SUBMIT_F_DAMAGE_FULL 整面提交。
5. **每域独立版本**：GETINFO 三版本域各自演进；input 域 v2 加字段只在 PointerEvent 尾部
   追加（sample_count/长度函数锚定头部，旧读端跳过尾部即可）。
6. **格式协商**：PREPARE format 不在 GETINFO format_bits 集内 → `ST_UNSUPPORTED`；
   SUBMIT 随行 VMO 的格式由客户端保证与协商一致（服务端段②消费者落地时校验）。

### 3.6 重采样与 intent 注记

- **重采样常量集中在 guiproto 单源**（消费侧 import，不长第二套）：
  `RESAMPLE_LATENCY_MS=5 / RESAMPLE_MIN_DELTA_MS=2 / RESAMPLE_MAX_DELTA_MS=20 /
  RESAMPLE_MAX_PREDICTION_MS=8`；预测量帽 = `min(Δ/2, 8ms)`（Android AOSP InputTransport
  实测值口径）。算法纯函数在 gui-inputd lib（`resample_to_vsync`，§6-I3）。
- **pump 不做重采样**：pump 只攒批；重采样在**消费侧最终一跳**（gui-wm/app 侧对
  history 采样做 vsync 对齐）——Flutter #73286 教训：中间层过早重采样会在最终一跳双重
  平滑放大延迟。`PE_FL_RESAMPLED` 位即消费侧标注回写面。
- **intent**：内核编码冻结 0x00–0x05（sched/src/intent.rs 单一事实源）；用户态自声明被封死
  （`sys_thread_create options != 0 → -EINVAL`）。**v1 可达路径 = 消息借入 + 
  CHAN_OPT_NO_INTENT opt-out**（guiproto wire 不编码 intent——它随 channel 消息头走，
  不在协议字节里）。gui-inputd 以 INTERACTIVE 档参与（借入语义：它收发的每条消息从
  对端借 intent）。段②挂账 §9-⑥。

---

## §4 berry-virtio 增量：virtio-input 设备类（I2）

> 规范 = virtio 1.1 **§5.8**（任务文本写 §5.9 是笔误——§5.9 是 Crypto）。落点：
> 新文件 `drivers/berry-virtio/src/input.rs`（纯增量）+ `lib.rs` 加 `pub mod input;`。
> crate 纪律不变：**volatile MMIO 读写不在本 crate**——纯函数只吃 `&[u8]`/写 `&mut [u8]`。

### 4.1 设备面事实

- `DEVICE_ID_INPUT = 18`（mmio 模块新增常量——现有 NETWORK=1/BLOCK=2 旁）。
- 两个 virtqueue：**eventq = q0**（设备→驱动事件流）、statusq = q1（LED/力反馈——v1
  不用，留位）。
- **event = 8B LE**：`{ev_type u16, code u16, value u32}`；value 语义上可为负
  （REL 轴）——解码为 u32，消费侧按 i32 解释。**无时间戳字段**：驱动在 dequeue 时刻打
  `ts_ns`（Linux evdev 同款——设备侧时间戳 virtio-input 规范不提供）。
- 设备 feature bits：**零**（仅 VERSION_1）。
- **config 区 136B @ MMIO 0x100**，字节对齐访问（本仓 MMIO 均字节口径先例）：

```
off 0   select  u8   // INPUT_CFG_SEL_*（写以选定查询目标）
off 1   subsel   u8   // 查询子选择（EV 类型或 PROP 序号）
off 2   size     u8   // 选定后 union 有效字节数
off 3   res[5]
off 8   union[128]    // 查询结果
```

- `INPUT_CFG_SEL_UNSET=0 / ID_NAME=1 / ID_SERIAL=2 / ID_DEVIDS=3 / PROP_BITS=0x10 /
  EV_BITS=0x11 / ABS_INFO=0x12`。
- 位图（PROP_BITS/EV_BITS）**LSB-first**（code n 的位 = `bitmap[n/8] & 1<<(n%8)`）。
- `ID_DEVIDS` → 8B：`InputDevids{bustype/vendor/product/version: u16}`（恰 8B，多拒少拒）。
- `ABS_INFO`（subsel = ABS 轴号）→ 20B：`InputAbsInfo{min/max/fuzz/flat/res: i32}`（恰 20B）。
- 流控：设备无缓冲时 MAY drop，但 **SHOULD 保持事件序列完整**（以 EV_SYN 为界）；QEMU
  实现缓冲到 SYN_REPORT 才投递——**一个完整批的 8B×N 在一次 used 环更新内可见**。

### 4.2 evdev 常量（本刀消费面全集 + 留位）

```
EV_SYN=0x00  EV_KEY=0x01  EV_REL=0x02  EV_ABS=0x03  EV_MSC=0x04
SYN_REPORT=0  SYN_MT_REPORT=2  SYN_DROPPED=3
ABS_X=0x00  ABS_Y=0x01
BTN_LEFT=0x110  BTN_RIGHT=0x111  BTN_MIDDLE=0x112  BTN_TOUCH=0x14a
ABS_MT_SLOT=0x2f  ABS_MT_POSITION_X=0x35  ABS_MT_POSITION_Y=0x36  ABS_MT_TRACKING_ID=0x39
REL_WHEEL=0x08（v1 忽略但计数——不入 PointerEvent）
```

- QEMU virtio-tablet 的 ABS 值域 **0..32767**（ABS_INFO min/max 可从 config 证实读出）；
  归一到屏幕整数像素由 gui-inputd `normalize_axis` 完成（§6-I3）——**用 ABS_INFO 读到的
  min/max 而非硬编 32767**（QEMU 事实对拍值，但协议上以设备自述为准）。

### 4.3 I2 测试（inline `#[cfg(test)] mod input_tests`，src/input.rs 内）

五件套：`put/get_input_event` roundtrip + LE 钉板（首字节 = ev_type 低字节）；恰长解析
（devids 恰 8B / absinfo 恰 20B，截断与尾随全拒）；`bitmap_test` LSB-first 定位钉板
（code 0/7/8/9/0x110 边界）；size 上限（INPUT_EVENT_SZ=8、INPUT_CFG_SZ=136 const assert）。

---

## §5 gui-inputd（I3）：lib 纯逻辑 + main guest 入口

### 5.1 lib.rs（host 测试面）

三块纯逻辑：**原始事件解码**（RawInputEvent + wire 转换）、**归一化**
（ABS 值域→屏幕 px）、**pump 状态机**（evdev 事件流 → PointerEvent 批）+ **重采样**纯函数。

pump 语义钉死（规划 GUI-4 行）：

- **EV_SYN(SYN_REPORT) 关批**：批内事件聚合为一个 PointerEvent（phase 按状态机定，坐标
  取批内最新采样，历史采样按 oldest→newest 入 history——header=newest，Android 同序）。
- **接触判据 = BTN_TOUCH 或 BTN_LEFT 任一 value=1**（QEMU tablet 发 BTN_TOUCH，鼠标发
  BTN_LEFT——两者等价对待）；状态机转移：
  - 空闲 + 接触 → `PHASE_DOWN`；接触中坐标变 → `PHASE_MOVE`；接触 + 释放 → `PHASE_UP`；
  - **同批多条相边按事件序 emit**（2026-10-29 修复回写轮钉板）：配对不变量「每条 DOWN
    终以 UP/CANCEL 收口」**在批内同样成立**——press+release 同批 = DOWN+UP 完整手势对
    （QMP 单命令 down+up 的标准 tap 注入形态：QEMU 缓冲到 SYN_REPORT 一次投递，§4.1）；
    接触中 release→re-press 同批 = UP 收旧手势 + 新 pointer_id 的 DOWN。批内采样挂首条
    相边事件、后续相边零采样合成（位置 = last known）；连续同向按压/释放并边（接触态
    按 live buttons 位推进，转移才记边）。SYN_DROPPED 作废窗口同时清接触位（窗口内
    被void 的按压/释放不落地 buttons——否则 CANCEL 后残留 BUTTON_L、重按压不记边）。
  - **SYN_DROPPED → 下一批合成 `PHASE_CANCEL`**（`PE_FL_SYNTHESIZED` 置位——序列完整性
    丢失时通知消费侧弃手势状态，Linux EVIOCG* SYN_DROPPED 处置同型）。
- 未知 ev_type/code：**忽略但计数**（`ignored` 计数器——诊断面，不响不炸）。
- REL 轴（REL_WHEEL 等）v1 忽略计数；ABS_MT_* v1 忽略计数（多点留位）。
- **history 溢出分批**：单逻辑接触跨 SYN 批采样数 > INPUT_SAMPLES_MAX → 拆多个
  PHASE_MOVE 事件（不丢采样——采样序列是规划钦定的不可逆件）。
- pump **不做重采样**（§3.6）；`delivery` 模式仅存 `PumpConfig` 透传，v0 不改变批语义。

### 5.2 main.rs（guest 面骨架——段①验收 = musl release 构建通过）

形态 = drivers/bins/vnet 同款独立 bin（编 musl target；Cargo.toml/build.rs 逐行对抄 vnet，
不自创链接编排）。生命周期：

1. **MMIO 探测**：扫窗 `[0x0A00_0000, +0x4000)`、步长 0x200（vprobe 同款；常量以
   `drivers/berry-virtio/src/mmio.rs` 现有约定为单一事实源），magic + VERSION_MODERN +
   **DEVICE_ID_INPUT=18** 命中即认。QEMU 槽位倒序注记（-device 声明序与槽号反序——
   vnet/vblk 实测先例）：扫全窗按设备类匹配，勿假设槽号。
2. **协商**：STATUS=0 复位（1s 回读帽）→ ACK|DRIVER → FEATURES_OK 读读回（feature 零位，
   仅 VERSION_1——若设备拒 VERSION_1 则诊断行 + 优雅退出）。
3. **DMA VMO**：vmo_create/commit/get_phys 模式（**永不映射**——vnet/vblk 同款）；eventq
   **always-posted**（预投 N 个读缓冲 + 每消费一个补投一个——vnet post_rx 先例）。
4. **事件循环**：dequeue `VirtioInputEvent` → 打 `ts_ns`（dequeue 时刻）→
   `raw_from_wire` → `pump.feed` → 客户端 INPUT_POLL 到来时 `pump.drain` 应答。
   - **wait_many 阻塞形态**：等待数组 = {服务 channel 端点 READABLE} ∪ {设备轮询项}；
     **设备项条件化入列**（P5 A-2 恒满足项纪律——仅当 used 环 idx 已前进才入列，常态
     真睡）；wait 超时 = 心跳，重臂续等（P6 A-3）。**心跳 tick = 100ms**——它同时是
     无消费窗口的设备层收割节奏：eventq 预投 16 缓冲、QEMU virtio_input_send 逐事件
     取描述符，耗尽即丢批尾，空闲期输入 > 16 事件/tick 时尾部被设备静默丢弃（press
     送达而 release 丢弃可致泵接触态滞留，至下次真实接触自愈；泵层另有 QUEUE_CAP=32
     事件丢新面——两层都只在无消费窗口内触发，段②接真消费方（按帧 POLL）后自然
     收窄）。离散转移零丢的结构性根治 = v2 IRQ 面（GIC 使能 virtio SPI + 可等待对象）。
   - **recv 收口容量给富余**（协议定值 0/1 handle + 富余 → 容量 4；`nh != 预期 → ST_BAD`
     分流，勿 trap——P6 A-4）。
   - 严格串行：**服务端永不主动 send**；INPUT_POLL 的阻塞语义 = 请求在队时服务端可睡到
     pump 有货再应答（零唤醒配套：无请求时不做任何发送）。
5. **优雅退出纪律**：设备缺席（扫窗零命中）或 gift 缺席（`gift2_h() == 0`）→ 打一行诊断
   + park（**不 trap**——段①无 svcmgr 拉起面，park 形态让真机首跑可观测）；常驻服务
   **不设退出钩子复位**（A-7 net 复位纪律适用于瞬态驱动；gui-inputd 常驻，退出路径仅
   park 不 reset——park 进程驻留不退出、DMA VMO 不回收，设备写落在自有帧内无害）。
   **trap 路径例外**（2026-10-29 修复回写轮钉板）：自陷前先 STATUS=0 复位撤 avail 队列
   ——DRIVER_OK 下设备仍会向 DMA VMO 帧写后续宿主输入事件，进程死后帧归还 buddy =
   设备写已回收帧、静默腐化无关进程（A-7 host-writer 同类残面；复位失败不拦死路）。
   **外部 process_terminate 不经驱动代码 = 结构性缺口**（v2 内核侧挂账：设备 kill 通知 /
   内核侧 DMA pin——vnet 常驻态同暴露）。
6. **私有 handle 表注记**：内核语境「常驻服务必须挂私有 handle 表」（console 类）针对
   **内核任务**共享 BOOTSTRAP_HT；gui-inputd 是**用户态进程**（自有进程表天然私有），
   该铁律天然满足——wait_many 纪律在多源等待引入时生效（§5.2-4 已落）。

### 5.3 服务拉起形态（段②接线，此处只定契约）

svcmgr spawn + gift 第 8 插（`0x0001_0008`——现有 7 插之后顺延，boot 侧 `assert_eq!`
锚表序先例）；input 域 channel 对由 svcmgr 创建、客户端端点经 gift 交付消费者（gui-wm，
GUI-5）。本刀 main 只消费 gift 端点形态（gift2_h()），**不依赖拉起序**。

---

## §6 钉板接口（I1/I2/I3 逐字冻结）

> impl 不得改签名（参数名/类型/返回类型/泛型生命周期一字不动），只能补实现体与测试。
> 模块内可加**私有**辅助项；新增 pub 项需回报设计员裁决（防接口面漂移）。
>
> **修订注（2026-10-29 修复回写轮）**：I1 初稿的借用切片形态（`PointerEvent<'a>` /
> `PresentReq::Submit { rects: &'a [Rect] }` / `ReleaseRep<'a>`）是实现期笔误——
> §0.3-2「显式小端字节串编码」+ 零 unsafe 纪律下，解析侧不可能从 `&[u8]` 安全产出
> 借用切片。裁决：**变长数据一律 owned 定容数组 + len**（I3 `PumpEvent` 同款先例），
> 全部 pub 数据结构补 `Debug/Clone/Copy/PartialEq/Eq` derive（roundtrip 对拍面）。
> 下方代码块已回写为交付 crate 的实际签名（`user/berry-guiproto/src/lib.rs` 模块头注
> 同源互指）——冻结语义以本节当前文本为准，变更集内不再存第二套规范形态。

### I1 `user/berry-guiproto/src/lib.rs`

```rust
#![cfg_attr(not(test), no_std)]

// ---- op 表 ----
pub const OP_GETINFO: u8 = 1;
pub const OP_SUBMIT: u8 = 2;
pub const OP_FRAME_CALLBACK: u8 = 3;
pub const OP_RELEASE: u8 = 4;
pub const OP_VOTE: u8 = 5;
pub const OP_POWER_MODE: u8 = 6;
pub const OP_INPUT_SUBSCRIBE: u8 = 0x20;
pub const OP_INPUT_POLL: u8 = 0x21;
pub const OP_REG_VBLANK: u8 = 0x40;
pub const OP_VBLANK_WAIT: u8 = 0x41;
pub const OP_PREPARE: u8 = 0x42;
pub const OP_COMMIT: u8 = 0x43;

// ---- 状态码 / 值域 ----
pub const ST_OK: u8 = 0;
pub const ST_BAD: u8 = 1;
pub const ST_EAGAIN: u8 = 2;
pub const ST_BUSY: u8 = 3;
pub const ST_NOENT: u8 = 4;
pub const ST_FULL: u8 = 5;
pub const ST_UNSUPPORTED: u8 = 6;

pub const SUBMIT_F_DAMAGE_FULL: u8 = 1 << 0;
pub const SUBMIT_F_UNSQUASHABLE: u8 = 1 << 1;

pub const PHASE_DOWN: u8 = 1;
pub const PHASE_MOVE: u8 = 2;
pub const PHASE_UP: u8 = 3;
pub const PHASE_CANCEL: u8 = 4;
pub const PHASE_HOVER_ENTER: u8 = 5;
pub const PHASE_HOVER_MOVE: u8 = 6;
pub const PHASE_HOVER_EXIT: u8 = 7;

pub const PE_FL_RESAMPLED: u8 = 1 << 0;
pub const PE_FL_SYNTHESIZED: u8 = 1 << 1;

pub const BUTTON_L: u8 = 1 << 0;
pub const BUTTON_R: u8 = 1 << 1;
pub const BUTTON_M: u8 = 1 << 2;

pub const SCENE_NORMAL: u8 = 0;
pub const SCENE_SCROLL: u8 = 1;
pub const SCENE_DRAG: u8 = 2;
pub const SCENE_ANIMATED: u8 = 3;
pub const SCENE_IDLE: u8 = 4;

pub const POWER_NORMAL: u8 = 0;
pub const POWER_DIM: u8 = 1;
pub const POWER_AOD: u8 = 2;
pub const POWER_OFF: u8 = 3;

pub const FMT_XRGB8888: u32 = 1 << 0;
pub const FMT_ARGB8888: u32 = 1 << 1;
pub const FMT_RGB565: u32 = 1 << 2;

pub const DELIVERY_BATCHED: u8 = 0;
pub const DELIVERY_IMMEDIATE: u8 = 1;

pub const FRAME_PRESENTED: u8 = 1;
pub const FRAME_DISCARDED: u8 = 2;

pub const PRESSURE_UNAVAILABLE: u16 = 0x8000;

// ---- 帽（全部带 const assert ≤ 256 / ≤ 内部一致性） ----
pub const DAMAGE_MAX: usize = 8;
pub const RELEASE_MAX: usize = 4;
pub const CREDITS_MAX: u8 = 2;
pub const INPUT_SAMPLES_MAX: usize = 4;
pub const INPUT_EVENTS_MAX: usize = 3;
pub const HDI_SLOTS_MAX: u8 = 2;

pub const PRESENT_REQ_MAX: usize = 152;
pub const PRESENT_REP_MAX: usize = 24;
pub const INPUT_EVT_MAX_LEN: usize = 80;
pub const INPUT_REP_MAX: usize = 244;
pub const HDI_REQ_MAX: usize = 20;
pub const HDI_REP_MAX: usize = 16;

pub const ATTACH_SUBMIT: usize = 1;
pub const ATTACH_REG_VBLANK: usize = 1;
pub const ATTACH_PREPARE: usize = 1;
pub const REPLY_ATTACH_PREPARE: usize = 1;
pub const REPLY_ATTACH_RELEASE_MAX: usize = 4;

// ---- 重采样常量单源（消费侧 import） ----
pub const RESAMPLE_LATENCY_MS: u64 = 5;
pub const RESAMPLE_MIN_DELTA_MS: u64 = 2;
pub const RESAMPLE_MAX_DELTA_MS: u64 = 20;
pub const RESAMPLE_MAX_PREDICTION_MS: u64 = 8;

// ---- 域版本（per-interface version） ----
pub const VER_PRESENT: u8 = 1;
pub const VER_INPUT: u8 = 1;
pub const VER_HDI: u8 = 1;

// ---- 共享结构 ----
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub struct Sample {
    pub timestamp_ns: u64,
    pub x: u32,
    pub y: u32,
}

pub struct PointerEvent {
    pub phase: u8,
    pub device_id: u8,
    pub buttons: u8,
    pub flags: u8,
    pub pointer_id: u16,
    pub timestamp_ns: u64,
    pub x: u32,
    pub y: u32,
    pub pressure: u16,
    pub contact_major: u16,
    pub contact_minor: u16,
    pub history: [Sample; INPUT_SAMPLES_MAX - 1],
    pub history_len: usize,
}

pub fn pointer_event_len(history_len: usize) -> Option<usize>;
pub fn encode_pointer_event(out: &mut [u8], ev: &PointerEvent) -> Option<usize>;
pub fn parse_pointer_event(buf: &[u8]) -> Option<PointerEvent>;

// ---- present 域 ----
pub enum PresentReq {
    Getinfo,
    Submit {
        surface_id: u32,
        buffer_id: u32,
        flags: u8,
        rate_hint: u8,
        target_present_ns: u64,
        rects: [Rect; DAMAGE_MAX],
        rect_count: usize,
    },
    FrameCallback { frame_token: u32 },
    ReleaseWait,
    Vote {
        sticky: u8,
        scene: u8,
        min_hz: u16,
        max_hz: u16,
        preferred_hz: u16,
    },
    PowerMode { mode: u8 },
}

pub fn encode_present_req(out: &mut [u8], req: &PresentReq) -> Option<usize>;
pub fn parse_present_req(buf: &[u8]) -> Option<PresentReq>;

pub struct GetinfoRep {
    pub version_present: u8,
    pub version_input: u8,
    pub version_hdi: u8,
    pub format_bits: u32,
    pub display_w: u32,
    pub display_h: u32,
    pub max_credits: u8,
}

pub fn encode_getinfo_rep(out: &mut [u8], rep: &GetinfoRep) -> usize;
pub fn parse_getinfo_rep(buf: &[u8]) -> Option<GetinfoRep>;

pub fn encode_submit_ack(out: &mut [u8], frame_token: u32) -> usize;
pub fn parse_submit_ack(buf: &[u8]) -> Option<u32>;

pub struct FrameDoneRep {
    pub status: u8,
    pub credits: u8,
    pub present_seq: u32,
    pub presented_ns: u64,
}

pub fn encode_frame_done_rep(out: &mut [u8], rep: &FrameDoneRep) -> usize;
pub fn parse_frame_done_rep(buf: &[u8]) -> Option<FrameDoneRep>;

pub fn encode_release_rep(out: &mut [u8], buffer_ids: &[u32]) -> Option<usize>;

pub struct ReleaseRep {
    pub buffer_ids: [u32; RELEASE_MAX],
    pub count: usize,
}

pub fn parse_release_rep(buf: &[u8]) -> Option<ReleaseRep>;

pub fn encode_status_rep(out: &mut [u8], st: u8) -> usize;
pub fn parse_status_rep(buf: &[u8]) -> Option<u8>;

// ---- input 域 ----
pub fn encode_input_sub_req(out: &mut [u8], delivery: u8) -> usize;
pub fn parse_input_sub_req(buf: &[u8]) -> Option<u8>;
pub fn encode_input_poll_req(out: &mut [u8], max_events: u8) -> usize;
pub fn parse_input_poll_req(buf: &[u8]) -> Option<u8>;
pub fn encode_input_events_rep(out: &mut [u8], events: &[PointerEvent]) -> Option<usize>;

pub struct InputEventsRep<'a> {
    /* 私有字段由 impl 定；对外只暴露下面两个方法 */
    pub(crate) buf: &'a [u8],
    pub(crate) count: u8,
}

impl<'a> InputEventsRep<'a> {
    pub fn count(&self) -> usize;
    pub fn event(&self, i: usize) -> Option<PointerEvent>;
}

pub fn parse_input_events_rep(buf: &[u8]) -> Option<Result<InputEventsRep<'_>, u8>>;

// ---- hdi 域 ----
pub fn encode_reg_vblank_req(out: &mut [u8], enable: u8) -> usize;
pub fn parse_reg_vblank_req(buf: &[u8]) -> Option<u8>;
pub fn encode_vblank_wait_req(out: &mut [u8]) -> usize;

pub struct VblankRep {
    pub seq: u32,
    pub ts_ns: u64,
}

pub fn encode_vblank_rep(out: &mut [u8], rep: &VblankRep) -> usize;
pub fn parse_vblank_rep(buf: &[u8]) -> Option<VblankRep>;

pub struct PrepareReq {
    pub flags: u8,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: u32,
}

pub fn encode_prepare_req(out: &mut [u8], req: &PrepareReq) -> usize;
pub fn parse_prepare_req(buf: &[u8]) -> Option<PrepareReq>;
pub fn encode_prepare_rep(out: &mut [u8], slot: u8) -> usize;
pub fn parse_prepare_rep(buf: &[u8]) -> Option<u8>;
pub fn encode_commit_req(out: &mut [u8], slot: u8, target_ns: u64) -> usize;
pub fn parse_commit_req(buf: &[u8]) -> Option<(u8, u64)>;
```

`InputEventsRep` 的 `pub(crate)` 字段是钉板的一部分（借用视图 + count 的最小实现形态）；
`parse_input_events_rep` 返回 `Option<Result<InputEventsRep, u8>>`：`None` = 畸形（长度
撒谎/越界），`Some(Err(st))` = **合法的 1B 错误应答**（st 非 OK），`Some(Ok(rep))` = 事件批
——三态分离是 fsproto/netdproto「撒谎长度拒」+「1B 错误应答合法态」的合成先例。

### I2 `drivers/berry-virtio/src/input.rs`（`lib.rs` 加 `pub mod input;`）

```rust
// lib.rs 仅增一行（mmio 模块内增常量）：
pub mod mmio {
    pub const DEVICE_ID_INPUT: u32 = 18;
}

// src/input.rs：
pub const INPUT_EVENT_SZ: usize = 8;

pub struct VirtioInputEvent {
    pub ev_type: u16,
    pub code: u16,
    pub value: u32,
}

pub fn put_input_event(buf: &mut [u8], off: usize, ev: &VirtioInputEvent) -> Option<()>;
pub fn get_input_event(buf: &[u8], off: usize) -> Option<VirtioInputEvent>;

// evdev 常量（§4.2 全集）
pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const EV_MSC: u16 = 0x04;
pub const SYN_REPORT: u16 = 0;
pub const SYN_MT_REPORT: u16 = 2;
pub const SYN_DROPPED: u16 = 3;
pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;
pub const BTN_TOUCH: u16 = 0x14a;
pub const ABS_MT_SLOT: u16 = 0x2f;
pub const ABS_MT_POSITION_X: u16 = 0x35;
pub const ABS_MT_POSITION_Y: u16 = 0x36;
pub const ABS_MT_TRACKING_ID: u16 = 0x39;
pub const REL_WHEEL: u16 = 0x08;

pub const INPUT_CFG_SZ: usize = 136;
pub const INPUT_CFG_SEL_UNSET: u8 = 0;
pub const INPUT_CFG_SEL_ID_NAME: u8 = 1;
pub const INPUT_CFG_SEL_ID_SERIAL: u8 = 2;
pub const INPUT_CFG_SEL_ID_DEVIDS: u8 = 3;
pub const INPUT_CFG_SEL_PROP_BITS: u8 = 0x10;
pub const INPUT_CFG_SEL_EV_BITS: u8 = 0x11;
pub const INPUT_CFG_SEL_ABS_INFO: u8 = 0x12;

pub struct InputDevids {
    pub bustype: u16,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
}

pub fn parse_input_devids(data: &[u8]) -> Option<InputDevids>;

pub struct InputAbsInfo {
    pub min: i32,
    pub max: i32,
    pub fuzz: i32,
    pub flat: i32,
    pub res: i32,
}

pub fn parse_input_absinfo(data: &[u8]) -> Option<InputAbsInfo>;

pub fn bitmap_test(bitmap: &[u8], code: u16) -> bool;
```

### I3 `drivers/bins/gui-inputd/src/lib.rs`

```rust
#![cfg_attr(not(test), no_std)]

use berry_guiproto::Sample;
use berry_virtio::input::VirtioInputEvent;

pub struct RawInputEvent {
    pub ts_ns: u64,
    pub ev_type: u16,
    pub code: u16,
    pub value: i32,
}

pub fn raw_from_wire(ts_ns: u64, ev: VirtioInputEvent) -> RawInputEvent;

pub trait RawSource {
    fn pop(&mut self) -> Option<RawInputEvent>;
}

pub struct FakeSource {
    /* 私有；host 测试喂键源 */
}

impl FakeSource {
    pub fn new() -> Self;
    pub fn push(&mut self, ev: RawInputEvent);
    pub fn len(&self) -> usize;
}

impl Default for FakeSource {}
impl RawSource for FakeSource {}

pub struct ScreenGeom {
    pub w: u32,
    pub h: u32,
}

pub struct AbsCal {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
}

pub fn normalize_axis(value: i32, cal_min: i32, cal_max: i32, screen_len: u32) -> Option<u32>;

pub fn resample_to_vsync(samples: &[Sample], vsync_ns: u64) -> Option<Sample>;

pub struct PumpConfig {
    pub geom: ScreenGeom,
    pub cal: AbsCal,
    pub delivery: u8,
}

pub struct PumpEvent {
    pub phase: u8,
    pub device_id: u8,
    pub buttons: u8,
    pub flags: u8,
    pub pointer_id: u16,
    pub timestamp_ns: u64,
    pub x: u32,
    pub y: u32,
    pub pressure: u16,
    pub history_len: usize,
    pub history: [Sample; berry_guiproto::INPUT_SAMPLES_MAX - 1],
}

pub struct PointerPump {
    /* 私有状态机 */
}

impl PointerPump {
    pub fn new(cfg: PumpConfig) -> Self;
    pub fn feed(&mut self, ev: RawInputEvent);
    pub fn pending(&self) -> usize;
    pub fn drain(&mut self, out: &mut [PumpEvent]) -> usize;
}

pub fn encode_pump_event(out: &mut [u8], ev: &PumpEvent) -> Option<usize>;
```

`PointerPump::drain` 语义：一次 drain 取走全部完整批（EV_SYN 已关批的），至多填满
`out`；返回写入数。`FakeSource` 的私有字段与 `RawSource for FakeSource` 方法体由 impl
定（钉板只锁签名——`impl Default for FakeSource {}` 空实现块即 derive 等价形态）。

---

## §7 落点与注册 + 验证命令

### 7.1 文件清单

| 文件 | 动作 |
|------|------|
| `user/berry-guiproto/Cargo.toml` | 新建（`[package] name="berry-guiproto"`；edition 对齐 berry-psproto；无依赖——纯 core） |
| `user/berry-guiproto/src/lib.rs` | 新建（I1 全量 + inline `#[cfg(test)] mod tests`） |
| `drivers/berry-virtio/src/input.rs` | 新建（I2 全量 + inline `#[cfg(test)] mod input_tests`） |
| `drivers/berry-virtio/src/lib.rs` | 增量：`pub mod input;` + mmio 模块 `DEVICE_ID_INPUT` 常量 |
| `drivers/bins/gui-inputd/Cargo.toml` | 新建（对抄 vnet：musl bin 形态 + path 依赖 berry-virtio / berry-guiproto / berry-sys） |
| `drivers/bins/gui-inputd/src/lib.rs` | 新建（I3 全量 + inline tests） |
| `drivers/bins/gui-inputd/src/main.rs` | 新建（§5.2 骨架） |
| `user/Cargo.toml` | members 追加 `"berry-guiproto"` + 注释行（GUI-4 段①：… 双消费 = gui-inputd 服务端 + 段② gui-wm 客户端） |
| `drivers/Cargo.toml` | members 追加 `"bins/gui-inputd"` + 注释行 |

### 7.2 验证命令（repo 根；段①验收线）

```bash
# host 测试三连（--target host 防根 config 锁 aarch64）
cargo test --manifest-path user/Cargo.toml -p berry-guiproto \
    --target "$(rustc -vV | sed -n 's/^host: //p')"
cargo test --manifest-path drivers/Cargo.toml -p berry-virtio \
    --target "$(rustc -vV | sed -n 's/^host: //p')"
cargo test --manifest-path drivers/Cargo.toml -p gui-inputd \
    --target "$(rustc -vV | sed -n 's/^host: //p')"

# guest 面 musl release 构建（vnet 同款 RUSTFLAGS——justfile 段②注册后收敛为 just 行）
CARGO_TARGET_DIR=drivers/cargo-target \
RUSTFLAGS="-C linker=$PWD/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
cargo build --release --manifest-path drivers/bins/gui-inputd/Cargo.toml \
    --target aarch64-unknown-linux-musl
```

justfile / smoke.sh / mkcpio 注册 = 段②挂账（§9-①②③④），本刀不触碰。

---

## §8 测试设计与验收线

### 8.1 berry-guiproto（I1，预计 ~35 测）

五件套（psproto/netdproto 先例）：

1. **roundtrip**：每 op encode→parse 恒等（含 PointerEvent n=1..4、SUBMIT K=0(FULL)/K=8、
   RELEASE count=0..4、INPUT_POLL count=1..3——空批不走成功应答形（1B EAGAIN 状态帧
   承接），`max_events` 请求域 1..=INPUT_EVENTS_MAX（0 = 野值拒））；
2. **拒绝矩阵**：每帧**每个切割点**截断全拒；尾随字节全拒；帽溢出（K=9/count=5/n=5/
   max_events=4）→ encode None / parse None；值域野值（phase=0/8、st=7、status=0/3、
   delivery=2、mode=4、scene=5、pressure=1001/0x8001（越 0..=1000 ∪ {0x8000} 域）、
   contact_major/minor≠0（v1 留位恒 0 双侧执法）、max_events=0）拒；
3. **wire 黄金字节**：GETINFO 应答 / SUBMIT K=1 / FRAME_CALLBACK 应答 / PointerEvent n=2
   各钉一条逐字节数组（首字节 LSB 的 LE 钉板内嵌——`assert_eq!(buf[8], ts & 0xff)` 式）；
4. **应答烘焙线**：非 OK 恒 1B（encode_status_rep 长度恒 1；parse 对 1B [st≠0] 恒成功）；
5. **恰满边界**：PRESENT_REQ_MAX=152 / INPUT_REP_MAX=244 恰满编解码过、+1B 拒；
   帽常量 const assert（≤256、24 = 4+16、80 = 32+48、152 = 24+16×8、244 = 4+3×80）。

### 8.2 berry-virtio input（I2，预计 ~12 测）

roundtrip + LE 钉板（ev_type 首字节）；恰长（devids 8B/absinfo 20B，7B/9B/19B/21B 全拒）；
bitmap LSB-first（code 0/7/8/9/0x110）；size const assert；put 越界 None。

### 8.3 gui-inputd lib（I3，预计 ~20 测）

- `normalize_axis`：端点（min→0 / max→screen_len-1）+ 单调性 + cal_max≤cal_min → None +
  QEMU tablet 校准 {0,32767}→{800,600} 抽样钉板；
- `resample_to_vsync`：空/单点 → None；vsync 落两采样间线性插值恒等钉板；外推超
  `min(Δ/2, 8ms)` 帽截断；Δ < MIN_DELTA → None（直接用最近点）；
- pump 状态机（FakeSource 喂键确定性自验——TAB selftest 同款纪律）：
  - DOWN→MOVE→UP 三相连批（BTN_TOUCH 路与 BTN_LEFT 路等价）；
  - 批内多采样 → history oldest→newest、header=newest；
  - SYN_DROPPED → CANCEL 合成（PE_FL_SYNTHESIZED）；
  - 未知 code 忽略计数；
  - 溢出分批（喂 5 采样跨两 SYN → 两 MOVE、采样零丢失）；
  - `encode_pump_event` → `parse_pointer_event` 对拍闭环。

### 8.4 验收线（段①）

**三 crate host 测试全绿 + gui-inputd musl release 构建通过**。零 QEMU 依赖、零内核改动、
零 rootfs/justfile 触碰。真机往返（run 注入标记）= 段②。

---

## §9 段②挂账清单（本刀不做，逐条）

1. **justfile test-host 注册**：berry-guiproto / gui-inputd 两行显式清单
   （`cargo test --manifest-path … --target {{host}}` 形态对齐既有行）+ berry-virtio 行
   既有（增量自动生效）；
2. **build-user 产链**：gui-inputd musl 构建行 + mkcpio `bin/gui-inputd=` 条目；
3. **QEMU 设备挂载**：run/run-rootfs 增 `-device virtio-tablet-device`（+QMP 通道注记）；
   QEMU 槽位倒序对拍（§5.2-1）；
4. **smoke.sh 注入往返标记**：QMP `input-send-event` → 状态谓词断言（非竞速——
   「标记不得建立在竞速上」执法点）；
5. **svcmgr 拉起段**：spawn + gift 第 8 插 `0x0001_0008` + boot 表序 assert；
6. **intent 面**：INTERACTIVE 自声明（内核 options 位 -EINVAL 封死——需内核改动另刀裁） +
   PM 交互脉冲（GUI-4 规划行 intent 项）；v1 可达 = 消息借入 + CHAN_OPT_NO_INTENT opt-out；
7. **多点/多设备/hover/REL_WHEEL/压力真值**：wire 全留位（§3.3），消费者落地另刀；
8. **gui-wm→应用最终一跳分发 + 重采样消费侧启用**（GUI-5）：PE_FL_RESAMPLED 回写面；
9. **HDI 域消费者接线**（GUI-2 / virtio-gpu S1）：GETINFO/PREPARE/COMMIT 服务端真值；
10. **内核新 VMO 插位**：评估结论 = 复用 H_MMIO 同窗已足（扫窗探测），无需内核改动——
    若段②实测撞窗再立另刀。

---

## §10 风险与开放问题

| # | 风险 | 处置 |
|---|------|------|
| 1 | QEMU virtio-tablet 行为假设（ABS 0..32767、SYN 完整批、BTN_TOUCH 形态）未真机对拍 | 段②首跑对拍（§9-③）；normalize_axis 用 ABS_INFO 自述而非硬编 |
| 2 | INPUT_REP_MAX=244 贴 256 上限 | v1 帽已留 12B 余量；事件变大走 input 域版本域升级（尾部追加锚，§3.5-5） |
| 3 | 阻塞应答 + 客户端崩溃 → 服务端挂死 | 严格串行下客户端不收则队列满 → 服务端 wait 到点重臂（心跳纪律）；v1 无对端关闭检测（EOF 缺口既有），段②拉起监督兜底 |
| 4 | IMMEDIATE 即到即派 v0 降级 BATCHED | 合法降级已立法（§3.3）；真即派待 gui-wm 消费者落地裁 |
| 5 | gui-inputd 与 guifbd 的域归属（input 域服务端未来是否并入 gui-wm） | wire 不变（域边界 = channel 边界）；服务端进程拓扑 GUI-5 裁 |
| 6 | resample vsync 源（GUI-1 fb 无 vsync 锁） | resample_to_vsync 收 vsync_ns 参数——源头由消费者给（hdi 域 vblank 或估算），协议不假设 |
| 7 | berry-virtio inline tests 与姊妹会话撞 `drivers/berry-virtio/src/**` | 纯增量纪律（新文件 + 一行 pub mod）；冲突时以先合者为准 rebase |

## §11 实施编排骨架（给 workflow 主会话）

- **I1（berry-guiproto）与 I2（berry-virtio input）零交集可并行**；
- **I3（gui-inputd）依赖 I1+I2**（path 引 + 类型面）——I1/I2 合并后启动；
- 验收代理在三者全绿后跑 §7.2 四条命令（独立验收 5/5 pass 先例）；
- 主会话管提交：单提交或 I1/I2/I3 三提交均可（原子性按 §0.1 落点表切）。

---

*本文由 GUI-4 段①设计员产出；impl 冲突回报渠道 = workflow 主会话。*
