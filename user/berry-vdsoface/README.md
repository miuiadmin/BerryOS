# berry-vdsoface —— vDSO 数据页布局与换算单源（纯函数面）

BerryOS `clock_read` MONOTONIC vDSO 化的单源面（W3-Q106，03 号审计
机会三）：vDSO 数据页布局常量 + magic/version 域 + ticks→ns 换算纯函数。

- **页布局冻结 ABI**：magic / version / freq / seq 域偏移与宽度常量
  （`VDSO_VA` 固定映射地址）——内核侧建页填页（`kernel/src/vdso.rs`，
  path 依赖跨工作区引本 crate，先例 = berry-fbcon）与 user 侧
  `berry-sys` fast path 读页换算同源，双消费不两写。
- **ticks→ns 双除式**：与内核 `sys_clock_read` 同式（先商后余防中间
  溢出），host 可单测。
- **page_ready 谓词**：magic 校验 + seq 奇偶锁读（宽松读侧），未就绪
  时 fast path 回落 SVC。

本 crate 零依赖、零 syscall 面，`cfg_attr(not(test), no_std)`，任意
host 可单测（`tests/` host 5 测，justfile test-host 接线）。
