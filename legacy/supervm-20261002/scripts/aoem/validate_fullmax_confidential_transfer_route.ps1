param(
  [string]$RepoRoot = "",
  [string]$AoemRoot = "",
  [string]$AoemHostArtifactRoot = "",
  [ValidateRange(1, 2000000)]
  [int]$BatchCount = 64
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RepoRoot([string]$Explicit) {
  if ($Explicit) {
    return (Resolve-Path $Explicit).Path
  }
  return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
}

function Resolve-AoemRoot([string]$Explicit, [string]$RepoRootPath) {
  if ($Explicit) {
    return (Resolve-Path $Explicit).Path
  }
  return (Resolve-Path (Join-Path $RepoRootPath "aoem")).Path
}

function Resolve-AoemHostArtifactRoot([string]$Explicit, [string]$RepoRootPath) {
  if ($Explicit) {
    return (Resolve-Path $Explicit).Path
  }
  $default = Join-Path $RepoRootPath "..\AOEM\dist\aoem-compute-native-host-integration-v1.2"
  return (Resolve-Path $default).Path
}

function Assert-Path([string]$Path, [string]$Label) {
  if (-not (Test-Path -LiteralPath $Path)) {
    throw "missing ${Label}: $Path"
  }
}

function Read-Json([string]$Path) {
  Assert-Path $Path "json"
  return Get-Content -Raw -Path $Path | ConvertFrom-Json
}

function Assert-False($Value, [string]$Label) {
  if ([bool]$Value) {
    throw "$Label must be false"
  }
}

function Assert-LineContains([string]$Line, [string]$Needle, [string]$Label) {
  if (-not $Line -or -not $Line.Contains($Needle)) {
    throw "$Label missing '$Needle': $Line"
  }
}

function Get-CommandPath([string]$Name) {
  $cmd = Get-Command $Name -ErrorAction SilentlyContinue
  if ($null -eq $cmd) {
    return ""
  }
  return $cmd.Source
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

$repoRootPath = Resolve-RepoRoot $RepoRoot
$aoemRootPath = Resolve-AoemRoot $AoemRoot $repoRootPath
$hostArtifactRoot = Resolve-AoemHostArtifactRoot $AoemHostArtifactRoot $repoRootPath

$windowsDll = Join-Path $aoemRootPath "windows\core\bin\aoem_ffi.dll"
$runtimeProfile = Join-Path $aoemRootPath "config\aoem-runtime-profile.json"
$runtimeManifest = Join-Path $aoemRootPath "manifest\aoem-manifest.json"
$pluginDir = Join-Path $aoemRootPath "windows\core\plugins"
$sdkManifestPath = Join-Path $aoemRootPath "aoem-sdk-manifest.json"
$hostManifestPath = Join-Path $hostArtifactRoot "aoem-sdk-manifest.json"
$hostSource = Join-Path $hostArtifactRoot "examples\hosted_confidential_transfer_smoke.c"
$hostInclude = Join-Path $hostArtifactRoot "include"
$ffiSurfaceDoc = Join-Path $hostArtifactRoot "docs\ffi-production-surface.md"

foreach ($required in @(
    $windowsDll,
    $runtimeProfile,
    $runtimeManifest,
    $pluginDir,
    $sdkManifestPath,
    $hostManifestPath,
    $hostSource,
    $hostInclude,
    $ffiSurfaceDoc
  )) {
  Assert-Path $required "required route acceptance input"
}

$sdkManifest = Read-Json $sdkManifestPath
$hostManifest = Read-Json $hostManifestPath

if ($sdkManifest.confidential_transfer.profile_id -ne "confidential_transfer_v1") {
  throw "SUPERVM SDK manifest does not expose confidential_transfer_v1 as the canonical privacy profile"
}
Assert-False $sdkManifest.claims.public_ffi_abi_changed "SUPERVM SDK public_ffi_abi_changed"
Assert-False $sdkManifest.claims.runtime_canon_changed "SUPERVM SDK runtime_canon_changed"

if ($hostManifest.host_profile.name -ne "confidential_transfer_v1") {
  throw "host artifact route policy does not name confidential_transfer_v1"
}
if ($hostManifest.host_profile.canonical_route_id -ne "ringct_prove_cache_admitted_v1") {
  throw "host artifact canonical_route_id drifted: $($hostManifest.host_profile.canonical_route_id)"
}
if ($hostManifest.host_profile.generation_symbol -ne "aoem_ringct_prove_batch_v1") {
  throw "host artifact generation symbol is not the batch canonical symbol"
}
if ($hostManifest.host_profile.validation_symbol -ne "aoem_privacy_execute_v1") {
  throw "host artifact validation symbol drifted"
}
Assert-False $hostManifest.host_profile.fallback_used "host artifact fallback_used"
Assert-False $hostManifest.host_profile.cpu_verify_used "host artifact cpu_verify_used"
Assert-False $hostManifest.host_profile.duplicate_privacy_dispatch "host artifact duplicate_privacy_dispatch"
Assert-False $hostManifest.host_profile.old_slow_path_reachable "host artifact old_slow_path_reachable"

$policy = $hostManifest.privacy_transfer_route_policy
if ($policy.canonical_production_route -ne "confidential_transfer_v1") {
  throw "privacy route policy canonical production route drifted: $($policy.canonical_production_route)"
}
if ($policy.canonical_route_id -ne "ringct_prove_cache_admitted_v1") {
  throw "privacy route policy canonical route id drifted: $($policy.canonical_route_id)"
}
Assert-False $policy.safe_and_plausibly_faster_candidate_found "privacy route policy safe_and_plausibly_faster_candidate_found"
if ([int]$policy.product_route_switch -ne 0) {
  throw "privacy route policy product_route_switch must be 0"
}
if ([int]$policy.performance_claim -ne 0) {
  throw "privacy route policy performance_claim must be 0"
}

$v1 = $policy.non_canonical_retained_assets |
  Where-Object { $_.profile -eq "ringct_batch_verifiable_transfer_v1" } |
  Select-Object -First 1
$v2 = $policy.non_canonical_retained_assets |
  Where-Object { $_.profile -eq "ringct_batch_verifiable_transfer_v2_folded_lr_v1" } |
  Select-Object -First 1
if ($null -eq $v1 -or $null -eq $v2) {
  throw "privacy route policy missing batch-verifiable retained assets"
}
if ([bool]$v1.production_recommended) {
  throw "ringct_batch_verifiable_transfer_v1 must not be production recommended"
}
if ([bool]$v2.production_recommended) {
  throw "ringct_batch_verifiable_transfer_v2_folded_lr_v1 must not be production recommended"
}
if ([bool]$v2.promotion_ready) {
  throw "ringct_batch_verifiable_transfer_v2_folded_lr_v1 promotion_ready must be false"
}

$ffiSurfaceText = Get-Content -Raw -Path $ffiSurfaceDoc
foreach ($needle in @(
    "confidential_transfer_v1:",
    "canonical production route",
    "route_id=ringct_prove_cache_admitted_v1",
    "ringct_batch_verifiable_transfer_v1:",
    "non-canonical retained research/regression asset",
    "ringct_batch_verifiable_transfer_v2_folded_lr_v1:"
  )) {
  if (-not $ffiSurfaceText.Contains($needle)) {
    throw "ffi production surface doc missing route policy text: $needle"
  }
}

$clang = Get-CommandPath "clang"
if (-not $clang) {
  throw "clang is required for confidential transfer route smoke"
}

$ctExe = Join-Path $env:TEMP "supervm_fullmax_confidential_transfer_route.exe"
$priorEnv = @{}
Push-Location $repoRootPath
try {
  Set-EnvScoped $priorEnv "AOEM_DLL" $windowsDll
  Set-EnvScoped $priorEnv "AOEM_DLL_MANIFEST" $runtimeManifest
  Set-EnvScoped $priorEnv "AOEM_RUNTIME_PROFILE" $runtimeProfile
  Set-EnvScoped $priorEnv "AOEM_FFI_PLUGIN_DIR" $pluginDir

  & $clang -std=c11 -Wall -Wextra -I $hostInclude $hostSource -o $ctExe
  if ($LASTEXITCODE -ne 0) {
    throw "compile confidential transfer route host failed: exit=$LASTEXITCODE"
  }

  $ctOutput = & $ctExe $windowsDll --run-prove --batch-count $BatchCount
  if ($LASTEXITCODE -ne 0) {
    throw "confidential transfer route host failed: exit=$LASTEXITCODE output=$ctOutput"
  }
}
finally {
  Restore-Env $priorEnv
  Pop-Location
}

$hostLine = ($ctOutput | Where-Object { $_ -like "AOEM_CONFIDENTIAL_TRANSFER_HOST|*" } | Select-Object -First 1)
$attrLine = ($ctOutput | Where-Object { $_ -like "CONFIDENTIAL_TRANSFER_PROVE_ATTR|*" } | Select-Object -First 1)

foreach ($needle in @(
    "profile=confidential_transfer_v1",
    "ringct=ok",
    "prove=ok",
    "generation_symbol=aoem_ringct_prove_batch_v1",
    "privacy_execute=ok",
    "verify=ok",
    "amount_hidden=ok",
    "batch_count=$BatchCount",
    "verify_mode=prove_admitted",
    "ffi_abi_unchanged=1",
    "runtime_canon_changed=0",
    "failures=0"
  )) {
  Assert-LineContains $hostLine $needle "confidential transfer host line"
}

foreach ($needle in @(
    "profile=confidential_transfer_v1",
    "batch_count=$BatchCount",
    "generation_symbol=aoem_ringct_prove_batch_v1",
    "route_id=ringct_prove_cache_admitted_v1",
    "fallback_used=false",
    "cpu_verify_used=false",
    "duplicate_verify_count=0",
    "verify_backend_used=prove_admitted_cache",
    "dedicated_ringct_kernel_used=false",
    "dedicated_coverage=none",
    "full_tx_verify_coverage=not_executed_prove_admitted",
    "old_slow_path_reachable=false",
    "privacy_execute_under_50ms=ok",
    "proof=ok",
    "verify=ok",
    "malformed=ok",
    "performance_claim=0"
  )) {
  Assert-LineContains $attrLine $needle "confidential transfer attribution line"
}

Write-Output (
  "SUPERVM_AOEM_FULLMAX_CONFIDENTIAL_TRANSFER_ROUTE|" +
  "profile=confidential_transfer_v1|" +
  "production_route=confidential_transfer_v1|" +
  "route_id=ringct_prove_cache_admitted_v1|" +
  "generation_symbol=aoem_ringct_prove_batch_v1|" +
  "privacy_execute=ok|" +
  "verify_backend=prove_admitted_cache|" +
  "fallback_used=false|" +
  "cpu_verify_used=false|" +
  "duplicate_verify_count=0|" +
  "old_slow_path_reachable=false|" +
  "batch_verifiable_profiles_non_canonical=ok|" +
  "product_route_switch=0|" +
  "performance_claim=0|" +
  "abi_behavior_changed=false|" +
  "runtime_canon_changed=false|" +
  "privacy_route_changed=false|" +
  "failures=0"
)
