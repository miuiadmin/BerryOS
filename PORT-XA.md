# PORT-XA 改动清单（paint 面——ui-wave2 → 主树波 2 移植）

> 移植者 XA ｜ 2026-11-15 ｜ 依据 [RECON-PORT.md](./RECON-PORT.md) 第 1/2/3 项 ｜ 基线 = 本 worktree `wip/ui-wave2-port` @ 3272bbdeb。
> 文件面：`user/berry-gfx/src/{cmd,queue,raster}.rs` + `user/berry-ui/src/{paint,theme}.rs`，恰为指派五文件；tests/ 零改动（新测全部文件内联）。
> 验收门：`cargo test --manifest-path user/Cargo.toml -p berry-ui -p berry-gfx --target <host>` 基线 361 过零缺席（只增不减），终态 421 过 0 失败（含并行会话 pattern/gallery 新增 51 测与本面新增 9 测）。

## 一、圆角 LUT 原语 + Cmd::FillRectRounded（对账第 1 项）

gfx 三文件增量，整件照搬源实现（`../ui-wave2` 同名文件）：

- `berry-gfx/src/cmd.rs`：`Cmd::FillRectRounded { rect, radius, paint }` + `Cmd::PushClip { rect }` + `Cmd::PopClip` 三变体追加枚举尾部（Present 之后——add-only 不重排，TLV wire 面零新字段）；encoder 三法 `fill_rect_rounded` / `push_clip` / `pop_clip`；`rect_of` 扩臂（FillRectRounded/PushClip 携 rect 负宽高同拒 BadRect，PopClip 无矩形）；新测 `rounded_and_clip_cmd_rect_gates`。
- `berry-gfx/src/raster.rs`：`RADIUS_LUT_MAX=255` 帽 + `radius_row_inset` const 单源几何（`r − ⌊√(r²−(r−row)²)⌋` 牛顿迭代整数平方根）+ `fill_rect_rounded` 执行臂（半径三重钳 min(w,h)/2 / LUT 帽 / 敌意巨值；一次预计算两角复用；直角快速路不另起 LUT）；exec 分派两臂（PushClip/PopClip no-op——栈语义在回放环）。
- `berry-gfx/src/queue.rs`：`CLIP_STACK_MAX=8` 裁剪栈回放环（push = 当前 clip ∩ rect 压栈；空交 = 零矩形结构性 no-op；超帽 push 折 no-op 保守 + debug_assert loud；空栈 pop no-op 防御）；新测 `fill_rounded_row_lut_golden`（r=4 行内缩手算字面 golden + 半径钳 min(w,h)/2 逐字节对拍 + radius==0 与 FillRect 同义）与 `clip_stack_replay`（收窄/嵌套/空交/弹出还原 + dirty 基口叠加）。

golden 零漂移自证：`berry-gfx/tests/golden.rs` 本刀零触碰且与移植源 `diff -q` 逐字节同文；存量 `FillRect` 像素路径未动（FillRectRounded 为独立变体与执行臂），golden 三测全绿。

## 二、Chip/Card/Toggle 直角/胶囊近似升级真圆角（对账第 2 项）

