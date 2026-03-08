param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [string]$DocOutputPath = "",
    [string]$RawCsvOutputPath = "",
    [ValidateRange(1, 20)]
    [int]$Repeats = 3,
    [ValidateRange(1, 50000000)]
    [int]$Txs = 1000000,
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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\prod-node-e2e-tps-$dateTag"
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
    $DocOutputPath = Join-Path $RepoRoot "docs_CN\AOEM-FFI\AOEM-PROD-E2E-TPS-SEAL-$dateTag.md"
}
if (-not [System.IO.Path]::IsPathRooted($DocOutputPath)) {
    $DocOutputPath = Join-Path $RepoRoot $DocOutputPath
}
$DocOutputPath = [System.IO.Path]::GetFullPath($DocOutputPath)
New-Item -ItemType Directory -Force -Path ([System.IO.Path]::GetDirectoryName($DocOutputPath)) | Out-Null

if (-not $RawCsvOutputPath) {
    $RawCsvOutputPath = Join-Path $RepoRoot "docs_CN\AOEM-FFI\AOEM-PROD-E2E-TPS-RAW-$dateTag.csv"
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

function Parse-ModeLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^mode=ffi_v2 variant=" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^mode=ffi_v2 variant=(?<variant>\w+) dll=(?<dll>.+?) rc=(?<rc>\d+)\((?<rc_name>[^)]+)\) submitted=(?<submitted>\d+) processed=(?<processed>\d+) success=(?<success>\d+) writes=(?<writes>\d+) elapsed_us=(?<elapsed>\d+)$"
    )
    if (-not $m.Success) { return $null }
    return [pscustomobject]@{
        variant = $m.Groups["variant"].Value
        dll = $m.Groups["dll"].Value
        rc = [int]$m.Groups["rc"].Value
        rc_name = $m.Groups["rc_name"].Value
        submitted = [int64]$m.Groups["submitted"].Value
        processed = [int64]$m.Groups["processed"].Value
        success = [int64]$m.Groups["success"].Value
        writes = [int64]$m.Groups["writes"].Value
        elapsed_us = [int64]$m.Groups["elapsed"].Value
        raw = $line
    }
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

