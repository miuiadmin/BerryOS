# 内核深度进化 23 号：第八连 E0425 隔离树实装复现——PXN/UXN 供体复活编译绿定谳与 protect 臂掩码缺陷预警（23 号件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：22 号件升级第八连预警（vmar.rs 段 3 手工取 overhaul 形 = 人为引入 `hal::PXN/UXN` 消费，而 paging.rs 供体不在冲突清单、git 自动取 base 删态）——本件把该预言在隔离 worktree 实装复现并全消：段 3 Perm 整取（Q106 四档 `ap()` 换 F0-2 六档 `pte_bits()` + 三消费点改接）在供体全无的 base 态 = **E0425 ×10**（`hal::PXN` 6 + `hal::UXN` 4）；供体双触点（paging.rs 两常量 + lib.rs re-export）贴回后 `cargo build -p berry-kernel` **0 error**、lockdep 双形绿——第八连从预言升为实测定谳；并带出两个新发现：**供体双触点独立必要性**（常量在、re-export 缺 = E0432 先红，re-export 层独立构成编译门）与 **protect 臂掩码缺陷预警**（base 现行 `AP_MASK` 两态掩码在 pte_bits 整取下升权残 UXN = 永不可执行，第 1 步施工须随刀换 overhaul `PERM_MASK` 形——22 号件调和面清单补此面）。

## 审计基线（2026-11-20 06:5x 直核）

- base 线 = `origin/wip/sched-1.1@bb03d412b`（22 号件本体笔即 base tip）；fceb = `fceb03ff3`；overhaul = `origin/wip/base-overhaul@492d5e00c`（未前进）。
- 隔离验证树 = git worktree（detach bb03d412b，`~/.hermes/cache/scratch/w3-23`），验证后拆除不污染主工作树；userbins 两 .bin 从主树拷入（gitignore 面，非 diff 内容，21 号件先例）。
- 隔离 target = 树内 `.target-none-w3q23`（`RUSTC_WRAPPER=""` 绕 sccache EPERM，20 号件先例）；四组对照均两轮以上可复现。

## 主发现一：第八连实装复现——段 3 整取 + 供体全无 = E0425 ×10

- **施工形**（模拟第 1 步段 3 手工解）：base `kernel/src/vmar.rs` Perm 区（doc + enum + `impl Perm { fn ap() }`，551 字符）整块换 overhaul 形（doc + 六档 enum + `From<u8>`/`TryFrom<u8>` + `pte_bits()` + `allows_exec()`，1616 字符）；三个 `.ap()` 消费点换单源 `pte_bits()`（eager 写叶 :420 / demand 登记臂 :523 / protect 写叶 :621）。
- **对照红**（供体全无 = base 原态）：`cargo check -p berry-kernel` EXIT=101，**E0425 ×10**——`cannot find value PXN in crate hal` ×6（pte_bits 六档 arm 各一）+ `UXN` ×4（Rw/Ro/UserRo/UserRw 三 arm + …合计四）——22 号件第八连预言逐字兑现，且红源在冲突段手工解时埋下（前七连均为消费者自动并区带回）。
- **供体复活后全消**：paging.rs `AP_USER_RO` 后插 fceb 形两常量（`pub const PXN: u64 = 1 << 53;` / `pub const UXN: u64 = 1 << 54;`）+ lib.rs `pub use paging::{… PXN, SH_INNER, SH_NONE, UXN}` 换 fceb 形——`cargo build -p berry-kernel`（aarch64-unknown-none 真链）EXIT=0，`cargo check` 默认 + `--features lockdep` 双形 0 error；26 警告全为 base 原树既有（static_mut_refs/unused 面，对照核过）。
- **判据升级（第八连实施化定稿）**：第 1 步段 3 施工序 = ①先贴 paging.rs 两常量 + lib.rs re-export（供体先行）→ ②再贴 vmar.rs Perm 区整取 → ③三消费点 `.ap()`→`.pte_bits()` 改接——三刀一批，任何一序颠倒即在中间态卡 E0425/E0432。

## 主发现二：供体双触点独立必要性——re-export 层独立构成编译门

- 对照实验：paging.rs 常量在、lib.rs re-export 剥除（回 base `SH_INNER, SH_NONE` 形）→ `cargo check` EXIT=101，**E0432** `unresolved imports paging::PXN, paging::UXN`（arch/aarch64/src/lib.rs:206）——红在 hal crate 自身编译期，先于 kernel 侧 E0425。
- 分档定谳：第五面供体 = **两触点一债务**（常量定义 + crate 导出面），任一缺席即红；21 号件 F0-2 复活五 re-export 面（active_cpu_asids 等）同构——「供体先行」判据的粒度须到 symbol 的**导出链**（定义 + re-export），非仅定义。
- 首贴时的形态坑：E0432 报在 re-export 行而常量已贴的场景仅当常量贴错文件/版本时出现；正确施工形下 E0432 = re-export 在而常量缺、E0425 = 常量链全在而 kernel 消费先贴——两红形态可互为诊断指纹（快速定位缺哪层）。

## 主发现三：protect 臂掩码缺陷预警——第 1 步施工新增调和面 E

