//! dpgface host 钉板——pager 表批窗装配（W3-Q73，tblwin 模块）：
//! n 门域 / 16B 小端拼装 / delta 页对齐（负值域）/ 判序首错即停 /
//! 往返指纹 / 冻结锚。

use berry_dpgface::tblwin::*;
use berry_dpgface::SLOTS;

/// 构造单窗 16B 小端字节（测试向量的向量——独立于被测
/// `window_to_bytes` 手拼，防自证）。
fn win_bytes(dv: u32, sv: u32, delta: i64) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0..4].copy_from_slice(&dv.to_le_bytes());
    b[4..8].copy_from_slice(&sv.to_le_bytes());
    b[8..16].copy_from_slice(&delta.to_le_bytes());
    b
}

// ---------------------------------------------------------------------------
// 冻结锚（X-6 §2.1 布局 + 四源镜像帽）
// ---------------------------------------------------------------------------

#[test]
fn t01_frozen_constants() {
    // X-6 冻结 N=32：与 EREQ_SLOTS / svcmgr PAGER_SLOTS 四源镜像。
    assert_eq!(PAGER_TABLE_MAX, 32);
    assert_eq!(PAGER_TABLE_MAX, berry_dpgface::SLOTS);
    // repr(C) u32+u32+i64 = 16B（kernel 侧另有 size_of const assert）。
    assert_eq!(WINDOW_BYTES, 16);
    // delta 对齐除数（berry_mm::va::PAGE_SIZE 镜像）。
    assert_eq!(PAGE_SIZE, 4096);
}

// ---------------------------------------------------------------------------
// n 门：0 拒 / 恰界放行 / 超限拒（全宽读）
// ---------------------------------------------------------------------------

#[test]
fn t02_n_gate_empty_rejected() {
    // 0x24 至少一窗——与 Q33 policy 空表合法**相反**的语义钉板
    // （旧内核 `n == 0 ||` 逐字序；防误抄 policy 口径加 Empty 放行）。
    assert_eq!(n_gate(0), Err(NGate::Empty));
    // 整表装配同态：0 在拷入前即拒（decode_table 的 N 门先于逐窗）。
    assert_eq!(decode_table(&[0u8; 512], 0), Err(TableErr::NGate(NGate::Empty)));
}

#[test]
fn t03_n_gate_boundaries() {
    // 恰界 1/32 放行。
    assert_eq!(n_gate(1), Ok(()));
    assert_eq!(n_gate(32), Ok(()));
    // 越界 33 拒——「全宽读」钉板：kernel 侧 tf.regs[2] as usize 不做
    // u32 截断受纳，x2 高 32 位脏值形态（1<<32 形）同拒。
    assert_eq!(n_gate(33), Err(NGate::TooBig));
    assert_eq!(n_gate(usize::MAX), Err(NGate::TooBig));
    assert_eq!(n_gate(1usize << 32), Err(NGate::TooBig)); // 高 32 位脏值
    // 判序：Empty 先于 TooBig（短路左先——同码无观测差，钉序防漂）。
    // n=0 不可能同时 TooBig，此处钉 n_gate 两个分支互斥稳定。
    assert_ne!(n_gate(0), Err(NGate::TooBig));
}

// ---------------------------------------------------------------------------
// 单窗 16B 小端拼装（dv/sv/delta 三域字节序）
// ---------------------------------------------------------------------------

#[test]
fn t04_decode_window_le_layout() {
    // 逐字节小端：dv 0x11223344 → 44 33 22 11；sv 0x55667788 → 88 77 66 55。
    let w = decode_window(&win_bytes(0x1122_3344, 0x5566_7788, 0)).unwrap();
    assert_eq!(w, Window { dv: 0x1122_3344, sv: 0x5566_7788, delta: 0 });

    // delta i64 全宽：i64::MIN/MAX 页对齐端点 + 字节序独立性。
    for delta in [0i64, 4096, -4096, i64::MAX - 4095, i64::MIN] {
        let w = decode_window(&win_bytes(1, 2, delta)).unwrap();
        assert_eq!(w.delta, delta, "delta={} 往返", delta);
    }
    // i64::MAX = 0x7FFF..FC00 非 4096 倍数 → 拒（端点恰界由 t06 钉）。
    assert_eq!(decode_window(&win_bytes(1, 2, i64::MAX)), Err(WinErr::BadDelta));
}

#[test]
fn t05_decode_window_sv_zero_passthrough() {
    // sv = 0 零页臂语义在门控段——装配面原样放行（不代答）。
    let w = decode_window(&win_bytes(7, 0, 0)).unwrap();
    assert_eq!(w.sv, 0);
    // dv = 0（boot 探测形 0x24(dv:0,n:1)）装配面亦放行——门败 -EBADF
    // 在后（判序：装配 ≠ 门控，两段不合并）。
    let w = decode_window(&win_bytes(0, 0, 0)).unwrap();
    assert_eq!(w.dv, 0);
}

// ---------------------------------------------------------------------------
// delta 页对齐域（i64 可负——Rust % 同号余语义）
// ---------------------------------------------------------------------------

