# 对标底稿：HarmonyOS（OpenHarmony + NEXT 商用）应用规范全景（A–M）

> 采集：2026-11-08（应用规范立法调研·子代理底稿三号）。
> 方法注记：web_search 工具认证故障，developer.huawei.com 为 SPA 抓不到正文。改走三条可靠通道并逐条核对原文：① OpenHarmony 官方文档仓（github.com/openharmony/docs master，与华为 harmonyos-guides 同源）共取 40+ 篇原文；② 华为商用文档 GitHub 镜像（github.com/HarmonyOS-AI/Harmony-Skills，7788 文件快照，含 Live View Kit / Intents Kit / 应用隐私保护 / 启动页等商用专有文档）；③ DuckDuckGo lite 检索补零散事实。来源缩写：**[OH]**=github.com/openharmony/docs/blob/master/zh-cn/application-dev/；**[SK]**=github.com/HarmonyOS-AI/Harmony-Skills/blob/main/；**[HW]**=developer.huawei.com。标注"未核实"处为华为商用侧未公开/仅二手来源。API 版本口径：5.0=API12，NEXT 5.x≈12–20，HarmonyOS 6≈23，master 已到 26。
> 消费面：`docs/应用规范-v1.0.md` 全部章节的对标证据（与我们形态最接近的参照系）。

## A. 包结构与清单
- 分发三层：开发态 Module → 编译态 HAP/HSP（HAR 直接编译进使用方，APP 内无独立 .har）→ 发布态 **App Pack(.app)**=全部 HAP+HSP+自动生成 pack.info（含 bundleName/versionCode + 各 module name/type/abilities）。签名以 HAP/HSP/APP 为单位；云端分发/端侧安装以 HAP/HSP 为单位。[OH]quick-start/application-package-structure-stage.md
- Module type：`entry`（主模块，同设备同类型仅 1 个）/`feature`（动态特性 0..n）/`har`（静态共享包，可发 OHPM 仓）/`shared`（HSP 动态共享包，运行时单份，另有集成态 HSP）/`skill`（AI 技能包 API26+，仅预置）。[OH]quick-start/application-package-overview.md
- **app.json5**：`bundleName`（反域名≥3 段，7–128 字节）、`bundleType`（app/atomicService/shared/appService/appPlugin(19+)/skill(26+)，缺省 app）、`label`≤63B、`versionCode` 0–2147483647、`versionName`≤127B、`minAPIVersion`（**编译时自动生成，手配无效**）、`apiReleaseType`（CanaryN/BetaN/ReleaseN）、按设备覆盖 minAPIVersion 的 tablet/tv/wearable/car/2in1/default 对象、`generateBuildHash`（仅预置）。[OH]quick-start/app-configuration-file.md
- API 三件套在**工程级 build-profile.json5**：compileSdkVersion/compatibleSdkVersion/targetSdkVersion，编译折算进 app.json5。[HW]ide-hvigor-build-profile-app（字段全名未核实）
- **module.json5**：name≤128B（DevEco 限 31）/type/srcEntry≤127B/description≤255B/mainElement/deviceTypes/deliveryWithInstall/installationFree（自动生成，atomicService→true）/virtualMachine/abilities[]/extensionAbilities[]/requestPermissions[]/definePermissions[]/metadata[]。[OH]quick-start/module-configuration-file.md
- `deviceTypes` 枚举仅 6 值：tablet/tv/wearable（富手表带电话）/car/2in1(PC)/default（手机类归此，**无 phone 值）。
- abilities 关键子字段：name≤127B、launchType、startWindowIcon/startWindowBackground（简易启动页必配）、removeMissionAfterTerminate、isolationProcess（仅 2in1/tablet）、process（API14+ 仅 2in1/tablet）、skills[](entities/actions/uris[type+scheme])；入口判定=entities 含 entity.system.home 且 actions 含 ohos.want.action.home。
- 组件三段寻址 bundleName+moduleName+abilityName；卡片另加 formName+formDimension 成五元组。[OH]form/arkts-ui-widget-configuration.md

**BerryOS 借鉴**：① app.json5/module.json5 双层结构（全局身份+模块清单）可直译为 .bap 的 `[app]`/`[[module]]` TOML 表；② "编译期折算注入"（installationFree/minAPIVersion 不许手写）防声明与事实漂移——appd 应校验 cpio 内容与 TOML 一致性；③ deviceTypes 用设备类档位（非型号）与 BerryOS rich→PC 同构。
**反面**：module.json5 字段面已膨胀到 10 万字节文档（60+ ext 类型），v1 的 .bap 必须克制：只收真正执行的键，未知键硬拒绝。

