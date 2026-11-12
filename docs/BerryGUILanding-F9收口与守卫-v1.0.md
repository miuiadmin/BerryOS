# BerryGUILanding F9 收口与守卫

> 版本：**1.0**｜2026-11-08
>
> 定位：GUI F9 的迁移收口、静态守卫与 SDK 边界规格。执行序权威 = [GUI框架地基路线-v1.19.md](GUI框架地基路线-v1.19.md)；F8 全量迁移结果见 [BerryBrowserPhotos-F8迁移-v1.0.md](BerryBrowserPhotos-F8迁移-v1.0.md)。容量口径遵循 [容量工程前置-A1设计草案-v1.0.md](容量工程前置-A1设计草案-v1.0.md)。

## §1 收口结果

F8 后六个普通 GUI app 已全部走 BerryUI：

```text
Settings
Files
Contacts
Browser
Photos
UITEST
```

F9 删除迁移期临时 crate：

```text
user/berry-nshfont/
```

职责最终归属：

```text
字形 VMO 映射 / WM / frame / input / destroy → berry-room
像素与几何光栅算法                      → berry-gfx
文本消费 / 控件 / 主题 / DisplayList     → berry-ui
授权 fs 数据 RPC                        → berry-fsclient
```

删除 `berry-nshfont` 后，仓库中不存在第三套 UI 客户端或第三套字体/基元路径。历史文档中的 F2/F3/F6/F7/F8 时点记录保留；现行指南只指向 Berry Room + BerryUI。

同时清理未消费的迁移前截图产物：

```text
out/berry-*-native*.png
```

这些是 ignored 调试产物，不属于镜像或测试证据；保留会误导后续像素锚对照。

## §2 静态守卫

新增工具：

```text
tools/gui_guard.py
```

入口：

```text
just gui-boundary-guard
```

CI 独立步骤：

```text
gui-boundary-guard（GUI F9 普通 app 禁令）
```

并挂入：

```text
just build
just check
just build-release
.github/workflows/ci.yml
```

### 扫描范围

守卫扫描：

```text
user/bins/*/manifest.toml
```

当 app 同时满足：

```text
app.type = "app"
capabilities.wm = true
surface_mode = retained / 未声明
```

即视为普通 Retained GUI app。

当前必须发现六个已知 app：

```text
SETTINGS
FILES
CONTACTS
BROWSER
PHOTOS
UITEST
```

扫描数量少于六或漏掉已知 app 均失败，防止 manifest 结构漂移导致守卫静默空转。

### 直接依赖门

普通 GUI app 必须在顶层、target-specific、build 与 dev dependency 表中保持同一直接依赖纪律，并直接依赖：

```text
berry-room
berry-ui(features = ["guest-font-vmo"])
```

允许：

```text
berry-gfx      # DisplayList 构造常量 / encoder 类型
berry-fsclient # 仅 manifest 授 fs 能力时
```

其它任何 `berry-*` 直接依赖表项均拒绝；嵌套 target 表会被递归展开。当前明确禁用：

```text
berry-sys
berry-guiproto
berry-nshfont
berry-fsproto
```

数据边界同时双向检查：

```text
manifest 有 fs → 必须用 berry-fsclient
manifest 无 fs → 不得用 berry-fsclient
```

### 源码禁令

守卫先去掉 Rust 注释，再扫描当代码 token。注释中的历史说明不误报；字符串中的禁词不豁免。

禁令：

```text
berry_channel_*
berry_vmar_*
berry_vmo_*
berry_handle_*
berry_process_*
berry_object_wait_*
berry_clock_*

use berry_sys
use berry_guiproto
use berry_nshfont

encode_wm_ / parse_wm_
encode_input_ / parse_input_
encode_present_ / parse_present_
gp::

FRAME_VA
FONT_VA
stride

put_px
blend_px
round_rect / fill_round_rect
circle / fill_circle
shadow / draw_shadow
berry_gfx::surface

unsafe
alloc::
```

`berry_gfx` 对普通 app 只允许 DisplayList 构造面，不允许直接调用 surface 光栅函数。

### 临时件禁令

守卫要求：

```text
user/berry-nshfont 不存在
```

未来若重新出现该目录，即使没有任何 app 引用，也会失败。

manifest 中出现 `surface_mode != "retained"` 时，F9 守卫同样 fail closed；在 P0 授权 renderer allowlist 进入本工具前，CanvasSurface / immersive 不存在静默豁免面。

## §3 守卫自检

`gui_guard.py --self-test` 构造隔离临时树，覆盖：

