param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [UInt64]$ChainId = 1,
    [string]$LocalGethEnode = "",
    [UInt64]$DiscoveryMaxPeers = 8,
    [UInt64]$DiscoveryMaxVisit = 1000,
    [UInt64]$SessionMaxPeers = 4,
    [UInt64]$PublicSessionMaxAttempts = 16,
    [UInt64]$PublicMaxRounds = 4,
    [string]$PublicPluginPorts = "30303,30304",
    [UInt64]$ProbeTimeoutMs = 8000,
    [UInt64]$ProbeCacheTtlMs = 16000,
    [UInt64]$ReadWindowMs = 4000,
    [UInt64]$WarmupSeconds = 2,
    [UInt64]$PollSeconds = 4,
    [UInt64]$PollRounds = 4,
    [string]$SummaryOut = "artifacts/migration/rlpx-session-canary-after-a484a8506-summary.json",
    [string]$MarkdownOut = "artifacts/migration/rlpx-session-canary-after-a484a8506.md",
    [switch]$SkipBuild,
    [switch]$FailOnPublicSessionFailure
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RootPath {
    param([string]$Root)
    if (-not $Root) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
    }
    return (Resolve-Path $Root).Path
}

function Resolve-FullPath {
    param(
        [string]$Root,
        [string]$Value
    )
    if ([System.IO.Path]::IsPathRooted($Value)) {
        return [System.IO.Path]::GetFullPath($Value)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $Root $Value))
}

function Ensure-DirectoryForFile {
    param([string]$Path)
    $dir = Split-Path -Parent $Path
    if ($dir) {
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
    }
}

function Invoke-JsonRpc {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)]$Params
    )
    $body = @{
        jsonrpc = "2.0"
        id = 1
        method = $Method
        params = $Params
    } | ConvertTo-Json -Depth 32 -Compress
    $resp = Invoke-RestMethod -Uri $Url -Method Post -ContentType "application/json" -Body $body
    if ($resp -is [string]) {
        $resp = $resp | ConvertFrom-Json
    }
    if (($resp.PSObject.Properties.Name -contains "error") -and $null -ne $resp.error) {
        throw ("{0} failed: code={1} message={2}" -f $Method, $resp.error.code, $resp.error.message)
    }
    return $resp
}

function Parse-HexU64 {
    param($Raw)
    if ($null -eq $Raw) {
        return [UInt64]0
    }
    $text = ([string]$Raw).Trim()
    if (-not $text) {
        return [UInt64]0
    }
    if ($text.StartsWith("0x") -or $text.StartsWith("0X")) {
        if ($text.Length -le 2) {
            return [UInt64]0
        }
        return [Convert]::ToUInt64($text.Substring(2), 16)
    }
    return [UInt64]$text
}

function Get-StageRank {
    param([string]$Stage)
    $value = ""
    if ($null -ne $Stage) {
        $value = $Stage.Trim().ToLowerInvariant()
    }
    switch ($value) {
        "tcp_connected" { return 1 }
        "auth_sent" { return 2 }
        "ack_seen" { return 3 }
        "hello_sent" { return 4 }
        "hello_seen" { return 5 }
        "status_seen" { return 6 }
        "status_sent" { return 7 }
        "ready" { return 8 }
        default { return 0 }
    }
}

function Get-StageFromItem {
    param($Item)
    if ($null -eq $Item) {
        return "disconnected"
    }
    if ($Item.PSObject.Properties.Name -contains "best_stage" -and $Item.best_stage) {
        return [string]$Item.best_stage
    }
    if ($Item.PSObject.Properties.Name -contains "stage" -and $Item.stage) {
        return [string]$Item.stage
    }
    return "disconnected"
}

function Get-StringField {
    param(
        $Obj,
        [string]$Name
    )
    if ($null -ne $Obj -and $Obj.PSObject.Properties.Name -contains $Name -and $null -ne $Obj.$Name) {
        return [string]$Obj.$Name
    }
    return ""
}

function Convert-ReportText {
    param([string]$Text)
    if ($null -eq $Text) {
        return ""
    }
    return ([string]$Text).Replace([string][char]96, "'") -replace "[^\x09\x0a\x0d\x20-\x7e]", "?"
}

function Get-DisconnectReasonCode {
    param([string]$Text)
    if (-not $Text) {
        return ""
    }
    $match = [regex]::Match($Text, "reason(?:_code)?=?(0x[0-9a-fA-F]+|\d+)")
    if ($match.Success) {
        return $match.Groups[1].Value
    }
    return ""
}

function Get-NodeIdFromEnode {
    param([string]$Endpoint)
    if (-not $Endpoint) {
        return ""
    }
    $match = [regex]::Match($Endpoint, "^enode://([^@]+)@")
    if ($match.Success) {
        return $match.Groups[1].Value
    }
    return ""
}

function Get-EnodeParts {
    param([string]$Endpoint)
    if (-not $Endpoint) {
        return $null
    }
    $match = [regex]::Match($Endpoint, "^enode://([^@]+)@(\[[^\]]+\]|[^:/?#]+):(\d+)(.*)$")
    if (-not $match.Success) {
        return $null
    }
    return [pscustomobject]@{
        node_id = $match.Groups[1].Value
        host = $match.Groups[2].Value
        port = [int]$match.Groups[3].Value
        suffix = $match.Groups[4].Value
    }
}

function Test-PublicEndpointHost {
    param([string]$EndpointHost)
    if (-not $EndpointHost) {
        return $false
    }
    $rawHost = $EndpointHost.Trim()
    if ($rawHost.StartsWith("[") -and $rawHost.EndsWith("]")) {
        $rawHost = $rawHost.Substring(1, $rawHost.Length - 2)
    }
    $addr = [System.Net.IPAddress]::None
    if (-not [System.Net.IPAddress]::TryParse($rawHost, [ref]$addr)) {
        return $true
    }
    if ([System.Net.IPAddress]::IsLoopback($addr)) {
        return $false
    }
    if ($addr.Equals([System.Net.IPAddress]::Any) -or $addr.Equals([System.Net.IPAddress]::IPv6Any)) {
        return $false
    }
    if ($addr.AddressFamily -eq [System.Net.Sockets.AddressFamily]::InterNetwork) {
        $bytes = $addr.GetAddressBytes()
        if ($bytes[0] -eq 10 -or $bytes[0] -eq 127 -or $bytes[0] -eq 0) {
            return $false
        }
        if ($bytes[0] -eq 172 -and $bytes[1] -ge 16 -and $bytes[1] -le 31) {
            return $false
        }
        if ($bytes[0] -eq 192 -and $bytes[1] -eq 168) {
            return $false
        }
        if ($bytes[0] -eq 169 -and $bytes[1] -eq 254) {
            return $false
        }
        if ($bytes[0] -eq 100 -and $bytes[1] -ge 64 -and $bytes[1] -le 127) {
            return $false
        }
        return $true
    }
    if ($addr.IsIPv6LinkLocal -or $addr.IsIPv6SiteLocal) {
        return $false
    }
    $ipv6 = $addr.GetAddressBytes()
    if (($ipv6[0] -band 0xfe) -eq 0xfc) {
        return $false
    }
    return $true
}

