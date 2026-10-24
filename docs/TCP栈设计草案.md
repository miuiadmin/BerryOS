# P6 尾巴：最小 TCP 栈服务设计草案（T-a/T-b/T-c/T-d）

> 日期：2026-10-22。背景：`P6收官盘点.md` 话术限定 1——「最小 TCP/IP 栈服务」是
> Phase 6 定义第三条的唯一未落项，Phase 全量收官待 TCP。本文 = v2 议程设计先行
> （同 A-8 草案体例：先裁决接缝与拆步，实施刀落地时逐条兑现）。
> 积木现状：A-7/A-8 已落 virtio-net 驱动（vnet 常驻服务）+ berry-netproto
> （eth/ARP/IPv4/ICMP/UDP/DNS 编解码，17 测）+ berry-netdproto（DNS RPC，14 测）。
> **执行结果（2026-10-23 补记）**：T-a..T-d 四刀全落（2026-10-22）——`TCP ok echo=64`（T-c 客户端全链）/ `TCPS ok echo=64`（T-d netdproto 服务化）进 smoke rootfs 相；host 面 berry-tcp 30 测 + netdproto 14→26 测；DEADLINE_ROOTFS 随 T-d 240s→300s。
> **冻结（2026-10-23）**：使命完成——本文自此为实施实录冻结档，后续演进不再回写（新变化进 `阶段沿革.md` 与现行规范文档）；文件位置不动，归档移动待另行裁决。

## 0. 目标与非目标

- **目标**：guest 侧最小 TCP **客户端**——主动 connect SLIRP 宿主回显服务，
  走完「三次握手 → 数据双向 → 四次挥手」全链，逐字节对拍自打标记 `TCP ok echo=`；
  状态机做成 host 可测纯函数 crate（「解析器即产品」第 7 例）。
- **非目标（v1 文档化取舍）**：服务端 listen/accept（hostfwd 反向面）；滑动窗口
  与拥塞控制（**固定窗口 + 顺序单发** in-flight=1）；乱序段缓存（丢弃靠重传）；
  SACK/timestamps/窗口缩放（SYN 选项只发 MSS，其余选项解析侧跳过容忍）；
  多会话并发（vnet 单会话独占——同「单设备单主人 v1 粒度」家族）；
  Nagle/延迟 ACK（禁用——echo 场景立即 ACK）。

## 1. 三块积木与缺口

| 积木 | 现状 | TCP 面缺口 |
|---|---|---|
| berry-netproto | eth/ARP/IPv4(RFC1071+total_len 权威)/ICMP/UDP(**伪头校验和**)/DNS | TCP 头编解码 + 伪头校验和（同 UDP 先例顺家族扩面） |
| vnet 驱动 | 常驻 serve()、双队列 qsize=16、rx 单描述符整收、reset_device、arp/icmp/dns_roundtrip 先例 | TCP 会话循环（握手/收发/挥手/重传定时）；**RX_BUF_SZ=160 几何不够**（§5） |
| berry-netdproto | DNS 严格串行 RPC | 有状态会话与单发单收模型不匹配（§3 T-d 裁决） |

## 2. 借鉴与排除 8 问

- **smoltcp**（MIT OR Apache-2.0，不在 `reference/` 树内——zCore 浅 clone 未带
  网络面；线上对照）：借其**状态机与 IO 分离**的组织（smoltcp 的 Socket/Interface
  分层正合「berry-tcp 纯函数 + vnet IO 驱动」切法）与最小 TCP 状态枚举形状；
  其完整拥塞控制面**不借**（非目标）。只看思路不搬代码。
- 排除 8 问过表：非调度/IPC 轴 PICK 无冲突；不碰「机制/策略分离」中心；
  Theseus 进程内/hubris 静态/seL4 不变量各面不涉及；**GPL 不进**——smoltcp 双证
  OK；验证关 = smoke 四跑 + host 测（G 关既有面）。裁决：可借。

## 3. 分层裁决（编解码/状态机/IO/消费四层）

1. **TCP 头编解码 → berry-netproto 扩面**：`encode_tcp`/`parse_tcp(buf, src, dst)`
   （伪头校验和同 `parse_udp` 先例——src/dst 进签名）；选项面 = SYN 段 MSS 选项
   可编可解、其余选项**跳过容忍**（长度字段步进，同 DNS 压缩指针的宽容族）；
   None 拒绝契约 day-one。序号语义（比较/推进）**不进** netproto——归状态机。
