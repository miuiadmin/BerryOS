# GUI-3 实施设计草案 —— berry-ui 保留树 + 脏区引擎内核（视图 crate）

> 版本：**1.0**（v1.0）｜2026-11-06

> 状态：**契约冻结**（本文 §6 即 impl 契约——workflow impl 逐字照此落码，改契约先改本文）。
> 钦定源：`docs/GUI里程碑规划-2026-10-v1.0.md` GUI-3 行 + ADR #52/#53 + GUI-10 增件 +3。
> 前置侦察：三路 recon（berry-gfx 消费面 14 坑 / berry-guiproto wire 对齐 20 条 / research 报告 29 建议与 R1–R10 反面清单）已回收，逐条吸收见 §4/§5。

## §0 范围裁定

- **工作区**：新建 `user/berry-ui/`（user workspace member + justfile test-host 单行注册）；纯 host crate，**内核零接触、smoke 零注册、QEMU 零依赖**——与 S-2b SMP 线文件面互斥（规划 :97 并行纪律），本刀不碰 smoke.sh。
- **单刀全量不分段**：host 纯函数面一体验收（GUI-4 段②的「涉 smoke 主线」债不在本刀——真消费者接驳归 GUI-5/9）。
- **风格铁律**（berry-gfx 家族同款）：`#![cfg_attr(not(test), no_std)]`；**全库零 alloc、零 panic**（一切失败 loud `Err`；敌意几何防御采样/checked/饱和——`GlyphBits::bit` 先例）；定容数组满 → loud Err 绝不静默丢；英文标识 + 中文头注；错误枚举 derive `Debug+Clone+Copy+PartialEq+Eq`（值级断言面）。
- **坐标三套显式换算**（gfx 坑 10）：内部几何 i32（`RectI`）／damage 面与输入面 u32（guiproto `Rect` 同口径）／容量 usize——跨界一律显式转换 + checked，禁裸 `as`。

## §1 立项依据

- **ADR #52**：自研 berry-ui 声明式框架——API 面数据驱动（struct 属性/enum 事件/Msg-only 回调/描述结构可序列化双入口）、引擎内核保留树+脏区、「声明即数据」AST 导出 day-one；v1 失效粒度视图级、属性级留缝 v1.5。
- **ADR #53**：三态停帧 RUN/SETTLE/IDLE（IDLE 撤定时器睡事件）；FPS expected/actual 双轨；vote 区间票面留位（接口先活、消费后到）；起振 1–2 帧延迟进遥测。
- **规划 :89 钦定面**：脏区合并 + Measure/Render 保留；软 vsync request-driven + 周期协商倒数；输入与帧同拍 doFrame；动画属性 Transform/Layout 分型。
- **GUI-10 增件 +3**：①属性级依赖记录骨架 day-one 留缝（dep.rs 只记录不消费）；②三态起振延迟遥测；③overdraw 预算断言 ≤2.5× 屏幕像素——**遮挡剔除与矩形合并同级验收**。
- **research 收敛点**（GUI-07/09/10）：保留模式四报告共决（空闲零重绘 = 低功耗身份支柱）；四家失效粒度收敛于属性级（v1 视图级不跳步）；P4 帧循环需求驱动 / P7 输入同拍+消费侧重采样 / P8 双轨打点 / P9 帧路径零分配；P12 crate 化 host 可离屏测 = AI 测试友好化。
- **验收 = host 测三件**（规划 :89）：脏区合并代数 / 三态停帧状态机 / 离屏渲染对拍。

## §2 拓扑

```
                 ┌────────────────────────────────────────────┐
                 │ user/berry-ui（本刀——引擎内核纯函数 crate） │
                 │  Ui 门面：保留树 arena + 布局 + 脏区 + 渲染  │
                 │  + 输入 referee + 动画表 + 三态帧循环 + 遥测 │
                 └──────┬───────────────────────┬──────────────┘
      path 依赖（消费）  │                       │  path 依赖（常量/类型单源，非收发）
  ┌─────────────────────▼────────┐   ┌──────────▼──────────────────┐
  │ berry-gfx（GUI-2 段①②）      │   │ berry-guiproto（GUI-4 段①） │
  │ CommandEncoder/DisplayList/  │   │ Rect(u32)/DAMAGE_MAX=8/     │
  │ Color/GlyphPos/FontSize/     │   │ RESAMPLE_* 四常量/SCENE_*   │
  │ GlyphAtlas(&'static 三档)    │   │ （wire 收发归 embedder）     │
  └──────────────────────────────┘   └─────────────────────────────┘
                        ▲
   guest 侧 embedder（GUI-5 gui-wm / gfxdemo——挂账不在本刀）：
   guiproto 编解码 × channel 会话 + fb VMO 映射 + submit 驱动
   host 侧 embedder = tests/（离屏 [u8] 纹理 + golden 对拍）
```

- 引擎**只产出** damage 与 DisplayList、**只消费** typed 事件——wire 编解码/channel/VMO 全在 embedder（拓扑对齐条 19：引擎不假设直摸 vblank/hdi 域）。
- 单线程语境立法（gfx LruCache 同款）：`Ui` 一律 `&mut self` 方法，无 Cell 无 Sync。

## §3 逐件设计（11 模块）

### 3.1 arena（lib.rs + arena.rs）
- `Ui<const N: usize>`：`nodes: [Option<Node>; N]` + freelist（首个空槽扫描或显式链——N≤256 线性可受，但用显式 free 链防「分配器线性扫骰子」K-1 雷同型）。
- `NodeId = u32` 打包 `gen<<16 | idx`（gen u16 代数防 use-after-free 误用——remove 后旧 id 再用 → `UiError::NodeBad` loud）。
- 树形态：单根 Root 节点（viewport 持有者）+ 每节点 `parent/first_child/next_sib/prev_sib` 槽索引（u32，`NONE=u32::MAX`）。
- `remove` 连子树：先收子树旧绝对界进 damage，再整族释放。

