//! berry-guiproto —— GUI wire 协议纯函数库（GUI-4 段①，`berry-fsproto`/
//! `berry-psproto`/`berry-netdproto` 先例：「解析器即产品」，零分配、零
//! unsafe、输入只借用；唯一依赖 = berry-manifest——GUI-8 §4 段①/ADR #58
//! 收官：VOTE scene 字节改携 berry_intent 六档值，`Intent` 判别值经其
//! 镜像 sched/src/intent.rs 单源转发，禁第三份手抄）。
//!
//! 拓扑（docs/GUI-4实施设计草案-v1.0.md §2）：**三域 = 三对独立 channel**，不是
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
/// present 域：输入脉冲（刀3 A6 触摸加速触发面——4B 无参、ST_OK 应答；
/// 任何持 present 端的观察者可发：v1 = 客户端输入环，inputd 直连挂
/// extras 抬帽刀）。add-only：7 前此号空缺。
pub const OP_INPUT_PULSE: u8 = 7;
/// present 域：呈现回执查询（刀4 P3——8B 请求携 frame_token，28B 应答
/// `{frame_token, presented_ns, seq, refresh_ns, flags}`；FRAME_CALLBACK
/// 16B 定长不动，本 op 是非阻塞的时序真相查询面）。add-only：8 前此号
/// 空缺（规范 v1.0 原文 0x07 与刀3 INPUT_PULSE 撞号，v1.1 修订）。
pub const OP_FRAME_TIMING: u8 = 8;
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
/// hdi 域：面板能力宣告（刷新率档位表 + 率域模型 + 当前档——刷新率体系
/// 规范 §4-4；hdi 域下一空位，Q2 裁弃先例 = 不塞 present GETINFO 保留位）。
pub const OP_PANEL_CAPS: u8 = 0x44;
/// hdi 域：等下一次 vblank 并取拍点三元组 `{seq u64, ts_ns u64,
/// refresh_ns u32}`（刷新率体系规范 §4-4 事件面；刀2）。VBLANK_WAIT 的
/// 16B 应答是 GOLDEN 钉板面（ADR #60 ⑥ add-only）——三元组走本新 op
/// 24B 应答，16B 形永不动。
pub const OP_VBLANK_TIMING: u8 = 0x45;
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
// ---------- 波2 输入焦点协议（六波方案步骤 3——add-only 扩面） ----------
//
// guiproto add-only 纪律：只增常量与新编解码函数，不动任何既有 op 的
// 字节面；服务端执法（wmd/guifbd 侧）挂波3 接线。

/// wm 域：焦点请求（客户端 → wmd：申请/让出焦点。payload `[op][win][r16]`）。
pub const OP_WM_FOCUS: u8 = 0x66;
/// wm 域：返回键路由（导航窗 → wmd：路由到焦点窗；payload `[op][r24]`）。
pub const OP_WM_BACK: u8 = 0x67;
/// input 域：授权注入（softime/测试 → gui-inputd：以真实输入同一事件面
/// 注入键盘/文本事件；payload `[op][dev_kind][key/text...]`——dev_kind =
/// DEV_KEYBOARD 的 code = Linux input 码（EV_KEY 泵同词汇，softime 镜像；
/// 与 INPUT_POLL 出向覆载的翻译后字节构成两域分注，见 DEV_KEYBOARD 注）。
/// 授权面（谁可注入）执法在 inputd 服务端。
pub const OP_INPUT_INJECT: u8 = 0x22;

/// 窗口层：服务层（软键盘 softime 等——压 SYSTEM 之上、独立合成带）。
pub const WM_LAYER_SERVICE: u8 = 2;

/// input 域设备类（PointerEvent.device_id 高位带——指针帧 v1 device_id
/// = 0 恒触屏；键盘/文本注入事件的 dev_kind 字节值域；DEV_KEYBOARD 另
/// 有 INPUT_POLL 出向覆载义，见其分注）：触摸屏。
pub const DEV_POINTER: u8 = 0;
/// 键盘（key 事件）。code 两域双口径（原单口径注「code = ASCII 可打印
/// 字节或控制键」只对出向成立，本批分注澄清）：
/// - 入向 OP_INPUT_INJECT：code = Linux input 码（gui-inputd EV_KEY
///   feed 泵同词汇——非 BerryOS 私码；softime 镜像 KEY_BACKSPACE=14/
///   KEY_ENTER=28，user/berry-softime/src/lib.rs）。
/// - 出向覆载 INPUT_POLL：inputd 把键盘事件借 PointerEvent 32B 定长帧
///   出向——device_id = DEV_KEYBOARD 时 x = 翻译后出向字节（ASCII 可
///   打印 + 控制码 0x0A 回车/0x08 退格/0x09 Tab/0x1B Esc + Ctrl 组合码
///   `c and 0x1F`）、phase = PHASE_DOWN、其余字段 0/合法占位；v1 只发
///   按下（release 不上 wire）。
pub const DEV_KEYBOARD: u8 = 1;
/// 文本提交（IME 提交面——payload = ASCII 字节串）。
pub const DEV_TEXT: u8 = 2;

// ---------- 波3 wm 事件通道（六波方案步骤 4 / 波3设计 §3——add-only） ----------
//
// guiproto add-only 纪律同波2：只增常量与新编解码函数，不动任何既有 op
// 的字节面。事件通道 = wm 域旁路单帧通道——WM_CREATE 应答随行 gift 的
// 属主端（表项记属主）或 OP_WM_SUBSCRIBE 订阅端（无窗跟随者面），只承载
// 下列三帧型，不与 wm_srv 请求面混跑。帧型带 0x90..=0x9F 落在全部 op 带
// （present 1..=6 / input 0x20..=0x22 / hdi 0x40..=0x43 / wm 0x60..=0x7F）
// 之外——错投任一请求分发面恒 ST_BAD，结构性防错投（wm 带互斥 Q1 同法）。

/// wm 域：订阅表变更事件通道（无窗跟随者——guifbd 表快照「通知驱动 +
/// 异步拉取」面，波3设计 §4）；ack = 1B ST_OK + 随行恰 1 handle（订阅者
/// 事件通道客户端端）。表跟随纪律（波3设计 §9）：TABLE_WAIT 冻结零新增
/// 驻留者，跟随一律走本通道通知 + TABLE_GET 快照。
pub const OP_WM_SUBSCRIBE: u8 = 0x68;

// ---------- per-app wm 通道（K1——launcher/各 [ui] app 独占通道面） ----------
//
// add-only 纪律同波2/波3：只增常量与新编解码函数，不动任何既有 op 的
// 字节面。0x69/0x6A 落 wm 域 0x60..=0x7F 带内（0x68 之后顺位），与
// 0x90..=0x9F 事件帧带外互不认领（帧型带隔离——新 codec 与 parse_wm_evt
// 双向钉板）。

/// wm 域：per-app 通道挂接（K1：客户端 → wmd 控制通道发）。请求恰 1B
/// `[op]` + 随行恰 1 handle（对端 srv 端——能力随消息迁入 wmd apps 表，
/// 应答面自此走该新通道——共享 dup 单 FIFO 应答互截缺陷的结构性解）；
/// 应答 = wmd 在**新通道**上回 1B ST_OK + 随行恰 1 handle（wmd 自持
/// srv 端的 TRANSFER 别名——DETACH 寻址面）。恰 1 随行的执法在 wmd
/// on_wm 分发层（wm 域零随行门之前——0x69 是 wm 域唯一合法随行 op）。
pub const OP_WM_ATTACH: u8 = 0x69;
/// wm 域：per-app 通道摘除（K1：app 退出的 wmd 侧槽回收面——app_reap_tail
/// 经控制通道发）。请求 5B `[op][srv_handle u32 LE]` 零随行；应答 1B
/// ST_OK/ST_BAD（wm_srv 控制通道回）。`srv_handle` 语义 = wmd apps 表项
/// handle 值（wmd 命名空间——内核 handle 表按进程私有、随行迁移必重编号，
/// 跨进程 id 不互通；寻址合同细面见 wmd 侧实现注）。
pub const OP_WM_DETACH: u8 = 0x6A;
/// 事件帧型：返回键投递（wmd → 焦点窗属主；`[kind][wid u16][r]`——500ms
/// 无 BACK_ACK 且无表变更则 wmd 兜底 DESTROY，wire 只管投递不管兜底）。
pub const WM_EVT_BACK: u8 = 0x90;
/// 事件帧型：表变更通知（wmd → 全体属主/订阅者；`[kind][version u32]`——
/// 尽力而为：非阻塞 send 满即丢，真值恒经 TABLE_GET 快照，丢弃至多迟一拍）。
pub const WM_EVT_TABLE: u8 = 0x91;
/// 事件帧型：返回应答（属主 → wmd，1B 无域——解除 500ms 兜底槽）。
pub const WM_EVT_BACK_ACK: u8 = 0x92;

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
/// PANEL_CAPS 档位表条目数上限（全表 9 档 + 余量）。
pub const PANEL_TIERS_MAX: usize = 16;

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
/// hdi 域应答帧帽（VBLANK_WAIT 应答 16B；PANEL_CAPS 变长应答独帽见下）。
pub const HDI_REP_MAX: usize = 16;
/// PANEL_CAPS 应答头定长（§4-4 头布局）。
pub const PANEL_CAPS_REP_HEAD: usize = 32;
/// PANEL_CAPS 档位条目长（8B）。
pub const PANEL_TIER_LEN: usize = 8;
/// PANEL_CAPS 应答帧帽 = 32 + 8×16（≤ CHAN_MSG_MAX 256——relay 256 窗直过）。
pub const PANEL_CAPS_REP_MAX: usize = PANEL_CAPS_REP_HEAD + PANEL_TIER_LEN * PANEL_TIERS_MAX;
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

/// present 域版本（v2 = 刀4 增 OP_FRAME_TIMING 呈现回执查询——刷新率
/// 体系规范 §7-P3）。
pub const VER_PRESENT: u8 = 2;
/// input 域版本。
pub const VER_INPUT: u8 = 1;
/// hdi 域版本（v2 = 刀1 增 OP_PANEL_CAPS；v3 = 刀2 增 OP_VBLANK_TIMING
/// 事件三元组 + PANEL_CAPS v2 档位行 stage 字节——刷新率体系规范 §4-4）。
pub const VER_HDI: u8 = 3;
/// wm 域版本（GUI-5 定形刀在册——ADR #50：per-interface version 随定形刀必落）。
pub const VER_WM: u8 = 1;

// ---------- 帧长常量（私有；公开帽常量见上）----------

/// SUBMIT 请求定长头。
const SUBMIT_REQ_HEAD: usize = 24;
/// SUBMIT 应答恰长：`[st][r][r16][frame_token u32]`。
const SUBMIT_ACK_LEN: usize = 8;
/// FRAME_CALLBACK 应答恰长 16B。
const FRAME_DONE_REP_LEN: usize = 16;
/// FRAME_TIMING 应答恰长（刀4 P3：`[st][flags][r16][token u32]
/// [presented_ns u64][seq u64][refresh_ns u32]`）。
const FRAME_TIMING_REP_LEN: usize = 28;
/// RELEASE 应答定长头。
const RELEASE_REP_HEAD: usize = 4;
/// VOTE 请求恰长 12B。
const VOTE_REQ_LEN: usize = 12;
/// VBLANK_WAIT 应答恰长 16B。
const VBLANK_REP_LEN: usize = 16;
/// VBLANK_TIMING 应答恰长 24B（事件面三元组——规范 §4-4）。
const VBLANK_TIMING_REP_LEN: usize = 24;
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

/// 层值域执法（波3 收宽——波2 R1 立法「拒点从 2 移钉 3」落地）：SERVICE=2
/// 入域（softime 服务窗面），3 起仍拒。scene_known 收宽先例同款（值域
/// 收宽不升版——VER_WM 恒 1）。
const fn wm_layer_known(l: u8) -> bool {
    l <= WM_LAYER_SERVICE
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
    /// INPUT_PULSE：输入活动脉冲（刀3 A6——无参，ST_OK 应答；触发仲裁器
    /// boost 窗，非投票面）。
    InputPulse,
    /// FRAME_TIMING：呈现回执查询（刀4 P3——8B 请求携 frame_token；命中
    /// → 28B 回执，未命中 → 1B ST_NOENT；非阻塞）。
    FrameTiming { frame_token: u32 },
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
        PresentReq::InputPulse => {
            let m = out.get_mut(..4)?;
            m[0] = OP_INPUT_PULSE;
            m[1] = 0;
            m[2] = 0;
            m[3] = 0;
            Some(4)
        }
        PresentReq::FrameTiming { frame_token } => {
            let m = out.get_mut(..8)?;
            m[0] = OP_FRAME_TIMING;
            m[1] = 0;
            m[2] = 0;
            m[3] = 0;
            m[4..8].copy_from_slice(&frame_token.to_le_bytes());
            Some(8)
        }
    }
}

/// 解析 present 域请求：七 op 各自恰长执法（截断/尾随 = 野值拒）；rect
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
        OP_INPUT_PULSE => {
            if buf.len() != 4 {
                return None;
            }
            Some(PresentReq::InputPulse)
        }
        OP_FRAME_TIMING => {
            if buf.len() != 8 {
                return None;
            }
            Some(PresentReq::FrameTiming {
                frame_token: le32(buf, 4)?,
            })
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

/// FRAME_TIMING 应答（刀4 P3 呈现回执——非阻塞时序真相查询；一次消费
/// 面可反复查直至 token 被环覆盖）。`seq` = 呈现归属拍的全点序（软面板
/// vblank 谱系——与 OP_VBLANK_TIMING 事件 seq 同域，T6 一号到底）；未
/// 启动拍点的停帧窗内 = 下一将拍点的序号（预告语义归属拍）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameTimingRep {
    pub frame_token: u32,
    /// 呈现时刻（guest 钟单源——P4 断言锚）。
    pub presented_ns: u64,
    /// 呈现归属拍全点序（vblank 事件 seq 同域）。
    pub seq: u64,
    /// 呈现时面板周期（ns）。
    pub refresh_ns: u32,
    /// FRAME_TIMING_F_* 位或。
    pub flags: u8,
}

/// FRAME_TIMING_F_PRESENTED：本帧已呈现（P1 恒置；P2 latch 呈现时置——
/// DISCARDED 与之互斥）。
pub const FRAME_TIMING_F_PRESENTED: u8 = 0x01;
/// FRAME_TIMING_F_FULL：damage = FULL（全帧重绘 vs 局部 rect 集）。
pub const FRAME_TIMING_F_FULL: u8 = 0x02;
/// FRAME_TIMING_F_DISCARDED：本帧被 squash 弃帧（P2 深度 1 管线：新
/// P2 帧压旧 P2 帧 = 旧帧从未上墙——刀6 §7-P2；与 PRESENTED 互斥）。
pub const FRAME_TIMING_F_DISCARDED: u8 = 0x04;

/// 编码 FRAME_TIMING 应答（恰 28B：`[st=0][flags][r16][frame_token u32]
/// [presented_ns u64][seq u64][refresh_ns u32]`；buf 不足 / flags 野位 =
/// 调用方错，assert）。
pub fn encode_frame_timing_rep(out: &mut [u8], rep: &FrameTimingRep) -> usize {
    assert!(out.len() >= FRAME_TIMING_REP_LEN, "guiproto: frame timing buf 不足");
    assert!(rep.flags & !(FRAME_TIMING_F_PRESENTED | FRAME_TIMING_F_FULL | FRAME_TIMING_F_DISCARDED) == 0,
        "guiproto: 野 frame timing flags");
    out[0] = ST_OK;
    out[1] = rep.flags;
    out[2] = 0;
    out[3] = 0;
    out[4..8].copy_from_slice(&rep.frame_token.to_le_bytes());
    out[8..16].copy_from_slice(&rep.presented_ns.to_le_bytes());
    out[16..24].copy_from_slice(&rep.seq.to_le_bytes());
    out[24..28].copy_from_slice(&rep.refresh_ns.to_le_bytes());
    FRAME_TIMING_REP_LEN
}

