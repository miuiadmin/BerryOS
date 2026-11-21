Type: standard
Version: 1.0.0
Status: current
Updated: 2026-11-16

# BerryDisplay-SoftwareLTPO-P1-v1.0：软件 LTPO 帧率仲裁

## 1. 范围

本规格约束 1080×1920@60 QEMU/virtio-gpu 产品相的显示服务软 vblank 帧率仲裁：

- VOTE 票面、SUBMIT 活跃度与 POWER_MODE 的聚合规则。
- 60→30→10→1Hz 的不活跃降档与提交即升档。
- QEMU 不可硬件 mode-set 时的诚实边界。

它不改变 present wire、HDI 权威、Berry Room 能力模型或 UI 引擎“产票不仲裁”的分工。

## 2. 单源策略

纯函数单源：

```text
user/berry-guiproto/src/ltpo.rs
pub fn software_ltpo_hz(...)
```

vgpud 持有状态并消费该函数；BerryUI 只继续产出 `VoteBallot`，不得直接设定显示频率。

## 3. 输入与输出

输入：

```text
min_hz / max_hz / preferred_hz  来自 VOTE
sticky                         VOTE 的停帧粘滞位
power_mode                     NORMAL / DIM / AOD / OFF
idle_ns                        最近一次真实 SUBMIT 至今
```

输出只允许：

```text
0 / 1 / 10 / 30 / 60 Hz
```

`0` 表示 OFF，显示服务不得调度软 vblank。

## 4. 仲裁规则

1. 本拓扑硬上界为 60Hz；交互票 `{60,120,120}` 仲裁为 60，不宣称 120。
2. 普通/动画票新鲜期保持 60。
3. 无真实 SUBMIT：
   - ≥250ms：降至 30。
   - ≥1s：保持 30。
   - ≥3s：降至 10。
4. sticky idle 或 `{0,0,0}` 票：逻辑屏保持 1Hz maintenance cadence；若无人 arm/park vblank，vgpbd/vgpud 结构性零唤醒。
5. POWER_DIM 封顶 30；AOD 固定 1；OFF 固定 0。
6. UI ballot 的 `min_hz` 不能被静默丢弃，但仍被 60Hz 显示帽与 DIM 电源帽约束。

## 5. vgpud 状态机

vgpud 记录最近票面、电源模式、最近真实 SUBMIT 与当前 cadence：

- 真实 SUBMIT 成功进入渲染/排队面后记为活跃，并立即把 cadence 升到策略结果。
- 每次软 vblank tick 用当前 idle 年龄重算；降档在下一拍生效。
- 升档时若已有远期 1Hz/10Hz slot，立即重排到新周期，避免交互起振等待一秒。
- cadence 变化打印：

```text
VGPULTPO ok hz=<hz> old=<old> reason=submit|vote|power|age
```

## 6. 诚实边界

QEMU virtio-gpu 没有物理 LTPO 面板 mode-set。本规格实现的是 LTPO 的软件策略与省电纪律，不能宣称：

```text
硬件 LTPO
面板 1–120Hz 自适应
真实屏幕功耗下降数值
```

真实面板接入后，接口语义保持 vote/rate_hint/power_mode；只允许后端把软件 cadence 映射到硬件 mode-set。

## 7. 验收

- berry-guiproto host：交互封 60、新鲜 60、250ms/1s/3s 降档、sticky/AOD/OFF、DIM 电源帽、ballot 下限不丢。
- product e2e recents：串口必须同时出现 hz=60、hz=30、hz=1 的 `VGPULTPO` 真值。
- 既有 11 点 BootProgress、edge-to-edge 系统栏、Recents/HOME 链零回归。
