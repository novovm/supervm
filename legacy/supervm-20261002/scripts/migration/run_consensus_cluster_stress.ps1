param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 50000000)]
    [int]$Txs = 100000,
    [ValidateRange(1, 50000000)]
    [int]$Accounts = 100000,
    [ValidateRange(1, 1000000)]
    [int]$BatchSize = 1000,
    [ValidateRange(4, 100)]
    [int]$Validators = 4,
    [ValidateRange(1, 1000000)]
    [int]$MaxBatches = 1000,
    [ValidateSet("udp", "tcp")]
    [string]$Transport = "udp",
    [ValidateSet("core", "persist", "wasm")]
    [string]$AoemVariant = "persist",
    [string]$AoemPluginDir = "",
    [ValidateSet("auto", "ops_wire_v1", "ops_v2")]
    [string]$D1IngressMode = "auto",
    [ValidateSet("release", "debug")]
    [string]$BuildProfile = "release",
    [ValidateRange(1024, 65500)]
    [int]$BasePort = 23000,
    [string[]]$NodeHosts = @("127.0.0.1"),
    [switch]$EnableSshRemote,
    [string]$SshUser = "",
    [string]$RemoteRepoRoot = "",
    [switch]$SkipBuild,
    [ValidateRange(30, 7200)]
    [int]$TimeoutSec = 1200
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if (-not (Get-Variable -Name IsWindows -ErrorAction SilentlyContinue)) {
    $IsWindows = ($env:OS -eq "Windows_NT")
}
if (-not (Get-Variable -Name IsMacOS -ErrorAction SilentlyContinue)) {
    $IsMacOS = $false
    if ($PSVersionTable.Platform -eq "Unix") {
        try {
            $IsMacOS = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)
        } catch {
            $IsMacOS = $false
        }
    }
}

function Resolve-AbsolutePath([string]$path, [string]$base) {
    if ([System.IO.Path]::IsPathRooted($path)) {
        return [System.IO.Path]::GetFullPath($path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $base $path))
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
    if ($proc.ExitCode -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed in $WorkDir`n$stdout`n$stderr"
    }
    return $stdout
}

function Test-LocalHostName([string]$hostName) {
    $h = $hostName.Trim().ToLowerInvariant()
    if ($h -eq "127.0.0.1" -or $h -eq "localhost" -or $h -eq "::1") { return $true }
    if ($h -eq $env:COMPUTERNAME.ToLowerInvariant()) { return $true }
    return $false
}

function Start-ManagedProcess {
    param(
        [string]$FilePath,
        [string]$Arguments,
        [string]$WorkDir,
        [hashtable]$EnvMap,
        [string]$StdoutPath,
        [string]$StderrPath
    )
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $FilePath
    $psi.Arguments = $Arguments
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    foreach ($k in $EnvMap.Keys) {
        $psi.Environment[$k] = [string]$EnvMap[$k]
    }
    $proc = [System.Diagnostics.Process]::Start($psi)
    $outWriter = [System.IO.StreamWriter]::new($StdoutPath, $false, [System.Text.Encoding]::UTF8)
    $errWriter = [System.IO.StreamWriter]::new($StderrPath, $false, [System.Text.Encoding]::UTF8)
    $outTask = $proc.StandardOutput.ReadToEndAsync()
    $errTask = $proc.StandardError.ReadToEndAsync()
    return [pscustomobject]@{
        Process = $proc
        OutTask = $outTask
        ErrTask = $errTask
        OutWriter = $outWriter
        ErrWriter = $errWriter
    }
}

function Extract-JsonObjectText([string]$text) {
    $start = $text.IndexOf('{')
    $end = $text.LastIndexOf('}')
    if ($start -lt 0 -or $end -lt $start) { return $null }
    return $text.Substring($start, $end - $start + 1)
}

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $RemoteRepoRoot) {
    $RemoteRepoRoot = $RepoRoot
}

