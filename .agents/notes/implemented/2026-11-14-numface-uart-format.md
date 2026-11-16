# Agent Note: UART 整数格式化族纯面单源 berry-numface（W3-Q83）

Status: implemented

- Type: note
- Date: 2026-11-14
- PR: #274

## 问题

HAL 四处散落同族「整数 → ASCII」装配律：`uart.rs` 的 `uart_put_dec`（u64 十进制）与 `uart_put_hex64`（固定 16 位小写 hex）、`trap.rs` 的 `dec2`（两位十进制）与 `put_kv`/`put_reg` 行装配、`psci.rs` 的 `put_hex32_upper`（8 位大写 hex）。装配律各自内联，数字表字面三份拷贝（HEX_LOWER/HEX_UPPER/DEC），trap dump 行装配序（key → hex64 → 换行 / "  x" → dec2 → "=0x" → hex64 → 换行）只存在于调用序列中——drift 面实存。

## 决策

- 新 crate `berry-numface`（零依赖纯函数，`#![cfg_attr(not(test), no_std)]` 双面，形态同 balface/bandface）：`write_dec_u64`/`write_hex64_lower`/`write_hex32_upper`/`write_dec2`/`write_kv_line`/`write_reg_line` 装配律单源 + 数字表/位宽常量（DEC64_MAX_DIGITS=20/HEX64_DIGITS=16/HEX32_DIGITS=8）单源。
- HAL 消费壳只留发射循环（uart_putc/uart_puts MMIO 臂零扰动）——asidface 迁 mmu.rs 面、uartface 迁 uart.rs 面同款纪律：HAL 可以依赖 face crate（#254/#255 先例），发射面留壳。
- host 10 测钉板：全值域采样 + 1000 随机对拍（参考实现 format! 对照）/hex64·hex32 冻结锚/dec2 寄存器下标 0..30/kv·reg 行装配序/敌意短缓冲安全律（截断不 panic 不越界）/数字表冻结锚/10^n 边界。
- 判据逐字节对齐原实现（`write_dec2` 保 `dec2` 恒两位形——n>99 只映低两位，消费面寄存器下标 ≤30 无截断语义差异）。

## 验证

- `cargo test -p berry-numface --target aarch64-apple-darwin` 10 测全绿（隔离 target）。
- HAL no_std 构建 0 error（numface 消费壳编译进 berry-hal-aarch64）。
- 装配输出零变化：四消费点行为逐字节等价（冻结锚钉死形态）。

## Related

- [timer-slack-face](./2026-11-13-timer-slack-face.md)（face crate 外迁纪律同款）
