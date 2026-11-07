# Settings Native Shell

- 独立 no_std GUI 进程，manifest `wm = true`。
- 自建 WM/RPC 与 input session，全屏窗口 + 窗口局部 VMO + wmd composition。
- WLAN / Bluetooth 控件接收 wmd owner-local UP，切换本进程状态并重绘提交。
- 左上返回键销毁窗口、回读空表、退出进程。

**诚实边界：v1 的开关是进程内状态，不接网络/蓝牙驱动，也不持久化。**
