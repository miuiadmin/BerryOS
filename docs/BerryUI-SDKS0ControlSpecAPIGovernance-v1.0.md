# BerryUI SDK-S0 控件规格与 API 治理基线

> 版本：**1.0**｜2026-11-11
>
> 定位：SDK-S0 的现行规格。执行序权威 = [GUI框架地基路线-v1.20.md](GUI框架地基路线-v1.20.md)。本文把 GUI-15 的官方源研究结论落成两件事：BerryUI 控件规格模板、BerryUI SDK API 账本与机器门禁。
>
> 范围：`berry_ui::sdk` facade 与 BerryUI 控件规格流程。不修改 berry-ui 运行时行为，不新增 guest payload。

---

## §1 出口门

SDK-S0 完成条件：

```text
控件规格模板冻结
TextField 首个控件规格落地
BerryUI SDK facade 100% 入账
since/channel/capability/owner/stability/breaking 字段冻结
tools/sdk_ledger.py --check 入 just check
API availability matrix 由机器生成
```

机器件：

```text
tools/sdk_api.toml
tools/sdk_ledger.py
```

运行：

```bash
python3 tools/sdk_ledger.py --write   # intentional API change
python3 tools/sdk_ledger.py --check   # CI / just check
```

---

## §2 控件规格模板

新交互控件进入 `berry_ui::sdk` 前，必须有 `docs/BerryUIControl-<Name>-v主.次.md`。模板骨架：

```text
1  Background
2  Problem / non-goals
3  Is this the right control?
4  Geometry / layout contract
5  State ownership
6  Focus contract
7  Keyboard contract
8  Pointer/touch contract
9  Text/IME contract（若适用）
10 Accessibility semantics
11 Theming/token contract
12 API Notes / API Details
13 Examples
14 Tests and evidence
15 Compatibility / @since
16 Open Questions
```

强制规则：

```text
交互控件必须给逐键 Keyboard contract
控件业务状态归 app，widget 只保存句柄与应用状态辅助
规格先列拒面与非目标
API Notes 是 rustdoc/用户文档单源
Examples 必须能对应 host test 或 GUI8 标记
```

首件：`docs/BerryUIControl-TextField-v1.0.md`。

---

## §3 API 账本 schema

单源文件：`tools/sdk_api.toml`。

字段：

```text
id          berry_ui::sdk::<Name>
kind        fn | type | const
module      desc/diagnostic/layout/metrics/theme/text_input/widgets/crate/sdk
since       刀片标签（F2/F4/C1/L1/...）；crate semver 后续升档
channel     in-tree（crates-io 后续 add-only）
capability  none（当前 BerryUI facade 不直接授予系统能力）
owner       berry-ui
stability   experimental | stable | frozen | deprecated
spec        相对路径，必须存在
breaking    数组；每条破坏性变更必须有 successor
```

弃用铁律：

```text
无 successor 的弃用不得合入
稳定/冻结 API 签名变更必须同批提供迁移说明
experimental 可以调整，但必须保持规格与账本同批更新
```

---

## §4 生成矩阵

下表由 `tools/sdk_ledger.py --write` 生成，禁止手改：

<!-- BEGIN generated:sdk-availability -->

