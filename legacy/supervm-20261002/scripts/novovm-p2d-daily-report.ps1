[CmdletBinding()]
param(
    [string]$RpcUrl = "http://127.0.0.1:8899",
    [string]$DayLabel = "",
    [string]$OutputDir = "",
    [string]$TraceTxHash = "",
    [int]$JournalLimit = 50,
    [string]$MainlineQueryStorePath = "",
    [string]$NativeExecutionStorePath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}

function Ensure-Directory {
    param([Parameter(Mandatory = $true)][string]$Path)
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
}

function Get-JsonPropertyOrNull {
    param(
        [AllowNull()]$Object,
        [Parameter(Mandatory = $true)][string]$PropertyName
    )
    if ($null -eq $Object) { return $null }
    $prop = $Object.PSObject.Properties[$PropertyName]
    if ($null -eq $prop) { return $null }
    return $prop.Value
}

function To-UInt64OrZero {
    param([AllowNull()]$Value)
    if ($null -eq $Value) { return [uint64]0 }
    try { return [uint64]$Value } catch { return [uint64]0 }
}

function To-StringOrDefault {
    param(
        [AllowNull()]$Value,
        [string]$Default = ""
    )
    if ($null -eq $Value) { return $Default }
    $text = [string]$Value
    if ([string]::IsNullOrWhiteSpace($text)) { return $Default }
    return $text
}

function Get-MapCount {
    param(
        [AllowNull()]$MapObject,
        [Parameter(Mandatory = $true)][string[]]$Keys
    )
    if ($null -eq $MapObject) { return [uint64]0 }
    $sum = [uint64]0
    foreach ($prop in $MapObject.PSObject.Properties) {
        $name = [string]$prop.Name
        foreach ($key in $Keys) {
            if ($name -eq $key -or $name -like "*:$key" -or $name -like "*.$key") {
                $sum += To-UInt64OrZero $prop.Value
                break
            }
        }
    }
    return $sum
}

function Convert-MapToBulletLines {
    param([AllowNull()]$MapObject)
    if ($null -eq $MapObject) {
        return @("- (empty)")
    }
    $rows = @()
    foreach ($prop in $MapObject.PSObject.Properties) {
        $rows += [pscustomobject]@{
            key = [string]$prop.Name
            value = To-UInt64OrZero $prop.Value
        }
    }
    if ($rows.Count -eq 0) {
        return @("- (empty)")
    }
    $rows = $rows | Sort-Object -Property `
        @{ Expression = { $_.value }; Descending = $true }, `
        @{ Expression = { $_.key }; Descending = $false }
    $out = @()
    foreach ($row in $rows) {
        $out += "- $($row.key): $($row.value)"
    }
    return $out
}

$script:RpcCounter = 0
$script:QueryDataSource = "rpc"
$script:RpcFallbackReason = ""
function Invoke-NovJsonRpc {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$Method,
        [AllowNull()]$Params = @{}
    )
    $script:RpcCounter += 1
    $payload = [ordered]@{
        jsonrpc = "2.0"
        id = $script:RpcCounter
        method = $Method
        params = $Params
    }
    $payloadJson = $payload | ConvertTo-Json -Depth 64 -Compress
    $response = Invoke-RestMethod -Method Post -Uri $Url -ContentType "application/json" -Body $payloadJson
    if ($null -ne (Get-JsonPropertyOrNull -Object $response -PropertyName "error")) {
        throw "RPC $Method failed: $($response.error | ConvertTo-Json -Compress -Depth 32)"
    }
    if ($null -eq (Get-JsonPropertyOrNull -Object $response -PropertyName "result")) {
        throw "RPC $Method returned no result field"
    }
    return $response
}

function Test-RpcFallbackableError {
    param([string]$Message)
    if ([string]::IsNullOrWhiteSpace($Message)) { return $false }
    $patterns = @(
        "unknown method",
        "Unable to connect",
        "actively refused",
        "timed out",
        "No connection could be made",
        "The remote name could not be resolved",
        "failed to connect"
    )
    foreach ($pattern in $patterns) {
        if ($Message -like "*$pattern*") {
            return $true
        }
    }
    return $false
}

