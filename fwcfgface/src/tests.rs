//! host 钉板（W3-Q86）——目录行走/推进律/域门全值域表驱动 + 敌意输入
//! + SeqState 推进模型对拍（Σplan == size 项尾截断恰等不变量）。

use crate::*;

/// 目录项编码器（host 侧构造向量——与 QEMU 布局逐字段对齐：BE count/
/// size/select·rsv + 56B 名域项内字节序）。
fn entry(select: u16, size: u32, name: &str) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&size.to_be_bytes());
    let sel_rsv = ((select as u32) << 16) | 0;
    v.extend_from_slice(&sel_rsv.to_be_bytes());
    let mut nbuf = [0u8; NAME_LEN];
    assert!(name.len() < NAME_LEN, "fixture 名须 < NAME_LEN");
    nbuf[..name.len()].copy_from_slice(name.as_bytes());
    v.extend_from_slice(&nbuf);
    v
}

/// 目录包装（count + N 项）。
fn dir(entries: &[Vec<u8>]) -> Vec<u8> {
    let mut v = (entries.len() as u32).to_be_bytes().to_vec();
    for e in entries {
        v.extend_from_slice(e);
    }
    v
}

#[test]
fn find_hit_miss_and_order() {
    let d = dir(&[entry(0x20, 7, "opt/berry/trace"), entry(0x21, 28, "etc/ramfb")]);
    let mut s1 = VecStream::new(&d);
    let e = find_file_in("etc/ramfb", &mut s1).expect("第二项命中");
    assert_eq!(e, FileEntry { select: 0x21, size: 28 });
    let mut s2 = VecStream::new(&d);
    let e2 = find_file_in("opt/berry/trace", &mut s2).expect("首项命中");
    assert_eq!(e2, FileEntry { select: 0x20, size: 7 });
    let mut s3 = VecStream::new(&d);
    assert!(find_file_in("opt/berry/missing", &mut s3).is_none(), "缺席 = None");
    // 前缀名非等值匹配（"opt/berry/trac" 不得命中 "opt/berry/trace"）。
    let mut s4 = VecStream::new(&d);
    assert!(find_file_in("opt/berry/trac", &mut s4).is_none());
}

#[test]
fn find_be_domain_restore() {
    // BE 域恢复：size=0x0100_0000（16MiB）、select=0x0020 高 16 位。
    let d = dir(&[entry(0x0020, 0x0100_0000, "big")]);
    let mut s = VecStream::new(&d);
    let e = find_file_in("big", &mut s).unwrap();
    assert_eq!(e.select, 0x0020);
    assert_eq!(e.size, 0x0100_0000);
    // select 域恢复单元律（高 16 位；reserved 低 16 位剥离）。
    assert_eq!(select_of(0x0020_0000u32.swap_bytes()), 0x0020);
    assert_eq!(select_of(0xFFFF_0001u32.swap_bytes()), 0xFFFF);
}

#[test]
fn find_malformed_rejects() {
    // count 帽：>1024 拒。
    let mut v = 1025u32.to_be_bytes().to_vec();
    let mut s = VecStream::new(&v);
    assert!(find_file_in("x", &mut s).is_none());
    // 恰帽（1024）不拒——目录本身合法（空项循环零迭代）。
    let v2 = 1024u32.to_be_bytes();
    let mut s2 = VecStream::new(&v2);
    assert!(find_file_in("x", &mut s2).is_none(), "帽内零项 = 无命中非拒");
    // 查询名帽：≥56 恒 None（含恰 56）。
    let long: String = "a".repeat(56);
    let d = dir(&[]);
    let mut s3 = VecStream::new(&d);
    assert!(find_file_in(&long, &mut s3).is_none());
    let max_ok: String = "a".repeat(55);
    let d2 = dir(&[entry(1, 1, &max_ok)]);
    let mut s4 = VecStream::new(&d2);
    assert!(find_file_in(&max_ok, &mut s4).is_some(), "55B = NAME_MAX 帽内合法");
    // 项名无 NUL（满 56B 非零）= 畸形项跳过，后续项仍可命中。
    let mut no_nul = Vec::new();
    no_nul.extend_from_slice(&1u32.to_be_bytes());
    no_nul.extend_from_slice(&0u32.to_be_bytes());
    no_nul.extend_from_slice(&[b'X'; NAME_LEN]);
    let d3 = dir(&[no_nul, entry(9, 3, "after")]);
    let mut s5 = VecStream::new(&d3);
    let e = find_file_in("after", &mut s5).expect("畸形项后仍命中");
    assert_eq!(e.select, 9);
}

