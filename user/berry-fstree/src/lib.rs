//! berry-fstree —— fsmgr RAM overlay 目录树纯函数库（C-0 刀 2，
//! `docs/VM里程碑设计草案.md` §C-0；`berry-fsproto`（协议/帽常量单源）/
//! `berry-fatfs`（纯函数先例）之后的「存储结构面」纯函数 crate：v1 平面表
//! `Vec<(名, 内容)>` 升 **NUL 分段路径树**）。
//!
//! 职责边界：本库只管 **RAM 层** 的树存储与路径解析——单段名（v1 形）与
//! 多段 NUL 路径（v2 形）同一套走；FAT12 只读/staging 层、层间路由
//! （`name_to_fat11` 门——8.3 大写名单段走 FAT，多段 NUL 串构造性落 RAM）
//! 与通道/等待语义全在消费方 fsmgr（`user/bins/fsmgr`）。路径合法性由
//! berry-fsproto `parse_request` 上游保证（空路径/空段/段超帽在协议面已
//! 拒）——本库不重检，非法输入按「段找不到」处理（不可达即 NOENT 族）。
//!
//! 语义不变量（**v1 零漂移根基**——单段名逐条对齐 fsmgr v1 的 do_put/
//! do_trunc/handle_get/render_list）：
//! - PUT：命中文件 → off 起写（off 超现长 = 零填充扩容，不缩容）；未命中
//!   → 建文件（父目录须在——**PUT 不隐式建目录**；文件数帽内）；命中
//!   目录 → ST_BAD。尺寸帽先检（end > FILE_SIZE_MAX → ST_FULL）。
//! - TRUNC：create-or-resize（size=0 兼 create——`echo > f` 建文件面）；
//!   建/拒面同 PUT。
//! - GET：off ≥ len → 空片 more=0（文件尾）；否则 ≤CHUNK 窗 + more。
//! - LIST（[`Tree::list_root`]）：根名**空白连接**、LIST_MAX 容纳截断
//!   ——目录也列（现网 smoke 流无目录 = 面上不可见；POSIX ls 语义）。
//! - v2 新面：MKDIR（父须在；目标已在——文件或目录同拒 → ST_EXISTS；
//!   目录数帽 ST_FULL）；UNLINK（文件/空目录 OK，非空目录 → ST_NOEMPTY）；
//!   READDIR（子名 **NUL 连接**、LIST_MAX 同帽容纳截断）；STAT
//!   （file=(TYPE_FILE, 字节数) / dir=(TYPE_DIR, 子项数)）。
//! - **中段解析撞文件 = 路径不可达**，与段缺失同报 ST_NOENT（POSIX 的
//!   ENOENT/ENOTDIR 之分 v2 不展开——状态码预算内「查无此路径」一义）。
//! - 类型冲突（文件 op 打目录 / READDIR 打文件）报 ST_BAD（协议头 v2 注）。
//! - 帽检序：尺寸/数量帽先于路径解析报（v1 do_put 同序先例——双真时
//!   ST_FULL 先报，诊断面与 v1 一致）。
//!
//! 零 unsafe；分配只在数据结构本体（树节点/内容 Vec）——路径输入只借用。
//! host 单测全盖（「AI 测试友好化」：树语义不依赖 QEMU 就能验证）。

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::vec; // 单路径形态：同时引模块与 vec! 宏（{self} 只进类型 ns）
use alloc::vec::Vec;

use berry_fsproto::{
    DIR_COUNT_MAX, FILE_COUNT_MAX, FILE_SIZE_MAX, LIST_MAX, ST_BAD, ST_EXISTS, ST_FULL,
    ST_NOEMPTY, ST_NOENT, ST_OK, TYPE_DIR, TYPE_FILE, CHUNK,
};

