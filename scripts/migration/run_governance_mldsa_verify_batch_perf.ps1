param(
    [string]$RepoRoot = "D:\WEB3_AI\SUPERVM",
    [string]$DllPath = "D:\WEB3_AI\SUPERVM\aoem\windows\core\bin\aoem_ffi.dll",
    [int[]]$Counts = @(1000, 10000, 100000),
    [int[]]$ParMinSet = @(1, 64),
    [int]$Repeats = 5,
    [int]$Level = 87,
    [int]$MessageSize = 32,
    [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"

function Get-NearestRankQuantile {
    param(
        [double[]]$Values,
        [double]$Quantile
    )
    if ($null -eq $Values -or $Values.Count -eq 0) { return 0.0 }
    $sorted = @($Values | Sort-Object)
    $q = [Math]::Min([Math]::Max($Quantile, 0.0), 1.0)
    $rank = [int][Math]::Ceiling($q * $sorted.Count)
    if ($rank -lt 1) { $rank = 1 }
    return [double]$sorted[$rank - 1]
}

function Format-F64 {
    param([double]$Value)
    return [Math]::Round($Value, 2)
}

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $stamp = Get-Date -Format "yyyy-MM-dd-HHmmss"
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\governance-mldsa-batch-perf-$stamp"
}
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
$casesDir = Join-Path $OutputDir "cases"
New-Item -ItemType Directory -Path $casesDir -Force | Out-Null

if (-not (Test-Path $DllPath)) {
    throw "missing AOEM FFI dll: $DllPath"
}

Push-Location $RepoRoot
try {
    & cargo build --release -p aoem-bindings --example mldsa_verify_batch_perf
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed for aoem-bindings example mldsa_verify_batch_perf"
    }
}
finally {
    Pop-Location
}

$exe = Join-Path $RepoRoot "target\release\examples\mldsa_verify_batch_perf.exe"
if (-not (Test-Path $exe)) {
    throw "benchmark executable not found: $exe"
}

$rows = New-Object System.Collections.Generic.List[object]
$rawCases = New-Object System.Collections.Generic.List[object]

foreach ($parMin in $ParMinSet) {
    foreach ($count in $Counts) {
        $outJson = Join-Path $casesDir ("mldsa-verify-batch-par{0}-n{1}.json" -f $parMin, $count)
        Write-Host ("[mldsa-batch-perf] run: par_min={0} count={1} repeats={2}" -f $parMin, $count, $Repeats)

        $env:AOEM_MLDSA_VERIFY_BATCH_PAR_MIN = "$parMin"
        & $exe `
            --dll $DllPath `
            --count $count `
            --repeats $Repeats `
            --level $Level `
            --message-size $MessageSize `
            --out $outJson
        if ($LASTEXITCODE -ne 0) {
            throw "mldsa_verify_batch_perf failed: par_min=$parMin count=$count"
        }

        $case = Get-Content -Path $outJson -Raw | ConvertFrom-Json
        $rawCases.Add($case) | Out-Null

        $rows.Add([pscustomobject]@{
            par_min = [int]$parMin
            count = [int]$count
            repeats = [int]$case.repeats
            p50_tps = [double]$case.p50_tps
            p90_tps = [double]$case.p90_tps
            p99_tps = [double]$case.p99_tps
            p50_ms = [double]$case.p50_ms
            p90_ms = [double]$case.p90_ms
            p99_ms = [double]$case.p99_ms
        }) | Out-Null
    }
}

$bestByCount = @()
foreach ($count in ($Counts | Sort-Object -Unique)) {
    $pick = $rows |
        Where-Object { $_.count -eq $count } |
        Sort-Object -Property @{Expression = "p50_tps"; Descending = $true}, @{Expression = "par_min"; Descending = $false} |
        Select-Object -First 1
    if ($null -ne $pick) {
        $bestByCount += [pscustomobject]@{
            count = [int]$pick.count
            best_par_min = [int]$pick.par_min
            best_p50_tps = [double]$pick.p50_tps
            best_p90_tps = [double]$pick.p90_tps
            best_p99_tps = [double]$pick.p99_tps
        }
    }
}

$summary = @{}
$summary["generated_at"] = (Get-Date).ToString("o")
$summary["repo_root"] = $RepoRoot
$summary["dll_path"] = $DllPath
$summary["level"] = $Level
$summary["message_size"] = $MessageSize
$summary["repeats"] = $Repeats
$summary["counts"] = @($Counts)
$summary["par_min_set"] = @($ParMinSet)
$summary["rows"] = $rows.ToArray()
$summary["best_by_count"] = @($bestByCount)
$summaryJson = Join-Path $OutputDir "governance-mldsa-verify-batch-perf-summary.json"
$summary | ConvertTo-Json -Depth 12 | Set-Content -Path $summaryJson -Encoding UTF8

$md = New-Object System.Collections.Generic.List[string]
$md.Add("# Governance ML-DSA Batch Verify Perf")
$md.Add("")
$md.Add("- generated_at: $($summary["generated_at"])")
$md.Add("- dll_path: $DllPath")
$md.Add("- level: $Level")
$md.Add("- message_size: $MessageSize")
$md.Add("- repeats: $Repeats")
$md.Add("- counts: $($Counts -join ', ')")
$md.Add("- par_min_set: $($ParMinSet -join ', ')")
$md.Add("")
$md.Add("## Matrix")
$md.Add("")
$md.Add("| par_min | count | repeats | p50_tps | p90_tps | p99_tps | p50_ms | p90_ms | p99_ms |")
$md.Add("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |")
foreach ($r in ($rows | Sort-Object par_min, count)) {
    $md.Add("| $($r.par_min) | $($r.count) | $($r.repeats) | $(Format-F64 $r.p50_tps) | $(Format-F64 $r.p90_tps) | $(Format-F64 $r.p99_tps) | $(Format-F64 $r.p50_ms) | $(Format-F64 $r.p90_ms) | $(Format-F64 $r.p99_ms) |")
}
$md.Add("")
$md.Add("## Best By Count (max p50_tps)")
$md.Add("")
$md.Add("| count | best_par_min | best_p50_tps | best_p90_tps | best_p99_tps |")
$md.Add("| ---: | ---: | ---: | ---: | ---: |")
foreach ($b in ($bestByCount | Sort-Object count)) {
    $md.Add("| $($b.count) | $($b.best_par_min) | $(Format-F64 $b.best_p50_tps) | $(Format-F64 $b.best_p90_tps) | $(Format-F64 $b.best_p99_tps) |")
}
$md.Add("")
$md.Add("## Artifacts")
$md.Add("")
$md.Add("- summary_json: $summaryJson")
$md.Add("- cases_dir: $casesDir")

$summaryMd = Join-Path $OutputDir "governance-mldsa-verify-batch-perf-summary.md"
$md | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "[mldsa-batch-perf] done"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"
Write-Host "  best_by_count:"
foreach ($b in ($bestByCount | Sort-Object count)) {
    Write-Host ("    n={0} -> par_min={1}, p50={2}, p90={3}, p99={4}" -f $b.count, $b.best_par_min, (Format-F64 $b.best_p50_tps), (Format-F64 $b.best_p90_tps), (Format-F64 $b.best_p99_tps))
}
