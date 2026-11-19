//! berry-argv host 单测：argv0 截断律 / 整参或零参截断 / wire 钉板 /
//! 空面边界；env v1.1 六族就地扩（空集返 0 / 单项多项 / 整项截断边界
//! / 空项跳过 / 256B 恰满 / 内嵌 NUL 断言）。argv 语义源 = berry-exec
//! 旧内嵌组装段（外迁前行为逐字等价，先在 berry-exec 侧确认原语义后
//! 落测）。

use berry_argv::*;

/// blob 解读辅助：按 NUL 分隔读出全部项（末项后的余零不算）。
fn items(blob: &[u8], plen: usize) -> Vec<&[u8]> {
    blob[..plen]
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .collect()
}

#[test]
fn basic_forms_and_wire() {
    // 单 argv0：blob = name\0，plen = len+1。
    let mut b = [0u8; ARGV_BLOB_SZ];
    let p = argv_blob(&mut b, b"bin/hexyl", &[]);
    assert_eq!(p, 10);
    assert_eq!(&b[..p], b"bin/hexyl\0");
    // argv0 + 单参（hexyl 的 --color=never 形态）。
    let p = argv_blob(&mut b, b"bin/hexyl", &[b"--color=never"]);
    assert_eq!(&b[..p], b"bin/hexyl\0--color=never\0");
    // argv0 + 双参（uu cp/mv 双操作数形态）。
    let p = argv_blob(
        &mut b,
        b"bin/uu_cp",
        &[b"-r", b"src/dir", b"dst/dir"],
    );
    assert_eq!(&b[..p], b"bin/uu_cp\0-r\0src/dir\0dst/dir\0");
    assert_eq!(items(&b, p), vec![b"bin/uu_cp".as_slice(), b"-r".as_slice(), b"src/dir".as_slice(), b"dst/dir".as_slice()]);
}

#[test]
fn argv0_truncation_191() {
    // argv0 截 191：192 blob 的 NUL 位恒保留——argv0 永不缺席。
    let long = [b'x'; 300];
    let mut b = [0u8; ARGV_BLOB_SZ];
    let p = argv_blob(&mut b, &long, &[]);
    assert_eq!(p, ARGV_BLOB_SZ);
    assert_eq!(&b[..191], &[b'x'; 191]);
    assert_eq!(b[191], 0);
    // 恰 191 = 满装（无截断）。
    let fit = [b'y'; 191];
    let p = argv_blob(&mut b, &fit, &[]);
    assert_eq!(p, ARGV_BLOB_SZ);
    assert_eq!(&b[..191], &[b'y'; 191]);
    // 192 = 截 1。
    let over = [b'z'; 192];
    let p = argv_blob(&mut b, &over, &[]);
    assert_eq!(p, ARGV_BLOB_SZ);
    assert_eq!(&b[..191], &[b'z'; 191]);
    assert_eq!(b[191], 0);
    // 截断 argv0 后追加参必不装（argv0 已吃满容量）。
    let p = argv_blob(&mut b, &long, &[b"arg"]);
    assert_eq!(p, ARGV_BLOB_SZ);
    assert_eq!(items(&b, p), vec![&long[..191]]);
}

#[test]
fn whole_arg_or_nothing() {
    let mut b = [0u8; ARGV_BLOB_SZ];
    // 容量账：argv0 "prog\0"=5；剩 187。参 A=100 → 101；参 B=85 → 86；
    // 5+101+86=192 恰满——两参都装。
    let a = [b'a'; 100];
    let bb = [b'b'; 85];
    let p = argv_blob(&mut b, b"prog", &[&a, &bb]);
    assert_eq!(p, 192);
    assert_eq!(items(&b, p).len(), 3);
    // 参 B=86：5+101+87=193 > 192 → B 整参不装（A 在场、B 缺席）。
    let bb2 = [b'c'; 86];
    let p = argv_blob(&mut b, b"prog", &[&a, &bb2]);
    assert_eq!(p, 106);
    assert_eq!(items(&b, p), vec![b"prog".as_slice(), a.as_slice()]);
    // 更长参在中段：中段装不下，其后短参也不再装（单调截断无洞）。
    let big = [b'd'; 190];
    let small = [b'e'; 3];
    let p = argv_blob(&mut b, b"prog", &[&big, &small]);
    assert_eq!(p, 5); // big 装不下 → small 也不装
    assert_eq!(items(&b, p), vec![b"prog".as_slice()]);
    // 恰满边界正例：参长 187 - 1 = 186 → 5+186+1=192 恰装。
    let fit = [b'f'; 186];
    let p = argv_blob(&mut b, b"prog", &[&fit]);
    assert_eq!(p, 192);
    // 单参恰超 1：187 长参不装。
    let over = [b'g'; 187];
    let p = argv_blob(&mut b, b"prog", &[&over]);
    assert_eq!(p, 5);
}

