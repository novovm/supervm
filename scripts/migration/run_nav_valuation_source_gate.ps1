param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 90)]
    [int]$TimeoutSeconds = 30
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\nav-valuation-source-gate"
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

    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        stdout = $stdout
        stderr = $stderr
        output = ($stdout + $stderr)
    }
}

function Invoke-NodeProbe {
    param(
        [string]$NodeExe,
        [string]$WorkDir,
        [hashtable]$EnvVars,
        [int]$TimeoutSeconds
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $NodeExe
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    foreach ($entry in $EnvVars.GetEnumerator()) {
        $psi.Environment[$entry.Key] = [string]$entry.Value
    }

    $proc = [System.Diagnostics.Process]::Start($psi)
    if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
        try { $proc.Kill() } catch {}
        throw "nav valuation source probe timed out after ${TimeoutSeconds}s"
    }
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        stdout = $stdout
        stderr = $stderr
        output = ($stdout + $stderr)
    }
}

function Parse-NavSourceOutLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^governance_market_nav_source_out:" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $payload = ($line -replace "^governance_market_nav_source_out:\s*", "")
    $pairs = @{}
    foreach ($token in ($payload -split "\s+")) {
        if ($token -match "^(?<k>[A-Za-z0-9_]+)=(?<v>\S+)$") {
            $pairs[$matches["k"]] = $matches["v"]
        }
    }
    if (-not $pairs.ContainsKey("proposal_id") -or -not $pairs.ContainsKey("nav_source_applied")) {
        return [ordered]@{ parse_ok = $false; raw = $line }
    }
    $fetchedSources = 0
    if ($pairs.ContainsKey("fetched_sources")) {
        $fetchedSources = [int64]$pairs["fetched_sources"]
    }
    $configuredSources = 0
    if ($pairs.ContainsKey("configured_sources")) {
        $configuredSources = [int64]$pairs["configured_sources"]
    }
    $minSources = 0
    if ($pairs.ContainsKey("min_sources")) {
        $minSources = [int64]$pairs["min_sources"]
    }
    $signatureRequired = $false
    if ($pairs.ContainsKey("signature_required")) {
        $signatureRequired = [bool]::Parse($pairs["signature_required"])
    }
    $signatureVerified = $false
    if ($pairs.ContainsKey("signature_verified")) {
        $signatureVerified = [bool]::Parse($pairs["signature_verified"])
    }
    return [ordered]@{
        parse_ok = $true
        proposal_id = [int64]$pairs["proposal_id"]
        nav_source_applied = [bool]::Parse($pairs["nav_source_applied"])
        source = $pairs["source"]
        price_bp = [int64]$pairs["price_bp"]
        fallback_used = [bool]::Parse($pairs["fallback_used"])
        fetched = [bool]::Parse($pairs["fetched"])
        fetched_sources = $fetchedSources
        configured_sources = $configuredSources
        min_sources = $minSources
        signature_required = $signatureRequired
        signature_verified = $signatureVerified
        reason_code = $pairs["reason_code"]
        strict = [bool]::Parse($pairs["strict"])
        mode = $pairs["mode"]
        raw = $line
    }
}

