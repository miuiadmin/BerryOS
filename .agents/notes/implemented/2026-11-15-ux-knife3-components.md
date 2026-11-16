# Agent Note: 刀 3 berry-ui 组件层 + launcher 建树三连环炸修复

Status: implemented

## Problem

刀 3（OH 形组件层）开工即撞刀 1 重放的隐藏缺陷：boot serial 里 `launcher: desktop build err=BadParent` 后 park——桌面从未真正建起。之前误判「系统活」的根因：`LAUNCH v=1` 是 svcmgr 打的 launcher 起动标记（进程拉起 ≠ 建树成功），`launcher: clock` 在 build **之前**打——两个标记都不证明桌面活着。

## Decision

**三连环炸与修法**（每修一关暴露下一关——boot 验证必须读到 `desktop build` 无 err 行才算过）：

1. **BadParent**：berry-ui `add()` 白名单只许容器为父（Rect/Text/Image/Icon 是叶子）。rescue 版结构「Icon 叠 Rect」「item Col 挂 press_face Rect 下」必炸。修法 = **Pad 双子叠加**：Pad 对所有子同位全幅分配（place_children 实证）= 叠加语义；Rect 先插 = 底，内容后插 = 盖上。桌面格与 nav 键同法。
2. **press_map NodeFull**：4 页全登记 85 对 > PRESS_MAP_MAX=28。修法 = 只登当前页 20 格 + 5 nav 键 = 25 对；翻页刷新挂进既有 `rehang_a11y` 钩子（nav faces 数组进 NavNodes 结构体随表重建）。
3. **a11y 32 帽**：5 键时代全账 = 状态栏 4 + dock 4 + nav 5 + 页 0 网格 21 = 34 > 32。修法 = 状态栏三条模拟值（信号/WiFi/电池恒满格）退 a11y——模拟值无读屏价值；账 31 < 32。

**wire 纪律（D26 教训重现）**：radius 首版直接内联 Rect props（11B→13B）被 V1 golden 字面量守卫拦下（golden_v1_stream_pinned_literal 红 = 编码器不兼容变更）。正解 = flags bit4 = HAS_RADIUS 扩展块（2B LE，序在 ALIGN 后；radius=0 无块 = V1 流零漂移）。flags 未知位绊线随迁 bit5（0x20 拒；0x10 转 HAS_RADIUS×非 Rect 由合法域表拒 UnknownProp）。

**刀 3 交付**：components.rs 组件层（OH token 单源：TopBar 56vp/BottomTabs 48vp/卡圆角 16vp/内距 16vp/隙 12vp + mix_under 预混原语 + card_spec/topbar_spec/bottom_tabs_spec/add_list_item 装配）；paint 圆角带状分解（整数牛顿 isqrt 零浮点 no_std 合规；radius=4 → 9 带+中带）。

## Verification

berry-ui 全测试面 207 测全绿（23 suite；press.rs 帽断言随 PRESS_MAP_MAX=28 扩容同步、reconcile.rs 绊线迁 bit5、52 处测试 RectProps 补 radius 字段）。launcher musl 链绿、build-user 全链绿（tlsscan 58 ELF 过）。boot noktest TCG 实证：`launcher: clock 01:17 utc` → `desktop build` 无 err → **桌面首帧真渲染**（PIL 验帧：壁纸青绿渐变 284104px + 状态栏白图标 + 蓝强调色）。

## Consequences

- 桌面按压反馈链（press_map 25 对）+ 翻页刷新面落地；后续刀 4（多窗合成）在 rehang 钩子加 SYSTEM 层避让。
- 组件层为刀 5（appdemo 示范 UI）与刀 6（OOBE）供米——「标题栏+卡片列表+底部页签」页可直接声明式拼出。
- 挂账：press_map 帽 28 在 25 对占用下余 3——刀 4 若加 SYSTEM 层按压面需扩帽或减登记；a11y 31/32 满——新增 a11y 面前先腾槽。

## Related

- [2026-11-14-模拟器体验统一分刀方案.md](../../plans/2026-11-14-模拟器体验统一分刀方案.md)
- [2026-11-15-ux-splash-replay.md](2026-11-15-ux-splash-replay.md)
