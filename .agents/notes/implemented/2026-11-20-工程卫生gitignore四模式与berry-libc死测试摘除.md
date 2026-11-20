# Agent Note: 工程卫生双刀——.gitignore 四模式防再犯与 berry-libc 死测试摘除

Status: implemented

## Problem

两处「说谎的卫生面」：① `.gitignore` 对构建与临时产物采取逐目录/逐文件名枚举（根锚定 `/target-*/` 十余个 `/xxx/cargo-target/`、逐盘名 `/disk.*.img`），枚举天然漏网——当日实抓：子目录深度隔离 target（`drivers/target-af/`、`tools/berry/target-fps1/`、`user/berry-gfx/target-lt/`、`drivers/bins/gui-inputd/target-ws1/` 等 12 目录）、任意名盘镜像（`disk-qw1.img`）、compat 三 crate 的 `cargo-target/` 产物 69 文件误入库（并行 lane 当刀已 staged rm --cached）、widgets 四 crate 同族产物 93 文件仍在库；② `user/berry-libc/src/lib.rs` 的 errno_probe 模块挂 5 个 `#[test]` 属性，但头注已勘定本 crate 结构性不可 host 测试（crt0 `global_asm!` 为 ELF 写法 darwin 汇编器拒 + libc 同名符号对 host libSystem 插入）——属性是永不执行的死属性，向读者谎报存在可跑测试面。

## Decision

- **枚举升通配（四模式追加在 .gitignore 末尾，不并旧节——旧条目留作历史判例文档）**：`cargo-target/`、`target-*/`、`*.img`、`screenshot.png` 全部任意深度无锚定。理由：产物目录名的「命名纪律」（隔离 tag 后缀、任意盘名、临时截图名）由各工具自选，白名单枚举每漏一次就要补一刀——通配一次盖全族。
- **berry-libc 摘属性不删函数**：5 个 `#[test]` 全删，函数体与 doc 注释原样保留（errno 立值档案 + `cargo check --tests` 编译验证路径不变），mod 配 `#[allow(dead_code)]`，mod 头注补摘属性缘由——可跑执法锚本就在 berry-fsclient 镜像（const assert 单源钉死），此处属性零执法价值。

## Alternatives considered

- **把 5 个测试函数连体删除**：被拒——errno 立值断言清单是「对照 asm-generic/musl 双源核实」的档案（EADDRINUSE=98 三审翻案等决策链就在注释里），删函数即丢档案；编译验证路径（check --tests）也要有实体才编。
- **`#[ignore]` 标注替代摘除**：被拒——`#[ignore]` 语义是「暂不跑、可复活」，而结构性不可测试是永久态，`#[ignore]` 反而把「永不执行」谎报成「暂时不执行」。
- **新四模式并入既有旧节（去重收口）**：被拒——旧条目（如六个重复的 `/.target-*/`）各自带历史判例注释，合并重写是纯文档整容且扩大本刀 diff 面；并行会话纪律下最小 diff 优先。

## Consequences

- 零误伤实证：`git ls-files` 无 tracked 的 `*.img`/`screenshot.png`/`target-*`；新 `cargo-target/` 命中的 tracked 仅 widgets 四 crate 历史误入库产物（93 文件，全部 `CACHEDIR.TAG`/fingerprint/rlib 构建产物，零源文件），且 gitignore 不 untrack——93 文件仍在 index，`git status` 无新增 deleted。
- 漏网 untracked 全数入ignore：12 个子目录深度 target + `disk-qw1.img` + compat/berry-efi/hofface 的 cargo-target 均被新四模式命中（check-ignore -v 逐条验）。
- berry-libc 验证：`cargo build --release -p berry-libc --target aarch64-apple-darwin` 绿（3 告警全在 net.rs 基线 never-used，与本次改动无关——build 不编 cfg(test) 模块）；`cargo check --tests` 绿且 errno_probe 零新告警（allow(dead_code) 生效）。
- 全链验证：`node .agents/tools/check-docs.mjs` 542 篇 0 告警；`just build-user` exit=0 全链绿（mkcpio 45842432B x64 条目、tlsscan x62 ELF 全过）。
- **临时文件清理建议（未执行，留后续刀）**：widgets 四 crate（intentd/island/scene-ir/widgetd）93 个入库构建产物建议 `git rm -r --cached` 清偿（与 compat 69 文件同族同因）；根目录 40+ 个 `target-*` 隔离目录与 `disk-qw1.img` 等本地缓存可整体删除腾盘（现已被 ignore，不碍 status）；`bootloader/berryboot/app/cargo-target-build/` 不匹配 `cargo-target/`（目录名不同），属并行 lane 活产物未动。

## Related

- [F103 收口调研](../../research/2026-11-17-四刀与合并事件遗漏检查.md)（target-* 族首刀枚举的出处——本刀是其通配化升级）
- [/.target-*/ 剥落重补判例](2026-11-13-pager-table-window-face.md)（根锚定模式被 base 重写剥落、随刀三次重补的同款循环实证——本刀通配化从模式面消解该循环）
