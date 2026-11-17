//! syspages —— 系统页两件（波3-件⑤c：应用列表页 + 设置页）。
//!
//! 职责：TabBar 三页的后两页建树与交互值面（桌面页 = content 既有
//! SwipePager 四页原样，归 desktop.rs；本模块只管 sys 页）。
//!
//! - **应用列表页（List+ListItem+Search）**：数据 = svcmgr 注册表真流
//!   （`icons::registry()`——boot 期 APP_LIST 拉取的同一镜像，零第二
//!   数据源）。结构 = 页 Stack（不透明底 Rect 盖住桌面 + 内容列）→
//!   标题 / Search 行（Input 件）→ 固定过滤词表行 → List 容器
//!   （`ViewKind::List`，item_count/item_height 声明面）内 Card 行
//!   （白底 = ListItem 语义，pattern.rs `LISTITEM_H` 几何单源——本树
//!   无圆角原语，Card 直角形态如实）。行点击 → 与桌面格同一 spawn 路。
//! - **Search 诚实降级（3b 暂缓面）**：输入链（softimed 软键盘升起 +
//!   TextEvent 回显）挂 3b 批——v1 = **固定过滤词表**（ALL / A-F /
//!   G-M / N-Z 首字母桶，ASCII id 词法域的确定性四分）。Input 件在场
//!   （placeholder + 焦点环），键入不生效，如实注记。
//! - **设置页（分组 Toggle/Radio/Seg/Slider）**：Boot 组 Toggle（真落
//!   `BOOT.JSN`——`settings_io` canonical 单簇原子写，宿主读回复用
//!   tools/gui8-smoke.py FAT12 先例）；Display 组 Radio×2（theme 互斥
//!   ——引擎只置位不清位，组互斥执法在本模块）+ Seg（anim 两段）；
//!   Sound 组 Slider（volume permille——引擎 DragStart/DragMove 行程
//!   跟手映射已接线；纯点击（无拖动）v1 = +100 permille 步进 wrap，
//!   如实注记）；About 组静态行。控件值变更即整文档写 `SETTINGS.JSN`。
//! - **SA 合入后切换**：本模块两页建树与 desktop.rs 建树底座同构
//!   （berry-appshell `scene`/`conf` 抽取面）——appshell 落地后建树
//!   迁 `appshell::scene` 组合糖、持久键迁 `appshell::conf`，本模块
//!   退役为页特化层（mandate 裁决：先 launcher 内联实现）。
//!
//! 视觉纪律：文案 ASCII；行高 48vp 触控下限（big @3x=144px / 紧凑
//! @1x=48px 双口径——desktop.rs 双档同律）；零 alloc 建树（定容数组）。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_ui::desc::{
    CardProps, ColProps, IconProps, InputProps, InputKind, ListProps, PadProps, Props,
    RadioProps, RectProps, RowProps, SegProps, SliderProps, StackAlign, StackProps, TextBuf,
    TextProps, TextStyle, ToggleProps, ViewKind, ViewSpec,
};
use berry_ui::layout::CrossAlign;
use berry_ui::pattern::{self, TabSlot};
use berry_ui::{NodeId, UiError, UiMain, ROLE_BUTTON, ROLE_TOGGLE};

use crate::desktop::ROLE_LIST;
use crate::icons;
use crate::settings_io::{BootMode, Settings};

/// 页底色（盖住桌面用不透明底——CLEAR_BG 同族深蓝灰）。
const PAGE_BG: Color = Color { r: 0x10, g: 0x18, b: 0x28 };
/// 正文白（状态栏 TEXT_FG 同族）。
const FG: Color = Color { r: 0xF0, g: 0xF0, b: 0xF4 };
/// 组头/次级灰。
const GRAY: Color = Color { r: 0x99, g: 0x99, b: 0x99 };
/// brand 蓝（pattern 底条选中染色同值）。
const BRAND: Color = Color { r: 0x00, g: 0x7D, b: 0xFF };
/// 过滤词未选底。
const CHIP_BG: Color = Color { r: 0x2A, g: 0x31, b: 0x3D };
/// Card 白底（paint CARD_BG 同值——ListItem normal 态）。
const CARD_BG: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };

