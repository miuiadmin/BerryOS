# berry 默认窗口尺寸改逻辑点口径(DPI 修复)

Status: implemented

## Problem

宿主窗口默认尺寸按物理像素直算(`window.rs::resumed()` 旧 :766-770 用 `with_inner_size(PhysicalSize)`,factor 基准 0.8/0.55 也是物理语义):在 2x HiDPI 屏上窗口视觉尺寸等效减半——16″ MBP(3456×2234 @2.0)desktop 档开窗仅 216×384 逻辑点,内容缩到约 0.2 倍,小到像不可交互,放大了「分辨率固定不能拖」的体感并加重字体微小感。根因解剖见 [01 窗口与分辨率](../../../research/桌面UX审计/01-窗口与分辨率.md) §2.1 与修法 a。

## Decision

窗口默认尺寸全面改逻辑点口径,改动收敛在 `tools/berry/src/window.rs` 单文件(实施波一刀 A):

- 尺寸计算抽成两个纯函数 `base_window_factor` + `default_window_size`(window.rs :195-246):panel 逻辑尺寸 × factor,0.9 屏幕钳制改在逻辑口径(显示器物理尺寸 / scale_factor),factor 下限 0.2 与尺寸下限 320×240 原值保留但同为逻辑语义;`with_inner_size` / `with_min_inner_size` 换 `LogicalSize`,由 OS 乘 scale_factor。
- factor 基准:高面板(panel_h > 1200)保留 0.55;矮面板 0.8 → 0.9。理由:01 号修法 a 的效果推算(486×864 逻辑)与验收线(「开窗 ≥ 480 逻辑点宽」)都按 0.9 算,旧 0.8 是物理像素时代的调参,若原样保留 desktop 档只有 432 逻辑点宽,过不了底稿自己的验收。
- `ScaleMode::Integer(n)` 因子直传语义保留(0.9 钳制照旧适用,与旧行为一致);RES 菜单换档行为与窗口标题「BerryOS」均不动。
- 验证:headless 开不了真窗,用单元断言推演——新 6 测(`default_size_tests`,window.rs :999-1067)覆盖 HiDPI(desktop 486×864 逻辑、经 winit `LogicalSize::to_physical(2.0)` 断言物理 972×1728)、phone 钳制(483×1005)、1x 屏钳制不变、Integer 直传、双下限;`cargo build -p berry`(host target)+ 21 测全绿;`berry shot` 冒烟(BERRY_HOME 重定向进工作区)端到端 boot 抓帧 1080×1920 不回归。真窗实测留用户屏。

## Alternatives considered

- 矮面板基准保留 0.8:1x 屏行为零变化,但 desktop 档 432 逻辑点宽 < 480,过不了 01 号验收线,弃。
- 手算 `PhysicalSize(逻辑 × scale_factor)` 传物理值:数学等价,但自己复刻 winit 已有的 DPI 换算,多一处口径分叉,弃。
- 窗口初始居中(`with_position`,01 号修法 a 可选第 3 条):不在本刀任务书范围,留待后续波。

## Consequences

- 2x 屏窗口面积翻倍(desktop 档 216×384 → 486×864 逻辑),内容起步缩放约 0.2 → 0.45 逻辑倍,直接缓解「太小/不能拖」体感与痛点⑥共因;1x 屏仅在基准束缚(大屏)时窗宽 432 → 486,钳制路径行为不变。
- `ScaleFactorChanged` 仍只重绘不重排(01 号已记,跨屏拖动后尺寸不自适应),本刀未动,与工具栏逻辑化(44px 命中区)同留实施波二。
- `berry shot` 冒烟若在沙箱内跑,需 `BERRY_HOME` 指进工作区(默认 `~/.berry` 在工作区外会被拒)。

## Related

- 底稿:[01 窗口与分辨率](../../../research/桌面UX审计/01-窗口与分辨率.md)(修法 a、§2.1 根因、验收)
- 改动:`tools/berry/src/window.rs`(:195-246 纯函数、:797-820 resumed、:999-1067 测试)
