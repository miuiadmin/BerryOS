//! berry-dmface —— fw_cfg `opt/berry/display` 显示模式请求解析纯面
//!（W3-Q84）。
//!
//! **动机**：`kernel/src/main.rs::display_mode_req` 是 GUI P2 的显示模式
//! 请求源——fw_cfg `opt/berry/display` 项读 `WxH[@Hz]` 形字符串。判定链
//! 五门（size 域 [1,32]/UTF-8/trim NUL+ASCII 空白/@ 前缀截断/WxH 十进制
//! 解析）此前只活在 kernel crate 里（host 测不可达），而它是显示面
//! `None` 降级（QEMU 默认模式兜底）的**语义单源**——错一门 = 要么放过
//! 畸形串（下游 u32 parse 二次拒但语义错层）、要么误拒合法模式（显示面
//! 静默降级）。本 crate 把判定链按纯投影（字节串入参——fw_cfg 读臂留壳）
//! 抽成纯函数 + 表驱动测试钉板；kernel 侧 `find_file`/`read_to` MMIO
//! 物理臂留壳（纪律同 safeck/chanface/aokface 先例族）。
//!
//! 抽离面（与消费侧逐字对齐——改这里的语义 = 改消费侧行为）：
//! - [`MAX_ITEM_BYTES`]：fw_cfg 项字节帽（32；kernel 侧 `e.size` 门同值）。
//! - [`parse_display_mode`]：五门判定链纯函数——`[u8]` 入参（fw_cfg
//!   `read_to` 的缓冲镜像），`Some((w, h))` = 合法模式请求。
//!
//! **留壳**（物理臂，不外迁）：`fwcfg::find_file` 目录行走 + `read_to`
//! DMA/MMIO 读臂（berry-hal-aarch64 fwcfg.rs 物理面）。

#![cfg_attr(not(test), no_std)]

/// fw_cfg 显示模式项字节帽（GUI P2 契约：`WxH[@Hz]` 串短小，32B 足容；
/// kernel 侧 `e.size == 0 || e.size > 32` 门同值单源迁此）。
pub const MAX_ITEM_BYTES: usize = 32;

/// 解析 fw_cfg `opt/berry/display` 项字节为 `(w, h)` 显示模式请求。
///
/// 判定链五门（与 kernel `display_mode_req` 逐字同序）：
/// 1. size 域 `(0, MAX_ITEM_BYTES]`（空项/越帽拒）；
/// 2. UTF-8 合法（畸形拒）；
/// 3. trim 首/尾 NUL 与 ASCII 空白（fw_cfg 字符串项可能带 NUL 填充）；
/// 4. `@` 后缀截断（`WxH@60` 只取 `WxH`——刷新率面 kernel 不消费）；
/// 5. `W`/`H` ASCII 十进制解析（u32 域；`0` 合法——消费侧几何门自决）。
///
/// 任一门拒 → `None`（kernel 侧降级 QEMU 默认模式）。
pub fn parse_display_mode(bytes: &[u8]) -> Option<(u32, u32)> {
    let len = bytes.len();
    if len == 0 || len > MAX_ITEM_BYTES {
        return None;
    }
    let s = core::str::from_utf8(bytes).ok()?;
    let s = s.trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
    let s = s.split('@').next()?;
    let (w, h) = s.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

#[cfg(test)]
mod tests;
