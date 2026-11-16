# NET-04 客机 TCP 四刀流水线：op 间 K 窗、请求序号与批量 IO——实施与对抗复验证据链（2026-11）

> 版本：**1.0**（v1.0）｜2026-11-16｜文件名 `NET-04-客机TCP四刀流水线与对抗复验-v1.0.md`（与 NET-02/03 命名对齐；docs 收口波定名，初稿名 `NET-04-客机TCP四刀流水线-v1.0.md`）
> **定位**：[四刀 plan](../.agents/plans/2026-11-16-客机tcp四刀流水线.md)（承 [NET-01](NET-01-客机TCP栈深研与裁决-v1.0.md)/[NET-02 首刀](NET-02-客机TCP首刀落地与对抗复验-v1.0.md)/[NET-03 二刀](NET-03-客机TCP二刀吞吐与对抗复验-v1.0.md) + 2026-11-16 probe 裁决）的实施波证据链——量子栈四拆（libc K 窗 / vnet serve 多 op 化+arm 门容量化 / netdproto seq 配对 / tx 批量完成）、顺带批（F72 窗新鲜度护栏 / ACK 域上界 / EVS 250B 入标记链）、吞吐 A/B 与对抗复验数据、八 op 语义域保持取证。
> **证据分级**：沿用 NET-01 口径——【证】= 本波实跑输出/一手行号；【走】= 走查；【界】= 工具边界。
> **物理基础**（probe 裁决，wire 直测定谳）：SLIRP 对 guest 侧通告窗恒 65535 无 WScale——NET-03 §2「snd_wnd=128」证伪；真闸 = 客机内同步等待/唤醒量子栈（tx_send 逐帧同步 + 泵收帧唤醒滞后 24-270ms + 通道逐跳串行），wire RTT 0.12-0.3ms 可忽略。证据原件 /tmp/berry-adv/probe-W.pcap（22328B）/probe-W-marks.pcap/probeW-serial.log。

---

## 0. 总判断

四刀按 [plan](../.agents/plans/2026-11-16-客机tcp四刀流水线.md) 落地，四拆同步律全数兑现：**(a)** libc Client 臂 K=2 op 间流水线（issue/harvest 两相 + seq 配对收割 + 陈值弃 + BUSY 同 seq 退避重发）；**(b)** vnet session-0 RT「入环即注册挂起 op、应答延迟至完成」（完成队列 + seq 戳 + 挂起期短 tick 泵节奏）；**(c)** arm 门容量化（环空槽 < ceil(len/MSS) → ST_BUSY 背压；命名臂去 drive_until 入环即答）；**(d)** tx 批量完成（整批帧单次页写/单门铃/单 poll + 突发窗纯 ACK 暂存突发尾一次收口 + recv 完成项 8B 短读）。协议面 netdproto seq 尾随旁路形（请求 `[op][dlen][data][seq u8]` ≤ 253 / 应答 `[st][dlen][data][seq]` ≤ 253 与 2B `[st][seq]`），v1 两形逐字节零漂移。顺带批：F72 snd_wl1/wl2 护栏 + ACK 域上界拒（RFC793 SEG.ACK ≤ SND.NXT）+ EVS 载荷 64B→250B 入标记链（黄金哈希同批换）。门面：berry-tcp 55/55（+2）+ netdproto 48/48（+5）+ netproto 24/24 + harness 21/21（19 旧臂零漂移 + h20/h21）+ berry-build 过 + check-docs 0；五标记链全绿（EVS ok n=2 为 250B/24B 契约更新后形态）。吞吐与对抗数据见 §2/§3（快测量级方向口径 + verify 正式对拍全过——echo 16KB 服务端门记差）。