2. **状态机 → 新纯函数 crate `berry-tcp`**（第 7 例）：零分配/零 unsafe/零依赖、
   输入只借用。v0 形状：`Conn { state, snd_iss/nxt/una, rcv_nxt, window }` +
   `on_seg(&mut Conn, &TcpSeg) -> EvOut`（每段至多一个出段动作 + 数据交付切片）+
   `on_rto(&mut Conn) -> EvOut`（重传定时由 vnet 侧 now() 驱动——纯函数层不吃时钟）。
   host 测试盖全状态分支（§7 T-b）。
3. **IO/会话 → vnet 内新模块**（`tcp.rs`）：把 EvOut 翻译成 tx_send/recv_frame 轮询，
   复用 gw_mac/tx_send/dns_roundtrip 全套基建；自验移 serve() 前（A-8b 同款：
   自验毕转常驻、DNS 面零回归）。
4. **消费 → netdproto 扩面缓一步（T-d 可缓）**：TCP 生命周期跨多次 RPC，与
   「严格串行单发单收」模型错配。v1 裁决 = **单会话三 op 折叠**（TCP_OPEN/
   TCP_ECHO/TCP_CLOSE——一次 RPC 走一段会话，vnet 单会话独占锁语义：OPEN 后
   未 CLOSE 再 OPEN → ST_BUSY）；「栈服务」话术最小判据 = T-c 栈跑通，T-d 是
   服务化加强（svcmgr 消费自验 `TCPS ok`），真消费面（bsh wget 类）v2 才有用户。

## 4. 最小状态机面与定值表

状态枚举（smoltcp 形状）：`Closed → SynSent → Established → FinWait1 → FinWait2
→ TimeWait`（客户端主动关闭主线）+ `CloseWait → LastAck`（服务端被动关闭侧，
T-c 走主动关闭、CloseWait 路径由 host 测试盖）+ RST 各态处理。定值：

| 项 | 定值 | 依据 |
|---|---|---|
| MSS | **128**（通告+收帽） | RX_BUF_SZ 256 − 帧开销 66 = 190 上限内留余量（§5） |
| 窗口 | 固定 512 恒通告 | 非目标滑窗；in-flight=1 下窗口仅保对端健康 |
| RTO | 3s 起、×2 退避、3 次帽 | TCG 友好；SLIRP 本地几乎不丢——重传是防御面非主路径 |
| TIME_WAIT | 2s（缩短） | 对端 SLIRP 非敌意；缩短防 vnet 会话循环拖 smoke |
| in-flight | 1（顺序单发，收 ACK 再发） | echo 64B 单段即完；多段流 v2 |
| 乱序段 | 丢弃（seq != rcv_nxt 即弃，回当前 ACK） | 不缓存；SLIRP 回环几乎不乱序 |

RST 面：收 RST → 任意状态直转 Closed（会话作废标记）；对「不认识的四元组的
入段」回 RST = v2（vnet 单会话、四元组过滤同 DNS 链——id+四元组双滤先例）。

## 5. 驱动几何：RX_BUF_SZ 160→256（本刀唯一 vnet 既有面改动）

帧开销 = 12（vnet_hdr，MRG_RXBUF 已协商）+ 14（eth）+ 20（IPv4）+ 20（TCP）=
**66B** → 现 RX_BUF_SZ=160 的 MSS 上限 94B（太碎）。**扩到 256**：16 描述符 ×
256B = 4096B = RX_BUF_PAGE **恰满单页**——DMA 四页排布零重排、A-7「rx 单描述符
整收」几何结论不变。防御：对端违约发大段 → 设备按描述符容量截断 → TCP 校验和
必失败 → 丢弃靠重传（规范恢复路径，非断言面）。MSS 通告 128 是第一道门
（对端遵守通告是 TCP 规范行为），截断防御是第二道。

## 6. SLIRP 验收面（host 回显服务 + guest 主动 connect）

- **方向裁决：guest 主动 connect 10.0.2.2:PORT**（QEMU user 网标准：10.0.2.2 =
  host 本机，guest 主动 TCP 连接由 SLIRP 转发到宿主 127.0.0.1 同端口）——与
  vnet 既有「guest 主动出站」模型（ARP→网关→DNS）同构；**不用 hostfwd**（那是
  host 连 guest 的反向面，需 listen/accept 服务端状态机，v2）。
- **host 侧**：新 `tools/echod.py` 确定性回显服务——bind 127.0.0.1:18080（定值），
  收即原样回发；收 client FIN 后被动关闭（走 CLOSE_WAIT→LAST_ACK 侧，让 client
  恰好验主动关闭全路径）；无时刻输入、内容全定（mkcpio/mkfat 同纪律）。
- **自验（vnet）**：ARP 复用 → connect → 握手 → 发 64B pattern
  （`berryos-tcp-probe-…`，ICMP 32B 同族）→ 收回显逐字节对拍 → 主动 FIN →
  挥手完成 → 打 `TCP ok echo=64`（**带参形态**——FS ok n= 先例；现有标记表
  无 TCP 前缀，带参仍为保险）→ 会话循环作废、DNS serve 面零扰动。
