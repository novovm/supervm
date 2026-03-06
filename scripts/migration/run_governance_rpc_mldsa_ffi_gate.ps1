param(
    [string]$RepoRoot = "",
    [string]$AoemRoot = "",
    [string]$OutputDir = "",
    [string]$Bind = "127.0.0.1:8902",
    [ValidateRange(1, 64)]
    [int]$ExpectedRequests = 9,
    [ValidateRange(1, 30)]
    [int]$StartupTimeoutSeconds = 12,
    [ValidateRange(1, 30)]
    [int]$ExitTimeoutSeconds = 12
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$IsWindowsHost = ($env:OS -eq "Windows_NT")
$IsMacOsHost = $false
if (-not $IsWindowsHost) {
    try {
        $uname = (& uname).Trim()
        if ($uname -eq "Darwin") {
            $IsMacOsHost = $true
        }
    } catch {
        $IsMacOsHost = $false
    }
}

if (-not $RepoRoot) { $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path } else { $RepoRoot = (Resolve-Path $RepoRoot).Path }
if (-not $AoemRoot) { $AoemRoot = Join-Path (Split-Path $RepoRoot -Parent) "AOEM" }
$AoemRoot = (Resolve-Path $AoemRoot).Path
if (-not $OutputDir) { $OutputDir = Join-Path $RepoRoot "artifacts\migration\governance-rpc-mldsa-ffi-gate" }
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Invoke-Cargo([string]$WorkDir, [string[]]$Args) {
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($Args | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")
    $p = [System.Diagnostics.Process]::Start($psi)
    $stdout = $p.StandardOutput.ReadToEnd()
    $stderr = $p.StandardError.ReadToEnd()
    $p.WaitForExit()
    if ($p.ExitCode -ne 0) { throw "cargo $($Args -join ' ') failed in $WorkDir`n$stdout`n$stderr" }
    return ($stdout + $stderr).Trim()
}

function Resolve-FirstPath([string[]]$Candidates) {
    foreach ($path in $Candidates) { if (Test-Path $path) { return (Resolve-Path $path).Path } }
    return ""
}

function Wait-Endpoint([string]$HostName, [int]$Port, [int]$TimeoutSeconds) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $c = [System.Net.Sockets.TcpClient]::new()
        try {
            $a = $c.BeginConnect($HostName, $Port, $null, $null)
            if ($a.AsyncWaitHandle.WaitOne(180)) { $c.EndConnect($a); return $true }
        } catch {} finally { $c.Dispose() }
        Start-Sleep -Milliseconds 120
    }
    return $false
}

function Post-Rpc([string]$Uri, [string]$Body) {
    $params = @{
        Uri = $Uri
        Method = "Post"
        ContentType = "application/json; charset=utf-8"
        Body = $Body
        UseBasicParsing = $true
    }
    $webCmd = Get-Command Invoke-WebRequest -ErrorAction Stop
    if ($webCmd.Parameters.ContainsKey("SkipHttpErrorCheck")) {
        $params["SkipHttpErrorCheck"] = $true
    }
    $resp = Invoke-WebRequest @params
    $json = $null
    if ($resp.Content) {
        try { $json = Parse-Json $resp.Content } catch { $json = $null }
    }
    return [ordered]@{ status = [int]$resp.StatusCode; body = [string]$resp.Content; json = $json }
}

function Rpc-Body([int]$Id, [string]$Method, [object]$Params) {
    return ([ordered]@{ jsonrpc = "2.0"; id = $Id; method = $Method; params = $Params } | ConvertTo-Json -Depth 16 -Compress)
}

function Prop([object]$Obj, [string]$Name) {
    if ($null -eq $Obj) { return $null }
    if ($Obj -is [System.Collections.IDictionary]) {
        if ($Obj.PSObject.Methods.Name -contains "ContainsKey") {
            if ($Obj.ContainsKey($Name)) { return $Obj[$Name] }
            return $null
        }
        if ($Obj.Contains($Name)) { return $Obj[$Name] }
        return $null
    }
    if ($Obj.PSObject.Properties.Name -contains $Name) { return $Obj.$Name }
    return $null
}

