param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [string]$ReportRoot = "artifacts/network-overlay-gate/fallback-chain-process",
    [int]$MaxFrames = 4,
    [int]$BasePort = 39410,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Join-RepoPath {
    param([string]$Path)
    if ([System.IO.Path]::IsPathRooted($Path)) {
        return $Path
    }
    return (Join-Path $RepoRoot $Path)
}

function Start-GateJob {
    param(
        [string]$Name,
        [string]$BinaryPath,
        [hashtable]$Environment
    )

    Start-Job -Name $Name -ArgumentList $BinaryPath, $Environment, $RepoRoot -ScriptBlock {
        param($BinaryPath, $Environment, $RepoRoot)
        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"
        Set-Location $RepoRoot
        foreach ($key in $Environment.Keys) {
            [Environment]::SetEnvironmentVariable($key, [string]$Environment[$key], "Process")
        }
        & $BinaryPath | Out-Null
    }
}

function Wait-GateJob {
    param(
        [System.Management.Automation.Job]$Job,
        [int]$TimeoutSeconds = 30
    )

    $completed = Wait-Job -Job $Job -Timeout $TimeoutSeconds
    if (-not $completed) {
        Stop-Job -Job $Job -ErrorAction SilentlyContinue
        $output = Receive-Job -Job $Job -ErrorAction SilentlyContinue | Out-String
        Remove-Job -Job $Job -Force -ErrorAction SilentlyContinue
        throw "gate job timed out: $($Job.Name)`n$output"
    }

    $output = Receive-Job -Job $Job 2>&1 | Out-String
    $state = $Job.State
    Remove-Job -Job $Job -Force -ErrorAction SilentlyContinue
    if ($state -ne "Completed") {
        throw "gate job failed: $($Job.Name)`n$output"
    }
}

function Read-JsonReport {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        throw "missing report: $Path"
    }
    Get-Content $Path -Raw | ConvertFrom-Json
}

function New-CaseDirectory {
    param([string]$CaseName)
    $path = Join-Path $ReportRootAbs $CaseName
    New-Item -ItemType Directory -Force -Path $path | Out-Null
    return $path
}

function New-CommonEnv {
    param(
        [string]$Mode,
        [string]$ReportPath
    )
    return @{
        NOVOVM_OVERLAY_GATE_MODE        = $Mode
        NOVOVM_OVERLAY_GATE_REPORT_PATH = $ReportPath
        NOVOVM_OVERLAY_GATE_MAX_FRAMES  = $MaxFrames
        NOVOVM_OVERLAY_GATE_TIMEOUT_MS  = 10000
    }
}

function Run-DirectCase {
    $caseName = "direct"
    $caseDir = New-CaseDirectory $caseName
    $receiverReportPath = Join-Path $caseDir "receiver.json"
    $senderReportPath = Join-Path $caseDir "sender.json"
    $receiverAddr = "127.0.0.1:$BasePort"

    $receiverEnv = New-CommonEnv "receiver" $receiverReportPath
    $receiverEnv.NOVOVM_OVERLAY_GATE_BIND_ADDR = $receiverAddr
    $receiverJob = Start-GateJob "$caseName-receiver" $GateBinary $receiverEnv
    Start-Sleep -Milliseconds 250

    $senderEnv = New-CommonEnv "sender" $senderReportPath
    $senderEnv.NOVOVM_OVERLAY_GATE_ROUTE = "direct"
    $senderEnv.NOVOVM_OVERLAY_GATE_TARGET_ADDR = $receiverAddr
    $senderEnv.NOVOVM_OVERLAY_GATE_REQUEST_ID = "fallback-process-direct"
    $senderJob = Start-GateJob "$caseName-sender" $GateBinary $senderEnv

    Wait-GateJob $senderJob
    Wait-GateJob $receiverJob

    $sender = Read-JsonReport $senderReportPath
    $receiver = Read-JsonReport $receiverReportPath
    $accepted = [bool]$sender.accepted -and [bool]$receiver.accepted -and
        ([int]$receiver.data_frames_received -eq $MaxFrames)
    return [ordered]@{
        case                 = $caseName
        expected_route       = "direct"
        accepted             = $accepted
        sender_report_path   = $senderReportPath
        receiver_report_path = $receiverReportPath
        sender               = $sender
        receiver             = $receiver
    }
}

