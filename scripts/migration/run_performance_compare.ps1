param(
    [string]$RepoRoot = "D:\WorksArea\SUPERVM",
    [string]$SvmRoot = "D:\WorksArea\SVM2026",
    [string]$OutputDir = "D:\WorksArea\SUPERVM\artifacts\migration\performance",
    [string]$BaselineJson = "",
    [switch]$AutoImportSvmBaseline,
    [string]$BaselineOutputDir = "",
    [string]$Variants = "core",
    [double]$AllowedRegressionPct = -5.0,
    [int64]$Txs = 1000000,
    [int]$KeySpace = 128,
    [double]$Rw = 0.5,
    [int]$Seed = 123,
    [int]$WarmupCalls = 10,
    [bool]$IncludeCapabilitySnapshot = $true,
    [ValidateSet("core", "persist", "wasm")]
    [string]$CapabilityVariant = "core",
    [string]$CapabilityJson = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

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

    $text = ($stdout + $stderr).Trim()
    if ($proc.ExitCode -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed in $WorkDir`n$text"
    }
    return $text
}

function Parse-WorldlineResult {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^result:" } | Select-Object -Last 1)
    if (-not $line) {
        throw "ffi_perf_worldline output missing result line"
    }

    $m = [regex]::Match(
        $line,
        "^result: elapsed_sec=(?<elapsed>[0-9.]+), done_ops=(?<done_ops>\d+), done_plans=(?<done_plans>\d+), done_calls=(?<done_calls>\d+), tps_unit=ops_per_s, tps=(?<tps>[0-9.]+), plans_per_s=(?<plans_per_s>[0-9.]+), ffi_v2_calls_per_s=(?<calls_per_s>[0-9.]+), avg_ops_per_plan=(?<avg_plan>[0-9.]+), avg_ops_per_call=(?<avg_call>[0-9.]+)$"
    )
    if (-not $m.Success) {
        throw "cannot parse worldline result line: $line"
    }

    return [ordered]@{
        elapsed_sec = [double]$m.Groups["elapsed"].Value
        done_ops = [int64]$m.Groups["done_ops"].Value
        done_plans = [int64]$m.Groups["done_plans"].Value
        done_calls = [int64]$m.Groups["done_calls"].Value
        tps = [double]$m.Groups["tps"].Value
        plans_per_s = [double]$m.Groups["plans_per_s"].Value
        ffi_v2_calls_per_s = [double]$m.Groups["calls_per_s"].Value
        avg_ops_per_plan = [double]$m.Groups["avg_plan"].Value
        avg_ops_per_call = [double]$m.Groups["avg_call"].Value
    }
}

function Get-DllPathForVariant {
    param([string]$AoemRoot, [string]$Variant)
    switch ($Variant) {
        "core" { return Join-Path $AoemRoot "bin\aoem_ffi.dll" }
        "persist" { return Join-Path $AoemRoot "variants\persist\bin\aoem_ffi.dll" }
        "wasm" { return Join-Path $AoemRoot "variants\wasm\bin\aoem_ffi.dll" }
        default { throw "invalid variant: $Variant" }
    }
}

function Get-CaseKey {
    param([string]$Variant, [string]$Preset)
    return "$Variant|$Preset"
}

function Get-CapabilitySnapshot {
    param(
        [string]$RepoRoot,
        [string]$Variant,
        [string]$CapabilityJson
    )

    $sourceJson = $CapabilityJson
    if (-not $sourceJson) {
        $scriptPath = Join-Path $RepoRoot "scripts\migration\dump_capability_contract.ps1"
        if (-not (Test-Path $scriptPath)) {
            throw "missing capability dump script: $scriptPath"
        }

        $capOutputDir = Join-Path $RepoRoot "artifacts\migration\capabilities"
        New-Item -ItemType Directory -Force -Path $capOutputDir | Out-Null

        & powershell -ExecutionPolicy Bypass -File $scriptPath -RepoRoot $RepoRoot -OutputDir $capOutputDir -Variant $Variant | Out-Null
        $sourceJson = Join-Path $capOutputDir "capability-contract-$Variant.json"
    }

    if (-not (Test-Path $sourceJson)) {
        throw "capability json not found: $sourceJson"
    }

    $raw = Get-Content -Path $sourceJson -Raw | ConvertFrom-Json
    if (-not $raw.contract) {
        throw "invalid capability json (missing contract): $sourceJson"
    }

    $fallbackCodes = @()
    if ($null -ne $raw.contract.fallback_reason_codes) {
        $fallbackCodes = @($raw.contract.fallback_reason_codes)
    }

    return [ordered]@{
        source_json = $sourceJson
        generated_at_utc = [string]$raw.generated_at_utc
        variant = [string]$raw.variant
        execute_ops_v2 = [bool]$raw.contract.execute_ops_v2
        zkvm_prove = [bool]$raw.contract.zkvm_prove
        zkvm_verify = [bool]$raw.contract.zkvm_verify
        msm_accel = [bool]$raw.contract.msm_accel
        msm_backend = [string]$raw.contract.msm_backend
        fallback_reason_codes = $fallbackCodes
        inferred_from_legacy_fields = [bool]$raw.contract.inferred_from_legacy_fields
    }
}

function Resolve-BaselineJsonPath {
    param(
        [string]$RepoRoot,
        [string]$SvmRoot,
        [string]$BaselineJson,
        [switch]$AutoImportSvmBaseline,
        [string]$BaselineOutputDir,
        [string]$Variant
    )

    if ($BaselineJson -and (Test-Path $BaselineJson)) {
        return $BaselineJson
    }

    if (-not $AutoImportSvmBaseline.IsPresent) {
        return ""
    }

    $scriptPath = Join-Path $RepoRoot "scripts\migration\import_svm2026_baseline.ps1"
    if (-not (Test-Path $scriptPath)) {
        throw "missing baseline import script: $scriptPath"
    }

    $outDir = if ($BaselineOutputDir) { $BaselineOutputDir } else { Join-Path $RepoRoot "artifacts\migration\baseline" }
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null

    & powershell -ExecutionPolicy Bypass -File $scriptPath `
        -RepoRoot $RepoRoot `
        -SvmRoot $SvmRoot `
        -OutputDir $outDir `
        -Variant $Variant | Out-Null

    $resolved = Join-Path $outDir "svm2026-baseline-$Variant.json"
    if (-not (Test-Path $resolved)) {
        throw "auto-import baseline json not found: $resolved"
    }
    return $resolved
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$bindingsDir = Join-Path $RepoRoot "crates\aoem-bindings"
$aoemRoot = Join-Path $RepoRoot "aoem"
$variantList = @($Variants.Split(",") | ForEach-Object { $_.Trim().ToLower() } | Where-Object { $_ -ne "" })
$presets = @("cpu_parity", "cpu_batch_stress")
$baselineVariant = if ($variantList.Count -gt 0) { $variantList[0] } else { "core" }
$baselineJsonResolved = Resolve-BaselineJsonPath `
    -RepoRoot $RepoRoot `
    -SvmRoot $SvmRoot `
    -BaselineJson $BaselineJson `
    -AutoImportSvmBaseline:$AutoImportSvmBaseline `
    -BaselineOutputDir $BaselineOutputDir `
    -Variant $baselineVariant

$items = @()
foreach ($variant in $variantList) {
    $dll = Get-DllPathForVariant -AoemRoot $aoemRoot -Variant $variant
    foreach ($preset in $presets) {
        $text = Invoke-Cargo -WorkDir $bindingsDir -CargoArgs @(
            "run", "--example", "ffi_perf_worldline", "--",
            "--preset", $preset,
            "--dll", $dll,
            "--txs", "$Txs",
            "--key-space", "$KeySpace",
            "--rw", "$Rw",
            "--seed", "$Seed",
            "--warmup-calls", "$WarmupCalls"
        )
        $parsed = Parse-WorldlineResult -Text $text
        $parsed["variant"] = $variant
        $parsed["preset"] = $preset
        $parsed["dll"] = $dll
        $items += [pscustomobject]$parsed
    }
}

$baselineItems = @{}
$baselineAvailable = $false
if ($baselineJsonResolved -and (Test-Path $baselineJsonResolved)) {
    $baselineRaw = Get-Content -Path $baselineJsonResolved -Raw | ConvertFrom-Json
    if ($baselineRaw.items) {
        foreach ($b in $baselineRaw.items) {
            $k = Get-CaseKey -Variant $b.variant -Preset $b.preset
            $baselineItems[$k] = $b
        }
        $baselineAvailable = $true
    }
}

$compareRows = @()
$comparePass = $true
if ($baselineAvailable) {
    foreach ($item in $items) {
        $k = Get-CaseKey -Variant $item.variant -Preset $item.preset
        if (-not $baselineItems.ContainsKey($k)) {
            $compareRows += [pscustomobject]@{
                variant = $item.variant
                preset = $item.preset
                baseline_tps = $null
                current_tps = $item.tps
                delta_pct = $null
                pass = $false
                reason = "missing_baseline_case"
            }
            $comparePass = $false
            continue
        }

        $base = [double]$baselineItems[$k].tps
        $deltaPct = if ($base -le 0.0) { 0.0 } else { (($item.tps - $base) / $base) * 100.0 }
        $pass = $deltaPct -ge $AllowedRegressionPct
        if (-not $pass) {
            $comparePass = $false
        }

        $compareRows += [pscustomobject]@{
            variant = $item.variant
            preset = $item.preset
            baseline_tps = [Math]::Round($base, 2)
            current_tps = [Math]::Round($item.tps, 2)
            delta_pct = [Math]::Round($deltaPct, 2)
            pass = $pass
            reason = if ($pass) { "within_threshold" } else { "regression_exceeds_threshold" }
        }
    }
}

$capabilitySnapshot = $null
if ($IncludeCapabilitySnapshot) {
    $capabilitySnapshot = Get-CapabilitySnapshot -RepoRoot $RepoRoot -Variant $CapabilityVariant -CapabilityJson $CapabilityJson
}

$result = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    baseline_json = if ($baselineAvailable) { $baselineJsonResolved } else { "" }
    baseline_available = $baselineAvailable
    allowed_regression_pct = $AllowedRegressionPct
    params = [ordered]@{
        variants = @($variantList)
        txs = $Txs
        key_space = $KeySpace
        rw = $Rw
        seed = $Seed
        warmup_calls = $WarmupCalls
    }
    items = $items
    compare = $compareRows
    compare_pass = if ($baselineAvailable) { $comparePass } else { $null }
    capability_contract = $capabilitySnapshot
    notes = @(
        "performance compare is only evaluated when a baseline JSON is provided",
        "capability snapshot records zk/msm readiness at report generation time"
    )
}

$jsonPath = Join-Path $OutputDir "performance-compare.json"
$mdPath = Join-Path $OutputDir "performance-compare.md"

$result | ConvertTo-Json -Depth 8 | Set-Content -Path $jsonPath -Encoding UTF8

$md = @(
    "# Performance Compare Report"
    ""
    "- generated_at_utc: $($result.generated_at_utc)"
    "- baseline_available: $($result.baseline_available)"
    "- allowed_regression_pct: $($result.allowed_regression_pct)"
    "- compare_pass: $($result.compare_pass)"
    ""
    "## Current Metrics"
    ""
    "| variant | preset | tps(ops/s) | elapsed_sec | done_ops |"
    "|---|---|---:|---:|---:|"
)

foreach ($item in $items) {
    $md += "| $($item.variant) | $($item.preset) | $([Math]::Round($item.tps,2)) | $([Math]::Round($item.elapsed_sec,3)) | $($item.done_ops) |"
}

if ($baselineAvailable) {
    $md += ""
    $md += "## Compare Against Baseline"
    $md += ""
    $md += "| variant | preset | baseline_tps | current_tps | delta_pct | pass | reason |"
    $md += "|---|---|---:|---:|---:|---|---|"
    foreach ($row in $compareRows) {
        $md += "| $($row.variant) | $($row.preset) | $($row.baseline_tps) | $($row.current_tps) | $($row.delta_pct) | $($row.pass) | $($row.reason) |"
    }
}

$md += ""
$md += "## Notes"
$md += ""
foreach ($n in $result.notes) {
    $md += "- $n"
}

if ($capabilitySnapshot) {
    $md += ""
    $md += "## Capability Snapshot"
    $md += ""
    $md += "- source_json: $($capabilitySnapshot.source_json)"
    $md += "- variant: $($capabilitySnapshot.variant)"
    $md += "- execute_ops_v2: $($capabilitySnapshot.execute_ops_v2)"
    $md += "- zkvm_prove: $($capabilitySnapshot.zkvm_prove)"
    $md += "- zkvm_verify: $($capabilitySnapshot.zkvm_verify)"
    $md += "- msm_accel: $($capabilitySnapshot.msm_accel)"
    $md += "- msm_backend: $($capabilitySnapshot.msm_backend)"
    $md += "- inferred_from_legacy_fields: $($capabilitySnapshot.inferred_from_legacy_fields)"
}

$md -join "`n" | Set-Content -Path $mdPath -Encoding UTF8

Write-Host "performance compare report generated:"
Write-Host "  $jsonPath"
Write-Host "  $mdPath"
