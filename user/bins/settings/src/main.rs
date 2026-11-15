//! settings —— 波 4 首件真 GUI 应用（SETTINGS appstub 秒退壳的垂直切片
//! 替换；.agents/research/2026-11-15-波4应用翻新侦察.md §一/§二）。
//!
//! **使命**：证明 berry-ui 41 kind 组件面能被真实 app 消费——单页设置 UI
//! （标题 Text + Toggle×2 + Slider + Check + 值 Text），交互全走引擎内建
//! [`widget_react`]（`pump_input` 期 Toggle·Check 点击翻转 / Slider 拖动
//! 跟值——app 零手写命中几何），事件经 `take_events` 消费：亮度 Slider 的
//! on_change 驱动百分值 Text 换字（`set_text` → 纯 PAINT 帧出屉）。
//!
//! **链形态** = berry-launcher 镜像（musl std + berry-libc shim + CRT0
//! 链接种子；gfxdemo/berry-launcher 先例）：
//!
//! - 服务连接：`wm_cli_h()` 门控（0 = GUI 通道未授权 → 单行诊断 + park
//!   常驻，**不秒退**——launcher「wm absent」降级同形）→ WM_GETINFO
//!   几何 → present GETINFO 轻断言 → 帧缓冲 VMO 三连（create/commit/
//!   map @ FRAME_VA）。
//! - 渲染循环：三态停帧（RUN/SETTLE/IDLE——引擎 `render_frame`/
//!   `next_wakeup` 驱动；damage 空集零 SUBMIT 零 RPC）+ SUBMIT →
//!   FRAME_CALLBACK → RELEASE 严格内联串行（在途 ≤1 结构性背压，
//!   launcher frameloop 同食谱）+ vote 票面变化才发。
//! - 输入：input_h() INPUT_POLL 批拉（≤3 事件/轮）→ 逐条 `push_pointer`
//!   → `pump_input`（手势 referee + widget_react 同拍）→ `take_events`。
//!   无 input 礼物（hi==0）= 静态页常驻（零输入零 RPC）。
//!
//! **波 4 已知缺口（侦察 §二「GUI 通道授权缺位」）**：svcmgr `app_launch`
//! 的 extra_gifts 现恰 fs 两枚——wm 客户端端/inputd 端对 app 无授权路径
//!（launcher 独享 extras [wm, fs, app_cli, input] 四件）。本程序为 wiring
//! 落地（波 4 设计首裁项）备好全链：礼物一到位，wm 门自然放行、input 面
//! 自然点亮；礼物缺席期 = 一行诊断 + park 常驻（窗口义务语义不折衷成
//! exit(0) 秒退）。
//!
//! **输出面**：manifest console=false → stdout 走 svcmgr 私有对（drain 归
//! `app_supervise`）；全部 println/eprintln 为诊断面，不构成串口标记。
//! 首帧呈现回执过链打一行 `settings: first frame`（console=true 调试装配
//! 时可见）。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, Rect as GfxRect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontSize;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto as gp;
use berry_libc::io::{input_h, self_vmar_h, wm_cli_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_handle_duplicate, berry_object_wait_many, berry_object_wait_one, berry_vmar_map,
    berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_TRANSFER, RIGHT_WRITE,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_ui::desc::{
    CheckProps, ColProps, DividerProps, RowProps, SliderProps, TextProps, TextStyle,
    ToggleProps,
};
use berry_ui::layout::CrossAlign;
use berry_ui::{
    NodeId, ProportionalTextMetrics, Props, TextBuf, UiError, UiMain, ViewKind, ViewSpec,
    ROLE_TEXT, ROLE_TOGGLE,
};

// 链接种子（fsmgr/guifbd/berry-launcher 同款）：rlib/LTO 按需保留——无引用
// 则 berry-libc 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 收侧 READABLE 帽（launcher 同值 300s：app 拉起时点 = 用户点击，
/// 首 RPC 距 boot 稳态可远——10s 会把长闲后的首击误判成超时）。
const RPC_CAP_NS: u64 = 300 * 1_000_000_000;

/// 帧缓冲映射 VA（berry-launcher frameloop 同位：mmap VA 窗
/// [0x5000_0000,0x6000_0000) 内固定位 +16MiB；页对齐）。
const FRAME_VA: usize = 0x5100_0000;

/// 默认 vsync 周期（60Hz；present GETINFO 无周期域——S2 食谱默认
/// 16_666_666 phase 0，launcher 同值）。
const VSYNC_NS: u64 = 16_666_666;

/// IDLE 稳态等待帽（S1 判词：Idle 相撤定时器 → 10s 周期兜底醒——非燃
/// CPU 阻塞形；事件口与定时面双活）。
const IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// glyph scratch 容量（堆置——paint 对超容 run 截断尾字形，视觉面非
/// 正确性面；本页 ~40 字形富余）。
const SCRATCH_GLYPHS: usize = 256;

/// 整帧清屏底色（launcher 同值——FULL 帧先铺底再回放 DisplayList）。
const CLEAR_BG: (u8, u8, u8) = (0x10, 0x18, 0x28);

// —— 视觉常量（暗底亮字；docs/应用规范-v1.0.md §3.2 对比口径）——
/// 主文本前景（对 CLEAR_BG ≈ 15.9:1）。
const TEXT_FG: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// 次级文本前景（亮度百分值；≈ 7:1）。分隔线色走引擎固定色板
///（DividerProps 无着色域——paint.rs 色板单源）。
const VALUE_FG: Color = Color { r: 0x9E, g: 0xA7, b: 0xB4 };

// —— 组件内在几何镜像（berry-ui lib.rs「组件 v1 几何常量」为 pub(crate)
//    非契约面——此处按值镜像并注明单源；样式系统落地后随迁）——
/// Toggle 内在宽（berry-ui TOGGLE_W 镜像）。
const TOGGLE_W: u32 = 52;
/// Slider 内在宽（berry-ui SLIDER_W 镜像）。
const SLIDER_W: u32 = 200;
/// Check 内在宽（berry-ui CHECK_W 镜像）。
const CHECK_W: u32 = 24;

// —— 事件 ID 面（props on_change 携载——take_events 消费锚）——
const EV_WIFI: u32 = 1;
const EV_BT: u32 = 2;
const EV_BRIGHT: u32 = 3;
const EV_SYNC: u32 = 4;

/// 亮度初值（permille；值域 0..=1000——desc.rs SliderProps 契约）。
const BRIGHT_INIT: u32 = 800;

/// 诊断 + park 收口（launcher `fail` 同形——app 常驻不被 reap，退出码无
/// 消费者；崩溃监督归 svcmgr app_supervise 域，本进程不自救）。
fn fail(what: &str) -> ! {
    eprintln!("settings: {what}");
    berry_libc::park()
}

/// now = clock_read；失败按 0 → deadline 即刻到期 = fail-fast（launcher
/// `now` 同口径——时钟不可用当死路，不静默自旋）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

/// 即关（RELEASE 归还别名——关失败 = handle 表泄漏，FAIL loud）。
fn close_h(h: Handle) {
    if berry_handle_close(h) != 0 {
        fail("handle close");
    }
}

/// 严格串行 RPC **发送半程**（launcher `rpc_send` 同形）：-EBUSY → 阻塞等
/// WRITABLE（帽重臂，雷 2 纪律不自旋）；send 原子性——EBUSY/错码时随行
/// handle 零迁移。**send 即 take**（§9 A6-b）。
fn rpc_send(ch: Handle, req: &[u8], attach: &[Handle]) {
    let (hptr, hn) = if attach.is_empty() {
        (core::ptr::null(), 0)
    } else {
        (attach.as_ptr(), attach.len())
    };
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(ch, req.as_ptr(), req.len(), hptr, hn);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            fail(&format!("rpc send err={s}"));
        }
        if deadline == 0 {
            deadline = now() + RPC_CAP_NS;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            fail("rpc send wait timeout");
        }
        deadline = 0; // 重臂（命中后仍满则续等——公平排队）
    }
}

