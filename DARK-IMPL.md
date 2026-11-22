# 暗色刀实施清单（维八三小刀）

状态：已完成历史——机制面已被 settings_io.rs（SETTINGS.CFG 五键，2026-11-16）取代，本文仅作暗色刀过程存档。
现行实现：[user/bins/settings/src/settings_io.rs](user/bins/settings/src/settings_io.rs)（取代面 = 本文的 theme_io.rs / SETTINGS.JSN 单键持久化，五键 canonical 含旧档迁移；引擎面 theme.rs 双档与 set_theme_mode 不在取代面、仍现行）。

设计一句话：给 berry-ui 补「主题档运行态」——引擎 `Ui` 持档 + `set_theme_mode` 换档即 FULL damage 重画，paint 的「组件 v1 固定色板」29 色整块迁 theme.rs `WidgetColors` 双档组（light 档逐色同值零漂移），dark 档逐色给对应值并顺手修 brand_pressed 暗档同值问题，settings 加暗色开关行（切换即时生效 + SETTINGS.JSN 单簇持久化）。

依据：`research/UI-02-六大系统UI差距深研与裁决-2026-11.md` 维八裁决（根因 = 工程量缺为主；三小刀 = paint 色板豁免清零改查 token、settings 暗色开关、dark 档 brand_pressed 同值顺手修；动态取色明示不做，与 OH 同立场）与 `.agents/research/2026-11-15-UI现状十四维盘点.md` 维八现状（token 双档在册、无切换入口、paint 不查主题档）。基点 `3af18dda1`，分支 `wip/ui-dark`。

## 改动清单

- `user/berry-ui/src/theme.rs`（+419/-17）
  - **补档**：`BRAND_PRESSED_DARK`（新——旧档与 `BRAND_DARK` 同值未区分，今按 20% 白遮罩律 `c+(255−c)/5` 给 0x63B2FF；light 档 0x005CBC 原值不动）与 `SUCCESS_DARK`（0x3FC388——补齐 dark 档 11→12 token 镜像）。
  - **组件色板迁入**：原 paint.rs 固定色板 29 色 light 常量同值迁入（ACCENT/ACCENT_PRESSED/TRACK_OFF/TRACK_DIM/KNOB/OUTLINE/ON_ACCENT/ON_SURFACE/TABS_BG/TABS_INDICATOR/SBAR_TRACK/SBAR_THUMB/BADGE_BG/ON_BADGE/SELECT_BG/SELECT_BORDER/SELECT_FG/INPUT_BG/INPUT_BORDER/INPUT_FOCUS_RING/INPUT_PLACEHOLDER/CANVAS_BG/REFRESH_BAR/SWIPE_ACTION_BG/PICK_BG/PICK_BORDER/TIP_BG/CARD_BG/CARD_SHADOW——每常量 doc 带「旧值 = paint.rs XXX 0x……」对照行）+ 29 个 `_DARK` 镜像（取值律：能对上既有 dark 语义常量直取——组件面底 → `BG_COMP_DARK` 0x252628 系、次级灰 → `FONT_SECONDARY_DARK` 0x999999 系、警示红 → `WARNING_DARK` 0xF56C5B；ACCENT 族 Material 蓝族内按白遮罩律提亮 0x1976D2→0x4791DB、按压 0x6BA7E2；余为文档化近似）。
  - **`WidgetColors` 29 槽组** + `WIDGETS_LIGHT`/`WIDGETS_DARK` 整组常量；`Tokens` 增 `w: WidgetColors` + `bg_disabled`/`fg_disabled`/`success` 三槽，`Tokens::pick` 单点按档填——dark 臂 `brand_pressed` 自 `BRAND_DARK` 改指 `BRAND_PRESSED_DARK`（同值缺陷的修复点）。
  - 内联测试 +4：`brand_pressed_distinct_dark`（暗档按压可区分——本刀核心断言）、`widget_palette_light_pinned_to_v1`（29 槽 light 值逐色钉死旧色板值——零漂移值级第二道门）、`widget_palette_dark_differs_per_slot`（23 槽双档不同，防「dark 照抄 light」回归；ON_ACCENT/ON_BADGE 双档同白为合法同值不在断言列）、`disabled_tokens_paired_and_distinct` 扩 Tokens 面随行断言；`two_themes_distinct_per_token` 扩 success。
- `user/berry-ui/src/lib.rs`（+23）
  - `Ui` 增 `theme: Theme` 运行态字段（默认 Light——旧行为零漂移）；`set_theme_mode`（换档置 FULL damage，`set_viewport` 同形；幂等——同档重设零脏帧）+ `theme_mode` 读面；`Ui::paint` 与 `render_frame` 内部两处 `paint::paint` 调用追加 `&Tokens::pick(self.theme)` 实参（公共签名零改）。
