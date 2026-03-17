param(
    [string]$RepoRoot = "",
    [string]$GatewayUrl = "http://127.0.0.1:9899",
    [UInt64]$ChainId = 1,
    [UInt64]$ObservationMinutes = 1,
    [UInt64]$WatchSeconds = 45,
    [string]$StartupCommand = "",
    [string]$NetworkNote = "",
    [switch]$SkipObservation,
    [string]$OutputRoot = "artifacts/migration/cross-machine-diag"
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
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)]$Params
    )
    $body = @{
        jsonrpc = "2.0"
        id = 1
        method = $Method
        params = $Params
    } | ConvertTo-Json -Depth 64 -Compress
    return Invoke-RestMethod -Uri $GatewayUrl -Method Post -ContentType "application/json" -Body $body -TimeoutSec 15
}

function Write-Section {
    param(
        [string]$Path,
        [string]$Title,
        [string]$Body
    )
    Add-Content -Path $Path -Value ("`n### {0}`n{1}`n" -f $Title, $Body)
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
Set-Location $RepoRoot

$psExe = if (Get-Command pwsh -ErrorAction SilentlyContinue) {
    "pwsh"
} elseif (Get-Command powershell -ErrorAction SilentlyContinue) {
    "powershell"
} else {
    throw "pwsh/powershell not found"
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$outputRootAbs = Resolve-FullPath -Root $RepoRoot -Value $OutputRoot
$bundleDir = Join-Path $outputRootAbs ("diag-{0}" -f $stamp)
New-Item -ItemType Directory -Force -Path $bundleDir | Out-Null

$versionsPath = Join-Path $bundleDir "versions-and-runtime.txt"
Set-Content -Path $versionsPath -Value ("generated_at={0}" -f (Get-Date).ToString("o"))

Write-Section -Path $versionsPath -Title "git branch" -Body ((& git rev-parse --abbrev-ref HEAD 2>&1 | Out-String).Trim())
Write-Section -Path $versionsPath -Title "git commit" -Body ((& git rev-parse HEAD 2>&1 | Out-String).Trim())
Write-Section -Path $versionsPath -Title "git status porcelain" -Body ((& git status --porcelain 2>&1 | Out-String).Trim())
Write-Section -Path $versionsPath -Title "pwsh -v" -Body ((& pwsh -v 2>&1 | Out-String).Trim())
Write-Section -Path $versionsPath -Title "cargo --version" -Body ((& cargo --version 2>&1 | Out-String).Trim())
Write-Section -Path $versionsPath -Title "rustc --version" -Body ((& rustc --version 2>&1 | Out-String).Trim())

$gatewayCandidates = @(
    (Join-Path $RepoRoot "target/debug/novovm-evm-gateway.exe"),
    (Join-Path $RepoRoot "target/debug/novovm-evm-gateway")
)
$gatewayBin = $gatewayCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($gatewayBin) {
    $hashText = (Get-FileHash $gatewayBin | Format-List | Out-String).Trim()
    Write-Section -Path $versionsPath -Title "gateway binary path" -Body $gatewayBin
    Write-Section -Path $versionsPath -Title "gateway binary hash" -Body $hashText
} else {
    Write-Section -Path $versionsPath -Title "gateway binary" -Body "not found under target/debug"
}

if ([string]::IsNullOrWhiteSpace($StartupCommand)) {
    $StartupCommand = "# fill your exact launch command here"
}
Set-Content -Path (Join-Path $bundleDir "startup-command.txt") -Value $StartupCommand

$sensitivePattern = "PRIVATE_KEY|SIGNER|SECRET|TOKEN|PASSWORD|MNEMONIC|API_KEY|KEY_HEX"
$envRows = Get-ChildItem Env: |
    Where-Object { $_.Name -match "^(NOVOVM_|STACK_|NODE1_|NODE2_)" } |
    Where-Object { $_.Name -notmatch $sensitivePattern } |
    Sort-Object Name |
    Select-Object Name, Value
$envRows | Format-Table -AutoSize | Out-String | Set-Content -Path (Join-Path $bundleDir "env-sanitized.txt")
$envRows | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 -Path (Join-Path $bundleDir "env-export.json")

$rpcRecords = New-Object System.Collections.ArrayList
$rpcCalls = @(
    @{ method = "eth_chainId"; params = @() },
    @{ method = "net_peerCount"; params = @{ chain_id = [UInt64]$ChainId } },
    @{ method = "evm_getPublicBroadcastStatus"; params = @{ chain_id = [UInt64]$ChainId } },
    @{ method = "evm_getPublicBroadcastCapability"; params = @{ chain_id = [UInt64]$ChainId } },
    @{ method = "evm_getPublicBroadcastPluginPeers"; params = @{ chain_id = [UInt64]$ChainId } },
    @{ method = "evm_snapshotExecutableIngress"; params = @{ chain_id = [UInt64]$ChainId; max_items = [UInt64]64; include_raw = $false; include_parsed = $true } },
    @{ method = "evm_snapshotPendingIngress"; params = @{ chain_id = [UInt64]$ChainId; max_items = [UInt64]64; include_raw = $false; include_parsed = $true } },
    @{ method = "evm_txpoolStatus"; params = @{ chain_id = [UInt64]$ChainId } },
    @{ method = "txpool_status"; params = @{ chain_id = [UInt64]$ChainId } }
)
foreach ($call in $rpcCalls) {
    $item = [ordered]@{
        at = (Get-Date).ToString("o")
        method = [string]$call.method
        params = $call.params
        ok = $false
        result = $null
        error = $null
    }
    try {
        $resp = Invoke-JsonRpc -Method $call.method -Params $call.params
        if ($resp -and $resp.PSObject.Properties.Name -contains "error" -and $null -ne $resp.error) {
            $item.error = $resp.error
        } else {
            $item.ok = $true
            $item.result = $resp.result
        }
    } catch {
        $item.error = $_.Exception.Message
    }
    [void]$rpcRecords.Add([pscustomobject]$item)
}
$rpcRecords | ConvertTo-Json -Depth 64 | Set-Content -Encoding UTF8 -Path (Join-Path $bundleDir "rpc-snapshot.json")

$obsStdout = Join-Path $bundleDir "observation.stdout.log"
$obsStderr = Join-Path $bundleDir "observation.stderr.log"
$watchStdout = Join-Path $bundleDir "watch.stdout.log"
$watchStderr = Join-Path $bundleDir "watch.stderr.log"
$obsSummary = Join-Path $bundleDir "evm-uniswap-observation-window-summary.json"

if (-not $SkipObservation) {
    $obsScript = Join-Path $RepoRoot "scripts/migration/run_evm_uniswap_observation_window.ps1"
    if (Test-Path $obsScript) {
        $obsArgs = @(
            "-ExecutionPolicy", "Bypass",
            "-File", $obsScript,
            "-SkipBuild",
            "-AttachExistingGateway",
            "-ChainId", ([string][UInt64]$ChainId),
            "-DurationMinutes", ([string][UInt64]$ObservationMinutes),
            "-IntervalSeconds", "5",
            "-WarmupSeconds", "6",
            "-SummaryOut", $obsSummary
        )
        $obsProc = Start-Process -FilePath $psExe -ArgumentList $obsArgs -WorkingDirectory $RepoRoot -RedirectStandardOutput $obsStdout -RedirectStandardError $obsStderr -PassThru
        Wait-Process -Id $obsProc.Id
    }

    $watchScript = Join-Path $RepoRoot "scripts/migration/watch_evm_uniswap_window.ps1"
    if (Test-Path $watchScript) {
        $watchArgs = @(
            "-ExecutionPolicy", "Bypass",
            "-File", $watchScript,
            "-GatewayUrl", $GatewayUrl,
            "-ChainId", ([string][UInt64]$ChainId),
            "-IntervalMs", "1000",
            "-SampleMax", "5"
        )
        $watchProc = Start-Process -FilePath $psExe -ArgumentList $watchArgs -WorkingDirectory $RepoRoot -RedirectStandardOutput $watchStdout -RedirectStandardError $watchStderr -PassThru
        Start-Sleep -Seconds ([int][Math]::Max(10, [int]$WatchSeconds))
        if (-not $watchProc.HasExited) {
            Stop-Process -Id $watchProc.Id -Force
        }
    }
}

$summaryRoot = Join-Path $RepoRoot "artifacts/migration/evm-uniswap-observation-window-summary.json"
if ((-not (Test-Path $obsSummary)) -and (Test-Path $summaryRoot)) {
    Copy-Item -Path $summaryRoot -Destination $obsSummary -Force
}

$gatewayLogCandidates = @(
    (Join-Path $RepoRoot "artifacts/migration/gateway.stdout.log"),
    (Join-Path $RepoRoot "artifacts/migration/gateway.stderr.log"),
    (Join-Path $RepoRoot "gateway.stdout.log"),
    (Join-Path $RepoRoot "gateway.stderr.log")
)
foreach ($log in $gatewayLogCandidates) {
    if (Test-Path $log) {
        Copy-Item -Path $log -Destination (Join-Path $bundleDir ([System.IO.Path]::GetFileName($log))) -Force
    }
}

$keyLines = New-Object System.Collections.ArrayList
if (Test-Path $obsStdout) {
    $lines = Get-Content -Path $obsStdout
    foreach ($line in $lines) {
        if ($line -match "pending=(?<pending>\d+)" -or $line -match "uniV2=(?<v2>\d+)" -or $line -match "uniV3=(?<v3>\d+)") {
            [void]$keyLines.Add($line)
        }
    }
}
if ($keyLines.Count -gt 0) {
    $firstSignals = @($keyLines | Select-Object -First 5)
    Set-Content -Path (Join-Path $bundleDir "key-signal-lines.txt") -Value $firstSignals
}

if (-not [string]::IsNullOrWhiteSpace($NetworkNote)) {
    Set-Content -Path (Join-Path $bundleDir "network-note.txt") -Value $NetworkNote
}

$zipPath = "{0}.zip" -f $bundleDir
if (Test-Path $zipPath) {
    Remove-Item -Force $zipPath
}
Compress-Archive -Path (Join-Path $bundleDir "*") -DestinationPath $zipPath -Force

Write-Host ("diag_bundle_dir={0}" -f $bundleDir)
Write-Host ("diag_bundle_zip={0}" -f $zipPath)
