# GUI-2 段③ 消费侧首刀实施设计草案（B 形态接线——guifbd present/hdi 域服务端 + gfxdemo 首帧上屏）

> 状态：**scout 完，设计待续**（2026-11-01 立稿——§0/§1/§2 骨架 + §7 开放问题全录；§3 逐文件落点/§4 标记/§5 验证/§6 编排骨待填）。
> 前置：GUI-2 段①② 已落 main（`5317e09`/`098d01b`——berry-gfx pass 形态 API + soft 光栅核 host 面全备）；GUI-4 段①② 已收官（guiproto 三域 wire 全定形 + input 域接线，`1023072`）；GUI-1 显示链在 CI（FBW/FBSERVE）。
> 定形锚点：`docs/GUI里程碑规划-2026-10.md` :102——「GUI-4（输入事件往返）或 GUI-2 消费侧首刀，先到先定」——**GUI-4 已到，本刀即锚点兑现刀**。

## §0 范围（本刀做 / 不做）

**本刀 = B 形态三件事的第一个真实消费者闭环**：berry-gfx 渲一帧 → VMO → `SUBMIT` → guifbd present 域服务端 → ramfb 上屏 → `FRAME_CALLBACK` 回执 → smoke 断言。协议 wire 层段① 已全定形（本刀零协议改动——ADR #54 纪律「协议先定形、无消费者不接线」的接线半边）。

**做**：
1. guifbd serve 循环升格：单路 console recv → **wait_many 双路**（console pipe + present 域 channel）；
2. present 域服务端真身六 op（GETINFO / SUBMIT / FRAME_CALLBACK / RELEASE / VOTE / POWER_MODE）最小实现；
3. hdi 域 ramfb 第一实现（REG_VBLANK / VBLANK_WAIT / PREPARE / COMMIT——软 vsync，`§9-⑨` 挂账兑现）；
4. console 路恒 FULL damage 语义验收（规划 :102「第一个消费者一并验收」义务——console put_str 语义 = submit(FULL) 走同一提交路径，「无 damage 不提交」不变量 day-one 执法）；
5. latch/squash 最小语义（credits ≤2 执法 + pending 帧深 1 squash 折叠）；
6. gfxdemo 消费端（berry-gfx pass API 渲帧 → SUBMIT 往返）+ 拓扑降级门（非 EFI 链 guifbd 缺席 = `GFX absent` 静默——netd 双门先例）；
7. smoke-image EFI 相 `GFX ok` 标记（rootfs 相零扰动）。

**不做**（挂账 §7）：gui-wm 分发跳（GUI-5，段① §9-⑧）；virtio-gpu 后端（S1 spike 另刀）；PM 脉冲真落地（GUI-6+）；vote 仲裁策略（本刀只记账不仲裁——票面收下即回执）；稳态零唤醒 smoke 断言全形态（本刀先落「无未决请求 = 无主动 send」结构面，能耗代理断言挂 GUI-6）；多 surface/多客户端（C 形态退化 B 的单客户端面）。

## §1 立项依据（钦定源）

- ADR #50：B 形态三件事 = submit(buffer, damage K≤8+FULL) / frame_callback(未来时刻表+credits≤2) / release；HDI 三口 ramfb 第一实现；**「无 damage 不提交」不变量 + latch/squash（GUI-10 补充）+ per-interface version（段① GetinfoRep 已落 ✓）**。
- ADR #51：soft 后端永久 CI 基线——本刀 gfxdemo 用 berry-gfx soft 渲帧即该基线的首个设备面消费。
- ADR #53：三态停帧是验收项非优化项——本刀 IDLE 面（脏区空∧无未决 = 不发任何消息）结构先行。
- 规划 :87 GUI-1 行：serve 循环按 B 形态三件事定形（原案 GUI-1 顺延件——本刀兑现）。
- 规划 :88 GUI-2 行：消费侧 = host 离屏像素断言（已落）→ 设备上屏（本刀）。
- 段① 草案 §9-⑨（HDI 域消费者接线挂账）+ §9-⑧（gui-wm 分发 = GUI-5 不在本刀）。

## §2 拓扑与通道（粗案）

```
gfxdemo（rootfs 程序）                guifbd（EFI 链常驻）
  │ berry-gfx pass 渲帧 → VMO 写像素     │
  │ present 域 channel ──────────────→ │ wait_many 双路：
  │   SUBMIT(vmo_h, damage K≤8/FULL)    │   ① console pipe（gift3——既有文本路，
  │   FRAME_CALLBACK（阻塞应答）←────── │      语义升格 submit(FULL)）
  │   RELEASE(vmo_h) ←────────────────  │   ② present channel（新——SUBMIT 随行
  │ hdi 域 channel ─────────────────→   │      VMO handle：CHAN_HND_MAX 语义内）
  │   REG_VBLANK(event)/VBLANK_WAIT/    │ hdi 服务端：软 vsync（VBLANK_WAIT 到来
  │   PREPARE/COMMIT                    │      才 arm 一次性 wait——零唤醒纪律）
  └─────────────────────────────────────┴ ramfb（fbcon 之外整窗可作帧目标）
```

