//! theme_io —— 主题档持久化面（维八暗色刀：settings 暗色开关的跨
//! boot 存活件）。
//!
//! 形态 = launcher `layout_io.rs` 的 canonical 单簇原子写先例同形
//! （LAYOUT.JSN 三约束裁剪）：
//!
//! - FAT12 卷根 8.3 大写名单文件（`SETTINGS.JSN`；CWD 默认根，bsh
//!   相对路径同形）；payload 恒 ≤2048B 单簇（近原子——掉电三态推导
//!   见 GUI-8 侦察 §FAT 约束，实值仅 10/11B）。
//! - 严格串行单 RPC 会话（fsclient 单读者协议——一次只持一个 fd，
//!   open→…→close 不重叠）；**close 即 commit**（TRUNC 游标——bsh
//!   `put_file`/launcher `put_file` 同款，O_TRUNC 省略 = 覆写幂等）。
//! - 三约束：**缺席回落默认 Light**（首 boot 常态，不预写——主题默认
//!   值无需物化，首个用户切换才落盘；LAYOUT.JSN 的「缺席也写」是布局
//!   自愈语义，此处不同）+ **坏件回默认并重写自愈**（可读但非
//!   canonical 两形之一——腐败面修复，下一 boot 起 canonical）+ **写
//!   失败非致命**（eprintln 诊断后内存态照用，app 不死——常驻纪律）。
//! - fs 礼物缺席（`fs_h()==0`：manifest 无 fs 词的装配/冻结相）→ 默认
//!   Light 直返零 RPC 零 fd 面（layout_io 冻结相短路同形；此时 v1 退
//!   化为内存态开关——诚实标注于 DARK-IMPL.md）。
//!
//! canonical 形恰两态：`{"dark":0}` / `{"dark":1}`（10/11B）。解析
//! fail-closed：只认两形全字节精确匹配，任何异字节/长度 → None（不猜
//! ——berry-layout 全 JSON 解析器对两态文件是杀鸡牛刀，此为最小诚实面）。

use berry_libc::fs::{open, O_CREAT, O_RDONLY, O_WRONLY};
use berry_libc::io::{close, fs_h, read, write};
use berry_libc::{get_errno, EIO, ENOENT};

use berry_ui::theme::Theme;

/// 卷根主题文件（8.3 大写——fsmgr 单 FAT 卷根；LAYOUT.JSN 同位）。
const PATH: &[u8] = b"SETTINGS.JSN\0";

/// 单次读块（bsh `get_file`/launcher `read_file` 同款 256B）。
const CHUNK: usize = 256;

/// 单簇帽（FAT12 2048B——LAYOUT_BYTES_MAX 同律；合法件远小于此）。
const BYTES_MAX: usize = 2048;

/// 档位 → canonical 字节（恰两形）。
pub fn canonical(t: Theme) -> &'static [u8] {
    match t {
        Theme::Light => b"{\"dark\":0}",
        Theme::Dark => b"{\"dark\":1}",
    }
}

/// canonical 字节 → 档位（fail-closed：全字节精确匹配两形之一，异字
/// 节/前缀/尾随/空 → None）。
pub fn parse(b: &[u8]) -> Option<Theme> {
    if b == canonical(Theme::Light) {
        Some(Theme::Light)
    } else if b == canonical(Theme::Dark) {
        Some(Theme::Dark)
    } else {
        None
    }
}

/// 读结局四态（layout_io `ReadOutcome` 同形）。
enum ReadOutcome {
    /// ENOENT——文件缺席（首 boot 常态）。
    Absent,
    /// 全文 ≤ `BYTES_MAX`。
    Bytes(Vec<u8>),
    /// 超单簇帽（异种/腐败文件面）。
    Oversize(usize),
    /// 其余 I/O 错（errno）。
    Failed(i32),
}

