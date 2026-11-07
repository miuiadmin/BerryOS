# PM-15 电源API跨系统对照与berry-sys草案（v1.0）

> 调研日期：2026-11-07 · 方法：仓内精读（SDK-01/02/03 结论节、PM-00..12、`user/berry-sys`+`berry-manifest`+`berry-psproto` 现有 API 风格盘点）+ 联网取证（WebSearch/WebFetch 抓 Android developer/Apple 官方页与 OpenHarmony GitHub 官方 md 全文，中英文并用）。
> 确定度图例：**高** = 官方文档/一手源码；**中** = 二手转述或社区实测；**低** = 推断或单一来源。
> 系列分工：PM-15 = **函数级对照 + SDK 立法形状**——把 PM-00 §5.2 契约十一项（`research/PM-00-全系统省电架构综合裁决-v1.0.md:248-262`）与 PM-02 启示 1–10（`research/PM-02-iOS与鸿蒙省电哲学对照-v1.0.md:285-296`）压到签名/错误码/能力位/默认值层；机制证据不重查（归 PM-00..12）。PM-13（SoC 平台）/PM-14（充电电池）为三期并行刀，本文 §2.6 充电面按「PM-14 落稿后对表」标注。SDK 文档实位于 `research/`（SDK-01:749 行/SDK-02:941 行/SDK-03:282 行），非 docs/ 下。

---

## §1 主线全景

六面共同规律：**现代三家全部收敛到「窄入口 + 白名单/配额 + 系统校验」，且每家的 API 形状就是它的滥用面**（PM-00 §3「形状决定命运」）。iOS 的招数是「面不存在」（APNs 无客户端订阅 API、充电控制无 API）；Android 的招数是「先全给后叠权限/配额补丁」（SCHEDULE/USE_EXACT_ALARM 双权限 = 补丁形状化石）；OH 的招数是「出生即收紧 + 数量硬帽」（30/2000/12000 提醒帽）。BerryOS 六面全部走「**服务 op + 能力位 + 配额显式**」，零新 syscall。

| 面 | Android | iOS | OH | BerryOS 归属（裁决） |
|---|---|---|---|---|
| ①定时/闹钟 | AlarmManager 五档 + 双权限 | BGTaskScheduler 排期（时刻只是 hint） | reminderAgentManager（数量帽） | berry-power ALARM op（三档×双钟域×wake 能力位） |
| ②推送 | FCM token/topic | **无客户端 API** | Push Kit getPushToken | pushd（唯一长连接）+ register/subscribe |
| ③长时任务 | 前台服务 type 化（14 起强制） | UIBackgroundModes 白名单 | continuousTask reason 白名单 | berry-power TASK op（reason u8 + 租约） |
| ④传感器 | batching/FIFO/flush 全套 | 直推 + 录后取批（Recorder/Batched） | `sensor.on{interval}`（无批量参数） | sensorsd（订阅带 max_latency + flush 逃生门） |
| ⑤热状态 | 7 档 + headroom 预测 | thermalState 4 档只读 | ThermalLevel 0–7 | berry-thermal（u8 四档只读） |
| ⑥电池/充电 | 读属性 + **无充电控制 API** | 读三件 + **无充电控制 API** | batteryInfo 常量 + 广播键 | berry-power BATTERY op（15 字段 + 计量标注位） |

**35 syscall 挂接总裁决（详见 §4.1）：六面零新号。** v1 在册 35 号（`tools/abi.toml:14-16`）无一电源 app 面；PM-00 §2 已裁「服务面 op 不占 ~35 syscall 预算」（`research/PM-00-全系统省电架构综合裁决-v1.0.md:118`）；precise(wake) 闹钟的内核面落在已在册预留的 v2 `timer_create/timer_set`（0x62/0x63，`tools/abi.toml` 时间组）。

---

## §2 关键事实与证据（六面 × 三家：函数/参数/错误/权限/配额/默认值）

### 2.1 定时器/闹钟（对齐 PM-00 三档 + PM-11 §4.2 挂起矩阵）

