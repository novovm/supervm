param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [string]$UpstreamRpc = "",
    [string]$Address = "",
    [UInt64]$ChainId = 1,
    [UInt64]$UpstreamTimeoutMs = 20000,
    [string]$SummaryOut = "artifacts/migration/evm-mainnet-read-attach-summary.json",
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

function Invoke-Rpc {
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
    $hasError = $resp.PSObject.Properties.Name -contains "error"
    if ($hasError -and $null -ne $resp.error) {
        throw ("{0} failed: code={1} message={2}" -f $Method, $resp.error.code, $resp.error.message)
    }
    $hasResult = $resp.PSObject.Properties.Name -contains "result"
    if (-not $hasResult) {
        throw ("{0} failed: response missing result field" -f $Method)
    }
    return $resp.result
}

function Normalize-Hex {
    param([AllowNull()][string]$Value)
    if ($null -eq $Value) { return $null }
    return $Value.Trim().ToLowerInvariant()
}

function Hex-ToBigInt {
    param([string]$Hex)
    if (-not $Hex) { return [System.Numerics.BigInteger]::Zero }
    $h = $Hex.Trim()
    if ($h.StartsWith("0x")) { $h = $h.Substring(2) }
    if (-not $h) { return [System.Numerics.BigInteger]::Zero }
    return [System.Numerics.BigInteger]::Parse($h, [System.Globalization.NumberStyles]::AllowHexSpecifier)
}

function BigInt-ToHex {
    param([System.Numerics.BigInteger]$Value)
    return ("0x{0:x}" -f $Value)
}