function Invoke-NodeRun {
    param(
        [string]$ExePath,
        [string]$TxWirePath,
        [int]$RunIndex,
        [string]$OutputDir,
        [int]$TimeoutSec,
        [string]$AoemVariant,
        [string]$AoemPluginDir
    )

    $stdoutPath = Join-Path $OutputDir ("run-{0}.stdout.log" -f $RunIndex)
    $stderrPath = Join-Path $OutputDir ("run-{0}.stderr.log" -f $RunIndex)
    if (Test-Path $stdoutPath) { Remove-Item $stdoutPath -Force }
    if (Test-Path $stderrPath) { Remove-Item $stderrPath -Force }

    $envMap = @{
        NOVOVM_NODE_MODE = "full"
        NOVOVM_EXEC_PATH = "ffi_v2"
        NOVOVM_TX_WIRE_FILE = $TxWirePath
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

    $exitCode = 1
    $stdoutText = ""
    $stderrText = ""
    $timedOut = $false
    $wallSw = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        $proc = Start-Process `
            -FilePath $ExePath `
            -WorkingDirectory (Split-Path $ExePath -Parent) `
            -NoNewWindow `
            -PassThru `
            -RedirectStandardOutput $stdoutPath `
            -RedirectStandardError $stderrPath

        $timedOut = -not $proc.WaitForExit($TimeoutSec * 1000)
        if ($timedOut) {
            try { $proc.Kill() } catch {}
        } else {
            $proc.Refresh()
            $exitCode = $proc.ExitCode
        }
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

    if (Test-Path $stdoutPath) {
        $stdoutText = Get-Content -Path $stdoutPath -Raw -ErrorAction SilentlyContinue
    }
    if (Test-Path $stderrPath) {
        $stderrText = Get-Content -Path $stderrPath -Raw -ErrorAction SilentlyContinue
    }

    $allText = $stdoutText + "`n" + $stderrText
    $mode = Parse-ModeLine -Text $allText
    $ingress = Parse-IngressContractLine -Text $allText
    $pass = (-not $timedOut -and $null -ne $mode -and $null -ne $ingress -and [int]$mode.rc -eq 0)

    return [pscustomobject]@{
        run = $RunIndex
        pass = $pass
        timed_out = $timedOut
        exit_code = $exitCode
        wall_ms = [Math]::Round($wallSw.Elapsed.TotalMilliseconds, 3)
        mode = $mode
        ingress = $ingress
        stdout = $stdoutPath
        stderr = $stderrPath
    }
}

$nodeDir = Join-Path $RepoRoot "crates\novovm-node"
$buildLabel = if ($BuildProfile -eq "release") { "release" } else { "debug" }
if ($SkipBuild.IsPresent) {
    Write-Host ("prod e2e: skip build requested ({0})" -f $buildLabel)
} else {
    Write-Host ("prod e2e: building novovm-node/novovm-txgen ({0}) ..." -f $buildLabel)
    $buildArgs = @("build")
    if ($BuildProfile -eq "release") {
        $buildArgs += "--release"
    }
    $buildArgs += @("--bin", "novovm-node", "--bin", "novovm-txgen")
    Invoke-Cargo -WorkDir $nodeDir -CargoArgs $buildArgs | Out-Null
}

$isWindowsHost = $env:OS -eq "Windows_NT"
$exeName = if ($isWindowsHost) { "novovm-node.exe" } else { "novovm-node" }
$exePath = Join-Path $RepoRoot ("target\{0}\{1}" -f $BuildProfile, $exeName)
if (-not (Test-Path $exePath)) {
    throw "novovm-node executable not found: $exePath"
}

$txWirePath = Join-Path $OutputDir "prod-e2e.txwire.bin"
Write-Host ("prod e2e: generating tx wire (txs={0}, accounts={1}) ..." -f $Txs, $Accounts)
Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("run", "--quiet", "--bin", "novovm-txgen", "--", "--out", $txWirePath, "--txs", "$Txs", "--accounts", "$Accounts") | Out-Null
if (-not (Test-Path $txWirePath)) {
    throw "tx wire file not generated: $txWirePath"
}
$txWirePath = [System.IO.Path]::GetFullPath($txWirePath)

$rows = New-Object System.Collections.Generic.List[object]
for ($run = 1; $run -le $Repeats; $run++) {
    Write-Host ("prod e2e: run {0}/{1} (variant={2}) ..." -f $run, $Repeats, $AoemVariant)
    $rows.Add([pscustomobject](Invoke-NodeRun -ExePath $exePath -TxWirePath $txWirePath -RunIndex $run -OutputDir $OutputDir -TimeoutSec $TimeoutSec -AoemVariant $AoemVariant -AoemPluginDir $AoemPluginDir)) | Out-Null
    Write-Host ("prod e2e: run {0}/{1} finished." -f $run, $Repeats)
}

$fails = @($rows | Where-Object { -not $_.pass })
if ($fails.Count -gt 0) {
    $failRuns = ($fails | ForEach-Object { $_.run }) -join ","
    throw "prod node e2e run failed on runs: $failRuns"
}

$hostPipelineDiagTps = @()
$kernelTps = @()
$wallMs = @()
foreach ($r in $rows) {
    $wall = [double]$r.wall_ms
    if ($wall -le 0) {
        throw "invalid wall_ms on run $($r.run)"
    }
    $wallMs += $wall
    $hostPipelineDiagTps += [Math]::Round(($Txs * 1000.0) / $wall, 2)

    $kernelElapsedUs = [double]$r.mode.elapsed_us
    if ($kernelElapsedUs -le 0) {
        throw "invalid mode elapsed_us on run $($r.run)"
    }
    $kernelTps += [Math]::Round(([double]$r.mode.processed * 1000000.0) / $kernelElapsedUs, 2)
}

$summary = [pscustomobject]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    repo_root = $RepoRoot
    output_dir = $OutputDir
    variant = $AoemVariant
    path_mode = "ffi_v2"
    d1_ingress_mode = @($rows | Select-Object -ExpandProperty ingress | Select-Object -ExpandProperty mode -Unique)
    d1_input_source = @($rows | Select-Object -ExpandProperty ingress | Select-Object -ExpandProperty source -Unique)
    d1_codec = @($rows | Select-Object -ExpandProperty ingress | Select-Object -ExpandProperty codec -Unique)
    aoem_ingress_path = @($rows | Select-Object -ExpandProperty ingress | Select-Object -ExpandProperty aoem_ingress_path -Unique)
    txs = $Txs
    accounts = $Accounts
    repeats = $Repeats
    build_profile = $BuildProfile
    tx_wire_file = $txWirePath
    node_executable = $exePath
    host_pipeline_diag_tps_p50 = Get-NearestRankQuantile -Values @($hostPipelineDiagTps) -Quantile 0.50
    host_pipeline_diag_tps_p90 = Get-NearestRankQuantile -Values @($hostPipelineDiagTps) -Quantile 0.90
    host_pipeline_diag_tps_p99 = Get-NearestRankQuantile -Values @($hostPipelineDiagTps) -Quantile 0.99
    host_pipeline_diag_scope = "single-process wall throughput per run (includes process startup + DLL load + ingress payload read + host marshaling)"
    host_pipeline_diag_cold_start_included = $true
    aoem_kernel_tps_p50 = Get-NearestRankQuantile -Values @($kernelTps) -Quantile 0.50
    aoem_kernel_tps_p90 = Get-NearestRankQuantile -Values @($kernelTps) -Quantile 0.90
    aoem_kernel_tps_p99 = Get-NearestRankQuantile -Values @($kernelTps) -Quantile 0.99
    aoem_kernel_tps_scope = "derived from AOEM elapsed_us in mode line (steady-state AOEM path, excludes host process startup)"
    consensus_network_e2e_tps_p50 = $null
    consensus_network_e2e_tps_p90 = $null
    consensus_network_e2e_tps_p99 = $null
    consensus_network_e2e_tps_note = "not_measured_in_single_node_ffi_v2_path; use scripts/migration/run_consensus_network_e2e_tps.ps1"
    wall_ms_p50 = Get-NearestRankQuantile -Values @($wallMs) -Quantile 0.50
    host_pipeline_diag_tps_samples = @($hostPipelineDiagTps)
    aoem_kernel_tps_samples = @($kernelTps)
}

