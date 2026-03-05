param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [string]$Bind = "127.0.0.1:8901",
    [ValidateRange(1024, 1048576)]
    [int]$MaxBodyBytes = 65536,
    [ValidateRange(1, 1000)]
    [int]$RateLimitPerIp = 128,
    [ValidateRange(1, 64)]
    [int]$ExpectedRequests = 13,
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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\governance-rpc-gate"
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
            # retry
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

function Get-PropertyOrNull {
    param(
        [object]$InputObject,
        [string]$Name
    )

    if ($null -eq $InputObject) {
        return $null
    }
    if ($InputObject -is [System.Collections.IDictionary]) {
        if ($InputObject.Contains($Name)) {
            return $InputObject[$Name]
        }
        return $null
    }
    if ($InputObject.PSObject.Properties.Name -contains $Name) {
        return $InputObject.$Name
    }
    return $null
}

function Parse-ProcessedCount {
    param([string]$StdoutText)

    if (-not $StdoutText) {
        return 0
    }
    $summaryLine = (
        $StdoutText -split "`r?`n" |
            Where-Object { $_ -match "^(governance_rpc_server_out:|chain_query_rpc_server_out:)" } |
            Select-Object -Last 1
    )
    if (-not $summaryLine) {
        return 0
    }
    $processedMatch = [regex]::Match($summaryLine, "processed=(?<processed>\d+)")
    if ($processedMatch.Success) {
        return [int]$processedMatch.Groups["processed"].Value
    }
    return 0
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
    $psi.Environment["NOVOVM_ENABLE_PUBLIC_RPC"] = "0"
    $psi.Environment["NOVOVM_ENABLE_GOV_RPC"] = "1"
    $psi.Environment["NOVOVM_GOV_RPC_BIND"] = $Bind
    $psi.Environment["NOVOVM_GOV_RPC_MAX_BODY_BYTES"] = "$MaxBodyBytes"
    $psi.Environment["NOVOVM_GOV_RPC_RATE_LIMIT_PER_IP"] = "$RateLimitPerIp"
    $psi.Environment["NOVOVM_GOV_RPC_MAX_REQUESTS"] = "$MaxRequests"
    $psi.Environment["NOVOVM_GOV_RPC_ALLOWLIST"] = "127.0.0.1"
    $psi.Environment["NOVOVM_GOVERNANCE_PROPOSER_ALLOWLIST"] = "0"
    $psi.Environment["NOVOVM_GOVERNANCE_EXECUTOR_ALLOWLIST"] = "0"
    return [System.Diagnostics.Process]::Start($psi)
}

