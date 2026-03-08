param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [string]$DocOutputPath = "",
    [string]$RawCsvOutputPath = "",
    [ValidateRange(2, 2000)]
    [int]$Repeats = 20,
    [ValidateRange(1, 50000000)]
    [int]$Txs = 100000,
    [ValidateRange(1, 50000000)]
    [int]$Accounts = 100000,
    [ValidateSet("core", "persist", "wasm")]
    [string]$AoemVariant = "persist",
    [string]$AoemPluginDir = "",
    [ValidateSet("auto", "ops_wire_v1", "ops_v2")]
    [string]$D1IngressMode = "auto",
    [string]$D1Codec = "",
    [ValidateSet("release", "debug")]
    [string]$BuildProfile = "release",
    [switch]$SkipBuild,
    [ValidateRange(30, 3600)]
    [int]$TimeoutSec = 600
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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\prod-node-steady-tps-$dateTag"
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
    $DocOutputPath = Join-Path $RepoRoot "docs_CN\AOEM-FFI\AOEM-PROD-STEADY-TPS-SEAL-$dateTag.md"
}
if (-not [System.IO.Path]::IsPathRooted($DocOutputPath)) {
    $DocOutputPath = Join-Path $RepoRoot $DocOutputPath
}
$DocOutputPath = [System.IO.Path]::GetFullPath($DocOutputPath)
New-Item -ItemType Directory -Force -Path ([System.IO.Path]::GetDirectoryName($DocOutputPath)) | Out-Null

if (-not $RawCsvOutputPath) {
    $RawCsvOutputPath = Join-Path $RepoRoot "docs_CN\AOEM-FFI\AOEM-PROD-STEADY-TPS-RAW-$dateTag.csv"
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
}

function Get-NearestRankQuantile {
    param(
        [object[]]$Values,
        [double]$Quantile
    )
    if (-not $Values -or $Values.Count -eq 0) {
        return $null
    }
    $numeric = New-Object System.Collections.Generic.List[double]
    foreach ($v in $Values) {
        if ($null -eq $v) { continue }
        $numeric.Add([double]$v)
    }
    if ($numeric.Count -eq 0) {
        return $null
    }
    $sorted = $numeric.ToArray()
    [Array]::Sort($sorted)
    $n = $sorted.Length
    $rank = [int][Math]::Ceiling($Quantile * $n)
    if ($rank -lt 1) { $rank = 1 }
    if ($rank -gt $n) { $rank = $n }
    return [Math]::Round($sorted[$rank - 1], 2)
}

function Parse-IngressContractLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^d1_ingress_contract: " } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^d1_ingress_contract: mode=(?<mode>\S+) source=(?<source>\S+) codec=(?<codec>\S+) aoem_ingress_path=(?<path>\S+)$"
    )
    if (-not $m.Success) { return $null }
    return [pscustomobject]@{
        mode = $m.Groups["mode"].Value
        source = $m.Groups["source"].Value
        codec = $m.Groups["codec"].Value
        aoem_ingress_path = $m.Groups["path"].Value
        raw = $line
    }
}

$nodeDir = Join-Path $RepoRoot "crates\novovm-node"
if (-not $SkipBuild.IsPresent) {
    Write-Host ("steady tps: building novovm-node/novovm-txgen ({0}) ..." -f $BuildProfile)
    $buildArgs = @("build")
    if ($BuildProfile -eq "release") { $buildArgs += "--release" }
    $buildArgs += @("--bin", "novovm-node", "--bin", "novovm-txgen")
    Invoke-Cargo -WorkDir $nodeDir -CargoArgs $buildArgs
}

$isWindowsHost = $env:OS -eq "Windows_NT"
$exeName = if ($isWindowsHost) { "novovm-node.exe" } else { "novovm-node" }
$exePath = Join-Path $RepoRoot ("target\{0}\{1}" -f $BuildProfile, $exeName)
if (-not (Test-Path $exePath)) {
    throw "novovm-node executable not found: $exePath"
}