- **帽值（RPC 成本模型，A-6 方法论）**：全链 ≈ 9-11 帧往返（SYN/SYN-ACK/ACK/
  DATA/ACK/回显/ACK/FIN/ACK×2），TCG 单帧 poll 往返 ~1s 级 → 自验帽 **30s**
  （DNS 20s 先例 + 链更长）；若 T-d 落地，vnet 会话帽 30s < client 帽 60s
  （「服务帽 < client 帽」纪律）。
- **实证前置（A-7 filter-dump 教训，实施刀第一节）**：先 `-object filter-dump`
  抓 pcap 核 SLIRP TCP 面——SYN-ACK 的 MSS 通告值、握手/挥手时序、RST 形态、
  是否补 padding——**断言值钉 pcap 实测之后**，不照规范想象写死。
- **smoke/justfile 接线清单（后续刀执行）**：smoke.sh rootfs/RX/持久化三处
  QEMU 前 `python3 tools/echod.py &` + PID 记录 + 相尾 kill（多会话禁 pkill
  同款）；ROOTFS_MARKERS 加 `TCP ok echo=`（三处计数 18→19 / 41→42 / 41→42）；
  justfile run-rootfs 期望块加行 + recipe 起停 echod（手动面自足）；
  mkcpio 无改动（vnet 已在 rootfs）。

## 7. 拆步（A-8 a/b/c/d 体例）

| 步 | 内容 | 验收 | 标记/测试 |
|---|---|---|---|
| **T-a** | berry-netproto TCP 编解码：encode_tcp/parse_tcp（伪头校验和）+ MSS 选项编/解 + 其余选项跳过容忍 + None 契约 day-one | host 测 | netproto 17→~24 测（预估） |
| **T-b** | `berry-tcp` 状态机纯函数 crate：Conn/on_seg/on_rto + §4 定值表 + 全状态分支 host 测试（握手两向/数据推进/重复段/RST 各态/挥手两侧/窗口恒定） | host 测 | 新 crate ~16-20 测（预估）；host 总 151→~190 |
| **T-c** | vnet 集成：RX_BUF_SZ 160→256（§5）+ tcp.rs 会话循环 + echod.py + 自验 connect/echo/挥手 + smoke/justfile 接线 | smoke 四跑全绿（裸 23 零漂移 + rootfs 19 + RX/持久化 42）+ EL2 + host + 手动 | **`TCP ok echo=64` = rootfs 相第 19 标记** |
| **T-d（可缓）** | netdproto TCP 三 op（OPEN/ECHO/CLOSE，单会话独占 ST_BUSY）+ svcmgr 消费自验 | smoke 追加 | `TCPS ok` = 第 20 标记（落了才打）；「栈服务」话术闭环 |

依赖序：T-a ∥ T-b（纯函数面互不依赖）→ T-c（消费两者）→ T-d。每刀独立走
「实施 → 四跑验收 → workflow 对抗审查 → 修复复验」闭环（八刀同款纪律）。

## 8. 风险与挂账

- **SLIRP TCP 行为差异**（时序/MSS/RST 形态）——§6 实证前置是硬门：pcap 没
  抓过不写断言。
- **重传路径无真实流量验证**：SLIRP 本地不丢包——RTO/退避只能靠 berry-tcp
  host 测试（注入丢段序列）+ 代码审查盖，真实丢包面 G 卡类（真机/外网）。
- **对端违约大段**：截断+校验和失败丢弃（§5 第二道门）——若 SLIRP 不遵守
  MSS 通告则 echo 面永败，pcap 实证时一并核（预期遵守：规范行为）。
- **vnet 常驻 DMA 布局改动**（RX_BUF_SZ 扩容）必须 DNS 面零回归——`DNS ok`
  标记四跑全绿是回归门。
- **单会话独占**：v1 并发 TCP 无面；netdproto T-d 的 ST_BUSY 语义文档化。
- **挂账 v2**：listen/accept（hostfwd 反向）、滑窗/拥塞、乱序缓存、SACK/
  timestamps、多会话、RST 回击、bsh 消费面（wget 类）。

## 9. 话术限定（对表口径）

- `TCP ok` = 最小 TCP 客户端单会话全链（握手/双向/挥手）在 SLIRP 虚拟网跑通
  ——**不等于「完整 TCP/IP 栈」**（无拥塞控制/滑窗/服务端/多会话，§0 非目标
  全表文档化）。
- Phase 6 全量收官判据随 T-c 达成更新（路线图现状行 + `P6收官盘点.md` 执行
  结果区追加）；T-d 落地则「最小 TCP/IP 栈**服务**」口径完整闭环。
