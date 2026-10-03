param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [string]$DocOutputPath = "",
    [string]$RawCsvOutputPath = "",
    [ValidateRange(1, 20)]
    [int]$Repeats = 3,
    [int64]$Txs = 1000000,
    [int]$KeySpace = 128,
    [double]$Rw = 0.5,
    [int]$Seed = 123,
    [int]$WarmupCalls = 5,
    [ValidateSet("debug", "release")]
    [string]$BuildProfile = "release",
    [string]$AoemPluginDir = "",
    [bool]$PreferComposedAoemRuntime = $true,
    [bool]$IncludeNetworkConsensusMatrix = $true,
    [ValidateSet("mesh", "pair_matrix")]
    [string]$NetworkProbeMode = "pair_matrix",
    [ValidateRange(1, 50)]
    [int]$NetworkRounds = 2,
    [ValidateRange(2, 12)]
    [int]$NetworkNodeCount = 2,
    [ValidateRange(5, 120)]
    [int]$NetworkTimeoutSeconds = 20
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

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

$dateTag = Get-Date -Format "yyyy-MM-dd"
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\aoem-tps-core-sidecar-$dateTag"
}
if (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

if (-not $AoemPluginDir) {
    $platform = if ($IsWindows) { "windows" } elseif ($IsMacOS) { "macos" } else { "linux" }
    $AoemPluginDir = Join-Path $RepoRoot "aoem\$platform\core\plugins"
}
if (-not (Test-Path $AoemPluginDir)) {
    throw "aoem plugin dir not found: $AoemPluginDir"
}
$AoemPluginDir = (Resolve-Path $AoemPluginDir).Path

if (-not $DocOutputPath) {
    $DocOutputPath = Join-Path $RepoRoot "docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-TPS-SEAL-$dateTag.md"
}
if (-not [System.IO.Path]::IsPathRooted($DocOutputPath)) {
    $DocOutputPath = Join-Path $RepoRoot $DocOutputPath
}
$DocOutputPath = [System.IO.Path]::GetFullPath($DocOutputPath)
New-Item -ItemType Directory -Force -Path ([System.IO.Path]::GetDirectoryName($DocOutputPath)) | Out-Null

if (-not $RawCsvOutputPath) {
    $RawCsvOutputPath = Join-Path $RepoRoot "docs_CN\AOEM-FFI\AOEM-FFI-CORE-SIDECAR-TPS-RAW-$dateTag.csv"
}
if (-not [System.IO.Path]::IsPathRooted($RawCsvOutputPath)) {
    $RawCsvOutputPath = Join-Path $RepoRoot $RawCsvOutputPath
}
$RawCsvOutputPath = [System.IO.Path]::GetFullPath($RawCsvOutputPath)
New-Item -ItemType Directory -Force -Path ([System.IO.Path]::GetDirectoryName($RawCsvOutputPath)) | Out-Null

$compareScript = Join-Path $RepoRoot "scripts\migration\run_performance_compare.ps1"
if (-not (Test-Path $compareScript)) {
    throw "missing compare script: $compareScript"
}
$networkScript = Join-Path $RepoRoot "scripts\migration\run_network_two_process.ps1"
if ($IncludeNetworkConsensusMatrix -and -not (Test-Path $networkScript)) {
    throw "missing network script: $networkScript"
}

function Get-LineName {
    param([string]$Preset, [string]$LineProfile)
    switch ("$Preset|$LineProfile") {
        "cpu_parity|seal_single" { return "cpu_parity_single" }
        "cpu_parity|seal_auto" { return "cpu_parity_auto_parallel" }
        "cpu_batch_stress|seal_single" { return "cpu_batch_stress_single" }
        "cpu_batch_stress|seal_auto" { return "cpu_batch_stress_auto_parallel" }
        default { throw "unknown line mapping: preset=$Preset line_profile=$LineProfile" }
    }
}

function Get-NearestRankQuantile {
    param(
        [double[]]$Values,
        [double]$Quantile
    )
    if (-not $Values -or $Values.Count -eq 0) {
        throw "quantile input cannot be empty"
    }
    $sorted = @($Values | Sort-Object)
    $n = $sorted.Count
    $rank = [Math]::Ceiling($Quantile * $n)
    if ($rank -lt 1) { $rank = 1 }
    if ($rank -gt $n) { $rank = $n }
    return [double]$sorted[$rank - 1]
}

function Format-F64 {
    param([double]$Value)
    return [Math]::Round($Value, 2)
}

function Get-NetworkConsensusSignalForVariant {
    param(
        [string]$Variant,
        [string]$ScriptPath,
        [string]$RepoRoot,
        [string]$OutputDir,
        [string]$AoemPluginDir,
        [string]$ProbeMode,
        [int]$Rounds,
        [int]$NodeCount,
        [int]$TimeoutSeconds
    )

    $netDir = Join-Path $OutputDir ("network-consensus-{0}" -f $Variant)
    $netParams = @{
        RepoRoot = $RepoRoot
        OutputDir = $netDir
        ProbeMode = $ProbeMode
        Rounds = $Rounds
        NodeCount = $NodeCount
        TimeoutSeconds = $TimeoutSeconds
        AoemVariant = $Variant
        AoemPluginDir = $AoemPluginDir
    }
    & $ScriptPath @netParams | Out-Null

    $jsonPath = Join-Path $netDir "network-two-process.json"
    if (-not (Test-Path $jsonPath)) {
        throw "missing network consensus json for variant=${Variant}: $jsonPath"
    }

    $raw = Get-Content -Path $jsonPath -Raw | ConvertFrom-Json
    $blockWirePass = [bool]$raw.block_wire_pass
    $viewSyncPass = [bool]$raw.view_sync_pass
    $newViewPass = [bool]$raw.new_view_pass
    [pscustomobject]@{
        variant = $Variant
        available = $true
        pass = [bool]$raw.pass
        mode = [string]$raw.mode
        rounds = [int]$raw.rounds
        round_pass_ratio = [double]$raw.round_pass_ratio
        pair_pass_ratio = [double]$raw.pair_pass_ratio
        block_wire_pass = $blockWirePass
        view_sync_pass = $viewSyncPass
        new_view_pass = $newViewPass
        consensus_binding_pass = $blockWirePass
        pacemaker_pass = ($viewSyncPass -and $newViewPass)
        recv_tps_available = if ($null -ne $raw.recv_tps_available) { [bool]$raw.recv_tps_available } else { $false }
        recv_tps_p50 = if ($null -ne $raw.recv_tps_p50) { [double]$raw.recv_tps_p50 } else { $null }
        recv_tps_p90 = if ($null -ne $raw.recv_tps_p90) { [double]$raw.recv_tps_p90 } else { $null }
        recv_tps_p99 = if ($null -ne $raw.recv_tps_p99) { [double]$raw.recv_tps_p99 } else { $null }
        send_tps_available = if ($null -ne $raw.send_tps_available) { [bool]$raw.send_tps_available } else { $false }
        send_tps_p50 = if ($null -ne $raw.send_tps_p50) { [double]$raw.send_tps_p50 } else { $null }
        send_tps_p90 = if ($null -ne $raw.send_tps_p90) { [double]$raw.send_tps_p90 } else { $null }
        send_tps_p99 = if ($null -ne $raw.send_tps_p99) { [double]$raw.send_tps_p99 } else { $null }
        e2e_tps_available = if ($null -ne $raw.recv_tps_available) { [bool]$raw.recv_tps_available } else { $false }
        e2e_tps_p50 = if ($null -ne $raw.recv_tps_p50) { [double]$raw.recv_tps_p50 } else { $null }
        e2e_tps_p90 = if ($null -ne $raw.recv_tps_p90) { [double]$raw.recv_tps_p90 } else { $null }
        e2e_tps_p99 = if ($null -ne $raw.recv_tps_p99) { [double]$raw.recv_tps_p99 } else { $null }
        source_json = $jsonPath
    }
}

$variants = @("core", "persist", "wasm")
$lineProfiles = @("seal_single", "seal_auto")
$lineOrder = @(
    "cpu_parity_single",
    "cpu_parity_auto_parallel",
    "cpu_batch_stress_single",
    "cpu_batch_stress_auto_parallel"
)
$presetByLineName = @{
    "cpu_parity_single" = "cpu_parity"
    "cpu_parity_auto_parallel" = "cpu_parity"
    "cpu_batch_stress_single" = "cpu_batch_stress"
    "cpu_batch_stress_auto_parallel" = "cpu_batch_stress"
}
$submitOpsByLineName = @{
    "cpu_parity_single" = 1
    "cpu_parity_auto_parallel" = 1
    "cpu_batch_stress_single" = 1024
    "cpu_batch_stress_auto_parallel" = 1024
}
$threadsArgByLineName = @{
    "cpu_parity_single" = "1"
    "cpu_parity_auto_parallel" = "auto"
    "cpu_batch_stress_single" = "1"
    "cpu_batch_stress_auto_parallel" = "auto"
}
$engineWorkersArgByLineName = @{
    "cpu_parity_single" = "16"
    "cpu_parity_auto_parallel" = "auto"
    "cpu_batch_stress_single" = "16"
    "cpu_batch_stress_auto_parallel" = "auto"
}

$networkConsensusByVariant = @{}
if ($IncludeNetworkConsensusMatrix) {
    foreach ($variant in $variants) {
        Write-Host ("network+consensus probe: variant={0} mode={1} rounds={2}" -f $variant, $NetworkProbeMode, $NetworkRounds)
        $networkConsensusByVariant[$variant] = Get-NetworkConsensusSignalForVariant `
            -Variant $variant `
            -ScriptPath $networkScript `
            -RepoRoot $RepoRoot `
            -OutputDir $OutputDir `
            -AoemPluginDir $AoemPluginDir `
            -ProbeMode $NetworkProbeMode `
            -Rounds $NetworkRounds `
            -NodeCount $NetworkNodeCount `
            -TimeoutSeconds $NetworkTimeoutSeconds
    }
}

$rawRows = New-Object System.Collections.Generic.List[object]

for ($run = 1; $run -le $Repeats; $run++) {
    foreach ($variant in $variants) {
        foreach ($lineProfile in $lineProfiles) {
            $runDir = Join-Path $OutputDir ("run-{0}-{1}-{2}" -f $run, $variant, $lineProfile)
            $params = @{
                RepoRoot = $RepoRoot
                OutputDir = $runDir
                Variants = $variant
                LineProfile = $lineProfile
                BuildProfile = $BuildProfile
                Txs = $Txs
                KeySpace = $KeySpace
                Rw = $Rw
                Seed = $Seed
                WarmupCalls = $WarmupCalls
                IncludeCapabilitySnapshot = $false
                AoemPluginDir = $AoemPluginDir
                PreferComposedAoemRuntime = $PreferComposedAoemRuntime
            }
            & $compareScript @params | Out-Null

            $jsonPath = Join-Path $runDir "performance-compare.json"
            if (-not (Test-Path $jsonPath)) {
                throw "missing compare report: $jsonPath"
            }
            $report = Get-Content -Path $jsonPath -Raw | ConvertFrom-Json
            foreach ($item in $report.items) {
                $lineName = Get-LineName -Preset ([string]$item.preset) -LineProfile $lineProfile
                $rawRows.Add([pscustomobject]@{
                    run = $run
                    variant = [string]$item.variant
                    line_profile = $lineProfile
                    line_name = $lineName
                    preset = [string]$item.preset
                    submit_ops = [int]$submitOpsByLineName[$lineName]
                    threads_arg = [string]$threadsArgByLineName[$lineName]
                    engine_workers_arg = [string]$engineWorkersArgByLineName[$lineName]
                    runtime_mode = [string]$item.runtime_mode
                    dll = [string]$item.dll
                    tps_ops_per_s = [double]$item.tps
                    plans_per_s = [double]$item.plans_per_s
                    ffi_v2_calls_per_s = [double]$item.ffi_v2_calls_per_s
                    avg_ops_per_plan = [double]$item.avg_ops_per_plan
                    avg_ops_per_call = [double]$item.avg_ops_per_call
                    elapsed_sec = [double]$item.elapsed_sec
                    done_ops = [int64]$item.done_ops
                }) | Out-Null
            }
        }
    }
}

$rawArray = @($rawRows.ToArray())
if ($rawArray.Count -eq 0) {
    throw "no raw benchmark samples generated"
}

$rawArray |
    Sort-Object variant, line_name, run |
    Export-Csv -Path $RawCsvOutputPath -NoTypeInformation -Encoding UTF8

$matrixRows = New-Object System.Collections.Generic.List[object]
foreach ($variant in $variants) {
    foreach ($lineName in $lineOrder) {
        $group = @($rawArray | Where-Object { $_.variant -eq $variant -and $_.line_name -eq $lineName })
        if ($group.Count -eq 0) {
            continue
        }

        $tpsSamples = [double[]]@($group | ForEach-Object { [double]$_.tps_ops_per_s })
        $plansSamples = [double[]]@($group | ForEach-Object { [double]$_.plans_per_s })
        $callsSamples = [double[]]@($group | ForEach-Object { [double]$_.ffi_v2_calls_per_s })
        $avgOpsPerPlanSamples = [double[]]@($group | ForEach-Object { [double]$_.avg_ops_per_plan })

        $first = $group[0]
        $netSignal = $null
        if ($networkConsensusByVariant.ContainsKey($variant)) {
            $netSignal = $networkConsensusByVariant[$variant]
        }
        $matrixRows.Add([pscustomobject]@{
            variant = $variant
            line_name = $lineName
            preset = $presetByLineName[$lineName]
            submit_ops = [int]$submitOpsByLineName[$lineName]
            threads_arg = [string]$threadsArgByLineName[$lineName]
            engine_workers_arg = [string]$engineWorkersArgByLineName[$lineName]
            runtime_mode = [string]$first.runtime_mode
            dll = [string]$first.dll
            p50_ops_per_s = Format-F64 (Get-NearestRankQuantile -Values $tpsSamples -Quantile 0.50)
            p90_ops_per_s = Format-F64 (Get-NearestRankQuantile -Values $tpsSamples -Quantile 0.90)
            p99_ops_per_s = Format-F64 (Get-NearestRankQuantile -Values $tpsSamples -Quantile 0.99)
            p50_plans_per_s = Format-F64 (Get-NearestRankQuantile -Values $plansSamples -Quantile 0.50)
            p50_calls_per_s = Format-F64 (Get-NearestRankQuantile -Values $callsSamples -Quantile 0.50)
            p50_avg_ops_per_plan = Format-F64 (Get-NearestRankQuantile -Values $avgOpsPerPlanSamples -Quantile 0.50)
            tps_samples = @($tpsSamples | ForEach-Object { Format-F64 $_ })
            network_consensus_available = ($null -ne $netSignal -and [bool]$netSignal.available)
            network_consensus_pass = if ($null -ne $netSignal) { [bool]$netSignal.pass } else { $null }
            network_consensus_mode = if ($null -ne $netSignal) { [string]$netSignal.mode } else { $null }
            network_consensus_rounds = if ($null -ne $netSignal) { [int]$netSignal.rounds } else { $null }
            network_consensus_round_pass_ratio = if ($null -ne $netSignal) { Format-F64 ([double]$netSignal.round_pass_ratio) } else { $null }
            consensus_binding_pass = if ($null -ne $netSignal) { [bool]$netSignal.consensus_binding_pass } else { $null }
            pacemaker_pass = if ($null -ne $netSignal) { [bool]$netSignal.pacemaker_pass } else { $null }
            block_wire_pass = if ($null -ne $netSignal) { [bool]$netSignal.block_wire_pass } else { $null }
            view_sync_pass = if ($null -ne $netSignal) { [bool]$netSignal.view_sync_pass } else { $null }
            new_view_pass = if ($null -ne $netSignal) { [bool]$netSignal.new_view_pass } else { $null }
            network_e2e_tps_available = if ($null -ne $netSignal) { [bool]$netSignal.e2e_tps_available } else { $null }
            network_e2e_tps_p50 = if ($null -ne $netSignal -and $null -ne $netSignal.e2e_tps_p50) { Format-F64 ([double]$netSignal.e2e_tps_p50) } else { $null }
            network_e2e_tps_p90 = if ($null -ne $netSignal -and $null -ne $netSignal.e2e_tps_p90) { Format-F64 ([double]$netSignal.e2e_tps_p90) } else { $null }
            network_e2e_tps_p99 = if ($null -ne $netSignal -and $null -ne $netSignal.e2e_tps_p99) { Format-F64 ([double]$netSignal.e2e_tps_p99) } else { $null }
            network_consensus_source_json = if ($null -ne $netSignal) { [string]$netSignal.source_json } else { $null }
        }) | Out-Null
    }
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    repo_root = $RepoRoot
    output_dir = $OutputDir
    raw_csv = $RawCsvOutputPath
    doc_md = $DocOutputPath
    mode = if ($IncludeNetworkConsensusMatrix) { "aoem_core_plus_sidecar_e2e" } else { "aoem_core_plus_sidecar" }
    params = [ordered]@{
        repeats = $Repeats
        txs = $Txs
        key_space = $KeySpace
        rw = $Rw
        seed = $Seed
        warmup_calls = $WarmupCalls
        build_profile = $BuildProfile
        aoem_plugin_dir = $AoemPluginDir
        prefer_composed_aoem_runtime = $PreferComposedAoemRuntime
        include_network_consensus_matrix = $IncludeNetworkConsensusMatrix
        network_probe_mode = if ($IncludeNetworkConsensusMatrix) { $NetworkProbeMode } else { $null }
        network_rounds = if ($IncludeNetworkConsensusMatrix) { $NetworkRounds } else { $null }
        network_node_count = if ($IncludeNetworkConsensusMatrix) { $NetworkNodeCount } else { $null }
        network_timeout_seconds = if ($IncludeNetworkConsensusMatrix) { $NetworkTimeoutSeconds } else { $null }
    }
    network_consensus_by_variant = @($networkConsensusByVariant.Values)
    matrix = @($matrixRows.ToArray())
    samples = $rawArray
}

$summaryJsonPath = Join-Path $OutputDir "aoem-core-sidecar-tps-summary.json"
$summaryMdPath = Join-Path $OutputDir "aoem-core-sidecar-tps-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJsonPath -Encoding UTF8

$doc = New-Object System.Collections.Generic.List[string]
$doc.Add("# AOEM FFI Core + Sidecar TPS Seal ($dateTag)")
$doc.Add("")
$doc.Add("## Goal")
$doc.Add("")
$doc.Add('- Use unified `AOEM core + optional sidecar` route (no variant DLL mode).')
$doc.Add('- Keep `ffi_perf_worldline` measurement shape and export P50/P90/P99 (nearest-rank).')
$doc.Add('- Cover `core/persist/wasm` with 4 lines (single/auto x parity/batch_stress).')
if ($IncludeNetworkConsensusMatrix) {
    $doc.Add('- Include `network + consensus` E2E probe matrix (block wire / view-sync / new-view).')
}
$doc.Add("")
$doc.Add("## Fixed Parameters")
$doc.Add("")
$doc.Add('- Example: `crates/aoem-bindings/examples/ffi_perf_worldline.rs`')
$doc.Add('- Core DLL: `SUPERVM/aoem/<platform>/core/bin/<dynlib>`')
$doc.Add('- Sidecar dir: `SUPERVM/aoem/<platform>/core/plugins`')
$doc.Add(('- Fixed args: `txs={0}`, `key_space={1}`, `rw={2}`, `seed={3}`, `warmup_calls={4}`' -f $Txs, $KeySpace, $Rw, $Seed, $WarmupCalls))
$doc.Add(('- Repeats: `n={0}`' -f $Repeats))
if ($IncludeNetworkConsensusMatrix) {
    $doc.Add(('- E2E network args: `mode={0}`, `rounds={1}`, `node_count={2}`, `timeout={3}s`' -f $NetworkProbeMode, $NetworkRounds, $NetworkNodeCount, $NetworkTimeoutSeconds))
}
$doc.Add("")
$doc.Add("## Statistics")
$doc.Add("")
$doc.Add('- P50/P90/P99 use nearest-rank.')
$doc.Add(('- Note: when `n={0}`, P90/P99 are for comparison only, not stability claims.' -f $Repeats))
$doc.Add("")
$doc.Add("## Raw Artifacts")
$doc.Add("")
$doc.Add(('- `{0}`' -f $RawCsvOutputPath))
$doc.Add(('- `{0}`' -f $summaryJsonPath))
if ($IncludeNetworkConsensusMatrix) {
    foreach ($variant in $variants) {
        if ($networkConsensusByVariant.ContainsKey($variant)) {
            $doc.Add(('- `{0}` (network+consensus, variant={1})' -f $networkConsensusByVariant[$variant].source_json, $variant))
        }
    }
}
$doc.Add("")

foreach ($variant in $variants) {
    $rows = @($matrixRows | Where-Object { $_.variant -eq $variant })
    if ($rows.Count -eq 0) { continue }
    $doc.Add("## $variant Matrix ($Repeats-run, P50/P90/P99)")
    $doc.Add("")
    $doc.Add("| line_name | preset | submit_ops | threads_arg | engine_workers_arg | runtime_mode | P50 ops/s | P90 ops/s | P99 ops/s | P50 plans/s | P50 calls/s | P50 avg_ops_per_plan |")
    $doc.Add("|---|---|---:|---|---|---|---:|---:|---:|---:|---:|---:|")
    foreach ($lineName in $lineOrder) {
        $r = @($rows | Where-Object { $_.line_name -eq $lineName }) | Select-Object -First 1
        if ($null -eq $r) { continue }
        $doc.Add("| $($r.line_name) | $($r.preset) | $($r.submit_ops) | $($r.threads_arg) | $($r.engine_workers_arg) | $($r.runtime_mode) | $($r.p50_ops_per_s) | $($r.p90_ops_per_s) | $($r.p99_ops_per_s) | $($r.p50_plans_per_s) | $($r.p50_calls_per_s) | $($r.p50_avg_ops_per_plan) |")
    }
    $doc.Add("")
    $doc.Add("### $variant Samples ($Repeats runs, ops/s)")
    $doc.Add("")
    foreach ($lineName in $lineOrder) {
        $r = @($rows | Where-Object { $_.line_name -eq $lineName }) | Select-Object -First 1
        if ($null -eq $r) { continue }
        $doc.Add(('- `{0}`' -f $lineName))
        foreach ($sample in $r.tps_samples) {
            $doc.Add("  - $sample")
        }
    }
    $doc.Add("")
}

if ($IncludeNetworkConsensusMatrix) {
    $doc.Add("## Network + Consensus E2E Matrix (by AOEM variant)")
    $doc.Add("")
    $doc.Add("| variant | pass | mode | rounds | round_pass_ratio | pair_pass_ratio | block_wire_pass | view_sync_pass | new_view_pass | consensus_binding_pass | pacemaker_pass | e2e_tps_p50 | e2e_tps_p90 | e2e_tps_p99 |")
    $doc.Add("|---|---|---|---:|---:|---:|---|---|---|---|---|---:|---:|---:|")
    foreach ($variant in $variants) {
        if (-not $networkConsensusByVariant.ContainsKey($variant)) { continue }
        $s = $networkConsensusByVariant[$variant]
        $p50 = if ($s.e2e_tps_available -and $null -ne $s.e2e_tps_p50) { Format-F64 ([double]$s.e2e_tps_p50) } else { "-" }
        $p90 = if ($s.e2e_tps_available -and $null -ne $s.e2e_tps_p90) { Format-F64 ([double]$s.e2e_tps_p90) } else { "-" }
        $p99 = if ($s.e2e_tps_available -and $null -ne $s.e2e_tps_p99) { Format-F64 ([double]$s.e2e_tps_p99) } else { "-" }
        $doc.Add("| $($s.variant) | $($s.pass) | $($s.mode) | $($s.rounds) | $(Format-F64 ([double]$s.round_pass_ratio)) | $(Format-F64 ([double]$s.pair_pass_ratio)) | $($s.block_wire_pass) | $($s.view_sync_pass) | $($s.new_view_pass) | $($s.consensus_binding_pass) | $($s.pacemaker_pass) | $p50 | $p90 | $p99 |")
    }
    $doc.Add("")
}

$doc.Add("## Reproduce")
$doc.Add("")
$doc.Add('```powershell')
$doc.Add(("& scripts/migration/run_aoem_tps_core_sidecar_report.ps1 -RepoRoot {0} -Repeats {1} -Txs {2} -AoemPluginDir {3} -IncludeNetworkConsensusMatrix:{4} -NetworkProbeMode {5} -NetworkRounds {6} -NetworkNodeCount {7} -NetworkTimeoutSeconds {8}" -f $RepoRoot, $Repeats, $Txs, $AoemPluginDir, ('$' + $IncludeNetworkConsensusMatrix.ToString().ToLowerInvariant()), $NetworkProbeMode, $NetworkRounds, $NetworkNodeCount, $NetworkTimeoutSeconds))
$doc.Add('```')
$doc.Add("")
$doc.Add("## Conclusion")
$doc.Add("")
$doc.Add('- `core` runtime mode: `core` (pure core DLL).')
$doc.Add('- `persist/wasm` runtime mode: `composed_plugin_sidecar` (core DLL + sidecar).')
$doc.Add('- `network + consensus` signals come from `run_network_two_process.ps1` (block wire + pacemaker + e2e_tps).')
$doc.Add('- All statistics are persisted in JSON/CSV for baseline comparison.')

$doc -join "`n" | Set-Content -Path $summaryMdPath -Encoding UTF8
$doc -join "`n" | Set-Content -Path $DocOutputPath -Encoding UTF8

Write-Host "aoem core+sidecar tps report generated:"
Write-Host "  summary_json: $summaryJsonPath"
Write-Host "  summary_md:   $summaryMdPath"
Write-Host "  raw_csv:      $RawCsvOutputPath"
Write-Host "  doc_md:       $DocOutputPath"
