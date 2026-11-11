# 应用 API 全景 v1.0（应用开发者可用面盘点与缺口方案）

> 版本：**1.0**（v1.0）｜2026-11-08

> **性质**：立法档的姊妹篇——回答「**应用拿什么 API 写、出齐了没有、没出的方案是什么**」。触发 = 用户检查：「用户态的 API 已经都出来了吗？不出来怎么做应用？要有规范，看别人怎么设计的。」
> **分账**：本文 = 有什么/缺什么/方案（API 全景）；`应用规范-v1.0.md` = 必须遵守什么（行为规范）；`应用开发指南.md` = 怎么打包跑起来（操作）；`系统调用ABI设计-v1.0.md` = 35 号权威编号表（一事实一归属，本文不复制全表只列族组）。
> **证据**：仓内代码真值（abi.toml/berry-libc/berry-ui 头注实测）+ 两轮 13 份对标底稿（`research/应用规范深研/README.md` 入口）。

## §0 一句话现状

**三层可用地已经能写出并跑通真实应用**（hexyl 0.17.0 未经修改 + appdemo/launcher 全链在役）；**管控面/后台面/GUI 高层 API 大多「立法在册、落地在途」**——这正是规范存在的意义：方案全套有、按刀序落、每个缺口有对标依据。下面逐层盘点，四态标注：【真】现役可用 ｜【法】立法在册未落（给挂靠刀） ｜【雏】雏形已入库待长全 ｜【空】空白本文立方案。

## §1 L0 内核层：36 在册 / 35 实现 / 帽 45

- 单一事实源 = `tools/abi.toml`（gen_abi 五产物同源，`just abi-check` 逐字节执法）——编号全表见 ABI 文档 §4，本文只列族组：handle 三（close/duplicate/replace）· object wait 三 + get_info · VMO/Vmar 九 · pager 三（demand paging G4）· process 四（create/start/exit_code/suspend）· thread 五（create/exit/set_priority/exception_channel/…）· sched_setattr（intent 注入面）· channel 三 · event 三 · clock · signal 两（send/take）· futex 两 · timer · mmap · telemetry/attest 预留。【真】
- berry-sys = raw 绑定 crate（abi_gen 镜像 + 封装），crates.io 发布序在册（SDK-1b）。【真】
- **判定**：内核机制面按 ADR #11 预算（≤45）健康收敛；35→45 的 10 号余量 = 号预算（电源六面已裁「新增 syscall=0」全走 IPC，ADR #68③——**号不缺，缺的是服务面**）。

## §2 L1 协议 crate 层：九件在册

| crate | 面 | 状态 |
|---|---|---|
| berry-fsproto | 路径命名空间五 op + statfs（v2） | 【真】（fsmgr 消费在役） |
| berry-netdproto | TCP 三 op 单会话 | 【真】（服务化自验在役；多会话【空】） |
| berry-psproto | 进程表 18 槽 | 【真】 |
| berry-guiproto | present/hdi/wm 域 + damage/vote/credits | 【真】（GUI-1..8 消费） |
| berry-appproto | APP_LIST 分片流/APP_SPAWN/APP_EVENT 留位 | 【真】（launcher 消费） |
| berry-blkproto / berry-ksproto / berry-netproto | vblk 自验/内核服务/网络低面 | 【真】（内部面为主） |
| berry-pwproto | 电源：PWSM v1——battery 38B/thermal 86B/modem 7B 快照 | 【雏】（ADR #68 六面的载体已起形；TimerSpec 32B/push 三口/task reason/SensorSub 未入） |

## §3 L2 berry-libc：musl shim「189 全定义」两档

- **真身档**【真】：malloc 族（静态 256KiB 堆）/write 族（fd 0-2→console channel）/mem/str 族/errno/clock 族/crt0（TLS 自设）/snprintf/fprintf/信号面全真（sigaction/sigprocmask/kill/raise/pause——A7-4 ③）/pthread 16 面（含 futex/mutex/cond）/mmap/io 25 面/fs 面（fd≥3 真身：open/read/write/close/fstat…经 fsclient→fsmgr）/net 6 面（经 netdproto）。
- **机械桩档**【真·诚实】：其余 musl 符号 loud 桩——ENOSYS/EBADF/`ok!` 成功谎报三分桶（stubs.rs 头注即账本）；**桩不假装成功**（除无竞争锁族），调用即显式失败。
- **开发者视角**：Rust std 面 = io/fs/thread/signal/字符串/格式化**可用**（hexyl/uutils 家族实证）；`std::net` 受 net 授能门（v1 appd loud 拒——【法】net 服务面挂账中）。