function Invoke-NovMainlineQuery {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [AllowNull()]$Params = @{},
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [string]$QueryStorePath = "",
        [string]$NativeStorePath = ""
    )
    $script:RpcCounter += 1
    $methodPrev = $env:NOVOVM_MAINLINE_QUERY_METHOD
    $paramsPrev = $env:NOVOVM_MAINLINE_QUERY_PARAMS
    $storePrev = $env:NOVOVM_MAINLINE_QUERY_STORE_PATH
    $nativeStorePrev = $env:NOVOVM_MAINLINE_NATIVE_EXECUTION_STORE_PATH
    try {
        $env:NOVOVM_MAINLINE_QUERY_METHOD = $Method
        $env:NOVOVM_MAINLINE_QUERY_PARAMS = ($Params | ConvertTo-Json -Depth 64 -Compress)
        if ([string]::IsNullOrWhiteSpace($QueryStorePath)) {
            Remove-Item Env:NOVOVM_MAINLINE_QUERY_STORE_PATH -ErrorAction SilentlyContinue
        } else {
            $env:NOVOVM_MAINLINE_QUERY_STORE_PATH = $QueryStorePath
        }
        if ([string]::IsNullOrWhiteSpace($NativeStorePath)) {
            Remove-Item Env:NOVOVM_MAINLINE_NATIVE_EXECUTION_STORE_PATH -ErrorAction SilentlyContinue
        } else {
            $env:NOVOVM_MAINLINE_NATIVE_EXECUTION_STORE_PATH = $NativeStorePath
        }
        Push-Location $RepoRoot
        try {
            $output = & cargo run -p novovm-node --bin supervm-mainline-query --quiet 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "mainline-query failed for ${Method}: $($output -join [Environment]::NewLine)"
            }
        } finally {
            Pop-Location
        }
        $outputText = ($output | ForEach-Object { [string]$_ }) -join [Environment]::NewLine
        $jsonStart = $outputText.IndexOf("{")
        if ($jsonStart -lt 0) {
            throw "mainline-query output does not contain JSON payload for $Method"
        }
        $jsonText = $outputText.Substring($jsonStart)
        $result = $jsonText | ConvertFrom-Json
        return [pscustomobject]@{
            jsonrpc = "2.0"
            id = $script:RpcCounter
            result = $result
        }
    } finally {
        if ($null -eq $methodPrev) { Remove-Item Env:NOVOVM_MAINLINE_QUERY_METHOD -ErrorAction SilentlyContinue } else { $env:NOVOVM_MAINLINE_QUERY_METHOD = $methodPrev }
        if ($null -eq $paramsPrev) { Remove-Item Env:NOVOVM_MAINLINE_QUERY_PARAMS -ErrorAction SilentlyContinue } else { $env:NOVOVM_MAINLINE_QUERY_PARAMS = $paramsPrev }
        if ($null -eq $storePrev) { Remove-Item Env:NOVOVM_MAINLINE_QUERY_STORE_PATH -ErrorAction SilentlyContinue } else { $env:NOVOVM_MAINLINE_QUERY_STORE_PATH = $storePrev }
        if ($null -eq $nativeStorePrev) { Remove-Item Env:NOVOVM_MAINLINE_NATIVE_EXECUTION_STORE_PATH -ErrorAction SilentlyContinue } else { $env:NOVOVM_MAINLINE_NATIVE_EXECUTION_STORE_PATH = $nativeStorePrev }
    }
}

function Invoke-NovDataQuery {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [AllowNull()]$Params = @{},
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [string]$QueryStorePath = "",
        [string]$NativeStorePath = ""
    )
    if ($script:QueryDataSource -eq "rpc") {
        try {
            return Invoke-NovJsonRpc -Url $Url -Method $Method -Params $Params
        } catch {
            $errorText = $_.Exception.Message
            if (-not (Test-RpcFallbackableError -Message $errorText)) {
                throw
            }
            $script:QueryDataSource = "mainline_query"
            if ([string]::IsNullOrWhiteSpace($script:RpcFallbackReason)) {
                $script:RpcFallbackReason = $errorText
            }
        }
    }
    return Invoke-NovMainlineQuery -Method $Method -Params $Params -RepoRoot $RepoRoot -QueryStorePath $QueryStorePath -NativeStorePath $NativeStorePath
}

function Write-JsonFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Object
    )
    $json = $Object | ConvertTo-Json -Depth 64
    Set-Content -Path $Path -Value $json -Encoding UTF8
}

function Format-Percent {
    param(
        [uint64]$Num,
        [uint64]$Den
    )
    if ($Den -eq 0) { return "n/a" }
    $pct = [double]$Num * 100.0 / [double]$Den
    return ("{0:N2}%" -f $pct)
}

