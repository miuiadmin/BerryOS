# sim 设备通道与契约设计 v1.1

> sim-S1（PM-17 §3.2 刀序表首列 sim 刀；PM-16 §4.1 契约具体化 + §4.4 步 1
> 开刀清单的**实施设计**）。裁决上游：PM-16（T0 形态 = UART1 字节流 + 消息层
> 原子快照 + 用户态服务 + 契约 crate 三消费）、PM-17 v1.1 §3.3-2（S1 提前，
> 唯一内核触碰 = IRQ 常量 + 派发分支）。本文冻结节拍/归属/帧布局三件，收口
> S1 全部实施级开问题；里程碑切分见 §10。
> **v1.1（M2 实施期修正，2026-11-07）**：四处实测推翻 v1.0 设计面——
> ①UART1 在席门由 PID 探测改为 fw_cfg 具名键（单串口配置下 0x0904_0000 是
> **洞**，盲探 PID = external abort，§2/§4.2）；②svcmgr 礼物对槽位 7/8 改
> 5/6（7 在 EFI-GOP 相被 endB_fb 占，§4.4）；③FIFO 实深 1 非文档值 16B
> （FEN=0，§2）——案 C 论证反而更强；④libc `read(0)` 的 10s 空闲帽 EOF
> 化返 0 与 §6「永睡」冲突 → 服务主循环改 echo 形直收（wait READABLE
> 1h 长帽重臂 + recv，§6）。
> **v1.1 补记⑤（M3 实施期，2026-11-07）**：§7 固定 sock 路径
> `/tmp/berryos-sim1.sock` 仅 run-rootfs 相保持；smoke RX 相 serial1 改
> 用 QMP 同款 mktemp 临时目录 sock（残留文件 bind 拒问题构造性消失 +
> 多跑并行零路径撞）。注入 client `tools/siminj.py` 取 argv[1] sock、
> 缺省回落固定路径——两相同一 client。

## 1. 定位与验收线

sim-S1 = **通道 + 契约面**：UART1（virt 板第二 PL011）注入字节流的 guest
侧全链——内核 IRQ 通道 → 转发 → 用户态 sim 服务 → ACK 回写。验收线
（PM-16 §4.4）：**注入一帧 battery 快照 → guest 打 `SIMBAT ok seq=N` 单消息
原子标记 + 宿主 client 读回 `ACK`**。三设备断言（A1–A9）是 sim-S2/S3 的刀，
S1 只交付它们赖以成立的通道与契约。零号 syscall 消耗（全部走既有
channel/event/wait 面）；v1 35 号口径零违例。

## 2. 通道物象（QEMU 实测，PM-16 §2/§3 + M2 dumpdtb/mtree 复核）

- **UART1 = pl011@0x0904_0000，SPI 8 = GIC INTID 40，LEVEL_HIGH**；存在条件 =
  配置第二个 `-serial`（QEMU 11.0.3 dumpdtb：双串口配置出 `pl011@9040000`
  节点、`interrupts = <0x00 0x08 0x04>`）。**单串口配置该地址是洞**（info
  mtree 无映射）——盲探 PID = external abort = boot 崩。地址属 virt 机型
  稳定 ABI（virt-2.4 起未变）——**无 DTB 可查（`-kernel` 路无设备树），按
  UART0 同款钉板级常量**。
- **在席门 = fw_cfg 具名键 `opt/berry/simdev`**（GUI P2 `opt/berry/display`
  同款拓扑真值纪律）：justfile 配方**同一行**配 serial1 与此键（拓扑契约——
  只配键不配 serial = 门开撞洞）；不带键的 run/smoke/smoke-image/EFI 路零
  扰动（EFI 无 fw_cfg 具名文件面，门恒闭）。
- 每 RX 一 IRQ（电平触发）；**FEN=0（复位默认，v1 不写 LCR_H）= FIFO 有效
  深度 1 字节**（UART0 实测同款——QEMU chardev 流控把 host 输入押在设备
  外不丢字，唯一丢字向量 = 内核环满）。
- 内核早页表以 2MiB Device 块覆盖 `[0x0900_0000, 0x0920_0000)`（
  `mmu.rs build_boot_pt` dev_blocks）——**UART1 页已在图内，零页表工作**；
  PL061（0x0903_0000，S2 GPIO3 电源键面）同块同免。
