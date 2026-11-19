//! cp —— uutils coreutils cp（=0.12.0，MIT）薄 harness（C-2a）。
//!
//! 上游 crate 逐字不动；本 crate 只做：CRT0 锚（G-eco-d hexyl 先例）+
//! uumain 调用（官方 `uucore::bin!` 同形——`#[uucore::main]` proc 宏已把
//! uumain 统一改写成 `-> i32` 退出码，直传即可）。copy_file_range/
//! posix_fadvise = C-2a ENOSYS/ok 桩——上游回落 read/write 循环（源级
//! 验证的降级路；属性保留缺口见 Cargo.toml 注记）。

/// crt0 锚（hexyl 先例）：berry_crt0_anchor 进根集合，crt0 路径不被
/// 链接器扫掉；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    let code = uu_cp::uumain(std::env::args_os());
    std::process::exit(code);
}
