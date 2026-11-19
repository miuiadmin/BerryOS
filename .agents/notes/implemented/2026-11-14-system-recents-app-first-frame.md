# SYSTEM Recents 与 APP 首帧反馈

Status: implemented

## Problem

P1 只证明 BACK/HOME/TASK 的协议链与 WM 表变化可见：TASK 仍是无 UI 的“轮换下一个 owner”，用户看不到任务列表，也不能选择任务；launcher 的 `Opening...` 只按 8 秒超时消失，成功路径仍不知道 app 第一帧何时真实合成。

## Decision

把 Recents 与首帧完成都放进 SYSTEM 层，不让普通 app 获得新能力：

- `berry-guiproto` add-only 增加 `0x6C OP_WM_TASK_SELECT` 与 `0x6D OP_WM_RECENTS_GET`；二者均仅 SYSTEM session 可用。`WmRecentOwners` 返回表序去重的 APP owner，隐藏 owner 仍是可恢复任务。
- `gui-wmd` 的 TASK 键不再先抢走 APP 状态，而是发送携带 `SYSTEM_NAV_ACTION_TASK` 的 `PE_FL_SYSTEM_INVALIDATE`；launcher 动态创建全屏 `WM_LAYER_SYSTEM` Recents 窗，显示权威 owner 卡与真实 app 标签。选择卡片经 `OP_WM_TASK_SELECT` 回到 wmd，由 WmTable 显示并聚焦选中 owner、隐藏其他 APP owner。
- `relay::submit_composite` 返回真实提交结果。APP owner 第一次成功合成 frame 后，wmd 用 `PE_FL_SYSTEM_APP_FRAME`（x=owner）唤醒 SYSTEM launcher；launcher 立即清除 Opening/pressed，并把该 owner 绑定到刚点击的 app 标签，供 Recents 卡复用。
- Recents 是系统模态。显示时摘除当前桌面页的 12 个 a11y 挂载并挂任务卡；关闭时恢复桌面页挂载。`MAX_NODES` 保持 512 契约，不靠扩容掩盖状态生命周期错误。

## Alternatives considered

- 宿主截图/假任务面板：拒绝；TASK 必须来自 WmTable 权威状态并经 WM 选择回到客机。
- 继续 TASK 轮换：拒绝；用户无法感知任务列表，也不知道将切到哪个 app。
- 用 8 秒超时当作启动成功：拒绝；成功反馈必须由真实 APP frame 合成事件清除。
- 抬 `MAX_NODES` 或 a11y 表容量：一度把节点帽抬到 576/640 仍见 `NodeFull`；诊断证明真正满帽是隐藏 Recents 仍占用 a11y 槽。恢复 512 并做模态挂载交换。

## Consequences

- 产品 1080×1920 HVF + virtio-gpu 实测：Berry Demo 点击 → `WMAPPFRAME ok owner=2` 清 Opening；TASK → `WMRECENTS ok id=2` 深色系统 overlay；点卡片 → `WMTASKSEL ok owner=2` 并恢复 app；HOME 仍回桌面。像素检查确认 Recents 背景 `#12161C` 与卡片 `#242C36` 在席，无 panic。
- `berry-guiproto` roundtrip/reject、`gui-wmd` host 8/8、BerryUI 全套、`just check`、`smoke-image` 147/147、GUI8 vgpu `--only-a` 全绿；1080×1920、HVF、virtio-gpu 与 Berry Room 权限边界不变。
- 本刀只建立 Recents 权威 owner 卡与当前 app 恢复；多 app 从隐藏态恢复仍需要 wmd 保留/重放该 owner 最近真实 frame（或 app 配置重绘契约），滑动 90–160ms 大帧提交也仍未解决，不能宣称 60fps 或完整多任务终态。

## Related

- [GUI interaction quality P1](2026-11-13-gui-interaction-quality-phase1.md) established the navigation wake-up and truthful Opening state; this note extends TASK into a visible SYSTEM surface and replaces the timeout-only success path.