| 维度 | Android | iOS | OH |
|---|---|---|---|
| 函数名 | `set/setAndAllowWhileIdle`（非精确，API31+ 1h 内送达）；`setWindow`（窗 <10min 被钳到 600000ms）；`setExact/setExactAndAllowWhileIdle`；`setAlarmClock`（「system never adjusts」+ 必要时退出低功耗态） | `BGTaskScheduler.submit(BGAppRefreshTaskRequest{earliestBeginDate}` / `BGProcessingTaskRequest{requiresExternalPower, requiresNetworkConnectivity}`） | `reminderAgentManager.publishReminder(ReminderRequestTimer/Calendar)` |
| 关键参数语义 | triggerAtMillis 绝对；setAlarmClock 携 `AlarmClockInfo{triggerTime, showIntent}` | earliestBeginDate **只是 hint**，系统全权择机 | 触发时间 + 通知按钮 + WantAgent 跳转 |
| 权限 | API31+ 精确闹钟必声明：`SCHEDULE_EXACT_ALARM`（用户授予/可撤/13+ 新装不预授）vs `USE_EXACT_ALARM`（自动授/不可撤/限定用例+Play 政策） | Info.plist `BGTaskSchedulerPermittedIdentifiers` 登记 identifier + Background Modes 开关 | `ohos.permission.PUBLISH_AGENT_REMINDER`（ACL 受限权限） |
| 配额 | `setExactAndAllowWhileIdle` 挂起态 **≈1 次/9min/app**（参考页旧文 + 2025-09 复核文，中）；撤权 → **app 停止 + 全部未来精确闹钟取消** + 状态变更广播 | 排期完全系统侧，无用户可调项 | **三方 app 30 个有效提醒 / 系统 app 2000 / 全系统 12000**；超限报错 **1700002** |
| 默认值 | 非精确为默认；4.4 起所有 repeating 皆非精确 | 系统择机（无「准点」承诺） | 到期走通知中心代理，不依赖进程存活 |
| 错误面 | SecurityException（未声明权限） | `BGTaskSchedulerError`：unavailable / tooManyPendingTasks / **notPermitted**（码值 1/2/3，中） | 1700002 数量超限；权限错 201 |

