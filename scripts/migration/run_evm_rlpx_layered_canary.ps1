param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [UInt64]$ChainId = 1,
    [string]$LocalGethEnode = "",
    [string]$RemoteControlledGethEnode = "",
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
    [string]$ReportTitle = "Public RLPx Readiness Closure After a484a8506",
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
        "status_sent" { return 6 }
        "status_seen" { return 7 }
        "ready" { return 8 }
        default { return 0 }
    }
}

function Resolve-GatewayExecutablePath {
    param([string]$Root)
    $targetRoot = ""
    if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
        $targetRoot = Resolve-FullPath -Root $Root -Value $env:CARGO_TARGET_DIR
    } else {
        $targetRoot = Join-Path $Root "target"
    }
    $gatewayExe = Join-Path $targetRoot "debug\novovm-evm-gateway.exe"
    if (-not (Test-Path $gatewayExe)) {
        throw ("gateway binary not found: {0} (CARGO_TARGET_DIR={1})" -f $gatewayExe, $env:CARGO_TARGET_DIR)
    }
    return $gatewayExe
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

function Test-EthCapsContainVersion {
    param(
        [string]$Caps,
        [string[]]$Versions
    )
    if (-not $Caps) {
        return $false
    }
    $parts = @($Caps -split "," | ForEach-Object { $_.Trim() } | Where-Object { $_ })
    foreach ($version in $Versions) {
        if ($parts -contains $version -or $parts -contains ("eth/{0}" -f $version)) {
            return $true
        }
    }
    return $false
}

function Test-SelectedEthVersion {
    param(
        [string]$Selected,
        [string[]]$Versions
    )
    if (-not $Selected) {
        return $false
    }
    $normalized = $Selected.Trim().ToLowerInvariant()
    foreach ($version in $Versions) {
        if ($normalized -eq $version -or $normalized -eq ("eth/{0}" -f $version)) {
            return $true
        }
    }
    return $false
}

function Get-RlpxFailureClass {
    param($Trace)
    if ($null -eq $Trace) {
        return "unknown"
    }
    $reason = [string]$Trace.reason
    $phase = [string]$Trace.disconnect_phase
    $selected = [string]$Trace.selected_eth_capability
    $remoteEthCaps = [string]$Trace.remote_eth_capabilities
    $localStatus = [string]$Trace.local_status_summary
    $remoteStatus = [string]$Trace.remote_status_summary
    $bestStage = [string]$Trace.best_stage

    if ($bestStage -eq "ready") {
        return "ready"
    }
    if ($remoteStatus) {
        return "status_seen_not_ready"
    }
    if ($reason -match "remote_status_decode_failed" -or $phase -eq "remote_status_decode_failed") {
        return "status_payload_mismatch"
    }
    if (Test-SelectedEthVersion -Selected $selected -Versions @("68")) {
        if ($localStatus) {
            return "eth68_only_after_local_status_before_remote_status"
        }
        return "eth68_only_peer"
    }
    if ($reason -match "eth_capability_not_found|no_shared_eth_capability" -or $phase -eq "capability_mismatch") {
        return "capability_mismatch"
    }
    if (Test-ReasonIsTooManyPeers -Reason $reason) {
        if ($phase -eq "after_status_sent_before_status_seen" -or $localStatus) {
            return "too_many_peers_after_local_status_before_remote_status"
        }
        if ($phase -eq "before_eth_status" -or $remoteEthCaps) {
            return "too_many_peers_after_hello_before_local_status"
        }
        return "too_many_peers_before_hello"
    }
    if ($phase -eq "after_status_sent_before_status_seen" -or $localStatus) {
        return "disconnect_after_local_status_before_remote_status"
    }
    if ($phase -eq "before_eth_status" -or ($remoteEthCaps -and -not $localStatus)) {
        return "disconnect_after_hello_before_local_status"
    }
    if ($phase -eq "before_hello" -or $reason -match "before_hello|remote_hello_timeout") {
        return "disconnect_before_hello"
    }
    if ($reason -match "ack|auth" -and -not $remoteEthCaps) {
        return "below_auth_ack"
    }
    if (Test-ReasonIsTcpTimeout -Reason $reason) {
        return "endpoint_timeout"
    }
    if ($reason -match "connect_failed|connection refused|unreachable") {
        return "tcp_connect_failed"
    }
    return "unknown"
}

