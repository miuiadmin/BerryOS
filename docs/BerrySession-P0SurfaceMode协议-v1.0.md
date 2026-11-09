# BerrySession P0 Surface Mode 协议

> 版本：**1.0**｜2026-11-08
>
> 定位：GUI P0 的 UI 形态与 Surface Mode 协议规格。执行序权威 = [GUI框架地基路线-v1.6.md](GUI框架地基路线-v1.6.md)；多形态裁决 = ADR #67 与 [GUI-12-复杂应用与游戏UI形态深研-2026-11.md](../research/GUI-12-复杂应用与游戏UI形态深研-2026-11.md)。本文冻结 `berry-guiproto::surface` 的词表、wire 布局、不变量与 host 测试；P1 已按本协议完成 escape 授权与生产接线，见 [BerrySession-P1SurfaceMode生产接线-v1.0.md](BerrySession-P1SurfaceMode生产接线-v1.0.md)。

## §1 一个契约，三条路径

Surface 合约不按 UI 框架分叉。所有 app 都向 Berry Session 声明同一个 Surface Mode 请求；host 授予同一个合约结构。

| 路径 | role | mode | 典型 input class | IME | a11y sidecar | escape hatch |
|---|---|---|---|---|---|---|
| 普通 BerryUI | App | Retained | Touch / Keyboard | Host 或 Disabled | Tree | 无 |
| 专业应用 | App | Canvas | Touch + Mouse + Keyboard | Host | Hybrid | Canvas |
| 游戏/实时 | App | Immersive | Keyboard + Gamepad / Touch | Host 或 Disabled | Regions | Immersive |

BerryUI 是 Retained 形态的默认工具包，不是特权渲染器。Retained、Canvas、Immersive 都必须携带：

```text
焦点仲裁
帧率意图
owner 死亡清理
a11y 语义旁车
```

Canvas 与 Immersive 只额外获得 escape hatch 授权；它们仍不能直写屏、私接 WM RPC/VMO/stride，或绕过 Berry Room 输入、IME、a11y、帧率与清理链。

## §2 词表

### Role

```text
0 App     普通 app surface；三条 UI 路径共用
1 System  系统壳/系统层 surface；v1 只允许 Retained
```

app 请求不得自报 `System`。系统 surface 由系统宿主创建。

### Mode

```text
0 Retained   BerryUI retained tree / DisplayList
1 Canvas     专业应用文档 CanvasSurface
2 Immersive  游戏/实时应用 immersive surface
```

### Input class

```text
bit0 Touch
bit1 Stylus
bit2 Mouse
bit3 Keyboard
bit4 Gamepad
```

未知位拒绝；请求至少声明一类输入。输入类是语义路由与焦点契约的一部分，不授予 app 直接读设备或解析 input wire 的权利。

### IME mode

```text
0 Disabled  无文本输入，显式声明
1 Host      host composer + composition 旁车
```

`Disabled` 不是绕过 IME：它表示该 surface 没有文本输入面。任何文本编辑都必须走 `Host`。

### a11y sidecar

```text
0 Tree     retained 语义树
1 Regions  canvas/immersive 语义区域表
2 Hybrid   BerryUI 外壳树 + 文档 canvas 区域表
```

不变量：

```text
Retained   → Tree
Canvas     → Regions 或 Hybrid
Immersive  → Regions
```

没有 `None` 值；即使游戏自绘 UI，也必须提供语义区域或显式空区域表。

### 合约位

```text
bit0 Focus    焦点仍由 Berry Session 仲裁
bit1 Rate     帧率意图仍走 present VOTE
bit2 Cleanup  owner 死亡仍清理窗口/surface/输入队列
bit3 A11y     语义 a11y sidecar 必须存在
bit4 Escape   Canvas/Immersive escape hatch 已授权
```

`Retained` 合约位必须恰为 `Focus|Rate|Cleanup|A11y`。`Canvas` 与 `Immersive` 必须恰为上述四位再加 `Escape`。未知位拒绝。

## §3 safe area 与 scale factor

safe area 与 scale 只能由 host 授予，app 请求不能自报。

