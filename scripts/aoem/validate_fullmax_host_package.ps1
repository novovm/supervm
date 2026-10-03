param(
  [string]$RepoRoot = "",
  [string]$AoemRoot = "",
  [switch]$SkipProofEngine,
  [switch]$SkipWorkerAdapter,
  [switch]$SkipConfidentialTransfer
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class FullmaxWinNative {
  [DllImport("kernel32", SetLastError=true, CharSet=CharSet.Ansi)]
  public static extern IntPtr LoadLibrary(string lpFileName);
  [DllImport("kernel32", SetLastError=true, CharSet=CharSet.Ansi)]
  public static extern IntPtr GetProcAddress(IntPtr hModule, string procName);
  [DllImport("kernel32", SetLastError=true)]
  public static extern bool FreeLibrary(IntPtr hModule);
}
[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
public delegate IntPtr FullmaxCapFn();
"@

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

function Assert-Checksums([string]$Root) {
  $sumPath = Join-Path $Root "CHECKSUMS.sha256"
  Assert-Path $sumPath "CHECKSUMS.sha256"
  $failures = @()
  $lineNo = 0
  foreach ($line in Get-Content -Path $sumPath) {
    $lineNo++
    if ([string]::IsNullOrWhiteSpace($line)) {
      continue
    }
    $parts = $line -split "  ", 2
    if ($parts.Count -ne 2) {
      $failures += "line ${lineNo}: invalid checksum format"
      continue
    }
    $expected = $parts[0].Trim().ToLowerInvariant()
    $rel = $parts[1].Trim()
    $path = Join-Path $Root ($rel -replace "/", [IO.Path]::DirectorySeparatorChar)
    if (-not (Test-Path -LiteralPath $path)) {
      $failures += "${rel}: missing"
      continue
    }
    $actual = (Get-FileHash -Algorithm SHA256 -Path $path).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
      $failures += "${rel}: checksum mismatch expected=$expected actual=$actual"
    }
  }
  if ($failures.Count -gt 0) {
    throw "checksum failures: $($failures -join '; ')"
  }
}

function Get-CommandPath([string]$Name) {
  $cmd = Get-Command $Name -ErrorAction SilentlyContinue
  if ($null -eq $cmd) {
    return ""
  }
  return $cmd.Source
}

function Invoke-Checked([string]$Label, [scriptblock]$Block) {
  & $Block
  if ($LASTEXITCODE -ne 0) {
    throw "$Label failed: exit=$LASTEXITCODE"
  }
}

function Read-AoemCapabilities([string]$DllPath) {
  $mod = [FullmaxWinNative]::LoadLibrary($DllPath)
  if ($mod -eq [IntPtr]::Zero) {
    throw "LoadLibrary failed: $DllPath"
  }
  try {
    $capPtr = [FullmaxWinNative]::GetProcAddress($mod, "aoem_capabilities_json")
    if ($capPtr -eq [IntPtr]::Zero) {
      throw "aoem_capabilities_json export missing"
    }
    $capFn = [Runtime.InteropServices.Marshal]::GetDelegateForFunctionPointer($capPtr, [FullmaxCapFn])
    $capC = $capFn.Invoke()
    if ($capC -eq [IntPtr]::Zero) {
      throw "aoem_capabilities_json returned null"
    }
    $capJson = [Runtime.InteropServices.Marshal]::PtrToStringAnsi($capC)
    return $capJson | ConvertFrom-Json
  }
  finally {
    [void][FullmaxWinNative]::FreeLibrary($mod)
  }
}

$repoRootPath = Resolve-RepoRoot $RepoRoot
$aoemRootPath = Resolve-AoemRoot $AoemRoot $repoRootPath
$windowsDll = Join-Path $aoemRootPath "windows\core\bin\aoem_ffi.dll"
$windowsHeader = Join-Path $aoemRootPath "windows\include\aoem.h"
$windowsIncludeDir = Split-Path -Parent $windowsHeader
$sdkManifestPath = Join-Path $aoemRootPath "aoem-sdk-manifest.json"
$runtimeManifestPath = Join-Path $aoemRootPath "manifest\aoem-manifest.json"
$windowsManifestPath = Join-Path $aoemRootPath "windows\manifest.json"

Assert-Path $windowsDll "Windows AOEM core DLL"
Assert-Path $windowsHeader "Windows AOEM header"

$sdkManifest = Read-Json $sdkManifestPath
$runtimeManifest = Read-Json $runtimeManifestPath
$windowsManifest = Read-Json $windowsManifestPath

Assert-Checksums $aoemRootPath

$actualWindowsSha = (Get-FileHash -Algorithm SHA256 -Path $windowsDll).Hash.ToLowerInvariant()
$sdkWindows = $sdkManifest.platforms.'windows-x86_64'
if ($sdkWindows.status -ne "included") {
  throw "SDK manifest windows-x86_64 status is not included"
}
if ($sdkWindows.library -ne "windows/core/bin/aoem_ffi.dll") {
  throw "SDK manifest Windows library path drifted: $($sdkWindows.library)"
}
if ($sdkWindows.library_sha256.ToLowerInvariant() -ne $actualWindowsSha) {
  throw "SDK manifest Windows sha mismatch: expected=$($sdkWindows.library_sha256) actual=$actualWindowsSha"
}
if ($sdkManifest.confidential_transfer.profile_id -ne "confidential_transfer_v1") {
  throw "confidential_transfer_v1 is not the SDK manifest canonical privacy route"
}
Assert-False $sdkManifest.claims.public_ffi_abi_changed "public_ffi_abi_changed"
Assert-False $sdkManifest.claims.runtime_canon_changed "runtime_canon_changed"

$runtimeWindows = $runtimeManifest.entries |
  Where-Object { $_.platform -eq "windows-x86_64" -and $_.dll -eq "windows/core/bin/aoem_ffi.dll" } |
  Select-Object -First 1
if ($null -eq $runtimeWindows) {
  throw "runtime manifest missing windows core entry"
}
if ($runtimeWindows.sha256.ToLowerInvariant() -ne $actualWindowsSha) {
  throw "runtime manifest Windows sha mismatch: expected=$($runtimeWindows.sha256) actual=$actualWindowsSha"
}
Assert-False $runtimeManifest.public_ffi_abi_changed "runtime manifest public_ffi_abi_changed"
Assert-False $runtimeManifest.runtime_canon_changed "runtime manifest runtime_canon_changed"

if ($windowsManifest.profile -ne "fullmax") {
  throw "windows manifest profile is not fullmax"
}
if ($windowsManifest.platform -ne "windows") {
  throw "windows manifest platform is not windows"
}
$requiredPlugins = @(
  "aoem_ffi_persist.dll",
  "aoem_ffi_runtime_wasm_wasmtime.dll",
  "aoem_ffi_zkvm.dll",
  "aoem_ffi_mldsa.dll",
  "aoem_kms_plugin.dll",
  "aoem_hsm_plugin.dll"
)
foreach ($plugin in $requiredPlugins) {
  if ($windowsManifest.plugins -notcontains $plugin) {
    throw "windows manifest missing FULLMAX plugin: $plugin"
  }
}

Push-Location $repoRootPath
try {
  $verifyScript = Join-Path $repoRootPath "scripts\aoem\verify_aoem_binary.ps1"
  $verifyJson = powershell -NoProfile -ExecutionPolicy Bypass -File $verifyScript `
    -AoemRoot $aoemRootPath `
    -DllPath $windowsDll `
    -ExpectedSha256 $actualWindowsSha `
    -Variant core | ConvertFrom-Json
  if ($verifyJson.status -ne "ok" -or -not $verifyJson.execute_ops_v2) {
    throw "verify_aoem_binary did not accept the Windows core DLL"
  }

  $capabilities = Read-AoemCapabilities $windowsDll
  if ($capabilities.ffi_surface_policy -ne "production_feature_internal_v1") {
    throw "ffi_surface_policy drifted: $($capabilities.ffi_surface_policy)"
  }
  if (-not [bool]$capabilities.privacy_execute_v1) {
    throw "privacy_execute_v1 capability missing"
  }
  if (-not ([string]$capabilities.ffi_production_abi).Contains("privacy_execute_v1")) {
    throw "privacy_execute_v1 missing from production ABI capability string"
  }

  $confidentialTransferStatus = "skipped"
  if (-not $SkipConfidentialTransfer.IsPresent) {
    $clang = Get-CommandPath "clang"
    if (-not $clang) {
      throw "clang is required for confidential transfer host smoke"
    }
    $ctExe = Join-Path $env:TEMP "supervm_confidential_transfer.exe"
    $ctSource = Join-Path $aoemRootPath "host-integration\embedded_confidential_transfer_host.c"
    Invoke-Checked "compile confidential transfer host" {
      & $clang -std=c11 -Wall -Wextra -I $windowsIncludeDir $ctSource -o $ctExe
    }
    $ctOutput = & $ctExe $windowsDll --run-prove
    if ($LASTEXITCODE -ne 0) {
      throw "confidential transfer host smoke failed: exit=$LASTEXITCODE output=$ctOutput"
    }
    $ctLine = ($ctOutput | Where-Object { $_ -like "SUPERVM_AOEM_CONFIDENTIAL_TRANSFER_HOST|*" } | Select-Object -First 1)
    if (-not $ctLine -or $ctLine -notmatch "prove=ok" -or $ctLine -notmatch "privacy_execute=ok" -or $ctLine -notmatch "amount_hidden=ok" -or $ctLine -notmatch "failures=0") {
      throw "confidential transfer host smoke did not emit acceptance: $ctOutput"
    }
    $confidentialTransferStatus = "ok"
  }

  $proofEngineStatus = "skipped"
  $workerAdapterStatus = if ($SkipWorkerAdapter.IsPresent) { "skipped" } else { "ok" }
  if (-not $SkipProofEngine.IsPresent) {
    $proofScript = Join-Path $repoRootPath "scripts\aoem\run_proof_engine_host_smoke.ps1"
    $proofArgs = @(
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      $proofScript,
      "-RepoRoot",
      $repoRootPath,
      "-LibraryPath",
      $windowsDll
    )
    if ($SkipWorkerAdapter.IsPresent) {
      $proofArgs += "-SkipWorkerAdapter"
    }
    $proofOutput = & powershell @proofArgs
    if ($LASTEXITCODE -ne 0) {
      throw "proof engine host smoke failed: exit=$LASTEXITCODE output=$proofOutput"
    }
    $proofLine = ($proofOutput | Where-Object { $_ -like "SUPERVM_AOEM_PROOF_ENGINE_HOST_SMOKE|*" } | Select-Object -First 1)
    if (-not $proofLine -or $proofLine -notmatch "proof=ok" -or $proofLine -notmatch "verify=ok" -or $proofLine -notmatch "failures=0") {
      throw "proof engine host smoke did not emit acceptance: $proofOutput"
    }
    if (-not $SkipWorkerAdapter.IsPresent) {
      $workerLine = ($proofOutput | Where-Object { $_ -like "AOEM_PROOF_WORKER_SUMMARY|*" } | Select-Object -First 1)
      if (-not $workerLine -or $workerLine -notmatch "proof=ok" -or $workerLine -notmatch "verify=ok" -or $workerLine -notmatch "failures=0") {
        throw "worker adapter smoke did not emit acceptance: $proofOutput"
      }
    }
    $proofEngineStatus = "ok"
  }

  Write-Output (
    "SUPERVM_AOEM_FULLMAX_HOST_ACCEPTANCE|" +
    "manifest_json=ok|" +
    "checksums=ok|" +
    "windows_core_dll=ok|" +
    "ffi_probe=ok|" +
    "ffi_surface_policy=ok|" +
    "confidential_transfer=$confidentialTransferStatus|" +
    "proof_engine=$proofEngineStatus|" +
    "worker_adapter=$workerAdapterStatus|" +
    "abi_behavior_changed=false|" +
    "runtime_canon_changed=false|" +
    "privacy_route_changed=false|" +
    "failures=0"
  )
}
finally {
  Pop-Location
}
