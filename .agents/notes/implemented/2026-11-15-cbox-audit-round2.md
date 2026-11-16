# Agent Note: cbox 二轮 workflow 审计修复回写（21 裁定全落）

Status: implemented

## Problem

一轮修复（[2026-11-15-cbox-audit-fixes.md](./2026-11-15-cbox-audit-fixes.md)）落树后，owner 再令「workflow 模式检查遗漏」——wf_69c3c236 二轮审查链跑出 21 CONFIRMED / 1 uncertain / 3 refuted。本笔记记二轮修复批的裁决与因果；代码在 rescue/cbox 工作树，spec 同批升 v0.5.0（[design.md](../../plans/cbox/design.md)）。

## 关键裁决与陷阱（为什么这么修）

- **fsmgr staging 三面语义（二轮最大发现）**：staging 是服务全局单槽，键 (fat11name, buf)；`handle_get` **staging-first**（staging 影子盘读）；`stage_put` 见异名**自动提交**上一名残留。推论三连：① 写半途留下的部分新账，后续任何异名 fs 写（bsh FATW、svcmgr 窗口皆现实写者——一轮「v1 无 init 后 FAT 写者」的 comment 前提被驳）会把残缓当 `CBOX.LOG` 提交、顶替旧账；② 不 close 只是不毁旧账，**不清槽**；③ 清槽 = 同名回写旧内容 + commit 一次清**槽 / 盘真值 / 读影**三面。修 = 写失败当场撤销性回写（launcher 第二 fd 全量重写旧账；svcmgr `cbox_restore_ledger` re-put 0..old_len + commit(old_len)），替代一轮「残留靠下次同窗重写自愈」的被动面——自愈在下窗前有整整一个异名写入暴露窗。
- **回写只限 commit 点前**：`cbox_write_ledger` 读回失配返 `-997` = 新账**已提交**（TRUNC 已落，盘上真值已是新内容）——此时回写旧账 = 撤销已提交数据（错向修复）。修 = `-997` 不回写，loud eco 行交调用方。边界对称性：PUT/TRUNC 段失败 = 未提交 → 回写安全；close 失败二义（TRUNC 已达/应答丢失）= 回写最坏撤销已提交 → 记录仍在 RAM 表、下窗同 (boot, seq) 补落，表现为 boot 维度假分裂（零数据丢失，与 boot_pick 挂账同族）。
- **gen-2 init 窗回混（时间闸的必然性）**：launcher 监督重启后 gen-2 立即重入 init（icons APP_LIST → layout → cbox_io）发 fs RPC；窗二 sole 门只看 APP_CHILDREN 对此盲；`cbox_drain_stale`（recv 至 EAGAIN）会把 gen-2 在途应答当残留吃掉——fsproto 严格串行无请求号、全体 client 共享单应答队列，回混即毁会话。协议**无 init-done 信号** → 时间闸 `LAUNCHER_INIT_UNTIL`（assemble 成功 now()+10s 帽、launcher 死亡清零）是 v1 唯一可实现收口；boot 形态结构性安全（assemble 先于稳态环）。根治 = launcher 独立 fs 会话（v2 挂账）。
- **boot_pick 判别测试（旧组不构成续号）**：内容匹配续号只对最新（max_boot）组生效——实现如此、测试先前没钉。判别例：全等行在旧 boot 组 + 最新组只有异内容行 → 必须开新组（若扫全组会误续 max_boot）。旧组命中 = 上上 boot 的同内容崩溃把本 boot 并进旧组，归组错位。
- **symbolize 野几何双形态（dev panic / release 错位成功读）**：shoff=u64::MAX 一类野值在朴素 `+` 下——dev 剖面（overflow-checks on）panic、release 回绕（u64::MAX+4 回绕 3 恰落 ELF 头内 =「错位成功读」形，比 panic 更险：静默出假数据）。修 = 节表偏移全链 checked 运算。同批：symtab 迭代帽只收节内**完整 24B 项**（sh_size 非 24 倍数的尾巴残项曾把 strtab 首字节当符号项读）；扩展计数野值夹文件长帽后仍可越真表 → Err 收口（帽挡的是 `vec![false; shnum]` 巨分配第一道闸，两道防线各司其职）。
- **SymSrc 两新态（诚实文案的分寸）**：`BadName`（任务名含路径段——账本 name 域来自崩溃记录非 CLI 校验域，join 前拒防 `--elf-dir` 逃逸）；`Derived`（`user_child`/`user_thread`——内核对无名单监督任务的统一常量记名，既无 userbins 件也无 `__user_` 锚点，走锚点查找只会拿「无锚点」误导文案——拼写漂移义 ≠ 结构性不可符号化义，故在内核候选序前立态）。
- **CLI 出口语义钉板（宿主脚本契约面）**：坏 `--pc` = 非零退出（脚本靠退出码判健康度，不能只靠 stderr 文本）；`--json` 契约 = stdout 纯 JSONL、human 诊断（拒行注记）走 stderr；`--pc` 0x 前缀与裸十六进制 stdout 逐字节相等——三面经 `CARGO_BIN_EXE_berry` 真进程边界集成测试钉板（单元测试钉不出 clap 解析与退出码链）。
- **内核 ELF 第四候选**：全局 release 件（`~/.cargo-target/aarch64-unknown-none/release/berry-kernel`）此前不在候选链——release-only 构建面会误报 NoElf。候选序定谳 = worktree debug → 全局 debug → worktree release → 全局 release（`tools/paths.sh` 三候选序的扩展，design.md 旧文「同源序」表述已改）。
- **CBOX_NEXT 饱和**：`fetch_add` 回绕后 `+1`（行编码 seq+1）在 dev 剖面 overflow panic、release 静默错——修 = 饱和 `saturating_add(1)`，2^32 次故障后 seq 钉 u32::MAX 不回绕（回绕 = 去重游标全体失效，宁可停止排队）。
- **ktest 污染隔离 + 未对齐读 UB**：黑匣子是生产面——ktest 用例前 `ktest_save`/后 `ktest_restore` 快存恢复 CBOX_NEXT 与表（否则 battery 窗把测试记录照实落账）；CboxRec（align 8）在字节偏移 12+k*104（≡4 mod 8）处——`read_volatile` 未对齐 = UB，拷进对齐临时（`copy_nonoverlapping`）才是规范化读。
- **环帽边界可测性（截短盘脱钩术）**：跳数帽 `d.len()/clus_bytes + 2` 的恰界测试 = 截短盘面使帽小而可数——FAT 表在数据区**前**，截短只缩 d.len() 不缩 FAT 覆盖，两量脱钩才能构造「链长恰 cap（尾 EOC）不误报 Loop」与「cap+1 环触发 Loop」的对照。

