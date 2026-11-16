# WAVE2-GALLERY 波 2 组件画廊（主树版 host 渲染联络表）

> 2026-11-15 ｜ 画廊移植者 XC 自 `../ui-wave2/WAVE2-GALLERY`（只读）移植为主树形态——依据 [../RECON-PORT.md](../RECON-PORT.md) 第 5 项；主树现实 = 41 ViewKind 全部通电（3272bbdeb）。通道：`Ui::paint` → `CommandEncoder` → `Queue::submit` → RGBx → PPM（测试面 [../user/berry-ui/tests/gallery2.rs](../user/berry-ui/tests/gallery2.rs)）→ 本目录 PNG（host 渲染路线——gfxdemo 本体零改动）。
> 43 格 = 接线 kind 39 格（41 kind 全量——Root 为每格视口、Col/Row/Pad 为每格锚 + 专项格；含 Button N/P/D 三态对比格）+ pattern.rs 值面 4 格（TopBar/BottomTabs/ListItem 五态/NavStack——主树 pattern 现状即几何常量 + 色值表 + 纯数据页栈，23 配方族挂路 2）。

## 看什么

- [gallery.png](gallery.png)——单张网格联络表（每格含标签行；宿主工具 `tools/make_gallery.py` 拼自 `ppm/`）。
- [cells.tsv](cells.tsv)——格序/尺寸/FNV-1a 64 位像素哈希单源（43 格）。
- `ppm/`——每格 P6 PPM（中间产物，不进提交；由测试再生）。
- `tools/probe_cells.py`——像素级色证据探针（43/43 过——paint.rs v1 固定色板 + theme.rs light 档 + pattern.rs ItemState 值面关键色在场/缺席断言，零目检依赖）。

## 再生成

```bash
cd user && CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$HOME/.cargo-target} \
  cargo test --target aarch64-apple-darwin -p berry-ui --test gallery2
python3 WAVE2-GALLERY/tools/make_gallery.py
python3 WAVE2-GALLERY/tools/probe_cells.py
```

像素哈希 golden 双轨：`gallery2_dump_cells`（dump + tsv）与 `gallery2_golden_hashes`（43 格哈希钉板——渲染回归即红）同文件内联；视觉或引擎渲染变更须首跑回填 GOLDEN 表（先 dump 拿新值再钉）。

## 格索引（43）

| 格族 | 格 |
|---|---|
| 批 2a kind | kind_button（三档）、kind_button_states（N/P/D 三态）、kind_toggle、kind_slider、kind_progress（Linear×3+Indet+Circular）、kind_tabs、kind_sbar（H+V） |
| 批 2b kind | kind_scroll（offset 200）、kind_list、kind_check、kind_radio、kind_seg、kind_badge、kind_divider、kind_spacer |
| 批 2c kind | kind_stack、kind_step、kind_select |
| 批 2d kind | kind_input（占位+焦点光标）、kind_edit、kind_canvas |
| 批 2e kind | kind_tip、kind_refresh、kind_swipeitem、kind_datepick、kind_timepick |
| 批 F/全通 kind | kind_collapse、kind_chip、kind_fab、kind_card、kind_gauge、kind_qr、kind_dragreorder、kind_image、kind_icon、kind_grid、kind_text、kind_rect、kind_layout |
| pattern 值面 | pat_topbar（TOPBAR_H 168px）、pat_bottomtabs（BOTTOMTABS_H 144px）、pat_listitem_5state（LISTITEM_H×ItemState 五态）、pat_navstack（NavStack 三页叠放） |

## 诚实标注（现态在图上的呈现——对账路 1/路 2 合入后验收轮重生成）

- 源画廊七件「裸锚无 paint 臂」格（Chip/Fab/Gauge/Qr/Card/Collapse/SwipeItem/DragReorder）在主树全部有真视觉——本版无该挂账。
- 路 1 未合现态：圆角全直角（Chip 胶囊/Card corner_radius=24 数据携载/ListItem 半径常量未消费）；disabled 档未实现（kind_button_states 的 D 格现态同 N 渲染）；Scroll 无 clip（溢出直出，视口缘由光栅纹理界截断）。
- 路 2 未合现态：pattern 格为值面消费（几何常量/色值表/纯数据页栈），源 23 配方族建树 fn 落地后 pat 格可换配方调用重建。
- v1 视觉面（引擎既有挂账，非本画廊引入）：Progress Circular/Indeterminate 统一线性渲染；Edit 单 run 渲染；Toggle/Chip 胶囊直角近似；Tabs 指示条在节点底缘（子面板必盖——消费面以无面板条 + 兄弟内容区组合，见测试头注）；Refresh 子拒面（纯顶缘指示条格）；SwipeItem 叶无行面（两端缘带格）；Spacer v1 0×0 占位（flex 分配挂后续布局刀）；TimePick 12h 档 v1 原值 HH:MM 渲染；Qr 为确定性伪随机图案（真编码挂后续刀）。
