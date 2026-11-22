# Agent Note: docs_guard 上 main 同步 + 索引补登（Curia 1:1 的 CI 收口半刀）

Status: implemented

## 动机

- main 的旧版 `tools/docs_guard.py` 在 1:1 载荷上直接崩溃（"AGENTS.md 缺失" exit 2——
  旧探针找不到模板态 AGENTS.md）→ CI smoke 的 docs-guard 步必红。
- wip 侧 `c671fe1b` 已做 1:1 适配（G2 行长律撤 / G3 换 Curia 契约锚 / G5·G7 三桥契约），
  但该版 guard 在 main 报 G10 未登文档两件：GUI-6 / GUI-8 实施设计草案（2026-11-06
  入库时索引漏登）。

## 处置

- `tools/docs_guard.py` 整取自 origin/wip/sched-1.1 @ `d380dd67`（1:1 适配版）。
  **不**携带 wip 的 docs-guard-adapt 笔记——其相对链接（revert/upgrade 两篇）在 main
  悬空，实证：check-docs --strict 3 条组 3 告警全由该文件引起；剔除后 main 纯树 0 告警。
- `docs/索引.md`：五区补登 GUI-6 / GUI-8 两行（状态与钦定源照草案头块抄录）；
  头部主索引 87→89、五区标注（40）→（42）。

## 验证（main 净树逐项，退出码不接管道实测）

- `python3 tools/docs_guard.py` → PASS 0 警（exit 0）
- `node .agents/tools/check-docs.mjs --strict` → 121 篇 0 告警（exit 0）
- `node .agents/tools/check-docs.mjs --health` → 载荷 14/14 + 三桥 symlink-ok

## 未了（不归本刀）

- `tools/writable_globals.py --check`：main 与 wip **双红**（kern_lock::NAME_HOOK /
  spin::SPIN_BARK / timer::SLICE_HZ_OVR 未入册）——scheduler 域，随 wip 侧修复合流。
- GitHub Actions billing 阻断（2026-11-12 ~03:58Z 起 job 不启动即死）——账户层，
  需在 Billing & plans 处理。
- smoke 的 cargo/build 段自 2026-11-06 起未真正执行过（先红 writable-globals、后阻
  billing），billing 解除后首跑见分晓。

关联：[2026-11-12-upgrade-curia-25-1to1-main.md](2026-11-12-upgrade-curia-25-1to1-main.md)
