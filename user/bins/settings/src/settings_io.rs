//! settings_io —— settings 应用五键持久化面（M2 第 8 步：theme_io 单键
//! 前身（已并入本模块）升格为全键版）。
//!
//! 形态沿 launcher `layout_io.rs` 的 canonical 单簇原子写先例（本模块的
//! 单键前身即同形，已并入）：
//!
//! - FAT12 卷根 8.3 大写名单文件（`SETTINGS.CFG`；CWD 默认根）。v1 写面
//!   只认单段 8.3 大写名（fsmgr 写路由——小写/多段构造性落 RAM 树掉电
//!   丢，fat.rs「多段构造性走 RAM 树」）；`/cfg` 目录挂载是 v2 蓝图
//!   （FAT 子目录写面官方推迟项），v1 落点钉死卷根平面。
//! - 严格串行单 RPC 会话；**close 即 commit**（TRUNC 游标）。payload 恒
//!   ≤2048B 单簇（近原子；canonical 实值 ~45B）。
//! - 三约束沿袭：**缺席回默认不预写**（首 boot 常态）+ **坏件回默认并
//!   canonical 重写自愈** + **写失败非致命**（内存态照用，app 不死）。
//!   写失败的下 boot 结局注记：部分写截断件 = 前缀不完整 → parse None →
//!   heal 全键默认化（不是回跳上一次成功值——单簇覆写无旧版可回，文档
//!   化取舍）。
//! - 旧档迁移：`SETTINGS.JSN`（单键 dark 两形）在场而 CFG 缺席 → 读旧
//!   值组新形立即落盘（迁移期一两版后旧读面可删）。
//! - fs 礼物缺席（`fs_h()==0`）→ 默认态直返零 RPC（内存态，同前身）。
//!
//! canonical 形（键序冻结）：`{"wifi":0,"bt":0,"bright":800,"sync":0,"dark":0}`
//! ——值域：bool 键 0/1；bright 0..=1000（Slider min/max 同源）。解析
//! fail-closed 受控面：只认此键序此值域的全字节形态（本 app 自己写的
//! canonical——异形即腐败，None → 自愈重写；不是通用 JSON 解析器）。

use berry_libc::fs::{open, O_CREAT, O_RDONLY, O_WRONLY};
use berry_libc::io::{close, fs_h, read, write};
use berry_libc::{get_errno, EIO, ENOENT};

use berry_ui::theme::Theme;

/// 卷根配置文件（8.3 大写——fsmgr 单 FAT 卷根；LAYOUT.JSN 同位）。
const PATH: &[u8] = b"SETTINGS.CFG\0";

/// 旧档（单键前身形态；迁移读取面——迁完一两版可删）。
const PATH_LEGACY: &[u8] = b"SETTINGS.JSN\0";

/// 单次读块（前身同款 256B）。
const CHUNK: usize = 256;

/// 单簇帽（FAT12 2048B）。
const BYTES_MAX: usize = 2048;

/// bright 值域上限。依赖注记：本页 Slider min=0 且 max=BRIGHT_MAX 恰使
/// 「绝对值 = permille 数值」恒等（desc.rs SliderProps 的 value 契约是
/// 0..=1000 permille）——改 max 必须同步折算语义，否则绝对值/permille
/// 分叉。
pub const BRIGHT_MAX: u32 = 1000;

/// 五键状态（wifi/bt/sync/dark = 布尔；bright = 0..=1000）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct State {
    pub wifi: bool,
    pub bt: bool,
    pub bright: u32,
    pub sync: bool,
    pub dark: bool,
}

impl State {
    /// 缺席默认（首 boot 常态——与建树历史初值一致：wifi on / bt off /
    /// bright 800 / sync off / light）。
    pub fn default_state() -> State {
        State { wifi: true, bt: false, bright: 800, sync: false, dark: false }
    }

    fn theme(&self) -> Theme {
        if self.dark { Theme::Dark } else { Theme::Light }
    }
}

/// 状态 → canonical 字节（键序冻结，~45B）。
pub fn canonical(st: &State) -> Vec<u8> {
    format!(
        "{{\"wifi\":{},\"bt\":{},\"bright\":{},\"sync\":{},\"dark\":{}}}",
        st.wifi as u8, st.bt as u8, st.bright, st.sync as u8, st.dark as u8
    )
    .into_bytes()
}

// ---- 受控解析（fail-closed；只认 canonical 键序值域——异形 None）----