/// 全量读（open(O_RDONLY) → read 循环至 EOF → close；读 fd 的 close
/// 无 commit 面）。帽 = `BYTES_MAX`+1——超即停读归 Oversize。
fn read_file() -> ReadOutcome {
    // SAFETY: PATH 是 NUL 结尾字面量；fd 按 libc 契约使用。
    let fd = unsafe { open(PATH.as_ptr() as *const _, O_RDONLY) };
    if fd < 0 {
        return match get_errno() {
            ENOENT => ReadOutcome::Absent,
            e => ReadOutcome::Failed(e),
        };
    }
    let mut data: Vec<u8> = Vec::new();
    let mut buf = [0u8; CHUNK];
    loop {
        // read 安全 fn（io.rs——fd≥3 走 fs 面）；<0 错 / 0 EOF。
        let n = read(fd, buf.as_mut_ptr() as *mut _, buf.len());
        if n < 0 {
            let e = get_errno();
            close(fd);
            return ReadOutcome::Failed(e);
        }
        if n == 0 {
            break; // EOF
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

/// 启动装载（main 单次调用）：三约束收口（见模块头）。
pub fn load() -> Theme {
    if fs_h() == 0 {
        // fs 礼物缺席（冻结相/无 fs 词装配）：默认直返零 RPC——v1 内存
        // 态开关面（换档不落盘，DARK-IMPL.md 诚实标注）。
        return Theme::Light;
    }
    match read_file() {
        ReadOutcome::Absent => Theme::Light, // 首 boot 常态：默认不预写
        ReadOutcome::Oversize(n) => heal(Some(format!("too big {n}"))),
        ReadOutcome::Failed(e) => {
            // I/O 面 unhealthy：回默认不重写（heal 写本身会再撞同错）。
            eprintln!("settings: theme read errno={e} -- default");
            Theme::Light
        }
        ReadOutcome::Bytes(b) => match parse(&b) {
            Some(t) => t,
            None => heal(None), // 腐败件：回默认 + canonical 重写自愈
        },
    }
}

/// 坏件自愈：默认 Light + canonical 重写（下一 boot 起可读；写失败非
/// 致命——内存态照用）。
fn heal(why: Option<String>) -> Theme {
    if let Some(w) = why {
        eprintln!("settings: theme {w} -- default");
    } else {
        eprintln!("settings: theme file corrupt -- rewrite");
    }
    save(Theme::Light);
    Theme::Light
}

/// 换档落盘（run 的 EV_THEME 臂消费）：canonical 覆写 + close=commit；
/// fs 礼物缺席装配（`fs_h()==0`）静默跳过（内存态开关——load 同门对
/// 称，零噪）；其余失败非致命（eprintln 后返——内存档位照用，常驻纪律）。
pub fn save(t: Theme) {
    if fs_h() == 0 {
        return;
    }
    if let Err(e) = put_file(canonical(t)) {
        eprintln!("settings: theme write errno={e}");
    }
}

/// 全量写（open(O_WRONLY|O_CREAT) → write 循环 → close=commit；Err
/// (errno) 透传，调用方非致命收口）。中途写失败仍 close——部分数据
/// 照旧提交（POSIX close 恒消费，bsh/launcher 文档化取舍同款）。
fn put_file(data: &[u8]) -> Result<(), i32> {
    // SAFETY: PATH NUL 结尾；fd 按 libc 契约使用。
    let fd = unsafe { open(PATH.as_ptr() as *const _, O_WRONLY | O_CREAT) };
    if fd < 0 {
        return Err(get_errno());
    }
    let mut off = 0usize;
    while off < data.len() {
        // write 安全 fn（io.rs——内部按 192B 分片 PUT）；部分写按返回量续写。
        let n = write(fd, data[off..].as_ptr() as *const _, data.len() - off);
        if n < 0 {
            let e = get_errno();
            close(fd); // 部分 commit（文档化取舍）
            return Err(e);
        }
        if n == 0 {
            close(fd);
            return Err(EIO); // 零写非 EOF = 协议破缺面
        }
        off += n as usize;
    }
    // close 安全 fn：出错 = commit（TRUNC）失败面——errno 透传。
    if close(fd) != 0 {
        return Err(get_errno());
    }
    Ok(())
}

// ---- 测试面（纯函数：canonical 两形 + fail-closed 解析；RPC 面经
// ---- `fs_h()`/`open` 不可宿主机执行——layout_io.rs 同注记，构建经
// ---- `cargo test --no-run --target aarch64-unknown-linux-musl` 编译
// ---- 过链，执行需 qemu-user（本机未装）。----
#[cfg(test)]
mod tests {
    use super::*;

    /// canonical 两形恰 10/11B（≤2048 单簇帽的结构性前提）。
    #[test]
    fn canonical_two_forms() {
        assert_eq!(canonical(Theme::Light), b"{\"dark\":0}");
        assert_eq!(canonical(Theme::Dark), b"{\"dark\":1}");
        assert_eq!(canonical(Theme::Light).len(), 10);
        assert_eq!(canonical(Theme::Dark).len(), 11);
        assert!(canonical(Theme::Light).len() <= BYTES_MAX);
    }

    /// roundtrip：两形 ↔ 两档全字节复原。
    #[test]
    fn parse_roundtrip() {
        assert_eq!(parse(canonical(Theme::Light)), Some(Theme::Light));
        assert_eq!(parse(canonical(Theme::Dark)), Some(Theme::Dark));
    }

    /// fail-closed：空/前缀/尾随/异字节/大小写/空白/旧式文本一律 None
    ///（layout_io 哨兵精确匹配同律——不猜不静默纠偏）。
    #[test]
    fn parse_fail_closed() {
        assert_eq!(parse(b""), None);
        assert_eq!(parse(b"{\"dark\":0"), None); // 前缀截断
        assert_eq!(parse(b"{\"dark\":0}\n"), None); // 尾随换行
        assert_eq!(parse(b"{\"dark\":1} "), None); // 尾随空格
        assert_eq!(parse(b"{\"dark\":2}"), None); // 越域值
        assert_eq!(parse(b"{\"DARK\":1}"), None); // 大小写
        assert_eq!(parse(b"{\"dark\":true}"), None); // 异型值
        assert_eq!(parse(b"dark=1"), None); // 旧式文本
        assert_eq!(parse(&[0u8; 2048]), None); // 满簇垃圾
        // 双写叠加（两形拼接）也不得误判。
        let mut both = canonical(Theme::Light).to_vec();
        both.extend_from_slice(canonical(Theme::Dark));
        assert_eq!(parse(&both), None);
    }
}
