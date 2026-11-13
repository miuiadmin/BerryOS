# virtio-gpu GPU2 HDI, backing pool, and calibration
Type: plan
Status: active
Updated: 2026-11-13

一句话说明：把 vgpud 从 present 传输后端补成完整 HDI 显示框架，并建立 soft↔vgpu 像素共识与 Mac 性能标定。

## 背景

GPU1 已让 1080×1920 产品桌面经真实 virtio-gpu present 后端运行，但 hdi 四 op、frame backing 身份、soft↔vgpu 共识与性能数据仍缺。用户追裁 Mac 模拟器必须从底层系统框架做到最好，不能靠降分辨率或表面补丁。

## 步骤

1. **已完成：HDI 四 op 真链**——vgpud 支持 REG_VBLANK、VBLANK_WAIT、PREPARE、COMMIT；fence 在 vblank/target 拍释放；svcmgr `gpuhdi.on` 诊断镜像跑四 op 并断言 fence。
2. **已完成：backing 池**——两个 virtio-gpu resource、first/last PA 指纹、LRU 换背、frame VMO READ pin 防页复用、scanout resource 切换。
3. **已完成：诊断产链**——`cpio_filter --add`、`tools/gpuhdi.on`、`just gpu2-hdi-image` 产 `out/berryos-gpuhdi.img`；正式产品/验证镜像不含 marker。
4. **已完成：性能埋点**——每 128 次 SUBMIT 输出 `VGPUPERF submits avg_us reattaches jitter_us`。
5. **待做：COMMIT 呈现语义**——PREPARE VMO 入池，COMMIT 在拍点 TRANSFER+FLUSH+SET_SCANOUT 后放 fence。
6. **待做：soft↔vgpu 像素共识 CI**——同一 GUI8 像素锚在 ramfb 与 vgpu 双后端各自全过，再加双 PPM 锚点对拍。
7. **待做：Mac GPU/ramfb 性能标定**——同动作双臂计时、`VGPUPERF` 汇总、`wait_used` 轮询策略评估；绝对值记录文档，不进跨机器 CI 阈值。
8. **已完成：fence/vblank 信号位 guiproto 常量单源**——`gp::HDI_USER_SIGNAL`，guifbd/vgpud/svcmgr 共同导入并 host 钉板。

## 未完成事项

- [ ] COMMIT 目前是 fence/vblank 记账，尚未把 slot VMO 排队呈现。
- [ ] soft↔vgpu 机器像素共识 gate 未落 CI。
- [ ] Mac 双臂性能标定报告未产出。
- [ ] `VGPUPERF` 只有埋点，缺少正式 harness 与结论。
