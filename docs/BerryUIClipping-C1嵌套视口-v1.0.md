# BerryUIClipping C1 嵌套视口裁剪

> 版本：**1.0**｜2026-11-08
>
> 定位：F9 后的首个 BerryUI 地基补刀。目标不是给某个 app 私造滚动效果，而是把“结构性视口裁剪”下沉为 BerryUI retained tree / BerryGFX DisplayList 的通用能力。执行序背景见 [GUI框架地基路线-v1.13.md](GUI框架地基路线-v1.13.md)；组件 API 见 [BerryUI-F4组件第一版-v1.1.md](BerryUI-F4组件第一版-v1.1.md)，guest 验收见 [BerryUI-F5系统验收-v1.0.md](BerryUI-F5系统验收-v1.0.md)，F9 边界守卫见 [BerryGUILanding-F9收口与守卫-v1.0.md](BerryGUILanding-F9收口与守卫-v1.0.md)。

## §1 问题与裁决

F5/F6/F7/F8 连续记录的挂账是同一件事：

```text
ScrollView 内容可溢出 Panel
SwipePager 越界页可压到相邻区域
子节点命中不受父视口裁剪
Text/RoundRect/Image 无法用简单几何交矩形精确裁剪
```

旧系统只有三层裁剪：

```text
damage ∩ viewport
paint damage culling
raster dirty clip
```

缺第四层：

```text
retained subtree clip
```

C1 裁决：

```text
Clip 是树结构语义，不是 Ui 运行时侧表
精确像素裁剪归 BerryGFX DisplayList
剔除/命中归 BerryUI
普通 app 只能使用 BerryUI Clip，不得直录 gfx Push/PopClip
```

## §2 BerryGFX DisplayList 裁剪

新增命令：

```text
PushClip { rect }
PopClip
```

`Queue::submit` 语义：

```text
初始 clip = texture bounds ∩ pass dirty
PushClip = current clip ∩ rect 入栈
PopClip  = 恢复上一层
所有 Draw/Fill/Glyph/Image 命令按 current clip 光栅
```

容量与拒面：

```text
CLIP_STACK_MAX = 16
多余 Pop       → SubmitError::UnbalancedClip
结束时未归零  → SubmitError::UnbalancedClip
超过 16 层    → SubmitError::ClipStackFull
负宽高        → DisplayList 录入期 CmdError::BadRect
```

所有 Push/Pop 栈错误在写入前先验，目标缓冲零半执行。BerryUI 作为 producer
另有命令水位不变量：每个已打开 Clip 都预留 Pop 槽，Push 无法配平时整棵
子树跳过，叶子满帽只截尾；因此 BerryUI 产出的 DisplayList 不会因命令满帽
留下 dangling Push / orphan Pop。外部手写列表若失衡，仍由 Queue 整表拒绝。

`raster::exec` 单命令直调面把 Push/Pop 视为 no-op；栈语义唯一归 Queue，避免第二套裁剪状态机。

## §3 BerryUI 结构性 Clip

新增 AST kind：

```text
ViewKind::Clip = 11
Props::Clip(ClipProps)
```

AST 形态：

```text
kind 11
props 基域 0B
w/h 经既有 HAS_SIZE 块（8B 哨兵对）
```

因此：

```text
旧 kind 0..=10 流逐字节不变
旧读端对 kind=11 继续 UnknownKind 拒绝
Node 仍冻结 144B / 19 字段
UiError / Msg / FrameOutcome 不扩面
```

布局语义：

```text
Clip 是透明容器
intrinsic：按子内容聚合；Some(w/h) 覆盖
final size：None 吃父可用域，Some 固定 port
子节点从 (0,0) 起在 Clip bounds 内布局
Clip 自身无视觉发射
```

`Ui::bounds(node)` 继续返回未裁剪视觉界；新增：

```rust
Ui::clip_of(node) -> RectI
```

返回自身与祖先 Clip 链、viewport 的交集，用于诊断与精确 damage。