**NET-03 §6「流水线独立于对端窗」表述的裁决**：**证实一半、修正一半**。证实面 = 出站方向：SLIRP 通告窗恒 65535（probe wire 直测），出站在途只受客机内环容（TX_SLOTS×MSS=512）支配，op 间流水线的发送相位确不受对端窗门控——「独立于对端窗」在出站面成立。修正面 = 入站方向与增益上限：K 个在途 op 的回显流受 **guest 自通告收窗 512** 门控（K=3 即 750 > 512 双面撞），且增益上限 = min(环容, 通告窗)/相位——「不受 SLIRP 通告窗支配」不能外推成「不受一切窗支配」。NET-03 §6 原文按此改排（①物理基础改写、②降级入站面、③降 K≥3 前置、④出局、⑤新增 tx 批量）。

## 1. 四拆落点（几何与语义）

- **netdproto seq 域**（P1）：`RT_SEQ_REQ_MAX = RT_SEQ_REP_MAX = 3+TCP_DATA_MAX = 253 ≤ CHAN_MSG_MAX`（两钉入断言族）+ `RT_PIPE_K = 2` 双侧共源（libc `TCP_PIPE_K` / vnet `RT_K` 同值）；`encode_rt_seq_request`（libc 专用）/ `parse_tcp_s_request` 扩 seq 出参（三形消解 = v1 恰长 → −4B sid → −1B seq，同字节 dlen 权威结构上互斥——「尾 4B 恰似合法 sid」的 seq 形帧单测钉板）/ `encode_tcp_reply`/`parse_tcp_reply` 扩 seq 形（2B `[st][seq]` 非 OK 携原 seq——BUSY 背压配对面；OK 空数据恒 v1 1B 形双侧对称拒）。named RT 携 seq 的 257 > 256 几何阻死钉为单测（手搓帧 parse 恒 None）。
- **libc K 窗**（P2/P3/P4）：`netd_rpc_raw` 拆 `netd_rpc_issue`/`netd_rpc_harvest`（单发面原样保留——DNS/OPEN/CLOSE/慢 op/Server 臂 v1 严格串行零漂移）；`sock_send` Client 臂重写为 `sock_send_pipe`：窗口槽 FIFO（head+inflight）+ 滚动 seq 发号（回绕距 256 ≫ K=2）+ 回显按 op 序入 RESIDUE（队头收割序不变式）+ 余量准入 `res_pending + (inflight+1)×250 ≤ RES_MAX`；RES_MAX 250 → `RT_PIPE_K×TCP_DATA_MAX = 500`（静态 +2KB）。错误路径纪律：帽到/op 级错 → 部分写/errno 返回，在途 op 迟到应答按陈值弃（seq ∉ 在途集即弃——v1「晚到应答毒化下一请求」债在单客户端 K 窗面收窄清偿）。BUSY 退避：收割帽收到退避时刻（1s 节拍），空等帽不吞重发窗。
- **vnet 会话/服务面**（P5/P6/P8/P9）：Sess.tx 扩容 `TX_CAP = TX_SLOTS×MSS = 512`（跨 op 连续，环空才失效——K 窗下 = 全部挂起 op 收口）；`RtPipe` 挂起 op 队列（定槽环 ×2：ops + done，各 K 槽）——enqueue = 环容量准入（空槽 ≥ ceil(len/MSS) + op 槽空 → BUSY 背压；态不可用 → BAD）→ 分批入环（批量 tx）→ 注册 {seq, len, end=snd_nxt, echo_got, deadline}；完成判据 = echo 收齐 + snd_una wrapping 覆盖末段（FIFO 队头独占——回显流严格有序不变式）；超时记录保留至 echo 收齐才弃置（流位对齐——迟到 echo 字节不致错位下一 op）；会话死（RST/重传到帽）全挂起 op 即刻 BAD 收口。回显收流幅 `RT_ECHO_MAX = K×TCP_DATA_MAX = 500`（echod/SLIRP 可把 K 个在途 op 的回显拼成单段——准入缓冲小于此 = 拼段被首刀准入闸拒收 = 重传同形拒 = 死锁面，几何不变式：未收回显 ≤ 挂起 op 数 × 单 op 帽）。泵节奏：挂起 op / RTO-TW 臂在席 → serve 等待窗 SVC_IDLE 10s 缩为 SVC_TICK 10ms（≤ 唤醒量子——完成延迟塌到节拍级）；全闲 10s 心跳零漂移。CLOSE 先收口在途 op（帽内泵到队列清空）再 shutdown——FIN 排队零改动。
- **vnet IO 面**（P10/P11）：`tx_flush_batch`——帧布局 IOB 内偏移（slot × `TX_FRAME_STEP` = 198 = 12 net 头 + 14 eth + 20 ip + 20 tcp + 4 mss 选项 + 128 段体；`TX_BATCH_MAX×STEP = 792 ≤ IOB_SZ 1600` 机器钉）+ desc 0..n-1 各指 TX_BUF_PAGE 内偏移 + 一次整页 vmo_write + 单门铃 + 单 poll used.idx 前进 n——「每帧一次唤醒量子」塌成「每批一次」；`tx_send` 保留为 ARP/ICMP/DNS 单帧路径（三调用点零改动）。`stage_seg`/`act_q`/`step_q` 批量族：act 的 Data/Syn/SynAck/Fin/Ack 全走批槽（满批自动 flush）；泵突发窗纯 ACK 暂存（`hold_ack`/`ack_due` + 突发尾 `ack_flush` 一次发出——累计 rcv_nxt 覆盖，对端 RTO ≥ 200ms ≫ 突发窗 ms 级）；阻塞路径（selftest/legacy roundtrip）每步即冲 = 行为零漂移。recv 完成项 8B 短读替代整页 4096B 读（used 环设备拥有——只读其一项）。
- **vnet 命名臂**（P7）：`send_data` 去 up-front 独占拒（保逐段泵等位 + SEND_CAP 30s 帽）；serve 命名 RT 臂去 drive_until arm_idle——入环即答 ST_OK「入环受纳，ACK 归泵收口」（应答字节形状/成功判据零漂移，时序提前；重传臂由 serve 空闲泵收口）。
- **顺带批**（P12-P15）：berry-tcp F72——Conn +`snd_wl1`/`snd_wl2`，on_seg_open 窗更新过新鲜度护栏（`d = seg.seq−wl1 ∈ (0,2^31)` 严格更新，或相等且 `seg.ack ≥ wl2`——迟到/乱序段不回缩窗；SLIRP 恒 65535 域理论面，护栏廉价保持）；ACK 域上界——`seg.ack > snd_nxt`（wrapping）整段 ① 拒（不推进 snd_una/不弹槽/不采纳窗——幻 ACK 弹未发槽的已知限制收口，多 op 在途放大该面）。EVS 250B——bsh `EVS_PAYLOAD_0` 64→250（公式 (i*7+3) 不变）+ skt client 臂同源三点 + [evsclient.py](../tools/evsclient.py) PAYLOADS/长度断言/黄金哈希 g0 同批换（`0f18a2e0…dd56`，g1 不变）——标记链覆盖 250B 全域多段路径（入站单段 250 + 出站 2×128 拼段）。

