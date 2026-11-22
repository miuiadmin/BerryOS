# 对标补集 2·三号：HarmonyOS 应用规范深集（第二轮）

> 采集：2026-11-08。**增量补集**——续 `应用规范-HarmonyOS对标-2026-11.md`（A–M），不重复其已核实条目；引用处标"一轮§X"。
> 通道：openharmony/docs master raw 直抓（本次新增 ~60 篇）+ Harmony-Skills 镜像（harmonyos-docs-lookup / harmonyos-sdk-api-lookup，7788→6708 md）。web_search 工具仍 401 故障；consumer.huawei.com/cn/harmonyos/design/ 返回 404（SPA+无此页）。
> 来源缩写：**[OH]**=github.com/openharmony/docs/blob/master/zh-cn/application-dev/；**[OH-UX]**=…/zh-cn/design/ux-design/；**[SK]**=github.com/HarmonyOS-AI/Harmony-Skills/blob/main/harmonyos-docs-lookup/references/。纯事实档，未核到处明标。

## 1. ohos.permission.* 全量权限表（master 四表共 660 条）

四张官方清单及口径（表头声明原文）：**开放·系统授权** 57 条（全部 normal/system_grant，面向所有应用）[OH]security/AccessToken/permissions-for-all.md；**开放·用户授权** 19 条（全部 normal/user_grant）[OH]…permissions-for-all-user.md；**受限开放** 90 条（"开放范围为普通应用，但需通过 ACL 跨级别申请"；调试改 UnsignedReleasedProfileTemplate.json 的 apl 字段，商用须市场签发）[OH]…restricted-permissions.md；**系统应用可用（ACL 可申请）** 494 条（"仅对 APL system_basic 及以上开放，均为 system_grant，可经 ACL 跨级申请"；491 条"是否支持证书授权=true"，3 条=false：PROXY_MESSAGE_AUTH/MANAGE_VOLUME_ENCRYPTION_POLICY/MANAGE_SKILL_PRIVILEGE；级别分布 350 system_basic/141 system_core/3 normal）[OH]…permissions-for-systems-apps.md。
- 补一轮§B：`PUBLISH_AGENT_REMINDER` 与 `KEEP_BACKGROUND_RUNNING` 均为 **normal/system_grant 开放权限**（一轮"开放级别未核实"已解）；`KEEP_BACKGROUND_RUNNING_SYSTEM`、`KEEP_BACKGROUND_RUNNING_SPECIAL_SCENARIO` 为 system_basic 受限（ACL）。
- 系统表 494 条动词前缀分布：ACCESS 101 / MANAGE_* 57 / GET 34 / SET 19 / READ 19 / WRITE 15 / START 10 / QUERY 9 / INSTALL 9 / USE 8 / EXEMPT 7（full list 在源文件，此处不展开）。
- **权限组（20 组，弹窗合并单位；位置/通讯录/通话记录/电话/信息/日历例外逐条展示）**：位置(LBG+LOCATION+APPROXIMATELY)/相机/麦克风/通讯录(READ|WRITE_CONTACTS)/日历(READ|WRITE_CALENDAR+READ|WRITE_WHOLE_CALENDAR)/健身运动/身体传感器/图片和视频(WRITE|READ_IMAGEVIDEO+MEDIA_LOCATION)/音乐和音频(WRITE|READ_AUDIO)/广告跟踪(APP_TRACKING_CONSENT)/读取已安装应用列表(GET_INSTALLED_BUNDLE_LIST)/多设备协同(DISTRIBUTED_DATASYNC)/蓝牙(ACCESS_BLUETOOTH)/星闪(ACCESS_NEARLINK)/电话(ANSWER_CALL+MANAGE_VOICEMAIL)/通话记录(READ|WRITE_CALL_LOG)/信息(6 短信权限)/剪切板(READ_PASTEBOARD)/截屏(CUSTOM_SCREEN_CAPTURE)/文件夹(READ_WRITE_DOWNLOAD|DESKTOP|DOCUMENTS_DIRECTORY)；旧 READ_MEDIA 等已 deprecated。[OH]security/AccessToken/app-permission-group-list.md
- 下表缩写：APL：normal / s_basic=system_basic / s_core=system_core；授权：sys=system_grant / user=user_grant / manual=手动设置授权(manual_settings)。受限表全部需 ACL；开放表不涉及证书授权字段。

### 1a. 开放·user_grant（19）
| 权限 | APL | 授权 | 起始API | 权限组 | 简介 |
|---|---|---|---|---|---|
| `ACCESS_BLUETOOTH` | normal | user | 10 | 蓝牙 | 允许应用接入蓝牙并使用蓝牙功能 |
| `MEDIA_LOCATION` | normal | user | 7 | 图片和视频 | 允许应用访问用户媒体文件中的地理位置信息 |
| `APP_TRACKING_CONSENT` | normal | user | 9 | 广告跟踪 | 允许应用读取开放匿名设备标识符 |
| `ACTIVITY_MOTION` | normal | user | 7 | 健身运动 | 允许应用读取用户的运动状态 |
| `CAMERA` | normal | user | 9 | 相机 | 允许应用使用相机 |
| `DISTRIBUTED_DATASYNC` | normal | user | 7 | 多设备协同 | 允许不同设备间的数据交换 |
| `LOCATION_IN_BACKGROUND` | normal | user | 7 | 位置信息 | 允许应用在后台运行时获取设备位置信息 |
| `LOCATION` | normal | user | 7 | 位置信息 | 允许应用获取设备位置信息 |
| `APPROXIMATELY_LOCATION` | normal | user | 9 | 位置信息 | 允许应用获取设备模糊位置信息 |
| `MICROPHONE` | normal | user | 8 | 麦克风 | 允许应用使用麦克风 |
| `READ_CALENDAR` | normal | user | 8 | 日历 | 允许应用读取日历信息 |
| `WRITE_CALENDAR` | normal | user | 8 | 日历 | 允许应用添加、移除或更改日历活动 |
| `READ_HEALTH_DATA` | normal | user | 7 | 身体传感器 | 允许应用读取用户的健康数据 |
| `ACCESS_NEARLINK` | normal | user | 12 | 星闪 | 允许应用接入星闪并使用星闪能力 |
| `READ_WRITE_DOWNLOAD_DIRECTORY` | normal | user | 11 | 文件夹 | 允许应用访问公共目录下Download目录及子目录 |
| `READ_WRITE_DOCUMENTS_DIRECTORY` | normal | user | 11 | 文件夹 | 允许应用访问公共目录下的Documents目录及子目 |
| `CUSTOM_SCREEN_CAPTURE` | normal | user | 14 | 截屏 | 允许应用获取屏幕图像 |
| `READ_MEDIA` | normal | user | 7 | 文件 | 允许应用读取用户外部存储中的媒体文件信息 |
| `WRITE_MEDIA` | normal | user | 7 | 文件 | 允许应用读写用户外部存储中的媒体文件信息 |