/// 严格串行 RPC **收侧半程**（launcher `rpc_recv` 同形）：空队 -EAGAIN →
/// 阻塞等 READABLE（`RPC_CAP_NS` 帽）；每轮重置容量（in-out 出参——漏
/// 重置 = 恒 -EINVAL 死循环，A1-b）。
fn rpc_recv(ch: Handle, rep: &mut [u8; 256], rhs: &mut [Handle; 8]) -> (usize, usize) {
    loop {
        let mut nb = rep.len();
        let mut nh = rhs.len();
        let s = berry_channel_recv(ch, rep.as_mut_ptr(), &mut nb, rhs.as_mut_ptr(), &mut nh);
        if s == 0 {
            if nb == 0 {
                fail("rpc empty reply");
            }
            return (nb, nh);
        }
        if s != BERRY_ERR_AGAIN {
            fail(&format!("rpc recv err={s}"));
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            fail("rpc reply timeout");
        }
    }
}

/// 严格串行 RPC 单趟（一请求一应答；launcher `rpc` 同形）。
fn rpc(
    ch: Handle,
    req: &[u8],
    attach: &[Handle],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> (usize, usize) {
    rpc_send(ch, req, attach);
    rpc_recv(ch, rep, rhs)
}

// —— 建树小件（launcher desktop.rs `text_spec`/`add_status_spacer` 同形）——

/// Text 叶 spec（超 `TEXT_INLINE`=64B loud Err——本页文案 ≤16B 域内）。
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

/// 定宽/定高占位 Col（launcher 状态栏 spacer 同形——Row/Col 游标容器里
/// Spacer 测量恒 0×0 不吃余量，flex 分配挂 berry-ui 后续布局刀，故定宽
/// Col 承担推位）。
fn box_spec(w: u32, h: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Col,
        key: 0,
        props: Props::Col(ColProps { gap: 0, w: Some(w), h: Some(h), cross: CrossAlign::Center }),
    }
}

