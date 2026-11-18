# BerryUI AppBar D3（现代导航与卡片列表）

> 版本：**1.0**｜2026-11-13（2026-11-18 已被 [iOS视觉与流体交互复刻标准-v1.0.md](iOS视觉与流体交互复刻标准-v1.0.md) 双态折叠大标题导航栏与 `InsetGroupedList` 规范取代，本文仅作 v1 过渡史实保留）
>
> 定位：BerryUI 基础组件层 modern app chrome 规格。触发裁决：应用内不能再用页面底部的裸文字 `Back`；进入应用应有真实顶栏返回按钮，Berry Demo 的目录应呈现现代卡片列表。
>
> 相关：[BerryUI-F4组件第一版-v1.1.md](BerryUI-F4组件第一版-v1.1.md)、[BerryDesign-D2TokenPairContract-v1.0.md](BerryDesign-D2TokenPairContract-v1.0.md)、[BerryDemo-D1-v1.0.md](BerryDemo-D1-v1.0.md)、[架构决策.md](架构决策.md) ADR #92。

## §1 范围

D3 只做组件与应用形态，不改 AST/wire/opcode：

```text
新增 IconButton  48dp 圆形图标命中面 + Press/Focus/Click 状态
AppBar with_back 56dp 顶栏 + 返回按钮 + 居中标题
新增 ListTile   48dp 卡片列表行 + leading icon + 双行文字 + trailing
```

`AppBar::new` / `Scaffold::new` 保留 F4 legacy 标题栏形态，既有 Browser/Contacts/Files/Photos/Settings/UITest 与 golden 零漂；显式 `with_back` 才启用现代导航栏。

## §2 几何

Product（1080×1920，@3x）：

```text
AppBar 高度             56dp = 168px
返回命中面               48dp = 144×144px，纵向居中 y=12
返回图标                 24dp = 72×72px native
图标间距                 4dp = 12px
标题                     S32×48，on_primary，水平居中
ListTile 最小高度        48dp = 144px
ListTile leading media   24dp = 72×72px 圆角面
ListTile trailing        Body 字形（如 >）
```

Compact（800×600，@2x）同公式缩放。全部经 `ThemeMetrics::effective_px(quarter_vp)` 单源换算，禁止组件私算线性像素。

## §3 AppBar / IconButton 契约

`Scaffold::with_back(..., normal_icon, active_icon)`：

```text
AppBar Panel(primary, h=168)
└ Row(w=viewport, h=168, cross=Center)
  ├ IconButton 144×144
  │ └ Icon 72×72
  ├ flex spacer
  ├ Title Text
  └ flex spacer
```

`IconButton`：

- 像素只经 app `IconSource` 供给；normal 与 active 图标成对传入，用于 Normal 与 Pressed/Focused/Selected 换图。
- `panel || icon` 都是命中与消息识别面。
- a11y `ROLE_BUTTON`，语义 `SEM_ACTION_ACTIVATE`，组件 focus 可达。
- Press 换 `surface_variant` 表面并切 active 图标；Release/Cancel 恢复。
- Click 返 true，业务动作仍归 app（返回栈/退出不能藏进控件）。
- Disabled 不响应输入，语义置 disabled。

Berry Demo 五个详情页（Widgets/Inputs/Lists/Theme/System）全部使用顶栏返回；Home 保留底部 `Exit`，语义仍是退出 app。`BERRYDEMOBACK ok` 继续只表示 Home Exit，不与页面返回混用。

## §4 ListTile 契约

`ListTileSpec` 是 Plain Data：

```text
title
detail: Option
leading_icon
trailing
selected
width
```

结构：

```text
Panel(surface/surface_variant, h=144, radius=Medium)
└ Row(w, h=144, cross=Center)
  ├ leading Panel 72×72 + Icon 48×48
  ├ content flex 1
  │ ├ Body title
  │ └ Body detail（Product 字体档单一真值；不用空绘 Caption）
  └ trailing Text
```

- `panel/title/detail/leading/trailing` 均属于命中面。
- a11y `ROLE_LIST_ITEM`，语义 activate；selected 置 checked。
- Press/Focus 有状态色；激活语义归 app。
- 不新增 swipe/long-press/callback；这些是未来引擎 Msg 面，不伪装成组件私货。

Berry Demo Home 的 Demo catalog 使用 ListTile，呈现现代系统的卡片目录；ScrollView 占满 Scaffold body，不再保留 25% 底部死区。

## §5 交互与 damage

Berry Demo 事件循环现在把 Press/Release/Focus/Drag 全量交给组件：

```text
非 Click 消息
→ deliver_widget
→ damage 非空才 render + present
→ damage 空 零提交
```

因此顶栏返回按钮有真实按压反馈，ScrollView 跟手滚动可呈现；停帧原则不变。

## §6 验收

Host：

```text
app_bar_back_is_real_48dp_touch_target
  AppBar 1080×168；back 144×144@y12；body y168
  node_at(back) 命中；a11y Back；Press/Click 状态链

list_tile_is_card_row_with_leading_media
  tile 1056×144；leading icon 48×48；title x84；trailing x1024
  ROLE_LIST_ITEM；Press 状态换色

既有 widget / golden / focus / damage 全量 PASS
```

Guest/Product：

```text
Berry Demo 详情页启动
→ AppBar 顶栏返回按钮真实像素可见
→ 点击返回 Home
→ Home Demo catalog 为卡片式 ListTile
→ WMIDROP bad / INPUTDROPS bad = 0
```

## §7 边界与后续

- 系统状态栏是 Berry Session/system overlay 责任，不在每个 app 里伪造；D3 不消费 safe area。
- large title / scroll collapse、subtitle、action chips 归 D4。
- ListTile trailing chevron 目前是文本字形；矢量 icon 资产与 IconSource 语义扩展归后续视觉刀。
- ARGB IconSource 像素不被 `fg` 改色；D3 用 normal/active 成对图标表达状态，这是当前渲染核的事实边界。