| API | kind | since | channel | stability | spec |
|---|---|---|---|---|---|
| `berry_ui::sdk::A11yRegion` | type | SEM-A1 | in-tree | experimental | `docs/BerryUI-SEMA1SemanticTreeSidecar-v1.0.md` |
| `berry_ui::sdk::AlertDialog` | type | TRI-CONTROLS-2 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::AppBar` | type | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::Axis` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::Badge` | type | TRI-CONTROLS-3 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::BoxConstraints` | type | LAYOUT-L4 | in-tree | experimental | `docs/BerryUI-LAYOUTL4BoxConstraintsIntrinsicBoundary-v1.0.md` |
| `berry_ui::sdk::Button` | type | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::Card` | type | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::Checkbox` | type | TRI-CONTROLS-1 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.0.md` |
| `berry_ui::sdk::Chip` | type | TRI-CONTROLS-3 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::ChipKind` | type | TRI-CONTROLS-3 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::CircleProps` | type | GUI-6 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::ClipProps` | type | C1-clipping | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::ColProps` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::ColorPair` | type | DESIGN-D2 | in-tree | experimental | `docs/BerryDesign-D2TokenPairContract-v1.0.md` |
| `berry_ui::sdk::ColorPairToken` | type | DESIGN-D2 | in-tree | experimental | `docs/BerryDesign-D2TokenPairContract-v1.0.md` |
| `berry_ui::sdk::ColorScheme` | type | DESIGN-D2 | in-tree | experimental | `docs/BerryDesign-D2TokenPairContract-v1.0.md` |
| `berry_ui::sdk::ColorSchemePreference` | type | DESIGN-D2 | in-tree | experimental | `docs/BerryDesign-D2TokenPairContract-v1.0.md` |
| `berry_ui::sdk::Constraints` | type | L1 | in-tree | stable | `docs/BerryUI-L1LocalConstraintsAdaptiveSlot-v1.0.md` |
| `berry_ui::sdk::ContrastLevel` | type | DESIGN-D2 | in-tree | experimental | `docs/BerryDesign-D2TokenPairContract-v1.0.md` |
| `berry_ui::sdk::CrossAlign` | type | L1 | in-tree | stable | `docs/BerryUI-L1LocalConstraintsAdaptiveSlot-v1.0.md` |
| `berry_ui::sdk::Density` | type | F3 | in-tree | stable | `docs/BerryDesign-F3主题与基元-v1.0.md` |
| `berry_ui::sdk::DialogAction` | type | TRI-CONTROLS-2 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::EmptyIconSource` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::EmptyState` | type | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::FlexBasis` | type | LAYOUT-L6a | in-tree | experimental | `docs/BerryUI-LAYOUTL6aFlexSpecAlgebra-v1.0.md` |
| `berry_ui::sdk::FlexEntry` | type | L2 | in-tree | stable | `docs/BerryUI-LAYOUTL6aFlexSpecAlgebra-v1.0.md` |
| `berry_ui::sdk::FlexSpec` | type | LAYOUT-L6a | in-tree | experimental | `docs/BerryUI-LAYOUTL6aFlexSpecAlgebra-v1.0.md` |
| `berry_ui::sdk::FocusDir` | type | C1 | in-tree | stable | `docs/BerryUI-C1ComponentFocusNavigation-v1.0.md` |
| `berry_ui::sdk::FocusEntry` | type | C1 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::FocusOp` | type | C1 | in-tree | stable | `docs/BerryUI-C1ComponentFocusNavigation-v1.0.md` |
| `berry_ui::sdk::FontSource` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::Grid` | type | TRI-CONTROLS-1 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.0.md` |
| `berry_ui::sdk::GridProps` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::HitDumpEntry` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::IconButton` | type | D3 | in-tree | experimental | `docs/BerryUI-AppBarD3-v1.0.md` |
| `berry_ui::sdk::IconProps` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::IconSource` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::ImageProps` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::InteractionState` | type | F3 | in-tree | stable | `docs/BerryDesign-F3主题与基元-v1.0.md` |
| `berry_ui::sdk::KEYED_DEPTH_MAX` | const | RUN-K1 | in-tree | experimental | `docs/BerryUI-RUNK1KeyedReApplyLifecycle-v1.0.md` |
| `berry_ui::sdk::KeyRoute` | type | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |
| `berry_ui::sdk::LayoutDumpEntry` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::LayoutStat` | type | L1 | in-tree | stable | `docs/BerryUI-L1LocalConstraintsAdaptiveSlot-v1.0.md` |
| `berry_ui::sdk::ListDetail` | type | L3 | in-tree | stable | `docs/BerryUI-L3TransposedContainerFlexListDetail-v1.0.md` |
| `berry_ui::sdk::ListDetailSpec` | type | L3 | in-tree | stable | `docs/BerryUI-L3TransposedContainerFlexListDetail-v1.0.md` |
| `berry_ui::sdk::ListItem` | type | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::ListTile` | type | D3 | in-tree | experimental | `docs/BerryUI-AppBarD3-v1.0.md` |
| `berry_ui::sdk::ListTileSpec` | type | D3 | in-tree | experimental | `docs/BerryUI-AppBarD3-v1.0.md` |
| `berry_ui::sdk::LoadingIndicator` | type | TRI-CONTROLS-1 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.0.md` |
| `berry_ui::sdk::Menu` | type | TRI-CONTROLS-2 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::MenuItemSpec` | type | TRI-CONTROLS-2 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::MonospaceTextMetrics` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::Msg` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::MsgBuf` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::NAV_ICON_BACK` | const | OH-SHELL | in-tree | stable | `docs/Berry设备平台-OpenHarmonyShell-v1.0.md` |
| `berry_ui::sdk::NAV_ICON_BACK_ACTIVE` | const | OH-SHELL | in-tree | stable | `docs/Berry设备平台-OpenHarmonyShell-v1.0.md` |
| `berry_ui::sdk::NAV_ICON_HOME` | const | OH-SHELL | in-tree | stable | `docs/Berry设备平台-OpenHarmonyShell-v1.0.md` |
| `berry_ui::sdk::NAV_ICON_HOME_ACTIVE` | const | OH-SHELL | in-tree | stable | `docs/Berry设备平台-OpenHarmonyShell-v1.0.md` |
| `berry_ui::sdk::NAV_ICON_TASK` | const | OH-SHELL | in-tree | stable | `docs/Berry设备平台-OpenHarmonyShell-v1.0.md` |
| `berry_ui::sdk::NAV_ICON_TASK_ACTIVE` | const | OH-SHELL | in-tree | stable | `docs/Berry设备平台-OpenHarmonyShell-v1.0.md` |
| `berry_ui::sdk::NavigationIconSource` | type | OH-SHELL | in-tree | stable | `docs/Berry设备平台-OpenHarmonyShell-v1.0.md` |
| `berry_ui::sdk::NodeId` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::OverlaySurface` | type | TRI-CONTROLS-2 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::PadProps` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::PanelProps` | type | F4 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::ProgressIndicator` | type | TRI-CONTROLS-1 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.0.md` |
| `berry_ui::sdk::Props` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::Radio` | type | TRI-CONTROLS-1 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.0.md` |
| `berry_ui::sdk::RadiusToken` | type | F3 | in-tree | stable | `docs/BerryDesign-F3主题与基元-v1.0.md` |
| `berry_ui::sdk::ReapplyStat` | type | RUN-K1 | in-tree | experimental | `docs/BerryUI-RUNK1KeyedReApplyLifecycle-v1.0.md` |
| `berry_ui::sdk::RectI` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::RectProps` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::RowProps` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::SEGMENTED_MAX` | const | TRI-CONTROLS-3 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::SEM_ACTION_ACTIVATE` | const | SEM-A1 | in-tree | experimental | `docs/BerryUI-SEMA1SemanticTreeSidecar-v1.0.md` |
| `berry_ui::sdk::SEM_ACTION_ADJUST` | const | SEM-A1 | in-tree | experimental | `docs/BerryUI-SEMA1SemanticTreeSidecar-v1.0.md` |
| `berry_ui::sdk::SEM_ACTION_FOCUS` | const | SEM-A1 | in-tree | experimental | `docs/BerryUI-SEMA1SemanticTreeSidecar-v1.0.md` |
| `berry_ui::sdk::SEM_ACTION_TEXT_EDIT` | const | SEM-A1 | in-tree | experimental | `docs/BerryUI-SEMA1SemanticTreeSidecar-v1.0.md` |
| `berry_ui::sdk::SEM_LABEL_MAX` | const | SEM-A1 | in-tree | experimental | `docs/BerryUI-SEMA1SemanticTreeSidecar-v1.0.md` |
| `berry_ui::sdk::SEM_MAX` | const | SEM-A1 | in-tree | experimental | `docs/BerryUI-SEMA1SemanticTreeSidecar-v1.0.md` |
| `berry_ui::sdk::SEM_STATE_CHECKED` | const | SEM-A1 | in-tree | experimental | `docs/BerryUI-SEMA1SemanticTreeSidecar-v1.0.md` |
| `berry_ui::sdk::SEM_STATE_DISABLED` | const | SEM-A1 | in-tree | experimental | `docs/BerryUI-SEMA1SemanticTreeSidecar-v1.0.md` |
| `berry_ui::sdk::SEM_STATE_SELECTED` | const | TRI-CONTROLS-3 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::SETTLE_DERIVE_MAX` | const | RUN-K1 | in-tree | experimental | `docs/BerryUI-RUNK1KeyedReApplyLifecycle-v1.0.md` |
| `berry_ui::sdk::SafeInsets` | type | TRI-CONTROLS-1 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.0.md` |
| `berry_ui::sdk::Scaffold` | type | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::ScrollView` | type | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::ScrollViewAxis` | type | TRI-CONTROLS-4 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::SearchField` | type | TRI-CONTROLS-3 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::Section` | type | TRI-CONTROLS-1 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.0.md` |
| `berry_ui::sdk::SegmentedControl` | type | TRI-CONTROLS-3 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::SegmentedItemSpec` | type | TRI-CONTROLS-3 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::SemEntry` | type | SEM-A1 | in-tree | experimental | `docs/BerryUI-SEMA1SemanticTreeSidecar-v1.0.md` |
| `berry_ui::sdk::Sheet` | type | TRI-CONTROLS-2 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::Slider` | type | TRI-CONTROLS-1 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.0.md` |
| `berry_ui::sdk::SlotVariant` | type | L1 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::SpacingToken` | type | F3 | in-tree | stable | `docs/BerryDesign-F3主题与基元-v1.0.md` |
| `berry_ui::sdk::StateColors` | type | F3 | in-tree | stable | `docs/BerryDesign-F3主题与基元-v1.0.md` |
| `berry_ui::sdk::Stepper` | type | TRI-CONTROLS-3 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::StrokeToken` | type | F3 | in-tree | stable | `docs/BerryDesign-F3主题与基元-v1.0.md` |
| `berry_ui::sdk::Switch` | type | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::TABS_MAX` | const | TRI-CONTROLS-1 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.0.md` |
| `berry_ui::sdk::TEXT_INPUT_MAX` | const | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |
| `berry_ui::sdk::TEXT_PICKER_MAX` | const | TRI-CONTROLS-3 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::TEXT_WRAP_MAX` | const | LAYOUT-L5 | in-tree | experimental | `docs/BerryUI-LAYOUTL5PerNodeBoxTextWrap-v1.0.md` |
| `berry_ui::sdk::TabBarStyle` | type | TRI-CONTROLS-1 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.0.md` |
| `berry_ui::sdk::TabItemSpec` | type | TRI-CONTROLS-1 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.0.md` |
| `berry_ui::sdk::Tabs` | type | TRI-CONTROLS-1 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.0.md` |
| `berry_ui::sdk::TextBuf` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::TextField` | type | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |
| `berry_ui::sdk::TextFieldSpec` | type | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |
| `berry_ui::sdk::TextInputEdit` | type | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |
| `berry_ui::sdk::TextInputKeyOp` | type | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |
| `berry_ui::sdk::TextInputKind` | type | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |
| `berry_ui::sdk::TextInputOutcome` | type | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |
| `berry_ui::sdk::TextInputState` | type | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |
| `berry_ui::sdk::TextMetrics` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::TextPicker` | type | TRI-CONTROLS-3 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::TextProps` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::TextScale` | type | DESIGN-D2 | in-tree | experimental | `docs/BerryDesign-D2TokenPairContract-v1.0.md` |
| `berry_ui::sdk::TextSelection` | type | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |
| `berry_ui::sdk::TextStyle` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::TextVariant` | type | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::TextWrapEntry` | type | LAYOUT-L5 | in-tree | experimental | `docs/BerryUI-LAYOUTL5PerNodeBoxTextWrap-v1.0.md` |
| `berry_ui::sdk::Theme` | type | F3 | in-tree | stable | `docs/BerryDesign-D2TokenPairContract-v1.0.md` |
| `berry_ui::sdk::ThemeColors` | type | F3 | in-tree | stable | `docs/BerryDesign-F3主题与基元-v1.0.md` |
| `berry_ui::sdk::ThemeMetrics` | type | F3 | in-tree | stable | `docs/BerryDesign-F3主题与基元-v1.0.md` |
| `berry_ui::sdk::Toast` | type | TRI-CONTROLS-2 | in-tree | stable | `docs/BerryUI-TriPlatformControls-v1.3.md` |
| `berry_ui::sdk::TypographyToken` | type | F3 | in-tree | stable | `docs/BerryDesign-F3主题与基元-v1.0.md` |
| `berry_ui::sdk::Ui` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::UiError` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::UiMain` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::ViewKind` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::ViewSpec` | type | F2 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::WidgetContext` | type | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::a11y_region_roundtrip` | fn | SEM-A1 | in-tree | experimental | `docs/BerryUI-SEMA1SemanticTreeSidecar-v1.0.md` |
| `berry_ui::sdk::column` | fn | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::contrast_meets` | fn | DESIGN-D2 | in-tree | experimental | `docs/BerryDesign-D2TokenPairContract-v1.0.md` |
| `berry_ui::sdk::contrast_ratio_centi` | fn | DESIGN-D2 | in-tree | experimental | `docs/BerryDesign-D2TokenPairContract-v1.0.md` |
| `berry_ui::sdk::divider` | fn | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::focus_key_intent` | fn | C1 | in-tree | stable | `docs/BerryUI-C1ComponentFocusNavigation-v1.0.md` |
| `berry_ui::sdk::icon` | fn | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::ime_overlap_px` | fn | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |
| `berry_ui::sdk::key_route` | fn | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |
| `berry_ui::sdk::resolve_color_scheme` | fn | DESIGN-D2 | in-tree | experimental | `docs/BerryDesign-D2TokenPairContract-v1.0.md` |
| `berry_ui::sdk::row` | fn | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::slot_variant` | fn | L1 | in-tree | stable | `docs/BerryUI-F2核心盘点-v1.0.md` |
| `berry_ui::sdk::text` | fn | F4 | in-tree | stable | `docs/BerryUI-F4组件第一版-v1.1.md` |
| `berry_ui::sdk::text_key_intent` | fn | GUI-T1 | in-tree | experimental | `docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md` |

<!-- END generated:sdk-availability -->

---

## §5 门禁接线

`just check` 顺序新增：

```text
python3 tools/sdk_ledger.py --check
```

新增独立 recipe：

```text
sdk-ledger-check
```

检查内容：

```text
schema/vocabulary
id 唯一
spec 文件存在
breaking successor 必填
facade 与账本双向 100% 对拍
生成矩阵与文档 byte 对拍
```

---

## §6 非目标

```text
不治理 berry-ui root re-exports 与 embedder 内部面
不发布 crates.io 新版本
不引入运行时反射/API 注册表
不自动解析 Rust doc
不为 F4 存量 13 控件一次性补规格
不把 CC-BY-SA/GPL 官方文本复制进公开文档
```

存量 F4 控件按后续接触顺序回填规格；当前只强制新控件与 TextField。

---

## §7 证据

- 依据：`research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md`
- OpenHarmony `.d.ts` `@since/@syscap/@deprecated+@useinstead` 证据：`reference/sdk-ui/cache/openharmony-interface-sdk-js`
- WinUI spec→docs→Gallery 流程证据：`reference/sdk-ui/cache/microsoft-ui-xaml-specs`
- 首件控件规格：`docs/BerryUIControl-TextField-v1.0.md`
- GUI-T1 实施规格：`docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md`
