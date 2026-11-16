# 弹簧动画实施清单（维三最小刀两刀）

设计一句话：给 berry-ui 加与既有定长时间线动画平行的「定点弹簧第二动画面」——Q16.16 纯整数质量-刚度-阻尼半隐式 Euler 解算（初速度入参、重定目标以当前状态续接、双阈值收敛吸附目标），SwipePager 松手吸附改为可配置档（默认 QuadOut 零变化、VelocitySpring 档以松手速度切弹簧），既有五属性四档缓动与 TLV/wire 面零改动。

依据：`research/UI-02-六大系统UI差距深研与裁决-2026-11.md` 维三裁决（最小刀法两刀：anim.rs 定点弹簧 + SwipePager 松手速度接入）与专节三「动画深度」归波建议。

## 改动清单

- `user/berry-ui/src/anim.rs`（+245 行，纯新增段）
  - `SpringPreset` 两档预设：`Smooth`（ω=12、ζ=1.0 临界阻尼，k=144/c=24，收敛≈0.5s——OH `curves.springMotion` 默认档与 iOS `.smooth` 共同气质）、`Bouncy`（ω=16、ζ=0.625 欠阻尼，k=256/c=20，过冲≈8%——iOS `.bouncy` 气质）；质量恒 1 已折入常量。
  - `SpringProp` 定点弹簧：构造入参 = 目标值 + 初速度（px/s，速度环 96ms 窗差分输出可直入）+ 预设两档 + node/prop（既有五属性之一）；`retarget` = 只换目标、位置/速度积分态原样续接（中断恢复主干——iOS velocity/shouldMerge 与 OH 终点变化自动接管的等价物）；`step` 半隐式 Euler 子步积分（子步 16ms、单 tick 子步帽 16 步=256ms 慢动作追赶，防长帧爆积分）；终止判据 = 位移 ≤0.5px 且 |速度| ≤20px/s 双阈值 → 吸附目标（终点钉板，与 anim::tick 完成帧同语义）。
  - `spring_tick` 弹簧表帧内推进：与 anim tick 同帧同时钟，经临时 `Anim` 借 node/prop 走**同一 `apply` 单源落点**（Transform/Layout 双型、s7 staging 面全复用）；死节点 no-op 但照常收敛移除（防 anim_active 卡死的既有纪律）。
- `user/berry-ui/src/lib.rs`（+56/-6 行）
  - `Ui` 增 `springs: [Option<SpringProp>; ANIM_MAX]` 第二表（与 anims 同帽同错位，满 = `AnimFull` loud）；`spring_add` 入表面；`anim_active` 并集两表（空弹簧表零动静）；`anim_clear` 双面全撤；`remove` 连带清随行弹簧；`render_frame` 在 anim tick 后、staging apply 前推进弹簧（s7 双面 cfg 接线）。
- `user/berry-ui/src/swipe.rs`（+85/-36 行）
  - 松手吸附从固定 QuadOut 240ms 改为可配置 `SnapMode`：`QuadOut`（默认——行为零变化，velocity 不参与）与 `VelocitySpring`（松手速度非零时切 `SpringProp`、以松手速度入参、Smooth 预设；速度为零含 Cancel 回落 QuadOut）；`set_snap_mode` 配置面；`Snap` 改枚举（QuadOut 时间线 / Spring 弹簧）；`begin_snap` 增速度入参；tick 双路径（QuadOut 逻辑逐字保持，Spring 走 step/done）。页宽冻结与建树语义零改。
- `user/berry-ui/tests/spring.rs`（新，9 测）：收敛网格（两档×六参数组终点精确到位）、Smooth 单调无过冲、Bouncy 过冲后回摆吸附、重定目标连续（位置与速度读出逐位不变+续步连续+折返场景）、初速度方向正确（带速起跳不即时收敛）、Q16.16 溢出负例（i32 端点+巨速度+秒级时钟跳：钳制不 panic 有限收敛）、引擎表三件（Transform 收敛排空+毕后零动静 / Layout prop 落点+死节点纪律 / 帽同 AnimFull+clear 双面）。
- `user/berry-ui/tests/swipe.rs`（+3 测）：默认档零变化断言（非零速度松手仍 QuadOut：起拍持 from、中点 -83 值级钉板、240ms 终点精确、到期即毕）、VelocitySpring 档（240ms 仍在途、单调段、双阈值吸附精确终点）、零速度/Cancel 回落 QuadOut。

## 关键裁决：不扩 `AnimProp` 枚举本体

任务书「AnimProp 枚举扩 Spring 变体」落为**挂载面扩展**而非枚举字面扩展，理由两条（均为硬约束）：

1. `AnimProp` 语义 = 「驱动哪个属性」（五属性之一），Spring 是**驱动方式**非属性——无属性面的 `AnimProp::Spring` 变体在 `kind_of`/`apply` 的 exhaustive match 纪律下只能落死臂（该纪律是 ADR #60 钉板先行的编译面）。
2. 载荷变体 `Spring(SpringProp)` 需在 SpringProp 内嵌目标 prop → 递归类型无 indirection 不可编译，而 Box 被热路径零分配约束禁用。

等价实现：`SpringProp.prop: AnimProp`（弹簧指认五属性之一）+ lib.rs 弹簧第二表（`spring_add`/`spring_tick`）——「动画挂载族 gains Spring」的 add-only 落法；既有五属性变体、四档 Easing、`Anim` 结构体、`interp` 逐字零改动（git diff 实证）。

## 约束合规

- no_std / 零分配 / 纯整数：SpringProp 全 core 整数算术（Q16.16 + i128 中间量，同 `interp` 风格）；无 Box/Vec/闭包新增；aarch64-unknown-none（guest 面）与 berry-launcher musl 链（`--target aarch64-unknown-linux-musl` + berry-ld）均编译通过。
- TLV/wire 面零新字段：Spring 是引擎内动画参数非 wire 态；`AnimProp` 未 TLV 化（grep 实证：仅 anim.rs 与 tests 引用），故「已 TLV 化则 golden 自证」条件不触发——但仍跑了全套 golden 作经验证据：gallery2 与 tlv2-7 全绿。
- cells.tsv 说明：`gallery2_dump_cells` 每次运行重写 `WAVE2-GALLERY/cells.tsv`，其 `kind_input` 行哈希在**基线 a2f9ff888 上同样漂移**（stash 对照实证，本机 dump 副作用，与硬编码 golden 断言无关——断言本身绿）；已 `git checkout` 还原，不夹带。

## 构建自验（CARGO_TARGET_DIR 隔离于 `.dsh-worktrees/ui-spring/.target-s`）

- A 面（默认主门）`cargo test -p berry-ui --target aarch64-apple-darwin`：328 测全绿（基线 316 + 新 12）。
- B 面（s7 实验轨）`--features s7`：330 绿 / 1 红 = `widgets7::batch_f_set_props_reflects_paint`——基线同红（stash 对照实证），s7 面既有问题非本刀引入；s7 为 host-only 实验轨道（Cargo.toml 注明不进 guest 产物/CI 主门）。
- guest 产物链：`cargo build -p berry-ui`（aarch64-unknown-none、no_std）通过；berry-launcher musl release 链通过。
