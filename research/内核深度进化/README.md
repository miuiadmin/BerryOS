# 内核深度进化（2026-11-17）

> 四路并行审计（内存管理/IPC 快路径/syscall 分派/调度热路径），找 Top-5 优化机会。

| # | 文件 | 主题 |
|---|---|---|
| 01 | `01-内存管理优化审计.md` | 分配器/VMO 页表遍历/退出回收——Top-5(含 zero_frame DC ZVA 快赢) |
| 01a | `01-机会3立项前置研究-进程退出回收双线对撞审计.md` | 机会 3 立项前研究：base-overhaul 线 F1-2 已承接，W3 不立项——01 号收官账 |
| 02 | `02-IPC快路径审计.md` | recv 10x walk/PI 重推门/消息环形化——Top-5 |
| 03 | `03-syscall分派审计.md` | 诊断残留退役/KERN_LOCK 归零/SVC 包络测量——Top-5 |
| 04 | `04-调度热路径审计.md` | preempt 门控/诊断退役/per-CPU 原子——Top-5 |
| 05 | `05-base-overhaul回流接缝预研-W3面资产对撞审计.md` | base-overhaul 回流时 W3 face crate 群对撞四态定级＋回流义务清单 |
| 06 | `06-recv_n重放形态预研-ChannelShared接缝审计.md` | 回流义务 3 深化：消费点六接缝施工图 + disarm 双重释放风险钉板 |
| 07 | `07-vslotface归并裁定预备-三查判序两形恒等性对拍审计.md` | 回流义务 2 深化：三查判序两形 200k 样本恒等对拍＋退役/保留/迁移裁定 |
| 08 | `08-syscall逐段对拍清单预备-回流六冲突段施工图.md` | 回流义务 5 深化：syscall.rs 回流六冲突段施工图（3 overhaul 权威 + 2 base 权威 + 1 组合重放）＋KLOG 窗缘/权利域两域裁定 |
| 09 | `09-handle表伸缩面审计-allocO(n)线扫与gen耗尽复合退化定谳.md` | 新审计面三候选查实（SMP/boot 不立项定谳）＋handle 表三退化轴：线扫×churn×gen 耗尽复合——候选刀 A 游标 hint/B gen 扩容，留 owner 裁定 |
| 10 | `10-bench测量面内联段查实与回流接缝补遗.md` | bench.rs 内联段不立项定谳（测量面物理臂）＋05 号回流审计补遗：bench.rs 分叉 177 行但自动合并零冲突、依赖闭包全活——base 权威正和件零手术 |
