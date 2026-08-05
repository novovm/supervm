param(
    [ValidateSet("commands", "run-node", "send", "aggregate")]
    [string]$Action = "commands",
    [string]$ConfigPath = "configs/network-overlay/adaptive-cross-machine-4node.example.json",
    [string]$Case = "adaptive-direct",
    [string]$NodeId = "",
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [string]$ReportRoot = "",
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

function Load-Json {
    param([string]$Path)
    $resolved = Join-RepoPath $Path
    if (-not (Test-Path $resolved)) {
        throw "missing config: $resolved"
    }
    Get-Content $resolved -Raw | ConvertFrom-Json
}

function Get-ConfigNode {
    param($Config, [string]$Id)
    $node = @($Config.nodes | Where-Object { $_.node_id -eq $Id })
    if ($node.Count -ne 1) {
        throw "expected exactly one node_id=$Id, found $($node.Count)"
    }
    $node[0]
}

function Get-ConfigCase {
    param($Config, [string]$Name)
    $caseConfig = @($Config.cases | Where-Object { $_.name -eq $Name })
    if ($caseConfig.Count -ne 1) {
        throw "expected exactly one case=$Name, found $($caseConfig.Count)"
    }
    $caseConfig[0]
}

function ConvertTo-PeerJson {
    param($Config)
    $peers = @()
    foreach ($node in $Config.nodes) {
        $peers += [ordered]@{
            peer_id = [string]$node.node_id
            endpoint = [string]$node.endpoint
            relay_enabled = [bool]$node.relay_enabled
        }
    }
    $peers | ConvertTo-Json -Compress
}

function Join-List {
    param($Value)
    if ($null -eq $Value) {
        return ""
    }
    @($Value) -join ","
}

function New-AdaptiveEnv {
    param(
        $Config,
        $CaseConfig,
        $Node,
        [string]$ReportPath,
        [bool]$IsSender
    )
    $envMap = @{
        NOVOVM_OVERLAY_GATE_MODE = "adaptive-node"
        NOVOVM_OVERLAY_GATE_REPORT_PATH = $ReportPath
        NOVOVM_OVERLAY_GATE_MAX_FRAMES = [string]$Config.max_frames
        NOVOVM_OVERLAY_GATE_TIMEOUT_MS = [string]$Config.timeout_ms
        NOVOVM_OVERLAY_ADAPTIVE_NODE_ID = [string]$Node.node_id
        NOVOVM_OVERLAY_ADAPTIVE_BIND_ADDR = [string]$Node.bind_addr
        NOVOVM_OVERLAY_ADAPTIVE_RELAY_ENABLED = if ([bool]$Node.relay_enabled) { "1" } else { "0" }
        NOVOVM_OVERLAY_ADAPTIVE_QUEUE_ENABLED = if ([bool]$Node.queue_enabled) { "1" } else { "0" }
        NOVOVM_OVERLAY_ADAPTIVE_PEERS_JSON = ConvertTo-PeerJson $Config
        NOVOVM_OVERLAY_ADAPTIVE_COOLDOWN_PEERS = Join-List $CaseConfig.cooldown_peers
        NOVOVM_OVERLAY_ADAPTIVE_COOLDOWN_ROUTE_FAMILIES = Join-List $CaseConfig.cooldown_route_families
    }
    if ($IsSender) {
        $envMap.NOVOVM_OVERLAY_ADAPTIVE_TARGET_PEER_ID = [string]$Config.target_peer_id
    }
    $envMap
}

function Set-ProcessEnv {
    param([hashtable]$Environment)
    foreach ($key in $Environment.Keys) {
        [Environment]::SetEnvironmentVariable($key, [string]$Environment[$key], "Process")
    }
}

function Ensure-GateBinary {
    if (-not $SkipBuild) {
        cargo build -q -p novovm-node --bin supervm-network-overlay-gate
    }
    $binary = Join-Path $RepoRoot "target\debug\supervm-network-overlay-gate.exe"
    if (-not (Test-Path $binary)) {
        throw "missing gate binary: $binary"
    }
    $binary
}

function Get-ReportPath {
    param([string]$CaseName, [string]$NodeName)
    Join-Path $ReportRootAbs (Join-Path $CaseName "$NodeName.json")
}

function Print-Commands {
    param($Config)
    foreach ($caseConfig in $Config.cases) {
        Write-Output ""
        Write-Output "## $($caseConfig.name)"
        foreach ($listenerId in @($caseConfig.listener_node_ids)) {
            Write-Output ("powershell -ExecutionPolicy Bypass -File scripts\novovm-adaptive-overlay-cross-machine-smoke.ps1 -Action run-node -ConfigPath {0} -Case {1} -NodeId {2}" -f $ConfigPath, $caseConfig.name, $listenerId)
        }
        Write-Output ("powershell -ExecutionPolicy Bypass -File scripts\novovm-adaptive-overlay-cross-machine-smoke.ps1 -Action send -ConfigPath {0} -Case {1} -NodeId {2}" -f $ConfigPath, $caseConfig.name, $caseConfig.sender_node_id)
        Write-Output ("powershell -ExecutionPolicy Bypass -File scripts\novovm-adaptive-overlay-cross-machine-smoke.ps1 -Action aggregate -ConfigPath {0} -Case {1}" -f $ConfigPath, $caseConfig.name)
    }
}

function Read-JsonReport {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        return $null
    }
    Get-Content $Path -Raw -Encoding UTF8 | ConvertFrom-Json
}

