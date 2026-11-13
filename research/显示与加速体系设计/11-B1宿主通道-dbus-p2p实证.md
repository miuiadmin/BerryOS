# 11 号：B1 宿主通道——D-Bus p2p 在 macOS 全链实证与三通道对照

> 实证日期：2026-11-13。服务对象：02 号 §2 β-1（`-display dbus,p2p=on` + Listener/Unix.Map + 宿主 wgpu/Metal 终态）——本文即任务书 11 号（β-1 Mac 宿主通道实证），闭 05 号 §0「D-Bus/memfd 面未覆盖」挂账与 02 号 §7 两条 [中] 风险。探针环境：Apple M3 Pro / 18GB / QEMU 11.0.3 Homebrew / GLib 2.90.1（Homebrew，`/opt/homebrew/opt/glib`）——与 01/02/05/07/09 号同机同版本，本机无 dbus-daemon/dbus-send/busctl（`which` 三查零命中）。方法：QEMU v11.0.3 源码逐文件定谳（GitLab tag 直取，落 `/tmp/b11/qemu-src/`）+ 双客户端实跑探针（手写 Python 线协议与 C/GDBus 客户端，产物 `/tmp/b11/`）+ 官方上游测试 `tests/qtest/dbus-display-test.c` 逐行对照——零仓内改动（本文与 README 索引行外）。结论确定度沿用 02 号口径：**[高]** = 一手源码/本机实证；推断显式标注。

---

## §0 结论速览

1. **β-1 在 macOS 定谳可行——全链实收**：`-display dbus,p2p=on` 在本机（无 dbus-daemon）实跑，C/GDBus 客户端经 QMP `getfd`+`add_client(@dbus-display)` 建立 p2p 主连接，`Console_0` 对象属性实读（Label=gpu0、Type=Graphic、1280×800、Keyboard/Mouse/MultiTouch 接口在席），`RegisterListener` 带 fd 应答 OK，**`ScanoutMap` 实收共享图 fd（mmap 读通）+ `UpdateMap` damage 矩形 + `Mouse.SetAbsPosition` 输入反走发送**——02 号 β-1 设计的每个环节在 mac 都有活体证据（§2）。02 号 §6 的「D-Bus Map 不可用 → 降 β-0」降档线**不触发**。
2. **memfd 之谜源码定谳**：macOS 无 memfd，QEMU 的 `qemu_memfd_alloc` 非 Linux 分支走 `mkstemp` 匿名临时文件 + `unlink` + `ftruncate` + `mmap MAP_SHARED`（util/memfd.c :119-134），fd 经 D-Bus unix fd passing 传出后壳进程 mmap 同一文件——**不是 POSIX shm_open**（`qemu_shm_alloc` 在 oslib-posix.c :985-1035 是 memory-backend 用的另一函数，不在此链）。活体证据：lsof 见 QEMU 持有 `$TMPDIR/memfd-izFQ5H` 4096000 字节（= stride 5120 × 800，与 ScanoutMap 几何互证）[高·实测]。
3. **p2p 免 daemon 定谳**：p2p=on 时 QEMU 不连不监听任何总线（ui/dbus.c :203-205 只置全局指针等 `add_client` 喂 fd）；需外部 bus 的是 addr 模式（:206-213 以 `AUTHENTICATION_CLIENT` 连给定地址）。addr 与 p2p 互斥（:531-532 拒答）。喂 fd 的正路 = QMP socket 上 `getfd`（SCM_RIGHTS）+ `add_client` 协议名 **`@dbus-display`**（monitor/qmp-cmds.c :119-163 协议表）——本机 QMP unix socket 实证可用。
4. **数据面三级路径源码定谳**（ui/dbus-listener.c :822-853）：宿主实现 `Listener.Unix.Map` 且 surface 有 share_handle 时，`UpdateMap(x,y,w,h)` 纯 16 字节矩形零像素搬运；否则退 `Scanout`（全帧字节）或 `Update`（子矩形字节，pixman 线性化拷贝 :763-803）。virtio-gpu 2D 资源建为 shareable pixman（hw/display/virtio-gpu.c :300-313）且 `do_set_scanout` 把 share_handle 接进 console surface（:689-690）；11.0.3 连默认 display surface 也是 shareable（ui/console.c :492-499）——共享图是设备原生能力，02 号 §2 预判在 mac 兑现。
5. **三通道对照定谳**：D-Bus p2p（零编码零拷贝，宿主 CPU 面 = GPU 上传仅 damage 矩形）＞ VNC raw 直收（增量矩形已实证〔09 号 76+ 帧/30s〕，但每 update 壳侧付 RFB 解析 + 逐像素标量转换 + 全帧 Frame clone——vnc.rs :506/:529-535）＞ cocoa 自带窗口（owner 已裁出局；02 号实证无 GL）。**Mac 最优推荐：β-0 VNC 增量先行（已在途）+ β-1 D-Bus p2p 直达终态**——β-1 非遥远升级，是本机已实证的下一步（§4）。
6. **实现注意（β-1 壳半的新增情报）**：D-Bus 线协议手写子集可行但有尖角——头部字段数组的长度域在报文偏移 12、内容 8 对齐（jeepney 参考实现对拍定谳），fd 传递必须 `NEGOTIATE_UNIX_FD` 且用带 fd list 的调用变体；GLib/macOS 的 SCM_RIGHTS 收发微基准全通（原生与 GLib 双向，`/tmp/b11/sp_fd_test3`）。上游 `dbus-display-test.c` 是壳实现的最佳参照骨架（§5）。