/// 行数帽（List 视口容量上界——双档按高度预算裁到帽内）。
pub const APP_ROWS_MAX: usize = 8;
/// 固定过滤词表（Search 3b 降级面——ASCII id 首字母四分桶）。
pub const FILTER_WORDS: [&[u8]; 4] = [b"ALL", b"A-F", b"G-M", b"N-Z"];
/// 全部通过滤位。
pub const FILTER_ALL: u8 = 0;

/// 组件事件 ID（props on_change 携载——frameloop take_events 消费面）。
pub const EV_BOOT: u32 = 1;
pub const EV_THEME_LIGHT: u32 = 2;
pub const EV_THEME_DARK: u32 = 3;
pub const EV_ANIM: u32 = 4;
pub const EV_VOL: u32 = 5;

/// 过滤词 → 注册表 id 首字节匹配（纯函数；域外 filter 值宽容 = ALL）。
pub fn filter_matches(filter: u8, id_first: u8) -> bool {
    match filter {
        1 => id_first.is_ascii_uppercase() && id_first <= b'F',
        2 => id_first >= b'G' && id_first <= b'M',
        3 => id_first >= b'N' && id_first <= b'Z',
        _ => true,
    }
}

/// 文本叶 spec（desktop.text_spec 同形本地面——跨模块私有不外借）。
fn text_spec(text: &[u8], fg: Color, scale: FontSize) -> Result<ViewSpec, UiError> {
    Ok(ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg, bg: None, scale },
            text: TextBuf::new(text)?,
        }),
    })
}

/// 带底色文本叶 spec（过滤词片——Text bg 承载底色，零额外节点）。
fn chip_spec(text: &[u8], fg: Color, bg: Color, scale: FontSize) -> Result<ViewSpec, UiError> {
    Ok(ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg, bg: Some(bg), scale },
            text: TextBuf::new(text)?,
        }),
    })
}

/// 定高垫片（Col 流隙——desktop add_status_spacer 同形）。
fn spacer(ui: &mut UiMain, parent: NodeId, h: u32) -> Result<NodeId, UiError> {
    ui.add(
        parent,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(1),
                h: Some(h),
                cross: CrossAlign::Center,
            }),
        },
    )
}

/// 页骨架：Stack（不透明底 Rect + 内容列）——底盖桌面、内容列流排。
fn page_frame(
    ui: &mut UiMain,
    host: NodeId,
    w: u32,
    h: u32,
) -> Result<(NodeId, NodeId), UiError> {
    let page = ui.add(
        host,
        ViewSpec {
            kind: ViewKind::Stack,
            key: 0,
            props: Props::Stack(StackProps { align: StackAlign::Start }),
        },
    )?;
    ui.add(
        page,
        ViewSpec {
            kind: ViewKind::Rect,
            key: 0,
            props: Props::Rect(RectProps { bg: PAGE_BG, w: Some(w), h: Some(h), radius: 0 }),
        },
    )?;
    let content = ui.add(
        page,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(w),
                h: Some(h),
                cross: CrossAlign::Stretch,
            }),
        },
    )?;
    Ok((page, content))
}

/// 应用列表行句柄（命中映射面——Card/Row/Pad/Text 四节点皆归本行）。
#[derive(Clone, Copy)]
pub struct AppRow {
    pub card: NodeId,
    pub row: NodeId,
    pub pad: NodeId,
    pub text: NodeId,
    /// 注册表索引（None = 空行——过滤后不足帽位的留白占位）。
    pub app: Option<usize>,
}

/// 应用列表页句柄。
pub struct AppPage {
    /// 页根（Stack——desktop.sys 持有以整树摘除）。
    pub node: NodeId,
    /// List 容器（a11y ROLE_LIST 锚）。
    pub list: NodeId,
    /// 过滤词节点（选中态换底——Text bg 双色）。
    pub words: [NodeId; 4],
    /// 行表（前 `rows_n` 激活）。
    pub rows: [AppRow; APP_ROWS_MAX],
    pub rows_n: usize,
}

