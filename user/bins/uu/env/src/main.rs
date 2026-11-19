//! env —— uutils coreutils env（=0.12.0，MIT）薄 harness（C-2c text 批；
//! C-2a 家族同型）。environ 真身已落（export 面刀：spawn env 消息→静态区）；
//! 无命令面时空环境输出空 + exit 0、`K=V` 经 setenv 落 environ 可列显——

/// crt0 锚（hexyl 先例）：berry_crt0_anchor 进根集合，crt0 路径不被
/// 链接器扫掉；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    let code = uu_env::uumain(std::env::args_os());
    std::process::exit(code);
}