function Run-RelayCase {
    $caseName = "relay"
    $caseDir = New-CaseDirectory $caseName
    $receiverReportPath = Join-Path $caseDir "receiver.json"
    $relayReportPath = Join-Path $caseDir "relay.json"
    $senderReportPath = Join-Path $caseDir "sender.json"
    $receiverAddr = "127.0.0.1:$($BasePort + 1)"
    $relayAddr = "127.0.0.1:$($BasePort + 2)"

    $receiverEnv = New-CommonEnv "receiver" $receiverReportPath
    $receiverEnv.NOVOVM_OVERLAY_GATE_BIND_ADDR = $receiverAddr
    $receiverJob = Start-GateJob "$caseName-receiver" $GateBinary $receiverEnv

    $relayEnv = New-CommonEnv "relay" $relayReportPath
    $relayEnv.NOVOVM_OVERLAY_GATE_BIND_ADDR = $relayAddr
    $relayEnv.NOVOVM_OVERLAY_GATE_RELAY_ID = "peer-relay-a"
    $relayJob = Start-GateJob "$caseName-relay" $GateBinary $relayEnv
    Start-Sleep -Milliseconds 250

    $senderEnv = New-CommonEnv "sender" $senderReportPath
    $senderEnv.NOVOVM_OVERLAY_GATE_ROUTE = "relay"
    $senderEnv.NOVOVM_OVERLAY_GATE_RELAY_ADDR = $relayAddr
    $senderEnv.NOVOVM_OVERLAY_GATE_RELAY_TARGET_ADDR = $receiverAddr
    $senderEnv.NOVOVM_OVERLAY_GATE_TARGET_ADDR = $receiverAddr
    $senderEnv.NOVOVM_OVERLAY_GATE_REQUEST_ID = "fallback-process-relay"
    $senderJob = Start-GateJob "$caseName-sender" $GateBinary $senderEnv

    Wait-GateJob $senderJob
    Wait-GateJob $relayJob
    Wait-GateJob $receiverJob

    $sender = Read-JsonReport $senderReportPath
    $relay = Read-JsonReport $relayReportPath
    $receiver = Read-JsonReport $receiverReportPath
    $accepted = [bool]$sender.accepted -and [bool]$relay.accepted -and [bool]$receiver.accepted -and
        ([int]$relay.frames_received -eq $MaxFrames) -and
        ([int]$receiver.data_frames_received -eq $MaxFrames)
    return [ordered]@{
        case                 = $caseName
        expected_route       = "relay"
        accepted             = $accepted
        sender_report_path   = $senderReportPath
        relay_report_path    = $relayReportPath
        receiver_report_path = $receiverReportPath
        sender               = $sender
        relay                = $relay
        receiver             = $receiver
    }
}

function Run-MultiHopCase {
    $caseName = "multihop"
    $caseDir = New-CaseDirectory $caseName
    $receiverReportPath = Join-Path $caseDir "receiver.json"
    $relayBReportPath = Join-Path $caseDir "relay-b.json"
    $relayCReportPath = Join-Path $caseDir "relay-c.json"
    $senderReportPath = Join-Path $caseDir "sender.json"
    $receiverAddr = "127.0.0.1:$($BasePort + 3)"
    $relayBAddr = "127.0.0.1:$($BasePort + 4)"
    $relayCAddr = "127.0.0.1:$($BasePort + 5)"

    $receiverEnv = New-CommonEnv "receiver" $receiverReportPath
    $receiverEnv.NOVOVM_OVERLAY_GATE_BIND_ADDR = $receiverAddr
    $receiverJob = Start-GateJob "$caseName-receiver" $GateBinary $receiverEnv

    $relayBEnv = New-CommonEnv "relay" $relayBReportPath
    $relayBEnv.NOVOVM_OVERLAY_GATE_BIND_ADDR = $relayBAddr
    $relayBEnv.NOVOVM_OVERLAY_GATE_RELAY_ID = "peer-relay-b"
    $relayBJob = Start-GateJob "$caseName-relay-b" $GateBinary $relayBEnv

    $relayCEnv = New-CommonEnv "relay" $relayCReportPath
    $relayCEnv.NOVOVM_OVERLAY_GATE_BIND_ADDR = $relayCAddr
    $relayCEnv.NOVOVM_OVERLAY_GATE_RELAY_ID = "peer-relay-c"
    $relayCJob = Start-GateJob "$caseName-relay-c" $GateBinary $relayCEnv
    Start-Sleep -Milliseconds 250

    $senderEnv = New-CommonEnv "sender" $senderReportPath
    $senderEnv.NOVOVM_OVERLAY_GATE_ROUTE = "multihop"
    $senderEnv.NOVOVM_OVERLAY_GATE_RELAY_ADDR = $relayBAddr
    $senderEnv.NOVOVM_OVERLAY_GATE_NEXT_HOP_ADDR = $relayCAddr
    $senderEnv.NOVOVM_OVERLAY_GATE_RELAY_TARGET_ADDR = $receiverAddr
    $senderEnv.NOVOVM_OVERLAY_GATE_TARGET_ADDR = $receiverAddr
    $senderEnv.NOVOVM_OVERLAY_GATE_REQUEST_ID = "fallback-process-multihop"
    $senderJob = Start-GateJob "$caseName-sender" $GateBinary $senderEnv

    Wait-GateJob $senderJob
    Wait-GateJob $relayBJob
    Wait-GateJob $relayCJob
    Wait-GateJob $receiverJob

    $sender = Read-JsonReport $senderReportPath
    $relayB = Read-JsonReport $relayBReportPath
    $relayC = Read-JsonReport $relayCReportPath
    $receiver = Read-JsonReport $receiverReportPath
    $accepted = [bool]$sender.accepted -and [bool]$relayB.accepted -and
        [bool]$relayC.accepted -and [bool]$receiver.accepted -and
        ([int]$relayB.frames_received -eq $MaxFrames) -and
        ([int]$relayC.frames_received -eq $MaxFrames) -and
        ([int]$receiver.data_frames_received -eq $MaxFrames)
    return [ordered]@{
        case                 = $caseName
        expected_route       = "multihop"
        accepted             = $accepted
        sender_report_path   = $senderReportPath
        relay_b_report_path  = $relayBReportPath
        relay_c_report_path  = $relayCReportPath
        receiver_report_path = $receiverReportPath
        sender               = $sender
        relay_b              = $relayB
        relay_c              = $relayC
        receiver             = $receiver
    }
}

