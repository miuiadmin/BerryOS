# 北极星审计 demo-face=off fatal 定谳——虚化三刀（SPAN/PAN、ASID 读点、keep_tlb 降级）

Status: implemented

## Problem

北极星审计最高优先级：fw_cfg 注入 `opt/berry/demo-face=off` 后内核在 `TAGTAB ok` 之后 D-abort fatal（ISS=0x4f、符号化 `syscall::copy_to_user → memcpy`、FAR=0x29800，HVF/TCG 3/3）。复现定谳推翻「demo-off 路径 bug」归因：default（demo-on）同样 fatal（FAR=0x2a048），真根因是三件「档间语义差」——内核假定 TCG/cortex-a72 形态的虚拟化语义，在 PAN 在席（HVF `-cpu host`/TCG `-cpu max`）与 QEMU HVF 的 ASID 自管面全部失守。审计面（HVF+noktest）下三刀依次暴露：第一刀毙于 svcmgr 首个带用户出参的 syscall；第二刀毙于 dpg_faulter 首触 demand；第三刀毙于 pager/svcmgr 取指与栈页的 L3 翻译错（HW/SW 走查面分裂——SW 走查叶在、HW 缺叶）。

## Decision

三刀全落 HAL/arch 层，内核语义面零改动，TCG/真机零漂移：

1. **SCTLR_EL1.SPAN=1**（`arch/aarch64/src/mmu.rs` `SCTLR_ENABLE` 加 bit 23）：PAN 实现上该位复位值 0 = 取异常进 EL1 自动置 PSTATE.PAN=1，而本内核从不使用 PAN（copy_from/to_user 靠 SW 走查逐页验 AP 后直接访问），PAN 一开则一切 EL0 可访问页的 EL1 数据访问变 level-3 权限错——即审计报的 ISS=0x4f fatal。HVF 实测故障态 SPSR/cpsr PAN=1、SCTLR=0x30101185（bit23=0）实锚。写 1 对无 PAN 硬件（cortex-a72 TCG）是写 RES1 位零漂移；PAN 真硬化（访用户面前显式开关）留 v2 安全批。
2. **`current_asid` 读点迁 CONTEXTIDR**（`mmu.rs`）：原读 TTBR0_EL1 位域——双不可靠：8 位 ASID 机上 ASID 落 [63:56]，`>>48` 读出 `asid<<8` 错键（HVF host 实测 `ID_AA64MMFR0.ASIDBits=0`）；且 HVF 对 TTBR0 读回丢弃 ASID 位域（实测写 `1<<56|base` 读回 `0|base`，Hypervisor 自管 guest ASID）——恒读 0 ⇒ dpg 探针 asid 反查恒 miss ⇒ 首触 demand 走 fatal。改读 CONTEXTIDR_EL1（激活面 `activate_ttbr0_regs` 每次同窗写的软件镜像 `asid<<13`，两档均忠实回读；per-CPU 寄存器无跨核串键面）。
3. **免失效激活档虚化降级**（`mmu.rs` `activate_ttbr0_keep_tlb` + boot 探针）：P0-3 keep_tlb 免 `tlbi aside1is` 的正确性前提是「按号标记的 TLB 语义」——HVF 把 guest ASID 自管后该前提破，跨表切换不失效 = 旧表项对新表别名（实测 pager 取指 0x10c64 与 svcmgr `copy_to_user` 写栈页均 L3 翻译错，且 `Vmar::unmap`/`clear_leaves` 全程零调用实锤非解映射）。修 = boot 单线程窗（`rust_main` 早期、首激活前）TTBR0 ASID 位域环回探针 `probe_asid_field_roundtrip`（表基不动只换标记，安全可探测面）：位域丢 = 置 `ASID_TLB_UNTRUSTED`，keep_tlb 整档退化 `activate_ttbr0` 全量档（多付一次按号失效，正确性优先）；TCG/KVM/真机位域环回零命中照走免失效档。

## Alternatives considered

- **msr pan 开关包裹 copy 面**（真 PAN 硬化）：正确长期形但面大（全部 EL1 访用户路径收口），与 v1「内核自由访问用户页」契约冲突，v2 安全批再立；本刀 SPAN=1 显式回归现契约。
- **per-CPU 静态镜像记当前 ASID**（替代 CONTEXTIDR 读点）：跨核共享静态会串键，per-CPU 区又引 HAL 对 smp 区的新依赖；CONTEXTIDR 本就是每次激活同窗写的 per-CPU 寄存器，零新状态。
- **无条件废 keep_tlb**：实测验证过（HVF 全绿）但把 P0-3 优化在 TCG/KVM/真机一并误伤；探针降级保住档位收益只在失守面付代价。
- **修 demo-off 专属面**（spawn 配置/env 块/VMO commit）：审计的候选根因全被复现数据否决——default 态同样 fatal、demo-off 仅改变首个踩雷 syscall 是哪一个（OFF 首触 `vmo_get_size` 出参落 0x29800、ON 首触 `channel_create` 出参落 0x2a048，两者同为 PAN 权限错）。

## Consequences

- 审计场景全绿：HVF+noktest+demo-off 零 fatal、boot 标记（DPG/THR/FRZ n=2/DPGP/DPGS n=4/DPGN n=4/BLOCK/AIDEMO/PI/INTENT）全过；TCG `-cpu max`+noktest+demo-off 同全绿（含 DPGPRESS ok）。default 与 demo-off 两态 TCG 对拍：ON 舰队字母流/G1.5 ok/SVC/DEMO/WAIT/CHAIN/MANY 全在，OFF 态 `DEMO gate off` 恰一次、舰队零输出——demo-face 门（前刀 kernel 半刀 + svcmgr x1 env 后半刀）行为不变。
- **HVF 残余两件时序标定 panic（预存非本刀面，如实立账）**：`dpg_press` θ_p 前向臂（dpg.rs:888，原生速供给停顿结构性低于 TCG 标定）与 `ai_demo` RT 噪声底（main.rs:4143）在 HVF 原生速度下可 assert——同「HVF 档跑 ktest 语义 ≠ TCG」缺口族（ktest FPSR 同族）；panic 后系统续跑（halt 只停当核的既有语义）、其余标记全过。归 AM-2b/ai_demo 标定刀，不在本刀扩大面。
- **dpg_press TCG 偶发 flake 为预存**：宿主高载下反向臂 Σ 可超阈 assert——基线内核（非本刀）同形复现 1/4，A/B 对拍本刀 4/4 绿，非回归。
- **TTBR0 ASID 位域环回探针**成为「虚化层是否保按号 TLB 语义」的单一可观测面；后续 HVF/真机带新档位（如 KVM 真机）首跑先看探针命中。

## Related

- [demo-face fw_cfg 门（kernel 半刀）](../implemented/2026-11-15-demo-face-fwcfg-gate.md)
- [svcmgr idle 后半刀（x1 env gift）](../implemented/2026-11-15-svcmgr-demo-face-idle-闭合.md)
- 雷册判例库「HVF 档语义 ≠ TCG 缺口族」（docs/雷册-判例库.md；07-B1 基线文档同族实录）