### 1b. 开放权限·系统授权 system_grant（57，全部 normal，免 ACL）
| 权限 | APL | 授权 | 起始API | 权限组 | 简介 |
|---|---|---|---|---|---|---|
| `USE_BLUETOOTH` | normal | sys | 8 | | 允许应用查看蓝牙的配置 |
| `GET_BUNDLE_INFO` | normal | sys | 7 | | 允许查询应用的基本信息 |
| `PREPARE_APP_TERMINATE` | normal | sys | 10 | | 允许应用关闭前执行自定义的预关闭动作 |
| `PRINT` | normal | sys | 10 | | 允许应用获取打印框架的能力 |
| `DISCOVER_BLUETOOTH` | normal | sys | 8 | | 允许应用配置本地蓝牙 |
| `ACCELEROMETER` | normal | sys | 7 | | 允许应用读取加速度传感器的数据 |
| `ACCESS_BIOMETRIC` | normal | sys | 6 | | 允许应用使用生物特征识别能力进行身份认证 |
| `ACCESS_NOTIFICATION_POLICY` | normal | sys | 7 | | 在本设备上允许应用访问通知策略 |
| `GET_NETWORK_INFO` | normal | sys | 8 | | 允许应用获取数据网络信息 |
| `SET_NETWORK_INFO` | normal | sys | 8 | | 允许应用配置数据网络 |
| `GET_WIFI_INFO` | normal | sys | 8 | | 允许应用获取Wi-Fi信息和使用P2P能力 |
| `GYROSCOPE` | normal | sys | 7 | | 允许应用读取陀螺仪传感器的数据 |
| `INTERNET` | normal | sys | 9 | | 允许使用Internet网络 |
| `KEEP_BACKGROUND_RUNNING` | normal | sys | 8 | | 允许Service Ability在后台持续运行 |
| `NFC_CARD_EMULATION` | normal | sys | 8 | | 允许应用实现卡模拟功能 |
| `NFC_TAG` | normal | sys | 7 | | 允许应用读写Tag卡片 |
| `PRIVACY_WINDOW` | normal | sys | 9 | | 允许应用将窗口设置为隐私窗口 |
| `PUBLISH_AGENT_REMINDER` | normal | sys | 7 | | 允许该应用使用后台代理提醒 |
| `SET_WIFI_INFO` | normal | sys | 8 | | 允许应用配置Wi-Fi设备 |
| `VIBRATE` | normal | sys | 7 | | 允许应用控制马达振动 |
| `CLEAN_BACKGROUND_PROCESSES` | normal | sys | 7 | | 允许应用根据其包名来识别并清理相关的后台进程 |
| `COMMONEVENT_STICKY` | normal | sys | 7 | | 允许应用发布粘性公共事件 |
| `ACCESS_CERT_MANAGER` | normal | sys | 9 | | 允许应用进行查询证书及私有凭据等操作 |
| `RUN_DYN_CODE` | normal | sys | 11 | | 允许系统方舟运行时引擎在受限模式下执行动态下发的方舟 |
| `READ_CLOUD_SYNC_CONFIG` | normal | sys | 11 | | 允许接入云空间的应用查询应用云同步相关配置信息 |
| `STORE_PERSISTENT_DATA` | normal | sys | 11 | | 允许应用存储持久化的数据 |
| `ACCESS_EXTENSIONAL_DEVICE_DRIVER` | normal | sys | 11 | | 允许应用使用外接设备增强功能 |
| `READ_ACCOUNT_LOGIN_STATE` | normal | sys | 12 | | 允许应用读取用户账号的登录状态 |
| `ACCESS_SERVICE_NAVIGATION_INFO` | normal | sys | 12 | | 允许应用访问导航信息服务 |
| `PROTECT_SCREEN_LOCK_DATA` | normal | sys | 12 | | 允许应用在锁屏后保护本应用敏感数据不被访问 |
| `ACCESS_CAR_DISTRIBUTED_ENGINE` | normal | sys | 12 | | 允许应用访问出行分布式业务引擎 |
| `WINDOW_TOPMOST` | normal | sys | 13 | | 允许应用将窗口设置为应用置顶窗口 |
| `MANAGE_INPUT_INFRARED_EMITTER` | normal | sys | 12 | | 允许应用使用红外接口 |
| `INPUT_KEYBOARD_CONTROLLER` | normal | sys | 15 | | 允许应用设置键盘功能键状态 |
| `SET_ABILITY_INSTANCE_INFO` | normal | sys | 15 | | 允许应用单独配置每个Ability的图标和标签信息 |
| `NDK_START_SELF_UI_ABILITY` | normal | sys | 15 | | 允许应用通过C API启动同应用的UIAbility |
| `GET_FILE_ICON` | normal | sys | 17 | | 允许应用获取指定类型文件的文件图标 |
| `DETECT_GESTURE` | normal | sys | 20 | | 允许应用感知手势操作 |
| `kernel.NET_RAW` | normal | sys | 20 | | 允许应用抓取网络数据包 |
| `kernel.DEBUGGER` | normal | sys | 20 | | 允许应用获取调试能力 |
| `kernel.ALLOW_DEBUG` | normal | sys | 20 | | 允许C/C++程序被调试 |
| `BACKGROUND_MANAGER_POWER_SAVE_MODE` | normal | sys | 20 | | 允许应用设置自身进程的省电模式 |
| `SET_WINDOW_TRANSPARENT` | normal | sys | 20 | | 允许应用设置主窗容器透明和去除主窗外边框阴影 |
| `START_WINDOW_BELOW_LOCK_SCREEN` | normal | sys | 21 | | 允许应用在锁屏状态下被启动 |
| `kernel.IGNORE_LIBRARY_VALIDATION` | normal | sys | 20 | | 允许独立二进制程序加载具有不同ownerid的独立二 |
| `TIMEOUT_SCREENOFF_DISABLE_LOCK` | normal | sys | 22 | | 允许应用使能超时息屏不锁屏功能 |
| `LOCK_WINDOW_CURSOR` | normal | sys | 22 | | 允许应用在窗口获焦时锁定鼠标光标 |
| `CUSTOMIZE_MENU_ICON` | normal | sys | 23 | | 允许应用在“文件管理”的右键菜单中配置自定义图标 |
| `kernel.EXEMPT_ANONYMOUS_EXECUTABLE_MEMORY` | normal | sys | 23 | | 允许声明匿名可执行内存 |
| `INHERIT_PARENT_PERMISSION` | normal | sys | 23 | | 允许子进程继承父进程的权限 |
| `GET_DONOTDISTURB_STATE` | normal | sys | 23 | | 允许应用获取系统的免打扰状态 |
| `ALLOW_COREDUMP` | normal | sys | 23 | | 允许系统将应用进程的内存转储到应用沙箱 |
| `HDR_BRIGHTNESS` | normal | sys | 24 | | 允许应用使用HDR提亮能力 |
| `CONNECT_OBJECTEDITOR_EXTENSION` | normal | sys | 24 | | 允许应用查询ObjectEditorExtensio |
| `QUERY_VOLUME_ENCRYPTION_STATUS` | normal | sys | 26.0.0 | | 允许应用查询卷加解密状态 |
| `STYLUS_FRAME_BOOST` | normal | sys | 26.0.0 | | 允许应用使用手写笔跟手性的增强接口 |
| `GET_ENTERPRISE_CONFIG` | normal | sys | 20 | | 允许应用获取行业定制配置文件 |