- 验证面仍是 SLIRP 虚拟网（真实外网 TCP 未验，G 卡类——盘点话术限定 2 同族）。

---

## 执行结果（追加区）

### T-b（2026-10-22 已落地）：`berry-tcp` 状态机纯函数 crate——「解析器即产品」第 7 例

- **形态**：零分配 / 零 unsafe / 零依赖 / `#![no_std]`、输入只借用（`TcpSeg::payload` 切片）；
  独立小工作区（Cargo.toml 空 `[workspace]` 表，stdhello/hexyl 同款先例）——T-c 时才接线进
  `user/` workspace + justfile test-host（防混合文件提交切分加剧）。
- **状态机与 IO 分离**（smoltcp 只借思路）：本 crate 只做状态与序号推进；出段经
  `Action`（Syn/Data/Fin/Ack——Ack 的 seq/ack 由 vnet 从 `Conn` 现值取）交还调用方；
  定时（RTO 重挂 / TIME_WAIT 到期）由 vnet 侧时钟驱动——纯函数层不吃时钟。
  `TcpSeg` 是自定义输入视图，与 T-a parse_tcp 互不依赖（T-a ∥ T-b 解耦点）。
- **§4 定值表逐项兑现**：MSS=128（通告+收帽，`send` 门控）/ 窗口恒 512（`consts_pinned`
  任意序列后不变断言）/ RTO 3s 起 ×2 退避 3 次帽（`rto_ms` 序列 3000/6000/12000/24000
  钉板；到帽 `on_rto` 放弃转 Closed）/ TIME_WAIT 2s（`expire_timewait`）/ in-flight=1
  单槽（`InFlight` 私有 enum，累计 ACK 下 FIN 覆盖单槽隐含确认数据）/ 乱序丢弃回当前
  ACK（seq != rcv_nxt 即弃，隐式重复 ACK 靠对端重传）/ 收 RST 任意状态直转 Closed。
- **on_seg_open 四步序**（顺序即正确性）：①ACK 推进 snd_una + in-flight 完全覆盖判定
  清槽/退避归零 → ②数据 seq 对齐交付推进 / 否则丢弃，两种都回 ACK → ③FIN 段尾对齐
  rcv_nxt 才收（Established→CloseWait、FinWait1|FinWait2→TimeWait 同时挥手直入、
  CloseWait 重复 FIN 忽略）→ ④FinWait1 && ack && inflight==None → FinWait2。
  序号一律 wrapping 算术防溢出 panic（「永不 panic」同族纪律）。
- **测试**：host 26 测全绿（`cargo test --manifest-path user/berry-tcp/Cargo.toml
  --target aarch64-apple-darwin`）——握手两向/坏 ACK 忽略/MSS 记录/发送门控/数据推进/
  重复段/乱序/ACK 推进与退避归零/RST 三态/被动关闭全路径/主动关闭全路径（含重复 FIN
  重 ACK + TIME_WAIT 到期）/同时挥手直入 TimeWatch/piggyback 数据+FIN 单 ACK/乱序 FIN
  丢弃/RTO 重传与 SYN 重传/退避帽放弃/定值钉板/Closed 幂等。状态名用 TCP 标准拼写
  `CloseWait`（§4 原文 `CloseWatch` 是笔误形态）。
- **实踩两记**：①生命周期 elision——helper 返回 `EvOut<'s>`（deliver 借用输入 seg）时
  elision 默认绑 `&mut self`，三个状态 helper 须显式 `<'s>`；②`let out = c.on_seg(&临时)`
  = E0716（out 携带对临时的借用），须先绑定局部变量再引用。测试序号几何：established
  后先交付数据再收 FIN，FIN 段 seq 必须对齐推进后的 rcv_nxt（首版 consts_pinned 用旧
  几何被 lib 正确拒收——乱序 FIN 丢弃是设计行为非 bug）。
- **下一步**：T-a（netproto TCP 编解码——改既有文件，待用户「提交」解冻后做）→ T-c
  （vnet 集成 + `TCP ok echo=64` = rootfs 相第 19 标记）→ T-d 可缓。

### T-b 对抗审查（2026-10-22 已收官）：13 raw → 4 confirmed 全修 / 9 refuted 留档

- **Workflow**（八刀同款纪律）：17 agents——4 维 find（状态机语义/序号算术/IO 契约面/
  测试盲区）→ 跨维去重 13 raw → 逐 finding 怀疑者对抗核实（「构造可达输入→错误行为
  完整链才 confirmed，拿不准即 refute」）。
