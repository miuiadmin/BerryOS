# BerrySession P4 Window Policy Matrix 生产接线

> 版本：**1.0**｜2026-11-08
>
> 定位：P3 Click-to-Focus 后的窗口策略验收矩阵刀。P4 不新增 wire op、
> 不改 `VER_WM`、不引入第二焦点权威；用 gfxdemo 两个既有半屏窗与一个
> 临时锁定窗，把 desktop miss 焦点保持、HIDE/SHOW refocus、z-order 命中
> 焦点、锁定窗销毁后的 synthesized CANCEL 逐项接入真实 QMP 链。
> 执行序权威 =
> [GUI框架地基路线-v1.19.md](GUI框架地基路线-v1.19.md)。

## §1 结论

P4 后已真实证明的窗口策略矩阵：

```text
desktop miss                  焦点与 table version 保持不变
focused window HIDE           refocus 到表序最末可见窗
window SHOW                   显窗接焦
z 平局                         表序先建者胜
z 提升                         高 z 命中者胜
pointer lock 目标窗销毁        UP 合成 CANCEL + SYNTHESIZED 并清锁
```

仍然不做：

```text
click-to-raise
MRU focus 栈
SYSTEM 层窗口抢焦
window drag/resize
多点触控 focus 迁移
```

焦点唯一权威继续是 gui-wmd `WmTable`。gfxdemo 只通过既有
`WM_CREATE / WM_SET / WM_DESTROY / WM_TABLE_GET / INPUT_POLL /
INPUT_KEY_POLL` 驱动和观察，不在 guest 侧造第二真相。

## §2 desktop miss 焦点保持

测试态：

```text
left window  = 可见，持有 Keyboard 合约
right window = HIDE
点击点       = 原 right window 中心
```

gui-wmd 语义：

```text
pointer DOWN miss
→ 不调用 focus_from_pointer
→ 不 bump table version
→ 不设置 input lock
→ 无 SYSTEM session 时静默丢弃
```

guest 观察面：

```text
WMHIDEFOCUS ok hidden=<right> focused=<left> v=<n>
WMMISSREADY ok x=<x> y=<y>
WMMISSFOCUS ok focused=<left> pointer=0 keys=2 v=<n>
```

证明：

```text
HIDE 后 table version = n
desktop miss + KEY_A 后 table version 仍 = n
left/right pointer queue 均无事件
KEY_A down/up 只进 left session
```

也就是说，desktop miss 不清焦、不改变 Keyboard 目标。

## §3 HIDE / SHOW refocus

`WM_SET_F_VIS` 语义：

```text
HIDE 持焦点窗 → refocus_after_removal()
SHOW          → 该窗接焦
```

P4 证明链：

```text
right focused
→ HIDE right
→ left focused，right invisible
→ desktop miss 后 left 仍 focused
→ SHOW right
→ right focused
```

marker：

```text
WMHIDEFOCUS ok hidden=2 focused=1 v=536870921
WMMISSFOCUS ok focused=1 pointer=0 keys=2 v=536870921
WMSHOWFOCUS ok shown=2 v=536870922
```

该链钉死隐藏窗不得残留 Keyboard focus：若 `focused()` 指向隐藏右窗，
desktop miss 后 KEY_A 不可能只进 left KeyQueue。

## §4 z-order 命中焦点

两个 APP 层窗先恢复为：

```text
left  = [0,400)
right = [400,800)
```

随后把 right 移入重叠带：

```text
right x = 200
overlap = [200,400)
click   = (300,300)
```

z 平局：

```text
layer 相同
z 均为 0
wm_hit_test 平局 = 表序先建者胜
→ left 获得焦点
```

marker：

```text
WMZREADY ok x=300 y=300 z=0
WMZFOCUS ok winner=1 z=0 v=...
```

随后：

```text
SET right z=1
同一点再点击
→ right 命中并获得焦点
```

marker：

```text
WMZRAISE ok window=2 z=1
WMZFOCUS ok winner=2 z=1 v=...
```

z 变更本身不改焦点；焦点只跟随 pointer DOWN 的 hit-test winner。

## §5 锁定窗销毁后的 synthesized CANCEL

P4 创建第三个临时 APP 窗：

```text
window = 3
rect   = 24,96,160,96
z      = 1
owner  = gfxdemo rogue session
```

z=1 使该窗在重叠区域胜出，避免被表序更早的 left 窗截获。

GUI8 注入：

```text
raw pointer DOWN（不自动 UP）
```

guest 收到 DOWN 后：

```text
WM_DESTROY lock window
```

随后 GUI8 注入 raw UP。gui-wmd 语义：

```text
InputLock 仍指向已销毁窗
route_geometry(lock.session, lock.window) = None
UP → PHASE_CANCEL
flags |= PE_FL_SYNTHESIZED
InputLock 清除
事件投给原锁定 session
```

真实 marker：

