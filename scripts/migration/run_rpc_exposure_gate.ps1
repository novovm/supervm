param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [string]$PublicBind = "127.0.0.1:8899",
    [string]$GovBind = "127.0.0.1:8901",
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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\rpc-exposure-gate"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

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
    if ($proc.ExitCode -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed in $WorkDir`n$stdout`n$stderr"
    }
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
            if ($async.AsyncWaitHandle.WaitOne(200)) {
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

function Test-TcpEndpoint {
    param(
        [string]$HostName,
        [int]$Port
    )

    $client = [System.Net.Sockets.TcpClient]::new()
    try {
        $async = $client.BeginConnect($HostName, $Port, $null, $null)
        if (-not $async.AsyncWaitHandle.WaitOne(200)) {
            return $false
        }
        $client.EndConnect($async)
        return $true
    } catch {
        return $false
    } finally {
        $client.Dispose()
    }
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

    if ($raw) {
        try {
            $convertFromJsonCmd = Get-Command ConvertFrom-Json -ErrorAction Stop
            if ($convertFromJsonCmd.Parameters.ContainsKey("Depth")) {
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

function Get-JsonPropertyOrNull {
    param(
        [object]$Obj,
        [string]$Name
    )
    if ($null -eq $Obj) {
        return $null
    }
    if ($Obj.PSObject.Properties.Name -contains $Name) {
        return $Obj.$Name
    }
    return $null
}

function Start-RpcServerProcess {
    param(
        [string]$NodeExe,
        [string]$RepoRoot,
        [string]$DbPath,
        [bool]$EnablePublic,
        [bool]$EnableGov,
        [string]$PublicBind,
        [string]$GovBind,
        [int]$PublicMaxRequests,
        [int]$GovMaxRequests
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
    $psi.Environment["NOVOVM_ENABLE_PUBLIC_RPC"] = if ($EnablePublic) { "1" } else { "0" }
    $psi.Environment["NOVOVM_ENABLE_GOV_RPC"] = if ($EnableGov) { "1" } else { "0" }
    $psi.Environment["NOVOVM_PUBLIC_RPC_BIND"] = $PublicBind
    $psi.Environment["NOVOVM_GOV_RPC_BIND"] = $GovBind
    $psi.Environment["NOVOVM_PUBLIC_RPC_MAX_REQUESTS"] = "$PublicMaxRequests"
    $psi.Environment["NOVOVM_GOV_RPC_MAX_REQUESTS"] = "$GovMaxRequests"
    $psi.Environment["NOVOVM_GOV_RPC_ALLOWLIST"] = "127.0.0.1"
    $psi.Environment["NOVOVM_GOVERNANCE_PROPOSER_ALLOWLIST"] = "0"
    $psi.Environment["NOVOVM_GOVERNANCE_EXECUTOR_ALLOWLIST"] = "0"
    return [System.Diagnostics.Process]::Start($psi)
}

function Save-ProcessLogs {
    param(
        [System.Diagnostics.Process]$Process,
        [string]$StdoutPath,
        [string]$StderrPath
    )

    if ($Process) {
        if (-not $Process.HasExited) {
            try { $Process.Kill() } catch {}
        }
        $Process.StandardOutput.ReadToEnd() | Set-Content -Path $StdoutPath -Encoding UTF8
        $Process.StandardError.ReadToEnd() | Set-Content -Path $StderrPath -Encoding UTF8
    } else {
        "" | Set-Content -Path $StdoutPath -Encoding UTF8
        "" | Set-Content -Path $StderrPath -Encoding UTF8
    }
}

$nodeCrateDir = Join-Path $RepoRoot "crates\novovm-node"
if (-not (Test-Path (Join-Path $nodeCrateDir "Cargo.toml"))) {
    throw "missing novovm-node Cargo.toml: $nodeCrateDir"
}
Invoke-Cargo -WorkDir $nodeCrateDir -CargoArgs @("build", "--quiet", "--bin", "novovm-node")

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

$dbPath = Join-Path $OutputDir "rpc-exposure-query-db.json"
'{"blocks":[],"txs":{},"receipts":{},"balances":{"1001":42}}' | Set-Content -Path $dbPath -Encoding UTF8

$publicUri = [Uri]("http://$PublicBind")
$govUri = [Uri]("http://$GovBind")
$publicEndpoint = "http://$PublicBind/rpc"
$govEndpoint = "http://$GovBind/rpc"

$scenarioDefault = [ordered]@{
    pass = $false
    error_reason = ""
    public_reject_ok = $false
    gov_closed_ok = $false
    process_exit_ok = $false
    stdout_log = Join-Path $OutputDir "rpc-exposure.default.stdout.log"
    stderr_log = Join-Path $OutputDir "rpc-exposure.default.stderr.log"
    response = $null
}

$proc = $null
try {
    $proc = Start-RpcServerProcess `
        -NodeExe $nodeExe `
        -RepoRoot $RepoRoot `
        -DbPath $dbPath `
        -EnablePublic $true `
        -EnableGov $false `
        -PublicBind $PublicBind `
        -GovBind $GovBind `
        -PublicMaxRequests 1 `
        -GovMaxRequests 0

    if (-not (Wait-TcpEndpoint -HostName $publicUri.Host -Port $publicUri.Port -TimeoutSeconds $StartupTimeoutSeconds)) {
        throw "default scenario: public rpc did not listen on $PublicBind"
    }

    $resp = Invoke-JsonPost -Uri $publicEndpoint -Body (Build-RpcBody -Id 1 -Method "governance_listAuditEvents" -Params @{ limit = 5 })
    $scenarioDefault.response = $resp
    $errObj = Get-JsonPropertyOrNull -Obj $resp.json -Name "error"
    $errCode = Get-JsonPropertyOrNull -Obj $errObj -Name "code"
    $scenarioDefault.public_reject_ok = ($resp.status -eq 200 -and [int]$errCode -eq -32601)
    $scenarioDefault.gov_closed_ok = -not (Test-TcpEndpoint -HostName $govUri.Host -Port $govUri.Port)

    if (-not $proc.WaitForExit($ExitTimeoutSeconds * 1000)) {
        throw "default scenario: rpc process did not exit within ${ExitTimeoutSeconds}s"
    }
    $scenarioDefault.process_exit_ok = ([int]$proc.ExitCode -eq 0)
    $scenarioDefault.pass = [bool]($scenarioDefault.public_reject_ok -and $scenarioDefault.gov_closed_ok -and $scenarioDefault.process_exit_ok)
    if (-not $scenarioDefault.pass) {
        $scenarioDefault.error_reason = "default_assert_failed(public_reject_ok=$($scenarioDefault.public_reject_ok), gov_closed_ok=$($scenarioDefault.gov_closed_ok), process_exit_ok=$($scenarioDefault.process_exit_ok))"
    }
} catch {
    $scenarioDefault.pass = $false
    $scenarioDefault.error_reason = $_.Exception.Message
} finally {
    Save-ProcessLogs -Process $proc -StdoutPath $scenarioDefault.stdout_log -StderrPath $scenarioDefault.stderr_log
}

$scenarioControlled = [ordered]@{
    pass = $false
    error_reason = ""
    public_reject_ok = $false
    gov_success_ok = $false
    process_exit_ok = $false
    stdout_log = Join-Path $OutputDir "rpc-exposure.controlled.stdout.log"
    stderr_log = Join-Path $OutputDir "rpc-exposure.controlled.stderr.log"
    public_response = $null
    gov_response = $null
}

$proc = $null
try {
    $proc = Start-RpcServerProcess `
        -NodeExe $nodeExe `
        -RepoRoot $RepoRoot `
        -DbPath $dbPath `
        -EnablePublic $true `
        -EnableGov $true `
        -PublicBind $PublicBind `
        -GovBind $GovBind `
        -PublicMaxRequests 1 `
        -GovMaxRequests 1

    if (-not (Wait-TcpEndpoint -HostName $publicUri.Host -Port $publicUri.Port -TimeoutSeconds $StartupTimeoutSeconds)) {
        throw "controlled scenario: public rpc did not listen on $PublicBind"
    }
    if (-not (Wait-TcpEndpoint -HostName $govUri.Host -Port $govUri.Port -TimeoutSeconds $StartupTimeoutSeconds)) {
        throw "controlled scenario: governance rpc did not listen on $GovBind"
    }

    $publicResp = Invoke-JsonPost -Uri $publicEndpoint -Body (Build-RpcBody -Id 2 -Method "governance_listAuditEvents" -Params @{ limit = 5 })
    $scenarioControlled.public_response = $publicResp
    $publicErrObj = Get-JsonPropertyOrNull -Obj $publicResp.json -Name "error"
    $publicErrCode = Get-JsonPropertyOrNull -Obj $publicErrObj -Name "code"
    $scenarioControlled.public_reject_ok = ($publicResp.status -eq 200 -and [int]$publicErrCode -eq -32601)

    $govResp = Invoke-JsonPost -Uri $govEndpoint -Body (Build-RpcBody -Id 3 -Method "governance_listAuditEvents" -Params @{ limit = 5 })
    $scenarioControlled.gov_response = $govResp
    $govErrObj = Get-JsonPropertyOrNull -Obj $govResp.json -Name "error"
    $govResult = Get-JsonPropertyOrNull -Obj $govResp.json -Name "result"
    $govMethod = Get-JsonPropertyOrNull -Obj $govResult -Name "method"
    $scenarioControlled.gov_success_ok = ($govResp.status -eq 200 -and $null -eq $govErrObj -and [string]$govMethod -eq "governance_listAuditEvents")

    if (-not $proc.WaitForExit($ExitTimeoutSeconds * 1000)) {
        throw "controlled scenario: rpc process did not exit within ${ExitTimeoutSeconds}s"
    }
    $scenarioControlled.process_exit_ok = ([int]$proc.ExitCode -eq 0)
    $scenarioControlled.pass = [bool]($scenarioControlled.public_reject_ok -and $scenarioControlled.gov_success_ok -and $scenarioControlled.process_exit_ok)
    if (-not $scenarioControlled.pass) {
        $scenarioControlled.error_reason = "controlled_assert_failed(public_reject_ok=$($scenarioControlled.public_reject_ok), gov_success_ok=$($scenarioControlled.gov_success_ok), process_exit_ok=$($scenarioControlled.process_exit_ok))"
    }
} catch {
    $scenarioControlled.pass = $false
    $scenarioControlled.error_reason = $_.Exception.Message
} finally {
    Save-ProcessLogs -Process $proc -StdoutPath $scenarioControlled.stdout_log -StderrPath $scenarioControlled.stderr_log
}

$pass = [bool]($scenarioDefault.pass -and $scenarioControlled.pass)
$errorReason = ""
if (-not $pass) {
    $parts = @()
    if (-not $scenarioDefault.pass) { $parts += "default_failed: $($scenarioDefault.error_reason)" }
    if (-not $scenarioControlled.pass) { $parts += "controlled_failed: $($scenarioControlled.error_reason)" }
    $errorReason = ($parts -join "; ")
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    error_reason = $errorReason
    public_bind = $PublicBind
    gov_bind = $GovBind
    public_endpoint = $publicEndpoint
    gov_endpoint = $govEndpoint
    default_safe_pass = [bool]$scenarioDefault.pass
    default_safe_public_reject_ok = [bool]$scenarioDefault.public_reject_ok
    default_safe_gov_closed_ok = [bool]$scenarioDefault.gov_closed_ok
    controlled_open_pass = [bool]$scenarioControlled.pass
    controlled_open_public_reject_ok = [bool]$scenarioControlled.public_reject_ok
    controlled_open_gov_success_ok = [bool]$scenarioControlled.gov_success_ok
    scenario_default = $scenarioDefault
    scenario_controlled = $scenarioControlled
}

$summaryJson = Join-Path $OutputDir "rpc-exposure-gate-summary.json"
$summaryMd = Join-Path $OutputDir "rpc-exposure-gate-summary.md"
$summary | ConvertTo-Json -Depth 24 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# RPC Exposure Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- error_reason: $($summary.error_reason)"
    "- public_bind: $($summary.public_bind)"
    "- gov_bind: $($summary.gov_bind)"
    "- default_safe_pass: $($summary.default_safe_pass)"
    "- default_safe_public_reject_ok: $($summary.default_safe_public_reject_ok)"
    "- default_safe_gov_closed_ok: $($summary.default_safe_gov_closed_ok)"
    "- controlled_open_pass: $($summary.controlled_open_pass)"
    "- controlled_open_public_reject_ok: $($summary.controlled_open_public_reject_ok)"
    "- controlled_open_gov_success_ok: $($summary.controlled_open_gov_success_ok)"
    "- summary_json: $summaryJson"
    ""
    "## Logs"
    ""
    "- default_stdout: $($summary.scenario_default.stdout_log)"
    "- default_stderr: $($summary.scenario_default.stderr_log)"
    "- controlled_stdout: $($summary.scenario_controlled.stdout_log)"
    "- controlled_stderr: $($summary.scenario_controlled.stderr_log)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "rpc exposure gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  default_safe_pass: $($summary.default_safe_pass)"
Write-Host "  controlled_open_pass: $($summary.controlled_open_pass)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"

if (-not $summary.pass) {
    throw "rpc exposure gate FAILED: $($summary.error_reason)"
}

Write-Host "rpc exposure gate PASS"
