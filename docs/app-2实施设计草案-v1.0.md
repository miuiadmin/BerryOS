# app-2 实施设计草案（应用生命周期第三刀：berry-libc 回调三口 + state 容器 v1）

> 版本：**1.0**（v1.0）｜2026-11-08
> **修订记录**：2026-11-08 对抗审查 17 路核实收编（MAJOR 3：在场判据槽位多产面/帧发送纪律/console 写面冻结审计；MINOR 11；QA 3——同区多条合并改，MAJOR 全收编）；唯一不入 = DOC-1 的「wait_frame 对账 SAVED+RESTORED 双向」替换短语经代码核伪（SAVED 臂仅采集 saved_c 不校验、唯一断言在 relaunch RESTORED 臂 c=saved_c+1），按核真口径统一落 §0/§7。

> **依据**：`应用规范-v1.0.md` §4.1-5（:188 立法原文——注册 pause 回调/写 state blob ≤64KiB/relaunch 读 state handle 三口，对标 UWP Suspending(deadline+deferral)/iOS scene「框架代收、应用只写业务」，未注册 = 默认可杀可墓碑零义务起步；§4.1-1 :184 pause 10s 契约）+ `app-1实施设计草案-v1.2.md` §5（app-2 前置立法：blob 头部 = berry-libc 三口代持保留区、payload 自 32B 起、`state_h()` 裸透句柄仅 v0 过渡形态；§4 v1 CRC32 经 hlen 扩展追加于头后）+ 调和书 `research/app-1调和审计-2026-11.md`（H 系①判据载体 = lifecycle 通道 / D8「libc 双 crate 形随 app-2」铅笔）+ 业界底稿 `research/应用规范-Windows-MSIX对标-2026-11.md` §3.1（SuspendingOperation.Deadline 因设备而异 + GetDeferral 仅延迟不追加时间）+ `research/应用规范-Apple-iOS对标-2026-11.md`（:39 Scene 生命周期词表——`sceneDidEnterBackground` 族框架回调形态；本文「框架代序列化」为规范 §4.1-5「框架代收、应用只写业务」的同义概括，非底稿原词——诚实边界注）。
> **时点**：app-1 全链落地后（a-1/b-1/b-2 三段 + 判例刀 `d563485`，沿革一六五/一六六；`FROZEN ok n=1`/`TOMB ok n=1` 在册）——SDK-2 开刀前的 libc 面一刀。v1.2 §8 挂账三件中 resume 复用注/DEADLINE 核账已随沿革一六九（3eb481e，2026-11-09 B 路补遗）先行收口，余 smoke-smp 实跑钉板由本刀刀3 顺路收口（2026-11-10 对账回写）。
> **形态先例**：app-1 草案 v1.2（密度/帽/eco 形）+ A7-4 协同投递循环（signal.rs——「登记不执行」与协同式投递点的边界教义源）+ berry-tls/berry-signal 纯面外迁 host 测 idiom（justfile test-host :550 面）+ stdhello/appdemo/bsh/sigchild 独立小工作区 musl 链（`[workspace]` 自声明先例）。
> **执行结果**：2026-11-10 代码刀（刀1+刀2）落地——刀1：`berry-lifecycle` 纯 crate 新立（user/ members 追加；32B 头编解码/CRC32 bitwise（png.rs 镜像互指单侧注记已写、`tools/berry` 侧对向注记随本刀禁改 tools/ 挂主会话）/hlen 分派（v0 白名单对恰 (0,12)/(1,32) 两组、混合对落 Bad）/8B 帧编解码/counter 代持状态机；host 测 17 项全绿 = §6 刀1 清单逐条：CRC `"123456789"→0xCBF43926` + 空基线 0 + 金件 blob 逐字节快照（payload `"berry"`→0x4A8DBAD3，zlib 独立预产）/counter 翻转域外钉板/前缀 12B 布局同位两条（offset const assert + v1 头喂 v0 白名单拒）/坏档四型/帽两臂/buf 两臂/帧 roundtrip 逐字节/counter 全序）+ berry-libc `lifecycle.rs`（五口 + 帧代发三分纪律（-EBUSY 自旋/他错 `lifecycle dead` park/-EAGAIN 不可达落死）+ 非阻塞诊断口（64-cap try-spin，BadFmt 分词三行：head/len/crc + buf-short）+ absent no-op 五件）+ io.rs 增 `console_h()` 访问器（诊断口取柄，纯增量）+ justfile test-host 追行。刀2：apptomb 换链（独立小工作区 + musl + berry-ld，build.rs/link-user.ld 退役删除、user/ members 摘除入 exclude）+ 换 API（四幕经三口：restore 三分型/ register/ poll+beat 两拍/ PAUSESV1 库内收束 exit(0)；TAG 手排与 12B 头手排与 `#[panic_handler]` 全退役；业务档 = 空态保存（v0 同义——v0 blob 本只含头）；诊断行全走非阻塞口（v0 行面保真：up/beat/saved/restored 行字面保留，resumed/probed 行随库化消亡（H1 辅证非判据），PROBED 回环由库 poll 代发——FROZEN/WAKE/TOMB 判据链保真的关键承接））+ appquiet absent 探针（§6 承诺⑧四断言，满帽 buf 落 static）+ justfile build-user APPTOMB 段改 musl 产链（manifest/金件/mkapp 零改——`stack=4 intent=background console=true` 过五道门禁复验）。验证：user/ 全 workspace none 面 `cargo build --release` 零 error（唯一 warning = svcmgr DPGB 段存量 `unused_assignments`，HEAD 原文非本刀引入）；apptomb/appquiet/appdemo/stdhello musl 面（berry-ld 链）全零 warning；任务给定 `-p apptomb -p svcmgr` 合并命令因 apptomb 摘出 workspace 结构性不可达，以等价两行执行（svcmgr `-p` none 面 + apptomb `--manifest-path` musl 面）；apptomb 产物 316KiB（stdhello 342KiB 同量级，R2）。**挂主会话收口件**：QEMU smoke 全表零漂实跑（FROZEN/WAKE/TOMB/APP 族）+ 首跑墙钟 + `just smoke-smp` 实跑钉板（刀3）；R5 svcmgr 两处『对端权威』注释修（本刀 svcmgr main.rs 零改约束下未动）+ `tools/berry/src/png.rs` CRC 镜像对向注记；stack_pages=4 实跑复核（若穿栈须 manifest+mkapp 金件连动——tools/ 禁改面）；mkapp 金件/负路径人工面落册与刀3 文书五件套。
> **输入口径**：本稿立稿输入 = 两路侦察结论（A：libc/框架面 dispatch 形态对比——本文 §2 裁决基座；B：协议/验收面事实书——本文 §1/§3/§6 基座）+ 本会话直读代码补核（下文全部行号锚点经工具核实；行号随刀漂移以函数名为锚，app-1 惯例）。

## §0 目标与范围盒子

**一句话**：把规范 §4.1-5 的三口从 svcmgr/apptomb 裸协议面升为 berry-libc 封装面——**显式 poll dispatch + 库代持 32B 容器头（含 CRC32）+ 帧面库代发**——apptomb 迁 libc 路作首个现役消费者，svcmgr tomb 面（帧码/对账/帽）**零改**。

**术语注**：本文「三口」= 规范 §4.1-5 立法的三个应用面（注册回调/写 state/读 state）；「配套协议机械件」（poll/beat）是三口的运行时载体，不是第四第五口——poll 的地位 = signal 面的 `may_dispatch`（协同式投递点），beat = 业务节拍帧的代发通道。对外 SDK 文档面（SDK-2）只暴露三口语义，机械件以「主循环模板」形态随附。

**范围盒子（本刀不做，逐条注明去处）**：

