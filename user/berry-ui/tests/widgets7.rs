//! 波2 批 F（末刀）——Gauge/Qr/DragReorder 渲染接线钉板：
//!
//! - **布局**：Gauge 64×64 表盘方格、Qr module_count×4 点阵方阵（21
//!   模块 = 84×84）、DragReorder 24×36 手柄列（内在单列）。
//! - **渲染**：Gauge 粗水平条（TRACK_OFF 底轨 + ACCENT 填充 ×value/
//!   1000）+ 百分值文本（750‰ → "75%"）；Qr 三角定位方块（7×7 方框
//!   套方框）+ 数据区确定性图案（同景两帧逐 fill 恒等）；DragReorder
//!   三条水平短杠（≡）。
//! - **交互**：DragReorder DragStart/DragMove → on_reorder（value =
//!   拖拽纵坐标——v1 记录位置）；空表手柄零操作。
//! - **命令面**：set_props 三 kind 开口（value/module_count/item_index
//!   面）+ 值域拒面（permille 域/QR 版本域/索引越项）。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, DisplayList, GlyphPos};
use berry_guiproto::{PHASE_DOWN, PHASE_MOVE};
use berry_ui::desc::{DragReorderProps, GaugeProps, GaugeStyle, Props, QrProps};
use berry_ui::hit::PtrEvent;
use berry_ui::theme::{ACCENT, ON_SURFACE, TRACK_OFF};
use berry_ui::{Ui, UiError, ViewKind, ViewSpec};

fn paint_full(ui: &mut Ui<64>) -> DisplayList<'static> {
    ui.drain_damage();
    ui.wake(0);
    let scr: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; 16].into_boxed_slice());
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(scr, &mut enc);
    assert!(rep.cmds > 0);
    enc.finish()
}

fn fills(dl: &DisplayList) -> Vec<(i32, i32, i32, i32, Color)> {
    dl.iter()
        .filter_map(|c| match c {
            Cmd::FillRect { rect, paint } => Some((rect.x, rect.y, rect.w, rect.h, paint.color)),
            _ => None,
        })
        .collect()
}

fn glyphs(dl: &DisplayList) -> Vec<(u8, i32, i32)> {
    let mut out = Vec::new();
    for c in dl.iter() {
        if let Cmd::DrawGlyphRun { run } = c {
            for g in run.glyphs {
                out.push((g.ch, g.x, g.y));
            }
        }
    }
    out
}

fn pump_one(ui: &mut Ui<64>, phase: u8, x: u32, y: u32, ts: u64) {
    ui.push_pointer(PtrEvent::from_wire(phase, 1, x, y, ts, 0, 0)).unwrap();
    ui.pump_input(0);
}

// ---------- ① Gauge 粗条 + 百分值文本 ----------

#[test]
fn gauge_layout_and_paint_golden() {
    let mut ui = Ui::<64>::new(200, 200);
    let root = ui.root();
    let gg = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Gauge,
                key: 0,
                props: Props::Gauge(GaugeProps { value: 750, style: GaugeStyle::Ring }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(gg).unwrap(), berry_ui::damage::RectI::new(0, 0, 64, 64));
    let dl = paint_full(&mut ui);
    let f = fills(&dl);
    // 上带粗条（16 厚——Progress 8px 轨的两倍）：底轨全宽 + 750‰ 填充 48。
    assert!(f.contains(&(0, 8, 64, 16, TRACK_OFF)), "底轨：{f:?}");
    assert!(f.contains(&(0, 8, 48, 16, ACCENT)), "750‰ → 48 填充：{f:?}");
    // 值文本 "75%"（下带双轴居中：x=(64-24)/2=20、y=32+(32-16)/2=40）。
    let g = glyphs(&dl);
    assert!(g.contains(&(b'7', 20, 40)), "百位 7：{g:?}");
    assert!(g.contains(&(b'5', 28, 40)), "十位 5：{g:?}");
    assert!(g.contains(&(b'%', 36, 40)), "百分号：{g:?}");
    // set_props 值随渲染（1000‰ → 满条 + "100%"）。
    ui.set_props(gg, Props::Gauge(GaugeProps { value: 1000, style: GaugeStyle::Bar }))
        .unwrap();
    let dl = paint_full(&mut ui);
    let f = fills(&dl);
    assert!(f.contains(&(0, 8, 64, 16, ACCENT)), "1000‰ 满条：{f:?}");
    let g = glyphs(&dl);
    assert!(g.contains(&(b'1', 16, 40)) && g.contains(&(b'0', 24, 40)), "100%：{g:?}");
    // 拒面：value 越千分域。
    assert_eq!(
        ui.set_props(gg, Props::Gauge(GaugeProps { value: 1001, style: GaugeStyle::Ring })),
        Err(UiError::NodeBad),
        "Gauge value > 1000 拒"
    );
}