/// 设置页运行期句柄（建树一次落定；帧循环只 `set_text` 亮度值 + 消费
/// 事件比对节点）。
struct Page {
    /// 亮度百分值 Text（Slider on_change 驱动换字）。
    bright_value: NodeId,
    /// 亮度 Slider 本体（事件源比对锚）。
    bright_slider: NodeId,
}

/// 建设置页树：根 Col → 顶留白 → 内容 Col（标题 + 分隔线 + 四行）。
/// 每行 = Row[定宽标签列, 定宽推位, 组件]（右侧对齐——组件 x 恒定，值
/// 文本换宽不抖动）。Err = 容量/文本域违例 → 调用方 fail 收口。
fn build(ui: &mut UiMain) -> Result<Page, UiError> {
    let (vp_w, vp_h) = ui.viewport();
    // 档位判别同 launcher desktop.rs：OH phone 1080×1920 走 big 档
    //（24vp×3 padding），800×600 调试档紧凑。
    let big = vp_w >= 600 && vp_h >= 1800;
    let pad_h = if big { 96 } else { 32 };
    let pad_v = if big { 120 } else { 40 };
    let row_gap = if big { 36 } else { 12 };
    let label_size = if big { FontSize::S32x48 } else { FontSize::S16x32 };
    let title_size = if big { FontSize::S32x48 } else { FontSize::S16x32 };
    let value_size = if big { FontSize::S24x48 } else { FontSize::S8x16 };
    let divider_t = if big { 3 } else { 1 };
    let content_w = vp_w.saturating_sub(pad_h * 2);
    // 标签列宽 = 最长文案「Brightness」10 字 × cell_w（等宽估值——值文本
    // 列另用定宽容器钉位，估值不进组件 x）。
    let label_col_w = 10 * label_size.cell_w() as u32;
    // 值列宽 = 4 字（「100%」最长档）× cell_w。
    let value_col_w = 4 * value_size.cell_w() as u32;

    let root = ui.root();
    let page_col = ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: row_gap,
                w: Some(vp_w),
                h: Some(vp_h),
                cross: CrossAlign::Center,
            }),
        },
    )?;
    // 顶留白（Col 游标从顶起）。
    ui.add(page_col, box_spec(1, pad_v))?;
    // 内容列（cross=Stretch——Divider 借随父拉通吃 content_w）。
    let content = ui.add(
        page_col,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: row_gap,
                w: Some(content_w),
                h: None,
                cross: CrossAlign::Stretch,
            }),
        },
    )?;

    // 标题（左对齐：内容列 Stretch 不改 Text 内在宽，游标 x=0 起）。
    let title = ui.add(content, text_spec(b"Settings", TEXT_FG, title_size)?)?;
    ui.set_a11y(title, ROLE_TEXT, b"settings")?;
    // 标题下分隔线（宽随父 = content_w；纯装饰不占 a11y 槽）。
    ui.add(
        content,
        ViewSpec {
            kind: ViewKind::Divider,
            key: 0,
            props: Props::Divider(DividerProps { thickness: divider_t, inset_start: 0, inset_end: 0 }),
        },
    )?;

    /// 单行装配（标签 + 推位——尾件由调用方追加）：`gaps` = 本行全部
    /// 子间缝数（Row gap × 子数-1），推位宽 = 余量扣除全部缝（行内总宽
    /// 恒 content_w——尾件 x 钉位不随文案宽抖动）。
    fn row(
        ui: &mut UiMain,
        parent: NodeId,
        content_w: u32,
        label_col_w: u32,
        label: &[u8],
        tail_w: u32,
        gaps: u32,
        gap: u32,
        size: FontSize,
    ) -> Result<NodeId, UiError> {
        let r = ui.add(
            parent,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap,
                    w: Some(content_w),
                    h: None,
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        // 标签列（定宽容器 + 内 Text——文本实测宽不进游标）。
        let lc = ui.add(
            r,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 0,
                    w: Some(label_col_w),
                    h: None,
                    cross: CrossAlign::Start,
                }),
            },
        )?;
        ui.add(lc, text_spec(label, TEXT_FG, size)?)?;
        // 推位（定宽——标签列到尾件的全部余量，扣尽全部缝）。
        let flex = content_w
            .saturating_sub(label_col_w)
            .saturating_sub(tail_w)
            .saturating_sub(gap * gaps);
        ui.add(r, box_spec(flex, 1))?;
        Ok(r)
    }

    // — Wi-Fi 开关（checked 初值 on——点击翻转走引擎 widget_react）—
    let r = row(
        ui,
        content,
        content_w,
        label_col_w,
        b"Wi-Fi",
        TOGGLE_W,
        2,
        row_gap,
        label_size,
    )?;
    let wifi = ui.add(
        r,
        ViewSpec {
            kind: ViewKind::Toggle,
            key: 0,
            props: Props::Toggle(ToggleProps { checked: true, on_change: EV_WIFI }),
        },
    )?;
    ui.set_a11y(wifi, ROLE_TOGGLE, b"wifi toggle")?;

    // — Bluetooth 开关 —
    let r = row(
        ui,
        content,
        content_w,
        label_col_w,
        b"Bluetooth",
        TOGGLE_W,
        2,
        row_gap,
        label_size,
    )?;
    let bt = ui.add(
        r,
        ViewSpec {
            kind: ViewKind::Toggle,
            key: 0,
            props: Props::Toggle(ToggleProps { checked: false, on_change: EV_BT }),
        },
    )?;
    ui.set_a11y(bt, ROLE_TOGGLE, b"bluetooth toggle")?;

    // — 亮度（标签 + 推位 + 值 Text + Slider 四子三缝：拖动跟值 →
    //   on_change → 帧循环 set_text 百分值。值文本入定宽列——数字变宽
    //   不推 Slider）—
    let r = row(
        ui,
        content,
        content_w,
        label_col_w,
        b"Brightness",
        value_col_w + SLIDER_W,
        3,
        row_gap,
        label_size,
    )?;
    let vc = ui.add(
        r,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(value_col_w),
                h: None,
                cross: CrossAlign::Start,
            }),
        },
    )?;
    let bright_value = ui.add(vc, text_spec(b"80%", VALUE_FG, value_size)?)?;
    ui.set_a11y(bright_value, ROLE_TEXT, b"brightness percent")?;
    let bright_slider = ui.add(
        r,
        ViewSpec {
            kind: ViewKind::Slider,
            key: 0,
            props: Props::Slider(SliderProps {
                value: BRIGHT_INIT,
                min: 0,
                max: 1000,
                on_change: EV_BRIGHT,
            }),
        },
    )?;
    ui.set_a11y(bright_slider, ROLE_TOGGLE, b"brightness slider")?;

    // — 自动同步复选框 —
    let r = row(
        ui,
        content,
        content_w,
        label_col_w,
        b"Auto-sync",
        CHECK_W,
        2,
        row_gap,
        label_size,
    )?;
    let sync = ui.add(
        r,
        ViewSpec {
            kind: ViewKind::Check,
            key: 0,
            props: Props::Check(CheckProps { checked: false, on_change: EV_SYNC }),
        },
    )?;
    ui.set_a11y(sync, ROLE_TOGGLE, b"auto-sync check")?;

    Ok(Page { bright_value, bright_slider })
}

