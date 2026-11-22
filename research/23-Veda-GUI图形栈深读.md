# Veda GUI/窗口/图形栈深读

> 调研日期 2026-11-08 · 方法:精读 /tmp/os-src/Veda/documentation/ARCHITECTURE.md 的 The window system / Displays and GPUs / The shell / Graphics and the toolkit / OpenGL ES and the GPU / Video / Applications 七节(全文 1227 行中的 599–1070 行),辅以 implementation/ 下 services/compositor、shell/、libraries/{proto,ui,gfx,gl}、guests/linux/{kms,renderer} 的目录结构与关键文件头注抽查(读 lib.rs/main.rs/协议文件头部注释,未逐行读实现);对照面只读 docs/GUI里程碑规划-2026-10-v1.0.md 与 docs/事件循环A7设计草案-v1.0.md 头部。
>
> 本文为 23 号专题（原 20 号与既有 Apple-iOS 对标报告撞号，2026-11-08 收口改号）;结论均注出处(文件:行号或节名),不自造事实。
> 兄弟篇:24 号(原 12,Veda 全景四家深读,含同意机制三件)、25 号(原 13,轨迹与踩坑)。
> **编号注记**:research/ 目录已有 应用规范-Apple-iOS对标-2026-11.md（原占 20 号位，后去号更名）,本文件按任务指定名落库,编号撞号如实记录,归档时再统一调号。
>
> 禁 git 操作、禁改代码,本报告为纯调研产物。

## 0. 一句话总评

Veda 的 GUI 栈是一个单人作者用纯 Rust 从光栅原语写到 OpenGL ES 3.0 编译器、再到桌面 shell 与锁屏的完整垂直栈,合成器/协议/工具包三层全部自研且决策逻辑全部 no_std host 可测;GPU 加速通过 driver VM 里 Linux+Mesa 的旁路实现,合成器本体 CPU 合成为基线、GPU 直画为优化路径。与 BerryOS GUI 线(B 形态协议/pass 形态 API/berryui 声明式)同构度最高的部分在窗口协议语义与停帧策略,差异最大的是 immediate-mode 工具包与 driver VM GPU 路线。

## 1. 问题①:窗口系统架构

### 1.1 服务形态

compositor 是单进程系统服务,同时提供三个协议服务:`display`(客户端创建窗口/挂 buffer/present 帧)、`input`(输入驱动上报键鼠事件)、`displaydev`(显示驱动交付可翻页的 picture)——compositor/main.rs:5–14 头注,ARCHITECTURE.md:601–603 同口径。关键工程决策:窗口系统**决定什么**(窗口/叠序/焦点/输入/权限/限额)全部放在 crate `compositor` 的 source/lib.rs 里(`#![no_std]`,wm/lock/idle/pointer/keys/input/keymap/switcher/window/buffers/subsurfaces/outbox/clients/limits 十四个模块),在宿主机上测试;main.rs 只负责服务协议、映射客户端内存与绘制——lib.rs:5–22 模块注释,ARCHITECTURE.md:603–605。服务形态上它是 Program::native(runtime-only no_std,按 musl target + rust-lld 链接、不碰 Linux 任何东西),理由是「必须及时应答的(window system, sound mixer)」——ARCHITECTURE.md:276–278(User space 节表格);对比之下 shell 与应用是 Rust std 程序(同表)。

### 1.2 合成模型:谁画、怎么提交 buffer、damage 跟踪

**提交模型(双/三缓冲 + FrameDone 配速)**:客户端在共享 VMO 里放两个 buffer(或三个,游戏边画边等上一个在途),画 back buffer 后 `present`;合成器取走帧后回 `FrameDone` 事件——它既释放再前一个 buffer(客户端零像素拷贝地按显示器节奏配速),又告诉客户端帧何时被看到;**不可见窗口(最小化、被锁屏盖住)的帧既不合成也不确认**,直到重新可见——ARCHITECTURE.md:607–613,proto/display.rs:7–13 头注同口径。客户端内存在 attach 时检查,且**从不借为 Rust slice**(客户端可能正在写它)——ARCHITECTURE.md:613–614。

**damage 跟踪三层**:(a) 客户端 present 请求自带 `damage: Vec<Rect>`(上限 DAMAGE_MAX=64 个矩形,空 = 全量)——proto/display.rs:366–376、:176;(b) ui 工具包整帧重画后**自行 diff 出差异行带**(bands of rows)只 present 行带——ARCHITECTURE.md:837–838,ui/app.rs:7–9 头注「A frame is drawn whole, and only what it changed of the frame on screen is presented」;(c) 合成器按 damage 驱动合成,每显示帧至多一次,把客户端画的差异累进自己的屏上 damage——compositor/render.rs:5–7「damaged regions are redrawn bottom to top」、wm.rs:8–10。合成路径两档:显示能翻页且 GPU 能画进其 picture 时,GPU 用 OpenGL ES 每帧直画下一个显示 picture(每个窗口/子表面是纹理,阴影圆角渐变是 shader);否则 CPU 画进 back buffer 再拷屏——ARCHITECTURE.md:632–640,compositor/gpu.rs:5–31。GPU 路径失败有 read-back 首帧检查,之后单帧超 2s 判死回退 CPU,服务恢复后重设至多 3 次(GPU_TIMEOUT_NS/GPU_RETRIES,render.rs:20–26)。

