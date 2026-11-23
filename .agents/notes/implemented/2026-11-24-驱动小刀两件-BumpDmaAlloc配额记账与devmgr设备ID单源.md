# 驱动小刀两件：BumpDmaAlloc 配额记账修正与 devmgr 设备 ID 改引 berry-virtio 单源

Status: implemented

## Problem

两件互不相干的小缺陷，同批落地。

其一，berry-hal 参考实现 `BumpDmaAlloc`（`user/berry-hal/src/lib.rs`）的耗尽门把对齐垫片算进了授额：`alloc_coherent` 拿「对齐后的池内偏移加 size」与 `cap` 比较，而堆基址非 64 对齐时首笔前的垫片（最多 63 字节）与每笔之间的垫片都落在这个偏移里。类型注释却承诺「授额 cap 全程可兑现」，[berry-hal 落地笔记](2026-11-20-berry-hal统一硬件抽象层crate.md)也以「多留 63 字节垫片按绝对地址对齐化同效」为由拒绝了 `Layout::align(64)`——实现没有兑现这两句话，物理长多出来的 63 字节成了永远碰不到的死区。基线代码在 host 上实测（同一进程实例）：`cap=100` 连续两笔 50，第二笔 Oom，`used()` 停在 82；`cap=100` 单笔 100 即 Oom；`cap=4096` 1 字节连分，64 笔后 Oom 而 `used()` 报 4033——交付 64 字节却记了 4033。`cap=4096` 连续 64 笔 64 在该实例兑满，在实现者报告的实例（堆基址残差 16）却第 64 笔即 Oom、只兑 4032：同一序列的结果随 malloc 给的基址漂移，正是缺陷本身。

其二，devmgr 驱动表（`user/bins/devmgr/src/lib.rs` 的 `DRIVER_TABLE`）六个 `device_id` 全是字面量 2、1、18、16、3、4，文档注释却写着「与 berry-virtio `mmio::DEVICE_ID_GPU` 等值钉板互证」——devmgr 根本不依赖 berry-virtio，所谓互证只是口径上的；berry-virtio 也没有 console 与 rng 两个常量。表与协议单源之间没有任何编译期或测试期的数值绑定，一侧改数另一侧不会发现。

## Decision

**BumpDmaAlloc：垫片不计配额，蓄水池物理长兜底。** 新增 `charged` 字段记已交付字节之和，`next` 降为纯物理游标。`alloc_coherent` 拆两门：配额门 `charged + size > cap` 即 Oom，在前；其后由纯函数 `plan(base, cursor, size, pool_len)` 按绝对地址推到下一 64 边界，并以物理长 `pool.len() = cap + 63` 执物理门。两门失败都零记账。`used()` 改回配额已计字节（与 `capacity()` 同尺），新增 `pool_used()` 暴露物理游标，用来分辨下一次 Oom 来自哪一门。由此得到的保证：对任意堆基址残差，单笔 `cap` 字节必成（基址垫片不超过 63，恰被物理长吸收）；size 全为 64 倍数时 Oom 当且仅当 Σsize 大于 cap。边界如实写进类型文档：size 非 64 倍数时分配间垫片累积可能先于配额把物理池吃尽（最少两笔即可触发），此时仍返回 Oom、不越界、不改已分配指针。

**devmgr 设备 ID 单源。** berry-virtio 的 `mmio` 模块新增 `DEVICE_ID_CONSOLE: u32 = 3` 与 `DEVICE_ID_RNG: u32 = 4`（规范 §5.3 Console Device 与 §5.4 Entropy Device；uapi `virtio_ids.h` 的 `VIRTIO_ID_CONSOLE` 与 `VIRTIO_ID_RNG` 同值），落位在 `DEVICE_ID_INPUT` 之后、跨 crate 别名的 `DEVICE_ID_GPU` 之前。devmgr 的 Cargo.toml 加 `berry-virtio` 路径依赖——`user/bins` 独立小工作区指向 `drivers/`，guifbd 已有同款先例；放在普通依赖而非 linux 门控下，因为 host `cargo test --lib` 的 lib 面就要用。`DRIVER_TABLE` 六行全部改引常量；测试的查表键故意保留规范字面值——表改引常量后，测试就是跨 crate 的等值钉板。berry-virtio 的 `tests/layout.rs` 给两常量加数值断言，并把设备 ID 族六员做两两互异检查。

## Alternatives considered

BumpDmaAlloc 保留实现只改文档（弃——cap 可兑现量随堆基址残差漂移，同一序列不同进程结果不同，「同效」承诺仍是假的，63 字节仍是死区）；每笔按 `ceil(size/64)×64` 计配额（弃——cap 非 64 倍数时单笔 cap 反而失败，cap=100 只能分出 64，违背「单笔 cap 必成」）。devmgr 只把 console 与 rng 两行改引常量（任务前提以为其余四行已是单源，核对后六行全是字面量，故六行全改）；依赖放在 linux 门控下（弃——lib 面在 host 就要编）。

## Consequences

berry-hal：lib 单测 8（基线 0）加 `tests/traits.rs` 集成 13（基线 8），零 warning；berry-driver 回归 7 测全绿（它不消费 BumpDmaAlloc，全仓只有 berry-hal 自身引用）。回归有效性经总审重跑确认：新 `traits.rs` 对基线门逻辑实跑 13 例中 7 例红，对修正版全绿。修正版在同一 host 实例的对照：两笔 50 皆成且 `used()` 为 100，单笔 100 成，64 笔 64 恰兑满而第 65 笔 Oom，1 字节连分 65 笔后由物理门拒而 `used()` 为 65、`pool_used()` 为 4097。`BumpDmaAlloc` 目前零生产消费者，影响面限于 host 测试挂具与将来换真机实现时的契约基准。

devmgr：host `cargo test --lib` 11 测全绿；`cargo check --locked --release --target aarch64-unknown-linux-musl` exit 0——Cargo.lock 与 Cargo.toml 同批入库，锁增量两个包（`berry-gpuproto` 与 `berry-virtio`）；berry-virtio 12 加 14 测全绿。突变检验（复审者独立实跑）：把 `DEVICE_ID_CONSOLE` 改成 5，berry-virtio 的 `mmio_table_invariants` 与 devmgr 的 `driver_table_bindings` 同时红，两侧都钉得住。合并端到端正常机（总审 run `1795455217-815507000-28401-host`）devmgr 行照常：`devmgr: 3 devices: [slot 29] input → gui-inputd ✓, [slot 30] input → gui-inputd ✓, [slot 31] blk → vblk ✓`。

代价：devmgr 从此多一条跨工作区路径依赖，`user/bins` 指向 `drivers/` 的方向与 guifbd 一致；berry-virtio 的 `DEVICE_ID_GPU` 仍是跨 crate 引 berry-gpuproto 的别名，本次没有动它。

## Related

- [berry-hal：统一硬件抽象层 crate](2026-11-20-berry-hal统一硬件抽象层crate.md)——`BumpDmaAlloc` 的家与被修正的承诺出处。
- [devmgr v1：MMIO 槽枚举 + 驱动匹配 + 服务状态报告](2026-11-20-devmgr-v1设备枚举与服务状态报告.md)——驱动表本体。
- [波 2 收尾三小刀：virtio_example 接 HAL + 分区 display() + 驱动表补全](2026-11-20-波2收尾三小刀-hal接线-分区display-驱动表补全.md)——console 与 rng 先行绑定入表的那一刀。
- [驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md)——§3.4 健壮性审计后续。