function Start-OneShotNavFeedServer {
    param(
        [Parameter(Mandatory=$true)]
        [int]$Port,
        [Parameter(Mandatory=$true)]
        [int]$PriceBp,
        [string]$SignatureSha256 = ""
    )

    return Start-Job -ScriptBlock {
        param($Port, $PriceBp, $SignatureSha256)
        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, [int]$Port)
        $listener.Start()
        try {
            $client = $listener.AcceptTcpClient()
            try {
                $stream = $client.GetStream()
                $reader = [System.IO.StreamReader]::new($stream, [System.Text.Encoding]::ASCII, $false, 1024, $true)
                try {
                    while ($true) {
                        $line = $reader.ReadLine()
                        if ($null -eq $line -or $line -eq "") {
                            break
                        }
                    }
                } finally {
                    $reader.Dispose()
                }

                if ($SignatureSha256) {
                    $body = "{`"price_bp`":$PriceBp,`"signature_sha256`":`"$SignatureSha256`"}"
                } else {
                    $body = "{`"price_bp`":$PriceBp}"
                }
                $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($body)
                $header = "HTTP/1.1 200 OK`r`nContent-Type: application/json`r`nContent-Length: $($bodyBytes.Length)`r`nConnection: close`r`n`r`n"
                $headerBytes = [System.Text.Encoding]::ASCII.GetBytes($header)
                $stream.Write($headerBytes, 0, $headerBytes.Length)
                $stream.Write($bodyBytes, 0, $bodyBytes.Length)
                $stream.Flush()
                $stream.Dispose()
            } finally {
                $client.Dispose()
            }
        } finally {
            $listener.Stop()
        }
    } -ArgumentList $Port, $PriceBp, $SignatureSha256
}

function Get-Sha256Hex {
    param([Parameter(Mandatory=$true)][string]$Value)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Value)
        $hash = $sha.ComputeHash($bytes)
        return ([System.BitConverter]::ToString($hash) -replace "-", "").ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$tests = @(
    [ordered]@{
        key = "nav_valuation_external_with_price_ok"
        crate = "novovm-consensus"
        workdir = Join-Path $RepoRoot "crates\novovm-consensus"
        args = @("test", "--quiet", "test_nav_valuation_source_external_with_price")
    },
    [ordered]@{
        key = "nav_valuation_missing_quote_fallback_ok"
        crate = "novovm-consensus"
        workdir = Join-Path $RepoRoot "crates\novovm-consensus"
        args = @("test", "--quiet", "test_nav_valuation_source_external_missing_quote_fallback")
    },
    [ordered]@{
        key = "nav_valuation_invalid_price_reject_ok"
        crate = "novovm-consensus"
        workdir = Join-Path $RepoRoot "crates\novovm-consensus"
        args = @("test", "--quiet", "test_nav_valuation_source_reject_invalid_price")
    },
    [ordered]@{
        key = "market_engine_nav_regression_ok"
        crate = "novovm-consensus"
        workdir = Join-Path $RepoRoot "crates\novovm-consensus"
        args = @("test", "--quiet", "test_market_engine_apply_policy")
    }
)

$results = @()
foreach ($t in $tests) {
    $workdir = (Resolve-Path $t.workdir).Path
    $res = Invoke-Cargo -WorkDir $workdir -CargoArgs $t.args
    $stdoutPath = Join-Path $OutputDir "$($t.key).stdout.log"
    $stderrPath = Join-Path $OutputDir "$($t.key).stderr.log"
    $res.stdout | Set-Content -Path $stdoutPath -Encoding UTF8
    $res.stderr | Set-Content -Path $stderrPath -Encoding UTF8

    $results += [ordered]@{
        key = $t.key
        crate = $t.crate
        workdir = $workdir
        command = "cargo $($t.args -join ' ')"
        pass = [bool]($res.exit_code -eq 0)
        exit_code = [int]$res.exit_code
        stdout_log = $stdoutPath
        stderr_log = $stderrPath
    }
}

$nodeCrateDir = Join-Path $RepoRoot "crates\novovm-node"
$nodeBuild = Invoke-Cargo -WorkDir $nodeCrateDir -CargoArgs @("build", "--quiet", "--bin", "novovm-node")
$nodeBuildStdout = Join-Path $OutputDir "nav_source_node_build.stdout.log"
$nodeBuildStderr = Join-Path $OutputDir "nav_source_node_build.stderr.log"
$nodeBuild.stdout | Set-Content -Path $nodeBuildStdout -Encoding UTF8
$nodeBuild.stderr | Set-Content -Path $nodeBuildStderr -Encoding UTF8
$results += [ordered]@{
    key = "nav_source_node_build_ok"
    crate = "novovm-node"
    workdir = $nodeCrateDir
    command = "cargo build --quiet --bin novovm-node"
    pass = [bool]($nodeBuild.exit_code -eq 0)
    exit_code = [int]$nodeBuild.exit_code
    stdout_log = $nodeBuildStdout
    stderr_log = $nodeBuildStderr
}

if ($nodeBuild.exit_code -eq 0) {
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
        $results += [ordered]@{
            key = "nav_source_node_binary_found"
            crate = "novovm-node"
            workdir = $nodeCrateDir
            command = "resolve novovm-node.exe"
            pass = $false
            exit_code = 1
            stdout_log = ""
            stderr_log = "missing novovm-node binary after build"
        }
    } else {
        $signatureKey = "nav_feed_signing_key_v1"
        $feedPriceBpA = 12300
        $feedPriceBpB = 12700
        $expectedPriceBp = [int](($feedPriceBpA + $feedPriceBpB) / 2)

        $portProbeA = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
        $portProbeA.Start()
        $feedPortA = $portProbeA.LocalEndpoint.Port
        $portProbeA.Stop()

        $portProbeB = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
        $portProbeB.Start()
        $feedPortB = $portProbeB.LocalEndpoint.Port
        $portProbeB.Stop()

        $sigA = Get-Sha256Hex -Value "nav_feed_v1|price_bp=$feedPriceBpA|$signatureKey"
        $sigB = Get-Sha256Hex -Value "nav_feed_v1|price_bp=$feedPriceBpB|$signatureKey"
        $feedJobA = Start-OneShotNavFeedServer -Port $feedPortA -PriceBp $feedPriceBpA -SignatureSha256 $sigA
        $feedJobB = Start-OneShotNavFeedServer -Port $feedPortB -PriceBp $feedPriceBpB -SignatureSha256 $sigB
        Start-Sleep -Milliseconds 120

        $positiveProbe = $null
        try {
            $positiveProbe = Invoke-NodeProbe `
                -NodeExe $nodeExe `
                -WorkDir $RepoRoot `
                -TimeoutSeconds $TimeoutSeconds `
                -EnvVars @{
                    NOVOVM_NODE_MODE = "governance_market_policy_probe"
                    NOVOVM_GOV_MARKET_NAV_VALUATION_MODE = "external_feed"
                    NOVOVM_GOV_MARKET_NAV_VALUATION_SOURCE_NAME = "external_feed_http_v1"
                    NOVOVM_GOV_MARKET_NAV_FEED_URLS = "http://127.0.0.1:$feedPortA/quote,http://127.0.0.1:$feedPortB/quote"
                    NOVOVM_GOV_MARKET_NAV_FEED_MIN_SOURCES = "2"
                    NOVOVM_GOV_MARKET_NAV_FEED_TIMEOUT_MS = "1200"
                    NOVOVM_GOV_MARKET_NAV_FEED_SIGNATURE_REQUIRED = "1"
                    NOVOVM_GOV_MARKET_NAV_FEED_SIGNATURE_KEY = $signatureKey
                }
        } finally {
            foreach ($job in @($feedJobA, $feedJobB)) {
                if ($job) {
                    try {
                        Wait-Job -Job $job -Timeout 2 | Out-Null
                    } catch {}
                    Receive-Job -Job $job -ErrorAction SilentlyContinue | Out-Null
                    Remove-Job -Job $job -Force -ErrorAction SilentlyContinue
                }
            }
        }

        $positiveStdout = Join-Path $OutputDir "nav_source_external_feed_probe.stdout.log"
        $positiveStderr = Join-Path $OutputDir "nav_source_external_feed_probe.stderr.log"
        $positiveProbe.stdout | Set-Content -Path $positiveStdout -Encoding UTF8
        $positiveProbe.stderr | Set-Content -Path $positiveStderr -Encoding UTF8
        $positiveLine = Parse-NavSourceOutLine -Text $positiveProbe.output
        $positivePass = [bool](
            $positiveProbe.exit_code -eq 0 -and
            $positiveLine -and
            $positiveLine.parse_ok -and
            $positiveLine.nav_source_applied -and
            $positiveLine.source -eq "external_feed_http_v1" -and
            $positiveLine.price_bp -eq $expectedPriceBp -and
            -not $positiveLine.fallback_used -and
            $positiveLine.fetched -and
            $positiveLine.fetched_sources -eq 2 -and
            $positiveLine.configured_sources -eq 2 -and
            $positiveLine.min_sources -eq 2 -and
            $positiveLine.signature_required -and
            $positiveLine.signature_verified -and
            $positiveLine.reason_code -eq "feed_quote_ok" -and
            $positiveLine.mode -eq "external_feed"
        )
        $results += [ordered]@{
            key = "nav_source_external_feed_probe_ok"
            crate = "novovm-node"
            workdir = $RepoRoot
            command = "NOVOVM_NODE_MODE=governance_market_policy_probe + external feed quote"
            pass = $positivePass
            exit_code = [int]$positiveProbe.exit_code
            stdout_log = $positiveStdout
            stderr_log = $positiveStderr
        }

        $portProbeFallback = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
        $portProbeFallback.Start()
        $feedPortFallback = $portProbeFallback.LocalEndpoint.Port
        $portProbeFallback.Stop()
        $fallbackPriceBp = 12600
        $fallbackSig = Get-Sha256Hex -Value "nav_feed_v1|price_bp=$fallbackPriceBp|$signatureKey"
        $fallbackFeedJob = Start-OneShotNavFeedServer -Port $feedPortFallback -PriceBp $fallbackPriceBp -SignatureSha256 $fallbackSig
        Start-Sleep -Milliseconds 80

        $fallbackProbe = $null
        try {
            $fallbackProbe = Invoke-NodeProbe `
                -NodeExe $nodeExe `
                -WorkDir $RepoRoot `
                -TimeoutSeconds $TimeoutSeconds `
                -EnvVars @{
                    NOVOVM_NODE_MODE = "governance_market_policy_probe"
                    NOVOVM_GOV_MARKET_NAV_VALUATION_MODE = "external_feed"
                    NOVOVM_GOV_MARKET_NAV_VALUATION_SOURCE_NAME = "external_feed_http_v1"
                    NOVOVM_GOV_MARKET_NAV_FEED_URLS = "http://127.0.0.1:$feedPortFallback/quote,http://127.0.0.1:65530/quote"
                    NOVOVM_GOV_MARKET_NAV_FEED_MIN_SOURCES = "2"
                    NOVOVM_GOV_MARKET_NAV_FEED_TIMEOUT_MS = "300"
                    NOVOVM_GOV_MARKET_NAV_FEED_SIGNATURE_REQUIRED = "1"
                    NOVOVM_GOV_MARKET_NAV_FEED_SIGNATURE_KEY = $signatureKey
                    NOVOVM_GOV_MARKET_NAV_FEED_STRICT = "0"
                }
        } finally {
            if ($fallbackFeedJob) {
                try {
                    Wait-Job -Job $fallbackFeedJob -Timeout 2 | Out-Null
                } catch {}
                Receive-Job -Job $fallbackFeedJob -ErrorAction SilentlyContinue | Out-Null
                Remove-Job -Job $fallbackFeedJob -Force -ErrorAction SilentlyContinue
            }
        }

        $fallbackStdout = Join-Path $OutputDir "nav_source_external_feed_fallback.stdout.log"
        $fallbackStderr = Join-Path $OutputDir "nav_source_external_feed_fallback.stderr.log"
        $fallbackProbe.stdout | Set-Content -Path $fallbackStdout -Encoding UTF8
        $fallbackProbe.stderr | Set-Content -Path $fallbackStderr -Encoding UTF8
        $fallbackLine = Parse-NavSourceOutLine -Text $fallbackProbe.output
        $fallbackPass = [bool](
            $fallbackProbe.exit_code -eq 0 -and
            $fallbackLine -and
            $fallbackLine.parse_ok -and
            $fallbackLine.nav_source_applied -and
            $fallbackLine.source -eq "external_feed_http_v1" -and
            $fallbackLine.fallback_used -and
            -not $fallbackLine.fetched -and
            $fallbackLine.fetched_sources -eq 1 -and
            $fallbackLine.configured_sources -eq 2 -and
            $fallbackLine.min_sources -eq 2 -and
            $fallbackLine.signature_required -and
            -not $fallbackLine.signature_verified -and
            $fallbackLine.reason_code -eq "feed_quote_insufficient_sources_fallback" -and
            $fallbackLine.mode -eq "external_feed"
        )
        $results += [ordered]@{
            key = "nav_source_external_feed_fallback_ok"
            crate = "novovm-node"
            workdir = $RepoRoot
            command = "NOVOVM_NODE_MODE=governance_market_policy_probe + multisource insufficient fallback"
            pass = $fallbackPass
            exit_code = [int]$fallbackProbe.exit_code
            stdout_log = $fallbackStdout
            stderr_log = $fallbackStderr
        }

        $portProbeBadSig = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
        $portProbeBadSig.Start()
        $feedPortBadSig = $portProbeBadSig.LocalEndpoint.Port
        $portProbeBadSig.Stop()
        $badPriceBp = 12999
        $badSig = Get-Sha256Hex -Value "nav_feed_v1|price_bp=$badPriceBp|wrong_signing_key"
        $badFeedJob = Start-OneShotNavFeedServer -Port $feedPortBadSig -PriceBp $badPriceBp -SignatureSha256 $badSig
        Start-Sleep -Milliseconds 80

        $strictFailProbe = $null
        try {
            $strictFailProbe = Invoke-NodeProbe `
                -NodeExe $nodeExe `
                -WorkDir $RepoRoot `
                -TimeoutSeconds $TimeoutSeconds `
                -EnvVars @{
                    NOVOVM_NODE_MODE = "governance_market_policy_probe"
                    NOVOVM_GOV_MARKET_NAV_VALUATION_MODE = "external_feed"
                    NOVOVM_GOV_MARKET_NAV_VALUATION_SOURCE_NAME = "external_feed_http_v1"
                    NOVOVM_GOV_MARKET_NAV_FEED_URLS = "http://127.0.0.1:$feedPortBadSig/quote"
                    NOVOVM_GOV_MARKET_NAV_FEED_MIN_SOURCES = "1"
                    NOVOVM_GOV_MARKET_NAV_FEED_TIMEOUT_MS = "300"
                    NOVOVM_GOV_MARKET_NAV_FEED_SIGNATURE_REQUIRED = "1"
                    NOVOVM_GOV_MARKET_NAV_FEED_SIGNATURE_KEY = $signatureKey
                    NOVOVM_GOV_MARKET_NAV_FEED_STRICT = "1"
                }
        } finally {
            if ($badFeedJob) {
                try {
                    Wait-Job -Job $badFeedJob -Timeout 2 | Out-Null
                } catch {}
                Receive-Job -Job $badFeedJob -ErrorAction SilentlyContinue | Out-Null
                Remove-Job -Job $badFeedJob -Force -ErrorAction SilentlyContinue
            }
        }

        $strictFailStdout = Join-Path $OutputDir "nav_source_external_feed_strict_reject.stdout.log"
        $strictFailStderr = Join-Path $OutputDir "nav_source_external_feed_strict_reject.stderr.log"
        $strictFailProbe.stdout | Set-Content -Path $strictFailStdout -Encoding UTF8
        $strictFailProbe.stderr | Set-Content -Path $strictFailStderr -Encoding UTF8
        $strictFailPass = [bool](
            $strictFailProbe.exit_code -ne 0 -and
            ($strictFailProbe.output -match "nav_feed_fetch_failed") -and
            ($strictFailProbe.output -match "signature mismatch")
        )
        $results += [ordered]@{
            key = "nav_source_external_feed_signature_strict_reject_ok"
            crate = "novovm-node"
            workdir = $RepoRoot
            command = "NOVOVM_NODE_MODE=governance_market_policy_probe + strict bad signature reject"
            pass = $strictFailPass
            exit_code = [int]$strictFailProbe.exit_code
            stdout_log = $strictFailStdout
            stderr_log = $strictFailStderr
        }
    }
}

$allPass = @($results | Where-Object { -not $_.pass }).Count -eq 0
$errorReason = ""
if (-not $allPass) {
    $failed = @($results | Where-Object { -not $_.pass } | Select-Object -ExpandProperty key)
    $errorReason = "failed_tests: $($failed -join ',')"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $allPass
    error_reason = $errorReason
    tests = $results
}

$summaryJson = Join-Path $OutputDir "nav-valuation-source-gate-summary.json"
$summaryMd = Join-Path $OutputDir "nav-valuation-source-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Nav Valuation Source Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- error_reason: $($summary.error_reason)"
    "- summary_json: $summaryJson"
    ""
    "## Tests"
)
foreach ($r in $results) {
    $md += "- $($r.key): pass=$($r.pass) exit_code=$($r.exit_code) crate=$($r.crate)"
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "nav valuation source gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  error_reason: $($summary.error_reason)"
Write-Host "  summary_json: $summaryJson"

if (-not $allPass) {
    throw "nav valuation source gate FAILED: $errorReason"
}

Write-Host "nav valuation source gate PASS"