| 不做 | 原因 | 去处 |
|---|---|---|
| 真触发（lc_policy 真事件源 FocusLost/MemPressure） | v0 无 wmd 焦点事件、TCG 无真压力（v1.2 §0 同款） | AM-1.5（wake-pending）后 GUI 线/v2 策略刀；policy 函数体换词书面已在 v1.2 §4 预留 |
| GUI 应用真冻结/失焦暂停 | freeze obligations 谓词（PAGER_REPLY/IPC_REPLY）未落；GUI extras 8/8 恰满 | v2 obligations 刀；本刀 §5 只预埋泵点位契约【v2 预埋】 |
| libc 阻塞点自动泵（may_dispatch 同五点旁挂 lifecycle_may_poll） | compute 紧循环/berry-sys 直调 wait_many（launcher 形）结构性失明——覆盖收益是幻觉；回调在 write()/read() 中途炸出 = 业务状态中途序列化（§2 裁决②③） | v2 证据驱动增强档——「注册 = 同意 = 可文档化偏差」A7-4 教义同源 |
| deadline 分档 + PAUSESV1 携 deadline 位 | appd 单钟执法现状（起算 = 发出时刻）；规范 §4.1-1 值 10s 单档 | v2 按 intent 分档复核；线兼容携法预埋注 = §7 帽表（下行 [4..8] 位空闲且无消费者——app 侧收帧不读 counter；帧 counter 断言只在 RESTORED 方向、SAVED 臂仅采集）【v2 预埋】 |
| deferral（回调内异步保存/延后完成通知） | v1 语义 = 同步回调 + deadline 内完成（UWP「GetDeferral 立即完成」等效形）；apptomb 契约同构 | v2 长保存诉求 → ExtendedExecution 对标另裁 |
| 跨 boot 复活/墓碑表落盘 | 表驻 RAM、$appdata 树未落（v1.2 §0 同款） | v2 |
| 多 app 并发墓碑/多窗 | v0 单活自验 | v2 真策略刀 |
| 多线程 lifecycle 面（pthread 并发调五口/并发入回调） | lifecycle 库态（counter/回调槽）static mut 无锁；libc 跨线程面已部分真身化（pthread_create 真身 + malloc/key futex——lib.rs ABI 口径节「多线程根基(T-1)」；errno 单实例窄竞窗），五口不在其列 | 随 T-1 另裁；v1 五口契约 = 应用主循环线程独占（§2.2 全口契约） |
| 三口 API 对 C 头文件/ABI 冻结 | v1 面 = berry-libc Rust 面（apptomb 唯一消费者） | SDK-2（对外 SDK 面 + berry-ld 消费者）——本刀只立语义不冻 C ABI |

**口径收窄注**：「apptomb 迁移」与「三口落库」是两把刀（§4 裁决分刀）——本草案立法两者，刀程切分到 §8；「迁移动了 smoke 判据」与「新 API 有 bug」在证据面上必须可归因分离。

## §1 现状与依赖（app-1 落件锚点表，2026-11-08 实测）

| 层 | 状态 | 锚点（函数名为锚，行号随刀漂移） |
|---|---|---|
| 协议/帧面【现役·本刀零改】 | 已落 | svcmgr TAG 族 :5698-5708（apptomb const 块 :47-55 对端权威）；`wait_frame` tag+counter 对账 :5863-5879；PAUSESV1 发送 + counter 恒 0（「暂停令无参」:6033-6035）；帽族 :5709-5720（UP 10s/静默窗 8s/RESUME 5s/PAUSE 10s/RESTORE 10s）；-EBUSY 退避 100ms×40 帽 4s :5894-5912（D4）；记表 gate 仅验首 4B magic :6076-6079；relaunch dup(R\|W) + 新 lifecycle 对 :6097-6103 |
| 自验消费者【现役·迁移对象】 | 在役 | apptomb hello 模型（Cargo 单依赖 berry-sys、no_std/no_main）：TAG 手排 :47-55；12B 头手排 :62-64（`BTMB`/ver=0/hlen=12/rsv=0/counter）；env 块自解析 :171-196；启动判读三分型 :203-224；稳态两拍循环 :229-269；`save_and_exit`（写成功才发 SAVED→exit(0)）:154-167；未知 tag 忽略续跑 :252-254（前向兼容面显式归本刀） |
| libc 访问器/装配面【现役·已备未消费】 | 在册 | `lifecycle_h()` io.rs:275-278（GIFT_H[0] ↔ env hs[4]）、`state_h()` :284-287（GIFT_H[1] ↔ hs[5]）——落种点 `berry_libc_boot` io.rs:107-112；svcmgr 装配 `let gifts = [lc_b, st_dup]` :5805（**仅 tomb_selftest 显式装配**；boot 批 app_launch 与 SPAWN RPC 恒不装 lifecycle 对——但 wants_fs 的 app 经同一 spawn 守卫在 hs[4]/[5] 落 rootfs VMO RO dup + fsmgr 通道 SEND\|RECV dup（fs_gifts 固占槽位、带 fs 词 app 结构性冲突——v1.2 §4 装配面立法原文；⇒ GIFT_H[0]/[1] 非 0 载错对象，在场判据见 §2.3 重立））；state VMO options=0 + commit 全幅 + dup(R\|W) :5774-5788 |
| 冻结载荷条件【现役·结构性约束】 | 在册 | sched.rs `freeze_precheck_by_pinned` :953-1000——SLEEPQ 任务 `wait_signals & (READABLE\|WRITABLE) != 0` 即全进程拒冻，对**任何** handle 的阻塞 object_wait 都触发；⇒ 三口 dispatch 结构性排除「lifecycle 句柄挂进阻塞 wait 集」的一切变体（v1.2 §5 立法原样继承——阻塞化不是无害 API 演进） |
| libc 执行面底账【现役·勘定】 | 在册 | musl 1.2.5 shim 真身/桩两档（lib.rs :26-36 分档表）；无运行时线程；`atexit` 登记不执行 io.rs:871-887；A7-4 协同投递 = `may_dispatch()` 五点（signal.rs :494-500 + io.rs :526/:560/:642 + sys.rs clock_nanosleep 尾 + futex/pthread_join）——compute 紧循环结构性收不到（A7-4 §3.4 偏差清单——文档化先例）；`exit` = EOF 先行 + 0x33 io.rs:844-847 |
| 主力 GUI 消费形态【现役·盲区证据】 | 在册 | berry-launcher frameloop.rs :563-579 稳态等待 = 直调 `berry_object_wait_many([wm, input], deadline)`，帧循环无必然 libc 调用——libc 阻塞点自动泵对 berry-sys 直调形态结构性失明（§2 裁决②的实证） |
| 验收面【现役·零漂基线】 | 在册 | tools/markers.sh ROOTFS_MARKERS 尾位 +FROZEN/+TOMB（smoke/smoke-smp 双脚本单源）；smoke-image.sh ECO_MARKERS 硬拷 = 第三消费方；规则/07 相构成表；mkapp 金件 tools/mkapp.py:1195（APPTOMB manifest 逐字节镜像，intent="background" 首个正样本，stack_pages=4）；mkapp 负样本 id 25B :1158 |
| 文书立法位【现役】 | 在册 | 规范 §4.1-5 :188；草案 v1.2 §5 :88（头部保留区/payload 自 32B 起/裸透句柄仅 v0 过渡）+ §4 :76（v1 CRC hlen 扩展）+ §7 :107（blob 头 12B 定长 → v1 经 hlen 扩展） |

**依赖判据（开工前置全绿）**：①`FROZEN ok n=1`/`TOMB ok n=1` rootfs 相在册（沿革一六五终验）；②apptomb/svcmgr TAG 双源镜像一致（现役无漂）；③host 链 `just test-host` 全绿（新纯 crate 落位前置）。

## §2 三口 API 定形（dispatch 裁决 + 签名 + 零义务兼容律）