## 2. 吞吐 A/B 快测（量级方向口径）

**环境**：macOS 宿主 TCG 单核、QEMU 128M、kt4 kernel@18388fb54 + kntree rootfs + 本波 user 侧新产物（同树同源链）；echod 19180 对端；数字为量级方向证据非基准成绩。

**数据**【证】（impl-ab-serial.log，同 boot 相邻相位交替 A/B——负载漂移互抵）：

| 相位 | B（串行 RPC 镜像） | A（libc K=2 窗 sock 相） | A/B 比值 |
|---|---|---|---|
| 4KB round1 | 841 B/s（rtt 190-380ms） | 1234 B/s | 1.47× |
| 4KB round2 | 927 B/s（rtt 93-458ms） | 1416 B/s | 1.53× |
| 16KB | 848 B/s（rtt 62-471ms） | 1355 B/s | 1.60× |

**判定**（快测口径）：绝对域门 **≥ 1000 B/s 过**（A 链 1234-1416 B/s，全部 bad=0 close=OK echo 逐字节对拍过）；同窗比值门 2.0× **未达**（1.47-1.60×）——如实记差，两因：(a) B 基线本身已被四拆抬升（串行 op 同享 tx 批量单 poll + 完成即答——probe 基线 487 B/s → 本窗 841-927 B/s，「⑤批量完成串行路径即刻受益」模型兑现）；对 probe 基线绝对值 A 链 ≈ 2.5-2.9× 但非同窗不可比；(b) K=2 增益上限 = 窗容/相位（模型 1.5-2× 域内）。**正式验收对拍留 owner 复跑窗**：B 链按 plan 口径换「仅 vnet/libc/tcptest 三件 HEAD（旧串行）版」同窗交错（本窗两链同 vnet，串行链吃到了 serve 改造红利——比值口径偏保守，绝对值口径不受影响）。

