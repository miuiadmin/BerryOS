# UITEST host probe

- GUI F1 验收 app：只消费 `berry-room::AppRoom`。
- 不写像素、不解析 WM RPC、不直接映射 VMO、不计算 stride。
- 全屏 frame VMO 保持 commit 后零页并真实 SUBMIT；runtime 等待一次
  owner-local `PhaseUP` 后销毁窗口并退出。