$txWirePath = Join-Path $OutputDir "steady.txwire.bin"
Write-Host ("steady tps: generating tx wire (txs={0}, accounts={1}) ..." -f $Txs, $Accounts)
Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("run", "--quiet", "--bin", "novovm-txgen", "--", "--out", $txWirePath, "--txs", "$Txs", "--accounts", "$Accounts")
if (-not (Test-Path $txWirePath)) {
    throw "tx wire file not generated: $txWirePath"
}
$txWirePath = [System.IO.Path]::GetFullPath($txWirePath)

$stdoutPath = Join-Path $OutputDir "steady.stdout.log"
$stderrPath = Join-Path $OutputDir "steady.stderr.log"
if (Test-Path $stdoutPath) { Remove-Item $stdoutPath -Force }
if (Test-Path $stderrPath) { Remove-Item $stderrPath -Force }

$envMap = @{
    NOVOVM_NODE_MODE = "full"
    NOVOVM_EXEC_PATH = "ffi_v2"
    NOVOVM_TX_WIRE_FILE = $txWirePath
    NOVOVM_TX_REPEAT_COUNT = "$Repeats"
    NOVOVM_AOEM_VARIANT = $AoemVariant
    NOVOVM_AOEM_PLUGIN_DIR = $AoemPluginDir
    NOVOVM_D1_INGRESS_MODE = $D1IngressMode
    NOVOVM_ENABLE_HOST_ADMISSION = "0"
}
if ($D1Codec -and $D1Codec.Trim().Length -gt 0) {
    $envMap["NOVOVM_D1_CODEC"] = $D1Codec.Trim()
}
$previousEnv = @{}
foreach ($entry in $envMap.GetEnumerator()) {
    $previousEnv[$entry.Key] = [Environment]::GetEnvironmentVariable($entry.Key, "Process")
    Set-Item -Path ("Env:{0}" -f $entry.Key) -Value $entry.Value
}

$wallSw = [System.Diagnostics.Stopwatch]::StartNew()
$exitCode = -1
try {
    $proc = Start-Process `
        -FilePath $exePath `
        -WorkingDirectory (Split-Path $exePath -Parent) `
        -NoNewWindow `
        -PassThru `
        -RedirectStandardOutput $stdoutPath `
        -RedirectStandardError $stderrPath
    $timedOut = -not $proc.WaitForExit($TimeoutSec * 1000)
    if ($timedOut) {
        try { $proc.Kill() } catch {}
        throw "novovm-node timed out after $TimeoutSec sec"
    }
    $proc.WaitForExit()
    $proc.Refresh()
    $exitCode = [int]$proc.ExitCode
} finally {
    $wallSw.Stop()
    foreach ($key in $previousEnv.Keys) {
        $prior = $previousEnv[$key]
        if ($null -eq $prior -or $prior -eq "") {
            Remove-Item -Path ("Env:{0}" -f $key) -ErrorAction SilentlyContinue
        } else {
            Set-Item -Path ("Env:{0}" -f $key) -Value $prior
        }
    }
}

$stdoutText = if (Test-Path $stdoutPath) { Get-Content -Path $stdoutPath -Raw } else { "" }
$stderrText = if (Test-Path $stderrPath) { Get-Content -Path $stderrPath -Raw } else { "" }
if ($exitCode -ne 0) {
    throw "novovm-node steady run failed: exit=$exitCode`n$stderrText"
}

$runMatches = [regex]::Matches(
    $stdoutText,
    "mode=ffi_v2 run=(?<idx>\d+)/(?<total>\d+) variant=(?<variant>\w+) dll=(?<dll>.+?) rc=(?<rc>\d+)\((?<rc_name>[^)]+)\) submitted=(?<submitted>\d+) processed=(?<processed>\d+) success=(?<success>\d+) writes=(?<writes>\d+) elapsed_us=(?<elapsed>\d+) host_elapsed_us=(?<host_elapsed>\d+)"
)
if ($runMatches.Count -le 0) {
    throw "steady run parse failed: no mode=ffi_v2 run lines found"
}
$ingress = Parse-IngressContractLine -Text $stdoutText
if ($null -eq $ingress) {
    throw "steady run parse failed: d1_ingress_contract line missing"
}

