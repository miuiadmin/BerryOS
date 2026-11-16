//! # berry-room —— Berry Room 应用运行室
//!
//! Berry Room 的分层边界：本 crate 只持有系统会话与生命周期（APP env、WM
//! session、per-app 字形 VMO、全屏窗口、frame VMO、present callback、
//! INPUT_POLL、WM_DESTROY）。它不布局、不定义控件样式，也不知道业务
//! 页面；F5 起仅把上层已产出的 DisplayList 光栅化到本 Room 的 frame
//! VMO。app 通过 [`AppRoom`] 消费这些能力。

#![cfg_attr(not(test), no_std)]

use berry_gfx::cmd::DisplayList;
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::{GlyphA8, GlyphAtlas, GlyphBits};
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto as gp;
use berry_sys::{
    berry_channel_create, berry_channel_recv, berry_channel_send, berry_clock_read,
    berry_handle_close, berry_handle_duplicate, berry_object_wait_one, berry_process_exit,
    berry_vmar_map, berry_vmar_unmap, berry_vmo_commit, berry_vmo_create, Handle,
    BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_WRITE,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};

/// Native Shell frame VMO 固定映射地址。
pub const FRAME_VA: usize = 0x5200_0000;
/// wmd per-app 32×48 HD A8 字形 VMO 固定映射地址。
pub const FONT_VA: usize = 0x5300_0000;
/// 95 ASCII glyph × 32×48 A8，页对齐到 0x24000。
pub const FONT_VMO_LEN: usize = 0x24000;
/// RPC 总帽：与 GUI 服务 256B 帧协议一致。
const RPC_CAP_NS: u64 = 120 * 1_000_000_000;

/// wmd per-app 32×48 ASCII A8 字形图集。`AppRoom::connect` 成功映射
/// FONT_VA 后才可消费；F5 BerryUI 客机用它替代静态 HD 字表，避免每个
/// app 包复制一份 145KB A8 数据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemFontAtlas;

/// 全局唯一 HD 图集对象；映射真值由 AppRoom 生命周期持有。
pub static SYSTEM_FONT_ATLAS: SystemFontAtlas = SystemFontAtlas;

const SYSTEM_GLYPH_W: usize = 32;
const SYSTEM_GLYPH_H: usize = 48;
const SYSTEM_GLYPH_BYTES: usize = SYSTEM_GLYPH_W * SYSTEM_GLYPH_H;

impl GlyphAtlas for SystemFontAtlas {
    fn cell_w(&self) -> usize {
        SYSTEM_GLYPH_W
    }

    fn cell_h(&self) -> usize {
        SYSTEM_GLYPH_H
    }

    fn glyph(&self, _ch: u8) -> Option<GlyphBits<'_>> {
        // 系统 VMO 只供给 A8 覆盖率；raster 的 coverage_cp 优先路径不
        // 需要 1-bit 位图。
        None
    }

    fn coverage(&self, ch: u8) -> Option<GlyphA8<'_>> {
        let index = usize::from(ch).checked_sub(32)?;
        if ch > b'~' {
            return None;
        }
        // SAFETY：AppRoom::connect 已把 wmd 私有字形 VMO 固定映射到
        // FONT_VA..+FONT_VMO_LEN；index < 95，且 95*1536=145,920 <
        // FONT_VMO_LEN=147,456（页对齐余量不读）。
        let coverage = unsafe {
            core::slice::from_raw_parts(
                (FONT_VA + index * SYSTEM_GLYPH_BYTES) as *const u8,
                SYSTEM_GLYPH_BYTES,
            )
        };
        Some(GlyphA8 {
            w: SYSTEM_GLYPH_W,
            h: SYSTEM_GLYPH_H,
            coverage,
        })
    }
}

/// 宿主启动配置。`uses_fs` 决定 env gift 中 broker 的位置：
/// 无 fs = hs[4]；`fs=["rootfs"]` 时 fs 双 gift 插前，broker = hs[6]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomConfig {
    pub argv0: &'static [u8],
    pub uses_fs: bool,
    /// manifest `surface_escape=true` 时为 true；connect 会把 svcmgr 铸
    /// 的 per-boot seed 随 WM_CONNECT 迁给 wmd。
    pub surface_escape: bool,
}

/// App 可声明的 Surface 形态（Berry Room 安全子集，不暴露 wire codec）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceModeKind {
    Retained,
    Canvas,
    Immersive,
}

/// Surface 请求语义。授权 token 由 Room 在 connect 期迁移；app 只见枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceRequest {
    pub mode: SurfaceModeKind,
    pub input_classes: u8,
    pub ime_mode: u8,
    pub a11y_mode: u8,
}

/// Host 授予结果（safe area/scale 只读）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceGrant {
    pub mode: SurfaceModeKind,
    pub input_classes: u8,
    pub ime_mode: u8,
    pub a11y_mode: u8,
    pub contract_flags: u8,
    pub scale_permille: u16,
    pub safe_left: u16,
    pub safe_top: u16,
    pub safe_right: u16,
    pub safe_bottom: u16,
}

/// Touch 输入类位。
pub const SURFACE_INPUT_TOUCH: u8 = 1 << 0;
/// Stylus 输入类位。
pub const SURFACE_INPUT_STYLUS: u8 = 1 << 1;
/// Mouse 输入类位。
pub const SURFACE_INPUT_MOUSE: u8 = 1 << 2;
/// Keyboard 输入类位（P2 gui-inputd → wmd 真生产链）。
pub const SURFACE_INPUT_KEYBOARD: u8 = 1 << 3;
/// IME Disabled。
pub const SURFACE_IME_DISABLED: u8 = 0;
/// a11y Hybrid（BerryUI 外壳 + canvas 区域）。
pub const SURFACE_A11Y_HYBRID: u8 = 2;
/// a11y Tree（Retained）。
pub const SURFACE_A11Y_TREE: u8 = 0;
/// a11y Regions。
pub const SURFACE_A11Y_REGIONS: u8 = 1;

