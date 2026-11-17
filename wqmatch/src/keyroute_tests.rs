//! berry-wqmatch keyroute host 钉板（W3-Q75）。
//!
//! 覆盖：桶散列全域性质/真键判据三形/计数分账三形对称/PI donor 判据
//! 三门独立/门控双零/常量冻结锚。参考实现对拍（独立写 Fibonacci 散列
//! 镜像——公式级独立而非调用被测函数）。

use super::keyroute::*;

/// 参考实现：Fibonacci 乘移散列（公式级独立镜像——u128 全宽乘 + 显式
/// mod 2^64 取低段再移位，与被测 `wrapping_mul + >> 58` 不同代码路径
/// 同值——sched 侧散列取的是低 64 位积的高 6 位，非 128 位积顶位）。
fn ref_bucket(key: u64) -> usize {
    let prod = (key as u128) * (0x9E3779B97F4A7C15u64 as u128);
    let lo = prod % (1u128 << 64);
    ((lo >> 58) as usize) % 64
}

#[test]
fn bucket_domain_all_keys() {
    // 全域扫描：任意键（含 0/WAIT_ANY/毒值）桶号恒在 [0,64)。
    let mut keys = vec![0usize, 1, 63, 64, 0x9E3779B97F4A7C15usize, usize::MAX];
    for k in 1..2000u64 {
        keys.push(k as usize * 2654435761);
    }
    for &k in &keys {
        let b = wait_bucket(k);
        assert!(b < WAIT_BUCKETS, "key {k:#x} bucket {b} 越域");
    }
}

#[test]
fn bucket_reference_agreement() {
    // 参考对拍：2000 键全吻合（u128 全宽镜像 vs wrapping 乘移）。
    for k in 0..2000u64 {
        let key = k.wrapping_mul(11400714819323198485).wrapping_add(k * 7 + 3);
        assert_eq!(wait_bucket(key as usize), ref_bucket(key), "key {key:#x}");
    }
}

#[test]
fn bucket_zero_key_zero() {
    // 键 0 恒桶 0（乘零 = 零）——纯睡不计数故此桶值无消费面，钉住防
    // 散列改形时 0 键漂移进活桶。
    assert_eq!(wait_bucket(0), 0);
}

#[test]
fn bucket_distribution_sanity() {
    // 分布健全性：2000 均匀步进键 ≥ 60 桶被命中（64 桶 Fibonacci 散列
    // 的基本铺开性——退化散列（如全落同桶）即红）。
    let mut hit = [false; WAIT_BUCKETS];
    for k in 0..2000u64 {
        hit[wait_bucket((k.wrapping_mul(6364136223846793005) + 1) as usize)] = true;
    }
    let n = hit.iter().filter(|&&h| h).count();
    assert!(n >= 60, "Fibonacci 散列只铺 {n} 桶——退化");
}

#[test]
fn key_chainable_three_forms() {
    assert!(!key_chainable(0), "纯睡不入键链");
    assert!(!key_chainable(crate::WAIT_ANY), "哨兵不入键链");
    assert!(key_chainable(1), "真键入");
    assert!(key_chainable(0x1234_5678), "真键入");
    // WaitKey::Obj 值域恰等钉板（三形投影一致性）。
    for raw in [1usize, 42, 0xdead_beef] {
        assert_eq!(key_chainable(raw), crate::WaitKey::from_raw(raw) == crate::WaitKey::Obj(raw));
    }
}

#[test]
fn count_kind_three_forms() {
    assert_eq!(count_kind(0), CountKind::Skip);
    assert_eq!(count_kind(crate::WAIT_ANY), CountKind::AnySentinel);
    // 真键：桶号与 wait_bucket 单源一致（枚举携带）。
    let k = 0x1000;
    assert_eq!(count_kind(k), CountKind::Bucket(wait_bucket(k)));
    // add/sub 判据对称钉板（同一 obj 恒同形——加减配平根基）。
    for obj in [0usize, crate::WAIT_ANY, 7, 0x9999] {
        assert_eq!(count_kind(obj), count_kind(obj), "恒等平凡");
        match count_kind(obj) {
            CountKind::Bucket(b) => assert!(b < WAIT_BUCKETS),
            _ => {}
        }
    }
}

