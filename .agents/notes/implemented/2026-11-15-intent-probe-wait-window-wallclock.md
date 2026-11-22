# INTENT 探针等待窗加墙钟下限 max(8×slice_ns, 400ms)（46 号案 a 首刀）

Status: implemented

## Problem

INTENT 验收司机 `task_intent_probe` 睡 `8 × slice_ns` 等全链落定——设计隐含 10Hz 假设（8×100ms=800ms）。被测者 `it_slow` 睡 200ms 墙钟后 send CH6 触发降级；250Hz 时 8×4ms=32ms、1000Hz 时 8ms，均 < 200ms，司机早醒，`intent_budget_capped_total() >= 2` 与 `intent_borrows() >= 3` 双断言炸 → `INTENT ok` 永缺席 → BCTS 镜像腿 bare-24 缺 → 判定单 FAIL。后果：OEM 按平台推荐值（desktop 档 250Hz）配置就拿不到 BCTS 判定单——slice 推荐值 × BCTS 判定单 × G5 产品链三者互斥（44 号 §2-1 P0，46 号 §4 建议案）。

## Decision

等待窗一处改墙钟下限：`now + 8 * slice_ns` → `now + core::cmp::max(8 * slice_ns, 400_000_000)`（`kernel/src/main.rs` `task_intent_probe`，46 号案 a——owner 裁决锚的拍板语句草案原样落刀）。要点：

- 10Hz 臂恒等零回归：8×100ms=800ms > 400ms，max 取大端，睡眠时长逐 ns 不变（default 盘 INTENT ok 7.709s 在案谱系保持——本轮实测 7.793s 同带）。
- 250/1000 档解锁：400ms = 2× it_slow 200ms 被测事件，司机不再早醒。
- 三断言（borrows≥3 / hop_capped≥1 / budget_capped≥2）零改动——验收面不弱化（案 b 否决的根据）。
- BCTS 侧零改动（markers.sh bare 相 add-only 合同不动）。
- 魔数对债务如实入注：it_slow 的 200ms 睡改则 400ms 跟改（源码注释已钉）。

## Alternatives considered

- 案 b（断言阈值随 slice_hz 比例折算）：否决——在推荐档位永久弱化验收面，与 bare 相 add-only 合同精神相悖；不修根因（窗 < 事件时刻，探针仍早醒只是不再喊）。
- 案 c（探针改事件驱动，形二条件轮询 + 墙钟兜底帽）：登记为正形后续——真同步零窗假设、对未来改参鲁棒，但改造面大于一行（循环 + SLEEPQ 探查 + 帽），探针从纯观察者变链参与者须过调度线自审；归 ktest 面翻新或 #84 BCTS 深化窗。

## Consequences

- 修后复验（本轮实跑，`/tmp/workshop47/`）：①修前对照臂（HEAD b05d894c4 无修内核 + 250Hz 盘）INTENT ok 缺席 + 断言行 `it_probe: 时间帽计数须 ≥2` 在——P0 在当前 HEAD 复现；②三档 boot（drill44 GUI 形拓扑）INTENT ok 全到：default 7.793s（10Hz 恒等带）/250 6.709s/1000 6.682s，eco 行 `slice_hz=250 slice=250000`、`slice_hz=1000 slice=62500` 单源换算直证；③BCTS 250Hz 镜像腿 `bcts-run --image` → **verdict = PASS（32/32；missing = 无）**——修前同形态 FAIL 缺 bare-24，跨线冲突解除实证。
- +368ms 标记延迟（250 档窗 32→400ms）对 240/360s 判定窗可忽略。
- INTENT 链内「以片数代时间」的等待就此消一处；事件驱动正形（案 c）留档见上。

## Related

- 建议案与三案利弊全文：[../../../research/厂商定制化深研/46-缝隙清偿轮-P0修窗建议案与P1三件落修.md](../../../research/厂商定制化深研/46-缝隙清偿轮-P0修窗建议案与P1三件落修.md)（§4）
- 咬点首发：44 号 §2-1（同目录 `44-终极演练-全能力综合.md`）
- 关联常量：`kernel/src/main.rs` `task_intent_slow` 200ms 睡（魔数对另一端）；先例 = `it_sinkB` 墙钟期限/wait_one「不赌时序」
