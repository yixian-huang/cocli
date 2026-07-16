use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx_core::query::query;
use sqlx_core::row::Row;
use sqlx_sqlite::SqliteRow;
use uuid::Uuid;

use super::{Store, StoreError};

const DEFAULT_WIKI_LIMIT: i64 = 50;
const MAX_WIKI_LIMIT: i64 = 200;

/// Current durable Markdown page.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiPage {
    pub id: Uuid,
    pub path: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}

/// Compact page metadata used by the wiki browser and search.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiPageSummary {
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub version: i64,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}

impl From<WikiPage> for WikiPageSummary {
    fn from(page: WikiPage) -> Self {
        Self {
            path: page.path,
            title: page.title,
            tags: page.tags,
            version: page.version,
            updated_at: page.updated_at,
            updated_by: page.updated_by,
        }
    }
}

/// Immutable snapshot recorded after every create, update, or revert.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiRevision {
    pub version: i64,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Page that contains a `[[wikilink]]` to another page.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiBacklink {
    pub path: String,
    pub title: String,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

impl Store {
    /// Creates or updates a Markdown page and records an immutable revision.
    ///
    /// A positive `if_version` enables optimistic concurrency for existing
    /// pages. Repeated links are de-duplicated before their graph is persisted.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_wiki_page(
        &self,
        path: &str,
        title: &str,
        content: &str,
        tags: &[String],
        updated_by: Option<&str>,
        reason: Option<&str>,
        if_version: Option<i64>,
    ) -> Result<WikiPage, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let page = upsert_wiki_page_in_transaction(
            &mut transaction,
            path,
            title,
            content,
            tags,
            updated_by,
            reason,
            if_version,
        )
        .await?;
        transaction.commit().await?;
        Ok(page)
    }

    /// Returns a page by its canonical path.
    pub async fn get_wiki_page(&self, path: &str) -> Result<Option<WikiPage>, StoreError> {
        let row = query(
            "SELECT id, path, title, content_md, tags, version, created_at, \
             updated_at, updated_by FROM wiki_pages WHERE path = ?",
        )
        .bind(path)
        .fetch_optional(&self.pool)
        .await?;
        row.map(wiki_page_from_row).transpose()
    }

    /// Lists pages by full-text substring and optional exact tag.
    pub async fn list_wiki_pages(
        &self,
        search: Option<&str>,
        tag: Option<&str>,
        limit: i64,
    ) -> Result<Vec<WikiPageSummary>, StoreError> {
        let search = search.map(str::trim).filter(|value| !value.is_empty());
        let pattern = search.map(|value| format!("%{value}%"));
        let tag = tag.map(str::trim).filter(|value| !value.is_empty());
        let limit = normalize_limit(limit);
        let rows = query(
            "SELECT id, path, title, content_md, tags, version, created_at, \
             updated_at, updated_by FROM wiki_pages \
             WHERE (? IS NULL OR path LIKE ? OR title LIKE ? OR content_md LIKE ?) \
             ORDER BY updated_at DESC, path LIMIT ?",
        )
        .bind(pattern.as_deref())
        .bind(pattern.as_deref())
        .bind(pattern.as_deref())
        .bind(pattern.as_deref())
        .bind(if tag.is_some() { MAX_WIKI_LIMIT } else { limit })
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(wiki_page_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map(|pages| {
                pages
                    .into_iter()
                    .filter(|page| tag.map_or(true, |tag| page.tags.iter().any(|item| item == tag)))
                    .map(WikiPageSummary::from)
                    .take(limit as usize)
                    .collect()
            })
    }

    /// Lists every page whose path starts with `prefix`.
    pub async fn list_wiki_pages_by_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<WikiPage>, StoreError> {
        let rows = query(
            "SELECT id, path, title, content_md, tags, version, created_at, \
             updated_at, updated_by FROM wiki_pages WHERE path LIKE ? \
             ORDER BY updated_at DESC, path",
        )
        .bind(format!("{prefix}%"))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(wiki_page_from_row).collect()
    }

    /// Lists immutable revisions in newest-first order.
    pub async fn list_wiki_revisions(
        &self,
        path: &str,
        limit: i64,
    ) -> Result<Vec<WikiRevision>, StoreError> {
        let rows = query(
            "SELECT revision.version, revision.title, revision.content_md, \
             revision.tags, revision.created_at, revision.created_by, revision.reason \
             FROM wiki_revisions revision \
             JOIN wiki_pages page ON page.id = revision.page_id \
             WHERE page.path = ? ORDER BY revision.version DESC LIMIT ?",
        )
        .bind(path)
        .bind(normalize_limit(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(wiki_revision_from_row).collect()
    }

    /// Lists current pages whose persisted link graph points to `path`.
    pub async fn list_wiki_backlinks(&self, path: &str) -> Result<Vec<WikiBacklink>, StoreError> {
        let rows = query(
            "SELECT page.path, page.title, page.updated_at, page.version \
             FROM wiki_links link JOIN wiki_pages page ON page.id = link.source_page_id \
             WHERE link.target_path = ? ORDER BY page.updated_at DESC, page.path",
        )
        .bind(path)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(WikiBacklink {
                    path: row.try_get("path")?,
                    title: row.try_get("title")?,
                    updated_at: row.try_get("updated_at")?,
                    version: row.try_get("version")?,
                })
            })
            .collect()
    }

    /// Restores historical content as a new current version.
    pub async fn revert_wiki_page(
        &self,
        path: &str,
        to_version: i64,
        updated_by: Option<&str>,
    ) -> Result<WikiPage, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let current = query("SELECT id, version, created_at FROM wiki_pages WHERE path = ?")
            .bind(path)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| StoreError::WikiPageNotFound(path.to_owned()))?;
        let id: Uuid = current.try_get("id")?;
        let current_version: i64 = current.try_get("version")?;
        let created_at: DateTime<Utc> = current.try_get("created_at")?;
        let revision = query(
            "SELECT title, content_md, tags FROM wiki_revisions \
             WHERE page_id = ? AND version = ?",
        )
        .bind(id)
        .bind(to_version)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| StoreError::WikiRevisionNotFound {
            path: path.to_owned(),
            version: to_version,
        })?;
        let title: String = revision.try_get("title")?;
        let content: String = revision.try_get("content_md")?;
        let tags_json: String = revision.try_get("tags")?;
        let tags: Vec<String> = serde_json::from_str(&tags_json)?;
        let version = current_version + 1;
        let now = Utc::now();
        query(
            "UPDATE wiki_pages SET title = ?, content_md = ?, tags = ?, \
             version = ?, updated_at = ?, updated_by = ? WHERE id = ?",
        )
        .bind(&title)
        .bind(&content)
        .bind(&tags_json)
        .bind(version)
        .bind(now)
        .bind(updated_by)
        .bind(id)
        .execute(&mut *transaction)
        .await?;
        let reason = format!("revert to version {to_version}");
        insert_wiki_revision(
            &mut transaction,
            id,
            version,
            &title,
            &content,
            &tags_json,
            now,
            updated_by,
            Some(&reason),
        )
        .await?;
        replace_wiki_links(&mut transaction, id, &content).await?;
        transaction.commit().await?;
        Ok(WikiPage {
            id,
            path: path.to_owned(),
            title,
            content,
            tags,
            version,
            created_at,
            updated_at: now,
            updated_by: updated_by.map(str::to_owned),
        })
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub(super) async fn upsert_wiki_page_in_transaction(
    transaction: &mut sqlx_core::transaction::Transaction<'_, sqlx_sqlite::Sqlite>,
    path: &str,
    title: &str,
    content: &str,
    tags: &[String],
    updated_by: Option<&str>,
    reason: Option<&str>,
    if_version: Option<i64>,
) -> Result<WikiPage, StoreError> {
    let now = Utc::now();
    let normalized_tags = normalize_tags(tags);
    let tags_json = serde_json::to_string(&normalized_tags)?;
    let existing = query("SELECT id, version, created_at FROM wiki_pages WHERE path = ?")
        .bind(path)
        .fetch_optional(&mut **transaction)
        .await?;

    let (id, version, created_at) = if let Some(existing) = existing {
        let id: Uuid = existing.try_get("id")?;
        let current_version: i64 = existing.try_get("version")?;
        if let Some(attempted_version) = if_version.filter(|version| *version > 0) {
            if attempted_version != current_version {
                return Err(StoreError::WikiVersionConflict {
                    path: path.to_owned(),
                    current_version,
                    attempted_version,
                });
            }
        }
        let version = current_version + 1;
        query(
            "UPDATE wiki_pages SET title = ?, content_md = ?, tags = ?, \
             version = ?, updated_at = ?, updated_by = ? WHERE id = ?",
        )
        .bind(title)
        .bind(content)
        .bind(&tags_json)
        .bind(version)
        .bind(now)
        .bind(updated_by)
        .bind(id)
        .execute(&mut **transaction)
        .await?;
        (id, version, existing.try_get("created_at")?)
    } else {
        let id = Uuid::new_v4();
        query(
            "INSERT INTO wiki_pages \
             (id, path, title, content_md, tags, version, created_at, updated_at, updated_by) \
             VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?)",
        )
        .bind(id)
        .bind(path)
        .bind(title)
        .bind(content)
        .bind(&tags_json)
        .bind(now)
        .bind(now)
        .bind(updated_by)
        .execute(&mut **transaction)
        .await?;
        (id, 1, now)
    };

    insert_wiki_revision(
        transaction,
        id,
        version,
        title,
        content,
        &tags_json,
        now,
        updated_by,
        reason,
    )
    .await?;
    replace_wiki_links(transaction, id, content).await?;
    Ok(WikiPage {
        id,
        path: path.to_owned(),
        title: title.to_owned(),
        content: content.to_owned(),
        tags: normalized_tags,
        version,
        created_at,
        updated_at: now,
        updated_by: updated_by.map(str::to_owned),
    })
}