### §2.1 dispatch 形态裁决：显式 poll 为 v1 唯一形态【v1 立法】

三候选对比结论（侦察 A 全案收编）：

| 维度 | A. 显式 `lifecycle_poll()`（**采**） | B. libc 阻塞点自动泵 | C. 信号面借用（signum 编码/HELD 复用） |
|---|---|---|---|
| freeze 兼容（§1 F2 约束） | 两拍循环原样继承（与 apptomb :229-269 手写 poll 同构） | 不违反 F2（泵在返回后）但不解决「应用长睡不醒」 | 信号不唤醒阻塞目标（A7-4 文档明「不唤醒」）——连叫醒都做不到 |
| 覆盖面 | 应用自定节拍、契约显式可测 | compute 紧循环/berry-sys 直调 wait_many（launcher 形）结构性收不到 | 同 B 盲区全继承 |
| 回调上下文 | 应用主循环内、语义位点确定（业务状态非中途态） | 任意 libc 阻塞调用返回处炸出——async-signal-safe 类问题整体进口、存档脏写面 | 同 B + `extern "C" fn(c_int)` 无载荷语境 |
| 协议契合 | channel recv 天然承载 8B 帧 + counter | 同 A（最终仍走 recv） | bit 合并丢消息/无载荷/可被 sigprocmask mask 断链——只配当门铃且省不掉 poll |
| 侵入面 | 最小（新面零触碰 io/fs/signal） | io/sys/futex/pthread 五文件全触碰 + in-callback 重入闸 | 污染 POSIX 信号命名空间 + berry-signal 单源表掺 lifecycle 语义 |

**裁决四理**：①F2 约束下两拍循环是唯一合法形态，A 与它同构、迁移零概念转换；②launcher 实证（§1 盲区证据行）B/C 对 berry-sys 直调 GUI 帧循环结构性失明——覆盖面收益是幻觉；③回调上下文确定性是「框架代收、应用只写业务」的必要条件（UWP/iOS 均在应用自己的事件循环上交付回调，B 把该语义降级成随机打断）；④C 的 bit 合并/无载荷/可 mask 三病使它只配当门铃，省不掉任何机制。**B 的五点旁挂 `lifecycle_may_poll()` 留作 v2 注册后增量增强档**（注册 = 同意 = 可文档化偏差，A7-4 §3.4 教义同源）——v1 不做。

### §2.2 API 面（berry-libc 新模块 `lifecycle.rs` + 纯面 crate `berry-lifecycle`）

**分层**：纯函数面（32B 头编解码/CRC32/hlen 分派/三分型状态机 + 8B 帧编解码——帧⇄(tag,counter) 纯函数，tag u8 + pad 3B 恒 0 + counter u32 LE——+ counter 代持状态机转移：Restored(N)→置 N+1 / beat→+1 / save→写头取当拍值）外迁独立 crate `berry-lifecycle`（user/ members 追加，零依赖 no_std——berry-tls/berry-signal idiom；berry-libc 结构性不可 `cargo test`，lib.rs :214-218 勘定）；syscall 胶水面（句柄获取 + syscall 收发〔vmo_write/recv/send〕+ 幕序编排）留 berry-libc `lifecycle.rs`（`pub mod` 追加于 lib.rs :64-76 模块表）。

**签名（Rust 面，C ABI 冻结挂 SDK-2——§0 不做表末行）**：

```
// —— 规范三口 ——
lifecycle_register(cb: Option<extern "C" fn(ev: LcEvent)>) -> LcReg   // 注册 pause/resume 回调（单槽覆写）
state_save(payload: &[u8]) -> LcSave                                   // 盖 32B 头 → 写 state VMO → 发 SAVED（写成功才发）
state_restore(buf: &mut [u8]) -> LcRestore                             // 读档三分型 + 库代发 UP/RESTORED（调用方契约 = buf ≥ 64KiB-32B 满帽，§3.2）
// —— 配套协议机械件（三口的 dispatch/节拍载体，非第四第五口语义）——
lifecycle_poll() -> u32                                                // 非阻塞排空（不返回型：PAUSESV1 收束见下）
lifecycle_beat() -> u32                                                // counter 库代持递增 + 发 BEAT，返当拍值（诊断辅证）
```

- `LcEvent = c_int` 自有枚举（v1 两值 `LC_EV_PAUSE=1`/`LC_EV_RESUME=2`；**勿复用帧 tag**——API 与线协议解耦，v2 事件词表扩展零协议联动）。`LcReg = {Ok, Absent}`（`cb=None` = 注销，同口覆写式）；`LcSave = {Ok, TooLarge, Absent, IoErr}`；`LcRestore = {Fresh, Restored(n), BadFmt}`（三分型归库——apptomb :203-224 启动判读的库化）。
- **counter 归属【v1 立法】**：counter = 协议对账域，**库代持**（restore 时读头 +1 存内部、beat 递增、save 写头）——应用 payload 不含协议域，RESTORED c=N+1 续接对账（svcmgr wait_frame :5872 位）由库兑现；「框架代收、应用只写业务」落到数据面。
- **帧面归属切分【v1 立法】**：UP/SAVED/RESUMED/RESTORED 属协议机械 → **库代发**；BEAT 属业务节拍 → 库代发但节拍由应用主循环驱动（`beat()` 挂心跳拍）；**svcmgr `wait_frame` 的 tag+counter 对账面（:5863-5879）与 8B 帧码零改** = 迁移不破 smoke 的判据线。
- **帧发送纪律【v1 立法】**：库代发五帧（UP/RESTORED/SAVED/RESUMED/BEAT）的 lifecycle 通道 send 错码三分——`0`=成功；`-EBUSY`=**非阻塞自旋重发**（apptomb `send_msg` 同款契约：满队瞬态〔队深 8、启动饱和窗〕自旋由轮转兜底必达），**明文禁 `wait_one(lifecycle_h, ·)`**——§1 冻结载荷条件在 send 面的落地（lifecycle app 是冻结判据载体进程本身，句柄入 wait 集即全进程拒冻）；`-EAGAIN`=send 面结构性不发生（chan send 满语义为 -EBUSY、-EAGAIN 仅 recv 空队——kernel channel.rs 头注），实现按不可达断言；他错=「lifecycle dead」park（§2.2 错码纪律条；帽表对偶行 = §7）。**与雷 2 判例（雷册 §A51「新用户程序打印一律阻塞形态」）的对立豁免**：雷 2 死状语境 = console 排水窗多生产者 RR 环序垄断（自旋者永不出 runqueue 结构性饿死）；lifecycle 通道 = 每 app 单通道、低频 8B 帧 + 对端 svcmgr `wait_frame` 定向收，非多生产者竞争面——且 lifecycle app 本身是 F2 冻结判据载体，阻塞形态结构性不可用，自旋是唯一合法形态非偷懒。
- **调用时序（规范 §4.1-5 三口的运行时形状，四幕）**：

```
幕①启动判读      state_restore(buf) ── Fresh ──→ 库发 UP c=0（counter 内部置 0）
                   │                    ├─ Restored ─→ 库发 RESTORED c=N+1（counter=N+1）
                   │                    └─ BadFmt ──→ 库单行诊断 + 库发 UP c=0
幕②注册          lifecycle_register(Some(cb))   —— Ok/Absent（零义务面见 §2.3）
幕③稳态两拍      loop { lifecycle_poll(); sleep(拍); lifecycle_beat(); }
                   ├─ poll 得 RESUME ──→ cb(Resume) + 库发 RESUMED c=当拍（判据 = 帧本身）
                   └─ poll 得 PAUSESV1 → cb(Pause) → cb 内可调 state_save
幕④pause 收束     cb 返回 → {已 save：SAVED 已发} → 库 exit(0)（EOF 先行）——poll 不返回
                   ├─ cb 未调 save：零写拒绝保存（不发 SAVED）→ 同 exit(0)——安全侧不入表
                   └─ save 返 IoErr（vmo_write 他错）：视同未 save——零写路径同 exit(0)（安全侧不入表，不 park——错码归调用方，§2.2 错码纪律条）
```

