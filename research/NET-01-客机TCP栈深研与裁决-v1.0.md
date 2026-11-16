# NET-01 客机 TCP 栈深研与裁决：对抗实测、能力矩阵与 VNC 路线差距地图（2026-11）

> 版本：**1.0**（v1.0）｜2026-11-15｜文件名 `NET-01-客机TCP栈深研与裁决-v1.0.md`
> **定位**：客机侧 TCP 栈（berry-tcp 状态机 + berry-netdproto 八 op + vnet IO/服务面 + berry-libc 应用面）的对抗实测与裁决稿——回答「这套栈离系统内建 VNC 服务还差多少」，把机制能力转成三态矩阵（实证/走查/缺失）与可执行差距地图；不改任何代码与 ADR，裁决倾向供后续 design 波拍板。
> **方法**：三层证据——①宿主纯函数层段级对抗 harness（15 测，`/tmp/berry-adv/sm-harness`）；②客机在环对抗（私有构建 rootfs + 私有高位口 fixture 宇宙 + 字节级毒化代理 + 宿主对抗 client；boot 全链五自验标记先绿后打）；③静态走查（全部一手行号）。串口全量带时间戳留 `/tmp/berry-adv/serial.log` 与 `drv.log`。
> **证据分级**：【证】= 本波实跑输出/一手行号直接支撑；【走】= 代码走查结论（未在客机实测）；【界】= 工具边界（结构性不可测，如实记录不虚报）。
> **已知边界（先读）**：① 段级丢/乱/重/迟对 guest↔SLIRP 半段**结构性不可注入**（SLIRP 是 TCP 代理，终结 guest 传输层并重组字节流——宿主代理只能毒化应用层字节）——该域证据 = 宿主 harness 直喂状态机 + 走查；② RTO 重传臂同因（in-flight=1 单段 + SLIRP 即时 ACK，客机侧撑不出真丢段）；③ 对抗机为 TCG 多会话重载，吞吐数字是量级证据非基准成绩。

---

## 0. 总判断

**这套栈是「逻辑正确但模型极小」的停等 TCP：状态机在其宣告语义域内对抗全臂无证伪（harness 15/15 + 客机在环五标记全绿），但吞吐模型（MSS=128 + in-flight=1 + 串行 RPC）与 VNC 帧流差两个数量级（实测客户端 486 B/s、服务端 echo 309 B/s），且入站存在 >MSS 单段黑洞面（对端一次写超 128B，vnet 拒收且无重组，SLIRP 拼段形态下整连接死锁）。裁决倾向：维持状态机、extend 收发面（发送窗消费 + in-flight>1 + 接收按序重组/分段）、patch 服务面（泵节奏 + 槽回收政策 + RPC 请求序号）。VNC 服务在当前栈上不可承载——先做 TCP 三刀再谈。**

## 1. 对象快照与对抗环境

### 1.1 对象（一手行号）

