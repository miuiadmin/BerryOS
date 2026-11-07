# Contacts Native Shell

- 独立 no_std GUI 进程，manifest `wm=true` + `fs=["rootfs"]`。
- 真实 READDIR 根目录、STAT/GET `CONTACTS.TXT`，解析 4 条 `name,number` 记录。
- 列表 → 详情 → 收藏切换 → 返回列表 → 退出；每态真实重绘并提交新帧。
- 输入经 wmd owner-local 路由；返回销毁窗口并回读空表。

**诚实边界：v1 不提供联系人 provider、拨号、短信、编辑或持久化；收藏是进程内状态。**
