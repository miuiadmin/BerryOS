# GUI interaction quality: navigation, launch feedback, swipe, and boot truth
Type: plan
Status: active
Updated: 2026-11-13

一句话说明：修复三键导航的可见结果、补齐真实点击/启动反馈、量化并优化当前 CPU 光栅与 virtio-gpu/VNC 显示链，并把 Boot Progress 扩成真实服务里程碑进度。

## 背景

2026-11-13 的 workflow 六路调查确认三组问题不是单点瑕疵：BACK/HOME/TASK 的事件链已到 `gui-wmd`，但隐藏窗口只改 WM 表，合成 VMO 旧像素无人清除，宿主工具栏也没有按下态；应用点击在 launcher 帧循环内同步等待 `APP_SPAWN`，且 DOWN/UP 同批抵消按压态，冷启动 3.715s 期间无任何中间反馈；滑动每帧由 BerryUI/BerryGFX guest CPU 全量光栅，经 `gui-wmd` 逐行 syscall 拷贝、vgpud 8.3MiB 级 `TRANSFER_TO_HOST_2D`、VNC Raw 与宿主全窗重绘，实测单帧约 107～209ms；Boot Progress 在产品 vgpu 主显示上只有 450/520/560/700/1000 五个真值点，且多数背靠背发生。

## 阶段

1. **P0 导航可见闭环**：WM 可见性变化后唤醒 SYSTEM launcher 并强制一次全屏重绘，清除被隐藏 app 的旧像素；宿主 BACK/HOME/TASK 增加立即按下态，QMP down/up 注入移出 winit UI 线程；空操作给出可见反馈。验收：前台 Settings 按 HOME/BACK 后画面同帧回桌面，串口 `WMNAV` 与像素变化一一对应。
2. **P0 点击与启动反馈**：launcher 在点击当帧置 launching 视觉，再发起 `APP_SPAWN`；按压态最短可见且覆盖 Dock；后续将 spawn RPC 异步化，并以真实首帧/退出事件清除占位。验收：点击后 300ms 内出现可见反馈，冷启动期间不再无响应，重复点击不再双起 Settings。
3. **P0 Boot Progress 真值细化**：新增系统启动里程碑台账，把 vgpu ready、wmd、inputd、fsmgr、vnet、launcher、SYSTEM 连接、首个 SUBMIT 与桌面交接映射为至少 9 个单调 permille 点；继续禁止运行时 wall-clock 插值。验收：`VGPUBOOT` 序列不少于 9 个真实状态点且 smoke/GUI8 零漂。
4. **P1 显示链第一轮减载**：优先落地宿主 VNC 32bpp 快路径、避免整帧 clone，并让 launcher 多 damage rect 不被单包络放大；为滑动建立 damage/帧率遥测。验收：`gpu2-desktop-calib` page/back/switch 与 `VGPUPERF` 均不劣化且有可解释下降。
5. **P1 wmd 合成与呈现流水**：把逐行 VMO syscall 拷贝改大块/映射拷贝，评估 HDI 双缓冲与异步 frame callback，使 raster 与 virtio-gpu 传输重叠。验收：GUI8 soft↔vgpu 像素共识零漂，IDLE 零唤醒不变。
6. **P2 结构性呈现路径**：按设备壳既有设计评估 shm ring/ivshmem 或宿主 Metal presenter 消费同一 WM composition 真相源，VNC 退输入或回退路径。验收：不建立第二像素真相源，产品仍保持 1080×1920、HVF 与 virtio-gpu 契约。

## 阶段 1 结果（2026-11-13）

- **已落**：WM 可见性变化通过 `PE_FL_SYSTEM_INVALIDATE` 唤醒 SYSTEM launcher 并全屏重绘；宿主工具栏有 pressed/click ripple，QMP 导航键脉冲不再阻塞 winit；launcher 有真实 Opening 状态并先呈反馈再阻塞 `APP_SPAWN`；inputd 空闲收割 100ms→16ms。
- **已落**：Boot Progress 扩为 9 个真实单调里程碑（450/500/520/560/620/700/800/900/1000）；900/1000 只归 SYSTEM surface 0，避免 GUI8 gfxdemo 提前消费桌面交接点。
- **已落第一轮显示减载**：wmd 源行 bulk-read + 目的行按全屏 stride 写、宿主 VNC 32bpp 快路径、BerryUI sparse scrolling damage。
- **实测**：产品 e2e `ENTER-SYSTEM PASS`，9 个 `VGPUBOOT` 点、真实 Berry Demo 启动、`WMNAV home/task/back` 各一次、`WMNAVINVAL sid=1` 三次、无 panic；`smoke-image` 147/147。
- **未收官**：最终 GUI8 `--vgpu` 在修复两个集成问题后被宿主上无关 TCG shell 外部 SIGKILL，需等宿主负载清空后重跑；滑动仍低于目标帧率，P2 pipeline/shm-ring 仍必要。

## 度量

- 宿主单调钟记录：toolbar click → `WMNAV`；pointer UP → launcher press frame；click → app first frame。
- Guest 侧只使用状态谓词与 `VGPUPERF`，不用串口输出耗时冒充呈现耗时。
- 基线：vgpu cold launch 3.715s、page 2.083s、back 2.333s、switch 1.490s、整窗 submit 107～209ms。
- 目标先定体验门槛：按下反馈小于 300ms；导航状态可见小于 500ms；滑动中间帧不少于 3 帧；启动全程有 truthful 状态反馈。绝对性能目标在本机第二轮标定后收敛。

## 边界

- 不降 1080×1920，不移除 HVF或 virtio-gpu。
- 不用定时器制造 boot progress 或应用启动成功假象。
- 不让普通 app 获得 present/HDI 能力，不在每个 app 内自制系统三键。
- 不以宿主截图替代真实显示面；所有优化必须过 soft↔vgpu 像素共识。