- **状态机** `user/berry-tcp/src/lib.rs`（503 行）：10 态（L46-60）；定值 MSS=128（L28）/窗口恒 512（L30）/RTO 3s×2 退避 3 次帽（L32-36）/TIME_WAIT 2s（L36）/in-flight=1（L225-233）。主动 open L163/on_seg_synsent L317；被动 listen 哨兵 L181（RST 豁免 L261-262）/passive_open L199/on_seg_synrcvd L339；共路 on_seg_open L358（ACK 单调推进 L364-378、乱序/重复丢弃回当前 ACK L380-389、FIN L391-401、同时挥手 FinWait1 直入 TimeWait L397-398）；TimeWait 重 ACK L419；LastAck L431；on_rto L286-307；AcceptQueue 固定 2 槽 L449。
- **协议面** `user/berry-netdproto/src/lib.rs`（1409 行）：八 op（L59-65/L353-362）；st 域 8 值（L68-79/L365-375）；TCP_DATA_MAX=128=MSS（L87，每 op 恰一段）；TCP_SESSIONS_MAX=8（L379）；POLL flags（L382-384）。
- **IO/服务面** `drivers/bins/vnet/src/tcp.rs`（1243 行）+ `main.rs`（1214 行）：Sess L104；arm_settled wrapping 安全 L119-121；connect L210；roundtrip L242-278（收等长回显 + 本段 ACK 到才 Ok；超收防御 L269-271）；shutdown L283-307；step L319-389（deadline=min(相位帽,RTO,TIME_WAIT,表最早定时)；非匹配段路由进表 L381-383）；parse_gw_tcp L508（**入段 >MSS 拒收** L519-521）；route_seg L549-610（四元组反查；SYN×Listen 口走 ACCEPT_Q_MAX 门 L596 与空槽 L597 才 passive_open，否则**静默丢**；其余丢 L609）；pump_timers L615；pump_once L655；TcpMux listen L714/ready_on L738/accept L751/poll_of L784/rx_drain L834/send_data L858/close_sid L940（帽到亦释槽 L985）。serve L840-1080：空闲窗 = recv EAGAIN → **pump_once(0) 单帧** → wait READABLE 10s（main.rs L866-873，SVC_IDLE_NS L120）；POLL/DRAIN/ACCEPT/LISTEN **不收帧**；tcp_op L1146-1203（session-0 独占 ST_BUSY L1158-1161；OPEN 帽 20s/RT 60s/CLOSE 60s，main.rs L129-131）。
- **宿主基线**：berry-tcp 41 测 / netdproto 42 / netproto 24 全绿；test-host 1751；check-docs 0 告警（probe 复核沿用）。

### 1.2 对抗环境（本波新建，全 /tmp/berry-adv 不入仓；净面属地零改动）

- **私有三件**：`vnet-priv`（tcp.rs 出站口重映射 18080→19180、18081→19181 两行——绕开并行 smoke 占用 18080-18083 红线，状态机与 IO 零改动）；`bsh-priv`（fs 自验后跳链直进交互 + `tcptest` 内建——五模式 client/stall/rstopen/echo/muxstat 经 bsh 自身 netd_rpc 直驱，零 spawn 零 gift）；`tcptest` 独立件（rootfs 备用，本波最终走内建）。
- **fixture 宇宙**：19180 echod（干净）/ 19181 hostfwd→guest 18081（LSN 环回）/ 19182 hostfwd→guest 18082（EVS）/ 19501-19502 hostfwd→同号（echo server）/ 19601-19607 toxproxy 七毒臂（drop/delay/dup/short/rst/hold:12/hold:70）→19690 echod / 19699 闭合口。
- **宿主件**：`toxproxy.py`（probe 原型沿用）+ `advcli.py`（load/thr/rstmid/halfclose/zombie）+ `bootdrv.py`（FIFO 命令环 wait/send/tail/quit + 带时戳串口全量）。

### 1.3 环境战记（六轮 boot，对裁决有信息量）

1. 首轮私有镜像 svcmgr 报 `eco: cpio bad` 而宿主同版走查器全绿；以当前树 fresh 重取产物 + 正确 release profile 后消失（根因未钉死，疑似撞并行会话增量重build产物窗口——如实记录）。
2. **128M 帧边**：rootfs VMO 全量 commit，现行 stock 档已贴边；私有档 +364KB 即压垮晚期 spawn（dpgs/dpgn/bc/thr）。-m 192M 可解但触发现行树内核 >128M 内存面 bug（`Guest moved used index` + svcmgr 被 G1.5 击杀）。
3. **demo-face=off 下 vnet 定时器停摆**（`vnet: tcp 挥手超时 state=TimeWait`，echo 已过、2s 到期不醒）——idle 深停泊与 vnet 1ms 轮询步进的交互，demo-ON 不复现。现行树潜在回归线索，移交调度/idle 线。
4. 内核代差：旧内核内嵌 svcmgr 无 launcher/bsh 拉起段（结构性无 prompt）——内核与 rootfs 必须同代配对。
5. **bsh 自验链 u2 起确定性挂起**（与帧余量无关）——私有构建以跳链绕开（网络面五标记先绿，不改判任何自验结论）。
最终可用配置：现行树内核 + 128M + demo-ON + 私有三件 rootfs（34754560B）。

