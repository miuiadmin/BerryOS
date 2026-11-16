# VNC 第三波实施计划（分发与打包收尾：三平台 web 资产/vnc web 连接上限）

Type: plan
Status: done
Updated: 2026-11-13

一句话说明：让 VNC 成熟面跟着「双击即用的发行物」走——三平台打包脚本把 vendored noVNC web 资产连同许可文件拷进对应资源位（macOS .app Resources、Linux AppDir share、Windows 包根），web_roots 解析链对齐 Linux FHS 落点，`berry vnc web` 加 `--max-conns` 连接上限（默认 16、超限 503），macOS 打包实跑验收，Linux/Windows 脚本做语法级自检。

## 背景

第二波（[2026-11-13-vnc-maturity-wave2.md](2026-11-13-vnc-maturity-wave2.md)，Status: done）落了 `berry vnc web`：std-only 静态服务 vendored noVNC 1.7.0（`tools/berry/web/`，约 1.5 MB），浏览器经 `path=` 参数直连 QEMU ws 端口。资产解析 `web_roots()` 五路：①exe 同目录 `web` ②exe 父目录 `Resources/web`（.app 先例）③`~/Library/Application Support/BerryOS/web` ④`%LOCALAPPDATA%/BerryOS/web` ⑤cwd 逐级向上的 `tools/berry/web`。开发态靠 ⑤ 命中仓库 checkout；第二波 audit 已知限制：serve() 每连接一线程、无连接数上限、仅绑 127.0.0.1。

probe（2026-11-13，f1–f6，脚本与产物在 `/tmp/vncprobe3/`，会话临时不入仓）实证三个缺口：其一，七个打包脚本（macos/build.sh、linux 四脚本、windows 三脚本）无一拷 web 资产——`.app` 的 `Contents/Resources/web/novnc/vnc.html` 实测 MISSING，Finder 双击（cwd=/）形态下 `berry vnc web` 必然 E_DEVICE；其二，web.rs serve() 的 accept 循环无计数无上限，现有 7 个单测无一覆盖 serve() 本身；其三，windows/assemble.sh 的 `zip -qr` 追加进既有 zip 不删旧条目、build.ps1 `New-Item -Force` 不清理陈旧文件，重复构建可能残留上一轮已不存在的条目。docker daemon 未运行（AppImage/Windows 交叉实构建本刀不可跑）；rustup 已装 aarch64-unknown-linux-musl，但按本阶段任务口径 Linux/Windows 仅做语法自检、实机构建不做。

## 契约

### 三平台打包资产落点

| 分发形态 | 资产落点（NOTICE.md + novnc/ 全目录） | web_roots 命中根 |
|---|---|---|
| macOS .app | `Contents/Resources/web/` | ② exe 父目录 `Resources/web`（既有） |
| Linux AppDir（tarball/AppImage 随 AppDir 整体打包自动继承） | `usr/share/berry/web/` | 新增根：exe 父目录 `share/berry/web` |
| Windows 目录包 | `<包根>/web/`（与 berry.exe 同层平铺） | ① exe 同目录 `web`（既有） |

落点裁决：功能规格点名「AppDir 的 share/berry/web 或等价」；采纳 `usr/share/berry/web`——与 AppDir 既有 `usr/share/berryos/`（客机镜像/固件）分层，`berry` 名下放宿主壳自有资源。该落点与 web_roots 现有五根不匹配，按「两头对齐、二选一改、最小改动」口径选改 web.rs：在 ② 之后增一条 exe 父目录 `share/berry/web`（AppDir 的 usr/bin/berry 解析为 `usr/share/berry/web`；对 .app 与 Windows 平铺两形态是无害 miss，各自命中既有根）。

许可随包（硬约束）：`NOTICE.md`、`novnc/LICENSE.txt`、`novnc/AUTHORS`、`novnc/vendor/pako/LICENSE` 四文件随 novnc 全目录完整进入每种分发形态；各打包脚本的拷贝步骤上方注释逐一列明其在分发形态内的路径（README 面向用户的说明归 docs 阶段）。

幂等语义：macOS/Linux 脚本本就整体 `rm -rf` 目标后重建，web 子目录再单独 `rm -rf` 后 `cp -R`（重复构建零残留）；windows/assemble.sh 在 zip 前先删旧 zip（修追加残留），build.ps1 改为整目录 `Remove-Item -Recurse -Force` 后重建（修 `New-Item -Force` 不清残），并加 TargetDir 等于仓库根的拒绝护栏。