function Build-RpcBody {
    param(
        [int]$Id,
        [string]$Method,
        [object]$Params
    )
    return ([ordered]@{
        jsonrpc = "2.0"
        id = $Id
        method = $Method
        params = $Params
    } | ConvertTo-Json -Depth 20 -Compress)
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

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

$dbPath = Join-Path $OutputDir "query-db.json"
'{"blocks":[],"txs":{},"receipts":{},"balances":{}}' | Set-Content -Path $dbPath -Encoding UTF8

$bindUri = [Uri]("http://$Bind")
$rpcEndpoint = "http://$Bind/rpc"
$stdoutPath = Join-Path $OutputDir "governance-rpc.stdout.log"
$stderrPath = Join-Path $OutputDir "governance-rpc.stderr.log"
$proc = $null
$requests = @()
$errorReason = ""
$pass = $false
$processed = 0

try {
    $proc = Start-RpcServerProcess `
        -NodeExe $nodeExe `
        -RepoRoot $RepoRoot `
        -DbPath $dbPath `
        -Bind $Bind `
        -MaxBodyBytes $MaxBodyBytes `
        -RateLimitPerIp $RateLimitPerIp `
        -MaxRequests $ExpectedRequests

    $listening = Wait-TcpEndpoint -HostName $bindUri.Host -Port $bindUri.Port -TimeoutSeconds $StartupTimeoutSeconds
    if (-not $listening) {
        throw "governance rpc server did not listen on $Bind within ${StartupTimeoutSeconds}s"
    }

    $submitParam2Resp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 1 -Method "governance_submitProposal" -Params @{
        proposer = 0
        op = "update_mempool_fee_floor"
        fee_floor = 17
    })
    $requests += [ordered]@{ step = "submit_param2"; resp = $submitParam2Resp }

    $sign1Resp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 2 -Method "governance_sign" -Params @{
        proposal_id = 1
        signer_id = 1
        support = $true
    })
    $requests += [ordered]@{ step = "sign1"; resp = $sign1Resp }
    $sign1Sig = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $sign1Resp -Name "json") -Name "result") -Name "signature"

    $vote0Resp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 3 -Method "governance_vote" -Params @{
        proposal_id = 1
        voter_id = 0
        support = $true
    })
    $requests += [ordered]@{ step = "vote0"; resp = $vote0Resp }

    $vote1SignedResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 4 -Method "governance_vote" -Params @{
        proposal_id = 1
        voter_id = 1
        support = $true
        signature = $sign1Sig
    })
    $requests += [ordered]@{ step = "vote1_signed"; resp = $vote1SignedResp }

    $executeResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 5 -Method "governance_execute" -Params @{
        proposal_id = 1
        executor = 0
    })
    $requests += [ordered]@{ step = "execute_param2"; resp = $executeResp }

    $policyResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 6 -Method "governance_getPolicy" -Params @{})
    $requests += [ordered]@{ step = "policy_after_execute"; resp = $policyResp }

    $unauthorizedSubmitResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 7 -Method "governance_submitProposal" -Params @{
        proposer = 2
        op = "update_mempool_fee_floor"
        fee_floor = 19
    })
    $requests += [ordered]@{ step = "unauthorized_submit"; resp = $unauthorizedSubmitResp }

    $submitSlashResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 8 -Method "governance_submitProposal" -Params @{
        proposer = 0
        op = "update_slash_policy"
        mode = "observe_only"
        equivocation_threshold = 2
        min_active_validators = 2
        cooldown_epochs = 5
    })
    $requests += [ordered]@{ step = "submit_slash"; resp = $submitSlashResp }

    $slashSignResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 9 -Method "governance_sign" -Params @{
        proposal_id = 2
        signer_id = 0
        support = $true
    })
    $requests += [ordered]@{ step = "slash_sign0"; resp = $slashSignResp }
    $slashSignSig = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $slashSignResp -Name "json") -Name "result") -Name "signature"

    $slashVote0Resp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 10 -Method "governance_vote" -Params @{
        proposal_id = 2
        voter_id = 0
        support = $true
        signature = $slashSignSig
    })
    $requests += [ordered]@{ step = "slash_vote0"; resp = $slashVote0Resp }

    $slashVote0DupResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 11 -Method "governance_vote" -Params @{
        proposal_id = 2
        voter_id = 0
        support = $true
        signature = $slashSignSig
    })
    $requests += [ordered]@{ step = "slash_vote0_duplicate"; resp = $slashVote0DupResp }

    $auditResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 12 -Method "governance_listAuditEvents" -Params @{ limit = 50 })
    $requests += [ordered]@{ step = "list_audit_events"; resp = $auditResp }

    $listResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 13 -Method "governance_listProposals" -Params @{})
    $requests += [ordered]@{ step = "list_proposals"; resp = $listResp }

    if (-not $proc.WaitForExit($ExitTimeoutSeconds * 1000)) {
        try { $proc.Kill() } catch {}
        throw "governance rpc server did not exit within ${ExitTimeoutSeconds}s"
    }
} finally {
    if ($proc) {
        if (-not $proc.HasExited) {
            try { $proc.Kill() } catch {}
        }
        $stdoutText = $proc.StandardOutput.ReadToEnd()
        $stderrText = $proc.StandardError.ReadToEnd()
        $stdoutText | Set-Content -Path $stdoutPath -Encoding UTF8
        $stderrText | Set-Content -Path $stderrPath -Encoding UTF8
        $processed = Parse-ProcessedCount -StdoutText $stdoutText
    } else {
        "" | Set-Content -Path $stdoutPath -Encoding UTF8
        "" | Set-Content -Path $stderrPath -Encoding UTF8
    }
}

$stepMap = @{}
foreach ($item in $requests) {
    $stepMap[$item.step] = $item.resp
}

$submitParam2Resp = $stepMap["submit_param2"]
$sign1Resp = $stepMap["sign1"]
$vote0Resp = $stepMap["vote0"]
$vote1SignedResp = $stepMap["vote1_signed"]
$executeResp = $stepMap["execute_param2"]
$policyResp = $stepMap["policy_after_execute"]
$unauthorizedSubmitResp = $stepMap["unauthorized_submit"]
$submitSlashResp = $stepMap["submit_slash"]
$slashSignResp = $stepMap["slash_sign0"]
$slashVote0Resp = $stepMap["slash_vote0"]
$duplicateResp = $stepMap["slash_vote0_duplicate"]
$auditResp = $stepMap["list_audit_events"]
$listResp = $stepMap["list_proposals"]

$submitParam2Err = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $submitParam2Resp -Name "json") -Name "error"
$sign1Err = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $sign1Resp -Name "json") -Name "error"
$vote0Err = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $vote0Resp -Name "json") -Name "error"
$vote1SignedErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $vote1SignedResp -Name "json") -Name "error"
$executeErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $executeResp -Name "json") -Name "error"
$unauthorizedSubmitErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $unauthorizedSubmitResp -Name "json") -Name "error"
$submitSlashErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $submitSlashResp -Name "json") -Name "error"
$slashSignErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $slashSignResp -Name "json") -Name "error"
$slashVote0Err = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $slashVote0Resp -Name "json") -Name "error"
$auditErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $auditResp -Name "json") -Name "error"
$listErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $listResp -Name "json") -Name "error"

$submitParam2Ok = ($submitParam2Resp.status -eq 200 -and $null -eq $submitParam2Err)
$sign1Sig = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $sign1Resp -Name "json") -Name "result") -Name "signature"
$sign1Ok = ($sign1Resp.status -eq 200 -and $null -eq $sign1Err -and -not [string]::IsNullOrWhiteSpace([string]$sign1Sig))
$vote0Ok = ($vote0Resp.status -eq 200 -and $null -eq $vote0Err)
$vote1SignedOk = ($vote1SignedResp.status -eq 200 -and $null -eq $vote1SignedErr)
$executeOk = ($executeResp.status -eq 200 -and $null -eq $executeErr)
$policyFee = 0
$policyResult = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $policyResp -Name "json") -Name "result"
$policyFeeRaw = Get-PropertyOrNull -InputObject $policyResult -Name "mempool_fee_floor"
if ($null -ne $policyFeeRaw -and "$policyFeeRaw" -ne "") {
    $policyFee = [int64]$policyFeeRaw
}
$policyOk = ($policyResp.status -eq 200 -and $policyFee -eq 17)
$unauthorizedSubmitErrMsg = ""
$unauthorizedSubmitErrMsgRaw = Get-PropertyOrNull -InputObject $unauthorizedSubmitErr -Name "message"
if ($null -ne $unauthorizedSubmitErrMsgRaw) {
    $unauthorizedSubmitErrMsg = [string]$unauthorizedSubmitErrMsgRaw
}
$unauthorizedSubmitRejectOk = ($unauthorizedSubmitResp.status -eq 200 -and $unauthorizedSubmitErrMsg.ToLowerInvariant().Contains("unauthorized proposer"))
$submitSlashOk = ($submitSlashResp.status -eq 200 -and $null -eq $submitSlashErr)
$slashSignSig = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $slashSignResp -Name "json") -Name "result") -Name "signature"
$slashSignOk = ($slashSignResp.status -eq 200 -and $null -eq $slashSignErr -and -not [string]::IsNullOrWhiteSpace([string]$slashSignSig))
$slashVote0Ok = ($slashVote0Resp.status -eq 200 -and $null -eq $slashVote0Err)
$duplicateErrMsg = ""
$duplicateErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $duplicateResp -Name "json") -Name "error"
$duplicateErrMsgRaw = Get-PropertyOrNull -InputObject $duplicateErr -Name "message"
if ($null -ne $duplicateErrMsgRaw) {
    $duplicateErrMsg = [string]$duplicateErrMsgRaw
}
$duplicateRejectOk = ($duplicateResp.status -eq 200 -and $duplicateErrMsg.ToLowerInvariant().Contains("duplicate governance vote"))
$auditCount = 0
$auditHasSignOk = $false
$auditHasExecuteOk = $false
$auditHasSubmitReject = $false
$auditResult = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $auditResp -Name "json") -Name "result"
$auditCountRaw = Get-PropertyOrNull -InputObject $auditResult -Name "count"
if ($null -ne $auditCountRaw -and "$auditCountRaw" -ne "") {
    $auditCount = [int]$auditCountRaw
}
$auditEvents = Get-PropertyOrNull -InputObject $auditResult -Name "events"
if ($null -ne $auditEvents) {
    foreach ($event in $auditEvents) {
        $action = [string](Get-PropertyOrNull -InputObject $event -Name "action")
        $outcome = [string](Get-PropertyOrNull -InputObject $event -Name "outcome")
        if ($action -eq "sign" -and $outcome -eq "ok") { $auditHasSignOk = $true }
        if ($action -eq "execute" -and $outcome -eq "ok") { $auditHasExecuteOk = $true }
        if ($action -eq "submit" -and $outcome -eq "reject") { $auditHasSubmitReject = $true }
    }
}
$auditOk = ($auditResp.status -eq 200 -and $null -eq $auditErr -and $auditCount -ge 5 -and $auditHasSignOk -and $auditHasExecuteOk -and $auditHasSubmitReject)
$listOk = ($listResp.status -eq 200 -and $null -eq $listErr)
$processedOk = ($processed -eq $ExpectedRequests)

$pass = [bool](
    $submitParam2Ok -and
    $sign1Ok -and
    $vote0Ok -and
    $vote1SignedOk -and
    $executeOk -and
    $policyOk -and
    $unauthorizedSubmitRejectOk -and
    $submitSlashOk -and
    $slashSignOk -and
    $slashVote0Ok -and
    $duplicateRejectOk -and
    $auditOk -and
    $listOk -and
    $processedOk
)

if (-not $submitParam2Ok) { $errorReason = "submit_param2_failed" }
elseif (-not $sign1Ok) { $errorReason = "sign1_failed" }
elseif (-not $vote0Ok) { $errorReason = "vote0_failed" }
elseif (-not $vote1SignedOk) { $errorReason = "vote1_signed_failed" }
elseif (-not $executeOk) { $errorReason = "execute_param2_failed" }
elseif (-not $policyOk) { $errorReason = "policy_not_applied" }
elseif (-not $unauthorizedSubmitRejectOk) { $errorReason = "unauthorized_submit_not_rejected" }
elseif (-not $submitSlashOk) { $errorReason = "submit_slash_failed" }
elseif (-not $slashSignOk) { $errorReason = "slash_sign_failed" }
elseif (-not $slashVote0Ok) { $errorReason = "slash_vote0_failed" }
elseif (-not $duplicateRejectOk) { $errorReason = "duplicate_vote_not_rejected" }
elseif (-not $auditOk) { $errorReason = "audit_events_invalid" }
elseif (-not $listOk) { $errorReason = "list_proposals_failed" }
elseif (-not $processedOk) { $errorReason = "processed_count_mismatch" }

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    error_reason = $errorReason
    bind = $Bind
    expected_requests = $ExpectedRequests
    processed_requests = $processed
    submit_param2_ok = $submitParam2Ok
    sign1_ok = $sign1Ok
    vote0_ok = $vote0Ok
    vote1_signed_ok = $vote1SignedOk
    execute_ok = $executeOk
    policy_ok = $policyOk
    unauthorized_submit_reject_ok = $unauthorizedSubmitRejectOk
    submit_slash_ok = $submitSlashOk
    slash_sign_ok = $slashSignOk
    slash_vote0_ok = $slashVote0Ok
    duplicate_reject_ok = $duplicateRejectOk
    audit_ok = $auditOk
    audit_count = $auditCount
    audit_has_sign_ok = $auditHasSignOk
    audit_has_execute_ok = $auditHasExecuteOk
    audit_has_submit_reject = $auditHasSubmitReject
    list_ok = $listOk
    stdout_log = $stdoutPath
    stderr_log = $stderrPath
    policy_fee_after_execute = $policyFee
    unauthorized_submit_error_message = $unauthorizedSubmitErrMsg
    duplicate_error_message = $duplicateErrMsg
}

$summaryJson = Join-Path $OutputDir "governance-rpc-gate-summary.json"
$summaryMd = Join-Path $OutputDir "governance-rpc-gate-summary.md"
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Governance RPC Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- error_reason: $($summary.error_reason)"
    "- bind: $($summary.bind)"
    "- expected_requests: $($summary.expected_requests)"
    "- processed_requests: $($summary.processed_requests)"
    "- submit_param2_ok: $($summary.submit_param2_ok)"
    "- sign1_ok: $($summary.sign1_ok)"
    "- vote0_ok: $($summary.vote0_ok)"
    "- vote1_signed_ok: $($summary.vote1_signed_ok)"
    "- execute_ok: $($summary.execute_ok)"
    "- policy_ok: $($summary.policy_ok)"
    "- unauthorized_submit_reject_ok: $($summary.unauthorized_submit_reject_ok)"
    "- submit_slash_ok: $($summary.submit_slash_ok)"
    "- slash_sign_ok: $($summary.slash_sign_ok)"
    "- slash_vote0_ok: $($summary.slash_vote0_ok)"
    "- duplicate_reject_ok: $($summary.duplicate_reject_ok)"
    "- audit_ok: $($summary.audit_ok)"
    "- audit_count: $($summary.audit_count)"
    "- audit_has_sign_ok: $($summary.audit_has_sign_ok)"
    "- audit_has_execute_ok: $($summary.audit_has_execute_ok)"
    "- audit_has_submit_reject: $($summary.audit_has_submit_reject)"
    "- list_ok: $($summary.list_ok)"
    "- policy_fee_after_execute: $($summary.policy_fee_after_execute)"
    "- unauthorized_submit_error_message: $($summary.unauthorized_submit_error_message)"
    "- duplicate_error_message: $($summary.duplicate_error_message)"
    "- stdout_log: $($summary.stdout_log)"
    "- stderr_log: $($summary.stderr_log)"
    "- summary_json: $summaryJson"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "governance rpc gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  error_reason: $($summary.error_reason)"
Write-Host "  summary_json: $summaryJson"

if (-not $pass) {
    throw "governance rpc gate FAILED: $errorReason"
}

Write-Host "governance rpc gate PASS"