## §1 问题与方法

任务书三问：① D-Bus on mac 可行性（p2p 免 daemon？unix socket 形态？Listener/Unix.Map 共享图在 mac 的实际形态？memfd→mac 替代？）；② 替代通道对照（D-Bus p2p / VNC raw 直收 / cocoa）；③ Mac 最优推荐（壳窗口 + wgpu/Metal 上屏最短路径）。

来源三路：QEMU v11.0.3 源码（GitLab tag `v11.0.3` 逐文件取，含 ui/dbus.c、ui/dbus-listener.c、ui/dbus-console.c、ui/qemu-pixman.c、util/memfd.c、util/oslib-posix.c、hw/display/virtio-gpu.c、ui/console.c、monitor/qmp-cmds.c、ui/dbus-display1.xml、tests/qtest/dbus-display-test.c）；本机实跑（双客户端探针 + lsof 活体 + GLib 微基准）；仓内既有实证（02 号 β-1 设计、05 号 S1 挂账、09 号 β-0 增量与 console 绑定序、07 号基线）。

QEMU 拓扑（探针 A 形，05 号 §1.2 同族）：`-machine virt -accel hvf -cpu host -m 256M -display dbus,p2p=on -qmp unix:...{,server,nowait} + AAVMF pflash + -device virtio-gpu-device -global virtio-mmio.force-legacy=false`——guest 侧由 AAVMF 的 VirtioGpuDxe 驱动 GPU（09 号 §4 同帧源），guifbd 未参与（通道实证不需要 guest 半）。

## §2 D-Bus on mac 可行性——实证与源码定谳

### 2.1 免 daemon 与喂 fd 机制

p2p 模式源码链 [高]：`-display dbus,p2p=on` → `dbus_init`（ui/dbus.c :526-544）以 `object_new_with_props` 建 DBusDisplay → UserCreatable `complete`（:188-263）在 p2p 分支**只置 `dbus_display = dd` 等待**（:203-205），不建任何 socket；QMP `add_client` 协议 `@dbus-display`（monitor/qmp-cmds.c :133）→ `monitor_get_fd` 取 `getfd` 存的 fd（:138）→ `fd_is_socket` 校验（:143）→ `dbus_display_add_client`（ui/dbus.c :315-372）：fd 包成 GSocket + `g_dbus_connection_new(AUTHENTICATION_SERVER | DELAY_MESSAGE_PROCESSING)` 异步认证，完成后 `add_client_ready`（:283-312）挂对象管理器（`set_connection`）并开消息处理。