$repoRoot = Resolve-RepoRoot
if ([string]::IsNullOrWhiteSpace($DayLabel)) {
    $DayLabel = (Get-Date).ToString("yyyy-MM-dd")
}
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = Join-Path $repoRoot "artifacts/mainline/p2d-run-phase/$DayLabel"
}

Ensure-Directory -Path $OutputDir
$rawDir = Join-Path $OutputDir "raw"
Ensure-Directory -Path $rawDir

$generatedAtUtc = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
$executionTraceParams = if ([string]::IsNullOrWhiteSpace($TraceTxHash)) { @{} } else { @{ tx_hash = $TraceTxHash } }
$settlementJournalParams = @{ limit = $JournalLimit }

$calls = [ordered]@{
    clearing_metrics = Invoke-NovDataQuery -Url $RpcUrl -RepoRoot $repoRoot -QueryStorePath $MainlineQueryStorePath -NativeStorePath $NativeExecutionStorePath -Method "nov_getTreasuryClearingMetricsSummary" -Params @{}
    policy_metrics = Invoke-NovDataQuery -Url $RpcUrl -RepoRoot $repoRoot -QueryStorePath $MainlineQueryStorePath -NativeStorePath $NativeExecutionStorePath -Method "nov_getTreasuryPolicyMetricsSummary" -Params @{}
    settlement_summary = Invoke-NovDataQuery -Url $RpcUrl -RepoRoot $repoRoot -QueryStorePath $MainlineQueryStorePath -NativeStorePath $NativeExecutionStorePath -Method "nov_getTreasurySettlementSummary" -Params @{}
    clearing_summary = Invoke-NovDataQuery -Url $RpcUrl -RepoRoot $repoRoot -QueryStorePath $MainlineQueryStorePath -NativeStorePath $NativeExecutionStorePath -Method "nov_getTreasuryClearingSummary" -Params @{}
    settlement_policy = Invoke-NovDataQuery -Url $RpcUrl -RepoRoot $repoRoot -QueryStorePath $MainlineQueryStorePath -NativeStorePath $NativeExecutionStorePath -Method "nov_getTreasurySettlementPolicy" -Params @{}
    execution_trace = Invoke-NovDataQuery -Url $RpcUrl -RepoRoot $repoRoot -QueryStorePath $MainlineQueryStorePath -NativeStorePath $NativeExecutionStorePath -Method "nov_getExecutionTrace" -Params $executionTraceParams
    settlement_journal = Invoke-NovDataQuery -Url $RpcUrl -RepoRoot $repoRoot -QueryStorePath $MainlineQueryStorePath -NativeStorePath $NativeExecutionStorePath -Method "nov_getTreasurySettlementJournal" -Params $settlementJournalParams
}

foreach ($key in $calls.Keys) {
    Write-JsonFile -Path (Join-Path $rawDir "$key.json") -Object $calls[$key]
}

$clearingSummaryPayload = Get-JsonPropertyOrNull -Object (Get-JsonPropertyOrNull -Object $calls.clearing_metrics.result -PropertyName "summary") -PropertyName "metrics"
$policySummaryPayload = Get-JsonPropertyOrNull -Object (Get-JsonPropertyOrNull -Object $calls.policy_metrics.result -PropertyName "summary") -PropertyName "metrics"
$settlementSummaryPayload = Get-JsonPropertyOrNull -Object $calls.settlement_summary.result -PropertyName "summary"
$clearingRiskPayload = Get-JsonPropertyOrNull -Object (Get-JsonPropertyOrNull -Object $calls.clearing_summary.result -PropertyName "summary") -PropertyName "risk"
$executionTracePayload = Get-JsonPropertyOrNull -Object $calls.execution_trace.result -PropertyName "trace"
$settlementPolicyPayload = Get-JsonPropertyOrNull -Object $calls.settlement_policy.result -PropertyName "policy"
$settlementJournalPayload = Get-JsonPropertyOrNull -Object $calls.settlement_journal.result -PropertyName "journal"

$attempts = To-UInt64OrZero (Get-JsonPropertyOrNull -Object $clearingSummaryPayload -PropertyName "total_clearing_attempts")
$successes = To-UInt64OrZero (Get-JsonPropertyOrNull -Object $clearingSummaryPayload -PropertyName "successful_clearings")
$failures = To-UInt64OrZero (Get-JsonPropertyOrNull -Object $clearingSummaryPayload -PropertyName "failed_clearings")
$failureCounts = Get-JsonPropertyOrNull -Object $clearingSummaryPayload -PropertyName "failure_counts"

