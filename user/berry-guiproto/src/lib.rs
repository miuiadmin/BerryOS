//! berry-guiproto —— GUI wire 协议纯函数库（GUI-4 段①，`berry-fsproto`/
//! `berry-psproto`/`berry-netdproto` 先例：「解析器即产品」，零分配、零
//! unsafe、输入只借用；唯一依赖 = berry-manifest——GUI-8 §4 段①/ADR #58
//! 收官：VOTE scene 字节改携 berry_intent 六档值，`Intent` 判别值经其
//! 镜像 sched/src/intent.rs 单源转发，禁第三份手抄）。
//!
//! 拓扑（docs/GUI-4实施设计草案.md §2）：**三域 = 三对独立 channel**，不是
//! 一个 op 空间——能力即权限（客户端拿不到它域 channel 端点就结构性发不了
//! 它域 op，微内核纪律，替代「op 层鉴权」的第二套机制）：
//! - **present 域**（op 1..6）：应用 → gui-wmd 代理（GUI-5 换手，ADR #61：
//!   客户端改连 wm 域，present/hdi 客户端端归 wmd 中继，guifbd 零改动）↔
//!   显示服务端——GETINFO（能力/版本/格式协商）/ SUBMIT（提交帧）/
//!   FRAME_CALLBACK（帧完成回执，阻塞应答）/ RELEASE（归还 buffer，阻塞
//!   应答）/ VOTE（帧率投票）/ POWER_MODE（显示电源模式）；
//! - **input 域**（op 0x20..0x21）：gui-inputd（`drivers/bins/gui-inputd`）
//!   持服务端，消费者 INPUT_SUBSCRIBE（订阅）/ INPUT_POLL（拉事件批，可
//!   阻塞）——**段①唯一接线面**；
//! - **hdi 域**（op 0x40..0x43）：REG_VBLANK / VBLANK_WAIT / PREPARE /
//!   COMMIT——GUI-2 / virtio-gpu S1 挂账（§9-⑨），段①仅编解码 + 测试；
//! - **wm 域**（op 0x60..=0x7F 互斥带，v1 六 op 0x60..0x65，GUI-5 第四域）：
//!   gui-wmd 持权威窗口表（两层窗/Z 序/避让/parent·group/焦点）；**两对
//!   channel 共享同一 op 空间**——wm 服务对（GETINFO/CREATE/DESTROY/SET/
//!   TABLE_GET）+ wmd↔inputd 推送对（inputd 发 TABLE_WAIT，wmd park 至表
//!   版本新于 last_seen 应答全表快照 ≤[`WM_REP_MAX`]）。TABLE_WAIT 携
//!   last_seen = **库内首个带参阻塞请求**（vblank_wait 无参阻塞先例的带参
//!   扩展——唤醒判据进 wire，立法注记）。
//!
//! 每域独立**严格串行 RPC**（一请求一应答、队列深 1 用量，netdproto 同款
//! 纪律）；服务端→客户端「推送」一律建模为**阻塞应答**（frame_callback /
//! release / vblank_wait 阻塞到终态才应答）——严格串行下**零服务端主动
//! send**（「零唤醒」增件的结构性保证：无未决请求时服务端不发任何消息）。
//! v1 无请求序号 → 超帽不重发纪律写给 client（迟到应答滞留队列无害）。
//!
//! 通用 wire 约定（§3.0）：全字段**小端**、`u16/u32/u64` 逐字节显式拼装
//! （非 `#[repr(C)]` 结构直拷——跨边界纪律）；帧首 4B 对齐头
//! `[op u8][域内参数 a u8][域内参数 b u8][r u8]`，**保留位编码侧恒写 0、
//! 解析侧恒不校验**（前向兼容锚）；定义过的位域（SUBMIT flags /
//! PointerEvent flags/buttons）未知位双侧拒（netdproto poll flags 先例）。
//!
//! **应答烘焙线不变量**：非 OK 应答恒 1B `[st]` 永不携带数据（「超时了但
//! 带数据」的歧义形态线上不存在）；成功应答 per-op 定长（变长面用 count
//! 前缀收在定长头内）。`ST_*` 是 u8 协议错误空间（不是 errno）。
//!
//! 严格解析：截断/尾随字节/越帽/野值/未知 op 一律 None（「编不出就拒绝，
//! 不截断」）；编码器 None 契约同 netdproto（含空 out 切片）；定长帧编码
//! 器（`-> usize` 形）buf 不足 / 值域野值 = 调用方程序员错，assert 炸响
//! （psproto `encode_mem_reply` 先例）。
//!
//! **对设计稿 §6-I1 的两处笔误修正**（签名语义不变、借用形态修正，理由：
//! §0.3-2 钉死「显式小端字节串编码、非结构直拷」+ 零 unsafe 纪律 ⇒ 解析侧
//! 不可能从 `&[u8]` 安全产出借用结构切片 `&[Rect]`/`&[Sample]`/`&[u32]`）：
//! 1. 解析产物携带变长数组一律 **owned 定容数组 + len**（I3 `PumpEvent`
//!    同款形态）：`PointerEvent.history`/`PresentReq::Submit.rects`/
//!    `ReleaseRep.buffer_ids` 借用切片 → `[T; 帽]` + `*_len`/`*_count`，
//!    类型生命周期参数随之移除；
//! 2. 数据结构补 `Debug/Clone/Copy/PartialEq/Eq` derive（roundtrip 对拍
//!    面先例：netdproto Req/Reply 同款）。

#![cfg_attr(not(test), no_std)]

// ---------- op 表（三域）----------

/// present 域：能力/版本/格式协商（per-interface version，Wayland 先例）。
pub const OP_GETINFO: u8 = 1;
/// present 域：提交一帧（damage/帧调度/格式/帧率意图四件载体）。
pub const OP_SUBMIT: u8 = 2;
/// present 域：帧完成回执（阻塞应答——阻塞到 token 终态：呈现或被弃）。
pub const OP_FRAME_CALLBACK: u8 = 3;
/// present 域：归还 buffer（阻塞应答——阻塞到服务端确认可回收）。
pub const OP_RELEASE: u8 = 4;
/// present 域：帧率投票（幂等、无应答数据；低功耗缝 + C1）。
pub const OP_VOTE: u8 = 5;
/// present 域：显示电源模式（低功耗缝）。
pub const OP_POWER_MODE: u8 = 6;
/// input 域：订阅 + 投递模式声明。
pub const OP_INPUT_SUBSCRIBE: u8 = 0x20;
/// input 域：拉事件批（可阻塞：阻塞到至少 1 个完整事件）。
pub const OP_INPUT_POLL: u8 = 0x21;
/// hdi 域：注册 vblank 通知源（随行 1 个 Event handle）。
pub const OP_REG_VBLANK: u8 = 0x40;
/// hdi 域：等下一次 vblank（阻塞应答）。
pub const OP_VBLANK_WAIT: u8 = 0x41;
/// hdi 域：buffer 进槽（随行 1 个 buffer VMO handle）。
pub const OP_PREPARE: u8 = 0x42;
/// hdi 域：提交槽到目标时刻。
pub const OP_COMMIT: u8 = 0x43;
/// wm 域：能力/版本/窗口帽协商（GUI-5；wm 域自带版本面——塞 present
/// GetinfoRep 保留位 = 协议面变更，Q2 裁弃）。
pub const OP_WM_GETINFO: u8 = 0x60;
/// wm 域：建窗（两层窗 + parent/group 字段缝 + 避让标志；ack 携 window_id）。
pub const OP_WM_CREATE: u8 = 0x61;
/// wm 域：毁窗（应答 1B OK）。
pub const OP_WM_DESTROY: u8 = 0x62;
/// wm 域：掩码式属性设置（几何/可见/Z 同帧原子生效；应答 1B OK）。
pub const OP_WM_SET: u8 = 0x63;
/// wm 域：拉窗口表快照（非阻塞全量）。
pub const OP_WM_TABLE_GET: u8 = 0x64;
/// wm 域：等表变更快照（阻塞应答——携 last_seen 版本号，表已新立即答）。
pub const OP_WM_TABLE_WAIT: u8 = 0x65;

// ---------- wm 域值域（GUI-5；钦定位域未知位双侧拒）----------

/// 窗口层：应用层（桌面/普通窗）。
pub const WM_LAYER_APP: u8 = 0;
/// 窗口层：系统层（状态栏等——压应用层任意 Z）。
pub const WM_LAYER_SYSTEM: u8 = 1;
/// CREATE flags：申请几何避让系统层（wmd 写有效几何进表，不变量式重推导）。
pub const WM_CREATE_F_AVOID_SYS: u8 = 1 << 0;
/// SET mask 位：几何四槽（x/y/w/h）。
pub const WM_SET_F_GEOM: u8 = 1 << 0;
/// SET mask 位：可见槽（vis）。
pub const WM_SET_F_VIS: u8 = 1 << 1;
/// SET mask 位：Z 序槽（z）。
pub const WM_SET_F_Z: u8 = 1 << 2;
/// SET vis 槽值：隐藏。
pub const WM_VIS_HIDE: u8 = 0;
/// SET vis 槽值：显示。
pub const WM_VIS_SHOW: u8 = 1;
/// WmEntry flags 位：可见（wm_hit_test 唯一参选位）。
pub const WM_WIN_F_VISIBLE: u8 = 1 << 0;
/// WmEntry flags 位：持焦点（wmd 单焦点窗策略面）。
pub const WM_WIN_F_FOCUSED: u8 = 1 << 1;

// ---------- 状态码 / 值域 ----------

/// 成功。
pub const ST_OK: u8 = 0;
/// 畸形请求（长度/值域/保留位非零——「请求本身不合法」）。
pub const ST_BAD: u8 = 1;
/// 暂无资源/暂无数据（input poll 空批合法态）。
pub const ST_EAGAIN: u8 = 2;
/// 单会话独占冲突（netdproto ST_BUSY 同款）。
pub const ST_BUSY: u8 = 3;
/// 引用不存在（frame_token/buffer_id/slot 无效）。
pub const ST_NOENT: u8 = 4;
/// 容量满（credits/slots/damage 超帽）。
pub const ST_FULL: u8 = 5;
/// 能力不支持（格式不在协商集等）。
pub const ST_UNSUPPORTED: u8 = 6;

/// SUBMIT flags：整面 damage（K 必须为 0）。
pub const SUBMIT_F_DAMAGE_FULL: u8 = 1 << 0;
/// SUBMIT flags：禁止 squash（只能延迟，不得判弃——latch/squash 增件）。
pub const SUBMIT_F_UNSQUASHABLE: u8 = 1 << 1;

/// 接触按下。
pub const PHASE_DOWN: u8 = 1;
/// 接触移动。
pub const PHASE_MOVE: u8 = 2;
/// 接触抬起。
pub const PHASE_UP: u8 = 3;
/// 接触取消（SYN_DROPPED 合成等）。
pub const PHASE_CANCEL: u8 = 4;
/// hover 进入（v1 留位不解码语义——pump 不产）。
pub const PHASE_HOVER_ENTER: u8 = 5;
/// hover 移动（同上留位）。
pub const PHASE_HOVER_MOVE: u8 = 6;
/// hover 离开（同上留位）。
pub const PHASE_HOVER_EXIT: u8 = 7;

/// PointerEvent flags：本事件含重采样点（消费侧标注回写面）。
pub const PE_FL_RESAMPLED: u8 = 1 << 0;
/// PointerEvent flags：合成事件（如 CANCEL）。
pub const PE_FL_SYNTHESIZED: u8 = 1 << 1;

/// 左键（位域）。
pub const BUTTON_L: u8 = 1 << 0;
/// 右键（位域）。
pub const BUTTON_R: u8 = 1 << 1;
/// 中键（位域）。
pub const BUTTON_M: u8 = 1 << 2;

// ---------- SCENE 词表（已退役——GUI-8 §4 段① / ADR #58 收官）----------
//
// v0 帧率意图第一版五值词表；GUI-8 起 VOTE scene 字节**改携 berry_intent
// 六档值**（下方 [`Intent`] 转发，单源链 = sched/src/intent.rs →
// berry-manifest 判别值钉板）。本词表**停发非删值**：常量保留 +
// deprecated 标注（add-only 纪律——删 wire 值 = 破已出货编码器的跨版本
// 流）；旧值 0..=4 落在 intent 域 0x00..=0x05 之内，parse 面随刀收宽
// （见 `scene_known`）——老编码器不炸，语义重释。发送侧折算走
// [`scene_to_intent`] 或直接发 [`Intent`] 档。

/// 场景：常态（退役词表——停发非删值，见上方块注）。
#[deprecated(note = "SCENE 词表已退役（GUI-8/ADR #58）：VOTE scene 位改携 Intent 六档值；旧值折算走 scene_to_intent")]
pub const SCENE_NORMAL: u8 = 0;
/// 场景：滚动（退役词表——停发非删值，见上方块注）。
#[deprecated(note = "SCENE 词表已退役（GUI-8/ADR #58）：VOTE scene 位改携 Intent 六档值；旧值折算走 scene_to_intent")]
pub const SCENE_SCROLL: u8 = 1;
/// 场景：拖拽（退役词表——停发非删值，见上方块注）。
#[deprecated(note = "SCENE 词表已退役（GUI-8/ADR #58）：VOTE scene 位改携 Intent 六档值；旧值折算走 scene_to_intent")]
pub const SCENE_DRAG: u8 = 2;
/// 场景：动画（退役词表——停发非删值，见上方块注）。
#[deprecated(note = "SCENE 词表已退役（GUI-8/ADR #58）：VOTE scene 位改携 Intent 六档值；旧值折算走 scene_to_intent")]
pub const SCENE_ANIMATED: u8 = 3;
/// 场景：空闲（退役词表——停发非删值，见上方块注）。
#[deprecated(note = "SCENE 词表已退役（GUI-8/ADR #58）：VOTE scene 位改携 Intent 六档值；旧值折算走 scene_to_intent")]
pub const SCENE_IDLE: u8 = 4;

/// berry_intent 六档词表（转发 `berry_manifest::Intent`——判别值
/// 0x00..=0x05 的镜像钉板在彼处，单一事实源 = `sched/src/intent.rs`）。
/// VOTE scene 字节的值域即此域（GUI-8 §4 段①/ADR #58 收官：帧率
/// ballot = berry_intent 的图形场景特化——hz 三元组是图形面，档位字节
/// 归六档语义域；呈现侧仲裁位保留，ADR #53 per-surface 意图位不撤）。
pub use berry_manifest::Intent;

/// SCENE 旧词表 → berry_intent 六档折算（ADR #58 收官映射表）：交互
/// 谱系四值（NORMAL/SCROLL/DRAG/ANIMATED）→ [`Intent::Interactive`]、
/// IDLE（可全停）→ [`Intent::Background`]；未知值 None loud——不静默折
/// [`Intent::Unspecified`]：「未声明」是 0x00 自身的语义，误折 = 顶档
/// 通胀（`sched/src/intent.rs` 数值约定注）。
#[allow(deprecated)] // 折算的源域就是退役词表本体（停发 ≠ 禁读）
pub const fn scene_to_intent(scene: u8) -> Option<u8> {
    match scene {
        SCENE_NORMAL | SCENE_SCROLL | SCENE_DRAG | SCENE_ANIMATED => {
            Some(Intent::Interactive as u8)
        }
        SCENE_IDLE => Some(Intent::Background as u8),
        _ => None,
    }
}

/// 电源模式：正常。
pub const POWER_NORMAL: u8 = 0;
/// 电源模式：调暗。
pub const POWER_DIM: u8 = 1;
/// 电源模式：常显（AOD）。
pub const POWER_AOD: u8 = 2;
/// 电源模式：关屏。
pub const POWER_OFF: u8 = 3;

/// 像素格式位（GETINFO format_bits 位域）：XRGB8888。
pub const FMT_XRGB8888: u32 = 1 << 0;
/// 像素格式位（位域）：ARGB8888。
pub const FMT_ARGB8888: u32 = 1 << 1;
/// 像素格式位（位域）：RGB565。
pub const FMT_RGB565: u32 = 1 << 2;

/// PREPARE flags 位（hdi 域 buffer 对象能力位）：预留——可被服务端
/// （合成器）直接采样（Veda 启示八，调研 23 号；与 per-surface 帧率意图位
/// 同批的伙伴位，GUI-2 消费侧已实施）；v1 无消费者，wire 位 add-only 冻结
/// 语义=0 不可采样——置位帧 encode assert 炸响 / parse None（既有「flags
/// v1 恒 0」双侧执法即其冻结面，位认领零 wire 行为变更）；未来激活 = 编码
/// assert 与解析值域同步收宽（`scene_known` 收宽先例，add-only）。
pub const PREPARE_F_SERVER_SAMPLABLE: u8 = 1 << 0;

/// 投递模式：攒批（SYN 批聚合）。
pub const DELIVERY_BATCHED: u8 = 0;
/// 投递模式：即到即派逃生门（v0 服务端允许降级 BATCHED——合法不违协议）。
pub const DELIVERY_IMMEDIATE: u8 = 1;

/// 帧终态：已呈现。
pub const FRAME_PRESENTED: u8 = 1;
/// 帧终态：被弃（squash/判弃）。
pub const FRAME_DISCARDED: u8 = 2;

/// 压力哨兵：设备无压力面（W3C 0.5 折叠语义由消费侧做——wire 只传哨兵）。
pub const PRESSURE_UNAVAILABLE: u16 = 0x8000;

// ---------- 帽（§3.1；const assert 钉板见文件尾）----------

/// SUBMIT damage 矩形数上限。
pub const DAMAGE_MAX: usize = 8;
/// 单次 RELEASE 归还 buffer 数上限。
pub const RELEASE_MAX: usize = 4;
/// 每 surface 未决帧信用上限（S6 背压闸）。
pub const CREDITS_MAX: u8 = 2;
/// 单 PointerEvent 采样数上限（含 header 采样）。
pub const INPUT_SAMPLES_MAX: usize = 4;
/// 单次 INPUT_POLL 应答事件数上限。
pub const INPUT_EVENTS_MAX: usize = 3;
/// PREPARE/COMMIT slot 数上限。
pub const HDI_SLOTS_MAX: u8 = 2;

/// present 域请求帧帽 = 24 + 16×8（SUBMIT 最坏）。
pub const PRESENT_REQ_MAX: usize = 152;
/// present 域应答帧帽（GETINFO 应答 24B）。
pub const PRESENT_REP_MAX: usize = 24;
/// 单 PointerEvent 帧帽 = 32 + 16×(4-1)。
pub const INPUT_EVT_MAX_LEN: usize = 80;
/// INPUT_POLL 应答帧帽 = 4 + 3×80。
pub const INPUT_REP_MAX: usize = 244;
/// hdi 域请求帧帽（PREPARE 请求 20B）。
pub const HDI_REQ_MAX: usize = 20;
/// hdi 域应答帧帽（VBLANK_WAIT 应答 16B）。
pub const HDI_REP_MAX: usize = 16;
/// wm 域窗口表上限（快照 = 12B 头 + 15×16B 表项 = [`WM_REP_MAX`] 单帧
/// 顶格；帽按快照单帧 ≤ CHAN_MSG_MAX 硬约束定，GUI-8 launcher 桌面形态复核）。
pub const WM_WINDOWS_MAX: usize = 15;
/// wm 域请求帧帽（SET 请求 20B 最坏）。
pub const WM_REQ_MAX: usize = 20;
/// wm 域应答帧帽 = 快照 12B 头 + 15×16B 表项。
pub const WM_REP_MAX: usize = 252;

/// SUBMIT 请求随行 handle 数（buffer VMO——须 dup 别名，vblk 协议头注契约）。
pub const ATTACH_SUBMIT: usize = 1;
/// REG_VBLANK 请求随行 handle 数（Event）。
pub const ATTACH_REG_VBLANK: usize = 1;
/// PREPARE 请求随行 handle 数（buffer VMO）。
pub const ATTACH_PREPARE: usize = 1;
/// PREPARE 应答随行 handle 数（fence Event——v1 恒空置位态）。
pub const REPLY_ATTACH_PREPARE: usize = 1;
/// RELEASE 应答随行 handle 数上限（归还 buffer 别名）。
pub const REPLY_ATTACH_RELEASE_MAX: usize = 4;

// ---------- 重采样常量单源（§3.6；消费侧 import，不长第二套）----------

/// 重采样目标延迟（Android AOSP InputTransport 实测值口径）。
pub const RESAMPLE_LATENCY_MS: u64 = 5;
/// 两采样间最小 Δ（低于此直接用最近点——不插值）。
pub const RESAMPLE_MIN_DELTA_MS: u64 = 2;
/// 两采样间最大 Δ（超过此不外推——序列断裂面交消费侧）。
pub const RESAMPLE_MAX_DELTA_MS: u64 = 20;
/// 预测量帽（预测量 = min(Δ/2, 8ms)）。
pub const RESAMPLE_MAX_PREDICTION_MS: u64 = 8;

