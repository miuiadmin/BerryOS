//! du —— uutils coreutils du（=0.12.0，MIT）薄 harness（C-2 刀 2；C-2a
//! 家族同型）。消费面零新增：stat/opendir/readdir 全走 C-1 已有面（探针
//! 实证单独链接通过、零 berry-libc 新符号，1.13MB binary 产出）。bin/du
//! 随 rootfs 发行；**自验步首跑实录裁出挂账**——uu_du 0.12.0 无条件
//! `thread::spawn` 踩 std 线程面平台缺口（C-2c uu_sort 同型），实录与
//! 表观尺寸面（st_blocks）回收义务见 bsh df_selftest 头注。

/// crt0 锚（hexyl 先例）：berry_crt0_anchor 进根集合，crt0 路径不被
/// 链接器扫掉；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    let code = uu_du::uumain(std::env::args_os());
    std::process::exit(code);
}
