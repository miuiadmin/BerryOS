//! # berry-layout —— BerryOS 桌面布局 JSON v1 纯函数解析/发射器
//!
//! `docs/GUI-8实施侦察-2026-11.md` §3.4（GUI-8 布局持久化：数据模型照抄
//! OpenHarmony LNCH 逐格一行——(page,row,col) 三元组定位 + app 引用；介质
//! 换 FAT12 卷根 `LAYOUT.JSN`：8.3 大写名单文件、只覆写不可删、覆写幂等。
//! 布局文件只存排布不存资产——图标/label 从 appd 资产面取）。「解析器即
//! 产品」家族第 9 例（loader/fsproto/virtio/blkproto/fatfs/netproto/
//! netdproto/tcp/manifest 之后）：**零分配、零 unsafe、零依赖、输入只
//! 借用、no_std + host 测内联**——消费面 launcher 是 musl target std 程序
//! （独立小工作区），本 crate 以 members 身份挂 host 测试（berry-manifest
//! 同款形态：消费面是 none/musl target 程序，带 std/alloc 依赖即构建链
//! 编译失败）。
//!
//! ## schema v1（冻结到字节级——侦察 §3.4 schema 六律蒸馏）
//!
//! - 顶层恰两键：`{"schema":1,"cells":[…]}`；cell 恰四键：
//!   `{"app":"APPDEMO","page":0,"row":0,"col":0}`——**键序固定**（语法位
//!   即 canonical 序：任何键序位上键与期望不符一律 `UnknownKey`，替换位
//!   与追加位皆然——「新键包在旧解析器上炸响 = 显式版本不匹配信号」，
//!   键 add-only 随版本域走；「schema 缺」亦折此面——固定键序语法里
//!   错位即不识）。
//! - 版本域：单 int `schema`，parser 只认 `==1`；其它值（0/2/饱和巨值）
//!   loud 拒（`Version`——found 带结构化诊断值）。
//! - cells 恰 ≥1 条：空数组 loud 拒（`EmptyCells`）——**无文件 = 消费方
//!   默认布局**，缺席语义归 launcher，不是本 crate 的事。
//! - (page,row,col) 三元组全布局唯一，重复 loud 拒（`DupCell`）；同 app
//!   多格 v1 允许（一 app 摆多页多格是合法桌面形态——不禁，禁则随 v2
//!   版本域显式走）。
//!
//! ## canonical 形（六律之⑥：确定性输出——同布局同字节）
//!
//! 紧凑单行零空白（空格/tab/LF/CR 任何位置即拒——全文级门，串内亦然）；
//! 无转义（id 词法不含 `\` 与 `"`，反斜杠在串内即拒）；整数纯十进制无
//! 前导零（`0` 合法、`01` 拒）无 `+` 无负（坐标域非负）；cells 按
//! (page,row,col) 升序。`emit` 恒产 canonical（乱序输入先排序后发射）；
//! `check_canonical(bytes)` = parse→emit→逐字节相等——AI 生成面/宿主
//! 工具的 canonical 镜像门禁（发射侧与解析侧的字节共识契约；乱序/带
//! 空白/键序错均 false）。
//!
//! ## 语法子集（仅 对象/数组/字符串/非负整数）
//!
//! 拒 float（`1.0` 折在 `.`）/bool（`true` 折在首字节）/null/尾逗号/
//! 嵌套超两层（语法面天然封顶：顶层对象 → cells 数组 → cell 对象，值位
//! 只收 string/int）/trailing garbage；字节域限可打印 ASCII 0x21..=0x7E
//! （空白四字节另有专属变体先行执法）；字符串值域 = id 词法（非词法
//! 字符即 `BadId`）。
//!
//! ## 容量公式（诚实注——「编不出就拒绝，不截断」）
//!
//! canonical 总长 = `22 + 36n + Σ|app|`：头 `{"schema":1,"cells":[` 21B
//! + 尾 `]}` 2B；每 cell 固定 32B + 三坐标各恒 1 数字节（域 ≤7 天然
//! 单位）+ id 长 = 35B+|app|，后续 cell 各带 1 逗号 → 22 + 36n + Σ|app|。
//! **worst case：48 cell × 24B id = 2902B > LAYOUT_BYTES_MAX=2048B**——
//! 语义全合法的布局可能编不进单簇帽：emit 层 loud 拒（`TooBig`），不
//! 截断不加压。反推舒适区：48 格满盘时 Σ|app| ≤ 298（平均 id ≈6B）。
//!
//! ## 帽常量单源 + 跨面镜像义务
//!
//! - `LAYOUT_BYTES_MAX=2048`：≤2048B 单簇 FAT 文件掉电三态近原子（终态
//!   = 旧版全文/新版全文/缺席——读簇 0 直取 root.first 不验 FAT，半文件
//!   死状结构性不可达；侦察 §3.4 推导追认为硬帽）。parser 与 emitter
//!   **双侧执法**；launcher 侧或宿主工具镜像时以本常量定义处为对账锚点
//!   （mkfat → berry-fatfs 头注义务同款）。
//! - `CELL_MAX=48` / `PAGE_MAX=4` / `ROW_MAX=8` / `COL_MAX=8`：桌面几何
//!   帽（4 页 × 8 行 × 8 列 = 256 格，48 = 恰满一屏图标密度）——同款
//!   单源义务，排他上界（合法域 0..=3/0..=7/0..=7）。
//! - `ID_MAX=24` + 词法 `[A-Za-z0-9._-]{1,24}`：**帽值与
//!   `berry-manifest::ID_MAX` 同源同值**（不引依赖——注释锚 + 测试钉，
//!   **改帽双侧同步**）。manifest 另有「首字符字母数字」律；layout 词法
//!   是其超集（宽松收——未安装 id 的回落语义归消费方，不在 parser 编死）。
//! - 消费方 = GUI-8 launcher（读写 LAYOUT.JSN）+ AI 生成面（canonical
//!   镜像义务：凡产布局字节必过 `check_canonical`）。

#![cfg_attr(not(test), no_std)]

use core::str::from_utf8;

// ---- 帽常量（单一事实源 = 本 crate；跨面镜像义务见模块头注）----

/// 布局文件 ≤2048B（单簇 FAT 掉电三态近原子帽——parser 与 emitter 双侧
/// 执法，「编不出就拒绝，不截断」）。
pub const LAYOUT_BYTES_MAX: usize = 2048;
/// cells 条数帽（恰满一屏图标密度）。
pub const CELL_MAX: usize = 48;
/// 页号域排他上界：合法 page ∈ 0..=3。
pub const PAGE_MAX: u8 = 4;
/// 行号域排他上界：合法 row ∈ 0..=7。
pub const ROW_MAX: u8 = 8;
/// 列号域排他上界：合法 col ∈ 0..=7。
pub const COL_MAX: u8 = 8;
/// app id 帽——**与 berry-manifest::ID_MAX 同源同值 24**（不引依赖，
/// 注释锚 + 测试钉：两侧 = 本 crate `cap_anchor_pins` + manifest
/// `id_max_pinned`，改帽任一侧即红；`tools/mkapp.py` python 镜像仍
/// 注释锚——改帽三处同步。词法见 `id_offend`）。
pub const ID_MAX: usize = 24;
/// 布局 schema 版本域唯一在册值（parser 只认 ==1；其它 loud 拒）。
pub const SCHEMA_VERSION: u32 = 1;