**子表面(subsurface)是零拷贝直通道**:进程内任意连接可为自己进程的窗口创建子表面(浏览器引擎线程独立 present 网页页帧),buffer 挂在各自的 memory object 上、attach 一次、逐个 present;present 应答返回上一个 buffer(合成器不再读它),GPU 还在读时经 fence 后由 `SubsurfaceEvent::Released` 延迟归还;窗口自己的帧负责「放置」子表面并声明 overlays 区域——GPU 路径下子表面 buffer 是**客户端内存上的纹理 in-place 采样,零拷贝**,网页页帧不经浏览器拷贝或重画窗口就直达屏幕——ARCHITECTURE.md:615–631,proto/display.rs:21–32。

**事件反压**:落后客户端的事件在合成器里等待并合并,携带状态的不丢(按键只与其释放一起丢)——outbox.rs:5–22 详述合并规则:最新的尺寸/状态/焦点/帧确认**替换**同类等待事件;pointer motion 与 scroll 合并进最后一个;到达 LIMIT 后丢 press 时连 release 一起丢,release 永不孤行。主循环一个 loop 先服务输入驱动、再每客户端轮流服务几个请求,帧到期即插一次合成——「no client can hold it」,main.rs:28–33、ARCHITECTURE.md:701–704。

### 1.3 客户端协议动词(display 协议全录)

proto/display.rs:359–476 定义 31 个方法,按序号:窗口生命周期 create_window(spec→(id, event channel))/attach_buffers(VMO 里 count≤3 个 buffer,行距 stride≤width+64)/present(index, damage, subsurface 放置, overlays)/set_title/set_state/destroy_window/set_cursor/screen_info(单数!返回 width/height/work_area)/剪贴板 set_clipboard·get_clipboard(限聚焦窗口进程与 shell)/set_position;窗口管理(权限见 1.4)list_windows(shell 与 agent)/activate_window/minimize_window/close_window/arrange_window(最大化/还原/贴边);启动协作 desktop_ready(shell 报桌面就绪,启动画面溶入);锁屏五件 lock/unlock/watch_lock(锁状态订阅)/set_idle_timeout(lock 服务专属)/keep_awake(id, awake);在场判定 user_active(agent 专属);子表面五件 create_subsurface/attach_subsurface_buffer/detach_subsurface_buffer/present_subsurface/destroy_subsurface。事件侧:Configure/CloseRequested/Focus/PointerMove/PointerButton/PointerLeave/Scroll/Key(带 keymap 产物 text)/FrameDone{shown, seen_ns, period_ns}——**帧确认带被看到时刻与帧周期**,客户端动画可对未来帧对时——WindowsChanged/StartMenuKey/AgentKey/Appearing(display.rs:281–313);子表面事件 FrameDone/Released(:327–332);锁通知 LockNotice::Changed(display.rs:345)。

**窗口分层与弹窗约束**:desktop/普通窗/无框窗/panel(预留屏空间)/popup/通知六层;popup 只能为本应用聚焦中的窗口开、至多半屏、随焦点走;**装饰(标题栏/边框/阴影)由合成器画**,所以挂死客户端的窗口仍可被拖动;close 请求 5 秒不答标为 not responding,再请求一次直接关连接——ARCHITECTURE.md:653–659。资源限额:每进程 64 窗 + 16 屏面积的 buffer、256 子表面 + 64 屏面积的子表面 buffer(ARCHITECTURE.md:669–671)。

### 1.4 权限模型(by identity)

display 协议注释与 ARCHITECTURE.md:660–671 给全表:桌面/panel/通知/管理他人窗口/开始菜单键与 agent 键 = 系统服务 shell;list_windows = shell 与 agent;剪贴板 = 聚焦窗口的进程(shell 也可写);锁屏/解锁/idle 超时 = lock 服务;触发锁 = shell/agent/lock 服务三方;**user_active(用户在场)唯独 agent**;keep_awake 只对自己的普通/无框窗;只有 device-manager 起的驱动(含 driver VM 内程序)能上报输入或 attach 显示。与 BerryOS「能力即权限」的差异:Veda 的授权单位是**服务身份**而非 capability token,由合成器按连接身份裁决(谁是谁由 clients.rs 跟踪,lib.rs:20)。

## 2. 问题②:GPU 驱动面

### 2.1 GPU 与 userspace 驱动形态:driver VM 里的 Linux+Mesa 全栈

