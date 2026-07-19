use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use cocli_store::SkillLibraryFile;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::skill_governance::sha256_hex;
use crate::GovernanceSkillTarget;

const MAX_ARTIFACT_FILES: usize = 5_000;
const MAX_ARTIFACT_BYTES: usize = 50 * 1024 * 1024;
const MANAGED_MARKER: &str = ".cocli-managed";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactFile {
    pub relative_path: String,
    pub mode: u32,
    pub content: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactBundle {
    pub files: Vec<ArtifactFile>,
    pub canonical_source: Option<PathBuf>,
    pub content_digest: String,
    pub manifest_digest: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MutationMode {
    Copy,
    Symlink,
    Remove,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupManifest {
    pub schema_version: u32,
    pub original_path: String,
    pub original_type: String,
    pub original_mode: Option<u32>,
    pub original_symlink_target: Option<String>,
    pub content_digest: Option<String>,
    pub manifest_digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MutationReceipt {
    pub target: String,
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub backup_ref: Option<String>,
    pub quarantine_ref: Option<String>,
    pub backup_manifest_ref: String,
    pub staging_ref: Option<String>,
    pub installation_mode: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedMutation {
    target: GovernanceSkillTarget,
    mode: MutationMode,
    receipt: MutationReceipt,
}

impl PreparedMutation {
    pub(crate) fn receipt(&self) -> &MutationReceipt {
        &self.receipt
    }
}

pub(crate) fn load_local_artifact(root: &Path) -> Result<ArtifactBundle, String> {
    let canonical = root
        .canonicalize()
        .map_err(|error| safe_io_error("canonicalize local Skill source", &error))?;
    if !canonical.is_dir() {
        return Err("local Skill source is not a directory".to_owned());
    }
    let mut files = Vec::new();
    walk_local_artifact(&canonical, &canonical, &mut files)?;
    finish_artifact(files, Some(canonical))
}

pub(crate) fn load_vendored_artifact(files: &[SkillLibraryFile]) -> Result<ArtifactBundle, String> {
    let mut artifact_files = Vec::with_capacity(files.len());
    for file in files {
        validate_relative_path(&file.rel_path)?;
        artifact_files.push(ArtifactFile {
            relative_path: file.rel_path.clone(),
            mode: normalized_mode(file.mode as u32),
            content: file.content.clone(),
        });
    }
    finish_artifact(artifact_files, None)
}

fn finish_artifact(
    mut files: Vec<ArtifactFile>,
    canonical_source: Option<PathBuf>,
) -> Result<ArtifactBundle, String> {
    if files.len() > MAX_ARTIFACT_FILES {
        return Err("Skill artifact contains too many files".to_owned());
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if files
        .windows(2)
        .any(|pair| pair[0].relative_path == pair[1].relative_path)
    {
        return Err("Skill artifact contains duplicate file paths".to_owned());
    }
    let total_bytes = files.iter().try_fold(0_usize, |total, file| {
        total
            .checked_add(file.content.len())
            .ok_or_else(|| "Skill artifact size overflow".to_owned())
    })?;
    if total_bytes > MAX_ARTIFACT_BYTES {
        return Err("Skill artifact exceeds the governance size limit".to_owned());
    }
    let manifest = files
        .iter()
        .find(|file| file.relative_path == "SKILL.md")
        .ok_or_else(|| "Skill artifact has no root SKILL.md".to_owned())?;
    let manifest_digest = digest_bytes(&manifest.content);
    let mut canonical_bytes = Vec::with_capacity(total_bytes + files.len() * 64);
    for file in &files {
        canonical_bytes.extend_from_slice(file.relative_path.as_bytes());
        canonical_bytes.push(0);
        canonical_bytes.extend_from_slice(format!("{:o}", file.mode).as_bytes());
        canonical_bytes.push(0);
        canonical_bytes.extend_from_slice(file.content.len().to_string().as_bytes());
        canonical_bytes.push(0);
        canonical_bytes.extend_from_slice(&file.content);
        canonical_bytes.push(0xff);
    }
    Ok(ArtifactBundle {
        files,
        canonical_source,
        content_digest: digest_bytes(&canonical_bytes),
        manifest_digest,
    })
}

fn walk_local_artifact(
    root: &Path,
    current: &Path,
    files: &mut Vec<ArtifactFile>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| safe_io_error("read local Skill source", &error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| safe_io_error("read local Skill source", &error))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) == Some(MANAGED_MARKER) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| safe_io_error("inspect local Skill source", &error))?;
        if metadata.file_type().is_symlink() {
            return Err(
                "local Skill artifacts containing symlinks require manual review".to_owned(),
            );
        }
        if metadata.is_dir() {
            walk_local_artifact(root, &path, files)?;
            continue;
        }
        if !metadata.is_file() {
            return Err("local Skill artifact contains an unsupported file type".to_owned());
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "local Skill source escaped its canonical root".to_owned())?;
        let relative = relative_path_string(relative)?;
        let content = fs::read(&path)
            .map_err(|error| safe_io_error("read local Skill artifact file", &error))?;
        files.push(ArtifactFile {
            relative_path: relative,
            mode: file_mode(&metadata),
            content,
        });
        if files.len() > MAX_ARTIFACT_FILES {
            return Err("Skill artifact contains too many files".to_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn apply_atomic_mutation(
    target: &GovernanceSkillTarget,
    mode: MutationMode,
    artifact: Option<&ArtifactBundle>,
    run_id: Uuid,
    action_id: Uuid,
) -> Result<MutationReceipt, String> {
    let prepared = prepare_atomic_mutation(target, mode, artifact, run_id, action_id)?;
    let result = backup_atomic_mutation(&prepared)
        .and_then(|()| stage_atomic_mutation(&prepared, artifact))
        .and_then(|()| activate_atomic_mutation(&prepared));
    if let Err(error) = result {
        let _ = rollback_atomic_mutation(prepared.receipt());
        return Err(error);
    }
    Ok(prepared.receipt)
}

pub(crate) fn prepare_atomic_mutation(
    target: &GovernanceSkillTarget,
    mode: MutationMode,
    artifact: Option<&ArtifactBundle>,
    run_id: Uuid,
    action_id: Uuid,
) -> Result<PreparedMutation, String> {
    prepare_safe_target(target)?;
    let before_fingerprint = fingerprint_path(&target.entry_path)?;
    let control_dir = target
        .scope_root
        .join(".cocli/governance/runs")
        .join(run_id.to_string())
        .join(action_id.to_string());
    create_dir_all_no_symlink(&target.scope_root, &control_dir)?;
    let backup_path = control_dir.join("backup-entry");
    let manifest_path = control_dir.join("backup-manifest.json");
    let backup_manifest = inspect_backup(&target.entry_path)?;
    write_json_sync(&manifest_path, &backup_manifest)?;
    let had_target = before_fingerprint != "missing";
    let expected_after = match mode {
        MutationMode::Copy => required_artifact(artifact)?.content_digest.clone(),
        MutationMode::Symlink => symlink_fingerprint_for_artifact(required_artifact(artifact)?)?,
        MutationMode::Remove => "missing".to_owned(),
    };
    let staging_ref = match mode {
        MutationMode::Copy => Some(copy_staging_path(target, action_id)?),
        MutationMode::Symlink => Some(symlink_staging_path(target, action_id)?),
        MutationMode::Remove => None,
    };
    Ok(PreparedMutation {
        target: target.clone(),
        mode,
        receipt: MutationReceipt {
            target: target.entry_path.to_string_lossy().into_owned(),
            before_fingerprint,
            after_fingerprint: expected_after,
            backup_ref: had_target.then(|| backup_path.to_string_lossy().into_owned()),
            quarantine_ref: (mode == MutationMode::Remove && had_target)
                .then(|| backup_path.to_string_lossy().into_owned()),
            backup_manifest_ref: manifest_path.to_string_lossy().into_owned(),
            staging_ref: staging_ref.map(|path| path.to_string_lossy().into_owned()),
            installation_mode: match mode {
                MutationMode::Copy => "copy",
                MutationMode::Symlink => "symlink",
                MutationMode::Remove => "remove",
            }
            .to_owned(),
        },
    })
}

pub(crate) fn backup_atomic_mutation(prepared: &PreparedMutation) -> Result<(), String> {
    let current = fingerprint_path(&prepared.target.entry_path)?;
    if current != prepared.receipt.before_fingerprint {
        return Err("governed Skill changed before backup CAS".to_owned());
    }
    if let Some(backup_ref) = &prepared.receipt.backup_ref {
        let backup_path = Path::new(backup_ref);
        if fs::symlink_metadata(backup_path).is_ok() {
            return Err("governed Skill backup path already exists".to_owned());
        }
        fs::rename(&prepared.target.entry_path, backup_path)
            .map_err(|error| safe_io_error("move governed Skill to backup", &error))?;
        let backed_up = fingerprint_path(backup_path)?;
        if backed_up != prepared.receipt.before_fingerprint {
            let _ = fs::rename(backup_path, &prepared.target.entry_path);
            return Err("governed Skill changed during backup CAS".to_owned());
        }
    }
    Ok(())
}

pub(crate) fn stage_atomic_mutation(
    prepared: &PreparedMutation,
    artifact: Option<&ArtifactBundle>,
) -> Result<(), String> {
    match prepared.mode {
        MutationMode::Copy => write_copy_staging(
            required_artifact(artifact)?,
            prepared
                .receipt
                .staging_ref
                .as_deref()
                .ok_or_else(|| "copy staging path is unavailable".to_owned())?,
        ),
        MutationMode::Symlink => write_symlink_staging(
            required_artifact(artifact)?,
            prepared
                .receipt
                .staging_ref
                .as_deref()
                .ok_or_else(|| "symlink staging path is unavailable".to_owned())?,
        ),
        MutationMode::Remove => Ok(()),
    }
}

pub(crate) fn activate_atomic_mutation(prepared: &PreparedMutation) -> Result<(), String> {
    if let Some(staging_ref) = &prepared.receipt.staging_ref {
        fs::rename(staging_ref, &prepared.target.entry_path)
            .map_err(|error| safe_io_error("atomically activate governed Skill", &error))?;
    }
    sync_directory(&prepared.target.search_root)?;
    let after_fingerprint = fingerprint_path(&prepared.target.entry_path)?;
    if after_fingerprint != prepared.receipt.after_fingerprint {
        return Err("governed Skill failed post-write fingerprint verification".to_owned());
    }
    Ok(())
}

pub(crate) fn rollback_atomic_mutation(receipt: &MutationReceipt) -> Result<(), String> {
    let target = PathBuf::from(&receipt.target);
    if let Some(staging_ref) = &receipt.staging_ref {
        remove_any(Path::new(staging_ref))?;
    }
    let current = fingerprint_path(&target)?;
    if current == receipt.before_fingerprint {
        return Ok(());
    }
    let interrupted_after_backup = current == "missing"
        && (receipt.before_fingerprint == "missing"
            || receipt
                .backup_ref
                .as_deref()
                .is_some_and(|backup| fs::symlink_metadata(backup).is_ok()));
    if current != receipt.after_fingerprint && !interrupted_after_backup {
        return Err("rollback blocked because the governed Skill changed after apply".to_owned());
    }
    let control_dir = PathBuf::from(&receipt.backup_manifest_ref)
        .parent()
        .ok_or_else(|| "backup manifest has no parent".to_owned())?
        .to_path_buf();
    let rollback_new = control_dir.join("rollback-new-entry");
    if current == receipt.after_fingerprint && current != "missing" {
        if fs::symlink_metadata(&rollback_new).is_ok() {
            return Err("rollback quarantine path already exists".to_owned());
        }
        fs::rename(&target, &rollback_new)
            .map_err(|error| safe_io_error("quarantine applied Skill for rollback", &error))?;
    }
    if let Some(backup) = &receipt.backup_ref {
        if let Err(error) = fs::rename(backup, &target) {
            if rollback_new.exists() {
                let _ = fs::rename(&rollback_new, &target);
            }
            return Err(safe_io_error("restore governed Skill backup", &error));
        }
    }
    let restored = fingerprint_path(&target)?;
    if restored != receipt.before_fingerprint {
        return Err("rollback verification did not restore the expected fingerprint".to_owned());
    }
    sync_directory(
        target
            .parent()
            .ok_or_else(|| "governed Skill target has no parent".to_owned())?,
    )?;
    Ok(())
}

pub(crate) fn fingerprint_path(path: &Path) -> Result<String, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok("missing".to_owned()),
        Err(error) => return Err(safe_io_error("inspect governed Skill", &error)),
    };
    if metadata.file_type().is_symlink() {
        let link = fs::read_link(path)
            .map_err(|error| safe_io_error("read governed Skill symlink", &error))?;
        let resolved = if link.is_absolute() {
            link
        } else {
            path.parent()
                .ok_or_else(|| "governed Skill symlink has no parent".to_owned())?
                .join(link)
        };
        let canonical = resolved
            .canonicalize()
            .map_err(|error| safe_io_error("resolve governed Skill symlink", &error))?;
        let artifact = load_local_artifact(&canonical)?;
        return symlink_fingerprint_for_artifact(&artifact);
    }
    if metadata.is_dir() {
        return Ok(load_local_artifact(path)?.content_digest);
    }
    Err("governed Skill target has an unsupported file type".to_owned())
}

pub(crate) fn is_safe_removal_candidate(path: &Path) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(error) => {
            return Err(safe_io_error(
                "inspect governed Skill removal target",
                &error,
            ))
        }
    };
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    if !metadata.is_dir() {
        return Ok(false);
    }
    Ok(fs::symlink_metadata(path.join(MANAGED_MARKER))
        .is_ok_and(|marker| marker.is_file() && !marker.file_type().is_symlink()))
}

fn inspect_backup(path: &Path) -> Result<BackupManifest, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(safe_io_error("inspect governed Skill backup", &error)),
    };
    let (original_type, mode, link, content, manifest) = match metadata {
        None => ("missing".to_owned(), None, None, None, None),
        Some(metadata) if metadata.file_type().is_symlink() => {
            let link = fs::read_link(path)
                .map_err(|error| safe_io_error("read governed Skill symlink", &error))?;
            (
                "symlink".to_owned(),
                Some(file_mode(&metadata)),
                Some(link.to_string_lossy().into_owned()),
                Some(fingerprint_path(path)?),
                None,
            )
        }
        Some(metadata) if metadata.is_dir() => {
            let artifact = load_local_artifact(path)?;
            (
                "directory".to_owned(),
                Some(file_mode(&metadata)),
                None,
                Some(artifact.content_digest),
                Some(artifact.manifest_digest),
            )
        }
        Some(_) => return Err("governed Skill target has an unsupported file type".to_owned()),
    };
    Ok(BackupManifest {
        schema_version: 1,
        original_path: path.to_string_lossy().into_owned(),
        original_type,
        original_mode: mode,
        original_symlink_target: link,
        content_digest: content,
        manifest_digest: manifest,
    })
}

fn prepare_safe_target(target: &GovernanceSkillTarget) -> Result<(), String> {
    if !target.search_root.starts_with(&target.scope_root)
        || target.entry_path.parent() != Some(target.search_root.as_path())
    {
        return Err("Runtime Skill target is outside its canonical scope".to_owned());
    }
    let name = target
        .entry_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Runtime Skill target name is invalid".to_owned())?;
    validate_name(name)?;
    create_dir_all_no_symlink(&target.scope_root, &target.search_root)?;
    let canonical_scope = target
        .scope_root
        .canonicalize()
        .map_err(|error| safe_io_error("canonicalize governance scope", &error))?;
    let canonical_search = target
        .search_root
        .canonicalize()
        .map_err(|error| safe_io_error("canonicalize Runtime Skill root", &error))?;
    if !canonical_search.starts_with(canonical_scope) {
        return Err("Runtime Skill search root escaped its canonical scope".to_owned());
    }
    Ok(())
}

fn create_dir_all_no_symlink(scope_root: &Path, path: &Path) -> Result<(), String> {
    if !path.starts_with(scope_root) {
        return Err("governance directory escaped its scope".to_owned());
    }
    fs::create_dir_all(scope_root)
        .map_err(|error| safe_io_error("create governance scope", &error))?;
    reject_symlink(scope_root)?;
    let relative = path
        .strip_prefix(scope_root)
        .map_err(|_| "governance directory escaped its scope".to_owned())?;
    let mut current = scope_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err("governance directory contains unsafe components".to_owned());
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("governance directory contains a symlink".to_owned());
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err("governance directory component is not a directory".to_owned());
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(&current)
                .map_err(|error| safe_io_error("create governance directory", &error))?,
            Err(error) => return Err(safe_io_error("inspect governance directory", &error)),
        }
    }
    Ok(())
}