```text
safe_left/top/right/bottom: u16，物理像素
scale_permille: u16，1000 = 1.0×
```

`scale_permille` 合法域：

```text
500..=4000
```

host 侧还必须以实际显示几何调用：

```rust
surface_mode_info_fits(&info, width, height)
```

不变量：

```text
width > 0
height > 0
safe_left + safe_right < width
safe_top + safe_bottom < height
```

即 safe area 必须留下至少一个可用像素；系统状态栏、打孔屏、圆角和手势区都由这四个 inset 表达，不引入第二套 display metrics。

## §4 WM 域 wire 面

P0 在 WM 域保留带内新增 add-only op：

```text
OP_WM_SURFACE_MODE = 0x67
```

既有 op `0x60..0x66` 与 `VER_WM = 1` 不变。未实现该 optional op 的 v1 服务按既有错误/拒面处理；P0 只冻结协议与 host 策略函数，不宣称生产 gui-wmd 已开放 Canvas/Immersive buffer。

### 请求：8B

```text
off 0  op = 0x67
off 1  role
off 2  mode
off 3  input_classes
off 4  ime_mode
off 5  a11y_mode
off 6  surface_id u16 LE，非 0
```

编码/解析函数：

```rust
encode_wm_surface_mode_req
parse_wm_surface_mode_req
surface_mode_req_valid
surface_mode_req_app_valid
```

### 成功授予：20B

```text
off 0  st = ST_OK
off 1  role
off 2  mode
off 3  input_classes
off 4  ime_mode
off 5  a11y_mode
off 6  contract_flags
off 7  reserved
off 8  scale_permille u16 LE
off 10 safe_left u16 LE
off 12 safe_top u16 LE
off 14 safe_right u16 LE
off 16 safe_bottom u16 LE
off 18 surface_id u16 LE，非 0
```

错误应答沿用全局烘焙线：非 OK 恒 1B `[st]`，不带数据。

编码/解析函数：

```rust
encode_wm_surface_mode_rep
parse_wm_surface_mode_rep
surface_mode_info_valid
surface_mode_info_fits
surface_mode_grant_matches
```

`surface_mode_grant_matches` 要求授予帧逐字段回显请求的 surface id、role、mode、input class、IME 与 a11y；host 只能新增 safe area、scale 与合约位。未授权 Canvas/Immersive 时必须回 1B `ST_DENIED` 或 `ST_UNSUPPORTED`，不得静默改授 Retained。

截断、尾随字节、未知 op、surface_id=0、未知 role/mode/input bit/IME/a11y 值、未知合约位、非法 scale、非法形态组合一律拒绝。一个 WM session 的多窗必须逐 surface 协商，不隐式绑定“当前窗”。

## §5 IME composition 旁车

IME composition 是独立定长旁车帧，不混入 PointerEvent：

```text
IME_COMPOSITION_LEN = 76B
IME_TEXT_MAX = 64B UTF-8
```

布局：

```text
off 0  revision u32 LE，非 0
off 4  phase
off 5  purpose
off 6  cursor_cp u16 LE
off 8  text_len
off 9  reserved = 0
off 10 reserved u16 = 0
off 12 text[text_len]
尾随   全 0
```

phase：

```text
0 Hide
1 Show
2 Update
3 Commit
4 Cancel
```

purpose：

```text
0 Text
1 Multiline
2 Number
3 Password
```

不变量：

```text
text 必须是合法 UTF-8
cursor_cp 是码点偏移，且 <= text 码点数
revision 非 0
Hide 必须空文本、cursor=0
尾部 padding 必须规范为 0
```

`Password` 的额外宿主策略（不落长期语义树、不进日志）由后续 IME 服务刀执行；P0 先冻结 wire 值域与明文不得进入 a11y 长期树的边界。

## §6 a11y semantic sidecar 记录

Tree、Regions、Hybrid 最终都以同一类扁平语义记录进入 sidecar；差异只在生产者：Retained/Hybrid 由 BerryUI 树导出，Canvas/Immersive 由应用或引擎申报。

```text
A11Y_REGION_LEN = 52B
A11Y_LABEL_MAX = 32B UTF-8
```