**模型对账**：probe 模型 K=2 域 1000-4000 B/s（每 2-op 批 2-3 唤醒量子）——实测 1234-1416 B/s 落域内下段；串行链 841-927 B/s 对 probe 实测 432-487 B/s ≈ 1.8-1.9×（serve 改造独立收益面）。wire 时序（impl-ab.pcap）：op 双段同批单门铃（128+122 同 µs 级时戳）、echo 0.3ms 返、完成即答不经等待窗——「每帧一次唤醒量子」面消除。

**正式验收对拍（verify 波，2026-11-16）**【证】（vfy-{a,b}{1,2}.log——B 链 = 正牌两刀基线 5391d43b4 的 vnet（kntree 构建，即 probe 基线同源产物）+ tcptest 串行 RPC（链 kntree 两刀 libc/netdproto）；A 链 = 四刀工作树 vnet/libc + sock K=2 窗；跨 boot 相邻交错 A1→B1→A2→B2 同窗同载（宿主恒载：并行巷双 QEMU 在席），kt4 kernel@18388fb54 两侧同一）：

| 相位 | B（两刀串行基线链） | A（四刀 sock K=2 链） | A/B 比值 |
|---|---|---|---|
| 4KB round1 | 697 B/s（rtt 33-551ms） | 1623 B/s | 2.33× |
| 4KB round2 | 615 B/s（rtt 159-606ms） | 1296 B/s | 2.11× |
| 16KB round1 | 644 B/s（rtt 131-678ms） | 1357 B/s | 2.11× |
| 4KB round1' | 660 B/s（rtt 93-557ms） | 1326 B/s | 2.01× |
| 4KB round2' | 622 B/s（rtt 93-678ms） | 1631 B/s | 2.62× |
| 16KB round2' | 582 B/s（rtt 182-766ms） | 1391 B/s | 2.39× |

**判定（正式口径）**：客户端 4KB 比值门 ≥ 2.0× **过**（四相位 2.01-2.62×，中位 ≈ 2.2×）；16KB 比值门 ≥ 2.0× **过**（2.11×/2.39×）；绝对门 ≥ 1000 B/s **过**（A 链 1296-1631 B/s，全部 bad=0 逐字节对拍过）；B 基线 582-697 B/s 落 probe 串行域（487 B/s 同源产物 + 本窗宿主载）。实施波快测的保守判读（B 同享 serve 红利）由正牌基线对拍证实：真基线下 K=2 窗增益即模型 2× 域。**echo 16KB 服务端门 ≥ 1.5× 未达记差**：B 链 echo 496/510 B/s（echo=OK 逐字节）vs A 链修复版 531 B/s（vfy-efix3：rx=tx=16384B echo=OK）≈ 1.06×——环容 512B（= 2 op）几何帽 + 暂存安全闸（见 §3 verify 实抓③）使第 3 op 起重回 ACK 步调，serve 改造的「入环即答」只在首 2 op 兑现；零回归成立（A ≥ B），1.5× 模型门的结构前提（无界暂存）不真——记差不粉饰，暂存紧缩（ACK 前缀弃置重基点）为 owner 后续面。测量窗注意：dpg pager watchdog 老内核 flake（18388fb54 kernel PAGER_WATCHDOG_SECS=30，宿主重载窗击杀后台 task，先例 probeW/NET-03 期 1-3 次/波）在本窗同样在席——对拍判据取吞吐相行全绿 + 无 30s 级墙钟/rtt 离群，watchdog 击杀点均在相位外（echo 相内击杀的 boot 整体重试）。

