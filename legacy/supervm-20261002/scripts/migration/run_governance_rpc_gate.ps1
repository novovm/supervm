param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [string]$Bind = "127.0.0.1:8901",
    [ValidateSet("ed25519")]
    [string]$GovernanceVoteVerifier = "ed25519",
    [ValidateRange(1024, 1048576)]
    [int]$MaxBodyBytes = 65536,
    [ValidateRange(1, 1000)]
    [int]$RateLimitPerIp = 128,
    [ValidateRange(1, 64)]
    [int]$ExpectedRequests = 16,
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

function Read-JsonFile {
    param([string]$Path)

    if (-not (Test-Path $Path)) {
        return $null
    }
    $raw = Get-Content -Path $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return $null
    }
    $convertFromJsonCmd = Get-Command ConvertFrom-Json -ErrorAction Stop
    $hasJsonDepth = $convertFromJsonCmd.Parameters.ContainsKey("Depth")
    if ($hasJsonDepth) {
        return ($raw | ConvertFrom-Json -Depth 64)
    }
    return ($raw | ConvertFrom-Json)
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

function Parse-GovernanceVoteVerifierStatus {
    param([string]$StdoutText)

    if (-not $StdoutText) {
        return [ordered]@{
            line = ""
            configured = ""
            active = ""
        }
    }
    $line = (
        $StdoutText -split "`r?`n" |
            Where-Object { $_ -match "^governance_vote_verifier_in:" } |
            Select-Object -Last 1
    )
    if (-not $line) {
        return [ordered]@{
            line = ""
            configured = ""
            active = ""
        }
    }
    $configured = ""
    $active = ""
    $configuredMatch = [regex]::Match($line, "configured=(?<configured>\S+)")
    if ($configuredMatch.Success) {
        $configured = [string]$configuredMatch.Groups["configured"].Value
    }
    $activeMatch = [regex]::Match($line, "active=(?<active>\S+)")
    if ($activeMatch.Success) {
        $active = [string]$activeMatch.Groups["active"].Value
    }
    return [ordered]@{
        line = [string]$line
        configured = $configured
        active = $active
    }
}

