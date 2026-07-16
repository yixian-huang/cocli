# cocli-runtime-pool

`cocli-runtime-pool` separates two concerns:

- `RuntimeRegistry` owns registered `Arc<dyn cocli_driver_core::Driver>`
  implementations and optional allowlist filtering.
- `RuntimeCatalog` reports what the current machine can actually launch:
  runtime name, installed binary, version, models, driver capabilities, and an
  explicit `unavailable_reason`.

The default `initial_oss_runtime_specs()` covers Claude, Cursor, Codex, and
Gemini. Callers may override binary paths or provide additional specs without
changing the registry contract.

Discovery is local and deterministic. `SystemRuntimeProbe` searches `PATH`,
checks executable files, and invokes the configured version arguments. It does
not call provider APIs. Tests and embedders can implement `RuntimeProbe` to
avoid process execution or supply cached metadata.