## 3. 对抗复验数据

**verify 波对抗复验（vfy-adv3，修复版 vnet+tcptest-adv 重排形，私有高位口 toxproxy 自起自收）**【证】：①stall——rt1(64B) TIMEOUT@60.2s → rt2(96B) OK 96B clean（两 boot 复现）→ close OK；②sock K=2 hold(45s) 抵达 500/500 bad=0@44.9s + 干净复打 3132 B/s；④rst——rt 即答 BAD（RST 击毙）+ close OK；⑤drop 串行——rt TIMEOUT@60.3s + close OK（正形恢复——实施波同形）；⑤b **多 op 在途丢段**（sock K=2 经 drop:1）——sent=500 got=250 bad=2@91s：被丢 op 无回显、幸存 op 回显按 FIFO 流序归队头（字节流语义正解）→ pattern 失配诚实检出、零死锁，close 后干净复打 1074 B/s 零通道毒化；⑥dup 单 op 128B——OK 128B clean（NET-03 尾垃圾容忍同形）+ close OK；⑥dup K=2 sock 500B——500/500 bad=0@2317 B/s（重复回显被 RtPipe 超收纪律吸收）+ 干净复打 1249 B/s；echo 相 16KB——rx=tx=16384B echo=OK（§2 修复版口径）；⑦burst——**记差 4/4 复现**（实施 3+verify 1），verify 增时标：send#1 n=2000 w=500@365ms、send#2 n=1500 w=500@**511ms**（非收割帽烧穿——send#2 快返但零残渣入槽）、recv errno=11 after 500B(sent=1000)@30s 帽——缺陷面进一步收窄至「send#2 快返路径的残渣入槽/应答配对丢弃面」（wire 全达在案，owner 深挖输入 +tcptest.adv 时标件）。注：burst 容错早退不关 fd（tcptest 私有件缺陷，其后 ⑧ 复打 connect errno=5 为 fd 泄漏 + 在途 op 占位所致，非 vnet 缺陷——记私有件偏差）。

**多 op 错配/流位对齐（本刀新风险面，必测）**【证】（impl-adv-serial.log，私有高位口 toxproxy hold:75→19180，进程自起自收）：

- **stall 臂**：rt1(64B) `st=TIMEOUT took 60191ms`（op 帽精确 60s）→ 迟到 echo（tox 75s 释放）→ **rt2(96B) `st=OK got=96B clean=true`**（0xa5 全等逐字节）→ close OK 2.3s——超时 op 记录保留至 echo 收齐的流位对齐纪律实证：迟到回显被死 op 吸收、下一 op 干净（TIMEOUT 可恢复可复打）。
- **sock K=2 hold 抵达 + 干净复打**：hold:45s（< 60s op 帽）口 sock `sent=500B got=500B bad=0`（两 op 同窗挂起 45s 后配对收割）→ 直连复打 `sent=500B got=500B bad=0 thru=1155B/s wall=432ms`——**迟到的窗口收割零通道毒化**（seq 配对面：陈值弃纪律兑现）。
- **毒臂族**：rst:1 臂 open OK → rt `st=BAD`（RST 击毙即答——挂起 op 即刻 BAD 收口不待帽）→ close OK；drop:1 臂 open OK → rt TIMEOUT 60.2s → close OK（在途环残留唯 CLOSE 收口语义保持）。
- **背压 burst**（**记差未解**）：单 send(2000B) → 首段部分写 500B（K 窗余量准入）+ 收割 500B 过 + 续写第二段（send n=1500 w=500）后收割饥饿——recv 空槽 EAGAIN 30s 帽烧穿（3/3 复现，`burst recv errno=11 after 500B (sent=1000)`）；**wire 层无罪证**（impl-adv.pcap：第二段两 op 的数据/回显/ACK 全部在 ~150ms 内完成，vnet 完成判据全达）——缺陷面锁定在 libc K 窗多次部分写序列的返回/入槽路径（单次部分写+收割、以及固定 2×chunk 节奏的 sock 相 4KB/16KB 全绿——仅 burst 的「部分写→续写」第二段触发）；owner 复跑窗深挖（诊断件 tcptest.adv 已备 /tmp/berry-adv，含逐 send/recv 时戳打点）。
- **echo 相（serve 改造面）未能计量**：burst 早退残留在途 op → 60s 帽到 TIMEOUT 应答无人收割 → 通道被陈值占位 → 后续 echo server 的 POLL 请求被陈值应答错配（rx=0B 卡 90s）——**wave10/14「dup netd 通道并发消费错配」债的活体复演**（本例为死客户端的未收割应答毒化后续消费者；单客户端纪律下本不应出现——根因同 burst 记差项，非独立新债）。serve 改造的吞吐代理证据 = B 串行链 841-927 B/s（对 probe 基线 487 ≈ 1.8-1.9×，串行 RT 同享入环即答+批量 tx）；正式 1.5× 对拍留 owner 复跑窗（burst 记差清偿后复跑）。

