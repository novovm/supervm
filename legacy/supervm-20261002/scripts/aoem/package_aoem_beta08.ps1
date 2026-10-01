param(
  [string]$AoemRoot = "$(Join-Path $PSScriptRoot '..\..\aoem')",
  [string]$OutRoot = "$(Join-Path $PSScriptRoot '..\..\artifacts\aoem-beta08')",
  [string]$PlatformBuildRoot = "$(Join-Path $PSScriptRoot '..\..\artifacts\aoem-platform-build')",
  [string]$Version = 'beta0.8',
  [bool]$RequireWindowsVariants = $true,
  [bool]$CreateZip = $true,
  [switch]$RequireFullPlatform,
  [switch]$SkipMacOS
)

$ErrorActionPreference = 'Stop'
$aoemRoot = (Resolve-Path $AoemRoot).Path
$ts = Get-Date -Format 'yyyyMMdd-HHmmss'
$bundle = Join-Path $OutRoot $ts
New-Item -ItemType Directory -Force -Path $bundle | Out-Null

# template layout
$paths = @(
  'windows/core/bin',
  'windows/persist/bin',
  'windows/wasm/bin',
  'windows/include',
  'linux/core/bin',
  'linux/persist/bin',
  'linux/wasm/bin',
  'linux/include'
)
if (-not $SkipMacOS) {
  $paths += @(
    'macos/core/bin',
    'macos/persist/bin',
    'macos/wasm/bin',
    'macos/include'
  )
}
foreach ($p in $paths) {
  New-Item -ItemType Directory -Force -Path (Join-Path $bundle $p) | Out-Null
}

function Assert-Exists([string]$path, [string]$label) {
  if (-not (Test-Path $path)) {
    throw "missing required file: $label => $path"
  }
}

function Copy-IfExists([string]$src,[string]$dstDir) {
  if (Test-Path $src) {
    Copy-Item -Force -Path $src -Destination $dstDir
  }
}

function Copy-IfExistsTo([string]$src, [string]$dstFile) {
  if (Test-Path $src) {
    Copy-Item -Force -Path $src -Destination $dstFile
  }
}

function First-Existing([string[]]$paths) {
  foreach ($p in $paths) {
    if (Test-Path $p) { return $p }
  }
  return $null
}

$coreDllSrc = First-Existing @(
  (Join-Path $aoemRoot 'bin/aoem_ffi.dll'),
  (Join-Path $PlatformBuildRoot 'windows/core/bin/aoem_ffi.dll')
)
$persistDllSrc = First-Existing @(
  (Join-Path $aoemRoot 'variants/persist/bin/aoem_ffi.dll'),
  (Join-Path $PlatformBuildRoot 'windows/persist/bin/aoem_ffi.dll')
)
$wasmDllSrc = First-Existing @(
  (Join-Path $aoemRoot 'variants/wasm/bin/aoem_ffi.dll'),
  (Join-Path $PlatformBuildRoot 'windows/wasm/bin/aoem_ffi.dll')
)
$headerSrc = First-Existing @(
  (Join-Path $aoemRoot 'include/aoem.h'),
  (Join-Path $PlatformBuildRoot 'windows/include/aoem.h')
)
$installInfoSrc = Join-Path $aoemRoot 'INSTALL-INFO.txt'
$linuxCoreSrc = Join-Path $PlatformBuildRoot 'linux/core/bin/libaoem_ffi.so'
$linuxPersistSrc = Join-Path $PlatformBuildRoot 'linux/persist/bin/libaoem_ffi.so'
$linuxWasmSrc = Join-Path $PlatformBuildRoot 'linux/wasm/bin/libaoem_ffi.so'
$linuxHeaderSrc = Join-Path $PlatformBuildRoot 'linux/include/aoem.h'
$macCoreSrc = Join-Path $PlatformBuildRoot 'macos/core/bin/libaoem_ffi.dylib'
$macPersistSrc = Join-Path $PlatformBuildRoot 'macos/persist/bin/libaoem_ffi.dylib'
$macWasmSrc = Join-Path $PlatformBuildRoot 'macos/wasm/bin/libaoem_ffi.dylib'
$macHeaderSrc = Join-Path $PlatformBuildRoot 'macos/include/aoem.h'