// ---------- 域版本（per-interface version，v0 全部 = 1）----------

/// present 域版本。
pub const VER_PRESENT: u8 = 1;
/// input 域版本。
pub const VER_INPUT: u8 = 1;
/// hdi 域版本。
pub const VER_HDI: u8 = 1;
/// wm 域版本（GUI-5 定形刀在册——ADR #50：per-interface version 随定形刀必落）。
pub const VER_WM: u8 = 1;

// ---------- 帧长常量（私有；公开帽常量见上）----------

/// SUBMIT 请求定长头。
const SUBMIT_REQ_HEAD: usize = 24;
/// SUBMIT 应答恰长：`[st][r][r16][frame_token u32]`。
const SUBMIT_ACK_LEN: usize = 8;
/// FRAME_CALLBACK 应答恰长 16B。
const FRAME_DONE_REP_LEN: usize = 16;
/// RELEASE 应答定长头。
const RELEASE_REP_HEAD: usize = 4;
/// VOTE 请求恰长 12B。
const VOTE_REQ_LEN: usize = 12;
/// VBLANK_WAIT 应答恰长 16B。
const VBLANK_REP_LEN: usize = 16;
/// PREPARE 请求恰长 20B。
const PREPARE_REQ_LEN: usize = 20;
/// COMMIT 请求恰长 12B。
const COMMIT_REQ_LEN: usize = 12;
/// PointerEvent 定长头。
const PE_HEAD: usize = 32;
/// PointerEvent 历史采样单项宽。
const PE_SAMPLE: usize = 16;
/// INPUT_POLL 应答定长头。
const INPUT_REP_HEAD: usize = 4;
/// WM_GETINFO 应答恰长 12B。
const WM_GETINFO_REP_LEN: usize = 12;
/// WM_CREATE 请求恰长 16B / ack 恰长 8B。
const WM_CREATE_REQ_LEN: usize = 16;
const WM_CREATE_ACK_LEN: usize = 8;
/// WM_DESTROY 请求恰长 8B。
const WM_DESTROY_REQ_LEN: usize = 8;
/// WM_SET 请求恰长 20B。
const WM_SET_REQ_LEN: usize = 20;
/// WM_TABLE_WAIT 请求恰长 8B。
const WM_TABLE_WAIT_REQ_LEN: usize = 8;
/// 窗口表快照定长头 / 表项单项宽。
const WM_SNAP_HEAD: usize = 12;
const WM_ENTRY_LEN: usize = 16;

// ---------- 值域谓词（const fn——位域/域值双侧对称执法）----------

const fn st_known(st: u8) -> bool {
    st <= ST_UNSUPPORTED
}

const fn phase_known(p: u8) -> bool {
    p >= PHASE_DOWN && p <= PHASE_HOVER_EXIT
}

const fn pe_flags_known(f: u8) -> bool {
    f & !(PE_FL_RESAMPLED | PE_FL_SYNTHESIZED) == 0
}

const fn buttons_known(b: u8) -> bool {
    b & !(BUTTON_L | BUTTON_R | BUTTON_M) == 0
}

/// pressure 值域（§3.3）：0..=1000 毫单位或 [`PRESSURE_UNAVAILABLE`] 哨兵
/// （双侧执法——1001..0x7FFF / 0x8001.. 越域拒）。
const fn pressure_known(p: u16) -> bool {
    p <= 1000 || p == PRESSURE_UNAVAILABLE
}

const fn submit_flags_known(f: u8) -> bool {
    f & !(SUBMIT_F_DAMAGE_FULL | SUBMIT_F_UNSQUASHABLE) == 0
}

/// VOTE scene 字节值域 = berry_intent 六档（GUI-8 §4 段① 起 scene 位携
/// [`Intent`] 判别值 0x00..=0x05；退役 SCENE 值 0..=4 为其子集——域随刀
/// 收宽属 add-only，老编码器不破）。0x06.. 保留段双侧拒（对齐
/// `sched/src/intent.rs` 编码冻结注记：0x06..0x0F 保留、内核 -EINVAL）。
const fn scene_known(s: u8) -> bool {
    s <= Intent::Maintenance as u8
}

const fn mode_known(m: u8) -> bool {
    m <= POWER_OFF
}

const fn delivery_known(d: u8) -> bool {
    d <= DELIVERY_IMMEDIATE
}

const fn frame_status_known(s: u8) -> bool {
    s == FRAME_PRESENTED || s == FRAME_DISCARDED
}

const fn wm_layer_known(l: u8) -> bool {
    l <= WM_LAYER_SYSTEM
}

const fn wm_create_flags_known(f: u8) -> bool {
    f & !WM_CREATE_F_AVOID_SYS == 0
}

/// SET mask 值域：mask=0（「空设置」非法定义在协议层——netdproto port 0 /
/// INPUT_POLL max_events 0 同款）与未知位双侧拒。
const fn wm_set_mask_known(m: u8) -> bool {
    m != 0 && m & !(WM_SET_F_GEOM | WM_SET_F_VIS | WM_SET_F_Z) == 0
}

const fn wm_vis_known(v: u8) -> bool {
    v <= WM_VIS_SHOW
}

const fn wm_win_flags_known(f: u8) -> bool {
    f & !(WM_WIN_F_VISIBLE | WM_WIN_F_FOCUSED) == 0
}

// ---------- LE 读写辅助（私有；encode 侧长度已由 get_mut 定界）----------

fn le16(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(off..off + 2)?.try_into().ok()?))
}

fn le32(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
}

fn le64(b: &[u8], off: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(off..off + 8)?.try_into().ok()?))
}

// ---------- 共享结构 ----------

/// damage 矩形（SUBMIT；wire 16B：x/y/w/h 各 u32 LE）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 历史采样（PointerEvent history 单项；wire 16B：ts u64 + x/y u32）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    pub timestamp_ns: u64,
    pub x: u32,
    pub y: u32,
}

/// 指针事件 v1（input 域与 gui-inputd 内部共用同一定义——单源编解码）。
///
/// wire 布局（§3.3）：定长头 32B + `(n-1)×16B` 历史采样，
/// `n = history_len + 1 ∈ 1..=[`INPUT_SAMPLES_MAX`]`：
///
/// ```text
/// off 0  phase u8 / 1 device_id u8 / 2 buttons u8 / 3 flags u8
/// off 4  pointer_id u16 / 6 rsv u16（0）
/// off 8  timestamp_ns u64（CNTVCT 口径）
/// off 16 x u32 / 20 y u32（整数物理像素，Y 向下为正）
/// off 24 pressure u16 / 26 contact_major u16 / 28 contact_minor u16
/// off 30 sample_count u8（header 采样 = 最新）/ 31 rsv u8（0）
/// off 32 history：(n-1)×{ts u64, x u32, y u32}（oldest→newest）
/// ```
///
/// `history` 为 owned 定容数组 + `history_len`（I3 `PumpEvent` 同款——见
/// 模块头注「笔误修正」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerEvent {
    pub phase: u8,
    pub device_id: u8,
    pub buttons: u8,
    pub flags: u8,
    /// 稳定指针 id（接触期间不变）。
    pub pointer_id: u16,
    pub timestamp_ns: u64,
    pub x: u32,
    pub y: u32,
    /// 毫单位压力（0..=1000）或 [`PRESSURE_UNAVAILABLE`] 哨兵（双侧执法）。
    pub pressure: u16,
    /// 接触椭圆长轴（px）——v1 留位恒 0（双侧执法，PREPARE flags 同款；
    /// v2 赋义随 input 域版本域升级）。
    pub contact_major: u16,
    /// 同上恒 0（双侧执法）。
    pub contact_minor: u16,
    /// 历史采样（前 `history_len` 项有效，oldest→newest）。
    pub history: [Sample; INPUT_SAMPLES_MAX - 1],
    pub history_len: usize,
}

/// PointerEvent 帧长：`history_len` ∈ 0..=INPUT_SAMPLES_MAX-1 →
/// `32 + history_len×16`；越帽 None（「编不出就拒绝」）。
pub fn pointer_event_len(history_len: usize) -> Option<usize> {
    if history_len >= INPUT_SAMPLES_MAX {
        return None;
    }
    Some(PE_HEAD + history_len * PE_SAMPLE)
}

/// 编码 PointerEvent（恰长写入；phase/flags/buttons/pressure 值域 +
/// contact_* v1 恒 0 + history 越帽 + out 不足 → None，不截断）。
/// sample_count 由 `history_len + 1` 单源推导。
pub fn encode_pointer_event(out: &mut [u8], ev: &PointerEvent) -> Option<usize> {
    let n = ev.history_len;
    if n >= INPUT_SAMPLES_MAX {
        return None;
    }
    if !phase_known(ev.phase)
        || !pe_flags_known(ev.flags)
        || !buttons_known(ev.buttons)
        || !pressure_known(ev.pressure)
        || ev.contact_major != 0
        || ev.contact_minor != 0
    {
        return None; // contact_*：v1 留位恒 0（双侧执法——PREPARE flags 同款）
    }
    let total = PE_HEAD + n * PE_SAMPLE;
    let m = out.get_mut(..total)?;
    m[0] = ev.phase;
    m[1] = ev.device_id;
    m[2] = ev.buttons;
    m[3] = ev.flags;
    m[4..6].copy_from_slice(&ev.pointer_id.to_le_bytes());
    m[6..8].copy_from_slice(&0u16.to_le_bytes()); // rsv 恒 0
    m[8..16].copy_from_slice(&ev.timestamp_ns.to_le_bytes());
    m[16..20].copy_from_slice(&ev.x.to_le_bytes());
    m[20..24].copy_from_slice(&ev.y.to_le_bytes());
    m[24..26].copy_from_slice(&ev.pressure.to_le_bytes());
    m[26..28].copy_from_slice(&ev.contact_major.to_le_bytes());
    m[28..30].copy_from_slice(&ev.contact_minor.to_le_bytes());
    m[30] = (n + 1) as u8; // sample_count：header 采样计入
    m[31] = 0;
    let mut off = PE_HEAD;
    for i in 0..n {
        let s = &ev.history[i];
        m[off..off + 8].copy_from_slice(&s.timestamp_ns.to_le_bytes());
        m[off + 8..off + 12].copy_from_slice(&s.x.to_le_bytes());
        m[off + 12..off + 16].copy_from_slice(&s.y.to_le_bytes());
        off += PE_SAMPLE;
    }
    Some(total)
}

/// 解析 PointerEvent：恰长执法（sample_count 定帧长，尾随字节 = 野值拒）；
/// sample_count 0 或越帽 / phase·flags·buttons·pressure 值域野值 /
/// contact_* 非 0 → None。保留位（off 6/31）恒不校验（前向兼容锚）。
pub fn parse_pointer_event(buf: &[u8]) -> Option<PointerEvent> {
    if buf.len() < PE_HEAD {
        return None;
    }
    let n = buf[30] as usize;
    if n == 0 || n > INPUT_SAMPLES_MAX {
        return None;
    }
    if buf.len() != PE_HEAD + (n - 1) * PE_SAMPLE {
        return None; // 恰长（尾随/截断一并拒）
    }
    let phase = buf[0];
    let buttons = buf[2];
    let flags = buf[3];
    let pressure = le16(buf, 24)?;
    let contact_major = le16(buf, 26)?;
    let contact_minor = le16(buf, 28)?;
    if !phase_known(phase)
        || !buttons_known(buttons)
        || !pe_flags_known(flags)
        || !pressure_known(pressure)
        || contact_major != 0
        || contact_minor != 0
    {
        return None; // contact_*：v1 留位恒 0（双侧执法——PREPARE flags 同款）
    }
    let history_len = n - 1;
    let mut history = [Sample { timestamp_ns: 0, x: 0, y: 0 }; INPUT_SAMPLES_MAX - 1];
    for i in 0..history_len {
        let off = PE_HEAD + i * PE_SAMPLE;
        history[i] = Sample {
            timestamp_ns: le64(buf, off)?,
            x: le32(buf, off + 8)?,
            y: le32(buf, off + 12)?,
        };
    }
    Some(PointerEvent {
        phase,
        device_id: buf[1],
        buttons,
        flags,
        pointer_id: le16(buf, 4)?,
        timestamp_ns: le64(buf, 8)?,
        x: le32(buf, 16)?,
        y: le32(buf, 20)?,
        pressure,
        contact_major,
        contact_minor,
        history,
        history_len,
    })
}

/// 事件流游标步进（私有）：读 `buf[off..]` 处 PointerEvent 的帧长
/// （header sample_count 定长）；截断/计数野值 → None。
fn event_span(buf: &[u8], off: usize) -> Option<usize> {
    let sc = *buf.get(off + 30)? as usize;
    if sc == 0 || sc > INPUT_SAMPLES_MAX {
        return None;
    }
    Some(PE_HEAD + (sc - 1) * PE_SAMPLE)
}

// ---------- present 域 ----------

/// present 域请求（解析产物；rects 为 owned 定容数组——见模块头注）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentReq {
    /// GETINFO：能力/版本/格式协商。
    Getinfo,
    /// SUBMIT：提交一帧（随行 1 个 buffer VMO handle；语义立法「K>0 或
    /// DAMAGE_FULL 二取一」是**服务端** ST_BAD 拒面——wire 层两者皆无仍
    /// 合法帧，编码/解析不拦）。
    Submit {
        surface_id: u32,
        buffer_id: u32,
        flags: u8,
        /// per-surface 帧率意图位（route A——day-one wire 位）。
        rate_hint: u8,
        /// 期望呈现时刻（0 = 尽快）。
        target_present_ns: u64,
        rects: [Rect; DAMAGE_MAX],
        rect_count: usize,
    },
    /// FRAME_CALLBACK：帧完成回执（阻塞应答）。
    FrameCallback { frame_token: u32 },
    /// RELEASE：归还 buffer（阻塞应答；请求无参，应答携归还清单）。
    ReleaseWait,
    /// VOTE：帧率投票（幂等）。
    Vote {
        /// 布尔语义（0/1）：跨场景切换保持。
        sticky: u8,
        /// 帧率意图档（GUI-8 §4 段①起携 berry_intent 六档值——[`Intent`]
        /// 判别值 0x00..=0x05；旧 SCENE 词表退役，折算见 [`scene_to_intent`]）。
        scene: u8,
        min_hz: u16,
        max_hz: u16,
        preferred_hz: u16,
    },
    /// POWER_MODE：显示电源模式。
    PowerMode { mode: u8 },
}

/// 编码 present 域请求（None 契约：rect_count 越帽 / flags·scene·mode·
/// sticky 野值 / out 不足——不截断）。
pub fn encode_present_req(out: &mut [u8], req: &PresentReq) -> Option<usize> {
    match req {
        PresentReq::Getinfo => {
            let m = out.get_mut(..4)?;
            m[0] = OP_GETINFO;
            m[1] = 0;
            m[2] = 0;
            m[3] = 0;
            Some(4)
        }
        PresentReq::Submit {
            surface_id,
            buffer_id,
            flags,
            rate_hint,
            target_present_ns,
            rects,
            rect_count,
        } => {
            if *rect_count > DAMAGE_MAX {
                return None;
            }
            if !submit_flags_known(*flags) {
                return None;
            }
            let total = SUBMIT_REQ_HEAD + rect_count * 16;
            let m = out.get_mut(..total)?;
            m[0] = OP_SUBMIT;
            m[1] = 0;
            m[2] = 0;
            m[3] = 0;
            m[4..8].copy_from_slice(&surface_id.to_le_bytes());
            m[8..12].copy_from_slice(&buffer_id.to_le_bytes());
            m[12] = *flags;
            m[13] = *rate_hint;
            m[14] = 0;
            m[15] = 0;
            m[16..24].copy_from_slice(&target_present_ns.to_le_bytes());
            let mut off = SUBMIT_REQ_HEAD;
            for i in 0..*rect_count {
                let r = &rects[i];
                m[off..off + 4].copy_from_slice(&r.x.to_le_bytes());
                m[off + 4..off + 8].copy_from_slice(&r.y.to_le_bytes());
                m[off + 8..off + 12].copy_from_slice(&r.w.to_le_bytes());
                m[off + 12..off + 16].copy_from_slice(&r.h.to_le_bytes());
                off += 16;
            }
            Some(total)
        }
        PresentReq::FrameCallback { frame_token } => {
            let m = out.get_mut(..8)?;
            m[0] = OP_FRAME_CALLBACK;
            m[1] = 0;
            m[2] = 0;
            m[3] = 0;
            m[4..8].copy_from_slice(&frame_token.to_le_bytes());
            Some(8)
        }
        PresentReq::ReleaseWait => {
            let m = out.get_mut(..4)?;
            m[0] = OP_RELEASE;
            m[1] = 0;
            m[2] = 0;
            m[3] = 0;
            Some(4)
        }
        PresentReq::Vote {
            sticky,
            scene,
            min_hz,
            max_hz,
            preferred_hz,
        } => {
            if !scene_known(*scene) || *sticky > 1 {
                return None;
            }
            let m = out.get_mut(..VOTE_REQ_LEN)?;
            m[0] = OP_VOTE;
            m[1] = *sticky;
            m[2] = *scene;
            m[3] = 0;
            m[4..6].copy_from_slice(&min_hz.to_le_bytes());
            m[6..8].copy_from_slice(&max_hz.to_le_bytes());
            m[8..10].copy_from_slice(&preferred_hz.to_le_bytes());
            m[10] = 0;
            m[11] = 0;
            Some(VOTE_REQ_LEN)
        }
        PresentReq::PowerMode { mode } => {
            if !mode_known(*mode) {
                return None;
            }
            let m = out.get_mut(..4)?;
            m[0] = OP_POWER_MODE;
            m[1] = *mode;
            m[2] = 0;
            m[3] = 0;
            Some(4)
        }
    }
}

/// 解析 present 域请求：六 op 各自恰长执法（截断/尾随 = 野值拒）；rect
/// 数由帧长推导（越帽拒）/ flags·scene·mode·sticky 值域对称拒；保留位
/// （off 1/2/3 等）恒不校验。
pub fn parse_present_req(buf: &[u8]) -> Option<PresentReq> {
    match *buf.first()? {
        OP_GETINFO => {
            if buf.len() != 4 {
                return None;
            }
            Some(PresentReq::Getinfo)
        }
        OP_SUBMIT => {
            if buf.len() < SUBMIT_REQ_HEAD {
                return None;
            }
            let rem = buf.len() - SUBMIT_REQ_HEAD;
            let k = rem / 16;
            if buf.len() != SUBMIT_REQ_HEAD + k * 16 || k > DAMAGE_MAX {
                return None; // 16B 对齐尾数 / 越帽均野值
            }
            let flags = buf[12];
            if !submit_flags_known(flags) {
                return None;
            }
            let mut rects = [Rect { x: 0, y: 0, w: 0, h: 0 }; DAMAGE_MAX];
            for i in 0..k {
                let off = SUBMIT_REQ_HEAD + i * 16;
                rects[i] = Rect {
                    x: le32(buf, off)?,
                    y: le32(buf, off + 4)?,
                    w: le32(buf, off + 8)?,
                    h: le32(buf, off + 12)?,
                };
            }
            Some(PresentReq::Submit {
                surface_id: le32(buf, 4)?,
                buffer_id: le32(buf, 8)?,
                flags,
                rate_hint: buf[13],
                target_present_ns: le64(buf, 16)?,
                rects,
                rect_count: k,
            })
        }
        OP_FRAME_CALLBACK => {
            if buf.len() != 8 {
                return None;
            }
            Some(PresentReq::FrameCallback {
                frame_token: le32(buf, 4)?,
            })
        }
        OP_RELEASE => {
            if buf.len() != 4 {
                return None;
            }
            Some(PresentReq::ReleaseWait)
        }
        OP_VOTE => {
            if buf.len() != VOTE_REQ_LEN {
                return None;
            }
            let sticky = buf[1];
            let scene = buf[2];
            if !scene_known(scene) || sticky > 1 {
                return None;
            }
            Some(PresentReq::Vote {
                sticky,
                scene,
                min_hz: le16(buf, 4)?,
                max_hz: le16(buf, 6)?,
                preferred_hz: le16(buf, 8)?,
            })
        }
        OP_POWER_MODE => {
            if buf.len() != 4 {
                return None;
            }
            let mode = buf[1];
            if !mode_known(mode) {
                return None;
            }
            Some(PresentReq::PowerMode { mode })
        }
        _ => None,
    }
}