## 2. r1 对抗实验矩阵

### 2.1 段级（宿主 sm-harness 直喂状态机）——15/15 绿【证】

| # | 场景 | 结果 |
|---|---|---|
| h01 | SynSent 假 ACK/纯 SYN | 全忽略；正确 SYN+ACK 才 Established |
| h02 | 数据 ACK 丢→RTO 链 | 重发同 seq×3（退避 3/6/12s）→第 4 次放弃转 Closed |
| h03 | 重复段 | 不二次交付，回当前 ACK |
| h04 | 乱序 | 不缓存不重组；丢+当前 ACK；缺口补上后对端重传段按序交付——恢复完全靠对端重传 |
| h05 | 旧 ACK/未来 ACK | 旧无回退；**未来 ACK 被接受**（snd_una 越 snd_nxt，违 RFC793 §3.3——无上界检查，v1 已知限制） |
| h06 | 零窗对端 | 状态机不读 seg.window（全仓零消费），send 照发——发送面忽略对端窗口 |
| h07 | RST 各态 | 7 态全转 Closed；Listen 哨兵豁免 |
| h08 | 主动关闭全臂 | FinWait1→2→TimeWait；重复 FIN 重 ACK；2s 后 Closed |
| h09 | 被动关闭 LastAck | CloseWait→LastAck→Closed 全臂 |
| h10 | 同时挥手 | FinWait1 直入 TimeWait |
| h11 | SYN 重传重同步/新 incarnation | 同 incarnation 重答 SYN\|ACK；异 seq 忽略 |
| h12 | 握手第三步捎带数据+FIN | 一步 Established+交付+CloseWait |
| h13 | 超长段（200B） | **状态机层照交付**——长度执法在 vnet IO 层（parse_gw_tcp L519-521 拒 >MSS），分层事实 |
| h14 | 部分 ACK | 保槽重发整段；全覆盖才清槽 |
| h15 | send 门控 | 占位/非 Established/0/129B 四面全拒 |

**结论：状态机在其宣告语义域内无一处证伪**；h05/h06 是两条真实语义毛刺（已知限制级）。

### 2.2 客机在环：客户端路径字节级毒化（toxproxy）【证】

boot 链基线：本配置下 `NET ok`/`TCP ok echo=64`/`LSN ok n=7`/`DNS ok`/`TCPS ok` 五标记全绿（私有端口宇宙 + 重映射 vnet）。

| 场景 | 注入 | guest 实测（serial.log 时戳） |
|---|---|---|
| 握手期 RST | 19699 闭合口 | `rstopen st=TIMEOUT took 94ms`——SLIRP 即时回击 RST，berry-tcp SynSent→Closed，vnet 答 **ST_TIMEOUT**（调用方不可区分「拒绝/不可达」，tcp_op L1168 形） |
| 回显字节吞 | 19601 drop:1 | `rt#1 st=TIMEOUT after 0B (60085ms)`；SLIRP 已 ACK 数据（槽清）→ **会话可复用可关**（close=OK）——纯超时面，无死锁 |
| 回显短斤 | 19604 short:1 | 同形：TIMEOUT@60s、close=OK |
| 回显加倍 | 19603 dup:1（256B 单写回显） | **0B 交付 + close=TIMEOUT**——回显 256B 到达（px-dup: `echo 256B -> guest @0.40s`）但 vnet 拒收 >MSS 段（L519-521），SLIRP 重传同段永拒→**入站黑洞**（详见 §3.3） |
| 中途真 RST | 19605 rst:1 | 325ms 内快死（rt 失败+close OK）——RST 任意态转 Closed 的 op 面表现 |
| 对端无响应 | 19607 hold:70 | rt1 TIMEOUT@60.1s；rt2（96B 异型探针）**TIMEOUT got=0B**——70s 放行后 160B 回显同为 >MSS 黑洞，迟到字节错配探针被遮蔽（TCP 字节流正确性不受影响；RPC 层错配风险见 §4 缺失项）；close TIMEOUT@60.2s（黑洞连坐 FIN 路径） |
| 延迟回显 | 19606 hold:12 | `sent=256B bad=0 close=OK thru=21B/s rtt[336..12054]ms`——停等在 12s 延迟下仍正确（128B/段未触黑洞），吞吐=21B/s |

