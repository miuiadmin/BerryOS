# C 刀三件：bench 预检逃生位 + vp() 单点 + CLI-first 改名 v1.8

Status: implemented

## Problem

F 号可做面三件（[F-P1可做面扫描](../../../research/全面遗漏审计/F-P1可做面扫描.md) §3，任务书 C）：

1. **F-C6**：`tools/bench.py` 的 `qemu_preflight()` 场空预检在 owner QEMU 长跑在场时死等 900s 后 loud SystemExit 硬拒——工具被堵死 15 分钟别无出路（45 号 §4-8 实咬；[bench_baseline_icount.notes.md](../../../tools/bench_baseline_icount.notes.md) 环境注记两轮 900s 等空超时在案）。
2. **F-B3**：`berry-launcher/src/desktop.rs` 布局断点 `vp_w >= 600 && vp_h >= 1800` 是竖屏 1080×1920 专属像素阈（变量名 vp_w/vp_h 实为 px——03 号 §3.2 病灶②），berry-ui 无 `vp()` 换算单点、OH_* 常量族手乘散布。
3. **F-D7**：`docs/Berry设备平台-CLI-first设计-v1.0.md` 文件名停在 v1.0 而头部已 1.8——名/版双标，违反版本化文档纪律的最重存量案（索引行同停在 1.5）。

## Decision

1. **bench.py 逃生位（已落地）**：等空超时从 SystemExit 硬拒降级为**非阻塞 Warn 放行**（stderr 两行：通知 + Warn 带在场 pids 与污染面说明——墙钟通道可能被污染、icount 虚拟时钟免疫）；新增 `--qemu-wait SECONDS`（默认 900，0 = 单探即判，负值拒绝；可与任一通道组合、入 `--selftest` 独占守卫）；pgrep 自身失效（rc∉{0,1}）仍硬拒。selftest 新增第 12 节六腿（subprocess.run 打桩：空场静默过 / 占场帽 0 降级 Warn / 等空轮询两探让场 / 守卫失效仍硬拒 + 子进程契约两腿），57→63 面全绿；另以伪签名进程实跑 live E2E（真实 pgrep 路径，恰逢 owner 三 QEMU 在场）确认 Warn 放行。
2. **vp() 单点（已落地）**：`berry-ui/src/lib.rs` 暴露 `pub fn vp(v: u32, density: u32) -> u32`（整数密度 saturating 乘法，03 号 §3.2 层二原案）+ 内联钉板两测（OH phone 真值锚 39vp@3=117px、360vp@3=1080px）；berry-ui 339 测全绿。**desktop.rs 断点 vp 化被阻**：开刀时桌面文件空闲，中途（23:34）被波3 批C1 会话占用（SYSTEM 窗/三栏重写 +332 行，含自建 `sys_density()`——其内部仍保持原像素判据「双口径判据保持原判据」）；按任务纪律「中途被占→blocked」已精确回退本刀在 desktop.rs 的唯一落笔（import 行），该文件零残留归波3 会话。断点 vp 化改刀须待波3 收笔，且须与 `sys_density`/`sys_bands_px` 单点协调（03 号层三层一 v0 档表推方案见 F-P1 §3-5，density_for 按 render 宽 1080 推 3 的设计随本笔记留档）。
3. **CLI-first 改名（已落地）**：文件名升 `docs/Berry设备平台-CLI-first设计-v1.9.md`（最小修=名就版，头部 1.8 为真值）；索引行三处对齐（链接/标签版本 1.5→1.8/定位列补 v1.8 一句）；现行文档入链清点改指 7 件（索引 ×2、架构决策 ×2、剩余缺口、验收清单、壳草案 ×3、tools/berry/README）；阶段沿革 :2200 的「新立 …-v1.0.md」为历史记录按纪律不回改。docs_guard rc=0、check-docs 仅余 owner 新笔记 2 条告警（非本批）。

## Alternatives considered

- **超时仍硬拒、仅加旗标绕过**：违背任务书「非阻塞 Warn 而非硬拒」的降级定调——owner 不知旗标存在时堵死依旧；且 CI 场地常态为空、超时路径只在真占场时触发，Warn 放行 + 污染面说明优于代拒。
- **断点 vp 化强行并刀**：波3 批C1 正在重写 build() 全域，并行写同一函数 = 双向丢笔风险 + 违「别人的已改文件一律不碰」；其 sys_density 已把判据收敛为单函数，后续一刀改一处即可，收益不损。
- **CLI-first 头部降 v1.0**：头部 1.8 有 v1.1-v1.8 八波完整变更序（VNC 六波等已落事实），降版 = 抹史；升名是唯一方向。
- **顺手同修 settings main.rs:293 同款像素断点**：任务书只点名 launcher desktop.rs；settings 非本刀文件域，留待断点 vp 化改刀时同批（发现已记，非推断）。

## Consequences

- bench 六通道在 owner QEMU 在场时可拿到降级放行跑完（icount 面数值可信，墙钟面自查假红）；CI 无人值守场景不再有 900s 死等，代价 = 失去「超时即失败」的代拒语义（Warn 承载）。
- berry-ui 公共 API +1（vp），settings/launcher 依赖面零扰动（纯增量）；波3 收笔后断点 vp 化有了层二单点可直消费。
- CLI-first 名/头/索引三处对齐，版本化文档纪律恢复；.agents 历史 VNC 文档的旧名链接由并行清链会话同窗刷齐（23:32:40 批量落盘，非本批动作）。
- 挂账：desktop.rs 断点 vp 化（待波3 收笔，锚 = sys_density() 现位）+ settings 同款断点（user/bins/settings/src/main.rs:293）+ 03 号层一 v1（info VMO density 域，与 02 号动态分辨率合刀）。

## Related

- 任务书：P1+P2+集成批量清偿 C 刀（F-C6/F-B3/F-D7）；[F-P1可做面扫描](../../../research/全面遗漏审计/F-P1可做面扫描.md) §0/§2/§3。
- 设计依据：[03-1080P显示框架总设计](../../../research/显示与加速体系设计/03-1080P显示框架总设计.md) §3（density/vp 框架三层）。
- 验证实录：bench selftest 63 面 PASS（tools/bench.py --selftest）；berry-ui 339 测 PASS（cargo test -p berry-ui --target aarch64-apple-darwin）；docs_guard 0 fail；check-docs 2 告警均非本批。
