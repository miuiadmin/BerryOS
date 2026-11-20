# 内核深度进化 24 号：aspace.rs 两段冲突处置与消费改接面 diff 草案——第九连 E0425 供体链生死分叉定谳（24 号件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：22 号件完成 vmar.rs 十段处置表、23 号件实装复现第八连（PXN/UXN）后，本件把回流第 1 步剩余的 kernel/src/aspace.rs 两段冲突（merge-file 实测 @:895 `alloc_asid` 双形 / @:930 `try_alloc_asid`+`free_asid` 尾段）与其消费改接面升为逐段 diff 草案，并带出**事故族第九连**：合并树（`fd07f8d2a`）解剖实证——aspace.rs 自动并区消费面全活（`sync_icache_pou` ×5、`active_cpu_asids` ×1、`berry_vmarface` use 区）而三层供体链两死一活：① `active_cpu_asids`（hal F0-2 面）与 mm `AsidAllocator::try_alloc`/`AsidState`（分代位图）自动取 base 回退形（arch lib.rs re-export = 0 / mmu.rs 定义 = 0 / mm asid.rs 无 try_alloc）——手动取 overhaul 形即 E0425/E0432 双红；② `vmarface` crate 是 **overhaul 纯新增件**（base/fceb 双侧零目录、非回退面），合并树自动整套带入（目录+members+kernel dep）——**首例供体链自动存活**；③ syscall 取号窗自动取 base v1 形（`alloc_asid()` 嵌 `new_root` 内联 expect 形），overhaul `try_alloc_asid`/`new_root_global`/`free_asid` 归还臂全被冲突段剥除——ASID 泄漏面回流施工单缺此刀即每销毁进程漏一号（21 号第 7 点配套义务在此汇合）。

## 审计基线（2026-11-20 07:3x 直核）

- base 线 = `origin/wip/sched-1.1@ed2a8b37b`（#586 合并笔即 base tip）；fceb = `fceb03ff3`；overhaul = `origin/wip/base-overhaul@492d5e00c`（未前进）。
- merge-tree 实跑：`git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/sched-1.1 origin/wip/base-overhaul` → 树 `fd07f8d2a`（exit=1，十文件冲突与 22 号基线零漂）。
- aspace.rs 两段段位 = `git merge-file -p`（ours 137 行 / base 158 行 / theirs 1004 行，exit=2）——冲突恰 2 段：`@:895` 与 `@:930`，与 13 号「aspace.rs 2 段取 overhaul」裁定吻合，base 前进零新增段。
- 消费面计数基线：合并树 aspace.rs `sync_icache_pou` ×5 / `active_cpu_asids` ×1 / `berry_vmarface` use ×1；ktest aspace_ptr ×16 / new_root_global ×2 / test_f1 ×4（= ovh 逐项相等，base 全零）。

## 主发现一：两段冲突解剖——ASID 分配面双形（16 号恒等区结论在合并语境下翻转）

- **段位 @:895（alloc_asid 头段）**：ours = base v1 `alloc_asid()` 计数轮转形（S-4 4-3 长注释 + `alloc()` 无 pinned）；theirs = overhaul `try_alloc_asid() -> Option<u16>` 防御形（`active_cpu_asids()` 钉死探测 + `try_alloc(&pinned)`）。共用尾区（惰性初始化 + 锁内 roll_gen_bump + G3-3 lockdep 放点）自动并区。
- **段位 @:930（尾段）**：ours = 锁外 `rolled` 即 `roll_quiesce_arm()` 武装；theirs = `picked` Option 折返 + `alloc_asid()` 便捷入口（expect 形）+ **`free_asid` 整函数**（0 短路 + 锁窗 `alloc.free(asid)`）。
- **处置**：两段全取 overhaul（13 号裁定）——`try_alloc_asid` 避开「硬件可用 ASID 全部钉死」时的 expect panic，是 base 形的防御升级；base 的锁外 arm 语义 overhaul 同款保留（16 号逐 token 对拍实证）。
- **16 号结论的语境修正**：16 号定谳「ASID 四函数 = 自动合并恒等区，回流零手术」的基线是 fceb vs overhaul（两形同源）；合并语境下 base 侧是**v1 回退形**（fceb 的 F0-2 形被 7f05be12e 剥除，15 号定谳），两段才成冲突。恒等区结论对 fceb 基线成立，对 base 合并语境不成立——回流施工以本件两段处置为准，16 号的「S-4 4-3 契约面不变」结论继续有效（两形武装序同构）。
- **消费改接面（本段配套刀）**：base 侧 `alloc_asid` 消费者三处——dpg.rs `:1084`/`:1117`（boot 自验 demand root，可信语境 expect 形**可保留** alloc_asid）、ktest.rs `:447`/`:561`/`:580`/`:588` 等（测试语境同）、syscall.rs `:1769`（**EL0 面须改接** try_alloc_asid → 耗尽折 `-ENOMEM` + new_root 失败臂 `free_asid` 归还，08 号段 4 与 21 号刀⑦在此汇合）。