## §4 Paint 裁剪

`paint.rs` 的剔除走查线程化传递：

```text
effective clip = ancestor clip ∩ current Clip bounds
```

Clip 子树可见性判断：

```text
node bounds ∩ damage ∩ effective clip
```

发射序使用 `order[]` 哨兵，不新增分配：

```text
逆收集序先占 Pop
收集子树
末录 Push
倒序发射得到 PushClip → subtree → PopClip
```

嵌套 Clip 按外层→内层自然嵌套；兄弟 Clip 各自弹回。病态巨树触发 `emit_walk_all` 回退路时，回退路同样压/弹 Clip，不因剔除缓冲溢出穿帮。

命令预算配平：

```text
PushClip 录入条件 = encoder.len + open_clips + 2 ≤ MAX_CMDS
叶子录入条件     = encoder.len + open_clips < MAX_CMDS
每打开一层 Clip = 预留一槽 Pop
预算不足的 Clip 子树整体跳过
防御性环/死槽退出前也先弹已打开 Clip
```

满帽截尾叶子的 `drawn_px` 仍按可见几何记账，`culled` 同步计数；这与命令录入失败不减少绘制足迹的既有 PaintReport 契约一致。

`drawn_px` 按节点界 ∩ viewport ∩ effective clip 统计，overdraw 不再把被裁掉的区域当作绘制足迹。

## §5 Hit 裁剪

`hit_walk` 与 paint 使用同一有效 Clip 链：

```text
点在有效 clip 外 → 整个子树不命中
点在 Clip port 内 → 继续子优先 topmost 命中
Clip 本身作为容器可命中
```

Clip 容器参与命中的意义：

```text
ScrollView 空白区域可锚定拖拽
滚动出视口的 item 不可 Press/Click
SwipePager 空白页带可锚定横滑
越界页内容不可命中
```

## §6 Damage 语义

布局 damage 仍保持安全超集。

`Ui::set_offset`（ScrollView / SwipePager / app 直控 transform 的共用口）收紧为：

```text
before/after 子树包络 ∩ 旧有效 Clip
before/after 子树包络 ∩ 新有效 Clip
再经 viewport clip
```

两种入账覆盖：

```text
固定 port 内滚动：只重绘 port 内旧新并集
Clip 节点自身移动：旧/新 port 可见域都入账
```

无 Clip 祖先时行为退化为既有 viewport 裁剪；旧 damage golden 不漂。

## §7 组件迁移

### ScrollView

新结构：

```text
Panel(surface, fixed viewport height)
└── Clip(w=parent, h=viewport)
    └── Col(content)
```

行为：

```text
Panel 背景仍完整绘制
内容 offset 可为负
内容 paint/hit 被 Clip 到 viewport
port 外内容不泄漏、不可命中
```

`ScrollView.on_msg` 的拖拽锚定节点加入 Clip port。

### SwipePager

新结构：

```text
Clip(w=viewport width)
└── Row(track)
    ├── page 0
    ├── page 1
    └── ...
```

行为：

```text
track 横向偏移时只显示当前 viewport 宽度内内容
相邻页不压到 dot row / 之外区域
dot row 仍是 Clip 兄弟，不随 track offset 移动
```

### 普通 app

六个 BerryUI app 无需手写裁剪逻辑；ScrollView 结构升级自动生效。F9 静态守卫新增禁令：

```text
普通 GUI app 不得直接调用 berry-gfx push_clip/pop_clip
```

Clip 是 BerryUI 树语义；直录 DisplayList Clip 命令属于绕过 retained tree。

## §8 测试证据

### BerryGFX

新增 `tests/clip_stack.rs`：

```text
Push/Pop 后恢复外层
嵌套 Clip 求交
失衡先验拒绝且零写入
16 层帽拒绝
负矩形录入期拒绝
```

BerryGFX 全量既有 raster/image/font/reject 测试保持全绿；
clip_stack 共 6 测试，含命令满帽缺 Pop 的整表拒绝。