function Start-RpcServerProcess {
    param(
        [string]$NodeExe,
        [string]$RepoRoot,
        [string]$DbPath,
        [string]$AuditDbPath,
        [string]$ChainAuditDbPath,
        [string]$Bind,
        [string]$VoteVerifier,
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
    $psi.Environment["NOVOVM_AOEM_VARIANT"] = "persist"
    $storageRoot = Split-Path -Parent $DbPath
    if (-not [string]::IsNullOrWhiteSpace($storageRoot)) {
        $psi.Environment["NOVOVM_D2D3_STORAGE_ROOT"] = $storageRoot
    }
    $psi.Environment["NOVOVM_CHAIN_QUERY_DB"] = $DbPath
    $psi.Environment["NOVOVM_GOVERNANCE_AUDIT_DB"] = $AuditDbPath
    $psi.Environment["NOVOVM_GOVERNANCE_CHAIN_AUDIT_DB"] = $ChainAuditDbPath
    $psi.Environment["NOVOVM_ENABLE_PUBLIC_RPC"] = "0"
    $psi.Environment["NOVOVM_ENABLE_GOV_RPC"] = "1"
    $psi.Environment["NOVOVM_GOV_RPC_BIND"] = $Bind
    $psi.Environment["NOVOVM_GOV_RPC_MAX_BODY_BYTES"] = "$MaxBodyBytes"
    $psi.Environment["NOVOVM_GOV_RPC_RATE_LIMIT_PER_IP"] = "$RateLimitPerIp"
    $psi.Environment["NOVOVM_GOV_RPC_MAX_REQUESTS"] = "$MaxRequests"
    $psi.Environment["NOVOVM_GOV_RPC_ALLOWLIST"] = "127.0.0.1"
    $psi.Environment["NOVOVM_GOVERNANCE_PROPOSER_ALLOWLIST"] = "0"
    $psi.Environment["NOVOVM_GOVERNANCE_EXECUTOR_ALLOWLIST"] = "0"
    $psi.Environment["NOVOVM_GOVERNANCE_VOTE_VERIFIER"] = $VoteVerifier
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

function Assert-CargoPathDependencyHygiene {
    param([string]$RepoRoot)

    $forbiddenPattern = [regex]"SVM2026[\\/]+contracts[\\/]+web30[\\/]+core"

    $vendorManifest = Join-Path $RepoRoot "vendor\web30-core\Cargo.toml"
    if (-not (Test-Path $vendorManifest)) {
        throw "missing vendor dependency manifest: $vendorManifest"
    }

    $cargoTomls = Get-ChildItem -Path $RepoRoot -Recurse -Filter Cargo.toml -File
    foreach ($manifest in $cargoTomls) {
        $text = Get-Content -Path $manifest.FullName -Raw
        if ($forbiddenPattern.IsMatch($text)) {
            throw "forbidden external path dependency found in $($manifest.FullName): do not reference SVM2026/contracts/web30/core directly; use vendor/web30-core"
        }
    }

    $cargoLocks = Get-ChildItem -Path $RepoRoot -Recurse -Filter Cargo.lock -File
    $forbiddenLockPattern = [regex]"path\+file:///.+SVM2026[\\/]+contracts[\\/]+web30[\\/]+core"
    foreach ($lock in $cargoLocks) {
        $text = Get-Content -Path $lock.FullName -Raw
        if ($forbiddenLockPattern.IsMatch($text)) {
            throw "forbidden absolute lockfile source found in $($lock.FullName): regenerate lockfile against vendor/web30-core"
        }
    }
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
Assert-CargoPathDependencyHygiene -RepoRoot $RepoRoot

$nodeCrateDir = Join-Path $RepoRoot "crates\novovm-node"
if (-not (Test-Path (Join-Path $nodeCrateDir "Cargo.toml"))) {
    throw "missing novovm-node Cargo.toml: $nodeCrateDir"
}
Invoke-Cargo -WorkDir $nodeCrateDir -CargoArgs @("build", "--quiet", "--bin", "novovm-node") | Out-Null

$cargoTargetDir = ""
if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    if ([System.IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
        $cargoTargetDir = $env:CARGO_TARGET_DIR
    } else {
        $cargoTargetDir = Join-Path $RepoRoot $env:CARGO_TARGET_DIR
    }
}
$nodeExeCandidates = @()
if ($IsWindows) {
    $nodeBinaryNames = @("novovm-node.exe", "novovm-node")
} else {
    $nodeBinaryNames = @("novovm-node", "novovm-node.exe")
}
if (-not [string]::IsNullOrWhiteSpace($cargoTargetDir)) {
    foreach ($name in $nodeBinaryNames) {
        $nodeExeCandidates += (Join-Path $cargoTargetDir "debug\$name")
    }
}
foreach ($name in $nodeBinaryNames) {
    $nodeExeCandidates += (Join-Path $RepoRoot "target\debug\$name")
    $nodeExeCandidates += (Join-Path $nodeCrateDir "target\debug\$name")
}
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
$auditDbPath = Join-Path $OutputDir "governance-audit-events.json"
if (Test-Path $auditDbPath) {
    Remove-Item -Path $auditDbPath -Force
}
$chainAuditDbPath = Join-Path $OutputDir "governance-chain-audit-events.json"
if (Test-Path $chainAuditDbPath) {
    Remove-Item -Path $chainAuditDbPath -Force
}

$bindUri = [Uri]("http://$Bind")
$rpcEndpoint = "http://$Bind/rpc"
$stdoutPath = Join-Path $OutputDir "governance-rpc.stdout.log"
$stderrPath = Join-Path $OutputDir "governance-rpc.stderr.log"
$proc = $null
$requests = @()
$errorReason = ""
$pass = $false
$processed = 0
$stdoutText = ""
$stderrText = ""
$voteVerifierConfigured = ""
$voteVerifierActive = ""
$voteVerifierStartupOk = $false
$voteVerifierLine = ""
$voteVerifierStagedRejectOk = $false
$voteVerifierRejectExitCode = 0
$voteVerifierRejectErrorMessage = ""
$voteVerifierRejectStdoutPath = Join-Path $OutputDir "governance-rpc-reject.stdout.log"
$voteVerifierRejectStderrPath = Join-Path $OutputDir "governance-rpc-reject.stderr.log"
$chainAuditRestartOk = $false
$chainAuditRestartCount = 0
$chainAuditRestartHeadSeq = 0
$chainAuditRestartRoot = ""
$chainAuditRestartRootOk = $false
$chainAuditRestartHasSubmitAccepted = $false
$chainAuditRestartHasExecuteApplied = $false
$chainAuditRestartErrorMessage = ""
$chainAuditRestartStdoutPath = Join-Path $OutputDir "governance-rpc-restart.stdout.log"
$chainAuditRestartStderrPath = Join-Path $OutputDir "governance-rpc-restart.stderr.log"
$chainAuditRoot = ""

try {
    $proc = Start-RpcServerProcess `
        -NodeExe $nodeExe `
        -RepoRoot $RepoRoot `
        -DbPath $dbPath `
        -AuditDbPath $auditDbPath `
        -ChainAuditDbPath $chainAuditDbPath `
        -Bind $Bind `
        -VoteVerifier $GovernanceVoteVerifier `
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

    $signUnsupportedSchemeResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 21 -Method "governance_sign" -Params @{
        proposal_id = 1
        signer_id = 1
        support = $true
        signature_scheme = "mldsa87"
    })
    $requests += [ordered]@{ step = "sign_unsupported_scheme"; resp = $signUnsupportedSchemeResp }

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

    $chainAuditAfterExecuteResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 61 -Method "governance_listChainAuditEvents" -Params @{
        proposal_id = 1
        limit = 50
    })
    $requests += [ordered]@{ step = "list_chain_audit_events_after_execute"; resp = $chainAuditAfterExecuteResp }

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

    $chainAuditResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 14 -Method "governance_listChainAuditEvents" -Params @{
        proposal_id = 1
        limit = 50
    })
    $requests += [ordered]@{ step = "list_chain_audit_events"; resp = $chainAuditResp }

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
        $voteVerifierStatus = Parse-GovernanceVoteVerifierStatus -StdoutText $stdoutText
        $voteVerifierLine = [string]$voteVerifierStatus.line
        $voteVerifierConfigured = [string]$voteVerifierStatus.configured
        $voteVerifierActive = [string]$voteVerifierStatus.active
        $voteVerifierStartupOk = (
            -not [string]::IsNullOrWhiteSpace($voteVerifierLine) -and
            $voteVerifierConfigured.ToLowerInvariant() -eq $GovernanceVoteVerifier.ToLowerInvariant() -and
            $voteVerifierActive.ToLowerInvariant() -eq "ed25519"
        )
    } else {
        "" | Set-Content -Path $stdoutPath -Encoding UTF8
        "" | Set-Content -Path $stderrPath -Encoding UTF8
    }
}