## 主发现二：事故族第九连——aspace.rs 整取的供体链生死分叉（第八连判据的树级扩展）

merge-tree 树 `fd07f8d2a` 解剖（第九连证据链）：

| 供体链 | 合并树形态 | 生死 | 红 |
|---|---|---|---|
| hal `active_cpu_asids`（F0-2 re-export） | arch lib.rs = 0、mmu.rs 定义 = 0（自动取 base 删态） | 死 | E0425（try_alloc_asid 消费） |
| mm `AsidAllocator::try_alloc`/`AsidState` 分代位图 | mm/asid.rs 无 try_alloc（自动取 base v1 形 `alloc()` only） | 死 | E0425（`try_alloc(&pinned)` 方法不存在） |
| mm/Cargo.toml `berry-asidface` dep | = 0（自动取 base 侧） | 死 | E0432（asid.rs `use berry_asidface::AsidState` 无解析） |
| `vmarface` crate 整套 | 目录 + 根 members + kernel/Cargo.toml dep 全自动带入（= ovh） | **活** | 无 |
| syscall 取号窗 | base v1 形（`alloc_asid()` 内联 new_root） | base 形 | ASID 泄漏（静默错） |

- **第九连定谳**：aspace.rs 两段手动取 overhaul 后，`try_alloc_asid` 消费 `hal::active_cpu_asids`（hal 层定义 + re-export 两触点）与 `AsidAllocator::try_alloc`（asidface crate + mm dep + asid.rs 分代形三触点）——供体全在「第 0 步」刀序（21 号件七刀）里，**若第 0 步先落、第 1 步 aspace 整取随刀，无红**；若跳过第 0 步直接整取即 E0425×2 + E0432×1。与第八连（vmar 段 3 引入 PXN/UXN 消费）同构，但供体扇入更宽（hal+mm+asidface 三层），且 mm/asid.rs 自动取回退形是**新形态**——前八连均为「消费者活供体死」，第九连含「供体半死」（同文件内新旧两形，git 自动并取旧形）。
- **vmarface 自动存活定谳（首例）**：base 与 fceb 双侧零 `vmarface/` 目录（四问核 0/0/1）——它不是回退面，是 overhaul 新增件；git 三方合并对 base 侧无演进的新增路径**整套自动带入**（目录、根 members `:36`、kernel dep `:28`）——事故族九连中首例供体链自动存活，与第 0 步三供体（active_cpu_asids/AsidState/memface 本体）的「自动取死」形成分叉判据：**新增件（base 无演进）自动活，回退件（base 有演进或删态）自动死**——供体先行原则的适用面判据。
- **syscall 取号窗静默错（无编译红）**：合并树 syscall `:1814` 自动取 base v1 形——`alloc_asid()` 在 new_root 内联（耗尽即 panic）且无 free_asid 归还臂；aspace.rs 整取带回 free_asid 函数后 syscall 侧不接 = 每销毁进程漏一号（255 号 8 位宽长跑必耗尽）。与 23 号调和面 E（掩码残位静默错）同档：**无编译红的静默错比 E0425 更危险**，施工单必须显式列刀。

## 主发现三：aspace.rs 整文件其余面 = 零手术区（自动并区恒等）

- 除两段外，合并树 aspace.rs 与 overhaul 逐行差 = 两段本体（`diff` 剥标记后仅段区差异）——F1-1/F1-2 架构面（`SpinLock`/`AddressSpace`/`teardown` 两阶段退出/`PtGuard` 区/`PERM_MASK`/`MiniAspace` Box 形）全部自动并区带入，零手工调和。
- `MiniAspace` 双线同构（base `:98` 裸 vmar+vmo / ovh `:965` Box 包装 `_vmo` 防悬空）：合并树取 ovh Box 形，消费者 ktest `:3385`/main.rs ×4 自动兼容（`new/activate/read_va/write_va` 签名恒等）。
- `USER_VA` 常量双线同值（`0x1000`），base 消费者 ktest ×11 / main.rs ×2 零改接。