（幕④ = apptomb `save_and_exit` :154-167 的库化同构——「写成功才发 SAVED → exit(0)」两径不变；未调 save 的 appd 侧降级路径与 v2 快判优化 = §8.2 K5。）
- **全口契约【v1 立法】**：五口（lifecycle_register/state_save/state_restore/lifecycle_poll/lifecycle_beat）= **应用主循环线程独占**；回调内可调集 = {state_save}；poll/register/beat 回调内重入 = UB；worker 线程（pthread_create——THR 面在役）并发调任一口 = UB——lifecycle 库态（counter/回调槽）为无锁 static mut，未入 libc 跨线程共享面（lib.rs「多线程根基(T-1)」部分真身化不含五口——§0 不做表第 8 行）。
- **同步回调 = 无 deferral 的立法说明**：UWP Suspending 的 Deadline+GetDeferral 是异步完成模型；BerryOS v1 = 回调同步执行 + deadline（10s，appd 侧单钟）内完成——等效「GetDeferral 立即完成」形；deadline 查询口**不做**（appd 发出时刻起算的单钟口径不可被 libc 侧第二执法钟稀释——B 侦察③钟面判据），v2 分档时经 §7 预埋位下发。
- **`#[panic_handler]`/错码纪律（兜底收缩）**：dead-park 兜底**仅限帧 send 面他错**（帧发送纪律三分表末臂）= apptomb `dead()` 同款 loud 单行诊断（`lifecycle dead`）后 park——失败归因双通道（app 侧诊断行 + svcmgr eco 降级行，D9 形）；state VMO 写面（vmo_write）错码**不 park**——返 `LcSave::IoErr` 归调用方（:80 值域语义兑现，消解「IoErr 与 park 不可同真」），幕④径走零写 exit(0)。错码三分哲学与 D4 先例同源（svcmgr ⑤ 退避：仅 -EBUSY 可重试、他错各对应明确拒绝点立即 loud）。
- **诊断口纪律【v1 立法】**：`lifecycle.rs` 增设**非阻塞 best-effort 诊断口**（hello `print_text`/`print_line` 64-cap try-spin 同款、超帽丢行）——库内 loud 诊断行（`lifecycle dead`/§3.2 BadFmt 行）与冻结敏感窗内的迁移侧诊断行全走此口，**不经 libc `console_write`**（EBUSY 时 wait_one(console, WRITABLE, 10s) 阻塞重试——冻结窗第二阻塞暴露面，§8.3 R3）；丢行无害的根据 = H1 已立法 console 行仅辅证、协议判据不依赖 console 行。

### §2.3 零义务兼容律【v1 立法】

- **在场判据【v1 立法·重立】**：GIFT_H 槽位 = **多产面**（io.rs `rootfs_h`/`fs_h`/`lifecycle_h`/`state_h` 同槽不同名）——**handle 非 0 不构成 lifecycle 在场判据**。`lifecycle_h()==0` ⇒ Absent 仅对**无 fs 词 app**成立（单向下行安全）；带 fs 词 app（现役反例 = APPDEMO，manifest `fs=["rootfs"]`、boot 批拉起——§1 装配面行）GIFT_H[0]/[1] 恒被 rootfs VMO dup + fsmgr 通道 dup 占用，`lifecycle_h()≠0` 载的是**错对象**：register 会返 Ok 而非 Absent、beat() 对 VMO 句柄 send 类型错必败 → dead-park——三口对该面 v1 定义为**未定义行为**（收窄执法 = SDK-2 主循环模板只随 lifecycle/tomb spawn 面分发、文档化不向 fs 词面承诺）；absence 判据重构（env 打标/专用槽——需破「svcmgr tomb 面零改」约束）挂账 v2（§8.4 SDK-2 前置输入清单带离）。
- **结构性缺席面（无 fs 词 ∧ 未装配 gift）**：`lifecycle_h()==0` ⇒ `register` 返 Absent、`poll` 立返 0、`state_save` 返 Absent、`state_restore` 返 Fresh 且**不发帧**（无对端无从等帧）、`beat()` 立返 0 不发帧 counter 不递增（与 poll 立返 0 同构）——**三口+机械件全 absent no-op**，零义务 app 零代码改动零行为漂移。验证锚 = appquiet absent 探针（刀2，§6 承诺⑧）+ appdemo/boot 批 smoke 全表零漂（appdemo 面 = fs 词 UB 收窄弱锚，§6 fs 词面验证锚行）。
- **未注册回调但装配了正确对象 gift**（正确对象的面 = 仅 tomb_selftest 显式 spawn 通道；其余面 = 未装配〔absent，上条〕或装配错对象〔fs 词 UB 面，首条〕）：`poll` 收 PAUSESV1 → 库直走零写路径 `exit(0)`——「默认可杀可墓碑」的诚实死法（监督杀 + 不入表，状态不保留）；`poll` 得 RESUME → cb 不调（分派前置未满足）+ 库照发 RESUMED c=当拍——§2.2 帧面归属切分的推论升明文；未收则照常 poll 返——**注册不是收帧的前置，只是回调分派的前置**。
- **hello 型 app 不链 libc 结构性不受影响**（无 libc 链接即无三口——范围与链接面同构）。

## §3 blob 容器 v1（32B 头 + CRC32 + hlen 扩展序）

### §3.1 头布局【v1 立法】

```
0..4   magic u32 LE = "BTMB"     ← v0 同位（appd 记表 gate 首读面零改，:6076-6079）
4      ver u8 = 1                 ← blob 格式版本（协议侧非 app 版本；v0=0）
5      hlen u8 = 32               ← 读者按 hlen 分派（12=v0 路径/32=v1 路径）
6..8   rsv u16 = 0                ← 恒 0（扩展只经 hlen 追加，不塞 rsv——v1.2 §4 原文）
8..12  counter u32 LE             ← v0/v1 同位（hlen 扩展只追加不改前缀字段布局——续接对账跨版本稳）
12..16 payload_len u32 LE         ← v1 新增
16..20 crc32 u32 LE               ← v1 新增（poly 0xEDB88320；覆盖域 = payload 全幅，不含头）
20..32 rsv2 12B = 0               ← 保留区（v2 元数据继续经 hlen 扩展追加）
32..   payload                    ← 应用业务域，帽 = 64KiB - 32B
```

- **「v0 头布局为自验件试验格式、非平台 ABI」注随本节退役**——v1 起头布局升为平台 ABI（v1.2 §5 头部保留区律的兑现点：magic/ver/hlen/len/crc/counter 全域归 berry-libc 三口代持，应用侧零返工）。
- **CRC32 落点【v1 立法】**：berry-libc/berry-sys 均无 crc/hash 面；berry-sys 纯 raw syscall 镜像章程（绝不分配绝不 panic）塞 CRC 直接违章程。落点 = `berry-lifecycle` 纯 crate 内嵌 bitwise 实现（poly 0xEDB88320，`tools/berry/src/png.rs` Crc32 同算法镜像——两处单源问题以 host 测向量钉板：标准校验值 `"123456789" → 0xCBF43926` + 金件 blob 逐字节快照；镜像互指注记双写）。64KiB 面上 bitwise 性能非约束（pause 窗 10s ≫ ms 级），查表优化不做。**头域完整性分工【v1 立法】**——CRC 覆盖域 = payload 全幅不含头（:118），非 blob 全幅保护：counter（8..12）单错不落 BadFmt（静默 Restored），由 svcmgr `wait_frame` 对账兜底（⑪ rctr 臂——restored c≠saved_c+1 → `tomb rctr` 红；跨裁体分工：协议域完整性归 svcmgr 对账、数据域归 berry-libc CRC）；len（12..16）错由「crc 按错 len 计算必不符」+ len 超界臂（§3.2）间接兜底（后果 = BadFmt 弃档按零态，非恢复原态）；hlen/ver 由 hlen 分派序直接落 BadFmt（§3.2 末行）。

