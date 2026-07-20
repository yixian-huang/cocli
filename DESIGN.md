# Design

## Source of truth

- Status: Active
- Last refreshed: 2026-07-20
- Primary product surfaces: local web client, local HTTP/Bridge API, persistent Agent runtime
- Evidence reviewed: `web/src/local/LocalApp.tsx`, `web/src/local/api.ts`, `web/src/local/local.css`, `crates/cocli-store/src/lib.rs`, `crates/cocli-api/src/lib.rs`, `crates/cocli-agent/src/prompt.rs`, `crates/cocli-server/src/runtime.rs`

This document is the canonical product and interaction contract for cocli. When code, copy, or older roadmap documents disagree with it, this document wins until it is deliberately refreshed.

## Brand

- Personality: calm, capable, local-first, explicit about state without exposing unnecessary machinery.
- Trust signals: durable state, clear ownership, reversible controls, visible provenance, actionable failures.
- Avoid: coding-only language, autonomous-agent spectacle, opaque orchestration, terminal-first interaction, and treating Runtime internals as the product.

## Product goals

- Goals:
  - Make persistent Agents and Channels the two natural starting points for any domain of work.
  - Let users give a requirement either to a Channel or directly to an Agent.
  - Let authorized Agents create durable Agents, Channels, and memberships, and optionally use Tasks as coordination primitives.
  - Preserve identity, memory, organization, and conversation independently of CLI processes and runtime sessions.
  - Keep Workspace an optional, non-product resource handle for local resolution and migration—not a feature line the product markets or deepens.
  - Make execution observable and controllable when needed without forcing users to understand Session, Turn, PID, or CLI concepts.
- Non-goals:
  - Owning an Agent's reasoning, diff review, checkpoint policy, rollback policy, validation gate, or budget enforcement.
  - Acting as a central intelligent task scheduler; Agents claim and organize work using durable coordination primitives when they need them.
  - Requiring a project, repository, directory, or worktree before useful work can begin.
  - **Treating Git repositories, worktrees, or other Workspace providers as product surfaces** the product maintains, prioritizes, or teaches as primary workflows. Git and similar providers may exist only as thin adapters for local path resolution, Runtime cwd, and portable rebinding—not as navigation, onboarding, or milestone depth.
  - **Defining a Channel by Tasks or by a formal purpose/goal.** A Channel is a durable collaboration room (conversation + membership + shared context), not a project shell, task board container, or purpose object.
  - Shipping Wiki as a core product surface. A future Wiki should be a plugin over stable extension contracts.
- Success signals:
  - A new user can create a Channel or Agent and receive useful work without selecting a filesystem path, attaching a Workspace, or creating a Task.
  - Agent identity and memory survive runtime restart or model changes.
  - One Agent can participate in multiple Channels and one Channel can contain multiple Agents.
  - A user can complete normal work without seeing a Session identifier or CLI process state.
  - Channel empty states invite conversation or inviting Agents—not filling a purpose field or opening a task board first.

## Subject semantics

### Agent

A persistent worker identity. Runtime, model, Session, and CLI process are execution details beneath it. Users talk to an Agent directly; Agents may join many Channels.

### Channel

A durable **collaboration context**: messages, participating Agents, and optional shared Memory.

- Primary jobs: talk together, keep history, manage membership, share context.
- **Not** a project, epic, or purpose container. Optional description fields are labels for humans, not product-required “purpose” or goal objects that structure the Channel.
- **Not** defined by Tasks. Tasks may appear as a secondary coordination tool when Agents (or users) need claim/dependency machinery; they do not define the Channel or own the primary surface.

### Task

A **coordination primitive** (claim, dependency, lifecycle) for Agents organizing work—not the user’s primary model of a Channel.

- Implementation may still scope Tasks under a Channel for durability and Bridge APIs.
- Product IA must not make Task boards or purpose/goal fields the Channel’s center of gravity.
- Empty or new Channels must work with zero Tasks.

### Workspace

An optional **resource handle**: how this installation resolves something local (path, URI, opaque locator) for Runtime or migration.