/// ListItem 行高（big = pattern::LISTITEM_H @3x；紧凑 = 48px @1x 触控
/// 下限）与行距（big 24 / 紧凑 8）。
fn item_geometry(big: bool) -> (u32, u32) {
    if big { (pattern::LISTITEM_H, 24) } else { (48, 8) }
}

/// 建应用列表页（`host` = pages Stack；`w/h` = 页几何）。
pub fn build_apps(
    ui: &mut UiMain,
    host: NodeId,
    w: u32,
    h: u32,
    big: bool,
    filter: u8,
) -> Result<AppPage, UiError> {
    let (page, content) = page_frame(ui, host, w, h)?;
    let (item_h, item_gap) = item_geometry(big);
    let title = if big { FontSize::S32x48 } else { FontSize::S16x32 };
    let body = if big { FontSize::S24x48 } else { FontSize::S16x32 };
    let small = if big { FontSize::S16x32 } else { FontSize::S8x16 };
    let pad = if big { 36 } else { 12 };
    let gap = if big { 24 } else { 8 };

    spacer(ui, content, pad)?;
    ui.add(content, text_spec(b"Apps", FG, title)?)?;
    spacer(ui, content, gap)?;

    // Search 行：Input 件（placeholder + 焦点环）+ 提示（3b 降级注记面
    // ——文案如实：「fixed filters」）。
    let search_row = ui.add(
        content,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap,
                w: None,
                h: None,
                cross: CrossAlign::Center,
            }),
        },
    )?;
    ui.add(
        search_row,
        ViewSpec {
            kind: ViewKind::Input,
            key: 0,
            props: Props::Input(InputProps {
                placeholder: TextBuf::new(b"search apps")?,
                max_len: 32,
                input_kind: InputKind::Text,
            }),
        },
    )?;
    ui.add(search_row, text_spec(b"fixed filters", GRAY, small)?)?;
    spacer(ui, content, gap)?;

    // 固定过滤词表（Search 3b 降级）：选中 = brand 底白字；未选 = 深底
    // 灰字。Text bg 承载——运行期换选 = set_props 换底色。
    let words_row = ui.add(
        content,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap,
                w: None,
                h: None,
                cross: CrossAlign::Center,
            }),
        },
    )?;
    let mut words = [host; 4]; // 哑元占位（NodeId 无公开构造）——循环内全量覆写
    for (i, wd) in FILTER_WORDS.iter().enumerate() {
        let sel = i as u8 == filter;
        let (fg, bg) = if sel { (CARD_BG, BRAND) } else { (FG, CHIP_BG) };
        words[i] = ui.add(words_row, chip_spec(wd, fg, bg, small)?)?;
        ui.set_a11y(words[i], ROLE_BUTTON, wd)?;
    }
    spacer(ui, content, gap)?;

    // 行数预算：List 前固定高（pad + 标题 + 三隙 + Search 行 + 过滤词
    // 行）之外全给 List；行节拍 = 行高 + 行距。帽 8、至少 1 行（极端
    // 窄高防御）。紧凑 800×600：448-132=316 → 5 行；big：1479-236=1243
    // → 7 行（List 无滚动 v1——容量外条目走过滤词收窄，如实注记）。
    let fixed = pad
        + if big { 48 } else { 32 }
        + 3 * gap
        + 48
        + if big { 32 } else { 16 };
    let budget = h.saturating_sub(fixed);
    let pitch = item_h + item_gap;
    let rows_n = ((budget / pitch.max(1)) as usize).clamp(1, APP_ROWS_MAX);

    // List 容器（声明面 item_count×item_height=pitch——行距并入节拍，
    // 视口容量单源）> Col（行流排）。
    let list = ui.add(
        content,
        ViewSpec {
            kind: ViewKind::List,
            key: 0,
            props: Props::List(ListProps { item_count: rows_n as u32, item_height: pitch }),
        },
    )?;
    let col = ui.add(
        list,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: item_gap,
                w: None,
                h: None,
                cross: CrossAlign::Stretch,
            }),
        },
    )?;
    ui.set_a11y(list, ROLE_LIST, b"apps")?;

    // 哑元占位（NodeId 无公开构造——前 rows_n 行循环内全量覆写，帽外
    // 行不读）。
    let mut rows =
        [AppRow { card: host, row: host, pad: host, text: host, app: None }; APP_ROWS_MAX];
    for r in rows.iter_mut().take(rows_n) {
        // 行 = Card（白底 ListItem normal 态）> Row > Pad(16vp) > label。
        let card = ui.add(
            col,
            ViewSpec {
                kind: ViewKind::Card,
                key: 0,
                props: Props::Card(CardProps { elevation: 0, corner_radius: 0 }),
            },
        )?;
        let row = ui.add(
            card,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: 0,
                    w: None,
                    h: Some(item_h),
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let inner = if big { pattern::LISTITEM_PAD_X } else { 16 };
        let padding = ui.add(
            row,
            ViewSpec {
                kind: ViewKind::Pad,
                key: 0,
                props: Props::Pad(PadProps { all: inner }),
            },
        )?;
        let text = ui.add(padding, text_spec(b"", FG, body)?)?;
        *r = AppRow { card, row, pad: padding, text, app: None };
    }
    let mut page = AppPage { node: page, list, words, rows, rows_n };
    apply_filter(ui, &mut page, filter)?;
    Ok(page)
}