实测读数（C 客户端 `gdbus_client_test`，`/tmp/b11/`）：三 QMP 命令全 `{"return": {}}`；SASL `AUTH EXTERNAL`（hex uid）被 QEMU 的 GDBus 接受（`OK <guid>`），`NEGOTIATE_UNIX_FD` → `AGREE_UNIX_FD`（Python 手写客户端同读数）——**macOS 上 EXTERNAL 认证可用**（GLib 凭据面在本机工作），SCM_RIGHTS 在 mac 的 AF_UNIX 上原生可用（QMP getfd 即证）。主连接对象面：`/org/qemu/Display1/VM` 的 `GetAll` 返回 `Name=QEMU 11.0.3, ConsoleIDs=[0]`；`/org/qemu/Display1/Console_0` 返回 Label=gpu0、Head=0、Type=Graphic、1280×800、Interfaces=[Keyboard, Mouse, MultiTouch]。

### 2.2 Listener 注册与共享图收帧

契约（ui/dbus-display1.xml）：`Console.RegisterListener(h listener)` 收一个 Unix socket fd；Listener 对象以 `Interfaces`(as) 只读属性自报扩展接口（XML 注释明言「Unlike the standard D-Bus Introspectable interface, querying this property does not require parsing XML」）；`Listener.Unix.Map.ScanoutMap(h handle, u offset, u width, u height, u stride, u pixman_format)` 带共享图 fd；`UpdateMap(i x, i y, i width, i height)` 纯 damage 矩形。

源码链 [高]：`dbus_console_register_listener`（ui/dbus-console.c :253-336）从 fd list 取 fd → 先回 method return（:301-306）→ 在该 fd 上建第二个 GDBus 连接（AUTHENTICATION_SERVER，:308-318）→ `dbus_display_listener_new`（:324）→ `setup_shared_map`（ui/dbus-listener.c :1119-1162）查 Listener `Interfaces` 含 `org.qemu.Display1.Listener.Unix.Map` 则 `can_share_map = true`。发帧三级路径（:822-853 `dbus_gfx_update`）：① map 可共享 → `ScanoutMap`（fd+offset+几何）一次 + 此后每 damage 一条 `UpdateMap` 16 字节（:839-844）；② 全帧更新且无 map → `Scanout` 全帧字节；③ 部分更新且无 map → `Update` 子矩形字节（:763-803 pixman 线性化拷贝）。

实测读数（C 探针 `probe_full`，两轮，`/tmp/b11/probe_full.log` 与 `probe_full2.log`）[高·实测]：

```text
[0.008] main p2p connection established
[0.008] Console_0 props: Label=gpu0 Type=Graphic 1280x800 Interfaces=[Keyboard,Mouse,MultiTouch]
[0.009] RegisterListener reply OK / listener p2p connection established
[0.009] listener objects registered (Listener + Unix.Map)
[0.010] *** ScanoutMap fd=14 off=0 1280x800 stride=5120 pixman=0x20020888
[0.011]     mmap ok（first 64 rows nonzero=0，固件黑底静止相）
[0.011] *** UpdateMap #1: 0,0 1280x800
[9.98]  input: Mouse.SetAbsPosition(300,300) sent
SUMMARY: ScanoutMap=1 UpdateMap=1 MouseSet 经由 Mouse 接口发送成功
```

pixman=0x20020888 = PIXMAN_x8r8g8b8（BGRX 字节序）——与现役 ramfb fmt=1 BGRX 同族，02 号 §3.1「格式直配零像素转换」在 D-Bus 面同样成立。40 秒窗仅 1 次 UpdateMap：UEFI boot manager 静止后无新 damage（诚实记录；damage 流的活体在 09 号 VNC 面已有 76+ 帧/30s 实证，同一 QEMU console 更新源）。

### 2.3 memfd→macOS 后备（任务书预问的定谳）