#[test]
fn pi_donor_candidate_gates() {
    // 四门独立：真键门 × 派生位门 × d_eff 门（W3-Q94 刀 a 起三门合取）。
    assert!(pi_donor_candidate(1, 1, 32), "真键 + READABLE + d_eff=32");
    assert!(pi_donor_candidate(1, 2, 1), "真键 + WRITABLE + d_eff=1 恰界过");
    assert!(pi_donor_candidate(1, 3, i32::MAX), "真键 + 双派生位 + 极值");
    assert!(pi_donor_candidate(1, 0xF_0000_0003, 40), "高位无关——派生位相交即过");
    // d_eff 门（刀 a 新面）：d_eff=0 恒拒——prio 0 donor 恒无捐赠可传。
    assert!(!pi_donor_candidate(1, 3, 0), "d_eff=0 拒（全零稳态门闭合面）");
    assert!(!pi_donor_candidate(1, 3, -5), "d_eff 负值拒（>0 严格门）");
    // 键门/位门维持（W3-Q75 语义面）：与 d_eff 门独立——d_eff>0 不救坏键/坏位。
    assert!(!pi_donor_candidate(1, 0, 32), "真键 + 非零 d_eff + 零信号（纯超时睡）拒");
    assert!(!pi_donor_candidate(1, 4, 32), "非派生位（bit2+）拒——d_eff 不救位门");
    assert!(!pi_donor_candidate(0, 3, 32), "纯睡拒（键门先于位门/d_eff 门）");
    assert!(!pi_donor_candidate(crate::WAIT_ANY, 3, 32), "哨兵拒（多对象不钉 donor）");
}

/// W3-Q94 刀 a：d_eff = max(prio_effective, pi_inherited) 与传播环取值
/// 同构——pi_inherited 单独抬 d_eff 过门（入队 eff=0、睡眠中被传播的
/// 欠账形态；配对靠重推尾趟 pi_derived_recount 补登）。
#[test]
fn pi_donor_d_eff_pi_component() {
    // eff=0 但 pi>0：d_eff = max(0, pi) = pi > 0 过门（传播环同构）。
    assert!(pi_donor_candidate(1, 3, 0.max(8)), "eff=0/pi=8 → d_eff=8 过门");
    assert!(!pi_donor_candidate(1, 3, 0.max(0)), "eff=0/pi=0 → d_eff=0 拒");
    // eff 主导：pi=0 但 eff>0 同过（冻结 boost 清零后 base 仍高的 donor）。
    assert!(pi_donor_candidate(1, 3, 16.max(0)), "eff=16/pi=0 过门");
    // 逐点对拍：d_eff 判据 == max(eff, pi) > 0（四象限全表）。
    for eff in [0i32, 1, 16, -3, i32::MIN, i32::MAX] {
        for pi in [0i32, 1, 8, -1, i32::MIN, i32::MAX] {
            let expected = pi > 0 || eff > 0;
            assert_eq!(
                pi_donor_candidate(1, 3, eff.max(pi)),
                expected,
                "eff={eff} pi={pi}"
            );
        }
    }
}

/// W3-Q94 刀 a 第三收口点扩展：pi_derived_recount_pi 累加器与入队判据
/// 逐字同源（同一函数消费）——全表对拍 + 全零稳态门闭合端到端。
#[test]
fn pi_derived_recount_accumulates() {
    // 全零稳态（v1 常态）：全员 d_eff=0 → 计数 0 → 门闭。
    let mut acc = 0u32;
    pi_derived_recount_pi(0x1000, 2 /* WRITABLE */, 0, &mut acc);
    pi_derived_recount_pi(0x2000, 1 /* READABLE */, 0, &mut acc);
    assert_eq!(acc, 0, "全零稳态：计数恒 0（门闭）");
    // 真 donor 命中累加；坏键/坏位/d_eff=0 不计。
    let mut acc = 0u32;
    pi_derived_recount_pi(0x1000, 2 /* WRITABLE */, 32, &mut acc);
    pi_derived_recount_pi(0x2000, 1 /* READABLE */, 0, &mut acc);
    pi_derived_recount_pi(0, 2 /* WRITABLE */, 32, &mut acc);
    pi_derived_recount_pi(crate::WAIT_ANY, 3, 32, &mut acc);
    assert_eq!(acc, 1, "恰一真 donor（32 优先）命中");
    // 多 donor 全计（多客户端服务形态）。
    let mut acc = 0u32;
    for eff in [1, 8, 36, 40] {
        pi_derived_recount_pi(0x1000 + eff as usize, 2 /* WRITABLE */, eff, &mut acc);
    }
    assert_eq!(acc, 4, "四 donor 全计（多客户端阻塞 WRITABLE 形态）");
}