/// R3 config 事件原因（Room 侧安全枚举，不暴露 wire codec）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigCause {
    Created,
    Geometry,
    Visibility,
    Occlusion,
}

/// R3 per-window 配置事件。raw/visible bounds 均为屏幕坐标；safe insets
/// 由消费侧从差集现场派生。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowConfigEvent {
    pub serial: u32,
    pub window_id: u16,
    pub cause: ConfigCause,
    pub visible: bool,
    pub raw: (u16, u16, u16, u16),
    pub visible_bounds: (u16, u16, u16, u16),
}

/// 从 APP env 块解析出的基础能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomEnv {
    pub boot_probe: bool,
    pub console: Handle,
    pub process: Handle,
    pub vmar: Handle,
    pub broker: Handle,
    pub fs: Handle,
    pub surface_token: Handle,
}

/// 宿主错误。调用方应打印诊断并以非 0 退出；本 crate 不 panic。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomError {
    Env,
    Channel,
    BusyTimeout,
    Send(i32),
    Recv(i32),
    Encode,
    Decode,
    SubmitAck,
    FrameDone,
    ReleaseReply,
    Handle,
    FontMap,
    Geometry,
    Vmo,
    Map,
    Input,
    Window,
    Destroy,
    Table,
    SurfaceDenied,
    SurfaceUnsupported,
    SurfaceBad,
    ConfigBad,
    ConfigDenied,
}

pub type RoomResult<T> = Result<T, RoomError>;

/// no_std app panic/early-exit 收口。正常生命周期用 AppRoom::exit。
pub fn panic_exit(code: i32) -> ! {
    berry_process_exit(code)
}

impl RoomError {
    /// 诊断短名（ASCII，无分配）。调用方可拼进自己的 app 前缀。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Channel => "channel",
            Self::BusyTimeout => "busy timeout",
            Self::Send(_) => "send",
            Self::Recv(_) => "recv",
            Self::Encode => "encode",
            Self::Decode => "decode",
            Self::SubmitAck => "submit ack",
            Self::FrameDone => "frame done",
            Self::ReleaseReply => "release reply",
            Self::Handle => "handle",
            Self::FontMap => "font map",
            Self::Geometry => "geometry",
            Self::Vmo => "vmo",
            Self::Map => "map",
            Self::Input => "input",
            Self::Window => "window",
            Self::Destroy => "destroy",
            Self::Table => "table",
            Self::SurfaceDenied => "surface denied",
            Self::SurfaceUnsupported => "surface unsupported",
            Self::SurfaceBad => "surface bad",
            Self::ConfigBad => "config bad",
            Self::ConfigDenied => "config denied",
        }
    }
}

/// 公开输入事件类型，避免 app 直接依赖/解析 GUI wire 协议。
pub use berry_guiproto::{
    INPUT_EVENTS_MAX, INPUT_KEY_EVENTS_MAX, INPUT_SAMPLES_MAX, KeyEvent, KEY_ACTION_DOWN,
    KEY_ACTION_UP, PHASE_UP, PointerEvent, Sample,
};

fn now_ns() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        berry_process_exit(134);
    }
    t
}

fn send_bytes(ch: Handle, bytes: &[u8], attach: &[Handle]) -> RoomResult<()> {
    let deadline = now_ns().checked_add(RPC_CAP_NS).ok_or(RoomError::BusyTimeout)?;
    loop {
        let (hp, hn) = if attach.is_empty() {
            (core::ptr::null(), 0)
        } else {
            (attach.as_ptr(), attach.len())
        };
        let st = berry_channel_send(ch, bytes.as_ptr(), bytes.len(), hp, hn);
        if st == 0 {
            return Ok(());
        }
        if st != BERRY_ERR_BUSY {
            return Err(RoomError::Send(st));
        }
        let mut observed = 0u64;
        if berry_object_wait_one(ch, SIGNAL_WRITABLE, deadline, &mut observed) != 0 {
            return Err(RoomError::BusyTimeout);
        }
    }
}