- **lib.rs 实现本体零 confirmed**——4 条全在 test-coverage 维度（盲区补钉板，不改实现）：
  - **C0 MEDIUM**（on_rto 的 `InFlight::Fin` 重传臂零覆盖——FIN 丢失唯一恢复路径无护栏，
    §8 指定 host 测试兜底的欠账）：补 `rto_fin_retransmit`——FinWait1（主动 close）与
    LastAck（被动 close）两侧 on_rto 各断言 `Action::Fin{seq}` + backoff 1。
  - **C1 MEDIUM**（LastAck 非覆盖 ACK 分支零覆盖——判据回归成 `>=seq` 误代 `>=seq+1`
    时唯一测试输入 ack_seg(1002) 对两者同真不可检）：补 `lastack_partial_ack_stays`——
    ack=fin 前一字节必须留 LastAck + 槽保留（on_rto 仍发 FIN）+ 重复 FIN 不动 +
    恰 fin+1 才 Closed（off-by-one 探针四断言）。
  - **C2 MEDIUM**（部分 ACK「推进 snd_una 但未覆盖段尾」不清槽 + RTO 从原 seq 全段重发
    零覆盖——丢包恢复核心语义，现有 ACK 测试要么全覆盖要么整体回退）：补
    `partial_ack_keeps_inflight`——send(64) 后 ack=1030：断言 una 推进、action None、
    on_rto 回 `Data{seq:1001,len:64}` 全段重发（前缀重叠可接受）。
  - **C3 LOW**（头注明文契约「数据在途直接 close，FIN 覆盖单槽」零钉板）：补
    `close_overrides_data_inflight`——send(64)+close：FIN seq=1065、on_rto 发 FIN 非
    Data、ack=1065 只覆盖数据尾时槽保留停 FinWait1、恰 1066 才 FinWait2。
- **关键 refuted 留档（R1）**：分段序同时挥手（对端 FIN 先到、我方 FIN 的 ACK 后到）→
  TimeWait 吞 ACK → inflight=Fin 泄漏穿 expire_timewait 进 Closed → on_rto 无 state
  守卫在作废会话重发 FIN——**机械推演成立但 v1 拓扑结构性不可达**（echod 被动关闭
  因果后于收我方 FIN，其 FIN 恒带 ack≥fin+2；CLOSING 态缺失/expire 清槽属 v2 保真性
  备注）。同族 refute：重复控制段静默不重 ACK（我方后续出段恒带 ack=rcv_nxt 的替代
  恢复路径）/乱序 FIN 丢弃不回 ACK（不可达）/snd 回绕语义分裂（ISS 受控无消费者）/
  MSS 收帽未执法（诚实对端+校验和两道防线）。
- **验收**：`cargo test --manifest-path user/berry-tcp/Cargo.toml
  --target aarch64-apple-darwin` **30 测全绿**（26+4）——重传面（FIN 臂/部分 ACK/
  SYN 臂）与挥手边界（LastAck off-by-one/FIN 覆盖槽）全钉板，§8「重传路径只能靠
  host 测试盖」的账清偿。

### T-a（2026-10-22 已落地）：berry-netproto TCP 编解码——伪头校验和 + MSS 选项面

- **API 面**：`TcpFlags{fin,syn,rst,ack}`（PSH/URG 解析侧**静默忽略**——host 栈
  回显段常带 PSH，拒掉会把正常回显段当坏包）+ `Tcp<'a>`（含 `mss: Option<u16>`
  与借用 payload）；`encode_tcp(out, src, dst, sport, dport, seq, ack, flags,
  window, mss, payload)` / `parse_tcp(buf, src, dst)`——签名带 src/dst（伪头
  校验和，`parse_udp` 先例同构）；常量 `IP_PROTO_TCP=6`/`TCP_HDR_SZ=20`/
  `TCP_OPT_EOL/NOP/MSS/MSS_SZ`。
- **编码契约**：MSS 选项只编在 SYN 段（非 SYN 携带 → None）且值非 0（SYN+0 →
  None）；data offset 按 `(20+opt)/4<<4`（选项长度 4 对齐前提）；csum 0 → 线上
  0xFFFF 补档（UDP 同款）；total > u16::MAX 拒；None 拒绝契约 day-one（空 out/
  溢出/参数面全拒）。
- **解析契约**：`buf.len() > u16::MAX` 防御界检最前（IP 面不可达但直接拒）；
  保留位两向（offset 低 nibble / flags 高 2 位）非零拒；伪头校验和**强制复核**
  （csum=0 也过不了——TCP 与 UDP「未算放行」相反的规范语义）；选项遍历步进
  （EOL 后余当 padding / NOP / olen<2 拒 / 越 offset 边界拒），MSS 提取规则 =
  olen==4 且值非 0 且首个（len≠4/值 0 按普通选项跳过）、**不设 SYN 门**
  （「无论段型」——编码侧才管段型纪律，解析侧宽容）；payload 从 data offset
  起借用。