## §4 L3 GUI 栈：官方内置 UI「已有可用最小面」

- **berry-gfx**【真】：pass 形态（WebGPU 语义子集：caps/cmd/pass/queue）+ font（位图档）/icon（BYI1 解码）/lru/color 八模块——软后端永久 CI 基线，GPU 后端缝在（ADR #51）。
- **berry-ui**【真·最小面】：声明式 API（ADR #52）——UiMain（add/set_props/set_text/invalidate/set_offset/set_a11y/set_press_map）+ anim（QuadOut/CubicOut Q16.16 定点缓动）+ swipe（SwipePager）+ damage/layout/hit/metrics + IconSource/TextMetrics 供件槽；容量帽 512 节点/8192 desc。【法】DSL（views/\*.bui 文本载体，C2 拍板）随 SDK-2；L3 库 semver 自演进（ADR #55「框架是库不是层」）。
- **官方样例** = berry-launcher（desktop/frameloop/icons/layout_io/wallpaper 五模块——第一个长活 GUI 应用，全部模式可抄）。
- **动效**【法·数值已采齐】：官方动效库 = berry-ui anim + token 闭集——数值表已从四家官方源采齐（GUI-11：M3 16 档 duration + 9 条 easing 精确曲线、OH hover250/hover-to-press100/press250/click200ms、WinUI 167/250/333ms）；进词表闭集执法（u16 原子 id，GUI-6 立项注记）。

## §5 管控面：立法在册·按刀落（「方案有、代码在途」的主区）

| 面 | 立法处 | 状态/挂刀 |
|---|---|---|
| **墓碑后台**（后台运行 vs 墓碑的分级：active→freeze→tombstone） | 应用模型 §4.1 + 规范 §4.1：pause(save) 10s 内 / state blob ≤64KiB / 墓碑表 LRU / relaunch 读档复活 | 【法】协议已冻、app-1 落地（`TOMB ok`）；freeze 原语随调点 1.1-S3；**intent 单轴四档待遇表已裁**（interactive 失焦先 freeze、utility 墓碑候选、background 立即候选——省电与性能同轴） |
| 后台执行（资源证据模型 + 短时配额） | 规范 §4.2 + ADR #68④ background 能力位 | 【法】随 PM 六面刀 |
| 电源/性能/功耗管控 SDK 六面 | ADR #68③：TimerSpec/push 三口/task reason+evidence/SensorSub/thermal 只读/BatteryInfo——全走 berry-pwproto，**新增 syscall=0**；豁免位 day-one | 【雏】pwproto 快照三 payload 已入库，四件未入 |
| 自启动管控 | 规范 §5：boot 签名白名单 + autostart 能力位默认关 + 用户授予 | 【法】v1 现役 = boot 批集成者声明；白名单随镜像工程刀 |
| 权限弹窗/时效三态 | 规范 §2.2/§2.4：always/session/once + `[[grants]]` reason+scene 缺即拒装 | 【法】G1 弹窗 UI 随 appd 扩面刀 |
| 模糊定位 | 规范 §2.4：一服务两档 coarse(≈5km)/fine，授权 UI 选档 | 【法】随 location 服务（真机窗口） |
| 通讯录/替身数据 | 规范 §2.3/§2.6：picker 一次性 handle + **替身通道（空白返回=假 handle）** | 【法】随 contacts 数据服务 |
| 通知 | 规范 §6：三档投稿制 + 常驻通知绑后台 | 【法】notifyd 随 GUI-9/v2 |
| 深链验证/分享/输入法 | 规范 §9 | 【法】【v2】 |
| 剪贴板管控 | 规范 §8.4：系统持有+一次性 handle+提示默认开 | 【法】随 clipboard 服务 |

