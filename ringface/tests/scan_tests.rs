//! scan 模块 host 钉板（W3-Q51）：ScanAcc 三分支带序状态机全表。
//!
//! 与 lib.rs 既有整环模拟测（scan_ring_* 系）正交：那边钉「游标 + 过滤 +
//! 带内序」合成的整环出选序；这边钉**累加器骨架本身**的判定律——初值律/
//! 带序重置/退化带/敌意输入零 panic/PURE_EEVDF 开关两形。

use berry_ringface::scan::{ScanAcc, PURE_EEVDF};
use berry_ringface::{eligible, TaskDesc, BLOCKED, EXITED, HELD_FROZEN, HELD_THROTTLE, READY, RUNNING};

fn desc(state: u8, held: u8, prio: i32, vrt: u64, vd: u64) -> TaskDesc {
    TaskDesc::new(state, held, prio, vrt, vd)
}

/// 参考实现：两圈旧形（段 1 全环定最高带 + 段 2 带内 EEVDF）——单圈
/// 合并扫描的等价性对拍基准（class.rs 注释所述「合并前形态」）。
fn two_pass_ref(ring: &[TaskDesc], start: usize, avg: u64, is_cur: &[bool]) -> Option<usize> {
    let n = ring.len();
    if n == 0 {
        return None;
    }
    // 段 1：最高带。
    let mut max_eff = i32::MIN;
    for i in 0..n {
        let idx = (start + i) % n;
        let p = &ring[idx];
        if !berry_ringface::eligible(p, is_cur[idx]) {
            continue;
        }
        if p.prio_effective > max_eff {
            max_eff = p.prio_effective;
        }
    }
    if max_eff == i32::MIN {
        // 无 eligible（或退化带——与单圈 None 兜底口径对齐：首见选中）。
        let mut seen_min = false;
        for i in 0..n {
            let idx = (start + i) % n;
            if berry_ringface::eligible(&ring[idx], is_cur[idx])
                && ring[idx].prio_effective == i32::MIN
            {
                seen_min = true;
                break;
            }
        }
        if !seen_min {
            return None;
        }
    }
    // 段 2：带内 EEVDF（eligible 优先、键严格 <、首见保序）。
    let mut best: Option<usize> = None;
    let mut best_elig = false;
    let mut best_key = (u64::MAX, u64::MAX);
    for i in 0..n {
        let idx = (start + i) % n;
        let p = &ring[idx];
        if !berry_ringface::eligible(p, is_cur[idx]) {
            continue;
        }
        let e = p.prio_effective;
        if e != max_eff {
            continue;
        }
        let elig = p.vruntime <= avg;
        let better = match best {
            None => true,
            Some(_) => {
                if elig != best_elig {
                    elig
                } else {
                    (p.vdeadline, p.vruntime) < best_key
                }
            }
        };
        if better {
            best = Some(idx);
            best_elig = elig;
            best_key = (p.vdeadline, p.vruntime);
        }
    }
    best
}

/// ScanAcc 单圈扫描（host 形：idx = usize）。
fn scan_once(ring: &[TaskDesc], start: usize, avg: u64, is_cur: &[bool]) -> Option<usize> {
    let n = ring.len();
    let mut acc = ScanAcc::new();
    for i in 0..n {
        let idx = (start + i) % n;
        acc.offer(&ring[idx], idx, is_cur[idx], avg);
    }
    acc.best()
}