调用链 [高]：virtio-gpu `resource_create_2d` → `qemu_pixman_image_new_shareable`（hw/display/virtio-gpu.c :300-313）→ `qemu_pixman_shareable_alloc`（ui/qemu-pixman.c :274-284）→ `qemu_memfd_alloc`（util/memfd.c :108-148）：Linux 走 `memfd_create`；**非 Linux 两步 fallback——先试 `qemu_memfd_create`（非 Linux 恒 ENOSYS，:96-98），再 `mkstemp("$TMPDIR/memfd-XXXXXX")` + `unlink` + `ftruncate` + `mmap MAP_SHARED`（:119-134）**。fd 存 `res->share_handle`，`do_set_scanout` 时经 `qemu_displaysurface_set_share_handle`（ui/console.c :456-465）接进 surface（virtio-gpu.c :689-690），`dbus_scanout_map` 把它 append 进 `GUnixFDList` 随 `ScanoutMap` 传出（dbus-listener.c :536-579）。

活体证据 [高·实测]：探针运行窗内 `lsof -p <qemu>` 见 `/private/var/folders/.../T/memfd-izFQ5H`（REG，4096000 字节，双 fd 引用同一 inode）——4096000 = 5120×800 与 ScanoutMap 的 stride×height 精确互证；壳侧 `mmap(fd, PROT_READ)` 成功读到像素区。**结论：mac 的「共享图」= 已 unlink 的匿名临时文件 + MAP_SHARED，跨进程共享语义完整，β-1 零拷贝前提成立**。附带定谳：这**不是** shm_open 路线（oslib-posix.c 的 `qemu_shm_alloc` :985-1035 服务 memory-backend，与本链无关）——02 号 §7「qemu_memfd 非 Linux 走临时文件后备」的 [中] 项升 [高]。

### 2.4 GLib/macOS 的 fd 面微基准

三组对照（`/tmp/b11/sp_fd_test*.c`，Homebrew GLib 2.90.1）[高·实测]：原生 socketpair `sendmsg/recvmsg` SCM_RIGHTS ✓；GLib `g_socket_send_message`（GUnixFDMessage）→ 原生 recvmsg ✓（nfds=1）；GLib 收 → 正确用法（`GSocketControlMessage ***` 出参）下 `g_socket_receive_message` 解出 `GUnixFDMessage nfds=1` ✓。**GLib 在 mac 的 D-Bus fd passing 双向可用**（中间曾误判接收侧缺陷，系本探针 API 用法错误——`g_dbus_connection_call*` 不带 fd 参，带 fd 的变体是 `g_dbus_connection_call_with_unix_fd_list[_sync]`，本地头文件 gdbusconnection.h :195-259 定谳；此坑记入 §5）。

## §3 三通道对照（数据路径 / 宿主 CPU / GPU 参与 / 工作量）

| 维度 | a) D-Bus p2p + Unix.Map（β-1） | b) VNC raw 直收（β-0 现役传输） | c) QEMU cocoa 窗口 |
| --- | --- | --- | --- |
| 数据路径 | QEMU 写 shareable pixman（=mkstemp MAP_SHARED）→ 壳 mmap 同一文件 → 按 UpdateMap 矩形 `write_texture` 上传 GPU → Metal 合成上屏 | QEMU 按 dirty 矩形发 RFB raw → TCP 环回 → 壳 RFB 解析（vnc.rs :465-508）→ 逐像素标量转换（:529-535 `pf.pixel` 循环）→ 全帧 Frame clone（:506）→ 呈现层 | QEMU 进程内合成上屏 |
| 每帧宿主 CPU 面 | **damage 矩形级**：读 UpdateMap 16B + 矩形内字节进 staging（write_texture 拷贝语义，docs.rs Queue::write_texture）——无序列化、无逐像素转换、无全帧遍历 | 每矩形：字节流读 + 逐像素标量循环 + 每次 update 全帧 clone（1280×800 相 = 4MB clone/update，vnc.rs :506 无 damage 载荷的现状） | 与壳无关（但壳失去全部自有面） |
| 编码与协议成本 | 零编码零像素拷贝（fd 传一次，矩形纯数字） | raw 无压缩（1280×800 全帧 4,096,000B；16×10 字符格小矩形 640B——算术，几何与 09 号实测矩形互证）；zlib/hextile 若协商则加解压 CPU | — |
| damage 到宿主 | 原生（UpdateMap 矩形即 damage；ScanoutMap Resize 语义自带） | 传输层增量（09 号 76+ 帧/30s 实证）但壳内damage 载荷缺失（frame.rs :29-36 Frame 无 damage 字段） | 取决于前端 |
| 输入 | 反走同连接（Mouse.SetAbsPosition/Keyboard.Press/MultiTouch——本探针实证发送路径） | 现役 VNC loopback（vnc.rs :397-415） | QEMU 窗口自收 |
| 壳自有面（工具栏/截图/--json） | 全保（壳自持 winit 窗口） | 全保 | **全失**（ADR #62 已裁） |
| mac 可达性 | **本机全链实证（§2）** | 实证（09 号） | 出局（02 号：cocoa 无 GL；本轮 `-display help` 复核 cocoa 在列但无 GL 面不变） |
| 壳侧新增工作量 | D-Bus 线协议子集 + QMP 喂 fd + Listener 服务 + wgpu 呈现（02 号估 1–1.5k 行；§5 尖角注记） | 近零（vnc.rs 已在役；damage 载荷 + wgpu 呈现为 B1-b 壳半） | — |

