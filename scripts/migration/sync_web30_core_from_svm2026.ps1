param(
    [string]$RepoRoot = "",
    [string]$Svm2026Root = "D:\WEB3_AI\SVM2026",
    [string[]]$PreserveFiles = @("dividend_pool.rs"),
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

$srcDir = Join-Path $Svm2026Root "contracts\web30\core\src"
$dstDir = Join-Path $RepoRoot "vendor\web30-core\src"
$outDir = Join-Path $RepoRoot "artifacts\migration\web30-core-sync"
New-Item -ItemType Directory -Path $outDir -Force | Out-Null

if (-not (Test-Path $srcDir)) {
    throw "missing source dir: $srcDir"
}
if (-not (Test-Path $dstDir)) {
    throw "missing target dir: $dstDir"
}

$preserveSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
foreach ($f in $PreserveFiles) {
    if ($f) { [void]$preserveSet.Add($f) }
}

$copied = @()
$skipped = @()
$missingInTarget = @()

$files = Get-ChildItem -Path $srcDir -Filter *.rs -File | Sort-Object Name
foreach ($file in $files) {
    $name = $file.Name
    $src = $file.FullName
    $dst = Join-Path $dstDir $name
    if (-not (Test-Path $dst)) {
        $missingInTarget += $name
        continue
    }

    if ($preserveSet.Contains($name)) {
        $skipped += $name
        continue
    }

    if (-not $DryRun) {
        Copy-Item -Path $src -Destination $dst -Force
    }
    $copied += $name
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    dry_run = [bool]$DryRun
    source_dir = $srcDir
    target_dir = $dstDir
    source_file_count = $files.Count
    copied_count = $copied.Count
    skipped_count = $skipped.Count
    missing_in_target_count = $missingInTarget.Count
    preserve_files = $PreserveFiles
    copied_files = $copied
    skipped_files = $skipped
    missing_in_target = $missingInTarget
}

$summaryJson = Join-Path $outDir "web30-core-sync-summary.json"
$summaryMd = Join-Path $outDir "web30-core-sync-summary.md"
$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# WEB30 Core Sync Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- dry_run: $($summary.dry_run)"
    "- source_dir: $($summary.source_dir)"
    "- target_dir: $($summary.target_dir)"
    "- source_file_count: $($summary.source_file_count)"
    "- copied_count: $($summary.copied_count)"
    "- skipped_count: $($summary.skipped_count)"
    "- missing_in_target_count: $($summary.missing_in_target_count)"
    "- preserve_files: $([string]::Join(', ', $PreserveFiles))"
    "- summary_json: $summaryJson"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "web30-core sync summary:"
Write-Host "  dry_run: $($summary.dry_run)"
Write-Host "  copied_count: $($summary.copied_count)"
Write-Host "  skipped_count: $($summary.skipped_count)"
Write-Host "  missing_in_target_count: $($summary.missing_in_target_count)"
Write-Host "  summary_json: $summaryJson"