/// GETINFO 应答（解析产物）。24B 布局见 `encode_getinfo_rep`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetinfoRep {
    pub version_present: u8,
    pub version_input: u8,
    pub version_hdi: u8,
    /// 格式**位域**（FMT_* 位或）；未知位前向兼容不拒（v2 加格式不改旧读端）。
    pub format_bits: u32,
    pub display_w: u32,
    pub display_h: u32,
    /// ≤ CREDITS_MAX（双侧执法）。
    pub max_credits: u8,
}

/// 编码 GETINFO 应答（恰 [`PRESENT_REP_MAX`] 24B；buf 不足 / max_credits
/// 越帽 = 调用方错，assert——psproto `encode_mem_reply` 先例）。
///
/// 布局：`[st=0][vp][vi][vh][r u32][format_bits u32][w u32][h u32]
/// [max_credits u8][r u8][r16 u16]`。
pub fn encode_getinfo_rep(out: &mut [u8], rep: &GetinfoRep) -> usize {
    assert!(out.len() >= PRESENT_REP_MAX, "guiproto: getinfo buf 不足");
    assert!(rep.max_credits <= CREDITS_MAX, "guiproto: max_credits 越帽");
    out[0] = ST_OK;
    out[1] = rep.version_present;
    out[2] = rep.version_input;
    out[3] = rep.version_hdi;
    out[4..8].copy_from_slice(&0u32.to_le_bytes()); // r 恒 0
    out[8..12].copy_from_slice(&rep.format_bits.to_le_bytes());
    out[12..16].copy_from_slice(&rep.display_w.to_le_bytes());
    out[16..20].copy_from_slice(&rep.display_h.to_le_bytes());
    out[20] = rep.max_credits;
    out[21] = 0;
    out[22..24].copy_from_slice(&0u16.to_le_bytes());
    PRESENT_REP_MAX
}

/// 解析 GETINFO 应答：恰 24B + st=ST_OK + max_credits ≤ [`CREDITS_MAX`]；
/// 保留区（off 4..8/21/22..24）恒不校验。
pub fn parse_getinfo_rep(buf: &[u8]) -> Option<GetinfoRep> {
    if buf.len() != PRESENT_REP_MAX || buf[0] != ST_OK {
        return None;
    }
    let max_credits = buf[20];
    if max_credits > CREDITS_MAX {
        return None;
    }
    Some(GetinfoRep {
        version_present: buf[1],
        version_input: buf[2],
        version_hdi: buf[3],
        format_bits: le32(buf, 8)?,
        display_w: le32(buf, 12)?,
        display_h: le32(buf, 16)?,
        max_credits,
    })
}

/// 编码 SUBMIT 应答（恰 8B `[st=0][r][r16][frame_token u32 LE]`；buf 不足
/// = 调用方错，assert）。
pub fn encode_submit_ack(out: &mut [u8], frame_token: u32) -> usize {
    assert!(out.len() >= SUBMIT_ACK_LEN, "guiproto: submit ack buf 不足");
    out[0] = ST_OK;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    out[4..8].copy_from_slice(&frame_token.to_le_bytes());
    SUBMIT_ACK_LEN
}

/// 解析 SUBMIT 应答：恰 8B + st=ST_OK → frame_token；错误应答是 1B 形
/// （`parse_status_rep` 面），8B 非 OK → None。
pub fn parse_submit_ack(buf: &[u8]) -> Option<u32> {
    if buf.len() != SUBMIT_ACK_LEN || buf[0] != ST_OK {
        return None;
    }
    le32(buf, 4)
}

/// FRAME_CALLBACK 应答（时序反馈 = 不可逆五件之三）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameDoneRep {
    /// FRAME_PRESENTED / FRAME_DISCARDED（0 非法值）。
    pub status: u8,
    /// 应答时刻剩余信用（≤ CREDITS_MAX）。
    pub credits: u8,
    /// 呈现序号（DISCARDED 时 = 被弃帧前最近呈现序）。
    pub present_seq: u32,
    /// 实际呈现时刻（DISCARDED 时 = 判弃时刻）。
    pub presented_ns: u64,
}

/// 编码 FRAME_CALLBACK 应答（恰 16B：`[st=0][status][credits][r]
/// [present_seq u32][presented_ns u64]`；buf 不足 / status·credits 野值
/// = 调用方错，assert）。
pub fn encode_frame_done_rep(out: &mut [u8], rep: &FrameDoneRep) -> usize {
    assert!(out.len() >= FRAME_DONE_REP_LEN, "guiproto: frame done buf 不足");
    assert!(frame_status_known(rep.status), "guiproto: 野 frame status");
    assert!(rep.credits <= CREDITS_MAX, "guiproto: credits 越帽");
    out[0] = ST_OK;
    out[1] = rep.status;
    out[2] = rep.credits;
    out[3] = 0;
    out[4..8].copy_from_slice(&rep.present_seq.to_le_bytes());
    out[8..16].copy_from_slice(&rep.presented_ns.to_le_bytes());
    FRAME_DONE_REP_LEN
}

/// 解析 FRAME_CALLBACK 应答：恰 16B + st=ST_OK + status ∈ {1,2} +
/// credits ≤ [`CREDITS_MAX`]。
pub fn parse_frame_done_rep(buf: &[u8]) -> Option<FrameDoneRep> {
    if buf.len() != FRAME_DONE_REP_LEN || buf[0] != ST_OK {
        return None;
    }
    let status = buf[1];
    let credits = buf[2];
    if !frame_status_known(status) || credits > CREDITS_MAX {
        return None;
    }
    Some(FrameDoneRep {
        status,
        credits,
        present_seq: le32(buf, 4)?,
        presented_ns: le64(buf, 8)?,
    })
}

/// RELEASE 应答（解析产物；owned 定容数组——见模块头注）。应答随行
/// `count` 个 VMO handle（服务端归还的 buffer 别名），handle 面不在帧内。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseRep {
    pub buffer_ids: [u32; RELEASE_MAX],
    pub count: usize,
}

/// 编码 RELEASE 应答（`[st=0][count u8][r16][count×u32 LE]`；count 越帽 /
/// out 不足 → None，不截断）。
pub fn encode_release_rep(out: &mut [u8], buffer_ids: &[u32]) -> Option<usize> {
    if buffer_ids.len() > RELEASE_MAX {
        return None;
    }
    let total = RELEASE_REP_HEAD + buffer_ids.len() * 4;
    let m = out.get_mut(..total)?;
    m[0] = ST_OK;
    m[1] = buffer_ids.len() as u8;
    m[2] = 0;
    m[3] = 0;
    for (i, id) in buffer_ids.iter().enumerate() {
        m[4 + i * 4..8 + i * 4].copy_from_slice(&id.to_le_bytes());
    }
    Some(total)
}

/// 解析 RELEASE 应答：st=ST_OK + count ≤ [`RELEASE_MAX`] + 恰长
/// （4 + count×4，尾随/截断一并拒）。
pub fn parse_release_rep(buf: &[u8]) -> Option<ReleaseRep> {
    if buf.len() < RELEASE_REP_HEAD || buf[0] != ST_OK {
        return None;
    }
    let count = buf[1] as usize;
    if count > RELEASE_MAX {
        return None;
    }
    if buf.len() != RELEASE_REP_HEAD + count * 4 {
        return None;
    }
    let mut rep = ReleaseRep {
        buffer_ids: [0; RELEASE_MAX],
        count,
    };
    for i in 0..count {
        rep.buffer_ids[i] = le32(buf, RELEASE_REP_HEAD + i * 4)?;
    }
    Some(rep)
}

/// 编码 1B 状态应答（一切非 OK 错误应答 + VOTE/POWER_MODE/SUBSCRIBE/
/// REG_VBLANK/COMMIT 的成功形；buf 不足 / 未知 st = 调用方错，assert）。
pub fn encode_status_rep(out: &mut [u8], st: u8) -> usize {
    assert!(st_known(st), "guiproto: 未知 st");
    assert!(!out.is_empty(), "guiproto: status rep buf 不足");
    out[0] = st;
    1
}

/// 解析 1B 状态应答：恰 1B + st 在册。烘焙线锚点：一切 st≠OK 的应答恒
/// 此形（对 1B 帧恒成功）。
pub fn parse_status_rep(buf: &[u8]) -> Option<u8> {
    if buf.len() != 1 {
        return None;
    }
    let st = buf[0];
    if !st_known(st) {
        return None;
    }
    Some(st)
}

// ---------- input 域 ----------

/// 编码 INPUT_SUBSCRIBE 请求（恰 4B `[op][delivery][r16]`；delivery 野值 /
/// buf 不足 = 调用方错，assert——解析侧对称拒）。
pub fn encode_input_sub_req(out: &mut [u8], delivery: u8) -> usize {
    assert!(delivery_known(delivery), "guiproto: 野 delivery");
    assert!(out.len() >= 4, "guiproto: input sub buf 不足");
    out[0] = OP_INPUT_SUBSCRIBE;
    out[1] = delivery;
    out[2] = 0;
    out[3] = 0;
    4
}

/// 解析 INPUT_SUBSCRIBE 请求：恰 4B + delivery ∈ {0,1}。
pub fn parse_input_sub_req(buf: &[u8]) -> Option<u8> {
    if buf.len() != 4 || buf[0] != OP_INPUT_SUBSCRIBE {
        return None;
    }
    let d = buf[1];
    if !delivery_known(d) {
        return None;
    }
    Some(d)
}

/// 编码 INPUT_POLL 请求（恰 4B `[op][max_events][r16]`；max_events 域
/// 1..=INPUT_EVENTS_MAX——0 拒（「拉零个事件」非法定义在协议层，netdproto
/// port 0 同款）；野值 / buf 不足 = 调用方错，assert）。
pub fn encode_input_poll_req(out: &mut [u8], max_events: u8) -> usize {
    assert!(
        max_events >= 1 && max_events as usize <= INPUT_EVENTS_MAX,
        "guiproto: 野 max_events"
    );
    assert!(out.len() >= 4, "guiproto: input poll buf 不足");
    out[0] = OP_INPUT_POLL;
    out[1] = max_events;
    out[2] = 0;
    out[3] = 0;
    4
}

/// 解析 INPUT_POLL 请求：恰 4B + max_events ∈ 1..=[`INPUT_EVENTS_MAX`]。
pub fn parse_input_poll_req(buf: &[u8]) -> Option<u8> {
    if buf.len() != 4 || buf[0] != OP_INPUT_POLL {
        return None;
    }
    let m = buf[1];
    if m < 1 || m as usize > INPUT_EVENTS_MAX {
        return None;
    }
    Some(m)
}

/// 编码 INPUT_POLL 事件批应答（`[st=0][count u8][r16][count 个
/// PointerEvent 依次串接]`，总长 ≤ [`INPUT_REP_MAX`]；events 空（该用 1B
/// `[ST_EAGAIN]` 形——`encode_status_rep`）或越帽 / out 不足 → None）。
pub fn encode_input_events_rep(out: &mut [u8], events: &[PointerEvent]) -> Option<usize> {
    if events.is_empty() || events.len() > INPUT_EVENTS_MAX {
        return None;
    }
    let mut total = INPUT_REP_HEAD;
    for ev in events {
        // 先界后乘（pointer_event_len 单源：越帽 None）——调用方可构造的野
        // history_len 在累加期即拒，绝不乘加溢出（encode_pointer_event 同序）。
        total += pointer_event_len(ev.history_len)?;
    }
    if total > INPUT_REP_MAX {
        return None;
    }
    let m = out.get_mut(..total)?;
    m[0] = ST_OK;
    m[1] = events.len() as u8;
    m[2] = 0;
    m[3] = 0;
    let mut off = INPUT_REP_HEAD;
    for ev in events {
        off += encode_pointer_event(&mut m[off..], ev)?;
    }
    debug_assert_eq!(off, total);
    Some(total)
}

/// INPUT_POLL 应答借用视图（事件逐个惰性重解析——零分配）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEventsRep<'a> {
    pub(crate) buf: &'a [u8],
    pub(crate) count: u8,
}

impl<'a> InputEventsRep<'a> {
    /// 事件数（1..=INPUT_EVENTS_MAX——成功形 count ≥ 1，空批走 1B EAGAIN）。
    pub fn count(&self) -> usize {
        self.count as usize
    }

    /// 取第 i 个事件（i ≥ count → None；事件流已整体校验，逐项重解析）。
    pub fn event(&self, i: usize) -> Option<PointerEvent> {
        if i >= self.count as usize {
            return None;
        }
        let mut off = 0usize;
        for k in 0..=i {
            let span = event_span(self.buf, off)?;
            let end = off.checked_add(span)?;
            if k == i {
                return parse_pointer_event(self.buf.get(off..end)?);
            }
            off = end;
        }
        None // 不可达（i < count 已检）
    }
}

/// 解析 INPUT_POLL 应答——**三态分离**（fsproto「撒谎长度拒」+ netdproto
/// 「1B 错误应答合法态」合成）：
/// - `None`：畸形（截断/尾随/事件边界撒谎/count 0 或越帽/1B OK 形/非 OK
///   携数据——成功必携 count ≥ 1，「无事件」走 1B `[ST_EAGAIN]` 形）；
/// - `Some(Err(st))`：合法 1B 错误应答（st ≠ OK 且在册）；
/// - `Some(Ok(rep))`：事件批。
pub fn parse_input_events_rep(buf: &[u8]) -> Option<Result<InputEventsRep<'_>, u8>> {
    let st = *buf.first()?;
    if !st_known(st) {
        return None;
    }
    if buf.len() == 1 {
        if st == ST_OK {
            return None; // 1B OK 形非规范（成功必携事件）
        }
        return Some(Err(st));
    }
    if st != ST_OK {
        return None; // 烘焙线：非 OK 恒 1B 不携数据
    }
    if buf.len() < INPUT_REP_HEAD {
        return None; // 2B/3B OK 形非法
    }
    let count = buf[1] as usize;
    if count == 0 || count > INPUT_EVENTS_MAX {
        return None;
    }
    let mut off = INPUT_REP_HEAD;
    for _ in 0..count {
        let span = event_span(buf, off)?;
        let end = off.checked_add(span)?;
        parse_pointer_event(buf.get(off..end)?)?; // 值域 + 恰长复检
        off = end;
    }
    if off != buf.len() {
        return None; // 尾随字节 = 野值
    }
    Some(Ok(InputEventsRep {
        buf: buf.get(INPUT_REP_HEAD..)?,
        count: count as u8,
    }))
}

// ---------- hdi 域 ----------

/// 编码 REG_VBLANK 请求（恰 4B `[op][enable][r16]`，随行 1 个 Event
/// handle；enable 布尔域 0/1——野值 = 调用方错，assert）。
pub fn encode_reg_vblank_req(out: &mut [u8], enable: u8) -> usize {
    assert!(enable <= 1, "guiproto: 野 enable");
    assert!(out.len() >= 4, "guiproto: reg vblank buf 不足");
    out[0] = OP_REG_VBLANK;
    out[1] = enable;
    out[2] = 0;
    out[3] = 0;
    4
}

/// 解析 REG_VBLANK 请求：恰 4B + enable ∈ {0,1}。
pub fn parse_reg_vblank_req(buf: &[u8]) -> Option<u8> {
    if buf.len() != 4 || buf[0] != OP_REG_VBLANK {
        return None;
    }
    let e = buf[1];
    if e > 1 {
        return None;
    }
    Some(e)
}

/// 编码 VBLANK_WAIT 请求（恰 4B `[op][r][r16]`，无参；解析面
/// [`parse_vblank_wait_req`]——GUI-2 段③ 消费侧首刀起在册）。
pub fn encode_vblank_wait_req(out: &mut [u8]) -> usize {
    assert!(out.len() >= 4, "guiproto: vblank wait buf 不足");
    out[0] = OP_VBLANK_WAIT;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    4
}

/// 解析 VBLANK_WAIT 请求：恰 4B + op 位（无参；保留域恒不校验——
/// GETINFO 无参面同款）。
pub fn parse_vblank_wait_req(buf: &[u8]) -> Option<()> {
    if buf.len() != 4 || buf[0] != OP_VBLANK_WAIT {
        return None;
    }
    Some(())
}

/// VBLANK_WAIT 应答（阻塞应答：`[st=0][r][r16][seq u32][ts_ns u64]`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VblankRep {
    /// vblank 序号。
    pub seq: u32,
    /// vblank 时刻。
    pub ts_ns: u64,
}

/// 编码 VBLANK_WAIT 应答（恰 16B；buf 不足 = 调用方错，assert）。
pub fn encode_vblank_rep(out: &mut [u8], rep: &VblankRep) -> usize {
    assert!(out.len() >= VBLANK_REP_LEN, "guiproto: vblank rep buf 不足");
    out[0] = ST_OK;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    out[4..8].copy_from_slice(&rep.seq.to_le_bytes());
    out[8..16].copy_from_slice(&rep.ts_ns.to_le_bytes());
    VBLANK_REP_LEN
}

/// 解析 VBLANK_WAIT 应答：恰 16B + st=ST_OK。
pub fn parse_vblank_rep(buf: &[u8]) -> Option<VblankRep> {
    if buf.len() != VBLANK_REP_LEN || buf[0] != ST_OK {
        return None;
    }
    Some(VblankRep {
        seq: le32(buf, 4)?,
        ts_ns: le64(buf, 8)?,
    })
}

/// PREPARE 请求（20B + 随行 1 个 buffer VMO handle）。format 是 FMT_*
/// **值**非位域；不在协商集 → ST_UNSUPPORTED 是**服务端**拒面（wire 层
/// 任意 u32 合法帧——编码/解析不拦，§3.5-6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrepareReq {
    /// v1 恒 0（双侧执法）——含预留位 [`PREPARE_F_SERVER_SAMPLABLE`]（bit0
    /// 已认领、语义冻结 0 不可采样，见其 doc；bit1..=7 未认领仍属「恒 0」
    /// 整体执法面）。
    pub flags: u8,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: u32,
}

/// 编码 PREPARE 请求（恰 20B `[op][flags][r16][w u32][h u32][stride u32]
/// [format u32]`；flags ≠ 0（含预留位 SERVER_SAMPLABLE——v1 冻结 0 不可
/// 采样）/ buf 不足 = 调用方错，assert）。
pub fn encode_prepare_req(out: &mut [u8], req: &PrepareReq) -> usize {
    assert!(req.flags == 0, "guiproto: prepare flags v1 恒 0");
    assert!(out.len() >= PREPARE_REQ_LEN, "guiproto: prepare req buf 不足");
    out[0] = OP_PREPARE;
    out[1] = req.flags;
    out[2] = 0;
    out[3] = 0;
    out[4..8].copy_from_slice(&req.width.to_le_bytes());
    out[8..12].copy_from_slice(&req.height.to_le_bytes());
    out[12..16].copy_from_slice(&req.stride.to_le_bytes());
    out[16..20].copy_from_slice(&req.format.to_le_bytes());
    PREPARE_REQ_LEN
}

/// 解析 PREPARE 请求：恰 20B + flags == 0（v1——含预留位
/// [`PREPARE_F_SERVER_SAMPLABLE`] 冻结 0，置位帧拒）；format 任意 u32 合法。
pub fn parse_prepare_req(buf: &[u8]) -> Option<PrepareReq> {
    if buf.len() != PREPARE_REQ_LEN || buf[0] != OP_PREPARE {
        return None;
    }
    let flags = buf[1];
    if flags != 0 {
        return None;
    }
    Some(PrepareReq {
        flags,
        width: le32(buf, 4)?,
        height: le32(buf, 8)?,
        stride: le32(buf, 12)?,
        format: le32(buf, 16)?,
    })
}

/// 编码 PREPARE 应答（恰 4B `[st=0][slot u8][r16]`，随行 1 个 fence
/// Event handle；slot 域 0..HDI_SLOTS_MAX-1——越帽 = 调用方错，assert）。
pub fn encode_prepare_rep(out: &mut [u8], slot: u8) -> usize {
    assert!(slot < HDI_SLOTS_MAX, "guiproto: slot 越帽");
    assert!(out.len() >= 4, "guiproto: prepare rep buf 不足");
    out[0] = ST_OK;
    out[1] = slot;
    out[2] = 0;
    out[3] = 0;
    4
}

/// 解析 PREPARE 应答：恰 4B + st=ST_OK + slot < [`HDI_SLOTS_MAX`]。
pub fn parse_prepare_rep(buf: &[u8]) -> Option<u8> {
    if buf.len() != 4 || buf[0] != ST_OK {
        return None;
    }
    let slot = buf[1];
    if slot >= HDI_SLOTS_MAX {
        return None;
    }
    Some(slot)
}