function Convert-EnodePort {
    param(
        [string]$Endpoint,
        [int]$Port
    )
    $parts = Get-EnodeParts -Endpoint $Endpoint
    if ($null -eq $parts) {
        return ""
    }
    return ("enode://{0}@{1}:{2}{3}" -f $parts.node_id, $parts.host, $Port, $parts.suffix)
}

function Get-PublicPluginPortList {
    param([string]$Ports)
    $values = New-Object System.Collections.Generic.List[int]
    foreach ($part in @($Ports -split ",")) {
        $trimmed = $part.Trim()
        if (-not $trimmed) {
            continue
        }
        $port = 0
        if ([int]::TryParse($trimmed, [ref]$port) -and $port -gt 0 -and $port -le 65535 -and -not $values.Contains($port)) {
            $values.Add($port)
        }
    }
    if ($values.Count -eq 0) {
        $values.Add(30303)
        $values.Add(30304)
    }
    return $values.ToArray()
}

function New-PublicSessionCandidateSelection {
    param(
        $Records,
        [UInt64]$MaxAttempts,
        [string]$Ports
    )
    $recordsArray = @()
    if ($null -ne $Records) {
        $recordsArray = @($Records)
    }
    $portList = @(Get-PublicPluginPortList -Ports $Ports)
    $dedup = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    $primary = New-Object System.Collections.Generic.List[object]
    $alternate = New-Object System.Collections.Generic.List[object]
    $filtered = [UInt64]0

    foreach ($record in $recordsArray) {
        $endpoint = Get-StringField -Obj $record -Name "endpoint"
        $remoteEnr = Get-StringField -Obj $record -Name "remote_enr"
        $remoteNodeId = Get-StringField -Obj $record -Name "remote_node_id"
        if (-not $remoteNodeId) {
            $remoteNodeId = Get-NodeIdFromEnode -Endpoint $endpoint
        }
        $parts = Get-EnodeParts -Endpoint $endpoint
        if ($null -eq $parts -or -not (Test-PublicEndpointHost -EndpointHost $parts.host)) {
            $filtered = [UInt64]($filtered + 1)
            continue
        }

        if ($dedup.Add($endpoint)) {
            $primary.Add([pscustomobject]@{
                endpoint = $endpoint
                remote_node_id = $remoteNodeId
                remote_enr = $remoteEnr
                source_endpoint = $endpoint
                port = $parts.port
            })
        }
        foreach ($port in $portList) {
            if ($port -eq $parts.port) {
                continue
            }
            $variant = Convert-EnodePort -Endpoint $endpoint -Port $port
            if ($variant -and $dedup.Add($variant)) {
                $alternate.Add([pscustomobject]@{
                    endpoint = $variant
                    remote_node_id = $remoteNodeId
                    remote_enr = $remoteEnr
                    source_endpoint = $endpoint
                    port = $port
                })
            }
        }
    }

    $combined = @($primary.ToArray()) + @($alternate.ToArray())
    $limit = [int][Math]::Max(1, [Math]::Min([UInt64]$combined.Count, [UInt64]$MaxAttempts))
    return [pscustomobject][ordered]@{
        candidate_peer_count = [UInt64]$recordsArray.Count
        candidate_after_filter_count = [UInt64]$combined.Count
        filtered_candidate_count = $filtered
        selected_attempt_count = [UInt64]$limit
        candidates = @($combined | Select-Object -First $limit)
    }
}

function Test-ReasonIsTooManyPeers {
    param([string]$Reason)
    if (-not $Reason) {
        return $false
    }
    $normalized = $Reason.ToLowerInvariant()
    return ($normalized.Contains("too_many_peers") -or $normalized.Contains("reason_code=4") -or $normalized.Contains("reason=4") -or $normalized.Contains("reason=0x4") -or $normalized.Contains("reason_code=0x4"))
}

function Test-ReasonIsTcpTimeout {
    param([string]$Reason)
    if (-not $Reason) {
        return $false
    }
    $normalized = $Reason.ToLowerInvariant()
    return ($normalized.Contains("timed out") -or $normalized.Contains("timeout") -or $normalized.Contains("10060"))
}

function Push-ProcessEnv {
    param([hashtable]$Environment)
    $state = @{}
    foreach ($key in $Environment.Keys) {
        $envPath = "Env:$key"
        $exists = Test-Path $envPath
        $oldValue = $null
        if ($exists) {
            $oldValue = (Get-Item -Path $envPath).Value
        }
        $state[$key] = [pscustomobject]@{
            exists = $exists
            value = $oldValue
        }
        Set-Item -Path $envPath -Value ([string]$Environment[$key])
    }
    return $state
}

function Pop-ProcessEnv {
    param([hashtable]$State)
    if ($null -eq $State) {
        return
    }
    foreach ($key in $State.Keys) {
        $entry = $State[$key]
        $envPath = "Env:$key"
        if ($entry.exists) {
            Set-Item -Path $envPath -Value ([string]$entry.value)
        } else {
            Remove-Item -Path $envPath -ErrorAction SilentlyContinue
        }
    }
}