不动的脚本：build-tarball.sh（`tar -C "$APP"` 整体打包，自动继承）、build-linux-binary.sh 与 build-cross-docker.sh（只产二进制，无分发树）、build-appimage-docker.sh 与 smoke 脚本（只消费 AppDir/产物）。check-appdir.sh 增 web 资产结构检查（vnc.html + NOTICE + 两级许可文件 + `usr/share/berry/web` 目录），使 build-tarball.sh 的前置校验连带把关。

### `berry vnc web --max-conns`

| 项 | 契约 |
|---|---|
| 旗标 | `--max-conns N`，值域 1..=1024，默认 16（`web::DEFAULT_MAX_CONNS`） |
| 非法值 | clap `value_parser`（`web::parse_max_conns`）拒绝，退出码 2（clap 用法错误语义） |
| 超限行为 | 回 `HTTP/1.1 503 Service Unavailable` + 一条 stderr（`berry: vnc web: connection limit N reached; refusing a client with 503`）；服务不崩、继续 accept |
| 准入实现 | `Arc<AtomicUsize>` 活连接计数，`fetch_add` 后判定超限则回退并拒绝；被接纳连接由 RAII `ConnSlot` 在 Drop 时递减（返回、错误、panic unwind 均归还名额） |
| 绑定 | 保持仅 127.0.0.1（不动） |
| 可观测 | JSON `vnc.web` 行增 `"max_conns"` 字段；human 输出 serving 行追加 `(max N connections)` |

### 验收口径内不改的东西

serve() 的 handle_conn 请求处理路径零改动；`--port/--run` 语义不变；vnc check/open/certs/passwd 不动；web 资产内容（vendored noVNC 1.7.0 子集）不动。

## 文件改动清单

| 文件 | 改动 |
|---|---|
| `tools/berry/src/web.rs` | 增 `DEFAULT_MAX_CONNS`/`MAX_CONNS_CEILING` 常量、`parse_max_conns`（纯函数）、`admit_connection`（纯函数）、`overload_response`（纯函数 503 渲染）、`ConnSlot`（RAII 递减）；`serve()` 增 `max_conns` 参数与准入闸；`web_roots()` 增 exe 父目录 `share/berry/web` 根并更新 doc 注释；单测新增解析/准入/503 渲染三组 |
| `tools/berry/src/commands.rs` | `vnc_web` 签名增 `max_conns: usize`，透传 serve()，JSON/human 输出带上限 |
| `tools/berry/src/main.rs` | `VncWebCmd` 增 `--max-conns`（value_parser + 默认值），dispatch 透传（仅 VNC 布线，不动并行会话 hunk） |
| `tools/berry/packaging/macos/build.sh` | Resources 拷贝段增 web 资产（rm -rf + cp -R + 存在校验 + 许可注释），尾 echo 增 web 行 |
| `tools/berry/packaging/linux/build-appdir.sh` | 增 `usr/share/berry/web` 拷贝段（die 校验 + rm -rf + cp -R + chmod a+rX），mkdir/chmod 列表与汇总行同步，头注释与 usage 增说明 |
| `tools/berry/packaging/linux/check-appdir.sh` | 增 web 资产结构检查（目录 + vnc.html + NOTICE.md + LICENSE.txt + AUTHORS + vendor/pako/LICENSE），usage 文案同步 |
| `tools/berry/packaging/windows/assemble.sh` | 增 web 拷贝段（校验 + rm -rf + cp -R + 许可注释）；zip 前删旧 zip、zip 路径按 OUT 推导，尾 echo 同步 |
| `tools/berry/packaging/windows/build.ps1` | 整目录重建（Remove-Item 护栏 + 重建）替代 New-Item -Force 追加；增 web 目录拷贝与许可注释 |
| `.agents/plans/2026-11-13-vnc-packaging-wave3.md` | 本计划（收口时 Status: done + 验收记录） |

## 测试矩阵

