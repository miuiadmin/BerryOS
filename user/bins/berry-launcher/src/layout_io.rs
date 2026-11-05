//! layout_io —— 布局读写面（GUI-8 段③ W1，docs/GUI-8实施设计草案.md
//! §4 段③「布局读写」+ §3-裁决四-b fs 让位序）。
//!
//! 责任：FAT12 卷根 `LAYOUT.JSN` 读 → berry-layout parse → Owned 镜像。
//! 三约束（草案 §4 段③）：**缺席回落默认** + **读/解析失败回默认重写
//! 自愈** + **写失败非致命**（eprintln 诊断后照用内存布局，launcher 不
//! 死——常驻纪律）。
//!
//! **冻结相零扰动**（草案 R1 修约 1/2）：launcher extras 恰 [wm] →
//! `fs_h()==0` → 默认布局直返零 RPC（libc 文件面无 fs gift 本就降级
//! ENOENT——显式早退 = 零 fd 表触面零噪音，语义等价而面更窄）。
//!
//! 写路径纪律：单簇 ≤2048B 近原子（`LAYOUT_BYTES_MAX` 单源—— berry-
//! layout 双侧执法）；严格串行单 RPC 会话（fsclient 单读者协议——一
//! 请求一应答无请求序号，本模块一次只持一个 fd，open→…→close 不交
//! 叠）；**close 即 commit**（TRUNC(游标)——C-1 裁决 2 两段式，bsh
//! `put_file` 同款）。
//!
//! Owned 镜像动机：`parse` 产物 `Layout<'a>` 钉在输入缓冲（零拷贝切
//! 进），读入缓冲是 `load()` 局部——镜像拷出（app id ≤24B/格）让布局
//! 值跟调用方走，免 static mut 缓冲 + 生命周期穿透。

use berry_layout::{emit, parse, Cell, Layout, CELL_MAX, ID_MAX, LAYOUT_BYTES_MAX};
use berry_libc::fs::{open, O_CREAT, O_RDONLY, O_WRONLY};
use berry_libc::io::{close, fs_h, read, write};
use berry_libc::{get_errno, EIO, ENOENT};

use crate::desktop::{GRID_COLS, SLOTS_PER_PAGE};
use crate::icons::AppEntry;

/// 卷根布局文件（8.3 大写名单——fsmgr 单 FAT 卷根；CWD 默认根，bsh
/// 相对路径同形）。
const PATH: &[u8] = b"LAYOUT.JSN\0";

/// 单次读块（bsh `get_file` 同款 256B）。
const CHUNK: usize = 256;

/// 一格布局的 Owned 镜像（parse 产物拷出——`app` 定长 + 有效长）。
#[derive(Clone, Copy)]
pub struct OwnedCell {
    pub app: [u8; ID_MAX],
    pub app_len: u8,
    pub page: u8,
    pub row: u8,
    pub col: u8,
}

impl OwnedCell {
    /// app id 有效字节切片。
    pub fn app_bytes(&self) -> &[u8] {
        &self.app[..self.app_len as usize]
    }
}

/// 布局 Owned 镜像（槽 0..len 激活——`Layout` 家族形态）。
#[derive(Clone, Copy)]
pub struct OwnedLayout {
    cells: [OwnedCell; CELL_MAX],
    len: usize,
}

impl OwnedLayout {
    /// 激活 cells 切片。
    pub fn cells(&self) -> &[OwnedCell] {
        &self.cells[..self.len]
    }
}

/// 空 OwnedCell 模板（数组初始化面）。
const BLANK: OwnedCell = OwnedCell { app: [0; ID_MAX], app_len: 0, page: 0, row: 0, col: 0 };

/// 默认布局 = 注册表顺序装填：第 i 个 app → (page=i/12, row=(i%12)/4,
/// col=i%4)，take(CELL_MAX)——4 页 × 12 格恰满（几何三源 = desktop 视
/// 窗常量单源；PAGE_MAX=4 权威，草案 §3-裁决三）。注册表空 → 0 cell
/// （合法态——desktop 全占位格；emit 对 0 cell 拒 EmptyCells ⇒ `save`
/// 静默跳过：无可持久化内容，W1 空注册表常态）。
pub fn default_layout(reg: &[AppEntry]) -> OwnedLayout {
    let mut l = OwnedLayout { cells: [BLANK; CELL_MAX], len: 0 };
    for (i, e) in reg.iter().enumerate().take(CELL_MAX) {
        let id = e.id_bytes();
        let mut c = BLANK;
        c.app[..id.len()].copy_from_slice(id);
        c.app_len = id.len() as u8;
        c.page = (i / SLOTS_PER_PAGE) as u8;
        c.row = ((i % SLOTS_PER_PAGE) / GRID_COLS) as u8;
        c.col = (i % GRID_COLS) as u8;
        l.cells[l.len] = c;
        l.len += 1;
    }
    l
}

