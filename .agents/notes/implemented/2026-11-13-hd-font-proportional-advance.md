# HD 字体比例 advance 三面落地(烘焙/图集/排布)+ launcher 换装

Status: implemented

## Problem

痛点⑥「英文字距大、字体效果差」的根子:Noto Sans CJK(比例字体)烘焙时 advance 算出即丢(tools/genhdfont.py 旧 :47-48 仅用于居中偏移),消费侧按 32px 等宽格排布——「i」真实 9px 塞 32px 格(格内空隙 72%),「Browser Settings」等宽排布 512px 而真实仅需 269px(膨胀 1.90 倍)。四环根因链与逐字符量化见 [03 顶栏字体触摸](../../../research/桌面UX审计/03-顶栏字体触摸-根因与修法.md) §1;本轮实施其 A 案三面(03 号 §1.3)+ 一个消费点切换。

## Decision

按 03 号 A 案「烘焙+图集先行+最小排布」三面推进,全部纯加法或零漂移改造:

1. 烘焙面(tools/genhdfont.py → user/berry-gfx/src/font_data_hd.rs):产物格式升 v2——尾部追加 `ADVANCE_HD: [u8; 95]`(每形 1B,`round(font.getlength())` 真实比例值)+ 头部 `FORMAT_VERSION: u16 = 2` 版本位;**只追加不改 v1 段**:位图 95×192B 黄金哈希 `57ec8c95…` 实测零漂移(重产后比对一致),advance 表首钉独立黄金哈希 `34cdbfb8…`。selftest 扩:advance 锚 9 形('i'=9/'W'=30/'m'=31 等,03 号 §1.1 表与宿主 PIL 重跑双源钉)+'Browser Settings' 累计 269 锚 + 拒面 4(缺段/截短/坏 token/越域值,mutate 限定段内文本防与位图段 token 串味)+ 贴边截断守卫(全 95 形 2×CELL_W 宽画布重渲染对比 ink 宽——03 号 §1.2-2 推断项「W/m 贴边截断」收口:实测零截断)。genfbfont.py 零改动:自绘等宽 art(设计宽 7 列)本就等宽语义,无 advance 可丢。
2. 图集面(user/berry-gfx/src/font.rs):`GlyphAtlas` trait 加 `fn advance(&self, ch: u8) -> u32`,**缺省实现 = `cell_w()`**——三档点阵(8x16/16x32/24x48)零覆写零漂移(单测全 256 字节面钉板);`FontAtlas32x48` 覆写读 `ADVANCE_HD`(域外字节回 cell_w,与 glyph None 空格语义成对)。位图几何不动——字形仍居中 32px 格内,只改推进量。
3. 排布面(user/berry-ui):paint.rs Text 发射从 `x = i × cell_w` 改逐字形累计 `atlas.advance(ch)`(三档下与旧式逐值恒等;HD 档恒比例——**渲染层推进由图集单源决定,与供件无关**);metrics.rs 新增 `ProportionalTextMetrics`(宽 = Σ advance,与 paint 同源;三档 ≡ Monospace 逐值恒等);set_text 的 s7 三态判据等长分支改经度量缝比对内在宽——等宽供件下等长 ⇒ 宽恒等,判据退化为 v1 行为(R2-8「等长字节串必等宽」钉板面不松),比例供件下等长不等宽("ii"≠"WW")走 GEOM 重测分支(03 号 §1.2-4 架构注记收口)。
4. 消费点切换(user/bins/berry-launcher/src/main.rs):唯一干净文件换装 `set_text_metrics(&ProportionalTextMetrics)`——big 模式(1080P)HD 状态栏/格 label 的测量与渲染同走真实比例;三档文本零漂移。desktop.rs/frameloop.rs 为 owner lane 在改(M 态),零触碰。

## Alternatives considered

- 烘焙段格式用「每形首字节带 advance」:否——FONT_HD 位图段字节必漂移(位图黄金哈希重钉、消费面索引全动);独立表追加段零漂移,旧消费面按 v1 解析零破坏。
- 改 FONT_SIZE 34→32 消贴边风险:否——advance 值与 03 号实测(269px)对不上且位图全面漂移;以宽画布截断守卫代替(实测 95 形零截断,'W'=30/'m'=31 贴边但在格内)。
- set_text 判据改「比对度量缓存宽」:不需要缓存——`tm.measure(old) == tm.measure(new)` 直接比对,等宽供件下两侧恒等(零漂移成立),少一个状态面。
- berry-ui 排布面加「比例模式开关位」(Text style 位或新 ViewKind):否——渲染/测量两开关会有四组合态、失配面更大;图集 advance 单源 + 供件配套换装只有两态且各自自洽。

## Consequences

- HD 档渲染从此无条件比例推进:若有未换装 ProportionalTextMetrics 的 HD 消费者,会呈现「测量等宽 512/渲染比例 269」失配形态(offscreen 测试留档对照)——现役 berry-ui 唯一 bin 消费者 launcher 已换装自洽;后续新 HD 消费者应随建随换装。
- launcher 时钟等长换文("12:00"→"13:05")在比例供件下宽可能变:自动走 GEOM 重测分支(正确行为;等宽档等长换文仍纯 PAINT,bench_s7 项 5 零漂移实证)。
- 黄金纪律:GOLDEN_SHA256 不变(如实记「位图零漂移」),GOLDEN_ADVANCE_SHA256 首钉;此后字源/字号变更两哈希连坐重钉。
- 不做面挂账(03 号 §1.3-4 原样有效):kerning(字符对级)、灰度抗锯齿(4-bit alpha 需 mono_blit→混合)、CJK 显示名——归 GUI-2 段④ 真字体刀。
- 验证面:CARGO_TARGET_DIR=/tmp/target-font cargo test -p berry-gfx -p berry-ui 全绿(默认与 --features s7 两口径);genhdfont --selftest 全绿;launcher/gfxdemo musl 交叉构建绿。

## Related

- 根因与修法源:[03 顶栏字体触摸](../../../research/桌面UX审计/03-顶栏字体触摸-根因与修法.md) §1(三面刀法)/§1.2-4(R2-8 注记)/§1.3(A 案)
- 实施文件:tools/genhdfont.py、user/berry-gfx/src/font_data_hd.rs(生成物)、user/berry-gfx/src/font.rs、user/berry-ui/src/paint.rs、user/berry-ui/src/metrics.rs、user/berry-ui/src/lib.rs、user/bins/berry-launcher/src/main.rs
- 测试钉板:berry-gfx font.rs `advance_default_and_hd_table`(缺省零漂移 + HD 锚 + 269 对拍);berry-ui tests/metrics.rs 比例供件 4 测(三档恒等/HD 真宽/槽换装/等长换文判据);tests/offscreen.rs `hd_proportional_glyph_positions_vs_monospace`(渲染对拍:等宽 512 形态手算留档 vs 比例 269 实证)
