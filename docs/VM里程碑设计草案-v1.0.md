# M-VM 里程碑设计草案（虚拟机里跑起来 + 命令行完善）

> 版本：**1.0**（v1.0）｜2026-11-06

> 2026-10-22 裁决（ADR #49）：**第一个用户可感里程碑 = 「在虚拟机里运行 + 命令行完善」**。
> 本文回答四件事：验收线是什么（§0）、按什么刀序做（§2–§3）、怎么和既有资产挂钩（§7）、哪些地方会咬人 + 何时止损（§5–§6）。
> 不是新架构决策——引导/驱动/FS/ABI 的架构决策分别见既有 ADR（#13 遗留修订面、#46 驱动模型、#45 存储、#44 应用包、ABI 单源 `tools/abi.toml`）；本文是里程碑级的拆解与排序。

## 0. 目标与验收线（三条全达成 = 里程碑收官）

| 关卡 | 验收线 | 自动化面 |
|------|--------|---------|
| **M-G1（第一站）自举镜像** | `just image` 产出一个磁盘镜像；**QEMU 不带 `-kernel`、只挂盘 + UEFI 固件**（AAVMF）从盘引导 → BerryBoot → 内核 → bsh 提示符可交互；rootfs 递送不再依赖 fw_cfg。UTM 挂同一镜像按手动清单点验（UI 无法 CI，QEMU+AAVMF 是自动化代理——UTM 底层就是 QEMU+UEFI）。 | `just smoke-image`（独立关卡，x86 最小集先例：先独立、收官时评估并入主 smoke） |
| **M-G2（第二站）Fusion 窗口** | VMware Fusion (ARM64) 虚机从镜像启动，**图形窗口出 bsh 提示符**、USB 键盘可打字；串口不再是唯一 console。允许降级线（§6 kill criteria）：窗口只读 demo + 串口键入兜底。 | 无 CI（真 Fusion 手动验收）；设备面实证刀（V-5a）产出的事实表是唯一可审计物 |
| **M-G3 CLI 可用** | bsh 下完成闭环：`cd` 进目录树 → `ls` → `cp/mv/rm/mkdir` → 管道 `|` + `grep` → `>` 重定向 → **重启后读回**；~20 个 uutils 命令 + `ps`/`berryctl` + Tab 补全 + Ctrl-C。 | `just smoke` 演示相（C-5，rootfs/RX 相扩段） |

**话术限定（验收口径，防过度宣称）**：
- 「自举镜像」≠ UTM 图形窗口——第一站的 console 仍是串口（QEMU stdio / UTM 串口终端），窗口点亮是第二站。
- 「窗口点亮」≠ GUI/桌面——GOP 线性帧缓冲 + 8×16 点阵字体 + 键盘回显，无窗口系统、无 GPU 加速、无图形栈。
- 「CLI 可用」≠ POSIX 全量——v1 仍无 fork（ADR #14）、信号面仅 Ctrl-C 一条语义路径、stat 元数据按 FAT 实情简化（§2 C-2）。
- 「能装进虚拟机」≠ 「装进物理机」——P8 真机不变；本里程碑不触发任何真机议程。

## 1. 现状盘点：离三站各差什么

已有的（都是本里程碑的直接资产）：
- **递送链**：fw_cfg 具名文件 → rootfs VMO（0x0001_0002）→ svcmgr 流式装载（G-eco 全链）；DTB `/chosen` 双源 probe 已预埋（`kernel/src/dtb.rs`——但只解析 initrd 区间，不解决「没有 QEMU 替我加载」）。
- **FS**：fsmgr 服务（RAM overlay + FAT12 读/写 + 跨 boot 持久化相）；berry-fatfs / berry-fsproto / berry-blkproto 纯函数 crate 家族（host 117 测）。
- **块设备**：vblk 常驻服务（virtio-mmio）；MMIO 物理窗口 VMO + boot 授能模式（A-2，devmgr 手工前身——ADR #46 已认）。
- **shell**：bsh（管道/env/`&&`/`$?`/`>` FAT 重定向/mini-readline）；console 行编辑在内核 console 任务（HIST/退格/ESC 序列）。
- **std 面**：musl target + berry-libc shim + berry-ld（hexyl 0.17.0 上游未改动全链跑通 =「编译即跑」的活证据）。
- **应用包**：.bap v0（appd 雏形 + mkapp）。

差距（按站）：
| 站 | 缺口 | 刀 |
|----|------|-----|
| M-G1 | **引导加载器从未开工**（`bootloader/` 目录留位至今空）；UEFI 协议面零接触；镜像分区表/打包；内核 handoff 面（x0 现在只认 DTB 指针/0）；FAT12 容量不够装 rootfs+内核（FAT16） | V-0 ~ V-4 |
| M-G2 | Fusion 设备面**未知**（GIC v3?、盘控制器 NVMe/AHCI?、有无 PL011?、USB xHCI、GOP）；帧缓冲 console 与 USB 键盘两个驱动件 | V-5a ~ V-8 |
| M-G3 | fsmgr 平面表（无目录树/路径）；libc 无文件面（fd 3+ 不存在）；bsh 无 PATH 查找、无 Ctrl-C/kill 面、无 Tab 补全；ps/berryctl 不存在 | C-0 ~ C-5 |

> **进展注记（2026-11-11 更——上一版 2026-10-28）**：M-G1 五刀全落（`just image` / `just smoke-image` 挂 CI，唯余 UTM 手动点验）；**M-G3 全链收官——C-0/C-1/C-2a/C-2b/C-2c/刀 2/挂账刀/C-3/C-4（+C-4b Ctrl-C）/C-5 已落**（C-5 = 里程碑演示相 `M3DEMO ok n=5`，rootfs 相 35——本文件 §2 C-5 注记）；M-G2 非未动——**实证刀 V-5a 工具面已落**（V-5a-0 berrydump 采集器 2026-10-28 + V-5a-1 段① AAVMF 首验基线 2026-10-28 + 段② 帽审计/段③ lspci 对拍清单同日入册，附录 B AAVMF 参照列已回填），**V-6 全线收案 2026-10-29**（刀 1 GOP 携面 + 刀 2 fbcon/guifbd 双路上屏，`FBW ok`/`FBSERVE ok` 入 smoke-image canon）；余项 = V-5a 收口（Fusion 实机会话——附录 B Fusion 列仍「待实证」）+ V-5b（盘驱动）/V-7（USB 键盘）冻结中、真押事实表。上表为 2026-10-22 立项时点的差距评估，勿当现状引用。

