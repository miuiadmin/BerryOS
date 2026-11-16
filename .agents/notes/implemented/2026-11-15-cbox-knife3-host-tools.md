# Agent Note: cbox 刀 3 宿主工具面（berry symbolize + crash-log）

Status: implemented

## Problem

[刀 2](2026-11-15-cbox-knife2-userland.md) 后账本已能落盘；本刀补 AGC 流水线的宿主半段：读 `CBOX.LOG` → 指纹聚合 → 符号化 → 报告。开工前先定谳符号源现实，再定撕裂容错语义（B4 掉电竞技场实测形态的承接）。

## 符号源现实（开工侦察定谳）

- 内核 ELF（berry-ld 链接）**保留 symtab**（211KB）——内核名可直查；且 `user_<名>` 族 rogue 自验件本质是内核内嵌 asm blob（`kernel/src/user.rs` `.globl __user_<名>`，映射于任务空间基址 `USER_CODE_VA=0x1000`）——账本 pc=0x1004 经 `__user_rogue_ptr` 锚点重定基（pc − 0x1000 + 锚点 vma）后落内核 ELF 符号空间，兜底即正解。
- userbins release ELF 被 berry-ld **全 strip**（仅余 .shstrtab）——symtab/dynsym 双空。工具面语义 = 如实报错指路产物位，不静默造假（真 userbins 符号挂构建改造，design.md v0.3.0 挂账）。

## 处置

- `tools/berry/src/symbolize.rs`（零依赖手写）：ELF64 头/节表/symtab 解析（SHT_SYMTAB → SHT_DYNSYM 回落），双空报错带 hint 指路；`nearest` = `st_value <= pc` 最大者报 `名+偏移`。合成 ELF 测试钉板（含 24B Elf64_Sym 布局）；真产物对拍 opt-in（`BERRY_SYMBOLIZE_TEST_ELF` 指向内核 ELF 时跑，缺省跳过——CI 无产物不许红）。
- `tools/berry/src/crashlog.rs`：FAT12 根文件直读（卷面发现 LBA0 验 BPB 健全性否则落 LBA16——主路印章区形态，fatcat.py 同款修法）；`FatErr::Torn`（链断/短链）如实报错非零退出不静默截断；行解析单源 `berry-cbox`（不长第二套）；`(kind, task, pc, lr)` 指纹聚合 → 次数/首末 boot id；`resolve_syms` 三级解析（userbins → worktree 内核 debug → 全局 `~/.cargo-target` 内核，与 `tools/paths.sh berryos_kernel()` 同源序），`user_<名>` 族走锚点重定基。
- CLI：`berry symbolize --pc --elf`（human/JSON 双面）+ `berry crash-log --disk [--elf-dir]`（human 报告/JSONL 指纹流）；`berry-cbox` 进 tools/berry 依赖（首个 path 依赖）。
- justfile test-host 接入 berry CLI 套件（22 测，berry-cbox 行隔壁）。
- design.md v0.2.0 → v0.3.0：工具面节补符号源现实/解析序/撕裂语义，边界挂账补 userbins 符号面与单簇件撕裂不可见两条。

## 验证

- test-host berry 套件 22/22 绿（4 新测：最近符号三态/strip 双空 hint/非 ELF 拒/真产物 opt-in 对拍；FAT12 对真 `mkfat --put` 造盘对拍 + 大小写不敏感 + NoEnt；解析聚合往返含坏行拒例；撕链检测）。`BERRY_SYMBOLIZE_TEST_ELF=<内核 ELF>` 下 opt-in 测实证过。
- 端到端（主路真盘）：刀 2 遗留 disk.img（490B 两行账本）`berry crash-log --disk disk.img --elf-dir .` → `#1 ×2 el0 user_rogue_ptr pc=0x1004 lr=0xffffff804004990c boot 1..2 → __user_rogue_ptr+0x4（内核 blob 重定基）`——重定基链路（0x1004 − 0x1000 + 锚点 → `__user_rogue_ptr`+4 = `ldr x5,[x4]` far=0x40000000 读塌）与 wtdie 案符号化人工结论一致。JSON 面/`berry symbolize` 独立面同验过。
- 撕裂端到端：4KB+ 多簇账本上清零 FAT 首簇项 → `crash-log: 账本簇链撕裂（链内 2048B < 目录项 4230B——掉电/中断形态）` exit 9；单簇件（490B/735B 形态）读满首簇即止不走链、读面无感——数据无损，非缺陷（design.md 挂账明示 `size > 簇容量` 为可检界）。

## 边界

- 测试 fixture 陷阱两枚（本批实踩）：mkfat `--put` 路径写**裸卷**（EFI 引导盘形态 vol@LBA0），非 `--put` 主路才有 16 扇区零前缀 vol@LBA16——测试 helper 需自补前缀对齐主路形态，否则硬编码 vol=LBA16 的几何读全垃圾（root_off 算到 17e9 越界 panic 暴露）；测试并行线程同 pid 共享临时文件名互踩——uniq 需 pid+原子序号。
- `lr=0xffffff80…` 高位形态是账本忠实记录（内核写盘字节原样），非工具面解析偏差。
- 多镜像聚合（LEDGER_MAX 轮出后跨账本拼接）维持挂账，v1 只单镜像。
