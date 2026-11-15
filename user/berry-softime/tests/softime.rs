//! berry-softime host 测试：注入编码往返（guiproto 单源）+ Shift 档语义
//! + 显隐状态机 + 键位表完整性（lite 集闭包）。

use berry_guiproto::{DEV_KEYBOARD, DEV_TEXT};
use berry_softime::{peek, Key, Keyboard, ROWS, FRAME_MAX, KEY_BACKSPACE, KEY_ENTER};

#[test]
fn printable_ascii_via_text_dev() {
    let mut kb = Keyboard::new();
    let mut buf = [0u8; FRAME_MAX];
    let n = kb.inject_frame(Key::Char(b'a'), &mut buf);
    assert!(n >= 4, "frame written");
    let (dev, payload) = peek(&buf[..n]).expect("parse");
    assert_eq!(dev, DEV_TEXT);
    assert_eq!(payload, b"a");
}

#[test]
fn shift_single_shot_uppercase() {
    let mut kb = Keyboard::new();
    let mut buf = [0u8; FRAME_MAX];
    assert_eq!(kb.inject_frame(Key::Shift, &mut buf), 0, "shift itself injects nothing");
    assert!(kb.shift_on());
    let n = kb.inject_frame(Key::Char(b'a'), &mut buf);
    let (_, payload) = peek(&buf[..n]).unwrap();
    assert_eq!(payload, b"A");
    assert!(!kb.shift_on(), "single-shot: shift falls back after letter");
}

#[test]
fn actions_via_keyboard_dev_linux_codes() {
    let mut kb = Keyboard::new();
    let mut buf = [0u8; FRAME_MAX];
    let n = kb.inject_frame(Key::Backspace, &mut buf);
    let (dev, payload) = peek(&buf[..n]).unwrap();
    assert_eq!((dev, payload), (DEV_KEYBOARD, &[KEY_BACKSPACE][..]));
    let n = kb.inject_frame(Key::Enter, &mut buf);
    let (dev, payload) = peek(&buf[..n]).unwrap();
    assert_eq!((dev, payload), (DEV_KEYBOARD, &[KEY_ENTER][..]));
    let n = kb.inject_frame(Key::Space, &mut buf);
    let (dev, payload) = peek(&buf[..n]).unwrap();
    assert_eq!((dev, payload), (DEV_TEXT, &b" "[..]));
}

#[test]
fn face_follows_focus_wanted() {
    let mut kb = Keyboard::new();
    assert!(!kb.face().visible);
    assert!(kb.face().focus_wants(true), "rise = transition");
    assert!(kb.face().visible);
    assert!(!kb.face().focus_wants(true), "no-op re-assert");
    assert!(kb.face().focus_wants(false), "fall = transition");
    assert!(!kb.face().visible);
}

#[test]
fn layout_closure_is_qwerty_lite() {
    // 26 字母全在席、无重复、动作行恰四键。
    let mut seen = [false; 26];
    let mut count = 0usize;
    for row in ROWS.iter() {
        for k in row.iter() {
            if let Key::Char(c) = k {
                assert!(c.is_ascii_lowercase());
                let i = (c - b'a') as usize;
                assert!(!seen[i], "duplicate letter {c}");
                seen[i] = true;
                count += 1;
            }
        }
    }
    assert_eq!(count, 26);
    assert_eq!(ROWS[3].len(), 4);
}