关键地形（scout 已核）：
- **guifbd 恒只在 EFI 链存活**（boot EFI-GOP 命中才授 fb 窗）→ **首帧上屏验证面 = smoke-image（EFI 相）**；rootfs 相 gfxdemo 走拓扑降级门（present 端点缺席 = `GFX absent`，rootfs 标记零扰动）。
- **wait_many 在册**（`NR 0x05` + `berry_object_wait_many` 包装，kernel syscall.rs:1893）——双路 serve 地基零内核改动。
- **boot 授能链不动**：fb 窗（H_FB 第 5 插）+ info VMO（H_FBINFO 第 6 插）既有；present/hdi channel 端点 = guifbd 起服务端 + 分发（§7-② 分发题）。
- ramfb 无翻页 → SUBMIT 处理 = 服务端读 VMO 按 damage 裁剪直拷 fb（CPU blit；damage 合并/裁剪界内）；FRAME_CALLBACK = 拷贝完成即应答（v1 无真实 vsync 点——软 vsync 时刻表 = clock 合成，§7-④）。
- **credits ≤2**：服务端在途帧计数执法（submit 拒超发——ST 码回执）；**squash** = pending 帧深 1，新帧覆盖旧 pending（保底一帧语义）；**latch** = FRAME_CALLBACK 应答点锁定（v1 退化形：应答即 latch）。

## §7 开放问题与挂账（设计待续——下一会话从这里接）

1. **present/hdi 端点分发题（§7-② 本刀最大设计点）**：gfxdemo（rootfs 普通程序）怎么拿 guifbd 的 channel 客户端端点？候选：a) svcmgr 中转 RPC（多一跳）；b) svcmgr 持服务端端点 + spawn 链 gift 下发（段② input 同款——但 gfxdemo 非预拉程序，gift 要进 bsh spawn 面）；c) 路径式命名空间挂端点（AGENTS 速览「路径式命名空间」——查 fsmgr/pipe 树有没有此机制，scout 未及）。**裁决待**。
2. **hdi 域 v1 裁剪面**：ramfb 直扫下 PREPARE/COMMIT 是否退化（槽位记录 + commit 即拷）还是完整三口——ADR #50「换实现不换契约」要求 wire 全实，服务端语义可退化但 op 面必须全应答。最小完整面 = 四 op 全应答（PREPARE 槽位记账 / COMMIT 关联 pending submit / VBLANK_WAIT 一次性 arm）。**设计待**。
3. **frame_callback 未来时刻表 v1 形态**：ADR #50「未来时刻表」——v1 ramfb 无真 vsync，回执时刻 = clock 合成（上次提交时刻 + 帧周期）还是完成即回？零唤醒纪律下定时器谁持有？候选：应答内嵌「next_target_ns」字段（wire 已有位？查 FrameDoneRep）而非服务端起定时器。**设计待**。
4. **console 路 FULL 升格的验收断言形**：put_str 走 submit(FULL) 语义后 FBSERVE ok 断言面是否需要加「console 帧 damage==FULL 计数」自证？smoke-image 现有 67 标记 +1（GFX ok）后的恰一次门。**设计待**。
5. **gfxdemo 渲染内容**：首帧内容定什么（蜂格图案 + 字形串？golden 对拍挂不挂——headless 黄金图是 SDK-1 面，本刀只断言往返标记 + 抽样像素？）。EFI 相断言 = 宿主侧 ramfb 像素可读吗（smoke-image 有无 fb 读回通道——现为 QEMU ramfb 无快照口，可能只断 guest 标记）。**设计待**。
6. **berry-gfx 设备面编译**：gfxdemo 是 musl 用户程序——berry-gfx 是 host 测 crate（no_std 兼容已核？段① 合同全库零 alloc no_std ✓）+ 用户态 target 链（user/bins/ 新 crate）。**实施核**。
7. **挂账承接**：PS_MAX=16 满帽（guifbd 常驻第 5？——查现 roster；若 gfxdemo 非常驻不占）；CHAN_HND_MAX=8 满帽（bsh 满配后 spawn gfxdemo 的 env 槽——段② §7-⑥ 同账，gfxdemo 走 bsh spawn 子进程 handle = 进程表面，非 env 槽——预期不撞，实施核）。
8. **gui-wm（GUI-5）分界**：本刀 gfxdemo 直连 present 域 = 单客户端退化面；GUI-5 gui-wm 进场后 gfxdemo 改连 wm——协议不变（present 域同 op），分发端点换手。记录防丢。

## §8 scout 结论存档（2026-11-01，免下会话重走）

- berry-guiproto lib.rs（2185 行）public 面全核：PresentReq（Submit.rects `[Rect; 帽]+len`）/ GetinfoRep（version_present/input/hdi 三域 + format_bits + max_credits ≤ CREDITS_MAX）/ FrameDoneRep / ReleaseRep / input 族 / hdi 常量（OP_REG_VBLANK 0x40..COMMIT 0x43，HDI_SLOTS_MAX=2）。三域 = 三对独立 channel（能力即权限）。
- guifbd main.rs（222 行）全核：gift2 = fbinfo VMO（40B 五域 u64 LE）、gift3 = console pipe；自验三步 → FBW ok → serve 循环（recv 256B / EAGAIN→wait READABLE 10s 重臂 / 非 EAGAIN trap / 首条 FBSERVE ok / put_str lossy）。
- wait_many：NR 0x05 在册 + berry_object_wait_many（berry-sys lib.rs:130）+ libc 已有消费先例（io.rs:176 stdin 并监）。
- guifbd 只在 EFI 链活（boot main.rs fb 授能块 EFI-GOP 门）；smoke-image 现标记 67（EFI 十段+接续+裸相 27+生态 27+RX+对拍）。
- GUI-2 段①②（main 098d01b/5317e09）：berry-gfx 3082 行 12 模块（caps/cmd/color/font×4/icon/lru/pass/queue/raster）——pass 形态 API + golden 回归 host 面全备，设备面零消费。
- ADR #50-54 五行全读；规划 :80/:87/:88/:102 四锚全读。