#[test]
fn find_stream_truncation() {
    // 流截断三形：count 后字段缺/名字段半截/空流。
    let mut v = 1u32.to_be_bytes().to_vec();
    v.extend_from_slice(&[0u8; 8]); // size+sel_rsv 后名域截断
    let mut s = VecStream::new(&v);
    assert!(find_file_in("x", &mut s).is_none());
    let empty: Vec<u8> = Vec::new();
    let mut s2 = VecStream::new(&empty);
    assert!(find_file_in("x", &mut s2).is_none());
}

#[test]
fn seq_plan_granularity_law() {
    // 推进律核心：非末段 4 字节粒度、末段余数合法、零/空恒 0。
    let st = SeqState::open(10).unwrap();
    assert_eq!(st.plan(4), 4, "恰 4 缓冲全读");
    assert_eq!(st.plan(3), 0, "非末段 3B 缓冲取整后 0");
    assert_eq!(st.plan(8), 8, "8 ≤ remaining 10 全读");
    let st2 = SeqState::open(10).unwrap();
    assert_eq!(st2.plan(9), 8, "缓冲 9 < remaining 10：非末段取整 8");
    let st3 = SeqState::open(10).unwrap();
    assert_eq!(st3.plan(0), 0, "空缓冲恒 0");
    // 末段（buf ≥ remaining）：余数合法（10 不取整）。
    let st4 = SeqState::open(10).unwrap();
    assert_eq!(st4.plan(10), 10, "恰末段：余数合法");
    let st5 = SeqState::open(10).unwrap();
    assert_eq!(st5.plan(11), 10, "跨项尾缓冲：截到项尾不取整");
}

#[test]
fn seq_open_zero_rejects() {
    assert!(SeqState::open(0).is_none(), "空项拒（size==0）");
    assert!(SeqState::open(1).is_some());
}

#[test]
fn seq_progress_model_sum_invariant() {
    // Σ plan 恰等 size（含项尾余数截断）——推进量恒 = 实读数不漂移。
    for size in [1u32, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 28, 100, 4095, 4096, 4097] {
        let mut st = SeqState::open(size).unwrap();
        let mut total = 0usize;
        for buf in [4usize, 8, 3, 16, 1, 5, 4096] {
            let n = st.plan(buf);
            assert!(n <= buf && n <= st.remaining as usize, "plan 界内");
            if n < buf {
                // 非满读三合法态：项尾（remaining ≤ buf）/非末段取整（4 的
                // 倍数）/0。取整态断言 4 倍数（余数只许发生在项尾）。
                assert!(
                    n == 0
                        || st.remaining as usize <= buf
                        || (n < st.remaining as usize && n % 4 == 0),
                    "非满读非法：n={n} buf={buf} rem={}",
                    st.remaining
                );
            }
            st.advance(n);
            total += n;
        }
        // 游标耗尽后 plan 恒 0。
        while !st.done() {
            let n = st.plan(4096);
            st.advance(n);
            total += n;
        }
        assert_eq!(total as u32, size, "Σplan 恰等 size（size={size}）");
        assert_eq!(st.plan(64), 0, "会话尽恒 0");
    }
}

#[test]
fn seq_saturating_advance() {
    // advance 过推（调用方错序防御）：saturating 不下溢、不 panic。
    let mut st = SeqState::open(4).unwrap();
    st.advance(100);
    assert!(st.done());
    assert_eq!(st.remaining, 0);
}

#[test]
fn ramfb_gate_domain() {
    assert!(ramfb_cfg_ok(28, 640, 480, 2560), "合法模式");
    assert!(!ramfb_cfg_ok(27, 640, 480, 2560), "项长非 28 拒");
    assert!(!ramfb_cfg_ok(29, 640, 480, 2560), "项长非 28 拒（上侧）");
    assert!(!ramfb_cfg_ok(28, 0, 480, 2560), "零宽拒");
    assert!(!ramfb_cfg_ok(28, 640, 0, 2560), "零高拒");
    assert!(!ramfb_cfg_ok(28, 640, 480, 0), "零 stride 拒");
}

#[test]
fn constants_frozen_anchor() {
    // 冻结锚（HAL/QEMU 布局合同——改值 = 破坏 wire 协议，须三处同改：
    // 本 crate + HAL fwcfg.rs + QEMU hw/fw_cfg 源）。
    assert_eq!(DIR_MAX_ENTRIES, 1024);
    assert_eq!(NAME_LEN, 56);
    assert_eq!(NAME_MAX, 55);
    assert_eq!(RAMFB_CFG_LEN, 28);
}
