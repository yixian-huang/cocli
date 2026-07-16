# ADR 0001: 分阶段对齐 cloud driver-core

- 状态：Accepted
- 日期：2026-07-16

## 决策

local 的长期 driver 契约对齐 `cocli-cloud/daemon-rs/crates/cocli-driver-core`，不继续扩展当前分叉的 `cocli-driver::{Driver, DriverProcess}`。
迁移采用 port/adapt：先稳定共享 core，再切换 Claude、Cursor、Codex、Gemini driver、runtime registry 与 agent actor，最后删除旧 trait。
M0 的 runtime 交付范围是 Claude、Cursor、Codex、Gemini；四者必须共享 runtime-neutral event、spawn、workspace、interrupt 和 turn lifecycle 契约。
cloud 的远端 server、machine key、multi-slot 与 SaaS 配置不进入 core。
local 保持 Rust 1.78，移植代码必须降级或替换不兼容依赖/API。
bridge 配置与 workspace 准备由 driver 边界负责，agent actor 只负责编排生命周期和事件。
迁移期允许短暂 adapter，但不保留双接口作为长期公共 API。
每个 runtime 必须有 parser、spawn argv、workspace 和 lifecycle contract fixtures；共享 registry matrix 与 `cargo test --workspace` 是合并门禁。
Claude 是第一条 contract 校准线，但不是唯一交付项；四个 runtime 在同一 P0 完成真实本机 E2E。

## 原因

cloud 契约已经覆盖能力声明、workspace 准备、stdio 编解码、退出分类与可选进程控制；复用该形状比维护两套相似但不兼容的 trait 成本更低。

## 后果

短期会增加共享 core 和四个 adapter crates；完成后 local 成为共享 runtime 上游，同时仍保持 SQLite、单机部署边界。非 Claude runtime 不再推迟到 M1。