function Update-RlpxFailureClassification {
    param([System.Collections.IDictionary]$Metrics)
    if ($null -eq $Metrics -or -not $Metrics.Contains("traces")) {
        return
    }
    foreach ($key in @(
        "disconnect_after_hello_before_local_status_count",
        "disconnect_after_local_status_before_remote_status_count",
        "capability_mismatch_count",
        "eth68_only_peer_count",
        "eth69_70_peer_count",
        "status_payload_mismatch_count",
        "endpoint_timeout_count"
    )) {
        $Metrics[$key] = [UInt64]0
    }

    foreach ($trace in @($Metrics.traces)) {
        $class = Get-RlpxFailureClass -Trace $trace
        $trace.failure_class = $class
        if ($class -match "after_hello_before_local_status") {
            $Metrics.disconnect_after_hello_before_local_status_count = [UInt64]($Metrics.disconnect_after_hello_before_local_status_count + 1)
        }
        if ($class -match "after_local_status_before_remote_status") {
            $Metrics.disconnect_after_local_status_before_remote_status_count = [UInt64]($Metrics.disconnect_after_local_status_before_remote_status_count + 1)
        }
        if ($class -eq "capability_mismatch") {
            $Metrics.capability_mismatch_count = [UInt64]($Metrics.capability_mismatch_count + 1)
        }
        if ($class -match "eth68_only") {
            $Metrics.eth68_only_peer_count = [UInt64]($Metrics.eth68_only_peer_count + 1)
        }
        if ((Test-SelectedEthVersion -Selected ([string]$trace.selected_eth_capability) -Versions @("69", "70")) -or
            (Test-EthCapsContainVersion -Caps ([string]$trace.remote_eth_capabilities) -Versions @("69", "70"))) {
            $Metrics.eth69_70_peer_count = [UInt64]($Metrics.eth69_70_peer_count + 1)
        }
        if ($class -eq "status_payload_mismatch") {
            $Metrics.status_payload_mismatch_count = [UInt64]($Metrics.status_payload_mismatch_count + 1)
        }
        if ($class -eq "endpoint_timeout") {
            $Metrics.endpoint_timeout_count = [UInt64]($Metrics.endpoint_timeout_count + 1)
        }
    }
}

function Get-CapabilityVersionsFromSummary {
    param(
        [string]$Caps,
        [string]$Name
    )
    if (-not $Caps) {
        return ""
    }
    $versions = New-Object System.Collections.Generic.List[string]
    foreach ($part in @($Caps -split ",")) {
        $trimmed = $part.Trim()
        if ($trimmed.StartsWith("$Name/")) {
            $versions.Add($trimmed.Substring($Name.Length + 1))
        }
    }
    return ($versions.ToArray() -join ",")
}