fn copy_staging_path(target: &GovernanceSkillTarget, action_id: Uuid) -> Result<PathBuf, String> {
    Ok(target.search_root.join(format!(
        ".{}.staging.{action_id}",
        target
            .entry_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "Runtime Skill target name is invalid".to_owned())?
    )))
}

fn symlink_staging_path(
    target: &GovernanceSkillTarget,
    action_id: Uuid,
) -> Result<PathBuf, String> {
    Ok(target.search_root.join(format!(
        ".{}.symlink.{action_id}",
        target
            .entry_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "Runtime Skill target name is invalid".to_owned())?
    )))
}

fn write_copy_staging(artifact: &ArtifactBundle, staging_ref: &str) -> Result<(), String> {
    let staging = PathBuf::from(staging_ref);
    if fs::symlink_metadata(&staging).is_ok() {
        return Err("Skill staging path already exists".to_owned());
    }
    fs::create_dir(&staging)
        .map_err(|error| safe_io_error("create Skill staging directory", &error))?;
    for artifact_file in &artifact.files {
        let path = safe_join(&staging, &artifact_file.relative_path)?;
        let parent = path
            .parent()
            .ok_or_else(|| "staged Skill file has no parent".to_owned())?;
        fs::create_dir_all(parent)
            .map_err(|error| safe_io_error("create staged Skill directory", &error))?;
        let mut file = File::create(&path)
            .map_err(|error| safe_io_error("create staged Skill file", &error))?;
        file.write_all(&artifact_file.content)
            .map_err(|error| safe_io_error("write staged Skill file", &error))?;
        file.sync_all()
            .map_err(|error| safe_io_error("sync staged Skill file", &error))?;
        set_mode(&path, artifact_file.mode)?;
    }
    let mut marker = File::create(staging.join(MANAGED_MARKER))
        .map_err(|error| safe_io_error("create governed Skill marker", &error))?;
    marker
        .write_all(b"cocli-skill-governance-v1\n")
        .map_err(|error| safe_io_error("write governed Skill marker", &error))?;
    marker
        .sync_all()
        .map_err(|error| safe_io_error("sync governed Skill marker", &error))?;
    sync_directory(&staging)
}