### 3.2 desc（desc.rs）——数据驱动描述 + AST 双入口
- `ViewKind` 六件：`Root/Rect/Text/Col/Row/Pad`（v1 视图集——Rect/Text 叶子、Col/Row/Pad 容器；叶子带子 = AST 拒面）。
- Props 全 struct/enum Plain Data（Copy）；`TextBuf { bytes: [u8; TEXT_INLINE=64], len: u8 }` 内联文本（no_std 零 alloc——超长 `UiError::TextTooLong` loud）。
- `ViewSpec { kind, key: u64, props }` builder 入口传参值（key 0 = 无 key）。
- **AST 二进制格式**（「声明即数据」day-one 硬要求；文本 DSL 归 GUI-8 挂账）：前序 TLV 流 `node := kind u8 | flags u8(bit0=has_key，其余位拒) | key u64 LE? | props(kind 定长：Root 0B / Rect 11B / Text ≤69B / Col·Row·Pad 4B) | child_count u8`；尾随字节拒、未知 kind 拒、截断拒——guiproto 同款拒面纪律。`encode_desc/apply_desc`（apply = 整树重建 + FULL damage）= 双入口第二入口；roundtrip 字节相等为验收。

### 3.3 layout（layout.rs）——v1 内建摆位 + Measure/Render 保留
- 单遍自顶向下：容器（Col/Row 累进游标 + gap；Pad 内缩）分配子界；叶子尺寸 = 定值（`Option<u32>`，None 随父）或内在（Text = `len × cell_w × cell_h`，档位几何 `FontSize::cell()` 单源——ch u8 字节推进语义，gfx 坑 11）。
- 缓存：每节点存内在尺寸 + `layout_dirty`；改色/改 paint 位 → 0 重排（`LayoutStat.measured` 可断言缓存有效性——验收点「Measure/Render 保留」）。
- **布局算法插拔缝不在本刀**：LayoutEngine trait 留 GUI-6 定形（避免先造错缝——克制纪律同「不引入运行时信号图」）；v1 内建算法文档明示挂账。

### 3.4 damage（damage.rs）——验收①主体
- `RectI { x,y,w,h: i32 }` 内部几何 + `DamageSet { rects: [guiproto::Rect; DAMAGE_MAX], count: u8, full: bool }` 产出面。
- **Σ×1.25 退化规则**（GUI-4 草案 §3.5-4 客户端义务，判定式进本模块、阈值常量唯一在此定义）：合并对 (a,b) 允许 iff `包络面积×4 ≤ (a.area+b.area)×5`（u64 中转；严格大于即拒并）。数学性质（2026-10-31 实施刀勘误——原文「相交矩形恒可并（包络 < Σ）」为过强断言：L 形薄条角点对 (0,0,100,1)+(0,0,1,100) 包络 10000 > Σ 200 证伪，可并判定只认浪费帽）：**并集为矩形的相交形（包含/十字/边贴）恒可并**（包络=并集 ≤ Σ）；L 形角交包络可超 Σ、一律走浪费帽裁决——**无相交特判**，测试钉板两条（矩形并形必并 + L 形值级拒并）。判定常量 `MERGE_WASTE_NUM=5 / MERGE_WASTE_DEN=4` 带草案出处注记。
- add 流程：先 clip 到 viewport（负界/零面积 no-op 丢弃——gfx 坑 5 的 max(0,·) 收口在此）；与既有矩形试并（优先相交者）；`count<8` 直挂；=8 时找最小包络可并对并；无可并可并 → `full=true` 清矩形集。**绝不产出 K=0 且非 FULL 的空集语义**（二取一：对齐条 3）。
- `RectSet<const N: usize>`（不相交矩形集，补集表示）：`subtract/union_area/intersects`——遮挡剔除的几何底座（N=16）。

### 3.5 paint（paint.rs）——验收③主体 + 遮挡剔除
- **前到后剔除、后到前发射**（OH ui_lite 同路）：按 damage 矩形逐个走树，维护 `RectSet` 已覆盖集（**仅 Rect 叶子（实心不透明）计遮挡源**——Text 蜂格有洞不计，保守正确）；节点可见区 = `bounds ∩ damage − covered` 为空 → skip（`culled++`）；否则记入发射序。
- 发射序 = 画家算法背到前（后者盖前者——v1 无 alpha 无 blend，gfx 坑 7 立法）；逐节点：Rect → `fill_rect`；Text → GlyphPos run 组进调用方 scratch → `draw_glyph_run`（bg=None 透明底）。尾部必录 `Present{ damage 包络 }`（soft no-op 也要录——真后端零改语义，gfx 坑 8）。
- **overdraw 计数**：`drawn_px`（实际发射面积 ∩ viewport）×256 / viewport 面素 = `overdraw_x256`；预算断言 `≤640`（= 2.5×256，整数比较无浮点）。测试守卫真伪自检：对照场景关剔除必超预算（consensus.rs:12「故意构造破坏形态断言必不相等」同法）。
- **借用形态**（gfx 坑 1/2 对策）：paint 收 `&self` + 调用方 `scratch: &'a mut [GlyphPos]` + `&mut CommandEncoder<'a>`——DisplayList 的 `'a` 钉在调用方 scratch 与 `'static` 图集上，**不钉在引擎上**（树与列表解耦，无自引用）。`FontSize::atlas()` 返 `&'static dyn GlyphAtlas` 单源。

### 3.6 hit + 手势（hit.rs）
- `node_at(x,y)`：topmost 优先（z 序逆走），`RectI::contains` 含 paint 偏移（Transform 动画后命中随视觉走）；v1 无 clip 视图（挂账）。
- **手势 referee**（每 pointer_id 独立状态 {down_node, down_xy, dragging}）：DOWN→hit→`Press`；MOVE 距按下点 > `DRAG_SLOP_PX=8` → `DragStart`，此后 `DragMove`；UP→在拖 → `DragEnd`，不在 → `Release`+`Click`；**CANCEL 终止手势不补 UP**（对齐条 14）——`Cancel`/`DragEnd`。Msg-only：引擎产 `MsgBuf`（定容 8 loud 满），app 的 update 消费——引擎内零闭包零回调副作用。
- 多指：pointer_id = 身份键（device_id v1 恒 0 勿读——对齐条 14）。

### 3.7 anim（anim.rs）——Transform/Layout 分型 day-one
- `Anim { node, prop: AnimProp, from/to: i32, start_ns, dur_ns, easing }`；`AnimProp::{OffsetX, OffsetY}` = **Transform 型**（paint 偏移，不重排——damage = 新旧界包络）；`{Width, Height, Gap}` = **Layout 型**（改 props + layout_dirty 重排）。分型为 v2 服务端动画/合成留缝（钦定面）。
- 定容表 `[Anim; ANIM_MAX=16]` loud 满；tick 在帧内推进，完成项移除；`anim_active()` 喂帧循环信号。

