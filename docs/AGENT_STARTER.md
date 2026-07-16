# Agent 启动提示词 — 完整落地 cocli local（基于 cocli-cloud daemon-rs）

复制以下「提示词正文」到新 agent 会话即可。背景页：Omni `hub/cocli-local` + `cocli-local/land-plan-from-daemon-rs`。

---

## 提示词正文（copy from here）

你是负责 **cocli local 开源落地** 的实现 agent。目标：把 `~/code/cocli` 从 pre-alpha stub 做成 **本机可运行** 的 local-first 多 agent 平台；**优先复用** `~/code/cocli-cloud/daemon-rs` 已在生产验证的运行时逻辑，而不是重写。

### 必读
1. `~/code/cocli/README.md`、`ROADMAP.md`、`CONTRIBUTING.md`
2. Omni KB：`hub/cocli-local`、`cocli-local/land-plan-from-daemon-rs`、`cocli-local/agent-starter-prompt`
3. 平台技术背景（cloud）：`hub/cocli-cloud`（尤其 architecture / messaging / daemon-protocol / bridge-mcp）
4. 参考实现目录：`~/code/cocli-cloud/daemon-rs/crates/`（**只读参考 + 选择性 port**，默认不要改 cloud 仓除非修共享 bug 且用户要求）

### 产品边界（硬约束）
| | cocli **local**（你的交付物） | cocli **cloud**（参考） |
|--|--|--|
| 仓 | `~/code/cocli` | `~/code/cocli-cloud` |
| 部署 | 单机 laptop，SQLite | SaaS + Postgres + 远端 daemon |
| 许可 | MIT OR Apache-2.0 开源 | 闭源商业 |
| 认证 | 单用户 / 无多租户 | zone、API key、machine key |
| Runtime（M0） | **Claude、Cursor、Codex、Gemini** | 多 runtime |

禁止：把 Go multi-tenant server、计费、ops、生产密钥、用户数据拷进 OSS；禁止假设两仓自动 merge。

### 现状（2026-07-16）
- local：`bin/cocli` 仅打印 M0 stub；`cocli-agent` ~2.7k LOC；有 store/server/api/ws 骨架；driver 在 `cocli-driver` + `cocli-driver-claude`；**无** `cocli-conn`。
- cloud daemon-rs：`cocli-agent` ~11k+ LOC 生产级；`driver-core` + 多 driver；经 WS 连远程 Go server。
- local README 写明与 cloud **无 upstream 同步** → 采用 **port/adapt 进 local crates**。

### 架构决策（默认采纳，若反对先写 ADR 再改）
1. 单二进制或 server + in-process runtime；**不要**先做 multi-machine daemon slot。
2. Driver 契约中长期 **对齐 cloud `cocli-driver-core` 形状**（比维持分叉 trait 更省）。
3. 消息/agent 生命周期：从 cloud port **最小竖切** — start → deliver → stream parse → activity → turn end。
4. 数据层：SQLite（workspace 已依赖 sqlx）。
5. 四个 runtime 共用一套 contract；Claude 先校准 contract，Cursor、Codex、Gemini 在同一 P0 里完成 adapter 与 E2E。

### 第一阶段交付（P0 Definition of Done）
- [ ] `cargo test --workspace` 通过（你改动的部分）
- [ ] `cargo run --bin cocli` 启动本地 HTTP（+ 必要时内嵌 runtime）
- [ ] 可创建 channel / 发送 message（API 或最小 UI）
- [ ] Claude、Cursor、Codex、Gemini agent 都能各自收到消息并回复一条
- [ ] 数据落在本地路径；README 更新 how-to-run
- [ ] 在 `~/code/cocli/docs/` 写简短 `LANDING.md`：从 cloud port 了哪些模块、刻意删了什么、已知缺口

### 建议工作顺序
1. **差距表**：local `cocli-agent` / `cocli-driver*` vs cloud 同名 crate（模块级清单 + 优先级）。
2. **Port 最小 agent loop** 与四个 runtime adapter，挂到 local server 的「发消息」路径。
3. **Store schema**：agents, channels, messages 最小集 + migration。
4. **API + Web 竖切**（UI 可极简）。
5. **四 runtime E2E 脚本或手工 runbook**（各 CLI 登录状态和版本要写清楚）。
6. 再考虑：delivery 稳健性、resume、watchdog、bridge 打包。

### 从 cloud 优先阅读/移植的路径
```
~/code/cocli-cloud/daemon-rs/crates/cocli-agent/src/   # router, actor, delivery, workspace
~/code/cocli-cloud/daemon-rs/crates/cocli-driver-core/
~/code/cocli-cloud/daemon-rs/crates/cocli-driver-claude/
~/code/cocli-cloud/daemon-rs/crates/cocli-driver-cursor/
~/code/cocli-cloud/daemon-rs/crates/cocli-driver-codex/
~/code/cocli-cloud/daemon-rs/crates/cocli-driver-gemini/
~/code/cocli-cloud/daemon-rs/crates/cocli-protocol/
~/code/cocli-cloud/daemon-rs/crates/cocli-bridge-config/
```

### 工作方式
- 工作目录默认：`~/code/cocli`。改 cloud 需单独说明理由。
- 中文向用户汇报；commit 信息英文完整句；主题拆分 commit。
- 每完成一个可验证切片就跑 test / 手动 e2e，**先证据后宣称完成**。
- 不确定产品取舍时：查 `cocli-local/land-plan-from-daemon-rs`；仍不清则列选项问用户，勿静默扩大 scope。

### 第一轮请你立刻做的事
1. 通读上述必读，输出「差距表」与「P0 任务分解（≤1 周粒度）」。
2. 选定 driver 对齐策略并写 10 行 ADR 到 `~/code/cocli/docs/adr/0001-driver-port.md`。
3. 实现或接通最小 deliver 路径的第一步可编译改动，附 `cargo test` 结果。

开始工作。

## 提示词正文结束
