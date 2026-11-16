# PM-16 sim 设备族与九断言挂 G0–G5——把 PM-10 验证结论压成可开刀的实施素材（v1.1）

> 调研日期：2026-11-07 · 版本 v1.1（sim-S1 收官 + sim-S2 开刀触发升版）。
> v1.1 修订：S1 M1–M3 已收官（berry-pwproto/UART1/simbattery/SIMBAT/ACK/smoke-image 负例镜）；S2 A2/A4/A8 已按 §4.4 步2 落地，载体偏差见 `docs/PM仿真S2断言实施设计-v1.0.md`。方法：联网取证（WebSearch/WebFetch 中英并用，直达 QEMU master 源码、Linux 内核 UAPI、OASIS 规范、GitHub changelog）+ **本地 QEMU 11.0.3 实机 DTB 转储实证**（dumpdtb + dtc 反编译）+ 仓内实查（tools/ smoke 体系、kernel/arch 遥测与中断面、justfile/CI 构建面逐行核对）。
> 确定度图例：**高** = 官方文档或源码逐字读到 / 本地实机复现；**中** = 权威来源转述未逐字核到原文；**低** = 合理推断。
> 分工：PM-10 §4 给了九断言与 sim 三设备**草案**；PM-12 §4 开放问题 6/7（sim 实现路径、KVM CI）留裁。本文只做一件事：**把草案压成可开刀素材**——实现路径实证收口（方案赛裁决建议）、三设备契约落到寄存器/命令面、九断言逐条给挂接点/前置/脚本形态/判据、开刀清单细到文件级。不重抄 PM-10 的业界验证史（三层模型/pm_test/suspend_stress_test 结论照引）。

---

## §0 一页结论（TL;DR）

1. **sim 实现路径裁决建议 = Tier-0「零 QEMU 改动」先行**：改 QEMU 源（fork，goldfish 先例）与 out-of-tree（上游**无**设备模块支持，实锤）都不选——因为 BerryOS 现用**系统包 QEMU**（本机 brew 11.0.3、CI apt qemu-system-arm，`justfile:18`/`ci.yml:48`），fork 意味着双端自建+上游漂移双税，而九断言的注入需求 Tier-0 通道全覆盖。
2. **PM-10「virt 板无唤醒源」需修正**：本地 QEMU 11.0.3 DTB 实证——virt 板有 **PL061 GPIO（0x09030000，SPI 7 = GIC ID 39）**，QMP `system_powerdown` 在 `-kernel` 路径（无固件→无 ACPI GED）**脉冲 GPIO3 电源键**（gpio-key 设备自复位 ~100ms 脉冲）= **免 fork 的可注入唤醒线**；另有**第二 UART（0x09040000，SPI 8 = GIC ID 40）**配置第二个 `-serial` 后即存在 = 免 fork 的字节流注入通道。真正缺的只是「QEMU 级挂起态的唤醒」（TCG 无 SYSTEM_SUSPEND，VM 从未真睡）——那本来就只能 KVM/真机测。
3. **三设备契约落法**：sim-battery/sim-thermal/sim-modem 不做 MMIO 寄存器设备，做**用户态服务 + 单一契约头文件**（建议 `user/berry-pwproto/` 新 crate）：消息层带「原子快照 + 递增序号」（在消息层实现，彻底规避 goldfish 逐字段寄存器写的顺序坑）；字段面照抄 goldfish battery 寄存器集 ∪ Fuchsia BatteryInfo 15 字段；**绿传递律落点 = sim 驱动、未来真机驱动、berry-sys 只读面三处消费同一 crate**——仿真专用分支禁止进主线（PM-12 回填 #16-⑬）。
4. **九断言分三批挂**：现成通道即可落的 A2/A4/A8（第一批，刀 1 收尾期）；需 timer slack 机制位的 A3、需 gui-inputd 的 A6/A6b（第二批）；需 M4 维护窗/berry-power 的 A5、需 power·mode op 的 A9（第三批）。全部状态谓词零竞速（判例 A32 纪律），时序敏感项全走 `-icount` 相。
5. **挂起三档现实收口**：boot 期 `PSCI_FEATURES(0x8400000A)` 探测 + 能力表进遥测（TCG 预期 NOT_SUPPORTED → 深睡请求断言「优雅降级=冻结循环 + 降级事件在册」）；KVM 相进 CI 的路径 = GitHub **ubuntu-24.04-arm 免费公有仓 runner**（2025-01 起），但 `/dev/kvm` 在该 runner 上是否放行**未证实** → 列 spike（开放问题 6 的可执行解）。

---

## §1 主线全景：sim 设备实现路径的实证收口

### 1.1 三路方案赛对照（刀 1 前置 PM-12 §3.3-6 的裁决轮）

