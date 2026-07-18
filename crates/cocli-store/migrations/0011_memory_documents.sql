CREATE TABLE IF NOT EXISTS memory_documents (
    id TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL UNIQUE,
    content_md TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by TEXT
);

CREATE INDEX IF NOT EXISTS idx_memory_documents_path
    ON memory_documents(path);

INSERT OR IGNORE INTO memory_documents
    (id, path, content_md, version, created_at, updated_at, updated_by)
SELECT id, path, content_md, version, created_at, updated_at, updated_by
FROM wiki_pages
WHERE path LIKE 'agents/%/memory/%'
   OR path LIKE 'channels/%/notes/%';
