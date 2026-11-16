# VNC 第九波：网页桌面产品化——自有壳页/自动重连/移动端/--open

Status: implemented

## Problem

VNC 前八波收官后，`berry vnc web` 打印的默认页是未改造的 vendored noVNC `vnc.html`：无品牌、无断线重连、无移动端适配，用户体验止步于「能连上」。同时 `vnc web` 打印 URL 后需要用户手工复制进浏览器，QR 码方案在 loopback-only 架构下的价值存疑，需要一并裁决。

## Decision

- **自有壳页替换默认页**：web 根新增三件本仓 Apache-2.0 资产（`index.html`/`berry.js`/`berry.css`），仅 import vendored `novnc/core/` 三模块（rfb/logging/keyboard），零 CDN、零新 Rust 依赖、不复制任何 noVNC app 层文本；状态栏（品牌/会话名/连接态含加密标注/分辨率）+ 工具栏五键（全屏/缩放三态/只读/软键盘/断开-重连）+ 指数退避自动重连（1s 起帽 30s、秒级倒计时、口令错终止转错误态）+ 唤醒锁 + 深色主题 + ≤700px 移动紧凑档（matchMedia 驱动 body 类，headless 可断言）。
- **双树路由**：`web.rs` 从单树 `root/novnc/<rel>` 改双树——`/` 与自有资产解析 web 根，`/novnc/<rel>` 原样保留为逃生门（vendored 页零改动、永久保留）；`find_web_root` 双哨兵（`index.html` 与 `novnc/vnc.html` 皆须在）；裸 `/vnc.html` 旧 URL 退役为 404。
- **口令纪律**：`#password=` fragment 读后即 `history.replaceState` 抹掉，仅在内存与 `sendCredentials` 调用中流动；`--open` 的 `open` argv 也剥 fragment（口令不进一次性 argv，与既有「口令不进 host 子进程 argv」口径一致）。
- **QR 砍项**：web 服务端与 QEMU ws 均 127.0.0.1 绑定，手机扫码打开的是手机自己的 loopback——功能死路。重启条件：LAN 绑定模式立项时随批评估，届时 QR 内容必须用剥掉 `#password=` 的 URL。
- **Chrome headless 虚拟时间下的 WebSocket 消息悬挂**（实现期最大坑，netlog+二分定位）：在 `--virtual-time-budget` 下，升级请求能完成 101，但渲染器不泵后续 ws 消息——我方页面仅收 335B（RFB banner 级）而 vendored 页同窗口收 382KB（整帧缓冲），页面永驻「连接中」；机制是虚拟时钟被 pending 网络挂住时才推进任务队列。二分归因：vendored 页的 `<audio preload="metadata">` 在拨号窗口留有 pending 媒体请求（其字体懒加载亦有同效；图片预载与空 CSS/fetch 接地均无效）。修法：壳页内建 `<audio preload="metadata">`——以自产铃音（`berry-bell.wav`，RFB `bell` 事件发声）为正当产品特性复得同机理，真浏览器零感知；另加 8s 看门狗（纯公有面 `disconnect()`）拆真实死挂连接交重连循环。附带钉死的外事实：QEMU ws 端点只认根路径——`ws://host:port`（空路径）升级悬挂、`ws://host:port/` 秒连，新页对 `path=` 形态一律过 `new URL().href` 规范化补尾斜杠。

## Alternatives considered

- **QR 三案**（自写 ~400 行/vendored qrcode-generator 1.4.4/nayuki 剥类型）：全否——loopback 死路前提下任何实现都是负资产，砍项并登记重启条件。
- **改造 vendored noVNC app 层（`vnc.html`/`app/ui.js`）做产品化**：否——app 层是 MPL-2.0/BSD 文件，混改违 vendored 红线且上游升级即冲突；probe f1 证实 core 公开面（RFB 构造/事件/sendCredentials、GestureHandler 与 Keyboard 内建）足够自建壳页，只 import 不拷贝。
- **双页并行、默认页不动**（新页按参数或路径按需访问）：否——产品化的主路径就是默认 URL，不换默认等于没做；落「默认换自有页 + `/novnc/vnc.html` 直访逃生门」，两页参数语义同义互换（三形态 URL 双页均可消费）。
- **自有资产放 `web/berry/` 子目录或塞进 novnc/**：否——前者多一层路由与 import 改写，后者混入 vendored MPL 树违反红线；web 根直放。
- **`raw=1` 参数别名做逃生门**：否——直访 `/novnc/vnc.html` 已是零分叉的逃生门，少一条参数逻辑。
- **虚拟时间悬挂的诸修法**（均实测否决或次优）：首拨延迟 500ms（探针 15/15 过但真页无效——环境负载敏感的假信号）、并发 fetch 接地 2/10、CSS 动画拖帧（opacity 合成器动画）1/10、img 预载 0/5、`--timeout` 真实时间模式（Chrome 载完即 dump，拨号落在 dump 后）、vendored 样式表移植 3/10——终取 `<audio preload="metadata">` 接地（3/3+5/5+6/6 定型）。
- **看门狗窥探 core 内部 `_sock._websocket.readyState`**：否——违「RFB 用法严格按计划引的 core 公有 API 面」纪律；改用纯公有面（超时即 disconnect）。
- **bell 事件做可见 UI**：否——计划边界明示不做 bell 专用 UI；只落自产铃音（声音非 UI）。

## Consequences

- 新页三形态 URL 全消费（明文 `path=`、`--tls` 明文 run `host/port/encrypt=false`、TLS run `host/port/encrypt=true`），与逃生门参数同义互换；单测 100 绿（98 基线 + U44 双哨兵 + U45 strip_fragment，U41-U43 为既有测试的 `/?` 形态与双树改写）。
- `check-appdir.sh` 增两检（index.html 在场 + NOTICE `## 自有资产` 行），打包脚本零改动（整目录拷贝天然随行）。
- headless 不可测面如实降级：全屏真行为（权限拒）、真 OSK/IME 组合输入、真机触摸手势、<500px 像素级观感、重连中间态时序——以 URL 参数初值 + 状态类 + 按钮在位断言替代，不虚报验证层级。
- Chrome 虚拟时间的拨号推迟是环境怪癖而非产品缺陷：真浏览器不受影响；500ms 延迟与看门狗对真浏览器无害（loopback 拨号毫秒级完成）。

## Related

- 计划：[.agents/plans/2026-11-14-vnc-web-polish-wave9.md](../../plans/2026-11-14-vnc-web-polish-wave9.md)（含 probe f1-f5 外事实依据与偏差记录）。
- 实现：[tools/berry/web/](../../../tools/berry/web/)（三件自有资产 + NOTICE 自有资产节）、[web.rs](../../../tools/berry/src/web.rs)（双树路由）、commands.rs `vnc_web`（--open）。
- 前波同族：[noVNC 网页桌面首落（第二波）](2026-11-13-vnc-maturity-wave2.md)、[彻底完善（第五波 TLS/代理）](2026-11-13-vnc-complete-wave5.md)、[合规收尾（第六波）](2026-11-14-vnc-compliance-wave6.md)；CLI-first §8/§14 v1.6 段为现行标准面。