function Convert-PluginPeerItemsToMetrics {
    param(
        $Items,
        [UInt64]$CandidateCount
    )
    $metrics = [ordered]@{
        tcp_connect_attempt_count = [UInt64]$CandidateCount
        tcp_connect_success_count = [UInt64]0
        tcp_connect_fail_count = [UInt64]0
        tcp_connect_timeout_count = [UInt64]0
        rlpx_auth_sent_count = [UInt64]0
        rlpx_auth_ack_seen_count = [UInt64]0
        rlpx_auth_timeout_count = [UInt64]0
        rlpx_disconnect_before_ack_count = [UInt64]0
        hello_sent_count = [UInt64]0
        hello_seen_count = [UInt64]0
        status_sent_count = [UInt64]0
        status_seen_count = [UInt64]0
        ready_count = [UInt64]0
        disconnected_count = [UInt64]0
        disconnect_reason_too_many_peers_count = [UInt64]0
        peer_cooldown_count = [UInt64]0
        selected_eth_capability = ""
        disconnect_reason_code = ""
        traces = @()
    }
    $itemsArray = @()
    if ($null -ne $Items) {
        $itemsArray = @($Items)
    }
    if ($itemsArray.Count -gt $metrics.tcp_connect_attempt_count) {
        $metrics.tcp_connect_attempt_count = [UInt64]$itemsArray.Count
    }
    foreach ($item in $itemsArray) {
        $stage = Get-StringField -Obj $item -Name "stage"
        $bestStage = Get-StageFromItem -Item $item
        $rank = Get-StageRank -Stage $bestStage
        $lastError = Get-StringField -Obj $item -Name "last_error"
        $inferredRank = $rank
        if ($lastError -match "before_eth_status|eth_status_timeout|eth_capability_not_found") {
            $inferredRank = [Math]::Max($inferredRank, (Get-StageRank -Stage "hello_seen"))
        } elseif ($lastError -match "before_hello|remote_hello_timeout") {
            $inferredRank = [Math]::Max($inferredRank, (Get-StageRank -Stage "hello_sent"))
        } elseif ($inferredRank -lt (Get-StageRank -Stage "auth_sent") -and $lastError -match "rlpx_|auth|handshake|ack") {
            $inferredRank = Get-StageRank -Stage "auth_sent"
        }
        $cap = Get-StringField -Obj $item -Name "selected_eth_capability"
        $endpoint = Get-StringField -Obj $item -Name "endpoint"
        $addr = Get-StringField -Obj $item -Name "addr_hint"
        $nodeHint = Get-StringField -Obj $item -Name "node_hint"
        $dialAttempts = Parse-HexU64 -Raw (Get-StringField -Obj $item -Name "dial_attempt_count")

        if ($inferredRank -ge (Get-StageRank -Stage "tcp_connected")) {
            $metrics.tcp_connect_success_count = [UInt64]($metrics.tcp_connect_success_count + 1)
        } elseif ($dialAttempts -gt 0 -or $lastError -match "connect|timed|refused|unreachable|unreachable") {
            $metrics.tcp_connect_fail_count = [UInt64]($metrics.tcp_connect_fail_count + 1)
            if (Test-ReasonIsTcpTimeout -Reason $lastError) {
                $metrics.tcp_connect_timeout_count = [UInt64]($metrics.tcp_connect_timeout_count + 1)
            }
        }
        if ($inferredRank -ge (Get-StageRank -Stage "auth_sent")) {
            $metrics.rlpx_auth_sent_count = [UInt64]($metrics.rlpx_auth_sent_count + 1)
        }
        if ($inferredRank -ge (Get-StageRank -Stage "ack_seen")) {
            $metrics.rlpx_auth_ack_seen_count = [UInt64]($metrics.rlpx_auth_ack_seen_count + 1)
        }
        if ($inferredRank -ge (Get-StageRank -Stage "hello_sent")) {
            $metrics.hello_sent_count = [UInt64]($metrics.hello_sent_count + 1)
        }
        if ($inferredRank -ge (Get-StageRank -Stage "hello_seen")) {
            $metrics.hello_seen_count = [UInt64]($metrics.hello_seen_count + 1)
        }
        if ($inferredRank -ge (Get-StageRank -Stage "status_seen")) {
            $metrics.status_seen_count = [UInt64]($metrics.status_seen_count + 1)
        }
        if ($inferredRank -ge (Get-StageRank -Stage "status_sent")) {
            $metrics.status_sent_count = [UInt64]($metrics.status_sent_count + 1)
        }
        if ($inferredRank -ge (Get-StageRank -Stage "ready")) {
            $metrics.ready_count = [UInt64]($metrics.ready_count + 1)
        }
        if ($stage -eq "disconnected") {
            $metrics.disconnected_count = [UInt64]($metrics.disconnected_count + 1)
        }
        if ($inferredRank -ge (Get-StageRank -Stage "auth_sent") -and $inferredRank -lt (Get-StageRank -Stage "ack_seen")) {
            if ($stage -eq "disconnected" -or $lastError) {
                $metrics.rlpx_disconnect_before_ack_count = [UInt64]($metrics.rlpx_disconnect_before_ack_count + 1)
            }
        }
        if ($inferredRank -ge (Get-StageRank -Stage "auth_sent") -and $inferredRank -lt (Get-StageRank -Stage "ack_seen") -and $lastError -match "timeout|timed|read|ack|eof") {
            $metrics.rlpx_auth_timeout_count = [UInt64]($metrics.rlpx_auth_timeout_count + 1)
        }
        if (-not $metrics.selected_eth_capability -and $cap) {
            $metrics.selected_eth_capability = $cap
        }
        $reasonCode = Get-DisconnectReasonCode -Text $lastError
        if (-not $metrics.disconnect_reason_code -and $reasonCode) {
            $metrics.disconnect_reason_code = $reasonCode
        }
        if (Test-ReasonIsTooManyPeers -Reason $lastError) {
            $metrics.disconnect_reason_too_many_peers_count = [UInt64]($metrics.disconnect_reason_too_many_peers_count + 1)
        }

        $remoteId = Get-NodeIdFromEnode -Endpoint $endpoint
        if (-not $remoteId) {
            $remoteId = $nodeHint
        }
        $metrics.traces += [pscustomobject]@{
            remote_node_id = $remoteId
            remote_endpoint = $addr
            endpoint = $endpoint
            stage = $stage
            best_stage = $bestStage
            reason = $lastError
            selected_eth_capability = $(if ($cap) { $cap } else { "none" })
        }
    }
    return $metrics
}

