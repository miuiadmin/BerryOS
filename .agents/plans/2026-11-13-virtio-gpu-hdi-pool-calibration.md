# virtio-gpu GPU2 HDI, backing pool, and calibration
Type: plan
Status: done
Updated: 2026-11-13

一句话说明：把 vgpud 从 present 传输后端补成完整 HDI 显示框架，并建立 soft↔vgpu 像素共识与 Mac 性能标定。

## 背景

GPU1 已让 1080×1920 产品桌面经真实 virtio-gpu present 后端运行，但 hdi 四 op、frame backing 身份、soft↔vgpu 共识与性能数据仍缺。用户追裁 Mac 模拟器必须从底层系统框架做到最好，不能靠降分辨率或表面补丁。

## 步骤

1. **已完成：HDI 四 op 真链**——REG_VBLANK / VBLANK_WAIT / PREPARE / COMMIT 全部接入；COMMIT 在拍点先 TRANSFER+FLUSH 再释放 fence。
2. **已完成：backing 池**——3 resource、first/last PA 指纹、LRU、frame VMO READ pin 与空 resource detach 拒面。
3. **已完成：诊断与共识产链**——`just gpu2-hdi-image`、`just gpu2-vgpu-image`、`tools/cpio_filter.py --add`、GUI8 `--vgpu`。
4. **已完成：动态几何**——vgpud 使用 display-info enabled mode，产品 1080×1920 与 GUI8 800×600 共用同一服务。
5. **已完成：soft↔vgpu 像素共识**——GUI8 `--vgpu` 挂真实 virtio-gpu、显式 `device=vgpu0` 截图，并执行既有像素/交互/落盘断言。
6. **已完成：Mac 双臂性能定标**——`tools/desktop-calib.py` + `just gpu2-desktop-calib` 输出 vgpu/ramfb 同动作延迟与 `VGPUPERF`。
7. **已完成：性能第一刀**——damage 按面积和/包络择优合并，控制队列先短自旋再 100µs 睡眠。
8. **已完成：契约单源**——`gp::HDI_USER_SIGNAL` 承载 vblank/fence 用户半位，guifbd/vgpud/svcmgr 共同导入。

## 结果

现行规格与验收细节见 [BerryMacEmulator-GPU2-v1.4.md](../../docs/BerryMacEmulator-GPU2-v1.4.md)。本计划完成于 GPU2 1.0 时点，保留为历史执行记录；后续 Mac Metal/shm-ring 呈现另开计划。

## 未完成事项

_（无）_
