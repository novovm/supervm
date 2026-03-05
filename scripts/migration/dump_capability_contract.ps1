param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateSet("core", "persist", "wasm")]
    [string]$Variant = "core"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\capabilities"
}

function Invoke-CargoStdout {
    param(
        [string]$WorkDir,
        [string[]]$CargoArgs,
        [hashtable]$EnvVars
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
    return $stdout.Trim()
}

function Get-DynlibNameCandidates {
    if ($IsWindows) {
        return @("aoem_ffi.dll")
    }
    if ($IsMacOS) {
        return @("libaoem_ffi.dylib")
    }
    return @("libaoem_ffi.so")
}

function Get-AoemVariantBinDir {
    param([string]$AoemRoot, [string]$Variant)
    switch ($Variant) {
        "core" { return Join-Path $AoemRoot "bin" }
        "persist" { return Join-Path $AoemRoot "variants\persist\bin" }
        "wasm" { return Join-Path $AoemRoot "variants\wasm\bin" }
        default { throw "invalid variant: $Variant" }
    }
}

function Get-DllPathForVariant {
    param(
        [string]$AoemRoot,
        [string]$Variant,
        [bool]$RequireExists = $false
    )

    $binDir = Get-AoemVariantBinDir -AoemRoot $AoemRoot -Variant $Variant
    $candidates = Get-DynlibNameCandidates
    foreach ($name in $candidates) {
        $candidate = Join-Path $binDir $name
        if (Test-Path $candidate) {
            return (Resolve-Path $candidate).Path
        }
    }

    $fallback = Join-Path $binDir $candidates[0]
    if ($RequireExists) {
        throw "aoem dynlib not found for variant=$Variant under $binDir (tried: $($candidates -join ', '))"
    }
    return $fallback
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$execDir = Join-Path $RepoRoot "crates\novovm-exec"
$aoemRoot = Join-Path $RepoRoot "aoem"
$aoemDll = Get-DllPathForVariant -AoemRoot $aoemRoot -Variant $Variant -RequireExists $true
$aoemManifest = Join-Path $aoemRoot "manifest\aoem-manifest.json"
$aoemRuntimeProfile = Join-Path $aoemRoot "config\aoem-runtime-profile.json"
$jsonText = Invoke-CargoStdout -WorkDir $execDir -CargoArgs @(
    "run", "--quiet", "--example", "capability_contract_dump"
) -EnvVars @{
    NOVOVM_AOEM_VARIANT = $Variant
    NOVOVM_AOEM_ROOT = $aoemRoot
    NOVOVM_AOEM_DLL = $aoemDll
    NOVOVM_AOEM_MANIFEST = $aoemManifest
    NOVOVM_AOEM_RUNTIME_PROFILE = $aoemRuntimeProfile
}

$contract = $jsonText | ConvertFrom-Json
$proverContract = $null
$proverError = ""
try {
    $proverDir = Join-Path $RepoRoot "crates\novovm-prover"
    if (Test-Path (Join-Path $proverDir "Cargo.toml")) {
        $proverText = Invoke-CargoStdout -WorkDir $proverDir -CargoArgs @(
            "run", "--quiet", "--example", "capability_bridge_dump"
        ) -EnvVars @{
            NOVOVM_AOEM_VARIANT = $Variant
            NOVOVM_AOEM_ROOT = $aoemRoot
            NOVOVM_AOEM_DLL = $aoemDll
            NOVOVM_AOEM_MANIFEST = $aoemManifest
            NOVOVM_AOEM_RUNTIME_PROFILE = $aoemRuntimeProfile
        }
        $proverContract = $proverText | ConvertFrom-Json
    }
} catch {
    $proverError = $_.Exception.Message
}
$generatedAt = [DateTime]::UtcNow.ToString("o")

$result = [ordered]@{
    generated_at_utc = $generatedAt
    variant = $Variant
    contract = $contract
    prover_contract = $proverContract
    prover_contract_error = $proverError
}

$jsonPath = Join-Path $OutputDir "capability-contract-$Variant.json"
$mdPath = Join-Path $OutputDir "capability-contract-$Variant.md"

$result | ConvertTo-Json -Depth 8 | Set-Content -Path $jsonPath -Encoding UTF8

$md = @(
    "# AOEM Capability Contract Snapshot"
    ""
    "- generated_at_utc: $generatedAt"
    "- variant: $Variant"
    "- execute_ops_v2: $($contract.execute_ops_v2)"
    "- zkvm_prove: $($contract.zkvm_prove)"
    "- zkvm_verify: $($contract.zkvm_verify)"
    "- msm_accel: $($contract.msm_accel)"
    "- msm_backend: $($contract.msm_backend)"
    "- fallback_reason: $($contract.fallback_reason)"
    "- fallback_reason_codes: $((@($contract.fallback_reason_codes) -join ', '))"
    "- zk_formal_fields_present: $($contract.zk_formal_fields_present)"
    "- inferred_from_legacy_fields: $($contract.inferred_from_legacy_fields)"
    "- prover_contract_ready: $($null -ne $proverContract)"
    "- prover_contract_error: $proverError"
    ""
    "## Raw Capabilities"
    ""
    '```json'
    ($contract.raw | ConvertTo-Json -Depth 8)
    '```'
)

$md -join "`n" | Set-Content -Path $mdPath -Encoding UTF8

Write-Host "capability contract snapshot generated:"
Write-Host "  $jsonPath"
Write-Host "  $mdPath"
