# Photos Native Shell

- 独立 no_std GUI 进程，manifest `wm=true` + `fs=["rootfs"]`。
- 真实 READDIR 根目录、STAT/GET `PATTERN.BIN`（5000B，逐字节公式验证）。
- 将文件字节渲染为灰度照片：网格缩略图 → 查看大图 → 返回网格 → 退出。
- WM/input session、全屏窗口、局部 VMO、wmd composition、owner-local 输入全真链。

**诚实边界：v1 是只读样本照片浏览器，不支持相机、编辑、相册管理或云图。**
