param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [string]$ReportRoot = "artifacts/network-overlay-gate/adaptive-node-process-matrix",
    [int]$MaxFrames = 4,
    [int]$BasePort = 39720,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Join-RepoPath {
    param([string]$Path)
    if ([System.IO.Path]::IsPathRooted($Path)) {
        return $Path
    }
    return Join-Path $RepoRoot $Path
}

function Start-AdaptiveNodeJob {
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
        throw "adaptive node job timed out: $($Job.Name)`n$output"
    }
    $output = Receive-Job -Job $Job 2>&1 | Out-String
    $state = $Job.State
    Remove-Job -Job $Job -Force -ErrorAction SilentlyContinue
    if ($state -ne "Completed") {
        throw "adaptive node job failed: $($Job.Name)`n$output"
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

function New-BaseEnv {
    param(
        [string]$NodeId,
        [string]$BindAddr,
        [string]$ReportPath,
        [bool]$RelayEnabled = $false
    )
    return @{
        NOVOVM_OVERLAY_GATE_MODE = "adaptive-node"
        NOVOVM_OVERLAY_GATE_REPORT_PATH = $ReportPath
        NOVOVM_OVERLAY_GATE_MAX_FRAMES = $MaxFrames
        NOVOVM_OVERLAY_GATE_TIMEOUT_MS = 10000
        NOVOVM_OVERLAY_ADAPTIVE_NODE_ID = $NodeId
        NOVOVM_OVERLAY_ADAPTIVE_BIND_ADDR = $BindAddr
        NOVOVM_OVERLAY_ADAPTIVE_RELAY_ENABLED = if ($RelayEnabled) { "1" } else { "0" }
        NOVOVM_OVERLAY_ADAPTIVE_QUEUE_ENABLED = "1"
        NOVOVM_OVERLAY_ADAPTIVE_PEERS_JSON = $PeersJson
    }
}

function Run-DirectCase {
    $caseDir = New-CaseDirectory "direct"
    $receiverPath = Join-Path $caseDir "node-b.json"
    $senderPath = Join-Path $caseDir "node-a.json"
    $receiverJob = Start-AdaptiveNodeJob "adaptive-direct-node-b" `
        (New-BaseEnv "node-b" "127.0.0.1:$BasePort" $receiverPath)
    Start-Sleep -Milliseconds 250
    $senderEnv = New-BaseEnv "node-a" "0.0.0.0:0" $senderPath
    $senderEnv.NOVOVM_OVERLAY_ADAPTIVE_TARGET_PEER_ID = "node-b"
    $senderJob = Start-AdaptiveNodeJob "adaptive-direct-node-a" $senderEnv
    Wait-GateJob $senderJob
    Wait-GateJob $receiverJob
    $sender = Read-JsonReport $senderPath
    $receiver = Read-JsonReport $receiverPath
    $accepted = [bool]$sender.accepted -and [bool]$receiver.accepted -and
        $sender.selected_path -eq "DirectNovoRudp" -and
        [int]$receiver.direct_frames_received -eq $MaxFrames
    return [ordered]@{ case = "direct"; accepted = $accepted; sender = $sender; receiver = $receiver }
}

function Run-RelayCase {
    $caseDir = New-CaseDirectory "relay"
    $receiverPath = Join-Path $caseDir "node-b.json"
    $relayPath = Join-Path $caseDir "relay-1.json"
    $senderPath = Join-Path $caseDir "node-a.json"
    $receiverJob = Start-AdaptiveNodeJob "adaptive-relay-node-b" `
        (New-BaseEnv "node-b" "127.0.0.1:$BasePort" $receiverPath)
    $relayJob = Start-AdaptiveNodeJob "adaptive-relay-relay-1" `
        (New-BaseEnv "relay-1" "127.0.0.1:$($BasePort + 10)" $relayPath $true)
    Start-Sleep -Milliseconds 250
    $senderEnv = New-BaseEnv "node-a" "0.0.0.0:0" $senderPath
    $senderEnv.NOVOVM_OVERLAY_ADAPTIVE_TARGET_PEER_ID = "node-b"
    $senderEnv.NOVOVM_OVERLAY_ADAPTIVE_COOLDOWN_PEERS = "node-b"
    $senderJob = Start-AdaptiveNodeJob "adaptive-relay-node-a" $senderEnv
    Wait-GateJob $senderJob
    Wait-GateJob $relayJob
    Wait-GateJob $receiverJob
    $sender = Read-JsonReport $senderPath
    $relay = Read-JsonReport $relayPath
    $receiver = Read-JsonReport $receiverPath
    $accepted = [bool]$sender.accepted -and [bool]$relay.accepted -and [bool]$receiver.accepted -and
        $sender.selected_path -eq "RelayNovoRudp" -and
        [int]$relay.relay_frames_forwarded -eq $MaxFrames -and
        [int]$receiver.direct_frames_received -eq $MaxFrames
    return [ordered]@{ case = "relay"; accepted = $accepted; sender = $sender; relay = $relay; receiver = $receiver }
}

function Run-MultihopCase {
    $caseDir = New-CaseDirectory "multihop"
    $receiverPath = Join-Path $caseDir "node-b.json"
    $relay2Path = Join-Path $caseDir "relay-2.json"
    $relay3Path = Join-Path $caseDir "relay-3.json"
    $senderPath = Join-Path $caseDir "node-a.json"
    $receiverJob = Start-AdaptiveNodeJob "adaptive-multihop-node-b" `
        (New-BaseEnv "node-b" "127.0.0.1:$BasePort" $receiverPath)
    $relay2Job = Start-AdaptiveNodeJob "adaptive-multihop-relay-2" `
        (New-BaseEnv "relay-2" "127.0.0.1:$($BasePort + 20)" $relay2Path $true)
    $relay3Job = Start-AdaptiveNodeJob "adaptive-multihop-relay-3" `
        (New-BaseEnv "relay-3" "127.0.0.1:$($BasePort + 30)" $relay3Path $true)
    Start-Sleep -Milliseconds 250
    $senderEnv = New-BaseEnv "node-a" "0.0.0.0:0" $senderPath
    $senderEnv.NOVOVM_OVERLAY_ADAPTIVE_TARGET_PEER_ID = "node-b"
    $senderEnv.NOVOVM_OVERLAY_ADAPTIVE_COOLDOWN_PEERS = "node-b,relay-1"
    $senderJob = Start-AdaptiveNodeJob "adaptive-multihop-node-a" $senderEnv
    Wait-GateJob $senderJob
    Wait-GateJob $relay2Job
    Wait-GateJob $relay3Job
    Wait-GateJob $receiverJob
    $sender = Read-JsonReport $senderPath
    $relay2 = Read-JsonReport $relay2Path
    $relay3 = Read-JsonReport $relay3Path
    $receiver = Read-JsonReport $receiverPath
    $accepted = [bool]$sender.accepted -and [bool]$relay2.accepted -and [bool]$relay3.accepted -and
        [bool]$receiver.accepted -and
        $sender.selected_path -eq "MultiHopRelay" -and
        [int]$relay2.relay_frames_forwarded -eq $MaxFrames -and
        [int]$relay3.relay_frames_forwarded -eq $MaxFrames -and
        [int]$receiver.direct_frames_received -eq $MaxFrames
    return [ordered]@{ case = "multihop"; accepted = $accepted; sender = $sender; relay_2 = $relay2; relay_3 = $relay3; receiver = $receiver }
}

function Run-QueueCase {
    $caseDir = New-CaseDirectory "queue"
    $senderPath = Join-Path $caseDir "node-a.json"
    $senderEnv = New-BaseEnv "node-a" "0.0.0.0:0" $senderPath
    $senderEnv.NOVOVM_OVERLAY_ADAPTIVE_TARGET_PEER_ID = "node-b"
    $senderEnv.NOVOVM_OVERLAY_ADAPTIVE_COOLDOWN_PEERS = "node-b,relay-1,relay-2,relay-3"
    $senderJob = Start-AdaptiveNodeJob "adaptive-queue-node-a" $senderEnv
    Wait-GateJob $senderJob
    $sender = Read-JsonReport $senderPath
    $accepted = [bool]$sender.accepted -and
        $sender.selected_path -eq "QueueFallback" -and
        [int]$sender.queued_count -eq $MaxFrames
    return [ordered]@{ case = "queue"; accepted = $accepted; sender = $sender }
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

$Peers = @(
    @{ peer_id = "node-b"; endpoint = "127.0.0.1:$BasePort"; relay_enabled = $false },
    @{ peer_id = "relay-1"; endpoint = "127.0.0.1:$($BasePort + 10)"; relay_enabled = $true },
    @{ peer_id = "relay-2"; endpoint = "127.0.0.1:$($BasePort + 20)"; relay_enabled = $true },
    @{ peer_id = "relay-3"; endpoint = "127.0.0.1:$($BasePort + 30)"; relay_enabled = $true }
)
$PeersJson = $Peers | ConvertTo-Json -Compress

$cases = @()
$cases += Run-DirectCase
$cases += Run-RelayCase
$cases += Run-MultihopCase
$cases += Run-QueueCase

$failedCases = @($cases | Where-Object { -not $_.accepted })
$accepted = $failedCases.Count -eq 0
$report = [ordered]@{
    accepted = $accepted
    scope = "adaptive_overlay_node_process_matrix_v0"
    boundary = [ordered]@{
        network_only = $true
        payload_treated_opaque = $true
        apfl_interpreted = $false
        aoem_called = $false
        ledger_semantics = $false
        novorudp_wire_changed = $false
    }
    max_frames = $MaxFrames
    base_port = $BasePort
    peer_count = $Peers.Count
    cases = $cases
}

$reportPath = Join-Path $ReportRootAbs "report.json"
$report | ConvertTo-Json -Depth 40 | Set-Content -Encoding UTF8 $reportPath
$report | ConvertTo-Json -Depth 8

if (-not $accepted) {
    throw "adaptive node process matrix failed: $reportPath"
}
