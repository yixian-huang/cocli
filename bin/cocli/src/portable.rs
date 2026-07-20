use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use chrono::{SecondsFormat, Utc};
use cocli_store::{PortableInventory, Store, CURRENT_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

const BUNDLE_FORMAT: &str = "cocli-portable-backup";
const BUNDLE_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "manifest.json";
const STATE_FILE: &str = "state.sqlite3";
const CHECKSUMS_FILE: &str = "checksums.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BundleInclusions {
    pub(crate) state_snapshot: bool,
    pub(crate) managed_workspaces: bool,
    pub(crate) os_credentials: bool,
    pub(crate) bridge_tokens: bool,
    pub(crate) live_execution_state: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BundleManifest {
    pub(crate) bundle_format: String,
    pub(crate) bundle_version: u32,
    pub(crate) app_version: String,
    pub(crate) schema_version: i64,
    #[serde(default = "default_inventory_version")]
    pub(crate) inventory_version: u32,
    pub(crate) created_at: String,
    pub(crate) inventory: PortableInventory,
    pub(crate) inclusions: BundleInclusions,
    pub(crate) state_file: String,
    pub(crate) state_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BundleChecksums {
    algorithm: String,
    files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct BundlePreflight {
    pub(crate) manifest: BundleManifest,
    pub(crate) unavailable_provider_keys: Vec<String>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct BundleRestore {
    pub(crate) installation_id: String,
    pub(crate) inventory: PortableInventory,
    pub(crate) unavailable_provider_keys: Vec<String>,
    pub(crate) previous_state: Option<PathBuf>,
}

pub(crate) async fn create_bundle(store: &Store, output: &Path) -> Result<BundleManifest> {
    if output.exists() {
        bail!(
            "portable backup destination already exists: {}",
            output.display()
        );
    }
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    tokio::fs::create_dir_all(parent)
        .await
        .context("failed to create portable backup parent directory")?;
    let staging = parent.join(format!(
        ".{}-portable-backup-{}",
        output
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("cocli"),
        nonce()
    ));
    tokio::fs::create_dir(&staging)
        .await
        .context("failed to create portable backup staging directory")?;

    let result = async {
        let state_path = staging.join(STATE_FILE);
        let inventory = store
            .export_portable_snapshot(&state_path)
            .await
            .context("failed to export sanitized portable state")?;
        let state_sha256 = sha256_file(&state_path).await?;
        let manifest = BundleManifest {
            bundle_format: BUNDLE_FORMAT.to_owned(),
            bundle_version: BUNDLE_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            schema_version: inventory.schema_version,
            inventory_version: default_inventory_version(),
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            inventory,
            inclusions: BundleInclusions {
                state_snapshot: true,
                managed_workspaces: false,
                os_credentials: false,
                bridge_tokens: false,
                live_execution_state: false,
            },
            state_file: STATE_FILE.to_owned(),
            state_sha256,
        };
        let manifest_path = staging.join(MANIFEST_FILE);
        write_json(&manifest_path, &manifest).await?;
        let mut files = BTreeMap::new();
        files.insert(STATE_FILE.to_owned(), manifest.state_sha256.clone());
        files.insert(MANIFEST_FILE.to_owned(), sha256_file(&manifest_path).await?);
        write_json(
            &staging.join(CHECKSUMS_FILE),
            &BundleChecksums {
                algorithm: "sha256".to_owned(),
                files,
            },
        )
        .await?;
        Ok::<_, anyhow::Error>(manifest)
    }
    .await;

    match result {
        Ok(manifest) => {
            tokio::fs::rename(&staging, output)
                .await
                .context("failed to install portable backup bundle")?;
            Ok(manifest)
        }
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(&staging).await;
            Err(error)
        }
    }
}

pub(crate) async fn preflight_bundle(input: &Path) -> Result<BundlePreflight> {
    if !input.is_dir() {
        bail!(
            "portable backup bundle is not a directory: {}",
            input.display()
        );
    }
    let manifest_path = input.join(MANIFEST_FILE);
    let checksums_path = input.join(CHECKSUMS_FILE);
    let manifest: BundleManifest = read_json(&manifest_path).await?;
    let checksums: BundleChecksums = read_json(&checksums_path).await?;
    if manifest.bundle_format != BUNDLE_FORMAT {
        bail!(
            "unsupported portable backup format: {}",
            manifest.bundle_format
        );
    }
    if manifest.bundle_version != BUNDLE_VERSION {
        bail!(
            "unsupported portable backup version: {}",
            manifest.bundle_version
        );
    }
    if manifest.schema_version != manifest.inventory.schema_version {
        bail!("portable backup manifest has inconsistent schema versions");
    }
    if manifest.inventory_version != default_inventory_version() {
        bail!(
            "unsupported portable inventory version: {}",
            manifest.inventory_version
        );
    }
    if manifest.schema_version <= 0 || manifest.schema_version > CURRENT_SCHEMA_VERSION {
        bail!(
            "portable backup schema {} is not supported by schema {}",
            manifest.schema_version,
            CURRENT_SCHEMA_VERSION
        );
    }
    if manifest.state_file != STATE_FILE {
        bail!("portable backup state filename is not supported");
    }
    if checksums.algorithm != "sha256" {
        bail!("portable backup checksum algorithm is not supported");
    }
    let state_path = input.join(STATE_FILE);
    let actual_state_hash = sha256_file(&state_path).await?;
    if actual_state_hash != manifest.state_sha256 {
        bail!("portable backup state checksum does not match manifest");
    }
    require_checksum(&checksums, STATE_FILE, &actual_state_hash)?;
    let actual_manifest_hash = sha256_file(&manifest_path).await?;
    require_checksum(&checksums, MANIFEST_FILE, &actual_manifest_hash)?;

    let unavailable_provider_keys = manifest
        .inventory
        .required_provider_keys
        .iter()
        .filter(|key| !matches!(key.as_str(), "directory" | "git"))
        .cloned()
        .collect::<Vec<_>>();
    let mut warnings = Vec::new();
    if !manifest.inclusions.managed_workspaces {
        warnings.push("managed Workspace materialization is not included".to_owned());
    }
    if manifest.inventory.workspace_binding_hints > 0 {
        warnings.push(
            "source-machine Workspace bindings are hints; this installation starts unbound"
                .to_owned(),
        );
    }
    if !unavailable_provider_keys.is_empty() {
        warnings.push("one or more Workspace Providers are unavailable".to_owned());
    }
    Ok(BundlePreflight {
        manifest,
        unavailable_provider_keys,
        warnings,
    })
}

pub(crate) async fn restore_bundle(data_dir: &Path, input: &Path) -> Result<BundleRestore> {
    let preflight = preflight_bundle(input).await?;
    tokio::fs::create_dir_all(data_dir)
        .await
        .context("failed to create cocli data directory")?;
    let restore_nonce = nonce();
    let staged = data_dir.join(format!(".portable-restore-{restore_nonce}.sqlite3"));
    tokio::fs::copy(input.join(STATE_FILE), &staged)
        .await
        .context("failed to stage portable state")?;
    if let Err(error) =
        verify_staged_state_checksum(&staged, &preflight.manifest.state_sha256).await
    {
        let _ = tokio::fs::remove_file(&staged).await;
        return Err(error);
    }

    let staged_store = match Store::open(&staged).await {
        Ok(store) => store,
        Err(error) => {
            let _ = tokio::fs::remove_file(&staged).await;
            return Err(error).context("portable state is not a valid migratable cocli database");
        }
    };
    let installation_id = match staged_store.prepare_portable_restore().await {
        Ok(installation_id) => installation_id,
        Err(error) => {
            staged_store.close().await;
            let _ = tokio::fs::remove_file(&staged).await;
            return Err(error).context("failed to sanitize staged portable state");
        }
    };
    staged_store.close().await;
    let verified = Store::open(&staged)
        .await
        .context("failed to reopen staged portable state")?;
    if verified.current_installation_id() != installation_id {
        verified.close().await;
        let _ = tokio::fs::remove_file(&staged).await;
        bail!("staged portable state did not retain its fresh installation identity");
    }
    let restored_inventory = verified
        .portable_inventory()
        .await
        .context("failed to verify staged portable inventory")?;
    verified.close().await;
    if !inventory_matches_after_migration(
        preflight.manifest.inventory_version,
        &preflight.manifest.inventory,
        &restored_inventory,
    ) {
        let _ = tokio::fs::remove_file(&staged).await;
        bail!("staged portable inventory does not match the bundle manifest");
    }

    let previous =
        crate::install_staged_database(data_dir, &staged, "pre-portable-restore").await?;
    Ok(BundleRestore {
        installation_id,
        inventory: restored_inventory,
        unavailable_provider_keys: preflight.unavailable_provider_keys,
        previous_state: previous,
    })
}

fn inventory_matches_after_migration(
    inventory_version: u32,
    source: &PortableInventory,
    restored: &PortableInventory,
) -> bool {
    match inventory_version {
        1 => {
            restored.schema_version >= source.schema_version
                && restored.channels == source.channels
                && restored.agents == source.agents
                && restored.tasks == source.tasks
                && restored.workspaces == source.workspaces
                && restored.workspace_attachments == source.workspace_attachments
                && restored.workspace_binding_hints == source.workspace_binding_hints
                && restored.required_provider_keys == source.required_provider_keys
                && restored.required_runtime_keys == source.required_runtime_keys
        }
        _ => false,
    }
}

const fn default_inventory_version() -> u32 {
    1
}

fn require_checksum(checksums: &BundleChecksums, name: &str, actual: &str) -> Result<()> {
    match checksums.files.get(name) {
        Some(expected) if expected == actual => Ok(()),
        Some(_) => bail!("portable backup checksum mismatch for {name}"),
        None => bail!("portable backup checksum is missing for {name}"),
    }
}

async fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).context("failed to encode portable metadata")?;
    tokio::fs::write(path, bytes)
        .await
        .with_context(|| format!("failed to write {}", path.display()))
}

async fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("failed to open {} for checksum", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .with_context(|| format!("failed to read {} for checksum", path.display()))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

async fn verify_staged_state_checksum(staged: &Path, expected: &str) -> Result<()> {
    if sha256_file(staged).await? != expected {
        bail!("staged portable state checksum does not match preflight");
    }
    Ok(())
}

fn nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use cocli_store::{
        AgentStatus, Store, WorkspaceBindingState, WorkspaceProviderKey, CURRENT_SCHEMA_VERSION,
    };

    use super::{
        create_bundle, preflight_bundle, restore_bundle, sha256_file, verify_staged_state_checksum,
    };

    #[tokio::test]
    async fn bundle_preflight_inventory_and_moved_git_rebind_round_trip() {
        let temp = tempfile::tempdir().expect("temp dir");
        let source_data = temp.path().join("source-data");
        let target_data = temp.path().join("target-data");
        let source_repository = temp.path().join("source-checkout");
        let target_repository = temp.path().join("moved-checkout");
        let bundle = temp.path().join("portable.cocli-backup");
        tokio::fs::create_dir_all(source_repository.join(".git"))
            .await
            .expect("source Git metadata");
        tokio::fs::create_dir_all(target_repository.join(".git"))
            .await
            .expect("target Git metadata");
        tokio::fs::create_dir_all(&source_data)
            .await
            .expect("source data directory");

        let source = Store::open(source_data.join("cocli.sqlite3"))
            .await
            .expect("source store");
        let source_installation_id = source.current_installation_id().to_owned();
        let channel = source.create_channel("portable").await.expect("channel");
        source
            .create_agent(
                channel.id,
                "portable-agent",
                "fake",
                None,
                AgentStatus::Running,
            )
            .await
            .expect("agent");
        let workspace = source
            .create_workspace(
                WorkspaceProviderKey::new("git").expect("provider key"),
                "Portable repository",
                Some("https://example.test/portable.git"),
                serde_json::json!({"preferred_ref": "main"}),
            )
            .await
            .expect("workspace");
        source
            .bind_workspace(
                workspace.id,
                source_repository.to_str().expect("source path"),
                None,
            )
            .await
            .expect("source binding");

        let manifest = create_bundle(&source, &bundle)
            .await
            .expect("bundle should be created");
        assert_eq!(manifest.bundle_version, 1);
        assert_eq!(manifest.inventory.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(manifest.inventory.channels, 1);
        assert_eq!(manifest.inventory.agents, 1);
        assert_eq!(manifest.inventory.workspaces, 1);
        assert_eq!(manifest.inventory.required_provider_keys, vec!["git"]);
        let preflight = preflight_bundle(&bundle)
            .await
            .expect("bundle should pass preflight");
        assert_eq!(preflight.manifest.inventory, manifest.inventory);
        source.close().await;

        let restored = restore_bundle(&target_data, &bundle)
            .await
            .expect("bundle should restore");
        assert_ne!(restored.installation_id, source_installation_id);
        let target = Store::open(target_data.join("cocli.sqlite3"))
            .await
            .expect("target store");
        let restored_workspace = target
            .get_workspace(workspace.id)
            .await
            .expect("workspace query")
            .expect("workspace should survive");
        assert_eq!(
            restored_workspace.portable_locator.as_deref(),
            Some("https://example.test/portable.git")
        );
        assert_ne!(
            restored_workspace.portable_locator.as_deref(),
            source_repository.to_str()
        );
        let unbound = target
            .current_workspace_binding(workspace.id)
            .await
            .expect("binding query")
            .expect("unbound binding");
        assert_eq!(unbound.state, WorkspaceBindingState::Unbound);
        let hints = target
            .list_workspace_bindings(workspace.id)
            .await
            .expect("binding hints");
        assert!(hints.iter().any(|binding| {
            binding.installation_id == source_installation_id
                && binding.local_locator.as_deref() == source_repository.to_str()
        }));
        let rebound = target
            .bind_workspace(
                workspace.id,
                target_repository.to_str().expect("target path"),
                None,
            )
            .await
            .expect("target rebind");
        assert_eq!(rebound.state, WorkspaceBindingState::Ready);
        assert_eq!(rebound.local_locator.as_deref(), target_repository.to_str());
        target.close().await;
    }

    #[tokio::test]
    async fn invalid_bundle_leaves_existing_installation_unchanged() {
        let temp = tempfile::tempdir().expect("temp dir");
        let target_data = temp.path().join("target-data");
        let bundle = temp.path().join("invalid.cocli-backup");
        tokio::fs::create_dir_all(&bundle)
            .await
            .expect("bundle directory");
        tokio::fs::write(bundle.join("manifest.json"), b"{}")
            .await
            .expect("invalid manifest");
        tokio::fs::create_dir_all(&target_data)
            .await
            .expect("target data directory");
        let target = Store::open(target_data.join("cocli.sqlite3"))
            .await
            .expect("target store");
        let target_installation_id = target.current_installation_id().to_owned();
        target
            .create_channel("must-survive")
            .await
            .expect("target channel");
        target.close().await;
        let before = tokio::fs::read(target_data.join("cocli.sqlite3"))
            .await
            .expect("target bytes");

        assert!(restore_bundle(&target_data, &bundle).await.is_err());

        let after = tokio::fs::read(target_data.join("cocli.sqlite3"))
            .await
            .expect("target bytes after failed restore");
        assert_eq!(after, before);
        let reopened = Store::open(target_data.join("cocli.sqlite3"))
            .await
            .expect("target should reopen");
        assert_eq!(reopened.current_installation_id(), target_installation_id);
        assert_eq!(
            reopened.list_channels().await.expect("channels")[0].name,
            "must-survive"
        );
        reopened.close().await;
    }

    #[tokio::test]
    async fn staged_restore_bytes_must_match_the_preflight_checksum() {
        let temp = tempfile::tempdir().expect("temp dir");
        let staged = temp.path().join("staged.sqlite3");
        tokio::fs::write(&staged, b"preflight-verified bytes")
            .await
            .expect("original staged bytes");
        let expected = sha256_file(&staged).await.expect("expected checksum");
        tokio::fs::write(&staged, b"bytes changed after preflight")
            .await
            .expect("mutated staged bytes");

        let error = verify_staged_state_checksum(&staged, &expected)
            .await
            .expect_err("changed staged bytes must be rejected");

        assert!(error
            .to_string()
            .contains("staged portable state checksum does not match preflight"));
    }

    #[tokio::test]
    async fn preflight_reports_providers_without_an_available_resolver() {
        let temp = tempfile::tempdir().expect("temp dir");
        let source_data = temp.path().join("source-data");
        let bundle = temp.path().join("providers.cocli-backup");
        tokio::fs::create_dir_all(&source_data)
            .await
            .expect("source data directory");
        let source = Store::open(source_data.join("cocli.sqlite3"))
            .await
            .expect("source store");
        source
            .create_workspace(
                WorkspaceProviderKey::new("external").expect("external provider key"),
                "External resource",
                Some("external://portable/resource"),
                serde_json::json!({}),
            )
            .await
            .expect("external workspace");
        source
            .create_workspace(
                WorkspaceProviderKey::new("managed").expect("managed provider key"),
                "Managed resource",
                None,
                serde_json::json!({}),
            )
            .await
            .expect("managed workspace");

        create_bundle(&source, &bundle)
            .await
            .expect("bundle should be created");
        let preflight = preflight_bundle(&bundle)
            .await
            .expect("bundle should pass preflight");
        assert_eq!(
            preflight.unavailable_provider_keys,
            vec!["external", "managed"]
        );
        assert!(preflight
            .warnings
            .iter()
            .any(|warning| warning.contains("Providers are unavailable")));
        source.close().await;
    }
}