### 1c. 受限开放权限（90，普通应用可经 ACL 申请）
| 权限 | APL | 授权 | 起始API | 权限组 | 简介 |
|---|---|---|---|---|---|
| `SYSTEM_FLOAT_WINDOW` | s_basic | sys | 7 | — | 允许应用使用悬浮窗的能力 |
| `READ_CONTACTS` | s_basic | user | 8 | 通讯录 | 允许应用读取联系人数据 |
| `WRITE_CONTACTS` | s_basic | user | 8 | 通讯录 | 允许应用添加、移除或更改联系人数据 |
| `READ_AUDIO` | s_basic | user | 9 | 音乐和音频/文件 | 允许读取用户公共目录的音频文件 |
| `WRITE_AUDIO` | s_basic | user | 9 | 音乐和音频 | 允许修改用户公共目录的音频文件 |
| `READ_IMAGEVIDEO` | s_basic | user | 9 | 图片和视频/文件 | 允许读取用户本地公共目录的图片或视频文件 |
| `WRITE_IMAGEVIDEO` | s_basic | user | 9 | 图片和视频 | 允许修改用户公共目录的图片或视频文件 |
| `WRITE_DOCUMENT` | s_basic | user | 9 | 文件 | 允许修改用户公共目录的文档 |
| `READ_DOCUMENT` | s_basic | user | 9 | 文件 | 允许读取用户公共目录的文档 |
| `READ_WRITE_DESKTOP_DIRECTORY` | s_basic | user | 11 | 文件夹 | 允许应用访问公共目录下Desktop目录及子目录 |
| `ACCESS_DDK_USB` | s_basic | sys | 11 | — | 允许扩展外设驱动访问USB DDK接口开发USB总线 |
| `ACCESS_DDK_HID` | s_basic | sys | 11 | — | 允许扩展外设驱动访问HID DDK接口开发HID类扩 |
| `READ_PASTEBOARD` | s_basic | user | 11 | 剪切板 | 允许应用读取剪贴板 |
| `FILE_ACCESS_PERSIST` | normal | sys | 11 | — | 允许应用支持持久化访问文件Uri |
| `INTERCEPT_INPUT_EVENT` | s_basic | sys | 11 | — | 允许应用拦截输入事件 |
| `INPUT_MONITORING` | s_basic | sys | 7 | — | 允许应用监听输入事件 |
| `SHORT_TERM_WRITE_IMAGEVIDEO` | s_basic | user | 12 | — | 允许应用保存图片、视频到用户公共目录 |
| `READ_WRITE_USER_FILE` | s_basic | sys | 13 | — | 允许应用访问并修改用户目录下的文件 |
| `READ_WRITE_USB_DEV` | s_basic | sys | 13 | — | 允许应用连接设备 |
| `GET_WIFI_PEERS_MAC` | s_basic | sys | 8 | — | 允许应用获取对端Wi-Fi设备的MAC地址 |
| `SET_TELEPHONY_ESIM_STATE_OPEN` | s_basic | sys | 13 | — | 允许运营商应用添加eSIM配置文件 |
| `kernel.DISABLE_CODE_MEMORY_PROTECTION` | s_basic | sys | 14 | — | 允许应用禁用本应用的代码运行时完整性保护 |
| `kernel.ALLOW_WRITABLE_CODE_MEMORY` | s_basic | sys | 14 | — | 允许应用申请可写可执行匿名内存 |
| `kernel.ALLOW_EXECUTABLE_FORT_MEMORY` | s_basic | sys | 14 | — | 允许系统JS引擎申请带MAP_FORT标识的匿名可执 |
| `MANAGE_PASTEBOARD_APP_SHARE_OPTION` | s_basic | sys | 14 | — | 允许应用设置或移除剪贴板数据的可粘贴范围 |
| `MANAGE_UDMF_APP_SHARE_OPTION` | s_basic | sys | 14 | — | 允许应用设置或移除其使用UDMF支持的数据分享范围 |
| `ACCESS_DISK_PHY_INFO` | s_basic | sys | 15 | — | 允许应用获取硬盘的硬件信息 |
| `PRELOAD_FILE` | s_basic | sys | 15 | — | 允许应用预加载文件以提升文件打开速度 |
| `SET_PAC_URL` | s_basic | sys | 15 | — | 允许应用设置代理自动配置脚本地址 |
| `PERSONAL_MANAGE_RESTRICTIONS` | s_basic | sys | 15 | — | 允许设备管理应用管理个人设备限制策略 |
| `START_PROVISIONING_MESSAGE` | s_basic | sys | 15 | — | 允许应用启动设备管理业务部署流程 |
| `USE_FRAUD_CALL_LOG_PICKER` | s_basic | sys | 15 | — | 允许应用使用诈骗通话记录选择器 |
| `USE_FRAUD_MESSAGES_PICKER` | s_basic | sys | 15 | — | 允许应用使用诈骗短信选择器 |
| `PERSISTENT_BLUETOOTH_PEERS_MAC` | s_basic | sys | 16 | — | 允许应用固化对端蓝牙设备MAC对应的虚拟地址 |
| `ACCESS_VIRTUAL_SCREEN` | s_basic | sys | 16 | — | 允许应用管控虚拟屏 |
| `MANAGE_APN_SETTING` | s_basic | sys | 16 | — | 允许应用读取或设置APN信息 |
| `GET_WIFI_LOCAL_MAC` | s_basic | sys | 8 | — | 允许应用获取本机的Wi-Fi设备的MAC地址 |
| `kernel.ALLOW_USE_JITFORT_INTERFACE` | s_basic | sys | 16 | — | 允许应用调用JITFort接口更新MAP_FORT内 |
| `GET_ETHERNET_LOCAL_MAC` | s_basic | sys | 13 | — | 允许应用查询以太网当前MAC地址 |
| `kernel.DISABLE_GOTPLT_RO_PROTECTION` | s_basic | sys | 17 | — | 允许应用关闭进程内.got.plt段的只读保护 |
| `USE_FRAUD_APP_PICKER` | s_basic | sys | 18 | — | 允许应用使用诈骗应用选择器 |
| `ACCESS_DDK_DRIVERS` | s_basic | sys | 18 | — | 允许扩展外设驱动客户端绑定到扩展外设驱动服务端 |
| `ACCESS_DDK_SCSI_PERIPHERAL` | s_basic | sys | 18 | — | 允许扩展外设驱动访问SCSI DDK接口开发SCSI |
| `kernel.SUPPORT_PLUGIN` | s_basic | sys | 19 | — | 允许主体应用安装插件 |
| `CUSTOM_SANDBOX` | s_basic | sys | 18 | — | 允许应用将沙箱类型改为动态沙箱 |
| `MANAGE_SCREEN_TIME_GUARD` | s_basic | sys | 20 | — | 允许应用调用屏幕时间守护相关接口 |
| `CUSTOMIZE_SAVE_BUTTON` | s_basic | sys | 20 | — | 允许应用自定义保存控件的图标和文本 |
| `GET_ABILITY_INFO` | s_basic | sys | 20 | — | 允许应用根据URI或者FileType查询Abili |
| `ACCESS_FIDO2_ONLINEAUTH` | s_basic | sys | 20 | — | 允许应用使用通行密钥服务的NDK |
| `USE_FLOAT_BALL` | s_basic | sys | 20 | — | 允许应用使用全局闪控球的能力 |
| `AUTO_RESTORE_MAIN_WINDOW` | s_basic | sys | 24 | — | 允许应用使用全局闪控球的自动恢复到应用主窗口的能力 |
| `DLP_GET_HIDE_STATUS` | s_basic | sys | 18 | — | 允许应用使用信息隐藏接口 |
| `READ_LOCAL_DEVICE_NAME` | s_basic | sys | 20 | — | 允许应用获取本机的设备显示名 |
| `atomicService.MANAGE_STORAGE` | s_basic | sys | 20 | — | 允许原子化服务申请差异化的存储空间大小 |
| `KEEP_BACKGROUND_RUNNING_SYSTEM` | s_basic | sys | 20 | — | 允许应用申请特殊类型长时任务 |
| `LINKTURBO` | s_basic | sys | 20 | — | 允许应用使用多网并发等网络加速能力 |
| `ACCESS_NET_TRACE_INFO` | s_basic | sys | 20 | — | 允许应用使用网络探测和获取TraceRoute信息的 |
| `READ_WHOLE_CALENDAR` | s_basic | user | 9 | 日历 | 允许应用读取所有的日历信息 |
| `WRITE_WHOLE_CALENDAR` | s_basic | user | 9 | 日历 | 允许应用添加、移除或更改所有的日历活动 |
| `SET_SYSTEMSHARE_APPLAUNCHTRUSTLIST` | s_basic | sys | 21 | — | 允许应用配置系统分享的目标应用名单 |
| `HOOK_KEY_EVENT` | s_basic | manual | 21 | — | 允许应用拦截键盘输入事件 |
| `WEB_NATIVE_MESSAGING` | s_basic | sys | 21 | — | 允许应用使用Web本地通信能力 |
| `SUBSCRIBE_NOTIFICATION` | s_basic | sys | 22 | — | 允许应用读取系统或者已安装应用发布的通知 |
| `CUSTOM_SCREEN_RECORDING` | s_basic | manual | 22 | — | 允许应用无需弹窗录制屏幕 |
| `ALLOW_EXTERNAL_NATIVE_CODE` | s_basic | sys | 23 | — | 允许应用使用外部native程序 |
| `GET_IP_MAC_INFO` | s_basic | sys | 22 | — | 允许应用获取网络设备的IP和MAC地址 |
| `atomicService.MANAGE_AGING` | s_basic | sys | 23 | — | 允许原子化服务申请差异化的老化策略 |
| `ACCESS_USER_FULL_DISK` | s_basic | manual | 22 | — | 允许应用无需弹窗访问用户公共路径 |
| `kernel.LOAD_INDEPENDENT_LIBRARY` | s_basic | sys | 20 | — | 允许应用加载二进制证书签名的共享库 |
| `CRYPTO_EXTENSION_REGISTER` | s_basic | sys | 22 | — | 允许应用注册和注销crypto扩展 |
| `MANAGE_MEDIA_RESOURCES_FOR_PUBLIC` | s_basic | sys | 23 | — | 允许应用获取和管理当前设备正在播放的媒体资源 |
| `MANAGE_RECENT_SNAPSHOT` | s_basic | sys | 26.0.0 | — | 允许应用设置在多任务中和Dock栏悬停显示的预览图 |
| `SET_WINDOW_ALPHA` | s_basic | sys | 26.0.0 | — | 允许应用设置主窗容器透明 |
| `MANAGE_CALL_FOR_DEVICES` | s_basic | sys | 23 | — | 允许应用管理通话状态 |
| `SET_WINDOW_TOUCH_AREAS` | s_basic | sys | 26.0.0 | — | 允许应用设置窗口事件热区 |
| `MANAGE_BLUETOOTH_ADVERTISER_NAME` | s_basic | sys | 23 | — | 允许应用自定义BLE广播名称 |
| `INPUT_DEVICE_CONFIGURATOR` | s_basic | sys | 24 | — | 允许应用绑定输入设备与显示屏幕 |
| `REGISTER_OBJECTEDITOR_EXTENSION` | s_basic | sys | 24 | — | 允许应用注册ObjectEditorExtensio |
| `ALLOW_ACCESSORY_ACCESS` | s_basic | sys | 26.0.0 | — | 允许应用使用配件接入服务完成配件关联和自动唤醒等功能 |
| `CHECK_CALL_LOG` | s_basic | sys | 24 | — | 允许应用根据特定条件(手机号码、通话时长) |
| `GET_NETWORK_STATS` | s_basic | sys | 10 | — | 允许应用查询其他应用的流量数据 |
| `ACCESS_DLP_SERVICE` | s_basic | sys | 26.0.0 | — | 允许应用或服务使用DLP提供的注册、解注册连云插件功 |
| `CONTROL_DEVICE` | s_basic | manual | 26.0.0 | — | 允许应用注入输入事件以控制本设备 |
| `KEEP_BACKGROUND_RUNNING_SPECIAL_SCENARIO` | s_basic | sys | 24 | — | 允许应用申请特殊类型长时任务 |
| `FLOAT_VIEW` | s_basic | user | 26.0.0 | — | 允许应用使用应用浮窗 |
| `GET_CALL_TRANSFER_INFO` | s_basic | sys | 26.0.0 | — | 允许应用查询呼叫转移状态 |
| `MANAGE_SKILL` | s_basic | sys | 26.0.0 | — | 允许应用使用skill的能力 |
| `kernel.AS_LDK_DRIVER` | s_basic | sys | 26.0.0 | — | 允许应用安装PCIe驱动 |
| `KNOCK_COLLABORATION` | s_basic | sys | 26.0.0 | — | 允许应用通过碰一碰的形式连接指定设备并实现信息交互和 |
| `PRINTER_DRIVER` | s_basic | sys | 24 | — | 允许应用管理打印系统 |
## 2. 应用性能规范（performance/ 目录要点 + 华为功耗检测数值）
- **功耗/CPU 红线（本轮最硬数值）**：系统每 5 分钟统计三方应用 CPU 平均负载（按核数归一化，10 核跑 5 分钟=10%），超限触发 HiAppEvent `CPU_USAGE_HIGH` 预警——**前台 >30%、后台 >10%（5 分钟均值）、单线程 >70%（1 分钟均值）**；异常抓栈默认单应用每天 1 次调用栈+1 次采样栈。[SK]系统-调测调优-Performance Analysis Kit-功耗检测.md
- 冷启动官方拆 5 阶段：进程创建&初始化→Application&Ability 初始化→Ability/AbilityStage 生命周期→加载绘制首页→网络数据二次刷新；优化示例数值（8 节实测，节选）：懒加载拆分 HAR 后 UI Ability Launching 阶段 61.3ms（优化前更大）。[OH]performance/improve-application-cold-start-speed.md
- **启动框架 AppStartup**：启动任务（StartupTask）+依赖图，自动模式（AbilityStage 构造期加载）/手动模式（startupManager.run）；`StartupConfig.timeoutMs` 所有启动任务总超时**默认 10000ms**；API20+ matchRules 按桌面卡片/通知/意图调用筛选自动任务；API21+ schedulerPhase=preAbilityStageLoad|postAbilityStageLoad（默认后者）；HSP/HAR 中任务必须 excludeFromAutoStart=true；支持 so 预加载任务；不允许循环依赖；feature HAP 自 API20 支持。[OH]application-models/app-startup.md；reference/apis-ability-kit/js-apis-app-appstartup-startupConfig.md
- **布局性能**（[OH]performance/ 目录）：① 组件复用 @Reusable——仅同父组件缓存池内复用，不能跨父；只标复用子树根；嵌套子组件需实现 aboutToRecycle/aboutToReuse；与 ForEach 全展开互斥（不能触发复用）；复用不支持嵌套使用 [OH]performance/component-recycle.md。② 自定义组件冻结——`@Component({freezeWhenInactive:true})`（仅常量），非激活页/组件不响应状态变量刷新、@Watch 不调用、节点不刷新，重新激活再统一更新（V1/V2 均支持）[OH]performance/custom_component_freeze.md。③ 冗余刷新——不可见组件动画空跑=每帧响应 Vsync；用 onVisibleAreaChange（可见时每帧计算可见性）等接口停掉屏外动画 [OH]performance/invisible-refresh-performance.md。④ 状态管理——复杂类型常量重复赋值会因代理判不等引发不必要刷新；a.b(this.object) 传原始对象不触发刷新 [OH]ui/state-management/arkts-state-management-faq-inner-component.md。⑤ 榜单文档：reduce-view-nesting-levels / proper-choice-between-if-and-visibility（用 if 条件销毁替代 Visibility隐藏）/ LazyForEach 按需加载+划出销毁回收 / component-reuse-overview（@Builder 与复用配合）。
- 帧率/时延度量法：平均帧率=完整 DoComposition 个数/时长（或 Repaint 标签）；点击完成时延以硬件中断为起点、Repaint 提交上屏为终点（示例 138.4ms）。[OH]performance/application-performance-guide.md
- **内存红线值**：OH/镜像公开文档均无"应用内存上限"数值；泄露检测仅有工具面（napi-faq-about-memory-leak、performance-optimization-using-hidumper、DevEco Profiler）→ **未核实**。"冷启动可冻结/不可冻结任务"红线分类：公开文档无此表述（仅有后台进程冻结与自定义组件冻结）→ **未核实**（疑为华为应用质量测试/上架预检内部标准）。[SK]应用框架-程序包结构-应用图标配置与开发.md 提及 DevEco Testing「UX 基础质量测试/上架预检」为华为侧质量门槛入口。