/// 树节点内容二相。
#[derive(Debug)]
pub enum Kind {
    /// 文件：内容字节（≤ FILE_SIZE_MAX）。
    File(Vec<u8>),
    /// 目录：子节点（**插入序**——list_root/readdir 的列举序契约）。
    Dir(Vec<Node>),
}

/// 树节点：名 + 内容。
#[derive(Debug)]
pub struct Node {
    pub name: Vec<u8>,
    pub kind: Kind,
}

/// RAM overlay 树（根无名——根即「/」，list_root 的列举主体）。
#[derive(Debug)]
pub struct Tree {
    root: Vec<Node>,
}

impl Tree {
    pub fn new() -> Self {
        Tree { root: Vec::new() }
    }

    /// 分片写（v1 do_put 语义不变量 + 多段路径面）：命中文件写 / 命中目录
    /// ST_BAD / 未命中建（父目录须在 + 文件数帽内）。
    pub fn put(&mut self, path: &[u8], off: u32, data: &[u8]) -> u8 {
        let off = off as usize;
        let end = off + data.len(); // u32 + ≤CHUNK：64 位下无溢出面
        if end > FILE_SIZE_MAX {
            return ST_FULL;
        }
        if let Some(node) = self.resolve_mut(path) {
            return match &mut node.kind {
                Kind::File(c) => {
                    if end > c.len() {
                        c.resize(end, 0); // 零填充扩容（PUT 语义）
                    }
                    c[off..end].copy_from_slice(data);
                    ST_OK
                }
                Kind::Dir(_) => ST_BAD,
            };
        }
        // 未命中建：帽检在父解析前（do_put 尺寸帽同序——双真时 ST_FULL 先报）。
        if self.count_files() >= FILE_COUNT_MAX {
            return ST_FULL;
        }
        let (parent, last) = split_last(path);
        let children = match self.children_mut(parent) {
            Some(c) => c,
            None => return ST_NOENT,
        };
        let mut c = vec![0u8; end];
        c[off..end].copy_from_slice(data);
        children.push(Node { name: last.to_vec(), kind: Kind::File(c) });
        ST_OK
    }

    /// 定长（v1 do_trunc 语义不变量 + 多段路径面）：create-or-resize，
    /// size=0 兼 create。
    pub fn trunc(&mut self, path: &[u8], size: u32) -> u8 {
        let size = size as usize;
        if size > FILE_SIZE_MAX {
            return ST_FULL;
        }
        if let Some(node) = self.resolve_mut(path) {
            return match &mut node.kind {
                Kind::File(c) => {
                    c.resize(size, 0); // 缩容丢尾 / 扩容零填（resize 语义）
                    ST_OK
                }
                Kind::Dir(_) => ST_BAD,
            };
        }
        if self.count_files() >= FILE_COUNT_MAX {
            return ST_FULL;
        }
        let (parent, last) = split_last(path);
        let children = match self.children_mut(parent) {
            Some(c) => c,
            None => return ST_NOENT,
        };
        children.push(Node { name: last.to_vec(), kind: Kind::File(vec![0u8; size]) });
        ST_OK
    }

