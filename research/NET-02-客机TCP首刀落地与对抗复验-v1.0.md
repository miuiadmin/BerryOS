# NET-02 客机 TCP 首刀落地:入站大段容忍与准入一致性——实施与对抗复验证据链(2026-11)

> 版本:**1.0**(v1.0)｜2026-11-15｜文件名 `NET-02-客机TCP首刀落地与对抗复验-v1.0.md`
> **定位**:[NET-01 客机 TCP 栈深研与裁决](NET-01-客机TCP栈深研与裁决-v1.0.md) 四层裁决中「收发面 extend 首刀」的实施波证据链——裁决所依三例黑洞(256B/160B 单写拒收死锁)的修复落地、几何整链抬升的实测效果、准入「宁拒不腐化」预检的压满复验,与五标记链零回归取证;裁决本身(NET-01 §5)不变,吞吐承诺不变(停等 486 B/s 上限,发送面零改动)。
> **证据分级**:沿用 NET-01 口径——【证】= 本波实跑输出/一手行号;【走】= 走查;【界】= 工具边界。

---

## 0. 总判断

首刀按 [plan](../.agents/plans/2026-11-15-客机tcp深研首刀.md) 落地:入站几何整链五点同抬(RX_BUF_SZ 256→1600、RX_BUFS 16→5、拒收线锚新常量 RX_SEG_MAX=1460、Sess.rx 512→2048、IOB_SZ 512→1600)+ 会话准入预检(收缓冲放不下整段时拒段回当前 ACK,不喂状态机)。宿主门全绿(berry-tcp 43/netdproto 42/netproto 24/test-host 1781 全绿零失败,`berry-tcp src/lib.rs` 零 diff);对抗复验三例全过(256B/1400B 单写全达回显逐字节对拍;8KB 压满触发预检拒段 2 次后全量恢复零错位);五标记链全绿(NET/TCP/LSN/DNS/TCPS 走正牌 `just run-rootfs`,EVS 走 apps 减配私有链——既有环境 flakes 见 §4 偏差)。黑洞解除,吞吐不变(~200 B/s 服务端 echo,与二刀判决一致)。

## 1. 改动面(全部锚点为落完后行号)

**`drivers/bins/vnet/src/main.rs`**(A 面):

- RX 几何(L72-84):`RX_BUFS` 16→5、`RX_BUF_SZ` 256→1600(12B vnet_hdr + 1514 满以太帧 + 余量;5×1600=8000 ≤ RX_BUF_PAGE 8192,「rx 缓冲带单页内、单描述符整收一帧」保持)。
- IOB 联动(L146-152,plan 缺环、实施中发现):`IOB_SZ` 512→1600 + 6 处硬编码 `[u8; 512]` 类型标注换 `[u8; IOB_SZ]`——`recv_frame` 整帧 vmo_read 进 IOB,RX_BUF_SZ 抬升而 IOB 不抬即静态缓冲溢出;tcp.rs 第 7 处同换。

**`drivers/bins/vnet/src/tcp.rs`**(B 面):

- 常量(L54-65):`RX_SEG_MAX=1460`(拒收线新锚,与 bt::MSS 发送帽解耦)、`RX_SESS_BUF=2048`(与 WINDOW=512 通告解耦)。
- const assert 换锚(L74-83):RX 侧改钉「满 MTU 段 + 帧开销 + vnet_hdr + 选项 ≤ RX_BUF_SZ」(旧钉「对端按我方通告回段」假设已被 NET-01 证伪);新增第三钉 `RX_BUF_SZ ≤ IOB_SZ`。
- `parse_gw_tcp` 拒收线(L557):`n > bt::MSS` → `n > RX_SEG_MAX`;doc 注释同步(拒收线锚实际链路几何而非对端通告)。
- 会话准入预检(route_seg L616-631,本刀核心):`s.rx_len + seg.payload.len() > s.rx.len()` → 不喂状态机、回当前 ACK(重复 ACK)、槽原样回写、loud 打印;空段恒过检(纯 ACK/FIN 无收缓冲面)。
- session-0 同构预检(step L391-412):`seg.payload.len() > rcv.len()` → 拒段回 ACK 跳过 on_seg;客户端 op 契约收等长 ≤MSS 回显,超规格回显语义外 = 相位帽 TIMEOUT(不再有截断腐化面)。
- `rx_push` 防御 trap 化(L195-207):预检后溢出结构性不可达,原「丢新字节 + loud」是到达即已腐化的静默路径(rcv_nxt 已推进,截断 = ACK 谎报),改 trap。
- `Sess.rx` 扩容(L128/L185/L192):`[u8; RX_SESS_BUF]`;TCPTABLE 注 L493-495。
- pbuf 两处(L388/L713):`[0u8; bt::MSS]` → `[0u8; RX_SEG_MAX]`。

**`user/berry-tcp/tests/state.rs`**(C 面,`src/lib.rs` 零改动):补 2 测——`large_seg_in_order_delivered`(1460B 满段按序全量交付 + rcv_nxt 推进 1460 + 单 ACK)、`large_seg_then_regular_continuity`(大段后接常规段序号衔接 + 乱序丢弃语义不漂移对照)。

零改动面(验收 3):`berry-tcp/src/lib.rs`、`berry-netdproto/`、`user/berry-libc/src/net.rs`、justfile、smoke 链——`git diff` 证改动仅 main.rs/tcp.rs/tests/state.rs 三件(+148/−45)。

## 2. 测试矩阵全格(判据→实跑)

