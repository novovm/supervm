param(
    [string]$RepoRoot = "D:\WorksArea\SUPERVM",
    [string]$OutputDir = "D:\WorksArea\SUPERVM\artifacts\migration\capabilities",
    [ValidateSet("core", "persist", "wasm")]
    [string]$Variant = "core"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

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

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$execDir = Join-Path $RepoRoot "crates\novovm-exec"
$jsonText = Invoke-CargoStdout -WorkDir $execDir -CargoArgs @(
    "run", "--quiet", "--example", "capability_contract_dump"
) -EnvVars @{
    NOVOVM_AOEM_VARIANT = $Variant
}

$contract = $jsonText | ConvertFrom-Json
$generatedAt = [DateTime]::UtcNow.ToString("o")

$result = [ordered]@{
    generated_at_utc = $generatedAt
    variant = $Variant
    contract = $contract
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
    "- inferred_from_legacy_fields: $($contract.inferred_from_legacy_fields)"
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