| 用例 | 方法 | 期望 |
|---|---|---|
| parse_max_conns 合法值 | web.rs 单测：1/16/1024 | Ok(值相等) |
| parse_max_conns 非法值 | web.rs 单测：0/1025/`-1`/`abc`/空串 | Err 且消息含值域 1 与 1024 |
| admit_connection 边界 | web.rs 单测：(15,16)/(16,16)/(0,1)/(1,1) | true/false/true/false |
| overload_response 渲染 | web.rs 单测 | 首行 `HTTP/1.1 503 Service Unavailable`、含 `Connection: close` 与上限值、Content-Length 与正文长度一致 |
| 既有测试零回归 | `cargo test --release`（tools/berry） | 既有 55 项全绿 |
| 三平台脚本语法 | `sh -n`（POSIX 脚本）+ build.ps1 可读性检查 | 零语法错误 |
| macOS 打包实跑 | `tools/berry/packaging/macos/build.sh` | exit 0；`Resources/web/` 树在位，四许可文件 + vnc.html 存在 |
| 重复构建幂等 | build.sh 连跑两次后比对 | 第二轮无旧残留、关键文件齐全 |
| `--max-conns` 非法 rc | 构建后 CLI 实跑 `--max-conns 0`、`--max-conns 2000` | 退出码 2 |
| `--max-conns` 合法值过闸 | CLI `berry vnc web --help` 或 dry 场景 | 旗标出现在帮助、默认 16 可覆盖 |

注：连接上限的服务级行为（真起 serve() 打满 16 连接观察 503）不在单测面（纪律「不起真服务」），由 verify 阶段打包态端到端顺带实证；单测钉在纯函数层（解析/准入判定/503 渲染）。

## 验收标准

1. `cd tools/berry && cargo test --release` 全绿：既有 55 项零回归 + 新增解析/准入/503 单测通过。
2. `just berry-build` 绿；若 docs-guard 因并行会话文档变红，先确认归属再绕过直接 cargo build 并记录（当前 check-docs 零告警，预期不触发）。
3. `out/BerryOS.app` 由改后脚本实跑产出：`Contents/Resources/web/` 含 NOTICE.md、novnc/vnc.html、novnc/LICENSE.txt、novnc/AUTHORS、novnc/vendor/pako/LICENSE；重复构建幂等；产物保留 out/。
4. CLI 实测：`--max-conns 0` 与 `--max-conns 2000` 退出码 2；`berry vnc web --help` 展示新旗标与默认值。
5. linux（build-appdir.sh、check-appdir.sh、build-tarball.sh、build-linux-binary.sh、build-appimage-docker.sh）与 windows（assemble.sh、build-cross-docker.sh）`sh -n` 通过；build.ps1 通过可读性检查；实机构建不做（记录）。

## 边界

1. 非目标：window.rs 解冻布线（仍冻结）；Windows/Linux 实机构建与实机冒烟（docker daemon 未运行；musl/AppDir/tarball 本机可跑但本阶段任务口径仅语法自检）；vnc check 的 TLS 客户端；noVNC 版本升级；连接数上限之外的 web 服务硬化（限速、来源审计等）。
2. 文档批量更新（README 打包节、CLI-first 设计 v1.3、沿革二百五十三节与撞号治理、Agent Note）归 docs 阶段，本阶段不动沿革。
3. 打包态浏览器端到端（Desktop 变体 + Chrome headless 截图落 out/）是 verify 阶段主戏，本阶段 macOS 实跑以默认 BerryOS.app 确认资产落位为限。
4. 并行会话冻结面（window.rs、profile.rs、main.rs 的 profile 默认值 hunk、arch/、kernel/、sched/ 等）零触碰；main.rs 只动 VNC 相关布线。

## 偏差

