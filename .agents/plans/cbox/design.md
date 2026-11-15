# cbox（黑匣子崩溃账本）总体设计

Type: spec
Status: active
Updated: 2026-11-15
Version: 0.1.0

一句话说明：cbox 是 BerryOS 的崩溃记录体系——内核异常路径写 RAM 崩溃记录表（黑匣子），用户态监督者经 syscall 排液落盘 FAT 账本（`CBOX.LOG`），宿主 `berry` 工具读账本做指纹聚合与符号化定位。对标华为 AGC / Android tombstone 的「采集 → 持久化 → 符号化 → 聚合 → 定界 → 报告」流水线，按单机无云端形态裁剪：后台等价物 = 宿主侧 berry 子命令。

## 背景与对标

- 触发 = 2026-11-15 owner 提交的华为 AGC 崩溃分析后台对照（com.berry.browser CPP_CRASH 报告 + 问题详情页截图）：八部件 = 崩溃捕获（signal/地址/模块/前后台/构建号）、持久化与下载、符号化（pc+模块+偏移 → 函数名）、故障指纹聚合、多维筛选、根因定位（三级表 + lastFatalMessage 原文 + 修复建议）、责任定界（so 路径定应用侧/系统侧）、问题单工作流（状态/优先级/备注）。
- BerryOS 现状：捕获面在役不弱（内核 `EXCEPTION` dump 寄存器全集 + `G1.5` kill 链 + 用户态自陷前诊断行 `WTDIE` 族 + svcmgr eco 降级行），但全部只进串口——串口 canary 交错实测撕毁约四成（`.agents/notes/implemented/2026-11-15-svcmgr-wtdie-selftrap-forensics.md` 可复用取证节），掉电/重启即失；符号化靠人工 stripped ELF 三件套；聚合/定界/报告为零。
- 立法承接：`docs/应用规范-v1.0.md` §4.1-5 已立「崩溃报告 = eco 降级行 + 死因/退出记录 + 代号重启链」，并立法**禁用「墓碑」词指崩溃转储**（BerryOS 墓碑 = pause(save)→relaunch 读档复活谱系，`docs/app-1实施设计草案-v1.2.md` 术语注）——本模块术语随之：黑匣子 = 内核 RAM 记录表，账本 = FAT 落盘文件。
- 术语另注：smoke 在册标记 `TOMB ok` 属生命周期墓碑自验，与本模块零关系；本模块新标记 `CBOX ok` 与既有全部标记互非子串（对照 `tools/smoke-image.sh` ECO_MARKERS 清单核过）。

## 范围

**做（v1，刀 1–3）**：内核异常路径崩溃记录（EL0 fault / EL1 fault 两类，环形 8 槽 RAM 表）；排液 syscall（ABI 0x42，copy-out 全量快照）；svcmgr 电池自检标记 + 监督死亡钩子排液落盘；宿主 `berry symbolize`（ELF symtab 最近符号）与 `berry crash-log`（FAT12 根文件直读 + 指纹聚合报告）。

**不做（v1）**：内核 panic 落盘（内核无块设备驱动——vblk 是用户态服务；内核直写盘需内核 virtio-blk 驱动，挂刀 4）；svcmgr 自身死亡的落盘（排液者即死者，同因挂刀 4 第二排液者方案）；构建号维度（`.berryver` 三元组在 wip lane，合入后账本行补 tag——见挂账）；AI 分析与修复建议（数据面立起后的远期）；ANR 类监督超时事件的入账（v1 只记异常 fault，监督杀面由 eco 行 + 账本 `kind` 间接可见）。

## 契约

### 记录格式（CboxRec，定宽 repr(C)）