$routeUnavailable = Get-MapCount -MapObject $failureCounts -Keys @("fee.clearing.route_unavailable", "route_unavailable")
$insufficientLiquidity = Get-MapCount -MapObject $failureCounts -Keys @("fee.clearing.insufficient_liquidity", "insufficient_liquidity")
$slippageExceeded = Get-MapCount -MapObject $failureCounts -Keys @("fee.clearing.slippage_exceeded", "slippage_exceeded")
$quoteExpired = Get-MapCount -MapObject $failureCounts -Keys @("fee.quote.quote_expired", "fee.quote_expired", "quote_expired")
$clearingDisabled = Get-MapCount -MapObject $failureCounts -Keys @("fee.clearing.clearing_disabled", "clearing_disabled")
$includedFailureTotal = $routeUnavailable + $insufficientLiquidity + $slippageExceeded
$denominator = if ($attempts -gt ($quoteExpired + $clearingDisabled)) {
    $attempts - ($quoteExpired + $clearingDisabled)
} else {
    [uint64]0
}

$thresholdState = To-StringOrDefault (Get-JsonPropertyOrNull -Object $policySummaryPayload -PropertyName "threshold_state") "unknown"
$policyContractId = To-StringOrDefault (Get-JsonPropertyOrNull -Object $policySummaryPayload -PropertyName "policy_contract_id") "unknown"
$policySource = To-StringOrDefault (Get-JsonPropertyOrNull -Object $policySummaryPayload -PropertyName "policy_source") "unknown"
$constrainedStrategy = To-StringOrDefault (Get-JsonPropertyOrNull -Object $policySummaryPayload -PropertyName "constrained_strategy") "none"
$thresholdStateHits = Get-JsonPropertyOrNull -Object $policySummaryPayload -PropertyName "threshold_state_hits"
$strategyHits = Get-JsonPropertyOrNull -Object $policySummaryPayload -PropertyName "constrained_strategy_hits"
$blockedHits = Get-MapCount -MapObject $thresholdStateHits -Keys @("blocked")
$traceFound = [bool](Get-JsonPropertyOrNull -Object $calls.execution_trace.result -PropertyName "found")

$settlementBuckets = Get-JsonPropertyOrNull -Object $settlementSummaryPayload -PropertyName "settlement_buckets_nov"
$reserveBucket = To-UInt64OrZero (Get-JsonPropertyOrNull -Object $settlementBuckets -PropertyName "reserve")
$feeBucket = To-UInt64OrZero (Get-JsonPropertyOrNull -Object $settlementBuckets -PropertyName "fee")
$riskBufferBucket = To-UInt64OrZero (Get-JsonPropertyOrNull -Object $settlementBuckets -PropertyName "risk_buffer")

$traceTxHash = To-StringOrDefault (Get-JsonPropertyOrNull -Object $executionTracePayload -PropertyName "tx_hash") "-"
$traceStatus = To-StringOrDefault (Get-JsonPropertyOrNull -Object $executionTracePayload -PropertyName "final_status") "-"
$traceFailureCode = To-StringOrDefault (Get-JsonPropertyOrNull -Object $executionTracePayload -PropertyName "final_failure_code") "-"

$journalTotalEntries = To-UInt64OrZero (Get-JsonPropertyOrNull -Object $settlementJournalPayload -PropertyName "total_entries")
$journalRequestedLimit = To-UInt64OrZero (Get-JsonPropertyOrNull -Object $settlementJournalPayload -PropertyName "requested_limit")
$journalEffectiveLimit = To-UInt64OrZero (Get-JsonPropertyOrNull -Object $settlementJournalPayload -PropertyName "effective_limit")

$reportPath = Join-Path $OutputDir "NOVOVM-CLEARING-METRICS-REPORT-$DayLabel.md"
$failureLines = Convert-MapToBulletLines -MapObject $failureCounts
$thresholdLines = Convert-MapToBulletLines -MapObject $thresholdStateHits
$strategyLines = Convert-MapToBulletLines -MapObject $strategyHits

