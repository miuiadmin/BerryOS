# 暗色刀验收报告（DARK-VERIFY）

验收对象 = 工作树未提交改动（基点 `3af18dda1`，分支 `wip/ui-dark`，HEAD 即基点——全部改动在工作树，`git diff 3af18dda1` 即实施面），实施清单见 [DARK-IMPL.md](DARK-IMPL.md)。验收员与实施者非同一会话，全程独立复跑：测试与构建用隔离 target `.target-d`，遮罩律为手算逐通道对拍，基线对照用临时 worktree。结论：**五门全过、全绿收讫、一处小修**（main.rs 一行尾随空白——修后重编过链）。

Updated: 2026-11-15

## ①豁免清零门

- 基线 `git show 3af18dda1:user/berry-ui/src/paint.rs` 源面（`#[cfg(test)]` 段前，基点在 1727 行）`Color { r:` 字面量恰 **30 处** = 29 个色板常量（77–248 行）+ 1 处 Image 臂黑色哨兵（756 行）。
- 工作树源面（测试段前，现 1686 行）恰 **1 处**（Image 哨兵，715 行）——**30 → 1，只减不增**，与 DARK-IMPL 声明一致。
- 豁免逐条有理由：台账注（paint.rs 色板迁出注）登记 #1 = Image 像素即色（ARGB 经 D28 icons 槽预混）、`fg` 形参对该臂是死实参非视觉色、无 token 语义可挂；使用点正上方携 `[paint-exempt]` ASCII 标记。相邻未动项双登记在案（lib.rs `PRESSED_BG` 0x404040 归 press 反馈面；gallery2 测试面 `INK` 0xFF1A1A1A 归宿主图标墨色——两文件本刀零触碰，登记属实）。
- grep 门独立手算：剥 `#[cfg(test)]` 后段 `Color { r:` 计数 1 == `[paint-exempt]` 计数 1，门断言（未登记即红、豁免数 == 1 增即红）与手算一致；`tests/dark.rs::paint_fixed_palette_exempt_gate` 复跑绿。
- 合法路径立法在案：新增固定色必须先在 theme.rs 立双档 token（台账注原文）。

## ②light 零漂移 + dark 档对拍

四道独立证据：

- **像素级**：`gallery2_golden_hashes` 复跑绿——43 格 FNV-1a 像素哈希对 golden 表逐格全等（含 kind_input 0xa9de573fc08899a1 现值）；`gallery2_dump_cells` 重写 `WAVE2-GALLERY/cells.tsv` 后 `git status WAVE2-GALLERY/` 零改动——**字节级零漂移**（cells.tsv 在库 tracked，重写产物与基线逐字节相同）。
- **值级**：`theme::tests::widget_palette_light_pinned_to_v1` 29 槽逐色钉死旧色板值（ACCENT 0x1976D2 … CARD_SHADOW 0xAAAAAA）复跑绿；widgets/scenes 族测试仅换导入路径（`berry_ui::paint::` → `berry_ui::theme::`，同名同值），断言语义不变全绿；仓内 grep 证 `paint::` 色板常量残留消费为零（tests 之外无引用）。
- **往返级**：`dark.rs::theme_mode_runtime_switch` 断言 dark 往返后 light 档命令流 Debug 全等——绿。
- **dark 档输出对拍**（探针色值）：dark.rs 以真实渲染 DisplayList 为面——light 档含 `TRACK_OFF` 0x9E9E9E 且无 dark 值混入；换 dark 后含 `TRACK_OFF_DARK` 0x5E5E63 与 dark accent 0x4791DB、light 值退场——绿。settings light 档 chrome = 波4 冻结常量逐字节（`CLEAR_BG`/`TEXT_FG`/`VALUE_FG` 直返）。

## ③brand_pressed dark 档区分 + 遮罩律

- 区分：`BRAND_PRESSED_DARK` 0x63B2FF ≠ `BRAND_DARK` 0x3D9FFF，`theme::tests::brand_pressed_distinct_dark` 绿（本刀核心断言）；light 档 `BRAND_PRESSED` 0x005CBC 原值不动（`brand_pressed_distinct_light` 绿）。
- 遮罩律手算（20% 白遮罩 `c+(255−c)/5` 下取整，逐通道）：BRAND_DARK (61,159,255) → r 61+194//5=99=0x63、g 159+96//5=178=0xB2、b 255+0//5=255=0xFF —— **逐通道精确命中** BRAND_PRESSED_DARK；且每通道 ≥ 常态（单调向白——暗底按压向白侧，方向自洽）。
- ACCENT 族同律复核：ACCENT 0x1976D2 → (71,145,219)=0x4791DB=ACCENT_DARK 精确命中；ACCENT_DARK → (107,167,226)=0x6BA7E2=ACCENT_PRESSED_DARK 精确命中；light 档对照 ACCENT×4/5 = (20,94,168)=0x145EA8=ACCENT_PRESSED 原值保持。`widget_palette_dark_differs_per_slot` 断 23 槽双档不同 + dark 档按压可区分——绿。

