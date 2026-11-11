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
//!
//! **段④ 冒烟往返（本模块唯一标记面）**：数据盘根 `GUI8.SMK` 的**全部
//! 字节** == [`SMOKE_MAGIC`] 时才进冒烟支路——`LAYOUT.JSN` 已存在且 ==
//! 确定性变体 → 打 `LAYOUT ok n=<cells>`（跨 boot 读回证明，恰一次）；
//! 否则写变体 + 读回自检，过链才打 `LAYOUTW ok`（写后 close=commit 已
//! 落）。哨兵缺席/载荷不符（常态 boot）= 上述零打印零额外语义；冻结相
//! 更被 `fs_h()==0` 结构性短路（哨兵探测本身也是 RPC，故必须排在此门
//! 之后）。**哨兵只读一次**：main 调 [`smoke_mode`] 取 bool，再传入
//! [`load`] 与 frameloop——本模块不再二次 open（避免读写窗内哨兵被换）。
//!
//! 变体形态（确定性，见 [`smoke_variant`]）：注册表首 app 钉 (page 0,
//! row 0, col 0)——冒烟点击靶；其余格移 page 3 并保 row/col（冲突时
//! 自 page 3 倒序 row-major 找空位）。APPDEMO+APPQUIET 下 canonical
//! emit = `{"schema":1,"cells":[{"app":"APPDEMO","page":0,"row":0,
//! "col":0},{"app":"APPQUIET","page":3,"row":0,"col":1}]}`。

use berry_layout::{emit, parse, Cell, Layout, CELL_MAX, ID_MAX, LAYOUT_BYTES_MAX};
use berry_libc::fs::{open, O_CREAT, O_RDONLY, O_WRONLY};
use berry_libc::io::{close, fs_h, read, write};
use berry_libc::{get_errno, EIO, ENOENT};

use crate::desktop::{GRID_COLS, PAGES, SLOTS_PER_PAGE};
use crate::icons::AppEntry;

/// 卷根布局文件（8.3 大写名单——fsmgr 单 FAT 卷根；CWD 默认根，bsh
/// 相对路径同形）。
const PATH: &[u8] = b"LAYOUT.JSN\0";

/// 冒烟相哨兵文件（8.3 大写；smoke-image 交互相数据盘根造此件）。**授权
/// 面 = 内容不是文件名**：仅当载荷逐字节 == [`SMOKE_MAGIC`] 才进 [`load`]
/// 的冒烟往返支路；常态 boot 绝无此载荷。
const SMOKE_PATH: &[u8] = b"GUI8.SMK\0";

/// 冒烟哨兵载荷魔数（22B，含结尾换行）。GUI8.SMK 是普通用户可创建的
/// 文件名——只凭存在性可被任意文件误触发标记面，故必须全字节精确匹配；
/// 短读/长读/任一异字节 = 非冒烟（fail closed）。
pub const SMOKE_MAGIC: &[u8] = b"BERRYOS-GUI8-SMOKE-V1\n";

/// 纯函数：哨兵载荷是否恰为 [`SMOKE_MAGIC`]（缺失由调用方以空切片表达；
/// 前缀、尾随字节、旧版文本一律 false）。
pub fn smoke_payload_matches(payload: &[u8]) -> bool {
    payload == SMOKE_MAGIC
}

/// 冒烟变体目标页（`PAGES-1` = 3——末页，非默认装填页）。
const SMOKE_VARIANT_PAGE: usize = PAGES - 1;

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