## 处置（按面）

- 内核：`cbox.rs`（seq 饱和 / ktest 快存恢复件 / `REC_ZERO_FOR_KTEST`）、`ktest.rs`（test_cbox 全量重写：A 空表 / B 单条 / C 恰满 8 / D 轮转窗序-槽映射 / E 锚夹取，污染隔离 + 对齐临时读；排液缓冲 844B = ABI 0x42 定长同源）。
- 用户态：berry-sys `berry_cbox_drain` 补 `#[no_mangle]`（邻件全有——缺它符号名被 mangling、链接面失约）；svcmgr（`cbox_restore_ledger` + `-997` 不回写 + `LAUNCHER_INIT_UNTIL` 闸 + restart 臂 if/else 分流）；launcher（put_file 失败 → 第二 fd 回写旧账，头注驳「无 init 后 FAT 写者」旧前提）。
- berry-cbox：boot_pick 判别测（旧组全等行不续号）。
- 工具：symbolize（checked 节表算术 + 24B 完整项帽 + 野几何四形态测）；crashlog（BadName/Derived 两态 + 第四内核候选 + `k<N>` 野 kind 数值可见 + 报告序钉板测 + 环帽边界测 + HOME 空目录换出的 NoElf 态测）；新 `tests/cli.rs` 集成测试（退出码 / JSON 契约 / 0x 前缀 parity）。

## 验证

- berry CLI 31 单测 + 2 集成测绿；berry-cbox 10 测绿（boot_pick_rules 内扩旧组判别例）。
- kernel / svcmgr / berry-sys（none-target）+ launcher（musl）cargo check 零新警告（svcmgr `i` 警告为 DPGB 旧账非本批）。
- `just smoke` 全绿（`smoke: PASS`；8 份串口日志各恰一次 `CBOXK ok`，`CBOX ok n=` 跨 boot 1→2→3——FAT 持久化相账本跨 boot 续账实证）；`just test-host` 93×ok 零失败；`SMOKE_KEEP_LOG=1 just smoke-image` 全绿（`CBOXK ok` + `CBOX ok n=2` 到席，kfile 字节对拍 5002120B）。三跑均以 worktree 本地 `CARGO_TARGET_DIR` 隔离——r2/r3 曾被兄弟会话全局 `~/.cargo-target` 重烘的外核顶替（banner 与 `CBOXK` 缺席定谳），门禁跑必须隔离 target dir。