## 2. B 线（CLI 四件，M-G3）——先做，早交付早验收

> B 线不依赖引导加载器，全部在现役 fw_cfg 链上做，每刀独立可验收。

### C-0 fsmgr 目录树 + fsproto v2
- **fsproto v2**：请求面从「单名」升「路径段数组」（或整条路径字符串 ≤PATH_MAX，编码用 NUL 分段——沿用 env 块 argv 的字节串先例）；新增 op：`LOOKUP/MKDIR/UNLINK/READDIR/STAT`；严格串行 RPC、编码器 None 拒绝契约、帽常量双侧同步义务——三者照旧（fsproto 纪律原文进新协议头注）。
- **fsmgr 树**：RAM overlay 从 `Vec<(name, data)>` 平面表升树（每节点 = 名 + children + data）；**FAT 面升目录**（berry-fatfs 增子目录链解析：DIR 属性项即目录文件、簇链走查——读面先行，写面子目录 v1 缓做、`>` 只落根，诚实缺口文档化）；挂载形状：FAT 卷挂 `/` 与现状兼容（大写 8.3 名在根），RAM 影子其上——**overlay 语义与名字空间按构造隔离（RAM 自由名 / FAT 8.3）不动**（A-5/A-6 既定裁决）。
- **回归线**：FATW/持久化相零漂移是硬门槛；fixture 共识测试 + `--check-fixture` 守卫随 FAT16 扩展同步（V-2 联动）。

### C-1 berry-libc 文件面
- fd 表：0/1/2 console 通道（现状不动）+ **fd 3+ = fs-backed**（open/close/read/write/lseek/stat/opendir/readdir 走 fs RPC；fs channel 经 env 块 gift 交付，vnet/fsmgr 同款）。
- chdir/getcwd 是 libc 侧字符串状态 + 每 op 全路径解析（cwd 前缀拼进请求——服务端无会话，严格串行 RPC 不变）。
- **bsh 内建迁同一路径**（cat/ls 不再直调 fs_rpc——「不长第二套」）；`run` 的 spawn 查找升路径语义（§C-2 PATH）。
- 验收：stdhello 消费面扩 `File::create/write/read_to_string` 自验（std::fs → libc → fs RPC 全链）；uutils 的运行基座即此刀。

### C-2 uutils 批（crates.io 上游逐字不动，hexyl 先例：`=版本` 锁死 + CRT0_SEED 尾锚 + musl target + berry-libc）
- **C-2a core 批（✅ 2026-10-25 落，`U1 ok` 九步，rootfs 相 24→25）**：`ls cat cp mv rm mkdir rmdir touch pwd echo true false`（12 命令）接入 rootfs——ls 列目录树、cp/mv/rm roundtrip、touch 建文件全验。
- **C-2b 管道面（✅ 2026-10-25 落，`U2 ok` 六步，rootfs 相 25→26）**：echo/true/false 执行验收（argv 多参/`-n`/真码/`&&` 链/管道对拍）。
- **C-2c text 批（✅ 2026-10-25 落，`U3 ok` 五步，rootfs 相 26→27；人类裁决改道）**：原拟 10 命令拆两半——**中批 6**（`head wc cut tr sort uniq`，uu 家族 12→18）五步自验（pwd 收编/cat/wc -c/cut|tr/三段链；head 挂 fcntl F_DUPFD 桩、sort 挂 std 线程面——执行步裁出随深坑挂账）；**深坑 6**（tail/env/date/grep/find/which）挂账另刀（`docs/挂账-C2c深坑件.md` 四节 + 源码归档：桩二分必留 4/可退 8、onig 互斥、自写三件）；随刀一体：容量五腿 48MiB（盘 24→48/帽 16→32/MAX_ORDER 13）+ EFI 路 rootfs 零拷贝借片（`RootfsBuf::Borrowed`，碎片免疫）。
- **刀 2（✅ 2026-10-27 落，`DF ok` 三步，rootfs 相 28→29）**：`df/du` + statfs/fstatfs 真值面——fsproto STATFS(10) op（无路径参卷级）→ fsmgr BPB 几何 + `count_free_clusters` FAT 扫描；uu_df/uu_du 薄 harness（uu 家族 18→20，bin 双发）；`/etc/mtab` fsmgr 启动预建（read_rs_list 二级回落）+ `st_dev` 非零定值（mount_dev_id 分组键）；bsh 三步 = ⓪几何定值（506/2048 恒定）+ ①RW.TXT 重定尺寸差分双向（相位无关——跨相 free 500/499 漂移免疫；原案 DFTEST.BIN 写删不可行：FAT unlink 留 v2）+ ②df 表格逐字节对拍（uu_df 渲染源钉板字面）。**du 自验步首跑实录裁出挂账**（uu_du 0.12.0 无条件 `thread::spawn` → std 线程钩子 outline-atomics ldxr 野指针 D-abort → G1.5 kill——C-2c uu_sort 同型 std 线程面缺口，随深坑刀回收）；两实录随刀入册：bsh `run_capture` 裸 spawn 子进程零 fs 能力（fs_h()==0 → ENOENT 与文件缺失不可区分——fs 消费子进程走 `run_capture_fs` gift 四元装配）、uu_df 直调 uumain 漏 `setup_localization`（表头变消息 ID 列宽漂——harness 手动 setup）。
- **PATH 语义**：bsh `run` → 非 base64 内建时查 `/bin/<name>`（cpio 内已有则直接 spawn；uutils bin 同链进 cpio）；栈页数默认表 + 按 bin 可覆盖（hexyl 32 页教训的结构化版——SpawnSpec 参数已在）。
- **诚实边界**：stat 元数据按 FAT 实情（size/名/目录位；权限恒 0755、mtime 恒卷定值——显示级差异不设关卡）；`df/du` 给 FAT 卷几何真值。