wire 布局：

```text
off 0  node_id u32 LE，非 0
off 4  parent_id u32 LE（0 = 根；不得等于 node_id）
off 8  x/y/w/h 各 u16 LE，surface-local 逻辑像素
off 16 role
off 17 actions
off 18 state
off 19 label_len
off 20 label[label_len]
尾随   全 0
```

role 与 BerryUI 现有 a11y 表 0..=5 单源对齐，并扩两个专业/游戏值：

```text
0 None
1 Button
2 Text
3 Image
4 Toggle
5 ListItem
6 Document
7 GameRegion
```

actions：

```text
bit0 Focus
bit1 Activate
bit2 Adjust
bit3 TextEdit
```

state：

```text
bit0 Visible
bit1 Focused
bit2 Disabled
bit3 Checked
```

不变量：

```text
node_id 非 0
parent_id != node_id
w/h 非 0
role/action/state/label 值域合法
label 是合法 UTF-8
尾部 padding 全 0
```

编码/解析函数：

```rust
encode_a11y_region
parse_a11y_region
```

空语义面不能用 `None` 逃逸：应用至少申报一个覆盖可用 surface 的根区域（可为 `GameRegion` 或 `Document`，label 可为空）。

## §7 测试证据

新增：

```text
user/berry-guiproto/tests/surface_mode.rs
```

覆盖：

```text
普通/专业/游戏三路径请求与授予 round-trip
请求与授予字节钉板
Retained 无特权、Canvas/Immersive 必须显式 Escape
System 角色禁止 app 自报、禁止 escape hatch
Retained/Canvas/Immersive a11y 组合不变量
safe area 与显示几何对拍
scale 500..4000 边界
未知 role/mode/input bit/IME/a11y/flags 拒面
截断/尾随/错 op/错误 1B 应答
IME UTF-8 与码点 cursor round-trip
IME 非法 UTF-8、越界 cursor、超长、非规范 padding 拒面
a11y Tree/Regions/Hybrid 扁平记录 round-trip
a11y 父子 id、role、action/state、UTF-8 label、padding 拒面
IME/a11y 全帧 golden 与帧帽 const assert
UTF-8 overlong、surrogate、越 U+10FFFF、bare continuation、截断类全拒
parse 侧直接 patch wire 的值域拒面
P0 定长帧逐 cut 截断与 +1 尾随扫频
WM 授予帧保留 off7 前向兼容锚与非 OK 1B 错误形
```

`berry-guiproto` 既有 44 项协议测试与 no_std 构建保持通过，证明 present/input/hdi/既有 WM 面零漂移；P0 新增 8 项测试。GUI8 self-test、双 boot + 自愈、normal 五 Native Shell + F5 BerryUI 真链，以及 smoke-image 138/138 均复跑通过。

## §8 实施边界

- P0 冻结协议与 host 策略，不分配 Canvas/Immersive buffer。
- 该边界为 P0 时点状态；P1 已处理 `0x67` 并保持 `VER_WM=1` / `0x60..=0x66` 帧型零漂。IME、a11y 批量传输与键盘/手柄生产者仍随后续刀。
- a11y sidecar 的批量传输、订阅与树更新策略归后续 a11y 服务刀；P0 冻结单记录 codec、role/action/state 值域与形态不可缺席。
- IME 服务与软键盘 UI 归后续输入/IME 刀；P0 只冻结 composition 帧。
- F5 `uitest` 不需要发送 `0x67`：Retained 默认合约由 Berry Room 现有 WM/frame/input 链承载。
- F6 Settings 迁移仍走普通 Retained 路径，不因 Canvas/Immersive 词表而扩 UI 范围。

## §9 预算与镜像

P0 是 host 协议刀，未用协议面经 LTO 裁剪，rootfs 与 F5 相同：

```text
rootfs   = 33,553,480B
单卷 cap = 33,554,432B
单卷余量 = 952B
总量 cap = 134,217,728B
UITEST.bap = 64,416B
```

P0 镜像 SHA-256：

```text
948d83c78a03834123b15215c49bfcd449c99d3eb00072f05c41535b5f2fd19d
```
