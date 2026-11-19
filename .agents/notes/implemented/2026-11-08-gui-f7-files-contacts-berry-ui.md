# GUI F7 Files/Contacts 迁移 BerryUI

Status: implemented — 2026-11-08

## Problem

F6 后，Files 与 Contacts 仍自带 WM client、frame VMO 映射、stride 计算和 berry-nshfont 像素循环。它们还各自手绘列表、详情、收藏与文件预览，正是 GUI 地基路线要停止的迁移前模板。

## Decision

将两 app 迁到 Berry Room + BerryUI，同时把数据链保留在 app 授权能力层：

1. WM session、窗口、frame VMO、owner-local 输入、submit、系统字形与销毁归 `AppRoom`。
2. 页面结构、布局、主题状态与 Button 消息归 `berry_ui::sdk`；app 只持有业务状态。
3. Files 的 READDIR/STAT/GET 与 Contacts 的 CONTACTS.TXT 读取继续走 `berry-fsclient`；app 不直接调用 channel/syscall。
4. GUI8 对两 app 改为解析 `FILESLAYOUT` / `CONTACTSLAYOUT` 的真实 Button bounds，再动态点击中心；截图锚改为 Berry Theme 与系统 A8。
5. 并发 boot probe 暴露旧“WM 全表必须为空”destroy 断言错误；Berry Room 与迁移前 Photos 改为验证本 window_id 消失，允许其它 app 窗口并发存在。

## Alternatives considered

- 只替换 WM 层、保留手绘列表：会继续复制控件状态与像素路径，无法过 F7；否决。
- 让 BerryUI 组件持有文件/联系人数据回调：破坏 app 业务状态边界；否决。
- 为了固定 GUI8 坐标硬编码两 app 布局：违背 ThemeMetrics 单源；否决。
- 保留全局空表 destroy 断言：并发 app 生命周期下会把其它合法窗口误判为泄漏；否决。

## Consequences

- Files/Contacts 源码无直接 berry-sys、berry-guiproto、berry-nshfont、WM RPC、VMO、stride 或像素循环。
- Files 保留 HELLO.TXT READDIR/STAT/GET 28B 真链；Contacts 保留 CONTACTS.TXT 4 条记录读取、Berry Wang 详情、收藏状态与返回列表。
- GUI8 normal 精确断言两 app 的 mode layout、RENDER/FRAME、HIT、数据 marker 与销毁链。
- FILES.bap 27,912B→69,768B，CONTACTS.bap 31,308B→73,236B；rootfs 总量 33,679,824B，rootfs0 恰单卷帽，rootfs1 125,392B，总量仍在 128MiB 帽内。
- Browser/Photos 仍是迁移前基线；F8 迁移前不得继续扩张其手写 UI。
- 规格见 [BerryFilesContacts-F7迁移-v1.0](../../../docs/BerryFilesContacts-F7迁移-v1.0.md)。
