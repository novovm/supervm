param(
    [string]$GatewayUrl = "http://127.0.0.1:9899",
    [string]$GethUrl = "http://127.0.0.1:8545",
    [UInt64]$ChainId = 1,
    [UInt64]$IntervalMs = 1500,
    [switch]$Once
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

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
    if ($null -eq $resp) {
        throw ("{0} failed: empty response" -f $Method)
    }
    if (($resp.PSObject.Properties.Name -contains "error") -and $null -ne $resp.error) {
        throw ("{0} failed: code={1} message={2}" -f $Method, $resp.error.code, $resp.error.message)
    }
    if (-not ($resp.PSObject.Properties.Name -contains "result")) {
        throw ("{0} failed: response missing result" -f $Method)
    }
    return $resp.result
}

function Resolve-PeerStage {
    param([psobject]$Peer)
    if ($null -ne $Peer.protocols -and $Peer.protocols.PSObject.Properties.Name -contains "eth") {
        return "ready"
    }
    if ($null -ne $Peer.protocols -and $Peer.protocols.PSObject.Properties.Count -gt 0) {
        return "ack_seen"
    }
    if ($null -ne $Peer.network) {
        if (($Peer.network.PSObject.Properties.Name -contains "connected") -and $Peer.network.connected) {
            return "tcp_connected"
        }
        if (($Peer.network.PSObject.Properties.Name -contains "inbound") -and $Peer.network.inbound) {
            return "tcp_connected"
        }
        if (($Peer.network.PSObject.Properties.Name -contains "trusted") -and $Peer.network.trusted) {
            return "tcp_connected"
        }
        if (($Peer.network.PSObject.Properties.Name -contains "static") -and $Peer.network.static) {
            return "tcp_connected"
        }
    }
    return "disconnected"
}

function Convert-GethPeerToSession {
    param([psobject]$Peer)
    $endpoint = ""
    if ($Peer.PSObject.Properties.Name -contains "enode" -and $Peer.enode) {
        $endpoint = [string]$Peer.enode
    } elseif ($Peer.PSObject.Properties.Name -contains "network" -and $null -ne $Peer.network) {
        if ($Peer.network.PSObject.Properties.Name -contains "remoteAddress" -and $Peer.network.remoteAddress) {
            $endpoint = ("enode://unknown@{0}" -f [string]$Peer.network.remoteAddress)
        }
    }
    if (-not $endpoint) {
        return $null
    }
    return @{
        endpoint = $endpoint
        stage = Resolve-PeerStage -Peer $Peer
        updated_ms = [UInt64]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
    }
}

Write-Host ("[bridge] gateway={0} geth={1} chain_id={2}" -f $GatewayUrl, $GethUrl, $ChainId)
while ($true) {
    $peers = Invoke-JsonRpc -Url $GethUrl -Method "admin_peers" -Params @()
    $sessions = @()
    foreach ($peer in $peers) {
        $session = Convert-GethPeerToSession -Peer $peer
        if ($null -ne $session) {
            $sessions += $session
        }
    }

    $report = @{
        chain_id = [UInt64]$ChainId
        sessions = $sessions
    }
    $ingest = Invoke-JsonRpc -Url $GatewayUrl -Method "evm_reportPublicBroadcastPluginSession" -Params $report
    $peerView = Invoke-JsonRpc -Url $GatewayUrl -Method "evm_getPublicBroadcastPluginPeers" -Params @{
        chain_id = [UInt64]$ChainId
    }

    Write-Host (
        "[bridge] sessions={0} applied={1} reachable={2}" -f
        $sessions.Count,
        $ingest.applied,
        $peerView.reachable
    )

    if ($Once) {
        break
    }
    Start-Sleep -Milliseconds ([int][Math]::Max(200, $IntervalMs))
}
