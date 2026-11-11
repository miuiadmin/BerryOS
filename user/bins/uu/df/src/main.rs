//! df —— uutils coreutils df（=0.12.0，MIT）薄 harness（C-2 刀 2；C-2a
//! 家族同型）。容量真值面 = berry-libc fs.rs `statfs` 真实现（OP_STATFS
//! RPC：fsmgr BPB 几何 + FAT 扫描空闲簇 → f_bsize/f_blocks/f_bfree）；
//! 挂载表 = uucore fsext `read_fs_list` 两级回落（/proc/self/mountinfo
//! 失败 → /etc/mtab——fsmgr 启动预建静态件，段④）。
//!
//! 与家族其它成员的差异：直调 uumain 绕过了 `#[uucore::main]`（bin_inner!
//! ）——而 **setup_localization 正是在 bin_inner! 里做的**。df 的表头
//! （"Filesystem"/"Type"）每次运行都经 translate!（fluent）输出：本地化
//! 器未初始化时 `get_message` **原样返回消息 ID**（"df-header-filesystem"
//! ——uucore locale.rs get_message_internal 契约），表头列宽随 ID 长度漂。
//! 故本 harness 在 uumain 前手动 setup（en-US 嵌入束——crates.io 版
//! uucore build.rs 扫 registry 把邻位 uu_* 的 ftl 编进二进制；release 路
//! current_exe 失败 → 无盘面 locales 目录 → 恰走嵌入路径，零盘 IO）。
//! 家族其它成员的 translate! 只进错误/帮助文案（无断言面），不加。

/// crt0 锚（hexyl 先例）：berry_crt0_anchor 进根集合，crt0 路径不被
/// 链接器扫掉；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    let _ = uucore::locale::setup_localization("df");
    let code = uu_df::uumain(std::env::args_os());
    std::process::exit(code);
}