Assert-Exists $coreDllSrc 'windows core dll'
Assert-Exists $headerSrc 'aoem.h'
Assert-Exists $installInfoSrc 'INSTALL-INFO.txt'
if ($RequireWindowsVariants) {
  Assert-Exists $persistDllSrc 'windows persist dll'
  Assert-Exists $wasmDllSrc 'windows wasm dll'
}
if ($RequireFullPlatform) {
  Assert-Exists $linuxCoreSrc 'linux core so'
  Assert-Exists $linuxPersistSrc 'linux persist so'
  Assert-Exists $linuxWasmSrc 'linux wasm so'
  Assert-Exists $linuxHeaderSrc 'linux aoem.h'
  if (-not $SkipMacOS) {
    Assert-Exists $macCoreSrc 'macos core dylib'
    Assert-Exists $macPersistSrc 'macos persist dylib'
    Assert-Exists $macWasmSrc 'macos wasm dylib'
    Assert-Exists $macHeaderSrc 'macos aoem.h'
  }
}

# windows files from current workspace install
Copy-IfExists $coreDllSrc (Join-Path $bundle 'windows/core/bin')
Copy-IfExists $persistDllSrc (Join-Path $bundle 'windows/persist/bin')
Copy-IfExists $wasmDllSrc (Join-Path $bundle 'windows/wasm/bin')
Copy-IfExists $headerSrc (Join-Path $bundle 'windows/include')
Copy-IfExists $installInfoSrc $bundle

# linux/macos files from platform build root
Copy-IfExistsTo $linuxCoreSrc (Join-Path $bundle 'linux/core/bin/libaoem_ffi.so')
Copy-IfExistsTo $linuxPersistSrc (Join-Path $bundle 'linux/persist/bin/libaoem_ffi.so')
Copy-IfExistsTo $linuxWasmSrc (Join-Path $bundle 'linux/wasm/bin/libaoem_ffi.so')
if (Test-Path $linuxHeaderSrc) {
  Copy-IfExistsTo $linuxHeaderSrc (Join-Path $bundle 'linux/include/aoem.h')
} else {
  Copy-IfExistsTo $headerSrc (Join-Path $bundle 'linux/include/aoem.h')
}

if (-not $SkipMacOS) {
  Copy-IfExistsTo $macCoreSrc (Join-Path $bundle 'macos/core/bin/libaoem_ffi.dylib')
  Copy-IfExistsTo $macPersistSrc (Join-Path $bundle 'macos/persist/bin/libaoem_ffi.dylib')
  Copy-IfExistsTo $macWasmSrc (Join-Path $bundle 'macos/wasm/bin/libaoem_ffi.dylib')
  if (Test-Path $macHeaderSrc) {
    Copy-IfExistsTo $macHeaderSrc (Join-Path $bundle 'macos/include/aoem.h')
  } else {
    Copy-IfExistsTo $headerSrc (Join-Path $bundle 'macos/include/aoem.h')
  }
}

# placeholders when platform libs are missing
if (-not (Test-Path (Join-Path $bundle 'linux/core/bin/libaoem_ffi.so'))) {
  "missing linux build outputs: run scripts/aoem/build_aoem_variants_current_os.ps1 on Linux host" |
    Set-Content -Path (Join-Path $bundle 'linux/README.txt') -Encoding UTF8
}
if ((-not $SkipMacOS) -and (-not (Test-Path (Join-Path $bundle 'macos/core/bin/libaoem_ffi.dylib')))) {
  "missing macOS build outputs: run scripts/aoem/build_aoem_variants_current_os.ps1 on macOS host" |
    Set-Content -Path (Join-Path $bundle 'macos/README.txt') -Encoding UTF8
}

$versionTxt = @(
  "version=$Version",
  "bundle_ts=$ts",
  "skip_macos=$SkipMacOS",
  "source_aoem_root=$($aoemRoot.Replace('\\','/'))"
)
$versionTxt | Set-Content -Path (Join-Path $bundle 'VERSION.txt') -Encoding UTF8

# capabilities snapshot (windows core if present)
$coreDll = Join-Path $bundle 'windows/core/bin/aoem_ffi.dll'
if (Test-Path $coreDll) {
  $verify = & (Join-Path $PSScriptRoot 'verify_aoem_binary.ps1') -DllPath $coreDll -Variant 'core' | ConvertFrom-Json
  $verify | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $bundle 'CAPABILITIES.json') -Encoding UTF8
}

