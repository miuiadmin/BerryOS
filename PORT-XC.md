# PORT-XC 波 2 画廊移植改动清单（WAVE2-GALLERY → 主树）

> 画廊移植者 XC ｜ 2026-11-15 ｜ 任务：按 [RECON-PORT.md](RECON-PORT.md) 第 5 项，把 `../ui-wave2/WAVE2-GALLERY`（只读：make_gallery.py/probe_cells.py/cells.tsv/README + 源测试 `../ui-wave2/user/berry-ui/tests/gallery2.rs`）移植为主树形态。
> 基线：主 worktree `wip/ui-wave2-port` @ 3272bbdeb「41 ViewKind 全部通电」。本面文件全部为**新增**，未改任何 src 文件；与路 1（gfx/paint/theme）、路 2（pattern）文件面零交集。

## 一、新增文件（本面全部改动）

| 文件 | 内容 |
|---|---|
| `user/berry-ui/tests/gallery2.rs` | 画廊测试双轨（对账第 5 项定的落位——源先例 tests/ 内、src 外）：`gallery2_dump_cells`（43 格 PPM dump + cells.tsv）+ `gallery2_golden_hashes`（FNV-1a 64 位像素哈希钉板——首跑已回填，渲染回归即红）。约 1065 行，零第三方依赖 |
| `WAVE2-GALLERY/README.md` | 联络表说明（看什么/再生成/格索引/诚实标注——主树版重写） |
| `WAVE2-GALLERY/cells.tsv` | 格序/尺寸/像素哈希单源（43 格，测试再生） |
| `WAVE2-GALLERY/gallery.png` | 单张网格联络表 2384×3490（大 PNG 按仓内先例入仓——仓内 `user/bins/appstub/**.png` 等 PNG 资产先例；tools 拼自 ppm/） |
| `WAVE2-GALLERY/tools/make_gallery.py` | PNG 拼版（源工具照搬，头注改主树版） |
| `WAVE2-GALLERY/tools/probe_cells.py` | 像素级色证据探针（43/43 过——色值单源重写为主树 paint.rs v1 固定色板 + theme.rs light 档 + pattern.rs ItemState 值面） |
| `.gitignore`（追加一行） | `/WAVE2-GALLERY/ppm/`——PPM 中间产物不进提交（源同律） |

## 二、格子集（43 格——主树现实适配，非源照抄）

- **kind 格 39**：41 ViewKind 全量覆盖。Root = 每格视口（不可 add）；Col/Row/Pad = 每格内容锚（`cell_col`）+ `kind_layout` 专项格；余 36 kind 各至少一格（`kind_button` 含 Filled/Outlined/Text 三档；`kind_sbar` 含 H/V 双向；`kind_timepick` 含 24h/12h 档；`kind_gauge` 含 75%/30% 双值）。源画廊七件「裸锚无 paint 臂」格（Chip/Fab/Gauge/Qr/Card/Collapse/SwipeItem/DragReorder）主树全部显真视觉。
- **三态对比格**：`kind_button_states`——N 常态 / P 按住不抬（`set_press_map` + `PHASE_DOWN` 定格，paint 面 PressedVis 查表换 ACCENT_PRESSED——widgets3 先例）/ D 档（`set_disabled` 挂路 1 未合，现态同 N 渲染，格内说明字与 README 诚实标注）。
- **pattern 值面格 4**（主树 pattern.rs 现状 = 几何常量 + ItemState 色值表 + NavStack 纯数据——「全件族」按值面消费；源 23 配方族建树 fn 挂路 2）：`pat_topbar`（TOPBAR_H 168px 三段条）、`pat_bottomtabs`（BOTTOMTABS_H 144px 四席 + active 着色）、`pat_listitem_5state`（五态 `ItemState::bg()` 值面上屏——该色表此前零 paint 消费）、`pat_navstack`（push 三页 + `nav.top()` 标题值面 + set_offset 叠放）。

## 三、主树 API 适配差异（对源画廊）