function Run-QueueCase {
    $caseName = "queue"
    $caseDir = New-CaseDirectory $caseName
    $senderReportPath = Join-Path $caseDir "sender.json"

    $senderEnv = New-CommonEnv "sender" $senderReportPath
    $senderEnv.NOVOVM_OVERLAY_GATE_ROUTE = "queue"
    $senderEnv.NOVOVM_OVERLAY_GATE_TARGET_ADDR = "127.0.0.1:$($BasePort + 6)"
    $senderEnv.NOVOVM_OVERLAY_GATE_REQUEST_ID = "fallback-process-queue"
    $senderJob = Start-GateJob "$caseName-sender" $GateBinary $senderEnv
    Wait-GateJob $senderJob

    $sender = Read-JsonReport $senderReportPath
    $accepted = [bool]$sender.accepted -and ([int]$sender.queued_count -eq $MaxFrames)
    return [ordered]@{
        case               = $caseName
        expected_route     = "queue"
        accepted           = $accepted
        sender_report_path = $senderReportPath
        sender             = $sender
    }
}

Set-Location $RepoRoot
$ReportRootAbs = Join-RepoPath $ReportRoot
New-Item -ItemType Directory -Force -Path $ReportRootAbs | Out-Null

if (-not $SkipBuild) {
    cargo build -q -p novovm-node --bin supervm-network-overlay-gate
}

$GateBinary = Join-Path $RepoRoot "target\debug\supervm-network-overlay-gate.exe"
if (-not (Test-Path $GateBinary)) {
    throw "missing gate binary: $GateBinary"
}

$cases = @()
$cases += Run-DirectCase
$cases += Run-RelayCase
$cases += Run-MultiHopCase
$cases += Run-QueueCase

$failedCases = @($cases | Where-Object { -not $_.accepted })
$accepted = $failedCases.Count -eq 0
$report = [ordered]@{
    accepted      = $accepted
    scope         = "network_overlay_fallback_chain_process_gate_v0"
    boundary      = [ordered]@{
        network_only          = $true
        payload_treated_opaque = $true
        apfl_interpreted      = $false
        aoem_called           = $false
        ledger_semantics      = $false
        novorudp_wire_changed = $false
    }
    max_frames    = $MaxFrames
    base_port     = $BasePort
    case_count    = $cases.Count
    cases         = $cases
}

$reportPath = Join-Path $ReportRootAbs "report.json"
$report | ConvertTo-Json -Depth 40 | Set-Content -Encoding UTF8 $reportPath
$report | ConvertTo-Json -Depth 8

if (-not $accepted) {
    throw "fallback chain process gate failed: $reportPath"
}
