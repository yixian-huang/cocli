use std::ffi::OsStr;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use cocli_store::SkillLibraryFile;
use tempfile::TempDir;
use tokio::net::lookup_host;
use tokio::process::Command;
use url::Url;

const MAX_SKILL_BYTES: usize = 50 * 1024 * 1024;
const MAX_SKILL_FILES: usize = 5_000;
const GIT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub(super) enum SkillImportError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Git(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SkillMetadata {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub user_invocable: bool,
}

#[derive(Debug)]
pub(super) struct FetchedSkill {
    pub files: Vec<SkillLibraryFile>,
    pub source_kind: String,
    pub source_url: String,
    pub source_ref: Option<String>,
    pub metadata: SkillMetadata,
}

pub(super) async fn fetch_skill(
    raw_source: &str,
    subpath: Option<&str>,
) -> Result<FetchedSkill, SkillImportError> {
    let source = raw_source.trim();
    if source.is_empty() {
        return Err(SkillImportError::Invalid(
            "skill source must not be empty".to_owned(),
        ));
    }
    if Path::new(source).is_absolute() {
        return fetch_local(source, subpath).await;
    }
    match Url::parse(source) {
        Ok(url) if url.scheme() == "https" => fetch_git(source, subpath).await,
        Ok(url) if url.scheme() == "file" => fetch_local(source, subpath).await,
        Ok(url) => Err(SkillImportError::Invalid(format!(
            "unsupported skill source scheme: {}",
            url.scheme()
        ))),
        Err(_) => fetch_local(source, subpath).await,
    }
}

pub(super) fn canonical_skill_name(value: &str) -> Result<String, SkillImportError> {
    let mut output = String::with_capacity(value.len());
    let mut separator = false;
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            separator = false;
        } else if matches!(character, '-' | '_') {
            output.push(character);
            separator = false;
        } else if !output.is_empty() && !separator {
            output.push('-');
            separator = true;
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    if output.is_empty() || output.len() > 80 {
        return Err(SkillImportError::Invalid(format!(
            "cannot derive a safe skill name from {value:?}"
        )));
    }
    Ok(output)
}

pub(super) fn derive_source_name(source: &str, subpath: Option<&str>) -> String {
    if let Some(subpath) = subpath.filter(|value| !value.trim().is_empty()) {
        if let Some(name) = Path::new(subpath).file_name().and_then(OsStr::to_str) {
            return name.to_owned();
        }
    }
    if let Ok(url) = Url::parse(source) {
        if let Some(segment) = url
            .path_segments()
            .and_then(|segments| segments.filter(|segment| !segment.is_empty()).next_back())
        {
            return segment.trim_end_matches(".git").to_owned();
        }
    }
    Path::new(source)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .trim_end_matches(".git")
        .to_owned()
}

async fn fetch_local(
    source: &str,
    subpath: Option<&str>,
) -> Result<FetchedSkill, SkillImportError> {
    let path = if source.starts_with("file://") {
        Url::parse(source)
            .map_err(|error| SkillImportError::Invalid(format!("invalid file URL: {error}")))?
            .to_file_path()
            .map_err(|_| SkillImportError::Invalid("invalid file URL path".to_owned()))?
    } else {
        PathBuf::from(source)
    };
    if !path.is_absolute() {
        return Err(SkillImportError::Invalid(
            "local skill source must be an absolute path or file:// URL".to_owned(),
        ));
    }
    let source_root = tokio::fs::canonicalize(&path)
        .await
        .map_err(|error| import_io("resolve local skill source", &error))?;
    let root = resolve_subpath(&source_root, subpath).await?;
    let walk_root = root.clone();
    let files = tokio::task::spawn_blocking(move || walk_skill_tree(&walk_root))
        .await
        .map_err(|error| SkillImportError::Io(format!("skill scan task failed: {error}")))??;
    let metadata = parse_metadata(&files);
    Ok(FetchedSkill {
        files,
        source_kind: "local".to_owned(),
        source_url: source.to_owned(),
        source_ref: None,
        metadata,
    })
}

async fn fetch_git(source: &str, subpath: Option<&str>) -> Result<FetchedSkill, SkillImportError> {
    validate_public_https(source).await?;
    let temp =
        TempDir::new().map_err(|error| import_io("create temporary skill checkout", &error))?;
    let destination = temp.path().to_owned();
    let mut command = Command::new("git");
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_SSH_COMMAND", "false")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .args([
            "-c",
            "protocol.file.allow=never",
            "-c",
            "protocol.ext.allow=never",
            "-c",
            "http.followRedirects=false",
            "clone",
            "--depth=1",
            "--no-tags",
            "--",
            source,
        ])
        .arg(&destination);
    let output = tokio::time::timeout(GIT_TIMEOUT, command.output())
        .await
        .map_err(|_| SkillImportError::Git("git clone timed out after 60 seconds".to_owned()))?
        .map_err(|error| SkillImportError::Git(format!("start git clone: {error}")))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(SkillImportError::Git(format!(
            "git clone failed: {}",
            detail.trim()
        )));
    }

    let revision = Command::new("git")
        .args(["-C"])
        .arg(&destination)
        .args(["rev-parse", "HEAD"])
        .output()
        .await
        .map_err(|error| SkillImportError::Git(format!("read git revision: {error}")))?;
    if !revision.status.success() {
        return Err(SkillImportError::Git(
            "git checkout has no readable HEAD revision".to_owned(),
        ));
    }
    let source_ref = String::from_utf8(revision.stdout)
        .map_err(|_| SkillImportError::Git("git revision is not UTF-8".to_owned()))?
        .trim()
        .to_owned();
    let root = resolve_subpath(&destination, subpath).await?;
    let walk_root = root.clone();
    let files = tokio::task::spawn_blocking(move || walk_skill_tree(&walk_root))
        .await
        .map_err(|error| SkillImportError::Io(format!("skill scan task failed: {error}")))??;
    let metadata = parse_metadata(&files);
    Ok(FetchedSkill {
        files,
        source_kind: "git".to_owned(),
        source_url: source.to_owned(),
        source_ref: Some(source_ref),
        metadata,
    })
}

