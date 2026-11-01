# berry-libc

Full-definition libc shim for the [BerryOS](https://github.com/berryos/berryos)
microkernel (experimental, aarch64-first research OS): lets Rust std
build for `aarch64-unknown-linux-musl` on BerryOS (malloc/write/
pthread/futex/crt0 real, the rest stubbed loudly).

**Status: experimental (0.1.0).** The API is unstable and tracks the
BerryOS v1 userland ABI. aarch64 only; pairs with the in-repo `berry-ld`
linker wrapper — see the repository's app guide.

BerryOS userland SDK stack (this crate in bold):

    berry-sys             <- raw syscall ABI
    berry-tls             <- TLS placement pure functions
    berry-netdproto       <- network-service RPC wire protocol
    berry-fsproto         <- filesystem-service RPC wire protocol
    berry-fsclient        <- fs RPC session + fd-table faces
    berry-libc            <- libc shim for Rust std (musl target) (this crate)

License: Apache-2.0. Design docs live in the repository (`docs/`,
mostly zh-CN).