- 宿主接入 = `-serial unix:<sock>,server=on,wait=off`（serial1 chardev；
  同现有 RX 相 FIFO/QMP 注入模式）。

## 3. 设备归属裁决：内核排水 + 转发，非用户 MMIO 窗（本刀核心取舍）

三案对比后取**案 C（内核排水 + EL1 转发任务 + 纯协议 bin）**：

| 面 | 案 A：服务持 MMIO 窗自排水 | 案 B：IRQ handler 只通知不碰设备 | **案 C：内核排水（定）** |
|---|---|---|---|
| 丢字/活锁 | 帧最长 98B > FIFO 16B：服务排水窗内后到字节静默丢 | 电平 IRQ 在服务排水前重发火；单核同优先级下 EL0 恒被抢占 = **活锁** | IRQ 语境内排空 FIFO 入内核环——**零丢字零活锁**（UART0 同款 26 个月实证形状） |
| 服务形态 | bin 持 MMIO 映射 + 设备寄存器语义 | 同左 | **纯 stdin/stdout bin**（bsh 同形，零设备知识） |
| 新面 | MMIO VMO 授径 + 固定 VA + 用户设备驱动纪律 | 同左 + 活锁无解 | 第二 RX 环 + 第二 console 形任务（全为既有形状的克隆） |
| 唤醒记账 | IRQ→服务直醒（U 类一次跳） | 同左 | IRQ→U 类（task_simcon 醒）→通道→I 类（服务醒）——与 console/bsh 链**同构**，刀 1 记账模型原生表达 |

案 C 的代价 = 内核多一排水面 + 一转发任务；换取零新设备访问路径、字节完整性
构造性保证、服务可完全脱离设备测试（host 对拍契约 crate 即可）。**演进债**：
真机电池/热/modem 驱动走 G-eco 用户驱动族（MMIO VMO 面，svcmgr virtio 先例）
时回收此裁决——S1 的转发任务对真机路径零约束（契约 crate 才是单源，通道端
可换）。wake 归类：`src_of(40) = WAKE_SRC_UART_RX`（与 UART0 同 U 类**归并
不拆**——PL011 同设备类；per-UART 粒度归 0x84 telemetry_open 终身面，刀 2
实施时快照契约已含 per-source 到达计数）。

## 4. 内核面（里程碑 M2 主体）

1. **gic.rs**：`pub const IRQ_PL1_RX: usize = 40`（注释体例对齐 IRQ_PL011_RX）
   + SPI 配置表项 `(IRQ_PL1_RX, 0xa0, 0x01, 0b01)`（电平，同 UART0 行）。
2. **uart.rs**：第二实例静态双份——`RxRing`/`rx_pop_all`/`lost` 计数克隆为
   `uart1_*`（避免泛型化搅动现役 UART0 面）；**在席门 = fw_cfg 具名键
   `opt/berry/simdev`**（v1.0 原案 PID 探测被 §2 洞事实否决——单串口配置下
   盲探即 external abort；门开才调 `uart1_init()` 开 RXIM + CR，serial1 未配
   = 键缺席 = 静默零扰动，拓扑真值纪律 LSN/EVS 同款）；`uart1_rx()`（IRQ
   语境排水 FIFO→环）+ `set_uart1_notify` 槽（返回 bool 供直切检查点，
   `set_rx_notify` 同口）+ `uart1_tx()`（DR 写 + FR.TXFF 有界自旋，ACK 回程
   用）。
3. **trap.rs**：IRQ 40 分派臂 = `wake_src_irq_enter(src_of(40))` →
   `uart1_rx()`（内调 notify 槽：置 UART1_EV 位 + `wake_matching` +
   `note_irq_arrival`，n==0 → `note_irq_no_waiter`）→ `gic::end` →
   （n>0 时）直切检查点 → `wake_src_irq_exit`——PL011_RX 臂逐句同形。
4. **main.rs**：boot 手搭（门 = `rootfs_present` ∧ fw_cfg `opt/berry/simdev`
   键在——virtio VMO 同 rootfs 门再叠拓扑键）：
   UART1_EV Event 对 + 帧通道对（task_simcon→simbattery）+ ACK 通道对
   （simbattery→task_simcon）；simbattery 礼物对进 svcmgr 表**第 5/6 插定值
   0x0001_0005/0x0001_0006**（boot assert 拦表序漂移，console 1/rootfs
   2/endB2 3/virtio 4 之后；v1.0 原案 7/8 被否——EFI-GOP 相 5/6/7 被
   fb/info/endB_fb 占，而 fw_cfg 键在 EFI 不可见 = 两相**结构性互斥**，
   assert 即并存绊网）；simcon 私有表（Event 1/帧 SEND 2/ACK RECV 3）+
   `UART1_EV_REF` 静态（RX_EV_REF 同款）。
