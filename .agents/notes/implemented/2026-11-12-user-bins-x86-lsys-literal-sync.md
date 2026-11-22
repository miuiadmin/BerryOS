# Agent Note: user/bins 22 独立件与 arch/x86_64 版本字面同值——L-SYS 收编刀（治理方案步骤 2 落地）

Status: implemented

## Problem

全仓版本扫描的遗漏账（[docs/阶段沿革.md](../../../docs/阶段沿革.md) 2026-11-12 步骤 2+3 段已挂账）：user/bins 下 22 个独立小工作区（musl 链 std 件 21 个 + none 壳 appstub）与 arch/x86_64（berry-x86）仍是 0.1.0，与 L-SYS 0.2.0 漂移一个版本。任务书对 bins 推荐方案 A（拆 [workspace] 入 user/ members + `version.workspace = true`）、对 arch/x86_64 直接令改 `version.workspace = true`——两者都以「成员化」为前提，须先验可行性再动手。

## Decision

- **22 件一律方案 B：`version = "0.2.0"` 字面 + [workspace] 保留**（bootloader 字面同值先例形态，注释锚「L-SYS 0.2.0」便于升版时 grep 定位）。方案 A 结构性不可行，四重依据：治理方案迁移路径步骤 2 原文是「user/bins 子 workspace **同步**」——同步值、不收编形；根 .cargo/config.toml 把 `build.target` 锁成 aarch64-unknown-none 且按 CWD 发现跨 workspace 边界生效，build-user 在 user/ 下的裸 `cargo build` 会以 none target 构建全部 member——std 件成员化当场炸（stdhello manifest 律「std 程序不可能在 none target 下编」）；GUI-8 冻结裁决「空 [workspace] escape 根 workspace，不入 user/Cargo.toml members」（berry-launcher manifest 头注）；成员化还使各 bin 自带 [profile.release]（opt-level=z/strip/lto）被「cargo 只认工作区根 profile」规则忽略，产物漂移。
- **appstub 随家族保独立**：22 件中唯一的 none-target 占位壳（justfile 显式 `--target aarch64-unknown-none` 编）。单件收编与家族「互不渗漏」形态不一，且其 8 占位 app 复制流 key 在独立构建槽——同走字面 0.2.0，验证按 justfile 原姿势。
- **arch/x86_64 任务书前提有误**：根 workspace members 列表无 arch/x86_64——它自带 [workspace]，头注明言「根 workspace 不含本目录」（ADR #41 独立小工作区，构建走 justfile build-x86 显式 `--target x86_64-unknown-none`）。`version.workspace = true` 在自有 workspace 根下无 [workspace.package] 可继承、必炸；改造为根 member 则根裸 `cargo build`/`check` 以 aarch64-unknown-none 编 x86 件必炸 + 其 [profile.release] 同款降位。故同走字面 0.2.0（bootloader 同款），不成员化。
- **锁同批刷新**：22 件 Cargo.lock 与 arch/x86_64/Cargo.lock 的自件及 berry-* path dep 条目 0.1.0→0.2.0。步骤 1 只刷了 user/Cargo.lock，bins 锁里 berry-libc 等仍钉 0.1.0（非 --locked 构建静默容忍的陈旧钉），本刀出清。
- **台账（[规则 14](../../../docs/规则/14-版本管理.md) §5）只减不增**：user/bins 22 件出账；berry-ksproto（兄弟 WIP 占用域、SEC-02 刀现役）显式入账、占用刀收编时随刀升；tools/berry 留账；hexyl 0.17.0 与 uu 家族锁步上游不动。

## Alternatives considered

- **方案 A（成员化收编 + workspace 继承）被拒**——上节结构性阻断之外，还须推翻 GUI-8 与 build-user 编排两处冻结设计，超出版本刀边界；收编若要做是独立的构建链重构刀。
- **各 bin 自有 [workspace] 加 [workspace.package] 供 `version.workspace = true` 自继承——被拒**：单件 workspace 加继承段是无谓间接层，bootloader 先例已定字面形态。
- **顺手升 tools/berry——被拒**：host 工具链、台账在案件，任务书范围外，另刀。

## Consequences

- L-SYS 0.2.0 字面同值覆盖 +23 件（22 bins + berry-x86）；主树自研面 0.1.0 仅剩台账两件（tools/berry、berry-ksproto）。
- 升版义务：这 23 处是字面值，升版逐 crate 手同步（规则 14 §2 第三条），注释锚「L-SYS」可全量定位。
- 验证证据：cargo check 21 件 musl（aarch64-unknown-linux-musl）+ berry-x86（x86_64-unknown-none）全绿；appstub 按 justfile 原姿势 `--release --target aarch64-unknown-none` 构建绿；berry-ld 咒语端到端构建 fail 出 aarch64 静态 ELF；23 件各自 `cargo metadata` 版本全 0.2.0；任务书给的 user/ metadata 命令 41 包全 0.2.0——注意该视图对独立件天然盲（bins 不在 members），真验收在各自 workspace 的 metadata。
- [规则 14](../../../docs/规则/14-版本管理.md) §2/§5 与 [阶段沿革](../../../docs/阶段沿革.md) 同批更新；docs_guard 复跑。

## Related

- 方案：[docs/全局版本号治理方案-v1.0.md](../../../docs/全局版本号治理方案-v1.0.md)（迁移路径步骤 2「user/bins 子 workspace 同步」）
- 前刀：[2026-11-12-user-workspace-version-inherit.md](./2026-11-12-user-workspace-version-inherit.md)、[2026-11-12-bootloader-drivers-lsys-unify.md](./2026-11-12-bootloader-drivers-lsys-unify.md)
- 规则：[docs/规则/14-版本管理.md](../../../docs/规则/14-版本管理.md)（§2 字面同值家族、§5 例外台账）
