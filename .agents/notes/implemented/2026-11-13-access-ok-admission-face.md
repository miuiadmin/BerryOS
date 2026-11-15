# Agent Note: access_ok 用户缓冲准入判定律纯面外迁 berry-aokface（W3-Q70）

Status: implemented

## Problem

kernel/ syscall.rs 的 access_ok 族（access_ok/access_ok_window）是全部带用户指针 syscall 的第一道门——-EFAULT 折射面的语义单源。判定律此前只活在 kernel crate（host 测不可达）：错一门 = 要么放过野指针（安全洞），要么误拒合法缓冲（功能红）。

## Decision

- 判定律抽成 berry-aokface 纯面（face crate 先例族第 23 件，safeck/chanface 纪律同款）：EL0 值域四门（len==0 恒过/checked_add 回卷拒/null 页 0x1000 下界/USER_VA_END=2^39 上界）+ EL1 高半单比较 + 回卷拒 + 叶 AP 位域读写拒律（写需 AP∈{00,01}、读需 AP≠11）+ span_pages 页序 + window_should_walk 同页跳过律（SAME_PAGE_NONE=u64::MAX 哨兵首块必查/缓存只记最近页/回访已查页仍须查——保守正确）。
- 常量 NULL_PAGE_BASE/USER_VA_END/PAGE_SIZE/SAME_PAGE_NONE 单源迁 crate，kernel re-export 对齐（消费点引用不变）。
- 留壳（物理臂不外迁）：walk_leaf TTBR0 三级走查 + PT 锁窗（§A113 walk+store 同窗铁律——last_page 生命周期 = 锁窗，禁提升 static）+ copy_from_user_locked 分块窗。

## 保形迁移判例（本轮自审抓住的语义漂移）

原 access_ok 的 checked_add 回卷门在 EL1/EL0 分派**之前**——两路都拒回卷。首版迁移把回卷门折进 el0_span_ok、EL1 侧只留 ptr>=KVB 单比较：EL1 高半顶端（ptr 接近 u64::MAX）回卷跨度从拒变放 = 语义漂移。修法三件：el1_span_ok 自带回卷门 + kernel 侧保留前置共门（幂等零漂移）+ host 钉板 el1_wraparound_rejected 专测锚定（漂移再犯即红）。教训：迁移判序面时先画原判序的门位图——共门折进单臂即漂移。

## Verification

- host 17 测全绿：四门全表恰界/AP 4×2 全表 + 位域外位不干扰/页序五形/哨兵-同页-回访三态/256B 块流对拍（走查次数 = 页数 2 非块数 5——同页跳过收益面）。
- test-host 163 组 1524 全绿 EXIT=0（隔离 CARGO_TARGET_DIR 前台实跑；base 13b33e946 系 1507 + aokface +17）。
- kernel no_std 本件零新增 error（E0425×9 = base 债 #247 在途同款——ktest.rs/pi.rs 面）。
- docs-guard PASS/abi-check PASS/check-docs 201 篇 0 警/台账 987 对拍 ok/wg --check 78 条 + --selftest 0 面红。
- 随刀债修：wg 两行（KEY_HEADS 登记 + SLICE_HZ_OVR 组标签 arch→scheduler，#258 同款）+ sleepq 笔记补落（base 第 13 次重写剥出）。
