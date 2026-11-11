# BerrySession R3 Config Event Architecture 生产接线

> 版本：**1.0**｜2026-11-11
>
> 定位：R2 raw/visible 双几何之后的事件面。R3 不做 resize 命令、display
> metadata、FormMetrics 或 SafeArea 容器；只让客户端能在既有严格串行 WM
> RPC 上取得一致的 per-window 配置快照，并用 serial ACK 收口消费。
> 执行序权威 = [GUI框架地基路线-v1.16.md](GUI框架地基路线-v1.16.md)。

## §1 背景

R2 后几何真值已分域：

```text
raw bounds     = buffer/stride/layout 基准
visible bounds = AVOID_SYS 可呈现/可命中域
occlusion      = host 推导 safe insets
```

但动态变化仍没有 app 侧事件面：

```text
TABLE_WAIT 是 inputd 单停车槽
wmd wait_many 16 handle 已满
Room 串行 RPC 无法接收主动 push
```

GUI-14 B7/D6 的结论是：在当前容量下只能做 poll-first。

## §2 工业语义对齐

R3 采用：

```text
Wayland xdg-shell:
  configure 是原子状态束
  多 configure 可丢弃旧值，只保留最新
  ack 必须精确；stale/unknown serial 拒绝
  未 configure 前不得提交 buffer

Fuchsia Flatland:
  首帧 Present 前可取 LayoutInfo
  logical_size 与 buffer scale 分离

Android edge-to-edge:
  raw bounds 与 insets 分通道
  insets 不缩小 buffer
```

Berry 对应：

```text
CONFIG_POLL = pull-once / poll-next configure
CONFIG_ACK  = exact serial ack
SUBMIT gate = 未同步 config 前拒绝 surface 使用
```

## §3 add-only wire

新增 op：

```text
0x6A OP_WM_CONFIG_POLL
0x6B OP_WM_CONFIG_ACK
```

不新增：

```text
handle
WM_CONNECT 语义
TABLE_WAIT 使用
status code
现有帧布局
```

### POLL 请求

```text
len = 4B
[0] op 0x6A
[1..4] reserved，双侧恒 0
```

### POLL 成功应答

```text
len = 32B
[0]  ST_OK
[1]  cause
[2]  visible bool（0/1）
[3]  reserved
[4]  serial u32
[8]  window_id u16
[10] reserved u16
[12] raw_x u16
[14] raw_y u16
[16] raw_w u16
[18] raw_h u16
[20] visible_x u16
[22] visible_y u16
[24] visible_w u16
[26] visible_h u16
[28] reserved u32（未来 scale/metadata add-only）
```

无事件：

```text
1B ST_EAGAIN
```

错误沿用 1B status。

### ACK 请求

```text
len = 12B
[0]  op 0x6B
[1..4]  reserved，双侧恒 0
[4]  window_id u16
[6]  reserved u16
[8]  serial u32
```

成功：

```text
1B ST_OK
```

### cause

```text
0 CREATED
1 GEOMETRY
2 VISIBILITY
3 OCCLUSION
```

未知值双侧拒绝。以下刻意不进 v1 cause：

```text
density / scale
rotation
IME
display id / posture
```

原因：没有真值源。scale 仍在 0x67，且当前恒 1000。

## §4 serial 与 coalescing

serial = 生成事件后的 WmTable version：

```text
同一表变可给多个窗相同 serial
ACK 地址 = (window_id, serial)
```

每个窗口只有一个 pending slot：

```text
新事件覆盖旧事件
未 ACK 的旧事件不再可读
只保留最新 full-state 快照
```

因此：

```text
A → B → A 的净状态可能与旧状态相同
仍会产生新 serial 事件
```

这是刻意选择：客户端收到的是可幂等应用的完整状态，不是必须重放
的 delta。

### no-op / focus

以下不产生事件：

```text
Z-only SET
GEOMETRY 位存在但值不变
VIS 位存在但值不变
focus bump
```

事件只由状态差异驱动。