$dateTag = Get-Date -Format "yyyy-MM-dd-HHmmss"
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\consensus-cluster-stress-$dateTag"
}
$OutputDir = Resolve-AbsolutePath -path $OutputDir -base $RepoRoot
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

if (-not $AoemPluginDir) {
    $platform = if ($IsWindows) { "windows" } elseif ($IsMacOS) { "macos" } else { "linux" }
    $candidateNew = Join-Path $RepoRoot "aoem\$platform\core\plugins"
    $candidateOld = Join-Path $RepoRoot "aoem\plugins"
    $AoemPluginDir = if (Test-Path $candidateNew) { $candidateNew } else { $candidateOld }
}
$AoemPluginDir = Resolve-AbsolutePath -path $AoemPluginDir -base $RepoRoot
if (-not (Test-Path $AoemPluginDir)) {
    throw "aoem plugin dir not found: $AoemPluginDir"
}

$nodeHostsExpanded = @()
if ($NodeHosts.Count -eq 1) {
    for ($i = 0; $i -lt $Validators; $i++) { $nodeHostsExpanded += $NodeHosts[0] }
} elseif ($NodeHosts.Count -eq $Validators) {
    $nodeHostsExpanded = $NodeHosts
} else {
    throw "NodeHosts count must be 1 or equal Validators($Validators), got $($NodeHosts.Count)"
}

$hasRemote = $false
foreach ($h in $nodeHostsExpanded) {
    if (-not (Test-LocalHostName $h)) {
        $hasRemote = $true
        break
    }
}
if ($hasRemote -and -not $EnableSshRemote.IsPresent) {
    throw "remote hosts detected in NodeHosts; rerun with -EnableSshRemote and ensure ssh/scp available"
}
if ($hasRemote -and -not (Test-LocalHostName $nodeHostsExpanded[0])) {
    throw "current cluster runner requires leader(node_id=0) to be local; set NodeHosts[0]=localhost"
}

$benchDir = Join-Path $RepoRoot "crates\novovm-bench"
if (-not $SkipBuild.IsPresent) {
    Write-Host ("cluster stress: building binaries ({0}) ..." -f $BuildProfile)
    $buildArgs = @("build")
    if ($BuildProfile -eq "release") { $buildArgs += "--release" }
    $buildArgs += @("--bin", "novovm-txgen", "--bin", "novovm-consensus-cluster-node")
    Invoke-Cargo -WorkDir $benchDir -CargoArgs $buildArgs | Out-Null
}

$isWindowsHost = $env:OS -eq "Windows_NT"
$txgenExe = if ($isWindowsHost) { "novovm-txgen.exe" } else { "novovm-txgen" }
$clusterExe = if ($isWindowsHost) { "novovm-consensus-cluster-node.exe" } else { "novovm-consensus-cluster-node" }
$txgenPath = Join-Path $RepoRoot ("target\{0}\{1}" -f $BuildProfile, $txgenExe)
$clusterPath = Join-Path $RepoRoot ("target\{0}\{1}" -f $BuildProfile, $clusterExe)
if (-not (Test-Path $txgenPath)) { throw "txgen executable not found: $txgenPath" }
if (-not (Test-Path $clusterPath)) { throw "cluster executable not found: $clusterPath" }

$txWirePath = Join-Path $OutputDir "cluster.txwire.bin"
Write-Host ("cluster stress: generating tx wire (txs={0}, accounts={1}) ..." -f $Txs, $Accounts)
Invoke-Cargo -WorkDir $benchDir -CargoArgs @("run", "--quiet", "--bin", "novovm-txgen", "--", "--out", $txWirePath, "--txs", "$Txs", "--accounts", "$Accounts") | Out-Null
if (-not (Test-Path $txWirePath)) { throw "tx wire not generated: $txWirePath" }
$txWirePath = [System.IO.Path]::GetFullPath($txWirePath)

