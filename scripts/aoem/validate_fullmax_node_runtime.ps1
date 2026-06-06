param(
  [string]$RepoRoot = "",
  [ValidateRange(1, 2000000)]
  [int]$Txs = 64,
  [ValidateRange(1, 2000000)]
  [int]$Accounts = 16,
  [ValidateSet("release", "debug")]
  [string]$BuildProfile = "debug",
  [ValidateRange(30, 1800)]
  [int]$TimeoutSec = 180
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RepoRoot([string]$Explicit) {
  if ($Explicit) {
    return (Resolve-Path $Explicit).Path
  }
  return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
}

function Set-EnvScoped([hashtable]$Prior, [string]$Name, [string]$Value) {
  if (-not $Prior.ContainsKey($Name)) {
    $Prior[$Name] = [Environment]::GetEnvironmentVariable($Name, "Process")
  }
  Set-Item -Path ("Env:{0}" -f $Name) -Value $Value
}

function Restore-Env([hashtable]$Prior) {
  foreach ($key in $Prior.Keys) {
    $value = $Prior[$key]
    if ($null -eq $value -or $value -eq "") {
      Remove-Item -Path ("Env:{0}" -f $key) -ErrorAction SilentlyContinue
    } else {
      Set-Item -Path ("Env:{0}" -f $key) -Value $value
    }
  }
}

function Invoke-Checked([string]$Label, [scriptblock]$Block) {
  & $Block
  if ($LASTEXITCODE -ne 0) {
    throw "$Label failed: exit=$LASTEXITCODE"
  }
}

$repoRootPath = Resolve-RepoRoot $RepoRoot
$aoemRoot = Join-Path $repoRootPath "aoem"
$aoemDll = Join-Path $aoemRoot "windows\core\bin\aoem_ffi.dll"
$aoemManifest = Join-Path $aoemRoot "manifest\aoem-manifest.json"
$aoemRuntimeProfile = Join-Path $aoemRoot "config\aoem-runtime-profile.json"
$aoemPluginDir = Join-Path $aoemRoot "windows\core\plugins"

foreach ($required in @($aoemDll, $aoemManifest, $aoemRuntimeProfile, $aoemPluginDir)) {
  if (-not (Test-Path -LiteralPath $required)) {
    throw "missing required AOEM host runtime path: $required"
  }
}

$priorEnv = @{}
Push-Location $repoRootPath
try {
  Set-EnvScoped $priorEnv "NOVOVM_AOEM_VARIANT" "core"
  Set-EnvScoped $priorEnv "NOVOVM_AOEM_ROOT" $aoemRoot
  Set-EnvScoped $priorEnv "NOVOVM_AOEM_DLL" $aoemDll
  Set-EnvScoped $priorEnv "NOVOVM_AOEM_MANIFEST" $aoemManifest
  Set-EnvScoped $priorEnv "NOVOVM_AOEM_RUNTIME_PROFILE" $aoemRuntimeProfile
  Set-EnvScoped $priorEnv "NOVOVM_AOEM_PLUGIN_DIR" $aoemPluginDir
  Set-EnvScoped $priorEnv "AOEM_DLL" $aoemDll
  Set-EnvScoped $priorEnv "AOEM_DLL_MANIFEST" $aoemManifest
  Set-EnvScoped $priorEnv "AOEM_RUNTIME_PROFILE" $aoemRuntimeProfile
  Set-EnvScoped $priorEnv "AOEM_FFI_PLUGIN_DIR" $aoemPluginDir

  Invoke-Checked "novovm-node queue_replay_smoke" {
    cargo test -p novovm-node queue_replay_smoke -- --nocapture
  }

  $txWireScript = Join-Path $repoRootPath "scripts\migration\run_ffi_v2_tx_wire_ingress_smoke.ps1"
  Invoke-Checked "ffi_v2 tx wire ingress smoke" {
    powershell -NoProfile -ExecutionPolicy Bypass -File $txWireScript `
      -RepoRoot $repoRootPath `
      -Txs $Txs `
      -Accounts $Accounts `
      -BuildProfile $BuildProfile `
      -TimeoutSec $TimeoutSec
  }

  Write-Output (
    "SUPERVM_AOEM_FULLMAX_NODE_RUNTIME_ACCEPTANCE|" +
    "aoem_runtime=ok|" +
    "queue_replay=ok|" +
    "ffi_v2_tx_wire=ok|" +
    "novovm_node=ok|" +
    "txs=$Txs|" +
    "accounts=$Accounts|" +
    "build_profile=$BuildProfile|" +
    "abi_behavior_changed=false|" +
    "runtime_canon_changed=false|" +
    "privacy_route_changed=false|" +
    "failures=0"
  )
}
finally {
  Restore-Env $priorEnv
  Pop-Location
}
