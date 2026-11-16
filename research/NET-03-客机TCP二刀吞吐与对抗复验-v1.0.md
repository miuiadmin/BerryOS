# NET-03 客机 TCP 二刀吞吐：发送窗消费、多段在途与 RPC 多段化——实施与对抗复验证据链（2026-11）

> 版本：**1.0**（v1.0）｜2026-11-16｜文件名 `NET-03-客机TCP二刀吞吐与对抗复验-v1.0.md`
> **定位**：[二刀 plan](../.agents/plans/2026-11-16-客机tcp二刀吞吐.md)（承 [NET-01](NET-01-客机TCP栈深研与裁决-v1.0.md)/[NET-02 首刀](NET-02-客机TCP首刀落地与对抗复验-v1.0.md)）的实施波证据链——参数裁决落地（含 MSS 不抬的再诠释）、吞吐前后对比与模型对账（验收线未达的如实记录与根因实证）、对抗复验数据、八 op 语义域保持取证；另载 NET-01 §5.1-1 量级结论的增量修正与四刀路线边界。
> **证据分级**：沿用 NET-01 口径——【证】= 本波实跑输出/一手行号；【走】= 走查；【界】= 工具边界。
> **代码面现势（先读）**：二刀五件代码未随任何 commit 落库——08:01 rebase/stash 风波卷离后已恢复：tag `tcp-knives-1-2-backup` 钉悬空快照 `4e638bd6b`（等价件 `078d911df`/`ca4fad911`），五路径 checkout 回树与快照逐字一致，berry-tcp 51/51 + netdproto 43/43 复验绿（2026-11-16 三路复跑）；落账归 owner、恢复实况与核对细节见 §7（本篇行号与门复跑仍锚快照态，fix 波增量见 §7 末条）。

---

## 0. 总判断

二刀按 [plan](../.agents/plans/2026-11-16-客机tcp二刀吞吐.md) 落地：RPC 载荷帽 TCP_DATA_MAX 128→250（inline 几何恰满）、MSS=128 不动、在途环 TX_SLOTS=4 定槽 FIFO（Syn/Data/Fin 三形统一）、Go-Back-N 队头重发 + 部分 ACK 弹槽、发送窗消费（snd_wnd 记账 + 零窗停发）、重传载荷源 Sess.tx 暂存（修 trap 面真重发）。宿主门全绿（berry-tcp 51/netdproto 43/netproto 24 + harness 19/19 含 4 新臂）+ 五标记链正牌全绿。**吞吐验收线未达（如实记差）**：客户端 4KB 最佳 706 B/s < 850 门，交错同窗基线对拍 1.23× < 1.8× 门，服务端 16KB 最佳 547 B/s 逼近 550 未稳跨——根因实证为 QEMU SLIRP 对 guest 侧通告窗 128B（snd_wnd=128，在途恒帽 1×MSS），多段在途逻辑在 harness 合成窗全绿但在活链无窗可用，模型「250B/op 单 RTT」前提在 SLIRP 域不成立。能力面提升真实：压满准入两栈对照（基线 4KB 突发即 wedge vs 二刀 DRAIN 收口）+ 多段在途/部分 ACK/close 排队/零窗停发四能力钉板。**代码面未落库**：五件已 checkout 回工作树（与 tag 快照逐字一致，见 §7）待 owner 落账。

## 1. 参数裁决落地：MSS 不抬的再诠释

**再诠释推理链**（对任务书三合取③「MSS 抬升」的改判，四步全【证】）：