```text
偏移  字段      宽度  义
0     magic     4     0x58424F43（"CBOX" 小端）
4     kind      1     1 = EL0 sync（用户任务 fault，G1.5 杀）；2 = EL1 sync（内核 fault，halt 前）
5     ver       1     记录格式版本 = 1
6     rsvd      2     零填充
8     seq       4     全局单调写序（1 起；0 保留——排液方去重游标）
12    stamp     4     槽代序 seqlock：偶 = 稳态 / 奇 = 写入中——排液见奇或复读变值以 magic=0 哨兵记录跳过
16    vct       8     crash 时刻 CNTVCT 原始周期
24    name      16    任务名 NUL 填充（CURRENT 空态的早期 boot fault = "(boot)"）
40    pc        8     ELR
48    far       8     FAR_EL1
56    esr       8     ESR_EL1
64    lr        8     x30（回溯真实调用点——WTDIE 定谳实证的主字段）
72    sp        8     SP
80    x19       8     } callee-saved 三元组
88    x20       8     } （WTDIE 案 x19=sig / x20=h / x21=e 的取证先例）
96    x21       8     }
----  总计      104
```

偏移由内核 `cbox.rs` const assert 钉板（TrapFrame 同款纪律），本表与其同源。寄存器全集仍走串口 dump（`dump_hdr`），账本只收定位必需的紧凑集——符号化要 pc+lr，诊断要 far/esr，先例取证要 x19–x21。后续加字段 = ver 递增 + 尾部追加，禁中插。

### 黑匣子写入约束（异常上下文纪律）

写入点：`exception_sync_el0`（dump 之前）与 `exception_sync`（EL1，halt 之前）两处。上下文 PSTATE.I=1（IRQ 屏蔽）——**零分配、零锁、零 uart**：槽位分配用 `AtomicU32::fetch_add`（NCPUS=2 双核可同时 fault，各自拿各自槽，last-writer-wins 可接受）；字段纯 store；撕裂防护 = 每槽 `stamp` 轻量 seqlock（写者先抬奇、写域、Release fence 后落偶；读者 stamp 复读夹 Acquire，不稳以 magic=0 哨兵告知跳过）。BRK recover 路不写（非崩溃面）。

### ABI 面（0x42 cbox_drain）

- `berry_cbox_drain(buf: *mut u8, cap: u32, out_len: *mut u32) -> i64`：全量快照 copy-out（表头 12B {ver, total_ever, stored} 顺排 + 存量记录按 seq 升序）；`cap == 0` = 尺寸查询（`*out_len` = 所需字节，返 0）；`cap` 不足 → `-ENOMEM`（不部分写）；出参 `out_len` = 实拷字节数。经 `copy_to_user` 界检（syscall.rs :230 既有件），caller_el1 同门。无 right 门（诊断几何面——`process_get_exit_code` need=0 惯例）。
- 入册：`tools/abi.toml` 异常组 0x42（0x41 thread_create_exception_channel v2 预留位隔壁），stage = "v1"、implemented = true → `just abi-gen` 再生四产物（kernel/berry-sys abi_gen.rs + ABI 文档 §4/§8.2 + abi.syz）。原型层手写双镜像（berry-sys `lib.rs`）同批。
- 语义边界：排液是**快照非消费**——记录永驻至槽位轮转；排液方自持 last_seq 去重。同 boot 多次排液幂等。

### 持久化（svcmgr 半件）

- 电池自检（boot battery，tomb_selftest 后）：排液 → 期望 n≥1（boot 期 rogue 自验件 `user_n_ptr` 等确定性 fault 在册——ktest/noktest 全形态同路径）→ fs 在位（`FS_A != 0`）则合并写 `CBOX.LOG`（卷根 8.3 名，同 `LAYOUT.JSN` 惯例）→ 读回对拍 → `CBOX ok n=<账本现存记录数>`。fs 缺席（RAM-only 形）= eco 降级行、不发标记（车辆面标记只挂有盘车辆）。
- 账本行格式（一行一记录，ASCII）：`CBOX v1 seq=<n> kind=<el0|el1> task=<名> pc=0x… far=0x… esr=0x… lr=0x… sp=0x… x19=0x… x20=0x… x21=0x… vct=<十进制> boot=<id>`。boot id = 账本现存最大 boot id + 1（读-改-写链自续）。
- 写策略 = 整文件读-改-写（`layout_io` put_file 形态先例）：合并去重（last_seq 游标）→ 超单簇帽（4KiB ≈ 20+ 行）时**老记录先行轮出**（环形语义，与内核 8 槽同构）→ 全量重写 → 读回对拍。写失败非致命（eco 行，监督面照跑）。
- 监督死亡钩子：`launcher_supervise` 与 `app_supervise` 的 TERMINATED 命中面各加一次排液 + 落盘（崩溃本是异常面，行不进 smoke 十段对拍——同 eco 纪律）。

