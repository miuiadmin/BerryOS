# Agent Note: ux-splash 孤儿分支重放入主干（刀 1 splash 半）

Status: implemented

## Problem

boot splash 双模式整套（含五键导航、press_map 按压态、HD 百分比字模）停在孤儿分支 rescue/ux-splash（7fd1709c）——主干历史重写后无 merge base，owner 持续体验「黑屏 boot + 三键导航 + 零按压反馈」的旧形态。

## Decision

按 lane-rescue 纪律文件面重放至新基底（origin/wip/sched-1.1 @148c8acf，worktree `../berryos-ux`，分支 wip/ux-unify）：

- 整取（origin == rescue 基底零漂移）：kernel splash.rs/splash_pct.rs/gen_splash_pct.py、arch boot/mmu/smp/uart（HVF 判例防御）、kernel main.rs/Cargo.toml（berry-fbcon 依赖）、berry CLI 主链四件。
- 三方合并干净：ktest.rs（8 段相位格）、dpg.rs（noktest 门）、window.rs（boot_mode 六点参数链）、gui8-smoke.py。
- 手工移植：guifbd guest.rs fbinfo 三态（1=splash/2=verbose）+「BerryOS starting...」过渡行（origin 已拆 main/guest 双文件，rescue 是老单文件形态）；berry-ui PRESS_MAP_MAX 16→28。
- launcher 四件采用「origin 版 + rescue 增量三方合并」并清理并集重复（三键常量、NavNodes 重复字段、nav_icon 闭包残段、BUILTIN_FACES 双定义、frameloop 嵌套破损）——origin 已入库的 TEXT_METRICS 演进完整保留。
- origin 已随别的 lane 入库的面不重放：rtc/psci SYSTEM_OFF/berryctl power/DPI 窗口/gfxdemo 清屏/sched re-export。

构建验证：内核 debug 绿、launcher musl 链绿、berry CLI host 绿、build-user 全链绿（rootfs 34.9MB ×60 条目 tlsscan 58 ELF 全过；绕过 Documents 树 File Provider 间歇 EPERM 用 /tmp target + sed 版 justfile/manifest）。镜像 mkimage 新鲜（mtime 对账过）。boot 实证：FB ok 480x854 + ROOTFS ok + 无 PANIC + splash 帧截屏（黑底白 logo/条/百分比）；noktest 门实证 LAUNCH v=1、switches 活（桌面饿死 = 在册 P0-1 修窗五件，他 lane 域非本刀回归）。

## Alternatives considered

- **cherry-pick/squash 整链**：merge-base 在被剥旧谱系上，会把重写剥掉的旧 base 内容当净增量带回（整库 diff），skill 明令禁止。
- **launcher 四件整取 rescue 版**：会覆盖 origin 已入库的 TEXT_METRICS/坑 6 演进（406/184/160/49 行差），先错后纠正——最终采用 origin 版 + 增量三方合并。

## Consequences

刀 1 完成 splash 合入半；入口统一半（五入口默认值收敛）与刀 2（状态栏 OH 形）未动，见 [2026-11-14-模拟器体验统一分刀方案.md](../../plans/2026-11-14-模拟器体验统一分刀方案.md)。verbose 形未单独跑（uart mirror 路径随重放入库，构造性验证）。noktest-wrap 包装脚本落 worktree out/（gitignored，复现法见 berryos-sim-shell skill）。

## Related

- [2026-11-14-模拟器体验统一分刀方案.md](../../plans/2026-11-14-模拟器体验统一分刀方案.md)
- [2026-11-14-survey-openharmony-ui-specs.md](../../research/2026-11-14-survey-openharmony-ui-specs.md)
