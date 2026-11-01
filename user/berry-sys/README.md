# berry-sys

Raw syscall bindings for the [BerryOS](https://github.com/berryos/berryos)
microkernel (experimental, aarch64-first research OS): `#[repr(C)]` ABI
types, generated constants, and `svc #0` wrappers. No allocation, never
panics.

**Status: experimental (0.1.0).** The API is unstable and tracks the
BerryOS v1 syscall ABI. Builds for aarch64 targets only
(`aarch64-unknown-none` / `aarch64-unknown-linux-musl`); useful only on
BerryOS or its QEMU image.

BerryOS userland SDK stack (this crate in bold):

    berry-sys             <- raw syscall ABI (this crate)
    berry-tls             <- TLS placement pure functions
    berry-netdproto       <- network-service RPC wire protocol
    berry-fsproto         <- filesystem-service RPC wire protocol
    berry-fsclient        <- fs RPC session + fd-table faces
    berry-libc            <- libc shim for Rust std (musl target)

License: Apache-2.0. Design docs live in the repository (`docs/`,
mostly zh-CN).
