//! BerryUI C1：结构性 Clip 容器的布局、DisplayList、离屏像素与命中语义。
//!
//! 反 mock 面：不仅断言 API/offset，还用 soft Queue + 哨兵像素证明 Clip 外
//! 零写入；命中与 paint 同一 Clip 链，不可见子树不可 Press。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, GlyphPos, Rect as GfxRect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_ui::damage::RectI;
use berry_ui::desc::{
    ClipProps, ColProps, GridProps, NodeId, PadProps, Props, RectProps, RowProps,
    ViewKind, ViewSpec,
};
use berry_ui::layout::CrossAlign;
use berry_ui::{Ui, UiMain};

const W: usize = 200;
const H: usize = 100;
const STRIDE: usize = W * 4;
const SENTINEL: u32 = 0xEEEE_EEEE;

const BLUE: Color = Color { r: 20, g: 40, b: 220 };
const GREEN: Color = Color { r: 30, g: 190, b: 80 };
const RED: Color = Color { r: 220, g: 40, b: 40 };

fn word(c: Color) -> u32 {
    TextureFormat::Rgbx.pack(c.r, c.g, c.b)
}

fn px(fb: &[u8], x: usize, y: usize) -> u32 {
    let off = y * STRIDE + x * 4;
    u32::from_le_bytes([fb[off], fb[off + 1], fb[off + 2], fb[off + 3]])
}

fn rect(bg: Color, w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg, w, h }),
    }
}

fn col(w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Col,
        key: 0,
        props: Props::Col(ColProps { gap: 0, w, h, cross: CrossAlign::Stretch }),
    }
}

/// Root → Col(100×80){ Clip → Blue(100×100), Green(100×20) }。
/// Clip port = 0..100 × 0..80；Blue 底部 20px 应被裁掉，右侧 100px 哨兵。
fn scene() -> (UiMain, NodeId, NodeId, NodeId) {
    let mut ui = UiMain::new(W as u32, H as u32);
    let root = ui.root();
    let stack = ui.add(root, col(Some(100), Some(80))).unwrap();
    let clip = ui
        .add(
            stack,
            ViewSpec { kind: ViewKind::Clip, key: 0, props: Props::Clip(ClipProps { w: None, h: Some(80) }) },
        )
        .unwrap();
    let blue = ui.add(clip, rect(BLUE, Some(100), Some(100))).unwrap();
    let green = ui.add(stack, rect(GREEN, Some(100), Some(20))).unwrap();
    ui.layout();
    (ui, clip, blue, green)
}

fn raster_ui(ui: &mut UiMain) -> Vec<u8> {
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scratch, &mut enc);
    let desc = TextureDesc::new(W, H, STRIDE, TextureFormat::Rgbx).unwrap();
    let mut fb = vec![0xEEu8; desc.need_bytes()];
    Queue::new()
        .submit(
            &enc.finish(),
            &mut fb,
            &desc,
            &PassDescriptor::new(LoadOp::Load, StoreOp::Store, None),
        )
        .unwrap();
    fb
}


#[test]
fn clip_container_layout_bounds_and_effective_clip() {
    let (ui, clip, blue, green) = scene();
    assert_eq!(ui.bounds(clip).unwrap(), RectI::new(0, 0, 100, 80));
    assert_eq!(ui.bounds(blue).unwrap(), RectI::new(0, 0, 100, 100), "bounds 保持未裁剪视觉界");
    assert_eq!(ui.bounds(green).unwrap(), RectI::new(0, 80, 100, 20));
    assert_eq!(ui.clip_of(blue).unwrap(), RectI::new(0, 0, 100, 80));
    assert_eq!(ui.clip_of(green).unwrap(), RectI::new(0, 0, 200, 100), "兄弟不受相邻 Clip 影响");
}