$expectedBatches = [Math]::Ceiling($Txs / [double]$BatchSize)
$expectedBatches = [Math]::Min($expectedBatches, $MaxBatches)
if ($expectedBatches -lt 1) { throw "expectedBatches computed as 0" }

$nodeSpecs = @()
for ($i = 0; $i -lt $Validators; $i++) {
    $nodeHost = $nodeHostsExpanded[$i]
    $listen = "{0}:{1}" -f $nodeHost, ($BasePort + $i)
    $peerParts = @()
    for ($j = 0; $j -lt $Validators; $j++) {
        if ($j -eq $i) { continue }
        $peerParts += ("{0}={1}:{2}" -f $j, $nodeHostsExpanded[$j], ($BasePort + $j))
    }
    $nodeSpecs += [pscustomobject]@{
        node_id = $i
        host = $nodeHost
        is_remote = (-not (Test-LocalHostName $nodeHost))
        listen_addr = $listen
        peers = ($peerParts -join ",")
        summary = (Join-Path $OutputDir ("node-{0}.summary.json" -f $i))
        stdout = (Join-Path $OutputDir ("node-{0}.stdout.log" -f $i))
        stderr = (Join-Path $OutputDir ("node-{0}.stderr.log" -f $i))
    }
}

$sw = [System.Diagnostics.Stopwatch]::StartNew()
$procHandles = @()
try {
    foreach ($spec in $nodeSpecs | Where-Object { $_.node_id -ne 0 }) {
        $envMap = @{
            NOVOVM_CLUSTER_NODE_ID = "$($spec.node_id)"
            NOVOVM_CLUSTER_VALIDATORS = "$Validators"
            NOVOVM_CLUSTER_LEADER_ID = "0"
            NOVOVM_CLUSTER_TRANSPORT = $Transport
            NOVOVM_CLUSTER_LISTEN_ADDR = $spec.listen_addr
            NOVOVM_CLUSTER_PEERS = $spec.peers
            NOVOVM_CLUSTER_EXPECTED_BATCHES = "$expectedBatches"
            NOVOVM_CLUSTER_TIMEOUT_SEC = "$TimeoutSec"
            NOVOVM_E2E_BATCH_SIZE = "$BatchSize"
            NOVOVM_E2E_MAX_BATCHES = "$MaxBatches"
            NOVOVM_E2E_SUMMARY_OUT = $spec.summary
        }
        if (-not $spec.is_remote) {
            $proc = Start-ManagedProcess -FilePath $clusterPath -Arguments "" -WorkDir (Split-Path $clusterPath -Parent) -EnvMap $envMap -StdoutPath $spec.stdout -StderrPath $spec.stderr
        } else {
            $target = if ($SshUser) { "$SshUser@$($spec.host)" } else { [string]$spec.host }
            $remoteClusterPath = Join-Path $RemoteRepoRoot ("target\{0}\{1}" -f $BuildProfile, $clusterExe)
            $cmdParts = @()
            foreach ($k in $envMap.Keys) {
                $v = [string]$envMap[$k]
                $cmdParts += ('$env:{0}=''{1}''' -f $k, ($v -replace '''', ''''''))
            }
            $cmdParts += ("& '{0}'" -f ($remoteClusterPath -replace "'", "''"))
            $psCommand = $cmdParts -join "; "
            $bytes = [System.Text.Encoding]::Unicode.GetBytes($psCommand)
            $encoded = [Convert]::ToBase64String($bytes)
            $sshArgs = "$target powershell -NoProfile -NonInteractive -EncodedCommand $encoded"
            $proc = Start-ManagedProcess -FilePath "ssh" -Arguments $sshArgs -WorkDir $RepoRoot -EnvMap @{} -StdoutPath $spec.stdout -StderrPath $spec.stderr
        }
        $procHandles += [pscustomobject]@{ Spec = $spec; Handle = $proc }
    }

    Start-Sleep -Milliseconds 1000

    $leader = $nodeSpecs | Where-Object { $_.node_id -eq 0 } | Select-Object -First 1
    $leaderEnv = @{
        NOVOVM_CLUSTER_NODE_ID = "0"
        NOVOVM_CLUSTER_VALIDATORS = "$Validators"
        NOVOVM_CLUSTER_LEADER_ID = "0"
        NOVOVM_CLUSTER_TRANSPORT = $Transport
        NOVOVM_CLUSTER_LISTEN_ADDR = $leader.listen_addr
        NOVOVM_CLUSTER_PEERS = $leader.peers
        NOVOVM_CLUSTER_EXPECTED_BATCHES = "$expectedBatches"
        NOVOVM_CLUSTER_TIMEOUT_SEC = "$TimeoutSec"
        NOVOVM_E2E_BATCH_SIZE = "$BatchSize"
        NOVOVM_E2E_MAX_BATCHES = "$MaxBatches"
        NOVOVM_E2E_SUMMARY_OUT = $leader.summary
        NOVOVM_TX_WIRE_FILE = $txWirePath
        NOVOVM_EXEC_PATH = "ffi_v2"
        NOVOVM_AOEM_VARIANT = $AoemVariant
        NOVOVM_AOEM_PLUGIN_DIR = $AoemPluginDir
        NOVOVM_D1_INGRESS_MODE = $D1IngressMode
    }
    $leaderProc = Start-ManagedProcess -FilePath $clusterPath -Arguments "" -WorkDir (Split-Path $clusterPath -Parent) -EnvMap $leaderEnv -StdoutPath $leader.stdout -StderrPath $leader.stderr
    $procHandles += [pscustomobject]@{ Spec = $leader; Handle = $leaderProc }

    $waitOrder = @(
        ($procHandles | Where-Object { $_.Spec.node_id -eq 0 }),
        ($procHandles | Where-Object { $_.Spec.node_id -ne 0 } | Sort-Object { $_.Spec.node_id })
    ) | ForEach-Object { $_ }
    foreach ($entry in $waitOrder) {
        $proc = $entry.Handle.Process
        $timedOut = -not $proc.WaitForExit($TimeoutSec * 1000)
        if ($timedOut) {
            try { $proc.Kill() } catch {}
            throw "node process timeout: node_id=$($entry.Spec.node_id)"
        }
        $proc.WaitForExit()
        $outText = $entry.Handle.OutTask.GetAwaiter().GetResult()
        $errText = $entry.Handle.ErrTask.GetAwaiter().GetResult()
        $entry.Handle.OutWriter.Write($outText)
        $entry.Handle.ErrWriter.Write($errText)
        $entry.Handle.OutWriter.Dispose()
        $entry.Handle.ErrWriter.Dispose()
        if ($entry.Spec.is_remote -and -not (Test-Path $entry.Spec.summary)) {
            $jsonText = Extract-JsonObjectText $outText
            if ($jsonText) {
                $jsonText | Set-Content -Path $entry.Spec.summary -Encoding UTF8
            }
        }
        if ($proc.ExitCode -ne 0) {
            throw "node process failed: node_id=$($entry.Spec.node_id) exit=$($proc.ExitCode)`n$errText"
        }
    }
} finally {
    $sw.Stop()
}

$nodeSummaries = @()
foreach ($spec in $nodeSpecs) {
    if (-not (Test-Path $spec.summary)) {
        throw "missing node summary: $($spec.summary)"
    }
    $nodeSummaries += (Get-Content $spec.summary -Raw | ConvertFrom-Json)
}
$leaderSummary = $nodeSummaries | Where-Object { $_.node_id -eq 0 } | Select-Object -First 1
if ($null -eq $leaderSummary) {
    throw "leader summary not found"
}

$clusterSummary = [ordered]@{
    generated_at_utc = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds().ToString()
    transport = $Transport
    validators = $Validators
    txs_total = $Txs
    accounts = $Accounts
    batch_size = $BatchSize
    expected_batches = $expectedBatches
    d1_ingress_mode = $D1IngressMode
    aoem_variant = $AoemVariant
    wall_ms = [Math]::Round($sw.Elapsed.TotalMilliseconds, 2)
    consensus_tps_p50 = [double]$leaderSummary.consensus_tps_p50
    consensus_tps_p90 = [double]$leaderSummary.consensus_tps_p90
    consensus_tps_p99 = [double]$leaderSummary.consensus_tps_p99
    consensus_latency_ms_p50 = [double]$leaderSummary.consensus_latency_ms_p50
    consensus_latency_ms_p90 = [double]$leaderSummary.consensus_latency_ms_p90
    consensus_latency_ms_p99 = [double]$leaderSummary.consensus_latency_ms_p99
    aoem_kernel_tps_p50 = [double]$leaderSummary.aoem_kernel_tps_p50
    aoem_kernel_tps_p90 = [double]$leaderSummary.aoem_kernel_tps_p90
    aoem_kernel_tps_p99 = [double]$leaderSummary.aoem_kernel_tps_p99
    leader_runtime_total_ms = [double]$leaderSummary.runtime_total_ms
    leader_network_message_count = [int64]$leaderSummary.network_message_count
    leader_network_message_bytes = [int64]$leaderSummary.network_message_bytes
    nodes = $nodeSummaries
}

$summaryJsonPath = Join-Path $OutputDir "cluster-summary.json"
$summaryMdPath = Join-Path $OutputDir "cluster-summary.md"
$clusterSummary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJsonPath -Encoding UTF8

$md = @()
$md += "# NOVOVM Multi-Process Cluster Stress Summary"
$md += ""
$md += "- transport: $Transport"
$md += "- validators: $Validators"
$md += "- txs_total: $Txs"
$md += "- batch_size: $BatchSize"
$md += "- expected_batches: $expectedBatches"
$md += "- d1_ingress_mode: $D1IngressMode"
$md += "- aoem_variant: $AoemVariant"
$md += "- wall_ms: $([Math]::Round($sw.Elapsed.TotalMilliseconds, 2))"
$md += ""
$md += "## Leader KPI"
$md += ""
$md += "- consensus_tps p50/p90/p99: $($leaderSummary.consensus_tps_p50) / $($leaderSummary.consensus_tps_p90) / $($leaderSummary.consensus_tps_p99)"
$md += "- consensus_latency_ms p50/p90/p99: $($leaderSummary.consensus_latency_ms_p50) / $($leaderSummary.consensus_latency_ms_p90) / $($leaderSummary.consensus_latency_ms_p99)"
$md += "- aoem_kernel_tps p50/p90/p99: $($leaderSummary.aoem_kernel_tps_p50) / $($leaderSummary.aoem_kernel_tps_p90) / $($leaderSummary.aoem_kernel_tps_p99)"
$md += "- leader_runtime_total_ms: $($leaderSummary.runtime_total_ms)"
$md += "- leader_network_message_count: $($leaderSummary.network_message_count)"
$md += "- leader_network_message_bytes: $($leaderSummary.network_message_bytes)"
$md += ""
$md += "## Artifacts"
$md += ""
$md += "- cluster_summary_json: $summaryJsonPath"
$md += "- output_dir: $OutputDir"
$md -join "`n" | Set-Content -Path $summaryMdPath -Encoding UTF8

Write-Host "cluster stress report generated:"
Write-Host "  summary_json: $summaryJsonPath"
Write-Host "  summary_md:   $summaryMdPath"
Write-Host "  consensus_tps_p50/p90/p99: $($leaderSummary.consensus_tps_p50) / $($leaderSummary.consensus_tps_p90) / $($leaderSummary.consensus_tps_p99)"
