param(
    [string]$RepoRoot = "",
    [string]$SvmRoot = "",
    [string]$OutputDir = "",
    [string]$BaselineJson = "",
    [switch]$AutoImportSvmBaseline,
    [string]$BaselineOutputDir = "",
    [string]$Variants = "core",
    [string]$AoemPluginDir = "",
    [bool]$PreferComposedAoemRuntime = $true,
    [double]$AllowedRegressionPct = -5.0,
    [int64]$Txs = 1000000,
    [int]$KeySpace = 128,
    [double]$Rw = 0.5,
    [int]$Seed = 123,
    [int]$WarmupCalls = 5,
    [ValidateRange(0, 30)]
    [int]$PresetCooldownSec = 0,
    [ValidateSet("default", "seal_single", "seal_auto")]
    [string]$LineProfile = "default",
    [ValidateSet("debug", "release")]
    [string]$BuildProfile = "release",
    [bool]$IncludeCapabilitySnapshot = $true,
    [ValidateSet("core", "persist", "wasm")]
    [string]$CapabilityVariant = "core",
    [string]$CapabilityJson = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if (-not (Get-Variable -Name IsWindows -ErrorAction SilentlyContinue)) {
    $IsWindows = ($env:OS -eq "Windows_NT")
}
if (-not (Get-Variable -Name IsMacOS -ErrorAction SilentlyContinue)) {
    $IsMacOS = $false
}
if (-not (Get-Variable -Name IsLinux -ErrorAction SilentlyContinue)) {
    $IsLinux = -not $IsWindows -and -not $IsMacOS
}

function Get-PowerShellHostCommand {
    if (Get-Command -Name "pwsh" -ErrorAction SilentlyContinue) {
        return "pwsh"
    }
    if (Get-Command -Name "powershell" -ErrorAction SilentlyContinue) {
        return "powershell"
    }
    throw "neither pwsh nor powershell is available in PATH"
}

function Resolve-AoemRoot {
    param([string]$RepoRoot)

    $dynlibCandidates = Get-DynlibNameCandidates
    $roots = @()
    if (-not [string]::IsNullOrWhiteSpace($env:NOVOVM_AOEM_ROOT)) {
        $roots += $env:NOVOVM_AOEM_ROOT
    }
    $roots += (Join-Path $RepoRoot "aoem")

    $workspaceParent = Split-Path $RepoRoot -Parent
    if (-not [string]::IsNullOrWhiteSpace($workspaceParent)) {
        $roots += (Join-Path $workspaceParent "AOEM")
        $roots += (Join-Path $workspaceParent "AOEM\artifacts\standalone-run")
    }

    foreach ($root in $roots) {
        if (-not $root -or -not (Test-Path $root)) { continue }
        foreach ($name in $dynlibCandidates) {
            if (Test-Path (Join-Path $root "bin\$name")) {
                return (Resolve-Path $root).Path
            }
        }
    }

    return (Join-Path $RepoRoot "aoem")
}

$PowerShellHost = Get-PowerShellHostCommand

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\performance"
}
if (-not $SvmRoot) {
    $sibling = Join-Path (Split-Path $RepoRoot -Parent) "SVM2026"
    if (Test-Path $sibling) {
        $SvmRoot = $sibling
    } else {
        throw "SvmRoot not found. Pass -SvmRoot explicitly or place sibling SVM2026 repo."
    }
}

