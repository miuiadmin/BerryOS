# GUI 架构协议完善与 Rust 实施计划

> 版本：**1.0**｜2026-11-12
>
> 类型：实施计划 / 路线执行计划。
>
> 定位：收口 SDK-S0、GUI-T1、SEM-A1、DESIGN-D2、LAYOUT-L4 之后的 GUI 架构协议遗漏，给出后续 Rust 分刀实施序、协议/实现边界、验收门、预算策略与风险停止条件。执行序权威 = [GUI框架地基路线-v1.20.md](GUI框架地基路线-v1.20.md)；GUI 域规则 = [规则/14-GUI与UI框架.md](规则/14-GUI与UI框架.md)；架构裁决 = [架构决策.md](架构决策.md) ADR #84。
>
> 本文回答一个关键问题：**“完善界面架构协议是不是就是各刀后置清单里的东西？”**
>
> 结论：**是核心来源，但不是全部，也不能混成一刀。** 后置清单是被拆散的待办事实；本计划把它们重新归并为可执行的 Rust 分刀，并补上研究审计发现的未排程缺口：runtime keyed re-apply、生命周期性质测试、窗口 capability record、presented-frame 反馈、输入活性 watchdog、显示元数据词表、UIContext seam、rootfs 增长策略与 SDK 文档三分层。

---

## §1 当前基线

截至：

```text
77174fefae855055144aa07332360bcebb83895f
feat(gui): 落地 LAYOUT-L4 约束代数
```

已落地主干：

```text
F0..F9 Berry Room / BerryUI / 五 app 迁移 / 守卫
C1 嵌套裁剪
P0 Surface Mode 协议
P1 Surface 生产接线
P2 Keyboard Input
P3 Click-to-Focus
P4 Window Policy Matrix
R1 Window/Surface Authority Hardening
R2 Occlusion Contract
R3 Config Event Architecture
C1 Component Focus Navigation
L1 Local Constraints & Adaptive Slot
L2 Flex Ratio Allocation
L3 Transposed Container Flex & ListDetail
SDK-S0 Control Spec & API Governance
GUI-T1 TextField Foundation Before IME
SEM-A1 Semantic Tree Sidecar
DESIGN-D2 Design Token Pair Contract
LAYOUT-L4 Box Constraints & Intrinsic Boundary
```

当前权威口径：

```text
SDK facade = 102 entries
Design pairs = 36
ADR = 83（本文后续增补 ADR #84）
rootfs0 = 33,554,432B，恰 32MiB 单卷帽
rootfs1 = 368,440B
rootfs 总量余量 = 100,294,856B
```

---

## §2 遗漏审计结论

三路 read-only workflow 研究复核后，发现以下不是已落地规格中的“实现细节”，而是**路线级遗漏**。

### 2.1 后续门缺口

路线 v1.19 与规则 14 的执行序均终止于 LAYOUT-L4。GUI-15 原优先队列：

```text
GUI-T1 + SDK-S0
SEM-A1
DESIGN-D2
layout v2 / LAYOUT-L4
```

已经耗尽。所有“归后续刀”的指针现在没有 owning milestone，按路线 §6 将导致下一刀程序上不可开工。

本计划通过路线 v1.20 补上后续 ladder。

### 2.2 研究承诺但未排程项

以下项在 GUI-14 / GUI-15 中有明确要求或通过标准，但没有落进路线：

```text
E7 keyed re-apply 与 settle cap
E8 apply/reset/remove/focus 生命周期状态机性质测试
E9 window capability record
E10 presented-time / frame credit 反馈
input-target liveness watchdog
UIContext seam
Tour / Guide / Reference 三层 SDK 文档
可执行示例与 reference/gallery 单源
```

### 2.3 已知欠账

#### gui-wmd host harness

P2 与 R1 规格仍承认缺少独立 host harness，现有验证主要靠 QEMU/GUI8 与 gfxdemo。后续 WM 协议刀需要先补 host 化矩阵，否则每轮协议回归都要付 TCG 时间。

#### density / threshold 单源

当前仍存在两个判定源：

```text
theme.rs:
w >= 1000 && h >= 1800

desktop.rs:
vp_w >= 600 && vp_h >= 1800
```

DESIGN-D2 已落 quarter-vp effective-pixel，但没有统一 density / adaptive threshold。该缺口必须进入后续布局/设计刀，不能继续复制阈值。

