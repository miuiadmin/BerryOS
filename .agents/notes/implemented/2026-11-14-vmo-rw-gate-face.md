# W3-Q78 vmo 读写窗口门：copy_gate 判序 + walk_copy 推进律单源

Status: implemented

## 背景与动机

`kernel/src/vmo.rs` 的 `read`/`write` 两函数各自内联同一套判序：mmio 拒（`NotSupported`）→ `check_bounds`（`OutOfRange`）→ `ensure_committed`（`NotCommitted`），再各自维护 `pos` 推进环逐段 `page_span` 拷贝。三道门原语已单源（W3-Q3 `berry-vmoledger`），但**组合判序**与**推进环**在两处重复——判序漂移风险：任一处换序（如 bounds 先于 mmio）会把 MMIO 设备窗口的 `-ENOTSUP` 折成越界/未提交误码。

## 交付面

- `berry_vmoledger::copy_gate(led, offset, len)`：三道门判序单源。纯判定零副作用。
- `berry_vmoledger::walk_copy(led, offset, buf_len, copy)`：分段推进执行器。每步 `(pa, off, n)`，`n` = 页内剩余与窗尾取小；环恰在窗尾收敛（§A136 同律）。前置 = copy_gate 已过（不重查——先查后拷，不留半截副作用）。
- `kernel/src/vmo.rs` read/write 壳化：done 游标 + 拷贝闭包注入；direct map `copy_nonoverlapping` unsafe 臂留壳。

## 测试面（host +8，vmoledger 13→21）

判序钉板（mmio+越界+未提交并存时 mmio 门最前）/各门穿透独立钉位/整页连续段/头尾跨页切分（首段页内剩余、尾段恰残量）/零长零调用/单字节三形/mmio 全窗口形 NotSupported/敌意窗长全域扫描（段和恰等 + n 域律 + 末段恰收窗尾）。

## 随刀债修

writable_globals 四行（SLICE_HZ_OVR arch→scheduler、HT_DRAINQ×3 scheduler→other——live 对拍红）+ sleepq 笔记补落 + .gitignore /.target-*/。

## Related

- [2026-11-13-p22-exit-async-ht-drain.md](2026-11-13-p22-exit-async-ht-drain.md)（HT_DRAINQ 组标签判例来源）
- 沿革 [阶段沿革.md](../../../docs/阶段沿革.md)（二百五十七）
