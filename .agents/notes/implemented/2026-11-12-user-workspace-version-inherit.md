# Agent Note: 用户态 workspace 版本统一——41 member 继承 [workspace.package]（版本治理步骤 1）

Status: implemented

## Problem

版本治理方案 v1.0（[docs/全局版本号治理方案-v1.0.md](../../../docs/全局版本号治理方案-v1.0.md)）盘点出版本号六大混乱点，其中用户态最散：user/Cargo.toml 独立 workspace 的 41 个 member crate 各自硬编码 `version = "0.1.0"`，与根 workspace 现行 v0.2.0 漂移一个版本——svcmgr banner（`BerryOS/svcmgr 0.1.0`）已经把漂移印在运行时输出上。迁移步骤 1 要求用户态版本单源化：`[workspace.package].version = "0.2.0"`，member 经 `version.workspace = true` 继承。

## Decision

- **单源落 workspace.package**：user/Cargo.toml 加 `[workspace.package]` 段（`version = "0.2.0"` + `edition = "2021"`）；本刀只迁 version 继承，edition 先列在段里备后续刀——升版本从此只改 workspace 一处。
- **member 全量 41 个继承，不是任务书说的 33 个**：实测 members 列表 41 路径（32 个 berry-* lib + 9 个 bins/*）全部 `version = "0.1.0"`，以 members 列表为准逐个替换为 `version.workspace = true`；hexyl（上游 0.17.0 锁定）与 uu 家族（上游 =0.12.0 锁步）住各自独立 workspace、本就不在 members，天然不动；berry-ksproto 是兄弟会话 WIP 的独立空 workspace，同样不碰。
- **任务书未预见的暗钉必须同批升**：berry-libc（8 处）与 berry-fsclient（3 处）的内部 path 依赖写的是 `berry-x = { path = ..., version = "0.1.0" }` 双写形态——被依赖 crate 升 0.2.0 后这 11 处钉立即解析失败（path dep 的 version req 照常执法），同批把钉升到 `"0.2.0"`，保留 path+version 双写惯例不拆。
- **CARGO_PKG_VERSION 消费者审计**（任务书点名 berry-sys）：berry-sys 源码零导出 CARGO_PKG_VERSION，无消费者；svcmgr banner 两处（`BC_VER` 常量 + 测试比对）双侧都用 `env!("CARGO_PKG_VERSION")` 自洽同变，无需改码，banner 值随本刀 0.1.0→0.2.0；berry-libc sys.rs uname 注释里「crate 版本 0.1.0」的陈述失真，同批改为「user workspace 继承，现 0.2.0，与根版本两源」——顺带把「两 workspace 同值仍属巧合对齐非单源」写明，防止后人误把 crate 版本当系统版本源。
- **SDK_CURRENT 不受影响**：berry-manifest 的 SDK_CURRENT 与 mkapp.py 的 `SDK_CURRENT_STR = "0.1.0"` 是 SDK 级独立概念（.bap 清单 compat 轨），与 Cargo version 分轨治理，本刀零接触。

## Alternatives considered

- **一刀同时继承 edition**：被拒——迁移步骤 1 范围只有 version；edition 列在 workspace.package 里备后续刀，一刀一改回滚面最小。
- **内部依赖钉全删（path 依赖不写 version）**：被拒——path+version 双写是仓内既有纪律（钉住「跨工作区引人也过版本对账」语义），本刀只升值不拆形。
- **借本刀让 berry-libc 的 CARGO_PKG_VERSION 直接当 uname release 值源**：被拒——user workspace 与根 workspace 是两个版本源，同值 0.2.0 只是治理对齐的巧合，root 升版而 user 未跟时即漂移；BERRY_UNAME_RELEASE 烙印注入 + 手工镜像义务维持原设计（ADR #83 v1.1）。

## Consequences

- 41 个 member 与根 workspace 版本号治理对齐到 0.2.0；`cargo metadata` 41 包全 0.2.0、`cargo check --workspace` 全绿（既有 warning 不增）。
- user/Cargo.lock 同批更新（41 包 0.1.0→0.2.0 行变）；svcmgr 运行时 banner 从此随 workspace 版本走，不再手改。
- 兄弟会话 worktree（.dsh-worktrees/sdk2-berryerr）自带同款 0.1.0 钉与 `BerryOS/svcmgr 0.1.0` 硬编码测试期望——其合并回主线时会撞上本刀结果，需按新版本值随迁（那是对方 WIP，本刀不越界代改）。
- 后续刀（edition 继承、hexyl/uu 例外台账化）见治理方案迁移路径；根 workspace 侧的步骤 2/3 未动。

## Related

- 方案：[docs/全局版本号治理方案-v1.0.md](../../../docs/全局版本号治理方案-v1.0.md)
- uname 值源单源设计（BERRY_UNAME_RELEASE 烙印）：user/berry-libc/src/sys.rs `UNAME_RELEASE_STR` 头注（ADR #83 v1.1）
