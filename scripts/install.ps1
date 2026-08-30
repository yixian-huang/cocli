# User-scoped installer for cocli + cocli-bridge (unsigned checksum path).
#
# Selects a version (COCLI_VERSION or the latest GitHub release) and OS/arch,
# verifies SHA-256 before replacing, installs both binaries together, and
# never touches the data directory.
#
# Until GitHub Releases exist, set COCLI_ARTIFACT_DIR to a directory of locally
# built binaries plus SHA256SUMS (see scripts/write-sha256sums.sh).
#
# Release asset names (D3 must match):
#   cocli-${VERSION}-${TARGET}.tar.gz
#   cocli-${VERSION}-${TARGET}.zip
#   SHA256SUMS
#
# Default prefix: %LOCALAPPDATA%\cocli (no admin).

[CmdletBinding()]
param(
    [switch]$Help
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoDefault = 'yixian-huang/cocli'
$UiUrl = 'http://127.0.0.1:8090'

function Show-Usage {
    @'
Usage: install.ps1

Install cocli.exe and cocli-bridge.exe into a user-scoped prefix (no admin).

Environment:
  COCLI_ARTIFACT_DIR  Directory of local binaries + SHA256SUMS (skips GitHub)
  COCLI_VERSION       Release version (default: latest GitHub release)
  COCLI_PREFIX        Install prefix (default: %LOCALAPPDATA%\cocli)
  COCLI_REPO          GitHub owner/repo (default: yixian-huang/cocli)
  COCLI_TARGET        Override Rust target triple
  GITHUB_TOKEN        Optional token for GitHub API / asset downloads

Binaries install to $COCLI_PREFIX. The data directory is not modified.

Next action after install: run `cocli`, then open http://127.0.0.1:8090
'@
}

if ($Help) {
    Show-Usage
    exit 0
}

function Die {
    param([string]$Message)
    [Console]::Error.WriteLine("install.ps1: $Message")
    exit 1
}

function Get-FileSha256Lower {
    param([string]$Path)
    (Get-FileHash -Algorithm SHA256 -Path $Path).Hash.ToLowerInvariant()
}

function Get-SumsHashFor {
    param(
        [string]$SumsPath,
        [string]$Name
    )
    foreach ($line in Get-Content -Path $SumsPath) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        $parts = $line.Trim() -split '\s+', 2
        if ($parts.Count -lt 2) { continue }
        $hash = $parts[0].ToLowerInvariant()
        $file = $parts[1].Trim()
        if ($file.StartsWith('*')) { $file = $file.Substring(1) }
        $file = $file.TrimStart('.', '\', '/')
        $file = [System.IO.Path]::GetFileName($file)
        if ($file -eq $Name) { return $hash }
    }
    return $null
}

function Assert-NamedFiles {
    param(
        [string]$SumsPath,
        [string[]]$Names,
        [string]$Directory
    )
    if (-not (Test-Path -LiteralPath $SumsPath)) {
        Die "checksum file not found: $SumsPath"
    }
    foreach ($name in $Names) {
        $path = Join-Path $Directory $name
        if (-not (Test-Path -LiteralPath $path)) {
            Die "missing artifact: $name"
        }
        $expected = Get-SumsHashFor -SumsPath $SumsPath -Name $name
        if ([string]::IsNullOrWhiteSpace($expected)) {
            Die "SHA256SUMS has no entry for $name"
        }
        $actual = Get-FileSha256Lower -Path $path
        if ($actual -ne $expected) {
            Die "SHA-256 mismatch for $name (expected $expected, got $actual)"
        }
    }
}

function Get-DefaultTarget {
    $arch = $env:PROCESSOR_ARCHITECTURE
    switch -Regex ($arch) {
        '^(AMD64|x86_64)$' { return 'x86_64-pc-windows-msvc' }
        '^(ARM64|aarch64)$' { return 'aarch64-pc-windows-msvc' }
        default { Die "unsupported architecture: $arch" }
    }
}

function Normalize-Version {
    param([string]$Raw)
    $version = $Raw.Trim()
    if ($version.StartsWith('v')) { $version = $version.Substring(1) }
    if ([string]::IsNullOrWhiteSpace($version)) { Die 'empty COCLI_VERSION' }
    return $version
}

function Get-GitHubHeaders {
    $headers = @{ 'User-Agent' = 'cocli-install' }
    $token = $env:GITHUB_TOKEN
    if ([string]::IsNullOrWhiteSpace($token)) { $token = $env:GH_TOKEN }
    if (-not [string]::IsNullOrWhiteSpace($token)) {
        $headers['Authorization'] = "Bearer $token"
    }
    return $headers
}

function Get-LatestTag {
    param([string]$Repo)
    $uri = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $release = Invoke-RestMethod -Uri $uri -Headers (Get-GitHubHeaders)
    } catch {
        return $null
    }
    if ($null -eq $release -or -not $release.tag_name) { return $null }
    return [string]$release.tag_name
}