fn lit(b: &[u8], i: &mut usize, s: &[u8]) -> bool {
    if b.len() >= *i + s.len() && &b[*i..*i + s.len()] == s {
        *i += s.len();
        true
    } else {
        false
    }
}

fn bool_v(b: &[u8], i: &mut usize) -> Option<bool> {
    match b.get(*i) {
        Some(b'0') => { *i += 1; Some(false) }
        Some(b'1') => { *i += 1; Some(true) }
        _ => None,
    }
}

fn num_v(b: &[u8], i: &mut usize, max: u32) -> Option<u32> {
    let mut v: u32 = 0;
    let mut n = 0;
    while let Some(c @ b'0'..=b'9') = b.get(*i) {
        v = v.checked_mul(10)?.checked_add((c - b'0') as u32)?;
        if v > max { return None; }
        *i += 1;
        n += 1;
    }
    (n > 0).then_some(v)
}

/// canonical 字节 → 状态（fail-closed：全字节恰尽此形，异字节/前缀/
/// 尾随/越域 → None）。
pub fn parse(b: &[u8]) -> Option<State> {
    let mut i = 0usize;
    if !lit(b, &mut i, b"{\"wifi\":") { return None; }
    let wifi = bool_v(b, &mut i)?;
    if !lit(b, &mut i, b",\"bt\":") { return None; }
    let bt = bool_v(b, &mut i)?;
    if !lit(b, &mut i, b",\"bright\":") { return None; }
    let bright = num_v(b, &mut i, BRIGHT_MAX)?;
    if !lit(b, &mut i, b",\"sync\":") { return None; }
    let sync = bool_v(b, &mut i)?;
    if !lit(b, &mut i, b",\"dark\":") { return None; }
    let dark = bool_v(b, &mut i)?;
    if !lit(b, &mut i, b"}") { return None; }
    (i == b.len()).then_some(State { wifi, bt, bright, sync, dark })
}

/// 读结局四态（前身同形）。
enum ReadOutcome {
    Absent,
    Bytes(Vec<u8>),
    Oversize(usize),
    Failed(i32),
}

/// 全量读（参数化路径——CFG 与旧 JSN 共用）。
fn read_at(path: &[u8]) -> ReadOutcome {
    // SAFETY: path 是 NUL 结尾字面量；fd 按 libc 契约使用。
    let fd = unsafe { open(path.as_ptr() as *const _, O_RDONLY) };
    if fd < 0 {
        return match get_errno() {
            ENOENT => ReadOutcome::Absent,
            e => ReadOutcome::Failed(e),
        };
    }
    let mut data: Vec<u8> = Vec::new();
    let mut buf = [0u8; CHUNK];
    loop {
        // read 安全 fn（io.rs）；<0 错 / 0 EOF。
        let n = read(fd, buf.as_mut_ptr() as *mut _, buf.len());
        if n < 0 {
            let e = get_errno();
            close(fd);
            return ReadOutcome::Failed(e);
        }
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n as usize]);
        if data.len() > BYTES_MAX {
            close(fd);
            return ReadOutcome::Oversize(data.len());
        }
    }
    close(fd);
    ReadOutcome::Bytes(data)
}

/// 旧档迁移读取（SETTINGS.JSN 单键两形——前身 canonical 同形）。
fn legacy_dark() -> Option<bool> {
    match read_at(PATH_LEGACY) {
        ReadOutcome::Bytes(b) => match b.as_slice() {
            b"{\"dark\":0}" => Some(false),
            b"{\"dark\":1}" => Some(true),
            _ => None, // 旧档腐败——视同缺席，走默认
        },
        _ => None,
    }
}

/// 启动装载（main 单次调用）：三约束收口 + 旧档迁移。
pub fn load() -> State {
    if fs_h() == 0 {
        return State::default_state();
    }
    match read_at(PATH) {
        ReadOutcome::Absent => match legacy_dark() {
            // 旧 JSN 在场：dark 键迁移进新形并立即落盘（其余键默认）。
            Some(dark) => {
                let st = State { dark, ..State::default_state() };
                save(&st);
                st
            }
            None => State::default_state(), // 首 boot 常态：默认不预写
        },
        ReadOutcome::Oversize(n) => heal(Some(format!("too big {n}"))),
        ReadOutcome::Failed(e) => {
            // I/O 面 unhealthy：回默认不重写（heal 写会再撞同错）。
            eprintln!("settings: read errno={e} -- default");
            State::default_state()
        }
        ReadOutcome::Bytes(b) => match parse(&b) {
            Some(st) => st,
            None => heal(None),
        },
    }
}

