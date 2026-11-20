# BerryUI RUN-K1 Keyed Re-Apply & Lifecycle Properties

> 版本：**1.0**｜2026-11-12
>
> 定位：RUN-K1 现行实施规格。执行序权威 = [GUI框架地基路线-v1.20.md](GUI框架地基路线-v1.20.md)；输入计划 = [GUI架构协议完善与Rust实施计划-v1.0.md](GUI架构协议完善与Rust实施计划-v1.0.md) Phase A；研究依据 = [../research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md](../research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md) E7/E8。
>
> 核心裁决：**先把树重应用与生命周期性质变成机器夹具，再继续扩 GUI 协议**。本刀不改 AST/wire、不新增 opcode、不把 routing/state 写进引擎；只让已有 `key` 域获得真实调和语义，并把 apply/reset/remove/focus 的不变量钉死。

---

## §1 问题

当前 `Ui::apply_desc(src)` 是整树重建：

```text
reset_arena()
desc::apply_tree()
```

后果：

```text
NodeId / slot / generation 不跨 apply 存活
focus、anim、sem、flex、press、dep 侧表被整清
同屏小幅状态更新也付出 full rebuild + full damage
```

`ViewSpec.key` 与 AST `FLAG_HAS_KEY` 已存在，但注释仍是“v1 仅透传，reconcile 挂账”。GUI-15 E7 要求 keyed re-apply 与 settle cap；E8 要求 apply/reset/remove/focus 生命周期状态机性质测试。后续 IME、SEM action、theme event、navigation 与 layout v2 都会频繁改树，必须先补这个运行时地基。

---

## §2 权威分层

```text
app：
    生成新 AST / ViewSpec，声明稳定 key

berry-ui：
    key 匹配、props 更新、移动、增删、侧表生命周期、焦点消息

berry-room：
    不参与；本刀无 host 协议变化

gui-wmd / gui-inputd / guifbd：
    wire 零漂；仅补 gui-wmd 表策略 host harness

berry-guiproto：
    零 opcode、零 version bump
```

---

## §3 key 语义

### 合法 key

```text
key != 0
```

同一父节点的子 key 必须唯一。key 的身份域是：

```text
(parent identity, key)
```

不是全局唯一，也不是兄弟序号。

### 非法面

```text
key == 0：
    无键节点，永不参与 keyed 匹配

同父重复 key：
    DuplicateKey loud reject

同 key 但 ViewKind 改变：
    KeyKindMismatch loud reject

index-only identity：
    禁止

JSON / runtime 字符串 key：
    禁止；v1 key 域就是 AST u64
```

`DescBuilder` 不在同层扫描兄弟 key；key 冲突在 `apply_tree` / `apply_desc_keyed` 的树构建与计划阶段单源执法。`Ui::add` 在同父面提前拒绝重复 key，防 builder 侧绕入。

---

## §4 数据模型

新增统计：

```rust
pub struct ReapplyStat {
    pub matched_clean: u32,
    pub updated: u32,
    pub moved: u32,
    pub added: u32,
    pub removed: u32,
    pub rederives: u32,
    pub fallback_full: bool,
}
```

新增常量：

```rust
SETTLE_DERIVE_MAX = 3
KEYED_DEPTH_MAX = 24
```

新增入口：

```rust
Ui::apply_desc_keyed(src: &[u8])
    -> Result<ReapplyStat, AstError>
```

`apply_desc(src)` 保持不变，继续作为 full-rebuild oracle 与 guest 默认路径。

实验期 keyed 通道还有小栈切片帽：

```text
source scratch <= 128 节点
source depth <= 24
```

超帽返回 `TooSmall` 且当前树零修改；full apply 仍保留 MAX_NODES=512 /
TREE_DEPTH_MAX=96 能力。该帽防止把 D29 32KB guest 栈预算扩大到 embedder
接入时才发现溢出。

---

## §5 调和算法

### Pass 0：源树解析

先在临时 arena 中完整解析 `src`：

```text
非法 AST：直接返回错误，当前树零修改
合法 AST：得到 source tree + source a11y
```

### Pass 1：只读计划

从 Root 开始递归计划：

```text
同父 + 同 key + 同 kind -> match
同父 + 同 key + 不同 kind -> KeyKindMismatch
key==0 或无匹配 -> add
旧子不在新父匹配集 -> remove subtree
```

计划输出：

```text
source slot -> target slot / add
target keep / remove
clean / update / move 计数
capacity 检查
a11y upsert 容量检查
```

计划阶段不修改当前树，不产生 damage，不改变 side table。

### Pass 2：先移除

对计划为 remove 的当前子树逐个调用既有：

```rust
Ui::remove(NodeId)
```

因此沿用既有清理链：

