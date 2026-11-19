# Recents owner frame cache

Status: implemented

## Problem

SYSTEM Recents owner 卡已经可见可选，但全屏 overlay 的 SUBMIT 会覆盖 wmd 的单层 composition VMO。选择一个隐藏的静态 APP 时，只改 WmTable 可见性不会让该 APP 自动重绘；用户可能看到残留 overlay 或桌面像素。此前只能诚实地把它列为未完成边界。

## Decision

把最近真实 APP 帧缓存在 wmd 内部，不扩 wire、不给普通 app 新能力：

- `WmTable::owner_windows(owner)` 导出 APP 窗口 `(id, raw, eff)` 布局指纹。cache/replay 只有指纹完全一致时才命中；隐藏期改几何、建窗或销毁窗都不重放旧像素。
- `Compositor` 保存 `fresh_owner`：APP submit 成功后该 owner 的真实像素在 composition 中。他人 CREATE/SET/SUBMIT 覆盖前，wmd 先按 fresh owner 的有效窗口把 composition 像素拷到 per-owner VMO。LRU 帽 3 个 owner（约 23.7MiB@1080p），未命中保持“等 app 自绘”，不伪造。
- `OP_WM_TASK_SELECT` 仍由 SYSTEM launcher 独占。launcher 先隐藏 Recents SYSTEM 窗，让 WmTable 重算非空 effective bounds，再请求 task_select；wmd 显示/聚焦目标 owner，并在指纹命中时把 owner VMO 写回 composition，经现有 present surface0 提交。提交前后排干 held aliases，连续切换不受 credits=2 影响。
- APP owner 死亡时 `reap_dead` 关闭其 cache VMO；owner 显式销毁全部窗口后不再出现在 Recents，旧 slot 由 LRU/死亡释放。

## Alternatives considered

- 桌面或宿主截图伪造任务帧：拒绝；缓存必须来自 APP 曾成功 submit 进 wmd composition 的真实像素。
- 让 app 在 SHOW config 后自绘：静态 Berry Room app 不轮询 config，不能作为系统任务切换的正确性前提。
- 每个 APP 直接持有 present/HDI 能力：拒绝；present 仍只归 wmd 显示服务。
- 无限缓存全部 owner：拒绝；6 理论 owner 约 47.5MiB，LRU 3 是内存与体验折中。

## Consequences

- 1080×1920 HVF + virtio-gpu 双 APP e2e：BerryDemo 首帧 → HOME → Settings 首帧 → TASK 捕获 owner3 → 选择隐藏 owner2 → `WMREPLAY ok owner=2`，选择后像素回到 APP 面、无 panic。基础 Recents e2e、smoke-image 147/147、GUI8 vgpu `--only-a`、gui-wmd host 8/8、`just check` 全绿。
- 缓存 VMO 增加最多约 23.7MiB guest committed 内存；产品 256MiB 与 128MiB smoke 均实测通过。
- 隐藏期几何变化/新窗仍故意 miss，等待 app 消费 config 后自绘；这是正确性优先。滑动整窗 transfer 瓶颈仍与本刀独立，不能借机宣称 60fps。

## Related

- [SYSTEM Recents and APP first frame](2026-11-14-system-recents-app-first-frame.md) established the visible owner cards and first-frame event; this note closes the static hidden-owner restoration gap it explicitly left open.