$restartProc = $null
try {
    $restartProc = Start-RpcServerProcess `
        -NodeExe $nodeExe `
        -RepoRoot $RepoRoot `
        -DbPath $dbPath `
        -AuditDbPath $auditDbPath `
        -ChainAuditDbPath $chainAuditDbPath `
        -Bind $Bind `
        -VoteVerifier $GovernanceVoteVerifier `
        -MaxBodyBytes $MaxBodyBytes `
        -RateLimitPerIp $RateLimitPerIp `
        -MaxRequests 1

    $restartListening = Wait-TcpEndpoint -HostName $bindUri.Host -Port $bindUri.Port -TimeoutSeconds $StartupTimeoutSeconds
    if (-not $restartListening) {
        throw "chain audit restart probe did not listen on $Bind within ${StartupTimeoutSeconds}s"
    }

    $chainAuditRestartResp = Invoke-JsonPost -Uri $rpcEndpoint -Body (Build-RpcBody -Id 31 -Method "governance_listChainAuditEvents" -Params @{
        proposal_id = 1
        limit = 50
    })

    if (-not $restartProc.WaitForExit($ExitTimeoutSeconds * 1000)) {
        try { $restartProc.Kill() } catch {}
        throw "chain audit restart probe did not exit within ${ExitTimeoutSeconds}s"
    }

    $chainAuditRestartErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $chainAuditRestartResp -Name "json") -Name "error"
    $chainAuditRestartResult = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $chainAuditRestartResp -Name "json") -Name "result"
    $chainAuditRestartCountRaw = Get-PropertyOrNull -InputObject $chainAuditRestartResult -Name "count"
    if ($null -ne $chainAuditRestartCountRaw -and "$chainAuditRestartCountRaw" -ne "") {
        $chainAuditRestartCount = [int]$chainAuditRestartCountRaw
    }
    $chainAuditRestartHeadSeqRaw = Get-PropertyOrNull -InputObject $chainAuditRestartResult -Name "head_seq"
    if ($null -ne $chainAuditRestartHeadSeqRaw -and "$chainAuditRestartHeadSeqRaw" -ne "") {
        $chainAuditRestartHeadSeq = [int]$chainAuditRestartHeadSeqRaw
    }
    $chainAuditRestartRootRaw = Get-PropertyOrNull -InputObject $chainAuditRestartResult -Name "root"
    if ($null -ne $chainAuditRestartRootRaw) {
        $chainAuditRestartRoot = ([string]$chainAuditRestartRootRaw).Trim().ToLowerInvariant()
    }
    $chainAuditRestartRootOk = ($chainAuditRestartRoot -match '^[0-9a-f]{64}$')
    $chainAuditRestartEvents = Get-PropertyOrNull -InputObject $chainAuditRestartResult -Name "events"
    if ($null -ne $chainAuditRestartEvents) {
        foreach ($event in $chainAuditRestartEvents) {
            $action = [string](Get-PropertyOrNull -InputObject $event -Name "action")
            $outcome = [string](Get-PropertyOrNull -InputObject $event -Name "outcome")
            if ($action -eq "submit" -and $outcome -eq "accepted") { $chainAuditRestartHasSubmitAccepted = $true }
            if ($action -eq "execute" -and $outcome -eq "applied") { $chainAuditRestartHasExecuteApplied = $true }
        }
    }

    $chainAuditRestartOk = (
        $chainAuditRestartResp.status -eq 200 -and
        $null -eq $chainAuditRestartErr -and
        $chainAuditRestartCount -ge 2 -and
        $chainAuditRestartHeadSeq -ge $chainAuditRestartCount -and
        $chainAuditRestartRootOk -and
        $chainAuditRestartHasSubmitAccepted -and
        $chainAuditRestartHasExecuteApplied
    )
} catch {
    $chainAuditRestartErrorMessage = $_.Exception.Message
    $chainAuditRestartOk = $false
} finally {
    if ($restartProc) {
        if (-not $restartProc.HasExited) {
            try { $restartProc.Kill() } catch {}
        }
        $restartStdout = $restartProc.StandardOutput.ReadToEnd()
        $restartStderr = $restartProc.StandardError.ReadToEnd()
        $restartStdout | Set-Content -Path $chainAuditRestartStdoutPath -Encoding UTF8
        $restartStderr | Set-Content -Path $chainAuditRestartStderrPath -Encoding UTF8
    }
    if (-not (Test-Path $chainAuditRestartStdoutPath)) {
        "" | Set-Content -Path $chainAuditRestartStdoutPath -Encoding UTF8
    }
    if (-not (Test-Path $chainAuditRestartStderrPath)) {
        "" | Set-Content -Path $chainAuditRestartStderrPath -Encoding UTF8
    }
}

$rejectProc = $null
try {
    $rejectProc = Start-RpcServerProcess `
        -NodeExe $nodeExe `
        -RepoRoot $RepoRoot `
        -DbPath $dbPath `
        -AuditDbPath $auditDbPath `
        -ChainAuditDbPath $chainAuditDbPath `
        -Bind $Bind `
        -VoteVerifier "mldsa87" `
        -MaxBodyBytes $MaxBodyBytes `
        -RateLimitPerIp $RateLimitPerIp `
        -MaxRequests 1

    if (-not $rejectProc.WaitForExit($StartupTimeoutSeconds * 1000)) {
        try { $rejectProc.Kill() } catch {}
        throw "vote verifier reject probe timed out (mldsa87 did not exit)"
    }

    $rejectStdout = $rejectProc.StandardOutput.ReadToEnd()
    $rejectStderr = $rejectProc.StandardError.ReadToEnd()
    $rejectStdout | Set-Content -Path $voteVerifierRejectStdoutPath -Encoding UTF8
    $rejectStderr | Set-Content -Path $voteVerifierRejectStderrPath -Encoding UTF8
    $voteVerifierRejectExitCode = [int]$rejectProc.ExitCode
    $rejectCombined = ($rejectStdout + $rejectStderr)
    $voteVerifierRejectErrorMessage = $rejectCombined.Trim()
    $rejectLower = $rejectCombined.ToLowerInvariant()
    $voteVerifierStagedRejectOk = (
        $voteVerifierRejectExitCode -ne 0 -and
        $rejectLower.Contains("unsupported governance vote verifier") -and
        (
            $rejectLower.Contains("disabled-by-policy") -or
            $rejectLower.Contains("policy-gated") -or
            $rejectLower.Contains("staged-only")
        )
    )
} catch {
    $voteVerifierRejectErrorMessage = $_.Exception.Message
    $voteVerifierStagedRejectOk = $false
} finally {
    if ($rejectProc) {
        if (-not $rejectProc.HasExited) {
            try { $rejectProc.Kill() } catch {}
        }
    }
    if (-not (Test-Path $voteVerifierRejectStdoutPath)) {
        "" | Set-Content -Path $voteVerifierRejectStdoutPath -Encoding UTF8
    }
    if (-not (Test-Path $voteVerifierRejectStderrPath)) {
        "" | Set-Content -Path $voteVerifierRejectStderrPath -Encoding UTF8
    }
}

$stepMap = @{}
foreach ($item in $requests) {
    $stepMap[$item.step] = $item.resp
}

$submitParam2Resp = $stepMap["submit_param2"]
$sign1Resp = $stepMap["sign1"]
$signUnsupportedSchemeResp = $stepMap["sign_unsupported_scheme"]
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
$chainAuditResp = $stepMap["list_chain_audit_events"]
$chainAuditAfterExecuteResp = $stepMap["list_chain_audit_events_after_execute"]