推断标注：D-Bus 路的「宿主 CPU 面 = damage 矩形级」结构判断一手（源码三级路径 + mmap 实证），绝对数待 B1-b 壳半落地后定标（任务书第三题性能定标归 12 号口径）。

## §4 Mac 最优推荐

**最短路径 = β-0 已在途 + β-1 直接兑现，不需要中间档**（β-1 的定量紧迫性另见 12 号同批定标：VNC 出口有 30ms 刷新定时的 33fps 结构性上限——60fps 目标下 β-0 只是过渡档）：

1. **β-0（先行，已实证）**：VNC 增量矩形自动在场（09 号），壳侧只差 Frame damage 载荷 + wgpu 呈现骨架（B1-b 壳半原计划）。console 绑定序按 09 号注记二（gpu 先于 ramfb 声明）。
2. **β-1（终态，本轮实证解锁）**：壳进程依次做——QMP unix socket 连 QEMU（现役 qemu.rs 已有 QMP 面）→ socketpair + `getfd`/`add_client(@dbus-display)` → p2p D-Bus 主连接（SASL EXTERNAL + NEGOTIATE_UNIX_FD）→ `RegisterListener`（第二 socketpair fd）→ 服务 Listener 接口（`Interfaces` 属性自报 Unix.Map）→ 收 `ScanoutMap` mmap 共享图 → `UpdateMap` 矩形驱动 `Queue::write_texture`（Bgra8Unorm-Srgb 或 Bgra8Unorm，bytes_per_row=stride 5120 类；pixman x8r8g8b8 = BGRA 字节序直配）→ 单 pass 采样缩放 + 工具栏层合成 → `present`；输入反走 `Mouse.SetAbsPosition`/`Keyboard`，VNC loopback 退役为诊断通道（QEMU 多 display 可并存，过渡期 `-display dbus` + `-vnc` 同挂合法）。
3. **Linux 同构升级缝**：`Listener.Unix.ScanoutDMABUF2` 在册（dbus-display1.xml），mac 不适用、Linux CI 可期——与 02 号 §2 判断一致，不进 mac 产品路径。

wgpu 形态依据（外部一手）：`Queue::write_texture` 拷贝语义——数据先入 staging、`submit` 才开始 GPU 执行（docs.rs wgpu Queue「Performance considerations」原文）；macOS 原生 Metal 后端（wgpu-hal `hal::api::Metal`）——壳从 softbuffer CPU 上屏换 wgpu 即得 Metal 合成，winit 集成不变。

## §5 实现注意（β-1 壳半的尖角清单，全部本探针踩过）

