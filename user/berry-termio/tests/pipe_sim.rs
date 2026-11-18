//! pipe_selftest 流仿真（T6 自查）——berry-term `pipe_selftest` 扩探针的
//! host 侧钉板：按 bsh 回显词汇表（editor.rs + main.rs clear/Interrupt/
//! prompt 面，2026-11-17 逐臂核实）构造 stdin_feed
//! `echo berryterm\nuname\nclear\n\x03` 的预期回显流，喂 TermCore 后跑
//! pipe_selftest 同款三断言。**流是假定解剖**（运行期真值由 berry-term
//! pipe_selftest 在 guest 内执法）——bsh 回显面改动时本测试与 pipe_selftest
//! 同批维护。

use berry_termio::{TermCore, TERM_ROWS};

/// 子串在场（berry-term `has_sub` 同形）。
fn has_sub(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// pipe_selftest 预期回显流 → 三断言全过（旧内容清除/`^C` 行在册/尾段
/// prompt 收尾）。
#[test]
fn pipe_selftest_stream_probes_pass() {
    // 喂流解剖（pipe_selftest stdin_feed 的逐段回显）：
    //   "bsh> echo berryterm\n"  prompt + 键入回显 + Enter 回显
    //   "berryterm\n"            echo 结果行
    //   "bsh> uname\n"           prompt + 键入回显 + Enter 回显
    //   "BerryOS aarch64 0.2.0\n" uname 结果行
    //   "bsh> clear\n"           prompt + 键入回显 + Enter 回显
    //   "\x1b[2J\x1b[H"          clear 7B（清屏 + 光标归位）
    //   "bsh> "                  prompt 重显
    //   "^C\n"                   Ctrl-C Interrupt 回显（追加在被中断行尾）
    //   "bsh> "                  prompt 重显
    let stream = b"bsh> echo berryterm\nberryterm\nbsh> uname\nBerryOS aarch64 0.2.0\nbsh> clear\n\x1b[2J\x1b[Hbsh> ^C\nbsh> ";
    let mut core = TermCore::new();
    core.feed(stream);
    // pipe_selftest ⓪b 三断言（与 berry-term main.rs 同款）。
    assert!(
        !(0..TERM_ROWS).any(|vi| has_sub(core.view_row(vi), b"berryterm")),
        "stale rows after clear"
    );
    assert!(
        (0..TERM_ROWS).any(|vi| core.view_row(vi) == b"bsh> ^C"),
        "^C line missing"
    );
    assert!(core.tail().ends_with(b"bsh> "), "tail not at prompt");
    // 附加解剖锚：清屏后唯一在册行 = 中断行，尾段恰为 prompt。
    assert_eq!(core.view_row(0), b"bsh> ^C");
    assert_eq!(core.view_row(1), b"");
    assert_eq!(core.tail(), b"bsh> ");
}

/// 反证：clear 序列缺席时旧内容仍在可见行集（断言真的在咬）。
#[test]
fn pipe_selftest_stream_probes_bite_without_clear() {
    let stream = b"bsh> echo berryterm\nberryterm\nbsh> ^C\nbsh> ";
    let mut core = TermCore::new();
    core.feed(stream);
    assert!(
        (0..TERM_ROWS).any(|vi| has_sub(core.view_row(vi), b"berryterm")),
        "无 clear 时旧内容应在场（负证）"
    );
}