#### display metadata vocabulary

V1 只有方向性裁决，没有词表和 codec。即使第二显示与 x86 @1x 未到，也应先冻结元数据表示与 unknown/fail-closed 语义。

#### multi-window 设计刀

R1 遗留的 z/occlusion、Room VA、waitset/session/window 上限、present stride、popup 角色与 4096px compositor bounce 均指向 W1，但没有规格。

#### per-spec 后置项没有总账

L1/L2/L3、C1、GUI-T1、SEM-A1、DESIGN-D2、LAYOUT-L4 各自都有后置节；此前没有统一依赖图和优先级。本文 §4/§5 即总账。

---

## §3 全局 Rust 实施不变量

后续每一刀必须继续满足。违反任意一条即停止，不得用“计划已批准”豁免。

### 3.1 分层不变量

```text
berry-gfx：
    纯光栅/字形/图像，不认识 app、窗口、主题、业务

berry-ui：
    retained tree、layout、state、focus、theme、semantics
    no_std、零分配、零 panic、整数优先
    不收发 WM RPC，不映射 VMO，不直接 syscall

berry-room：
    WM/frame/input/font/insets 生命周期 host
    不定义业务页面，不实现控件视觉

gui-wmd / guifbd / gui-inputd：
    所有权、仲裁、路由、呈现
    不为某个 app 特判

普通 app：
    状态、数据、页面描述、消息
    不手写 WM/VMO/stride/像素
```

### 3.2 协议不变量

```text
AST / ViewKind / Props / Msg：
    默认零漂
    只允许 add-only + 版本域/ flags 域
    旧流 round-trip 与 reject-face 必测

berry-guiproto：
    opcode/结构 add-only
    per-interface version bump
    无 producer 的能力返回 ST_UNSUPPORTED

R3 config：
    serial 语义保留
    last-one-wins 保留
    未 ACK 禁 submit 保留
    新 cause 只能 add-only

SDK：
    公开 facade 必入 sdk_api.toml
    since / stability / owner / spec / breaking history 全量

Design：
    新视觉 token 必入 paired ledger
    active text >= 4.5:1
    Disabled 显式 inactive 豁免
```

### 3.3 Rust 运行时不变量

```text
no_std
零 heap allocation
零 panic path
零浮点布局/主题计算
固定容量数组与显式 loud error
Node 冻结面不得随意扩张：
    新运行期状态优先 side table
    生命周期与 slot generation 绑定
```

### 3.4 标准验收电池

每刀至少：

```text
cargo test -p berry-ui
just test-host
just check
berry-ui no_std
berry-ui guest-font-vmo no_std
uitest no_std
GUI8 self-test
GUI8 normal-boot
GUI8 dual-boot canonical persistence
smoke-image 138/138 或更新后的单调递增计数
cargo deny licenses
docs_guard
Curia strict
writable_globals
git diff --check
```

协议刀另加：

```text
codec round-trip
旧流零漂
unknown/reject face
authority/capability 拒面
serial / credit / liveness 性质测试
```

视觉刀另加：

```text
像素锚
文本色数锚
design ledger
theme scheme golden
```

---

## §4 后置项归类：协议刀 vs Rust 实现刀

结论：**后置清单不是同一类工作。** 必须先分类，再排刀。

### 4.1 LAYOUT-L4 后置

| 后置项 | 归类 | 依赖 |
|---|---|---|
| per-node incoming box | layout kernel Rust 实现 | L4 BoxConstraints |
| box-sensitive measure | layout kernel Rust 实现 | per-node box；`TextMetrics` 需宽度输入 |
| Text wrap under max width | layout kernel + 文本状态 | box-sensitive measure |
| TextField shrink-to-width | 控件/layout 实现 | Text wrap + GUI-T1 |
| flex grow/shrink/basis | runtime side table 实现 | box-sensitive measure |
| nested flex negotiation | layout kernel 实现 | per-node box + flex algebra |
| Grid fr | layout track 分配实现 | flex algebra |
| MainAlign / SpaceBetween / alignSelf | Props/AST add-only 协议 | layout kernel 稳定后 |
| intrinsic width/height query | SDK facade / contract-lite | per-node intrinsic |
| persistent constraint cache | 性能实现 | kernel 语义稳定 |

关键停止条件：

