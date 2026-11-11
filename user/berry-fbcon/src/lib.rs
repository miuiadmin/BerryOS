//! fbcon 纯函数 crate（V-6 刀 2 前置段——`docs/VM里程碑设计草案.md` §4 V-6：
//! 帧缓冲 console 的字体与渲染核先纯函数化 host 测，服务接入/VMO 授能/
//! smoke 标记留刀 2 后段）。两模块：
//!
//! - [`font`]：自绘 8×16 点阵字形（公有领域自绘数组——设计文档钦定「自绘」
//!   非移植，零版权面；覆盖可打印 ASCII 32..=126）。
//! - [`render`]：蜂格渲染核（glyph blit / 滚动行移 / 像素打包）——消费
//!   handoff GOP 事实（`kernel gop_info`：w/h/stride 字节/fmt 0=RGBx 1=BGRx）。
//!
//! 两侧约定对偶锁定：字形 MSB=最左列；stride 为**行字节数**（可大于 w×4，
//! 行尾 padding 寻址一律走 stride）；滚动重叠区用 `core::ptr::copy`（memmove
//! ——`copy_from_slice` 自重叠 = UB，P6 A-7 立法）。
#![cfg_attr(not(test), no_std)]

pub mod font;
pub mod render;