- **测试**：netproto host **17→24 测全绿**（新增 7 测 + `tcp_csum_naive`/
  `tcp_handmade` 两 helper——naive 版换 proto=6 且伪头 len = 段长「TCP 头自身
  无长度字段」与 UDP 的显式 len 域之别；handmade 搓 encode 编不出的形态：PSH/
  URG 位、任意选项组合、坏偏移，csum 事后重算）。覆盖面：SYN+MSS wire 钉板 +
  naive 交叉验证、四形态往返、坏字节/伪头 src 不符/csum=0 拒/0→0xFFFF 补档角
  （搜 sport 使 naive==0——one's-complement 可解必存在）、选项混合步进 + 首个
  有效 MSS + len≠4/值 0 跳过 + 非 SYN 段照提、offset 坏值与保留位探针（改字段
  后**重算 csum** 使目标字段成为唯一拒绝因——relen 闭包同法）、olen<2 用
  `[253,1,0,0]`（裸 `[253,1]` 不行——handmade 的 opts 4 对齐 assert 下 olen 修
  补后 offset=20 使选项区不可见，len 位必须 pad 到 4）、encode 拒面 + 恰贴合 +
  total>u16、PSH|URG|ACK 段解析过且 TcpFlags 只出 ack。
- **下一步**：T-c（vnet 集成：RX_BUF_SZ 160→256 + tcp.rs 会话循环 + echod.py +
  pcap filter-dump 实证前置 + `TCP ok echo=64` = rootfs 相第 19 标记）→ T-d 可缓。

### T-c（2026-10-22 已落地）：vnet TCP 自验会话全链——`TCP ok echo=64` = rootfs 相第 19 标记

- **代码面四件**：①`user/bins/vnet/src/tcp.rs`（会话循环，IO 驱动层——§3 分层
  裁兑：berry-tcp 管状态/序号、本模块管 IO 与时钟）：四元组过滤闭包
  `-> Option<()>` 统一 `?` 早退（杂散帧/SLIRP 周期 IPv6 RA 不误配）；`Arm{end,
  deadline}` RTO 挂臂（run_action 统一挂、on_seg 后按 snd_una wrapping 覆盖或
  Closed 对账清臂——LastAck 例外靠 Closed 兜）；进度门三段（Established 未发
  →send / got==64 且臂清 →close / Closed 且 close 完成 → 打标记 return）；
  TIME_WAIT 循环尾**按状态重挂**（处于 TimeWait 即重挂——v1 环境窗内唯一
  合法到达帧 = 重复 FIN 本就应重挂，杂散段重挂无害且 30s 帽兜底）；组帧
  TCP 段先进 `[u8;152]` 栈缓冲再层层包进 IOB（A-7「组帧在位不再搬」纪律）。
  ②vnet main.rs：`RX_BUF_SZ` 160→256（16×256=4096 恰满 RX_BUF_PAGE 单页——
  DMA 四页排布零重排，rx 单描述符整收几何不变）+ `IOB_SZ` 抽常量（512）；
  `tcp::selftest()` 插在 NET ok 自验与 serve() 常驻之间。③`tools/echod.py`
  宿主 TCP 回显服务（bind 127.0.0.1:18080、单线程 accept、TCP_NODELAY、
  recv(4096)→sendall、空读 = client FIN → 被动关闭——client 恰好验**主动
  关闭**全路径）。④接线：justfile run-rootfs 起/收 echod（mktemp 日志 +
  listening 轮询 + trap EXIT）+ test-host 挂 berry-tcp + smoke.sh 三相共用
  一个 echod 实例（rootfs 相 QEMU 前起、cleanup 统一收）+ `TCP ok echo=`
  入 ROOTFS_MARKERS 尾插（19 元素）+ RX/持久化计数 41→42。
- **SLIRP 实证（pcap filter-dump 硬门）**：对端 SYN-ACK 通告 MSS 1460 但
  **遵守我方 MSS 128 通告**（64B 回显单段不分片）；11 TCP 帧教科书时序
  （握手 3 + 数据 4 + 挥手 4）；我方 win 恒 512、对端 65535；guest 主动
  出站 10.0.2.2:18080 经 SLIRP 转发宿主 127.0.0.1 同端口（user 网标准，
  不用 hostfwd）。EL2 手动与 EL1 行号完全一致（NET ok @132 → TCP ok
  echo=64 @157 → DNS ok @158）——时序确定性极好，DNS 零回归。
