//! Winit key -> X11 keysym. Printable ASCII maps to itself; other Unicode
//! uses the 0x01000000+codepoint convention; named keys use the X11 range.

use winit::keyboard::{Key, NamedKey};

pub fn keysym(key: &Key) -> Option<u32> {
    match key {
        Key::Character(s) => {
            let c = s.chars().next()?;
            let cp = c as u32;
            if (0x20..=0x7e).contains(&cp) {
                Some(cp)
            } else if cp > 0x7e {
                Some(0x0100_0000 + cp)
            } else {
                None
            }
        }
        Key::Named(n) => named(n),
        _ => None,
    }
}

fn named(n: &NamedKey) -> Option<u32> {
    Some(match n {
        NamedKey::Space => 0x20,
        NamedKey::Enter => 0xff0d,
        NamedKey::Backspace => 0xff08,
        NamedKey::Tab => 0xff09,
        NamedKey::Escape => 0xff1b,
        NamedKey::Delete => 0xffff,
        NamedKey::Insert => 0xff63,
        NamedKey::Home => 0xff50,
        NamedKey::End => 0xff57,
        NamedKey::PageUp => 0xff55,
        NamedKey::PageDown => 0xff56,
        NamedKey::ArrowLeft => 0xff51,
        NamedKey::ArrowUp => 0xff52,
        NamedKey::ArrowRight => 0xff53,
        NamedKey::ArrowDown => 0xff54,
        NamedKey::Shift => 0xffe1,
        NamedKey::Control => 0xffe3,
        NamedKey::Alt => 0xffe9,
        NamedKey::Super => 0xffeb,
        NamedKey::F1 => 0xffbe,
        NamedKey::F2 => 0xffbf,
        NamedKey::F3 => 0xffc0,
        NamedKey::F4 => 0xffc1,
        NamedKey::F5 => 0xffc2,
        NamedKey::F6 => 0xffc3,
        NamedKey::F7 => 0xffc4,
        NamedKey::F8 => 0xffc5,
        NamedKey::F9 => 0xffc6,
        NamedKey::F10 => 0xffc7,
        NamedKey::F11 => 0xffc8,
        NamedKey::F12 => 0xffc9,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_and_named() {
        assert_eq!(keysym(&Key::Character("a".into())), Some(0x61));
        assert_eq!(keysym(&Key::Named(NamedKey::Enter)), Some(0xff0d));
        assert_eq!(keysym(&Key::Named(NamedKey::ArrowLeft)), Some(0xff51));
    }
}
