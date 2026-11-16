# 产品 e2e rig 收编

Status: implemented

## Problem

多个现行标准和阶段条目引用“产品 Recents/frame-cache e2e 全绿”，但执行脚本只存在于 gitignored `out/`，且硬编码本机绝对路径。换机器或清理 `out/` 后，这些验收声明不可复现、不可审计，退化成一次性口头证据。

## Decision

新增 `tools/product-e2e.py` 与 `just product-e2e`，把 Recents、owner frame-cache 与 shell-press 三条产品链收编为可审查、可复跑脚本。脚本从仓库相对路径装载 `tools/gui8-smoke.py` 的 QMP helper，保留真实 HVF + virtio-gpu + Berry Room + 1080×1920@60 拓扑，去掉了本机绝对路径，并支持 `--suite recents|frame-cache|shell-press|all`、`--image`、`--work-dir`、`--code`、`--qemu`、`--timeout`、`--keep`。

产品链除原有像素与串行标记断言外，新增产品 BootProgress `m=1..11` 严格顺序断言与 `VGPUBOOT fail` 拒绝。这样“胶囊进度真值链”与“完成后切桌面”成为可指名的机器验收，而不是只数 `BOOT100`。`just product-e2e` 默认重建缺失的 `out/berryos-desktop.img` 并依次运行两套；`PRODUCT_E2E_SUITE`、`PRODUCT_E2E_KEEP`、`PRODUCT_E2E_REBUILD` 提供本机开关。

## Alternatives considered

- 继续把脚本留在 `out/`：与 `.gitignore` 冲突，不可复现，拒绝。
- 直接把两份脚本复制入库：保留大量复制粘贴与硬编码路径，维护成本高，拒绝。
- 把产品链并入 GUI8：GUI8 的哨兵/常态拓扑和产品 desktop 镜像拓扑不同，混在一起会弱化两边语义，拒绝。
- 只断言串口标记不做截图：无法验证 Recents/frame-cache 像素与 owner 恢复，拒绝。

## Consequences

`PRODUCT_E2E_KEEP=1 just product-e2e` 已在真实产品镜像上通过：Recents 六项基础像素检查、Inputs Tab badge 命中、Android 透明系统栏/AOSP 三键像素、edge-to-edge APP 与软件 LTPO 60/30/1Hz 真值、frame-cache 七项像素检查、shell-press Dock 按压/真实启动链、三条 suite 的精确标记、产品 m=1..11 里程碑链和 `KERNEL PANIC=0` 全绿。旧 `out/verify-recents.py` 与 `out/verify-frame-cache.py` 仍可作为历史产物，但不再是验收来源。

这条收编不改变产品行为，不改变 1080×1920@60、HVF、virtio-gpu、Berry Room、present wire 或 soft↔vgpu 共识。Photos 沉浸恢复、Files 长按/刷新、TextArea 真键、Contacts Large AppBar、verbose system 面等专项 rig 仍未入库，需后续按同一模式继续收编。

## Related

- [OpenHarmony 式产品系统壳](2026-11-15-openharmony-product-shell.md)
- [双模式启动面与真实胶囊进度](2026-11-15-dual-mode-boot-progress.md)
- [三方共性控件体系](2026-11-15-tri-platform-controls.md)