function Install-Atomic {
    param(
        [string]$Source,
        [string]$Destination
    )
    $destDir = Split-Path -Parent $Destination
    if (-not (Test-Path -LiteralPath $destDir)) {
        New-Item -ItemType Directory -Path $destDir -Force | Out-Null
    }
    $tmp = Join-Path $destDir ('.cocli-new.' + [guid]::NewGuid().ToString('n') + '.tmp')
    $prev = "$Destination.prev"
    Copy-Item -LiteralPath $Source -Destination $tmp -Force
    if (Test-Path -LiteralPath $Destination) {
        Move-Item -LiteralPath $Destination -Destination $prev -Force
    }
    try {
        Move-Item -LiteralPath $tmp -Destination $Destination -Force
    } catch {
        if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force }
        if (Test-Path -LiteralPath $prev) {
            Move-Item -LiteralPath $prev -Destination $Destination -Force
        }
        throw
    }
}

function Restore-Binaries {
    param(
        [string]$Prefix,
        [string[]]$Names
    )
    # Roll back names successfully installed in this run. Restore dest.prev when
    # present (upgrade); otherwise remove dest (first install had no previous).
    if ($null -eq $Names -or @($Names).Count -eq 0) { return }
    foreach ($name in @($Names)) {
        $dest = Join-Path $Prefix $name
        $prev = "$dest.prev"
        if (Test-Path -LiteralPath $prev) {
            if (Test-Path -LiteralPath $dest) {
                Remove-Item -LiteralPath $dest -Force
            }
            Move-Item -LiteralPath $prev -Destination $dest -Force
        } elseif (Test-Path -LiteralPath $dest) {
            Remove-Item -LiteralPath $dest -Force
        }
    }
}

function Clear-Prev {
    param(
        [string]$Prefix,
        [string[]]$Names
    )
    foreach ($name in $Names) {
        $prev = Join-Path $Prefix ($name + '.prev')
        if (Test-Path -LiteralPath $prev) {
            Remove-Item -LiteralPath $prev -Force
        }
    }
}

function Find-Binary {
    param(
        [string]$Root,
        [string]$Name
    )
    $direct = Join-Path $Root $Name
    if (Test-Path -LiteralPath $direct) { return $direct }
    $match = Get-ChildItem -Path $Root -Recurse -File -Filter $Name | Select-Object -First 1
    if ($null -eq $match) { return $null }
    return $match.FullName
}

$binaries = @('cocli.exe', 'cocli-bridge.exe')
$target = $env:COCLI_TARGET
if ([string]::IsNullOrWhiteSpace($target)) { $target = Get-DefaultTarget }

$prefix = $env:COCLI_PREFIX
if ([string]::IsNullOrWhiteSpace($prefix)) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        Die 'LOCALAPPDATA is required for the default prefix'
    }
    $prefix = Join-Path $env:LOCALAPPDATA 'cocli'
}

$repo = $env:COCLI_REPO
if ([string]::IsNullOrWhiteSpace($repo)) { $repo = $RepoDefault }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ('cocli-install-' + [guid]::NewGuid().ToString('n'))
New-Item -ItemType Directory -Path $work -Force | Out-Null
$stage = Join-Path $work 'stage'
New-Item -ItemType Directory -Path $stage -Force | Out-Null
$sumsPath = Join-Path $work 'SHA256SUMS'

