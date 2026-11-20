[package]
name = "{slug}"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
berry-libc = {{ path = "../../berry-libc" }}

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