```text
focus
anim
sem
a11y
flex
press
dep
slot generation
```

### Pass 3：按 source 前序调和

对每个 source 节点：

```text
matched：
    保持 slot + generation
    props 变化走 Ui::set_props
    props 相同则零操作
    兄弟序变化只 unlink/relink，不重建 NodeId

added：
    走 Ui::add(parent, ViewSpec)
```

移动 / 更新的 damage 走既有 set_props、remove、add 与显式新旧界收集，不引入第二 damage 代数。

### Pass 4：重建声明式 a11y

a11y 是 AST 声明面，不是运行时状态。调和末尾：

```text
source a11y 按 matched NodeId upsert
未出现在 source a11y 的既有 a11y 条目保留
removed subtree 的 a11y 已由 remove 清理
```

运行期 `sem/focus/flex/anim/dep/press/a11y` 不因 keyed 小改被整清；显式摘除仍走 `take_a11y/take_sem`。

---

## §6 full rebuild fallback

以下情况直接走既有 `apply_desc`：

```text
当前无 Root
计划删除数 + 新增数 >= 当前活节点数
```

fallback 统计：

```text
removed = 旧活节点数
added = 新活节点数
fallback_full = true
rederives = 1
matched_clean = updated = moved = 0
```

fallback 不代表失败；它是低效路径的正确 oracle。若源树非法，仍先返回错误并保持当前树不变，不做 fallback。

---

## §7 settle cap

本刀不存在迭代 derive loop；一次 `apply_desc_keyed` 至多一次树调和：

```text
rederives = 1
SETTLE_DERIVE_MAX = 3
```

机器验收：

```text
同一 AST 第二次 apply_desc_keyed：
    matched_clean == live node count
    updated == moved == added == removed == 0
    layout measured == 0（净树 no-op）
    damage 为空
```

这证明一次应用即 settle。若未来 re-entrant derive 进入引擎，必须在返回前证明：

```text
rederives <= SETTLE_DERIVE_MAX
```

否则 loud fail，不允许静默多遍。

---

## §8 生命周期性质

新增 host 性质测试以随机确定性操作序列驱动：

```text
add
set_props
set_text
remove
set_focusable
focus_set / focus_clear / focus_move
apply_desc_keyed
apply_desc full oracle
```

必须证明：

```text
removed subtree：
    focus 清空
    anim 清理
    sem 清理
    a11y 清理
    flex 清理
    dep 解绑
    无悬挂 NodeId

FocusIn / FocusOut：
    只由焦点状态机产生
    交替出现
    removed subtree 不产生合成 FocusOut

Click：
    不因 apply/remove/reapply 合成
    无双重激活

非法转换：
    loud error
    状态字节保持
    无半执行
```

keyed 与 full-rebuild oracle 对拍：

```text
encode_desc 字节相等
layout dump 几何相等
focus order 相等
sem snapshot 相等
```

---

## §9 UIContext seam

现状每个 app 都手抄：

```rust
WidgetContext::for_viewport(theme, width, height)
Ui::new(width, height)
```

本刀最小固化：

```rust
WidgetContext::for_ui(theme, &ui)
```

语义：

```text
读取 Ui.viewport()
调用 Theme.metrics_for_viewport
不读取屏幕、设备档、SafeArea 或系统主题
不新增第二上下文权威
```

六个 BerryUI app 的构造点改为 `for_ui`，消除 viewport 手抄。`for_viewport` 保留给 host 离屏测试与无 Ui 场景。

SafeArea、display scale、density threshold 与系统主题仍归后续 LAYOUT/WM/DESIGN 刀，不在本刀偷做。

---

## §10 gui-wmd host harness v1

gui-wmd 引入 lib/bin 双面：

```text
lib：
    WmTable 纯策略面 + WmIo 供给缝
    host cargo test 可运行

bin/guest：
    berry-libc / berry-sys I/O 与 serve 循环
    guest 行为零漂
```

`WmIo` 最小面：

```rust
now_ns()
send()
status()
close_handles()
diag_wrap_clear()
trap()
```

host harness v1 覆盖：

```text
WM_CREATE / owner / focus
P3 pointer focus
P4 HIDE/SHOW refocus
WM_TABLE_WAIT version 语义
config poll/ack serial
destroy_owned
forget_parked
AVOID_SYS 有效几何推导
compositor destination 右/下越界裁剪
```

不覆盖：

```text
present relay
compositor
真实 channel I/O
多进程死亡链
```

这些仍由 GUI8 / smoke-image 真链承担。

---

## §11 AST / wire 合同

```text
ViewKind 零新增
Props 零新增
Msg 零新增
Node 19 字段 / 144B 零漂
encode_desc / apply_desc 帧格式零漂
FLAG_HAS_KEY 语义不变
key u64 wire 表示不变
berry-guiproto opcode 零新增
R3 serial / ACK / last-one-wins 零漂
```