### 3.8 frame（frame.rs）——验收②主体 + 软 vsync
- **三态纯函数状态机**：`FramePhase::{Run, Settle, Idle}`；`FrameSignals { damage, anim, input, present_pending }`；`next_phase(cur, sig)` 真值表：
  - 任一信号真 → `Run`（保持/升级）。
  - 全假但 `present_pending`（在途帧回调未清）→ `Settle`。
  - 全假且无在途 → `Idle`。
  - 静默强降：Run 态 `now − last_work_ns > IDLE_AFTER_MS=2000`（OH 常量）→ Idle 兜底。
- **IDLE = `next_wakeup() == None`**（撤软 vsync 定时器、睡事件——R3 反面清单对策）；`wake(now)` → Run + **立即渲不欠拍** + 起振遥测（`idle_exits++/wakes++`——Weston「extra vblank」1–2 帧延迟可观测，增件②）。
- **request-driven 落法**：引擎不持定时器——`next_wakeup() -> Option<u64>`（绝对 ns）交 embedder 睡/等事件（GUI-10 ⑤ 形态的最薄实现；专用 fd/SPSC 环归 guest embedder 挂账）。
- **周期 = 协商倒数**：`set_vsync(period_ns, phase_ns)`（embedder 按仲裁结果注入；vote 引擎只产票不仲裁——对齐条 12）。UI/合成错相 = phase 参数注入位（真后端验证挂 S4）。
- **doFrame 序列**（P7 输入同拍）：`pump_input(target_ns) -> MsgBuf`（帧头采样消费：referee + 重采样）→ app update 改树 → `render_frame(now, scratch, enc) -> FrameOutcome`（anim tick → layout → paint → 遥测 → 相位推进）。两段拆分让 app 的 update 落在输入消费与绘制之间且零回调。
- `FrameOutcome { drew: bool, damage, expected_present_ns, rate_hint_hz: u8 }`——`drew=false`（damage 空）**不提交**（「无 damage 不提交」+ IDLE 零 SUBMIT 零 FRAME_CALLBACK，对齐条 6）。`expected_present_ns = next_vsync_tick(now)`（未来时刻表 expected 轨）。
- **credits 背压簿记**：`note_submitted()` / `note_presented(seq, ns, status, credits_remaining)`（actual 轨 + SETTLE→IDLE 确认 + jank 判定 `actual > expected + period/2` → `deadline_miss++`）。

### 3.9 resample（resample.rs）——消费侧最终一跳
- 纯函数 `resample_to_target(hist: &[Sample], target_ns) -> Option<Sample>`，**语义逐条对齐 gui-inputd `resample_to_vsync`**（I:168）：<2 采样/非单调/首尾同 ts → None；跨度 <2ms 或 >20ms → None；括点线性插值；前向外推帽 `min(Δ/2, 8ms)` 帽沿截停、ts 诚实报 vsync；无后向外推；撞 ts 取新采样。**常量 import guiproto 单源**（5/2/20/8ms——对齐条 16，不长第二套）。
- 目标推导：`target = expected_present_ns − RESAMPLE_LATENCY_MS`（Android 同式）；引擎在 pump_input 内对活跃 MOVE 流应用，合成采样 `resampled` 标记随 Msg 透出。

### 3.10 dep（dep.rs）——属性级依赖记录骨架（增件①）
- `DepId(pub u32)`（app 状态属性句柄）+ 定容边表 `[Option<(NodeId, DepId)>; DEP_EDGES_MAX=64]`；`dep_bind` 记录 / `dep_dependents` 查询。**v1 不消费**（失效仍视图级 Invalidate——裁决不跳步）；缝的验收 = 形状钉板测试 + v1.5 键路径 diff 对拍挂账（S7 spike 定生死：属性级收益 <20% 帧耗时 → 维持视图级至 v2）。

### 3.11 telemetry（telemetry.rs）——双轨打点（P8）
- `FrameRec { seq, input_ts_ns, frame_begin_ns, frame_end_ns, expected_ns, actual_ns, presented, discarded, damage_full, damage_rects, overdraw_x256, jank }` 环 `[FrameRec; 64]` + 计数器族（`frames/wakes/idle_exits/spinup_late_frames/deadline_miss`）。
- 一份数据喂三消费者：host 断言（本刀 tests）/ 性能工具 / AI 遥测（guest 侧收口归 GUI-9）；IDLE 唤醒 0 / icount≈0 进 smoke 挂 GUI-9 能耗相。

## §4 berry-gfx 消费坑对表（14 条 → 设计落点）

| # | 坑 | 落点 |
|---|---|---|
| 1 | 借用生命周期钉 encoder | §3.5：DisplayList `'a` 钉调用方 scratch + 'static 图集，不钉引擎 |
| 2 | DisplayList 数 KB 栈对象 | 每帧 encoder 由 embedder 建；引擎不囤列表（paint 即录即还） |
| 3 | Cell 非 Sync / 渲染中重入 | §2 立法：全 &mut、单线程、无 Cell |
| 4 | 录成功 ≠ submit 过 | 诊断面两层：UiError vs SubmitError 分呈（embedder 职责注记） |
| 5 | 零尺寸 no-op / 负界错误 | §3.4：damage 入口 clip + 零面积丢弃；负 w/h 内部恒非法（RectI 构造收口） |
| 6 | dirty 只管写不管读 | 本刀无 blit 消费；滚动搬运挂账时回读此条 |
| 7 | 无 alpha 无 blend | §3.5 画家算法不透明覆盖；遮挡剔除恰因此成立 |
| 8 | Present 照录 | §3.5 尾命令恒 Present{damage 包络} |
| 9 | stride/need_bytes | 离屏测试缓冲按 `need_bytes()` 开（末行无 padding） |
| 10 | 坐标三套 | §0 换算铁律；测试断言含跨界样本 |
| 11 | ch=u8 字节推进/缺字形空格 | §3.3 Text 内在尺寸按字节×cell；渲染语义随 gfx（不改） |
| 12 | VMO 纹理留位 | embedder 接驳职责（guest 挂账 GUI-5） |
| 13 | Device/Queue 形态 | embedder 持有；引擎零 gfx 会话态 |
| 14 | 测试方法论 | §8 全套复用（哨兵 0xEE/golden 两级/拒面值级/零半执行/守卫真伪自检） |