#[allow(clippy::too_many_arguments)]
async fn insert_wiki_revision(
    transaction: &mut sqlx_core::transaction::Transaction<'_, sqlx_sqlite::Sqlite>,
    page_id: Uuid,
    version: i64,
    title: &str,
    content: &str,
    tags_json: &str,
    created_at: DateTime<Utc>,
    created_by: Option<&str>,
    reason: Option<&str>,
) -> Result<(), StoreError> {
    query(
        "INSERT INTO wiki_revisions \
         (id, page_id, version, title, content_md, tags, created_at, created_by, reason) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4())
    .bind(page_id)
    .bind(version)
    .bind(title)
    .bind(content)
    .bind(tags_json)
    .bind(created_at)
    .bind(created_by)
    .bind(reason)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn replace_wiki_links(
    transaction: &mut sqlx_core::transaction::Transaction<'_, sqlx_sqlite::Sqlite>,
    page_id: Uuid,
    content: &str,
) -> Result<(), StoreError> {
    query("DELETE FROM wiki_links WHERE source_page_id = ?")
        .bind(page_id)
        .execute(&mut **transaction)
        .await?;
    for target in parse_wiki_links(content) {
        query("INSERT OR IGNORE INTO wiki_links (source_page_id, target_path) VALUES (?, ?)")
            .bind(page_id)
            .bind(target)
            .execute(&mut **transaction)
            .await?;
    }
    Ok(())
}

fn parse_wiki_links(content: &str) -> Vec<String> {
    let bytes = content.as_bytes();
    let mut index = 0;
    let mut seen = HashSet::new();
    let mut links = Vec::new();
    while index + 3 < bytes.len() {
        if bytes[index] != b'[' || bytes[index + 1] != b'[' {
            index += 1;
            continue;
        }
        let start = index + 2;
        let mut end = start;
        while end + 1 < bytes.len() && !(bytes[end] == b']' && bytes[end + 1] == b']') {
            end += 1;
        }
        if end + 1 >= bytes.len() {
            break;
        }
        let target = &content[start..end];
        if is_valid_wiki_link_target(target) && seen.insert(target.to_owned()) {
            links.push(target.to_owned());
        }
        index = end + 2;
    }
    links
}

fn is_valid_wiki_link_target(target: &str) -> bool {
    !target.is_empty()
        && target
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/'))
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .filter(|tag| seen.insert((*tag).to_owned()))
        .map(str::to_owned)
        .collect()
}

fn normalize_limit(limit: i64) -> i64 {
    if limit <= 0 || limit > MAX_WIKI_LIMIT {
        DEFAULT_WIKI_LIMIT
    } else {
        limit
    }
}

fn wiki_page_from_row(row: SqliteRow) -> Result<WikiPage, StoreError> {
    Ok(WikiPage {
        id: row.try_get("id")?,
        path: row.try_get("path")?,
        title: row.try_get("title")?,
        content: row.try_get("content_md")?,
        tags: serde_json::from_str(&row.try_get::<String, _>("tags")?)?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        updated_by: row.try_get("updated_by")?,
    })
}

fn wiki_revision_from_row(row: SqliteRow) -> Result<WikiRevision, StoreError> {
    Ok(WikiRevision {
        version: row.try_get("version")?,
        title: row.try_get("title")?,
        content: row.try_get("content_md")?,
        tags: serde_json::from_str(&row.try_get::<String, _>("tags")?)?,
        created_at: row.try_get("created_at")?,
        created_by: row.try_get("created_by")?,
        reason: row.try_get("reason")?,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_wiki_links;

    #[test]
    fn parses_unique_safe_wikilinks_in_source_order() {
        assert_eq!(
            parse_wiki_links(
                "See [[roadmap/local-loop]] and [[agent_notes.v1]]. \
                 Ignore [[not a link]], [[target|alias]], and [[roadmap/local-loop]]."
            ),
            vec!["roadmap/local-loop", "agent_notes.v1"]
        );
    }
}