$report = @()
$report += "# NOVOVM Clearing Metrics Report - $DayLabel"
$report += ""
$report += "- Generated at (UTC): $generatedAtUtc"
$report += "- RPC endpoint: $RpcUrl"
$report += "- Data source: $script:QueryDataSource"
if (-not [string]::IsNullOrWhiteSpace($script:RpcFallbackReason)) {
    $report += "- RPC fallback reason: $script:RpcFallbackReason"
}
$report += "- Raw snapshot directory: $rawDir"
$report += ""
$report += "## 1. Clearing Overview"
$report += ""
$report += "| Metric | Value |"
$report += "| --- | ---: |"
$report += "| total_clearing_attempts | $attempts |"
$report += "| successful_clearings | $successes |"
$report += "| failed_clearings | $failures |"
$report += "| success_rate | $(Format-Percent -Num $successes -Den $attempts) |"
$report += ""
$report += "## 2. P3 Decision Inputs (Current Snapshot)"
$report += ""
$report += "- denominator (attempts_non_nov excluding quote_expired and clearing_disabled): $denominator"
$report += "- route_unavailable: $routeUnavailable (" + (Format-Percent -Num $routeUnavailable -Den $denominator) + ")"
$report += "- insufficient_liquidity: $insufficientLiquidity (" + (Format-Percent -Num $insufficientLiquidity -Den $denominator) + ")"
$report += "- slippage_exceeded: $slippageExceeded (" + (Format-Percent -Num $slippageExceeded -Den $denominator) + ")"
$report += "- included_failure_total: $includedFailureTotal (" + (Format-Percent -Num $includedFailureTotal -Den $denominator) + ")"
$report += "- blocked_state_hits: $blockedHits"
$report += ""
$report += "Failure counts (all):"
$report += $failureLines
$report += ""
$report += "## 3. Policy Snapshot"
$report += ""
$report += "- policy_contract_id: $policyContractId"
$report += "- policy_source: $policySource"
$report += "- threshold_state: $thresholdState"
$report += "- constrained_strategy: $constrainedStrategy"
$report += ""
$report += "threshold_state_hits:"
$report += $thresholdLines
$report += ""
$report += "constrained_strategy_hits:"
$report += $strategyLines
$report += ""
$report += "## 4. Settlement Buckets Snapshot"
$report += ""
$report += "- reserve bucket NOV: $reserveBucket"
$report += "- fee bucket NOV: $feeBucket"
$report += "- risk_buffer bucket NOV: $riskBufferBucket"
$report += "- journal total entries: $journalTotalEntries (requested_limit=$journalRequestedLimit, effective_limit=$journalEffectiveLimit)"
$report += ""
$report += "## 5. Execution Trace Snapshot"
$report += ""
$report += "- trace_found: $traceFound"
$report += "- trace_tx_hash: $traceTxHash"
$report += "- trace_final_status: $traceStatus"
$report += "- trace_final_failure_code: $traceFailureCode"
$report += ""
$report += "## 6. Operator Notes"
$report += ""
$report += "- Decision statement: P3 remains Decision Only / Not Enabled unless threshold policy is satisfied."
$report += "- Observation:"
$report += "- Action:"
$report += ""
$report += "## 7. Files"
$report += ""
$report += "- raw/clearing_metrics.json"
$report += "- raw/policy_metrics.json"
$report += "- raw/settlement_summary.json"
$report += "- raw/clearing_summary.json"
$report += "- raw/settlement_policy.json"
$report += "- raw/execution_trace.json"
$report += "- raw/settlement_journal.json"
$report += ""

Set-Content -Path $reportPath -Value ($report -join [Environment]::NewLine) -Encoding UTF8

$exportSummary = [ordered]@{
    day_label = $DayLabel
    generated_at_utc = $generatedAtUtc
    rpc_url = $RpcUrl
    data_source = $script:QueryDataSource
    rpc_fallback_reason = $script:RpcFallbackReason
    output_dir = $OutputDir
    report_path = $reportPath
    raw_dir = $rawDir
    trace_tx_hash = $TraceTxHash
    calls = @(
        "nov_getTreasuryClearingMetricsSummary",
        "nov_getTreasuryPolicyMetricsSummary",
        "nov_getTreasurySettlementSummary",
        "nov_getTreasuryClearingSummary",
        "nov_getTreasurySettlementPolicy",
        "nov_getExecutionTrace",
        "nov_getTreasurySettlementJournal"
    )
}

$summaryPath = Join-Path $OutputDir "export-summary.json"
Write-JsonFile -Path $summaryPath -Object $exportSummary

$exportSummary | ConvertTo-Json -Depth 16