```text
WMLOCKREADY ok window=3 x=24 y=96 w=160 h=96 v=536870929
WMLOCKDOWN ok window=3 destroyed=1
WMCANCEL ok window=3 phase=4 flags=2 v=536870930
```

GUI8 断言：

```text
phase = 4
flags & PE_FL_SYNTHESIZED != 0
```

不要求 `flags == 2`，因为 resampled 位可与 synthesized 位 OR 在一起。

HIDE 与 DESTROY 在 `route_geometry()` 中同走不可见/失踪路径；P4 实测
DESTROY 变体，HIDE 变体的合成 CANCEL 等价性在文档钉板，另测归后续。

## §6 无交互 boot 降级

P4 只在 P3 focus proof 成功进入交互态后执行：

```text
focus_keyboard_proof() == true → window_policy_proof()
focus_keyboard_proof() == false → WMFOCUSSKIP 后直接继续 legacy flow
```

Boot B / Boot C / normal boot：

```text
WMFOCUSSKIP ok ×1
WMFOCUS ok ×0
WMKEYFOCUS ×0
WMHIDEFOCUS / WMMISSFOCUS / WMSHOWFOCUS ×0
WMZFOCUS ×0
WMLOCKDOWN / WMCANCEL ×0
```

因此 P4 不给无 QMP 输入的 boot 增加第二组 30s 等待。

## §7 GUI8 精确断言

Boot A：

```text
WMFOCUSREADY ×1
WMFOCUSSKIP  ×0

WMFOCUS ok ×4
  P3 left/right ×2
  z tie left  ×1
  z top right ×1

WMKEYROUTE ok ×6
  P3 left down/up ×2
  P3 right down/up ×2
  desktop miss 后 left down/up ×2

WMKEYDROP bad ×0

WMHIDEFOCUS ×1
WMMISSREADY ×1
WMMISSFOCUS ×1
WMSHOWFOCUS ×1
WMZREADY ×1
WMZFOCUS ×2
WMZRAISE ×1
WMLOCKREADY ×1
WMLOCKDOWN ×1
WMCANCEL ×1
```

Boot B / Boot C / normal：

```text
WMFOCUSSKIP ×1
P4 矩阵 marker 全零
```

self-test 负例：

```text
缺 desktop miss 证明
缺 z-order 证明
缺锁定窗 CANCEL
Boot B 泄漏 P4 marker
键盘路由数漂移
WMFOCUS 计数漂移
P4 版本链漂移
```

## §8 回归

已通过：

```text
GUI8 self-test
GUI8 Boot A 矩阵
GUI8 dual boot + layout self-heal
GUI8 normal-boot

P2 normal Keyboard 计数不变：
  WMKEYDROP bad ×6
  授权 WMKEYROUTE down/up 各一
  UITESTKEY ×1

pointer/touch：
  ABS burst 零丢
  真实 CLICK
  真实 SWIPE
  owner-local route

六个 BerryUI app 真链不变
```

最终回归已通过：

```text
just test-host
just check
gfxdemo musl target check
gui-wmd musl test profile check
berry-guiproto / berry-gfx / berry-ui no_std
berry-ui guest-font-vmo no_std
berry-room no_std
GUI8 self-test
GUI8 dual boot + layout self-heal
GUI8 normal-boot
smoke-image 138/138
cargo deny licenses
docs_guard / Curia strict / writable_globals
```

## §9 A1 预算

当前 rootfs：

```text
总量       = 33,831,160B
单卷 cap   = 33,554,432B
rootfs0    = 33,554,432B
rootfs1    = 276,728B
总量 cap   = 134,217,728B
总量余量   = 100,386,568B
```

GUI 包体：

```text
BROWSER.bap  = 74,188B
SETTINGS.bap = 72,040B
FILES.bap    = 72,424B
PHOTOS.bap   = 92,000B
CONTACTS.bap = 75,444B
UITEST.bap   = 71,784B
```

增长主要来自 gfxdemo P4 状态迁移与 GUI8 反 mock 断言。

## §10 最终镜像

```text
SHA-256 = 6bce3e0b419ab8bcf321f183995377d5fff2453c3b7713c96dde8eadec08b4c2
```

## §11 未完成边界

- HIDE 变体的 synthesized CANCEL 未单独 runtime 注入；与 DESTROY 同走
  `route_geometry=None` 路径，等价性已由代码语义钉板。
- CANCEL 后 lock 清除缺少独立正向 re-lock 证明；当前由 wmd clear 代码
  与后续既有 pointer 流零回退覆盖。
- refocus fallback 仍是表序最末可见窗，不是 MRU。
- click-to-raise 未开放；z 变更只影响 hit-test。
- SYSTEM 层窗口仍未落地，SYSTEM 不抢焦仍无真实窗口正例。
- 多点触控 / 多显示器 focus 迁移未开放。
- gui-wmd host harness 仍缺，P4 由真实 QMP 链覆盖。