## §5 wire 对齐表（对齐条 → 落点）

关键条（全 20 条见 recon 归档）：#1 damage u32 整数物理像素 Y 向下（坐标系即布局/脏区系，无 Q16.16）；#2–4 K≤8 收敛 + 二取一 + Σ×1.25 引擎侧义务（§3.4）；#6 IDLE 零 SUBMIT 零 FRAME_CALLBACK（§3.8 drew=false 不提交）；#7 双轨数据源 target_present_ns/presented_ns（§3.8/§3.11）；#8 credits≤2 簿记不轰炸（§3.8）；#13–16 输入口径/哨兵折叠/常量单源/最终一跳（§3.6/§3.9）；#12 rate_hint 每帧必填（FrameOutcome）；#19 只持 present+input 端点（§2 拓扑）；R1–R10 反面清单逐条过堂：R3 三态停帧 ✓ / R5 脏区 day-one ✓ / R7 帧率协商倒数 ✓ / 其余条目以挂账或 embedder 职责注记明示（评审 R1 闸门项）。

## §6 钉板接口（冻结——逐字照此落码）

```rust
// ===== lib.rs =====
#![cfg_attr(not(test), no_std)]
pub mod anim; pub mod damage; pub mod dep; pub mod desc; pub mod frame;
pub mod hit; pub mod layout; pub mod paint; pub mod resample; pub mod telemetry;
// arena 内嵌 lib.rs（Node/槽表私有），NodeId 在 desc.rs 公开。

pub const MAX_NODES: usize = 256;
pub const TEXT_INLINE: usize = 64;
pub const MSG_MAX: usize = 8;
pub const ANIM_MAX: usize = 16;
pub const INPUT_BUF_MAX: usize = 8;
pub const DEP_EDGES_MAX: usize = 64;
pub const DRAG_SLOP_PX: i32 = 8;
pub const OVERDRAW_BUDGET_X256: u32 = 640;   // 2.5×256（GUI-10 反清单 7）
pub const IDLE_AFTER_MS: u64 = 2000;         // OH 常量：静默强降 IDLE
pub type UiMain = Ui<MAX_NODES>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiError {
    NodeFull,                       // arena 定容满——loud
    NodeBad,                        // 代数不匹配（悬空 NodeId）/坏槽
    BadParent,                      // 容器位传叶子 / Root 不能为子
    TextTooLong { len: usize },     // > TEXT_INLINE
    AnimFull, DepFull, MsgFull,     // 定容族
    NoRoot,                         // 需先 root()
}

// 节点内部形态（impl 三方共享——冻结字段，pub(crate)）
pub(crate) struct Node {
    pub gen: u16, pub alive: bool,
    pub kind: desc::ViewKind, pub key: u64, pub props: desc::Props,
    pub parent: u32, pub first_child: u32, pub next_sib: u32, pub prev_sib: u32, // 槽索引；NONE = u32::MAX
    pub x: i32, pub y: i32, pub w: i32, pub h: i32,     // 父相对摆位 + 尺寸（layout 产物）
    pub meas_w: u32, pub meas_h: u32,                    // 内在尺寸缓存（Measure 保留）
    pub layout_dirty: bool, pub paint_dirty: bool,
    pub ox: i32, pub oy: i32,                            // paint 偏移（Transform 动画）
}

pub struct Ui<const N: usize> { /* nodes/freelist/damage/input_buf/anims/frame 态/telem/dep 边/vsync/credits —— 私有 */ }

impl<const N: usize> Ui<N> {
    pub fn new(viewport_w: u32, viewport_h: u32) -> Self;
    pub fn set_viewport(&mut self, w: u32, h: u32);            // FULL damage + 全量重排
    pub fn viewport(&self) -> (u32, u32);

    // —— 树构造（builder 入口①）——
    pub fn root(&mut self) -> NodeId;                          // 单根幂等
    pub fn add(&mut self, parent: NodeId, spec: desc::ViewSpec) -> Result<NodeId, UiError>;
    pub fn remove(&mut self, node: NodeId) -> Result<(), UiError>;   // 连子树；damage 旧绝对界
    pub fn set_props(&mut self, node: NodeId, props: desc::Props) -> Result<(), UiError>; // 几何变→重排脏+damage 新旧界；纯色变→paint 脏
    pub fn set_text(&mut self, node: NodeId, bytes: &[u8]) -> Result<(), UiError>;
    pub fn invalidate(&mut self, node: NodeId) -> Result<(), UiError>; // 视图级重绘（old+new 界进 damage）
    pub fn bounds(&self, node: NodeId) -> Result<damage::RectI, UiError>; // 绝对界（含 ox/oy）
    pub fn kind_of(&self, node: NodeId) -> Result<desc::ViewKind, UiError>;

    // —— 布局 / 脏区 ——
    pub fn layout(&mut self) -> layout::LayoutStat;            // 脏子树重排；新界进 damage
    pub fn damage(&self) -> &damage::DamageSet;
    pub fn drain_damage(&mut self) -> damage::DamageSet;

    // —— 渲染（DisplayList 'a 钉调用方 scratch，不钉引擎）——
    pub fn paint<'a>(&self, scratch: &'a mut [berry_gfx::GlyphPos],
                     enc: &mut berry_gfx::CommandEncoder<'a>) -> paint::PaintReport;

    // —— 输入 ——
    pub fn push_pointer(&mut self, ev: hit::PtrEvent) -> Result<(), UiError>;  // 缓冲 ≤INPUT_BUF_MAX（满 UiError::MsgFull——不自陷不静默丢）
    pub fn pump_input(&mut self, expected_present_ns: u64) -> desc::MsgBuf;    // 帧头：referee + 重采样 + 消费
    pub fn node_at(&self, x: u32, y: u32) -> Option<NodeId>;

    // —— 动画 ——
    pub fn anim_add(&mut self, a: anim::Anim) -> Result<(), UiError>;
    pub fn anim_clear(&mut self);
    pub fn anim_active(&self) -> bool;

    // —— 帧循环 ——
    pub fn set_vsync(&mut self, period_ns: u64, phase_ns: u64); // 周期 = 协商倒数；phase = UI/合成错相注入位
    pub fn vsync(&self) -> (u64, u64);
    pub fn render_frame<'a>(&mut self, now_ns: u64, scratch: &'a mut berry_gfx::GlyphPos,
                            enc: &mut berry_gfx::CommandEncoder<'a>) -> frame::FrameOutcome; // anim tick→layout→paint→遥测→相位
    pub fn frame_phase(&self) -> frame::FramePhase;
    pub fn next_wakeup(&self) -> Option<u64>;                   // None = IDLE（撤定时器）
    pub fn wake(&mut self, now_ns: u64);                        // IDLE→Run 立即渲不欠拍 + 起振遥测
    pub fn note_submitted(&mut self);                           // credits 簿记（≤CREDITS_MAX）
    pub fn note_presented(&mut self, seq: u32, presented_ns: u64, status: u8, credits_remaining: u8);
    pub fn vote_hint(&self) -> frame::VoteBallot;               // 票面 {min,max,preferred,scene,sticky}——仲裁在显示服务

    // —— AST 双入口（入口②）——
    pub fn encode_desc(&self, out: &mut [u8]) -> Result<usize, desc::AstError>;
    pub fn apply_desc(&mut self, src: &[u8]) -> Result<(), desc::AstError>;   // 整树重建 + FULL damage

    // —— 依赖骨架（v1 记录不消费）——
    pub fn dep_bind(&mut self, node: NodeId, dep: dep::DepId) -> Result<(), UiError>;
    pub fn dep_dependents(&self, dep: dep::DepId, out: &mut [NodeId]) -> usize;

    pub fn telemetry(&self) -> &telemetry::Telemetry;
}

// ===== desc.rs =====
pub struct NodeId(pub(crate) u32);   // Debug+Copy+Eq+Hash；gen<<16|idx

pub enum ViewKind { Root, Rect, Text, Col, Row, Pad }   // Debug+Clone+Copy+PartialEq+Eq

pub struct RectProps { pub bg: Color, pub w: Option<u32>, pub h: Option<u32> }  // None = 随父
pub struct TextStyle { pub fg: Color, pub bg: Option<Color>, pub scale: FontSize }
pub struct TextProps { pub style: TextStyle, pub text: TextBuf }
pub struct ColProps { pub gap: u32 }
pub struct RowProps { pub gap: u32 }
pub struct PadProps { pub all: u32 }

pub enum Props { Root, Rect(RectProps), Text(TextProps), Col(ColProps), Row(RowProps), Pad(PadProps) }  // Clone+Copy

pub struct ViewSpec { pub kind: ViewKind, pub key: u64, pub props: Props }

pub struct TextBuf { pub bytes: [u8; crate::TEXT_INLINE], pub len: u8 }
impl TextBuf {
    pub fn new(bytes: &[u8]) -> Result<Self, crate::UiError>;
    pub fn as_bytes(&self) -> &[u8];
    pub fn is_empty(&self) -> bool;
}

pub enum Msg {
    Press { node: NodeId, x: u32, y: u32 },
    Release { node: NodeId, x: u32, y: u32 },
    Click { node: NodeId },
    DragStart { node: NodeId, x: u32, y: u32 },
    DragMove { node: NodeId, x: u32, y: u32, resampled: bool },
    DragEnd { node: NodeId },
    Cancel { node: NodeId },
}
pub struct MsgBuf { /* [Option<Msg>; MSG_MAX] + n */ }
impl MsgBuf { pub fn iter(&self) -> impl Iterator<Item = &Msg>; pub fn len(&self) -> usize; pub fn is_empty(&self) -> bool; }

pub enum AstError { TooSmall { need: usize, got: usize }, UnknownKind { got: u8 }, UnknownFlags { got: u8 },
    UnknownProp { kind: ViewKind, got: u8 }, Truncated, TrailingBytes { extra: usize }, BadParent, TextTooLong { len: usize } }

pub(crate) fn encode_props(props: &Props, out: &mut [u8]) -> Result<usize, AstError>;
pub(crate) fn parse_node(src: &[u8]) -> Result<(ViewSpec, usize /*消费字节*/), AstError>;

// ===== damage.rs =====
pub struct RectI { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }   // 负 w/h 内部恒非法（构造收口）
impl RectI {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self;      // w/h max(0,·) 收口
    pub fn right(&self) -> i32; pub fn bottom(&self) -> i32;
    pub fn area(&self) -> u64;
    pub fn contains(&self, x: i32, y: i32) -> bool;
    pub fn intersect(&self, other: &RectI) -> RectI;
    pub fn envelope(&self, other: &RectI) -> RectI;
    pub fn is_empty(&self) -> bool;
    pub fn to_wire(&self) -> berry_guiproto::Rect;           // i32→u32：负侧饱和 0（damage 已 clip viewport 故恒 ≥0）
    pub fn from_wire(r: &berry_guiproto::Rect) -> RectI;     // u32→i32 checked（>i32::MAX 饱和）
}
pub const MERGE_WASTE_NUM: u64 = 5;   // Σ×(1+1/4)（GUI-4 草案 §3.5-4——判定式唯一在此）
pub const MERGE_WASTE_DEN: u64 = 4;
pub fn try_merge(a: &berry_guiproto::Rect, b: &berry_guiproto::Rect) -> Option<berry_guiproto::Rect>; // 包络×4 ≤ Σ面积×5（u64）
pub struct DamageSet { pub rects: [berry_guiproto::Rect; berry_guiproto::DAMAGE_MAX], pub count: u8, pub full: bool }
impl DamageSet {
    pub fn new() -> Self;
    pub fn is_empty(&self) -> bool;             // full=false 且 count==0
    pub fn add(&mut self, r: berry_guiproto::Rect);   // 内部全 u64 面积运算
    pub fn add_full(&mut self);
    pub fn iter(&self) -> impl Iterator<Item = &berry_guiproto::Rect>;
    pub fn envelope(&self) -> Option<berry_guiproto::Rect>;  // full → None（整面语义由调用方用 viewport）
    pub fn union_px(&self) -> u64;              // Σ面积（FULL → u64::MAX 语义）
    pub fn clear(&mut self);
}
pub struct RectSet<const N: usize> { /* 不相交矩形集 */ }
impl<const N: usize> RectSet<N> {
    pub fn new() -> Self;
    pub fn add(&mut self, r: RectI) -> bool;           // 满帽 false；入集自动与既有不相交化
    pub fn subtract(&mut self, r: RectI);              // 补集减
    pub fn is_covered(&self, r: &RectI) -> bool;       // r ⊆ 集
    pub fn visible(&self, r: &RectI) -> RectI;         // r − 集 的最大可见片（空 → 零尺寸）
    pub fn area(&self) -> u64;
    pub fn len(&self) -> usize;
}

// ===== layout.rs =====
pub struct LayoutStat { pub measured: u32, pub placed: u32 }   // 缓存有效性断言面
pub(crate) fn relayout(nodes: &mut [Option<crate::Node>], vp_w: u32, vp_h: u32,
                       damage: &mut DamageSet, freed: &[u32]) -> LayoutStat;

// ===== paint.rs =====
pub struct PaintReport { pub cmds: usize, pub drawn_px: u64, pub culled: u32, pub overdraw_x256: u32 }
pub(crate) fn paint(nodes: &[Option<crate::Node>], root: u32, vp: (u32, u32), damage: &DamageSet,
                    scratch: &mut [GlyphPos], enc: &mut CommandEncoder) -> PaintReport;

// ===== hit.rs =====
pub struct PtrEvent { pub phase: u8, pub pointer_id: u16, pub x: u32, pub y: u32,
                      pub ts_ns: u64, pub buttons: u8, pub pressure_mi: u16 }
impl PtrEvent {
    /// wire → 引擎面：pressure 0x8000 哨兵折 500（W3C 0.5——引擎侧义务，对齐条 13）
    pub fn from_wire(phase: u8, pointer_id: u16, x: u32, y: u32, ts_ns: u64, buttons: u8, pressure: u16) -> Self;
}
pub(crate) struct Gesture { /* per-pointer 状态 */ }
pub(crate) fn dispatch(gesture: &mut Gesture, nodes: &[Option<crate::Node>], root: u32, ev: PtrEvent, out: &mut MsgBuf);

// ===== anim.rs =====
pub enum AnimProp { OffsetX, OffsetY, Width, Height, Gap }   // Offset* = Transform 型；其余 Layout 型
pub enum Easing { Linear, StepEnd }
pub struct Anim { pub node: NodeId, pub prop: AnimProp, pub from: i32, pub to: i32,
                  pub start_ns: u64, pub dur_ns: u64, pub easing: Easing }
pub(crate) fn tick(anims: &mut [Option<Anim>], nodes: &mut [Option<crate::Node>],
                   damage: &mut DamageSet, now_ns: u64) -> bool /* anim_active */;

// ===== frame.rs =====
pub enum FramePhase { Run, Settle, Idle }   // Debug+Clone+Copy+PartialEq+Eq
pub struct FrameSignals { pub damage: bool, pub anim: bool, pub input: bool, pub present_pending: bool }
pub fn next_phase(cur: FramePhase, sig: FrameSignals, idle_ms_since_work: u64) -> FramePhase; // 纯函数（验收②）
pub struct FrameOutcome { pub drew: bool, pub damage: DamageSet, pub expected_present_ns: u64, pub rate_hint_hz: u8 }
pub struct VoteBallot { pub min_hz: u16, pub max_hz: u16, pub preferred_hz: u16, pub scene: u8, pub sticky: u8 }

// ===== resample.rs =====
pub struct Sample { pub ts_ns: u64, pub x: i64, pub y: i64 }
pub fn resample_to_target(hist: &[Sample], target_ns: u64) -> Option<Sample>;  // 常量 berry_guiproto 单源

// ===== dep.rs =====
pub struct DepId(pub u32);

// ===== telemetry.rs =====
pub const FRAME_RING: usize = 64;
pub struct FrameRec { pub seq: u32, pub input_ts_ns: u64, pub frame_begin_ns: u64, pub frame_end_ns: u64,
                      pub expected_ns: u64, pub actual_ns: u64, pub presented: bool, pub discarded: bool,
                      pub damage_full: bool, pub damage_rects: u8, pub overdraw_x256: u32, pub jank: bool }
pub struct Telemetry { /* ring + 计数器族 */ }
impl Telemetry {
    pub fn last(&self, n: usize) -> impl Iterator<Item = &FrameRec>;
    pub fn frames(&self) -> u64; pub fn wakes(&self) -> u64; pub fn idle_exits(&self) -> u64;
    pub fn spinup_late_frames(&self) -> u32; pub fn deadline_miss(&self) -> u32;
}
```