    /// 分片读（v1 handle_get 语义不变量）。Miss = 未命中（fsmgr 走 FAT
    /// 回落——本库不知 FAT 存在）；IsDir = 类型冲突面（调用方打 ST_BAD）。
    pub fn read(&self, path: &[u8], off: u32) -> ReadRes<'_> {
        let node = match self.resolve(path) {
            Some(n) => n,
            None => return ReadRes::Miss,
        };
        let c = match &node.kind {
            Kind::File(c) => c,
            Kind::Dir(_) => return ReadRes::IsDir,
        };
        let off = off as usize;
        if off >= c.len() {
            return ReadRes::Win(&c[c.len()..], false); // 空片收尾
        }
        let end = (off + CHUNK).min(c.len());
        ReadRes::Win(&c[off..end], end < c.len())
    }

    /// 建目录：父目录须在；目标已在（文件/目录同拒）ST_EXISTS；目录数帽
    /// （全局 Dir 节点计数）ST_FULL。
    pub fn mkdir(&mut self, path: &[u8]) -> u8 {
        if self.count_dirs() >= DIR_COUNT_MAX {
            return ST_FULL;
        }
        let (parent, last) = split_last(path);
        let children = match self.children_mut(parent) {
            Some(c) => c,
            None => return ST_NOENT,
        };
        if children.iter().any(|n| n.name.as_slice() == last) {
            return ST_EXISTS;
        }
        children.push(Node { name: last.to_vec(), kind: Kind::Dir(Vec::new()) });
        ST_OK
    }

    /// 删文件/空目录：非空目录 ST_NOEMPTY；目标/父缺失 ST_NOENT。
    /// （FAT 名不走此面——UNLINK 不回落 FAT，删除盘上文件 = v2 外。）
    pub fn unlink(&mut self, path: &[u8]) -> u8 {
        let (parent, last) = split_last(path);
        let children = match self.children_mut(parent) {
            Some(c) => c,
            None => return ST_NOENT,
        };
        let idx = match children.iter().position(|n| n.name.as_slice() == last) {
            Some(i) => i,
            None => return ST_NOENT,
        };
        if let Kind::Dir(c) = &children[idx].kind {
            if !c.is_empty() {
                return ST_NOEMPTY;
            }
        }
        children.remove(idx); // 文件槽同此释放（cap 复用面见 file_cap 测）
        ST_OK
    }

    /// 目录项列（NUL 连接——READDIR 应答 blob；LIST_MAX 容纳截断，截断 =
    /// 后续项不列，调用方可见）。Noent = 路径不可达；NotDir = 目标是文件。
    pub fn readdir(&self, path: &[u8], buf: &mut [u8]) -> ReadDirRes {
        let node = match self.resolve(path) {
            Some(n) => n,
            None => return ReadDirRes::Noent,
        };
        let children = match &node.kind {
            Kind::Dir(c) => c,
            Kind::File(_) => return ReadDirRes::NotDir,
        };
        ReadDirRes::Names(render(children, buf, 0))
    }

    /// 类型 + 尺寸：file = (TYPE_FILE, 字节数)；dir = (TYPE_DIR, 子项数)。
    pub fn stat(&self, path: &[u8]) -> StatRes {
        match self.resolve(path) {
            Some(n) => match &n.kind {
                Kind::File(c) => StatRes::Ok(TYPE_FILE, c.len() as u32),
                Kind::Dir(c) => StatRes::Ok(TYPE_DIR, c.len() as u32),
            },
            None => StatRes::Noent,
        }
    }

    /// v1 LIST 面（零漂移关键）：根名**空白连接**、LIST_MAX 容纳截断——
    /// 与 v1 render_list 逐字节同形（目录也列，插入序）。
    pub fn list_root(&self, buf: &mut [u8]) -> usize {
        render(&self.root, buf, b' ')
    }

    /// 根名 **NUL 连接**（C-1 刀 4 READDIR 根面）：与 [`list_root`] 同序同帽
    /// 仅分隔符不同——fsmgr READDIR 空路径 = overlay 全景（RAM 根 + FAT 未
    /// 影子名续列），libc `opendir("/")` 的目录 blob 供应源。
    pub fn list_root_nul(&self, buf: &mut [u8]) -> usize {
        render(&self.root, buf, 0)
    }

    /// 根名借用表（FAT 影子去重用——文件与目录都算：RAM 任意根名（含
    /// 8.3 形目录名）影子 FAT 同名 display 项，overlay 上层遮下层的通则）。
    pub fn root_names(&self) -> Vec<&[u8]> {
        self.root.iter().map(|n| n.name.as_slice()).collect()
    }

    /// 全路径解析（只读）：任一段缺失 / 中段撞文件 → None。
    fn resolve(&self, path: &[u8]) -> Option<&Node> {
        let mut node: Option<&Node> = None;
        for seg in path.split(|&b| b == 0) {
            let kids = match node {
                None => &self.root,
                Some(n) => match &n.kind {
                    Kind::Dir(c) => c,
                    Kind::File(_) => return None, // 中段撞文件：不可达
                },
            };
            node = kids.iter().find(|n| n.name.as_slice() == seg);
            if node.is_none() {
                return None;
            }
        }
        node
    }

    /// 全路径解析（可变）：父 walk（children_mut 重借链——跨迭代引用
    /// 循环 NLL 不认，索引面拆两步才编得过）+ 末段定位。
    fn resolve_mut(&mut self, path: &[u8]) -> Option<&mut Node> {
        let (parent, last) = split_last(path);
        let children = self.children_mut(parent)?;
        children.iter_mut().find(|n| n.name.as_slice() == last)
    }

    /// 父目录 children 可变引用（parent = path 去末段；空 parent = 根）。
    /// 中段缺失/撞文件 → None。
    fn children_mut(&mut self, parent: &[u8]) -> Option<&mut Vec<Node>> {
        if parent.is_empty() {
            return Some(&mut self.root);
        }
        let mut children: &mut Vec<Node> = &mut self.root;
        for seg in parent.split(|&b| b == 0) {
            let node = children.iter_mut().find(|n| n.name.as_slice() == seg)?;
            children = match &mut node.kind {
                Kind::Dir(c) => c,
                Kind::File(_) => return None,
            };
        }
        Some(children)
    }

    /// 全局文件节点计数（帽执法面——扫描实现：树 ≤ 24 节点，免维护计数）。
    fn count_files(&self) -> usize {
        count_kind(&self.root, false)
    }

    /// 全局目录节点计数。
    fn count_dirs(&self) -> usize {
        count_kind(&self.root, true)
    }
}