/// 解析 FRAME_TIMING 应答：恰 28B + st=ST_OK；flags 未知位前向兼容不拒
/// （v2 加位不改旧读端——FMT 位域先例）。
pub fn parse_frame_timing_rep(buf: &[u8]) -> Option<FrameTimingRep> {
    if buf.len() != FRAME_TIMING_REP_LEN || buf[0] != ST_OK {
        return None;
    }
    Some(FrameTimingRep {
        frame_token: le32(buf, 4)?,
        presented_ns: le64(buf, 8)?,
        seq: le64(buf, 16)?,
        refresh_ns: le32(buf, 24)?,
        flags: buf[1],
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

/// 率域模型二类（规范 §3.2）：Tiered = 离散档位表；FreeRange = 连续区间。
pub const RATE_MODEL_TIERED: u8 = 0;
pub const RATE_MODEL_FREERANGE: u8 = 1;

/// PANEL_CAPS flags 位（规范 §4-4）：per_frame = FreeRange 逐帧变节拍
///（§7-P6）；hold = 面板内容自保持（停帧物理前提）；self_refresh = PSR；
/// beat_owner = 节拍归属面板（panel-driven，v3 硬件 TE 位）。
pub const PANEL_FLAG_PER_FRAME: u8 = 1 << 0;
pub const PANEL_FLAG_HOLD: u8 = 1 << 1;
pub const PANEL_FLAG_SELF_REFRESH: u8 = 1 << 2;
pub const PANEL_FLAG_BEAT_OWNER: u8 = 1 << 3;

/// 档位 class（规范 §3.1 族列）：产品 / 测试（非测试模式禁选）/ LTPO。
pub const PANEL_CLASS_PRODUCT: u8 = 0;
pub const PANEL_CLASS_TEST: u8 = 1;
pub const PANEL_CLASS_LTPO: u8 = 2;

/// vrr 三态（DRM vrr_capable 形状）。
pub const PANEL_VRR_NONE: u8 = 0;
pub const PANEL_VRR_CAPABLE: u8 = 1;
pub const PANEL_VRR_SUPPORTED_OFF: u8 = 2;

/// 编码 PANEL_CAPS 请求（恰 4B 无参 `[op][r][r16]`，VBLANK_WAIT 同形）。
pub fn encode_panel_caps_req(out: &mut [u8]) -> usize {
    assert!(out.len() >= 4, "guiproto: panel caps buf 不足");
    out[0] = OP_PANEL_CAPS;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    4
}

/// 解析 PANEL_CAPS 请求：恰 4B + op 位（无参；保留域恒不校验）。
pub fn parse_panel_caps_req(buf: &[u8]) -> Option<()> {
    if buf.len() != 4 || buf[0] != OP_PANEL_CAPS {
        return None;
    }
    Some(())
}

/// 档位表条目（8B wire `[hz u16][class u8][stage u8][period_ns u32]`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelTier {
    pub hz: u16,
    /// PANEL_CLASS_*（0=产品 1=测试 2=LTPO——规范 §3.1）。
    pub class: u8,
    /// softpanel::STAGE_* 首启阶段（规范 §3.1 阶段列；v2 起在册——v1 应答
    /// 此位恒 0，解析面前向透传）。
    pub stage: u8,
    /// 周期 ns = 档位表单源值（规范 §3.4：round(1e9/hz)）。
    pub period_ns: u32,
}

/// PANEL_CAPS 应答体（变长 = 32B 头 + 8B×n_tiers；解析面执法长度恰等）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelCapsRep {
    /// 应答格式版本（v1 = 1）。
    pub version: u8,
    /// RATE_MODEL_*（0=Tiered 1=FreeRange——规范 §3.2 二类）。
    pub rate_model: u8,
    /// PANEL_FLAG_* 位或。
    pub flags: u8,
    /// Tiered = 档位表首尾；FreeRange = 区间边界。
    pub min_hz: u16,
    pub max_hz: u16,
    /// 连续域粒度（Tiered 恒 0 = 无意义；FreeRange 默认 1Hz）。
    pub granularity_hz: u16,
    /// PANEL_VRR_* 三态（缺属性 = 缺支持 ≠ 支持但关闭）。
    pub vrr: u8,
    pub current_hz: u16,
    pub current_period_ns: u32,
    /// 当前相位 ns（v1 恒 0——FREE 自由钟起点即相位零）。
    pub current_phase_ns: u32,
    /// 档位表定长槽 + n_tiers 计数（帽 [`PANEL_TIERS_MAX`]）。
    pub tiers: [PanelTier; PANEL_TIERS_MAX],
    pub n_tiers: u8,
}

/// 编码 PANEL_CAPS 应答（32B 头 + 8B×n_tiers；buf 不足 / 档位越帽 =
/// 调用方错，assert）。头布局：`[st][ver][model][flags][n][vrr][r×2]
/// [min u16][max u16][gran u16][r16][cur_hz u16][r16][cur_period u32]
/// [cur_phase u32][r u32]`。
pub fn encode_panel_caps_rep(out: &mut [u8], rep: &PanelCapsRep) -> usize {
    let n = rep.n_tiers as usize;
    assert!(n <= PANEL_TIERS_MAX, "guiproto: panel tiers 越帽");
    let len = PANEL_CAPS_REP_HEAD + PANEL_TIER_LEN * n;
    assert!(out.len() >= len, "guiproto: panel caps rep buf 不足");
    out[0] = ST_OK;
    out[1] = rep.version;
    out[2] = rep.rate_model;
    out[3] = rep.flags;
    out[4] = rep.n_tiers;
    out[5] = rep.vrr;
    out[6] = 0;
    out[7] = 0;
    out[8..10].copy_from_slice(&rep.min_hz.to_le_bytes());
    out[10..12].copy_from_slice(&rep.max_hz.to_le_bytes());
    out[12..14].copy_from_slice(&rep.granularity_hz.to_le_bytes());
    out[14] = 0;
    out[15] = 0;
    out[16..18].copy_from_slice(&rep.current_hz.to_le_bytes());
    out[18] = 0;
    out[19] = 0;
    out[20..24].copy_from_slice(&rep.current_period_ns.to_le_bytes());
    out[24..28].copy_from_slice(&rep.current_phase_ns.to_le_bytes());
    out[28] = 0;
    out[29] = 0;
    out[30] = 0;
    out[31] = 0;
    for (i, t) in rep.tiers[..n].iter().enumerate() {
        let o = PANEL_CAPS_REP_HEAD + i * PANEL_TIER_LEN;
        out[o..o + 2].copy_from_slice(&t.hz.to_le_bytes());
        out[o + 2] = t.class;
        out[o + 3] = t.stage;
        out[o + 4..o + 8].copy_from_slice(&t.period_ns.to_le_bytes());
    }
    len
}

/// 解析 PANEL_CAPS 应答：st=ST_OK + 长度恰 = 头 + 8B×n + n ≤ 帽
///（尾随拒）；model/flags/vrr 前向不校验（未知值透传——add-only 纪律）。
pub fn parse_panel_caps_rep(buf: &[u8]) -> Option<PanelCapsRep> {
    if buf.len() < PANEL_CAPS_REP_HEAD || buf[0] != ST_OK {
        return None;
    }
    let n = buf[4] as usize;
    if n > PANEL_TIERS_MAX || buf.len() != PANEL_CAPS_REP_HEAD + PANEL_TIER_LEN * n {
        return None;
    }
    let mut tiers = [PanelTier { hz: 0, class: 0, stage: 0, period_ns: 0 }; PANEL_TIERS_MAX];
    for (i, t) in tiers[..n].iter_mut().enumerate() {
        let o = PANEL_CAPS_REP_HEAD + i * PANEL_TIER_LEN;
        *t = PanelTier {
            hz: le16(buf, o)?,
            class: buf[o + 2],
            stage: buf[o + 3],
            period_ns: le32(buf, o + 4)?,
        };
    }
    Some(PanelCapsRep {
        version: buf[1],
        rate_model: buf[2],
        flags: buf[3],
        min_hz: le16(buf, 8)?,
        max_hz: le16(buf, 10)?,
        granularity_hz: le16(buf, 12)?,
        vrr: buf[5],
        current_hz: le16(buf, 16)?,
        current_period_ns: le32(buf, 20)?,
        current_phase_ns: le32(buf, 24)?,
        tiers,
        n_tiers: buf[4],
    })
}

// ---------- OP_VBLANK_TIMING：vblank 拍点三元组（刷新率体系规范 §4-4 事件面）----------

/// 编码 VBLANK_TIMING 请求（恰 4B 无参 `[op][r][r16]`，VBLANK_WAIT 同形）。
pub fn encode_vblank_timing_req(out: &mut [u8]) -> usize {
    assert!(out.len() >= 4, "guiproto: vblank timing buf 不足");
    out[0] = OP_VBLANK_TIMING;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    4
}

/// 解析 VBLANK_TIMING 请求：恰 4B + op 位（无参；保留域恒不校验）。
pub fn parse_vblank_timing_req(buf: &[u8]) -> Option<()> {
    if buf.len() != 4 || buf[0] != OP_VBLANK_TIMING {
        return None;
    }
    Some(())
}

/// VBLANK_TIMING 应答（阻塞应答 24B：`[st=0][r][r16][seq u64][ts_ns u64]
/// [refresh_ns u32]`——事件面三元组；VblankRep 16B 是 GOLDEN 钉板面，
/// 三元组不入彼帧）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VblankTimingRep {
    /// vblank 序号（软面板全点序；丢弃点消耗序号——客户端见跳号 = 诚实
    /// 丢帧信号）。
    pub seq: u64,
    /// vblank 时刻 ns（FREE = 标称点；DET = 虚拟钟精确步进）。
    pub ts_ns: u64,
    /// 本拍刷新周期 ns（切档生效拍起报新周期）。
    pub refresh_ns: u32,
}

/// 编码 VBLANK_TIMING 应答（恰 24B；buf 不足 = 调用方错，assert）。
pub fn encode_vblank_timing_rep(out: &mut [u8], rep: &VblankTimingRep) -> usize {
    assert!(out.len() >= VBLANK_TIMING_REP_LEN, "guiproto: vblank timing rep buf 不足");
    out[0] = ST_OK;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    out[4..12].copy_from_slice(&rep.seq.to_le_bytes());
    out[12..20].copy_from_slice(&rep.ts_ns.to_le_bytes());
    out[20..24].copy_from_slice(&rep.refresh_ns.to_le_bytes());
    VBLANK_TIMING_REP_LEN
}

/// 解析 VBLANK_TIMING 应答：恰 24B + st=ST_OK。
pub fn parse_vblank_timing_rep(buf: &[u8]) -> Option<VblankTimingRep> {
    if buf.len() != VBLANK_TIMING_REP_LEN || buf[0] != ST_OK {
        return None;
    }
    Some(VblankTimingRep {
        seq: le64(buf, 4)?,
        ts_ns: le64(buf, 12)?,
        refresh_ns: le32(buf, 20)?,
    })
}

/// 软面板纯核（刷新率体系规范 §5 四接口的能力/事件/控制/故障注入面的
/// 可宿测部分）：档位周期表单源（§3.4——表的家自 guifbd `mod panel`
/// 迁入，规范 §13「按概念迁移不按文件形态」；活设备半件留 guifbd 调用
/// 侧）+ boot 配置域解包 + 拍点状态机（FREE 零扰动 / DET 虚拟钟 / 故障
/// 旋钮全确定）。控制面 wire 化（切档 op）在刀3 仲裁器——本模块先立
/// 内部面 + host 回放门。
pub mod softpanel {
    use super::{PANEL_CLASS_LTPO, PANEL_CLASS_PRODUCT, PANEL_CLASS_TEST, RATE_MODEL_FREERANGE};

    /// 首启阶段（规范 §3.1 阶段列）：档位表随分期解锁。
    pub const STAGE_V1: u8 = 1;
    pub const STAGE_V2: u8 = 2;
    pub const STAGE_V3: u8 = 3;

    /// 档位表条目（单源；wire 面 [`super::PanelTier`] 由 guifbd 装配）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TierDef {
        pub hz: u16,
        /// 周期 ns = round(1e9/hz)。
        pub period_ns: u64,
        /// PANEL_CLASS_*（测试档非测试模式禁选归刀3 仲裁器过滤——面板边
        /// 只验档位成员制）。
        pub class: u8,
        /// STAGE_*。
        pub stage: u8,
    }

    /// 档位周期表单源（规范 §3.1 全表：产品 5 + 测试 2 + LTPO 2）。
    pub const TIERS: [TierDef; 9] = [
        TierDef { hz: 30, period_ns: 33_333_333, class: PANEL_CLASS_PRODUCT, stage: STAGE_V1 },
        TierDef { hz: 60, period_ns: 16_666_667, class: PANEL_CLASS_PRODUCT, stage: STAGE_V1 },
        TierDef { hz: 120, period_ns: 8_333_333, class: PANEL_CLASS_PRODUCT, stage: STAGE_V1 },
        TierDef { hz: 240, period_ns: 4_166_667, class: PANEL_CLASS_PRODUCT, stage: STAGE_V2 },
        TierDef { hz: 480, period_ns: 2_083_333, class: PANEL_CLASS_PRODUCT, stage: STAGE_V2 },
        TierDef { hz: 165, period_ns: 6_060_606, class: PANEL_CLASS_TEST, stage: STAGE_V2 },
        TierDef { hz: 180, period_ns: 5_555_556, class: PANEL_CLASS_TEST, stage: STAGE_V2 },
        TierDef { hz: 1, period_ns: 1_000_000_000, class: PANEL_CLASS_LTPO, stage: STAGE_V3 },
        TierDef { hz: 10, period_ns: 100_000_000, class: PANEL_CLASS_LTPO, stage: STAGE_V3 },
    ];

    /// Tiered 档位成员校验（表外 → None = 调用方 loud 回退面）。
    pub fn resolve(hz: u16) -> Option<(u16, u64)> {
        TIERS.iter().find(|t| t.hz == hz).map(|t| (t.hz, t.period_ns))
    }

    /// 周期单点换算（§3.4 唯一换算点）：表内直取；表外半取整
    /// `round(1e9/hz)`——FreeRange 任意 hz 与表外请求共用。hz=0 → 0
    ///（退化值；调用方档位门在先）。
    pub fn period_of(hz: u16) -> u64 {
        match resolve(hz) {
            Some((_, p)) => p,
            None if hz > 0 => (1_000_000_000 + hz as u64 / 2) / hz as u64,
            None => 0,
        }
    }

    /// FreeRange 仿真宣告区间（§3.2：1..=480Hz 连续）。
    pub const FREERANGE_MIN_HZ: u16 = 1;
    pub const FREERANGE_MAX_HZ: u16 = 480;
    /// 连续域粒度（默认 1Hz——量化即钳制；粒度面留位给未来非 1 面板）。
    pub const FREERANGE_GRAN_HZ: u16 = 1;

    /// FreeRange 请求量化：钳区间 + 粒度取整。
    pub fn freerange_quantize(hz: u16) -> u16 {
        let hz = hz.max(FREERANGE_MIN_HZ).min(FREERANGE_MAX_HZ);
        hz - hz % FREERANGE_GRAN_HZ
    }

    /// 控制面模式（§5；boot 固定，运行期不切换）。
    pub const MODE_FREE: u8 = 0;
    /// 宿主锁（刀3 仲裁器立法后接入；刀2 预留在册不生效）。
    pub const MODE_HOST_LOCK: u8 = 1;
    /// 确定性钟：t0 由 seed 派生、ts = t0 + k·period 精确步进，墙钟无关
    ///（§10 T2 回放门——同输入序列 → 位元相等 (seq, ts_ns)）。DET 态关
    /// 追赶（每醒恰一点）：追赶计数依赖墙钟滞后量，会破回放。
    pub const MODE_DETERMINISTIC: u8 = 2;

    /// boot info VMO 尾插域 7/8 的解包形（§5 配置面）。零域 = 缺省
    /// Tiered/FREE/无故障（旧内核 zero 尾 → 缺省态，add-only 尾插纪律）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PanelCfg {
        /// RATE_MODEL_*。
        pub model: u8,
        /// MODE_*。
        pub mode: u8,
        /// DET 钟基派生 + LCG 种子（FREE 态仅作后者）。
        pub seed: u32,
        /// 丢帧率千分比（0..=1000；0 = 无丢帧）。
        pub drop_permille: u16,
        /// 拍点 ts 抖动幅度 ns（仅扰上报 ts，不扰调度与钟基）。
        pub jitter_ns: u32,
        /// 切档生效拍起丢弃帧数（面板重配伪影）。
        pub sw_drop: u8,
        /// P6 变节拍（§7-P6，刀6）：FreeRange 逐帧扫描形态（拍点 =
        /// max(上一扫描 + min_interval, target)——帧就绪即上墙）。仅
        /// FreeRange 有意义（Tiered 档位制无逐帧语义，位被忽略）。d7
        /// bit48（保留位 add-only——旧内核零域 → false 缺省兼容）。
        pub per_frame: bool,
        /// PANEL_VRR 三态（§6.3 A8 的门位；d7 bits49-50 add-only）。
        /// FreeRange 仿真恒 CAPABLE（宣告与仲裁门同源）；Tiered 仿真缺省
        /// NONE，`tiered:vrr` boot 旋钮抬 CAPABLE（LFC guest 活证用）。
        pub vrr: u8,
    }

    impl PanelCfg {
        /// 缺省（零域）形。
        pub const DEFAULT: PanelCfg = PanelCfg {
            model: super::RATE_MODEL_TIERED,
            mode: MODE_FREE,
            seed: 0,
            drop_permille: 0,
            jitter_ns: 0,
            sw_drop: 0,
            per_frame: false,
            vrr: super::PANEL_VRR_NONE,
        };

        /// 解包 LE u64 域对（布局见 [`PanelCfg::pack_domains`]——kernel 打包
        /// 手镜像，host 对拍钉板）。
        pub fn from_domains(d7: u64, d8: u64) -> PanelCfg {
            PanelCfg {
                model: (d7 & 0xFF) as u8,
                mode: ((d7 >> 8) & 0xFF) as u8,
                seed: (d7 >> 16) as u32,
                drop_permille: (d8 & 0xFFFF) as u16,
                jitter_ns: ((d8 >> 16) & 0xFFFF_FFFF) as u32,
                sw_drop: ((d8 >> 48) & 0xFF) as u8,
                per_frame: (d7 >> 48) & 1 == 1,
                vrr: ((d7 >> 49) & 0b11) as u8,
            }
        }

        /// 打包 LE u64 域对：域 7 = `[model u8][mode u8][seed u32]
        /// [per_frame u1][vrr u2][r13]`；域 8 = `[drop_pm u16][jitter_ns
        /// u32][sw_drop u8][r u8]`。
        pub fn pack_domains(&self) -> (u64, u64) {
            let d7 = (self.model as u64)
                | ((self.mode as u64) << 8)
                | ((self.seed as u64) << 16)
                | ((self.per_frame as u64) << 48)
                | (((self.vrr & 0b11) as u64) << 49);
            let d8 = (self.drop_permille as u64)
                | ((self.jitter_ns as u64) << 16)
                | ((self.sw_drop as u64) << 48);
            (d7, d8)
        }
    }

    /// 故障裁决 LCG（Knuth MMIX 常数；seed 派生初态）。
    fn lcg_step(x: &mut u64) -> u64 {
        *x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *x >> 33
    }

    /// 软面板拍点状态机。FREE 无故障态 = 现状零扰动面（ts 直通标称点）；
    /// DET 态虚拟钟墙钟无关；故障旋钮（丢帧/抖动/切档丢帧）LCG 裁决全确定。
    pub struct SoftPanel {
        cfg: PanelCfg,
        hz: u16,
        period_ns: u64,
        /// 全点序（丢弃点也消耗——面板拍点存在、事件未投递）。
        seq: u64,
        /// DET 虚拟钟：ts = det_base + det_k·period（切档生效拍重锚）。
        det_base_ns: u64,
        det_k: u64,
        /// LCG 状态。
        rng: u64,
        /// 在途切档 (新 hz, 新周期, 距生效帧数)。
        pending: Option<(u16, u64, u32)>,
        /// 切档生效拍起待丢帧数。
        sw_drop_left: u8,
        /// 最近一次切档生效 ACK（无切档 = None）。
        last_ack: Option<SwitchAck>,
        /// P6 DET 虚拟钟累进：上一扫描墙钟（0 = 未见首扫）。
        p6_last_wall_ns: u64,
        /// P6 DET 虚拟钟已流逝 ns（ts = det_base + 本值——按扫描间隔累进，
        /// 同输入序列 → 同 ts 链，T2 回放门 P6 变体的钟基）。
        p6_det_elapsed_ns: u64,
    }

    /// 切档 ACK（§5 控制面；生效拍拍出）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SwitchAck {
        /// 生效拍 ts（抖动前标称值——抖动是量测噪声不入钟基）。
        pub effective_ns: u64,
        /// 生效后周期。
        pub new_refresh_ns: u32,
    }

    impl SoftPanel {
        /// 由 boot 配置 + 初始档构造（初始档解析归调用方——Tiered 表外
        /// loud 回退是 guifbd 活设备面）。
        pub fn new(cfg: PanelCfg, hz: u16) -> SoftPanel {
            SoftPanel {
                cfg,
                hz,
                period_ns: period_of(hz),
                seq: 0,
                det_base_ns: 1_000_000_000 + cfg.seed as u64 * 6_700_417,
                det_k: 0,
                rng: 0x9E37_79B9_7F4A_7C15 ^ (cfg.seed as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9),
                pending: None,
                sw_drop_left: 0,
                last_ack: None,
                p6_last_wall_ns: 0,
                p6_det_elapsed_ns: 0,
            }
        }

        pub fn hz(&self) -> u16 {
            self.hz
        }

        pub fn period_ns(&self) -> u64 {
            self.period_ns
        }

        /// 下一将拍点的全点序（呈现归属拍查询——刀4 P3 回执 seq 源：本帧
        /// 落在当前拍间隙 → 归属下一拍 = `seq + 1`；与 on_point 产出的
        /// 事件 seq 同域，T6 一号到底）。
        pub fn next_seq(&self) -> u64 {
            self.seq + 1
        }

        pub fn mode(&self) -> u8 {
            self.cfg.mode
        }

        /// boot 配置透出（PANEL_CAPS 模型分形等消费面）。
        pub fn cfg(&self) -> PanelCfg {
            self.cfg
        }

        /// 最近切档 ACK（刀3 仲裁器查询面）。
        pub fn last_ack(&self) -> Option<SwitchAck> {
            self.last_ack
        }

        /// P6 逐帧扫描形态在席（`freerange:per_frame` boot——§7-P6 刀6）。
        pub fn per_frame(&self) -> bool {
            self.cfg.per_frame
        }

        /// P6 面板最小重扫间隔（max_hz 帧率帽的周期形态——480Hz 仿真）。
        pub fn min_interval_ns(&self) -> u64 {
            period_of(FREERANGE_MAX_HZ)
        }

        /// P6 自刷新重扫上界（min_hz 周期——1Hz 仿真；超窗无新帧 = 重扫
        /// 上一帧，停帧语义（A7）不在本面——无人观察时零拍点）。
        pub fn max_interval_ns(&self) -> u64 {
            period_of(FREERANGE_MIN_HZ)
        }

        /// P6 扫描点裁决（§7-P6 纯函数）：`max(上一扫描开始 + min_interval,
        /// target)`——帧就绪即上墙，仅让面板帧率帽。target=0（无目标域）
        /// = 尽快语义退化 `last + min_interval`。
        pub fn scan_point(&self, last_scan_ns: u64, target_ns: u64) -> u64 {
            let floor = last_scan_ns.saturating_add(self.min_interval_ns());
            floor.max(target_ns)
        }

        /// 当前全点序（P2/P6 latch 在拍点取归属 seq 用——含被故障旋钮
        /// 丢弃的点：丢弃点拍在、事件未投递，帧仍归属它）。
        pub fn seq_now(&self) -> u64 {
            self.seq
        }

        /// FREE 族追赶在席（DET 关——见 MODE_DETERMINISTIC 注）。
        pub fn catchup(&self) -> bool {
            self.cfg.mode != MODE_DETERMINISTIC
        }

        /// 切档请求（§5 控制面，受理排 2 帧生效窗）：Tiered = 档位表成员
        /// 制（class 过滤归刀3 仲裁器）；FreeRange = 钳制量化。零档/同档/
        /// 表外/在途更慢或同速 → None（拒）；**在途更快放行抢占**（换目标
        /// 重开 2 帧窗）——输入 boost 必须拉回正要睡的面板：per_frame 空闲
        /// 态生效窗按 1Hz 重扫走 ~2s，run7 k6-p6f arb1 实证 boost 撞未落地
        /// 的 LFC 降档在途窗被吞（触摸抢占语义——更慢不得反过来打断升路）。
        pub fn request_refresh(&mut self, hz: u16) -> Option<()> {
            if hz == 0 {
                return None;
            }
            let (hz, p) = match self.cfg.model {
                RATE_MODEL_FREERANGE => {
                    let q = freerange_quantize(hz);
                    (q, period_of(q))
                }
                _ => resolve(hz)?,
            };
            if p == 0 {
                return None;
            }
            if let Some((_, pending_p, _)) = self.pending {
                if p >= pending_p {
                    return None;
                }
            }
            if p == self.period_ns {
                return None;
            }
            self.pending = Some((hz, p, 2));
            Some(())
        }

        /// 拍点裁决：`free_nominal_ns` = FREE 态标称点（调用方调度面值；
        /// DET 态忽略）。返回 None = 本拍丢弃（seq 仍消耗）。
        pub fn on_point(&mut self, free_nominal_ns: u64) -> Option<super::VblankTimingRep> {
            self.seq = self.seq.wrapping_add(1);
            // ts：FREE = 标称点直通；DET = 虚拟钟精确步进。
            let nominal = if self.cfg.mode == MODE_DETERMINISTIC {
                let t = self.det_base_ns.wrapping_add(self.det_k.wrapping_mul(self.period_ns));
                self.det_k = self.det_k.wrapping_add(1);
                t
            } else {
                free_nominal_ns
            };
            self.emit(nominal, false)
        }

        /// P6 扫描拍点裁决（`on_point` 的逐帧形态——seq/抖动/切档窗/丢帧
        /// 全共用；ts：FREE = 扫描点墙钟直通、DET = 虚拟钟按扫描间隔累进
        /// ——同输入序列（同 target 链）→ 同 ts 序列，T2 回放门 P6 变体）。
        pub fn on_scan(&mut self, wall_scan_ns: u64) -> Option<super::VblankTimingRep> {
            self.seq = self.seq.wrapping_add(1);
            let nominal = if self.cfg.mode == MODE_DETERMINISTIC {
                if self.p6_last_wall_ns != 0 {
                    self.p6_det_elapsed_ns = self
                        .p6_det_elapsed_ns
                        .saturating_add(wall_scan_ns.saturating_sub(self.p6_last_wall_ns));
                }
                self.p6_last_wall_ns = wall_scan_ns;
                self.det_base_ns.wrapping_add(self.p6_det_elapsed_ns)
            } else {
                wall_scan_ns
            };
            self.emit(nominal, true)
        }

        /// 拍点共尾：抖动旋钮 → 切档生效窗 → 丢帧裁决 → 载荷。`p6` = 逐帧
        /// 形态（DET 切档重锚面分岔）。
        fn emit(&mut self, nominal: u64, p6: bool) -> Option<super::VblankTimingRep> {
            // 抖动旋钮：仅扰上报 ts（绘制顺序固定——jitter 抽签恒先于丢帧
            // 抽签，两旋钮并存时序列仍确定）。
            let mut ts = nominal;
            if self.cfg.jitter_ns > 0 {
                let j = self.cfg.jitter_ns as u64;
                let d = (lcg_step(&mut self.rng) % (2 * j + 1)) as i64 - j as i64;
                ts = ts.wrapping_add(d as u64);
            }
            // 切档生效窗：受理后第 2 拍 = 生效拍（ts 按旧钟出、refresh 自
            // 本拍报新周期、DET 钟基重锚）。
            if let Some((hz, p, left)) = self.pending {
                if left <= 1 {
                    self.pending = None;
                    self.hz = hz;
                    self.period_ns = p;
                    self.last_ack = Some(SwitchAck {
                        effective_ns: nominal,
                        new_refresh_ns: p as u32,
                    });
                    if self.cfg.sw_drop > 0 {
                        self.sw_drop_left = self.cfg.sw_drop;
                    }
                    if self.cfg.mode == MODE_DETERMINISTIC {
                        if p6 {
                            // P6 重锚：本拍 ts = 新基（累进清零、墙钟链不断）。
                            self.det_base_ns = nominal;
                            self.p6_det_elapsed_ns = 0;
                        } else {
                            self.det_base_ns = nominal;
                            self.det_k = 1;
                        }
                    }
                } else {
                    self.pending = Some((hz, p, left - 1));
                }
            }
            // 丢帧裁决：切档丢帧优先，常丢帧 LCG 千分比。
            if self.sw_drop_left > 0 {
                self.sw_drop_left -= 1;
                return None;
            }
            if self.cfg.drop_permille > 0
                && lcg_step(&mut self.rng) % 1000 < self.cfg.drop_permille as u64
            {
                return None;
            }
            Some(super::VblankTimingRep {
                seq: self.seq,
                ts_ns: ts,
                refresh_ns: self.period_ns as u32,
            })
        }
    }
}