#[test]
fn clip_display_list_and_pixels_do_not_leak() {
    let (mut ui, _, _, _) = scene();
    ui.wake(0);
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scratch, &mut enc);
    let dl = enc.finish();
    {
        // 命令流：PushClip(100×80) → Blue → Pop → Green → Present。
        let commands: Vec<&Cmd> = dl.iter().collect();
        assert!(matches!(commands[0], Cmd::PushClip { rect } if *rect == GfxRect::new(0, 0, 100, 80)));
        assert!(matches!(commands[1], Cmd::FillRect { paint, .. } if paint.color == BLUE));
        assert!(matches!(commands[2], Cmd::PopClip));
        assert!(matches!(commands[3], Cmd::FillRect { paint, .. } if paint.color == GREEN));
        assert!(matches!(commands[4], Cmd::Present { .. }));
        assert_eq!(rep.cmds, 5);
    }
    drop(dl);
    assert_eq!(rep.drawn_px, 100 * 80 + 100 * 20, "overdraw 使用有效 clip 足迹");

    let fb = raster_ui(&mut ui);
    let blue = word(BLUE);
    let green = word(GREEN);
    for y in 0..H {
        for x in 0..W {
            let expected = if x < 100 && y < 80 {
                blue
            } else if x < 100 && y < 100 {
                green
            } else {
                SENTINEL
            };
            assert_eq!(px(&fb, x, y), expected, "@({x},{y})");
        }
    }
}

#[test]
fn hit_test_follows_same_clip_chain() {
    let (ui, clip, blue, green) = scene();
    assert_eq!(ui.node_at(50, 70), Some(blue), "Clip 内命中蓝叶");
    assert_eq!(ui.node_at(50, 90), Some(green), "蓝叶未裁剪区域落到兄弟绿叶");
    assert_eq!(ui.node_at(150, 40), None, "Clip 右侧不可命中");
    assert_eq!(ui.node_at(50, 40), Some(blue));
    let _ = clip;
}

#[test]
fn nested_clips_intersect_for_paint_and_hit() {
    let mut ui = UiMain::new(200, 100);
    let root = ui.root();
    let outer_holder = ui.add(root, col(Some(120), Some(100))).unwrap();
    let outer = ui
        .add(outer_holder, ViewSpec { kind: ViewKind::Clip, key: 0, props: Props::Clip(ClipProps { w: None, h: Some(100) }) })
        .unwrap();
    let inner_holder = ui.add(outer, col(Some(70), Some(100))).unwrap();
    let inner = ui
        .add(inner_holder, ViewSpec { kind: ViewKind::Clip, key: 0, props: Props::Clip(ClipProps { w: None, h: Some(100) }) })
        .unwrap();
    let red = ui
        .add(inner, rect(Color { r: 220, g: 40, b: 40 }, Some(200), Some(100)))
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(red).unwrap(), RectI::new(0, 0, 200, 100));
    assert_eq!(ui.bounds(inner).unwrap(), RectI::new(0, 0, 70, 100));
    assert_eq!(ui.clip_of(red).unwrap(), RectI::new(0, 0, 70, 100));
    assert_eq!(ui.node_at(50, 50), Some(red));
    assert_eq!(ui.node_at(90, 50), Some(outer), "内 port 外、外 port 内命中外层 Clip 容器");

    let fb = raster_ui(&mut ui);
    for y in 0..H {
        for x in 0..W {
            let expected = if x < 70 { word(RED) } else { SENTINEL };
            assert_eq!(px(&fb, x, y), expected, "@({x},{y})");
        }
    }
}


#[test]
fn offset_damage_folds_to_ancestor_clip() {
    let (mut ui, _, blue, _) = scene();
    ui.drain_damage();
    ui.set_offset(blue, 0, -20).unwrap();
    let damage = ui.damage();
    assert!(!damage.full);
    assert_eq!(damage.count, 1, "Clip 外的旧/新包络不放大 damage");
    assert_eq!(
        damage.rects[0],
        berry_guiproto::Rect { x: 0, y: 0, w: 100, h: 80 }
    );
}