try {
    $artifactDir = $env:COCLI_ARTIFACT_DIR
    if (-not [string]::IsNullOrWhiteSpace($artifactDir)) {
        if (-not (Test-Path -LiteralPath $artifactDir -PathType Container)) {
            Die "COCLI_ARTIFACT_DIR is not a directory: $artifactDir"
        }
        $artifactDir = (Resolve-Path -LiteralPath $artifactDir).Path
        $srcSums = Join-Path $artifactDir 'SHA256SUMS'
        if (-not (Test-Path -LiteralPath $srcSums)) {
            Die "COCLI_ARTIFACT_DIR has no SHA256SUMS. Generate it with scripts/write-sha256sums.sh"
        }
        foreach ($name in $binaries) {
            $src = Join-Path $artifactDir $name
            if (-not (Test-Path -LiteralPath $src)) {
                Die "COCLI_ARTIFACT_DIR missing $name"
            }
            Copy-Item -LiteralPath $src -Destination (Join-Path $stage $name) -Force
        }
        Copy-Item -LiteralPath $srcSums -Destination $sumsPath -Force
        Assert-NamedFiles -SumsPath $sumsPath -Names $binaries -Directory $stage
    } else {
        $versionRaw = $env:COCLI_VERSION
        if ([string]::IsNullOrWhiteSpace($versionRaw)) {
            $versionRaw = Get-LatestTag -Repo $repo
            if ([string]::IsNullOrWhiteSpace($versionRaw)) {
                Die "no GitHub release found for $repo. Build locally and set COCLI_ARTIFACT_DIR (see scripts/write-sha256sums.sh)."
            }
        }
        $version = Normalize-Version $versionRaw
        $tag = "v$version"
        $archive = "cocli-$version-$target.zip"
        $base = "https://github.com/$repo/releases/download/$tag"
        $headers = Get-GitHubHeaders
        Invoke-WebRequest -Uri "$base/SHA256SUMS" -OutFile $sumsPath -Headers $headers
        $archivePath = Join-Path $work $archive
        Invoke-WebRequest -Uri "$base/$archive" -OutFile $archivePath -Headers $headers
        Assert-NamedFiles -SumsPath $sumsPath -Names @($archive) -Directory $work
        $extract = Join-Path $work 'extract'
        New-Item -ItemType Directory -Path $extract -Force | Out-Null
        Expand-Archive -LiteralPath $archivePath -DestinationPath $extract -Force
        foreach ($name in $binaries) {
            $found = Find-Binary -Root $extract -Name $name
            if ([string]::IsNullOrWhiteSpace($found)) {
                Die "archive $archive does not contain $name"
            }
            Copy-Item -LiteralPath $found -Destination (Join-Path $stage $name) -Force
        }
        $innerOk = $true
        foreach ($name in $binaries) {
            if ([string]::IsNullOrWhiteSpace((Get-SumsHashFor -SumsPath $sumsPath -Name $name))) {
                $innerOk = $false
                break
            }
        }
        if ($innerOk) {
            Assert-NamedFiles -SumsPath $sumsPath -Names $binaries -Directory $stage
        }
    }

    if (-not (Test-Path -LiteralPath $prefix)) {
        New-Item -ItemType Directory -Path $prefix -Force | Out-Null
    }
    $writable = $false
    try {
        $probe = Join-Path $prefix ('.cocli-write-probe-' + [guid]::NewGuid().ToString('n'))
        New-Item -ItemType File -Path $probe -Force | Out-Null
        Remove-Item -LiteralPath $probe -Force
        $writable = $true
    } catch {
        $writable = $false
    }
    if (-not $writable) {
        Die "install prefix is not writable (no admin by default): $prefix"
    }

    $installed = @()
    try {
        foreach ($name in $binaries) {
            Install-Atomic -Source (Join-Path $stage $name) -Destination (Join-Path $prefix $name)
            $installed += $name
        }
    } catch {
        Restore-Binaries -Prefix $prefix -Names @($installed)
        Die "failed to install binaries; destination restored to previous state"
    }
    Clear-Prev -Prefix $prefix -Names $binaries

    $cocliPath = Join-Path $prefix 'cocli.exe'
    $bridgePath = Join-Path $prefix 'cocli-bridge.exe'
    $cocliVer = (& $cocliPath --version).Trim()
    $bridgeVer = (& $bridgePath --version).Trim()
    Write-Host ""
    Write-Host "Installed:"
    Write-Host "  $cocliVer -> $cocliPath"
    Write-Host "  $bridgeVer -> $bridgePath"
    Write-Host ""
    Write-Host "Next: run ``cocli``, then open $UiUrl"

    $pathEntries = $env:PATH -split ';'
    if ($pathEntries -notcontains $prefix) {
        Write-Host ""
        Write-Host "Note: $prefix is not on PATH. Add it before running ``cocli``."
    }
} finally {
    if (Test-Path -LiteralPath $work) {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }
}