### C-3 ps + berryctl
- `ps`：svcmgr 查询 RPC（拉起者有 ChildHandles 表——name/state/exit code/reap 后残留诊断）；表驱动、零新内核面。
- `berryctl` v0：`ver`（版本串）/`ps`（同面）/`mem`（svcmgr 汇总：VMO 计数经既有 vmo_get_size 类查询——**内核遥测读 syscall 不新增**，telemetry_open 留 v3 挂账不动）。
- 验收：`ps` 输出含 bsh/echo/svcmgr/fsmgr/vblk/vnet 全家常驻名单——`PS ok` 标记。

### C-4 Ctrl-C + Tab 补全
- **Ctrl-C**：console 收 0x03 → 不进行缓冲、直接作为特殊消息转发 bsh；bsh 前台有子进程时杀之。**kill 面 = ABI 扩号**（v1 无 process_terminate——Plan B 只做了自愿退出）：0x3B `process_terminate(handle, code)` 走 **abi.toml 单源管线**（一次一号：meta 计数 + 条目 → `just abi-gen` 三面 → ktest F 段错误面 + svcmgr/bsh 消费面）；语义 = 置 TERMINATED + 收口同 exit 钩子（**杀 = 异常路径退出，走同一 task_exit_hook——退出路径唯一既有裁决不破**）。
- **Tab 补全**：词汇表快照方案——console 启动时经 svcmgr 拿一次 `/bin` 名单 + 运行期 fs LIST 缓存（新文件补不到 = v0 可接受，shell设计 §2.4 的「需 rootfs 知识」以静态近似先行）；补全逻辑在 console（编辑面在端点上游的既定架构不变）。真 RPC 到 bsh 的动态补全留 v2。
- 验收：smoke RX 相注入 0x03 杀一个 sleep 中前台进程 + Tab 补全 `he` → `hexyl` 回显——`INT ok` / `TAB ok` 两标记。

### C-5 里程碑演示相（smoke 化）
- 脚本化断言链：`ls /bin ≥N` → `mkdir /t && cp /bin/… /t/` → `cat f | grep <pat>` → `echo x$?y > T.TXT`（展开造内容，f42t 先例）→ 重启读回（持久化相扩）→ `ps` 名单。
- 全部状态谓词、零竞速面（4-5a 纪律）；标记带参形态互非子串（DEMO ok ⊂ AIDEMO ok 教训）。
- **✅ 已落地 2026-10-28**（bsh `m3demo_selftest` 五步 → `M3DEMO ok n=5`，rootfs 相 34→35/三合相 60→61；五相 smoke 全绿 + 金丝雀 30 严格递增 + test-host 363/0 + smoke-image canon 20 零变化）。四项拆解决策留档：① ps 名单步由 `PS ok n=` 承接（C-3 svcmgr ps_selftest 账本面——bsh 无法自造 psproto gift 通道，不重拉）；② 「ls /bin ≥N」落形为 cpio_names ≥38 + 七锚 + `ls /` 断言 HELLO.TXT/PATTERN.BIN（/bin 目录枚举面已有 DIR ok 承接，不重复）；③ 重启读回 = T3.TXT 故意留盘 + 持久化相注入 `cat T3.TXT` 断言 x1y——本相自验覆写幂等、跨 boot 正证仍 = OUT.TXT f42t（弱化面如实记载）；④ **cp 步 collect 帽 30s**（首跑定标实录：uu_cp TCG spawn→exit ≈17s 慢而活、10s 帽误判 eof=false——EOF 实发而帽先弃、reap 30s 帽内干净退 0；修法 = collect 帽参数化三层穿透 `collect_cap`/`run_capture_fs_cap`/`run_capture_gifts_cap`，svcmgr `U1_EOF_CAP` 同口径）。链序排 fat 族后（①ls 依赖挂载卷）→ EFI 路无盘 fat park 挡断、结构性不打。

## 3. A 线第一站（M-G1）：BerryBoot + 自举镜像

### V-0 BerryBoot v0（UEFI 应用，`bootloader/berryboot/`）
> **R1 风险位（§5）——首个引导加载器，拆两小步走。**

- **V-0a EFI 骨架**：Rust no_std EFI 应用。**免 nightly 路线**（G-eco-a 同纪律）：builtin `aarch64-unknown-none` 编 staticlib + `rust-lld -flavor link /subsystem:efi_application /machine:arm64` 出 PE32+（不引 custom target/nightly）；入口 `efi_main(ImageHandle, SystemTable)`，先落地：GOP/ConOut 打 banner + SimpleFileSystem 打开 FAT 卷 + 读一个文件逐字节验内容。验收 = QEMU `-bios QEMU_EFI.fd`（AAVMF）下串口出 `BERRYBOOT ok`。
- **V-0b 装载 + handoff**：读 `kernel.elf` → **berry-loader ELF64 解析复用**（crate 零分配零依赖，no_std 直接消费——「解析器即产品」第一次跨内核/用户态/引导三界复用）→ GetMemoryMap → **ExitBootServices（重试协议：map key 变了重取）** → 按 PT_LOAD 铺到物理内存 → 跳 entry，x0 = handoff 结构指针。
- **handoff 结构**（`#[repr(C)]`，BerryBoot/内核两份镜像 + const assert）：`{magic, memmap(ptr/len/descriptors), init_blob(ptr/len), cmdline 留位}`；init blob = 从盘上读的 rootfs.cpio——**fw_cfg 的替代者**。

### V-1 内核 handoff 面（三源归一）
- boot asm x19 抢救 x0 不动；`rust_main` 首参按 **magic 分流**：BerryBoot handoff / DTB 指针（真机/Linux 协议，现状）/ 0（fw_cfg 裸跑，**研发快路保留——日常 `just run` 不等镜像**）。
- rootfs 来源扩第三源：handoff init_blob → 拷贝进标准 VMO → 交付 svcmgr 表第 2 插 0x0001_0002（与 fw_cfg 路完全同汇，boot assert 定值不动）；DTB `/chosen` 路径照旧预埋。
- **内存拓扑按 handoff memmap 初始化**（现状 mm init 假设 QEMU virt 线性 RAM——BerryBoot 路下 buddy 初始化按 map 的 Conventional 段建；QEMU 路零扰动）。
- 验收：QEMU+AAVMF 挂盘启动走全标记（裸相 23 到齐 = 内核行为零漂移的证据）。

