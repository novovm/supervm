param(
    [string]$RepoRoot = "",
    [string]$OutputPath = "",
    [string]$ManifestPath = "",
    [string]$Sha256Path = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

if (-not $OutputPath) {
    $OutputPath = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\third-party-audit-handoff-pack-2026-03-13-1342.tar.gz"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath = Join-Path $RepoRoot $OutputPath
}
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
$outputDir = Split-Path -Parent $OutputPath
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

if (-not $ManifestPath) {
    $ManifestPath = [System.IO.Path]::ChangeExtension($OutputPath, ".manifest.json")
} elseif (-not [System.IO.Path]::IsPathRooted($ManifestPath)) {
    $ManifestPath = Join-Path $RepoRoot $ManifestPath
}
$ManifestPath = [System.IO.Path]::GetFullPath($ManifestPath)

if (-not $Sha256Path) {
    $Sha256Path = $OutputPath + ".sha256.txt"
} elseif (-not [System.IO.Path]::IsPathRooted($Sha256Path)) {
    $Sha256Path = Join-Path $RepoRoot $Sha256Path
}
$Sha256Path = [System.IO.Path]::GetFullPath($Sha256Path)

$files = @(
    "docs_CN/SVM2026-MIGRATION/NOVOVM-THIRD-PARTY-AUDIT-HANDOFF-PACK-2026-03-13.md"
    "docs_CN/SVM2026-MIGRATION/NOVOVM-VULNERABILITY-RESPONSE-POLICY-2026-03-13.md"
    "docs_CN/SVM2026-MIGRATION/NOVOVM-GA-CLOSURE-REPORT-DRAFT-2026-03-13.md"
    "docs_CN/SVM2026-MIGRATION/NOVOVM-OPEN-BUSINESS-SURFACE-CLOSURE-CHECKLIST-2026-03-13.md"
    "docs_CN/SVM2026-MIGRATION/NOVOVM-THIRD-PARTY-AUDIT-INTAKE-REGISTER-2026-03-13.md"
    "docs_CN/SVM2026-MIGRATION/NOVOVM-THIRD-PARTY-AUDIT-REQUEST-TEMPLATE-2026-03-13.md"
    "docs_CN/SVM2026-MIGRATION/NOVOVM-THIRD-PARTY-AUDIT-REQUEST-DRAFT-2026-03-13.md"
    "docs_CN/SVM2026-MIGRATION/NOVOVM-THIRD-PARTY-AUDIT-REPORT-TEMPLATE-2026-03-13.md"
    "artifacts/migration/week1-2026-03-13/release-candidate-novovm-rc-2026-03-13-ga-v1-econops-1334/rc-candidate.json"
    "artifacts/migration/week1-2026-03-13/release-candidate-novovm-rc-2026-03-13-ga-v1-econops-1334/snapshot/release-snapshot.json"
    "artifacts/migration/week1-2026-03-13/release-candidate-novovm-rc-2026-03-13-ga-v1-econops-1334/snapshot/acceptance-gate-full/acceptance-gate-summary.json"
    "artifacts/migration/week1-2026-03-13/security-scan/cargo-audit.json"
    "artifacts/migration/week1-2026-03-13/security-scan/cargo-deny-advisories.json"
    "artifacts/migration/week1-2026-03-13/security-scan/cargo-deny-policy.json"
    "artifacts/migration/week1-2026-03-13/week4-blocker-status/week4-blocker-status.json"
    "artifacts/migration/week1-2026-03-13/week4-blocker-status/week4-blocker-status.md"
)

$missing = @()
foreach ($rel in $files) {
    $abs = Join-Path $RepoRoot $rel
    if (-not (Test-Path $abs)) {
        $missing += $rel
    }
}
if ($missing.Count -gt 0) {
    throw "missing required files for handoff pack: $($missing -join ', ')"
}

$tempList = Join-Path $outputDir ("handoff-pack-files-" + [guid]::NewGuid().ToString("N") + ".txt")
$files | Set-Content -Path $tempList -Encoding UTF8

try {
    if (Test-Path $OutputPath) {
        Remove-Item -Path $OutputPath -Force
    }
    & tar -czf $OutputPath -C $RepoRoot -T $tempList
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $OutputPath)) {
        throw "failed to build handoff pack"
    }
} finally {
    Remove-Item -Path $tempList -ErrorAction SilentlyContinue
}

$sha = (Get-FileHash -Path $OutputPath -Algorithm SHA256).Hash.ToLowerInvariant()
"$sha  $OutputPath" | Set-Content -Path $Sha256Path -Encoding UTF8

$manifest = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    repo_root = $RepoRoot
    output_path = $OutputPath
    sha256 = $sha
    sha256_file = $Sha256Path
    file_count = $files.Count
    files = $files
}
$manifest | ConvertTo-Json -Depth 8 | Set-Content -Path $ManifestPath -Encoding UTF8

Write-Host "third-party audit handoff pack built:"
Write-Host "  output_path: $OutputPath"
Write-Host "  sha256: $sha"
Write-Host "  sha256_file: $Sha256Path"
Write-Host "  manifest: $ManifestPath"
Write-Host "  file_count: $($files.Count)"