**段级 harness 21/21**【证】：19 旧臂零漂移 + h20 F72 镜像（陈旧段不回缩窗/新鲜段覆写/同 seq 陈 ack 不采纳）+ h21 幻 ACK 拒（越 snd_nxt 不推进不弹槽不采纳窗 + 合法 ACK 照常）；h06 携窗 ACK 的 seq 随 F72 取对端 snd_nxt 新鲜值（真实形态，测试意图不变）。工具边界承 NET-01：段级丢/乱/重/迟只在宿主 harness 域；客机对抗面限应用层字节几何 + 毒臂【界】。

**实施期实抓两 bug（私有 fixture 快测的必要性实证）**：① RtPipe 定槽环 head==tail 满/空歧义——K=2 双 op 在环被模算术算成空（`olen=(tail+K-head)%K`，tail 回绕即 0）= 回显超收 + 完成丢失（A 链首跑 90s 收割帽烧穿实抓），修复 = 显式 ops_len/done_len 计数；② 完成队列 drain 时延面——busy 判据不含 done 队列 → 完成即答白等 SVC_IDLE 10s 心跳（B 串行链 10.2s/op 实抓），修复 = 判据扩含 done + EAGAIN 泵后立即 drain + RT 即时应答（BUSY/BAD）携原 seq。

**verify 波实抓两 bug（echo 相崩溃根因，均即修）**【证】（vfy-a1/vfy-ediag/vfy-efix3 证据链 + DIAG #[track_caller] trap 钉位）：③ 表会话 DRAIN 割幅未帽 `TCP_DATA_MAX`——serve 的 DRAIN 复用 session-0 回显收流缓冲 `rt[RT_ECHO_MAX=500]`，表会话攒 >250B 后首割即 500B 入应答 → `encode_tcp_reply` 越帽返 None → serve 防御 trap 即崩（4/4 确定性；EVS 250B 单段收割不触发故标记链漏网）；修复 = DRAIN 割幅帽 250B（多次割循环收清，v1 契约保持）。④ 命名 RT「入环即答」后连发越过 TX_CAP 暂存——send_data 只受环槽/窗门控，echo server 以 RPC 节奏连发 250B op 时 `snd_nxt−tx_seq0` 在 ACK 归泵前单调涨过 512 → act_q Data 臂暂存越界 trap（第 3 op 必中；两刀形态 drive_until 每 op 等 ACK 故不可达，四刀语义变化的边界暴露）；修复 = send_data 暂存容量闸（下一片容不下时泵等环空——post_account 环空即失效暂存重立基点，帽内；ACK 跟得上时零等待，入环即答时序不受扰）。修复后 echo 相 16KB 全量 rx=tx=16384B echo=OK（VF/VD 链实证）。

## 4. 语义域保持（八 op 红线）与门面