/// permille（0..=1000）→ 百分值字节串（≤4B：`100%` 最长档）。
fn pct_bytes(v: u32) -> [u8; 4] {
    let pct = v.min(1000) / 10;
    let mut out = [0u8; 4];
    if pct >= 100 {
        out[0] = b'1';
        out[1] = b'0';
        out[2] = b'0';
        out[3] = b'%';
    } else if pct >= 10 {
        out[0] = b'0' + (pct / 10) as u8;
        out[1] = b'0' + (pct % 10) as u8;
        out[2] = b'%';
    } else {
        out[0] = b'0' + pct as u8;
        out[1] = b'%';
    }
    out
}

/// 百分值有效长（`pct_bytes` 的伴随面——set_text 按有效长切片）。
fn pct_len(v: u32) -> usize {
    let pct = v.min(1000) / 10;
    if pct >= 100 {
        4
    } else if pct >= 10 {
        3
    } else {
        2
    }
}

fn main() {
    // —— 降级门：wm 客户端端未交付（波 4 缺口——app_launch extra_gifts
    //    现▮fs 两枚，GUI 通道授权路径未落）→ 一行诊断 + park 常驻
    //    （launcher「wm absent」同形；appstub 的 exit(0) 秒退就此退役；
    //    wiring 落地后本门自然放行）。
    let hw = wm_cli_h();
    if hw == 0 {
        eprintln!("settings: wm absent (no gui gift)");
        berry_libc::park();
    }

    // —— WM_GETINFO 一次握手（几何真值源；原值宁报勿猜——parse 失败 =
    //    eprintln 诊断 + park，不留半开态）。
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_wm_getinfo_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let info = match gp::parse_wm_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("wm getinfo rep bad st={}", rep[0])),
    };
    eprintln!(
        "settings: wm v={} {}x{} input={}",
        info.version_wm,
        info.disp_w,
        info.disp_h,
        input_h()
    );

    // —— UI arena 堆置（launcher R5 仪器定谳：`UiMain::new` 构造中间体
    //    LTO 后仍部分落栈 ~29KB——8 页栈穿保护页先例；Box::leak = 常驻
    //    面无回收语义，leak 即分配）。比例文本供件同 launcher（HD 档
    //    内在测量走真实 advance；三档点阵下与等宽逐值恒等）。
    let ui: &'static mut UiMain =
        Box::leak(Box::new(UiMain::new(info.disp_w as u32, info.disp_h as u32)));
    static TEXT_METRICS: ProportionalTextMetrics = ProportionalTextMetrics;
    ui.set_text_metrics(&TEXT_METRICS);

    // —— 建树（Err = 容量/文本域违例 → fail 收口）。
    let page = match build(ui) {
        Ok(p) => p,
        Err(e) => fail(&format!("page build err={e:?}")),
    };

    // —— 帧循环（常驻义务——永不返回）。
    run(hw, input_h(), ui, page);
}

