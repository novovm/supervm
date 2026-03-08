param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [string]$DocOutputPath = "",
    [string]$RawCsvOutputPath = "",
    [ValidateRange(1, 20)]
    [int]$Repeats = 3,
    [ValidateRange(1, 2000000)]
    [int]$Txs = 10000,
    [ValidateRange(1, 2000000)]
    [int]$Accounts = 1024,
    [ValidateRange(30, 1800)]
    [int]$TimeoutSec = 180,
    [string]$AoemPluginDir = "",
    [string]$BuildProfile = "release",
    [ValidateRange(1, 4096)]
    [int]$BatchCount = 1,
    [ValidateRange(1, 512)]
    [int]$IngressWorkers = 16,
    [ValidateSet("fast", "full")]
    [string]$AdapterSignalMode = "fast",
    [string]$Profiles = "core_only,core_persist,core_wasm",
    [switch]$AllowDiagnosticWall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $AllowDiagnosticWall.IsPresent) {
    throw @"
run_tx_e2e_tps_core_sidecar_report.ps1 is DIAGNOSTIC ONLY and is blocked by default.
It reports host wall-clock throughput (process + ingress + encode + mempool + submit), not pure AOEM kernel TPS.
Use:
  1) scripts/migration/run_aoem_tps_core_sidecar_report.ps1   (kernel TPS as main KPI)
  2) scripts/migration/run_network_two_process.ps1            (network/consensus E2E TPS)
