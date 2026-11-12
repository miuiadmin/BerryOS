# virtio-gpu 2D backend for the Mac emulator
Type: plan
Status: active
Updated: 2026-11-13

一句话说明：在既有 soft/ramfb 显示链上接入真实 virtio-gpu 2D 后端，用 damage/flush 消除 Mac 模拟器的逐行 bounce copy 与盲目整面刷新，并保持 soft 后端永久 CI 基线。

## 背景

HVF1 已把 Mac 产品桌面固定在 1080×1920、修复 HVF 内核差异并接通真实 partial damage（[BerryMacEmulator-HVF1-v1.0.md](../../docs/BerryMacEmulator-HVF1-v1.0.md)）。当前显示仍由 guest CPU 光栅到 ramfb；QEMU 11.0.3 的 `apple-gfx-pci` 无法给 EDK2/BerryBoot 提供 GOP（`eco: fb absent`），不能冒充 GPU 加速。

现行架构已预留本计划：ADR #50 规定 ramfb 第一实现、virtio-gpu 第二实现且 HDI 契约不变；ADR #51 规定 soft → virtio-gpu 2D → native 后端序列。研究结论：下一步选 **guest virtio-gpu 2D**，不是 Mac 侧第二套 DisplayList 光栅器。它不把 BerryUI 光栅迁出 guest CPU，但能把 guifbd 每次 SUBMIT 的逐行 `berry_vmo_read`/拷贝换成 guest 资源 attach + `TRANSFER_TO_HOST_2D` + 带 damage 的 `RESOURCE_FLUSH`，并让 host 只重绘损坏区。

## 步骤

1. **Phase 0 spike（可弃）**：在 Mac 桌面 QEMU 拓扑追加 `-device virtio-gpu-device`，记录 AAVMF 是否绑定 GOP、双 console Cocoa 行为、设备槽位与 HVF 吞吐。出口 = go/no-go 与性能预期，不合入生产 recipe。
2. **Phase 1 codec**：新增 `drivers/berry-virtio/src/gpu.rs`，定义 `DEVICE_ID_GPU=16`、24B 控制命令头与 GET_DISPLAY_INFO / RESOURCE_CREATE_2D / SET_SCANOUT / RESOURCE_FLUSH / TRANSFER_TO_HOST_2D / RESOURCE_ATTACH_BACKING / DETACH_BACKING 编解码。验证 = host 单测与 no_std/零依赖检查。
3. **Phase 2 driver**：新增 `drivers/bins/vgpud`，复刻 vblk 的 MMIO/negotiate/queue/DMA 模式；配置区读沿用对齐 `u32` 再拆 LE 的 HVF 规则。建立 1080×1920 XRGB8888 scanout resource，attach client VMO，按 damage TRANSFER+FLUSH；打印 `VGPU ok w= h=`。设备缺席静默跳过，既有标记零漂。
4. **Phase 3 present wiring**：把 guifbd 拆 host-testable lib + backend trait；ramfb 保持现行为，vgpu 后端承接 present/HDI。优先拓扑 = vgpu 模式由 vgpud 持 present/hdi 端点，guifbd 保留 console/FBSERVE，避免 `CHAN_HND_MAX` 抬帽；只有该拓扑被证明不可行才另裁 8→9。激活 PREPARE slot/fence 与 COMMIT 顺序。验证 = GUI8 normal/dual、smoke-image 146/146、soft↔vgpu 像素共识、稳态零唤醒与 HVF1 延迟表复测。
5. **Phase 4 berry-gfx backendization**：让 `Caps::Device` 持可选 vgpu 会话，VMO-backed texture slot 走同一 submit 语义；扩展 soft↔vgpu consensus golden。soft 后端仍是永久 CI 基线。
6. **Phase 5 Mac shell composition**：把 vgpu damage 喂给 berry 设备壳 shm-ring/Metal 呈现层；Mac Metal 只做 host 缩放/边框/光标叠层，不承担客机 UI 合成真相。

## 未完成事项

- [ ] Phase 0：AAVMF/Cocoa/槽位/吞吐 spike 尚未执行。
- [ ] Phase 1：virtio-gpu codec 与测试尚未实现。
- [ ] Phase 2：`vgpud` 与 svcmgr 装配尚未实现。
- [ ] Phase 3：guifbd backend 化、present/HDI 激活与全量回归尚未实现。
- [ ] Phase 4：berry-gfx 双后端像素共识尚未实现。
- [ ] Phase 5：Mac 设备壳 Metal/shm-ring 组合尚未实现。
- [ ] DEFERRED: Mac 侧 DisplayList 光栅器 / 未驱动 `apple-gfx-pci` — 原因 = 前者需要第二套永久渲染器且违反客机像素唯一真值，后者无 EDK2 GOP；归属 = 本计划明确不做，除非 virtio-gpu 数据证明 guest 光栅仍是主瓶颈后再立新 ADR。

## Related

- [BerryMacEmulator-HVF1-v1.0.md](../../docs/BerryMacEmulator-HVF1-v1.0.md)
- [架构决策.md](../../docs/架构决策.md) ADR #50/#51/#53/#61/#90
- [GUI渲染调研-2026-10-v1.0.md](../../docs/GUI渲染调研-2026-10-v1.0.md)
- [berry设备壳设计草案-v1.0.md](../../docs/berry设备壳设计草案-v1.0.md)
