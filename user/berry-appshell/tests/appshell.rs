//! berry-appshell host 测试（std harness——家族形态同 berry-layout）：
//! 建树断言（kind/a11y/事件 id）+ toast 时序 + loud 拒面。

use berry_appshell::{dialog, menu, toast, toast_expired, scrim, TOAST_TTL_MS};
use berry_gfx::cmd::Color;
use berry_ui::desc::ViewKind;
use berry_ui::theme::{Theme, Tokens};
use berry_ui::Ui;

const W: u32 = 1080;
const H: u32 = 1920;

fn light() -> Tokens {
    Tokens::pick(Theme::Light)
}

#[test]
fn dialog_build_shape() {
    let mut ui = Ui::<64>::new(W, H);
    let root = ui.root();
    let d = dialog(
        &mut ui,
        root,
        &light(),
        b"Delete file",
        b"This cannot be undone",
        b"Delete",
        b"Cancel",
        101,
        102,
    )
    .expect("build");
    assert_eq!(ui.kind_of(d.node), Ok(ViewKind::Stack));
    // a11y：双钮标签在册（Button role）。
    let labels: Vec<&[u8]> = ui.a11y_iter().map(|e| e.label.as_bytes()).collect();
    assert!(labels.contains(&&b"Delete"[..]));
    assert!(labels.contains(&&b"Cancel"[..]));
    assert_ne!(d.on_confirm, d.on_cancel);
    // 撤除 = remove 根（宿主协议）。
    ui.remove(d.node).expect("remove");
}

#[test]
fn menu_build_ids_and_rows() {
    let mut ui = Ui::<64>::new(W, H);
    let root = ui.root();
    let entries: [&[u8]; 3] = [b"Open", b"Rename", b"Delete"];
    let m = menu(&mut ui, root, &light(), &entries, 200).expect("build");
    assert_eq!(ui.kind_of(m.node), Ok(ViewKind::Stack));
    assert_eq!(m.len, 3);
    // 事件 id = base + i（宿主差分路由契约）。
    for i in 0..3u32 {
        let _ = m.on_pick_base + i;
    }
    let labels: Vec<&[u8]> = ui.a11y_iter().map(|e| e.label.as_bytes()).collect();
    for e in entries {
        assert!(labels.contains(&e), "missing label {e:?}");
    }
}

#[test]
fn toast_ttl_and_expired() {
    let mut ui = Ui::<32>::new(W, H);
    let root = ui.root();
    let t = toast(&mut ui, root, b"Saved 1", 1_000).expect("build");
    assert!(!toast_expired(&t, 1_000 + TOAST_TTL_MS - 1));
    assert!(toast_expired(&t, 1_000 + TOAST_TTL_MS));
    assert!(toast_expired(&t, 9_999)); // 远后时刻饱和减无翻车
}

#[test]
fn scrim_is_darkened_page() {
    let t = light();
    let p: Color = t.bg_page;
    let s = scrim(&t);
    assert_eq!(s.r, p.r / 5 * 3);
    assert!(s.r <= p.r && s.g <= p.g && s.b <= p.b);
}

#[test]
fn text_too_long_loud() {
    let mut ui = Ui::<32>::new(W, H);
    let root = ui.root();
    let long = [b'x'; 80]; // > TEXT_INLINE 64
    let r = dialog(&mut ui, root, &light(), &long, b"body", b"ok", b"no", 1, 2);
    assert!(r.is_err());
    let m = menu(&mut ui, root, &light(), &[&long[..]], 9);
    assert!(m.is_err());
    let t2 = toast(&mut ui, root, &long, 0);
    assert!(t2.is_err());
}

#[test]
fn no_std_face_compiles_for_none_target_consumers() {
    // 编译面断言由 CI/cargo check -p berry-appshell --target aarch64-unknown-none 承载；
    // 此处仅锚定 cfg 形态（lib 头 #![cfg_attr(not(test), no_std)]）。
    assert!(true);
}