### 2.3 客机在环：服务端面（hostfwd + 宿主对抗 client）【证】

| 场景 | 实测 |
|---|---|
| **容量 10 并发**（C1） | 宿主 10/10 connect OK（SLIRP 前端恒收）；guest 恰收 **7 条**（dsid 2-8，echo 19B×7 全对拍 OK）；conn7-9 echo 全 TIMEOUT——**8 槽=1 listen+7 数据整帽**，第 8+ SYN 被 route_seg 静默丢（L596-607 不应答即丢），宿主侧重传耗尽后 RST |
| 中途 RST / 尸连接 | 被 C1 满表遮蔽（SYN 无槽可进）——未独立取到，如实记「遮蔽未测」 |
| 半关闭 | 见 §3.3 S2：WR 半关的 FIN **可达**（fin=1 事件），与全 close 的 FIN 不可达形成对照 |
| 槽回收 | **终态 muxstat：8×NOSESSION**——echo server 的 CLOSE 政策（会话 CLOSE+listen CLOSE）可全量回收；但对端 FIN 不可达时若服务不主动 CLOSE，槽**永不回收**（C1 后 fin=0 live=7，后续连接全被拒） |

## 3. r2 吞吐与多会话实测

### 3.1 客户端路径（干净 echod 直连，128B/段停等）【证】

| 载荷 | 墙钟 | 吞吐 | rtt/op |
|---|---|---|---|
| 4KB（32 段串行） | 8.42s | **486 B/s** | 124-470ms |
| 16KB（128 段串行） | 34.92s | **469 B/s** | 107-497ms |
| 256B@12s 延迟 | 12.39s | 21 B/s | 336ms-12s |

外推 64KB ≈ 139s。**VNC 需 1KB@10fps=10KB/s（差 21×）、30fps=30KB/s（差 64×）**。上限来自协议模型（in-flight=1 + 每 op 一段 + 串行 RPC），非实现低效——TCG 下每段往返百毫秒级是系统常数。

### 3.2 服务端路径（mux echo server）【证】

- 小块节流入站（100B/150ms 节拍，避开黑洞）：4KB 全量 echo 逐字节 OK，done@13.27s，**309 B/s**，首回显落后发完仅 0.34s——echo 循环（POLL→DRAIN→RT/128B）的节奏上限量级与客户端路径同阶。
- 大块入站（1024B 连发）：4KB 字节全达（rx=4864B）但回显仅 2560B 时窗尽——RPC 节拍追不上；宿主 5.11s 等待窗内 0B 回显。
- **多会话并发上限：整 8 槽**（§2.3 C1）——1 listen + 7 数据；ACCEPT_Q_MAX=2 是 pending 门非总量门（本测 Pending 全数转 Ready，未见独立收紧）。

### 3.3 入站 >MSS 单段黑洞（本波最大新发现）【证】