// ---- 解析产物 ----

/// 一格桌面排布：app 引用 + (page,row,col) 定位（OpenHarmony LNCH 逐格
/// 一行同构）。全借用——零拷贝切进输入缓冲。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell<'a> {
    /// app id（id 词法 `[A-Za-z0-9._-]{1,24}`；引用完整性归消费方）。
    pub app: &'a str,
    pub page: u8,
    pub row: u8,
    pub col: u8,
}

/// 解析/手构的布局（cells 定长数组 + len——零分配；激活面 = `cells()`
/// 切片）。同 app 多格允许；(page,row,col) 唯一性由 parse/emit 执法
/// （手构面在 emit 收口统一裁断）。
#[derive(Debug, Clone, Copy, Eq)]
pub struct Layout<'a> {
    /// 槽 0..len 为激活 cells；尾部槽是未激活垫层（`PartialEq` 只比激活面）。
    /// 私有（家族形态——berry-manifest `ManifestErr` 同款）：`len ≤ CELL_MAX`
    /// 不变量由 `new`/`push`/`parse` 构造面维护，绕构造器直构从构造上
    /// 不可达——`cells()` 裸切永不越界（in-crate 测试可直构钉防御层）。
    cells: [Cell<'a>; CELL_MAX],
    len: usize,
}

impl<'a> Layout<'a> {
    /// 空布局（0 cell——只作手构起点；emit 对 0 cell loud 拒）。
    pub const fn new() -> Self {
        Layout {
            cells: [Cell { app: "", page: 0, row: 0, col: 0 }; CELL_MAX],
            len: 0,
        }
    }

    /// 激活 cells 切片（`cells[..len]`）。
    pub fn cells(&self) -> &[Cell<'a>] {
        &self.cells[..self.len]
    }

    /// 追加一格（仅查条数帽——值域/词法/唯一性归 emit 收口统一执法）。
    pub fn push(&mut self, c: Cell<'a>) -> Result<(), LayoutErr> {
        if self.len >= CELL_MAX {
            return Err(LayoutErr::CellCap { at: 0, count: self.len + 1 });
        }
        self.cells[self.len] = c;
        self.len += 1;
        Ok(())
    }
}

/// PartialEq 只比激活面（垫层槽不参与——手构中间态不被未激活槽污染）。
impl<'a> PartialEq for Layout<'a> {
    fn eq(&self, o: &Self) -> bool {
        self.cells() == o.cells()
    }
}

// ---- 错误面（语义化 enum——结构化诊断，AI 可读）----

/// 解析/发射失败理由。parse 侧变体带冒犯字节偏移 `at`；emit/push 侧无
/// 输入面（at 恒 0，语义在 desc/数值字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutErr {
    /// 超 `LAYOUT_BYTES_MAX`（parse 侧 len = 输入字节数；emit 侧 len =
    /// canonical 展开长——「编不出就拒绝，不截断」）。
    TooBig { len: usize },
    /// cells 条数超 `CELL_MAX`（count = 实见条数）。
    CellCap { at: usize, count: usize },
    /// page/row/col 越域（at = 数值 token 起始偏移）。
    Range { at: usize },
    /// (page,row,col) 三元组重复（at = 重复 cell 起始偏移）。
    DupCell { at: usize },
    /// 键序位上陌生键/错位键（顶层与 cell 皆，替换位与追加位皆；含
    /// 「schema 缺」——固定键序语法里错位即不识）。
    UnknownKey { at: usize },
    /// schema 值 != 1（found = 饱和解析值）。
    Version { at: usize, found: u32 },
    /// 空白字节（canonical 零空白：空格/tab/LF/CR 任何位置即拒）。
    Whitespace { at: usize },
    /// 非 0x21..=0x7E 字节（多字节 UTF-8/控制字节皆此面）。
    NonAscii { at: usize },
    /// app id 词法违例（at = 串内容首冒犯字节偏移；空串 = 内容起点；
    /// 超长 = 第 ID_MAX+1 字节位）。
    BadId { at: usize },
    /// cells 空数组（at = `]` 偏移）——无文件 ≠ 空数组（缺席 = 消费方
    /// 默认布局）。
    EmptyCells { at: usize },
    /// 其余语法错（未闭合串/尾逗号/前导零/符号/小数点/bool/null/坏
    /// 结构/trailing garbage——at = 冒犯字节偏移）。
    Syntax { at: usize },
    /// emit 输出窗不足（need = canonical 长，have = 调用方窗长）。
    NoRoom { need: usize, have: usize },
}

impl LayoutErr {
    /// eco 诊断行短理由（全 ASCII ≤24B——launcher 降级行发射窗按
    /// berry-manifest ManifestErr 同口径设计）。
    pub fn desc(&self) -> &'static str {
        match self {
            LayoutErr::TooBig { .. } => "too big",
            LayoutErr::CellCap { .. } => "too many cells",
            LayoutErr::Range { .. } => "coord range",
            LayoutErr::DupCell { .. } => "dup cell",
            LayoutErr::UnknownKey { .. } => "unknown key",
            LayoutErr::Version { .. } => "schema mismatch",
            LayoutErr::Whitespace { .. } => "whitespace",
            LayoutErr::NonAscii { .. } => "non-ascii",
            LayoutErr::BadId { .. } => "bad app id",
            LayoutErr::EmptyCells { .. } => "empty cells",
            LayoutErr::Syntax { .. } => "bad syntax",
            LayoutErr::NoRoom { .. } => "no room",
        }
    }

    /// 冒犯字节偏移（emit/push 侧错误无输入面恒 0；TooBig/NoRoom 无位点）。
    pub fn at(&self) -> usize {
        match self {
            LayoutErr::TooBig { .. } | LayoutErr::NoRoom { .. } => 0,
            LayoutErr::CellCap { at, .. }
            | LayoutErr::Range { at }
            | LayoutErr::DupCell { at }
            | LayoutErr::UnknownKey { at }
            | LayoutErr::Version { at, .. }
            | LayoutErr::Whitespace { at }
            | LayoutErr::NonAscii { at }
            | LayoutErr::BadId { at }
            | LayoutErr::EmptyCells { at }
            | LayoutErr::Syntax { at } => *at,
        }
    }
}

// ---- 内部词法/文法 ----

/// id 词法执法：`[A-Za-z0-9._-]{1,ID_MAX}`（帽值镜像 berry-manifest——
/// 见模块头注）。返回首冒犯字节下标：空串 = 0；非法字符 = 其位；超长 =
/// ID_MAX（第 25 字节位）。合法 = None。
fn id_offend(id: &[u8]) -> Option<usize> {
    if id.is_empty() {
        return Some(0);
    }
    for (i, &b) in id.iter().enumerate() {
        if !(b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-') {
            return Some(i);
        }
    }
    if id.len() > ID_MAX {
        return Some(ID_MAX);
    }
    None
}