# sha256 sums
$sumFile = Join-Path $bundle 'SHA256SUMS'
if (Test-Path $sumFile) { Remove-Item $sumFile -Force }
$bundleNorm = (Resolve-Path $bundle).Path -replace '^\\\\\?\\', ''
Get-ChildItem -Path $bundle -Recurse -File |
  Where-Object { $_.Name -ne 'SHA256SUMS' } |
  ForEach-Object {
    $hash = (Get-FileHash -Algorithm SHA256 -Path $_.FullName).Hash.ToLowerInvariant()
    $fullNorm = ($_.FullName -replace '^\\\\\?\\', '')
    if ($fullNorm.StartsWith($bundleNorm, [System.StringComparison]::OrdinalIgnoreCase)) {
      $rel = $fullNorm.Substring($bundleNorm.Length).TrimStart('\') -replace '\\','/'
    } else {
      $rel = [System.IO.Path]::GetFileName($fullNorm)
    }
    "$hash  $rel" | Add-Content -Path $sumFile -Encoding UTF8
  }

# manifest for current aoem install
$manifestOut = Join-Path $bundle 'aoem-manifest.json'
& (Join-Path $PSScriptRoot 'build_aoem_manifest.ps1') -AoemRoot $aoemRoot -OutManifest $manifestOut | Out-Null

# release index
$releaseIndex = Join-Path $bundle 'RELEASE-INDEX.md'
$lines = @(
  "# AOEM Release Assets Index",
  "",
  "version=$Version",
  "bundle_ts=$ts",
  "require_full_platform=$RequireFullPlatform",
  "skip_macos=$SkipMacOS",
  "",
  "## Included assets",
  "- windows/core/bin/aoem_ffi.dll: $(Test-Path (Join-Path $bundle 'windows/core/bin/aoem_ffi.dll'))",
  "- windows/persist/bin/aoem_ffi.dll: $(Test-Path (Join-Path $bundle 'windows/persist/bin/aoem_ffi.dll'))",
  "- windows/wasm/bin/aoem_ffi.dll: $(Test-Path (Join-Path $bundle 'windows/wasm/bin/aoem_ffi.dll'))",
  "- linux/core/bin/libaoem_ffi.so: $(Test-Path (Join-Path $bundle 'linux/core/bin/libaoem_ffi.so'))",
  "- linux/persist/bin/libaoem_ffi.so: $(Test-Path (Join-Path $bundle 'linux/persist/bin/libaoem_ffi.so'))",
  "- linux/wasm/bin/libaoem_ffi.so: $(Test-Path (Join-Path $bundle 'linux/wasm/bin/libaoem_ffi.so'))",
  "- macos/core/bin/libaoem_ffi.dylib: $(if ($SkipMacOS) { 'excluded_by_flag' } else { [string](Test-Path (Join-Path $bundle 'macos/core/bin/libaoem_ffi.dylib')) })",
  "- macos/persist/bin/libaoem_ffi.dylib: $(if ($SkipMacOS) { 'excluded_by_flag' } else { [string](Test-Path (Join-Path $bundle 'macos/persist/bin/libaoem_ffi.dylib')) })",
  "- macos/wasm/bin/libaoem_ffi.dylib: $(if ($SkipMacOS) { 'excluded_by_flag' } else { [string](Test-Path (Join-Path $bundle 'macos/wasm/bin/libaoem_ffi.dylib')) })",
  "- windows/include/aoem.h",
  "- aoem-manifest.json",
  "- CAPABILITIES.json (core snapshot, if available)",
  "- SHA256SUMS",
  "",
  "## Distribution rule",
  "- Repository tracks minimal host set.",
  "- Full variant binaries should be distributed via GitHub Releases assets.",
  "- Use -RequireFullPlatform for final all-platform release gate."
)
$lines | Set-Content -Path $releaseIndex -Encoding UTF8

$zipPath = Join-Path $OutRoot ("aoem-$Version-$ts.zip")
if ($CreateZip) {
  if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
  Compress-Archive -Path (Join-Path $bundle '*') -DestinationPath $zipPath -CompressionLevel Optimal
}

Write-Output "bundle_ready=$bundle"
if ($CreateZip) { Write-Output "zip_ready=$zipPath" }
