# Agent Note: W3-Q106 clock_read MONOTONIC vDSO 化（03 号分派审计机会三）实施记录

Status: implemented

## 背景与动机

[03 号 syscall 分派审计](../../../research/内核深度进化/03-syscall分派审计.md) 机会三实锤：`sys_clock_read` MONOTONIC 分支只是 CNTVCT/CNTFRQ 两次 MRS + 纯算术 + copy_out，而 EL0VCTEN 硬件门早已全核打开（timer.rs B-1 为 bench1 开、smp.rs 副核 bring-up 同开）——一次完整 SVC 包络（入口 19 存 + KERN_LOCK 全对 + 尾三查 + 恢复 19 读）换两条本可在 EL0 执行的指令，审计判词「整号消灭」。

## 改动面

单源 crate `user/berry-vdsoface`（no_std 纯函数，host 5 测；先例 berry-fbcon 跨工作区 path 依赖）：

- 页布局冻结 ABI（v1）：`+0x00` magic "BRVDSO10" / `+0x08` freq 快照 / `+0x10` seq（恒 0，v2 REALTIME seqlock 预留）；布局常量钉板测。
- `ticks_to_ns` 双除式（先商后余防中间溢出）与内核 `sys_clock_read` 逐位同式，host 对拍测含非二次幂频率精确域。
- `page_ready` 页头谓词（magic+freq 非 0）。

内核半 `kernel/src/vdso.rs`：

- boot 期（timer::init 后、首 AS 前）建全局单页 VMO：magic + CNTFRQ 快照 + seq=0。内容 boot 后恒定 → 无 seqlock 面；失败 expect 炸响（boot 可信语境）。
- `map_into` 双装配路径接线：`UserProcess::build`（user.rs）与 `sys_process_create`（syscall.rs）——每个用户 AS 固定 `VDSO_VA=0x4800_0000`（全带扫描选址，避 ktest rogue_ptr 0x4000_0000）只读映射；共享单 VMO 权衡裁决见模块头注（per-AS 拷贝零语义增益）。
- `Perm::UserRo`（AP=11）新权限档——paging.rs 原「留 v2 按需」兑现，vDSO 首个消费者。

用户态 fast path `user/berry-sys`：

- `berry_clock_read` MONOTONIC 臂：读页头 16B（volatile）→ `mrs cntvct_el0` → 同式换算，整号不陷 SVC；页未就绪或 REALTIME → 回落原 syscall（0x61 语义不变，ABI add-only）。
- 一体构建契约（crate 与内核同 image 组装）注记在 berry_clock_read 文档块。

配套：just test-host 接线 +5、wg 台账 +VDSO_VMO、unsafe 台账随源重产 1178 对拍 ok、07 头条 2903→2908〔268 组〕。

## 验证

- test-host 268 组 **2908/0 全绿 EXIT=0**（隔离 .target-host-w3q106）。
- `just build` exit 0（本件动 user/ 与 kernel 链接面——四门不含目标机构建，#217 判例补跑）+ `just build-user` exit 0（tlsscan 61 ELF 全过）。
- 五门禁绿：docs-guard 0 警（G10 126=126）/ abi-check 41 号 / wg 91 / check-docs --strict 441 篇 0 告 / 台账 --check 1178 对拍 ok（9.39%/10.49%）。
- 本地 QEMU smoke：boot 过 vDSO 装配点（每 AS 新映射面——R132 双盲判例要求 QEMU 实跑）。

## 遗留与移交

- REALTIME epoch + seqlock 更新点 = v2 扩展（页布局 add-only 预留 seq 域）。
- libc 高层消费（事件循环/动画类）走 berry_clock_read 自然受益，无逐点改造义务。
- Q105 撞号避让：#498 已占用 W3-Q105（G1.5 悬账收口），本件改号 W3-Q106。

## 追刀：发布面守卫十一连扩容（run 44360602853 红根）

CI smoke `sdk-publish-guard` 硬门 2b 拦截 `berry-sys/Cargo.toml:25 berry-vdsoface = { path = … }` path-only 线（本件引入）。修法三面：

- manifest 双写形：`berry-vdsoface = { path = "../berry-vdsoface", version = "0.2.0" }`（版本段随 workspace 单源 0.2.0）。
- vdsoface 正式入发布族（第 11 crate）：Cargo.toml 补 description/repository/readme 三键 + README.md 本体（strmem 同款纯函数面文风）；硬门 1 清单 / path-only 反扫清单 / pc 定值（=0）扩至十一 manifest；档② 完整 package 清单 berry-sys 移出（有内部依赖后 verification build 须 registry 可解析——fsclient B6 判例同款降 --list）、vdsoface 入；档③ --list 增 berry-sys。
- 同步律全链：dw 正向线 +1（sys→vdsoface）；sdk-publish.yml 依赖序九连（vdsoface 首）；应用开发指南 §9/§9.1 十一连口径 + publish 序 + 名占预检清单（顺带清偿 base 滞后：文档八连口径未含 lifecycle/strmem 两 crate）。

本地自证：守卫六门全绿（GATE1/2-dw/2b/pc）+ `cargo package -p berry-vdsoface` verification build 过（仅 B2 已知噪声）+ `cargo package --list -p berry-sys` 干净。
