//! settings_io —— 设置页数据面（波3-件⑤c：BOOT.JSN / SETTINGS.JSN
//! 持久键）。
//!
//! 责任：设置页两持久文件的 canonical 读写——I/O 半程走
//! **`appshell::conf`**（单簇 ≤2048B 近原子写：open(O_WRONLY|O_CREAT)
//! → write 循环 → **close 即 commit**（TRUNC(游标) 两段式，bsh
//! `put_file` 同款）——SA 件⑦ f016aa99a 落地后即时切换，mandate 首选
//! 项「appshell::conf 同款」兑现；schema 面（canonical 编解码/默认值
//! 回退）留本模块）。
//!
//! - **BOOT.JSN**：boot 模式默认键（波3 设计 §1「boot 模式默认持久键」
//!   / §7 验收句「Toggle 切 verbose → FAT 落 BOOT.JSN」）。canonical
//!   字节两形：`{"boot":"splash"}`（17B，默认档——图形 splash 相）/
//!   `{"boot":"verbose"}`（18B——串口滚屏相；16B/17B 为初稿笔误，波3a
//!   验收修正）。宿主消费面 = berry CLI `--boot-mode` 缺席时的数据盘
//!   默认读回（波1 合入方；FAT12 宿主读回复用 tools/gui8-smoke.py
//!   `Fat12Volume.read_file` 先例——2026-11-15 审查建议 c）。
//! - **SETTINGS.JSN**：其余设置项真存（设置页 Toggle/Seg/Slider 的真值
//!   面）。canonical 字节单形（键序字典序固定、无空白）：
//!   `{"anim":1,"theme":0,"volume":500}`——`anim` 0/1 = 动画开关、
//!   `theme` 0/1 = light/dark、`volume` 0..=1000 permille。
//!
//! 读语义（与 layout_io 三约束同族但**不重写自愈**）：缺席 = 默认值
//! 零打印（首 boot 常态）；坏件/I-O 错 = eprintln 诊断 + 内存默认值
//! （写失败非致命同律——下一次 save 全量覆写即自愈，不额外造写流量）。
//!
//! 冻结相零扰动：`fs_h()==0` → 读 = 默认直返零 RPC，写 = 静默跳过
//! （bsh 手跑/冻结相 extras 面——layout_io 同款结构性早退）。
//!
//! v1 诚实边界：boot 键只影响**下次 reboot** 的宿主默认（本 boot 相
//! 不可逆——splash/verbose 是 boot 早期链）；设置项的真值消费面
//! （theme 全局换肤 / anim 帧率档 / volume 真混音）挂后续波，本波只
//! 立「真存真读」数据面 + 控件真值反映。

use berry_appshell::conf::{read_key, write_key, ConfKey, ReadOutcome as ConfRead};
use berry_libc::io::fs_h;

/// boot 模式默认键（8.3 大写——fsmgr 单 FAT 卷根；单簇帽 2048 同
/// layout_io `LAYOUT_BYTES_MAX` 纪律）。
const BOOT_KEY: ConfKey = ConfKey { path: b"BOOT.JSN\0", cap: 2048 };
/// 设置项键。
const SETTINGS_KEY: ConfKey = ConfKey { path: b"SETTINGS.JSN\0", cap: 2048 };

/// boot 模式两档（波1 设计：图形 splash 为默认形态；verbose = 串口
/// 滚屏相——fbinfo 第 6 域 gui_mode 的既有双档投影）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    Splash,
    Verbose,
}