### §3.2 读写兼容规则（hlen 分派序）

| 场景 | 读侧行为 | 后果 |
|---|---|---|
| 零态（非 BTMB） | Fresh——全新启动语义 | 库发 UP |
| BTMB ∧ hlen=12 ∧ ver=0 | v0 路径（旧档兼容读） | 库按 v0 counter 位续接 |
| BTMB ∧ hlen=32 ∧ ver=1 ∧ len/crc 过验 | Restored(n)——payload 填 buf；n ≡ 头中 payload_len（满帽下实填量恒等，消歧义） | 库发 RESTORED c=N+1 |
| 同上但 payload_len > buf.len()（buf 不足帽） | BadFmt 臂拒读——诊断行分词 buf-short ≠ 档坏 | 不截断不半档；按零态库发 UP |
| BTMB ∧ hlen=32 ∧ (len 超界 ∨ crc 不符) | **BadFmt 弃档**按零态 + 诊断行 | 库发 UP（CRC 首个真实消费者：静默坏档 → 显式红） |
| BTMB ∧ (ver 超前 ∨ hlen ∉ {12,32}) | BadFmt 弃档 + 诊断行 | 同 v0 badfmt 臂（apptomb :216-218 同义） |
| 旧头新读（hlen=12 → v1 读者） | v0 路径天然兼容 | 续接稳 |
| 新头旧读（hlen=32 → v0 读者） | 落 badfmt 臂安全降级（不 misparse） | restored 判据断裂 = loud 红 |

- **同刀约束（推论立法）**：v1 头写读两侧必须在**同一自验件二进制内同刀升级**（自写自读天然同步——刀2 的形态根据）；迁移窗内 v0 读者遇 v1 头 = badfmt 安全红（`TOMB FAIL` loud），不产生静默错档。appd 记入 gate 只验首 4B（:6076-6079）+ counter offset 8..12 跨版本同位 ⇒ **svcmgr 记表/relaunch 链零改**。
- **写侧规则**：`state_save` 恒写 v1 头（ver=1/hlen=32）——无版本协商（v0 写者只有 apptomb 一员且随刀2 同刀消亡）；buf 越帽（>64KiB-32B）返 TooLarge 拒写（VMO 64KiB 定尺寸结构性执法之上再加库侧前置拒——诊断前移）。**零长 payload 合法**（len=0 ∧ crc(空) 过验 = 「空态保存」）：区别于零写拒绝保存——空态保存 = 有头有 SAVED = 入墓碑表、relaunch 按零 payload 恢复（应用「要墓碑位不要业务档」的合法形；crc 空输入基线 = init 直出 0，host 测钉板）。
- **读侧 buf 契约【v1 立法】**：调用方契约 = `buf ≥ 64KiB-32B` 满帽；`payload_len > buf.len()` 即拒读（BadFmt 臂，上表——诊断行分词 buf-short ≠ 档坏），**不截断不半档**——截断形（Restored(buf.len())）使 n 语义永久歧义 + 半档静默，不采；n ≡ 头中 payload_len（满帽下实填量恒等）。连带的栈面约束：满帽 buf 65504B > apptomb 栈 4 页 16KiB——buf 落 static/BSS 不入栈（并入 §4 步3 stack_pages 复检面）。

## §4 apptomb 迁移（分刀裁决 + 步骤）

**裁决：分两刀（刀1 先立三口纯面，刀2 迁消费者）**【v1 立法】。理由：①`TOMB ok` 是 app-1 验收锚——同刀迁移则「新 API 正确性」与「既有谓词连续性」耦合，红即不可归因；②§3.2 兼容矩阵表明头版本切换只能在自写自读同刀内安全完成——迁移刀必须独立、可单独回滚；③v1.2 §5 的「libc 双 crate 形随 app-2」是铅笔立法（D8），分刀使铅笔兑现点显式。**备选形态**（apptomb 保留 hello 型当裸协议 oracle + 另立 musl 型自验件——双层钉板，成本 +1 bin +1 标记 + smoke 连动）**不采**：oracle 价值由 svcmgr tomb_selftest 的 wire 断言（帧码 + counter 对账 + 记表链）承接，TOMB ok 本身就是协议对拍面。

**迁移前后形态对照（刀2 验收的差分面）**：

| 面 | 现役 hello 形 | 迁移后 libc 形 |
|---|---|---|
| 链 | user/ members + aarch64-unknown-none、单依赖 berry-sys | 独立小工作区 `[workspace]` + musl target、单依赖 berry-libc |
| 入口/env | `#[no_std] #[no_main]` `_start(env)` 自解析（:171-196） | libc crt0 `berry_libc_boot` 代收（argv/console/extras 落种 io.rs:95-118） |
| 协议帧 | TAG 常量族手排 + `send_msg`/recv 手写（:47-55/:229-256） | 库代发（UP/RESTORED/SAVED/RESUMED/BEAT）+ `poll` 分派（§2.2） |
| blob 头 | 12B 手排 ver=0（:62-64） | 32B ver=1 库代持 + CRC32（§3） |
| counter | app 局部变量 c | 库代持（restore 读入/beat 递增/save 写头） |
| 死亡/退出 | `dead()` 自旋 park + `berry_process_exit(0)` | 库 loud 诊断 + `exit`（EOF 先行 io.rs:844-847）同义 |
| sleep | `berry_thread_sleep(t0+2s)` 直调 | libc `nanosleep`（同底 sys.rs:114-135——可冻形态不变，R3） |
| 不变式 | 零义务装配面/selftest 打包位/两拍循环/禁阻塞等任何 handle（通道+console 写）/「写成功才发」 | 全数原样继承（§6 判据链逐环） |

**迁移步骤（刀2 主体，apptomb 单 bin 原名原位）**：

1. **换链**：apptomb 转独立小工作区 musl target——自身 `[workspace]` 声明（stdhello/appdemo/bsh/sigchild 四先例同形）+ 从 user/Cargo.toml members 摘除；justfile build-user 追 musl 段（stdhello 同链：berry-libc shim + berry-ld 剥 musl crt + `-zmax-page-size=4096`）；env 解析/argv/console 全交 libc crt0（`berry_libc_boot`）；`#[used] static CRT0_SEED`（appdemo :23/appquiet :16 先例——防 fat-LTO 丢 crt0）；打包位 selftest/ 专用位不动（D1 立法零改）、mkapp 金件随 manifest 变更连动（§7）。
2. **换 API**：TAG 手排收发（:229-256）→ `register` + `poll` 入心跳拍；`save_and_exit`（:154-167）→ `state_save` + 库内收束；启动判读（:203-224）→ `state_restore` 三分型；BEAT/counter → `lifecycle_beat()`（counter 库代持后 apptomb 侧 c 变量消亡，诊断行用返回值）；`dead()`/argv0 断言保留（契约面不变）；console 诊断行保留，但**冻结敏感窗（稳态两拍循环 + pause 收束幕）内改经非阻塞诊断口**（§2.2 诊断口纪律——R3 对策主案）；备选（主案受阻时用）= 稳态拍诊断行仅收 PAUSESV1 后打——覆盖不完全（up/saved/restored 行仍在敏感窗），仅降级形态。单依赖 berry-sys → 单依赖 berry-libc（TAG 常量族/12B 头手排/`#[panic_handler]` 全数退役——双源镜像消亡，svcmgr 侧常量升唯一权威）。
3. **资源复检**：std 运行时入链后 stack_pages 预算重验（现值 4——stdhello 量级参照；launcher 8→64 页穿保护页先例 frameloop.rs :37-44 为上界警示，apptomb 无 berry-ui 链不需同量级）；mkapp 金件 stack_pages 字段随实测连动；满帽 state_restore buf 65504B > 栈 4 页 16KiB——buf 落 static/BSS 不入栈（§3.2 读侧契约的栈面复检项，launcher 穿保护页先例同警）；sleep 面经 libc `nanosleep` → 底层同 `berry_thread_sleep`（sys.rs :114-135）——SLEEPQ 无 wait_signals 形态不变，FROZEN 判据链零漂（首验点 R3）；EOF/`exit(0)` 同义（io.rs :844-847 已含 send_stdout_eof）。

