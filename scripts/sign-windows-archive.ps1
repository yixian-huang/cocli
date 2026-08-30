# Authenticode-sign cocli.exe + cocli-bridge.exe inside a windows zip and rewrite it.
#
# Requires:
#   WINDOWS_CERTIFICATE_PFX        base64-encoded .pfx
#   WINDOWS_CERTIFICATE_PASSWORD   PFX password

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Archive,
    [Parameter(Mandatory = $true)][string]$OutDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Die {
    param([string]$Message)
    [Console]::Error.WriteLine("sign-windows-archive: $Message")
    exit 1
}

if (-not (Test-Path -LiteralPath $Archive)) {
    Die "archive not found: $Archive"
}
$pfxB64 = $env:WINDOWS_CERTIFICATE_PFX
$password = $env:WINDOWS_CERTIFICATE_PASSWORD
if ([string]::IsNullOrWhiteSpace($pfxB64)) { Die 'WINDOWS_CERTIFICATE_PFX is empty' }
if ([string]::IsNullOrWhiteSpace($password)) { Die 'WINDOWS_CERTIFICATE_PASSWORD is empty' }

$archiveItem = Get-Item -LiteralPath $Archive
$expectedSuffix = "-pc-windows-msvc.zip"
if (-not $archiveItem.Name.StartsWith("cocli-$Version-") -or -not $archiveItem.Name.EndsWith($expectedSuffix)) {
    Die "expected windows zip named cocli-$Version-<triple>-pc-windows-msvc.zip, got $($archiveItem.Name)"
}

$target = $archiveItem.Name.Substring("cocli-$Version-".Length)
$target = $target.Substring(0, $target.Length - 4) # strip .zip

$signtool = Get-ChildItem -Path 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' } |
    Select-Object -First 1
if ($null -eq $signtool) {
    Die 'signtool.exe not found under Windows Kits 10 bin\x64'
}

if (-not (Test-Path -LiteralPath $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
}
$outFull = (Resolve-Path -LiteralPath $OutDir).Path

$work = Join-Path ([System.IO.Path]::GetTempPath()) ('cocli-sign-windows-' + [guid]::NewGuid().ToString('n'))
New-Item -ItemType Directory -Path $work -Force | Out-Null
$extract = Join-Path $work 'extract'
$bins = Join-Path $work 'bins'
$pfxPath = Join-Path $work 'cert.pfx'
New-Item -ItemType Directory -Path $extract -Force | Out-Null
New-Item -ItemType Directory -Path $bins -Force | Out-Null

try {
    [IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($pfxB64))
    Expand-Archive -LiteralPath $archiveItem.FullName -DestinationPath $extract -Force
    foreach ($name in @('cocli.exe', 'cocli-bridge.exe')) {
        $src = Join-Path $extract $name
        if (-not (Test-Path -LiteralPath $src)) {
            Die "archive is missing $name"
        }
        Copy-Item -LiteralPath $src -Destination (Join-Path $bins $name) -Force
    }

    $timestamp = 'http://timestamp.digicert.com'
    foreach ($name in @('cocli.exe', 'cocli-bridge.exe')) {
        $path = Join-Path $bins $name
        & $signtool.FullName sign /fd SHA256 /td SHA256 /tr $timestamp /f $pfxPath /p $password $path
        if ($LASTEXITCODE -ne 0) {
            Die "signtool failed for $name (exit $LASTEXITCODE)"
        }
    }

    $repoRoot = Split-Path -Parent $PSScriptRoot
    $pack = Join-Path $repoRoot 'scripts\package-release-archive.sh'
    $bash = Get-Command bash -ErrorAction SilentlyContinue
    if ($null -eq $bash) {
        Die 'bash is required to repack the signed zip'
    }
    & $bash.Source $pack $Version $target $bins $outFull
    if ($LASTEXITCODE -ne 0) {
        Die "package-release-archive.sh failed (exit $LASTEXITCODE)"
    }
    Write-Host "signed $($archiveItem.Name)"
} finally {
    if (Test-Path -LiteralPath $work) {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }
}
