# berry-fsclient

Client side of the [BerryOS](https://github.com/miuiadmin/BerryOS)
filesystem-service RPC (experimental, aarch64-first research OS):
session core (strict-serial, poisoning, EBUSY-retry) plus pure
fd-table/cwd/stat/dirent faces used by the berry-libc file layer.

**Status: experimental (0.1.0).** The API is unstable and tracks the
BerryOS v1 wire format. The syscall-touching session module is
cfg(not(test)) — host tests cover the pure faces.

BerryOS userland SDK stack (this crate in bold):

    berry-sys             <- raw syscall ABI
    berry-tls             <- TLS placement pure functions
    berry-netdproto       <- network-service RPC wire protocol
    berry-fsproto         <- filesystem-service RPC wire protocol
    berry-fsclient        <- fs RPC session + fd-table faces (this crate)
    berry-libc            <- libc shim for Rust std (musl target)

License: Apache-2.0. Design docs live in the repository (`docs/`,
mostly zh-CN).
