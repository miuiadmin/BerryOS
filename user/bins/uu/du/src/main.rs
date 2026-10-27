//! du —— uutils coreutils du（=0.12.0，MIT）薄 harness（C-2 刀 2；C-2a
//! 家族同型）。消费面零新增：stat/opendir/readdir 全走 C-1 已有面（探针
//! 实证单独链接通过、零 berry-libc 新符号，1.13MB binary 产出）。rootfs
//! 入册 + bsh `DF ok` 的 du 对拍段随段④。

/// crt0 锚（hexyl 先例）：berry_crt0_anchor 进根集合，crt0 路径不被
/// 链接器扫掉；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    let code = uu_du::uumain(std::env::args_os());
    std::process::exit(code);
}
