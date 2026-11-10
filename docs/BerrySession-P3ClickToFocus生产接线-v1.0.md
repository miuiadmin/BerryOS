# BerrySession P3 Click-to-Focus 生产接线

> 版本：**1.0**｜2026-11-08
>
> 定位：P2 Keyboard Input 后的窗口策略刀。P3 在 gui-wmd 权威 pointer
> DOWN 命中路径更新窗口焦点，并让 Keyboard KeyQueue 跟随点击目标。
> 不新增 wire op，不改 `VER_WM`，不引入 click-to-raise 或第二焦点权威。
> 执行序权威 = [GUI框架地基路线-v1.8.md](GUI框架地基路线-v1.8.md)。

## §1 语义

P3 后焦点触发源为：

```text
CREATE              新可见窗接焦（既有）
SET_F_VIS SHOW      显窗接焦（既有）
pointer DOWN 命中   点击窗接焦（P3）
焦点窗 HIDE/DESTROY refocus_after_removal（既有）
owner 死亡          destroy_owned → refocus（既有）
```

pointer DOWN 规则：

```text
hit-test 命中真实可见 APP 层窗口 → table.focus = 命中窗
hit-test 命中 SYSTEM 层窗口     → 不改焦点
desktop miss                     → 焦点保持不变
点击当前焦点窗                   → no-op，不 bump，不打标记
```

桌面 miss 不清焦：v1 桌面由 launcher 以 surface_id=0 呈现，不是 WM 窗口；
造 window=0 焦点会形成第二语义。点击桌面后键盘仍路由到原焦点窗。

## §2 快照与输入锁

焦点变更走 `WmTable::focus_from_pointer()`：

```text
窗口必须存在、可见、APP layer
若焦点已相同 → no-op
否则 focus 更新 + bump()
```

`bump()` 语义不变：

```text
table version + 1
向当前 parked WM_TABLE_WAIT 推送全量快照
WmEntry focused flag 跟随移动
```

证据 marker：

```text
WMFOCUS ok sid=<owner> window=<id> v=<table_version>
```

输入锁与焦点正交：

```text
pointer lock = DOWN→MOVE→UP 粘滞路由
focus        = Keyboard / 后续窗口策略权威
```

同一次 DOWN 的 lock window 与新 focus window 相同；MOVE/UP 不再改焦点。

## §3 启动序调整

P3 前 svcmgr 顺序：

```text
gui-wmd → gfxdemo 阻塞 reap → ... → gui-inputd
```

P3 双窗证明需要真实输入链先在席，因此调整为：

```text
gui-wmd → gui-inputd → gfxdemo 阻塞 reap → ... → launcher
```

影响：

```text
inputd 的 INPUT/INPUTDEV 基线早于 gfxdemo
wmd push 链在 gfxdemo 前建立
pointer/touch 后续行为不变
```

Boot B/C 无 QMP 焦点注入时，gfxdemo 显式降级：

```text
WMFOCUSSKIP ok reason=no-pointer
```

不伪造证明；skip 前有 30s 有界等待，随后继续既有 marker 流。

## §4 gfxdemo 双窗验收车

gfxdemo 原本已有两个真实半屏窗口：

```text
session 1 / window 1: left  [0, w/2)
session 2 / window 2: right [w/2, w)
```

P3 为两窗均协商：

```text
Retained
Touch | Keyboard
IME Disabled
a11y Tree
```

Retained Keyboard 不需要 P1 per-boot escape seed；Canvas/Immersive escape
仍继续受 seed 约束。

guest ready marker：

```text
WMFOCUSREADY ok left=1 right=2
left_rect=0,0,400,600
right_rect=400,0,400,600
```

GUI8 注入序列：

```text
1. QMP click left center
2. 等 WMFOCUS ok sid=1 window=1 v=<n>
3. QMP KEY_A down/up
4. 等 WMKEYFOCUS ok side=left

5. QMP click right center
6. 等 WMFOCUS ok sid=2 window=2 v=<m>
7. 断言 m > n
8. QMP KEY_A down/up
9. 等 WMKEYFOCUS ok side=right
```

最终：

```text
WMFOCUSKEY ok left=1 right=2 proofs=2
```

真实日志：

