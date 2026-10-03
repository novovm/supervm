param(
  [string]$AoemRoot = "$(Join-Path $PSScriptRoot '..\..\aoem')",
  [string]$OutManifest = "$(Join-Path $PSScriptRoot '..\..\aoem\manifest\aoem-manifest.json')",
  [string]$ReleaseVersion = "Beta 0.8"
)

$ErrorActionPreference = 'Stop'
$aoemRoot = (Resolve-Path $AoemRoot).Path

$entries = @()

function Get-DynlibNameCandidates {
  return @("aoem_ffi.dll", "libaoem_ffi.so", "libaoem_ffi.dylib")
}

# Unified runtime manifest tracks only platform core dynlibs.
# persist/wasm/zkvm/etc are composed at runtime from sidecar plugins.
function Get-DynlibPlatform([string]$LibName) {
  switch ($LibName) {
    "aoem_ffi.dll" { return "windows" }
    "libaoem_ffi.so" { return "linux" }
    "libaoem_ffi.dylib" { return "macos" }
    default { return "unknown" }
  }
}

function Add-EntryFromRel([string]$Name, [string]$Platform, [string]$RelDynlib) {
  $dynlibPath = Join-Path $aoemRoot $RelDynlib
  if (-not (Test-Path $dynlibPath)) {
    return
  }
  $hash = (Get-FileHash -Algorithm SHA256 -Path $dynlibPath).Hash.ToLowerInvariant()
  $relNormalized = $RelDynlib.Replace('\\','/')
  $script:entries += [pscustomobject]@{
    name = $Name
    platform = $Platform
    dll = $relNormalized
    sha256 = $hash
    abi_expected = 1
    capabilities_required = [pscustomobject]@{
      execute_ops_v2 = $true
    }
  }
}

function Add-CoreEntries {
  foreach ($libName in (Get-DynlibNameCandidates)) {
    $platform = Get-DynlibPlatform -LibName $libName
    switch ($platform) {
      "windows" {
        Add-EntryFromRel -Name "core" -Platform $platform -RelDynlib "windows/core/bin/$libName"
      }
      "linux" {
        Add-EntryFromRel -Name "core" -Platform $platform -RelDynlib "linux/core/bin/$libName"
      }
      "macos" {
        Add-EntryFromRel -Name "core" -Platform $platform -RelDynlib "macos/core/bin/$libName"
      }
      default {
        # no-op
      }
    }
    # legacy fallback layout compatibility
    Add-EntryFromRel -Name "core" -Platform $platform -RelDynlib "bin/$libName"
  }
}

Add-CoreEntries

$outDir = Split-Path -Parent $OutManifest
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$manifest = [pscustomobject]@{
  generated_at_utc = (Get-Date).ToUniversalTime().ToString('o')
  release_version = $ReleaseVersion
  aoem_root = $aoemRoot.Replace('\\','/')
  entries = $entries
}

$manifestJson = $manifest | ConvertTo-Json -Depth 6
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($OutManifest, $manifestJson, $utf8NoBom)
Write-Output "manifest_written=$OutManifest entries=$($entries.Count)"
