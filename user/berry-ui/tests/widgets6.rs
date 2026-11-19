//! 波2 批 E——Refresh/SwipeItem/DatePick/TimePick 渲染接线钉板：
//!
//! - **渲染**：Refresh 顶缘拉动指示条（offset permille → 高比例）、
//!   SwipeItem 两端动作缘带（reveal 比例）、DatePick YYYY-MM-DD 十字
//!   节框显、TimePick HH:MM 五字节框显。
//! - **布局**：DatePick 96×48 / TimePick 64×48 内在；Refresh 随父容器
//!   （指示条叠子之上）。
//! - **命令面**：set_props 四 kind 开口 + 值域拒面。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, DisplayList, GlyphPos};
use berry_ui::desc::{DatePickProps, Props, RefreshProps, SwipeItemProps, TimePickProps};
use berry_ui::theme::{PICK_BG, PICK_BORDER, REFRESH_BAR, SELECT_FG, SWIPE_ACTION_BG};
use berry_ui::{Ui, UiError, ViewKind, ViewSpec};

fn paint_full(ui: &mut Ui<64>) -> DisplayList<'static> {
    ui.drain_damage();
    ui.wake(0);
    let scr: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; 32].into_boxed_slice());
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

// ---------- ① Refresh 指示条 + SwipeItem 缘带 ----------

#[test]
fn refresh_bar_and_swipe_reveal_paint() {
    let mut ui = Ui::<64>::new(200, 100);
    let root = ui.root();
    let rf = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Refresh,
                key: 0,
                props: Props::Refresh(RefreshProps { refreshing: false, offset: 500, on_refresh: 3 }),
            },
        )
        .unwrap();
    let sw = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::SwipeItem,
                key: 0,
                props: Props::SwipeItem(SwipeItemProps {
                    action_count: 2,
                    reveal_start: 250,
                    reveal_end: 500,
                    on_action: 4,
                }),
            },
        )
        .unwrap();
    ui.layout();
    // Refresh 随父（Root 直子 = viewport）→ 指示条 = min(h,32)=32 × 50% = 16。
    let dl = paint_full(&mut ui);
    assert!(
        fills(&dl).contains(&(0, 0, 200, 16, REFRESH_BAR)),
        "offset 500‰ → 16px 指示条"
    );
    // SwipeItem 随父全幅：start 25% × min(w,96)=96 → 24px 左缘带；end 50% → 48px 右缘带。
    let f = fills(&dl);
    assert!(f.contains(&(0, 0, 24, 100, SWIPE_ACTION_BG)), "左缘带 24px：{f:?}");
    assert!(f.contains(&(152, 0, 48, 100, SWIPE_ACTION_BG)), "右缘带 48px：{f:?}");
    let _ = (rf, sw);
}

// ---------- ② DatePick/TimePick 框显 ----------

#[test]
fn datepick_timepick_paint_golden() {
    let mut ui = Ui::<64>::new(200, 200);
    let root = ui.root();
    let dp = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::DatePick,
                key: 0,
                props: Props::DatePick(DatePickProps {
                    year: 2026,
                    month: 11,
                    day: 15,
                    min_year: 2000,
                    max_year: 2100,
                    on_change: 7,
                }),
            },
        )
        .unwrap();
    let tp = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::TimePick,
                key: 0,
                props: Props::TimePick(TimePickProps { hour: 9, minute: 5, use_24h: true, on_change: 8 }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(dp).unwrap().w, 96);
    assert_eq!(ui.bounds(tp).unwrap().w, 64);
    let dl = paint_full(&mut ui);
    let g = glyphs(&dl);
    // "2026-11-15" 逐字节（框内居中）。
    let date_str: Vec<u8> = b"2026-11-15".to_vec();
    for &b in date_str.iter() {
        assert!(g.iter().any(|&(c, _, _)| c == b), "DatePick 字形 {b}{}：{g:?}", b as char);
    }
    let time_str: Vec<u8> = b"09:05".to_vec();
    for &b in time_str.iter() {
        assert!(g.iter().any(|&(c, _, _)| c == b), "TimePick 字形 {b}{}：{g:?}", b as char);
    }
    // 框底白 + 边灰。
    let f = fills(&dl);
    assert!(f.contains(&(0, 0, 96, 2, PICK_BORDER)));
    assert!(f.contains(&(2, 2, 92, 44, PICK_BG)));
}

// ---------- ③ 命令面开口 + 值域拒 ----------

#[test]
fn batch_e_set_props_and_rejects() {
    let mut ui = Ui::<64>::new(200, 200);
    let root = ui.root();
    let tp = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::TimePick,
                key: 0,
                props: Props::TimePick(TimePickProps { hour: 9, minute: 5, use_24h: true, on_change: 8 }),
            },
        )
        .unwrap();
    let dp = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::DatePick,
                key: 0,
                props: Props::DatePick(DatePickProps {
                    year: 2026,
                    month: 11,
                    day: 15,
                    min_year: 2000,
                    max_year: 2100,
                    on_change: 7,
                }),
            },
        )
        .unwrap();
    ui.layout();
    // 开口：set_props 生效（渲染随变 09:05 → 23:59）。
    ui.set_props(tp, Props::TimePick(TimePickProps { hour: 23, minute: 59, use_24h: true, on_change: 8 }))
        .unwrap();
    let dl = paint_full(&mut ui);
    let g = glyphs(&dl);
    assert!(
        g.iter().any(|&(c, _, _)| c == b'3'),
        "23:59 含 3：{g:?}"
    );
    // 拒面：hour 24 / minute 60。
    assert_eq!(
        ui.set_props(tp, Props::TimePick(TimePickProps { hour: 24, minute: 0, use_24h: true, on_change: 8 })),
        Err(UiError::NodeBad)
    );
    assert_eq!(
        ui.set_props(tp, Props::TimePick(TimePickProps { hour: 0, minute: 60, use_24h: true, on_change: 8 })),
        Err(UiError::NodeBad)
    );
    // DatePick 拒面：month 13 / day 0 / year 越界。
    assert_eq!(
        ui.set_props(
            dp,
            Props::DatePick(DatePickProps {
                year: 2026, month: 13, day: 15, min_year: 2000, max_year: 2100, on_change: 7
            })
        ),
        Err(UiError::NodeBad)
    );
    assert_eq!(
        ui.set_props(
            dp,
            Props::DatePick(DatePickProps {
                year: 2026, month: 11, day: 0, min_year: 2000, max_year: 2100, on_change: 7
            })
        ),
        Err(UiError::NodeBad)
    );
    assert_eq!(
        ui.set_props(
            dp,
            Props::DatePick(DatePickProps {
                year: 1999, month: 11, day: 15, min_year: 2000, max_year: 2100, on_change: 7
            })
        ),
        Err(UiError::NodeBad)
    );
    // Refresh offset 越千分域拒。
    let rf = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Refresh,
                key: 0,
                props: Props::Refresh(RefreshProps { refreshing: false, offset: 0, on_refresh: 0 }),
            },
        )
        .unwrap();
    assert_eq!(
        ui.set_props(rf, Props::Refresh(RefreshProps { refreshing: false, offset: 1001, on_refresh: 0 })),
        Err(UiError::NodeBad)
    );
}
