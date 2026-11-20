# 内核深度进化 10 号：bench.rs 测量面内联段查实＋base-overhaul 回流接缝补遗（05 号审计缺口定谳）

Type: research
Status: done
Updated: 2026-11-19

一句话说明：补货梯度候选「bench.rs 996 行内联段扫描（Q61/Q62 查实纪律）」执行完毕，两件产出定谳——其一，**内联段查实不立项**：bench.rs 无纯函数判定面可外迁（计时骨架 `bench`/`bench_fa` 是测量物理臂本体——被测对象的成本就在副作用与系统调用里，抽离即毁测量；xh/churn 六面断言全是水位恒等/错误码精确这类「副作用即语义」金丝雀，host 侧无对拍物）；其二，**05 号回流审计实锤漏了 bench.rs——本件补遗**：`git diff` 两线实测 bench.rs 分叉 177 行（+20/−157——overhaul 线零吸收 W3-Q98 send_recv_pair 与 W3-Q102 svc_roundtrip 两基准项及 N_METRICS 8→10 演进），但 `git merge-tree --write-tree` 实跑两线自动合并**零冲突**且依赖闭包全活（CHAN_DEPTH 双线同形、svc4/svc6、NR_* 导出、Vmo 五面、Fair::pick_next_filtered、Topo::single 均在合并树），bench.rs 属「base 权威自动保留」态非手术面——唯 tools 三件套（bench.py/markers.sh/smoke-image.sh）随 N_METRICS 面同向自动保留，`BENCH ok n=10` 标记字面无需手术；另抓 base 自身注释陈旧债一笔（main.rs:1132「n=8；配对修复主线收」——bench.rs 侧 W3-Q98/Q102 已把 N_METRICS 滚到 10、悬账 2026-11-18 已收，main.rs 注释未随迁）随刀修复。

## 查实基线（2026-11-19 22:0x 直核）

- base 线 = `origin/wip/sched-1.1@0631260ed`（bench.rs 996 行）；overhaul 线 = `origin/wip/base-overhaul@492d5e00c`（bench.rs 819 行）；`git merge-base` = fceb03ff3。
- 内联段扫描全量：模块头口径注（:1-33）/N_METRICS 与 S-2 常量（:49-94）/计时骨架 bench·bench_fa（:135-152/:276-305）/十基准项（:153-465）/pick_scan+send_recv_pair（:337-465）/AM 对（:264-275）/sb_task 对（:474-536）/run_sched_epoch（:538-558）/churn 三金丝雀（:560-662）/xh 六面（:663-996）。逐段读过，无一段是「纯判定律可剥壳外迁」形态。

## 定谳一：内联段不立项（Q61/Q62/Q41 同款关闭）

- 计时骨架（`bench`/`bench_fa`）= CNTVCT 读 + 循环 + UART 打行的物理臂——抽离 host 即无测量物。
- 十基准项每项 = 被测子系统真调用（alloc/free、VMO commit/rw、svc4/svc6 直调、pick_next_filtered 直调）——被测成本就在调用与拷贝里，与 ktest 直调面同判（W3-Q99/Q100/Q101 判例：直调物理臂不外迁）。
- churn 三金丝雀 + xh 六面 = 断言全绑 `mm::frames().allocated_pages()` 水位恒等与错误码精确——「副作用即语义」，host 无对拍物；static mut XH_* 族已按 raw 指针纪律在册（台账），非债。
- 结论：bench.rs 是测量基建本体，无 face crate 外迁面。后续若加基准项，按 N_METRICS 头注五处同步律走（markers.sh/smoke-image.sh/smoke.sh/bench.py/bcts）。

## 定谳二：05 号审计补遗——bench.rs 是漏审的回流接缝（本件主产出）

05 号审计（`05-base-overhaul回流接缝预研-W3面资产对撞审计.md`）对撞重心定级覆盖 vmar/channel/syscall 三处，**未及 bench.rs**——本件实测补遗：

- **分叉实测**：`git diff base overhaul -- kernel/src/bench.rs` = +20/−177 行。overhaul 线无 send_recv_pair（W3-Q98）、无 svc_roundtrip（W3-Q102）、N_METRICS=8、AM 头注少两项锚、pick_scan 铺展注释形旧（vr 直改形 vs base 勘正注形）、迭代 128 注释 SOAK_PRINT_EVERY 旧档形。
- **自动合并实跑**：`git merge-tree --write-tree`（tree f352114d）exit=1（全仓 10 文件冲突——Cargo.lock/台账/justfile/aspace/channel/dpg/ktest/syscall/user/vmar，与 05/08 号清单一致）**bench.rs 不在冲突单**，自动取 base 侧全量（N_METRICS=10、双基准项、26 处 send_recv/svc_round 命中全活）。
- **依赖闭包全活核对**（合并树逐项 grep）：`channel::CHAN_DEPTH`（双线同形导出）/`syscall::NR_*` 七常量 + `VMO_MAX_SIZE`（合并树 :87/:176）/`ktest::svc4/svc6`（:2758/:2795）/`vmo::Vmo` 五面（create/commit/read/write/size）/`Fair::pick_next_filtered`（sched/class.rs 7 处）/`Runqueue::resync_vruntime_sum`/`Task::new_kernel_with_as`（:293）/`Topo::single`（runqueue.rs）/workspace members 全量含 sched·chanface·vmarface·vslotface。
- **tools 三件套随行**：bench.py/markers.sh/smoke-image.sh 两线同向分叉（n=8 vs n=10 + 双基准项），均非冲突单，自动取 base 侧；`BENCH ok n=10` 标记字面（markers.sh MARKERS 数组）在合并树保留——smoke 收割契约零手术。
- **定谳**：bench.rs 族回流态 = **「base 权威自动保留」正和件**（05 号四态定级的正和态），无 W3 侧手术义务；05 号义务清单 1-6 无需增补。

## 随刀债修（base 自身注释陈旧）

main.rs:1132-1135 注释两处陈旧：①「n=8」——bench.rs N_METRICS 已随 W3-Q98（8→9）/W3-Q102（9→10）滚到 10，trailer 实发 `BENCH ok n=10`；②「配对悬账主线收」——悬账 2026-11-18 已收（bench.rs :533「配对悬账已收」头注与 main.rs 注释相抵）。注释随实况改写（n=10 + 悬账已收指针），零代码面。

## 与其他文档的关系

- 05 号回流审计（本件补遗对象）：[05-base-overhaul回流接缝预研-W3面资产对撞审计.md](05-base-overhaul回流接缝预研-W3面资产对撞审计.md)。
- 查实纪律先例：[../../.agents/ 之外统一引用 QUEUE.md W3-Q41/Q44/Q61/Q62 查实关闭条目]（W3-Q61 环不存在查实、W3-Q62 process.rs 无状态机余量查实——本件同款「前提不成立即查实关闭」）。
- N_METRICS 五处同步律单源：kernel/src/bench.rs :53-61 头注（本件「不立项定谳」节引）。
- bench trailer 契约：tools/bench.py（len==done_n 收割）+ tools/markers.sh（`BENCH ok n=10` 字面）。
- 沿革：`docs/阶段沿革.md` 六百一十节（随本批）。