```text
WMFOCUSREADY ok left=1 right=2 ...
WMFOCUS ok sid=1 window=1 v=2684354567
WMKEYFOCUS ok side=left window=1 pointer=1 keys=2
WMFOCUS ok sid=2 window=2 v=2684354568
WMKEYFOCUS ok side=right window=2 pointer=1 keys=2
WMFOCUSKEY ok left=1 right=2 proofs=2
```

KeyQueue 证据：

```text
左窗点击后 KEY_A 只进 session 1
右窗点击后 KEY_A 只进 session 2
共 WMKEYROUTE ok ×4
零 key fallback 到旧焦点窗
```

## §5 no-op 与清理语义

已聚焦窗再次点击：

```text
WMFOCUS 新增 0
table version 不变
```

Boot A 实测 gfxdemo 死亡后 WM refocus 已落在后续 route window；GUI8 再点
该窗时不产生第三次 `WMFOCUS`，钉板 no-op 面。

隐藏窗：

```text
不参与 hit-test，不能获得点击焦点
若为焦点窗则 refocus_after_removal
```

销毁 / owner 死亡：

```text
窗口摘除
Surface Keyboard 合约随窗口清理
KeyQueue 随 WmSession 清理
table refocus + bump
```

既有证据继续覆盖：

```text
WMDEAD ok id=...
WMDEAD ok id=... wins=...
WMDTY ok ...
```

## §6 不做的事情

```text
不做 click-to-raise
不引入 WM_SET_FOCUS op
不让 app/inputd 写焦点
不做 MRU focus 栈
不清 desktop focus
不处理系统层窗口抢焦
```

z 序仅影响 hit-test winner：

```text
点击谁命中，谁获得焦点
```

raise 归后续窗口策略刀。

## §7 测试与守卫

已落地：

```text
GUI8 self-test：
缺 WMFOCUSREADY FAIL
缺一侧 WMKEYFOCUS FAIL
缺焦点快照 FAIL
键盘路由数漂移 FAIL
缺已聚焦 no-op 证明 FAIL
WMFOCUS 计数漂移 FAIL

GUI8 full Boot A：
WMFOCUSREADY ×1
WMFOCUS ok ×2
WMKEYFOCUS left ×1
WMKEYFOCUS right ×1
WMFOCUSKEY ×1
WMFOCUSSKIP ×0
WMKEYROUTE ok ×4
WMKEYDROP bad ×0
WMSNAPFOCUS ×1
WMFOCUSNOOP ×1

GUI8 full Boot B/C：
WMFOCUSSKIP ×1
WMFOCUS ok ×0
WMKEYFOCUS ×0
不伪造焦点证明

GUI8 normal：
WMFOCUSSKIP ×1
WMFOCUS ok ×0
WMKEYFOCUS ×0
P2 四段 Keyboard 计数保持
WMKEYDROP bad ×6
授权 WMKEYROUTE down/up 各一
UITESTKEY ×1
六 BerryUI app 真链零回退
```

版本递增断言：

```text
right focus version > left focus version
```

marker-gated 顺序：

```text
QMP key 必须等 WMFOCUS marker 后注入
避免 key 先于 click harvest 时按旧焦点误投
```

最终回归已通过：

```text
just test-host
just check
gui-wmd musl test profile check
svcmgr none target check
gfxdemo musl target check
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

## §8 预算

当前 rootfs：

```text
总量       = 33,826,600B
单卷 cap   = 33,554,432B
rootfs0    = 33,554,432B
rootfs1    = 272,168B
总量 cap   = 134,217,728B
总量余量   = 100,394,328B
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

增长主要来自：

```text
wmd focus snapshot marker
gfxdemo 双通道 input/key poll
svcmgr 启动序调整
GUI8 P3 focus phase
```

## §9 最终镜像

```text
SHA-256 = 6484dd6032be8dddfe43f83c58e73a06dedcbc70dcbccb50c78ee17e931689c2
```

## §10 未完成边界

- click-to-raise 未开放。
- MRU / z-top focus 栈未开放；refocus 仍是表序最末可见窗。
- SYSTEM 层窗口点击不抢焦；真实系统层窗口仍未落地。
- P4 已补 desktop miss、HIDE/SHOW、z-order 与 stale lock CANCEL 验收矩阵。
- gui-wmd 仍无本机 host 测试 harness；P3 通过 GUI8 双窗真链覆盖。
- 多点触控 / 多显示器 focus 迁移未开放。
- Window drag/resize 策略未开放。
