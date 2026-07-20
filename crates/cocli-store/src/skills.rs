use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;
use sqlx_core::row::Row;
use sqlx_sqlite::SqliteRow;
use uuid::Uuid;

use super::{Store, StoreError};

const MAX_SKILL_NAME_LEN: usize = 80;
const MAX_SKILL_BYTES: usize = 50 * 1024 * 1024;

/// One file stored in or read from the local skill library.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLibraryFile {
    pub rel_path: String,
    pub mode: i64,
    pub content: Vec<u8>,
    pub size: i64,
}

/// File metadata returned without loading its body.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLibraryFileMeta {
    pub rel_path: String,
    pub mode: i64,
    pub size: i64,
}

/// Metadata for one imported local skill.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLibraryEntry {
    pub id: Uuid,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub user_invocable: bool,
    pub source_kind: String,
    pub source_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_subpath: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    pub total_bytes: i64,
    pub file_count: i64,
    pub imported_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub in_use_count: i64,
}

/// Owned input used to create one skill library entry atomically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSkillLibrary {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub user_invocable: bool,
    pub source_kind: String,
    pub source_url: String,
    pub source_subpath: Option<String>,
    pub source_ref: Option<String>,
    pub files: Vec<SkillLibraryFile>,
}

/// Durable record linking one library entry to one agent workspace.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillInstall {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub library_id: Uuid,
    pub install_path: String,
    pub installed_at: DateTime<Utc>,
    pub library_name: String,
    pub source_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
}

