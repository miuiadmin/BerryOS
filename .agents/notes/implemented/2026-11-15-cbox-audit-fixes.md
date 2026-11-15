# Agent Note: cbox 全线审计修复回写（23 裁定全落）

Status: implemented

## Problem

刀 1–3 落树后，owner 令「workflow 模式检查遗漏，深度分析」——10 agent 审查→对抗验证链跑出 23 条 CONFIRMED。本笔记记修复批的裁决与因果；代码在 rescue/cbox 工作树，spec 同批升 v0.4.0（[design.md](../../plans/cbox/design.md)）。

## 关键裁决与陷阱（为什么这么修）

- **EL1 臂无自动 IRQ 屏蔽（前提错误，非行为缺陷）**：same-EL sync 异常（EL1→EL1）不自动置 PSTATE.I——只有 taken-from-EL0（EL0→EL1）自动屏蔽；`arch/aarch64/src/trap.rs` 自家注释为证。原 design 「上下文 PSTATE.I=1」 是错前提。但修复是**改注释不改行为**：seqlock + `fetch_add` 原子分槽本身抢占安全（IRQ 至多延迟单条写入，读者见不稳跳），无需 `msr daifset`。
- **volatile 配对（Rust 抽象机）**：读者原用 plain load 读槽——与 `write_volatile` 并发在抽象机层面就是 UB，哪怕撕裂可检。修 = 读者 `read_volatile` 配对。教训：撕裂检测逻辑正确≠数据竞争合法。
- **两趟同锚（不部分写的结构性保证）**：syscall 尺寸查询趟与拷贝趟各取 `CBOX_NEXT` 锚，趟间新崩溃会让第二趟更长——部分拷出后才 ENOMEM，违反「不部分写」。修 = `drain_total()` 取锚一次传两趟（`drain_with(total, …)` 内 `min(total, now)` 夹取），同锚下结构性等长（不稳槽换哨兵不改行长）。
- **close = TRUNC 提交（落账写失败路径）**：berry-libc 的 close 对可写 fd 发 `commit(fs_h, wire, offset)` = 游标处截断——写半途 close 会把部分写入**提交成账**毁旧账；fsproto 无 discard RPC 可撤 staging。修 = 窗三 put_file 失败路径不 close，代价 = 有界 fd 泄漏（init 每会话 ≤1）。残留 staging 由下次同窗同名全量重写自愈；**跨名写入会自动提交残留**（fsmgr stage_put 单槽全局语义）——结构性修复挂 fsmgr per-fd staging/discard（v2 候选）。
- **装载节内也有映射标（真件对拍抓的）**：SHF_ALLOC 滤掉 `.debug_*` 的 `$d`/`$x`（实测 265 个），但真内核 ELF `.text` 内**合法装载节**也有 `$x`/`$d` 指令/数据边界标——单层滤不净。修 = 第二层 `$` 前缀名滤。这条是 opt-in 真件测试红了才现形的：合成 ELF 钉不出真链接器的节布局。
- **环检测边界**：簇链成环 + 目录项 size 诚实（环内增长可达 size）→ 读满即止，数据错而读不报错——FAT 元数据无环检面，同单簇撕裂盲区族；可检形态 = 野 size（超盘容量）+ 环 → 跳数帽 `盘长/簇字节 + 2` 报 Loop。
- **FAT 解析 FatErr 不 panic**：几何参数野值三面——spc=0（除零）、簇号越界 FAT 窗、shnum==0 扩展节计数——全走 `get()` 界检 Err 出。
- **退出码语义（全毁 ≠ 健康）**：账本非空而零行可解析 = 全毁形态，原实现退出 0 与健康态混淆——修 = 非零退出；部分坏行不致命（人类面头注 `（拒 N 坏行）`、JSON 面 stderr 注，坏行不计指纹）。
- **名域 1..=15 互逆契约**：name 定长 16B NUL 填充，满 16 字节任务名（`current_task_name` 无长度界）会破坏行编解码互逆——内核写侧 `min(15)` 保底 NUL，宿主渲染名字域外字节 → `?`。

## 处置（按面）

- 内核：`cbox.rs`（PSTATE 注释分臂 / volatile 读写注 / min(15) / `drain_total`+`drain_with` 锚 / 读者 `read_volatile`）、`syscall.rs`（cap `u32::try_from` 拒野值 / 两趟同锚 / 防御性注释）、`abi.toml` 注释改「syscall 整体走 SVC_CLASS_TABLE 在册取锁」+ `just abi-gen` 再生。
- 用户态：launcher `put_file` 失败不 close（含设计因果注释）；svcmgr 死亡排液 `kind != 0` 哨兵滤；berry-cbox `name_visible` 15 界 + 名域守卫测。
- 工具：symbolize 两层滤 + shnum==0 扩展计数 + 测试件双污染标（`$d` 钉名滤、`debug_marker` 钉装载滤）；crashlog `SymSrc` 六态（NoDir/NoElf/UserBins/Kernel/Blob/NoAnchor 各有诚实文案）/ `parse_ledger` 拒行计数 / bpv_ok spc 幂二 / 三新测（环检测、野盘不 panic、重定基数学与源态）。
- 文档：design.md v0.3.0 → v0.4.0（PSTATE 前提 / 两趟同锚 / 写失败不 close / 两层滤 / 退出码契约 / 环盲区 / staging 跨名窗挂账 / 三处未合入 lane 引用注记）。

## 验证

- berry CLI 27/27 + opt-in 真内核 ELF 对拍绿（`BERRY_SYMBOLIZE_TEST_ELF`）；berry-cbox 10/10。
- kernel / svcmgr / launcher（musl）cargo check 零新警告（svcmgr `i` 警告为 DPGB 旧账非本批）。
- e2e 回归：刀 2 遗留 disk.img → `berry crash-log` 输出与落账形态逐字一致（`#1 ×2 el0 user_rogue_ptr pc=0x1004 … → __user_rogue_ptr+0x4（内核 blob 重定基）`）；symbolize 0x 前/裸两形态同路径。