1. **D-Bus 报文头布局**：固定头 12 字节（endian/type/flags/version + body_len u32 + serial u32），头字段数组 `a(yv)` 的长度域在偏移 12..15、内容自 16 起（struct 8 对齐）——**不是**「16 字节定长头 + 12 处填充」。本探针手写层曾按后者构造，GDBus 静默丢弃消息（超时无应答）；jeepney 参考实现字节级对拍定谳（`/tmp/b11/` 对拍脚本）。
2. **fd 传递两前提**：握手期 `NEGOTIATE_UNIX_FD`/`AGREE_UNIX_FD`；带 fd 的方法调用须用 fd-list 变体（GLib 是 `g_dbus_connection_call_with_unix_fd_list[_sync]`；手写层则消息头带 `UNIX_FDS` 计数 + SCM_RIGHTS 同 sendmsg）。RegisterListener 不带 fd list 时 QEMU 侧断言 `G_IS_UNIX_FD_LIST` 后报错——本探针实测的错误形态可作壳测试的负例。
3. **Listener 侧角色**：RegisterListener 的 fd 上，**QEMU 是 GDBus 认证服务端、壳是客户端**（dbus-console.c :308-318；上游测试同构，tests/qtest/dbus-display-test.c :13-52）——壳握手方向勿反。
4. **能力自报**：QEMU 探测 Listener 能力走标准 `Properties.GetAll` 读 `Interfaces` 属性（XML 注释明言）；壳 Listener 应答 `{"Interfaces": ["org.qemu.Display1.Listener.Unix.Map"]}` 即激活 map 路径（本探针实测 `can_share_map` 生效判据 = ScanoutMap 到达）。用 GDBus `register_object` 风格框架时框架会代答；手写时需自答。
5. **手写客户端的一致性风险**：本探针 Python/jeepney 客户端在 12 秒窗内对所有对象路径收到 UnknownObject（含 VM），同拓扑下 C/GDBus 客户端全通——根因未隔离（候选：GDBus 服务端 add_client_ready 异步时序与手写客户端首消息的竞态），**开放问题在册**；建议壳半手写线协议时以「QEMU 实连 + ScanoutMap 实收」为验收断言，勿只做回环自测。
6. **上游测试即参照骨架**：`tests/qtest/dbus-display-test.c`（socketpair→add_client→AUTHENTICATION_CLIENT→RegisterListener→listener skeleton→ScanoutMap mmap 断言）与壳侧需求逐段同构，实现顺序可直接照搬。

## §6 风险与开放问题

- **§5-5 手写客户端 UnknownObject 竞态**：根因未隔离（不影响 β-1 可行性定谳——C 客户端全链通；影响的是壳实现工作量估计的置信度）。归属：B1-b 壳半首刀排查。
- **ScanoutMap 后备文件的页对齐**：mkstemp 路线无 memfd 的 seal/对齐保证；`offset` 字段（ScanoutMap 契约）承载 fb->offset，blob 路径下非零——v1 2D 资源实测 off=0，壳 mmap 按契约带 offset 即可（勿假设恒 0）。
- **多 Listener 并存语义**：RegisterListener 契约允许多 listener（「Multiple listeners may be registered simultaneously」）——壳与诊断工具并存可行，但 UpdateMap 扇出成本未测（推断：QEMU 逐 listener 发消息，成本线性）。
- **GLib 版本面**：本机 GLib 2.90.1（dev 系列）；fd 面微基准全通。若未来 Homebrew 升级引入回归，§2.4 的三组微基准是快速回归探针。
- **性能定标不在本文**：β-1 vs β-0 vs ramfb 基线的正式对比归任务书第三题（12 号已定标：VNC 出口 33fps 结构上限 + 帧成本四段分解）；本文只给结构账（§3）与通道可行性。

## §7 产物与复现

`/tmp/b11/`：`probe_full.c`（全链 C 探针：QMP 喂 fd + GDBus 双连接 + Listener vtable + ScanoutMap mmap + UpdateMap 计数 + Mouse 注入）与 `probe_full.log`/`probe_full2.log`（两轮实录）；`gdbus_client_test.c`（最小 C 客户端，VM/Console_0 属性读数）；`test_reg_nofd.c`（RegisterListener 正负例）；`sp_fd_test*.c`（GLib/macOS SCM_RIGHTS 微基准三件）；`dbus_p2p_probe.py`/`probe_v3.py`（Python 手写线协议与 jeepney 客户端——含失败实录，§5-5 证据）；`qemu-src/`（v11.0.3 源文件十一件，行号引用即出于此）；`pylibs/`（jeepney 0.9.0，对拍参考实现）。QEMU 拓扑见 §1；vars.fd = 64MiB 0xFF（05 号 §1.2 同法）。

## 来源索引

