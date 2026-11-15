# BerryBoot Boot Progress P1
Type: plan
Status: done
Updated: 2026-11-15

一句话说明：为 1080×1920 Mac 模拟器建立真实 0→100% 启动进度框架，并在 BerryBoot ramfb 首屏与 vgpud 主显示之间传递系统阶段真值。

## 背景

产品开机此前缺少系统级加载过程：BerryBoot 只打串口阶段行，Mac 主显示在 vgpud 接管前没有真实进度体验。用户追裁不能靠假动画或降分辨率，必须从 boot/display 服务框架实现。

## 步骤

1. 建立 `berry-bootprogress` 纯函数 crate：permille 真值、阶段词表、几何布局、native framebuffer 渲染与单调性。
2. BerryBoot 读取 `opt/berry/display`，产品在装载器阶段直接把 ramfb 设为 1080×1920，超固件 GOP 缓冲时钉完整帧页并写 fw_cfg DMA。
3. BerryBoot 用 KERNEL/INITRD 实际字节与段装载状态推进 0→100%，并输出 `BOOTPROG ok display WxH`。
4. present 域增加 add-only `BootProgress` 控制帧；vgpud 用私有 VMO 渲染系统阶段并 flush 到主显示。
5. svcmgr 在 gui-wmd 首帧前发送 450/520/560/700 permille，随后关闭私有 dup，wmd 首帧自然替换进度面。
6. smoke-image 与 GUI8 把 BootProgress 状态谓词纳入守门。

## 结果

现行规格见 [BerryBoot-BootProgress-P1-v1.2.md](../../docs/BerryBoot-BootProgress-P1-v1.2.md)。产品实测已有 1080×1920 0%、滚动中与满进度 ramfb 首屏，以及 vgpu 70% 进度与 LAUNCH 后桌面替换。

## 未完成事项

_（P1 无；QEMU Cocoa 单主 console 从 ramfb 首屏到 vgpu 桌面的宿主切换归 P2。）_