async fn resolve_subpath(
    source_root: &Path,
    subpath: Option<&str>,
) -> Result<PathBuf, SkillImportError> {
    let Some(subpath) = subpath.filter(|value| !value.trim().is_empty()) else {
        return Ok(source_root.to_owned());
    };
    let relative = Path::new(subpath);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(SkillImportError::Invalid(format!(
            "skill subpath escapes the source: {subpath}"
        )));
    }
    let resolved = tokio::fs::canonicalize(source_root.join(relative))
        .await
        .map_err(|error| import_io("resolve skill subpath", &error))?;
    if !resolved.starts_with(source_root) {
        return Err(SkillImportError::Invalid(format!(
            "skill subpath escapes the source: {subpath}"
        )));
    }
    Ok(resolved)
}

fn walk_skill_tree(root: &Path) -> Result<Vec<SkillLibraryFile>, SkillImportError> {
    let metadata =
        std::fs::symlink_metadata(root).map_err(|error| import_io("inspect skill root", &error))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(SkillImportError::Invalid(
            "skill source must resolve to a real directory".to_owned(),
        ));
    }
    let mut files = Vec::new();
    let mut total = 0_usize;
    collect_files(root, root, &mut files, &mut total)?;
    files.sort_by(|left, right| left.rel_path.cmp(&right.rel_path));
    if !files.iter().any(|file| file.rel_path == "SKILL.md") {
        return Err(SkillImportError::Invalid(
            "skill root must contain SKILL.md".to_owned(),
        ));
    }
    Ok(files)
}

