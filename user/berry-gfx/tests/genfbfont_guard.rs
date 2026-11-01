//! genfbfont 重产守卫（GUI-2 段②）：`tools/genfbfont.py --check` 的 host
//! 测试钩子——生成器内存重产三档字体表 ↔ 在册文件逐字节比对，漂移即
//! 本测试 FAIL（经 `std::process::Command` 调起，CI/本机同线执法）。
//!
//! 防的雷（P6 A-5 基准漂移雷的字体版，mkfat `--check-fixture` 同款）：
//! 改 ART 字典后忘 `python3 tools/genfbfont.py` 重产 = 三档生成物一致
//! 陈旧——berry-fbcon fbcon 测试与 berry-gfx NN 放大共识测试都对着
//! **陈旧基准**自证假绿（共识测试两侧同源于在册字体，「双绿」退化成
//! 单实现自洽），只有字节级重产比对能抓。挂在本 crate 的 host 测试里
//! （而非 justfile）= `cargo test -p berry-gfx` 单入口自带此守卫。
//!
//! 失败时的修复动作 = `python3 tools/genfbfont.py` 重产三档随源入册
//! （见生成器模块头工作流）。

use std::process::Command;

#[test]
fn genfbfont_artifacts_match_generator() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/genfbfont.py")
        .canonicalize()
        .expect("tools/genfbfont.py 在仓（生成器是三档字体表的单一事实源）");
    let out = Command::new("python3")
        .arg(script)
        .arg("--check")
        .output()
        .expect("python3 可用（CI/本机工具链契约——genfbfont 本身即 python3）");
    assert!(
        out.status.success(),
        "genfbfont --check FAIL（ART/生成器与在册字体表漂移）——重产: \
         python3 tools/genfbfont.py\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