### counter 回绕

table version：

```text
epoch u8 << 24 | counter u24
```

counter 回绕时递增 epoch，避免 `2^24` 次表变后 serial 重复。极端
`epoch=0xFF` 终局只清空已成旧 pending、保留本 bump 刚生成的新事件与
已同步窗 liveness；这是约 `2^32` 表变后的终局防御，不作为常规重同步
路径。事件从不影响 WmTable 几何真值。

## §5 事件生成矩阵

### CREATE

新建窗口：

```text
cause = CREATED
raw = CREATE 申请几何
visible = 当前 AVOID_SYS 推导结果
```

既有 AVOID_SYS 窗若被新 SYSTEM occluder 改 eff：

```text
cause = OCCLUSION
```

### WM_SET

本窗：

```text
raw 变化        → GEOMETRY
vis 变化        → VISIBILITY
```

其他窗：

```text
只有 SYSTEM 层变化能改变 AVOID_SYS eff
eff 变化       → OCCLUSION
```

### DESTROY

被销毁窗不产生事件。幸存 AVOID_SYS 窗若 eff 恢复：

```text
OCCLUSION
```

session 死亡清理同理；pending 状态嵌在 `Win` 内，随窗口一起消失。

## §6 ACK 语义

按序检查：

```text
window 不存在       → ST_NOENT
window 属他人       → ST_DENIED
无 pending          → ST_DENIED
serial 不精确匹配   → ST_DENIED
匹配               → 清 pending，ST_OK
```

错 serial、旧 serial、重复 ACK 都必须重新 POLL 最新状态。没有
“>= last” 排序接受，也没有历史 ACK ring。

ACK 不改变几何真值，不触发提交，只是消费 advisory config。

状态 split：

```text
0x67：窗口缺失/隐藏 → ST_NOENT；config gate → ST_DENIED
SUBMIT：surface 绑定失败（缺失/隐藏/gate）→ ST_DENIED
```

## §7 未配置提交 teeth

0x67 先经 config gate，再进入双几何/可见性检查；SUBMIT 的 surface 绑定
合并返回 ST_DENIED。两者都要求：

```text
config 已同步
当前无 pending config
```

因此 CREATE 后未 POLL/ACK：

```text
0x67 → ST_DENIED
SUBMIT → ST_DENIED
```

这对应 Wayland “未 configure 不得 attach buffer” 的牙齿。它不改变
R2 几何执法，只是在进入 surface 使用前强制取得一致初始配置。

动态 OCCLUSION/VISIBILITY 未 ACK 时，同样拒绝下一帧 surface 使用；
客户端应：

```text
POLL → 应用 raw/visible → ACK → 重新布局/渲染 → SUBMIT
```

## §8 Room 消费

新增安全 API：

```rust
AppRoom::poll_config()
AppRoom::ack_config()
```

`open_window()` 流程改为：

```text
WM_CREATE
CONFIG_POLL
校验 CREATED/raw display/visible
CONFIG_ACK
分配 raw display VMO
```

R2 的 TABLE_GET bootstrap 被替换，普通 Room app 不再读取全窗口表。

打开后的 `poll_config()`：

```text
OCCLUSION/VISIBILITY → 刷新 occlusion snapshot
raw bounds 变化      → ConfigBad（v1 固定 VMO 不得静默重解释）
```

`submit()` 在帧边界先排空并 ACK pending config；若事件在 POLL→ACK 间被
并发表变更覆盖，会重新 POLL 最新 full-state。需要按 insets 重排的业务仍应
在构建 DisplayList 前显式 `poll_config()`；自动 BerryUI config pump /
SafeArea 容器归后续刀。隐藏事件仍携历史几何槽，Room 会保留其推导
occlusion；hidden-window 语义归后续 P3 决策。

## §9 GUI8 / 反 mock

### gfxdemo

每个真实窗口 CREATE 后 drain/ACK config。新负例：

