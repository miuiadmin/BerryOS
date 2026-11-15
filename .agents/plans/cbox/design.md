# cbox（黑匣子崩溃账本）总体设计

Type: spec
Status: active
Updated: 2026-11-16
Version: 0.5.1

一句话说明：cbox 是 BerryOS 的崩溃记录体系——内核异常路径写 RAM 崩溃记录表（黑匣子），用户态监督者经 syscall 排液落盘 FAT 账本（`CBOX.LOG`），宿主 `berry` 工具读账本做指纹聚合与符号化定位。对标华为 AGC / Android tombstone 的「采集 → 持久化 → 符号化 → 聚合 → 定界 → 报告」流水线，按单机无云端形态裁剪：后台等价物 = 宿主侧 berry 子命令。

## 背景与对标

- 触发 = 2026-11-15 owner 提交的华为 AGC 崩溃分析后台对照（com.berry.browser CPP_CRASH 报告 + 问题详情页截图）：八部件 = 崩溃捕获（signal/地址/模块/前后台/构建号）、持久化与下载、符号化（pc+模块+偏移 → 函数名）、故障指纹聚合、多维筛选、根因定位（三级表 + lastFatalMessage 原文 + 修复建议）、责任定界（so 路径定应用侧/系统侧）、问题单工作流（状态/优先级/备注）。
- BerryOS 现状：捕获面在役不弱（内核 `EXCEPTION` dump 寄存器全集 + `G1.5` kill 链 + 用户态自陷前诊断行 `WTDIE` 族 + svcmgr eco 降级行），但全部只进串口——串口 canary 交错实测撕毁约四成（`.agents/notes/implemented/2026-11-15-svcmgr-wtdie-selftrap-forensics.md` 可复用取证节——wip/boot-events-knife6 lane 件 e58c94ccf，未合入本枝，合入后到席），掉电/重启即失；符号化靠人工 stripped ELF 三件套；聚合/定界/报告为零。
- 立法承接：`docs/应用规范-v1.0.md` §4.1-5 已立「崩溃报告 = eco 降级行 + 死因/退出记录 + 代号重启链」，并立法**禁用「墓碑」词指崩溃转储**（BerryOS 墓碑 = pause(save)→relaunch 读档复活谱系，`docs/app-1实施设计草案-v1.2.md` 术语注；两件均未合入 lane 件，合入后到席）——本模块术语随之：黑匣子 = 内核 RAM 记录表，账本 = FAT 落盘文件。
- 术语另注：smoke 在册标记 `TOMB ok` 属生命周期墓碑自验，与本模块零关系；本模块新标记 `CBOX ok` 与既有全部标记互非子串（对照 `tools/smoke-image.sh` ECO_MARKERS 清单核过）。

## 范围

**做（v1，刀 1–3）**：内核异常路径崩溃记录（EL0 fault / EL1 fault 两类，环形 8 槽 RAM 表）；排液 syscall（ABI 0x42，copy-out 全量快照）；纯核 crate `berry-cbox`；svcmgr 两窗（battery 标记 + 监督死亡面钩子）+ launcher init 第三窗落盘（见持久化节）；宿主 `berry symbolize`（ELF symtab 最近符号）与 `berry crash-log`（FAT12 根文件直读 + 指纹聚合报告）。

**不做（v1）**：内核 panic 落盘（内核无块设备驱动——vblk 是用户态服务；内核直写盘需内核 virtio-blk 驱动，挂刀 4）；svcmgr 自身死亡的落盘（排液者即死者，同因挂刀 4 第二排液者方案）；构建号维度（`.berryver` 三元组在 wip lane，合入后账本行补 tag——见挂账）；AI 分析与修复建议（数据面立起后的远期）；ANR 类监督超时事件的入账（v1 只记异常 fault，监督杀面由 eco 行 + 账本 `kind` 间接可见）。

## 契约

### 记录格式（CboxRec，定宽 repr(C)）