/// 过滤词换选视觉（Text bg 双色翻转——set_props 全换）。
fn word_face(ui: &mut UiMain, node: NodeId, word: &[u8], selected: bool, small: FontSize) {
    let (fg, bg) = if selected { (CARD_BG, BRAND) } else { (FG, CHIP_BG) };
    let text = match TextBuf::new(word) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("launcher: filter face text err={e:?}");
            return;
        }
    };
    let props = Props::Text(TextProps {
        style: TextStyle { fg, bg: Some(bg), scale: small },
        text,
    });
    if let Err(e) = ui.set_props(node, props) {
        eprintln!("launcher: filter face err={e:?}");
    }
}

/// 应用过滤：注册表序扫匹配项装前 `rows_n` 行（set_text 换文 + app 位
/// 回填）；不足帽位的尾行留空（短列表留白如实——行序稳定优先，不做
/// 折叠动画面）。幂等；空 label 文本零宽不占视觉面。
pub fn apply_filter(ui: &mut UiMain, page: &mut AppPage, filter: u8) -> Result<(), UiError> {
    let reg = icons::registry();
    let mut pick = 0usize;
    for r in page.rows.iter_mut().take(page.rows_n) {
        let mut found = None;
        while pick < reg.len() {
            let first = *reg[pick].id_bytes().first().unwrap_or(&b'A');
            if filter_matches(filter, first) {
                found = Some(pick);
                pick += 1;
                break;
            }
            pick += 1;
        }
        let (app, label): (Option<usize>, &[u8]) = match found {
            Some(k) => (Some(k), reg[k].label_bytes()),
            None => (None, b""),
        };
        r.app = app;
        ui.set_text(r.text, label)?;
    }
    Ok(())
}

/// 过滤词切换（frameloop 消费面）：换选视觉 + 重装行（a11y/press 由
/// 调用方 rehang）。
pub fn set_filter(
    ui: &mut UiMain,
    page: &mut AppPage,
    filter: u8,
    big: bool,
) -> Result<(), UiError> {
    let small = if big { FontSize::S16x32 } else { FontSize::S8x16 };
    for (i, &wn) in page.words.iter().enumerate() {
        word_face(ui, wn, FILTER_WORDS[i], i as u8 == filter, small);
    }
    apply_filter(ui, page, filter)
}

/// 命中节点 → 行序（Card/Row/Pad/Text 四面皆归）。
pub fn row_index_of(page: &AppPage, node: NodeId) -> Option<usize> {
    page.rows
        .iter()
        .take(page.rows_n)
        .position(|r| r.card == node || r.row == node || r.pad == node || r.text == node)
}

/// 设置页句柄。
pub struct SettingsPage {
    pub node: NodeId,
    pub boot_toggle: NodeId,
    pub theme_light: NodeId,
    pub theme_dark: NodeId,
    pub anim_seg: NodeId,
    pub vol_slider: NodeId,
    /// 行标签（press_map visual 面——控件不在 visual 白名单，反馈落标签）。
    pub boot_label: NodeId,
    pub theme_label: NodeId,
    pub anim_label: NodeId,
    pub vol_label: NodeId,
}