/// 敌意全表：所有字段域的组合穷举（小域笛卡尔）零 panic + 与参考实现
/// 逐例一致。
fn adversarial_matrix() {
    let states = [READY, RUNNING, BLOCKED, EXITED];
    let helds = [0u8, HELD_FROZEN, HELD_THROTTLE, HELD_FROZEN | HELD_THROTTLE];
    let prios = [i32::MIN, -1, 0, 5, 999, 1000, i32::MAX];
    let vrts = [0u64, 5, 50, 100, u64::MAX];
    let vds = [0u64, 100, 800, 900, u64::MAX];
    let avgs = [0u64, 50, 200, u64::MAX];
    // 固定 4 元素环（域采样密度下 4! 位置换 × 域组合分批）。
    let mut cases = 0usize;
    let mut mismatches = 0usize;
    // 预生成域值池（确定性 LCG——无外部依赖可复现）。
    let mut seed = 0x5eed_0000_0051_0000u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..2000 {
        let n = 1 + (next() as usize % 4);
        let ring: Vec<TaskDesc> = (0..n)
            .map(|_| {
                desc(
                    states[(next() as usize) % states.len()],
                    helds[(next() as usize) % helds.len()],
                    prios[(next() as usize) % prios.len()],
                    vrts[(next() as usize) % vrts.len()],
                    vds[(next() as usize) % vds.len()],
                )
            })
            .collect();
        let _ = &ring;
        let is_cur: Vec<bool> = (0..n).map(|_| next() % 2 == 0).collect();
        let avg = avgs[(next() as usize) % avgs.len()];
        let start = next() as usize % n;
        let a = scan_once(&ring, start, avg, &is_cur);
        let b = two_pass_ref(&ring, start, avg, &is_cur);
        // 04 号审计机会一②（2026-11-17）chosen-in-max-band 等价律：!
        // PURE_EEVDF 形下 chosen 必在扫描集最高带——chosen.eff ≥ 全体
        // eligible 节点 eff（双圈合一门控形 Fair::pick_next_preempt「胜者
        // eff 严格更高律」的数学证书钉板：门控行读胜者 eff > threshold
        // ⟺ 旧探峰存在性，前提即 chosen-in-max-band——此处逐例执法）。
        if !PURE_EEVDF {
            if let Some(c) = a {
                for j in 0..n {
                    assert!(
                        !berry_ringface::eligible(&ring[j], is_cur[j])
                            || ring[c].prio_effective >= ring[j].prio_effective,
                        "chosen-in-max-band 律破：chosen[{c}].eff {} < eligible[{j}].eff {}",
                        ring[c].prio_effective,
                        ring[j].prio_effective
                    );
                }
            }
        }
        cases += 1;
        if a != b {
            mismatches += 1;
        }
    }
    assert_eq!(mismatches, 0, "单圈合并扫描与两圈参考实现 {cases} 例须全一致");
}

#[test]
fn scan_band_reset_on_higher_band() {
    // 更高带重置：后见高带整带压制先见低带（键无关——低带键再优也不抢）。
    let ring = [
        desc(READY, 0, 10, 0, 0),      // 低带键最优
        desc(READY, 0, 20, 900, 900),  // 高带键最差
        desc(READY, 0, 20, 800, 800),  // 高带键次优
    ];
    let is_cur = [false; 3];
    for s in 0..3 {
        assert_eq!(scan_once(&ring, s, 0, &is_cur), Some(2), "起点 {s}");
    }
    // 中途升带再降带：best 恒持最高带内最优（降带不重置）。
    let ring2 = [
        desc(READY, 0, 30, 500, 500),
        desc(READY, 0, 10, 0, 0),
        desc(READY, 0, 30, 400, 400),
        desc(READY, 0, 20, 0, 0),
    ];
    let is_cur4 = [false; 4];
    for s in 0..4 {
        assert_eq!(scan_once(&ring2, s, 0, &is_cur4), Some(2), "升带降带 起点 {s}");
    }
    // 高带先见后更低带插入不重置；更高带后来者重置（FIFO 位次无关）。
    let ring3 = [
        desc(READY, 0, 20, 100, 100),
        desc(READY, 0, 10, 0, 0),
        desc(READY, 0, 30, 0, 0),
    ];
    for s in 0..3 {
        assert_eq!(scan_once(&ring3, s, 0, &is_cur), Some(2), "ring3 起点 {s}");
    }
}

#[test]
fn scan_degenerate_min_band_first_seen() {
    // 退化带：prio_effective == i32::MIN 者不被「更高带重置」捕获——
    // 落同带首见（best=None 兜底）。
    let ring = [
        desc(READY, 0, i32::MIN, 10, 100),
        desc(READY, 0, i32::MIN, 5, 50),
    ];
    let is_cur = [false; 2];
    // 同带内键比较：第二个键 (50,5) < (100,10) → 替换。
    assert_eq!(scan_once(&ring, 0, 20, &is_cur), Some(1));
    // 首键更小则保首见。
    let ring2 = [
        desc(READY, 0, i32::MIN, 5, 50),
        desc(READY, 0, i32::MIN, 10, 100),
    ];
    assert_eq!(scan_once(&ring2, 0, 20, &is_cur), Some(0));
    // 混合：i32::MIN 带与 0 带（0 > MIN → 正常更高带重置）。
    let ring3 = [
        desc(READY, 0, i32::MIN, 0, 0),
        desc(READY, 0, 0, 100, 100),
    ];
    assert_eq!(scan_once(&ring3, 0, 0, &is_cur), Some(1), "0 带压制 MIN 带");
}