- **八 op 对外语义零改动**【走】：op 集合、v1 请求/应答字节形状、恰长执法、尾随 session 编码、v1 session-0 逐字节、烘焙线（非 OK 恒不携 data/sid/addr）全部不动——唯二扩面均「可扩不可破」：RT 请求/应答各增 +1B 尾随 seq 形（长度歧义消解见 §1；无 seq 请求恒无 seq 应答——bsh/tcptest 等旧消费者零漂移）；命名 RT ST_OK 细化「入环受纳，ACK 归泵收口」（应答字节形状/成功判据零漂移，仅时序提前）。
- ST_BUSY 域扩为 RT 环容量背压码（既有码位，旧消费者按既有分诊面——libc Server 臂零接触：命名臂泵等位不返 BUSY）；ST_TIMEOUT 语义不变（帽到应答、会话保命、在途环残留唯 CLOSE 可收口——挂起 op 记录保留至 echo 收齐为新增流位对齐纪律）。
- **门面**【证】：berry-tcp 55/55（53+2：f72_stale_ack_does_not_shrink_window / phantom_ack_beyond_snd_nxt_rejected）+ berry-netdproto 48/48（43+5：rt_seq_request_roundtrip_and_wire_pinned / rt_seq_reply_roundtrip_and_wire_pinned / rt_three_form_disambiguation / rt_seq_plus_session_geometrically_blocked / rt_pipe_k_geometry_pinned）+ berry-netproto 24/24 + 私有 harness 21/21（19 旧臂零漂移 + h20 F72 镜像 + h21 幻 ACK 拒；h06 携窗 ACK 的 seq 随 F72 取新鲜值 5001——真实对端 snd_nxt 形态，测试意图不变）+ berry-build 过 + evsclient selftest 绿（新黄金哈希）+ check-docs 0 告警。
- test-host 全门现红（并行巷 web.rs E0063 非本面）以 crate 级 + harness + check-docs 替代，记偏差（承 plan 语义门裁定）。

## 5. 偏差与边界

- **计划内契约更新**：EVS 载荷 64B→250B（标记判据 EVS ok n=2 不变；破二刀「user bins 冻结面」口径——survey F76 已裁决归四刀批）；v1 应答 4B/5B 帧的解析域扩（`[st][dlen][data][x]` 自四刀起 = 合法 seq 形——v1 恰长优先不受扰，旧钉板两处随批更新注明）。
- **与 plan 的出入**：① `TX_FRAME_STEP` 实取 198（plan 记 194——194 漏算 4B MSS 选项位；几何钉按实值）；② stall 臂语义随「去 up-front 独占拒」更新——旧「rt2 立即 BAD」改判「rt2 挂起 → TIMEOUT → 迟到 echo 流位对齐 → 下一 op 干净」（对抗判据即 plan 语义）；③ recv 批量收割取 8B 完成项短读轻量形（帧仍逐帧入 IOB——省整页读不引入帧缓存生命周期面）。
- **不做面**（边界承 plan）：WScale 降级入站/服务面专属（出站窗已 65535 无缩放）；环扩 TX_SLOTS=8 与 K≥3 双前置（K=2 恰满 512 环 + guest 自通告收窗 512 双界内）；服务端面（命名 RT）流水线不做——seq 请求形 257 > 256 几何受阻，本刀只收 serve 改造串行收益；dup netd 通道并发多消费者仍越界（wave10/14 债收窄非清偿——多客户端并发消费配对需请求序号之外的通道语义）；槽空闲超时仍 NET-01 §5.1-3 既挂账。
- **验收线口径**：本波快测为量级方向证据（单 boot 相邻相位交替 A/B）；plan 验收线（4KB ≥ 2.0× 且绝对 ≥ 1000 B/s / 16KB ≥ 2.0× / 服务端 echo 16KB ≥ 1.5×）的正式对拍由 **verify 波完成**（§2 正式验收对拍节）：4KB/16KB 比值门与绝对门全过；echo 16KB 服务端门 1.5× **未达记差**（A 修复版 531 B/s vs B 496/510 ≈ 1.06×——环容 512B（= 2 op）几何帽 + 暂存容量闸使第 3 op 起重回 ACK 步调；零回归成立；暂存紧缩〔ACK 前缀弃置重基点〕为 owner 后续面）。快测记差原因（B 同享 serve 红利）由正牌基线对拍证实。
- **burst 记差未解项**（owner 深挖）：libc K 窗「部分写→续写」第二段收割饥饿（4/4 复现；verify 增时标——send#2 快返 511ms 零残渣入槽，排除收割帽烧穿假说；wire 无罪，详见 §3）——修复后需复跑：burst 全量对拍 + 同窗 2.0× 比值复测。
- **五刀剩余路线**（按证据充分度排序，均为后续立项候选非本刀欠账）：① burst「部分写→续写」收割饥饿清偿（缺陷面已锁定 libc K 窗 send#2 快返路径的入槽/配对丢弃面——wire 无罪在案，清偿后 burst 全量+同窗 2.0× 复测）；② 暂存紧缩（ACK 前缀弃置重基点/compaction）——echo 16KB 服务端门 1.5× 的结构解锁件（环容 512B=2 op 几何帽下「入环即答」只在首 2 op 兑现，紧缩可把帽后移）；③ K≥3 扩窗三前置（环扩 TX_SLOTS=8 + guest 通告窗联动 + seq 形通道帽 257>256）——3802 B/s（K=4@263ms）只作静默窗天花板；④ dup netd 通道并发多消费者配对（wave10/14 债全偿——单客户端 K 窗面已收窄）；⑤ WScale/动态通告（入站/服务面专属，出站窗 65535 已足）。NET-01 §5.1-1「三刀合取才进 10KB/s 域」在此路线图下的重估留五刀立项时做。

