# cocli local 落地记录

更新日期：2026-07-16

本文记录从 `~/code/cocli-cloud/daemon-rs` 向 local 仓的选择性 port、刻意删除项与 P0 缺口。cloud 仓只作为只读参考，不存在自动同步。

## 第一轮差距表

| 领域 | local 现状 | cloud daemon-rs 参考 | P0 处理 |
|---|---|---|---|
| 进程模型 | `bin/cocli` 仅打印版本 stub | 远端 server + WS daemon + agent 子进程 | 组装单机 HTTP server + in-process runtime，不 port multi-machine slot |
| 持久化 | `cocli-store` 是 placeholder | SaaS server 使用 Postgres | 新建 SQLite migration 与 repository；只保留 agents/channels/messages/sessions 最小集 |
| HTTP/API | `cocli-api`、`cocli-server`、`cocli-ws` 是 placeholder | Go server 承担 REST/WS/队列 | 用 Axum 实现 local API、事件广播和 runtime 调度 |
| Agent runtime | 已有 start/deliver/claude stream parse/activity/turn-end 骨架，约 3k LOC | 约 17k LOC，含恢复、watchdog、skills、多 runtime、指标 | P0 保留最小竖切；稳定性模块按需求选择性 port |
| Deliver | 原实现用 `send().await`，mailbox 满会阻塞，关闭会丢；启动队列无上限/去重 | 有界队列、retry 去重、`try_send`、flush/rebuffer | 第一轮已 port 有界接管语义；SQLite durable retry 后续补 |
| Driver contract | `cocli-driver::{Driver, DriverProcess}`，与 cloud 分叉 | `cocli-driver-core::Driver` + optional sub-traits | 按 ADR 0001 分阶段新增 core、切 claude/registry/actor、删除旧 trait |
| Claude driver | spawn/stream parser 可用；MCP config 主要由 actor 写；workspace prepare 是 no-op | driver 负责 permissions、MCP config、spawn/parse/encode | 随 core cutover 把 `.claude/settings.local.json` 与 MCP config 下沉到 driver |
| Protocol | 已有 start/deliver/activity/turn/session 等 Phase 0 类型 | 协议更完整，覆盖 bridge/recovery/skills/steer 等 | local 内部调用优先；只 port P0 事件，不复制 cloud 多租户 wire |
| Bridge | `cocli-bridge-config` 只写 claude MCP config，无 bridge binary | 完整 MCP tools + daemon HTTP/WS proxy | P0 实现 local loopback bridge 最小 send/check；不 port远端 proxy/auth |
| Connection | 无 `cocli-conn` | 自动重连远端 Go server | P0 不新增 conn；server 与 runtime 同进程，用 channel/trait 边界 |
| Web | React 工程存在，Rust embed crate 是 placeholder | 完整 Slack-like UI | HTTP 竖切后接最小 channel/message 页面 |
| Runtime 范围 | 只有 Claude crate，agent core 已预留多 runtime 能力 | Claude、Cursor、Codex、Gemini 生产实现 | 四个 runtime 同属 P0；共享 contract，分 adapter 验收 |

## P0 任务分解

每个工作包都小于一周，可独立验证。

| 工作包 | 估算 | 交付与门禁 |
|---|---:|---|
| P0-1 Deliver 接管语义 | 0.5–1 天 | 有界队列、retry 去重、mailbox full/closed rebuffer；`cargo test -p cocli-agent` |
| P0-2 Driver core cutover | 2–4 天 | 新 `cocli-driver-core`、registry/actor 切换、旧 trait 删除；共享 contract tests |
| P0-2a Four runtime adapters | 3–5 天 | Claude、Cursor、Codex、Gemini parser/spawn/workspace/lifecycle fixtures 与 registry matrix |
| P0-3 SQLite 最小模型 | 2–3 天 | migrations + agents/channels/messages/sessions repository；内存 SQLite 测试 |
| P0-4 Local runtime service | 2–4 天 | store message → dispatch deliver → persist activity/turn；fake driver integration test |
| P0-5 HTTP server 竖切 | 2–3 天 | `cargo run --bin cocli` 启动；health/create channel/post/list message API；curl smoke |
| P0-6 Local bridge + four-runtime E2E | 3–5 天 | 最小 send/check MCP 工具；四个 CLI 各收一条并回复；登录态和版本写入 runbook |
| P0-7 最小 Web UI | 2–3 天 | channel 列表、消息流、发送框；前端 lint/typecheck/test |
| P0-8 打包与文档 | 1–2 天 | XDG 数据路径、README how-to-run、全 workspace test/clippy、手工 E2E 记录 |

## 已 port

### 2026-07-16：deliver queue 第一切片

来源形状：

- `daemon-rs/crates/cocli-agent/src/queue.rs`
- `daemon-rs/crates/cocli-agent/src/router.rs`

适配到 local：

- 同一 `(channel_id, seq)` 的重试更新原队列项，不重复放大。
- 队列上限设为 64，与 local actor mailbox 容量一致。
- router 使用 `try_send`，mailbox 满或关闭时 rebuffer，关闭时移除 stale sender。
- 中断的 flush 保持原顺序放回队首。

### 2026-07-16：production driver-core 契约

来源：

- `cocli-cloud/daemon-rs/crates/cocli-driver-core/`
- production 参考 commit：`8d590a13`

落地：

- 新增独立 `cocli-driver-core` crate；现有 `cocli-driver` 暂时保留，等待 adapter/agent 原子切换。
- Port object-safe `Driver`、六个 optional sub-traits、runtime-neutral `DriverEvent`、spawn/config types 与 turn-exit helpers。
- 保持 core 不依赖 protocol、conn、tenant、store 或 cloud SaaS 类型。
- 增加 object-safety、factory freshness、event/type normalization contract tests。
- crate 版本固定为 `0.0.1`，补齐 repository、documentation、README 与 package metadata，可被 Git rev 或后续 crates.io 精确版本消费。
- 新增 `scripts/check-driver-core-cloud-compat.sh`：只读 archive cloud revision，在临时 workspace 将 daemon-rs 全部 core 依赖重定向到 local path 或指定 Git rev，并运行完整 workspace tests。
- 已用 cloud HEAD `0e20d7b2` 验证 daemon binary、agent、Claude、Cursor、Codex、Gemini 及其余现有 runtime driver 全 workspace 通过；cloud 仓未被修改。

## 刻意未 port

- Go 多租户 server、Postgres schema、zone/machine/用户认证。
- 计费、ops portal、生产部署配置、密钥或用户数据。
- `cocli-conn` 远端 WS 重连层和 multi-machine slot。
- Kimi、Chatrs、Grok、OpenCode 等本轮未声明 runtime；Claude、Cursor、Codex、Gemini 已进入 P0。
- cloud 的完整 delivery priority、digest、two-phase read-ack、watchdog、quota recovery；这些在 P0 最小 E2E 后按风险补。

## 当前缺口

- `cargo run --bin cocli` 仍未启动 HTTP。
- SQLite store、API、server、WS 与 Rust web embed 仍是 placeholder。
- 本仓没有可执行的 `cocli-bridge`，四个 runtime 尚不能把工具回复写回 local store。
- driver contract 尚未执行 ADR 0001 的 cutover。
- Cursor、Codex、Gemini adapter crates 尚未进入 local workspace。
- 本轮只验证了 Rust 单元/工作区测试，尚未完成四 runtime 真实 E2E。
