# BerryBrowserPhotos F8 迁移

> 版本：**1.0**｜2026-11-08
>
> 定位：GUI F8 的 Browser 与 Photos 迁移规格。执行序权威 = [GUI框架地基路线-v1.15.md](GUI框架地基路线-v1.15.md)；组件与主题见 [BerryUI-F4组件第一版-v1.1.md](BerryUI-F4组件第一版-v1.1.md)，宿主范式见 [BerryUI-F5系统验收-v1.0.md](BerryUI-F5系统验收-v1.0.md)，F7 先例见 [BerryFilesContacts-F7迁移-v1.0.md](BerryFilesContacts-F7迁移-v1.0.md)。容量口径遵循 [容量工程前置-A1设计草案-v1.0.md](容量工程前置-A1设计草案-v1.0.md)：单卷 ≤32MiB、多卷总量 ≤128MiB。

## §1 迁移结果

Browser 与 Photos 均从手写 Native Shell 迁到：

```text
Berry Room：APP env / WM session / 窗口 / frame VMO / 输入 / submit / destroy
BerryUI：Scaffold / AppBar / ScrollView / Card / Button / Text / IconSource
Berry Design：Theme + ThemeMetrics + InteractionState
系统字形：SYSTEM_FONT_ATLAS → FontSource
```

Photos 的数据面继续使用：

```text
berry-fsclient 会话机
```

Browser 不持 fs 能力；它仍是诚实本地壳，不新增网络、HTML 解析或 JavaScript 引擎。

两 app 源码不再出现：

```text
berry_channel_*
berry_vmar_*
berry_vmo_*
WM RPC encode/parse
frame stride
frame 指针
put_px / fill / round_rect / circle / glyph 像素循环
berry-nshfont
berry-sys 直接依赖
berry-guiproto 直接依赖
```

至此 Settings、Files、Contacts、Browser、Photos、UITEST 六个 GUI app 全部走 BerryUI 正常路径。

## §2 Browser 页面与真实链

页面：

```text
Scaffold + AppBar “Browser”
ScrollView(surface)
Card(surface)
Back Button
Title：berry://native-shell
Body：Native shell ready
Body：No web engine in v1
```

业务状态：

```text
frame_seq
```

诚实边界不变：

```text
无网络
无 HTML 引擎
无 JavaScript
不声称 web 能力
```

运行期行为：

```text
桌面真实 APP_SPAWN
→ BerryUI 页面
→ DisplayList
→ Room submit
→ 点击 Back Button
→ BROWSERHIT node=back
→ BROWSERBACK
→ BROWSERDESTROY / BROWSEREXIT
```

Browser 输入走 Berry Room owner-local session；没有手写屏幕区域命中。

## §3 Photos 页面与真实数据链

页面：

```text
Grid：Scaffold + AppBar “Photos” + Card + Back + 文件名 + 图像 + View Button
View：Scaffold + Card + Back + 文件名/尺寸 + 大图
```

业务状态：

```text
mode: grid | view
frame_seq
```

真实数据链保留：

```text
READDIR root
STAT PATTERN.BIN
GET PATTERN.BIN chunks
5000B 逐字节公式校验
PHOTOSDATA ok n=5000
```

无数据盘拓扑使用同一确定性公式样本：

```text
PHOTOSLOCAL ok n=5000
```

图像链：

```text
PATTERN.BIN 5000B 字节校验
→ PatternSource IconSource
→ 50×100 ARGB 源
→ BerryUI SmoothArgb DisplayList
→ Berry Room 光栅/present
```

app 只声明图像源与页面状态；像素循环、stride 和 frame VMO 归 Berry Room / BerryGFX。

运行期行为：

```text
点击 View Button
→ PHOTOSOPEN
→ 查看页 layout/render/frame
→ PHOTOSVIEW

点击 Back
→ 返回网格 layout/render/frame
→ PHOTOSLIST n=1

再次点击 Back
→ PHOTOSHIT node=back
→ PHOTOSBACK
→ PHOTOSDESTROY / PHOTSEXIT
```

## §4 GUI8 真链验收

GUI8 normal 对两 app 均改为 layout-driven：

```text
解析 BROWSERLAYOUT / PHOTOSLAYOUT 的真实 Button bounds
计算中心点
QMP 注入 owner-local 点击
```

实测 Browser 布局：

```text
Back = (12,72,1056,60)，点击 (540,102)
```

实测 Photos 布局：

```text
grid Back  = (12,72,1056,60)
grid image = (12,192,360,720)
View Button = (12,918,1056,60)，点击 (540,948)

view Back  = (12,72,1056,60)
view image = (12,192,540,1080)
```