/// 读-判-自愈三约束收口（main 单次调用）。
pub fn load(reg: &[AppEntry]) -> OwnedLayout {
    if fs_h() == 0 {
        // 冻结相：extras 恰 [wm]——布局持久化面结构性缺席，默认直返
        //（零 RPC 零 fd 面；W1 冻结十段零字节扰动的结构性保证）。
        return default_layout(reg);
    }
    match read_file() {
        ReadOutcome::Absent => heal(reg, None),
        ReadOutcome::Oversize(n) => heal(reg, Some(format!("too big {n}"))),
        ReadOutcome::Failed(e) => heal(reg, Some(format!("read errno={e}"))),
        ReadOutcome::Bytes(b) => match parse(&b) {
            Ok(l) => owned_from(&l),
            Err(e) => heal(reg, Some(format!("parse {} at {}", e.desc(), e.at()))),
        },
    }
}

/// 回默认 + 重写自愈（`why` = 诊断前缀，None = 缺席常态零打印——首
/// boot 无文件非异常；写失败在 `save` 内非致命收口）。
fn heal(reg: &[AppEntry], why: Option<String>) -> OwnedLayout {
    if let Some(w) = why {
        eprintln!("launcher: layout {w} -- default");
    }
    let d = default_layout(reg);
    save(&d);
    d
}

/// 读结局四态。
enum ReadOutcome {
    /// ENOENT——文件缺席（首 boot 常态）。
    Absent,
    /// 全文 ≤ `LAYOUT_BYTES_MAX`。
    Bytes(Vec<u8>),
    /// 超单簇帽（异种/腐败文件面——重写自愈）。
    Oversize(usize),
    /// 其余 I/O 错（errno）。
    Failed(i32),
}

/// 全量读（bsh `get_file` 形态精简）：open(O_RDONLY) → read 循环至
/// EOF → close。读 fd 的 close 无 commit 面（幂等消费）。帽 =
/// `LAYOUT_BYTES_MAX`+1——超即停读归 Oversize（合法布局 ≤2048 单簇）。
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
        if data.len() > LAYOUT_BYTES_MAX {
            close(fd);
            return ReadOutcome::Oversize(data.len());
        }
    }
    close(fd);
    ReadOutcome::Bytes(data)
}

/// 全量写自愈（bsh `put_file` 形态）：Owned → Layout 重建 → emit
/// canonical ≤2048 → open(O_WRONLY|O_CREAT) → write 循环 → **close 即
/// commit**（TRUNC(游标)；O_TRUNC 省略 = 覆写幂等——旧长于新时残留被
/// TRUNC 截断，bsh 同款语义注记）。一切失败非致命——eprintln 后返。
fn save(owned: &OwnedLayout) {
    if owned.len == 0 {
        return; // 0 cell：emit 拒 EmptyCells——无可持久化内容
    }
    let mut l = Layout::new();
    for c in owned.cells() {
        // id 域 = parse 产物（ASCII 词法）或默认装配（ASCII id）——
        // from_utf8 失败 = 镜像破缺防御面，非致命拒写。
        let app = match core::str::from_utf8(c.app_bytes()) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("launcher: layout save bad utf8");
                return;
            }
        };
        if let Err(e) = l.push(Cell { app, page: c.page, row: c.row, col: c.col }) {
            eprintln!("launcher: layout save {}", e.desc());
            return;
        }
    }
    let mut out = [0u8; LAYOUT_BYTES_MAX];
    let n = match emit(&l, &mut out) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("launcher: layout emit {}", e.desc());
            return;
        }
    };
    if let Err(e) = put_file(&out[..n]) {
        eprintln!("launcher: layout write errno={e}");
    }
}

/// 全量写（open → write 循环 → close=commit；Err(errno) 透传，调用方
/// 非致命收口）。中途写失败仍 close——部分数据照旧提交（POSIX close
/// 恒消费，bsh 文档化取舍同款）。
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

/// parse 产物 → Owned 镜像（parse 保证条数 ≤ CELL_MAX——溢出面不可达）。
fn owned_from(l: &Layout<'_>) -> OwnedLayout {
    let mut o = OwnedLayout { cells: [BLANK; CELL_MAX], len: 0 };
    for c in l.cells() {
        let mut oc = BLANK;
        oc.app[..c.app.len()].copy_from_slice(c.app.as_bytes());
        oc.app_len = c.app.len() as u8;
        oc.page = c.page;
        oc.row = c.row;
        oc.col = c.col;
        o.cells[o.len] = oc;
        o.len += 1;
    }
    o
}
