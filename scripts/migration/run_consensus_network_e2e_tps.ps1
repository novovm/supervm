param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [string]$DocOutputPath = "",
    [string]$RawCsvOutputPath = "",
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
    [ValidateSet("core", "persist", "wasm")]
    [string]$AoemVariant = "persist",
    [string]$AoemPluginDir = "",
    [ValidateSet("auto", "ops_wire_v1", "ops_v2")]
    [string]$D1IngressMode = "auto",
    [string]$D1Codec = "",
    [ValidateSet("release", "debug")]
    [string]$BuildProfile = "release",
    [switch]$SkipBuild,
    [ValidateRange(30, 7200)]
    [int]$TimeoutSec = 1200
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

$dateTag = Get-Date -Format "yyyy-MM-dd"
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\consensus-network-e2e-tps-$dateTag"
}
if (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

if (-not $AoemPluginDir) {
    $AoemPluginDir = Join-Path $RepoRoot "aoem\plugins"
}
if (-not (Test-Path $AoemPluginDir)) {
    throw "aoem plugin dir not found: $AoemPluginDir"
}
$AoemPluginDir = (Resolve-Path $AoemPluginDir).Path

if (-not $DocOutputPath) {
    $DocOutputPath = Join-Path $RepoRoot "docs_CN\CONSENSUS\NOVOVM-CONSENSUS-NETWORK-E2E-TPS-SEAL-$dateTag.md"
}
if (-not [System.IO.Path]::IsPathRooted($DocOutputPath)) {
    $DocOutputPath = Join-Path $RepoRoot $DocOutputPath
}
$DocOutputPath = [System.IO.Path]::GetFullPath($DocOutputPath)
New-Item -ItemType Directory -Force -Path ([System.IO.Path]::GetDirectoryName($DocOutputPath)) | Out-Null

if (-not $RawCsvOutputPath) {
    $RawCsvOutputPath = Join-Path $RepoRoot "docs_CN\CONSENSUS\NOVOVM-CONSENSUS-NETWORK-E2E-TPS-RAW-$dateTag.csv"
}
if (-not [System.IO.Path]::IsPathRooted($RawCsvOutputPath)) {
    $RawCsvOutputPath = Join-Path $RepoRoot $RawCsvOutputPath
}
$RawCsvOutputPath = [System.IO.Path]::GetFullPath($RawCsvOutputPath)
New-Item -ItemType Directory -Force -Path ([System.IO.Path]::GetDirectoryName($RawCsvOutputPath)) | Out-Null

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

$nodeDir = Join-Path $RepoRoot "crates\novovm-node"
if (-not $SkipBuild.IsPresent) {
    Write-Host ("consensus network e2e: building binaries ({0}) ..." -f $BuildProfile)
    $buildArgs = @("build")
    if ($BuildProfile -eq "release") { $buildArgs += "--release" }
    $buildArgs += @("--bin", "novovm-txgen", "--bin", "novovm-consensus-network-e2e")
    Invoke-Cargo -WorkDir $nodeDir -CargoArgs $buildArgs | Out-Null
}

$isWindowsHost = $env:OS -eq "Windows_NT"
$txgenExe = if ($isWindowsHost) { "novovm-txgen.exe" } else { "novovm-txgen" }
$benchExe = if ($isWindowsHost) { "novovm-consensus-network-e2e.exe" } else { "novovm-consensus-network-e2e" }
$txgenPath = Join-Path $RepoRoot ("target\{0}\{1}" -f $BuildProfile, $txgenExe)
$benchPath = Join-Path $RepoRoot ("target\{0}\{1}" -f $BuildProfile, $benchExe)
if (-not (Test-Path $txgenPath)) { throw "txgen executable not found: $txgenPath" }
if (-not (Test-Path $benchPath)) { throw "bench executable not found: $benchPath" }

$txWirePath = Join-Path $OutputDir "consensus-network-e2e.txwire.bin"
Write-Host ("consensus network e2e: generating tx wire (txs={0}, accounts={1}) ..." -f $Txs, $Accounts)
Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("run", "--quiet", "--bin", "novovm-txgen", "--", "--out", $txWirePath, "--txs", "$Txs", "--accounts", "$Accounts") | Out-Null
if (-not (Test-Path $txWirePath)) {
    throw "tx wire file not generated: $txWirePath"
}
$txWirePath = [System.IO.Path]::GetFullPath($txWirePath)

$summaryJsonPath = Join-Path $OutputDir "consensus-network-e2e-summary.json"
$stdoutPath = Join-Path $OutputDir "consensus-network-e2e.stdout.log"
$stderrPath = Join-Path $OutputDir "consensus-network-e2e.stderr.log"
if (Test-Path $stdoutPath) { Remove-Item $stdoutPath -Force }
if (Test-Path $stderrPath) { Remove-Item $stderrPath -Force }

$envMap = @{
    NOVOVM_EXEC_PATH = "ffi_v2"
    NOVOVM_TX_WIRE_FILE = $txWirePath
    NOVOVM_AOEM_VARIANT = $AoemVariant
    NOVOVM_AOEM_PLUGIN_DIR = $AoemPluginDir
    NOVOVM_D1_INGRESS_MODE = $D1IngressMode
    NOVOVM_E2E_BATCH_SIZE = "$BatchSize"
    NOVOVM_E2E_VALIDATORS = "$Validators"
    NOVOVM_E2E_MAX_BATCHES = "$MaxBatches"
    NOVOVM_E2E_SUMMARY_OUT = $summaryJsonPath
}
if ($D1Codec -and $D1Codec.Trim().Length -gt 0) {
    $envMap["NOVOVM_D1_CODEC"] = $D1Codec.Trim()
}
$previousEnv = @{}
foreach ($entry in $envMap.GetEnumerator()) {
    $previousEnv[$entry.Key] = [Environment]::GetEnvironmentVariable($entry.Key, "Process")
    Set-Item -Path ("Env:{0}" -f $entry.Key) -Value $entry.Value
}

$exitCode = -1
$sw = [System.Diagnostics.Stopwatch]::StartNew()
try {
    $proc = Start-Process `
        -FilePath $benchPath `
        -WorkingDirectory (Split-Path $benchPath -Parent) `
        -NoNewWindow `
        -PassThru `
        -RedirectStandardOutput $stdoutPath `
        -RedirectStandardError $stderrPath
    $timedOut = -not $proc.WaitForExit($TimeoutSec * 1000)
    if ($timedOut) {
        try { $proc.Kill() } catch {}
        throw "consensus network e2e timed out after $TimeoutSec sec"
    }
    $proc.WaitForExit()
    $proc.Refresh()
    $exitCode = [int]$proc.ExitCode
} finally {
    $sw.Stop()
    foreach ($key in $previousEnv.Keys) {
        $prior = $previousEnv[$key]
        if ($null -eq $prior -or $prior -eq "") {
            Remove-Item -Path ("Env:{0}" -f $key) -ErrorAction SilentlyContinue
        } else {
            Set-Item -Path ("Env:{0}" -f $key) -Value $prior
        }
    }
}

$stdoutText = if (Test-Path $stdoutPath) { Get-Content $stdoutPath -Raw } else { "" }
$stderrText = if (Test-Path $stderrPath) { Get-Content $stderrPath -Raw } else { "" }
if ($exitCode -ne 0) {
    throw "consensus network e2e failed: exit=$exitCode`n$stderrText"
}
if (-not (Test-Path $summaryJsonPath)) {
    throw "summary json missing: $summaryJsonPath`n$stdoutText"
}

$summary = Get-Content $summaryJsonPath -Raw | ConvertFrom-Json

$csvRows = @(
    [pscustomobject]@{
        generated_at_utc = $summary.generated_at_utc
        variant = $summary.variant
        d1_ingress_mode = $summary.d1_ingress_mode
        d1_input_source = $summary.d1_input_source
        d1_codec = $summary.d1_codec
        aoem_ingress_path = $summary.aoem_ingress_path
        validators = $summary.validators
        txs_total = $summary.txs_total
        batches = $summary.batches
        batch_size = $summary.batch_size
        consensus_network_e2e_tps_p50 = $summary.consensus_network_e2e_tps_p50
        consensus_network_e2e_tps_p90 = $summary.consensus_network_e2e_tps_p90
        consensus_network_e2e_tps_p99 = $summary.consensus_network_e2e_tps_p99
        consensus_network_e2e_latency_ms_p50 = $summary.consensus_network_e2e_latency_ms_p50
        consensus_network_e2e_latency_ms_p90 = $summary.consensus_network_e2e_latency_ms_p90
        consensus_network_e2e_latency_ms_p99 = $summary.consensus_network_e2e_latency_ms_p99
        aoem_kernel_tps_p50 = $summary.aoem_kernel_tps_p50
        aoem_kernel_tps_p90 = $summary.aoem_kernel_tps_p90
        aoem_kernel_tps_p99 = $summary.aoem_kernel_tps_p99
        network_message_count = $summary.network_message_count
        network_message_bytes = $summary.network_message_bytes
        wall_ms = [Math]::Round($sw.Elapsed.TotalMilliseconds, 2)
    }
)
$csvRows | Export-Csv -Path $RawCsvOutputPath -NoTypeInformation -Encoding UTF8

$summaryMdPath = Join-Path $OutputDir "consensus-network-e2e-summary.md"
$md = @()
$md += "# NOVOVM Consensus Network E2E TPS Seal ($dateTag)"
$md += ""
$md += "- path: consensus + network + aoem (single-process multi-node simulation)"
$md += "- variant: $AoemVariant"
$md += "- d1_ingress_mode: $($summary.d1_ingress_mode)"
$md += "- d1_input_source: $($summary.d1_input_source)"
$md += "- d1_codec: $($summary.d1_codec)"
$md += "- aoem_ingress_path: $($summary.aoem_ingress_path)"
$md += "- txs_total: $($summary.txs_total)"
$md += "- validators: $($summary.validators)"
$md += "- batches: $($summary.batches)"
$md += "- batch_size: $($summary.batch_size)"
$md += "- wall_ms: $([Math]::Round($sw.Elapsed.TotalMilliseconds, 2))"
$md += ""
$md += "## TPS / Latency"
$md += ""
$md += "- consensus_network_e2e_tps p50/p90/p99: $($summary.consensus_network_e2e_tps_p50) / $($summary.consensus_network_e2e_tps_p90) / $($summary.consensus_network_e2e_tps_p99)"
$md += "- consensus_network_e2e_latency_ms p50/p90/p99: $($summary.consensus_network_e2e_latency_ms_p50) / $($summary.consensus_network_e2e_latency_ms_p90) / $($summary.consensus_network_e2e_latency_ms_p99)"
$md += "- aoem_kernel_tps p50/p90/p99: $($summary.aoem_kernel_tps_p50) / $($summary.aoem_kernel_tps_p90) / $($summary.aoem_kernel_tps_p99)"
$md += "- network_message_count: $($summary.network_message_count)"
$md += "- network_message_bytes: $($summary.network_message_bytes)"
$md += ""
$md += "## Artifacts"
$md += ""
$md += "- summary_json: $summaryJsonPath"
$md += "- raw_csv: $RawCsvOutputPath"
$md += "- stdout: $stdoutPath"
$md += "- stderr: $stderrPath"
$md -join "`n" | Set-Content -Path $summaryMdPath -Encoding UTF8

Copy-Item -Path $summaryMdPath -Destination $DocOutputPath -Force

Write-Host "consensus network e2e tps report generated:"
Write-Host "  summary_json: $summaryJsonPath"
Write-Host "  summary_md:   $summaryMdPath"
Write-Host "  raw_csv:      $RawCsvOutputPath"
Write-Host "  doc_md:       $DocOutputPath"
Write-Host "  consensus_network_e2e_tps_p50/p90/p99: $($summary.consensus_network_e2e_tps_p50) / $($summary.consensus_network_e2e_tps_p90) / $($summary.consensus_network_e2e_tps_p99)"
