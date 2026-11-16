//! berry-appshell —— 应用壳 crate（波3-件⑦，plans/2026-11-15-波3桌面三栏
//! 与系统窗实施设计.md §6；launcher 五模块纯抽取，行为零变化）。
//!
//! # 模块面（§6 抽取清单）
//!
//! - [`rpc`]：严格串行 RPC 半程族（send/recv 单趟拆分、EBUSY/EAGAIN 阻塞
//!   帽重臂）+ `fail`/park 收口——抽自 berry-launcher main.rs。
//! - [`pump`]：帧循环骨架（IDLE 帽醒 10s/damage 收集/SUBMIT→FRAME_CALLBACK
//!   →RELEASE/vote）+ poll 集（input 事件面）——抽自 frameloop.rs；应用
//!   特化（Msg 分发/自测/标记）经 [`pump::HostHooks`] 回调注入。
//! - [`scene`]：窗树装配底座（状态栏 scrim 动态预混/三组对齐/图标桥/时钟
//!   文本面）——抽自 desktop.rs；桌面特化（网格/壁纸/dock/TabBar 三页）
//!   留 launcher。
//! - [`icons`]：app 注册表 + BYI/OH 资产供给桥（含 status_icons/wallpaper
//!   两生成子模块）——抽自 icons.rs。
//! - [`conf`]：FAT canonical 单簇原子读写（LAYOUT.JSN/BOOT.JSN 同款持久键
//!   面：缺席/坏件回默认 + 重写自愈 + 写失败非致命三约束的读写半程）——
//!   抽自 layout_io.rs。
//!
//! # 形态纪律
//!
//! 库本体 no_std（`cfg_attr(not(test), no_std)` + `extern crate alloc`，
//! berry-ui 同款）：berry-sys syscall 半程按 berry-fsclient 先例
//! `cfg(not(test))` 门（host 符号缺席）；berry-libc 面（诊断输出/park/fs
//! fd 操作）**不直连**——berry-libc 结构性不可 host `cargo test`（crt0
//! `global_asm!` 为 ELF 目标写法 + libc 符号对 host libSystem 插入），
//! 消费者（std musl 应用）boot 期经 [`bind`] 注入 [`Host`] 绑定表。
//! 纯函数面（几何/像素/分类谓词/建树 spec）host 测试（justfile
//! test-host 显式行，`--target <host>`——根 config 锁 none target 跨
//! workspace 边界命中，berry-ui 同款 gotcha）。
//!
//! 消费模板（launcher 形态）：`use berry_appshell as appshell;` +
//! `appshell::bind(&SHELL_HOST)` → 各模块 API 按需取用；诊断行前缀 =
//! `Host::tag`（launcher 绑 "launcher"——与抽取前字节一致）。

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod conf;
pub mod overlay;
pub mod icons;
pub mod pump;
pub mod rpc;
pub mod scene;

// 刀C overlay 面（Dialog/Menu/Toast）根出口 re-export——base 030095971
// 单文件 crate 形态的兼容面（测试/消费者直引 berry_appshell::dialog 等）。
pub use overlay::{dialog, menu, scrim, toast, toast_expired, TOAST_TTL_MS};

mod host;

pub use host::{bind, Fs, Host};