### V-2 盘格式 v0：MBR + 单 FAT16 分区
- 单分区 FAT16（UEFI spec 认 FAT12/16/32 做 ESP；可移动介质默认路径 `\EFI\BOOT\BOOTAA64.EFI` 恰 8.3）：**BOOTAA64.EFI + KERNEL.ELF + INITRD.BIN** 三件套。GPT/多分区/A-B slot 不做（ADR #47 L1 是系统目录级、不背分区）。
- **FAT16 = berry-fatfs/mkfat 双扩**（FAT12→16：fat 表项 12 位紧包 → 16 位定宽，簇数 4085..65524 域检——解析器「不静默按错宽解」纪律照旧）；**mkfat 确定性 + fixture 共识 + `--check-fixture` 守卫三件套同步扩**（A-5 对抗审查立的防线不允许因扩 FAT16 而破）；旧 FAT12 盘（smoke 现役）继续可读——**版本域在 BPB 自描述，消费端按域分派**。
- 8.3 名单（BOOTAA64.EFI/KERNEL.ELF/INITRD.BIN/RW.TXT/OUT.TXT/HELLO.TXT/PATTERN.BIN）全数合法——LFN 零需求。
- 验收回执：**V-2 FAT16 能力层已于 2026-10-25 收官**——mkfat/berry-fatfs/fsmgr 三面落地 + host 325 测 + 五跑零漂移 + FatDxe 实验实证（MBR/分区偏移与 build-boot 盘尺寸调整留 V-3）。

### V-3 `just image` 确定性打包
- `tools/mkimage.py`（mkcpio/mkfat 同纪律：无时刻输入、同输入同字节）：MBR 表（C/H/S 几何用现代 LBA 假几何，protective 值自洽即可）+ FAT16 卷（mkfat 复用，`build_volume` 公共函数化——**单编码器纪律**）+ 三件套填入。输出 `berryos.img`（gitignored）。
- 挂 `just build-user` 尾接（build → image 全链一步）；`just image` 命令演进表兑现（开发体验 §4）。

### V-4 img-boot smoke 相
- `tools/smoke-image.sh`（smoke-x86 同款独立关卡）：AAVMF 固件 + `-drive berryos.img` 启动 → 断言 `BERRYBOOT ok` + 内核全标记 + bsh 提示符 + `RX ok` 注入相；**UTM 手动清单**（同镜像、点验项 = 插盘/开机/到提示符/键盘输入四条）落本文附录。
- 收官前独立（ARM64 主 smoke 五跑零扰动）；M-G1 收官时评估并入主 smoke 为第六跑。

## 4. A 线第二站（M-G2）：Fusion 窗口

### V-5a Fusion 设备面实证（investigation-first，S-0/fw_cfg 方法论：先拿事实再写码）
- 产出物 = **设备事实表**（本文附录 B 回填）：Fusion ARM64 虚机的 GIC 版本与 CPU 数、盘控制器型号（NVMe? AHCI? virtio 有无）、**有无 PL011/16550 串口**（决定键盘降级线是否存在）、USB 控制器（xHCI?）、GOP 分辨率/格式、ACPI 还是 DT（发现机制）。
- 方法：Fusion 装 UEFI shell / 任意 EFI 调试 app dump（V-0a 骨架顺手复用）+ Linux live 起来 `lspci/lsdev` 对拍。
- **此刀定 V-5b~V-7 的范围盒**——事实表没拿全不动驱动码。
- **V-5a-0 工具面已落（2026-10-28，纯 host 产链零 QEMU）**：`bootloader/berrydump/` 独立 workspace EFI dump 应用（**不进** berryboot members / 不动 justfile / 不进 image 产链——dump 工具编译错零耦合 `just build-boot`/`just image` 主链；berry-efi 跨 workspace path dep 合法：纯数据 crate）。同刀 berry-efi 加法扩展：`EfiConfigurationTable`(24B) / `EfiPciIoProtocol`(176B，pci_read@64 typed Uint32 配置空间读) / free_pool@72·locate_handle_buffer@312 typed / ACPI20·DTB·PciIo·BlockIo·SimpleNetwork 五 GUID——**既有消费者 BerryBoot 零扰动实证**（BERRYBOOT.EFI 产链复跑 entry=0x1200 img=0x2458B relocs=5 形状不变；berry-efi host 13→15 测）。纯解析面 `app/src/parse.rs`：RSDP/XSDT/MADT/SPCR/DBG2/FDT 全 bounds-checked Option、畸形截断不炸（半程事实保留——MADT 尾随残字节执法点 host 钉板）；**ACPI packed 子表偏移钉板**（GICC flags@+13、GICD base@+9/ver@+21、GICR base@+5、GAS address@+4——与 C 结构体习惯偏移不同，合成 fixture 8 测钉死）。**runbook**：① `sh bootloader/berrydump/build.sh` → `out/BERRYDUMP.EFI`（16896B）；② 放进 FAT 卷 `\EFI\BOOT\BOOTAA64.EFI`（可移动介质默认路径——Fusion/AAVMF 直接自举；`just image` 产物换名同位即可）；③ Fusion ARM64 虚机挂盘开机，ConOut（窗口/串口）收割行式事实；④ 按行前缀对表回填附录 B。**缺席行也是事实**（`xxx: absent`——机器可读缺席报告纪律）。**V-5a-1 段① 已收官 2026-10-28**（AAVMF/QEMU 首验三实抓修复闭环，详见 `docs/阶段沿革.md` 同日条）：① MADT 偏移全套按真字节归真（条目起@44、ARM type 编号 0x0B..0x0F、ACPICA 结构视图子域）+ 撕裂 hex 重组法证；② **PciIo 布局两轮实抓**——真表 160B、开头 PollMem+PollIo 两枚单函数、Pci.Read@48/GetLocation@112（EDK2 头逐字裁决；布局错 = 恒 INVALID_PARAMETER 不炸的签名不匹配故障面）；③ SPCR GSI@54/baud@58 归真 + SpcrFacts 增 gsi 域；④ madtraw/spcrraw hex 锚转常驻行 + pci 失败 loud（pcih[i]/pcir[i]）+ pci[i] 增 `loc=` BDF 段；⑤ aavmf-firstboot.sh 自含首验脚本入库。**AAVMF 基线事实见附录 B 参照列**。挂账段②③④：Fusion 实机会话（用户手动）+ lspci 对拍 + 附录 B Fusion 列回填 = V-5a 收口。

