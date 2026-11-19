# Vendored 第三方资产声明（tools/berry/web/）

## noVNC

- 上游: [noVNC](https://github.com/novnc/noVNC)，版本 v1.7.0（release 名 noVNC 1.7.0，2026-04-28 发布）。
- 源 tarball: <https://github.com/novnc/noVNC/archive/refs/tags/v1.7.0.tar.gz>（726728 字节）。
- sha256: `b1003a11b6e6e8d8f7f5e5586daae7f8ca651d8aee0aa155ff9ac841c48f52c6`。
- 入仓方式: implement 阶段（2026-11-13）重新下载并校验 sha256 通过后，解包取最小子集拷入 `novnc/`；probe 阶段留档副本（`/tmp/vncprobe2/minimal/`）与本次下载做过逐文件 `diff -rq` 零差异复核。
- 上游许可文件原样保留、未修改一字: `novnc/LICENSE.txt`、`novnc/AUTHORS`、`novnc/vendor/pako/LICENSE`（MIT）。

### 许可分层（见 novnc/LICENSE.txt）

- `novnc/core/**/*.js`、`novnc/app/*.js`: MPL-2.0。
- `novnc/vnc.html`、`novnc/app/styles/*.css`: BSD-2-Clause。
- Orbitron 字体条目: OFL-1.1。
- `novnc/app/images/`: CC-BY-SA-3.0。
- `novnc/vendor/pako/`: 上游自带 MIT LICENSE（`novnc/vendor/pako/LICENSE`）。

### 子集与剔除

入仓为 noVNC 官方发行的最小可服务子集（约 1.5 MB，58 个 JS，纯 ESM 无构建步骤，`vnc.html` 直连 QEMU 原生 websocket 端口）:

```text
novnc/
├── vnc.html            # 入口页
├── app/                # UI 层（js/styles/images/locale/sounds）
├── core/               # RFB 核心（含 crypto、input、util、decoders）
├── vendor/pako/        # zlib 依赖（core/inflator.js、deflator.js 硬依赖）
├── defaults.json
├── mandatory.json
├── LICENSE.txt         # 上游原样
└── AUTHORS             # 上游原样
```

剔除项（不影响 `vnc.html` 运行）: `po/`、`tests/`、`docs/`、`snap/`、`utils/`、`package.json`、`eslint.config.mjs`、`karma.conf.cjs`、`vnc_lite.html`、`README.md`。`app/ui.js` 对 `./package.json` 的版本号 fetch 有 catch 兜底（缺失时隐藏版本栏），不影响连接功能。

### 获取与校验命令

```bash
curl -fSL -o novnc-1.7.0.tar.gz https://github.com/novnc/noVNC/archive/refs/tags/v1.7.0.tar.gz
shasum -a 256 novnc-1.7.0.tar.gz   # 期望 b1003a11b6e6e8d8f7f5e5586daae7f8ca651d8aee0aa155ff9ac841c48f52c6
tar xzf novnc-1.7.0.tar.gz
```

更新本资产时: 重新下载新版本、按上表重建子集、更新本文件的版本/sha256/日期，并保持上游许可文件原样。

## 自有资产

- `index.html`、`berry.js`、`berry.css`、`berry-logo.svg`、`berry-bell.wav` 为本仓自有资产（Apache-2.0，见仓库根 LICENSE；logo 与铃音为本仓自产合成，非第三方），非 vendored——不入 THIRD-PARTY-NOTICES。
- 运行时 import vendored `novnc/core/`（MPL-2.0）的 `rfb.js`、`util/logging.js`、`input/keyboard.js` 三模块，不修改任何上游文件；与 vendored `novnc/` 树（含 `novnc/vnc.html` 原生页逃生门）零文件混置。