显示与 GPU 都是 Linux 的,住在 driver VM 里(DRIVERVM.md,ARCHITECTURE.md:713–714)。形态是**旁路渲染服务**:driver VM 里的 renderer 程序(implementation/guests/linux/renderer,结构 = decoder/ 的 C 半(Mesa 构建的解码器)+ source/ 的 Rust 半(服务 gpu 协议),「cargo automation linux」构建)在 Mesa Gallium 驱动上跑——iris(i915 或 xe,Intel GPU)/virgl(virtio-gpu,QEMU)/softpipe(测试用,driver-virtual-machine.renderer=softpipe)——ARCHITECTURE.md:914–919。QEMU 下 driver VM 拿 QEMU VGA(bochs 驱动)+ virtio-gpu(Mesa virgl),virglrenderer 用宿主 GPU 渲染(ARCHITECTURE.md:724–726)。显示侧 kms 程序(implementation/guests/linux/kms)经 Linux KMS 驱动持有固件 framebuffer 的显示器,经 displaydev 接上合成器;合成器画进 write-combining VMO,Linux 当 dma-buf 原样显示,vblank 翻页,翻页时刻打上 Veda 时钟,无原生模式时显示引擎缩放(ARCHITECTURE.md:716–721);kms/drm.rs:5–9 头注列 DRM 面:connector/mode 枚举、PRIME 导入内存的 framebuffer、atomic mode set、page flip 事件,rustix 直发 ioctl。

### 2.2 OpenGL ES 栈:自研 API 层 + 自研编译器 + 双后端

**全自研,无移植**:`gl`(implementation/libraries/gl,纯 Rust)给应用 OpenGL ES 3.0 + GLSL ES 1.00/3.00;`gl::veda::context` 在 gpu 服务存在且 renderer 能力够时用 GPU、否则全 CPU 软渲(有 GPU 服务时说明原因);`gl::veda::gpu_context` 只要 GPU 否则报错——ARCHITECTURE.md:852–858。`glsl` 库(implementation/libraries/glsl)是自研 GLSL ES 编译器:parse/check/link,每阶段降 SSA 形式做优化与验证,`glsl::ops` 每操作只定义一次(常量折叠与解释器共用);**双后端**发软渲染器字节码与 virgl 的 TGSI;shader 源码按敌意输入处理——一切可增长的结构有界、一切索引钳制,任何 shader 不能让编译器 panic 或无界运行——ARCHITECTURE.md:860–867。

**软渲染器 gl::soft**:三角形装 64 像素 tile 桶,tile 并行光栅化,一次着色 16 个 fragment,4x MSAA,全 ES 3.0 格式,ETC2,精确 sRGB(ARCHITECTURE.md:884–886)。**GPU 后端 gl::virgl** 说 virglrenderer 协议:状态对象缓存、只发变化、共享 staging buffer 传数据、fence 过后复用释放的 buffer;glBufferData 保存储、host 按序执行、读回前自等 fence;renderbuffer 4 采样;并维护一张**九行 host 兼容 workaround 表**(ANGLE 越界读丢绘制、整数常量运行时构造、transform feedback 缓冲每缓冲一个 vertex shader 变体、3D 纹理切片经 2D 图像、无 glCopyImageSubData 时经 blit 或 guest 内存、Intel Windows 驱动的 transform feedback 与 blit 顺序缺陷、逐传输读回、新资源自清零、stencil 存 depth-stencil 格式)——ARCHITECTURE.md:887–912。

**renderer 侧安全与等待**:每连接一个 context + 一块共享内存(commands/staging/query/fence word),布局为 proto::gpu::Session 状态,`gl` 在映射前检查——**renderer 不被信任读客户端内存**;命令逐字 copy 出共享内存、逐词检查后才喂 Gallium——「客户端只能 fail 自己的 context,never the renderer」;一个 poll 同时等客户端连接与每 context 最老 pending fence 的 sync_file,无 sync_file 的设备才 1ms 轮询;格式按 virgl 约定(depth 在 S8_UINT_Z24_UNORM 高位),iris 低位则在拷贝时翻;**gpu::import 让 renderer 直接画进合成器的显示 picture**(dma-buf,virgl 不行因其宿主画在自己的内存);i915 上显示 picture 映射为 uncached(Gen12 LLC 缓存显示引擎不读,会出陈旧扫描行),Meteor Lake 之后与 xe 做不到就拒绝内存、合成器回退 CPU 画;处理器也读的内存(WebKit 网页)保 i915 缓存一致性映射;只读内存只许采样——ARCHITECTURE.md:921–961。

**egl 层**:给链接进程序的 C 代码 EGL 1.5 + ES 3.0 C 函数,over `gl`:surfaceless display、FBO/pbuffer、每线程 current context、share group(失败即不渲不软渲)、fence、按名取函数(eglGetProcAddress,libepoxy/WebKit 用)——ARCHITECTURE.md:876–883。

## 3. 问题③:工具链/工具包

### 3.1 分层栈(全部自研 + 点缀第三方解码)