fn collect_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<SkillLibraryFile>,
    total: &mut usize,
) -> Result<(), SkillImportError> {
    let entries =
        std::fs::read_dir(current).map_err(|error| import_io("read skill directory", &error))?;
    for entry in entries {
        let entry = entry.map_err(|error| import_io("read skill directory entry", &error))?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| import_io("inspect skill entry", &error))?;
        if file_type.is_symlink() {
            return Err(SkillImportError::Invalid(format!(
                "skill source contains a symbolic link: {}",
                entry.path().display()
            )));
        }
        if file_type.is_dir() {
            collect_files(root, &entry.path(), files, total)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if files.len() >= MAX_SKILL_FILES {
            return Err(SkillImportError::Invalid(format!(
                "skill contains more than {MAX_SKILL_FILES} files"
            )));
        }
        let metadata = entry
            .metadata()
            .map_err(|error| import_io("read skill file metadata", &error))?;
        let declared_size = usize::try_from(metadata.len()).map_err(|_| {
            SkillImportError::Invalid("skill file size cannot be represented locally".to_owned())
        })?;
        if total
            .checked_add(declared_size)
            .map_or(true, |next| next > MAX_SKILL_BYTES)
        {
            return Err(SkillImportError::Invalid(format!(
                "skill exceeds the {MAX_SKILL_BYTES}-byte import limit"
            )));
        }
        let bytes =
            std::fs::read(entry.path()).map_err(|error| import_io("read skill file", &error))?;
        *total = total
            .checked_add(bytes.len())
            .ok_or_else(|| SkillImportError::Invalid("skill byte count overflowed".to_owned()))?;
        if *total > MAX_SKILL_BYTES {
            return Err(SkillImportError::Invalid(format!(
                "skill exceeds the {MAX_SKILL_BYTES}-byte import limit"
            )));
        }
        let rel_path = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| SkillImportError::Invalid("skill file escaped its root".to_owned()))?
            .to_string_lossy()
            .replace('\\', "/");
        let mode = file_mode(&metadata);
        files.push(SkillLibraryFile {
            rel_path,
            mode,
            size: bytes.len() as i64,
            content: bytes,
        });
    }
    Ok(())
}

#[cfg(unix)]
fn file_mode(metadata: &std::fs::Metadata) -> i64 {
    use std::os::unix::fs::PermissionsExt;
    i64::from(metadata.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn file_mode(_metadata: &std::fs::Metadata) -> i64 {
    0o644
}

fn parse_metadata(files: &[SkillLibraryFile]) -> SkillMetadata {
    let Some(skill_md) = files.iter().find(|file| file.rel_path == "SKILL.md") else {
        return SkillMetadata::default();
    };
    let Ok(content) = std::str::from_utf8(&skill_md.content) else {
        return SkillMetadata::default();
    };
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return SkillMetadata::default();
    }
    let mut metadata = SkillMetadata::default();
    for line in lines {
        let line = line.trim();
        if line == "---" {
            break;
        }
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'').to_owned();
        match key.trim().to_ascii_lowercase().as_str() {
            "name" => metadata.name = value,
            "display-name" | "display_name" => metadata.display_name = value,
            "description" => metadata.description = value,
            "user-invocable" | "user_invocable" => {
                metadata.user_invocable = value.eq_ignore_ascii_case("true");
            }
            _ => {}
        }
    }
    metadata
}