- 消费面（`berry-ui/src/paint.rs`）：`fill_rounded` 共核（开档且 radius>0 → `fill_rect_rounded`，否则回落 `fill` 同义直角）；Toggle 轨 `capsule_h` → `FillRectRounded(r=h/2)` 真胶囊（滑块圆保持 oct3——弧族原语面 = 波 3+）；Chip 底 → r=`theme::OH_RADIUS_PX`（8vp @3x = 24px，raster 侧钳 min(w,h)/2，32 高自然折胶囊）；Card 消费 `p.corner_radius`（TLV 数据已在流里跑）——阴影条与卡面同半径圆角（角区阴影随面收口），视觉足迹仍恒在节点界内（damage 正确性优先纪律不变，双 fill 内缩几何原样）。
- 档位机制：进程级开关 `paint::set_true_corners(bool)` / `paint::true_corners()`（core AtomicBool），**默认 false = v1 近似档**。约束成因：主树既有测试断言零改动是硬门——`tests/widgets.rs:159-191` 逐值钉板 Toggle 胶囊三 fill 与 cmds.len()==16、`tests/widgets3.rs:730/883` 钉板 Chip/Card 直角 fill 序列，默认档翻圆角必破（源树的解法是删 widgets1-7，本移植面不许）；feature 门需改 Cargo.toml（面外）。opt-in 开关是五文件面内的唯一满足双约束形态；画廊波（路 3）与宿主可直接开档，默认档翻转挂后续刀（一行改动）。
- 遗留：Button r=8vp（RECON 可选项）未取——主树 Button 48px 高下 24px 半径退化全胶囊，与 Toggle 视觉同质，定档挂画廊波；Toggle 滑块真圆（`FillRectRounded(d,d,d/2)` 即真圆）同挂。
- 新测：`paint::tests::xa_legacy_default_no_rounded_cmds`（默认档零漂移锚——无 Rounded 命令 + 胶囊三 fill 几何 + Chip/Card 直角）、`paint::tests::xa_true_corners_rounded_toggle_chip_card`（命令面四条 Rounded 值级 + 像素面角区不写/中段照铺/阴影缘带）。

## 三、disabled 档扩展 PressedVis 查表机制（对账第 3 项）

- 机制（`berry-ui/src/paint.rs`）：`PressedVis` 扩三态槽表——新增 `disabled: [u32;32]` / `n_disabled` 字段（`empty()` 全零，lib.rs `pressed_snapshot` 现行字段写法零改动兼容）+ `is_disabled` / `state()` 查表 + `NodeVisState { Normal, Pressed, Disabled }` pub 枚举；优先级 disabled > pressed > normal（禁用件不表达按压反馈——源实现消息机 Disabled 锁死语义的 paint 面同律）。
- 首批 kind 按对账建议五件：Button（Filled 底+标签双换）/ Toggle（轨换禁用灰，滑块不变）/ Check（框+勾换弱化灰）/ Radio（环+内圆换弱化灰）/ Seg（active 段换禁用底）。Slider 与输入族挂后批（对账原文同序）。
- theme 面（`berry-ui/src/theme.rs`，add-only 零既有 token 语义改动）：`BG_DISABLED`（0xF2F2F2——OH disabled 组件面，与 pattern.rs `ItemState::Disabled` 值面同源）/ `FG_DISABLED`（0x9E9E9E）+ dark 镜像 `BG_DISABLED_DARK` / `FG_DISABLED_DARK` + 圆角 token `OH_RADIUS_VP=8` / `OH_RADIUS_PX=24`（px 直录避免 theme→pattern 反向依赖，与 `pattern::DENSITY` 的一致性由测试对拍钉板）；`Tokens`/`pick` 结构未动。新测 `disabled_tokens_paired_and_distinct` + `radius_token_px_pinned`。
- 新测：`paint::tests::xa_disabled_three_state_first_batch`（三态查表值级 + disabled 优先于 pressed + 五 kind 像素级禁用观感 + pressed/normal 双对照）。
- 写侧遗留（面外，挂 lib.rs 面）：`set_disabled` 写侧表、消息机 Disabled 锁死、hit 侧拒——源实现三处照搬面在本指派文件面（lib.rs/hit.rs）之外；本面已立机制与验收（内联单测直建 Node 树走 `paint()`，不依赖写侧 API）。

## 四、clip 最小面：Scroll/List 溢出裁剪（对账第 4 项——判有价值，做了）

对账判「移植价值高（现状溢出即视觉缺陷）」，最小面落地：