// ---------- ② Qr 定位方块 + 确定性图案 ----------

#[test]
fn qr_paint_golden_and_deterministic() {
    let mut ui = Ui::<64>::new(120, 120);
    let root = ui.root();
    let qr = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Qr,
                key: 0,
                props: Props::Qr(QrProps {
                    module_count: 21,
                    dark_color: 0x00_00_00,
                    light_color: 0xFF_FF_FF,
                }),
            },
        )
        .unwrap();
    ui.layout();
    // 21 模块 × 4px = 84×84 点阵方阵。
    assert_eq!(ui.bounds(qr).unwrap(), berry_ui::damage::RectI::new(0, 0, 84, 84));
    let black = Color { r: 0, g: 0, b: 0 };
    let white = Color { r: 0xFF, g: 0xFF, b: 0xFF };
    let dl = paint_full(&mut ui);
    let f = fills(&dl);
    // 亮色全底（1 fill 铺底）。
    assert!(f.contains(&(0, 0, 84, 84, white)), "亮色底：{f:?}");
    // 三角定位方块（TL/TR/BL 各 7×7）：外环角模块暗（4×4/模块）。
    assert!(f.contains(&(0, 0, 4, 4, black)), "TL 外环角：{f:?}");
    assert!(f.contains(&(80, 0, 4, 4, black)), "TR 外环角（col 20 → x80）：{f:?}");
    assert!(f.contains(&(0, 80, 4, 4, black)), "BL 外环角（row 20 → y80）：{f:?}");
    // 方框套方框：TL 3×3 暗心（模块 (3,3) → 像素 (12,12)）。
    assert!(f.contains(&(12, 12, 4, 4, black)), "TL 暗心：{f:?}");
    // 定位方块暗模块下限 = 3×33（外环 24 + 暗心 9）；数据区确定性图案
    // 至少再添一枚暗 fill（> 下限即数据面非空）。
    let darks = f.iter().filter(|&&(_, _, _, _, c)| c == black).count();
    assert!(darks > 3 * 33, "数据区暗模块非空（darks={darks}）");
    // 确定性：同景两帧逐 fill 恒等（module_count 种子——golden 可对拍）。
    let dl2 = paint_full(&mut ui);
    assert_eq!(fills(&dl), fills(&dl2), "两帧图案恒等");
    // 拒面：module_count 不在 QR 版本域（20 < 21 / 23 非 4 步长）。
    assert_eq!(
        ui.set_props(
            qr,
            Props::Qr(QrProps { module_count: 20, dark_color: 0, light_color: 0xFFFFFF })
        ),
        Err(UiError::NodeBad),
        "module_count 20 拒（< 21）"
    );
    assert_eq!(
        ui.set_props(
            qr,
            Props::Qr(QrProps { module_count: 23, dark_color: 0, light_color: 0xFFFFFF })
        ),
        Err(UiError::NodeBad),
        "module_count 23 拒（非 4 步长）"
    );
}

// ---------- ③ DragReorder 三杠 + 拖拽派发 ----------