/// 组头装配（组名 + 前后隙）。
fn group_header(
    ui: &mut UiMain,
    content: NodeId,
    name: &[u8],
    gap: u32,
    row_gap: u32,
    small: FontSize,
) -> Result<(), UiError> {
    spacer(ui, content, row_gap)?;
    ui.add(content, text_spec(name, GRAY, small)?)?;
    spacer(ui, content, gap)?;
    Ok(())
}

/// 控件行装配（定高 Row + 前置标签；返回 (row, label)——label 供 press
/// visual / a11y 双用）。
fn ctrl_row(
    ui: &mut UiMain,
    content: NodeId,
    label: &[u8],
    item_h: u32,
    row_gap: u32,
    body: FontSize,
) -> Result<(NodeId, NodeId), UiError> {
    let row = ui.add(
        content,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: row_gap,
                w: None,
                h: Some(item_h),
                cross: CrossAlign::Center,
            }),
        },
    )?;
    let lab = ui.add(row, text_spec(label, FG, body)?)?;
    Ok((row, lab))
}

/// 建设置页。
pub fn build_settings(
    ui: &mut UiMain,
    host: NodeId,
    w: u32,
    h: u32,
    big: bool,
    boot: BootMode,
    settings: &Settings,
) -> Result<SettingsPage, UiError> {
    let (page, content) = page_frame(ui, host, w, h)?;
    let (item_h, _gap) = item_geometry(big);
    let title = if big { FontSize::S32x48 } else { FontSize::S16x32 };
    let body = if big { FontSize::S24x48 } else { FontSize::S16x32 };
    let small = if big { FontSize::S16x32 } else { FontSize::S8x16 };
    let pad = if big { 36 } else { 12 };
    let gap = if big { 24 } else { 8 };
    let row_gap = if big { 48 } else { 16 };

    spacer(ui, content, pad)?;
    ui.add(content, text_spec(b"Settings", FG, title)?)?;

    // —— Boot 组：verbose 开关（真落 BOOT.JSN）——
    group_header(ui, content, b"Boot", gap, row_gap, small)?;
    let (r, boot_label) = ctrl_row(ui, content, b"Verbose boot", item_h, row_gap, body)?;
    let boot_toggle = ui.add(
        r,
        ViewSpec {
            kind: ViewKind::Toggle,
            key: 0,
            props: Props::Toggle(ToggleProps {
                checked: boot == BootMode::Verbose,
                on_change: EV_BOOT,
            }),
        },
    )?;
    ui.set_a11y(boot_toggle, ROLE_TOGGLE, b"verbose boot")?;

    // —— Display 组：theme 二选一（Radio 组互斥执法在本模块）+ anim 段 ——
    group_header(ui, content, b"Display", gap, row_gap, small)?;
    let (r, theme_label) = ctrl_row(ui, content, b"Theme", item_h, row_gap, body)?;
    let theme_light = ui.add(
        r,
        ViewSpec {
            kind: ViewKind::Radio,
            key: 0,
            props: Props::Radio(RadioProps {
                selected: !settings.dark,
                group: 1,
                on_change: EV_THEME_LIGHT,
            }),
        },
    )?;
    ui.add(r, text_spec(b"Light", GRAY, small)?)?;
    let theme_dark = ui.add(
        r,
        ViewSpec {
            kind: ViewKind::Radio,
            key: 0,
            props: Props::Radio(RadioProps {
                selected: settings.dark,
                group: 1,
                on_change: EV_THEME_DARK,
            }),
        },
    )?;
    ui.add(r, text_spec(b"Dark", GRAY, small)?)?;
    ui.set_a11y(theme_light, ROLE_BUTTON, b"theme light")?;
    ui.set_a11y(theme_dark, ROLE_BUTTON, b"theme dark")?;

    let (r, anim_label) = ctrl_row(ui, content, b"Animation", item_h, row_gap, body)?;
    let anim_seg = ui.add(
        r,
        ViewSpec {
            kind: ViewKind::Seg,
            key: 0,
            props: Props::Seg(SegProps {
                segment_count: 2,
                active: if settings.anim { 1 } else { 0 },
                on_change: EV_ANIM,
            }),
        },
    )?;
    ui.set_a11y(anim_seg, ROLE_BUTTON, b"animation")?;

    // —— Sound 组：volume 滑条（permille 域；纯点击 = 步进，drag 已接线）——
    group_header(ui, content, b"Sound", gap, row_gap, small)?;
    let (r, vol_label) = ctrl_row(ui, content, b"Volume", item_h, row_gap, body)?;
    let vol_slider = ui.add(
        r,
        ViewSpec {
            kind: ViewKind::Slider,
            key: 0,
            props: Props::Slider(SliderProps {
                value: settings.volume.min(1000),
                min: 0,
                max: 1000,
                on_change: EV_VOL,
            }),
        },
    )?;
    ui.set_a11y(vol_slider, ROLE_BUTTON, b"volume")?;

    // —— About 组（静态行——真值消费面挂后续波）——
    group_header(ui, content, b"About", gap, row_gap, small)?;
    ui.add(content, text_spec(b"BerryOS v0.2.0", GRAY, small)?)?;
    ui.add(content, text_spec(b"launcher wave3 syspages", GRAY, small)?)?;

    Ok(SettingsPage {
        node: page,
        boot_toggle,
        theme_light,
        theme_dark,
        anim_seg,
        vol_slider,
        boot_label,
        theme_label,
        anim_label,
        vol_label,
    })
}