依赖（`user/berry-ui/Cargo.toml`）：`berry-gfx = { path = "../berry-gfx" }` + `berry-guiproto = { path = "../berry-guiproto" }`（后者零依赖纯 crate，仅取常量/Rect 单源）；零 dev-dependencies。

## §7 落点与验证

| 文件 | 动作 |
|---|---|
| `user/berry-ui/Cargo.toml` | 新建（guiproto/gfx 头注风格——本文件为钦定源） |
| `user/berry-ui/src/{lib,desc,damage,layout,paint,hit,anim,frame,resample,dep,telemetry}.rs` | 新建 ×11 |
| `user/berry-ui/tests/{damage,frame,offscreen,tree,layout,reconcile,input,anim,resample,dep}.rs` | 新建 ×10 |
| `user/Cargo.toml` | members 追加 `"berry-ui"`（guiproto 行后 + 一行注释） |
| `justfile` | test-host 段 berry-gfx 行后 + 注释 + `cargo test … -p berry-ui --target {{host}}` |
| `docs/GUI-3实施设计草案-v1.0.md` | 新建（本文件） |

验证（gui3 worktree，`CARGO_TARGET_DIR=$HOME/.cargo-target-gui3`）：

```bash
cargo test --manifest-path user/Cargo.toml -p berry-ui --target "$(rustc -vV | sed -n 's/^host: //p')"   # 本刀主验收
cargo build --manifest-path user/Cargo.toml -p berry-ui --target aarch64-unknown-none                     # no_std 消费面
just test-host                                                                                            # 全量不回归（host 665 → 665+N）
```