### 工具面（berry CLI）

- `berry symbolize --pc 0x… --elf <路径>`：ELF64 symtab 最近符号（≤ pc 最大 st_value；strip 件回落 dynsym，双空 = 报错指路构建产物位）。零新依赖——手写 ELF 头/节表/symtab 解析（~150 行）。
- `berry crash-log --disk <镜像>`：FAT12 只读直读（BPB → 根目录 → 簇链；数据盘几何与 smoke 造盘同源）→ 解析 `CBOX.LOG` → 按 `(kind, task, pc, lr)` 指纹聚合 → 报告：每指纹次数 / 首末 boot id / 任务名 → 符号化（`--elf-dir` 给定时映射 `user_<名>` → `user/cargo-target/aarch64-unknown-none/release/<名>`，内核名 → 内核 ELF）。退出码：有记录 0；镜像/账本不可读非零。
- 任务名 → ELF 定界即「责任定界」的 BerryOS 形态：加载基址全系统固定（`USER_CODE_VA` 族），pc 归属由任务名单值判定，无需华为式 so 路径扫描。

### 分期刀谱

| 刀 | 内容 | 验收 |
|---|---|---|
| 刀 1 | 内核半件：cbox.rs + trap 双钩 + ABI 0x42 入册再生 | `cargo check` 零新告警；`just abi-check` 绿 |
| 刀 2 | svcmgr 半件：电池自检 + 监督钩子 + CBOX.LOG 落盘 | smoke-image 串口 `CBOX ok n=` 到席 + 账本文件在盘 |
| 刀 3 | berry 工具：symbolize + crash-log | 端到端：造崩溃 → 账本 → crash-log 报告出符号名 |
| 刀 4 | DEFERRED: 内核 panic 落盘（内核 virtio-blk 最小驱动 / 预留扇区直写）+ 第二排液者（svcmgr 死亡面）| 原因：内核无块设备驱动；触发：内核态崩溃取证需求进前排 |

### 验证

- 刀 1：`cargo check -p berry-kernel --target aarch64-unknown-none` 零新告警；`just abi-check` 逐字节绿。
- 刀 2：`SMOKE_KEEP_LOG=1 just smoke-image`——串口 `CBOX ok n=` 与 rogue EXCEPTION 行数对账；数据盘 `CBOX.LOG` 宿主侧字节直检。
- 刀 3：test-host 单测（ELF 解析对真产物、FAT12 对造盘脚本产物、聚合对合成账本）；端到端配方 = 跑一辆含 rogue 的车 → `berry crash-log --disk` → 输出含 `user_n_ptr` 符号名。
- 回归：`just test-host` 全绿；`just smoke` 全绿（环境碰撞窗现实下按 `gate-contention-five-forms` 判读三步处置，全绿补跑挂合并前——先例纪律）。

## 边界与挂账

- 内核 panic / svcmgr 自死两大「黑匣子随机同毁」面挂刀 4（见分期表 DEFERRED 行）——v1 账本覆盖面 = 监督链可观察的用户任务崩溃（rogue 自验件 + launcher/app 监督面 + guifbd 类常驻任务 fault），恰是 wtdie 案「launcher 停摆连坐链」一类的取证主需求面。
- 构建号维度：wip lane `.berryver` 三元组合入后，账本行补 `build=<三元组>` tag（改点 = svcmgr 落盘处单点，格式 ver 不动——tag 是行尾追加）。
- 串口 dump 与账本**双写并存**（不撤串口面）：串口是全寄存器深度取证，账本是指纹聚合与跨 boot 存活——两者证据链互补，撕毁率四成的串口面正是账本的存在理由。
- `CBOX.LOG` 单簇帽轮出后老记录不可恢复（FAT 单文件面）——跨账本聚合由宿主 `berry crash-log` 多镜像参数承接（v1 只单镜像，多镜挂刀 3 后按需）。