/// Radio 组互斥执法（引擎只置位——清位归本面）。
pub fn enforce_theme_pair(ui: &mut UiMain, page: &SettingsPage, dark: bool) {
    let mk = |sel: bool, ev: u32| Props::Radio(RadioProps { selected: sel, group: 1, on_change: ev });
    let _ = ui.set_props(page.theme_light, mk(!dark, EV_THEME_LIGHT));
    let _ = ui.set_props(page.theme_dark, mk(dark, EV_THEME_DARK));
}

/// Slider 纯点击步进（+100 permille wrap——Drag 路引擎已接，本面补
/// 无拖动点击；frameloop Msg::Click 消费）。返回新值。
pub fn step_slider(ui: &mut UiMain, page: &SettingsPage, cur: u32) -> u32 {
    let next = if cur >= 1000 { 0 } else { (cur + 100).min(1000) };
    let _ = ui.set_props(
        page.vol_slider,
        Props::Slider(SliderProps { value: next, min: 0, max: 1000, on_change: EV_VOL }),
    );
    next
}

/// 行 press 装配消费面：行 (trigger, visual) 对写入 `out` 前缀（pad/text
/// 双触发面 → label Text 换底；只装有 app 的行）。返回条数（帽
/// [`berry_ui::PRESS_MAP_MAX`] 由调用方缓冲承载）。
pub fn row_press_entries(page: &AppPage, out: &mut [(NodeId, NodeId)]) -> usize {
    let mut n = 0usize;
    for r in page.rows.iter().take(page.rows_n) {
        if r.app.is_none() || n + 2 > out.len() {
            continue;
        }
        out[n] = (r.pad, r.text);
        out[n + 1] = (r.text, r.text);
        n += 2;
    }
    n
}