#[test]
fn clip_ast_roundtrip_and_add_only_kind() {
    let (ui, _, blue, green) = scene();
    let mut stream = [0u8; 512];
    let len = ui.encode_desc(&mut stream).unwrap();

    // 手工树先取值级期望，再 apply 同一流重建（apply 会 reset arena）。
    let mut manual = Ui::<16>::new(W as u32, H as u32);
    let mroot = manual.root();
    let mstack = manual.add(mroot, col(Some(100), Some(80))).unwrap();
    let mclip = manual
        .add(mstack, ViewSpec { kind: ViewKind::Clip, key: 0, props: Props::Clip(ClipProps { w: None, h: Some(80) }) })
        .unwrap();
    let mblue = manual.add(mclip, rect(BLUE, Some(100), Some(100))).unwrap();
    let mgreen = manual.add(mstack, rect(GREEN, Some(100), Some(20))).unwrap();
    manual.layout();
    let expected = [
        (manual.bounds(mclip).unwrap(), manual.clip_of(mclip).unwrap()),
        (manual.bounds(mblue).unwrap(), manual.clip_of(mblue).unwrap()),
        (manual.bounds(mgreen).unwrap(), manual.clip_of(mgreen).unwrap()),
    ];
    let _ = blue;
    let _ = green;

    let mut rebuilt = Ui::<16>::new(W as u32, H as u32);
    assert_eq!(rebuilt.apply_desc(&stream[..len]), Ok(()));
    rebuilt.layout();
    // 前序第 3/4/5 节点 = Clip/Blue/Green（Root、外层 Col 后）。
    let seed = rebuilt.root();
    let mut dump = [
        berry_ui::diagnostic::LayoutDumpEntry {
            id: seed, parent: None, first_child: None, next_sibling: None,
            kind: ViewKind::Root, key: 0, depth: 0, x: 0, y: 0, w: 0, h: 0,
            bounds: RectI::new(0, 0, 0, 0), measured_w: 0, measured_h: 0,
            layout_dirty: false, paint_dirty: false,
        };
        8
    ];
    let dump_len = rebuilt.layout_dump(&mut dump).unwrap();
    let entries: Vec<_> = dump[..dump_len].to_vec();
    let got: Vec<_> = entries[2..5]
        .iter()
        .map(|e| (e.bounds, rebuilt.clip_of(e.id).unwrap()))
        .collect();
    assert_eq!(got.as_slice(), expected.as_slice());

    // DescBuilder 双入口：begin_clip + HAS_SIZE 块与 direct add 编码等价。
    let mut builder = berry_ui::desc::DescBuilder::new();
    builder.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    builder.begin_clip(7).unwrap();
    builder.size(Some(100), Some(80)).unwrap();
    builder.end().unwrap();
    builder.end().unwrap();
    let mut direct = Ui::<8>::new(W as u32, H as u32);
    let direct_root = direct.root();
    let direct_clip = direct
        .add(
            direct_root,
            ViewSpec {
                kind: ViewKind::Clip,
                key: 7,
                props: Props::Clip(ClipProps { w: Some(100), h: Some(80) }),
            },
        )
        .unwrap();
    direct.layout();
    let mut direct_stream = [0u8; 64];
    let direct_len = direct.encode_desc(&mut direct_stream).unwrap();
    assert_eq!(builder.bytes(), &direct_stream[..direct_len]);
    let _ = direct_clip;


    // add-only 未知 kind 拒面顺延到 44（12..=43 已并入 Circle/Panel/Clip）。
    let mut probe = Ui::<8>::new(10, 10);
    probe.root();
    assert_eq!(
        probe.apply_desc(&[44, 0]),
        Err(berry_ui::AstError::UnknownKind { got: 44 })
    );
}

#[test]
fn command_budget_never_leaves_dangling_clip() {
    let mut ui = Ui::<512>::new(512, 8);
    let root = ui.root();
    let clip = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Clip,
                key: 0,
                props: Props::Clip(ClipProps { w: Some(512), h: Some(8) }),
            },
        )
        .unwrap();
    let row = ui
        .add(
            clip,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: 0,
                    w: None,
                    h: None,
                    cross: CrossAlign::Stretch,
                }),
            },
        )
        .unwrap();
    for _ in 0..300 {
        ui.add(row, rect(RED, Some(1), Some(8))).unwrap();
    }
    ui.layout();
    ui.wake(0);
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scratch, &mut enc);
    let dl = enc.finish();
    assert_eq!(rep.cmds, berry_gfx::cmd::MAX_CMDS);

    let mut depth = 0usize;
    let mut balance_ok = true;
    for command in dl.iter() {
        match command {
            Cmd::PushClip { .. } => depth += 1,
            Cmd::PopClip => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth > 1 {
            balance_ok = false;
        }
    }
    assert_eq!(depth, 0, "命令满帽也必须保留 PopClip 槽位");
    assert!(balance_ok);

    let desc = TextureDesc::new(W, H, STRIDE, TextureFormat::Rgbx).unwrap();
    let mut fb = vec![0xEEu8; desc.need_bytes()];
    assert!(Queue::new()
        .submit(
            &dl,
            &mut fb,
            &desc,
            &PassDescriptor::new(LoadOp::Load, StoreOp::Store, None),
        )
        .is_ok());
}

