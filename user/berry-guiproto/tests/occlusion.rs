//! GUI R2 occlusion / buffer contract host tests：raw buffer 坐标、visible
//! 遮挡裁剪、insets 词表与 Flutter 形派生代数。

use berry_guiproto::authority::SurfaceGeometry;
use berry_guiproto::insets::{
    inset_kind_known, OcclusionSnapshot, INSET_KIND_IME, INSET_KIND_SYSTEM_BARS,
    INSET_KIND_SYSTEM_OVERLAYS, PRODUCIBLE_INSET_KINDS,
};
use berry_guiproto::Rect;

#[test]
fn r2_top_occlusion_preserves_raw_row_mapping() {
    let g = SurfaceGeometry::new((0, 0, 800, 600), (0, 30, 800, 570))
        .expect("valid top occlusion");
    assert_eq!(g.visible_local(), Rect { x: 0, y: 30, w: 800, h: 570 });
    assert_eq!(g.occlusion_insets(), (0, 30, 0, 0));

    let straddling = Rect { x: 0, y: 0, w: 800, h: 40 };
    assert_eq!(
        g.clip_damage_to_visible(straddling),
        Some(Rect { x: 0, y: 30, w: 800, h: 10 })
    );
    let under_bar = Rect { x: 0, y: 0, w: 800, h: 30 };
    assert_eq!(g.clip_damage_to_visible(under_bar), None);
    let outside_raw = Rect { x: 900, y: 10, w: 20, h: 20 };
    assert_eq!(g.clip_damage_to_visible(outside_raw), None);
}

#[test]
fn r2_left_occlusion_prevents_horizontal_skew() {
    let g = SurfaceGeometry::new((0, 0, 800, 600), (30, 0, 770, 600))
        .expect("valid left occlusion");
    assert_eq!(g.visible_local(), Rect { x: 30, y: 0, w: 770, h: 600 });
    assert_eq!(g.occlusion_insets(), (30, 0, 0, 0));
    assert_eq!(
        g.clip_damage_to_visible(Rect { x: 20, y: 0, w: 40, h: 1 }),
        Some(Rect { x: 30, y: 0, w: 30, h: 1 })
    );
    assert_eq!(
        g.clip_damage_to_visible(Rect { x: 0, y: 0, w: 30, h: 600 }),
        None
    );
}

#[test]
fn r2_geometry_rejects_non_subset_and_zero_visible() {
    assert!(SurfaceGeometry::new((10, 10, 100, 100), (0, 10, 100, 100)).is_none());
    assert!(SurfaceGeometry::new((10, 10, 100, 100), (10, 10, 101, 100)).is_none());
    assert!(SurfaceGeometry::new((10, 10, 100, 100), (10, 10, 0, 100)).is_none());
    assert!(SurfaceGeometry::new((10, 10, 100, 100), (10, 10, 100, 0)).is_none());
    assert!(SurfaceGeometry::new((10, 10, 0, 100), (10, 10, 0, 100)).is_none());
    assert!(SurfaceGeometry::new((10, 10, 100, 0), (10, 10, 100, 0)).is_none());
    assert!(SurfaceGeometry::new((10, 10, 100, 100), (10, 10, 100, 100)).is_some());
    // u16 原点+宽高可在 u32 域越过 0xFFFF；insets 推导不得在 u16 加法回绕。
    let edge = SurfaceGeometry::new(
        (60_000, 60_000, 60_000, 60_000),
        (60_000, 60_000, 30_000, 30_000),
    )
    .expect("u32 geometry domain");
    assert_eq!(edge.occlusion_insets(), (0, 0, 30_000, 30_000));
}

#[test]
fn r2_inset_vocabulary_is_add_only_and_producer_gated() {
    assert!(inset_kind_known(INSET_KIND_SYSTEM_BARS));
    assert!(inset_kind_known(INSET_KIND_IME));
    assert!(inset_kind_known(INSET_KIND_SYSTEM_OVERLAYS));
    assert!(!inset_kind_known(7));
    assert_eq!(PRODUCIBLE_INSET_KINDS, [INSET_KIND_SYSTEM_BARS]);
}

#[test]
fn r2_effective_padding_uses_flutter_algebra() {
    let snap = OcclusionSnapshot::new(
        [10, 30, 20, 0],
        [4, 40, 0, 12],
    );
    assert_eq!(snap.effective_padding(), [6, 0, 20, 0]);
}
