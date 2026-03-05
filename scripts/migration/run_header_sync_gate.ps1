param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(2, 1000000)]
    [int]$RemoteHeaders = 8,
    [ValidateRange(1, 999999)]
    [int]$LocalHeaders = 3,
    [ValidateRange(1, 1000000)]
    [int]$FetchLimit = 64,
    [ValidateRange(1, 1000000)]
    [int]$TamperHeight = 5,
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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\header-sync-gate"
}

if ($LocalHeaders -ge $RemoteHeaders) {
    throw "LocalHeaders ($LocalHeaders) must be less than RemoteHeaders ($RemoteHeaders)"
}
if ($TamperHeight -le $LocalHeaders -or $TamperHeight -ge $RemoteHeaders) {
    throw "TamperHeight ($TamperHeight) must be in ($LocalHeaders, $RemoteHeaders)"
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

function Parse-HeaderSyncLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^header_sync_out:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }

    $m = [regex]::Match(
        $line,
        "^header_sync_out:\s+mode=(?<mode>\S+)\s+codec=(?<codec>\S+)\s+remote_tip=(?<remote_tip>\d+)\s+local_tip_before=(?<local_tip_before>\d+)\s+fetched=(?<fetched>\d+)\s+applied=(?<applied>\d+)\s+local_tip_after=(?<local_tip_after>\d+)\s+complete=(?<complete>true|false)\s+pass=(?<pass>true|false)\s+tamper_at=(?<tamper_at>\d+)\s+reason=(?<reason>\S+)$"
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
        remote_tip = [int64]$m.Groups["remote_tip"].Value
        local_tip_before = [int64]$m.Groups["local_tip_before"].Value
        fetched = [int64]$m.Groups["fetched"].Value
        applied = [int64]$m.Groups["applied"].Value
        local_tip_after = [int64]$m.Groups["local_tip_after"].Value
        complete = [bool]::Parse($m.Groups["complete"].Value)
        pass = [bool]::Parse($m.Groups["pass"].Value)
        tamper_at = [int64]$m.Groups["tamper_at"].Value
        reason = $m.Groups["reason"].Value
        raw = $line
    }
}

