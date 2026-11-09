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
        }
    }
}

/// 公开输入事件类型，避免 app 直接依赖/解析 GUI wire 协议。
pub use berry_guiproto::{
    INPUT_EVENTS_MAX, INPUT_SAMPLES_MAX, PHASE_UP, PointerEvent, Sample,
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
    opened: bool,
}

impl AppRoom {
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
        let attach = [wm_peer, input_peer, env.process];
        let (n, nhandles) = rpc(env.broker, &request[..n], &attach, &mut reply, &mut rhs)?;
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

    /// 创建全屏窗口并映射局部 frame VMO。返回 `(width, height)`。
    pub fn open_window(&mut self) -> RoomResult<(usize, usize)> {
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
            flags: 0,
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
        self.opened = true;
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
        let frame = self.frame_mut()?;
        let pass = PassDescriptor::new(LoadOp::Load, StoreOp::Store, None);
        Queue::new()
            .submit(dl, frame, &desc, &pass)
            .map_err(|_| RoomError::Geometry)?;
        Ok(dl.len())
    }

    /// 借出窗口局部 XRGB buffer。宿主不写它；业务渲染归上层 UI/光栅。
    pub fn frame_mut(&mut self) -> RoomResult<&mut [u8]> {
        if !self.opened {
            return Err(RoomError::Window);
        }
        // SAFETY：open_window 成功后 FRAME_VA..+frame_bytes 是本进程独占
        // 映射；&mut self 保证调用方借用与后续 host 方法互斥。
        Ok(unsafe {
            core::slice::from_raw_parts_mut(FRAME_VA as *mut u8, self.frame_bytes)
        })
    }

    /// SUBMIT → FRAME_CALLBACK → RELEASE_WAIT。返回 frame token。
    pub fn submit(&mut self, buffer_id: u32) -> RoomResult<u32> {
        if !self.opened {
            return Err(RoomError::Window);
        }
        let mut alias: Handle = 0;
        if berry_handle_duplicate(self.frame_vmo, RIGHT_READ, &mut alias) != 0 || alias == 0 {
            return Err(RoomError::Handle);
        }
        let mut request = [0u8; 256];
        let mut reply = [0u8; 256];
        let mut rhs = [0 as Handle; 8];
        let submit = gp::PresentReq::Submit {
            surface_id: u32::from(self.window),
            buffer_id,
            flags: gp::SUBMIT_F_DAMAGE_FULL,
            rate_hint: 0,
            target_present_ns: 0,
            rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
            rect_count: 0,
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
        if release.count != 1 || nhandles != 1 || rhs[0] == 0 {
            return Err(RoomError::ReleaseReply);
        }
        if berry_handle_close(rhs[0]) != 0 {
            return Err(RoomError::Handle);
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
