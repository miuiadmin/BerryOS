# Agent Note: BerryUI AppBar D3 modern app chrome

Status: implemented — 2026-11-13

- Date: 2026-11-13
- Type: decision
- Related: [BerryUI-AppBarD3-v1.0.md](../../../docs/BerryUI-AppBarD3-v1.0.md), [BerryDemo-D1-v1.0.md](../../../docs/BerryDemo-D1-v1.0.md), [架构决策.md](../../../docs/架构决策.md) ADR #92

## Problem

Berry Demo detail pages used full-width text `Back` buttons at the bottom of scrolling cards. They had no real top navigation affordance, no icon, no pressed state, and the demo catalog looked like a widget test rather than a modern mobile app. The user rejected this as unpresentable compared with HarmonyOS/Android/iOS and asked for real basic components rather than a demo-specific patch.

## Decision

Move navigation chrome into BerryUI. Add `IconButton` with a 48dp touch target, normal/active icon states, focus/a11y/semantic activation, and Press/Release handling. Add `Scaffold::with_back` / `AppBar::with_back` as a 56dp top bar with a real back icon and centered title while keeping legacy `AppBar::new` geometry unchanged for existing apps and goldens. Add `ListTile` as a 48dp card row with leading icon, two-line text, trailing affordance, selection/focus/press states, and a Plain Data spec.

Berry Demo now uses the top AppBar back button on all five detail pages, fills the Scaffold body with its ScrollView, uses ListTile for the Home catalog, and forwards all non-Click messages to widgets so press states and scrolling render when damage exists. Pixels remain supplied by the app IconSource; there is no framebuffer access, AST/wire change, or callback ownership in widgets.

## Alternatives considered

- **Hand-build a back row inside Berry Demo**: would improve one demo but leave every third-party app without a standard.
- **Put back-stack behavior inside IconButton**: violates Msg-only component ownership; navigation belongs to the app.
- **Change legacy AppBar geometry globally**: would force unrelated app and golden churn before each app was audited.
- **Fake a system status bar in each app**: status/safe-area ownership belongs to Berry Session/system overlay, not app chrome.
- **Use ARGB `fg` tint for pressed icons**: current Argb raster ignores `fg`; paired normal/active icons express the state without changing the render contract.

## Consequences

Berry Demo now has a real 1080×1920 modern top bar: 168px high, 144×144 back touch target, 72px native arrow, centered title, visible pressed/focus states, and a card-style ListTile catalog. Host tests pin AppBar, IconButton, and ListTile geometry/a11y/state; all BerryUI suites pass. GUI8 assertions are updated to treat the new in-bar back rect and extra press frames as stronger semantics rather than weakening counts.