ARCHITECTURE.md:821–830 表:`raster`(路径/二次三次曲线/圆弧、变换、描边、带 coverage span 的抗锯齿 scanline 光栅化)→ `font`(基于 ttf-parser:TrueType/CFF、metrics、kerning、光栅 glyph、glyph 位图 cache、文本布局)→ `image`(JPEG/PNG/BMP 用 zune 解码器、QOI 用 qoi crate、fast_image_resize 缩放;自家 PNG/BMP 编码器 deflate 用 miniz_oxide、JPEG 用 jpeg-encoder)→ `gfx`(`Canvas`:填充/渐变/圆角矩形/路径/位图/带圆角图片/阴影/文字布局渲染;降采样与模糊;**两帧间 damage 计算**;DrawCache 跨帧保留)→ `ui`(工具包)→ `text`(Text Editor 的编辑模型:行缓冲/选区/成组撤销/搜索/软换行/语法高亮)。Crate 目录印证:libraries/{raster,font,image,gfx,ui,text} 均在(Repository layout 节,ARCHITECTURE.md:1203–1205)。

### 3.2 ui 工具包形态:immediate-mode

`ui` 是 immediate-mode 工具包:应用的 `update` 每帧布局并绘制全部控件、同一次调用里控件报告交互;跨帧状态住在 per-window 的 `UiState`、按 widget id 键控;**帧只在有输入或动画要帧时画**——ARCHITECTURE.md:832–835。lib.rs:5–31 列模块:window(display 协议客户端封装,FrameTiming)/app(事件循环 run 驱动 App 或 Host)/fonts(SharedText)/agent(把应用能力报给 voice agent)/media(播放器控件)/subsurface/menu/text_input/status(托盘 glyph)/theme(dark)。app.rs:7–13 头注补帧经济细节:整帧画、只 present 与屏上帧差异部分;有 subsurface 时只有画在其上的部分花窗口的帧;帧在「有东西要帧」(输入/等待的 handle/自请求重画)时画,且**处理过输入的帧之后再画一帧**(输入可能改变了它处理前已画的内容)。控件清单:按钮/开关/滑条/文本框/列表/滚动区/标签页/菜单/模态对话框/文件对话框/矢量图标/托盘状态 glyph/暗色主题/secret_input(锁屏密码框:只收键盘、禁拷出贴入,ARCHITECTURE.md:785–786)——ARCHITECTURE.md:829。

性能与移植纪律:「画多少花多少」——路径只对 clip 矩形光栅化,画过的东西经 gfx::DrawCache 跨帧保留;**像素循环保持标量**,因为 QEMU TCG 用慢 helper 模拟 SSE 整数乘法与 shuffle(gfx::color 注明如何检查生成代码)——ARCHITECTURE.md:841–845。一进程一个文本上下文 SharedText:字体解析一次、glyph 光栅进一个 cache(ARCHITECTURE.md:839–840)。图标体系:线性图标 24 单位格路径,托盘状态 glyph 20 像素格、边缘对齐整像素(ARCHITECTURE.md:846–848)。

### 3.3 输入路由

输入路由在合成器不在工具包:键经 keymap(US 布局)到聚焦窗;pointer 事件到指针下的窗口或按键按下时所在的窗口;窗口键(Super+方向/Super+D/Alt+Tab/Alt+F4/单击 Super 唤开始菜单)是合成器自留的——ARCHITECTURE.md:704–709;输入驱动消失时按住的键与按钮被释放(ARCHITECTURE.md:708–709)。

## 4. 问题④:桌面 shell 与锁屏

### 4.1 桌面 shell

shell(implementation/shell/components/desktop)是**普通进程**,合成器按系统服务 shell 的身份信任它——ARCHITECTURE.md:744–745。它的全部 surface(桌面/taskbar/开始菜单与日历/通知)是 ui::Host,从一个共享模型画、经主循环执行的 action 改模型;窗口清单来自 compositor(list_windows + WindowsChanged 刷新),已装应用来自 launcher(包增删时重取)——ARCHITECTURE.md:746–751。源文件印证分工:desktop/taskbar/start/calendar/notify/chrome/approval/firewall/power/volume/wifi/tooltip(startup 源文件清单)。shell 协议让应用换壁纸与发通知(展示应用自己的 tile 与名字,每进程 10 秒至多 5 条);通知还报应用崩溃(launcher::watch)与承载防火墙问题(其按钮出现 1 秒后才可点,防误点);Restart/Shutdown 逐窗请求关闭如同点关闭键,全部关闭后才断电;**超时纪律:调任何服务至多等 10 秒(关机前写盘等 2 分钟),挂死的服务冻结不了桌面**——ARCHITECTURE.md:753–765。

### 4.2 锁屏(窗口管理细节,补 24 号已记的 agent 抑制联动)

