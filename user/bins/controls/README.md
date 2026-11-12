# CONTROLS

BerryUI Rich Controls Demo。它是普通 Berry Room GUI app：桌面图标点击后经
真实 `APP_SPAWN` 启动，创建 WM 窗口、提交 DisplayList frame、消费指针输入，
最后销毁退出。

页面包含：

- Berry Gallery
- Buttons & States
- Text Inputs
- Lists & Panes
- Theme & Density

本 app 只依赖 `berry-gfx`、`berry-room` 和 `berry-ui(guest-font-vmo)`。它不
解析 WM RPC、不映射 VMO、不计算 stride、不写像素，也不使用 surface escape。