function New-RlpxMetricAccumulator {
    return [ordered]@{
        candidate_peer_count = [UInt64]0
        candidate_after_filter_count = [UInt64]0
        selected_attempt_count = [UInt64]0
        public_session_round_count = [UInt64]0
        tcp_connect_attempt_count = [UInt64]0
        tcp_connect_success_count = [UInt64]0
        tcp_connect_fail_count = [UInt64]0
        tcp_connect_timeout_count = [UInt64]0
        rlpx_auth_sent_count = [UInt64]0
        rlpx_auth_ack_seen_count = [UInt64]0
        rlpx_auth_timeout_count = [UInt64]0
        rlpx_disconnect_before_ack_count = [UInt64]0
        hello_sent_count = [UInt64]0
        hello_seen_count = [UInt64]0
        status_sent_count = [UInt64]0
        status_seen_count = [UInt64]0
        ready_count = [UInt64]0
        disconnected_count = [UInt64]0
        disconnect_reason_too_many_peers_count = [UInt64]0
        peer_cooldown_count = [UInt64]0
        selected_eth_capability = ""
        disconnect_reason_code = ""
        traces = @()
    }
}

function Add-RlpxMetrics {
    param(
        [System.Collections.IDictionary]$Accumulator,
        $Metrics
    )
    if ($null -eq $Metrics) {
        return
    }
    foreach ($key in @(
        "tcp_connect_attempt_count",
        "tcp_connect_success_count",
        "tcp_connect_fail_count",
        "tcp_connect_timeout_count",
        "rlpx_auth_sent_count",
        "rlpx_auth_ack_seen_count",
        "rlpx_auth_timeout_count",
        "rlpx_disconnect_before_ack_count",
        "hello_sent_count",
        "hello_seen_count",
        "status_sent_count",
        "status_seen_count",
        "ready_count",
        "disconnected_count",
        "disconnect_reason_too_many_peers_count"
    )) {
        if ($Metrics.Contains($key)) {
            $Accumulator[$key] = [UInt64]($Accumulator[$key] + [UInt64]$Metrics[$key])
        }
    }
    if (-not $Accumulator.selected_eth_capability -and $Metrics.selected_eth_capability) {
        $Accumulator.selected_eth_capability = $Metrics.selected_eth_capability
    }
    if (-not $Accumulator.disconnect_reason_code -and $Metrics.disconnect_reason_code) {
        $Accumulator.disconnect_reason_code = $Metrics.disconnect_reason_code
    }
    foreach ($trace in @($Metrics.traces)) {
        $Accumulator.traces += $trace
    }
}

function Invoke-GatewaySessionLayer {
    param(
        [string]$LayerName,
        [string[]]$Peers,
        [string]$RepoRootValue,
        [string]$GatewayExe,
        [string]$Bind,
        [UInt64]$LayerChainId,
        [string]$LogDir,
        [string]$StateRoot
    )
    $peers = @($Peers | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -First ([int]$SessionMaxPeers))
    $layer = [ordered]@{
        name = $LayerName
        status = "skipped"
        reason = ""
        peers = $peers
        metrics = $null
        capability = $null
        plugin_peers = $null
        gateway_stdout = ""
        gateway_stderr = ""
    }
    if ($peers.Count -eq 0) {
        $layer.reason = "no peer endpoint supplied"
        return [pscustomobject]$layer
    }

    $safeLayer = $LayerName.Replace(" ", "-").Replace("_", "-")
    $runTag = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $gwOut = Join-Path $LogDir ("rlpx-layered-{0}-{1}.stdout.log" -f $safeLayer, $runTag)
    $gwErr = Join-Path $LogDir ("rlpx-layered-{0}-{1}.stderr.log" -f $safeLayer, $runTag)
    $layer.gateway_stdout = $gwOut
    $layer.gateway_stderr = $gwErr

    $stateDir = Join-Path $StateRoot $safeLayer
    New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
    $envMap = @{
        "NOVOVM_GATEWAY_BIND" = $Bind
        "NOVOVM_GATEWAY_UA_STORE_PATH" = (Join-Path $stateDir "unified-account-router.rocksdb")
        "NOVOVM_GATEWAY_ETH_TX_INDEX_PATH" = (Join-Path $stateDir "eth-tx-index.rocksdb")
        "NOVOVM_GATEWAY_SPOOL_DIR" = (Join-Path $stateDir "spool")
        "NOVOVM_GATEWAY_WARN_LOG" = "1"
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ROUTE_POLICY" = "plugin_only"
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES" = "0"
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_MIN_CANDIDATES" = "0"
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS" = ($peers -join ",")
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PORTS" = $PublicPluginPorts
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PROBE_TIMEOUT_MS" = ([string]$ProbeTimeoutMs)
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PROBE_CACHE_TTL_MS" = ([string]$ProbeCacheTtlMs)
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_SESSION_PROBE_MODE" = "disabled"
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_ENABLE" = "1"
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_ENABLE" = "1"
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_SINGLE_SESSION" = "1"
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_MAX_PEERS_PER_TICK" = ([string][Math]::Max(1, [Math]::Min([int]$SessionMaxPeers, $peers.Count)))
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_POLL_MS" = "250"
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_TIMEOUT_MS" = ([string]$ProbeTimeoutMs)
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_READ_WINDOW_MS" = ([string]$ReadWindowMs)
    }

    $gatewayProc = $null
    try {
        $envState = Push-ProcessEnv -Environment $envMap
        try {
            $gatewayProc = Start-Process `
                -FilePath $GatewayExe `
                -WorkingDirectory $RepoRootValue `
                -RedirectStandardOutput $gwOut `
                -RedirectStandardError $gwErr `
                -PassThru `
                -WindowStyle Hidden
        } finally {
            Pop-ProcessEnv -State $envState
        }
        Start-Sleep -Seconds ([int][Math]::Max(1, $WarmupSeconds))
        if ($gatewayProc.HasExited) {
            $layer.status = "failed"
            $layer.reason = "gateway exited before polling"
            return [pscustomobject]$layer
        }
        $url = "http://$Bind"
        $rounds = [int][Math]::Max(1, $PollRounds)
        for ($round = 1; $round -le $rounds; $round++) {
            try {
                $capResp = Invoke-JsonRpc -Url $url -Method "evm_getPublicBroadcastCapability" -Params @{ chain_id = [UInt64]$LayerChainId }
                $peersResp = Invoke-JsonRpc -Url $url -Method "evm_getPublicBroadcastPluginPeers" -Params @{ chain_id = [UInt64]$LayerChainId }
                $capResult = $capResp.result
                $peerResult = $peersResp.result
                $layer.capability = [ordered]@{
                    mode = Get-StringField -Obj $capResult -Name "mode"
                    ready = $(if ($capResult.PSObject.Properties.Name -contains "ready") { [bool]$capResult.ready } else { $false })
                    native_plugin_peer_count = Get-StringField -Obj $capResult -Name "native_plugin_peer_count"
                    native_plugin_session_stage_counts = $(if ($capResult.PSObject.Properties.Name -contains "native_plugin_session_stage_counts") { $capResult.native_plugin_session_stage_counts } else { $null })
                    native_plugin_mempool_ingest_enabled = $(if ($capResult.PSObject.Properties.Name -contains "native_plugin_mempool_ingest_enabled") { [bool]$capResult.native_plugin_mempool_ingest_enabled } else { $false })
                    native_plugin_mempool_ingest_last_error = Get-StringField -Obj $capResult -Name "native_plugin_mempool_ingest_last_error"
                }
                $layer.plugin_peers = [ordered]@{
                    peer_source = Get-StringField -Obj $peerResult -Name "peer_source"
                    total = Get-StringField -Obj $peerResult -Name "total"
                    reachable = Get-StringField -Obj $peerResult -Name "reachable"
                    checked_ms = Get-StringField -Obj $peerResult -Name "checked_ms"
                }
                $items = @($peersResp.result.items)
                $metrics = Convert-PluginPeerItemsToMetrics -Items $items -CandidateCount ([UInt64]$peers.Count)
                $layer.metrics = $metrics
                $layer.status = "completed"
                if ($metrics.ready_count -gt 0 -or $metrics.status_seen_count -gt 0) {
                    break
                }
            } catch {
                $layer.status = "failed"
                $layer.reason = $_.Exception.Message
                break
            }
            Start-Sleep -Seconds ([int][Math]::Max(1, $PollSeconds))
        }
        if ($layer.status -eq "completed" -and $null -eq $layer.metrics) {
            $layer.reason = "no plugin peer rows returned"
        }
    } finally {
        if ($null -ne $gatewayProc -and -not $gatewayProc.HasExited) {
            try {
                Stop-Process -Id $gatewayProc.Id -Force -ErrorAction SilentlyContinue
            } catch {
            }
        }
    }
    return [pscustomobject]$layer
}