async fn validate_public_https(source: &str) -> Result<(), SkillImportError> {
    let url = Url::parse(source)
        .map_err(|error| SkillImportError::Invalid(format!("invalid skill URL: {error}")))?;
    if url.scheme() != "https" {
        return Err(SkillImportError::Invalid(
            "remote skill sources must use https://".to_owned(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(SkillImportError::Invalid(
            "skill URL credentials are not allowed".to_owned(),
        ));
    }
    if url.port_or_known_default() != Some(443) {
        return Err(SkillImportError::Invalid(
            "remote skill sources must use HTTPS port 443".to_owned(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| SkillImportError::Invalid("skill URL has no host".to_owned()))?;
    let lower = host.to_ascii_lowercase();
    if lower.ends_with(".local") || lower.ends_with(".onion") {
        return Err(SkillImportError::Invalid(format!(
            "skill URL host is not allowed: {host}"
        )));
    }
    let addresses = tokio::time::timeout(Duration::from_secs(5), lookup_host((host, 443)))
        .await
        .map_err(|_| SkillImportError::Invalid("skill URL DNS lookup timed out".to_owned()))?
        .map_err(|error| {
            SkillImportError::Invalid(format!("skill URL DNS lookup failed: {error}"))
        })?;
    let mut found = false;
    for address in addresses {
        found = true;
        if blocked_ip(address.ip()) {
            return Err(SkillImportError::Invalid(format!(
                "skill URL resolves to a blocked address: {}",
                address.ip()
            )));
        }
    }
    if !found {
        return Err(SkillImportError::Invalid(
            "skill URL host has no addresses".to_owned(),
        ));
    }
    Ok(())
}

fn blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => blocked_ipv4(ip),
        IpAddr::V6(ip) => blocked_ipv6(ip),
    }
}

fn blocked_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || octets[0] == 0
        || octets[0] == 10
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 169 && octets[1] == 254)
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        || (octets[0] == 192 && octets[1] == 168)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
        || (octets[0] == 192 && octets[1] == 88 && octets[2] == 99)
        || (octets[0] == 198 && octets[1] == 18)
        || (octets[0] == 198 && octets[1] == 19)
        || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
        || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        || octets[0] >= 240
}

fn blocked_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
        || ip.to_ipv4_mapped().is_some_and(blocked_ipv4)
}

fn import_io(action: &str, error: &io::Error) -> SkillImportError {
    SkillImportError::Io(format!("{action}: {error}"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn imports_local_skill_and_parses_frontmatter() {
        let source = tempdir().expect("temp source");
        fs::create_dir_all(source.path().join("scripts")).expect("scripts");
        fs::write(
            source.path().join("SKILL.md"),
            "---\nname: Demo Skill\ndisplay-name: Demo\ndescription: local demo\nuser-invocable: true\n---\n",
        )
        .expect("skill md");
        fs::write(source.path().join("scripts/run.sh"), "echo ok\n").expect("script");

        let fetched = fetch_skill(source.path().to_str().expect("path"), None)
            .await
            .expect("fetch local");
        assert_eq!(fetched.source_kind, "local");
        assert_eq!(fetched.metadata.name, "Demo Skill");
        assert!(fetched.metadata.user_invocable);
        assert_eq!(fetched.files.len(), 2);
    }

    #[tokio::test]
    async fn rejects_private_https_and_oversized_local_files_before_reading() {
        let blocked = fetch_skill("https://127.0.0.1/skill.git", None)
            .await
            .expect_err("private address should be blocked");
        assert!(blocked.to_string().contains("blocked address"));

        let source = tempdir().expect("temp source");
        fs::write(source.path().join("SKILL.md"), "# Demo\n").expect("skill md");
        let oversized = fs::File::create(source.path().join("large.bin")).expect("large file");
        oversized
            .set_len((MAX_SKILL_BYTES + 1) as u64)
            .expect("sparse large file");
        let error = fetch_skill(source.path().to_str().expect("path"), None)
            .await
            .expect_err("oversized skill should fail");
        assert!(error.to_string().contains("import limit"));
    }

    #[test]
    fn normalizes_names_and_blocks_private_addresses() {
        assert_eq!(
            canonical_skill_name(" Demo Skill ").expect("safe name"),
            "demo-skill"
        );
        assert!(blocked_ip("127.0.0.1".parse().expect("IP")));
        assert!(blocked_ip("10.0.0.1".parse().expect("IP")));
        assert!(!blocked_ip("1.1.1.1".parse().expect("IP")));
    }
}
