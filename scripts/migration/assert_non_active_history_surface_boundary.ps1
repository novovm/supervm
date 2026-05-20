param(
    [string]$RepoRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RootPath {
    param([string]$Root)
    if (-not $Root) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
    }
    return (Resolve-Path $Root).Path
}

function Assert-Contains {
    param(
        [string]$Path,
        [string]$Needle,
        [string]$Label
    )
    if (-not (Test-Path $Path)) {
        throw "history surface boundary violation: missing ${Label}: $Path"
    }
    $text = Get-Content -Raw -Path $Path
    if (-not $text.Contains($Needle)) {
        throw "history surface boundary violation: missing ${Label}: $Needle"
    }
}

function Assert-DoesNotContain {
    param(
        [string]$Path,
        [string]$Needle,
        [string]$Label
    )
    if (-not (Test-Path $Path)) {
        return
    }
    $text = Get-Content -Raw -Path $Path
    if ($text.Contains($Needle)) {
        throw "history surface boundary violation: unexpected ${Label}: $Needle in $Path"
    }
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
Push-Location $RepoRoot
try {
    $strategyArchiveReadme = Join-Path "docs_CN" (Join-Path ("M" + "EV") "README.md")
    Assert-Contains -Path $strategyArchiveReadme -Needle "ARCHIVED / NON-ACTIVE" -Label "strategy archive marker"
    Assert-Contains -Path $strategyArchiveReadme -Needle "not NOVOVM EVM acceptance criteria" -Label "strategy archive acceptance boundary"
    Assert-Contains -Path $strategyArchiveReadme -Needle "not an active gateway requirement" -Label "strategy archive gateway boundary"

    $svmReferenceReadme = "docs_CN/WEB30-PROTOCOL/SVM2026-REFERENCE/README.md"
    Assert-Contains -Path $svmReferenceReadme -Needle "Historical source snapshot / archived reference" -Label "SVM2026 archive marker"
    Assert-Contains -Path $svmReferenceReadme -Needle "not current NOVOVM external product naming" -Label "SVM2026 naming boundary"

    $migrationIndex = "docs_CN/WEB30-PROTOCOL/WEB30-PROTOCOL-MIGRATION-INDEX-2026-03-05.md"
    Assert-Contains -Path $migrationIndex -Needle "Historical migration note" -Label "WEB30 migration archive marker"
    Assert-Contains -Path $migrationIndex -Needle "repository/path migration target" -Label "WEB30 migration naming boundary"

    $daemon = "scripts/novovm-prod-daemon.ps1"
    Assert-Contains -Path $daemon -Needle "NOVOVM single-mainline policy" -Label "disabled daemon NOVOVM policy"
    Assert-Contains -Path $daemon -Needle "internal historical code name" -Label "disabled daemon historical naming boundary"

    $englishLayerMap = "docs/NOVOVM-NETWORK/NOVOVM-CORE-PLUGIN-EXTERNAL-LAYER-MAP-2026-04-17.md"
    Assert-DoesNotContain -Path $englishLayerMap -Needle "NOVOVM / SUPERVM (Host)" -Label "external dual-brand host wording"
    Assert-Contains -Path $englishLayerMap -Needle "SUPERVM may remain only as repo/path/internal historical code name" -Label "English layer map historical naming boundary"
    Assert-Contains -Path $englishLayerMap -Needle "plugin capability, not the host" -Label "English EVM plugin boundary"

    $chineseLayerMap = "docs_CN/NOVOVM-NETWORK/NOVOVM-CORE-PLUGIN-EXTERNAL-LAYER-MAP-2026-04-17.md"
    Assert-DoesNotContain -Path $chineseLayerMap -Needle "NOVOVM / SUPERVM (Host)" -Label "Chinese external dual-brand host wording"
    Assert-Contains -Path $chineseLayerMap -Needle "repository/path/internal historical code name only" -Label "Chinese layer map historical naming boundary"
    Assert-Contains -Path $chineseLayerMap -Needle "plugin capability, not host identity" -Label "Chinese EVM plugin boundary"

    $gethCompat = "artifacts/migration/geth-upstream-compat-after-a484a8506.md"
    Assert-Contains -Path $gethCompat -Needle "Not Claimed" -Label "geth compatibility not-claimed section"
    Assert-Contains -Path $gethCompat -Needle "External observation-window result" -Label "external observation not-claimed boundary"

    $rlpxSession = "artifacts/migration/rlpx-session-canary-after-a484a8506.md"
    Assert-Contains -Path $rlpxSession -Needle "not treated as eth session readiness" -Label "public RLPx readiness boundary"

    $localGeth = "artifacts/migration/rlpx-local-geth-canary-after-a484a8506.md"
    Assert-Contains -Path $localGeth -Needle "The prior public finding remains classified as public peer selection" -Label "local-only RLPx readiness boundary"

    $eth71Design = "artifacts/migration/eth71-bal-design-after-a484a8506.md"
    Assert-Contains -Path $eth71Design -Needle "Status: design complete, implementation not started." -Label "eth71 design-only boundary"
    Assert-Contains -Path $eth71Design -Needle "is not advertised" -Label "eth71 no-advertise boundary"

    Write-Host "non-active history surface boundary guard passed"
} finally {
    Pop-Location
}
