# BerrySession R1 Window/Surface Authority Hardening 生产接线

> 版本：**1.0**｜2026-11-11
>
> 定位：GUI-14 二轮对抗深研后的第一实施刀。R1 不做 FormMetrics、safe inset
> 或多窗，只先关 GUI 权威面的安全洞：SYSTEM 身份、SYSTEM layer、全局
> surface、跨 owner parent 与 damage 越窗。执行序权威 =
> [GUI框架地基路线-v1.18.md](GUI框架地基路线-v1.18.md)。

## §1 背景

GUI-14 发现 P0/P1/P4 后仍存在以下高置信问题：

```text
System role 生产不可达
APP session 可自报 SYSTEM layer
client_type 自声明
surface_id=0 是任意 session 的全局涂刷原语
damage 只按屏幕裁剪，可越窗绘制
parent 只验在册，不验同 owner
CREATE/SHOW/refocus 对 SYSTEM layer 层盲抢焦
```

这些问题会阻塞后续：

```text
SYSTEM-layer safe inset
IME Host / 软键盘
system UI / status bar 迁移
multi-window / popup
```

R1 的目标是先把“谁能造系统层、谁能画全局面、谁能画进别的窗”关死。

## §2 SYSTEM client 能力链

新增 per-boot SYSTEM seed：

```text
svcmgr
  → 0x69 OP_WM_SYSTEM_SEED 预共享给 gui-wmd
  → 同 seed 以 16 字符 hex argv 传给 berry-launcher
  → launcher 铸一次性 seed channel
  → WM_CONNECT(SYSTEM) 第 4 handle 随行
  → gui-wmd 验证后建立 SYSTEM session
```

控制帧：

```text
OP_WM_SYSTEM_SEED = 0x69
len = 9B
payload = op + seed[8]
截断/尾随/错 op 拒绝
```

与 P1 escape seed 的关系：

```text
0x68 escape seed：APP Canvas/Immersive escape 授权
0x69 system seed：SYSTEM client 身份授权
二者独立，不能互换
```

wm connect 规则：

```text
APP:
  hs = [wm, input, process]
  或 + escape seed channel

SYSTEM:
  hs = [wm, input, process, system seed channel]
  seed 必须 equals per-boot system seed
  只允许一个 SYSTEM session
```

自报行为：

```text
WM_CLIENT_SYSTEM + 无 seed / 错 seed → ST_DENIED
APP + SYSTEM seed 当 escape seed → ST_DENIED
client 晚到/重复 0x69 → ST_DENIED
```

seed 混合使用独立时间样本、escape seed、不同 label 与 splitmix64 形
finalizer，避免“两类 seed 只差同源输入线性式”。它仍是 v1 过渡熵源，
真熵刀前不宣称 CSPRNG。

GUI8 真实证据：

```text
WMSYSAUTH ok id=...
WMSYSCLIENTDENY ok
WMSYSSEEDDENY ok
WMSYSCTRLDENY ok
```

## §3 WM layer 与 parent 权限

新增 `berry-guiproto::authority` 纯策略模块：

```rust
wm_create_layer_allowed(client_type, layer)
wm_parent_allowed(parent, parent_owner, owner, id)
wm_create_flags_allowed(client_type, flags)
wm_policy_focus_allowed(layer)
clip_damage_to_surface(rect, w, h)
```

WM_CREATE 规则：

```text
APP session    → 只能创建 WM_LAYER_APP
SYSTEM session → 只能创建 WM_LAYER_SYSTEM
未知 client     → 拒绝
```

parent 规则：

```text
parent = 0           顶层合法
parent != 0          必须在册且同 owner
parent 不得等于新窗 id
```

AVOID_SYS：

```text
APP 可声明
SYSTEM 层自身不得声明
```

GUI8 真实负例：

```text
APP 自报 SYSTEM layer → WMLAYERDENY ok
rogue session 挂 hw session 窗口 → WMPARENTDENY ok
```

拒绝路径不消耗 `alloc_id`：layer/flags/parent/full 检查都先于窗口 id
分配。否则安全负例会使后续 runtime app 的精确 window id 漂移。

## §4 全局 surface 与 damage 权限

### surface_id=0

`surface_id=0` 是 wmd 聚合帧/全局桌面 surface，只允许 SYSTEM session 提交：

```text
APP submit surface_id=0 → ST_DENIED
SYSTEM submit surface_id=0 → 允许
```

GUI8 真实负例：

```text
APP session submit surface_id=0 → WMGLOBALDENY ok
```

### damage 裁剪

非 FULL damage 先与本 surface 本地矩形求交：

```text
clip_damage_to_surface(r, src_w, src_h)
```

语义：

```text
完全越界 → 丢弃该 rect
部分越界 → 裁剪到本窗
整批裁完为空 → ST_BAD
输出到 present 的 rect 加窗口原点
```

