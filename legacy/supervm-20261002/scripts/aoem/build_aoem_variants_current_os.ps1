# Copyright (c) 2026 Xonovo Technology
# All rights reserved.
# Author: Xonovo Technology

param(
  [string]$AoemSourceRoot = "",
  [string]$OutRoot = "$(Join-Path $PSScriptRoot '..\..\artifacts\aoem-platform-build')",
  [ValidateSet('auto', 'windows', 'linux', 'macos')]
  [string]$Platform = 'auto',
  [string[]]$Variants = @('core', 'persist', 'wasm'),
  [switch]$Clean
)

$ErrorActionPreference = 'Stop'

function Assert-Path([string]$Path, [string]$Label) {
  if (-not (Test-Path $Path)) {
    throw "missing required path: $Label => $Path"
  }
}

function Detect-Platform([string]$p) {
  if ($p -ne 'auto') { return $p }
  if ($IsWindows) { return 'windows' }
  if ($IsLinux) { return 'linux' }
  if ($IsMacOS) { return 'macos' }
  throw "unable to detect host platform"
}

function Variant-Features([string]$variant) {
  switch ($variant) {
    'core' { return '' }
    'persist' { return 'rocksdb-persistence' }
    'wasm' { return 'wasmtime-runtime' }
    default { throw "unsupported variant: $variant" }
  }
}

if ([string]::IsNullOrWhiteSpace($AoemSourceRoot)) {
  if ($env:AOEM_SOURCE_ROOT) {
    $AoemSourceRoot = $env:AOEM_SOURCE_ROOT
  } else {
    $AoemSourceRoot = Join-Path (Join-Path $PSScriptRoot '..\..\..') 'AOEM'
  }
}

$aoemRoot = (Resolve-Path $AoemSourceRoot).Path
$platform = Detect-Platform $Platform
$manifestPath = Join-Path $aoemRoot 'crates/ffi/aoem-ffi/Cargo.toml'
Assert-Path $manifestPath 'AOEM FFI Cargo.toml'

$libName = switch ($platform) {
  'windows' { 'aoem_ffi.dll' }
  'linux' { 'libaoem_ffi.so' }
  'macos' { 'libaoem_ffi.dylib' }
  default { throw "unsupported platform: $platform" }
}

$headerSrc = Join-Path $aoemRoot 'crates/ffi/aoem-ffi/include/aoem.h'
Assert-Path $headerSrc 'aoem.h'

if ($Clean) {
  foreach ($variant in $Variants) {
    $targetDir = Join-Path $aoemRoot ("cargo-target-ffi-" + $platform + "-" + $variant)
    if (Test-Path $targetDir) {
      Remove-Item -Recurse -Force $targetDir
    }
  }
}

foreach ($variant in $Variants) {
  $features = Variant-Features $variant
  $targetDir = Join-Path $aoemRoot ("cargo-target-ffi-" + $platform + "-" + $variant)
  $args = @('build', '--release', '--manifest-path', $manifestPath, '--target-dir', $targetDir)
  if (-not [string]::IsNullOrWhiteSpace($features)) {
    $args += @('--features', $features)
  }
  & cargo @args
  if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed for variant=$variant rc=$LASTEXITCODE"
  }

  $builtLib = Join-Path $targetDir ("release/" + $libName)
  Assert-Path $builtLib "$variant output library"

  $dstLibDir = Join-Path $OutRoot "$platform/$variant/bin"
  New-Item -ItemType Directory -Force -Path $dstLibDir | Out-Null
  Copy-Item -Force -Path $builtLib -Destination (Join-Path $dstLibDir $libName)
}

$dstIncludeDir = Join-Path $OutRoot "$platform/include"
New-Item -ItemType Directory -Force -Path $dstIncludeDir | Out-Null
Copy-Item -Force -Path $headerSrc -Destination (Join-Path $dstIncludeDir 'aoem.h')

$meta = @{
  platform = $platform
  variants = $Variants
  aoem_source_root = ($aoemRoot -replace '\\', '/')
  output_root = ((Resolve-Path $OutRoot).Path -replace '\\', '/')
  library_name = $libName
  built_at_utc = (Get-Date).ToUniversalTime().ToString('s') + 'Z'
}
$metaPath = Join-Path $OutRoot "$platform/BUILD-META.json"
$meta | ConvertTo-Json -Depth 5 | Set-Content -Path $metaPath -Encoding UTF8

Write-Output "build_ready_platform=$platform"
Write-Output "build_ready_root=$OutRoot"
