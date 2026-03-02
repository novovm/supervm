param(
  [string]$AoemRoot = "$(Join-Path $PSScriptRoot '..\..\aoem')",
  [string]$OutManifest = "$(Join-Path $PSScriptRoot '..\..\aoem\manifest\aoem-manifest.json')"
)

$ErrorActionPreference = 'Stop'
$aoemRoot = (Resolve-Path $AoemRoot).Path

$entries = @()

function Add-Entry([string]$Name, [string]$RelDll) {
  $dllPath = Join-Path $aoemRoot $RelDll
  if (-not (Test-Path $dllPath)) {
    return
  }
  $hash = (Get-FileHash -Algorithm SHA256 -Path $dllPath).Hash.ToLowerInvariant()
  $script:entries += [pscustomobject]@{
    name = $Name
    dll = $RelDll.Replace('\\','/')
    sha256 = $hash
    abi_expected = 1
    capabilities_required = [pscustomobject]@{
      execute_ops_v2 = $true
    }
  }
}

Add-Entry -Name 'core' -RelDll 'bin/aoem_ffi.dll'
Add-Entry -Name 'persist' -RelDll 'variants/persist/bin/aoem_ffi.dll'
Add-Entry -Name 'wasm' -RelDll 'variants/wasm/bin/aoem_ffi.dll'

$outDir = Split-Path -Parent $OutManifest
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$manifest = [pscustomobject]@{
  generated_at_utc = (Get-Date).ToUniversalTime().ToString('o')
  aoem_root = $aoemRoot.Replace('\\','/')
  entries = $entries
}

$manifest | ConvertTo-Json -Depth 6 | Set-Content -Path $OutManifest -Encoding UTF8
Write-Output "manifest_written=$OutManifest entries=$($entries.Count)"