```text
偏移  字段      宽度  义
0     magic     4     0x58424F43（"CBOX" 小端）
4     kind      1     1 = EL0 sync（用户任务 fault，G1.5 杀）；2 = EL1 sync（内核 fault，halt 前）
5     ver       1     记录格式版本 = 1
6     rsvd      2     零填充
8     seq       4     全局单调写序（1 起；0 保留——排液方去重游标）；fetch_add 饱和处理——2^32 次故障后钉在 u32::MAX 不回绕（回绕 = 去重游标全体失效，宁可停止排队）
12    stamp     4     槽代序 seqlock：偶 = 稳态 / 奇 = 写入中——排液见奇或复读变值以 magic=0 哨兵记录跳过
16    vct       8     crash 时刻 CNTVCT 原始周期
24    name      16    任务名 NUL 填充（CURRENT 空态的早期 boot fault = "(boot)"；写侧 min(15)——满 16 字节名必留至少一个 NUL，行编解码 1..=15 可见字节互逆契约的单侧源）
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

写入点：`exception_sync_el0`（dump 之前）与 `exception_sync`（EL1，halt 之前）两处。IRQ 屏蔽面分臂如实：EL0 臂 taken-from-EL0 异常自动 PSTATE.I=1；EL1 臂 same-EL 异常**不**自动屏蔽（`arch/aarch64/src/trap.rs` 同注）——设计不依赖屏蔽：**零分配、零锁、零 uart**，seqlock + 原子分槽本身抢占安全（IRQ 至多延迟单条写入；读者见不稳即跳）。槽位分配用 `AtomicU32::fetch_add`（NCPUS=2 双核可同时 fault，各自拿各自槽，last-writer-wins 可接受）；字段纯 store；撕裂防护 = 每槽 `stamp` 轻量 seqlock（写者先抬奇、写域、Release fence 后落偶；读者 stamp 复读夹 Acquire，不稳以 magic=0 哨兵告知跳过）；表读写皆 volatile 配对（Rust 抽象机数据竞争规范化——plain load 并发 write_volatile 是 UB，volatile 配对才是合法读）。BRK recover 路不写（非崩溃面）。

### ABI 面（0x42 cbox_drain）

- `berry_cbox_drain(buf: *mut u8, cap: u32, out_len: *mut u32) -> i64`：全量快照 copy-out（表头 12B {ver, total_ever, stored} 顺排 + 存量记录按 seq 升序）；`cap == 0` = 尺寸查询（`*out_len` = 所需字节，返 0）；`cap` 不足 → `-ENOMEM`（不部分写）；出参 `out_len` = 实拷字节数；`cap` 值超出 u32 可表示域 → `-EINVAL`。**两趟同锚**：尺寸查询趟与拷贝趟共享同一 `drain_total()` 锚（`cbox::drain_with(total, …)` 夹取 `min(total, now)`）——同锚下两趟结构性等长（12B 头 + 存量×104B；不稳槽换哨兵不改行长），趟间新崩溃不会造成「部分拷出后才 ENOMEM」的破窗形态。经 `copy_to_user` 界检（syscall.rs :230 既有件），caller_el1 同门。无 right 门（诊断几何面——`process_get_exit_code` need=0 惯例）。
- 入册：`tools/abi.toml` 异常组 0x42（0x41 thread_create_exception_channel v2 预留位隔壁），stage = "v1"、implemented = true → `just abi-gen` 再生四产物（kernel/berry-sys abi_gen.rs + ABI 文档 §4/§8.2 + abi.syz）。原型层手写双镜像（berry-sys `lib.rs`）同批。
- 语义边界：排液是**快照非消费**——记录永驻至槽位轮转；排液方自持 last_seq 去重。同 boot 多次排液幂等。

### 持久化（用户态半件：三落账窗）

纯核单源在 `user/berry-cbox`（快照解码/行编解码/`(boot, seq)` 去重合并/超帽轮出/boot 续号——svcmgr/launcher/宿主 berry 工具三方同引，host 单测钉板）。落盘 = 整文件读-改-写 `CBOX.LOG`（卷根 8.3 名，`LAYOUT.JSN` 惯例同形）。fsproto 严格串行无请求号（一切 client 端是同一通道端点的 dup、共享单应答队列——两活跃客户交叠 RPC = 回混毁会话），故落账窗安全 = **机制性唯一活跃客户**，非时序赌目：

- **窗一 battery（svcmgr）**：fsmgr spawn Ok 后、vnet 预拉前——fs 端点集刚建，vnet/launcher/app 全未起，svcmgr 机械唯一 fs client。门 = `fs_client != 0`。期望 n≥1：boot 期 rogue 自验件（`user_n_ptr` 族）确定性 fault 早于本窗（vblk 自验 RPC 往返前）。成功打 `CBOX ok n=<账本现存行数>`（车辆面在册标记）；失败 eco 降级行 `cbox bat`。RAM-only 形（EFI 冒烟路——无 virtio）fsmgr 照起、账本落 RAM 树、标记照发；盘面真值验收走主路（见验证节）。
- **窗二 死亡面（svcmgr app_supervise）**：TERMINATED 命中后排液——新 seq 在册恒打 eco 行 `cbox rec`（serial 面包屑，与 fs 无关）；fs 落盘只在**死者是最后一个活 app**（post-take APP_CHILDREN 表空 = 无并发 fs client；launcher 在 app 相位结构上不写 fs——layout RMW 只在 init 期，guifbd/wmd 零 fs 面）门后，经 FS_A 会话 + `cbox_drain_stale` 预排空（死者 mid-RPC 残留应答先 recv 至 EAGAIN、随行 handle 关净丢弃，不 trap）。多 app 并存时不写——活者可能正处 fs RPC 中途，宁缺勿毁；记录留内核 RAM 表延后到下个安全窗。写失败 eco 行 `cbox w`。**gen-2 init 窗闸**（轮 2 审计补）：launcher 监督重启后 gen-2 立即重入 init（icons/layout/cbox_io 全 fs 面）发 RPC——sole 门对此盲（它只看 APP_CHILDREN）；此时窗二排液的 `cbox_drain_stale` 会把 gen-2 在途应答当残留吃掉（回混毁会话）。闸 = `LAUNCHER_INIT_UNTIL`（assemble 成功即 `now()+10s` 帽、launcher 死亡即清零）：窗内死亡面不落盘（记录留 RAM 表，`cbox rec` eco 行照发），窗过期即恢复。时间闸是 v1 唯一可实现面（协议无 init-done 信号）；boot 形态结构性不需要（assemble 先于稳态环）。根治 = launcher 独立 fs 会话（见挂账）。
- **窗三 launcher init（berry-launcher）**：layout 装载后自持 fs 会话（extras fs dup，libc 单 fd 严格串行——layout_io 同纪律）排液落账一次——catch launcher 自身上一会话崩溃（监督重启后 init 重入）与 battery 后新入账 fault。冻结相（`fs_h()==0`）结构性零 RPC 直退；失败非致命照进桌面（诊断小写 `launcher: cbox` 前缀，不产 `CBOX ok` 子串）。
- **gui8 冒烟相三窗全冻结**（2026-11-16 补）：`GUI8.SMK` 哨兵（载荷 `BERRYOS-GUI8-SMOKE-V1\n` 精确匹配，与 launcher `layout_io::smoke_mode` 同一哨兵——wire 契约常量双读者）在席时 cbox 一切 fs 落账窗直退——窗一/窗二经 svcmgr `cbox_flush` 顶上门（`cbox_smoke_freeze`，battery/死亡面共用收口点；battery 标记面变 `CBOX ok n=0` = fixture 世界空账本真值；死亡面 stale 排空在门前照走），窗三 launcher `if !smoke { init_flush() }`。根因：ktest 每 boot 必记一条 rogue kill（on_fault 钩，故意杀非真崩溃），boot B 排液必有新账必重写，撞 gui8「跨 boot 读路径零重写」盘哈希不变量（实证：launcher 门先行后仍红，boot B 串口 `CBOX ok n=2` = 窗一重写；`berry crash-log` 解出 CBOX.LOG 2 条 = boot 1..2 各一条 `user_rogue_ptr`）；fixture 世界冻结语义与 layout_io 冒烟支路同族，常态 boot 落账面不变。

三窗同账本互恰 = `(boot, seq)` 去重（键在账本行内，跨写者各自持见闻——非单游标）+ boot id 内容匹配续号，无跨写者协调面。boot id 规则（`cbox::boot_pick`）：账本空 → 1；**最新（max_boot）boot 组里**有一行解析 Rec 与快照某记录全等（同 seq）→ 复用 max_boot（同 boot 幂等重入）；否则 max_boot + 1——内容匹配只对最新组生效（旧组全等行不构成续号：上上 boot 的同内容崩溃不得把本 boot 并进旧组）。

- 账本行格式（一行一记录，ASCII，字段序冻结 add-only）：`CBOX v1 seq=<n> kind=<el0|el1> task=<名> pc=0x… far=0x… esr=0x… lr=0x… sp=0x… x19=0x… x20=0x… x21=0x… vct=<十进制> boot=<id>`；解析严格（非数字字节显式拒）。
- 写策略：合并去重 → 超 `LEDGER_MAX`（4096B = FAT 两簇；mkfat SPC=4×512B）时**老记录先行轮出**（环形语义，与内核 8 槽同构）→ 分片 PUT staging → **`TRUNC(len)` 才是 commit 点** → 读回对拍。读侧 Noent = 干净空账本；其他 Err = 中止落账（瞬态错误下重写 = 毁旧账，宁可不落）。写失败非致命（eco 行，监督面照跑）。
- **写失败 = 撤销性回写，不 close**（轮 2 审计定谳改）：libc 的 close 对可写 fd = 游标处 TRUNC 提交——写半途 close 会把部分写入提交成账、毁掉旧账；fsproto 无 discard RPC 可撤 staging。而 staging 是 fsmgr 服务全局单槽，**跨名写自动提交残留**（stage_put 见异名即把上一名 staging 提交）——部分新账留在槽里，后续任何异名 fs 写都会把残缓当 `CBOX.LOG` 提交。故写失败当场**回写旧内容**（撤销性回写）：launcher 侧经第二个 fd 全量重写旧账 + close（= TRUNC commit）一次清 staging 槽 / 盘真值 / handle_get 读影三面，首个 fd 维持不 close（泄漏有界，每会话 ≤2）；svcmgr 侧 `cbox_restore_ledger` 同构（re-put 旧内容 0..old_len + commit(old_len)）。回写只限 **commit 点前**的失败（PUT/TRUNC 段）；读回失配（`-997`）= 新账**已提交**（盘上真值已是新内容），回写旧账 = 撤销已提交数据——不回，loud eco 行交调用方。回写自身失败 = 尽力面（残留语义回落「同名全量重写自愈」：下窗同名单槽重写覆盖）。

### 工具面（berry CLI）

- `berry symbolize --pc 0x… --elf <路径>`：ELF64 symtab 最近符号（≤ pc 最大 st_value；strip 件回落 dynsym，双空 = 报错指路构建产物位）。`--pc` 带/不带 `0x` 前缀皆受（两形态同值——CLI 集成测试钉板）；解析失败 = 非零退出（宿主脚本面契约）。符号域**两层滤**：① SHF_ALLOC 装载节（`.debug_*` 的 `$d`/`$x` 映射标非装载）；② `$` 前缀名（AArch64 映射标——真内核 ELF 装载节 `.text` 内也有 `$x`/`$d` 边界标，光靠装载滤不净；它们永不为人类答案）。零新依赖——手写 ELF 头/节表/symtab 解析（shnum==0 扩展节计数形态支持——真节数在 sh[0].sh_size，夹文件长帽）。**野几何防御**（轮 2 审计）：节表偏移全链 checked 运算（shoff=u64::MAX 一类野值在朴素 `+` 下 dev 剖面 panic、release 回绕出「错位成功读」形——Err 出不 panic）；symtab 迭代帽只收节内**完整 24B 项**（sh_size 非 24 倍数的尾巴残项曾被当符号项读）。符号源现实（刀 3 定谳）：内核 ELF（berry-ld 链但保留 symtab）可直接查；userbins release 件被全 strip（仅余 .shstrtab）——双空如实报错指路，不静默造假（userbins 符号面挂构建改造，见挂账）。
- `berry crash-log --disk <镜像>`：FAT12 只读直读（卷面发现：LBA0 非法 BPB 则落 LBA16——主路印章区形态；根目录 → 簇链）→ 解析 `CBOX.LOG` → 按 `(kind, task, pc, lr)` 指纹聚合 → 报告：每指纹次数 / 首末 boot id / 任务名 → 符号化（次数降序、并列按指纹序——报告序钉板）。未知 kind 数值可见（`k<N>`，不 `?` 丢信息）。符号解析（`--elf-dir` 给定时）：任务名先过**路径段先验拒**（`/`、`\`、`.`、`..`——账本 name 域来自崩溃记录非 CLI 校验域，join 前拒防 `--elf-dir` 逃逸，独立态如实报）；`user_<名>` 族先试 userbins release 件（现实全 strip → 双空报错冒泡），回落内核 ELF `__user_<名>` 锚点重定基（任务空间基址 `USER_CODE_VA = 0x1000`：pc − 0x1000 + 锚点 vma）；内核名直查；**派生常量名**（`user_child`/`user_thread`——内核对无名单监督任务的统一记名，运行期才知真身）独立态如实报「结构性不可符号化」，不走锚点查找拿误导文案。内核 ELF 候选序 = worktree 本地 debug → 全局 `~/.cargo-target` debug → **两处 release 变体**（轮 2 审计补——全局 release 件此前不在候选链，release-only 构建面误报 NoElf）。退出码：有记录 0；镜像/账本不可读非零；**账本非空而零行可解析 = 全毁形态非零**（不可与「健康空账本」混淆退出码）；部分坏行不致命——坏行不计指纹，人类面报头注 `（拒 N 坏行）`、JSON 面 stderr 注一行（`--json` 契约 = stdout 纯 JSONL、human 诊断走 stderr——CLI 集成测试钉板）。**链断 = 账本撕裂**（目录项在而簇链不足 size——掉电/中断形态）→ 如实报错非零退出，不静默截断；**簇链成环** = 跳数超盘容量增长帽报 Loop（野 size + 环可检；size 诚实的环读面无感——FAT 元数据盲区，见挂账）。FAT 解析全程 FatErr 不 panic（几何参数野值——spc=0 / 簇号越界 FAT 窗——一律 Err 出）。
- 任务名 → ELF 定界即「责任定界」的 BerryOS 形态：加载基址全系统固定（`USER_CODE_VA` 族），pc 归属由任务名单值判定，无需华为式 so 路径扫描。

### 分期刀谱

| 刀 | 内容 | 验收 |
|---|---|---|
| 刀 1 | 内核半件：cbox.rs + trap 双钩 + ABI 0x42 入册再生 | `cargo check` 零新告警；`just abi-check` 绿 |
| 刀 2 | 用户态半件：berry-cbox 纯核 + svcmgr 两窗（battery/死亡面）+ launcher init 窗 + CBOX.LOG 落盘 | smoke-image 串口 `CBOX ok n=` 到席 + 账本文件在盘 |
| 刀 3 | berry 工具：symbolize + crash-log | 端到端：造崩溃 → 账本 → crash-log 报告出符号名 |
| 刀 4 | DEFERRED: 内核 panic 落盘（内核 virtio-blk 最小驱动 / 预留扇区直写）+ 第二排液者（svcmgr 死亡面）| 原因：内核无块设备驱动；触发：内核态崩溃取证需求进前排 |

### 验证

- 刀 1：`cargo check -p berry-kernel --target aarch64-unknown-none` 零新告警；`just abi-check` 逐字节绿。
- 刀 2：`SMOKE_KEEP_LOG=1 just smoke-image`——串口 `CBOX ok n=` 到席 + 与 rogue `EXCEPTION` 行数对账（EFI 路 = RAM 树落账面）；FAT 盘面真值 = 主路 `just smoke` 后 disk.img 根 `CBOX.LOG` 宿主侧字节直检。
- 刀 3：test-host 单测（symbolize 合成 ELF 钉板 + 真产物 opt-in 对拍、FAT12 对真 mkfat 产物、聚合对合成账本）；端到端配方 = 跑一辆含 rogue 的车 → `berry crash-log --disk <盘> --elf-dir <仓根>` → 输出含 `__user_<名>+<偏移>（内核 blob 重定基）`。
- 回归：`just test-host` 全绿；`just smoke` 全绿（环境碰撞窗现实下按 `gate-contention-five-forms` 判读三步处置，全绿补跑挂合并前——先例纪律）。

## 边界与挂账

- userbins 符号面：release 件被全 strip（仅余 .shstrtab）——`crash-log --elf-dir` 对 userbins 名现状必然走内核 blob 重定基兜底（`user_<名>` 族内嵌 blob 的 pc 本就归于内核 ELF 符号空间，兜底即正解）；非 blob 型 userbins 真符号 = 构建改造保留 symtab（或符号侧产物），按需挂后续。
- 单簇件撕裂不可见：FAT 元数据撕（首簇项释放）对 ≤1 簇文件读面无感——读满首簇即止不走链（B4 掉电竞技场 735B 实测形态即此，数据无损）；撕链可检性以 `size > 簇容量` 为界，跨簇链断才报 Torn。同族盲区：**簇链成环 + 目录项 size 诚实**（环内增长可达 size）→ 读满即止数据错而不报——FAT 元数据无环检面（本工具只读不改 FAT），野 size（超盘容量）+ 环才报 Loop；读侧环防御 = 跳数帽 `盘长/簇字节 + 2`。
- fsmgr staging 残留跨名窗：staging 是服务全局单槽——写失败撤销性回写（写策略节）已把窗内残留当场清掉；残余面 = **回写自身也失败**（尽力面）后恰有异名写入插队，fsmgr 会把残留当上一名提交（`user/bins/fsmgr/src/fat.rs` stage_put 跨名自动提交语义）。结构性修复 = fsmgr per-fd staging 或 discard RPC，挂 fsmgr 改造（v2 候选）。staging-first 读影（handle_get 先查 staging 后读盘）由回写三面清理同除。
- **gen-2 init 窗回混**：launcher 监督重启后 gen-2 init 期 fs RPC 与窗二 `cbox_drain_stale` 排空的交叠面（共享应答队列）——现缓解 = `LAUNCHER_INIT_UNTIL` 时间闸（窗二节）；时间闸是概率性收口（10s 帽内 init 未完成即恢复暴露），根治 = launcher 独立 fs 会话（fsmgr 服务端 per-client 会话隔离，v2 候选）。
- **boot 收口窗不排液**：init 窗闸期内死亡面不落盘（`cbox rec` eco 行照发）——记录留内核 RAM 表等下窗；闸期反复 launcher 崩溃叠加 >8 次即触 8 槽轮出丢失面（下行）。
- **8 槽延后上界**：RAM 表环形 8 槽——两安全窗之间 >8 次故障则最早记录被轮出永久丢失（账本只收幸存窗内快照）。v1 接受（监督链单窗故障数现实个位数）；扩槽 / 排液阈值触发挂 v2。
- **派生任务身份面**：内核 syscall.rs 对无名单监督任务记常量名 `user_child`/`user_thread`——账本面如实态（SymSrc::Derived，结构性不可符号化）。v2 = 内核记录真身（spawn 元数据名或映像锚），工具面自动跟随。
- 内核 panic / svcmgr 自死两大「黑匣子随机同毁」面挂刀 4（见分期表 DEFERRED 行）——v1 账本覆盖面 = 监督链可观察的用户任务崩溃（rogue 自验件 + launcher/app 监督面 + guifbd 类常驻任务 fault），恰是 wtdie 案「launcher 停摆连坐链」一类的取证主需求面。
- 多 app 稳态死亡面不落盘（窗二 sole 门未开——活者 fs RPC 交叠风险）：记录留内核 RAM 表延后到下个 sole 死亡/窗三/下 boot battery；稳态即时落账挂多 app 会话串行化（v2 候选）。
- 窗二 stale 应答复余窗：死者 mid-RPC 应答可能恰在 `cbox_drain_stale` 排空后才入队（服务端处理中）——与 bsh「唯一读者」纪律同类（既有系统性纪律，本刀不加重）；窗一/窗三不经此面。
- boot_pick 全等并列边角：跨 boot 字节级全同崩溃（含 vct 恰等）会并组（两 boot 归一 boot id）——零数据损失、归组错位，挂账接受。同族：内容匹配只对最新 boot 组生效（契约节）+ 回写撤销已提交新账的极小概率形态（写策略节 close 失败二义）——两者都表现为 boot 维度重分裂（下 boot 续号开新组），零数据损失，同挂账接受。
- RAM-only 形标记行为：EFI 冒烟路无 virtio——battery 落 RAM 树、标记照发；盘面真值验收走主路 disk.img（验证节刀 2 行）。
- 构建号维度：wip lane `.berryver` 三元组合入后，账本行补 `build=<三元组>` tag（改点 = svcmgr 落盘处单点，格式 ver 不动——tag 是行尾追加）。
- 串口 dump 与账本**双写并存**（不撤串口面）：串口是全寄存器深度取证，账本是指纹聚合与跨 boot 存活——两者证据链互补，撕毁率四成的串口面正是账本的存在理由。
- `CBOX.LOG` 两簇帽（LEDGER_MAX=4096B）轮出后老记录不可恢复（FAT 单文件面）——跨账本聚合由宿主 `berry crash-log` 多镜像参数承接（v1 只单镜像，多镜挂刀 3 后按需）。