function Read-GatewayRlpxLogDiagnostics {
    param([string]$Path)
    $map = @{}
    if (-not $Path -or -not (Test-Path $Path)) {
        return $map
    }
    foreach ($line in (Get-Content $Path -ErrorAction SilentlyContinue)) {
        $hello = [regex]::Match($line, "hello_received endpoint=(\S+) remote_proto=(\d+) remote_name=(\S+) remote_caps=(\S+)")
        if ($hello.Success) {
            $endpoint = $hello.Groups[1].Value
            if (-not $map.ContainsKey($endpoint)) {
                $map[$endpoint] = [ordered]@{}
            }
            $caps = $hello.Groups[4].Value
            $map[$endpoint].remote_client_id = $hello.Groups[3].Value
            $map[$endpoint].remote_p2p_version = $hello.Groups[2].Value
            $map[$endpoint].remote_capabilities = $caps
            $map[$endpoint].remote_eth_capabilities = Get-CapabilityVersionsFromSummary -Caps $caps -Name "eth"
            $map[$endpoint].remote_snap_capabilities = Get-CapabilityVersionsFromSummary -Caps $caps -Name "snap"
            $selected = [regex]::Match($line, "selected_eth=(\d+)")
            if ($selected.Success) {
                $map[$endpoint].selected_eth_capability = ("eth/{0}" -f $selected.Groups[1].Value)
            }
            continue
        }
        $disconnect = [regex]::Match($line, "disconnect_received endpoint=(\S+) phase=(\S+) reason_code=(\d+) reason=(\S+)")
        if ($disconnect.Success) {
            $endpoint = $disconnect.Groups[1].Value
            if (-not $map.ContainsKey($endpoint)) {
                $map[$endpoint] = [ordered]@{}
            }
            $map[$endpoint].disconnect_phase = $disconnect.Groups[2].Value
            $map[$endpoint].disconnect_reason_code = $disconnect.Groups[3].Value
            $map[$endpoint].disconnect_reason_name = $disconnect.Groups[4].Value
            continue
        }
        $statusSent = [regex]::Match($line, "status_sent endpoint=(\S+) local_chain_id=(\d+) local_status=(\S+)")
        if ($statusSent.Success) {
            $endpoint = $statusSent.Groups[1].Value
            if (-not $map.ContainsKey($endpoint)) {
                $map[$endpoint] = [ordered]@{}
            }
            $map[$endpoint].local_status_summary = $statusSent.Groups[3].Value
            continue
        }
        $status = [regex]::Match($line, "status_received endpoint=(\S+) remote_chain_id=(\d+) negotiated_eth=(\d+) remote_status=(\S+)")
        if ($status.Success) {
            $endpoint = $status.Groups[1].Value
            if (-not $map.ContainsKey($endpoint)) {
                $map[$endpoint] = [ordered]@{}
            }
            $map[$endpoint].remote_chain_id = $status.Groups[2].Value
            $map[$endpoint].selected_eth_capability = ("eth/{0}" -f $status.Groups[3].Value)
            $map[$endpoint].remote_status_summary = $status.Groups[4].Value
        }
    }
    return $map
}

