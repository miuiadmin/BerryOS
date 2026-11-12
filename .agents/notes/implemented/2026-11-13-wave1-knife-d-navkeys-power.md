# 三键导航条 + guest 内关机闭环(berryctl power / PSCI SYSTEM_OFF)

Status: implemented

## Problem

桌面无任何系统导航面:launcher 只有翻页 + 点击 spawn,无返回/桌面/关机——用户「关机都没有」的直评属实(关机唯一入口在宿主 berry 工具栏 OFF 键)。根因与修法分层见 [02 系统导航与状态栏](../../../research/桌面UX审计/02-系统导航与状态栏.md) §4/§6.1 M2+M3。本刀 = 实施波一刀 D。

## Decision

三件套:launcher 底部三键导航 + 内核 `power_off` syscall(0x88,PSCI SYSTEM_OFF)+ berryctl `power` 子命令。

- **关机链走真 PSCI**:侦察结论——kernel `arch/aarch64/src/psci.rs` 有现役 PSCI 探测(HVC conduit + `PSCI_VERSION` 查询,boot 期打 `PSCI version 0x00010001`),QEMU TCG/KVM 的 SYSTEM_OFF(0x84000008)均受理(PM-10 §1.6 实读 `psci.c`:走 `qemu_system_shutdown_request`);缺的只是 SYSTEM_OFF 调用面。按任务书「PSCI 在则 berry-sys 调用面加 power」走 add-only 加号全链:`psci.rs` 加 `system_off_hvc`(conduit 活性缓存 `PSCI_CONDUIT_ALIVE` 门——probe 期 PSCI_VERSION 有效即 true;EFI handoff/非 EL1 路不发 HVC 诚实返 -1)→ abi.toml 加 0x88 `power_off`(nr_total 37→38,`gen_abi.py` NO_GATE_SYSCALLS 白名单同步——零 handle 面)→ 五面再生 → kernel `sys_power_off` handler(SVC_CLASS_TABLE)+ reg → berry-sys `berry_power_off()` wrapper。
- **v1 无门控如实注**:单用户仿真系统无能力框架;门控与优雅关机序(服务停摆/数据 flush)随审计 02 号 I3 刀。probe 缓存门保证无 PSCI 拓扑(EFI handoff 相)诚实 `-ENOTSUP`——launcher 关机键失败打「use host toolbar OFF」提示、berryctl 退 1,宿主 OFF 键仍是该拓扑唯一关机路。
- **三键导航**(`desktop.rs` build 尾 + `frameloop.rs` on_nav_click):返回(v1 = 翻页语义,`page>0` 吸附上一页 + a11y 重挂;应用内返回随 wmd 系统键语义刀 I2——如实注)/ 桌面(回 page 0,已在 = 无操作)/ 关机(二击 3s 窗确认:首击 `set_props` 换暗红警示面 + 窗起算;窗内再击 = `berry_power_off()` 成功不归;过期复位,armed deadline 纳入稳态 wait 帽保证 3s 准时醒)。走现役 `Msg::Click` 通道零新输入面。
- **44px 命中区硬指标**:键 = Icon 节点整体,big 档 132px(44vp×3)/紧凑档 44px(应用规范 §3.2 下限);紧凑档 `grid_h` 公式扣 44(`gui8-smoke.py` BOTTOM_PAD 镜像同步 8→52,cell0_center 黄金值 (95,99)→(95,95) 随更)。
- **图标自绘**(ADR #63 备选路):OH 官方仓无导航键现成 Apache-2.0 资产,按任务书「或 berry-ui 自绘最简形」——`icons.rs` 程序化 66×66 Argb 深底圆角块(0xFF101828,CLEAR_BG 同族)+ 白符号(back = 左向实心三角 / home = 圆环 / power = 缺口圆环 + 竖线),Argb 直写像素(`argb_blit` 忽略 fg tint);armed 变体 = 同形暗红底(0xFF7A1F1F)。对比度:符号白对深底 ≈14:1,底对壁纸 ≥5:1(§3.2 非文本 3:1 达标)。
- **a11y 32 帽修订(裁决八)**:原 4 页 ROLE_LIST 全挂 = 32/32 满员,三键无槽。改**当前页单挂**(`rehang_a11y` 同批 take/set 页容器),腾 3 槽给导航键,常态恰满:4 状态 + 1 页容器 + 20 格 + 4 dock + 3 键 = 32。非当前页本就非可见页,不出 a11y 树是三系常规形态。
- 树规模 357→361(+navbar Row + 3 Icon)< 512。

## Alternatives considered

- 关机走「宿主工具栏 OFF 提示行」(任务书无 PSCI 分支):侦察定谳 PSCI 基础设施在 + QEMU 支持 SYSTEM_OFF,加调用面是小刀,真链价值(用户面直接止痛 + QEMU 退出可断言)远大于提示行,弃。
- SYSTEM_OFF 也过 PSCI_FEATURES 门:规范上 SYSTEM_OFF 是 0.2 起强制函数、FEATURES 只查可选函数;TCG 的 FEATURES 返 -1(SYSTEM_SUSPEND 因此 susp=0)但 SYSTEM_OFF 在——按 FEATURES 门会错杀 TCG,故用「conduit 活」判定,弃 FEATURES 门。
- 导航键图标从 OH 仓续取 SVG:需网络取资产 + PIL 光栅链 + 资产清单/生成器全套,波一工期外;自绘单色形与 OH 状态图标 1-bit 白形同族,留 ADR #63 续取为波二升级位。
- 三键并进 dock 区:dock 是 app 常驻槽(PhoneSmartDock 语义),混入系统键语义漂移;独立 navbar 行(恰好落 big 档 OH_DOCK_BOTTOM×3=135px sysUI 保留区)形态与 OH 一致,弃并区。

## Consequences

- guest 内关机真链成:点击关机二击 → PSCI SYSTEM_OFF → QEMU 整机退出(EFI handoff 相除外——conduit 归固件,诚实降级打提示);`berryctl power` 同链(CLI 面,bsh `run` 无参限制下 ver/ps/mem 之外的子命令由 spawn 侧带 argv 消费,见 Risks)。
- 三键只在桌面态有效:应用全屏独占时随 launcher 内容层整层被盖(审计 02 号 §2.3 边界),SYSTEM 窗随 I1 刀;任务书已明示此验收口径。
- abi.toml 加号是 ABI 冻结面变更:add-only 先例 0x86/0x87(2026-11-07 破冻),本次 0x88 同形(gen_abi selftest 四元组钉板 (38,42,41,45) 同步);docs/系统调用ABI设计-v1.0.md 三处手写计数行同步。
- 裁决八修订(页容器单挂)改变 a11y 树形态:外部若有按「4 页容器全挂」断言的消费面需同步——当前无此断言面(gui8-smoke 只对串口标记)。

## 验证账(2026-11-13 实跑)

- 已过:cargo build(kernel debug / berry-sys / berryctl musl / launcher musl 全绿);`gen_abi --check/--selftest` PASS(五面一致,四元组同步);`gui8-smoke.py --self-test` PASS(几何镜像 BOTTOM_PAD 8→52 随刀同步,cell0_center 黄金 (95,99)→(95,95));`docs_guard`/`check-docs` PASS;裸相 smoke 跑到 RX 交互末段方被会话 timeout 截断 = 前 54 标记含 `BC ok`(berryctl 回归)全过;`-kernel` 裸跑串口 `PSCI version 0x00010001` = EL1 路 HVC conduit 活证(`off_supported` 门真,SYSTEM_OFF 可达)。
- 被并行回归堵住的两项端到端(EFI 相):`gui8-smoke` 双 boot 与 `berry shot` 桌面抓帧——svcmgr 在 eco/bsh spawn 段确定性崩(PC=0 空指针,`G1.5: kill user task 'user_svcmgr'`,EXIT ok 之后 launcher `LAUNCH v=` 已打而首帧未出;首崩形态曾为 `expect("launcher spawn")` 的 P3-1 -13,补位提交 f44c2ba8 后移至此)。归因:svcmgr 源码与其依赖面本刀零接触(berry-sys 仅追加常量与新 no_mangle 符号),纯 HEAD 独立基线不可得(sched 重构进行中 HEAD 编不过),崩点随并行刀提交演进而移。svcmgr 修复后重跑 `python3 tools/gui8-smoke.py` 与 `berry shot --wait-marker "LAUNCH v="` 即闭环三键可视/关机二击 QEMU 退出两项。
- `berryctl power` 端到端另受 bsh v0 `run <name>` 拒带参限制(结构性,无 argv spawn 通道)——CLI 关机面在 bsh 交互档不可直达,如实注;内核链与 launcher 关机键共用同一 `berry_power_off()`,后者端到端即全覆盖。

## Related

- 底稿:[02 系统导航与状态栏](../../../research/桌面UX审计/02-系统导航与状态栏.md) §6.1 M2/M3、§6.2 I1-I3。
- PSCI 能力矩阵:[PM-10 无真机电源验证](../../../research/PM-10-无真机电源验证-v1.0.md) §1.6(TCG SYSTEM_OFF 实读)。
- 应用规范 §3.2(44px/3:1):[应用规范 v1.0](../../../docs/应用规范-v1.0.md)。
- 同波:状态栏可信化(2026-11-13-wave1-knife-b-statusbar-trust.md)、窗口逻辑尺寸(2026-11-13-berry-window-logical-size.md)。