5. **task_simcon**（EL1 转发任务，task_console 骨架克隆减编辑面）：
   `wait_many [UART1_EV 位, ack端 READABLE]`（10s 心跳帽）→ 醒即
   `disable_irq` 包络 `uart1_rx_pop_all` + 清位 → `channel_send` 帧端
   （EBUSY→等 WRITABLE 重试，task_console endB2 同款）；ack 端可读 →
   recv 排空 → `uart1_tx` 逐字节写出。

## 5. 契约帧布局（v1 冻结；单源 = `user/berry-pwproto`）

共同头 8B `[magic u32 LE "PWSM"][dev u8 1/2/3][ver u8 =1][seq u16 LE]` +
定长全量快照 + `changed_bits u32`（对齐 goldfish INT_STATUS 事件位图）。
多字节全 LE。三设备快照定长（PM-16 §4.1 表逐字段落位）：

- **电池 50B 帧 / 38B 载荷**：present/ac_online（布尔 0/1）/status（≤3 四
  态）/health（≤7 八态）/capacity_pct（≤100）/voltage_mV u16/temp_mc i32/
  current_now_uA i32/current_avg_uA i32/charge_full_uAh u32/
  charge_counter_uAh u32/cycle_count u16/timestamp_ns u64/measured（P8
  实测/推算位）。
- **热 98B 帧 / 86B 载荷**（FRAME_MAX）：sensor_count ≤2；per 传感器 42B =
  kind（0 real/1 virtual）+ temp_mc i32 + trip_count ≤4 + trips×{level ≤3 +
  threshold_mc i32 + clr_mc i32}（**clr ≤ threshold 迟滞窗契约**）；尾
  level_hint u8 四档。
- **modem 19B 帧 / 7B 载荷**：rrc_state u8/tail_ms u32/signal_tier u8/
  in_maint_window u8。
- **ACK 9B**：头 + st u8（0 已应用/1 解析·值域拒/2 seq 回退）——宿主可断言
  注入真到达（对抗回声歧义：断言吃 ACK 不吃注入回显）。
- **流式解码**（`StreamDecoder` 逐字节喂）：魔数失配/未知 dev = 杂音**静默
  再同步**（丢 1B 重扫）；头合法帧收齐但语义拒（ver/值域）= 显式 yield
  `Bad`→服务回 ACK ST_BAD——「没收到」（无 ACK）与「收到但拒」（BAD）线上
  可分。值域执法编码/解析**双向对称**（线上不存在编码器收、解析器拒的形态）；
  黄金字节钉板（全零电池帧 seq=1 的 50B 线值）host 测试冻结，宿主注入
  client 共享此基线。
- seq 防重放/防陈旧：消费侧自持 last_seq，回退 → ACK STALE（状态机在服务，
  契约只承语义）。

## 6. 服务面（`user/bins/simbattery`，musl std bin，bsh 同产链）

- **spawn**：svcmgr boot 序（rootfs 相；门 = 0x0001_0005 在席——裸相/表缺
  插自然缺席，拓扑真值；EFI 相 fw_cfg 键不可见恒缺席）；`SpawnSpec {
  stdin_handle = Some(帧端 5 插)，extra_gifts = [ACK 端 6 插]，stdout =
  console 发端，… }`——fd0 = 帧字节流（与 bsh 键盘行同形）、fd1/2 = 直打
  serial0。
- **主循环**：echo 形直收（`berry_channel_recv` → 空 -EAGAIN →
  `wait_one` READABLE **1h 长帽重臂** → 重收；v1.1 修正④：libc `read(0)`
  10s 空闲帽 EOF 化返 0，与「永睡」冲突，弃用）→ 逐字节
  `StreamDecoder::push` → `Frame`：dev 须 battery（他设备帧拒 BAD）、
  seq 单调（首帧或 > last → ST_OK 并更新；否则 ST_STALE 不应用）→ ACK
  写 gift[0] 端（channel_send，EBUSY→wait WRITABLE 重试）→ **仅 ST_OK**
  打 `SIMBAT ok seq=N\n` 单消息 stdout（A60 纪律：整行一条 channel 消息，
  console 单写者拼行，不走逐字节分片；BAD/STALE 无 SIMBAT 行）；`Bad` →
  ACK ST_BAD。无注入 = 永睡，零唤醒（1h 重臂在 10s 粒度 wake 记账下为零）
  零标记（B4/裸相零扰动）。
