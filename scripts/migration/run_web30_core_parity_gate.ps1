param(
    [string]$RepoRoot = "",
    [string]$Svm2026Root = "D:\WEB3_AI\SVM2026",
    [string]$OutputDir = "",
    [string[]]$AllowedDriftFiles = @("dividend_pool.rs")
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\web30-core-parity-gate"
}

$srcDir = Join-Path $Svm2026Root "contracts\web30\core\src"
$dstDir = Join-Path $RepoRoot "vendor\web30-core\src"

if (-not (Test-Path $srcDir)) {
    throw "missing source dir: $srcDir"
}
if (-not (Test-Path $dstDir)) {
    throw "missing target dir: $dstDir"
}

New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null

function File-HashMap {
    param([string]$Dir)
    $map = @{}
    Get-ChildItem -Path $Dir -Filter *.rs -File | ForEach-Object {
        $map[$_.Name] = (Get-FileHash $_.FullName -Algorithm SHA256).Hash
    }
    return $map
}

$srcMap = File-HashMap -Dir $srcDir
$dstMap = File-HashMap -Dir $dstDir

$allNames = @($srcMap.Keys + $dstMap.Keys | Sort-Object -Unique)
$rows = @()
$missingInSource = @()
$missingInTarget = @()
$mismatch = @()
$mismatchNonAllowed = @()
$mismatchAllowed = @()
$allowedSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
foreach ($f in $AllowedDriftFiles) {
    if ($f) { [void]$allowedSet.Add($f) }
}

foreach ($name in $allNames) {
    $hasSrc = $srcMap.ContainsKey($name)
    $hasDst = $dstMap.ContainsKey($name)
    if (-not $hasSrc) {
        $missingInSource += $name
    }
    if (-not $hasDst) {
        $missingInTarget += $name
    }
    if ($hasSrc -and $hasDst) {
        $same = ($srcMap[$name] -eq $dstMap[$name])
        if (-not $same) {
            $mismatch += $name
            if ($allowedSet.Contains($name)) {
                $mismatchAllowed += $name
            } else {
                $mismatchNonAllowed += $name
            }
        }
        $rows += [ordered]@{
            file = $name
            same = $same
            source_hash = $srcMap[$name]
            target_hash = $dstMap[$name]
            allowed_drift = $allowedSet.Contains($name)
        }
    } else {
        $rows += [ordered]@{
            file = $name
            same = $false
            source_hash = if ($hasSrc) { $srcMap[$name] } else { "" }
            target_hash = if ($hasDst) { $dstMap[$name] } else { "" }
            allowed_drift = $false
        }
    }
}

$pass = [bool](
    $missingInSource.Count -eq 0 -and
    $missingInTarget.Count -eq 0 -and
    $mismatchNonAllowed.Count -eq 0
)

$errorReason = ""
if ($missingInSource.Count -gt 0) {
    $errorReason = "missing_in_source"
} elseif ($missingInTarget.Count -gt 0) {
    $errorReason = "missing_in_target"
} elseif ($mismatchNonAllowed.Count -gt 0) {
    $errorReason = "hash_mismatch_non_allowed"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    error_reason = $errorReason
    source_dir = $srcDir
    target_dir = $dstDir
    source_file_count = $srcMap.Count
    target_file_count = $dstMap.Count
    compared_file_count = $rows.Count
    exact_match_count = (@($rows | Where-Object { $_.same }).Count)
    mismatch_count = $mismatch.Count
    mismatch_non_allowed_count = $mismatchNonAllowed.Count
    mismatch_allowed_count = $mismatchAllowed.Count
    allowed_drift_files = $AllowedDriftFiles
    missing_in_source = $missingInSource
    missing_in_target = $missingInTarget
    mismatch_non_allowed = $mismatchNonAllowed
    mismatch_allowed = $mismatchAllowed
    file_rows = $rows
}

$summaryJson = Join-Path $OutputDir "web30-core-parity-gate-summary.json"
$summaryMd = Join-Path $OutputDir "web30-core-parity-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# WEB30 Core Parity Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- error_reason: $($summary.error_reason)"
    "- source_dir: $($summary.source_dir)"
    "- target_dir: $($summary.target_dir)"
    "- source_file_count: $($summary.source_file_count)"
    "- target_file_count: $($summary.target_file_count)"
    "- compared_file_count: $($summary.compared_file_count)"
    "- exact_match_count: $($summary.exact_match_count)"
    "- mismatch_count: $($summary.mismatch_count)"
    "- mismatch_non_allowed_count: $($summary.mismatch_non_allowed_count)"
    "- mismatch_allowed_count: $($summary.mismatch_allowed_count)"
    "- allowed_drift_files: $([string]::Join(', ', $AllowedDriftFiles))"
    "- summary_json: $summaryJson"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "web30-core parity gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  exact_match_count: $($summary.exact_match_count)"
Write-Host "  mismatch_count: $($summary.mismatch_count)"
Write-Host "  mismatch_non_allowed_count: $($summary.mismatch_non_allowed_count)"
Write-Host "  mismatch_allowed_count: $($summary.mismatch_allowed_count)"
Write-Host "  summary_json: $summaryJson"

if (-not $pass) {
    throw "web30-core parity gate FAILED: $errorReason"
}

Write-Host "web30-core parity gate PASS"
