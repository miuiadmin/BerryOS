# Agent Note: gui8 零重写不变量 × cbox 落账窗结构性冲突（冒烟相冻结）

Status: implemented

## Problem

[cbox 刀 2](2026-11-15-cbox-knife2-userland.md) 落地后 `just gui8-smoke` 恒红于 boot B 盘哈希断言（数据盘字节变化——跨 boot 读路径不应重写），两跑两红确定性复现。cbox 刀谱收口时未跑 gui8-smoke（笔记零记录），带病入库。

## Decision

根因链（留证盘 + 刀 3 自家工具闭环取证）：

- 内核 cbox `on_fault` 钩记录一切 EL0 fault——**ktest 的 rogue 自验件故意杀（`G1.5: kill user task 'user_rogue_ptr'`）也是 fault** → 每 boot（含 EFI 链 gui8 boot）内核 RAM 表必有 ≥1 条记录。
- launcher init 第三窗（`cbox_io::init_flush`）排液一次 + `(boot, seq)` 去重合并——boot B 的 rogue 记录是**新 boot 新 seq**，`appended > 0` 必重写 `CBOX.LOG`。
- 实证：`GUI8_KEEP=1` 留证盘经 `berry crash-log --disk layout.img` 解出 `CBOX.LOG 2 条 / 1 指纹：user_rogue_ptr pc=0x1004 boot 1..2`——boot A/B 各一条，boot B 的新账即重写即哈希漂移。boot A 串口另有 `PANEL ok hz=60`（刷新率刀1 证据行）——率通路无辜。

修法（两侧同门，第一侧先行翻车实录）：

- **首修只门 launcher 窗三**（`if !smoke { cbox_io::init_flush(); }`）——重跑仍红。boot B 串口 `CBOX ok n=2` 铁证：**svcmgr 窗一 battery 每 boot 也落账**（ktest rogue kill 早于 battery 窗 → 期望 n≥1 语义面在 fixture 世界必写）。教训：三窗是三个独立写者，堵一个不等于堵全部——取证面串口标记（`CBOX ok`）比盘面哈希更早指出真凶。
- **终修 = 三窗全冻结**：窗一/窗二经 svcmgr `cbox_flush` 顶上门（`cbox_smoke_freeze`：哨兵 `GUI8.SMK` get_chunk 精确匹配——定长 22B 门 + 字节全等，Noent/错载荷 fail closed；battery/死亡面共用收口点一次覆盖；死亡面 stale 排空在门前照走——会话卫生态不随冻结退化；fixture 形 battery 标记变 `CBOX ok n=0` = 空账本真值）+ 窗三 launcher `if !smoke` 门（`layout_io::smoke_mode()` 同一哨兵——wire 契约常量双读者，单源在 gui8-smoke 夹具）。fixture 世界冻结语义与 layout_io 冒烟支路同族；常态/真机 boot 落账面零变化。[design.md](../../plans/cbox/design.md) 窗三节后同批补注（v0.5.0→v0.5.1）。

## Alternatives considered

- gui8-smoke.py 侧做 FAT12 解析把 CBOX.LOG 从盘哈希中挖除：否——harness 长出第二套 FAT 读码，复杂度与维护面远超一行门；且断言本意护的是 layout 读路径，不护账本文件字节。
- 内核 on_fault 对 ktest 故意杀免记：否——内核无法分辨故意杀与真崩溃（ktest rogue 与真 rogue 同一 fault 形态）；且「每 boot 一条 rogue 噪声账」是 cbox 设计既裁语义，翻案归 cbox lane。
- gui8 数据盘预植满账（LEDGER_MAX）使 append 拒绝：否——测试夹具伪装生产语义，脏。
- svcmgr 侧哨兵探测结果缓存 static（battery 算一次死亡面复用）：否——flush 是稀有事件（battery 一次 + sole 死亡），每次一条 22B get_chunk 可忽略；少一个 static 状态面。

## Consequences

- gui8-smoke 终修后须全绿（boot B 哈希稳定——冒烟相三窗均不落账）；刀 1 的 `PANEL ok hz=60 ns=16666667` 证据行在 gui8 双 boot 串口在席（800x600@60 档位表真值）。
- fixture 相 battery 标记面从 `CBOX ok n=1/n=2` 变 `CBOX ok n=0`——gui8-smoke 不断言 CBOX 标记，零碰撞；主路 smoke（盘无 GUI8.SMK）标记面不变（n≥1）。
- 教训（跨刀收口纪律）：改用户态常驻件的落盘面，收口门必须含 gui8-smoke——smoke.sh（rootfs 相）不跑 launcher，覆盖不到 EFI GUI 链的盘写；且**多写者面修冲突先枚举全部写者再动手**——首修只堵一窗白烧一轮重烘重跑。
- cbox 账本在真机/常态 boot 每 boot 至少一条 rogue 噪声账（ktest 先于一切）——设计既裁，非本修面。

## Related

- [cbox 总体设计](../../plans/cbox/design.md) 窗三节（本修补注处）
- [刷新率刀1 率通路](2026-11-15-refresh-knife1-rate-path.md)（发现本冲突的门跑属其收口管线）