机制链：对端一次写 >128B（真实 TCP 对端常态——本波实测 SLIRP 在部分形态下不按我方 MSS=128 通告分段，拼段后单段可达数百 B）→ vnet parse_gw_tcp 拒收（L519-521 宁拒勿断）→ 依赖对端重传恢复，但**重传仍是同形大段** → 永拒 → 0 交付 → 该连接死锁（连 FIN 路径连坐：close=TIMEOUT）。实证三例：dup 臂（256B→0B/60s+close 超时）、stall rt2（160B→0B）、对照小块路径（≤128B 全通）。**VNC 服务端收 client 请求/按键流（小块）不踩；但任何大块入站（剪贴板/文件传输/bursty 控制流）必踩**。

### 3.4 泵节奏（数据可见性 = RPC 节拍）【走+证】

serve() 仅在驱动型 op（RT/CLOSE 的 drive_until 1ms 泵）与每应答后 EAGAIN 单帧 pump 时收帧；POLL/DRAIN/ACCEPT 不收帧；空闲窗 10s 一帧（main.rs L866-873）。实证侧写：echo 路径首回显落后发送节奏 ~0.3s（客户端节拍驱动收帧）；纯 POLL 轮询下突发段的可见性以 client 节拍为限。

## 4. r3 能力矩阵

三态：**实证**（本波实跑）/ **走查**（代码+harness，未客机在环）/ **缺失**。VNC 需求清单取自任务书（多会话≥2 / KB 级持续流 / 劣化 LAN 对端 / 半开恢复）。

| 机制 | 三态 | 对抗结果 | VNC 对照 |
|---|---|---|---|
| 握手（主动/被动） | 客机在环实证 | 五标记绿；假 ACK/重 SYN/捎带全臂正确（h01/h11/h12） | 满足 |
| 序号推进 | 走查（plain `>` L365，echo 域无害；h05 毛刺） | 未来 ACK 越界接受 | 长流理论风险，非阻塞 |
| FIN 双向/同时挥手 | 客机在环实证 | 主动全绿（TCP ok）；被动臂 halfclose fin=1 事件达；h08-h10 全绿 | 满足 |
| RST 任意态 | 客机在环实证 | h07 七态+Listen 豁免；握手期 RST→ST_TIMEOUT 94ms；中途 RST 快死可收口 | 满足 |
| 重传/RTO 退避 | harness 实证（客机=界） | h02/h14：3 次退避后放弃转 Closed | 真网卡域才有意义 |
| 接收窗（自方通告 512） | 实证 | 恒定通告；rx 溢出防御 loud 丢新（L183-191） | 满足 |
| **发送窗（对端窗口消费）** | **缺失** | h06：window 解析后丢弃，零窗对端照发 | 非阻塞（LAN 对端窗口大），劣化域隐患 |
| **接收按序重组** | **缺失**（设计即无） | h04 无缓存；配合 §3.3 构成入站黑洞面 | **阻塞项** |
| **入站大段接收** | **缺失（拒收）** | §3.3 三例死锁 | **阻塞项**（大块入站必踩） |
| 多会话 | 客机在环实证 | 整 8 槽（1+7）；满后 SYN 静默丢；FIN 不可达时槽需政策回收 | 单客户端满足；≥2 客户端贴边 |
| 并发吞吐 | 客机在环实证 | 客户端 486 B/s / 服务端 echo 309 B/s | **阻塞项**（差 21-64×） |
| 劣化对端鲁棒 | 部分（毒化实证+段级 harness） | 丢字节→TIMEOUT 可恢复可复用；RST→可恢复；延迟→正确但慢；迟到错配 RPC 面无请求序号（本波被黑洞遮蔽未独立实证，netdproto 头注自认） | 部分满足 |
| 半开恢复 | 客机在环实证 | stall 臂：对端静默死→RT TIMEOUT→**CLOSE 可收口**（会话弃置语义成立）；但无 keepalive/空闲超时——恢复依赖应用层政策 | 政策可满足 |
| RFC793 最小合规 | — | 三毛刺：未来 ACK 接受（§3.3）/零窗照发（§3.1 流控）/无 RST 回陌生四元组（v2 缺） | LAN 内可容忍 |