/// 光标（字节偏移全程保留——诊断面的位点单源）。
struct P<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> P<'a> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn eat(&mut self, c: u8) -> Result<(), LayoutErr> {
        match self.peek() {
            Some(x) if x == c => {
                self.i += 1;
                Ok(())
            }
            _ => Err(LayoutErr::Syntax { at: self.i }),
        }
    }

    /// 键序位执法：此处应见 <expect>，实遇任何其它键（陌生键/已知键
    /// 错位）一律 `UnknownKey`。
    fn key(&mut self, expect: &[u8]) -> Result<(), LayoutErr> {
        let start = self.i;
        if self.string_token()? != expect {
            return Err(LayoutErr::UnknownKey { at: start });
        }
        Ok(())
    }

    /// JSON 串 token：吃引号对，返回内容字节段。反斜杠一律拒（无转义
    /// 语义）；未闭合拒（at = 起始引号位）。内容字符集已由全文门保证
    /// 可打印 ASCII。
    fn string_token(&mut self) -> Result<&'a [u8], LayoutErr> {
        let start = self.i;
        self.eat(b'"')?;
        let s = self.i;
        loop {
            match self.peek() {
                Some(b'"') => {
                    let out = &self.b[s..self.i];
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => return Err(LayoutErr::Syntax { at: self.i }),
                Some(_) => self.i += 1,
                None => return Err(LayoutErr::Syntax { at: start }),
            }
        }
    }

    /// 非负十进制 int token：`[0-9]+`、无前导零（`0` 合法、`01` 拒）；
    /// 溢出饱和到 u32::MAX（值域执法在调用方：schema !=1 → Version、
    /// 坐标越域 → Range）。token 后必须紧跟结构字节 `,`/`}`/`]`——
    /// `1.0`/`1e5` 折在小数点/字母位（float 子集外），`true`/`null`
    /// 折在首字节。返回 (值, token 起始偏移)。
    fn uint(&mut self) -> Result<(u32, usize), LayoutErr> {
        let start = self.i;
        let first = match self.peek() {
            Some(d) if d.is_ascii_digit() => d,
            _ => return Err(LayoutErr::Syntax { at: start }),
        };
        let mut n: u32 = 0;
        while let Some(d) = self.peek() {
            if !d.is_ascii_digit() {
                break;
            }
            n = n.saturating_mul(10).saturating_add((d - b'0') as u32);
            self.i += 1;
        }
        if first == b'0' && self.i - start > 1 {
            return Err(LayoutErr::Syntax { at: start + 1 }); // 前导零
        }
        match self.peek() {
            Some(b',') | Some(b'}') | Some(b']') => Ok((n, start)),
            _ => Err(LayoutErr::Syntax { at: self.i }),
        }
    }
}

fn parse_cell<'a>(p: &mut P<'a>) -> Result<Cell<'a>, LayoutErr> {
    p.eat(b'{')?;
    p.key(b"app")?;
    p.eat(b':')?;
    let id_at = p.i + 1; // 串内容起点（起始引号后）
    let id = p.string_token()?;
    if let Some(k) = id_offend(id) {
        return Err(LayoutErr::BadId { at: id_at + k });
    }
    p.eat(b',')?;
    p.key(b"page")?;
    p.eat(b':')?;
    let (page, at) = p.uint()?;
    if page >= PAGE_MAX as u32 {
        return Err(LayoutErr::Range { at });
    }
    p.eat(b',')?;
    p.key(b"row")?;
    p.eat(b':')?;
    let (row, at) = p.uint()?;
    if row >= ROW_MAX as u32 {
        return Err(LayoutErr::Range { at });
    }
    p.eat(b',')?;
    p.key(b"col")?;
    p.eat(b':')?;
    let (col, at) = p.uint()?;
    if col >= COL_MAX as u32 {
        return Err(LayoutErr::Range { at });
    }
    // cell 收口：`}` 合法；`,` 后 `}` = 尾逗号 Syntax；`,` + 键 = 追加
    // 未知 cell 键。
    match p.peek() {
        Some(b'}') => p.i += 1,
        Some(b',') => {
            let at = p.i + 1;
            p.i += 1;
            if p.peek() == Some(b'}') {
                return Err(LayoutErr::Syntax { at: p.i });
            }
            p.string_token()?;
            return Err(LayoutErr::UnknownKey { at });
        }
        _ => return Err(LayoutErr::Syntax { at: p.i }),
    }
    // 字节集已由 id_offend 验为 ASCII 词法——from_utf8 必然成功，
    // unwrap_or 仅防御（永不 panic 承诺）。
    Ok(Cell {
        app: from_utf8(id).unwrap_or(""),
        page: page as u8,
        row: row as u8,
        col: col as u8,
    })
}

/// 布局 JSON v1 解析主入口。帽先于语法：>`LAYOUT_BYTES_MAX` 先折
/// `TooBig`，再全文空白/字节域门，然后才进文法。cells 排序序不设限
/// （canonical 判定归 `check_canonical`——emit 恒产升序）。永不 panic；
/// 一切拒绝经 `LayoutErr` 带字节偏移。
pub fn parse(bytes: &[u8]) -> Result<Layout<'_>, LayoutErr> {
    if bytes.len() > LAYOUT_BYTES_MAX {
        return Err(LayoutErr::TooBig { len: bytes.len() });
    }
    for (i, &b) in bytes.iter().enumerate() {
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            return Err(LayoutErr::Whitespace { at: i });
        }
        if !(0x21..=0x7E).contains(&b) {
            return Err(LayoutErr::NonAscii { at: i });
        }
    }

    let mut p = P { b: bytes, i: 0 };
    p.eat(b'{')?;
    // 顶层键 1/2：schema（键序固定——错位/陌生键折 UnknownKey）
    p.key(b"schema")?;
    p.eat(b':')?;
    let (v, at) = p.uint()?;
    if v != SCHEMA_VERSION {
        return Err(LayoutErr::Version { at, found: v });
    }
    p.eat(b',')?;
    // 顶层键 2/2：cells
    p.key(b"cells")?;
    p.eat(b':')?;
    p.eat(b'[')?;

    let mut layout = Layout::new();
    if p.peek() == Some(b']') {
        return Err(LayoutErr::EmptyCells { at: p.i });
    }
    loop {
        let cell_start = p.i;
        let cell = parse_cell(&mut p)?;
        // 三元组唯一（同 app 多格允许——比的是坐标不是 id）
        for c in layout.cells() {
            if (c.page, c.row, c.col) == (cell.page, cell.row, cell.col) {
                return Err(LayoutErr::DupCell { at: cell_start });
            }
        }
        if layout.len >= CELL_MAX {
            return Err(LayoutErr::CellCap { at: cell_start, count: layout.len + 1 });
        }
        layout.cells[layout.len] = cell;
        layout.len += 1;
        match p.peek() {
            Some(b']') => {
                p.i += 1;
                break;
            }
            Some(b',') => {
                p.i += 1;
                if p.peek() == Some(b']') {
                    return Err(LayoutErr::Syntax { at: p.i }); // 尾逗号
                }
            }
            _ => return Err(LayoutErr::Syntax { at: p.i }),
        }
    }
    // 顶层收口：`}` 合法；`,` + 键 = 追加未知顶层键（「新键包在旧
    // 解析器上炸响」面）。
    match p.peek() {
        Some(b'}') => p.i += 1,
        Some(b',') => {
            let at = p.i + 1;
            p.i += 1;
            p.string_token()?;
            return Err(LayoutErr::UnknownKey { at });
        }
        _ => return Err(LayoutErr::Syntax { at: p.i }),
    }
    if p.i != p.b.len() {
        return Err(LayoutErr::Syntax { at: p.i }); // trailing garbage
    }
    Ok(layout)
}

