//! guifbd —— GUI framebuffer 显示服务（B1-b+c 起双后端）：lib/bin 双面
//! crate 的 bin 侧薄壳（gui-inputd 先例）。
//!
//! - **lib 侧**（`src/lib.rs`）：vgpu 后端纯面——后端分派 / INDIRECT 链
//!   构造 / ATTACH_BACKING 段切 / 提交序（host `cargo test --lib`）。
//! - **guest 侧**（`src/guest.rs`，本壳 `mod guest` 装载）：真 virtio-gpu
//!   设备链 + berry-libc syscall 面 + 既有 ramfb 呈现/serve 全量（981 行
//!   现役面零重排迁入）。ramfb 红线（无 virtio-gpu 设备时现役行为逐字节
//!   不变）的构造保证在 guest 侧 `main` 的交接分派处。
//!
//! host 兜底 main：host `cargo test --lib` 只建 lib 纯面（bin 不连
//! berry-libc——Cargo.toml 按目标门控），空壳防符号插入劫（T-1 实录）。

#[cfg(target_os = "linux")]
mod guest;

#[cfg(target_os = "linux")]
fn main() {
    guest::main();
}

#[cfg(not(target_os = "linux"))]
fn main() {}
