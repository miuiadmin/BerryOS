# BerryFilesContacts F7 迁移

> 版本：**1.0**｜2026-11-08
>
> 定位：GUI F7 的 Files 与 Contacts 迁移规格。执行序权威 = [GUI框架地基路线-v1.19.md](GUI框架地基路线-v1.19.md)；组件与主题见 [BerryUI-F4组件第一版-v1.1.md](BerryUI-F4组件第一版-v1.1.md)，宿主范式见 [BerryUI-F5系统验收-v1.0.md](BerryUI-F5系统验收-v1.0.md)，Settings 先例见 [BerrySettings-F6迁移-v1.0.md](BerrySettings-F6迁移-v1.0.md)。容量口径遵循 [容量工程前置-A1设计草案-v1.0.md](容量工程前置-A1设计草案-v1.0.md)：单卷 ≤32MiB、多卷总量 ≤128MiB。

## §1 迁移结果

Files 与 Contacts 均从手写 Native Shell 迁到：

```text
Berry Room：APP env / WM session / 窗口 / frame VMO / 输入 / submit / destroy
BerryUI：Scaffold / AppBar / ScrollView / Card / Button / Text / Row / Column
Berry Design：Theme + ThemeMetrics + InteractionState
系统字形：SYSTEM_FONT_ATLAS → FontSource
数据链：berry-fsclient 会话机
```

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

数据 RPC 归 `berry-fsclient`，UI RPC 归 Berry Room；app 只持有页面状态与数据语义。

## §2 Files 页面与数据链

页面：

```text
Scaffold + AppBar “Files”
ScrollView(surface)
Card(surface)
List Button：Back + 根目录条目
Detail：文件名 Title + Size Body + 内容预览 Body + Back Button
```

业务状态：

```text
entries[8]
count
selected
mode: list | detail
frame_seq
```

真实数据链保留：

```text
READDIR root
STAT HELLO.TXT
GET HELLO.TXT chunks
文件 size / bytes 校验
```

运行期行为：

```text
点击 HELLO.TXT Button
→ STAT + GET 真链
→ FILESOPEN / FILESSTAT / FILESREAD
→ BerryUI detail damage
→ DisplayList
→ Room submit
```

详情返回列表和列表退出均由 Button Msg 驱动，不再按固定 x/y 区域手写命中。

## §3 Contacts 页面与数据链

页面：

```text
Scaffold + AppBar “Contacts”
ScrollView(surface)
Card(surface)
List Button：Back + 4 位联系人
Detail：姓名 Title + 号码 Body + Favorite Button + Back Button
```

业务状态：

```text
contacts[4]
detail index
favorite[4]
mode: list | detail
frame_seq
```

数据链保留：

```text
READDIR root
STAT CONTACTS.TXT
GET CONTACTS.TXT chunks
解析 4 条 name,number
```

无数据盘拓扑下仍使用编译内 4 条同构 fallback，不伪装远程数据。

运行期行为：

```text
点击 Berry Wang
→ CONTACTSDETAIL i=1
→ Favorite Button Click
→ app 翻转 favorite 状态
→ Selected 视觉重绘
→ CONTACTSFAV n=1
→ 返回列表
→ 列表 Back 退出
```

收藏状态归 app；Button 只映射 BerryUI Msg 与 InteractionState。

## §4 GUI8 真链验收

GUI8 normal 对两 app 均改为 layout-driven：

```text
解析 FILESLAYOUT / CONTACTSLAYOUT 的真实 Button bounds
计算中心点
QMP 注入 owner-local 点击
```

实测 Files 布局：

```text
list Back  = (12,72,1056,60)
HELLO Button = (12,204,1056,60)，点击 (540,234)
detail Back = (12,234,1056,60)
```

实测 Contacts 布局：

```text
list Back = (12,72,1056,60)
Berry Wang Button = (12,204,1056,60)，点击 (540,234)
Favorite Button = (12,180,1056,60)，点击 (540,210)
detail Back = (12,246,1056,60)
```

精确数据与状态：

```text
FILESSTAT ok size=28
FILESREAD ok n=28
CONTACTSDETAIL ok i=1
CONTACTSFAV ok n=1
```

精确帧与布局：

```text
Files：mode0 layout×3、mode1 layout×1、RENDER/FRAME×4
Contacts：mode0 layout×3、mode1 layout×2、RENDER/FRAME×5
```

截图像素锚：

```text
Files AppBar primary = (0,116,224)
Files surface        = (255,255,255)
Contacts AppBar      = (0,116,224)
Favorite Selected    = (214,228,255)
```

两 app 均保留系统 A8 反马赛克色数锚：

```text
box = (20,90,900,420)
minimum colors = 128
```

GUI8 self-test 假串同步覆盖 layout/render/hit/detail/favorite/back/destroy/exit；缺数据、缺帧、缺重绘、顺序倒置均 fail closed。

## §5 并发生命周期修正

F7 并发 boot probe 暴露一个真实生命周期缺陷：旧断言要求 WM 全局表为空。并发下其它 app 的窗口合法存在，Files/Photos 可能在 Contacts 尚未销毁时先完成 destroy。

修正为：

```text
Berry Room destroy 后只要求本 window_id 从权威表消失
旧 Photos destroy 同律
其它窗口可存在
```

这不放宽 owner 清理：自己的窗口、surface、输入队列和 frame 资源仍必须消失。

## §6 预算

F7 最终 rootfs：

```text
总量       = 33,679,824B
rootfs0    = 33,554,432B（恰单卷帽）
rootfs1    = 125,392B
总量 cap   = 134,217,728B
总量余量   = 100,537,904B
```

迁移包体：

```text
FILES    27,912B → 69,768B，增量 41,856B
CONTACTS 31,308B → 73,236B，增量 41,928B
stack    两 app 均 64 pages
```

其它 GUI 包体：

```text
BROWSER.bap  = 31,804B
SETTINGS.bap = 69,408B
PHOTOS.bap   = 27,768B
UITEST.bap   = 64,768B
MMS.bap      = 32,584B
```

Berry Room per-window destroy 逻辑使所有 BerryUI app 包体小幅重排；系统 A8 字表仍未复制进 app。

## §7 回归证据

已通过：

```text
Files no_std build
Contacts no_std build
Berry Room no_std check
Photos no_std check
BerryUI 默认/S7/no_std
just test-host
just abi-check
cargo deny licenses
GUI8 self-test
GUI8 normal-boot
GUI8 dual boot + layout self-heal
smoke-image 138/138
```

最终镜像哈希与全量回归清单见 §8 与 [阶段沿革.md](阶段沿革.md)。

## §8 最终镜像

```text
SHA-256 = c85bd31db67a262abcc098617906464946a093fec2cc31cf2d4da99841e4ff58
```

若最终源码清理改变字节，以最终提交时镜像哈希为准。

## §9 未完成边界

- F7 时点 Browser/Photos 仍是基线；F8 已迁移并见 [BerryBrowserPhotos-F8迁移-v1.0.md](BerryBrowserPhotos-F8迁移-v1.0.md)。
- Files 仍只读；无写、删、搜索、权限管理或持久化。
- Contacts 仍只读；无拨号、短信、编辑或系统联系人服务。
- F7 时点嵌套 clipping 归后续刀；C1 已落地结构性 Clip，两 app 继续经 ScrollView 自动获得裁剪。
- F7 时点 CanvasSurface / immersive buffer 未开放；P1 已接线，但两 app 仍走 Retained 路径。
- `berry-nshfont` 在 F7 时点仍被 Browser/Photos 引用；F8 后无 GUI app 消费者，F9 已删除。