#[test]
fn empty_and_edge_forms() {
    let mut b = [0u8; ARGV_BLOB_SZ];
    // 空 argv0：blob = 单 NUL（纯函数面不拒绝；spawn 面不出现）。
    let p = argv_blob(&mut b, b"", &[]);
    assert_eq!(p, 1);
    assert_eq!(b[0], 0);
    // 空 argv0 + 参：参从位 1 起（argv0 NUL 已占 1）。
    let p = argv_blob(&mut b, b"", &[b"only"]);
    assert_eq!(&b[..p], b"\0only\0");
    // 空参项：len 0 → 只装 NUL 分隔（合法——项面空串）。
    let p = argv_blob(&mut b, b"p", &[b"", b"x"]);
    assert_eq!(&b[..p], b"p\0\0x\0");
    assert_eq!(items(&b, p), vec![b"p".as_slice(), b"x".as_slice()]);
    // 余部不动：out 预填 0xAB，plen 外原样。
    let mut b = [0xABu8; ARGV_BLOB_SZ];
    let p = argv_blob(&mut b, b"prog", &[]);
    assert_eq!(p, 5);
    assert_eq!(&b[p..], &[0xAB; ARGV_BLOB_SZ - 5]);
    // out 短于 192：cap 取小——argv0 截 out.len()-1、参按同律。
    let mut s = [0u8; 8];
    let p = argv_blob(&mut s, b"abcdefgh", &[b"XY"]);
    assert_eq!(p, 8);
    assert_eq!(&s, b"abcdefg\0".as_slice());
}

#[test]
fn real_world_forms() {
    // 实测面：uu_ls 长参形态（台账「实测最长的 uu_ls 长参面也在 192 内」）。
    let mut b = [0u8; ARGV_BLOB_SZ];
    let p = argv_blob(&mut b, b"bin/uu_ls", &[b"--color=always", b"--all", b"-l", b"--human-readable"]);
    assert_eq!(p, 9 + 1 + 14 + 1 + 5 + 1 + 2 + 1 + 16 + 1);
    let got = items(&b, p);
    assert_eq!(got, vec![
        b"bin/uu_ls".as_slice(),
        b"--color=always".as_slice(),
        b"--all".as_slice(),
        b"-l".as_slice(),
        b"--human-readable".as_slice(),
    ]);
    // bsh run 形态（单短参）。
    let p = argv_blob(&mut b, b"bin/hello", &[b"world"]);
    assert_eq!(&b[..p], b"bin/hello\0world\0");
}

// —— env v1.1 六族（消息二载荷组装律；items 辅助按 NUL 分隔解读通用）——

#[test]
fn env_empty_set_returns_zero() {
    // 空集返 0：空导出集零发送的面值（发否由调用方判，本函数纯组装）。
    let mut b = [0xABu8; ENV_BLOB_SZ];
    assert_eq!(env_blob(&mut b, &[]), 0);
    // 余部不动：plen=0 → 整缓冲原样。
    assert_eq!(&b[..], &[0xAB; ENV_BLOB_SZ]);
}

#[test]
fn env_single_and_multi_entries() {
    let mut b = [0u8; ENV_BLOB_SZ];
    // 单项：K=V\0。
    let p = env_blob(&mut b, &[b"PATH=/bin"]);
    assert_eq!(p, 10);
    assert_eq!(&b[..p], b"PATH=/bin\0");
    // 多项续装：序保持（产侧导出集序 = 最后赋值先后）。
    let p = env_blob(&mut b, &[b"A=1", b"BB=22", b"CCC=333"]);
    assert_eq!(&b[..p], b"A=1\0BB=22\0CCC=333\0");
    assert_eq!(
        items(&b, p),
        vec![b"A=1".as_slice(), b"BB=22".as_slice(), b"CCC=333".as_slice()]
    );
    // 值可含任意非 NUL 字节（bsh 全导出面预拼实形）。
    let p = env_blob(&mut b, &[b"__E=1", b"EDITOR=vi -e"]);
    assert_eq!(&b[..p], b"__E=1\0EDITOR=vi -e\0");
}