## ④settings 开关行为断言

- 开关行在案：第五行 `"Dark theme"` + Toggle（`EV_THEME=5`、checked 初值 = `theme_io::load()` 同源、a11y `ROLE_TOGGLE/"dark theme toggle"`）。
- 切换三面齐：①`ui.set_theme_mode(m)`（引擎色板随档换 + FULL damage）；②chrome 双档——`clear` 底随档换 + 7 个 chrome 文本节点（标题/五标签/亮度值，build 期 `reg_chrome` 入册）按族位 `set_props` 换 fg（纯 PAINT 帧）；③`theme_io::save(m)` 落盘；末尾 `wake`。幂等门 `m != mode` 与引擎面同档短路双设在案。
- 持久化面断言：theme_io 3 测（canonical 两形恰 10/11B、roundtrip、fail-closed 11 负例含双写拼接）——musl 链 `cargo test --no-run` 编译过链（host 执行结构性不可达 = layout_io 同注记，DARK-IMPL 诚实标注在案；引擎链路运行时行为由 dark.rs host 三测代偿）。三约束代码核对：缺席回默认不预写、坏件回默认 + canonical 重写自愈、写失败 eprintln 非致命；`fs_h()==0` 双门静默短路。
- manifest `fs = ["rootfs"]` 授能在案（APPDEMO fs 词先例同形）。

## ⑤纪律核

- **AnimProp 式 add-only 无涉**：`git diff 3af18dda1 --name-only` 恰 13 文件，全部属本刀域（berry-ui src 3 + tests 8 + settings 2）；`anim.rs`、`spring.rs`、`swipe.rs`、`gallery2.rs`、`tlv2..tlv7.rs`、`reconcile.rs` 零触碰（diff 行数 0 实证）。
- **TLV golden 零漂移**：tlv2 12 + tlv3 6 + tlv4 7 + tlv5 7（含 `golden_v5_stream_pinned_literal`）+ tlv6 7 + tlv7 7 + reconcile 12 复跑全绿。
- **主树脏文件零触碰**：主仓 worktree 脏面全为其他会话域（docs/BCTS、docs/显示管线、tools/berry/web、.agents plans/research、target 类产物），与本刀路径（user/berry-ui、user/bins/settings）零交集；本刀在专属 worktree `wip/ui-dark`。
- **公共签名零改**：`Ui::paint`/`render_frame` 公共签名块基线↔当前 diff 为空（tokens 为内部实参追加）。
- **s7 实验轨 1 红为基线同红**：`--features s7` 下 `widgets7::batch_f_set_props_reflects_paint` 红——开临时 worktree（纯 `3af18dda1`）复跑同红 + SPRING-IMPL.md 在案（s7 面既有问题；断言面 = measure 计数语义，与本刀色板/主题零交叠）。非本刀引入，不修。

## ⑥修与全量复跑

- 唯一红位 = `main.rs:548` 尾随空白（本刀引入行）——已修，settings musl release 重编过链。
- 主门 `cargo test --manifest-path user/Cargo.toml -p berry-ui --target aarch64-apple-darwin`（`CARGO_TARGET_DIR=.target-d`）：**334 测全绿 0 红**（unittests 64 + 集成 270；基线 331 + dark.rs 3），与 DARK-IMPL 声明一致。
- 构建面复跑全过：no_std guest（`cargo build -p berry-ui`，aarch64-unknown-none）；settings release musl（berry-ld + `-zmax-page-size=4096`）；berry-launcher release musl（berry-ui 另一消费面回归）；settings `cargo test --no-run` musl。
- 编译警告面零新增：全量警告恰 6 条且全在存量测试文件（inject4×3、widgets3/widgets5/widgets6 各 1，unused import/variable）；widgets5/6 的警告项（`INPUT_PLACEHOLDER`/`SELECT_FG`）经 diff 核对 = 仅导入路径换、标识符使用面未变，警告结构性先在。
