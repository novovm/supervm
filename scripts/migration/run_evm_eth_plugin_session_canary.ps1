param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [UInt64]$ChainId = 1,
    [string]$PluginPorts = "30303,30304",
    [string]$NativePeers = "",
    [UInt64]$ProbeTimeoutMs = 8000,
    [UInt64]$ProbeCacheTtlMs = 12000,
    [UInt64]$WarmupSeconds = 3,
    [UInt64]$ProbeSeconds = 6,
    [UInt64]$ProbeRounds = 5,
    [string]$SummaryOut = "artifacts/migration/evm-eth-plugin-session-canary-summary.json",
    [switch]$SkipBuild
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
    param([string]$Raw)
    if (-not $Raw) {
        return [UInt64]0
    }
    $trimmed = $Raw.Trim()
    if ($trimmed.StartsWith("0x") -or $trimmed.StartsWith("0X")) {
        return [Convert]::ToUInt64($trimmed.Substring(2), 16)
    }
    return [UInt64]$trimmed
}

function Push-ProcessEnv {
    param([hashtable]$Environment)
    $state = @{}
    if ($null -eq $Environment) {
        return $state
    }
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

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$SummaryOut = Resolve-FullPath -Root $RepoRoot -Value $SummaryOut
$SummaryDir = Split-Path -Parent $SummaryOut
if ($SummaryDir) {
    New-Item -ItemType Directory -Force -Path $SummaryDir | Out-Null
}

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
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$runTag = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$gwOut = Join-Path $logDir "evm-eth-plugin-session-canary-gateway.stdout.log"
$gwErr = Join-Path $logDir "evm-eth-plugin-session-canary-gateway.stderr.log"
if (Test-Path $gwOut) {
    try {
        Remove-Item -Force -ErrorAction Stop $gwOut
    } catch {
        $gwOut = Join-Path $logDir ("evm-eth-plugin-session-canary-gateway.stdout.{0}.log" -f $runTag)
        Write-Warning ("stdout log is busy; using timestamped log path: {0}" -f $gwOut)
    }
}
if (Test-Path $gwErr) {
    try {
        Remove-Item -Force -ErrorAction Stop $gwErr
    } catch {
        $gwErr = Join-Path $logDir ("evm-eth-plugin-session-canary-gateway.stderr.{0}.log" -f $runTag)
        Write-Warning ("stderr log is busy; using timestamped log path: {0}" -f $gwErr)
    }
}

$envMap = @{
    "NOVOVM_GATEWAY_BIND" = $GatewayBind
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ROUTE_POLICY" = "plugin_only"
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PORTS" = $PluginPorts
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PROBE_TIMEOUT_MS" = ([string]$ProbeTimeoutMs)
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PROBE_CACHE_TTL_MS" = ([string]$ProbeCacheTtlMs)
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_SESSION_PROBE_MODE" = "enode"
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS" = $NativePeers
}
if ([string]::IsNullOrWhiteSpace($NativePeers)) {
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES"] = "1"
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_MIN_CANDIDATES"] = "4"
} else {
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES"] = "0"
}

$summary = [ordered]@{
    started_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    gateway_bind = $GatewayBind
    chain_id = $ChainId
    plugin_ports = $PluginPorts
    native_peers = $NativePeers
    probe_timeout_ms = $ProbeTimeoutMs
    probe_cache_ttl_ms = $ProbeCacheTtlMs
    warmup_seconds = $WarmupSeconds
    probe_seconds = $ProbeSeconds
    probe_rounds = $ProbeRounds
    pass_connectivity = $false
    ready_count = 0
    ack_seen_count = 0
    auth_sent_count = 0
    disconnected_count = 0
    total = 0
    reachable = 0
    plugin_peer_source = ""
    capability = $null
    plugin_peers = $null
    gateway_stdout = $gwOut
    gateway_stderr = $gwErr
}

$gatewayProc = $null
try {
    $envState = Push-ProcessEnv -Environment $envMap
    try {
        $gatewayProc = Start-Process `
            -FilePath $gatewayExe `
            -WorkingDirectory $RepoRoot `
            -RedirectStandardOutput $gwOut `
            -RedirectStandardError $gwErr `
            -PassThru `
            -NoNewWindow
    } finally {
        Pop-ProcessEnv -State $envState
    }

    Start-Sleep -Seconds ([int][Math]::Max(1, $WarmupSeconds))
    if ($gatewayProc.HasExited) {
        throw "gateway exited early"
    }

    $url = "http://$GatewayBind"

    # Prime once and then wait for probe loop.
    $null = Invoke-JsonRpc -Url $url -Method "evm_getPublicBroadcastCapability" -Params @{ chain_id = [UInt64]$ChainId }
    $rounds = [int][Math]::Max(1, $ProbeRounds)
    for ($round = 1; $round -le $rounds; $round++) {
        Start-Sleep -Seconds ([int][Math]::Max(1, $ProbeSeconds))
        $peersResp = Invoke-JsonRpc -Url $url -Method "evm_getPublicBroadcastPluginPeers" -Params @{ chain_id = [UInt64]$ChainId }
        $capResp = Invoke-JsonRpc -Url $url -Method "evm_getPublicBroadcastCapability" -Params @{ chain_id = [UInt64]$ChainId }

        $summary.plugin_peers = $peersResp
        $summary.capability = $capResp

        $result = $peersResp.result
        $summary.total = Parse-HexU64 -Raw ([string]$result.total)
        $summary.reachable = Parse-HexU64 -Raw ([string]$result.reachable)
        $summary.plugin_peer_source = [string]$result.peer_source

        $ready = 0
        $ack = 0
        $auth = 0
        $disconnected = 0
        foreach ($item in $result.items) {
            $stage = [string]$item.stage
            switch ($stage) {
                "ready" { $ready += 1 }
                "ack_seen" { $ack += 1 }
                "auth_sent" { $auth += 1 }
                "disconnected" { $disconnected += 1 }
            }
        }
        $summary.ready_count = $ready
        $summary.ack_seen_count = $ack
        $summary.auth_sent_count = $auth
        $summary.disconnected_count = $disconnected

        if ($ready -gt 0 -or $ack -gt 0) {
            $summary.pass_connectivity = $true
            break
        }
    }
    if (-not $summary.pass_connectivity) {
        throw "plugin session canary failed: no ready/ack peer"
    }
}
finally {
    if ($null -ne $gatewayProc -and -not $gatewayProc.HasExited) {
        try {
            Stop-Process -Id $gatewayProc.Id -Force -ErrorAction SilentlyContinue
        } catch {
        }
    }
    $summary.completed_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    $summaryJson = $summary | ConvertTo-Json -Depth 80
    Set-Content -Path $SummaryOut -Value $summaryJson -Encoding UTF8
    Write-Host "summary written: $SummaryOut"
}

if (-not $summary.pass_connectivity) {
    throw "evm eth plugin session canary failed; inspect summary: $SummaryOut"
}

Write-Host "evm eth plugin session canary ok"
