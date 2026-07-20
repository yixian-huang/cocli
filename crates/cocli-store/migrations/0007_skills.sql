CREATE TABLE IF NOT EXISTS skill_library (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    user_invocable INTEGER NOT NULL DEFAULT 0,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('git', 'http', 'local')),
    source_url TEXT NOT NULL,
    source_subpath TEXT,
    source_ref TEXT,
    total_bytes INTEGER NOT NULL,
    file_count INTEGER NOT NULL,
    imported_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_skill_library_updated
    ON skill_library(updated_at DESC, name);

CREATE TABLE IF NOT EXISTS skill_library_files (
    library_id TEXT NOT NULL REFERENCES skill_library(id) ON DELETE CASCADE,
    rel_path TEXT NOT NULL,
    mode INTEGER NOT NULL DEFAULT 420,
    content BLOB NOT NULL,
    size INTEGER NOT NULL,
    PRIMARY KEY(library_id, rel_path)
);

CREATE INDEX IF NOT EXISTS idx_skill_library_files_library
    ON skill_library_files(library_id, rel_path);

CREATE TABLE IF NOT EXISTS agent_skill_installs (
    id TEXT PRIMARY KEY NOT NULL,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    library_id TEXT NOT NULL REFERENCES skill_library(id) ON DELETE CASCADE,
    install_path TEXT NOT NULL,
    installed_at TEXT NOT NULL,
    UNIQUE(agent_id, library_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_skill_installs_agent
    ON agent_skill_installs(agent_id, installed_at);

CREATE INDEX IF NOT EXISTS idx_agent_skill_installs_library
    ON agent_skill_installs(library_id, installed_at);