if ($summary.d1_ingress_mode.Count -ne 1 -or $summary.d1_input_source.Count -ne 1 -or $summary.d1_codec.Count -ne 1 -or $summary.aoem_ingress_path.Count -ne 1) {
    throw "ingress contract drift across runs: mode=$($summary.d1_ingress_mode -join ',') source=$($summary.d1_input_source -join ',') codec=$($summary.d1_codec -join ',') path=$($summary.aoem_ingress_path -join ',')"
}
$summary.d1_ingress_mode = $summary.d1_ingress_mode[0]
$summary.d1_input_source = $summary.d1_input_source[0]
$summary.d1_codec = $summary.d1_codec[0]
$summary.aoem_ingress_path = $summary.aoem_ingress_path[0]

$summaryJsonPath = Join-Path $OutputDir "prod-node-e2e-tps-summary.json"
$summaryMdPath = Join-Path $OutputDir "prod-node-e2e-tps-summary.md"
$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryJsonPath -Encoding UTF8

$csvRows = @()
for ($i = 0; $i -lt $rows.Count; $i++) {
    $csvRows += [pscustomobject]@{
        run = $rows[$i].run
        variant = $AoemVariant
        txs = $Txs
        accounts = $Accounts
        host_pipeline_diag_tps = $hostPipelineDiagTps[$i]
        aoem_kernel_tps = $kernelTps[$i]
        wall_ms = [Math]::Round([double]$rows[$i].wall_ms, 3)
        mode_line = $rows[$i].mode.raw
        ingress_line = $rows[$i].ingress.raw
        d1_ingress_mode = $rows[$i].ingress.mode
        d1_input_source = $rows[$i].ingress.source
        d1_codec = $rows[$i].ingress.codec
        aoem_ingress_path = $rows[$i].ingress.aoem_ingress_path
        stdout = $rows[$i].stdout
        stderr = $rows[$i].stderr
    }
}
$csvRows | Export-Csv -Path $RawCsvOutputPath -NoTypeInformation -Encoding UTF8

