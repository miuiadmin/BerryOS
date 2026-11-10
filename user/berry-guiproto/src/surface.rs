//! Berry Session Surface Mode 协议（GUI P0 / ADR #67：Surface 协商 +
//! IME composition + a11y semantic sidecar）。
//!
//! 一个系统契约承载三条 UI 路径：普通 BerryUI、专业 CanvasSurface、游戏
//! immersive surface。本模块只定义并编解码 WM 域的形态请求/授予帧、IME
//! composition 旁车帧与 a11y semantic sidecar 单记录；窗口所有权、焦点、
//! frame lifecycle 与进程死亡清理仍
//! 由既有 WM/present/input 域承担，BerryUI 只是 Retained 形态的默认工具
//! 包，不是特权渲染器。
//!
//! wire 纪律与父模块一致：小端、定长、零分配、零 unsafe；未知值/未知位/
//! 截断/尾随字节一律拒绝。IME/a11y 的保留域与尾部 padding 恒 0 且双侧
//! 执法；WM 授予帧保留位沿用父模块前向兼容约定（编码恒 0、解析不校验）。
//! safe area 以物理像素记录；`scale_permille` 以 1000 表示 1.0×
//! （500..=4000 合法），用于逻辑几何与物理像素之间的单向换算。帧率意图
//! 继续走 present VOTE，不在本模块复制。

use super::{
    le16, st_known, wm_client_known, WM_CLIENT_APP, WM_CLIENT_SYSTEM,
    OP_WM_SURFACE_ESCAPE_SEED, OP_WM_SYSTEM_SEED, OP_WM_SURFACE_MODE, ST_BAD, ST_DENIED,
    ST_OK, ST_UNSUPPORTED,
};

/// Surface 角色：普通 app surface。三条 P0 路径都在该角色下。
pub const SURFACE_ROLE_APP: u8 = 0;
/// Surface 角色：系统壳/系统层 surface。v1 只允许 Retained 形态。
pub const SURFACE_ROLE_SYSTEM: u8 = 1;

/// 形态：BerryUI retained tree / DisplayList。
pub const SURFACE_MODE_RETAINED: u8 = 0;
/// 形态：专业应用文档 CanvasSurface escape hatch。
pub const SURFACE_MODE_CANVAS: u8 = 1;
/// 形态：游戏/实时应用 immersive surface escape hatch。
pub const SURFACE_MODE_IMMERSIVE: u8 = 2;

/// 输入类：触摸。
pub const INPUT_CLASS_TOUCH: u8 = 1 << 0;
/// 输入类：笔。
pub const INPUT_CLASS_STYLUS: u8 = 1 << 1;
/// 输入类：鼠标。
pub const INPUT_CLASS_MOUSE: u8 = 1 << 2;
/// 输入类：键盘。
pub const INPUT_CLASS_KEYBOARD: u8 = 1 << 3;
/// 输入类：手柄。
pub const INPUT_CLASS_GAMEPAD: u8 = 1 << 4;
/// v1 已定义输入类全集。
pub const INPUT_CLASS_KNOWN: u8 = INPUT_CLASS_TOUCH
    | INPUT_CLASS_STYLUS
    | INPUT_CLASS_MOUSE
    | INPUT_CLASS_KEYBOARD
    | INPUT_CLASS_GAMEPAD;

/// IME：该 surface 无文本输入，显式声明 disabled；不是绕过 composer。
pub const IME_MODE_DISABLED: u8 = 0;
/// IME：文本经 host composer 与 composition 旁车事件交付。
pub const IME_MODE_HOST: u8 = 1;

/// a11y sidecar：retained 语义树（BerryUI AST/a11y 表的消费面）。
pub const A11Y_MODE_TREE: u8 = 0;
/// a11y sidecar：canvas/immersive 语义区域表。
pub const A11Y_MODE_REGIONS: u8 = 1;
/// a11y sidecar：BerryUI 外壳语义树 + 文档 canvas 区域表（专业应用）。
pub const A11Y_MODE_HYBRID: u8 = 2;

