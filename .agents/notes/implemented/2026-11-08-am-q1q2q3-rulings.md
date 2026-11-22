# Agent Note: am 三裁决（Q1 IPC×冻结 / Q2 discardable 落点 / Q3 debounce 与豁免）

Status: implemented

## Problem

[am 总体设计 v0.1.0](../../plans/am/design.md) 立项时留三条开放问题交拍板：Q1 同步 IPC 打冻结进程的语义（-EAGAIN vs 唤醒即解冻）、Q2 discardable 是 VMO 第 6 类后备还是匿名类标志位、Q3 冻结 debounce 与豁免面初值。仓主授权代理拍板，口径 = 「最强大、最优秀的方式」。

## Decision

- **Q1 = 唤醒即解冻（c），v1 直上、不再 M4+ 延后**。两个一手事实把原先的「b 先行」倾向推翻：其一，BerryOS channel 本就是非阻塞有界队列（send 满拒 -EBUSY、阻塞走 `wait_one`）——到冻结端点的消息天然排队，A14 广播排队形态免费获得，实现成本大降；其二，1.1 安全门已立法「token 持有者死亡 → auto-thaw」——am 崩溃 = 全体解冻 = 退化到无冻结而非死锁，c 的最大风险已被既有设计吸收。机制面 = send 命中冻结端点打 wake-pending + Event 通知 token 持有者（零新 syscall）；策略面 = am 默认立即 close token（解冻裁决权留在用户态）。-EAGAIN 不设新口——`wait_one` 超时即覆盖探活场景，不为罕见需求加 ABI 面。
- **Q2 = 匿名（物理页）类 `DISCARDABLE` 选项位，不设第 6 类后备**：回收策略与后备来源是正交维度（Fuchsia 同款——ZX_VMO_DISCARDABLE 是创建标志不是 backing type）；混入枚举把策略焊进类型系统。锁语义挂 commit/decommit 动词扩展（options 位），零号预算（ADR #68 纪律）。前置：动代码前先升 `内存管理设计` v1.1 补条目。
- **Q3 = 自适应 debounce 三元 + 豁免面缩到长时任务白名单**：基线 10s【推，HDR 实测定标】、压力 WARNING→0（立即冻结）、intent/返回概率调制（钩子 AM-5 接 AI 实验轨道）；豁免面只留 PM-00 已立法的长时任务 reason 白名单——Q1-c 使绑定豁免消失（call 即唤醒，冻结安全），豁免面保持小而可审计（bfcache 教训）。

## Alternatives considered

- **Q1 取 b（-EAGAIN 先行）**：被弃——全员重试 = 生态负担（PM-00「默认值即生态」教训的反面）；且排队免费 + auto-thaw 兜底使 c 的两大成本消失，b 的「实现简单」优势不再成立。
- **Q2 取第 6 类后备**：被弃——策略与来源不正交，枚举膨胀后 evictable/pinned 等回收策略位都要跟着开类，类型系统被回收策略污染。
- **Q3 取固定 10s + 宽豁免面**：被弃——固定值在压力下错档（该立即冻结时等 10s）；宽豁免面（绑定/持锁类）在 Q1-c 后失去必要性，多一级豁免多一级审计债。

## Consequences

- design.md 升 v0.2.0（draft→active），AM-1 增前置内核小刀（wake-pending + Event 通知），AM-1 可即开。
- AM-4 前置多一件：`内存管理设计` v1.1 补 DISCARDABLE 条目（本裁决 Q2 是其依据）。
- wake-flood 防护进 am 遥测（解冻频次 HDR 分位 + 升级处置路径）。
- debounce 基线 10s 是【推】值——AM-1 落地后用 HDR 实测定标，避免 Android 数值盲搬（其 10s 是 cached 态语义，BerryOS 是后台事件语义，语境不同）。

## Related

- [am 总体设计 v0.2.0](../../plans/am/design.md) · [AM-01 调研](../../../research/AM-01-墓碑模式开源对标-v1.0.md) · [调点调度 v1.1 freeze 节](../../../docs/调点调度-v1.1.md)