smoke / QEMU / kernel：零接触零注册（host-only 刀）。

## §8 测试设计（10 件，验收三件加粗）

1. **`tests/damage.rs`（验收①）**：Σ×1.25 双侧边界（恰 1.25 允并 / +1px 弃并——`×4 ≤ ×5` 整数式钉板）；相交可并性分形钉板（并集为矩形的相交形必并 + L 形角点对值级拒并——§3.4 勘误后口径）；K≤8（9 矩形无对可并 → FULL；可并对 → 收敛 8）；FULL 后 add no-op；clip viewport（负坐标/越界）；零面积丢弃；u64 巨参不溢出；RectSet subtract/covered 几何网格；try_merge 值级断言。
2. **`tests/frame.rs`（验收②）**：`next_phase` 真值表全格（4 信号 × 3 态采样）；IDLE `next_wakeup()==None` + 零 tick；`wake` → Run + 立即帧（expected 不欠拍）+ `idle_exits/wakes` 递增；2000ms 静默强降；SETTLE 在途呈现确认 → IDLE；`note_presented` actual 轨入环 + jank 判定（超半周期 `deadline_miss++`）；credits 簿记 ≤2；vote_hint 随 anim/scene 变化。
3. **`tests/offscreen.rs`（验收③）**：golden 场景（Root+Pad+Col{Rect,Text,Row{…}}）逐字节像素对拍（gfx golden 两级法 + 0xEE 哨兵 + `need_bytes` 缓冲）；Present 尾命令恒在；增量 damage：改单节点色 → damage == 该节点界且像素 diff 恰该区；**遮挡剔除**：被盖 Rect `culled≥1`、overdraw_x256 ≤ 640；守卫真伪自检——同场景直绘（不走剔除）overdraw 必超 640（断言必不相等）；damage 外像素零扰动（哨兵执法）。
4. `tests/tree.rs`：容量拒面（`Ui<4>` 满 → NodeFull）；remove 连子树 + damage 旧界；代数防复用（remove 后旧 NodeId → NodeBad）；double-remove 拒；Root 幂等。
5. `tests/layout.rs`：Col/Row/Pad 摆位钉板；Text 内在尺寸 = len×cell（档位×2）；Option 随父；**Measure 缓存**：改色 → `measured==0`；改文 → 仅该子树；set_viewport 全量重排。
6. `tests/reconcile.rs`：encode→apply→encode 字节相等（roundtrip）；builder 树与 AST 树 encode 相等（双入口等价）；apply 后 FULL damage；拒面：未知 kind/flags 未知位/截断/尾随/叶子带子/文本超长。
7. `tests/input.rs`：hit topmost；Click 全链（Down+Up 同点 → Press+Release+Click）；slop 内 UP=Click、slop 外=Drag 三段；CANCEL 终止不补 UP；双 pointer_id 独立；buffer 满 loud；pressure 哨兵折 500。
8. `tests/anim.rs`：Transform 型 `measured==0`（不重排）+ damage=新旧包络；Layout 型触发重排；终点值钉板（to 精确落位）；StepEnd 语义；表满 loud；anim_active 随表空翻 false。
9. `tests/resample.rs`：<2 采样/非单调 None；<2ms / >20ms None；括点插值精确；外推帽 min(Δ/2,8ms) 帽沿截停 + ts 诚实；撞 ts 取新。
10. `tests/dep.rs`：bind/dependents 记录查询；满帽 loud；**不影响失效**（v1 语义钉板——dep_bind 后 invalidate 仍视图级）。

