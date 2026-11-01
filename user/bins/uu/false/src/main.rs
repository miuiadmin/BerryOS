//! false —— uutils coreutils false（=0.12.0，MIT）薄 harness（C-2a）。
//!
//! 上游 crate 逐字不动；本 crate 只做：CRT0 锚（G-eco-d hexyl 先例）+
//! uumain 调用（官方 `uucore::bin!` 同形——`#[uucore::main]` proc 宏已把
//! uumain 统一改写成 `-> i32` 退出码，直传即可）。链接/行为缺口由
//! berry-libc C-2a 桩批补（stubs.rs/fs.rs）。

/// crt0 锚（hexyl 先例）：berry_crt0_anchor 进根集合，crt0 路径不被
/// 链接器扫掉；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    let code = uu_false::uumain(std::env::args_os());
    std::process::exit(code);
}