/// BottomTabs 建树糖消费封口（desktop.rs 两档共用——big 走 pattern
/// 冻结名 `bottom_tabs`；紧凑 @1x 镜像同构）。挂在 `parent`（desktop 的
/// 条宿主 Stack——实例整条重建换选的 z 槽）。返回 (实例根, 页位节点序)。
pub fn build_tab_bar(
    ui: &mut UiMain,
    parent: NodeId,
    selected: u8,
) -> Result<(NodeId, [NodeId; 3]), UiError> {
    let (vp_w, _) = ui.viewport();
    let big = vp_w >= 600;
    let bar_h = if big { pattern::BOTTOMTABS_H } else { 48 };
    // 实例根 Col：满宽定高——pattern 的底条 Rect/Row 与页位都落其内，
    // 摘除即整条消失（重建零残）。
    let inst = ui.add(
        parent,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(vp_w),
                h: Some(bar_h),
                cross: CrossAlign::Stretch,
            }),
        },
    )?;
    let nodes = if big {
        let slots = [
            TabSlot { icon: icons::NAV_HOME, label: b"Home", selected: selected == 0 },
            TabSlot { icon: icons::NAV_RECENTS, label: b"Apps", selected: selected == 1 },
            TabSlot { icon: icons::NAV_ROTATE, label: b"Set", selected: selected == 2 },
        ];
        let all = pattern::bottom_tabs_slots(ui, inst, &slots)?;
        [all[0], all[1], all[2]]
    } else {
        // 紧凑 @1x 镜像：48px 条 / 24px 图标 / S8x16 标签 / 2px 顶指示
        // 条 / brand 染色——结构与 pattern 同构（big/compact 双档同律，
        // desktop.rs 既有双档纪律）。
        // 白底 Rect 已撤（沉浸规范 §6 第 5 项）——条不再铺 CARD_BG 底，
        // 与 big 档撤底同律；Row 透明满宽承载页位。
        let row = ui.add(
            inst,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: 0,
                    w: Some(vp_w),
                    h: Some(bar_h),
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        let ids = [icons::NAV_HOME, icons::NAV_RECENTS, icons::NAV_ROTATE];
        let labels: [&[u8]; 3] = [b"Home", b"Apps", b"Set"];
        let mut cells = [inst; 3]; // 哑元占位（NodeId 无公开构造）——循环内覆写
        for i in 0..3 {
            let cw = vp_w / 3;
            let cell = ui.add(
                row,
                ViewSpec {
                    kind: ViewKind::Col,
                    key: 0,
                    props: Props::Col(ColProps {
                        gap: 1,
                        w: Some(cw),
                        h: Some(bar_h),
                        cross: CrossAlign::Center,
                    }),
                },
            )?;
            if i as u8 == selected {
                ui.add(
                    cell,
                    ViewSpec {
                        kind: ViewKind::Rect,
                        key: 0,
                        props: Props::Rect(RectProps { bg: BRAND, w: Some(cw), h: Some(2), radius: 0 }),
                    },
                )?;
            }
            ui.add(
                cell,
                ViewSpec {
                    kind: ViewKind::Icon,
                    key: 0,
                    props: Props::Icon(IconProps { id: ids[i], fg: FG, w: Some(24), h: Some(24) }),
                },
            )?;
            let lc = if i as u8 == selected { BRAND } else { GRAY };
            ui.add(cell, text_spec(labels[i], lc, FontSize::S8x16)?)?;
            cells[i] = cell;
        }
        cells
    };
    Ok((inst, nodes))
}

// ---- 纯函数测试面（零 RPC/零 fd——同 settings_io 注）----
#[cfg(test)]
mod tests {
    use super::*;

    /// 首字母四分桶完备且互斥（ASCII 大写域全覆盖、小写/数字落 ALL）。
    #[test]
    fn filter_buckets_partition_ascii() {
        assert!(filter_matches(0, b'Z')); // ALL
        assert!(filter_matches(0, b'a'));
        for c in b'A'..=b'F' {
            assert!(filter_matches(1, c), "{} 应落 A-F", c as char);
            assert!(!filter_matches(2, c) && !filter_matches(3, c));
        }
        for c in b'G'..=b'M' {
            assert!(filter_matches(2, c), "{} 应落 G-M", c as char);
            assert!(!filter_matches(1, c) && !filter_matches(3, c));
        }
        for c in b'N'..=b'Z' {
            assert!(filter_matches(3, c), "{} 应落 N-Z", c as char);
            assert!(!filter_matches(1, c) && !filter_matches(2, c));
        }
        // 域外（小写/数字/空）只落 ALL。
        assert!(!filter_matches(1, b'a') && !filter_matches(2, b'a') && !filter_matches(3, b'a'));
        assert!(!filter_matches(1, b'0'));
    }

    /// 过滤词表恰四档、域外 filter 值宽容为 ALL（防御面）。
    #[test]
    fn filter_words_and_fallback() {
        assert_eq!(FILTER_WORDS.len(), 4);
        assert_eq!(FILTER_WORDS[0], b"ALL");
        assert!(filter_matches(9, b'?')); // 域外 = ALL
    }
}