- dlen 单字节编码域顶 255（netdproto encode `as u8`）；通道 inline 帽 chanface `CHAN_MSG_MAX=256` 单源，会话形请求 `[op][dlen][data][sid u32]` = 6+dlen ≤ 256 ⇒ **dlen ≤ 250**（恰满零浪费）——「dlen 域顶 255」在通道几何下不成立。
- 串行 RPC 下吞吐 = 每 op 载荷/RTT，与分段数无关：MSS ≥ 250 使 ROUNDTRIP 退回一段/op（in-flight>1 结构性不可达）；MSS ∈ [128,250) 段数同为 2（ceil(250/128)）吞吐无差；MSS < 128 只增段数与 40B/段头开销；MSS 536/1460 在 250B op 域零吞吐收益反吃 vnet 栈预算（SEG_BUF_SZ/scratch 随动）。
- 裁决：**抬升对象 = `TCP_DATA_MAX` 128→250，`bt::MSS=128` 不动**——这是对「MSS 抬升」的再诠释而非违约：三合取的吞吐意图由载荷帽承载，MSS 真抬升（op 载荷基数 >250 的流水线波）记边界待四刀。
- 机器钉四件套（netdproto 快照 L89-103）：`CHAN_MSG_MAX=256` 镜像常量（kernel 单源同值注）+ 断言族 `6+TCP_DATA_MAX ≤ CHAN_MSG_MAX`（恰满式）/`REQ_LEN_MAX ≤ CHAN_MSG_MAX`/`TCP_REP_MAX ≤ CHAN_MSG_MAX`/`REQ_S_LEN_MAX ≤ CHAN_MSG_MAX`——超帽即编译期炸响，后续抬值先过几何；vnet 双向几何钉（tcp.rs 快照 L96-97）：`TCP_DATA_MAX > bt::MSS`（多段钉，单侧抬 MSS 即炸）+ `bt::TX_SLOTS × bt::MSS ≥ TCP_DATA_MAX`（环容量钉，缩环/抬帽即炸）——旧钉 `TCP_DATA_MAX == bt::MSS` 退役。
- 头注契约句改写（netdproto 快照 L34-36）：TCP_DATA_MAX = RPC 单 op 载荷帽（inline 通道几何定值），与 berry-tcp MSS 解耦——TCP 分段归 vnet/berry-tcp 发送面，回显严格等长但可多段。

**在途结构落点**（berry-tcp 快照行号）：`TX_SLOTS=4`（L34；一段 250B op 拆 2 槽 + close 排队 FIN 1 槽 + 余量 1）；`TxSlot` 三形定槽环替换 InFlight 单槽（L141-156）；send 拒面四形——非 Established/len 越域/环满/越窗（L280-299）；close FIN 排队尾不再覆盖（L304-320）；on_rto GBN 队头重发/到帽清环（L351-372）；ACK 消费 snd_wnd 记账（一切携 ACK 段）+ 覆盖槽从环头弹出（跨段覆盖一次弹多头、straddling 保槽）+ 进度归零 backoff（L425-450）。

**重传载荷源**（vnet tcp.rs 快照）：Sess 增 `tx:[u8;TCP_DATA_MAX]+tx_len+tx_seq0`（L142-144）——op 首段发送时整批拷入，重发按 `(seq−tx_seq0, len)` 切片组帧；act Data 臂双面（初次拷暂存/重发切暂存，失配/无源/越界 trap loud）；step 与 pump_timers 两处 RTO 路径由此获得真载荷（旧 `act(ev.action, &[])` 的 trap 面消除）；单臂语义升级「覆盖全环最晚段尾」（arm_extend 只进不退）。roundtrip/send_data 按 MSS 分批入环、环/窗阻塞时泵等位（roundtrip L290-、send_data L1032-，等位帽 SEND_CAP_NS 30s）；三处 scratch 换 TCP_DATA_MAX 幅（connect/roundtrip/shutdown）；自验面零改动（PATTERN 64B 单段，`TCP ok echo=64` 字节面零漂移）。

## 2. 吞吐前后对比与模型对账

**模型自算**（plan）：N=4×M=128、P=250 ⇒ 250B/RTT ⇒ **951 B/s@263ms**（全域 532-2016 随 RTT 分布）；模型校验今值 487@263ms 与十四波实测 486 吻合（误差 <4%）。

**实测对比**【证】（环境：macOS 宿主 TCG 单核、并行重载窗 load ~6.8、kernel 81057ef9-dirty debug；数字为重载量级证据非基准成绩，对拍必须同窗——本波方法论为交错窗基线对拍）：