- `user/berry-ui/src/paint.rs`（+178/-206）
  - **固定色板块退役**：29 个 pub 色常量删除，原位留「迁移台账 + 豁免清单」注（见下节）；组件发射 124 处取色点全部改 `tokens.w.*` / `tokens.bg_disabled`/`tokens.fg_disabled`（原 `crate::theme::BG_DISABLED`/`FG_DISABLED` 直引常量改为按档取——disabled 档随主题翻）。
  - `paint`/`emit_walk_all`/`emit_node` 三签名追加 `tokens: &crate::theme::Tokens`（cull_walk 纯收集不穿）。
  - 内联测试的 `render` 固定传 light 档 `Tokens`（迁移后 light 同值——本面即零漂移自证）；三处测试函数加 `tokens()` light 档绑定。
- `user/berry-ui/tests/dark.rs`（新，3 测）
  - `theme_mode_runtime_switch`：默认 Light（运行态锚）→ light 命令流含旧 `TRACK_OFF` 值、无 dark 值混入；`set_theme_mode(Dark)` 后档位翻转 + `damage().full` 在案 + dark token 入命令流、light 值退场；同档重设幂等零脏帧；切回 Light 命令流与首帧 Debug 全等（dark 往返零漂移）。
  - `theme_mode_switch_before_first_frame`：空树换档防御（不炸、档位照翻）。
  - `paint_fixed_palette_exempt_gate`：豁免 grep 门（见下节）。
- `user/berry-ui/tests/{widgets,widgets2..7,scenes}.rs`（各 ±1-3 行）：色板常量导入路径 `berry_ui::paint::` → `berry_ui::theme::`（同名同值——值级对拍断言语义不变）。
- `user/bins/settings/src/theme_io.rs`（新，模块 + 3 测）
  - SETTINGS.JSN 持久化面 = launcher `layout_io.rs` canonical 单簇原子写先例同形：FAT12 卷根 8.3 大写、≤2048B 单簇、open(O_WRONLY|O_CREAT) → write 循环 → **close 即 commit**（TRUNC 游标，覆写幂等）、严格串行单 fd 会话。canonical 形恰两态 `{"dark":0}`/`{"dark":1}`（10/11B）；`parse` fail-closed 只认两形全字节精确匹配。三约束：缺席回默认 Light 不预写（与 LAYOUT.JSN「缺席也写」的差异 = 主题默认值无需物化，首个用户切换才落盘）；坏件回默认 + canonical 重写自愈；写失败非致命 eprintln。`fs_h()==0` 装配 load/save 双门静默短路（零 RPC 零噪——layout_io 冻结相先例；此时退化为内存态开关）。
  - 测试 3 件（canonical 两形恰 10/11B、roundtrip、fail-closed 负例 11 条含双写拼接）。
- `user/bins/settings/src/main.rs`（+162/-36）
  - **暗色开关行**（第五行）：`"Dark theme"` 标签 + Toggle（`on_change: EV_THEME=5`，checked 初值 = theme_io 装载档同源）+ a11y `ROLE_TOGGLE/"dark theme toggle"`；消费 pattern/theme 冻结名（`Theme`/`Tokens::pick`/`set_theme_mode`——不发明新词汇）。
  - **切换 = 即时生效三面**：①引擎面 `set_theme_mode`（色板随档换 + FULL damage 全帧重画）；②app chrome 面 = `chrome(Theme)` 双档（light 臂返回波4 冻结常量 `CLEAR_BG`/`TEXT_FG`/`VALUE_FG` 逐字节保持——light 零漂移；dark 臂 `Tokens::pick(Dark)` 直取 bg_page/font_primary/font_secondary——「Tokens 接进 settings 消费」的落点），clear 底随档换 + 全部 chrome 文本（标题 + 五标签 + 亮度值，建树期 `reg_chrome` 入册 7 节点）按族位 `set_props` 换 fg（纯 PAINT 帧零重排）；③`theme_io::save` 落盘。
  - 启动面：`theme_io::load()` → `build(ui, dark)`（Toggle 初值与 chrome 取档一源）→ `set_theme_mode`（首帧即按档）。
- `user/bins/settings/manifest.toml`（+4）：`[capabilities]` 增 `fs = ["rootfs"]`（APPDEMO fs 词先例同形——svcmgr app_launch 授 fsmgr 端，`fs_h()` 消费；槽位律 wants_fs=[0]/[1] 双端不动、wm/input 尾追 [2]/[3] 不扰）。

## 豁免清单终态（paint.rs——豁免数只减不能增）

迁移后 paint.rs 源面（测试模块前）`Color` 字面量恰 1 处，豁免 1 项（基线 29 项 → 1 项，净减 28）：

1. **emit_node Image 臂黑色哨兵实参**（`Color { r: 0, g: 0, b: 0 }`）：Image 像素即色（ARGB 经 D28 icons 槽预混），`fg` 形参对该臂是死实参非视觉色——无 token 语义可挂；Icon 臂的 `p.fg` 是 props 数据面（app 给值）非本文件色。使用点携 `[paint-exempt]` ASCII 标记登记。