`AstError` 作为 Rust 内部错误族可 add-only 扩展：

```text
DuplicateKey
KeyKindMismatch
```

它们不序列化、不进 wire、不改变旧流。

---

## §12 SDK 面

新增 facade：

```text
ReapplyStat
SETTLE_DERIVE_MAX
KEYED_DEPTH_MAX
```

既有 `Ui` 覆盖新增方法 `apply_desc_keyed`，不新增独立顶层函数名。`WidgetContext` 已在 facade 中，新增方法不另立条目。

SDK ledger 以：

```text
since = RUN-K1
stability = experimental
spec = 本文
```

---

## §13 Host 证据

新增：

```text
user/berry-ui/tests/keyed.rs
user/berry-ui/tests/lifecycle.rs
user/bins/gui-wmd/tests/table_host.rs
```

`keyed.rs` 9 测钉板：

```text
key identity 跨 apply 存活
移动不换 NodeId
props clean 零操作
props update 走 set_props
key==0 不匹配
DuplicateKey reject
KeyKindMismatch reject
失败保持旧树
full-rebuild oracle 等价
fallback_full
capacity TooSmall
source depth cap
第二次 apply settle
```

`lifecycle.rs` 6 测钉板 E8 状态机性质、removed-subtree 侧表清理、焦点消息交替与 64 轮 LCG 随机序列 oracle。

`table_host.rs` 6 测钉板 gui-wmd 表策略 host 矩阵：owner/focus/destroy、R3 config serial 与 surface gate、AVOID_SYS 双几何、TABLE_WAIT/forget_parked、P4 HIDE/SHOW refocus、合成器 destination 右/下越界裁剪。

---

## §14 Guest / GUI8 证据

RUN-K1 是 host-only 运行时刀。guest 不新增标记、不改 UITEST 树、不新增提交帧。

验收复跑既有 GUI8：

```text
GUI8 self-test
GUI8 normal-boot
GUI8 dual-boot canonical persistence
smoke-image 138/138
```

零漂锚：

```text
UITESTAST bytes=860 a11y=9 ×2
UITESTL4 ×2
UITESTTOKEN ×2
GUI-T1 全链
SEM-A1 快照
L1/L2/L3 布局
render ×12
WMKEYROUTE 14/14
像素锚
```

六个 BerryUI app 只改 WidgetContext 构造方式，页面 AST 与视觉必须零漂。

---

## §15 预算

LAYOUT-L4 基线：

```text
UITEST.bap 101,656B
rootfs.cpio 33,922,872B
rootfs0 33,554,432B
rootfs1 368,440B
总量余量 100,294,856B
```

RUN-K1 最终：

```text
BROWSER.bap  = 79,820B   (+88B)
SETTINGS.bap = 78,368B   (+144B)
FILES.bap    = 77,928B   (0B)
PHOTOS.bap   = 97,376B   (+16B)
CONTACTS.bap = 81,332B   (+112B)
UITEST.bap   = 101,648B  (-8B)

rootfs.cpio 33,923,776B  (+904B)
rootfs0     33,554,432B  (0B，仍恰 32MiB 单卷帽)
rootfs1       369,344B  (+904B)
总量余量   100,293,952B
```

全部增长进入 rootfs1；rootfs0 零增长。

最终镜像：

```text
SHA-256 = 7881a3114272d789ee01d952d785e61d2017c4854f5fd25a18097544d45608d4
```

---

## §16 非目标

```text
BUI 语法
JSON/rich key
per-node layout constraints
Text wrap
SEM action dispatch
IME Host
theme event
multi-window API
连续 re-derive loop
props diff wire
跨进程 semantic id 服务
```

---

## §17 最终复跑

```text
cargo test -p berry-ui
gui-wmd host tests
just test-host
just check
berry-ui no_std
berry-ui guest-font-vmo no_std
uitest no_std
GUI8 self-test
GUI8 normal-boot
GUI8 dual-boot canonical persistence
smoke-image 138/138
cargo deny licenses
docs_guard
Curia strict
writable_globals
git diff --check
```

最终复跑结果：

```text
cargo test -p berry-ui             PASS
just test-host                     PASS
just check                         PASS
berry-ui no_std                    PASS
berry-ui guest-font-vmo no_std     PASS
uitest no_std                      PASS
gui-wmd host tests                 PASS 6/6
gui-wmd musl guest check           PASS
GUI8 self-test                    PASS
GUI8 normal-boot                  PASS
GUI8 dual-boot canonical persistence PASS
smoke-image 138/138                PASS
cargo deny licenses                PASS
docs_guard / Curia strict         PASS
writable_globals                  PASS
```