impl Default for Tree {
    fn default() -> Self {
        Self::new()
    }
}

fn count_kind(nodes: &[Node], dir: bool) -> usize {
    nodes
        .iter()
        .map(|n| match &n.kind {
            Kind::File(_) if dir => 0,
            Kind::Dir(c) if dir => 1 + count_kind(c, dir),
            Kind::File(_) => 1,
            Kind::Dir(c) => count_kind(c, dir),
        })
        .sum()
}

/// [`Tree::read`] 结果。
#[derive(Debug)]
pub enum ReadRes<'a> {
    /// 命中：≤CHUNK 窗 + more（false = 尾片/空片）。
    Win(&'a [u8], bool),
    /// 未命中（调用方回落 FAT 面）。
    Miss,
    /// 目标是目录（调用方打 ST_BAD）。
    IsDir,
}

/// [`Tree::readdir`] 结果。
#[derive(Debug)]
pub enum ReadDirRes {
    /// 命中：写入 buf 的字节数（NUL 连接子名，可为 0 = 空目录）。
    Names(usize),
    Noent,
    NotDir,
}

/// [`Tree::stat`] 结果。
#[derive(Debug, PartialEq)]
pub enum StatRes {
    /// (TYPE_FILE/TYPE_DIR, size)：file = 字节数；dir = 子项数。
    Ok(u8, u32),
    Noent,
}

/// 名列渲染（sep 连接，n0 起写；LIST_MAX 与 buf.len 取小为帽——容纳截断，
/// 截断 = 后续项不列，调用方可见）。
fn render(nodes: &[Node], buf: &mut [u8], sep: u8) -> usize {
    let cap = LIST_MAX.min(buf.len());
    let mut n = 0usize;
    for node in nodes {
        let need = node.name.len() + if n > 0 { 1 } else { 0 };
        if n + need > cap {
            break;
        }
        if n > 0 {
            buf[n] = sep;
            n += 1;
        }
        buf[n..n + node.name.len()].copy_from_slice(&node.name);
        n += node.name.len();
    }
    n
}

