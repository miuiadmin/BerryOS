//! Shared navigation glyphs for guest apps.
//!
//! The pixels are original geometric constants (no external asset), sized for
//! the 24vp product navigation target at 3x. Keeping them in BerryUI avoids a
//! second source of truth for system-app back buttons while still routing the
//! actual pixels through each app's `IconSource`.

use crate::metrics::IconSource;

pub const NAV_ICON_BACK: u16 = 0x9001;
pub const NAV_ICON_BACK_ACTIVE: u16 = 0x9002;
pub const NAV_ICON_HOME: u16 = 0x9003;
pub const NAV_ICON_HOME_ACTIVE: u16 = 0x9004;
pub const NAV_ICON_TASK: u16 = 0x9005;
pub const NAV_ICON_TASK_ACTIVE: u16 = 0x9006;

const DIM: u32 = 72;
const PIXELS: usize = (DIM * DIM) as usize;

const NORMAL_INK: u32 = 0xFF1B2540;
const ACTIVE_INK: u32 = 0xFF0074E0;

const fn arrow(active: bool) -> [u32; PIXELS] {
    let ink = if active { ACTIVE_INK } else { NORMAL_INK };
    let mut out = [0u32; PIXELS];
    let mut i = 0usize;
    while i < PIXELS {
        let p = i as u32;
        let x = p % DIM;
        let y = p / DIM;
        let cy = 36i32;
        let dx = x as i32 - 18;
        let dy = y as i32 - cy;
        let on_stroke = (-dy - dx).abs() <= 5 || (dy - dx).abs() <= 5;
        let on_tail = y >= 30 && y < 42 && x >= 18 && x < 54;
        if on_stroke || on_tail {
            out[i] = ink;
        }
        i += 1;
    }
    out
}

const fn home(active: bool) -> [u32; PIXELS] {
    let ink = if active { ACTIVE_INK } else { NORMAL_INK };
    let mut out = [0u32; PIXELS];
    let mut i = 0usize;
    while i < PIXELS {
        let p = i as u32;
        let x = p % DIM;
        let y = p / DIM;
        let outer = {
            let dx = x as i32 - 36;
            let dy = y as i32 - 36;
            let d = dx * dx + dy * dy;
            d >= 324 && d <= 576
        };
        if outer {
            out[i] = ink;
        }
        i += 1;
    }
    out
}

const fn task(active: bool) -> [u32; PIXELS] {
    let ink = if active { ACTIVE_INK } else { NORMAL_INK };
    let mut out = [0u32; PIXELS];
    let mut i = 0usize;
    while i < PIXELS {
        let p = i as u32;
        let x = p % DIM;
        let y = p / DIM;
        let border = (x >= 18 && x < 54 && (y == 22 || y == 50))
            || (y >= 22 && y < 50 && (x == 18 || x == 53));
        if border {
            out[i] = ink;
        }
        i += 1;
    }
    out
}

static BACK: [u32; PIXELS] = arrow(false);
static BACK_ACTIVE: [u32; PIXELS] = arrow(true);
static HOME: [u32; PIXELS] = home(false);
static HOME_ACTIVE: [u32; PIXELS] = home(true);
static TASK: [u32; PIXELS] = task(false);
static TASK_ACTIVE: [u32; PIXELS] = task(true);

/// Original geometric navigation glyph source shared by system apps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavigationIconSource;

impl IconSource for NavigationIconSource {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        matches!(
            id,
            NAV_ICON_BACK
                | NAV_ICON_BACK_ACTIVE
                | NAV_ICON_HOME
                | NAV_ICON_HOME_ACTIVE
                | NAV_ICON_TASK
                | NAV_ICON_TASK_ACTIVE
        )
        .then_some((DIM, DIM))
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        match id {
            NAV_ICON_BACK => Some(&BACK),
            NAV_ICON_BACK_ACTIVE => Some(&BACK_ACTIVE),
            NAV_ICON_HOME => Some(&HOME),
            NAV_ICON_HOME_ACTIVE => Some(&HOME_ACTIVE),
            NAV_ICON_TASK => Some(&TASK),
            NAV_ICON_TASK_ACTIVE => Some(&TASK_ACTIVE),
            _ => None,
        }
    }

    fn smooth_scale(&self, _id: u16) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_source_has_paired_states() {
        let source = NavigationIconSource;
        for id in [
            NAV_ICON_BACK,
            NAV_ICON_BACK_ACTIVE,
            NAV_ICON_HOME,
            NAV_ICON_HOME_ACTIVE,
            NAV_ICON_TASK,
            NAV_ICON_TASK_ACTIVE,
        ] {
            assert_eq!(source.dims(id), Some((72, 72)));
            assert_eq!(source.pixels(id).map(<[u32]>::len), Some(72 * 72));
        }
        assert!(source.pixels(0x9007).is_none());
    }

    #[test]
    fn navigation_glyphs_are_nonempty_and_transparent_outside_strokes() {
        let source = NavigationIconSource;
        for id in [NAV_ICON_BACK, NAV_ICON_HOME, NAV_ICON_TASK] {
            let pixels = source.pixels(id).unwrap();
            assert!(pixels.iter().any(|&p| p != 0));
            assert!(pixels.iter().any(|&p| p == 0));
        }
    }
}