- **对抗审查（workflow 14 agents 四维 session-loop/timing/wiring/test-gap ×
  逐 finding 对抗核实，3 confirmed 全修 / 7 refuted 留档）**：①smoke.sh
  cleanup 无条件删 ECHO_LOG——SMOKE_KEEP_LOG=1 只保串口日志时 echod 死因
  （零异常处理，traceback 只落它）恒被抹 → ECHO_LOG 进保留分支 + fail()
  带 echod 日志尾部（失败必带现场）；②TIME_WAIT 重挂注释宣称「事件驱动
  （Ack 动作）」、实现是按状态无条件重挂 → 三处注释对齐实现（tcp.rs 模块
  头注/循环尾注 + berry-tcp lib.rs on_seg_timewait 注——A-3 errno 注记
  先例，零行为风险）；③MSS=128 定值安全真约束（RX 最大段 ≤ RX_BUF_SZ /
  TX 整帧 ≤ IOB）原先只活在注释 → tcp.rs 消费侧两条 const assert + 两个
  尺寸升 pub(crate) 同源（A-4 SECTOR_SZ const assert 同族先例——v2 抬
  MSS 过 186/458 即编译期炸响）。**refuted 留档**：close 进度门只认
  Established（echod 被动关闭是显式契约；**v2 换对端时 close 门要扩
  `matches!(state, Established | CloseWait)`**）；RTO 梯 3+6+12+24=45s >
  cap 30s（cap 恒先到期落「自验超时」loud，失败面正确）；snd_una 回绕
  冻结不在 v1 承诺面；大段 >256 帧被 SLIRP NAT 防火墙挡（宿主中间盒到
  不了 guest）；RST 零 seq/ack 校验 = 草案 §4 明文 v1 定值；echod 死于
  ConnectionResetError 正常流不可达（排障面被 ① 覆盖）。
- **验收**：host **188 测**全绿（11+51+18+13+13+14+24+14+30——berry-tcp 30）
  + smoke 四跑全绿（裸 23 零漂移 + rootfs 19 含 `TCP ok echo=` + RX 42
  注入四段 + 持久化 42 跨 boot f42t + 金丝雀 10 条严格递增）+ EL2 手动
  同绿（21 前缀 + TCP ok 恰一次 @157 + DNS ok @158）+ 手动 run-rootfs
  10s 级出标记。
- **下一步**：T-d（可缓）——netdproto TCP 三 op（OPEN/ECHO/CLOSE 单会话
  独占 ST_BUSY）+ svcmgr 消费自验 `TCPS ok` = 第 20 标记。

### T-d（2026-10-22 已落地）：TCP 服务化——netdproto 三 op + svcmgr 消费自验，`TCPS ok echo=64` = rootfs 相第 20 标记（§9「栈服务」话术闭环，Phase 6 收官刀）