**QEMU v11.0.3 源码（GitLab tag 直取，行号为 /tmp/b11/qemu-src/ 镜像行号）**：ui/dbus.c :188-263（complete：p2p 只置全局/addr 连 bus/console 注册）、:283-312（add_client_ready：set_connection+start）、:315-372（add_client：GSocket+AUTH SERVER）、:526-544（dbus_init：addr/p2p 互斥源自 opts 检查 :531-532）；monitor/qmp-cmds.c :119-163（add_client 协议表 @dbus-display + getfd + fd_is_socket）；ui/dbus-console.c :253-336（RegisterListener：fd list→先回执→第二连接 AUTH SERVER→listener_new）；ui/dbus-listener.c :536-579（ScanoutMap 发送：GUnixFDList+几何）、:763-803（Update 子矩形 pixman 线性化）、:822-853（dbus_gfx_update 三级路径）、:1023-1037（implements 查 Interfaces）、:1119-1162（setup_shared_map）；ui/dbus-display1.xml（RegisterListener/Listener.Interfaces/Unix.Map.ScanoutMap/UpdateMap 契约原文）；ui/qemu-pixman.c :274-300（shareable_alloc→qemu_memfd_alloc）；util/memfd.c :50-100（Linux memfd/非 Linux ENOSYS）、:108-148（mkstemp 后备+MAP_SHARED）；util/oslib-posix.c :985-1035（qemu_shm_alloc 与本链无关的旁证）；hw/display/virtio-gpu.c :293-313（resource_create_2d shareable）、:662-695（do_set_scanout 接 share_handle）；ui/console.c :456-465（set_share_handle）、:478-505（默认 surface 亦 shareable）；include/qemu/dbus.h :26（ROOT=/org/qemu/Display1）；ui/dbus-console.c :551（Console_%d 路径）；tests/qtest/dbus-display-test.c :13-52（官方 p2p 姿势）、:86-178（listener skeleton+Interfaces 自报+ScanoutMap mmap 断言）；tests/qtest/libqtest.c :1913-1943（qtest_qmp_add_client：getfd+SCM_RIGHTS+add_client）。URL 形态：https://gitlab.com/qemu-project/qemu/-/blob/v11.0.3/ui/dbus-listener.c （其余文件同法换路径）。

**本机实测（2026-11-13，Apple M3 Pro / QEMU 11.0.3 Homebrew / GLib 2.90.1）**：§2 全部读数（probe_full 两轮 / gdbus_client_test / lsof memfd-izFQ5H / sp_fd_test3 微基准）；SASL EXTERNAL + AGREE_UNIX_FD（Python 手写与 C/GDBus 双客户端）；Python/jeepney UnknownObject 失败实录（§5-5）。

**本地仓内（路径:行号）**：`tools/berry/src/display/vnc.rs` :246-329（VncSource 连接与 enc 协商）、:393-415（输入反走）、:465-508（update：矩形循环+全帧 clone :506）、:510-538（raw 逐像素转换）；`tools/berry/src/display/frame.rs` :29-36（Frame 无 damage 载荷现状）；`tools/berry/src/display/ram.rs`（ramfb 对照面，07 号详测）；`tools/berry/src/window.rs` :481-537（softbuffer CPU 呈现现状）；`research/显示与加速体系设计/02-virtio-gpu-2D档-GPU加速路径兑现设计-v1.0.md` §2（β-1 设计）/§6（降档线）/§7（风险两条）；05 号 §0（D-Bus/memfd 挂账）/§1.2（拓扑形态）；07 号 §2.5（fb 直读法）；09 号 §4（VNC 增量 76+ 帧与 console 绑定序）；README.md（目录索引）。

**外部**：wgpu `Queue::write_texture`（staging 拷贝语义 + submit 才执行 + bytes_per_row 布局）与 Metal 后端（wgpu-hal `hal::api::Metal`）：https://docs.rs/wgpu/latest/wgpu/struct.Queue.html#method.write_texture ；GLib gdbusconnection.h（call 与 call_with_unix_fd_list 变体族，本地 /opt/homebrew/include/glib-2.0/gio/gdbusconnection.h :195-259）。