- Domain-neutral in principle; **not** a first-class product line.
- Git / directory / managed / external providers are **adapters**, not product features to deepen. The product does not maintain Git workflows, worktree UX, or provider-specific milestones.
- Useful for: portable backup rebinding, installation-local binding, optional Runtime working directory—not for onboarding or primary navigation.
- Never a startup prerequisite.

### Memory, Skills, Runtime

- Memory and Skills are tools used by Agents (and shared Memory under a Channel when attached).
- Runtime history and raw execution details are diagnostic surfaces.
- Skill/MCP governance supports multi-Runtime desktop work; they remain subordinate to Agent/Channel subjects.

## Personas and jobs

- Primary personas: individuals coordinating persistent local AI workers across software, research, writing, analysis, operations, and custom domains.
- User jobs:
  - Start or continue a durable body of work through conversation.
  - Delegate directly to a trusted Agent.
  - Bring several Agents together around a shared conversation.
  - Inspect current work and intervene when an Agent is blocked or failing.
  - Search, back up, restore, and migrate durable local state (subjects and conversation—not Git product workflows).
- Key contexts of use: focused desktop work, long-running local workflows, intermittent monitoring, and recovery after process or application restarts.

## Information architecture

- Primary navigation: Channels, Agents, global Search, Settings.
- Core routes/screens:
  - **Channel (primary):** conversation stream, participating Agents, optional shared Memory, activity summary. Task lists and Workspace attachments—if exposed—are secondary disclosure, not default center tabs that imply “project mode.”
  - **Agent (primary):** direct conversation, current execution summary, Channels membership, private Memory, Skills, runtime diagnostics when needed.
  - **Settings:** Runtime availability and advanced execution configuration; backup remains a global durable-state action.
- Content hierarchy:
  - Channel and Agent are first-class subjects.
  - Conversation and membership define the Channel experience.
  - Tasks are optional coordination under a subject, not the subject’s definition.
  - Memory, Skills, and diagnostics are scoped beneath an Agent or Channel when relevant.
  - Workspace is not primary navigation and must not appear as a required setup step.
  - Search is a global action, not a domain object.
  - Runtime, Session, Turn, tool calls, and CLI output appear only in diagnostics or error recovery.

Direct Agent conversation may use a system-managed private Channel underneath, but the UI and public interaction contract present it as a conversation with the Agent. This keeps one durable message substrate without leaking implementation detail.

## Design principles

- Subjects before tools: navigate by Channel and Agent; reveal Tasks, Memory, Skills, Workspace, and diagnostics only as needed.
- Conversation before coordination: Channel defaults to talking and membership; Task machinery is progressive disclosure.
- Persistent identity over process: an Agent exists when its runtime is idle, stopped, restarted, or replaced.
- Progressive disclosure: show outcome and work state first; expose execution internals only for diagnosis and control.
- Domain neutrality: core copy and flows must remain valid outside software development.
- Explicit scope: private Agent context, shared Channel context, and any attached resource handles must never blur together.
- Adapters stay thin: do not grow Git, worktree, or provider-specific product UX under the cocli brand.
- Tradeoffs: reuse the Channel message model for direct Agent conversations; accept a system-managed private Channel to avoid parallel conversation semantics. Accept Task rows scoped under Channels in storage without making Tasks the product definition of a Channel.

## Visual language

- Color: extend the existing dark/light semantic tokens in `web/src/local/local.css`; orange remains the primary action accent.
- Typography: human-readable system sans for content; monospace only for compact operational signals and diagnostics.
- Spacing/layout rhythm: retain the existing 4px-based token scale and dense desktop workspace rhythm.
- Shape/radius/elevation: restrained panels and small radii; elevation only for overlays, menus, and modal inspection.
- Motion: 150–220ms transitions; no decorative motion that competes with live execution updates.
- Imagery/iconography: existing cocli brand assets and Lucide icons; icons supplement labels rather than replace them.

## Components

- Existing components to reuse: local select controls, Channel message stream, optional task workspace (secondary), memory workspace, skills workspace, history inspector, global search dialog.
- New/changed components:
  - Subject switcher for Channels and Agents.
  - Agent detail shell with direct conversation and scoped utility views.
  - Channel participant/membership management.
  - Agent state summary separating lifecycle from execution state.
  - Settings surface for Runtime configuration plus a global durable-state
    backup action; restore remains an offline CLI recovery operation in alpha.
  - Avoid promoting Workspace/Git browsers or Task boards as default Channel chrome.