#[test]
fn t06_delta_alignment_domain() {
    // 正对齐放行。
    for d in [0i64, 4096, 8192, 1 << 40] {
        assert_eq!(decode_window(&win_bytes(1, 1, d)), Ok(Window { dv: 1, sv: 1, delta: d }));
    }
    // 正非对齐拒（恰界 ±1）。
    for d in [1i64, 4095, 4097, (1 << 40) + 1] {
        assert_eq!(decode_window(&win_bytes(1, 1, d)), Err(WinErr::BadDelta));
    }
    // 负对齐放行（恒等选源常数可负——同 0x23）。
    for d in [-4096i64, -8192, -(1 << 40), i64::MIN] {
        assert_eq!(decode_window(&win_bytes(1, 1, d)), Ok(Window { dv: 1, sv: 1, delta: d }));
    }
    // 负非对齐拒（Rust % 同号余：-4095 % 4096 == -4095 ≠ 0）。
    for d in [-1i64, -4095, -4097, i64::MAX, i64::MIN + 1] {
        assert_eq!(decode_window(&win_bytes(1, 1, d)), Err(WinErr::BadDelta));
    }
    // i64::MIN 恰对齐（2^63 = 4096 的倍数）——极值端点钉板。
    assert_eq!(decode_window(&win_bytes(1, 1, i64::MIN)).map(|w| w.delta), Ok(i64::MIN));
}

// ---------------------------------------------------------------------------
// 整表：判序首错即停 + 尾部零窗 + 满装
// ---------------------------------------------------------------------------

#[test]
fn t07_table_first_error_wins() {
    // 第 2 窗 delta 坏 → 首错即停（idx 锚定），第 3 窗更坏不掩盖。
    let mut raw = [0u8; 512];
    raw[0..16].copy_from_slice(&win_bytes(1, 1, 0));
    raw[16..32].copy_from_slice(&win_bytes(2, 2, 4097)); // 坏
    raw[32..48].copy_from_slice(&win_bytes(3, 3, 1)); // 更坏（不达）
    assert_eq!(decode_table(&raw, 3), Err(TableErr::BadDelta { idx: 1 }));
    // 修复第 2 窗 → 第 3 窗首错浮现（idx 推进——首错位置可观测）。
    raw[16..32].copy_from_slice(&win_bytes(2, 2, 4096));
    assert_eq!(decode_table(&raw, 3), Err(TableErr::BadDelta { idx: 2 }));
    raw[32..48].copy_from_slice(&win_bytes(3, 3, -4096));
    let (wins, n) = decode_table(&raw, 3).unwrap();
    assert_eq!(n, 3);
    assert_eq!(wins[2], Window { dv: 3, sv: 3, delta: -4096 });
}

#[test]
fn t08_table_tail_zero_windows() {
    // n < MAX：尾部零窗 = kernel staging `[PagerWindow{0,0,0}; MAX]`
    // 初值同态（收货环只走 0..n，尾部不可达但须确定性）。
    let mut raw = [0u8; 512];
    raw[0..16].copy_from_slice(&win_bytes(9, 8, 4096));
    let (wins, n) = decode_table(&raw, 1).unwrap();
    assert_eq!(n, 1);
    assert_eq!(wins[0], Window { dv: 9, sv: 8, delta: 4096 });
    for i in 1..SLOTS {
        assert_eq!(wins[i], Window::default(), "尾窗 {} 须零态", i);
    }
}

#[test]
fn t09_table_full_load_roundtrip() {
    // 满装 32 窗 LCG 伪随机全字段往返（dv/sv/delta 域内取值）。
    let mut raw = [0u8; 512];
    let mut x: u64 = 0x2424_0001; // 0x24 系种子
    let mut expect = [Window::default(); SLOTS];
    for i in 0..32 {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let dv = (x >> 32) as u32;
        let sv = if (x >> 31) & 1 == 0 { 0 } else { (x >> 40) as u32 };
        let delta = (((x >> 8) as i64) % 1024) * 4096; // 对齐域
        raw[i * 16..i * 16 + 16].copy_from_slice(&win_bytes(dv, sv, delta));
        expect[i] = Window { dv, sv, delta };
    }
    let (wins, n) = decode_table(&raw, 32).unwrap();
    assert_eq!(n, 32);
    assert_eq!(wins, expect);
}

#[test]
fn t10_table_gate_precedes_windows() {
    // 判序：N 门先于逐窗——n 越界时即使字节全坏也报 NGate 不报 BadDelta。
    let mut raw = [0xFFu8; 512]; // 全 0xFF：任何窗解码都 BadDelta
    assert_eq!(decode_table(&raw, 33), Err(TableErr::NGate(NGate::TooBig)));
    assert_eq!(decode_table(&raw, 0), Err(TableErr::NGate(NGate::Empty)));
    // 恰界 n=1 只看首窗——第 2 窗坏字节不达（拷入长度 = n*16 语义在
    // kernel 电压段，本面从 raw 就位处接管只扫 0..n）。
    raw[0..16].copy_from_slice(&win_bytes(1, 1, 0));
    assert!(decode_table(&raw, 1).is_ok());
}

// ---------------------------------------------------------------------------
// 往返指纹（window_to_bytes ↔ decode_window）
// ---------------------------------------------------------------------------

#[test]
fn t11_roundtrip_fingerprint() {
    // 编码-解码全往返：装配律冻结的指纹（任一端漂移即红）。
    let cases = [
        Window { dv: 0, sv: 0, delta: 0 },
        Window { dv: u32::MAX, sv: u32::MAX, delta: 0 },
        Window { dv: 1, sv: 0, delta: i64::MIN },
        Window { dv: 0xDEAD_BEEF, sv: 0xCAFE_F00D, delta: -(1 << 32) },
        Window { dv: 0x0000_0001, sv: 0x8000_0000, delta: 1 << 52 },
    ];
    for w in cases {
        let b = window_to_bytes(&w);
        assert_eq!(decode_window(&b), Ok(w), "roundtrip {:?}", w);
    }
    // window_to_bytes 与手拼向量逐字节一致（双实现锚）。
    assert_eq!(window_to_bytes(&Window { dv: 0x1122_3344, sv: 0x5566_7788, delta: -4096 }), win_bytes(0x1122_3344, 0x5566_7788, -4096));
}
