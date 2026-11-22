# Agent Note: cbox 刀 2 用户态半件（三落账窗 + berry-cbox 纯核 + CBOX.LOG 落盘）

Status: implemented

## Problem

[刀 1](2026-11-15-cbox-knife1-kernel.md) 落了内核半件（RAM 记录表 + trap 双钩 + ABI 0x42 排液）；本刀补用户态半件：崩溃记录要能活着跨 boot——svcmgr/launcher 在机制性唯一活跃 fs client 的安全窗里排液落账 `CBOX.LOG`（FAT 卷根），设计从 v0.1.0 的「battery 单窗」升格 v0.2.0 三窗案（[design.md](../../plans/cbox/design.md) 持久化节——fsproto 无请求号串行面下，落账窗安全 = 机制性唯一客户，非时序赌目）。

## 处置

- `user/berry-cbox` 纯核 crate：快照解码 / 账本行编解码（解析严格，非数字字节显式拒）/ `(boot, seq)` 去重合并 / 超帽轮出 / `boot_pick` 续号——svcmgr / launcher / 宿主 berry 工具三方同引，host 单测钉板（justfile test-host 面接入）。
- svcmgr 窗一 battery：fsmgr spawn Ok 后 vnet 前，门 `fs_client != 0`，成功 `CBOX ok n=<行数>`（车辆面在册标记）、失败 eco 降级行 `cbox bat`；RAM-only 形（EFI 冒烟路）照发标记。
- svcmgr 窗二死亡面：`app_supervise` TERMINATED 命中后排液，新 seq 恒打 `cbox rec`（serial 面包屑）；落盘加 sole 门（post-take APP_CHILDREN 空 = 无并发 fs client），经 FS_A + `cbox_drain_stale` 预排空死者 mid-RPC 残留应答；写失败 `cbox w`。
- launcher 窗三 init：layout 装载后自持 fs 会话排液落账一次；冻结相 `fs_h()==0` 结构性直退；失败非致命（`launcher: cbox` 前缀，不产 `CBOX ok` 子串）。
- 内核回修一刀：NR_CBOX_DRAIN 初判 SVC_CLASS_NONE 为漏审——出参 copy_out 链经 walk_leaf 无锁走查页表 = §A113 读者面（0x61/0x87 同链同判），升 SVC_CLASS_TABLE，NONE 实装号归零不变量恢复。
- 排障三缺陷（跑 1–5 迭代）：
  1. 临时诊断行 `copy_from_slice(14≠11)` panic——no_std 用户任务 panic 零输出，svcmgr 被 G1.5 静默击杀、整条监督链饿死；stripped ELF 三步定位法（陷阱惯用法 + 寄存器实参 x0/x1/x2 + 字符串 VMA 交叉引用）破案，全量回退 TEMP-DBG 脚手架（no_std panic 静默击杀属会话外部记忆，不入仓）。
  2. `CBOX_PATH` 带尾 NUL——fsproto RPC 面 `path_valid` 尾随 NUL 拒 → ST_BAD → 码 1；两制厘清：RPC 直调走裸字节切片，libc `open` 才要 C 串（`resolve_to` 剥壳）。
  3. `cbox_ok_line` 漏 `n=`——`CBOX ok 2` 与在册标记 `CBOX ok n=` 非全等，门判 FAIL；标记格式按 smoke 标记纪律全等比对。

## 验证

- 跑 5（终形：TEMP-DBG 全退 + `n=` 修正 + 内核类修）`SMOKE_KEEP_LOG=1 just smoke-image` exit 0 全绿；串口 `CBOX ok n=2`（日志 0DXgK8 L316）与击杀时间线对账闭合：kill#1 `user_rogue_ptr`（L166）+ kill#2 `user_child`（L228）皆 battery 前，n=2 精确。
- kill#3（L408，reapq 自测）为内核自测任务非 APP_CHILDREN 成员，`cbox rec` 静默为正行为；APP 电池 9 app 正常终到零假 eco = 窗二负例过关（正例 = 真 app 崩溃，smoke 无此场景，挂刀 3 端到端配方承接）。
- 主路 `just smoke` exit 0 全绿（78 标记零回归）；7 个带盘相位 boot 串口 `CBOX ok n=` 全席、`cbox bat`/`cbox w` 零降级行。逐相对账（n = 写后账本行数）：rootfs/RX/dirs 各新盘 n=1；持久化相复用 RX 盘 n=2；掉电 A/B/C 复用链 n=1→2→3——跨 boot 累积语义实证。
- 盘面真值：`python3 tools/fatcat.py disk.img CBOX.LOG` 提出 2 行 490B（boot=1 seq=1 `user_rogue_ptr` + boot=2 seq=1——`boot_pick` 续号 +1、vct 不同非全等不并组，正合规格）；行格式 16 字段与 spec 冻结序一致。OUT.TXT 对照提取同步过（工具健全性双验）。
- fatcat 本批修一缺陷：主路盘 LBA0 是 `BERRYOSVBLK!` 印章保留区非 MBR，FAT 卷真身在 LBA16——初版从 LBA0 读 BPB 得垃圾（OUT.TXT 也「找不到」的构造性无关暴露）；改卷面发现（LBA0 验 BPB 健全性，否则落 LBA16）。

## 边界

- kill#3 类孤儿记录留内核 RAM 表，兑现面 = 后续 boot 的 battery 窗（主路多相 boot 链正是此形态）。
- `tools/fatcat.py` 为本刀宿主侧临时检视件（FAT12 根文件直读），刀 3 `berry crash-log` 落地后由其承接超集。
- 多 app 稳态死亡面不落盘（sole 门不开）、窗二 stale 应答复余窗、boot_pick 全等并列边角——三条挂账见 design.md 边界节。
- 掉电竞技场（disk.pwr.img）终态观察：CBOX.LOG 目录项在册（735B = 3 行）但 FAT 链断（首簇项 0x0）——B4 测试以 SIGKILL 撕盘为目的，Boot C 排液与读回验证自证过，链断属场内附带面。刀 3 `berry crash-log` 读面必须容忍此态（链断 = 账本不可读，按 spec 非零退出如实报）。