## 6. 代码面与落账建议

- 本波改动面：`user/berry-netdproto/src/lib.rs`（seq 域+断言族+5 测）、`user/berry-tcp/src/lib.rs + tests/state.rs`（F72/ACK 域+2 测）、`user/berry-libc/src/net.rs`（K 窗+RES_MAX 500+issue/harvest 拆壳）、`drivers/bins/vnet/src/tcp.rs + main.rs`（RtPipe/批量 tx/serve 臂/泵节奏/recv 短读 + verify 两修：DRAIN 割幅帽 TCP_DATA_MAX、send_data 暂存容量闸）、`user/bins/bsh/src/main.rs`（EVS 250B 三点+F76 跟随修）、`tools/evsclient.py`（载荷/断言/哈希）、`justfile`（test-host rootface 重复错行 1 行清除——926 行笔误修复本体已随并行巷 merge 落 HEAD）。零改动面：berry-netproto、kernel、user bins 其余。
- 可提交单元建议（owner 即刻落，显式路径；并行会话在树，禁 `git add -A`/`git add .`）：① `user/berry-netdproto user/berry-tcp`（seq 域+F72/ACK 域，两 crate 测绿自洽）；② `drivers/bins/vnet`（四拆+verify 两修，tcp.rs+main.rs）；③ `user/berry-libc/src/net.rs`（K 窗）；④ `user/bins/bsh/src/main.rs tools/evsclient.py`（EVS 250B 同源批）；⑤ 文档链：`research/NET-03-客机TCP二刀吞吐与对抗复验-v1.0.md`（§2/§6 改写）+ `research/NET-04-客机TCP四刀流水线与对抗复验-v1.0.md` + `.agents/plans/2026-11-16-客机tcp四刀流水线.md`（翻 done）+ `.agents/notes/implemented/2026-11-16-客机tcp四刀流水线.md` + `.agents/notes/implemented/2026-11-16-客机tcp二刀吞吐.md`（+1 行互链）+ `docs/阶段沿革.md`（二百八十三/二百八十五两节）；⑥ `justfile`（rootface 重复行清除单列一笔——共享文件，提交前与并行巷树态复核）。私有件（tcptest/bootimpl/harness 扩臂）不入仓（/tmp/berry-adv 留档）；probe 证据件（probe-W.pcap 族）归档建议同前。工作树尚有并行巷属地（berry-appshell/berry-launcher/pm-integration/Cargo.lock/Berry设备平台 文档删除等）——一律不在本波落账范围。