#[test]
fn pi_boost_counted_forms() {
    assert!(!pi_boost_counted(0));
    assert!(pi_boost_counted(1));
    assert!(pi_boost_counted(-1));
    assert!(pi_boost_counted(i32::MIN));
    assert!(pi_boost_counted(i32::MAX));
}

#[test]
fn gate_clear_law() {
    // P1 门控：双零才清（任一非零 = 有等待者可能，须扫）。
    assert!(gate_clear(0, 0));
    assert!(!gate_clear(1, 0), "哨兵计数非零——恒扫");
    assert!(!gate_clear(0, 1), "桶计数非零——恒扫");
    assert!(!gate_clear(u32::MAX, u32::MAX));
}

#[test]
fn pi_gate_needed_law() {
    // PI 重推门：双零才免（任一非零 = donor 可能或 boost 待清）。
    assert!(!pi_gate_needed(0, 0), "双零——重推可跳");
    assert!(pi_gate_needed(1, 0), "donor 可能");
    assert!(pi_gate_needed(0, 1), "boost 待清");
    assert!(pi_gate_needed(u32::MAX, u32::MAX));
}

#[test]
fn constants_frozen() {
    // 常量冻结锚（sched 侧 const assert 镜像对拍——值变即双红）。
    assert_eq!(WAIT_BUCKETS, 64);
    assert_eq!(SIG_DERIVED, 3);
    assert_eq!(crate::WAIT_ANY, usize::MAX);
    // SIG_DERIVED 位分解（READABLE|WRITABLE 各恰一位）。
    assert_eq!(SIG_DERIVED.count_ones(), 2);
}

#[test]
fn count_kind_add_sub_symmetry_model() {
    // 加减配平模型：任意键序列入队/离队（计数表模型 [u32;64] + 哨兵
    // 独立面），终态全零——分账判据的对称性全周期验证。
    let mut model = [0u32; WAIT_BUCKETS];
    let mut any = 0u32;
    let objs = [
        0usize,           // 纯睡 ×2（不计数）
        0,
        crate::WAIT_ANY,  // 哨兵 ×1
        0x1000,           // 真键（桶 b1）
        0x1000,           // 同键二等待者
        0x2000,           // 异键（桶 b2）
        crate::WAIT_ANY,  // 哨兵 ×2
        0x1000,           // 真键 ×3
    ];
    for &o in &objs {
        match count_kind(o) {
            CountKind::Skip => {}
            CountKind::AnySentinel => any += 1,
            CountKind::Bucket(b) => model[b] += 1,
        }
    }
    assert_eq!(any, 2, "哨兵计数");
    let b1 = wait_bucket(0x1000);
    assert_eq!(model[b1], 3, "真键 0x1000 三等待者");
    let b2 = wait_bucket(0x2000);
    assert_ne!(b1, b2, "两键恰好异桶（本模型锚——值由散列定；钉住当前值防漂移）");
    assert_eq!(model[b2], 1);
    // 离队逆序全清。
    for &o in objs.iter().rev() {
        match count_kind(o) {
            CountKind::Skip => {}
            CountKind::AnySentinel => any -= 1,
            CountKind::Bucket(b) => model[b] -= 1,
        }
    }
    assert_eq!(any, 0);
    assert!(model.iter().all(|&c| c == 0), "终态全零");
}
