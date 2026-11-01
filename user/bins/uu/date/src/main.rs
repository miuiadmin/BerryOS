//! date —— uutils coreutils date（=0.12.0，MIT）薄 harness（C-2c text 批；
//! C-2a 家族同型）。ICU4X 面 ≈1.9MiB（text 批容量腿主增量）；locale 符号经 berry-libc nl_langinfo/setlocale 桩应答（C locale）；确定性验收走 `date -u -d @0`。

/// crt0 锚（hexyl 先例）：berry_crt0_anchor 进根集合，crt0 路径不被
/// 链接器扫掉；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    let code = uu_date::uumain(std::env::args_os());
    std::process::exit(code);
}