$aggMatch = [regex]::Match(
    $stdoutText,
    "mode=ffi_v2_aggregate variant=(?<variant>\w+) dll=(?<dll>.+?) rc=(?<rc>\d+)\((?<rc_name>[^)]+)\) repeats=(?<repeats>\d+) submitted_total=(?<submitted_total>\d+) processed_total=(?<processed_total>\d+) success_total=(?<success_total>\d+) writes_total=(?<writes_total>\d+) host_exec_us=(?<host_exec>\d+) aoem_exec_us=(?<aoem_exec>\d+)"
)
if (-not $aggMatch.Success) {
    throw "steady run parse failed: aggregate line missing"
}

$hostSamples = @()
$kernelSamples = @()
$csvRows = @()
foreach ($m in $runMatches) {
    $processed = [double]$m.Groups["processed"].Value
    $hostUs = [double]$m.Groups["host_elapsed"].Value
    $aoemUs = [double]$m.Groups["elapsed"].Value
    if ($hostUs -le 0 -or $aoemUs -le 0) {
        throw "steady run parse failed: invalid elapsed_us in run line"
    }
    $hostTps = [Math]::Round($processed * 1000000.0 / $hostUs, 2)
    $kernelTps = [Math]::Round($processed * 1000000.0 / $aoemUs, 2)
    $hostSamples += $hostTps
    $kernelSamples += $kernelTps
    $csvRows += [pscustomobject]@{
        run = [int]$m.Groups["idx"].Value
        host_pipeline_diag_tps_steady = $hostTps
        aoem_kernel_tps = $kernelTps
        processed = [int64]$processed
        host_elapsed_us = [int64]$hostUs
        aoem_elapsed_us = [int64]$aoemUs
    }
}

$processedTotal = [double]$aggMatch.Groups["processed_total"].Value
$hostExecUs = [double]$aggMatch.Groups["host_exec"].Value
$aoemExecUs = [double]$aggMatch.Groups["aoem_exec"].Value
$repeatsActual = [int]$aggMatch.Groups["repeats"].Value

$summary = [pscustomobject]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    repo_root = $RepoRoot
    output_dir = $OutputDir
    variant = $AoemVariant
    path_mode = "ffi_v2"
    d1_ingress_mode = $ingress.mode
    d1_input_source = $ingress.source
    d1_codec = $ingress.codec
    aoem_ingress_path = $ingress.aoem_ingress_path
    txs = $Txs
    accounts = $Accounts
    repeats = $repeatsActual
    build_profile = $BuildProfile
    tx_wire_file = $txWirePath
    node_executable = $exePath
    wall_ms = [Math]::Round($wallSw.Elapsed.TotalMilliseconds, 2)
    host_pipeline_diag_tps_steady_p50 = Get-NearestRankQuantile -Values @($hostSamples) -Quantile 0.50
    host_pipeline_diag_tps_steady_p90 = Get-NearestRankQuantile -Values @($hostSamples) -Quantile 0.90
    host_pipeline_diag_tps_steady_p99 = Get-NearestRankQuantile -Values @($hostSamples) -Quantile 0.99
    host_pipeline_diag_tps_steady_scope = "single long-lived process; excludes per-repeat process startup/DLL load"
    host_pipeline_diag_cold_start_included = $false
    aoem_kernel_tps_p50 = Get-NearestRankQuantile -Values @($kernelSamples) -Quantile 0.50
    aoem_kernel_tps_p90 = Get-NearestRankQuantile -Values @($kernelSamples) -Quantile 0.90
    aoem_kernel_tps_p99 = Get-NearestRankQuantile -Values @($kernelSamples) -Quantile 0.99
    aoem_kernel_tps_scope = "derived from per-repeat AOEM elapsed_us in mode lines"
    host_pipeline_diag_tps_steady_aggregate = [Math]::Round($processedTotal * 1000000.0 / $hostExecUs, 2)
    aoem_kernel_tps_aggregate = [Math]::Round($processedTotal * 1000000.0 / $aoemExecUs, 2)
    consensus_network_e2e_tps_p50 = $null
    consensus_network_e2e_tps_p90 = $null
    consensus_network_e2e_tps_p99 = $null
    consensus_network_e2e_tps_note = "not_measured_in_single_process_steady_path"
    host_pipeline_diag_tps_steady_samples = @($hostSamples)
    aoem_kernel_tps_samples = @($kernelSamples)
    stdout = $stdoutPath
    stderr = $stderrPath
    ingress_line = $ingress.raw
}