/// 帧循环 + 输入 + 事件消费（launcher frameloop 食谱的 app 最小形：无
/// pager/自测/spawn 面；widget 交互 = 引擎 widget_react，app 只消费事件
/// 换值文本）。
fn run(hw: Handle, hi_in: Handle, ui: &'static mut UiMain, page: Page) -> ! {
    // —— present GETINFO（wmd 中继同端）：能力面轻断言 + 几何一致性
    //    （建树真值 = wm GETINFO；present 权威——失配 fail loud）。
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = match gp::encode_present_req(&mut req, &gp::PresentReq::Getinfo) {
        Some(m) => m,
        None => fail("present getinfo encode"),
    };
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let pinfo = match gp::parse_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("present getinfo bad st={}", rep[0])),
    };
    if pinfo.version_present != gp::VER_PRESENT
        || pinfo.format_bits & gp::FMT_XRGB8888 == 0
        || pinfo.max_credits != gp::CREDITS_MAX
    {
        fail(&format!(
            "present getinfo vp={} fmt={:#x} credits={}",
            pinfo.version_present, pinfo.format_bits, pinfo.max_credits
        ));
    }
    let (vp_w, vp_h) = ui.viewport();
    if pinfo.display_w != vp_w || pinfo.display_h != vp_h {
        fail(&format!(
            "disp mismatch wm {}x{} present {}x{}",
            vp_w, vp_h, pinfo.display_w, pinfo.display_h
        ));
    }

    // —— 跨代清账（launcher R2 修同款：前代实例死于 SUBMIT ack→RELEASE
    //    close 窗的孤儿别名，RELEASE_WAIT 应答 = held 全量排空含他源）。
    let m = match gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait) {
        Some(m) => m,
        None => fail("release encode"),
    };
    let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    if gp::parse_release_rep(&rep[..n]).is_none() {
        fail(&format!("boot release bad st={}", rep[0]));
    }
    for h in &rhs[..nh] {
        close_h(*h);
    }

    // —— 帧缓冲 VMO 三连（gfxdemo/launcher raw 形同源）：stride = w*4
    //    tight；len 页取整（vmar_map 页倍执法）。
    let (w, h) = (pinfo.display_w as usize, pinfo.display_h as usize);
    let stride = w.checked_mul(4).unwrap_or_else(|| fail("stride overflow"));
    let frame_bytes = stride.checked_mul(h).unwrap_or_else(|| fail("frame overflow"));
    let len = frame_bytes
        .checked_add(0xFFF)
        .map(|v| v & !0xFFF)
        .unwrap_or_else(|| fail("page round overflow"));
    let mut vmo: Handle = 0;
    if berry_vmo_create(len as u64, 0, &mut vmo) != 0 || vmo == 0 {
        fail("vmo create");
    }
    if berry_vmo_commit(vmo, 0, len as u64) != 0 {
        fail("vmo commit");
    }
    let hv = self_vmar_h();
    if hv == 0 {
        fail("no self vmar gift");
    }
    let mut va = FRAME_VA;
    if berry_vmar_map(hv, 0, vmo, 0, &mut va, len, RIGHT_READ | RIGHT_WRITE) != 0 || va != FRAME_VA
    {
        fail("vmar map");
    }
    let desc = match TextureDesc::new(w, h, stride, TextureFormat::Rgbx) {
        Ok(d) => d,
        Err(e) => fail(&format!("texture desc {e:?}")),
    };
    // SAFETY: [FRAME_VA, FRAME_VA+len) 刚由 vmar_map 落位且整段已 commit
    //（gfxdemo/launcher 同形）；run 永不返回——fb 终身持有。
    let fb: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(va as *mut u8, len) };

    // —— glyph scratch 堆置（DisplayList 'a 钉调用方 scratch——paint.rs
    //    头注契约；堆置免栈穿）。
    let scratch: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; SCRATCH_GLYPHS].into_boxed_slice());

    // —— vsync 协商 + 起振（Idle→Run + 条件 FULL——首帧整帧 damage）。
    ui.set_vsync(VSYNC_NS, 0);
    ui.wake(now());

    let mut last_expected: u64 = 0; // pump 重采样 target（上帧 expected 轨）
    let mut last_vote: Option<berry_ui::frame::VoteBallot> = None; // 票面变化才发
    let mut presented: u64 = 0; // FRAME_CALLBACK 过链计数（首帧诊断门）
    let mut buf_id: u32 = 1; // SUBMIT buffer 代际
    let mut bright_v: u32 = BRIGHT_INIT; // 值缓存（值同跳过 set_text——零脏帧）
    let mut hi = hi_in; // input 端（失效面置 0——静态页续活）
    let mut input_pending = false; // INPUT_POLL 在途
    let mut input_buf = [0u8; gp::INPUT_REP_MAX];
    let mut input_hs = [0 as Handle; 8];

    loop {
        let t_now = now();

        // —— 真实输入（launcher 段④ 同形）：先收上一轮 INPUT_POLL 应答，
        //    再挂下一轮 poll；事件逐条 push 进引擎（widget_react 在
        //    pump_input 期同拍消费——Toggle 翻转/Slider 跟值落 props）。
        if hi != 0 && input_pending {
            let mut nb = input_buf.len();
            let mut nh = input_hs.len();
            let s = berry_channel_recv(
                hi,
                input_buf.as_mut_ptr(),
                &mut nb,
                input_hs.as_mut_ptr(),
                &mut nh,
            );
            if s == 0 {
                input_pending = false;
                for h in &input_hs[..nh] {
                    close_h(*h);
                }
                match gp::parse_input_events_rep(&input_buf[..nb]) {
                    Some(Ok(view)) => {
                        for i in 0..view.count() {
                            if let Some(ev) = view.event(i) {
                                let pe = berry_ui::hit::PtrEvent::from_wire(
                                    ev.phase,
                                    ev.pointer_id,
                                    ev.x,
                                    ev.y,
                                    ev.timestamp_ns,
                                    ev.buttons,
                                    ev.pressure,
                                );
                                if let Err(e) = ui.push_pointer(pe) {
                                    eprintln!("settings: input push err={e:?}");
                                }
                            }
                        }
                        ui.wake(now());
                    }
                    Some(Err(gp::ST_EAGAIN)) => {} // 无事件常态
                    Some(Err(st)) => eprintln!("settings: input poll st={st}"),
                    None => eprintln!("settings: input poll rep bad len={nb}"),
                }
            } else if s != BERRY_ERR_AGAIN {
                eprintln!("settings: input recv err={s}");
                hi = 0;
                input_pending = false;
            }
        }
        if hi != 0 && !input_pending {
            if input_arm(hi) {
                input_pending = true;
            } else {
                // 输入面失效（arm 失败 = 通道死/超时）——置 0 永久降级为
                // 静态页（launcher 段④ 同形；页面常驻义务不受输入面拖累）。
                hi = 0;
            }
        }

        // —— 输入泵（referee + 重采样 + widget_react——引擎单口）。
        //    手势 Msg 本页无消费者（无翻页/拖滚动容器）：读取即弃；
        //    组件事件经 take_events 走 app 面。
        let _ = ui.pump_input(last_expected);

        // —— 组件事件消费：Toggle/Check = 引擎内已翻转（视觉已脏，读取
        //    即记账）；Brightness Slider on_change → 百分值 Text 换字
        //    （值同跳过——零冗余脏帧）。
        let evs = ui.take_events();
        let mut new_bright: Option<u32> = None;
        for ev in evs.iter() {
            if ev.node == page.bright_slider && ev.event == EV_BRIGHT {
                new_bright = Some(ev.value);
            }
        }
        if let Some(v) = new_bright {
            if v != bright_v {
                let b = pct_bytes(v);
                if let Err(e) = ui.set_text(page.bright_value, &b[..pct_len(v)]) {
                    fail(&format!("brightness set_text err={e:?}"));
                }
                bright_v = v;
                ui.wake(now());
            }
        }

        // —— 渲染（damage 空集零提交——跳过帧零 RPC）。
        let mut enc = CommandEncoder::new();
        let out = ui.render_frame(t_now, scratch, &mut enc);
        last_expected = out.expected_present_ns;
        if out.drew {
            let dmg = out.damage;
            // K>0 xor FULL 立法兜底：count==0 非空集视同整帧。
            let full = dmg.full || dmg.count == 0;
            let pass = if full {
                PassDescriptor::clear(CLEAR_BG.0, CLEAR_BG.1, CLEAR_BG.2)
            } else {
                // 增量帧：Load+Store + K 矩形并集包络（soft 光栅只在
                // dirty 域动笔，域外像素保持上帧）。
                PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(damage_envelope(&dmg)))
            };
            let dl = enc.finish();
            if let Err(e) = Queue::new().submit(&dl, fb, &desc, &pass) {
                fail(&format!("raster submit {e:?}"));
            }

            // SUBMIT：fresh READ dup 别名随行（send 即 take——原 vmo 留
            // 表；TRANSFER 保位——随行预检执法）。
            let mut alias: Handle = 0;
            if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias) != 0
                || alias == 0
            {
                fail("dup alias");
            }
            let (flags, rects, k) = if full {
                (
                    gp::SUBMIT_F_DAMAGE_FULL,
                    [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
                    0usize,
                )
            } else {
                (0u8, dmg.rects, dmg.count as usize)
            };
            let sreq = gp::PresentReq::Submit {
                surface_id: 0,
                buffer_id: buf_id,
                flags,
                rate_hint: out.rate_hint_hz,
                target_present_ns: out.expected_present_ns,
                rects,
                rect_count: k,
            };
            let m = match gp::encode_present_req(&mut req, &sreq) {
                Some(m) => m,
                None => fail("submit encode"),
            };
            let (n, _) = rpc(hw, &req[..m], &[alias], &mut rep, &mut rhs);
            let token = match gp::parse_submit_ack(&rep[..n]) {
                Some(t) => t,
                None => fail(&format!("submit ack bad st={}", rep[0])),
            };
            ui.note_submitted();

            // FRAME_CALLBACK：blit-at-SUBMIT 形下帧已呈现——断
            // PRESENTED（DISCARDED = 呈现链破相 fail loud）。
            let m = match gp::encode_present_req(
                &mut req,
                &gp::PresentReq::FrameCallback { frame_token: token },
            ) {
                Some(m) => m,
                None => fail("frame callback encode"),
            };
            let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
            let done = match gp::parse_frame_done_rep(&rep[..n]) {
                Some(d) => d,
                None => fail(&format!("frame done bad st={}", rep[0])),
            };
            if done.status != gp::FRAME_PRESENTED {
                fail(&format!("frame status={}", done.status));
            }
            // seq = 本帧引擎序（render 后 telemetry 帧计数 -1 = last_seq）；
            // credits 以服务端应答为真值重同步（S2 食谱）。
            ui.note_presented(
                (ui.telemetry().frames() - 1) as u32,
                now(),
                done.status,
                done.credits,
            );

            // RELEASE（归还别名即关——在途 ≤1 结构性背压；RETURN-ALL
            // 语义：count≥1 断言 + 随行全量 close）。
            let m = match gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait) {
                Some(m) => m,
                None => fail("release encode"),
            };
            let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
            let rel = match gp::parse_release_rep(&rep[..n]) {
                Some(r) => r,
                None => fail(&format!("release rep bad st={}", rep[0])),
            };
            if rel.count < 1 || nh != rel.count {
                fail(&format!("release count={} nh={}", rel.count, nh));
            }
            for h in &rhs[..nh] {
                close_h(*h);
            }

            presented += 1;
            buf_id = buf_id.wrapping_add(1);
            if buf_id == 0 {
                buf_id = 1;
            }
            if presented == 1 {
                // 首帧呈现回执过链（诊断面——console=false 走私有对，
                // console=true 调试装配串口可见）。
                println!("settings: first frame");
            }
        }

        // —— vote（S2 食谱：票面变化才发——省流；幂等面在服务端）。
        let ballot = ui.vote_hint();
        if last_vote != Some(ballot) {
            let vreq = gp::PresentReq::Vote {
                sticky: ballot.sticky,
                scene: ballot.scene,
                min_hz: ballot.min_hz,
                max_hz: ballot.max_hz,
                preferred_hz: ballot.preferred_hz,
            };
            let m = match gp::encode_present_req(&mut req, &vreq) {
                Some(m) => m,
                None => fail("vote encode"),
            };
            let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
            match gp::parse_status_rep(&rep[..n]) {
                Some(gp::ST_OK) => last_vote = Some(ballot),
                Some(st) => fail(&format!("vote st={st}")),
                None => fail(&format!("vote rep bad len={n}")),
            }
        }

        // —— 稳态等待（S1 判词：wait_many n≤2 [wm READABLE, input
        //    READABLE] + deadline = min(next_wakeup, now+IDLE 帽)；Idle
        //    相 = 睡满 10s 零渲染零 RPC 非燃 CPU；TIMEDOUT = 期限到
        //    正常醒——渲染下一拍 / Idle 空转睡回）。
        let cap = now() + IDLE_CAP_NS;
        let deadline = ui.next_wakeup().map_or(cap, |t| t.min(cap));
        let mut hs = [hw, 0 as Handle];
        let ss = [SIGNAL_READABLE, SIGNAL_READABLE];
        let mut n = 1usize;
        if hi != 0 && input_pending {
            hs[1] = hi;
            n = 2;
        }
        let mut idx = 0usize;
        let mut obs = 0u64;
        let wst = berry_object_wait_many(hs.as_ptr(), ss.as_ptr(), n, deadline, &mut idx, &mut obs);
        if wst != 0 && wst != BERRY_ERR_TIMEDOUT {
            fail(&format!("wait err={wst}"));
        }
    }
}