/// 合约位：焦点仍由 Berry Session 仲裁。
pub const SURFACE_F_FOCUS: u8 = 1 << 0;
/// 合约位：帧率意图仍走 present VOTE。
pub const SURFACE_F_RATE: u8 = 1 << 1;
/// 合约位：owner 死亡仍触发窗口/surface/输入队列清理。
pub const SURFACE_F_CLEANUP: u8 = 1 << 2;
/// 合约位：语义 a11y sidecar 必须存在。
pub const SURFACE_F_A11Y: u8 = 1 << 3;
/// 合约位：canvas/immersive escape hatch 已授权。
pub const SURFACE_F_ESCAPE: u8 = 1 << 4;
/// v1 已定义合约位全集。
pub const SURFACE_FLAG_KNOWN: u8 = SURFACE_F_FOCUS
    | SURFACE_F_RATE
    | SURFACE_F_CLEANUP
    | SURFACE_F_A11Y
    | SURFACE_F_ESCAPE;

/// scale 下界（0.5×）。
pub const SCALE_PERMILLE_MIN: u16 = 500;
/// scale 上界（4.0×）。
pub const SCALE_PERMILLE_MAX: u16 = 4_000;

/// P1 seed 控制帧长（op + 8B per-boot seed）。
pub const SURFACE_ESCAPE_SEED_REQ_LEN: usize = 9;
/// R1 SYSTEM client seed 控制帧长。
pub const WM_SYSTEM_SEED_REQ_LEN: usize = 9;

/// WM Surface Mode 请求帧长。
pub const SURFACE_MODE_REQ_LEN: usize = 8;
/// WM Surface Mode 成功授予帧长。
pub const SURFACE_MODE_REP_LEN: usize = 20;
/// IME composition 文本帽（UTF-8 字节数）。
pub const IME_TEXT_MAX: usize = 64;
/// IME composition 旁车帧长。
pub const IME_COMPOSITION_LEN: usize = 12 + IME_TEXT_MAX;

/// IME 事件：输入面隐藏。
pub const IME_PHASE_HIDE: u8 = 0;
/// IME 事件：输入面显示（composition 会话开始）。
pub const IME_PHASE_SHOW: u8 = 1;
/// IME 事件：composition 更新。
pub const IME_PHASE_UPDATE: u8 = 2;
/// IME 事件：文本提交。
pub const IME_PHASE_COMMIT: u8 = 3;
/// IME 事件：composition 取消。
pub const IME_PHASE_CANCEL: u8 = 4;

/// IME 用途：普通单行文本。
pub const IME_PURPOSE_TEXT: u8 = 0;
/// IME 用途：多行文本。
pub const IME_PURPOSE_MULTILINE: u8 = 1;
/// IME 用途：数字。
pub const IME_PURPOSE_NUMBER: u8 = 2;
/// IME 用途：密码/敏感输入（sidecar 不得把明文写入长期语义树）。
pub const IME_PURPOSE_PASSWORD: u8 = 3;

/// Surface Mode 请求。app 声明期望形态与语义旁车；safe area/scale 由 host
/// 授予，不允许客户端自报。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceModeReq {
    /// WM window/surface id；0 拒绝。一个 session 多窗时逐 surface 协商。
    pub surface_id: u16,
    pub role: u8,
    pub mode: u8,
    pub input_classes: u8,
    pub ime_mode: u8,
    pub a11y_mode: u8,
}

/// Host 授予的 Surface Mode 合约。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceModeInfo {
    pub surface_id: u16,
    pub role: u8,
    pub mode: u8,
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

/// IME composition 旁车事件。text 是 UTF-8；cursor_cp 是码点偏移。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImeComposition {
    pub revision: u32,
    pub phase: u8,
    pub purpose: u8,
    pub cursor_cp: u16,
    pub text_len: u8,
    pub text: [u8; IME_TEXT_MAX],
}

fn surface_role_known(v: u8) -> bool {
    v <= SURFACE_ROLE_SYSTEM
}

fn surface_mode_known(v: u8) -> bool {
    v <= SURFACE_MODE_IMMERSIVE
}

