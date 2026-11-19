//! LRU 组（段② `lru` 模块）：定容 N 槽 stamp-clock 逐出的**语义序**验收
//! ——命中提升 / 未命中不动 / 覆写刷新不逐出 / 满表逐出最久未用 / 容量
//! 1 与容量 0 边界 / clone 保序，全走 pub API（`put`/`get` 的值级断言，
//! 逐出载荷 `(K, V)` 逐项对拍）。
//!
//! 与 `src/lru.rs` 内嵌单元测试的分立价值：这里验**多轮交错的完整序列**
//! （内嵌单测每条只走两三步）+ `icon::IconLruCache` 包装面的组合行为
//! （capacity/Default/clear 与核同拍）——「语义在长序列下仍成立」是
//! stamp-clock 实现唯一会漂的地方（逐出 victim 选错只在序列中段显形）。
//!
//! 契约注记（deviations 详报）：本仓实现无 `get_mut`（Cell 内部可变性
//! 使 `get` 即碰热、`&self` 单口——`get_mut` 无独立语义面）；N=0 的
//! `put` 返 `None`（vacuous——无处可放也无可逐），非「逐出自身返
//! `Some((k,v))`」形态。

use berry_gfx::icon::{IconAtlas, IconId, IconLruCache};
use berry_gfx::lru::LruCache;

// —— 命中提升序（LRU 的定义性行为）：A B 碰 A → 插 C 逐 B ——
#[test]
fn hit_promotes_eviction_order() {
    let c: LruCache<2, u8, u8> = LruCache::new();
    assert_eq!(c.put(1, 10), None, "空槽");
    assert_eq!(c.put(2, 20), None);
    assert_eq!(c.get(&1), Some(10), "命中 A（碰热）");
    // A 已最新 → 满表插入逐 B；若 get 不碰热则逐 A——两分支可分辨。
    assert_eq!(c.put(3, 30), Some((2, 20)), "逐出最久未用的 B");
    assert_eq!(c.get(&1), Some(10), "A 幸存");
    assert_eq!(c.get(&2), None, "B 已逐");
    assert_eq!(c.get(&3), Some(30));
    assert_eq!(c.len(), 2, "容量钉死");
}

// —— 未命中不动任何槽：miss 后的逐出序与 miss 前完全一致 ——
//
// 咬住什么：miss 路径若误刷新（如把扫描中撞到的槽重盖章），LRU 序被
// 查询行为污染——本测用「hit A + miss C + 插 D」三步把受害者唯一钉死。
#[test]
fn miss_does_not_disturb_order() {
    let c: LruCache<2, u8, &str> = LruCache::new();
    c.put(1, "a");
    c.put(2, "b");
    assert_eq!(c.get(&1), Some("a"), "hit A（A 最新、B 最旧）");
    assert_eq!(c.get(&9), None, "miss（不动任何槽）");
    // 若 miss 误把 B 刷新为最新 → 此处逐 A；正确实现逐 B。
    assert_eq!(c.put(4, "d"), Some((2, "b")), "miss 不改变逐出序");
    assert_eq!(c.get(&1), Some("a"));
    assert_eq!(c.get(&4), Some("d"));
}

// —— 覆写不逐出 + 代刷新：覆写后的键成了最新（返 None 不带逐出载荷）——
#[test]
fn overwrite_refreshes_without_evicting() {
    let c: LruCache<2, u8, u8> = LruCache::new();
    c.put(1, 10);
    c.put(2, 20);
    assert_eq!(c.put(1, 11), None, "同键覆写：不逐出、不占新槽");
    assert_eq!(c.len(), 2);
    assert_eq!(c.get(&1), Some(11), "覆写后取新值");
    // 覆写即刷新：键 1 最新 → 插 3 逐 2（「替换后再插入验证它成了最新」）。
    assert_eq!(c.put(3, 30), Some((2, 20)), "逐出的是未刷新的键 2");
    assert_eq!(c.get(&1), Some(11), "覆写键幸存（它已是最新）");
}

// —— 长序列交错（容量 3）：多轮 put/get 后逐出 victim 仍严格 = 戳最小 ——
#[test]
fn interleaved_sequence_keeps_strict_lru() {
    let c: LruCache<3, u8, u16> = LruCache::new();
    c.put(1, 100);
    c.put(2, 200);
    c.put(3, 300);
    assert_eq!(c.put(4, 400), Some((1, 100)), "满表首逐 = 最旧 1");
    // 现存 {2,3,4}；碰热 2 → 插 5 逐 3。
    assert_eq!(c.get(&2), Some(200));
    assert_eq!(c.put(5, 500), Some((3, 300)), "碰热 2 后逐 3");
    // 现存 {2,4,5}；碰热 4、碰热 2 → 插 6 逐 5。
    assert_eq!(c.get(&4), Some(400));
    assert_eq!(c.get(&2), Some(200));
    assert_eq!(c.put(6, 600), Some((5, 500)), "双碰热后逐 5");
    // 终态四键判别：2/4/6 活、1/3/5 亡。
    assert_eq!(c.get(&2), Some(200));
    assert_eq!(c.get(&4), Some(400));
    assert_eq!(c.get(&6), Some(600));
    assert_eq!(c.get(&1), None);
    assert_eq!(c.get(&3), None);
    assert_eq!(c.get(&5), None);
    assert_eq!(c.len(), 3);
}

