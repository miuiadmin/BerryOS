// 上游 hexyl 0.17.0 的 src/tests.rs 未随搬运——本 crate 是 G-eco-d d-3c
// D4 路线①的生态链路判据件（判据语义与上游件清单见 Cargo.toml 头注），
// 非 host 测试面。main.rs 逐字搬运自带上文 #[cfg(test)] mod tests; 声明，
// 缺本文件则 cargo test 在本 crate 报 E0583 休眠红（日常构建 cfg(test)
// 不编译故不咬）。空模块仅消该红：零测试零行为面，main.rs 逐字性不动。