fn input_classes_known(v: u8) -> bool {
    v != 0 && v & !INPUT_CLASS_KNOWN == 0
}

fn ime_mode_known(v: u8) -> bool {
    v <= IME_MODE_HOST
}

fn a11y_mode_known(v: u8) -> bool {
    v <= A11Y_MODE_HYBRID
}

fn ime_phase_known(v: u8) -> bool {
    v <= IME_PHASE_CANCEL
}

fn ime_purpose_known(v: u8) -> bool {
    v <= IME_PURPOSE_PASSWORD
}

/// 请求词面与形态不变量。System 角色只允许 Retained；Retained 只能用语义
/// 树；Canvas 用区域或混合；Immersive 用区域。输入类不得为空。
pub fn surface_mode_req_valid(req: &SurfaceModeReq) -> bool {
    if req.surface_id == 0
        || !surface_role_known(req.role)
        || !surface_mode_known(req.mode)
        || !input_classes_known(req.input_classes)
        || !ime_mode_known(req.ime_mode)
        || !a11y_mode_known(req.a11y_mode)
    {
        return false;
    }
    if req.role == SURFACE_ROLE_SYSTEM && req.mode != SURFACE_MODE_RETAINED {
        return false;
    }
    match req.mode {
        SURFACE_MODE_RETAINED => req.a11y_mode == A11Y_MODE_TREE,
        SURFACE_MODE_CANVAS => {
            req.a11y_mode == A11Y_MODE_REGIONS || req.a11y_mode == A11Y_MODE_HYBRID
        }
        SURFACE_MODE_IMMERSIVE => req.a11y_mode == A11Y_MODE_REGIONS,
        _ => false,
    }
}

/// app 会话额外约束：不得自报 System 角色。系统壳 surface 由系统宿主创建。
pub fn surface_mode_req_app_valid(req: &SurfaceModeReq) -> bool {
    req.role == SURFACE_ROLE_APP && surface_mode_req_valid(req)
}

/// 授予合约不变量：所有形态共享焦点/帧率/死亡清理/a11y 语义；escape 位
/// 当且仅当 Canvas/Immersive 授权。safe area 与显示几何的对齐由
/// [`surface_mode_info_fits`] 单独执法。
pub fn surface_mode_info_valid(info: &SurfaceModeInfo) -> bool {
    let base = SurfaceModeReq {
        surface_id: info.surface_id,
        role: info.role,
        mode: info.mode,
        input_classes: info.input_classes,
        ime_mode: info.ime_mode,
        a11y_mode: info.a11y_mode,
    };
    if !surface_mode_req_valid(&base)
        || info.contract_flags & !SURFACE_FLAG_KNOWN != 0
        || !(SCALE_PERMILLE_MIN..=SCALE_PERMILLE_MAX).contains(&info.scale_permille)
    {
        return false;
    }
    let mandatory = SURFACE_F_FOCUS | SURFACE_F_RATE | SURFACE_F_CLEANUP | SURFACE_F_A11Y;
    match info.mode {
        SURFACE_MODE_RETAINED => info.contract_flags == mandatory,
        SURFACE_MODE_CANVAS | SURFACE_MODE_IMMERSIVE => {
            info.contract_flags == mandatory | SURFACE_F_ESCAPE
        }
        _ => false,
    }
}

/// 授予帧必须逐字段回应请求的 surface id/role/mode/input/IME/a11y；
/// host 只能新增 safe area、scale 与合约位，不能静默降级形态或语义旁车。
/// 不授权 escape hatch 时服务应回 1B ST_DENIED/ST_UNSUPPORTED，而非改发
/// Retained。
pub fn surface_mode_grant_matches(req: &SurfaceModeReq, info: &SurfaceModeInfo) -> bool {
    surface_mode_req_valid(req)
        && surface_mode_info_valid(info)
        && req.surface_id == info.surface_id
        && req.role == info.role
        && req.mode == info.mode
        && req.input_classes == info.input_classes
        && req.ime_mode == info.ime_mode
        && req.a11y_mode == info.a11y_mode
}