impl BootMode {
    /// canonical JSON 值字面（单源——emit 与文档对拍锚点）。
    pub fn word(self) -> &'static [u8] {
        match self {
            BootMode::Splash => b"splash",
            BootMode::Verbose => b"verbose",
        }
    }
    /// 全键 canonical 文档（canonical 单簇原子写的净载荷；有效长 =
    /// [`Self::canonical_len`]，余零填充）。
    pub fn canonical(self) -> [u8; 24] {
        const HEAD: &[u8] = br#"{"boot":""#;
        let mut out = [0u8; 24];
        out[..HEAD.len()].copy_from_slice(HEAD);
        let w = self.word();
        out[HEAD.len()..HEAD.len() + w.len()].copy_from_slice(w);
        let n = HEAD.len() + w.len();
        out[n] = b'"';
        out[n + 1] = b'}';
        out
    }
    /// canonical 全长。
    pub fn canonical_len(self) -> usize {
        br#"{"boot":""#.len() + self.word().len() + 2
    }
}

/// 其余设置项真值面（SETTINGS.JSN 的 Owned 镜像）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// 音量 0..=1000 permille（Slider 同域）。
    pub volume: u32,
    /// 动画开关（true = 开）。
    pub anim: bool,
    /// 主题（true = dark）。
    pub dark: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { volume: 500, anim: true, dark: false }
    }
}

impl Settings {
    /// canonical 全文档（键序字典序 anim/theme/volume 固定、无空白——
    /// 宿主侧字节对拍锚点；有效长 = [`Self::canonical_len`]，余零填充）。
    pub fn canonical(&self) -> [u8; 48] {
        let mut out = [0u8; 48];
        let mut n = 0usize;
        let put = |out: &mut [u8; 48], n: &mut usize, s: &[u8]| {
            out[*n..*n + s.len()].copy_from_slice(s);
            *n += s.len();
        };
        put(&mut out, &mut n, br#"{"anim":"#);
        put(&mut out, &mut n, if self.anim { b"1" } else { b"0" });
        put(&mut out, &mut n, b",\"theme\":");
        put(&mut out, &mut n, if self.dark { b"1" } else { b"0" });
        put(&mut out, &mut n, b",\"volume\":");
        let v = self.volume.min(1000);
        let mut digs = [0u8; 4];
        let dn = if v >= 1000 {
            digs.copy_from_slice(b"1000");
            4
        } else {
            let mut m = [b'0'; 3];
            m[0] = b'0' + (v / 100) as u8;
            m[1] = b'0' + (v / 10 % 10) as u8;
            m[2] = b'0' + (v % 10) as u8;
            let first = if v >= 100 { 0 } else if v >= 10 { 1 } else { 2 };
            digs[..3 - first].copy_from_slice(&m[first..]);
            3 - first
        };
        put(&mut out, &mut n, &digs[..dn]);
        put(&mut out, &mut n, b"}");
        out
    }

    /// canonical 全长。
    pub fn canonical_len(&self) -> usize {
        br#"{"anim":,"theme":,"volume":}"#.len()
            + 1
            + 1
            + {
                let v = self.volume.min(1000);
                if v >= 1000 { 4 } else if v >= 100 { 3 } else if v >= 10 { 2 } else { 1 }
            }
    }
}

/// boot 模式默认键读（main 建树单次调用——设置页 Toggle 初值真源）。
/// 缺席 = Splash 默认（波1：图形模式为默认形态）；坏件 = 诊断 + 默认。
pub fn load_boot() -> BootMode {
    if fs_h() == 0 {
        return BootMode::Splash; // 冻结相：零 RPC 默认直返
    }
    match read_key(&BOOT_KEY) {
        ConfRead::Bytes(b) => parse_boot(&b),
        ConfRead::Absent => BootMode::Splash,
        ConfRead::Oversize(n) => {
            eprintln!("launcher: boot key too big {n} -- default splash");
            BootMode::Splash
        }
        ConfRead::Failed(e) => {
            eprintln!("launcher: boot key read errno={e} -- default splash");
            BootMode::Splash
        }
    }
}

/// boot 文档解析（纯函数——canonical 子串精确匹配，SMOKE_MAGIC 同款
/// fail-closed 纪律；前缀/异字节一律不认）。
pub fn parse_boot(doc: &[u8]) -> BootMode {
    if contains(doc, br#""boot":"verbose""#) {
        BootMode::Verbose
    } else if contains(doc, br#""boot":"splash""#) {
        BootMode::Splash
    } else {
        if !doc.is_empty() {
            eprintln!("launcher: boot key unrecognized -- default splash");
        }
        BootMode::Splash
    }
}

/// boot 模式写（设置页 Toggle 翻转即落——canonical 单簇原子写）。
/// 写失败非致命（eprintln 后返回 false，控件态照翻——真值下次再追）。
pub fn save_boot(mode: BootMode) -> bool {
    if fs_h() == 0 {
        return false; // 冻结相：无持久面，静默
    }
    let doc = mode.canonical();
    match write_key(&BOOT_KEY, &doc[..mode.canonical_len()]) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("launcher: boot key write errno={e}");
            false
        }
    }
}

