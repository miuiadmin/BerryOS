# ktest 失败续跑终态汇总（moss 形态）否决

Status: rejected —— no_std panic=abort 立法下 panic 不可 catch，moss 三重前提（nightly intrinsic/unwind 构建/handler 续跑）均被 BerryOS 立法排除；软断言限定形态收益子集错位且审计不可规模化，密度需求走 host 外移

## Problem

research/18 §3 可借鉴①：moss `#[ktest]` harness 失败续跑 + 终态汇总，一次 QEMU boot 收全部失败清单，收割密度高于 BerryOS ktest 的首败即 panic 停。该借鉴能否在 BerryOS 落地，需要可行性裁决。

## Proposal

内核内 ktest 改「记失败继续、末尾汇总」（moss 形态）。**否决**，依据三层：

1. **moss 形态依赖三件 BerryOS 已立法排除的东西**（moss `src/testing/mod.rs:80`）：
   - `core::intrinsics::catch_unwind` 是 nightly-only intrinsic（moss 需 `#![cfg_attr(test, feature(core_intrinsics))]` + nightly 工具链）；BerryOS 钉版 stable 1.97.0（`rust-toolchain.toml`，SDK-02「同输入同字节」编译器轴立法）。为测试便利抬 nightly = 动编译器轴，不成比例。
   - catch 的语义前提是 panic=unwind（moss workspace 无 panic=abort，cargo test 构建走 unwind）；BerryOS dev/release 双 profile 显式 `panic = "abort"`（workspace `Cargo.toml`）。abort 下栈不回卷，catch 无从谈起；改 unwind = 裸金属 aarch64 链接 EH 运行时，panic 路径引入全新失败面。
   - moss catch 后回到 runner 循环续跑；BerryOS panic handler `halt()` 是 fail-loud 语义锚（G0 判据「panic 必须有人可读的 dump」+ f4 F9 SMP 声明「halt 只停当核」，`kernel/src/main.rs:108`）——「catch 后继续」与其直接冲突。
2. **软断言限定形态（仅纯函数测试改「记录失败+continue」，全局态保持首败即停）技术上可写，但收益/成本不成立**：
   - 子集错位：可安全续跑的纯函数/静态观测测试（RING2/RING3/INTENT/UTIL/HDR 族）恰是失败密度最低面；高失败密度面（HEAP/FRAME/SYSCALL/VMO/HANDLE——操作全局堆、帧分配器、ASID、demand root 槽池、静态 mut）失败后续跑 = 在破态内核上跑，二次 panic 淹没首因、假绿或挂死。「收全失败清单」的价值主张主体兑现不了。
   - 依赖审计不可规模化：ktest 数百断言中，软断言放行后同函数后续语句可能建立在失败前提上（如 handle=0 继续传 svc），无机械判据，每处需人工审计数据依赖。
   - 语义分裂成本：ktest.rs 是锁序全图首读者高契约载体（六横切锁纪律头注）；`ktest done (N failed)` 的 N 只覆盖软断言子集，防误读 + N>0 仍须 fail-loud panic 才不破主干永绿红线。
3. **等价且更便宜的路径已在**：失败续跑 + 终态汇总是 cargo test 原生语义——`just test-host` 基建已在（mm/src frame/buddy/asid/memmap 均 `#[cfg(test)]`）。纯逻辑测试要密度，正道是外移 host（moss 同款形态：其纯逻辑大头本就在 libkernel 384 `#[test]` host 面，内核内 catch 只是兜底）。且修失败循环成本薄：一次 QEMU boot 秒级，panic handler 已打 file:line:col 精确定位。

## Alternatives considered

- **moss 完整形态（catch_unwind 续跑全部测试）**：否——nightly intrinsic / panic=unwind / handler 续跑三重立法冲突，见 Proposal ①。
- **软断言限定形态（纯函数测试 soft_assert 计数 + run() 尾部汇总行）**：否——收益子集错位 + 依赖审计不可规模化 + 高契约文件双断言语义分裂，见 Proposal ②。
- **panic handler 重定向（打消息后自造栈回卷回 ktest runner）**：否——panic=abort 下无 EH 运行时支撑；自造回卷会在锁持有半途/堆半分配态下复活执行，未定义面比首败即停更大，且直接违反 f4 F9 halt 语义声明。
- **host 外移吸收密度需求**：正道备选，不属本 note 否决面——密度痛点真出现时先走 `test-host`（moss libkernel 同款）。

## Consequences

- research/18 §3 可借鉴①（P1 在账）由本 note 兜底否决；18 号正文不回改（一事实一归属：调研记对照事实，裁决归 note）。
- ktest 断言语义维持 fail-loud 首败即停；`panic = "abort"` 与 handler halt 立法不动。
- 重开条件：ktest 纯函数测试体量显著增长且失败频发成痛点时，优先 host 外移，其次重审软断言限定形态；任何形态不抬 nightly、不改 panic 策略。
- 基线取证（2026-11-08，QEMU virt/cortex-a72/-kernel ELF 直载）：boot 全测试标记绿 + `ktest done` 出现、无 PANIC、内核续跑至调度金丝雀（switches 金丝雀）——现状未受影响（本次未改码，取证性质）。

## Related

- `research/18-四家测试体系对比.md` §3 可借鉴①、§2 矩阵「内核内自测」行
- `kernel/src/ktest.rs`（头注：断言失败 → panic；六横切锁纪律）
- `kernel/src/main.rs` panic handler（G0 fail-loud + f4 F9）
- moss 对照：`/tmp/os-src/moss-kernel/src/testing/mod.rs`、`rust-toolchain.toml`（nightly）
