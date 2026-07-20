# cocli-runtime-pool

`cocli-runtime-pool` separates two concerns:

- `RuntimeRegistry` owns registered `Arc<dyn cocli_driver_core::Driver>`
  implementations and optional allowlist filtering.
- `RuntimeCatalog` reports what the current machine can actually launch:
  runtime name, installed binary, version, models, driver capabilities, and an
  explicit `unavailable_reason`.

The default `initial_oss_runtime_specs()` covers every OSS-owned production
adapter: Claude, Cursor, Codex, Gemini, Kimi, Grok, Chatrs, and OpenCode.
Callers may override binary paths or provide additional specs without changing
the registry contract.

Binary discovery is local and deterministic. `SystemRuntimeProbe` searches
`PATH`, checks executable files, and invokes the configured version arguments.
Tests and embedders can implement `RuntimeProbe` to avoid process execution or
supply cached metadata.

`discover_runtime_models()` is the optional production model-enrichment layer.
It prefers local CLI/cache data, queries provider APIs only when matching
credentials are present, and retains deterministic fallbacks.