## 5. r4 差距地图与裁决倾向

### 5.1 阻塞项（VNC 前必须动）

1. **吞吐模型**（`berry-tcp` in-flight=1 L225-233 + `netdproto` TCP_DATA_MAX=128 L87 + vnet 停等 roundtrip L242-278）：需 extend——发送窗记账与消费、in-flight>1（重传队列）、MSS 上调（vnet RX_BUF_SZ 几何约束 main.rs L80-84 需同步）、RPC 面多段化。量级：每刀独立都只能拿常数因子；三刀合取才进 10KB/s 域。
2. **入站大段黑洞**（vnet parse_gw_tcp L519-521 拒 >MSS + berry-tcp 无重组 L380-389）：需 extend 接收面——至少按 rcv_wnd 缓存乱序/大段拆分交付（或 MSS 通告与 RX 几何同步抬升后放宽拒收线）。
3. **泵/事件模型**（serve 空闲 10s 一帧 L866-873 + POLL 不收帧 + 客户端 1s 节拍）：需 patch——vnet 空闲窗步进提频或表定时驱动收帧；VNC 输入延迟目标 <100ms 需推模型或 10ms 级轮询。

### 5.2 非阻塞项（可后置/政策解）

- 槽容量 8（单 VNC 客户端单 listen 恰好）；槽回收靠服务端 CLOSE 政策（终态全释实证）+ 建议加空闲超时。
- 零窗忽略/未来 ACK/序号回绕/无挑战 ACK——LAN/SLIRP 域无害，列已知限制。
- RST 不可区分拒绝/不可达——调用方语义债，小刀。

### 5.3 未知项（工具边界）

- 真丢段域的 RTO/退避/乱序丢弃臂（结构性不可注入于 SLIRP；macOS 无 root 不可 raw 注入；唯一路径=改 QEMU netdev/自建虚拟网卡——超出本波工具盒）。
- RTT 自适应缺失的实际代价（RTO 恒 3s 定值——真网络劣化域）。
- SLIRP 分段行为的确定性（本波观察到拼段相关的不一致：小块路径全通 vs 单写大块黑洞 vs 1024 连发字节全达——未穷尽边界）。

### 5.4 裁决倾向

**维持 berry-tcp 状态机（语义域内全部实证正确，是资产不是负债）；extend vnet/berry-tcp 收发面（窗口消费 + in-flight>1 + 接收重组/大段）；patch 服务面（泵节奏 + 槽超时 + netdproto 请求序号）。VNC 路线判断：当前栈不可承载——「先把 TCP 协议研究出来」的答案是三刀 extend 先行，VNC 服务化后置。**

## 附录 A 命令与产物（全 /tmp/berry-adv）

- 段级 harness：`cd sm-harness && cargo test`（15/15）。
- 客机在环：`bootdrv.py`（FIFO：wait/send/tail/quit）+ `runscenarios.sh`；串口全量 `serial.log`、时戳流 `drv.log`、毒化臂 `px-*.log`、宿主 client `advcli.py`、场景汇总 `scenarios.log`。
- 关键原始行（drv.log）：`T: rstopen st=TIMEOUT took 94ms`；`T: rt#1 st=TIMEOUT after 0B (60085ms)`；`T: client done sent=4096B bad=0 close=OK rtt[124..470]ms wall=8420ms thru=486B/s`；`T: client done sent=16384B ... thru=469B/s`；`T: ev acc dsid=2..8`（恰 7 条）；`T: ev summary acc=7 fin=0 rx=133B tx=133B live=7`；`T: ev summary acc=4 fin=1 rx=4864B tx=2560B`；终态 `T: sid=1..8 st=NOSESSION`；`AC: smallchunk total=4096B ... thru=309B/s echo=OK`。