1. Linux 落点采纳 `usr/share/berry/web` 并改 web.rs 增解析根（功能规格「share/berry/web 或等价，以脚本现状为准」的两头对齐裁决：脚本侧维持 FHS 惯例，解析侧加一条与既有根无冲突的候选；不采纳把资产塞进 `usr/bin/web` 命中根 ① 的方案——bin 目录放资产违背 FHS 惯例）。
2. windows 两脚本的幂等修复（assemble.sh zip 前删旧包、build.ps1 整目录重建）超出「拷 web 资产」的最小面，属 probe issues 点名的「implement 一并修」，随本刀携带。
3. check-appdir.sh 新增 FAIL 级检查后，不含 web 资产的旧 AppDir 结构检查不过——与新 build-appdir.sh 恒拷 web 的行为一致，属预期收紧。
4. macOS build.sh 的 `rm -rf "$APP"` 路径安全护栏（对齐 linux 护栏体系）probe 标为可选加固项，本刀不做（非硬冲突，留待后续统一治理）。
5. 实现后如遇进一步事实（如 clap 对 value_parser 的行为差异、cp -R 的符号链接语义），按最小改动就地裁决并补记本节。
6. build.ps1 语法自检降级为可读性检查：宿主无 PowerShell/pwsh，只验证了 ASCII 纯文本、无 CR、花括号配平（16/16）与路径引用完整性；真正的 parse 级校验留待有 Windows/PowerShell 环境的分发验证刀。
7. 幂等实证口径：以两次 build.sh 产物的文件清单 diff 为准（134 文件全同）；未做逐字节比对（二进制/镜像每次拷贝字节恒定，由 cp 语义保证，清单级已覆盖「旧文件残留」这一幂等语义的关切点）。
8. 沿革撞号治理节号顺移（docs 阶段落笔时新事实）：任务口径给二百五十一/二百五十二/二百五十三，但「二百五十一」已被并行会话已提交节（6da67f09「显示加速 B1-a」）占用——按「保持时序 + 节号唯一 + 已提交内容一字不动」顺移为二百五十二/二百五十三，第三波新节为二百五十四。另：本刀两节此前已被并行提交（48848bb1/5af8d387）裹挟入库，仍按口径整节移动（只动自己两节，正文与 HEAD 逐字节比对仅节号行变化）。

## 未完成事项

1. Linux musl/AppDir/tarball 本地三连与 Windows/AppImage docker 构建：本刀仅语法自检，构建验证留待后续（DEFERRED：原因 = docker daemon 未运行且任务口径限语法自检；归属 = 后续分发验证刀；触发条件 = docker daemon 可用或目标平台实机到位）。docs 阶段与 verify 阶段均已收口，证据见「验收记录（docs 阶段）」「验收记录（verify 阶段）」。

## 验收记录（2026-11-13，implement 阶段）

1. 单测：`cd tools/berry && PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/cargo-target" cargo test --release --target aarch64-apple-darwin` → `58 passed; 0 failed`（既有 55 零回归 + 新增 max_conns_parsing/admission_boundaries/overload_response_rendering 三项）。
2. 构建：`just berry-build` → docs_guard PASS（0 警），cargo release 构建成功；无需绕过。
3. CLI 旗标：`berry vnc web --help` 展示 `--max-conns <MAX_CONNS>  Max concurrent page connections (1..=1024, default 16) [default: 16]`；`--max-conns 0/2000/abc` 退出码均 2，错误文本 `must be between 1 and 1024`；`--max-conns 16` 合法过闸（后续在 run 查找处以 rc=3 失败，属预期顺序）。
4. macOS 打包：`sh tools/berry/packaging/macos/build.sh` exit 0（约 1s）；`out/BerryOS.app/Contents/Resources/web/` 128 文件 1.5M（NOTICE.md + novnc/），五个必查文件（NOTICE.md、novnc/vnc.html、novnc/LICENSE.txt、novnc/AUTHORS、novnc/vendor/pako/LICENSE）全在位；NOTICE.md 与源 sha256 一致；连跑两次产物文件清单 diff 全同（134 文件）——幂等坐实；产物保留 out/。
5. web_roots 几何证明：.app 二进制 exe=Contents/MacOS/berry → 新解析链下 root ② 恰为 `Contents/Resources/web`，`novnc/vnc.html is_file = True`（服务级 URL 实测归 verify 阶段端到端）。
6. 脚本语法：八个 POSIX 脚本（macos/build.sh、linux 五脚本、windows/assemble.sh、build-cross-docker.sh）`sh -n` 全过；build.ps1 可读性检查过（偏差 6 口径）。
7. 治理面：docs/阶段沿革.md、tools/berry/README.md、CLI-first 设计文档本阶段零改动（归 docs 阶段）；冻结面零触碰（main.rs 仅动 VncWebCmd 定义与 VncAction::Web dispatch 两处 VNC 布线）。

## 验收记录（2026-11-13，verify 阶段）

以下为打包态端到端与全量回归实录（v1–v5），一段一物证；本批 run 均自起自收（stop+cleanup 完成），证据产物保留 `out/`。

