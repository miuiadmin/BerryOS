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
        id: "desktop",
        panel_w: 540,
        panel_h: 960,
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
