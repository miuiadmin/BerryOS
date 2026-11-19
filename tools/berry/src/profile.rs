//! Built-in device profiles (design doc §附录 C).
//!
//! v0/P1 profiles only drive the host window aspect and QEMU memory. The
//! `render_*` fields are the target guest mode for P2 (ramfb modeset); until
//! then the guest still boots at 800x600 and the window letterboxes it.

use crate::error::{BerryError, Result};

#[derive(Clone, Copy, Debug)]
pub struct Profile {
    pub id: &'static str,
    pub panel_w: u32,
    pub panel_h: u32,
    pub render_w: u32,
    pub render_h: u32,
    pub refresh: u32,
    pub memory_mib: u32,
}

pub const PROFILES: &[Profile] = &[
    Profile {
        id: "default",
        panel_w: 800,
        panel_h: 600,
        render_w: 800,
        render_h: 600,
        refresh: 60,
        memory_mib: 128,
    },
    Profile {
        // 竖屏 1080P 手机主形态（owner 2026-11-13 裁定：初始分辨率即 1080P、
        // 竖屏 + 底部导航键，不做低清/横屏妥协）；render = panel = 1080x1920
        // 零 letterbox。
        id: "desktop",
        panel_w: 1080,
        panel_h: 1920,
        render_w: 1080,
        render_h: 1920,
        refresh: 60,
        memory_mib: 256,
    },
    Profile {
        id: "phone",
        panel_w: 1080,
        panel_h: 2250,
        render_w: 480,
        render_h: 1000,
        refresh: 60,
        memory_mib: 256,
    },
    Profile {
        id: "phone16",
        panel_w: 1080,
        panel_h: 1920,
        render_w: 480,
        render_h: 854,
        refresh: 60,
        memory_mib: 256,
    },
    Profile {
        id: "phone20",
        panel_w: 1080,
        panel_h: 2400,
        render_w: 540,
        render_h: 1200,
        refresh: 60,
        memory_mib: 256,
    },
    Profile {
        id: "tablet",
        panel_w: 1600,
        panel_h: 2560,
        render_w: 800,
        render_h: 1280,
        refresh: 60,
        memory_mib: 256,
    },
];

pub fn get(id: &str) -> Result<Profile> {
    PROFILES
        .iter()
        .find(|p| p.id == id)
        .copied()
        .ok_or_else(|| {
            let names = PROFILES.iter().map(|p| p.id).collect::<Vec<_>>().join(", ");
            BerryError::env(format!("unknown profile '{id}'"))
                .with_hint(format!("known profiles: {names}"))
        })
}

pub fn default_profile() -> Profile {
    get("default").expect("default profile")
}

pub fn phone() -> Profile {
    get("phone").expect("phone profile")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表内 id 唯一——重复 id 会让 `get` 静默命中前者，CLI 面不可见。
    #[test]
    fn profile_ids_unique() {
        let mut ids: Vec<&str> = PROFILES.iter().map(|p| p.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate profile id in PROFILES");
    }

    /// 几何契约：面板与渲染档非零、纵向屏面板高>宽、刷新率与内存 sane。
    /// desktop 形态 render = panel（零 letterbox，owner 2026-11-13 裁定）。
    #[test]
    fn geometry_sane() {
        for p in PROFILES {
            assert!(p.panel_w > 0 && p.panel_h > 0, "{} panel", p.id);
            assert!(p.render_w > 0 && p.render_h > 0, "{} render", p.id);
            assert!(p.refresh > 0, "{} refresh", p.id);
            assert!(p.memory_mib > 0, "{} memory", p.id);
            // 全表现役档都是竖屏面板（default 800x600 是唯一横档——带头
            // letterbox 的 v0 形态；其余五个都是手机/平板纵向形态）。
            if p.id != "default" {
                assert!(p.panel_h > p.panel_w, "{} panel must be portrait", p.id);
            }
        }
    }

    /// desktop 零 letterbox 契约（owner 裁定：render = panel 1080x1920）。
    #[test]
    fn desktop_render_equals_panel() {
        let d = get("desktop").unwrap();
        assert_eq!((d.render_w, d.render_h), (d.panel_w, d.panel_h));
    }

    /// `get` 命中/未命中语义：未命中报 env 错且 hint 列全名。
    #[test]
    fn get_hit_and_miss() {
        let p = get("phone16").unwrap();
        assert_eq!((p.panel_w, p.panel_h), (1080, 1920));
        let err = get("nope").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unknown profile 'nope'"), "msg={msg}");
        // hint 字段独立于 Display——CLI 用户唯一的发现面，必须列全六名。
        let hint = err.hint.expect("missing hint on unknown profile");
        for id in PROFILES.iter().map(|p| p.id) {
            assert!(hint.contains(id), "hint missing known profile {id}");
        }
    }
}