If you explicitly need host pipeline diagnostics, rerun with: -AllowDiagnosticWall `$true
"@
}

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

$dateTag = Get-Date -Format "yyyy-MM-dd"
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\tx-e2e-tps-core-sidecar-$dateTag"
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
    $DocOutputPath = Join-Path $RepoRoot "docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-E2E-TX-TPS-SEAL-$dateTag.md"
}
if (-not [System.IO.Path]::IsPathRooted($DocOutputPath)) {
    $DocOutputPath = Join-Path $RepoRoot $DocOutputPath
}
$DocOutputPath = [System.IO.Path]::GetFullPath($DocOutputPath)
New-Item -ItemType Directory -Force -Path ([System.IO.Path]::GetDirectoryName($DocOutputPath)) | Out-Null

if (-not $RawCsvOutputPath) {
    $RawCsvOutputPath = Join-Path $RepoRoot "docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-E2E-TX-TPS-RAW-$dateTag.csv"
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

function Parse-ModeLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^mode=ffi_v2 variant=" } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^mode=ffi_v2 variant=(?<variant>\w+) dll=(?<dll>.+?) rc=(?<rc>\d+)\((?<rc_name>[^)]+)\) submitted=(?<submitted>\d+) processed=(?<processed>\d+) success=(?<success>\d+) writes=(?<writes>\d+) elapsed_us=(?<elapsed>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{ parse_ok = $false; raw = $line }
    }
    [ordered]@{
        parse_ok = $true
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

function Parse-RuntimeLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^aoem_runtime_in: " } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^aoem_runtime_in: variant=(?<variant>\w+) dll=(?<dll>.+?) persist_backend=(?<persist>\S+) wasm_runtime=(?<wasm>\S+) zkvm_mode=(?<zkvm>\S+) mldsa_mode=(?<mldsa>\S+) ingress_workers=(?<workers>\d+) plugin_dir=(?<plugin>.+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{ parse_ok = $false; raw = $line }
    }
    [ordered]@{
        parse_ok = $true
        runtime_variant = $m.Groups["variant"].Value
        dll = $m.Groups["dll"].Value
        persist_backend = $m.Groups["persist"].Value
        wasm_runtime = $m.Groups["wasm"].Value
        zkvm_mode = $m.Groups["zkvm"].Value
        mldsa_mode = $m.Groups["mldsa"].Value
        ingress_workers = [int]$m.Groups["workers"].Value
        plugin_dir = $m.Groups["plugin"].Value
        raw = $line
    }
}

function Parse-StageLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^ffi_v2_stage_ms: " } | Select-Object -Last 1)
    if (-not $line) { return $null }
    $m = [regex]::Match(
        $line,
        "^ffi_v2_stage_ms: runtime_open=(?<runtime_open>[0-9.]+) tx_build=(?<tx_build>[0-9.]+) tx_codec=(?<tx_codec>[0-9.]+) mempool=(?<mempool>[0-9.]+) tx_meta=(?<tx_meta>[0-9.]+) batch_map=(?<batch_map>[0-9.]+) adapter=(?<adapter>[0-9.]+) aoem_submit=(?<aoem_submit>[0-9.]+) batch_a=(?<batch_a>[0-9.]+) network=(?<network>[0-9.]+) total=(?<total>[0-9.]+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{ parse_ok = $false; raw = $line }
    }
    [ordered]@{
        parse_ok = $true
        runtime_open_ms = [double]$m.Groups["runtime_open"].Value
        tx_build_ms = [double]$m.Groups["tx_build"].Value
        tx_codec_ms = [double]$m.Groups["tx_codec"].Value
        mempool_ms = [double]$m.Groups["mempool"].Value
        tx_meta_ms = [double]$m.Groups["tx_meta"].Value
        batch_map_ms = [double]$m.Groups["batch_map"].Value
        adapter_ms = [double]$m.Groups["adapter"].Value
        aoem_submit_ms = [double]$m.Groups["aoem_submit"].Value
        batch_a_ms = [double]$m.Groups["batch_a"].Value
        network_ms = [double]$m.Groups["network"].Value
        total_ms = [double]$m.Groups["total"].Value
        raw = $line
    }
}

function Get-NearestRankQuantile {
    param(
        [double[]]$Values,
        [double]$Quantile
    )
    if (-not $Values -or $Values.Count -eq 0) {
        return $null
    }
    $sorted = @($Values | Sort-Object)
    $n = $sorted.Count
    $rank = [Math]::Ceiling($Quantile * $n)
    if ($rank -lt 1) { $rank = 1 }
    if ($rank -gt $n) { $rank = $n }
    return [Math]::Round([double]$sorted[$rank - 1], 2)
}

function Format-F64 {
    param([double]$Value)
    return [Math]::Round($Value, 2)
}

function Resolve-Profiles {
    param([string]$Raw)
    $map = @{
        "core_only" = @{ name = "core_only"; persist_backend = "none"; wasm_runtime = "none" }
        "core_persist" = @{ name = "core_persist"; persist_backend = "rocksdb"; wasm_runtime = "none" }
        "core_wasm" = @{ name = "core_wasm"; persist_backend = "none"; wasm_runtime = "wasmtime" }
        "core_persist_wasm" = @{ name = "core_persist_wasm"; persist_backend = "rocksdb"; wasm_runtime = "wasmtime" }
    }
    $resolved = New-Object System.Collections.Generic.List[object]
    $tokens = @($Raw -split "," | ForEach-Object { $_.Trim().ToLowerInvariant() } | Where-Object { $_ })
    if ($tokens.Count -eq 0) {
        throw "Profiles cannot be empty"
    }
    $seen = @{}
    foreach ($token in $tokens) {
        if (-not $map.ContainsKey($token)) {
            throw "unsupported profile: $token (valid: core_only, core_persist, core_wasm, core_persist_wasm)"
        }
        if ($seen.ContainsKey($token)) { continue }
        $seen[$token] = $true
        $resolved.Add([pscustomobject]$map[$token]) | Out-Null
    }
    return @($resolved.ToArray())
}

function Invoke-TxE2ERun {
    param(
        [string]$ExePath,
        [string]$TxWirePath,
        [string]$ProfileName,
        [string]$PersistBackend,
        [string]$WasmRuntime,
        [int]$RunIndex,
        [int]$Txs,
        [int]$Accounts,
        [int]$BatchCount,
        [int]$IngressWorkers,
        [int]$TimeoutSec,
        [string]$AoemPluginDir,
        [string]$OutputDir,
        [string]$AdapterSignalMode
    )

    $stdoutPath = Join-Path $OutputDir ("run-{0}-{1}.stdout.log" -f $RunIndex, $ProfileName)
    $stderrPath = Join-Path $OutputDir ("run-{0}-{1}.stderr.log" -f $RunIndex, $ProfileName)
    if (Test-Path $stdoutPath) { Remove-Item $stdoutPath -Force }
    if (Test-Path $stderrPath) { Remove-Item $stderrPath -Force }

    $envMap = @{
        NOVOVM_EXEC_PATH = "ffi_v2"
        NOVOVM_TX_WIRE_FILE = $TxWirePath
        NOVOVM_BATCH_A_BATCHES = "$BatchCount"
        NOVOVM_BATCH_A_STRICT = "0"
        NOVOVM_NETWORK_STRICT = "0"
        NOVOVM_ENABLE_BATCH_A = "0"
        NOVOVM_ENABLE_NETWORK_SMOKE = "0"
        NOVOVM_TX_META_VERIFY_SIG = "0"
        NOVOVM_AOEM_VARIANT = "core"
        NOVOVM_AOEM_PERSIST_BACKEND = $PersistBackend
        NOVOVM_AOEM_WASM_RUNTIME = $WasmRuntime
        NOVOVM_AOEM_PLUGIN_DIR = $AoemPluginDir
        NOVOVM_INGRESS_WORKERS = "$IngressWorkers"
    }
    if ($AdapterSignalMode -eq "fast") {
        $envMap["NOVOVM_ADAPTER_SIGNAL_FAST"] = "1"
    } else {
        $envMap["NOVOVM_ADAPTER_SIGNAL_FAST"] = "0"
    }

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $proc = Start-Process -FilePath $ExePath -WorkingDirectory (Split-Path $ExePath -Parent) -NoNewWindow -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath -Environment $envMap
    $timedOut = $false
    try {
        Wait-Process -Id $proc.Id -Timeout $TimeoutSec -ErrorAction Stop
    } catch {
        $timedOut = $true
    }
    $sw.Stop()

    if ($timedOut) {
        try { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue } catch {}
        return [ordered]@{
            run = $RunIndex
            profile = $ProfileName
            timeout = $true
            pass = $false
            wall_ms = [Math]::Round($sw.Elapsed.TotalMilliseconds, 2)
            timeout_sec = $TimeoutSec
            stdout = $stdoutPath
            stderr = $stderrPath
            reason = "timeout"
        }
    }

    $stdoutText = if (Test-Path $stdoutPath) { Get-Content $stdoutPath -Raw } else { "" }
    $mode = Parse-ModeLine -Text $stdoutText
    $runtimeLine = Parse-RuntimeLine -Text $stdoutText
    $stage = Parse-StageLine -Text $stdoutText
    $commitLine = ($stdoutText -split "`r?`n" | Where-Object { $_ -match "^commit_out:" } | Select-Object -Last 1)
    $netLine = ($stdoutText -split "`r?`n" | Where-Object { $_ -match "^network_pacemaker:" } | Select-Object -Last 1)

    $modeOk = ($null -ne $mode -and $mode.parse_ok -and $mode.rc -eq 0)
    $runtimeOk = ($null -ne $runtimeLine -and $runtimeLine.parse_ok)
    $requireBatchA = ($envMap["NOVOVM_ENABLE_BATCH_A"] -eq "1")
    $requireNetwork = ($envMap["NOVOVM_ENABLE_NETWORK_SMOKE"] -eq "1")
    $commitOk = ((-not $requireBatchA) -or ($null -ne $commitLine -and $commitLine.Trim().Length -gt 0))
    $networkOk = ((-not $requireNetwork) -or ($null -ne $netLine -and $netLine -match "view_sync=true" -and $netLine -match "new_view=true"))
    $pass = ($proc.ExitCode -eq 0 -and $modeOk -and $runtimeOk -and $commitOk -and $networkOk)
    $reason = if ($pass) {
        "ok"
    } elseif (-not $modeOk) {
        "mode_failed"
    } elseif (-not $runtimeOk) {
        "runtime_line_missing"
    } elseif (-not $commitOk) {
        "batch_a_missing"
    } elseif (-not $networkOk) {
        "network_missing"
    } else {
        "node_path_failed"
    }

    $processed = if ($modeOk) { [double]$mode.processed } else { 0.0 }
    $wallSec = $sw.Elapsed.TotalSeconds
    $wallTps = if ($wallSec -gt 0) { $processed / $wallSec } else { 0.0 }
    $engineSec = if ($modeOk) { ([double]$mode.elapsed_us) / 1000000.0 } else { 0.0 }
    $engineTps = if ($engineSec -gt 0) { $processed / $engineSec } else { $null }
    $stageTotal = if ($null -ne $stage -and $stage.parse_ok) { [double]$stage.total_ms } else { $null }
    $engineMs = if ($modeOk) { ([double]$mode.elapsed_us) / 1000.0 } else { $null }
    $nonEngineMs = if ($null -ne $stageTotal -and $null -ne $engineMs) { [Math]::Max(0.0, $stageTotal - $engineMs) } else { $null }
    $nonEnginePct = if ($null -ne $stageTotal -and $stageTotal -gt 0 -and $null -ne $nonEngineMs) { ($nonEngineMs / $stageTotal) * 100.0 } else { $null }
    $bootstrapMs = if ($null -ne $stageTotal) { [Math]::Max(0.0, $sw.Elapsed.TotalMilliseconds - $stageTotal) } else { $null }

    return [ordered]@{
        run = $RunIndex
        profile = $ProfileName
        timeout = $false
        pass = $pass
        exit_code = $proc.ExitCode
        wall_ms = [Math]::Round($sw.Elapsed.TotalMilliseconds, 2)
        wall_tps = Format-F64 $wallTps
        engine_tps = if ($null -ne $engineTps) { Format-F64 $engineTps } else { $null }
        submitted = if ($modeOk) { [int64]$mode.submitted } else { $null }
        processed = if ($modeOk) { [int64]$mode.processed } else { $null }
        success = if ($modeOk) { [int64]$mode.success } else { $null }
        elapsed_us = if ($modeOk) { [int64]$mode.elapsed_us } else { $null }
        mode_line = if ($modeOk) { [string]$mode.raw } else { $null }
        runtime_variant = if ($runtimeOk) { [string]$runtimeLine.runtime_variant } else { $null }
        runtime_persist_backend = if ($runtimeOk) { [string]$runtimeLine.persist_backend } else { $null }
        runtime_wasm_runtime = if ($runtimeOk) { [string]$runtimeLine.wasm_runtime } else { $null }
        runtime_ingress_workers = if ($runtimeOk) { [int]$runtimeLine.ingress_workers } else { $null }
        runtime_plugin_dir = if ($runtimeOk) { [string]$runtimeLine.plugin_dir } else { $null }
        stage_line = if ($null -ne $stage -and $stage.parse_ok) { [string]$stage.raw } else { $null }
        stage_runtime_open_ms = if ($null -ne $stage -and $stage.parse_ok) { Format-F64 $stage.runtime_open_ms } else { $null }
        stage_tx_build_ms = if ($null -ne $stage -and $stage.parse_ok) { Format-F64 $stage.tx_build_ms } else { $null }
        stage_tx_codec_ms = if ($null -ne $stage -and $stage.parse_ok) { Format-F64 $stage.tx_codec_ms } else { $null }
        stage_mempool_ms = if ($null -ne $stage -and $stage.parse_ok) { Format-F64 $stage.mempool_ms } else { $null }
        stage_tx_meta_ms = if ($null -ne $stage -and $stage.parse_ok) { Format-F64 $stage.tx_meta_ms } else { $null }
        stage_batch_map_ms = if ($null -ne $stage -and $stage.parse_ok) { Format-F64 $stage.batch_map_ms } else { $null }
        stage_adapter_ms = if ($null -ne $stage -and $stage.parse_ok) { Format-F64 $stage.adapter_ms } else { $null }
        stage_aoem_submit_ms = if ($null -ne $stage -and $stage.parse_ok) { Format-F64 $stage.aoem_submit_ms } else { $null }
        stage_batch_a_ms = if ($null -ne $stage -and $stage.parse_ok) { Format-F64 $stage.batch_a_ms } else { $null }
        stage_network_ms = if ($null -ne $stage -and $stage.parse_ok) { Format-F64 $stage.network_ms } else { $null }
        stage_total_ms = if ($null -ne $stageTotal) { Format-F64 $stageTotal } else { $null }
        non_engine_ms = if ($null -ne $nonEngineMs) { Format-F64 $nonEngineMs } else { $null }
        non_engine_pct = if ($null -ne $nonEnginePct) { Format-F64 $nonEnginePct } else { $null }
        bootstrap_ms = if ($null -ne $bootstrapMs) { Format-F64 $bootstrapMs } else { $null }
        commit_line = $commitLine
        network_line = $netLine
        stdout = $stdoutPath
        stderr = $stderrPath
        reason = $reason
    }
}

$nodeDir = Join-Path $RepoRoot "crates\novovm-node"
$benchDir = Join-Path $RepoRoot "crates\novovm-bench"
if ($BuildProfile -eq "release") {
    Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("build", "--quiet", "--release", "--bin", "novovm-node")
} else {
    Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("build", "--quiet", "--bin", "novovm-node")
}

$txGenArgs = @("run", "--quiet", "--bin", "novovm-txgen", "--", "--out", (Join-Path $OutputDir "ingress.txwire.bin"), "--txs", "$Txs", "--accounts", "$Accounts")
Invoke-Cargo -WorkDir $benchDir -CargoArgs $txGenArgs
$txWirePath = Join-Path $OutputDir "ingress.txwire.bin"
if (-not (Test-Path $txWirePath)) {
    throw "tx wire ingress file not generated: $txWirePath"
}
$txWirePath = [System.IO.Path]::GetFullPath($txWirePath)

$exePath = if ($BuildProfile -eq "release") {
    Join-Path $nodeDir "cargo-target\release\novovm-node.exe"
} else {
    Join-Path $nodeDir "cargo-target\debug\novovm-node.exe"
}
if (-not (Test-Path $exePath)) {
    $fallback = Join-Path $RepoRoot ("target\{0}\novovm-node.exe" -f $BuildProfile)
    if (-not (Test-Path $fallback)) {
        throw "novovm-node executable not found: $exePath"
    }
    $exePath = $fallback
}

$profileDefs = Resolve-Profiles -Raw $Profiles
$rawRows = New-Object System.Collections.Generic.List[object]
foreach ($profile in $profileDefs) {
    for ($run = 1; $run -le $Repeats; $run++) {
        $row = Invoke-TxE2ERun -ExePath $exePath -TxWirePath $txWirePath -ProfileName $profile.name -PersistBackend $profile.persist_backend -WasmRuntime $profile.wasm_runtime -RunIndex $run -Txs $Txs -Accounts $Accounts -BatchCount $BatchCount -IngressWorkers $IngressWorkers -TimeoutSec $TimeoutSec -AoemPluginDir $AoemPluginDir -OutputDir $OutputDir -AdapterSignalMode $AdapterSignalMode
        $rawRows.Add([pscustomobject]$row) | Out-Null
    }
}

$raw = @($rawRows.ToArray())
if ($raw.Count -eq 0) {
    throw "no tx e2e samples generated"
}
$raw | Sort-Object profile, run | Export-Csv -Path $RawCsvOutputPath -NoTypeInformation -Encoding UTF8

$matrixRows = New-Object System.Collections.Generic.List[object]
foreach ($profile in $profileDefs) {
    $profileName = $profile.name
    $group = @($raw | Where-Object { $_.profile -eq $profileName })
    if ($group.Count -eq 0) { continue }

    $passRows = @($group | Where-Object { $_.pass -and -not $_.timeout })
    $wallSamples = [double[]]@($passRows | ForEach-Object { [double]$_.wall_tps })
    $engineSamples = [double[]]@($passRows | Where-Object { $null -ne $_.engine_tps } | ForEach-Object { [double]$_.engine_tps })
    $mempoolMsSamples = [double[]]@($passRows | Where-Object { $null -ne $_.stage_mempool_ms } | ForEach-Object { [double]$_.stage_mempool_ms })
    $adapterMsSamples = [double[]]@($passRows | Where-Object { $null -ne $_.stage_adapter_ms } | ForEach-Object { [double]$_.stage_adapter_ms })
    $aoemSubmitMsSamples = [double[]]@($passRows | Where-Object { $null -ne $_.stage_aoem_submit_ms } | ForEach-Object { [double]$_.stage_aoem_submit_ms })
    $batchAMsSamples = [double[]]@($passRows | Where-Object { $null -ne $_.stage_batch_a_ms } | ForEach-Object { [double]$_.stage_batch_a_ms })
    $networkMsSamples = [double[]]@($passRows | Where-Object { $null -ne $_.stage_network_ms } | ForEach-Object { [double]$_.stage_network_ms })
    $stageTotalMsSamples = [double[]]@($passRows | Where-Object { $null -ne $_.stage_total_ms } | ForEach-Object { [double]$_.stage_total_ms })
    $nonEnginePctSamples = [double[]]@($passRows | Where-Object { $null -ne $_.non_engine_pct } | ForEach-Object { [double]$_.non_engine_pct })
    $bootstrapMsSamples = [double[]]@($passRows | Where-Object { $null -ne $_.bootstrap_ms } | ForEach-Object { [double]$_.bootstrap_ms })

    $runtimeSample = $passRows | Select-Object -First 1

    $matrixRows.Add([pscustomobject]@{
        profile = $profileName
        persist_backend = $profile.persist_backend
        wasm_runtime = $profile.wasm_runtime
        runtime_variant = if ($runtimeSample) { $runtimeSample.runtime_variant } else { $null }
        runtime_ingress_workers = if ($runtimeSample) { $runtimeSample.runtime_ingress_workers } else { $null }
        runs = $group.Count
        pass_runs = $passRows.Count
        timeout_runs = (@($group | Where-Object { $_.timeout })).Count
        all_pass = ($passRows.Count -eq $group.Count)
        wall_tps_p50 = Get-NearestRankQuantile -Values $wallSamples -Quantile 0.50
        wall_tps_p90 = Get-NearestRankQuantile -Values $wallSamples -Quantile 0.90
        wall_tps_p99 = Get-NearestRankQuantile -Values $wallSamples -Quantile 0.99
        engine_tps_p50 = Get-NearestRankQuantile -Values $engineSamples -Quantile 0.50
        engine_tps_p90 = Get-NearestRankQuantile -Values $engineSamples -Quantile 0.90
        engine_tps_p99 = Get-NearestRankQuantile -Values $engineSamples -Quantile 0.99
        mempool_ms_p50 = Get-NearestRankQuantile -Values $mempoolMsSamples -Quantile 0.50
        adapter_ms_p50 = Get-NearestRankQuantile -Values $adapterMsSamples -Quantile 0.50
        aoem_submit_ms_p50 = Get-NearestRankQuantile -Values $aoemSubmitMsSamples -Quantile 0.50
        batch_a_ms_p50 = Get-NearestRankQuantile -Values $batchAMsSamples -Quantile 0.50
        network_ms_p50 = Get-NearestRankQuantile -Values $networkMsSamples -Quantile 0.50
        stage_total_ms_p50 = Get-NearestRankQuantile -Values $stageTotalMsSamples -Quantile 0.50
        non_engine_pct_p50 = Get-NearestRankQuantile -Values $nonEnginePctSamples -Quantile 0.50
        bootstrap_ms_p50 = Get-NearestRankQuantile -Values $bootstrapMsSamples -Quantile 0.50
    }) | Out-Null
}

$overallPass = ((@($raw | Where-Object { $_.pass }) | Measure-Object).Count -eq $raw.Count)

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    repo_root = $RepoRoot
    output_dir = $OutputDir
    doc_md = $DocOutputPath
    raw_csv = $RawCsvOutputPath
    mode = "host_pipeline_wall_tps_diagnostic_core_sidecar_profiles"
    params = [ordered]@{
        repeats = $Repeats
        txs = $Txs
        accounts = $Accounts
        timeout_sec = $TimeoutSec
        batch_count = $BatchCount
        ingress_workers = $IngressWorkers
        adapter_signal_mode = $AdapterSignalMode
        profiles = @($profileDefs | ForEach-Object { $_.name })
        aoem_plugin_dir = $AoemPluginDir
        build_profile = $BuildProfile
        tx_wire_file = $txWirePath
    }
    overall_pass = $overallPass
    matrix = @($matrixRows.ToArray())
    samples = $raw
}

$summaryJsonPath = Join-Path $OutputDir "tx-e2e-tps-summary.json"
$summaryMdPath = Join-Path $OutputDir "tx-e2e-tps-summary.md"
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $summaryJsonPath -Encoding UTF8

$doc = New-Object System.Collections.Generic.List[string]
$doc.Add("# AOEM Core + Sidecar Host-Pipeline TPS Diagnostic ($dateTag)")
$doc.Add("")
$doc.Add("> Diagnostic-only report. Do not use as AOEM kernel TPS KPI.")
$doc.Add("")
$doc.Add("## Scope")
$doc.Add("")
$doc.Add('- Host pipeline via `novovm-node` production ingress (`tx_wire_file -> ops_encode -> aoem_submit`).')
$doc.Add("- AOEM is always loaded from core dll; persist/wasm are sidecar runtime profiles.")
$doc.Add('- TPS metric is host wall-clock: `processed_tx / wall_time`.')
$doc.Add('- This is NOT pure kernel TPS and NOT network/consensus E2E TPS.')
$doc.Add("- Quantiles: P50 / P90 / P99 (nearest-rank).")
$doc.Add("")
$doc.Add("## Fixed Params")
$doc.Add("")
$doc.Add(("- txs per run: {0}" -f $Txs))
$doc.Add(("- accounts: {0}" -f $Accounts))
$doc.Add(("- repeats: {0}" -f $Repeats))
$doc.Add(("- timeout_sec: {0}" -f $TimeoutSec))
$doc.Add(("- batch_count: {0}" -f $BatchCount))
$doc.Add(("- ingress_workers: {0}" -f $IngressWorkers))
$doc.Add(("- adapter_signal_mode: {0}" -f $AdapterSignalMode))
$doc.Add(("- profiles: {0}" -f (($profileDefs | ForEach-Object { $_.name }) -join ",")))
$doc.Add(("- build_profile: {0}" -f $BuildProfile))
$doc.Add(("- aoem_plugin_dir: {0}" -f $AoemPluginDir))
$doc.Add("")
$doc.Add("## Matrix")
$doc.Add("")
$doc.Add("| profile | persist_backend | wasm_runtime | runtime_variant | ingress_workers | runs | pass_runs | wall_tps_p50 | wall_tps_p90 | wall_tps_p99 |")
$doc.Add("|---|---|---|---|---:|---:|---:|---:|---:|---:|")
foreach ($r in $matrixRows) {
    $doc.Add("| $($r.profile) | $($r.persist_backend) | $($r.wasm_runtime) | $($r.runtime_variant) | $($r.runtime_ingress_workers) | $($r.runs) | $($r.pass_runs) | $($r.wall_tps_p50) | $($r.wall_tps_p90) | $($r.wall_tps_p99) |")
}
$doc.Add("")
$doc.Add("## Stage P50 (ms)")
$doc.Add("")
$doc.Add("| profile | mempool_ms_p50 | adapter_ms_p50 | aoem_submit_ms_p50 | batch_a_ms_p50 | network_ms_p50 | stage_total_ms_p50 |")
$doc.Add("|---|---:|---:|---:|---:|---:|---:|")
foreach ($r in $matrixRows) {
    $doc.Add("| $($r.profile) | $($r.mempool_ms_p50) | $($r.adapter_ms_p50) | $($r.aoem_submit_ms_p50) | $($r.batch_a_ms_p50) | $($r.network_ms_p50) | $($r.stage_total_ms_p50) |")
}
$doc.Add("")
$doc.Add("## Overhead P50")
$doc.Add("")
$doc.Add("| profile | non_engine_pct_p50 | bootstrap_ms_p50 |")
$doc.Add("|---|---:|---:|")
foreach ($r in $matrixRows) {
    $doc.Add("| $($r.profile) | $($r.non_engine_pct_p50) | $($r.bootstrap_ms_p50) |")
}
$doc.Add("")
$doc.Add("## Reproduce")
$doc.Add("")
$doc.Add('```powershell')
$doc.Add("& scripts/migration/run_tx_e2e_tps_core_sidecar_report.ps1 -RepoRoot $RepoRoot -Repeats $Repeats -Txs $Txs -Accounts $Accounts -TimeoutSec $TimeoutSec -BatchCount $BatchCount -IngressWorkers $IngressWorkers -AdapterSignalMode $AdapterSignalMode -Profiles '$Profiles' -AoemPluginDir $AoemPluginDir -BuildProfile $BuildProfile")
$doc.Add('```')
$doc.Add("")
$doc.Add("## Artifacts")
$doc.Add("")
$doc.Add("- $summaryJsonPath")
$doc.Add("- $RawCsvOutputPath")

$docText = $doc -join "`n"
$docText | Set-Content -Path $summaryMdPath -Encoding UTF8
$docText | Set-Content -Path $DocOutputPath -Encoding UTF8

Write-Host "tx e2e tps report generated:"
Write-Host "  summary_json: $summaryJsonPath"
Write-Host "  summary_md:   $summaryMdPath"
Write-Host "  raw_csv:      $RawCsvOutputPath"
Write-Host "  doc_md:       $DocOutputPath"
Write-Host "  overall_pass: $overallPass"
