//! wc —— uutils coreutils wc（=0.12.0，MIT）薄 harness（C-2c text 批；
//! C-2a 家族同型）。输出列宽按 GNU 计（bsh u3 链钉板）；

/// crt0 锚（hexyl 先例）：berry_crt0_anchor 进根集合，crt0 路径不被
/// 链接器扫掉；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    let code = uu_wc::uumain(std::env::args_os());
    std::process::exit(code);
}