```text
不得引入第二遍全树 measure
不得因 cache stale 造成错误几何
不得破坏 L1..L4 既有 golden
不得把 flex weight 写进 AST/wire
```

### 4.2 GUI-T1 / 文本与 IME 后置

| 后置项 | 归类 | 依赖 |
|---|---|---|
| click caret / drag selection | berry-ui 输入实现 | GUI-T1 + text hit |
| 长按菜单 | 组件实现 | F4 菜单组件 |
| TextArea / multiline | 控件 + add-only contract | Text wrap |
| IME composition commit | 协议 + 状态机 | IME Host |
| soft keyboard / IME Host | 系统协议刀 | R1/R2/R3 + GUI-T1 |
| KeyboardAvoider | berry-ui insets 实现 | IME insets producer |
| FormMetrics | SDK contract | IME + layout |
| clipboard | 新跨进程服务协议 | capability 设计 |
| autofill | 后置协议 | FormMetrics + 安全模型 |

### 4.3 SEM-A1 后置

| 后置项 | 归类 | 依赖 |
|---|---|---|
| batch semantic frame | 协议 | SEM-A1 |
| poll/ack subscription | 协议 | R3 模式 |
| stable id + incremental diff | 协议 | semantic frame |
| value/text range wire v2 | add-only 协议 | SEM-A1 |
| action dispatch | 协议 + app Msg 回流 | stable id |
| heading / hidden / accessibilityLevel | a11y 词表协议 | SEM-A2 |
| minimum touch target | a11y/layout contract | LAYOUT-L5 |
| screen reader service | 新 SYSTEM 服务协议 | SEM-A2 + R1 seed |
| scroll-into-view | action/layout 实现 | action dispatch + layout |

### 4.4 DESIGN-D2 后置

| 后置项 | 归类 | 依赖 |
|---|---|---|
| 五 app dark/high-contrast migration | app/golden 迁移 | D2 已可做 |
| PRESSED_BG / swipe dots legacy 清理 | token 迁移 | D2 |
| elevation token | design contract-lite | D2 |
| motion token | design contract-lite | 动画基础 |
| system theme setting | R3 add-only cause 协议 | D2 |
| OEM brand theme | 组装/定制协议 | ADR #82 |
| HCT 动态取色 | 独立设计研究 | 不复制 Material HCT |
| 多档字形 / 连续 text scale | 字形资产 + 协议混合 | rootfs 策略 |
| continuous scale_permille | P0/R3 add-only 粒度 | display/density 词表 |

### 4.5 C1 / L1 / L2 / L3 其他后置

```text
2D geometric focus、FocusGroup、FocusRestorer：
    berry-ui runtime focus side table 实现

Route / Back / stateful ListDetail：
    app-side navigation API contract；选择/路由仍不进布局引擎

Room 自动 config pump、occluded-basis 翻转：
    R3 消费面实现

WindowSizeClass ladder、SafeArea 4-sided：
    SDK/P0 contract；先核 codec 余量

动画、模糊、阴影：
    berry-gfx / berry-ui effects 实现；受预算与像素锚约束
```

---

## §5 Rust 分刀实施序

路线 v1.20 将以下列作为后续执行门。默认严格串行；只有 Phase A 与 Phase B 的文件面/验证面确实隔离时，才允许并行兄弟会话，且提交仍由主会话串行收口。

### Phase 0 — PLAN-G0 计划与预算前置

状态：本文落档即完成。

交付：

```text
后置项总账
协议/实现分类
rootfs 增长策略
后续 milestone ladder
```

预算裁决：

```text
rootfs0 已到 32MiB 单卷帽：
    禁止任何“再塞一点进 rootfs0”的方案
    新字形/大资产优先独立 volume 或 rootfs1
    每 .bap 与 rootfs 卷必须报告
    不得降低壁纸/图标质量换空间
```

退出门：

```text
路线 v1.20 + 规则 14 + 本计划 + ADR #84
docs_guard / Curia strict 全绿
```

---

### Phase A — RUN-K1 Keyed Re-Apply & Lifecycle Properties（已落地）

**为什么第一刀不是继续堆协议：**

后续 layout、IME、SEM、theme 都会频繁改树、移动焦点、触发重应用。若 apply/reset/remove/focus 的生命周期性质没有机器夹具，后续每刀都会重复引入同类回归。

交付：