## 3. resourceschedule 资源调度子系统
- **应用分组官方枚举 GroupType**（usageStatistics，系统 API）：ALIVE_GROUP=10 活跃 / DAILY_GROUP=20 经常使用 / FIXED_GROUP=30 常用（定期） / RARE_GROUP=40 极少使用 / LIMITED_GROUP=50 受限 / NEVER_GROUP=60 从未使用；"常用应用"判定=GroupType≤30。[OH]reference/apis-backgroundtasks-kit/js-apis-resourceschedule-deviceUsageStatistics-sys.md
- **分组→延迟任务频控链路**（官方原文数值）：活跃 最小间隔 2h / 经常 4h / 常用 24h / 极少 48h / 受限、从未=禁止；能效资源豁免分组不受限。[OH]task-management/work-scheduler.md
- **能效资源申请**（仅系统特权应用=配 runningResourcesApply 特权）：EfficiencyResourcesRequest{resourceTypes, isApply, timeOut(ms), isPersist, isProcess(仅 CPU 支持进程维度), reason, cpuLevel(23+)}；ResourceType 位掩码 CPU=1/COMMON_EVENT=2/TIMER=4/WORK_SCHEDULER=8/BLUETOOTH=16/GPS=32/AUDIO=64/RUNNING_LOCK=128(10+)/SENSOR=256(10+)；EfficiencyResourcesCpuLevel：SMALL_CPU=0 小核/MEDIUM_CPU=1 中核/LARGE_CPU=2 大核（系统按负载在灭屏等空闲时段分配）。[OH]task-management/efficiency-resource-request-sys.md；reference/apis-backgroundtasks-kit/js-apis-resourceschedule-backgroundTaskManager-sys.md
- **系统负载融合档位 SystemLoadLevel**（API12+，三方可订阅）：LOW=0/NORMAL=1/MEDIUM=2/HIGH=3/OVERHEATED=4/WARNING=5/EMERGENCY=6/ESCAPE=7，各档附"无感知业务应降规格/暂停/停止/仅基础功能/强制停止"分级处置原文。[OH]reference/apis-basic-services-kit/js-apis-resourceschedule-systemload.md
- **设备待机 deviceStandby**（系统 API）：豁免资源 ResourceType NETWORK=1/RUNNING_LOCK=2/TIMER=4/WORK_SCHEDULER=8/AUTO_SYNC=16/PUSH=32/FREEZE=64；ExemptedAppInfo{resourceTypes,name,duration 豁免时长}。[OH]reference/apis-backgroundtasks-kit/js-apis-resourceschedule-deviceStandby-sys.md
- 冻结/挂起机制原文：应用退后台"一小段时间后"挂起（reasonable-running 文档实测数秒内）；**DATA_TRANSFER 长时任务仅提升进程优先级、进程仍会被挂起**，上传下载须走系统 request 代理接口托管。[OH]performance/reasonable-running-backgroundTask.md
- 华为商用 cgroup 式分组（perceptible/verified/blind/suspended/restricted）公开文档仍无 → **未核实**；官方公开口径即上述"使用频度分组"。

