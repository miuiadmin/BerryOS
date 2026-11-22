# Agent Note: pager 表批窗装配纯面（W3-Q73 dpgface tblwin）

Status: implemented

## Problem

`sys_pager_supply_mapped_table`（0x24，X-6 刀① 表批）的入参装配段——n ∈ [1,32] 门、`PagerWindow` 16B 小端拼装（dv u32/sv u32/delta i64）、delta 页对齐预检——只活在 `kernel/src/syscall.rs` handler 内联里。kernel crate host 不可达（依赖 HAL global_asm），三面判定律无钉板：① n 门「0 拒」与 Q33 policy 表「空表合法」语义相反，抄错口径无测试可红；② `% PAGE_SIZE` 对 i64 负 delta 的 Rust 同号余语义（-4096 放行/-1 拒/i64::MIN 恰对齐）是隐式知识；③ 拷入→预检→门控的判序（EFAULT 透传 → delta 拒 → -EBADF）是 X-6 §2.2 语义裁定的投影，重排即改变可观测错误码序。

## Decision

扩既有 berry-dpgface（W3-Q24 槽池判定律同源延续，零新 crate/members 不动）加 `tblwin` 模块：`n_gate`（Empty/TooBig 两态，0 拒钉板）/`decode_window`（16B 小端拼装 + delta 预检）/`decode_table`（整表单源——N 门先于逐窗、首错即停带 idx）/`window_to_bytes`（往返指纹，svcmgr 用户态双镜像对拍锚）。kernel handler 壳消费同一函数（staging-预检段逐字等价替换；`copy_from_user` 电压/门控/retain/收货环留壳），`PAGER_TABLE_MAX` 四源镜像 const assert 执法（本帽 = dpgface = `EREQ_SLOTS` = svcmgr `PAGER_SLOTS`）。host 11 测：n 门恰界与高 32 位脏值全宽读、LE 逐字节、负 delta 域与 i64::MIN 端点、判序首错位置推进、尾部零窗、满装 32 窗 LCG 往返、编码-解码全往返指纹。

## Alternatives considered

新 crate berry-tblwinface——否决：pager 窗表与 dpgface 主面（0x24 收货环消费的 `el0_take_for` 槽池）同域，Q33/Q34 先例（解码面扩既有 crate）更贴。kernel 侧 `PagerWindow` 换用 dpgface 结构体——否决：repr(C) ABI 面与 ktest 直造留在 kernel，face 侧纯值域投影，避免 ABI 面跨 crate 漂移。

## Consequences

0x24 装配判定律有 host 钉板（改语义不改测试必红）；错误码序（EFAULT→EINVAL→EBADF）冻结进测试判序面。随刀三件（#258/#259/#260/#261 同款 base 债）：wg 两行（KEY_HEADS/SLICE_HZ_OVR 组标签）+ sleepq 笔记补落 + `.gitignore /.target-*/` 补回——base 重写剥落后 test-host 的 wg selftest 必红。

Related: [2026-11-13-timer-slack-face.md](2026-11-13-timer-slack-face.md)（同系 face crate 先例；paging descriptor face = #260 在途）；X-6 批供提速设计 §2.1/§2.2（语义裁定原文）。
