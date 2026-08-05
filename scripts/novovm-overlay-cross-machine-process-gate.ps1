param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [string]$ConfigPath = "configs/network-overlay/cross-machine-loopback.example.json",
    [string]$ReportRoot = "artifacts/network-overlay-gate/cross-machine-process",
    [ValidateSet("all-local", "receiver", "relay", "sender", "queue")]
    [string]$Role = "all-local",
    [ValidateSet("all", "direct", "relay", "multihop", "queue")]
    [string]$Route = "all",
    [int]$RelayIndex = 0,
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

function Read-OverlayConfig {
    param([string]$Path)
    $resolved = Join-RepoPath $Path
    if (-not (Test-Path $resolved)) {
        throw "missing overlay config: $resolved"
    }
    Get-Content $resolved -Raw | ConvertFrom-Json
}

function New-CommonEnv {
    param(
        [string]$Mode,
        [string]$ReportPath
    )
    return @{
        NOVOVM_OVERLAY_GATE_MODE        = $Mode
        NOVOVM_OVERLAY_GATE_REPORT_PATH = $ReportPath
        NOVOVM_OVERLAY_GATE_MAX_FRAMES  = [int]$Config.max_frames
        NOVOVM_OVERLAY_GATE_TIMEOUT_MS  = [int]$Config.timeout_ms
    }
}

function Invoke-GateForeground {
    param([hashtable]$Environment)
    foreach ($key in $Environment.Keys) {
        [Environment]::SetEnvironmentVariable($key, [string]$Environment[$key], "Process")
    }
    & $GateBinary
    if ($LASTEXITCODE -ne 0) {
        throw "gate process failed with exit code $LASTEXITCODE"
    }
}