### BerryUI

新增 `tests/clipping.rs`：

```text
Clip 布局与 bounds/clip_of 语义
DisplayList 精确命令流
port 外像素哨兵零写入
兄弟区域不被溢出子树污染
hit 与 paint 同一裁剪链
嵌套 Clip 求交
set_offset damage 折叠到祖先 Clip
AST roundtrip
DescBuilder begin_clip + size 与 direct add 字节收敛
命令满帽 256 条仍 Push/Pop 配平且 Queue submit 成功
kind=11 add-only / unknown kind=12 拒面
```

`tests/widgets.rs` 新增 ScrollView 真实溢出：

```text
content bounds 保持未裁剪
port 内内容可见
port 外像素哨兵
滚出 item 不可命中
Panel 背景 + 有效内容足迹计入 drawn_px
```

`tests/swipe.rs` 节点账重钉：

```text
8 页 pager = root + Clip + track + 8 pages + dot row + 8 dots
= 20 nodes
launcher 512 帽余量重钉
```

### UITEST / GUI8

UITEST 内嵌滚动样本从“刚好放得下”改为真实溢出：

```text
ListItem 高度 = viewport + 48
runtime 首帧 offset = 24
boot probe 保留 offset = 24
```

Guest 自证后输出：

```text
UITESTCLIP ok port=... item=... probe=... seq=...
```

自证条件：

```text
clip_of(item) == ScrollView port
item bounds 覆盖 port 外
port 下方探针不得命中 item
真实 render + Room submit 后才打印
```

GUI8 精确断言：

```text
UITESTCLIP ×2
UITESTRENDER ×10
UITESTAST bytes=712
ScrollView port/item/probe 几何一致性
owner-local 输入坐标交叉验证不回退
```

## §9 预算

当前 C1 rootfs：

```text
总量       = 33,802,144B
rootfs0    = 33,554,432B（恰单卷帽）
rootfs1    = 247,712B
总量 cap   = 134,217,728B
总量余量   = 100,415,584B
```

GUI 包体：

```text
BROWSER.bap  = 74,100B
SETTINGS.bap = 71,952B
FILES.bap    = 72,264B
PHOTOS.bap   = 91,920B
CONTACTS.bap = 75,332B
UITEST.bap   = 68,592B
```

增长来源：

```text
BerryGFX Push/Pop + Queue 栈
BerryUI Clip AST/layout/paint/hit
ScrollView/Swipe 每个 viewport 新增 Clip 节点
UITEST 真实溢出样本与 marker
```

系统 A8 字表仍未复制进 app。

## §10 回归边界

已通过：

```text
BerryGFX 全量测试
BerryGFX no_std
BerryUI 默认面全量测试
BerryUI --features s7
BerryUI no_std
BerryUI guest-font-vmo no_std
gui-boundary-guard --self-test（C1 时点 18 类负样本；P1 起 21 类）
gui-boundary-guard
UITEST no_std
just check
GUI8 self-test
GUI8 normal-boot
GUI8 dual boot + layout self-heal
smoke-image 138/138
```

host/ABI/license/docs/global/GUI8 full/smoke-image 均已按最终源码与最终镜像复跑通过。

## §11 最终镜像

```text
SHA-256 = ade8806045aafa051beb2c9108d0fd5a8ab8fa5c55a897d043c058273fee4f48
```

## §12 未完成边界

- 复杂嵌套滚动交互（双轴滚动、滚动条、nested scroll linking）未扩张。
- Transform 动画 damage 仍以安全超集为主；只有 `set_offset` 公共口收紧到 Clip 链。
- Clip 只有矩形 port；轴向裁剪、圆角 clip、stencil/alpha mask 未开放。
- C1 时点 CanvasSurface / immersive buffer 未接线；P1 已落地授权 allowlist 与 Berry Room buffer 借用。
- BerryUI clip 栈深为 16；超深结构性 Clip 在 paint/hit 中按空可见域 fail closed。