grep 门 = `tests/dark.rs::paint_fixed_palette_exempt_gate`：`include_str!("../src/paint.rs")` 剥 `#[cfg(test)]` 后段，断言 `Color { r:` 字面量数 == `[paint-exempt]` 标记数（未登记色即红）且豁免数 == 1（增即红）。台账注释写明新增固定色的合法路径 = 先在 theme.rs 立双档 token。

相邻未动项（非 paint.rs 域，登记备查）：lib.rs `PRESSED_BG` 0x404040（press_map 短路径换底，自有文档化豁免注——归 press 反馈面非色板面，本刀不碰）；gallery2 测试面 `INK` 0xFF1A1A1A（宿主图标预混墨色，值源 theme::FONT_PRIMARY 同步维护注在案）。

## light 档零漂移自证

- **像素级**：gallery2 43 格 FNV-1a golden 像素哈希零改动全绿（`gallery2_golden_hashes ... ok`）；`gallery2_dump_cells` 重写产物 `WAVE2-GALLERY/cells.tsv` 与 ppm 对基线 `git diff` 零漂移。本刀未重生成任何画廊格（「格子因 dark 支持重生成须自证」的前件不成立——dark 支持全部走运行态换档，格内容不变）。
- **值级**：theme.rs `widget_palette_light_pinned_to_v1` 29 槽逐色钉死旧值；paint.rs 内联测试恒 light 档全绿；widgets/scenes 族值级对拍断言（导入路径换、断言语义不变）全绿。
- **往返级**：dark.rs `theme_mode_runtime_switch` 断言 dark 往返后 light 命令流与首帧全等。
- settings light 档观感 = 波4 冻结值（chrome light 臂逐字节原常量；引擎 light 档同值）。

## 构建自验（CARGO_TARGET_DIR 隔离于 `.dsh-worktrees/ui-dark/.target-d`）

- 主门 `cargo test --manifest-path user/Cargo.toml -p berry-ui --target aarch64-apple-darwin`：**334 测全绿 0 红**（基线 331 + dark.rs 新 3；unittests 含 theme 新 4 测）。
- s7 实验轨 `--features s7`：全绿除 `widgets7::batch_f_set_props_reflects_paint` 1 红——基线同红（SPRING-IMPL.md 在案：s7 面既有问题非本刀引入；断言面 = 纯绘值变的 measure 计数语义，与本刀色板/主题零交叠）。
- no_std guest 面：`cargo build --manifest-path user/Cargo.toml -p berry-ui`（workspace 默认 aarch64-unknown-none）通过。
- musl 应用链（`--target aarch64-unknown-linux-musl` + berry-ld + `-zmax-page-size=4096`）：settings release 构建通过；berry-launcher release 构建通过（berry-ui 另一消费面回归）；settings `cargo test --no-run` 编译过链。
- 编译警告：berry-ui 面零新增（存量 widgets3/widgets5/widgets6/inject4 unused import 基线即有——widgets5 的 `INPUT_PLACEHOLDER`/`SELECT_FG` unused 经基线对照确认先在）。

## 诚实标注

- **settings 单测执行面**：settings 为 berry-libc musl 独立小工作区，宿主 `cargo test` 结构性不可达（layout_io.rs 同注记）；本机无 qemu-user，3 件 theme_io 测试只过编译门（`cargo test --no-run` 过链）——执行挂 guest/CI 侧。开关-引擎链路的运行时行为由 berry-ui 面 dark.rs 三测在 host 全绿代偿（换档/FULL damage/幂等/往返零漂移）。
- **无 fs 装配退化**：manifest 无 fs 词的装配（或冻结相）下 load/save 双门短路，暗色开关退化为内存态（重启回默认 Light）——theme_io 头注与本清单双登记。
- **按压遮罩律暗档方向的证据等级**：OH 官方页（ux-design/visual-colors.md，本会话 web_fetch 直读）只立法「同语义色深浅双档换值」（ohos_id_color_activated 例），未给暗档按压遮罩方向；本刀取 20% 白遮罩（`c+(255−c)/5`）= light 档黑遮罩律（`c×4/5`，ACCENT_PRESSED 先例）的对称对应（暗底上向黑不可辨；Material/iOS 暗色档同向）——theme.rs 头注文档化近似声明在案，值非 wire 契约，测试只钉「可区分」不钉具体值。
- **settings 装配链未接线**：本基点 justfile 的 app 装配循环仍用 appstub 平像打 SETTINGS 包（真 settings ELF 的接线归波4 wiring 面）——本刀交付 crate/manifest 层，装配线随波4 收口。
- **暗色档观感未过人眼**：dark 值表是文档化近似（OH dark 族对齐 + 遮罩律派生），无截图验收面；首帧暗色观感的调整空间在 theme.rs `_DARK` 常量，值非契约可迭代。
