//! appquiet —— app-0 demo app B（`docs/app-0实施设计草案.md` §4）。
//!
//! manifest `console = false`：**能力缺席可运行**的活证——stdout = appd
//! `channel_pair` 私有对端（输出不进串口），main 静默；appd collect 面
//! 只收 libc exit（EOF 先行）发的零长消息（fail 同款快路径）。
//!
//! argv0 自检 = manifest `entry` 串 `bin/appquiet`（appdemo 同款断言面：
//! 错值 → abort(134) → reap != Ok(0) → `APP ok` 缺失 loud）。42/0 的
//! 退出码分工：fail 演非零源，本双 app 演 0——`APP ok n=` 谓词要全 0。
//!
//! app-2 刀2：lifecycle absent 探针落此（见 main 内注释——草案 §6
//! 承诺⑧；console=false 零漂锚不变，成功面零输出零帧）。

use std::process::ExitCode;

use berry_libc::lifecycle::{
    lifecycle_poll, lifecycle_register, state_restore, state_save, LcReg, LcRestore, LcSave,
};

// 链接种子（stdhello 同款）：无引用则 berry-libc 的 crt0（含 `_start`）
// 不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

// app-2 刀2 absent 探针 buf（草案 §6 承诺⑧）：满帽 64KiB-32B——落
// static 不入栈（§3.2 读侧契约栈面约束：65504B 大于任何常规栈帧）。
static mut LC_BUF: [u8; berry_libc::lifecycle::PAYLOAD_CAP] =
    [0; berry_libc::lifecycle::PAYLOAD_CAP];

fn main() -> ExitCode {
    assert_eq!(
        std::env::args().next().as_deref(),
        Some("bin/appquiet"),
        "argv0 should be manifest entry"
    );
    // app-2 刀2：lifecycle absent 探针（草案 §6 承诺⑧——零义务兼容律
    // §2.3 的活证）。appquiet 无 fs 词 ∧ 非 lifecycle spawn 面 ⇒
    // GIFT_H[0]/[1] 缺席 ⇒ 五口全 absent no-op：poll 立返 0、register
    // 返 Absent、restore 返 Fresh 且不发帧、save 返 Absent（beat 不探
    // —— absent 面 beat 立返 0 与 poll 立返 0 同构，免第二拍）。
    // 成功面零输出零帧（console=false 零漂锚 + APP ok n=2 零漂）；
    // 失败面自动 loud：assert 炸 → abort(134) → reap 非 0 → APP ok
    // 缺失；误发帧 → 对 h=0 send 失败 → 库 lifecycle dead park 同红。
    // SAFETY: main 线程独占（lifecycle 全口契约——启动期单线程面）。
    let buf: &mut [u8] = unsafe { &mut *core::ptr::addr_of_mut!(LC_BUF) };
    assert_eq!(lifecycle_poll(), 0, "absent poll should be a no-op");
    assert_eq!(lifecycle_register(None), LcReg::Absent);
    assert_eq!(state_restore(buf), LcRestore::Fresh);
    assert_eq!(state_save(&[]), LcSave::Absent);
    ExitCode::from(0)
}