/// 编码 COMMIT 请求（恰 12B `[op][slot u8][r16][target u64]`，target =
/// 期望呈现 ns、0 = 尽快；slot 越帽 = 调用方错，assert）。
pub fn encode_commit_req(out: &mut [u8], slot: u8, target_ns: u64) -> usize {
    assert!(slot < HDI_SLOTS_MAX, "guiproto: slot 越帽");
    assert!(out.len() >= COMMIT_REQ_LEN, "guiproto: commit req buf 不足");
    out[0] = OP_COMMIT;
    out[1] = slot;
    out[2] = 0;
    out[3] = 0;
    out[4..12].copy_from_slice(&target_ns.to_le_bytes());
    COMMIT_REQ_LEN
}

/// 解析 COMMIT 请求：恰 12B + slot < [`HDI_SLOTS_MAX`] → (slot, target)。
pub fn parse_commit_req(buf: &[u8]) -> Option<(u8, u64)> {
    if buf.len() != COMMIT_REQ_LEN || buf[0] != OP_COMMIT {
        return None;
    }
    let slot = buf[1];
    if slot >= HDI_SLOTS_MAX {
        return None;
    }
    Some((slot, le64(buf, 4)?))
}

// ---------- wm 域（GUI-5：gui-wmd 窗口守护——权威窗口表 + 命中分发）----------
//
// 十组编解码对：GETINFO req/rep、CREATE req/ack、DESTROY req、SET req、
// TABLE_GET req、TABLE_WAIT req、快照应答（TABLE_GET/TABLE_WAIT 共用
// `encode_wm_snapshot_rep`/`parse_wm_snapshot_rep`）+ DESTROY/SET 的 1B OK
// 成功应答（复用 `encode_status_rep`/`parse_status_rep`）。ST_* 七码零新增
// （Q18）：id/parent 无效 → ST_NOENT、表满 → ST_FULL、layer/mask/flags/vis
// 野值 → ST_BAD（wire 层 None——值域执法在 parse/encode，引用存在性是
// 服务端策略面）；ST_BUSY 预留多客户端刀（v1 串行深 1 不可达）。

/// 编码 WM_GETINFO 请求（恰 4B `[op][r][r16]`，无参；`encode_vblank_wait_req`
/// 同款 assert 形）。
pub fn encode_wm_getinfo_req(out: &mut [u8]) -> usize {
    assert!(out.len() >= 4, "guiproto: wm getinfo req buf 不足");
    out[0] = OP_WM_GETINFO;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    4
}

/// 解析 WM_GETINFO 请求：恰 4B + op 位（保留域恒不校验——无参面同款）。
pub fn parse_wm_getinfo_req(buf: &[u8]) -> Option<()> {
    if buf.len() != 4 || buf[0] != OP_WM_GETINFO {
        return None;
    }
    Some(())
}

/// WM_GETINFO 应答（解析产物；12B 布局见 `encode_wm_getinfo_rep`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WmGetinfoRep {
    pub version_wm: u8,
    /// ≤ [`WM_WINDOWS_MAX`]（双侧执法——镜像 max_credits ≤ CREDITS_MAX）。
    pub windows_max: u8,
    /// 显示几何（wmd 启动期一次缓存；真值源 present GETINFO 逐调用中继
    /// 非自答——wmd 不持第二份 present 常量副本，Q15）。
    pub disp_w: u16,
    pub disp_h: u16,
}

/// 编码 WM_GETINFO 应答（恰 12B `[st=0][version_wm][windows_max][r]
/// [disp_w u16][disp_h u16][r u32]`；buf 不足 / windows_max 越帽 = 调用方
/// 错，assert——`encode_getinfo_rep` 先例）。
pub fn encode_wm_getinfo_rep(out: &mut [u8], rep: &WmGetinfoRep) -> usize {
    assert!(out.len() >= WM_GETINFO_REP_LEN, "guiproto: wm getinfo buf 不足");
    assert!(rep.windows_max as usize <= WM_WINDOWS_MAX, "guiproto: windows_max 越帽");
    out[0] = ST_OK;
    out[1] = rep.version_wm;
    out[2] = rep.windows_max;
    out[3] = 0;
    out[4..6].copy_from_slice(&rep.disp_w.to_le_bytes());
    out[6..8].copy_from_slice(&rep.disp_h.to_le_bytes());
    out[8..12].copy_from_slice(&0u32.to_le_bytes()); // r 恒 0
    WM_GETINFO_REP_LEN
}

/// 解析 WM_GETINFO 应答：恰 12B + st=ST_OK + windows_max ≤
/// [`WM_WINDOWS_MAX`]；保留位（off 3/8..12）恒不校验。
pub fn parse_wm_getinfo_rep(buf: &[u8]) -> Option<WmGetinfoRep> {
    if buf.len() != WM_GETINFO_REP_LEN || buf[0] != ST_OK {
        return None;
    }
    let windows_max = buf[2];
    if windows_max as usize > WM_WINDOWS_MAX {
        return None;
    }
    Some(WmGetinfoRep {
        version_wm: buf[1],
        windows_max,
        disp_w: le16(buf, 4)?,
        disp_h: le16(buf, 6)?,
    })
}

/// WM_CREATE 请求（解析产物）。x/y/w/h = **申请几何**（表项回显的是避让
/// 裁剪后的有效几何——申请值不回显，`WmEntry` doc 注对拍口径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WmCreateReq {
    /// [`WM_LAYER_APP`]/[`WM_LAYER_SYSTEM`]（值域双侧执法）。
    pub layer: u8,
    /// [`WM_CREATE_F_AVOID_SYS`] 位域（未知位双侧拒）。
    pub flags: u8,
    /// 父窗 id（0 = 无父——QNX 缝安全域聚合，v1 字段缝先占）。
    pub parent: u16,
    /// 组 id（v1 恒 0 合法，字段缝）。
    pub group: u8,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

/// 编码 WM_CREATE 请求（恰 16B `[op][layer][flags][r][parent u16][group][r]
/// [x u16][y u16][w u16][h u16]`；layer/flags 野值 / out 不足 → None）。
pub fn encode_wm_create_req(out: &mut [u8], req: &WmCreateReq) -> Option<usize> {
    if !wm_layer_known(req.layer) || !wm_create_flags_known(req.flags) {
        return None;
    }
    let m = out.get_mut(..WM_CREATE_REQ_LEN)?;
    m[0] = OP_WM_CREATE;
    m[1] = req.layer;
    m[2] = req.flags;
    m[3] = 0;
    m[4..6].copy_from_slice(&req.parent.to_le_bytes());
    m[6] = req.group;
    m[7] = 0;
    m[8..10].copy_from_slice(&req.x.to_le_bytes());
    m[10..12].copy_from_slice(&req.y.to_le_bytes());
    m[12..14].copy_from_slice(&req.w.to_le_bytes());
    m[14..16].copy_from_slice(&req.h.to_le_bytes());
    Some(WM_CREATE_REQ_LEN)
}

/// 解析 WM_CREATE 请求：恰 16B + op + layer/flags 值域对称拒；保留位
/// （off 3/7）恒不校验。
pub fn parse_wm_create_req(buf: &[u8]) -> Option<WmCreateReq> {
    if buf.len() != WM_CREATE_REQ_LEN || buf[0] != OP_WM_CREATE {
        return None;
    }
    let layer = buf[1];
    let flags = buf[2];
    if !wm_layer_known(layer) || !wm_create_flags_known(flags) {
        return None;
    }
    Some(WmCreateReq {
        layer,
        flags,
        parent: le16(buf, 4)?,
        group: buf[6],
        x: le16(buf, 8)?,
        y: le16(buf, 10)?,
        w: le16(buf, 12)?,
        h: le16(buf, 14)?,
    })
}

/// 编码 WM_CREATE 应答（恰 8B `[st=0][r][r16][window_id u16][r16]`——
/// SUBMIT ack 同形 u16 缩宽；buf 不足 = 调用方错，assert）。
pub fn encode_wm_create_ack(out: &mut [u8], window_id: u16) -> usize {
    assert!(out.len() >= WM_CREATE_ACK_LEN, "guiproto: wm create ack buf 不足");
    out[0] = ST_OK;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    out[4..6].copy_from_slice(&window_id.to_le_bytes());
    out[6..8].copy_from_slice(&0u16.to_le_bytes());
    WM_CREATE_ACK_LEN
}

/// 解析 WM_CREATE 应答：恰 8B + st=ST_OK → window_id（id≠0 由 wmd 分配
/// 策略保证——gfxdemo 建窗断言面，wire 层不执法）。
pub fn parse_wm_create_ack(buf: &[u8]) -> Option<u16> {
    if buf.len() != WM_CREATE_ACK_LEN || buf[0] != ST_OK {
        return None;
    }
    le16(buf, 4)
}

/// 编码 WM_DESTROY 请求（恰 8B `[op][r][r16][window_id u16][r16]`；buf
/// 不足 = 调用方错，assert）。应答 1B OK——`encode_status_rep` 复用。
pub fn encode_wm_destroy_req(out: &mut [u8], window_id: u16) -> usize {
    assert!(out.len() >= WM_DESTROY_REQ_LEN, "guiproto: wm destroy buf 不足");
    out[0] = OP_WM_DESTROY;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    out[4..6].copy_from_slice(&window_id.to_le_bytes());
    out[6..8].copy_from_slice(&0u16.to_le_bytes());
    WM_DESTROY_REQ_LEN
}

/// 解析 WM_DESTROY 请求：恰 8B + op → window_id（存在性 = 服务端 ST_NOENT
/// 策略面，wire 不拦）。
pub fn parse_wm_destroy_req(buf: &[u8]) -> Option<u16> {
    if buf.len() != WM_DESTROY_REQ_LEN || buf[0] != OP_WM_DESTROY {
        return None;
    }
    le16(buf, 4)
}

/// WM_SET 请求（解析产物；20B 掩码式，Q17）。移动+缩放+可见+Z 同帧原子
/// 生效（表版本恰 +1，inputd 恰醒一次收一张快照）。masked-out 槽位值无
/// 语义仍编入（定长 +「编什么收什么」——禁实现侧偷偷清零，解析对称回显）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WmSetReq {
    /// [`WM_SET_F_GEOM`]/[`WM_SET_F_VIS`]/[`WM_SET_F_Z`] 位域（mask=0 /
    /// 未知位双侧拒）。
    pub mask: u8,
    pub window_id: u16,
    /// 几何槽（GEOM 位选中才生效——申请几何，有效几何推导在 wmd）。
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    /// 可见槽（[`WM_VIS_HIDE`]/[`WM_VIS_SHOW`]——值域无条件双侧执法，Q18；
    /// VIS 位选中才生效）。
    pub vis: u8,
    /// Z 槽（任意 u8——z 平铺客户端自选语义，Q5）。
    pub z: u8,
}

/// 编码 WM_SET 请求（恰 20B `[op][mask][r16][window_id u16][r16][x u16]
/// [y u16][w u16][h u16][vis][z][r16]`；mask=0/未知位、vis 野值 / out
/// 不足 → None）。
pub fn encode_wm_set_req(out: &mut [u8], req: &WmSetReq) -> Option<usize> {
    if !wm_set_mask_known(req.mask) || !wm_vis_known(req.vis) {
        return None;
    }
    let m = out.get_mut(..WM_SET_REQ_LEN)?;
    m[0] = OP_WM_SET;
    m[1] = req.mask;
    m[2] = 0;
    m[3] = 0;
    m[4..6].copy_from_slice(&req.window_id.to_le_bytes());
    m[6] = 0;
    m[7] = 0;
    m[8..10].copy_from_slice(&req.x.to_le_bytes());
    m[10..12].copy_from_slice(&req.y.to_le_bytes());
    m[12..14].copy_from_slice(&req.w.to_le_bytes());
    m[14..16].copy_from_slice(&req.h.to_le_bytes());
    m[16] = req.vis;
    m[17] = req.z;
    m[18] = 0;
    m[19] = 0;
    Some(WM_SET_REQ_LEN)
}

/// 解析 WM_SET 请求：恰 20B + op + mask/vis 值域对称拒；保留位（off
/// 2..4/6..8/18..20）恒不校验；masked-out 槽位照收不清零。
pub fn parse_wm_set_req(buf: &[u8]) -> Option<WmSetReq> {
    if buf.len() != WM_SET_REQ_LEN || buf[0] != OP_WM_SET {
        return None;
    }
    let mask = buf[1];
    let vis = buf[16];
    if !wm_set_mask_known(mask) || !wm_vis_known(vis) {
        return None;
    }
    Some(WmSetReq {
        mask,
        window_id: le16(buf, 4)?,
        x: le16(buf, 8)?,
        y: le16(buf, 10)?,
        w: le16(buf, 12)?,
        h: le16(buf, 14)?,
        vis,
        z: buf[17],
    })
}

/// 编码 WM_TABLE_GET 请求（恰 4B `[op][r][r16]`，无参非阻塞；buf 不足 =
/// 调用方错，assert）。
pub fn encode_wm_table_get_req(out: &mut [u8]) -> usize {
    assert!(out.len() >= 4, "guiproto: wm table get buf 不足");
    out[0] = OP_WM_TABLE_GET;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    4
}

/// 解析 WM_TABLE_GET 请求：恰 4B + op 位（保留域恒不校验）。
pub fn parse_wm_table_get_req(buf: &[u8]) -> Option<()> {
    if buf.len() != 4 || buf[0] != OP_WM_TABLE_GET {
        return None;
    }
    Some(())
}

/// 编码 WM_TABLE_WAIT 请求（恰 8B `[op][r][r16][last_seen u32]`——**库内
/// 首个带参阻塞请求**：wmd park 至表版本新于 last_seen 应答快照，表已新
/// 立即答（last_seen=0 = 冷启动恒立即答——version 恒 ≥1）；buf 不足 =
/// 调用方错，assert）。
pub fn encode_wm_table_wait_req(out: &mut [u8], last_seen: u32) -> usize {
    assert!(out.len() >= WM_TABLE_WAIT_REQ_LEN, "guiproto: wm table wait buf 不足");
    out[0] = OP_WM_TABLE_WAIT;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    out[4..8].copy_from_slice(&last_seen.to_le_bytes());
    WM_TABLE_WAIT_REQ_LEN
}

/// 解析 WM_TABLE_WAIT 请求：恰 8B + op → last_seen（任意 u32 合法——纪元
/// 回退角由消费侧超龄 WAIT(0) 重探测兜底，Q3）。
pub fn parse_wm_table_wait_req(buf: &[u8]) -> Option<u32> {
    if buf.len() != WM_TABLE_WAIT_REQ_LEN || buf[0] != OP_WM_TABLE_WAIT {
        return None;
    }
    le32(buf, 4)
}

/// 窗口表项（快照 16B 定宽，Q5）。x/y/w/h = **有效几何域**——避让裁剪后的
/// 实际可用矩形（wmd 不变量式重推导维护，任何 CREATE/DESTROY/SET 触发受
/// 影响表项重算）；申请几何不回显（v1 无系统层重叠时有效 == 申请，GUI-8
/// 加系统窗时客户端对拍断言须随改）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WmEntry {
    /// 窗口 id（wmd 分配；id≠0 由分配策略保证，wire 层任意 u16 合法帧）。
    pub id: u16,
    /// 父窗 id（0 = 无父）。
    pub parent: u16,
    /// 组 id（v1 恒 0，字段缝）。
    pub group: u8,
    /// 有效几何（半开矩形 `[x, x+w) × [y, y+h)`，整数物理像素）。
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    /// [`WM_LAYER_APP`]/[`WM_LAYER_SYSTEM`]（两层窗 v1 下即类型优先级一维
    /// 表，规划 :234 WMM 参照裁语；值域双侧执法）。
    pub layer: u8,
    /// 同层 Z 序（客户端自选平铺；服务端归一化 = VER_WM 升版面）。
    pub z: u8,
    /// [`WM_WIN_F_VISIBLE`]/[`WM_WIN_F_FOCUSED`] 位域（未知位双侧拒）。
    pub flags: u8,
}

/// 窗口表快照（TABLE_GET/TABLE_WAIT 共用应答形；owned 定容数组 + count
/// ——模块头注惯例）。version u32 = `(epoch u8 << 24) | counter u24`，
/// counter ∈ 1..=0xFFFFFF 跳 0 ⇒ 服务端恒发 ≥1（WAIT(0) = 冷启动立即答）；
/// epoch 取 wmd 启动 CNTVCT 派生——无持久化下唯一跨启动区分源（「新于
/// last_seen」为大概率非必然，纪元回退由消费侧超龄 WAIT(0) 重探测兜底，
/// Q3；wire 层 version 任意 u32 合法不执法）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WmSnapshot {
    pub version: u32,
    /// 显示几何域（与 WM_GETINFO 缓存同源——inputd 命中坐标系对齐锚）。
    pub disp_w: u16,
    pub disp_h: u16,
    /// 窗口表（前 `count` 项有效，表序 = 平局裁决序）。
    pub entries: [WmEntry; WM_WINDOWS_MAX],
    /// 表项数（0 = 合法空表——窗口表空是真实初始态，区别于 INPUT_POLL
    /// count 0 拒（空批有 1B EAGAIN 替代形），Q15）。
    pub count: usize,
}

/// 编码窗口表快照应答（TABLE_GET/TABLE_WAIT 共用；12B 头 `[st=0][count u8]
/// [r16][version u32][disp_w u16][disp_h u16]` + count×16B 表项，总长 ≤
/// [`WM_REP_MAX`]；count 越帽 / 表项 layer·flags 野值 / out 不足 → None，
/// 不截断）。
pub fn encode_wm_snapshot_rep(out: &mut [u8], snap: &WmSnapshot) -> Option<usize> {
    if snap.count > WM_WINDOWS_MAX {
        return None;
    }
    for e in snap.entries.iter().take(snap.count) {
        if !wm_layer_known(e.layer) || !wm_win_flags_known(e.flags) {
            return None;
        }
    }
    let total = WM_SNAP_HEAD + snap.count * WM_ENTRY_LEN;
    let m = out.get_mut(..total)?;
    m[0] = ST_OK;
    m[1] = snap.count as u8;
    m[2] = 0;
    m[3] = 0;
    m[4..8].copy_from_slice(&snap.version.to_le_bytes());
    m[8..10].copy_from_slice(&snap.disp_w.to_le_bytes());
    m[10..12].copy_from_slice(&snap.disp_h.to_le_bytes());
    let mut off = WM_SNAP_HEAD;
    for e in snap.entries.iter().take(snap.count) {
        m[off..off + 2].copy_from_slice(&e.id.to_le_bytes());
        m[off + 2..off + 4].copy_from_slice(&e.parent.to_le_bytes());
        m[off + 4] = e.group;
        m[off + 5..off + 7].copy_from_slice(&e.x.to_le_bytes());
        m[off + 7..off + 9].copy_from_slice(&e.y.to_le_bytes());
        m[off + 9..off + 11].copy_from_slice(&e.w.to_le_bytes());
        m[off + 11..off + 13].copy_from_slice(&e.h.to_le_bytes());
        m[off + 13] = e.layer;
        m[off + 14] = e.z;
        m[off + 15] = e.flags;
        off += WM_ENTRY_LEN;
    }
    Some(total)
}

/// 解析窗口表快照应答——**三态分离**（`parse_input_events_rep` 同形）：
/// - `None`：畸形（截断/尾随/count 越帽/表项 layer·flags 野值/1B OK 形/
///   非 OK 携数据）；
/// - `Some(Err(st))`：合法 1B 错误应答（wmd 未上线/降级面 `[ST_EAGAIN]` 等）；
/// - `Some(Ok(snap))`：快照（count=0 = 合法空表）。
pub fn parse_wm_snapshot_rep(buf: &[u8]) -> Option<Result<WmSnapshot, u8>> {
    let st = *buf.first()?;
    if !st_known(st) {
        return None;
    }
    if buf.len() == 1 {
        if st == ST_OK {
            return None; // 1B OK 形非规范（成功必携 12B 头——空表也是头形）
        }
        return Some(Err(st));
    }
    if st != ST_OK {
        return None; // 烘焙线：非 OK 恒 1B 不携数据
    }
    if buf.len() < WM_SNAP_HEAD {
        return None; // 2..12B OK 形非法
    }
    let count = buf[1] as usize;
    if count > WM_WINDOWS_MAX {
        return None;
    }
    if buf.len() != WM_SNAP_HEAD + count * WM_ENTRY_LEN {
        return None; // 恰长（尾随/截断一并拒）
    }
    let mut entries = [WmEntry {
        id: 0,
        parent: 0,
        group: 0,
        x: 0,
        y: 0,
        w: 0,
        h: 0,
        layer: 0,
        z: 0,
        flags: 0,
    }; WM_WINDOWS_MAX];
    for i in 0..count {
        let off = WM_SNAP_HEAD + i * WM_ENTRY_LEN;
        let layer = buf[off + 13];
        let flags = buf[off + 15];
        if !wm_layer_known(layer) || !wm_win_flags_known(flags) {
            return None;
        }
        entries[i] = WmEntry {
            id: le16(buf, off)?,
            parent: le16(buf, off + 2)?,
            group: buf[off + 4],
            x: le16(buf, off + 5)?,
            y: le16(buf, off + 7)?,
            w: le16(buf, off + 9)?,
            h: le16(buf, off + 11)?,
            layer,
            z: buf[off + 14],
            flags,
        };
    }
    Some(Ok(WmSnapshot {
        version: le32(buf, 4)?,
        disp_w: le16(buf, 8)?,
        disp_h: le16(buf, 10)?,
        entries,
        count,
    }))
}

