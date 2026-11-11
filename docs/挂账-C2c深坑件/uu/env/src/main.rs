//! env —— uutils coreutils env（=0.12.0，MIT）薄 harness（C-2c text 批；
//! C-2a 家族同型）。environ = NULL 空表（berry-libc 桩）——空输出 + exit 0；

/// crt0 锚（hexyl 先例）：berry_crt0_anchor 进根集合，crt0 路径不被
/// 链接器扫掉；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    let code = uu_env::uumain(std::env::args_os());
    std::process::exit(code);
}