function Run-Aggregate {
    param($Config, $CaseConfig)
    $senderReport = Read-JsonReport (Get-ReportPath $CaseConfig.name $CaseConfig.sender_node_id)
    $listenerReports = @()
    foreach ($listenerId in @($CaseConfig.listener_node_ids)) {
        $listenerReports += [ordered]@{
            node_id = $listenerId
            report = Read-JsonReport (Get-ReportPath $CaseConfig.name $listenerId)
        }
    }

    $accepted = $null -ne $senderReport -and
        [bool]$senderReport.accepted -and
        $senderReport.selected_path -eq $CaseConfig.expected_path

    if ($CaseConfig.expected_path -eq "QueueFallback") {
        $accepted = $accepted -and
            [int]$senderReport.queued_count -eq [int]$Config.max_frames -and
            [int]$senderReport.sent_frame_count -eq 0 -and
            [int]$senderReport.sent_bytes_total -eq 0
    } else {
        foreach ($entry in $listenerReports) {
            if ($null -eq $entry.report) {
                $accepted = $false
                continue
            }
            $isTarget = $entry.node_id -eq $Config.target_peer_id
            if ($isTarget) {
                $accepted = $accepted -and [int]$entry.report.direct_frames_received -eq [int]$Config.max_frames
            } elseif ([bool](Get-ConfigNode $Config $entry.node_id).relay_enabled) {
                $accepted = $accepted -and [int]$entry.report.relay_frames_forwarded -eq [int]$Config.max_frames
            }
        }
    }

    $report = [ordered]@{
        accepted = [bool]$accepted
        scope = "adaptive_overlay_cross_machine_smoke_v0"
        case = $CaseConfig.name
        expected_path = $CaseConfig.expected_path
        boundary = [ordered]@{
            network_only = $true
            payload_treated_opaque = $true
            apfl_interpreted = $false
            aoem_called = $false
            opcode114_called = $false
            ledger_semantics = $false
            product_mainline_runtime = $false
            recipient_ack_verified = $false
            durable_delivery_journal = $false
            novorudp_wire_changed = $false
        }
        sender = $senderReport
        listeners = $listenerReports
    }
    $aggregatePath = Join-Path $ReportRootAbs (Join-Path $CaseConfig.name "aggregate.json")
    New-Item -ItemType Directory -Force -Path (Split-Path $aggregatePath -Parent) | Out-Null
    $report | ConvertTo-Json -Depth 40 | Set-Content -Encoding UTF8 $aggregatePath
    $report | ConvertTo-Json -Depth 8
    if (-not $accepted) {
        throw "adaptive cross-machine case failed or missing reports: $($CaseConfig.name)"
    }
}

Set-Location $RepoRoot
$Config = Load-Json $ConfigPath
if ([string]::IsNullOrWhiteSpace($ReportRoot)) {
    $ReportRoot = "artifacts/network-overlay-gate/$($Config.run_id)"
}
$ReportRootAbs = Join-RepoPath $ReportRoot
New-Item -ItemType Directory -Force -Path $ReportRootAbs | Out-Null

if ($Action -eq "commands") {
    Print-Commands $Config
    exit 0
}

$CaseConfig = Get-ConfigCase $Config $Case

if ($Action -eq "aggregate") {
    Run-Aggregate $Config $CaseConfig
    exit 0
}

if ([string]::IsNullOrWhiteSpace($NodeId)) {
    throw "-NodeId is required for Action=$Action"
}

$Node = Get-ConfigNode $Config $NodeId
$GateBinary = Ensure-GateBinary
$reportPath = Get-ReportPath $Case $NodeId
New-Item -ItemType Directory -Force -Path (Split-Path $reportPath -Parent) | Out-Null
$isSender = $Action -eq "send"
$envMap = New-AdaptiveEnv $Config $CaseConfig $Node $reportPath $isSender
Set-ProcessEnv $envMap
& $GateBinary