**不迁移清单（刀2 结构性不动面，验收差分的负空间）**：svcmgr **零行为改**（帧面/记表/relaunch/ECO 行族零漂；注释面唯一例外 = §8.3 R5 两处修订）；markers.sh/smoke-image/规则/07 零触碰；APPTOMB 打包位 selftest/ 专用位与 manifest id 零改；boot 批 app_launch/SPAWN RPC 装配面零改（零义务律的面侧根据——§2.3 验证锚由此成立）。

## §5 GUI 消费面（泵点位契约，本刀不验收）【v2 预埋】

- **结构性张力（本节立法对象）**：F2 冻结载荷条件禁 lifecycle 句柄入 wait 集（§1）⇒ 未来 GUI lifecycle app 的帧循环（launcher 形 = `wait_many([wm, input], deadline)` 直调，frameloop.rs :563-579）**收不到通道可读唤醒**——PAUSESV1 到达时 app 睡多深、poll 间隔就多长；launcher 的 IDLE 帽 10s 与 pause deadline 10s 同值 = **投递余量恰为零**。
- **泵点位契约（对 SDK 文档面预立法）**：①每帧 `wait_many` 返回后插 `lifecycle_poll()`（返回后泵不违反 F2——泵不在等中）；②**注册 = 同意 deadline ≤ poll 周期上界**：注册回调的 GUI app 须把帧循环 deadline 上界压到 poll_cap（v1 立法契约、**值不定标**——帧率 60fps 形退化时仍 ≤1s 量级，定值挂 v2 GUI 冻结刀首验点）；③idle 长睡档 = 帧循环 deadline 与 poll_cap 取 min（IDLE 帽让位的形态面随 v2 obligations 刀裁）。
- **边界诚实注**：GUI 冻结本身在范围外（§0 不做表第 2 行——obligations 谓词未落、extras 8/8 满）；本节只把「未来 GUI app 接三口时的泵点位纪律」先行成文，防 SDK-2 对外面承诺「自动送达」——**送达语义 v1 = 显式 poll、永不自动**。

## §6 验收面（host 测 + QEMU 零漂承诺）

**验收矩阵（刀 × 面）**：

| 刀 | host | QEMU | 文书/连动 |
|---|---|---|---|
| 刀1 | `berry-lifecycle` 全测（下条） | smoke 全表零漂（零新面承诺——新面无调用者，链接面不拉入） | test-host 追行、索引/沿革 |
| 刀2 | ——（消费者面无 host 可测） | FROZEN/TOMB/APP 族全表零漂（含 appquiet absent 探针，承诺⑧）+ 首跑墙钟记录 + SMP 相（§6 末条——markers 单源结构性零改，实跑钉板归刀3） | mkapp 金件连动、justfile 换链、svcmgr 注释修 R5 + grep『对端权威』零残留、负路径人工面落册 |
| 刀3 | —— | `just smoke-smp` 实跑钉板（v1.2 §8 挂账件） | 规范/全景/ADR/索引/沿革五件套 |

- **刀1 host 测（`berry-lifecycle` 纯 crate，just test-host 追一行）**：32B 头编解码 roundtrip；hlen=12 旧头 v0 路径分派；hlen=32/ver=1 正路径；坏档四型（ver 超前/hlen 异值/len 超界/crc 不符）→ BadFmt；counter 翻转域外钉板——金件 blob 翻转 counter 单字节 → 仍 Restored(n=翻转值) 非 BadFmt（钉死「CRC 不护头」契约〔§3.1 边界注〕，防后人好心扩域破 v0 前缀兼容；「由对账红」半属 svcmgr QEMU 面，不进 host 断言——由刀2 零漂承诺 + 现役 rctr 臂承接）；CRC32 校验值 `"123456789" → 0xCBF43926` + 金件 blob 逐字节快照；前缀 12B **布局同位**两条断言——(a) MAGIC_OFF=0..4/COUNTER_OFF=8..12 offset 常量断言（ver@4/hlen@5 跨版本变值 0→1/12→32，不属逐字节等值域）；(b) v1 头（ver=1/hlen=32）喂 v0 白名单读者（ver=0∧hlen=12）落 badfmt 臂——§3.2 末行安全红的 host 侧对称臂，与坏档四型互补；payload 帽边界（64KiB-32B 过/拒两臂）；读侧 buf 两臂（足帽过/不足帽拒——§3.2 读侧契约）；三分型状态机全转移；8B 帧 roundtrip 逐字节断言（tag u8 + pad 3B 恒 0 + counter u32 LE）；counter 代持全序转移断言（UP c=0 / RESTORED c=N+1 / RESUMED c=当拍 / save 写头 = 当拍值）。
- **刀1 QEMU 承诺**：**smoke 零新面**——新增面无调用者（berry-libc 新模块未被任何现役 bin 引用，链接面不拉入；appquiet absent 探针归刀2 追加，本刀「无调用者」字面成立），rootfs 相**实跑**全表逐行对拍（FROZEN/TOMB/APP n=9/APPUI/BSH/THR/FRZ n=2 等）+ SMP 相 markers.sh 单源**结构性零改**（双数组零改 ⇒ smoke-smp 相 2 轮询面构造性零改；实跑钉板归刀3——§6 末条/§8.1 刀3）；mkapp 金件不动。
- **刀2 QEMU 承诺（判据链逐环零漂——B 侦察③清单收编）**：①消息面：8B 帧 + TAG 值表 + counter 对账（saved_c ↔ restored c=N+1）；②授予面：extras 绝对位 [4]/[5]、state=dup(R\|W)、64KiB commit 全幅；③blob 面：BTMB magic + counter offset 8..12 续接（头 12→32B，svcmgr 首读面零改）；④退出面：写成功才发 SAVED → EOF 先行 → exit(0) → reap 真码 0；⑤行为面：两拍循环 + 绝对时刻心跳 + **禁阻塞等任何 handle**（通道面〔帧发送纪律〕+ console 写面〔诊断口纪律〕双禁——§1 冻结载荷条件「对任何 handle 的阻塞 object_wait 都触发」口径，R3 对齐）；⑥标记/打包面：`FROZEN ok n=1`/`TOMB ok n=1` 字面零漂、selftest/ 专用位 cpio 直查、APP 族分母零特判不动——svcmgr 本刀**零行为改**（注释面 R5 两处例外，§4）⇒ 其 ECO 负路径行族 14 条（tomb guard/svmo/spawn/up/susp/win/resume/res/ptmo/rcode/nosave/respawn/rctr/rfail）**结构性零漂**；app 侧 console 诊断行（badfmt/saved/beat 族）字面随库化演变——console 辅证非判据（H1），不进 smoke grep 面；⑦钟面：deadline 起算 = appd 发出时刻——libc 侧无第二执法钟（§2.2 无查询口立法）；⑧零义务探针：appquiet（console=false 零漂锚、无 fs 词）main 启动期单次 `lifecycle_poll()` 断言返 0 + `lifecycle_register(None)` 断言返 Absent + `state_restore(&mut buf)` 断言 Fresh + `state_save(&[])` 断言 Absent（buf 取满帽 static——§3.2 读侧契约）——成功面 absent no-op 无帧无输出，APP ok n= 零漂、零新标记/APP 族分母承诺不破；失败面自动 loud（assert 炸 → abort(134) → reap 非 0 → APP ok 缺失——appquiet 头注自述机制；误发帧 → 对 h=0 send 失败 → 库 dead park 同红）。
- **fs 词面验证锚（结构性测不到的诚实注）**：fs 词 musl 型 app = 在场判据 UB 面（§2.3 首条）——现役 APPDEMO 即现成载体（manifest `fs=["rootfs"]`、boot 批拉起），smoke 零漂形态测不到该面（不调即不红）；真探针须显式调 `lifecycle_register` 断言返 Absent 而非 Ok（假在场）+ `state_save` 不触发 dead-park——依赖 v2 absence 判据重构（§2.3 挂账），v1 以「SDK-2 主循环模板只随 lifecycle/tomb spawn 面分发」文档化收窄 + APPDEMO boot 批 smoke 零漂为弱锚。
- **canon 纪律**：本刀**零新标记**立法（三口是封装迁移不是新谓词）——markers.sh/smoke-image/规则/07 三消费方零触碰；若刀2 证据面迫使加标记，过 103 字面 grep -F 全表互查 + `APPTOMB ok n=1` 含 `TOMB ok n=1` 子串不可用（v1.2 §6 纪律原样）。
- **负路径人工面（挂账，CI 结构性不可达）**：①v1 头喂 v0 读者 → badfmt 红（TOMB FAIL loud 安全降级）——同刀升级后线上不可达，负样本变体验证；②cb 内不调 `state_save` 的零写路径 → appd ptmo 族降级 + 不入表；③`state_save` 越帽 payload → TooLarge 前置拒 + VMO 定尺寸结构性执法双层；随 v1.2 负路径五件人工面同册记录。
- **SMP 面义务**：刀2 收官跑 rootfs 相全表对拍足证迁移；`just smoke-smp` 实跑钉板归刀3 文书收官（markers 单源自动继承——v1.2 §8 挂账件收口，余件仅本项〔resume 注/DEADLINE 核账已随沿革一六九先行闭合〕，§8.1 刀3）。

