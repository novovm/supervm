param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 1000)]
    [int]$InvalidPeers = 2,
    [ValidateRange(1, 1000)]
    [int]$InvalidBurst = 6,
    [ValidateRange(1, 1000)]
    [int]$BanAfter = 3,
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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\network-dos-gate"
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

function Parse-NetworkDosLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^network_dos_out:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }

    $m = [regex]::Match(
        $line,
        "^network_dos_out:\s+mode=(?<mode>\S+)\s+codec=(?<codec>\S+)\s+peers=(?<peers>\d+)\s+invalid_peers=(?<invalid_peers>\d+)\s+invalid_burst=(?<invalid_burst>\d+)\s+ban_after=(?<ban_after>\d+)\s+invalid_detected=(?<invalid_detected>\d+)\s+bans=(?<bans>\d+)\s+storm_rejected=(?<storm_rejected>\d+)\s+healthy_accepts=(?<healthy_accepts>\d+)\s+pass=(?<pass>true|false)\s+reason=(?<reason>\S+)$"
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
        codec = $m.Groups["codec"].Value
        peers = [int64]$m.Groups["peers"].Value
        invalid_peers = [int64]$m.Groups["invalid_peers"].Value
        invalid_burst = [int64]$m.Groups["invalid_burst"].Value
        ban_after = [int64]$m.Groups["ban_after"].Value
        invalid_detected = [int64]$m.Groups["invalid_detected"].Value
        bans = [int64]$m.Groups["bans"].Value
        storm_rejected = [int64]$m.Groups["storm_rejected"].Value
        healthy_accepts = [int64]$m.Groups["healthy_accepts"].Value
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

$stdoutPath = Join-Path $OutputDir "network-dos.stdout.log"
$stderrPath = Join-Path $OutputDir "network-dos.stderr.log"

$psi = [System.Diagnostics.ProcessStartInfo]::new()
$psi.FileName = $nodeExe
$psi.WorkingDirectory = $RepoRoot
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.CreateNoWindow = $true
$psi.Environment["NOVOVM_NODE_MODE"] = "network_dos_probe"
$psi.Environment["NOVOVM_NET_DOS_INVALID_PEERS"] = "$InvalidPeers"
$psi.Environment["NOVOVM_NET_DOS_INVALID_BURST"] = "$InvalidBurst"
$psi.Environment["NOVOVM_NET_DOS_BAN_AFTER"] = "$BanAfter"

$proc = [System.Diagnostics.Process]::Start($psi)
if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
    try { $proc.Kill() } catch {}
    throw "network_dos_probe timed out after ${TimeoutSeconds}s"
}

$stdout = $proc.StandardOutput.ReadToEnd()
$stderr = $proc.StandardError.ReadToEnd()
$stdout | Set-Content -Path $stdoutPath -Encoding UTF8
$stderr | Set-Content -Path $stderrPath -Encoding UTF8
$parsed = Parse-NetworkDosLine -Text ($stdout + $stderr)

$pass = $false
$errorReason = ""
if ($proc.ExitCode -ne 0) {
    $errorReason = "network_dos_probe exited with code $($proc.ExitCode)"
} elseif (-not $parsed -or -not $parsed.parse_ok) {
    $errorReason = "failed to parse network_dos_out line"
} else {
    $pass = (
        $parsed.pass -and
        $parsed.invalid_peers -eq $InvalidPeers -and
        $parsed.invalid_burst -eq $InvalidBurst -and
        $parsed.ban_after -eq $BanAfter -and
        $parsed.bans -eq $InvalidPeers -and
        $parsed.healthy_accepts -ge 1 -and
        $parsed.storm_rejected -ge $InvalidPeers -and
        $parsed.invalid_detected -ge ($InvalidPeers * $BanAfter)
    )
    if (-not $pass) {
        $errorReason = "network dos assertion failed (parsed_pass=$($parsed.pass), bans=$($parsed.bans), healthy_accepts=$($parsed.healthy_accepts), storm_rejected=$($parsed.storm_rejected), invalid_detected=$($parsed.invalid_detected))"
    }
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    invalid_peers = $InvalidPeers
    invalid_burst = $InvalidBurst
    ban_after = $BanAfter
    node_exe = $nodeExe
    exit_code = [int]$proc.ExitCode
    error_reason = $errorReason
    network_dos_signal = $parsed
    stdout = $stdoutPath
    stderr = $stderrPath
}

$summaryJson = Join-Path $OutputDir "network-dos-gate-summary.json"
$summaryMd = Join-Path $OutputDir "network-dos-gate-summary.md"
$summary | ConvertTo-Json -Depth 12 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Network DoS Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- invalid_peers: $($summary.invalid_peers)"
    "- invalid_burst: $($summary.invalid_burst)"
    "- ban_after: $($summary.ban_after)"
    "- node_exe: $($summary.node_exe)"
    "- exit_code: $($summary.exit_code)"
    "- error_reason: $($summary.error_reason)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "network dos gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"

if (-not $summary.pass) {
    throw "network dos gate FAILED: $($summary.error_reason)"
}

Write-Host "network dos gate PASS"