#[test]
fn sparse_offset_damage_keeps_separate_painted_leaves() {
    let mut ui = Ui::<32>::new(200, 100);
    let root = ui.root();
    let port = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Clip,
                key: 0,
                props: Props::Clip(ClipProps { w: Some(200), h: Some(100) }),
            },
        )
        .unwrap();
    let track = ui
        .add(
            port,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: 60,
                    w: Some(200),
                    h: Some(100),
                    cross: CrossAlign::Center,
                }),
            },
        )
        .unwrap();
    let a = ui.add(track, rect(BLUE, Some(40), Some(20))).unwrap();
    let b = ui.add(track, rect(GREEN, Some(40), Some(20))).unwrap();
    ui.layout();
    ui.drain_damage();

    ui.set_offset_sparse(track, 10, 0).unwrap();
    let damage = ui.damage();
    assert!(!damage.full);
    assert_eq!(damage.count, 2, "两个不相邻 painted leaf 不提升 FULL");
    assert!(damage.union_px() < 20_000, "damage 必须显著小于 200×100 port");
    assert_ne!(a, b);
}

#[test]
fn sparse_grid_offset_damage_keeps_row_bands() {
    let mut ui = Ui::<64>::new(200, 100);
    let root = ui.root();
    let port = ui.add(root, ViewSpec {
        kind: ViewKind::Clip,
        key: 0,
        props: Props::Clip(ClipProps { w: Some(200), h: Some(100) }),
    }).unwrap();
    let track = ui.add(port, ViewSpec {
        kind: ViewKind::Row,
        key: 0,
        props: Props::Row(RowProps { gap: 0, w: Some(200), h: Some(100), cross: CrossAlign::Stretch }),
    }).unwrap();
    for _page in 0..2 {
        let page = ui.add(track, ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps { gap: 0, w: Some(100), h: Some(100), cross: CrossAlign::Center }),
        }).unwrap();
        let grid = ui.add(page, ViewSpec {
            kind: ViewKind::Grid,
            key: 0,
            props: Props::Grid(GridProps { cols: 2, rows: 2, gap: 20, cross: CrossAlign::Center, w: Some(100), h: Some(100) }),
        }).unwrap();
        for _cell in 0..4 {
            let pad = ui.add(grid, ViewSpec {
                kind: ViewKind::Pad,
                key: 0,
                props: Props::Pad(PadProps { all: 0 }),
            }).unwrap();
            ui.add(pad, rect(BLUE, Some(30), Some(15))).unwrap();
        }
    }
    ui.layout();
    ui.drain_damage();
    ui.set_offset_sparse(track, 10, 0).unwrap();
    let damage = ui.damage();
    assert!(!damage.full, "Grid 行带不得退化为 FULL");
    assert!(damage.count <= 4);
    assert!(damage.union_px() < 12_000, "行间空白仍应保持不传输");
}

