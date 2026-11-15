# Intent M4 带骨架: class_fair ×intent 实例化纯面

Status: implemented

## Problem
ADR #42 第二段（M4/v3）立法「class_fair 按 intent 实例化 ×4 带 + 带间严格序 + 每带预算包络」,但 M4 并线前三件空缺:①带参数（权重区间/quantum/latency target）无定义无基准数值——启动配置覆盖面没有可覆盖的形状;②策略注入表（`sched/src/policy.rs` 三槽快照）无 band 选择位——v3 `policy_bind` 协议的带通道无数据位可落;③sched crate 只 aarch64 可编,参数域/映射面若长在 sched 内则 host 不可测（W3-Q 族老病）。同时入场券纪律（v1 遥测实证 + loadgen + 尾延迟断言 + 空闲核金丝雀,`调点调度-v1.2.md` §7）禁止提前实装带执法——需要「形状现在定、实现 M4 补」的骨架。

## Decision
骨架级三件 + 预留两窗（sched/ 域,2026-11-15 调度优化终窗②号刀）:

1. **带定义单源 `berry-iprofile`**（intent 键域表 crate 的同键域扩展,W3-Q1 同款纪律）:`IntentBand{low_weight, high_weight, quantum_ns, latency_target_ns}` + 六行缺省表 `INTENT_BAND_TABLE`（0x00 保守行 / 0x01–0x04 ×4 现役 / 0x05 冻结行——六档全行,`WATCH_PROFILE_TABLE` 同款域完整化纪律）+ 行/整表执法 + `band_enabled`（×4 集判）+ `band_id_of_intent`（带 id ↔ intent 档**恒等**映射——§11 带键 = intent,恒等是结构位非冗余:未来解耦只改此处）+ `band_of_intent`（查表 + 现役门,None = 无带回落现役单带）。数值全为推断值【推】,锚三件:手表表同档权重包络（host 测试逐行同源断言——机器钉死「带权重与 AI 夹紧包络同轴」）、现役 SLICE_HZ=10 的 100ms quantum 锚（0x00 行与缺省行为零漂移）、各档 timer_slack 量级;ADR「数字进启动配置不写死」——表只当骨架基线。
2. **实例化面 `sched/src/class.rs`**:`FairBand{band_id, band: IntentBand}`——`SchedClass` trait 方法全数委托现役 unit `Fair`（行为逐位零漂移;入场券未过不分化）;工厂 `class_fair_intent(intent) -> Option<FairBand>`（×4 现役 Some,0x00/0x05/保留域 None）。`Fair` 本体一字不动——`RoundRobin` 别名与 kernel ktest 二十余处 `let rr = Fair;` 调用点零破坏。
3. **policy band 选择位**:`berry-polface::Slot` 自 `_rsv: u16` 取低字节为 `band: u8`（偏移 14,**16B 布局不变**——kernel `PolicyEntryABI` 镜像 `_rsv` 整 16 位保留且非 0 一律 -EINVAL,v1 ABI 天然不透传,布局兼容不破）+ `BAND_SEL_NONE = 0` 词汇常量;`insert_slot`/`snap_insert` 增 band 参（v1 写路恒 0）;`policy.rs` 增 `band_of(task_key)` 读出口（`placement_hint` 同形,ADR #21 带内纪律注记:band/intent 不入 AI 动作空间,v3 写者授权面随协议立法定）。
4. **预留窗（不实装）**:task→band 赋值（elf spawn 时 manifest intent 档 → `band_id_of_intent` → `class_fair_intent` 建带实例入 ×N 带实例表;带间严格序经既有 `priority() -> i32` 映射,trait 签名零改动）——注释窗落 `class_fair_intent` 文档 + `sched.rs::spawn_intent` 文档两处。

**带词汇依立法非任务书括注**:×4 = 0x01 LC / 0x02 interactive / 0x03 utility / 0x04 background——ADR #42 原文（`docs/架构决策.md` #42 行）、`berry-bandface` BAND_SLOTS 槽注、`调点调度-v1.2.md` §7 机制清单三处一致;0x05 maintenance 编码冻结不启用（表行落保守值 = 参数阶梯最低端锚）。任务书括注「interactive/utility/bg/maintenance 各一带」与三处立法不符,按立法走;「interactive 最小 quantum/最高权重」在表内为 LC 所顶（LC 5ms/512..1024 更极端）,「maintenance 最大/最低」逐字成立。