/// safe area 必须留出至少 1px 可用区（左右/上下不得吞满整个 surface）。
pub fn surface_mode_info_fits(info: &SurfaceModeInfo, width: u32, height: u32) -> bool {
    if !surface_mode_info_valid(info) || width == 0 || height == 0 {
        return false;
    }
    let left = u32::from(info.safe_left);
    let right = u32::from(info.safe_right);
    let top = u32::from(info.safe_top);
    let bottom = u32::from(info.safe_bottom);
    left.saturating_add(right) < width && top.saturating_add(bottom) < height
}

/// P1 production 授予策略（host 可测；gui-wmd 直接消费）：
/// - 无 escape 授权的 Canvas/Immersive → ST_DENIED；
/// - Gamepad/IME Host 无生产者或服务 → ST_UNSUPPORTED；Keyboard 由 P2 生产链承载；
/// - escape→escape 可重协商，降回 Retained → ST_DENIED；
/// - safe=0、scale=1000，合约位按 P0 精确生成。
pub fn surface_mode_production_policy(
    req: SurfaceModeReq,
    escape: bool,
    old: Option<SurfaceModeInfo>,
    display_w: u32,
    display_h: u32,
) -> Result<SurfaceModeInfo, u8> {
    surface_mode_session_policy(
        WM_CLIENT_APP,
        req,
        escape,
        old,
        display_w,
        display_h,
    )
}

/// R1 session-aware production policy：APP 不得自报 System role；SYSTEM
/// 不得自报 App role，且 v1 只授予 Retained、无 escape。两类 client 的
/// Gamepad/IME Host 与 escape 降级拒面保持一致。
pub fn surface_mode_session_policy(
    client_type: u8,
    req: SurfaceModeReq,
    escape: bool,
    old: Option<SurfaceModeInfo>,
    display_w: u32,
    display_h: u32,
) -> Result<SurfaceModeInfo, u8> {
    if !wm_client_known(client_type) || !surface_mode_req_valid(&req) {
        return Err(ST_DENIED);
    }
    let role_ok = match client_type {
        WM_CLIENT_APP => req.role == SURFACE_ROLE_APP,
        WM_CLIENT_SYSTEM => req.role == SURFACE_ROLE_SYSTEM,
        _ => false,
    };
    if !role_ok {
        return Err(ST_DENIED);
    }
    let wants_escape = req.mode == SURFACE_MODE_CANVAS || req.mode == SURFACE_MODE_IMMERSIVE;
    if wants_escape && !escape {
        return Err(ST_DENIED);
    }
    if req.input_classes & INPUT_CLASS_GAMEPAD != 0
        || req.ime_mode == IME_MODE_HOST
    {
        return Err(ST_UNSUPPORTED);
    }
    if let Some(old) = old {
        let old_escape = old.mode == SURFACE_MODE_CANVAS || old.mode == SURFACE_MODE_IMMERSIVE;
        if old_escape && !wants_escape {
            return Err(ST_DENIED);
        }
    }
    let mandatory = SURFACE_F_FOCUS | SURFACE_F_RATE | SURFACE_F_CLEANUP | SURFACE_F_A11Y;
    let info = SurfaceModeInfo {
        surface_id: req.surface_id,
        role: req.role,
        mode: req.mode,
        input_classes: req.input_classes,
        ime_mode: req.ime_mode,
        a11y_mode: req.a11y_mode,
        contract_flags: if wants_escape {
            mandatory | SURFACE_F_ESCAPE
        } else {
            mandatory
        },
        scale_permille: 1000,
        safe_left: 0,
        safe_top: 0,
        safe_right: 0,
        safe_bottom: 0,
    };
    if !surface_mode_grant_matches(&req, &info)
        || !surface_mode_info_fits(&info, display_w, display_h)
    {
        return Err(ST_BAD);
    }
    Ok(info)
}