fn triple(c: &Cell<'_>) -> (u8, u8, u8) {
    (c.page, c.row, c.col)
}

/// 写cursor（零分配直写 out——NoRoom 前置检查后恒成功）。
struct W<'o> {
    out: &'o mut [u8],
    pos: usize,
}

impl<'o> W<'o> {
    fn put(&mut self, s: &[u8]) {
        self.out[self.pos..self.pos + s.len()].copy_from_slice(s);
        self.pos += s.len();
    }
}

/// canonical 发射：校验（条数/值域/词法/唯一性）→ (page,row,col) 升序
/// 排序 → 写入 `out` 返回字节数。**恒产 canonical**（乱序输入先排序）；
/// canonical 展开超 `LAYOUT_BYTES_MAX` 或输出窗不足均 loud 拒——「编不
/// 出就拒绝，不截断」。emit 侧错误无输入偏移面（at 恒 0）。
pub fn emit(layout: &Layout<'_>, out: &mut [u8]) -> Result<usize, LayoutErr> {
    let n = layout.len;
    if n == 0 {
        return Err(LayoutErr::EmptyCells { at: 0 });
    }
    if n > CELL_MAX {
        return Err(LayoutErr::CellCap { at: 0, count: n });
    }
    // 值域/词法执法（parse 同款约束单源同款——emit 恒产 canonical 的前提）
    for c in &layout.cells[..n] {
        if c.page >= PAGE_MAX || c.row >= ROW_MAX || c.col >= COL_MAX {
            return Err(LayoutErr::Range { at: 0 });
        }
        if id_offend(c.app.as_bytes()).is_some() {
            return Err(LayoutErr::BadId { at: 0 });
        }
    }
    // 排序置换表（插入排序零分配）；相邻等键 = 三元组重复
    let mut order: [usize; CELL_MAX] = [0; CELL_MAX];
    for k in 0..n {
        order[k] = k;
    }
    for k in 1..n {
        let mut j = k;
        while j > 0 && triple(&layout.cells[order[j]]) < triple(&layout.cells[order[j - 1]]) {
            order.swap(j - 1, j);
            j -= 1;
        }
    }
    for w in 1..n {
        if triple(&layout.cells[order[w - 1]]) == triple(&layout.cells[order[w]]) {
            return Err(LayoutErr::DupCell { at: 0 });
        }
    }
    // 容量公式（头注容量诚实注同源）：22 + 36n + Σ|app|（三坐标域 ≤7
    // 恒 1 数字节——值域执法在前保证）。
    let total = 22 + 36 * n + layout.cells[..n].iter().map(|c| c.app.len()).sum::<usize>();
    if total > LAYOUT_BYTES_MAX {
        return Err(LayoutErr::TooBig { len: total });
    }
    if out.len() < total {
        return Err(LayoutErr::NoRoom { need: total, have: out.len() });
    }
    let mut w = W { out, pos: 0 };
    w.put(b"{\"schema\":1,\"cells\":[");
    for k in 0..n {
        if k > 0 {
            w.put(b",");
        }
        let c = &layout.cells[order[k]];
        w.put(b"{\"app\":\"");
        w.put(c.app.as_bytes());
        w.put(b"\",\"page\":");
        w.put(&[b'0' + c.page]); // 值域 ≤7 恒单位数字（Range 执法在前保证）
        w.put(b",\"row\":");
        w.put(&[b'0' + c.row]);
        w.put(b",\"col\":");
        w.put(&[b'0' + c.col]);
        w.put(b"}");
    }
    w.put(b"]}");
    debug_assert_eq!(w.pos, total);
    Ok(w.pos)
}

