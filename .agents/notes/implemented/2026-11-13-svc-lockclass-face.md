# Agent Note: SVC 锁类位集单源外迁 berry-svcface（W3-Q67）

Status: implemented

## Problem

SVC 入口锁类分档（S-3c c-2c slice 2b）的六类位集常量与 42 号落类表此前分居两处且 host 测不可达：常量在 `arch/aarch64/src/kern_lock.rs`（HAL 只 aarch64-unknown-none 可编），落类表是 `kernel/src/syscall.rs install()` 的 42 行 `reg()` 字面量。类位集对 sched `HELD_*` 的位镜像一致性只有 kernel main 编译期断言执法（aarch64-only），「NONE 实装号归零律」「未注册号 NONE 默认」零 host 钉板——落类回归（如误把带 copy_out 链的号降 NONE）在 host 侧无咬合。

## Decision

- **常量单源迁移**：`SVC_CLASS_*` 六常量自 kern_lock.rs 迁 `berry-svcface`（无依赖纯面 crate，`cfg(test)` 链 std 跑 host 测）；HAL `pub use` re-export——kernel install()/trap.rs/main 断言面引用路径零改动。
- **注册表纯投影**：42 项 `(nr, class)` 表与 install() 逐行对齐（`tools/svc_registry_check.py` 三方对拍执法——install() 42 行 ↔ REGISTRY ↔ NR 常量域，挂 just test-host 链；负例验证：单项类替换即咬出）；`class_of(nr)` 未注册号返 NONE——与 HAL `SYSCALLS` 默认槽 `(sys_unimplemented, NONE)` 全表初始化同一律的纯函数形。物理臂（static mut 派发表/boot 单线程 install 写入）留壳 HAL。
- **位镜像判据面**：`assembled_class` + `HELD_*_BIT` 把 kernel main 六断言的语义做成 host 可测形（位值漂移 = main 断言红 + host 测红双执法）；REAPQ 位只在 ALL 档（kill/exit 链经全谱包络）入律。
- **易错号钉板**：0x83 policy_bind 字面量号（NR 常量层 v3 未产）、0x3E process_suspend（非 yield）、0x3F thread_yield——注册表锚点抽查钉死。

## Consequences

- 落类回归面双咬合：改 svcface 注册表而忘 install() = 对拍脚本红（文本级三方失配）；改 kern_lock 常量值 = host `class_bits_mirror_held_assembly` 红 + kernel main 断言红。svcface 内部测只覆盖 REGISTRY 自洽（往返/形状/锚点），install() 漂移的咬合在对拍脚本——两者互补缺一不可。
- HAL 增一依赖（berry-svcface）——face crate 无依赖纯面，构建图不变序。
- 后续落类审计（S-4 降档族延续）改注册表即可获得同款钉板，无需逐号手测。