function Resolve-FirstTxHash {
    param($BlockObj)
    if ($null -eq $BlockObj) { return $null }
    $txs = $BlockObj.transactions
    if ($null -eq $txs -or -not ($txs -is [System.Collections.IEnumerable])) {
        return $null
    }
    foreach ($tx in $txs) {
        if ($tx -is [string] -and $tx) {
            return [string]$tx
        }
        if ($tx -is [psobject] -and $tx.PSObject.Properties["hash"]) {
            $h = [string]$tx.hash
            if ($h) { return $h }
        }
    }
    return $null
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$SummaryOut = Resolve-FullPath -Root $RepoRoot -Value $SummaryOut
$SummaryDir = Split-Path -Parent $SummaryOut
if ($SummaryDir) {
    New-Item -ItemType Directory -Force -Path $SummaryDir | Out-Null
}

if (-not $Address) {
    throw "missing Address: pass -Address <0x...>"
}
if ($Address -notmatch '^0x[a-fA-F0-9]{40}$') {
    throw "invalid Address format: $Address"
}

if (-not $UpstreamRpc) {
    $UpstreamRpc = $env:NOVOVM_GATEWAY_ETH_UPSTREAM_RPC
}
if (-not $UpstreamRpc) {
    $UpstreamRpc = "https://ethereum-rpc.publicnode.com"
}

$TargetRoot = if ($env:CARGO_TARGET_DIR) {
    [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
} else {
    Join-Path $RepoRoot "target"
}
if (-not $SkipBuild) {
    Push-Location $RepoRoot
    try {
        & cargo build -p novovm-evm-gateway
        if ($LASTEXITCODE -ne 0) {
            throw "build failed: novovm-evm-gateway"
        }
    } finally {
        Pop-Location
    }
}
$GatewayExe = Resolve-BinaryPath -TargetRoot $TargetRoot -BinaryBaseName "novovm-evm-gateway"
if (-not (Test-Path $GatewayExe)) {
    throw "gateway binary not found: $GatewayExe"
}

$logDir = Resolve-FullPath -Root $RepoRoot -Value "artifacts/migration/logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$gwOut = Join-Path $logDir "evm-mainnet-read-attach-gateway.stdout.log"
$gwErr = Join-Path $logDir "evm-mainnet-read-attach-gateway.stderr.log"
if (Test-Path $gwOut) { Remove-Item -Force $gwOut }
if (Test-Path $gwErr) { Remove-Item -Force $gwErr }

$chainHex = ("0x{0:x}" -f $ChainId)
$envMap = @{
    "NOVOVM_GATEWAY_BIND" = $GatewayBind
    "NOVOVM_GATEWAY_ETH_DEFAULT_CHAIN_ID" = ([string]$ChainId)
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC" = $UpstreamRpc
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_TIMEOUT_MS" = ([string]$UpstreamTimeoutMs)
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
}
finally {
    Pop-ProcessEnv -State $envState
}

$startedAt = [DateTimeOffset]::UtcNow
$summary = [ordered]@{
    started_at_utc = $startedAt.ToString("o")
    gateway_bind = $GatewayBind
    chain_id = $ChainId
    upstream_rpc = $UpstreamRpc
    address = $Address
    chain_id_match = $false
    block_number_delta = $null
    anchor_block = $null
    block_match = $false
    balance_match = $false
    code_match = $false
    tx_receipt_checked = $false
    tx_receipt_match = $null
    sample_tx_hash = $null
    gateway_stdout = $gwOut
    gateway_stderr = $gwErr
    overall_pass = $false
}

try {
    Start-Sleep -Milliseconds 900
    if ($gatewayProc.HasExited) {
        throw "gateway exited early"
    }

    $gwUrl = "http://$GatewayBind"
    $upUrl = $UpstreamRpc

    $gwChain = [string](Invoke-Rpc -Url $gwUrl -Method "eth_chainId" -Params @())
    $upChain = [string](Invoke-Rpc -Url $upUrl -Method "eth_chainId" -Params @())
    $summary.gateway_chain_id = $gwChain
    $summary.upstream_chain_id = $upChain
    $summary.chain_id_match = (Normalize-Hex $gwChain) -eq (Normalize-Hex $upChain) -and ((Normalize-Hex $gwChain) -eq "0x1")

    $gwBlockHex = [string](Invoke-Rpc -Url $gwUrl -Method "eth_blockNumber" -Params @())
    $upBlockHex = [string](Invoke-Rpc -Url $upUrl -Method "eth_blockNumber" -Params @())
    $summary.gateway_block_number = $gwBlockHex
    $summary.upstream_block_number = $upBlockHex

    $gwBlockNum = Hex-ToBigInt $gwBlockHex
    $upBlockNum = Hex-ToBigInt $upBlockHex
    $delta = $upBlockNum - $gwBlockNum
    if ($delta -lt 0) { $delta = -$delta }
    $summary.block_number_delta = [int64]$delta

    $anchorNum = $upBlockNum
    if ($anchorNum -gt 8) {
        $anchorNum = $anchorNum - 8
    }
    $anchorHex = BigInt-ToHex $anchorNum
    $summary.anchor_block = $anchorHex

    $gwBlock = Invoke-Rpc -Url $gwUrl -Method "eth_getBlockByNumber" -Params @($anchorHex, $false)
    $upBlock = Invoke-Rpc -Url $upUrl -Method "eth_getBlockByNumber" -Params @($anchorHex, $false)
    $summary.gateway_block_hash = $gwBlock.hash
    $summary.upstream_block_hash = $upBlock.hash
    $summary.block_match =
        (Normalize-Hex([string]$gwBlock.hash) -eq Normalize-Hex([string]$upBlock.hash)) -and
        (Normalize-Hex([string]$gwBlock.stateRoot) -eq Normalize-Hex([string]$upBlock.stateRoot)) -and
        (Normalize-Hex([string]$gwBlock.transactionsRoot) -eq Normalize-Hex([string]$upBlock.transactionsRoot)) -and
        (Normalize-Hex([string]$gwBlock.receiptsRoot) -eq Normalize-Hex([string]$upBlock.receiptsRoot))

    $gwBalance = [string](Invoke-Rpc -Url $gwUrl -Method "eth_getBalance" -Params @($Address, $anchorHex))
    $upBalance = [string](Invoke-Rpc -Url $upUrl -Method "eth_getBalance" -Params @($Address, $anchorHex))
    $summary.gateway_balance = $gwBalance
    $summary.upstream_balance = $upBalance
    $summary.balance_match = (Normalize-Hex $gwBalance) -eq (Normalize-Hex $upBalance)

    $gwCode = [string](Invoke-Rpc -Url $gwUrl -Method "eth_getCode" -Params @($Address, $anchorHex))
    $upCode = [string](Invoke-Rpc -Url $upUrl -Method "eth_getCode" -Params @($Address, $anchorHex))
    $summary.gateway_code = $gwCode
    $summary.upstream_code = $upCode
    $summary.code_match = (Normalize-Hex $gwCode) -eq (Normalize-Hex $upCode)

    $sampleTxHash = Resolve-FirstTxHash -BlockObj $upBlock
    if ($sampleTxHash) {
        $summary.tx_receipt_checked = $true
        $summary.sample_tx_hash = $sampleTxHash
        $gwReceipt = Invoke-Rpc -Url $gwUrl -Method "eth_getTransactionReceipt" -Params @($sampleTxHash)
        $upReceipt = Invoke-Rpc -Url $upUrl -Method "eth_getTransactionReceipt" -Params @($sampleTxHash)
        if ($null -eq $gwReceipt -or $null -eq $upReceipt) {
            $summary.tx_receipt_match = $false
        } else {
            $summary.tx_receipt_match =
                (Normalize-Hex([string]$gwReceipt.blockHash) -eq Normalize-Hex([string]$upReceipt.blockHash)) -and
                (Normalize-Hex([string]$gwReceipt.status) -eq Normalize-Hex([string]$upReceipt.status)) -and
                (Normalize-Hex([string]$gwReceipt.transactionIndex) -eq Normalize-Hex([string]$upReceipt.transactionIndex)) -and
                (Normalize-Hex([string]$gwReceipt.effectiveGasPrice) -eq Normalize-Hex([string]$upReceipt.effectiveGasPrice))
        }
    }

    $receiptOk = if ($summary.tx_receipt_checked) { [bool]$summary.tx_receipt_match } else { $true }
    $summary.overall_pass =
        [bool]$summary.chain_id_match -and
        [bool]$summary.block_match -and
        [bool]$summary.balance_match -and
        [bool]$summary.code_match -and
        $receiptOk
}
finally {
    if ($null -ne $gatewayProc -and -not $gatewayProc.HasExited) {
        try {
            Stop-Process -Id $gatewayProc.Id -Force -ErrorAction SilentlyContinue
        } catch {
        }
    }
    $summary.completed_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    $summaryJson = $summary | ConvertTo-Json -Depth 32
    Set-Content -Path $SummaryOut -Value $summaryJson -Encoding UTF8
    Write-Host "summary written: $SummaryOut"
}

if (-not [bool]$summary.overall_pass) {
    throw "mainnet read attach failed; inspect summary: $SummaryOut"
}

Write-Host ("mainnet read attach ok: anchor={0} address={1}" -f $summary.anchor_block, $Address)