/// 窗口命中测试（纯函数，**单源**——wmd 权威面与 inputd 快照面共用本实现，
/// 禁两处手写：算法挨着 `WmEntry` 数据类型随 wire 同刀演进，Q9 裁决）。
///
/// 选举序：仅 [`WM_WIN_F_VISIBLE`] 置位项参选 → layer 降序（系统层压应用
/// 层）→ z 降序（同层高位在上）→ 点入有效矩形（半开 `[x, x+w) × [y, y+h)`
/// ——w=0/h=0 恒不中）；layer 与 z 双平局取表序靠前。坐标 x/y 为 u32
/// （PointerEvent 口径），表项几何 u16 提升比较（u16+u16 ⊂ u32 无溢出）。
pub fn wm_hit_test(entries: &[WmEntry], x: u32, y: u32) -> Option<u16> {
    let mut best: Option<&WmEntry> = None;
    for e in entries {
        if e.flags & WM_WIN_F_VISIBLE == 0 {
            continue; // 隐藏窗不参选（让击穿由「不参选」自然发生）
        }
        let hit = x >= e.x as u32
            && x < e.x as u32 + e.w as u32
            && y >= e.y as u32
            && y < e.y as u32 + e.h as u32;
        if !hit {
            continue;
        }
        best = match best {
            // 严格大于：平局保表序靠前（先见者胜）。
            Some(b) if e.layer > b.layer || (e.layer == b.layer && e.z > b.z) => Some(e),
            Some(b) => Some(b),
            None => Some(e),
        };
    }
    best.map(|e| e.id)
}

// ---------- 帽一致性 const assert（§3.1/§8.1-5；改帽必炸）----------

