//! heapface host 钉板测试（W3-Q39）。

use berry_heapface::*;

/// SLAB_CLASSES 形状：严格递增 / 全 2 的幂 / 首 8 末 2048 / 与 MAX_CLASS 自洽。
#[test]
fn slab_classes_shape() {
    assert_eq!(SLAB_CLASSES.len(), 9);
    assert_eq!(SLAB_CLASSES[0], 8);
    assert_eq!(SLAB_CLASSES[8], MAX_CLASS);
    for (i, &c) in SLAB_CLASSES.iter().enumerate() {
        assert!(c.is_power_of_two(), "class {c} 非 2 的幂");
        if i > 0 {
            assert!(SLAB_CLASSES[i - 1] < c, "下标 {i} 不严格递增");
            // 相邻 class 恰 ×2（2 的幂梯）——多余间隙即配置漂移。
            assert_eq!(SLAB_CLASSES[i - 1] * 2, c, "下标 {i} 非恰翻倍梯");
        }
    }
}

/// class_index 全域：[1,2048] 上「向上取整到 class」逐点执法 + 边界。
#[test]
fn class_index_rounds_up_to_class() {
    // 全域逐点：eff ∈ [1,2048] → 首 class ≥ eff。
    for eff in 1..=MAX_CLASS {
        let ci = class_index(eff).expect("eff ≤ MAX_CLASS 必有类");
        assert!(SLAB_CLASSES[ci] >= eff, "eff={eff} class 不足");
        if ci > 0 {
            assert!(
                SLAB_CLASSES[ci - 1] < eff,
                "eff={eff} 取了过大的 class（非最小）"
            );
        } else {
            assert_eq!(ci, 0, "eff={eff} ≤ 8 必落首类");
        }
    }
    // 恰值直落：eff == class 时下标即该类（无 +1）。
    for (i, &c) in SLAB_CLASSES.iter().enumerate() {
        assert_eq!(class_index(c), Some(i), "恰值 eff={c} 必直落第 {i} 类");
        if i > 0 {
            // c-1（如 15）仍向上取整到 c 本身（相邻 class 恰 ×2，c-1 > 前类）。
            assert_eq!(class_index(c - 1), Some(i), "c-1 必仍落本类 {c}");
        }
    }
    // 越上限 → None（整页路由）。
    assert_eq!(class_index(MAX_CLASS + 1), None);
    assert_eq!(class_index(usize::MAX), None);
}

/// eff = max(size, align)：align 抬 class 律 + 大 align 走整页。
#[test]
fn effective_size_align_law() {
    assert_eq!(effective_size(24, 8), 24);
    assert_eq!(effective_size(24, 64), 64, "align 抬占主导");
    assert_eq!(effective_size(0, 16), 16);
    assert_eq!(effective_size(16, 0), 16);
    // 端到端：align 4096 必出 slab 域。
    assert_eq!(class_index(effective_size(8, 4096)), None);
    // align 抬恰界：size=17/align=16 → eff=17 → class 32（下标 2）。
    assert_eq!(class_index(effective_size(17, 16)), Some(2));
}

/// 切页几何：9 类全整除 4KiB 无尾浪费 + nslots 单调不增。
#[test]
fn slots_per_page_geometry() {
    let mut prev = usize::MAX;
    for &c in SLAB_CLASSES.iter() {
        let n = slots_per_page(c).expect("表内 class 必有槽数");
        assert_eq!(c * n, PAGE_SIZE, "class {c} 切页有余数（尾浪费）");
        assert!(n < prev || c == SLAB_CLASSES[0], "槽数应随 class 递减");
        prev = n;
    }
    assert_eq!(slots_per_page(8), Some(512));
    assert_eq!(slots_per_page(2048), Some(2));
    // 表外值（含 0 / MAX_CLASS+1 / 非 2 幂）一律 None。
    assert_eq!(slots_per_page(0), None);
    assert_eq!(slots_per_page(24), None);
    assert_eq!(slots_per_page(MAX_CLASS * 2), None);
}

/// div_round_up：ceil 律 + 恰除零虚胖 + 与手工算术全域一致。
#[test]
fn div_round_up_ceil_law() {
    // 恰除：商恰 n/d，无 +1。
    for k in 1..8usize {
        let n = k * PAGE_SIZE;
        assert_eq!(div_round_up(n, PAGE_SIZE), k, "恰除必零虚胖");
    }
    // 通用：ceil(n/d) 与 (n+d-1)/d 等值（同式重述——执法同式而非同函）。
    for n in 1..2000usize {
        for d in [1usize, 3, 7, 12, PAGE_SIZE] {
            let manual = (n + d - 1) / d;
            assert_eq!(div_round_up(n, d), manual, "n={n} d={d}");
            // ceil 语义下界：商 ≥ n/d。
            assert!(div_round_up(n, d) * d >= n);
        }
    }
}

