//! gui-wmd host 面：权威窗口表的纯策略与状态转移。channel I/O、relay、
//! virtio/present 组合与 berry-libc CRT0 全部留在 guest bin；host 测试通过
//! [`table::WmIo`] 注入确定性 I/O。

#![cfg_attr(not(test), no_std)]

pub mod table;