// ---------- arbiter 域（刀3 §6：票簿 + A 规则裁决——纯核，guifbd 活面接线）----------
//
// 设计照 softpanel 同款：裁决状态机纯核（时钟一律入参注入——host 可测、
// 零墙依赖），活设备面（RPC 钩 / 证据行 / 调度执行）在 guifbd。v1 单客户
// 户端单票槽（§6.4 多客户端合并刀前不动——A2/A4 已按集合语义书写，扩容
// 点 = `Ballot` 槽换切片 + 优先级取 max）。
//
// 规范锚（刷新率体系规范 v1.0 §6.3）：A1 票生命周期（v1 简化 = 单槽替换 +
// 断连清空——「随下一张票失效」在单槽下即替换语义；sticky 的跨内容存活
// 是多内容面，v1 无内容概念）；A2 全局区间 + 无解安全档；A3 候选集（Tiered
// 产品档 ∩ G，测试档仅 DET 测试模式可入；FreeRange 连续区间）；A4 选档；
// A5 迟滞非对称（升 2 拍 / 降驻留 1s，DIM 降一档）；A6 触摸加速（500ms 窗
// 立即升）；A7 静止停帧（票/damage/armed 三条件——guifbd 零唤醒不变量
// 的法定化身）；A9 切档执行（决策 → softpanel 控制面）；A11 平台兜底。
//
// **A11 无票缺省 = boot 档（平台 fw_cfg 表达）而非硬编 60**：`display=@30`
// 是平台明示档，无票时裁决回 boot 档（缺省 boot 即 60——两读一致）；硬编
// 60 会与刀1 的 @30 boot 语义自相残杀。Unspecified 票（无偏好贡献）同路。

pub mod arbiter {
    use super::softpanel::{self, PanelCfg};
    use super::Intent;

    // Intent 判别值（crate 根单源转发 berry-manifest——cast 定值防手抄，
    // 比较位/匹配位两用）。
    const I_UNSPEC: u8 = Intent::Unspecified as u8;
    const I_LATENCY: u8 = Intent::LatencyCritical as u8;
    const I_INTERACTIVE: u8 = Intent::Interactive as u8;
    const I_UTILITY: u8 = Intent::Utility as u8;
    const I_BG: u8 = Intent::Background as u8;

    /// A2 平台底线（Hz）。
    pub const PLATFORM_FLOOR_HZ: u16 = 30;
    /// A5 降档驻留判据窗（ns）。
    pub const T_DOWN_DWELL_NS: u64 = 1_000_000_000;
    /// A6 触摸加速窗（ns）。
    pub const T_TOUCH_BOOST_NS: u64 = 500_000_000;
    /// A5 升档确认拍数（连续 2 拍）。
    pub const UP_CONFIRM_TICKS: u8 = 2;
    /// A6 boost 目标基准档（Hz）。
    pub const BOOST_HZ: u16 = 120;
    /// A2 无解安全档（Hz；Tiered 表内产品档）。
    pub const SAFE_HZ: u16 = 60;

    /// 票面值形（wire `PresentReq::Vote` 五域转递；intent = berry_intent
    /// 判别值 0x00..=0x05）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Ballot {
        pub sticky: bool,
        pub intent: u8,
        pub min_hz: u16,
        pub max_hz: u16,
        pub preferred_hz: u16,
    }

    /// 决策源（证据行与测试判别；Down 提交恒记 Dwell——A5 规则面）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Src {
        Vote,
        Dwell,
        Boost,
        Dim,
        Platform,
        Fallback,
        /// A8 LFC 进入（整数倍重发面——内容节拍裁决的稳态迁移）。
        Lfc,
    }

    /// 裁决输出。guifbd 执行面：`Switch` → `panel.request_refresh(hz)` +
    /// 证据行（在飞切档被拒 → [`Arbiter::on_switch_rejected`] 回滚重裁）；
    /// `StopFrame` / `Resume` → A7 证据行（调度面归 guifbd 零唤醒基线）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Decision {
        Stay,
        Switch { hz: u16, src: Src },
        StopFrame,
        Resume,
    }

    /// 评估时机（A5 判据的拍/事件/时钟三分——升档 streak 只随**拍**积累）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Kind {
        /// 软面板拍点后（streak 递增）。
        Tick,
        /// serve 环时钟醒（只推进降档驻留与 boost 到期——不积 streak）。
        Time,
        /// 状态变更事件（票/脉冲/PM——不积 streak；A6 起臂拍走 BoostArm）。
        Event,
        /// A6 脉冲起臂（立即升——绕过 streak，规范「立即」）。
        BoostArm,
        /// A10 预告升档（密集预告 ∧ 票非背景类——立即升，绕 streak，
        /// 消「升档迟一帧」；目标仍走 target() 链，src 不新设）。
        Hint,
    }

    /// 仲裁器纯核。
    pub struct Arbiter {
        model: u8,
        /// DET 测试模式 → 测试档可入候选（A3——仅测试拓扑）。
        det_test: bool,
        dim: bool,
        /// A11 无票缺省档 = boot 档。
        no_vote_hz: u16,
        cur_hz: u16,
        ballot: Option<Ballot>,
        // —— A5 迟滞 ——
        up_target: Option<u16>,
        up_streak: u8,
        down: Option<(u16, u64)>,
        // —— A6 ——
        boost_until: u64,
        // —— A7 ——
        stopped: bool,
        /// A2 无解记号（证据/遥测代理——读侧消费即清）。
        warn_no_g: bool,
        // —— A8 LFC ——
        /// 资格折算（构造时 = vrr capable ∧ 面板 max ≥ 2×min；per_frame
        /// 形态 LFC 天然内含——调用方 per_frame 面不走 LFC 裁决）。
        lfc_capable: bool,
        /// 当前置倍数（0 = 未激活——证据面读；内容率 ≥ G_min 即复位）。
        lfc_k: u32,
        /// A8 禁用告警记号（整数倍越 G_max / 档空 = 振荡护栏——读即清）。
        warn_lfc_off: bool,
    }

    impl Arbiter {
        /// 由面板配置 + boot 档构造（boot 档 = A11 无票缺省）。
        pub fn new(cfg: &PanelCfg, boot_hz: u16) -> Arbiter {
            // A8 面板能力门：vrr capable ∧ max ≥ 2×min（FreeRange 仿真
            // 区间 1..=480、Tiered 仿真档表极值——两 sim 面均过倍率门，
            // 真门 = vrr 位）。
            let (pmin, pmax) = if cfg.model == super::RATE_MODEL_FREERANGE {
                (softpanel::FREERANGE_MIN_HZ, softpanel::FREERANGE_MAX_HZ)
            } else {
                let mut lo = u16::MAX;
                let mut hi = 0u16;
                for t in softpanel::TIERS.iter() {
                    lo = lo.min(t.hz);
                    hi = hi.max(t.hz);
                }
                (lo, hi)
            };
            Arbiter {
                model: cfg.model,
                det_test: cfg.mode == softpanel::MODE_DETERMINISTIC,
                dim: false,
                no_vote_hz: boot_hz,
                cur_hz: boot_hz,
                ballot: None,
                up_target: None,
                up_streak: 0,
                down: None,
                boost_until: 0,
                stopped: false,
                warn_no_g: false,
                lfc_capable: cfg.vrr == super::PANEL_VRR_CAPABLE && pmax >= 2 * pmin,
                lfc_k: 0,
                warn_lfc_off: false,
            }
        }

        pub fn hz(&self) -> u16 {
            self.cur_hz
        }

        pub fn stopped(&self) -> bool {
            self.stopped
        }

        /// A6 boost 窗内在席（guifbd 静默汇拢面——boost 窗内禁停帧）。
        pub fn boost_active(&self, now: u64) -> bool {
            self.boost_until != 0 && now < self.boost_until
        }

        /// A2 无解告警消费（读即清——证据行单发）。
        pub fn take_warn_no_g(&mut self) -> bool {
            let w = self.warn_no_g;
            self.warn_no_g = false;
            w
        }

        /// A8 资格透出（PANEL_CAPS 宣告与裁决门同源——guifbd 宣告面用）。
        pub fn lfc_capable(&self) -> bool {
            self.lfc_capable
        }

        /// A8 当前置倍数（0 = 未激活——`PANELARB lfc` 证据行载荷面）。
        pub fn lfc_k(&self) -> u32 {
            self.lfc_k
        }

        /// A8 禁用告警消费（读即清——禁用条件到席即振荡护栏证据）。
        pub fn take_warn_lfc_off(&mut self) -> bool {
            let w = self.warn_lfc_off;
            self.warn_lfc_off = false;
            w
        }

        /// A8：内容节拍更新（guifbd 在呈现面量得的稳态内容 hz；0 = 无内
        /// 容/未知——LFC 复位不迁移）。内容率 ≥ G_min → LFC 无关（k=0 复
        /// 位，正常仲裁接管）；跌破 G_min → 面板率 = k×内容率（k =
        /// ceil(G_min/内容率)）：Tiered 对齐 ≥ target 的最小产品档（测试档
        /// 禁入——LFC 是稳态不是测试拓扑）、FreeRange 区间直取（粒度对齐
        /// 归 softpanel 受理面）。k×内容率越 G_max / 档空 → 禁用 + 告警记
        /// 号（A8 振荡护栏原文）。立即执行（内容节拍是慢变量，非拍积
        /// streak 面——进入/退出都是稳态迁移）。
        pub fn on_content_cadence(&mut self, content_hz: u16) -> Decision {
            let (gmin, gmax, solvable) = self.g_range();
            if !self.lfc_capable || !solvable || content_hz == 0 || content_hz >= gmin {
                self.lfc_k = 0;
                return Decision::Stay;
            }
            let k = (gmin + content_hz - 1) / content_hz; // ceil ≥ 1
            let target = k as u32 * content_hz as u32; // u32 域防 16 位积溢
            if target > gmax as u32 {
                self.warn_lfc_off = true;
                self.lfc_k = 0;
                return Decision::Stay;
            }
            let target = target as u16;
            let final_hz = if self.model == super::RATE_MODEL_FREERANGE {
                target
            } else {
                let mut best: Option<u16> = None;
                for t in softpanel::TIERS.iter() {
                    if t.class != super::PANEL_CLASS_PRODUCT {
                        continue;
                    }
                    if t.hz < gmin || t.hz > gmax {
                        continue;
                    }
                    if t.hz >= target && best.map_or(true, |b: u16| t.hz < b) {
                        best = Some(t.hz);
                    }
                }
                match best {
                    Some(hz) => hz,
                    None => {
                        self.warn_lfc_off = true;
                        self.lfc_k = 0;
                        return Decision::Stay;
                    }
                }
            };
            self.lfc_k = k as u32;
            if final_hz == self.cur_hz {
                return Decision::Stay;
            }
            self.commit(final_hz, Src::Lfc)
        }

        /// A1：新票入槽（单槽替换）+ 即时裁决。
        pub fn on_vote(&mut self, now: u64, quiet: bool, b: Ballot) -> Decision {
            self.ballot = Some(b);
            self.evaluate(now, quiet, Kind::Event)
        }

        /// A1：断连撤票（sticky 同撤——断连即撤无差别）+ 重仲裁。
        pub fn on_disconnect(&mut self, now: u64, quiet: bool) -> Decision {
            self.ballot = None;
            self.evaluate(now, quiet, Kind::Event)
        }

        /// A5 DIM 偏好：POWER_DIM 置位 = 整体偏好降一档。
        pub fn on_power_mode(&mut self, now: u64, quiet: bool, mode: u8) -> Decision {
            self.dim = mode == super::POWER_DIM;
            self.evaluate(now, quiet, Kind::Event)
        }

        /// A6：输入脉冲——起臂 boost 窗 + 立即裁决（BoostArm 绕 streak）。
        /// 脉冲即活动：停帧态先破停（Resume 证据由调用方经 [`Arbiter::on_activity`]
        /// 先行取走——guifbd 接线序 = on_activity → on_input_pulse）。
        pub fn on_input_pulse(&mut self, now: u64, quiet: bool) -> Decision {
            self.stopped = false;
            self.boost_until = now + T_TOUCH_BOOST_NS;
            self.evaluate(now, quiet, Kind::BoostArm)
        }

        /// A10：呈现预告（SUBMIT `rate_hint`/`target_present_ns` 的
        /// notifyExpectedPresent 面）——预告密集 ∧ 票 Intent 非背景类 →
        /// 升档决策立即完成（Kind::Hint 绕 streak，消「升档迟一帧」）。
        /// 密集判定归调用方（两域任一指示节拍快于现档）。资格 = 偏好承载
        /// 三档（LatencyCritical/Interactive/Utility——§6.2 偏好档 > 30 的
        /// 非背景类；Maintenance/Background/Unspecified 无高帧偏好，预告
        /// 不构成升档意图，且过门会经 evaluate 起降档驻留锚——预告面禁
        /// 降档副作用，结构排除）。不合格 → Stay（零裁决副作用——damage
        /// 活动面归 [`Arbiter::on_activity`]，停帧资格不因预告翻转）；
        /// 资格三档结构性排除停帧路（A7 资格 = 无票 ∨ 背景类），evaluate
        /// 的 StopFrame 分支不可达。
        pub fn on_present_hint(&mut self, now: u64, quiet: bool, dense: bool) -> Decision {
            let qualifying = dense
                && match self.ballot {
                    Some(b) => {
                        b.intent == I_LATENCY || b.intent == I_INTERACTIVE || b.intent == I_UTILITY
                    }
                    None => false,
                };
            if !qualifying {
                return Decision::Stay;
            }
            self.evaluate(now, quiet, Kind::Hint)
        }

        /// A7：活动破除（damage / armed / parked——停帧→恢复；运行期活动
        /// 无裁决输出）。
        pub fn on_activity(&mut self, now: u64) -> Decision {
            if self.stopped {
                self.stopped = false;
                return Decision::Resume;
            }
            let _ = now;
            Decision::Stay
        }

        /// 拍点后评估（A5 streak 积累 + 停帧入判）。
        pub fn on_tick(&mut self, now: u64, quiet: bool) -> Decision {
            self.evaluate(now, quiet, Kind::Tick)
        }

        /// serve 环时钟醒（降档驻留 / boost 到期推进——Kind::Time）。
        pub fn on_time(&mut self, now: u64, quiet: bool) -> Decision {
            self.evaluate(now, quiet, Kind::Time)
        }

        /// 切档请求被拒（softpanel 在飞窗/同档）后回滚重裁：`actual` = 面板
        /// 现档（下一轮评估按真值重走迟滞——升路 streak 清零重数，降路驻留
        /// 保守侧重计时：commit 已清 down，被拒即等同降档意图重发起点）。
        pub fn on_switch_rejected(&mut self, actual_hz: u16) {
            self.cur_hz = actual_hz;
            self.up_target = None;
            self.up_streak = 0;
        }

        /// 下一醒时刻（serve deadline 的 min 臂候选）：降档驻留到点 / boost
        /// 窗到点（到期回落的降档驻留须重新计时）；停帧态零醒（A7）。
        pub fn next_wake(&self, now: u64) -> Option<u64> {
            if self.stopped {
                return None;
            }
            let mut w = None;
            if let Some((_, t0)) = self.down {
                let d = t0 + T_DOWN_DWELL_NS;
                if d > now {
                    w = Some(d);
                }
            }
            if self.boost_until > now && self.boost_until > w.unwrap_or(0) {
                w = Some(self.boost_until);
            }
            w
        }

        // —— 内部 ——

        /// A7 停帧资格：无票或全票 ∈ {Background, Unspecified} 且静默
        ///（armed/parked/damage 由调用方 quiet 布尔汇拢——damage 破除走
        /// on_activity 的 Resume 面，此判不重复计活动）。
        fn stop_eligible(&self, quiet: bool) -> bool {
            if !quiet {
                return false;
            }
            match self.ballot {
                None => true,
                Some(b) => b.intent == I_BG || b.intent == I_UNSPEC,
            }
        }

        fn evaluate(&mut self, now: u64, quiet: bool, kind: Kind) -> Decision {
            // A7 先于迟滞：停帧即静默（判据缓存清空）。
            if self.stop_eligible(quiet) {
                self.up_target = None;
                self.up_streak = 0;
                self.down = None;
                if !self.stopped {
                    self.stopped = true;
                    return Decision::StopFrame;
                }
                return Decision::Stay;
            }
            if self.stopped {
                // 条件破除（票偏好化 / 活动到）——恢复当前档（档位不动）。
                self.stopped = false;
                // 递归一次走正常裁决（事件本身可能同时携升档判据）。
                return Decision::Resume;
            }
            let (target, src) = self.target(now);
            if src == Src::Fallback {
                self.warn_no_g = true;
            }
            if target == self.cur_hz {
                self.up_target = None;
                self.up_streak = 0;
                self.down = None;
                return Decision::Stay;
            }
            if target > self.cur_hz {
                self.down = None;
                if self.up_target != Some(target) {
                    self.up_target = Some(target);
                    self.up_streak = 0;
                }
                match kind {
                    Kind::BoostArm | Kind::Hint => return self.commit(target, src),
                    Kind::Tick => self.up_streak += 1,
                    Kind::Time | Kind::Event => {}
                }
                if self.up_streak >= UP_CONFIRM_TICKS {
                    return self.commit(target, src);
                }
                return Decision::Stay;
            }
            // 降档：驻留判据（A5——目标变更重计时）。
            self.up_target = None;
            self.up_streak = 0;
            match self.down {
                Some((hz, t0)) if hz == target => {
                    if now.saturating_sub(t0) >= T_DOWN_DWELL_NS {
                        return self.commit(target, Src::Dwell);
                    }
                }
                _ => self.down = Some((target, now)),
            }
            Decision::Stay
        }

        fn commit(&mut self, hz: u16, src: Src) -> Decision {
            self.cur_hz = hz;
            self.up_target = None;
            self.up_streak = 0;
            self.down = None;
            Decision::Switch { hz, src }
        }

        /// 目标档（A2 → A6 → A4 链）。
        fn target(&self, now: u64) -> (u16, Src) {
            let (gmin, gmax, solvable) = self.g_range();
            if !solvable {
                // A2 无解：安全档 60（Tiered 表内产品档；FreeRange 直值）
                // + 告警记号——禁空转不裁决。
                return (self.safe_hz(), Src::Fallback);
            }
            // A6 boost 窗内：min(BOOST_HZ, G_max)，立即语义由 BoostArm 承担。
            if self.boost_active(now) {
                let b = BOOST_HZ.min(gmax);
                return (self.select(b, gmin, gmax), Src::Boost);
            }
            match self.ballot {
                Some(b) if b.intent != I_UNSPEC => {
                    let mut pref = if b.preferred_hz != 0 {
                        b.preferred_hz
                    } else {
                        intent_default(b.intent)
                    };
                    if self.dim {
                        pref = dim_down(pref);
                        let r = self.select(pref, gmin, gmax);
                        return (r, Src::Dim);
                    }
                    (self.select(pref, gmin, gmax), Src::Vote)
                }
                // 无票/Unspecified 票（无偏好贡献）：A11 boot 档过 select——
                // Unspecified 票的 min/max 仍是 A2 约束（约束≠偏好）。
                _ => (self.select(self.no_vote_hz, gmin, gmax), Src::Platform),
            }
        }

        /// A2 全局区间：`[max(票 min ∪ {平台底线}), min(票 max ∪ {面板 max})]`；
        /// 票域 0 = 不设界。返回 (gmin, gmax, 可解)。
        fn g_range(&self) -> (u16, u16, bool) {
            let panel_max = if self.model == super::RATE_MODEL_FREERANGE {
                softpanel::FREERANGE_MAX_HZ
            } else {
                panel_tier_max()
            };
            let mut gmin = PLATFORM_FLOOR_HZ;
            let mut gmax = panel_max;
            if let Some(b) = self.ballot {
                if b.min_hz != 0 {
                    gmin = gmin.max(b.min_hz);
                }
                if b.max_hz != 0 {
                    gmax = gmax.min(b.max_hz);
                }
            }
            (gmin, gmax, gmin <= gmax)
        }

        /// A4 选档（Tiered 查表 / FreeRange 连续钳制）。
        fn select(&self, pref: u16, gmin: u16, gmax: u16) -> u16 {
            if self.model == super::RATE_MODEL_FREERANGE {
                // 连续区间直取 + 越界最近侧钳制（粒度对齐归 softpanel）。
                return pref.clamp(
                    gmin.max(softpanel::FREERANGE_MIN_HZ),
                    gmax.min(softpanel::FREERANGE_MAX_HZ),
                );
            }
            // Tiered：候选 = 档位表内合法族 ∩ G（测试档仅 DET 测试模式）。
            let mut best: Option<u16> = None; // pref 命中或 ≥ pref 最小
            let mut top: Option<u16> = None; // 候选最高档
            for t in softpanel::TIERS.iter() {
                if !self.tier_admissible(t) {
                    continue;
                }
                if t.hz < gmin || t.hz > gmax {
                    continue;
                }
                top = Some(top.map_or(t.hz, |m: u16| m.max(t.hz)));
                if t.hz >= pref && best.map_or(true, |b: u16| t.hz < b) {
                    best = Some(t.hz);
                }
            }
            match best.or(top) {
                Some(hz) => hz,
                // 候选空（G 排空合法族）→ A2 安全档路径兜底（理论经 solvable
                // 预过滤后仍可达：G 全落在测试档带上且非 DET）。
                None => self.safe_hz(),
            }
        }

        fn tier_admissible(&self, t: &softpanel::TierDef) -> bool {
            match t.class {
                super::PANEL_CLASS_PRODUCT => true,
                super::PANEL_CLASS_TEST => self.det_test,
                _ => false, // LTPO 档 v1 不入候选（AOD 组合拳归刀7）
            }
        }

        fn safe_hz(&self) -> u16 {
            if self.model == super::RATE_MODEL_FREERANGE {
                return SAFE_HZ.clamp(
                    softpanel::FREERANGE_MIN_HZ,
                    softpanel::FREERANGE_MAX_HZ,
                );
            }
            SAFE_HZ
        }
    }

    /// Tiered 面板宣告上限（档位表最大 hz——含测试族；A2 的「面板 max_hz」）。
    fn panel_tier_max() -> u16 {
        let mut m = 0u16;
        for t in softpanel::TIERS.iter() {
            m = m.max(t.hz);
        }
        m
    }

    /// §6.2 法定表：preferred_hz 缺省（0）时按 Intent 映射。
    fn intent_default(intent: u8) -> u16 {
        match intent {
            I_LATENCY => panel_tier_max(),
            I_INTERACTIVE => 120,
            I_UTILITY => SAFE_HZ,
            // Maintenance / Background → 30（Background 另有停帧候选路 A7）。
            _ => PLATFORM_FLOOR_HZ,
        }
    }

    /// A5 DIM 降一档：偏好降到严格次低产品档（Tiered 阶梯 notion 对
    /// FreeRange 同样适用——「档」的共享语义）；已最低则不动。
    fn dim_down(pref: u16) -> u16 {
        let mut below = 0u16;
        for t in softpanel::TIERS.iter() {
            if t.class == super::PANEL_CLASS_PRODUCT && t.hz < pref && t.hz > below {
                below = t.hz;
            }
        }
        if below != 0 {
            below
        } else {
            pref
        }
    }
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