24 号报告已记「锁屏抑制 agent 监听 + presence 语义」(research/24-2026新OS全景与四家深读.md:29),本节补窗口管理面。**compositor::lock 的窗口层规则**(ARCHITECTURE.md:672–681):锁屏期间合成器只画 lock 服务的窗口(WindowKind::Lock,压在所有其他层之上),罩在自己的 backdrop 上直到锁屏画出了自己(锁屏没画好前不露出任何锁屏要遮的内容);只有锁屏窗口拿焦点与指针;窗口键全部失效;display::lock 或 Super+L 即锁,同时终止进行中的移动/切换器/弹窗;解锁只认 lock 服务(display::unlock),焦点回到锁前窗口;**装了锁屏的系统 init 起来就是锁着的(崩溃重启后也是)**;系统服务经 display::watch_lock 订阅锁状态(先报现状再报每次变化)。

**idle 锁的配速语义**(ARCHITECTURE.md:682–700,compositor/source/idle.rs):超时由 lock 服务设(display::set_idle_timeout,0 = 永不,未设即永不);计时起点是最后输入/最后解锁/屏幕停止被保活的时刻(电影演完也给用户全程时间);重复设置不重置计时(防重启的 lock 服务把锁一直吊着);keep_awake 语义三条件——客户端显式请求(Videos 播视频/Photos 放幻灯/Interceptor 游戏中)+ 窗口可见(未最小化、未被上层整窗遮住)+ **全屏不算**(暂停并留在全屏的电影、一张图片,不让无人值守的电脑保持解锁)——对齐 Android FLAG_KEEP_SCREEN_ON 与 Wayland idle inhibit 的「应用说何时有东西在播」原则;user_active 唯独 agent 可发、锁屏时为 no-op、只在与用户的对话中发(免提对话不会因锁屏中断)、**旁听内容永不触发**。

**lock 服务本身**(ARCHITECTURE.md:767–819):显示锁屏/要密码/独家解锁,决策在 lock-core(host 测,状态时序布局在 640x480 到 4K 屏上验)。密码管理:Settings 经 lock 协议设置/改/删;服务只存 salted Argon2id 哈希(PHC 串,19 MiB 内存两遍,OWASP 选型),私有目录 /home/.private/lock,整文件改名一步落盘;读回的记录做格式校验(只认 Argon2id 1.3、参数在界内)。校验独立线程跑(Argon2 花的那零点几秒里环灯动画不停);常数时间比较;输入即从内存擦除;密码框只收键盘(secret_input)。**错密码节流**:前四次免费,之后 30s/1min/5min/15min 递增(lock_core::throttle),计数落盘——重启服务或机器都占不到便宜,Settings 里输错也计入。idle 默认值跟密码走(lock_core::idle):设了密码 5 分钟、没设永不——理由写成一篇小论文:设密码说明机器要防人,无人解锁的屏幕正违背之;没密码锁屏也拦不住任何人,锁了只是打扰;每次到达/重启/改值都把时间报给 compositor。**fail-safe 三连**:lock 服务崩 = 屏幕保持锁着、compositor 画 backdrop、等 init 重启它;compositor 崩 = 重启即锁;哈希记录在但读不出 = 任何密码都不解锁。视觉:静止态壁纸加 veil + 大时钟日期;一键一击升起卡片(模糊壁纸 + 头像环 + 密码框 + 提示行);解锁时环胀大淡出、屏幕向桌面淡出。

## 5. 问题⑤:显示热插拔/多屏

**单屏系统,无多屏支持**——这是与「任务问句预期」最直接的不符处,如实记录。证据:display 协议 screen_info() 返回单个 ScreenInfo{width, height, work_area}(proto/display.rs:233–238,`9 => fn screen_info() -> ScreenInfo` :382);compositor 的 screen 模块文档全部单数「the screen」(screen.rs);ARCHITECTURE.md:641–652 通篇「the firmware's framebuffer」;文档与协议无第二显示器、无显示器枚举、无输出拓扑概念。limits 里「16 screens' worth of buffers」(ARCHITECTURE.md:670)是**内存限额的计量单位**(屏幕面积的同义词),不是多屏。

**驱动的动态 attach/detach 有完整降级路径**(这是热插拔的退化形态):合成器启动先画固件 framebuffer(in-place,60Hz);能 flip 的显示驱动经 displaydev attach 后,每帧画进不在屏上的 picture、下一 vblank 显示、显示后合成下一帧(无撕裂、按屏配速);**只有 device-manager 起的驱动可 attach**;驱动消失或 flip 挂起 1 秒 → 帧画进其全部 picture 与固件 framebuffer;合成器启动后**不动固件 framebuffer 直到确认内存还是屏幕的**(没驱动要来/来的是纯像素驱动/20 秒过去),因为驱动可能自用它(Intel GPU aperture)——ARCHITECTURE.md:641–652。displaydev 协议头注补:请求与完成走共享页(FlipState)+ 两事件(request/done),每边只写自己字段、同时至多一个在途 flip;完成带 vblank 开始时刻与帧周期(合成器动画按帧将被看到的时刻对时);**驱动消失后合成器继续画进全部 picture(不知屏上显示哪个);合成器消失后驱动回显固件 framebuffer 再 attach 下一个合成器**——displaydev.rs:5–60;开机黑屏直到「帧从此去的屏就绪」(displaydev::expect:device-manager 告知显示控制器的驱动要来),保证启动画面第一次就出现在桌面将来的屏上——displaydev.rs:「What is coming」节。KMS 侧枚举 connector 与 mode(kms/drm.rs:6–7),但那是 Linux 驱动侧的职责,不暴露给 Veda 协议面。

