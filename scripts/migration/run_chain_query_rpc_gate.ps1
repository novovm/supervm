param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [string]$Bind = "127.0.0.1:8899",
    [ValidateRange(1024, 1048576)]
    [int]$MaxBodyBytes = 65536,
    [ValidateRange(1, 1000)]
    [int]$RateLimitPerIp = 64,
    [ValidateRange(1, 32)]
    [int]$ExpectedRequests = 5,
    [ValidateRange(2, 64)]
    [int]$RateLimitProbeRequests = 3,
    [ValidateRange(1, 1000)]
    [int]$RateLimitProbePerIp = 2,
    [ValidateRange(1, 30)]
    [int]$StartupTimeoutSeconds = 8,
    [ValidateRange(1, 30)]
    [int]$ExitTimeoutSeconds = 12
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\chain-query-rpc-gate"
}

function Invoke-Cargo {
    param(
        [string]$WorkDir,
        [string[]]$CargoArgs
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($CargoArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")

    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()

    $text = ($stdout + $stderr).Trim()
    if ($proc.ExitCode -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed in $WorkDir`n$text"
    }
    return $text
}

function Wait-TcpEndpoint {
    param(
        [string]$HostName,
        [int]$Port,
        [int]$TimeoutSeconds
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $client = [System.Net.Sockets.TcpClient]::new()
        try {
            $async = $client.BeginConnect($HostName, $Port, $null, $null)
            if ($async.AsyncWaitHandle.WaitOne(180)) {
                $client.EndConnect($async)
                return $true
            }
        } catch {
            # retry until timeout
        } finally {
            $client.Dispose()
        }
        Start-Sleep -Milliseconds 120
    }
    return $false
}

function Invoke-JsonPost {
    param(
        [string]$Uri,
        [string]$Body
    )

    $raw = ""
    $status = 0
    $parsed = $null
    $webCmd = Get-Command Invoke-WebRequest -ErrorAction Stop
    $hasSkipHttpErrorCheck = $webCmd.Parameters.ContainsKey("SkipHttpErrorCheck")

    try {
        $invokeParams = @{
            Uri = $Uri
            Method = "Post"
            ContentType = "application/json; charset=utf-8"
            Body = $Body
            UseBasicParsing = $true
            ErrorAction = "Stop"
        }
        if ($hasSkipHttpErrorCheck) {
            $invokeParams["SkipHttpErrorCheck"] = $true
        }
        $response = Invoke-WebRequest @invokeParams
        $status = [int]$response.StatusCode
        $raw = [string]$response.Content
    } catch {
        $webResponse = $null
        if ($_.Exception -and $_.Exception.PSObject.Properties.Name -contains "Response") {
            $webResponse = $_.Exception.Response
        }
        if ($null -eq $webResponse) {
            throw
        }

        if ($webResponse.PSObject.Properties.Name -contains "StatusCode") {
            $statusCodeValue = $webResponse.StatusCode
            if ($statusCodeValue -is [int]) {
                $status = [int]$statusCodeValue
            } else {
                $status = [int]$statusCodeValue.value__
            }
        }

        $stream = $null
        $reader = $null
        try {
            $stream = $webResponse.GetResponseStream()
            if ($null -ne $stream) {
                $reader = [System.IO.StreamReader]::new($stream)
                $raw = $reader.ReadToEnd()
            }
        } finally {
            if ($reader) { $reader.Dispose() }
            if ($stream) { $stream.Dispose() }
        }
    }

    $convertFromJsonCmd = Get-Command ConvertFrom-Json -ErrorAction Stop
    $hasJsonDepth = $convertFromJsonCmd.Parameters.ContainsKey("Depth")
    if ($raw) {
        try {
            if ($hasJsonDepth) {
                $parsed = $raw | ConvertFrom-Json -Depth 32
            } else {
                $parsed = $raw | ConvertFrom-Json
            }
        } catch {
            $parsed = $null
        }
    }
    return [ordered]@{
        status = [int]$status
        body = $raw
        json = $parsed
    }
}

function Start-RpcServerProcess {
    param(
        [string]$NodeExe,
        [string]$RepoRoot,
        [string]$DbPath,
        [string]$Bind,
        [int]$MaxBodyBytes,
        [int]$RateLimitPerIp,
        [int]$MaxRequests
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $NodeExe
    $psi.WorkingDirectory = $RepoRoot
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Environment["NOVOVM_NODE_MODE"] = "rpc_server"
    $psi.Environment["NOVOVM_CHAIN_QUERY_DB"] = $DbPath
    $psi.Environment["NOVOVM_ENABLE_PUBLIC_RPC"] = "1"
    $psi.Environment["NOVOVM_ENABLE_GOV_RPC"] = "0"
    $psi.Environment["NOVOVM_RPC_BIND"] = $Bind
    $psi.Environment["NOVOVM_PUBLIC_RPC_BIND"] = $Bind
    $psi.Environment["NOVOVM_RPC_MAX_BODY_BYTES"] = "$MaxBodyBytes"
    $psi.Environment["NOVOVM_PUBLIC_RPC_MAX_BODY_BYTES"] = "$MaxBodyBytes"
    $psi.Environment["NOVOVM_RPC_RATE_LIMIT_PER_IP"] = "$RateLimitPerIp"
    $psi.Environment["NOVOVM_PUBLIC_RPC_RATE_LIMIT_PER_IP"] = "$RateLimitPerIp"
    $psi.Environment["NOVOVM_RPC_MAX_REQUESTS"] = "$MaxRequests"
    $psi.Environment["NOVOVM_PUBLIC_RPC_MAX_REQUESTS"] = "$MaxRequests"
    return [System.Diagnostics.Process]::Start($psi)
}

function Save-ProcessLogs {
    param(
        [System.Diagnostics.Process]$Process,
        [string]$StdoutPath,
        [string]$StderrPath,
        [string]$StdoutTextPrefix = "",
        [string]$StderrTextPrefix = ""
    )

    if (-not $Process) {
        return [ordered]@{
            stdout = $StdoutTextPrefix
            stderr = $StderrTextPrefix
        }
    }
    if (-not $Process.HasExited) {
        try { $Process.Kill() } catch {}
    }

    $stdoutText = $StdoutTextPrefix + $Process.StandardOutput.ReadToEnd()
    $stderrText = $StderrTextPrefix + $Process.StandardError.ReadToEnd()
    $stdoutText | Set-Content -Path $StdoutPath -Encoding UTF8
    $stderrText | Set-Content -Path $StderrPath -Encoding UTF8
    return [ordered]@{
        stdout = $stdoutText
        stderr = $stderrText
    }
}

function Parse-ProcessedCount {
    param([string]$StdoutText)

    if (-not $StdoutText) {
        return 0
    }
    $summaryLine = ($StdoutText -split "`r?`n" | Where-Object { $_ -match "^chain_query_rpc_server_out:" } | Select-Object -Last 1)
    if (-not $summaryLine) {
        return 0
    }
    $processedMatch = [regex]::Match($summaryLine, "processed=(?<processed>\d+)")
    if ($processedMatch.Success) {
        return [int]$processedMatch.Groups["processed"].Value
    }
    return 0
}

function Has-JsonProperty {
    param(
        [object]$Obj,
        [string]$Name
    )
    if ($null -eq $Obj) {
        return $false
    }
    return ($Obj.PSObject.Properties.Name -contains $Name)
}

function Get-JsonPropertyOrNull {
    param(
        [object]$Obj,
        [string]$Name
    )
    if (Has-JsonProperty -Obj $Obj -Name $Name) {
        return $Obj.$Name
    }
    return $null
}

function Invoke-RpcScenario {
    param(
        [string]$NodeExe,
        [string]$RepoRoot,
        [string]$DbPath,
        [string]$Bind,
        [int]$MaxBodyBytes,
        [int]$RateLimitPerIp,
        [int]$MaxRequests,
        [int]$StartupTimeoutSeconds,
        [int]$ExitTimeoutSeconds,
        [object[]]$Payloads,
        [string]$StdoutPath,
        [string]$StderrPath
    )

    $bindUri = [Uri]("http://$Bind")
    $rpcEndpoint = "http://$Bind/rpc"
    $proc = $null
    $requests = @()
    $stdoutText = ""
    $stderrText = ""
    $basePass = $false
    $errorReason = ""
    $exitCode = -1
    $processed = 0

    try {
        $proc = Start-RpcServerProcess `
            -NodeExe $NodeExe `
            -RepoRoot $RepoRoot `
            -DbPath $DbPath `
            -Bind $Bind `
            -MaxBodyBytes $MaxBodyBytes `
            -RateLimitPerIp $RateLimitPerIp `
            -MaxRequests $MaxRequests

        $listening = Wait-TcpEndpoint -HostName $bindUri.Host -Port $bindUri.Port -TimeoutSeconds $StartupTimeoutSeconds
        if (-not $listening) {
            throw "rpc server did not listen on $Bind within ${StartupTimeoutSeconds}s"
        }

        foreach ($entry in $Payloads) {
            $body = [ordered]@{
                jsonrpc = "2.0"
                id = $entry.id
                method = $entry.method
                params = $entry.params
            } | ConvertTo-Json -Depth 16 -Compress

            $resp = Invoke-JsonPost -Uri $rpcEndpoint -Body $body
            $requests += [ordered]@{
                id = $entry.id
                method = $entry.method
                status = [int]$resp.status
                body = $resp.body
                json = $resp.json
            }
        }

        $serverExited = $proc.WaitForExit($ExitTimeoutSeconds * 1000)
        if (-not $serverExited) {
            throw "rpc server did not exit within ${ExitTimeoutSeconds}s after requests"
        }

        $exitCode = [int]$proc.ExitCode
        $stdoutText = $proc.StandardOutput.ReadToEnd()
        $stderrText = $proc.StandardError.ReadToEnd()
        $processed = Parse-ProcessedCount -StdoutText $stdoutText
        $basePass = (
            $exitCode -eq 0 -and
            $processed -eq $Payloads.Count
        )
    } catch {
        $errorReason = $_.Exception.Message
        $basePass = $false
    } finally {
        $logs = Save-ProcessLogs `
            -Process $proc `
            -StdoutPath $StdoutPath `
            -StderrPath $StderrPath `
            -StdoutTextPrefix $stdoutText `
            -StderrTextPrefix $stderrText
        $stdoutText = $logs.stdout
        $stderrText = $logs.stderr
        if ($processed -eq 0) {
            $processed = Parse-ProcessedCount -StdoutText $stdoutText
        }
        if ($proc -and $proc.HasExited -and $exitCode -lt 0) {
            $exitCode = [int]$proc.ExitCode
        }
    }

    return [ordered]@{
        base_pass = $basePass
        error_reason = $errorReason
        exit_code = $exitCode
        processed = $processed
        expected = $Payloads.Count
        requests = $requests
        server_stdout = $StdoutPath
        server_stderr = $StderrPath
        rate_limit_per_ip = $RateLimitPerIp
        max_requests = $MaxRequests
        endpoint = $rpcEndpoint
    }
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

if ($RateLimitProbeRequests -le $RateLimitProbePerIp) {
    throw "RateLimitProbeRequests ($RateLimitProbeRequests) must be greater than RateLimitProbePerIp ($RateLimitProbePerIp) to trigger 429"
}

$nodeCrateDir = Join-Path $RepoRoot "crates\novovm-node"
if (-not (Test-Path (Join-Path $nodeCrateDir "Cargo.toml"))) {
    throw "missing novovm-node Cargo.toml: $nodeCrateDir"
}

Invoke-Cargo -WorkDir $nodeCrateDir -CargoArgs @("build", "--quiet", "--bin", "novovm-node") | Out-Null
$nodeExeCandidates = @(
    (Join-Path $RepoRoot "target\debug\novovm-node.exe"),
    (Join-Path $nodeCrateDir "target\debug\novovm-node.exe")
)
$nodeExe = ""
foreach ($candidate in $nodeExeCandidates) {
    if (Test-Path $candidate) {
        $nodeExe = (Resolve-Path $candidate).Path
        break
    }
}
if (-not $nodeExe) {
    throw "missing novovm-node binary after build; checked: $($nodeExeCandidates -join ', ')"
}

$blockHash = ("ab" * 32).ToLowerInvariant()
$stateRoot = ("cd" * 32).ToLowerInvariant()
$proposalHash = ("ef" * 32).ToLowerInvariant()
$parentHash = ("00" * 32)
$txHash = ("12" * 32).ToLowerInvariant()

$seedDb = [ordered]@{
    blocks = @(
        [ordered]@{
            height = 7
            epoch_id = 2
            parent_hash = $parentHash
            state_root = $stateRoot
            tx_count = 1
            batch_count = 1
            proposal_hash = $proposalHash
            block_hash = $blockHash
        }
    )
    txs = [ordered]@{}
    receipts = [ordered]@{}
    balances = [ordered]@{
        "1001" = 42
    }
}
$seedDb.txs[$txHash] = [ordered]@{
    tx_hash = $txHash
    block_height = 7
    block_hash = $blockHash
    account = 1001
    key = 77
    value = 42
    nonce = 1
    fee = 3
    success = $true
}
$seedDb.receipts[$txHash] = [ordered]@{
    tx_hash = $txHash
    block_height = 7
    block_hash = $blockHash
    success = $true
    gas_used = 3
    state_root = $stateRoot
}

$dbPath = Join-Path $OutputDir "rpc-query-db.json"
$seedDb | ConvertTo-Json -Depth 8 | Set-Content -Path $dbPath -Encoding UTF8

$queryStdoutPath = Join-Path $OutputDir "rpc-server.query.stdout.log"
$queryStderrPath = Join-Path $OutputDir "rpc-server.query.stderr.log"
$rateStdoutPath = Join-Path $OutputDir "rpc-server.rate-limit.stdout.log"
$rateStderrPath = Join-Path $OutputDir "rpc-server.rate-limit.stderr.log"

$queryPayloads = @(
    [ordered]@{ id = 1; method = "getBlock"; params = [ordered]@{ height = 7 } },
    [ordered]@{ id = 2; method = "getTransaction"; params = [ordered]@{ tx_hash = $txHash } },
    [ordered]@{ id = 3; method = "getReceipt"; params = [ordered]@{ tx_hash = $txHash } },
    [ordered]@{ id = 4; method = "getBalance"; params = [ordered]@{ account = "1001" } },
    [ordered]@{ id = 5; method = "getUnknown"; params = [ordered]@{} }
)
if ($queryPayloads.Count -ne $ExpectedRequests) {
    throw "ExpectedRequests=$ExpectedRequests does not match built payload count=$($queryPayloads.Count)"
}

$rateLimitPayloads = @()
for ($i = 0; $i -lt $RateLimitProbeRequests; $i++) {
    $rateLimitPayloads += [ordered]@{
        id = 101 + $i
        method = "getBalance"
        params = [ordered]@{ account = "1001" }
    }
}

$queryScenario = Invoke-RpcScenario `
    -NodeExe $nodeExe `
    -RepoRoot $RepoRoot `
    -DbPath $dbPath `
    -Bind $Bind `
    -MaxBodyBytes $MaxBodyBytes `
    -RateLimitPerIp $RateLimitPerIp `
    -MaxRequests $ExpectedRequests `
    -StartupTimeoutSeconds $StartupTimeoutSeconds `
    -ExitTimeoutSeconds $ExitTimeoutSeconds `
    -Payloads $queryPayloads `
    -StdoutPath $queryStdoutPath `
    -StderrPath $queryStderrPath

$queryResults = $queryScenario.requests
$querySignalPass = $false
$queryError = $queryScenario.error_reason
if ($queryScenario.base_pass -and $queryResults.Count -eq $ExpectedRequests) {
    $blockResult = Get-JsonPropertyOrNull -Obj $queryResults[0].json -Name "result"
    $blockRecord = Get-JsonPropertyOrNull -Obj $blockResult -Name "block"
    $txResult = Get-JsonPropertyOrNull -Obj $queryResults[1].json -Name "result"
    $txRecord = Get-JsonPropertyOrNull -Obj $txResult -Name "transaction"
    $receiptResult = Get-JsonPropertyOrNull -Obj $queryResults[2].json -Name "result"
    $receiptRecord = Get-JsonPropertyOrNull -Obj $receiptResult -Name "receipt"
    $balanceResult = Get-JsonPropertyOrNull -Obj $queryResults[3].json -Name "result"
    $unknownErr = Get-JsonPropertyOrNull -Obj $queryResults[4].json -Name "error"

    $blockOk = (
        $queryResults[0].status -eq 200 -and
        $null -ne $blockResult -and
        $null -ne $blockRecord -and
        [bool](Get-JsonPropertyOrNull -Obj $blockResult -Name "found") -eq $true -and
        [int64](Get-JsonPropertyOrNull -Obj $blockRecord -Name "height") -eq 7 -and
        [string](Get-JsonPropertyOrNull -Obj $blockRecord -Name "block_hash") -eq $blockHash
    )
    $txOk = (
        $queryResults[1].status -eq 200 -and
        $null -ne $txResult -and
        $null -ne $txRecord -and
        [bool](Get-JsonPropertyOrNull -Obj $txResult -Name "found") -eq $true -and
        [string](Get-JsonPropertyOrNull -Obj $txRecord -Name "tx_hash") -eq $txHash -and
        [int64](Get-JsonPropertyOrNull -Obj $txRecord -Name "account") -eq 1001
    )
    $receiptOk = (
        $queryResults[2].status -eq 200 -and
        $null -ne $receiptResult -and
        $null -ne $receiptRecord -and
        [bool](Get-JsonPropertyOrNull -Obj $receiptResult -Name "found") -eq $true -and
        [string](Get-JsonPropertyOrNull -Obj $receiptRecord -Name "tx_hash") -eq $txHash -and
        [string](Get-JsonPropertyOrNull -Obj $receiptRecord -Name "state_root") -eq $stateRoot
    )
    $balanceOk = (
        $queryResults[3].status -eq 200 -and
        $null -ne $balanceResult -and
        [bool](Get-JsonPropertyOrNull -Obj $balanceResult -Name "found") -eq $true -and
        [string](Get-JsonPropertyOrNull -Obj $balanceResult -Name "account") -eq "1001" -and
        [int64](Get-JsonPropertyOrNull -Obj $balanceResult -Name "balance") -eq 42
    )
    $unknownMethodOk = (
        $queryResults[4].status -eq 200 -and
        $null -ne $unknownErr -and
        [int](Get-JsonPropertyOrNull -Obj $unknownErr -Name "code") -eq -32602
    )
    $querySignalPass = (
        $blockOk -and
        $txOk -and
        $receiptOk -and
        $balanceOk -and
        $unknownMethodOk
    )
    if (-not $querySignalPass) {
        $queryError = "query assertion failed (block=$blockOk, tx=$txOk, receipt=$receiptOk, balance=$balanceOk, unknown_method=$unknownMethodOk)"
    }
}

$rateScenario = Invoke-RpcScenario `
    -NodeExe $nodeExe `
    -RepoRoot $RepoRoot `
    -DbPath $dbPath `
    -Bind $Bind `
    -MaxBodyBytes $MaxBodyBytes `
    -RateLimitPerIp $RateLimitProbePerIp `
    -MaxRequests $RateLimitProbeRequests `
    -StartupTimeoutSeconds $StartupTimeoutSeconds `
    -ExitTimeoutSeconds $ExitTimeoutSeconds `
    -Payloads $rateLimitPayloads `
    -StdoutPath $rateStdoutPath `
    -StderrPath $rateStderrPath

$rateResults = $rateScenario.requests
$rateLimitSignalPass = $false
$rateError = $rateScenario.error_reason
if ($rateScenario.base_pass -and $rateResults.Count -eq $RateLimitProbeRequests) {
    $allowedOk = $true
    for ($i = 0; $i -lt $RateLimitProbePerIp; $i++) {
        if ($rateResults[$i].status -ne 200) {
            $allowedOk = $false
            break
        }
    }

    $limitedOk = $true
    for ($i = $RateLimitProbePerIp; $i -lt $RateLimitProbeRequests; $i++) {
        if ($rateResults[$i].status -ne 429) {
            $limitedOk = $false
            break
        }
        $errorObj = Get-JsonPropertyOrNull -Obj $rateResults[$i].json -Name "error"
        if ($null -ne $errorObj -and [int](Get-JsonPropertyOrNull -Obj $errorObj -Name "code") -ne -32029) {
            $limitedOk = $false
            break
        }
    }
    $rateLimitSignalPass = $allowedOk -and $limitedOk
    if (-not $rateLimitSignalPass) {
        $rateError = "rate-limit assertion failed (allowed_ok=$allowedOk, limited_ok=$limitedOk, probe_per_ip=$RateLimitProbePerIp, probe_requests=$RateLimitProbeRequests)"
    }
}

$gatePass = $querySignalPass -and $rateLimitSignalPass
$errorReason = ""
if (-not $gatePass) {
    $parts = @()
    if (-not $querySignalPass) {
        $parts += "query_stage_failed: $queryError"
    }
    if (-not $rateLimitSignalPass) {
        $parts += "rate_limit_stage_failed: $rateError"
    }
    $errorReason = $parts -join "; "
}

$rpcEndpoint = "http://$Bind/rpc"
$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $gatePass
    bind = $Bind
    endpoint = $rpcEndpoint
    expected_requests = $ExpectedRequests
    max_body_bytes = $MaxBodyBytes
    rate_limit_per_ip = $RateLimitPerIp
    rate_limit_probe_per_ip = $RateLimitProbePerIp
    rate_limit_probe_requests = $RateLimitProbeRequests
    query_db = $dbPath
    node_exe = $nodeExe
    error_reason = $errorReason
    query_signal = [ordered]@{
        pass = $querySignalPass
        base_pass = $queryScenario.base_pass
        exit_code = $queryScenario.exit_code
        processed = $queryScenario.processed
        expected = $queryScenario.expected
        error_reason = $queryError
        server_stdout = $queryScenario.server_stdout
        server_stderr = $queryScenario.server_stderr
    }
    rate_limit_signal = [ordered]@{
        pass = $rateLimitSignalPass
        base_pass = $rateScenario.base_pass
        exit_code = $rateScenario.exit_code
        processed = $rateScenario.processed
        expected = $rateScenario.expected
        rate_limit_per_ip = $rateScenario.rate_limit_per_ip
        error_reason = $rateError
        server_stdout = $rateScenario.server_stdout
        server_stderr = $rateScenario.server_stderr
    }
    requests = $queryResults
    rate_limit_requests = $rateResults
}

$summaryJson = Join-Path $OutputDir "chain-query-rpc-gate-summary.json"
$summaryMd = Join-Path $OutputDir "chain-query-rpc-gate-summary.md"
$summary | ConvertTo-Json -Depth 16 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Chain Query RPC Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- bind: $($summary.bind)"
    "- endpoint: $($summary.endpoint)"
    "- expected_requests: $($summary.expected_requests)"
    "- max_body_bytes: $($summary.max_body_bytes)"
    "- rate_limit_per_ip: $($summary.rate_limit_per_ip)"
    "- rate_limit_probe_per_ip: $($summary.rate_limit_probe_per_ip)"
    "- rate_limit_probe_requests: $($summary.rate_limit_probe_requests)"
    "- query_db: $($summary.query_db)"
    "- node_exe: $($summary.node_exe)"
    "- error_reason: $($summary.error_reason)"
    ""
    "## Signals"
    ""
    "- query_signal.pass: $($summary.query_signal.pass)"
    "- query_signal.base_pass: $($summary.query_signal.base_pass)"
    "- query_signal.processed: $($summary.query_signal.processed)/$($summary.query_signal.expected)"
    "- query_signal.exit_code: $($summary.query_signal.exit_code)"
    "- query_signal.error_reason: $($summary.query_signal.error_reason)"
    "- rate_limit_signal.pass: $($summary.rate_limit_signal.pass)"
    "- rate_limit_signal.base_pass: $($summary.rate_limit_signal.base_pass)"
    "- rate_limit_signal.processed: $($summary.rate_limit_signal.processed)/$($summary.rate_limit_signal.expected)"
    "- rate_limit_signal.exit_code: $($summary.rate_limit_signal.exit_code)"
    "- rate_limit_signal.error_reason: $($summary.rate_limit_signal.error_reason)"
    ""
    "## Query Request Results"
    ""
    "| id | method | status | has_result | has_error |"
    "|---|---|---:|---|---|"
)
foreach ($r in $queryResults) {
    $hasResult = ($null -ne $r.json -and ($r.json.PSObject.Properties.Name -contains "result"))
    $hasError = ($null -ne $r.json -and ($r.json.PSObject.Properties.Name -contains "error"))
    $md += "| $($r.id) | $($r.method) | $($r.status) | $hasResult | $hasError |"
}
$md += ""
$md += "## Rate Limit Probe Results"
$md += ""
$md += "| id | method | status | error_code |"
$md += "|---|---|---:|---:|"
foreach ($r in $rateResults) {
    $errorCode = ""
    if ($null -ne $r.json -and ($r.json.PSObject.Properties.Name -contains "error")) {
        $errorCode = "$($r.json.error.code)"
    }
    $md += "| $($r.id) | $($r.method) | $($r.status) | $errorCode |"
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "chain query rpc gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  query_signal.pass: $($summary.query_signal.pass)"
Write-Host "  rate_limit_signal.pass: $($summary.rate_limit_signal.pass)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"

if (-not $summary.pass) {
    throw "chain query rpc gate FAILED: $($summary.error_reason)"
}

Write-Host "chain query rpc gate PASS"