## Alternatives considered
- `Fair` 本体加字段参数化（否:kernel ktest 二十余处 `let rr = Fair;` / `static RR: Fair = Fair;` 全破——违「不破坏现有调用」;`FairBand` 委托形零破坏,且「同一 trait ×N 实例」的最小落体正是独立实例型）。
- `Directive` 加 band 字段（否:kernel `PE_ZERO`/`policy_tbl_parse` 十处字面量构造全破,kernel 非本刀域;band 位走 Slot 保留字节,v3 协议再扩 Directive/ABI 双侧）。
- 新建 face crate 承带表（否:iprofile 已是 intent 键域表 crate,同键域扩展零新 crate、零 workspace/justfile 接线——最小侵入）。
- ×4 带按任务书括注四档（否:与 ADR #42/bandface 槽注/调点调度 v1.2 §7 三处立法冲突——以立法为唯一事实源）。
- 带参数数值实测定档（否:M4 前无 loadgen 证据,推断值 +【推】标注 + 启动配置覆盖位即骨架本分;预埋带执法配置反而违反入场券立法本身——厂商定制化深研 10 号 X3 判词同源）。

## Consequences
- host 钉板 +10 测:iprofile 23 绿（+9:表自洽/权重包络与手表表逐行同源/quantum·latency 阶梯严格递增/0x00 保守行/×4 集门/查表现役门/恒等映射全域/行执法负例/整表负例）、polface 14 绿（+1:band 位插入→查表持久化 + 16B 布局钉板）;bandface/intface/sliceface/ringface/safeck 零连坐全绿。
- `just build` 绿、内核 aarch64 check 绿、smoke **boot 相回归绿**（隔离验证:HEAD+本刀 hunks 的独立 worktree,`SMOKE_PHASES=boot` 32/32 标记到齐 exit 0——归因干净,不含并行会话未提交改动）;unsafe 台账重产对拍绿 997 条（`band_of` 新 unsafe 块在册,分类「其它/block」与 `placement_hint` 同列;本次重产一并收编上一会话 kernel demo-face 改动遗留的行号漂移——内容锚继承,无人工判读丢失）。
- **smoke rootfs 相未在本窗闭合——环境性,非本刀回归**:宿主 6+ QEMU 并行/负载 10+ 下 TCG 吞吐不足,rootfs 链 600s 帽超时（PIPE/VIRTIO/VBLK/BLKD 已到、FAT 前爬不动;并行会话 smoke 同症双态——同工件 13:08 空窗跑过 FAT、13:11/13:33 负载下同点趴下）。本刀运行面零可达证明:新 API 全仓零调用方、policy 快照 band 字节写 0 与旧 `_rsv` 逐位相同、sched.rs 改动纯注释——rootfs 生态链（fs/net/bsh）不在本刀代码路径上。
- M4 并线工作清单已窗式留位:带间严格序经 `priority()` 差异化 / 每带预算包络接 `band.rs`（BANDS 槽 + 原子账已在）/ 启动配置覆盖 `INTENT_BAND_TABLE` / v3 `policy_bind` 开 band 写路 + `PolicyEntryABI` 字节 14 命名。
- 风险:推断数值非实测（ADR 遥测条款的本意就是 M4 表来自生产 A/B 非拍脑袋）;入场券证据始终不满足则单实例 + 查表键即终态（ADR #42 原文）——届时 `FairBand`/工厂为沉睡面,与 `band.rs` 预算骨架同命运,不是债务。

## Related
[demo-face fw_cfg 门控](2026-11-15-demo-face-fwcfg-gate.md)（同窗①号刀的上轮 kernel 面——其 Residual 节所述 svcmgr 半边与本刀同排程）;[Intent M4 svcmgr 透传首件](2026-11-15-intent-m4-svcmgr透传首件.md)（预留窗④ 的上游数据源——manifest intent 已流进内核 Task 双字段，开窗即得真值）;`docs/架构决策.md` ADR #42/#21/#64;`docs/调点调度-v1.2.md` §7;`research/调度彻底计划/03-彻底计划骨架-刀序里程碑验证体系-v1.0.md` CP-12（M4 正刀的入场券判定与断言面）。