const _: () = assert!(PRESENT_REQ_MAX == 24 + 16 * DAMAGE_MAX);
const _: () = assert!(PRESENT_REP_MAX == 24);
const _: () = assert!(INPUT_EVT_MAX_LEN == 32 + 16 * (INPUT_SAMPLES_MAX - 1));
const _: () = assert!(INPUT_REP_MAX == 4 + INPUT_EVENTS_MAX * INPUT_EVT_MAX_LEN);
const _: () = assert!(HDI_REQ_MAX == 20);
const _: () = assert!(HDI_REP_MAX == 16);
// 一切帧帽 ≤ CHAN_MSG_MAX = 256（kernel/src/channel.rs 单一事实源）。
const _: () = assert!(PRESENT_REQ_MAX <= 256);
const _: () = assert!(PRESENT_REP_MAX <= 256);
const _: () = assert!(INPUT_EVT_MAX_LEN <= 256);
const _: () = assert!(INPUT_REP_MAX <= 256);
const _: () = assert!(HDI_REQ_MAX <= 256);
const _: () = assert!(HDI_REP_MAX <= 256);
// wm 域帧算术（§7 Q5：快照单帧 12 + 15×16 = 252 ≤ CHAN_MSG_MAX 硬约束）。
const _: () = assert!(WM_WINDOWS_MAX == 15);
const _: () = assert!(WM_REQ_MAX == 20);
const _: () = assert!(WM_REP_MAX == 12 + 15 * 16);
const _: () = assert!(WM_REP_MAX == WM_SNAP_HEAD + WM_WINDOWS_MAX * WM_ENTRY_LEN);
const _: () = assert!(WM_REQ_MAX <= 256);
const _: () = assert!(WM_REP_MAX <= 256);

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试样本事件（history_len ∈ 0..=3，采样值随 seed 区分防静默错位）。
    fn sample_ev(history_len: usize, seed: u64) -> PointerEvent {
        let mut history = [Sample { timestamp_ns: 0, x: 0, y: 0 }; INPUT_SAMPLES_MAX - 1];
        for (i, s) in history.iter_mut().enumerate().take(history_len) {
            *s = Sample {
                timestamp_ns: seed + i as u64,
                x: 10 + i as u32,
                y: 20 + i as u32,
            };
        }
        PointerEvent {
            phase: PHASE_MOVE,
            device_id: 1,
            buttons: BUTTON_L,
            flags: PE_FL_RESAMPLED,
            pointer_id: 7,
            timestamp_ns: 0x0102_0304_0506_0708,
            x: 400,
            y: 300,
            pressure: 500,
            contact_major: 0,
            contact_minor: 0,
            history,
            history_len,
        }
    }

    /// 测试样本 SUBMIT（rect_count/flags 参数化；rect 首项 {1,2,3,4}，尾槽
    /// 置零与解析产物规范形对齐——wire 只携 count 内槽）。
    fn submit_req(rect_count: usize, flags: u8) -> PresentReq {
        let mut rects = [Rect { x: 0, y: 0, w: 0, h: 0 }; DAMAGE_MAX];
        for r in rects.iter_mut().take(rect_count) {
            *r = Rect { x: 1, y: 2, w: 3, h: 4 };
        }
        PresentReq::Submit {
            surface_id: 7,
            buffer_id: 9,
            flags,
            rate_hint: 60,
            target_present_ns: 0x0102_0304_0506_0708,
            rects,
            rect_count,
        }
    }

    // ===== 1. 帽钉板（§8.1-5：改帽必炸——const assert 的可见镜像）=====

    #[test]
    fn const_frame_caps_pinned() {
        assert_eq!(DAMAGE_MAX, 8);
        assert_eq!(RELEASE_MAX, 4);
        assert_eq!(CREDITS_MAX, 2);
        assert_eq!(INPUT_SAMPLES_MAX, 4);
        assert_eq!(INPUT_EVENTS_MAX, 3);
        assert_eq!(HDI_SLOTS_MAX, 2);
        assert_eq!(PRESENT_REQ_MAX, 152);
        assert_eq!(PRESENT_REP_MAX, 24);
        assert_eq!(INPUT_EVT_MAX_LEN, 80);
        assert_eq!(INPUT_REP_MAX, 244);
        assert_eq!(HDI_REQ_MAX, 20);
        assert_eq!(HDI_REP_MAX, 16);
        // 随行 handle 面钉板（服务端 recv 容量按「协议定值 + 富余」给）。
        assert_eq!(ATTACH_SUBMIT, 1);
        assert_eq!(ATTACH_REG_VBLANK, 1);
        assert_eq!(ATTACH_PREPARE, 1);
        assert_eq!(REPLY_ATTACH_PREPARE, 1);
        assert_eq!(REPLY_ATTACH_RELEASE_MAX, 4);
        // 重采样常量单源（§3.6：消费侧 import，不长第二套）。
        assert_eq!(RESAMPLE_LATENCY_MS, 5);
        assert_eq!(RESAMPLE_MIN_DELTA_MS, 2);
        assert_eq!(RESAMPLE_MAX_DELTA_MS, 20);
        assert_eq!(RESAMPLE_MAX_PREDICTION_MS, 8);
        assert_eq!((VER_PRESENT, VER_INPUT, VER_HDI), (1, 1, 1));
        // wm 域帽钉板（GUI-5 §7 Q5——wm const assert 的可见镜像）。
        assert_eq!(WM_WINDOWS_MAX, 15);
        assert_eq!(WM_REQ_MAX, 20);
        assert_eq!(WM_REP_MAX, 252);
        assert_eq!(VER_WM, 1);
    }

    // ===== 2. present 域 =====

    #[test]
    fn getinfo_roundtrip_and_wire_golden() {
        // 版本三元组取相异值 (1,2,3)——相邻同型字段互换必炸（C-2a「钉板
        // 要钉全」纪律；全 1 形态下偏移互换字节相同、黄金测恒绿）。
        let rep = GetinfoRep {
            version_present: 1,
            version_input: 2,
            version_hdi: 3,
            format_bits: FMT_XRGB8888 | FMT_RGB565,
            display_w: 800,
            display_h: 600,
            max_credits: 2,
        };
        let mut buf = [0u8; PRESENT_REP_MAX];
        let n = encode_getinfo_rep(&mut buf, &rep);
        assert_eq!(n, 24);
        // 黄金字节钉板（改布局 = 此测必炸）。
        let want: [u8; 24] = [
            0, 1, 2, 3, //
            0, 0, 0, 0, //
            0x05, 0, 0, 0, //
            0x20, 0x03, 0, 0, //
            0x58, 0x02, 0, 0, //
            2, 0, 0, 0,
        ];
        assert_eq!(&buf[..n], &want[..]);
        assert_eq!(parse_getinfo_rep(&buf[..n]), Some(rep));
        // format_bits 未知位前向兼容不拒（v2 加格式不改旧读端）。
        let mut rep2 = rep;
        rep2.format_bits |= 1 << 31;
        let mut b2 = [0u8; PRESENT_REP_MAX];
        encode_getinfo_rep(&mut b2, &rep2);
        assert_eq!(parse_getinfo_rep(&b2[..24]), Some(rep2));
    }

    #[test]
    fn getinfo_rejects_truncation_and_domain() {
        let mut buf = [0u8; PRESENT_REP_MAX];
        encode_getinfo_rep(
            &mut buf,
            &GetinfoRep {
                version_present: 1,
                version_input: 1,
                version_hdi: 1,
                format_bits: FMT_XRGB8888,
                display_w: 800,
                display_h: 600,
                max_credits: 2,
            },
        );
        // 定长帧截断每切点全拒。
        for cut in 0..PRESENT_REP_MAX {
            assert_eq!(parse_getinfo_rep(&buf[..cut]), None, "截断 @{cut} 应拒");
        }
        // 尾随字节。
        let mut tail = [0u8; PRESENT_REP_MAX + 1];
        tail[..PRESENT_REP_MAX].copy_from_slice(&buf);
        assert_eq!(parse_getinfo_rep(&tail), None);
        // st 坏 / credits 越帽。
        let mut bad = buf;
        bad[0] = ST_BAD;
        assert_eq!(parse_getinfo_rep(&bad), None);
        let mut over = buf;
        over[20] = CREDITS_MAX + 1;
        assert_eq!(parse_getinfo_rep(&over), None);
    }

    #[test]
    #[allow(deprecated)] // Vote scene 沿用退役 SCENE_ANIMATED——老值在新域仍合法（跨版本兼容面）
    fn present_req_all_ops_roundtrip() {
        let reqs = [
            PresentReq::Getinfo,
            PresentReq::Submit {
                surface_id: 7,
                buffer_id: 9,
                flags: SUBMIT_F_DAMAGE_FULL | SUBMIT_F_UNSQUASHABLE,
                rate_hint: 120,
                target_present_ns: u64::MAX,
                rects: [Rect { x: 1, y: 2, w: 3, h: 4 }; DAMAGE_MAX],
                rect_count: DAMAGE_MAX,
            },
            PresentReq::FrameCallback { frame_token: u32::MAX },
            PresentReq::ReleaseWait,
            PresentReq::Vote {
                sticky: 1,
                scene: SCENE_ANIMATED,
                min_hz: 1,
                max_hz: 120,
                preferred_hz: 60,
            },
            PresentReq::PowerMode { mode: POWER_AOD },
        ];
        for req in &reqs {
            let mut buf = [0u8; PRESENT_REQ_MAX];
            let n = encode_present_req(&mut buf, req).expect("合法请求应编出");
            assert_eq!(parse_present_req(&buf[..n]), Some(*req));
        }
    }

    #[test]
    fn submit_k_boundaries() {
        let mut buf = [0u8; PRESENT_REQ_MAX];
        // K=0 无 DAMAGE_FULL：wire 合法帧（「K>0 或 FULL 二取一」是服务端
        // ST_BAD 拒面，§3.5-1——协议层不拦）。
        let k0 = submit_req(0, 0);
        let n0 = encode_present_req(&mut buf, &k0).unwrap();
        assert_eq!(n0, 24);
        assert_eq!(parse_present_req(&buf[..n0]), Some(k0));
        // K=DAMAGE_MAX 恰满帽 = PRESENT_REQ_MAX。
        let k8 = submit_req(DAMAGE_MAX, 0);
        let n8 = encode_present_req(&mut buf, &k8).unwrap();
        assert_eq!(n8, PRESENT_REQ_MAX);
        assert_eq!(parse_present_req(&buf[..n8]), Some(k8));
    }

    #[test]
    fn submit_wire_golden() {
        let req = submit_req(1, 0);
        let mut buf = [0u8; PRESENT_REQ_MAX];
        let n = encode_present_req(&mut buf, &req).unwrap();
        assert_eq!(n, 40);
        let want: [u8; 40] = [
            2, 0, 0, 0, //
            7, 0, 0, 0, //
            9, 0, 0, 0, //
            0, 60, 0, 0, //
            0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, //
            1, 0, 0, 0, //
            2, 0, 0, 0, //
            3, 0, 0, 0, //
            4, 0, 0, 0,
        ];
        assert_eq!(&buf[..n], &want[..]);
        // LE 钉板：target_present_ns 首字节 = 最低位字节。
        assert_eq!(buf[16], 0x08);
    }

    #[test]
    fn submit_reject_matrix() {
        let mut buf = [0u8; PRESENT_REQ_MAX + 16];
        // 编码拒绝面：K 越帽 / 未知 flags 位。
        assert_eq!(encode_present_req(&mut buf, &submit_req(DAMAGE_MAX + 1, 0)), None);
        assert_eq!(
            encode_present_req(&mut buf, &submit_req(1, SUBMIT_F_DAMAGE_FULL | 0x04)),
            None
        );
        // 基准帧 K=1。
        let n = encode_present_req(&mut buf, &submit_req(1, 0)).unwrap();
        assert_eq!(n, 40);
        // 截断每切点拒——cut=24 除外（恰是合法 K=0 形：定长头即完整帧）。
        for cut in 0..n {
            if cut == 24 {
                continue;
            }
            assert_eq!(parse_present_req(&buf[..cut]), None, "截断 @{cut} 应拒");
        }
        // 尾随字节。
        assert_eq!(parse_present_req(&buf[..n + 1]), None);
        // 16B 对齐尾数（25B）。
        assert_eq!(parse_present_req(&buf[..25]), None);
        // K 越帽（24 + 9×16 = 168B 合成帧）。
        let mut over = [0u8; 24 + (DAMAGE_MAX + 1) * 16];
        over[0] = OP_SUBMIT;
        assert_eq!(parse_present_req(&over), None);
        // 未知 flags 位（0x80）。
        let mut fl = buf;
        fl[12] = 0x80;
        assert_eq!(parse_present_req(&fl[..n]), None);
        // 未知 op / 空。
        assert_eq!(parse_present_req(&[0x07, 0, 0, 0]), None);
        assert_eq!(parse_present_req(&[]), None);
    }

    #[test]
    fn frame_done_roundtrip_golden_and_rejects() {
        let rep = FrameDoneRep {
            status: FRAME_PRESENTED,
            credits: 2,
            present_seq: 0xA1B2_C3D4,
            presented_ns: 0x1122_3344_5566_7788,
        };
        let mut buf = [0u8; 16];
        let n = encode_frame_done_rep(&mut buf, &rep);
        assert_eq!(n, 16);
        // 黄金字节钉板。
        let want: [u8; 16] = [
            0, 1, 2, 0, //
            0xD4, 0xC3, 0xB2, 0xA1, //
            0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11,
        ];
        assert_eq!(&buf[..n], &want[..]);
        assert_eq!(parse_frame_done_rep(&buf[..n]), Some(rep));
        // DISCARDED 终态 + 顶格值。
        let rep2 = FrameDoneRep {
            status: FRAME_DISCARDED,
            credits: 0,
            present_seq: u32::MAX,
            presented_ns: u64::MAX,
        };
        encode_frame_done_rep(&mut buf, &rep2);
        assert_eq!(parse_frame_done_rep(&buf[..16]), Some(rep2));
        // 截断每切点 / 尾随 / st 坏 / status 0 或 3 / credits 越帽。
        encode_frame_done_rep(&mut buf, &rep);
        for cut in 0..16 {
            assert_eq!(parse_frame_done_rep(&buf[..cut]), None, "截断 @{cut} 应拒");
        }
        let mut t = [0u8; 17];
        t[..16].copy_from_slice(&buf);
        assert_eq!(parse_frame_done_rep(&t), None); // 17B 拒
        let mut bad = buf;
        bad[0] = ST_BAD;
        assert_eq!(parse_frame_done_rep(&bad), None);
        let mut st0 = buf;
        st0[1] = 0;
        assert_eq!(parse_frame_done_rep(&st0), None);
        let mut st3 = buf;
        st3[1] = 3;
        assert_eq!(parse_frame_done_rep(&st3), None);
        let mut cr = buf;
        cr[2] = CREDITS_MAX + 1;
        assert_eq!(parse_frame_done_rep(&cr), None);
    }

    #[test]
    fn release_rep_roundtrip_and_boundaries() {
        // count 0..=RELEASE_MAX 全边界往返。
        for k in 0..=RELEASE_MAX {
            let ids: Vec<u32> = (0..k).map(|i| 0x100 + i as u32).collect();
            let mut buf = [0u8; 4 + RELEASE_MAX * 4];
            let n = encode_release_rep(&mut buf, &ids).unwrap();
            assert_eq!(n, 4 + k * 4);
            let rep = parse_release_rep(&buf[..n]).unwrap();
            assert_eq!(rep.count, k);
            for i in 0..k {
                assert_eq!(rep.buffer_ids[i], ids[i]);
            }
        }
        // 编码拒绝面：越帽。
        let five = [1u32; RELEASE_MAX + 1];
        let mut buf = [0u8; 4 + (RELEASE_MAX + 1) * 4];
        assert_eq!(encode_release_rep(&mut buf, &five), None);
        // 解析拒绝面：count 越帽 / 截断每切点 / 尾随 / 非 OK 形（烘焙线：
        // 错误应答是 1B status 形，不进本解析器）。
        assert_eq!(parse_release_rep(&[ST_OK, 5, 0, 0]), None);
        let ids = [7u32, 9];
        let mut b2 = [0u8; 12];
        let n = encode_release_rep(&mut b2, &ids).unwrap();
        for cut in 0..n {
            assert_eq!(parse_release_rep(&b2[..cut]), None, "截断 @{cut} 应拒");
        }
        let mut tail = [0u8; 13];
        tail[..n].copy_from_slice(&b2);
        assert_eq!(parse_release_rep(&tail), None);
        assert_eq!(parse_release_rep(&[ST_BAD]), None);
    }

    #[test]
    #[allow(deprecated)] // 黄金字节沿退役 SCENE_DRAG(=2)——跨版本流钉板（值在 intent 域内合法）
    fn vote_roundtrip_domain_and_golden() {
        let req = PresentReq::Vote {
            sticky: 1,
            scene: SCENE_DRAG,
            min_hz: 30,
            max_hz: 120,
            preferred_hz: 60,
        };
        let mut buf = [0u8; VOTE_REQ_LEN];
        let n = encode_present_req(&mut buf, &req).unwrap();
        assert_eq!(n, 12);
        // 黄金字节钉板。
        let want: [u8; 12] = [
            5, 1, 2, 0, //
            0x1E, 0x00, //
            0x78, 0x00, //
            0x3C, 0x00, //
            0, 0,
        ];
        assert_eq!(&buf[..n], &want[..]);
        assert_eq!(parse_present_req(&buf[..n]), Some(req));
        // 全值域往返：intent 六档 0x00..=0x05（GUI-8 §4 段①起 scene 位携
        // 六档值；退役 SCENE 值 0..=4 含于其内——跨版本兼容面）。
        for s in 0..=Intent::Maintenance as u8 {
            let r = PresentReq::Vote {
                sticky: 0,
                scene: s,
                min_hz: 0,
                max_hz: u16::MAX,
                preferred_hz: u16::MAX,
            };
            let n = encode_present_req(&mut buf, &r).unwrap();
            assert_eq!(parse_present_req(&buf[..n]), Some(r));
        }
        // 值域拒绝：sticky 2 / scene 6（intent 保留段 0x06.. 首——编码 +
        // 解析对称；scene 5 = INTENT_MAINTENANCE GUI-8 起入域，旧拒点 5
        // 让位 6）。
        let bad_sticky = PresentReq::Vote {
            sticky: 2,
            scene: SCENE_NORMAL,
            min_hz: 0,
            max_hz: 0,
            preferred_hz: 0,
        };
        assert_eq!(encode_present_req(&mut buf, &bad_sticky), None);
        let mut f = want;
        f[1] = 2;
        assert_eq!(parse_present_req(&f), None);
        let bad_scene = PresentReq::Vote {
            sticky: 0,
            scene: Intent::Maintenance as u8 + 1,
            min_hz: 0,
            max_hz: 0,
            preferred_hz: 0,
        };
        assert_eq!(encode_present_req(&mut buf, &bad_scene), None);
        let mut f2 = want;
        f2[2] = Intent::Maintenance as u8 + 1;
        assert_eq!(parse_present_req(&f2), None);
        // 截断每切点。
        for cut in 0..VOTE_REQ_LEN {
            assert_eq!(parse_present_req(&buf[..cut]), None);
        }
    }

    #[test]
    #[allow(deprecated)] // 源域 = 退役 SCENE 词表（折算面本体——停发 ≠ 禁读）
    fn scene_to_intent_folds_full_domain() {
        // 交互谱系四值 → INTERACTIVE(0x02)。
        for s in [SCENE_NORMAL, SCENE_SCROLL, SCENE_DRAG, SCENE_ANIMATED] {
            assert_eq!(scene_to_intent(s), Some(Intent::Interactive as u8), "scene={}", s);
        }
        // IDLE → BACKGROUND(0x04)。
        assert_eq!(scene_to_intent(SCENE_IDLE), Some(Intent::Background as u8));
        // 未知值 None loud：0x05..=0xFF 全拒（0x05 = INTENT_MAINTENANCE
        // 非 SCENE 源值；0x06.. = intent 保留段；高位段同拒）。
        for v in Intent::Maintenance as u8..=u8::MAX {
            assert_eq!(scene_to_intent(v), None, "scene={}", v);
        }
        // 折算产物全部落 wire 合法域（scene_known）——发送面闭环。
        for s in [SCENE_NORMAL, SCENE_SCROLL, SCENE_DRAG, SCENE_ANIMATED, SCENE_IDLE] {
            assert!(scene_known(scene_to_intent(s).unwrap()), "scene={}", s);
        }
        // 判别值与 sched/intent.rs 冻结面对账（镜像链锚—— berry-manifest
        // 侧 intent_discriminants_pinned 的对偶断言面）。
        assert_eq!(Intent::Unspecified as u8, 0x00);
        assert_eq!(Intent::LatencyCritical as u8, 0x01);
        assert_eq!(Intent::Interactive as u8, 0x02);
        assert_eq!(Intent::Utility as u8, 0x03);
        assert_eq!(Intent::Background as u8, 0x04);
        assert_eq!(Intent::Maintenance as u8, 0x05);
    }

    #[test]
    fn power_mode_roundtrip_and_domain() {
        let mut buf = [0u8; 4];
        for m in [POWER_NORMAL, POWER_DIM, POWER_AOD, POWER_OFF] {
            let r = PresentReq::PowerMode { mode: m };
            let n = encode_present_req(&mut buf, &r).unwrap();
            assert_eq!(n, 4);
            assert_eq!(parse_present_req(&buf[..n]), Some(r));
        }
        // 值域拒绝：mode 4（编码 + 解析对称）。
        assert_eq!(
            encode_present_req(&mut buf, &PresentReq::PowerMode { mode: POWER_OFF + 1 }),
            None
        );
        assert_eq!(parse_present_req(&[OP_POWER_MODE, 4, 0, 0]), None);
        // 尾随 / 截断。
        assert_eq!(parse_present_req(&[OP_POWER_MODE, 0, 0, 0, 0]), None);
        assert_eq!(parse_present_req(&[OP_POWER_MODE, 0]), None);
    }

    #[test]
    fn status_rep_domain_and_baking_line() {
        let mut out = [0u8; 1];
        for st in [ST_OK, ST_BAD, ST_EAGAIN, ST_BUSY, ST_NOENT, ST_FULL, ST_UNSUPPORTED] {
            assert_eq!(encode_status_rep(&mut out, st), 1);
            assert_eq!(parse_status_rep(&out), Some(st));
        }
        // 烘焙线：一切错误应答恒 1B、不携数据——多字节非 OK 形在
        // input_events_rep 面单独钉板（见 three_state 测试）。
        assert_eq!(parse_status_rep(&[ST_UNSUPPORTED + 1]), None);
        assert_eq!(parse_status_rep(&[ST_OK, 0]), None);
        assert_eq!(parse_status_rep(&[]), None);
    }

    #[test]
    #[should_panic(expected = "guiproto")]
    fn encode_status_rep_rejects_unknown_st() {
        let mut out = [0u8; 1];
        encode_status_rep(&mut out, ST_UNSUPPORTED + 1);
    }

    #[test]
    fn submit_ack_roundtrip_and_rejects() {
        let mut buf = [0u8; 8];
        let n = encode_submit_ack(&mut buf, 0xDEAD_BEEF);
        assert_eq!(n, 8);
        assert_eq!(parse_submit_ack(&buf[..n]), Some(0xDEAD_BEEF));
        assert_eq!(parse_submit_ack(&buf[..7]), None);
        let mut t = [0u8; 9];
        t[..8].copy_from_slice(&buf);
        assert_eq!(parse_submit_ack(&t), None);
        let mut bad = buf;
        bad[0] = ST_BAD; // 8B 非 OK 形不存在（错误恒 1B）。
        assert_eq!(parse_submit_ack(&bad), None);
        assert_eq!(parse_submit_ack(&[ST_FULL]), None); // 1B 错误形走 status 解析器
    }

    // ===== 3. PointerEvent / input 域 =====

    #[test]
    fn pointer_event_len_domain() {
        assert_eq!(pointer_event_len(0), Some(32));
        assert_eq!(pointer_event_len(INPUT_SAMPLES_MAX - 1), Some(INPUT_EVT_MAX_LEN));
        assert_eq!(pointer_event_len(INPUT_SAMPLES_MAX), None);
        assert_eq!(pointer_event_len(9), None);
    }

    #[test]
    fn pointer_event_roundtrip_all_sample_counts() {
        for hl in 0..INPUT_SAMPLES_MAX {
            let ev = sample_ev(hl, 99);
            let mut buf = [0u8; INPUT_EVT_MAX_LEN];
            let n = encode_pointer_event(&mut buf, &ev).unwrap();
            assert_eq!(n, 32 + hl * 16);
            assert_eq!(pointer_event_len(hl), Some(n));
            assert_eq!(parse_pointer_event(&buf[..n]), Some(ev));
        }
        // 全 phase 值域往返。
        let mut buf = [0u8; INPUT_EVT_MAX_LEN];
        for p in [
            PHASE_DOWN,
            PHASE_MOVE,
            PHASE_UP,
            PHASE_CANCEL,
            PHASE_HOVER_ENTER,
            PHASE_HOVER_MOVE,
            PHASE_HOVER_EXIT,
        ] {
            let mut ev = sample_ev(0, 1);
            ev.phase = p;
            let n = encode_pointer_event(&mut buf, &ev).unwrap();
            assert_eq!(parse_pointer_event(&buf[..n]), Some(ev));
        }
        // 按键位域 + flags 位域全组合。
        let mut ev = sample_ev(1, 2);
        ev.buttons = BUTTON_L | BUTTON_R | BUTTON_M;
        ev.flags = PE_FL_RESAMPLED | PE_FL_SYNTHESIZED;
        let n = encode_pointer_event(&mut buf, &ev).unwrap();
        assert_eq!(parse_pointer_event(&buf[..n]), Some(ev));
        // 压力哨兵 + 值域边界（0 / 1000 毫单位）往返（消费侧折叠语义不入
        // wire；越域拒绝面见 reject 矩阵）。
        let mut ev2 = sample_ev(0, 3);
        ev2.pressure = PRESSURE_UNAVAILABLE;
        let n = encode_pointer_event(&mut buf, &ev2).unwrap();
        assert_eq!(parse_pointer_event(&buf[..n]), Some(ev2));
        for pr in [0u16, 1000] {
            let mut evp = sample_ev(0, 6);
            evp.pressure = pr;
            let n = encode_pointer_event(&mut buf, &evp).unwrap();
            assert_eq!(parse_pointer_event(&buf[..n]), Some(evp));
        }
        // 编码拒绝面：history_len 越帽 / out 不足。
        let mut ev3 = sample_ev(0, 4);
        ev3.history_len = INPUT_SAMPLES_MAX;
        assert_eq!(encode_pointer_event(&mut buf, &ev3), None);
        let ev4 = sample_ev(1, 5);
        let mut small = [0u8; 47];
        assert_eq!(encode_pointer_event(&mut small, &ev4), None);
    }

    #[test]
    fn pointer_event_wire_golden_le_pin() {
        let mut ev = sample_ev(1, 0);
        ev.phase = PHASE_MOVE;
        ev.device_id = 0;
        ev.buttons = BUTTON_L;
        ev.flags = PE_FL_RESAMPLED;
        ev.pointer_id = 0x1234;
        ev.history[0] = Sample {
            timestamp_ns: 99,
            x: 11,
            y: 22,
        };
        let mut buf = [0u8; INPUT_EVT_MAX_LEN];
        let n = encode_pointer_event(&mut buf, &ev).unwrap();
        assert_eq!(n, 48);
        let want: [u8; 48] = [
            2, 0, 1, 1, //
            0x34, 0x12, 0, 0, //
            0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, //
            0x90, 0x01, 0, 0, //
            0x2C, 0x01, 0, 0, //
            0xF4, 0x01, 0, 0, //
            0, 0, //
            2, 0, //
            99, 0, 0, 0, 0, 0, 0, 0, //
            11, 0, 0, 0, //
            22, 0, 0, 0,
        ];
        assert_eq!(&buf[..n], &want[..]);
        // LE 钉板：ts 首字节 = 最低位字节；sample_count = 32+16 编码位。
        assert_eq!(buf[8], 0x08);
        assert_eq!(buf[30], 2);
        // contact_major/minor（off 26/28）恒 0：v1 双侧执法（非 0 拒，见
        // reject 矩阵）——合法帧上两对字节恒 0，对内互换在 v1 接口面不可
        // 区分且无语义；v2 赋义时随 input 版本域升级并以相异值重钉。
    }

    #[test]
    fn pointer_event_reject_matrix() {
        let base = sample_ev(1, 7); // 48B（n=2）
        let mut buf = [0u8; INPUT_EVT_MAX_LEN];
        let n = encode_pointer_event(&mut buf, &base).unwrap();
        assert_eq!(n, 48);
        // 截断每切点全拒。
        for cut in 0..n {
            assert_eq!(parse_pointer_event(&buf[..cut]), None, "截断 @{cut} 应拒");
        }
        // 尾随字节。
        let mut tail = vec![0u8; n + 1];
        tail[..n].copy_from_slice(&buf[..n]);
        assert_eq!(parse_pointer_event(&tail), None);
        // sample_count = 0 / 越帽。
        let mut sc0 = buf;
        sc0[30] = 0;
        assert_eq!(parse_pointer_event(&sc0[..32]), None);
        let mut sc5 = vec![0u8; 32 + 4 * 16];
        sc5[0] = PHASE_MOVE;
        sc5[30] = 5;
        assert_eq!(parse_pointer_event(&sc5), None);
        // 值域拒绝：phase 0/8、未知 flags 位、未知 buttons 位。
        let mut ph0 = buf;
        ph0[0] = 0;
        assert_eq!(parse_pointer_event(&ph0[..n]), None);
        let mut ph8 = buf;
        ph8[0] = 8;
        assert_eq!(parse_pointer_event(&ph8[..n]), None);
        let mut fl = buf;
        fl[3] = 0xFC;
        assert_eq!(parse_pointer_event(&fl[..n]), None);
        let mut bt = buf;
        bt[2] = 0x08;
        assert_eq!(parse_pointer_event(&bt[..n]), None);
        // 值域拒绝（§3.3 双侧执法）：pressure 1001 / 0x7FFF / 0x8001、
        // contact_major/minor 非 0——编码 + 解析对称。
        for bad in [1001u16, 0x7FFF, 0x8001] {
            let mut p = base;
            p.pressure = bad;
            assert_eq!(encode_pointer_event(&mut buf, &p), None, "pressure {bad:#x} 编码应拒");
            let mut f = buf; // buf 始终持合法基准帧（拒绝路径零写入）
            f[24] = (bad & 0xFF) as u8;
            f[25] = (bad >> 8) as u8;
            assert_eq!(parse_pointer_event(&f[..n]), None, "pressure {bad:#x} 解析应拒");
        }
        let mut cm = base;
        cm.contact_major = 7;
        assert_eq!(encode_pointer_event(&mut buf, &cm), None);
        let mut cn = base;
        cn.contact_minor = 9;
        assert_eq!(encode_pointer_event(&mut buf, &cn), None);
        let _ = encode_pointer_event(&mut buf, &base).unwrap();
        let mut fm = buf;
        fm[26] = 7; // contact_major
        assert_eq!(parse_pointer_event(&fm[..n]), None);
        let mut fn_ = buf;
        fn_[28] = 9; // contact_minor
        assert_eq!(parse_pointer_event(&fn_[..n]), None);
        // 空。
        assert_eq!(parse_pointer_event(&[]), None);
    }

    #[test]
    fn input_sub_roundtrip_and_rejects() {
        let mut buf = [0u8; 4];
        for d in [DELIVERY_BATCHED, DELIVERY_IMMEDIATE] {
            let n = encode_input_sub_req(&mut buf, d);
            assert_eq!(n, 4);
            assert_eq!(parse_input_sub_req(&buf[..n]), Some(d));
        }
        // 值域拒绝：delivery 2。
        assert_eq!(parse_input_sub_req(&[OP_INPUT_SUBSCRIBE, 2, 0, 0]), None);
        // 恰长：截断 / 尾随 / 错 op。
        assert_eq!(parse_input_sub_req(&[OP_INPUT_SUBSCRIBE, 0, 0]), None);
        assert_eq!(parse_input_sub_req(&[OP_INPUT_SUBSCRIBE, 0, 0, 0, 0]), None);
        assert_eq!(parse_input_sub_req(&[OP_INPUT_POLL, 0, 0, 0]), None);
    }

    #[test]
    fn input_poll_req_domain() {
        let mut buf = [0u8; 4];
        for m in 1..=INPUT_EVENTS_MAX as u8 {
            let n = encode_input_poll_req(&mut buf, m);
            assert_eq!(n, 4);
            assert_eq!(parse_input_poll_req(&buf[..n]), Some(m));
        }
        // 值域拒绝：0（「拉零个事件」非法）与越帽 4。
        assert_eq!(parse_input_poll_req(&[OP_INPUT_POLL, 0, 0, 0]), None);
        assert_eq!(
            parse_input_poll_req(&[OP_INPUT_POLL, INPUT_EVENTS_MAX as u8 + 1, 0, 0]),
            None
        );
        // 尾随 / 错 op。
        assert_eq!(parse_input_poll_req(&[OP_INPUT_POLL, 1, 0, 0, 0]), None);
        assert_eq!(parse_input_poll_req(&[OP_INPUT_SUBSCRIBE, 1, 0, 0]), None);
    }

    #[test]
    fn input_events_rep_roundtrip_and_accessor() {
        // 混合采样数三事件批（0/1/3 历史）往返 + 逐项访问器。
        let events = [sample_ev(0, 100), sample_ev(1, 200), sample_ev(3, 300)];
        let mut buf = [0u8; INPUT_REP_MAX];
        let n = encode_input_events_rep(&mut buf, &events).unwrap();
        assert_eq!(n, 4 + 32 + 48 + 80);
        let rep = parse_input_events_rep(&buf[..n]).unwrap().ok().unwrap();
        assert_eq!(rep.count(), 3);
        for (i, want) in events.iter().enumerate() {
            assert_eq!(rep.event(i), Some(*want), "事件 {i} 往返漂移");
        }
        assert_eq!(rep.event(3), None); // 越界访问器
        // 空批 / 越帽批编码拒绝（空批走 1B EAGAIN 形）。
        assert_eq!(encode_input_events_rep(&mut buf, &[]), None);
        let four = [sample_ev(0, 1); INPUT_EVENTS_MAX + 1];
        assert_eq!(encode_input_events_rep(&mut buf, &four), None);
        // out 不足拒绝。
        let mut small = vec![0u8; n - 1];
        assert_eq!(encode_input_events_rep(&mut small, &events), None);
        // 野 history_len（公有字段调用方可构造）：总长累加先界后乘
        // （pointer_event_len 单源）→ None 非 panic——debug 构建同样
        // （编码器 None 契约，编码期溢出 = 违约）。
        let mut wild = sample_ev(0, 7);
        wild.history_len = usize::MAX / 16;
        assert_eq!(encode_input_events_rep(&mut buf, &[wild]), None);
    }

    #[test]
    fn input_events_rep_three_state_and_baking_line() {
        // 三态之一：合法 1B 错误应答（非 OK 且在册）→ Err(st)。
        assert_eq!(parse_input_events_rep(&[ST_EAGAIN]), Some(Err(ST_EAGAIN)));
        assert_eq!(parse_input_events_rep(&[ST_BUSY]), Some(Err(ST_BUSY)));
        // 烘焙线：非 OK 携数据 = 畸形（None——「超时了但带数据」线上不存在）。
        assert_eq!(parse_input_events_rep(&[ST_BUSY, 1, 0, 0]), None);
        assert_eq!(parse_input_events_rep(&[ST_FULL, 0xEE]), None);
        // 未知 st / 1B OK 形（成功必携 count ≥ 1）/ 空帧 → None。
        assert_eq!(parse_input_events_rep(&[ST_UNSUPPORTED + 1]), None);
        assert_eq!(parse_input_events_rep(&[ST_OK]), None);
        assert_eq!(parse_input_events_rep(&[]), None);
        // count 0 / 越帽 / 头截断。
        assert_eq!(parse_input_events_rep(&[ST_OK, 0, 0, 0]), None);
        assert_eq!(parse_input_events_rep(&[ST_OK, 4, 0, 0]), None);
        assert_eq!(parse_input_events_rep(&[ST_OK, 1]), None);
        // count=1 但事件流截断。
        assert_eq!(parse_input_events_rep(&[ST_OK, 1, 0, 0, 2]), None);
    }

    #[test]
    fn input_events_rep_truncation_and_cap_boundary() {
        // 单事件帧截断每切点全拒（多事件帧的事件边界切点 = 合法短帧，
        // 故用单事件做全切点扫描）。
        let events = [sample_ev(1, 42)];
        let mut buf = [0u8; INPUT_REP_MAX];
        let n = encode_input_events_rep(&mut buf, &events).unwrap();
        assert_eq!(n, 4 + 48);
        for cut in 0..n {
            assert_eq!(
                parse_input_events_rep(&buf[..cut]),
                None,
                "截断 @{cut} 应拒"
            );
        }
        // 尾随字节。
        let mut tail = vec![0u8; n + 1];
        tail[..n].copy_from_slice(&buf[..n]);
        assert_eq!(parse_input_events_rep(&tail), None);
        // 帽边界恰满：3 事件 × 3 历史 = 4 + 3×80 = INPUT_REP_MAX。
        let full = [sample_ev(3, 1), sample_ev(3, 2), sample_ev(3, 3)];
        let mut big = [0u8; INPUT_REP_MAX];
        let m = encode_input_events_rep(&mut big, &full).unwrap();
        assert_eq!(m, INPUT_REP_MAX);
        let rep = parse_input_events_rep(&big[..m]).unwrap().ok().unwrap();
        assert_eq!(rep.count(), INPUT_EVENTS_MAX);
        assert_eq!(rep.event(2), Some(full[2]));
    }

    // ===== 4. hdi 域 =====

    #[test]
    fn reg_vblank_roundtrip_and_rejects() {
        let mut buf = [0u8; 4];
        for e in [0u8, 1] {
            let n = encode_reg_vblank_req(&mut buf, e);
            assert_eq!(n, 4);
            assert_eq!(parse_reg_vblank_req(&buf[..n]), Some(e));
        }
        // 值域拒绝：enable 2。
        assert_eq!(parse_reg_vblank_req(&[OP_REG_VBLANK, 2, 0, 0]), None);
        // 恰长 / 错 op。
        assert_eq!(parse_reg_vblank_req(&[OP_REG_VBLANK, 1, 0]), None);
        assert_eq!(parse_reg_vblank_req(&[OP_REG_VBLANK, 1, 0, 0, 0]), None);
        assert_eq!(parse_reg_vblank_req(&[OP_VBLANK_WAIT, 1, 0, 0]), None);
    }

    #[test]
    fn vblank_wait_req_roundtrip_and_rejects() {
        // 无参请求：roundtrip + 黄金字节钉板。
        let mut buf = [0u8; 4];
        let n = encode_vblank_wait_req(&mut buf);
        assert_eq!(n, 4);
        assert_eq!(&buf[..n], &[OP_VBLANK_WAIT, 0, 0, 0][..]);
        assert_eq!(parse_vblank_wait_req(&buf[..n]), Some(()));
        // 保留位 r/r16 解析侧恒不校验（前向兼容锚——GETINFO 无参同款）。
        let mut wild = buf;
        wild[1] = 0xEE;
        wild[2] = 0xEE;
        wild[3] = 0xEE;
        assert_eq!(parse_vblank_wait_req(&wild), Some(()));
        // 截断每切点 / 长尾 / 错 op。
        for cut in 0..4 {
            assert_eq!(parse_vblank_wait_req(&buf[..cut]), None, "截断 @{cut} 应拒");
        }
        assert_eq!(parse_vblank_wait_req(&[OP_VBLANK_WAIT, 0, 0, 0, 0]), None);
        assert_eq!(parse_vblank_wait_req(&[OP_REG_VBLANK, 0, 0, 0]), None);
    }

    #[test]
    fn vblank_rep_roundtrip_golden_and_rejects() {
        let rep = VblankRep {
            seq: 0xDEAD_BEEF,
            ts_ns: 12345,
        };
        let mut buf = [0u8; VBLANK_REP_LEN];
        let n = encode_vblank_rep(&mut buf, &rep);
        assert_eq!(n, 16);
        // 黄金字节钉板。
        let want: [u8; 16] = [
            0, 0, 0, 0, //
            0xEF, 0xBE, 0xAD, 0xDE, //
            0x39, 0x30, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(&buf[..n], &want[..]);
        assert_eq!(parse_vblank_rep(&buf[..n]), Some(rep));
        // 顶格值往返。
        let top = VblankRep {
            seq: u32::MAX,
            ts_ns: u64::MAX,
        };
        encode_vblank_rep(&mut buf, &top);
        assert_eq!(parse_vblank_rep(&buf), Some(top));
        // 截断每切点 / 尾随 / st 坏。
        encode_vblank_rep(&mut buf, &rep);
        for cut in 0..VBLANK_REP_LEN {
            assert_eq!(parse_vblank_rep(&buf[..cut]), None, "截断 @{cut} 应拒");
        }
        let mut tail = [0u8; 17];
        tail[..16].copy_from_slice(&buf);
        assert_eq!(parse_vblank_rep(&tail), None);
        let mut bad = buf;
        bad[0] = ST_BUSY;
        assert_eq!(parse_vblank_rep(&bad), None);
    }

    #[test]
    fn prepare_req_roundtrip_golden_and_rejects() {
        let req = PrepareReq {
            flags: 0,
            width: 800,
            height: 600,
            stride: 3200,
            format: FMT_XRGB8888,
        };
        let mut buf = [0u8; PREPARE_REQ_LEN];
        let n = encode_prepare_req(&mut buf, &req);
        assert_eq!(n, 20);
        // 黄金字节钉板。
        let want: [u8; 20] = [
            0x42, 0, 0, 0, //
            0x20, 0x03, 0, 0, //
            0x58, 0x02, 0, 0, //
            0x80, 0x0C, 0, 0, //
            1, 0, 0, 0,
        ];
        assert_eq!(&buf[..n], &want[..]);
        assert_eq!(parse_prepare_req(&buf[..n]), Some(req));
        // format 任意 u32 合法（不在协商集 → 服务端 ST_UNSUPPORTED 拒面）。
        let mut odd = req;
        odd.format = 0xFFFF_FFFF;
        let n = encode_prepare_req(&mut buf, &odd);
        assert_eq!(parse_prepare_req(&buf[..n]), Some(odd));
        // flags ≠ 0 拒（v1 恒 0，双侧执法）——预留位 SERVER_SAMPLABLE
        // （bit0）同面：冻结语义=0 不可采样，置位帧解析 None（编码 assert
        // 面见 should_panic 测试；Veda 启示八预留，激活 = 值域收宽）。
        let mut b2 = [0u8; PREPARE_REQ_LEN];
        encode_prepare_req(&mut b2, &req);
        b2[1] = PREPARE_F_SERVER_SAMPLABLE;
        assert_eq!(parse_prepare_req(&b2), None);
        // 预留位钉板：认领 bit0；flags=0 默认形上面已往返 = 新位默认 0
        // 不可采样（与帧率意图位同批的伙伴位，GUI-2 消费侧已实施）。
        assert_eq!(PREPARE_F_SERVER_SAMPLABLE, 1 << 0);
        assert_eq!(req.flags & PREPARE_F_SERVER_SAMPLABLE, 0);
        // 截断每切点 / 尾随 / 错 op。
        encode_prepare_req(&mut buf, &req);
        for cut in 0..PREPARE_REQ_LEN {
            assert_eq!(parse_prepare_req(&buf[..cut]), None, "截断 @{cut} 应拒");
        }
        let mut tail = [0u8; 21];
        tail[..20].copy_from_slice(&buf);
        assert_eq!(parse_prepare_req(&tail), None);
        assert_eq!(parse_prepare_req(&[OP_COMMIT, 0, 0, 0]), None);
    }

    #[test]
    #[should_panic(expected = "guiproto")]
    fn prepare_encode_rejects_server_samplable_frozen_zero() {
        // 预留位 v1 冻结 0：置位编码 = 调用方程序员错，assert 炸响——
        // 双侧执法的编码侧一半（解析侧 None 面在 roundtrip 测试钉板）。
        let req = PrepareReq {
            flags: PREPARE_F_SERVER_SAMPLABLE,
            width: 800,
            height: 600,
            stride: 3200,
            format: FMT_XRGB8888,
        };
        let mut buf = [0u8; PREPARE_REQ_LEN];
        encode_prepare_req(&mut buf, &req);
    }

    #[test]
    fn prepare_rep_domain() {
        let mut buf = [0u8; 4];
        for slot in 0..HDI_SLOTS_MAX {
            let n = encode_prepare_rep(&mut buf, slot);
            assert_eq!(n, 4);
            assert_eq!(parse_prepare_rep(&buf[..n]), Some(slot));
        }
        // slot 越帽拒（解析面；编码面 assert 同契约）。
        assert_eq!(parse_prepare_rep(&[ST_OK, HDI_SLOTS_MAX, 0, 0]), None);
        assert_eq!(parse_prepare_rep(&[ST_OK, 255, 0, 0]), None);
        // st 坏 / 尾随。
        assert_eq!(parse_prepare_rep(&[ST_NOENT, 0, 0, 0]), None);
        assert_eq!(parse_prepare_rep(&[ST_OK, 0, 0, 0, 0]), None);
    }

    #[test]
    fn commit_req_roundtrip_and_rejects() {
        let mut buf = [0u8; COMMIT_REQ_LEN];
        let n = encode_commit_req(&mut buf, 0, 0);
        assert_eq!(n, 12);
        assert_eq!(parse_commit_req(&buf[..n]), Some((0, 0)));
        let n = encode_commit_req(&mut buf, HDI_SLOTS_MAX - 1, u64::MAX);
        assert_eq!(parse_commit_req(&buf[..n]), Some((HDI_SLOTS_MAX - 1, u64::MAX)));
        // slot 越帽拒。
        assert_eq!(parse_commit_req(&[OP_COMMIT, HDI_SLOTS_MAX, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]), None);
        // 截断每切点 / 尾随 / 错 op。
        encode_commit_req(&mut buf, 1, 12345);
        for cut in 0..COMMIT_REQ_LEN {
            assert_eq!(parse_commit_req(&buf[..cut]), None, "截断 @{cut} 应拒");
        }
        let mut tail = [0u8; 13];
        tail[..12].copy_from_slice(&buf);
        assert_eq!(parse_commit_req(&tail), None);
        assert_eq!(parse_commit_req(&[OP_PREPARE, 0, 0, 0]), None);
    }

    // ===== 5. 前向兼容锚：保留位解析侧恒不校验 =====

    #[test]
    fn reserved_fields_forward_compat_anchors() {
        // 头 a/b/r（GETINFO 无参 → 全保留）：塞 0xEE 仍解析。
        assert_eq!(
            parse_present_req(&[OP_GETINFO, 0xEE, 0xEE, 0xEE]),
            Some(PresentReq::Getinfo)
        );
        // SUBMIT 头 r（off 1）与 r16（off 14/15）。
        let mut buf = [0u8; PRESENT_REQ_MAX];
        let req = submit_req(1, 0);
        let n = encode_present_req(&mut buf, &req).unwrap();
        buf[1] = 0xEE;
        buf[14] = 0xEE;
        buf[15] = 0xEE;
        assert_eq!(parse_present_req(&buf[..n]), Some(req));
        // PointerEvent rsv（off 6/7、31）。
        let ev = sample_ev(1, 9);
        let mut b2 = [0u8; INPUT_EVT_MAX_LEN];
        let m = encode_pointer_event(&mut b2, &ev).unwrap();
        b2[6] = 0xEE;
        b2[7] = 0xEE;
        b2[31] = 0xEE;
        assert_eq!(parse_pointer_event(&b2[..m]), Some(ev));
        // GETINFO 应答保留区（off 4..8、21、22..24）。
        let rep = GetinfoRep {
            version_present: 1,
            version_input: 1,
            version_hdi: 1,
            format_bits: FMT_XRGB8888,
            display_w: 800,
            display_h: 600,
            max_credits: 2,
        };
        let mut b3 = [0u8; PRESENT_REP_MAX];
        encode_getinfo_rep(&mut b3, &rep);
        for i in 4..8 {
            b3[i] = 0xFF;
        }
        b3[21] = 0xFF;
        b3[22] = 0xFF;
        b3[23] = 0xFF;
        assert_eq!(parse_getinfo_rep(&b3), Some(rep));
        // FRAME_CALLBACK 应答 r（off 3）。
        let fr = FrameDoneRep {
            status: FRAME_PRESENTED,
            credits: 1,
            present_seq: 5,
            presented_ns: 6,
        };
        let mut b4 = [0u8; 16];
        encode_frame_done_rep(&mut b4, &fr);
        b4[3] = 0xEE;
        assert_eq!(parse_frame_done_rep(&b4), Some(fr));
    }

    #[test]
    fn submit_ack_wire_golden() {
        // SUBMIT 应答黄金字节（8B 定长）。
        let mut buf = [0u8; 8];
        let n = encode_submit_ack(&mut buf, 0x0102_0304);
        assert_eq!(n, 8);
        assert_eq!(&buf[..n], &[0, 0, 0, 0, 0x04, 0x03, 0x02, 0x01][..]);
    }

    // ===== 6. wm 域（GUI-5——13 条 wire 义务 + wm_hit_test 五组）=====

    /// 测试样本快照（count 项，每项字段相异值随 i 递变防互换静绿——版本
    /// 三元组 (1,2,3) 先例同纪律）。
    fn wm_snap_sample(count: usize) -> WmSnapshot {
        let mut entries = [WmEntry {
            id: 0,
            parent: 0,
            group: 0,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            layer: 0,
            z: 0,
            flags: 0,
        }; WM_WINDOWS_MAX];
        for (i, e) in entries.iter_mut().enumerate().take(count) {
            *e = WmEntry {
                id: 0x10 + i as u16,
                parent: 0x20 + i as u16,
                group: i as u8,
                x: (100 + i) as u16,
                y: (200 + i) as u16,
                w: (50 + i) as u16,
                h: (60 + i) as u16,
                layer: WM_LAYER_APP,
                z: i as u8,
                flags: WM_WIN_F_VISIBLE,
            };
        }
        WmSnapshot {
            version: 0x0A00_0001,
            disp_w: 800,
            disp_h: 600,
            entries,
            count,
        }
    }

    #[test]
    fn wm_all_ops_roundtrip() {
        let mut buf = [0u8; WM_REP_MAX];
        // GETINFO req / rep。
        let n = encode_wm_getinfo_req(&mut buf);
        assert_eq!(n, 4);
        assert_eq!(parse_wm_getinfo_req(&buf[..n]), Some(()));
        let gi = WmGetinfoRep {
            version_wm: VER_WM,
            windows_max: 15,
            disp_w: 800,
            disp_h: 600,
        };
        let n = encode_wm_getinfo_rep(&mut buf, &gi);
        assert_eq!(n, 12);
        assert_eq!(parse_wm_getinfo_rep(&buf[..n]), Some(gi));
        // CREATE req（相异值）/ ack。
        let cr = WmCreateReq {
            layer: WM_LAYER_SYSTEM,
            flags: WM_CREATE_F_AVOID_SYS,
            parent: 3,
            group: 4,
            x: 5,
            y: 6,
            w: 7,
            h: 8,
        };
        let n = encode_wm_create_req(&mut buf, &cr).unwrap();
        assert_eq!(n, 16);
        assert_eq!(parse_wm_create_req(&buf[..n]), Some(cr));
        let n = encode_wm_create_ack(&mut buf, 0x1234);
        assert_eq!(n, 8);
        assert_eq!(parse_wm_create_ack(&buf[..n]), Some(0x1234));
        // DESTROY req。
        let n = encode_wm_destroy_req(&mut buf, 0x5678);
        assert_eq!(n, 8);
        assert_eq!(parse_wm_destroy_req(&buf[..n]), Some(0x5678));
        // SET req：全 mask 相异值 + masked-out 槽照编照收（仅 Z 位——几何/
        // vis 槽携带值无语义原样往返，禁偷清零）。
        let sr = WmSetReq {
            mask: WM_SET_F_GEOM | WM_SET_F_VIS | WM_SET_F_Z,
            window_id: 9,
            x: 10,
            y: 11,
            w: 12,
            h: 13,
            vis: WM_VIS_SHOW,
            z: 14,
        };
        let n = encode_wm_set_req(&mut buf, &sr).unwrap();
        assert_eq!(n, 20);
        assert_eq!(parse_wm_set_req(&buf[..n]), Some(sr));
        let partial = WmSetReq {
            mask: WM_SET_F_Z,
            window_id: 2,
            x: 0xFFFF,
            y: 0xFFFF,
            w: 0xFFFF,
            h: 0xFFFF,
            vis: WM_VIS_SHOW,
            z: 0xAB,
        };
        let n = encode_wm_set_req(&mut buf, &partial).unwrap();
        assert_eq!(parse_wm_set_req(&buf[..n]), Some(partial));
        // TABLE_GET req / TABLE_WAIT req（0 与顶格 last_seen——纪元回退角
        // 由消费侧兜底，wire 层任意 u32 合法）。
        let n = encode_wm_table_get_req(&mut buf);
        assert_eq!(n, 4);
        assert_eq!(parse_wm_table_get_req(&buf[..n]), Some(()));
        for ls in [0u32, 0x0A00_0001, u32::MAX] {
            let n = encode_wm_table_wait_req(&mut buf, ls);
            assert_eq!(n, 8);
            assert_eq!(parse_wm_table_wait_req(&buf[..n]), Some(ls));
        }
        // 快照应答（TABLE_GET/TABLE_WAIT 共用）。
        let snap = wm_snap_sample(2);
        let n = encode_wm_snapshot_rep(&mut buf, &snap).unwrap();
        assert_eq!(n, 12 + 2 * 16);
        assert_eq!(parse_wm_snapshot_rep(&buf[..n]).unwrap().ok().unwrap(), snap);
        // DESTROY/SET 成功应答 = 1B status 面（第十组，复用）。
        let n = encode_status_rep(&mut buf, ST_OK);
        assert_eq!(parse_status_rep(&buf[..n]), Some(ST_OK));
    }

    #[test]
    fn wm_wire_golden() {
        // 黄金字节钉板（改布局 = 此测必炸——C-2a 纪律）。全帧 LE。
        let mut buf = [0u8; WM_REP_MAX];
        // GETINFO rep 12B。
        let gi = WmGetinfoRep {
            version_wm: 1,
            windows_max: 15,
            disp_w: 800,
            disp_h: 600,
        };
        let n = encode_wm_getinfo_rep(&mut buf, &gi);
        assert_eq!(n, 12);
        assert_eq!(
            &buf[..n],
            &[0, 1, 15, 0, 0x20, 0x03, 0x58, 0x02, 0, 0, 0, 0][..]
        );
        // GETINFO req 4B。
        let n = encode_wm_getinfo_req(&mut buf);
        assert_eq!(&buf[..n], &[0x60, 0, 0, 0][..]);
        // CREATE req 16B。
        let cr = WmCreateReq {
            layer: 1,
            flags: 1,
            parent: 3,
            group: 4,
            x: 5,
            y: 6,
            w: 7,
            h: 8,
        };
        let n = encode_wm_create_req(&mut buf, &cr).unwrap();
        assert_eq!(n, 16);
        assert_eq!(
            &buf[..n],
            &[0x61, 1, 1, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0, 8, 0][..]
        );
        // CREATE ack 8B。
        let n = encode_wm_create_ack(&mut buf, 0x1234);
        assert_eq!(&buf[..n], &[0, 0, 0, 0, 0x34, 0x12, 0, 0][..]);
        // DESTROY req 8B。
        let n = encode_wm_destroy_req(&mut buf, 0x5678);
        assert_eq!(&buf[..n], &[0x62, 0, 0, 0, 0x78, 0x56, 0, 0][..]);
        // SET req 20B。
        let sr = WmSetReq {
            mask: 7,
            window_id: 9,
            x: 10,
            y: 11,
            w: 12,
            h: 13,
            vis: 1,
            z: 14,
        };
        let n = encode_wm_set_req(&mut buf, &sr).unwrap();
        assert_eq!(n, 20);
        assert_eq!(
            &buf[..n],
            &[0x63, 7, 0, 0, 9, 0, 0, 0, 10, 0, 11, 0, 12, 0, 13, 0, 1, 14, 0, 0][..]
        );
        // TABLE_GET req 4B / TABLE_WAIT req 8B。
        let n = encode_wm_table_get_req(&mut buf);
        assert_eq!(&buf[..n], &[0x64, 0, 0, 0][..]);
        let n = encode_wm_table_wait_req(&mut buf, 0x0102_0304);
        assert_eq!(&buf[..n], &[0x65, 0, 0, 0, 0x04, 0x03, 0x02, 0x01][..]);
        // LE 钉板：TABLE_WAIT last_seen 首字节 = 最低位字节。
        assert_eq!(buf[4], 0x04);
        // 快照 1 项 = 12B 头 + 16B 表项。
        let snap = wm_snap_sample(1);
        let n = encode_wm_snapshot_rep(&mut buf, &snap).unwrap();
        assert_eq!(n, 28);
        assert_eq!(
            &buf[..n],
            &[
                0, 1, 0, 0, // 头：st/count/r16
                0x01, 0, 0, 0x0A, // version 0x0A000001 LE
                0x20, 0x03, 0x58, 0x02, // disp_w/disp_h
                0x10, 0, 0x20, 0, 0, // 表项：id/parent/group
                0x64, 0, 0xC8, 0, 0x32, 0, 0x3C, 0, // x/y/w/h
                0, 0, 1, // layer/z/flags
            ][..]
        );
        // LE 钉板：version 首字节 = 最低位字节（counter 段）。
        assert_eq!(buf[4], 0x01);
        assert_eq!(buf[7], 0x0A);
    }

    #[test]
    fn wm_truncation_and_trailing_rejects() {
        // 定长帧截断每切点全拒 + 尾随拒（count 在头内——快照短前缀不构成
        // 合法小帧）。
        let mut buf = [0u8; WM_REP_MAX];
        let mut frames: Vec<Vec<u8>> = Vec::new();
        let n = encode_wm_getinfo_req(&mut buf);
        frames.push(buf[..n].to_vec());
        let n = encode_wm_getinfo_rep(
            &mut buf,
            &WmGetinfoRep { version_wm: 1, windows_max: 15, disp_w: 800, disp_h: 600 },
        );
        frames.push(buf[..n].to_vec());
        let n = encode_wm_create_req(
            &mut buf,
            &WmCreateReq {
                layer: WM_LAYER_APP,
                flags: 0,
                parent: 0,
                group: 0,
                x: 1,
                y: 2,
                w: 3,
                h: 4,
            },
        )
        .unwrap();
        frames.push(buf[..n].to_vec());
        let n = encode_wm_create_ack(&mut buf, 1);
        frames.push(buf[..n].to_vec());
        let n = encode_wm_destroy_req(&mut buf, 1);
        frames.push(buf[..n].to_vec());
        let n = encode_wm_set_req(
            &mut buf,
            &WmSetReq {
                mask: WM_SET_F_GEOM,
                window_id: 1,
                x: 1,
                y: 2,
                w: 3,
                h: 4,
                vis: WM_VIS_HIDE,
                z: 0,
            },
        )
        .unwrap();
        frames.push(buf[..n].to_vec());
        let n = encode_wm_table_get_req(&mut buf);
        frames.push(buf[..n].to_vec());
        let n = encode_wm_table_wait_req(&mut buf, 7);
        frames.push(buf[..n].to_vec());
        let n = encode_wm_snapshot_rep(&mut buf, &wm_snap_sample(1)).unwrap();
        frames.push(buf[..n].to_vec());

        // 帧序号分派解析器（九帧型 → 统一 Option<()> 拒面）。
        fn dispatch(fi: usize, b: &[u8]) -> Option<()> {
            match fi {
                0 => parse_wm_getinfo_req(b).map(|_| ()),
                1 => parse_wm_getinfo_rep(b).map(|_| ()),
                2 => parse_wm_create_req(b).map(|_| ()),
                3 => parse_wm_create_ack(b).map(|_| ()),
                4 => parse_wm_destroy_req(b).map(|_| ()),
                5 => parse_wm_set_req(b).map(|_| ()),
                6 => parse_wm_table_get_req(b).map(|_| ()),
                7 => parse_wm_table_wait_req(b).map(|_| ()),
                _ => match parse_wm_snapshot_rep(b) {
                    Some(Ok(_)) | Some(Err(_)) => Some(()),
                    None => None,
                },
            }
        }
        for (fi, frame) in frames.iter().enumerate() {
            for cut in 0..frame.len() {
                assert_eq!(dispatch(fi, &frame[..cut]), None, "帧 {fi} 截断 @{cut} 应拒");
            }
            let mut tail = frame.clone();
            tail.push(0xEE);
            assert_eq!(dispatch(fi, &tail), None, "帧 {fi} 尾随应拒");
        }
    }

    #[test]
    fn wm_value_domain_reject_matrix() {
        // 值域拒矩阵（编码 + 解析对称；相异野值防互换静绿）。
        let mut buf = [0u8; WM_REP_MAX];
        // CREATE：layer 野值 2 / flags 未知位 0x02。
        let good_create = WmCreateReq {
            layer: WM_LAYER_APP,
            flags: 0,
            parent: 3,
            group: 4,
            x: 5,
            y: 6,
            w: 7,
            h: 8,
        };
        let bad_layer = WmCreateReq { layer: 2, ..good_create };
        assert_eq!(encode_wm_create_req(&mut buf, &bad_layer), None);
        let mut f = [0u8; WM_CREATE_REQ_LEN];
        let _ = encode_wm_create_req(&mut f, &good_create).unwrap();
        f[1] = 2;
        assert_eq!(parse_wm_create_req(&f), None);
        let bad_flags = WmCreateReq { flags: WM_CREATE_F_AVOID_SYS | 0x02, ..good_create };
        assert_eq!(encode_wm_create_req(&mut buf, &bad_flags), None);
        let mut f = [0u8; WM_CREATE_REQ_LEN];
        let _ = encode_wm_create_req(&mut f, &good_create).unwrap();
        f[2] = 0x02;
        assert_eq!(parse_wm_create_req(&f), None);
        // SET：mask=0 / mask 未知位 0x08 / vis 野值 2（vis 无条件执法——
        // masked-out 也拒，Q18）。
        let good = WmSetReq {
            mask: WM_SET_F_GEOM,
            window_id: 1,
            x: 2,
            y: 3,
            w: 4,
            h: 5,
            vis: WM_VIS_HIDE,
            z: 6,
        };
        let n = encode_wm_set_req(&mut buf, &good).unwrap();
        let patches: [(usize, u8, &str); 3] = [
            (1, 0, "mask=0"),
            (1, 0x08, "mask 未知位"),
            (16, 2, "vis 野值"),
        ];
        for (off, val, why) in patches {
            let mut f = buf;
            f[off] = val;
            assert_eq!(parse_wm_set_req(&f[..n]), None, "{why} 解析应拒");
        }
        assert_eq!(encode_wm_set_req(&mut buf, &WmSetReq { mask: 0, ..good }), None);
        assert_eq!(
            encode_wm_set_req(&mut buf, &WmSetReq { mask: 0x08 | WM_SET_F_Z, ..good }),
            None
        );
        assert_eq!(encode_wm_set_req(&mut buf, &WmSetReq { vis: 2, ..good }), None);
        // 快照表项：layer 野值 2（off 头+13）/ flags 未知位 0x04（off 头+15）。
        let n = encode_wm_snapshot_rep(&mut buf, &wm_snap_sample(1)).unwrap();
        let mut f = buf;
        f[WM_SNAP_HEAD + 13] = 2;
        assert_eq!(parse_wm_snapshot_rep(&f[..n]), None);
        let mut f = buf;
        f[WM_SNAP_HEAD + 15] = 0x04;
        assert_eq!(parse_wm_snapshot_rep(&f[..n]), None);
        let mut bad_snap = wm_snap_sample(1);
        bad_snap.entries[0].flags = WM_WIN_F_FOCUSED | 0x04;
        assert_eq!(encode_wm_snapshot_rep(&mut buf, &bad_snap), None);
        bad_snap.entries[0].layer = 2;
        assert_eq!(encode_wm_snapshot_rep(&mut buf, &bad_snap), None);
        // GETINFO rep：windows_max 越帽 16（解析面；编码面 assert 见
        // should_panic 测试）。
        let n = encode_wm_getinfo_rep(
            &mut buf,
            &WmGetinfoRep { version_wm: 1, windows_max: 15, disp_w: 800, disp_h: 600 },
        );
        let mut f = buf;
        f[2] = 16;
        assert_eq!(parse_wm_getinfo_rep(&f[..n]), None);
    }

    #[test]
    fn wm_cap_boundary_and_empty_table() {
        // 帽边界恰满：15 项 = 12 + 15×16 = WM_REP_MAX 顶格。
        let full = wm_snap_sample(WM_WINDOWS_MAX);
        let mut buf = [0u8; WM_REP_MAX];
        let n = encode_wm_snapshot_rep(&mut buf, &full).unwrap();
        assert_eq!(n, WM_REP_MAX);
        let got = parse_wm_snapshot_rep(&buf[..n]).unwrap().ok().unwrap();
        assert_eq!(got.count, WM_WINDOWS_MAX);
        assert_eq!(got.entries[WM_WINDOWS_MAX - 1], full.entries[WM_WINDOWS_MAX - 1]);
        // 越帽：16 项编码 None；count=16 解析 None（合成头）。
        let mut over = wm_snap_sample(WM_WINDOWS_MAX);
        over.count = WM_WINDOWS_MAX + 1;
        assert_eq!(encode_wm_snapshot_rep(&mut buf, &over), None);
        let mut f = buf;
        f[1] = WM_WINDOWS_MAX as u8 + 1;
        assert_eq!(parse_wm_snapshot_rep(&f[..n]), None);
        // count=0 = 合法空表钉（12B 头恰帧——区别于 INPUT_POLL count 0 拒：
        // 空批有 1B EAGAIN 替代形，窗口表空 = 真实初始态，Q15）。
        let empty = wm_snap_sample(0);
        let n = encode_wm_snapshot_rep(&mut buf, &empty).unwrap();
        assert_eq!(n, 12);
        let got = parse_wm_snapshot_rep(&buf[..n]).unwrap().ok().unwrap();
        assert_eq!(got.count, 0);
        assert_eq!(got, empty);
    }

    #[test]
    fn wm_snapshot_three_state_and_baking_line() {
        // 三态之一：合法 1B 错误应答（wmd 未上线/降级面）→ Err(st)。
        assert_eq!(parse_wm_snapshot_rep(&[ST_EAGAIN]), Some(Err(ST_EAGAIN)));
        assert_eq!(parse_wm_snapshot_rep(&[ST_BUSY]), Some(Err(ST_BUSY)));
        // 烘焙线：非 OK 携数据 = 畸形（None）。
        assert_eq!(parse_wm_snapshot_rep(&[ST_BAD, 0, 0, 0]), None);
        assert_eq!(parse_wm_snapshot_rep(&[ST_FULL, 0xEE]), None);
        // 未知 st / 1B OK 形（成功必携 12B 头）/ 空帧 → None。
        assert_eq!(parse_wm_snapshot_rep(&[ST_UNSUPPORTED + 1]), None);
        assert_eq!(parse_wm_snapshot_rep(&[ST_OK]), None);
        assert_eq!(parse_wm_snapshot_rep(&[]), None);
        // 头截断（2..12B OK 形非法）/ count=1 表项截断。
        assert_eq!(parse_wm_snapshot_rep(&[ST_OK, 0]), None);
        assert_eq!(parse_wm_snapshot_rep(&[ST_OK, 0, 0, 0]), None);
        assert_eq!(parse_wm_snapshot_rep(&[ST_OK, 1, 0, 0, 1]), None);
    }

    #[test]
    fn wm_reserved_wild_forward_compat() {
        // 保留位解析侧恒不校验（前向兼容锚——0xEE 塞槽仍解析）。
        let mut buf = [0u8; WM_REP_MAX];
        // GETINFO req r/r16。
        assert_eq!(parse_wm_getinfo_req(&[OP_WM_GETINFO, 0xEE, 0xEE, 0xEE]), Some(()));
        // TABLE_GET req r/r16。
        assert_eq!(parse_wm_table_get_req(&[OP_WM_TABLE_GET, 0xEE, 0xEE, 0xEE]), Some(()));
        // TABLE_WAIT req r/r16（off 1..4）。
        let n = encode_wm_table_wait_req(&mut buf, 5);
        buf[1] = 0xEE;
        buf[2] = 0xEE;
        buf[3] = 0xEE;
        assert_eq!(parse_wm_table_wait_req(&buf[..n]), Some(5));
        // CREATE req r（off 3/7）。
        let cr = WmCreateReq {
            layer: WM_LAYER_APP,
            flags: 0,
            parent: 1,
            group: 2,
            x: 3,
            y: 4,
            w: 5,
            h: 6,
        };
        let n = encode_wm_create_req(&mut buf, &cr).unwrap();
        buf[3] = 0xEE;
        buf[7] = 0xEE;
        assert_eq!(parse_wm_create_req(&buf[..n]), Some(cr));
        // SET req r16（off 2..4/6..8/18..20）。
        let sr = WmSetReq {
            mask: WM_SET_F_GEOM | WM_SET_F_Z,
            window_id: 1,
            x: 2,
            y: 3,
            w: 4,
            h: 5,
            vis: WM_VIS_SHOW,
            z: 6,
        };
        let n = encode_wm_set_req(&mut buf, &sr).unwrap();
        for i in [2usize, 3, 6, 7, 18, 19] {
            buf[i] = 0xEE;
        }
        assert_eq!(parse_wm_set_req(&buf[..n]), Some(sr));
        // GETINFO rep r（off 3）/ r u32（off 8..12）。
        let gi = WmGetinfoRep {
            version_wm: 1,
            windows_max: 15,
            disp_w: 800,
            disp_h: 600,
        };
        let n = encode_wm_getinfo_rep(&mut buf, &gi);
        buf[3] = 0xFF;
        for i in 8..12 {
            buf[i] = 0xFF;
        }
        assert_eq!(parse_wm_getinfo_rep(&buf[..n]), Some(gi));
        // CREATE ack r/r16（off 1..4、6..8）。
        let n = encode_wm_create_ack(&mut buf, 9);
        buf[1] = 0xEE;
        buf[6] = 0xEE;
        buf[7] = 0xEE;
        assert_eq!(parse_wm_create_ack(&buf[..n]), Some(9));
        // 快照头 r16（off 2..4）。
        let snap = wm_snap_sample(1);
        let n = encode_wm_snapshot_rep(&mut buf, &snap).unwrap();
        buf[2] = 0xEE;
        buf[3] = 0xEE;
        assert_eq!(
            parse_wm_snapshot_rep(&buf[..n]).unwrap().ok().unwrap(),
            snap
        );
    }

    #[test]
    fn wm_open_value_fields_not_rejected() {
        // 开放值域不拒：z/group/parent/version/last_seen 任意值合法帧（值域
        // 执法只钉钦定位域——SET mask 与 CREATE flags 的未知位拒是**值域面**
        // 非「未知位不拒」保留面，两纪律分立注记）。
        let mut buf = [0u8; WM_REP_MAX];
        let wild = WmSetReq {
            mask: WM_SET_F_Z,
            window_id: 0,
            x: 0xFFFF,
            y: 0xFFFF,
            w: 0xFFFF,
            h: 0xFFFF,
            vis: WM_VIS_HIDE,
            z: 0xFF,
        };
        let n = encode_wm_set_req(&mut buf, &wild).unwrap();
        assert_eq!(parse_wm_set_req(&buf[..n]), Some(wild));
        let wild_c = WmCreateReq {
            layer: WM_LAYER_APP,
            flags: 0,
            parent: 0xFFFF,
            group: 0xEE,
            x: 0xFFFF,
            y: 0xFFFF,
            w: 0xFFFF,
            h: 0xFFFF,
        };
        let n = encode_wm_create_req(&mut buf, &wild_c).unwrap();
        assert_eq!(parse_wm_create_req(&buf[..n]), Some(wild_c));
        // 快照 version 任意 u32（含 0——「恒 ≥1」是服务端不变量非 wire 执法，
        // Q3）；id=0 引用存在性归服务端 ST_NOENT 策略面（Q18 分工）。
        for v in [0u32, u32::MAX] {
            let mut s = wm_snap_sample(1);
            s.version = v;
            let n = encode_wm_snapshot_rep(&mut buf, &s).unwrap();
            assert_eq!(parse_wm_snapshot_rep(&buf[..n]).unwrap().ok().unwrap(), s);
        }
        let n = encode_wm_destroy_req(&mut buf, 0);
        assert_eq!(parse_wm_destroy_req(&buf[..n]), Some(0));
    }

    #[test]
    fn wm_wrong_op_confusion_rejects() {
        // 错 op 混淆拒：拿 0x01/0x20/0x40 投 wm 解析器必 None（0x60 带互斥，
        // Q1——结构性防错投）。
        assert_eq!(parse_wm_getinfo_req(&[OP_GETINFO, 0, 0, 0]), None);
        assert_eq!(parse_wm_getinfo_req(&[OP_INPUT_POLL, 0, 0, 0]), None);
        assert_eq!(parse_wm_table_get_req(&[OP_REG_VBLANK, 0, 0, 0]), None);
        for wrong in [OP_GETINFO, OP_INPUT_SUBSCRIBE, OP_COMMIT] {
            let mut f = [0u8; WM_CREATE_REQ_LEN];
            f[0] = wrong;
            assert_eq!(parse_wm_create_req(&f), None);
            let mut f = [0u8; WM_SET_REQ_LEN];
            f[0] = wrong;
            assert_eq!(parse_wm_set_req(&f), None);
            let mut f = [0u8; WM_TABLE_WAIT_REQ_LEN];
            f[0] = wrong;
            assert_eq!(parse_wm_table_wait_req(&f), None);
            let mut f = [0u8; WM_DESTROY_REQ_LEN];
            f[0] = wrong;
            assert_eq!(parse_wm_destroy_req(&f), None);
        }
        // 反向：wm op 投三既有域解析器必 None（既有面零改动的防错投证明）。
        assert_eq!(parse_present_req(&[OP_WM_CREATE, 0, 0, 0]), None);
        assert_eq!(parse_input_poll_req(&[OP_WM_TABLE_GET, 1, 0, 0]), None);
        assert_eq!(parse_reg_vblank_req(&[OP_WM_GETINFO, 0, 0, 0]), None);
        assert_eq!(parse_vblank_wait_req(&[OP_WM_SET, 0, 0, 0]), None);
    }

    #[test]
    #[should_panic(expected = "guiproto")]
    fn wm_encode_getinfo_rep_rejects_cap_overflow() {
        // assert 契约：windows_max 越帽 = 调用方程序员错，炸响不静默。
        let mut buf = [0u8; WM_GETINFO_REP_LEN];
        encode_wm_getinfo_rep(
            &mut buf,
            &WmGetinfoRep {
                version_wm: 1,
                windows_max: WM_WINDOWS_MAX as u8 + 1,
                disp_w: 800,
                disp_h: 600,
            },
        );
    }

    #[test]
    fn wm_encode_none_contract() {
        // None 契约（netdproto 同款——含空 out 切片，不截断不 panic）。
        let mut buf = [0u8; WM_REP_MAX];
        let cr = WmCreateReq {
            layer: WM_LAYER_APP,
            flags: 0,
            parent: 0,
            group: 0,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
        let sr = WmSetReq {
            mask: WM_SET_F_VIS,
            window_id: 1,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            vis: WM_VIS_SHOW,
            z: 0,
        };
        // 值域野值（已在 reject 矩阵钉过——此处 None 面）与 out 不足。
        assert_eq!(encode_wm_create_req(&mut buf, &cr), Some(16));
        assert_eq!(encode_wm_create_req(&mut [], &cr), None);
        assert_eq!(encode_wm_set_req(&mut [], &sr), None);
        let mut small = [0u8; 19];
        assert_eq!(encode_wm_set_req(&mut small, &sr), None);
        let snap = wm_snap_sample(1);
        assert_eq!(encode_wm_snapshot_rep(&mut [], &snap), None);
        let mut small = [0u8; 27];
        assert_eq!(encode_wm_snapshot_rep(&mut small, &snap), None);
    }

    #[test]
    fn wm_hit_test_groups() {
        // 组①空表 → None。
        assert_eq!(wm_hit_test(&[], 400, 300), None);
        let base = WmEntry {
            id: 7,
            parent: 0,
            group: 0,
            x: 100,
            y: 200,
            w: 50,
            h: 60,
            layer: WM_LAYER_APP,
            z: 0,
            flags: WM_WIN_F_VISIBLE,
        };
        // 组②表边界：半开 [x, x+w) × [y, y+h)——左/上界含、右/下界不含；
        // w=0/h=0 恒不中。
        let table = [base];
        assert_eq!(wm_hit_test(&table, 100, 200), Some(7)); // 左上角恰中
        assert_eq!(wm_hit_test(&table, 149, 259), Some(7)); // 右下内界
        assert_eq!(wm_hit_test(&table, 150, 250), None); // 右界外
        assert_eq!(wm_hit_test(&table, 140, 260), None); // 下界外
        assert_eq!(wm_hit_test(&table, 99, 250), None); // 左界外
        assert_eq!(wm_hit_test(&[WmEntry { w: 0, ..base }], 100, 200), None);
        assert_eq!(wm_hit_test(&[WmEntry { h: 0, ..base }], 100, 200), None);
        // 组③Z 遮盖：同层高 z 压低 z（表序无关）；系统层压应用层任意 z；
        // layer/z 双平局取表序靠前。
        let low = WmEntry { id: 1, x: 0, y: 0, w: 800, h: 600, z: 1, ..base };
        let high = WmEntry { id: 2, x: 0, y: 0, w: 800, h: 600, z: 9, ..base };
        let sys = WmEntry { id: 3, x: 0, y: 0, w: 800, h: 30, layer: WM_LAYER_SYSTEM, ..base };
        assert_eq!(wm_hit_test(&[low, high], 400, 300), Some(2));
        assert_eq!(wm_hit_test(&[high, low], 400, 300), Some(2));
        assert_eq!(wm_hit_test(&[low, sys], 400, 10), Some(3)); // 层压 z
        let tie_b = WmEntry { id: 5, ..low };
        let tie_a_table = [WmEntry { id: 4, ..low }, tie_b];
        assert_eq!(wm_hit_test(&tie_a_table, 400, 300), Some(4)); // 平局表序靠前
        // 组④避让带：系统状态栏占 y∈[0,30)，app 有效几何自 y=30 起（避让
        // 已重推导进表项）——带内命中系统窗；系统窗缺席则带内无命中（app
        // 有效矩形不覆盖 = 击穿出表）。
        let app = WmEntry { id: 6, x: 0, y: 30, w: 800, h: 570, ..base };
        assert_eq!(wm_hit_test(&[app, sys], 400, 10), Some(3));
        assert_eq!(wm_hit_test(&[app], 400, 10), None);
        assert_eq!(wm_hit_test(&[app], 400, 300), Some(6));
        // 组⑤隐藏窗让击穿：F_VISIBLE 未置位不参选——点穿到下层；全隐藏 None。
        let hidden = WmEntry { id: 8, flags: 0, ..high };
        assert_eq!(wm_hit_test(&[hidden, low], 400, 300), Some(1));
        assert_eq!(wm_hit_test(&[hidden], 400, 300), None);
    }
}