- gfx 侧：`PushClip`/`PopClip` 变体 + queue 裁剪栈（第一节已列）。
- paint 侧（`berry-ui/src/paint.rs`）：①`cull_walk` 的 `d` 参语义扩为有效窗 eff（= damage ∩ 祖先 clip 界——叶子可见区按 eff 判）；②clip 段标记——Scroll/List 子树段以 MARK_POP/子树/MARK_PUSH(界) 序入收集流，发射取逆恰成 `PUSH 子树 POP` 嵌套段（`MARK_PUSH`/`MARK_POP` 哨兵 = u32::MAX−2/−1，与槽值/NONE 不撞）；③`emit_walk_all` 回退走查 clip 段直接 Push/Pop 包夹（前序天然正确序）；④空窗子树整剪（不复发不标段）。damage.rs 零改（与源逐字节同文的预判成立）；遮挡集 `covered.add(vis)` 按 eff 收窄记录 = 保守多画不少画。
- 开关形态：**kind 即登记**（Scroll/List 走查即开裁剪）——「登记即开」的最小内嵌形态，无需 lib.rs `clip_on` 侧表；泛化 per-slot `set_clip` API 挂 lib.rs 面后续刀。主树既有测试无 Scroll/List paint 场景（widgets3 只断言 bounds；tlv3/tlv6 是解码面），既有断言零波及。
- 新测：`paint::tests::xa_scroll_list_clip_overflow`（Scroll 溢出子像素级裁到视口界 + PushClip/PopClip 命令 + cmds 计数 + 段外妹妹不受影响 + List 同律）。
- 口径注记：`drawn_px` 发射足迹仍按节点界 ∩ viewport 计（不扣 clip 窗）——overdraw 度量保守偏高，与源实现同口径。

## 五、验证与口径

- 基线对照：动手前全量 361 过存档（`/tmp/portxa-baseline.txt`），终态全量 `grep -c "^test .* ok$"` = 421、基线 361 项 `comm` 零缺席、0 失败——只增不减成立。增量 60 = 本面 9（gfx 3 + theme 2 + paint 4）+ 并行会话 51（pattern.rs 配方族 47 + gallery2 两测等，非本面）。
- 本面新增 9 测：`cmd::tests::rounded_and_clip_cmd_rect_gates`、`queue::tests::fill_rounded_row_lut_golden`、`queue::tests::clip_stack_replay`、`theme::tests::disabled_tokens_paired_and_distinct`、`theme::tests::radius_token_px_pinned`、`paint::tests::xa_legacy_default_no_rounded_cmds`、`paint::tests::xa_true_corners_rounded_toggle_chip_card`、`paint::tests::xa_disabled_three_state_first_batch`、`paint::tests::xa_scroll_list_clip_overflow`。
- `--features s7` 面：berry-ui lib 61 过 0 失败（s7 与默认双面均绿）。
- clippy：工具链 1.97.0-aarch64-apple-darwin 未装 cargo-clippy，未跑（cargo check/test 零本面警告）。
- 测试落点纪律：新测全部内联于五文件（`#[cfg(test)] mod tests`），不新增 tests/ 文件——「只改五文件 + PORT-XA.md」的最严读法。

## 六、并行会话交互注记

- 路 2（pattern.rs +2816 行）与路 3（gallery2.rs + WAVE2-GALLERY/）在本次移植进行中并行落地；其 pattern `refresh_*` 两测一度 BadParent 失败（lib.rs add 白名单面，非本面致因——本面未动 lib.rs），后由该会话自愈；gallery2 两测与本面改动共存绿。
- 一次共享 target 目录的构建指纹竞态（berry-ui 编译期看不到 gfx 新方法、源文件实存）：touch 重编即解，非代码缺陷；并行会话建议错峰跑 `cargo test` 或各自 `CARGO_TARGET_DIR`。

## 七、遗留面清单（后续刀）

- lib.rs/hit.rs 面：`set_disabled` 写侧表 + 消息机 Disabled 锁死 + hit 拒（源三处照搬）；per-slot `set_clip` 泛化 API（本面 kind 即登记已覆盖 Scroll/List 最小面）。
- 默认档翻转：真圆角默认开（一行）挂画廊波定档（Button/滑块圆角半径档同批裁决）。
- Slider/输入族 disabled 挂后批；弧族原语（Progress Circular/Check 勾/Radio 环真形）= 波 3+。
- Curia 笔记：本刀未写 `.agents/notes/`（指派文件面限定五文件 + PORT-XA.md；本清单即过程记录）——归档裁决留编排面。