fn put16(out: &mut [u8], off: usize, v: u16) {
    out[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

/// 编码 P1 per-boot escape seed 控制帧。seed 不进入任何公开常量；由
/// svcmgr 启动期生成并只在预共享时走此帧。
pub fn encode_wm_surface_escape_seed_req(
    out: &mut [u8], seed: &[u8; 8],
) -> Option<usize> {
    let m = out.get_mut(..SURFACE_ESCAPE_SEED_REQ_LEN)?;
    m[0] = OP_WM_SURFACE_ESCAPE_SEED;
    m[1..9].copy_from_slice(seed);
    Some(SURFACE_ESCAPE_SEED_REQ_LEN)
}

/// 解析 P1 escape seed 控制帧；长度/op/保留面全部严格。
pub fn parse_wm_surface_escape_seed_req(buf: &[u8]) -> Option<[u8; 8]> {
    if buf.len() != SURFACE_ESCAPE_SEED_REQ_LEN
        || buf[0] != OP_WM_SURFACE_ESCAPE_SEED
    {
        return None;
    }
    let mut seed = [0u8; 8];
    seed.copy_from_slice(&buf[1..9]);
    Some(seed)
}

/// 编码 R1 per-boot SYSTEM client seed 控制帧。与 escape seed 同形但
/// op 独立，禁止复用 0x68 造成两类特权互换。
pub fn encode_wm_system_seed_req(out: &mut [u8], seed: &[u8; 8]) -> Option<usize> {
    let m = out.get_mut(..WM_SYSTEM_SEED_REQ_LEN)?;
    m[0] = OP_WM_SYSTEM_SEED;
    m[1..9].copy_from_slice(seed);
    Some(WM_SYSTEM_SEED_REQ_LEN)
}

/// 解析 R1 SYSTEM seed 控制帧；截断/尾随/错 op 拒绝。
pub fn parse_wm_system_seed_req(buf: &[u8]) -> Option<[u8; 8]> {
    if buf.len() != WM_SYSTEM_SEED_REQ_LEN || buf[0] != OP_WM_SYSTEM_SEED {
        return None;
    }
    let mut seed = [0u8; 8];
    seed.copy_from_slice(&buf[1..9]);
    Some(seed)
}

/// 编码 WM Surface Mode 请求（恰 8B：
/// `[op][role][mode][input][ime][a11y][surface_id u16]`）。
pub fn encode_wm_surface_mode_req(
    out: &mut [u8], req: &SurfaceModeReq,
) -> Option<usize> {
    if !surface_mode_req_valid(req) {
        return None;
    }
    let m = out.get_mut(..SURFACE_MODE_REQ_LEN)?;
    m[0] = OP_WM_SURFACE_MODE;
    m[1] = req.role;
    m[2] = req.mode;
    m[3] = req.input_classes;
    m[4] = req.ime_mode;
    m[5] = req.a11y_mode;
    put16(m, 6, req.surface_id);
    Some(SURFACE_MODE_REQ_LEN)
}

/// 解析 WM Surface Mode 请求。off 6..8 是 surface_id，本帧无保留位。
pub fn parse_wm_surface_mode_req(buf: &[u8]) -> Option<SurfaceModeReq> {
    if buf.len() != SURFACE_MODE_REQ_LEN || buf[0] != OP_WM_SURFACE_MODE {
        return None;
    }
    let req = SurfaceModeReq {
        surface_id: le16(buf, 6)?,
        role: buf[1],
        mode: buf[2],
        input_classes: buf[3],
        ime_mode: buf[4],
        a11y_mode: buf[5],
    };
    surface_mode_req_valid(&req).then_some(req)
}

/// 编码 WM Surface Mode 成功授予帧（恰 20B）。
pub fn encode_wm_surface_mode_rep(
    out: &mut [u8], info: &SurfaceModeInfo,
) -> Option<usize> {
    if !surface_mode_info_valid(info) {
        return None;
    }
    let m = out.get_mut(..SURFACE_MODE_REP_LEN)?;
    m[0] = ST_OK;
    m[1] = info.role;
    m[2] = info.mode;
    m[3] = info.input_classes;
    m[4] = info.ime_mode;
    m[5] = info.a11y_mode;
    m[6] = info.contract_flags;
    m[7] = 0;
    put16(m, 8, info.scale_permille);
    put16(m, 10, info.safe_left);
    put16(m, 12, info.safe_top);
    put16(m, 14, info.safe_right);
    put16(m, 16, info.safe_bottom);
    put16(m, 18, info.surface_id);
    Some(SURFACE_MODE_REP_LEN)
}

/// 解析授予帧。非 OK 错误沿用 1B 状态应答烘焙线，返回 `Err(st)`。
pub fn parse_wm_surface_mode_rep(
    buf: &[u8],
) -> Option<Result<SurfaceModeInfo, u8>> {
    if buf.len() == 1 {
        let st = buf[0];
        return (st_known(st) && st != ST_OK).then_some(Err(st));
    }
    if buf.len() != SURFACE_MODE_REP_LEN || buf[0] != ST_OK {
        return None;
    }
    let info = SurfaceModeInfo {
        surface_id: le16(buf, 18)?,
        role: buf[1],
        mode: buf[2],
        input_classes: buf[3],
        ime_mode: buf[4],
        a11y_mode: buf[5],
        contract_flags: buf[6],
        scale_permille: le16(buf, 8)?,
        safe_left: le16(buf, 10)?,
        safe_top: le16(buf, 12)?,
        safe_right: le16(buf, 14)?,
        safe_bottom: le16(buf, 16)?,
    };
    surface_mode_info_valid(&info).then_some(Ok(info))
}

/// 统计 UTF-8 前缀码点数；非法序列返回 None。
fn utf8_codepoints(bytes: &[u8]) -> Option<usize> {
    let mut i = 0usize;
    let mut n = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        let len = if b < 0x80 {
            1
        } else if b & 0xE0 == 0xC0 {
            2
        } else if b & 0xF0 == 0xE0 {
            3
        } else if b & 0xF8 == 0xF0 {
            4
        } else {
            return None;
        };
        if i.checked_add(len)? > bytes.len() {
            return None;
        }
        match len {
            2 if b < 0xC2 => return None,
            3 if b == 0xE0 && bytes[i + 1] & 0xE0 == 0x80 => return None,
            3 if b == 0xED && bytes[i + 1] & 0xE0 == 0xA0 => return None,
            4 if b == 0xF0 && bytes[i + 1] & 0xF0 == 0x80 => return None,
            4 if b == 0xF4 && bytes[i + 1] & 0xF0 != 0x80 => return None,
            4 if b > 0xF4 => return None,
            _ => {}
        }
        for &c in &bytes[i + 1..i + len] {
            if c & 0xC0 != 0x80 {
                return None;
            }
        }
        i += len;
        n += 1;
    }
    Some(n)
}

fn ime_composition_valid(ev: &ImeComposition) -> bool {
    if !ime_phase_known(ev.phase)
        || !ime_purpose_known(ev.purpose)
        || ev.revision == 0
        || ev.text_len as usize > IME_TEXT_MAX
    {
        return false;
    }
    let len = usize::from(ev.text_len);
    let text = &ev.text[..len];
    let cps = match utf8_codepoints(text) {
        Some(n) => n,
        None => return false,
    };
    if usize::from(ev.cursor_cp) > cps || !ev.text[len..].iter().all(|&b| b == 0) {
        return false;
    }
    if ev.phase == IME_PHASE_HIDE {
        return len == 0 && ev.cursor_cp == 0;
    }
    true
}

/// 编码 IME composition 旁车帧（恰 76B）。
pub fn encode_ime_composition(
    out: &mut [u8], ev: &ImeComposition,
) -> Option<usize> {
    if !ime_composition_valid(ev) {
        return None;
    }
    let m = out.get_mut(..IME_COMPOSITION_LEN)?;
    m[..4].copy_from_slice(&ev.revision.to_le_bytes());
    m[4] = ev.phase;
    m[5] = ev.purpose;
    put16(m, 6, ev.cursor_cp);
    m[8] = ev.text_len;
    m[9] = 0;
    put16(m, 10, 0);
    let len = usize::from(ev.text_len);
    m[12..12 + len].copy_from_slice(&ev.text[..len]);
    m[12 + len..].fill(0);
    Some(IME_COMPOSITION_LEN)
}

/// 解析 IME composition 旁车帧。保留域与文本尾部零填充均须规范。
pub fn parse_ime_composition(buf: &[u8]) -> Option<ImeComposition> {
    if buf.len() != IME_COMPOSITION_LEN || buf[9] != 0 || buf[10..12] != [0, 0] {
        return None;
    }
    let len = usize::from(buf[8]);
    if len > IME_TEXT_MAX || buf[12 + len..].iter().any(|&b| b != 0) {
        return None;
    }
    let mut ev = ImeComposition {
        revision: u32::from_le_bytes(buf[..4].try_into().ok()?),
        phase: buf[4],
        purpose: buf[5],
        cursor_cp: le16(buf, 6)?,
        text_len: buf[8],
        text: [0; IME_TEXT_MAX],
    };
    ev.text[..len].copy_from_slice(&buf[12..12 + len]);
    ime_composition_valid(&ev).then_some(ev)
}

// ---------- a11y semantic sidecar（Tree/Regions/Hybrid 共用扁平记录） ----------

/// a11y 语义记录标签帽（UTF-8 字节数）。
pub const A11Y_LABEL_MAX: usize = 32;
/// a11y 语义记录帧长。
pub const A11Y_REGION_LEN: usize = 20 + A11Y_LABEL_MAX;

/// 角色：容器/无特别语义（与 BerryUI ROLE_NONE 对齐）。
pub const A11Y_ROLE_NONE: u8 = 0;
/// 角色：按钮（与 BerryUI ROLE_BUTTON 对齐）。
pub const A11Y_ROLE_BUTTON: u8 = 1;
/// 角色：文本（与 BerryUI ROLE_TEXT 对齐）。
pub const A11Y_ROLE_TEXT: u8 = 2;
/// 角色：图像（与 BerryUI ROLE_IMAGE 对齐）。
pub const A11Y_ROLE_IMAGE: u8 = 3;
/// 角色：开关（与 BerryUI ROLE_TOGGLE 对齐）。
pub const A11Y_ROLE_TOGGLE: u8 = 4;
/// 角色：列表项（与 BerryUI ROLE_LIST_ITEM 对齐）。
pub const A11Y_ROLE_LIST_ITEM: u8 = 5;
/// 角色：专业文档 canvas 根/页块。
pub const A11Y_ROLE_DOCUMENT: u8 = 6;
/// 角色：游戏语义区域/控件。
pub const A11Y_ROLE_GAME_REGION: u8 = 7;

/// a11y 动作：可聚焦。
pub const A11Y_ACTION_FOCUS: u8 = 1 << 0;
/// a11y 动作：可激活。
pub const A11Y_ACTION_ACTIVATE: u8 = 1 << 1;
/// a11y 动作：可调整值。
pub const A11Y_ACTION_ADJUST: u8 = 1 << 2;
/// a11y 动作：可编辑文本。
pub const A11Y_ACTION_TEXT_EDIT: u8 = 1 << 3;
/// a11y 已定义动作全集。
pub const A11Y_ACTION_KNOWN: u8 = A11Y_ACTION_FOCUS
    | A11Y_ACTION_ACTIVATE
    | A11Y_ACTION_ADJUST
    | A11Y_ACTION_TEXT_EDIT;

/// a11y 状态：可见。
pub const A11Y_STATE_VISIBLE: u8 = 1 << 0;
/// a11y 状态：聚焦。
pub const A11Y_STATE_FOCUSED: u8 = 1 << 1;
/// a11y 状态：禁用。
pub const A11Y_STATE_DISABLED: u8 = 1 << 2;
/// a11y 状态：选中/勾选。
pub const A11Y_STATE_CHECKED: u8 = 1 << 3;
/// a11y 已定义状态全集。
pub const A11Y_STATE_KNOWN: u8 = A11Y_STATE_VISIBLE
    | A11Y_STATE_FOCUSED
    | A11Y_STATE_DISABLED
    | A11Y_STATE_CHECKED;

/// 扁平语义记录。Tree/Hybrid 由 retained 树导出；Regions/Immersive 由
/// 应用/引擎显式申报。坐标是 surface-local 逻辑像素，经 scale 换算到物理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct A11yRegion {
    pub node_id: u32,
    pub parent_id: u32,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub role: u8,
    pub actions: u8,
    pub state: u8,
    pub label_len: u8,
    pub label: [u8; A11Y_LABEL_MAX],
}