| 面 | 二刀链 | 基线链（HEAD 全栈，两链唯一差异 = TCP 栈件） | 判定 |
|---|---|---|---|
| 客户端 4KB | 六样本 706/587/558/547/489/427 B/s（run1/v1b×2/echo2a/run2×2） | 488/446 B/s | 最佳 706 < 850 门；交错窗（v1b 07:33 vs base 07:19）比值 1.23×（v1b 均值 572.5/基线 467）< 1.8× 门 |
| 客户端 16KB | 675/574/459 B/s | 466 B/s | 比值 1.23×（574/466）< 1.8× 门 |
| 服务端 echo 4KB | 403/275 B/s（echo=OK） | 突发即 wedge（timeout+RST）无数字 | 计划口径基线 309 仅参照 |
| 服务端 echo 16KB | 547/262 B/s（echo=OK） | 同上 | 最佳 547 逼近 550 门未稳跨 |

**根因探针**【证】：二刀链 128B/op 均 273ms（min 131）与基线 262ms（min 114）同域 = wire 路径零回归；129B/op 均 423ms ≈ 2× 128B（第二段哪怕 1B 恒等首段 ACK 后才发）、250B op 均 ~410-490ms——**活链 snd_wnd=128 实证**：QEMU SLIRP 对 guest 侧通告窗 128B，在途恒帽 1×MSS。250B/op 双 RTT 串行下每 RTT 字节 125 ≈ 基线 128，净得仅来自 op 固定开销摊薄（250/0.45 vs 128/0.273 ≈ 1.2×），与实测 1.23× 吻合。结论：TX_SLOTS=4 环/GBN/部分 ACK 逻辑全对（harness 合成窗 ≥250 全绿），活链无窗可用；四刀重开项 = 窗协商（WScale）/通告窗联动或 op 间流水线（后者独立于对端窗）。

**服务面容量对照**【证】：基线链 4KB 突发即 wedge（advcli thr timeout + 后续连接全 RST，首刀收缓冲容量面）vs 二刀链 4KB/16KB 突发 DRAIN 收口 echo=OK——两栈容量差异本身是能力面证据。

## 3. 对抗复验数据

- **段级 harness 19/19**【证】：h01-h15 旧臂零漂移 + h16 双段在途 GBN 队头重传 + h17 跨段部分 ACK 弹槽（straddle 保段）+ h18 close 排队数据环后 + h19 窗限在途/环帽压满准入。工具边界承 NET-01：段级丢/乱/重/迟对 guest↔SLIRP 半段结构性不可注入——段级对抗只在宿主 harness 域，客机面限应用层字节几何【界】。
- **客机毒臂**【证】（私有高位口 fixture 19601-19607/19699/19501-19502 + echod 19180，进程自起自收）：drop:3 臂 386 B/s、dup:2 臂 405、short:2 臂 461、rst:2 臂 284（均 bad=0 close=OK 零死锁）；hold:12 迟回显 20 B/s（2 趟 ~12s RT 恰如设计）；stall hold:70——rt1 TIMEOUT 60s 帽 → rt2 got=0B（迟到字节零错配干净归属）→ poll 净空 → close OK 2365ms（TIMEOUT 可恢复可复打）；C1 压满 10 并发/8 槽 connected=10/10；rstopen（无 Close 泄漏臂）st=OK 462ms；毒臂后 muxstat 全表 NOSESSION 零泄漏。
- **大回显两形态**【证】（逐字节对拍，SLIRP 理通告/不理通告两形态全覆盖）：形态A 250B 单写 thru=303 B/s echo=OK（first_echo@0.56s）；形态B 256B 2×128 拼段 thru=230 B/s echo=OK；另 512B 样本 292 B/s echo=OK。
- **编排教训**（复用 fixture 必读）：legacy 单会话在席 OPEN 恒 BUSY → rstopen 泄漏臂必须最后跑；会话槽闭连接缓慢回收（boot 自验残留 + 序贯连接渐耗 8 槽 → 新 SYN 拒 RST，ready_left=0 现象）——属 NET-01 §5.1-3 三刀「槽空闲超时」既挂账域，非本刀回归，量测规避 = 新鲜 boot + 首连接形 + 连接间 settle；hostfwd 裸 connect 假阳（SLIRP 缓冲）→ 就绪探询必须功能形（发收回显）。
- 证据原件：`/tmp/berry-adv/verify/`（38 日志 + measure/probe 脚本 + 私有 rootfs）与 `/tmp/berry-adv/sm-harness/`。

