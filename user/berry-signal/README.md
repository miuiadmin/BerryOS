# berry-signal

BerryOS POSIX 信号面纯函数底座（A7-4 协同式投递）。

## 是什么

信号语义（sigaction 表 / 掩码 / 默认动作 / 投递分诊）的纯函数核——
内核只管「跨进程位置位」（ABI 0x86 `signal_send` / 0x87
`signal_take`），投递执行全在 berry-libc 用户态。本 crate 承载语义核：

- `ActionTable`——sigaction 三值编码表（0=DFL / 1=IGN / ≥2=fn 地址）
- `apply_mask`——sigprocmask 三 how 状态机
- `set_*` 族——musl 128B `sigset_t` 位图层（宽进严出）
- `default_action`——默认动作表（v1 无停止类，偏差文档化）
- `plan_dispatch`——归并式分诊：`pending × mask × actions` → 升序
  零副作用步列表（libc 只执行：Hold 回 held / Call 就地调 /
  Terminate `exit(128+sig)`）

零依赖、no_std core、任意 host 可跑单元测试。消费者 =
berry-libc `signal.rs`。

设计文档：`docs/事件循环A7-4信号面实施设计草案.md`（仓内 zh-CN）。

License: Apache-2.0