function Merge-GatewayRlpxLogDiagnostics {
    param(
        [System.Collections.IDictionary]$Metrics,
        [string]$Path
    )
    if ($null -eq $Metrics -or -not $Metrics.Contains("traces")) {
        return
    }
    $diag = Read-GatewayRlpxLogDiagnostics -Path $Path
    if ($diag.Count -eq 0) {
        return
    }
    $beforeHello = [UInt64]0
    $beforeStatus = [UInt64]0
    $afterStatusSent = [UInt64]0
    $helloSeen = [UInt64]0
    $statusSent = [UInt64]0
    $statusSeen = [UInt64]0
    foreach ($trace in @($Metrics.traces)) {
        $endpoint = [string]$trace.endpoint
        if (-not $endpoint -or -not $diag.ContainsKey($endpoint)) {
            continue
        }
        $entry = $diag[$endpoint]
        foreach ($name in @("remote_client_id", "remote_eth_capabilities", "remote_snap_capabilities", "remote_capabilities", "disconnect_phase", "disconnect_reason_name", "local_status_summary", "remote_status_summary")) {
            if ($entry.Contains($name) -and -not $trace.$name) {
                $trace.$name = [string]$entry[$name]
            }
        }
        if ($entry.Contains("selected_eth_capability") -and $trace.selected_eth_capability -eq "none") {
            $trace.selected_eth_capability = [string]$entry.selected_eth_capability
        }
    }
    foreach ($trace in @($Metrics.traces)) {
        if ($trace.remote_client_id -or $trace.remote_eth_capabilities) {
            $helloSeen = [UInt64]($helloSeen + 1)
        }
        if ($trace.local_status_summary) {
            $statusSent = [UInt64]($statusSent + 1)
        }
        if ($trace.remote_status_summary) {
            $statusSeen = [UInt64]($statusSeen + 1)
        }
        if ($trace.disconnect_phase -eq "before_hello") {
            $beforeHello = [UInt64]($beforeHello + 1)
        } elseif ($trace.disconnect_phase -eq "before_eth_status") {
            $beforeStatus = [UInt64]($beforeStatus + 1)
        } elseif ($trace.disconnect_phase -eq "after_status_sent_before_status_seen") {
            $afterStatusSent = [UInt64]($afterStatusSent + 1)
        }
    }
    $Metrics.disconnect_before_hello_count = [UInt64][Math]::Max([UInt64]$Metrics.disconnect_before_hello_count, $beforeHello)
    $Metrics.disconnect_before_status_count = [UInt64][Math]::Max([UInt64]$Metrics.disconnect_before_status_count, $beforeStatus)
    $Metrics.disconnect_after_status_sent_count = [UInt64][Math]::Max([UInt64]$Metrics.disconnect_after_status_sent_count, $afterStatusSent)
    $Metrics.hello_seen_count = [UInt64][Math]::Max([UInt64]$Metrics.hello_seen_count, $helloSeen)
    $Metrics.status_sent_count = [UInt64][Math]::Max([UInt64]$Metrics.status_sent_count, $statusSent)
    $Metrics.status_seen_count = [UInt64][Math]::Max([UInt64]$Metrics.status_seen_count, $statusSeen)
    Update-RlpxFailureClassification -Metrics $Metrics
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
        disconnect_before_hello_count = [UInt64]0
        disconnect_before_status_count = [UInt64]0
        disconnect_after_status_sent_count = [UInt64]0
        disconnect_after_hello_before_local_status_count = [UInt64]0
        disconnect_after_local_status_before_remote_status_count = [UInt64]0
        capability_mismatch_count = [UInt64]0
        eth68_only_peer_count = [UInt64]0
        eth69_70_peer_count = [UInt64]0
        status_payload_mismatch_count = [UInt64]0
        endpoint_timeout_count = [UInt64]0
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
        $remoteClientId = Get-StringField -Obj $item -Name "remote_client_id"
        $remoteEthCaps = Get-StringField -Obj $item -Name "remote_eth_capabilities"
        $remoteSnapCaps = Get-StringField -Obj $item -Name "remote_snap_capabilities"
        $remoteCaps = Get-StringField -Obj $item -Name "remote_capabilities"
        $helloSeenElapsedMs = Get-StringField -Obj $item -Name "hello_seen_elapsed_ms"
        $statusSeenElapsedMs = Get-StringField -Obj $item -Name "status_seen_elapsed_ms"
        $statusSentElapsedMs = Get-StringField -Obj $item -Name "status_sent_elapsed_ms"
        $localStatusSummary = Get-StringField -Obj $item -Name "local_status_summary"
        $remoteStatusSummary = Get-StringField -Obj $item -Name "remote_status_summary"
        $disconnectPhase = Get-StringField -Obj $item -Name "disconnect_phase"
        $disconnectReasonName = Get-StringField -Obj $item -Name "disconnect_reason_name"
        $disconnectElapsedMs = Get-StringField -Obj $item -Name "disconnect_elapsed_ms"
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
        if ($disconnectPhase -eq "before_hello" -or $lastError -match "before_hello|remote_hello_timeout") {
            $metrics.disconnect_before_hello_count = [UInt64]($metrics.disconnect_before_hello_count + 1)
        }
        if ($disconnectPhase -eq "before_eth_status" -or (($disconnectPhase -ne "after_status_sent_before_status_seen") -and $lastError -match "before_eth_status|eth_status_timeout|eth_capability_not_found")) {
            $metrics.disconnect_before_status_count = [UInt64]($metrics.disconnect_before_status_count + 1)
        }
        if ($disconnectPhase -eq "after_status_sent_before_status_seen") {
            $metrics.disconnect_after_status_sent_count = [UInt64]($metrics.disconnect_after_status_sent_count + 1)
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
            remote_client_id = $remoteClientId
            remote_eth_capabilities = $remoteEthCaps
            remote_snap_capabilities = $remoteSnapCaps
            remote_capabilities = $remoteCaps
            hello_seen_elapsed_ms = $helloSeenElapsedMs
            status_sent_elapsed_ms = $statusSentElapsedMs
            status_seen_elapsed_ms = $statusSeenElapsedMs
            local_status_summary = $localStatusSummary
            remote_status_summary = $remoteStatusSummary
            failure_class = "unknown"
            disconnect_phase = $disconnectPhase
            disconnect_reason_name = $disconnectReasonName
            disconnect_elapsed_ms = $disconnectElapsedMs
        }
    }
    Update-RlpxFailureClassification -Metrics $metrics
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
        disconnect_before_hello_count = [UInt64]0
        disconnect_before_status_count = [UInt64]0
        disconnect_after_status_sent_count = [UInt64]0
        disconnect_after_hello_before_local_status_count = [UInt64]0
        disconnect_after_local_status_before_remote_status_count = [UInt64]0
        capability_mismatch_count = [UInt64]0
        eth68_only_peer_count = [UInt64]0
        eth69_70_peer_count = [UInt64]0
        status_payload_mismatch_count = [UInt64]0
        endpoint_timeout_count = [UInt64]0
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
        "disconnect_before_hello_count",
        "disconnect_before_status_count",
        "disconnect_after_status_sent_count",
        "disconnect_after_hello_before_local_status_count",
        "disconnect_after_local_status_before_remote_status_count",
        "capability_mismatch_count",
        "eth68_only_peer_count",
        "eth69_70_peer_count",
        "status_payload_mismatch_count",
        "endpoint_timeout_count",
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
    if ($null -ne $layer.metrics) {
        Merge-GatewayRlpxLogDiagnostics -Metrics $layer.metrics -Path $gwErr
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
                if ($endpoint -and (Test-SelectedEthVersion -Selected ([string]$trace.selected_eth_capability) -Versions @("68"))) {
                    [void]$cooldown.Add($endpoint)
                    $nodeId = [string]$trace.remote_node_id
                    if ($nodeId) {
                        [void]$cooldownNodes.Add($nodeId)
                    }
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
        if ($m.Contains("disconnect_before_hello_count")) {
            $lines.Add(('- disconnect_before_hello_count: `{0}`' -f $m.disconnect_before_hello_count))
        }
        if ($m.Contains("disconnect_before_status_count")) {
            $lines.Add(('- disconnect_before_status_count: `{0}`' -f $m.disconnect_before_status_count))
        }
        if ($m.Contains("disconnect_after_status_sent_count")) {
            $lines.Add(('- disconnect_after_status_sent_count: `{0}`' -f $m.disconnect_after_status_sent_count))
        }
        if ($m.Contains("disconnect_after_hello_before_local_status_count")) {
            $lines.Add(('- disconnect_after_hello_before_local_status_count: `{0}`' -f $m.disconnect_after_hello_before_local_status_count))
        }
        if ($m.Contains("disconnect_after_local_status_before_remote_status_count")) {
            $lines.Add(('- disconnect_after_local_status_before_remote_status_count: `{0}`' -f $m.disconnect_after_local_status_before_remote_status_count))
        }
        if ($m.Contains("capability_mismatch_count")) {
            $lines.Add(('- capability_mismatch_count: `{0}`' -f $m.capability_mismatch_count))
        }
        if ($m.Contains("eth68_only_peer_count")) {
            $lines.Add(('- eth68_only_peer_count: `{0}`' -f $m.eth68_only_peer_count))
        }
        if ($m.Contains("eth69_70_peer_count")) {
            $lines.Add(('- eth69_70_peer_count: `{0}`' -f $m.eth69_70_peer_count))
        }
        if ($m.Contains("status_payload_mismatch_count")) {
            $lines.Add(('- status_payload_mismatch_count: `{0}`' -f $m.status_payload_mismatch_count))
        }
        if ($m.Contains("endpoint_timeout_count")) {
            $lines.Add(('- endpoint_timeout_count: `{0}`' -f $m.endpoint_timeout_count))
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
            $traceClient = Convert-ReportText -Text ([string]$trace.remote_client_id)
            $localStatus = Convert-ReportText -Text ([string]$trace.local_status_summary)
            $remoteStatus = Convert-ReportText -Text ([string]$trace.remote_status_summary)
            $failureClass = Convert-ReportText -Text ([string]$trace.failure_class)
            $lines.Add(('- peer=`{0}` endpoint=`{1}` stage=`{2}` best=`{3}` class=`{4}` phase=`{5}` reason=`{6}` cap=`{7}` client=`{8}` eth_caps=`{9}` snap_caps=`{10}` hello_ms=`{11}` status_sent_ms=`{12}` status_seen_ms=`{13}` disconnect_ms=`{14}` local_status=`{15}` remote_status=`{16}`' -f $trace.remote_node_id, $trace.remote_endpoint, $trace.stage, $trace.best_stage, $failureClass, $trace.disconnect_phase, $traceReason, $trace.selected_eth_capability, $traceClient, $trace.remote_eth_capabilities, $trace.remote_snap_capabilities, $trace.hello_seen_elapsed_ms, $trace.status_sent_elapsed_ms, $trace.status_seen_elapsed_ms, $trace.disconnect_elapsed_ms, $localStatus, $remoteStatus))
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

    $gatewayExe = Resolve-GatewayExecutablePath -Root $RepoRoot
    Write-Host "gateway executable: $gatewayExe"

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

    $remoteControlledLayer = if ([string]::IsNullOrWhiteSpace($RemoteControlledGethEnode)) {
        [pscustomobject][ordered]@{
            name = "remote controlled geth peer"
            status = "skipped"
            reason = "RemoteControlledGethEnode was not supplied; this diagnostic does not exercise a controlled geth peer over a public network path"
            peers = @()
            metrics = $null
            capability = $null
            plugin_peers = $null
            gateway_stdout = ""
            gateway_stderr = ""
            readiness_claimed = $false
        }
    } else {
        Invoke-GatewaySessionLayer `
            -LayerName "remote controlled geth peer" `
            -Peers @($RemoteControlledGethEnode) `
            -RepoRootValue $RepoRoot `
            -GatewayExe $gatewayExe `
            -Bind $GatewayBind `
            -LayerChainId $ChainId `
            -LogDir $logDir `
            -StateRoot $stateRoot
    }
    if ($null -ne $remoteControlledLayer.metrics) {
        $rm = $remoteControlledLayer.metrics
        $remoteControlledLayer | Add-Member -NotePropertyName readiness_claimed -NotePropertyValue (
            $rm.ready_count -gt 0 -and
            $rm.rlpx_auth_ack_seen_count -gt 0 -and
            $rm.hello_seen_count -gt 0 -and
            $rm.status_seen_count -gt 0 -and
            (Test-SelectedEthVersion -Selected ([string]$rm.selected_eth_capability) -Versions @("69", "70"))
        ) -Force
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
        gateway_executable = $gatewayExe
        state_root = $stateRoot
        canary = [ordered]@{
            discovery_max_peers = $DiscoveryMaxPeers
            discovery_max_visit = $DiscoveryMaxVisit
            session_max_peers = $SessionMaxPeers
            public_session_max_attempts = $PublicSessionMaxAttempts
            public_max_rounds = $PublicMaxRounds
            public_plugin_ports = $PublicPluginPorts
            remote_controlled_geth_enode_supplied = (-not [string]::IsNullOrWhiteSpace($RemoteControlledGethEnode))
        }
        local_geth_session = $localLayer
        remote_controlled_geth_session = $remoteControlledLayer
        public_discovery_only = $discovery
        public_discovered_peer_session = $publicLayer
        boundary = [ordered]@{
            patch_type = "public_rlpx_readiness_closure"
            external_brand = "NOVOVM"
            rlpx_status_timing_changed = $true
            does_not_change_evm_execution_semantics = $true
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
    $md.Add(("# {0}" -f $ReportTitle))
    $md.Add("")
    $md.Add("Status: public RLPx readiness/status failure classification report.")
    $md.Add("")
    $md.Add("Scope:")
    $md.Add("- This report records public discovered-peer RLPx readiness progress and the current failure class by using peer candidate diversity, endpoint filtering, cooldown, and failure-stage accounting.")
    $md.Add("- It does not change geth-facing RPC compatibility, BAL guard behavior, or NOVOVM plugin architecture.")
    $md.Add("- Bootnode and DNS discovery targets are discovery inputs only; readiness is assessed only against discovered session peers.")
    $md.Add(('- Gateway executable: `{0}`.' -f $gatewayExe))
    $md.Add(('- The gateway uses isolated state paths under `{0}` and does not reuse `artifacts/gateway/unified-account-router.rocksdb`.' -f $stateRoot))
    $md.Add("")
    $md.Add("Prior Evidence:")
    $md.Add("- Local controlled geth evidence from the previous follow-up showed TCP, RLPx auth ack, Hello, Status, negotiated eth/69, and ready_count=1.")
    $md.Add("- Earlier public short-window samples stopped below auth ack and observed too_many_peers / TCP timeout outcomes.")
    $md.Add("- RemoteControlledGethEnode is a controlled public-network comparison point; it is reported separately from random public discovered-peer readiness.")
    $md.Add("")
    $md.Add("Public Peer Selection Changes:")
    $md.Add("- DNS ENR discovery can collect a larger candidate pool before session attempts.")
    $md.Add("- Public session candidates are filtered for usable public endpoints.")
    $md.Add("- Session attempts are spread across candidates and rounds instead of treating the first discovered peer as the whole public result.")
    $md.Add("- Peers returning too_many_peers are cooled down for later rounds; TCP timeout endpoints are penalized.")
    $md.Add("- eth/68-only Hello samples are classified separately and cooled down so eth/69 or eth/70 peers remain the readiness target.")
    $md.Add("- Candidate port diversity is controlled by PublicPluginPorts.")
    $md.Add("")
    $md.Add("Layered Results:")
    $md.Add("")
    foreach ($line in (Convert-LayerToMarkdown -Layer $localLayer)) {
        $md.Add($line)
    }
    foreach ($line in (Convert-LayerToMarkdown -Layer $remoteControlledLayer)) {
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
    if ($remoteControlledLayer.status -eq "skipped") {
        $md.Add("- Remote controlled geth session was not exercised because no remote controlled enode was supplied.")
    } elseif ($remoteControlledLayer.readiness_claimed) {
        $md.Add("- Remote controlled geth session readiness was observed in this run.")
    } elseif ($null -ne $remoteControlledLayer.metrics) {
        $md.Add("- Remote controlled geth session did not reach the readiness standard in this run.")
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
    if ($null -ne $publicLayer.metrics) {
        $pm = $publicLayer.metrics
        $md.Add("Status Gap Diagnostics:")
        $md.Add(('- disconnect_before_hello_count: `{0}`' -f $pm.disconnect_before_hello_count))
        $md.Add(('- disconnect_before_status_count: `{0}`' -f $pm.disconnect_before_status_count))
        $md.Add(('- disconnect_after_status_sent_count: `{0}`' -f $pm.disconnect_after_status_sent_count))
        $md.Add(('- disconnect_after_hello_before_local_status_count: `{0}`' -f $pm.disconnect_after_hello_before_local_status_count))
        $md.Add(('- disconnect_after_local_status_before_remote_status_count: `{0}`' -f $pm.disconnect_after_local_status_before_remote_status_count))
        $md.Add(('- capability_mismatch_count: `{0}`' -f $pm.capability_mismatch_count))
        $md.Add(('- eth68_only_peer_count: `{0}`' -f $pm.eth68_only_peer_count))
        $md.Add(('- eth69_70_peer_count: `{0}`' -f $pm.eth69_70_peer_count))
        $md.Add(('- status_payload_mismatch_count: `{0}`' -f $pm.status_payload_mismatch_count))
        $md.Add(('- endpoint_timeout_count: `{0}`' -f $pm.endpoint_timeout_count))
        $md.Add(('- hello_seen_count: `{0}`' -f $pm.hello_seen_count))
        $md.Add(('- status_sent_count: `{0}`' -f $pm.status_sent_count))
        $md.Add(('- status_seen_count: `{0}`' -f $pm.status_seen_count))
        if ($pm.hello_seen_count -gt 0 -and $pm.status_seen_count -eq 0) {
            $md.Add("- The sampled public run observed at least one remote Hello but did not observe remote Status.")
        }
        if ($pm.eth68_only_peer_count -gt 0 -and $pm.eth69_70_peer_count -eq 0) {
            $md.Add("- Observed Hello samples were eth/68-only and are separated from the eth/69 or eth/70 readiness target.")
        }
        $md.Add("- Per-peer compact traces include remote client, eth/snap capability hints, disconnect phase, and elapsed timing when the gateway observed them.")
        $md.Add("")
    }
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
    $md.Add("- A remote controlled geth pass would demonstrate that the gateway RLPx path can traverse a public network path to a known geth peer; random public discovered-peer readiness remains a separate claim.")
    $md.Add("- If the public session reaches auth ack or Hello but not Status, the likely area is public peer selection, remote peer policy, endpoint quality, or Status exchange compatibility with sampled public peers.")
    $md.Add("- If a future public run stops before ack, the likely area remains public peer selection, endpoint reachability, network egress, or remote policy.")
    $md.Add("- If both local and public sessions stop before ack, the next independent patch should inspect RLPx auth/session details.")
    $md.Add("- A run that does not observe ack also does not proceed far enough to observe Hello, Status, or eth capability negotiation in that run.")
    $md.Add("- This does not mean the NOVOVM EVM plugin lacks Hello/Status handling.")
    $md.Add("")
    $md.Add("Not Claimed:")
    $md.Add("- no full geth full node parity")
    $md.Add("- no EVM execution semantic rewrite")
    $md.Add("- no full eth/71 or BAL implementation")
    $md.Add("- no real balHash metadata source")
    $md.Add("- no public random-peer readiness unless separately observed")
    $md.Add("- no old UnifiedAccountRouter state migration")
    $md.Add("- no strategy-specific acceptance result")
    $md.Add("- no new NOVOVM plugin architecture")
    $md.Add("")
    $reportScope = [string]$MarkdownOut
    $repoRootForReport = (Get-Location).Path
    if ($reportScope) {
        $reportScope = $reportScope -replace ('^' + [regex]::Escape($repoRootForReport) + '[\\/]*'), ''
    }
    $md.Add("Diff Audit:")
    $md.Add('- Rust scope: no new Rust changes are required for this follow-up; the report reuses the gateway RLPx Status diagnostics added by the prior Status exchange diagnostics patches.')
    $md.Add('- Script scope: `scripts/migration/run_evm_rlpx_layered_canary.ps1` adds a RemoteControlledGethEnode comparison layer and reports public Status exchange failure classes.')
    $md.Add(('- Report scope: `{0}` records this public Status failure classification canary run.' -f $reportScope))
    $md.Add("- RLPx Status timing is not changed by this follow-up.")
    $md.Add("- No eth_baseFee, balHash, eth/71 guard, BAL fallback, UA RocksDB, or plugin architecture behavior is changed.")
    $md.Add("")
    $md.Add("Merge Note:")
    $md.Add("- This is a public RLPx Status exchange failure classification and remote controlled geth evidence patch.")
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
