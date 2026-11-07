//! gencjkfont 重产守卫（GUI 真实系统化 Phase 4）：`tools/gencjkfont.py
//! --check` 的 host 包装。检查字符表 SHA、A2 二进制 SHA、页偏移/长度与
//! 55 页 zlib 全解压长度。生成器漂移或手工改生成物时这里 FAIL loud。

use std::process::Command;

#[test]
fn gencjkfont_artifacts_match_generator() {
    let root = env!("CARGO_MANIFEST_DIR");
    let script = std::path::Path::new(root)
        .join("../../tools/gencjkfont.py")
        .canonicalize()
        .expect("tools/gencjkfont.py 在仓（3500 CJK A2 页包单一事实源）");
    let out = Command::new("python3")
        .arg(script)
        .arg("--check")
        .output()
        .expect("python3 可用（CI/本机工具链契约——生成器本身即 python3）");
    assert!(
        out.status.success(),
        "gencjkfont --check FAIL（字符表/A2 页包与生成器漂移）——按上游 \
         SHA 取全量 Noto 字源后重产: python3 tools/gencjkfont.py\n--- stdout \
         ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