impl Store {
    /// Creates a library entry and all file bodies in one transaction.
    pub async fn create_skill_library(
        &self,
        draft: NewSkillLibrary,
    ) -> Result<SkillLibraryEntry, StoreError> {
        validate_skill_name(&draft.name)?;
        validate_skill_files(&draft.files)?;
        let exists: i64 = query_scalar("SELECT COUNT(*) FROM skill_library WHERE name = ?")
            .bind(&draft.name)
            .fetch_one(&self.pool)
            .await?;
        if exists != 0 {
            return Err(StoreError::SkillNameConflict(draft.name));
        }

        let id = Uuid::new_v4();
        let now = Utc::now();
        let total_bytes = skill_total_bytes(&draft.files)?;
        let mut transaction = self.pool.begin().await?;
        query(
            "INSERT INTO skill_library \
             (id, name, display_name, description, user_invocable, source_kind, source_url, \
              source_subpath, source_ref, total_bytes, file_count, imported_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&draft.name)
        .bind(&draft.display_name)
        .bind(&draft.description)
        .bind(draft.user_invocable)
        .bind(&draft.source_kind)
        .bind(&draft.source_url)
        .bind(&draft.source_subpath)
        .bind(&draft.source_ref)
        .bind(total_bytes)
        .bind(draft.files.len() as i64)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        insert_skill_files(&mut transaction, id, &draft.files).await?;
        transaction.commit().await?;
        self.get_skill_library(id)
            .await?
            .ok_or(StoreError::SkillLibraryNotFound(id))
    }

    /// Lists the local skill catalog with current install counts.
    pub async fn list_skill_library(&self) -> Result<Vec<SkillLibraryEntry>, StoreError> {
        let rows = query(
            "SELECT library.*, \
             (SELECT COUNT(*) FROM agent_skill_installs install \
              WHERE install.library_id = library.id) AS in_use_count \
             FROM skill_library library ORDER BY library.updated_at DESC, library.name",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(skill_library_from_row).collect()
    }

    /// Returns one skill library entry.
    pub async fn get_skill_library(
        &self,
        library_id: Uuid,
    ) -> Result<Option<SkillLibraryEntry>, StoreError> {
        let row = query(
            "SELECT library.*, \
             (SELECT COUNT(*) FROM agent_skill_installs install \
              WHERE install.library_id = library.id) AS in_use_count \
             FROM skill_library library WHERE library.id = ?",
        )
        .bind(library_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(skill_library_from_row).transpose()
    }

    /// Returns one skill library entry by its canonical name.
    pub async fn get_skill_library_by_name(
        &self,
        name: &str,
    ) -> Result<Option<SkillLibraryEntry>, StoreError> {
        let row = query(
            "SELECT library.*, \
             (SELECT COUNT(*) FROM agent_skill_installs install \
              WHERE install.library_id = library.id) AS in_use_count \
             FROM skill_library library WHERE library.name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        row.map(skill_library_from_row).transpose()
    }

    /// Lists file metadata for one library entry.
    pub async fn list_skill_library_files(
        &self,
        library_id: Uuid,
    ) -> Result<Vec<SkillLibraryFileMeta>, StoreError> {
        let rows = query(
            "SELECT rel_path, mode, size FROM skill_library_files \
             WHERE library_id = ? ORDER BY rel_path",
        )
        .bind(library_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(SkillLibraryFileMeta {
                    rel_path: row.try_get("rel_path")?,
                    mode: row.try_get("mode")?,
                    size: row.try_get("size")?,
                })
            })
            .collect()
    }

    /// Loads all file bodies for installation or export.
    pub async fn load_skill_library_files(
        &self,
        library_id: Uuid,
    ) -> Result<Vec<SkillLibraryFile>, StoreError> {
        let rows = query(
            "SELECT rel_path, mode, content, size FROM skill_library_files \
             WHERE library_id = ? ORDER BY rel_path",
        )
        .bind(library_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(skill_file_from_row).collect()
    }

    /// Loads one file body by safe relative path.
    pub async fn get_skill_library_file(
        &self,
        library_id: Uuid,
        rel_path: &str,
    ) -> Result<Option<SkillLibraryFile>, StoreError> {
        validate_skill_file_path(rel_path)?;
        let row = query(
            "SELECT rel_path, mode, content, size FROM skill_library_files \
             WHERE library_id = ? AND rel_path = ?",
        )
        .bind(library_id)
        .bind(rel_path)
        .fetch_optional(&self.pool)
        .await?;
        row.map(skill_file_from_row).transpose()
    }

    /// Replaces every file and source revision in one transaction.
    pub async fn replace_skill_library_files(
        &self,
        library_id: Uuid,
        source_ref: Option<&str>,
        files: &[SkillLibraryFile],
    ) -> Result<SkillLibraryEntry, StoreError> {
        validate_skill_files(files)?;
        let total_bytes = skill_total_bytes(files)?;
        let now = Utc::now();
        let mut transaction = self.pool.begin().await?;
        let result = query(
            "UPDATE skill_library SET source_ref = ?, total_bytes = ?, file_count = ?, \
             updated_at = ? WHERE id = ?",
        )
        .bind(source_ref)
        .bind(total_bytes)
        .bind(files.len() as i64)
        .bind(now)
        .bind(library_id)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() == 0 {
            return Err(StoreError::SkillLibraryNotFound(library_id));
        }
        query("DELETE FROM skill_library_files WHERE library_id = ?")
            .bind(library_id)
            .execute(&mut *transaction)
            .await?;
        insert_skill_files(&mut transaction, library_id, files).await?;
        transaction.commit().await?;
        self.get_skill_library(library_id)
            .await?
            .ok_or(StoreError::SkillLibraryNotFound(library_id))
    }

    /// Deletes one library entry and cascades files and install records.
    pub async fn delete_skill_library(&self, library_id: Uuid) -> Result<(), StoreError> {
        let result = query("DELETE FROM skill_library WHERE id = ?")
            .bind(library_id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(StoreError::SkillLibraryNotFound(library_id));
        }
        Ok(())
    }

    /// Records an installed library at its runtime-specific workspace path.
    pub async fn create_agent_skill_install(
        &self,
        agent_id: Uuid,
        library_id: Uuid,
        install_path: &str,
    ) -> Result<AgentSkillInstall, StoreError> {
        validate_install_path(install_path)?;
        let exists: i64 = query_scalar(
            "SELECT COUNT(*) FROM agent_skill_installs \
             WHERE agent_id = ? AND library_id = ?",
        )
        .bind(agent_id)
        .bind(library_id)
        .fetch_one(&self.pool)
        .await?;
        if exists != 0 {
            return Err(StoreError::SkillAlreadyInstalled {
                agent_id,
                library_id,
            });
        }
        let id = Uuid::new_v4();
        query(
            "INSERT INTO agent_skill_installs \
             (id, agent_id, library_id, install_path, installed_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(agent_id)
        .bind(library_id)
        .bind(install_path)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        self.get_agent_skill_install(id)
            .await?
            .ok_or(StoreError::SkillInstallNotFound(id))
    }

    /// Returns one install record joined with its library metadata.
    pub async fn get_agent_skill_install(
        &self,
        install_id: Uuid,
    ) -> Result<Option<AgentSkillInstall>, StoreError> {
        let row = query(&agent_skill_install_select("WHERE install.id = ?"))
            .bind(install_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(agent_skill_install_from_row).transpose()
    }

    /// Lists all managed skill installs for one agent.
    pub async fn list_agent_skill_installs(
        &self,
        agent_id: Uuid,
    ) -> Result<Vec<AgentSkillInstall>, StoreError> {
        let rows = query(&agent_skill_install_select(
            "WHERE install.agent_id = ? ORDER BY install.installed_at, install.id",
        ))
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(agent_skill_install_from_row).collect()
    }

    /// Lists all agents that should be refreshed after a library update.
    pub async fn list_skill_library_installs(
        &self,
        library_id: Uuid,
    ) -> Result<Vec<AgentSkillInstall>, StoreError> {
        let rows = query(&agent_skill_install_select(
            "WHERE install.library_id = ? ORDER BY install.installed_at, install.id",
        ))
        .bind(library_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(agent_skill_install_from_row).collect()
    }

    /// Removes one install record owned by an agent.
    pub async fn delete_agent_skill_install(
        &self,
        agent_id: Uuid,
        install_id: Uuid,
    ) -> Result<(), StoreError> {
        let result = query("DELETE FROM agent_skill_installs WHERE id = ? AND agent_id = ?")
            .bind(install_id)
            .bind(agent_id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(StoreError::SkillInstallNotFound(install_id));
        }
        Ok(())
    }
}

async fn insert_skill_files(
    transaction: &mut sqlx_core::transaction::Transaction<'_, sqlx_sqlite::Sqlite>,
    library_id: Uuid,
    files: &[SkillLibraryFile],
) -> Result<(), StoreError> {
    for file in files {
        query(
            "INSERT INTO skill_library_files (library_id, rel_path, mode, content, size) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(library_id)
        .bind(&file.rel_path)
        .bind(file.mode)
        .bind(&file.content)
        .bind(file.size)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

fn validate_skill_name(name: &str) -> Result<(), StoreError> {
    if name.is_empty()
        || name.len() > MAX_SKILL_NAME_LEN
        || !name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        return Err(StoreError::InvalidSkillName(name.to_owned()));
    }
    Ok(())
}

fn validate_skill_files(files: &[SkillLibraryFile]) -> Result<(), StoreError> {
    if files.is_empty() {
        return Err(StoreError::InvalidSkillFilePath(
            "skill contains no files".to_owned(),
        ));
    }
    for file in files {
        validate_skill_file_path(&file.rel_path)?;
        if file.size != file.content.len() as i64 {
            return Err(StoreError::InvalidSkillFileSize {
                path: file.rel_path.clone(),
                declared: file.size,
                actual: file.content.len(),
            });
        }
    }
    Ok(())
}

fn validate_skill_file_path(path: &str) -> Result<(), StoreError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.ends_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path.contains('\\')
    {
        return Err(StoreError::InvalidSkillFilePath(path.to_owned()));
    }
    Ok(())
}

fn validate_install_path(path: &str) -> Result<(), StoreError> {
    validate_skill_file_path(path)
}

fn skill_total_bytes(files: &[SkillLibraryFile]) -> Result<i64, StoreError> {
    let total = files
        .iter()
        .try_fold(0_usize, |total, file| total.checked_add(file.content.len()));
    let Some(total) = total else {
        return Err(StoreError::SkillLibraryTooLarge {
            bytes: usize::MAX,
            limit: MAX_SKILL_BYTES,
        });
    };
    if total > MAX_SKILL_BYTES {
        return Err(StoreError::SkillLibraryTooLarge {
            bytes: total,
            limit: MAX_SKILL_BYTES,
        });
    }
    Ok(total as i64)
}

fn skill_library_from_row(row: SqliteRow) -> Result<SkillLibraryEntry, StoreError> {
    Ok(SkillLibraryEntry {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        display_name: row.try_get("display_name")?,
        description: row.try_get("description")?,
        user_invocable: row.try_get("user_invocable")?,
        source_kind: row.try_get("source_kind")?,
        source_url: row.try_get("source_url")?,
        source_subpath: row.try_get("source_subpath")?,
        source_ref: row.try_get("source_ref")?,
        total_bytes: row.try_get("total_bytes")?,
        file_count: row.try_get("file_count")?,
        imported_at: row.try_get("imported_at")?,
        updated_at: row.try_get("updated_at")?,
        in_use_count: row.try_get("in_use_count")?,
    })
}

fn skill_file_from_row(row: SqliteRow) -> Result<SkillLibraryFile, StoreError> {
    Ok(SkillLibraryFile {
        rel_path: row.try_get("rel_path")?,
        mode: row.try_get("mode")?,
        content: row.try_get("content")?,
        size: row.try_get("size")?,
    })
}

fn agent_skill_install_select(suffix: &str) -> String {
    format!(
        "SELECT install.id, install.agent_id, install.library_id, install.install_path, \
         install.installed_at, library.name AS library_name, library.source_url, \
         library.source_ref FROM agent_skill_installs install \
         JOIN skill_library library ON library.id = install.library_id {suffix}"
    )
}

fn agent_skill_install_from_row(row: SqliteRow) -> Result<AgentSkillInstall, StoreError> {
    Ok(AgentSkillInstall {
        id: row.try_get("id")?,
        agent_id: row.try_get("agent_id")?,
        library_id: row.try_get("library_id")?,
        install_path: row.try_get("install_path")?,
        installed_at: row.try_get("installed_at")?,
        library_name: row.try_get("library_name")?,
        source_url: row.try_get("source_url")?,
        source_ref: row.try_get("source_ref")?,
    })
}
