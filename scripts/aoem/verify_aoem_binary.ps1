param(
  [string]$AoemRoot = "$(Join-Path $PSScriptRoot '..\..\aoem')",
  [string]$ManifestPath = "$(Join-Path $PSScriptRoot '..\..\aoem\manifest\aoem-manifest.json')",
  [string]$Variant = 'core',
  [string]$DllPath,
  [string]$ExpectedSha256,
  [int]$ExpectedAbi = 1
)

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class WinNative {
  [DllImport("kernel32", SetLastError=true, CharSet=CharSet.Ansi)]
  public static extern IntPtr LoadLibrary(string lpFileName);
  [DllImport("kernel32", SetLastError=true, CharSet=CharSet.Ansi)]
  public static extern IntPtr GetProcAddress(IntPtr hModule, string procName);
  [DllImport("kernel32", SetLastError=true)]
  public static extern bool FreeLibrary(IntPtr hModule);
}
[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
public delegate UInt32 AbiFn();
[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
public delegate IntPtr CapFn();
[UnmanagedFunctionPointer(CallingConvention.Cdecl)]
public delegate IntPtr VerFn();
"@

function Get-ManifestEntry {
  param([string]$Path, [string]$Name)
  if (-not (Test-Path $Path)) { return $null }
  $m = Get-Content -Raw -Path $Path | ConvertFrom-Json
  if (-not $m.entries) { return $null }
  return $m.entries | Where-Object { $_.name -eq $Name } | Select-Object -First 1
}

$entry = $null
if (-not $DllPath -or -not $ExpectedSha256) {
  $entry = Get-ManifestEntry -Path $ManifestPath -Name $Variant
}

if (-not $DllPath) {
  if ($entry) {
    $DllPath = Join-Path (Resolve-Path $AoemRoot).Path ($entry.dll -replace '/', [IO.Path]::DirectorySeparatorChar)
  } else {
    $root = (Resolve-Path $AoemRoot).Path
    switch ($Variant) {
      'core' { $DllPath = Join-Path $root 'bin/aoem_ffi.dll' }
      'persist' { $DllPath = Join-Path $root 'variants/persist/bin/aoem_ffi.dll' }
      'wasm' { $DllPath = Join-Path $root 'variants/wasm/bin/aoem_ffi.dll' }
      default { throw "dll path not provided and unknown variant: $Variant" }
    }
  }
}
if (-not $ExpectedSha256 -and $entry) {
  $ExpectedSha256 = $entry.sha256
}
if ($entry -and $entry.abi_expected) {
  $ExpectedAbi = [int]$entry.abi_expected
}

if (-not (Test-Path $DllPath)) { throw "dll not found: $DllPath" }

$actualHash = (Get-FileHash -Algorithm SHA256 -Path $DllPath).Hash.ToLowerInvariant()
if ($ExpectedSha256) {
  if ($actualHash -ne $ExpectedSha256.ToLowerInvariant()) {
    throw "sha256 mismatch: expected=$ExpectedSha256 actual=$actualHash"
  }
}

$mod = [WinNative]::LoadLibrary($DllPath)
if ($mod -eq [IntPtr]::Zero) {
  throw "LoadLibrary failed: $DllPath"
}
try {
  $abiPtr = [WinNative]::GetProcAddress($mod, 'aoem_abi_version')
  $capPtr = [WinNative]::GetProcAddress($mod, 'aoem_capabilities_json')
  $verPtr = [WinNative]::GetProcAddress($mod, 'aoem_version_string')
  if ($abiPtr -eq [IntPtr]::Zero -or $capPtr -eq [IntPtr]::Zero -or $verPtr -eq [IntPtr]::Zero) {
    throw 'required exports missing'
  }

  $abiFn = [Runtime.InteropServices.Marshal]::GetDelegateForFunctionPointer($abiPtr, [AbiFn])
  $capFn = [Runtime.InteropServices.Marshal]::GetDelegateForFunctionPointer($capPtr, [CapFn])
  $verFn = [Runtime.InteropServices.Marshal]::GetDelegateForFunctionPointer($verPtr, [VerFn])

  $abi = $abiFn.Invoke()
  if ($abi -ne $ExpectedAbi) {
    throw "abi mismatch: expected=$ExpectedAbi actual=$abi"
  }

  $verC = $verFn.Invoke()
  $capC = $capFn.Invoke()
  if ($verC -eq [IntPtr]::Zero -or $capC -eq [IntPtr]::Zero) {
    throw 'version/capabilities returned null'
  }

  $version = [Runtime.InteropServices.Marshal]::PtrToStringAnsi($verC)
  $capJson = [Runtime.InteropServices.Marshal]::PtrToStringAnsi($capC)
  $cap = $capJson | ConvertFrom-Json

  if (-not $cap.execute_ops_v2) {
    throw 'capability check failed: execute_ops_v2=false'
  }

  $result = [pscustomobject]@{
    status = 'ok'
    variant = $Variant
    dll = $DllPath
    sha256 = $actualHash
    abi = $abi
    version = $version
    execute_ops_v2 = [bool]$cap.execute_ops_v2
    bundle_profile = [string]$cap.bundle_profile
    rocksdb_persistence = [bool]$cap.rocksdb_persistence
    wasmtime_runtime = [bool]$cap.wasmtime_runtime
  }
  $result | ConvertTo-Json -Depth 5
}
finally {
  [void][WinNative]::FreeLibrary($mod)
}