#[cfg(unix)]
fn write_symlink_staging(artifact: &ArtifactBundle, staging_ref: &str) -> Result<(), String> {
    use std::os::unix::fs::symlink;

    let source = artifact
        .canonical_source
        .as_deref()
        .ok_or_else(|| "vendored artifacts cannot be installed as symlinks".to_owned())?;
    let staging = PathBuf::from(staging_ref);
    if fs::symlink_metadata(&staging).is_ok() {
        return Err("Skill symlink staging path already exists".to_owned());
    }
    symlink(source, &staging)
        .map_err(|error| safe_io_error("create temporary governed Skill symlink", &error))
}

#[cfg(not(unix))]
fn write_symlink_staging(_artifact: &ArtifactBundle, _staging_ref: &str) -> Result<(), String> {
    Err("automatic Skill symlink installation is unsupported on this platform".to_owned())
}

fn write_json_sync(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| "serialize governed Skill backup manifest".to_owned())?;
    let mut file = File::create(path)
        .map_err(|error| safe_io_error("create governed Skill backup manifest", &error))?;
    file.write_all(&bytes)
        .map_err(|error| safe_io_error("write governed Skill backup manifest", &error))?;
    file.write_all(b"\n")
        .map_err(|error| safe_io_error("write governed Skill backup manifest", &error))?;
    file.sync_all()
        .map_err(|error| safe_io_error("sync governed Skill backup manifest", &error))
}

fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| safe_io_error("sync governance directory", &error))
}

fn required_artifact(artifact: Option<&ArtifactBundle>) -> Result<&ArtifactBundle, String> {
    artifact.ok_or_else(|| "governed Skill mutation requires a verified artifact".to_owned())
}

fn remove_any(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(safe_io_error("inspect governed Skill path", &error)),
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
            .map_err(|error| safe_io_error("remove governed Skill directory", &error))
    } else {
        fs::remove_file(path).map_err(|error| safe_io_error("remove governed Skill path", &error))
    }
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_relative_path(relative)?;
    Ok(root.join(relative))
}

fn validate_relative_path(relative: &str) -> Result<(), String> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("Skill artifact file path is unsafe".to_owned());
    }
    Ok(())
}

fn relative_path_string(path: &Path) -> Result<String, String> {
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("Skill artifact path is unsafe".to_owned());
    }
    Ok(path
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 80
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || matches!(name, "." | "..")
    {
        return Err("governed Skill name is invalid".to_owned());
    }
    Ok(())
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

fn symlink_fingerprint_for_artifact(artifact: &ArtifactBundle) -> Result<String, String> {
    let target = artifact
        .canonical_source
        .as_deref()
        .ok_or_else(|| "vendored artifacts cannot be installed as symlinks".to_owned())?;
    Ok(digest_bytes(
        format!(
            "symlink:{}:{}:{}",
            target.to_string_lossy(),
            artifact.content_digest,
            artifact.manifest_digest
        )
        .as_bytes(),
    ))
}

fn reject_symlink(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| safe_io_error("inspect governance scope", &error))?;
    if metadata.file_type().is_symlink() {
        return Err("governance scope is a symlink".to_owned());
    }
    Ok(())
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    normalized_mode(metadata.permissions().mode())
}

