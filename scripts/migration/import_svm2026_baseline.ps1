param(
    [string]$RepoRoot = "",
    [string]$SvmRoot = "",
    [string]$OutputDir = "",
    [ValidateSet("core", "persist", "wasm")]
    [string]$Variant = "core",
    [string]$SourceFile = "",
    [double]$CpuParityTps = 0.0,
    [double]$CpuBatchStressTps = 0.0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\baseline"
}
if (-not $SvmRoot) {
    $sibling = Join-Path (Split-Path $RepoRoot -Parent) "SVM2026"
    if (Test-Path $sibling) {
        $SvmRoot = $sibling
    } else {
        throw "SvmRoot not found. Pass -SvmRoot explicitly or place sibling SVM2026 repo."
    }
}

function Parse-TpsValue {
    param([string]$Text)
    if (-not $Text) {
        return $null
    }
    $v = 0.0
    if ([double]::TryParse($Text, [ref]$v)) {
        return [Math]::Round($v, 2)
    }
    return $null
}

function Parse-BaselineBenchText {
    param([string]$Text)

    $parity = $null
    $stress = $null

    $baselineMatch = [regex]::Match(
        $Text,
        "(?ms)Baseline\s*\([^)]+\)\s*:\s*.*?TPS\s*=\s*(?<tps>[0-9]+(?:\.[0-9]+)?)"
    )
    if ($baselineMatch.Success) {
        $parity = Parse-TpsValue -Text $baselineMatch.Groups["tps"].Value
    }

    $shardedMatch = [regex]::Match(
        $Text,
        "(?ms)Sharded\s*\([^)]+\)\s*:\s*.*?TPS\s*=\s*(?<tps>[0-9]+(?:\.[0-9]+)?)"
    )
    if ($shardedMatch.Success) {
        $stress = Parse-TpsValue -Text $shardedMatch.Groups["tps"].Value
    }

    return [ordered]@{
        cpu_parity = $parity
        cpu_batch_stress = $stress
    }
}

function Parse-TpsFromBenchFile {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        return $null
    }
    $text = Get-Content -Path $Path -Raw
    $m = [regex]::Match($text, "\[\s*TPS\s*\]\s*:\s*(?<tps>[0-9]+(?:\.[0-9]+)?)")
    if (-not $m.Success) {
        return $null
    }
    return Parse-TpsValue -Text $m.Groups["tps"].Value
}

function Read-TpsFromSourceFile {
    param([string]$Path)

    if (-not (Test-Path $Path)) {
        throw "source file not found: $Path"
    }

    $ext = [System.IO.Path]::GetExtension($Path).ToLowerInvariant()
    if ($ext -eq ".json") {
        $raw = Get-Content -Path $Path -Raw | ConvertFrom-Json
        if ($raw.items) {
            $parity = $null
            $stress = $null
            foreach ($item in $raw.items) {
                if ($item.preset -eq "cpu_parity" -and $item.tps) {
                    $parity = Parse-TpsValue -Text "$($item.tps)"
                }
                if ($item.preset -eq "cpu_batch_stress" -and $item.tps) {
                    $stress = Parse-TpsValue -Text "$($item.tps)"
                }
            }
            return [ordered]@{
                cpu_parity = $parity
                cpu_batch_stress = $stress
            }
        }
    }

    $text = Get-Content -Path $Path -Raw
    return Parse-BaselineBenchText -Text $text
}

function Resolve-LatestFile {
    param([string]$GlobPattern)
    $items = Get-ChildItem -Path $GlobPattern -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending
    if ($items -and $items.Count -gt 0) {
        return $items[0].FullName
    }
    return $null
}

$sourcesUsed = @()
$parity = if ($CpuParityTps -gt 0.0) { [Math]::Round($CpuParityTps, 2) } else { $null }
$stress = if ($CpuBatchStressTps -gt 0.0) { [Math]::Round($CpuBatchStressTps, 2) } else { $null }