/// canonical 门禁：parse→emit→逐字节相等。AI 生成面/宿主工具镜像义务的
/// 执法点——凡产布局字节必过此关。parse 拒绝（空白/键序错/语法错/超帽
/// 皆然）或 cells 排序序非升序均 false。
pub fn check_canonical(bytes: &[u8]) -> bool {
    match parse(bytes) {
        Ok(l) => {
            let mut buf = [0u8; LAYOUT_BYTES_MAX];
            match emit(&l, &mut buf) {
                Ok(n) => buf[..n] == *bytes,
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

// ---- host 测试面（先写钉板再动实现——家族纪律；正例 17 + parse 侧
// ---- 负例 39 + emit 侧负例 5 = 61）----

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小正例（canonical 字节串整串钉板——65B = 22+36×1+7，容量公式
    /// 的活证）。
    const MIN: &str = r#"{"schema":1,"cells":[{"app":"APPDEMO","page":0,"row":0,"col":0}]}"#;

    /// 双 cell 乱序形（parse 收、check_canonical 假——排序序归 canonical）。
    const TWO_UNSORTED: &str = r#"{"schema":1,"cells":[{"app":"B","page":1,"row":0,"col":0},{"app":"A","page":0,"row":0,"col":0}]}"#;

    fn ok(s: &str) -> Layout<'_> {
        parse(s.as_bytes()).expect("parse ok")
    }

    fn bad(s: &str) -> LayoutErr {
        parse(s.as_bytes()).expect_err("parse err")
    }

    fn cell_json(app: &str, page: u32, row: u32, col: u32) -> String {
        format!(r#"{{"app":"{}","page":{},"row":{},"col":{}}}"#, app, page, row, col)
    }

    fn doc_json(cells: &[String]) -> String {
        format!(r#"{{"schema":1,"cells":[{}]}}"#, cells.join(","))
    }

    fn layout_of<'a>(cells: &'a [(&'a str, u8, u8, u8)]) -> Layout<'a> {
        let mut l = Layout::new();
        for &(app, page, row, col) in cells {
            l.push(Cell { app, page, row, col }).expect("push");
        }
        l
    }

    fn emit_str(layout: &Layout<'_>) -> String {
        let mut buf = [0u8; LAYOUT_BYTES_MAX];
        let n = emit(layout, &mut buf).expect("emit ok");
        String::from_utf8(buf[..n].to_vec()).expect("utf8")
    }

    /// 恰 2048B 文档：48 cell（page 0，r 0..6 × c 0..8 升序）+ Σ|app|=298
    /// （2×11B + 46×6B）——22+36×48+298 = 2048 恰满单簇帽。last_extra
    /// 给末 cell id 加长（0 = 恰 2048；1 = 2049 越帽）。
    fn doc_48(last_extra: usize) -> String {
        let mut cs = Vec::new();
        let mut i = 0;
        for r in 0..6u32 {
            for c in 0..8u32 {
                let len = if i < 2 {
                    11
                } else if i == 47 {
                    6 + last_extra
                } else {
                    6
                };
                cs.push(cell_json(&"a".repeat(len), 0, r, c));
                i += 1;
            }
        }
        assert_eq!(i, CELL_MAX);
        doc_json(&cs)
    }

    // ---- 正例 ----

    // 1. 最小正例逐字段钉板。
    #[test]
    fn minimal_parse_pin() {
        let l = ok(MIN);
        assert_eq!(l.len, 1);
        let c = &l.cells()[0];
        assert_eq!((c.app, c.page, c.row, c.col), ("APPDEMO", 0, 0, 0));
    }

    // 2. canonical 字节串整串钉 + 容量公式活证（65 = 22+36×1+7）。
    #[test]
    fn canonical_bytes_pinned() {
        assert_eq!(MIN.len(), 22 + 36 * 1 + "APPDEMO".len());
        assert_eq!(MIN.len(), 65);
        assert_eq!(emit_str(&ok(MIN)), MIN);
        assert!(check_canonical(MIN.as_bytes()));
    }

    // 3. 恰 2048B（单簇帽满格正例）解析 + 往返 + canonical；2049 折
    //    TooBig（帽值 `<` vs `<=` 漂移防线）。
    #[test]
    fn exact_2048_parses_and_roundtrips() {
        let s = doc_48(0);
        assert_eq!(s.len(), LAYOUT_BYTES_MAX);
        let l = ok(&s);
        assert_eq!(l.len, CELL_MAX);
        assert!(check_canonical(s.as_bytes()));
        assert_eq!(emit_str(&l), s);
    }

    // 4. 48 cell 满 cap 手构往返（emit→parse→逐字节）。
    #[test]
    fn full_48_cells_roundtrip() {
        let mut cells = Vec::new();
        for r in 0..6u8 {
            for c in 0..8u8 {
                cells.push(("a", 0u8, r, c));
            }
        }
        let l = layout_of(&cells);
        let s = emit_str(&l);
        assert_eq!(s.len(), 22 + 36 * CELL_MAX + CELL_MAX); // Σ|app| = 48×1
        let back = ok(&s);
        assert_eq!(back.cells(), l.cells());
        assert!(check_canonical(s.as_bytes()));
    }

    // 5. emit 恒产 canonical：乱序输入先排序后发射；parse 保输入序。
    #[test]
    fn emit_sorts_unsorted_layout() {
        let l = ok(TWO_UNSORTED);
        assert_eq!(l.cells()[0].app, "B"); // parse 保输入序（不规整化）
        assert_eq!(
            emit_str(&l),
            r#"{"schema":1,"cells":[{"app":"A","page":0,"row":0,"col":0},{"app":"B","page":1,"row":0,"col":0}]}"#
        );
    }

    // 6. canonical 不动点：emit(parse(emit(x))) == emit(x)。
    #[test]
    fn emit_fixpoint() {
        let l = ok(TWO_UNSORTED);
        let once = emit_str(&l);
        let twice = emit_str(&ok(&once));
        assert_eq!(once, twice);
    }

    // 7. check_canonical 真值面：最小形/恰满形/多格形。
    #[test]
    fn check_canonical_true_matrix() {
        let d48 = doc_48(0);
        for s in [
            MIN,
            d48.as_str(),
            r#"{"schema":1,"cells":[{"app":"A","page":0,"row":0,"col":0},{"app":"A","page":3,"row":7,"col":7}]}"#,
        ] {
            assert!(check_canonical(s.as_bytes()), "{s}");
        }
    }

    // 8. check_canonical 假值面：乱序/带空白/键序错/空输入。
    #[test]
    fn check_canonical_false_matrix() {
        assert!(!check_canonical(TWO_UNSORTED.as_bytes())); // 乱序
        let ws = MIN.replace("APPDEMO", " APPDEMO"); // 带空白（串内亦拒）
        assert!(!check_canonical(ws.as_bytes()));
        let swapped = r#"{"cells":[{"app":"A","page":0,"row":0,"col":0}],"schema":1}"#;
        assert!(!check_canonical(swapped.as_bytes())); // 键序错
        assert!(!check_canonical(b"")); // 空输入
    }

    // 9. 同 app 多格允许（v1 不禁——两格三格同 id 皆收）。
    #[test]
    fn same_app_multiple_cells_ok() {
        let s = r#"{"schema":1,"cells":[{"app":"A","page":0,"row":0,"col":0},{"app":"A","page":1,"row":0,"col":0},{"app":"A","page":3,"row":7,"col":7}]}"#;
        let l = ok(s);
        assert_eq!(l.len, 3);
        assert!(l.cells().iter().all(|c| c.app == "A"));
    }

    // 10. id 词法边界：24B 恰满/1B/全字符集/首字符 ._- 合法（manifest
    //     首字符字母数字律不照搬——超集面，头注已注）。
    #[test]
    fn id_lexicon_boundaries() {
        let id24 = "a".repeat(ID_MAX);
        for id in [id24.as_str(), "x", "aB3._-xZ9", ".hidden", "-lead", "_under"] {
            let s = doc_json(&[cell_json(id, 0, 0, 0)]);
            assert_eq!(ok(&s).cells()[0].app, id, "{id}");
        }
    }

    // 11. 坐标域顶格：page=3/row=7/col=7 合法。
    #[test]
    fn coords_top_of_domain() {
        let s = doc_json(&[cell_json("A", 3, 7, 7)]);
        let c = ok(&s).cells()[0];
        assert_eq!((c.page, c.row, c.col), (3, 7, 7));
        assert_eq!((PAGE_MAX, ROW_MAX, COL_MAX), (4, 8, 8)); // 排他上界钉
    }

    // 12. 全页分布往返（page 0..3 各有格——含全页分布的 round-trip 面）。
    #[test]
    fn all_pages_roundtrip() {
        let mut cells = Vec::new();
        for p in 0..4u8 {
            cells.push(("app.demo_1", p, p % 8, (p + 3) % 8));
        }
        let l = layout_of(&cells);
        let s = emit_str(&l);
        let back = ok(&s);
        assert_eq!(back.cells(), l.cells());
        assert!(check_canonical(s.as_bytes()));
    }

    // 13. push 条数帽 + Layout 相等只比激活面。
    #[test]
    fn push_cap_and_layout_eq() {
        let mut l = Layout::new();
        for k in 0..CELL_MAX {
            l.push(Cell { app: "a", page: 0, row: (k / 8) as u8, col: (k % 8) as u8 })
                .expect("push ok");
        }
        assert_eq!(
            l.push(Cell { app: "z", page: 3, row: 7, col: 7 }),
            Err(LayoutErr::CellCap { at: 0, count: CELL_MAX + 1 })
        );
        let mut other = layout_of(&[("a", 0, 0, 0)]);
        other.cells[47] = Cell { app: "GARBAGE", page: 3, row: 7, col: 7 }; // 未激活槽
        assert_ne!(l, layout_of(&[("a", 0, 0, 0)]));
        assert_eq!(other, layout_of(&[("a", 0, 0, 0)]));
    }

    // 14. emit 输出窗：恰满收；差一折 NoRoom（need/have 结构化诊断）。
    #[test]
    fn emit_buffer_room() {
        let l = ok(MIN);
        let mut exact = [0u8; 65];
        assert_eq!(emit(&l, &mut exact).unwrap(), 65);
        let mut small = [0u8; 64];
        assert_eq!(
            emit(&l, &mut small),
            Err(LayoutErr::NoRoom { need: 65, have: 64 })
        );
    }

    // 15. worst case 容量诚实注：48 cell × 24B id = 2902B 语义全合法但
    //     编不进单簇帽——emit loud 拒（TooBig{len:2902}），不截断。
    #[test]
    fn emit_worst_case_rejects() {
        let id24 = "a".repeat(ID_MAX);
        let mut cells = Vec::new();
        for r in 0..6u8 {
            for c in 0..8u8 {
                cells.push((id24.as_str(), 0u8, r, c));
            }
        }
        let l = layout_of(&cells);
        assert_eq!(
            emit(&l, &mut [0u8; LAYOUT_BYTES_MAX * 2]),
            Err(LayoutErr::TooBig { len: 2902 }) // 22+36×48+48×24
        );
    }

    // 16. 诊断面契约：全变体 desc ≤24B 纯 ASCII（eco 发射窗）+ at() 全覆盖。
    #[test]
    fn err_desc_window() {
        let all = [
            LayoutErr::TooBig { len: 2049 },
            LayoutErr::CellCap { at: 7, count: 49 },
            LayoutErr::Range { at: 7 },
            LayoutErr::DupCell { at: 7 },
            LayoutErr::UnknownKey { at: 7 },
            LayoutErr::Version { at: 7, found: 2 },
            LayoutErr::Whitespace { at: 7 },
            LayoutErr::NonAscii { at: 7 },
            LayoutErr::BadId { at: 7 },
            LayoutErr::EmptyCells { at: 7 },
            LayoutErr::Syntax { at: 7 },
            LayoutErr::NoRoom { need: 65, have: 64 },
        ];
        for e in &all {
            let d = e.desc();
            assert!(d.len() <= 24 && d.is_ascii(), "{d}");
            assert_eq!(e.at(), if matches!(e, LayoutErr::TooBig { .. } | LayoutErr::NoRoom { .. }) { 0 } else { 7 });
        }
    }

    // 17. 帽值钉板（跨面镜像义务的测试锚：ID_MAX 与 berry-manifest 同值
    //     24——两侧各钉一份 = 本测 + manifest `id_max_pinned` 对偶，任一侧
    //     改帽即红；tools/mkapp.py python 镜像仍注释锚，改帽三处同步）。
    #[test]
    fn cap_anchor_pins() {
        assert_eq!(ID_MAX, 24);
        assert_eq!(LAYOUT_BYTES_MAX, 2048);
        assert_eq!(CELL_MAX, 48);
        assert_eq!((PAGE_MAX, ROW_MAX, COL_MAX), (4, 8, 8));
        assert_eq!(SCHEMA_VERSION, 1);
    }

    // ---- emit 侧负例（手构收口执法——push 只查条数帽，值域/词法/唯一
    //      性/条数帽全在 emit 收口统一裁断；launcher 手构面 = new+push
    //      →emit→写 LAYOUT.JSN，这半边契约的钉板）----

    // E1. 0 cell loud 拒（Layout::new() 直发——空布局非合法落盘形）。
    #[test]
    fn emit_rejects_empty_layout() {
        assert_eq!(emit(&Layout::new(), &mut [0u8; 8]), Err(LayoutErr::EmptyCells { at: 0 }));
    }

    // E2. 三元组重复两形：push 序相邻 + 乱序 push 排序后相邻（排序后
    //     相邻判重路径两形皆钉）。
    #[test]
    fn emit_rejects_dup_triple() {
        let mut adjacent = Layout::new();
        adjacent.push(Cell { app: "A", page: 0, row: 0, col: 0 }).unwrap();
        adjacent.push(Cell { app: "B", page: 0, row: 0, col: 0 }).unwrap();
        assert_eq!(emit(&adjacent, &mut [0u8; 8]), Err(LayoutErr::DupCell { at: 0 }));

        let mut scattered = Layout::new(); // push 序不相邻，排序后相邻
        scattered.push(Cell { app: "A", page: 0, row: 0, col: 0 }).unwrap();
        scattered.push(Cell { app: "B", page: 3, row: 7, col: 7 }).unwrap();
        scattered.push(Cell { app: "C", page: 0, row: 0, col: 0 }).unwrap();
        assert_eq!(emit(&scattered, &mut [0u8; 8]), Err(LayoutErr::DupCell { at: 0 }));
    }

    // E3. 坐标越域三向（page=4/row=8/col=8——emit 值域收口，parse N3-N5
    //     对偶）。
    #[test]
    fn emit_rejects_coord_range() {
        for (page, row, col) in [(4u8, 0u8, 0u8), (0, 8, 0), (0, 0, 8)] {
            let cells = [("A", page, row, col)];
            let l = layout_of(&cells);
            assert_eq!(emit(&l, &mut [0u8; 8]), Err(LayoutErr::Range { at: 0 }), "({page},{row},{col})");
        }
    }

    // E4. id 词法违例（空串/非法字符——emit 词法收口，parse N25/N26 对偶）。
    #[test]
    fn emit_rejects_bad_id() {
        for id in ["", "A/B"] {
            let cells = [(id, 0, 0, 0)];
            let l = layout_of(&cells);
            assert_eq!(emit(&l, &mut [0u8; 8]), Err(LayoutErr::BadId { at: 0 }), "{id}");
        }
    }

    // E5. 条数帽越界（in-crate 直构 len=49——emit 帽查先于切片不 panic；
    //     外部经私有字段从构造上不可达，此测钉防御层本身）。
    #[test]
    fn emit_rejects_cell_cap_over() {
        let l = Layout { cells: [Cell { app: "a", page: 0, row: 0, col: 0 }; CELL_MAX], len: CELL_MAX + 1 };
        assert_eq!(emit(&l, &mut [0u8; 8]), Err(LayoutErr::CellCap { at: 0, count: CELL_MAX + 1 }));
    }

    // ---- 负例全家桶（每条独立测；at 位点逐条钉死——诊断位是产品面）----

    // N1. 文档 2049B > 2048B 单簇帽。
    #[test]
    fn reject_doc_over_2048() {
        let s = doc_48(1);
        assert_eq!(s.len(), LAYOUT_BYTES_MAX + 1);
        assert_eq!(bad(&s), LayoutErr::TooBig { len: 2049 });
    }

    // N2. 49 cells > CELL_MAX（at = 第 49 个 cell 起始：21 + 36 + 47×37）。
    #[test]
    fn reject_49_cells() {
        let mut cs: Vec<String> = (0..6u32)
            .flat_map(|r| (0..8u32).map(move |c| cell_json("a", 0, r, c)))
            .collect();
        cs.push(cell_json("z", 1, 0, 0));
        let at = 21 + 36 + 47 * 37 + 1; // 头 21B + 首 cell 36B + 47×(逗号+36B) + 第 49 cell 前逗号
        assert_eq!(bad(&doc_json(&cs)), LayoutErr::CellCap { at, count: 49 });
    }

    // N3. page=4 越域（值域排他上界；at = page 值 token 起始 = 21+18）。
    #[test]
    fn reject_page_out_of_range() {
        let s = doc_json(&[cell_json("A", 4, 0, 0)]);
        assert_eq!(bad(&s), LayoutErr::Range { at: 39 });
        assert!(matches!(bad(&doc_json(&[cell_json("A", 99, 0, 0)])), LayoutErr::Range { .. }));
    }

    // N4. row=8 越域（at = row 值 token 起始 = N3 page 位 +8）。
    #[test]
    fn reject_row_out_of_range() {
        let s = doc_json(&[cell_json("A", 0, 8, 0)]);
        assert_eq!(bad(&s), LayoutErr::Range { at: 47 });
    }

    // N5. col=8 越域（at = col 值 token 起始 = N4 row 位再 +8）。
    #[test]
    fn reject_col_out_of_range() {
        let s = doc_json(&[cell_json("A", 0, 0, 8)]);
        assert_eq!(bad(&s), LayoutErr::Range { at: 55 });
    }

    // N6. 重复 (page,row,col)（异 app 同坐标亦拒——比的是坐标不是 id；
    //      at = 重复 cell 起始 `{` 位）。
    #[test]
    fn reject_duplicate_triple() {
        let s = doc_json(&[cell_json("A", 0, 0, 0), cell_json("B", 0, 0, 0)]);
        let at = s.rfind('{').unwrap();
        assert_eq!(at, 58); // 头 21 + 首 cell 36 + 逗号 1
        assert_eq!(bad(&s), LayoutErr::DupCell { at });
    }

    // N7. 未知顶层键（替换位：cells → grids）。
    #[test]
    fn reject_unknown_top_key_displaced() {
        let s = MIN.replace("\"cells\"", "\"grids\"");
        assert_eq!(bad(&s), LayoutErr::UnknownKey { at: 12 });
    }

    // N8. 未知顶层键（追加位：三键）。
    #[test]
    fn reject_unknown_top_key_appended() {
        let mut s = MIN.to_string();
        s.pop();
        s.push_str(r#","extra":0}"#);
        assert!(matches!(bad(&s), LayoutErr::UnknownKey { .. }));
    }

    // N9. 未知 cell 键（替换位：app → badge）。
    #[test]
    fn reject_unknown_cell_key_displaced() {
        let s = MIN.replace("\"app\"", "\"badge\"");
        assert!(matches!(bad(&s), LayoutErr::UnknownKey { .. }));
    }

    // N10. 未知 cell 键（追加位：第五键）。
    #[test]
    fn reject_unknown_cell_key_appended() {
        let mut s = MIN.to_string();
        s.truncate(s.len() - 3); // 剥 `}]}`，挂在 col 后 = cell 第五键
        s.push_str(r#","badge":0}]}"#);
        assert!(matches!(bad(&s), LayoutErr::UnknownKey { .. }));
    }

    // N11. schema=0（版本不配——found 结构化诊断）。
    #[test]
    fn reject_schema_zero() {
        let s = MIN.replace("\"schema\":1", "\"schema\":0");
        assert_eq!(bad(&s), LayoutErr::Version { at: 10, found: 0 });
    }

    // N12. schema=2 / 饱和巨值（溢出折 u32::MAX 仍 Version）。
    #[test]
    fn reject_schema_two() {
        let s = MIN.replace("\"schema\":1", "\"schema\":2");
        assert_eq!(bad(&s), LayoutErr::Version { at: 10, found: 2 });
        let s2 = MIN.replace("\"schema\":1", "\"schema\":99999999999");
        assert_eq!(bad(&s2), LayoutErr::Version { at: 10, found: u32::MAX });
    }

    // N13. schema 缺（cells 顶到首位——固定键序语法里错位即不识）。
    #[test]
    fn reject_schema_missing() {
        let s = r#"{"cells":[{"app":"A","page":0,"row":0,"col":0}]}"#;
        assert_eq!(bad(s), LayoutErr::UnknownKey { at: 1 });
    }

    // N14. schema 重复键（位 2 应见 cells——同折 UnknownKey）。
    #[test]
    fn reject_schema_dup_key() {
        let s = r#"{"schema":1,"schema":1,"cells":[{"app":"A","page":0,"row":0,"col":0}]}"#;
        assert!(matches!(bad(s), LayoutErr::UnknownKey { .. }));
    }

    // N15. schema 值是串（类型面——int 位遇 `"` 折 Syntax）。
    #[test]
    fn reject_schema_string_value() {
        let s = MIN.replace("\"schema\":1", "\"schema\":\"1\"");
        assert!(matches!(bad(&s), LayoutErr::Syntax { .. }));
    }

    // N16. cells 空数组（loud 拒——无文件 = 消费方默认布局，不是本 crate
    //      的事；at = `]` 偏移）。
    #[test]
    fn reject_empty_cells_array() {
        assert_eq!(bad(r#"{"schema":1,"cells":[]}"#), LayoutErr::EmptyCells { at: 21 });
    }

    // N17-N20. 空白四字节各一（任何位置即拒——此处钉键串内部位）。
    #[test]
    fn reject_space() {
        let mut s = MIN.to_string();
        s.insert(5, ' ');
        let at = s.find(' ').unwrap();
        assert_eq!(bad(&s), LayoutErr::Whitespace { at });
    }

    #[test]
    fn reject_tab() {
        let mut s = MIN.to_string();
        s.insert(5, '\t');
        let at = s.find('\t').unwrap();
        assert_eq!(bad(&s), LayoutErr::Whitespace { at });
    }

    #[test]
    fn reject_lf() {
        let mut s = MIN.to_string();
        s.insert(5, '\n');
        let at = s.find('\n').unwrap();
        assert_eq!(bad(&s), LayoutErr::Whitespace { at });
    }

    #[test]
    fn reject_cr() {
        let mut s = MIN.to_string();
        s.insert(5, '\r');
        let at = s.find('\r').unwrap();
        assert_eq!(bad(&s), LayoutErr::Whitespace { at });
    }

    // N21. cells 数组尾逗号。
    #[test]
    fn reject_trailing_comma_in_cells() {
        let s = r#"{"schema":1,"cells":[{"app":"A","page":0,"row":0,"col":0},]}"#;
        assert!(matches!(bad(s), LayoutErr::Syntax { .. }));
    }

    // N22. cell 对象尾逗号。
    #[test]
    fn reject_trailing_comma_in_cell() {
        let s = r#"{"schema":1,"cells":[{"app":"A","page":0,"row":0,"col":0,}]}"#;
        assert!(matches!(bad(s), LayoutErr::Syntax { .. }));
    }

    // N23. 转义反斜杠（v1 无转义语义——串内 `\` 即拒）。
    #[test]
    fn reject_escape_backslash() {
        let s = MIN.replace("APPDEMO", r"A\B");
        assert!(matches!(bad(&s), LayoutErr::Syntax { .. }));
    }

    // N24. id 超长（小写 25B > ID_MAX——镜像帽的越界侧；at = 第 25 字节位
    //      = 内容起点 29 + ID_MAX）。
    #[test]
    fn reject_id_over_24() {
        let id25 = "a".repeat(ID_MAX + 1);
        let s = doc_json(&[cell_json(&id25, 0, 0, 0)]);
        assert_eq!(bad(&s), LayoutErr::BadId { at: 29 + ID_MAX });
    }

    // N25. id 非法字符（`/` 可打印但词法外——BadId 不是 NonAscii；at =
    //      `/` 字节位 = 内容起点 + 3）。
    #[test]
    fn reject_id_illegal_char() {
        let s = doc_json(&[cell_json("APP/DEMO", 0, 0, 0)]);
        let at = s.find('/').unwrap();
        assert_eq!(at, 32); // 内容起点 29 + APP 3 字节
        assert_eq!(bad(&s), LayoutErr::BadId { at });
    }

    // N26. id 空串（`""`——词法下界 1B 的越界侧；at = 内容起点 29）。
    #[test]
    fn reject_id_empty() {
        let s = doc_json(&[cell_json("", 0, 0, 0)]);
        assert_eq!(bad(&s), LayoutErr::BadId { at: 29 });
    }

    // N27. 前导零（坐标 01 / schema 01 皆折 Syntax——文法面非值域面）。
    #[test]
    fn reject_leading_zero() {
        let s = doc_json(&[cell_json("A", 0, 0, 0)]).replace("\"page\":0", "\"page\":01");
        assert!(matches!(bad(&s), LayoutErr::Syntax { .. }));
        let s2 = MIN.replace("\"schema\":1", "\"schema\":01");
        assert!(matches!(bad(&s2), LayoutErr::Syntax { .. }));
    }

    // N28. 负数（`-` 非数字——坐标域非负）。
    #[test]
    fn reject_negative_coord() {
        let s = doc_json(&[cell_json("A", 0, 0, 0)]).replace("\"page\":0", "\"page\":-1");
        assert!(matches!(bad(&s), LayoutErr::Syntax { .. }));
    }

    // N29. float（`1.0` 折在 `.` 位）。
    #[test]
    fn reject_float_coord() {
        let s = doc_json(&[cell_json("A", 0, 0, 0)]).replace("\"row\":0", "\"row\":1.0");
        assert!(matches!(bad(&s), LayoutErr::Syntax { .. }));
    }

    // N30. bool（`true` 折在首字节——int 位非数字）。
    #[test]
    fn reject_bool_coord() {
        let s = doc_json(&[cell_json("A", 0, 0, 0)]).replace("\"page\":0", "\"page\":true");
        assert!(matches!(bad(&s), LayoutErr::Syntax { .. }));
    }

    // N31. null。
    #[test]
    fn reject_null_coord() {
        let s = doc_json(&[cell_json("A", 0, 0, 0)]).replace("\"col\":0", "\"col\":null");
        assert!(matches!(bad(&s), LayoutErr::Syntax { .. }));
    }

    // N32. trailing garbage（合法文档后粘字节）。
    #[test]
    fn reject_trailing_garbage() {
        let s = format!("{MIN}x");
        assert_eq!(bad(&s), LayoutErr::Syntax { at: MIN.len() });
    }

    // N33. 非 ASCII 字节（多字节 UTF-8 折 NonAscii——字节域门）。
    #[test]
    fn reject_non_ascii() {
        let s = MIN.replace("APPDEMO", "A\u{e9}B");
        let at = s.find('\u{e9}').unwrap();
        assert_eq!(bad(&s), LayoutErr::NonAscii { at });
    }

    // N34. 字符串未闭合（吞到文档尾无引号——折在起始引号位 = 内容 ABC
    //      前 1 字节）。
    #[test]
    fn reject_unclosed_string() {
        let s = r#"{"schema":1,"cells":[{"app":"ABC"#;
        assert_eq!(bad(s), LayoutErr::Syntax { at: 28 });
    }

    // N35. 文档未闭合（截尾 `}`）。
    #[test]
    fn reject_unclosed_doc() {
        let s = &MIN[..MIN.len() - 1];
        assert!(matches!(bad(s), LayoutErr::Syntax { .. }));
    }

    // N36. 空文档族：空串 / `{}` / 裸 `[`。
    #[test]
    fn reject_empty_doc_family() {
        assert_eq!(bad(""), LayoutErr::Syntax { at: 0 });
        assert_eq!(bad("{}"), LayoutErr::Syntax { at: 1 });
        assert!(matches!(bad("["), LayoutErr::Syntax { .. }));
    }

    // N37. 嵌套超两层（app 值位数组/对象——值位只收 string/int）。
    #[test]
    fn reject_nested_value() {
        let s = MIN.replace("\"APPDEMO\"", "[\"A\"]");
        assert!(matches!(bad(&s), LayoutErr::Syntax { .. }));
        let s2 = MIN.replace("\"APPDEMO\"", "{\"x\":1}");
        assert!(matches!(bad(&s2), LayoutErr::Syntax { .. }));
    }

    // N38. cells 非数组（对象顶替）。
    #[test]
    fn reject_cells_not_array() {
        let s = MIN.replace("\"cells\":[{\"app\":\"APPDEMO\",\"page\":0,\"row\":0,\"col\":0}]", "\"cells\":{\"a\":1}");
        assert!(matches!(bad(&s), LayoutErr::Syntax { .. }));
    }

    // N39. schema 在场、cells 键整体缺席（「版本域先写」半成品——AI
    //      生成面最易产形，N13「schema 缺」的对偶面；两形同折 at=11：
    //      闭合形 = `,` 缺席位 / 截尾形 = uint 收口位）。
    #[test]
    fn reject_schema_only_no_cells() {
        assert_eq!(bad(r#"{"schema":1}"#), LayoutErr::Syntax { at: 11 });
        assert_eq!(bad(r#"{"schema":1"#), LayoutErr::Syntax { at: 11 });
    }
}