#[cfg(not(unix))]
fn file_mode(_metadata: &fs::Metadata) -> u32 {
    0o644
}

fn normalized_mode(mode: u32) -> u32 {
    let mode = mode & 0o777;
    if mode == 0 {
        0o644
    } else {
        mode
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(normalized_mode(mode)))
        .map_err(|error| safe_io_error("set governed Skill file mode", &error))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<(), String> {
    Ok(())
}

fn safe_io_error(action: &str, error: &io::Error) -> String {
    format!("{action}: {}", error.kind())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_artifact(root: &Path, body: &str) {
        fs::create_dir_all(root).expect("artifact root");
        fs::write(root.join("SKILL.md"), body).expect("manifest");
        fs::create_dir_all(root.join("references")).expect("references");
        fs::write(root.join("references/info.md"), "reference").expect("reference");
    }

    fn target(root: &Path, name: &str) -> GovernanceSkillTarget {
        let scope_root = root.join("agent");
        let search_root = scope_root.join(".codex/skills");
        GovernanceSkillTarget {
            entry_path: search_root.join(name),
            search_root,
            scope_root,
        }
    }

    #[test]
    fn local_artifact_digest_is_stable_and_aliases_canonicalize() {
        let temp = tempdir().expect("temp");
        let artifact = temp.path().join("artifact");
        write_artifact(&artifact, "---\nname: reviewer\n---\n");
        let first = load_local_artifact(&artifact).expect("artifact");
        let second = load_local_artifact(&artifact.join("../artifact")).expect("alias");
        assert_eq!(first.content_digest, second.content_digest);
        assert_eq!(first.manifest_digest, second.manifest_digest);
        assert_eq!(first.canonical_source, second.canonical_source);
    }

    #[test]
    fn traversal_and_source_symlinks_are_blocked() {
        let files = vec![SkillLibraryFile {
            rel_path: "../escape".to_owned(),
            mode: 0o644,
            content: Vec::new(),
            size: 0,
        }];
        assert!(load_vendored_artifact(&files)
            .expect_err("traversal")
            .contains("unsafe"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let temp = tempdir().expect("temp");
            let artifact = temp.path().join("artifact");
            write_artifact(&artifact, "manifest");
            symlink("SKILL.md", artifact.join("alias.md")).expect("symlink");
            assert!(load_local_artifact(&artifact)
                .expect_err("source symlink")
                .contains("manual review"));
        }
    }

    #[test]
    fn copy_update_remove_and_rollback_are_atomic_and_cas_safe() {
        let temp = tempdir().expect("temp");
        let source_one = temp.path().join("one");
        let source_two = temp.path().join("two");
        write_artifact(&source_one, "one");
        write_artifact(&source_two, "two");
        let first = load_local_artifact(&source_one).expect("first");
        let second = load_local_artifact(&source_two).expect("second");
        let target = target(temp.path(), "reviewer");

        let install = apply_atomic_mutation(
            &target,
            MutationMode::Copy,
            Some(&first),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect("install");
        assert_eq!(
            fingerprint_path(&target.entry_path).expect("fingerprint"),
            first.content_digest
        );

        let update = apply_atomic_mutation(
            &target,
            MutationMode::Copy,
            Some(&second),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect("update");
        assert_eq!(
            fingerprint_path(&target.entry_path).expect("fingerprint"),
            second.content_digest
        );
        rollback_atomic_mutation(&update).expect("rollback update");
        assert_eq!(
            fingerprint_path(&target.entry_path).expect("fingerprint"),
            first.content_digest
        );

        fs::write(target.entry_path.join("SKILL.md"), "user edit").expect("user edit");
        assert!(rollback_atomic_mutation(&install)
            .expect_err("CAS conflict")
            .contains("changed after apply"));

        let remove = apply_atomic_mutation(
            &target,
            MutationMode::Remove,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect("remove");
        assert_eq!(
            fingerprint_path(&target.entry_path).expect("missing"),
            "missing"
        );
        assert!(remove.quarantine_ref.is_some());
        rollback_atomic_mutation(&remove).expect("rollback remove");
        assert!(target.entry_path.exists());

        let corrupted_backup = apply_atomic_mutation(
            &target,
            MutationMode::Copy,
            Some(&second),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect("update with backup");
        fs::write(
            Path::new(
                corrupted_backup
                    .backup_ref
                    .as_deref()
                    .expect("backup reference"),
            )
            .join("SKILL.md"),
            "corrupted backup",
        )
        .expect("corrupt backup fixture");
        assert!(rollback_atomic_mutation(&corrupted_backup)
            .expect_err("corrupt backup must fail verification")
            .contains("expected fingerprint"));
    }

    #[test]
    fn prepared_receipt_recovers_crashes_after_backup_staging_and_activation() {
        let temp = tempdir().expect("temp");
        let source_one = temp.path().join("one");
        let source_two = temp.path().join("two");
        write_artifact(&source_one, "one");
        write_artifact(&source_two, "two");
        let first = load_local_artifact(&source_one).expect("first");
        let second = load_local_artifact(&source_two).expect("second");
        let target = target(temp.path(), "crash-safe");
        apply_atomic_mutation(
            &target,
            MutationMode::Copy,
            Some(&first),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect("initial install");

        let after_backup = prepare_atomic_mutation(
            &target,
            MutationMode::Copy,
            Some(&second),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect("prepare backup crash");
        backup_atomic_mutation(&after_backup).expect("backup boundary");
        rollback_atomic_mutation(after_backup.receipt()).expect("recover after backup");
        assert_eq!(
            fingerprint_path(&target.entry_path).expect("restored first"),
            first.content_digest
        );

        let after_staging = prepare_atomic_mutation(
            &target,
            MutationMode::Copy,
            Some(&second),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect("prepare staging crash");
        backup_atomic_mutation(&after_staging).expect("backup before staging");
        stage_atomic_mutation(&after_staging, Some(&second)).expect("staging boundary");
        rollback_atomic_mutation(after_staging.receipt()).expect("recover after staging");
        assert_eq!(
            fingerprint_path(&target.entry_path).expect("restored after staging"),
            first.content_digest
        );
        assert!(!Path::new(
            after_staging
                .receipt()
                .staging_ref
                .as_deref()
                .expect("staging ref")
        )
        .exists());

        let after_activation = prepare_atomic_mutation(
            &target,
            MutationMode::Copy,
            Some(&second),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect("prepare activation crash");
        backup_atomic_mutation(&after_activation).expect("backup before activation");
        stage_atomic_mutation(&after_activation, Some(&second)).expect("stage before activation");
        activate_atomic_mutation(&after_activation).expect("activation boundary");
        rollback_atomic_mutation(after_activation.receipt()).expect("recover after activation");
        assert_eq!(
            fingerprint_path(&target.entry_path).expect("restored after activation"),
            first.content_digest
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_install_is_atomic_and_scope_escape_is_blocked() {
        let temp = tempdir().expect("temp");
        let source = temp.path().join("source");
        write_artifact(&source, "manifest");
        let artifact = load_local_artifact(&source).expect("artifact");
        let target = target(temp.path(), "linked");
        let receipt = apply_atomic_mutation(
            &target,
            MutationMode::Symlink,
            Some(&artifact),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect("symlink");
        assert!(fs::symlink_metadata(&target.entry_path)
            .expect("metadata")
            .file_type()
            .is_symlink());
        rollback_atomic_mutation(&receipt).expect("rollback");

        let escaped = GovernanceSkillTarget {
            scope_root: temp.path().join("agent"),
            search_root: temp.path().join("outside"),
            entry_path: temp.path().join("outside/skill"),
        };
        assert!(apply_atomic_mutation(
            &escaped,
            MutationMode::Copy,
            Some(&artifact),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect_err("escape")
        .contains("outside"));

        let blocked_scope = temp.path().join("blocked-scope");
        fs::write(&blocked_scope, "not a directory").expect("blocked scope fixture");
        let unavailable = GovernanceSkillTarget {
            scope_root: blocked_scope.clone(),
            search_root: blocked_scope.join(".codex/skills"),
            entry_path: blocked_scope.join(".codex/skills/reviewer"),
        };
        assert!(apply_atomic_mutation(
            &unavailable,
            MutationMode::Copy,
            Some(&artifact),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect_err("unwritable scope must fail before mutation")
        .contains("create governance scope"));
    }

    #[cfg(unix)]
    #[test]
    fn broken_and_circular_target_symlinks_fail_closed() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("temp");
        let broken = temp.path().join("broken");
        symlink("missing", &broken).expect("broken symlink");
        assert!(fingerprint_path(&broken)
            .expect_err("broken link must not fingerprint")
            .contains("resolve governed Skill symlink"));

        let first = temp.path().join("first");
        let second = temp.path().join("second");
        symlink("second", &first).expect("first circular link");
        symlink("first", second).expect("second circular link");
        assert!(fingerprint_path(&first)
            .expect_err("circular link must not fingerprint")
            .contains("resolve governed Skill symlink"));
    }
}