```text
stable key diff / re-apply
key conflict loud reject
settle cap
apply/reset/remove/focus 状态机性质测试
UIContext seam 盘点与最小固化
gui-wmd host harness 第一版
```

Rust 面：

```text
user/berry-ui/src/desc.rs
user/berry-ui/src/lib.rs
user/berry-ui/src/focus.rs 或 focus side table 所在模块
user/berry-ui/tests/{keyed,lifecycle}.rs
tools/ 或 host harness 新测试
```

关键性质：

```text
禁 index-only key
禁 app 可伪造 JSON key
同 parent key 冲突 loud reject
removed subtree：
    focus 清空
    anim 清理
    sem 清理
    flex/a11y side table 清理
无合成 FocusOut / Click
无双重激活
非法状态转换失败而非静默吞掉
交互帧重推导 p99 <= 3
keyed 结果与 full-rebuild oracle 几何/AST 语义一致
```

退出门：

```text
AST/wire 零漂
GUI8 bytes=860 / a11y=9 不变
full-rebuild fallback 保留
随机操作序列性质测试通过
```

---

### Phase B — LAYOUT-L5 Per-node Box & Text Wrap（已落地）

交付：

```text
per-node incoming BoxConstraints
constraints down / sizes up / parent positions
box-sensitive Text measure
Text wrap under max width
TextField shrink-to-width 最小面
density / adaptive threshold 单源
```

明确非目标：

```text
flex grow/shrink/basis
Grid fr
MainAlign/alignSelf wire
多行富文本
连续字号乘法
```

设计要求：

```text
仍是一后序 measure + 一前序 place
每个 slot 每次 relayout 至多 measure 一次
CountingMetrics 计数不得高于基线 +10%
constraint equality 短路
damage 恒真实 viewport
hit 根域仍 Root 摆放界
```

`TextMetrics` 演进方向：

```rust
// 现状
measure(text: &[u8], scale: u8) -> (u32, u32)

// 目标缝，具体命名以规格为准
measure_with_box(text: &[u8], scale: u8, box: BoxConstraints)
    -> TextMeasure
```

必须保持 no allocation。wrap 结果用固定容量行数组或调用方 scratch，不得引入 heap。

退出门：

```text
L1..L4 全部既有测试不改语义
UITEST L2/L3 348/696 不变
新增 wrap/shrink golden
AST 默认零漂
rootfs 预算记录
```

---

### Phase C — LAYOUT-L6 Flex/Grid/Intrinsic Completion

交付：

```text
flex basis/grow/shrink
嵌套 flex measure negotiation
Grid fr/auto track
MainAlign / SpaceBetween / alignSelf
intrinsic width/height query
persistent constraint cache
```

协议边界：

```text
默认全部 runtime side table / pure function
flex weight 不进 AST/wire
MainAlign/alignSelf 若需要序列化，另开 add-only Props 域
旧流零漂
```

退出门：

```text
L2 1:2、gap、固定子、余数语义不变
L3 transposed pane 语义不变
Grid 新 golden
cache stale 注入测试失败
单遍测量守卫继续成立
```

---

### Phase D — WM-R4 Capability Record & Frame Feedback

这是“界面架构协议”核心刀之一。

#### D1 Window Capability Record

把 P4 的命令式策略矩阵提升为纯数据：

```rust
WindowCapabilityRecord {
    focusable: bool,
    touch_modal: bool,
    ime_visible: bool,
    layer_intent: LayerIntent,
    // add-only flags only
}
```

要求：

```text
flag × lifecycle 决策由纯函数完成
无隐藏状态
host harness 可重放整个 P4 矩阵
旧策略输出不变
```

#### D2 Presented Frame Feedback

补 requested / latched / presented / dropped：

```text
requested   app 已提交
latched     compositor 已拾取
presented   显示链路呈现
dropped     超时/替换/错误丢弃
```

目标：

```text
frame credit 泄漏可检测
迟到帧不破坏 damage 正确性
FrameTimeline 四桶归因有数据面
steady-state zero-wake 语义保留
```

#### D3 Input Liveness Watchdog

```text
target 必须在界内消费输入
超时 cancel / synthesize
不能静默丢键
不能造成焦点第二权威
```

退出门：

```text
berry-guiproto add-only
旧 opcode 零漂
GUI8 normal/full 真链
host harness 重放 P4 + liveness 矩阵
```