## 复活施工卡（第 1 步 aspace.rs 面实施化，六刀一批）

- **刀① 两段取 overhaul**：`@:895` 换 try_alloc_asid 头段 + `@:930` 换 Option 折返/alloc_asid 便捷入口/free_asid 整函数（16 号 token 对拍背书零语义漂移）。
- **刀② 供体链三触点核活**（第 0 步已落则跳过）：arch lib.rs re-export + mmu.rs `active_cpu_asids` 定义；mm/asid.rs 分代形（asid.rs 整取）+ mm/Cargo.toml asidface dep；asidface crate（base 已在树，members 未登——21 号刀①）。
- **刀③ syscall 取号窗改接**：`alloc_asid()` 内联形 → `try_alloc_asid()` else 返 `-ENOMEM` + `new_root_global` 失败臂 `free_asid(new_asid)` 归还（08 号段 4 + 21 号刀⑦汇合点）。
- **刀④ dpg/ktest 消费保留判**：dpg `:1084`/`:1117`、ktest `:447`/`:561`+ boot/测试可信语境 `alloc_asid()` expect 形**保留不改**（ovh 同形实证——ovh dpg `:1083` 亦用 `alloc_asid()`）；仅 EL0 面（syscall）改接。
- **刀⑤ vmarface 零刀**：自动带入（首例），仅核 members/dep 行在合并树（`Cargo.toml:36`/`kernel/Cargo.toml:28`）。
- **刀⑥ 台账/docs 随刀**：aspace.rs unsafe 面（`&raw mut ASIDS` 系）行号随段区漂移重产；沿革让号。
- 验证口径：`cargo build -p berry-kernel` 0 error（真门）+ `--features lockdep` 双形；对照红 = 刀②剥除复现 E0425/E0432；ASID 泄漏面 = ktest 生灭循环后 `active_count()` 归零断言（ovh ktest `:3446` 同款）。

## 与其他文档的关系

- 施工单源：[13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md](13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md)——「aspace.rs 2 段取 overhaul」裁定的实施化，本件供段位/消费改接/供体链三面。
- 恒等区修正：[16-try_alloc_asid两形语义对拍-ASID恒等区与destroy路径语义差定谳-Q110游标面消亡清单件.md](16-try_alloc_asid两形语义对拍-ASID恒等区与destroy路径语义差定谳-Q110游标面消亡清单件.md)——其恒等区结论基线为 fceb vs ovh；本件证合并语境（base v1 回退形）下两段成冲突，语境修正而非推翻（S-4 4-3 契约面结论仍立）。
- 判据链：[22-vmar.rs十段整取第1步实施化diff草案-事故族第五面PXNUXN同点回退定谳.md](22-vmar.rs十段整取第1步实施化diff草案-事故族第五面PXNUXN同点回退定谳.md)（第八连）→ [23-第八连E0425隔离树实装复现-PXNUXN供体复活编译绿定谳与protect臂掩码缺陷预警.md](23-第八连E0425隔离树实装复现-PXNUXN供体复活编译绿定谳与protect臂掩码缺陷预警.md)（第八连实装）→ 本件（第九连：供体链生死分叉 + vmarface 首例自动存活）。
- 第 0 步衔接：[21-F0-1F0-2供体复活第0步实装验证-隔离树编译四连红全消定谳.md](21-F0-1F0-2供体复活第0步实装验证-隔离树编译四连红全消定谳.md)——第九连供体链 = 其刀②④⑤（asid.rs/mmu.rs/lib.rs）；其刀⑦（Vmar::drop 归还）与本件刀③汇合为 syscall 取号窗双接线。
- 证据可复现：merge-tree = `git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/sched-1.1 origin/wip/base-overhaul`（树 `fd07f8d2a`）；两段段位 = `git show {base,fceb,ovh}:kernel/src/aspace.rs` 三方 `git merge-file -p`（`<<<<<<<` ×2）；供体生死 = `git show fd07f8d2a:{arch/aarch64/src/lib.rs,mm/src/asid.rs,mm/Cargo.toml,Cargo.toml,kernel/Cargo.toml}` grep 计数；vmarface 四问 = `git ls-tree {base,fceb,ovh} --name-only | grep -c '^vmarface$'` = 0/0/1。
- 沿革：`docs/阶段沿革.md` 六百三十二节（随本批）。