- **缺陷机理**：base 现行 protect 写叶形 `(e & !AP_MASK) | perm.ap()`（vmar.rs :621，`AP_MASK = 0b11 << 6` 两态）；段 3 整取后该点换单源 `pte_bits()`，但掩码若不随刀升级，升权形（如 UserRw→UserRx）旧叶 UXN 位（bit 54）残留在 `e` 里清不掉——**数学演示**：`e = AP_USER_RW|PXN|UXN`（map 时 UserRw 写入）protect 成 UserRx 后叶 = `0x600000000000c0`，UXN 残留 = 用户代码页永不可执行（静默错，无编译红无 panic）。
- **overhaul 正确形参照**：aspace.rs `PERM_MASK: u64 = (0b11 << 6) | hal::PXN | hal::UXN`（:43 定义、:582 protect 写叶消费）——掩码与 pte_bits 权限域同宽。
- **施工义务登记**：22 号件十段处置表段 9（protect 主体）须补调和面 E：写叶掩码 `AP_MASK`→`PERM_MASK`（含 `hal::PXN | hal::UXN`），与段 3 同批随刀；demand 登记臂 :523（`RegionKind::Demand(p) => p.pte_bits()`）与 supply 写叶（`page_descriptor | ap` 直拼形）无残留面（新写叶不含旧位），零调和。
- **判普**：凡「单源权限位合成函数」整取（ap()→pte_bits()），所有 RMW 写叶点（read-modify-write 形）的掩码宽度必须与合成域同宽——eager 写叶（construct-from-zero 形）与 demand 登记臂无此约束；掩码滞后 = 升权残位静默错，比编译红更危险。

## 主发现四：AP 面恒等快验与 6 档扩容冲击核

- **AP 恒等**：pte_bits 输出剥 NX 位后与 base ap() 逐档恒等（Rw=0/Ro=0x80/UserRw=0x40/UserRo=0xc0）——22 号件调和面 C（判别值换位零 ABI 面）复确认：全树 `Perm::` 消费 28 处全为构造点（syscall 4/dpg 2/aspace 1/ktest 15/user 3/vdso 2/sched 注释 1），无 `TryFrom<u8>`/`as u8` 序列化消费（全树 grep 实证），判别值换位不触任何消费面。
- **穷尽 match 冲击**：base 侧无 `match perm` 穷尽臂（全树核）——六档扩容零穷尽破坏；`TryFrom<u8>` 的 Err 形取 `VmarError::NotMapped`（ovh 形）为纯新增面，base 零消费者（预留 ABI 演进，UserRx=4/UserRwxCompat=5 判别值即 ovh 冻结值）。
- **allows_exec 消费登记**：base 侧零消费（纯 ovh 新增）；overhaul 侧消费 = aspace.rs protect 的 MMIO 拒执行门（`is_mmio && allows_exec` 拒 NotMapped）+ PoU 同步门（`!is_mmio && allows_exec` 触发 sync_icache_pou）——两门随第 1 步 aspace.rs 整取（13 号已裁两段）自动承接，本件不预放（Q25 REJECT 教训）。

## 复活施工卡（第 1 步段 3 实施化定稿，三刀一批）

- **刀① 供体先行**：paging.rs `AP_USER_RO` 后插两常量（fceb 形逐字）+ lib.rs re-export 换 fceb 形——E0432/E0425 双红形态的解层。
- **刀② Perm 区整取**：base 551 字符块换 overhaul 1616 字符块（doc + 六档 + From/TryFrom + pte_bits + allows_exec）。
- **刀③ 消费点改接**：三 `.ap()` → `.pte_bits()`（eager :420 / demand :523 / protect :621）+ **protect 写叶掩码 AP_MASK→PERM_MASK**（调和面 E，本件新增）。
- 验证口径：`cargo build -p berry-kernel` 0 error（真门）；对照红（供体剥除）可复现；台账面——pte_bits/perm.as u8 均 const fn 无 unsafe，台账零增量预期（第 1 步全量重产时核对）。

## 与其他文档的关系

- 预警源：[22-vmar.rs十段整取第1步实施化diff草案-事故族第五面PXNUXN同点回退定谳.md](22-vmar.rs十段整取第1步实施化diff草案-事故族第五面PXNUXN同点回退定谳.md)——第八连预警的实施化复现件，其十段处置表段 3/段 9 补调和面 E（protect 掩码）。
- 实装链：[19-…第六连](19-第六连E0425合并树实证-供体复活随刀序实施化与vmar六调用点表述修正.md)（随刀序）→ [20-…①②](20-kernLock双guard与mm三锁助手复活实装验证-第1步随刀序12实跑定谳.md)（①②）→ [21-…③](21-F0-1F0-2供体复活第0步实装验证-隔离树编译四连红全消定谳.md)（③第 0 步）→ 22 号（④草案）→ 本件（段 3 实装验证 + 调和面 E 新增）。
- 事故族判例谱：15 号（第二面）→ 17 号（第三面）→ 18 号（第四面）→ 22 号（第五面 PXN/UXN）——本件为第五面供体的实装验证件。
- 证据可复现：隔离树 = `git worktree add --detach ~/.hermes/cache/scratch/w3-23 bb03d412b`；对照红 = 段 3 贴入 + 供体全剥 → `cargo check -p berry-kernel`（E0425 ×10 / E0432 形）；绿 = 供体贴回 → `cargo build` EXIT=0；掩码演示 = `(AP_USER_RW|PXN|UXN) & ~AP_MASK | (AP_USER_RO|PXN)` = `0x600000000000c0`（UXN 残留）；PERM_MASK 正确形 = overhaul aspace.rs :43。
- 沿革：`docs/阶段沿革.md` 六百三十一节（随本批；原编六百三十让号 W2-Q240 随迁件）。