## §9 挂账（明示不偷工）

- 布局算法插拔缝（LayoutEngine trait）——GUI-6 定形（v1 内建 Col/Row/Pad/Fixed；网格归 GUI-6）。
- 属性级失效消费 + S7 spike（kill = 收益 <20% 帧耗时 → 维持视图级至 v2）——dep.rs 缝已 day-one。
- 文本 DSL / 宏糖（GUI-8/SDK 线）——AST 二进制双入口已 day-one，DSL 编译到同一 AST 非第二事实源。
- guest 真消费者接驳（gui-wm=GUI-5：channel 会话 × guiproto 编解码 × fb VMO 映射 × Present{damage} 驱动；gfxdemo/smoke 注册归 GUI-9）。
- overdraw ≤2.5× 与 IDLE 唤醒 0 / icount≈0 进 smoke——GUI-9 能耗相（本刀 host 断言 + 遥测面 day-one）。
- vote/power_mode 真消费（QEMU 单帧率三扇出 no-op——接口先活、消费后到）。
- UI/合成错相真后端验证（S4 spike）；credits 折叠实证（S6）。
- clip/滚动视图、hover 三相、多点手势细化、长按——PHASE_HOVER_* 留位 pump 不产（gui-inputd 同口径）。
- per-surface 帧率意图（rate_hint 已随 FrameOutcome day-one；多 surface 归 GUI-5+）。

## §10 实施对账（impl workflow 实录——契约偏差裁决记录）

三路并行落码（A = lib/desc；C = frame/anim/hit/resample/telemetry/dep；B = damage/layout/paint 输出超限阵亡——集成段按 §6 冻结签名全量补写）+ 集成修编译 + 双路测试 84 件全绿 + 修复段闭环 2 findings。以下偏差经裁决**接受为实现事实**，§6 原文与实现的出入以此为准：

**路径/可见性类**
1. `use berry_gfx::cmd::{CommandEncoder, GlyphPos}`——gfx 无根 re-export，路径修正（同类型零语义差）。
2. `VoteBallot.sticky` pub——§6 字面全字段 pub，实现笔误由集成段纠正（实现 = 契约）。
3. `root()` 容量失败返 `NodeId(u32::MAX)` 哨兵（冻结签名非 Result；N≥1 结构性不可达）。

