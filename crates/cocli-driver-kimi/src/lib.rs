//! Kimi-code driver: persistent wire runtime for kimi-code CLI.
//!
//! Mirrors Slock's `kimi --wire` shape: initialize over JSON-RPC, then
//! prompt/steer requests over stdin for subsequent turns.
//!
//! Key properties:
//!   - Registry driver is a `ProcessFactory`; per-process driver owns session
//!     id and stdin.
//!   - `is_turn_exit() == false`; the process stays alive across turns.
//!   - MCP config is passed via `--mcp-config-file`.
//!   - Tool prefix: `mcp__chat__` (kimi-code MCP naming convention)

pub mod conv;
pub mod driver;
pub mod events;
pub mod spawn;

pub use driver::KimiDriver;
pub use events::{parse_line, KimiEvent};
pub use spawn::{spawn_kimi, write_kimi_agents_md, write_kimi_mcp_config, SpawnContext};
