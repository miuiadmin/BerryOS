//! appdemo —— app-0 demo app A（`docs/app-0实施设计草案.md` §4）。
//!
//! manifest `console = true`：能力授予面活证——stdout = console 发端别名
//! （appd 翻译授权集：spawn 内 dup RIGHT_SEND 经 env 块 gift[0] 交付），
//! `app APPDEMO up` 一行经 console 服务落串口（smoke 的独立 grep 断言位，
//! `ROOTFS_MARKERS` 数组之外——设计 §6 钉板）。
//!
//! SDK-1a：manifest 升新形态（[compat] 三值 + `fs = ["rootfs"]`）——本
//! bin 是 fs 词的**消费侧自证**（`/etc/mtab` 读回，见 main 内注释）。
//!
//! argv0 自检 = manifest `entry` 串 `bin/appdemo`（stdhello argv[0] 先例）：
//! appd 以 entry 串作 `SpawnSpec.name`——错值 = 断言炸 → abort(134) →
//! appd `reap != Ok(0)` → `APP ok` 缺失 = smoke FAIL loud。
//!
//! `ExitCode::from(0)` 真退出（0x33，EOF 先行）——appd reap 收 0 是
//! `APP ok n=` 谓词的一环（确定性状态谓词，EXIT ok 同族）。

use std::process::ExitCode;

// 链接种子（stdhello 同款）：无引用则 berry-libc 的 crt0（含 `_start`）
// 不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() -> ExitCode {
    assert_eq!(
        std::env::args().next().as_deref(),
        Some("bin/appdemo"),
        "argv0 should be manifest entry"
    );
    println!("app APPDEMO up");
    // fs 词自证（SDK-1a §6.2）：libc open/read 经 fsclient → fs_h gift
    // （env hs[5]）→ fsmgr RPC 真往返——「词表 = 镜子」执法面：manifest
    // 声明 fs 且 appd 装了四元 gift，此读才可能成功。mtab = fsmgr RAM
    // 树预建定值（盘相无关——FAT 子目录/持久化/B4/EFI 全相位恒在）。
    // 失败全 loud：expect/assert panic → abort(134) → reap 非 0 →
    // `APP ok n=2` 缺失 = smoke FAIL。
    let mtab = std::fs::read("/etc/mtab").expect("appdemo: mtab read");
    assert_eq!(mtab, b"/dev/root / fat rw 0 0\n", "mtab 内容漂移");
    println!("app APPDEMO fs ok 23");
    ExitCode::from(0)
}