#[test]
fn dragreorder_paint_and_drag_events() {
    let mut ui = Ui::<64>::new(120, 120);
    let root = ui.root();
    let dr = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::DragReorder,
                key: 0,
                props: Props::DragReorder(DragReorderProps {
                    item_index: 1,
                    total_items: 4,
                    on_reorder: 9,
                }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(dr).unwrap(), berry_ui::damage::RectI::new(0, 0, 24, 36));
    let dl = paint_full(&mut ui);
    let f = fills(&dl);
    // 三条水平短杠（≡）：12×2、杠厚即间距 → y 13/17/21、x 居中 6。
    assert!(f.contains(&(6, 13, 12, 2, ON_SURFACE)), "杠 1：{f:?}");
    assert!(f.contains(&(6, 17, 12, 2, ON_SURFACE)), "杠 2：{f:?}");
    assert!(f.contains(&(6, 21, 12, 2, ON_SURFACE)), "杠 3：{f:?}");
    // 拖拽：DOWN(12,18) → MOVE(12,30) 越 slop 8 → DragStart（value=30）；
    // 再 MOVE(12,40) → DragMove（value=40）——on_reorder 逐样本记录 y。
    pump_one(&mut ui, PHASE_DOWN, 12, 18, 1_000_000);
    pump_one(&mut ui, PHASE_MOVE, 12, 30, 2_000_000);
    pump_one(&mut ui, PHASE_MOVE, 12, 40, 3_000_000);
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().map(|e| (e.event, e.value)).collect::<Vec<_>>(),
        vec![(9, 30), (9, 40)],
        "on_reorder = 拖拽纵坐标逐样本"
    );
    // 空表手柄：零操作（Select 零项同律）。
    ui.set_props(
        dr,
        Props::DragReorder(DragReorderProps { item_index: 0, total_items: 0, on_reorder: 9 }),
    )
    .unwrap();
    pump_one(&mut ui, PHASE_DOWN, 12, 18, 4_000_000);
    pump_one(&mut ui, PHASE_MOVE, 12, 40, 5_000_000);
    assert!(ui.take_events().is_empty(), "空表拖拽零派发");
    // 拒面：item_index 越项。
    assert_eq!(
        ui.set_props(
            dr,
            Props::DragReorder(DragReorderProps { item_index: 4, total_items: 4, on_reorder: 9 })
        ),
        Err(UiError::NodeBad),
        "item_index >= total_items 拒"
    );
    // add 面同律。
    assert_eq!(
        ui.add(
            root,
            ViewSpec {
                kind: ViewKind::DragReorder,
                key: 0,
                props: Props::DragReorder(DragReorderProps {
                    item_index: 5,
                    total_items: 4,
                    on_reorder: 0,
                }),
            },
        ),
        Err(UiError::NodeBad),
        "add 面越项拒"
    );
}

// ---------- ④ 命令面开口：Gauge/Qr set_props 生效（渲染随变） ----------

#[test]
fn batch_f_set_props_reflects_paint() {
    let mut ui = Ui::<64>::new(200, 200);
    let root = ui.root();
    let qr = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Qr,
                key: 0,
                props: Props::Qr(QrProps {
                    module_count: 21,
                    dark_color: 0x00_00_00,
                    light_color: 0xFF_FF_FF,
                }),
            },
        )
        .unwrap();
    ui.layout();
    // module_count 21 → 25：内在 84 → 100（几何面随 props 变）。
    ui.set_props(
        qr,
        Props::Qr(QrProps { module_count: 25, dark_color: 0, light_color: 0xFFFFFF }),
    )
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(qr).unwrap().w, 100, "25 模块 → 100 宽");
    let white = Color { r: 0xFF, g: 0xFF, b: 0xFF };
    assert!(fills(&paint_full(&mut ui)).contains(&(0, 0, 100, 100, white)), "亮底随界");
    // 纯绘面：dark_color 换色不改几何。
    ui.set_props(
        qr,
        Props::Qr(QrProps { module_count: 25, dark_color: 0x19_76_D2, light_color: 0xFFFFFF }),
    )
    .unwrap();
    let st = ui.layout();
    assert_eq!(st.measured, 0, "纯绘值变零重测（Measure 保留）");
    assert_eq!(ui.bounds(qr).unwrap().w, 100, "几何不变");
}
