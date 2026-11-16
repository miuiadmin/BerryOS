# Agent Note: G5 内核半——boot-params 消费端通道裁决与件缺静默口径

Status: implemented

## Problem

厂商定制化 G5（P1）：`[platform.boot.params]` 投放面（轮十五，`etc/boot-params.toml` 计划面条目）已就位但零运行时效果——「只验不透传」。内核半要落两键消费：`sched_slice_hz` 换算注入 slice 粒度、`intent_default` 新任务缺省 intent。设计（产品配置层 v1.1 §1.6-补）预置了消费路径（rootfs cpio 早期查找），但任务书要求先裁决：若「内核读不了 rootfs」则按候选通道修正（INITRD 段承载 / svcmgr 早期读 etc 回传）。

## Decision

**通道裁决：内核能读 rootfs，设计原案直接落体**（5 行记录 = `kernel/src/bootparams.rs` 头注）：rootfs boot 期缓冲（`kernel/src/main.rs:438-603` 三源装载）本就是内核可达字节流，cpio 定址查找是新原语但机制可行——消费端单点 `kernel/src/bootparams.rs`（newc 定址查找 = berry-loader `cpio_walk` 同口径内核侧窄化 + TOML 子集有界解析）；候选 a（INITRD.BIN 段承载）不采（新增产链序列化段面，违单一序列化器纪律）；候选 b（svcmgr 回传）不采（svcmgr spawn 在 timer::init/sched init 之后，回传无通道且时序倒挂——slice_hz 必须在 timer::init 前定）。

**两处口径修正（机制与设计表冲突的最小可行修正，推断标注）**：

1. **件缺 = 静默零行**（设计表原列「件缺 → loud 诊断行」）：工具侧「空 params = 不投放」使件缺是默认镜像常态，逐 boot 打诊断行违 smoke 逐行零漂移纪律。修正后 loud 诊断只对坏值/坏格式/schema≠1（坏键回缺省、好键照常应用）。
2. **intent 出生档双置**：注入点 `task.rs try_assemble` 同置 declared/effective（设计表只指名 intent_declared）——「出生档 = 自身声明」语义一致化，`spawn_intent` 先例。

配套：`sched_slice_hz` 换算单源 `timer::slice_cycles_for_hz`（init 的 SLICE 与 eco 行共用一处除式，防漂移）；`intent_default` 缺省对账落定 = unspecified（0x00 现役实值，缺席零漂移——§1.6-3 表注「utility〔推断〕」实证收口）。

## Alternatives considered

- **INITRD.BIN 段承载 params（候选 a / §1.6-4 原案二进制定长段）**：被拒——需给产链新增序列化段面（新格式新投放步），且 §1.6-补 已裁「locale 同构 + 单一序列化器 + 内核解析面零新二进制格式」。
- **svcmgr 早期读 etc 回传内核（候选 b）**：被拒——svcmgr 由内核 spawn（main.rs :1294），晚于 timer::init（:714）与首个 spawn（:766）；回传需新内核通道（syscall 或启动参数），且 slice_hz 决策点在 svcmgr 存在前。
- **fw_cfg 具名键递送 params**：被拒——§1.6-补 已裁出局（EFI 路无 fw_cfg 具名文件 = 真机不可达；rootfs 三源装载在位即双环境同链）。
- **件缺打诊断行（照设计表原文）**：被拒——见 Decision 修正 1（与「空 params 不投放」合同自相矛盾）。

## Consequences

- 参数真生效链闭合：定制镜像 QEMU 活证 `bootparams slice_hz=250 slice=250000 intent=interactive`（tiny 单卷 + full 多卷 VMO 拼装两路）；差分 64Hz → `slice=976562`；坏值 5000 → `bootparams: range` + 好键连坐不弃；默认镜像零行（bare/rootfs 双形态）——回归零漂移。
- ktest `test_bootparams` 静默盖解析/查找/注入三面（fail = panic 冒烟自然红，零输出行保逐行零漂移）。
- **「单编码器纪律」CPIO_PAD 消费点四点 → 五点**（新增 kernel bootparams）：mkcpio.py / berry-loader 头注的「四点同步」字样本刀不共笔（并行 lane 易漂移件），下次动那些文件时对齐；改垫粒度的 grep 面（`4096`/`CPIO_PAD`）现含 kernel/src/bootparams.rs。
- 实件投放（etc/boot-params.toml 生成）仍归全产线步⑥（V13 主会话收口）——本刀验证件用 mkcpio 位置参数 `etc/boot-params.toml=<file>` 手工产链，即全产线将用的同形入口。
- 解析面向前兼容：未知键跳过不弃段（v1 内核跑 v2 镜像），产线硬拒在 berry_assemble——两侧宽严分工；schema 头 ≠1 全段弃（防 v2 语义被误用）。

## Related

- 设计落体记录：`docs/产品配置层与槽位合同设计-v1.1.md` §1.6-补二（轻追记四）。
- 投放面（轮十五）：同文 §1.6-补；`tools/berry_assemble.py` BOOT_PARAMS_SPEC/BOOT_PARAMS_ENTRY。