### V-5b 盘驱动（按事实表选型）
- NVMe（预测概率大）：admin queue + IO queue + PRP——berry-virtio 同形纯函数 crate（RingLayout/描述符编解码拆 host 测）+ 常驻服务（vblk 纪律全套：静默常驻/超时重臂/服务不死）。
- 若 GICv3：内核 HAL GIC 面增量（GICv2 保留、按设备树/事实表分派——HAL 抽象的第一次真分派考验）。
- 若 Fusion 有 virtio-blk：直接复用 vblk（最好结局）。

### V-6 帧缓冲 console 服务
- 数据源 = V-0b handoff 携 GOP 信息（phys addr/w/h/stride/格式，XRGB8888 或 BGR8888 线性帧缓冲）→ boot 授能 MMIO 窗口 VMO（A-2 模式，第 N 插定值断言）→ **用户态 fbcon 服务**（驱动=持证应用精神；console 内容双路：串口照旧 + fbcon 并行——kernel console 任务只加 fan-out，不改架构）。
- 字形：内嵌 8×16 点阵（公有领域 VGA 布局，自绘数组入 `user/`，零版权面）；滚动 = memcpy 行移。
- 验收：Fusion/QEMU `-device ramfb`（或 AAVMF GOP）窗口出 bsh 提示符与串口同内容。
- **V-6a 刀 1（GOP 携面）已落地 2026-10-28**：数据链前半段——BerryBoot banner 期 `QueryMode` → `GopFacts` 快照（fb_pa/width/height/stride/format 五域）→ handoff `gop_*` @56..@76 填实——**V-0b 刀 3 冻结留位启用，version 不 bump（v0 布局零改动，smoke `HANDOFF ok ver=0` 断言零扰动）**。缺席语义 = 全零递送：无显示设备 / QueryMode 失败 / fmt=3 BltOnly（无线性帧缓冲）/ frame_buffer_base==0 任一 → `GopFacts::ABSENT`（装载器侧）；内核 `handoff::gop_info` 界检（四非零 + fmt≤2）双保险拒收。行字节数换算 `gop_stride_bytes`：fmt≤2 恒 32bpp → PixelsPerScanLine×4（checked_mul 溢出拒）；fmt>3 非规范 → None（宁缺勿猜）。内核消费面 = EFI 路专属打点行 `FB ok <W>x<H> fmt=<f> stride=<s> fb=0x…`（HANDOFF ok 后；QEMU `-kernel` 路恒缺席零扰动；与全部在册 smoke 标记无子串包含关系，smoke-image canon 零漂移）。**范围盒裁决（scoping carve-out）**：V-6 机制面开工走 AAVMF GOP 路径——§0/§4 验收线既有明文「Fusion/QEMU（或 AAVMF GOP）」双路径，GOP 几何运行时取自固件上报，**不押 Fusion 待实证项**（附录 B Fusion 列回填后零改动适配——BGRx/RGBx/BitMask 三格式皆携原值）；**V-5b（盘驱动）/V-7（USB 键盘）维持冻结**（真押事实表——V-5a 铁律不破）。验证 = berryboot host 测试 +1（1→2；`gop_stride_bytes_faces` 界检七面：fmt≤2 定宽/BltOnly 拒/非规范拒/溢出拒）+ smoke-image **EFI 面全绿**（EFI 十段 + HANDOFF/MMAP/ROOTFS + 裸相 26 脚本断言过）+ 串口实收 `FB ok 800x600 fmt=1 stride=3200 fb=0x43d00000`（与附录 B AAVMF GOP 参照行逐值一致——装载器上报与内核消费两视图对拍闭合）。**生态 21 相 FAIL 归因 T-1 组合树**：用户态故障被杀（`user_child`×3 / `user_svcmgr`，双 run + 兄弟会话独立 run 三点复现，皆在 T-1 多线程刀 WIP 脏区；GOP 携面 = boot 期数据面 + 打印，与用户态无因果链）——**canon 生态零漂移回归已销账（2026-10-28，T-1 修复后兄弟会话 smoke-image 绿 run 回执）**：`PASS` exit=0（EFI 十段 + 接续 + 裸相 26 + 生态 21 含 T-1 新标记 `THR ok` + kfile 3470976B/init 29466088B 字节对拍 + 金丝雀 100→1700 严格递增），`FB ok 800x600 fmt=1 stride=3200` 同链全绿——GOP 携面与生态面同 run 双证；完整输出 /tmp/t1-fix-smoke-image.log、抽验串口 /var/folders/.../berryos-smoke-image.1XtAEP 留档（临时件，回执已转录本条）。**刀 2 挂账**：fb MMIO 窗 VMO 授能（A-2 模式 boot 插表定值断言）+ 用户态 fbcon 服务（8×16 字形/滚动行移/console fan-out）+ smoke 标记（像素读回断言形态，非 grep 像素）——待 T-1 多线程刀 WIP 提交后动（user 工作区注册面 + smoke.sh 均在其脏区）。
- **V-6 刀 2 前置段（fbcon 纯函数 crate）已落地 2026-10-28**：`user/berry-fbcon/` 独立小工作区（berrydump 先例：自带空 `[workspace]` 脱离 user/ members——刀 2 后段服务接入时随注册迁正），font/render 两模块 host 测 **28/28**（font 13 + render 15）。**font 半边 = 生成器路线**：`tools/genfbfont.py`——art 画稿（`#`/`.` 可视化、自绘原创设计公有领域——零版权面可审计，生成器头注明令禁抄外部字体数据）→ `font_data.rs` 单一事实源生成（abi_gen 同款纪律），生成器钉板结构约定（rows 0..=2/row 15 恒空、行宽 8、col 7 右留白、12 行帽）；`font.rs` 承载 API（`glyph(ch) -> Option<&'static [u8; 16]>`：32..=126 域内 Some、域外 None 宁缺勿猜——render 消费面 `glyph None → BLANK 全 bg` 对偶）+ 结构钉板测试（**字面 'L'/'I' 位序钉板** = 对抗审查 F2——期望位图独立手算不与生成器同源重算、基线 row 12 锚、下行部 g/j/p/q/y 伸 rows 13..=14、x 高带、宽度谱系 M≥6 列/I≤3 列、字母数字密度下界）。**render 半边**（workflow agent 一次成 + 主会话补测）：`PixelFormat{Rgbx,Bgrx}` + `from_raw` 界检（BitMask/BltOnly→None 宁缺勿猜）+ `Fbcon::new` 拒面（w<8/h<16/stride<w×4/fb 容量 checked 乘加）+ `put_char/put_str`（'\n' 越底自动滚/行尾回绕）+ `scroll_up`（`core::ptr::copy` memmove——`copy_from_slice` 自重叠 UB，P6 A-7 立法）+ 滚屏露出行 bg 记账 = 最近写入面 bg + 多屏长串压测 = 末屏收敛逐字节对拍。**workflow 编排实录（对抗审查 F1-F3 闭环）**：render 半边 agent 一次成 15 测；font 半边 **两次死于 32K 输出上限**（首死 = 单响应巨型 Write；次死 = 分块写纪律下仍死于绘制设计的长思考输出——字形批量设计+输出对子 agent 是结构性超限面，分块纪律只管工具调用管不住思考面）→ 主会话亲自接管改走 art→生成器路线（画稿可目验 + 生成器钉结构 + 测试抓真雷：首跑三败全真雷——M 定位笔误 c7 误墨、密度下界误圈标点（句号 1 墨点合法稀疏——修法收窄到字母数字域）、下行对照断言切片长度 3≠2）。验证 = 真仓全量 28/28（显式 `--target <host>` + 私有 CARGO_TARGET_DIR——根 config 锁 aarch64 跨 workspace 边界 gotcha）+ ASCII 渲染 8 形抽目验（A/M/@/g 等）。F1（font 缺席）随落地闭环；F2/F3 如上。**刀 2 后段挂账不变且已解阻塞**（T-1 提交 bf46090 已落）：fb MMIO 窗 VMO 授能 + fbcon 服务接入 user 工作区注册 + smoke 标记。
- **V-6 刀 2 后段（GUI-1 消化）已收官 2026-10-29，V-6 全线收案**：两段两提交——**段①（e7f2397）fb 授能 + guifbd 自验**：EFI-GOP 命中分支钉存五域 → fb 窗 `Vmo::create_mmio`（469 页）插 svcmgr 表第 5 插 H_FB + 几何五域 40B 匿名 VMO 第 6 插 H_FBINFO（`FB vmo=0x10005 pages=469` 打点；-kernel 路零插表零扰动）；`user/bins/guifbd`（musl std + berry-fbcon，vblk 形态）gift2 读 info → `Fbcon::new` 自验三步（clear 双相位差分/字形蜂格逐像素对拍/scroll 行移）→ `FBW ok w=800 h=600 cells=100x37`（vblk println 同路）；berry-fbcon 随迁正 user/ members（host 28）；`SpawnSpec` +mmio_va 参数化（`FB_VA=0x7100_0000` 单源）；**采样面收敛蜂格区**（sliver 铁律首跑实踩：@1,598 底 sliver 固件残留误炸——右 w%8/底 h%16 不属 console 一律不触碰）。**段② console fan-out 双路上屏**：boot 建 fb 管道（`ChannelEnd` 对，console 表第 5 插 endK_fb=0x0001_0005 持 SEND、peer 第 7 插 0x0001_0007 落 svcmgr 表经 env extras[1] take 迁 guifbd 表 = libc `gift3_h()`）；console drain 循环整段 best-effort 转发（n≤256B 内联额度内一发；满 -EBUSY/-EAGAIN → 丢弃 + `fbdrop=` 计数打点不重试不阻塞保串口主路；零长 EOF 不转发）；guifbd serve 循环 fsmgr 形态（recv → EAGAIN → wait_one(READABLE,+10s) 重臂）put_str 上屏，首条到达 `FBSERVE ok`（稳态矩阵打印持续不断 → 首条必达 = 状态谓词非竞速）；通道 opt-out 不借 intent；`FBW ok`/`FBSERVE ok` 入 smoke-image ECO canon（21→23）。**段② 排障实录（2026-10-29，结论 = 无 bug）**：首跑目验疑「屏面冻结/龟速 0.6-1 字/s」——lldb 断点四步实证链全部翻案：①`wake_matching(endB_fb)` 首轮 send 即唤醒沉睡者（后续 send 命中时已 Ready，n=0 合法）；②消费是突发非滴流（一轮拿片连收 8 条/0.1s，RQ 6 任务纯 FIFO guifbd 环尾 ≤1.2s 必达）；③节奏连续 ≈32 字/s（10s wait 帽是重臂超时非消费周期——数值巧合强化了误诊）；④无干扰快照 20s 内 37/37 行像素哈希全变 = 全速滚动；⑤终局读屏——用 berry-fbcon 自家字体表把帧缓冲像素反解回文本，满屏稳态矩阵巨行与串口逐字同源同序（95 字形全精确匹配）。「冻结」实为 boot 头几分钟 ECO/selftest 输出恰 ~15 行的正常中间态 + 未实测的速率推导连锁误读；方法论教训（时间尺度错觉/单点目验锚定/重读原始数据翻案）入 `docs/阶段沿革.md` 追记。验证：smoke-image PASS（62/62 = EFI 十段 + 接续 + 裸相 26 + 生态 23 + kfile/init 字节对拍 + 金丝雀严格递增 + RX 注入复验）+ 主 smoke 五跑零漂移 + test-host 410/0。

