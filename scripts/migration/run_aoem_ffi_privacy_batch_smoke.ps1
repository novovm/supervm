param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateSet("core", "persist", "wasm")]
    [string]$AoemVariant = "persist",
    [ValidateSet("release", "debug")]
    [string]$BuildProfile = "release"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if (-not (Get-Variable -Name IsWindows -ErrorAction SilentlyContinue)) {
    $IsWindows = ($env:OS -eq "Windows_NT")
}

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\aoem-ffi-privacy-batch-smoke"
}
if (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Invoke-CargoStdout {
    param(
        [string]$WorkDir,
        [string[]]$CargoArgs,
        [hashtable]$EnvVars = @{}
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($CargoArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")
    foreach ($k in $EnvVars.Keys) {
        $psi.Environment[$k] = [string]$EnvVars[$k]
    }

    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()
    if ($proc.ExitCode -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed in $WorkDir`n$stdout`n$stderr"
    }
    return [ordered]@{
        stdout = $stdout.Trim()
        stderr = $stderr.Trim()
    }
}

function Get-DllName {
    if ($IsWindows) { return "aoem_ffi.dll" }
    if ($IsMacOS) { return "libaoem_ffi.dylib" }
    return "libaoem_ffi.so"
}

function Resolve-AoemDllPath {
    param([string]$RepoRootValue)
    $fromEnv = $env:NOVOVM_AOEM_DLL
    if ($fromEnv -and (Test-Path $fromEnv)) {
        return (Resolve-Path $fromEnv).Path
    }
    $name = Get-DllName
    $platform = if ($IsWindows) { "windows" } elseif ($IsMacOS) { "macos" } else { "linux" }
    $candidates = @(
        (Join-Path $RepoRootValue "aoem\$platform\core\bin\$name"),
        (Join-Path $RepoRootValue "aoem\$platform\bin\$name"),
        (Join-Path $RepoRootValue "aoem\bin\$name"),
        (Join-Path $RepoRootValue "aoem\plugins\$name"),
        (Join-Path $RepoRootValue "bin\$name")
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) { return (Resolve-Path $c).Path }
    }
    throw "aoem core dynlib not found (tried: $($candidates -join ', ')); set NOVOVM_AOEM_DLL explicitly"
}

function Get-BoolField {
    param(
        [object]$Obj,
        [string]$Name,
        [bool]$Default = $false
    )
    if ($null -eq $Obj) { return $Default }
    $prop = $Obj.PSObject.Properties[$Name]
    if ($null -eq $prop) { return $Default }
    return [bool]$prop.Value
}

$dllPath = Resolve-AoemDllPath -RepoRootValue $RepoRoot
$aoemRoot = Join-Path $RepoRoot "aoem"
$manifestPath = Join-Path $aoemRoot "manifest\aoem-manifest.json"
$runtimeProfilePath = Join-Path $aoemRoot "config\aoem-runtime-profile.json"
$manifestSkipPath = Join-Path $OutputDir "__manifest_skip__.json"

$bindingsDir = Join-Path $RepoRoot "crates\aoem-bindings"
$probeProfilePath = Join-Path $OutputDir "ffi-install-probe-profile.json"
$groth16BatchSmokePath = Join-Path $OutputDir "groth16-prove-batch-smoke.json"

$probe = Invoke-CargoStdout -WorkDir $bindingsDir -CargoArgs @(
    "run", "--quiet", "--example", "ffi_install_probe", "--",
    "--dll", $dllPath,
    "--out", $probeProfilePath
) -EnvVars @{
    AOEM_DLL_MANIFEST = $manifestSkipPath
    AOEM_DLL_MANIFEST_REQUIRED = "0"
}

$groth16BatchSmoke = Invoke-CargoStdout -WorkDir $bindingsDir -CargoArgs @(
    "run", "--quiet", "--example", "groth16_prove_batch_smoke", "--",
    "--dll", $dllPath,
    "--out", $groth16BatchSmokePath
) -EnvVars @{
    AOEM_DLL_MANIFEST = $manifestSkipPath
    AOEM_DLL_MANIFEST_REQUIRED = "0"
}

if (-not (Test-Path $probeProfilePath)) {
    throw "ffi_install_probe profile not generated: $probeProfilePath"
}
$groth16BatchSmoke.stderr | Set-Content -Path (Join-Path $OutputDir "groth16-prove-batch-smoke.stderr.log") -Encoding UTF8
$groth16BatchSmoke.stdout | Set-Content -Path (Join-Path $OutputDir "groth16-prove-batch-smoke.stdout.log") -Encoding UTF8
$groth16BatchSummary = if (Test-Path $groth16BatchSmokePath) {
    Get-Content -Path $groth16BatchSmokePath -Raw | ConvertFrom-Json
} else {
    $null
}
$probeProfile = Get-Content -Path $probeProfilePath -Raw | ConvertFrom-Json
$sym = $probeProfile.ffi_symbol_contract

$execDir = Join-Path $RepoRoot "crates\novovm-exec"
$contractOutPath = Join-Path $OutputDir "capability-contract.json"
$contractErrPath = Join-Path $OutputDir "capability-contract.stderr.log"
$contract = Invoke-CargoStdout -WorkDir $execDir -CargoArgs @(
    "run", "--quiet", "--example", "capability_contract_dump"
) -EnvVars @{
    NOVOVM_AOEM_VARIANT = $AoemVariant
    NOVOVM_AOEM_ROOT = $aoemRoot
    NOVOVM_AOEM_DLL = $dllPath
    NOVOVM_AOEM_MANIFEST = $manifestSkipPath
    NOVOVM_AOEM_RUNTIME_PROFILE = $runtimeProfilePath
    AOEM_DLL_MANIFEST = $manifestSkipPath
    AOEM_DLL_MANIFEST_REQUIRED = "0"
}
$contract.stdout | Set-Content -Path $contractOutPath -Encoding UTF8
$contract.stderr | Set-Content -Path $contractErrPath -Encoding UTF8
$contractJson = $contract.stdout | ConvertFrom-Json
$raw = $contractJson.raw

$symbolRingBatch = Get-BoolField -Obj $sym -Name "ring_signature_verify_batch_web30_v1"
$symbolGroth16Prove = Get-BoolField -Obj $sym -Name "groth16_prove_v1"
$symbolGroth16ProveBatch = Get-BoolField -Obj $sym -Name "groth16_prove_batch_v1"
$symbolGroth16ProveAuto = Get-BoolField -Obj $sym -Name "groth16_prove_auto_path"
$symbolBulletBatch = Get-BoolField -Obj $sym -Name "bulletproof_batch_v1"
$symbolRingctBatch = Get-BoolField -Obj $sym -Name "ringct_batch_v1"
$symbolAll = Get-BoolField -Obj $sym -Name "privacy_batch_v1_all"

$capRingBatch = Get-BoolField -Obj $raw -Name "ring_signature_batch_verify"
$capBulletBatch = Get-BoolField -Obj $raw -Name "bulletproof_batch_verify"
$capRingctBatch = Get-BoolField -Obj $raw -Name "ringct_batch_verify"

$groth16BatchPass = $false
if ($null -ne $groth16BatchSummary) {
    $groth16BatchPass = [bool]$groth16BatchSummary.pass
}

$pass = ($symbolRingBatch -and $symbolBulletBatch -and $symbolRingctBatch -and $symbolAll -and $capRingBatch -and $capBulletBatch -and $capRingctBatch -and $groth16BatchPass)

$summary = [ordered]@{
    schema = "aoem_ffi_privacy_batch_smoke_v1"
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    aoem_variant = $AoemVariant
    aoem_dll = $dllPath
    probe_profile_json = $probeProfilePath
    groth16_prove_batch_smoke_json = $groth16BatchSmokePath
    capability_contract_json = $contractOutPath
    manifest_check_mode = "skip_for_symbol_probe"
    ffi_symbol_contract = [ordered]@{
        ring_signature_verify_batch_web30_v1 = $symbolRingBatch
        groth16_prove_v1 = $symbolGroth16Prove
        groth16_prove_batch_v1 = $symbolGroth16ProveBatch
        groth16_prove_auto_path = $symbolGroth16ProveAuto
        bulletproof_batch_v1 = $symbolBulletBatch
        ringct_batch_v1 = $symbolRingctBatch
        privacy_batch_v1_all = $symbolAll
    }
    capability_flags = [ordered]@{
        ring_signature_batch_verify = $capRingBatch
        bulletproof_batch_verify = $capBulletBatch
        ringct_batch_verify = $capRingctBatch
    }
    groth16_batch_smoke = [ordered]@{
        pass = $groth16BatchPass
        witness_count = if ($null -ne $groth16BatchSummary) { $groth16BatchSummary.witness_count } else { 0 }
        elapsed_us = if ($null -ne $groth16BatchSummary) { $groth16BatchSummary.elapsed_us } else { 0 }
    }
}

$summaryJsonPath = Join-Path $OutputDir "aoem-ffi-privacy-batch-smoke-summary.json"
$summaryMdPath = Join-Path $OutputDir "aoem-ffi-privacy-batch-smoke-summary.md"
($summary | ConvertTo-Json -Depth 8) | Set-Content -Path $summaryJsonPath -Encoding UTF8

$md = @(
    "# AOEM FFI Privacy Batch Smoke",
    "",
    "- pass: $pass",
    "- aoem_variant: $AoemVariant",
    "- aoem_dll: $dllPath",
    "- probe_profile_json: $probeProfilePath",
    "- groth16_prove_batch_smoke_json: $groth16BatchSmokePath",
    "- capability_contract_json: $contractOutPath",
    "",
    "## FFI Symbol Contract",
    "",
    "- ring_signature_verify_batch_web30_v1: $symbolRingBatch",
    "- groth16_prove_v1: $symbolGroth16Prove",
    "- groth16_prove_batch_v1: $symbolGroth16ProveBatch",
    "- groth16_prove_auto_path: $symbolGroth16ProveAuto",
    "- bulletproof_batch_v1: $symbolBulletBatch",
    "- ringct_batch_v1: $symbolRingctBatch",
    "- privacy_batch_v1_all: $symbolAll",
    "",
    "## Capability Flags",
    "",
    "- ring_signature_batch_verify: $capRingBatch",
    "- bulletproof_batch_verify: $capBulletBatch",
    "- ringct_batch_verify: $capRingctBatch"
    "",
    "## Groth16 Prove Batch Smoke",
    "",
    "- pass: $groth16BatchPass",
    "- witness_count: $($summary.groth16_batch_smoke.witness_count)",
    "- elapsed_us: $($summary.groth16_batch_smoke.elapsed_us)"
)
$md | Set-Content -Path $summaryMdPath -Encoding UTF8

Write-Host "aoem ffi privacy batch smoke generated:"
Write-Host "  summary_json: $summaryJsonPath"
Write-Host "  summary_md:   $summaryMdPath"
Write-Host "  overall_pass: $pass"

if (-not $pass) {
    throw "aoem ffi privacy batch smoke failed"
}
