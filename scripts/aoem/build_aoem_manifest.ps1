param(
  [string]$AoemRoot = "$(Join-Path $PSScriptRoot '..\..\aoem')",
  [string]$OutManifest = "$(Join-Path $PSScriptRoot '..\..\aoem\manifest\aoem-manifest.json')"
)

$ErrorActionPreference = 'Stop'
$aoemRoot = (Resolve-Path $AoemRoot).Path

$entries = @()

function Get-DynlibNameCandidates {
  return @("aoem_ffi.dll", "libaoem_ffi.so", "libaoem_ffi.dylib")
}

function Get-VariantRelBinDir([string]$Name) {
  switch ($Name) {
    'core' { return 'bin' }
    'persist' { return 'variants/persist/bin' }
    'wasm' { return 'variants/wasm/bin' }
    default { throw "unknown variant: $Name" }
  }
}

function Get-DynlibPlatform([string]$LibName) {
  switch ($LibName) {
    "aoem_ffi.dll" { return "windows" }
    "libaoem_ffi.so" { return "linux" }
    "libaoem_ffi.dylib" { return "macos" }
    default { return "unknown" }
  }
}

function Resolve-VariantRelDynlibs([string]$Name) {
  $relBin = Get-VariantRelBinDir -Name $Name
  $out = @()
  foreach ($libName in (Get-DynlibNameCandidates)) {
    $rel = "$relBin/$libName"
    $abs = Join-Path $aoemRoot $rel
    if (Test-Path $abs) {
      $out += $rel
    }
  }
  return $out
}

function Add-Entry([string]$Name) {
  $relDynlibs = @(Resolve-VariantRelDynlibs -Name $Name)
  if ($relDynlibs.Count -eq 0) {
    return
  }
  foreach ($relDynlib in $relDynlibs) {
    $dynlibPath = Join-Path $aoemRoot $relDynlib
    if (-not (Test-Path $dynlibPath)) {
      continue
    }
    $hash = (Get-FileHash -Algorithm SHA256 -Path $dynlibPath).Hash.ToLowerInvariant()
    $relNormalized = $relDynlib.Replace('\\','/')
    $platform = Get-DynlibPlatform -LibName ([IO.Path]::GetFileName($relDynlib))
    $script:entries += [pscustomobject]@{
      name = $Name
      platform = $platform
      dll = $relNormalized
      sha256 = $hash
      abi_expected = 1
      capabilities_required = [pscustomobject]@{
        execute_ops_v2 = $true
      }
    }
  }
}

Add-Entry -Name 'core'
Add-Entry -Name 'persist'
Add-Entry -Name 'wasm'

$outDir = Split-Path -Parent $OutManifest
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$manifest = [pscustomobject]@{
  generated_at_utc = (Get-Date).ToUniversalTime().ToString('o')
  aoem_root = $aoemRoot.Replace('\\','/')
  entries = $entries
}

$manifestJson = $manifest | ConvertTo-Json -Depth 6
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($OutManifest, $manifestJson, $utf8NoBom)
Write-Output "manifest_written=$OutManifest entries=$($entries.Count)"