### V-7 USB HID 键盘（xHCI 最小，或降级线）
- 最小面：单设备（键盘）、根 hub 之外零拓扑、无热插拔、HID boot protocol 轮询（不解析 report descriptor）；xHCI 环管理（DCBAA/cmd ring/event ring）是体量大头——**纯函数 crate 化**（ring 编解码 host 测先行，berry-virtio 先例）。
- **降级线（V-5a 事实表裁定）**：若 Fusion 暴露串口 → 键盘先走串口终端（M-G2 验收线注明的允许降级），xHCI 顺延独立刀；若 USB 非必要路径存在其它输入面同理。

### V-8 Fusion 手动验收
- 清单式：建虚机（ARM/Linux 无人值守模板改引导盘）→ 挂 berryos.img → 开机到提示符 → 键盘 `ls` 出目录 → Ctrl-C 杀 sleep → 截图留档（`docs/` 附验收记录）。**无 CI、不阻塞收官判定之外的任何刀**。

## 5. 风险与对策

| # | 风险 | 对策 |
|---|------|------|
| R1 | **BerryBoot 是首个引导器**（UEFI 协议面 + ExitBootServices 时序 + PE 链接全首踩） | V-0 拆两小步（骨架先点亮再装载）；AAVMF 即 QEMU 可自动化，不等 Fusion；研发循环保留 `-kernel` 快路（V-1 三源归一） |
| R2 | FAT16/路径化**破既有 overlay 隔离或 smoke 漂移** | FATW/持久化相零漂移 = C-0/V-2 硬回归线；fixture 守卫同步扩；overlay「RAM 自由名/FAT 8.3」构造隔离不动 |
| R3 | **Fusion 设备栈体量未知**（GICv3/NVMe/xHCI 任一都可能超预算） | V-5a 实证先行、事实表定盒；驱动件逐个独立刀、kill criteria 见 §6 |
| R4 | xHCI 是最重单件（event ring/传输环/trb 编码） | 键盘降级线预置（串口兜底，V-5a 裁定）；xHCI 纯函数 crate 化先 host 测 |
| R5 | Ctrl-C 需 kill 面 = **ABI 扩号**（0x3B） | 走 abi.toml 单源管线一次一号（G-eco 扩号先例：meta 计数 + 三面生成 + `--check` 守卫）；语义挂 task_exit_hook 既有退出路径，不开第二退出面 |
| R6 | C-1 fd 表/路径解析改 libc 面，**stdhello/hexyl 零回归**是底线 | fd 0/1/2 路径一字不动；新增面独立标记自验；hexyl 基准比对照跑（`HEXYL ok` 不许缺） |