## 4. 语义域保持（八 op 红线）与门面

- **八 op 对外语义零改动**【走】：op 集合、请求/应答字节形状、恰长执法、尾随 session 编码、v1 session-0 字节、烘焙线（非 OK 恒不携 data/sid/addr）全部不动——netdproto diff 仅头注契约句 + CHAN_MSG_MAX 镜像 + 断言族 + TCP_DATA_MAX 定值 + 1 测，parse/encode 逻辑零触碰（L223/257/291/315 门随符号自动跟随）。
- 唯一变量：TCP_DATA_MAX 容量定值 128→250（接受域 1..=250）。互操作矩阵：旧客户端（≤128）对新服务端逐字节不变；新客户端 dlen ∈ 129..=250 对旧服务端 = parse 拒 → ST_BAD（树内双侧同批锁步，无混布面）。
- ROUNDTRIP 语义细化「发一批（≤250B，TCP 层内部分段）收等长回显」——分段对 RPC 面透明，回显仍严格等长，对拍归 client 不变。
- 判据翻新两处 = 语义域扩中的「可扩不可破」扩面：h06 零窗照发 → 零窗停发 + 窗更新放行（树内镜像 zero_window_blocks_until_update 同批）；close_overrides_data_inflight 改义 close_queues_fin_behind_data（旧「槽已换主」断言废除并注明）。
- **门面**【证】：berry-tcp 51（含二刀 8 新测 + 首刀 2 测）/ berry-netdproto 43（+1 恰满测 tcp_s_dlen_250_fills_inline_cap）/ berry-netproto 24 全绿；just test-host 197×ok 零失败；just berry-build docs_guard 0 警；check-docs 自有内容 0 告警；五标记链正牌 `just run-rootfs` 全绿——TCP ok echo=64（字节面零漂移）/ LSN ok n=7 / TCPS ok echo=64 / PIPE ok n=6419 / EVS ok n=2（`out/logs/run-rootfs-1794786581.log`，5/5 grep 实证）。
- **冻结面零触碰**【证】：berry-netproto / justfile / kernel chanface / berry-libc（P16 预期零 diff 兑现——net.rs 不在改动清单）/ user bins（bsh/tcptest/evsclient）均不在快照改动清单；diff 面严格限于 berry-tcp src+tests、netdproto src、vnet tcp.rs+main.rs（注释级）。

## 5. NET-01 §5.1-1 量级修正衔接

NET-01 §5.1-1 断言「三刀合取才进 10KB/s 域」（吞吐模型条），本波两段增量修正：

- **裁决期修正**（plan 偏差节）：三合取上限 = 250/RTT ≈ 0.5-2.1KB/s（十四波 RTT 分布 124-470ms）——VNC 10KB/s（@10fps）需 K ≥ 11@263ms（K ≥ 5@124ms）、30KB/s 需 K ≥ 32，且 K ≥ 3 即撞 WINDOW=512 通告节流（收窗联动成前置）。「三刀合取进 10KB/s 域」修正为「三合取进 KB/s 域，10KB/s 域需四刀 op 间流水线 + 通告窗联动 + 环扩（N ≥ 2K）」。
- **实测期再收紧**（本波 §2）：SLIRP 域 snd_wnd=128 下在途恒帽 1×MSS，连「250B/op 单 RTT」也需窗协商/联动才成立——串行 RPC 实测落 ~1.2×。四刀之前任何吞吐预期都应按活链窗 128 口径自算。
- NET-01 原文不改动（深研裁决稿的历史定位），修正由本篇与 plan 偏差节承载；VNC 前置链重排：三刀（服务面三项）+ 本刀能力面叠加后仍 KB/s 域，四刀才是量级刀。

## 6. 边界与四刀路线