因此 oversized VMO 不能把像素画到邻窗。

GUI8 真实负例：

```text
owner 自己提交完全越窗 damage → WMDAMAGEBAD ok
```

## §5 SYSTEM 层焦点策略

P3 只在 pointer focus 路径排除了 SYSTEM layer；R1 补齐策略焦点路径：

```text
CREATE SYSTEM window → 不接焦
SET_F_VIS SHOW SYSTEM → 不接焦
refocus_after_removal → 只在 APP layer 中选回焦窗
```

SYSTEM 仍可作为 overlay / future IME host，但不得因创建或显示抢占 Keyboard
focus。

## §6 System role 授予策略

新增 session-aware Surface policy：

```rust
surface_mode_session_policy(
    client_type,
    req,
    escape,
    old,
    display_w,
    display_h,
)
```

规则：

```text
APP    → role 必须 App
SYSTEM → role 必须 System
SYSTEM → mode 必须 Retained
SYSTEM → 无 Escape
Gamepad / IME Host 仍 ST_UNSUPPORTED
escape→Retained 降级仍拒绝
```

既有 `surface_mode_production_policy` 保持为 APP 便捷入口，测试与旧调用零漂。

当前生产 launcher 尚未发送 System role 0x67；本刀先保证策略可达且 host
测试覆盖，System UI/IME 表面归后续刀。GUI8 同时加入 APP 自报 System role
负例：

```text
WMROLEMASQDENY ok
```

## §7 GUI8 / 反 mock

每个 EFI boot 都精确断言：

```text
WMSYSAUTH            ×1
WMSYSCLIENTDENY      ×1
WMSYSSEEDDENY        ×1
WMSYSCTRLDENY        ×1
WMROLEMASQDENY       ×1
WMLAYERDENY          ×1
WMPARENTDENY     ×1
WMGLOBALDENY     ×1
WMDAMAGEBAD      ×1
```

Boot A 真链：

```text
SYSTEM 自报拒绝
launcher SYSTEM seed 正例
APP SYSTEM layer 拒绝
跨 owner parent 拒绝
APP 全局 surface 拒绝
越窗 damage 拒绝
```

normal boot 同样保留这些 authority 基线。

Host 测试新增：

```text
system seed codec strict
session-aware surface policy
wrong SYSTEM seed / late 0x69 / role masquerade real negatives
layer authority matrix
parent authority matrix
focus policy
damage local clip
```

## §8 未完成边界

- AVOID_SYS effective geometry 与 Room request-size buffer 的错位仍未修，
  归 R2 Occlusion Contract。
- SYSTEM session 字形供给仍未开放；IME/system UI 归后续刀。
- per-boot SYSTEM seed 仍非 CSPRNG，熵源与失败锁定归安全硬化后续刀；
  launcher argv 以 hex 保存 seed 至进程生命期，未来 argv/proc/crash dump
  面开放前必须改为一次性 handle gift。
- SYSTEM layer 生产 UI 正例尚未创建；R2 的 GUI8 哨兵相已加入真实
  SYSTEM-layer 遮挡窗，但常态 launcher 状态栏迁移仍未发生。
- configure/inset/resize 事件架构仍缺失，归 R3。
- compositor z/occlusion 仍未实现，归 multi-window 设计刀。
- gui-wmd 仍无完整 host harness；R1 用 guiproto authority 纯函数 + GUI8
  真链覆盖。
- R2 已补部分越界 damage 的像素级 e2e：`WMDAMAGECLIP` 证明窗内 8px 落
  品红、右邻 4px 保持原底色。

最终回归已通过：

```text
berry-guiproto host：46 unit + 15 integration
just test-host
just check
gui-wmd / svcmgr / launcher / gfxdemo target check
berry-guiproto / berry-gfx / berry-ui no_std
berry-ui guest-font-vmo no_std
berry-room no_std
GUI8 self-test
GUI8 normal-boot
GUI8 dual boot + layout self-heal
smoke-image 138/138
cargo deny licenses
docs_guard / Curia strict / writable_globals
```

## §9 预算

当前构建：

```text
总量       = 33,837,152B
单卷 cap   = 33,554,432B
rootfs0    = 33,554,432B
rootfs1    = 282,720B
总量 cap   = 134,217,728B
总量余量   = 100,380,576B
```

rootfs0 已恰在单卷帽（0B 余量）。下一刀若新增 rootfs 载荷，必须先给
拆卷/压缩/迁出 cap 方案，不得静默越过。

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
authority 纯策略与测试
SYSTEM seed 控制帧
svcmgr/launcher/wmd 授权链
gfxdemo 五类真实负例
```

## §10 最终镜像

当前构建 SHA-256：

```text
bccac6d76f8fcbd15850007b15f66ea08094fab48b5304fa3be04193c3b13194
```

复跑镜像 SHA-256 即上值。