$summaryJsonPath = Join-Path $OutputDir "prod-node-steady-tps-summary.json"
$summaryMdPath = Join-Path $OutputDir "prod-node-steady-tps-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJsonPath -Encoding UTF8
$csvRows | Export-Csv -Path $RawCsvOutputPath -NoTypeInformation -Encoding UTF8

$md = @()
$md += "# AOEM Production Steady TPS Seal ($dateTag)"
$md += ""
$md += "- binary: novovm-node"
$md += "- mode: ffi_v2 (production-only)"
$md += "- d1_ingress_mode: $($summary.d1_ingress_mode)"
$md += "- d1_input_source: $($summary.d1_input_source)"
$md += "- d1_codec: $($summary.d1_codec)"
$md += "- aoem_ingress_path: $($summary.aoem_ingress_path)"
$md += "- steady_mode: single process, in-process repeats"
$md += "- variant: $AoemVariant"
$md += "- txs_per_repeat: $Txs"
$md += "- repeats: $repeatsActual"
$md += "- tx_wire_file: $txWirePath"
$md += ""
$md += "## TPS"
$md += ""
$md += "- host_pipeline_diag_tps_steady p50/p90/p99: $($summary.host_pipeline_diag_tps_steady_p50) / $($summary.host_pipeline_diag_tps_steady_p90) / $($summary.host_pipeline_diag_tps_steady_p99)"
$md += "- aoem_kernel_tps p50/p90/p99: $($summary.aoem_kernel_tps_p50) / $($summary.aoem_kernel_tps_p90) / $($summary.aoem_kernel_tps_p99)"
$md += "- host_pipeline_diag_tps_steady_aggregate: $($summary.host_pipeline_diag_tps_steady_aggregate)"
$md += "- aoem_kernel_tps_aggregate: $($summary.aoem_kernel_tps_aggregate)"
$md += ""
$md += "## Notes"
$md += ""
$md += "- This script uses one long-lived process and one AOEM session."
$md += "- Cold-start overhead is excluded from per-repeat steady TPS."
$md += "- summary_json: $summaryJsonPath"
$md += "- raw_csv: $RawCsvOutputPath"
$md -join "`n" | Set-Content -Path $summaryMdPath -Encoding UTF8

Copy-Item -Path $summaryMdPath -Destination $DocOutputPath -Force

Write-Host "prod node steady tps report generated:"
Write-Host "  summary_json: $summaryJsonPath"
Write-Host "  summary_md:   $summaryMdPath"
Write-Host "  raw_csv:      $RawCsvOutputPath"
Write-Host "  doc_md:       $DocOutputPath"
Write-Host "  host_pipeline_diag_tps_steady_p50/p90/p99: $($summary.host_pipeline_diag_tps_steady_p50) / $($summary.host_pipeline_diag_tps_steady_p90) / $($summary.host_pipeline_diag_tps_steady_p99)"