/// 坏件自愈：默认态 + canonical 重写（下一 boot 起可读；写失败非致命）。
fn heal(why: Option<String>) -> State {
    if let Some(w) = why {
        eprintln!("settings: {w} -- default");
    } else {
        eprintln!("settings: file corrupt -- rewrite");
    }
    let st = State::default_state();
    save(&st);
    st
}

/// 换值落盘（run 事件臂消费）：canonical 覆写 + close=commit；fs 缺席
/// 静默跳过；失败非致命（内存态照用，常驻纪律）。
pub fn save(st: &State) {
    if fs_h() == 0 {
        return;
    }
    if let Err(e) = put_file(&canonical(st)) {
        eprintln!("settings: write errno={e}");
    }
}

/// 主题档便捷面（main 的 theme 消费同源——不改 run 臂的 Theme 语义）。
pub fn theme_of(st: &State) -> Theme {
    st.theme()
}

/// 全量写（open(O_WRONLY|O_CREAT) → write 循环 → close=commit；前身
/// 同款，部分写失败仍 close 部分提交——文档化取舍）。
fn put_file(data: &[u8]) -> Result<(), i32> {
    // SAFETY: PATH NUL 结尾；fd 按 libc 契约使用。
    let fd = unsafe { open(PATH.as_ptr() as *const _, O_WRONLY | O_CREAT) };
    if fd < 0 {
        return Err(get_errno());
    }
    let mut off = 0usize;
    while off < data.len() {
        // write 安全 fn（内部按 192B 分片 PUT）；部分写按返回量续写。
        let n = write(fd, data[off..].as_ptr() as *const _, data.len() - off);
        if n < 0 {
            let e = get_errno();
            close(fd);
            return Err(e);
        }
        if n == 0 {
            close(fd);
            return Err(EIO);
        }
        off += n as usize;
    }
    if close(fd) != 0 {
        return Err(get_errno());
    }
    Ok(())
}

// ---- 测试面（纯函数：canonical 形 + fail-closed 解析 + 迁移面；RPC 面
// ---- 经 fs_h()/open 不可宿主机执行——前身同注记。----
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_form() {
        let st = State::default_state();
        assert_eq!(
            canonical(&st),
            b"{\"wifi\":1,\"bt\":0,\"bright\":800,\"sync\":0,\"dark\":0}".to_vec()
        );
        assert!(canonical(&st).len() <= BYTES_MAX);
    }

    #[test]
    fn parse_roundtrip() {
        let st = State { wifi: true, bt: true, bright: 1000, sync: true, dark: true };
        assert_eq!(parse(&canonical(&st)), Some(st));
        assert_eq!(parse(&canonical(&State::default_state())), Some(State::default_state()));
    }

    #[test]
    fn parse_fail_closed() {
        assert_eq!(parse(b""), None);
        assert_eq!(parse(b"{\"wifi\":1"), None); // 前缀截断
        assert_eq!(parse(b"{\"wifi\":1,\"bt\":0,\"bright\":800,\"sync\":0,\"dark\":0}\n"), None); // 尾随
        assert_eq!(parse(b"{\"wifi\":2,\"bt\":0,\"bright\":800,\"sync\":0,\"dark\":0}"), None); // 越域 bool
        assert_eq!(parse(b"{\"wifi\":1,\"bt\":0,\"bright\":1001,\"sync\":0,\"dark\":0}"), None); // bright 越域
        assert_eq!(parse(b"{\"bt\":0,\"wifi\":1,\"bright\":800,\"sync\":0,\"dark\":0}"), None); // 键序
        assert_eq!(parse(b"{\"WIFI\":1,\"bt\":0,\"bright\":800,\"sync\":0,\"dark\":0}"), None); // 大小写
        assert_eq!(parse(&[0u8; 2048]), None); // 满簇垃圾
        // 双写叠加也不得误判。
        let mut both = canonical(&State::default_state());
        both.extend_from_slice(&canonical(&State::default_state()));
        assert_eq!(parse(&both), None);
    }

    #[test]
    fn bright_bounds() {
        let st = State { bright: 0, ..State::default_state() };
        assert_eq!(parse(&canonical(&st)), Some(st));
        let st = State { bright: BRIGHT_MAX, ..State::default_state() };
        assert_eq!(parse(&canonical(&st)), Some(st));
    }
}
