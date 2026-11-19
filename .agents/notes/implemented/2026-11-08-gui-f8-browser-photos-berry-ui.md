# GUI F8 Browser/Photos 迁移 BerryUI

Status: implemented — 2026-11-08

## Problem

F7 后，Browser 与 Photos 仍是最后两个手写 Native Shell：各自自建 WM/input session、映射窗口 VMO、计算 stride 并用 berry-nshfont/pixel loop 手绘。Photos 还在 app 内直接执行灰度像素循环；Browser 的返回键依赖固定区域。

## Decision

1. Browser 与 Photos 迁到 `berry-room + berry-ui`；WM/窗口/frame/input/destroy、字形与 present 归 Berry Room，页面结构、布局、主题状态和 Button 消息归 BerryUI。
2. Photos 的 PATTERN.BIN READDIR/STAT/GET 与 5000B 逐字节校验继续走 berry-fsclient；图像由 `IconSource + SmoothArgb` 进入 DisplayList，不由 app 写像素。
3. Browser 保留诚实本地壳边界：无网络、无 HTML、无 JavaScript，不伪装网页能力。
4. GUI8 解析 `BROWSERLAYOUT` / `PHOTOSLAYOUT` 的真实 Button bounds 并动态点击中心；截图锚改为 Berry Theme、系统 A8 与真实 PATTERN 灰度像素。
5. Room submit 与 Berry Launcher 按 guifbd RETURN-ALL 语义接受 `ReleaseWait count=0..=2` 并全量关闭随行别名；count/handle 失配仍 loud 拒。
6. GUI boot probe 的 child 回收等待帽调整为 240s：TCG 下真实 fs+光栅+present 可超过 30s，但退出真码 0 谓词不变，挂死仍 fail closed。
7. Contacts 与 Photos 的实际页面节点均小于 32，UI arena 收敛到 32 以降低交互/退出栈压，不改变页面语义。

## Alternatives considered

- 保留 Photos 手写像素循环、只换 WM：继续复制 UI 光栅路径，无法过 F8；否决。
- 给 Browser 增加网络/HTML 桩：违反诚实边界；否决。
- 让 BerryUI 组件持有图像读取或浏览器状态：破坏 app 业务状态边界；否决。
- 继续 strict count=1 ReleaseWait：把 guifbd 明确定义的 count=0 幂等空账当协议错误，Browser 间歇失败；否决。
- 继续用 30s 回收帽：在 TCG 下误杀仍在执行真实 FAT/光栅链的 GUI boot probe；否决。

## Consequences

- 六个 GUI app（Settings/Files/Contacts/Browser/Photos/UITEST）全部走 BerryUI 正常路径。
- Browser/Photos 源码无直接 berry-sys、berry-guiproto、berry-nshfont、WM RPC、VMO、stride、像素循环、alloc 或 unsafe。
- GUI8 normal 精确断言两 app 的 layout/render/frame/hit/back/data/view/destroy 链；Photos 保留 5000B PATTERN 数据与 SmoothArgb 图像证据。
- BROWSER.bap 31,804B→71,412B；PHOTOS.bap 27,768B→89,440B；rootfs 总量 33,780,848B，rootfs0 恰单卷帽，rootfs1 226,416B，总量仍在 128MiB 帽内。
- `berry-nshfont` 已无 GUI app 消费者，F9 可删除并加静态守卫。
- 规格见 [BerryBrowserPhotos-F8迁移-v1.0](../../../docs/BerryBrowserPhotos-F8迁移-v1.0.md)。