| 面 | 判据 | 实跑【证】 |
|---|---|---|
| 宿主单测 | berry-tcp 41+2 / netdproto 42 / netproto 24 全绿 | 43/42/24 全绿(`cargo test -p … --target aarch64-apple-darwin`) |
| 宿主全量基线 | test-host 基线保持零失败 | **1781 passed / 0 failed**(基线随并行线自 1751 增长,零回归) |
| 段级 harness | sm-harness 15 全绿(berry-tcp src 零改动的回归钉) | 15/15 绿 |
| 状态机大段交付 | C1 新测绿 | 43 含 2 新测绿 |
| 对抗:黑洞解除 | 256B 单写从「0B 交付 + close TIMEOUT」变全达对拍 | `T: ev echo sid=2 rx=256B tx=256B` + 宿主 `echo=OK`,首回显 1.31s |
| 对抗:黑洞解除 | 1400B 单写臂同判据(新增臂) | `T: ev echo sid=3 rx=1400B tx=1400B` + `echo=OK` |
| 对抗:准入不腐化 | 压满拒段后恢复,无字节错位 | 8KB 压满:`vnet: tcp 准入拒段 sid=6 rx_len=2048 seg=128 cap=2048` ×2 → `rx=8192B tx=8192B` + `echo=OK`(逐字节) |
| 五标记链 | 五标记全打零回归 | NET/TCP/LSN/DNS/TCPS 正牌 `just run-rootfs` 全绿;EVS 走 apps 减配私有链 `EVS ok n=2` + `EVS client ok n=2`(§4 偏差) |
| boot 冒烟 | 静态增量后 boot 链完整 | 私有链 13 标记全绿(TAGTAB@2.3s … EVS@51.7s);.bss 实测 +9536B、.rodata +144B、文件 +752B |
| 文档门 | check-docs --strict 0 告警 | 13 条全部源于并行会话在途删除 `docs/显示与加速体系设计-v1.0.md`(改动前基线同数);本波文档增量零新增告警 |

对抗环境:私有 fixture(复用 NET-01 工具链 `/tmp/berry-adv`:bootdrv 私有 QEMU + hostfwd 19181→18081/19182→18082/19501/19502 + echod 19180 + evsclient 19182;vnet-priv = 首刀源 + `adv_remap_out` 出站口 18080→19180/18081→19181 两行,状态机与 IO 零改动)。客机 echo 面为 bsh 内建 `tcptest echo`(LISTEN/ACCEPT/POLL/DRAIN/RT/CLOSE 全 netd 八 op 面);宿主侧 `advcli.py thr`/自写 `advburst.py`(单 syscall 全量连发)。串口带时戳全量留 `/tmp/berry-adv/serial.log` 与 `drv.log`。

## 3. 实测中新钉的事实

- **预检拒段路径实证**:8KB 连发时 SLIRP 在途推进与客机 DRAIN 节奏赛跑,收缓冲精确压满(rx_len=2048=cap)时后续 128B 段被拒 2 次、对端重传在 DRAIN 腾空后到达——字节流完整、序号无错位。拒段打印本身即「宁拒不腐化」闸门活体证据。
- **SLIRP 分段行为收窄**(对 NET-01 §3.1「SLIRP 不理通告」的精化):本波观测拒段 `seg=128`——服务端路径上 SLIRP 按我方 MSS=128 通告分段、按 WINDOW=512 限在途;NET-01 的数百 B 大段拼段发生在其 px-dup 臂特定形态。结论不变(1460 拒收线覆盖一切观测形态),但「恒不理通告」过强,分段策略仍属未穷尽边界。
- **黑洞两道闸归因维持**:首刀整链抬升后,`recv_frame` len 检查与 `parse_gw_tcp` 拒收线均不再吃 >128B 入站;即使超限帧(IP/TCP 选项使帧 >1600)出现,第一道闸拒收 = 拒帧不死锁,交付语义仍一致。
- **吞吐不变(按设计)**:三例对拍吞吐 158/192/204 B/s——停等上限(MSS 128 + in-flight=1 + 串行 RPC)未动,二刀(发送窗消费 + in-flight>1 + 多段化)是 VNC 前置不变。

## 4. 偏差与既有 flakes(如实记录)

- **EVS 标记走私有链**:正牌 `just run-rootfs` 两趟(改动前 22:19 基线趟 + 本波趟)均在 bsh u3 step=6 挂「sort/uniq spawn failed」——既有内存压 flakes(改动前同形实证,与本刀无关;u3 bad 即 `park()` 断链,EVS 结构性不可达)。apps 减配私有 rootfs(34.1MB,掉 10 个 .bap + tcptest 备用件)后 u3 过、`EVS ok n=2` 全绿。归属等价:vnet/tcp.rs 代码路径同一(仅 hostfwd 端口映射与 rootfs 非网络条目不同)。
- **重试趟死于并行会话在途编译错**(berry-launcher E0425,非本波面);本波第一趟 `just run-rootfs` 在一致性树态下完成,四标记证据有效。
- **check-docs 13 条告警**:全部为并行会话删除 `docs/显示与加速体系设计-v1.0.md` 造成的 `docs/调度彻底计划-v1.2.md` 断链(3 处 ×多链接),本波文档增量零新增;该删除非本波动作,不代修。
- 工具边界承 NET-01:真丢段域 RTO/乱序臂仍结构性不可注入;bootdrv FIFO 消费循环有缓冲积压 bug,本波已修(裸 `os.read` 按行切,私有件不入仓)。

## 5. 后续(移交二刀)

发送窗消费 + in-flight>1 + MSS/RPC 多段化(吞吐 486 B/s 域 → 10KB/s 域)是 VNC 服务化前置;服务面三 patch(泵节奏/槽超时/netdproto 请求序号)与真网卡域重开项(乱序重组/窗口执法/RST 回陌生四元组/序号回绕)见 [plan](../.agents/plans/2026-11-15-客机tcp深研首刀.md) 未完成事项节。
