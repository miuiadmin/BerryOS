# {app_id} —— BerryOS 应用

脚手架由 `mkapp.py --new` 产出（SDK-1a）；契约详见 `docs/应用开发指南.md`。

## 构建（咒语参数勿动——justfile build-user 同链）

本工程须位于 `<ROOT>/user/bins/{slug}/`（Cargo.toml 的 berry-libc 相对路径
按此位置钉死 `../../berry-libc`；脚手架到别处 = 手改该 path）。

```bash
ROOT=$(cd ../../.. && pwd)   # <ROOT>/user/bins/{slug} → checkout 根
cd "$ROOT/user"
CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
    cargo build --release --manifest-path bins/{slug}/Cargo.toml \
    --target aarch64-unknown-linux-musl
```

参数逐项为什么：见指南 §4（musl target + berry-ld 剥 musl crt + 4K
max-page）。产物：`user/cargo-target/aarch64-unknown-linux-musl/release/{slug}`。

## 打包四步

1. 组 staging：`mkdir -p /tmp/{slug}-stage/bin`；
2. 放件：`cp manifest.toml /tmp/{slug}-stage/` 且
   `cp <产物> /tmp/{slug}-stage/bin/`（bin/ 恰一个文件，名 = entry 去
   `bin/` 前缀；SHA256SUMS 是 mkapp 自动生成的，**不要手放**）；
   **GUI-7 可选**：manifest 声明 `[ui]` icon/views 后，staging 放
   `icons/<名>.png` 源（mkapp 编码为 `icons/<名>.byi` 入包——源不入包）
   与 `views/<名>` 文件（原样入包）；未声明而放置会被交叉门拒；
3. 打包：`python3 $ROOT/tools/mkapp.py /tmp/{app_id}.bap /tmp/{slug}-stage`；
4. 复验：`python3 $ROOT/tools/mkapp.py --check /tmp/{app_id}.bap`。

## 契约三件（速览）

- **argv0**：spawn 名 = manifest `entry`（`bin/{slug}`）——main 里断言钉着；
- **EOF 先行**：退出走 `ExitCode::from(0)` 真退出（libc exit 族先发零长 EOF）；
- **退出码**：appd reap 收 0 才计入 `APP ok` 谓词；panic → abort(134) → loud。

## in-tree 接线（进 rootfs）

进 rootfs.cpio 需 justfile build-user 挂账位（SDK-1a §10 O7——脚手架刀不
接线）：构建产物 + staging 四步后，把 `{app_id}.bap` 以 `app/{app_id}.bap`
条目加进 build-user 的 mkcpio 调用（appdemo/appquiet 同款两行）。