/// 两可见页 × 5 行（launcher 5×4 档行数）的 Grid 槽板：port 200×250 +
/// track Row + 2 页 Col(100 宽) → Grid(1×5, gap 20) → Pad→Rect(80×30)。
/// 行位两两差 54（> 合并阈 2）——跨页同带合并的确定性几何面。
fn banded_two_page_grid() -> (Ui<32>, NodeId) {
    let mut ui = Ui::<32>::new(200, 250);
    let root = ui.root();
    let port = ui.add(root, ViewSpec {
        kind: ViewKind::Clip,
        key: 0,
        props: Props::Clip(ClipProps { w: Some(200), h: Some(250) }),
    }).unwrap();
    let track = ui.add(port, ViewSpec {
        kind: ViewKind::Row,
        key: 0,
        props: Props::Row(RowProps { gap: 0, w: Some(200), h: Some(250), cross: CrossAlign::Stretch }),
    }).unwrap();
    for _page in 0..2 {
        let page = ui.add(track, ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps { gap: 0, w: Some(100), h: Some(250), cross: CrossAlign::Center }),
        }).unwrap();
        let grid = ui.add(page, ViewSpec {
            kind: ViewKind::Grid,
            key: 0,
            props: Props::Grid(GridProps { cols: 1, rows: 5, gap: 20, cross: CrossAlign::Center, w: Some(100), h: Some(250) }),
        }).unwrap();
        for _cell in 0..5 {
            let pad = ui.add(grid, ViewSpec {
                kind: ViewKind::Pad,
                key: 0,
                props: Props::Pad(PadProps { all: 0 }),
            }).unwrap();
            ui.add(pad, rect(BLUE, Some(80), Some(30))).unwrap();
        }
    }
    ui.layout();
    ui.drain_damage();
    (ui, track)
}

/// 审计修 ②钉板：两页同带按 y±2 跨页合并成恰 5 条——修复前每页各自
/// 成带 add_exact 直挂，第 9 条即破 DAMAGE_MAX=8 退 FULL（整帧重绘，
/// 行间空白白传输）。
#[test]
fn sparse_grid_two_pages_five_rows_merge_cross_page() {
    let (mut ui, track) = banded_two_page_grid();
    ui.set_offset_sparse(track, 30, 0).unwrap();
    let damage = ui.damage();
    assert!(!damage.full, "两页×5 行不得破帽退 FULL");
    assert_eq!(damage.count, 5, "跨页同带按 y±2 合并 = 恰 5 条行带（修复前 10 带破帽 FULL）");
    // 合并后仍是 5 条独立带：y 两两差 > 2（同带判定阈）。
    let mut ys: Vec<u32> = damage.iter().map(|r| r.y).collect();
    ys.sort_unstable();
    assert!(
        ys.windows(2).all(|w| w[1] - w[0] > 2),
        "带 y 两两差须超合并阈：{ys:?}"
    );
    // 行间空白保持不传输：5 带 ≤ 5×190×30 = 28500 « 200×250 = 50000。
    assert!(damage.union_px() <= 28_500, "行间空白不传输，got {}", damage.union_px());
    assert!(damage.union_px() < 50_000, "显著小于整帧");
}

/// 审计修 ②钉板：同帧批内多次 MOVE = 多次 set_offset_sparse 累计进同
/// 一 damage 集（帧间不 drain）——y±2 合并后仍 5 带不破帽，且带集与
/// 单跳总位移（0→55）逐字节等价（并集覆盖无洞）。
#[test]
fn sparse_grid_repeated_offset_same_frame_stays_banded() {
    let (mut ui, track) = banded_two_page_grid();
    // 批内两 MOVE（一次 pump 分发两条 DragMove → 两次写）。
    ui.set_offset_sparse(track, 30, 0).unwrap();
    ui.set_offset_sparse(track, 55, 0).unwrap();
    let acc = ui.damage();
    assert!(!acc.full, "同帧多次 offset 累计不得破帽退 FULL");
    assert_eq!(acc.count, 5, "同带合并后仍恰 5 条");

    // 单跳 0→55 的参照树：两带集排序后逐字段相等。
    let (mut ui2, track2) = banded_two_page_grid();
    ui2.set_offset_sparse(track2, 55, 0).unwrap();
    let single = ui2.damage();
    assert!(!single.full);
    assert_eq!(single.count, 5);
    let norm = |d: &berry_ui::damage::DamageSet| {
        let mut v: Vec<(u32, u32, u32, u32)> =
            d.iter().map(|r| (r.x, r.y, r.w, r.h)).collect();
        v.sort_unstable();
        v
    };
    assert_eq!(norm(acc), norm(single), "0→30→55 累计带集 == 0→55 单跳带集");
}