function Start-GateJob {
    param(
        [string]$Name,
        [hashtable]$Environment
    )
    Start-Job -Name $Name -ArgumentList $GateBinary, $Environment, $RepoRoot -ScriptBlock {
        param($GateBinary, $Environment, $RepoRoot)
        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"
        Set-Location $RepoRoot
        foreach ($key in $Environment.Keys) {
            [Environment]::SetEnvironmentVariable($key, [string]$Environment[$key], "Process")
        }
        & $GateBinary | Out-Null
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

function New-ReceiverEnv {
    param([string]$ReportPath)
    $env = New-CommonEnv "receiver" $ReportPath
    $env.NOVOVM_OVERLAY_GATE_BIND_ADDR = [string]$Config.receiver.bind_addr
    return $env
}

function New-RelayEnv {
    param(
        [int]$Index,
        [string]$ReportPath
    )
    if ($Index -ge @($Config.relays).Count) {
        throw "relay index out of range: $Index"
    }
    $relay = @($Config.relays)[$Index]
    $env = New-CommonEnv "relay" $ReportPath
    $env.NOVOVM_OVERLAY_GATE_BIND_ADDR = [string]$relay.bind_addr
    $env.NOVOVM_OVERLAY_GATE_RELAY_ID = [string]$relay.node_id
    return $env
}

function New-SenderEnv {
    param(
        [string]$RouteName,
        [string]$ReportPath
    )
    $env = New-CommonEnv "sender" $ReportPath
    $env.NOVOVM_OVERLAY_GATE_ROUTE = $RouteName
    $env.NOVOVM_OVERLAY_GATE_BIND_ADDR = [string]$Config.sender.bind_addr
    $env.NOVOVM_OVERLAY_GATE_TARGET_ADDR = [string]$Config.receiver.public_addr
    $env.NOVOVM_OVERLAY_GATE_RELAY_TARGET_ADDR = [string]$Config.receiver.public_addr
    $env.NOVOVM_OVERLAY_GATE_REQUEST_ID = "cross-machine-$RouteName"

    if ($RouteName -eq "relay" -or $RouteName -eq "multihop") {
        if (@($Config.relays).Count -lt 1) {
            throw "route $RouteName requires at least one relay"
        }
        $env.NOVOVM_OVERLAY_GATE_RELAY_ADDR = [string]@($Config.relays)[0].public_addr
    }
    if ($RouteName -eq "multihop") {
        if (@($Config.relays).Count -lt 2) {
            throw "route multihop requires at least two relays"
        }
        $env.NOVOVM_OVERLAY_GATE_NEXT_HOP_ADDR = [string]@($Config.relays)[1].public_addr
    }
    return $env
}

function Run-DirectCase {
    $caseDir = New-CaseDirectory "direct"
    $receiverPath = Join-Path $caseDir "receiver.json"
    $senderPath = Join-Path $caseDir "sender.json"
    $receiverJob = Start-GateJob "cross-direct-receiver" (New-ReceiverEnv $receiverPath)
    Start-Sleep -Milliseconds 250
    $senderJob = Start-GateJob "cross-direct-sender" (New-SenderEnv "direct" $senderPath)
    Wait-GateJob $senderJob
    Wait-GateJob $receiverJob
    $sender = Read-JsonReport $senderPath
    $receiver = Read-JsonReport $receiverPath
    $accepted = [bool]$sender.accepted -and [bool]$receiver.accepted -and
        ([int]$receiver.data_frames_received -eq [int]$Config.max_frames)
    return [ordered]@{
        case = "direct"; accepted = $accepted; sender = $sender; receiver = $receiver
        sender_report_path = $senderPath; receiver_report_path = $receiverPath
    }
}

function Run-RelayCase {
    $caseDir = New-CaseDirectory "relay"
    $receiverPath = Join-Path $caseDir "receiver.json"
    $relayPath = Join-Path $caseDir "relay.json"
    $senderPath = Join-Path $caseDir "sender.json"
    $receiverJob = Start-GateJob "cross-relay-receiver" (New-ReceiverEnv $receiverPath)
    $relayJob = Start-GateJob "cross-relay-relay" (New-RelayEnv 0 $relayPath)
    Start-Sleep -Milliseconds 250
    $senderJob = Start-GateJob "cross-relay-sender" (New-SenderEnv "relay" $senderPath)
    Wait-GateJob $senderJob
    Wait-GateJob $relayJob
    Wait-GateJob $receiverJob
    $sender = Read-JsonReport $senderPath
    $relay = Read-JsonReport $relayPath
    $receiver = Read-JsonReport $receiverPath
    $accepted = [bool]$sender.accepted -and [bool]$relay.accepted -and [bool]$receiver.accepted -and
        ([int]$relay.frames_received -eq [int]$Config.max_frames) -and
        ([int]$receiver.data_frames_received -eq [int]$Config.max_frames)
    return [ordered]@{
        case = "relay"; accepted = $accepted; sender = $sender; relay = $relay; receiver = $receiver
        sender_report_path = $senderPath; relay_report_path = $relayPath; receiver_report_path = $receiverPath
    }
}

function Run-MultiHopCase {
    $caseDir = New-CaseDirectory "multihop"
    $receiverPath = Join-Path $caseDir "receiver.json"
    $relay0Path = Join-Path $caseDir "relay-0.json"
    $relay1Path = Join-Path $caseDir "relay-1.json"
    $senderPath = Join-Path $caseDir "sender.json"
    $receiverJob = Start-GateJob "cross-multihop-receiver" (New-ReceiverEnv $receiverPath)
    $relay0Job = Start-GateJob "cross-multihop-relay-0" (New-RelayEnv 0 $relay0Path)
    $relay1Job = Start-GateJob "cross-multihop-relay-1" (New-RelayEnv 1 $relay1Path)
    Start-Sleep -Milliseconds 250
    $senderJob = Start-GateJob "cross-multihop-sender" (New-SenderEnv "multihop" $senderPath)
    Wait-GateJob $senderJob
    Wait-GateJob $relay0Job
    Wait-GateJob $relay1Job
    Wait-GateJob $receiverJob
    $sender = Read-JsonReport $senderPath
    $relay0 = Read-JsonReport $relay0Path
    $relay1 = Read-JsonReport $relay1Path
    $receiver = Read-JsonReport $receiverPath
    $accepted = [bool]$sender.accepted -and [bool]$relay0.accepted -and [bool]$relay1.accepted -and
        [bool]$receiver.accepted -and
        ([int]$relay0.frames_received -eq [int]$Config.max_frames) -and
        ([int]$relay1.frames_received -eq [int]$Config.max_frames) -and
        ([int]$receiver.data_frames_received -eq [int]$Config.max_frames)
    return [ordered]@{
        case = "multihop"; accepted = $accepted; sender = $sender
        relay_0 = $relay0; relay_1 = $relay1; receiver = $receiver
        sender_report_path = $senderPath; relay_0_report_path = $relay0Path
        relay_1_report_path = $relay1Path; receiver_report_path = $receiverPath
    }
}

function Run-QueueCase {
    $caseDir = New-CaseDirectory "queue"
    $senderPath = Join-Path $caseDir "sender.json"
    $senderJob = Start-GateJob "cross-queue-sender" (New-SenderEnv "queue" $senderPath)
    Wait-GateJob $senderJob
    $sender = Read-JsonReport $senderPath
    $accepted = [bool]$sender.accepted -and ([int]$sender.queued_count -eq [int]$Config.max_frames)
    return [ordered]@{
        case = "queue"; accepted = $accepted; sender = $sender; sender_report_path = $senderPath
    }
}

function Invoke-SingleRole {
    $roleReportName = if ($Role -eq "relay") { "relay-$RelayIndex" } else { $Role }
    $reportPath = Join-Path $ReportRootAbs "$Route-$roleReportName.json"
    switch ($Role) {
        "receiver" {
            Invoke-GateForeground (New-ReceiverEnv $reportPath)
        }
        "relay" {
            Invoke-GateForeground (New-RelayEnv $RelayIndex $reportPath)
        }
        "sender" {
            if ($Route -eq "all") {
                throw "sender role requires -Route direct|relay|multihop"
            }
            Invoke-GateForeground (New-SenderEnv $Route $reportPath)
        }
        "queue" {
            Invoke-GateForeground (New-SenderEnv "queue" $reportPath)
        }
        default {
            throw "unsupported single role: $Role"
        }
    }
}

Set-Location $RepoRoot
$Config = Read-OverlayConfig $ConfigPath
$ReportRootAbs = Join-RepoPath $ReportRoot
New-Item -ItemType Directory -Force -Path $ReportRootAbs | Out-Null

if (-not $SkipBuild) {
    cargo build -q -p novovm-node --bin supervm-network-overlay-gate
}

$GateBinary = Join-Path $RepoRoot "target\debug\supervm-network-overlay-gate.exe"
if (-not (Test-Path $GateBinary)) {
    throw "missing gate binary: $GateBinary"
}

if ($Role -ne "all-local") {
    Invoke-SingleRole
    exit 0
}

$cases = @()
if ($Route -eq "all" -or $Route -eq "direct") {
    $cases += Run-DirectCase
}
if ($Route -eq "all" -or $Route -eq "relay") {
    $cases += Run-RelayCase
}
if ($Route -eq "all" -or $Route -eq "multihop") {
    $cases += Run-MultiHopCase
}
if ($Route -eq "all" -or $Route -eq "queue") {
    $cases += Run-QueueCase
}

$failedCases = @($cases | Where-Object { -not $_.accepted })
$accepted = $failedCases.Count -eq 0
$report = [ordered]@{
    accepted = $accepted
    scope = "network_overlay_cross_machine_process_gate_v0"
    config_path = (Join-RepoPath $ConfigPath)
    role = $Role
    route = $Route
    boundary = [ordered]@{
        network_only = $true
        payload_treated_opaque = $true
        apfl_interpreted = $false
        aoem_called = $false
        ledger_semantics = $false
        product_mainline_runtime = $false
        recipient_ack_verified = $false
        durable_delivery_journal = $false
        novorudp_wire_changed = $false
    }
    max_frames = [int]$Config.max_frames
    case_count = $cases.Count
    cases = $cases
}

$reportPath = Join-Path $ReportRootAbs "report.json"
$report | ConvertTo-Json -Depth 40 | Set-Content -Encoding UTF8 $reportPath
$report | ConvertTo-Json -Depth 8

if (-not $accepted) {
    throw "cross-machine process gate failed: $reportPath"
}
