# berry VNC 第三波（分发与打包收尾：三平台 web 资产/vnc web 连接上限）

Status: implemented

## Problem

第二波（[2026-11-13-vnc-maturity-wave2](2026-11-13-vnc-maturity-wave2.md)）落了 `berry vnc web` 网页桌面，但资产解析链是开发态优先：`web_roots()` 里 `.app` 形态靠 exe 父目录 `Resources/web` 命中，而打包脚本一个都不拷 web 资产——probe 实测七个打包脚本无一拷贝，`.app` 内 `Contents/Resources/web/novnc/vnc.html` MISSING，Finder 双击（cwd=/）形态下 `berry vnc web` 必然 E_DEVICE。同时 `serve()` accept 循环无计数无上限（每连接一线程，第二波 audit 已记为限制）；windows 两脚本幂等有残（assemble.sh 的 `zip -qr` 追加进既有 zip 不删旧条目、build.ps1 的 `New-Item -Force` 不清理陈旧文件）。

## Decision

- 三平台资产落点：macOS `.app` `Contents/Resources/web/`（命中 `web_roots()` 既有根②）；Linux AppDir `usr/share/berry/web/`——与 AppDir 既有 `usr/share/berryos/`（客机镜像/固件）分层，`berry` 名下放宿主壳自有资源，并为此在 `web.rs` 增一条 exe 父目录 `share/berry/web` 解析根（AppDir `usr/bin/berry` 命中；对 `.app`/Windows 平铺是无害 miss）；Windows 目录包 `<包根>/web/` 与 berry.exe 平铺（命中既有根①）。tarball/AppImage 随 AppDir 整体打包自动继承；docker 构建复用上述脚本同样继承。
- 幂等语义：macOS/Linux 脚本 `rm -rf` 目标后 `cp -R`（与既有整树重建同构）；assemble.sh 改 zip 前删旧 zip，build.ps1 改整目录 `Remove-Item -Recurse -Force` 重建并加 TargetDir=仓库根拒绝护栏；check-appdir.sh 增 web 结构 FAIL 检（vnc.html + NOTICE + 两级许可文件）连带把关 tarball 前置。
- 许可随包：`NOTICE.md` 与上游 `novnc/LICENSE.txt`、`AUTHORS`、`vendor/pako/LICENSE` 随 novnc 全目录完整进每种分发形态，包内路径写进各打包脚本拷贝步骤注释与 README。
- 连接上限 `--max-conns N`：默认 16、值域 1..=1024（clap `value_parser`，非法退出码 2）；准入 = `Arc<AtomicUsize>` 计数 `fetch_add` 超限回退 + RAII `ConnSlot` Drop 归还（返回/错误/panic unwind 均回收）；超限回 `HTTP/1.1 503` + 恰一条 stderr，服务不崩继续 accept；绑定保持仅 127.0.0.1；单测钉纯函数层（解析/准入判定/503 渲染三组），服务级行为（三并发 503/名额回收）由 verify 打包态端到端实证。

## Alternatives considered

- AppDir 资产落点 `usr/share/berry/web` + 改 `web.rs` 增解析根 vs 塞进 `usr/bin/web` 命中既有根① vs 用既有 `usr/share/berryos/`：bin 目录放静态资产违背 FHS 惯例；`berryos/` 名下是客机件，宿主壳资源混入破坏目录归属语义；增解析根是一条对其他形态无害 miss 的零冲突候选。选 `share/berry/web` + 增根。
- 幂等用 `rm -rf` + `cp -R` vs `rsync --delete`：脚本既有形态就是整树 `rm -rf` 重建，rsync 是新外部依赖（Windows 侧更没有），对「整目录都是本刀产物」的场景无增量收益；`rm -rf` + 路径护栏与既有脚本体系同构。选 `rm -rf`。
- 上限默认值 16 vs 8 vs 只给旗标不给默认上限：无默认上限等于维持现状（每连接一线程无闸）；8 过紧——一个 noVNC 页可能伴随重连/预取等多请求，verify 三并发探针也要余量；16 对「个人开发机环回页面服务」足够宽裕又能在失控循环（脚本反复开页不关）时兜住线程数，1024 上界防误输入天文数字。选 16/1024。
- verify 主戏用 Desktop 变体（`BERRY_IMAGE=portrait + BERRY_BUNDLE_ID=dev.berryos.shell.desktop`）vs 默认 `BerryOS.app`：要证的是「双击态零仓库依赖 + 浏览器看见真实桌面」，desktop 档配数据盘才有 GUI 桌面与可对照第二波 w1 的调色板（123 采样色同构）；console 档首帧是文本面，像素多样性弱。选 Desktop 变体，产物独立落 `out/BerryOS-Desktop.app` 不动既有 `BerryOS.app`。

## Consequences

- 打包态 `berry vnc web` 直接可用：`.app` 二进制在 `/tmp` cwd 起服务，JSON 自报资产根 = `.app` `Contents/Resources/web`，仓库 `tools/berry/web` 临时改名后页面仍 200 且字节一致（三重实证排除仓库命中）；Chrome headless 截图与第二波桌面调色板同构（`out/vnc-w3-packaged-browser.png`）。
- `--max-conns` 行为闭环：超限 503、恰一条 stderr、释放后名额回收、服务不崩；JSON `vnc.web` 增 `max_conns` 字段；单测 55→58。
- check-appdir.sh 收紧后，不含 web 资产的旧 AppDir 结构检查不过——与新 build-appdir.sh 恒拷 web 一致，属预期。
- Windows/Linux 实机构建与冒烟仍未做（docker daemon 未运行；仅 `sh -n`/build.ps1 可读性检查），留分发验证刀。
- 环境级观察随刀入档（README 已注记）：多会话共享 `~/.berry` 时无 `--run` 的 device 回收命中「最新 running」（本刀 host run 两次被并行会话顺带停掉）；Chrome headless 虚拟时间下 noVNC `resize=scale` 画布不提交。
- 沿革撞号治理副产品：本刀第一/二波节被并行提交裹挟入库后仍按口径移文末重排——「二百五十一」已被并行已提交节占用，顺移为二百五十二/二百五十三，第三波新节二百五十四；并行会话内容一字不动。

## Related

- 计划（契约/偏差/验收记录）：[2026-11-13-vnc-packaging-wave3](../../plans/2026-11-13-vnc-packaging-wave3.md)
- 第一波笔记：[2026-11-13-vnc-desktop-suite](2026-11-13-vnc-desktop-suite.md)；第二波笔记：[2026-11-13-vnc-maturity-wave2](2026-11-13-vnc-maturity-wave2.md)（其「.app 打包拷贝留打包刀」由本刀收口）
- 设计（§8/§12/§14，v1.3）：[Berry设备平台-CLI-first设计-v1.0](../../../docs/Berry设备平台-CLI-first设计-v1.8.md)
- vendored 资产声明：[tools/berry/web/NOTICE.md](../../../tools/berry/web/NOTICE.md)
- 代码：`tools/berry/src/web.rs`（max_conns 纯函数 + `ConnSlot` + `web_roots` 新根）、`commands.rs`/`main.rs`（布线）、`packaging/{macos,linux,windows}`（资产拷贝与幂等）
- 实证产物：`out/BerryOS-Desktop.app`、`out/vnc-w3-packaged-browser.png`
- 沿革：[阶段沿革](../../../docs/阶段沿革.md)二百五十四节
