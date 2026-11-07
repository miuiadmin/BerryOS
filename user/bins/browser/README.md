# Browser Native Shell

BerryOS 的第一个独立真实 GUI app 进程。

- manifest 能力：`wm = true`；APP_SPAWN 由 svcmgr fresh dup WMD broker 端点。
- 自建 WM/RPC 与 input 两根 session 通道，创建全屏窗口并提交窗口局部 VMO。
- boot 批以 `boot-probe` 参数完整走 WM → window → frame → destroy 后退出。
- 桌面点击为常驻 runtime 形；左上返回键经 wmd owner-local 路由后销毁窗口。

**诚实边界：v1 没有网络、HTML 解析或 JavaScript 引擎，不声称 web 能力。**
