param(
    [string]$RepoRoot = "",
    [string]$AoemSourceRoot = "",
    [string]$SupervmAoemRoot = "",
    [string]$OutputDir = "",
    [bool]$IncludeCore = $true,
    [bool]$IncludePersist = $true,
    [bool]$IncludeWasm = $true,
    [bool]$IncludeZkvm = $true,
    [bool]$IncludeMldsa = $true
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if (-not (Get-Variable -Name IsWindows -ErrorAction SilentlyContinue)) {
    $IsWindows = ($env:OS -eq "Windows_NT")
}

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

if (-not $AoemSourceRoot) {
    $AoemSourceRoot = Join-Path (Split-Path $RepoRoot -Parent) "AOEM"
}
$AoemSourceRoot = (Resolve-Path $AoemSourceRoot).Path

if (-not $SupervmAoemRoot) {
    $SupervmAoemRoot = Join-Path $RepoRoot "aoem"
}
$SupervmAoemRoot = (Resolve-Path $SupervmAoemRoot).Path

if (-not $OutputDir) {
    $OutputDir = Join-Path $SupervmAoemRoot "plugins"
}
if (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Get-PluginNameCandidates {
    param([string]$Kind)
    if ($IsWindows) {
        switch ($Kind) {
            "persist" { return @("aoem_ffi_persist_rocksdb.dll", "aoem_ffi_persist.dll") }
            "wasm" { return @("aoem_ffi_runtime_wasm_wasmtime.dll", "aoem_ffi_wasm.dll") }
            "zkvm" { return @("aoem_ffi_zkvm_executor.dll", "aoem_ffi_zkvm.dll") }
            "mldsa" { return @("aoem_ffi_crypto_mldsa.dll", "aoem_ffi_mldsa.dll") }
            default { return @() }
        }
    }
    if ($IsMacOS) {
        switch ($Kind) {
            "persist" { return @("libaoem_ffi_persist_rocksdb.dylib", "libaoem_ffi_persist.dylib") }
            "wasm" { return @("libaoem_ffi_runtime_wasm_wasmtime.dylib", "libaoem_ffi_wasm.dylib") }
            "zkvm" { return @("libaoem_ffi_zkvm_executor.dylib", "libaoem_ffi_zkvm.dylib") }
            "mldsa" { return @("libaoem_ffi_crypto_mldsa.dylib", "libaoem_ffi_mldsa.dylib") }
            default { return @() }
        }
    }
    switch ($Kind) {
        "persist" { return @("libaoem_ffi_persist_rocksdb.so", "libaoem_ffi_persist.so") }
        "wasm" { return @("libaoem_ffi_runtime_wasm_wasmtime.so", "libaoem_ffi_wasm.so") }
        "zkvm" { return @("libaoem_ffi_zkvm_executor.so", "libaoem_ffi_zkvm.so") }
        "mldsa" { return @("libaoem_ffi_crypto_mldsa.so", "libaoem_ffi_mldsa.so") }
        default { return @() }
    }
}

function Find-PluginSource {
    param(
        [string]$AoemRoot,
        [string]$Kind
    )
    $nameCandidates = Get-PluginNameCandidates -Kind $Kind
    if ($nameCandidates.Count -eq 0) {
        return ""
    }

    $dirs = @(
        (Join-Path $AoemRoot "artifacts\aoem-$Kind-plugin\current"),
        (Join-Path $AoemRoot "artifacts\aoem-$Kind-plugin"),
        (Join-Path $AoemRoot "cargo-target\release"),
        (Join-Path $AoemRoot "target\release")
    )

    foreach ($dir in $dirs) {
        if (-not (Test-Path $dir)) { continue }
        foreach ($name in $nameCandidates) {
            $candidate = Join-Path $dir $name
            if (Test-Path $candidate) {
                return (Resolve-Path $candidate).Path
            }
        }
    }
    return ""
}

$coreOutDir = Join-Path $SupervmAoemRoot "bin"
New-Item -ItemType Directory -Force -Path $coreOutDir | Out-Null

function Get-CoreLibName {
    if ($IsWindows) { return "aoem_ffi.dll" }
    if ($IsMacOS) { return "libaoem_ffi.dylib" }
    return "libaoem_ffi.so"
}

function Find-CoreFfiSource {
    param([string]$AoemRoot)
    $libName = Get-CoreLibName
    $direct = @(
        (Join-Path $AoemRoot "cargo-target\release\$libName"),
        (Join-Path $AoemRoot "target\release\$libName")
    )
    foreach ($p in $direct) {
        if (Test-Path $p) { return (Resolve-Path $p).Path }
    }

    $artifactsRoot = Join-Path $AoemRoot "artifacts"
    if (-not (Test-Path $artifactsRoot)) {
        return ""
    }
    $latest = Get-ChildItem -Path $artifactsRoot -Recurse -Filter $libName -File |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($null -ne $latest) {
        return $latest.FullName
    }
    return ""
}

$coreCopy = $null
if ($IncludeCore) {
    $coreSrc = Find-CoreFfiSource -AoemRoot $AoemSourceRoot
    if (-not $coreSrc) {
        throw "core FFI library not found under AOEM source root: $AoemSourceRoot"
    }
    $coreDst = Join-Path $coreOutDir ([System.IO.Path]::GetFileName($coreSrc))
    Copy-Item -Path $coreSrc -Destination $coreDst -Force
    $coreCopy = [ordered]@{
        source = $coreSrc
        destination = $coreDst
    }

    $headerSrc = Join-Path $AoemSourceRoot "crates\ffi\aoem-ffi\include\aoem.h"
    $headerDstDir = Join-Path $SupervmAoemRoot "include"
    if (Test-Path $headerSrc) {
        New-Item -ItemType Directory -Force -Path $headerDstDir | Out-Null
        Copy-Item -Path $headerSrc -Destination (Join-Path $headerDstDir "aoem.h") -Force
    }
}

$plan = @()
if ($IncludePersist) { $plan += "persist" }
if ($IncludeWasm) { $plan += "wasm" }
if ($IncludeZkvm) { $plan += "zkvm" }
if ($IncludeMldsa) { $plan += "mldsa" }

$copied = @()
$missing = @()
foreach ($kind in $plan) {
    $src = Find-PluginSource -AoemRoot $AoemSourceRoot -Kind $kind
    if (-not $src) {
        $missing += $kind
        continue
    }
    $dst = Join-Path $OutputDir ([System.IO.Path]::GetFileName($src))
    Copy-Item -Path $src -Destination $dst -Force
    $copied += [ordered]@{
        kind = $kind
        source = $src
        destination = $dst
    }
}

$result = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    aoem_source_root = $AoemSourceRoot
    supervm_aoem_root = $SupervmAoemRoot
    core_copy = $coreCopy
    plugin_output_dir = $OutputDir
    requested = $plan
    copied = $copied
    missing = $missing
    pass = ($missing.Count -eq 0)
}

$summaryJson = Join-Path $OutputDir "aoem-sidecar-sync-summary.json"
$summaryMd = Join-Path $OutputDir "aoem-sidecar-sync-summary.md"

$result | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# AOEM Sidecar Plugin Sync Summary",
    "",
    "- generated_at_utc: $($result.generated_at_utc)",
    "- aoem_source_root: $AoemSourceRoot",
    "- supervm_aoem_root: $SupervmAoemRoot",
    "- core_copy: $(if ($coreCopy) { $coreCopy.destination } else { '(disabled)' })",
    "- plugin_output_dir: $OutputDir",
    "- pass: $($result.pass)",
    "",
    "## Requested",
    ""
)
foreach ($k in $plan) {
    $md += "- $k"
}
$md += ""
$md += "## Copied"
$md += ""
if ($copied.Count -eq 0) {
    $md += "- (none)"
} else {
    foreach ($item in $copied) {
        $md += "- $($item.kind): $($item.destination)"
    }
}
$md += ""
$md += "## Missing"
$md += ""
if ($missing.Count -eq 0) {
    $md += "- (none)"
} else {
    foreach ($k in $missing) {
        $md += "- $k"
    }
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "aoem sidecar plugin sync summary:"
Write-Host "  $summaryJson"
Write-Host "  $summaryMd"
if (-not $result.pass) {
    throw "missing sidecar plugins: $($missing -join ', ')"
}
Write-Host "aoem sidecar plugin sync PASS"
