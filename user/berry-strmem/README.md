# berry-strmem —— mem/str 族字节序语义单源（纯函数面）

BerryOS libc 垫片的 `mem`/`str` 族语义单源，以纯函数外迁（先例族第 7 例：
berry-tls / berry-mmap / berry-signal / berry-winplan / berry-argv /
berry-spawngeo 同款「纯面外迁」纪律）：

- **volatile 拷/填/搬**：`memcpy_sem` / `memset_sem` / `memmove_sem`——
  朴素 Rust 循环会被 LLVM 惯用形识别升格回对 libc 同名符号的自递归调用，
  逐字节/逐 u64 `read_volatile`/`write_volatile` 不可识别（法证教训一级
  防御）；双 8 对齐时启 u64 快路，`memmove_sem` 重叠区反向拷贝。
- **字节序比较/扫描**：`memcmp_sem` / `bcmp_sem` / `strlen_sem` /
  `strnlen_sem` / `strcmp_sem` / `strchr_sem` / `strrchr_sem`。
- **strncpy pad-to-n 律** / **atoi 双饱和**（负向 `i32::MIN`、正向
  `i32::MAX`，非 UB 非回绕）。

消费端 = berry-libc `strmem.rs` 薄 `#[no_mangle]` C-ABI 壳（对外符号面
零漂移，「不长第二套」）；本 crate 零依赖、零 syscall 面，`cfg_attr(not(test),
no_std)`，任意 host 可单测（`tests/semantics.rs` 15 测，justfile test-host
接线）。

外迁动因：berry-libc 结构性不可 `cargo test`（crt0 `global_asm!` 的
ADRP/`:lo12:` 是 ELF 目标写法，darwin 汇编器拒非 GOT 形式），strmem 面
原先零测试——外迁后语义单源在本 crate，host 测试钉板。
