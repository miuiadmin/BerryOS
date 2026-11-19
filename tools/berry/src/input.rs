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

    /// 全部 27 个命名键逐键钉板（X11 keysym 区段契约）——防止映射表被
    /// 静默改动（错值/漏键在运行期表现为 VNC 侧按键错乱，无编译期信号）。
    #[test]
    fn named_keys_full_table() {
        let cases: &[(&NamedKey, u32)] = &[
            (&NamedKey::Space, 0x20),
            (&NamedKey::Enter, 0xff0d),
            (&NamedKey::Backspace, 0xff08),
            (&NamedKey::Tab, 0xff09),
            (&NamedKey::Escape, 0xff1b),
            (&NamedKey::Delete, 0xffff),
            (&NamedKey::Insert, 0xff63),
            (&NamedKey::Home, 0xff50),
            (&NamedKey::End, 0xff57),
            (&NamedKey::PageUp, 0xff55),
            (&NamedKey::PageDown, 0xff56),
            (&NamedKey::ArrowLeft, 0xff51),
            (&NamedKey::ArrowUp, 0xff52),
            (&NamedKey::ArrowRight, 0xff53),
            (&NamedKey::ArrowDown, 0xff54),
            (&NamedKey::Shift, 0xffe1),
            (&NamedKey::Control, 0xffe3),
            (&NamedKey::Alt, 0xffe9),
            (&NamedKey::Super, 0xffeb),
            (&NamedKey::F1, 0xffbe),
            (&NamedKey::F5, 0xffc2),
            (&NamedKey::F10, 0xffc7),
            (&NamedKey::F11, 0xffc8),
            (&NamedKey::F12, 0xffc9),
        ];
        for (key, want) in cases {
            assert_eq!(keysym(&Key::Named((*key).clone())), Some(*want));
        }
        // F2/F3/F4/F6/F7/F8/F9 同段连续性抽查（0xffbf..0xffc6 区段）。
        assert_eq!(keysym(&Key::Named(NamedKey::F2)), Some(0xffbf));
        assert_eq!(keysym(&Key::Named(NamedKey::F3)), Some(0xffc0));
        assert_eq!(keysym(&Key::Named(NamedKey::F4)), Some(0xffc1));
        assert_eq!(keysym(&Key::Named(NamedKey::F6)), Some(0xffc3));
        assert_eq!(keysym(&Key::Named(NamedKey::F7)), Some(0xffc4));
        assert_eq!(keysym(&Key::Named(NamedKey::F8)), Some(0xffc5));
        assert_eq!(keysym(&Key::Named(NamedKey::F9)), Some(0xffc6));
    }

    /// 未收录命名键返回 None（F13 起出表）。
    #[test]
    fn named_key_out_of_table_is_none() {
        assert_eq!(keysym(&Key::Named(NamedKey::F13)), None);
        assert_eq!(keysym(&Key::Named(NamedKey::CapsLock)), None);
    }

    /// 字符面三档：可打印 ASCII 恒等映射 / 高于 0x7e 走 0x01000000+cp /
    /// 控制字符（<0x20）拒绝。多字符串取首字符（winit 合成输入形态）。
    #[test]
    fn character_boundaries() {
        // 可打印 ASCII 两端。
        assert_eq!(keysym(&Key::Character(" ".into())), Some(0x20));
        assert_eq!(keysym(&Key::Character("~".into())), Some(0x7e));
        assert_eq!(keysym(&Key::Character("A".into())), Some(0x41));
        // 非 ASCII（含中文）走 Unicode 约定段。
        assert_eq!(keysym(&Key::Character("中".into())), Some(0x0100_0000 + 0x4e2d));
        // 控制字符拒绝（0x1b ESC 文字面 vs Escape 键位分家）。
        assert_eq!(keysym(&Key::Character("\u{1b}".into())), None);
        assert_eq!(keysym(&Key::Character("\u{0}".into())), None);
        // 空串拒绝；多字符取首。
        assert_eq!(keysym(&Key::Character("".into())), None);
        assert_eq!(keysym(&Key::Character("ab".into())), Some(0x61));
    }
}
