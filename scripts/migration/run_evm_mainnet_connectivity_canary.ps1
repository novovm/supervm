param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [string]$UpstreamRpc = "",
    [string]$UcaId = "",
    [string]$FromAddress = "0xD5575458f801ea6fc180B8AC05C14324757eF239",
    [string]$RawTx = "0x02f86b0180843b9aca008506fc23ac0082520894000000000000000000000000000000000000dead8080c080a02eb155703830e47b3351dbded9067b704d7f93d8fc0ab7e55daca18b04927dd9a0395904520690be84d0d170972f4a48b3cde2637d440600d24256b803ca4091fc",
    [UInt64]$ChainId = 1,
    [UInt64]$UpstreamTimeoutMs = 20000,
    [string]$SummaryOut = "artifacts/migration/evm-mainnet-connectivity-canary-summary.json",
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

function Resolve-BinaryPath {
    param(
        [string]$TargetRoot,
        [string]$BinaryBaseName
    )
    $isWindowsOs = $env:OS -eq "Windows_NT"
    $exeName = if ($isWindowsOs) { "$BinaryBaseName.exe" } else { $BinaryBaseName }
    return (Join-Path $TargetRoot "debug\$exeName")
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

if (-not $UpstreamRpc) {
    $UpstreamRpc = $env:NOVOVM_GATEWAY_ETH_UPSTREAM_RPC
}
if (-not $UpstreamRpc) {
    throw "missing UpstreamRpc: pass -UpstreamRpc or set NOVOVM_GATEWAY_ETH_UPSTREAM_RPC"
}
if (-not $UcaId) {
    $UcaId = "uca-mainnet-connectivity-{0:yyyyMMddHHmmssfff}" -f (Get-Date)
}

$TargetRoot = if ($env:CARGO_TARGET_DIR) {
    [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
} else {
    Join-Path $RepoRoot "target"
}
if (-not $SkipBuild) {
    & cargo build -p novovm-evm-gateway
    if ($LASTEXITCODE -ne 0) {
        throw "build failed: novovm-evm-gateway"
    }
}
$GatewayExe = Resolve-BinaryPath -TargetRoot $TargetRoot -BinaryBaseName "novovm-evm-gateway"
if (-not (Test-Path $GatewayExe)) {
    throw "gateway binary not found: $GatewayExe"
}

$logDir = Resolve-FullPath -Root $RepoRoot -Value "artifacts/migration/logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$gwOut = Join-Path $logDir "evm-mainnet-connectivity-gateway.stdout.log"
$gwErr = Join-Path $logDir "evm-mainnet-connectivity-gateway.stderr.log"
if (Test-Path $gwOut) { Remove-Item -Force $gwOut }
if (Test-Path $gwErr) { Remove-Item -Force $gwErr }

$chainHex = ("0x{0:x}" -f $ChainId)
$envMap = @{
    "NOVOVM_GATEWAY_BIND" = $GatewayBind
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC" = $UpstreamRpc
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC" = $UpstreamRpc
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_TIMEOUT_MS" = ([string]$UpstreamTimeoutMs)
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC_TIMEOUT_MS" = ([string]$UpstreamTimeoutMs)
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_CHAIN_$ChainId" = $UpstreamRpc
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_CHAIN_$chainHex" = $UpstreamRpc
}

$envState = Push-ProcessEnv -Environment $envMap
try {
    $gatewayProc = Start-Process `
        -FilePath $GatewayExe `
        -WorkingDirectory $RepoRoot `
        -RedirectStandardOutput $gwOut `
        -RedirectStandardError $gwErr `
        -PassThru `
        -NoNewWindow
} finally {
    Pop-ProcessEnv -State $envState
}

$summary = [ordered]@{
    started_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    gateway_bind = $GatewayBind
    chain_id = $ChainId
    upstream_rpc = $UpstreamRpc
    uca_id = $UcaId
    uca_id_effective = $UcaId
    from_address = $FromAddress
    raw_tx = $RawTx
    chain_id_result = $null
    gateway_send = $null
    gateway_send_retry = $null
    tx_status = $null
    upstream_direct_send = $null
    pass_connectivity = $false
    gateway_stdout = $gwOut
    gateway_stderr = $gwErr
}

try {
    Start-Sleep -Milliseconds 1800
    if ($gatewayProc.HasExited) {
        throw "gateway exited early"
    }

    $url = "http://$GatewayBind"

    $chainReq = @{
        jsonrpc = "2.0"
        id = 1
        method = "eth_chainId"
        params = @()
    } | ConvertTo-Json -Depth 6 -Compress
    $chainResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $chainReq
    $summary.chain_id_result = $chainResp

    $createReq = @{
        jsonrpc = "2.0"
        id = 2
        method = "ua_createUca"
        params = @{
            uca_id = $UcaId
        }
    } | ConvertTo-Json -Depth 10 -Compress
    $null = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $createReq

    $bindReq = @{
        jsonrpc = "2.0"
        id = 3
        method = "ua_bindPersona"
        params = @{
            uca_id = $UcaId
            persona_type = "evm"
            chain_id = [UInt64]$ChainId
            external_address = $FromAddress
        }
    } | ConvertTo-Json -Depth 10 -Compress
    $null = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $bindReq

    $sendReq = @{
        jsonrpc = "2.0"
        id = 4
        method = "eth_sendRawTransaction"
        params = @{
            uca_id = $UcaId
            chain_id = [UInt64]$ChainId
            from = $FromAddress
            raw_tx = $RawTx
            require_public_broadcast = $true
            return_detail = $true
        }
    } | ConvertTo-Json -Depth 16 -Compress
    $sendResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $sendReq
    $summary.gateway_send = $sendResp

    $sendError = $null
    $sendErrorProp = $sendResp.PSObject.Properties["error"]
    if ($null -ne $sendErrorProp) {
        $sendError = $sendErrorProp.Value
    }
    $sendResult = $null
    $sendResultProp = $sendResp.PSObject.Properties["result"]
    if ($null -ne $sendResultProp) {
        $sendResult = $sendResultProp.Value
    }

    $sendErrorCode = $null
    $sendErrorMessage = $null
    if ($null -ne $sendError) {
        $sendErrorCodeProp = $sendError.PSObject.Properties["code"]
        if ($null -ne $sendErrorCodeProp) {
            $sendErrorCode = [string]$sendErrorCodeProp.Value
        }
        $sendErrorMessageProp = $sendError.PSObject.Properties["message"]
        if ($null -ne $sendErrorMessageProp -and $null -ne $sendErrorMessageProp.Value) {
            $sendErrorMessage = [string]$sendErrorMessageProp.Value
        }
    }

    # If address has already been bound to another UCA, retry using binding_owner directly.
    if ($sendErrorCode -eq "-32033" -and $sendErrorMessage) {
        $m = [regex]::Match($sendErrorMessage, "binding_owner=([a-zA-Z0-9_\\-]+)")
        if ($m.Success) {
            $bindingOwner = $m.Groups[1].Value
            if (-not [string]::IsNullOrWhiteSpace($bindingOwner)) {
                $summary.uca_id_effective = $bindingOwner
                $retryReq = @{
                    jsonrpc = "2.0"
                    id = 4
                    method = "eth_sendRawTransaction"
                    params = @{
                        uca_id = $bindingOwner
                        chain_id = [UInt64]$ChainId
                        from = $FromAddress
                        raw_tx = $RawTx
                        require_public_broadcast = $true
                        return_detail = $true
                    }
                } | ConvertTo-Json -Depth 16 -Compress
                $retryResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $retryReq
                $summary.gateway_send_retry = $retryResp
                $sendResp = $retryResp

                $sendError = $null
                $sendErrorProp = $sendResp.PSObject.Properties["error"]
                if ($null -ne $sendErrorProp) {
                    $sendError = $sendErrorProp.Value
                }
                $sendResult = $null
                $sendResultProp = $sendResp.PSObject.Properties["result"]
                if ($null -ne $sendResultProp) {
                    $sendResult = $sendResultProp.Value
                }
            }
        }
    }

    $txHash = $null
    if ($null -ne $sendError) {
        $sendErrorData = $null
        $sendErrorDataProp = $sendError.PSObject.Properties["data"]
        if ($null -ne $sendErrorDataProp) {
            $sendErrorData = $sendErrorDataProp.Value
        }
        if ($null -ne $sendErrorData) {
            $sendErrorTxHashProp = $sendErrorData.PSObject.Properties["tx_hash"]
            if ($null -ne $sendErrorTxHashProp -and -not [string]::IsNullOrWhiteSpace([string]$sendErrorTxHashProp.Value)) {
                $txHash = [string]$sendErrorTxHashProp.Value
            }
        }
    }
    if (-not $txHash -and $sendResult -is [string]) {
        $txHash = [string]$sendResult
    } elseif (-not $txHash -and $null -ne $sendResult) {
        $sendResultTxHashProp = $sendResult.PSObject.Properties["tx_hash"]
        if ($null -ne $sendResultTxHashProp -and -not [string]::IsNullOrWhiteSpace([string]$sendResultTxHashProp.Value)) {
            $txHash = [string]$sendResultTxHashProp.Value
        }
    }
    if ($txHash) {
        $statusReq = @{
            jsonrpc = "2.0"
            id = 5
            method = "evm_getTxSubmitStatus"
            params = @{
                chain_id = [UInt64]$ChainId
                tx_hash = $txHash
            }
        } | ConvertTo-Json -Depth 16 -Compress
        $summary.tx_status = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $statusReq
    }

    $upstreamReq = @{
        jsonrpc = "2.0"
        id = 6
        method = "eth_sendRawTransaction"
        params = @($RawTx)
    } | ConvertTo-Json -Depth 8 -Compress
    $upstreamResp = Invoke-RestMethod -Uri $UpstreamRpc -Method Post -ContentType "application/json" -Body $upstreamReq
    $summary.upstream_direct_send = $upstreamResp

    $chainOk = ($chainResp.result -eq ("0x{0:x}" -f $ChainId))
    $gatewayErrorCode = $null
    if ($null -ne $sendError) {
        $sendErrorCodeProp = $sendError.PSObject.Properties["code"]
        if ($null -ne $sendErrorCodeProp) {
            $gatewayErrorCode = [string]$sendErrorCodeProp.Value
        }
    }
    $gatewayBoundaryReached = ($gatewayErrorCode -eq "-32040")

    $upstreamInsufficientFunds = $false
    $upstreamError = $null
    $upstreamErrorProp = $upstreamResp.PSObject.Properties["error"]
    if ($null -ne $upstreamErrorProp) {
        $upstreamError = $upstreamErrorProp.Value
    }
    if ($null -ne $upstreamError) {
        $upstreamErrorMsgProp = $upstreamError.PSObject.Properties["message"]
        if ($null -ne $upstreamErrorMsgProp -and $null -ne $upstreamErrorMsgProp.Value) {
            $msg = [string]$upstreamErrorMsgProp.Value
            if ($msg.ToLowerInvariant().Contains("insufficient funds")) {
                $upstreamInsufficientFunds = $true
            }
        }
    }

    $summary.pass_connectivity = ($chainOk -and $gatewayBoundaryReached -and $upstreamInsufficientFunds)
    if (-not $summary.pass_connectivity) {
        throw "connectivity canary did not satisfy expected boundary checks"
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
    $summaryJson = $summary | ConvertTo-Json -Depth 28
    Set-Content -Path $SummaryOut -Value $summaryJson -Encoding UTF8
    Write-Host "summary written: $SummaryOut"
}

if (-not $summary.pass_connectivity) {
    throw "mainnet connectivity canary failed; inspect summary: $SummaryOut"
}

Write-Host "mainnet connectivity canary ok"