/// 设置项读（main 建树单次调用——设置页控件初值真源）。
pub fn load_settings() -> Settings {
    if fs_h() == 0 {
        return Settings::default();
    }
    match read_key(&SETTINGS_KEY) {
        ConfRead::Bytes(b) => parse_settings(&b),
        ConfRead::Absent => Settings::default(),
        ConfRead::Oversize(n) => {
            eprintln!("launcher: settings too big {n} -- defaults");
            Settings::default()
        }
        ConfRead::Failed(e) => {
            eprintln!("launcher: settings read errno={e} -- defaults");
            Settings::default()
        }
    }
}

/// `"key":digits` 数值扫描（纯函数——容忍键序/未知键/空白缺席，缺键
/// 保持默认；非数字值视同缺键）。值帽 `max` 钳域。
fn scan_u32(doc: &[u8], key: &[u8], max: u32) -> Option<u32> {
    let mut i = 0usize;
    while i + key.len() + 2 < doc.len() {
        // 逐位找 "key": 前缀（含引号对与冒号——近似键不误吞）。
        if doc[i] == b'"'
            && doc[i + 1..].starts_with(key)
            && doc[i + 1 + key.len()] == b'"'
            && doc[i + 2 + key.len()] == b':'
        {
            let mut j = i + 3 + key.len();
            let mut v: u32 = 0;
            let mut any = false;
            while j < doc.len() && doc[j].is_ascii_digit() {
                v = v.saturating_mul(10).saturating_add((doc[j] - b'0') as u32);
                any = true;
                j += 1;
            }
            if any {
                return Some(v.min(max));
            }
            i = j; // 值非数字 → 继续找下一处
        }
        i += 1;
    }
    None
}

/// 设置文档解析（纯函数——三键各自独立扫描，缺键/坏值保持默认）。
pub fn parse_settings(doc: &[u8]) -> Settings {
    let mut s = Settings::default();
    if let Some(v) = scan_u32(doc, b"anim", 1) {
        s.anim = v == 1;
    }
    if let Some(v) = scan_u32(doc, b"theme", 1) {
        s.dark = v == 1;
    }
    if let Some(v) = scan_u32(doc, b"volume", 1000) {
        s.volume = v;
    }
    if !doc.is_empty() && scan_u32(doc, b"anim", 1).is_none() {
        eprintln!("launcher: settings unrecognized -- partial defaults");
    }
    s
}

/// 设置项写（控件变更即落——canonical 单簇原子写；失败非致命）。
pub fn save_settings(s: &Settings) -> bool {
    if fs_h() == 0 {
        return false;
    }
    let doc = s.canonical();
    match write_key(&SETTINGS_KEY, &doc[..s.canonical_len()]) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("launcher: settings write errno={e}");
            false
        }
    }
}

/// 子串包含（字节窗扫描；两 canonical 形恒短）。
fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

// ---- 纯函数测试面（layout_io 同注：aarch64-musl 目标可编译，宿主无
// ---- qemu-user 不可执行；用例零 RPC/零 fd——canonical 字节对拍锚点）----
#[cfg(test)]
mod tests {
    use super::*;