/// 拆末段：(父路径, 末段名)。path 非空且无尾随 NUL（协议面 path_valid
/// 保证——rposition 无命中 = 单段路径，父为空 = 根）。
fn split_last(path: &[u8]) -> (&[u8], &[u8]) {
    match path.iter().rposition(|&b| b == 0) {
        Some(i) => (&path[..i], &path[i + 1..]),
        None => (&[], path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(r: ReadRes) -> (Vec<u8>, bool) {
        match r {
            ReadRes::Win(s, m) => (s.to_vec(), m),
            _ => panic!("非 Win：{r:?}"),
        }
    }

    #[test]
    fn put_get_v1_semantics() {
        // v1 do_put/handle_get 语义不变量：命中写 / 零填充扩容 / 窗 + more。
        let mut t = Tree::new();
        assert_eq!(t.put(b"a", 0, b"hello"), ST_OK);
        // 覆写前缀（不缩容）。
        assert_eq!(t.put(b"a", 0, b"J"), ST_OK);
        // off 超现长 = 零填充。
        assert_eq!(t.put(b"a", 8, b"XY"), ST_OK);
        let (s, more) = win(t.read(b"a", 0));
        assert_eq!(s, b"Jello\0\0\0XY");
        assert!(!more);
        // 文件尾 off == len：空片 + more=0。
        let (s, more) = win(t.read(b"a", 10));
        assert!(s.is_empty());
        assert!(!more);
    }

    #[test]
    fn get_multi_chunk_more_chain() {
        // 400B 文件 → 192+192+16 三窗，more 链由调用方 off 前进收敛。
        let mut t = Tree::new();
        let mut body = vec![0u8; 400];
        for (i, b) in body.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        // 分片 PUT（协议面 ≤CHUNK/片——这里直接两次 200B 模拟等效终态）。
        assert_eq!(t.put(b"f", 0, &body[..192]), ST_OK);
        assert_eq!(t.put(b"f", 192, &body[192..]), ST_OK);
        let (s, m) = win(t.read(b"f", 0));
        assert_eq!(s.len(), CHUNK);
        assert!(m);
        let (s, m) = win(t.read(b"f", 192));
        assert_eq!(s.len(), CHUNK);
        assert!(m);
        let (s, m) = win(t.read(b"f", 384));
        assert_eq!(&s[..], &body[384..]);
        assert!(!m);
    }

    #[test]
    fn put_parent_missing_no_implicit_mkdir() {
        // PUT 不隐式建目录：父缺 → ST_NOENT；建后 OK。
        let mut t = Tree::new();
        assert_eq!(t.put(b"d\0f", 0, b"x"), ST_NOENT);
        assert_eq!(t.mkdir(b"d"), ST_OK);
        assert_eq!(t.put(b"d\0f", 0, b"x"), ST_OK);
        let (s, _) = win(t.read(b"d\0f", 0));
        assert_eq!(s, b"x");
    }

    #[test]
    fn mid_segment_file_unreachable() {
        // 中段撞文件 = 路径不可达（与段缺失同报 NOENT 族）。
        let mut t = Tree::new();
        assert_eq!(t.trunc(b"f", 4), ST_OK); // 根下建文件 f
        assert_eq!(t.put(b"f\0x", 0, b"y"), ST_NOENT);
        assert_eq!(t.mkdir(b"f\0x"), ST_NOENT);
        assert_eq!(t.unlink(b"f\0x"), ST_NOENT);
        assert_eq!(t.stat(b"f\0x"), StatRes::Noent);
        assert!(matches!(t.read(b"f\0x", 0), ReadRes::Miss));
    }

    #[test]
    fn file_ops_on_dir_reject() {
        // 类型冲突：文件 op 打目录 / 读目录 → ST_BAD 族（IsDir/NotDir 面）。
        let mut t = Tree::new();
        assert_eq!(t.mkdir(b"d"), ST_OK);
        assert_eq!(t.put(b"d", 0, b"x"), ST_BAD);
        assert_eq!(t.trunc(b"d", 4), ST_BAD);
        assert!(matches!(t.read(b"d", 0), ReadRes::IsDir));
    }

    #[test]
    fn trunc_v1_semantics() {
        // create-or-resize：size=0 建 / 扩容零填 / 缩容丢尾。
        let mut t = Tree::new();
        assert_eq!(t.trunc(b"a", 0), ST_OK);
        let (s, _) = win(t.read(b"a", 0));
        assert!(s.is_empty());
        assert_eq!(t.trunc(b"a", 8), ST_OK);
        let (s, _) = win(t.read(b"a", 0));
        assert_eq!(s, vec![0u8; 8]);
        assert_eq!(t.put(b"a", 0, b"12345678"), ST_OK);
        assert_eq!(t.trunc(b"a", 3), ST_OK);
        let (s, _) = win(t.read(b"a", 0));
        assert_eq!(s, b"123");
        // 未命中建（父须在）。
        assert_eq!(t.trunc(b"nope\0b", 4), ST_NOENT);
    }

    #[test]
    fn mkdir_exists_faces() {
        let mut t = Tree::new();
        assert_eq!(t.mkdir(b"d"), ST_OK);
        assert_eq!(t.mkdir(b"d"), ST_EXISTS);
        // 目标是已存在文件：同拒 ST_EXISTS（EEXIST 不分型）。
        assert_eq!(t.trunc(b"f", 0), ST_OK);
        assert_eq!(t.mkdir(b"f"), ST_EXISTS);
        // 嵌套。
        assert_eq!(t.mkdir(b"d\0e"), ST_OK);
        assert_eq!(t.mkdir(b"d\0e"), ST_EXISTS);
        assert_eq!(t.mkdir(b"x\0e"), ST_NOENT); // 父缺
    }

    #[test]
    fn dir_cap() {
        // DIR_COUNT_MAX=16 全局计数（含嵌套）；17th → ST_FULL。
        let mut t = Tree::new();
        for i in 0..16 {
            let name = alloc::format!("d{i:02}");
            assert_eq!(t.mkdir(name.as_bytes()), ST_OK);
        }
        assert_eq!(t.mkdir(b"extra"), ST_FULL);
        // 释放一个槽后复用。
        assert_eq!(t.unlink(b"d00"), ST_OK);
        assert_eq!(t.mkdir(b"extra"), ST_OK);
    }

    #[test]
    fn file_cap() {
        // FILE_COUNT_MAX=8 全局计数；9th → ST_FULL；unlink 释放槽复用。
        let mut t = Tree::new();
        for i in 0..8 {
            let name = alloc::format!("f{i}");
            assert_eq!(t.put(name.as_bytes(), 0, b"x"), ST_OK);
        }
        assert_eq!(t.put(b"f8", 0, b"x"), ST_FULL);
        assert_eq!(t.trunc(b"f8", 0), ST_FULL);
        assert_eq!(t.unlink(b"f0"), ST_OK);
        assert_eq!(t.put(b"f8", 0, b"x"), ST_OK);
    }

    #[test]
    fn size_cap() {
        let mut t = Tree::new();
        assert_eq!(t.put(b"a", FILE_SIZE_MAX as u32 - 1, b"xy"), ST_FULL); // end 超帽
        assert_eq!(t.put(b"a", FILE_SIZE_MAX as u32 - 1, b"x"), ST_OK); // 恰帽内
        assert_eq!(t.trunc(b"a", FILE_SIZE_MAX as u32 + 1), ST_FULL);
    }

    #[test]
    fn unlink_faces() {
        let mut t = Tree::new();
        assert_eq!(t.put(b"a", 0, b"x"), ST_OK);
        assert_eq!(t.unlink(b"a"), ST_OK);
        assert!(matches!(t.read(b"a", 0), ReadRes::Miss));
        assert_eq!(t.unlink(b"a"), ST_NOENT);
        // 非空目录拒、清空后放行。
        assert_eq!(t.mkdir(b"d"), ST_OK);
        assert_eq!(t.put(b"d\0c", 0, b"x"), ST_OK);
        assert_eq!(t.unlink(b"d"), ST_NOEMPTY);
        assert_eq!(t.unlink(b"d\0c"), ST_OK);
        assert_eq!(t.unlink(b"d"), ST_OK);
    }

    #[test]
    fn readdir_nul_joined() {
        let mut t = Tree::new();
        assert_eq!(t.mkdir(b"d"), ST_OK);
        assert_eq!(t.put(b"d\0b", 0, b"1"), ST_OK);
        assert_eq!(t.put(b"d\0a", 0, b"1"), ST_OK);
        assert_eq!(t.mkdir(b"d\0c"), ST_OK);
        let mut buf = [0u8; 256];
        // 插入序：b、a、c。
        match t.readdir(b"d", &mut buf) {
            ReadDirRes::Names(n) => assert_eq!(&buf[..n], b"b\0a\0c"),
            r => panic!("非 Names：{r:?}"),
        }
        // 空目录 / 缺失 / 打文件。
        match t.readdir(b"d\0c", &mut buf) {
            ReadDirRes::Names(n) => assert_eq!(n, 0),
            r => panic!("非 Names：{r:?}"),
        }
        assert!(matches!(t.readdir(b"nope", &mut buf), ReadDirRes::Noent));
        assert!(matches!(t.readdir(b"d\0a", &mut buf), ReadDirRes::NotDir));
    }

    #[test]
    fn readdir_list_max_truncate() {
        // 名列帽：10 × 31B 名（NUL 连接 320B）> LIST_MAX → 容纳截断。
        let mut t = Tree::new();
        assert_eq!(t.mkdir(b"d"), ST_OK);
        let long = [b'm'; 31];
        for i in 0..10u8 {
            let mut name = long.clone();
            name[0] = b'0' + i;
            let mut path = vec![0u8; 0];
            path.extend_from_slice(b"d\0");
            path.extend_from_slice(&name);
            assert_eq!(t.mkdir(&path), ST_OK);
        }
        let mut buf = [0u8; 256];
        match t.readdir(b"d", &mut buf) {
            ReadDirRes::Names(n) => {
                assert!(n <= LIST_MAX);
                // 截断可见：只列得下前 6 项（6×32-1=191 ≤ 200 < 7×32-1=223）。
                let segs = buf[..n].split(|&b| b == 0).count();
                assert_eq!(segs, 6);
            }
            r => panic!("非 Names：{r:?}"),
        }
    }

    #[test]
    fn stat_faces() {
        let mut t = Tree::new();
        assert_eq!(t.put(b"a", 0, b"12345"), ST_OK);
        assert_eq!(t.mkdir(b"d"), ST_OK);
        assert_eq!(t.put(b"d\0x", 0, b"1"), ST_OK);
        assert_eq!(t.mkdir(b"d\0e"), ST_OK);
        assert_eq!(t.stat(b"a"), StatRes::Ok(TYPE_FILE, 5));
        assert_eq!(t.stat(b"d"), StatRes::Ok(TYPE_DIR, 2)); // 子项数
        assert_eq!(t.stat(b"d\0x"), StatRes::Ok(TYPE_FILE, 1));
        assert_eq!(t.stat(b"d\0nope"), StatRes::Noent);
    }

    #[test]
    fn list_root_v1_shape() {
        // v1 render_list 逐字节同形：空白连接、插入序、无尾随空白。
        let mut t = Tree::new();
        assert_eq!(t.put(b"a", 0, b"1"), ST_OK);
        assert_eq!(t.put(b"b", 0, b"1"), ST_OK);
        assert_eq!(t.mkdir(b"d"), ST_OK);
        assert_eq!(t.put(b"c", 0, b"1"), ST_OK);
        let mut buf = [0u8; 256];
        let n = t.list_root(&mut buf);
        assert_eq!(&buf[..n], b"a b d c");
        // 容纳截断：5 × 48B 名（空白连接 244B）> 200 → 帽内尽量多列。
        let mut t2 = Tree::new();
        let long = [b'n'; 48];
        for i in 0..5u8 {
            let mut name = long.clone();
            name[0] = b'0' + i;
            assert_eq!(t2.put(&name, 0, b"1"), ST_OK);
        }
        let mut buf2 = [0u8; 256];
        let n2 = t2.list_root(&mut buf2);
        assert!(n2 <= LIST_MAX);
        assert_eq!(n2, 4 * 49 - 1); // 4 项恰容（4×48+3=195 ≤ 200 < 5×48+4）
    }

    #[test]
    fn list_root_nul_shape() {
        // C-1 刀 4：READDIR 根 blob = 与 list_root 同序同帽、NUL 连接。
        let mut t = Tree::new();
        assert_eq!(t.put(b"a", 0, b"1"), ST_OK);
        assert_eq!(t.put(b"b", 0, b"1"), ST_OK);
        assert_eq!(t.mkdir(b"d"), ST_OK);
        let mut buf = [0u8; 256];
        let n = t.list_root_nul(&mut buf);
        assert_eq!(&buf[..n], b"a\x00b\x00d");
        // 空根 = 空 blob（opendir("/") 于空系统 → 空列）。
        let mut buf2 = [0u8; 256];
        assert_eq!(Tree::new().list_root_nul(&mut buf2), 0);
    }

    #[test]
    fn deep_path_depth8() {
        // PATH_DEPTH_MAX=8：7 层目录 + 第 8 层文件。
        let mut t = Tree::new();
        let mut path: Vec<u8> = Vec::new();
        for i in 0..7u8 {
            if !path.is_empty() {
                path.push(0);
            }
            path.push(b'a' + i);
            assert_eq!(t.mkdir(&path), ST_OK);
        }
        let mut fpath = path.clone();
        fpath.push(0);
        fpath.push(b'h'); // a\0b\0c\0d\0e\0f\0g\0h：深 8
        assert_eq!(t.put(&fpath, 0, b"deep"), ST_OK);
        let (s, _) = win(t.read(&fpath, 0));
        assert_eq!(s, b"deep");
        assert_eq!(t.stat(&fpath), StatRes::Ok(TYPE_FILE, 4));
        assert_eq!(t.stat(&path), StatRes::Ok(TYPE_DIR, 1));
        // 深穿删除：叶子文件掉后目录链自深至浅逐层可清（空目录才放行）。
        assert_eq!(t.unlink(&fpath), ST_OK);
        for d in (0..7).rev() {
            let mut q: Vec<u8> = Vec::new();
            for j in 0..=d as u8 {
                if !q.is_empty() {
                    q.push(0);
                }
                q.push(b'a' + j);
            }
            assert_eq!(t.unlink(&q), ST_OK, "深 {:?} 应可删", q);
        }
    }

    #[test]
    fn root_names_shadow_shape() {
        // FAT 影子去重的数据面：文件与目录都入表（含 8.3 形目录名——
        // overlay 上层遮下层通则，LIST 去重由 append_names 消费方执法）。
        let mut t = Tree::new();
        assert_eq!(t.put(b"a", 0, b"1"), ST_OK);
        assert_eq!(t.mkdir(b"SUB"), ST_OK);
        assert_eq!(t.put(b"c.txt", 0, b"1"), ST_OK); // 小写：RAM 面
        let names = t.root_names();
        assert_eq!(names, vec![b"a".as_slice(), b"SUB".as_slice(), b"c.txt".as_slice()]);
    }
}