if ($SourceFile) {
    $parsed = Read-TpsFromSourceFile -Path $SourceFile
    if (-not $parity -and $parsed.cpu_parity) {
        $parity = $parsed.cpu_parity
    }
    if (-not $stress -and $parsed.cpu_batch_stress) {
        $stress = $parsed.cpu_batch_stress
    }
    $sourcesUsed += $SourceFile
}

if ((-not $parity) -or (-not $stress)) {
    $baselineBench = Join-Path $SvmRoot "baseline_bench.txt"
    if (Test-Path $baselineBench) {
        $parsed = Read-TpsFromSourceFile -Path $baselineBench
        if (-not $parity -and $parsed.cpu_parity) {
            $parity = $parsed.cpu_parity
        }
        if (-not $stress -and $parsed.cpu_batch_stress) {
            $stress = $parsed.cpu_batch_stress
        }
        $sourcesUsed += $baselineBench
    }
}

if (-not $parity) {
    $baselineBench8t = Resolve-LatestFile -GlobPattern (Join-Path $SvmRoot "bench_baseline_*.txt")
    if ($baselineBench8t) {
        $p = Parse-TpsFromBenchFile -Path $baselineBench8t
        if ($p) {
            $parity = $p
            $sourcesUsed += $baselineBench8t
        }
    }
}

if (-not $stress) {
    $cpuBench8t = Resolve-LatestFile -GlobPattern (Join-Path $SvmRoot "bench_cpu_*.txt")
    if ($cpuBench8t) {
        $s = Parse-TpsFromBenchFile -Path $cpuBench8t
        if ($s) {
            $stress = $s
            $sourcesUsed += $cpuBench8t
        }
    }
}

if (-not $stress) {
    $hintsBench8t = Resolve-LatestFile -GlobPattern (Join-Path $SvmRoot "bench_hints_*.txt")
    if ($hintsBench8t) {
        $s = Parse-TpsFromBenchFile -Path $hintsBench8t
        if ($s) {
            $stress = $s
            $sourcesUsed += $hintsBench8t
        }
    }
}

if (-not $parity -or -not $stress) {
    throw "cannot resolve complete baseline TPS from SVM2026 sources; parity=$parity stress=$stress. You can pass -CpuParityTps and -CpuBatchStressTps explicitly."
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$result = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    source = [ordered]@{
        svm_root = $SvmRoot
        files = @($sourcesUsed | Select-Object -Unique)
    }
    items = @(
        [ordered]@{
            variant = $Variant
            preset = "cpu_parity"
            tps = $parity
            source = "svm2026_baseline_import"
        },
        [ordered]@{
            variant = $Variant
            preset = "cpu_batch_stress"
            tps = $stress
            source = "svm2026_baseline_import"
        }
    )
}

$jsonPath = Join-Path $OutputDir "svm2026-baseline-$Variant.json"
$mdPath = Join-Path $OutputDir "svm2026-baseline-$Variant.md"

$result | ConvertTo-Json -Depth 8 | Set-Content -Path $jsonPath -Encoding UTF8

$md = @(
    "# SVM2026 Baseline Import"
    ""
    "- generated_at_utc: $($result.generated_at_utc)"
    "- variant: $Variant"
    "- cpu_parity_tps: $parity"
    "- cpu_batch_stress_tps: $stress"
    ""
    "## Source Files"
    ""
)

foreach ($f in ($result.source.files | Select-Object -Unique)) {
    $md += "- $f"
}

$md += ""
$md += "## Baseline Items"
$md += ""
$md += "| variant | preset | tps | source |"
$md += "|---|---|---:|---|"
foreach ($item in $result.items) {
    $md += "| $($item.variant) | $($item.preset) | $($item.tps) | $($item.source) |"
}

$md -join "`n" | Set-Content -Path $mdPath -Encoding UTF8

Write-Host "svm2026 baseline generated:"
Write-Host "  $jsonPath"
Write-Host "  $mdPath"