    /// boot canonical 两形字节精确（宿主对拍锚点：FAT 读回必须逐字节
    /// 等于这两串——波1 berry CLI 消费方按此对齐）。
    #[test]
    fn boot_canonical_bytes() {
        let m = BootMode::Verbose;
        let doc = &m.canonical()[..m.canonical_len()];
        assert_eq!(doc, br#"{"boot":"verbose"}"#);
        assert_eq!(doc.len(), 18);
        let m = BootMode::Splash;
        let doc = &m.canonical()[..m.canonical_len()];
        assert_eq!(doc, br#"{"boot":"splash"}"#);
        assert_eq!(doc.len(), 17);
    }

    /// boot 解析 fail-closed：canonical 两形互认；前缀/异字节/空文档
    /// 一律 Splash 默认。
    #[test]
    fn boot_parse_fail_closed() {
        assert_eq!(parse_boot(br#"{"boot":"verbose"}"#), BootMode::Verbose);
        assert_eq!(parse_boot(br#"{"boot":"splash"}"#), BootMode::Splash);
        assert_eq!(parse_boot(b""), BootMode::Splash);
        assert_eq!(parse_boot(br#"{"boot":"verb"#), BootMode::Splash);
        assert_eq!(parse_boot(br#"{"boot":"verboseX"}"#), BootMode::Splash);
        assert_eq!(parse_boot(br#"{"boot":"SPLASH"}"#), BootMode::Splash);
    }

    /// settings canonical 单形字节精确（键序字典序、无空白）。
    #[test]
    fn settings_canonical_bytes() {
        let s = Settings { volume: 500, anim: true, dark: false };
        let doc = &s.canonical()[..s.canonical_len()];
        assert_eq!(doc, br#"{"anim":1,"theme":0,"volume":500}"#);
        let s = Settings { volume: 0, anim: false, dark: true };
        let doc = &s.canonical()[..s.canonical_len()];
        assert_eq!(doc, br#"{"anim":0,"theme":1,"volume":0}"#);
        let s = Settings { volume: 1000, anim: true, dark: true };
        let doc = &s.canonical()[..s.canonical_len()];
        assert_eq!(doc, br#"{"anim":1,"theme":1,"volume":1000}"#);
        let s = Settings { volume: 7, anim: false, dark: false };
        let doc = &s.canonical()[..s.canonical_len()];
        assert_eq!(doc, br#"{"anim":0,"theme":0,"volume":7}"#);
    }

    /// 解析容忍面：canonical 回读全等；键序颠倒/未知键/缺键/坏值各自
    /// 独立存活（缺键保默认）。
    #[test]
    fn settings_parse_roundtrip_and_tolerance() {
        let s = Settings { volume: 730, anim: true, dark: false };
        let doc = s.canonical();
        let back = parse_settings(&doc[..s.canonical_len()]);
        assert_eq!(back, s);
        // 键序颠倒 + 未知键。
        assert_eq!(
            parse_settings(br#"{"volume":9,"zz":1,"anim":0,"theme":1}"#),
            Settings { volume: 9, anim: false, dark: true }
        );
        // 缺键保默认（volume 默认 500）。
        let p = parse_settings(br#"{"anim":0}"#);
        assert_eq!((p.anim, p.dark, p.volume), (false, false, 500));
        // 坏值（非数字）= 缺键。
        let p = parse_settings(br#"{"volume":abc,"anim":1}"#);
        assert_eq!((p.volume, p.anim), (500, true));
        // 空文档 = 全默认。
        assert_eq!(parse_settings(b""), Settings::default());
        // 值域钳制。
        assert_eq!(parse_settings(br#"{"volume":99999}"#).volume, 1000);
    }

    /// 扫描器不误吞前缀近似键（引号必包）。
    #[test]
    fn scan_key_exact_quotes() {
        assert_eq!(scan_u32(br#"{"volumex":9,"volume":3}"#, b"volume", 1000), Some(3));
        assert_eq!(scan_u32(br#"{"anim":1}"#, b"anim", 1), Some(1));
        assert_eq!(scan_u32(br#"{"animal":1}"#, b"anim", 1), None);
    }
}