```text
正样本：
    六个 app 全部合法
    注释中的历史 token 不误报

负样本：
    直接 channel syscall
    vmar 映射
    stride
    put_px
    unsafe
    berry-nshfont 直接依赖
    berry-nshfont crate 回潮
    target-specific berry-sys 依赖
    src 嵌套模块中的像素基元
    未授权 immersive renderer
    缺 guest-font-vmo
    授 fs 但缺 berry-fsclient
    syscall 函数指针
    块注释绕行
    r# / r## raw string 绕行
    build 依赖冒充运行时 berry-room
    普通 app 直调 DisplayList Push/PopClip
```

任一负样本未失败即 self-test FAIL。正样本同时覆盖嵌套块注释、字符串中的 `//` 与 char 引号，证明注释/字面量状态机不误报。

当前结果：

```text
C1 起正样本 + 18 类负样本 fail closed 全绿；P1 起 21 类
```

## §4 P0 授权 renderer 边界

P0 已冻结 Surface Mode 协议；F9 时点 CanvasSurface / immersive buffer 尚未开放，P1 已按 allowlist 接线。

因此 F9 守卫当前没有授权 renderer allowlist。未来落地专业 Canvas 或游戏 immersive app 时，必须同时提供：

```text
manifest surface role/mode
授权 renderer allowlist
Berry Room surface/input/IME/a11y 能力面
静态守卫分流规则
协议与生命周期测试
```

不得通过删除 `gui-boundary-guard` 或把 renderer 伪装成普通 Retained app 绕行。

专业/游戏 renderer 的像素算法只能写入授权 CanvasSurface / immersive surface；外壳仍走 BerryUI，语义旁车不得降级。

## §5 文档与 SDK 收口

同步更新：

```text
docs/应用开发指南.md
docs/规则/14-GUI与UI框架.md
docs/规则/07-测试与冒烟.md
docs/命令速览.md
tools/gui8-smoke.py
user/bins/settings/README.md
user/bins/svcmgr/src/main.rs
```

现行文档不再说“迁移期旧 Native Shell 仍持有 berry-nshfont”。F2/F3/F6/F7/F8 规格与阶段沿革中的历史时点记录保留为决策链。

SDK 当前唯一普通 GUI 路径：

```text
RoomConfig
AppRoom::connect
AppRoom::open_window
FontSource + SYSTEM_FONT_ATLAS
Ui<N>
WidgetContext
BerryUI widgets
AppRoom::render_display_list
AppRoom::submit
AppRoom::poll_input
AppRoom::destroy
```

普通 app 禁止重建：

```text
WM RPC client
frame VMO client
stride 计算
像素/字形客户端
```

## §6 预算

`berry-nshfont` 在 F8 后已无消费者，删除不改变任何 `.bap` 或 rootfs 字节。

F9 rootfs 保持：

```text
总量       = 33,780,848B
rootfs0    = 33,554,432B（恰单卷帽）
rootfs1    = 226,416B
总量 cap   = 134,217,728B
总量余量   = 100,436,880B
```

GUI 包体保持：

```text
BROWSER.bap  = 71,412B
SETTINGS.bap = 69,456B
FILES.bap    = 69,816B
PHOTOS.bap   = 89,440B
CONTACTS.bap = 72,836B
UITEST.bap   = 64,816B
```

A1 口径：

```text
单卷 ≤ 33,554,432B
总量 ≤ 134,217,728B
```

## §7 回归证据

已通过：

```text
gui-boundary-guard --self-test（F9 时点 17 类；C1 起 18 类；P1 起 21 类）
gui-boundary-guard
just build
just check
just build-user
Settings/Files/Contacts/Browser/Photos/UITEST no_std
Berry Room no_std
berry-exec no_std
Berry Launcher aarch64-musl
BerryUI 默认/S7/no_std/guest
just test-host
just abi-check
CI 同口径全 workspace cargo deny licenses
GUI8 self-test
GUI8 normal-boot
GUI8 dual boot + layout self-heal
smoke-image 138/138
docs_guard
Curia strict
writable_globals
git diff --check
```

## §8 最终镜像

删除无消费者 crate 不改变 guest 字节，最终镜像与 F8 提交后构建同哈希：

```text
SHA-256 = 40c87b4bdd232e1ea1f27570cda91b2e58ae107bc365624e70064c763b3e9fb3
```

## §9 未完成边界

- F9 时点 CanvasSurface / immersive buffer 未开放；P1 已完成授权接线，但 F9 历史守卫边界不变。
- F9 时点嵌套 clipping 仍归后续刀；C1 已落地并见 [BerryUIClipping-C1嵌套视口-v1.0.md](BerryUIClipping-C1嵌套视口-v1.0.md)。
- Browser 仍无网络 / HTML / JavaScript。
- Photos 仍无相机 / 编辑 / 云图。
- Files / Contacts 仍为只读。
- 专业与游戏 renderer 需要后续 P0 接线刀，不得从普通 app 路径私开。
