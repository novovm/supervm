param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(4, 1000)]
    [int]$Nodes = 4,
    [ValidateRange(0, 999)]
    [int]$FailedLeader = 0,
    [ValidateRange(1, 30)]
    [int]$TimeoutSeconds = 10
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\pacemaker-failover-gate"
}
if ($FailedLeader -ge $Nodes) {
    throw "FailedLeader ($FailedLeader) must be less than Nodes ($Nodes)"
}

function Invoke-Cargo {
    param(
        [string]$WorkDir,
        [string[]]$CargoArgs
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($CargoArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")

    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()

    $text = ($stdout + $stderr).Trim()
    if ($proc.ExitCode -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed in $WorkDir`n$text"
    }
    return $text
}

function Parse-PacemakerFailoverLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^pacemaker_failover_out:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }

    $m = [regex]::Match(
        $line,
        "^pacemaker_failover_out:\s+mode=(?<mode>\S+)\s+transport=(?<transport>\S+)\s+nodes=(?<nodes>\d+)\s+failed_leader=(?<failed_leader>\d+)\s+initial_view=(?<initial_view>\d+)\s+next_view=(?<next_view>\d+)\s+next_leader=(?<next_leader>\d+)\s+timeout_votes=(?<timeout_votes>\d+)\s+timeout_quorum=(?<timeout_quorum>\d+)\s+timeout_cert=(?<timeout_cert>true|false)\s+local_view_advanced=(?<local_view_advanced>\d+)\s+view_sync_votes=(?<view_sync_votes>\d+)\s+new_view_votes=(?<new_view_votes>\d+)\s+qc_formed=(?<qc_formed>true|false)\s+committed=(?<committed>true|false)\s+committed_height=(?<committed_height>\d+)\s+pass=(?<pass>true|false)\s+reason=(?<reason>\S+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }

    return [ordered]@{
        parse_ok = $true
        mode = $m.Groups["mode"].Value
        transport = $m.Groups["transport"].Value
        nodes = [int64]$m.Groups["nodes"].Value
        failed_leader = [int64]$m.Groups["failed_leader"].Value
        initial_view = [int64]$m.Groups["initial_view"].Value
        next_view = [int64]$m.Groups["next_view"].Value
        next_leader = [int64]$m.Groups["next_leader"].Value
        timeout_votes = [int64]$m.Groups["timeout_votes"].Value
        timeout_quorum = [int64]$m.Groups["timeout_quorum"].Value
        timeout_cert = [bool]::Parse($m.Groups["timeout_cert"].Value)
        local_view_advanced = [int64]$m.Groups["local_view_advanced"].Value
        view_sync_votes = [int64]$m.Groups["view_sync_votes"].Value
        new_view_votes = [int64]$m.Groups["new_view_votes"].Value
        qc_formed = [bool]::Parse($m.Groups["qc_formed"].Value)
        committed = [bool]::Parse($m.Groups["committed"].Value)
        committed_height = [int64]$m.Groups["committed_height"].Value
        pass = [bool]::Parse($m.Groups["pass"].Value)
        reason = $m.Groups["reason"].Value
        raw = $line
    }
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$nodeCrateDir = Join-Path $RepoRoot "crates\novovm-node"
if (-not (Test-Path (Join-Path $nodeCrateDir "Cargo.toml"))) {
    throw "missing novovm-node Cargo.toml: $nodeCrateDir"
}
Invoke-Cargo -WorkDir $nodeCrateDir -CargoArgs @("build", "--quiet", "--bin", "novovm-node") | Out-Null

$nodeExeCandidates = @(
    (Join-Path $RepoRoot "target\debug\novovm-node.exe"),
    (Join-Path $nodeCrateDir "target\debug\novovm-node.exe")
)
$nodeExe = ""
foreach ($candidate in $nodeExeCandidates) {
    if (Test-Path $candidate) {
        $nodeExe = (Resolve-Path $candidate).Path
        break
    }
}
if (-not $nodeExe) {
    throw "missing novovm-node binary after build; checked: $($nodeExeCandidates -join ', ')"
}

$stdoutPath = Join-Path $OutputDir "pacemaker-failover.stdout.log"
$stderrPath = Join-Path $OutputDir "pacemaker-failover.stderr.log"

$psi = [System.Diagnostics.ProcessStartInfo]::new()
$psi.FileName = $nodeExe
$psi.WorkingDirectory = $RepoRoot
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.CreateNoWindow = $true
$psi.Environment["NOVOVM_NODE_MODE"] = "pacemaker_failover_probe"
$psi.Environment["NOVOVM_PACEMAKER_NODES"] = "$Nodes"
$psi.Environment["NOVOVM_PACEMAKER_FAILED_LEADER"] = "$FailedLeader"

$proc = [System.Diagnostics.Process]::Start($psi)
if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
    try { $proc.Kill() } catch {}
    throw "pacemaker_failover_probe timed out after ${TimeoutSeconds}s"
}

$stdout = $proc.StandardOutput.ReadToEnd()
$stderr = $proc.StandardError.ReadToEnd()
$stdout | Set-Content -Path $stdoutPath -Encoding UTF8
$stderr | Set-Content -Path $stderrPath -Encoding UTF8
$parsed = Parse-PacemakerFailoverLine -Text ($stdout + $stderr)

$pass = $false
$errorReason = ""
if ($proc.ExitCode -ne 0) {
    $errorReason = "pacemaker_failover_probe exited with code $($proc.ExitCode)"
} elseif (-not $parsed -or -not $parsed.parse_ok) {
    $errorReason = "failed to parse pacemaker_failover_out line"
} else {
    $pass = (
        $parsed.pass -and
        $parsed.nodes -eq $Nodes -and
        $parsed.failed_leader -eq $FailedLeader -and
        $parsed.timeout_cert -and
        $parsed.local_view_advanced -ge $parsed.timeout_quorum -and
        $parsed.view_sync_votes -ge $parsed.timeout_quorum -and
        $parsed.new_view_votes -ge $parsed.timeout_quorum -and
        $parsed.qc_formed -and
        $parsed.committed -and
        $parsed.committed_height -ge 1 -and
        $parsed.next_leader -ne $FailedLeader
    )
    if (-not $pass) {
        $errorReason = "pacemaker failover assertion failed (parsed_pass=$($parsed.pass), timeout_cert=$($parsed.timeout_cert), view_sync_votes=$($parsed.view_sync_votes), new_view_votes=$($parsed.new_view_votes), qc_formed=$($parsed.qc_formed), committed=$($parsed.committed), committed_height=$($parsed.committed_height))"
    }
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    nodes = $Nodes
    failed_leader = $FailedLeader
    node_exe = $nodeExe
    exit_code = [int]$proc.ExitCode
    error_reason = $errorReason
    pacemaker_failover_signal = $parsed
    stdout = $stdoutPath
    stderr = $stderrPath
}

$summaryJson = Join-Path $OutputDir "pacemaker-failover-gate-summary.json"
$summaryMd = Join-Path $OutputDir "pacemaker-failover-gate-summary.md"
$summary | ConvertTo-Json -Depth 12 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Pacemaker Failover Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- nodes: $($summary.nodes)"
    "- failed_leader: $($summary.failed_leader)"
    "- node_exe: $($summary.node_exe)"
    "- exit_code: $($summary.exit_code)"
    "- error_reason: $($summary.error_reason)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "pacemaker failover gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"

if (-not $summary.pass) {
    throw "pacemaker failover gate FAILED: $($summary.error_reason)"
}

Write-Host "pacemaker failover gate PASS"