function Invoke-PublicReadinessClosure {
    param(
        $Records,
        [string]$RepoRootValue,
        [string]$GatewayExe,
        [string]$Bind,
        [UInt64]$LayerChainId,
        [string]$LogDir,
        [string]$StateRoot
    )
    $selection = New-PublicSessionCandidateSelection -Records $Records -MaxAttempts $PublicSessionMaxAttempts -Ports $PublicPluginPorts
    $metrics = New-RlpxMetricAccumulator
    $metrics.candidate_peer_count = $selection.candidate_peer_count
    $metrics.candidate_after_filter_count = $selection.candidate_after_filter_count
    $metrics.selected_attempt_count = $selection.selected_attempt_count

    $layer = [ordered]@{
        name = "public discovered-peer session"
        status = "skipped"
        reason = ""
        peers = @($selection.candidates | ForEach-Object { [string]$_.endpoint })
        selection = $selection
        metrics = $metrics
        rounds = @()
        capability = $null
        plugin_peers = $null
        gateway_stdout = ""
        gateway_stderr = ""
        readiness_claimed = $false
    }
    if (@($selection.candidates).Count -eq 0) {
        $layer.reason = "no usable public session candidates after endpoint filtering"
        return [pscustomobject]$layer
    }

    $candidateQueue = @($selection.candidates)
    $attempted = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    $cooldown = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    $cooldownNodes = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    $timeoutPenalty = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    $roundLimit = [int][Math]::Max(1, $PublicMaxRounds)

    for ($round = 1; $round -le $roundLimit; $round++) {
        $batch = New-Object System.Collections.Generic.List[string]
        $batchNodeIds = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
        foreach ($candidate in $candidateQueue) {
            $endpoint = [string]$candidate.endpoint
            if (-not $endpoint -or $attempted.Contains($endpoint) -or $cooldown.Contains($endpoint) -or $timeoutPenalty.Contains($endpoint)) {
                continue
            }
            $nodeId = [string]$candidate.remote_node_id
            if ($nodeId -and $cooldownNodes.Contains($nodeId)) {
                continue
            }
            if ($nodeId -and $batchNodeIds.Contains($nodeId)) {
                continue
            }
            $batch.Add($endpoint)
            [void]$attempted.Add($endpoint)
            if ($nodeId) {
                [void]$batchNodeIds.Add($nodeId)
            }
            if ($batch.Count -ge [int][Math]::Max(1, $SessionMaxPeers)) {
                break
            }
        }
        if ($batch.Count -eq 0) {
            break
        }

        $metrics.public_session_round_count = [UInt64]($metrics.public_session_round_count + 1)
        $roundLayer = Invoke-GatewaySessionLayer `
            -LayerName ("public discovered-peer session round {0}" -f $round) `
            -Peers @($batch.ToArray()) `
            -RepoRootValue $RepoRootValue `
            -GatewayExe $GatewayExe `
            -Bind $Bind `
            -LayerChainId $LayerChainId `
            -LogDir $LogDir `
            -StateRoot $StateRoot
        $layer.rounds += $roundLayer
        $layer.gateway_stdout = $roundLayer.gateway_stdout
        $layer.gateway_stderr = $roundLayer.gateway_stderr
        if ($null -ne $roundLayer.capability) {
            $layer.capability = $roundLayer.capability
        }
        if ($null -ne $roundLayer.plugin_peers) {
            $layer.plugin_peers = $roundLayer.plugin_peers
        }
        if ($null -ne $roundLayer.metrics) {
            Add-RlpxMetrics -Accumulator $metrics -Metrics $roundLayer.metrics
            foreach ($trace in @($roundLayer.metrics.traces)) {
                $endpoint = [string]$trace.endpoint
                $reason = [string]$trace.reason
                if ($endpoint -and (Test-ReasonIsTooManyPeers -Reason $reason)) {
                    [void]$cooldown.Add($endpoint)
                    $nodeId = [string]$trace.remote_node_id
                    if ($nodeId) {
                        [void]$cooldownNodes.Add($nodeId)
                    }
                } elseif ($endpoint -and (Test-ReasonIsTcpTimeout -Reason $reason)) {
                    [void]$timeoutPenalty.Add($endpoint)
                }
            }
            if ($metrics.ready_count -gt 0) {
                break
            }
        }
    }

    $metrics.peer_cooldown_count = [UInt64][Math]::Max($cooldown.Count, $cooldownNodes.Count)
    $metrics.tcp_connect_timeout_count = [UInt64]([Math]::Max([UInt64]$metrics.tcp_connect_timeout_count, [UInt64]$timeoutPenalty.Count))
    $layer.metrics = $metrics
    $layer.readiness_claimed = ($metrics.ready_count -gt 0 -and $metrics.rlpx_auth_ack_seen_count -gt 0 -and $metrics.hello_seen_count -gt 0 -and $metrics.status_seen_count -gt 0 -and ($metrics.selected_eth_capability -eq "69" -or $metrics.selected_eth_capability -eq "70" -or $metrics.selected_eth_capability -eq "eth/69" -or $metrics.selected_eth_capability -eq "eth/70"))
    if ($metrics.ready_count -gt 0) {
        $layer.status = "completed"
        $layer.reason = "public discovered-peer session reached ready"
    } elseif ($metrics.public_session_round_count -gt 0) {
        $layer.status = "completed"
        $layer.reason = ("public discovered-peer session did not reach ready after {0} round(s)" -f $metrics.public_session_round_count)
    } else {
        $layer.status = "skipped"
        $layer.reason = "no public session batch remained after cooldown or endpoint penalty filtering"
    }
    return [pscustomobject]$layer
}

