# GUI F9 收口：删除 nshfont 并落地静态守卫

Status: implemented — 2026-11-08

## Problem

F8 后六个普通 GUI app 已全部走 Berry Room + BerryUI，但迁移期临时 crate `berry-nshfont` 仍留在仓库。它曾集中持有字体 VMO 映射、WM/frame 客户端习惯和像素基元调用；继续存在会诱导新 app 回到第三套 UI 路径。路线禁令此前主要靠文档和人工 review，没有机器化 CI 门。

## Decision

1. 删除 `user/berry-nshfont/`：字形 VMO 映射归 Berry Room，光栅算法归 BerryGFX，文本/控件消费归 BerryUI，授权 fs RPC 归 berry-fsclient。
2. 新增 `tools/gui_guard.py`：
   - 识别 manifest `type=app`、`wm=true`、Retained 的普通 GUI app；
   - 钉住 Settings/Files/Contacts/Browser/Photos/UITEST 六个已知 app；
   - 强制直接依赖 `berry-room` 与 `berry-ui(guest-font-vmo)`；
   - fs 能力与 `berry-fsclient` 双向对账；
   - 去 Rust 注释后拦截 channel/vmar/vmo/handle/process/wait/clock、WM RPC、`gp::`、`FRAME_VA/FONT_VA`、stride、put/blend/round/circle/shadow、`berry_gfx::surface`、unsafe 与 alloc；
   - 拒绝 `berry-nshfont` crate 或依赖回潮。
3. 守卫提供 `--self-test`：一个六 app 正样本 + syscall/vmar/stride/pixel/unsafe/nshfont 依赖/nshfont crate、块注释与 raw-string 绕行、target/build 依赖、嵌套源码、immersive、feature/fs、运行时依赖冒充等十七类负样本，全部 fail closed。
4. 执法入口进入 `just build`、`just check`、`just build-release` 与 GitHub CI 的 fail-fast 阶段；新增 `just gui-boundary-guard`。
5. 清理现行文档和调试产物中的旧 Native Shell 口径；历史迁移规格与阶段沿革保留时点决策链。
6. 未来 P0 CanvasSurface / immersive renderer 落地时，必须先给守卫增加 manifest surface role/mode 和授权 allowlist，不得删除守卫绕行。

## Alternatives considered

- 仅靠 grep 手工检查：新 app 或重构时容易漏检，无法进 CI；否决。
- 保留无消费者的 nshfont：会继续保留第三套 API 诱惑和陈旧文档负担；否决。
- 把 BerryGFX 也从普通 app 禁掉：app 仍需 CommandEncoder/GlyphPos/FontSize 构造 DisplayList；只禁 `berry_gfx::surface` 直射像素面。
- 现在为 Canvas/游戏开宽松例外：P0 授权 surface 尚未接线，例外会成为绕行洞；否决。

## Consequences

- 六个普通 GUI app 均无法再直接依赖 berry-sys/berry-guiproto/berry-nshfont 或手写 WM/VMO/stride/像素路径而不红灯。
- `berry-nshfont` 删除不改变任何 `.bap` 或 rootfs 字节；F9 保持 rootfs 总量 33,780,848B、rootfs0 恰 32MiB 单卷帽。
- `just gui-boundary-guard` 成为 GUI SDK 的最低边界门；后续新增普通 GUI app 必须同步扩展已知 app 集与测试。
- 规格见 [BerryGUILanding-F9收口与守卫-v1.0](../../../docs/BerryGUILanding-F9收口与守卫-v1.0.md)。
