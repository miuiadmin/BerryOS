# 显示与加速体系设计（框架级重设计研究，2026-11-13）

> 缘起：用户令——不打补丁、从底层系统框架设计；1080P 初始；GPU 加速（Mac 先行）；模拟器给开发者用（性能=能用不是看图）。
> 定稿 = docs/显示与加速体系设计-v1.0.md；本目录为其证据基座。

| # | 文件 | 核心定谳 |
|---|---|---|
| 01 | `01-HVF真相与页粒度.md` | 桌面其实已在 HVF 跑（三层实锤）；**慢根因=渲染链**（每变化帧≥4 次全帧 CPU 遍历+静止帧也全帧 pread）；**16K 迁移不需要做**（HVF 对 guest 页粒度零约束，三源定谳）；两缺口立账 |
| 02 | `02-virtio-gpu-2D档-GPU加速路径兑现设计-v1.0.md` | 案 β 全取（壳窗口+VNC 增量先行→D-Bus 共享图+宿主 wgpu/Metal 终态）；案 α 本机结构性出局（cocoa 无 GL/无 virgl，实证）；全链 10-17 人日；分辨率动态自然消解 |
| 03 | `03-1080P显示框架总设计.md` | 横屏 1920×1080 定案；窗口 1:1 不缩（0.9 因子=补丁遗产退役）；launcher big 判据断点同刀修；product.toml [platform.display] 立键（定制化挂接）；density/vp 框架 |
| 05 | `05-S1spike-三验实录.md` | 刀 3 S1 spike 收口：**判定 go**（设备=TCG 串口实收 `vprobe: slot 28 did=16` 非静默枚举/传输=VNC RFB 三问应答+五轮稳定/HVF=UEFI 双 boot ktest done×2）；FDT 对设备在席零鉴别力（32 节点静态常量）；B1 硬前提三件（vprobe 白名单行/force-legacy=false/设备全名 virtio-gpu-device）；意外发现 HVF+kernel 直启+rootfs 相既有 fatal（对照法钉死与 gpu 无关，挂账）；D-Bus/memfd 面未覆盖如实记 |
| 07 | `07-B1基线-ramfb链实测.md` | B1-a 前基线锚（TCG/HVF×headless/窗口×idle/swipe/时钟 12 轮+/tmp 探针零仓改）：**idle 稳态满核 100%**（guest/FB 全静仍 QEMU 满转，归因未定谳）；**交互零重绘**（swipe/tap/200s 时钟窗 FB sha256 纹丝不动——空数据盘形态唯一真帧=boot 首帧 TCG 15.3–16.3s/HVF 11.4–11.6s）；**host 壳面 7–9% 单核**（02 号「静帧全帧 pread」定量兑现，纯读仅 6%）；**pread 8.29MB p50=0.993ms** 假设验证过；三工具陷阱（screenshot 恒 VNC 黑/fb= 无 0x 前缀/ps 不可用）+HEAD 镜像两缺陷（漏提交 3 行导出+HVF ktest fatal）如实记 |