function Invoke-Cargo {
    param(
        [string]$WorkDir,
        [string[]]$CargoArgs,
        [hashtable]$EnvVars = @{}
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($CargoArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")
    foreach ($k in $EnvVars.Keys) {
        $psi.Environment[$k] = [string]$EnvVars[$k]
    }

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

function Get-DynlibNameCandidates {
    if ($IsWindows) {
        return @("aoem_ffi.dll")
    }
    if ($IsMacOS) {
        return @("libaoem_ffi.dylib")
    }
    return @("libaoem_ffi.so")
}

function Get-AoemVariantBinDir {
    param([string]$AoemRoot, [string]$Variant)
    switch ($Variant) {
        "core" { return Join-Path $AoemRoot "bin" }
        "persist" { return Join-Path $AoemRoot "bin" }
        "wasm" { return Join-Path $AoemRoot "bin" }
        default { throw "invalid variant: $Variant" }
    }
}

function Get-DllPathForVariant {
    param(
        [string]$AoemRoot,
        [string]$Variant,
        [bool]$RequireExists = $false
    )

    $binDir = Get-AoemVariantBinDir -AoemRoot $AoemRoot -Variant $Variant
    $candidates = Get-DynlibNameCandidates
    foreach ($name in $candidates) {
        $candidate = Join-Path $binDir $name
        if (Test-Path $candidate) {
            return (Resolve-Path $candidate).Path
        }
    }

    $fallback = Join-Path $binDir $candidates[0]
    if ($RequireExists) {
        throw "aoem dynlib not found for variant=$Variant under $binDir (tried: $($candidates -join ', '))"
    }
    return $fallback
}

function Get-AoemPluginNameCandidatesForVariant {
    param([string]$Variant)
    switch ($Variant) {
        "persist" {
            if ($IsWindows) { return @("aoem_ffi_persist_rocksdb.dll") }
            if ($IsMacOS) { return @("libaoem_ffi_persist_rocksdb.dylib") }
            return @("libaoem_ffi_persist_rocksdb.so")
        }
        "wasm" {
            if ($IsWindows) { return @("aoem_ffi_runtime_wasm_wasmtime.dll") }
            if ($IsMacOS) { return @("libaoem_ffi_runtime_wasm_wasmtime.dylib") }
            return @("libaoem_ffi_runtime_wasm_wasmtime.so")
        }
        default { return @() }
    }
}

function Resolve-AoemRuntimeForVariant {
    param(
        [string]$AoemRoot,
        [string]$Variant,
        [string]$AoemPluginDir,
        [bool]$PreferComposed = $true,
        [bool]$RequireExists = $false
    )

    $coreDll = Get-DllPathForVariant -AoemRoot $AoemRoot -Variant "core" -RequireExists:$false

    if ($Variant -eq "core") {
        if ($RequireExists -and -not (Test-Path $coreDll)) {
            throw "aoem core dynlib not found: $coreDll"
        }
        return [ordered]@{
            dll = $coreDll
            mode = "core"
            env = @{}
        }
    }

    if ($PreferComposed -and (Test-Path $coreDll)) {
        $pluginNames = Get-AoemPluginNameCandidatesForVariant -Variant $Variant
        $candidateDirs = @()
        if ($AoemPluginDir) {
            $candidateDirs += $AoemPluginDir
            $candidateDirs += (Join-Path $AoemRoot $AoemPluginDir)
        }
        $candidateDirs += @(
            (Join-Path $AoemRoot "plugins"),
            (Join-Path $AoemRoot "bin\plugins"),
            (Join-Path $AoemRoot "bin")
        )
        $pluginDirFound = ""
        foreach ($dir in $candidateDirs) {
            if (-not $dir -or -not (Test-Path $dir)) { continue }
            foreach ($name in $pluginNames) {
                if (Test-Path (Join-Path $dir $name)) {
                    $pluginDirFound = (Resolve-Path $dir).Path
                    break
                }
            }
            if ($pluginDirFound) { break }
        }

        if ($pluginDirFound) {
            $envVars = @{
                AOEM_FFI_PLUGIN_DIR = $pluginDirFound
                AOEM_FFI_PERSIST_BACKEND = "none"
                AOEM_FFI_WASM_RUNTIME = "none"
                AOEM_FFI_ZKVM_MODE = "none"
                AOEM_FFI_MLDSA_MODE = "none"
            }
            if ($Variant -eq "persist") {
                $envVars["AOEM_FFI_PERSIST_BACKEND"] = "rocksdb"
                $envVars["AOEM_FFI_PERSIST_PLUGIN_DIR"] = $pluginDirFound
            } elseif ($Variant -eq "wasm") {
                $envVars["AOEM_FFI_WASM_RUNTIME"] = "wasmtime"
                $envVars["AOEM_FFI_WASM_PLUGIN_DIR"] = $pluginDirFound
            }
            return [ordered]@{
                dll = $coreDll
                mode = "composed_plugin_sidecar"
                env = $envVars
            }
        }
    }

    if ($RequireExists) {
        throw "aoem sidecar plugin not found for variant=$Variant (core=$coreDll); require core+sidecar mode"
    }

    return [ordered]@{
        dll = $coreDll
        mode = "sidecar_missing"
        env = @{}
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
        [string]$CapabilityJson,
        [string]$AoemPluginDir,
        [bool]$PreferComposedAoemRuntime
    )

    $sourceJson = $CapabilityJson
    if (-not $sourceJson) {
        $scriptPath = Join-Path $RepoRoot "scripts\migration\dump_capability_contract.ps1"
        if (-not (Test-Path $scriptPath)) {
            throw "missing capability dump script: $scriptPath"
        }

        $capOutputDir = Join-Path $RepoRoot "artifacts\migration\capabilities"
        New-Item -ItemType Directory -Force -Path $capOutputDir | Out-Null

        if ($AoemPluginDir) {
            & $PowerShellHost -NoProfile -File $scriptPath -RepoRoot $RepoRoot -OutputDir $capOutputDir -Variant $Variant -AoemPluginDir $AoemPluginDir | Out-Null
        } else {
            & $PowerShellHost -NoProfile -File $scriptPath -RepoRoot $RepoRoot -OutputDir $capOutputDir -Variant $Variant | Out-Null
        }
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
    $fallbackReason = ""
    if ($null -ne $raw.contract.fallback_reason) {
        $fallbackReason = [string]$raw.contract.fallback_reason
    }
    $zkFormalFieldsPresent = $false
    if ($null -ne $raw.contract.zk_formal_fields_present) {
        $zkFormalFieldsPresent = [bool]$raw.contract.zk_formal_fields_present
    }
    $proverReady = $false
    if ($raw.prover_contract -and $null -ne $raw.prover_contract.prover_ready) {
        $proverReady = [bool]$raw.prover_contract.prover_ready
    }

    return [ordered]@{
        source_json = $sourceJson
        generated_at_utc = [string]$raw.generated_at_utc
        variant = [string]$raw.variant
        execute_ops_v2 = [bool]$raw.contract.execute_ops_v2
        zkvm_prove = [bool]$raw.contract.zkvm_prove
        zkvm_verify = [bool]$raw.contract.zkvm_verify
        zkvm_probe_api_present = [bool]$raw.contract.zkvm_probe_api_present
        zkvm_symbol_supported = if ($null -ne $raw.contract.zkvm_symbol_supported) { [bool]$raw.contract.zkvm_symbol_supported } else { $null }
        zk_formal_fields_present = $zkFormalFieldsPresent
        msm_accel = [bool]$raw.contract.msm_accel
        msm_backend = [string]$raw.contract.msm_backend
        mldsa_verify = [bool]$raw.contract.mldsa_verify
        fallback_reason = $fallbackReason
        fallback_reason_codes = $fallbackCodes
        prover_ready = $proverReady
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

    & $PowerShellHost -NoProfile -File $scriptPath `
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

if ($BuildProfile -eq "debug") {
    Write-Warning "BuildProfile=debug may produce lower TPS than release-seal metrics."
}

$bindingsDir = Join-Path $RepoRoot "crates\aoem-bindings"
$aoemRoot = Resolve-AoemRoot -RepoRoot $RepoRoot
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
    $runtime = Resolve-AoemRuntimeForVariant -AoemRoot $aoemRoot -Variant $variant -AoemPluginDir $AoemPluginDir -PreferComposed:$PreferComposedAoemRuntime -RequireExists $true
    $dll = [string]$runtime.dll
    $envVars = @{}
    foreach ($k in $runtime.env.Keys) { $envVars[$k] = $runtime.env[$k] }
    for ($presetIndex = 0; $presetIndex -lt $presets.Count; $presetIndex++) {
        $preset = $presets[$presetIndex]
        $submitOps = if ($preset -eq "cpu_parity") { "1" } else { "1024" }
        $cargoArgs = @("run")
        if ($BuildProfile -eq "release") {
            $cargoArgs += "--release"
        }
        $cargoArgs += @(
            "--example", "ffi_perf_worldline", "--",
            "--preset", $preset,
            "--dll", $dll,
            "--submit-ops", $submitOps,
            "--txs", "$Txs",
            "--key-space", "$KeySpace",
            "--rw", "$Rw",
            "--seed", "$Seed",
            "--warmup-calls", "$WarmupCalls"
        )

        switch ($LineProfile) {
            "seal_single" {
                $cargoArgs += @("--threads", "1", "--engine-workers", "4")
            }
            "seal_auto" {
                $cargoArgs += @("--threads", "auto", "--engine-workers", "auto")
            }
            default { }
        }

        $text = Invoke-Cargo -WorkDir $bindingsDir -CargoArgs $cargoArgs -EnvVars $envVars
        $parsed = Parse-WorldlineResult -Text $text
        $parsed["variant"] = $variant
        $parsed["preset"] = $preset
        $parsed["dll"] = $dll
        $parsed["runtime_mode"] = [string]$runtime.mode
        $items += [pscustomobject]$parsed

        if ($PresetCooldownSec -gt 0 -and $presetIndex -lt ($presets.Count - 1)) {
            Start-Sleep -Seconds $PresetCooldownSec
        }
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
$capabilitySnapshotNote = "capability snapshot is disabled for this run"
if ($IncludeCapabilitySnapshot) {
    try {
        $capabilitySnapshot = Get-CapabilitySnapshot -RepoRoot $RepoRoot -Variant $CapabilityVariant -CapabilityJson $CapabilityJson -AoemPluginDir $AoemPluginDir -PreferComposedAoemRuntime:$PreferComposedAoemRuntime
        $capabilitySnapshotNote = "capability snapshot loaded (variant=$CapabilityVariant)"
    } catch {
        $capabilitySnapshot = $null
        $capabilitySnapshotNote = "capability snapshot failed and was skipped: $($_.Exception.Message)"
    }
}

$result = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    baseline_json = if ($baselineAvailable) { $baselineJsonResolved } else { "" }
    baseline_available = $baselineAvailable
    allowed_regression_pct = $AllowedRegressionPct
    params = [ordered]@{
        variants = @($variantList)
        build_profile = $BuildProfile
        line_profile = $LineProfile
        txs = $Txs
        key_space = $KeySpace
        rw = $Rw
        seed = $Seed
        warmup_calls = $WarmupCalls
        preset_cooldown_sec = $PresetCooldownSec
    }
    items = $items
    compare = $compareRows
    compare_pass = if ($baselineAvailable) { $comparePass } else { $null }
    capability_contract = $capabilitySnapshot
    notes = @(
        "performance compare is only evaluated when a baseline JSON is provided",
        "capability snapshot records zk/msm readiness at report generation time",
        $capabilitySnapshotNote
    )
}

$jsonPath = Join-Path $OutputDir "performance-compare.json"
$mdPath = Join-Path $OutputDir "performance-compare.md"

$result | ConvertTo-Json -Depth 8 | Set-Content -Path $jsonPath -Encoding UTF8

$md = @(
    "# Performance Compare Report"
    ""
    "- generated_at_utc: $($result.generated_at_utc)"
    "- build_profile: $BuildProfile"
    "- line_profile: $LineProfile"
    "- baseline_available: $($result.baseline_available)"
    "- allowed_regression_pct: $($result.allowed_regression_pct)"
    "- compare_pass: $($result.compare_pass)"
    ""
    "## Current Metrics"
    ""
    "| variant | preset | runtime_mode | tps(ops/s) | elapsed_sec | done_ops |"
    "|---|---|---|---:|---:|---:|"
)

foreach ($item in $items) {
    $md += "| $($item.variant) | $($item.preset) | $($item.runtime_mode) | $([Math]::Round($item.tps,2)) | $([Math]::Round($item.elapsed_sec,3)) | $($item.done_ops) |"
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
    $md += "- zkvm_probe_api_present: $($capabilitySnapshot.zkvm_probe_api_present)"
    $md += "- zkvm_symbol_supported: $($capabilitySnapshot.zkvm_symbol_supported)"
    $md += "- zk_formal_fields_present: $($capabilitySnapshot.zk_formal_fields_present)"
    $md += "- prover_ready: $($capabilitySnapshot.prover_ready)"
    $md += "- msm_accel: $($capabilitySnapshot.msm_accel)"
    $md += "- msm_backend: $($capabilitySnapshot.msm_backend)"
    $md += "- mldsa_verify: $($capabilitySnapshot.mldsa_verify)"
    $md += "- fallback_reason: $($capabilitySnapshot.fallback_reason)"
    $md += "- fallback_reason_codes: $((@($capabilitySnapshot.fallback_reason_codes) -join ', '))"
    $md += "- inferred_from_legacy_fields: $($capabilitySnapshot.inferred_from_legacy_fields)"
}

$md -join "`n" | Set-Content -Path $mdPath -Encoding UTF8

Write-Host "performance compare report generated:"
Write-Host "  $jsonPath"
Write-Host "  $mdPath"
