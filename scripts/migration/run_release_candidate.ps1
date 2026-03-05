param(
    [string]$RepoRoot = "",
    [string]$RcRef = "",
    [string]$OutputDir = "",
    [double]$AllowedRegressionPct = -5.0,
    [ValidateRange(1, 9)]
    [int]$PerformanceRuns = 3,
    [ValidateRange(2, 20)]
    [int]$AdapterStabilityRuns = 3,
    [switch]$FullSnapshotProfileV2
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

function Invoke-Process {
    param(
        [string]$FileName,
        [string[]]$Arguments,
        [string]$WorkingDirectory
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $FileName
    $psi.WorkingDirectory = $WorkingDirectory
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($Arguments | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")

    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()

    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        stdout = $stdout.Trim()
        stderr = $stderr.Trim()
        output = ($stdout + $stderr).Trim()
    }
}

function Normalize-RcRef {
    param([string]$Value)
    $trimmed = $Value.Trim()
    if (-not $trimmed) {
        throw "rc_ref cannot be empty"
    }
    return ($trimmed -replace '[\\/:*?"<>|\s]+', '-')
}

$gitHead = Invoke-Process -FileName "git" -Arguments @("rev-parse", "HEAD") -WorkingDirectory $RepoRoot
if ($gitHead.exit_code -ne 0 -or -not $gitHead.stdout) {
    throw "failed to read git HEAD commit hash: $($gitHead.output)"
}
$commitHash = $gitHead.stdout

if (-not $RcRef) {
    $gitShort = Invoke-Process -FileName "git" -Arguments @("rev-parse", "--short=12", "HEAD") -WorkingDirectory $RepoRoot
    if ($gitShort.exit_code -ne 0 -or -not $gitShort.stdout) {
        throw "failed to read git short hash: $($gitShort.output)"
    }
    $RcRef = "rc-$($gitShort.stdout)"
}
$rcRefNormalized = Normalize-RcRef -Value $RcRef

if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\release-candidate-$rcRefNormalized"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$snapshotScript = Join-Path $RepoRoot "scripts\migration\run_release_snapshot.ps1"
if (-not (Test-Path $snapshotScript)) {
    throw "missing release snapshot script: $snapshotScript"
}

$expectedProfile = if ($FullSnapshotProfileV2) { "full_snapshot_v2" } else { "full_snapshot_v1" }
$snapshotOutputDir = Join-Path $OutputDir "snapshot"
if ($FullSnapshotProfileV2) {
    & $snapshotScript `
        -RepoRoot $RepoRoot `
        -OutputDir $snapshotOutputDir `
        -AllowedRegressionPct $AllowedRegressionPct `
        -PerformanceRuns $PerformanceRuns `
        -AdapterStabilityRuns $AdapterStabilityRuns `
        -FullSnapshotProfileV2 | Out-Null
} else {
    & $snapshotScript `
        -RepoRoot $RepoRoot `
        -OutputDir $snapshotOutputDir `
        -AllowedRegressionPct $AllowedRegressionPct `
        -PerformanceRuns $PerformanceRuns `
        -AdapterStabilityRuns $AdapterStabilityRuns | Out-Null
}

$snapshotJson = Join-Path $snapshotOutputDir "release-snapshot.json"
$snapshotMd = Join-Path $snapshotOutputDir "release-snapshot.md"
if (-not (Test-Path $snapshotJson)) {
    throw "missing release snapshot json: $snapshotJson"
}
$snapshot = Get-Content -Path $snapshotJson -Raw | ConvertFrom-Json

$acceptanceJson = Join-Path $snapshotOutputDir "acceptance-gate-full\acceptance-gate-summary.json"
if (-not (Test-Path $acceptanceJson)) {
    throw "missing acceptance summary json: $acceptanceJson"
}
$acceptance = Get-Content -Path $acceptanceJson -Raw | ConvertFrom-Json

if ([string]$snapshot.profile_name -ne $expectedProfile) {
    throw "unexpected snapshot profile: $($snapshot.profile_name); expected $expectedProfile"
}
if (-not [bool]$snapshot.overall_pass) {
    throw "release candidate snapshot is not green: overall_pass=false"
}

$freezeRule = if ($FullSnapshotProfileV2) {
    "full_snapshot_v1 semantic is frozen; full_snapshot_v2 includes rpc exposure gate as additive security profile"
} else {
    "full_snapshot_v1 semantic is frozen; additive capability changes must use full_snapshot_v2"
}

$rcCandidate = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    rc_ref = $RcRef
    rc_ref_normalized = $rcRefNormalized
    commit_hash = $commitHash
    status = "ReadyForMerge/SnapshotGreen"
    snapshot_profile = [string]$snapshot.profile_name
    snapshot_overall_pass = [bool]$snapshot.overall_pass
    governance_param3_pass = [bool]$acceptance.governance_param3_pass
    rpc_exposure_pass = if ($expectedProfile -eq "full_snapshot_v2") { [bool]$acceptance.rpc_exposure_pass } else { $false }
    adapter_stability_pass = [bool]$acceptance.adapter_stability_pass
    snapshot_json = $snapshotJson
    snapshot_md = $snapshotMd
    acceptance_summary_json = $acceptanceJson
    freeze_rule = $freezeRule
}

$rcJson = Join-Path $OutputDir "rc-candidate.json"
$rcMd = Join-Path $OutputDir "rc-candidate.md"
$rcCandidate | ConvertTo-Json -Depth 8 | Set-Content -Path $rcJson -Encoding UTF8

$md = @(
    "# NOVOVM Release Candidate"
    ""
    "- generated_at_utc: $($rcCandidate.generated_at_utc)"
    "- rc_ref: $($rcCandidate.rc_ref)"
    "- commit_hash: $($rcCandidate.commit_hash)"
    "- status: $($rcCandidate.status)"
    "- snapshot_profile: $($rcCandidate.snapshot_profile)"
    "- snapshot_overall_pass: $($rcCandidate.snapshot_overall_pass)"
    "- governance_param3_pass: $($rcCandidate.governance_param3_pass)"
    "- rpc_exposure_pass: $($rcCandidate.rpc_exposure_pass)"
    "- adapter_stability_pass: $($rcCandidate.adapter_stability_pass)"
    "- snapshot_json: $($rcCandidate.snapshot_json)"
    "- acceptance_summary_json: $($rcCandidate.acceptance_summary_json)"
    "- freeze_rule: $($rcCandidate.freeze_rule)"
    "- rc_json: $rcJson"
)
$md -join "`n" | Set-Content -Path $rcMd -Encoding UTF8

Write-Host "release candidate generated:"
Write-Host "  rc_ref: $($rcCandidate.rc_ref)"
Write-Host "  commit_hash: $($rcCandidate.commit_hash)"
Write-Host "  status: $($rcCandidate.status)"
Write-Host "  snapshot_profile: $($rcCandidate.snapshot_profile)"
Write-Host "  snapshot_overall_pass: $($rcCandidate.snapshot_overall_pass)"
Write-Host "  governance_param3_pass: $($rcCandidate.governance_param3_pass)"
Write-Host "  rpc_exposure_pass: $($rcCandidate.rpc_exposure_pass)"
Write-Host "  adapter_stability_pass: $($rcCandidate.adapter_stability_pass)"
Write-Host "  rc_json: $rcJson"
Write-Host "  rc_md: $rcMd"

Write-Host "release candidate PASS"