/// K 矩形并集包络（增量 pass 的 dirty 域；launcher frameloop 同形——
/// engine damage 恒 viewport 裁剪，i32 折 try_from 饱和防御）。
fn damage_envelope(dmg: &berry_ui::damage::DamageSet) -> GfxRect {
    let mut x0 = u32::MAX;
    let mut y0 = u32::MAX;
    let mut x1 = 0u32;
    let mut y1 = 0u32;
    for r in &dmg.rects[..dmg.count as usize] {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.x.saturating_add(r.w));
        y1 = y1.max(r.y.saturating_add(r.h));
    }
    let (ew, eh) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    GfxRect::new(
        i32::try_from(x0).unwrap_or(0),
        i32::try_from(y0).unwrap_or(0),
        i32::try_from(ew).unwrap_or(0).max(0),
        i32::try_from(eh).unwrap_or(0).max(0),
    )
}

/// 挂一轮 INPUT_POLL（launcher `input_arm` 同形）：inputd 阻塞批拉 ≤3
/// 事件；-EBUSY = 通道满（前一轮未收）等 WRITABLE 后重试；其余错 =
/// 输入面失效（调用方置 hi=0——静态页续活不死）。
fn input_arm(h: Handle) -> bool {
    let mut req = [0u8; 4];
    let n = gp::encode_input_poll_req(&mut req, gp::INPUT_EVENTS_MAX as u8);
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(h, req.as_ptr(), n, core::ptr::null(), 0);
        if s == 0 {
            return true;
        }
        if s != BERRY_ERR_BUSY {
            eprintln!("settings: input send err={s}");
            return false;
        }
        if deadline == 0 {
            deadline = now() + 10_000_000_000;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            eprintln!("settings: input send wait timeout");
            return false;
        }
        deadline = 0;
    }
}