1. 色板：源 dark 基线 `theme::{token,Token}` → 主树 light 档常量 `theme::{BG_PAGE,BRAND,...}`（无 token() 面）；格底 = BG_PAGE 0xF1F3F5。
2. 状态面：源 `set_style(StyleParams)`/`set_disabled` → 主树 `set_press_map(&[(btn,btn)])` + `push_pointer(PHASE_DOWN)` 按住定格（Button 压档消费唯一在 paint 查表）。
3. 输入面：源 `set_text`/`set_focused` → 主树 `set_input_text`（Input/Edit 双收）+ `set_focus`。
4. 建树现实（主树特有，格设计随之）：
   - Refresh 不在 add 容器白名单（子拒面）→ `kind_refresh` = 纯顶缘指示条格（offset 600 → 19px REFRESH_BAR）；
   - Tabs 指示条在节点底缘、子面板 y=48 铺到节点底必盖指示条（widgets3 cmd golden 的 z 序现实）→ 条用零高 helper 子定宽（420×0——meas 携宽零面积空绘）+ 内容区改兄弟 Stack 自管；
   - List place `_` 臂全幅叠放 → 行 tiling 经单一内容 Col 承载；
   - Scroll 无 clip（路 1 未合）→ offset 200 格的溢出由光栅纹理界截断，探针断言带 1/BRAND 与带 4/SUCCESS 缺席；
   - SwipeItem 叶无行面 → 两端缘带格；
   - Spacer v1 0×0 占位 → 两端块相邻（说明字标注 flex 分配挂后续刀）。
5. 图标供件：16×16 行位图五枚 + 24×24 缩略图一枚（Image res=9），墨色按 light 基线烘焙 0xFF1A1A1A（=FONT_PRIMARY 同值）。

## 四、生成与验证记录（全部可复现）

```bash
cd user && cargo test --target aarch64-apple-darwin -p berry-ui --test gallery2   # 双测绿
python3 WAVE2-GALLERY/tools/make_gallery.py    # gallery.png 2384x3490, 43 cells
python3 WAVE2-GALLERY/tools/probe_cells.py     # probe: 43/43 格色证据通过
```

- 首跑回填纪律已执行：dump 出 tsv → 回填 GOLDEN 43 项 → `gallery2_golden_hashes` 绿。
- Qr 完整性核验：21 模块 84×84 满幅（cmds 帽 256 未触）。
- 在途兼容核验：路 1/路 2 并行改动（本日已入工作树：gfx cmd/queue/raster + paint/theme/pattern +3455 行）后复跑画廊双测**仍绿**——现面渲染未漂移；该面属并行会话，本面未触碰。

## 五、验收轮待办（路 1/路 2 合入后重生成——README 诚实标注同源）

1. 圆角合入（FillRectRounded 消费 Chip/Card/ListItem 等）：重 dump → 回填 GOLDEN（视觉变更即红是设计行为）→ `probe_cells.py` 增圆角/直角判别（角点像素断言）。
2. disabled 合入（set_disabled + BG_DISABLED 0xF2F2F2——theme 已加 token）：`kind_button_states` D 格改真调用 `set_disabled`，探针加 BG_DISABLED 在场断言；先吃 disabled 的五件（Button/Toggle/Check/Radio/Seg）可增设同族三态行。
3. clip 合入：`kind_scroll` 改子区域视口格（现全幅格看不出 clip 差），探针加窗外哨兵色缺席断言。
4. 路 2 pattern 配方族落地：四个 pat 格可换 `pat::top_bar/bottom_tabs/list_item/...` 配方调用重建（值面格退役或并存——届时对账裁决）。
5. 每次重生成后：`cells.tsv`/`gallery.png`/GOLDEN 三面同批刷新（tsv 是格序单源，PNG 拼自 ppm/）。

## 六、纪律说明

- 本面只新增 `user/berry-ui/tests/gallery2.rs` + `WAVE2-GALLERY/**`（ppm/ 已 gitignore）+ `.gitignore` 一行；未改 berry-ui/berry-gfx 任何 src——与路 1/路 2 文件面零交集（对账第六节核验成立）。
- 并行会话在途文件（`user/berry-gfx/src/{cmd,queue,raster}.rs`、`user/berry-ui/src/{paint,pattern,theme}.rs`、`RECON-PORT.md`）一律未触碰；`RECON-PORT.md` 为对账员未跟踪产物。
- 提交时按显式路径：`git commit -- user/berry-ui/tests/gallery2.rs WAVE2-GALLERY .gitignore`。