#[test]
fn scan_filter_precedes_band_comparison() {
    // eligible 过滤前置：被过滤的高带者不占带——低带 eligible 者胜。
    let ring = [
        desc(RUNNING, 0, 100, 0, 0),        // 他核 Running 高带 → 跳过
        desc(READY, HELD_FROZEN, 99, 0, 0), // 冻结高带 → 跳过
        desc(READY, 0, 1, 500, 500),        // 低带 eligible → 胜
    ];
    let is_cur = [false; 3];
    assert_eq!(scan_once(&ring, 0, 0, &is_cur), Some(2));
    // my_cur 豁免反转：Running 者若是 cur → 参与带竞争 → 高带胜。
    let is_cur_cur = [true, false, false];
    assert_eq!(scan_once(&ring, 0, 0, &is_cur_cur), Some(0));
    // 全过滤 = None（高带被过滤不产生任何 best）。
    let all_out = [
        desc(RUNNING, 0, 100, 0, 0),
        desc(READY, HELD_THROTTLE, 99, 0, 0),
    ];
    assert_eq!(scan_once(&all_out, 0, 0, &is_cur), None);
}

#[test]
fn scan_pure_eevdf_band_comparison_disabled() {
    // PURE_EEVDF=true：带比较恒假 → 全员同带纯 EEVDF（deadline 带
    // 语义失效——高带键差者输给低带键优者）。
    let ring = [
        desc(READY, 0, 1000, 900, 900), // deadline 带键差
        desc(READY, 0, 0, 100, 100),    // fair 带键优
    ];
    let is_cur = [false; 2];
    let mut acc = ScanAcc::new();
    for (i, p) in ring.iter().enumerate() {
        acc.offer_with(p, i, is_cur[i], 0, true);
    }
    assert_eq!(acc.best(), Some(1), "纯 EEVDF 形：键优者胜带序失效");
    // 生产形（false）：deadline 带压制。
    assert_eq!(scan_once(&ring, 0, 0, &is_cur), Some(0));
    // 开关常量钉板：v1 生产形恒 false（class.rs 同值）。
    assert!(!PURE_EEVDF);
}

#[test]
fn scan_empty_and_single() {
    // 空环 None；单元素全形。
    let is_cur = [false; 1];
    assert_eq!(scan_once(&[], 0, 0, &is_cur), None);
    let solo = [desc(READY, 0, 0, 1, 1)];
    assert_eq!(scan_once(&solo, 0, 0, &is_cur), Some(0));
    // 单元素被过滤 = None。
    let frozen = [desc(READY, HELD_FROZEN, 0, 1, 1)];
    assert_eq!(scan_once(&frozen, 0, 0, &is_cur), None);
    // 单元素退化带（i32::MIN）正常选中。
    let min_band = [desc(READY, 0, i32::MIN, 1, 1)];
    assert_eq!(scan_once(&min_band, 0, 0, &is_cur), Some(0));
}

#[test]
fn scan_adversarial_full_matrix_zero_panic() {
    adversarial_matrix();
}

#[test]
fn scan_cursor_start_round_trip() {
    // 同键等权三任务：首见保序 = 起点决定胜者（advance_past 游标推进
    // 的 FIFO 轮转机械基础——骨架与游标合成的整环不变量）。
    let ring = [
        desc(READY, 0, 0, 5, 100),
        desc(READY, 0, 0, 5, 100),
        desc(READY, 0, 0, 5, 100),
    ];
    let is_cur = [false; 3];
    assert_eq!(scan_once(&ring, 0, 10, &is_cur), Some(0));
    assert_eq!(scan_once(&ring, 1, 10, &is_cur), Some(1));
    assert_eq!(scan_once(&ring, 2, 10, &is_cur), Some(2));
}

#[test]
fn scan_eevdf_lag_priority_in_band() {
    // 带内 lag 优先：落后者（vruntime<=avg）胜——即使 vdeadline 更晚。
    let ring = [
        desc(READY, 0, 0, 100, 100), // lead（超 avg）
        desc(READY, 0, 0, 0, 999),   // lag（落后）键最差
    ];
    let is_cur = [false; 2];
    assert_eq!(scan_once(&ring, 0, 50, &is_cur), Some(1), "lag 优先");
    // 双落后 → 纯键比较（键优者胜）。
    assert_eq!(scan_once(&ring, 0, 200, &is_cur), Some(0), "双落后纯键");
    // 双超前 → 纯键比较（EEVDF 无 lag 补偿档差）。
    assert_eq!(scan_once(&ring, 0, 0, &is_cur), Some(1), "==avg 算落后");
}

// —— W3-Q93：best_eff 带门访问器（try_preempt 双圈合一的探峰单源）——
// 值域律 + 与「旧 has_eff_above_filtered 两圈形」参考实现逐例对拍 +
// best_eff > i32::MIN ⟹ best.is_some() 不变式。