精确断言：

```text
Browser：layout/render/FRAME ×1、HIT back ×1、BACK ×1
Photos：mode0 layout ×2、mode1 layout ×1、RENDER/FRAME ×3
Photos：OPEN ×1、VIEW ×1、HIT back ×1、BACK ×1
```

截图像素锚：

```text
Browser AppBar = (0,116,224)
Browser surface = (255,255,255)
Photos AppBar = (0,116,224)
Photos PATTERN 像素 = (148,148,148)
```

Browser 保留系统 A8 色数锚：

```text
box = (20,70,900,250)
minimum colors = 128
```

Photos 图像灰度层色数锚：

```text
box = (20,180,900,1300)
minimum colors = 128
```

GUI8 self-test 假串同步覆盖 layout/render/hit/view/back/destroy/exit；缺进程、缺 5000B 数据、缺帧、缺布局或顺序倒置均 fail closed。

## §5 GUI 系统真缺陷修正

### ReleaseWait RETURN-ALL 全量清账

guifbd 的 `ReleaseWait` 明确规定：

```text
空表 count=0 是合法幂等应答
```

Berry Room 旧 submit 只接受 count=1，把合法空账误判为 `release reply`，造成 Browser 间歇失败。F8 修正为：

```text
count 与随行 handle 恰等（可为 0..=2）→ 全量 close
count/handle 失配或零句柄值 → ReleaseReply loud 拒
```

Berry Launcher 同步接受 0..=2 的 RETURN-ALL 形态并全量关闭。并发下另一个
session 先清账可产生 0，先提交可产生 2；两者都是合法协议形态。

### GUI boot probe 回收帽

F8 后 GUI boot probe 会真实执行：

```text
fs FAT 链
BerryUI 布局
DisplayList 光栅
frame present
destroy
```

TCG 下 Contacts/Photos 可超过旧 30s child wait 帽，导致 appd `APP ok n=9` 缺席。`berry-exec` 的 GUI 回收等待帽调整为：

```text
240s
```

退出真码 0 谓词不变；挂死仍 fail closed。GUI8 Boot B 预算同步为 420s：真实慢链可收口，240s 内未退出的挂死仍先由 `eco: app` loud 暴露。

### Contacts/Photos 页面容量精简

Contacts 与 Photos 列表/查看页实际节点数均远小于 96。F8 将两 app UI arena 收敛到 32，降低 TCG 交互/退出栈压，不改变页面语义与 marker。

## §6 预算

F8 当前 rootfs：

```text
总量       = 33,780,848B
rootfs0    = 33,554,432B（恰单卷帽）
rootfs1    = 226,416B
总量 cap   = 134,217,728B
总量余量   = 100,436,880B
```

迁移包体：

```text
BROWSER 31,804B → 71,412B，增量 39,608B
PHOTOS  27,768B → 89,440B，增量 61,672B
stack   两 app 均 64 pages
```

其它 GUI 包体：

```text
SETTINGS.bap = 69,456B
FILES.bap    = 69,816B
CONTACTS.bap = 72,836B
UITEST.bap   = 64,816B
MMS.bap      = 32,584B
```

Berry Room ReleaseWait 修正使 BerryUI app 包体发生 16B 量级重排；系统 A8 字表仍未复制进 app。

## §7 回归证据

已通过：

```text
Browser no_std build
Photos no_std build
Contacts no_std build
Berry Room no_std check
berry-exec no_std check
Berry Launcher aarch64-musl build
BerryUI 默认/S7/no_std/guest
just test-host
just abi-check
cargo deny licenses
GUI8 self-test
GUI8 normal-boot
GUI8 dual boot + layout self-heal
smoke-image 138/138
```

## §8 当前镜像

当前构建哈希：

```text
SHA-256 = 40c87b4bdd232e1ea1f27570cda91b2e58ae107bc365624e70064c763b3e9fb3
```

若后续源码清理改变字节，以最终提交时镜像哈希为准。

## §9 未完成边界

- Browser 仍无网络、HTML、JavaScript、多标签、历史或书签。
- Photos 仍无相机、编辑、相册管理、云图或持久收藏。
- Files/Contacts 仍保持 F7 的只读边界。
- F8 时点嵌套 clipping 归后续刀；C1 已落地结构性 Clip。
- F8 时点 CanvasSurface / immersive buffer 未开放；P1 已接线，但两 app 仍走 Retained 路径。
- `berry-nshfont` 已无 GUI app 消费者；F9 已删除并落地 `gui-boundary-guard`。