## B. 权限模型
- 特权等级 **APL**：normal（三方默认）/system_basic/system_core（仅系统应用，不可自配）。[OH]security/AccessToken/app-permission-mgmt-overview.md
- 授权三类：**system_grant**（安装即授）/**user_grant**（运行时弹窗）/manual_settings（API21+ 只能去设置开）。
- **ACL 跨级**：权限级别高于应用 APL 时须在签名 Profile（HarmonyAppProvision）`acls.allowed-acls` 数组追加；带数据的受限权限走 `app-services-capabilities`。调试可改 SDK 的 UnsgnedReleasedProfileTemplate.json；**商用必须经应用市场申请发布证书**。[OH]security/AccessToken/declare-permissions-in-acl.md
- `requestPermissions[]`：name + `reason`（$string 引用，user_grant **必填**且多语种，上架校验）+ `usedScene{abilities:[名], when:"inuse"|"always"}`（user_grant **必填**）+ requiredFeature（系统应用≤64 字符）。任一 HAP 声明即全应用生效。[OH]security/AccessToken/declare-permissions.md
- **权限组**（同组一弹窗合并授权；位置/通讯录/日历例外逐条展示）：位置(LOCATION+APPROXIMATELY_LOCATION+LOCATION_IN_BACKGROUND)/相机/麦克风/通讯录(READ|WRITE_CONTACTS，受限)/日历/健身运动等。[OH]security/AccessToken/app-permission-group-list.md
- 三方 user_grant 约 20 项：CAMERA、MICROPHONE、READ/WRITE_CALENDAR、LOCATION、APPROXIMATELY_LOCATION、LOCATION_IN_BACKGROUND、MEDIA_LOCATION、READ_MEDIA、WRITE_MEDIA、ACTIVITY_MOTION、READ_HEALTH_DATA、READ/WRITE_DOCUMENTS_DIRECTORY、READ/WRITE_DOWNLOAD_DIRECTORY、APP_TRACKING_CONSENT、ACCESS_BLUETOOTH、ACCESS_NEARLINK、DISTRIBUTED_DATASYNC、CUSTOM_SCREEN_CAPTURE。[OH]security/AccessToken/permissions-for-all-user.md
- **受限开放权限**（要 ACL）：READ_CONTACTS、WRITE_CONTACTS、READ/WRITE_AUDIO、READ/WRITE_IMAGEVIDEO、SYSTEM_FLOAT_WINDOW(system_basic)、CAPTURE_SCREEN(system_core)、KEEP_BACKGROUND_RUNNING_SYSTEM 等 30+ 项。[OH]security/AccessToken/restricted-permissions.md
- 签名 Profile：`apl` + `app-feature`（hos_system_app|hos_normal_app，决定系统 API 可用性）+ `app-distribution-type`（app_gallery/enterprise/enterprise_mdm/enterprise_normal/os_integration(预置)/crowdtesting/internaltesting/none）。[OH]security/app-provision-structure.md
- reason 文案有专门规范章节，上架时校验表述与场景一致。

**BerryOS 借鉴**：① requestPermissions 三要素（name+reason+usedScene{ability,when}）就是"能力请求的意图+范围"，可直译 `[[grant]] service=... reason=... scope={component, timing=inuse|always}`，appd 据此翻译成 handle；② ACL 本质="超出默认等级的能力需发行渠道背书"——纯能力模型下可译为"受限能力需系统签名 manifest 或商店签发授权节"，而非运行时查权限字符串；③ 权限组=授权 UI 的合并单位，BerryOS 按"服务域"分组弹窗。
**反面**：ohos.permission.* 散在 3 张长表，开发者极易申错级；BerryOS manifest 只写"服务名+档位"，级别判定表内置于 appd。

## C. 生命周期与后台
- **UIAbility 生命周期**：onCreate → onWindowStageCreate → onForeground/onBackground（可循环）→ onWindowStageDestroy → onDestroy；最近任务上滑关闭触发 onDestroy。[OH]application-models/uiability-lifecycle.md
- launchType：singleton（默认）/multiton（曾用名 standard）/specified（指定实例：启动前回调应用决定复用或新建）。[OH]application-models/uiability-launch-type.md
- **ExtensionAbility type 枚举 60 项**，三方常用：form/workScheduler/inputMethod/share/accessibility/vpn/wallpaper/backup/print/push/voip/thumbnail/preview/photoEditor/autoFill/password/autoFill/smart/enterpriseAdmin/driver/action/embeddedUI/insightIntentUI(被小艺调起)/liveForm(20+)/appService(20+)/appAccountAuthorization/selection/distributed/formEdit；仅系统或特权：service/dataShare（需 allowAppUsePrivilegeExtension 特权，不对三方开放）/fileAccess/staticSubscriber/window/adsService/sysDialog/*(userAuth/power/…)/sysPicker/*(share/photoPicker/…)/uiService 等。[OH]quick-start/module-configuration-file.md#type标签
- **短时任务**：单日（24h）配额默认 **10 分钟**、单次最大 **3 分钟**、低电量单次 **1 分钟**；仅后台计时；超时不取消→挂起/终止。[OH]task-management/transient-task.md
- **长时任务 11 类**：DATA_TRANSFER/AUDIO_PLAYBACK/AUDIO_RECORDING/LOCATION/BLUETOOTH_INTERACTION/MULTI_DEVICE_CONNECTION/VOIP(13+)/TASK_KEEPING(API21 前仅 PC，之后需 ACL)/MODE_AV_PLAYBACK_AND_RECORD(22+)/MODE_SPECIAL_SCENARIO_PROCESSING(22+)/MODE_NEARLINK(26)。约束：DATA_TRANSFER 首次进度更新超 **10 分钟**未更新即取消；进度通知必须用实况窗；任务与常驻通知绑定（用户可感知）。[OH]task-management/continuous-task.md
- 延迟任务：workScheduler（WorkSchedulerExtensionAbility，系统按网络/充电状态调度）。[OH]task-management/work-scheduler.md
- **代理提醒**：倒计时/日历/闹钟 3 类；单应用有效提醒≤**30**，全系统≤**12000**（API10+，此前 2000）；需 ohos.permission.PUBLISH_AGENT_REMINDER（开放级别未核实）。[OH]task-management/agent-powered-reminder.md
- 后台资源总约束：内存/CPU 占比/**24h 磁盘写 IO** 三类配额，超限后台进程直接终止；长时任务仅提高回收优先级。[SK]Background Tasks Kit简介
- **应用分组冻结**（perceptible/verified/blind/suspended/restricted）：华为商用资源调度文档未公开 → **未核实**。
- mission：任务快照展示于最近任务；removeMissionAfterTerminate=true 销毁即移除（2in1/平板自由多窗不生效）。

**BerryOS 借鉴**：① "短时配额（日10min/次3min）+长时类型白名单（每类绑定系统可感知证据）+代理提醒"三层模型可平移到微核：长时任务本质=appd 向调度服务担保该进程持某资源 handle（音频输出 handle=后台播放权）；② usedScene.when(inuse/always) 可作能力时效声明；③ 代理提醒是纯能力模型的自然推论：系统服务持定时器 handle，应用死了提醒仍在。
**反面**：长时任务从 9 类膨胀到 11 类且按业务命名（星闪/AV 合并档），枚举会无限生长；BerryOS 应按"资源证据"（持音频流/定位流/传输流）定义，枚举封闭。

## D. 自启动与关联启动
- 开机自启：API21+ autoStartupManager，**三方只能查询自身状态**；设置接口为系统 API 需 ohos.permission.MANAGE_APP_BOOT。用户入口：设置-应用和元服务-应用启动管理，逐应用手动添加。[OH]reference/apis-ability-kit/js-apis-app-ability-autoStartupManager.md；[HW]/consumer/cn/blog/topic/03200052718536104
- 关联启动：设置内逐应用"允许关联启动"开关；开发者文档细则未公开 → **未核实**（定性：最严管控样本，默认收紧+白名单放行）。
- 后台弹窗：禁止后台弹窗拉界面，经通知/实况窗承接（细则**未核实**）。
- **BerryOS 借鉴**：① "三方只能查询、系统应用才能改"——自启 handle 归系统服务，manifest 声明 `wants_boot` 只是必要条件，用户在设置授予才是充分条件；② 关联启动的能力化表达：拉起 B 需持 B 的启动 handle，而该 handle 只能由用户操作或系统服务派生——链式唤醒被能力图天然切断，无需事后管控。
- **反面**：HarmonyOS 把启动管理做成每应用三个开关（自启/关联/后台弹窗），用户两极化全关/全开；BerryOS 收敛到"能力授予"单一心智。

## E. 通知
- SlotType 枚举+行为矩阵（通知中心/横幅/锁屏/铃声振动/状态栏/自动亮屏）：`SOCIAL_COMMUNICATION`(1) 全开；`SERVICE_INFORMATION`(2) 全开；`CONTENT_INFORMATION`(3) 仅通知中心；`CUSTOMER_SERVICE`(5) 通知中心+铃声+状态栏；`LIVE_VIEW` 实况窗（商用）；`UNKNOWN_TYPE`(0)/`OTHER_TYPES`(0xFFFF) 仅通知中心。[OH]notification/notification-slot.md
- 实况窗内容类型 NOTIFICATION_CONTENT_SYSTEM_LIVE_VIEW(11+)：文本+胶囊/按钮/时间/进度；不持久化，生命周期随发布方。[OH]notification/live-view-notification-sys.md
- NotificationRequest：id/content/notificationSlotType/wantAgent(点跳) 等。[OH]notification/notification-overview.md
- 免打扰/横幅级别由 slot+系统设置共决（细则未核实）。
- **BerryOS 借鉴**：① slot="通知打扰度档位"而非渠道——通知 handle 归通知服务（应用只是投稿方，退场不影响已发通知）；② 6 档行为矩阵可压缩为 3 档（社会级/服务级/静默级）+锁屏/亮屏开关。
- **反面**：OTHER_TYPES 成垃圾桶档被滥用；BerryOS 要求 manifest 声明通知档位并在授权时展示。

## F. UI 规范
- **图标**：分层图标（前景+背景 PNG+layered_image.json），系统统一遮罩/圆角裁切；也支持单层。社区规格背景 324×324、前景安全区 216×216（**未核实**）；AGC 上架图标 512×512（**未核实**）。[OH]quick-start/layered-image.md；[HW]design-guides/application-icon
- **启动页**：简易启动页=abilities 内 startWindowIcon+startWindowBackground（每 UIAbility 必配，图标不随窗口缩放、不建议透明）；增强启动页 startWindow 字段（资源随窗口缩放，建议 jpg/png）。[SK]应用启动页的配置与使用
- 系统图标：标准 **24vp**、中央 22vp 绘制区留 1vp、外圆角 4vp/内圆角 2.5vp/间隙拐角 3.5vp。[OH]design/ux-design/visual-icons.md
- **栅格**：Margin/Gutter/Column，推荐 4/8 倍数（如 32vp+16vp+4 列）。GridRow 默认断点 **["320vp","600vp","840vp"]**（可加 1080vp 第四档），六档 xs/sm/md/lg/xl/xxl；columns 默认(API20+) {xs:2,sm:4,md:8,lg:12,xl:12,xxl:12}；参照物默认 WindowSize。[OH]reference/apis-arkui/arkui-ts/ts-container-gridrow.md
- 字体单位 **fp**（随系统字号缩放）、布局 vp；官方字阶梯度表（未核实档位）。深色模式：dark/ 资源限定词；卡片也有深浅色适配。多设备：自适应布局+断点+栅格；2in1 多窗 preferMultiWindowOrientation(default/portrait/landscape/landscape_auto)。无障碍：accessibilityText/焦点管理（商用 screenReader 规范未核实）。
- **BerryOS 借鉴**：① 分层图标+系统统一遮罩="应用供内容、形状归系统"，BerryOS 桌面同法保版式统一；② 320/600/840 断点与穿戴→手机→平板→PC 天然对齐，可按 display class handle 暴露；③ .bap TOML 收 `splash={icon,bg}` 成本极低防白屏。
- **反面**：简易/增强启动页双轨互踩（startWindow 使简易失效）；BerryOS 只留一套。

## G. 桌面组件
- 卡片=FormExtensionAbility+ArkTS 卡片页+form_config.json（metadata $profile:form_config 挂载）。字段：name≤127B/displayName≤30B/description≤255B/src/isDynamic(动态|静态)/updateEnabled/**updateDuration(单位 30 分钟**，N→30N 分钟，0 不生效)/scheduledUpdateTime(24h 制"10:30")/multiScheduledUpdateTime/dataProxyEnabled(代理刷新 API12+)/supportDimensions/defaultDimension/previewImages(穿戴必配)/resizable(20+)/groupId。[OH]form/arkts-ui-widget-configuration.md
- **supportDimensions 7 档**：1*2、2*2、2*4、4*4、6*4 + 仅手表 2*3、3*3；defaultDimension 须取自其中。
- **刷新配额**：定时刷新每卡每日≤**50 次**（0 点重置）；setFormNextRefreshTime 最短 **5 分钟**；同应用共享计时器→首刷最多偏差 30 分钟；不可见只记账、可见统一刷；开代理刷新则定时/下次刷新失效（API26 起可同开）。[OH]form/arkts-ui-widget-passive-refresh.md
- **实况窗（Live View Kit，三方开放）**：卡片态+胶囊态+锁屏态，模板化设计。数量：通知中心滑动最多 **24 条**、胶囊列表最多 **5 条**；同 id 同时刻 1 个（Push Kit 同 id 12h 不可复用）。频控：Push Kit 云更新单窗每设备 5 分钟≤10 次/每小时≤60 次（出行/赛事 30/180）；系统级流控创建 15/s、更新 30/s；应用级创建 10/s、更新 20/s。**超 2 小时未更新→胶囊隐藏**仅存通知中心。[SK]Live View Kit FAQ+实况窗设计规范
- 元服务（atomicService）免安装，卡片是其桌面入口形态。
- **BerryOS 借鉴**：① 卡片用格数元组（2×2…）而非像素，桌面服务换算 dp——.bap 可声明 `[[widget]] grid=[2,2]`+渲染入口，appd 授桌面注册 handle；② 每卡日 50 次+最短 5 分钟是可抄防骚扰硬数字；③ "更新频控+2h 不更新自动降级为普通通知"的状态机可直接复用。
- **反面**：updateDuration 以 30 分钟为计数单位（N=2 表示 1 小时）反直觉；BerryOS 直接写分钟。

## H. 系统应用/预装
- 系统应用身份=签名 Profile：app-feature: hos_system_app + 更高 apl；app-distribution-type: os_integration=预置。系统 API 仅系统应用可调。[OH]security/app-provision-structure.md
- 特权 ext（service/dataShare）需设备侧特权 allowAppUsePrivilegeExtension，不对三方开放。[OH]module.json5#type标签
- NEXT：系统应用全面自研（不基于 AOSP），系统应用更新走应用市场；NEXT 完全不兼容 APK（公开口径）。[HW]
- 预装可卸载：分系统核心/可卸载增值/第三方合作三类，多数非核心可卸载（工信部 2023 规范），可"恢复应用"重装（媒体来源，**未核实**官方清单）。
- **BerryOS 借鉴**：① 系统应用=同一 .bap 格式+签名档位差异，而非第二套打包格式——系统应用也是 .bap，只是带系统签名与更高能力白名单，appd 按签名链授初始 handle 集；② app-distribution-type 的 8 值枚举是现成"发行渠道枚举"参考。
- **反面**：系统/三方 API 面差异靠 syscap 隐性区分，开发者"编译过运行挂"；BerryOS 在 manifest 校验期就拒越权声明。

## I. 隐私合规
- 三原则成文：**控制力、透明度、数据最小化**；上架时市场按规则校验，不满足无法上架。[SK]应用安全-应用隐私保护.md
- **位置双档**：仅 APPROXIMATELY_LOCATION=模糊（精度 **5 公里**，API9+）；叠加 LOCATION=米级；同组同弹窗，用户可只授模糊档。
- **Picker**：图库/文件/联系人等系统 Picker 临时授权所选数据，免整库权限（API9+）。**安全控件**：SaveButton（落盘）/PasteButton（读剪贴板）——点按即临时授权免弹窗。[OH]security/AccessToken/security-component-overview.md
- **剪贴板**：API12+ 读取需 ohos.permission.READ_PASTEBOARD（user_grant）或 PasteButton。[SK]剪贴板服务
- 一次性授权（only this time）支持。[OH]one-time-authorization.md
- 权限使用记录/隐私中心（敏感权限访问记录）：商用系统特性，开发者文档无细则 → **未核实**。
- **BerryOS 借鉴**：① 安全控件=点按即能力：UI 一次点击生成一次性 handle（PasteButton→clipboard.read once-handle），应成 GUI 规范一级公民；② 模糊/精确不是两权限而是一服务两档输出——位置服务 handle 带精度属性（coarse=5km/fine），授权 UI 选档；③ 最小化成文且进上架流程=合规进管道而非文档。
- **反面**：剪贴板管控 API12 才补，早期零提示；BerryOS v1 就把剪贴板做成默认无 handle 的服务。

## J. 分发与商店
- 上架单元=.app；AGC 流程：创建应用→证书(.cer)+Profile(.p7b)→hap-sign-tool/DevEco 签名→AGC 审核上架（免费 icon/截图材料要求**未核实**）。[OH]application-package-structure-stage.md、security/hapsigntool-guidelines.md
- **元服务包体**：整体≤**10MB**（可申请突破）；分包时首包及子包各≤**2MB**。[HW]/consumer/cn/doc/atomic-faqs/faqs-product-13
- 企业分发：profile 三档 enterprise/enterprise_mdm/enterprise_normal；MDM 仅企业设备。内测/众测：crowdtesting/internaltesting。
- NEXT 口径：5.x 起不兼容 APK，仅 ArkTS 原生生态。[HW]
- **BerryOS 借鉴**：① 元服务 10MB/分包 2MB 是免安装形态包体约束样板——v1 无商店也可让 appd 内置包体分级（超限必须完整安装确认）；② 发行渠道枚举应放签名元数据而非 manifest，发行信息与声明信息分离。
- **反面**：上架校验分散 AGC/编译器/市场三处报错晚；BerryOS 让 appd 装 .bap 时全量静态校验（结构+manifest+能力表）一处报全。

## K. 数据与存储
- **沙箱**：/data/storage/el1/bundle（只读安装目录）、/data/storage/el1/base、/data/storage/el2/base（按加密级别 el1/el2 分层）；base 下 files/（持久）、cache/（超配额系统自动清理）、temp/、haps/；另有 distributedfiles（el2，跨设备直访）。[OH]file-management/app-sandbox-directory.md
- 沙箱路径≠物理路径：普通应用只见子集（hdc=系统视角见全量）。
- 公共媒体：photoAccessHelper+PhotoViewPicker（选图临时授权）+SaveButton（免权限落盘）。[OH]media/medialibrary/photoAccessHelper-overview.md
- 分布式数据：distributedData（KV/对象）+RDB 跨设备同步；分布式文件走 distributedfiles。轻量：Preferences；关系型：RelationalStore。
- **BerryOS 借鉴**：① el1/el2 加密层级+files/cache 的"清理契约"（cache 随时可清、应用须可重建）应写进沙箱规范第一页；② Picker="数据不离开所有者进程，只发一次性只读 handle"，比授权-自取安全一个量级，与能力模型天然匹配；③ 卸载即清需 FS 服务与 appd 间做成事务。
- **反面**：目录种类多而语义细，持久数据常被误写进 cache；BerryOS 收敛为 data/（持久）+cache/（可丢）两类。

## L. 跨应用互操作
- **Want**：显式（bundleName+abilityName）/隐式（entities/actions/uri/type 匹配 skills）；startAbility(ForResult)；隐式多匹配时系统列选（商用含安全确认弹窗，细则**未核实**）。[OH]application-models/uiability-intra-device-interaction.md
- URI 临时授权：Want flags（ohos.want.flag.authReadUriPermission/authWriteUriPermission）授目标应用对该 URI 临时读写。
- 系统分享：Share Kit；应用可注册 `share` 类型 ShareExtensionAbility 为分享目标（三方可用）；系统面板=sysPicker/share。
- 输入法：inputMethod 类型 ExtensionAbility。
- 应用接续：跨设备 UIAbility 迁移（continuation）+分布式软总线，需 DISTRIBUTED_DATASYNC（user_grant）。[OH]application-models/uiability-cross-device-interaction.md
- **BerryOS 借鉴**：① 隐式 Want=服务注册表+能力路由：appd 维护「意图名→已授权服务 handle」表，隐式跳转=appd 代查转发，天然带权限过滤，无需复制 URI 体系；② URI 临时授权="文件 handle 附加授权再转交"，与 handle 转让语义同构，直接立法；③ 分享面板做系统单例，避免三方互相拉起。
- **反面**：隐式 Want 多候选弹窗体验长期混乱（双开浏览器）；BerryOS 隐式路由要求 handler 预注册唯一或用户固定默认。

## M. 特有补充
- **三件关系**：元服务（atomicService，免安装、可独立上架）⊇ 服务卡片（桌面常驻入口，FormExtensionAbility，静态/动态）+ 实况窗（Live View Kit，进行时任务的胶囊/卡片/锁屏实时形态，随任务结束消亡不持久化）。卡片既是 App 也是元服务的入口；实况窗是通知的活形态。
- **Intent Kit**：应用在意图声明文件 **insight_intent.json** 声明意图（名称/参数 schema/入口）；两模式：**意图调用**（系统→应用，小艺理解意图带参调起）与**意图共享**（应用→系统，完成时/将来时两机制+辅助实体，供本地索引与主动建议）；垂域各有意图 Schema；上架需在小艺开放平台注册意图并审核，**约 3 个工作日**；ext 组件 insightIntentUI 可被小艺以窗口形态调起；API26 新增 skill 类型 Bundle（AI 技能包，仅预置）。[SK]Intents Kit简介/接入流程
- **多端协同**：分布式软总线+超级终端；应用侧=MULTI_DEVICE_CONNECTION 长时任务+DISTRIBUTED_DATASYNC 权限+跨设备迁移 API。
- **工具链**：DevEco Studio；hvigor（构建：build-profile.json5+hvigorfile.ts）；ohpm（包管理：oh-package.json5，OHPM 中心仓/私仓）；hdc（调试，系统视角）；hap-sign-tool（签名）；DevEco Testing/Inspector（细则未核实）。
- **ArkUI 声明式**：UI=f(state)（@Component/@State/@Builder 等装饰器状态驱动），产物 .abc 字节码（方舟运行时）；ArkTS=TS 约束子集（禁 any 等）。
- **BerryOS 借鉴**：① insight_intent.json"声明式意图 schema+平台审核"是 AI 原生 OS 的正确抽象——BerryOS 的 intent(u8) 第一类输入（ADR #42/#58 六档）可对齐为 .bap `[intent]` 表（动词+实体 schema），系统意图路由而非应用轮询；② 意图共享（将来时/完成时）给 AI 调度提供合规的用户行为先验数据源；③ hvigor/ohpm 分离对应 just+未来 berry-pm，仓库协议 v1 定死。
- **反面**：意图审核走线下人工（3 工作日）说明 schema 治理成本高；BerryOS v1 只开放固定动词集，不做自由 schema。

## BerryOS 若照抄会踩的坑（8 条）
1. **抄 module.json5 的字段广度**：60+ ext 类型、10 万字节文档是 15 年生态沉积；v1 manifest 只留 app 身份/组件/能力请求/入口四节，未知键硬拒绝。
2. **抄权限字符串+三张级别表**：换前缀照搬=把声明式权限污染进纯能力模型；BerryOS 只该出现"服务名+档位"，映射表在 appd 内。
3. **抄 ACL 的手工 profile 编辑体验**：开发者改 JSON 调试、上架又走商店二次申请，双轨漂移是 HarmonyOS 最大吐槽点；BerryOS 受限能力从第一天只有一条申请通道。
4. **长时任务按业务名枚举**（DATA_TRANSFER/VOIP/星闪…无限膨胀）：应改成"持什么资源证据"（音频流/定位流/传输流 handle），枚举封闭、资源开放。
5. **抄 slot+channel 双层通知模型**：HarmonyOS 6 档还算克制，Android 式渠道无限列表是反面教材；BerryOS 通知档位固定 3 档且入 manifest。
6. **抄卡片"格数档位+五套刷新机制并存"**（updateDuration/定点/多定点/下次刷新/代理刷新互斥矩阵）：文档都写不清；BerryOS 只留"定时刷新（配额）+代理推送"两轨。
7. **忽略"manifest 声明必须被运行时强制"**：HarmonyOS 大量字段靠上架审核兜底（reason 文案/usedScene 正确性），端上不校验；BerryOS 无商店红利，appd 必须在安装/启动时全量强制校验，否则规范即空文。
8. **预装/系统应用走第二套格式**：HarmonyOS 用同一 HAP+profile 差异实现系统/三方统一是对的；若给系统应用开 .bap 后门格式，能力模型第一性（handle 即权限）即被击穿。