v1 打包 Desktop 变体：`BERRY_IMAGE=out/berryos-portrait.img BERRY_BUNDLE_ID=dev.berryos.shell.desktop tools/berry/packaging/macos/build.sh out/BerryOS-Desktop.app` exit 0（`packaged:` + `web:` 两行回显）；五个必查文件全在位——`Contents/Resources/web/NOTICE.md`（2424B）、`novnc/vnc.html`（17810B）、`novnc/LICENSE.txt`（2163B）、`novnc/AUTHORS`（358B）、`novnc/vendor/pako/LICENSE`（1084B），`Resources/web` 共 128 文件 1.5M；`Contents/MacOS/berry` 1808624B 在位且功能等价（`--version` 0.1.0、`vnc web --help` 含 `--max-conns`——与仓库 CLI 字节差异仅 codesign 重签名 blob）；`cmp` 证 `Resources/berryos.img` == `out/berryos-portrait.img`；Info.plist `CFBundleIdentifier=dev.berryos.shell.desktop`、版本 0.1.0，codesign adhoc Identifier 同步。

v2 打包态起机（零仓库依赖）：数据盘 `python3 tools/mkfat.py /tmp/vncw3/run/gui-data.img` + `BERRYOSVBLK!` dd 签名后，在 `/tmp/vncw3/run` 以 `.app` 二进制 `BERRY_DATA_DISK=… device start --profile desktop --image <.app>/Contents/Resources/berryos.img --memory 256 --vnc on --vnc-ws on --keep` 起机（run `1794548537-229251000-42952-host`）；QEMU argv 实录双 `-vnc`（`127.0.0.1:64222,websocket=64223` + `unix:…/vnc.sock`）+ `-fw_cfg name=opt/berry/display,string=1080x1920@60` + 数据盘经 virtio-blk `drive=dd` 挂 `/tmp` 盘；`berry vnc status` 报 `vnc: 127.0.0.1:64222 (auth none)` + `ws: ws://127.0.0.1:64223`；serial 至 `WMFOCUSREADY ok left=1 right=2`；VNC 帧最终 painted（1080x1920、19315 采样色，白 + 蓝 (0,162,255) 家族）。

v3 打包态网页桌面（资产来自 .app Resources 三重实证）：其一，`.app` 二进制在 `/tmp` cwd 起 `vnc web`，JSON 自报 `"root":"…/out/BerryOS-Desktop.app/Contents/Resources/web"`（服务端解析根）；其二，curl `/vnc.html` 200（17810B）与 Resources 副本及仓库存量三方 sha256 同一（`df7dbd37…`），URL 空间 = `root/novnc`（`/vendor/pako/LICENSE`、`/LICENSE.txt`、`/AUTHORS` 均 200）；其三（最硬），把仓库 `tools/berry/web` 临时改名 `web.w3hidden` 后 URL 仍 200 且字节一致（trap 保证改回、零残留）——排除 cwd 第⑤路命中仓库的可能。连接性：Chrome `--dump-dom` 状态栏「已连接（未加密）到QEMU」，events.jsonl 各 ws 客户端全生命周期（connected/initialized/disconnected）落档。像素实证：终局 run 5 Chrome headless 截图 `out/vnc-w3-packaged-browser.png`（480x1000、18411B、123 采样色，主色白 27741 + BerryOS 蓝 (0,116,224) 1755 + (27,37,64) 190）与第二波 w1 桌面调色板同构 = 打包态浏览器渲染真实 GUI 桌面。过程实测：Chrome headless 虚拟时间下 noVNC `resize=scale` 画布确定性不提交（6+ 次空白重试），去 `resize` 参数后画布正常；桌面需抽屉交互（上滑 + 图标位 tap）方 `LAUNCH ok`/`RENDER ok ×12`。

v4 连接上限：`--max-conns 2` 起服务（JSON `"max_conns":2`）后 python 三并发——第 3 连接收 `HTTP/1.1 503 Service Unavailable`，前两连接正常 200（`/vnc.html` 17810B 正文、`/LICENSE.txt`），关闭释放后新连接再 200（名额回收），服务全程不崩；stderr 恰一行 `berry: vnc web: connection limit 2 reached; refusing a client with 503`；非法值 `.app` 二进制实测 `--max-conns 0`/`abc`/`2000` 退出码均 2。