## 6. kill criteria 与 SMP 慢推

**Station-2 冻结线（自动触发）**：Fusion 线任一驱动件（V-5b/V-6/V-7）**连续两提交周期占用 >50% 主线工时** → 该件冻结顺延（窗口只读 demo / 串口键入降级线顶上），**M-G1/M-G3 不受拖累**——UTM 站交付不等 Fusion。触发即回写本表 + 路线图，同 x86 #41 kill criteria 纪律。

**V-0 兜底**：若 AAVMF 固件行为在两个提交周期内无法稳定（QEMU 侧引导不可复现），降级 = UTM 直接验收 + QEMU 研发循环维持 `-kernel`（M-G1 验收线仍以「挂盘开机」为准，自动化面缩窄为镜像内容断言）。

**SMP v2 并行慢推（ADR #49 同步裁决）**：M-VM 期间 SMP 不断线——**每开发周期 ≤1 刀**（一周期 ≈ 一功能刀 + 对抗审查 + 回写），下一刀 = **S-1 per-CPU 基建**（`SMP设计草案-v1.0.md`，S-0 副核启动通道已实证 2026-10-22）；SMP 刀与 M-VM 刀不混树提交（多会话 git 纪律）；S-5/G2 不因本里程碑提前。

## 7. 与既有文档/资产的挂钩

- `开发体验.md`：§1 四级阶梯——本里程碑 = **L3 兑现**（`just image`/`just dev` 命令演进表随刀位落）；§5 三级火箭第二级（交互 shell + FS）的体验面补完。
- `路线图-v1.1.md`：本里程碑横跨 P5（shell 收尾：Tab/Ctrl-C）+ P6（驱动延伸：Fusion 设备）+ 引导加载器（P0 留位终于开工）；**Phase 8 真机不动**。
- `shell设计-v1.0.md` §2.4：Tab 补全 v2 兑现（快照近似版）；`§2.2 channel 即 pipe` 已兑现面照旧。
- `驱动模型设计-v1.0.md`（ADR #46）：fbcon/NVMe/xHCI 全按「驱动=持证应用」走 svcmgr 拉起链（devmgr v2 未到——boot 手授 MMIO 窗 = A-2 既有模式，devmgr 到位后迁正）。
- `存储布局与沙箱设计-v1.0.md`（ADR #45）：镜像分区表（MBR/FAT16）是**引导实现细节**，不升用户概念（四棵树是系统盘布局语义，v1 盘仍是单卷 FAT——重议触发器既有「分区永远是实现细节」条款正引用于此）。
- `系统调用ABI设计-v1.0.md` + `tools/abi.toml`：唯一扩号 = 0x3B process_terminate（R5）；单源管线全流程（meta → gen → check → ktest/berry-sys 同步）。
- `应用模型设计-v1.0.md`（ADR #43/#44）：uutils bin = 普通程序进 rootfs cpio（不打包 .bap——.bap 是 GUI/app 面的 v0 演练，CLI bin 走 bin/ 直载既有链）；fbcon/vnet/vblk 同 = 系统服务非 app。
- `SMP设计草案-v1.0.md`：慢推节奏挂钩（§6）。
- `测试策略-v1.0.md`：smoke-image 独立关卡（x86 先例）；C-5 演示相并入主 smoke。

## 8. 刀序（建议执行序，非锁死）

