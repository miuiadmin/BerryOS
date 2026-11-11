# 公开镜像说明

本仓库是 BerryOS 开发主仓的白名单镜像（单向导出，fresh 历史）。
部分中文设计文档（阶段沿革 / 复盘 / 审查报告 / 战略裁决 / 收官盘点 /
规则 / 架构决策 ADR 全文 / 雷册判例库）属于内部过程记录，仅存于开发
主仓，不在此镜像中——`docs/索引.md` 中对应行的链接在本镜像内不可达。
核心设计文档全部在册。
CI 形态差异：内部 CI 双道（ci.yml/ci-light.yml）依赖内部 runner 设施，不入
本镜像；公开面保留 sdk-publish.yml（crates.io 发布链）。构建入口 `just build`
/ `just smoke` 与上游同源（需 QEMU 与 Rust 工具链，见 README）。