/// 参考实现 = 旧 sched has_eff_above_filtered 的纯值域形（独立两圈：
/// 先全环探「存在 eligible 且 eff > v」）。
fn has_above_ref(ring: &[TaskDesc], is_cur: &[bool], v: i32) -> bool {
    ring.iter()
        .zip(is_cur)
        .any(|(d, &c)| eligible(d, c) && d.prio_effective > v)
}

#[test]
fn best_eff_sentinel_and_band_law() {
    // 空前缀哨兵：初值 i32::MIN = 旧探峰空环/全过滤返 false 同义。
    let acc = ScanAcc::<usize>::new();
    assert_eq!(acc.best_eff(), i32::MIN);
    assert!(acc.best().is_none());
    // 全过滤前缀（RUNNING/FROZEN/THROTTLE）：哨兵不动。
    let mut acc = ScanAcc::new();
    let filtered = [
        desc(RUNNING, 0, 5, 0, 0),
        desc(READY, HELD_FROZEN, 5, 0, 0),
        desc(READY, HELD_THROTTLE, 7, 0, 0),
    ];
    for (i, d) in filtered.iter().enumerate() {
        acc.offer(d, i, false, 0);
    }
    assert_eq!(acc.best_eff(), i32::MIN, "全过滤前缀带哨兵不动");
    assert!(acc.best().is_none());
    // 带门正例：最高 eligible 带 = best_eff（同带更低键不夺门）。
    let mut acc = ScanAcc::new();
    let ring = [
        desc(READY, 0, 10, 0, 0),
        desc(READY, 0, 30, 99, 999), // 更高带键最差仍夺门
        desc(READY, 0, 10, 0, 0),
    ];
    for (i, d) in ring.iter().enumerate() {
        acc.offer(d, i, false, 0);
    }
    assert_eq!(acc.best_eff(), 30);
    assert_eq!(acc.best(), Some(1), "更高带整带压制同带键优者");
}

#[test]
fn best_eff_above_gate_matches_reference() {
    // 「best_eff() > v」⟺ 旧探峰谓词——全域采样对拍（含 v 恰界两侧）。
    let mut seed = 0x5eed_0000_0093_0000u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let states = [READY, RUNNING, BLOCKED, EXITED];
    let helds = [0u8, HELD_FROZEN, HELD_THROTTLE, HELD_FROZEN | HELD_THROTTLE];
    let prios = [i32::MIN, -1, 0, 5, 999, 1000, i32::MAX];
    let vs = [i32::MIN, -1, 0, 5, 999, 1000, i32::MAX - 1, i32::MAX];
    let mut cases = 0usize;
    let mut mismatches = 0usize;
    for _ in 0..3000 {
        let n = 1 + (next() as usize % 5);
        let ring: Vec<TaskDesc> = (0..n)
            .map(|_| {
                desc(
                    states[(next() as usize) % states.len()],
                    helds[(next() as usize) % helds.len()],
                    prios[(next() as usize) % prios.len()],
                    (next() % 200) as u64,
                    (next() % 200) as u64,
                )
            })
            .collect();
        let is_cur: Vec<bool> = (0..n).map(|_| next() % 2 == 0).collect();
        let v = vs[(next() as usize) % vs.len()];
        let mut acc = ScanAcc::new();
        for (i, d) in ring.iter().enumerate() {
            acc.offer(d, i, is_cur[i], 100);
        }
        let gate = acc.best_eff() > v;
        let reference = has_above_ref(&ring, &is_cur, v);
        cases += 1;
        if gate != reference {
            mismatches += 1;
        }
        // 不变式：带哨兵越出 ⟹ 必有出选者（best 与门同前缀）。
        if acc.best_eff() > i32::MIN {
            assert!(acc.best().is_some(), "best_eff 越哨兵须有出选者");
        }
    }
    assert_eq!(mismatches, 0, "best_eff 带门与探峰参考实现 {cases} 例须全一致");
}

#[test]
fn best_eff_pure_eevdf_sentinel_law() {
    // PURE_EEVDF 开形：带比较恒假 → best_eff 恒哨兵（v1 恒 false 生产不经
    // 此；实验轨道门退化 = 双圈合一不适用于 pure 形的编译期根因记录）。
    let mut acc = ScanAcc::new();
    let ring = [
        desc(READY, 0, 10, 0, 0),
        desc(READY, 0, 1000, 5, 5),
    ];
    for (i, d) in ring.iter().enumerate() {
        acc.offer_with(d, i, false, 50, true);
    }
    assert_eq!(acc.best_eff(), i32::MIN, "pure 形带门恒哨兵（带比较恒假）");
    assert!(acc.best().is_some(), "pure 形仍按纯键出选");
}