**语义裁定类**
4. `set_text/set_props` kind 失配 → `Err(NodeBad)`（UiError 族无 kind 变体，NodeBad 兼任 loud）。
5. CANCEL 分型：拖中 = DragEnd+Cancel 双发；非拖 = 仅 Cancel；空白按压 = 无 Msg。Click/Release/DragEnd 锚定 DOWN 时刻命中节点（UP 不重命中）。
6. hit/node_at 排除 Root（viewport 持有者不参与命中——背景按压 = 无命中；两入口单一结论）。
7. resample 显式严格递增 ts 预检（对单调输入与 gui-inputd 逐位一致；对区间重复 ts 更严 = 文档「非单调→None」语义的明示执法）。
8. Text TLV 上限实算 **73B** 非 §3.2 字面 69B（原文漏计 bg Option 载荷 + tag 字节；TLV 格式本身逐字节按 §6）。
9. Rect `w/h=None` = 主轴折 meas 0 / 十字轴吃满分配；Text 的 GlyphRun bg 透传 `style.bg`（§6 TextStyle 字段语义，非 §3.5 字面 bg=None）。
10. culled 节点同计 damage（外露差 + 被盖两面——保守方向正确）；relayout `freed` 参数签名保留不消费（走查自根天然跳过死槽；removal 旧界 damage 在 `Ui::remove` 先行入账——文档化取舍）。
11. RectSet 方法集以 §6 冻结集为准（add/subtract/is_covered/visible/area/len——§3.4 散文的 union_area/intersects 未造；area() 即直和，集内不相交无双计）。
12. 动画死节点 / prop-kind 失配：不应用但照常完成移除（防 anim_active 卡死永不停帧）。
13. Gesture 内部 `PTR_MAX=4`（§6 外自定）；`lost_msgs/lost_events` 为写不读诊断计数。
14. desc `parse_node` 只解析节点体（kind/flags/key/props），child_count 归树走查域；`child_count>255 → AstError::TooSmall{need:256,got:n}` 映射。
14a. **加性面收据**（R1 勘误——§6 契约外的实现自有 API，在此入册防「契约外漂移无账」）：`desc::DescBuilder`（TLV 流式构造器——测试与宿主侧打包面用，含 CHILD_MAX=255 拒面）；`desc::{DESC_MAX=4096, DESC_DEPTH_MAX=16}`（Builder 路显式执法帽，apply 路不对称见条 22）；`RectSet::is_empty`（wake 条件化 FULL 的判空消费面）；`lib::DEFAULT_VSYNC_PERIOD_NS`（≈60Hz 缺省）；`resample::target_from_expected`（目标时刻单源推导——pump_input 唯一消费，未双轨）。

**测试期发现（已闭环）**
15. try_merge「相交恒可并」伪命题——§3.4/§8 已勘误（分形陈述 + L 形反例值级）；damage.rs 头注同步分形化；测试名改 `try_merge_intersecting_cases_value_pinned`。
16. render_frame 跳帧 `rate_hint_hz` 原无条件 `rate_hz()` → 改走 `FrameOutcome::skipped()`（跳帧诚实填 0，跳帧不提交）；回归钉板双向（绘制帧 60 / 跳过帧 0）。

**三 seam 收敛 + 审查修复（审查刀闭环——R1 规划/R2 正确性/R3 突变探针 + Fix 四段 workflow）**

审查轮实录：R2 13 findings（1 high = set_props 缺 kind 校验漏画级 + 6 medium 集中三 seam 与 wake 全屏）+ R1 5（§10 记账反事实 + 加性面缺收据）+ R3 探针 9 发 weak 3/gaps 6；Fix 段处置 17 项（修 14/驳 3——驳回项均属本文档回执面，按「docs 归属方改」原则移交本节）。修复后 **96 测全绿**（84+12 新增）+ no_std 过。

17. ~~frame.rs 纯函数面与 lib.rs 内联双轨~~ **已收敛单源**：lib.rs 内联帧循环全迁 frame.rs 纯函数并被 Ui 消费——`next_vsync_tick`（mod 归一容忍敌意 phase ≥ period + saturating_add）、`next_wakeup`（Run = min(vsync 拍, 动画期限 max(now), 强降闹钟)；Settle = 强降闹钟单源不追拍——省电修正，embedder 义务 = FRAME_CALLBACK 到达即醒）、`Credits` 背压簿记（note_submitted 委托；门返回值丢弃 = 「已提交」事后记账，真值源 = 显示服务应答回填）、`ballot_for` 四档定值票（idle `{0,0,0,sticky:1}` 修正——原 idle 档 sticky:0 与活面 vote_hint 矛盾，迁移前先修再迁防停帧粘滞信号静默翻转；**interacting 第三信号接通 dragging 态 → SCENE_DRAG 票 `{60,120,120}` 首次可达**）。dead_code 全摘、内联副本删除——「测试面 ≠ 消费面」结构性消亡（原记账「frame 测试钉 dead code」为 R1 勘误：钉的是 next_phase+Ui 内联面，四纯函数当时零覆盖；收敛后 Ui 面即单源面）。
18. ~~dep.rs DepTable 未消费~~ **已收敛**：`Ui.deps: dep::DepTable` 字段级消费，同边幂等 bind（满帽重绑同边 Ok 不占槽——修公开 API 可自造 DepFull 容量耗尽）；`remove()` 在抬 gen 前对子树逐 id `unbind_node`（序 = 先解绑后抬 gen）；`reset_arena` 走 `clear()`；lib.rs 内联边数组删除。
19. ~~`DragMove{resampled}` 恒 false~~ **已翻位**：pump_input 记 `did_resample` + dispatch 前后 `out.len()` 判别——新增恰一条且为 DragMove 时翻位（MOVE 臂至多产 1 条 Msg，末条判别无歧义）；raw 路恒 false；公开 API 零改动。
- 修复轮其余要点（在案事实，非欠账）：set_props kind 守卫先于状态变更拒 `NodeBad`；wake **条件化 FULL**（空集才置——无条件 FULL 吞增量脏区 = 废脏区引擎，省电支柱级修复）；anim Offset damage 改 subtree 包络（防溢出子树残影）；paint 超 ORDER_CAP 回退前序直发不丢画面（原静默丢 = 漏画方向，与 RectSet 保守丢弃反）；DescBuilder `end()` 拒面先于任何状态变更（重试稳定）；node_at 委派 hit.rs 单源（lib.rs 薄转发——双实现消亡）；target_from_expected 双轨删除（改泵参直减饱和算术单源）；telemetry 环回卷 + 迟到应答 no-op 专测。
20. no_std 构建两条 target-feature warning（-fp-armv8/-neon future hard error）来自仓库根 .cargo/config.toml 经依赖链（fbcon→gfx→guiproto→ui）传播——非 berry-ui 源码产生，根治归工作区级配置另刀。

**留白（有界，挂 GUI-4 段② gui-wm 消费面刀顺手补）**
21. damage `add()` 流程③多候选择优（≥2 可并对时小包络优先）无值级钉板——确定性引擎下无正确性差（任选可并对均收敛 ≤8），仅集形状未钉。
22. desc `apply_desc` 路深度帽靠 MAX_NODES=256 间接约束（~257 层递归才 TooSmall），与 DescBuilder 的 DESC_DEPTH_MAX=17 显式执法不对称——递归深度 ≤256 帧有界、栈风险可容；敌意流真实入口在 gui-wm 侧，届时对 apply 面补单源执法。