v5 全量回归：`cd tools/berry && cargo test --release` 58 passed 0 failed；`berry doctor` `ok:true`；`just berry-smoke` 全绿（docs_guard PASS 0 警、marker `BerryOS boot` ok、`out/berry-shot.png` 800x600）；第一二波快检——无口令档 `vnc check` rc=0（1080x1920）、口令档（`--vnc-pass w3rightpw`）错口令 python DES 探针（位反 key + openssl des-ecb）`SecurityResult: 1` 拒绝、对口令 `vnc check` `ok:true`（480x1000 auth=vnc）、仓库态 `vnc web` root 解析到仓库 `tools/berry/web`（cwd 第⑤路）且页面 200；自清——本批 6 个 run 全部 stop+cleanup（含一次误入隔离 home 的 shell run）、`vnc web` 服务进程 killed、Chrome headless 零残留、`/tmp/vncw3` 删除、并行会话 run 与 QEMU 未动；打包产物保留 `out/`（新建 `BerryOS-Desktop.app` 179M + `vnc-w3-packaged-browser.png`，`BerryOS.app` 等既有产物不动）。（fix 补记 2026-11-13：本批实有第 7 个 run `1794554323-487843000-35554-host`——15:18 因 `/tmp/vncw3/run/gui-data.img` 写锁 qemu 起败 state=failed，未入上述自清批；当日下午 sweep 后 fix 阶段以 `berry device stop --run` + `cleanup --run` 显式补清（目录连同 vnc.pass/ram.bin 256M/vars.fd 64M 已删），另清 `~/.berry/tmp/vncpass-68812-…` 一次性交接残留——见 [../research/2026-11-13-vnc三波遗漏检查.md](../research/2026-11-13-vnc三波遗漏检查.md) F13/F14。）

verify 阶段发现（均非本刀改动面缺陷，记录备查）：其一，并行会话共享 `~/.berry` 时，其无 `--run` 的 device 回收周期会命中「最新 running」= 本刀 host run（run 1 于 13:50:11、run 2 于 13:53:48 两次被顺带停掉，各在 15-18s 后出现并行新 shell run）；规避 = 后续 run 以 `BERRY_HOME` 隔离（`/tmp` 专用 home），多会话同机时建议 docs 阶段在 README 注记显式 `--run`/隔离 home。其二，QEMU 双 `-vnc`（ext TCP + unix）实例的 dirty 刷新可分叉：高负载下 unix 实例长期黑帧而 ws 实例与客机 RAM（ram.bin 直读 0x40800000 实测 4367 色）已画，个别 run 的 QMP accept 亦被拒（主循环受挤），重起低负载后正常。其三，`berry vnc web` 默认 URL 带 `resize=scale`，Chrome headless `--virtual-time-budget` 下该模式画布不提交，自动化截图需去 `resize` 参数（docs 阶段 README Chrome 指引可注记）。

## 验收记录（2026-11-13，docs 阶段）

1. 沿革撞号治理：本刀两节（原 5821「二百四十八：VNC 桌面全套」、原 5854「二百五十：VNC 成熟化第二波」）整节移至文末，节号改二百五十二/二百五十三（「二百五十一」已被并行会话已提交节 6da67f09 占用，按节号唯一与保持时序顺移，见偏差 8）；正文与 HEAD 逐字节比对仅节号行变化，并行会话内容（含落笔期间并行新落的「九十一轮」「f7b5970a」内容）一字未动；第三波新节二百五十四（「VNC 第三波——分发与打包收尾」）接其后，五段式（刀面/宿主/客机零改动/实测/未落）。
2. 节号核验（全文件扫描 `^## ` 节头的二百系列）：治理后「二百四十八」「二百五十」「二百五十一」「二百五十二」「二百五十三」「二百五十四」各恰一次——本刀治理的两处撞号（二百四十八/二百五十）清零；二百系列其余十对重复（二百一十一/二百二十一/二百三十三/二百三十九/二百四十～二百四十四/二百四十九）在 HEAD 已提交内容中原样存在，归属并行会话与历史遗留，按「已提交内容一字不动」不修，仅记录归属。
3. tools/berry/README.md：双击包节补三平台 web 资产落点（.app `Contents/Resources/web/`、AppDir `usr/share/berry/web/`、Windows `<包根>/web/`）、双击态 vnc web 用法（零仓库依赖）与许可随包说明（NOTICE 与三上游许可文件的包内路径）；命令清单与新增「VNC 第三波」段补 `--max-conns`（默认 16/1..=1024/503）；Chrome headless 指引注记 `resize=scale` 虚拟时间画布坑；已知限制更新 Windows/Linux 实机未验条目（脚本已写、语法自检过、实机构建未做）并新增多会话共享 `~/.berry` 回收风险条目。
4. CLI-first 设计升 v1.3：版本行 + 【v1.3】注记行、§8.1 `vnc web [--run] [--port N] [--max-conns N]` 与 v1.3 段、§12 打包节许可随包一句、§14 刀序表「VNC 分发（已落，v1.3）」行；未重构，§15 等其余节未动。
5. Agent Note：`.agents/notes/implemented/2026-11-13-vnc-packaging-wave3.md` 新建（写前 rg 搜 `.agents/notes/` 含 rejected 零撞题），与前两波笔记互链；Alternatives 载四项真实裁决（AppDir 资产落点、rm -rf vs rsync、上限默认 16、.app e2e 用 Desktop 变体）。
6. `node .agents/tools/check-docs.mjs` 零告警（报告模式）；docs-guard 未因并行会话文档变红，无需归属裁定绕过。

