// gui-wmd 双面入口：lib 面承载可 host 测试的 WmTable；guest 面只在
// musl/Linux 目标编译，避免 berry-libc/libc 符号在 macOS test harness 中
// 劫持 libSystem（gui-inputd 先例）。host cargo test 的 bin 为空壳 main。
#[cfg(target_os = "linux")]
#[path = "guest.rs"]
mod guest;

#[cfg(target_os = "linux")]
fn main() {
    guest::run();
}

#[cfg(not(target_os = "linux"))]
fn main() {}