function Invoke-HeaderSyncProbe {
    param(
        [string]$NodeExe,
        [string]$WorkDir,
        [hashtable]$EnvVars,
        [int]$TimeoutSeconds,
        [string]$StdoutPath,
        [string]$StderrPath
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $NodeExe
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    foreach ($k in $EnvVars.Keys) {
        $psi.Environment[$k] = [string]$EnvVars[$k]
    }

    $proc = [System.Diagnostics.Process]::Start($psi)
    if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
        try { $proc.Kill() } catch {}
        throw "header_sync_probe timed out after ${TimeoutSeconds}s"
    }

    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $stdout | Set-Content -Path $StdoutPath -Encoding UTF8
    $stderr | Set-Content -Path $StderrPath -Encoding UTF8
    $text = ($stdout + $stderr).Trim()
    $parsed = Parse-HeaderSyncLine -Text $text

    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        stdout = $stdout
        stderr = $stderr
        parsed = $parsed
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

$positiveStdout = Join-Path $OutputDir "header-sync-positive.stdout.log"
$positiveStderr = Join-Path $OutputDir "header-sync-positive.stderr.log"
$negativeStdout = Join-Path $OutputDir "header-sync-negative.stdout.log"
$negativeStderr = Join-Path $OutputDir "header-sync-negative.stderr.log"

$positiveRun = Invoke-HeaderSyncProbe `
    -NodeExe $nodeExe `
    -WorkDir $RepoRoot `
    -EnvVars @{
        NOVOVM_NODE_MODE = "header_sync_probe"
        NOVOVM_HEADER_SYNC_REMOTE_HEADERS = "$RemoteHeaders"
        NOVOVM_HEADER_SYNC_LOCAL_HEADERS = "$LocalHeaders"
        NOVOVM_HEADER_SYNC_FETCH_LIMIT = "$FetchLimit"
        NOVOVM_HEADER_SYNC_TAMPER_PARENT_AT = "0"
    } `
    -TimeoutSeconds $TimeoutSeconds `
    -StdoutPath $positiveStdout `
    -StderrPath $positiveStderr

$negativeRun = Invoke-HeaderSyncProbe `
    -NodeExe $nodeExe `
    -WorkDir $RepoRoot `
    -EnvVars @{
        NOVOVM_NODE_MODE = "header_sync_probe"
        NOVOVM_HEADER_SYNC_REMOTE_HEADERS = "$RemoteHeaders"
        NOVOVM_HEADER_SYNC_LOCAL_HEADERS = "$LocalHeaders"
        NOVOVM_HEADER_SYNC_FETCH_LIMIT = "$FetchLimit"
        NOVOVM_HEADER_SYNC_TAMPER_PARENT_AT = "$TamperHeight"
    } `
    -TimeoutSeconds $TimeoutSeconds `
    -StdoutPath $negativeStdout `
    -StderrPath $negativeStderr

$positiveSignal = $positiveRun.parsed
$negativeSignal = $negativeRun.parsed

$positivePass = (
    $positiveRun.exit_code -eq 0 -and
    $null -ne $positiveSignal -and
    $positiveSignal.parse_ok -and
    $positiveSignal.pass -and
    $positiveSignal.complete -and
    $positiveSignal.local_tip_after -eq $positiveSignal.remote_tip -and
    $positiveSignal.tamper_at -eq 0
)

$negativePass = (
    $negativeRun.exit_code -eq 0 -and
    $null -ne $negativeSignal -and
    $negativeSignal.parse_ok -and
    (-not $negativeSignal.pass) -and
    (-not $negativeSignal.complete) -and
    $negativeSignal.tamper_at -eq $TamperHeight -and
    $negativeSignal.local_tip_after -lt $negativeSignal.remote_tip -and
    $negativeSignal.reason.StartsWith("parent_hash_mismatch_at_")
)

$pass = $positivePass -and $negativePass
$errorReason = ""
if (-not $pass) {
    $errorReason = "header sync gate assertion failed (positive_pass=$positivePass, negative_pass=$negativePass)"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    remote_headers = $RemoteHeaders
    local_headers = $LocalHeaders
    fetch_limit = $FetchLimit
    tamper_height = $TamperHeight
    node_exe = $nodeExe
    error_reason = $errorReason
    header_sync_signal = [ordered]@{
        pass = $positivePass
        exit_code = $positiveRun.exit_code
        parsed = $positiveSignal
        stdout = $positiveStdout
        stderr = $positiveStderr
    }
    header_sync_negative_signal = [ordered]@{
        pass = $negativePass
        exit_code = $negativeRun.exit_code
        parsed = $negativeSignal
        stdout = $negativeStdout
        stderr = $negativeStderr
    }
}

$summaryJson = Join-Path $OutputDir "header-sync-gate-summary.json"
$summaryMd = Join-Path $OutputDir "header-sync-gate-summary.md"
$summary | ConvertTo-Json -Depth 16 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Header Sync Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- remote_headers: $($summary.remote_headers)"
    "- local_headers: $($summary.local_headers)"
    "- fetch_limit: $($summary.fetch_limit)"
    "- tamper_height: $($summary.tamper_height)"
    "- node_exe: $($summary.node_exe)"
    "- error_reason: $($summary.error_reason)"
    ""
    "## Signals"
    ""
    "- header_sync_signal.pass: $($summary.header_sync_signal.pass)"
    "- header_sync_signal.exit_code: $($summary.header_sync_signal.exit_code)"
    "- header_sync_negative_signal.pass: $($summary.header_sync_negative_signal.pass)"
    "- header_sync_negative_signal.exit_code: $($summary.header_sync_negative_signal.exit_code)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "header sync gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  header_sync_signal.pass: $($summary.header_sync_signal.pass)"
Write-Host "  header_sync_negative_signal.pass: $($summary.header_sync_negative_signal.pass)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"

if (-not $summary.pass) {
    throw "header sync gate FAILED: $($summary.error_reason)"
}

Write-Host "header sync gate PASS"