## 6. 对照 BerryOS:启示与不适用面

BerryOS 口径基线(docs/GUI里程碑规划-2026-10-v1.0.md 头部与 §3/§4):berryui = 自研声明式(API 面声明式、引擎内核保留树 + 脏区、布局自研,ADR #52;:78);渲染 API = pass 形态 WebGPU 语义子集(GUI-2,:92);窗口协议 = B 形态三件事 submit(buffer, dirty_rects)/frame_callback(未来时刻表 + credits≤2)/release(裁决 I,:84、:91);低功耗缝 = damage K≤8 + FULL 位、三态停帧 RUN·SETTLE·IDLE(IDLE 撤定时器)、vote 区间票面、per-surface 帧率意图位(G/C1,:82);帧循环地基 = timer/wait_many/pthread-futex 已全实现(:§2 表行 6);A7 草案补充 BerryOS 事件循环本体是 POSIX 原语批(信号/timer/mmap),FPU 面归 M-GUI 线在途(docs/事件循环A7设计草案-v1.0.md 头部)。

### 6.1 启示(可直接对账的)

**启示一:B 形态三件事在 Veda 有完整同构实证,且 Veda 证明「帧确认带时刻」是正确形态。** Veda 的 present(damage 矩形集)+ FrameDone{shown, seen_ns, period_ns} + buffer 归还(present 应答/Released 事件)= BerryOS submit + frame_callback + release 的活体样本;Veda 双/三 buffer + FrameDone 释放前一 buffer 正是 credits≤2 的自然实现(credits 的物理意义 = 在途帧数上限);displaydev 完成带 vblank 时刻与帧周期 → FrameDone 的 seen_ns/period_ns,恰是 BerryOS「帧回调未来时刻表」的服务端实现路径——合成器自己从显示驱动拿 vblank 时刻表,再批发给客户端。BerryOS 的「协议位 day-one 定形,后补 = 协议破裂」(GUI-10 C1 纪律)在 Veda 得到反向印证:display 协议一版之内就装下了 damage/子表面/锁/idle/保活全语义,没有后期加动词的补丁痕迹(动词序号 1–44 连续紧凑,proto/display.rs:359–476)。

**启示二:双侧停帧与 BerryOS 三态停帧同向,Veda 补了服务侧一档的语义细节。** Veda 客户端「帧只在有输入或动画要帧时画」(ARCHITECTURE.md:835)+ 服务端「不可见窗口不合成不确认」(:612–613)= RUN 与 IDLE 的两侧;BerryOS 的 SETTLE(停帧过渡态)在 Veda ui 里有一个精确对应物:「处理过输入的帧之后再画一帧,因为输入可能改变了它处理前已画的内容」(ui/app.rs:11–13)——停帧前多画一帧的规则,BerryOS 三态状态机设计可以直接吸收。另一条:重复设置 idle 不重置计时、计时起点含「停止保活时刻」(ARCHITECTURE.md:685–688)是 BerryOS vote/power·mode op 设计时同类边界条件的现成答案。

**启示三:事件反压合并规则是 gui-inputd 的第一道闸,合并语义要按「是否携带状态」分类立法。** outbox.rs 的四条规则(状态类替换、通知不重复入队、motion/scroll 合并进最后一个、press-release 成对丢弃且 release 永不孤行)是慢客户端下输入通道不炸的完整答案;BerryOS 手势 referee(规划 §2 闭包件 9)处理的是识别与仲裁,outbox 处理的是**传输层合并**,两层 BerryOS 都要,且 Veda 这套规则可直接改写成 berry-guiproto 输入通道的验收断言。

**启示四:装饰权在合成器 = 客户端挂死时系统仍可交互,移动形态同样需要。** PC 形态下 Veda 合成器画标题栏让挂死窗口可拖动、5 秒无应答标 not responding、再关连接(ARCHITECTURE.md:656–659);移动形态下 BerryOS 状态栏/通知卡片/应用切换器若全部由 berry 侧系统进程持有绘制,应用 ANR 时系统 UI 不死——与 BerryOS「单 GUI 进程内嵌 berry-ui + 三薄服务」(规划 §3-C)并不冲突:三薄服务里 gui-wmd 持系统 chrome 的绘制权即可获得同款韧性。

**启示五:权限按服务身份裁决的全表 + keep_awake 语义,是 BerryOS 能力面设计的高质量对账材料。** Veda 用连接身份(shell/lock/agent/driver/普通进程)而非 token,在合成器一处集中裁决 31 个动词(:359–476 每条注释都写明谁可调);BerryOS 内核态 handle 天然更细,但「每个协议动词注释里写清谁可调、被拒不解释」的文档纪律值得 berry-guiproto 照抄。keep_awake 三条件(显式请求 + 可见 + 全屏不算)与 user_active 唯 agent、旁听永不触发,对 BerryOS 省电支柱(AI 原生调度同理:intent 输入算不算用户在场)是现成语义模板。

**启示六:决策库/协议/绘制三分与 BerryOS「逻辑进 host 可测库」完全同构,Veda 把它贯彻到全栈。** compositor 的 lib.rs(no_std、host 测)+ lock-core + videos-core + browser-core + interceptor-game 全是同一模式(ARCHITECTURE.md:603–605、:1033–1036、:1040–1043);BerryOS 已有同款惯例(fbcon 28 host 测,规划 §2),Veda 证明该模式可以撑起整个 GUI 栈的测试面——berryui 的保留树 + 脏区引擎内核(GUI-3)照此切分,视图 diff/脏区合并/三态停帧状态机全部可 host 测(规划 §4 GUI-3 验收列已这么写,方向正确)。

**启示七:Veda 的 virgl workaround 表是「借宿主渲染器」路线的公开风险清单,BerryOS virtio-gpu spike 前必读。** ARCHITECTURE.md:902–912 九条 host 缺陷绕行(ANGLE 越界、整数常量 denormal 化、transform feedback 变体、3D 纹理、Intel Windows 驱动两条、逐传输读回、新资源清零、stencil 格式)实证了 GUI-09「借宿主栈 = 继承宿主全部怪癖」的判断;BerryOS S1 spike(virtio-gpu 实测,规划头部注明暂留 GUI-2 刀前)的 kill criteria 应对照这张表逐条问。同理 renderer 侧「命令逐词检查后喂 Gallium、客户端只能 fail 自己的 context」(:934–939)是 BerryOS 未来 GPU 服务(guifbd 之后端)信任模型的可抄样板:GPU 服务不信任客户端共享内存,与 compositor 不把客户端内存借为 Rust slice(:613–614)是同一条铁律的两次应用。

**启示八:子表面(零拷贝子图直出屏)是 B 形态协议 day-one 该想清楚要不要留位的语义。** Veda 子表面让浏览器页帧经 GPU in-place 采样直达屏幕、视频帧经 frame pool 直达窗口(ARCHITECTURE.md:615–631、:1019–1024),移动端对应物 = 视频卡片/相机预览/网页内嵌。BerryOS v1 闭包(15 件)无此件,但 buffer 导入(import 别的进程内存为纹理)与 fence 语义(virgl 场景 Released 事件的成因)若 day-one 不留,以后加 = C1 拍板时点名的同类协议破裂;最低成本动作 = 在 berry-guiproto 的 buffer 对象上预留「可被服务端直接采样」能力位,与 per-surface 帧率意图位同批落。

### 6.2 不适用面(语言/规模/形态差异,诚实记录)

**不适用一:immediate-mode 与声明式是哲学分叉,BerryOS 不能也不必跟。** Veda ui 每帧全布局全绘制 + UiState 按 widget id 存跨帧态(ARCHITECTURE.md:832–834);BerryOS 拍板 berryui 声明式 + 保留树 + 脏区(ADR #52),两者的状态管理/widget id/依赖追踪机制不可互换。Veda 路线的隐含代价:整帧重画 + 自行 diff 行带(:837–838)意味着框架层永远做全量比较,树级脏区(视图知道自己脏)天然少做这部分;BerryOS GUI-3 的「脏区合并 + Measure/Render 保留」正是树级优势,不必被 Veda 的成功带偏。同理 Veda 的文本 DSL 缺位(无 DSL,ui 就是 Rust API)与 BerryOS C2(文本 DSL 为主、AST 导出)无对照面。

**不适用二:GPU 全栈自研的规模门槛,Veda 是单人多年全职深潜的产物,BerryOS 无此预算。** Veda 自研了 GLSL ES 编译器(SSA + 双后端)、ES 3.0 完整 API 面、软渲染器(tile binning + MSAA + 全格式)、virgl 协议后端、egl C 层——Repository layout 节 libraries/{glsl,gl,egl} 一行装下(ARCHITECTURE.md:1204),但那是数万行级的专深工程;BerryOS 的取舍(pass 形态 API + soft 永久 CI 基线 + 后端兑现序列 soft→virtio-gpu→Vulkan,规划 §7/:163)是规模正确的选择,Veda 的价值在提供各环节的语义参照而非实现路径。BerryOS 已拍板自研光栅核(sparse strips 思想子集)与 Veda raster/gfx 的覆盖面差距大但方向同源,Veda 的「像素循环保持标量因为 TCG 模拟 SSE 慢」(:843–845)是 BerryOS QEMU 主验环境下同款约束的直接预警——berry-gfx 快速路径先做标量版,向量化为后端兑现序的一环。

**不适用三:driver VM 路线 BerryOS v1 无对应物。** Veda 的 GPU/显示/KMS 全经 driver VM 里的 Linux(§2.1),前提是它自带完整 hypervisor + 驱动 VM(24 号报告已记其 virtualization 模块);BerryOS v1 驱动用户态直跑、显示走 HDI 三口(ramfb = RegVBlank(软)/Prepare/Commit,规划 §4 GUI-1),guifbd 对 ramfb 的软 vblank 与 Veda displaydev 的硬 flip 在「完成带 vblank 时刻」语义上可以对齐,但实现机制完全不同,不构成移植面。

**不适用四:PC 桌面 shell 的窗口管理细节不进 BerryOS M-GUI 闭包。** taskbar/开始菜单/Alt+Tab 切换器/贴边(arrange_window 的 snapping)/窗口装饰是 PC 形态资产;BerryOS 验收形态是图标网格 + 滑动桌面 + 状态栏(规划 §1),窗口管理只剩单应用全屏栈。可带走的是超时纪律(等服务至多 10s,:763–765)与通知限速(每进程 10 秒 5 条,:754–755)这类系统卫生参数,不是窗口管理本身。

**不适用五:多屏互相不构成参考。** Veda 单屏(§5),BerryOS 移动优先也单屏;唯一可带走的是驱动消失/合成器消失的**双向降级路径**(:647–648、displaydev 头注)——「显示服务死了,驱动回显最底层帧缓冲等下一个显示服务」,对 BerryOS guifbd 崩溃重启时 ramfb 上保留最后一帧的设计是同款思路。

## 7. 与预期不符处(诚实记录)

一、**GUI 栈完成度远超 24 号报告「102★ 且处早期」的印象**。24 号(research/24:9)记 Veda「仅 102★ 且处早期」,本次深读发现其图形栈已到「自研 GLSL 编译器带 SSA 优化与验证、virgl 后端带九条 host workaround、子表面 GPU 零拷贝 in-place 采样、锁屏 Argon2id + 落盘节流 + 三重 fail-safe」的完成度——星数与成熟度在此项目上严重脱钩,它更像单人多年的全职作品;24 号的「早期」判断对 GUI 线不成立,应以此稿为准。二、**GPU 合成不是主路而是优化路径**:读前预期 GPU 合成为默认,实际 CPU 合成是基线与回退(GPU 首帧 read-back 检查、2s 超时、3 次重试的整套防御就是为随时放弃 GPU),架构叙述顺序也是 CPU 在先(ARCHITECTURE.md:632–640、gpu.rs:28–31)。三、**无多屏与热插拔协议**:任务问句预设了多屏/热插拔的可能,实际是单屏 + 驱动级 attach/detach 降级路径,无显示器枚举/拓扑/输出热插拔事件(§5);记录此不符以免后来者再找。四、**display 协议比预期「窗口协议」宽得多**:31 个动词里近三分之一是锁/idle/保活/在场判定/启动协作(desktop_ready/ Appearing),窗口系统实际上兼任了电源与在场的裁决点——这本身是本稿最重要的启示来源(启示二、五)。

## 8. 出处索引

- /tmp/os-src/Veda/documentation/ARCHITECTURE.md:§The window system(599–709)/§Displays and GPUs(711–726)/§The shell(728–819)/§Graphics and the toolkit(821–848)/§OpenGL ES and the GPU(850–961)/§Video(998–1036)/§Applications(1038–1070)/§User space(265–283)/§Repository layout(1192–1227)。
- /tmp/os-src/Veda/implementation/services/compositor/source/:main.rs(5–33 头注)/lib.rs(5–22)/wm.rs(5–30)/outbox.rs(5–22)/gpu.rs(5–31)/render.rs(5–26)。
- /tmp/os-src/Veda/implementation/libraries/proto/source/display.rs(5–56 头注、153–176 常量、233–238 ScreenInfo、281–345 事件、359–476 协议动词)与 displaydev.rs(5–60 头注)。
- /tmp/os-src/Veda/implementation/libraries/ui/source/lib.rs(5–31)与 app.rs(5–19)。
- /tmp/os-src/Veda/implementation/guests/linux/kms/source/drm.rs(5–9)与 main.rs(5–13);renderer 目录结构(decoder/ C 半 + source/ Rust 半)。
- 对照面:/Users/murongcheng/Documents/code/berryos/docs/GUI里程碑规划-2026-10-v1.0.md(头部 + §1 + §2 表 + §3-C/G/I + §4 GUI-1/2/3 + §7);docs/事件循环A7设计草案-v1.0.md(头部);research/24-2026新OS全景与四家深读.md(:9、:29)。