## §7 记账与帽（全族对账）

| 帽 | 值 | 状态 |
|---|---|---|
| state VMO（blob 帽） | 64KiB 定尺寸（结构性执法）不变 | 现役；payload 域 = 64KiB - 32B |
| blob 头 | **12B → 32B**（hlen 扩展追加，前缀 12B **字段布局同位**——magic/rsv/counter 段不动；ver@4、hlen@5 按版本变值） | 本刀立法；业界水位注随 v1.2 §7（64KiB v0 值，v2 按 intent 档复核） |
| CRC32 | poly 0xEDB88320、覆盖 payload 全幅 | 本刀立法；纯 crate 内嵌 + png.rs 镜像互指 + 0xCBF43926 钉板 |
| pause deadline | 10s、起算 = appd 发出时刻（单钟） | 现役不变；v2 分档携法预埋：PAUSESV1 下行 [4..8] 位（现恒 0「暂停令无参」:6035 可随立法修）——下行 counter 位**无消费者**（apptomb 收 PAUSESV1 即走 save_and_exit、用本地拍 counter，不读 [4..8]）⇒ 复用 [4..8] 零帧格式改动；帧 counter 断言**只在 RESTORED 方向**（SAVED 帧仅采集 saved_c、续接校验在 relaunch 臂 c=saved_c+1——与 §6 承诺①「saved_c ↔ restored c=N+1」口径一致）；新 tag/16B 帧形需验 recv 长帧 nb 语义，风险面不取【v2 预埋】 |
| 回调注册表 | 1 槽（覆写式） | v1 单回调；多回调/优先级 v2 另裁 |
| 回调 payload | `state_save` 入参 ≤ 64KiB-32B（TooLarge 前置拒） | 本刀立法 |
| state_restore buf | ≥ 64KiB-32B 满帽（**满帽拒读**——写侧 TooLarge 的读侧对偶；不截断不半档） | 本刀立法（§3.2 读侧契约——读侧 buf 两臂 host 钉板） |
| 帧发送错码 | 三分：-EBUSY 非阻塞自旋重发 / -EAGAIN 不可达断言 / 他错 lifecycle dead park | 本刀立法（§2.2 帧发送纪律——禁 wait_one(lifecycle_h,·)，F2 send 面落地） |
| GUI poll 周期上界 | 契约面 ≤ poll_cap（**值不定标**） | §5；定值挂 v2 GUI 冻结刀首验点 |
| apptomb stack_pages | 4（现值）→ 刀2 实测复核 | std 运行时入链重验；mkapp 金件连动 |
| smoke 墙钟 | 刀2 零增量（同拓扑同帧数） | 刀3 核账；smoke-smp 实跑钉板顺路 |
| TAG 值表 | 1..7 封闭（未知 tag 忽略续跑——apptomb :252-254 前向兼容注由本刀承接） | 现役；tag 8+ 分配挂 v2 真触发刀 |

## §8 刀程（三段，各段独立可验可提交）+ kill criteria + 风险首验点 + 文书

### §8.1 刀程

- **刀1（三口纯面 + 库接线）**：`berry-lifecycle` 纯 crate（头编解码/CRC32/分派/三分型 + 8B 帧编解码 + counter 代持转移 + host 测）+ berry-libc `lifecycle.rs` 三口/机械件 + absent no-op 面（`lifecycle_h()==0` 全降级——清单 = §2.3 五件：register/poll/save/restore/beat）+ justfile test-host 追行。验收：host 测全绿 + QEMU smoke 全表零漂（零新面承诺）。回滚粒度 = 单 crate + 单模块，无消费者。
- **刀2（apptomb 迁 libc 路）**：§4 三步（换链/换 API/资源复检）+ appquiet absent 探针一步（§6 承诺⑧）+ TAG 双源镜像消亡（svcmgr 常量升唯一权威）+ §8.3 R5 注释修（两处）+ mkapp 金件连动。验收：`FROZEN ok`/`TOMB ok`/APP 族全表零漂 + 首跑墙钟记录 + svcmgr 侧 grep『对端权威』零残留 + 负路径人工面挂账清单落册。
- **刀3（文书收官 + v1.2 挂账顺路收口）**：规范 §4.1-5 状态注（「随 app-1 落地」→「app-2 已落地」+ 三口签名速查）+ 应用API全景-v1.0 §7 速查行翻绿 + 草案 v1.2 §5 铅笔兑现追记 + v1.2 §4 头布局注退役追记（「v0 头布局为自验件试验格式、非平台 ABI」→ 平台 ABI 随本文 §3.1；12B → hlen=32）+ v1.2 §7 帽表行「blob 头 12B 定长」追记（v1 起升 32B——app-2 §3.1）+ SDK-2 前置输入挂账行落册（§8.4，与规范状态注同批）+ ADR 追记（#64：三口首个消费者 + F2 约束继承注；#42：freeze 级消费位补 libc 形）+ 索引/沿革 + `just smoke-smp` 实跑钉板（v1.2 §8 挂账余件——resume 复用注/DEADLINE 核账已随沿革一六九〔3eb481e〕先行闭合，2026-11-10 对账）+ 规则/06 瘦行（若判例成）。

### §8.2 kill criteria