## 4. 后台任务四类 API 深挖（apis-backgroundtasks-kit）
- **长时任务 BackgroundMode 数值**：DATA_TRANSFER=1/AUDIO_PLAYBACK=2/AUDIO_RECORDING=3/LOCATION=4/BLUETOOTH_INTERACTION=5/MULTI_DEVICE_CONNECTION=6/VOIP=8(13+)/TASK_KEEPING=9（API21 起非 PC 设备需 ACL KEEP_BACKGROUND_RUNNING_SYSTEM）/MODE_AV_PLAYBACK_AND_RECORD=12(22+)/MODE_SPECIAL_SCENARIO_PROCESSING=13(22+，须用户授权弹窗 requestAuthFromUser；API24 起改需 ACL KEEP_BACKGROUND_RUNNING_SPECIAL_SCENARIO)/MODE_NEARLINK=14(26)。约束原文：进度超 10 分钟不更新即取消；进度通知必须实况窗；AUDIO_PLAYBACK 不接 AVSession 则后台任务模块代发通知；特殊场景类型必须单独使用不支持通知合并；BackgroundTaskSubmode 子类型（如车钥匙）API21+ 配套。[OH]reference/apis-backgroundtasks-kit/js-apis-resourceschedule-backgroundTaskManager.md
- 系统侧取消/暂停原因枚举（预留未启用但已定义）：SYSTEM_CANCEL_NOT_USE_MULTI_DEVICE=10（申请多设备互联未使用）、SYSTEM_CANCEL_USE_ILLEGALLY=11（类型与业务不符）、SYSTEM_SUSPEND_SYSTEM_LOAD_WARNING=12（系统高负载暂停）、SYSTEM_SUSPEND_NEARLINK_NOT_USED=17/…_DATA_NOT_EXIST=18（星闪无连接/无数据流，26+）。[OH]同上
- 短时任务（一轮§C 已录 10min/日、3min/次、低电 1min）API 面：transient_task C API 同步提供（capi-transient-task*）。
- **延迟任务 workScheduler**：WorkInfo 字段{workId, networkType, chargerType, batteryStatus, storageLevel, isRepeat, repeatCycleTime(ms), repeatCount, isPersisted(重启恢复), isDeepIdle, deviceId, bundleName, groupId?}；NetworkType：ANY=0/MOBILE=1/WIFI=2/BLUETOOTH=3/WIFI_P2P=4/ETHERNET=5；ChargingType：ANY=0/AC=1/USB=2/WIRELESS=3；BatteryStatus：LOW=0/OKAY=1/LOW_OR_OKAY=2；StorageLevel：LOW=0/OKAY=1。**数量限制：同应用同时最多 10 个延迟任务；循环任务间隔至少 2 小时；单次回调最长 2 分钟**（系统特权经能效资源 WORK_SCHEDULER 扩至充电 20 分钟/非充电 10 分钟）。[OH]reference/apis-backgroundtasks-kit/js-apis-resourceschedule-workScheduler.md；task-management/work-scheduler.md
- **代理提醒 reminderAgentManager**：ReminderType TIMER=0 倒计时/CALENDAR=1 日历/ALARM=2 闹钟；ReminderRequest 字段：reminderType/actionButton（**三方最多 2 按钮，系统 API10+ 最多 3**）/wantAgent/maxScreenWantAgent/notificationId/slotType/snoozeTimes(默认 0)/timeInterval（延时提醒间隔，**最少 30s**，不适用倒计时）/snoozeContent/snoozeSlotType(11+)/ringChannel(20+，默认闹钟通道)/expiredContent/title/Content/…；日历提醒支持设置/查询/删除"不提醒日期"。数量配额（一轮§C：单应用≤30、全系统≤12000）。[OH]reference/apis-backgroundtasks-kit/js-apis-reminderAgentManager.md

