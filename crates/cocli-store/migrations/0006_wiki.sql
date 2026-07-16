CREATE TABLE IF NOT EXISTS wiki_pages (
    id TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    content_md TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by TEXT
);

CREATE INDEX IF NOT EXISTS idx_wiki_pages_updated
    ON wiki_pages(updated_at DESC, path);

CREATE TABLE IF NOT EXISTS wiki_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    page_id TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    title TEXT NOT NULL,
    content_md TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    created_by TEXT,
    reason TEXT,
    UNIQUE(page_id, version)
);

CREATE INDEX IF NOT EXISTS idx_wiki_revisions_page
    ON wiki_revisions(page_id, version DESC);

CREATE TABLE IF NOT EXISTS wiki_links (
    source_page_id TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    target_path TEXT NOT NULL,
    PRIMARY KEY(source_page_id, target_path)
);

CREATE INDEX IF NOT EXISTS idx_wiki_links_target
    ON wiki_links(target_path, source_page_id);