// ---------- 波2 输入焦点协议编解码（add-only——见 OP_WM_FOCUS 块注） ----------

/// WM_FOCUS 请求帧长（`[op][win u16][r16]`）。
pub const WM_FOCUS_REQ_LEN: usize = 8;
/// WM_FOCUS 请求帧帽（= 请求帧长——帽一致性族同形）。
pub const WM_FOCUS_REQ_MAX: usize = WM_FOCUS_REQ_LEN;
/// WM_BACK 请求帧长（`[op][r24]`）。
pub const WM_BACK_REQ_LEN: usize = 8;
/// WM_BACK 请求帧帽。
pub const WM_BACK_REQ_MAX: usize = WM_BACK_REQ_LEN;
/// INPUT_INJECT 帧长基（`[op][dev_kind]` + 载荷 ≤ 24 字节 + 尾 r16）。
pub const INPUT_INJECT_REQ_MAX: usize = 28;

/// 编码 WM_FOCUS 请求：`win` = 目标窗 id（0 = 让出焦点——本窗失焦不动
/// 他窗）。buf 不足 = 调用方错 assert（域内既有编码器同律）。
pub fn encode_wm_focus_req(out: &mut [u8], win: u16) -> usize {
    assert!(out.len() >= WM_FOCUS_REQ_LEN, "guiproto: wm focus buf 不足");
    out[0] = OP_WM_FOCUS;
    out[1..3].copy_from_slice(&win.to_le_bytes());
    out[3] = 0;
    out[4..8].copy_from_slice(&[0; 4]);
    WM_FOCUS_REQ_LEN
}

/// 解析 WM_FOCUS 请求：恰 8B + op → win（任意 u16 合法）。
pub fn parse_wm_focus_req(buf: &[u8]) -> Option<u16> {
    if buf.len() != WM_FOCUS_REQ_LEN || buf[0] != OP_WM_FOCUS {
        return None;
    }
    Some(u16::from_le_bytes([buf[1], buf[2]]))
}

/// 编码 WM_BACK 请求（无参——导航键触发，wmd 查表路由到焦点窗）。
pub fn encode_wm_back_req(out: &mut [u8]) -> usize {
    assert!(out.len() >= WM_BACK_REQ_LEN, "guiproto: wm back buf 不足");
    out[0] = OP_WM_BACK;
    out[1..8].fill(0);
    WM_BACK_REQ_LEN
}

/// 解析 WM_BACK 请求：恰 8B + op → true。
pub fn parse_wm_back_req(buf: &[u8]) -> Option<()> {
    if buf.len() != WM_BACK_REQ_LEN || buf[0] != OP_WM_BACK {
        return None;
    }
    Some(())
}

// ---------- 波3 wm 事件通道编解码（add-only——见 OP_WM_SUBSCRIBE 块注） ----------

/// WM_SUBSCRIBE 请求帧长（`[op][r][r16]`——TABLE_GET 同形）。
pub const WM_SUBSCRIBE_REQ_LEN: usize = 4;
/// WM_EVT_BACK 帧长（`[kind][wid u16][r]`）。
pub const WM_EVT_BACK_LEN: usize = 4;
/// WM_EVT_TABLE 帧长（`[kind][version u32]`）。
pub const WM_EVT_TABLE_LEN: usize = 5;
/// WM_EVT_BACK_ACK 帧长（`[kind]`——1B 回程帧）。
pub const WM_BACK_ACK_LEN: usize = 1;

/// 编码 WM_SUBSCRIBE 请求（无域）。buf 不足 = 调用方错 assert。
pub fn encode_wm_subscribe_req(out: &mut [u8]) -> usize {
    assert!(out.len() >= WM_SUBSCRIBE_REQ_LEN, "guiproto: wm subscribe buf 不足");
    out[0] = OP_WM_SUBSCRIBE;
    out[1] = 0;
    out[2] = 0;
    out[3] = 0;
    WM_SUBSCRIBE_REQ_LEN
}

/// 解析 WM_SUBSCRIBE 请求：恰 4B + op（保留位解析侧恒不校验——前向兼容
/// 锚同 GETINFO/TABLE_GET 族）。
pub fn parse_wm_subscribe_req(buf: &[u8]) -> Option<()> {
    if buf.len() != WM_SUBSCRIBE_REQ_LEN || buf[0] != OP_WM_SUBSCRIBE {
        return None;
    }
    Some(())
}

/// 编码 WM_EVT_BACK 事件帧：`wid` = 返回键路由目标窗。尾 r 解析侧恒不
/// 校验（前向兼容锚）。
pub fn encode_wm_evt_back(out: &mut [u8], wid: u16) -> usize {
    assert!(out.len() >= WM_EVT_BACK_LEN, "guiproto: wm evt back buf 不足");
    out[0] = WM_EVT_BACK;
    out[1..3].copy_from_slice(&wid.to_le_bytes());
    out[3] = 0;
    WM_EVT_BACK_LEN
}

/// 解析 WM_EVT_BACK 事件帧：恰 4B + kind → wid（任意 u16——存在性归
/// wmd 策略面）。
pub fn parse_wm_evt_back(buf: &[u8]) -> Option<u16> {
    if buf.len() != WM_EVT_BACK_LEN || buf[0] != WM_EVT_BACK {
        return None;
    }
    Some(u16::from_le_bytes([buf[1], buf[2]]))
}

/// 编码 WM_EVT_TABLE 事件帧：`version` = 表版本（u32 任意值——「恒 ≥1」
/// 是服务端不变量非 wire 执法，Q3 同款）。
pub fn encode_wm_evt_table(out: &mut [u8], version: u32) -> usize {
    assert!(out.len() >= WM_EVT_TABLE_LEN, "guiproto: wm evt table buf 不足");
    out[0] = WM_EVT_TABLE;
    out[1..5].copy_from_slice(&version.to_le_bytes());
    WM_EVT_TABLE_LEN
}

/// 解析 WM_EVT_TABLE 事件帧：恰 5B + kind → version。
pub fn parse_wm_evt_table(buf: &[u8]) -> Option<u32> {
    if buf.len() != WM_EVT_TABLE_LEN || buf[0] != WM_EVT_TABLE {
        return None;
    }
    Some(u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]))
}

/// 编码 WM_EVT_BACK_ACK 回程帧（1B 无域）。
pub fn encode_wm_back_ack(out: &mut [u8]) -> usize {
    assert!(out.len() >= WM_BACK_ACK_LEN, "guiproto: wm back ack buf 不足");
    out[0] = WM_EVT_BACK_ACK;
    WM_BACK_ACK_LEN
}

/// 解析 WM_EVT_BACK_ACK 回程帧：恰 1B + kind。
pub fn parse_wm_back_ack(buf: &[u8]) -> Option<()> {
    if buf.len() != WM_BACK_ACK_LEN || buf[0] != WM_EVT_BACK_ACK {
        return None;
    }
    Some(())
}

/// 事件通道帧统一判别（消费侧单入口——属主端收 BACK/TABLE、wmd 端收
/// BACK_ACK 共用；错帧型 None 交调用方策略）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WmEvt {
    /// 返回键投递（携带目标窗 id）。
    Back(u16),
    /// 表变更通知（携带表 version）。
    Table(u32),
    /// 返回应答（1B 无域）。
    BackAck,
}

/// 解析事件通道帧 → [`WmEvt`]（帧型互斥判别面——三个单帧解析器的收口）。
pub fn parse_wm_evt(buf: &[u8]) -> Option<WmEvt> {
    match buf.first() {
        Some(&WM_EVT_BACK) => parse_wm_evt_back(buf).map(WmEvt::Back),
        Some(&WM_EVT_TABLE) => parse_wm_evt_table(buf).map(WmEvt::Table),
        Some(&WM_EVT_BACK_ACK) => parse_wm_back_ack(buf).map(|_| WmEvt::BackAck),
        _ => None,
    }
}

// ---------- per-app wm 通道编解码（K1——add-only，见 OP_WM_ATTACH 块注） ----------

/// WM_ATTACH 请求帧长（恰 `[op]` 1B——随行恰 1 handle 的执法在 wmd
/// on_wm 分发层，不在本 codec）。
pub const WM_ATTACH_REQ_LEN: usize = 1;
/// WM_DETACH 请求帧长（`[op][srv_handle u32 LE]`）。
pub const WM_DETACH_REQ_LEN: usize = 5;

/// 编码 WM_ATTACH 请求（无域）。buf 不足 = 调用方错 assert。
pub fn encode_wm_attach_req(out: &mut [u8]) -> usize {
    assert!(out.len() >= WM_ATTACH_REQ_LEN, "guiproto: wm attach buf 不足");
    out[0] = OP_WM_ATTACH;
    WM_ATTACH_REQ_LEN
}

/// 解析 WM_ATTACH 请求：恰 1B + op（随行面归分发层执法）。
pub fn parse_wm_attach_req(buf: &[u8]) -> Option<()> {
    if buf.len() != WM_ATTACH_REQ_LEN || buf[0] != OP_WM_ATTACH {
        return None;
    }
    Some(())
}

/// 编码 WM_DETACH 请求：`srv_handle` = 寻址值（任意 u32——存在性归 wmd
/// 策略面）。
pub fn encode_wm_detach_req(out: &mut [u8], srv_handle: u32) -> usize {
    assert!(out.len() >= WM_DETACH_REQ_LEN, "guiproto: wm detach buf 不足");
    out[0] = OP_WM_DETACH;
    out[1..5].copy_from_slice(&srv_handle.to_le_bytes());
    WM_DETACH_REQ_LEN
}

/// 解析 WM_DETACH 请求：恰 5B + op → srv_handle。
pub fn parse_wm_detach_req(buf: &[u8]) -> Option<u32> {
    if buf.len() != WM_DETACH_REQ_LEN || buf[0] != OP_WM_DETACH {
        return None;
    }
    Some(u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]))
}

/// 编码 INPUT_INJECT 请求：`dev_kind` ∈ {DEV_POINTER, DEV_KEYBOARD,
/// DEV_TEXT}；`payload` ≤ 24B（键盘 = 1B key code；文本 = ASCII 串）。
/// 野 dev_kind / 载荷越帽 / buf 不足 → None（调用方可构造面——assert
/// 面仅 buf 不足）。
pub fn encode_input_inject_req(
    out: &mut [u8],
    dev_kind: u8,
    payload: &[u8],
) -> Option<usize> {
    if payload.len() > 24 {
        return None;
    }
    let total = 2 + payload.len() + 2;
    if out.len() < total {
        return None;
    }
    out[0] = OP_INPUT_INJECT;
    out[1] = dev_kind;
    out[2..2 + payload.len()].copy_from_slice(payload);
    out[total - 2..total].fill(0);
    Some(total)
}