#[test]
fn env_whole_entry_or_nothing() {
    let mut b = [0u8; ENV_BLOB_SZ];
    // 容量账：项 a=100 占 101；bb=85 占 86；101+86=187 → 两项都装。
    let a = [b'a'; 100];
    let bb = [b'b'; 85];
    let p = env_blob(&mut b, &[&a, &bb]);
    assert_eq!(p, 187);
    assert_eq!(items(&b, p).len(), 2);
    // 恰满正例：bb'=154 → 101+155=256 恰满帽，两项都装。
    let fit = [b'c'; 154];
    assert_eq!(env_blob(&mut b, &[&a, &fit]), 256);
    // 恰超 1：bb''=155 → 101+156=257 > 256 → bb'' 整项不装（a 在场、
    // bb'' 缺席——绝不截半项）。
    let over = [b'd'; 155];
    let p = env_blob(&mut b, &[&a, &over]);
    assert_eq!(p, 101);
    assert_eq!(items(&b, p), vec![a.as_slice()]);
    // 中段装不下，其后项不再装（单调截断无洞——空项也救不回）。
    let big = [b'e'; 256];
    let p = env_blob(&mut b, &[&big, b"", b"K=V"]);
    assert_eq!(p, 0);
    assert!(items(&b, p).is_empty());
}

#[test]
fn env_empty_entries_skipped() {
    let mut b = [0u8; ENV_BLOB_SZ];
    // 头部空项：跳过不装（若装即裸 NUL）。
    let p = env_blob(&mut b, &[b"", b"K=V"]);
    assert_eq!(&b[..p], b"K=V\0");
    // 中段空项：两侧项直接相邻。
    let p = env_blob(&mut b, &[b"A=1", b"", b"B=2"]);
    assert_eq!(&b[..p], b"A=1\0B=2\0");
    // 全空项集：返 0（等价空集）。
    assert_eq!(env_blob(&mut b, &[b"", b"", b""]), 0);
    // 输出面无双 NUL 字面（编码面禁 \0\0 歧义——解析面零歧义钉板）。
    let p = env_blob(&mut b, &[b"", b"X=1", b"", b"Y=2", b""]);
    assert_eq!(&b[..p], b"X=1\0Y=2\0");
    assert!(!b[..p].windows(2).any(|w| w == [0, 0]));
}

#[test]
fn env_exact_256_fill() {
    let mut b = [0u8; ENV_BLOB_SZ];
    // 单项 255B + NUL = 256 恰满帽（帽 = 通道消息帽，恰满可发）。
    let e = [b'k'; 255];
    let p = env_blob(&mut b, &[&e]);
    assert_eq!(p, ENV_BLOB_SZ);
    assert_eq!(&b[..255], &e);
    assert_eq!(b[255], 0);
    // 单项恰超 1：256B 项 + NUL = 257 → 整项不装、返 0。
    let over = [b'm'; 256];
    assert_eq!(env_blob(&mut b, &[&over]), 0);
    // 多项恰满：101 + 99 + 56 = 256。
    let x = [b'a'; 100];
    let y = [b'b'; 98];
    let z = [b'c'; 55];
    assert_eq!(env_blob(&mut b, &[&x, &y, &z]), 256);
    // out 短于 256：cap 取小——项按同律对短缓冲截断。
    let mut s = [0u8; 8];
    let p = env_blob(&mut s, &[b"A=1", b"B=2", b"C=3"]);
    assert_eq!(&s[..p], b"A=1\0B=2\0");
}

#[test]
#[should_panic(expected = "env_blob")]
fn env_embedded_nul_intercepted() {
    // 内嵌 NUL 项 = 产侧构造缺陷（KEY=VALUE 预拼面不含 NUL）——debug
    // 构建拦（host cargo test 默认 debug）；release 编译出为防御面。
    let mut b = [0u8; ENV_BLOB_SZ];
    let _ = env_blob(&mut b, &[b"K=V", b"BA\0D=2"]);
}