```
C-0 → C-1 → V-0a → V-0b → C-2a → V-1 → V-2 → V-3/V-4（= M-G1 ✅ 落，唯余 UTM 手动点验）
  → C-2b → C-2c（✅ 中批 6 + 容量一体）→ 刀 2（✅ df+statfs 三步，du 挂账裁出）→ 挂账刀（✅ 深坑 6 + head·sort·du 执行面 + bsh fs_gifts 三 spawn 点）→ C-3（✅ ps + berryctl）→ V-5a（实证刀，可与 C 线并行）→ V-5b/V-6 → C-4（✅ Tab 补全 + C-4b Ctrl-C）→ C-5（✅ M3DEMO 五步 = M-G3 大体成）
  → V-7（或降级线）→ V-8（= M-G2 收官）
```

- **B 线先行**：C-0/C-1/C-2a 无引导依赖、每刀独立标记验收——最早给用户「命令行变强」的实感。
- **V-5a 随时可插**：实证刀产出事实表，不阻塞也不被阻塞。
- 工期量级（单人 + AI 协作，TCG 可自动化部分）：B 线 ~3–4 周期，A1 线 ~3–4 周期，A2 线视 V-5a 事实表 2–6 周期——**全程 ~6–10 周期**；kill criteria 任一触发即压缩 Station-2 面保 M-G1/M-G3。
- 每刀照旧：设计对齐 → 小步提交 → smoke 全绿零漂移 → 对抗审查 → CLAUDE/AGENTS 回写。

## 附录 A：UTM 手动验收清单（M-G1）

1. UTM 新建虚拟机 → Virtualize → Linux → ARM64 (aarch64) → 不选 ISO，「跳过」安装。
2. 驱动器挂 `berryos.img`；系统固件 = UEFI（默认）；串口留空（M-G1 console 走窗口前先用串口日志口，或直接看 V-6 前的串口终端面）。
3. 开机 → 串口/控制台出 `BerryOS boot` → `BERRYBOOT ok` → 内核标记流 → `bsh>` 提示符。
4. 键入 `rx`（回显验证）→ `ls` → 截图留档 `docs/验收/M-G1.png`（目录随手点验时创建）。

## 附录 B：Fusion 设备事实表（V-5a 回填）

| 项 | Fusion 事实 | AAVMF 参照（V-5a-1 段① 实测 2026-10-28） | 影响 |
|----|------|------|------|
| CPU 拓扑 / GIC 版本 | 待实证 | `cpu=1/1`、GICv2（gicd@0x8000000 / gicc@0x8010000 / gicr=无条目） | GICv3 → HAL 增量刀 |
| 盘控制器 | 待实证 | PCI 枚举无盘控制器（usb-storage 走 XHCI；`hcs: blk=1` = USB 盘 BlockIo） | NVMe → 新驱动刀；virtio → 复用 vblk |
| 串口（PL011/16550） | 待实证 | PL011@0x9000000、GSI=33、baud=3（SPCR+DBG2 双表同基址交叉验证 ✓） | 有 → 键盘降级线成立 |
| USB 控制器 | 待实证 | qemu-xhci `1b36:000d` @0/2/0（c=0c0330） | xHCI → V-7 范围 |
| GOP（分辨率/格式） | 待实证 | 800x600 fmt=1 m=1/3（640x480/800x600/1024x768 三模式） | fbcon 几何 |
| 硬件发现机制（ACPI/DT） | 待实证 | ACPI 2.0（acpi20=1 dtb=0；XSDT 九表 FACP/APIC/PPTT/GTDT/MCFG/SPCR/DBG2/IORT/BGRT） | devmgr v2 输入面；v0 可全走 UEFI handoff |
| （旁证）平台 virtio | 待实证 | virtio-net `1af4:1000` @0/1/0（QEMU 默认 NIC——非显式配置即在位） | Fusion 若有 virtio-blk → 直接复用 vblk |

> V-5a-0 起回填的行式映射（berrydump 输出 → 本表行）：`madt:` → CPU/GIC 行（`cpu=e/t` + `gicd=3(GICv3)` + gicd@/gicr@ 基址）；`pci:` → 盘控制器/USB/网卡行（class 三元：0108xx=NVMe、0C0330=XHCI、0200xx=ETH——**USB 事实主源**）；`spcr:`/`dbg2:` → 串口行（iface=3=PL011 + base，两表交叉验证）；`gop:`/`gop[m]:` → GOP 行；`cfg:`+`fdt:` → 发现机制行（acpi20/dtb 计数 + FDT magic 校验）；`hcs:` blk/net 计数 = 旁证。主源 = berrydump 实机输出；对拍 = Linux live `lspci`（方法原案）。
>
> **段③ 对拍命令清单（2026-10-28 入册——Linux live（ARM64 ISO）与段② 同一次 Fusion 会话顺手收，每条 ↔ 一行 berrydump 输出）**：`lspci -nn`（vendor:device + class ↔ `pci[i]:` loc=BDF/class 逐条对拍）、`lspci -t`（桥接拓扑）、`dmesg | grep -iE "gic|its"`（↔ `madt:` gicd= 行）、`nproc`（↔ `cpu=e/t`）、`ls /sys/firmware/acpi/tables/ /sys/firmware/devicetree 2>/dev/null`（↔ `cfg:`+`fdt:` 发现机制）、`dmesg | grep -iE "pl011|16550|serial"`（↔ `spcr:`/`dbg2:` 串口）。
>
> **段② 前置预检（2026-10-28，berrydump 走查帽审计——结论：零改动收案）**：各帽对 Fusion 设备量充足且**溢出恒 loud**——`PCI_PRINT_CAP=16`（Fusion ARM64 虚机 PciIo 句柄典型 5–10，超帽打 `pci: more=N`）、`GOP_MODE_CAP=8`（`gop: more=`）、`MAX_XSDT_ENTRIES=16`（`xsdt: n=…+` 截断标记）、`CFG_WALK_CAP=32`（真实平台 ≤10）——单次 Fusion 会话不会因帽静默丢事实；more/truncated 标记行是「无静默帽」纪律执法点，后续改码勿抹。
