# svcmgr 双崩刀（workflow 编排：侦查三角度→实施三件套→对抗审查→修复回写）

Status: implemented

一句话说明：workflow 编排模式（owner 指令 2026-11-22）收口堵死 gui8 与 rootfs 两条验证链的 svcmgr 双崩——侦查三角度（wf_7ae538bd）交叉定谳：vDSO 快路径五轴出罪（x17/x18 魔数=page_ready 比较常量在 x18 的良性滞留，反汇编 VA 0x152d4 码型逐位吻合）、我方点名的 NET/libc 批次全部考古赦免（git diff 证内容早入 10:05 绿树）、崩 A=svcmgr 主任务 4KiB 单页栈无保护页静默溢出腐化控制流（SP=0x28f70 已扎进 data 页实锤，cbox 死亡排液深调用图触发）、崩 B 拆两死（B1=vnet TCP 自验泵 32KiB 栈被 38.6KiB 需求穿掉——不是深序测试子；B2=svcmgr 命中自家 trap() 自陷块自毙——在册 per-child 1.75MiB 帧泄漏耗干 45MiB 池后的 eager 栈三站 -ENOMEM 最可能）；实施轮（wf_19cdc4a0）三件套+对抗审查 GO（三 P2 仪表面主线修复回写）+B1 主修，全部一次过链验证（见验证节）。

## 裁决与判例

- **侦查 workflow 配方实证**：三角度（嫌疑面审计/时间线考古/取证深挖）并行 + 行号级 schema，20 分钟出全谳且互相纠错——vdso 面反汇编出罪、timeline 面考古反转赦免我方点名批次、crash 面纠正任务书两处误设（崩 B 实为两死；B1 死者是 vnet 非 256KiB 栈测试子）。主会话裁决吸收全部三方后才写实施规格——分会话裁决纪律的活例。
- **崩 A 机制（栈几何）**：boot 进程主任务 4KiB 单页栈紧邻 data 页无保护页——下行溢出静默扎进 data 腐化控制流（ret-to-0 死状 PC=0/FP=2，SP 已在 data 页内）。修=保护页化+4 页预算：**溢出从静默腐化变干净 D-abort 被杀**（dump SP/FAR 直指祸首）。
- **崩 B2 机制（资源面）**：约束②面扩——eager 栈三站（vmo_create/commit/vmar_map）照 process_create 先例 -ENOMEM 降级（SPAWN_NOMEM 旗+spawn_nomem_exit 全量回收+Err）；帧泄漏本体（内核帧归还刀）仍在册另案，本刀只保证池干时 svcmgr 拒起不自杀。
- **B1 机制（预算面）**：编译器栈冲突防护探针踩守卫页=设计态干净签名（P4 期同款死状 tcp.rs:251 注在册——栈压是复发类问题）；修=stack_pages 8→16（64KiB，bsh/wmd 同档先例），不改迭代（无失控递归，24.4KiB 外链+14.2KiB 复合帧的合法需求）。
- **自陷点名仪表**：杀链 D-abort 且 FAR==0x4000_0000 时读用户栈 [SP]/[SP+8] 双槽+x30 交叉锚打 `selftrap retsite=`——下次任何 svcmgr/berry-exec 自陷直接点名违约站点（审查 P2 修复回写：walk+read 同 PT 锁窗、双槽防单槽扑空、print_lock 包多段打印）。

## 实施面

- kernel/src/user.rs：`USER_STACK_PAGES=4` + `USER_STACK_TOP` 0x4000→0x8000（boot 单页舰队 7 站同值双改）+ `UserProcess::build` 栈区几何（data 上方一页不映保护页+栈 4 页；svcmgr：映像 [0x1000,0x29000)/data [0x29000,0x2a000)/保护 [0x2a000,0x2b000)/栈 [0x2b000,0x2f000)/sp0=0x2f000，main.rs 读动态 stack_top 自动同步）+ `selftrap_retsite` 仪表本体。
- arch/aarch64/src/{lib,trap}.rs：`SelftrapProbe` 槽（boot 屏蔽窗注册，user_fault_exit/cbox/demand_probe 同族反转模式）+ 杀链 EC==DABORT_LOWER 且 FAR==SELFTRAP_FAR 时回调。
- user/berry-exec/src/lib.rs：eager 栈三站 -ENOMEM 降级（Fresh 形回收序：demand 槽注销→unmap→close，ht=0/env=0/None 按创建序一一对应；非 -ENOMEM 仍 trap）。
- user/bins/svcmgr/src/main.rs：vnet SpawnSpec stack_pages 8→16（定量依据注释在位）。
- kernel/src/main.rs：`set_selftrap_probe` 注册（set_cbox_hook 后同窗）。

## 验证

- 编译面：berry-kernel（aarch64-unknown-none）与 berry-exec（musl+berry-ld）check 全过（存量警告零新增）。
- 实施期两轮回归与修复（主线复核抓出，agent 审查面漏项）：① 单页舰队 good 模板把 VMO 映到固定 VA 0x5000——新栈 [0x4000,0x8000) 撞占 → vmar_map -EINVAL → 断言自陷 → watchdog 杀数 2 panic（寄存器五元组 x1=vmar/x2=vmo/x5=len/x6=rights/x0=-22 直指）；修=模板映射 VA 0x5000→0x9000（映/读/解映三处）——判例：**改 boot VA 布局的消费者审计必须含映射请求 VA**（数据槽与 SP 之外的第三类硬编码，实施与审查都只查了前两类）。② 环境债两轮：echod 18080 残留独占 + 被 TaskStop 的 QEMU 子进程逃逸占 18081/18082（QEMU 字段协议老坑再现）——认准自家特征清场后真跑。
- e2e 终验（第四轮，场净）：**旧死点全段标记雪崩复活**——`G1.5 ok`（0x9000 修复面）、`DPGPRESS ok`、`NET ok`、**`TCP ok echo=64`（B1 修复实证——此标记在全部崩链从未打出）**、`TCPS ok`/`DNS ok`/`PS ok`/`BC ok`/`THR ok`/`APP ok n=10`/`DPGP ok`/`DPGS ok`/`DPGN ok`/`HEXYL ok`/`CHAIN ok`——svcmgr 全程存活穿越此前必死段（B2 面未再触，-ENOMEM 降级就位）；自陷仪表全程仅 rogue_ptr/asm 模板的预期噪声行（`selftrap retsite=UNREADABLE x30=0xffffff80...`），无 svcmgr 自陷。链尾 EVS/重载窗长相照跑（rc=1 归因兄弟评审 worktree QEMU 占宿主 18083——sktclient 绑不上、客机 SKT 无帽阻塞，第五种环境债形态，刀面标记不受影响）。**gui8 补证（同日）**：gui8-smoke 全链 PASS rc=0（boot A 点击相全绿=崩 A 原发位穿越；boot B 布局持久相全绿）——svcmgr 双崩刀与 TLS 协议刀双闭实证。
- 判据反转面：三轮验证里每轮的死法（panic→端口→逃逸 QEMU）依次排空后才见真绿——多轮环境债叠加是「同码异果先查载荷」判据的又一案。