```text
WMCONFIGSUBMITDENY       未 ACK CREATED 前 SUBMIT 拒绝
WMCONFIGSURFACEDENY      未 ACK CREATED 前 0x67 拒绝
WMCONFIGFOREIGNACK       foreign session 精确 serial ACK 拒绝
WMCONFIGDESTROYEDACK     已销毁窗旧 ACK → ST_NOENT
WMCONFIGNOOPEMPTY        同值 GEOMETRY SET 不产事件
WMCONFIGHIDE             VISIBILITY cause + visible=false
WMCONFIGSHOW             VISIBILITY cause + visible=true
WMCONFIGFOCUSEMPTY       focus-only bump 不产事件（Boot A）
```

### launcher 哨兵相

APP route 窗在真实 SYSTEM occluder 下验证：

```text
WMCONFIGCREATED     CREATED pull-once + raw/visible 校验
WMCONFIGACKDENY      错 serial ACK 拒绝
WMCONFIGEMPTY       ACK 后 poll 空
WMCONFIGCOALESCE    HIDE+SHOW 两次变化只保留最新 OCCLUSION
WMCONFIGSTALE       已消费 serial 重发 ACK 拒绝
```

`WMCONFIGCOALESCE` 的 serial 必须不同于 CREATED，visible 必须为最终
SHOW 后状态；中间 HIDE 事件不可读。

### normal boot

机器断言：

```text
SUBMITDENY / SURFACEDENY / FOREIGNACK / DESTROYEDACK
NOOPEMPTY / HIDE / SHOW     ×1
FOCUSEMPTY / WMCONFIGWRAP   ×0
launcher 五个哨兵 marker    ×0
```

六个 BerryUI app 走 Room CREATE→POLL→ACK，P2/P3/P4 零回退。

## §10 host 测试

新增 `berry-guiproto/tests/config_event.rs`：

```text
POLL request golden
ACK request golden
32B event golden
roundtrip
ST_EAGAIN / ST_BAD 三态
reserved byte reject
cause 值域
visible bool 值域
serial/window 0 reject
truncation/trailing/op-confusion reject
```

## §11 未完成边界

- 仍是 poll-first，无 per-session push/wait；wait_many 16 handle 帽未解。
- 无 app→host geometry request；Apple 式可拒绝 request 归 P7 预研。
- 无 resize buffer 生命周期；Room 固定 VMO 对 raw GEOMETRY loud 拒绝。
- scale/density/rotation/display metadata 无生产者，不入 event。
- IME transient occlusion 归 P6b。
- BerryUI 尚无自动 config pump / SafeArea 容器；app 需在帧边界显式
  poll/ack。
- 多窗 session poll 按表序逐条返回，不做跨窗一致性快照承诺。
- TABLE_GET/TABLE_WAIT 仍是既有全表面；本刀只移除 Room 初始 bootstrap
  的常规使用。

最终回归已通过：

```text
berry-guiproto host：46 unit + 23 integration
berry-room host snapshot：1
just test-host（含 berry-room）
just check
gui-wmd / launcher / gfxdemo target check
berry-guiproto / berry-gfx / berry-ui no_std
berry-ui guest-font-vmo no_std
berry-room no_std
GUI8 self-test
GUI8 normal-boot
GUI8 dual boot + layout self-heal（A/B/C 事件矩阵）
smoke-image 138/138
cargo deny licenses
docs_guard / Curia strict / writable_globals
```

## §12 预算

当前构建：

```text
总量       = 33,870,696B
单卷 cap   = 33,554,432B
rootfs0    = 33,554,432B
rootfs1    = 316,264B
总量 cap   = 134,217,728B
总量余量   = 100,347,032B
```

rootfs0 仍恰在单卷帽。后续继续加载荷必须先给拆卷/压缩/迁出方案。

## §13 最终镜像

当前候选 SHA-256：

```text
6c9dd8612f6621e11891c644ba322ba9d92d639ffbb69f6b87f68cc12300a986
```

复跑镜像 SHA-256 即上值。
