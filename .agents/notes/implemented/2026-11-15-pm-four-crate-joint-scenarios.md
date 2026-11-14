# PM 四 crate 联合场景:pm-integration 全链纯面(CP-08 接线前)

Status: implemented

## Problem

berry-power(频率帽聚合/充电限流票 mA 域)、berry-thermal(step_wise+迟滞+JEITA)、berry-em(EM 表/EAS 能量律)、berry-slackface(timer slack 聚合)四件各自独立测试全绿(31+13+8+12)但**从未联合**——件间数据流只存在于 ADR 文本(#66 ②「频率帽汇入 berry-power 同一聚合点取 min」/#66 ⑤「JEITA 票由 berry-thermal 出票」/#68 P11「最少者赢」)与调度彻底计划 CP-08 段,跨 crate 接缝(热帽 pct↔聚合面 kHz 的单位差、mA 票直连、帽→EM 容量)零验证——CP-08 接线刀动真通道前需要纯面证据:上游出→下游收,值域与归因逐拍对得上。

## Decision

新建 [user/pm-integration](../../../user/pm-integration/)(独立小工作区,四 crate 只读 path 依赖,零改其源;berry-em/berry-slackface 同族形态,不入 user/Cargo.toml members)。三场景 + 两桥 + 契约钉板:

- **场景 A(热降频全链)**:thermal 穿 trip 出 `freq_cap_pct`/`charge_limit_ma` → 桥 → `FreqCapAgg` 三源 min——帽归因随档切换(fair 不敌低电帽→serious/critical 热帽最紧→回落解除回低电帽),坏拍/停报失败安全贯穿(Err 不触席,帽保持);berry-em 帽下选核两形态随帽变:归因反转(帽截掉跨档跳升后打包偏好抹平,平局回 prev:Some(0)→Some(1))与放不下让域(task_util > 帽后域顶容量→None 回退)。
- **场景 B(slack+EM 联合)**:六档 slack 域(intent 0x00..0x05 全覆盖+系统缺省档)→10 定时器分桶(4 桶,唤醒省 60%,含 UNSPEC 预算恰用满/LC 钉点/INT 恰满三类边界)→每桶批需求对 QEMU 域容量检查(唤醒不超容量,超容 overutil 让位);对抗件(t7 工作量翻倍→批 1150>1024)证翻转。
- **场景 C(充电票+关机边界)**:JEITA 四温区票 ramp mA 直连投 JEITA 席(低电票走 pwproto 真帧经会话面入席、用户帽走 vote_pct 兼容投递)→min(三票)归因随票面切换(JEITA→USER);哨兵边界(不限 u32::MAX 单席归因/min 穿透/平票席号小者/禁充 0 压过一切);critical 三面同拍(发布 3/帽 25%/票 0 停充)+坏拍锁存保持——关机短链本体在内核(#66 ③),用户态纯面验到票 0 为止。
- **两桥(全【推断标注】,桥非裁决)**:①`thermal_cap_khz` pct→kHz 向下取整只收紧,fmax 取 `PolicyParams::default_sim()`(与 QEMU 表顶频点同值,契约钉板防漂)——pct↔kHz 换算属内核聚合点写通道 ABI 未立法面(04 号复核 §6-2,CP-08/CP-10 窗);②`cap_limited_domain` 帽截 OPP 表(帽内最高频点容量=域有效容量,Linux arch_scale 保守直译)——真实 EAS 帽接线归 CP-09。充电票面**无桥**:F1 终裁后 mA 直连,联合测试即 F1 缝闭合的实证。
- **契约钉板**:跨 crate 哨兵同值(thermal `CHARGE_NO_CAP_MA`==power `CHG_LIMIT_UNLIMITED_MA`==u32::MAX)、桥 fmax 基==EM 表顶点、席位号在册、util 核数==EM 域宽——值域漂移即咬。

## Alternatives considered

/tmp 脚本(否:任务书二选一,crate 形态可版本化、Cargo.lock 纪律、CP-08 接线刀可复用为回归面);四 crate 互加跨测(否:零碰四 crate 现役树,且跨 crate dev-dependency 破坏各自独立小工作区形态);等 CP-08 接线后 QEMU smoke 一起验(否:#66 行尾 smoke 断言四件套是接线后真通道面,接线前纯面先行正是本件定位——错序会把纯面 bug 与通道 bug 搅在一起)。

## Consequences

验证实跑(host=aarch64-apple-darwin,`CARGO_TARGET_DIR=target-cargo cargo test --manifest-path user/pm-integration/Cargo.toml --target aarch64-apple-darwin`):14 测全绿;四被测件 31+13+8+12 复跑全绿零回归(只读依赖);lib 桥面 `--lib --target aarch64-unknown-none` 真编过(2 条 warning 为仓 none-target rustflags 既存面,与四 crate 同款);Cargo.lock 纯 path 依赖零外部条目。留窗:justfile test-host 未挂本件(justfile=owner 占用零碰,04 号复核 F3 同款);两桥随 CP-08/CP-09 真通道/真 EAS 立法退役改口;本件不进产线/rootfs。手算坑一条:EWMA 折叠链(0→128→…→752)逐窗手算易错(曾误 741→req 723632),场景内 req 断言值以 cargo 实跑复算钉板(734375)。

## Related

[PM 双服务骨架](./2026-11-13-pm-dual-service-skeleton.md)与[F1/F2 终裁](./2026-11-13-f1-f2-charge-unit-ma-and-sim-stream-split.md)(重叠互链:两笔记立件与单位并轨,本件证其联合面);[EM 能量面 berry-em](./2026-11-13-em-energy-face-berry-em.md)、[timer slack 面](./2026-11-13-timer-slack-face.md)(重叠互链:各自判定律单源,本件是消费侧联合);[04 号复核](../../../research/调度彻底计划/04-PM双服务骨架复核.md) §6 接线窗清单(桥 1 归其第 2 件);ADR #66/#68([架构决策](../../../docs/架构决策.md) 决策总表 :74/:76);[调度彻底计划](../../../docs/调度彻底计划-v1.0.md) CP-08(:99)/CP-09(:105)/CP-10(:106)。
