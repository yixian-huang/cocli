//! Codex app-server notifications the daemon intentionally ignores.
//!
//! Mirrors Go `knownSilentNotificationSet` + `knownSilentNotificationPrefixes`
//! (codex.go:866-937). Listed explicitly so genuinely new methods still
//! surface the one-shot `[runtime_drift]` alert.

const SILENT_METHODS: &[&str] = &[
    // session / account lifecycle
    "thread/status/changed",
    "thread/archived",
    "thread/unarchived",
    "thread/name/updated",
    "thread/goal/updated",
    "thread/goal/cleared",
    "skills/changed",
    "account/updated",
    "account/login/completed",
    "app/list/updated",
    "fs/changed",
    "model/verification",
    "serverRequest/resolved",
    "remoteControl/status/changed",
    "externalAgentConfig/import/completed",
    // soft warnings
    "deprecationNotice",
    "warning",
    "guardianWarning",
    "configWarning",
    "windows/worldWritableWarning",
    "windowsSandbox/setupCompleted",
    // streaming deltas / progress
    "turn/diff/updated",
    "turn/plan/updated",
    "item/plan/delta",
    "item/reasoning/summaryTextDelta",
    "item/reasoning/summaryPartAdded",
    "item/reasoning/textDelta",
    "command/exec/outputDelta",
    "process/outputDelta",
    "item/commandExecution/outputDelta",
    "item/commandExecution/terminalInteraction",
    "item/fileChange/outputDelta",
    "item/fileChange/patchUpdated",
    "item/mcpToolCall/progress",
    "rawResponseItem/completed",
    // hook lifecycle
    "hook/started",
    "hook/completed",
    // fuzzy file search (UI-only)
    "fuzzyFileSearch/sessionUpdated",
    "fuzzyFileSearch/sessionCompleted",
];

const SILENT_PREFIXES: &[&str] = &["mcpServer/", "thread/realtime/"];

pub(crate) fn is_known_silent_notification(method: &str) -> bool {
    if SILENT_METHODS.contains(&method) {
        return true;
    }
    SILENT_PREFIXES
        .iter()
        .any(|prefix| method.starts_with(prefix))
}
