# 内核深度进化 11 号：vmarface 入册清单预备——回流义务 6 深化（#171 判例族全量对账）

Type: research
Status: done
Updated: 2026-11-19

一句话说明：05 号件回流义务 6 预判 berry-vmarface（overhaul 新建 crate）回流时须「入册（members/白名单/justfile/unsafe 台账），按 #171 判例族全量对账」——本件把该义务落为**逐触点对拍清单**：以 #171（wsrcface 净增量重放）与 Q38 族第十四例（b046aeb22 布局树补登）为先例锚，两线实测八触点，结论——**六触点零手术自动并、两触点手工接（justfile 一行 + 台账账面行核对）**，另抓 overhaul 线自身债一笔（白名单漏登 vmarface，回流时须 W3 侧代偿补笔）。

## 先例锚（#171 判例族入册触点清单）

#171（wsrcface，fa7af998b）确立的入册面 = 五接线点 + 三 docs 面：①Cargo.toml members；②kernel/Cargo.toml deps；③justfile test-host 挂测行；④tools/sync-public.sh 白名单（**先登防红判例**——白名单不入册则 check-members 即红，test-host 前置门）；⑤Cargo.lock 随产；docs 面 = ⑥unsafe 台账（若 crate 在扫描面）⑦docs/规则/README.md 布局树 face 列表（Q38 族影抄纪律）⑦'07-测试与冒烟口径行（test-host 组数随挂测滚新）＋沿革节。

## 两线八触点对拍实测（base 0631260ed vs overhaul 492d5e00c，merge-base fceb03ff3）

| # | 触点 | 两线分叉 | merge-tree 态 | 回流动作 |
|---|---|---|---|---|
| 1 | Cargo.toml members | base −asidface；overhaul +vmarface | **不在冲突单**（异区自动并） | 零手术：并流自动取并集，vmarface 随 overhaul 侧进 |
| 2 | kernel/Cargo.toml deps | base +berry-vdsoface；overhaul +berry-vmarface | **不在冲突单**（异区自动并） | 零手术：两行并存自动并 |
| 3 | Cargo.lock | 双侧均改（成员增删） | **冲突**（机械冲突） | 机械解：`cargo metadata`/`--locked` 重产即平（05615c1e7 先例——union 脚本不碰，cargo 自产） |
| 4 | justfile test-host | overhaul 于 asidface 行后插 `cargo test -p berry-vmarface --target {{host}}`；**base 侧 asidface 行已拆**（asidface 目录孤儿化，F0-2 二级合龙收口） | **冲突**（10 文件冲突单成员） | **手工接一行**：`cargo test -p berry-mm` 行后直插（现邻 `berry-wqmatch` 注释块之前），不复活 asidface 死行 |
| 5 | unsafe 台账 | vmarface **零 unsafe**（lib.rs 747 行 unsafe 仅文档注释 1 处命中）且**不在扫描面**（台账扫描面 = kernel/ mm/ sched/ ipc/ arch/，face crate 全族不入账） | 台账冲突源自 kernel/ 侧行数漂（义务 5 syscall 手术的随刀重产面） | 零手术：vmarface 本身无台账义务；台账重产随义务 5 手术批走 |
| 6 | writable-globals | wg 扫描面同 = kernel/ mm/ sched/ ipc/ arch/，vmarface 零 static mut | 不涉 | 零动作 |
| 7 | tools/sync-public.sh 白名单 | base 白名单闭合（44 员对账，--check-members 机器门执法）；**overhaul 白名单零 vmarface 笔**（overhaul 线自身债——该线未跑 check-members 门） | **不在冲突单**（overhaul 侧零改动自动取 base） | **手工补一笔**：白名单 vmarface 补登（wsrcface :268-270 先登防红判例同款），随回流合并随刀 |
| 8 | docs/规则/README.md 布局树 | base 侧 face 列表 35 员（含 asidface 尾注）；overhaul 侧未动布局树 | 不在冲突单 | **手工补一笔**：face 列表 +vmarface（vmarface = 层级 Vmar 有序区间树/子区间切分/子空间回收纯状态机〔F1-1 overhaul 线新建，随回流入库〕），face 计数与 members 闭合核对（Q38 族影抄纪律） |

另：07-测试与冒烟.md 口径行（test-host 组数）随 justfile 挂测行 +4 组同步滚新（asidface 12 测已拆的前提下净增 vmarface 4 host 测——07 头条组数按合并后实跑滚新，单物理行巨文件 patch 纪律见 W1 深测坑）；沿革节随批。

## vmarface 资产面速记（入册对账基准）

- berry-vmarface：no_std + alloc、**零 unsafe** 的 BTreeMap 有序区间树状态机（F1-1）；747 行含 4 host 测；`[dependencies]` 空段（纯函数面无内核依赖）。
- base 线对应消费 = vmar.rs −1114 区间树重构后 `VmarTree/check_slot/destroy_vmar/RemovedSpan`（05 号件②对撞重心）；overhaul 引用 2 处（kernel/vmar.rs + ktest.rs F1-1 自检）。
- 与 vslotface 语义重叠面（区间合法性 vs 槽位三查）的归并裁定单源在 07 号件——本件不重裁。

## 定谳

1. **入册八触点中六零手术**：members/deps/白名单冲突面/wg 全部自动并或零涉——05 号件预判「members/白名单/justfile/unsafe 台账」四手术面实测收敛为**两手工点**（justfile 挂测行 + 白名单补笔）+ docs 影抄两笔（布局树/07 口径行）。
2. **overhaul 线自身债代偿**：白名单漏登 vmarface 回流时由 W3 侧随刀补（不要求 overhaul 线补——线已停更 2026-11-18 20:01，代偿成本一笔）。
3. **执行时点 = 回流合并随刀**（与 07 号归并裁定、08 号 syscall 六段手术同批），不预先改 base 线。
4. 本件零代码面：纯研究件，base 线零改动。

## 与其他文档的关系

- 义务单源：[05-base-overhaul回流接缝预研-W3面资产对撞审计.md](05-base-overhaul回流接缝预研-W3面资产对撞审计.md)（义务 6）。
- 归并裁定：[07-vslotface归并裁定预备-三查判序两形恒等性对拍审计.md](07-vslotface归并裁定预备-三查判序两形恒等性对拍审计.md)。
- syscall 手术图：[08-syscall逐段对拍清单预备-回流六冲突段施工图.md](08-syscall逐段对拍清单预备-回流六冲突段施工图.md)。
- 台账补遗：[10-bench测量面内联段查实与回流接缝补遗.md](10-bench测量面内联段查实与回流接缝补遗.md)（10 文件冲突单）。
- 先例：#171（fa7af998b wsrcface 接线四点）、b046aeb22（Q38 族布局树补登）、05615c1e7（Cargo.lock 机械解先例）。
- 沿革：`docs/阶段沿革.md` 六百一十一节（随本批）。