fn recv_message(
    ch: Handle,
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> RoomResult<(usize, usize)> {
    loop {
        let mut nbytes = rep.len();
        let mut nhandles = rhs.len();
        let st = berry_channel_recv(
            ch,
            rep.as_mut_ptr(),
            &mut nbytes,
            rhs.as_mut_ptr(),
            &mut nhandles,
        );
        if st == 0 {
            return Ok((nbytes, nhandles));
        }
        if st != BERRY_ERR_AGAIN {
            return Err(RoomError::Recv(st));
        }
        let deadline = now_ns().checked_add(RPC_CAP_NS).ok_or(RoomError::BusyTimeout)?;
        let mut observed = 0u64;
        if berry_object_wait_one(ch, SIGNAL_READABLE, deadline, &mut observed) != 0 {
            return Err(RoomError::BusyTimeout);
        }
    }
}

fn rpc(
    ch: Handle,
    request: &[u8],
    attach: &[Handle],
    reply: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> RoomResult<(usize, usize)> {
    send_bytes(ch, request, attach)?;
    recv_message(ch, reply, rhs)
}

fn pair() -> RoomResult<(Handle, Handle)> {
    let mut a: Handle = 0;
    let mut b: Handle = 0;
    if berry_channel_create(0, &mut a, &mut b) != 0 || a == 0 || b == 0 {
        return Err(RoomError::Channel);
    }
    Ok((a, b))
}

impl RoomEnv {
    /// 读取并验证 APP bootstrap env。argv0 是 manifest entry 的完整
    /// `bin/<name>` 串；boot-probe 由 svcmgr 追加 argv 派生。
    pub fn read(env_ch: Handle, config: RoomConfig) -> RoomResult<Self> {
        let mut blob = [0u8; 192];
        let mut handles = [0 as Handle; 8];
        let mut nbytes = blob.len();
        let mut nhandles = handles.len();
        let st = berry_channel_recv(
            env_ch,
            blob.as_mut_ptr(),
            &mut nbytes,
            handles.as_mut_ptr(),
            &mut nhandles,
        );
        if st != 0 {
            return Err(RoomError::Env);
        }
        let minimum = if config.uses_fs { 7 } else { 5 };
        let broker_index = if config.uses_fs { 6 } else { 4 };
        let surface_token = if config.surface_escape {
            let token_index = broker_index + 1;
            if nhandles <= token_index || handles[token_index] == 0 {
                return Err(RoomError::Env);
            }
            handles[token_index]
        } else {
            0
        };
        if nhandles < minimum
            || nbytes <= config.argv0.len()
            || &blob[..config.argv0.len()] != config.argv0
            || handles[0] == 0
            || handles[2] == 0
            || handles[3] == 0
            || handles[broker_index] == 0
            || (config.uses_fs && handles[5] == 0)
        {
            return Err(RoomError::Env);
        }
        let boot_probe = blob[..nbytes]
            .windows(b"\0boot-probe".len())
            .any(|window| window == b"\0boot-probe");
        Ok(Self {
            boot_probe,
            console: handles[0],
            process: handles[2],
            vmar: handles[3],
            broker: handles[broker_index],
            fs: if config.uses_fs { handles[5] } else { 0 },
            surface_token,
        })
    }
}

/// Native Shell 宿主。成功 `open_window` 后持有一个全屏窗口与其局部
/// frame VMO；`destroy` 显式走 WM_DESTROY + 空表回读 + 资源关闭。
pub struct AppRoom {
    env: RoomEnv,
    wm: Handle,
    input: Handle,
    font_vmo: Handle,
    frame_vmo: Handle,
    window: u16,
    width: usize,
    height: usize,
    stride: usize,
    frame_bytes: usize,
    frame_map_len: usize,
    surface: Option<gp::surface::SurfaceModeInfo>,
    /// R2 create-time persistent occlusion 快照。TABLE_GET 是 v1 stopgap：
    /// 无 push config event，SYSTEM 窗在其后出现时此值可陈旧，直到下次
    /// 0x67 或未来的 R3 config 事件。
    occlusion: gp::insets::OcclusionSnapshot,
    /// 最近一次光栅化 dirty 矩形（None = FULL）。submit 用它替代恒
    /// FULL，避免一个 Switch/文本状态触发整屏三次拷贝。
    last_dirty: Option<gp::Rect>,
    opened: bool,
}

fn surface_grant_from_info(info: &gp::surface::SurfaceModeInfo) -> SurfaceGrant {
    SurfaceGrant {
        mode: match info.mode {
            gp::surface::SURFACE_MODE_CANVAS => SurfaceModeKind::Canvas,
            gp::surface::SURFACE_MODE_IMMERSIVE => SurfaceModeKind::Immersive,
            _ => SurfaceModeKind::Retained,
        },
        input_classes: info.input_classes,
        ime_mode: info.ime_mode,
        a11y_mode: info.a11y_mode,
        contract_flags: info.contract_flags,
        scale_permille: info.scale_permille,
        safe_left: info.safe_left,
        safe_top: info.safe_top,
        safe_right: info.safe_right,
        safe_bottom: info.safe_bottom,
    }
}

/// raw/visible 双矩形 → persistent occlusion snapshot。right/bottom 用
/// u32 差集，防 u16 原点+宽高越界回绕；派生值不落第二存储真值。
/// damage → present flags/rects（None 或零面积 = FULL 保守面）。
fn submit_damage(
    dirty: Option<gp::Rect>,
) -> (u8, [gp::Rect; gp::DAMAGE_MAX], usize) {
    let mut rects = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
    match dirty {
        Some(rect) if rect.w > 0 && rect.h > 0 => {
            rects[0] = rect;
            (0, rects, 1)
        }
        _ => (gp::SUBMIT_F_DAMAGE_FULL, rects, 0),
    }
}

fn occlusion_from_bounds(
    raw: (u16, u16, u16, u16),
    visible: (u16, u16, u16, u16),
) -> gp::insets::OcclusionSnapshot {
    let left = visible.0.saturating_sub(raw.0);
    let top = visible.1.saturating_sub(raw.1);
    let raw_right = u32::from(raw.0) + u32::from(raw.2);
    let visible_right = u32::from(visible.0) + u32::from(visible.2);
    let raw_bottom = u32::from(raw.1) + u32::from(raw.3);
    let visible_bottom = u32::from(visible.1) + u32::from(visible.3);
    let right = raw_right.saturating_sub(visible_right);
    let bottom = raw_bottom.saturating_sub(visible_bottom);
    gp::insets::OcclusionSnapshot::new(
        [left, top, right as u16, bottom as u16],
        [0; 4],
    )
}

impl AppRoom {
    /// App-visible monotonic clock. Gesture helpers need an explicit time
    /// source; this does not grant WM, present, HDI, or SYSTEM capability.
    pub fn now_ns(&self) -> u64 {
        now_ns()
    }

    /// 解析 env、建立 WM/input session、接收并映射 wmd 私有字形 VMO。
    pub fn connect(env_ch: Handle, config: RoomConfig) -> RoomResult<Self> {
        let env = RoomEnv::read(env_ch, config)?;
        let (wm, wm_peer) = pair()?;
        let (input, input_peer) = pair()?;
        let mut request = [0u8; 256];
        let mut reply = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let n = gp::encode_wm_connect_req(&mut request, gp::WM_CLIENT_APP)
            .ok_or(RoomError::Encode)?;
        let attach = if config.surface_escape {
            [wm_peer, input_peer, env.process, env.surface_token]
        } else {
            [wm_peer, input_peer, env.process, 0]
        };
        let attach_count = if config.surface_escape { 4 } else { 3 };
        let (n, nhandles) = rpc(
            env.broker,
            &request[..n],
            &attach[..attach_count],
            &mut reply,
            &mut rhs,
        )?;
        if gp::parse_wm_connect_rep(&reply[..n]).is_none()
            || nhandles != 1
            || rhs[0] == 0
        {
            let _ = berry_handle_close(wm_peer);
            let _ = berry_handle_close(input_peer);
            return Err(RoomError::Decode);
        }
        let font_vmo = rhs[0];
        let mut font_va = FONT_VA;
        if berry_vmar_map(
            env.vmar,
            0,
            font_vmo,
            0,
            &mut font_va,
            FONT_VMO_LEN,
            RIGHT_READ | RIGHT_WRITE,
        ) != 0 || font_va != FONT_VA
        {
            return Err(RoomError::FontMap);
        }
        if berry_handle_close(env.broker) != 0 {
            return Err(RoomError::Handle);
        }
        Ok(Self {
            env,
            wm,
            input,
            font_vmo,
            frame_vmo: 0,
            window: 0,
            width: 0,
            height: 0,
            stride: 0,
            frame_bytes: 0,
            frame_map_len: 0,
            surface: None,
            occlusion: gp::insets::OcclusionSnapshot::new([0; 4], [0; 4]),
            last_dirty: None,
            opened: false,
        })
    }

    pub fn console(&self) -> Handle {
        self.env.console
    }

    pub fn fs_handle(&self) -> Handle {
        self.env.fs
    }

    pub fn boot_probe(&self) -> bool {
        self.env.boot_probe
    }

    pub fn print(&self, bytes: &[u8]) {
        let deadline = now_ns().checked_add(RPC_CAP_NS).unwrap_or(u64::MAX);
        loop {
            let st = berry_channel_send(
                self.env.console,
                bytes.as_ptr(),
                bytes.len(),
                core::ptr::null(),
                0,
            );
            if st == 0 {
                return;
            }
            if st != BERRY_ERR_BUSY {
                berry_process_exit(134);
            }
            let mut observed = 0u64;
            if berry_object_wait_one(
                self.env.console,
                SIGNAL_WRITABLE,
                deadline,
                &mut observed,
            ) != 0
            {
                berry_process_exit(134);
            }
        }
    }

    /// P1：对当前窗口显式协商 Surface Mode。普通 Room 未携带 escape
    /// token 时，wmd 对 Canvas/Immersive 返回 1B 拒绝；本 Room 不自报
    /// System role，不携带 safe area/scale。
    pub fn configure_surface(&mut self, request: SurfaceRequest) -> RoomResult<SurfaceGrant> {
        if !self.opened || self.window == 0 {
            return Err(RoomError::Window);
        }
        let mode = match request.mode {
            SurfaceModeKind::Retained => gp::surface::SURFACE_MODE_RETAINED,
            SurfaceModeKind::Canvas => gp::surface::SURFACE_MODE_CANVAS,
            SurfaceModeKind::Immersive => gp::surface::SURFACE_MODE_IMMERSIVE,
        };
        let wire = gp::surface::SurfaceModeReq {
            surface_id: self.window,
            role: gp::surface::SURFACE_ROLE_APP,
            mode,
            input_classes: request.input_classes,
            ime_mode: request.ime_mode,
            a11y_mode: request.a11y_mode,
        };
        let mut buf = [0u8; gp::surface::SURFACE_MODE_REQ_LEN];
        let n = gp::surface::encode_wm_surface_mode_req(&mut buf, &wire)
            .ok_or(RoomError::Encode)?;
        let mut reply = [0u8; 256];
        let mut handles = [0 as Handle; 8];
        let (n, nhandles) = rpc(self.wm, &buf[..n], &[], &mut reply, &mut handles)?;
        if nhandles != 0 {
            return Err(RoomError::Decode);
        }
        let info = match gp::surface::parse_wm_surface_mode_rep(&reply[..n]) {
            Some(Ok(info)) => info,
            Some(Err(gp::ST_UNSUPPORTED)) => return Err(RoomError::SurfaceUnsupported),
            Some(_) => return Err(RoomError::SurfaceDenied),
            None => return Err(RoomError::SurfaceBad),
        };
        if !gp::surface::surface_mode_grant_matches(&wire, &info)
            || !gp::surface::surface_mode_info_fits(
                &info,
                self.width as u32,
                self.height as u32,
            )
        {
            return Err(RoomError::SurfaceBad);
        }
        self.surface = Some(info);
        self.occlusion = gp::insets::OcclusionSnapshot::new(
            [info.safe_left, info.safe_top, info.safe_right, info.safe_bottom],
            [0; 4],
        );
        Ok(surface_grant_from_info(&info))
    }

    /// 当前窗口物理像素尺寸（safe area 未扣除）。
    pub fn window_size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// 最近一次显式 Surface 授予；None = 默认 Retained。
    pub fn surface_grant(&self) -> Option<SurfaceGrant> {
        self.surface.map(|i| surface_grant_from_info(&i))
    }

    /// R2 create-time persistent occlusion 快照（raw surface-local）。
    /// `effective_padding()` 当前等于 view_padding；IME transient 域为 0，
    /// 等 P6b IME Host 才有生产者。
    pub fn occlusion(&self) -> gp::insets::OcclusionSnapshot {
        self.occlusion
    }

    /// R3 非阻塞拉一条本 session pending config event。空批返回 `None`；
    /// 不新增 handle、不占用 TABLE_WAIT，适配 Room 严格串行 RPC。
    pub fn poll_config(&mut self) -> RoomResult<Option<WindowConfigEvent>> {
        let mut request = [0u8; 256];
        let mut reply = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let n = gp::encode_wm_config_poll_req(&mut request);
        let (n, _) = rpc(self.wm, &request[..n], &[], &mut reply, &mut rhs)?;
        let event = match gp::parse_wm_config_event_rep(&reply[..n]) {
            Some(Ok(ev)) => {
                let cause = match ev.cause {
                    gp::WM_CONFIG_CAUSE_CREATED => ConfigCause::Created,
                    gp::WM_CONFIG_CAUSE_GEOMETRY => ConfigCause::Geometry,
                    gp::WM_CONFIG_CAUSE_VISIBILITY => ConfigCause::Visibility,
                    _ => ConfigCause::Occlusion,
                };
                Ok(Some(WindowConfigEvent {
                    serial: ev.serial,
                    window_id: ev.window_id,
                    cause,
                    visible: ev.visible,
                    raw: (ev.raw_x, ev.raw_y, ev.raw_w, ev.raw_h),
                    visible_bounds: (
                        ev.visible_x, ev.visible_y, ev.visible_w, ev.visible_h,
                    ),
                }))
            }
            Some(Err(gp::ST_EAGAIN)) => Ok(None),
            Some(Err(gp::ST_BAD)) => Err(RoomError::ConfigBad),
            Some(Err(_)) => Err(RoomError::ConfigDenied),
            None => Err(RoomError::Decode),
        }?;
        if let Some(event) = &event {
            // Room v1 是单窗 + 固定 raw display VMO：动态 GEOMETRY 若改
            // raw bounds，不能静默重解释既有 buffer；OCCLUSION/VISIBILITY
            // 则刷新派生 snapshot。open_window 的 CREATED pull-once 在
            // opened=false 阶段由调用方校验并 ACK。
            if self.opened {
                let current_raw = (
                    0u16,
                    0u16,
                    self.width as u16,
                    self.height as u16,
                );
                if event.window_id != self.window || event.raw != current_raw {
                    return Err(RoomError::ConfigBad);
                }
                self.occlusion =
                    occlusion_from_bounds(event.raw, event.visible_bounds);
            }
        }
        Ok(event)
    }

    /// R3 ACK：只确认当前 pending 精确 serial；错/旧 serial 由 wmd 拒绝。
    pub fn ack_config(&mut self, event: &WindowConfigEvent) -> RoomResult<()> {
        let mut request = [0u8; 256];
        let mut reply = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let wire = gp::WmConfigAckReq {
            window_id: event.window_id,
            serial: event.serial,
        };
        let n = gp::encode_wm_config_ack_req(&mut request, &wire)
            .ok_or(RoomError::Encode)?;
        let (n, _) = rpc(self.wm, &request[..n], &[], &mut reply, &mut rhs)?;
        match gp::parse_status_rep(&reply[..n]) {
            Some(gp::ST_OK) => Ok(()),
            Some(gp::ST_BAD) => Err(RoomError::ConfigBad),
            Some(_) => Err(RoomError::ConfigDenied),
            None => Err(RoomError::Decode),
        }
    }

    /// Android-style edge-to-edge default: raw content extends beneath the
    /// persistent status/navigation overlays while Surface Mode still returns
    /// authoritative safe insets for app layout.
    pub fn open_window(&mut self) -> RoomResult<(usize, usize)> {
        self.open_window_with_flags(gp::WM_CREATE_F_EDGE_TO_EDGE)
    }

    /// 创建沉浸全屏窗口：显式请求隐藏持久系统保留区。普通 app 仍只获
    /// Berry Room 授权面，不取得 present/HDI/SYSTEM 能力。
    pub fn open_window_immersive(&mut self) -> RoomResult<(usize, usize)> {
        self.open_window_with_flags(0)
    }

    fn open_window_with_flags(&mut self, create_flags: u8) -> RoomResult<(usize, usize)> {
        if self.opened {
            return Err(RoomError::Window);
        }
        let mut request = [0u8; 256];
        let mut reply = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let n = gp::encode_wm_getinfo_req(&mut request);
        let (n, _) = rpc(self.wm, &request[..n], &[], &mut reply, &mut rhs)?;
        let info = gp::parse_wm_getinfo_rep(&reply[..n]).ok_or(RoomError::Decode)?;
        let create = gp::WmCreateReq {
            layer: gp::WM_LAYER_APP,
            flags: create_flags,
            parent: 0,
            group: 0,
            x: 0,
            y: 0,
            w: info.disp_w,
            h: info.disp_h,
        };
        let n = gp::encode_wm_create_req(&mut request, &create).ok_or(RoomError::Encode)?;
        let (n, _) = rpc(self.wm, &request[..n], &[], &mut reply, &mut rhs)?;
        let window = match gp::parse_wm_create_ack(&reply[..n]) {
            Some(id) if id != 0 => id,
            _ => return Err(RoomError::Decode),
        };
        // R3 initial config pull-once：CREATE 后必有一条 CREATED event。
        // Room 不再用全表 TABLE_GET 推导初始 occlusion，避免把整表信息披露
        // 给普通 app；事件 raw/visible 双矩形就是本窗单一几何真值。
        let event = self.poll_config()?.ok_or(RoomError::Table)?;
        if event.window_id != window
            || event.cause != ConfigCause::Created
            || !event.visible
            || event.raw != (0, 0, info.disp_w, info.disp_h)
        {
            return Err(RoomError::ConfigBad);
        }
        let occlusion = occlusion_from_bounds(event.raw, event.visible_bounds);
        self.ack_config(&event)?;
        let width = usize::from(info.disp_w);
        let height = usize::from(info.disp_h);
        let stride = width.checked_mul(4).ok_or(RoomError::Geometry)?;
        let frame_bytes = stride.checked_mul(height).ok_or(RoomError::Geometry)?;
        let map_len = frame_bytes
            .checked_add(0xFFF)
            .and_then(|v| v.checked_sub(v & 0xFFF))
            .ok_or(RoomError::Geometry)?;
        let mut frame_vmo: Handle = 0;
        if berry_vmo_create(map_len as u64, 0, &mut frame_vmo) != 0
            || frame_vmo == 0
            || berry_vmo_commit(frame_vmo, 0, map_len as u64) != 0
        {
            return Err(RoomError::Vmo);
        }
        let mut va = FRAME_VA;
        if berry_vmar_map(
            self.env.vmar,
            0,
            frame_vmo,
            0,
            &mut va,
            map_len,
            RIGHT_READ | RIGHT_WRITE,
        ) != 0 || va != FRAME_VA
        {
            let _ = berry_handle_close(frame_vmo);
            return Err(RoomError::Map);
        }
        self.window = window;
        self.width = width;
        self.height = height;
        self.stride = stride;
        self.frame_bytes = frame_bytes;
        self.frame_map_len = map_len;
        self.frame_vmo = frame_vmo;
        self.occlusion = occlusion;
        self.last_dirty = None;
        self.opened = true;
        if create_flags & gp::WM_CREATE_F_EDGE_TO_EDGE != 0 {
            let grant = self.configure_surface(SurfaceRequest {
                mode: SurfaceModeKind::Retained,
                input_classes: SURFACE_INPUT_TOUCH,
                ime_mode: SURFACE_IME_DISABLED,
                a11y_mode: SURFACE_A11Y_TREE,
            })?;
            self.occlusion = gp::insets::OcclusionSnapshot::new(
                [
                    grant.safe_left,
                    grant.safe_top,
                    grant.safe_right,
                    grant.safe_bottom,
                ],
                [0; 4],
            );
        }
        Ok((width, height))
    }

    pub fn size(&self) -> RoomResult<(usize, usize)> {
        if !self.opened {
            return Err(RoomError::Window);
        }
        Ok((self.width, self.height))
    }

    /// 将 BerryUI/BerryGFX DisplayList 光栅化到本 Room 的 frame VMO。
    /// app 不接触 stride、frame 指针或像素循环；这是 Berry Room 的渲染
    /// 宿主面。返回 DisplayList 命令数。
    pub fn render_display_list(&mut self, dl: &DisplayList<'_>) -> RoomResult<usize> {
        self.render_display_list_damaged(dl, None)
    }

    /// 按 embedder 提供的 dirty 矩形光栅化并记录；随后同名 `submit` 会
    /// 把同一面转成 present damage。`None` = FULL/未知，保守整帧。
    pub fn render_display_list_damaged(
        &mut self,
        dl: &DisplayList<'_>,
        damage: Option<&gp::Rect>,
    ) -> RoomResult<usize> {
        if !self.opened {
            return Err(RoomError::Window);
        }
        let desc = TextureDesc::new(
            self.width,
            self.height,
            self.stride,
            TextureFormat::Rgbx,
        )
        .map_err(|_| RoomError::Geometry)?;
        let dirty = damage.map(|r| {
            let x = i32::try_from(r.x).unwrap_or(i32::MAX);
            let y = i32::try_from(r.y).unwrap_or(i32::MAX);
            let w = i32::try_from(r.w).unwrap_or(i32::MAX);
            let h = i32::try_from(r.h).unwrap_or(i32::MAX);
            berry_gfx::cmd::Rect::new(x, y, w, h)
        });
        let frame = self.frame_mut()?;
        let pass = PassDescriptor::new(LoadOp::Load, StoreOp::Store, dirty);
        Queue::new()
            .submit(dl, frame, &desc, &pass)
            .map_err(|_| RoomError::Geometry)?;
        self.last_dirty = damage.copied();
        Ok(dl.len())
    }

    /// 借出窗口局部 XRGB buffer。P1 起私有：Retained 只能经
    /// `render_display_list` 写入；业务像素写入必须先取得 Canvas/Immersive
    /// 授权，再走 `surface_frame_mut`。
    fn frame_mut(&mut self) -> RoomResult<&mut [u8]> {
        if !self.opened {
            return Err(RoomError::Window);
        }
        // SAFETY：open_window 成功后 FRAME_VA..+frame_bytes 是本进程独占
        // 映射；&mut self 保证调用方借用与后续 host 方法互斥。
        Ok(unsafe {
            core::slice::from_raw_parts_mut(FRAME_VA as *mut u8, self.frame_bytes)
        })
    }

    /// P1：授权 Canvas/Immersive 的像素借用。返回 XRGB 窗口 buffer，
    /// stride/映射/VMO handle 均留在 Room；Retained app 调用恒拒绝。
    pub fn surface_frame_mut(&mut self) -> RoomResult<&mut [u8]> {
        match self.surface.map(|i| i.mode) {
            Some(gp::surface::SURFACE_MODE_CANVAS)
            | Some(gp::surface::SURFACE_MODE_IMMERSIVE) => self.frame_mut(),
            _ => Err(RoomError::SurfaceDenied),
        }
    }

    /// SUBMIT → FRAME_CALLBACK → RELEASE_WAIT。返回 frame token。
    pub fn submit(&mut self, buffer_id: u32) -> RoomResult<u32> {
        if !self.opened {
            return Err(RoomError::Window);
        }
        // R3 帧边界：SUBMIT 前排空并 ACK config，避免动态 OCCLUSION/
        // VISIBILITY 永久阻塞 app。需要按 insets 重排的业务应在构建
        // DisplayList 前调用 poll_config；这里至少保证 Room 不会带着
        // pending config 进入下一帧。
        loop {
            let event = match self.poll_config()? {
                Some(event) => event,
                None => break,
            };
            match self.ack_config(&event) {
                Ok(()) => continue,
                // pending 在 POLL→ACK 间被并发表变更覆盖：重新 POLL 最新
                // full-state，不做排序猜测。
                Err(RoomError::ConfigDenied) => continue,
                Err(e) => return Err(e),
            }
        }
        let mut alias: Handle = 0;
        if berry_handle_duplicate(self.frame_vmo, RIGHT_READ, &mut alias) != 0 || alias == 0 {
            return Err(RoomError::Handle);
        }
        let mut request = [0u8; 256];
        let mut reply = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let (flags, rects, rect_count) = submit_damage(self.last_dirty);
        let submit = gp::PresentReq::Submit {
            surface_id: u32::from(self.window),
            buffer_id,
            flags,
            rate_hint: 0,
            target_present_ns: 0,
            rects,
            rect_count,
        };
        let n = gp::encode_present_req(&mut request, &submit).ok_or(RoomError::Encode)?;
        let (n, _) = rpc(self.wm, &request[..n], &[alias], &mut reply, &mut rhs)?;
        let token = gp::parse_submit_ack(&reply[..n]).ok_or(RoomError::SubmitAck)?;
        let n = gp::encode_present_req(
            &mut request,
            &gp::PresentReq::FrameCallback { frame_token: token },
        )
        .ok_or(RoomError::Encode)?;
        let (n, _) = rpc(self.wm, &request[..n], &[], &mut reply, &mut rhs)?;
        match gp::parse_frame_done_rep(&reply[..n]) {
            Some(done) if done.status == gp::FRAME_PRESENTED => {}
            _ => return Err(RoomError::FrameDone),
        }
        let n = gp::encode_present_req(
            &mut request,
            &gp::PresentReq::ReleaseWait,
        )
        .ok_or(RoomError::Encode)?;
        let (n, nhandles) = rpc(self.wm, &request[..n], &[], &mut reply, &mut rhs)?;
        let release = gp::parse_release_rep(&reply[..n]).ok_or(RoomError::ReleaseReply)?;
        // RELEASE_WAIT 是 RETURN-ALL：guifbd 会排空聚合 held 表。0..=2
        // 都是合法形态——其它 present session 可先清账（0）或先提交（2）。
        // 契约面只校验 count 与随行 handle 恰等；全部随行别名都关闭。
        let count = release.count as usize;
        if count != nhandles || count > rhs.len() {
            return Err(RoomError::ReleaseReply);
        }
        for handle in rhs.iter().take(count) {
            if *handle == 0 || berry_handle_close(*handle) != 0 {
                return Err(RoomError::Handle);
            }
        }
        Ok(token)
    }

    /// 非阻塞 INPUT_POLL。空批返回 `Ok(0)`；返回数为复制到 `out` 的事件数。
    pub fn poll_input(&mut self, out: &mut [PointerEvent]) -> RoomResult<usize> {
        if out.is_empty() {
            return Err(RoomError::Input);
        }
        let max = out.len().min(gp::INPUT_EVENTS_MAX) as u8;
        let mut request = [0u8; 4];
        let n = gp::encode_input_poll_req(&mut request, max);
        let mut reply = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let (n, _) = rpc(self.input, &request[..n], &[], &mut reply, &mut rhs)?;
        match gp::parse_input_events_rep(&reply[..n]) {
            Some(Ok(events)) => {
                let count = events.count();
                if count > out.len() {
                    return Err(RoomError::Input);
                }
                for i in 0..count {
                    out[i] = events.event(i).ok_or(RoomError::Decode)?;
                }
                Ok(count)
            }
            Some(Err(_)) => Ok(0),
            None => Err(RoomError::Decode),
        }
    }

    /// 阻塞等待第一个 Phase_UP 事件。返回事件原样，坐标已是窗口局部。
    pub fn wait_pointer_up(&mut self) -> RoomResult<PointerEvent> {
        let mut events = [PointerEvent {
            phase: gp::PHASE_CANCEL,
            device_id: 0,
            buttons: 0,
            flags: 0,
            pointer_id: 0,
            timestamp_ns: 0,
            x: 0,
            y: 0,
            pressure: gp::PRESSURE_UNAVAILABLE,
            contact_major: 0,
            contact_minor: 0,
            history: [gp::Sample { timestamp_ns: 0, x: 0, y: 0 };
                gp::INPUT_SAMPLES_MAX - 1],
            history_len: 0,
        }; gp::INPUT_EVENTS_MAX];
        loop {
            let count = self.poll_input(&mut events)?;
            for event in events.iter().take(count) {
                if event.phase == gp::PHASE_UP {
                    return Ok(*event);
                }
            }
        }
    }

    /// P2 非阻塞键盘事件拉取。空批返回 `Ok(0)`；事件按 wmd focus +
    /// Surface input_classes 授权后入队。
    pub fn poll_keys(&mut self, out: &mut [KeyEvent]) -> RoomResult<usize> {
        if out.is_empty() {
            return Err(RoomError::Input);
        }
        let max = out.len().min(gp::INPUT_KEY_EVENTS_MAX) as u8;
        let mut request = [0u8; 4];
        let n = gp::encode_input_key_poll_req(&mut request, max);
        let mut reply = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let (n, _) = rpc(self.input, &request[..n], &[], &mut reply, &mut rhs)?;
        match gp::parse_input_key_events_rep(&reply[..n]) {
            Some(Ok(events)) => {
                let count = events.count();
                if count > out.len() {
                    return Err(RoomError::Input);
                }
                for i in 0..count {
                    out[i] = events.event(i).ok_or(RoomError::Decode)?;
                }
                Ok(count)
            }
            Some(Err(_)) => Ok(0),
            None => Err(RoomError::Decode),
        }
    }

    /// 显式销毁窗口、回读确认本窗口表项消失、解除映射并关闭宿主资源。
    pub fn destroy(&mut self) -> RoomResult<()> {
        if !self.opened {
            return Err(RoomError::Destroy);
        }
        let mut request = [0u8; 256];
        let mut reply = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let n = gp::encode_wm_destroy_req(&mut request, self.window);
        let (n, _) = rpc(self.wm, &request[..n], &[], &mut reply, &mut rhs)?;
        match gp::parse_status_rep(&reply[..n]) {
            Some(gp::ST_OK) => {}
            _ => return Err(RoomError::Destroy),
        }
        let n = gp::encode_wm_table_get_req(&mut request);
        let (n, _) = rpc(self.wm, &request[..n], &[], &mut reply, &mut rhs)?;
        // 并发 GUI app 生命周期下，全局空表是过强断言：其它 boot probe 或
        // 常驻窗口可以合法存在。本 Room 的清理真值 = 自己的 window_id 已
        // 从权威表消失；表快照畸形/无法解析仍 loud 拒。
        match gp::parse_wm_snapshot_rep(&reply[..n]) {
            Some(Ok(snapshot)) => {
                let mine = u16::from(self.window);
                if snapshot.entries.iter().take(snapshot.count).any(|e| e.id == mine) {
                    return Err(RoomError::Table);
                }
            }
            _ => return Err(RoomError::Table),
        }
        // 映射先解除，再关句柄；失败不掩盖 WM 已销毁的真相，统一 Destroy。
        if berry_vmar_unmap(self.env.vmar, FRAME_VA, self.frame_map_len) != 0
            || berry_vmar_unmap(self.env.vmar, FONT_VA, FONT_VMO_LEN) != 0
            || berry_handle_close(self.input) != 0
            || berry_handle_close(self.wm) != 0
            || berry_handle_close(self.frame_vmo) != 0
            || berry_handle_close(self.font_vmo) != 0
        {
            return Err(RoomError::Destroy);
        }
        self.opened = false;
        self.surface = None;
        self.input = 0;
        self.wm = 0;
        self.frame_vmo = 0;
        self.font_vmo = 0;
        Ok(())
    }

    /// 宿主侧统一退出。app 不直接调用 process syscall。
    pub fn exit(&self, code: i32) -> ! {
        berry_process_exit(code)
    }
}

#[cfg(test)]
mod tests {
    use super::{occlusion_from_bounds, submit_damage};
    use berry_guiproto as gp;

    #[test]
    fn partial_damage_becomes_one_wire_rect() {
        let (flags, rects, n) = super::submit_damage(Some(gp::Rect {
            x: 12,
            y: 34,
            w: 56,
            h: 78,
        }));
        assert_eq!(flags, 0);
        assert_eq!(n, 1);
        assert_eq!(
            rects[0],
            gp::Rect { x: 12, y: 34, w: 56, h: 78 }
        );
    }

    #[test]
    fn full_or_empty_damage_stays_conservative_full() {
        for dirty in [
            None,
            Some(gp::Rect { x: 0, y: 0, w: 0, h: 10 }),
            Some(gp::Rect { x: 0, y: 0, w: 10, h: 0 }),
        ] {
            let (flags, rects, n) = super::submit_damage(dirty);
            assert_eq!(flags, gp::SUBMIT_F_DAMAGE_FULL);
            assert_eq!(n, 0);
            assert_eq!(rects[0], gp::Rect { x: 0, y: 0, w: 0, h: 0 });
        }
    }

    #[test]
    fn r2_occlusion_snapshot_matches_raw_bounds() {
        let none = occlusion_from_bounds((0, 0, 800, 600), (0, 0, 800, 600));
        assert_eq!(none.view_padding, [0, 0, 0, 0]);
        assert_eq!(none.view_insets, [0, 0, 0, 0]);
        assert_eq!(none.effective_padding(), [0, 0, 0, 0]);

        let top = occlusion_from_bounds((0, 0, 800, 600), (0, 32, 800, 568));
        assert_eq!(top.view_padding, [0, 32, 0, 0]);
        assert_eq!(top.effective_padding(), [0, 32, 0, 0]);

        let left_bottom = occlusion_from_bounds((0, 0, 800, 600), (24, 0, 776, 568));
        assert_eq!(left_bottom.view_padding, [24, 0, 0, 32]);
    }
}