## 5. AGC 上架材料与审核（镜像内可得部分）
- 上架图标规则原文：PNG 格式；**不允许自行裁剪圆角、不允许资源内加内边距**；显示无模糊/拉伸/压缩/锯齿；轻量穿戴 icon.png 单层圆形 **104×104px**（Launcher）、icon_small.png **92×92px**（权限管理）；生成于 phone-xxxldpi 的 **icon.png 216×216px 可直接用于 AGC 上传**；上传图标必须与包内一致；AGC 平台素材规范有专页（doc id agc-help-app-visual-asset-spec-0000002277607976，SPA 未抓到正文 → 数值未核实）。[SK]应用框架-程序包结构-应用图标配置与开发.md
- 审核依据文档链：《华为开发者服务协议》《华为APIs使用协议》《华为推送服务使用协议》《应用审核指南》(doc/app/50104)《元服务审核指南》(doc/app/50129)——Push/Account 等细则均援引（审核细则数值本体在 SPA → 未核实）。[SK]应用服务-Push Kit…-学习Push Kit接入规范.md
- 质量测试入口：DevEco Testing 专项测试「UX 基础质量测试」（可选规则"应用图标"）、「上架预检」服务、AGC 云测试 UX 测试。[SK]同图标文档
- 图标动态管理：AppGallery Connect 可动态管理应用图标（图标管理服务，实现在端侧切换；正文 270B 仅 TOC → 细节未核实）。[SK]应用服务-AppGallery Kit-图标管理服务.md