/// order_for：与 next_power_of_two().trailing_zeros() 全域逐点一致 +
/// 浪费界（2^order ≥ npages 且 2^(order-1) < npages——最小阶）。
#[test]
fn order_for_minimality() {
    assert_eq!(order_for(1), 0);
    for npages in 1..=4096usize {
        let o = order_for(npages);
        // 最小性：2^o ≥ npages 且（npages>1 时）2^(o-1) < npages。
        assert!(1usize << o >= npages, "npages={npages} 2^{o} 不足");
        if npages > 1 {
            assert!(
                1usize << (o - 1) < npages,
                "npages={npages} 阶 {o} 非最小"
            );
        }
        // 与标准幂函数逐点一致。
        assert_eq!(
            o,
            npages.next_power_of_two().trailing_zeros() as usize,
            "npages={npages} 与 next_power_of_two 不一致"
        );
    }
    // 恰幂点直落：npages=2^k 时阶恰 k（无 +1）。
    for k in 0..16usize {
        assert_eq!(order_for(1 << k), k, "恰幂点 2^{k} 阶必恰 k");
    }
}

/// 端到端路由：alloc 面（size/align → Slab/Pages）+ alloc/dealloc 对称。
#[test]
fn route_end_to_end_and_symmetry() {
    // 零大小：Layout 非零不变量外的防御面 → None。
    assert_eq!(route(0, 1), None);
    // 小对象各形。
    assert_eq!(
        route(1, 1),
        Some(Route::Slab { class: 8, slots: 512 })
    );
    assert_eq!(
        route(9, 8),
        Some(Route::Slab { class: 16, slots: 256 })
    );
    assert_eq!(
        route(2048, 8),
        Some(Route::Slab {
            class: 2048,
            slots: 2
        })
    );
    // 恰出界 → 整页 1 页 order 0。
    assert_eq!(
        route(2049, 8),
        Some(Route::Pages {
            npages: 1,
            order: 0
        })
    );
    // 跨页：4097 → 2 页 order 1（2^1=2 ≥ 2）。
    assert_eq!(
        route(4097, 8),
        Some(Route::Pages {
            npages: 2,
            order: 1
        })
    );
    // 大对齐：size 1/align 4096 → 整页（slab 域外）。
    assert_eq!(
        route(1, 4096),
        Some(Route::Pages {
            npages: 1,
            order: 0
        })
    );
    // ktest A1-2 账面锚点复现：16B/页 × 10240 页 = 160KiB → order 6（64 页）；
    // 2B/页 × 10240 = 20KiB → order 3（8 页）。
    let np40 = 40 * 1024 * 1024 / PAGE_SIZE;
    let ledger = (16 * np40).div_ceil(PAGE_SIZE).next_power_of_two()
        + (2 * np40).div_ceil(PAGE_SIZE).next_power_of_two();
    assert_eq!(ledger, 72, "A1-2 记账双 Vec 72 页锚点");
    assert_eq!(
        route(16 * np40, 8),
        Some(Route::Pages {
            npages: 40,
            order: 6
        })
    );
    // 对称律：dealloc 与 alloc 同 (size,align) 必同路——全域抽样。
    for &(s, a) in &[
        (1usize, 1usize),
        (16, 16),
        (100, 8),
        (2048, 2048),
        (2049, 8),
        (5000, 4096),
        (16 * 10240, 8),
    ] {
        assert_eq!(
            dealloc_route(s, a),
            route(s, a),
            "对称破坏 at size={s} align={a}"
        );
    }
}

/// 路由两臂全域扫描：无 None 洞（除零大小）+ 分派单调性。
#[test]
fn route_full_domain_scan() {
    let mut slab_count = 0;
    for size in 1..=8192usize {
        let r = route(size, 1).expect("size ≥ 1 必有路由");
        match r {
            Route::Slab { class, slots } => {
                slab_count += 1;
                assert!(class >= size);
                assert_eq!(class * slots, PAGE_SIZE);
            }
            Route::Pages { npages, order } => {
                assert!(npages * PAGE_SIZE >= size);
                assert!(1usize << order >= npages);
            }
        }
    }
    // slab 域 = [1,2048] 恰 2048 个 size；整页域 = 2049..=8192。
    assert_eq!(slab_count, 2048, "slab 域分派计数");
    // 整页 npages 阶跃点：跨页边界 +1 页。
    assert_eq!(
        route(4096, 1),
        Some(Route::Pages {
            npages: 1,
            order: 0
        })
    );
    assert_eq!(
        route(4097, 1),
        Some(Route::Pages {
            npages: 2,
            order: 1
        })
    );
    // 2 幂页数恰直落（npages=4 → order 2 非 3）。
    assert_eq!(
        route(PAGE_SIZE * 4, 1),
        Some(Route::Pages {
            npages: 4,
            order: 2
        })
    );
}
