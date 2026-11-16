# Agent Note: 刷新率刀1 率通路五段贯通（@Hz + boot VMO + PANEL_CAPS）

Status: implemented

## Problem

[刷新率规范 v1.0](../../../docs/刷新率体系规范-v1.0.md) 立法（9ff8b77b3）后率通路五段全无实现：fw_cfg `opt/berry/display` 的 @Hz 后缀被内核丢弃、boot info VMO 无率域、周期常量三处硬编码各自为政（guifbd `SOFT_VSYNC_NS` / launcher `VSYNC_NS` / berry-ui DEFAULT）、服务端率值无通道到达客户端。刀1 = 打通 fw_cfg → 内核解析 → boot VMO → guifbd 档位表 → PANEL_CAPS op → berry-ui 注入全链（[分刀方案](../../plans/2026-11-15-刷新率体系实施分刀方案.md) §2）。

## Decision

- **校验分家（一事实一归属）**：内核只做语法校验（`display_mode_req` 拆 `@` 取 hz，1..=1000 外 = `DISP hz fail` 诊断行 + 回退 60）；档位成员校验归 guifbd `mod panel::TIERS` 单源（`resolve` 落表外 = eprintln + (60, 16_666_667) loud 回退）。内核不持档位表副本——面板宣告语义全部在 guifbd。
- **PANEL_CAPS = 0x44（hdi 域第五 op），应答自立帽**：请求 4B 无参（VBLANK_WAIT 形）；应答变长 32B 头 + 8B×n 档位行，`PANEL_CAPS_REP_MAX = 32 + 8×16 = 160` ≤ CHAN_MSG_MAX=256；解析定长拒尾随。VER_HDI 1→2。不扩 HDI_REP_MAX——那是 REG_VBLANK 16B 载荷的钉板契约，动它 = 破 v1 冻结面。
- **boot info VMO 48B→56B 尾插第七 u64 域 gui_hz（add-only）**：旧内核 zero 尾 → guifbd `rd(6)=0` → resolve 回退 60；旧 guifbd 只读前 48B 无感。落点 kernel/src/main.rs 的 info 构造处——计划写 handoff.rs，实测 VMO 构造本在 main.rs，落点从实。
- **客户端注入 version_hdi≥2 版本门**：launcher 仅在 GETINFO `version_hdi >= 2` 时发 PANEL_CAPS 取 (period_ns, phase_ns) 注 `set_vsync`；否则保 berry-ui `DEFAULT_VSYNC_PERIOD_NS = 16_666_667`（注释改「无服务端回退缺省」——真值单源 guifbd 档位表）。新旧混跑零破面。
- **gui-wmd 中继双端区间同步扩 `..=OP_PANEL_CAPS`**：main.rs `on_wm` hdi 臂 + relay.rs `expect_attach` (0,0) 臂——漏任一端，launcher 的 PANEL_CAPS 在中继即 ST_BAD。
- **hdi 域编码器 usize 直返**（present 域 Option 形不同）——launcher 侧不套 `enc_req` 包装。

## Alternatives considered

- 扩 `HDI_REP_MAX` 容变长应答：否——REG_VBLANK 事件 16B 载荷契约钉板（GOLDEN 面），变长应答自立 160B 帽零碰撞。
- 内核持档位表做语义校验：否——档位表单源在 guifbd（面板宣告者），内核持副本即双写。
- launcher 无版本门盲发 PANEL_CAPS：否——旧 guifbd 回 ST_BAD 1B 帧，`parse_panel_caps_rep` None 走 fail 杀启动；版本门保混跑。

## Consequences

- guiproto host 测试 46/46 绿（新增：9 档表镜像往返、FreeRange n=0 槽清零形、ST_NOENT/n 失配/超帽/逐字节截断/尾随拒；pin 测试 VER_HDI 同批抬 2）。
- 四仓 cargo check 绿（guifbd/gui-wmd/berry-launcher 独立 workspace + kernel，worktree 本地 target 让行）。
- smoke 三跑两红一绿全环境面：首红 = echod 18080 被兄弟会话主树 smoke 占用（端口占形态；guest 标记实已全绿——SLIRP 对端连的是兄弟 echod，功能等价）；二红 = FAT 子目录相 420s 零标记，宿主 5 分钟负载 4.54（兄弟 QEMU + berry-live 100% 一核）TCG 饿死——同内核同拓扑前三相连跑全绿、宿主安静后重跑全相 PASS（真退出码 0）。gate 碰撞判例再添两形态；重活严格串行、一次只跑一个 QEMU 链。
- **教训：`check-docs.mjs` ≠ `tools/docs_guard.py`**——立法提交 9ff8b77b3 只过了前者，漏 docs/索引.md 登记（G10），smoke 才抓到；规格登记两工具都要过。
- gui8-smoke 收口（刀1 门跑带出 cbox 存量冲突）：修后 PASS（真退出码 0，boot B 零重写断言在席）；`PANEL ok hz=60 ns=16666667` 证据行在 gui8 双 boot 串口在席（800x600@60 档位表真值，version_hdi=2 门注入端到端生效）。冲突根因与修法见 [gui8-cbox 冒烟相冻结笔记](2026-11-16-gui8-cbox-smoke-gate.md)（cbox 刀 2 带病面，非刀1 缺陷）。
- @Hz 端到端探针三 boot 全绿（probe 拓扑照 gui8-smoke：AAVMF + 快照镜像 + ramfb + display fw_cfg，每 boot 钉自己 QEMU PID，300s 帽）：`720x1280@30` → `DISP ok 720x1280@30` + `PANEL ok hz=30 ns=33333333`（档位表真值直通，无回退行）；`720x1280@77` → 内核语法过（`DISP ok @77`）+ guifbd loud 回退行 `panel hz=77 not in tiers, fallback 60` + `PANEL ok hz=60 ns=16666667`；裸 `720x1280` → 缺省 60 直注（`DISP ok @60` + `PANEL ok hz=60`）。校验分家（内核语法 1..=1000 / guifbd 语义档位归属）两臂各有实证；720x1280 超启动帧帽走 `DISP defer`→分配→`DISP ok` 路径顺带验证。

## Related

- [刷新率体系实施分刀方案](../../plans/2026-11-15-刷新率体系实施分刀方案.md)（本刀 §2）
- [cbox 崩溃账本刀谱笔记](2026-11-15-cbox-knife1-kernel.md)——同名 worktree 另一刀谱，互不相干