$submitParam2Err = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $submitParam2Resp -Name "json") -Name "error"
$sign1Err = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $sign1Resp -Name "json") -Name "error"
$signUnsupportedSchemeErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $signUnsupportedSchemeResp -Name "json") -Name "error"
$vote0Err = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $vote0Resp -Name "json") -Name "error"
$vote1SignedErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $vote1SignedResp -Name "json") -Name "error"
$executeErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $executeResp -Name "json") -Name "error"
$unauthorizedSubmitErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $unauthorizedSubmitResp -Name "json") -Name "error"
$submitSlashErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $submitSlashResp -Name "json") -Name "error"
$slashSignErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $slashSignResp -Name "json") -Name "error"
$slashVote0Err = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $slashVote0Resp -Name "json") -Name "error"
$auditErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $auditResp -Name "json") -Name "error"
$listErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $listResp -Name "json") -Name "error"
$chainAuditErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $chainAuditResp -Name "json") -Name "error"
$chainAuditAfterExecuteErr = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $chainAuditAfterExecuteResp -Name "json") -Name "error"

$submitParam2Ok = ($submitParam2Resp.status -eq 200 -and $null -eq $submitParam2Err)
$sign1Sig = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $sign1Resp -Name "json") -Name "result") -Name "signature"
$sign1Ok = ($sign1Resp.status -eq 200 -and $null -eq $sign1Err -and -not [string]::IsNullOrWhiteSpace([string]$sign1Sig))
$signUnsupportedSchemeErrMsg = ""
$signUnsupportedSchemeErrMsgRaw = Get-PropertyOrNull -InputObject $signUnsupportedSchemeErr -Name "message"
if ($null -ne $signUnsupportedSchemeErrMsgRaw) {
    $signUnsupportedSchemeErrMsg = [string]$signUnsupportedSchemeErrMsgRaw
}
$signUnsupportedSchemeRejectOk = ($signUnsupportedSchemeResp.status -eq 200 -and $signUnsupportedSchemeErrMsg.ToLowerInvariant().Contains("unsupported governance signature scheme"))
$vote0Ok = ($vote0Resp.status -eq 200 -and $null -eq $vote0Err)
$vote1SignedOk = ($vote1SignedResp.status -eq 200 -and $null -eq $vote1SignedErr)
$executeOk = ($executeResp.status -eq 200 -and $null -eq $executeErr)
$executeVoteVerifierName = ""
$executeVoteVerifierScheme = ""
$executeResult = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $executeResp -Name "json") -Name "result"
$executeVoteVerifier = Get-PropertyOrNull -InputObject $executeResult -Name "vote_verifier"
$executeVoteVerifierNameRaw = Get-PropertyOrNull -InputObject $executeVoteVerifier -Name "name"
if ($null -ne $executeVoteVerifierNameRaw) {
    $executeVoteVerifierName = [string]$executeVoteVerifierNameRaw
}
$executeVoteVerifierSchemeRaw = Get-PropertyOrNull -InputObject $executeVoteVerifier -Name "signature_scheme"
if ($null -ne $executeVoteVerifierSchemeRaw) {
    $executeVoteVerifierScheme = ([string]$executeVoteVerifierSchemeRaw).Trim().ToLowerInvariant()
}
$executeVoteVerifierOk = (
    $executeOk -and
    -not [string]::IsNullOrWhiteSpace($executeVoteVerifierName) -and
    $executeVoteVerifierScheme -eq $voteVerifierActive.ToLowerInvariant()
)
$policyFee = 0
$policyChainAuditHeadSeq = 0
$policyChainAuditRoot = ""
$policyChainAuditRootOk = $false
$policyResult = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $policyResp -Name "json") -Name "result"
$policyFeeRaw = Get-PropertyOrNull -InputObject $policyResult -Name "mempool_fee_floor"
if ($null -ne $policyFeeRaw -and "$policyFeeRaw" -ne "") {
    $policyFee = [int64]$policyFeeRaw
}
$policyChainAudit = Get-PropertyOrNull -InputObject $policyResult -Name "governance_chain_audit"
$policyChainAuditHeadSeqRaw = Get-PropertyOrNull -InputObject $policyChainAudit -Name "head_seq"
if ($null -ne $policyChainAuditHeadSeqRaw -and "$policyChainAuditHeadSeqRaw" -ne "") {
    $policyChainAuditHeadSeq = [int]$policyChainAuditHeadSeqRaw
}
$policyChainAuditRootRaw = Get-PropertyOrNull -InputObject $policyChainAudit -Name "root"
if ($null -ne $policyChainAuditRootRaw) {
    $policyChainAuditRoot = ([string]$policyChainAuditRootRaw).Trim().ToLowerInvariant()
}
$policyChainAuditRootOk = ($policyChainAuditRoot -match '^[0-9a-f]{64}$')
$policyOk = ($policyResp.status -eq 200 -and $policyFee -eq 17 -and $policyChainAuditRootOk)
$chainAuditAfterExecuteHeadSeq = 0
$chainAuditAfterExecuteRoot = ""
$chainAuditAfterExecuteRootOk = $false
$chainAuditAfterExecuteResult = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $chainAuditAfterExecuteResp -Name "json") -Name "result"
$chainAuditAfterExecuteHeadSeqRaw = Get-PropertyOrNull -InputObject $chainAuditAfterExecuteResult -Name "head_seq"
if ($null -ne $chainAuditAfterExecuteHeadSeqRaw -and "$chainAuditAfterExecuteHeadSeqRaw" -ne "") {
    $chainAuditAfterExecuteHeadSeq = [int]$chainAuditAfterExecuteHeadSeqRaw
}
$chainAuditAfterExecuteRootRaw = Get-PropertyOrNull -InputObject $chainAuditAfterExecuteResult -Name "root"
if ($null -ne $chainAuditAfterExecuteRootRaw) {
    $chainAuditAfterExecuteRoot = ([string]$chainAuditAfterExecuteRootRaw).Trim().ToLowerInvariant()
}
$chainAuditAfterExecuteRootOk = ($chainAuditAfterExecuteRoot -match '^[0-9a-f]{64}$')
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
$auditHasSignRejectUnsupportedScheme = $false
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
        $detail = [string](Get-PropertyOrNull -InputObject $event -Name "detail")
        if ($action -eq "sign" -and $outcome -eq "ok") { $auditHasSignOk = $true }
        if ($action -eq "execute" -and $outcome -eq "ok") { $auditHasExecuteOk = $true }
        if ($action -eq "submit" -and $outcome -eq "reject") { $auditHasSubmitReject = $true }
        if ($action -eq "sign" -and $outcome -eq "reject" -and $detail.ToLowerInvariant().Contains("unsupported signature scheme")) {
            $auditHasSignRejectUnsupportedScheme = $true
        }
    }
}
$auditOk = ($auditResp.status -eq 200 -and $null -eq $auditErr -and $auditCount -ge 6 -and $auditHasSignOk -and $auditHasExecuteOk -and $auditHasSubmitReject -and $auditHasSignRejectUnsupportedScheme)
$auditPersistCount = 0
$auditPersistNextSeq = 0
$auditPersistHasSignOk = $false
$auditPersistHasExecuteOk = $false
$auditPersistHasSubmitReject = $false
$auditPersistHasSignRejectUnsupportedScheme = $false
$auditPersistOk = $false
$auditPersistJson = Read-JsonFile -Path $auditDbPath
if ($null -ne $auditPersistJson) {
    $auditPersistCountRaw = Get-PropertyOrNull -InputObject $auditPersistJson -Name "events"
    if ($null -ne $auditPersistCountRaw) {
        $auditPersistCount = @($auditPersistCountRaw).Count
    }
    $auditPersistNextSeqRaw = Get-PropertyOrNull -InputObject $auditPersistJson -Name "next_seq"
    if ($null -ne $auditPersistNextSeqRaw -and "$auditPersistNextSeqRaw" -ne "") {
        $auditPersistNextSeq = [int]$auditPersistNextSeqRaw
    }
    $persistEvents = Get-PropertyOrNull -InputObject $auditPersistJson -Name "events"
    if ($null -ne $persistEvents) {
        foreach ($event in $persistEvents) {
            $action = [string](Get-PropertyOrNull -InputObject $event -Name "action")
            $outcome = [string](Get-PropertyOrNull -InputObject $event -Name "outcome")
            $detail = [string](Get-PropertyOrNull -InputObject $event -Name "detail")
            if ($action -eq "sign" -and $outcome -eq "ok") { $auditPersistHasSignOk = $true }
            if ($action -eq "execute" -and $outcome -eq "ok") { $auditPersistHasExecuteOk = $true }
            if ($action -eq "submit" -and $outcome -eq "reject") { $auditPersistHasSubmitReject = $true }
            if ($action -eq "sign" -and $outcome -eq "reject" -and $detail.ToLowerInvariant().Contains("unsupported signature scheme")) {
                $auditPersistHasSignRejectUnsupportedScheme = $true
            }
        }
    }
}
$auditPersistOk = (
    (Test-Path $auditDbPath) -and
    $auditPersistCount -ge $auditCount -and
    $auditPersistNextSeq -ge $auditPersistCount -and
    $auditPersistHasSignOk -and
    $auditPersistHasExecuteOk -and
    $auditPersistHasSubmitReject -and
    $auditPersistHasSignRejectUnsupportedScheme
)
$listOk = ($listResp.status -eq 200 -and $null -eq $listErr)
$chainAuditCount = 0
$chainAuditHeadSeq = 0
$chainAuditRoot = ""
$chainAuditRootOk = $false
$chainAuditHasSubmitAccepted = $false
$chainAuditHasExecuteApplied = $false
$chainAuditHasExecuteAppliedVerifier = $false
$chainAuditResult = Get-PropertyOrNull -InputObject (Get-PropertyOrNull -InputObject $chainAuditResp -Name "json") -Name "result"
$chainAuditCountRaw = Get-PropertyOrNull -InputObject $chainAuditResult -Name "count"
if ($null -ne $chainAuditCountRaw -and "$chainAuditCountRaw" -ne "") {
    $chainAuditCount = [int]$chainAuditCountRaw
}
$chainAuditHeadSeqRaw = Get-PropertyOrNull -InputObject $chainAuditResult -Name "head_seq"
if ($null -ne $chainAuditHeadSeqRaw -and "$chainAuditHeadSeqRaw" -ne "") {
    $chainAuditHeadSeq = [int]$chainAuditHeadSeqRaw
}
$chainAuditRootRaw = Get-PropertyOrNull -InputObject $chainAuditResult -Name "root"
if ($null -ne $chainAuditRootRaw) {
    $chainAuditRoot = ([string]$chainAuditRootRaw).Trim().ToLowerInvariant()
}
$chainAuditRootOk = ($chainAuditRoot -match '^[0-9a-f]{64}$')
$chainAuditEvents = Get-PropertyOrNull -InputObject $chainAuditResult -Name "events"
if ($null -ne $chainAuditEvents) {
    foreach ($event in $chainAuditEvents) {
        $action = [string](Get-PropertyOrNull -InputObject $event -Name "action")
        $outcome = [string](Get-PropertyOrNull -InputObject $event -Name "outcome")
        $detail = [string](Get-PropertyOrNull -InputObject $event -Name "detail")
        if ($action -eq "submit" -and $outcome -eq "accepted") { $chainAuditHasSubmitAccepted = $true }
        if ($action -eq "execute" -and $outcome -eq "applied") {
            $chainAuditHasExecuteApplied = $true
            $detailLower = $detail.ToLowerInvariant()
            if (
                $detailLower.Contains("verifier=") -and
                $detailLower.Contains("signature_scheme=")
            ) {
                $chainAuditHasExecuteAppliedVerifier = $true
            }
        }
    }
}
$chainAuditOk = (
    $chainAuditResp.status -eq 200 -and
    $null -eq $chainAuditErr -and
    $chainAuditCount -ge 2 -and
    $chainAuditHeadSeq -ge $chainAuditCount -and
    $chainAuditRootOk -and
    $chainAuditHasSubmitAccepted -and
    $chainAuditHasExecuteApplied -and
    $chainAuditHasExecuteAppliedVerifier
)
$policyChainAuditConsistencyOk = (
    $chainAuditAfterExecuteResp.status -eq 200 -and
    $null -eq $chainAuditAfterExecuteErr -and
    $chainAuditAfterExecuteRootOk -and
    $policyChainAuditRootOk -and
    $policyChainAuditRoot -eq $chainAuditAfterExecuteRoot -and
    $policyChainAuditHeadSeq -eq $chainAuditAfterExecuteHeadSeq
)
$chainAuditPersistCount = 0
$chainAuditPersistHeadSeq = 0
$chainAuditPersistRoot = ""
$chainAuditPersistRootOk = $false
$chainAuditPersistHasSubmitAccepted = $false
$chainAuditPersistHasExecuteApplied = $false
$chainAuditPersistHasExecuteAppliedVerifier = $false
$chainAuditPersistOk = $false
$chainAuditPersistJson = Read-JsonFile -Path $chainAuditDbPath
if ($null -ne $chainAuditPersistJson) {
    $chainAuditPersistRootRaw = Get-PropertyOrNull -InputObject $chainAuditPersistJson -Name "root_hex"
    if ($null -ne $chainAuditPersistRootRaw) {
        $chainAuditPersistRoot = ([string]$chainAuditPersistRootRaw).Trim().ToLowerInvariant()
    }
    $chainAuditPersistRootOk = (
        $chainAuditPersistRoot -match '^[0-9a-f]{64}$' -and
        ($chainAuditRoot -eq "" -or $chainAuditPersistRoot -eq $chainAuditRoot)
    )
    $chainPersistEvents = Get-PropertyOrNull -InputObject $chainAuditPersistJson -Name "events"
    if ($null -ne $chainPersistEvents) {
        $chainAuditPersistCount = @($chainPersistEvents).Count
        foreach ($event in $chainPersistEvents) {
            $seqRaw = Get-PropertyOrNull -InputObject $event -Name "seq"
            if ($null -ne $seqRaw -and "$seqRaw" -ne "") {
                $seq = [int]$seqRaw
                if ($seq -gt $chainAuditPersistHeadSeq) {
                    $chainAuditPersistHeadSeq = $seq
                }
            }
            $action = [string](Get-PropertyOrNull -InputObject $event -Name "action")
            $outcome = [string](Get-PropertyOrNull -InputObject $event -Name "outcome")
            $detail = [string](Get-PropertyOrNull -InputObject $event -Name "detail")
            if ($action -eq "submit" -and $outcome -eq "accepted") { $chainAuditPersistHasSubmitAccepted = $true }
            if ($action -eq "execute" -and $outcome -eq "applied") {
                $chainAuditPersistHasExecuteApplied = $true
                $detailLower = $detail.ToLowerInvariant()
                if (
                    $detailLower.Contains("verifier=") -and
                    $detailLower.Contains("signature_scheme=")
                ) {
                    $chainAuditPersistHasExecuteAppliedVerifier = $true
                }
            }
        }
    }
}
$chainAuditPersistOk = (
    (Test-Path $chainAuditDbPath) -and
    $chainAuditPersistCount -ge 2 -and
    $chainAuditPersistHeadSeq -ge $chainAuditPersistCount -and
    $chainAuditPersistRootOk -and
    $chainAuditPersistHasSubmitAccepted -and
    $chainAuditPersistHasExecuteApplied -and
    $chainAuditPersistHasExecuteAppliedVerifier
)
$chainAuditRestartRootOk = (
    $chainAuditRestartRootOk -and
    ($chainAuditRoot -eq "" -or $chainAuditRestartRoot -eq $chainAuditRoot)
)
$chainAuditRestartHasExecuteAppliedVerifier = $false
$chainAuditRestartJson = Read-JsonFile -Path $chainAuditDbPath
if ($null -ne $chainAuditRestartJson) {
    $restartEvents = Get-PropertyOrNull -InputObject $chainAuditRestartJson -Name "events"
    if ($null -ne $restartEvents) {
        foreach ($event in $restartEvents) {
            $action = [string](Get-PropertyOrNull -InputObject $event -Name "action")
            $outcome = [string](Get-PropertyOrNull -InputObject $event -Name "outcome")
            if ($action -eq "execute" -and $outcome -eq "applied") {
                $detail = [string](Get-PropertyOrNull -InputObject $event -Name "detail")
                $detailLower = $detail.ToLowerInvariant()
                if (
                    $detailLower.Contains("verifier=") -and
                    $detailLower.Contains("signature_scheme=")
                ) {
                    $chainAuditRestartHasExecuteAppliedVerifier = $true
                }
            }
        }
    }
}
$chainAuditRestartOk = ($chainAuditRestartOk -and $chainAuditRestartRootOk)
$processedOk = ($processed -eq $ExpectedRequests)