| 维度 | T0 现有设备顶替（零 QEMU 改动） | T1 fork QEMU 加 berry-sim MMIO 设备 | T2 fork QEMU 加 virtio 实验 ID 设备 |
|---|---|---|---|
| 形态 | 用户态 sim 服务 + 既有通道（UART1/fw_cfg/QMP/宿主 socket）注入 | goldfish 模板：固定地址 MMIO 窗 + 1 条 IRQ（[Emulator Internal Book](https://github.com/aospbooks/android-emulator-internal-book/blob/main/06-virtual-hardware.md) 原话「a window of MMIO registers at a fixed physical address plus one interrupt line」） | virtio-mmio 传输 + 自定 Device ID（512+ 实验段）+ 自写设备模型 |
| QEMU 维护成本 | **零**（系统包照用） | 双端自建（macOS 本机 + Ubuntu CI 容器/源码构建）+ 上游漂移跟进 | 同 T1 + virtio 队列逻辑复杂度 |
| guest 侧新码 | sim 服务 + 契约 crate + UART1/GIC 接线 | PL061 式 MMIO 驱动（形态最像真机） | 复用 berry-virtio 传输层（`drivers/berry-virtio/src/lib.rs:197-240` 寄存器偏移表现成） |
| 快照原子性 | 消息层 seq+全量替换（**天然原子**） | 寄存器组写序需设备侧保证 | config generation counter（规范 §4.2 CONFIG_GEN `0xfc` 现成） |
| 唤醒事件语义 | UART1 每字节 1 IRQ / GPIO3 100ms 脉冲 | 专用 IRQ + doorbell FIFO（最干净） | virtqueue 中断（事件=FIFO 语义） |
| 先例 | Android emulator console 注入（PM-10 事实 6） | goldfish 设备族（PM-10 事实 6） | virtio-fs 先 ID 26 保留后标准（[virtio-spec issue #31](https://github.com/oasis-tcs/virtio-spec/issues/31)） |
| 裁决建议 | **✅ 先行**（覆盖九断言全部注入需求） | 押后：真机契约冻结评审时若「物理域语义=一条 IRQ 一个事件」成为硬需求再立项 | 押后：对「快照+事件」极简面是杀鸡用牛刀；v1 结束前无真机则 T0 即终态 |

**out-of-tree 实锤**：QEMU 无官方 out-of-tree 设备模块机制——设备模型（QOM TypeInfo 注册、`sysbus_create_simple` 挂机）必须编进 QEMU 二进制；TCG plugin API 明确只做 instrumentation 不做设备仿真（社区讨论 + [QOM 文档](https://qemu-project.gitlab.io/qemu/devel/qom.html)）【中】。结论：**「不改 QEMU」与「fork QEMU」二选一**，没有第三条「外挂模块」路。

### 1.2 仓内 QEMU 使用现状（决定 fork 成本的底账）

| 项 | 事实 | 出处 | 确定度 |
|---|---|---|---|
| 本机 QEMU | 11.0.3（brew，`QEMU emulator version 11.0.3` 实测） | 本地 `--version` 输出 | 高 |
| 命令入口 | `QEMU := "qemu-system-aarch64"`（PATH 系统包，无自建/钉场版本文件） | `justfile:18` | 高 |
| CI QEMU | Ubuntu apt `qemu-system-arm`（版本随发行版漂） | `.github/workflows/ci.yml:48` | 高 |
| 机型 | `-machine virt` + `-kernel` ELF 直加载（非 Linux boot 协议，无 DTB 递送） | `arch/aarch64/src/fwcfg.rs:5-9` | 高 |
| virtio 面 | virtio-mmio 32 槽窗 [0x0A00_0000,0x0A00_4000) 包成 MMIO VMO 授 svcmgr（第 4 插 0x0001_0004）；设备落位随 `-device` 顺序漂移，消费端扫窗 | `kernel/src/main.rs:1259-1288` | 高 |
| 固件 | AAVMF（brew 路默认 + CI 环境变量覆盖）→ EFI 相才有固件 | `justfile:840` · `ci.yml:233` | 高 |

含义：**fork QEMU 不是改一行的事，是把「系统包即用」换成「双端构建管线」**（CI 增源码构建或私有容器镜像步骤、本地 brew 替自建 + 升级跟版）；T1/T2 的收益必须大到盖过这条固定成本才立项。

### 1.3 Tier-0 现有注入通道清单（本地 QEMU 11.0.3 实证）

实证方法：`qemu-system-aarch64 -M virt -cpu cortex-a72 -serial null -serial file:... -machine dumpdtb=...` + `dtc` 反编译（2026-11-07 本机执行，DTS 摘录见 §2）。

| 通道 | 板级事实 | 触发方式 | 通道语义 | 适用断言 |
|---|---|---|---|---|
| **UART1 第二串口** | pl011@9040000，SPI 8（GIC ID 40），alias serial1；配第二个 `-serial` 即存在（无 TrustZone 时） | 宿主写 chardev（unix socket/FIFO，同现有 RX 相 FIFO 注入模式） | 字节流（自定命令协议）+ 每 RX 一 IRQ | sim 三设备命令面主通道 |
| **PL061 电源键** | pl061@9030000，SPI 7（GIC ID 39）；gpio-keys/poweroff 挂 GPIO3（KEY_POWER 0x74）；`system_powerdown` → 无固件路径 `qemu_set_irq(gpio_key, 1)` → gpio-key 自复位 ~100ms 脉冲 | QMP `system_powerdown`（可重复） | 电平脉冲 = 电源键事件 IRQ | A4 无主唤醒正向对照、A3 唤醒风暴、A9 状态机 OFF 态入口 |
| **UART0 RX** | pl011@9000000，SPI 1（GIC ID 33）——`arch/aarch64/src/gic.rs:30-31` 已接线 | FIFO stdin（smoke RX 相八段注入现成） | 控制台字节流 | A6 键盘类事件注入 |
| **QMP input-send-event** | virtio-tablet（GUI-4 段② INPUTP 通道现成） | `tools/smoke.sh:1041-1078` qmp_inject 三批事件 | 触摸/键鼠事件 | A6 触摸风暴 |
| **宿主 socket 对端** | virtio-net + hostfwd（soak 三件套 echod/evsclient/sktclient 现成） | 宿主进程收发 | 网络事件 IRQ | A5 维护窗流量注入 |
| **fw_cfg 具名项** | `opt/berry/rootfs` 同机制（`fwcfg.rs:35` 单源）；boot 一次性 | `-fw_cfg name=opt/berry/sim,file=...` | boot 期脚本/配置（无 IRQ） | sim 注入剧本装载 |

坑注：GPIO3 通道在 **EFI/AAVMF 相不可用**（`firmware_loaded` 时 acpi_dev=GED 创建，powerdown 走 ACPI 事件不走 GPIO——`hw/arm/virt.c:3225,1593-1598`）；PM 断言全部挂 `-kernel` 相即无此问题。guest 侧 PL061 中断为锁存型，脉冲后须写 ICR 清锁才收下一发——风暴注入节奏设计时计入。

---

## §2 关键事实与证据（汇总表）

| # | 事实 | 确定度 | 出处 |
|---|---|---|---|
| 1 | virt 板 UART1=0x09040000/IRQ SPI8、UART0=0x09000000/SPI1、GPIO=0x09030000/SPI7 | 高 | [hw/arm/virt.c:201-267](https://raw.githubusercontent.com/qemu/qemu/master/hw/arm/virt.c) + 本地 dumpdtb 实证 |
| 2 | gpio-keys poweroff 键挂 PL061 GPIO3（linux,code=0x74 KEY_POWER） | 高 | 本地 DTB 反编译（dts `gpio-keys{ poweroff{ gpios=<0x8004 0x03 0x00> }}`）+ [virt.c:1615-1618](https://raw.githubusercontent.com/qemu/qemu/master/hw/arm/virt.c) |
| 3 | `GPIO_PIN_POWER_BUTTON = 3` | 高 | [include/hw/arm/virt.h](https://raw.githubusercontent.com/qemu/qemu/master/include/hw/arm/virt.h) |
| 4 | `system_powerdown`：无 acpi_dev → 脉冲 gpio_key；acpi_dev 仅 `aarch64 && firmware_loaded && acpi on` 时创建 | 高 | virt.c:1589-1598 · :3225（master 实读） |
| 5 | gpio-key 设备 = 置线后 IRQ 拉高 100ms 自落（自复位脉冲） | 高 | [hw/gpio/gpio_key.c 头注](https://raw.githubusercontent.com/qemu/qemu/master/hw/gpio/gpio_key.c)「raised for 100ms before being dropped again」 |
| 6 | UART1 创建条件 = `serial_hd(1)` 已配且无 TrustZone | 高 | virt.c:3197-3206 + [virt 文档](https://www.qemu.org/docs/master/system/arm/virt.html)（「second NonSecure UART exists only if a backend is explicitly configured」） |
| 7 | QEMU 无 out-of-tree 设备模块支持；设备模型必须进树 | 中 | [QOM 文档](https://qemu-project.gitlab.io/qemu/devel/qom.html) + 社区讨论（§1.1） |
| 8 | goldfish battery 寄存器图：0x00 INT_STATUS/0x04 INT_ENABLE/0x08 AC_ONLINE/0x0C STATUS/0x10 HEALTH/0x14 PRESENT/0x18 CAPACITY/0x1C VOLTAGE/0x20 TEMP/0x24 CHARGE_COUNTER/0x28 VOLTAGE_MAX/0x2C CURRENT_MAX/0x30 CURRENT_NOW/0x34 CURRENT_AVG/0x38 CHARGE_FULL_UAH/0x40 CYCLE_COUNT | 高 | [drivers/power/supply/goldfish_battery.c:34-57](https://raw.githubusercontent.com/torvalds/linux/master/drivers/power/supply/goldfish_battery.c)（实读） |
| 9 | goldfish 设备 = 固定地址 MMIO 寄存器窗 + 一条中断线 | 高 | [Android Emulator Internal Book ch6](https://github.com/aospbooks/android-emulator-internal-book/blob/main/06-virtual-hardware.md) |
| 10 | virtio 标准 Device ID 已分至 48（media；1=net/2=blk/18=input/26=fs…） | 高 | [include/uapi/linux/virtio_ids.h](https://raw.githubusercontent.com/torvalds/linux/master/include/uapi/linux/virtio_ids.h)（实读） |
| 11 | virtio 附录留实验/厂商段（512+），新 ID 走 TC/virtio-dev 分配 | 中（附录正文未逐字核到，PM-10 §1.6 同判） | [OASIS virtio 1.3](https://docs.oasis-open.org/virtio/virtio/v1.3/csd01/virtio-v1.3-csd01.html)（附录 B.3 在册）+ [issue #31](https://github.com/oasis-tcs/virtio-spec/issues/31) |
| 12 | virt 机型设备地址官方告诫「guest code must look in the DTB」；BerryOS `-kernel` 路无 DTB → 板级常量钉（UART/GIC 现行做法） | 高 | [virt 文档](https://www.qemu.org/docs/master/system/arm/virt.html) + `fwcfg.rs:5-9` |
| 13 | TCG 无 PSCI SYSTEM_SUSPEND（0x8400000C）/CPU_SUSPEND=WFI；KVM 2022 起支持 | 高（转引复核） | [target/arm/tcg/psci.c](https://github.com/qemu/qemu/blob/master/target/arm/tcg/psci.c)（PM-10 事实 9/10） |
| 14 | GitHub 公有仓免费 arm64 runner（`ubuntu-24.04-arm`，4 vCPU）；`/dev/kvm` 是否放行未证实 | 高/低（两截） | [GitHub changelog 2025-01-16](https://github.blog/changelog/2025-01-16-linux-arm64-hosted-runners-now-available-for-free-in-public-repositories-public-preview) + [talos-box#74](https://github.com/randax/talos-box/issues/74) |
| 15 | WAKE_RING 64 槽/WakeRec 32B `#[repr(C)]`（_pad 3B ABI 演进保留）——telemetry_open 映射形状 day-one 已冻 | 高 | `kernel/src/telemetry.rs:276-302` |
| 16 | G1.5 boot 断言已验唤醒环：计数≥2/单调/原因合法/双路径覆盖——A4 断言挂点同构可扩 | 高 | `kernel/src/user.rs:1874-1882` |
| 17 | IRQ 派发回调槽（EOIR 后 C-ABI）+ 唤醒源清单极小（CNTV PPI27 + PL011 SPI1 + SGI）——归因面 day-one 可穷举 | 高 | `arch/aarch64/src/lib.rs:141-164` · `gic.rs:28-31` |
| 18 | 冻结原语 SuspendTarget/SuspendTok 已在途（sched-1.1）；NR_PROCESS_SUSPEND 已在册 | 高 | `kernel/src/suspend.rs:12-30` · `kernel/src/syscall.rs:84` |
| 19 | QMP 注入通道现成：INPUTP 三批 input-send-event + unix sock + 回退省 device 键 | 高 | `tools/smoke.sh:982-1078` |
| 20 | B4 掉电相 pwrcls 毒环注入先例（宿主改盘面 + guest 标记双面不变量） | 高 | `tools/pwrcls.py:1-26` · 规则 07:23 |
| 21 | timer 原语带 slack+policy 三元组 = 设计位未落（A3 前置）；0x05 maintenance 编码冻结不启用（A5 前置） | 高 | `docs/调度器设计-v1.1.md:126,155` · ADR #42（`docs/架构决策.md`） |
| 22 | 交互功耗双断言（触摸风暴合并率 N 事件/150ms→恰 1 脉冲；INDIRECT 合成事件不续命）一期已立项 | 高 | `PM-00:198,285` · `PM-04:96,218` |
| 23 | TCG CNTV tick 实测 ≈ SLICE_HZ/2；时序断言一律 `-icount` | 高 | 规则 08:27 · `docs/测试策略-v1.0.md:88` |

---

## §3 教训与坑

1. **goldfish 寄存器图 = 逐字段写 = 顺序坑的寄存器版**：goldfish 每字段一个寄存器、宿主逐个写（事实 8），Android emulator 注入顺序敏感坑（PM-10 坑 4）正是这个形状的运行时后果。T0 方案在**消息层做全量快照+seq**，顺序坑构造性消失——不需要设备侧保证写序。
2. **「无唤醒源」的说法要精确化**：缺的是「QEMU 级挂起态唤醒」（VM 真睡了再叫醒——TCG 根本没有睡态，PM-10 定谳不变）；不缺的是「idle/freeze 态的 IRQ 唤醒注入线」（GPIO3/UART1 两条，事实 1-6）。把两者混谈会导致误判 fork QEMU 是必需品。
3. **ACPI 分流陷阱**：同一 QMP `system_powerdown`，`-kernel` 相走 GPIO、EFI/AAVMF 相走 ACPI GED（事实 4）。断言脚本若跨相复用同一命令，会在 EFI 相静默改道 → 断言哑弹。挂接表里 PM 断言只进 `-kernel` 相。
4. **PL061 锁存语义**：GPIO IRQ 锁存需 guest 写 ICR 清除；风暴注入的「K 发」要 guest 侧配合清锁，纯宿主 QMP 连发不是 K 个独立事件。对照：UART1 RX 每字节天然一 IRQ——风暴用 UART1，单发电源键事件用 GPIO3。
5. **设备地址无 DTB 可查**：BerryOS `-kernel` 路没有 DTB（事实 12），sim 设备如果走 T1 MMIO 方案，地址必须像 UART/GIC 一样钉板级常量；QEMU 官方告诫设备地址可变。T0 方案的 UART1 地址属 virt 机型稳定 ABI（virt 机型兼容承诺下 0x09040000 自 virt-2.4 起未变）【中】。
6. **fork 的真实成本在构建管线不在设备代码**：goldfish 式设备本体 ~200 行 C；贵的是本机 brew→自建、CI apt→源码构建/容器镜像、QEMU 升级跟版三件套（§1.2）。T1 立项判据应写成「断言需求清单」而非「写个设备不难」。
7. **注入回声/竞速/首事件三坑的 PM 版**（转引规则 07 判例，本刀全程有效）：断言串不得被注入回声满足（A79，`规则/07:57`）；判据必须状态谓词非竞速（A32，`规则/07:51`）；首事件裕量按首拍网格标定（A129，`规则/07:66`）；挂起循环类**至少跑两轮**——pm_test「二跑才炸」官方规律（PM-10 事实 2）+ A110 flake 双证。
8. **时序敏感断言必 `-icount`**：A3 的 ε 邻近窗口、A1 的 icount≈0 都挂 B-2 通道（`tools/bench.py --icount` 现成，规则 07:42）；TCG 2× CNTV 效应下非 icount 时序断言只配当金丝雀（事实 23）。
9. **仿真绿传递律是排他律不是口号**：sim 驱动一旦走「仿真专用 if 分支」，绿迹对真机归零（PM-10 §3-8）。落法 = 契约 crate 单源三消费（sim 驱动/真机驱动/berry-sys），CI 侧加 grep 守卫扫 `#[cfg(feature = "sim")]` 类分支进契约文件即拒（学 markers.sh 防假绿兜底 `tools/markers.sh:20-25`）。

---

## §4 BerryOS 启示与落法

### 4.1 三设备契约具体化（T0 形态：UART1 字节流 + 消息层原子快照）

共同骨架（三设备同一传输、同一消息帧；帧头 8B 定长）：

| 区 | 内容 | 说明 |
|---|---|---|
| 帧头 | magic u32 + dev u8（1=batt/2=therm/3=modem） + ver u8 + seq u16 | ver 学 OH HDI IDL 版本目录（PM-10 事实 15）；seq 递增 = 原子快照序号（坑 1 的解） |
| 载荷 | 全量快照（定长 per 设备） | 每帧全量替换，宿主禁止多帧拼装 |
| 事件位图 | u32 changed_bits | 对齐 goldfish INT_STATUS（BATTERY_STATUS_CHANGED/AC_STATUS_CHANGED 先例，事实 8） |
| 回声 | guest 应答 `ACK seq` 经同通道回写 | 宿主可断言注入真到达（对抗 A79 回声歧义：断言吃 ACK 不吃注入回显） |

设备分则（字段集 = 业界契约面并集，标注来源）：

| 设备 | 快照字段草案 | 契约对齐来源 | 可断言什么 |
|---|---|---|---|
| sim-battery | {present, ac_online, status(4 态), health(8 态), capacity_pct, voltage_mV, temp_mC, current_now_uA, current_avg_uA, charge_full_uAh, charge_counter_uAh, cycle_count, timestamp_ns, measured:bool} | goldfish 寄存器集（事实 8）∪ Fuchsia BatteryInfo 15 字段（PM-08 §6：level_percent/level_status/time_remaining union/remaining_charge_uah/full_capacity_uah/present/双电流/temperature_mc/timestamp/battery_spec）+ P8 独有 measured/modeled 位（PM-12 回填 #12） | 低电阈值降档迁移；充电事件→策略响应；AC 插拔→状态机 |
| sim-thermal | {sensor_count, sensors[{kind:real\|virtual, temp_mC, trips[{level, threshold_mc, clr_mc}]}], level_hint} | PM-07 §4-2 自报接口（sensor_id, kind, temp_mc, trips 表）+ thermal_level u8 四档只读（PM-07 §4-5） | 穿 trip→N ms 帽变化（遥测 THERM 臂）；回落解除；迟滞窗不抖动；critical 短链不依赖服务存活（PM-07 §4-7 四件套） |
| sim-modem | {rrc_state, tail_ms(尾能档), signal_tier, in_maint_window} | PM-05 §4.5（尾能时长/弱信号档可配置）+ PM-11 §2.4 基线表 12 项（Android 生产默认值） | A5 维护窗攒批（DEFERRABLE 流量窗内突发、窗外 0 唤醒）；无租约时 broker 下电调用发生 |

**绿传递律单源落点**：新 crate `user/berry-pwproto/`（仅数据结构 + 帧编解码，host 测试可跑 `just test-host`）三消费——① user 态 sim 服务（解析 UART1 帧→经**驱动自报接口**上送，与 PM-07 §4-6「sim-thermal=普通热驱动走同一接口」同构）；② 未来真机驱动（电池/热/modem 驱动产出同结构）；③ berry-sys 只读面（SDK 契约，PM-12 回填 #15）。真机到来换数据源不换结构。

### 4.2 九断言逐条挂 G0–G5 总表

> 挂接点 = 既有相（规则 07:20-27 相构成表）；「第一批」= 刀 1 收尾期即可落（零新机制位）。判据全部状态谓词；标 [icount] 者挂 B-2 确定性相。

| # | 断言 | 最早 G 档/批次 | 挂接点 | 前置依赖 | 脚本形态 | 通过判据 |
|---|---|---|---|---|---|---|
| A1 | IDLE 稳态零唤醒 | G1+（任意稳态相） | soak 相稳态窗扩展 + `bench-icount` [icount] | 刀 1 per-source 计数器（PM-12 §3.3-3）；ADR #53 已立法 | soak 心跳加唤醒计数字段 + icount 对拍 | 稳态窗 WAKE_RING 醒因计数=0 ∧ icount≈0；非零即 FAIL 并打醒因 id |
| A2 | 冻结循环压测（**跑两轮**） | 现在可挂/第一批 | ktest（boot 期）——NR_PROCESS_SUSPEND 在册（事实 18） | sched-1.1 冻结原语收尾（载体已在途） | ktest 两轮 freeze/thaw 循环 + 末轮全量标记 | N≥2 轮 token 计数严格递增 ∧ 末轮全量标记在席 ∧ 金丝雀不熄（学 suspend_stress_test 计数器断言，PM-10 §1.4） |
| A3 | 唤醒风暴→合并率 | G1+/第二批 | ktest [icount]（风暴自造零宿主依赖）+ UART1/GPIO3 宿主风暴变体 | **timer 原语 slack+policy 三元组落地**（事实 21 设计位未落=硬前置） | guest 测试任务自造 K 个 ε 邻近超时 + 合并窗 M | 实测唤醒 ≤ ⌈K/M⌉ ∧ 无丢失（K 全部到期或被合并执行）；宿主变体用 UART1 RX 连发字节 |
| A4 | 无主唤醒=失败 | 现在可挂/第一批 | ①boot 断言扩展（user.rs:1874 同构）②smoke PM 相正向对照 | 刀 1 归因面（源 id 铸造，PM-12 §3.3-1 硬前置） | ①稳态窗逐醒因查持有者；②QMP `system_powerdown` 注入 GPIO3 无主 IRQ | ①稳态窗未知醒因=0；②正向对照：无主 IRQ 被记账为 unowned + 系统存活 + 计数 +1（毒环式负例，学 pwrcls 先例事实 20） |
| A5 | 维护窗攒批对齐 | M4 后/第三批 | rootfs 相扩展（sim-modem 注入经 UART1） | M4 0x05 maintenance 编码启用 + berry-power 维护窗策略 + sim-modem | 注入 DEFERRABLE 流量剧本 → 窗内突发发完 | 窗内任务执行数 ≥ 阈值 ∧ 窗外唤醒数=0 ∧ 一条尾能买单（合并后 modem 拉起次数=1，PM-05 §4.5 判据） |
| A6 | 触摸风暴合并率 | gui-inputd 后/第二批 | RX 注入相 h 段扩展（INPUTP 通道现成，事实 19） | gui-inputd 合并窗机制 + P7 交互事件分类（PM-12 回填 #11） | qmp_inject 风暴化：150ms 窗连发 N 事件 | 合并后分发次数/注入次数 ≤ 1/N·窗（PM-00:198「N 事件/150ms→恰 1 脉冲」） |
| A6b | AOD 合成事件不续命（INDIRECT） | 同 A6 同刀 | 同上（事件带 INDIRECT 位注入） | 同 A6 + userActivity 同构入口（PM-04 §8-12） | 注入带/不带 INDIRECT 位两路事件对比 | INDIRECT 事件不重置 idle 超时（DIM→OFF 照常迁移）；直接事件照常续命（永动机负例，PM-00:285 测试用例） |
| A7 | 停帧→交互起振延迟 | GUI 线 | GUI 遥测（非门禁） | GUI 起振遥测协议位（`docs/GUI里程碑规划-2026-10-v1.0.md:82`） | 遥测导出 | IDLE→首帧 ≤1–2 帧预算，只记录不拦截 |
| A8 | 电源类别毒环 | 现在可挂/第一批 | B4 掉电相扩展（pwrcls 通道现成） | lease/power·mode 校验面最小实现 | pwrcls 式注入非法样本（负 seq/未 init 帧非法类别） | 非法请求 -EINVAL ∧ 记账不腐蚀 ∧ 连环毒后系统活+全量标记照打 |
| A9 | 电源状态机迁移矩阵 | power·mode op 落地后/第三批 | ktest + smoke PM 相（枚举已冻） | ADR #53 ④ power·mode op（未落=硬前置）；GPIO3 提供 OFF 态事件源 | NORMAL/DIM/AOD/OFF 全矩阵枚举驱动 | 合法迁移成功 ∧ 非法迁移 -EINVAL ∧ 终态遥测对齐（pm_test 逐级过门同构，PM-10 §1.2） |

### 4.3 挂起链路 QEMU 三档现实与 KVM CI 路径

| 档 | SYSTEM_SUSPEND | BerryOS 行为 | 断言形态 |
|---|---|---|---|
| TCG（主验场） | NOT_SUPPORTED（事实 13） | boot 期 `PSCI_FEATURES(0x8400000A, 0x8400000C)` 探测 → 能力表进 boot 遥测行（学 smp.rs:572 S-1 分型门同型扩展，零改三档代码） | 深睡请求断言**降级事件在册**：走「优雅降级=冻结循环（A2）」路径 + 遥测记 `degrade_to=freeze`；探测行缺席本身 = FAIL（防静默假绿） |
| KVM | 2022 起支持（事实 13） | 同一探测分支自然翻绿 | 真深睡路径断言进 KVM 相（非主线） |
| 真机固件 | 取决于 SoC | 同上零改 | 契约冻结评审对表 |

**KVM 相进 CI 的路径（开放问题 6 可执行解）**：GitHub 公有仓 `ubuntu-24.04-arm` 免费 runner（事实 14）+ `qemu-system-arm -accel kvm`；**spike 项** = 该 runner `/dev/kvm` 是否放行未证实（社区在查，[talos-box#74](https://github.com/randax/talos-box/issues/74)）——首个 spike workflow 只跑 `ls /dev/kvm` + 一次 KVM boot 探测行，半页 CI 配置即可裁决。备选：自托管 arm64 runner；macOS HVF 是本地第二选项但 PSCI 实现归属未核【低】，不作依赖。

### 4.4 开刀清单（三步独立可验证，每步单独成提交）

**步 1（通道+契约面，零内核机制位）**
- `arch/aarch64/src/gic.rs`：+`IRQ_PL1_RX: usize = 40`（UART1 SPI8+32，对齐 IRQ_PL011_RX 注释体例）；PL011 RX 派发面加 UART1 分支（`lib.rs:141-164` 回调槽同口）。
- 新 `user/berry-pwproto/`：帧头/三设备快照结构 + 编解码 + host 单测（进 `just test-host` 台账）。
- 新 user 态 sim 服务（挂 svcmgr spawn 序或独立 bin）：UART1 读帧 → 驱动自报接口上送 → 回 ACK。
- `justfile` run-rootfs/smoke 加 `-serial unix:...`（serial1）+ 宿主注入 client（抄 qmp_inject 的 python socket 模式，`smoke.sh:1041` 同构）。
- 验收：注入一帧 battery 快照 → guest 遥测打 `SIMBAT ok seq=N` 单消息原子标记（A60 纪律：svcmgr 私有通道收齐比对，不走 console 分片）。

**步 2（断言第一批挂 smoke）**
- A2：ktest 冻结循环两轮（NR_PROCESS_SUSPEND 现成）。
- A4：user.rs:1874 唤醒环断言扩 source 归因位（吃 WakeRec._pad 3B，PM-12 §3.3-3）；正向对照 = QMP system_powerdown 无主 IRQ（smoke PM 微相）。
- A8：pwrcls 毒环加电源类别非法样本。
- `tools/markers.sh` 双数组 +PM 族标记（smoke.sh/smoke-smp.sh 消费自动跟随；smoke-image 硬拷贝勿动——ECO 族不进 EFI 面）。
- 验收：`just smoke` 全绿含新 PM 族标记；负例样本故意反转一次验证断言可红（fail loud 自证）。

**步 3（PSCI 探测 + 挂起降级）**
- `arch/aarch64/src/`（smp.rs 旁新 psci.rs 或扩展现有）：PSCI_VERSION + PSCI_FEATURES 探测函数（HVC conduit 分型门复用，`smp.rs:572-573`）。
- boot 遥测加能力行 `PSCI susp=n`(0/1)；深睡请求路径接 A2 冻结循环 + `degrade_to` 遥测。
- 验收：TCG 上探测行 `susp=0` + 深睡请求后降级事件在册 + 系统存活全标记照打。
- ✅ 已落（PM仿真S3 v1.0）——`docs/PM仿真S3断言实施设计-v1.0.md`；载体 = arch `psci.rs` 探测行 + ktest `PWRDEG ok d=1` 降级证据链（裸相 30→32）。

**押后不排期**：A3（等 timer slack 刀）、A5/A9（等 M4/berry-power/power·mode op）、T1/T2 fork QEMU（等「断言需求清单盖过构建管线成本」判据触发）、KVM spike（等 CI 窗口）。

### 4.5 明确不做

- 不 fork QEMU 起步（§1.1 裁决）；不为深睡做 TCG 假实现（PM-10 §4.5 沿袭）。
- 不往 smoke 塞功耗绝对值断言（PM-10 §1.1 三层模型铁律）。
- 不在 EFI/AAVMF 相挂 GPIO3 断言（坑 3 ACPI 分流）。
- sim 契约文件不留 `#[cfg(feature="sim")]` 分支（坑 9 排他律 + CI grep 守卫）。

---

## §5 引用源列表

**联网取证**
- QEMU virt 机型源码（master 实读）：https://raw.githubusercontent.com/qemu/qemu/master/hw/arm/virt.c（memmap/irqmap/powerdown 分流/UART1 创建）· https://raw.githubusercontent.com/qemu/qemu/master/include/hw/arm/virt.h（GPIO_PIN_POWER_BUTTON）· https://raw.githubusercontent.com/qemu/qemu/master/hw/gpio/gpio_key.c（100ms 自复位脉冲）
- QEMU virt 文档：https://www.qemu.org/docs/master/system/arm/virt.html（virtio-mmio-transports/第二 UART 条件/DTB 权威）
- QOM/设备开发：https://qemu-project.gitlab.io/qemu/devel/qom.html · https://qemu-project.gitlab.io/qemu/devel/qdev.html
- goldfish battery 驱动（寄存器图）：https://raw.githubusercontent.com/torvalds/linux/master/drivers/power/supply/goldfish_battery.c
- Android Emulator Internal Book：https://github.com/aospbooks/android-emulator-internal-book/blob/main/06-virtual-hardware.md
- virtio：https://raw.githubusercontent.com/torvalds/linux/master/include/uapi/linux/virtio_ids.h（标准 ID 至 48）· https://docs.oasis-open.org/virtio/virtio/v1.3/csd01/virtio-v1.3-csd01.html（附录 B.3 在册；实验段正文未逐字核到）· https://github.com/oasis-tcs/virtio-spec/issues/31（ID 26 分配例）
- GitHub arm64 runner：https://github.blog/changelog/2025-01-16-linux-arm64-hosted-runners-now-available-for-free-in-public-repositories-public-preview · KVM 放行调查 https://github.com/randax/talos-box/issues/74
- 转引自 PM-10（未重复核）：https://github.com/qemu/qemu/blob/master/target/arm/tcg/psci.c · https://www.infradead.org/~mchehab/kernel_docs/power/basic-pm-debugging.html · https://chromium.googlesource.com/chromiumos/platform2/+/main/power_manager/tools/suspend_stress_test

**本地实证（2026-11-07，本机 QEMU 11.0.3）**
- `-M virt dumpdtb` + dtc 反编译：pl011@9040000（SPI8）/pl011@9000000（SPI1）/pl061@9030000（SPI7）/gpio-keys poweroff GPIO3 KEY_POWER(0x74) 全部在 DTS 逐字核到。
- `qemu-system-aarch64 --version` = 11.0.3（brew）。

**仓内对接**
- `kernel/src/telemetry.rs:276-302`（WAKE_RING/WakeRec 32B/_pad）· `kernel/src/user.rs:1874-1882`（唤醒环 boot 断言）· `kernel/src/suspend.rs:12-30`（SuspendTok）· `kernel/src/syscall.rs:79-88`（NR_PROCESS_SUSPEND 在册）· `kernel/src/main.rs:1259-1288`（virtio-mmio 窗 VMO）· `arch/aarch64/src/lib.rs:141-164`（IRQ 回调槽）· `arch/aarch64/src/gic.rs:28-31`（IRQ 27/33）· `arch/aarch64/src/smp.rs:572-573,790`（conduit 分型/CPU_ON）· `arch/aarch64/src/fwcfg.rs`（fw_cfg 具名项单源）
- `tools/markers.sh:12,18,20-25`（标记单源+防假绿兜底）· `tools/smoke.sh:45,982-1078`（八跑/QMP 注入）· `tools/pwrcls.py:1-26`（B4 毒环）· `justfile:18,840` · `.github/workflows/ci.yml:48,233`
- `docs/规则/07-测试与冒烟.md:20-27,42,51-66`（相构成/bench-icount/判例族）· `docs/规则/08-QEMU与仿真.md:23,27`（conduit/2× 效应）· `docs/测试策略-v1.0.md:20-29,84-88`（G0–G5/代理指标/-icount 纪律）· `docs/调度器设计-v1.1.md:126,155`（timer slack 设计位）
- `research/PM-10:§1.6,§4`（九断言/sim 草案/psci 定谳）· `research/PM-12:§3.3-6,§4-6/7`（刀 1 前置/开放问题）· `research/PM-07:§4-5/6/7`（sim-thermal/四件套）· `research/PM-05:157-167,198-203`（sim-modem/meter_query）· `research/PM-08:§6`（BatteryInfo 15 字段）· `research/PM-00:198,285`（交互双断言立项）· `research/PM-04:96,218`（INDIRECT 不续命）

（v1.0 完；升级触发器：步 1-3 任一落刀 / KVM spike 出结果 / T1 fork 立项裁决，三者任一发生即升 v1.1。）