/// 读-判-自愈三约束收口（main 单次调用）。`smoke` = main 经 [`smoke_mode`]
/// 单次读取的哨兵判定（真 = 冒烟往返支路，段④ 标记面；假 = 原三约束面逐
/// 字节不变）。
pub fn load(reg: &[AppEntry], smoke: bool) -> OwnedLayout {
    if fs_h() == 0 {
        // 冻结相：extras 恰 [wm]——布局持久化面结构性缺席，默认直返
        //（零 RPC 零 fd 面；W1 冻结十段零字节扰动的结构性保证）。
        return default_layout(reg);
    }
    if smoke {
        return smoke_load(reg);
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

/// 冒烟模式探测（main 单次调用；结果同时门控 layout 冒烟往返与 frameloop
/// 的 `CLICK ok` / `launcher: click ready`）。**fail closed**：`fs_h()==0`
/// 冻结相直返 false（零 RPC）；文件缺席、I/O 失败、短读/长读、任一异字节
/// 一律 false——只认 [`SMOKE_MAGIC`] 全字节精确匹配。
pub fn smoke_mode() -> bool {
    if fs_h() == 0 {
        return false;
    }
    // 多读 1 字节：长于魔数的文件必得 len+1（不可能误判为精确匹配）。
    let mut buf = [0u8; SMOKE_MAGIC.len() + 1];
    // SAFETY: SMOKE_PATH 是 NUL 结尾字面量；fd 按 libc 契约使用。
    let fd = unsafe { open(SMOKE_PATH.as_ptr() as *const _, O_RDONLY) };
    if fd < 0 {
        return false;
    }
    let n = read(fd, buf.as_mut_ptr() as *mut _, buf.len());
    let _ = close(fd); // 只读 fd 的 close 无 commit 面
    if n < 0 {
        return false;
    }
    smoke_payload_matches(&buf[..n as usize])
}

/// 冒烟往返支路（`GUI8.SMK` 载荷精确匹配）：
///
/// - 卷根 `LAYOUT.JSN` 已在册且与 [`smoke_variant`] 全等（无视书写序，
///   按四元组多重集比对）→ 打 `LAYOUT ok n=<cells>` 后直接用（跨 boot
///   读回证明；本 boot 零写）。
/// - 缺席/异常/不等 → 写确定性变体；`persist` 过链（含 close=commit）
///   后再全量读回比对，仍全等才打 `LAYOUTW ok`。写失败非致命（沿用
///   `save` 的 eprintln 收口，launcher 照用内存变体）。
///
/// 诊断一律小写 `layout` 前缀——绝不产 `LAYOUT ok` / `LAYOUTW ok` 子串
///（标记 canon：grep -qF 存在性断言）。
fn smoke_load(reg: &[AppEntry]) -> OwnedLayout {
    // GUI-8 冒烟验收是双 app 基准（APPDEMO 点击 + APPQUIET 跨页）；
    // 桌面常态注册表现在有 9 app，冒烟支路仍取前两格钉板——canonical
    // 109B / host 对拍 /点击靶全部零漂。
    let n = reg.len().min(2);
    let want = smoke_variant(&reg[..n]);
    if want.cells().is_empty() {
        return want; // 空注册表：无可持久化内容（persist 同 save 静默跳过）
    }
    match read_file() {
        ReadOutcome::Bytes(b) => match parse(&b) {
            Ok(l) => {
                let got = owned_from(&l);
                if same_cells(&got, &want) {
                    println!("LAYOUT ok n={}", got.cells().len());
                    return got;
                }
                eprintln!("launcher: layout smoke variant differs -- rewrite");
            }
            Err(e) => eprintln!("launcher: layout smoke parse {} at {}", e.desc(), e.at()),
        },
        ReadOutcome::Absent => {}
        ReadOutcome::Oversize(n) => eprintln!("launcher: layout smoke too big {n} -- rewrite"),
        ReadOutcome::Failed(e) => eprintln!("launcher: layout smoke read errno={e} -- rewrite"),
    }
    if let Err(e) = persist(&want) {
        eprintln!("launcher: layout {e}");
        return want;
    }
    // 读回自检（同 boot 二次读——fs 面写后读一致；不一致 = 落盘面破缺，
    // 不打标记，smoke 侧由标记缺失 loud）。
    match read_file() {
        ReadOutcome::Bytes(b) => match parse(&b) {
            Ok(l) if same_cells(&owned_from(&l), &want) => println!("LAYOUTW ok"),
            Ok(_) => eprintln!("launcher: layout smoke readback differs"),
            Err(e) => eprintln!("launcher: layout smoke readback parse {}", e.desc()),
        },
        ReadOutcome::Absent => eprintln!("launcher: layout smoke readback absent"),
        ReadOutcome::Oversize(n) => eprintln!("launcher: layout smoke readback too big {n}"),
        ReadOutcome::Failed(e) => eprintln!("launcher: layout smoke readback errno={e}"),
    }
    want
}

/// 确定性冒烟变体：注册表首 app 钉 (page 0, row 0, col 0)（保持可点
/// 靶——冒烟点击注入落此格）；其余格移 [`SMOKE_VARIANT_PAGE`] 并保
/// row/col，目标位已被占（默认布局跨页同 row/col = 常态）时自页 3 倒序
/// row-major 找首个空位——48 格容量域内恒有解（`CELL_MAX` 与
/// `PAGES×SLOTS_PER_PAGE` 编译期恒等，desktop.rs 钉板）。
fn smoke_variant(reg: &[AppEntry]) -> OwnedLayout {
    let mut l = default_layout(reg);
    if l.len <= 1 {
        return l;
    }
    let mut used = [[false; SLOTS_PER_PAGE]; PAGES];
    used[0][0] = true; // 首格目标位（default_layout 首项恒 (0,0,0)）
    for i in 1..l.len {
        let row = l.cells[i].row as usize;
        let col = l.cells[i].col as usize;
        let mut slot = Some((SMOKE_VARIANT_PAGE, row, col));
        if used[SMOKE_VARIANT_PAGE][row * GRID_COLS + col] {
            slot = None;
        }
        if slot.is_none() {
            'find: for p in (0..PAGES).rev() {
                for s in 0..SLOTS_PER_PAGE {
                    if !used[p][s] {
                        slot = Some((p, s / GRID_COLS, s % GRID_COLS));
                        break 'find;
                    }
                }
            }
        }
        if let Some((p, r, c)) = slot {
            used[p][r * GRID_COLS + c] = true;
            l.cells[i].page = p as u8;
            l.cells[i].row = r as u8;
            l.cells[i].col = c as u8;
        }
    }
    l
}

/// 布局全等（四元组多重集——parse 产物按 canonical 序，变体按注册表
/// 序：书写序非语义，比对必须无视排列）。
fn same_cells(a: &OwnedLayout, b: &OwnedLayout) -> bool {
    if a.len != b.len {
        return false;
    }
    let mut used = [false; CELL_MAX];
    for ca in a.cells() {
        let mut hit = false;
        for (j, cb) in b.cells().iter().enumerate() {
            if !used[j]
                && ca.app_bytes() == cb.app_bytes()
                && ca.page == cb.page
                && ca.row == cb.row
                && ca.col == cb.col
            {
                used[j] = true;
                hit = true;
                break;
            }
        }
        if !hit {
            return false;
        }
    }
    true
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
    if let Err(e) = persist(owned) {
        eprintln!("launcher: layout {e}");
    }
}

/// 写盘收口（`save` 与冒烟往返共用）：Ok = write 全量 + close(commit)
/// 过链。Err = 诊断串（调用方决定非致命收口；串面与历史 eprintln 逐
/// 字节同形）。
fn persist(owned: &OwnedLayout) -> Result<(), String> {
    if owned.len == 0 {
        return Ok(()); // 0 cell：无可持久化内容（同 save 静默语义）
    }
    let (n, out) = encode_owned(owned)?;
    put_file(&out[..n]).map_err(|e| format!("write errno={e}"))
}

/// Owned → canonical 字节（容量 ≤ `LAYOUT_BYTES_MAX` 单簇）。`save` 的
/// 非致命 eprintln 与冒烟往返的读回自检共用本编码面（测试亦直接消费）。
fn encode_owned(owned: &OwnedLayout) -> Result<(usize, [u8; LAYOUT_BYTES_MAX]), String> {
    let mut l = Layout::new();
    for c in owned.cells() {
        // id 域 = parse 产物（ASCII 词法）或默认装配（ASCII id）——
        // from_utf8 失败 = 镜像破缺防御面，非致命拒写。
        let app = match core::str::from_utf8(c.app_bytes()) {
            Ok(s) => s,
            Err(_) => return Err("save bad utf8".into()),
        };
        if let Err(e) = l.push(Cell { app, page: c.page, row: c.row, col: c.col }) {
            return Err(format!("save {}", e.desc()));
        }
    }
    let mut out = [0u8; LAYOUT_BYTES_MAX];
    let n = match emit(&l, &mut out) {
        Ok(n) => n,
        Err(e) => return Err(format!("emit {}", e.desc())),
    };
    Ok((n, out))
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

// ---- host/交叉编译测试面（纯逻辑：变体装配 + 四元组全等 + canonical
// ---- 编码；RPC 面经 `fs_h()`/`open` 不可宿主机执行——见下注）----
//
// 可执行性注记：本 crate 唯一可编译目标是 aarch64-unknown-linux-musl
// （berry-libc 全定义 libc shim，含 AArch64 ELF 专用 inline asm/crt0）。
// 宿主机（aarch64-apple-darwin）构建在 berry-libc 的 `adrp
// BERRY_ARGV` 处硬失败，故 `cargo test` 宿主路结构性不可达；本仓
// `just test-host` 亦不收 launcher（独立 escape 小工作区，不入
// user/Cargo.toml members）。测试构建经
// `cargo test --no-run --target aarch64-unknown-linux-musl` 编译过链，
// 执行需 qemu-user（本机未装）。下列用例只碰纯函数，零 RPC/零 fd。
#[cfg(test)]
mod tests {
    use super::*;

    /// 造一条注册表条目（label/icon 面不参与布局逻辑）。
    fn entry(id: &str) -> AppEntry {
        let mut e = AppEntry {
            id: [0; ID_MAX],
            id_len: 0,
            label: [0; 64],
            label_len: 0,
            icon: crate::icons::NO_ICON,
        };
        e.id[..id.len()].copy_from_slice(id.as_bytes());
        e.id_len = id.len() as u8;
        e
    }

    fn emit_owned(o: &OwnedLayout) -> String {
        let (n, buf) = encode_owned(o).expect("encode ok");
        core::str::from_utf8(&buf[..n]).expect("utf8").to_string()
    }

    /// 双 app 冒烟镜像：变体 = 首格钉 (0,0,0) + 其余移 page 3，canonical
    /// emit 恰 109B（22+36×2+7+8）——冒烟相宿主侧字节对拍基准。
    #[test]
    fn smoke_variant_two_apps_canonical_bytes() {
        let reg = [entry("APPDEMO"), entry("APPQUIET")];
        let v = smoke_variant(&reg);
        assert_eq!(v.cells().len(), 2);
        assert_eq!(v.cells()[0].app_bytes(), b"APPDEMO");
        assert_eq!((v.cells()[0].page, v.cells()[0].row, v.cells()[0].col), (0, 0, 0));
        assert_eq!(v.cells()[1].app_bytes(), b"APPQUIET");
        assert_eq!((v.cells()[1].page, v.cells()[1].row, v.cells()[1].col), (3, 0, 1));
        let doc = emit_owned(&v);
        assert_eq!(
            doc,
            r#"{"schema":1,"cells":[{"app":"APPDEMO","page":0,"row":0,"col":0},{"app":"APPQUIET","page":3,"row":0,"col":1}]}"#
        );
        assert_eq!(doc.len(), 109);
        // 变体 ≠ 默认（LAYOUTW/LAYOUT 判据的判别力前提）。
        assert!(!same_cells(&smoke_variant(&reg), &default_layout(&reg)));
    }

    /// 注满 CELL_MAX（48）仍须无重复格（emit DupCell 硬门）且首格不动
    /// ——默认布局跨页同 row/col 是常态，变体冲突回退面活证。
    #[test]
    fn smoke_pair_from_nine_apps_is_canonical_two_cells() {
        let reg: Vec<AppEntry> = ["APPDEMO", "APPQUIET", "BROWSER", "CAMERA", "CLOCK"]
            .iter()
            .map(|s| entry(s))
            .collect();
        let n = reg.len().min(2);
        let v = smoke_variant(&reg[..n]);
        assert_eq!(v.cells().len(), 2);
        assert_eq!(emit_owned(&v).len(), 109);
    }

    #[test]
    fn smoke_variant_cell_max_is_collision_free() {
        let ids: Vec<String> = (0..CELL_MAX).map(|i| format!("APP{i:02}")).collect();
        let reg: Vec<AppEntry> = ids.iter().map(|s| entry(s)).collect();
        let d = default_layout(&reg);
        let v = smoke_variant(&reg);
        assert_eq!(v.cells().len(), CELL_MAX);
        assert_eq!(
            (v.cells()[0].app_bytes(), v.cells()[0].page, v.cells()[0].row, v.cells()[0].col),
            (&b"APP00"[..], 0, 0, 0)
        );
        let mut seen = [[false; SLOTS_PER_PAGE]; PAGES];
        for c in v.cells() {
            let s = c.row as usize * GRID_COLS + c.col as usize;
            assert!(!seen[c.page as usize][s], "duplicate slot in variant");
            seen[c.page as usize][s] = true;
        }
        // 每 app 恰一格（变体只搬坐标，不增不减）。
        for a in d.cells() {
            assert_eq!(
                v.cells().iter().filter(|b| a.app_bytes() == b.app_bytes()).count(),
                1
            );
        }
        let _ = emit_owned(&v); // 过 emit 全域执法（Range/DupCell/TooBig）
    }

    /// 全等 = 四元组多重集（parse canonical 序 vs 注册表序）；坐标/身份
    /// 任一不同即不等。
    #[test]
    fn same_cells_order_insensitive_and_strict() {
        let reg = [entry("APPDEMO"), entry("APPQUIET")];
        let a = smoke_variant(&reg);
        let doc = emit_owned(&a);
        let b = owned_from(&parse(doc.as_bytes()).expect("parse ok"));
        assert!(same_cells(&a, &b));
        assert!(same_cells(&b, &a));
        let mut bad = b;
        bad.cells[1].page = 0;
        assert!(!same_cells(&a, &bad));
        let mut short = b;
        short.len = 1;
        assert!(!same_cells(&a, &short));
    }

    /// 空注册表：0 cell（emit 拒 EmptyCells——`persist`/`save` 静默跳过，
    /// 冒烟支路亦不打标记）。
    #[test]
    fn smoke_variant_empty_registry_is_empty() {
        let v = smoke_variant(&[]);
        assert_eq!(v.cells().len(), 0);
        assert!(persist(&v).is_ok());
    }

    /// 哨兵授权面 = 精确载荷（不是文件名/存在性）：只认全字节相等；旧
    /// 文本、缺尾换行、多一字节、前缀截断、任意异字节一律 fail closed。
    #[test]
    fn smoke_payload_requires_exact_magic_bytes() {
        assert_eq!(SMOKE_MAGIC, b"BERRYOS-GUI8-SMOKE-V1\n");
        assert_eq!(SMOKE_MAGIC.len(), 22);
        assert!(smoke_payload_matches(SMOKE_MAGIC));
        // 历史 harness 文本（旧版内容）不再授权。
        assert!(!smoke_payload_matches(b"BERRYOS GUI8 INTERACTIVE SMOKE MARKER v1\n"));
        // 缺尾换行 / 多一字节 / 空文件 / 前缀。
        assert!(!smoke_payload_matches(b"BERRYOS-GUI8-SMOKE-V1"));
        assert!(!smoke_payload_matches(b"BERRYOS-GUI8-SMOKE-V1\n\n"));
        assert!(!smoke_payload_matches(b""));
        assert!(!smoke_payload_matches(b"BERRYOS-GUI8-SMOKE"));
        // 单字节替换（大小写敏感）。
        let mut bad = SMOKE_MAGIC.to_vec();
        bad[0] = b'b';
        assert!(!smoke_payload_matches(&bad));
    }
}