function Hex-Decode([string]$Raw) {
    $s = $Raw.Trim()
    if (($s.Length % 2) -ne 0) { throw "invalid hex length" }
    $b = New-Object byte[] ($s.Length / 2)
    for ($i = 0; $i -lt $b.Length; $i++) { $b[$i] = [Convert]::ToByte($s.Substring($i * 2, 2), 16) }
    return ,$b
}

function Hex-Encode([byte[]]$Bytes) { return (($Bytes | ForEach-Object { $_.ToString("x2") }) -join "") }

function U64LE([UInt64]$V) {
    $b = [BitConverter]::GetBytes($V)
    if (-not [BitConverter]::IsLittleEndian) { [Array]::Reverse($b) }
    return ,$b
}

function U32LE([UInt32]$V) {
    $b = [BitConverter]::GetBytes($V)
    if (-not [BitConverter]::IsLittleEndian) { [Array]::Reverse($b) }
    return ,$b
}

function I64LE([Int64]$V) {
    $b = [BitConverter]::GetBytes($V)
    if (-not [BitConverter]::IsLittleEndian) { [Array]::Reverse($b) }
    return ,$b
}

function Compute-MempoolFeeFloorProposalDigestHex([UInt64]$ProposalId, [UInt32]$Proposer, [UInt64]$CreatedHeight, [Int64]$FeeFloor) {
    $data = New-Object System.Collections.Generic.List[byte]
    $data.AddRange([System.Text.Encoding]::ASCII.GetBytes("GOV_PROPOSAL_V1:"))
    $data.AddRange([byte[]](U64LE $ProposalId))
    $data.AddRange([byte[]](U32LE $Proposer))
    $data.AddRange([byte[]](U64LE $CreatedHeight))
    $data.Add(2)
    $data.AddRange([byte[]](I64LE $FeeFloor))
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $digestBytes = $sha.ComputeHash($data.ToArray())
    } finally {
        $sha.Dispose()
    }
    return (Hex-Encode $digestBytes)
}

function Build-VoteMsgHex([UInt64]$ProposalId, [UInt64]$Height, [string]$DigestHex, [bool]$Support) {
    $digest = @((Hex-Decode $DigestHex))
    if ($digest.Count -ne 32) { throw "proposal_digest must be 32 bytes" }
    [byte[]]$digestBytes = $digest
    $list = New-Object System.Collections.Generic.List[byte]
    $list.AddRange([System.Text.Encoding]::ASCII.GetBytes("GOV_VOTE_V1:"))
    $list.AddRange([byte[]](U64LE $ProposalId))
    $list.AddRange([byte[]](U64LE $Height))
    $list.AddRange($digestBytes)
    $supportByte = if ($Support) { [byte]1 } else { [byte]0 }
    $list.Add($supportByte)
    return (Hex-Encode $list.ToArray())
}

function Parse-Json([string]$Raw) {
    $convertFromJsonCmd = Get-Command ConvertFrom-Json -ErrorAction Stop
    if ($convertFromJsonCmd.Parameters.ContainsKey("Depth")) {
        return ($Raw | ConvertFrom-Json -Depth 32)
    }
    try {
        Add-Type -AssemblyName System.Web.Extensions -ErrorAction SilentlyContinue
        $serializer = New-Object System.Web.Script.Serialization.JavaScriptSerializer
        $serializer.MaxJsonLength = [int]::MaxValue
        return $serializer.DeserializeObject($Raw)
    } catch {
        return ($Raw | ConvertFrom-Json)
    }
}