- **不做面**（本刀明界）：快重传（dup-ACK 计数，SLIRP 即时 ACK 域结构性积不起三连）/ 按段退避 / RTT 自适应 RTO（恒 3s×2 退避）/ 乱序重组（SLIRP 代理域已重组字节流）/ 序号回绕 plain 比较、未来 ACK 越界接受维持已知限制 / 持续零窗无探针（窗更新丢失 = 相位帽 TIMEOUT；SLIRP 大窗域结构性不触发）——收益域均 = 真网卡，届时同批重开。
- **四刀重开项**（本波实证支撑，优先序建议）：① op 间流水线（libc 面改形，独立于对端窗——不受 SLIRP 通告窗支配，模型 K=4@263ms 即 3802 B/s，NET-01 §5.1-1 10KB/s 域正路）；② 窗协商（WScale）或通告窗联动（动态通告 = RX_SESS_BUF−rx_len，K ≥ 3 撞 512 节流的前置）；③ 环扩 N ≥ 2K（流水线 K op 需 2K 槽）；④ 通道帽 CHAN_MSG_MAX 抬升评估（kernel 属地——op 载荷 >250 的另一条路径）。四刀立项建议：①先行（与②解耦可并行验证）。
- 三刀服务面三项（泵节奏/槽空闲超时/请求序号，NET-01 §5.1-3）不受本波影响；槽缓慢回收 RST 现象已实证归属（§3）。

## 7. 代码面现势与恢复（重要）

- **事件与恢复实况**【证】：二刀五件（berry-tcp src lib.rs 577 行 + tests/state.rs 878 行、netdproto src、vnet tcp.rs 1438 行 + main.rs；与首刀三件两刀同裹去重共五件）在 2026-11-16 08:01 并行 rebase/stash 风波中被卷离工作树——风波后 HEAD 仍两刀皆无（netdproto TCP_DATA_MAX=128、berry-tcp 无 TX_SLOTS）。恢复已完成：轻量 tag `tcp-knives-1-2-backup` 钉住悬空 WIP 快照 `4e638bd6b`（08:01:20 stash 形，base 81057ef95；等价件 `078d911df`/`ca4fad911` 与之 TCP 内容逐字同），五路径 `git checkout 4e638bd6b -- user/berry-tcp user/berry-netdproto drivers/bins/vnet/src/tcp.rs drivers/bins/vnet/src/main.rs` 摘取回树（摘取即首刀+二刀两波同回），工作树五路径与快照 diff 零行（逐字一致，恢复后多路复核）。
- **恢复核对**【证】：锚点三钉在位（tests/state.rs 51 测、netdproto TCP_DATA_MAX=250 及 6+250 ≤ CHAN_MSG_MAX 断言族、vnet Sess.tx/tx_seq0）；berry-tcp 51/51 + netdproto 43/43 三路复绿（2026-11-16 恢复后复跑）；sm-harness h19 对活树 path 依赖 `tx_inflight`——树卷离即编译炸（loud 非假绿），是恢复态的独立旁证。四路 sweep 与恢复完整性对拍清单见 [两刀遗漏检查 survey](../.agents/research/2026-11-16-tcp两刀遗漏检查.md)。
- **待办（落账归 owner）**：五件与本篇、二刀 plan/note 均未 commit——落账前工作树是唯一载体（tag 只钉快照不保树，stash/reset/clean 风波即再卷），紧急落账建议（显式路径三批）在 survey 裁决节；tag 处置（push 第二保险或落账后退役）同归 owner。落账后重走 berry-build + 五标记链再取吞吐数字——本篇门面与量测结论均取得于快照态，恢复后以复跑为准（五标记链复跑与吞吐重取尚未兑现）。
- **fix 波增量（2026-11-16，survey F73/F77）**：恢复核对后树面新增两处可归因差异——vnet tcp.rs connect/shutdown 两泵注释补明「泵期交付字节被静默丢弃」取舍（行为零改动）、berry-tcp state.rs 补 F77 三臂（TimeWait 收 RST 第四臂 + 数据段捎带窗更新 + 缩窗限在途两新测，51 → 53 测全绿）；此后工作树 vs tag 快照的 diff 不再为零，恢复点核对基线以 tag 快照（51 测）为准。