fn a11y_region_valid(region: &A11yRegion) -> bool {
    let len = usize::from(region.label_len);
    if region.node_id == 0
        || region.parent_id == region.node_id
        || region.w == 0
        || region.h == 0
        || region.role > A11Y_ROLE_GAME_REGION
        || region.actions & !A11Y_ACTION_KNOWN != 0
        || region.state & !A11Y_STATE_KNOWN != 0
        || len > A11Y_LABEL_MAX
        || utf8_codepoints(&region.label[..len]).is_none()
        || !region.label[len..].iter().all(|&b| b == 0)
    {
        return false;
    }
    true
}

fn put32(out: &mut [u8], off: usize, v: u32) {
    out[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

/// 编码 a11y 语义记录（恰 52B）。
pub fn encode_a11y_region(out: &mut [u8], region: &A11yRegion) -> Option<usize> {
    if !a11y_region_valid(region) {
        return None;
    }
    let m = out.get_mut(..A11Y_REGION_LEN)?;
    put32(m, 0, region.node_id);
    put32(m, 4, region.parent_id);
    put16(m, 8, region.x);
    put16(m, 10, region.y);
    put16(m, 12, region.w);
    put16(m, 14, region.h);
    m[16] = region.role;
    m[17] = region.actions;
    m[18] = region.state;
    m[19] = region.label_len;
    let len = usize::from(region.label_len);
    m[20..20 + len].copy_from_slice(&region.label[..len]);
    m[20 + len..].fill(0);
    Some(A11Y_REGION_LEN)
}

/// 解析 a11y 语义记录。保留域不存在；尾部零填充必须规范。
pub fn parse_a11y_region(buf: &[u8]) -> Option<A11yRegion> {
    if buf.len() != A11Y_REGION_LEN {
        return None;
    }
    let len = usize::from(buf[19]);
    if len > A11Y_LABEL_MAX || buf[20 + len..].iter().any(|&b| b != 0) {
        return None;
    }
    let mut region = A11yRegion {
        node_id: u32::from_le_bytes(buf[..4].try_into().ok()?),
        parent_id: u32::from_le_bytes(buf[4..8].try_into().ok()?),
        x: le16(buf, 8)?,
        y: le16(buf, 10)?,
        w: le16(buf, 12)?,
        h: le16(buf, 14)?,
        role: buf[16],
        actions: buf[17],
        state: buf[18],
        label_len: buf[19],
        label: [0; A11Y_LABEL_MAX],
    };
    region.label[..len].copy_from_slice(&buf[20..20 + len]);
    a11y_region_valid(&region).then_some(region)
}

const _: () = assert!(SURFACE_MODE_REQ_LEN == 8);
const _: () = assert!(SURFACE_ESCAPE_SEED_REQ_LEN == 9);
const _: () = assert!(SURFACE_MODE_REP_LEN == 20);
const _: () = assert!(IME_TEXT_MAX == 64);
const _: () = assert!(IME_COMPOSITION_LEN == 12 + IME_TEXT_MAX);
const _: () = assert!(A11Y_LABEL_MAX == 32);
const _: () = assert!(A11Y_REGION_LEN == 20 + A11Y_LABEL_MAX);
const _: () = assert!(SURFACE_MODE_REQ_LEN <= 256);
const _: () = assert!(SURFACE_MODE_REP_LEN <= 256);
const _: () = assert!(IME_COMPOSITION_LEN <= 256);
const _: () = assert!(A11Y_REGION_LEN <= 256);