## 6. Design 官网与 OH UX 规范数值
- consumer.huawei.com/cn/harmonyos/design/ **404**；HarmonyOS Design（色彩/字体/栅格官网值）→ **未核实**。
- **OH 官方 UX（zh-cn/design/ux-design/，60 篇）可得**：主色 #007DFF(ohos_id_color_emphasize)/反色 #006CDE/警告 #FA2A2D/警示 #FF7500/通讯 #E84826(挂断)、#00CB87(接通)；系统默认主色=蓝（人因：男女接受度与地域偏好最高、色觉障碍可辨识）。[OH-UX]visual-basis.md、visual-colors.md
- **8vp 网格**为基本单位（小控件对齐 4vp）；vp=px×(dpi/160)（160dpi 时 1vp=1px）；fp=1vp×scale（随系统字号）。分层参数（色彩/字体/圆角/间距/阴影/模糊/缩放）由《系统资源分层设计表 V1.0.xlsm》给出全量 id→设备取值。[OH-UX]visual-basis.md、design-resources.md
- **栅格推荐值**：Margin 32vp+Gutter 16vp+4 列，或 Margin 40vp+Gutter 24vp+8 列（4/8 倍数原则）。[OH-UX]grid-system.md
- 响应式四断点与设备映射：超小=穿戴/小=默认设备(手机)/中=平板/大=智慧屏与 PC；缩进布局触发条件（留白>50% 或每行>30 字）、挪移布局（宽高比变化>200%）、重复布局（缩放放大>150%）；8/12 栅格默认支持 6/8 栅格缩进与 4 栅格重复。[OH-UX]responsive-layout.md
- 应用图标：系统统一遮罩裁切；OH 提供应用图标设计模板 zip 与 Icons 资源包。[OH-UX]visual-app-icons.md、design-resources.md（一轮§F 的 24vp 系统图标规格仍有效）
- 字号层级：按设备形态给分层参数（表在 xlsm，正文无数值 → 具体字号未核实）。[OH-UX]visual-fonts.md

## 7. 一次开发多端部署（一多）
- 官方推荐路径=自适应布局（相对容器约束，无级连续）+响应式布局（断点+栅格有级变化：缩进/挪移/重复三式）+栅格断点（GridRow 默认 ["320vp","600vp","840vp"]，一轮§F）。[OH-UX]adaptive-layout.md、responsive-layout.md
- UX 四原则原文：差异性/一致性/灵活性（同类设备调整布局位置、内容宽度、横向组件数，防横竖屏切换空白模糊变形截断）/兼容性（异构设备还需支持不同输入方式与功能架构调整）；自适应应用架构示例：手机底 Tab→平板侧边 Tab→大屏顶部 Tab。[OH-UX]app-ux-design.md
- 拉起规范：API12 起不推荐显式 Want 拉起他方应用，改应用链接；**App Linking**（https+域名校验，未装跳浏览器）vs **Deep Linking**（自定义 scheme，易仿冒，多匹配弹选择框；openLink appLinkingOnly 矩阵 4 行为表）。[OH]application-models/app-startup-overview.md
- **多端协同 hop**：跨设备 startAbility/startAbilityForResult/connectServiceExtensionAbility/call 四式；约束原文——"多端协同任务管理能力尚未具备，开发者当前只能通过开发系统应用获取设备列表，**不支持三方应用接入**"；createDeviceManager 为系统 API。[OH]application-models/hop-multi-device-collaboration.md；跨设备迁移见 hop-cross-device-migration.md（可迁移/不可迁移 Ability 配置）。
- "1 基 N 特"（一套工程多设备特性分层）提法未见于 OH/SK 公开文档 → **未核实**。

## 8. Push Kit 开发者规范
- **消息两大类**：服务与通讯（锁屏+铃声+振动；系统按场景流量管控频控）/资讯营销（**静默通知仅通知中心**；按应用类别限每日数量）。[SK]应用服务-Push Kit…-开发准备-申请推送场景化消息权益.md
- **category 取值全表**：服务与通讯=IM、VOIP、MISS_CALL、TRAVEL、HEALTH、WORK、ACCOUNT、EXPRESS、FINANCE(仅金融银行/支付类应用)、DEVICE_REMINDER、MAIL(仅邮箱/办公)、PLAY_VOICE(锁屏+振动无铃声)、SUBSCRIPTION；资讯营销=MARKETING（财经动态/生活资讯/调研/其他/内容推荐/新闻/社交动态）。未开通自分类权益→默认 MARKETING；携带未授权 category→自动归 MARKETING。[SK]同上
- **频控数值**：资讯营销 单应用/日/设备 默认 **2 条**，仅"新闻（需《互联网新闻信息服务许可证》）"=**5 条**（其余 20+ 三级分类全为 2 条）；调测期 testMessage=true 每项目/日全网 **1000 条**（超限回执 256；场景化超限 256、被管控 102）；**卡片刷新消息：已上架 2 条/天、未上架 5 条/天**；实况窗 5 分钟≤10 次/小时≤60 次（出行打车与赛事比分 30/180）；单次请求 Push Token≤**1000** 个；消息体≤**4KB**（不含 Token）。[SK]Push Kit常见问题-关于通知消息被频控的问题.md；申请推送场景化消息权益.md
- **后台消息**：不显通知/铃声/角标；应用前台即传、不在前台缓存**最新 1 条、最多 7 天**；建议**每小时≤2 条**否则可能被丢；受设备电量/系统休眠/用户行为影响可能延迟。[SK]…-推送场景化消息-推送后台消息.md
- 自分类权益申请三原则（分类维度化/消息与分类一致/示例与类型对应）；语音播报权益仅商家订单收款场景；应用内通话权益仅沟通/告警类且仅服务自身用户；配套文档：通知内容管理细则、通知违规处罚标准。[SK]同目录