// —— 容量 1 边界：单槽即满表——每一次新键插入都是逐出 ——
#[test]
fn capacity_one_boundary() {
    let c: LruCache<1, u8, u8> = LruCache::new();
    assert_eq!(c.capacity(), 1);
    assert_eq!(c.put(1, 10), None, "首插走空槽");
    assert_eq!(c.len(), 1);
    assert_eq!(c.get(&1), Some(10));
    assert_eq!(c.put(2, 20), Some((1, 10)), "单槽满表：新键即逐旧键");
    assert_eq!(c.get(&1), None);
    assert_eq!(c.get(&2), Some(20));
    // 同键覆写在单槽下同样不逐出（值更新走原槽）。
    assert_eq!(c.put(2, 22), None);
    assert_eq!(c.get(&2), Some(22));
    assert_eq!(c.len(), 1);
}

// —— 容量 0 文档化行为：vacuous——无槽可放、无旧可逐、恒未命中 ——
//
// 契约注记：原契约草案写「放入即逐出自身（返 Some((k,v))）」；本仓
// 实现取 `None`（put 头注：无处可放 = vacuous 成功，不 panic）。两种
// 形态都「不 panic」，值级断言钉死的是**实现所选语义**。
#[test]
fn capacity_zero_documented_vacuous() {
    let c: LruCache<0, u8, u8> = LruCache::new();
    assert_eq!(c.capacity(), 0);
    assert!(c.is_empty());
    assert_eq!(c.len(), 0);
    assert_eq!(c.get(&1), None, "零槽恒未命中");
    assert_eq!(c.put(1, 10), None, "零槽 put = vacuous（非 Some 自逐）");
    assert_eq!(c.len(), 0, "零槽恒空");
    assert!(c.is_empty());
}

// —— len/is_empty 全程转移 + clear 清空后空槽复用（零逐出）——
#[test]
fn len_is_empty_and_clear_cycle() {
    let c: LruCache<3, u8, u8> = LruCache::new();
    assert!(c.is_empty() && c.len() == 0);
    c.put(1, 1);
    assert!(!c.is_empty() && c.len() == 1);
    c.put(2, 2);
    c.put(3, 3);
    assert_eq!(c.len(), 3, "满帽");
    c.put(1, 9); // 覆写不改 len。
    assert_eq!(c.len(), 3);
    c.put(4, 4); // 逐出不改 len。
    assert_eq!(c.len(), 3);
    c.clear();
    assert!(c.is_empty() && c.len() == 0);
    assert_eq!(c.get(&1), None, "清空后全 miss");
    assert_eq!(c.put(5, 5), None, "清空后插入走空槽（零逐出）");
    assert_eq!(c.get(&5), Some(5));
}

// —— clone 保序：戳表随时钟一起拷贝——副本上的逐出序与原本一致 ——
#[test]
fn clone_preserves_eviction_order() {
    let c: LruCache<2, u8, u8> = LruCache::new();
    c.put(1, 10);
    c.put(2, 20);
    assert_eq!(c.get(&1), Some(10), "碰热 1（1 最新、2 最旧）");
    let d = c.clone();
    // 两份各自满表插入 → 逐出 victim 必须同为 2（拷贝不含「戳重置」类
    // 的静默语义变化——否则副本退化为 FIFO 逐 1）。
    assert_eq!(c.put(3, 30), Some((2, 20)), "原本逐 2");
    assert_eq!(d.put(3, 30), Some((2, 20)), "副本同逐 2（序随拷贝）");
    assert_eq!(c.get(&1), Some(10));
    assert_eq!(d.get(&1), Some(10));
}

// —— IconLruCache 包装面：capacity/Default/clear/trait 口与核同拍 ——
#[test]
fn icon_lru_wrapper_face_matches_core() {
    const A: [u8; 2] = [0b1000_0000, 0b0100_0000];
    const B: [u8; 2] = [0b1100_0000, 0b1100_0000];
    const C: [u8; 2] = [0b0000_0000, 0b1000_0000];
    let a = berry_gfx::font::GlyphBits::new(9, 1, &A).unwrap();
    let b = berry_gfx::font::GlyphBits::new(9, 1, &B).unwrap();
    let c = berry_gfx::font::GlyphBits::new(9, 1, &C).unwrap();
    // Default 与 new 同形（空表同容量）。
    let cache: IconLruCache<3> = IconLruCache::default();
    assert_eq!(cache.capacity(), 3);
    assert!(cache.is_empty());
    // 装载 → 查询往返（roundtrip）：位图三事实（几何 + 行字节）原样取回。
    assert_eq!(cache.put(IconId(10), a), None);
    assert_eq!(cache.put(IconId(20), b), None);
    assert_eq!(cache.len(), 2);
    let got = cache.get(&IconId(10)).unwrap();
    assert_eq!((got.w, got.h), (9, 1));
    assert_eq!(got.rows, &A[..], "行字节原样往返");
    // trait 口（&dyn IconAtlas——命令内嵌的消费路径）与便捷口同效。
    let atlas: &dyn IconAtlas = &cache;
    assert!(atlas.icon(IconId(20)).is_some());
    assert!(atlas.icon(IconId(99)).is_none(), "缺图标 None");
    // 满表逐出：碰热 10 后插 30 → 逐 20（包装不改变核语义）。
    assert_eq!(cache.get(&IconId(10)), Some(a));
    assert_eq!(cache.put(IconId(30), c), None, "尚有空槽");
    let ev = cache.put(IconId(40), c).unwrap();
    assert_eq!(ev.0, IconId(20), "逐出最久未用 20");
    assert!(cache.get(&IconId(20)).is_none());
    // clear → 空表 → 空槽复用。
    cache.clear();
    assert!(cache.is_empty());
    assert_eq!(cache.put(IconId(50), a), None, "清空后零逐出");
    assert!(cache.get(&IconId(10)).is_none());
}