证据：Android 官方 alarms 页全取回（[developer.android.com/develop/background-work/services/alarms](https://developer.android.com/develop/background-work/services/alarms)，高）；9min 配额（[2025-09 复核文](https://www.google.com/goto?url=CAESqwEBxCJF14Pl3SIQOJhnkVVkiuZv5xHbzQTlCfYCJksaAh_U1i9f7HxYJke1Rkezom-rd61joo19vlii__tdj3z34tg9JlLNc4Xhm8wO7a2FUYFeeQht8c4_6mrKxuBuGVucEADc6D5BvyGykz9415fLuvOqwflCXpuEr6sfpfc9JJF1JUyjOLHxpgVyuKPW4FJd7WZ2ycRF_YepNuE3A2ZBaE5S8L87z80WWE)转 [StackOverflow 49483713](https://stackoverflow.com/questions/49483713/setexactandallowwhileidle-for-alarmmanager-is-not-working-properly)，中）；iOS（[Apple 官方](https://developer.apple.com/documentation/uikit/using-background-tasks-to-update-your-app)，高；notPermitted 成因 [ITNEXT](https://itnext.io/background-app-refresh-in-4-steps-6b0baef45e5a)，中）；OH 30/2000/12000 + 1700002（[CSDN Background Tasks Kit](https://blog.csdn.net/bestadc/article/details/148065471)、[IT营 1700002 帖](https://bbs.itying.com/topic/6907073c4a7a41007019a637)，中）。setAlarmClock prep-work 降背景负载（PM-12 已核，`research/PM-12-二期深研综合与PM00回填表-v1.0.md:133`，高）。

### 2.2 推送订阅

| 维度 | Android (FCM) | iOS (APNs) | OH (Push Kit) |
|---|---|---|---|
| 函数名 | `FirebaseMessaging.getToken()` / `subscribeToTopic(topic)` | **无订阅 API**——只有 `UIApplication.registerForRemoteNotifications()` 拿 device token；topic 是服务端/证书概念 | `pushService.getPushToken()`（AGC 开通后）+ 通知开关 `requestEnableNotification()`（仅弹一次） |
| 通道 | 系统级单长连接（FCM） | 系统级单通道（APNs，2009 起） | 系统级单通道 |
| 权限 | POST_NOTIFICATIONS（Android13+ 通知面） | 推送能力随证书/entitlement | AGC 开通 + 通知授权 |
| 配额 | topic 订阅数有限制（官方未给硬数，中） | 无（面不存在） | 平台侧管控，优先通知权益需审核 |
| token 刷新 | onNewToken 回调 | 系统择机换发 | onNewToken 式回调，Kit 自动缓存重试 |

证据：APNs「客户端无订阅 API、topic 走证书」为 Apple 官方部署指南长期事实（[support.apple.com](https://support.apple.com/en-ng/guide/deployment/dep2de55389a/web)，高）；Push Kit（[华为官方指南](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/push-send-alert)、[HMS Push Kit 页](https://developer.huawei.com/consumer/en/hms/huawei-pushkit)，高）；FCM 单连接省电机理（[Doze 官方](https://developer.android.com/training/monitoring-device-state/doze-standby)，高，转 PM-00 §1）。中国缺统一推送的代价 = N 条心跳（PM-01 §2.3，`research/PM-01-安卓省电演化史与历史包袱-v1.0.md`，高/中）。

### 2.3 长时任务/后台运行（对齐 PM-02 u8 reason 表）

| 维度 | Android 前台服务 | iOS UIBackgroundModes | OH continuousTask |
|---|---|---|---|
| 函数名 | `startForeground(id, notification, type)` | Info.plist 静态声明（audio/location/voip/bluetooth-central/processing/remote-notification…） | `backgroundTaskManager.startBackgroundRunning(context, BackgroundMode.X, wantAgent)` / `stopBackgroundRunning` |
| 类型表 | API34 起**必须声明 type**（camera/dataSync/health/location/mediaPlayback/…/specialUse），缺 type 直接 `MissingForegroundServiceTypeException` 崩溃 | 白名单 + 审核制，无运行时申请 | BackgroundMode 枚举白名单（AUDIO_PLAYBACK 等） |
| 权限 | `FOREGROUND_SERVICE` + 逐 type `FOREGROUND_SERVICE_*` | entitlement/审核 | `ohos.permission.KEEP_BACKGROUND_RUNNING` |
| 配额 | dataSync **6h/24h**（Android 15 起 onTimeout 强停）；Android 16 起前台服务与用户豁免面也开始吃 JobScheduler 配额（PM-00 §1 增量核证，高） | 无配额概念——从不给予执行权 | 音频类须配 AVSession；**焦点丢失 → 自动冻结**（PM-02 §4-4，高） |
| 校验 | type 与实际行为靠 Play 审核 | 审核 | 系统交叉校验（声明的 reason 与可观测硬件使用） |

证据：Android 14 type 强制（[官方 behavior change](https://developer.android.com/about/versions/14/changes/fgs-types-required)，高）；dataSync 6h（[Android 15 迁移文](https://www.google.com/goto?url=CAESjAEBxCJF19DPEtFgW1jp4AHKBo2jjRM3He44QKy9rFLkme1IGYtaCxORbwAMp271Eg2f2yZXx3EHBK179K0j0SVxQa5a6JcfeevhK9tXTWTjOf3IyGM_PIU9BflEkw8YQBtK7oEALwnZ7W-CuzK3GBjtyIQI4uBqd_N5FohBptSpr3c8cu5Sh2cQagq)，中）；AVSession 焦点联动（[openharmony docs](https://github.com/openharmony-rs/openharmony-docs/blob/master/en/application-dev/media/avsession/avsession-background-scene.md)，高）；KEEP_BACKGROUND_RUNNING 用法（[知乎案例](https://zhuanlan.zhihu.com/p/1924572924620306039)，中）。**reason u8 枚举（0x01 音频/0x02 导航/0x03 设备连接/0x04 下载/0x05 重载计算、未知枚举一律拒）**已在册（`research/PM-02-iOS与鸿蒙省电哲学对照-v1.0.md:288`，高）。

### 2.4 传感器与 batching（对齐 PM-04 §8.3-9）

| 维度 | Android | iOS | OH |
|---|---|---|---|
| 订阅函数 | `registerListener(sensor, listener, samplingPeriodUs, maxBatchReportLatencyUs)` | `CMMotionManager.startAccelerometerUpdates(handler)`（`accelerometerUpdateInterval` 秒） | `sensor.on(SensorId.X, cb, {interval: ns})`；`sensor.once()` 单发无 interval |
| batching | FIFO 硬批量：`fifoMaxEventCount/fifoReservedEventCount` 能力申报 + `flush()`；唤醒类/非唤醒类不共 FIFO；**on-change 最新值存批缓冲外**（计步覆盖惨案）；AP 出 suspend 数 ms 须一批捎回 | 无在线批量；**录后取批**：`CMSensorRecorder.recordAccelerometer(forDuration:)` ≤43200s（12h）固定 50Hz；iOS17 `CMBatchedSensorManager` 800Hz 每秒一批 | **JS API 无任何 batching/FIFO/latency 参数**——只有 interval（默认 200_000_000ns） |
| 逃生门 | maxReportLatency=0 即到即派 | 直推档天然即到即派 | 无档概念 |
| 权限 | 高频定位/心率等按传感器分权限 | `NSMotionUsageDescription` | `ohos.permission.ACCELEROMETER` / `GYROSCOPE`（隐私类分权，其余免权） |
| 错误码 | 按 HAL 层上抛 | 运行时错误对象 | **201** 权限拒 / **401** 参数错 / **14500101** 服务异常 |

证据：AOSP batching 契约全文（[source.android.com](https://source.android.com/docs/core/interaction/sensors/batching)，高，PM-04 §5.1 已收）；CMSensorRecorder 12h/50Hz（[Apple 官方](https://developer.apple.com/documentation/coremotion/cmsensorrecorder)，高）；CMBatchedSensorManager iOS17（[WWDC23](https://developer.apple.com/videos/play/wwdc2023/10181/)，高）；OH sensor 官方 md 全取回（[GitHub raw](https://raw.githubusercontent.com/openharmony/docs/master/en/application-dev/reference/apis-sensor-service-kit/js-apis-sensor.md)，高）。

### 2.5 热状态（对齐 PM-07 thermal_level 四档只读）

| 维度 | iOS | Android | OH |
|---|---|---|---|
| 函数名 | `ProcessInfo.thermalState`（nominal/fair/serious/critical 四档） | `PowerManager.addThermalStatusChangedListener`（THROTTLING_NONE..SHUTDOWN 七档）+ `getThermalHeadroom(seconds)` 预测 | `thermal.getLevel()` / `registerThermalLevelCallback(cb)` |
| 订阅 | `NSProcessInfoThermalStateDidChangeNotification` | listener 回调 | callback 注册 |
| 权限 | 无 | 无（普通 API） | **无权限**（仅 SysCap 声明） |
| 错误码 | — | — | **401** 参数错 |
| 议价面 | **零**——只读枚举 + 自愿配合 | 零（headroom 只读） | 零（OH 档位 0–7：COOL..EMERGENCY=6，ESCAPE=7 API11） |

证据：iOS 四档（PM-02 §2.5 表 `research/PM-02-iOS与鸿蒙省电哲学对照-v1.0.md:118-131`，高）；Android thermal API（[ADPF/Thermal](https://developer.android.com/games/optimize/adpf/thermal)，高）；OH thermal 官方 md 全取回（[GitHub raw](https://raw.githubusercontent.com/openharmony/docs/master/en/application-dev/reference/apis-basic-services-kit/js-apis-thermal.md)，高）。BerryOS 已裁 **u8 四档（0=nominal/1=fair/2=serious/3=critical，4–255 保留）+ 只读 + 无议价 + critical 无豁免**（PM-07 §4-5 `research/PM-07-热管理与电源耦合-v1.0.md:196`，高）。

### 2.6 电池/电源信息与充电控制（对齐 PM-14 契约；PM-14 落稿后对表）

| 维度 | Android | iOS | OH |
|---|---|---|---|
| 读函数 | `BatteryManager.getIntProperty(BATTERY_PROPERTY_CAPACITY/CHARGE_COUNTER/CURRENT_NOW/ENERGY_COUNTER/STATUS)` + `isCharging()` + 粘性广播 `ACTION_BATTERY_CHANGED` | `UIDevice.batteryLevel`（0..1；未开监控恒 -1.0）/ `batteryState`（unknown/charging/full/unplugged）/ `isBatteryMonitoringEnabled` 开关 + 通知 | `@ohos.batteryInfo` 只读常量：`batterySOC`、`chargingStatus`(NONE/ENABLE/DISABLE/FULL)、`healthStatus`、`pluggedType`(AC/USB/WIRELESS)、`voltage`(µV)、`batteryTemperature`(0.1°C)、`technology`、`batteryCapacityLevel`(9+)、`nowCurrent`(mA,12+)；变更走 `COMMON_EVENT_BATTERY_CHANGED` 键值 |
| 充电控制 | **无公共 API**——Pixel「80% 限充」走 `charge_limit` 系统设置键（WRITE_SECURE_SETTINGS 侧通道） | **无公共 API**——限充/优化充电纯 Settings 面 | 无三方充电控制 API（智慧充电系统侧，低） |
| 权限 | `BATTERY_STATS`（部分属性）/广播免权 | 免权（开启监控即可） | 免权（无 systemapi 字段，全公共） |
| 错误码 | — | -1.0 哨兵值（监控未开） | 无（常量直读） |

证据：BatteryManager 参考（[developer.android.com/reference/android/os/BatteryManager](https://developer.android.com/reference/android/os/BatteryManager)，高）；Pixel charge_limit 侧通道（[Reddit/Tasker 实测](https://www.reddit.com/r/tasker/comments/1bcgz0s/found_how_to_control_charging_optimization_80/)，中）；UIDevice 三件（[Apple 官方](https://developer.apple.com/documentation/uikit/uidevice/isbatterymonitoringenabled)，高；限充无 API = 官方文档长期缺位，高）；OH batteryInfo 官方 md 全取回（[GitHub raw](https://raw.githubusercontent.com/openharmony/docs/master/en/application-dev/reference/apis-basic-services-kit/js-apis-battery-info.md)，高）。**Fuchsia BatteryInfo 15 字段集**（PM-12 #12 已裁「直接抄」，`research/PM-12-二期深研综合与PM00回填表-v1.0.md:167`，高）。

---

## §3 教训与坑（引 SDK-03 课号；无课号 = 本报告新增）

1. 【SDK-03 Android 课「自由后台执行是债务最高的 API」（`research/SDK-03-系统API历史教训与一次设计定谳-v1.0.md:139`）】六面全部 day-one「声明约束、系统代跑」——本报告把它从原则压到六张签名表（§4.2）。
2. 【SDK-03 原则 1 细粒度资源模型（:43）】能力位按面细开闭集词表（`timer=[precise]` 而非 `power=[all]`），禁「全类别电源」原语——Android `USE_EXACT_ALARM`「自动授+不可撤+靠 Play 政策限用例」就是粗权限配逃生门的活化石（§2.1，高）。
3. 【SDK-03 原则 4 错误即数据（:93）】服务 op 应答 = 稳定枚举 + 附因结构（缺能力名/超配额值/可重试位），裸字符串不穿 berry-power 边界——OH 1700002（带码带义）是正面样本，iOS -1.0 哨兵值（无错误通道、魔法数冒充状态）是反面样本。
4. 【SDK-03 Windows 课「静默降级成功」（:162）】与「配额超限自动降档」的边界必须写死：Android 精确闹钟超配额**降级 batched 不报错**+遥测计费（PM-11 §4.4-3）是合法的显式降级——合法条件 = ①契约明文 ②遥测可查 ③应用可查询实际档位；三者缺一即静默降级。
5. 【SDK-03 OH SysCap 课（:173）】六面能力位进 manifest `[capabilities]` 闭集（`user/berry-manifest/src/lib.rs:10-45` 先例：词表=服务的镜子），安装期子集校验 + 统一 `NOT_SUPPORTED`。
6. 【SDK-03 Hyrum 课（:192）】配额数值、放行时刻形态、订阅合并策略全部进「不承诺」面——只承诺档位语义与错误码，不承诺调度形态。
7. 【新增｜双权限 = 补丁形状】同一能力两种授予通道（SCHEDULE vs USE_EXACT_ALARM）+ 撤权即**全杀未来闹钟**（app 停止）是兼容史逼出来的补丁；BerryOS 一能力一通道，撤销语义 = 断线自动回收（handle 天然自回收，PM-00 P1）。
8. 【新增｜数量帽要在服务侧且分级】OH 30/2000/12000 三级帽证明数量记账放用户态服务足够（非内核面），且「三方/系统/全局」三口径要一次定齐（中，§2.1）。
9. 【新增｜「无 API」也是 API 形状】APNs 无客户端订阅 API + 双巨头充电控制无 API：**面不存在**比「面存在但受限」省掉整个执法成本——BerryOS 充电控制照此裁（§4.5-6）。
10. 【新增｜batching 逃生门必须计费】latency=0 即到即派保留（PM-00 §5.2「给，明码标价」），但每次订阅进遥测 wake 记账——否则逃生门变成默认后门，50Hz×N app = 每秒吵醒 AP 50N 次（PM-04 §5.1，高）。
11. 【新增｜iOS「录后取批」是第三种 batching 形态】在线批（Android FIFO）/离线录（CMSensorRecorder 12h@50Hz）/秒批（iOS17 800Hz 1批/s）——BerryOS v1 只立法在线批 + flush；录后取批挂 hub 域版本域（PM-05 §4.5，v2+）。

---

## §4 BerryOS 启示（SDK-2 立法的直接输入；本节为建议稿，立法走 SDK-2 开刀 ADR）

### 4.1 与 35 syscall 预算的挂接主张（先裁归属，再写签名）

| 面 | 归属 | 内核挂接 | 裁决依据 |
|---|---|---|---|
| ①–③⑥ | **berry-power 通道 op**（`POW_OP_*` 扩展族，PM-00 §2 已定 op 不占 syscall 预算） | precise(wake) 的 always-on 编程 → **v2 已预留 0x62/0x63 timer_create/timer_set**（`tools/abi.toml` 时间组）；唤醒记账走 P10/telemetry_open 在册 | 服务面 = IPC op 是微内核五原则正解；35 在册号无一电源 app 面，加号即破小冻结面（SDK-03 原则 5 :105） |
| ② pushd 通道 | pushd（网络服务立项时，PM-00 §5.0 窗口表） | 无 | 单长连接省电机理在服务拓扑不在 syscall（PM-00 §1 Q1，高） |
| ④ sensorsd 通道 | sensorsd（订阅/flush/batch 面全在服务） | IRQ/event 已有 0x71–0x73 v2 预留承接驱动面 | batching 是上报策略非内核机制（PM-04 §5.1，高） |
| ⑤ berry-thermal 通道 | berry-thermal（PM-07 §4-1 已裁） | 降档汇入既有帽聚合点（不改 syscall） | 档位仲裁=查表+迟滞纯策略（PM-07，高） |

**结论：六面合计新增 syscall = 0。** SDK 层落法：新增 `berry-pwproto`（+pushd/sensorsd 各自 proto）纯函数协议 crate——照 `berry-psproto` 先例（`user/berry-psproto/src/lib.rs:1-30`：零分配/零 unsafe/定宽 LE/op u8/严格解析/host 先测/wire add-only/per-interface version）；berry-sys **不加号**，只加服务客户端包装 crate（extern "C"、永不 panic、Status 返回，风格对齐 `user/berry-sys/src/lib.rs:126` 的 `berry_handle_close` 族）。

### 4.2 六面 berry-sys 风格草案（签名 + 错误码；C-ABI，raw 层不 panic 不分配）

```rust
// —— ① 定时/闹钟：berry-power ALARM 族。TimerSpec 定长 32B（size:u32=4 + tier/clock/reason/reserved 4×u8=4 + 3×u64=24）【勘误 2026-11-07：原注「40B（size+tier+clock+flags+3×u64+reason）」双错——尺寸算错，且 flags 字段不存在（实为 reserved）】——
#[repr(C)] pub struct TimerSpec {
    pub size: u32,        // 前向兼容（SchedAttr 同款，user/berry-sys/src/lib.rs:52-67 先例）
    pub tier: u8,         // 0=batched(默认) 1=window 2=precise；野值 -EINVAL
    pub clock: u8,        // 0=MONO(停钟域) 1=SLEPT(连续域)；不复用 POSIX 名（PM-11 §4.1）
    pub reason: u8,       // 调用方用途（遥测归因用，不门控）
    pub reserved: u8,
    pub earliest_ns: u64, // window 下界；batched/precise 为触发时刻
    pub deadline_ns: u64, // window 上界 = slack 显式化
    pub lease_ns: u64,    // 到期回调自带的 wake 租约时长（PM-12 #10-③：hrtimer 双段 keep-alive 同构）
}
pub fn berry_timer_schedule(spec: *const TimerSpec, alarm: *mut Handle, lease: *mut Handle) -> Status;
pub fn berry_timer_cancel(alarm: Handle) -> Status;   // handle 关闭即语义回收（断线自动回收）
// 错误：-EINVAL（野 tier/clock/deadline<earliest）；-EACCES→BERRY_ERR_ACCESS_DENIED（precise 未申报
// 能力位）；配额超限 **不报错**：自动降级 tier=0 + 遥测计费（Android「降级不报错」惯例，PM-11 §4.4-3）
// 默认：tier=batched / clock=MONO / precise 需 [capabilities] timer=["precise"] 且默认零配额
// （B8 能力类别，PM-12 #10-④：wake 位=能力，CAP_WAKE_ALARM 同构）

// —— ② 推送：pushd 通道（app 侧只有三口，无 QoS 旋钮无长连接原语）——
pub fn berry_push_get_token(buf: *mut u8, len: *mut usize) -> Status;      // token ≤64B
pub fn berry_push_subscribe(topic: *const u8, len: usize, ttl_ns: u64) -> Status; // 订阅带 TTL（B5 租约）
pub fn berry_push_unsubscribe(topic: *const u8, len: usize) -> Status;
// 错误：BERRY_ERR_NOT_SUPPORTED（pushd 未起——本地批处理模式）；订阅量超帽 -INVALID_ARG 族 + 附因
// 默认：TTL 到期自动退订；进程死=订阅灭（PM-00 §5.2「从不给」行）

// —— ③ 长时任务：berry-power TASK 族（reason u8 对齐 PM-02 §4-2）——
pub fn berry_task_begin(reason: u8, evidence: Handle, task: *mut Handle) -> Status;
// reason ∈ {0x01 音频,0x02 导航,0x03 设备连接,0x04 下载,0x05 重载计算}，未知枚举一律 -EINVAL
// evidence：0x01 必须传音频焦点会话 handle（系统交叉验证；焦点丢失→自动冻结，PM-02 §4-4）
pub fn berry_task_renew(task: Handle, extend_ns: u64) -> Status;   // 到期自动失效（无 -1 永久租约）
pub fn berry_task_end(task: Handle) -> Status;
// 错误：-EINVAL（野 reason）；-EBADSTATE（0x01 无焦点句柄）；租约上限内置（对齐 dataSync 6h/24h 量级，初值表刀内定）

// —— ④ 传感器：sensorsd 通道（PM-04 §8.3-9 全套）——
#[repr(C)] pub struct SensorSub {
    pub sensor_id: u16,       // 驱动 manifest 注册表内 ID
    pub period_ns: u64,       // 采样周期（钳制进驱动申报 [min,max]——Android minDelay 同构）
    pub max_latency_ns: u64,  // 0=即到即派逃生门（遥测计费）；>0 进 FIFO 批
    pub flags: u32,           // bit0=WAKEUP 类（唤醒/非唤醒两通道不共 FIFO）；bit1=ON_CHANGE
}
pub fn berry_sensor_subscribe(sub: *const SensorSub, out: *mut Handle) -> Status;
pub fn berry_sensor_flush(h: Handle) -> Status;          // 显式冲刷（AOSP flush 同构）
pub fn berry_sensor_read(h: Handle, ev: *mut SensorBatch, n: *mut usize) -> Status; // 一批捎回
// 错误：-ENOTSUP（无此传感器/hub 域未开）；-EINVAL（period 越界/订阅数超帽）；ON_CHANGE 最新值存批缓冲外（契约明文）

// —— ⑤ 热状态：berry-thermal 通道（只读两口，无议价）——
pub fn berry_thermal_level(out: *mut u8) -> Status;      // 0–3 有效；4–255 保留（读到保留值 = 协议错）
pub fn berry_thermal_subscribe(ch: Handle) -> Status;    // 复用通知通道（变档事件）
// 错误：仅 BERRY_ERR_NOT_SUPPORTED（berry-thermal 未起——critical 短链内核兜底不受影响，PM-07 §4-7）
// 语义：severity 是承诺面，温度只进遥测不进契约（PM-07 教训 2）

// —— ⑥ 电池：berry-power BATTERY 族（15 字段 + 计量标注）——
#[repr(C)] pub struct BatteryInfo {   // Fuchsia battery.fidl 15 字段同源（PM-12 #12）+ 标注位
    pub soc_pct: u8, pub status: u8,   // 0=放电 1=充电 2=充满 3=禁充
    pub plugged: u8,                   // 0=无 1=AC 2=USB 3=无线
    pub health: u8, pub temp_01c: i16, pub voltage_uv: u32, pub current_ma: i32,
    pub charge_uah: u32, pub energy_uwh: u32, pub full_uah: u32,
    pub spec_version: u16, pub present: u8, pub measured: u8,  // 0=modeled 1=measured（P8 标注位）
    pub reserved: [u8;2], pub timestamp_ns: u64,
}
pub fn berry_battery_get(out: *mut BatteryInfo) -> Status;
pub fn berry_battery_subscribe(ch: Handle) -> Status;    // 变更事件（SOC/状态/插拔）
// 充电控制：**无 app API**（§4.5-6）；charge_limit 当前值可读（进 BatteryInfo v2 add-only 字段）
// 错误：-ENOTSUP（无电池——QEMU/PC 桌面档合法态）；-EFAULT；建模值冒充实测值 = smoke 断言拦（PM-00 §5.3-9）
```

### 4.3 能力位总表（manifest `[capabilities]` add-only 词表增量；词表=服务的镜子，SDK-01 冲突项 1 `research/SDK-01-软件标准与SDK综合裁决.md:97`）

| 能力键 | 值域（闭集） | 默认 | 执法点 |
|---|---|---|---|
| `timer` | `["precise"]`（batched/window 免申报） | 未申报 = 只有 batched/window，precise 零配额 | mkapp 词法闭集 + appd 运行期拒（两级执法，`user/berry-manifest/src/lib.rs:27-33` 同款） |
| `push` | `["receive"]` | 无 | pushd 通道握手校验 |
| `background` | `["audio","nav","link","download","compute"]`（= reason 镜像） | 无 | berry-power admission（sched-1.1 准入检查器挂枚举表，PM-02 §4-2 落点） |
| `sensor` | `["accel","gyro"]`（仅隐私敏感类；环境类免申报） | 无 | sensorsd 订阅时校验 |
| （thermal/battery） | **零能力位**——只读面人人可查 | — | — |
| （充电控制） | **永不设键** | — | §4.5-6 |

### 4.4 默认值与配额总表（数值全部「初值 + 策略表项可调 + 遥测导出」，写进不承诺面）

| 项 | 初值主张 | 对照锚 |
|---|---|---|
| 定时默认档 | batched（clock=MONO） | Android 非精确为默认（§2.1，高） |
| precise 配额 | 每 app 每 idle 世代 1 次（≈9min 语义起步），超限自动降级 | Android 9min（中）+ PM-11 §4.4-3 |
| 定时订阅/提醒数帽 | 每 app 30 活跃（对齐 OH 三方帽；系统服务 2000/全局 12000 同口径三级） | OH §2.1（中） |
| 长时任务租约 | 默认 5min、续租上限 6h/24h（dataSync 量级） | Android 15（高） |
| 订阅 TTL | push/事件订阅默认 24h、进程死即灭 | PM-00 §5.2 B5 |
| 传感器并发订阅帽 | 每 app 8 路；latency=0 订阅 ≤2 路（遥测计费） | 新增（量级主张，低） |
| window 最小窗宽 | 10min（<10min 钳制，明示钳制不静默） | Android setWindow 600000ms（高） |
| 维护窗放行抖动 | 必须 jitter（防全体设备同时醒来涌网络） | Android 最佳实践 + setAlarmClock prep-work（高） |

### 4.5 反面清单（六面合计「绝不做成什么形状」；供 ABI/SDK 评审 checklist）

1. 不做 `setExactAndAllowWhileIdle` 式「免身份精确」——precise 从第一天 = 能力位 + 配额 + 遥测计费（§3-2/7）。
2. 不做双权限同能力（SCHEDULE/USE 式）——一能力一通道一撤销语义（handle 断线回收）。
3. 不做 app 侧长连接/QoS 旋钮/心跳原语——pushd 单通道是省电契约本体（PM-00 §5.3-10）。
4. 不做 `-1` 永久租约/无限期常驻——一切租约带 TTL（OH 永久锁勿学，PM-02 §4-3）。
5. 不做热议价/豁免申请 API——critical 档无豁免（PM-07 §4-5）；豁免走 PM-06 assertion 通道且永不进 app 面。
6. 不做 app 侧充电控制 API——两大平台一致「面不存在」；限充是用户设置不是应用能力（§2.6，高）。
7. 不做全局 batching/精度开关——slack 永远 per-thread/per-subscription（Windows 30 年教训，PM-00 §5.3-1）。
8. 不做 per-app 实测毫安时承诺——measured/modeled 标注位是契约一部分（PM-00 §5.3-3/9）。
9. 不做私有热档位/私有放行时刻——档位全局四档、放行时刻形态不承诺（§3-6 Hyrum）。
10. 不复用 `CLOCK_MONOTONIC` 名、不做静默续订——订阅到期即灭、显式续订留痕（PM-11 §4.1 + §3-4 边界）。

---

## §5 引用源列表

**外部（本报告新取证）**
- Android alarms 官方页（全取回：五档/双权限/10min 钳制/撤权全杀）— https://developer.android.com/develop/background-work/services/alarms
- Android 14 FGS type 强制 — https://developer.android.com/about/versions/14/changes/fgs-types-required
- Android dataSync 6h/24h（Android 15）— https://developer.android.com/about/versions/15/behavior-changes-15
- Android BatteryManager — https://developer.android.com/reference/android/os/BatteryManager
- Android ADPF/Thermal — https://developer.android.com/games/optimize/adpf/thermal
- 9min 精确闹钟配额（社区复核）— https://stackoverflow.com/questions/49483713/setexactandallowwhileidle-for-alarmmanager-is-not-working-properly
- Apple：BGTaskScheduler — https://developer.apple.com/documentation/uikit/using-background-tasks-to-update-your-app ；CMSensorRecorder — https://developer.apple.com/documentation/coremotion/cmsensorrecorder ；UIDevice 电池 — https://developer.apple.com/documentation/uikit/uidevice/isbatterymonitoringenabled ；APNs 部署 — https://support.apple.com/en-ng/guide/deployment/dep2de55389a/web
- OpenHarmony 官方 md 全取回：thermal — https://raw.githubusercontent.com/openharmony/docs/master/en/application-dev/reference/apis-basic-services-kit/js-apis-thermal.md ；sensor — https://raw.githubusercontent.com/openharmony/docs/master/en/application-dev/reference/apis-sensor-service-kit/js-apis-sensor.md ；batteryInfo — https://raw.githubusercontent.com/openharmony/docs/master/en/application-dev/reference/apis-basic-services-kit/js-apis-battery-info.md
- OH 后台任务/提醒帽与错误码（社区，中）— https://blog.csdn.net/bestadc/article/details/148065471 · https://bbs.itying.com/topic/6907073c4a7a41007019a637 · https://zhuanlan.zhihu.com/p/1924572924620306039
- OH AVSession 后台音频（官方仓）— https://github.com/openharmony-rs/openharmony-docs/blob/master/en/application-dev/media/avsession/avsession-background-scene.md
- Push Kit — https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/push-send-alert · https://developer.huawei.com/consumer/en/hms/huawei-pushkit
- Pixel charge_limit 侧通道（中）— https://www.reddit.com/r/tasker/comments/1bcgz0s/found_how_to_control_charging_optimization_80/

**仓内**
- `research/PM-00-全系统省电架构综合裁决-v1.0.md`（§2 op 表 :118-131 · §5.1 P1–P10 :229-246 · §5.2 契约 :248-262 · §5.3 反面清单）
- `research/PM-02-iOS与鸿蒙省电哲学对照-v1.0.md`（§4 moves :285-296 · thermalState 表 :118-131）
- `research/PM-04-触摸与交互功耗-v1.0.md`（§5.1 batching 契约 :136-143 · §8.3-9 :212）
- `research/PM-07-热管理与电源耦合-v1.0.md`（§4-5 thermal_level 四档 :196）
- `research/PM-11-挂起与时钟语义-v1.0.md`（§4.1 双钟 :203-206 · §4.2 三档×挂起矩阵 :208-216 · §4.4 断言 :230-239）
- `research/PM-12-二期深研综合与PM00回填表-v1.0.md`（回填 #10 :165 · #12 :167 · #15 :175）
- `research/SDK-01-软件标准与SDK综合裁决.md`（执行摘要 :20-34 · 冲突项 1 :97）· `research/SDK-03-系统API历史教训与一次设计定谳-v1.0.md`（原则 1/4/5 :43/93/105 · Android 后台课 :139 · SysCap :173 · Hyrum :192）
- 代码风格基线：`user/berry-sys/src/lib.rs`（类型 :41-44 · SchedAttr :52-67 · wrapper 族 :126）· `user/berry-manifest/src/lib.rs`（schema v0/v1 :10-45）· `user/berry-psproto/src/lib.rs`（op/定宽/严格解析 :1-80）· `tools/abi.toml`（nr_total=35 :14-16 · timer v2 预留 0x62/0x63）

**待回填**：PM-14（充电电池域）落稿后，对表 §2.6 充电面与 BatteryInfo 字段集；PM-13（SoC 平台）落稿后，对表 sensorsd 的驱动 manifest 感知器申报字段（fifo_max/fifo_reserved 归 L3 申报面）。

（v1.0 完；升级触发器：SDK-2 契约刀立项 / PM-14 落稿对表 / berry-power op 面首次 ABI 评审，三者任一发生即升 v1.1。）