- **代码面三件**：①**`user/berry-netdproto` TCP 三 op 扩面**（host 14→26 测）：
  OPEN `[op][port u16 LE]` 3B（port 0 拒）/ ROUNDTRIP `[op][dlen][data]`
  2+dlen（dlen ∈ 1..=128 = berry-tcp MSS 通告值——对端遵守我方通告，回显
  单段不超帽；0 拒）/ CLOSE `[op]` 1B；应答按形状分两形——**1B `[st]`**
  （OPEN/CLOSE 应答 + 一切非 OK 错误应答）与 **`[st][dlen][data]`**（仅
  ROUNDTRIP 成功，回传服务端**真实收到的字节**——对拍归 client，「DNS
  addr 该是多少归 svcmgr」同款分工）；`TCP_DATA_MAX=128`/`REQ_LEN_MAX=
  TCP_REP_MAX=130`；**烘焙线 TCP 版**：非 OK 状态恒不携 data，编码/解析
  两侧对称执法（线上不存在「超时了但带数据」的歧义形态）；st 合法域含
  `ST_BUSY`（仅 TCP 面——DNS 面编码器不收，无会话语义）；编码器 None
  拒绝契约 day-one（fsproto 对抗审查同族坑前置免疫）。②**vnet serve()
  扩 TCP 三臂**（A-8b 常驻纪律零新面）：`tcp_op` 分派——OPEN 撞在席会话
  → `ST_BUSY`（§8 单会话独占文档化语义）→ miss 时 `TCP_SPORT_NEXT`
  AtomicU16 发号（48081 起，与自验口 48080 错开）`Sess::connect` 新会话
  （**T-c 驱动原样复用——「不长第二套」**）；ROUNDTRIP 无会话 = 协议面
  ST_BAD / `Sess::roundtrip`（`RtRes::Timeout` 会话保命不销毁——
  in-flight 可能仍在途，后续 op 自见分晓）；CLOSE `sess.take()` 无论
  成败弃置（Sess 无 Drop 面纯内存几何，对端超时自收）+ `shutdown` 全
  挥手。服务帽 OPEN 20s / RT 30s / CLOSE 30s 逐 op 各等各的——恒 <
  client 帽 60s（blk_rpc「服务帽 < client 帽」纪律：「服务活着只是慢」
  时 ST_TIMEOUT 应答最迟 ~30s 到，client 帽只兜服务整个挂死面）；**回复
  路径 EBUSY 三清**——随行 handle 回收撞 BERRY_ERR_BUSY 降级 ST_BAD 时
  连带清 addr/dl（「非 OK 恒全零/恒不携 data」烘焙线在服务侧也不许残留
  成功值——DNS 成功后撞 EBUSY 时 addr 不清 → encode_reply None → trap
  服务死，防御面顺带修复）；未知 op / parse 拒一律 DNS 形 ST_BAD（旧
  垃圾消息行为零改动）。③**svcmgr 消费自验**（dns_rpc 样板）：`tcp_rpc`
  三 op 严格串行（encode → send_retry 无随行 → wait READABLE 60s 帽 →
  recv 富余收口 → parse——`TcpReply.data` 借用 recv 缓冲，**拷出 owned
  返回**后缓冲即被下一 op 复用）；`tcp_selftest` = OPEN(18080) →
  ROUNDTRIP 64B PATTERN（与 vnet tcp.rs `PATTERN` 两端各自内嵌同串——
  改任一侧须同步重验）逐字节对拍 → CLOSE **无论 RT 成败必收口**（vnet
  侧 Timeout 不销毁会话，不 CLOSE = 会话泄漏至下轮 OPEN 撞 ST_BUSY）→
  三步全 ST_OK 才打 `TCPS ok echo=64`（NET_A==0 静默跳过 = 裸跑零扰动；
  失败分诊 `eco: tcps rpc` 链路断 / `eco: tcps reply` 链路活答案错）。
  **标记互非子串**：`TCPS ok echo=` vs 自验 `TCP ok echo=` 第 4 字符
  'S' vs ' ' 区分——「栈本身能跑」（T-c，Sess 直连）与「栈经服务化 RPC
  面能被消费」（T-d，netdproto 三 op）两半口径的机器可辨面。接线：
  smoke.sh ROOTFS_MARKERS 19→20（`TCPS ok echo=` 尾插）+ RX/持久化
  42→43 五处 + `DEADLINE_ROOTFS` 240→300s（三 op TCG 逐 op 握手/挥手
  全走真网卡 ≈+15-25s 叠 FATW commit 后 240s 余量见底）。
- **对抗审查（workflow 9 agents 四维 proto/vnet-serve/svcmgr-client/wiring
  × 逐 finding 对抗核实，4 confirmed 全 low 全修 / 0 refuted）**：①TCP
  应答面缺 wire 黄金字节钉板（request/DNS reply 钉板在册、reply 面
  roundtrip 闭环对对称漂移不设防）→ 补 `tcp_reply_wire_pinned`（1B 非 OK
  形 + `[st][dlen][data]` 成功形，25→26 测）；②`TCP_DATA_MAX=128` 与
  `bt::MSS=128` 弹道交汇双定值只活在注释——单侧抬 TCP_DATA_MAX = parse
  放行而 `conn.send` 被 berry-tcp `len > MSS` 拒 → RtRes::Bad → 静默
  ST_BAD 零诊断（MSS 方向另有 consts_pinned 字面钉板把守）→ vnet tcp.rs
  消费侧 const assert 绑定（A-4 SECTOR_SZ 同族）；③「Timeout 会话保命
  ——后续 op 自见分晓」注释与行为相悖（in-flight 占位使后续 ROUNDTRIP
  恒 ST_BAD，仅 CLOSE 可收口：FIN 覆盖单槽）→ main.rs/tcp.rs 两处注释
  对齐实现（A-3 errno 先例）；④DEADLINE_ROOTFS 240→300 改值扫尾漏一处
  smoke.sh 注释 → 历史化（「当时 240s」）。svcmgr-client 维度零 finding。
- **验收**：host **200 测**全绿（11+51+18+13+13+14+24+**26**+30——
  netdproto TCP 三 op 面 14→26）+ smoke 四跑全绿（裸 23 零漂移 +
  rootfs 20 含 `TCPS ok echo=` + RX 43 注入四段 + 持久化 43 跨 boot
  f42t + 金丝雀 10 条严格递增）+ EL2 手动同绿（21 前缀 + TCP ok 与
  TCPS ok 各恰一次 + DNS 零回归，20 标记全到）。
- **下一步**：无——本刀为 TCP 四刀收官，§9 话术限定（「最小 TCP 客户端
  单会话跑通 ≠ 完整 TCP/IP 栈」+「栈经服务化 RPC 面能被消费」）完整
  兑现；多会话/并发 client/被动打开/请求序号防重随 v2。