## 9. 服务卡片深挖（form/）
- **form_config.json 全字段**（补一轮§G）：updateEnabled（true 才有周期刷新；**updateDuration 优先级高于 scheduledUpdateTime**）/scheduledUpdateTime（24h 制精确到分，缺省 "0:0" 不刷新）/updateDuration（单位 30 分钟，N→30N 分钟，0 不生效，缺省 0）/multiScheduledUpdateTime（API18+，**最多 24 个时刻**，须配合 scheduledUpdateTime）/dataProxyEnabled（API12+；true 时**定时刷新和下次刷新不生效，但不影响定点刷新**）/isDynamic（动态|静态）/supportShapes（rect|circle，缺省 rect）/renderingMode（API15+：autoColor 可被使用方改色/可上桌面或锁屏、fullColor 桌面专用不可改、singleColor 锁屏专用可改）/fontScaleFollowSystem（缺省 true）/transparencyEnabled（背板透明卡，仅系统或申请能力者）/enableBlurBackground（模糊背板，API23+ 仅旗舰机）/conditionUpdate（API18 配置、**26 生效**，network；系统应用）/resizable+groupId（API20+，跨卡共享尺寸档）/supportDeviceTypes（API22+，缺省 phone/tablet/tv/wearable/car/2in1）。[OH]form/arkts-ui-widget-configuration.md
- 刷新配额原文：每卡每日定时刷新≤**50 次**（0 点重置），定时=setFormNextRefreshTime 也计入；代理刷新开启后定时/下次刷新失效（API26 起可同开，一轮§G）。[SK]应用框架-服务卡片-卡片更新与数据交互.md；[OH]form/arkts-ui-widget-update-by-proxy-sys.md
- **ArkTS 卡片能力限制清单（原文）**：widget.abc 运行在系统公共 FRS 渲染进程；仅 ArkUI 声明式范式、**不支持跨平台开发**；仅可导入带"卡片能力"标记的模块；支持 HAR **不支持 HSP**；**不支持 native 语言/加载 so**；仅部分组件/事件/动效/数据/状态管理 API；**卡片内不支持左右滑动控件**（防与宿主手势冲突）；不支持极速预览；不支持断点调试。[OH]form/arkts-form-overview.md#约束与限制
- 卡片三档：静态（仅 UI 组件布局，FormLink 跳转，内存开销小）/动态（+通用事件、自定义动效、postCardAction router|message|call）/互动（+破框动效，API20+）。postCardAction 的 router 对非系统应用仅支持跳自己应用的 UIAbility。[OH]form/arkts-form-overview.md；arkts-ui-widget-event-overview.md

## 10. ArkUI 性能与规范
- @Reusable 复用七规则见 §2（同父缓存池/根标记/aboutToReuse 回调/ForEach 互斥/禁嵌套复用）。[OH]performance/component-recycle.md
- freezeWhenInactive 组件冻结：V1/V2 通用，仅常量；非激活不响应状态刷新，激活时统一补偿更新。[OH]performance/custom_component_freeze.md
- UI 高性能开发官方五思路：工具驱动（CPU Profiler+ArkUI Inspector）/惰性加载优先（LazyForEach）/布局计算简化（减嵌套、高性能节点）/更新代替重建（组件复用）/状态精确控制（少建状态变量）。[SK]应用框架-ArkUI…-UI开发调试调优-UI高性能开发.md
- **Web 组件（ArkWeb）**：基于 Chromium，系统版本映射 OH4.0 前=M99 / 4.1–5.1=M114 / 6.0=M132（默认推荐，可切 M114）/ 6.1=M132 / 7.0=M144（默认，可切 M132）；能力=声明式+离屏加载、JavaScriptProxy、同层渲染、网络/媒体播放托管、无痕浏览、广告拦截、"坚盾守护模式"、crashpad 崩溃收集、DevTools 调试；访问在线网页需 ohos.permission.INTERNET。[OH]web/web-component-overview.md

## 11. 账号与 IAP
- **何时必须提供华为账号登录（上架审核硬规则原文）**：① 应用支持第三方账号登录→**必须提供华为账号登录选项**；② 元服务内构建账号体系→**必须使用华为账号登录能力**；另有官方登录设计规范专页（design-guides/id-0000001880001344，SPA 未核实）。[SK]应用服务-Account Kit…-开发者使用Account Kit的登录能力的管理细则.md
- 账号标识：OpenID（应用维度唯一）/UnionID（开发者账号维度唯一）/GroupUnionID（关联主体账号组维度）；Authorization Code→Access Token/Refresh Token/ID Token(OIDC)；scopes=申请用户数据类型（profile、quickLoginAnonymousPhone 等），permission=数据/接口权限。[SK]…-Account Kit简介.md
- Account Kit 能力×设备矩阵：获取头像昵称/手机号/风险等级=Phone/Tablet/PC2in1/Wearable/TV；收货地址、发票抬头=Phone/Tablet/PC2in1；未成年人模式=Phone/Tablet/PC2in1/TV；一键登录体验 Phone/Tablet/PC2in1 一致。[SK]同上
- **IAP 商品类型四分**：消耗型（用后即耗，无时限权益）/非消耗型（一次性永久）/自动续期订阅（连续周期，自动续费）/非续期订阅（单周期）；中国境内全四类支持，Wearable 境外仅支持存在；TV 拉起收银台为扫码支付页。[SK]应用服务-IAP Kit…-IAP Kit简介.md
- **IAP 抽成/佣金比例**：IAP 简介与接入规范均无抽成数值 → **未核实**（镜像 FAQ 有价格展示/订阅周期细则：订阅按自然月等）。

## 12. 鸿蒙应用上架备案（中国区）
- Harmony-Skills 镜像（7788→6708 md 全树检索"备案/软著/ICP"）无命中；web_search 工具 401 故障；AGC「应用信息」页（doc/app/50104-01，被图标规范援引）为 SPA 未抓到 → **APP 备案/软件著作权要求：未核实**（仅可确认：上架审核指南文档存在且被各 Kit 规范援引为合规依据链）。

## 附：本轮新增来源文件清单（均 raw 直抓成功）
[OH] performance/{performance-overview, improve-application-cold-start-speed, application-performance-guide, custom_component_freeze, component-recycle, invisible-refresh-performance, …}.md；application-models/{app-startup, hop-multi-device-collaboration}.md；reference/apis-ability-kit/js-apis-app-appstartup-{startupConfig,startupManager}.md；reference/apis-backgroundtasks-kit/js-apis-{resourceschedule-backgroundTaskManager(+sys), resourceschedule-workScheduler, reminderAgentManager, resourceschedule-deviceStandby-sys, resourceschedule-deviceUsageStatistics-sys, backgroundProcessManager}.md；reference/apis-basic-services-kit/js-apis-resourceschedule-systemload.md；web/web-component-overview.md；[OH-UX] visual-basis/colors/fonts、grid-system、responsive-layout、app-ux-design、design-resources、visual-app-icons.md；[SK] Push Kit 9 篇、Account Kit 4 篇、IAP Kit 4 篇、程序包结构-应用图标配置与开发、服务卡片-卡片更新与数据交互、Performance Analysis Kit-功耗检测、ArkUI-UI高性能开发。