## §6 空白面方案（本文新增方案位——对标依据 → BerryOS 方案）

1. **应用侧 lifecycle API**：对标 UWP `Suspending`(deadline+deferral)/iOS scene 回调——BerryOS = lifecycle channel 消息协议已冻（规范 §4.1），**API 承载 = berry-libc 新 lifecycle 面**（注册回调/写 state blob/读 relaunch state handle），随 app-1 同刀；应用不直接碰 channel 细节。
2. **官方 UI 框架的公开 API 冻结**：对标 SwiftUI/ArkUI 公开面——berry-ui 是 L3 semver 库（ADR #55），**公开 API 面（UiMain 群）随 SDK-2 冻结**；DSL 双入口共享同一 TLV AST 单源（C2）；弃用走 API 治理章程双版本纪律。
3. **动效 token 闭集**：数值已采齐（GUI-11）——方案 = 官方动效库三件（时长档/缓动曲线/物理回弹）进 u16 词表，mkapp/ABI 单源执法（样式原子同链）；应用自定义曲线允许但不得冒用官方 token 名。
4. **替身数据 API**：对标 ColorOS 隐私替身/小米空白通行证（监管对冲件）——纯能力模型实现 = **假 handle**（contacts 服务对「替身」选择返回空/假树 handle，应用无感知）；授予 UI 三选（真授权/替身/拒绝）随 G1 弹窗刀。
5. **开屏/摇一摇广告禁令**：对标 292 号文（开屏「找得到、关得了」/禁整屏跳转）+ OPPO 审核规范（禁摇一摇高灵敏/禁锁屏广告）——BerryOS 执法位 = 弹窗必经系统 UI 服务渲染（结构性管控点）+ 商店审核（v2）；立法进规范 §10.4。
6. **`std::net` 通路**：net 服务多会话/客户端通用面（现单会话 ST_BUSY）——挂网络服务设计（索引「v2 前瞻」在册）。

## §7 「今天写一个应用」速查

| 想做什么 | 今天（v1） | 明天（挂刀） |
|---|---|---|
| CLI 工具（cat/ls/grep 类） | ✅ std io/fs/字符串全真——uutils 家族在役 | — |
| 打印/日志 | ✅ `console=true` → println! 直落串口；或 appd 采集对 | — |
| 多线程 | ✅ pthread/futex 真身（T-1） | — |
| 信号 | ✅ sigaction/kill/pause（A7-4） | — |
| 文件持久化 | ✅ `fs=["rootfs"]` 全卷（过渡态） | $appdata 私有树（v2） |
| GUI 应用 | ✅ 最小面：berry-ui+berry-gfx+wm/present RPC（launcher 样例） | DSL 热重载/多窗（SDK-2/#61） |
| 网络 | ⛔ 词表收、appd loud 拒（诚实） | net 服务面（网络设计刀） |
| 后台常驻/墓碑 | ✅ 机制+libc 三口已落——app-1 段 b-1 实证 `FROZEN ok n=1`/`TOMB ok n=1`（freeze-first + 墓碑 v0）；app-2 落 berry-libc lifecycle 面（register/save/restore 三口 + poll/beat 机械件、32B 头+CRC32 容器 v1，apptomb 经 libc 路首证；契约位 = app-1 草案 v1.2 + app-2 草案 §2/§3） | 真触发策略（失焦/压力）挂 GUI 线/v2；GUI 泵点位契约/主循环模板 + C ABI 冻结随 SDK-2 |
| 定时唤醒/推送 | ⛔ | PM 六面（#68） |
| 权限弹窗申请 | ⛔ v1 能力 = 安装期 manifest 声明即得 | G1 运行时弹窗 |

## §8 挂钩

`应用规范-v1.0.md`（§2/§3/§4/§5/§10 行为条文）· `应用开发指南.md`（操作面）· `系统调用ABI设计-v1.0.md` §4（编号权威表）· ADR #11/#43–#44/#51/#52/#55/#61/#64/#67/#68 · `research/应用规范深研/README.md`（13 底稿）· GUI-11（token 数值源）。