$pass = [bool](
    $voteVerifierStartupOk -and
    $voteVerifierStagedRejectOk -and
    $submitParam2Ok -and
    $sign1Ok -and
    $signUnsupportedSchemeRejectOk -and
    $vote0Ok -and
    $vote1SignedOk -and
    $executeOk -and
    $executeVoteVerifierOk -and
    $policyOk -and
    $policyChainAuditConsistencyOk -and
    $unauthorizedSubmitRejectOk -and
    $submitSlashOk -and
    $slashSignOk -and
    $slashVote0Ok -and
    $duplicateRejectOk -and
    $auditOk -and
    $auditPersistOk -and
    $listOk -and
    $chainAuditOk -and
    $chainAuditPersistOk -and
    $chainAuditRestartOk -and
    $chainAuditRestartHasExecuteAppliedVerifier -and
    $processedOk
)

if (-not $voteVerifierStartupOk) { $errorReason = "vote_verifier_startup_invalid" }
elseif (-not $voteVerifierStagedRejectOk) { $errorReason = "vote_verifier_policy_reject_failed" }
elseif (-not $submitParam2Ok) { $errorReason = "submit_param2_failed" }
elseif (-not $sign1Ok) { $errorReason = "sign1_failed" }
elseif (-not $signUnsupportedSchemeRejectOk) { $errorReason = "sign_unsupported_scheme_not_rejected" }
elseif (-not $vote0Ok) { $errorReason = "vote0_failed" }
elseif (-not $vote1SignedOk) { $errorReason = "vote1_signed_failed" }
elseif (-not $executeOk) { $errorReason = "execute_param2_failed" }
elseif (-not $executeVoteVerifierOk) { $errorReason = "execute_vote_verifier_missing" }
elseif (-not $policyOk) { $errorReason = "policy_not_applied" }
elseif (-not $policyChainAuditConsistencyOk) { $errorReason = "policy_chain_audit_mismatch" }
elseif (-not $unauthorizedSubmitRejectOk) { $errorReason = "unauthorized_submit_not_rejected" }
elseif (-not $submitSlashOk) { $errorReason = "submit_slash_failed" }
elseif (-not $slashSignOk) { $errorReason = "slash_sign_failed" }
elseif (-not $slashVote0Ok) { $errorReason = "slash_vote0_failed" }
elseif (-not $duplicateRejectOk) { $errorReason = "duplicate_vote_not_rejected" }
elseif (-not $auditOk) { $errorReason = "audit_events_invalid" }
elseif (-not $auditPersistOk) { $errorReason = "audit_events_not_persisted" }
elseif (-not $listOk) { $errorReason = "list_proposals_failed" }
elseif (-not $chainAuditOk) { $errorReason = "chain_audit_events_invalid" }
elseif (-not $chainAuditPersistOk) { $errorReason = "chain_audit_events_not_persisted" }
elseif (-not $chainAuditRestartOk) { $errorReason = "chain_audit_restart_probe_failed" }
elseif (-not $chainAuditRestartHasExecuteAppliedVerifier) { $errorReason = "chain_audit_restart_missing_execute_verifier" }
elseif (-not $processedOk) { $errorReason = "processed_count_mismatch" }

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    error_reason = $errorReason
    bind = $Bind
    expected_requests = $ExpectedRequests
    processed_requests = $processed
    vote_verifier_configured = $voteVerifierConfigured
    vote_verifier_active = $voteVerifierActive
    vote_verifier_startup_ok = $voteVerifierStartupOk
    vote_verifier_startup_line = $voteVerifierLine
    vote_verifier_policy_reject_ok = $voteVerifierStagedRejectOk
    vote_verifier_policy_reject_exit_code = $voteVerifierRejectExitCode
    vote_verifier_policy_reject_error_message = $voteVerifierRejectErrorMessage
    vote_verifier_policy_reject_stdout_log = $voteVerifierRejectStdoutPath
    vote_verifier_policy_reject_stderr_log = $voteVerifierRejectStderrPath
    vote_verifier_staged_reject_ok = $voteVerifierStagedRejectOk
    vote_verifier_staged_reject_exit_code = $voteVerifierRejectExitCode
    vote_verifier_staged_reject_error_message = $voteVerifierRejectErrorMessage
    vote_verifier_staged_reject_stdout_log = $voteVerifierRejectStdoutPath
    vote_verifier_staged_reject_stderr_log = $voteVerifierRejectStderrPath
    submit_param2_ok = $submitParam2Ok
    sign1_ok = $sign1Ok
    sign_unsupported_scheme_reject_ok = $signUnsupportedSchemeRejectOk
    vote0_ok = $vote0Ok
    vote1_signed_ok = $vote1SignedOk
    execute_ok = $executeOk
    execute_vote_verifier_name = $executeVoteVerifierName
    execute_vote_verifier_scheme = $executeVoteVerifierScheme
    execute_vote_verifier_ok = $executeVoteVerifierOk
    policy_ok = $policyOk
    policy_chain_audit_consistency_ok = $policyChainAuditConsistencyOk
    policy_chain_audit_head_seq = $policyChainAuditHeadSeq
    policy_chain_audit_root = $policyChainAuditRoot
    policy_chain_audit_root_ok = $policyChainAuditRootOk
    chain_audit_after_execute_head_seq = $chainAuditAfterExecuteHeadSeq
    chain_audit_after_execute_root = $chainAuditAfterExecuteRoot
    chain_audit_after_execute_root_ok = $chainAuditAfterExecuteRootOk
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
    audit_has_sign_reject_unsupported_scheme = $auditHasSignRejectUnsupportedScheme
    audit_persist_ok = $auditPersistOk
    audit_persist_count = $auditPersistCount
    audit_persist_next_seq = $auditPersistNextSeq
    audit_persist_has_sign_ok = $auditPersistHasSignOk
    audit_persist_has_execute_ok = $auditPersistHasExecuteOk
    audit_persist_has_submit_reject = $auditPersistHasSubmitReject
    audit_persist_has_sign_reject_unsupported_scheme = $auditPersistHasSignRejectUnsupportedScheme
    audit_persist_path = $auditDbPath
    list_ok = $listOk
    chain_audit_ok = $chainAuditOk
    chain_audit_count = $chainAuditCount
    chain_audit_head_seq = $chainAuditHeadSeq
    chain_audit_root = $chainAuditRoot
    chain_audit_root_ok = $chainAuditRootOk
    chain_audit_has_submit_accepted = $chainAuditHasSubmitAccepted
    chain_audit_has_execute_applied = $chainAuditHasExecuteApplied
    chain_audit_has_execute_applied_verifier = $chainAuditHasExecuteAppliedVerifier
    chain_audit_persist_ok = $chainAuditPersistOk
    chain_audit_persist_count = $chainAuditPersistCount
    chain_audit_persist_head_seq = $chainAuditPersistHeadSeq
    chain_audit_persist_root = $chainAuditPersistRoot
    chain_audit_persist_root_ok = $chainAuditPersistRootOk
    chain_audit_persist_has_submit_accepted = $chainAuditPersistHasSubmitAccepted
    chain_audit_persist_has_execute_applied = $chainAuditPersistHasExecuteApplied
    chain_audit_persist_has_execute_applied_verifier = $chainAuditPersistHasExecuteAppliedVerifier
    chain_audit_persist_path = $chainAuditDbPath
    chain_audit_restart_ok = $chainAuditRestartOk
    chain_audit_restart_count = $chainAuditRestartCount
    chain_audit_restart_head_seq = $chainAuditRestartHeadSeq
    chain_audit_restart_root = $chainAuditRestartRoot
    chain_audit_restart_root_ok = $chainAuditRestartRootOk
    chain_audit_restart_has_submit_accepted = $chainAuditRestartHasSubmitAccepted
    chain_audit_restart_has_execute_applied = $chainAuditRestartHasExecuteApplied
    chain_audit_restart_has_execute_applied_verifier = $chainAuditRestartHasExecuteAppliedVerifier
    chain_audit_restart_error_message = $chainAuditRestartErrorMessage
    chain_audit_restart_stdout_log = $chainAuditRestartStdoutPath
    chain_audit_restart_stderr_log = $chainAuditRestartStderrPath
    stdout_log = $stdoutPath
    stderr_log = $stderrPath
    policy_fee_after_execute = $policyFee
    unauthorized_submit_error_message = $unauthorizedSubmitErrMsg
    duplicate_error_message = $duplicateErrMsg
    sign_unsupported_scheme_error_message = $signUnsupportedSchemeErrMsg
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
    "- vote_verifier_configured: $($summary.vote_verifier_configured)"
    "- vote_verifier_active: $($summary.vote_verifier_active)"
    "- vote_verifier_startup_ok: $($summary.vote_verifier_startup_ok)"
    "- vote_verifier_startup_line: $($summary.vote_verifier_startup_line)"
    "- vote_verifier_policy_reject_ok: $($summary.vote_verifier_policy_reject_ok)"
    "- vote_verifier_policy_reject_exit_code: $($summary.vote_verifier_policy_reject_exit_code)"
    "- vote_verifier_policy_reject_error_message: $($summary.vote_verifier_policy_reject_error_message)"
    "- vote_verifier_policy_reject_stdout_log: $($summary.vote_verifier_policy_reject_stdout_log)"
    "- vote_verifier_policy_reject_stderr_log: $($summary.vote_verifier_policy_reject_stderr_log)"
    "- vote_verifier_staged_reject_ok: $($summary.vote_verifier_staged_reject_ok)"
    "- vote_verifier_staged_reject_exit_code: $($summary.vote_verifier_staged_reject_exit_code)"
    "- vote_verifier_staged_reject_error_message: $($summary.vote_verifier_staged_reject_error_message)"
    "- vote_verifier_staged_reject_stdout_log: $($summary.vote_verifier_staged_reject_stdout_log)"
    "- vote_verifier_staged_reject_stderr_log: $($summary.vote_verifier_staged_reject_stderr_log)"
    "- submit_param2_ok: $($summary.submit_param2_ok)"
    "- sign1_ok: $($summary.sign1_ok)"
    "- sign_unsupported_scheme_reject_ok: $($summary.sign_unsupported_scheme_reject_ok)"
    "- vote0_ok: $($summary.vote0_ok)"
    "- vote1_signed_ok: $($summary.vote1_signed_ok)"
    "- execute_ok: $($summary.execute_ok)"
    "- execute_vote_verifier_name: $($summary.execute_vote_verifier_name)"
    "- execute_vote_verifier_scheme: $($summary.execute_vote_verifier_scheme)"
    "- execute_vote_verifier_ok: $($summary.execute_vote_verifier_ok)"
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
    "- audit_has_sign_reject_unsupported_scheme: $($summary.audit_has_sign_reject_unsupported_scheme)"
    "- audit_persist_ok: $($summary.audit_persist_ok)"
    "- audit_persist_count: $($summary.audit_persist_count)"
    "- audit_persist_next_seq: $($summary.audit_persist_next_seq)"
    "- audit_persist_has_sign_ok: $($summary.audit_persist_has_sign_ok)"
    "- audit_persist_has_execute_ok: $($summary.audit_persist_has_execute_ok)"
    "- audit_persist_has_submit_reject: $($summary.audit_persist_has_submit_reject)"
    "- audit_persist_has_sign_reject_unsupported_scheme: $($summary.audit_persist_has_sign_reject_unsupported_scheme)"
    "- audit_persist_path: $($summary.audit_persist_path)"
    "- list_ok: $($summary.list_ok)"
    "- chain_audit_ok: $($summary.chain_audit_ok)"
    "- chain_audit_count: $($summary.chain_audit_count)"
    "- chain_audit_head_seq: $($summary.chain_audit_head_seq)"
    "- chain_audit_has_submit_accepted: $($summary.chain_audit_has_submit_accepted)"
    "- chain_audit_has_execute_applied: $($summary.chain_audit_has_execute_applied)"
    "- chain_audit_has_execute_applied_verifier: $($summary.chain_audit_has_execute_applied_verifier)"
    "- chain_audit_persist_ok: $($summary.chain_audit_persist_ok)"
    "- chain_audit_persist_count: $($summary.chain_audit_persist_count)"
    "- chain_audit_persist_head_seq: $($summary.chain_audit_persist_head_seq)"
    "- chain_audit_persist_has_submit_accepted: $($summary.chain_audit_persist_has_submit_accepted)"
    "- chain_audit_persist_has_execute_applied: $($summary.chain_audit_persist_has_execute_applied)"
    "- chain_audit_persist_has_execute_applied_verifier: $($summary.chain_audit_persist_has_execute_applied_verifier)"
    "- chain_audit_persist_path: $($summary.chain_audit_persist_path)"
    "- chain_audit_restart_ok: $($summary.chain_audit_restart_ok)"
    "- chain_audit_restart_count: $($summary.chain_audit_restart_count)"
    "- chain_audit_restart_head_seq: $($summary.chain_audit_restart_head_seq)"
    "- chain_audit_restart_has_submit_accepted: $($summary.chain_audit_restart_has_submit_accepted)"
    "- chain_audit_restart_has_execute_applied: $($summary.chain_audit_restart_has_execute_applied)"
    "- chain_audit_restart_has_execute_applied_verifier: $($summary.chain_audit_restart_has_execute_applied_verifier)"
    "- chain_audit_restart_error_message: $($summary.chain_audit_restart_error_message)"
    "- chain_audit_restart_stdout_log: $($summary.chain_audit_restart_stdout_log)"
    "- chain_audit_restart_stderr_log: $($summary.chain_audit_restart_stderr_log)"
    "- policy_fee_after_execute: $($summary.policy_fee_after_execute)"
    "- unauthorized_submit_error_message: $($summary.unauthorized_submit_error_message)"
    "- duplicate_error_message: $($summary.duplicate_error_message)"
    "- sign_unsupported_scheme_error_message: $($summary.sign_unsupported_scheme_error_message)"
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