/// 解析 INPUT_INJECT 请求：op + dev_kind ∈ 三值域 → (dev_kind, payload)。
pub fn parse_input_inject_req<'a>(buf: &'a [u8]) -> Option<(u8, &'a [u8])> {
    if buf.len() < 4 || buf[0] != OP_INPUT_INJECT {
        return None;
    }
    let dk = buf[1];
    if dk > DEV_TEXT {
        return None;
    }
    Some((dk, &buf[2..buf.len() - 2]))
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
        // 域版本钉板（hdi v3 = 刀2 OP_VBLANK_TIMING + PANEL_CAPS v2 stage；
        // present v2 = 刀4 OP_FRAME_TIMING）。
        assert_eq!((VER_PRESENT, VER_INPUT, VER_HDI), (2, 1, 3));
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
        // 未知 op（0x1F——远离 present 1..=8 与 input 0x20+ 两域分配）。
        assert_eq!(parse_present_req(&[0x1F, 0, 0, 0]), None);
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
    fn frame_timing_wire_roundtrip_golden_and_rejects() {
        // 0x08 8B 请求面（与 FRAME_CALLBACK 同形）。
        assert_eq!(OP_FRAME_TIMING, 8);
        assert_eq!(VER_PRESENT, 2, "present 域 v2 = 刀4 OP_FRAME_TIMING");
        let mut buf = [0u8; 8];
        let n = encode_present_req(&mut buf, &PresentReq::FrameTiming { frame_token: 7 }).unwrap();
        assert_eq!(n, 8);
        assert_eq!(&buf[..8], &[OP_FRAME_TIMING, 0, 0, 0, 7, 0, 0, 0]);
        assert_eq!(parse_present_req(&buf[..8]), Some(PresentReq::FrameTiming { frame_token: 7 }));
        assert_eq!(parse_present_req(&buf[..7]), None, "截断拒");
        let mut trail = [0u8; 9];
        trail[..8].copy_from_slice(&buf);
        assert_eq!(parse_present_req(&trail), None, "尾随拒");
        assert_eq!(encode_present_req(&mut buf[..7], &PresentReq::FrameTiming { frame_token: 7 }), None);
        // 28B 应答黄金字节钉板：`[st=0][flags][r16][token][presented][seq]
        // [refresh]`——LE 低位在前。
        let rep = FrameTimingRep {
            frame_token: 0xA1B2_C3D4,
            presented_ns: 0x1122_3344_5566_7788,
            seq: 0xFEDC_BA98_7654_3210,
            refresh_ns: 8_333_333,
            flags: FRAME_TIMING_F_PRESENTED | FRAME_TIMING_F_FULL,
        };
        let mut out = [0u8; 28];
        let n = encode_frame_timing_rep(&mut out, &rep);
        assert_eq!(n, 28);
        let want: [u8; 28] = [
            0, 0x03, 0, 0, //
            0xD4, 0xC3, 0xB2, 0xA1, //
            0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, //
            0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, //
            0x15, 0x28, 0x7F, 0x00,
        ];
        assert_eq!(&out[..n], &want[..]);
        assert_eq!(parse_frame_timing_rep(&out[..n]), Some(rep));
        // 顶格值 + flags=0 合法（无位集——保留给 DISCARDED 语义刀）。
        let rep2 = FrameTimingRep {
            frame_token: u32::MAX,
            presented_ns: u64::MAX,
            seq: u64::MAX,
            refresh_ns: u32::MAX,
            flags: 0,
        };
        encode_frame_timing_rep(&mut out, &rep2);
        assert_eq!(parse_frame_timing_rep(&out[..28]), Some(rep2));
        // 截断每切点 / 尾随 / st 坏 / encode 野 flags 位 assert 面。
        encode_frame_timing_rep(&mut out, &rep);
        for cut in 0..28 {
            assert_eq!(parse_frame_timing_rep(&out[..cut]), None, "截断 @{cut} 应拒");
        }
        let mut t = [0u8; 29];
        t[..28].copy_from_slice(&out);
        assert_eq!(parse_frame_timing_rep(&t), None); // 29B 拒
        let mut bad = out;
        bad[0] = ST_BAD;
        assert_eq!(parse_frame_timing_rep(&bad), None);
        // 未知 flags 位：parse 前向兼容不拒（v2 加位不改旧读端）。
        let mut fut = out;
        fut[1] = 0x80;
        assert_eq!(parse_frame_timing_rep(&fut).map(|r| r.flags), Some(0x80));
    }

    #[test]
    #[should_panic(expected = "guiproto")]
    fn frame_timing_encode_wild_flags_panics() {
        let rep = FrameTimingRep {
            frame_token: 1,
            presented_ns: 2,
            seq: 3,
            refresh_ns: 4,
            flags: 0x80,
        };
        let mut sink = [0u8; 28];
        let _ = encode_frame_timing_rep(&mut sink, &rep);
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

    #[test]
    fn panel_caps_req_domain() {
        let mut buf = [0u8; 8];
        let n = encode_panel_caps_req(&mut buf);
        assert_eq!(n, 4);
        assert_eq!(parse_panel_caps_req(&buf[..n]), Some(()));
        // 截断 / 尾随 / 错 op。
        assert_eq!(parse_panel_caps_req(&buf[..3]), None);
        assert_eq!(parse_panel_caps_req(&[OP_PANEL_CAPS, 0, 0, 0, 0]), None);
        assert_eq!(parse_panel_caps_req(&[OP_COMMIT, 0, 0, 0]), None);
    }

    #[test]
    fn panel_caps_rep_roundtrip_and_rejects() {
        // Tiered 全表 9 档（规范 §3.1——period_ns = round(1e9/hz) 单源值；
        // stage 自 softpanel::TIERS 单源镜像——v2 应答形）。
        let src = [
            (30u16, PANEL_CLASS_PRODUCT, softpanel::STAGE_V1, 33_333_333u32),
            (60, PANEL_CLASS_PRODUCT, softpanel::STAGE_V1, 16_666_667),
            (120, PANEL_CLASS_PRODUCT, softpanel::STAGE_V1, 8_333_333),
            (240, PANEL_CLASS_PRODUCT, softpanel::STAGE_V2, 4_166_667),
            (480, PANEL_CLASS_PRODUCT, softpanel::STAGE_V2, 2_083_333),
            (165, PANEL_CLASS_TEST, softpanel::STAGE_V2, 6_060_606),
            (180, PANEL_CLASS_TEST, softpanel::STAGE_V2, 5_555_556),
            (1, PANEL_CLASS_LTPO, softpanel::STAGE_V3, 1_000_000_000),
            (10, PANEL_CLASS_LTPO, softpanel::STAGE_V3, 100_000_000),
        ];
        let mut tiers = [PanelTier { hz: 0, class: 0, stage: 0, period_ns: 0 }; PANEL_TIERS_MAX];
        for (i, &(hz, class, stage, p)) in src.iter().enumerate() {
            tiers[i] = PanelTier { hz, class, stage, period_ns: p };
        }
        let rep = PanelCapsRep {
            version: 2,
            rate_model: RATE_MODEL_TIERED,
            flags: 0,
            min_hz: 1,
            max_hz: 480,
            granularity_hz: 0,
            vrr: PANEL_VRR_NONE,
            current_hz: 30,
            current_period_ns: 33_333_333,
            current_phase_ns: 0,
            tiers,
            n_tiers: src.len() as u8,
        };
        let mut buf = [0u8; PANEL_CAPS_REP_MAX];
        let n = encode_panel_caps_rep(&mut buf, &rep);
        assert_eq!(n, PANEL_CAPS_REP_HEAD + PANEL_TIER_LEN * 9);
        assert_eq!(parse_panel_caps_rep(&buf[..n]), Some(rep));
        // 档位行第 4 字节 = stage（v1 此位恒 0，v2 起在册）。
        assert_eq!(buf[PANEL_CAPS_REP_HEAD + 3], softpanel::STAGE_V1);
        // FreeRange 形（空档位表 = 头恰 32B；per_frame + vrr capable；
        // tiers 槽清零——n_tiers 外的槽位解析面恒零，比对同形）。
        let mut fr = rep;
        fr.rate_model = RATE_MODEL_FREERANGE;
        fr.flags = PANEL_FLAG_PER_FRAME | PANEL_FLAG_SELF_REFRESH;
        fr.granularity_hz = 1;
        fr.vrr = PANEL_VRR_CAPABLE;
        fr.n_tiers = 0;
        fr.tiers = [PanelTier { hz: 0, class: 0, stage: 0, period_ns: 0 }; PANEL_TIERS_MAX];
        let nf = encode_panel_caps_rep(&mut buf, &fr);
        assert_eq!(nf, PANEL_CAPS_REP_HEAD);
        assert_eq!(parse_panel_caps_rep(&buf[..nf]), Some(fr));
        // st 坏 / n 与长度不符 / n 越帽 / 截断每切点 / 尾随。
        let mut bad = [0u8; PANEL_CAPS_REP_MAX + 1];
        bad[..n].copy_from_slice(&buf[..n]);
        bad[0] = ST_NOENT;
        assert_eq!(parse_panel_caps_rep(&bad[..n]), None);
        bad[0] = ST_OK;
        bad[4] = 8; // n=8 ≠ 实际 9 → 长度错拒
        assert_eq!(parse_panel_caps_rep(&bad[..n]), None);
        bad[4] = PANEL_TIERS_MAX as u8 + 1; // 越帽（长度同时不符双拒）
        assert_eq!(parse_panel_caps_rep(&bad[..PANEL_CAPS_REP_MAX]), None);
        bad[4] = 9;
        for cut in 0..n {
            assert_eq!(parse_panel_caps_rep(&bad[..cut]), None, "截断 @{cut} 应拒");
        }
        assert_eq!(parse_panel_caps_rep(&bad[..n + 1]), None);
    }

    #[test]
    fn vblank_timing_req_rep_codecs() {
        // 请求恰 4B 无参（VBLANK_WAIT 同形、op 位别）。
        let mut buf = [0u8; 32];
        let n = encode_vblank_timing_req(&mut buf);
        assert_eq!(n, 4);
        assert_eq!(&buf[..n], &[OP_VBLANK_TIMING, 0, 0, 0][..]);
        assert_eq!(parse_vblank_timing_req(&buf[..n]), Some(()));
        assert_eq!(parse_vblank_timing_req(&buf[..3]), None);
        assert_eq!(parse_vblank_timing_req(&[OP_VBLANK_TIMING, 0, 0, 0, 0]), None);
        assert_eq!(parse_vblank_timing_req(&[OP_VBLANK_WAIT, 0, 0, 0]), None);
        // 应答 24B 三元组往返 + 域取相异值（C-2a 钉板要钉全）。
        let rep = VblankTimingRep { seq: 0x0102_0304_0506_0708, ts_ns: 0x1111_2222_3333_4444, refresh_ns: 8_333_333 };
        let n = encode_vblank_timing_rep(&mut buf, &rep);
        assert_eq!(n, 24);
        assert_eq!(parse_vblank_timing_rep(&buf[..n]), Some(rep));
        // 16B GOLDEN 面不混帧：VBLANK_WAIT 解析拒 24B、TIMING 解析拒 16B。
        let mut vb = [0u8; VBLANK_REP_LEN];
        let nvb = encode_vblank_rep(&mut vb, &VblankRep { seq: 7, ts_ns: 99 });
        assert_eq!(parse_vblank_timing_rep(&vb[..nvb]), None);
        assert_eq!(parse_vblank_rep(&buf[..n]), None);
        // st 坏 / 截断每切点 / 尾随。
        assert_eq!(parse_vblank_timing_rep(&{ let mut b = buf; b[0] = ST_NOENT; b }[..n]), None);
        for cut in 0..n {
            assert_eq!(parse_vblank_timing_rep(&buf[..cut]), None, "截断 @{cut} 应拒");
        }
        assert_eq!(parse_vblank_timing_rep(&buf[..n + 1]), None);
    }

    // ===== softpanel 纯核（刷新率体系规范 §5；§10 T2 回放门）=====

    #[test]
    fn softpanel_tier_table_single_source() {
        // §3.1 全表钉板：(hz, period_ns, class, stage)。
        assert_eq!(softpanel::TIERS.len(), 9);
        let expect = [
            (30u16, 33_333_333u64, PANEL_CLASS_PRODUCT, softpanel::STAGE_V1),
            (60, 16_666_667, PANEL_CLASS_PRODUCT, softpanel::STAGE_V1),
            (120, 8_333_333, PANEL_CLASS_PRODUCT, softpanel::STAGE_V1),
            (240, 4_166_667, PANEL_CLASS_PRODUCT, softpanel::STAGE_V2),
            (480, 2_083_333, PANEL_CLASS_PRODUCT, softpanel::STAGE_V2),
            (165, 6_060_606, PANEL_CLASS_TEST, softpanel::STAGE_V2),
            (180, 5_555_556, PANEL_CLASS_TEST, softpanel::STAGE_V2),
            (1, 1_000_000_000, PANEL_CLASS_LTPO, softpanel::STAGE_V3),
            (10, 100_000_000, PANEL_CLASS_LTPO, softpanel::STAGE_V3),
        ];
        for (t, e) in softpanel::TIERS.iter().zip(expect) {
            assert_eq!((t.hz, t.period_ns, t.class, t.stage), e);
        }
    }

    #[test]
    fn softpanel_period_of_and_quantize() {
        assert_eq!(softpanel::resolve(60), Some((60, 16_666_667)));
        assert_eq!(softpanel::resolve(77), None);
        // 表内直取 / 表外半取整单点换算（§3.4 唯一换算点）。
        assert_eq!(softpanel::period_of(30), 33_333_333);
        assert_eq!(softpanel::period_of(77), 12_987_013);
        assert_eq!(softpanel::period_of(50), 20_000_000);
        assert_eq!(softpanel::period_of(1), 1_000_000_000);
        assert_eq!(softpanel::period_of(0), 0);
        // FreeRange 钳制（粒度 1 → 钳制即量化）。
        assert_eq!(softpanel::freerange_quantize(0), 1);
        assert_eq!(softpanel::freerange_quantize(300), 300);
        assert_eq!(softpanel::freerange_quantize(500), 480);
    }

    #[test]
    fn softpanel_domain_pack_roundtrip() {
        // 零域 = 缺省（旧内核 zero 尾 → Tiered/FREE/无故障）。
        assert_eq!(softpanel::PanelCfg::from_domains(0, 0), softpanel::PanelCfg::DEFAULT);
        // 全字段往返 + 位布局钉板（kernel 打包手镜像对拍面）。
        let cfg = softpanel::PanelCfg {
            model: RATE_MODEL_FREERANGE,
            mode: softpanel::MODE_DETERMINISTIC,
            seed: 0xDEAD_BEEF,
            drop_permille: 137,
            jitter_ns: 250_000,
            sw_drop: 2,
            per_frame: true,
            vrr: PANEL_VRR_CAPABLE,
        };
        let (d7, d8) = cfg.pack_domains();
        // 域 7 = [model u8][mode u8][seed u32][per_frame u1][vrr u2][r13]。
        assert_eq!(d7 & 0xFF, RATE_MODEL_FREERANGE as u64);
        assert_eq!((d7 >> 8) & 0xFF, softpanel::MODE_DETERMINISTIC as u64);
        assert_eq!((d7 >> 16) as u32, 0xDEAD_BEEF);
        assert_eq!((d7 >> 48) & 0b111, 0b011); // per_frame=1 + vrr=CAPABLE(01)
        assert_eq!(d7 >> 51, 0);
        // 域 8 = [drop_pm u16][jitter_ns u32][sw_drop u8][r u8]。
        assert_eq!(d8 & 0xFFFF, 137);
        assert_eq!((d8 >> 16) as u32, 250_000);
        assert_eq!((d8 >> 48) & 0xFF, 2);
        assert_eq!(d8 >> 56, 0);
        assert_eq!(softpanel::PanelCfg::from_domains(d7, d8), cfg);
    }

    #[test]
    fn softpanel_free_passthrough_zero_disturbance() {
        // FREE 无故障 = 现状零改动面：ts 直通标称点、seq 自 1 连续、
        // refresh = 当前档周期、追赶在席。
        let mut p = softpanel::SoftPanel::new(softpanel::PanelCfg::DEFAULT, 60);
        assert!(p.catchup());
        let mut nominal = 10_000_000_000u64;
        for i in 1..=3u64 {
            let t = p.on_point(nominal).unwrap();
            assert_eq!((t.seq, t.ts_ns, t.refresh_ns), (i, nominal, 16_666_667));
            nominal += 16_666_667;
        }
    }

    #[test]
    fn softpanel_det_replay() {
        // T2 回放门：DET 同 seed → 位元相等 (seq, ts_ns, refresh_ns)，
        // 与墙钟输入序列完全无关。
        let run = |seed: u32, walls: &[u64]| -> Vec<(u64, u64, u32)> {
            let cfg = softpanel::PanelCfg { mode: softpanel::MODE_DETERMINISTIC, seed, ..softpanel::PanelCfg::DEFAULT };
            let mut p = softpanel::SoftPanel::new(cfg, 60);
            walls
                .iter()
                .map(|&w| {
                    let t = p.on_point(w).unwrap();
                    (t.seq, t.ts_ns, t.refresh_ns)
                })
                .collect()
        };
        let a = run(42, &[0, 5, 999, 123_456]);
        let b = run(42, &[7_777_777, 3, 80, 1_000_000_000]);
        assert_eq!(a, b, "同 seed 异墙钟 → 必位元相等");
        // 序列钉板：t0 = 1e9 + seed·6700417、间距恰一周期。
        assert_eq!(a[0].1, 1_000_000_000u64 + 42 * 6_700_417);
        assert!(a.windows(2).all(|w| w[1].1 - w[0].1 == 16_666_667));
        // 异 seed → 异钟基（非巧合等序）。
        assert_ne!(run(43, &[0])[0].1, a[0].1);
        // DET 关追赶。
        let p = softpanel::SoftPanel::new(
            softpanel::PanelCfg { mode: softpanel::MODE_DETERMINISTIC, ..softpanel::PanelCfg::DEFAULT },
            60,
        );
        assert!(!p.catchup());
    }

    #[test]
    fn softpanel_switch_two_frame_window() {
        // 受理 → 第 1 拍旧节拍、第 2 拍 = 生效拍（ts 旧钟、refresh 新周期、
        // ACK 出席）、第 3 拍起新节拍。
        let mut p = softpanel::SoftPanel::new(softpanel::PanelCfg::DEFAULT, 60);
        assert_eq!(p.request_refresh(120), Some(()));
        let t1 = p.on_point(10_000_000_000).unwrap();
        assert_eq!((t1.refresh_ns, p.hz()), (16_666_667, 60));
        assert_eq!(p.last_ack(), None);
        let t2 = p.on_point(10_016_666_667).unwrap();
        assert_eq!(t2.refresh_ns, 8_333_333);
        let ack = p.last_ack().unwrap();
        assert_eq!((ack.effective_ns, ack.new_refresh_ns), (t2.ts_ns, 8_333_333));
        let t3 = p.on_point(10_033_333_334).unwrap();
        assert_eq!(t3.refresh_ns, 8_333_333);
        // Tiered 表外拒（档位成员制）；零档/同档/在途重复拒。
        let mut q = softpanel::SoftPanel::new(softpanel::PanelCfg::DEFAULT, 60);
        assert_eq!(q.request_refresh(77), None);
        assert_eq!(q.request_refresh(0), None);
        assert_eq!(q.request_refresh(60), None);
        assert_eq!(q.request_refresh(120), Some(()));
        assert_eq!(q.request_refresh(30), None, "在途重复请求应拒");
        // FreeRange：表外受理（钳制量化——500 → 480）。
        let mut f = softpanel::SoftPanel::new(
            softpanel::PanelCfg { model: RATE_MODEL_FREERANGE, ..softpanel::PanelCfg::DEFAULT },
            60,
        );
        assert_eq!(f.request_refresh(500), Some(()));
        f.on_point(1).unwrap();
        let t = f.on_point(2).unwrap();
        assert_eq!((t.refresh_ns, f.hz()), (2_083_333, 480));
    }

    #[test]
    fn softpanel_switch_inflight_faster_preempts() {
        // 在途抢占方向性（run7 k6-p6f arb1 回归钉）：未落地的降档可被
        // 「更快」请求换目标重开 2 帧生效窗（输入 boost 抢占正要睡的
        // 面板）；同速在途重复仍拒；更慢打断由上测 30-撞-在途-120 钉。
        let mut p = softpanel::SoftPanel::new(softpanel::PanelCfg::DEFAULT, 60);
        assert_eq!(p.request_refresh(30), Some(())); // 在途降档 60→30
        assert_eq!(p.request_refresh(30), None, "同速在途重复仍拒");
        assert_eq!(p.request_refresh(120), Some(()), "更快抢占在途降档");
        // 抢占后生效窗重开：第 1 拍旧钟、第 2 拍生效（120Hz）。
        let t1 = p.on_point(10_000_000_000).unwrap();
        assert_eq!(t1.refresh_ns, 16_666_667);
        let t2 = p.on_point(10_016_666_667).unwrap();
        assert_eq!((t2.refresh_ns, p.hz()), (8_333_333, 120));
    }

    #[test]
    fn softpanel_fault_knobs() {
        // drop=1000‰ → 全丢（seq 仍消耗——面板拍点在、事件不投递）。
        let cfg = softpanel::PanelCfg { drop_permille: 1000, ..softpanel::PanelCfg::DEFAULT };
        let mut p = softpanel::SoftPanel::new(cfg, 60);
        for i in 1..=3u64 {
            let t = p.on_point(i);
            assert_eq!(t, None);
        }
        // drop 后投递面 seq 单调且见跳号（诚实丢帧信号）。
        let cfg = softpanel::PanelCfg { drop_permille: 500, seed: 7, ..softpanel::PanelCfg::DEFAULT };
        let mut p = softpanel::SoftPanel::new(cfg, 60);
        let seqs: Vec<u64> = (0..200u64).filter_map(|i| p.on_point(i).map(|t| t.seq)).collect();
        assert!(!seqs.is_empty() && seqs.windows(2).all(|w| w[1] > w[0]));
        assert!(seqs.len() < 200, "drop=500‰ 200 拍全过 = LCG 未生效");
        assert!(seqs.windows(2).any(|w| w[1] - w[0] > 1), "无跳号 = 丢帧未耗 seq");
        // jitter：|ts - nominal| ≤ 幅度；同输入序列 → 位元相等。
        let cfg = softpanel::PanelCfg { jitter_ns: 1000, seed: 9, ..softpanel::PanelCfg::DEFAULT };
        let mut p = softpanel::SoftPanel::new(cfg, 60);
        let mut snaps = Vec::new();
        for i in 0..50u64 {
            let nom = 1_000_000_000 + i * 16_666_667;
            let t = p.on_point(nom).unwrap();
            let delta = (t.ts_ns as i64 - nom as i64).abs();
            assert!(delta <= 1000, "jitter 越幅 @{i}: {delta}");
            snaps.push(t);
        }
        let mut p2 = softpanel::SoftPanel::new(cfg, 60);
        for (i, s) in snaps.iter().enumerate() {
            let nom = 1_000_000_000 + i as u64 * 16_666_667;
            assert_eq!(p2.on_point(nom).unwrap(), *s, "jitter 序列不确定 @{i}");
        }
        // sw_drop：切档生效拍起丢 N 拍，恢复后 seq 连续于全点序。
        let cfg = softpanel::PanelCfg { sw_drop: 2, ..softpanel::PanelCfg::DEFAULT };
        let mut p = softpanel::SoftPanel::new(cfg, 60);
        p.request_refresh(120).unwrap();
        p.on_point(1).unwrap(); // 旧节拍拍
        assert!(p.on_point(2).is_none()); // 生效拍丢 1/2
        assert!(p.on_point(3).is_none()); // 丢 2/2
        let t = p.on_point(4).unwrap(); // 恢复投递（新节拍）
        assert_eq!((t.seq, t.refresh_ns), (4, 8_333_333));
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
        // ATTACH req 1B（K1 per-app 通道——随行恰 1 面在 wmd 分发层执法）。
        let n = encode_wm_attach_req(&mut buf);
        assert_eq!(&buf[..n], &[0x69][..]);
        // DETACH req 5B（srv_handle u32 LE）。
        let n = encode_wm_detach_req(&mut buf, 0x0102_0304);
        assert_eq!(&buf[..n], &[0x6A, 0x04, 0x03, 0x02, 0x01][..]);
        // LE 钉板：srv_handle 首字节 = 最低位字节。
        assert_eq!(buf[1], 0x04);
        // 帧型带隔离核证（K1 §1.1）：0x69/0x6A 与 0x90 族事件帧带外双向
        // 互不认领（新 codec 拒事件帧首字节；parse_wm_evt 拒两新 op）。
        assert_eq!(parse_wm_attach_req(&[WM_EVT_TABLE, 0, 0, 0, 0]), None);
        assert_eq!(parse_wm_detach_req(&[WM_EVT_BACK, 0, 0, 0]), None);
        assert_eq!(parse_wm_evt(&[OP_WM_ATTACH]), None);
        assert_eq!(parse_wm_evt(&[OP_WM_DETACH, 0, 0, 0, 0]), None);
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
        let n = encode_wm_attach_req(&mut buf);
        frames.push(buf[..n].to_vec());
        let n = encode_wm_detach_req(&mut buf, 0x1234_5678);
        frames.push(buf[..n].to_vec());

        // 帧序号分派解析器（十一帧型 → 统一 Option<()> 拒面）。
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
                9 => parse_wm_attach_req(b).map(|_| ()),
                10 => parse_wm_detach_req(b).map(|_| ()),
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
        // CREATE：layer 野值 3（波3 收宽：SERVICE=2 入域，拒点移钉 3）/
        // flags 未知位 0x02。
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
        let bad_layer = WmCreateReq { layer: 3, ..good_create };
        assert_eq!(encode_wm_create_req(&mut buf, &bad_layer), None);
        let mut f = [0u8; WM_CREATE_REQ_LEN];
        let _ = encode_wm_create_req(&mut f, &good_create).unwrap();
        f[1] = 3;
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
        // 快照表项：layer 野值 3（波3 收宽同 CREATE）/ flags 未知位 0x04（off 头+15）。
        let n = encode_wm_snapshot_rep(&mut buf, &wm_snap_sample(1)).unwrap();
        let mut f = buf;
        f[WM_SNAP_HEAD + 13] = 3;
        assert_eq!(parse_wm_snapshot_rep(&f[..n]), None);
        let mut f = buf;
        f[WM_SNAP_HEAD + 15] = 0x04;
        assert_eq!(parse_wm_snapshot_rep(&f[..n]), None);
        let mut bad_snap = wm_snap_sample(1);
        bad_snap.entries[0].flags = WM_WIN_F_FOCUSED | 0x04;
        assert_eq!(encode_wm_snapshot_rep(&mut buf, &bad_snap), None);
        bad_snap.entries[0].layer = 3;
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

    // —— 波2 输入焦点协议（add-only）：FOCUS/BACK/INJECT 往返 + 拒面 ——

    #[test]
    fn wm_focus_back_roundtrip_and_rejects() {
        let mut buf = [0u8; 32];
        let n = encode_wm_focus_req(&mut buf, 0x0304);
        assert_eq!(&buf[..n], &[OP_WM_FOCUS, 0x04, 0x03, 0, 0, 0, 0, 0][..]);
        assert_eq!(parse_wm_focus_req(&buf[..n]), Some(0x0304));
        // 让出（win=0）合法。
        let n = encode_wm_focus_req(&mut buf, 0);
        assert_eq!(parse_wm_focus_req(&buf[..n]), Some(0));
        // 拒面：长度不符 / 错 op。
        assert_eq!(parse_wm_focus_req(&buf[..n - 1]), None);
        assert_eq!(parse_wm_focus_req(&[OP_WM_BACK, 0, 0, 0, 0, 0, 0, 0]), None);
        // BACK。
        let n = encode_wm_back_req(&mut buf);
        assert_eq!(&buf[..n], &[OP_WM_BACK, 0, 0, 0, 0, 0, 0, 0][..]);
        assert_eq!(parse_wm_back_req(&buf[..n]), Some(()));
        assert_eq!(parse_wm_back_req(&buf[..n + 1]), None);
        // 新 op 不与既有 wm 带冲突（互斥带内新值——add-only 不改旧值）。
        assert_ne!(OP_WM_FOCUS, OP_WM_BACK);
        assert!(OP_WM_FOCUS > OP_WM_TABLE_WAIT && OP_WM_BACK > OP_WM_TABLE_WAIT);
    }

    #[test]
    fn input_inject_roundtrip_and_rejects() {
        let mut buf = [0u8; 64];
        // 键盘单键。
        let n = encode_input_inject_req(&mut buf, DEV_KEYBOARD, &[b'a']).unwrap();
        assert_eq!(parse_input_inject_req(&buf[..n]), Some((DEV_KEYBOARD, &[b'a'][..])));
        // 文本串。
        let n = encode_input_inject_req(&mut buf, DEV_TEXT, b"hello").unwrap();
        assert_eq!(parse_input_inject_req(&buf[..n]), Some((DEV_TEXT, &b"hello"[..])));
        // 拒面：载荷越帽（25B）/ buf 不足 / 野 dev_kind。
        assert_eq!(encode_input_inject_req(&mut buf, DEV_TEXT, &[0u8; 25]), None);
        let mut tiny = [0u8; 3];
        assert_eq!(encode_input_inject_req(&mut tiny, DEV_KEYBOARD, &[b'a']), None);
        let n = encode_input_inject_req(&mut buf, DEV_TEXT, b"xy").unwrap();
        let mut bad = buf[..n].to_vec();
        bad[1] = DEV_TEXT + 1;
        assert_eq!(parse_input_inject_req(&bad), None, "野 dev_kind 拒");
        assert_eq!(parse_input_inject_req(&[OP_INPUT_POLL, 0, 0, 0]), None, "错 op 拒");
        assert_eq!(WM_LAYER_SERVICE, 2, "服务层值域在 SYSTEM 之上");
    }

    // —— 波3 wm 事件通道 + SERVICE 层收宽（add-only 消费侧测试） ——

    #[test]
    fn wm_layer_service_create_and_snapshot_legal() {
        // SERVICE=2 入域（波2 R1「拒点从 2 移钉 3」落地）：CREATE/快照双侧
        // 编解码对称收 2。
        let mut buf = [0u8; WM_REP_MAX];
        let svc = WmCreateReq {
            layer: WM_LAYER_SERVICE,
            flags: 0,
            parent: 0,
            group: 0,
            x: 0,
            y: 400,
            w: 800,
            h: 200,
        };
        let n = encode_wm_create_req(&mut buf, &svc).unwrap();
        assert_eq!(parse_wm_create_req(&buf[..n]), Some(svc));
        let mut snap = wm_snap_sample(1);
        snap.entries[0].layer = WM_LAYER_SERVICE;
        let n = encode_wm_snapshot_rep(&mut buf, &snap).unwrap();
        assert_eq!(parse_wm_snapshot_rep(&buf[..n]).unwrap().ok().unwrap(), snap);
    }

    #[test]
    fn wm_hit_test_service_layer_precedence() {
        // 层降序天然覆盖 SERVICE>SYSTEM>APP（layer 数值序即层序——零算法
        // 改动，波2 冻结口径的波3 消费侧钉板）。
        let base = WmEntry {
            id: 7,
            parent: 0,
            group: 0,
            x: 0,
            y: 0,
            w: 800,
            h: 600,
            layer: WM_LAYER_APP,
            z: 0,
            flags: WM_WIN_F_VISIBLE,
        };
        let sys = WmEntry { id: 3, h: 30, layer: WM_LAYER_SYSTEM, ..base };
        let svc = WmEntry {
            id: 9,
            y: 560,
            h: 40,
            layer: WM_LAYER_SERVICE,
            z: 0,
            ..base
        };
        // SERVICE 压 SYSTEM 任意 z（软键盘暂态压顶语义）。
        assert_eq!(wm_hit_test(&[sys, svc], 400, 575), Some(9));
        assert_eq!(wm_hit_test(&[svc, sys], 400, 575), Some(9));
        // SERVICE 窗外仍中 SYSTEM。
        assert_eq!(wm_hit_test(&[sys, svc], 400, 10), Some(3));
        // APP 恒垫底：带外点穿到 APP、带内点中 SYSTEM。
        assert_eq!(wm_hit_test(&[base, sys, svc], 400, 10), Some(3));
        assert_eq!(wm_hit_test(&[base, sys, svc], 400, 300), Some(7));
        assert_eq!(wm_hit_test(&[base, svc], 400, 300), Some(7));
    }

    #[test]
    fn wm_subscribe_roundtrip_and_rejects() {
        let mut buf = [0u8; 8];
        let n = encode_wm_subscribe_req(&mut buf);
        assert_eq!(&buf[..n], &[OP_WM_SUBSCRIBE, 0, 0, 0][..]);
        assert_eq!(parse_wm_subscribe_req(&buf[..n]), Some(()));
        // 拒面：长度不符 / 错 op（含 TABLE_GET 同形邻值）。
        assert_eq!(parse_wm_subscribe_req(&buf[..n - 1]), None);
        assert_eq!(parse_wm_subscribe_req(&buf[..n + 1]), None);
        assert_eq!(parse_wm_subscribe_req(&[OP_WM_TABLE_GET, 0, 0, 0]), None);
        // op 带位：wm 域 0x60..=0x7F 内 TABLE_WAIT 之后的互斥新值。
        assert!(OP_WM_SUBSCRIBE > OP_WM_BACK && OP_WM_BACK > OP_WM_FOCUS);
    }

    #[test]
    fn wm_evt_frames_roundtrip_and_rejects() {
        let mut buf = [0u8; 8];
        // BACK 事件：`[kind][wid u16][r]` 恰 4B；尾 r 解析侧恒不校验。
        let n = encode_wm_evt_back(&mut buf, 0x0304);
        assert_eq!(&buf[..n], &[WM_EVT_BACK, 0x04, 0x03, 0][..]);
        assert_eq!(parse_wm_evt_back(&buf[..n]), Some(0x0304));
        buf[3] = 0xEE;
        assert_eq!(parse_wm_evt_back(&buf[..n]), Some(0x0304));
        assert_eq!(parse_wm_evt_back(&buf[..n - 1]), None);
        assert_eq!(parse_wm_evt_back(&buf[..n + 1]), None);
        // TABLE 事件：`[kind][version u32]` 恰 5B；version 任意 u32 合法
        // （「恒 ≥1」是服务端不变量非 wire 执法，Q3 同款）。
        let n = encode_wm_evt_table(&mut buf, 0x1122_3344);
        assert_eq!(&buf[..n], &[WM_EVT_TABLE, 0x44, 0x33, 0x22, 0x11][..]);
        assert_eq!(parse_wm_evt_table(&buf[..n]), Some(0x1122_3344));
        for v in [0u32, u32::MAX] {
            let n = encode_wm_evt_table(&mut buf, v);
            assert_eq!(parse_wm_evt_table(&buf[..n]), Some(v));
        }
        assert_eq!(parse_wm_evt_table(&buf[..4]), None);
        assert_eq!(parse_wm_evt_table(&buf[..6]), None);
        // BACK_ACK：恰 1B。
        let n = encode_wm_back_ack(&mut buf);
        assert_eq!(&buf[..n], &[WM_EVT_BACK_ACK][..]);
        assert_eq!(parse_wm_back_ack(&buf[..n]), Some(()));
        assert_eq!(parse_wm_back_ack(&[]), None);
        assert_eq!(parse_wm_back_ack(&[WM_EVT_BACK, 0]), None);
    }

    #[test]
    fn wm_evt_unified_parse_and_band_isolation() {
        // 统一判别面：三帧型互斥收口。
        let mut buf = [0u8; 8];
        let n = encode_wm_evt_back(&mut buf, 9);
        assert_eq!(parse_wm_evt(&buf[..n]), Some(WmEvt::Back(9)));
        let n = encode_wm_evt_table(&mut buf, 5);
        assert_eq!(parse_wm_evt(&buf[..n]), Some(WmEvt::Table(5)));
        let n = encode_wm_back_ack(&mut buf);
        assert_eq!(parse_wm_evt(&buf[..n]), Some(WmEvt::BackAck));
        // 错帧型/截断/空 → None。
        assert_eq!(parse_wm_evt(&[0x00]), None);
        assert_eq!(parse_wm_evt(&[WM_EVT_BACK, 0x01]), None);
        assert_eq!(parse_wm_evt(&[]), None);
        // 帧型带隔离：0x90..=0x92 落在全部 op 带（present 1..=6 / input
        // 0x20..=0x22 / hdi 0x40..=0x43 / wm 0x60..=0x7F）之外——错投任一
        // 请求分发面恒 ST_BAD（结构性防错投，wm 带互斥 Q1 同法）。
        let kinds = [WM_EVT_BACK, WM_EVT_TABLE, WM_EVT_BACK_ACK];
        for k in kinds {
            assert!(k < OP_WM_GETINFO || k > 0x7F, "事件帧型 {} 不得落 wm op 带", k);
            assert!(!(1..=6).contains(&k) && !(0x20..=0x22).contains(&k));
            assert!(!(0x40..=0x43).contains(&k));
            assert!(parse_wm_create_req(&[k, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]) == None);
        }
        assert_ne!(WM_EVT_BACK, WM_EVT_TABLE);
        assert_ne!(WM_EVT_TABLE, WM_EVT_BACK_ACK);
        assert_ne!(WM_EVT_BACK, WM_EVT_BACK_ACK);
    }

    // ---------- 刀3 仲裁器决策表（规范 §6.3 A1-A11——每条一正一负 + 组合钉板）----------
    use super::arbiter::*;

    /// Tiered/FREE boot 档仲裁器（`hz` = boot 档）。
    fn ar_boot(hz: u16) -> Arbiter {
        Arbiter::new(&softpanel::PanelCfg::DEFAULT, hz)
    }
    /// DET 模式（det_test = true——A3 测试档准入）。
    fn ar_det(hz: u16) -> Arbiter {
        Arbiter::new(
            &softpanel::PanelCfg {
                mode: softpanel::MODE_DETERMINISTIC,
                ..softpanel::PanelCfg::DEFAULT
            },
            hz,
        )
    }
    /// FreeRange 模型。
    fn ar_free(hz: u16) -> Arbiter {
        Arbiter::new(
            &softpanel::PanelCfg {
                model: RATE_MODEL_FREERANGE,
                ..softpanel::PanelCfg::DEFAULT
            },
            hz,
        )
    }
    /// 票样本（intent 判别值 × min/max/preferred）。
    fn bal(intent: u8, min: u16, max: u16, pref: u16) -> Ballot {
        Ballot { sticky: false, intent, min_hz: min, max_hz: max, preferred_hz: pref }
    }
    /// 测试时钟：60Hz 拍距。
    const T: u64 = 16_666_667;

    #[test]
    fn arb_wire_input_pulse() {
        // 0x07 4B 无参面：编解码往返 + 截断/尾随/op 错位拒。
        assert_eq!(OP_INPUT_PULSE, 7);
        let mut buf = [0u8; 8];
        let n = encode_present_req(&mut buf, &PresentReq::InputPulse).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], &[OP_INPUT_PULSE, 0, 0, 0]);
        assert_eq!(parse_present_req(&buf[..4]), Some(PresentReq::InputPulse));
        assert_eq!(parse_present_req(&buf[..5]), None, "尾随字节拒");
        assert_eq!(parse_present_req(&[0x09, 0, 0, 0]), None, "未知 op 拒（0x08 已被刀4 FRAME_TIMING 占用）");
        // out 不足 None（无参面同款 None 契约）。
        assert_eq!(encode_present_req(&mut buf[..3], &PresentReq::InputPulse), None);
    }

    #[test]
    fn arb_pins() {
        // 决策常量钉板（规范 §6.3 A5/A6/A2）。
        assert_eq!(PLATFORM_FLOOR_HZ, 30);
        assert_eq!(T_DOWN_DWELL_NS, 1_000_000_000);
        assert_eq!(T_TOUCH_BOOST_NS, 500_000_000);
        assert_eq!(UP_CONFIRM_TICKS, 2);
        assert_eq!(BOOST_HZ, 120);
        assert_eq!(SAFE_HZ, 60);
        // LTPO 档（1/10Hz）结构性出局：平台底线 30 > 全部 LTPO 档位——
        // class 过滤之外 G 区间先排空。
        let ltpo_max = softpanel::TIERS
            .iter()
            .filter(|t| t.class == PANEL_CLASS_LTPO)
            .map(|t| t.hz)
            .max()
            .unwrap();
        assert!(PLATFORM_FLOOR_HZ > ltpo_max);
        // 初态。
        let mut a = ar_boot(60);
        assert_eq!(a.hz(), 60);
        assert!(!a.stopped());
        assert!(!a.take_warn_no_g());
    }

    #[test]
    fn arb_a11_platform_is_boot_tier() {
        // A11 正面：无票缺省 = boot 档（@30 平台表达）而非硬编 60——boost
        // 抬 120 后回落到 30。
        let mut a = ar_boot(30);
        assert_eq!(a.on_input_pulse(0, false), Decision::Switch { hz: 120, src: Src::Boost });
        // boost 窗（500ms）内维持；过期后降档驻留起算。
        assert_eq!(a.on_time(600_000_000, false), Decision::Stay);
        assert_eq!(a.next_wake(600_000_000), Some(1_600_000_000));
        assert_eq!(
            a.on_time(1_600_000_000, false),
            Decision::Switch { hz: 30, src: Src::Dwell },
            "回落 boot 30 而非硬编 60"
        );
        assert_eq!(a.hz(), 30);
        assert_eq!(a.next_wake(1_600_000_000), None);
        // 无票且目标==当前：Stay（不空转切档）。
        let mut b = ar_boot(60);
        assert_eq!(b.on_time(0, false), Decision::Stay);
    }

    #[test]
    fn arb_a2_range_floor_and_constraints() {
        // G = [max(30, min), min(480, max)] ∩ 档位表：pref 120 min 90 max 200
        // → 唯一候选 120（60 在 gmin 之下出局）。
        let mut a = ar_boot(60);
        assert_eq!(
            a.on_vote(0, false, bal(Intent::Interactive as u8, 90, 200, 120)),
            Decision::Stay
        );
        assert_eq!(a.on_tick(T, false), Decision::Stay);
        assert_eq!(
            a.on_tick(2 * T, false),
            Decision::Switch { hz: 120, src: Src::Vote }
        );
        // 平台底线钳制：pref 5 → 最低产品档 30（禁穿地板）。
        let mut f = ar_boot(60);
        f.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 5));
        assert_eq!(f.on_time(T_DOWN_DWELL_NS, false), Decision::Switch { hz: 30, src: Src::Dwell });
        // Unspecified 票的 max 约束仍生效（约束≠偏好——Platform 臂过 select）。
        let mut u = ar_boot(120);
        u.on_vote(0, false, bal(Intent::Unspecified as u8, 0, 60, 0));
        assert_eq!(u.on_time(T, false), Decision::Stay, "驻留起算");
        assert_eq!(
            u.on_time(T + T_DOWN_DWELL_NS, false),
            Decision::Switch { hz: 60, src: Src::Dwell },
            "boot 120 被 Unspecified max=60 约束压回 60"
        );
    }

    #[test]
    fn arb_a2_no_solution_fallback() {
        // 正面：min > max 无解 → 安全档 60 + Fallback 源 + 告警记号（读即清）。
        let mut a = ar_boot(30);
        a.on_vote(0, false, bal(Intent::Interactive as u8, 480, 30, 0));
        assert_eq!(a.on_tick(T, false), Decision::Stay);
        assert_eq!(
            a.on_tick(2 * T, false),
            Decision::Switch { hz: 60, src: Src::Fallback }
        );
        assert!(a.take_warn_no_g(), "A2 无解告警到席");
        assert!(!a.take_warn_no_g(), "读即清");
        // 负面：可解区间不走 Fallback。
        let mut b = ar_boot(30);
        b.on_vote(0, false, bal(Intent::Interactive as u8, 30, 120, 0));
        b.on_tick(T, false);
        assert_eq!(b.on_tick(2 * T, false), Decision::Switch { hz: 120, src: Src::Vote });
        assert!(!b.take_warn_no_g());
    }

    #[test]
    fn arb_a3_candidate_classes() {
        // 正面（DET 准入）：G=[100,200] 测试档 165/180 仅 DET 测试模式入候选
        // ——pref 170 → 180。
        let mut d = ar_det(60);
        d.on_vote(0, false, bal(Intent::Interactive as u8, 100, 200, 170));
        assert_eq!(d.on_tick(T, false), Decision::Stay);
        assert_eq!(
            d.on_tick(2 * T, false),
            Decision::Switch { hz: 180, src: Src::Vote }
        );
        // 负面（FREE 排斥）：同票在 FREE 模式 → 唯一产品候选 120（170 无
        // ≥pref 候选 → 顶档兜底）。
        let mut f = ar_boot(60);
        f.on_vote(0, false, bal(Intent::Interactive as u8, 100, 200, 170));
        assert_eq!(f.on_tick(T, false), Decision::Stay);
        assert_eq!(
            f.on_tick(2 * T, false),
            Decision::Switch { hz: 120, src: Src::Vote }
        );
    }

    #[test]
    fn arb_a4_intent_defaults_six_values() {
        // §6.2 法定表六值钉板（pref=0 走 Intent 映射；升 2 拍/降驻留路径各验）。
        // Unspecified → 无偏好贡献 → Platform boot 档（== cur → Stay）。
        let mut u = ar_boot(60);
        assert_eq!(u.on_vote(0, false, bal(Intent::Unspecified as u8, 0, 0, 0)), Decision::Stay);
        // LatencyCritical → 面板顶档 480。
        let mut l = ar_boot(60);
        l.on_vote(0, false, bal(Intent::LatencyCritical as u8, 0, 0, 0));
        assert_eq!(l.on_tick(T, false), Decision::Stay);
        assert_eq!(
            l.on_tick(2 * T, false),
            Decision::Switch { hz: 480, src: Src::Vote }
        );
        // Interactive → 120。
        let mut i = ar_boot(60);
        i.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 0));
        assert_eq!(i.on_tick(T, false), Decision::Stay);
        assert_eq!(
            i.on_tick(2 * T, false),
            Decision::Switch { hz: 120, src: Src::Vote }
        );
        // Utility → 60（== cur → Stay 负面）。
        let mut ut = ar_boot(60);
        assert_eq!(ut.on_vote(0, false, bal(Intent::Utility as u8, 0, 0, 0)), Decision::Stay);
        // Maintenance → 30（降档驻留）。
        let mut m = ar_boot(60);
        m.on_vote(0, false, bal(Intent::Maintenance as u8, 0, 0, 0));
        assert_eq!(m.on_time(T_DOWN_DWELL_NS, false), Decision::Switch { hz: 30, src: Src::Dwell });
        // Background → 30（非静默面同 Maintenance；静默面走 A7）。
        let mut bg = ar_boot(60);
        bg.on_vote(0, false, bal(Intent::Background as u8, 0, 0, 0));
        assert_eq!(bg.on_time(T_DOWN_DWELL_NS, false), Decision::Switch { hz: 30, src: Src::Dwell });
        // preferred_hz 非零覆盖 Intent 缺省：Interactive + pref 240 → 240。
        let mut p = ar_boot(60);
        p.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 240));
        p.on_tick(T, false);
        assert_eq!(p.on_tick(2 * T, false), Decision::Switch { hz: 240, src: Src::Vote });
    }

    #[test]
    fn arb_a4_freerange_continuous() {
        // 连续区间直取：pref 144 非 Tiered 档位也直落。
        let mut a = ar_free(60);
        a.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 144));
        assert_eq!(a.on_tick(T, false), Decision::Stay);
        assert_eq!(
            a.on_tick(2 * T, false),
            Decision::Switch { hz: 144, src: Src::Vote }
        );
        // 越界最近侧钳制：600 → 480（面板 max）。
        let mut hi = ar_free(60);
        hi.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 600));
        hi.on_tick(T, false);
        assert_eq!(hi.on_tick(2 * T, false), Decision::Switch { hz: 480, src: Src::Vote });
        // 票域钳制：pref 50 ∈ [100,200] → 100（钳后高于 cur 60 → 升路 2 拍）。
        let mut cl = ar_free(60);
        cl.on_vote(0, false, bal(Intent::Interactive as u8, 100, 200, 50));
        assert_eq!(cl.on_tick(T, false), Decision::Stay);
        assert_eq!(cl.on_tick(2 * T, false), Decision::Switch { hz: 100, src: Src::Vote });
        // FreeRange 无候选空集问题：G=[300,400] pref 50 → 连续钳 300（非安全档）。
        let mut fb = ar_free(60);
        fb.on_vote(0, false, bal(Intent::Interactive as u8, 300, 400, 50));
        assert_eq!(fb.on_tick(T, false), Decision::Stay);
        assert_eq!(fb.on_tick(2 * T, false), Decision::Switch { hz: 300, src: Src::Vote });
    }

    #[test]
    fn arb_a5_up_two_tick_streak() {
        // 正面：连续 2 拍确认（票/第一拍均不切）。
        let mut a = ar_boot(60);
        a.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 120));
        assert_eq!(a.on_tick(T, false), Decision::Stay, "第 1 拍不切");
        assert_eq!(a.on_tick(2 * T, false), Decision::Switch { hz: 120, src: Src::Vote });
        // 负面①：事件不积 streak——连续重复投票零拍切不了。
        let mut e = ar_boot(60);
        for _ in 0..5 {
            assert_eq!(
                e.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 120)),
                Decision::Stay
            );
        }
        // 负面②：目标回 cur 清 streak——重投后须满 2 拍（防 1 拍泄漏切档）。
        let mut r = ar_boot(60);
        r.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 120));
        r.on_tick(T, false); // streak 1
        assert_eq!(r.on_vote(2 * T, false, bal(Intent::Utility as u8, 0, 0, 0)), Decision::Stay); // target 60==cur → 清
        r.on_vote(3 * T, false, bal(Intent::Interactive as u8, 0, 0, 120));
        assert_eq!(r.on_tick(4 * T, false), Decision::Stay, "清后第 1 拍不切");
        assert_eq!(r.on_tick(5 * T, false), Decision::Switch { hz: 120, src: Src::Vote });
        // 负面③：目标变更重置 streak——pref 换 240 后重新数 2 拍。
        let mut c = ar_boot(60);
        c.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 120));
        c.on_tick(T, false); // streak 1 @120
        c.on_vote(2 * T, false, bal(Intent::Interactive as u8, 0, 0, 240)); // 目标变更
        assert_eq!(c.on_tick(3 * T, false), Decision::Stay, "变更后第 1 拍不切");
        assert_eq!(c.on_tick(4 * T, false), Decision::Switch { hz: 240, src: Src::Vote });
    }

    #[test]
    fn arb_a5_down_dwell_semantics() {
        // 正面：1s 驻留满才切（999ms 不切 / 1000ms 切）。
        let mut a = ar_boot(60);
        a.on_vote(0, false, bal(Intent::Maintenance as u8, 0, 0, 0));
        assert_eq!(a.on_time(999_999_999, false), Decision::Stay);
        assert_eq!(a.on_time(1_000_000_000, false), Decision::Switch { hz: 30, src: Src::Dwell });
        // 同目标驻留保序：中途同目标事件不重计时（t0 不动）。
        let mut s = ar_boot(60);
        s.on_vote(0, false, bal(Intent::Maintenance as u8, 0, 0, 0));
        s.on_vote(500_000_000, false, bal(Intent::Maintenance as u8, 0, 0, 30)); // pref 显式 30 同目标
        assert_eq!(s.on_time(999_999_999, false), Decision::Stay);
        assert_eq!(s.on_time(1_000_000_000, false), Decision::Switch { hz: 30, src: Src::Dwell });
        // 目标离开清驻留：重降须重新满 1s（t0 重锚）。
        let mut d = ar_boot(60);
        d.on_vote(0, false, bal(Intent::Maintenance as u8, 0, 0, 0)); // down(30, 0)
        d.on_vote(400_000_000, false, bal(Intent::Utility as u8, 0, 0, 0)); // target 60==cur → 清
        d.on_vote(500_000_000, false, bal(Intent::Maintenance as u8, 0, 0, 0)); // 重锚 down(30, 500ms)
        assert_eq!(d.on_time(1_200_000_000, false), Decision::Stay, "700ms < 1s");
        assert_eq!(d.on_time(1_500_000_000, false), Decision::Switch { hz: 30, src: Src::Dwell });
        // next_wake 透视：驻留臂到点。
        let mut w = ar_boot(60);
        w.on_vote(1_000_000_000, false, bal(Intent::Maintenance as u8, 0, 0, 0));
        assert_eq!(w.next_wake(1_000_000_000), Some(2_000_000_000));
    }

    #[test]
    fn arb_a5_dim_one_tier_down() {
        // 正面（Dim 源上行）：boot 30 + Interactive → dim 降一档 60 → 升路携 Dim。
        let mut a = ar_boot(30);
        a.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 120));
        assert_eq!(a.on_power_mode(T, false, POWER_DIM), Decision::Stay, "事件不积 streak");
        assert_eq!(a.on_tick(2 * T, false), Decision::Stay);
        assert_eq!(a.on_tick(3 * T, false), Decision::Switch { hz: 60, src: Src::Dim });
        // 正面（降路）：120 + DIM → 60 走驻留（降路提交源恒 Dwell）。
        let mut d = ar_boot(60);
        d.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 120));
        d.on_tick(T, false);
        d.on_tick(2 * T, false); // 120 到位
        assert_eq!(d.on_power_mode(3 * T, false, POWER_DIM), Decision::Stay);
        assert_eq!(
            d.on_time(3 * T + T_DOWN_DWELL_NS, false),
            Decision::Switch { hz: 60, src: Src::Dwell }
        );
        // 负面：已最低不动（30 无更低产品档）。
        let mut f = ar_boot(30);
        f.on_vote(0, false, bal(Intent::Maintenance as u8, 0, 0, 0)); // target 30 == cur
        assert_eq!(f.on_power_mode(T, false, POWER_DIM), Decision::Stay);
        // 负面：DIM 不动无票 Platform 档（偏好修饰词——无偏好可降）。
        let mut n = ar_boot(60);
        assert_eq!(n.on_power_mode(0, false, POWER_DIM), Decision::Stay);
        // 恢复：PM NORMAL 清 dim → 目标回 120 == cur → 降档驻留被清、1s 后
        // 也不切（DIM 降档意图整体撤销）。
        let mut r = ar_boot(60);
        r.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 120));
        r.on_tick(T, false);
        r.on_tick(2 * T, false); // 120
        r.on_power_mode(3 * T, false, POWER_DIM); // dim → down(60, 3T) 起
        assert_eq!(r.on_power_mode(4 * T, false, POWER_NORMAL), Decision::Stay);
        assert_eq!(r.on_time(4 * T + T_DOWN_DWELL_NS, false), Decision::Stay, "NORMAL 撤销降档意图");
        assert_eq!(r.hz(), 120);
    }

    #[test]
    fn arb_a6_boost_window() {
        // 正面：立即升（绕 streak——零拍切 120）。
        let mut a = ar_boot(60);
        assert_eq!(a.on_input_pulse(0, false), Decision::Switch { hz: 120, src: Src::Boost });
        assert_eq!(a.hz(), 120);
        // 窗内维持：400ms 重臂延窗，700ms 仍在窗（不落降）。
        assert_eq!(a.on_input_pulse(400_000_000, false), Decision::Stay, "已 120 维持");
        assert_eq!(a.on_time(700_000_000, false), Decision::Stay, "延窗内不降");
        // 过窗回落：900ms 过期（boost_until=900ms）→ 驻留起算 → 1.9s 切 60。
        assert_eq!(a.on_time(900_000_000, false), Decision::Stay);
        assert_eq!(
            a.on_time(1_900_000_000, false),
            Decision::Switch { hz: 60, src: Src::Dwell }
        );
        // G 上限：Tiered max=90 → boost 目标量化到 60（== cur → 无升可走）。
        let mut t = ar_boot(60);
        t.on_vote(0, false, bal(Intent::Interactive as u8, 0, 90, 0));
        assert_eq!(t.on_input_pulse(T, false), Decision::Stay, "90 无档可量化——停在 60");
        // FreeRange max=90 → 连续 90 直取。
        let mut f = ar_free(60);
        f.on_vote(0, false, bal(Intent::Interactive as u8, 0, 90, 0));
        assert_eq!(f.on_input_pulse(T, false), Decision::Switch { hz: 90, src: Src::Boost });
        // 停帧态脉冲 = 活动：直接脉冲破停 + boost（Resume 证据归调用方 on_activity 序）。
        let mut s = ar_boot(60);
        assert_eq!(s.on_tick(0, true), Decision::StopFrame);
        assert_eq!(s.on_input_pulse(T, false), Decision::Switch { hz: 120, src: Src::Boost });
    }

    #[test]
    fn arb_a10_hint_dense_upshift() {
        // 正面①「不迟一帧」：Interactive pref=120 票 + 当前 60（票落账零拍
        // 未升——Event 不积 streak）→ 密集预告 → 单发即切 120（对照路：无
        // 预告须 2 拍 streak——arb_a5_up_two_tick_streak 钉板）。
        let mut a = ar_boot(60);
        a.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 120));
        assert_eq!(
            a.on_present_hint(T, false, true),
            Decision::Switch { hz: 120, src: Src::Vote },
            "密集预告立即升——绕 streak"
        );
        assert_eq!(a.hz(), 120);
        // 资格三档矩阵：LatencyCritical / Interactive / Utility 均享快路。
        for intent in [Intent::LatencyCritical, Intent::Interactive, Intent::Utility] {
            let mut q = ar_boot(60);
            q.on_vote(0, false, bal(intent as u8, 0, 0, 120));
            assert_eq!(
                q.on_present_hint(T, false, true),
                Decision::Switch { hz: 120, src: Src::Vote },
                "{intent:?} 快路"
            );
        }
        // 负面①：非密集 → Stay 且 streak 面零污染（后续正常 2 拍路完好）。
        let mut n = ar_boot(60);
        n.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 120));
        assert_eq!(n.on_tick(T, false), Decision::Stay, "streak 1");
        assert_eq!(n.on_present_hint(T + 5_000_000, false, false), Decision::Stay);
        assert_eq!(n.on_tick(2 * T, false), Decision::Switch { hz: 120, src: Src::Vote });
        // 负面②：背景类/无高帧偏好 Intent（Background/Unspecified/
        // Maintenance）密集预告 → Stay。
        for intent in [Intent::Background, Intent::Unspecified, Intent::Maintenance] {
            let mut b = ar_boot(60);
            b.on_vote(0, false, bal(intent as u8, 0, 0, 120));
            assert_eq!(b.on_present_hint(T, false, true), Decision::Stay, "{intent:?} 不入快路");
        }
        // 负面③：无票密集预告 → Stay（A10 要偏好承载票——无票走 A11
        // 平台档，预告不构成偏好）。
        let mut nv = ar_boot(60);
        assert_eq!(nv.on_present_hint(T, false, true), Decision::Stay);
        // 动机例（规范 A10）：30Hz 降探后首帧压界——Maintenance 驻留降
        // 30 → 换 Interactive 120 票（Event 不升）→ 密集预告立即升。
        let mut m = ar_boot(60);
        m.on_vote(0, false, bal(Intent::Maintenance as u8, 0, 0, 0));
        assert_eq!(m.on_time(T_DOWN_DWELL_NS, false), Decision::Switch { hz: 30, src: Src::Dwell });
        m.on_vote(1_100_000_000, false, bal(Intent::Interactive as u8, 0, 0, 120));
        assert_eq!(
            m.on_present_hint(1_100_000_000, false, true),
            Decision::Switch { hz: 120, src: Src::Vote },
            "降探后首帧预告即升——消迟一帧"
        );
        // 负面④：预告禁降档快路（target < cur 仍走驻留——Maintenance 票
        // 密集预告不提前降）。
        let mut d = ar_boot(120);
        d.on_vote(0, false, bal(Intent::Maintenance as u8, 0, 0, 0)); // down(30, 0)
        assert_eq!(d.on_present_hint(T, false, true), Decision::Stay, "降目标仍走驻留");
        assert_eq!(
            d.on_time(T + T_DOWN_DWELL_NS, false),
            Decision::Switch { hz: 30, src: Src::Dwell }
        );
        // 负面⑤：G 帽量化后 == cur → Stay（Tiered max=90 → 120 量化 60）。
        let mut g = ar_boot(60);
        g.on_vote(0, false, bal(Intent::Interactive as u8, 0, 90, 120));
        assert_eq!(g.on_present_hint(T, false, true), Decision::Stay, "90 帽内无档可升");
        // DIM ∧ 预告：目标走 Dim 链携 Dim 源（boot 30 + Interactive 120 →
        // dim 60 > 30 = 升路）。
        let mut dm = ar_boot(30);
        dm.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 120));
        dm.on_power_mode(0, false, POWER_DIM);
        assert_eq!(dm.on_present_hint(T, false, true), Decision::Switch { hz: 60, src: Src::Dim });
        // boost 窗内预告：目标已 Boost 面——Hint 直commit 不改写源。
        let mut b = ar_boot(60);
        b.on_input_pulse(0, false); // boost 120 到位
        let mut v = ar_boot(60);
        v.on_vote(0, false, bal(Intent::Utility as u8, 0, 0, 120));
        assert_eq!(v.on_input_pulse(0, false), Decision::Switch { hz: 120, src: Src::Boost });
        assert_eq!(v.on_present_hint(T, false, true), Decision::Stay, "boost 120 == 票目标——维持");
    }

    #[test]
    fn arb_a7_stop_and_resume() {
        // 正面：无票 + 静默 → 停帧（幂等）+ 零醒。
        let mut a = ar_boot(60);
        assert_eq!(a.on_tick(0, true), Decision::StopFrame);
        assert!(a.stopped());
        assert_eq!(a.next_wake(0), None);
        assert_eq!(a.on_tick(T, true), Decision::Stay, "已停保持");
        assert_eq!(a.on_time(2 * T, true), Decision::Stay, "停帧零醒（on_time 无动作）");
        // 负面①：非静默（armed/parked/damage）不停。
        let mut b = ar_boot(60);
        assert_eq!(b.on_tick(0, false), Decision::Stay);
        assert!(!b.stopped());
        // 负面②：非资格 intent 不停（Interactive）。
        let mut i = ar_boot(60);
        i.on_vote(0, true, bal(Intent::Interactive as u8, 0, 0, 0));
        assert_eq!(i.on_tick(T, true), Decision::Stay);
        assert!(!i.stopped());
        // 正面②：Unspecified / Background 静默停（评估含 A7 前置——投票当拍
        // 即停，后继拍 Stay 幂等）。
        let mut u = ar_boot(60);
        assert_eq!(u.on_vote(0, true, bal(Intent::Unspecified as u8, 0, 0, 0)), Decision::StopFrame);
        assert!(u.stopped());
        assert_eq!(u.on_tick(T, true), Decision::Stay);
        let mut bg = ar_boot(60);
        assert_eq!(bg.on_vote(0, true, bal(Intent::Background as u8, 0, 0, 0)), Decision::StopFrame);
        assert!(bg.stopped());
        // A7 先于迟滞：驻留中的票面静默化 → 停帧清驻留，恢复后重锚满窗
        //（Background 资格票——Maintenance 要 30Hz 不入停帧集）。
        let mut h = ar_boot(60);
        h.on_vote(0, false, bal(Intent::Background as u8, 0, 0, 0)); // down(30, 0)
        assert_eq!(h.on_tick(2_000_000_000, true), Decision::StopFrame, "静默即停（驻留 2s 已满也先停）");
        assert_eq!(h.on_activity(2_000_000_000), Decision::Resume);
        assert_eq!(h.on_time(2_500_000_000, false), Decision::Stay, "恢复后重锚 500ms < 1s");
        assert_eq!(h.on_time(3_500_000_000, false), Decision::Switch { hz: 30, src: Src::Dwell });
        // 活动恢复：Resume 一次，后续活动 Stay。
        let mut r = ar_boot(60);
        r.on_tick(0, true); // stop
        assert_eq!(r.on_activity(T), Decision::Resume);
        assert_eq!(r.on_activity(2 * T), Decision::Stay);
        // 票面破坏停帧：停帧态投 Interactive 票 → Resume（单决策面——升档判据
        // 随后继拍积累）。
        let mut v = ar_boot(60);
        v.on_tick(0, true); // stop
        assert_eq!(
            v.on_vote(T, true, bal(Intent::Interactive as u8, 0, 0, 0)),
            Decision::Resume
        );
        assert_eq!(v.hz(), 60, "恢复不改档");
        assert_eq!(v.on_tick(2 * T, true), Decision::Stay);
        assert_eq!(v.on_tick(3 * T, true), Decision::Switch { hz: 120, src: Src::Vote });
    }

    #[test]
    fn arb_a1_ballot_lifecycle() {
        // 单槽替换：后票全量代前票（Utility → Interactive pref 240）。
        let mut a = ar_boot(60);
        a.on_vote(0, false, bal(Intent::Utility as u8, 0, 0, 0)); // target 60 == cur
        a.on_vote(T, false, bal(Intent::Interactive as u8, 0, 0, 240));
        assert_eq!(a.on_tick(2 * T, false), Decision::Stay);
        assert_eq!(a.on_tick(3 * T, false), Decision::Switch { hz: 240, src: Src::Vote });
        // 断连撤票：回落 Platform boot 档（驻留路径）。
        assert_eq!(a.on_disconnect(4 * T, false), Decision::Stay);
        assert_eq!(
            a.on_time(4 * T + T_DOWN_DWELL_NS, false),
            Decision::Switch { hz: 60, src: Src::Dwell }
        );
        // sticky v1 同撤：断连清票无差别（多内容面归 §6.4 刀）。
        let mut s = ar_boot(60);
        s.on_vote(0, false, Ballot { sticky: true, ..bal(Intent::Interactive as u8, 0, 0, 120) });
        s.on_tick(T, false);
        s.on_tick(2 * T, false); // 120
        assert_eq!(s.on_disconnect(3 * T, false), Decision::Stay);
        assert_eq!(
            s.on_time(3 * T + T_DOWN_DWELL_NS, false),
            Decision::Switch { hz: 60, src: Src::Dwell },
            "sticky 票断连同撤"
        );
    }

    #[test]
    fn arb_a9_switch_rejected_rollback() {
        // 升路被拒：回滚真值档 + 清 streak → 重新满 2 拍再发。
        let mut a = ar_boot(60);
        a.on_vote(0, false, bal(Intent::Interactive as u8, 0, 0, 120));
        a.on_tick(T, false);
        assert_eq!(a.on_tick(2 * T, false), Decision::Switch { hz: 120, src: Src::Vote });
        a.on_switch_rejected(60); // guifbd：在飞窗拒 → 面板真值 60
        assert_eq!(a.hz(), 60);
        assert_eq!(a.on_tick(3 * T, false), Decision::Stay, "streak 已清");
        assert_eq!(a.on_tick(4 * T, false), Decision::Switch { hz: 120, src: Src::Vote });
        // 降路被拒：重走满窗驻留（commit 已清 down——保守侧重计时）。
        let mut d = ar_boot(60);
        d.on_vote(0, false, bal(Intent::Maintenance as u8, 0, 0, 0));
        assert_eq!(d.on_time(T_DOWN_DWELL_NS, false), Decision::Switch { hz: 30, src: Src::Dwell });
        d.on_switch_rejected(60);
        assert_eq!(
            d.on_time(T_DOWN_DWELL_NS + 100_000_000, false),
            Decision::Stay,
            "被拒重锚 100ms < 1s"
        );
        assert_eq!(
            d.on_time(1_100_000_000 + T_DOWN_DWELL_NS, false),
            Decision::Switch { hz: 30, src: Src::Dwell },
            "重锚（1.1s）满窗再发"
        );
    }

    #[test]
    fn arb_t4_switch_smoothness_stack() {
        // T4 叠层门：SoftPanel + Arbiter 全链——单次升档、seq 连续零丢、ACK
        // 一致、仲裁乐观档与面板生效档收敛（guifbd 同款执行序）。
        let cfg = softpanel::PanelCfg::DEFAULT;
        let mut panel = softpanel::SoftPanel::new(cfg, 60);
        let mut arb = Arbiter::new(&cfg, 60);
        let mut now = 0u64;
        let mut last_seq = 0u64;
        let mut switches = 0u64;
        arb.on_vote(now, false, bal(Intent::Interactive as u8, 0, 0, 120));
        for _ in 0..12 {
            if let Some(rep) = panel.on_point(now) {
                assert_eq!(rep.seq, last_seq + 1, "无故障零丢弃——seq 连续");
                last_seq = rep.seq;
            }
            if let Decision::Switch { hz, .. } = arb.on_tick(now, false) {
                if panel.request_refresh(hz).is_none() {
                    arb.on_switch_rejected(panel.hz());
                } else {
                    switches += 1;
                }
            }
            now += panel.period_ns();
        }
        assert_eq!(switches, 1, "单次升档（无抖动重复切）");
        assert_eq!(panel.hz(), 120, "面板生效 120");
        assert_eq!(arb.hz(), 120, "仲裁乐观档与面板收敛");
        let ack = panel.last_ack().unwrap();
        assert_eq!(ack.new_refresh_ns as u64, 8_333_333, "ACK 周期 = 120Hz");
        assert_eq!(ack.effective_ns, 3 * T, "生效拍 = 受理后第 2 拍标称点");
        assert_eq!(last_seq, 12, "12 拍全投递（T4 丢 vblank = 0 ≤ 1）");
    }

    // —— 刀6 纯核：P6 逐帧扫描 + A8 LFC ——

    /// A8 专用构造（vrr 位 × 模型 × boot 档可注——lfc_capable 由 vrr 与
    /// 面板极值折算）。
    fn ar_vrr(vrr: u8, model: u8, boot_hz: u16) -> Arbiter {
        Arbiter::new(
            &softpanel::PanelCfg {
                vrr,
                model,
                ..softpanel::PanelCfg::DEFAULT
            },
            boot_hz,
        )
    }

    #[test]
    fn softpanel_p6_scan_point() {
        // P6 逐帧拍点纯函数钉板：地板 = 上一扫描 + min_interval（480Hz 帧率
        // 帽），target 迟于地板 = 锁相（panel follows host），target=0 = 尽快
        // 语义退化到地板。
        let p = softpanel::SoftPanel::new(softpanel::PanelCfg::DEFAULT, 60);
        assert!(!p.per_frame(), "缺省 cfg 非逐帧形态");
        assert_eq!(p.min_interval_ns(), 2_083_333, "min_interval = 480Hz 周期");
        assert_eq!(p.max_interval_ns(), 1_000_000_000, "max_interval = 1Hz 自刷新窗");
        let mi = p.min_interval_ns();
        assert_eq!(p.scan_point(10_000_000_000, 10_000_000_001), 10_000_000_000 + mi);
        assert_eq!(
            p.scan_point(10_000_000_000, 10_000_000_000 + 5 * mi),
            10_000_000_000 + 5 * mi,
            "target 迟于地板 → 锁相"
        );
        assert_eq!(p.scan_point(10_000_000_000, 0), 10_000_000_000 + mi);
        // 饱和防御：last 近 u64 峰值不回绕。
        assert_eq!(p.scan_point(u64::MAX - 1, 0), u64::MAX);
    }

    #[test]
    fn softpanel_p6_det_replay() {
        // T2 P6 变体：同扫描墙钟链 → 同 (seq, ts, refresh) 逐位相等；DET 虚
        // 拟钟 ts = det_base + 累进墙距（非墙钟直通、非固定网格——逐帧锁相
        // 语义，Δts = 墙钟帧距）。
        let mk = || {
            let mut cfg = softpanel::PanelCfg::DEFAULT;
            cfg.mode = softpanel::MODE_DETERMINISTIC;
            cfg.per_frame = true;
            cfg.seed = 7;
            softpanel::SoftPanel::new(cfg, 60)
        };
        // 26fps 帧距墙钟链（38_461_538/…_539ns）。
        let walls = [1_000_000_000u64, 1_038_461_538, 1_076_923_076, 1_115_384_615];
        let mut a = mk();
        let out_a: Vec<_> = walls.iter().map(|w| a.on_scan(*w).unwrap()).collect();
        let mut b = mk();
        let out_b: Vec<_> = walls.iter().map(|w| b.on_scan(*w).unwrap()).collect();
        assert_eq!(
            out_a.iter().map(|r| (r.seq, r.ts_ns, r.refresh_ns)).collect::<Vec<_>>(),
            out_b.iter().map(|r| (r.seq, r.ts_ns, r.refresh_ns)).collect::<Vec<_>>(),
            "同输入 → 逐位相等（T2）"
        );
        assert_eq!(out_a[0].seq, 1);
        // det_base(seed=7) = 1e9 + 7×6_700_417 ≠ 首墙钟——DET 虚拟钟非直通。
        assert_eq!(out_a[0].ts_ns, 1_046_902_919);
        for i in 1..walls.len() {
            assert_eq!(out_a[i].ts_ns - out_a[i - 1].ts_ns, walls[i] - walls[i - 1]);
        }
        // 切档重锚透明性：第 2 扫后受理 120Hz → 第 4 扫生效（受理后第 2
        // 拍），refresh 自该拍报新周期；重锚（基 = 本拍 ts、累进清零）后
        // Δts 仍 = 墙钟帧距——钟链不断不跳。
        let mut c = mk();
        let mut ts_chain = Vec::new();
        for (i, w) in walls.iter().enumerate() {
            if i == 2 {
                assert!(c.request_refresh(120).is_some());
            }
            let r = c.on_scan(*w).unwrap();
            ts_chain.push(r.ts_ns);
            if i == 3 {
                assert_eq!(r.refresh_ns, 8_333_333, "生效拍起报 120Hz 周期");
                assert_eq!(c.last_ack().unwrap().effective_ns, r.ts_ns);
            } else {
                assert_eq!(r.refresh_ns, 16_666_667);
            }
        }
        for i in 1..ts_chain.len() {
            assert_eq!(ts_chain[i] - ts_chain[i - 1], walls[i] - walls[i - 1], "重锚零扰动");
        }
    }

    #[test]
    fn arb_a8_lfc_decision_table() {
        // A8 决策表：正例（Tiered 档对齐 / FreeRange 区间直取 + 稳态 Stay
        // + 回升复位）× 负例（vrr 门 / 越界禁用 / 档空禁用 / 无票 / 内容
        // 0）——禁用两路各出告警记号（读即清）。
        // ① Tiered 正例：boot 120、G=[60,120]，内容 30 → k=2 → 档对齐
        // ≥60 的最小产品档 60（面板随内容降频——LFC 降档主场景）。
        let mut a = ar_vrr(PANEL_VRR_CAPABLE, RATE_MODEL_TIERED, 120);
        a.on_vote(0, false, bal(Intent::Interactive as u8, 60, 120, 120));
        assert!(a.lfc_capable());
        assert_eq!(a.on_content_cadence(30), Decision::Switch { hz: 60, src: Src::Lfc });
        assert_eq!(a.lfc_k(), 2);
        assert_eq!(a.hz(), 60);
        assert_eq!(a.on_content_cadence(30), Decision::Stay, "稳态同节拍不重复切");
        assert_eq!(a.lfc_k(), 2);
        assert_eq!(a.on_content_cadence(90), Decision::Stay, "内容 ≥ G_min → 无关 LFC");
        assert_eq!(a.lfc_k(), 0, "回升复位 k");
        // ② Tiered 低价内容：G=[30,120]、内容 24 → k=2 → 48 → 对齐 60。
        let mut b = ar_vrr(PANEL_VRR_CAPABLE, RATE_MODEL_TIERED, 120);
        b.on_vote(0, false, bal(Intent::Utility as u8, 30, 120, 60));
        assert_eq!(b.on_content_cadence(24), Decision::Switch { hz: 60, src: Src::Lfc });
        assert_eq!(b.lfc_k(), 2);
        // ③ vrr 门负例：NONE / SUPPORTED_OFF 恒 Stay 零迁移。
        let mut n = ar_vrr(PANEL_VRR_NONE, RATE_MODEL_TIERED, 120);
        n.on_vote(0, false, bal(Intent::Interactive as u8, 60, 120, 120));
        assert!(!n.lfc_capable());
        assert_eq!(n.on_content_cadence(30), Decision::Stay);
        assert_eq!(n.lfc_k(), 0);
        let mut o = ar_vrr(PANEL_VRR_SUPPORTED_OFF, RATE_MODEL_TIERED, 120);
        o.on_vote(0, false, bal(Intent::Interactive as u8, 60, 120, 120));
        assert_eq!(o.on_content_cadence(30), Decision::Stay);
        // ④ 整数倍越 G_max：G=[60,60]、内容 50 → k=2 → 100 > 60 → 禁用
        // + 告警（振荡护栏——单发读即清）。
        let mut w = ar_vrr(PANEL_VRR_CAPABLE, RATE_MODEL_TIERED, 60);
        w.on_vote(0, false, bal(Intent::Interactive as u8, 60, 60, 60));
        assert_eq!(w.on_content_cadence(50), Decision::Stay);
        assert!(w.take_warn_lfc_off());
        assert!(!w.take_warn_lfc_off(), "告警读即清");
        // ⑤ 档空禁用：G=[160,190] 全落测试档带（非 DET 产品族空）→ 内容
        // 90 → k=2 → 180 ∈G 但 TEST 禁入 → 告警 Stay。
        let mut e = ar_vrr(PANEL_VRR_CAPABLE, RATE_MODEL_TIERED, 120);
        e.on_vote(0, false, bal(Intent::Interactive as u8, 160, 190, 180));
        assert_eq!(e.on_content_cadence(90), Decision::Stay);
        assert!(e.take_warn_lfc_off(), "档空禁用告警");
        // ⑥ 无票：G=[30,480]（平台底线 × 面板极值），内容 40 ≥ 30 无关；
        // 内容 25 → k=2 → 50 → 对齐 60。
        let mut f = ar_vrr(PANEL_VRR_CAPABLE, RATE_MODEL_TIERED, 120);
        assert_eq!(f.on_content_cadence(40), Decision::Stay);
        assert_eq!(f.lfc_k(), 0);
        assert_eq!(f.on_content_cadence(25), Decision::Switch { hz: 60, src: Src::Lfc });
        assert_eq!(f.lfc_k(), 2);
        // ⑦ FreeRange 正例：区间直取无档对齐（k=ceil(60/35)=2 → 70Hz）；
        // 内容 0 = 无内容复位。
        let mut g = ar_vrr(PANEL_VRR_CAPABLE, RATE_MODEL_FREERANGE, 120);
        g.on_vote(0, false, bal(Intent::Interactive as u8, 60, 120, 120));
        assert_eq!(g.on_content_cadence(35), Decision::Switch { hz: 70, src: Src::Lfc });
        assert_eq!(g.lfc_k(), 2);
        assert_eq!(g.hz(), 70);
        assert_eq!(g.on_content_cadence(0), Decision::Stay, "内容 0 复位不迁移");
        assert_eq!(g.lfc_k(), 0);
    }
}
