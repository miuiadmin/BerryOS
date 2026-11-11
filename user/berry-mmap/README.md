# berry-mmap

Anonymous-`mmap` bookkeeping pure functions for the
[BerryOS](https://github.com/miuiadmin/BerryOS) microkernel (experimental,
aarch64-first research OS): a fixed-capacity region table, a VA-window
first-fit allocator (bottom-up, munmap hole reuse), and page-rounding
arithmetic — pure functions, host-testable.

**Status: experimental (0.1.0).** The API is unstable and tracks the
BerryOS v1 userland ABI. no_std core; builds and unit-tests on any host
(no arch-specific code).

BerryOS userland SDK stack (this crate in bold):

    berry-sys             <- raw syscall ABI
    berry-tls             <- TLS placement pure functions
    berry-netdproto       <- network-service RPC wire protocol
    berry-fsproto         <- filesystem-service RPC wire protocol
    **berry-mmap**        <- anonymous-mmap bookkeeping pure functions (this crate)
    berry-fsclient        <- fs RPC session + fd-table faces
    berry-libc            <- libc shim for Rust std (musl target)

License: Apache-2.0. Design docs live in the repository (`docs/`,
mostly zh-CN).