## 验收记录（2026-11-13，audit 阶段）

1. 复审全过：八个 POSIX 脚本 `sh -n` 与 build.ps1 可读性检查（ASCII/无 CR/花括号 16:16）复跑零错；macOS build.sh 实测幂等（重建后 134 文件清单与重建前全同，投入 `STALE-SENTINEL.js`/`STALE-TOP.txt` 后再重建均被清除）；两个 `.app`（BerryOS.app/BerryOS-Desktop.app）五个必查资产文件在位、sha256 与源同一、128 文件数与源一致；check-appdir.sh 运行时双面演练（无 web 的旧 AppDir exit=1 报 FAIL、`/tmp` 副本补 web 后 33 ok 0 fail）；build-tarball.sh 整树 `tar -C` 与 build-appimage-docker.sh `cp -a /app` 消费 AppDir 均自动继承 web 资产——docker 复用关系属实；`cargo test --release` 58 绿、`just berry-build` 绿（docs_guard PASS 0 警）、check-docs 201 篇 0 告警复跑；沿革节号全文件复扫二百四十八～二百五十四各恰一次（十对既有重复全在 HEAD 已提交内容，归属并行/历史），并行已提交节与 `git show HEAD` 机器比对零改动、本刀两节正文逐字节一致且节内无 markdown 链接；c2 双态复测（隔离 `BERRY_HOME` 起 default 档 run：仓库态 JSON `root` = 仓库 `tools/berry/web`；`.app` 二进制在 `/tmp` cwd `root` = `Contents/Resources/web`，`/vnc.html` 与 `/LICENSE.txt` 均 200 且页面 sha256 与仓库同一）；c3 复验（`--max-conns 2` 三并发第 3 连接收 `HTTP/1.1 503`、stderr 恰一行、释放后新连接 200 名额回收、服务不崩）。run/进程自起自收（stop+cleanup、隔离 home 删除、18123/18124 端口释放），并行会话 QEMU 未触碰。
2. 就地修三处：其一，windows/assemble.sh 的幂等改法引入回归——`ZIP=$(dirname -- "$OUT")/…` 为相对路径而 zip 在 `cd "$OUT"` 之后执行，默认参数下 `/tmp` 模拟实测 `zip error: Could not create output file`（exit 15），改为绝对路径 `$(CDPATH= cd -- "$(dirname -- "$OUT")" && pwd -P)/BerryOS-win64.zip` 并复验成功；其二，web 资产权限归一——源 `tools/berry/web/NOTICE.md` 0600 被 `cp -R` 原样带入 `.app`（Linux 脚本本有 `chmod -R a+rX`，macOS/Windows 侧缺同款），macos/build.sh 与 assemble.sh 拷贝后补 `chmod -R a+rX`、源文件同步 chmod 644，重建后 `.app` 内 NOTICE.md 644 且文件清单不变（BerryOS-Desktop.app 为 verify 历史产物保留原貌，归一自下一次构建起生效）；其三，wave2 笔记沿革指针——`.agents/notes/implemented/2026-11-13-vnc-maturity-wave2.md` 的「沿革：…二百五十节」经本刀重排后指向错节，更新为「二百五十三节（VNC 第三波撞号治理自『二百五十』重排）」。
3. 记录不修：check-appdir.sh 既有 fail-fast 语义（`set -e` 下首个 `check_file` 失败即终止，其后检查不再输出）为 HEAD 既有行为，web 检查沿用同型非本刀引入；macos/build.sh `rm -rf "$APP"` 无路径护栏维持偏差 4 的顺延裁决；wave1/wave2 计划文档中「沿革二百四十八/二百五十节」字样为落笔当时的历史记述（当时为真），按「只写当前状态」不改历史记录，活性指针（implemented 笔记）已修。
