# Berry Demo

`BERRYDEMO` 是 BerryOS 的系统级演示应用：同一入口同时展示 BerryUI 控件、
Berry Room 生命周期与应用开发标准。桌面图标点击后经真实 `APP_SPAWN`
启动，创建 WM 窗口、提交 DisplayList frame、消费指针输入，最后销毁退出。

页面包含：

- Berry Gallery
- Buttons & States
- Text Inputs
- Lists & Panes
- Theme & Density
- System & Standards

本 app 只依赖 `berry-gfx`、`berry-room` 和 `berry-ui(guest-font-vmo)`。它不
解析 WM RPC、不映射 VMO、不计算 stride、不写像素，也不使用 surface escape；
manifest/.bap/Berry Room/BerryUI/IconSource 就是应用开发标准演示本身。
