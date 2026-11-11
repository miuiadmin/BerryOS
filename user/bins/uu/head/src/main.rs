//! head —— uutils coreutils head（=0.12.0，MIT）薄 harness（C-2c text 批；
//! C-2a 家族同型）。fd 克隆面挂账：unix stdin 路径 `try_clone_to_owned` → fcntl(F_DUPFD)
//! 桩 -EINVAL，head 家族 v1 结构性不可验收（U3 步③门审裁出，验收随
//! libc fd 克隆刀补——bsh main.rs U3 头注）。

/// crt0 锚（hexyl 先例）：berry_crt0_anchor 进根集合，crt0 路径不被
/// 链接器扫掉；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    let code = uu_head::uumain(std::env::args_os());
    std::process::exit(code);
}