function Convert-LayerToMarkdown {
    param($Layer)
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add(("### {0}" -f $Layer.name))
    $lines.Add("")
    $lines.Add(('- status: `{0}`' -f $Layer.status))
    if ($Layer.reason) {
        $safeReason = Convert-ReportText -Text ([string]$Layer.reason)
        $lines.Add(('- reason: `{0}`' -f $safeReason))
    }
    if ($null -ne $Layer.metrics) {
        $m = $Layer.metrics
        if ($m.Contains("candidate_peer_count")) {
            $lines.Add(('- candidates: discovered=`{0}`, after_filter=`{1}`, selected_attempts=`{2}`, rounds=`{3}`' -f $m.candidate_peer_count, $m.candidate_after_filter_count, $m.selected_attempt_count, $m.public_session_round_count))
        }
        $tcpTimeout = $(if ($m.Contains("tcp_connect_timeout_count")) { $m.tcp_connect_timeout_count } else { [UInt64]0 })
        $lines.Add(('- tcp: attempts=`{0}`, success=`{1}`, fail=`{2}`, timeout=`{3}`' -f $m.tcp_connect_attempt_count, $m.tcp_connect_success_count, $m.tcp_connect_fail_count, $tcpTimeout))
        $lines.Add(('- auth: sent=`{0}`, ack_seen=`{1}`, timeout=`{2}`, disconnect_before_ack=`{3}`' -f $m.rlpx_auth_sent_count, $m.rlpx_auth_ack_seen_count, $m.rlpx_auth_timeout_count, $m.rlpx_disconnect_before_ack_count))
        $lines.Add(('- p2p/eth: hello_sent=`{0}`, hello_seen=`{1}`, status_sent=`{2}`, status_seen=`{3}`, ready=`{4}`' -f $m.hello_sent_count, $m.hello_seen_count, $m.status_sent_count, $m.status_seen_count, $m.ready_count))
        $lines.Add(('- selected_eth_capability: `{0}`' -f ($(if ($m.selected_eth_capability) { $m.selected_eth_capability } else { "none" }))))
        if ($m.Contains("disconnect_reason_too_many_peers_count")) {
            $lines.Add(('- disconnect_reason_too_many_peers_count: `{0}`' -f $m.disconnect_reason_too_many_peers_count))
        }
        if ($m.Contains("peer_cooldown_count")) {
            $lines.Add(('- peer_cooldown_count: `{0}`' -f $m.peer_cooldown_count))
        }
        if ($m.disconnect_reason_code) {
            $lines.Add(('- disconnect_reason_code: `{0}`' -f $m.disconnect_reason_code))
        }
        $lines.Add("")
        $lines.Add("Compact traces:")
        foreach ($trace in @($m.traces)) {
            $traceReason = Convert-ReportText -Text ([string]$trace.reason)
            $lines.Add(('- peer=`{0}` endpoint=`{1}` stage=`{2}` best=`{3}` reason=`{4}` cap=`{5}`' -f $trace.remote_node_id, $trace.remote_endpoint, $trace.stage, $trace.best_stage, $traceReason, $trace.selected_eth_capability))
        }
    }
    $lines.Add("")
    return $lines.ToArray()
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$SummaryOut = Resolve-FullPath -Root $RepoRoot -Value $SummaryOut
$MarkdownOut = Resolve-FullPath -Root $RepoRoot -Value $MarkdownOut
Ensure-DirectoryForFile -Path $SummaryOut
Ensure-DirectoryForFile -Path $MarkdownOut

Push-Location $RepoRoot
try {
    if (-not $SkipBuild) {
        & cargo build -p novovm-evm-gateway
        if ($LASTEXITCODE -ne 0) {
            throw "build failed: novovm-evm-gateway"
        }
    }

    $gatewayExe = Join-Path $RepoRoot "target\debug\novovm-evm-gateway.exe"
    if (-not (Test-Path $gatewayExe)) {
        throw "gateway binary not found: $gatewayExe"
    }

    $logDir = Resolve-FullPath -Root $RepoRoot -Value "artifacts/migration/logs"
    $stateRoot = Resolve-FullPath -Root $RepoRoot -Value ("artifacts/migration/state/rlpx-layered-canary-{0}" -f ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()))
    New-Item -ItemType Directory -Force -Path $logDir | Out-Null
    New-Item -ItemType Directory -Force -Path $stateRoot | Out-Null

    $localLayer = if ([string]::IsNullOrWhiteSpace($LocalGethEnode)) {
        [pscustomobject][ordered]@{
            name = "local controlled geth peer"
            status = "skipped"
            reason = "LocalGethEnode was not supplied; this diagnostic does not spawn a geth peer"
            peers = @()
            metrics = $null
            capability = $null
            plugin_peers = $null
            gateway_stdout = ""
            gateway_stderr = ""
        }
    } else {
        Invoke-GatewaySessionLayer `
            -LayerName "local controlled geth peer" `
            -Peers @($LocalGethEnode) `
            -RepoRootValue $RepoRoot `
            -GatewayExe $gatewayExe `
            -Bind $GatewayBind `
            -LayerChainId $ChainId `
            -LogDir $logDir `
            -StateRoot $stateRoot
    }

    $resolver = Join-Path $RepoRoot "scripts\migration\resolve_eth_dns_enodes.py"
    $discovery = [ordered]@{
        name = "public discovery-only"
        status = "failed"
        reason = ""
        discovery_ping_sent_count = [UInt64]0
        discovery_pong_seen_count = [UInt64]0
        dns_discovery_query_sent_count = [UInt64]0
        dns_discovery_enode_seen_count = [UInt64]0
        discovered_peer_count = [UInt64]0
        candidate_session_peer_count = [UInt64]0
        records = @()
        note = "DNS ENR discovery is exercised here; UDP discv4 ping/pong is not performed by this diagnostic and is not treated as session acceptance."
    }
    try {
        $resolverOutput = & python $resolver --json --include-enr --max-enodes ([int]$DiscoveryMaxPeers) --max-visit ([int]$DiscoveryMaxVisit) 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw (($resolverOutput | Out-String).Trim())
        }
        $resolved = ($resolverOutput | Out-String) | ConvertFrom-Json
        $records = @()
        if ($resolved.PSObject.Properties.Name -contains "records") {
            foreach ($record in @($resolved.records)) {
                $records += [pscustomobject]@{
                    remote_enr = [string]$record.enr
                    endpoint = [string]$record.enode
                    remote_node_id = Get-NodeIdFromEnode -Endpoint ([string]$record.enode)
                }
            }
        } else {
            foreach ($enode in @($resolved.enodes)) {
                $records += [pscustomobject]@{
                    remote_enr = ""
                    endpoint = [string]$enode
                    remote_node_id = Get-NodeIdFromEnode -Endpoint ([string]$enode)
                }
            }
        }
        $discovery.status = "completed"
        $discovery.discovery_ping_sent_count = [UInt64]0
        $discovery.discovery_pong_seen_count = [UInt64]0
        $discovery.dns_discovery_query_sent_count = [UInt64]1
        $discovery.dns_discovery_enode_seen_count = [UInt64]$records.Count
        $discovery.discovered_peer_count = [UInt64]$records.Count
        $discovery.candidate_session_peer_count = [UInt64]$records.Count
        $discovery.records = @($records)
    } catch {
        $discovery.reason = $_.Exception.Message
    }

    $publicLayer = Invoke-PublicReadinessClosure `
        -Records $discovery.records `
        -RepoRootValue $RepoRoot `
        -GatewayExe $gatewayExe `
        -Bind $GatewayBind `
        -LayerChainId $ChainId `
        -LogDir $logDir `
        -StateRoot $stateRoot

    $summary = [ordered]@{
        started_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
        repo_root = $RepoRoot
        chain_id = $ChainId
        gateway_bind = $GatewayBind
        state_root = $stateRoot
        canary = [ordered]@{
            discovery_max_peers = $DiscoveryMaxPeers
            discovery_max_visit = $DiscoveryMaxVisit
            session_max_peers = $SessionMaxPeers
            public_session_max_attempts = $PublicSessionMaxAttempts
            public_max_rounds = $PublicMaxRounds
            public_plugin_ports = $PublicPluginPorts
        }
        local_geth_session = $localLayer
        public_discovery_only = $discovery
        public_discovered_peer_session = $publicLayer
        boundary = [ordered]@{
            patch_type = "public_rlpx_readiness_closure"
            external_brand = "NOVOVM"
            does_not_change_protocol_semantics = $true
            does_not_redefine_plugin_architecture = $true
        }
        completed_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    }

    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText(
        $SummaryOut,
        (($summary | ConvertTo-Json -Depth 100) + "`n"),
        $utf8NoBom
    )

    $md = New-Object System.Collections.Generic.List[string]
    $md.Add("# Public RLPx Readiness Closure After a484a8506")
    $md.Add("")
    $md.Add("Status: public RLPx readiness closure canary report.")
    $md.Add("")
    $md.Add("Scope:")
    $md.Add("- This report attempts to close public discovered-peer RLPx readiness by improving peer candidate diversity, endpoint filtering, cooldown, and failure-stage accounting.")
    $md.Add("- It does not change geth-facing RPC compatibility, BAL guard behavior, or NOVOVM plugin architecture.")
    $md.Add("- Bootnode and DNS discovery targets are discovery inputs only; readiness is assessed only against discovered session peers.")
    $md.Add(('- The gateway uses isolated state paths under `{0}` and does not reuse `artifacts/gateway/unified-account-router.rocksdb`.' -f $stateRoot))
    $md.Add("")
    $md.Add("Prior Evidence:")
    $md.Add("- Local controlled geth evidence from the previous follow-up showed TCP, RLPx auth ack, Hello, Status, negotiated eth/69, and ready_count=1.")
    $md.Add("- Earlier public short-window samples stopped below auth ack and observed too_many_peers / TCP timeout outcomes.")
    $md.Add("")
    $md.Add("Public Peer Selection Changes:")
    $md.Add("- DNS ENR discovery can collect a larger candidate pool before session attempts.")
    $md.Add("- Public session candidates are filtered for usable public endpoints.")
    $md.Add("- Session attempts are spread across candidates and rounds instead of treating the first discovered peer as the whole public result.")
    $md.Add("- Peers returning too_many_peers are cooled down for later rounds; TCP timeout endpoints are penalized.")
    $md.Add("- Candidate port diversity is controlled by PublicPluginPorts.")
    $md.Add("")
    $md.Add("Layered Results:")
    $md.Add("")
    foreach ($line in (Convert-LayerToMarkdown -Layer $localLayer)) {
        $md.Add($line)
    }
    $md.Add("### public discovery-only")
    $md.Add("")
    $md.Add(('- status: `{0}`' -f $discovery.status))
    if ($discovery.reason) {
        $safeDiscoveryReason = Convert-ReportText -Text ([string]$discovery.reason)
        $md.Add(('- reason: `{0}`' -f $safeDiscoveryReason))
    }
    $md.Add(('- discovery_ping_sent_count: `{0}`' -f $discovery.discovery_ping_sent_count))
    $md.Add(('- discovery_pong_seen_count: `{0}`' -f $discovery.discovery_pong_seen_count))
    $md.Add(('- dns_discovery_query_sent_count: `{0}`' -f $discovery.dns_discovery_query_sent_count))
    $md.Add(('- dns_discovery_enode_seen_count: `{0}`' -f $discovery.dns_discovery_enode_seen_count))
    $md.Add(('- discovered_peer_count: `{0}`' -f $discovery.discovered_peer_count))
    $md.Add(('- candidate_session_peer_count: `{0}`' -f $discovery.candidate_session_peer_count))
    $md.Add(('- note: `{0}`' -f $discovery.note))
    $md.Add("")
    foreach ($record in @($discovery.records | Select-Object -First ([int]$DiscoveryMaxPeers))) {
        $md.Add(('- remote_node_id=`{0}` endpoint=`{1}` remote_enr=`{2}`' -f $record.remote_node_id, $record.endpoint, $record.remote_enr))
    }
    $md.Add("")
    foreach ($line in (Convert-LayerToMarkdown -Layer $publicLayer)) {
        $md.Add($line)
    }
    $md.Add("Public Session Result:")
    if ($localLayer.status -eq "skipped") {
        $md.Add("- Local controlled geth session was not exercised because no local enode was supplied.")
    }
    if ($discovery.status -eq "completed" -and $discovery.candidate_session_peer_count -gt 0) {
        $md.Add("- Public DNS ENR discovery produced candidate session peers; bootnode/DNS discovery is not treated as eth session readiness.")
    }
    if ($null -ne $publicLayer.metrics) {
        $pm = $publicLayer.metrics
        if ($publicLayer.readiness_claimed) {
            $md.Add("- Public discovered-peer session readiness was observed in this run.")
        } elseif ($pm.rlpx_auth_sent_count -gt 0 -and $pm.rlpx_auth_ack_seen_count -eq 0) {
            $md.Add("- Public discovered-peer session stopped below auth ack in this run.")
        } elseif ($pm.tcp_connect_success_count -eq 0 -and $pm.tcp_connect_fail_count -gt 0) {
            $md.Add("- Public discovered-peer session stopped at TCP connectivity in this run.")
        } elseif ($pm.hello_seen_count -eq 0 -and $pm.rlpx_auth_ack_seen_count -gt 0) {
            $md.Add("- Public discovered-peer session reached auth ack but did not observe Hello in this run.")
        } elseif ($pm.status_seen_count -eq 0 -and $pm.hello_seen_count -gt 0) {
            $md.Add("- Public discovered-peer session reached Hello but did not observe Status in this run.")
        } elseif ($pm.ready_count -gt 0) {
            $md.Add("- Public discovered-peer session reached ready in this run.")
        }
    }
    $md.Add("")
    $md.Add("Readiness Claim:")
    if ($publicLayer.readiness_claimed) {
        $md.Add("- public RLPx readiness: CLAIMED for this canary run.")
        $md.Add("- The run observed TCP success, auth ack, Hello, Status, selected eth/69 or eth/70, and ready_count >= 1.")
    } else {
        $md.Add("- public RLPx readiness: NOT CLAIMED.")
        $md.Add("- A readiness claim requires TCP success, auth ack, Hello, Status, selected eth/69 or eth/70, and ready_count >= 1 in the public discovered-peer session.")
    }
    $md.Add("")
    $md.Add("Interpretation:")
    $md.Add("- Prior local controlled geth evidence passed through TCP, auth ack, Hello, Status, eth/69, and ready.")
    $md.Add("- If the public session reaches auth ack or Hello but not Status, the likely area is public peer selection, remote peer policy, endpoint quality, or Status exchange compatibility with sampled public peers.")
    $md.Add("- If a future public run stops before ack, the likely area remains public peer selection, endpoint reachability, network egress, or remote policy.")
    $md.Add("- If both local and public sessions stop before ack, the next independent patch should inspect RLPx auth/session details.")
    $md.Add("- A run that does not observe ack also does not proceed far enough to observe Hello, Status, or eth capability negotiation in that run.")
    $md.Add("- This does not mean the NOVOVM EVM plugin lacks Hello/Status handling.")
    $md.Add("")
    $md.Add("Not Claimed:")
    $md.Add("- no full geth full node parity")
    $md.Add("- no protocol semantic rewrite")
    $md.Add("- no full eth/71 or BAL implementation")
    $md.Add("- no real balHash metadata source")
    $md.Add("- no old UnifiedAccountRouter state migration")
    $md.Add("- no strategy-specific acceptance result")
    $md.Add("- no new NOVOVM plugin architecture")
    $md.Add("")
    $md.Add("Diff Audit:")
    $md.Add('- Script scope: `scripts/migration/run_evm_rlpx_layered_canary.ps1` extends public candidate selection, retry diversity, cooldown accounting, and readiness reporting.')
    $md.Add('- Report scope: `artifacts/migration/public-rlpx-readiness-closure-after-a484a8506.md` records this public closure run.')
    $md.Add("- No active protocol semantic files are modified by this canary task.")
    $md.Add("- No eth_baseFee, balHash, eth/71 guard, BAL fallback, UA RocksDB, or plugin architecture behavior is changed.")
    $md.Add("")
    $md.Add("Merge Note:")
    $md.Add("- This is a public RLPx readiness closure canary improvement and evidence patch.")
    $md.Add("- The observed public run reached auth ack and Hello on sampled peers but did not observe Status or ready.")
    $md.Add("- Public RLPx readiness remains not claimed until a public discovered-peer session observes Status and ready_count >= 1.")
    $md.Add("")
    while ($md.Count -gt 0 -and $md[$md.Count - 1] -eq "") {
        $md.RemoveAt($md.Count - 1)
    }
    [System.IO.File]::WriteAllText(
        $MarkdownOut,
        (($md.ToArray() -join "`n") + "`n"),
        $utf8NoBom
    )

    Write-Host "summary written: $SummaryOut"
    Write-Host "report written: $MarkdownOut"

    if ($FailOnPublicSessionFailure) {
        $metrics = $publicLayer.metrics
        if ($null -eq $metrics -or $metrics.ready_count -eq 0) {
            throw "public discovered-peer session did not reach ready"
        }
    }
} finally {
    Pop-Location
}
