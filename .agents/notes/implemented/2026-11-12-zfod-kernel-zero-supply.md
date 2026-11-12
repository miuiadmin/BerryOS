# Agent Note: ZFOD 匿名零页内核直供——判定条件从创建面移到供给面学习

Status: implemented

## Problem

路线图 P1-3：demand 缺页链（EL0 缺页 → probe_el0 武装 → 唤 pager → pager 0x21/0x23/0x24 供给 → 唤 faulter → 重试）最坏 2 个调度片 + 多次 IPC 往返，但多数缺页是匿名零页首触（bss/堆/栈类——X-5 demand 装载的零窗），页内容本就该是零，pager 提供不了任何内容。要求内核直供零页消掉这段往返。

前置调查结论（本笔记的核心决策输入）：**需求 VMO 的 metadata 不区分匿名/文件后备**。`Vmo` 对象上只有 `demand: bool`；berry-exec 的 file 窗与零窗同形创建（`vmo_create(len, VMO_OPT_DEMAND)`），种类区别只活在 svcmgr 用户态窗登记表（`PagerReq{dv,sv,delta}` 的 sv=rootfs vs sv=0）——内核侧在创建/映射时点无任何可判信号。任务书的 blocked 条款（「metadata 不区分则报告 blocked」）字面命中。

## Decision

不报 blocked，把判定条件从**创建面**移到**供给面学习**：`Vmo` 增对象级 `zfod: bool` 标志，置位点 = `pager_supply_one` 的**零臂**（`sv.is_none()`——0x21/0x23/0x24 的 `src=0`）供给成功时（`kernel/src/syscall.rs`）；读取点 = `probe_el0` 命中 demand 区间后（`kernel/src/dpg.rs`）——标志真则 KL 窗内直接 `supply_page`（出帧清零 + 填叶 + tlbi/broadcast），探针返 true、eret 重试命中，零 pager 往返；直供失败（帧耗尽）降级续走原 arm 链兜底。

论证骨架：零臂的 ABI 语义就是「供给页 = 纯清零页」（ABI §0x21 词面），pager 用零臂喂过这枚 VMO = pager 亲证「本 VMO 无内容后备」——这是内核侧唯一能观察到该事实的权威时点。ZFOD 直供的页（`supply_page` = alloc + 清零 + 按区间权限填叶）与零臂产物**逐字节等价**，故对零窗语义零漂移；file 窗（sv=rootfs）永不置位、原链零漂移。EL1 面（`probe`）刻意不加：内核 dpg_pager 供给后写 magic（有内容语义），其 VMO 永不经供给面，加快路径是纯死码。

采用任务书的「简化方案」（每次首触分配新零页、非共享），不走全局零页 + COW：v1 无 COW 基础设施，且 `supply_page` 整链现成复用（幂等门/记账/tlbi 全继承），零新机制面。

## Alternatives considered

- **报告 blocked（严格按任务书条款）**：被拒——缺口真实存在但只堵死「创建面声明」一条路；供给面学习在内核侧三文件内（dpg.rs/vmo.rs/syscall.rs，无 ABI/用户态改动）即可给出语义等价的判定条件，收益照拿。
- **vmo_create 加 options bit（如 bit1 `VMO_OPT_DEMAND_ZERO`）**：被拒为 v1 刀面——真 metadata 正形，但要动 tools/abi.toml + berry-sys 绑定 + berry-exec 三色窗分流 + rootfs 重建，远超「只改 dpg.rs/vmo.rs 或类似」的刀面；留作后续正形（见 Consequences 挂账）。
- **svcmgr 把窗登记注册进内核（新 syscall 或激活 v3 预留 0x22）**：被拒——同样越刀面（ABI + 用户态双改），且为快路径引入 pager 注册时序依赖（登记前首触走谁？），复杂度不成比例。
- **全局零页（Linux 同款）+ 写触 COW**：被拒——v1 无 COW；且现探针 fsc 滤只收翻译缺页（4..=7），写触只读零页产生的权限 fault（fsc 1..=3）走原杀链，COW 需另开处理面。
- **按「首个零臂供给前」逐页启发（未标记即慢路径）**：这就是本设计的实际形态——首次仍慢是学习成本，换来的是此后整窗快路径；单窗单页的极端（如 musl 族 1 页零尾）退化为零收益零损失。

## Consequences

- 匿名零页首触（零窗第 2 页起 + 运行期 bss 扩展）从「2 调度片 + 多 IPC」折叠为单异常往返；每零窗首次缺页仍走 pager（学习点）。
- **设计契约「一 VMO 一种类」**：标志置位即终身，要求 pager 对单 VMO 供给种类恒定。现役消费面结构成立（berry-exec 一窗一 VMO、svcmgr 一 dv 一 (sv,delta)、全树无同 VMO 混臂流——dpgp/DPGS/DPGN/dpgf/bench/foot 全 sv=真源，零窗族恒 sv=0）；假想混种类 pager（先零臂后内容臂）的后续缺页会被直供零页且不再武装，此为该形态的语义收窄（v1 无此形态）。
- 置位判据收紧到「零臂**且供给成功**」：有源臂（含源读失败形——DPGN ③ `-EBADSTATE` 子恒唤收零页 exit 9）不置位，该测后续缺页仍走 pager 链，断言面零漂移。
- ktest E3h 的 sv=0 批供（vmo_g/vmo_h）会置位，但两 VMO 随即 unmap+close 销毁——无后续缺页面，零干扰；此后新写的内核测若依赖「零臂供给后再次缺页仍武装」需知此契约。
- 挂账（后续正形）：若日后要消灭「首次仍慢」或支持混种类 pager，走 options bit（创建面声明）或 pager 窗注册面；届时 `zfod` 学习点可平滑退役为声明点的兜底。
- 验证：`just smoke` 全相（裸相 DPG ok + rootfs 相 DPGP/DPGS/DPGN/HEXYL ok + MMAP ok 全标记）——见笔记同批提交的改动与冒烟日志。

## Related

- [系统调用ABI设计-v1.0.md §0x21/0x23](../../../docs/系统调用ABI设计-v1.0.md)（零臂词面） · [G4按需调页实施设计草案-v1.0.md](../../../docs/G4按需调页实施设计草案-v1.0.md)（原链设计） · [内存管理设计-v1.1.md](../../../docs/内存管理设计-v1.1.md)