$md = @()
$md += "# AOEM Production Path TPS Seal ($dateTag)"
$md += ""
$md += "- binary: novovm-node"
$md += "- mode: ffi_v2 (production-only)"
$md += "- d1_ingress_mode: $($summary.d1_ingress_mode)"
$md += "- d1_input_source: $($summary.d1_input_source)"
$md += "- d1_codec: $($summary.d1_codec)"
$md += "- aoem_ingress_path: $($summary.aoem_ingress_path)"
$md += "- variant: $AoemVariant"
$md += "- txs: $Txs"
$md += "- accounts: $Accounts"
$md += "- repeats: $Repeats"
$md += "- tx_wire_file: $txWirePath"
$md += "- executable: $exePath"
$md += ""
$md += "## TPS"
$md += ""
$md += "- host_pipeline_diag_tps p50/p90/p99: $($summary.host_pipeline_diag_tps_p50) / $($summary.host_pipeline_diag_tps_p90) / $($summary.host_pipeline_diag_tps_p99)"
$md += "- aoem_kernel_tps p50/p90/p99: $($summary.aoem_kernel_tps_p50) / $($summary.aoem_kernel_tps_p90) / $($summary.aoem_kernel_tps_p99)"
$md += "- consensus_network_e2e_tps p50/p90/p99: $($summary.consensus_network_e2e_tps_p50) / $($summary.consensus_network_e2e_tps_p90) / $($summary.consensus_network_e2e_tps_p99)"
$md += "- consensus_network_e2e_tps_note: $($summary.consensus_network_e2e_tps_note)"
$md += "- wall_ms p50: $($summary.wall_ms_p50)"
$md += ""
$md += "## Notes"
$md += ""
$md += "- This script measures production novovm-node path only."
$md += "- It does not call any gate/probe/legacy binary."
$md += "- host_pipeline_diag_tps includes process startup and DLL load for each run."
$md += "- host_pipeline_diag_tps includes ingress payload read + host marshaling."
$md += "- aoem_kernel_tps is derived from AOEM elapsed_us and excludes process startup."
$md += "- For strict steady-state publish numbers, prefer long-lived process mode (single process, reused AOEM handle/session)."
$md += ""
$md += "- summary_json: $summaryJsonPath"
$md += "- raw_csv: $RawCsvOutputPath"
$md += ""
$md += "## Reproduce"
$md += ""
$md += '```powershell'
$md += "& scripts/migration/run_prod_node_e2e_tps.ps1 -RepoRoot $RepoRoot -Repeats $Repeats -Txs $Txs -Accounts $Accounts -AoemVariant $AoemVariant -BuildProfile $BuildProfile -D1IngressMode $D1IngressMode -D1Codec '$D1Codec'"
$md += '```'
$md | Set-Content -Path $summaryMdPath -Encoding UTF8

Copy-Item -Path $summaryMdPath -Destination $DocOutputPath -Force

Write-Host "prod node e2e tps report generated:"
Write-Host "  summary_json: $summaryJsonPath"
Write-Host "  summary_md:   $summaryMdPath"
Write-Host "  raw_csv:      $RawCsvOutputPath"
Write-Host "  doc_md:       $DocOutputPath"
Write-Host "  host_pipeline_diag_tps_p50/p90/p99: $($summary.host_pipeline_diag_tps_p50) / $($summary.host_pipeline_diag_tps_p90) / $($summary.host_pipeline_diag_tps_p99)"