1. **K1**：刀2 后 TOMB FAIL 不可归因 → 分刀本身体现（刀1 纯面 host 钉板在前——头面 + 帧面〔8B roundtrip/counter 代持全序〕+ 三分型全测，库胶水侧已收缩至句柄获取/syscall 收发/幕序编排，承诺①判据链首环闭合——刀2 红即消费者侧/链侧二选一）；刀2 **间歇红**先查 console 写面假红源（§8.3 R3 第二审计项：svcmgr 退避预算 100ms×40=4s < console 阻塞帽 10s/臂——冻结尝试撞持续满队 = susp fail 假红，非迁移回归），防误触整刀回滚；再不稳 → apptomb 回退 hello 型（git 单刀回滚）+ 备选形态升主案（§4 另立 musl 自验件，成本 +1 bin +1 标记）。
2. **K2**：musl 链 berry-libc 链接面不符（berry-libc shim + berry-ld 剥 musl crt 链接面与 std 链冲突）→ apptomb 保留 hello 型现役、三口以「库面在册 + 首消费者另立」形态收刀（SDK-2 前置位不空转）。触发观察点三处：①justfile build-user musl 编译段（berry-libc shim 链接 + berry-ld 剥 crt，FAIL = 链接期红）；②staging cp ELF 本体 → mkapp 打 .bap（门禁五道 + 金件快照连动）；③tomb_selftest spawn（ECO 负路径行族 spawn 项）。依据注记：apptomb 装配径 = cp ELF 本体直进 staging → mkapp（justfile 同段），**不经 elf2flat**——mkapp 门禁第 5 道要求 `\x7fELF` 魔数 = elf2flat 平像产物反被拒，二者结构性互斥；elf2flat 消费全在 none-target 平像族（echo/talker/thrchild/dpgp/abuser/svcmgr；hello 亦 ELF 本体直进 cpio）。
3. **K3**：CRC 覆盖域/性能争议 → hlen 扩展序保前缀字段布局不动，追加域再扩展（v2 头 → hlen=48+ 形），读写两侧同刀律不变。
4. **K4**：GUI 泵点位张力（§5）在 v2 实测恶化（poll_cap 与帧率冲突）→ poll_cap 定标提前 + B 案五点旁挂增强档提级——证据驱动，不预注册。
5. **K5**：appd 侧「未注册回调 + PAUSESV1 → exit(0) 无 SAVED」慢降级（10s 超时才杀）成真策略痛点 → appd 加「reap 早退快判」臂（通道对端关闭语义先行验证）——挂 v2 真触发刀，本刀不动 svcmgr。

### §8.3 风险与首验点

- **R1（同刀升级约束）**：v1 头写读两侧同二进制同刀——host 测 const assert 钉死前缀 12B **布局同位**（MAGIC/COUNTER offset 常量断言——ver@4/hlen@5 跨版本变值，非逐字节等值域）+ `BLOB_VER`/`BLOB_HLEN` 单源常量（写读共源即同刀的结构性保证）。
- **R2（链/体积面）**：std 运行时入链后 apptomb 体积/栈预算首跑实测（stdhello 量级参照；mkapp 金件连动防静默漂移）；`CRT0_SEED` 防 LTO 丢 crt0（sigchild 先例）。
- **R3（冻结判据连续性）**：迁移后 sleep 经 libc `nanosleep`（同底 `berry_thread_sleep`）——SLEEPQ 无 wait_signals 形态零漂的机器验证 = 刀2 首跑 `FROZEN ok` 即证；若红，先查 libc 面 PEND/EOF 残留态是否引入阻塞 object_wait（read_inner :713 有 wait_one——**stdin 面**：apptomb stdin_feed=Some(&[]) 零长 EOF 恒在，read 不阻塞——契约面随迁移保真，首验清单列明）。**第二审计项——console 写面**：libc 形 console 诊断行全数经 write(1)→`console_write`（io.rs :446-483），EBUSY 时 `object_wait_one(CONSOLE_H, WRITABLE, 10s/臂)` 阻塞重试且命中后重臂（:479）＝ stdin 面之外**第二处阻塞 object_wait**；stdin 有零长 EOF 先行保证（read_inner 不阻塞），console 面无对应保证、满队源在册（v1.2 §3「TCG console 满队瞬态窗」）；svcmgr 退避预算 100ms×40=4s < 阻塞帽 10s/臂——冻结尝试撞持续满队即 susp fail 假红（间歇 `FROZEN FAIL`）。对策主案 = §2.2 诊断口纪律 + §4 步2（敏感窗诊断行改非阻塞口）；§8.2 K1 已注记此假红源。
- **R4（canon 复核）**：刀2 收官 grep -F 全表互查（零新标记承诺的机器验证）；ECO 行族字面对拍。
- **R5（双源镜像消亡的回归面）**：TAG 常量从 apptomb 侧删除后，svcmgr 侧同款旧注**两处**同步修——:5696-5697（TAG 族注「apptomb const 块为对端权威」）与 :5721-5722（头布局注「12B 定长…与 apptomb const 块同源镜像（对端权威）」）→「唯一权威」；头布局注另加格式追注（「12B/ver=0/hlen=12 为 v0 档，v1 hlen=32 读者按 hlen 分派——app-2 §3.2」）。只修前一处则防回归目的未竟（同款注两处在册）；验收面 = 刀2 grep『对端权威』零残留（§8.1）。

### §8.4 ADR 与文书清单

- 规范 §4.1-5 状态注 + 应用API全景 §7 速查行 + 草案 v1.2 §5 追记 + v1.2 §4 头布局注退役追记（「非平台 ABI」→ 平台 ABI 随本文 §3.1；12B → hlen=32）+ v1.2 §7 帽表「blob 头 12B 定长」追记（v1 起升 32B）（刀3）。
- **SDK-2 前置输入挂账（刀3 落册——2026-11-10 已落册，与规范 §4.1-5 状态注同批）**：§5 泵点位契约三条（①每帧 `wait_many` 返回后插 `lifecycle_poll` ②注册 = 同意 deadline ≤ poll 周期上界 ③idle 取 min）+ §5「送达语义 v1 = 显式 poll、永不自动」承诺 + §0 术语注机械件主循环模板形态 + §2.3 absence 判据重构挂账（fs 词 UB 面 v2 收窄——env 打标/专用槽）——随 SDK-2 文档面/主循环模板执法，SDK-2 开刀输入清单必读。
- ADR #64 追记（三口首消费 + F2 继承 + counter 库代持立法）；ADR #42 追记（freeze 级消费位补 libc 形注）。
- 索引/沿革五件套；mkapp 金件连动（刀2）；规则/06 瘦行（判例如成）；负路径人工面记录册（随 v1.2 五件同册）。

### §8.5 方法注记

- v1.0 立稿 = 两路侦察摘要（A：libc/框架面 dispatch 候选对比与推荐案；B：协议/验收面事实书——TAG 值表/帧格式/授予形/兼容矩阵/零漂清单）+ 本会话直读补核（svcmgr tomb 面/apptomb 全文/berry-libc io+lib+signal/sched freeze_precheck/launcher frameloop/markers/smoke-image/justfile/mkapp 金件/规则/07/沿革一六五·一六六/调和书 D-H 系——侦察摘要中「D5」编号经核调和书无此条目（D 系在册 = D1-D4/D6-D11），相关结论按 H 系①/D8/D9 原号引用）。
- 行号口径：本文全部行号 = 2026-11-08 基线（HEAD 6743f2f）实测，随刀漂移以函数名为锚（app-1 惯例）。
- 业界连线：UWP Deadline 因设备而异 + GetDeferral 仅延迟不追加时间（MSIX 底稿 §3.1）→ BerryOS 单钟 + 同步回调的简化合法性根据；iOS Scene 生命周期词表（Apple 一轮底稿 :39——`sceneDidEnterBackground` 族框架回调）→ 「框架代收、应用只写业务」的谱系根据（「框架代序列化」为本文概括语非底稿原词）；Android 无对应面（SavedState 框架代收 + blob 边界 = v1.2 §13 连线延续，不重复引）。