function Invoke-SignerExe([string]$SignerExe, [string[]]$SignerArgs) {
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $SignerExe
    $psi.WorkingDirectory = (Split-Path $SignerExe -Parent)
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($SignerArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")
    $p = [System.Diagnostics.Process]::Start($psi)
    $stdout = $p.StandardOutput.ReadToEnd()
    $stderr = $p.StandardError.ReadToEnd()
    $p.WaitForExit()
    if ($p.ExitCode -ne 0) {
        throw "signer failed: args=$($SignerArgs -join ' ')`n$stdout`n$stderr"
    }
    return (Parse-Json $stdout)
}

$nodeCrate = Join-Path $RepoRoot "crates\novovm-node"
Invoke-Cargo -WorkDir $nodeCrate -Args @("build", "--quiet", "--bin", "novovm-node") | Out-Null

$nodeExeName = if ($IsWindowsHost) { "novovm-node.exe" } else { "novovm-node" }
$nodeExe = Resolve-FirstPath @(
    (Join-Path $RepoRoot "target\debug\$nodeExeName"),
    (Join-Path $nodeCrate "target\debug\$nodeExeName")
)
if (-not $nodeExe) { throw "missing novovm-node binary after build" }

$aoemFfiManifest = Join-Path $AoemRoot "crates\ffi\aoem-ffi\Cargo.toml"
Invoke-Cargo -WorkDir $AoemRoot -Args @("build", "--quiet", "--manifest-path", $aoemFfiManifest, "--features", "mldsa") | Out-Null

$aoemLibName = if ($IsWindowsHost) { "aoem_ffi.dll" } elseif ($IsMacOsHost) { "libaoem_ffi.dylib" } else { "libaoem_ffi.so" }
$aoemLibPath = Resolve-FirstPath @(
    (Join-Path $AoemRoot "cargo-target\debug\$aoemLibName"),
    (Join-Path $AoemRoot "cargo-target\release\$aoemLibName"),
    (Join-Path $AoemRoot "target\debug\$aoemLibName"),
    (Join-Path $AoemRoot "target\release\$aoemLibName")
)
if (-not $aoemLibPath) { throw "missing AOEM FFI library ($aoemLibName) after build" }

$signerManifest = Join-Path $RepoRoot "scripts\migration\mldsa87-vote-signer\Cargo.toml"
if (-not (Test-Path $signerManifest)) { throw "missing signer manifest: $signerManifest" }
Invoke-Cargo -WorkDir $RepoRoot -Args @("build", "--quiet", "--manifest-path", $signerManifest) | Out-Null
$signerExeName = if ($IsWindowsHost) { "mldsa87-vote-signer.exe" } else { "mldsa87-vote-signer" }
$signerExe = Resolve-FirstPath @(
    (Join-Path $RepoRoot "target\debug\$signerExeName"),
    (Join-Path (Join-Path (Split-Path $signerManifest -Parent) "target\debug") $signerExeName)
)
if (-not $signerExe) { throw "missing signer binary after build: $signerExeName" }

$k0 = Invoke-SignerExe -SignerExe $signerExe -SignerArgs @("keygen")
$k1 = Invoke-SignerExe -SignerExe $signerExe -SignerArgs @("keygen")
$pub0 = [string](Prop $k0 "pubkey_hex")
$sec0 = [string](Prop $k0 "secret_hex")
$pub1 = [string](Prop $k1 "pubkey_hex")
$sec1 = [string](Prop $k1 "secret_hex")
if ([string]::IsNullOrWhiteSpace($pub0) -or [string]::IsNullOrWhiteSpace($sec0) -or [string]::IsNullOrWhiteSpace($pub1) -or [string]::IsNullOrWhiteSpace($sec1)) {
    throw "signer keygen returned empty fields"
}

$dbPath = Join-Path $OutputDir "query-db.json"
'{"blocks":[],"txs":{},"receipts":{},"balances":{}}' | Set-Content -Path $dbPath -Encoding UTF8
$auditPath = Join-Path $OutputDir "governance-audit-events.json"
if (Test-Path $auditPath) { Remove-Item -Path $auditPath -Force }

$stdoutLog = Join-Path $OutputDir "governance-rpc-mldsa-ffi.stdout.log"
$stderrLog = Join-Path $OutputDir "governance-rpc-mldsa-ffi.stderr.log"
$endpoint = "http://$Bind/rpc"
$uri = [Uri]("http://$Bind")
$proc = $null
$steps = @{}
$voteVerifierLine = ""
$processed = 0

try {
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $nodeExe
    $psi.WorkingDirectory = $RepoRoot
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Environment["NOVOVM_NODE_MODE"] = "rpc_server"
    $psi.Environment["NOVOVM_CHAIN_QUERY_DB"] = $dbPath
    $psi.Environment["NOVOVM_GOVERNANCE_AUDIT_DB"] = $auditPath
    $psi.Environment["NOVOVM_ENABLE_PUBLIC_RPC"] = "0"
    $psi.Environment["NOVOVM_ENABLE_GOV_RPC"] = "1"
    $psi.Environment["NOVOVM_GOV_RPC_BIND"] = $Bind
    $psi.Environment["NOVOVM_GOV_RPC_MAX_REQUESTS"] = "$ExpectedRequests"
    $psi.Environment["NOVOVM_GOV_RPC_ALLOWLIST"] = "127.0.0.1"
    $psi.Environment["NOVOVM_GOVERNANCE_PROPOSER_ALLOWLIST"] = "0"
    $psi.Environment["NOVOVM_GOVERNANCE_EXECUTOR_ALLOWLIST"] = "0"
    $psi.Environment["NOVOVM_GOVERNANCE_VOTE_VERIFIER"] = "mldsa87"
    $psi.Environment["NOVOVM_GOVERNANCE_MLDSA_MODE"] = "aoem_ffi"
    $psi.Environment["NOVOVM_AOEM_FFI_LIB_PATH"] = $aoemLibPath
    $psi.Environment["NOVOVM_GOVERNANCE_MLDSA87_PUBKEYS"] = "0:$pub0,1:$pub1"
    $proc = [System.Diagnostics.Process]::Start($psi)

    if (-not (Wait-Endpoint -HostName $uri.Host -Port $uri.Port -TimeoutSeconds $StartupTimeoutSeconds)) { throw "rpc server did not listen on $Bind" }

    $steps.submit = Post-Rpc -Uri $endpoint -Body (Rpc-Body 1 "governance_submitProposal" @{ proposer = 0; op = "update_mempool_fee_floor"; fee_floor = 23 })
    $steps.sign_reject = Post-Rpc -Uri $endpoint -Body (Rpc-Body 2 "governance_sign" @{ proposal_id = 1; signer_id = 0; support = $true; signature_scheme = "mldsa87" })
    $steps.get = Post-Rpc -Uri $endpoint -Body (Rpc-Body 3 "governance_getProposal" @{ proposal_id = 1 })

    $proposal = Prop (Prop $steps.get.json "result") "proposal"
    $proposalIdRaw = Prop $proposal "proposal_id"
    $proposerRaw = Prop $proposal "proposer"
    $heightRaw = Prop $proposal "created_height"
    $digest = [string](Prop $proposal "proposal_digest")
    if ([string]::IsNullOrWhiteSpace($digest)) {
        $op = [string](Prop $proposal "op")
        $payload = Prop $proposal "payload"
        $feeFloorRaw = Prop $payload "fee_floor"
        if ($op -eq "update_mempool_fee_floor" -and $null -ne $proposalIdRaw -and $null -ne $proposerRaw -and $null -ne $heightRaw -and $null -ne $feeFloorRaw) {
            $digest = Compute-MempoolFeeFloorProposalDigestHex -ProposalId ([UInt64]$proposalIdRaw) -Proposer ([UInt32]$proposerRaw) -CreatedHeight ([UInt64]$heightRaw) -FeeFloor ([Int64]$feeFloorRaw)
        }
    }
    if ($null -eq $heightRaw -or [string]::IsNullOrWhiteSpace($digest)) {
        $getBody = [string](Prop $steps.get "body")
        throw "governance_getProposal missing created_height/proposal_digest; body=$getBody"
    }
    $height = [UInt64]$heightRaw
    $msgHex = Build-VoteMsgHex -ProposalId 1 -Height $height -DigestHex $digest -Support $true
    $sig0 = [string](Prop (Invoke-SignerExe -SignerExe $signerExe -SignerArgs @("sign", "--message-hex", $msgHex, "--secret-hex", $sec0)) "signature_hex")
    $sig1 = [string](Prop (Invoke-SignerExe -SignerExe $signerExe -SignerArgs @("sign", "--message-hex", $msgHex, "--secret-hex", $sec1)) "signature_hex")

    $steps.vote0 = Post-Rpc -Uri $endpoint -Body (Rpc-Body 4 "governance_vote" @{ proposal_id = 1; voter_id = 0; support = $true; signature_scheme = "mldsa87"; signature = $sig0; mldsa_pubkey = $pub0 })
    $steps.vote1 = Post-Rpc -Uri $endpoint -Body (Rpc-Body 5 "governance_vote" @{ proposal_id = 1; voter_id = 1; support = $true; signature_scheme = "mldsa87"; signature = $sig1; mldsa_pubkey = $pub1 })
    $steps.execute = Post-Rpc -Uri $endpoint -Body (Rpc-Body 6 "governance_execute" @{ proposal_id = 1; executor = 0 })
    $steps.policy = Post-Rpc -Uri $endpoint -Body (Rpc-Body 7 "governance_getPolicy" @{})
    $steps.audit = Post-Rpc -Uri $endpoint -Body (Rpc-Body 8 "governance_listAuditEvents" @{ limit = 50 })
    $steps.list = Post-Rpc -Uri $endpoint -Body (Rpc-Body 9 "governance_listProposals" @{})

    if (-not $proc.WaitForExit($ExitTimeoutSeconds * 1000)) { try { $proc.Kill() } catch {}; throw "rpc process did not exit within ${ExitTimeoutSeconds}s" }
} finally {
    if ($proc) {
        if (-not $proc.HasExited) { try { $proc.Kill() } catch {} }
        $stdout = $proc.StandardOutput.ReadToEnd()
        $stderr = $proc.StandardError.ReadToEnd()
        $stdout | Set-Content -Path $stdoutLog -Encoding UTF8
        $stderr | Set-Content -Path $stderrLog -Encoding UTF8
        $voteVerifierLine = (($stdout -split "`r?`n" | Where-Object { $_ -like "governance_vote_verifier_in:*" } | Select-Object -Last 1))
        $processedMatch = [regex]::Match($stdout, "processed=(?<n>\d+)")
        if ($processedMatch.Success) { $processed = [int]$processedMatch.Groups["n"].Value }
    }
}

$okSubmit = ($steps.submit.status -eq 200 -and $null -eq (Prop $steps.submit.json "error"))
$signErr = [string](Prop (Prop $steps.sign_reject.json "error") "message")
$okSignReject = ($steps.sign_reject.status -eq 200 -and $signErr.ToLowerInvariant().Contains("does not support local mldsa87 signing"))
$okGet = ($steps.get.status -eq 200 -and $null -eq (Prop $steps.get.json "error"))
$okVote0 = ($steps.vote0.status -eq 200 -and $null -eq (Prop $steps.vote0.json "error"))
$okVote1 = ($steps.vote1.status -eq 200 -and $null -eq (Prop $steps.vote1.json "error"))
$okExecute = ($steps.execute.status -eq 200 -and $null -eq (Prop $steps.execute.json "error"))
$policyFee = [int64](Prop (Prop $steps.policy.json "result") "mempool_fee_floor")
$okPolicy = ($steps.policy.status -eq 200 -and $null -eq (Prop $steps.policy.json "error") -and $policyFee -eq 23)
$okAudit = ($steps.audit.status -eq 200 -and $null -eq (Prop $steps.audit.json "error"))
$okList = ($steps.list.status -eq 200 -and $null -eq (Prop $steps.list.json "error"))
$okVerifier = ($voteVerifierLine.ToLowerInvariant().Contains("configured=mldsa87") -and $voteVerifierLine.ToLowerInvariant().Contains("active_scheme=mldsa87") -and $voteVerifierLine.ToLowerInvariant().Contains("mldsa_mode=aoem_ffi"))
$okProcessed = ($processed -eq $ExpectedRequests)
$pass = [bool]($okVerifier -and $okSubmit -and $okSignReject -and $okGet -and $okVote0 -and $okVote1 -and $okExecute -and $okPolicy -and $okAudit -and $okList -and $okProcessed)

$errorReason = ""
if (-not $pass) {
    if (-not $okVerifier) { $errorReason = "vote_verifier_startup_invalid" }
    elseif (-not $okSubmit) { $errorReason = "submit_failed" }
    elseif (-not $okSignReject) { $errorReason = "mldsa_local_sign_not_rejected" }
    elseif (-not $okGet) { $errorReason = "get_proposal_failed" }
    elseif (-not $okVote0) { $errorReason = "vote0_failed" }
    elseif (-not $okVote1) { $errorReason = "vote1_failed" }
    elseif (-not $okExecute) { $errorReason = "execute_failed" }
    elseif (-not $okPolicy) { $errorReason = "policy_not_applied" }
    elseif (-not $okAudit) { $errorReason = "audit_failed" }
    elseif (-not $okList) { $errorReason = "list_failed" }
    else { $errorReason = "processed_count_mismatch" }
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    error_reason = $errorReason
    bind = $Bind
    expected_requests = $ExpectedRequests
    processed_requests = $processed
    aoem_root = $AoemRoot
    aoem_ffi_library_path = $aoemLibPath
    signer_manifest = $signerManifest
    signer_binary_path = $signerExe
    vote_verifier_line = $voteVerifierLine
    vote_verifier_startup_ok = $okVerifier
    submit_ok = $okSubmit
    sign_mldsa_local_reject_ok = $okSignReject
    get_proposal_ok = $okGet
    vote0_mldsa_ok = $okVote0
    vote1_mldsa_ok = $okVote1
    execute_ok = $okExecute
    policy_ok = $okPolicy
    policy_fee_after_execute = $policyFee
    audit_ok = $okAudit
    list_ok = $okList
    processed_ok = $okProcessed
    sign_mldsa_local_reject_message = $signErr
    stdout_log = $stdoutLog
    stderr_log = $stderrLog
}

$summaryJson = Join-Path $OutputDir "governance-rpc-mldsa-ffi-gate-summary.json"
$summaryMd = Join-Path $OutputDir "governance-rpc-mldsa-ffi-gate-summary.md"
$summary | ConvertTo-Json -Depth 12 | Set-Content -Path $summaryJson -Encoding UTF8
@(
    "# Governance RPC ML-DSA AOEM FFI Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- error_reason: $($summary.error_reason)"
    "- bind: $($summary.bind)"
    "- expected_requests: $($summary.expected_requests)"
    "- processed_requests: $($summary.processed_requests)"
    "- aoem_ffi_library_path: $($summary.aoem_ffi_library_path)"
    "- vote_verifier_line: $($summary.vote_verifier_line)"
    "- policy_fee_after_execute: $($summary.policy_fee_after_execute)"
    "- stdout_log: $($summary.stdout_log)"
    "- stderr_log: $($summary.stderr_log)"
    "- summary_json: $summaryJson"
) -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "governance rpc mldsa ffi gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  error_reason: $($summary.error_reason)"
Write-Host "  summary_json: $summaryJson"

if (-not $summary.pass) { throw "governance rpc mldsa ffi gate FAILED: $($summary.error_reason)" }
Write-Host "governance rpc mldsa ffi gate PASS"