---

### Phase E — DESIGN-D3 Theme Event & Production Schemes

交付：

```text
R3 add-only THEME cause
系统 Settings 主题开关
wmd per-window theme event
Room poll/ack
BerryUI Theme::for_scheme 切换
五 app dark/high-contrast golden 迁移
PRESSED_BG / swipe dots legacy 白名单清理
elevation 最小 token 面
```

非目标：

```text
HCT
OEM 品牌主题
连续字号
动画完整 token 族
```

退出门：

```text
36 design pair ledger 全绿
active text >= 4.5:1
theme switch 只走 set_props/damage
几何不变除非字号档变化
每 scheme 像素锚
```

---

### Phase F — SEM-A2 Semantic Transport & Action Dispatch

交付：

```text
batch semantic frame
poll/ack subscription
stable node id
incremental diff
value/text range wire v2
action dispatch
heading / hidden / accessibilityLevel 词表
minimum touch target contract
```

边界：

```text
SEM 不成为第二 focus/hit 权威
action 回流 app Msg 队列
未知词 loud reject
跨进程服务先不产品化
```

退出门：

```text
SEM-A1 overlay 零回退
action dispatch 真链
非导出语义不可读
wire add-only + 旧流零漂
```

---

### Phase G — IME-1 IME Host & Soft Keyboard

前置已满足：

```text
GUI-T1 文本状态机已落地
P0 IME composition 词表已冻结
R1 SYSTEM seed 已落地
R2 insets::Ime 消费面已冻结
R3 config event 已落地
```

交付：

```text
IME Host 系统层
soft keyboard SYSTEM surface
insets::Ime 真实 producer
composition commit
KeyboardAvoider 最小面
FormMetrics 第一版
```

退出门：

```text
text focus 先于 component activation
IME surface 不抢策略焦点
occlusion invariant 保持
R2 insets 数学零回退
AST/wire add-only
真实键盘链 UITEST/GUI8
```

---

### Phase H — NAV-1 Route/Back/Form State

交付：

```text
Route/Back stack API
stateful ListDetail controller
scroll-into-view
FormMetrics 消费面
组件 lifecycle/freeze hook
```

边界：

```text
路由是 app-side library
选择/路由不进 berry-ui 布局引擎
Back key 走 wmd -> Room -> app Msg
```

退出门：

```text
真实 Back 键链
长列表 churn/paint 计数下降或持平
既有页面 golden 零回退
```

---

### Phase I — WM-R5 Multi-window & Display Geometry

先设计，后实现。

设计面：

```text
session/window/waitset 容量
Room VA 分配
buffer pool
popup / main / auxiliary role
present stride 字段
u16 display domain
4096px compositor bounce
display rotate
display metadata vocabulary
```

第一小刀只冻结：

```text
display id
density source
refresh
rotation
layout scale
sampling scale
unknown / unsupported 状态
```

多窗口 API 只有在内存/合成预算模拟过门后才开工。

---

### Phase J — GFX-F1 Font / GPU / Frame Intent

并行资产线，但必须受 Phase 0 预算策略约束。

#### J1 True CJK Font

```text
思源黑体 OFL 1.1
common-CJK subset
host bake
A8 atlas
分片/压缩
独立 rootfs volume
多档字形供给
```

停止条件：

```text
若资产导致 rootfs 多卷超帽，先做拆卷/迁出
不得降质壁纸/图标
不得每个 app 静态复制大字表
```

#### J2 virtio-gpu 2D

```text
pass-shaped API 后端化
HDI 0x40..0x43 激活
soft backend 永久 CI baseline
damage K<=8
```

#### J3 Frame-rate intent

```text
per-surface intent bit 激活
帧率策略不破坏 steady-state zero-wake
```

---

### Phase K — DOC-D3 Executable Docs & SDK Closure

交付：

```text
Tour / Guide / Reference 三层文档
所有示例可编译/可运行
reference/gallery 从 sdk_api.toml 单源生成
文档构建进 just check
零手写 API 表
channel/stability 治理收口
```

---

## §6 推荐近期执行窗口

### 第一刀：RUN-K1（已落地）

理由：

```text
host-only，风险最低
直接补 GUI-15 E7/E8
所有后续 IME/SEM/layout/theme 刀都消费 re-apply 与生命周期性质
可先不碰 wire，避免在计划刚扩展时引入协议漂移
```

