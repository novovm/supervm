param(
  [string]$AoemRoot = "$(Join-Path $PSScriptRoot '..\..\aoem')",
  [string]$OutRoot = "$(Join-Path $PSScriptRoot '..\..\artifacts\aoem-beta08')",
  [string]$Version = 'beta0.8'
)

$ErrorActionPreference = 'Stop'
$aoemRoot = (Resolve-Path $AoemRoot).Path
$ts = Get-Date -Format 'yyyyMMdd-HHmmss'
$bundle = Join-Path $OutRoot $ts

# template layout
$paths = @(
  'windows/core/bin',
  'windows/persist/bin',
  'windows/wasm/bin',
  'windows/include',
  'linux/core/bin',
  'linux/persist/bin',
  'linux/wasm/bin',
  'linux/include',
  'macos/core/bin',
  'macos/persist/bin',
  'macos/wasm/bin',
  'macos/include'
)
foreach ($p in $paths) {
  New-Item -ItemType Directory -Force -Path (Join-Path $bundle $p) | Out-Null
}

function Copy-IfExists([string]$src,[string]$dstDir) {
  if (Test-Path $src) {
    Copy-Item -Force -Path $src -Destination $dstDir
  }
}

# windows files from current workspace install
Copy-IfExists (Join-Path $aoemRoot 'bin/aoem_ffi.dll') (Join-Path $bundle 'windows/core/bin')
Copy-IfExists (Join-Path $aoemRoot 'variants/persist/bin/aoem_ffi.dll') (Join-Path $bundle 'windows/persist/bin')
Copy-IfExists (Join-Path $aoemRoot 'variants/wasm/bin/aoem_ffi.dll') (Join-Path $bundle 'windows/wasm/bin')
Copy-IfExists (Join-Path $aoemRoot 'include/aoem.h') (Join-Path $bundle 'windows/include')
Copy-IfExists (Join-Path $aoemRoot 'INSTALL-INFO.txt') $bundle

# placeholders for non-windows
"place linux build outputs here: libaoem_ffi.so" | Set-Content -Path (Join-Path $bundle 'linux/README.txt') -Encoding UTF8
"place macOS build outputs here: libaoem_ffi.dylib" | Set-Content -Path (Join-Path $bundle 'macos/README.txt') -Encoding UTF8

$versionTxt = @(
  "version=$Version",
  "bundle_ts=$ts",
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
Get-ChildItem -Path $bundle -Recurse -File |
  Where-Object { $_.Name -ne 'SHA256SUMS' } |
  ForEach-Object {
    $hash = (Get-FileHash -Algorithm SHA256 -Path $_.FullName).Hash.ToLowerInvariant()
    $rel = $_.FullName.Substring($bundle.Length).TrimStart('\\') -replace '\\','/'
    "$hash  $rel" | Add-Content -Path $sumFile -Encoding UTF8
  }

# manifest for current aoem install
$manifestOut = Join-Path $bundle 'aoem-manifest.json'
& (Join-Path $PSScriptRoot 'build_aoem_manifest.ps1') -AoemRoot $aoemRoot -OutManifest $manifestOut | Out-Null

Write-Output "bundle_ready=$bundle"
