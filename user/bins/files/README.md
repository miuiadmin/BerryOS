# Files Native Shell

- 独立 no_std GUI 进程，manifest `wm=true` + `fs=["rootfs"]`。
- 自建 WM/RPC 与 input session，全屏窗口 + 窗口局部 VMO + wmd composition。
- 根目录列表经真实 fsmgr `READDIR`；点击文件走 `STAT` + `GET` 显示尺寸与内容预览。
- 详情页返回列表，列表页返回销毁窗口并退出进程。

**诚实边界：v1 只读浏览，无写、删除、搜索或权限管理。**
