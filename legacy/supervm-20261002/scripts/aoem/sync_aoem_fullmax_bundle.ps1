param(
    [string]$RepoRoot = "",
    [string]$AoemSourceRoot = "",
    [string]$SupervmAoemRoot = "",
    [string]$Stamp = "",
    [string]$MacosStamp = "",
    [string]$ReleaseVersion = "Beta 0.8",
    [switch]$CleanOld
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RepoRoot([string]$Explicit) {
    if ($Explicit) { return (Resolve-Path $Explicit).Path }
    return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
}

function Resolve-AoemSourceRoot([string]$Explicit, [string]$RepoRootPath) {
    if ($Explicit) { return (Resolve-Path $Explicit).Path }
    return (Resolve-Path (Join-Path (Split-Path $RepoRootPath -Parent) "AOEM")).Path
}

function Resolve-SupervmAoemRoot([string]$Explicit, [string]$RepoRootPath) {
    if ($Explicit) { return (Resolve-Path $Explicit).Path }
    return (Join-Path $RepoRootPath "aoem")
}

function Get-LatestStamp([string]$BaseDir) {
    if (-not (Test-Path $BaseDir)) { return "" }
    $d = Get-ChildItem -Path $BaseDir -Directory |
        Where-Object { $_.Name -match '^\d{8}-\d{6}$' } |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if ($null -eq $d) { return "" }
    return $d.Name
}

function Copy-PlatformBundle([string]$Platform, [string]$BundleStamp, [string]$SourceRoot, [string]$DestRoot) {
    if (-not $BundleStamp) {
        throw "missing stamp for platform=$Platform"
    }
    $src = Join-Path $SourceRoot ("artifacts\ffi-bundles\fullmax\$Platform\$BundleStamp")
    if (-not (Test-Path $src)) {
        throw "bundle not found: $src"
    }
    $dst = Join-Path $DestRoot $Platform
    if (Test-Path $dst) {
        Remove-Item -Recurse -Force $dst
    }
    New-Item -ItemType Directory -Force -Path $dst | Out-Null
    Copy-Item -Path (Join-Path $src "*") -Destination $dst -Recurse -Force
    return $dst
}

$repoRoot = Resolve-RepoRoot $RepoRoot
$aoemSourceRoot = Resolve-AoemSourceRoot $AoemSourceRoot $repoRoot
$supervmAoemRoot = Resolve-SupervmAoemRoot $SupervmAoemRoot $repoRoot

if ($CleanOld.IsPresent) {
    foreach ($legacy in @("bin", "plugins", "include")) {
        $p = Join-Path $supervmAoemRoot $legacy
        if (Test-Path $p) {
            Remove-Item -Recurse -Force $p
        }
    }
}

New-Item -ItemType Directory -Force -Path $supervmAoemRoot | Out-Null

$windowsStamp = if ($Stamp) { $Stamp } else { Get-LatestStamp (Join-Path $aoemSourceRoot "artifacts\ffi-bundles\fullmax\windows") }
$linuxStamp = if ($Stamp) { $Stamp } else { Get-LatestStamp (Join-Path $aoemSourceRoot "artifacts\ffi-bundles\fullmax\linux") }

$windowsOut = Copy-PlatformBundle -Platform "windows" -BundleStamp $windowsStamp -SourceRoot $aoemSourceRoot -DestRoot $supervmAoemRoot
$linuxOut = Copy-PlatformBundle -Platform "linux" -BundleStamp $linuxStamp -SourceRoot $aoemSourceRoot -DestRoot $supervmAoemRoot

$macosUsed = ""
if ($MacosStamp) {
    $null = Copy-PlatformBundle -Platform "macos" -BundleStamp $MacosStamp -SourceRoot $aoemSourceRoot -DestRoot $supervmAoemRoot
    $macosUsed = $MacosStamp
}

$rootConfigDir = Join-Path $supervmAoemRoot "config"
if (-not (Test-Path $rootConfigDir)) {
    New-Item -ItemType Directory -Force -Path $rootConfigDir | Out-Null
}
$runtimeProfilePath = Join-Path $rootConfigDir "aoem-runtime-profile.json"
if (-not (Test-Path $runtimeProfilePath)) {
    @{
        schema = "aoem-runtime-profile/v1"
        generated_at_utc = [DateTime]::UtcNow.ToString("o")
        version = "aoem-1.0.0"
        threads = @{ default = 8 }
    } | ConvertTo-Json -Depth 6 | Set-Content -Path $runtimeProfilePath -Encoding UTF8
}

foreach ($platform in @("windows", "linux", "macos")) {
    $platformDir = Join-Path $supervmAoemRoot $platform
    if (-not (Test-Path $platformDir)) { continue }
    $configDir = Join-Path $platformDir "config"
    New-Item -ItemType Directory -Force -Path $configDir | Out-Null
    Copy-Item -Path $runtimeProfilePath -Destination (Join-Path $configDir "aoem-runtime-profile.json") -Force
}

powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\aoem\build_aoem_manifest.ps1") -AoemRoot $supervmAoemRoot -ReleaseVersion $ReleaseVersion
if ($LASTEXITCODE -ne 0) {
    throw "build_aoem_manifest.ps1 failed: exit=$LASTEXITCODE"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    aoem_source_root = $aoemSourceRoot
    supervm_aoem_root = $supervmAoemRoot
    release_version = $ReleaseVersion
    windows_stamp = $windowsStamp
    linux_stamp = $linuxStamp
    macos_stamp = $macosUsed
    windows_out = $windowsOut
    linux_out = $linuxOut
    clean_old = $CleanOld.IsPresent
}
$summaryPath = Join-Path $supervmAoemRoot "sync-fullmax-summary.json"
$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryPath -Encoding UTF8

Write-Output "STATUS=PASS"
Write-Output "SUMMARY=$summaryPath"