- 标记互非子串核对：`SIMBAT` 与现役标记两两互非前缀（`SIG ok`/`INPUTP`/
  `INT ok` 首字符即分；裸 grep 足够，`(^|[^A-Z])SIMBAT` 兜底不加——现役
  无 `-SIMBAT` 形）。

## 7. 宿主面（里程碑 M3）

- `justfile` run-rootfs：+`-serial mon:stdio -serial unix:/tmp/berryos-sim1.sock,
  server=on,wait=off`（serial0 显式钉 stdio 防吞、serial1 = 注入通道；路径
  固定供手跑 client 缺省复用——smoke RX 相改 mktemp 临时目录 sock，补记⑤
  ——recipe 起手 rm -f 残留 socket）+
  `-fw_cfg name=opt/berry/simdev,string=1`（在席门键，与 serial1 同线配对）。
- 注入 client（python，抄 qmp_inject socket 模式）：连 sock → 发黄金帧
  （或参数帧）→ **读回 ACK 9B 对拍**（ST_OK + seq）→ 串口日志 grep
  `SIMBAT ok`。挂点 = RX 注入相内追加段（serial0/serial1 两串口独立，
  bsh 会话零干扰，不增 QEMU 相）。
- 标记三镜：markers.sh 双数组 + `SIMBAT`、smoke-image/EFI 计数、
  规则/07 相构成表——与 `RX ok` 同批入册。

## 8. 测试面

- **host（M1，已落）**：pwproto 9 测——三设备往返/ACK 往返+拒面/编码器
  短缓冲+值域 None/流式逐切点钉板/杂音+伪头再同步/收齐坏帧显式 Bad/
  ver 漂移 Bad/黄金字节。
- **QEMU（M2 手验 → M3 自动）**：注入 50B 电池帧 → serial0 见
  `SIMBAT ok seq=1` + client 读回 ACK OK；再注 seq=2 → seq=2；注坏值帧
  （capacity=101）→ ACK BAD + 无 SIMBAT 行；重注 seq=1 → ACK STALE。
  负例反转红（改 decoder 值域一条 → BAD 流必红）随 M3 一次验证。

## 9. 偏差与在案决定（对 PM-16 原案）

1. 设备归属内核（§3 案 C）——原案「用户态服务」语义保留（协议/状态机/
   标记全在用户态），设备寄存器面 v1 归内核；真机驱动路径演进债登记。
2. wake 归类 U 类归并不拆（§3）——枚举零漂移，0x84 终身面补粒度。
3. GIC SPI40 常量+表项 = 内核触碰面，超 PM-17 §3.2「仅 IRQ 常量+分支」
   原文一行（另含 SPI 表项 + uart1 排水面）——同形扩张（UART0 全套克隆），
   非新机制位；在案备查。
4. sim-thermal/sim-modem 的服务面消费（自报接口上送）不在 S1（无消费者，
   避免「建铸号无消费者」覆辙——裁决 §1 问①评语同源）；S1 只承契约帧的
   编解码单源。

## 10. 里程碑切分（每步独立可验、单独成提交）

| 步 | 内容 | 验收 |
|---|---|---|
| **M1** | pwproto 契约 crate + host 9 测 + workspace/test-host 接线 + 本设计文档 | `just test-host` pwproto 行绿 |
| **M2** | 内核面（gic/uart1/trap/main 接线/task_simcon）+ simbattery bin + svcmgr spawn + run-rootfs serial1 | 手验四流（OK/递增/坏值 STALE） |
| **M3** | smoke 注入 client + RX 相追加段 + 标记三镜 + 负例反转红一次 | `just smoke` 全绿含 SIMBAT |

M2 起 kernel/src/main.rs 与兄弟 X-3 pager spawn 段同文件异区——pathspec
提交 + 逐 hunk 归属核查（判例 A125）。
