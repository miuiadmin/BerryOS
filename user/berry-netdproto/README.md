# berry-netdproto

Wire protocol for the [BerryOS](https://github.com/berryos/berryos)
network service RPC (experimental, aarch64-first research OS): strict
request/reply byte layouts for DNS and TCP round-trip ops — pure no_std
encoder/parser, host-testable.

**Status: experimental (0.1.0).** The API is unstable and tracks the
BerryOS v1 wire format. Protocol-only: channel/wait semantics live at
each end.

BerryOS userland SDK stack (this crate in bold):

    berry-sys             <- raw syscall ABI
    berry-tls             <- TLS placement pure functions
    berry-netdproto       <- network-service RPC wire protocol (this crate)
    berry-fsproto         <- filesystem-service RPC wire protocol
    berry-fsclient        <- fs RPC session + fd-table faces
    berry-libc            <- libc shim for Rust std (musl target)

License: Apache-2.0. Design docs live in the repository (`docs/`,
mostly zh-CN).
