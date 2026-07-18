# Design

## Source of truth

- Status: Active
- Last refreshed: 2026-07-18
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
  - Let authorized Agents create durable Agents, Channels, memberships, and Tasks to organize work.
  - Preserve identity, memory, organization, and context independently of CLI processes and runtime sessions.
  - Keep Workspace optional and domain-neutral; Git repositories and worktrees are adapters, not prerequisites.
  - Make execution observable and controllable when needed without forcing users to understand Session, Turn, PID, or CLI concepts.
- Non-goals:
  - Owning an Agent's reasoning, diff review, checkpoint policy, rollback policy, validation gate, or budget enforcement.
  - Acting as a central intelligent task scheduler; Agents claim and organize work using durable coordination primitives.
  - Requiring a project, repository, directory, or worktree before useful work can begin.
  - Shipping Wiki as a core product surface. A future Wiki should be a plugin over stable extension contracts.
- Success signals:
  - A new user can create a Channel or Agent and receive useful work without selecting a filesystem path.
  - Agent identity and memory survive runtime restart or model changes.
  - One Agent can participate in multiple Channels and one Channel can contain multiple Agents.
  - A user can complete normal work without seeing a Session identifier or CLI process state.

## Personas and jobs

- Primary personas: individuals coordinating persistent local AI workers across software, research, writing, analysis, operations, and custom domains.
- User jobs:
  - Start or continue a durable body of work.
  - Delegate directly to a trusted Agent.
  - Bring several Agents together around a shared context.
  - Inspect current work and intervene when an Agent is blocked or failing.
  - Search, back up, restore, and migrate durable local state.
- Key contexts of use: focused desktop work, long-running local workflows, intermittent monitoring, and recovery after process or application restarts.

## Information architecture

- Primary navigation: Channels, Agents, global Search, Settings.
- Core routes/screens:
  - Channel: conversation, Tasks, participating Agents, shared Memory, optional Workspace, activity summary.
  - Agent: direct conversation, current work, Channels, private Memory, Skills, optional Workspace, runtime diagnostics.
  - Settings: Runtime availability and advanced execution configuration; backup remains a global durable-state action.
- Content hierarchy:
  - Channel and Agent are first-class subjects.
  - Tasks and shared context are scoped beneath a Channel.
  - Memory, Skills, and diagnostics are scoped beneath an Agent or Channel.
  - Search is a global action, not a domain object.
  - Runtime, Session, Turn, tool calls, and CLI output appear only in diagnostics or error recovery.

Direct Agent conversation may use a system-managed private Channel underneath, but the UI and public interaction contract present it as a conversation with the Agent. This keeps one durable message substrate without leaking implementation detail.

## Design principles

- Subjects before tools: navigate by Channel and Agent, then reveal their Tasks, Memory, Skills, Workspace, and diagnostics.
- Persistent identity over process: an Agent exists when its runtime is idle, stopped, restarted, or replaced.
- Progressive disclosure: show outcome and work state first; expose execution internals only for diagnosis and control.
- Domain neutrality: core copy and flows must remain valid outside software development.
- Explicit scope: private Agent context, shared Channel context, and attached Workspace resources must never blur together.
- Tradeoffs: reuse the Channel message model for direct Agent conversations; accept a system-managed private Channel to avoid parallel conversation semantics.

## Visual language

- Color: extend the existing dark/light semantic tokens in `web/src/local/local.css`; orange remains the primary action accent.
- Typography: human-readable system sans for content; monospace only for compact operational signals and diagnostics.
- Spacing/layout rhythm: retain the existing 4px-based token scale and dense desktop workspace rhythm.
- Shape/radius/elevation: restrained panels and small radii; elevation only for overlays, menus, and modal inspection.
- Motion: 150–220ms transitions; no decorative motion that competes with live execution updates.
- Imagery/iconography: existing cocli brand assets and Lucide icons; icons supplement labels rather than replace them.

## Components

- Existing components to reuse: local select controls, Channel message stream, task workspace, memory workspace, skills workspace, history inspector, global search dialog.
- New/changed components:
  - Subject switcher for Channels and Agents.
  - Agent detail shell with direct conversation and scoped utility views.
  - Channel participant/membership management.
  - Agent state summary separating lifecycle from execution state.
  - Settings surface for Runtime configuration plus a global durable-state
    backup action; restore remains an offline CLI recovery operation in alpha.
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
- Empty: offer the next meaningful action: create a Channel, create an Agent, invite an Agent, or send a first requirement.
- Error: name the affected subject, preserve durable state, and offer retry or diagnostics.
- Success: update the relevant subject in place; avoid transient success screens.
- Disabled: explain missing capability or lifecycle state in adjacent copy or tooltip.
- Offline/slow network: local state remains readable; live execution indicates disconnected/reconnecting and backfills durable events after reconnect.

## Content voice

- Tone: direct, calm, specific, and domain-neutral.
- Terminology:
  - Use Agent, Channel, Task, Memory, Skill, Workspace, and Runtime consistently.
  - Say “working”, “waiting”, “paused”, or “needs attention” in normal UI.
  - Reserve Session, Turn, process, token, stdout, and stderr for diagnostics.
- Microcopy rules: describe the user's consequence, not the internal API action; never call a Channel a project or assume a Task changes code.

## Implementation constraints

- Framework/styling system: React 19, TypeScript, Vite, existing local CSS tokens and components.
- Design-token constraints: extend semantic variables; do not hard-code theme-specific colors in components.
- Performance constraints: fetch independent subject data in parallel, avoid duplicate global listeners, derive view state during render, and keep heavy diagnostics off the default path.
- Compatibility constraints: migrate existing single-Channel Agents into memberships; preserve durable messages, tasks, memory, skills, and runtime history.
- Test/screenshot expectations: component tests for both subject entry points, membership changes, direct Agent conversation, hidden diagnostics, and empty/error states; Rust integration tests for the same HTTP contracts.

## Open questions

- [ ] Define the first stable Workspace provider contract after managed, directory, Git, and external metadata prove the common fields.
- [ ] Define plugin packaging and permission contracts before reintroducing Wiki or other optional knowledge products.
- [ ] Decide whether Agent execution profiles are editable snapshots or separately reusable named resources after the identity/membership migration lands.
