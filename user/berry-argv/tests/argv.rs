//! berry-argv host 单测：argv0 截断律 / 整参或零参截断 / wire 钉板 /
//! 空面边界。语义源 = berry-exec env 块组装段（外迁前行为逐字等价，
//! 先在 berry-exec 侧确认原语义后落测）。

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