### 第二刀：LAYOUT-L5（已落地）

理由：

```text
L4 已铺 BoxConstraints
Text wrap 是 FormMetrics、IME、scroll-into-view、TextArea 的共同前置
仍可在 berry-ui 内部收敛
```

### 第三刀：WM-R4

理由：

```text
在 runtime/layout 稳定后补 Session 协议纯数据面
为 IME visible、multi-window、frame feedback 提供统一 record
```

---

## §7 协议设计模板

每一把协议刀必须先写规格，且规格至少包含：

```text
1. 问题与现有行为
2. 权威分层：wmd / Room / berry-ui / app
3. 数据模型与 Rust 类型
4. wire/opcode/version 域
5. add-only 与旧流零漂证明
6. reject face / unknown face
7. capability 与权限
8. serial / credit / liveness 语义
9. host codec 测试
10. GUI8 真链证据
11. 预算与镜像 SHA
12. 非目标
```

禁止只在代码里“顺手加一个 opcode”。

---

## §8 测试与反 mock 增量

后续新增统一要求：

### keyed re-apply

```text
随机 op sequence
key conflict
subtree remove
full-rebuild oracle
settle cap
```

### layout v2

```text
CountingMetrics
constraint equality
golden only changes at parent boundary
L1..L4 regression
intrinsic query
cache invalidation
```

### WM protocol

```text
record pure decision matrix
present/drop feedback
credit leak
input watchdog
old opcode reject
```

### SEM

```text
batch frame snapshot
diff oracle
action dispatch
non-exported deny
unknown vocabulary reject
```

### IME

```text
composition phase
commit/cancel
insets show/hide
occlusion
keyboard focus routing
real soft keyboard surface
```

### theme

```text
R3 serial
theme cause
scheme switch damage
contrast ledger
pixel anchors
```

---

## §9 风险与停止条件

### 技术风险

| 风险 | 停止条件 / 处置 |
|---|---|
| layout v2 滑向多遍 measure | measured 计数超基线 +10% 即停止 redesign |
| key diff 身份不稳定 | key 冲突或 oracle 不等即 fallback full rebuild |
| side table 容量扩张 | 必须测试 NodeFull/EntryFull loud，不允许静默丢 |
| rootfs0 超帽 | 自动停止，先资产拆卷/迁出 |
| GUI8 TCG 时间膨胀 | host harness 先行，QEMU 只留关键真链 |
| IME 变成第二输入权威 | focus 仍唯一归 wmd |
| SEM 变成第二 hit/focus 权威 | semantic action 只回流 app Msg |
| theme event 破坏 R3 serial | cause add-only，serial 语义测试不变 |
| multi-window 容量不可证 | 只留设计，不开 API |

### 过程风险

```text
不得因计划存在而跳过 route/spec/ledger
不得把多协议混成一个提交
不得用 app 样例替代协议测试
不得用“后置”堆成无主清单
```

---

## §10 明确继续拒绝 / 延后

以下不是近期实现面：

```text
platform channel / 外部 runtime embedding
automotive D9 产品化
HCT 动态取色
OEM 品牌主题
rich text
autofill
3D scene graph
Vulkan backend
第二显示生产行为
连续字号缩放
全量动画/模糊体系
```

原因：

```text
缺少真实设备/消费者
缺少预算或安全模型
或依赖前面 Phase 未完成
```

---

## §11 计划维护规则

1. 本文是执行计划，不是替代路线的第二权威。
2. 改顺序、改边界、改非目标，先升本文版本并同步路线与规则 14。
3. 每完成一刀：
   - 更新路线状态
   - 在本文对应 Phase 标注已落地
   - 写 implemented note
   - 追加阶段沿革
   - 回填镜像 SHA 与预算
4. 后置项不得从本文删除；若取消，移入“明确拒绝/延后”并写理由。
5. 计划本身不引入代码、opcode、AST 或 SDK facade。

---

## §12 第一版收口状态

本文第一版已完成：

```text
三路 read-only 研究/遗漏审计
后置项协议/实现分类
Rust 分刀 ladder
近期三刀建议
全局不变量
测试矩阵
预算策略
风险停止条件
```

后续实施从：

```text
RUN-K1 Keyed Re-Apply & Lifecycle Properties
```

开始。