- Variants and states: active, paused, archived lifecycle; idle, queued, working, waiting, blocked, failed, and offline execution states.
- Token/component ownership: local shell tokens stay in `web/src/local/local.css`; do not add a parallel design-system layer.

## Accessibility

- Target standard: WCAG 2.2 AA for primary flows.
- Keyboard/focus behavior: all navigation, tabs, dialogs, controls, and diagnostic disclosure must be keyboard reachable with visible focus.
- Contrast/readability: use semantic text and state tokens; never communicate Agent state by color alone.
- Screen-reader semantics: first-class navigation landmarks, labeled state text, dialog titles, and live regions only for actionable execution changes.
- Reduced motion and sensory considerations: honor `prefers-reduced-motion`; avoid continuous animation for working state.

## Responsive behavior

- Supported breakpoints/devices: desktop-first from 1024px; usable single-column layout down to 320px.
- Layout adaptations: collapse subject rail and secondary inspector into drawers on narrow screens; keep the active conversation or detail view primary.
- Touch/hover differences: every hover action needs a persistent or focus-visible equivalent; minimum touch targets are 40px where space permits.

## Interaction states

- Loading: keep the current subject visible and show scoped progress; avoid replacing the entire application shell.
- Empty: offer the next meaningful action: create a Channel, create an Agent, invite an Agent, or send a first message. Do not require a purpose field, Task, or Workspace attachment.
- Error: name the affected subject, preserve durable state, and offer retry or diagnostics.
- Success: update the relevant subject in place; avoid transient success screens.
- Disabled: explain missing capability or lifecycle state in adjacent copy or tooltip.
- Offline/slow network: local state remains readable; live execution indicates disconnected/reconnecting and backfills durable events after reconnect.

## Content voice

- Tone: direct, calm, specific, and domain-neutral.
- Terminology:
  - Use Agent, Channel, Task, Memory, Skill, Workspace, and Runtime consistently when the concept appears.
  - Prefer “conversation”, “members”, and “shared context” when describing a Channel.
  - Say “working”, “waiting”, “paused”, or “needs attention” in normal UI.
  - Reserve Session, Turn, process, token, stdout, and stderr for diagnostics.
- Microcopy rules:
  - Describe the user's consequence, not the internal API action.
  - Never call a Channel a project, epic, or purpose container.
  - Never assume a Task changes code or that work requires a repository.
  - Do not market Git, worktrees, or Workspace providers as the product.

## Implementation constraints

- Framework/styling system: React 19, TypeScript, Vite, existing local CSS tokens and components.
- Design-token constraints: extend semantic variables; do not hard-code theme-specific colors in components.
- Performance constraints: fetch independent subject data in parallel, avoid duplicate global listeners, derive view state during render, and keep heavy diagnostics off the default path.
- Compatibility constraints: migrate existing single-Channel Agents into memberships; preserve durable messages, tasks, memory, skills, and runtime history. Existing Task-under-Channel and Workspace provider schemas may remain for durability and Bridge; product surfaces must stop treating them as Channel/product definition.
- Test/screenshot expectations: component tests for both subject entry points, membership changes, direct Agent conversation, Channel empty states without Tasks, hidden diagnostics, and empty/error states; Rust integration tests for the same HTTP contracts.

## Open questions

- [x] ~~Define the first stable Workspace provider contract after managed, directory, Git, and external metadata prove the common fields.~~ **Closed (2026-07-20):** no product-level provider contract roadmap. Providers stay thin adapters; do not invest in Git/Workspace product depth. Portable binding fields remain an implementation concern for backup/rebind only.
- [ ] Define plugin packaging and permission contracts before reintroducing Wiki or other optional knowledge products.
- [ ] Decide whether Agent execution profiles are editable snapshots or separately reusable named resources after the identity/membership migration lands.
- [ ] How far to demote Task UI in the local client (hidden by default vs secondary tab) without breaking Agent Bridge coordination flows.
